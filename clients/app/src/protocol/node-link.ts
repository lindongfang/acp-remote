/**
 * `schemas/node-link/v1/pairing.schema.json` 的 `$defs.qrPayload` 的 TypeScript 镜像
 * （`NODE_LINK_PROTOCOL.md` §13.1）：node-link 配对二维码的载荷。
 *
 * ## 为什么只镜像这一项
 *
 * 前端只**读**这份载荷：配对入口据此判定「这条二维码是不是本入口的内容」并呈现对端信息。
 * node-link 的 claim / status HTTP 面由节点发起（§13.2 / §13.3），浏览器不是它的参与方，
 * 因此 `pairing.schema.json` 的其余 `$defs`（claimRequest / statusResponse / …）不在前端镜像里
 * ——镜像一份用不到的 DTO 只会与协议漂移。
 *
 * ## 与 sync 侧同源的口径
 *
 * 字段名、必填性与取值域逐项对应 schema；字段的**结构校验**在组合根（`readQrPayload` 的
 * node-link 分支），失败形态与 sync 侧一致（`protocol.invalid_json` / `protocol.schema_invalid`）。
 * 「哪一类入口接受哪一类载荷」不在这里判定——那是配对角色的选择（design D2/D3）。
 */

import type { Base64Url32, Base64Url65, Timestamp, Uuid } from "./common";

/** `$defs.qrPayload`。 */
export interface NodeLinkQrPayload {
  readonly pairingProtocol: "acp-remote-nodelink-v1";
  /** Owner 节点标识（JSON 键名保留协议原文）。 */
  readonly ownerNodeId: Uuid;
  /** Owner 节点公钥（base64url-65 字节）。 */
  readonly ownerPublicKey: Base64Url65;
  /** §13.1 的 `$defs.endpoint`：`wss://<host>/node-link/v1`。 */
  readonly endpoint: string;
  readonly pairingId: Uuid;
  readonly pairingSecret: Base64Url32;
  readonly expiresAt: Timestamp;
}
