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

import {
  DECLARED_SUBPROTOCOLS,
  LIVENESS_PROBE_INTERVAL_MS,
  LIVENESS_TIMEOUT_MS,
  REQUIRED_FEATURES,
  SYNC_WS_SUBPROTOCOL,
  SyncConnection,
  blockCauseForCloseCode,
  compareCursors,
  sortedFeatures,
} from "./connection";
import type { ClientBlockCause, SyncClientEvent } from "./connection";
import { CLOSE } from "./blocking";
import type { Cursor } from "../protocol";
import { EventLedger } from "./dedupe";
import { SnapshotStaging } from "./snapshot";
import {
  FakeClock,
  FakeHostIdentity,
  FakeIdentity,
  FakeRandom,
  FakeSocketFactory,
  FakeTranscript,
  fakeDigest,
  makeAuthenticated,
  makeChallenge,
  makeCommandResult,
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
function setup(
  input: {
    readonly resumeCursor?: Cursor | null;
    readonly shared?: { readonly random: FakeRandom };
    readonly sharedClock?: FakeClock;
    /** 覆盖升级请求声明的子协议列表（缺省即组合根传的那一份）。 */
    readonly subprotocols?: readonly string[];
  } = {},
): Harness {
  const sockets = new FakeSocketFactory();
  const random = input.shared?.random ?? new FakeRandom();
  const transcript = new FakeTranscript();
  const identity = new FakeIdentity();
  const host = new FakeHostIdentity();
  const digest = fakeDigest();
  const events: SyncClientEvent[] = [];
  const clock = input.sharedClock ?? new FakeClock();
  const ledger = new EventLedger({ serverEpoch: SERVER_EPOCH, resumeFrom: input.resumeCursor ?? null });

  const connection = new SyncConnection(
    {
      hostId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
      url: "wss://host.example:8443/sync/v1",
      subprotocols: input.subprotocols ?? DECLARED_SUBPROTOCOLS,
      identity,
      host,
      random,
      transcript,
      openSocket: sockets.open,
      resumeCursor: input.resumeCursor ?? null,
      clock,
      // 账本由测试持有：跨连接复用同一实例（§8.5 切换窗口）。
      ledgerFor: () => ledger,
      onEvent: (event) => {
        events.push(event);
      },
    },
    new SnapshotStaging(digest),
  );

  return { connection, sockets, random, transcript, identity, host, digest, events, ledger, clock };
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

/** 事件库里某条事件的稳定 `eventId`（与 `makeEvent` 的 messageId 规则对齐，便于阅读断言）。 */
function eventIdFor(sequence: string): string {
  return `aaaaaaaa-0000-4000-8000-${sequence.padStart(12, "0")}`;
}

/** `sync.caught_up`（服务端屏障，§9.3 第 4 步）。 */
function caughtUp(sequence: string): Record<string, unknown> {
  return {
    protocolVersion: 1,
    type: "sync.caught_up",
    messageId: `cccccccc-1111-4111-8111-${sequence.padStart(12, "0")}`,
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: { cursor: { serverEpoch: SERVER_EPOCH, globalSequence: sequence } },
  };
}

/** 一条 §9.3 合规的「增量重放 + 屏障」应答器。 */
interface ReplayServer {
  /** 交付队列里的全部消息（真实 WSS 上消息异步到达；同步递归只会把失控表现成爆栈）。 */
  drain(): void;
  /** 再排入一次「重放 + caught_up」。 */
  replay(): void;
  /** 推进服务端 head（模拟服务端继续产生事件）。 */
  setHead(sequence: string): void;
  /** 收到的 `sync.subscribe` 条数（含认证后的首条）。 */
  subscribeCount(): number;
}

/**
 * 把假服务端建模成 §9.3 的**响应式**重放器。
 *
 * ## 为什么必须有它
 *
 * 前三轮的替身都停在「客户端主动发、服务端被动等」，于是 §9.3 的两个关键语义——「收到
 * `sync.subscribe` 就重放可见事件」与「重放后必发 `sync.caught_up`」——根本没被建模。
 * 缺了它们，「服务端漏投」「屏障吞事件」「退避上限不可达」这一整类缺陷在测试里无法构造，
 * 用例只能证明「服务端什么都不做时有界」。
 *
 * @param options.visible 本设备**可见**的序号；`withheld` 里的序号是服务端明知可见却不投递的
 *   （正常服务端不会有，这里用来构造真实的丢失区间）。
 */
function attachReplayServer(
  context: Harness,
  options: {
    readonly visible: readonly string[];
    readonly head: string;
    readonly withheld?: ReadonlySet<string>;
  },
): ReplayServer {
  const socket = context.sockets.latest;
  let head = options.head;
  let lastCursor: string | null = null;
  const queue: unknown[] = [];
  let subscribes = 0;

  const enqueueReplay = (): void => {
    for (const sequence of options.visible) {
      if (options.withheld?.has(sequence) === true) continue;
      if (lastCursor !== null && BigInt(sequence) <= BigInt(lastCursor)) continue;
      queue.push(makeEvent({ globalSequence: sequence, eventId: eventIdFor(sequence) }));
    }
    queue.push(caughtUp(head));
  };

  // 包装必须在握手**之前**装上：认证后的首条 subscribe 同样是服务端重放的触发点。
  const originalSend = socket.send.bind(socket);
  socket.send = (text: string): void => {
    originalSend(text);
    const parsed: unknown = JSON.parse(text);
    if (typeof parsed !== "object" || parsed === null || !("type" in parsed)) return;
    if (parsed.type !== "sync.subscribe") return;
    if (!("body" in parsed)) return;
    const body: unknown = parsed.body;
    if (typeof body !== "object" || body === null || !("cursor" in body)) return;
    const cursor: unknown = body.cursor;
    lastCursor =
      typeof cursor === "object" && cursor !== null && "globalSequence" in cursor && typeof cursor.globalSequence === "string"
        ? cursor.globalSequence
        : null;
    subscribes += 1;
    enqueueReplay();
  };

  return {
    drain: () => {
      for (const message of queue.splice(0)) socket.deliver(message);
    },
    replay: () => enqueueReplay(),
    setHead: (sequence: string) => {
      head = sequence;
    },
    subscribeCount: () => subscribes,
  };
}

/** 被呈现的事件序号序列（R14 的「不丢、不重」都落在这条序列上）。 */
function presentedSequences(context: Harness): string[] {
  return context.events
    .filter((event) => event.kind === "event")
    .map((event) => event.event.body.globalSequence);
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

  it("scope 过滤造成的序号空洞不判丢失：直接呈现并推进水位，不重订阅", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    await completeHandshake(context);
    // 本用例的计时器断言只关心缺口重订阅：先关掉存活探测，避免它的常驻定时器混入计数。
    context.connection.setProbeVisible(false);
    // §9.2：2/3/4 因权限过滤对本设备不可见，下一条可见事件是 5。
    context.sockets.latest.deliver(makeEvent({ globalSequence: "5", eventId: eventIdFor("5") }));

    // 反例：若仍按 `last + 1` 判缺口，这里会是 gap_detected + 一条待触发的重订阅 → 断言变红。
    expect(context.events.some((event) => event.kind === "gap_detected")).toBe(false);
    expect(context.clock.pendingTimers).toBe(0);
    expect(presentedSequences(context)).toEqual(["5"]);
    expect(context.sockets.latest.sent.filter((message) => message.parsed["type"] === "sync.ack")).toHaveLength(1);
    expect(context.connection.ackedCursor?.globalSequence).toBe("5");
  });
});

describe("§9.3 判别力：重放 + 屏障 + scope 空洞", () => {
  /**
   * 主用例：一条真正按 §9.3 建模的服务端（收到 `sync.subscribe` → 重放可见事件 → 发
   * `sync.caught_up` → 之后继续实时事件），设备侧存在因 scope 过滤而不可见的序号段。
   *
   * 事件库布局：
   * - seq 1：上次会话已确认；
   * - seq 2/3/4：因权限过滤对本设备**不可见**（§9.2 明文允许的常态）；
   * - seq 5/6/7：可见，重放时投递；
   * - seq 9：可见但服务端漏投（它随后发 `caught_up@9`，即**违反了自己的屏障声明**）。
   */
  it("每条服务端投递的可见事件都被呈现；合法空洞不触发重订阅；漏投可检出且能补齐", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    const server = attachReplayServer(context, {
      visible: ["5", "6", "7", "9"],
      head: "9",
      withheld: new Set(["9"]),
    });
    await completeHandshake(context);
    // 本用例用 `pendingTimers` 判别缺口重订阅是否存在：关掉存活探测，保持计时器计数只反映它。
    context.connection.setProbeVisible(false);
    server.drain();

    // (1) 服务端实际投递的 5/6/7 全部呈现：不存在「已投递但被丢弃」的事件。
    //     反例：修复前这里是 presented=["8"]（5/6/7 被 gap 分支丢掉）→ 断言变红。
    expect(presentedSequences(context)).toEqual(["5", "6", "7"]);
    // (2) 2/3/4 的空洞不产生任何丢失判定，也不触发重订阅。
    expect(context.events.filter((event) => event.kind === "gap_detected")).toHaveLength(0);
    expect(context.clock.pendingTimers).toBe(0);
    expect(server.subscribeCount()).toBe(1);
    // (3) 屏障被采纳为权威水位（§9.5：不可见段也可以 ACK `caught_up.cursor`）。
    expect(context.connection.ackedCursor).toEqual({ serverEpoch: SERVER_EPOCH, globalSequence: "9" });
    expect(context.events.find((event) => event.kind === "online")).toMatchObject({
      cursor: { globalSequence: "9" },
    });

    // (4) 屏障之后的实时事件继续正常呈现（事件流没有停摆）。
    context.sockets.latest.deliver(makeEvent({ globalSequence: "11", eventId: eventIdFor("11") }));
    expect(presentedSequences(context)).toEqual(["5", "6", "7", "11"]);

    // (5) 事后让服务端重放 5/6/7：不重复呈现，也不丢。
    server.replay();
    server.drain();
    expect(presentedSequences(context)).toEqual(["5", "6", "7", "11"]);
    expect(context.events.filter((event) => event.kind === "duplicate_dropped")).toHaveLength(3);
    expect(context.events.filter((event) => event.kind === "gap_detected")).toHaveLength(0);

    // (6) 服务端把此前漏投的 9 补投过来：必须被检出（丢失区间证据）并**补齐呈现**。
    //     反例：修复前这类事件被当作 duplicate 静默丢弃、永久不可恢复 → 断言变红。
    context.sockets.latest.deliver(makeEvent({ globalSequence: "9", eventId: eventIdFor("9") }));
    expect(presentedSequences(context)).toEqual(["5", "6", "7", "11", "9"]);
    expect(context.events.filter((event) => event.kind === "gap_detected")).toEqual([
      { kind: "gap_detected", expected: "11", received: "9" },
    ]);
    // 补齐后水位不动（未呈现过的序号不满足「已处理」），但从最后确认位置重新订阅去看是否还有漏投。
    expect(context.connection.ackedCursor?.globalSequence).toBe("11");
    expect(context.clock.pendingTimers).toBe(1);

    // (7) 同一条再投一次是重复，不二次呈现。
    context.sockets.latest.deliver(makeEvent({ globalSequence: "9", eventId: eventIdFor("9") }));
    expect(presentedSequences(context)).toEqual(["5", "6", "7", "11", "9"]);
  });

  /**
   * 退避预算（P1-B）：§9.3 要求服务端**每次**重放后都发 `caught_up`，所以「收到过 `caught_up`
   * 就清零预算」会让预算每轮复位、退避恒为 250ms、`maxAttempts` 永远不可达。
   *
   * 事件库：seq 2/3/4 不可见；5/6/7 重放；10..15 可见但服务端违反屏障声明不投递，随后逐轮补投；
   * 屏障固定停在 15，因此每一轮重订阅的 `caught_up` 都**不推进**水位。
   */
  it("水位不前进时预算不被 caught_up 清零：退避递增、到顶后上报并停止", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    const server = attachReplayServer(context, {
      visible: ["5", "6", "7", "10", "11", "12", "13", "14", "15"],
      head: "15",
      withheld: new Set(["10", "11", "12", "13", "14", "15"]),
    });
    await completeHandshake(context);
    // 本用例推进假时钟来验证退避：关闭存活探测，否则计时器计数与推进都会混入探测定时器。
    context.connection.setProbeVisible(false);
    server.drain();
    expect(presentedSequences(context)).toEqual(["5", "6", "7"]);
    expect(context.connection.ackedCursor?.globalSequence).toBe("15");

    const delays = [250, 500, 1000, 2000, 4000];
    const withheld = ["10", "11", "12", "13", "14"];
    for (const [index, sequence] of withheld.entries()) {
      context.sockets.latest.deliver(makeEvent({ globalSequence: sequence, eventId: eventIdFor(sequence) }));
      expect(context.clock.pendingTimers).toBe(1);
      // 退避确实在递增：提前 1ms 不触发，正点才触发。
      const delayMs = delays[index] ?? 0;
      context.clock.advanceBy(delayMs - 1);
      expect(server.subscribeCount()).toBe(1 + index);
      context.clock.advanceBy(1);
      expect(server.subscribeCount()).toBe(2 + index);
      // §9.3：重订阅后服务端重放（游标之上无新事件）+ 发 `caught_up@15`（水位不前进）。
      server.drain();
      // 恢复点必须是最后确认位置。
      const subscribes = context.sockets.latest.sent.filter((message) => message.parsed["type"] === "sync.subscribe");
      expect((subscribes.at(-1)?.parsed["body"] as { cursor: unknown }).cursor).toEqual({
        serverEpoch: SERVER_EPOCH,
        globalSequence: "15",
      });
    }

    // 第 6 条漏投事件：预算已到顶 → 上报可诊断原因，并且**真的不再重订阅**（不是只上报）。
    context.sockets.latest.deliver(makeEvent({ globalSequence: "15", eventId: eventIdFor("15") }));
    expect(context.events.some((event) => event.kind === "rejected" && event.reason === "gap_resubscribe_exhausted")).toBe(
      true,
    );
    expect(context.clock.pendingTimers).toBe(0);
    expect(server.subscribeCount()).toBe(1 + delays.length);

    // 安全网只停「重订阅」，不停「呈现」：到顶之后到达的可见事件仍必须补齐。
    expect(presentedSequences(context)).toEqual(["5", "6", "7", "10", "11", "12", "13", "14", "15"]);
  });

  it("屏障真正抬高水位时预算才复位，下一段抖动拿得到完整预算", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    const server = attachReplayServer(context, {
      visible: ["5", "6", "7", "10", "11", "12", "13", "14", "15"],
      head: "15",
      withheld: new Set(["10", "11", "12", "13", "14", "15"]),
    });
    await completeHandshake(context);
    // 同样推进假时钟：只保留缺口重订阅的定时器。
    context.connection.setProbeVisible(false);
    server.drain();

    // 连续 6 条漏投事件把预算走完。
    for (const sequence of ["10", "11", "12", "13", "14", "15"]) {
      context.sockets.latest.deliver(makeEvent({ globalSequence: sequence, eventId: eventIdFor(sequence) }));
      context.clock.advanceBy(4000);
      server.drain();
    }
    expect(context.events.some((event) => event.kind === "rejected" && event.reason === "gap_resubscribe_exhausted")).toBe(
      true,
    );

    // 服务端继续推进 head 并重放：这一次 `caught_up@40` **抬高了水位** → 预算复位。
    server.setHead("40");
    server.replay();
    server.drain();
    expect(context.connection.ackedCursor?.globalSequence).toBe("40");

    // 新一段抖动拿得到完整预算（第 1 次就是 250ms）。
    context.sockets.latest.deliver(makeEvent({ globalSequence: "39", eventId: eventIdFor("39") }));
    expect(context.clock.pendingTimers).toBe(1);
    context.clock.advanceBy(249);
    expect(server.subscribeCount()).toBe(1 + 5);
    context.clock.advanceBy(1);
    expect(server.subscribeCount()).toBe(1 + 5 + 1);
  });

  it("caught_up 把账本推进到屏障后，下一条可见事件直接呈现（恢复通道与水位对齐）", async () => {
    // 触发条件不是边缘情况：§9.2/§9.5 明文允许存在因 scope 过滤而不可见的序号段。
    const context = setup();
    await completeHandshake(context);
    // 序号 100 之前的事件因权限过滤不可见（这是常态，不是异常）。
    context.sockets.latest.deliver(caughtUp("100"));
    // 屏障之后的**第一条可见事件**序号必然大于 100（中间有不可见段）。
    context.sockets.latest.deliver(makeEvent({ globalSequence: "101", eventId: eventIdFor("101") }));

    // 反例：若账本不感知 caught_up（修复前），这里是 gap_detected 且 subscribe 会反复重发。
    expect(context.events.filter((event) => event.kind === "gap_detected")).toHaveLength(0);
    expect(presentedSequences(context)).toEqual(["101"]);
    expect(context.connection.ackedCursor?.globalSequence).toBe("101");
  });

  it("跨 epoch 的 caught_up 不被采纳：不 ACK、不宣告 online、也不推进已确认游标", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    await completeHandshake(context);
    context.sockets.latest.deliver({
      protocolVersion: 1,
      type: "sync.caught_up",
      messageId: "cccccccc-1111-4111-8111-000000000002",
      connectionId: CONNECTION_ID,
      connectionSequence: "1",
      body: { cursor: { serverEpoch: "11111111-2222-4333-8444-555555555555", globalSequence: "500" } },
    });

    // 反例：账本拒绝（epoch 不可比）而连接侧仍照单采纳，会让外来游标覆盖真实恢复点 → 断言变红。
    expect(context.connection.ackedCursor?.globalSequence).toBe("1");
    expect(context.sockets.latest.sent.filter((message) => message.parsed["type"] === "sync.ack")).toHaveLength(0);
    expect(context.events.some((event) => event.kind === "online")).toBe(false);
    expect(context.events.some((event) => event.kind === "rejected" && event.reason === "caught_up_epoch_mismatch")).toBe(
      true,
    );
  });
});

describe("CRITICAL-3：出站序号从 1 严格递增", () => {
  it("同连接内连续 ACK 与 subscribe 的序号不重复、不回退", async () => {
    const context = setup({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "1" } });
    await completeHandshake(context);
    context.connection.setProbeVisible(true);
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
    // 认证成功后立即发出的存活探测 ping 也消耗一个客户端方向序号（subscribe=1、ping=2、
    // 两条 ACK=3/4、ping 的 pong=5）。
    // 反例：若 ACK 恒为 "0" 或 subscribe 抄服务端序号 → 断言变红。
    expect(sequences).toEqual(["1", "2", "3", "4", "5"]);
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
          url: "wss://host.example:8443/sync/v1",
          subprotocols: DECLARED_SUBPROTOCOLS,
          identity: new FakeIdentity(),
          host: new FakeHostIdentity(),
          random: new FakeRandom(),
          transcript: new FakeTranscript(),
          openSocket: sockets.open,
          resumeCursor: null,
          clock: new FakeClock(),
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

describe("连接契约：升级请求声明的子协议（§3.1）", () => {
  it("建立 socket 时把声明的子协议列表交给端口（非空且含约定子协议）", () => {
    const context = setup();

    // 断言的是**端口实际收到的入参**：漏传时这里是 `undefined`、传空列表时这里是 `[]`，
    // 两种写法都会让下面两条断言变红（只看 URL 的断言对它们完全无感）。
    const declared = context.sockets.protocols[0];
    expect(declared).toBeDefined();
    expect(declared?.length ?? 0).toBeGreaterThan(0);
    expect(declared).toContain(SYNC_WS_SUBPROTOCOL);
    expect(declared).toEqual(DECLARED_SUBPROTOCOLS);
  });

  it("声明列表由构造方给出（端口是接缝，不是常量读取处）", () => {
    // 注入一份**别**的声明：端口必须原样交出去，而不是把它替换成模块内的缺省常量。
    const context = setup({ subprotocols: ["acp-remote.sync.v999.json"] });

    expect(context.sockets.protocols).toEqual([["acp-remote.sync.v999.json"]]);
  });

  it("服务端未选中约定子协议：进阻断态、终止连接，不落回普通重连", async () => {
    const context = setup();
    // 真实内核只在 `WebSocket.protocol` 上暴露协商结果：服务端不选 subprotocol 时升级仍然
    // 成功、没有 error 事件、也没有非 1000 的关闭码，因此这件事只能由客户端自己核对。
    context.sockets.latest.negotiated = "";
    context.connection.start();
    context.sockets.latest.open();
    await waitUntil(() => context.events.some((event) => event.kind === "blocked"), "进入阻断态");

    // 反例：不核对协商结果时，这条连接会照常发出 client_hello，并在服务端关闭后落回
    // `closed`（普通重连路径）→ 下面每条断言都会变红。
    expect(context.sockets.latest.sent).toEqual([]);
    expect(context.events.some((event) => event.kind === "closed")).toBe(false);
    expect(context.events.find((event) => event.kind === "blocked")).toMatchObject({
      kind: "blocked",
      cause: "version_incompatible",
      code: 4406,
      reason: "subprotocol_not_selected",
    });
    // 客户端主动终止这条连接（§3.1），而不是把它当成「网络不好」再试一次。
    expect(context.sockets.latest.closedWith).toEqual({ code: 4406, reason: "version_incompatible" });
  });

  it("负向对照：网络断开走可区分的事件（`closed`，不是 `blocked`）", async () => {
    const context = setup();
    await completeHandshake(context);
    context.sockets.latest.serverClose(1006, "connection reset by peer");

    // 「子协议/版本不符」与「网络断开」必须能分开：前者是阻断态（停止自动重连），
    // 后者是普通断开（状态层级按退避重连）。若实现把两者合成一条路径，本用例会变红。
    expect(context.events.some((event) => event.kind === "blocked")).toBe(false);
    expect(context.events.find((event) => event.kind === "closed")).toMatchObject({ code: 1006 });
  });
});

describe("连接契约：必需 feature 的覆盖判定（§5.2）", () => {
  it("服务端选中的 feature 覆盖必需集合时握手继续", async () => {
    const context = setup();
    context.connection.start();
    context.sockets.latest.open();
    await waitUntil(() => context.sockets.latest.types().includes("auth.client_hello"), "发出 clientHello");
    context.sockets.latest.deliver(
      makeChallenge({ connectionId: CONNECTION_ID, selectedFeatures: [...REQUIRED_FEATURES] }),
    );
    await waitUntil(() => context.sockets.latest.types().includes("auth.client_proof"), "发出 clientProof");

    expect(context.events.some((event) => event.kind === "blocked")).toBe(false);
  });

  it("服务端漏选一条必需 feature：阻断且不发设备证明", async () => {
    const context = setup();
    context.connection.start();
    context.sockets.latest.open();
    await waitUntil(() => context.sockets.latest.types().includes("auth.client_hello"), "发出 clientHello");
    // ACK 面仍在，缺的是命令状态查询（`core.command-status.v1`）——客户端必须在签设备证明
    // **之前**发现这件事：缺能力的连接一旦被当成正常连接，R13/R14 的保证会静默失效。
    context.sockets.latest.deliver(makeChallenge({ connectionId: CONNECTION_ID, selectedFeatures: ["core.event-ack.v1"] }));
    await waitUntil(() => context.events.some((event) => event.kind === "blocked"), "进入阻断态");

    // 反例：若握手处不做集合包含判定，这条连接会发出 client_proof 并继续 subscribe → 断言变红。
    expect(context.sockets.latest.types()).not.toContain("auth.client_proof");
    expect(context.events.find((event) => event.kind === "blocked")).toMatchObject({
      kind: "blocked",
      cause: "version_incompatible",
      reason: "required_features_missing",
    });
    // 原因与「协议版本不兼容」「子协议未被接受」是三个不同取值：同一句「不兼容」文案下
    // 三者恢复路径不同，事件里必须能分开。
    expect(context.events.find((event) => event.kind === "rejected")).toMatchObject({
      reason: "required_features_missing",
      detail: "core.command-status.v1,core.session-activity.v1",
    });
  });

  it("协议版本不符的成因与必需 feature 缺失可区分", async () => {
    const context = setup();
    context.connection.start();
    context.sockets.latest.open();
    await waitUntil(() => context.sockets.latest.types().includes("auth.client_hello"), "发出 clientHello");
    context.sockets.latest.deliver(makeChallenge({ connectionId: CONNECTION_ID, selectedProtocolVersion: 2 }));
    // 版本判定在 `loadPairedHost(...)` 的 await 之后（connection.ts:520 → :525）：阻断是异步产生的，
    // 必须先等事件落地再断言，否则读到的是尚未推进的状态。
    await waitUntil(() => context.events.some((event) => event.kind === "blocked"), "进入阻断态");

    // 反例：若版本判定被删掉，这条连接会继续走到验签并发出 client_proof → 两条断言都变红。
    expect(context.sockets.latest.types()).not.toContain("auth.client_proof");
    expect(context.events.find((event) => event.kind === "blocked")).toMatchObject({
      kind: "blocked",
      cause: "version_incompatible",
      reason: "protocol_version_unsupported",
    });
  });

  it("服务端回 protocol.feature_required 时同样进阻断态，原因仍是必需 feature", async () => {
    const context = setup();
    await completeHandshake(context);
    context.sockets.latest.deliver({
      protocolVersion: 1,
      type: "error",
      messageId: "dddddddd-1111-4111-8111-000000000002",
      connectionId: CONNECTION_ID,
      connectionSequence: "1",
      body: {
        code: "protocol.feature_required",
        message: "服务端未启用必需能力",
        retryable: false,
        correlationId: null,
        details: { features: ["core.command-status.v1"] },
      },
    });

    expect(context.events.find((event) => event.kind === "blocked")).toMatchObject({
      kind: "blocked",
      cause: "version_incompatible",
      code: 4406,
      reason: "required_features_missing",
    });
  });
});
// ── 应用层存活探测（§13） ─────────────────────────────────────────────────────

/** 本连接已发出的全部 `control.ping`（按发送序）。 */
function livenessPings(context: Harness): Record<string, unknown>[] {
  return context.sockets.latest.sent
    .filter((message) => message.parsed["type"] === "control.ping")
    .map((message) => message.parsed);
}

/** ping 的 nonce（`body.nonce`）。 */
function nonceOf(ping: Record<string, unknown>): string {
  return (ping["body"] as { readonly nonce: string }).nonce;
}

/** 一条与服务端形状一致的 `control.pong`。 */
function pongFor(nonce: string, sequence = 1): Record<string, unknown> {
  return {
    protocolVersion: 1,
    type: "control.pong",
    messageId: `eeeeeeee-1111-4111-8111-${String(sequence).padStart(12, "0")}`,
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: { nonce },
  };
}

/** 确认当前所有待确认的 ping（每个 nonce 回一条 pong；重复清理是无害的）。 */
function acknowledgeAllPings(context: Harness): void {
  for (const [index, ping] of livenessPings(context).entries()) {
    context.sockets.latest.deliver(pongFor(nonceOf(ping), index + 1));
  }
}

describe("应用层存活探测：认证后按秒发 ping", () => {
  it("认证成功后立即发第一条，此后每 1000ms 一条，nonce 为 22 字符无填充 base64url", async () => {
    const context = setup();
    await completeHandshake(context);
    context.connection.setProbeVisible(true);

    const first = livenessPings(context)[0];
    expect(first).toBeDefined();
    expect(first?.["protocolVersion"]).toBe(1);
    expect(first?.["connectionId"]).toBe(CONNECTION_ID);
    // subscribe 占 1，首条 ping 是客户端方向的第 2 个序号。
    expect(first?.["connectionSequence"]).toBe("2");
    expect(first?.["body"]).toEqual({ nonce: expect.any(String) });
    const firstNonce = nonceOf(first as Record<string, unknown>);
    expect(firstNonce).toMatch(/^[A-Za-z0-9_-]{22}$/);

    context.clock.advanceBy(LIVENESS_PROBE_INTERVAL_MS);
    expect(livenessPings(context)).toHaveLength(2);
    context.clock.advanceBy(LIVENESS_PROBE_INTERVAL_MS);
    expect(livenessPings(context)).toHaveLength(3);

    // 每条 ping 的 nonce 都是新的：复用 nonce 会让迟到的 pong 顶替后一条的确认。
    const nonces = livenessPings(context).map(nonceOf);
    expect(new Set(nonces).size).toBe(nonces.length);
  });

  it("匹配的 pong 清除对应 ping：持续应答时连接保持打开", async () => {
    const context = setup();
    await completeHandshake(context);
    context.connection.setProbeVisible(true);
    acknowledgeAllPings(context);

    for (let tick = 0; tick < 6; tick += 1) {
      context.clock.advanceBy(LIVENESS_PROBE_INTERVAL_MS);
      acknowledgeAllPings(context);
    }

    expect(livenessPings(context)).toHaveLength(7);
    // 反例：若 pong 不被匹配到待确认表，3 秒后这里会有 closed → 断言变红。
    expect(context.events.some((event) => event.kind === "closed")).toBe(false);
  });

  it("不请自来的 pong 被忽略：它不能为任何真实 ping 续命", async () => {
    const context = setup();
    await completeHandshake(context);
    context.connection.setProbeVisible(true);
    context.sockets.latest.deliver(pongFor("A".repeat(22)));

    context.clock.advanceBy(LIVENESS_TIMEOUT_MS);

    const closed = context.events.filter((event) => event.kind === "closed");
    expect(closed).toHaveLength(1);
    expect(closed[0]).toMatchObject({ code: CLOSE.serverUnavailable, detail: "liveness_timeout" });
  });

  it("业务事件、服务端 ping 与命令结果都不清除待确认 ping（不重置存活判定）", async () => {
    const context = setup();
    await completeHandshake(context);
    context.connection.setProbeVisible(true);
    const before = livenessPings(context).length;

    context.sockets.latest.deliver(makeEvent({ globalSequence: "1", eventId: eventIdFor("1") }));
    context.sockets.latest.deliver(caughtUp("1"));
    // 服务端发起的 ping：客户端只回 pong，不能用它证明客户端→服务端方向也通。
    context.sockets.latest.deliver({
      protocolVersion: 1,
      type: "control.ping",
      messageId: "eeeeeeee-1111-4111-8111-000000000002",
      connectionId: CONNECTION_ID,
      connectionSequence: "1",
      body: { nonce: "B".repeat(22) },
    });
    context.sockets.latest.deliver(
      makeCommandResult({ requestId: "4c4dafda-dd98-442e-8d55-252b75bac72d", status: "accepted" }),
    );

    expect(livenessPings(context)).toHaveLength(before);
    context.clock.advanceBy(LIVENESS_TIMEOUT_MS);
    expect(context.events.filter((event) => event.kind === "closed")).toHaveLength(1);
  });

  it("3 秒没有匹配 pong：恰好一条可重试 closed（liveness_timeout），socket 关闭回调不再补发", async () => {
    const context = setup();
    await completeHandshake(context);
    context.connection.setProbeVisible(true);

    // 假时钟按整秒步进：每一拍检查一次最早 ping 的年龄（1000 → 2000 → 3000ms）。
    context.clock.advanceBy(LIVENESS_PROBE_INTERVAL_MS);
    context.clock.advanceBy(LIVENESS_PROBE_INTERVAL_MS);
    expect(context.events.some((event) => event.kind === "closed")).toBe(false);

    context.clock.advanceBy(LIVENESS_PROBE_INTERVAL_MS);
    const closed = context.events.filter((event) => event.kind === "closed");
    expect(closed).toHaveLength(1);
    expect(closed[0]).toEqual({
      kind: "closed",
      code: CLOSE.serverUnavailable,
      retryable: true,
      detail: "liveness_timeout",
    });
    // 连接在本地被终止：不是靠 socket 的远端关闭事件。
    expect(context.sockets.latest.closedWith).toEqual({ code: CLOSE.serverUnavailable, reason: "liveness_timeout" });
    expect(context.clock.pendingTimers).toBe(0);

    // 内核随后投递的关闭事件不得再产生第二条 `closed`。
    context.sockets.latest.serverClose(CLOSE.serverUnavailable, "service_unavailable");
    expect(context.events.filter((event) => event.kind === "closed")).toHaveLength(1);
  });
});

describe("应用层存活探测：可见性", () => {
  it("隐藏时取消计时器并清空待确认 ping：长时间后台不判超时、不再发 ping", async () => {
    const context = setup();
    await completeHandshake(context);
    context.connection.setProbeVisible(true);
    const before = livenessPings(context).length;

    context.connection.setProbeVisible(false);
    expect(context.clock.pendingTimers).toBe(0);
    context.clock.advanceBy(10_000);

    expect(livenessPings(context)).toHaveLength(before);
    expect(context.events.some((event) => event.kind === "closed")).toBe(false);
  });

  it("恢复可见立即补发 ping，并从那一刻重新获得完整 3 秒宽限", async () => {
    const context = setup();
    await completeHandshake(context);
    context.connection.setProbeVisible(true);
    context.connection.setProbeVisible(false);
    context.clock.advanceBy(10_000);
    const before = livenessPings(context).length;

    context.connection.setProbeVisible(true);
    // 「立即」：不推进时钟就已经有一条新 ping。
    expect(livenessPings(context)).toHaveLength(before + 1);

    context.clock.advanceBy(LIVENESS_PROBE_INTERVAL_MS);
    context.clock.advanceBy(LIVENESS_PROBE_INTERVAL_MS);
    expect(context.events.some((event) => event.kind === "closed")).toBe(false);

    context.clock.advanceBy(LIVENESS_PROBE_INTERVAL_MS);
    expect(context.events.filter((event) => event.kind === "closed")).toHaveLength(1);
  });

  it("认证前与关闭后设置可见性都是安全的空操作", async () => {
    const context = setup();
    expect(() => context.connection.setProbeVisible(false)).not.toThrow();
    expect(() => context.connection.setProbeVisible(true)).not.toThrow();
    context.connection.close();
    expect(() => context.connection.setProbeVisible(true)).not.toThrow();
    context.clock.advanceBy(5_000);
    expect(livenessPings(context)).toHaveLength(0);
  });
});

describe("应用层存活探测：关闭与阻断", () => {
  it("主动 close 取消探测定时器，不再发 ping，也不产生 closed 事件", async () => {
    const context = setup();
    await completeHandshake(context);
    context.connection.setProbeVisible(true);
    const before = livenessPings(context).length;

    context.connection.close();
    expect(context.clock.pendingTimers).toBe(0);
    context.clock.advanceBy(5_000);

    expect(livenessPings(context)).toHaveLength(before);
    expect(context.events.some((event) => event.kind === "closed")).toBe(false);
  });

  it("阻断态取消探测定时器：不探测、不超时，只有 blocked", async () => {
    const context = setup();
    await completeHandshake(context);
    context.connection.setProbeVisible(true);
    const before = livenessPings(context).length;

    context.sockets.latest.deliver({
      protocolVersion: 1,
      type: "error",
      messageId: "dddddddd-1111-4111-8111-000000000003",
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
    expect(context.clock.pendingTimers).toBe(0);
    context.clock.advanceBy(5_000);

    expect(livenessPings(context)).toHaveLength(before);
    expect(context.events.some((event) => event.kind === "closed")).toBe(false);
    expect(context.events.filter((event) => event.kind === "blocked")).toHaveLength(1);
  });
});
