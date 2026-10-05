/**
 * `schemas/sync/v1/pairing.schema.json` 的严格 TypeScript 镜像：HTTPS 配对请求/响应。
 *
 * 配对**不**经 WSS；二维码载荷里的 `canonicalOrigin` 决定设备身份绑定的来源，
 * 来源变化必须重新配对（`pwa-web-client` 的「来源变化后不再自动信任」）。
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

/** `$defs.qrPayload`：扫码前由主机出示的配对凭据。 */
export interface PairingQrPayload {
  readonly pairingProtocol: "acp-remote-pairing-v1";
  readonly hostId: Uuid;
  readonly hostPublicKey: Base64Url65;
  readonly canonicalOrigin: CanonicalOrigin;
  readonly pairingId: Uuid;
  readonly pairingSecret: Base64Url32;
  readonly expiresAt: Timestamp;
}

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

/** `statusResponse.device`（仅在 `status = "approved"` 时出现）。 */
export interface PairingApprovedDevice {
  readonly deviceId: Uuid;
  readonly name: string;
  readonly scopes: readonly string[];
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
  | PairingQrPayload
  | PairingClaimRequest
  | PairingClaimResponse
  | PairingStatusRequest
  | PairingStatusResponse
  | PairingHttpError;
