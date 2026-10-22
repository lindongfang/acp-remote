/**
 * `SyncClient`：连接、命令派发与快照仓库的门面。
 *
 * ## 这一层负责什么
 *
 * - 持有**去重账本**与**快照仓库**（最后一次已完成状态），跨连接复用：
 *   账本跨连接保留才能识别 §8.5 切换窗口里的重复投递；快照仓库跨连接保留才能在
 *   `sync.reset_required` 时保持「上一次已完成状态」。
 * - **转发** `SyncClientEvent`：本层原样交给消费方，由组合根（WP7）翻译成连接状态机事件，
   并把阻断态落到底层 `stopAutoReconnect` 上——**状态机是唯一判定源**，门面不自己判断该不该重连。
 * - 跨连接持有**已确认游标**：重连时用它订阅并给账本定位续传起点。
 * - 以稳定 `requestId` 派发命令；重连后重试**复用**同一 ID，或改用 `command.status` 查询。
 *
 * ## 快照替换为什么是「先验证、后一次替换」
 *
 * `snapshot_verified` 事件只在 digest、数量、cursor 全部通过后发出；消费方收到它才替换
 * 仓库里的一次完成状态。验证失败时本层不发出该事件，仓库因此保持原样——
 * 这正是 spec 场景「重建快照时不破坏已完成状态」的结构性保证。
 */

import type { CommandMessage, Cursor, OriginCursor, SessionPreparationBody, SessionRef, Uuid } from "../protocol";
import { SyncConnection, type AuthenticatedInfo, type ClientBlockCause, type SyncClientEvent } from "./connection";
import { compareCursors } from "./connection";
import { advanceOriginCursor } from "./remote-merge";
import { EventLedger } from "./dedupe";
import { SnapshotStaging } from "./snapshot";
import type { VerifiedSnapshot } from "./snapshot";
import type { ClockPort, DigestPort, RandomPort, SocketFactory, TranscriptCodec, DeviceIdentityLike, HostIdentityPort } from "./ports";

/** 已完成（可离线渲染）的快照状态。 */
export interface CompletedSnapshot {
  readonly snapshotId: Uuid;
  readonly cursor: Cursor;
  readonly resources: VerifiedSnapshot["resources"];
  readonly verifiedAtMs: number;
}

/** 快照仓库：只保留最后一次**已完成**状态。 */
export class SnapshotRepository {
  #current: CompletedSnapshot | null = null;

  /** 当前已完成状态；从未验证通过过时为 `null`。 */
  get current(): CompletedSnapshot | null {
    return this.#current;
  }

  /**
   * 原子替换。
   *
   * 只有验证通过的快照才走到这里，因此「验证失败保留旧状态」不需要额外的分支。
   */
  replace(snapshot: VerifiedSnapshot, nowMs: number): void {
    this.#current = {
      snapshotId: snapshot.snapshotId,
      cursor: snapshot.cursor,
      resources: snapshot.resources,
      verifiedAtMs: nowMs,
    };
  }
}

/** 命令派发登记：稳定 `requestId` 与重试次数。 */
export interface DispatchRecord {
  readonly requestId: Uuid;
  readonly command: string;
  /** 已派发次数（含重试）。重试**不**产生新的 `requestId`。 */
  readonly dispatchCount: number;
  /** 是否已收到服务端确认（`accepted` 或终态）。 */
  readonly acknowledged: boolean;
}

/** `SyncClient` 的构造输入。 */
export interface SyncClientOptions {
  readonly hostId: Uuid;
  readonly url: string;
  /** 升级请求里声明的子协议列表（由组合根给出，见 `DECLARED_SUBPROTOCOLS`）。 */
  readonly subprotocols: readonly string[];
  readonly identity: DeviceIdentityLike;
  readonly host: HostIdentityPort;
  readonly random: RandomPort;
  readonly transcript: TranscriptCodec;
  readonly digest: DigestPort;
  /** 建立 socket 的工厂（Web 实现见 `socket.web.ts`；端口形状见 `SocketFactory`）。 */
  readonly openSocket: SocketFactory;
  /** 最后一次持久 ACK 的游标；首次同步为 `null`。 */
  readonly resumeCursor: Cursor | null;
  /** 事件的唯一消费者（`src/state/` 的连接机与后续的 feature 层）。 */
  readonly onEvent: (event: SyncClientEvent) => void;
  /**
   * 时钟端口：快照时间戳与缺口恢复的退避定时。
   *
   * 只保留**一个**时间来源（§8.5 的退避接缝）：`nowMs` 不作为独立入口要求组合根提供第二个时钟，
   * 缺省时委托 `clock.now()`——两套时钟若来自不同时源，快照时间戳与退避就会给出互相矛盾的诊断。
   */
  readonly clock: ClockPort;
  /** 覆盖时钟读数（测试与「时间戳不来自时钟」的场景可显式注入）；缺省用 `clock.now()`。 */
  readonly nowMs?: () => number;
}

/**
 * 一个来源会话的运行期状态（`design.md` D3/D4）。
 *
 * 它**只活在当前进程**：`cursor` 是最后连续处理位置 C，仅用于回源重放衔接与去重；页面刷新后
 * 它随实例一起消失，下一次打开会话从 `null` 订阅并先回源读取重建。`seen` 是当前运行期的来源
 * 事件去重窗口——跨连接保留，避免切换窗口里的重投把同一条事件拼接两次。
 */
interface RemoteRuntime {
  readonly sessionRef: SessionRef;
  /** 用户/页面仍希望保持订阅（切换会话或断线时只置假，不丢已处理位置）。 */
  wanted: boolean;
  /** 当前连接上的订阅 ID；连接关闭或订阅被替换后为 `null`。 */
  subscriptionId: Uuid | null;
  cursor: OriginCursor | null;
  caughtUp: boolean;
  readonly seen: Set<string>;
  readonly seenOrder: string[];
}

/** 来源事件去重窗口大小（与去重账本同量级，避免长时间打开会话无界增长）。 */
const REMOTE_SEEN_LIMIT = 2000;

/**
 * 同步客户端门面。
 *
 * 一个实例覆盖整个应用生命周期：连接断开后由 `src/state/` 的状态机决定何时重新调用
 * `connect()`，本类**不自己**做重连定时——退避策略与阻断判定属于状态层。
 */
export class SyncClient {
  readonly #options: SyncClientOptions;
  readonly #snapshots = new SnapshotRepository();
  readonly #dispatch = new Map<Uuid, DispatchRecord>();
  #connection: SyncConnection | null = null;
  #snapshotRefreshNeeded = false;
  #snapshotRefreshInFlight = false;
  #snapshotRefreshAgain = false;
  #snapshotRefreshVerified = false;
  #snapshotRefreshCaughtUp = false;
  /** 跨连接复用的去重账本；服务端 epoch 变化时丢弃（必须重建快照）。 */
  #ledger: EventLedger | null = null;
  /** 来源会话的运行期订阅与连续位置（不落盘；键为 `owner|session`）。 */
  readonly #remote = new Map<string, RemoteRuntime>();
  /** 阻断后停止自动重连；`src/state/` 显式解除前不再接受 `connect()`。 */
  #blocked: ClientBlockCause | null = null;
  /**
   * 已确认游标（跨连接存活）。
   *
   * 它必须活在门面上而不是某个 `SyncConnection` 实例里：实例随连接消亡，而 R14 要求
   * 「从最后确认的位置连续恢复」——若每次重连都退回构造期的 `options.resumeCursor`，
   * 服务端就会从头重放整段历史。
   */
  #ackedCursor: Cursor | null;
  readonly #now: () => number;
  /**
   * 期望的存活探测可见性（跨连接保存）。
   *
   * 它必须活在门面上：重连时新建的 `SyncConnection` 还没有认证，组合根也不一定再次收到
   * 可见性事件；若只在连接实例上设置，重连后的第一条连接就会按「始终可见」探测，后台标签页
   * 也会持续发 ping。`connect()` 把这里的值应用到新连接。
   *
   * 缺省 `false`：直接构造 `SyncClient` 的测试/消费者不自动产生 ping；组合根在装配时
   * 按 `LifecyclePort.currentVisibility()` 显式打开。
   */
  #probeVisible = false;

  constructor(options: SyncClientOptions) {
    this.#options = options;
    this.#now = options.nowMs ?? ((): number => options.clock.now());
    this.#ackedCursor = options.resumeCursor;
  }

  /** 快照仓库（离线渲染目录页的唯一数据来源）。 */
  get snapshots(): SnapshotRepository {
    return this.#snapshots;
  }

  /** 当前阻断原因；`null` 表示未阻断。 */
  get blocked(): ClientBlockCause | null {
    return this.#blocked;
  }

  /**
   * 是否有一条**活着**的连接（socket 尚未关闭）。
   *
   * 判定问的是 socket 而不是「是否持有连接对象」：后者在服务端关闭之后仍为真，
   * 组合根会据此以为还在线并跳过重连。
   */
  get connected(): boolean {
    return this.#connection !== null && !this.#connection.closed;
  }

  /** 已确认游标；重连时作为 `sync.subscribe` 的恢复点。 */
  get ackedCursor(): Cursor | null {
    return this.#ackedCursor;
  }

  /**
   * 设置应用层存活探测的可见性，并下发给当前连接（若有）。
   *
   * 没有连接时只记录期望值：连接建立后由 `connect()` 应用，认证完成后才真正开始探测。
   * 该方法是幂等的，且不改变任何命令/订阅行为。
   */
  setProbeVisible(visible: boolean): void {
    this.#probeVisible = visible;
    this.#connection?.setProbeVisible(visible);
  }

  /** 认证信息；未认证时为 `null`。 */
  get authenticatedInfo(): AuthenticatedInfo | null {
    return this.#connection?.authenticatedInfo ?? null;
  }

  /**
   * 当前连接的标识（`auth.authenticated.connectionId`）；**没有活着的已认证连接时为 `null`**。
   *
   * 组合根与 `ClientStore` 需要它来给命令信封填必填字段（`$defs.command`）。它此前只有私有的
   * `#connection?.connectionId`，于是 feature 层的 `ConnectionIdentity` 在真实运行时**永远**
   * 取不到值——创建会话的第一道闸门恒不过。
   *
   * 「活着的」这个限定不可省：连接实例随 socket 关闭后仍留在 `#connection` 上（重连才替换它），
   * 直接透传会让**已断开**的连接继续拿到标识，于是 `session.create` 能被派发到一条死连接上，
   * 而服务端根本收不到。§8.4 也明确「认证状态只属于当前 WSS 连接，关闭后立即失效」。
   * 只读暴露，不提供任何「连接不存在时编一个 id」的入口。
   */
  get connectionId(): Uuid | null {
    if (!this.connected) return null;
    return this.#connection?.connectionId ?? null;
  }

  /** 建立一条连接并完成逐连接握手。阻断状态下拒绝连接（spec：停止自动重连）。 */
  connect(): void {
    if (this.#blocked !== null) {
      throw new Error(`连接处于阻断态（${this.#blocked}），不得自动重连`);
    }
    this.disconnect();
    const connection = new SyncConnection(
      {
        hostId: this.#options.hostId,
        url: this.#options.url,
        subprotocols: this.#options.subprotocols,
        identity: this.#options.identity,
        host: this.#options.host,
        random: this.#options.random,
        transcript: this.#options.transcript,
        clock: this.#options.clock,
        openSocket: this.#options.openSocket,
        resumeCursor: this.#ackedCursor,
        ledgerFor: (serverEpoch) => this.#ledgerFor(serverEpoch),
        onEvent: (event) => {
          this.#onConnectionEvent(event);
        },
      },
      // 暂存器每次连接一份：断线即丢弃未完成快照（§9.4）。
      new SnapshotStaging(this.#options.digest),
    );
    this.#connection = connection;
    // 新连接继承门面保存的可见性；认证完成后 `#startProbe` 才真正生效。
    connection.setProbeVisible(this.#probeVisible);
    connection.start();
  }

  /** 关闭当前连接。 */
  disconnect(): void {
    this.#connection?.close();
    this.#connection = null;
    this.#snapshotRefreshInFlight = false;
    // 连接级订阅随 socket 失效；运行期连续位置与去重窗口保留，供重连后从 C 继续。
    for (const runtime of this.#remote.values()) {
      runtime.subscriptionId = null;
      runtime.caughtUp = false;
    }
  }

  /**
   * 为一条来源会话开启（或刷新）订阅（§9.7）。
   *
   * 每次调用都生成新的 `subscriptionId`：同一条连接上重新订阅必须让旧 ID 的迟到帧失效。
   * 若当前运行期仍持有最后连续位置 C，就用它请求来源增量重放；否则用 `null` 走首次重建。
   * **读取重建由消费方另行发起**——本方法只负责订阅，不把 C 当成完整视图的替代。
   */
  subscribeRemote(sessionRef: SessionRef): void {
    const key = this.#remoteKey(sessionRef);
    let runtime = this.#remote.get(key);
    if (runtime === undefined) {
      runtime = {
        sessionRef,
        wanted: true,
        subscriptionId: null,
        cursor: null,
        caughtUp: false,
        seen: new Set(),
        seenOrder: [],
      };
      this.#remote.set(key, runtime);
    }
    runtime.wanted = true;
    this.#sendRemoteSubscribe(runtime);
  }

  /** 取消订阅但保留运行期位置：再次打开同一会话仍可从 C 衔接（只用于回源重放，不是恢复视图）。 */
  unsubscribeRemote(sessionRef: SessionRef): void {
    const runtime = this.#remote.get(this.#remoteKey(sessionRef));
    if (runtime === undefined) return;
    runtime.wanted = false;
    const subscriptionId = runtime.subscriptionId;
    runtime.subscriptionId = null;
    runtime.caughtUp = false;
    if (subscriptionId !== null) this.#connection?.sendRemoteUnsubscribe(subscriptionId);
  }

  /** 当前运行期保存的最后连续来源位置（诊断与测试用；不提供任何持久化入口）。 */
  remoteCursor(sessionRef: SessionRef): OriginCursor | null {
    return this.#remote.get(this.#remoteKey(sessionRef))?.cursor ?? null;
  }

  #remoteKey(sessionRef: SessionRef): string {
    return `${sessionRef.ownerNodeId}|${sessionRef.sessionId}`;
  }

  /** 在当前连接上发一条新订阅；没有已认证连接时只保留 `wanted`，认证后统一重发。 */
  #sendRemoteSubscribe(runtime: RemoteRuntime): void {
    const connection = this.#connection;
    if (connection === null || connection.closed || this.connectionId === null) return;
    const previous = runtime.subscriptionId;
    if (previous !== null) connection.sendRemoteUnsubscribe(previous);
    const subscriptionId = this.#options.random.uuid();
    const sent = connection.sendRemoteSubscribe({
      subscriptionId,
      sessionRef: runtime.sessionRef,
      cursor: runtime.cursor,
    });
    if (!sent) return;
    runtime.subscriptionId = subscriptionId;
    runtime.caughtUp = false;
    this.#options.onEvent({
      kind: "remote_subscribing",
      sessionRef: runtime.sessionRef,
      subscriptionId,
      cursor: runtime.cursor,
    });
  }

  /** 认证后的统一重发：只重发仍被期望的订阅，且每条都换新 ID。 */
  #resubscribeRemote(): void {
    for (const runtime of this.#remote.values()) {
      if (!runtime.wanted) continue;
      this.#sendRemoteSubscribe(runtime);
    }
  }

  #remoteRuntimeOf(sessionRef: SessionRef): RemoteRuntime | undefined {
    return this.#remote.get(this.#remoteKey(sessionRef));
  }

  /** 来源事件去重的有界窗口：返回 `true` 表示首次见到。 */
  #markRemoteSeen(runtime: RemoteRuntime, originEventId: string): boolean {
    if (runtime.seen.has(originEventId)) return false;
    runtime.seen.add(originEventId);
    runtime.seenOrder.push(originEventId);
    if (runtime.seenOrder.length > REMOTE_SEEN_LIMIT) {
      const evicted = runtime.seenOrder.shift();
      if (evicted !== undefined) runtime.seen.delete(evicted);
    }
    return true;
  }

  /** 同连接刷新资源清单；合并在途请求，断线后保留刷新意图。 */
  refreshSnapshot(): void {
    this.#snapshotRefreshNeeded = true;
    if (this.#snapshotRefreshInFlight) this.#snapshotRefreshAgain = true;
    this.#flushSnapshotRefresh();
  }

  #flushSnapshotRefresh(): void {
    if (!this.#snapshotRefreshNeeded || this.#snapshotRefreshInFlight || this.connectionId === null) return;
    this.#snapshotRefreshInFlight = true;
    this.#snapshotRefreshAgain = false;
    this.#snapshotRefreshVerified = false;
    this.#snapshotRefreshCaughtUp = false;
    this.#connection?.refreshSnapshot();
  }

  /** 解除阻断（用户在主机侧确认身份或重新配对后由状态层调用）。 */
  clearBlocking(): void {
    this.#blocked = null;
  }

  /**
   * 以**稳定 `requestId`** 派发命令。
   *
   * 同一个 `requestId` 重复调用即视为重试：`dispatchCount` 递增而 ID 不变
   * （spec 场景「重连后以同一标识确认结果」）。
   */
  dispatch(command: CommandMessage): DispatchRecord {
    const requestId = command.body.requestId;
    const existing = this.#dispatch.get(requestId);
    const record: DispatchRecord =
      existing === undefined
        ? { requestId, command: command.body.command, dispatchCount: 1, acknowledged: false }
        : { ...existing, dispatchCount: existing.dispatchCount + 1 };
    this.#dispatch.set(requestId, record);
    this.#connection?.sendCommand(command);
    return record;
  }

  /**
   * 在当前连接上声明查看选择（`session.view`，§11.8）。
   *
   * 返回 `false` 表示没有已认证连接或该帧未发出；调用方据此保留声明状态为「未生效」，
   * 而不是假装已被服务端接受。`viewVersion` 的单调性由 feature 层维护。
   */
  setSessionView(input: { readonly sessionId: Uuid | null; readonly viewVersion: number }): boolean {
    return this.#connection?.sendSessionView(input) ?? false;
  }

  /**
   * 发送一条准备控制帧（`session.preparation`，§11.9）。
   *
   * 返回 `false` 表示没有已认证连接或该帧未发出（与 {@link setSessionView} 同一口径）：
   * 调用方据此把这次操作标为未发出，而不是伪造一个「看起来已经在分配」的状态。
   * 操作去重（`operationId`）仍由服务端与 feature 层各自持有。
   */
  dispatchPreparation(body: SessionPreparationBody): boolean {
    return this.#connection?.sendPreparation(body) ?? false;
  }

  /** 某条命令的派发登记。 */
  dispatchRecord(requestId: Uuid): DispatchRecord | null {
    return this.#dispatch.get(requestId) ?? null;
  }

  /** 构造一条 `command.status` 查询：查询自身的 ID 是新的，但 `targetRequestId` 是原 ID。 */
  buildStatusQuery(input: { readonly targetRequestId: Uuid; readonly requestId: Uuid }): CommandMessage {
    return {
      protocolVersion: 1,
      type: "command",
      messageId: input.requestId,
      connectionId: this.#requireConnectionId(),
      // 占位：真正的出站序号由 `SyncConnection.sendCommand` 用客户端方向计数器覆盖（§4.1）。
      connectionSequence: "0",
      body: {
        requestId: input.requestId,
        command: "command.status",
        payload: { targetRequestId: input.targetRequestId },
      },
    };
  }

  /**
   * 取本 epoch 的去重账本。
   *
   * epoch 相同时复用既有账本——这是 §8.5 切换窗口去重的前提；epoch 变化意味着
   * 服务端事件库已重建，必须丢弃旧账本（并等待 `sync.reset_required` 重建快照）。
   *
   * 这里是账本的**唯一**构造点：认证事件不得重建它，否则 `seenEventIds` 与游标一起清零，
   * 重投的旧事件会被二次呈现（R14「重复投递 MUST NOT 造成重复呈现」）。
   */
  #ledgerFor(serverEpoch: string): EventLedger {
    const existing = this.#ledger;
    if (existing !== null && existing.serverEpoch === serverEpoch) return existing;
    // 从最后确认游标起步：重连后的第一条事件必须是该游标的下一条。
    // 跨 epoch 的游标不可比，服务端事件库已重建 → 必须从无游标状态重新同步。
    const acked = this.#ackedCursor;
    const resumeFrom = acked !== null && acked.serverEpoch === serverEpoch ? acked : null;
    const ledger = new EventLedger({ serverEpoch, resumeFrom });
    this.#ledger = ledger;
    // 换 epoch 时恢复点必须**一起**丢掉（与 `connection.ts` 的 `#onAuthenticated` 同一口径）。
    // 留着它有两个后果：`ackedCursor` 对外呈现一条永远比较不出前进的游标（组合根会把它
    // 持久化成下次连接的 `resumeFrom`），而 `#isAdoptableBarrier` 若以它为 epoch 基准，
    // 会把重建后的第一条合法快照判成「外来 epoch」而永久拒掉——重建快照只发一次，
    // 随后的增量事件救不回它（§9.4 的正常恢复路径，不是异常）。
    if (acked !== null && acked.serverEpoch !== serverEpoch) this.#ackedCursor = null;
    return ledger;
  }

  #requireConnectionId(): Uuid {
    const id = this.#connection?.connectionId;
    if (id === null || id === undefined) {
      throw new Error("尚未建立已认证连接，无法构造命令");
    }
    return id;
  }

  /**
   * 快照屏障能否被门面采纳（同 epoch 且只前进）。
   *
   * 连接层已在 `#onSnapshotEnd` 里先让账本判过一次；这里再判一次是**必要的冗余**：
   * 门面是唯一把游标写进恢复点的地方，若它无条件采纳，`#ackedCursor` 就可能与账本水位分叉。
   * 分叉的后果是不可自愈的——一条更旧的屏障成为下次连接的 `resumeFrom` 后，它之后的真实事件
   * 永远补不回来；而一条越界游标还会让 `new EventLedger({resumeFrom})` 在**构造期**抛错，
   * 异常逃出 socket 回调后连接再也建立不起来。
   *
   * **基准为什么只能是 `#ackedCursor` 而不是别的**：跨 epoch 的游标不可比，所以「同 epoch」
   * 这件事本身必须相对于某个**当前有效**的 epoch 来判定。`#ackedCursor` 在 `#ledgerFor`
   * 换 epoch 时已被清空（见那里的注释），因此它与账本始终同 epoch；早先以它为基准的版本
   * 在 epoch 变化时仍持有旧 epoch 的游标，于是把重建后的第一条**合法**快照判成外来游标
   * 永久丢弃——这正是本守卫在引入时带来的回归。
   */
  #isAdoptableBarrier(cursor: Cursor): boolean {
    const acked = this.#ackedCursor;
    if (acked === null) return true;
    // 跨 epoch 的序号不可比：外来游标一律不采纳（与 `#moveAcked` 同一口径）。
    if (acked.serverEpoch !== cursor.serverEpoch) return false;
    return compareCursors(cursor, acked) >= 0;
  }

  #onConnectionEvent(event: SyncClientEvent): void {
    if (event.kind === "authenticated") {
      // 认证完成：把仍被期望的订阅全部重发（每条换新 subscriptionId），并让消费方回源读取重建。
      this.#resubscribeRemote();
    }
    if (event.kind === "closed" || event.kind === "blocked") {
      for (const runtime of this.#remote.values()) {
        runtime.subscriptionId = null;
        runtime.caughtUp = false;
      }
    }
    if (event.kind === "remote_event") {
      const runtime = this.#remoteRuntimeOf(event.sessionRef);
      if (runtime === undefined || !runtime.wanted || runtime.subscriptionId !== event.subscriptionId) return;
      if (!this.#markRemoteSeen(runtime, event.event.originEventId)) return;
      runtime.cursor = advanceOriginCursor(runtime.cursor, {
        originEpoch: event.event.originEpoch,
        originSequence: event.event.originSequence,
      });
    }
    if (event.kind === "remote_caught_up") {
      const runtime = this.#remoteRuntimeOf(event.sessionRef);
      if (runtime === undefined || runtime.subscriptionId !== event.subscriptionId) return;
      runtime.cursor = advanceOriginCursor(runtime.cursor, event.cursor);
      runtime.caughtUp = true;
    }
    if (event.kind === "remote_reset_required") {
      const runtime = this.#remoteRuntimeOf(event.sessionRef);
      if (runtime === undefined || runtime.subscriptionId !== event.subscriptionId) return;
      // 重置意味着此前的位置不再可恢复：C 必须丢掉，去重窗口也随来源重置清空。
      // （同一来源重置后可能重放相同标识的新内容；旧窗口会把它们误判成重复。）
      runtime.cursor = null;
      runtime.caughtUp = false;
      runtime.seen.clear();
      runtime.seenOrder.length = 0;
    }
    if (event.kind === "remote_unavailable") {
      const runtime = this.#remoteRuntimeOf(event.sessionRef);
      if (runtime === undefined || runtime.subscriptionId !== event.subscriptionId) return;
      runtime.subscriptionId = null;
      runtime.caughtUp = false;
    }
    if (event.kind === "remote_unsubscribed") {
      const runtime = this.#remoteRuntimeOf(event.sessionRef);
      if (runtime === undefined || runtime.subscriptionId !== event.subscriptionId) return;
      runtime.subscriptionId = null;
      runtime.caughtUp = false;
    }
    if (event.kind === "snapshot_verified") {
      // 只有验证通过**且**账本已采纳其屏障才替换；失败或被拒的路径根本不会走到这里。
      if (this.#isAdoptableBarrier(event.snapshot.cursor)) {
        this.#snapshots.replace(event.snapshot, this.#now());
        this.#ackedCursor = event.snapshot.cursor;
        if (this.#snapshotRefreshInFlight) this.#snapshotRefreshVerified = true;
      }
    }
    if (event.kind === "event") {
      // 事件已处理并 ACK：把恢复点抬到它的事件序号（由账本的游标给出，避免自己拼游标）。
      this.#ackedCursor = this.#ledger?.cursor ?? this.#ackedCursor;
    }
    if (event.kind === "online") {
      this.#ackedCursor = event.cursor;
      if (this.#snapshotRefreshInFlight) this.#snapshotRefreshCaughtUp = true;
    }
    if (event.kind === "closed" || event.kind === "blocked") this.#snapshotRefreshInFlight = false;
    if (event.kind === "blocked") {
      // 阻断后停止自动重连：状态机不再调用 `connect()`，本层也拒绝。
      this.#blocked = event.cause;
    }
    if (event.kind === "command_result") {
      const record = this.#dispatch.get(event.result.body.requestId);
      if (record !== undefined) {
        this.#dispatch.set(record.requestId, { ...record, acknowledged: true });
      }
    }
    this.#options.onEvent(event);
    // 哈希校验与 caught_up 的到达可能先后互换；两者都完成才允许下一轮刷新。
    if (this.#snapshotRefreshInFlight && this.#snapshotRefreshVerified && this.#snapshotRefreshCaughtUp) {
      this.#snapshotRefreshNeeded = this.#snapshotRefreshAgain;
      this.#snapshotRefreshAgain = false;
      this.#snapshotRefreshInFlight = false;
    }
    if (event.kind === "online" || event.kind === "snapshot_verified") this.#flushSnapshotRefresh();
  }
}
