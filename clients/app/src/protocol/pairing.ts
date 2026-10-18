/**
 * `schemas/sync/v1/pairing.schema.json` 的严格 TypeScript 镜像：HTTPS 配对请求/响应。
 *
 * 配对**不**经 WSS；设备配对的 `canonicalOrigin` 来自配对码 lookup 或扫码取回响应，
 * 它决定设备身份绑定的来源，来源变化必须重新配对（`pwa-web-client` 的「来源变化后不再自动信任」）。
 * 设备短二维码本身的 `q2` 载荷不是 JSON，编解码在 `./device-qr`。
 */

import type {
  Base64Url32,
  Base64Url64,
  Base64Url65,
  ErrorCode,
  Timestamp,
  Uuid,
} from "./common";

/** `$defs.canonicalOrigin`：`^https://[^/?#]+$`，无路径、无查询、无片段。 */
export type CanonicalOrigin = string;

/**
 * `$defs.lookupRequest`：配对码入口用 8 位码一次性取回配对绑定与秘密（§7.5）。
 *
 * `pairingCode` 是 Crockford base32（40 bit），大小写不敏感；服务端先做归一化
 * （`I`/`L → 1`、`O → 0`）再比较。它是**取回** secret 的凭据，不是 HMAC 密钥。
 */
export interface PairingLookupRequest {
  readonly protocolVersion: 1;
  readonly pairingCode: string;
}

/**
 * `$defs.lookupResponse`：lookup 成功返回的绑定材料（§7.5）。
 *
 * 不推进配对状态；成功即消耗配对码。之后的 claim/status/transcript 与二维码入口完全相同，
 * 因此 `hostPublicKey` 就是本次配对的固定主机公钥（网页入口的首次信任，design D5）。
 */
export interface PairingLookupResponse {
  readonly protocolVersion: 1;
  readonly pairingId: Uuid;
  readonly hostId: Uuid;
  readonly hostPublicKey: Base64Url65;
  readonly canonicalOrigin: CanonicalOrigin;
  readonly pairingSecret: Base64Url32;
  readonly expiresAt: Timestamp;
}

/**
 * `$defs.qrLookupRequest`：新设备短二维码的扫码取回请求（§7.6）。
 *
 * `qrToken` 是设备创建时独立生成的 32 字节取回凭据，既不是配对码也不是配对秘密；它同样以
 * 无填充 base64url 传输。来源不在请求体里，而由 HTTP `Origin` 与配对登记绑定比较。
 */
export interface PairingQrLookupRequest {
  readonly protocolVersion: 1;
  readonly qrToken: Base64Url32;
}

/**
 * `$defs.qrLookupResponse`：扫码取回成功返回与配对码 lookup 同形的七字段绑定。
 *
 * 取回不推进配对状态、不消耗网页配对码、不创建信任；重复取回返回同一组材料。
 */
export type PairingQrLookupResponse = PairingLookupResponse;

/** `$defs.claimRequest`：设备首次声明配对，`proof` 由设备私钥签名。 */
export interface PairingClaimRequest {
  readonly protocolVersion: 1;
  readonly pairingId: Uuid;
  readonly hostId: Uuid;
  readonly deviceId: Uuid;
  readonly deviceName: string;
  readonly clientKind: "pwa" | "android" | "ios" | "desktop";
  readonly canonicalOrigin: CanonicalOrigin;
  readonly devicePublicKey: Base64Url65;
  readonly clientNonce: Base64Url32;
  readonly proof: Base64Url32;
}

/** `$defs.claimResponse`：声明已被接收，等待主机侧确认。 */
export interface PairingClaimResponse {
  readonly protocolVersion: 1;
  readonly pairingRequestId: Uuid;
  readonly serverNonce: Base64Url32;
  readonly hostProof: Base64Url64;
  readonly status: "pending_confirmation";
  readonly expiresAt: Timestamp;
}

/** `$defs.statusRequest`：轮询配对状态，`proof` 绑定 `requestNonce`。 */
export interface PairingStatusRequest {
  readonly protocolVersion: 1;
  readonly hostId: Uuid;
  readonly deviceId: Uuid;
  readonly pairingId: Uuid;
  readonly pairingRequestId: Uuid;
  readonly requestNonce: Base64Url32;
  readonly proof: Base64Url32;
}

/**
 * `statusResponse.device`（仅在 `status = "approved"` 时出现）。
 *
 * 只有设备非秘密元数据：schema 的 `additionalProperties: false` 里没有授权集合字段
 * （设备方向没有授权维度），因此这里 MUST NOT 出现任何 scope。
 */
export interface PairingApprovedDevice {
  readonly deviceId: Uuid;
  readonly name: string;
}

/** `statusResponse.host`（仅在 `status = "approved"` 时出现）。 */
export interface PairingApprovedHost {
  readonly hostId: Uuid;
  readonly hostPublicKey: Base64Url65;
}

/** `$defs.statusResponse.status` 封闭词表。 */
export type PairingStatus =
  | "pending_confirmation"
  | "approved"
  | "rejected"
  | "expired"
  | "consumed";

/**
 * `$defs.statusResponse`：schema 用 `if/then/else` 保证 `device`/`host` **只**在
 * `approved` 时出现，这里用判别联合表达同一约束。
 */
export type PairingStatusResponse =
  | {
      readonly protocolVersion: 1;
      readonly pairingRequestId: Uuid;
      readonly status: "approved";
      readonly expiresAt: Timestamp;
      readonly device: PairingApprovedDevice;
      readonly host: PairingApprovedHost;
    }
  | {
      readonly protocolVersion: 1;
      readonly pairingRequestId: Uuid;
      readonly status: Exclude<PairingStatus, "approved">;
      readonly expiresAt: Timestamp;
    };

/** `$defs.httpError`：配对 HTTP 面的错误载荷（与 WSS 的 `error.body` 同构）。 */
export interface PairingHttpError {
  readonly code: ErrorCode;
  readonly message: string;
  readonly retryable: boolean;
  readonly correlationId: Uuid | null;
  readonly details: Record<string, unknown>;
}

/** `pairing.schema.json` 的顶层 `oneOf`。 */
export type PairingMessage =
  | PairingLookupRequest
  | PairingLookupResponse
  | PairingQrLookupRequest
  | PairingQrLookupResponse
  | PairingClaimRequest
  | PairingClaimResponse
  | PairingStatusRequest
  | PairingStatusResponse
  | PairingHttpError;
