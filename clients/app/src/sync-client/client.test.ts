/**
 * R13：命令以稳定 `requestId` 收敛；快照仓库只在验证通过后替换。
 *
 * 判别力来源：
 * - 重试时换新 `requestId` → 「同一 ID 重派」用例变红；
 * - 把「连接断开」当作结果不确定 → 「断开不进 uncertain」用例变红（在 command-machine.test.ts）；
 * - `command.status` 查询复用目标 ID 而非新意图 → 「查询指向原 requestId」用例变红；
 * - 验证失败也替换快照 → 「失败不覆盖已完成状态」用例变红（与 snapshot.test.ts 互补：这里测门面层）。
 */

import { describe, expect, it } from "vitest";

import { SyncClient } from "./client";
import type { SyncClientEvent } from "./connection";
import type { CommandMessage, Uuid } from "../protocol";
import {
  FakeHostIdentity,
  FakeIdentity,
  FakeRandom,
  FakeSocketFactory,
  FakeTranscript,
  expectedSnapshotDigest,
  fakeDigest,
  makeAuthenticated,
  makeChallenge,
  makeCommandResult,
  makeSnapshotBegin,
  makeSnapshotChunk,
  makeSnapshotEnd,
  waitUntil,
} from "./testing/harness";

const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const SNAPSHOT_ID = "8194de43-e213-423d-acf4-2e3549304566";
const REQUEST_ID = "4c4dafda-dd98-442e-8d55-252b75bac72d";

const SESSIONS_ITEM = [
  {
    sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
    title: "修复测试",
    agent: { agentId: "claude", name: "Claude" },
    state: "idle",
    origin: { kind: "local" },
    currentMode: null,
    version: "1",
    createdAt: "2026-10-01T00:00:00.000Z",
    updatedAt: "2026-10-01T00:00:00.000Z",
  },
];

/** 门面的被测环境（命名类型，测试不通过 `ReturnType` 引用）。 */
interface ClientHarness {
  readonly client: SyncClient;
  readonly sockets: FakeSocketFactory;
  readonly events: SyncClientEvent[];
}

/** 建一个已认证的面门替身环境。 */
function buildClient(): ClientHarness {
  const sockets = new FakeSocketFactory();
  const events: SyncClientEvent[] = [];
  const client = new SyncClient({
    hostId: HOST_ID,
    url: "wss://host.example:8443/sync",
    identity: new FakeIdentity(),
    host: new FakeHostIdentity(),
    random: new FakeRandom(),
    transcript: new FakeTranscript(),
    digest: fakeDigest(),
    openSocket: sockets.open,
    resumeCursor: null,
    onEvent: (event) => {
      events.push(event);
    },
    nowMs: () => 1_700_000_000_000,
  });
  return { client, sockets, events };
}

async function authenticate(context: ClientHarness): Promise<void> {
  context.client.connect();
  context.sockets.latest.open();
  await waitUntil(() => context.sockets.latest.types().includes("auth.client_hello"), "发出 clientHello");
  context.sockets.latest.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
  await waitUntil(() => context.sockets.latest.types().includes("auth.client_proof"), "发出 clientProof");
  context.sockets.latest.deliver(makeAuthenticated({ connectionId: CONNECTION_ID }));
  await waitUntil(() => context.sockets.latest.types().includes("sync.subscribe"), "发出 subscribe");
}

/** 造一条 `session.prompt` 命令。 */
function promptCommand(requestId: Uuid): CommandMessage {
  return {
    protocolVersion: 1,
    type: "command",
    messageId: "bbbbbbbb-1111-4111-8111-000000000009",
    connectionId: CONNECTION_ID,
    connectionSequence: "0",
    body: {
      requestId,
      command: "session.prompt",
      sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
      payload: { content: [{ type: "text", text: "继续" }] },
    },
  };
}

describe("R13：稳定 requestId 与重试", () => {
  it("同一 requestId 重派时 ID 不变、派发次数递增", async () => {
    const context = buildClient();
    await authenticate(context);

    const first = context.client.dispatch(promptCommand(REQUEST_ID));
    const second = context.client.dispatch(promptCommand(REQUEST_ID));

    // 反例：若重试换新 ID，这里会是两个不同 ID → 断言变红。
    expect(first.requestId).toBe(REQUEST_ID);
    expect(second.requestId).toBe(REQUEST_ID);
    expect(second.dispatchCount).toBe(2);

    const prompts = context.sockets.latest.sent.filter((message) => {
      const body = message.parsed["body"];
      return typeof body === "object" && body !== null && (body as { command?: string }).command === "session.prompt";
    });
    expect(prompts).toHaveLength(2);
    const ids = prompts.map((message) => (message.parsed["body"] as { requestId: string }).requestId);
    expect(new Set(ids).size).toBe(1);
  });

  it("command.status 查询的 targetRequestId 是原 ID，查询自身用新 ID", async () => {
    const context = buildClient();
    await authenticate(context);
    context.client.dispatch(promptCommand(REQUEST_ID));

    const queryId = "a-new-query-request-id-0000-0000-0000000000";
    const query = context.client.buildStatusQuery({ targetRequestId: REQUEST_ID, requestId: queryId });
    const body = query.body as { command: string; requestId: string; payload: { targetRequestId: string } };

    expect(body.command).toBe("command.status");
    expect(body.requestId).toBe(queryId);
    // 反例：若查询复用同一 requestId 当作新意图 → 断言变红。
    expect(body.payload.targetRequestId).toBe(REQUEST_ID);
  });

  it("收到 command.result 后标记为已确认", async () => {
    const context = buildClient();
    await authenticate(context);
    context.client.dispatch(promptCommand(REQUEST_ID));
    expect(context.client.dispatchRecord(REQUEST_ID)?.acknowledged).toBe(false);

    context.sockets.latest.deliver(makeCommandResult({ requestId: REQUEST_ID, status: "accepted" }));

    expect(context.client.dispatchRecord(REQUEST_ID)?.acknowledged).toBe(true);
  });

  it("未建立连接时构造查询命令抛错（不得凭空造出无连接的请求）", () => {
    const context = buildClient();
    expect(() => context.client.buildStatusQuery({ targetRequestId: REQUEST_ID, requestId: "q-1" })).toThrow();
  });
});

describe("R12：阻断后拒绝自动重连", () => {
  it("收到 4411 后 blocked 被置位，connect() 抛错", async () => {
    const context = buildClient();
    await authenticate(context);
    context.sockets.latest.serverClose(4411, "connection_replaced");

    expect(context.client.blocked).toBe("replaced_by_other_connection");
    // 反例：若阻断后仍允许 connect()，两个标签页会互相顶替形成重连风暴 → 断言变红。
    expect(() => context.client.connect()).toThrow(/阻断/);
  });

  it("clearBlocking 后可再次连接", async () => {
    const context = buildClient();
    await authenticate(context);
    context.sockets.latest.serverClose(4411, "connection_replaced");
    context.client.clearBlocking();

    expect(context.client.blocked).toBeNull();
    context.client.connect();
    expect(context.sockets.sockets).toHaveLength(2);
  });
});

describe("R14：门面只在快照验证通过后替换", () => {
  it("验证通过的快照进入仓库", async () => {
    const context = buildClient();
    await authenticate(context);

    context.sockets.latest.deliver(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 1 }));
    const chunk = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM });
    context.sockets.latest.deliver(chunk.rawText);
    await waitUntil(() => context.events.length > 0, "快照 chunk 已摄入");
    context.sockets.latest.deliver(
      makeSnapshotEnd({ snapshotId: SNAPSHOT_ID, chunkCount: 1, digest: expectedSnapshotDigest([chunk.rawText]) }),
    );
    await waitUntil(() => context.client.snapshots.current !== null, "快照进入仓库");

    expect(context.client.snapshots.current?.snapshotId).toBe(SNAPSHOT_ID);
    expect(context.client.snapshots.current?.resources.sessions).toHaveLength(1);
  });

  it("验证失败的快照不进入仓库（保持 null 或旧值）", async () => {
    const context = buildClient();
    await authenticate(context);

    context.sockets.latest.deliver(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 1 }));
    const chunk = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM });
    context.sockets.latest.deliver(chunk.rawText);
    await waitUntil(() => context.events.length > 0, "快照 chunk 已摄入");
    context.sockets.latest.deliver(
      makeSnapshotEnd({ snapshotId: SNAPSHOT_ID, chunkCount: 1, digest: expectedSnapshotDigest(["不同的字节"]) }),
    );
    await waitUntil(() => context.events.some((event) => event.kind === "rejected"), "digest 校验失败");

    // 反例：若门面把未验证内容写入仓库，这里会非 null → 断言变红。
    expect(context.client.snapshots.current).toBeNull();
  });

  it("reset_required 丢弃暂存区并重新请求快照", async () => {
    const context = buildClient();
    await authenticate(context);
    context.sockets.latest.deliver(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 3 }));
    const chunk = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM });
    context.sockets.latest.deliver(chunk.rawText);
    await waitUntil(() => context.events.length > 0, "快照 chunk 已摄入");

    context.sockets.latest.deliver({
      protocolVersion: 1,
      type: "sync.reset_required",
      messageId: "cccccccc-1111-4111-8111-000000000002",
      connectionId: CONNECTION_ID,
      connectionSequence: "1",
      body: { reason: "cursor_expired", snapshotAvailable: true },
    });
    await waitUntil(() => context.sockets.latest.types().includes("sync.snapshot_request"), "重新请求快照");

    expect(context.events.some((event) => event.kind === "reset_required")).toBe(true);
    // 客户端按服务端要求重新请求快照（§9.4）。
    expect(context.sockets.latest.last("sync.snapshot_request")).toBeDefined();
    // 之前的部分快照不得再被完成。
    context.sockets.latest.deliver(
      makeSnapshotEnd({ snapshotId: SNAPSHOT_ID, chunkCount: 3, digest: expectedSnapshotDigest([chunk.rawText]) }),
    );
    await waitUntil(() => context.events.some((event) => event.kind === "rejected"), "暂存已丢弃");
    expect(context.client.snapshots.current).toBeNull();
  });

  it("reset 之后到达的 snapshot_end 不得完成替换（与未完成暂存的 chunk 完全匹配也无效）", async () => {
    const context = buildClient();
    await authenticate(context);

    context.sockets.latest.deliver(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 1 }));
    const chunk = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM });
    context.sockets.latest.deliver(chunk.rawText);
    await waitUntil(() => context.events.length > 0, "快照 chunk 已摄入");

    // 服务端要求重建且**不**提供快照：客户端不请求，但必须丢弃暂存区。
    context.sockets.latest.deliver({
      protocolVersion: 1,
      type: "sync.reset_required",
      messageId: "cccccccc-1111-4111-8111-000000000003",
      connectionId: CONNECTION_ID,
      connectionSequence: "1",
      body: { reason: "epoch_mismatch", snapshotAvailable: false },
    });
    await waitUntil(() => context.events.some((event) => event.kind === "reset_required"), "收到 reset_required");

    // 迟到的 snapshot_end：chunk 数、cursor 与 digest 全部与刚才那份暂存**完全吻合**。
    // 唯一能挡住它的就是「reset 丢弃了暂存区」这条不变量。
    context.sockets.latest.deliver(
      makeSnapshotEnd({ snapshotId: SNAPSHOT_ID, chunkCount: 1, digest: expectedSnapshotDigest([chunk.rawText]) }),
    );
    await waitUntil(() => context.events.some((event) => event.kind === "rejected"), "迟到的 snapshot_end 被拒");

    // 反例：若 reset 不丢弃暂存区，这份快照会被验证通过并替换仓库 → 断言变红。
    expect(context.client.snapshots.current).toBeNull();
  });
});