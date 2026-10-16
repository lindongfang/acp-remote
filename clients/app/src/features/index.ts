/**
 * feature 控制器层（`docs/FRONTEND_DESIGN.md` §3 的 `features/`）。
 *
 * **归属 WP7**（`plan.md` 的 WP7 行）：R15–R19 的控制器、视图模型与状态仓库都在这一层。
 *
 * 该层可以消费 `src/domain/` 的视图模型与 `src/state/` 的状态类型，
 * 但 MUST NOT 直接触碰 WebSocket/IndexedDB/CryptoKey（`AGENTS.md` §6）——
 * 它对同步客户端的依赖面被收窄成 `ports.ts` 的三个端口。
 */

import type {
  AgentCatalogView,
  DirectoryListState,
  SessionReadPageView,
  SessionSummaryView,
} from "../domain";

/**
 * feature 层消费的视图模型面。
 *
 * 这是一份**只读**的契约导出口：WP7 的各 feature 从这里取类型，
 * 而不是各自去 import `src/protocol/` 的原始字段——这正是
 * `pwa-web-client` 的「视图模型不暴露协议载荷」在结构上的落点。
 */
export interface FeatureViewModels {
  readonly directories: DirectoryListState;
  readonly sessions: readonly SessionSummaryView[];
  readonly agents: AgentCatalogView;
  readonly conversation: SessionReadPageView;
}

// ── 端口 ────────────────────────────────────────────────────────────────────
export type { CommandGateway, ConnectionIdentity, IdentifierSource, SnapshotGateway, SyncGateway } from "./ports";

// ── 状态仓库 ────────────────────────────────────────────────────────────────
export type {
  ClientState,
  ClientStoreOptions,
  CommandOutcome,
  ConversationThreadState,
  CreateOutcome,
} from "./client-store";
export { ClientStore, initialClientState } from "./client-store";
export { ClientStoreProvider, useClientState, useClientStore } from "./runtime";

// ── 呈现覆盖（D5：由客户端自持）─────────────────────────────────────────────
export type { PresentationRoute } from "./presentation-coverage";
export {
  PRESENTATION_COVERAGE,
  TIMELINE_CARD_EVENT_TYPES,
  UNCOVERED_EVENT_TYPES,
  hasDedicatedView,
  presentationRouteFor,
} from "./presentation-coverage";

// ── 时间线与易失叠加层（D1/D2/D6）──────────────────────────────────────────
export type {
  CardTimelineEntry,
  DegradedTimelineEntry,
  FileChangeDiagnostics,
  FileChangeView,
  MessageChannel,
  MessageTimelineEntry,
  TimelineEntry,
  TimelineEntryBase,
  VolatileMessageState,
} from "./conversation-timeline";
export {
  appendDelta,
  buildFileChangeViews,
  buildTimeline,
  deltaChannelOf,
  volatileText,
} from "./conversation-timeline";

// ── 交互应答（D7）──────────────────────────────────────────────────────────
export { buildInteractionViews } from "./interaction-model";

// ── 输入/取消与设置（D5/D6）────────────────────────────────────────────────
export type {
  CancelPresentation,
  ConversationInputModel,
  ConversationInputState,
  SubmissionPresentation,
} from "./conversation-input-model";
export { buildConversationInputModel } from "./conversation-input-model";
export type { ConversationSettingsModel, SettingsChoice, SettingsSectionModel } from "./conversation-settings-model";
export { buildConversationSettingsModel, modelConfigOption } from "./conversation-settings-model";

// ── R15 目录 ────────────────────────────────────────────────────────────────
export type {
  DirectoryContentOrigin,
  DirectoryCountsModel,
  DirectoryDetailModel,
  DirectoryFilter,
  DirectoryMonthModel,
  DirectoryPageModel,
  DirectoryRowModel,
  SessionRowModel,
} from "./directory-model";
export {
  CONTENT_ORIGIN_LABELS,
  DIRECTORY_EMPTY_COPY,
  buildDirectoryDetailModel,
  buildDirectoryPageModel,
  buildSessionRowsByAlias,
  monthKeyOf,
} from "./directory-model";

// ── R16 会话创建 ────────────────────────────────────────────────────────────
export type { CreateSessionEntryModel, CreateSessionRefs, CreateSubmissionModel } from "./create-session-model";
export {
  CREATE_DIRECTORY_HINT,
  CREATE_EFFECT_NOTE,
  CreateSessionPayloadViolation,
  assertCreateSessionRefs,
  buildCreateSessionCommand,
  buildCreateSessionEntryModel,
  createSubmissionModel,
  isSubmissionAvailable,
  isSubmissionTerminal,
} from "./create-session-model";

// ── R17/R18 对话与降级 ──────────────────────────────────────────────────────
export type {
  ConversationCapabilitiesModel,
  ConversationHeaderModel,
  ConversationMessageModel,
  ConversationPageModel,
  ConversationPagingModel,
  ConversationTimelineModel,
  UsageReading,
} from "./conversation-model";
export {
  buildConversationModel,
  conversationCapabilities,
  nextBeforeCursor,
  structuredHistoryAvailability,
} from "./conversation-model";
export type { DegradedEventModel, DegradedReason, RawAvailability, RawNotSyncedReason, UnsupportedBy } from "./degradation-model";
export {
  RAW_NOT_SYNCED_LABELS,
  UNSUPPORTED_LABELS,
  buildDegradedEventModel,
  buildDegradedRecordModel,
  isDegraded,
  isDegradedRecord,
  rawAvailabilityOf,
} from "./degradation-model";

// ── R19 主机与连接 ──────────────────────────────────────────────────────────
export type { AgentRowModel, HostIdentityModel, HostPanelModel } from "./host-panel-model";
export { CONNECTION_STATE_LABELS, UNKNOWN_STATE_NOTE, buildHostPanelModel } from "./host-panel-model";

// ── 配对 ────────────────────────────────────────────────────────────────────
export type {
  PairingKind,
  PairingPageModel,
  PairingPhase,
  PairingPollResult,
  PairingQrRead,
  PairingTransport,
} from "./pairing-model";
export {
  PAIRING_ERROR_HINTS,
  applyClaiming,
  applyPollResult,
  applyScannedNodeLink,
  applyScannedQr,
  canonicalOriginOf,
  initialPairingPageModel,
  insecureContextFailure,
  kindMismatchFailure,
  originMismatchFailure,
} from "./pairing-model";
