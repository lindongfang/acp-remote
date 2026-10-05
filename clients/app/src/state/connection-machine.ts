/**
 * 连接互斥状态机（R12「连接状态是互斥状态机」）。
 *
 * ## 为什么是「一个值」而不是一组布尔
 *
 * `docs/FRONTEND_DESIGN.md` §5 与 spec 的同一节都明确：状态**必须**互斥。
 * 一组布尔（`isConnecting && isReplaying && !isOffline`）在真实时序里必然出现
 * 「同时为真」的中间帧，UI 就会把 `replaying` 呈现成 `online`——这正是
 * 「追平期不谎称已在线」要禁止的行为。因此本文件的状态就是一个封闭联合类型。
 *
 * ## 迁移表为什么显式写出
 *
 * 把合法迁移写成 `Record<ConnectionState, readonly ConnectionState[]>` 而不是散落的
 * `if`，是为了让「哪些迁移合法」成为一个**可枚举、可断言**的事实：测试可以遍历全表
 * 检查每条边都由某个事件驱动，且不可达状态（例如 `unpaired` 直接到 `online`）确实
 * 不在表里。这比逐条 `if` 更难写错，也更难被后续改动悄悄放宽。
 */

import { BLOCKING_CONNECTION_STATES, type ConnectionState } from "./index";

/** 12 个状态的封闭清单（8 常规 + 4 阻断），与 spec 的枚举逐字对应。 */
export const CONNECTION_STATES = {
  unpaired: true,
  pairing: true,
  disconnected: true,
  connecting: true,
  authenticating: true,
  replaying: true,
  online: true,
  reconnecting: true,
  revoked: true,
  incompatible: true,
  identity_changed: true,
  replaced: true,
} as const satisfies Record<ConnectionState, true>;

export type ConnectionStateName = keyof typeof CONNECTION_STATES;

/** 阻断态的封闭清单（`revoked`/`incompatible`/`identity_changed`/`replaced`）。 */
export const BLOCKING_STATES = {
  revoked: true,
  incompatible: true,
  identity_changed: true,
  replaced: true,
} as const;

export type BlockingState = keyof typeof BLOCKING_STATES;

/** 阻断原因的可判定分类：UI 说明与恢复路径据此分文案。 */
export type BlockingCause =
  /** 设备被主机撤销：清缓存并要求重新配对。 */
  | "device_revoked"
  /** 协议版本或必需 feature 不兼容：升级客户端。 */
  | "version_incompatible"
  /** 主机身份（已配对 Host key）与本次来源不符：必须回主机侧确认。 */
  | "host_identity_changed"
  /** 同设备另一条已认证连接顶替了本连接。 */
  | "replaced_by_other_connection";

/**
 * 阻断原因 → 阻断状态。
 *
 * 两者取值不同名（`device_revoked` → `revoked`），因此映射必须显式写出：
 * 直接把 `cause` 当作 `state` 会产生一个既不在 12 态枚举内、也没有任何消费方的值。
 */
const BLOCKING_STATE_BY_CAUSE: Record<BlockingCause, BlockingState> = {
  device_revoked: "revoked",
  version_incompatible: "incompatible",
  host_identity_changed: "identity_changed",
  replaced_by_other_connection: "replaced",
};

/** 阻断态携带的说明：原因 + 可行的下一步（R12 的 MUST）。 */
export interface BlockingDetail {
  readonly state: BlockingState;
  readonly cause: BlockingCause;
  /** 面向用户的原因说明。 */
  readonly reason: string;
  /** 可行的下一步；必须存在——阻断态不能只说「连不上」。 */
  readonly nextStep: string;
}

const BLOCKING_DETAILS: Record<BlockingCause, { readonly reason: string; readonly nextStep: string }> = {
  device_revoked: {
    reason: "这台设备已被主机撤销。",
    nextStep: "在电脑端重新配对后再试；本地数据需要重新建立。",
  },
  version_incompatible: {
    reason: "主机与这个客户端的协议版本或必需能力不兼容。",
    nextStep: "升级客户端，或在电脑端升级主机后重试。",
  },
  host_identity_changed: {
    reason: "主机身份与配对时记录的不一致。",
    nextStep: "回到主机侧确认新身份后再继续；为安全起见不会自动信任。",
  },
  replaced_by_other_connection: {
    reason: "同一设备的另一个标签页已经建立了已认证连接。",
    nextStep: "关闭另一个标签页，或在此页手动重新连接。",
  },
};

/** 状态机接受的迁移事件。取值与 sync-client 交付的观测点一一对应。 */
export type ConnectionEvent =
  /** 用户发起配对。 */
  | { readonly kind: "pair_started" }
  /** 配对成功，已持有设备身份。 */
  | { readonly kind: "pair_succeeded" }
  /** 本地无设备身份（浏览器清过站点数据）。 */
  | { readonly kind: "identity_lost" }
  /** 开始一次连接尝试。 */
  | { readonly kind: "connect_started"; readonly fromReconnect: boolean }
  /** socket 已打开，握手开始。 */
  | { readonly kind: "socket_opened" }
  /** 认证成功并已发出 subscribe；处于重放追平。 */
  | { readonly kind: "subscribed" }
  /** `sync.caught_up` 到达：此后为实时事件。 */
  | { readonly kind: "caught_up" }
  /** 连接断开，准备退避重连。 */
  | { readonly kind: "connection_closed"; readonly retryable: boolean }
  /** 用户主动断开。 */
  | { readonly kind: "disconnect_requested" }
  /** 进入阻断态。 */
  | { readonly kind: "blocked"; readonly cause: BlockingCause }
  /** 用户在阻断态后显式恢复（例如重新配对完成或确认了主机身份）。 */
  | { readonly kind: "blocking_cleared" };

/** 连接机的完整状态：一个互斥状态 + 阻断说明。 */
export interface ConnectionMachineState {
  readonly state: ConnectionStateName;
  /** 仅阻断态非空。 */
  readonly blocking: BlockingDetail | null;
  /**
   * 是否处于「本地内容尚未追平」。
   *
   * spec 场景「追平期不谎称已在线」要求呈现层明确标注这一点；它由 `state` 派生，
   * 不是独立可写的布尔——否则又会退化成可同时成立的组合。
   */
  readonly caughtUp: boolean;
}

/** 初始状态：未配对。 */
export function initialConnectionState(): ConnectionMachineState {
  return { state: "unpaired", blocking: null, caughtUp: false };
}

/** 阻断态判定：与 `src/state/index.ts` 的 `BLOCKING_CONNECTION_STATES` 同源（测试断言两者一致）。 */
export function isBlockingState(state: ConnectionStateName): state is BlockingState {
  return BLOCKING_CONNECTION_STATES[state] === true;
}

/**
 * 合法迁移表：每个事件允许的**来源**状态（目标态由 `transition` 的分支决定）。
 *
 * 表里没有的组合一律视为**非法**，`transition` 会抛错而不是静默接受——
 * 静默接受会让「追平期被误标成在线」这类 bug 在运行时完全不可见。
 *
 * 表中刻意**不含**的边（每条都有 spec 依据）：
 *
 * - `unpaired → connecting`：没有设备身份就没有可签名的握手，连接必然失败。
 * - `* → online`（除 `replaying --caught_up-->`）：在线只能由追平完成产生，
 *   不存在「连上就是在线」的捷径。
 * - `*阻断态 → connect_started`：恢复必须由显式动作触发
   （`blocking_cleared` → `disconnected` → `connect_started`），避免两个标签页互相顶替。
 */
const ALLOWED_SOURCES: Record<ConnectionEvent["kind"], Partial<Record<ConnectionStateName, true>>> = {
  pair_started: { unpaired: true, disconnected: true, revoked: true, identity_changed: true, incompatible: true },
  pair_succeeded: { pairing: true },
  identity_lost: { online: true, disconnected: true, reconnecting: true, revoked: true, identity_changed: true },
  // 阻断态**不在**来源里：恢复必须先 `blocking_cleared`（回到 disconnected），否则
  // `transition` 会把 `blocking` 清成 null，丢掉 R12 要求阻断态携带的原因与下一步。
  connect_started: {
    disconnected: true,
    connecting: true,
    authenticating: true,
    replaying: true,
    online: true,
    reconnecting: true,
  },
  socket_opened: { connecting: true, reconnecting: true },
  subscribed: { authenticating: true },
  caught_up: { replaying: true },
  connection_closed: { connecting: true, authenticating: true, replaying: true, online: true },
  disconnect_requested: {
    connecting: true,
    authenticating: true,
    replaying: true,
    online: true,
    reconnecting: true,
    unpaired: true,
    disconnected: true,
  },
  blocked: {
    connecting: true,
    authenticating: true,
    replaying: true,
    online: true,
    reconnecting: true,
    disconnected: true,
    revoked: true,
    incompatible: true,
    identity_changed: true,
    replaced: true,
  },
  blocking_cleared: { revoked: true, incompatible: true, identity_changed: true, replaced: true },
};

/** 非法迁移错误：显式失败而不是静默改状态。 */
export class IllegalConnectionTransition extends Error {
  readonly from: ConnectionStateName;
  readonly event: ConnectionEvent["kind"];

  constructor(from: ConnectionStateName, event: ConnectionEvent["kind"]) {
    super(`连接状态机：${from} 不接受 ${event}`);
    this.name = "IllegalConnectionTransition";
    this.from = from;
    this.event = event;
  }
}

/**
 * 应用一个事件。
 *
 * @throws {IllegalConnectionTransition} 该迁移不在表内。
 */
export function transition(
  current: ConnectionMachineState,
  event: ConnectionEvent,
): ConnectionMachineState {
  const allowed = ALLOWED_SOURCES[event.kind]?.[current.state];
  if (allowed !== true) throw new IllegalConnectionTransition(current.state, event.kind);

  if (event.kind === "blocked") {
    // 因果名与状态名不是同一组取值（`device_revoked` → `revoked`），因此显式映射：
    // 直接把 cause 当状态用会得到一个既不在 12 态枚举内、也没人消费的值。
    const blockedState = BLOCKING_STATE_BY_CAUSE[event.cause];
    const detail = BLOCKING_DETAILS[event.cause];
    return {
      state: blockedState,
      blocking: { state: blockedState, cause: event.cause, ...detail },
      caughtUp: false,
    };
  }
  if (event.kind === "blocking_cleared") {
    // 阻断态解除后回到「已断开」而不是直接在线：恢复必须重新走握手与追平。
    return { state: "disconnected", blocking: null, caughtUp: false };
  }
  if (event.kind === "caught_up") {
    return { state: "online", blocking: null, caughtUp: true };
  }
  if (event.kind === "subscribed") {
    return { state: "replaying", blocking: null, caughtUp: false };
  }
  if (event.kind === "socket_opened") {
    return { state: "authenticating", blocking: null, caughtUp: false };
  }
  if (event.kind === "connection_closed") {
    // retryable=false 表示不该再自动重连（例如服务端明确拒绝），落到 disconnected 等用户动作。
    return { state: event.retryable ? "reconnecting" : "disconnected", blocking: null, caughtUp: false };
  }
  if (event.kind === "connect_started") {
    return { state: event.fromReconnect ? "reconnecting" : "connecting", blocking: null, caughtUp: false };
  }
  if (event.kind === "identity_lost") {
    return { state: "unpaired", blocking: null, caughtUp: false };
  }
  if (event.kind === "pair_started") {
    return { state: "pairing", blocking: null, caughtUp: false };
  }
  if (event.kind === "pair_succeeded") {
    return { state: "disconnected", blocking: null, caughtUp: false };
  }
  // disconnect_requested
  return { state: "disconnected", blocking: null, caughtUp: false };
}

/** 阻断态是否必须停止自动重连（R12 的 MUST）。 */
export function shouldAutoReconnect(current: ConnectionMachineState): boolean {
  return current.blocking === null && current.state !== "unpaired";
}

/**
 * 离线期间用户输入是否可被标记为「已发送」或「已排队」。
 *
 * spec「离线时输入不被标记为已发送」+ `docs/FRONTEND_DESIGN.md` §5
 * 「第一阶段默认不离线排队 prompt」：只有 `online` 允许。
 */
export function canMarkInputAsSent(current: ConnectionMachineState): boolean {
  return current.state === "online";
}

/** 每个事件允许的来源状态（测试用它断言迁移表与 12 态一致、阻断态确实不发自动重连）。 */
export function allowedSourceStates(): Readonly<Record<ConnectionEvent["kind"], readonly ConnectionStateName[]>> {
  const out: Record<string, readonly ConnectionStateName[]> = {};
  for (const [kind, sources] of Object.entries(ALLOWED_SOURCES)) {
    out[kind] = Object.keys(sources ?? {}) as ConnectionStateName[];
  }
  return out as Record<ConnectionEvent["kind"], readonly ConnectionStateName[]>;
}