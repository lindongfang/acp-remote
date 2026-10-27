/**
 * `schemas/sync/v1/attachment.schema.json` 的严格 TypeScript 镜像与运行期解码器。
 *
 * ## 为什么单独一份
 *
 * 附件传输是**独立已认证 WSS 连接**的帧族（`docs/SYNC_PROTOCOL.md` §3.4），不是业务消息
 * `message.schema.json` 的一支：它只在协商 `core.attachment-transfer.v1` 后启用，未协商的连接
 * 收到任何 `attachment.*` 帧都按协议错误关闭。把它塞进业务消息联合会让「这条连接到底协商了什么」
 * 从类型上不可见，因此这里单独建模、单独解码。
 *
 * ## 两份不变量
 *
 * 1. **偏移与长度一律是十进制字符串**（`^(0|[1-9][0-9]*)$`）：附件可以远超 JavaScript 的
 *    safe integer，客户端不得先把它们读成 number 再序列化回去（那会静默改写偏移）。
 * 2. **每一帧都重复 `transferId` 与会话/主体绑定**：连接复用不改变归属，跨会话或跨主体复用
 *    标识必须被拒绝（`attachment.binding_mismatch`）。
 *
 * ## 解码策略
 *
 * 与 `src/sync-client/wire.ts` 一样**不做宽容解析**：未知键、缺必填键、错误类型、越界长度都
 * 返回结构化的失败原因，而不是猜测、补默认值或静默丢弃。调用方据此关闭连接或回一条
 * `attachment.error`，绝不继续按半条帧处理。
 *
 * 本文件是纯计算（没有 Node/浏览器 API），因此协议 barrel 的运行期面可以直接导出它。
 */

import type {
  Base64Url,
  Base64Url32,
  DecimalString,
  ErrorCode,
  Timestamp,
  Uuid,
} from "./common";

/** 附件传输的 feature ID（`compatibility/features/v1/features.json` 的 sync 列表）。 */
export const ATTACHMENT_FEATURE = "core.attachment-transfer.v1";

/** 附件连接的 WSS 路径（`docs/SYNC_PROTOCOL.md` §3.4；与业务连接共用 subprotocol）。 */
export const ATTACHMENT_WS_PATH = "/sync/v1/attachments";

/** 单块原始数据的固定上限：32 KiB（v1 常量，不得由 `limits` 下调）。 */
export const ATTACHMENT_CHUNK_SIZE = 32768;

/** 单块 base64url 文本的最大长度：`ceil(32768 * 4 / 3)`。 */
export const ATTACHMENT_CHUNK_BASE64_MAX_LENGTH = 43691;

/** 每个传输同时最多一块未确认（`maxUnacknowledgedChunks`）。 */
export const ATTACHMENT_MAX_UNACKNOWLEDGED_CHUNKS = 1;

/** 单设备同时最多 2 个活动传输。 */
export const ATTACHMENT_MAX_ACTIVE_TRANSFERS_PER_DEVICE = 2;

/** 单节点同时最多 8 个活动传输。 */
export const ATTACHMENT_MAX_ACTIVE_TRANSFERS_PER_NODE = 8;

/** 清单每页最多 64 个 `transferId`（`manifestPageBody.transferIds.maxItems`）。 */
export const ATTACHMENT_MANIFEST_PAGE_SIZE = 64;

/** 每次 `attachment.read` 请求的固定读取上限（与单块原始数据同一档）。 */
export const ATTACHMENT_READ_MAX_LENGTH = ATTACHMENT_CHUNK_SIZE;

/** 帧名封闭词表：唯一机器定义是 `compatibility/commands/v1/commands.json` 的 `transportFrames`。 */
export const ATTACHMENT_FRAME_TYPES = [
  "attachment.begin",
  "attachment.chunk",
  "attachment.chunk_ack",
  "attachment.seal",
  "attachment.sealed",
  "attachment.status",
  "attachment.abort",
  "attachment.read",
  "attachment.read_chunk",
  "attachment.manifest_begin",
  "attachment.manifest_page",
  "attachment.manifest_sealed",
  "attachment.error",
] as const;

export type AttachmentFrameType = (typeof ATTACHMENT_FRAME_TYPES)[number];

/** v1 传输预算（`$defs.transferBudget` 的固定常量）。 */
export const ATTACHMENT_TRANSFER_BUDGET = {
  chunkSize: ATTACHMENT_CHUNK_SIZE,
  maxUnacknowledgedChunks: ATTACHMENT_MAX_UNACKNOWLEDGED_CHUNKS,
  maxActiveTransfersPerDevice: ATTACHMENT_MAX_ACTIVE_TRANSFERS_PER_DEVICE,
  maxActiveTransfersPerNode: ATTACHMENT_MAX_ACTIVE_TRANSFERS_PER_NODE,
} as const;

/** 传输主体：Sync 面只接受设备（`kind: "device"`）。 */
export interface AttachmentSubject {
  readonly kind: "device";
  readonly id: Uuid;
}

/** 封存后的传输状态（`statusBody.state`）。 */
export type AttachmentTransferState = "receiving" | "sealed" | "aborted" | "expired";

/** 中止原因封闭词表。 */
export type AttachmentAbortReason =
  | "client_cancelled"
  | "budget_exceeded"
  | "revoked"
  | "digest_mismatch";

/** 每帧共有的信封字段。 */
interface FrameBase<TType extends AttachmentFrameType, TBody> {
  readonly protocolVersion: 1;
  readonly type: TType;
  readonly messageId: Uuid;
  readonly connectionId: Uuid;
  readonly connectionSequence: DecimalString;
  readonly body: TBody;
}

/** `attachment.begin` body for a formal session upload. */
export interface AttachmentBeginBody {
  readonly transferId: Uuid;
  readonly sessionId: Uuid;
  readonly subject: AttachmentSubject;
  readonly totalLength: DecimalString;
  readonly chunkSize: 32768;
  readonly mediaType: string;
  readonly fileName: string;
}

export interface AttachmentBeginFrame
  extends FrameBase<"attachment.begin", AttachmentBeginBody> {}

export interface AttachmentChunkFrame
  extends FrameBase<"attachment.chunk", {
    readonly transferId: Uuid;
    readonly offset: DecimalString;
    /** 单块原始数据（非填充 base64url，≤ 32768 字节）。 */
    readonly data: Base64Url;
  }> {}

export interface AttachmentChunkAckFrame
  extends FrameBase<"attachment.chunk_ack", {
    readonly transferId: Uuid;
    readonly offset: DecimalString;
    readonly length: DecimalString;
    /** `true` = 该偏移同内容已接受过，本次返回原确认。 */
    readonly duplicate: boolean;
    readonly nextOffset: DecimalString;
  }> {}

export interface AttachmentSealFrame
  extends FrameBase<"attachment.seal", {
    readonly transferId: Uuid;
    readonly length: DecimalString;
    readonly sha256: Base64Url32;
  }> {}

export interface AttachmentSealedFrame
  extends FrameBase<"attachment.sealed", {
    readonly transferId: Uuid;
    readonly attachmentId: Uuid;
    readonly length: DecimalString;
    readonly sha256: Base64Url32;
    readonly sealedAt: Timestamp;
  }> {}

export interface AttachmentStatusFrame
  extends FrameBase<"attachment.status", {
    readonly transferId: Uuid;
    readonly state: AttachmentTransferState;
    readonly receivedLength: DecimalString;
    readonly nextOffset: DecimalString;
    readonly sealedSha256: Base64Url32 | null;
  }> {}

export interface AttachmentAbortFrame
  extends FrameBase<"attachment.abort", {
    readonly transferId: Uuid;
    readonly reason: AttachmentAbortReason;
  }> {}

export interface AttachmentReadFrame
  extends FrameBase<"attachment.read", {
    readonly attachmentId: Uuid;
    readonly sessionId: Uuid;
    readonly offset: DecimalString;
    readonly maxLength: DecimalString;
  }> {}

export interface AttachmentReadChunkFrame
  extends FrameBase<"attachment.read_chunk", {
    readonly attachmentId: Uuid;
    readonly sessionId: Uuid;
    readonly offset: DecimalString;
    readonly length: DecimalString;
    readonly totalLength: DecimalString;
    readonly data: Base64Url;
    readonly eof: boolean;
  }> {}

/** `attachment.manifest_begin` body for a formal session attachment manifest. */
export interface AttachmentManifestBeginBody {
  readonly submissionId: Uuid;
  readonly sessionId: Uuid;
  readonly subject: AttachmentSubject;
  readonly attachmentCount: DecimalString;
}

export interface AttachmentManifestBeginFrame
  extends FrameBase<"attachment.manifest_begin", AttachmentManifestBeginBody> {}

export interface AttachmentManifestPageFrame
  extends FrameBase<"attachment.manifest_page", {
    readonly submissionId: Uuid;
    readonly transferIds: readonly Uuid[];
    readonly final: boolean;
  }> {}

export interface AttachmentManifestSealedFrame
  extends FrameBase<"attachment.manifest_sealed", {
    readonly submissionId: Uuid;
    readonly attachmentCount: DecimalString;
    readonly manifestDigest: Base64Url32;
    readonly sealedAt: Timestamp;
  }> {}

export interface AttachmentErrorFrame
  extends FrameBase<"attachment.error", {
    readonly code: ErrorCode;
    readonly message: string;
    readonly retryable: boolean;
    readonly transferId: Uuid | null;
    readonly details: Record<string, unknown>;
  }> {}

/** 附件帧联合（`attachment.schema.json` 的顶层 `oneOf`）。 */
export type AttachmentFrame =
  | AttachmentBeginFrame
  | AttachmentChunkFrame
  | AttachmentChunkAckFrame
  | AttachmentSealFrame
  | AttachmentSealedFrame
  | AttachmentStatusFrame
  | AttachmentAbortFrame
  | AttachmentReadFrame
  | AttachmentReadChunkFrame
  | AttachmentManifestBeginFrame
  | AttachmentManifestPageFrame
  | AttachmentManifestSealedFrame
  | AttachmentErrorFrame;

/** 解码失败原因：封闭词表，便于调用方与测试区分「拒在哪一层」。 */
export type AttachmentFrameInvalidReason =
  | "not_object"
  | "protocol_version"
  | "envelope"
  | "unknown_type"
  | "body"
  | "uuid"
  | "decimal"
  | "base64url"
  | "subject"
  | "budget";

export type AttachmentFrameDecode =
  | { readonly ok: true; readonly frame: AttachmentFrame }
  | { readonly ok: false; readonly reason: AttachmentFrameInvalidReason; readonly detail: string };

/** UUID 文本形状（`common.schema.json#/$defs/uuid`）。 */
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
/** 无前导零十进制字符串。 */
const DECIMAL_PATTERN = /^(0|[1-9][0-9]*)$/;
/** 无填充 base64url。 */
const BASE64URL_PATTERN = /^[A-Za-z0-9_-]+$/;
/** 时间戳固定形状（毫秒 UTC）。 */
const TIMESTAMP_PATTERN = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/;

function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const prototype: unknown = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function isUuid(value: unknown): value is string {
  return typeof value === "string" && UUID_PATTERN.test(value);
}

function isDecimal(value: unknown): value is string {
  return typeof value === "string" && DECIMAL_PATTERN.test(value);
}

/** `$defs.positiveDecimalString`：`1..=2^63−1` 的无前导零十进制字符串（`"0"` 不合法）。 */
function isBase64Url(value: unknown, maxLength: number): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= maxLength && BASE64URL_PATTERN.test(value);
}

function isTimestamp(value: unknown): value is Timestamp {
  return typeof value === "string" && TIMESTAMP_PATTERN.test(value);
}

function hasOnlyKeys(value: Record<string, unknown>, allowed: readonly string[]): boolean {
  const set = new Set(allowed);
  return Object.keys(value).every((key) => set.has(key));
}

/** 必填键都在场（值类型由各帧的读取器继续确认）。 */
function hasKeys(value: Record<string, unknown>, required: readonly string[]): boolean {
  return required.every((key) => Object.hasOwn(value, key));
}

/**
 * 解一帧附件消息。
 *
 * `body` 用**公开键集合**白名单逐一确认（等价于 schema 的 `additionalProperties: false`），
 * 因而未知键不会被静默保留成「未来字段」——这些帧是封闭合同，未协商的字段本来就不允许出现。
 */
export function decodeAttachmentFrame(value: unknown): AttachmentFrameDecode {
  if (!isPlainObject(value)) {
    return { ok: false, reason: "not_object", detail: "附件帧必须是 JSON 对象" };
  }
  if (value["protocolVersion"] !== 1) {
    return { ok: false, reason: "protocol_version", detail: "protocolVersion 必须为 1" };
  }
  if (
    !isUuid(value["messageId"]) ||
    !isUuid(value["connectionId"]) ||
    !isDecimal(value["connectionSequence"])
  ) {
    return { ok: false, reason: "envelope", detail: "messageId/connectionId/connectionSequence 形状不符" };
  }
  const type = value["type"];
  if (typeof type !== "string" || !(ATTACHMENT_FRAME_TYPES as readonly string[]).includes(type)) {
    return { ok: false, reason: "unknown_type", detail: `未知附件帧类型：${String(type)}` };
  }
  if (!isPlainObject(value["body"])) {
    return { ok: false, reason: "body", detail: "body 必须是对象" };
  }
  if (!hasOnlyKeys(value, ["protocolVersion", "type", "messageId", "connectionId", "connectionSequence", "body"])) {
    return { ok: false, reason: "envelope", detail: "信封含未知键" };
  }
  const body = value["body"];
  const frame = decodeBody(type as AttachmentFrameType, body);
  if (!frame.ok) return frame;
  return {
    ok: true,
    frame: {
      protocolVersion: 1,
      type,
      messageId: value["messageId"],
      connectionId: value["connectionId"],
      connectionSequence: value["connectionSequence"],
      body: frame.body,
    } as unknown as AttachmentFrame,
  };
}

type BodyDecode =
  | { readonly ok: true; readonly body: Record<string, unknown> }
  | { readonly ok: false; readonly reason: AttachmentFrameInvalidReason; readonly detail: string };

function invalidBody(detail: string): BodyDecode {
  return { ok: false, reason: "body", detail };
}

function decodeBody(type: AttachmentFrameType, body: Record<string, unknown>): BodyDecode {
  switch (type) {
    case "attachment.begin": {
      const shared = ["transferId", "subject", "totalLength", "chunkSize", "mediaType", "fileName"];
      if (!hasKeys(body, shared) || !hasOnlyKeys(body, [...shared, "sessionId"])) {
        return invalidBody("attachment.begin body keys");
      }
      if (!isUuid(body["transferId"])) return { ok: false, reason: "uuid", detail: "transferId must be UUID" };
      if (!isUuid(body["sessionId"])) return { ok: false, reason: "uuid", detail: "sessionId must be UUID" };
      const subject = decodeSubject(body["subject"]);
      if (!subject.ok) return subject;
      if (!isDecimal(body["totalLength"])) {
        return { ok: false, reason: "decimal", detail: "totalLength must be decimal" };
      }
      if (body["chunkSize"] !== ATTACHMENT_CHUNK_SIZE) {
        return { ok: false, reason: "budget", detail: `chunkSize must be ${ATTACHMENT_CHUNK_SIZE}` };
      }
      if (
        typeof body["mediaType"] !== "string" ||
        body["mediaType"].length === 0 ||
        body["mediaType"].length > 255 ||
        typeof body["fileName"] !== "string" ||
        body["fileName"].length === 0 ||
        body["fileName"].length > 255
      ) {
        return invalidBody("mediaType/fileName length must be 1..=255");
      }
      return { ok: true, body };
    }
    case "attachment.chunk": {
      if (!hasKeys(body, ["transferId", "offset", "data"]) || !hasOnlyKeys(body, ["transferId", "offset", "data"])) {
        return invalidBody("attachment.chunk 的键集合不符");
      }
      if (!isUuid(body["transferId"])) return { ok: false, reason: "uuid", detail: "transferId 必须是 UUID" };
      if (!isDecimal(body["offset"])) {
        return { ok: false, reason: "decimal", detail: "offset 必须是十进制字符串" };
      }
      if (!isBase64Url(body["data"], ATTACHMENT_CHUNK_BASE64_MAX_LENGTH)) {
        return { ok: false, reason: "base64url", detail: "data 必须是单块原始数据的 base64url" };
      }
      return { ok: true, body };
    }
    case "attachment.chunk_ack": {
      if (
        !hasKeys(body, ["transferId", "offset", "length", "duplicate", "nextOffset"]) ||
        !hasOnlyKeys(body, ["transferId", "offset", "length", "duplicate", "nextOffset"])
      ) {
        return invalidBody("attachment.chunk_ack 的键集合不符");
      }
      if (!isUuid(body["transferId"])) return { ok: false, reason: "uuid", detail: "transferId 必须是 UUID" };
      if (!isDecimal(body["offset"]) || !isDecimal(body["length"]) || !isDecimal(body["nextOffset"])) {
        return { ok: false, reason: "decimal", detail: "offset/length/nextOffset 必须是十进制字符串" };
      }
      if (typeof body["duplicate"] !== "boolean") return invalidBody("duplicate 必须是布尔值");
      return { ok: true, body };
    }
    case "attachment.seal": {
      if (
        !hasKeys(body, ["transferId", "length", "sha256"]) ||
        !hasOnlyKeys(body, ["transferId", "length", "sha256"])
      ) {
        return invalidBody("attachment.seal 的键集合不符");
      }
      if (!isUuid(body["transferId"])) return { ok: false, reason: "uuid", detail: "transferId 必须是 UUID" };
      if (!isDecimal(body["length"])) {
        return { ok: false, reason: "decimal", detail: "length 必须是十进制字符串" };
      }
      if (!isBase64Url(body["sha256"], 43) || body["sha256"].length !== 43) {
        return { ok: false, reason: "base64url", detail: "sha256 必须是 43 字符 base64url" };
      }
      return { ok: true, body };
    }
    case "attachment.sealed": {
      if (
        !hasKeys(body, ["transferId", "attachmentId", "length", "sha256", "sealedAt"]) ||
        !hasOnlyKeys(body, ["transferId", "attachmentId", "length", "sha256", "sealedAt"])
      ) {
        return invalidBody("attachment.sealed 的键集合不符");
      }
      if (!isUuid(body["transferId"])) return { ok: false, reason: "uuid", detail: "transferId 必须是 UUID" };
      if (!isUuid(body["attachmentId"])) return { ok: false, reason: "uuid", detail: "attachmentId 必须是 UUID" };
      if (!isDecimal(body["length"])) {
        return { ok: false, reason: "decimal", detail: "length 必须是十进制字符串" };
      }
      if (!isBase64Url(body["sha256"], 43) || body["sha256"].length !== 43) {
        return { ok: false, reason: "base64url", detail: "sha256 必须是 43 字符 base64url" };
      }
      if (!isTimestamp(body["sealedAt"])) return invalidBody("sealedAt 必须是 UTC 毫秒时间戳");
      return { ok: true, body };
    }
    case "attachment.status": {
      if (
        !hasKeys(body, ["transferId", "state", "receivedLength", "nextOffset", "sealedSha256"]) ||
        !hasOnlyKeys(body, ["transferId", "state", "receivedLength", "nextOffset", "sealedSha256"])
      ) {
        return invalidBody("attachment.status 的键集合不符");
      }
      if (!isUuid(body["transferId"])) return { ok: false, reason: "uuid", detail: "transferId 必须是 UUID" };
      if (!["receiving", "sealed", "aborted", "expired"].includes(String(body["state"]))) {
        return invalidBody("state 必须是 receiving/sealed/aborted/expired");
      }
      if (!isDecimal(body["receivedLength"]) || !isDecimal(body["nextOffset"])) {
        return { ok: false, reason: "decimal", detail: "receivedLength/nextOffset 必须是十进制字符串" };
      }
      const digest = body["sealedSha256"];
      if (digest !== null && (!isBase64Url(digest, 43) || digest.length !== 43)) {
        return { ok: false, reason: "base64url", detail: "sealedSha256 必须是 43 字符 base64url 或 null" };
      }
      return { ok: true, body };
    }
    case "attachment.abort": {
      if (
        !hasKeys(body, ["transferId", "reason"]) ||
        !hasOnlyKeys(body, ["transferId", "reason"])
      ) {
        return invalidBody("attachment.abort 的键集合不符");
      }
      if (!isUuid(body["transferId"])) return { ok: false, reason: "uuid", detail: "transferId 必须是 UUID" };
      if (!["client_cancelled", "budget_exceeded", "revoked", "digest_mismatch"].includes(String(body["reason"]))) {
        return invalidBody("reason 不在封闭词表里");
      }
      return { ok: true, body };
    }
    case "attachment.read": {
      if (
        !hasKeys(body, ["attachmentId", "sessionId", "offset", "maxLength"]) ||
        !hasOnlyKeys(body, ["attachmentId", "sessionId", "offset", "maxLength"])
      ) {
        return invalidBody("attachment.read 的键集合不符");
      }
      if (!isUuid(body["attachmentId"]) || !isUuid(body["sessionId"])) {
        return { ok: false, reason: "uuid", detail: "attachmentId/sessionId 必须是 UUID" };
      }
      if (!isDecimal(body["offset"]) || !isDecimal(body["maxLength"])) {
        return { ok: false, reason: "decimal", detail: "offset/maxLength 必须是十进制字符串" };
      }
      return { ok: true, body };
    }
    case "attachment.read_chunk": {
      if (
        !hasKeys(body, ["attachmentId", "sessionId", "offset", "length", "totalLength", "data", "eof"]) ||
        !hasOnlyKeys(body, ["attachmentId", "sessionId", "offset", "length", "totalLength", "data", "eof"])
      ) {
        return invalidBody("attachment.read_chunk 的键集合不符");
      }
      if (!isUuid(body["attachmentId"]) || !isUuid(body["sessionId"])) {
        return { ok: false, reason: "uuid", detail: "attachmentId/sessionId 必须是 UUID" };
      }
      if (!isDecimal(body["offset"]) || !isDecimal(body["length"]) || !isDecimal(body["totalLength"])) {
        return { ok: false, reason: "decimal", detail: "offset/length/totalLength 必须是十进制字符串" };
      }
      if (!isBase64Url(body["data"], ATTACHMENT_CHUNK_BASE64_MAX_LENGTH)) {
        return { ok: false, reason: "base64url", detail: "data 必须是单块原始数据的 base64url" };
      }
      if (typeof body["eof"] !== "boolean") return invalidBody("eof 必须是布尔值");
      return { ok: true, body };
    }
    case "attachment.manifest_begin": {
      const shared = ["submissionId", "subject", "attachmentCount"];
      if (!hasKeys(body, shared) || !hasOnlyKeys(body, [...shared, "sessionId"])) {
        return invalidBody("attachment.manifest_begin body keys");
      }
      if (!isUuid(body["submissionId"])) {
        return { ok: false, reason: "uuid", detail: "submissionId must be UUID" };
      }
      if (!isUuid(body["sessionId"])) {
        return { ok: false, reason: "uuid", detail: "sessionId must be UUID" };
      }
      const subject = decodeSubject(body["subject"]);
      if (!subject.ok) return subject;
      if (!isDecimal(body["attachmentCount"])) {
        return { ok: false, reason: "decimal", detail: "attachmentCount must be decimal" };
      }
      return { ok: true, body };
    }
    case "attachment.manifest_page": {
      if (
        !hasKeys(body, ["submissionId", "transferIds", "final"]) ||
        !hasOnlyKeys(body, ["submissionId", "transferIds", "final"])
      ) {
        return invalidBody("attachment.manifest_page 的键集合不符");
      }
      if (!isUuid(body["submissionId"])) {
        return { ok: false, reason: "uuid", detail: "submissionId 必须是 UUID" };
      }
      const ids = body["transferIds"];
      if (
        !Array.isArray(ids) ||
        ids.length === 0 ||
        ids.length > ATTACHMENT_MANIFEST_PAGE_SIZE ||
        !ids.every((id) => isUuid(id))
      ) {
        return invalidBody(`transferIds 必须是 1..=${ATTACHMENT_MANIFEST_PAGE_SIZE} 个 UUID`);
      }
      if (new Set(ids).size !== ids.length) return invalidBody("transferIds 必须去重");
      if (typeof body["final"] !== "boolean") return invalidBody("final 必须是布尔值");
      return { ok: true, body };
    }
    case "attachment.manifest_sealed": {
      if (
        !hasKeys(body, ["submissionId", "attachmentCount", "manifestDigest", "sealedAt"]) ||
        !hasOnlyKeys(body, ["submissionId", "attachmentCount", "manifestDigest", "sealedAt"])
      ) {
        return invalidBody("attachment.manifest_sealed 的键集合不符");
      }
      if (!isUuid(body["submissionId"])) {
        return { ok: false, reason: "uuid", detail: "submissionId 必须是 UUID" };
      }
      if (!isDecimal(body["attachmentCount"])) {
        return { ok: false, reason: "decimal", detail: "attachmentCount 必须是十进制字符串" };
      }
      if (!isBase64Url(body["manifestDigest"], 43) || body["manifestDigest"].length !== 43) {
        return { ok: false, reason: "base64url", detail: "manifestDigest 必须是 43 字符 base64url" };
      }
      if (!isTimestamp(body["sealedAt"])) return invalidBody("sealedAt 必须是 UTC 毫秒时间戳");
      return { ok: true, body };
    }
    case "attachment.error": {
      if (
        !hasKeys(body, ["code", "message", "retryable", "transferId", "details"]) ||
        !hasOnlyKeys(body, ["code", "message", "retryable", "transferId", "submissionId", "details"])
      ) {
        return invalidBody("attachment.error 的键集合不符");
      }
      if (typeof body["code"] !== "string" || body["code"].length === 0) {
        return invalidBody("code 必须是非空字符串");
      }
      if (typeof body["message"] !== "string" || body["message"].length > 1024) {
        return invalidBody("message 必须是至多 1024 字符的字符串");
      }
      if (typeof body["retryable"] !== "boolean") return invalidBody("retryable 必须是布尔值");
      if (body["transferId"] !== null && !isUuid(body["transferId"])) {
        return { ok: false, reason: "uuid", detail: "transferId 必须是 UUID 或 null" };
      }
      // 可选键：缺席表示「未核实该清单」；在场时必须是 UUID，不能用 `null` 冒充。
      if (Object.hasOwn(body, "submissionId") && !isUuid(body["submissionId"])) {
        return { ok: false, reason: "uuid", detail: "submissionId 必须是 UUID" };
      }
      if (!isPlainObject(body["details"])) return invalidBody("details 必须是对象");
      return { ok: true, body };
    }
    default:
      return { ok: false, reason: "unknown_type", detail: "未知附件帧类型" };
  }
}

function decodeSubject(value: unknown): BodyDecode {
  if (!isPlainObject(value) || !hasOnlyKeys(value, ["kind", "id"])) {
    return invalidBody("subject 必须是只含 kind/id 的对象");
  }
  if (value["kind"] !== "device") {
    return { ok: false, reason: "subject", detail: "Sync 面的附件主体只能是 device" };
  }
  if (!isUuid(value["id"])) return { ok: false, reason: "uuid", detail: "subject.id 必须是 UUID" };
  return { ok: true, body: value };
}

/**
 * 单块原始数据 → 无填充 base64url。
 *
 * 附件帧的 `data` 是原始字节，不是文本；编码只在传输边界发生（业务视图里没有 base64）。
 */
export function encodeAttachmentChunk(data: Uint8Array): Base64Url {
  if (typeof btoa !== "function") {
    throw new Error("当前运行时没有 base64 编码能力（btoa）");
  }
  let binary = "";
  for (const byte of data) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}

/** 无填充 base64url → 原始字节；形状不合法时抛错（调用方应先用解码器确认帧形状）。 */
export function decodeAttachmentChunk(data: Base64Url): Uint8Array {
  if (!BASE64URL_PATTERN.test(data)) throw new Error("data 不是无填充 base64url");
  if (typeof atob !== "function") {
    throw new Error("当前运行时没有 base64 解码能力（atob）");
  }
  const base64 = data.replaceAll("-", "+").replaceAll("_", "/");
  const padded = base64 + "=".repeat((4 - (base64.length % 4)) % 4);
  const binary = atob(padded);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
}
