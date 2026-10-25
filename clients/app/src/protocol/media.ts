/**
 * `schemas/sync/v1/media.schema.json` 的严格 TypeScript 镜像与运行期解码器。
 *
 * 三个值对象各自描述一件**已经发生的事实**：
 *
 * - `messageChannel`：回复与思考分别持久化；Agent 未提供思考时不生成 `thought`。
 * - `partialResult`：turn 终态的正文完整性。`partial` 表示已收到内容被保留但上游未完成，
 *   **不得**被当作完整成功回复。
 * - `mediaReference`：Owner 已封存附件的读取引用。它只标识内容与可用性，**不带**下载 URL、
 *   临时路径或 bearer 凭据；读取必须经 §3.4 的已认证附件连接并重新校验会话与主体归属。
 * - `effectiveInteraction`：服务端本次读取时确认过的当前有效请求（`requestVersion` +
 *   完整 `options`/`schema`）。历史卡片只有定位信息，不能直接应答。
 *
 * 与 `protocol/attachment.ts` 同一策略：封闭对象逐键白名单、不宽容解析、不补默认值。
 */

import type {
  Base64Url32,
  DecimalString,
  InteractionOption,
  Timestamp,
  Uuid,
} from "./common";

/** `$defs.messageChannel` 的封闭词表。 */
export type MessageChannel = "reply" | "thought";

/** `$defs.partialResult.state`。 */
export type PartialResultState = "completed" | "partial" | "failed" | "cancelled";

/** `$defs.partialResult.reason`；`completed` 时为 `null`。 */
export type PartialResultReason =
  | "agent_stopped"
  | "agent_failed"
  | "turn_cancelled"
  | "connection_lost"
  | "retention_expired";

/** `$defs.partialResult`。 */
export interface PartialResult {
  readonly state: PartialResultState;
  readonly reason: PartialResultReason | null;
}

/** `$defs.mediaReference.kind`。 */
export type MediaReferenceKind = "image" | "audio" | "file";

/** `$defs.mediaReference.availability`：不可用是显式事实，不得用空字节或空 URL 冒充。 */
export type MediaAvailability = "available" | "expired" | "not_fetched" | "unsupported";

/** `$defs.mediaReference`。 */
export interface MediaReference {
  readonly attachmentId: Uuid;
  readonly sessionId: Uuid;
  readonly kind: MediaReferenceKind;
  readonly mediaType: string;
  readonly byteLength: DecimalString;
  readonly sha256: Base64Url32;
  readonly displayName: string | null;
  readonly availability: MediaAvailability;
}

/** `$defs.effectiveInteraction`。 */
export interface EffectiveInteraction {
  readonly sequence?: DecimalString;
  readonly interactionId: Uuid;
  readonly sessionId: Uuid;
  readonly kind: "permission" | "elicitation";
  /** 每次 pending 写入时递增的十进制字符串；resolve 必须原样回传。 */
  readonly requestVersion: DecimalString;
  readonly expiresAt: Timestamp | null;
  readonly options: readonly InteractionOption[];
  /** elicitation 的 ACP 表单 schema 原样投影；permission 时为 `null`。 */
  readonly schema: Readonly<Record<string, unknown>> | null;
}

export type MediaDecode<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly detail: string };

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const DECIMAL_PATTERN = /^(0|[1-9][0-9]*)$/;
const BASE64URL_PATTERN = /^[A-Za-z0-9_-]+$/;
const TIMESTAMP_PATTERN = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/;

function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const prototype: unknown = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function hasOnlyKeys(value: Record<string, unknown>, allowed: readonly string[]): boolean {
  const set = new Set(allowed);
  return Object.keys(value).every((key) => set.has(key));
}

function isUuid(value: unknown): value is Uuid {
  return typeof value === "string" && UUID_PATTERN.test(value);
}

function isDecimal(value: unknown): value is DecimalString {
  return typeof value === "string" && DECIMAL_PATTERN.test(value);
}

/** `messageChannel` 判别（未知取值返回 `null`，绝不回落到 `reply`）。 */
export function messageChannelOf(value: unknown): MessageChannel | null {
  return value === "reply" || value === "thought" ? value : null;
}

const PARTIAL_STATES: Readonly<Record<string, PartialResultState>> = {
  completed: "completed",
  partial: "partial",
  failed: "failed",
  cancelled: "cancelled",
};

const PARTIAL_REASONS: Readonly<Record<string, PartialResultReason>> = {
  agent_stopped: "agent_stopped",
  agent_failed: "agent_failed",
  turn_cancelled: "turn_cancelled",
  connection_lost: "connection_lost",
  retention_expired: "retention_expired",
};

/** `$defs.partialResult` 解码；`completed` 必须没有 reason，其余必须有一个已知 reason。 */
export function decodePartialResult(value: unknown): MediaDecode<PartialResult> {
  if (!isPlainObject(value) || !hasOnlyKeys(value, ["state", "reason"])) {
    return { ok: false, detail: "partialResult 必须是只含 state/reason 的对象" };
  }
  const state = typeof value["state"] === "string" ? PARTIAL_STATES[value["state"]] : undefined;
  if (state === undefined) return { ok: false, detail: "partialResult.state 不在封闭词表里" };
  const rawReason = value["reason"];
  if (rawReason === null || rawReason === undefined) {
    if (state === "completed") return { ok: true, value: { state, reason: null } };
    return { ok: false, detail: "非 completed 的 partialResult 必须给出 reason" };
  }
  if (typeof rawReason !== "string") return { ok: false, detail: "partialResult.reason 必须是字符串或 null" };
  const reason = PARTIAL_REASONS[rawReason];
  if (reason === undefined) return { ok: false, detail: "partialResult.reason 不在封闭词表里" };
  if (state === "completed") return { ok: false, detail: "completed 的 partialResult 不得携带 reason" };
  return { ok: true, value: { state, reason } };
}

const MEDIA_KINDS: Readonly<Record<string, MediaReferenceKind>> = {
  image: "image",
  audio: "audio",
  file: "file",
};

const MEDIA_AVAILABILITY: Readonly<Record<string, MediaAvailability>> = {
  available: "available",
  expired: "expired",
  not_fetched: "not_fetched",
  unsupported: "unsupported",
};

/** `$defs.mediaReference` 解码。 */
export function decodeMediaReference(value: unknown): MediaDecode<MediaReference> {
  if (
    !isPlainObject(value) ||
    !hasOnlyKeys(value, [
      "attachmentId",
      "sessionId",
      "kind",
      "mediaType",
      "byteLength",
      "sha256",
      "displayName",
      "availability",
    ])
  ) {
    return { ok: false, detail: "mediaReference 的键集合不符" };
  }
  if (!isUuid(value["attachmentId"]) || !isUuid(value["sessionId"])) {
    return { ok: false, detail: "attachmentId/sessionId 必须是 UUID" };
  }
  const kind = typeof value["kind"] === "string" ? MEDIA_KINDS[value["kind"]] : undefined;
  if (kind === undefined) return { ok: false, detail: "mediaReference.kind 不在封闭词表里" };
  const availability =
    typeof value["availability"] === "string" ? MEDIA_AVAILABILITY[value["availability"]] : undefined;
  if (availability === undefined) return { ok: false, detail: "mediaReference.availability 不在封闭词表里" };
  const mediaType = value["mediaType"];
  if (typeof mediaType !== "string" || mediaType.length === 0 || mediaType.length > 255) {
    return { ok: false, detail: "mediaType 长度必须在 1..=255" };
  }
  if (!isDecimal(value["byteLength"])) return { ok: false, detail: "byteLength 必须是十进制字符串" };
  const sha256 = value["sha256"];
  if (typeof sha256 !== "string" || sha256.length !== 43 || !BASE64URL_PATTERN.test(sha256)) {
    return { ok: false, detail: "sha256 必须是 43 字符 base64url" };
  }
  const displayName = value["displayName"];
  if (displayName !== null && (typeof displayName !== "string" || displayName.length === 0 || displayName.length > 255)) {
    return { ok: false, detail: "displayName 必须是 1..=255 字符的字符串或 null" };
  }
  return {
    ok: true,
    value: {
      attachmentId: value["attachmentId"],
      sessionId: value["sessionId"],
      kind,
      mediaType,
      byteLength: value["byteLength"],
      sha256,
      displayName,
      availability,
    },
  };
}

/** 读 `options`：`maxItems` 16、每项 `{optionId,label,kind}` 三个非空字符串。 */
function decodeOptions(value: unknown): MediaDecode<readonly InteractionOption[]> {
  if (!Array.isArray(value) || value.length > 16) return { ok: false, detail: "options 必须是至多 16 项的数组" };
  const options: InteractionOption[] = [];
  for (const item of value) {
    if (!isPlainObject(item) || !hasOnlyKeys(item, ["optionId", "label", "kind"])) {
      return { ok: false, detail: "option 的键集合不符" };
    }
    const { optionId, label, kind } = item;
    if (
      typeof optionId !== "string" ||
      optionId.length === 0 ||
      optionId.length > 128 ||
      typeof label !== "string" ||
      label.length === 0 ||
      label.length > 512 ||
      typeof kind !== "string" ||
      kind.length === 0 ||
      kind.length > 64
    ) {
      return { ok: false, detail: "option 的字段长度或类型不符" };
    }
    options.push({ optionId, label, kind });
  }
  return { ok: true, value: options };
}

/**
 * `$defs.effectiveInteraction` 解码。
 *
 * `schema` 是 ACP 表单的**开放对象**（原样投影），因此只要求是对象或 `null`，不在协议层解释
 * 字段；值校验属于 `src/domain/form.ts`（它按当前支持的 schema 子集判定可表达性）。
 */
export function decodeEffectiveInteraction(value: unknown): MediaDecode<EffectiveInteraction> {
  if (
    !isPlainObject(value) ||
    !hasOnlyKeys(value, [
      "interactionId",
      "sessionId",
      "kind",
      "requestVersion",
      "sequence",
      "expiresAt",
      "options",
      "schema",
    ])
  ) {
    return { ok: false, detail: "effectiveInteraction 的键集合不符" };
  }
  if (!isUuid(value["interactionId"]) || !isUuid(value["sessionId"])) {
    return { ok: false, detail: "interactionId/sessionId 必须是 UUID" };
  }
  if (value["kind"] !== "permission" && value["kind"] !== "elicitation") {
    return { ok: false, detail: "kind 必须是 permission 或 elicitation" };
  }
  if (!isDecimal(value["requestVersion"])) {
    return { ok: false, detail: "requestVersion 必须是十进制字符串" };
  }
  if (value["sequence"] !== undefined && !isDecimal(value["sequence"])) {
    return { ok: false, detail: "sequence 必须是规范十进制字符串" };
  }
  const expiresAt = value["expiresAt"];
  if (expiresAt !== null && (typeof expiresAt !== "string" || !TIMESTAMP_PATTERN.test(expiresAt))) {
    return { ok: false, detail: "expiresAt 必须是 UTC 毫秒时间戳或 null" };
  }
  const options = decodeOptions(value["options"]);
  if (!options.ok) return options;
  const schema = value["schema"];
  if (schema !== null && !isPlainObject(schema)) {
    return { ok: false, detail: "schema 必须是对象或 null" };
  }
  return {
    ok: true,
    value: {
      interactionId: value["interactionId"],
      sessionId: value["sessionId"],
      kind: value["kind"],
      requestVersion: value["requestVersion"],
      ...(value["sequence"] === undefined ? {} : { sequence: value["sequence"] as DecimalString }),
      expiresAt,
      options: options.value,
      schema: schema === null ? null : schema,
    },
  };
}
