/**
 * 结构化内容的显式降级与诊断（R18「未识别与不可用的结构化内容显式降级」）。
 *
 * ## 四类降级原因（每类都有 MUST，对应 spec 的三条场景）
 *
 * 1. `no_dedicated_view`：事件类型不在 `event-views.schema.json` 的 34 个 `$defs` 里。
 *    MUST 呈现降级卡片并保留原文与元数据；MUST NOT 静默隐藏，也 MUST NOT 把它降级成普通文本
 *    （因此本模型**不**提供 `text` 字段——没有专用视图时根本没有可渲染的正文）。
 * 2. `unsupported`：能力因**客户端 / 服务端 / Agent** 之一不可用。三类原因必须可区分，
 *    因此它们是三个不同的取值而不是一句笼统的「不可用」。
 * 3. `raw_not_synced`：原文因单条上限/保留期/存储失败未随事件下发。
 *    MUST 说明原因，MUST NOT 渲染占位内容假装完整——因此本模型在这种情况下**没有** `rawJson`，
 *    组件也没有可渲染的原文分支。
 * 4. `binary_not_delivered`：结构化事件已收到、但引用的二进制内容没有下发。
 *    MUST 说明「事件到了、内容没到」这一区别，而不是显示成图片/资源本身。
 *
 * ## 为什么降级信息不进消息正文
 *
 * `src/domain/conversation.ts` 的 `MessageView` 只有纯文本与 `hasStructuredContent` 标记：
 * 结构化块的原文与元数据由本模型承载。这样「未识别事件」不会伪装成一条普通消息，
 * 整个会话的渲染也不会因某一条未知事件而失败。
 */

import type { ErrorCode, EventMessage, PublicError, RawAcp } from "../protocol";
import { isKnownEventType } from "../protocol";

/** 不支持的三类原因（R18 MUST 可区分）。 */
export type UnsupportedBy = "client" | "broker" | "agent";

/** 三类原因的错误码 → 判别值。唯一来源是 `$defs.errorCode` 的 `capability.unsupported_by_*`。 */
const UNSUPPORTED_BY_CODE: Partial<Record<ErrorCode, UnsupportedBy>> = {
  "capability.unsupported_by_client": "client",
  "capability.unsupported_by_broker": "broker",
  "capability.unsupported_by_agent": "agent",
};

/** 三类原因的展示文案：措辞必须互不相同，合并成一句即违反 R18。 */
export const UNSUPPORTED_LABELS: Readonly<Record<UnsupportedBy, string>> = {
  client: "当前客户端不支持",
  broker: "服务端不支持",
  agent: "Agent 不支持",
};

/** 原文未下发的三种原因（`RawAcp.rawUnavailable.reason`）。 */
export type RawNotSyncedReason = "size_limit" | "retention_expired" | "storage_failure";

/** 原文未下发原因的展示文案。 */
export const RAW_NOT_SYNCED_LABELS: Readonly<Record<RawNotSyncedReason, string>> = {
  size_limit: "原文超出单条上限，未随事件下发",
  retention_expired: "原文已过保留期，未随事件下发",
  storage_failure: "原文写入失败，未随事件下发",
};

/** 原文可用性。`absent` 与 `not_synced` 是两件事：前者没有原文可谈，后者有但没给。 */
export type RawAvailability =
  | { readonly kind: "available"; readonly rawJson: string; readonly byteLength: string }
  | {
      readonly kind: "not_synced";
      readonly reason: RawNotSyncedReason;
      readonly byteLength: string;
      readonly sha256: string | null;
    }
  | { readonly kind: "absent" };

/** 降级原因（判别联合）。 */
export type DegradedReason =
  | { readonly kind: "no_dedicated_view" }
  | { readonly kind: "unsupported"; readonly by: UnsupportedBy; readonly detail: string }
  | { readonly kind: "raw_not_synced"; readonly reason: RawNotSyncedReason }
  | { readonly kind: "binary_not_delivered"; readonly mimeType: string | null; readonly uri: string | null };

/** 一条降级事件的视图模型。 */
export interface DegradedEventModel {
  readonly eventId: string;
  /** 原样保留的事件类型：未识别的事件也照实显示，不替换成占位名称。 */
  readonly eventType: string;
  readonly globalSequence: string;
  readonly createdAt: string;
  readonly sessionId: string | null;
  readonly originKind: string;
  readonly reason: DegradedReason;
  readonly raw: RawAvailability;
  /** 原始 `view`；只在诊断抽屉里以 JSON 呈现，**不**进消息正文。 */
  readonly view: Readonly<Record<string, unknown>>;
}

/** 由结构化错误码判定三类不支持之一；不是不支持类错误时返回 `null`。 */
export function unsupportedByFromErrorCode(code: ErrorCode | null | undefined): UnsupportedBy | null {
  if (code === null || code === undefined) return null;
  return UNSUPPORTED_BY_CODE[code] ?? null;
}

/** `RawAcp`（可缺席）→ 原文可用性。 */
function rawAvailability(acp: RawAcp | undefined): RawAvailability {
  if (acp === undefined) return { kind: "absent" };
  if ("rawUnavailable" in acp) {
    return {
      kind: "not_synced",
      reason: acp.rawUnavailable.reason,
      byteLength: acp.rawUnavailable.byteLength,
      sha256: acp.rawUnavailable.sha256,
    };
  }
  return { kind: "available", rawJson: acp.rawJson, byteLength: acp.byteLength };
}

/** 从一个 `view` 里取结构化错误的 `code`（若存在）。 */
function errorCodeInView(view: Readonly<Record<string, unknown>>): ErrorCode | null {
  const error = view["error"];
  if (typeof error !== "object" || error === null || !("code" in error)) return null;
  const code = error.code;
  return typeof code === "string" ? (code as ErrorCode) : null;
}

/**
 * `view` 是否引用了**没有下发**的二进制内容。
 *
 * 结构化引用（`image_ref` / `resource_ref`）的 `displayState` 缺席时**不得**当成可用：
 * 宁可说明「事件已收到、内容未下发」，也不显示一个假的图片位。
 */
function referencesUndeliveredBinary(view: Readonly<Record<string, unknown>>): boolean {
  const block = view["block"];
  if (typeof block !== "object" || block === null || !("type" in block)) return false;
  const type = block.type;
  if (type !== "image_ref" && type !== "resource_ref") return false;
  return !("displayState" in block) || block.displayState !== "available";
}

/** `view` 里引用的二进制内容的描述（mimeType / uri），供说明文案使用。 */
function binaryReference(view: Readonly<Record<string, unknown>>): { mimeType: string | null; uri: string | null } {
  const block = view["block"];
  if (typeof block !== "object" || block === null) return { mimeType: null, uri: null };
  const mimeType = "mimeType" in block ? block.mimeType : null;
  const uri = "uri" in block ? block.uri : null;
  return {
    mimeType: typeof mimeType === "string" ? mimeType : null,
    uri: typeof uri === "string" ? uri : null,
  };
}

/**
 * 判定一条事件/结果是否需要降级呈现。
 *
 * 四条判定互不覆盖：未识别事件、原文未下发、结构化引用无内容、显式不支持。
 * 任何一条成立即需要降级卡片——MUST NOT 因为「还有其它可用信息」而静默隐藏。
 */
export function isDegraded(input: {
  readonly eventType: string;
  readonly view: Readonly<Record<string, unknown>>;
  readonly raw: RawAvailability;
  readonly error?: PublicError | null;
}): boolean {
  if (!isKnownEventType(input.eventType)) return true;
  if (input.raw.kind === "not_synced") return true;
  if (unsupportedByFromErrorCode(input.error?.code) !== null) return true;
  if (unsupportedByFromErrorCode(errorCodeInView(input.view)) !== null) return true;
  return referencesUndeliveredBinary(input.view);
}

/** 显式不支持时优先取事件视图里的说明，其次取公开错误的消息。 */
function unsupportedDetail(input: {
  readonly view: Readonly<Record<string, unknown>>;
  readonly error?: PublicError | null;
}): string {
  const message = input.error?.message;
  if (typeof message === "string" && message.length > 0) return message;
  const reason = input.view["reason"];
  return typeof reason === "string" ? reason : "该内容没有可用的呈现方式";
}

/**
 * 构造降级事件模型。
 *
 * 调用方只在 `isDegraded` 为真时调用：事件有专用视图时不产生降级卡片。
 */
export function buildDegradedEventModel(input: {
  readonly event: EventMessage;
  /** 关联的结构化错误（例如命令结果里的 `capability.unsupported_by_agent`）。 */
  readonly error?: PublicError | null;
}): DegradedEventModel {
  const view = input.event.body.payload.view;
  const raw = rawAvailability(input.event.body.payload.acp);

  const unsupported =
    unsupportedByFromErrorCode(input.error?.code) ?? unsupportedByFromErrorCode(errorCodeInView(view));
  let reason: DegradedReason;
  if (unsupported !== null) {
    reason = { kind: "unsupported", by: unsupported, detail: unsupportedDetail({ view, error: input.error ?? null }) };

  } else if (raw.kind === "not_synced") {
    reason = { kind: "raw_not_synced", reason: raw.reason };
  } else if (!isKnownEventType(input.event.body.eventType)) {
    reason = { kind: "no_dedicated_view" };
  } else {
    // 已知事件类型且原文可用：唯一剩下的降级理由是引用了未下发的二进制内容。
    reason = { kind: "binary_not_delivered", ...binaryReference(view) };
  }

  return {
    eventId: input.event.body.eventId,
    eventType: input.event.body.eventType,
    globalSequence: input.event.body.globalSequence,
    createdAt: input.event.body.createdAt,
    sessionId: input.event.body.sessionId,
    originKind: input.event.body.origin.kind,
    reason,
    raw,
    view,
  };
}
