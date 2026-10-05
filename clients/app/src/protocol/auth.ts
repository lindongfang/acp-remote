/**
 * `schemas/sync/v1/auth.schema.json` 的严格 TypeScript 镜像。
 *
 * 四条认证消息的必填集合与信封形状逐字对应 schema：
 * 认证前只有 `messageId`（无 `connectionId`/`connectionSequence`），认证后两者齐备。
 */

import type { Base64Url32, Base64Url64, FeatureList, Uuid } from "./common";

/** `$defs.clientHello`：`minProtocolVersion` 必须不大于 `maxProtocolVersion`（运行时判定）。 */
export interface AuthClientHello {
  readonly protocolVersion: 1;
  readonly type: "auth.client_hello";
  readonly messageId: Uuid;
  readonly body: {
    readonly minProtocolVersion: number;
    readonly maxProtocolVersion: number;
    readonly hostId: Uuid;
    readonly deviceId: Uuid;
    readonly clientKind: "pwa" | "android" | "ios" | "desktop";
    readonly clientNonce: Base64Url32;
    readonly supportedFeatures: FeatureList;
    readonly requiredFeatures: FeatureList;
  };
}

/** `$defs.serverChallenge`：`hostProof` 是 64-byte P1363 `r || s` 的无填充 base64url。 */
export interface AuthServerChallenge {
  readonly protocolVersion: 1;
  readonly type: "auth.server_challenge";
  readonly messageId: Uuid;
  readonly body: {
    readonly selectedProtocolVersion: number;
    readonly hostId: Uuid;
    readonly connectionId: Uuid;
    readonly serverNonce: Base64Url32;
    readonly selectedFeatures: FeatureList;
    readonly hostProof: Base64Url64;
  };
}

/** `$defs.clientProof`。 */
export interface AuthClientProof {
  readonly protocolVersion: 1;
  readonly type: "auth.client_proof";
  readonly messageId: Uuid;
  readonly body: {
    readonly connectionId: Uuid;
    readonly deviceId: Uuid;
    readonly deviceProof: Base64Url64;
  };
}

/** `$defs.authenticated`：连接级限额随认证结果下发。 */
export interface AuthAuthenticated {
  readonly protocolVersion: 1;
  readonly type: "auth.authenticated";
  readonly messageId: Uuid;
  readonly connectionId: Uuid;
  readonly connectionSequence: string;
  readonly body: {
    readonly deviceId: Uuid;
    readonly scopes: readonly string[];
    readonly serverEpoch: Uuid;
    readonly headGlobalSequence: string;
    readonly heartbeatIntervalMs: number;
    readonly limits: {
      readonly maxMessageBytes: number;
      readonly maxPromptBytes: number;
      readonly maxReplayEventsPerBatch: number;
    };
  };
}

/** `auth.schema.json` 的顶层 `oneOf`。 */
export type AuthMessage =
  | AuthClientHello
  | AuthServerChallenge
  | AuthClientProof
  | AuthAuthenticated;
