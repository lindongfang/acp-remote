/**
 * `schemas/sync/v1/error.schema.json` 的严格 TypeScript 镜像：
 * 连接级错误（认证前/后两种信封）、`control.ping`、`control.pong`。
 */

import type { Base64Url16, DecimalString, ErrorCode, Uuid } from "./common";

/** `$defs.preAuthBase`：认证完成前省略 `connectionId`/`connectionSequence`。 */
export interface PreAuthBase {
  readonly protocolVersion: 1;
  readonly messageId: Uuid;
}

/** `$defs.postAuthBase`。 */
export interface ErrorPostAuthBase {
  readonly protocolVersion: 1;
  readonly messageId: Uuid;
  readonly connectionId: Uuid;
  readonly connectionSequence: DecimalString;
}

/**
 * `$defs.errorBody.body`：`details` 是 schema 里明确的开放对象；
 * `correlationId` 是可空 uuid（键必填）。
 */
export interface ErrorBodyFields {
  readonly type: "error";
  readonly body: {
    readonly code: ErrorCode;
    readonly message: string;
    readonly retryable: boolean;
    readonly correlationId: Uuid | null;
    readonly details: Record<string, unknown>;
  };
}

/** `$defs.error`：认证前或认证后两种信封之一。 */
export type ErrorMessage = (PreAuthBase | ErrorPostAuthBase) & ErrorBodyFields;

/** `$defs.ping`。 */
export interface ControlPing extends ErrorPostAuthBase {
  readonly type: "control.ping";
  readonly body: { readonly nonce: Base64Url16 };
}

/** `$defs.pong`。 */
export interface ControlPong extends ErrorPostAuthBase {
  readonly type: "control.pong";
  readonly body: { readonly nonce: Base64Url16 };
}

/** `error.schema.json` 的顶层 `oneOf`。 */
export type ErrorSchemaMessage = ErrorMessage | ControlPing | ControlPong;
