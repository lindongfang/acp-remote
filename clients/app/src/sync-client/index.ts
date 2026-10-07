/**
 * 同步客户端层（`docs/FRONTEND_DESIGN.md` §3 的 `sync-client/`）。
 *
 * **归属 WP6**（`plan.md` 的 WP6 行）：连接、逐连接签名握手、`subscribe`、ACK、
 * 增量重放、快照暂存与原子替换、`eventId` 去重、`requestId` 幂等重试都由 WP6 实现。
 * WP5a 固定了这一层的**位置**、依赖方向与四个类型别名；WP6 在此之上补齐实现。
 *
 * 本文件是该层的**唯一公开面**：组合根（WP7 的 `app/`）只从这里取端口与门面，
 * 其它层不直接 import 内部模块。
 */

import type { Cursor, EventMessage, SyncWireMessage } from "../protocol";

/** 已确认的全局游标（ACK 的语义载体）。 */
export type GlobalCursor = Cursor;

/** 从 wire 收到的持久化事件。 */
export type IncomingEvent = EventMessage;

/** 任意一条 WSS 消息（解码前反而是 `unknown`；WP6 负责严格解码）。 */
export type WireMessage = SyncWireMessage;

/**
 * 事件去重的键：稳定 `eventId`（`AGENTS.md` §3「客户端按稳定 `eventId` 去重」）。
 * 单独命名以避免 WP6 用 `messageId` 或连接序号误作去重键。
 */
export type EventDedupKey = EventMessage["body"]["eventId"];

// ── 端口（平台能力由组合根注入，见 `ports.ts` 的说明）──────────────────────
export type {
  ByteArray,
  CancelTimer,
  ClockPort,
  DeviceIdentityLike,
  DeviceIdentityMaterial,
  DigestPort,
  HostChallengeInput,
  HostIdentityPort,
  PairedHostRecord,
  RandomPort,
  SocketFactory,
  SocketHandlers,
  SocketPort,
  TranscriptCodec,
  TranscriptField,
} from "./ports";

// ── 连接与握手 ─────────────────────────────────────────────────────────────
export type { AuthenticatedInfo, ClientBlockCause, SyncClientEvent, SyncConnectionOptions } from "./connection";
export {
  REQUIRED_FEATURES,
  SUPPORTED_FEATURES,
  SyncConnection,
  blockCauseForCloseCode,
  compareCursors,
  isRetryableCloseCode,
  sortedFeatures,
} from "./connection";
export { SyncClient, SnapshotRepository } from "./client";
export type { CompletedSnapshot, DispatchRecord, SyncClientOptions } from "./client";
export { openWebSocket } from "./socket.web";

// ── 解码 ───────────────────────────────────────────────────────────────────
export type { WireDecodeResult, WireDecoded, WireRejected, WireRejectReason } from "./wire";
export { cursorsEqual, decodeWireMessage, isCommandStatusRecord, isCursor, isErrorCode } from "./wire";
export { Base64UrlError, bytesEqual, decodeBase64Url, encodeBase64Url } from "./base64url";

// ── 去重、快照与分页 ───────────────────────────────────────────────────────
export { EventLedger, isImportedEventIdConsistent, SEQUENCE_UPPER_BOUND } from "./dedupe";
export type { BarrierVerdict, EventVerdict, SequenceRejection } from "./dedupe";
export { SnapshotStaging, SnapshotValidationError } from "./snapshot";
export type { SnapshotDiscardReason, StagedResources, StagedSnapshot, VerifiedSnapshot } from "./snapshot";
export {
  IncompleteReadCursorError,
  assertPagesDoNotOverlap,
  buildReadPayload,
  cursorOf,
  earliestCursor,
  toLoadedPage,
} from "./paging";
export type { LoadedPage, PageRequest, ReadCursor } from "./paging";

// ── Agent 覆盖层与 imported 落盘分流 ────────────────────────────────────────
export { AgentConnectionOverlayStore, overlayFromEvent } from "./agent-overlay";
export type { ContentKind, ContentRoute, SessionOrigin } from "./imported-content";
export { mayMarkInputAsSent, routeContent } from "./imported-content";