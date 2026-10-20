/**
 * 连接阻断态与关闭码（`docs/SYNC_PROTOCOL.md` §12.3）的**唯一**判定处。
 *
 * 为什么从 `connection.ts` 抽出来：业务连接与附件连接是两条独立的 WSS，但「哪些关闭码意味着
 * 必须停止重连」是同一份协议事实。把它留在业务连接里会让附件连接要么复制一份表（两处表必然
 * 漂移），要么 import 业务连接（连接实现之间互相依赖）。这里只放协议常量与纯函数，
 * 两条连接的实现都从这一处取用。
 */

/** 关闭码（§12.3 的封闭表）。 */
export const CLOSE = {
  normal: 1000,
  protocolError: 4400,
  authRequired: 4401,
  versionIncompatible: 4406,
  identityConflict: 4409,
  deviceRevoked: 4410,
  connectionReplaced: 4411,
  serverUnavailable: 4500,
} as const;

/** 阻断原因（与 `src/state/connection-machine.ts` 的 `BlockingCause` 一一对应）。 */
export type ClientBlockCause =
  | "device_revoked"
  | "version_incompatible"
  | "host_identity_changed"
  | "replaced_by_other_connection";

/**
 * `version_incompatible` 之下**具体是哪一步谈不拢**（`blocked` 事件的 `reason`）：封闭词表。
 *
 * 为什么粗粒度原因不够：`version_incompatible` 同时覆盖三种**恢复路径不同**的情形——
 * 服务端没接受约定的子协议（要对准两端的部署与来源）、协议版本不兼容（要升级客户端或主机）、
 * 必需 feature 未被选中（要在主机侧升级）。三者落到同一个 `cause` 就再也分不出来，
 * 用户与诊断只能看到「协议或能力不兼容」这一句笼统的说明。
 *
 * 词表只登记这一族的细化：其余阻断原因（撤销、身份变化、被顶替）本身已经足够具体，
 * 没有进一步可分的取值，事件里因此不带 `reason`。
 */
export type IncompatibleReason =
  /** 服务端没有选中约定的子协议（§3.1）。 */
  | "subprotocol_not_selected"
  /** 协议版本不兼容（close 4406 / `protocol.version_unsupported` / 选中的版本不等于本实现）。 */
  | "protocol_version_unsupported"
  /** 服务端选中的 feature 未覆盖客户端必需列表（§5.2）。 */
  | "required_features_missing";

/** 关闭码 → 阻断原因；`null` 表示不是阻断（可退避重连）。 */
export function blockCauseForCloseCode(code: number): ClientBlockCause | null {
  if (code === CLOSE.connectionReplaced) return "replaced_by_other_connection";
  if (code === CLOSE.deviceRevoked) return "device_revoked";
  if (code === CLOSE.versionIncompatible) return "version_incompatible";
  if (code === CLOSE.identityConflict) return "host_identity_changed";
  return null;
}

/** 关闭码是否值得退避重试。 */
export function isRetryableCloseCode(code: number): boolean {
  return code === CLOSE.normal || code === CLOSE.serverUnavailable || code === CLOSE.protocolError || code === CLOSE.authRequired;
}
