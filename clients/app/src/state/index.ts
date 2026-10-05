/**
 * 客户端状态机层（`docs/FRONTEND_DESIGN.md` §3 的 `state/`）。
 *
 * **归属 WP6**（`plan.md` 的 WP6 行）：连接 12 态（8 常规态 + 4 阻断态）与命令 6 态的
 * 迁移实现由 WP6 承担。WP5a 只把**状态集合**与**命令状态集合**按 specs 逐字固定下来，
 * 让 WP7 的门控分支有稳定取值域可依赖，同时不实现迁移逻辑。
 *
 * 取值逐条取自 `specs/pwa-web-client/spec.md` 的「连接状态是互斥状态机」与
 * 「命令以稳定标识与终态收敛」两节。
 */

/**
 * 连接状态：互斥状态机（不是可同时成立的布尔组合）。
 *
 * 8 个常规态 + 4 个阻断态 = 12 态。
 */
export type ConnectionState =
  // 常规态
  | "unpaired"
  | "pairing"
  | "disconnected"
  | "connecting"
  | "authenticating"
  | "replaying"
  | "online"
  | "reconnecting"
  // 阻断态：必须停止自动重连并向用户说明原因
  | "revoked"
  | "incompatible"
  | "identity_changed"
  | "replaced";

/**
 * 命令状态：只有服务端确认才进入终态；`uncertain` 不得由网络断开自行断定。
 */
export type CommandState =
  | "draft"
  | "submitting"
  | "accepted"
  | "completed"
  | "failed"
  | "rejected"
  | "uncertain";

/** 阻断态清单：`state` 层的迁移实现据此停止自动重连（WP6 消费）。 */
export const BLOCKING_CONNECTION_STATES: Record<ConnectionState, true | undefined> = {
  revoked: true,
  incompatible: true,
  identity_changed: true,
  replaced: true,
  unpaired: undefined,
  pairing: undefined,
  disconnected: undefined,
  connecting: undefined,
  authenticating: undefined,
  replaying: undefined,
  online: undefined,
  reconnecting: undefined,
};
