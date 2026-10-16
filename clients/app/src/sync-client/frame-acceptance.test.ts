/**
 * 连接级回归：`origin.kind = "node"` 的合法事件必须走完
 * 「解码边界受理 → 账本提交 → 交给消费方呈现 → 发出覆盖它的 `sync.ack`」这条链路。
 *
 * ## 为什么在连接层而不是只测解码器
 *
 * 解码器受理只是必要条件。`docs/SYNC_PROTOCOL.md` §9.5 的 ACK 只在事件被账本提交后发出，
 * 而提交发生在 `SyncConnection` 的入站事件处理里（账本用已呈现事件序号的最大值给出游标）。
 * 因此「假 socket 上是否出现覆盖该事件 `globalSequence` 的 `sync.ack`」这一条断言覆盖的是
 * **整条链路**：帧被拒 → 不进账本 → 不发 ACK → 服务端按同一游标反复补发，客户端游标永久停滞。
 *
 * ## 判别力来源
 *
 * 把 `wire.ts` 的 `EVENT_ORIGIN_KINDS` 改回四值（丢掉 `node`）：这条帧被判 `field_type` 拒收，
 * 消费方收不到 `{kind: "event"}`、假 socket 上也拿不到 `sync.ack` → 本文件两条用例都变红
 * （第一条卡在「node 事件被呈现」、第二条卡在同一处）。两条用例因此互为对照：
 * 第二条另外证明「node 事件之后游标没有被卡住」，而不是只证明它自己被受理。
 */

import { describe, expect, it } from "vitest";

import { SyncClient } from "./client";
import { DECLARED_SUBPROTOCOLS } from "./connection";
import type { SyncClientEvent } from "./connection";
import type { EventMessage, Uuid } from "../protocol";
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
  makeEvent,
  waitUntil,
} from "./testing/harness";
import type { FakeSocket } from "./testing/harness";

const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
/** 与 `auth.authenticated` 同 epoch；账本按它建，跨 epoch 的游标不可比。 */
const SERVER_EPOCH = "00384a03-bc90-4095-b65d-82fb8cc47e13";

/** `origin.kind = "node"` 事件的节点身份（`fixtures/sync/v1/valid/event-node-origin.json` 的形状）。 */
const NODE_ID = "9c1e0a58-3b71-4c52-8f2a-6d5e7f8091a2";

const NODE_EVENT_SEQUENCE = "2370";
const NODE_EVENT_ID = "6c2d8b41-5e30-4a7f-9b18-2d4e6f8091a2";
const NEXT_EVENT_SEQUENCE = "2371";
const NEXT_EVENT_ID = "7d3e9c52-6f41-4b80-ac29-3e5f7081a2b3";

/** 门面的被测环境（命名类型，测试不通过 `ReturnType` 引用）。 */
interface ClientHarness {
  readonly client: SyncClient;
  readonly sockets: FakeSocketFactory;
  readonly events: SyncClientEvent[];
}

/** 建一个未连接的门面替身环境（与 `client.test.ts` 的装配一致）。 */
function setupClient(): ClientHarness {
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
    subprotocols: DECLARED_SUBPROTOCOLS,
    resumeCursor: null,
    clock: new FakeClock(),
    onEvent: (event) => {
      events.push(event);
    },
  });
  return { client, sockets, events };
}

/** 跑完握手并停在「已认证、已发 subscribe」。 */
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

/**
 * 一条 `origin.kind = "node"` 的合法事件：`nodeId` 在场且为 UUID、`deviceId` 为空。
 *
 * `makeEvent` 的 `origin` 固定为 `{kind: "daemon", deviceId: null}`（`testing/harness.ts` 不改），
 * 因此这里在 `makeEvent` 的返回结构上覆写 `origin`，其余字段沿用同一构造器。
 */
function nodeOriginEvent(input: {
  readonly globalSequence: string;
  readonly eventId: Uuid;
}): EventMessage {
  const base = makeEvent({ globalSequence: input.globalSequence, eventId: input.eventId });
  return {
    ...base,
    body: {
      ...base.body,
      origin: { kind: "node", deviceId: null, nodeId: NODE_ID },
    },
  };
}

/** 已交给消费方呈现的事件序号（按呈现顺序）。 */
function presentedSequences(context: ClientHarness): readonly string[] {
  return context.events
    .filter((event) => event.kind === "event")
    .map((event) => event.event.body.globalSequence);
}

/** 假 socket 上已发出的 `sync.ack` 的游标序号（按发出顺序）。 */
function ackedSequences(socket: FakeSocket): readonly string[] {
  const sequences: string[] = [];
  for (const message of socket.sent) {
    if (message.parsed["type"] !== "sync.ack") continue;
    const body = message.parsed["body"];
    if (typeof body !== "object" || body === null) continue;
    const cursor = (body as Record<string, unknown>)["cursor"];
    if (typeof cursor !== "object" || cursor === null) continue;
    const sequence = (cursor as Record<string, unknown>)["globalSequence"];
    if (typeof sequence === "string") sequences.push(sequence);
  }
  return sequences;
}

describe("连接级：node 来源事件被受理、呈现并推进 ACK 游标（§9.5）", () => {
  it('origin.kind = "node"（含 nodeId、deviceId: null）的事件进入账本、交给消费方，并触发覆盖它的 sync.ack', async () => {
    const context = setupClient();
    await authenticate(context);
    const socket = context.sockets.latest;

    socket.deliver(nodeOriginEvent({ globalSequence: NODE_EVENT_SEQUENCE, eventId: NODE_EVENT_ID }));
    await waitUntil(
      () => presentedSequences(context).includes(NODE_EVENT_SEQUENCE),
      "node 事件被呈现",
    );

    // 反例：词表退回四值 → 这条帧被判 `field_type` 拒收，走到这里时上面的 `waitUntil` 已失败。
    expect(context.events.filter((event) => event.kind === "rejected")).toEqual([]);

    // 交给消费方的正是这条事件本身，且来源仍是 node（呈现路径不吞掉身份字段）。
    const delivered = context.events.filter((event) => event.kind === "event").map((event) => event.event);
    expect(delivered).toHaveLength(1);
    expect(delivered[0]?.body.origin).toEqual({ kind: "node", deviceId: null, nodeId: NODE_ID });

    // §9.5 的 ACK 只在账本提交后发出，游标取「已呈现事件序号的最大值」——因此正好覆盖该事件。
    expect(ackedSequences(socket)).toEqual([NODE_EVENT_SEQUENCE]);
  });

  it("node 事件之后的普通事件同样被呈现（游标没有被卡住）", async () => {
    const context = setupClient();
    await authenticate(context);
    const socket = context.sockets.latest;

    socket.deliver(nodeOriginEvent({ globalSequence: NODE_EVENT_SEQUENCE, eventId: NODE_EVENT_ID }));
    await waitUntil(
      () => presentedSequences(context).includes(NODE_EVENT_SEQUENCE),
      "node 事件被呈现",
    );

    socket.deliver(makeEvent({ globalSequence: NEXT_EVENT_SEQUENCE, eventId: NEXT_EVENT_ID }));
    await waitUntil(
      () => presentedSequences(context).includes(NEXT_EVENT_SEQUENCE),
      "其后的普通事件被呈现",
    );

    // 两条事件都按序呈现（node 事件若被丢弃，序列里就没有它）。
    expect(presentedSequences(context)).toEqual([NODE_EVENT_SEQUENCE, NEXT_EVENT_SEQUENCE]);
    expect(ackedSequences(socket)).toEqual([NODE_EVENT_SEQUENCE, NEXT_EVENT_SEQUENCE]);
  });
});
