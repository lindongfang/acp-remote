/**
 * P1-N1 / P1-N2：快照屏障的采纳裁决，以及恢复点的**不可污染性**。
 *
 * ## 缺陷链（MU3c 随主分支带入）
 *
 * `#onSnapshotEnd` 原先先 `emit snapshot_verified`、后 `ledger.advanceTo(barrier)`。账本拒绝
 * 该屏障（跨 epoch 或序号越界）时连接层正确地不采纳，但门面 `SyncClient` 已经**无条件**把
 * `event.snapshot.cursor` 写进了 `#ackedCursor`。这条被污染的游标在下次连接时成为
 * `resumeFrom`，`new EventLedger({resumeFrom})` 的 `parseSequence` 返回
 * `sequence_out_of_range` 并在**构造期**抛错；抛错点在 `#onAuthenticated` 内、由 socket 的
 * `onMessage` 同步调用，异常因此逃出回调，连接再也建立不起来。
 *
 * ## 判别力来源（每条都写成「反例会红」）
 *
 * - 「先 emit 后判定」回退 → 用例 A 的 `snapshot_verified` 断言与 `ackedCursor` 不变断言同时变红；
 * - 门面去掉幂等守卫（只保留连接层的重排）→ 用例 B 的恢复点断言变红；
 * - 「越界游标无害」的错觉 → 用例 C 的前半个用例直接证明构造期确实抛错（前提被实测，不是推测），
 *   后半个用例证明重连不再抛错且 `sync.subscribe` 带的是**上一次的有效游标**；
 * - 断言被「全部拒绝」的平凡实现蒙混 → 每个用例都配一条**合法**屏障的对照，证明确有交付发生。
 */

import { describe, expect, it } from "vitest";

import { SyncClient } from "./client";
import { SyncConnection } from "./connection";
import type { SyncClientEvent } from "./connection";
import { EventLedger } from "./dedupe";
import { SnapshotStaging } from "./snapshot";
import type { Cursor, Uuid } from "../protocol";
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
  makeSnapshotBegin,
  makeSnapshotChunk,
  makeSnapshotEnd,
  waitUntil,
  type FakeSocket,
} from "./testing/harness";

const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const SERVER_EPOCH = "00384a03-bc90-4095-b65d-82fb8cc47e13";
/** 服务端事件库重建后的另一个 epoch：序号与本 epoch 不可比。 */
const FOREIGN_EPOCH = "99999999-8888-4777-8666-555555555555";
const GOOD_SNAPSHOT_ID = "8194de43-e213-423d-acf4-2e3549304566";
const POISONED_SNAPSHOT_ID = "5b0d4c1e-6d4a-4a1e-9c3d-2f7b6c8d9e01";

/** 快照 chunks 的固定内容（只用于让 digest 有输入）。 */
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

/** 会话数不同的第二份快照内容：用于断言「仓库里仍然是上一次完成的那一份」。 */
const OTHER_SESSIONS_ITEM = [
  {
    ...SESSIONS_ITEM[0]!,
    sessionId: "6b62cd10-a465-43cd-b3f1-704e2d49e99f",
    title: "被污染的那次同步",
  },
];

/** 交付一份单 chunk 快照（begin/chunk/end 一气呵成）。 */
function deliverSnapshot(
  socket: FakeSocket,
  input: {
    readonly snapshotId: Uuid;
    readonly cursor: Cursor;
    readonly items: readonly unknown[];
  },
): void {
  socket.deliver(makeSnapshotBegin({ snapshotId: input.snapshotId, chunkCount: 1, cursor: input.cursor }));
  const chunk = makeSnapshotChunk({
    snapshotId: input.snapshotId,
    chunkIndex: "0",
    resource: "sessions",
    items: input.items,
  });
  socket.deliver(chunk.rawText);
  socket.deliver(
    makeSnapshotEnd({
      snapshotId: input.snapshotId,
      chunkCount: 1,
      digest: expectedSnapshotDigest([chunk.rawText]),
      cursor: input.cursor,
    }),
  );
}

/** 两种「账本必然拒绝」的屏障：跨 epoch 的外来游标、越界序号。 */
const REJECTED_BARRIERS: readonly {
  readonly label: string;
  readonly cursor: Cursor;
  readonly reason: string;
}[] = [
  {
    label: "跨 epoch 的外来游标",
    cursor: { serverEpoch: FOREIGN_EPOCH, globalSequence: "10" },
    reason: "snapshot_barrier_epoch_mismatch",
  },
  {
    label: "越界序号（超过 2^53-1）",
    cursor: { serverEpoch: SERVER_EPOCH, globalSequence: "99999999999999999999999999" },
    reason: "snapshot_barrier_sequence_out_of_range",
  },
];

/** 一条被测连接（连接层用例用）。 */
function setupConnection(resumeCursor: Cursor | null): {
  readonly connection: SyncConnection;
  readonly sockets: FakeSocketFactory;
  readonly events: SyncClientEvent[];
} {
  const sockets = new FakeSocketFactory();
  const events: SyncClientEvent[] = [];
  const ledger = new EventLedger({ serverEpoch: SERVER_EPOCH, resumeFrom: resumeCursor });
  const connection = new SyncConnection(
    {
      hostId: HOST_ID,
      url: "wss://host.example:8443/sync",
      identity: new FakeIdentity(),
      host: new FakeHostIdentity(),
      random: new FakeRandom(),
      transcript: new FakeTranscript(),
      openSocket: sockets.open,
      resumeCursor,
      clock: new FakeClock(),
      ledgerFor: () => ledger,
      onEvent: (event) => {
        events.push(event);
      },
    },
    new SnapshotStaging(fakeDigest()),
  );
  return { connection, sockets, events };
}

/** 门面的被测环境。 */
function setupClient(input: { readonly resumeCursor?: Cursor | null } = {}): {
  readonly client: SyncClient;
  readonly sockets: FakeSocketFactory;
  readonly events: SyncClientEvent[];
} {
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
    resumeCursor: input.resumeCursor ?? null,
    clock: new FakeClock(),
    nowMs: () => 1_700_000_000_000,
    onEvent: (event) => {
      events.push(event);
    },
  });
  return { client, sockets, events };
}

/** 跑完握手并停在「已认证、已发 subscribe」。 */
async function authenticate(
  context: { readonly client: SyncClient; readonly sockets: FakeSocketFactory },
): Promise<void> {
  context.client.connect();
  const socket = context.sockets.latest;
  socket.open();
  await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
  socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
  await waitUntil(() => socket.types().includes("auth.client_proof"), "发出 clientProof");
  socket.deliver(makeAuthenticated({ connectionId: CONNECTION_ID, serverEpoch: SERVER_EPOCH }));
  await waitUntil(() => socket.types().includes("sync.subscribe"), "发出 subscribe");
}

describe("P1-N1：账本拒绝的快照屏障不得被交付", () => {
  it.each(REJECTED_BARRIERS)("$label 的屏障：不发出 snapshot_verified，已确认游标保持上一次的有效值", async (barrier) => {
    const resume: Cursor = { serverEpoch: SERVER_EPOCH, globalSequence: "5" };
    const context = setupConnection(resume);

    context.connection.start();
    const socket = context.sockets.latest;
    socket.open();
    await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
    socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
    await waitUntil(() => socket.types().includes("auth.client_proof"), "发出 clientProof");
    socket.deliver(makeAuthenticated({ connectionId: CONNECTION_ID, serverEpoch: SERVER_EPOCH }));

    deliverSnapshot(socket, {
      snapshotId: POISONED_SNAPSHOT_ID,
      cursor: barrier.cursor,
      items: OTHER_SESSIONS_ITEM,
    });
    await waitUntil(
      () => context.events.some((event) => event.kind === "rejected" && event.reason === barrier.reason),
      "账本拒绝该屏障",
    );

    // 反例：若仍先 emit 再判定，这里会出现一条 snapshot_verified，断言变红。
    expect(context.events.some((event) => event.kind === "snapshot_verified")).toBe(false);
    // 反例：若把外来/越界游标写进已确认游标，这里会变成 barrier.cursor.globalSequence，断言变红。
    expect(context.connection.ackedCursor).toEqual(resume);
  });

  it("对照：合法的快照屏障仍然交付并推进已确认游标（证明确有交付发生，而非一律拒绝）", async () => {
    const resume: Cursor = { serverEpoch: SERVER_EPOCH, globalSequence: "5" };
    const context = setupConnection(resume);

    context.connection.start();
    const socket = context.sockets.latest;
    socket.open();
    await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
    socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
    await waitUntil(() => socket.types().includes("auth.client_proof"), "发出 clientProof");
    socket.deliver(makeAuthenticated({ connectionId: CONNECTION_ID, serverEpoch: SERVER_EPOCH }));

    const barrier: Cursor = { serverEpoch: SERVER_EPOCH, globalSequence: "10" };
    deliverSnapshot(socket, { snapshotId: GOOD_SNAPSHOT_ID, cursor: barrier, items: SESSIONS_ITEM });
    await waitUntil(
      () => context.events.some((event) => event.kind === "snapshot_verified"),
      "合法快照被交付",
    );

    expect(context.connection.ackedCursor).toEqual(barrier);
  });
});

describe("P1-N1：门面不采纳账本拒绝的游标", () => {
  it.each(REJECTED_BARRIERS)("$label：恢复点与快照仓库都保持上一次的有效值", async (barrier) => {
    const context = setupClient();
    await authenticate(context);

    const good: Cursor = { serverEpoch: SERVER_EPOCH, globalSequence: "10" };
    deliverSnapshot(context.sockets.latest, {
      snapshotId: GOOD_SNAPSHOT_ID,
      cursor: good,
      items: SESSIONS_ITEM,
    });
    await waitUntil(() => context.client.snapshots.current?.snapshotId === GOOD_SNAPSHOT_ID, "第一份快照入仓");

    deliverSnapshot(context.sockets.latest, {
      snapshotId: POISONED_SNAPSHOT_ID,
      cursor: barrier.cursor,
      items: OTHER_SESSIONS_ITEM,
    });
    await waitUntil(
      () =>
        context.events.some(
          (event) => event.kind === "rejected" && event.reason.startsWith("snapshot_barrier_"),
        ),
      "屏障被账本拒绝",
    );

    // 反例：门面若无条件采纳 `#ackedCursor`，这里会变成 barrier.cursor，断言变红。
    expect(context.client.ackedCursor).toEqual(good);
    // 反例：仓库若被这份不可用快照覆盖，snapshotId 会变成 POISONED_SNAPSHOT_ID，断言变红。
    expect(context.client.snapshots.current?.snapshotId).toBe(GOOD_SNAPSHOT_ID);
  });

  it("对照：epoch 相同且序号更高（前进）的快照仍被采纳", async () => {
    const context = setupClient();
    await authenticate(context);

    const first: Cursor = { serverEpoch: SERVER_EPOCH, globalSequence: "10" };
    deliverSnapshot(context.sockets.latest, {
      snapshotId: GOOD_SNAPSHOT_ID,
      cursor: first,
      items: SESSIONS_ITEM,
    });
    await waitUntil(() => context.client.snapshots.current?.snapshotId === GOOD_SNAPSHOT_ID, "第一份快照入仓");

    const newer: Cursor = { serverEpoch: SERVER_EPOCH, globalSequence: "20" };
    deliverSnapshot(context.sockets.latest, {
      snapshotId: POISONED_SNAPSHOT_ID,
      cursor: newer,
      items: OTHER_SESSIONS_ITEM,
    });
    await waitUntil(() => context.client.snapshots.current?.snapshotId === POISONED_SNAPSHOT_ID, "第二份快照入仓");

    expect(context.client.ackedCursor).toEqual(newer);
  });
});

describe("P1-N2：恢复点不可被污染，否则每次重连都在构造期抛错", () => {
  it("前提实测：越界的 resumeFrom 在 EventLedger 构造期就抛错（污染为何致命）", () => {
    // 这条断言把「污染会卡死同步」从推断变成实测：构造期抛错，且异常类型是普通 Error。
    expect(() =>
      new EventLedger({
        serverEpoch: SERVER_EPOCH,
        resumeFrom: { serverEpoch: SERVER_EPOCH, globalSequence: "99999999999999999999999999" },
      }),
    ).toThrow(/sequence_out_of_range/);
  });

  it("前提实测：以被污染的 resumeCursor 重建门面后，认证在构造期抛错且异常逃出 socket 回调", async () => {
    const context = setupClient({ resumeCursor: { serverEpoch: SERVER_EPOCH, globalSequence: "99999999999999999999999999" } });
    context.client.connect();
    const socket = context.sockets.latest;
    socket.open();
    await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
    socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
    await waitUntil(() => socket.types().includes("auth.client_proof"), "发出 clientProof");

    // 账本尚不存在（首条连接），`ledgerFor` 因此用这个 resumeCursor 构造 EventLedger。
    // 抛错点由 socket 的 `onMessage` 同步调用，异常逃出回调 → 认证再也无法完成。
    expect(() => {
      socket.deliver(makeAuthenticated({ connectionId: CONNECTION_ID, serverEpoch: SERVER_EPOCH }));
    }).toThrow(/resumeFrom 的 globalSequence 不可用/);
    expect(socket.types()).not.toContain("sync.subscribe");
  });

  it.each(REJECTED_BARRIERS)(
    "经历一次被拒快照（$label）后，持久化的恢复点仍是上一次的有效值：用它重建门面能正常建立连接",
    async (barrier) => {
      const context = setupClient();
      await authenticate(context);

      const good: Cursor = { serverEpoch: SERVER_EPOCH, globalSequence: "10" };
      deliverSnapshot(context.sockets.latest, {
        snapshotId: GOOD_SNAPSHOT_ID,
        cursor: good,
        items: SESSIONS_ITEM,
      });
      await waitUntil(() => context.client.snapshots.current?.snapshotId === GOOD_SNAPSHOT_ID, "第一份快照入仓");

      // 一次畸形快照：digest 通过、但屏障被账本拒绝。
      deliverSnapshot(context.sockets.latest, {
        snapshotId: POISONED_SNAPSHOT_ID,
        cursor: barrier.cursor,
        items: OTHER_SESSIONS_ITEM,
      });
      await waitUntil(
        () => context.events.some((event) => event.kind === "rejected" && event.reason === barrier.reason),
        "屏障被账本拒绝",
      );

      // 组合根持久化的就是这个值：用它重建门面（即下一次启动/重连的真实路径）。
      const persisted = context.client.ackedCursor;
      const rebuilt = setupClient({ resumeCursor: persisted });
      rebuilt.client.connect();
      const socket = rebuilt.sockets.latest;
      socket.open();
      await waitUntil(() => socket.types().includes("auth.client_hello"), "重建后发出 clientHello");
      socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
      await waitUntil(() => socket.types().includes("auth.client_proof"), "重建后发出 clientProof");

      // 反例：若恢复点被污染（越界），这行 deliver 会抛 `resumeFrom 的 globalSequence 不可用`，
      // 用例变红；若被跨 epoch 外来游标覆盖，随后 subscribe 带的是 `cursor: null`，断言同样变红。
      expect(() => {
        socket.deliver(makeAuthenticated({ connectionId: CONNECTION_ID, serverEpoch: SERVER_EPOCH }));
      }).not.toThrow();
      await waitUntil(() => socket.types().includes("sync.subscribe"), "重建后发出 subscribe");
      expect(socket.last("sync.subscribe")?.["body"]).toMatchObject({ cursor: good });
    },
  );
});
