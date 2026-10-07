// 【交接副本】来源 worktree .worktrees/tp3 的 clients/app/src/sync-client/r13-dispatch-idempotency.test.ts
// 交付提交 6e8307096629dcfd3fa63ac20559e25b0a3e7a7a（分支 feat/tp3），基线 f6ea252。
// 用途：premerge 门禁按「相对变更目录」解析 test_delivery.artifacts 的可读性；本副本在权威交付物之外。
// 除顶部本 4 行说明外与来源逐字节相同（权威可运行副本在 worktree 内，由 PV3 执行）。
/**
 * R13：`SyncClient` 派发层的 `requestId` 复用（跨重试、跨 wire 层）。
 *
 * ## 为什么状态机层的断言不够
 *
 * `r13-command-idempotency.test.ts` 覆盖 `command-machine.ts` 的 `retrySameRequest`。
 * 但 R13 的 MUST 说的是「客户端 MUST NOT 为同一意图生成新的请求标识」——真正派发命令的是
 * `SyncClient.dispatch`。本文件因此在**派发层**断言：
 *
 * - 同一 `requestId` 派发 3 次 → 派发登记里**只有一条**记录、`dispatchCount` 为 3；
 * - 三次出站 wire 的 `body.requestId` 完全相同；
 * - `command.status` 查询的 `requestId` 是新的，但 `targetRequestId` 指向原 ID；
 * - 换一个 `requestId` 得到第二条独立登记（对照：上一条不是「表里只有一条」而恒真）。
 *
 * ## 判别力
 *
 * 若实现为重试生成新 ID：`dispatchRecord(原ID)` 返回 `null`、出站 `requestId` 不等 → 断言变红。
 */

import { describe, expect, it } from "vitest";

import { SyncClient } from "./client";
import type { TranscriptCodec } from "./ports";
import type { SessionPromptCommand } from "../protocol";
import {
  FakeClock,
  FakeHostIdentity,
  FakeIdentity,
  FakeRandom,
  FakeSocketFactory,
  fakeDigest,
  makeAuthenticated,
  makeChallenge,
  waitUntil,
} from "./testing/harness";

const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const DEVICE_ID = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
const SESSION_ID = "5d73cd10-a465-43cd-b3f1-704e2d49e99e";

/** 一个意图的稳定 `requestId`。 */
const PROMPT_REQUEST_ID = "5e2f7a91-6c34-4b8d-9e05-1f7a3c8b2d64";
/** 另一个意图的 `requestId`（对照：证明登记表不是「只放一条」）。 */
const OTHER_REQUEST_ID = "9b8e0d24-7f51-4c6a-b3d9-4a2e6f0c1b57";
/** `command.status` 查询自身的 ID（查询是新消息，ID 自然是新的）。 */
const QUERY_REQUEST_ID = "c4d7a913-2e68-4b5f-8a06-7d9b1e3f5c28";

/** 只做字节占位的 transcript 端口：派发断言不涉及编码，被测对象是信封与登记。 */
const TRANSCRIPT: TranscriptCodec = {
  encode: () => new Uint8Array(new ArrayBuffer(1)),
  u16be: () => new Uint8Array(2),
  uuid16: () => new Uint8Array(16),
  utf8: () => new Uint8Array(new ArrayBuffer(1)),
  nulJoinedUtf8: () => new Uint8Array(new ArrayBuffer(1)),
};

/** 被测客户端 + 它的假 socket（只替换传输，握手与信封都是生产实现）。 */
interface Harness {
  readonly client: SyncClient;
  readonly sockets: FakeSocketFactory;
}

/** 造一条 `session.prompt` 命令信封。 */
function promptCommand(requestId: string): SessionPromptCommand {
  return {
    protocolVersion: 1,
    type: "command",
    messageId: "aaaaaaaa-1111-4111-8111-000000000009",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: {
      requestId,
      command: "session.prompt",
      sessionId: SESSION_ID,
      payload: { content: [{ type: "text", text: "继续" }] },
    },
  };
}

function setup(): Harness {
  const sockets = new FakeSocketFactory();
  const client = new SyncClient({
    hostId: HOST_ID,
    url: "wss://host.example:8443/sync",
    identity: new FakeIdentity({
      deviceId: DEVICE_ID,
      canonicalOrigin: "https://host.example:8443",
      publicKeyBase64Url: "AQID",
    }),
    host: new FakeHostIdentity(),
    random: new FakeRandom(),
    transcript: TRANSCRIPT,
    digest: fakeDigest(),
    openSocket: sockets.open,
    resumeCursor: null,
    clock: new FakeClock(1_700_000_000_000),
    onEvent: () => undefined,
  });
  return { client, sockets };
}

/** 跑完真实握手与认证，让 `connectionId` 真的可用。 */
async function authenticate(harness: Harness): Promise<void> {
  harness.client.connect();
  const socket = harness.sockets.latest;
  socket.open();
  await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
  socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
  await waitUntil(() => socket.types().includes("auth.client_proof"), "发出 clientProof");
  socket.deliver(makeAuthenticated({ connectionId: CONNECTION_ID }));
  await waitUntil(() => harness.client.connectionId === CONNECTION_ID, "认证完成");
}

describe("R13：SyncClient 派发登记按 requestId 复用", () => {
  it("同一 requestId 派发 3 次：登记只有一条，dispatchCount 为 3", () => {
    const { client } = setup();

    const first = client.dispatch(promptCommand(PROMPT_REQUEST_ID));
    const second = client.dispatch(promptCommand(PROMPT_REQUEST_ID));
    const third = client.dispatch(promptCommand(PROMPT_REQUEST_ID));

    // 反例：若每次派发都登记一条互不相等的记录（换 ID 或按次新建），断言变红。
    expect(first.requestId).toBe(PROMPT_REQUEST_ID);
    expect(second.requestId).toBe(first.requestId);
    expect(second.dispatchCount).toBe(first.dispatchCount + 1);
    expect(third.requestId).toBe(first.requestId);
    expect(third.dispatchCount).toBe(3);

    const record = client.dispatchRecord(PROMPT_REQUEST_ID);
    expect(record?.requestId).toBe(PROMPT_REQUEST_ID);
    expect(record?.dispatchCount).toBe(3);
    // 反例：若「发出即算已确认」，这里会是 true → 断言变红（R13 要求服务端确认才算）。
    expect(record?.acknowledged).toBe(false);
  });

  it("换一个 requestId 得到第二条独立登记（对照：上一条不是「表里只有一条」）", () => {
    const { client } = setup();
    client.dispatch(promptCommand(PROMPT_REQUEST_ID));
    client.dispatch(promptCommand(PROMPT_REQUEST_ID));
    client.dispatch(promptCommand(OTHER_REQUEST_ID));

    expect(client.dispatchRecord(PROMPT_REQUEST_ID)?.dispatchCount).toBe(2);
    expect(client.dispatchRecord(OTHER_REQUEST_ID)?.dispatchCount).toBe(1);
    // 从未派发过的 ID 必须查不到：反例（返回一条 `dispatchCount: 0` 的占位记录）会红。
    expect(client.dispatchRecord(QUERY_REQUEST_ID)).toBeNull();
  });

  it("未连接时不产生 wire，但登记仍然建立（重连后复用的依据）", () => {
    const { client, sockets } = setup();
    client.dispatch(promptCommand(PROMPT_REQUEST_ID));
    client.dispatch(promptCommand(PROMPT_REQUEST_ID));

    expect(sockets.sockets).toHaveLength(0);
    expect(client.dispatchRecord(PROMPT_REQUEST_ID)).toEqual({
      requestId: PROMPT_REQUEST_ID,
      command: "session.prompt",
      dispatchCount: 2,
      acknowledged: false,
    });
  });
});

describe("R13：出站 wire 上的 requestId 也复用同一标识", () => {
  it("已认证后连派三次，三条命令的 requestId 集合大小为 1", async () => {
    const harness = setup();
    await authenticate(harness);
    harness.client.dispatch(promptCommand(PROMPT_REQUEST_ID));
    harness.client.dispatch(promptCommand(PROMPT_REQUEST_ID));
    harness.client.dispatch(promptCommand(PROMPT_REQUEST_ID));

    const wire = harness.sockets.latest.sent;
    const sent = wire.map((message) => message.parsed).filter((message) => message["type"] === "command");
    expect(sent).toHaveLength(3);
    const ids = new Set(sent.map((message) => (message["body"] as Record<string, unknown>)["requestId"]));
    // 反例：若重试生成新 ID，`ids` 会有 3 个成员 → 断言变红。
    expect([...ids]).toEqual([PROMPT_REQUEST_ID]);
    expect(harness.client.dispatchRecord(PROMPT_REQUEST_ID)?.dispatchCount).toBe(3);
  });

  it("两次派发的派发次数递增而 requestId 相同（消息身份与命令身份是两个概念）", async () => {
    const harness = setup();
    await authenticate(harness);

    const first = harness.client.dispatch(promptCommand(PROMPT_REQUEST_ID));
    const second = harness.client.dispatch(promptCommand(PROMPT_REQUEST_ID));

    expect(first.dispatchCount).toBe(1);
    expect(second.dispatchCount).toBe(2);
    expect(second.requestId).toBe(first.requestId);
    // 本用例的 messageId 由构造器固定，因此只断言 requestId 相等已足够；
    // messageId 相同不构成缺陷（幂等键是 requestId，不是 messageId）。
    expect(first.requestId).toBe(second.requestId);
  });
});

describe("R13：command.status 查询用 targetRequestId 指向原命令", () => {
  it("未认证时构造查询抛错（不得编造 connectionId）", () => {
    const { client } = setup();
    expect(() =>
      client.buildStatusQuery({ targetRequestId: PROMPT_REQUEST_ID, requestId: QUERY_REQUEST_ID }),
    ).toThrow(/connectionId|连接/u);
  });

  it("查询自身是新 ID，被查命令经 targetRequestId 传递", async () => {
    const harness = setup();
    await authenticate(harness);

    const query = harness.client.buildStatusQuery({
      targetRequestId: PROMPT_REQUEST_ID,
      requestId: QUERY_REQUEST_ID,
    });
    // 反例：若实现把 targetRequestId 写成查询自身的 ID → 断言变红。
    expect(query.body.command).toBe("command.status");
    expect(query.body.requestId).toBe(QUERY_REQUEST_ID);
    expect(query.body.requestId).not.toBe(PROMPT_REQUEST_ID);
    expect(query.body.payload).toEqual({ targetRequestId: PROMPT_REQUEST_ID });
    expect(query.connectionId).toBe(CONNECTION_ID);
    // 查询不产生新的派发登记：它不是一次重试。
    expect(harness.client.dispatchRecord(PROMPT_REQUEST_ID)).toBeNull();
  });

  it("被查命令仍在等结论时，查询不改变它的登记（不得因查询而新增派发次数）", async () => {
    const harness = setup();
    await authenticate(harness);

    harness.client.dispatch(promptCommand(PROMPT_REQUEST_ID));
    const before = harness.client.dispatchRecord(PROMPT_REQUEST_ID);
    harness.client.buildStatusQuery({
      targetRequestId: PROMPT_REQUEST_ID,
      requestId: QUERY_REQUEST_ID,
    });
    const after = harness.client.dispatchRecord(PROMPT_REQUEST_ID);

    expect(after).toEqual(before);
    expect(after?.dispatchCount).toBe(1);
  });
});