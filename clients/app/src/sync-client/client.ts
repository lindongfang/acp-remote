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

import type { CommandMessage, Cursor, Uuid } from "../protocol";
import { SyncConnection, type AuthenticatedInfo, type ClientBlockCause, type SyncClientEvent } from "./connection";
import { EventLedger } from "./dedupe";
import { SnapshotStaging } from "./snapshot";
import type { VerifiedSnapshot } from "./snapshot";
import type { DigestPort, RandomPort, SocketHandlers, SocketPort, TranscriptCodec, DeviceIdentityLike, HostIdentityPort } from "./ports";

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
  readonly identity: DeviceIdentityLike;
  readonly host: HostIdentityPort;
  readonly random: RandomPort;
  readonly transcript: TranscriptCodec;
  readonly digest: DigestPort;
  readonly openSocket: (input: { readonly url: string; readonly handlers: SocketHandlers }) => SocketPort;
  /** 最后一次持久 ACK 的游标；首次同步为 `null`。 */
  readonly resumeCursor: Cursor | null;
  /** 事件的唯一消费者（`src/state/` 的连接机与后续的 feature 层）。 */
  readonly onEvent: (event: SyncClientEvent) => void;
  readonly nowMs?: () => number;
}

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
  /** 跨连接复用的去重账本；服务端 epoch 变化时丢弃（必须重建快照）。 */
  #ledger: EventLedger | null = null;
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

  constructor(options: SyncClientOptions) {
    this.#options = options;
    this.#now = options.nowMs ?? (() => 0);
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

  /** 已关闭连接或未连接。 */
  get connected(): boolean {
    return this.#connection !== null;
  }

  /** 已确认游标；重连时作为 `sync.subscribe` 的恢复点。 */
  get ackedCursor(): Cursor | null {
    return this.#ackedCursor;
  }

  /** 认证信息；未认证时为 `null`。 */
  get authenticatedInfo(): AuthenticatedInfo | null {
    return this.#connection?.authenticatedInfo ?? null;
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
        identity: this.#options.identity,
        host: this.#options.host,
        random: this.#options.random,
        transcript: this.#options.transcript,
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
    connection.start();
  }

  /** 关闭当前连接。 */
  disconnect(): void {
    this.#connection?.close();
    this.#connection = null;
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
    return ledger;
  }

  #requireConnectionId(): Uuid {
    const id = this.#connection?.connectionId;
    if (id === null || id === undefined) {
      throw new Error("尚未建立已认证连接，无法构造命令");
    }
    return id;
  }

  #onConnectionEvent(event: SyncClientEvent): void {
    if (event.kind === "snapshot_verified") {
      // 只有验证通过才替换：失败路径根本不会走到这里。
      this.#snapshots.replace(event.snapshot, this.#now());
      this.#ackedCursor = event.snapshot.cursor;
    }
    if (event.kind === "event") {
      // 事件已处理并 ACK：把恢复点抬到它的事件序号（由账本的游标给出，避免自己拼游标）。
      this.#ackedCursor = this.#ledger?.cursor ?? this.#ackedCursor;
    }
    if (event.kind === "online") {
      this.#ackedCursor = event.cursor;
    }
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
  }
}
