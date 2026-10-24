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
export type {
  AttachmentFileHandle,
  AttachmentMediaPort,
  AttachmentTransferGateway,
  CommandGateway,
  ConnectionIdentity,
  IdentifierSource,
  SnapshotGateway,
  SyncGateway,
} from "./ports";

export type {
  AttachmentBatchModel,
  AttachmentCapability,
  AttachmentKind,
  AttachmentUploadStatus,
  DraftAttachment,
} from "./attachment-model";
export {
  attachmentCapabilityOf,
  buildAttachmentBatch,
  isWorkspacePathRef,
  withUploadedBytes,
} from "./attachment-model";

// ── 状态仓库 ────────────────────────────────────────────────────────────────
export type {
  AttachmentMediaState,
  AttachmentRunState,
  ClientState,
  ClientStoreOptions,
  CommandOutcome,
  ConnectionCloseRecord,
  ConversationThreadState,
  CreateOutcome,
  ReconnectProgress,
  SessionLifecycleState,
} from "./client-store";
export { ClientStore, initialClientState, initialSessionLifecycleState } from "./client-store";
export type {
  ActivationFailure,
  ActivationFailureKind,
  ActivationPhase,
  SessionActivationState,
  SessionActivationView,
  SessionPageEvent,
  SessionViewDeclaration,
} from "./session-lifecycle";
export {
  ACTIVATION_INACTIVE_NOTICE,
  ACTIVATION_PROGRESS_NOTICE,
  ACTIVATION_UNAVAILABLE_NOTICE,
  SESSION_LIFECYCLE_FEATURE,
  SESSION_VIEW_RENEW_INTERVAL_MS,
  activationViewOf,
  classifyActivationFailure,
  nextViewVersion,
} from "./session-lifecycle";
export { ClientStoreProvider, useClientState, useClientStore } from "./runtime";

// ── 连接问题（顶栏只分在线/离线，阻断原因由弹层承担）────────────────────────
export type {
  ConnectionAvailability,
  ConnectionProblemAction,
  ConnectionProblemInput,
  ConnectionProblemKind,
  ConnectionProblemModel,
} from "./connection-problem";
export {
  buildConnectionProblem,
  networkAvailabilityOf,
  shouldShowConnectionProblem,
} from "./connection-problem";

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
  LocalPromptState,
  MessageChannel,
  MessageTimelineEntry,
  SentPromptPhase,
  SentPromptView,
  TimelineEntry,
  TimelineEntryBase,
  TimelineTurnGroup,
  VolatileMessageState,
} from "./conversation-timeline";
export {
  appendDelta,
  buildFileChangeViews,
  buildTimeline,
  deltaChannelOf,
  groupTimelineEntries,
  residualVolatileText,
  volatileText,
} from "./conversation-timeline";

// ── 过程阶段（D3：提交中/等待/恢复与思考/工具/回复互斥）────────────────────
export type {
  ContentPhase,
  ConversationProcessModel,
  ConversationProcessPhase,
} from "./conversation-process-model";
export { buildConversationProcess } from "./conversation-process-model";

// ── 交互应答（D7）──────────────────────────────────────────────────────────
export { buildInteractionViews } from "./interaction-model";
export type { PendingInteractionInput } from "./interaction-model";

// ── 输入/取消与设置（D5/D6）────────────────────────────────────────────────
export type {
  CancelPresentation,
  ConversationInputBlock,
  ConversationInputModel,
  ConversationInputState,
  SubmissionPresentation,
} from "./conversation-input-model";
export { buildConversationInputModel } from "./conversation-input-model";
export type { ConversationSettingsModel, SettingsChoice, SettingsSectionModel } from "./conversation-settings-model";
export { buildConversationSettingsModel, modelConfigOption } from "./conversation-settings-model";

// ── R15 目录 ────────────────────────────────────────────────────────────────
export type {
  DirectoryCountsModel,
  DirectoryDetailModel,
  DirectoryFilter,
  DirectoryPageModel,
  DirectoryRowModel,
  SessionRowModel,
} from "./directory-model";
export {
  DIRECTORY_DETAIL_PAGE_SIZE,
  DIRECTORY_EMPTY_COPY,
  buildDirectoryDetailModel,
  buildDirectoryPageModel,
  buildSessionRowsByDirectoryId,
  correctPageNumber,
  filterSessionsForDirectory,
  formatRelativeTime,
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
  ConversationColumnName,
  ConversationHeaderModel,
  ConversationMessageModel,
  ConversationPageModel,
  ConversationPagingModel,
  ConversationSecondaryView,
  ConversationSidebarDirectoryModel,
  ConversationSidebarModel,
  ConversationTimelineModel,
  UsageReading,
} from "./conversation-model";
export {
  CONVERSATION_NO_DIRECTORY_NOTICE,
  CONVERSATION_SECONDARY_VIEWS,
  CONVERSATION_SIDEBAR_PAGE_SIZE,
  DEFAULT_CONVERSATION_SECONDARY_VIEW,
  buildConversationModel,
  buildConversationSidebar,
  conversationCapabilities,
  hiddenConversationColumns,
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

// ── 文件栏两来源与互斥内容态 ──────────────────────────────────────────────
export type {
  AllFilesSourceState,
  FileContentState,
  FileContentView,
  FileGitDiffContent,
  FilePreviewContent,
  FileSource,
  FileViewContext,
  GitChangesSourceState,
  SessionFilesPanelModel,
  SessionFilesSourcesState,
  SessionFilesState,
} from "./session-files";
export {
  FILE_IMAGE_PREVIEW_BUDGET_BYTES,
  FILE_PREVIEW_BUDGET_BYTES,
  FILE_REFRESH_MAX_RETRY_MS,
  FILE_SOURCES,
  FILE_SOURCE_LABELS,
  buildSessionFilesPanelModel,
  contentForSession,
  emptyAllFilesDirectory,
  emptyGitChanges,
  emptySources,
  initialSessionFilesState,
  normalizeFileSource,
  selectedSourceOf,
  sourceUnsupportedNotice,
  sourcesOf,
} from "./session-files";
export type {
  DownloadGate,
  DownloadPortLike,
  DownloadSinkLike,
  SessionFileDownloadState,
  SessionFileDownloaderOptions,
} from "./session-file-download";
export {
  DOWNLOAD_RETRY_MAX_MS,
  SESSION_FILE_DOWNLOAD_BUDGET_BYTES,
  SessionFileDownloader,
  safeDownloadFileName,
} from "./session-file-download";

// ── R19 主机与连接 ──────────────────────────────────────────────────────────
export type { AgentRowModel, HostIdentityModel, HostPanelModel } from "./host-panel-model";
export { CONNECTION_STATE_LABELS, UNKNOWN_STATE_NOTE, buildHostPanelModel } from "./host-panel-model";

// ── 配对 ────────────────────────────────────────────────────────────────────
export type {
  PairingFailureKind,
  PairingKind,
  PairingLookupOutcome,
  PairingPageModel,
  PairingPhase,
  PairingPollResult,
  PairingPresentation,
  PairingQrRead,
  PairingTransport,
} from "./pairing-model";
export {
  PAIRING_ERROR_HINTS,
  applyAwaitingCode,
  applyClaiming,
  applyDeviceQrAppGuide,
  applyLegacyDeviceQrUnsupported,
  applyLookingUp,
  applyNoPayload,
  applyPollResult,
  applyScannedNodeLink,
  canonicalOriginOf,
  codeInvalidFailure,
  initialPairingPageModel,
  insecureContextFailure,
  kindMismatchFailure,
  normalizePairingCode,
  originMismatchFailure,
} from "./pairing-model";
