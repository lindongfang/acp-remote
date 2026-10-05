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
import type { CommandMessage, Cursor, Uuid } from "../protocol";
import {
  FakeClock,
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
  makeEvent,
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
  readonly clock: FakeClock;
}

/** 建一个未连接的门面替身环境；`resumeCursor` 模拟「上次持久化的确认游标」。 */
function buildClient(input: { readonly resumeCursor?: Cursor | null } = {}): ClientHarness {
  const sockets = new FakeSocketFactory();
  const events: SyncClientEvent[] = [];
  const clock = new FakeClock();
  const client = new SyncClient({
    hostId: HOST_ID,
    url: "wss://host.example:8443/sync",
    identity: new FakeIdentity(),
    host: new FakeHostIdentity(),
    random: new FakeRandom(),
    transcript: new FakeTranscript(),
    digest: fakeDigest(),
    openSocket: sockets.open,
    resumeCursor: input.resumeCursor ?? null,
    clock,
    onEvent: (event) => {
      events.push(event);
    },
    nowMs: () => 1_700_000_000_000,
  });
  return { client, sockets, events, clock };
}

/** `auth.authenticated` 的默认 serverEpoch。 */
const SERVER_EPOCH = "00384a03-bc90-4095-b65d-82fb8cc47e13";
/** 服务端事件库重建后的新 epoch（§9.4）。 */
const NEXT_SERVER_EPOCH = "11111111-2222-4333-8444-555555555555";

/** 对**当前最近建立**的连接跑完握手（重连用例用它驱动第二条连接）。 */
async function authenticate(context: ClientHarness): Promise<void> {
  context.client.connect();
  const socket = context.sockets.latest;
  socket.open();
  await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
  socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
  await waitUntil(() => socket.types().includes("auth.client_proof"), "发出 clientProof");
  socket.deliver(makeAuthenticated({ connectionId: CONNECTION_ID, serverEpoch: SERVER_EPOCH }));
  await waitUntil(() => socket.types().includes("sync.subscribe"), "发出 subscribe");
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

describe("CRITICAL-1：socket 级 burst 下的多 chunk 快照", () => {
  it("三个 chunk 背靠背投递（不逐个 await）仍能完成并替换仓库", async () => {
    const context = buildClient();
    await authenticate(context);
    const socket = context.sockets.latest;

    socket.deliver(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 3 }));
    const chunks = [
      makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM }),
      makeSnapshotChunk({
        snapshotId: SNAPSHOT_ID,
        chunkIndex: "1",
        resource: "workspaces",
        items: [{ alias: "api", displayName: "API" }],
      }),
      makeSnapshotChunk({
        snapshotId: SNAPSHOT_ID,
        chunkIndex: "2",
        resource: "agents",
        items: [{ agentId: "claude", displayName: "Claude", default: true }],
      }),
    ];
    // WSS 的真实投递形态：三条 chunk 连着下来，中间**没有**任何 await 把它们拆开。
    for (const chunk of chunks) socket.deliver(chunk.rawText);
    socket.deliver(
      makeSnapshotEnd({
        snapshotId: SNAPSHOT_ID,
        chunkCount: 3,
        digest: expectedSnapshotDigest(chunks.map((chunk) => chunk.rawText)),
      }),
    );
    await waitUntil(() => context.client.snapshots.current !== null, "快照进入仓库");

    const current = context.client.snapshots.current;
    expect(current?.resources.sessions).toHaveLength(1);
    expect(current?.resources.workspaces).toHaveLength(1);
    expect(current?.resources.agents).toHaveLength(1);
    // 反例：若 chunk 处理在摘要的 await 处交错，chunk 1 会读到未推进的 receivedChunks
    // 被判 chunk_out_of_order 并 discard → 这里会出现 rejected 且仓库仍为 null → 断言变红。
    expect(context.events.filter((event) => event.kind === "rejected")).toEqual([]);
  });
});

describe("CRITICAL-2：重连后账本与去重不得重置", () => {
  it("连接 → 断 → 重连 → 重投已见事件判重复，且从最后确认位置续传", async () => {
    const context = buildClient();
    await authenticate(context);
    const first = context.sockets.latest;

    const seq1 = makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000001" });
    const seq2 = makeEvent({ globalSequence: "2", eventId: "aaaaaaaa-0000-4000-8000-000000000002" });
    first.deliver(seq1);
    first.deliver(seq2);
    await waitUntil(() => context.events.filter((event) => event.kind === "event").length === 2, "处理两条事件");

    // 服务端异常断开（非阻断），状态层随后调用 connect()。
    first.serverClose(1006, "abnormal");
    await authenticate(context);
    const second = context.sockets.latest;
    expect(second).not.toBe(first);

    // 重连必须从最后确认位置（seq 2）续传，而不是从无游标开始。
    const subscribe = second.last("sync.subscribe");
    expect((subscribe?.["body"] as { cursor: unknown }).cursor).toEqual({
      serverEpoch: SERVER_EPOCH,
      globalSequence: "2",
    });

    // 服务端在切换窗口里重投第一条：必须判重复，不得二次呈现。
    second.deliver(seq1);
    await waitUntil(
      () => context.events.filter((event) => event.kind === "duplicate_dropped").length === 1,
      "重投判重复",
    );
    expect(context.events.filter((event) => event.kind === "event")).toHaveLength(2);
  });

  it("重连后首个事件序号远大于 1 时不得判成缺口（续传起点不是 1）", async () => {
    // 上次持久化的确认游标在 2317：本次连接处理 2318，断线后服务端从 2319 续发。
    const context = buildClient({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "2317" } });
    await authenticate(context);
    context.sockets.latest.deliver(
      makeEvent({ globalSequence: "2318", eventId: "aaaaaaaa-0000-4000-8000-000000002318" }),
    );
    await waitUntil(() => context.events.some((event) => event.kind === "event"), "处理高序号事件");

    context.sockets.latest.serverClose(1006, "abnormal");
    await authenticate(context);

    // 服务端从最后确认位置续发 2319。
    context.sockets.latest.deliver(
      makeEvent({ globalSequence: "2319", eventId: "aaaaaaaa-0000-4000-8000-000000002319" }),
    );
    await waitUntil(() => context.events.filter((event) => event.kind === "event").length === 2, "续传事件被处理");

    // 反例：若账本在认证时被重建（resumeFrom=null），2319 会要求首条恰为 1 → 判 gap → 断言变红。
    expect(context.events.some((event) => event.kind === "gap_detected")).toBe(false);
  });
});

describe("NEW-2：epoch 变化后订阅游标与 ACK 必须重新起步", () => {
  it("服务端 epoch 变化后首个 sync.subscribe 的 cursor 为 null，且新 epoch 的事件能被 ACK", async () => {
    const context = buildClient({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "2317" } });
    await authenticate(context);
    context.sockets.latest.deliver(
      makeEvent({ globalSequence: "2318", eventId: "aaaaaaaa-0000-4000-8000-000000002318" }),
    );
    await waitUntil(() => context.events.some((event) => event.kind === "event"), "处理高序号事件");
    expect(context.client.ackedCursor).toEqual({ serverEpoch: SERVER_EPOCH, globalSequence: "2318" });

    // 服务端事件库重建（§9.4 列为必须重建快照的**正常**场景），epoch 随之变化。
    context.sockets.latest.serverClose(1006, "abnormal");
    context.client.connect();
    const socket = context.sockets.latest;
    socket.open();
    await waitUntil(() => socket.types().includes("auth.client_hello"), "重连发出 clientHello");
    socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
    await waitUntil(() => socket.types().includes("auth.client_proof"), "重连发出 clientProof");
    socket.deliver(makeAuthenticated({ connectionId: CONNECTION_ID, serverEpoch: NEXT_SERVER_EPOCH }));
    await waitUntil(() => socket.types().includes("sync.subscribe"), "重连发出 subscribe");

    // (a) 旧 epoch 的游标绝不能发出去：服务端对它只回 `sync.cursor_invalid`（§9.4/:660），
    //     客户端只上报不恢复，事件流就此停摆。
    // 反例：若 epoch 变化后仍带旧游标，这里会拿到 {serverEpoch: 旧, globalSequence: "2318"} → 变红。
    const subscribe = socket.last("sync.subscribe");
    expect((subscribe?.["body"] as { cursor: unknown }).cursor).toBeNull();

    // (b) 跨 epoch 后 ACK 必须恢复：#lastAcked 若仍持旧游标，`compareCursors` 恒判回退，
    //     于是每条事件都产出 ack_not_advancing，一条 sync.ack 也发不出去（违反 §9.5）。
    socket.deliver(makeEvent({ globalSequence: "1", eventId: "bbbbbbbb-0000-4000-8000-000000000001" }));
    socket.deliver(makeEvent({ globalSequence: "2", eventId: "bbbbbbbb-0000-4000-8000-000000000002" }));
    socket.deliver(makeEvent({ globalSequence: "3", eventId: "bbbbbbbb-0000-4000-8000-000000000003" }));

    const acks = socket.sent.filter((message) => message.parsed["type"] === "sync.ack");
    // 反例：若 #lastAcked 未清空，这里是 0 条 ACK、3 条 ack_not_advancing → 变红。
    expect(acks).toHaveLength(3);
    expect(context.events.some((event) => event.kind === "rejected" && event.reason === "ack_not_advancing")).toBe(false);
    expect(context.client.ackedCursor).toEqual({ serverEpoch: NEXT_SERVER_EPOCH, globalSequence: "3" });
  });
});

describe("CRITICAL-3：客户端方向出站序号", () => {
  it("认证后的出站消息序号从 1 开始严格递增，且不抄服务端计数器", async () => {
    const context = buildClient();
    context.client.connect();
    const socket = context.sockets.latest;
    socket.open();
    await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
    socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
    await waitUntil(() => socket.types().includes("auth.client_proof"), "发出 clientProof");
    // 服务端方向的计数器给一个显眼的值：客户端若抄它，第一条 subscribe 就会是 42。
    socket.deliver(makeAuthenticated({ connectionId: CONNECTION_ID, serverConnectionSequence: "42" }));
    await waitUntil(() => socket.types().includes("sync.subscribe"), "发出 subscribe");

    socket.deliver(makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000001" }));
    socket.deliver(makeEvent({ globalSequence: "2", eventId: "aaaaaaaa-0000-4000-8000-000000000002" }));
    context.client.dispatch(promptCommand(REQUEST_ID));
    await waitUntil(() => socket.types().includes("sync.ack"), "发出 ACK");

    const afterAuth = socket.sent
      .filter((message) => message.parsed["connectionId"] !== undefined)
      .map((message) => `${String(message.parsed["type"])}:${String(message.parsed["connectionSequence"])}`);
    // 反例：若 subscribe 抄服务端序号（42）→ 第一项变 subscribe:42 → 断言变红。
    expect(afterAuth[0]).toBe("sync.subscribe:1");
    // 反例：若计数器恒为 0 或不递增 → 这里会出现重复或 0 → 断言变红。
    expect(afterAuth.map((entry) => entry.split(":")[1])).toEqual(
      afterAuth.map((_entry, index) => String(index + 1)),
    );
  });

  it("认证前的两条消息不带 connectionSequence（§4.1）", async () => {
    const context = buildClient();
    context.client.connect();
    const socket = context.sockets.latest;
    socket.open();
    await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");

    for (const type of ["auth.client_hello", "auth.client_proof"]) {
      const message = socket.last(type);
      expect(message?.["connectionSequence"]).toBeUndefined();
      expect(message?.["connectionId"]).toBeUndefined();
    }
    void context;
  });
});