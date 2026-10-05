/**
 * feature 控制器层（`docs/FRONTEND_DESIGN.md` §3 的 `features/`）。
 *
 * **归属 WP7**（`plan.md` 的 WP7 行）：R15–R19 的控制器、视图模型与状态仓库都在这一层。
 *
 * 该层可以消费 `src/domain/` 的视图模型与 `src/state/` 的状态类型，
 * 但 MUST NOT 直接触碰 WebSocket/IndexedDB/CryptoKey（`AGENTS.md` §5）——
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
export type { ClientState, ClientStoreOptions, ConversationThreadState, CreateOutcome } from "./client-store";
export { ClientStore, initialClientState } from "./client-store";
export { ClientStoreProvider, useClientState, useClientStore } from "./runtime";

// ── R15 目录 ────────────────────────────────────────────────────────────────
export type {
  DirectoryContentOrigin,
  DirectoryCountsModel,
  DirectoryDetailModel,
  DirectoryPageModel,
  DirectoryRowModel,
  SessionRowModel,
} from "./directory-model";
export {
  CONTENT_ORIGIN_LABELS,
  DIRECTORY_EMPTY_COPY,
  buildDirectoryDetailModel,
  buildDirectoryPageModel,
} from "./directory-model";

// ── R16 会话创建 ────────────────────────────────────────────────────────────
export type { CreateSessionEntryModel, CreateSessionPermission, CreateSessionRefs, CreateSubmissionModel } from "./create-session-model";
export {
  CREATE_DENIED_NOTE,
  CREATE_SESSION_SCOPE,
  CreateSessionPayloadViolation,
  assertCreateSessionRefs,
  buildCreateSessionCommand,
  buildCreateSessionEntryModel,
  createSessionPermission,
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
  UsageReading,
} from "./conversation-model";
export { buildConversationModel, conversationCapabilities, nextBeforeCursor } from "./conversation-model";
export type { DegradedEventModel, DegradedReason, RawAvailability, RawNotSyncedReason, UnsupportedBy } from "./degradation-model";
export {
  RAW_NOT_SYNCED_LABELS,
  UNSUPPORTED_LABELS,
  buildDegradedEventModel,
  isDegraded,
  unsupportedByFromErrorCode,
} from "./degradation-model";

// ── R19 主机与连接 ──────────────────────────────────────────────────────────
export type { AgentRowModel, HostIdentityModel, HostPanelModel } from "./host-panel-model";
export { CONNECTION_STATE_LABELS, UNKNOWN_STATE_NOTE, buildHostPanelModel } from "./host-panel-model";

// ── 配对 ────────────────────────────────────────────────────────────────────
export type { PairingPageModel, PairingPhase, PairingPollResult, PairingTransport } from "./pairing-model";
export {
  PAIRING_ERROR_HINTS,
  applyClaiming,
  applyPollResult,
  applyScannedQr,
  initialPairingPageModel,
} from "./pairing-model";
