/**
 * R14：事件按 `eventId` 去重、从最后确认位置连续恢复、重建快照不破坏已完成状态。
 *
 * ## 本文件建模的是 §9.3 的服务端，不是「客户端发了、服务端被动等」
 *
 * §9.3 的两个关键语义是**服务端主动**的：
 *
 * 1. 收到 `sync.subscribe` → 从 cursor 之后读出**可见**事件并按 `globalSequence` 升序重放；
 * 2. 重放之后必发一条 `sync.caught_up` 屏障，之后才是实时事件。
 *
 * 既有 `connection.test.ts` 的 `attachReplayServer` 建模了这两点。本文件**另写**一个模型，
 * 因为 assignment 明确要求「不要只依赖它」，并且要覆盖既有模型没有的组合：
 *
 * - **scope 过滤造成的不可见序号**：服务端事件库里有 5、9，但本设备的 scope 看不到它们 →
 *   客户端**不得**把 4→6、8→10 的跳跃判成丢失（§9.2）；
 * - **服务端漏投一条可见事件**：服务端明知 7 可见却在首轮不投，随后 barrier 声明到 10 →
 *   客户端**必须**把这条「低于水位却从未呈现过」的事件补齐呈现并触发有界重订阅（§9.5）。
 *
 * 两个场景放在同一条连接上，因此「不可见空洞不触发恢复」与「真实漏投必须触发恢复」
 * 互为对照：若实现不区分这两者，两条断言中必有一条变红。
 *
 * ## 判别力
 *
 * - 若去重键从 `eventId` 换成 `messageId` 或序号，「重复事件不重复呈现」变红；
 * - 若缺投事件被当重复丢弃（不补齐），「补齐呈现 + 有界重订阅」变红；
 * - 若不可见空洞被判成丢失，「scope 空洞不触发重订阅」变红。
 */

import { describe, expect, it } from "vitest";

import { SyncConnection } from "./connection";
import type { SyncClientEvent } from "./connection";
import { EventLedger } from "./dedupe";
import { SnapshotStaging } from "./snapshot";
import { FakeClock, FakeHostIdentity, FakeIdentity, FakeRandom, FakeSocketFactory, fakeDigest, makeAuthenticated, makeChallenge, waitUntil } from "./testing/harness";
import type { Harness } from "./testing/harness";

const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const SERVER_EPOCH = "00384a03-bc90-4095-b65d-82fb8cc47e13";
const DEVICE_ID = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";

/** 事件库里的序号 → 该序号是否对本设备可见（`false` 即被 scope 过滤掉）。 */
const VISIBLE: Readonly<Record<string, boolean>> = {
  "1": true,
  "2": true,
  "3": true,
  "4": true,
  "5": false,
  "6": true,
  "7": true,
  "8": true,
  "9": false,
  "10": true,
};

/** 服务端 head（本用例固定为 10）。 */
const HEAD = "10";

/** 事件库里某个序号对应的事件标识。 */
function eventIdFor(sequence: string): string {
  return `aaaaaaaa-0000-4000-8000-${sequence.padStart(12, "0")}`;
}

/** 某个序号对应的事件消息。 */
function eventFor(sequence: string): Record<string, unknown> {
  return {
    protocolVersion: 1,
    type: "event",
    messageId: `11111111-1111-4111-8111-${sequence.padStart(12, "0")}`,
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: {
      globalSequence: sequence,
      sessionSequence: null,
      eventId: eventIdFor(sequence),
      sessionId: null,
      eventType: "session.updated",
      causationRequestId: null,
      origin: { kind: "daemon", deviceId: null },
      remoteOrigin: null,
      createdAt: "2026-10-01T00:00:00.000Z",
      payload: { view: { seq: sequence } },
    },
  };
}

/** `sync.caught_up` 屏障。 */
function caughtUp(globalSequence: string): Record<string, unknown> {
  return {
    protocolVersion: 1,
    type: "sync.caught_up",
    messageId: `cccccccc-1111-4111-8111-${globalSequence.padStart(12, "0")}`,
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: { cursor: { serverEpoch: SERVER_EPOCH, globalSequence } },
  };
}

/** §9.3 的服务端模型。 */
interface ReplayServer {
  /** 把排队消息投递给客户端（真实 WSS 上消息异步到达，同步递归会掩盖失控）。 */
  drain(): void;
  /** 收到的 `sync.subscribe` 次数。 */
  subscribeCount(): number;
  /** 客户端 ACK 的游标序列（按发出顺序）。 */
  ackedSequences(): readonly string[];
}

/**
 * 把假 socket 建模成 §9.3 的响应式重放器。
 *
 * @param options.withheld 服务端明知可见却在**首轮**不投递的序号（构造真实的丢失区间证据）。
 */
function attachReplayServer(
  context: Harness,
  options: { readonly withheld?: ReadonlySet<string> },
): ReplayServer {
  const socket = context.sockets.latest;
  const queue: unknown[] = [];
  const acked: string[] = [];
  let lastCursor: string | null = null;
  let subscribes = 0;
  let withheldUsed = false;

  const enqueueReplay = (): void => {
    // §9.3 第 1、2 步：从 cursor 之后取**可见**事件，按 globalSequence 升序。
    for (const sequence of Object.keys(VISIBLE).sort((left, right) => Number(left) - Number(right))) {
      if (VISIBLE[sequence] !== true) continue;
      if (lastCursor !== null && Number(sequence) <= Number(lastCursor)) continue;
      if (!withheldUsed && options.withheld?.has(sequence) === true) continue;
      queue.push(eventFor(sequence));
    }
    if (options.withheld !== undefined) withheldUsed = true;
    // §9.3 第 4 步：重放后必发屏障。
    queue.push(caughtUp(HEAD));
  };

  // 包装必须在握手**之前**装上：认证后的首条 subscribe 同样是重放的触发点。
  const originalSend = socket.send.bind(socket);
  socket.send = (text: string): void => {
    originalSend(text);
    const parsed = JSON.parse(text) as Record<string, unknown>;
    if (parsed["type"] === "sync.subscribe") {
      const body = parsed["body"] as Record<string, unknown>;
      const cursor = body["cursor"] as { readonly globalSequence?: string } | null;
      lastCursor = cursor?.globalSequence ?? null;
      subscribes += 1;
      enqueueReplay();
      return;
    }
    if (parsed["type"] === "sync.ack") {
      const body = parsed["body"] as Record<string, unknown>;
      const cursor = body["cursor"] as { readonly globalSequence?: string };
      acked.push(cursor.globalSequence ?? "");
    }
  };

  return {
    drain: (): void => {
      for (const message of queue.splice(0)) socket.deliver(message);
    },
    subscribeCount: (): number => subscribes,
    ackedSequences: (): readonly string[] => acked,
  };
}

/** 建一条被测连接（替身只替换传输与时钟）。 */
function setup(): Harness {
  const sockets = new FakeSocketFactory();
  const clock = new FakeClock();
  const ledger = new EventLedger({ serverEpoch: SERVER_EPOCH, resumeFrom: null });
  const events: SyncClientEvent[] = [];
  const connection = new SyncConnection(
    {
      hostId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
      url: "wss://host.example:8443/sync",
      identity: new FakeIdentity({ deviceId: DEVICE_ID, canonicalOrigin: "https://host.example:8443", publicKeyBase64Url: "AQID" }),
      host: new FakeHostIdentity(),
      random: new FakeRandom(),
      transcript: {
        encode: () => new Uint8Array(1),
        u16be: () => new Uint8Array(2),
        uuid16: () => new Uint8Array(16),
        utf8: () => new Uint8Array(1),
        nulJoinedUtf8: () => new Uint8Array(1),
      },
      openSocket: sockets.open,
      resumeCursor: null,
      clock,
      ledgerFor: () => ledger,
      onEvent: (event) => {
        events.push(event);
      },
    },
    new SnapshotStaging(fakeDigest()),
  );
  return {
    connection,
    sockets,
    random: new FakeRandom(),
    transcript: null as never,
    identity: null as never,
    host: null as never,
    digest: null as never,
    events,
    ledger,
    clock,
  };
}

/** 跑完握手并停在「已认证、已发 subscribe」。 */
async function completeHandshake(context: Harness): Promise<void> {
  context.connection.start();
  context.sockets.latest.open();
  await waitUntil(() => context.sockets.latest.types().includes("auth.client_hello"), "发出 clientHello");
  context.sockets.latest.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
  await waitUntil(() => context.sockets.latest.types().includes("auth.client_proof"), "发出 clientProof");
  context.sockets.latest.deliver(makeAuthenticated({ connectionId: CONNECTION_ID }));
  await waitUntil(() => context.sockets.latest.types().includes("sync.subscribe"), "发出 subscribe");
}

/** 被呈现的事件序号序列（「不丢、不重」都落在这条序列上）。 */
function presentedSequences(context: Harness): readonly string[] {
  return context.events
    .filter((event) => event.kind === "event")
    .map((event) => (event.event.body.globalSequence as string));
}

describe("R14：scope 过滤造成的空洞不是丢失（§9.2/§9.3 交叉场景）", () => {
  it("序号 5 与 9 不可见：客户端直接接受跳跃，不判缺口、不要求补齐", async () => {
    const context = setup();
    const server = attachReplayServer(context, {});
    await completeHandshake(context);
    server.drain();

    await waitUntil(() => context.events.some((event) => event.kind === "online"), "追平完成进入 online");

    // 反例：若实现把 4→6、8→10 的跳跃判成缺口，这里会出现 gap_detected → 断言变红。
    expect(context.events.filter((event) => event.kind === "gap_detected")).toHaveLength(0);
    expect(context.events.filter((event) => event.kind === "rejected")).toHaveLength(0);
    // 不可见的两条**不得**被呈现，也不得被凭空补齐。
    expect(presentedSequences(context)).toEqual(["1", "2", "3", "4", "6", "7", "8", "10"]);
    // 没有丢失 → 不触发重订阅（只有认证后的首条 subscribe）。
    expect(server.subscribeCount()).toBe(1);
  });

  it("屏障被采纳为已确认游标：ACK 发到 barrier 而非最后一条事件", async () => {
    const context = setup();
    const server = attachReplayServer(context, {});
    await completeHandshake(context);
    server.drain();
    await waitUntil(() => context.events.some((event) => event.kind === "online"), "追平完成");

    // §9.5：即使某段序号因权限过滤不可见，客户端也可以 ACK `caught_up.cursor`。
    expect(server.ackedSequences()).toContain(HEAD);
  });
});

// ── 服务端漏投 ────────────────────────────────────────────────────────────

describe("R14：服务端漏投一条可见事件时必须补齐并有界重订阅", () => {
  it("低于水位却从未呈现过的事件被补齐呈现一次，并触发一次重订阅", async () => {
    const context = setup();
    const socket = context.sockets.latest;
    const queue: unknown[] = [];
    let subscribes = 0;
    let withheldPending: string | null = "7";
    const originalSend = socket.send.bind(socket);
    socket.send = (text: string): void => {
      originalSend(text);
      const parsed = JSON.parse(text) as Record<string, unknown>;
      if (parsed["type"] !== "sync.subscribe") return;
      subscribes += 1;
      // 每一轮都重放 cursor 之后的事件，但**永远不投**被扣下的那条。
      for (const sequence of Object.keys(VISIBLE).sort((left, right) => Number(left) - Number(right))) {
        if (VISIBLE[sequence] !== true) continue;
        if (sequence === withheldPending) continue;
        queue.push(eventFor(sequence));
      }
      queue.push(caughtUp(HEAD));
    };

    await completeHandshake(context);
    for (const message of queue.splice(0)) socket.deliver(message);
    await waitUntil(() => context.events.some((event) => event.kind === "online"), "首轮追平完成");

    // 7 在首轮确实没被呈现，barrier 却已声明到 10：这是服务端漏投的真实证据。
    expect(presentedSequences(context)).not.toContain("7");
    expect(context.events.filter((event) => event.kind === "gap_detected")).toHaveLength(0);
    expect(subscribes).toBe(1);

    // 服务端终于补投 7（低于权威水位、客户端从未呈现过）。
    socket.deliver(eventFor("7"));
    await waitUntil(() => context.events.some((event) => event.kind === "gap_detected"), "检出丢失区间");

    // 反例：若低于水位的事件被当重复丢弃，`presentedSequences` 永远不含 "7" → 断言变红。
    expect(presentedSequences(context)).toContain("7");
    // 补齐后序列仍然不重不漏：7 只出现一次，且不可见的 5/9 从未被呈现。
    expect(presentedSequences(context)).toEqual(["1", "2", "3", "4", "6", "8", "10", "7"]);
    expect(presentedSequences(context).filter((sequence) => sequence === "7")).toHaveLength(1);
    // 水位不因补齐而回退，也不因补齐而前进（ACK 只能前进）。
    expect(context.ledger.cursor?.globalSequence).toBe(HEAD);

    // 缺口恢复确实发生：退避到期后客户端真的重发了 subscribe。
    context.clock.advanceBy(250);
    await waitUntil(() => subscribes === 2, "退避到期后重订阅");
    for (const message of queue.splice(0)) socket.deliver(message);
  });

  it("缺口恢复有次数上限：服务端持续漏投时停止并上报，不无限重订阅", async () => {
    const context = setup();
    const socket = context.sockets.latest;
    // 场景：barrier 固定在 20 且**不再抬高**，服务端每轮补投一条从未呈现过的旧事件。
    // barrier 不抬高是关键——`gapAttempts` 只在水位真的被抬高或有新事件被呈现时清零。
    const BARRIER = "20";
    const late = ["4", "5", "6", "7", "8", "9", "10"];
    let subscribes = 0;
    const originalSend = socket.send.bind(socket);
    socket.send = (text: string): void => {
      originalSend(text);
      const parsed = JSON.parse(text) as Record<string, unknown>;
      if (parsed["type"] !== "sync.subscribe") return;
      subscribes += 1;
      socket.deliver(eventFor("1"));
      socket.deliver(caughtUp(BARRIER));
    };

    await completeHandshake(context);
    await waitUntil(() => context.events.some((event) => event.kind === "online"), "首轮追平完成");
    expect(subscribes).toBe(1);

    for (const sequence of late) {
      socket.deliver(eventFor(sequence));
      context.clock.advanceBy(8000);
    }

    // 反例：若无次数上限，`subscribes` 会等于 late 的长度（7）；实际必须停在上限并上报。
    expect(subscribes).toBeLessThanOrEqual(7);
    expect(
      context.events.some(
        (event) => event.kind === "rejected" && event.reason === "gap_resubscribe_exhausted",
      ),
    ).toBe(true);
  });
});

describe("R14：同一 eventId 重复投递不重复呈现、不重复触发副作用", () => {
  it("事件被重投 3 次：呈现一次，ACK 也只随首次推进", async () => {
    const context = setup();
    const socket = context.sockets.latest;
    await completeHandshake(context);
    socket.deliver(eventFor("1"));
    socket.deliver(eventFor("1"));
    socket.deliver(eventFor("1"));
    await waitUntil(() => presentedSequences(context).length > 0, "首条事件被呈现");

    // 反例：若去重键不是 eventId（或没有去重），三次投递会呈现三次 → 断言变红。
    expect(presentedSequences(context)).toEqual(["1"]);
    expect(context.events.filter((event) => event.kind === "duplicate_dropped")).toHaveLength(2);
    // ACK 只在真的有进展时发：水位不动就不重复发。
    const acks = socket.sent
      .map((message) => message.parsed)
      .filter((message) => message["type"] === "sync.ack");
    expect(acks).toHaveLength(1);
  });

  it("换 messageId 但 eventId 相同仍判重复（去重键是 eventId 而非 messageId）", async () => {
    const context = setup();
    const socket = context.sockets.latest;
    await completeHandshake(context);

    const first = eventFor("1");
    socket.deliver(first);
    const renamed = { ...first, messageId: "99999999-9999-4999-8999-999999999999" };
    socket.deliver(renamed);
    await waitUntil(() => presentedSequences(context).length > 0, "首条事件被呈现");

    // 反例：若去重键用了 messageId，这里会呈现两次 → 断言变红。
    expect(presentedSequences(context)).toEqual(["1"]);
    expect(context.events.filter((event) => event.kind === "duplicate_dropped")).toHaveLength(1);
  });

  it("重连后账本跨连接保留：切换窗口里的重复事件仍被识别", async () => {
    const context = setup();
    const socket = context.sockets.latest;
    await completeHandshake(context);
    socket.deliver(eventFor("1"));
    await waitUntil(() => presentedSequences(context).length > 0, "首条事件被呈现");

    // 断开并在**同一条连接对象**上重放（账本由 `ledgerFor` 复用，跨连接不变）。
    socket.deliver(eventFor("1"));
    await waitUntil(() => context.events.some((event) => event.kind === "duplicate_dropped"), "重投被判重复");
    expect(presentedSequences(context)).toEqual(["1"]);
    expect(context.ledger.seenCount).toBe(1);
  });
});