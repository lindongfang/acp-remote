/**
 * CRITICAL-1：服务端事件库重建后，**新 epoch 的合法快照必须入仓**。
 *
 * ## 缺陷链（`221e76e` 引入的回归）
 *
 * 门面的 `#isAdoptableBarrier` 以 `#ackedCursor` 为基准判断「同 epoch」。但 `#ackedCursor`
 * 在 epoch 变化时**不会被清空**——清空只发生在连接层的 `#acked`（`connection.ts`）与门面的
 * `#ledgerFor`（后者当时只把 `resumeFrom` 置 null，不动恢复点本身）。于是失效链是：
 *
 * 1. epoch A 下 `#ackedCursor = {A, "10"}`；
 * 2. 服务端事件库重建，重连后 `auth.authenticated` 带 epoch B：连接层正确清 `#acked`，
 *    门面的 `#ledgerFor` 也用 `resumeFrom = null` 建了新账本；
 * 3. 服务端按 §9.4 补发快照，`snapshot_end` 的 cursor 是 `{B, "1"}`；
 * 4. **连接层判定通过**（新账本，`advanceTo` 接受），发出 `snapshot_verified`；
 * 5. 门面 `#isAdoptableBarrier({B,"1"})` 看到 `acked.serverEpoch (A) !== B` → 返回 `false`
 *    → 快照既不入仓也不推进恢复点；
 * 6. 后续增量事件会把 `#ackedCursor` 抬到 B，但**来不及了**——重建快照是一次性的。
 *
 * 结果：仓库停在 epoch A 的旧资源上，事件流在 epoch B 上继续推进，**永久不一致且无错误呈现**。
 * 这与被修的 P1-N1/N2 同型（永久卡死），触发条件换成了 §9.4 的正常恢复流程。
 *
 * ## 判别力来源
 *
 * - 修法是「换 epoch 时恢复点一起丢掉」（`#ledgerFor`），不是「一律采纳」也不是「一律拒绝」：
 *   同文件里两条负向用例证明账本拒绝的屏障仍不被采纳，两条正向用例证明重建后的合法快照
 *   被采纳且恢复点前进。四个方向缺一不可。
 * - 修复前：正向用例的「仓库拿到新快照」断言变红（连接层已交付、门面丢弃）。
 * - 把修复换成「一律采纳」：负向用例的「恢复点保持上一次的有效值」断言变红。
 * - 把修复换成「一律拒绝」：正向用例同样变红——这正是「平凡实现也会全绿」的陷阱。
 */

import { describe, expect, it } from "vitest";

import { SyncClient } from "./client";
import type { SyncClientEvent } from "./connection";
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
/** epoch A：事件库尚未重建。 */
const EPOCH_A = "00384a03-bc90-4095-b65d-82fb8cc47e13";
/** epoch B：服务端事件库重建后的新 epoch（§9.4）。 */
const EPOCH_B = "11111111-2222-4333-8444-555555555555";
/** 外来 epoch：既不是 A 也不是 B，任何时刻都不是本连接的账本 epoch。 */
const FOREIGN_EPOCH = "99999999-8888-4777-8666-555555555555";

/** 重建前的快照。 */
const BEFORE_REBUILD_SNAPSHOT_ID = "8194de43-e213-423d-acf4-2e3549304566";
/** 重建后的快照。 */
const REBUILT_SNAPSHOT_ID = "5b0d4c1e-6d4a-4a1e-9c3d-2f7b6c8d9e01";

/** 重建前的会话（标题可断言，用来证明仓库确实换了）。 */
const BEFORE_REBUILD_SESSION = [
  {
    sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
    title: "重建前的标题",
    agent: { agentId: "claude", name: "Claude" },
    state: "idle",
    origin: { kind: "local" },
    currentMode: null,
    version: "1",
    createdAt: "2026-10-01T00:00:00.000Z",
    updatedAt: "2026-10-01T00:00:00.000Z",
  },
];

/** 重建后的会话。 */
const REBUILT_SESSION = [{ ...BEFORE_REBUILD_SESSION[0]!, title: "重建后的标题" }];

/** 门面的被测环境。 */
interface ClientHarness {
  readonly client: SyncClient;
  readonly sockets: FakeSocketFactory;
  readonly events: SyncClientEvent[];
}

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
    resumeCursor: null,
    clock: new FakeClock(),
    nowMs: () => 1_700_000_000_000,
    onEvent: (event) => {
      events.push(event);
    },
  });
  return { client, sockets, events };
}

/** 跑完握手并停在「已认证、已发 subscribe」；`serverEpoch` 决定本次连接绑定的账本。 */
async function authenticateAt(context: ClientHarness, serverEpoch: Uuid): Promise<void> {
  context.client.connect();
  const socket = context.sockets.latest;
  socket.open();
  await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
  socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
  await waitUntil(() => socket.types().includes("auth.client_proof"), "发出 clientProof");
  socket.deliver(makeAuthenticated({ connectionId: CONNECTION_ID, serverEpoch }));
  await waitUntil(() => socket.types().includes("sync.subscribe"), "发出 subscribe");
}

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

describe("CRITICAL-1：epoch 变化后合法的新快照必须入仓", () => {
  it("服务端事件库重建：新 epoch 的快照替换仓库，恢复点前进到新 epoch", async () => {
    const context = setupClient();
    await authenticateAt(context, EPOCH_A);

    const before: Cursor = { serverEpoch: EPOCH_A, globalSequence: "10" };
    deliverSnapshot(context.sockets.latest, {
      snapshotId: BEFORE_REBUILD_SNAPSHOT_ID,
      cursor: before,
      items: BEFORE_REBUILD_SESSION,
    });
    await waitUntil(
      () => context.client.snapshots.current?.snapshotId === BEFORE_REBUILD_SNAPSHOT_ID,
      "重建前的快照入仓",
    );

    // 服务端异常断开 → 事件库重建 → 重连拿到 epoch B。
    context.sockets.latest.serverClose(1006, "server_restarted");
    await authenticateAt(context, EPOCH_B);

    const rebuilt: Cursor = { serverEpoch: EPOCH_B, globalSequence: "1" };
    deliverSnapshot(context.sockets.latest, {
      snapshotId: REBUILT_SNAPSHOT_ID,
      cursor: rebuilt,
      items: REBUILT_SESSION,
    });
    await waitUntil(
      () => context.client.snapshots.current?.snapshotId === REBUILT_SNAPSHOT_ID,
      "重建后的快照入仓",
    );

    // 反例（回归点）：门面若仍以旧 epoch 的恢复点为基准，这里会一直停在重建前 → 变红。
    expect(context.client.snapshots.current?.resources.sessions[0]?.title).toBe("重建后的标题");
    // 恢复点必须一并前进：它会被组合根持久化并成为下次连接的 resumeFrom。
    expect(context.client.ackedCursor).toEqual(rebuilt);
  });

  it("后续增量事件到来也救不回被丢弃的快照（证明上条不是靠事件推进侥幸通过的）", async () => {
    const context = setupClient();
    await authenticateAt(context, EPOCH_A);
    deliverSnapshot(context.sockets.latest, {
      snapshotId: BEFORE_REBUILD_SNAPSHOT_ID,
      cursor: { serverEpoch: EPOCH_A, globalSequence: "10" },
      items: BEFORE_REBUILD_SESSION,
    });
    await waitUntil(
      () => context.client.snapshots.current?.snapshotId === BEFORE_REBUILD_SNAPSHOT_ID,
      "重建前的快照入仓",
    );

    context.sockets.latest.serverClose(1006, "server_restarted");
    await authenticateAt(context, EPOCH_B);
    deliverSnapshot(context.sockets.latest, {
      snapshotId: REBUILT_SNAPSHOT_ID,
      cursor: { serverEpoch: EPOCH_B, globalSequence: "1" },
      items: REBUILT_SESSION,
    });
    await waitUntil(
      () => context.client.snapshots.current?.snapshotId === REBUILT_SNAPSHOT_ID,
      "重建后的快照入仓",
    );

    // 走一步「追平」，让恢复点被 `online` 事件抬到新 epoch 的更高序号。
    context.sockets.latest.deliver({
      protocolVersion: 1,
      type: "sync.caught_up",
      messageId: "6b1f9d70-0000-4000-8000-00000000ffff",
      connectionId: CONNECTION_ID,
      connectionSequence: "9",
      body: { cursor: { serverEpoch: EPOCH_B, globalSequence: "7" } },
    });
    await waitUntil(() => context.client.ackedCursor?.globalSequence === "7", "恢复点被抬到新 epoch");

    // 仓库仍然是重建后的那一份，而不是被增量事件改回旧内容。
    expect(context.client.snapshots.current?.resources.sessions[0]?.title).toBe("重建后的标题");
  });
});

describe("CRITICAL-1：修法不得退化为「一律采纳」或「一律拒绝」", () => {
  it("外来 epoch 的屏障仍不被采纳（连接层已拒，门面也不得替换仓库）", async () => {
    const context = setupClient();
    await authenticateAt(context, EPOCH_A);
    deliverSnapshot(context.sockets.latest, {
      snapshotId: BEFORE_REBUILD_SNAPSHOT_ID,
      cursor: { serverEpoch: EPOCH_A, globalSequence: "10" },
      items: BEFORE_REBUILD_SESSION,
    });
    await waitUntil(
      () => context.client.snapshots.current?.snapshotId === BEFORE_REBUILD_SNAPSHOT_ID,
      "第一份快照入仓",
    );

    deliverSnapshot(context.sockets.latest, {
      snapshotId: REBUILT_SNAPSHOT_ID,
      cursor: { serverEpoch: FOREIGN_EPOCH, globalSequence: "10" },
      items: REBUILT_SESSION,
    });
    await waitUntil(
      () => context.events.some((event) => event.kind === "rejected" && event.reason === "snapshot_barrier_epoch_mismatch"),
      "账本拒绝该屏障",
    );
    // 反例（回归点）：门面若改成「一律采纳」，这里会变成 REBUILT_SNAPSHOT_ID → 变红。
    expect(context.client.snapshots.current?.snapshotId).toBe(BEFORE_REBUILD_SNAPSHOT_ID);
    expect(context.client.ackedCursor).toEqual({ serverEpoch: EPOCH_A, globalSequence: "10" });
  });

  it("同 epoch 且序号更旧的快照：连接层交付了它，门面必须拒（守卫的直接判别力）", async () => {
    const context = setupClient();
    await authenticateAt(context, EPOCH_A);
    deliverSnapshot(context.sockets.latest, {
      snapshotId: BEFORE_REBUILD_SNAPSHOT_ID,
      cursor: { serverEpoch: EPOCH_A, globalSequence: "20" },
      items: BEFORE_REBUILD_SESSION,
    });
    await waitUntil(
      () => context.client.snapshots.current?.snapshotId === BEFORE_REBUILD_SNAPSHOT_ID,
      "第一份快照入仓",
    );

    deliverSnapshot(context.sockets.latest, {
      snapshotId: REBUILT_SNAPSHOT_ID,
      cursor: { serverEpoch: EPOCH_A, globalSequence: "5" },
      items: REBUILT_SESSION,
    });
    // `advanceTo` 对更旧的屏障返回 `already_covered`（不是 `rejected`），因此连接层**会**把它
    // 交付给门面。这条路径是门面上那道冗余守卫唯一的用武之地：若把用例写成「连接层已拒」，
    // 它就退化成对连接层的断言，守卫本身零判别力（`review-wp7-r1` §7 变异三的教训）。
    await waitUntil(
      () =>
        context.events.some(
          (event) => event.kind === "snapshot_verified" && event.snapshot.snapshotId === REBUILT_SNAPSHOT_ID,
        ),
      "连接层交付了这条更旧的快照",
    );

    // 反例：若门面改成「一律采纳」，这里会变成 REBUILT_SNAPSHOT_ID → 变红。
    expect(context.client.snapshots.current?.snapshotId).toBe(BEFORE_REBUILD_SNAPSHOT_ID);
    // 恢复点同样不得回退：它会成为下次连接的 `resumeFrom`，回退即永久丢事件。
    expect(context.client.ackedCursor).toEqual({ serverEpoch: EPOCH_A, globalSequence: "20" });
  });

  it("epoch 不变时新快照照常替换仓库（修法没有把同 epoch 的正常同步也拒掉）", async () => {
    const context = setupClient();
    await authenticateAt(context, EPOCH_A);
    deliverSnapshot(context.sockets.latest, {
      snapshotId: BEFORE_REBUILD_SNAPSHOT_ID,
      cursor: { serverEpoch: EPOCH_A, globalSequence: "10" },
      items: BEFORE_REBUILD_SESSION,
    });
    await waitUntil(
      () => context.client.snapshots.current?.snapshotId === BEFORE_REBUILD_SNAPSHOT_ID,
      "第一份快照入仓",
    );

    deliverSnapshot(context.sockets.latest, {
      snapshotId: REBUILT_SNAPSHOT_ID,
      cursor: { serverEpoch: EPOCH_A, globalSequence: "30" },
      items: REBUILT_SESSION,
    });
    await waitUntil(
      () => context.client.snapshots.current?.snapshotId === REBUILT_SNAPSHOT_ID,
      "第二份快照入仓",
    );

    expect(context.client.ackedCursor).toEqual({ serverEpoch: EPOCH_A, globalSequence: "30" });
  });
});