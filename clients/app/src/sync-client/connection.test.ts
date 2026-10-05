/**
 * R11/R12：逐连接签名握手、`subscribe`、ACK、阻断态。
 *
 * 每个用例都写成「反例会红」的形式：
 * - nonce 复用 → 两条连接的 `clientNonce` 断言不相等；
 * - 跳过 host 验签 → 断言 `hostProof` 为假时进入 `host_identity_changed` 且不发 `client_proof`；
 * - 把认证当会话级凭据 → 断言第二条连接仍重跑完整四步握手；
 * - 把 `caught_up` 之前当在线 → 断言 `replaying` 与 `online` 是两个不同事件。
 */

import { describe, expect, it } from "vitest";

import { SyncConnection, blockCauseForCloseCode, compareCursors, sortedFeatures } from "./connection";
import type { ClientBlockCause, SyncClientEvent } from "./connection";
import type { Cursor } from "../protocol";
import { EventLedger } from "./dedupe";
import { SnapshotStaging } from "./snapshot";
import {
  FakeHostIdentity,
  FakeIdentity,
  FakeRandom,
  FakeSocketFactory,
  FakeTranscript,
  fakeDigest,
  makeAuthenticated,
  makeChallenge,
  makeEvent,
  waitUntil,
  type Harness,
} from "./testing/harness";

const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const SERVER_EPOCH = "00384a03-bc90-4095-b65d-82fb8cc47e13";

/**
 * 建一条被测连接与它的全部替身。
 *
 * @param shared 共享的随机源。两条连接必须共用同一个实例：nonce 复用与否是本用例的判别点，
 *   而独立的计数器会让两次生成恰好相同，使断言失效。
 */
function setup(input: { readonly resumeCursor?: Cursor | null; readonly shared?: { readonly random: FakeRandom } } = {}): Harness {
  const sockets = new FakeSocketFactory();
  const random = input.shared?.random ?? new FakeRandom();
  const transcript = new FakeTranscript();
  const identity = new FakeIdentity();
  const host = new FakeHostIdentity();
  const digest = fakeDigest();
  const events: SyncClientEvent[] = [];
  const ledger = new EventLedger({ serverEpoch: SERVER_EPOCH, resumeFrom: input.resumeCursor ?? null });

  const connection = new SyncConnection(
    {
      hostId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
      url: "wss://host.example:8443/sync",
      identity,
      host,
      random,
      transcript,
      openSocket: sockets.open,
      resumeCursor: input.resumeCursor ?? null,
      // 账本由测试持有：跨连接复用同一实例（§8.5 切换窗口）。
      ledgerFor: () => ledger,
      onEvent: (event) => {
        events.push(event);
      },
    },
    new SnapshotStaging(digest),
  );

  return { connection, sockets, random, transcript, identity, host, digest, events, ledger };
}

/** 跑完握手（hello → challenge → proof → authenticated）。 */
async function completeHandshake(context: Harness): Promise<void> {
  context.connection.start();
  context.sockets.latest.open();
  await waitUntil(() => context.sockets.latest.types().includes("auth.client_hello"), "发出 clientHello");
  context.sockets.latest.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
  await waitUntil(() => context.sockets.latest.types().includes("auth.client_proof"), "发出 clientProof");
  context.sockets.latest.deliver(makeAuthenticated({ connectionId: CONNECTION_ID }));
  await waitUntil(() => context.sockets.latest.types().includes("sync.subscribe"), "发出 subscribe");
}

describe("R11：逐连接签名握手", () => {
  it("握手按 clientHello → serverChallenge → clientProof → authenticated 四步发出", async () => {
    const context = setup();
    await completeHandshake(context);

    expect(context.sockets.latest.types()).toEqual([
      "auth.client_hello",
      "auth.client_proof",
      "sync.subscribe",
    ]);
    // clientProof 必须在 challenge 之后、authenticated 之前。
    const proof = context.sockets.latest.last("auth.client_proof");
    expect(proof).toBeDefined();
  });

  it("每次连接重新生成 clientNonce，绝不复用", async () => {
    const shared = { random: new FakeRandom() };
    const first = setup({ shared });
    await completeHandshake(first);
    const second = setup({ shared });
    await completeHandshake(second);

    const nonceOf = (socketText: string): string => {
      const parsed = JSON.parse(socketText) as { body: { clientNonce: string } };
      return parsed.body.clientNonce;
    };
    const nonce1 = nonceOf(first.sockets.latest.sent[0]?.text ?? "{}");
    const nonce2 = nonceOf(second.sockets.latest.sent[0]?.text ?? "{}");

    expect(nonce1).not.toBe(nonce2);
    // 反例：若实现把 nonce 缓存在连接之外复用，两次会相等 → 上面的断言变红。
    expect(first.random.byteCalls).toEqual([32, 32]);
  });

  it("hostProof 校验失败时不发 clientProof，并进入身份变化阻断", async () => {
    const context = setup();
    context.connection.start();
    context.sockets.latest.open();
    await waitUntil(() => context.sockets.latest.types().includes("auth.client_hello"), "发出 clientHello");
    context.host.verificationResult = false;
    context.sockets.latest.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
    await waitUntil(() => context.events.some((event) => event.kind === "blocked"), "进入阻断态");

    expect(context.sockets.latest.types()).not.toContain("auth.client_proof");
    const blocked = context.events.find((event) => event.kind === "blocked");
    expect(blocked).toMatchObject({ kind: "blocked", cause: "host_identity_changed" });
  });

  it("已配对 hostId 与 challenge 不一致时判身份变化，不自动接受", async () => {
    const context = setup();
    context.connection.start();
    context.sockets.latest.open();
    await waitUntil(() => context.sockets.latest.types().includes("auth.client_hello"), "发出 clientHello");
    context.sockets.latest.deliver(
      makeChallenge({ connectionId: CONNECTION_ID, hostId: "99999999-9999-4999-8999-999999999999" }),
    );
    await waitUntil(() => context.events.some((event) => event.kind === "blocked"), "进入阻断态");

    const blocked = context.events.find((event) => event.kind === "blocked");
    expect(blocked).toMatchObject({ cause: "host_identity_changed" });
    expect(context.host.challenges).toHaveLength(0);
  });

  it("版本不兼容进入阻断态而不是继续握手", async () => {
    const context = setup();
    context.connection.start();
    context.sockets.latest.open();
    await waitUntil(() => context.sockets.latest.types().includes("auth.client_hello"), "发出 clientHello");
    context.sockets.latest.deliver(makeChallenge({ connectionId: CONNECTION_ID, selectedProtocolVersion: 2 }));
    await waitUntil(() => context.events.some((event) => event.kind === "blocked"), "进入阻断态");

    expect(context.events.find((event) => event.kind === "blocked")).toMatchObject({
      cause: "version_incompatible",
    });
    expect(context.sockets.latest.types()).not.toContain("auth.client_proof");
  });

  it("设备证明 transcript 用 device-proof domain 与两个 nonce", async () => {
    const context = setup();
    await completeHandshake(context);

    expect(context.transcript.encoded).toHaveLength(1);
    expect(context.transcript.encoded[0]?.domain).toBe("acp-remote/device-proof/v1");
    const tags = context.transcript.encoded[0]?.fields.map((field) => field.tag) ?? [];
    // §6.3 Device proof 字段集 1,2,3,6,9,10,11,12，按 tag 严格递增。
    expect(tags).toEqual([1, 2, 3, 6, 9, 10, 11, 12]);
    // 私钥只经端口签名一次。
    expect(context.identity.proofs).toHaveLength(1);
  });
});

describe("R11/R12：subscribe 与重放追平", () => {
  it("订阅使用最后确认游标；首次为 null", async () => {
    const first = setup();
    await completeHandshake(first);
    const firstSubscribe = first.sockets.latest.last("sync.subscribe");
    expect((firstSubscribe?.["body"] as { cursor: unknown }).cursor).toBeNull();

    const resumed = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "12" } });
    await completeHandshake(resumed);
    const resumedSubscribe = resumed.sockets.latest.last("sync.subscribe");
    expect((resumedSubscribe?.["body"] as { cursor: unknown }).cursor).toEqual({
      serverEpoch: SERVER_EPOCH,
      globalSequence: "12",
    });
  });

  it("caught_up 之前处于重放追平，两者不是同一状态", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    await completeHandshake(context);
    context.sockets.latest.deliver(makeEvent({ globalSequence: "2", eventId: "aaaaaaaa-0000-4000-8000-000000000002" }));
    context.sockets.latest.deliver(makeEvent({ globalSequence: "3", eventId: "aaaaaaaa-0000-4000-8000-000000000003" }));
    context.sockets.latest.deliver({
      protocolVersion: 1,
      type: "sync.caught_up",
      messageId: "cccccccc-1111-4111-8111-000000000001",
      connectionId: CONNECTION_ID,
      connectionSequence: "1",
      body: { cursor: { serverEpoch: SERVER_EPOCH, globalSequence: "3" } },
    });

    const kinds = context.events.map((event) => event.kind);
    // 反例：若实现把 replaying 直接当 online，`replaying` 会消失 → 本断言变红。
    expect(kinds).toContain("replaying");
    expect(kinds).toContain("online");
    expect(kinds.indexOf("replaying")).toBeLessThan(kinds.indexOf("online"));
  });

  it("事件进入可恢复状态后才 ACK，且 ACK 只能前进", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    await completeHandshake(context);
    context.sockets.latest.deliver(makeEvent({ globalSequence: "2", eventId: "aaaaaaaa-0000-4000-8000-000000000002" }));
    context.sockets.latest.deliver(makeEvent({ globalSequence: "3", eventId: "aaaaaaaa-0000-4000-8000-000000000003" }));

    const acks = context.sockets.latest.sent.filter((message) => message.parsed["type"] === "sync.ack");
    expect(acks).toHaveLength(2);
    expect((acks[0]?.parsed["body"] as { cursor: { globalSequence: string } }).cursor.globalSequence).toBe("2");

    // 重复投递不得产生新的呈现，也不得让 ACK 回退。
    context.sockets.latest.deliver(makeEvent({ globalSequence: "2", eventId: "aaaaaaaa-0000-4000-8000-000000000002" }));
    expect(context.sockets.latest.sent.filter((m) => m.parsed["type"] === "sync.ack")).toHaveLength(2);
    expect(context.events.filter((event) => event.kind === "duplicate_dropped")).toHaveLength(1);
    expect(context.connection.ackedCursor?.globalSequence).toBe("3");
  });

  it("序号出现缺口时不推进游标并上报 gap", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    await completeHandshake(context);
    context.sockets.latest.deliver(makeEvent({ globalSequence: "5", eventId: "aaaaaaaa-0000-4000-8000-000000000005" }));

    expect(context.events.find((event) => event.kind === "gap_detected")).toMatchObject({
      expected: "2",
      received: "5",
    });
    // 反例：若缺口被静默跳过，ACK 会前进到 5 → 本断言变红。
    expect(context.sockets.latest.sent.filter((m) => m.parsed["type"] === "sync.ack")).toHaveLength(0);
    expect(context.connection.ackedCursor?.globalSequence).toBe("1");
  });
});

describe("MAJOR-1：序号缺口必须真的恢复", () => {
  it("判 gap 后从最后确认游标重发 subscribe（不静默停摆）", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    await completeHandshake(context);
    context.sockets.latest.deliver(makeEvent({ globalSequence: "5", eventId: "aaaaaaaa-0000-4000-8000-000000000005" }));
    await waitUntil(
      () => context.sockets.latest.sent.filter((message) => message.parsed["type"] === "sync.subscribe").length === 2,
      "缺口后重新订阅",
    );

    const subscribes = context.sockets.latest.sent.filter((message) => message.parsed["type"] === "sync.subscribe");
    // 恢复点必须是最后确认位置（seq 1），不是缺口后的 5，也不是 null。
    expect((subscribes[1]?.parsed["body"] as { cursor: unknown }).cursor).toEqual({
      serverEpoch: SERVER_EPOCH,
      globalSequence: "1",
    });
    // 重订阅本身也必须是合法的出站序号（不能与上一条重复）。
    expect(subscribes[1]?.parsed["connectionSequence"]).toBe("2");
  });

  it("重新订阅后从断点续传的事件可正常处理", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    await completeHandshake(context);
    context.sockets.latest.deliver(makeEvent({ globalSequence: "9", eventId: "aaaaaaaa-0000-4000-8000-000000000009" }));
    await waitUntil(() => context.events.some((event) => event.kind === "gap_detected"), "上报缺口");

    context.sockets.latest.deliver(makeEvent({ globalSequence: "2", eventId: "aaaaaaaa-0000-4000-8000-000000000002" }));

    // 反例：若缺口后事件流停摆，这里会是 0 条 event → 断言变红。
    expect(context.events.filter((event) => event.kind === "event")).toHaveLength(1);
    expect(context.connection.ackedCursor?.globalSequence).toBe("2");
  });
});

describe("CRITICAL-3：出站序号从 1 严格递增", () => {
  it("同连接内连续 ACK 与 subscribe 的序号不重复、不回退", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    await completeHandshake(context);
    context.sockets.latest.deliver(makeEvent({ globalSequence: "2", eventId: "aaaaaaaa-0000-4000-8000-000000000002" }));
    context.sockets.latest.deliver(makeEvent({ globalSequence: "3", eventId: "aaaaaaaa-0000-4000-8000-000000000003" }));
    context.sockets.latest.deliver({
      protocolVersion: 1,
      type: "control.ping",
      messageId: "eeeeeeee-1111-4111-8111-000000000001",
      connectionId: CONNECTION_ID,
      connectionSequence: "1",
      body: { nonce: "AQID" },
    });

    const sequences = context.sockets.latest.sent
      .filter((message) => message.parsed["connectionId"] !== undefined)
      .map((message) => message.parsed["connectionSequence"]);
    // 反例：若 ACK 恒为 "0" 或 subscribe 抄服务端序号 → 断言变红。
    expect(sequences).toEqual(["1", "2", "3", "4"]);
  });
});

describe("R12：阻断态", () => {
  it("close 4411 映射为被顶替，不自动重连", () => {
    expect(blockCauseForCloseCode(4411)).toBe("replaced_by_other_connection");
  });

  it("close 4410/4406/4409 分别映射撤销/版本不兼容/身份变化", () => {
    const mapping: Record<number, ClientBlockCause> = {
      4410: "device_revoked",
      4406: "version_incompatible",
      4409: "host_identity_changed",
    };
    for (const [code, cause] of Object.entries(mapping)) {
      expect(blockCauseForCloseCode(Number(code))).toBe(cause);
    }
  });

  it("被顶替后不再产生 closed 事件（不会进入重连路径）", async () => {
    const context = setup();
    await completeHandshake(context);
    context.sockets.latest.serverClose(4411, "connection_replaced");

    const blocked = context.events.find((event) => event.kind === "blocked");
    expect(blocked).toMatchObject({ cause: "replaced_by_other_connection", code: 4411 });
    // 反例：若实现把 4411 当普通可重试关闭，这里会出现 closed → 断言变红。
    expect(context.events.some((event) => event.kind === "closed")).toBe(false);
  });

  it("device.revoked 错误码进入撤销阻断", async () => {
    const context = setup();
    await completeHandshake(context);
    context.sockets.latest.deliver({
      protocolVersion: 1,
      type: "error",
      messageId: "dddddddd-1111-4111-8111-000000000001",
      connectionId: CONNECTION_ID,
      connectionSequence: "1",
      body: {
        code: "auth.device_revoked",
        message: "设备已撤销",
        retryable: false,
        correlationId: null,
        details: {},
      },
    });

    expect(context.events.find((event) => event.kind === "blocked")).toMatchObject({
      cause: "device_revoked",
    });
  });
});

describe("R14：跨连接去重", () => {
  it("同一条 eventId 在第二条连接重投时不重复呈现", async () => {
    const ledger = new EventLedger({ serverEpoch: SERVER_EPOCH });
    const sockets = new FakeSocketFactory();
    const digest = fakeDigest();
    const events: SyncClientEvent[] = [];

    const build = (): SyncConnection =>
      new SyncConnection(
        {
          hostId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
          url: "wss://host.example:8443/sync",
          identity: new FakeIdentity(),
          host: new FakeHostIdentity(),
          random: new FakeRandom(),
          transcript: new FakeTranscript(),
          openSocket: sockets.open,
          resumeCursor: null,
          ledgerFor: () => ledger,
          onEvent: (event) => {
            events.push(event);
          },
        },
        new SnapshotStaging(digest),
      );

    const first = build();
    first.start();
    sockets.latest.open();
    await waitUntil(() => sockets.latest.types().includes("auth.client_hello"), "第一条连接发出 clientHello");
    sockets.latest.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
    await waitUntil(() => sockets.latest.types().includes("auth.client_proof"), "第一条连接发出 clientProof");
    sockets.latest.deliver(makeAuthenticated({ connectionId: CONNECTION_ID }));
    await waitUntil(() => sockets.latest.types().includes("sync.subscribe"), "第一条连接发出 subscribe");

    const event = makeEvent({ globalSequence: "1", eventId: "aaaaaaaa-0000-4000-8000-000000000001" });
    sockets.latest.deliver(event);

    // 第二条连接：账本仍是同一个（§8.5 切换窗口）。
    const second = build();
    second.start();
    sockets.latest.open();
    await waitUntil(() => sockets.latest.types().includes("auth.client_hello"), "第二条连接发出 clientHello");
    sockets.latest.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
    await waitUntil(() => sockets.latest.types().includes("auth.client_proof"), "第二条连接发出 clientProof");
    sockets.latest.deliver(makeAuthenticated({ connectionId: CONNECTION_ID }));
    await waitUntil(() => sockets.latest.types().includes("sync.subscribe"), "第二条连接发出 subscribe");
    sockets.latest.deliver(event);

    expect(events.filter((item) => item.kind === "event")).toHaveLength(1);
    expect(events.filter((item) => item.kind === "duplicate_dropped")).toHaveLength(1);
  });
});

describe("纯函数判定", () => {
  it("feature 列表去重并按 UTF-8 字节升序排列", () => {
    expect(sortedFeatures(["b.v1", "a.v1", "b.v1"])).toEqual(["a.v1", "b.v1"]);
  });

  it("跨 epoch 的游标不可比（判为需要重新订阅）", () => {
    expect(
      compareCursors(
        { serverEpoch: "00000000-0000-4000-8000-000000000000", globalSequence: "99" },
        { serverEpoch: SERVER_EPOCH, globalSequence: "1" },
      ),
    ).toBe(-1);
    expect(
      compareCursors({ serverEpoch: SERVER_EPOCH, globalSequence: "3" }, { serverEpoch: SERVER_EPOCH, globalSequence: "3" }),
    ).toBe(0);
  });
});