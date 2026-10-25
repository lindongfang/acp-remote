/**
 * 客户端状态仓库：把 `src/sync-client/` 的事件流与命令结果收敛成页面可渲染的模型。
 *
 * ## 这一层做什么、不做什么
 *
 * **做**：把 `SyncClientEvent` 归并进一份不可变状态，并从状态投影出各页面的视图模型。
 * **不做**：不持有 socket、不碰 IndexedDB/WebCrypto、不自己判定该不该重连——重连与阻断
 * 由 `src/state/` 的连接状态机决定（`shouldAutoReconnect`），本页只读它的结论。
 *
 * ## 呈现覆盖由客户端自持（`design.md` D5）
 *
 * 每条事件先归一成 `StructuredRecord`（实时事件与按会话读取的历史条目**同形**），再按
 * `presentation-coverage.ts` 的覆盖表分流：命中 → 走它自己的通道；未命中 → 降级卡片。
 * 判定**不**看协议词表，因此「协议已知但客户端还没有卡片」的事件同样有降级卡片，不会被静默丢掉。
 *
 * ## 正文只来自 session.read（D6/D8）
 *
 * 事件侧不拼接正式正文：消息增量只进**易失**叠加层（按 `messageId` 累加），`session.read`
 * 返回同 id 的完整正文后以读取结果为准（叠加层里同 id 的条目随即不再呈现）；
 * `turn.delta_compacted` 作废该轮次的累加。这样不存在「快照里的明细」与「读回来的明细」
 * 两套权威需要合并。
 *
 * ## 命令出口只有一个
 *
 * 用户意图（创建会话、读取明细、翻页、提交提示、取消轮次、应答交互、切换模型/模式）都经
 * `#dispatchIntent`，它用**稳定 `requestId`** 并把记录以 `submitting` 写进 `commands`；
 * 终态由 `command_result` 回填（R13）。终态之前，一切呈现只能是「待确认」。
 */

import type {
  CommandBody,
  CommandResultMessage,
  CommandResultStatus,
  CommandStatusRecord,
  ConfigOptionView,
  ElicitationResolutionAction,
  ElicitationRespondPayload,
  EventMessage,
  ModeState,
  OriginCursor,
  PairingLookupResponse,
  PromptContentBlock,
  PublicError,
  SessionReadInclude,
  SessionReadResult,
  SessionRef,
  SessionSummary,
  SnapshotItemHistory,
  SyncSessionPreparationResult,
  SyncSessionViewResult,
  SnapshotItemMessages,
  Uuid,
} from "../protocol";
import { ATTACHMENT_READ_MAX_LENGTH, decodeEffectiveInteraction, decodePartialResult, messageChannelOf } from "../protocol";
import { isCommandStatusRecord } from "../sync-client/wire";
import type {
  ReadCursor,
  RemoteCoverageMap,
  StagedResources,
  SyncClientEvent,
} from "../sync-client";
import {
  EMPTY_REMOTE_COVERAGE,
  advanceOriginCursor,
  coverageFromRead,
  isDeltaCovered,
  mergeRemoteCoverage,
  pruneCoveredDeltas,
} from "../sync-client";

/** 刷新后附件连接可能仍在认证/重连：媒体读取对「尚未就绪」做有界等待，其他错误立即失败。 */
const ATTACHMENT_READY_ATTEMPTS = 20;
const ATTACHMENT_READY_DELAY_MS = 150;

/** 远程来源流能力（`resource.stateless-origin.v1`，§9.7）。 */
const STATELESS_ORIGIN_FEATURE = "resource.stateless-origin.v1";

function isTransientAttachmentReadiness(error: unknown): boolean {
  const text = error instanceof Error ? error.message : String(error);
  return (
    text.includes("尚未就绪") ||
    text.includes("尚未装配") ||
    text.includes("not ready") ||
    text.includes("not connected")
  );
}

function delay(ms: number): Promise<void> {
  // 目标 lib 尚无 `Promise.withResolvers`（ES2024）；这里只有一个 resolve，执行器形式最直接。
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}
import {
  AgentConnectionOverlayStore,
  ATTACHMENT_FEATURE,
  AttachmentTransferError,
  LOCAL_SESSION_LIFECYCLE_FEATURE,
  PENDING_INTERACTIONS_FEATURE,
  SESSION_HISTORY_FEATURE,
  SESSION_TITLE_FEATURE,
  assertHistoryPagesDoNotOverlap,
  assertPagesDoNotOverlap,
  decodeBase64Url,
  toLoadedHistoryPage,
  toLoadedPage,
} from "../sync-client";
import type {
  PendingInteractionInput,
} from "./interaction-model";
import type { AllFilesDirectoryView, FileSource, FileSourceUnsupportedReason, GitDiffView } from "../domain/session-files";
import { displayDirectoryPath } from "../domain/directory";
import {
  SESSION_FILES_FEATURE,
  WORKSPACE_FILES_FEATURE,
  decodeSessionFilesResult,
  isSessionFilesCommandName,
} from "../protocol/session-files";
import type { SessionFilesCommandBody, WorkspaceFilesTarget } from "../protocol/session-files";
import type {
  FileContentState,
  FileViewContext,
  SessionFilesPanelModel,
  SessionFilesSourcesState,
  SessionFilesState,
} from "./session-files";
import type {
  SessionTitleCommitReason,
  SessionTitleEditorState,
  SessionTitleEditorView,
  SessionTitlePendingCommit,
} from "./session-title-editor";
import {
  abandonTitleEditing,
  beginTitleEditing,
  buildSessionTitleEditorView,
  cancelTitleEditing,
  editTitleDraft,
  initialSessionTitleEditorState,
  markTitleEditError,
  markTitleSubmitting,
  shouldAttemptTitleCommit,
  titleCommitReflected,
  validateSessionTitle,
} from "./session-title-editor";
import {
  FILE_IMAGE_PREVIEW_BUDGET_BYTES,
  FILE_PREVIEW_BUDGET_BYTES,
  FILE_REFRESH_MAX_RETRY_MS,
  buildSessionFilesPanelModel,
  emptyAllFilesDirectory,
  initialSessionFilesState,
  normalizeFileSource,
  selectedSourceOf,
  sourcesOf,
} from "./session-files";
import type { BlockingCause, CommandRecord, ConnectionEvent, ConnectionMachineState } from "../state";
import {
  IllegalConnectionTransition,
  applyCommandResult,
  createCommand,
  initialConnectionState,
  isBlockingState,
  markSubmitting,
  transition,
} from "../state";
import type {
  AgentCatalogEntryView,
  AttachmentMediaView,
  FormInputValue,
  InteractionView,
  ModeChangedTimelineCard,
  PartialResultView,
  SessionReadPageView,
  StructuredRecord,
} from "../domain";
import {
  INTERACTION_UNAVAILABLE_LABELS,
  PARTIAL_RESULT_LABELS,
  buildAgentCatalogView,
  buildElicitationForm,
  buildSessionReadPageView,
  buildTimelineCard,
  compareRecordOrder,
  directoryNamesOf,
  localActivityOf,
  mediaBlocksOfDelta,
  recordFromEvent,
  recordFromHistoryItem,
  recordFromRemoteEvent,
  recordGroupOf,
  validateElicitationForm,
  viewText,
} from "../domain";
import type { AgentConnectionOverlay } from "../domain/agent-catalog";
import type {
  AttachmentCapability,
  AttachmentKind,
  DraftAttachment,
} from "./attachment-model";
import {
  attachmentCapabilityOf,
  buildAttachmentBatch,
  isWorkspacePathRef,
} from "./attachment-model";
import type {
  AttachmentFileHandle,
  AttachmentMediaPort,
  AttachmentTransferGateway,
  ConnectionIdentity,
  IdentifierSource,
  PreparationDraftAccess,
  PreparationDraftAttachmentInput,
  PreparationDraftContent,
  PreparationDraftGateway,
  PreparationDraftScope,
  PreparationTabLocatorSnapshot,
  SyncGateway,
} from "./ports";
import type {
  ConversationHeaderModel,
  ConversationPageModel,
  ConversationSecondaryView,
  UsageReading,
} from "./conversation-model";
import {
  DEFAULT_CONVERSATION_SECONDARY_VIEW,
  buildConversationModel,
  buildConversationSidebar,
  conversationCapabilities,
  structuredHistoryAvailability,
} from "./conversation-model";
import type { ConversationInputModel } from "./conversation-input-model";
import { buildConversationInputModel } from "./conversation-input-model";
import type {
  ActivationFailure,
  SessionActivationState,
  SessionActivationView,
  SessionPageEvent,
  SessionViewDeclaration,
} from "./session-lifecycle";
import {
  ACTIVATION_INACTIVE_NOTICE,
  ACTIVATION_UNAVAILABLE_NOTICE,
  SESSION_VIEW_RENEW_INTERVAL_MS,
  activationViewOf,
  canAdvanceViewVersion,
  classifyActivationFailure,
  isOwnedSummary,
  nextViewVersion,
} from "./session-lifecycle";
import type { ConversationSettingsModel } from "./conversation-settings-model";
import { buildConversationSettingsModel } from "./conversation-settings-model";
import type { SentPromptPhase, TimelineEntry, VolatileMessageState } from "./conversation-timeline";
import {
  appendDelta,
  buildFileChangeViews,
  buildTimeline,
  deltaChannelOf,
  stabilizeTimelineEntries,
} from "./conversation-timeline";
import { buildConversationProcess } from "./conversation-process-model";
import type { DegradedEventModel } from "./degradation-model";
import { buildDegradedRecordModel, buildRecordDiagnosticsModel, isDegradedRecord } from "./degradation-model";
import { buildInteractionViews } from "./interaction-model";
import type {
  CreateSessionEntryModel,
  CreateSessionRefs,
  DirectoryBrowseState,
  DirectoryBrowserModel,
  DirectorySelection,
  RegisteredDirectoryModel,
} from "./create-session-model";
import {
  BROWSE_RESULT_INVALID_ERROR,
  CREATE_SHEET_BROWSE_KEY,
  WORKSPACE_BROWSE_FEATURE,
  assertCreateSessionRefs,
  buildCreateFolderPath,
  buildCreateSessionCommand,
  buildCreateSessionEntryModel,
  buildDirectoryBrowserModel,
  crumbsFor,
  initialDirectoryBrowseState,
  openedDirectoryBrowseState,
  parseBrowseResult,
} from "./create-session-model";
import type {
  DirectoryDetailModel,
  DirectoryFilter,
  DirectoryPageModel,
  SessionRowModel,
} from "./directory-model";
import {
  buildDirectoryDetailModel,
  buildDirectoryPageModel,
  buildSessionRowsByDirectoryId,
  correctPageNumber,
} from "./directory-model";
import type { HostIdentityModel, HostPanelModel } from "./host-panel-model";
import { buildHostPanelModel } from "./host-panel-model";
import type {
  PreparationAssociation,
  PreparationBeginOutcome,
  PreparationPageModel,
  PreparationSessionState,
  PreparationSlotRoute,
  PreparationWorkspace,
  PreparationWorkspaceInput,
} from "./preparation-model";
import {
  PREPARATION_DUPLICATE_TAB_NOTICE,
  PREPARATION_NO_TAB_NONCE_NOTICE,
  PREPARATION_NOT_RESTORABLE_NOTICE,
  PREPARATION_PHASE_LABELS,
  PREPARATION_RENEW_INTERVAL_MS,
  PREPARATION_STORAGE_FAILED_NOTICE,
  PREPARATION_STORE_REBUILT_NOTICE,
  PREPARATION_UNCERTAIN_NOTICE,
  PREPARATION_UNSUPPORTED_NOTICE,
  SESSION_PREPARATION_FEATURE,
  buildAllocateBody,
  buildConfigureBody,
  buildContinueStartBody,
  buildInitialContent,
  buildInitialStartBody,
  buildPreparationPageModel,
  buildReleaseBody,
  buildRenewBody,
  buildRestoreBody,
  isSessionStartProjection,
  preparationErrorAction,
} from "./preparation-model";
import type {
  PairingKind,
  PairingLookupOutcome,
  PairingPageModel,
  PairingPollResult,
  PairingQrRead,
  PairingWiring,
} from "./pairing-model";
import {
  PAIRING_ERROR_HINTS,
  PAIRING_POLL_INTERVAL_MS,
  applyAwaitingCode,
  applyClaiming,
  applyDeviceQrAppGuide,
  applyLegacyDeviceQrUnsupported,
  applyLookingUp,
  applyNoPayload,
  applyPairingCloseoutFailure,
  applyPairingFailure,
  applyPairingKind,
  applyPollResult,
  applyScannedNodeLink,
  canonicalOriginOf,
  codeInvalidFailure,
  initialPairingPageModel,
  insecureContextFailure,
  kindMismatchFailure,
  normalizePairingCode,
  originMismatchFailure,
  pairedHostRecordFor,
  pairedHostWriteFailure,
  publicErrorForCloseoutFailure,
  publicErrorForQrInput,
  publicErrorForTransportFailure,
} from "./pairing-model";
import { DEVICE_QR_FRAGMENT_PREFIX } from "../protocol";

/**
 * 对话页首屏请求的明细资源。
 *
 * 正文（`messages`）之外还要配置项（模型/模式的可选项）。`pending_interactions` 只在协商
 * `core.pending-interactions.v1` 后带上：它返回服务端确认过的**当前有效**完整请求（含
 * `requestVersion`/`options`），是刷新或换设备后仍可应答的唯一依据。结构化历史只在协商
 * `core.session-history.v1` 时才带。
 */
const CONVERSATION_INCLUDES: readonly SessionReadInclude[] = ["messages", "config_options"];

/** 一次 `session.read` 取回的明细资源。 */
interface StagedReadResources {
  readonly pendingInteractions: readonly PendingInteractionInput[];
  readonly configOptions: readonly ConfigOptionView[];
  readonly configVersion: string | null;
  /** 本会话的 Agent 能力声明（`capabilities` 资源；服务端未提供时缺席）。 */
  readonly agentCapabilities: Readonly<Record<string, unknown>> | null;
}

/** 一个会话的明细线程（分页现取的结果）。 */
export interface ConversationThreadState {
  readonly page: SessionReadPageView | null;
  /** 已读到的原始消息项（`session.read` 的元素形状）：向上翻页的不重不漏校验以它为准。 */
  readonly items: readonly SnapshotItemMessages[];
  /** 结构化历史那一页是否还有更早条目（`historyHasEarlier`，与消息分页互不影响）。 */
  readonly historyHasEarlier: boolean;
  /** 历史条的续取游标（会话内事件序号）。 */
  readonly nextBeforeHistory: string | null;
  readonly loadingEarlier: boolean;
  readonly loadingHistory: boolean;
  readonly pagingError: string | null;
  /**
   * 本线程是否已完成过一次「重新取得当前有效请求」。
   *
   * 完成后仍未被服务端确认的历史交互不再标「尚未确认」（`read_from_history`），而改标
   * 「服务端当前不再确认」（`not_effective`）——否则按钮点下去在结果为空时没有任何可见变化。
   */
  readonly interactionsRefreshed?: boolean;
  /** 已被有效请求读取覆盖的请求事件；不覆盖读取发出后才到达的新事件。 */
  readonly checkedInteractionEvents?: ReadonlySet<string>;
}

/** 一次 `session.read` 的用途：首屏、向上翻消息、向上翻历史、刷新有效交互。 */
type ReadIntent =
  | "first"
  | "earlier"
  | "earlier_history"
  | "refresh_interactions"
  | "refresh_messages"
  /** 来源断开/重置/刷新后：无论是否仍持有 C，都先回源读取重建（D4）。 */
  | "rebuild_remote";

/** 待完成的读取请求。 */
interface PendingRead {
  readonly interactionEventsAtRequest: ReadonlySet<string>;
  readonly sessionId: Uuid;
  readonly intent: ReadIntent;
  /**
   * 发起读取时已经存在于易失叠加层的增量（`messageId → deltaIndex`）。
   *
   * 读取结果只清掉**这一批**：同一请求之后新到的增量留在叠加层里，接在权威正文之后。
   * 这就是 design D5 的「读取屏障」——旧读取不覆盖新增量。
   */
  readonly volatileAtRequest: ReadonlyMap<string, ReadonlySet<string>>;
  /** 远程来源坐标；本节点自有读取为 `null`。 */
  readonly remote: SessionRef | null;
  /** 发起读取时的来源代数；返回时代数已变说明来源被清除/重置，结果必须丢弃。 */
  readonly generation: number;
}

/** 一次发送尝试的上传/清单状态（与业务命令记录分离：上传成功不等于消息被受理）。 */
export interface AttachmentRunState {
  readonly phase: "uploading" | "sealed" | "failed";
  /** 稳定 `requestId`：本次发送重试复用同一个（服务端幂等）。 */
  readonly promptRequestId: Uuid;
  /** 清单标识（`manifest_sealed` 的标识；`session.prompt.submissionId`）。 */
  readonly submissionId: Uuid;
  /** 提交清单的过程与结果。 */
  readonly manifestSealed: boolean;
  readonly error: string | null;
}

/** 一条媒体读取的呈现状态（视图模型见 `domain/media.ts`）。 */
export type AttachmentMediaState = AttachmentMediaView;

/** 本设备提交的切换意图（模型/模式各一条），用于「切换中」的呈现。 */
interface SwitchIntent {
  readonly requestId: Uuid;
  readonly kind: "model" | "mode";
}

/** 来源连接事实（`sync.remote_source_state`）；只存在于当前运行期。 */
export type RemoteSourceKind = "unknown" | "online" | "offline" | "revoked";

/**
 * 一个 Owner 来源的运行期代数与状态。
 *
 * `generation` 在每次「来源业务模型失效」时递增（断开、离线/撤销、重置）：在途读取带着旧
 * 代数返回时必须被丢弃，旧页因此不可能回滚新内容。
 */
export interface RemoteSourceRuntime {
  readonly state: RemoteSourceKind;
  readonly generation: number;
}

/**
 * 一个远程会话的运行期模型（`design.md` D3/D4）。
 *
 * - `phase === "ready"` 才表示「读取重建完成且来源已追平」，此前不呈现消息/结构化内容；
 * - `cursor` 是最后连续处理位置 C（只用于回源重放衔接与去重，不是可恢复索引）；
 * - `coverage` 记录每条消息已应用的读取覆盖位置，迟到增量据此不再拼接；
 * - `error` 是读取失败或来源不可用的明确原因（不展示缓存标题）。
 */
export interface RemoteSessionRuntime {
  readonly ownerNodeId: Uuid;
  readonly sessionId: Uuid;
  readonly phase: "subscribing" | "replaying" | "ready" | "unavailable";
  readonly sourceState: RemoteSourceKind;
  readonly cursor: OriginCursor | null;
  readonly caughtUp: boolean;
  readonly rebuilt: boolean;
  readonly coverage: RemoteCoverageMap;
  readonly error: PublicError | null;
  readonly generation: number;
}

/** 最近一次连接关闭/阻断的事实（诊断与连接问题弹层的数据来源）。 */
export interface ConnectionCloseRecord {
  readonly code: number;
  readonly reason: string | null;
  /** 收到关闭事件的时刻（仓库时钟）。 */
  readonly atMs: number;
}

/** 自动重连的进度（由组合根的退避安排写入；只是呈现用的事实，不参与连接状态机）。 */
export interface ReconnectProgress {
  readonly attempts: number;
  readonly nextRetryAtMs: number | null;
}

/**
 * 本机会话生命周期的连接期状态（`core.local-session-lifecycle.v1`）。
 *
 * 只活在当前进程：查看声明、租约与激活尝试都不落盘。`generation` 在每次认证成功时递增，
 * 与 `viewVersion` 一起抑制旧连接/旧响应（迟到的 `session.view.result` 或 `session.resume`
 * 终态不得切回旧页面、不得续租旧 viewer）。
 */
export interface SessionLifecycleState {
  /** 本次连接是否协商到该 feature；未协商时不发查看/恢复帧，并明确显示自动激活不可用。 */
  readonly negotiated: boolean;
  /** 认证连接代：每次 `authenticated` 递增。 */
  readonly generation: number;
  /** 平台 adapter 报告的可见性；非浏览器缺省 `visible`。 */
  readonly visibility: "visible" | "hidden";
  /** 当前对话页选中的会话；离页/卸载后为 `null`。 */
  readonly selectedSessionId: Uuid | null;
  /** 最近一次发出的查看声明（含离页声明）。 */
  readonly view: SessionViewDeclaration | null;
  /** 各会话最近一次激活尝试（按会话保存，供 A/B 快切后丢弃旧结果）。 */
  readonly activations: ReadonlyMap<Uuid, SessionActivationState>;
  /** 最近一次被接受声明的租约到期时刻（服务端时钟；只用于诊断与呈现）。 */
  readonly leaseExpiresAt: string | null;
}

/** 初始生命周期状态：未认证、可见、没有选择。 */
export function initialSessionLifecycleState(): SessionLifecycleState {
  return {
    negotiated: false,
    generation: 0,
    visibility: "visible",
    selectedSessionId: null,
    view: null,
    activations: new Map(),
    leaseExpiresAt: null,
  };
}

/** 写入一次激活尝试（按会话覆盖；返回新的只读 Map）。 */
function withSessionActivation(
  current: ReadonlyMap<Uuid, SessionActivationState>,
  next: SessionActivationState,
): ReadonlyMap<Uuid, SessionActivationState> {
  const map = new Map(current);
  map.set(next.sessionId, next);
  return map;
}

/** 按 `requestId` 找回那次激活尝试；找不到即迟到的旧结果。 */
function findActivationByRequest(
  current: ReadonlyMap<Uuid, SessionActivationState>,
  requestId: Uuid,
): SessionActivationState | null {
  for (const entry of current.values()) {
    if (entry.requestId === requestId) return entry;
  }
  return null;
}

/**
 * 终结仍在途的激活尝试（连接断开/连接代变化时）。
 *
 * 结果按连接核对，旧连接/旧代上的在途尝试永远等不到终态；保留它们会让页面永远停在
 * 「正在恢复」，也会让「进入会话不重复派发」的判定把它们当成仍然有效。重新进入/重连时
 * 重新申请即可（`session.resume` 本身对同一会话的并发激活是合并的）。
 */
function withoutActivating(
  current: ReadonlyMap<Uuid, SessionActivationState>,
): ReadonlyMap<Uuid, SessionActivationState> {
  let removed = false;
  const map = new Map<Uuid, SessionActivationState>();
  for (const [sessionId, entry] of current) {
    if (entry.phase === "activating") {
      removed = true;
      continue;
    }
    map.set(sessionId, entry);
  }
  return removed ? map : current;
}

/**
 * `session.resume` 结果里的摘要核对：`sessionId` 与 `session.sessionId` 都必须指向那次尝试。
 *
 * `command.result` 与 `command.status` 两条路径共用同一判据——形状不符**不得**被当成成功。
 */
function resumedSummaryOf(result: unknown, sessionId: Uuid): SessionSummary | null {
  const value = toResultRecord(result);
  if (value === null || value["sessionId"] !== sessionId) return null;
  const summary = value["session"];
  if (summary === undefined || summary === null || typeof summary !== "object") return null;
  const candidate = summary as SessionSummary;
  return candidate.sessionId === sessionId ? candidate : null;
}

/** 结果形状不符的一次激活失败（与明确失败同名同义：历史仍可读，可显式重试）。 */
function activationShapeFailure(): ActivationFailure {
  return {
    kind: "failed",
    code: null,
    message: "恢复结果形状不符，无法确认会话已可用；历史仍可阅读，可重试。",
  };
}

/**
 * 本设备已提交、尚未被权威消息接替的用户消息。
 *
 * 本地条目只有**可靠身份**：`turnId` 来自 `session.prompt` 的 `accepted { turnId }`（首条
 * `session.start` 没有该字段，用它准备关联的 `messageId` 或权威读取确认的首条消息）；
 * 视图层只按这些标识接替，不按相同文字或时间接近推断重复。
 */
export interface SentPrompt {
  readonly requestId: Uuid;
  readonly sessionId: Uuid;
  readonly text: string;
  /** 附件文件名（仅用于呈现；字节与上传状态不在这里）。 */
  readonly attachmentNames: readonly string[];
  readonly createdAt: string;
  /** 本地阶段：上传/封存/待确认/已受理/不确定/未成立。 */
  readonly phase: SentPromptPhase;
  /** 服务端受理后给出的轮次标识；尚未受理为 `null`。 */
  readonly turnId: string | null;
  /** 已确认的正式用户消息标识（首条准备关联或权威首条读取）；未知为 `null`。 */
  readonly messageId: string | null;
  /** 本地实情（上传失败/取消说明）；没有为 `null`。 */
  readonly notice: string | null;
}

/** 客户端状态（不可变快照）。 */
export interface ClientState {
  readonly connection: ConnectionMachineState;
  /** 本设备当前这次会话是否已经成功认证过（`auth.authenticated` 至少到达过一次）。 */
  readonly hasAuthenticated: boolean;
  /**
   * 本设备已经提交、但权威用户消息尚未接替的用户消息（按 `requestId`）。
   *
   * 用户消息的权威来源是 `session.read` 的消息页；Agent 不一定回显用户输入，读取也可能尚未返回。
   * 这里保留「我刚发出去的正文」与它的本地阶段（上传/封存/待确认/已受理/不确定/未成立），让用户
   * 立刻看到自己的消息与真实进度。视图层**只按** `messageId` 或 `turnId` 的同身份正式消息接替它，
   * MUST NOT 按相同文字抵消（文字相同的两次输入是两条独立消息）。
   */
  readonly sentPrompts: ReadonlyMap<Uuid, SentPrompt>;
  /**
   * 已确认的本地接替关联（每个会话一份 `messageId → requestId`）：权威用户消息接替本地回显时
   * 在同一状态更新里登记。时间线据此让正式消息沿用本地条目的稳定键与分组（节点原位更新，D2）。
   *
   * 按会话分桶是为了**能清理**：会话被删除、来源数据被清空或连接重置时整桶丢弃，锚点不会只增
   * 不删、把每个会话的历史 `messageId` 一直挂在进程内存里。仍在使用的会话不受影响。
   *
   * 关联只来自 `messageId`/`turnId` 的同身份确认，**不**按相同文字或时间接近推断。
   */
  readonly sentPromptAnchors: ReadonlyMap<Uuid, ReadonlyMap<string, string>>;
  /** 最近一次 `closed`/`blocked` 的关闭码与原因；从未关闭为 `null`。 */
  readonly lastClose: ConnectionCloseRecord | null;
  /** 自动重连的尝试次数与下次重试时刻；未安排重连时次数为 0。 */
  readonly reconnect: ReconnectProgress;
  /** 本设备标识（`auth.authenticated.deviceId`）；未认证为 `null`。 */
  readonly deviceId: string | null;
  /** 服务端本次协商选中的 feature（决定 `history` include 要不要带）。 */
  readonly selectedFeatures: readonly string[];
  /** 本机会话生命周期（查看声明、激活尝试、连接代与可见性）。 */
  readonly lifecycle: SessionLifecycleState;
  /** 最后一次**验证通过并完成替换**的快照资源；断连时仍是目录页的唯一数据来源。 */
  readonly resources: StagedResources | null;
  /**
   * 来源 Owner 的运行期状态与代数（内存；不落盘）。
   *
   * 连接断开或来源 `offline`/`revoked` 时清除该来源的展示数据并递增代数，在途读取据此失效。
   */
  readonly remoteSources: ReadonlyMap<Uuid, RemoteSourceRuntime>;
  /** 远程会话的运行期模型（内存；刷新即失去，下次打开先回源读取重建）。 */
  readonly remoteSessions: ReadonlyMap<Uuid, RemoteSessionRuntime>;
  readonly overlay: readonly AgentConnectionOverlay[];
  /** 实时事件归一后的结构化记录（按 `eventId`，先到先留）。 */
  readonly structured: ReadonlyMap<string, StructuredRecord>;
  /** 按会话读取的历史条目（按 `eventId`）。与 `structured` 去重后合并呈现。 */
  readonly history: ReadonlyMap<string, StructuredRecord>;
  /** 易失叠加层：按 `messageId` 累加的流式正文（正文权威仍是 `session.read`）。 */
  readonly volatile: ReadonlyMap<string, VolatileMessageState>;
  /**
   * 已作废的轮次（`<sessionId>|<turnId>`）：`turn.delta_compacted` 之后该轮次的累加内容
   * 不再呈现，迟到的增量也不再累加。
   */
  readonly discardedTurns: ReadonlySet<string>;
  /** 会话的文本草稿（只在内存；`no-content-cache` 边界，D10）。 */
  readonly drafts: ReadonlyMap<Uuid, string>;
  /** 会话标题编辑器（按会话绑定；内存、不恢复进度，`design.md` D4）。 */
  readonly titleEditor: SessionTitleEditorState;
  /** 已派发、尚未以「终态 + 权威摘要」收束的标题命令（按 `requestId`）。 */
  readonly titleCommits: ReadonlyMap<Uuid, SessionTitlePendingCommit>;
  /** 离页后失败的可归因提示（按会话）；只显示错误，不保留跨页草稿。 */
  readonly titleNotices: ReadonlyMap<Uuid, string>;
  /** 已选择的附件（只在内存；选择阶段不上传、不落盘）。 */
  readonly attachments: ReadonlyMap<Uuid, readonly DraftAttachment[]>;
  /**
   * Web 准备状态（键是 Owner 分配的 `sessionId`）。
   *
   * 存在即「这个 ID 当前是准备会话」：没有正式会话行、模式/配置来自临时真实端点。
   * 正式受理之后这一项仍保留（承载请求关联与状态），但页面改读正式会话。
   */
  readonly preparations: ReadonlyMap<Uuid, PreparationSessionState>;
  /** 每次发送尝试的上传/清单状态；与命令记录分离。 */
  readonly attachmentRuns: ReadonlyMap<Uuid, AttachmentRunState>;
  /** 工作目录路径引用（相对路径；与服务端再校验之前只是草稿的一部分）。 */
  readonly pathRefs: ReadonlyMap<Uuid, readonly string[]>;
  /** 已读取的媒体（按 `attachmentId`）：只保存对象 URL 与状态，字节不留在公开状态里。 */
  readonly attachmentMedia: ReadonlyMap<string, AttachmentMediaState>;
  /** 每个会话最近一次用量事件。键**不存在**即「从未收到用量事件」（R17 第一态）。 */
  readonly usage: ReadonlyMap<Uuid, UsageReading>;
  readonly threads: ReadonlyMap<Uuid, ConversationThreadState>;
  /** 本次读回的明细资源（待处理交互、配置项）。 */
  readonly readResources: ReadonlyMap<Uuid, StagedReadResources>;
  /**
   * `session.mode.list` 的结果（可选项 + 清单时刻的当前值与版本）。
   *
   * 它**不**随 `session.mode.changed` 更新：那是另一份信号，由 `#modeChangeOf` 读出来与它合并
   * （当前值的回落顺序见 `conversation-settings-model.ts`）。
   */
  readonly modeStates: ReadonlyMap<Uuid, ModeState>;
  /** 本设备提交的应答：`interactionId → requestId`。 */
  readonly responseRequestIds: ReadonlyMap<string, Uuid>;
  /** 本设备提交的提示：`sessionId → requestId`。 */
  readonly promptRequestIds: ReadonlyMap<Uuid, Uuid>;
  /** 本设备提交的取消：`turnId → requestId`。 */
  readonly cancelRequestIds: ReadonlyMap<string, Uuid>;
  /** 本设备提交的模型/模式切换：`sessionId → 意图`。 */
  readonly switchIntents: ReadonlyMap<Uuid, SwitchIntent>;
  readonly commands: ReadonlyMap<Uuid, CommandRecord>;
  /** 每个目录当前那次创建会话的 `requestId`；未提交过的目录没有键。 */
  readonly createRequestIds: ReadonlyMap<string, Uuid>;
  /** 结果不确定的 `requestId`：保留到用户主动处理，不随时间或重连清除。 */
  readonly uncertainRequestIds: readonly Uuid[];
  readonly createSheetDirectoryId: string | null;
  /**
   * 顶部入口打开的创建弹层（没有已知目录，目录由浏览/选择给出）。
   *
   * 与 `createSheetDirectoryId` **互斥**、共同构成「弹层是否打开」这**一个**判据的两支：三个写入方
   * （`openCreateSheet` / `openCreateSheetToBrowse` / `closeCreateSheet`）每次都同时设置两者，
   * 因此不存在「两边都开着」或「都关着却还渲染弹层」的中间态。之所以不把它塞进 `createSheetDirectoryId`
   * 的取值域，是因为那个字段的语义是「入口所属的已记录目录标识」，而顶部入口根本没有目录。
   */
  readonly createSheetBrowses: boolean;
  /** 目录浏览选择器（`workspace.browse`）的状态：单份、跨路由存活。 */
  readonly directoryBrowse: DirectoryBrowseState;
  readonly folderRequest: { readonly requestId: Uuid; readonly parentPath: string; readonly name: string; readonly refreshed: boolean; readonly entered?: boolean } | null;
  readonly diagnosticsEventId: string | null;
  /** 目录页查询词；与详情页的页码是两个独立状态（两页各有自己的 UI 状态）。 */
  readonly directoryQuery: string;
  readonly directoryFilter: DirectoryFilter;
  /**
   * 目录页页码输入框里的值（已纠正）；缺省首页。
   *
   * 与详情页的页码是两个独立状态：查询词 / 筛选变化时回到第一页，避免「筛完只剩两页，
   * 却还停在第 4 页」的悬空状态。
   */
  readonly directoryListPage: string;
  /**
   * 详情页页码输入框里的值（**合法值**：提交时已按 `correctPageNumber` 纠正）。
   *
   * 单份 UI 状态：跨路由存活、只影响渲染。存的是字符串而不是数字，因为它是输入框的回填值，
   * 而且「非法输入回落到当前页」这条判定要用提交时那一刻的生效页。
   */
  readonly directoryDetailPage: string;
  /**
   * 上面那个页码属于**哪个目录**；`null` 表示它不属于任何目录（没有生效值）。
   *
   * 页码是单份状态，而它的含义与目录绑定：`directoryDetail(directoryId)` 只在两者对得上时才采用这个值，
   * 换一个目录立刻按第一页渲染——**首帧**就是对的，不必等挂载后的复位（复位晚一帧时，用户会先看到
   * 上一个目录的页码被渲染出来再被跳回去）。
   */
  readonly directoryDetailDirectoryId: string | null;
  /** 对话页窄屏的次级视图（D5）；三栏内容始终在 DOM，未选中的栏只隐藏。 */
  readonly conversationSecondaryView: ConversationSecondaryView;
  /** 对话页左栏的目录内搜索词；只影响左栏列表，不改中栏当前会话。 */
  readonly conversationSidebarQuery: string;
  /** 对话页左栏页码输入；由模型纠正到有效页码。 */
  readonly conversationSidebarPageInput: string;
  readonly pairing: PairingPageModel;
  readonly host: HostIdentityModel | null;
  /** 文件栏两来源、互斥内容视图与刷新状态（与草稿/时间线/展开态互不重叠）。 */
  readonly sessionFiles: SessionFilesState;
}

/** 初始状态：未配对、没有快照。 */
export function initialClientState(): ClientState {
  return {
    connection: initialConnectionState(),
    hasAuthenticated: false,
    sentPrompts: new Map(),
    sentPromptAnchors: new Map(),
    lastClose: null,
    reconnect: { attempts: 0, nextRetryAtMs: null },
    deviceId: null,
    selectedFeatures: [],
    lifecycle: initialSessionLifecycleState(),
    resources: null,
    remoteSources: new Map(),
    remoteSessions: new Map(),
    overlay: [],
    structured: new Map(),
    history: new Map(),
    volatile: new Map(),
    discardedTurns: new Set(),
    drafts: new Map(),
    titleEditor: initialSessionTitleEditorState(),
    titleCommits: new Map(),
    titleNotices: new Map(),
    attachments: new Map(),
    preparations: new Map(),
    attachmentRuns: new Map(),
    pathRefs: new Map(),
    attachmentMedia: new Map(),
    usage: new Map(),
    threads: new Map(),
    readResources: new Map(),
    modeStates: new Map(),
    responseRequestIds: new Map(),
    promptRequestIds: new Map(),
    cancelRequestIds: new Map(),
    switchIntents: new Map(),
    commands: new Map(),
    createRequestIds: new Map(),
    uncertainRequestIds: [],
    createSheetDirectoryId: null,
    createSheetBrowses: false,
    directoryBrowse: initialDirectoryBrowseState(),
    folderRequest: null,
    diagnosticsEventId: null,
    directoryQuery: "",
    directoryFilter: "all",
    directoryListPage: "",
    directoryDetailPage: "",
    directoryDetailDirectoryId: null,
    conversationSecondaryView: DEFAULT_CONVERSATION_SECONDARY_VIEW,
    conversationSidebarQuery: "",
    conversationSidebarPageInput: "",
    pairing: initialPairingPageModel(),
    host: null,
    sessionFiles: initialSessionFilesState(),
  };
}

/**
 * 应用一个连接机事件；迁移表不接受时保持原状态。
 *
 * 「保持原状态」而不是抛错，是因为关闭之后仍可能到达迟到的 `online`/`closed`
 * （旧 socket 的回调）。迁移表仍是唯一判定源——只是不把这类竞态升级成崩溃。
 */
function applyConnectionEvent(current: ConnectionMachineState, event: ConnectionEvent): ConnectionMachineState {
  try {
    return transition(current, event);
  } catch (error) {
    if (error instanceof IllegalConnectionTransition) return current;
    throw error;
  }
}

/**
 * 结构化记录的去重键：**来源节点 + 事件标识**。
 *
 * 只按裸 `eventId` 会让不同 Owner 的同名标识互相覆盖（来源命名空间要求），因此本节点记录
 * 用 `local:` 前缀，远程记录用自己的 `ownerNodeId`。
 */
function recordKey(record: StructuredRecord): string {
  return `${record.ownerNodeId ?? "local"}:${record.eventId}`;
}

/** 一条来源不可用的结构化原因（不把远程数据写进错误载荷，只给固定类别）。 */
function remoteUnavailableError(ownerNodeId: Uuid, detail?: string): PublicError {
  return {
    code: "resource.remote_unavailable",
    message:
      detail === undefined
        ? `来源节点「${ownerNodeId}」当前不可用；远程会话只在来源在线时可读取。`
        : detail,
    retryable: true,
    details: {},
  };
}

/** 某个会话的远程来源坐标；本节点自有会话或未知时为 `null`。 */
function remoteRefOf(state: ClientState, sessionId: Uuid): SessionRef | null {
  const summary = state.resources?.sessions.find((item) => item.sessionId === sessionId);
  if (summary !== undefined && summary.origin.kind === "remote") {
    return { ownerNodeId: summary.origin.ownerNodeId, sessionId };
  }
  const runtime = state.remoteSessions.get(sessionId);
  return runtime === undefined ? null : { ownerNodeId: runtime.ownerNodeId, sessionId };
}

/** 来源代数：不存在时为 0（首次清除从 1 开始，在途读取凭旧值被丢弃）。 */
function sourceGenerationOf(state: ClientState, ownerNodeId: Uuid): number {
  return state.remoteSources.get(ownerNodeId)?.generation ?? 0;
}

/**
 * 清除**一个 Owner** 的远程展示数据（不触碰本节点自有资源与其它来源）。
 *
 * 这是 spec「来源断开时清除该来源的业务展示数据」的唯一实现：会话摘要、实时/历史结构化
 * 记录、易失增量、线程、读回的明细资源、用量与来源命令记录一起清除；运行期模型保留，
 * 以便显示「不可用」而不是一段凭空消失的页面。`generation` 递增使在途读取失效。
 *
 * @param nextState 该来源的新连接事实（`offline`/`revoked`/`unknown`）。
 * @param error 明确的不可用原因；`null` 表示只清数据（例如主动重建前）。
 */
function clearRemoteOwnerData(
  state: ClientState,
  ownerNodeId: Uuid,
  nextState: RemoteSourceKind,
  error: PublicError | null,
): ClientState {
  const generation = sourceGenerationOf(state, ownerNodeId) + 1;
  const sessionIds = new Set<Uuid>();
  for (const summary of state.resources?.sessions ?? []) {
    if (summary.origin.kind === "remote" && summary.origin.ownerNodeId === ownerNodeId) {
      sessionIds.add(summary.sessionId);
    }
  }
  for (const [sessionId, runtime] of state.remoteSessions) {
    if (runtime.ownerNodeId === ownerNodeId) sessionIds.add(sessionId);
  }
  for (const record of state.structured.values()) {
    if (record.ownerNodeId === ownerNodeId && record.sessionId !== null) sessionIds.add(record.sessionId);
  }
  for (const record of state.history.values()) {
    if (record.ownerNodeId === ownerNodeId && record.sessionId !== null) sessionIds.add(record.sessionId);
  }

  const resources =
    state.resources === null
      ? null
      : {
          ...state.resources,
          sessions: state.resources.sessions.filter(
            (summary) => !(summary.origin.kind === "remote" && summary.origin.ownerNodeId === ownerNodeId),
          ),
        };
  const structured = new Map(state.structured);
  for (const [key, record] of structured) {
    if (record.ownerNodeId === ownerNodeId) structured.delete(key);
  }
  const history = new Map(state.history);
  for (const [key, record] of history) {
    if (record.ownerNodeId === ownerNodeId) history.delete(key);
  }
  const volatile = new Map(state.volatile);
  for (const [messageId, tail] of volatile) {
    if (tail.sessionId !== null && sessionIds.has(tail.sessionId)) volatile.delete(messageId);
  }
  const threads = new Map(state.threads);
  const readResources = new Map(state.readResources);
  const usage = new Map(state.usage);
  const modeStates = new Map(state.modeStates);
  const switchIntents = new Map(state.switchIntents);
  const attachmentRuns = new Map(state.attachmentRuns);
  const promptRequestIds = new Map(state.promptRequestIds);
  const sentPromptAnchors = new Map(state.sentPromptAnchors);
  const commands = new Map(state.commands);
  const removedRequests = new Set<Uuid>();
  for (const sessionId of sessionIds) {
    threads.delete(sessionId);
    readResources.delete(sessionId);
    usage.delete(sessionId);
    modeStates.delete(sessionId);
    switchIntents.delete(sessionId);
    attachmentRuns.delete(sessionId);
    promptRequestIds.delete(sessionId);
    // 会话被移除（来源数据清除）后，它的接替锚点不再指向任何仍在呈现的正文：整桶丢弃。
    sentPromptAnchors.delete(sessionId);
  }
  for (const [requestId, record] of commands) {
    if (record.sessionId !== null && sessionIds.has(record.sessionId)) {
      commands.delete(requestId);
      removedRequests.add(requestId);
    }
  }
  const uncertainRequestIds = state.uncertainRequestIds.filter((id) => !removedRequests.has(id));
  const remoteSessions = new Map(state.remoteSessions);
  for (const sessionId of sessionIds) {
    const existing = remoteSessions.get(sessionId);
    remoteSessions.set(sessionId, {
      ownerNodeId,
      sessionId,
      phase: nextState === "online" ? "subscribing" : "unavailable",
      sourceState: nextState,
      cursor: existing?.cursor ?? null,
      caughtUp: false,
      rebuilt: false,
      coverage: EMPTY_REMOTE_COVERAGE,
      error: nextState === "online" ? null : error,
      generation,
    });
  }
  const remoteSources = new Map(state.remoteSources);
  remoteSources.set(ownerNodeId, { state: nextState, generation });
  // 文件栏若正打开该来源会话的文件，内容同样属于远程展示数据：退回对话态并递增代次，
  // 使在途查询的结果无法再落回界面。
  const sessionFiles =
    state.sessionFiles.content.kind === "file" &&
    sessionIds.has(state.sessionFiles.content.context.sessionId)
      ? {
          ...state.sessionFiles,
          content: { kind: "conversation" as const },
          generation: state.sessionFiles.generation + 1,
        }
      : state.sessionFiles;

  return {
    ...state,
    resources,
    structured,
    history,
    volatile,
    threads,
    readResources,
    usage,
    modeStates,
    switchIntents,
    attachmentRuns,
    promptRequestIds,
    sentPromptAnchors,
    commands,
    uncertainRequestIds,
    remoteSessions,
    remoteSources,
    sessionFiles,
  };
}

/** 清空**全部**远程来源（连接断开/阻断时）：本节点自有资源保持原样。 */
function clearAllRemoteData(state: ClientState): ClientState {
  const owners = new Set<Uuid>();
  for (const [owner] of state.remoteSources) owners.add(owner);
  for (const runtime of state.remoteSessions.values()) owners.add(runtime.ownerNodeId);
  for (const summary of state.resources?.sessions ?? []) {
    if (summary.origin.kind === "remote") owners.add(summary.origin.ownerNodeId);
  }
  let next = state;
  for (const owner of owners) {
    next = clearRemoteOwnerData(next, owner, "unknown", remoteUnavailableError(owner));
  }
  return next;
}

/**
 * 清除**一个远程会话**的展示数据（订阅结束/来源重置）。
 *
 * 与 `clearRemoteOwnerData` 的差别只在范围：摘要保留（来源仍在线时它不是缓存），但线程、
 * 记录、易失增量与来源命令记录必须丢弃，因为下一次呈现只能来自新的原子读取。
 */
function clearRemoteSessionData(
  state: ClientState,
  runtime: RemoteSessionRuntime,
  phase: RemoteSessionRuntime["phase"],
  error: PublicError | null,
): ClientState {
  const ownerNodeId = runtime.ownerNodeId;
  const sessionId = runtime.sessionId;
  const generation = sourceGenerationOf(state, ownerNodeId) + 1;
  const structured = new Map(state.structured);
  for (const [key, record] of structured) {
    if (record.ownerNodeId === ownerNodeId && record.sessionId === sessionId) structured.delete(key);
  }
  const history = new Map(state.history);
  for (const [key, record] of history) {
    if (record.ownerNodeId === ownerNodeId && record.sessionId === sessionId) history.delete(key);
  }
  const volatile = new Map(state.volatile);
  for (const [messageId, tail] of volatile) {
    if (tail.sessionId === sessionId) volatile.delete(messageId);
  }
  const threads = new Map(state.threads);
  threads.delete(sessionId);
  const readResources = new Map(state.readResources);
  readResources.delete(sessionId);
  const usage = new Map(state.usage);
  usage.delete(sessionId);
  const modeStates = new Map(state.modeStates);
  modeStates.delete(sessionId);
  const switchIntents = new Map(state.switchIntents);
  switchIntents.delete(sessionId);
  const attachmentRuns = new Map(state.attachmentRuns);
  attachmentRuns.delete(sessionId);
  const promptRequestIds = new Map(state.promptRequestIds);
  promptRequestIds.delete(sessionId);
  // 锚点只对「这条已读回并留在时间线上的正式消息」有意义：会话数据被清空后页面要重建，
  // 留下的锚点只会变成永不回收的键。
  const sentPromptAnchors = new Map(state.sentPromptAnchors);
  sentPromptAnchors.delete(sessionId);
  const commands = new Map(state.commands);
  const removedRequests = new Set<Uuid>();
  for (const [requestId, record] of commands) {
    if (record.sessionId === sessionId) {
      commands.delete(requestId);
      removedRequests.add(requestId);
    }
  }
  const remoteSessions = new Map(state.remoteSessions);
  remoteSessions.set(sessionId, {
    ...runtime,
    phase,
    caughtUp: false,
    rebuilt: false,
    coverage: EMPTY_REMOTE_COVERAGE,
    error,
    generation,
  });
  const remoteSources = new Map(state.remoteSources);
  remoteSources.set(ownerNodeId, { state: runtime.sourceState, generation });
  const sessionFiles =
    state.sessionFiles.content.kind === "file" &&
    state.sessionFiles.content.context.sessionId === sessionId
      ? {
          ...state.sessionFiles,
          content: { kind: "conversation" as const },
          generation: state.sessionFiles.generation + 1,
        }
      : state.sessionFiles;
  return {
    ...state,
    structured,
    history,
    volatile,
    threads,
    readResources,
    usage,
    modeStates,
    switchIntents,
    attachmentRuns,
    promptRequestIds,
    sentPromptAnchors,
    commands,
    uncertainRequestIds: state.uncertainRequestIds.filter((id) => !removedRequests.has(id)),
    remoteSessions,
    remoteSources,
    sessionFiles,
  };
}

/** 会话摘要 → 替换 `resources.sessions` 里的同 id 条目（目录汇总随之更新）。 */
function withSessionSummary(resources: StagedResources | null, summary: SessionSummary): StagedResources | null {
  if (resources === null) return resources;
  const rest = resources.sessions.filter((item) => item.sessionId !== summary.sessionId);
  return { ...resources, sessions: [summary, ...rest] };
}

/**
 * 远程读取结果里的原子 `sessionSummary`：只有**不更旧**的版本才替换清单条目。
 *
 * 读取位置 R 与清单可能隔着一次实时更新；用更旧的读取摘要覆盖清单会让标题/状态回滚
 * （D6 明确要求用读取时的摘要与版本防止这一竞态）。
 */
function withVersionedSummary(
  resources: StagedResources | null,
  summary: SessionSummary,
): StagedResources | null {
  if (resources === null) return resources;
  const existing = resources.sessions.find((item) => item.sessionId === summary.sessionId);
  if (existing !== undefined) {
    const incoming = BigInt(summary.version);
    const current = BigInt(existing.version);
    if (incoming < current) return resources;
  }
  return withSessionSummary(resources, summary);
}

/**
 * `session.created`/`session.updated` 视图里的会话摘要。
 *
 * 历史事件视图可能携带升级前的七态标签（`queued`/`waiting_*`/`failed`/`closed`）。那些标签是
 * **带版本的过去事实**：实时主状态只能来自最新的权威三态摘要，因此这里只接受
 * `idle|running|pending`，其余整条摘要都不采用——旧标签既不会覆盖当前状态，也不会抛错
 * （design D4：不能用旧标签覆盖新状态）。缺少合法状态的摘要同样不采用。
 */
function sessionSummaryIn(event: EventMessage): SessionSummary | null {
  if (
    event.body.eventType !== "session.created" &&
    event.body.eventType !== "session.updated" &&
    event.body.eventType !== "session.activity.changed"
  ) {
    return null;
  }
  const summary = event.body.payload.view["session"];
  if (typeof summary !== "object" || summary === null || !("sessionId" in summary)) return null;
  const candidate = summary.sessionId;
  if (typeof candidate !== "string" || candidate.length === 0) return null;
  const state = "state" in summary ? summary.state : undefined;
  if (state !== "idle" && state !== "running" && state !== "pending") return null;
  return summary as SessionSummary;
}

/** 远程事件摘要：会话标识、来源节点都必须与订阅一致，否则不采用。 */
function isRemoteSummary(value: unknown, sessionId: Uuid, ownerNodeId: Uuid): value is SessionSummary {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<SessionSummary>;
  if (candidate.sessionId !== sessionId) return false;
  const origin = candidate.origin;
  return (
    typeof origin === "object" &&
    origin !== null &&
    origin.kind === "remote" &&
    origin.ownerNodeId === ownerNodeId
  );
}

/** `session.usage.changed` 视图里的两项计数。 */
function usageIn(event: EventMessage): UsageReading | null {
  if (event.body.eventType !== "session.usage.changed") return null;
  const view = event.body.payload.view;
  const used = view["used"];
  const size = view["size"];
  if (typeof used !== "string" || typeof size !== "string") return null;
  return { used, size };
}

/**
 * `session.origin.online_changed` 视图里的来源可达性（D5 顺带口径）。
 *
 * 该事件**只**改可达性这一条标注：`ownerNodeId`/`exportId`/`originEpoch` 是同一来源的坐标，
 * 事件带的是它们的副本，用它去重写摘要会让「来源」在两个地方各有一份权威。因此这里只取
 * `online`，其余字段由 {@link withOriginOnline} 逐字段保持原样。
 */
function originOnlineIn(event: EventMessage): boolean | null {
  if (event.body.eventType !== "session.origin.online_changed") return null;
  const online = event.body.payload.view["online"];
  return typeof online === "boolean" ? online : null;
}

/**
 * 只改 `origin.online`，摘要其余字段**逐字段保持**（同一个值、同一个引用）。
 *
 * 本节点自有的会话没有可标注的来源（`origin.kind === "local"`）——那时返回原对象，
 * 不把 `local` 改写成 `remote`（那会凭空给会话安一个来源节点）。
 */
export function withOriginOnline(
  resources: StagedResources | null,
  sessionId: Uuid,
  online: boolean,
): StagedResources | null {
  if (resources === null) return resources;
  const target = resources.sessions.find((summary) => summary.sessionId === sessionId);
  if (target === undefined || target.origin.kind !== "remote") return resources;
  if (target.origin.online === online) return resources;
  const sessions = resources.sessions.map((summary) =>
    summary.sessionId === sessionId
      ? { ...summary, origin: { ...summary.origin, online } }
      : summary,
  );
  return { ...resources, sessions };
}

/** `CommandResultValue` → 可展开成键值对象的结果；非对象结果（如 `modeList`）取 `null`。 */
function toResultRecord(value: unknown): Readonly<Record<string, unknown>> | null {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
  return value as Readonly<Record<string, unknown>>;
}

/**
 * 会话有效标题（`design.md` D1/D7）：服务端已把「自定义 > Agent > 未命名」合并进
 * `SessionSummary.title`，客户端只读这一个值，不复制 Agent 标题、不另建优先级。
 * 空标题呈现为未命名占位。
 */
function titleOf(summary: SessionSummary | undefined): string {
  const title = summary?.title;
  return title === undefined || title === null || title.length === 0 ? "未命名会话" : title;
}

/** 标题命令失败/结果未知的呈现文案（不编造完成）。 */
function titleFailureText(status: CommandResultStatus, error: PublicError | null): string {
  if (error !== null) {
    return error.retryable
      ? `${error.code}：${error.message}（可重试）`
      : `${error.code}：${error.message}`;
  }
  if (status === "uncertain") {
    return "标题修改结果未知；恢复连接后会查询原请求，不会自动重发。";
  }
  return `标题修改未成功（${status}）。`;
}

/** 命令失败的说明（结构化：码 + 消息 + 可重试）。 */
function describeCommandFailure(result: CommandResultMessage): string {
  const error = result.body.error;
  if (error === null) return `${result.body.command} 未成功（${result.body.status}）`;
  return error.retryable ? `${error.code}：${error.message}（可重试）` : `${error.code}：${error.message}`;
}

/** `partialResult` 视图；形状不合规时返回 `null`（不编造完成状态）。 */
function partialViewFrom(value: unknown): PartialResultView | null {
  const decoded = decodePartialResult(value);
  if (!decoded.ok) return null;
  return {
    state: decoded.value.state,
    reason: decoded.value.reason,
    label: PARTIAL_RESULT_LABELS[decoded.value.state],
  };
}

/**
 * 历史条目并入状态（按 `eventId` 去重）。
 *
 * 去重的两道口子都在这里：实时已收到的事件不因历史再收一遍而多出一条（首次呈现的
 * 那份记录更全——它带 `globalSequence` 与本次投递的原文字节）；同一历史页重复读取也不重复。
 */
function withHistory(
  state: ClientState,
  sessionId: Uuid,
  items: readonly SnapshotItemHistory[],
  ownerNodeId: string | null = null,
): ClientState {
  const history = new Map(state.history);
  for (const item of items) {
    const record = recordFromHistoryItem(item, sessionId, ownerNodeId);
    const key = recordKey(record);
    if (state.structured.has(key) || history.has(key)) continue;
    history.set(key, record);
  }
  return { ...state, history };
}

/** 读回的待处理交互逐项解码（协商 `core.pending-interactions.v1` 后的 `effectiveInteraction` 与旧定位形状）。 */
function decodePendingInteractions(raw: readonly unknown[]): PendingInteractionInput[] {
  const pendingInteractions: PendingInteractionInput[] = [];
  for (const item of raw) {
    const decoded = decodeEffectiveInteraction(item);
    if (decoded.ok) {
      pendingInteractions.push({
        interactionId: decoded.value.interactionId,
        sessionId: decoded.value.sessionId,
        kind: decoded.value.kind,
        requestVersion: decoded.value.requestVersion,
        ...(decoded.value.sequence === undefined ? {} : { sequence: decoded.value.sequence }),
        expiresAt: decoded.value.expiresAt,
        options: decoded.value.options,
        schema: decoded.value.schema,
        effective: true,
        createdAt: null,
      });
      continue;
    }
    // 旧 peer 的定位形状（无 requestVersion/options）：保留为不可操作卡片，不静默丢掉。
    const legacy = legacyPendingInteraction(item);
    if (legacy !== null) pendingInteractions.push(legacy);
  }
  return pendingInteractions;
}

/** 首屏读回的明细资源落到会话上（待处理交互 + 配置项，D7 与 D5 的配置面）。 */
function withReadResources(state: ClientState, sessionId: Uuid, read: SessionReadResult): ClientState {
  const item = read.resources.config_options?.[0];
  const capabilityItem = read.resources.capabilities?.[0];
  const readResources = new Map(state.readResources);
  readResources.set(sessionId, {
    pendingInteractions: decodePendingInteractions(read.resources.pending_interactions ?? []),
    configOptions: item?.configOptions ?? [],
    configVersion: item?.version ?? null,
    agentCapabilities:
      capabilityItem !== undefined && typeof capabilityItem.agentCapabilities === "object" && capabilityItem.agentCapabilities !== null
        ? capabilityItem.agentCapabilities
        : null,
  });
  return { ...state, readResources };
}

/**
 * 刷新有效交互的结果落到会话上：**只**替换 `pendingInteractions`。
 *
 * `config_options` 与 `capabilities` 来自首屏读取（它们不在这次请求的 include 里）；这里必须
 * 原样保留，否则一次「重新取得当前有效请求」会把模型/模式可选项和 Agent 能力声明一起清空。
 */
function withPendingInteractions(state: ClientState, sessionId: Uuid, read: SessionReadResult): ClientState {
  const existing = state.readResources.get(sessionId);
  const readResources = new Map(state.readResources);
  readResources.set(sessionId, {
    pendingInteractions: decodePendingInteractions(read.resources.pending_interactions ?? []),
    configOptions: existing?.configOptions ?? [],
    configVersion: existing?.configVersion ?? null,
    agentCapabilities: existing?.agentCapabilities ?? null,
  });
  return { ...state, readResources };
}

/** 旧 peer 的 `snapshotItem.pending_interactions` 形状 → 不可操作的内部条目。 */
function legacyPendingInteraction(value: unknown): PendingInteractionInput | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const interactionId: unknown = Reflect.get(value, "interactionId");
  const sessionId: unknown = Reflect.get(value, "sessionId");
  const kind: unknown = Reflect.get(value, "kind");
  const schema: unknown = Reflect.get(value, "schema");
  const createdAt: unknown = Reflect.get(value, "createdAt");
  if (
    typeof interactionId !== "string" ||
    typeof sessionId !== "string" ||
    (kind !== "permission" && kind !== "elicitation")
  ) {
    return null;
  }
  return {
    interactionId,
    sessionId,
    kind,
    requestVersion: null,
    expiresAt: null,
    options: null,
    schema: typeof schema === "object" && schema !== null ? (schema as Readonly<Record<string, unknown>>) : null,
    effective: false,
    createdAt: typeof createdAt === "string" ? createdAt : null,
  };
}

/** 一次用户意图的提交结果：成功给出 `requestId`；失败给出原因且**没有派发任何命令**。 */
export type CommandOutcome =
  | { readonly ok: true; readonly requestId: Uuid }
  | { readonly ok: false; readonly reason: string };

/** 创建会话的提交结果（与 {@link CommandOutcome} 同一形状的历史名）。 */
export type CreateOutcome = CommandOutcome;

/** 「新建文件夹」的结果：成功给出候选路径（它随后成为选定结果），失败给出原因且**没有**改选定。 */
export type CreateFolderOutcome = CommandOutcome;

/** 未建立已认证连接时的统一拒绝理由（命令信封需要 `connectionId`）。 */
const NOT_CONNECTED: CommandOutcome = { ok: false, reason: "尚未建立已认证连接，无法提交命令" };

/** 空线程（首次读取前）。 */
const EMPTY_THREAD: ConversationThreadState = {
  page: null,
  items: [],
  historyHasEarlier: false,
  nextBeforeHistory: null,
  loadingEarlier: false,
  loadingHistory: false,
  pagingError: null,
};

/** 轮次生命周期的五个事件类型（分组与「进行中」判定都只看它们）。 */
const TURN_EVENT_TYPES: Readonly<Record<string, true>> = {
  "turn.queued": true,
  "turn.started": true,
  "turn.completed": true,
  "turn.cancelled": true,
  "turn.failed": true,
};

/** 轮次的终态：这三个状态之外都算「进行中」（可取消）。 */
const TERMINAL_TURN_STATES: Readonly<Record<string, true>> = {
  completed: true,
  cancelled: true,
  failed: true,
};

/** 不可变地写入一个键。 */
function withEntry<K, V>(map: ReadonlyMap<K, V>, key: K, value: V): ReadonlyMap<K, V> {
  const next = new Map(map);
  next.set(key, value);
  return next;
}

/** 不可变地删除一个键（键不存在时返回原映射）。 */
function withoutEntry<K, V>(map: ReadonlyMap<K, V>, key: K): ReadonlyMap<K, V> {
  if (!map.has(key)) return map;
  const next = new Map(map);
  next.delete(key);
  return next;
}

/**
 * 权威用户消息接替本地回显：**同一次状态更新**里移除已接替的本地条目，并登记
 * `messageId → requestId` 锚点。
 *
 * 依据只有可靠标识：`prompt.messageId` 等于该消息的 `messageId`，或 `prompt.turnId` 等于该消息
 * 的 `turnId`。相同文字、相近时间都不是身份——同文两次发送是两条独立输入。
 *
 * 锚点让时间线继续用本地条目的稳定键承载这条正式消息；本地条目同时消失，因此「页替换/重读后
 * 回显重新出现」在状态层就不可能发生（显示层的同身份让位判据仍保留，作为第二道防线）。
 */
function withPromptTakeover(
  state: ClientState,
  sessionId: Uuid,
  confirmed: readonly {
    readonly messageId: string;
    readonly turnId: string | null;
    readonly causationRequestId?: string | null;
  }[],
): ClientState {
  if (state.sentPrompts.size === 0 || confirmed.length === 0) return state;
  let prompts: Map<Uuid, SentPrompt> | null = null;
  let anchors: Map<Uuid, ReadonlyMap<string, string>> | null = null;
  for (const prompt of state.sentPrompts.values()) {
    if (prompt.sessionId !== sessionId) continue;
    const match = confirmed.find((item) =>
      (prompt.messageId !== null && prompt.messageId === item.messageId) ||
      (prompt.turnId !== null && item.turnId !== null && prompt.turnId === item.turnId) ||
      (item.causationRequestId != null && prompt.requestId === item.causationRequestId));
    if (match === undefined) continue;
    prompts ??= new Map(state.sentPrompts);
    prompts.delete(prompt.requestId);
    anchors ??= new Map(state.sentPromptAnchors);
    const sessionAnchors = new Map(anchors.get(sessionId) ?? []);
    sessionAnchors.set(match.messageId, prompt.requestId);
    anchors.set(sessionId, sessionAnchors);
  }
  if (prompts === null || anchors === null) return state;
  return { ...state, sentPrompts: prompts, sentPromptAnchors: anchors };
}

/**
 * 创建一条本地提交条目（唯一构造点，保证每个创建路径都带上阶段与身份字段）。
 *
 * `turnId`/`messageId` 只放**已知**的可靠标识：拿不到就留 `null`，让条目继续以「本地待接替」
 * 的形式存在，而不是退化成按文字猜测。
 */
function sentPromptOf(input: {
  readonly requestId: Uuid;
  readonly sessionId: Uuid;
  readonly text: string;
  readonly attachmentNames: readonly string[];
  readonly createdAt: string;
  readonly phase: SentPromptPhase;
  readonly turnId?: string | null;
  readonly messageId?: string | null;
  readonly notice?: string | null;
}): SentPrompt {
  return {
    requestId: input.requestId,
    sessionId: input.sessionId,
    text: input.text,
    attachmentNames: input.attachmentNames,
    createdAt: input.createdAt,
    phase: input.phase,
    turnId: input.turnId ?? null,
    messageId: input.messageId ?? null,
    notice: input.notice ?? null,
  };
}

/**
 * 受理结果里的 `turnId`（`session.prompt` 的 `accepted { turnId }`）。
 *
 * 只认字符串字段；形状不符时返回 `null`——不能把「读不到」当成某个轮次标识。
 */
function acceptedTurnIdOf(result: unknown): string | null {
  const record = toResultRecord(result);
  const turnId = record === null ? null : record["turnId"];
  return typeof turnId === "string" && turnId.length > 0 ? turnId : null;
}

/** 纯函数版：更新一条本地提交的阶段/身份（不存在时原样返回，不用迟到结果复活条目）。 */
function withPromptPhase(
  state: ClientState,
  requestId: Uuid,
  update: (prompt: SentPrompt) => SentPrompt,
): ClientState {
  const current = state.sentPrompts.get(requestId);
  if (current === undefined) return state;
  const next = update(current);
  if (next === current) return state;
  return { ...state, sentPrompts: withEntry(state.sentPrompts, requestId, next) };
}

/** 按时间位置取最新的一条记录；无记录时为 `null`。 */
function newestRecordOf(records: readonly StructuredRecord[]): StructuredRecord | null {
  let newest: StructuredRecord | null = null;
  for (const record of records) {
    if (newest === null || compareRecordOrder(record, newest) > 0) newest = record;
  }
  return newest;
}

/**
 * 当前进行中的轮次（`turn.*` 里最新一条的状态不在终态集合里）。
 *
 * 取「最新一条」而不是「有没有 `turn.started`」：轮次的终态可能由 `turn.completed`/
 * `turn.cancelled`/`turn.failed` 中的任意一条给出，只有最新状态才是当前的。
 */
function activeTurnOf(records: readonly StructuredRecord[]): { readonly turnId: string } | null {
  const latest = newestRecordOf(records.filter((record) => TURN_EVENT_TYPES[record.eventType] === true));
  if (latest === null) return null;
  const state = viewText(latest.view, "state");
  const turnId = viewText(latest.view, "turnId");
  if (state === null || turnId === null) return null;
  return TERMINAL_TURN_STATES[state] === true ? null : { turnId };
}

/**
 * 应答前的闸门：只有 `answerable` 允许提交，其余边界各自给出**明确**原因（D7）。
 *
 * 每种原因都对应 spec 的一条场景：仅历史读到、已被其它设备处理、已提交待确认、本设备已处理。
 */
function responseGate(interaction: InteractionView | null): CommandOutcome | null {
  if (interaction === null) return { ok: false, reason: "这条待处理交互不在本设备的状态里" };
  const availability = interaction.availability;
  switch (availability.kind) {
    case "answerable":
      return null;
    case "submitting":
      return { ok: false, reason: "这条交互的应答已经提交，正在等待服务端终态" };
    case "handled_here":
      return { ok: false, reason: `这条交互已由本设备处理（${availability.resolution}）` };
    case "handled_elsewhere":
      return { ok: false, reason: `这条交互已被其它设备处理（${availability.resolution}）` };
    case "not_answerable":
      return { ok: false, reason: INTERACTION_UNAVAILABLE_LABELS[availability.reason] };
  }
}

/** `ClientStore` 的构造输入。 */
export interface ClientStoreOptions {
  readonly sync: SyncGateway;
  readonly ids: IdentifierSource;
  readonly now: () => number;
  /** 已建立连接的标识；未连接为 `null`（此时不派发任何命令）。 */
  readonly identity: ConnectionIdentity;
  /** imported 会话来源节点的展示名。 */
  readonly ownerLabelOf?: (ownerNodeId: string) => string;
  /**
   * 配对链路（组合根注入）。
   *
   * 未装配时配对用例**明确失败**而不是静默无操作：没有传输就没有配对，让页面停在
   * 「正在配对」比当场说清楚更糟。测试替身注入它即可完整驱动配对流程（含 claim 与轮询）。
   */
  readonly pairing?: PairingWiring | undefined;
  /**
   * 退避定时端口（文件栏的突发事件合并与限流重试）。缺省不安排定时器：测试可直接调用
   * `refreshFiles` 驱动，生产由组合根注入时钟的 `schedule`。
   */
  readonly schedule?: ((delayMs: number, task: () => void) => () => void) | undefined;
  /**
   * 附件与媒体端口（组合根装配）。缺省时附件能力**明确不可用**：`attachmentsSupported`
   * 为假并给出说明，不会伪造一个「点了没反应」的上传入口。
   */
  readonly attachments?:
    | {
        readonly media: AttachmentMediaPort;
        readonly transfer: AttachmentTransferGateway;
      }
    | undefined;
  /**
   * 浏览器准备草稿端口（组合根注入平台实现）。
   *
   * 缺省时准备流程仍可分配/发送（草稿只在内存），但页面 MUST 明确说明「刷新无法恢复」，
   * 不能报告完整保存成功。
   */
  readonly preparationDrafts?: PreparationDraftGateway | undefined;
  /**
   * 每个标签页的准备归属 nonce（32 字节 base64url，进入凭据 claims、不进入 URL）。
   *
   * 缺省时无法发起准备：凭据不能凭空拼出一个 nonce，宁可在入口明确失败。
   */
  readonly preparationTabNonce?: (() => string) | undefined;
}

/** 一条在途的文件栏查询（请求代次与返回上下文在结果到达时逐项核对）。 */
interface PendingFileQuery {
  readonly refreshId?: number;
  readonly requestId: Uuid;
  readonly kind: "list" | "read" | "git_status" | "git_diff";
  readonly sessionId: Uuid;
  readonly source: FileSource;
  readonly path: string | null;
  readonly generation: number;
  /** 列表/状态的续页标记：`undefined` = 第一页，`null` = 没有更多。 */
  readonly after?: string | undefined;
  /** 是否把结果追加到已有页（翻页）。 */
  readonly append?: boolean | undefined;
  /** 预览续块：已拼接的字节与版本。 */
  readonly chunks?: Uint8Array[] | undefined;
  readonly version?: string | undefined;
  readonly sizeBytes?: string | undefined;
  readonly fileKind?: "text" | "image" | "binary" | undefined;
  readonly previewTruncated?: boolean | undefined;
}

/**
 * 正在进行的一次配对。
 *
 * **不进 `ClientState`**：lookup 响应含 `pairingSecret`，而 `ClientState` 是可导出、可快照的
 * 公开状态（诊断与导出配置都会读它）。载荷只活在仓库的私有字段里，并在已批准或终态时立即
 * 丢掉（§7.3：成功或终态后清除配对 payload 与内存中的 secret）。
 *
 * 浏览器唯一的配对入口是 8 位配对码；`qr` 由 lookup 响应拼成（与配对资料同形），
 * 设备二维码不会进入本会话（design D6）。
 */
interface PairingSession {
  readonly qr: PairingLookupResponse;
  /** 取消在途的状态轮询。 */
  cancel: (() => void) | null;
  /** 取消按载荷 `expiresAt` 安排的到期回调（design D3）。 */
  expiryCancel: (() => void) | null;
}

type Listener = () => void;

/**
 * 客户端状态仓库。
 *
 * 一个实例覆盖整个应用生命周期：组合根把 `SyncClient` 作为 `sync` 传进来，
 * 并把 `onEvent` 接到 `apply`。页面只读它的选择器结果与用户意图方法。
 */
export class ClientStore {
  readonly #sync: SyncGateway;
  readonly #ids: IdentifierSource;
  readonly #now: () => number;
  readonly #identity: ConnectionIdentity;
  readonly #ownerLabelOf: (ownerNodeId: string) => string;
  readonly #pairing: PairingWiring | null;
  readonly #overlayStore = new AgentConnectionOverlayStore();
  readonly #pendingReads = new Map<Uuid, PendingRead>();
  /** Fixed lower bound while the two independently paged resources catch up. */
  readonly #timelineFloors = new Map<Uuid, string>();
  /**
   * 每个会话最近一次的时间线条目（只用于**引用复用**，不是正文缓存）。
   *
   * `conversationPage()` 每次都会重建条目；没有这一层，`memo` 化的条目视图会被新对象全部击穿，
   * 一条流式增量就会重新渲染所有历史正文（D1）。
   */
  readonly #timelineEntries = new Map<Uuid, readonly TimelineEntry[]>();
  readonly #listeners = new Set<Listener>();
  readonly #directoryRefreshRequests = new Set<Uuid>();
  readonly #schedule: ((delayMs: number, task: () => void) => () => void) | null;
  /** 在途的文件栏查询（含请求代次与返回上下文）。 */
  readonly #pendingFileQueries = new Map<Uuid, PendingFileQuery>();
  readonly #expandedFileDirectories = new Map<Uuid, readonly string[]>();
  readonly #fileRefreshRounds = new Map<string, { id: number; remaining: number; failed: boolean }>();
  #fileRefreshId = 0;
  #activeFileRefreshNeeded = false;
  /** 突发事件合并/限流退避定时器；键为 `<sessionId>|<source>`。 */
  readonly #fileRefreshTimers = new Map<string, () => void>();
  /** 已排队但在飞的刷新：一次只允许一个同来源查询。 */
  readonly #fileRefreshQueued = new Set<string>();
  /** 压缩通知后待重读权威消息的会话（`turn.delta_compacted`）。 */
  readonly #messageRefreshQueued = new Set<Uuid>();
  readonly #interactionRefreshQueued = new Set<Uuid>();
  /** 每条标题命令本连接上已发出过结果查询的 `requestId`（重连后清空，不换号重发）。 */
  readonly #titleStatusQueries = new Set<Uuid>();
  /** 首次读取在连接认证前被丢弃的会话；`authenticated` 后补发。 */
  readonly #deferredReads = new Set<Uuid>();
  /** 远程来源在订阅/重置/重连后待发起的原子读取重建。 */
  readonly #remoteRebuilds = new Set<Uuid>();
  /** `session.prompt` 派发前的草稿/附件快照：被拒绝或失败时恢复（不静默丢用户内容）。 */
  readonly #promptDrafts = new Map<Uuid, {
    readonly sessionId: Uuid;
    readonly text: string;
    readonly pathRefs: readonly string[];
    readonly attachments: readonly DraftAttachment[];
  }>();
  /** 当前对话页的会话；切换时旧会话的文件读取必须作废（迟到的响应不能重新打开文件）。 */
  #activeConversationSession: Uuid | null = null;
  /** 查看续租的计时器句柄（每条连接只有一个，离页/隐藏/断开即取消）。 */
  #viewRenewCancel: (() => void) | null = null;
  /** 防止 `#ensureLifecycle` 在提交与订阅回调之间递归重入。 */
  #syncingLifecycle = false;
  /** 附件端口（组合根装配）；未装配时附件入口明确不可用。 */
  readonly #attachmentMedia: AttachmentMediaPort | null;
  readonly #attachmentTransfer: AttachmentTransferGateway | null;
  /** 平台句柄只活在私有表里；公开状态只带元数据与对象 URL。 */
  readonly #attachmentHandles = new Map<string, AttachmentFileHandle>();
  /** 正在进行的上传（避免同一会话重复启动两批）。 */
  readonly #uploadingSessions = new Set<Uuid>();
  /** 每个会话当前在途的传输标识（取消时用它发 `attachment.abort`）。 */
  readonly #activeTransfers = new Map<Uuid, string>();
  /** 被用户取消的上传（在途轮询到它即中止，且整批标为已取消）。 */
  readonly #cancelledUploads = new Set<Uuid>();
  /**
   * 每次媒体读取的代次（按 `attachmentId`）。释放/重新读取都会换一个代次，因此迟到的读取结果
   * 既不会重新挂上已撤销的对象 URL，也不会覆盖后一次读取。
   */
  readonly #mediaReads = new Map<string, number>();
  /** 同一附件的在途读取：并发调用共享同一个 Promise；代次用于跨卸载/重建串行化。 */
  readonly #mediaLoads = new Map<string, { readonly generation: number; readonly promise: Promise<void> }>();
  /** 浏览器准备草稿端口（未装配时草稿只在内存，页面如实说明刷新不可恢复）。 */
  readonly #preparationDrafts: PreparationDraftGateway | null;
  readonly #preparationTabNonce: (() => string) | null;
  /** 每个准备会话的草稿访问句柄与最近一次写入的版本。 */
  readonly #preparationDraftAccess = new Map<
    Uuid,
    { readonly access: PreparationDraftAccess; revision: number | null; readonly restorable: boolean }
  >();
  /** 等待分配结果的 `allocate` 操作（按 `operationId`）。迟到的旧操作不在表内即被忽略。 */
  readonly #pendingAllocations = new Map<
    Uuid,
    {
      readonly resolve: (outcome: PreparationBeginOutcome) => void;
      readonly agentId: string;
      readonly workspace: PreparationWorkspace;
      readonly tabNonce: string;
    }
  >();
  /** 每个准备会话当前在飞的准备操作；应答必须与它逐字匹配才生效。 */
  readonly #preparationOps = new Map<Uuid, { readonly operationId: Uuid; readonly kind: "restore" | "configure" | "renew" | "release" }>();
  /** 每个 ready 准备的续租计时器（隐藏/断开/受理/释放都取消）。 */
  readonly #preparationRenewTimers = new Map<Uuid, () => void>();
  /** 本连接上已经发过的首条状态查询（重连后清空；同一目标不重复查）。 */
  readonly #preparationStatusQueries = new Set<Uuid>();
  /** 「先核实原 start，再 restore」的待办会话（`preparation.stale` 的固定动作）。 */
  readonly #preparationVerifyThenRestore = new Set<Uuid>();
  /** 已恢复的附件字节（刷新后不依赖原 File/Blob 句柄）。 */
  readonly #restoredAttachmentBytes = new Map<string, Uint8Array>();
  /** 同一准备会话的首条提交（防止双击产生第二次执行）。 */
  readonly #preparationSubmissions = new Set<Uuid>();
  /** 首条的上传清单标识：一次提交尝试内稳定（重试复用）。 */
  readonly #preparationSubmissionIds = new Map<Uuid, Uuid>();
  /** `requestId → 准备会话`：`command.result` 只带请求标识，需要它才能归并到同一准备。 */
  readonly #preparationRequestSessions = new Map<Uuid, Uuid>();
  /** 已受理、等待权威读取验证的会话：读到完整会话后才清浏览器正文/Blob。 */
  readonly #preparationAwaitingRead = new Set<Uuid>();
  /** 归并阶段不能做的副作用（读会话、发控制帧）：提交完成后再执行。 */
  readonly #preparationEffects: (() => void)[] = [];
  /** 正在尝试恢复准备的会话：未知 URL 的跳转必须等它落地（否则合法准备会被误判成不存在）。 */
  readonly #preparationOpening = new Set<Uuid>();
  /**
   * 唯一准备槽位的在途分配。
   *
   * 所有普通新建入口共用一个闸门：已有在途分配时再次新建只会拿到同一个 Promise，
   * 不会发第二帧 `allocate`，也不会按入口各自的目录分配第二个 ID。
   */
  #pendingSlotAllocation: Promise<PreparationBeginOutcome> | null = null;
  /** 页面请求过从本标签页定位恢复槽位：主机身份/认证就绪后自动补一次。 */
  #tabRestoreRequested = false;
  /** 启动时读定位并恢复槽位的在途任务（并发调用共享）。 */
  #tabRestoreInFlight: Promise<Uuid | null> | null = null;
  /** 正在显式更换 Agent 的会话（防重复替换）。 */
  readonly #preparationBusy = new Set<Uuid>();
  /** 草稿正文的合并写定时器（按键路径去重）。 */
  readonly #preparationSaveTimers = new Map<Uuid, () => void>();
  #pairingSession: PairingSession | null = null;
  #state: ClientState = initialClientState();

  constructor(options: ClientStoreOptions) {
    this.#sync = options.sync;
    this.#ids = options.ids;
    this.#now = options.now;
    this.#identity = options.identity;
    this.#ownerLabelOf = options.ownerLabelOf ?? ((ownerNodeId: string) => ownerNodeId);
    this.#pairing = options.pairing ?? null;
    this.#schedule = options.schedule ?? null;
    this.#attachmentMedia = options.attachments?.media ?? null;
    this.#attachmentTransfer = options.attachments?.transfer ?? null;
    this.#preparationDrafts = options.preparationDrafts ?? null;
    this.#preparationTabNonce = options.preparationTabNonce ?? null;
  }

  /** 当前状态。 */
  get state(): ClientState {
    return this.#state;
  }

  /** 自动重连进度（链接器只读；写入走下面三个方法）。 */
  get reconnect(): ReconnectProgress {
    return this.#state.reconnect;
  }

  /**
   * 组合根安排了一次退避重连。
   *
   * 这些事实只用于呈现（「已重试 N 次，M 秒后重试」），不参与连接状态机：连接状态仍然只由
   * `connection` 一个值表达。
   */
  noteReconnectScheduled(attempts: number, nextRetryAtMs: number): void {
    const current = this.#state.reconnect;
    if (current.attempts === attempts && current.nextRetryAtMs === nextRetryAtMs) return;
    this.#commit({ ...this.#state, reconnect: { attempts, nextRetryAtMs } });
  }

  /** 下一次尝试已经开始：清掉「下次重试时刻」，保留已尝试次数。 */
  noteReconnectAttemptStarted(): void {
    const current = this.#state.reconnect;
    if (current.nextRetryAtMs === null) return;
    this.#commit({ ...this.#state, reconnect: { attempts: current.attempts, nextRetryAtMs: null } });
  }

  /** 连接已恢复（或不需要再重连）：进度归零。 */
  clearReconnectProgress(): void {
    const current = this.#state.reconnect;
    if (current.attempts === 0 && current.nextRetryAtMs === null) return;
    this.#commit({ ...this.#state, reconnect: { attempts: 0, nextRetryAtMs: null } });
  }

  /** 订阅状态变化（React 的 `useSyncExternalStore` 与页面共用这一入口）。 */
  subscribe(listener: Listener): () => void {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  }

  /**
   * 摄入一条 `SyncClientEvent`。
   *
   * 连接、快照、事件、命令结果四类都在这里归并；`reset_required` 刻意**不**清空
   * `resources`——§9.4 要求验证失败时保留上一次已完成的状态。
   */
  apply(event: SyncClientEvent): void {
    const readPending = event.kind === "command_result" ? this.#pendingReads.get(event.result.body.requestId) : undefined;
    const wasOnline = this.#state.connection.state === "online";
    // 归并前后用来判断「哪些会话的线程被删掉了」：时间线复用层随会话一起释放。
    const threadsBefore = this.#state.threads;
    // 新连接上的结果查询重新计数：同一条标题命令每次重连后都会再查一次原请求。
    if (event.kind === "authenticated") {
      this.#titleStatusQueries.clear();
      this.#preparationStatusQueries.clear();
      this.#preparationOps.clear();
      // 旧连接上的分配应答不会再来：结束在途闸门，避免永久挡住下一次新建。
      this.#dropPendingSlotAllocations("连接已更新，这次新建没有完成，请重试");
    }
    const reduced = this.#reduce(event);
    // 连接离开 online（closed/blocked）或建立新连接（authenticated）后，旧连接上的在途
    // 文件查询不会再回来：必须丢掉它们并把来源的 loading 复位，否则 in-flight 标记会让
    // 下一次刷新被永久排队、来源永久停在加载中。
    const resetFiles = event.kind === "authenticated" || event.kind === "closed" || event.kind === "blocked";
    if (resetFiles) this.#interactionRefreshQueued.clear();
    if (resetFiles) this.#resetFileQueries();
    // 读取结果只属于发起它的连接：换连接后旧结果不会回来。留着在途表会永久挡住同一会话的
    // 后续读取（页面只剩实时事件、看不到已保存的用户输入与历史）。
    const droppedReads = resetFiles ? this.#resetPendingReads() : [];
    const next = resetFiles ? this.#withFileLoadingStopped(reduced) : reduced;
    const recovered = droppedReads.length === 0 ? next : this.#withReadLoadingStopped(next, droppedReads);
    // 快照、`session.updated`、读取结果等任何归并之后都检查一次：标题命令的「completed」
    // 只有在权威摘要反映这次保存后才能退出编辑。
    const settled = this.#settleTitleCommits(recovered);
    if (settled === this.#state) return;
    if (resetFiles) this.#timelineFloors.clear();
    // 时间线复用层只对**同一份**会话数据有意义：换连接/断开后一切都要按新数据重整，
    // 不复用旧连接上拼出来的条目对象（也避免整份数组被继续持有）。
    if (resetFiles) this.#timelineEntries.clear();
    if (readPending !== undefined && event.kind === "command_result") {
      if (event.result.body.status !== "completed") this.#timelineFloors.delete(readPending.sessionId);
      else if (!this.#timelineFloors.has(readPending.sessionId) &&
        ["first", "rebuild_remote", "earlier", "earlier_history"].includes(readPending.intent)) {
        const thread = settled.threads.get(readPending.sessionId);
        const message = thread?.page?.earliestCursor?.sequence;
        const history = thread?.nextBeforeHistory;
        if (message !== undefined && history != null) {
          this.#timelineFloors.set(readPending.sessionId, BigInt(message) < BigInt(history) ? message : history);
        }
      }
    }
    this.#commit(settled);
    this.#pruneTimelineEntries(threadsBefore);
    if (readPending !== undefined) this.#alignTimelineWindow(readPending.sessionId);
    if (!wasOnline && this.#state.connection.state === "online" && this.#activeConversationSession !== null) {
      this.#activeFileRefreshNeeded = true;
    }
    this.#drainActiveFileRefresh();
    this.#drainQueuedFileRefreshes();
    this.#drainQueuedMessageRefreshes();
    this.#drainQueuedInteractionRefreshes();
    // 归并阶段推迟的副作用（请求正式会话、发准备控制帧）在提交之后执行。
    this.#drainPreparationEffects();
    // 首条受理后的权威读取：读到完整会话才清浏览器正文/Blob（只保留请求关联）。
    this.#drainPreparationsAwaitingRead();
    // 连接代变化或重新追平：可见的当前会话要立即重新声明并激活（§11.8 的页面顺序）。
    // 断开/阻断时先停续租：旧连接上的计时器不得继续发帧。
    if (event.kind === "closed" || event.kind === "blocked") {
      this.#cancelViewRenewal();
      this.#cancelAllPreparationRenewals();
      // 旧连接上的准备操作不会再有应答：清掉在飞标记，重连后按 restore/状态查询重新收口。
      this.#preparationOps.clear();
      this.#preparationStatusQueries.clear();
      this.#dropPendingSlotAllocations("连接已断开，这次新建没有完成，请重试");
    }
    if (event.kind === "online") {
      this.#ensureLifecycle("authenticated");
      this.#recoverPreparations();
    }
    if (this.#tabRestoreRequested && (event.kind === "authenticated" || event.kind === "snapshot_verified")) {
      // 页面已经在等槽位恢复：主机身份/认证到这个时点才可用，补读一次定位。
      void this.restoreTabPreparation();
    }
    if (event.kind === "authenticated") {
      this.#drainDeferredReads();
      // 认证完成即查询上一条连接上未收束的标题命令（原 requestId，不重发）。
      this.#drainTitleStatusQueries();
      // 同一 ID 重新准备：存活阶段 restore，`submitting` 只查原请求（不重发）。
      this.#recoverPreparations();
    }
    if (event.kind === "snapshot_verified") {
      // 快照到达后才能判定来源：补发延迟读取，并为已打开的远程会话建立订阅与重建。
      this.#drainDeferredReads();
      // 深链首次打开时摘要可能晚于路由到达：此刻才判定它是 owned 会话并开始激活。
      this.#ensureLifecycle("enter");
      const active = this.#activeConversationSession;
      if (active !== null) {
        const ref = remoteRefOf(this.#state, active);
        if (ref !== null) {
          this.#sync.subscribeRemote(ref);
          this.#queueRemoteRebuild(active);
        }
      }
    }
    // 来源清单失效只重新查询清单（快照），不携带任何旧资源副本。
    if (event.kind === "remote_catalog_changed") this.#sync.refreshSnapshot();
    this.#drainQueuedRemoteRebuilds();
    // 创建完成只带目录引用，路径必须从服务端快照取得，不能按用户输入伪造目录条目。
    if (event.kind === "command_result" && event.result.body.command === "session.create" &&
      event.result.body.status === "completed" && !this.#directoryRefreshRequests.has(event.result.body.requestId)) {
      const value = toResultRecord(event.result.body.result);
      const summary = value?.["session"] as SessionSummary | undefined;
      const directoryId = summary?.workspace?.directoryId;
      if (directoryId !== undefined && summary?.sessionId === value?.["sessionId"] &&
        !next.resources?.workspaces.some((workspace) => workspace.directoryId === directoryId)) {
        this.#directoryRefreshRequests.add(event.result.body.requestId);
        this.#sync.refreshSnapshot();
      }
    }
    const folder = this.#state.folderRequest;
    if (folder !== null && !folder.refreshed && this.#lastCommandOf(folder.requestId)?.state === "completed") {
      this.#commit({ ...this.#state, folderRequest: { ...folder, refreshed: true } });
      this.browseDirectory(folder.parentPath);
    }
    const current = this.#state.folderRequest;
    const browse = this.#state.directoryBrowse;
    if (current !== null && current.refreshed && !current.entered &&
      this.#lastCommandOf(current.requestId)?.state === "completed" &&
      browse.here === current.parentPath && browse.loaded && browse.pendingRequestId === null && browse.error === null) {
      const child = browse.entries.find((entry) => entry.name === current.name);
      if (child !== undefined) {
        this.#commit({ ...this.#state, folderRequest: { ...current, entered: true } });
        this.browseDirectory(child.path);
      }
    }
  }

  #reduce(event: SyncClientEvent): ClientState {
    switch (event.kind) {
      case "authenticated": {
        const selectedFeatures = [...(event.selectedFeatures ?? [])];
        // 新认证连接 = 新连接代：旧连接上的查看声明与租约一律作废（服务端也会清掉它们）。
        // 可见性保留（页面事实），选择保留（返回前台/重连后要立即重新声明并激活）。
        const lifecycle = this.#state.lifecycle;
        return {
          ...this.#state,
          hasAuthenticated: true,
          deviceId: event.info.deviceId,
          selectedFeatures,
          lifecycle: {
            ...lifecycle,
            negotiated: selectedFeatures.includes(LOCAL_SESSION_LIFECYCLE_FEATURE),
            generation: lifecycle.generation + 1,
            view: null,
            leaseExpiresAt: null,
            // 旧代上的在途激活不可能再收到终态（结果按连接核对）：终结它们，让重新进入/返回
            // 前台能按新连接重新申请，而不是永远停在「正在恢复」。
            activations: withoutActivating(lifecycle.activations),
          },
        };
      }
      case "authenticating": {
        // 一条事件对应连接机的两步：开始一次连接尝试（connecting），随后 socket 打开进入认证（authenticating）。
        const started = applyConnectionEvent(this.#state.connection, {
          kind: "connect_started",
          fromReconnect: this.#state.connection.state === "reconnecting",
        });
        return { ...this.#state, connection: applyConnectionEvent(started, { kind: "socket_opened" }) };
      }
      case "replaying":
        return this.#withConnection({ kind: "subscribed" });
      case "online":
        return this.#withConnection({ kind: "caught_up" });
      case "closed":
        return this.#withTitleConnectionLost(clearAllRemoteData({
          ...this.#withConnection({ kind: "connection_closed", retryable: event.retryable }),
          lifecycle: this.#withoutViewer(this.#state.lifecycle),
          lastClose: {
            code: event.code,
            reason: event.detail ?? null,
            atMs: this.#now(),
          },
        }));
      case "blocked":
        return this.#withTitleConnectionLost(clearAllRemoteData({
          ...this.#withConnection({ kind: "blocked", cause: event.cause }),
          lifecycle: this.#withoutViewer(this.#state.lifecycle),
          lastClose: {
            code: event.code,
            reason: event.reason ?? event.cause,
            atMs: this.#now(),
          },
        }));
      case "snapshot_verified":
        return { ...this.#state, resources: event.snapshot.resources };
      case "event":
        return this.#withEvent(event.event);
      case "command_result":
        return this.#withCommandResult(event.result);
      case "session_view_result":
        return this.#withViewResult(event.result);
      case "session_preparation_result":
        // 准备应答按 `operationId` 归并：迟到/旧代的应答不改变当前状态。
        return this.#withPreparationResult(this.#state, event.result);
      case "remote_subscribing":
        return this.#withRemoteSubscribing(event);
      case "remote_event":
        return this.#withRemoteEvent(event);
      case "remote_caught_up":
        return this.#withRemoteCaughtUp(event);
      case "remote_reset_required":
        return this.#withRemoteReset(event);
      case "remote_unavailable":
        return this.#withRemoteUnavailable(event);
      case "remote_unsubscribed":
        return this.#withRemoteUnsubscribed(event);
      case "remote_source_state":
        return this.#withRemoteSourceState(event.ownerNodeId, event.state);
      default:
        return this.#state;
    }
  }

  #withConnection(event: ConnectionEvent): ClientState {
    return { ...this.#state, connection: applyConnectionEvent(this.#state.connection, event) };
  }

  /**
   * 连接离开（断开/阻断）：作废查看声明与租约，并终结不可能再收口的在途激活。
   *
   * 结果按连接核对，因此旧连接上的 `session.resume` 终态永远不会被接受；保留 `activating`
   * 会让页面永远停在「正在恢复」。终结后重新连上时按选择重新申请即可。
   */
  #withoutViewer(lifecycle: SessionLifecycleState): SessionLifecycleState {
    return {
      ...lifecycle,
      view: null,
      leaseExpiresAt: null,
      activations: withoutActivating(lifecycle.activations),
    };
  }

  /**
   * `session.view.result` 的归并。
   *
   * 三重核对缺一不可：**同一连接**（旧连接的迟到应答不得复活 viewer）、**同一版本**（低于当前
   * 版本的应答是已离开/已替换的声明）、**同一选择**（离页应答 `sessionId: null` 不是「正在查看
   * 空会话」）。`stale` 不改任何查看状态：下一次续租会带更高版本重新声明。
   */
  #withViewResult(result: SyncSessionViewResult): ClientState {
    const lifecycle = this.#state.lifecycle;
    const view = lifecycle.view;
    const connectionId = this.#identity.connectionId;
    if (connectionId === null || result.connectionId !== connectionId) return this.#state;
    if (view === null) return this.#state;
    if (result.body.viewVersion !== view.viewVersion) return this.#state;
    if (result.body.sessionId !== view.sessionId) return this.#state;
    if (result.body.status !== "accepted") return this.#state;
    if (result.body.sessionId === null) return this.#state;
    if (result.body.leaseExpiresAt === null) return this.#state;
    if (lifecycle.leaseExpiresAt === result.body.leaseExpiresAt) return this.#state;
    return {
      ...this.#state,
      lifecycle: { ...lifecycle, leaseExpiresAt: result.body.leaseExpiresAt },
    };
  }

  /**
   * 平台 adapter 转来的页面生命周期（可见性 / `pagehide`）。
   *
   * 隐藏与「即将卸载」都**立即**停止续租并声明离开；它们只改变连接期的查看状态，不清除历史、
   * 草稿或选择——返回可见时按同一个选择重新声明并激活。重复的同值事件是幂等的。
   */
  applyLifecycle(event: SessionPageEvent): void {
    const current = this.#state.lifecycle;
    if (event.kind === "visibility" && current.visibility === event.state) return;
    const hidden = event.kind === "pagehide" || event.state === "hidden";
    if (hidden) {
      this.#cancelViewRenewal();
      // 后台不保活临时 Agent：隐藏即停止续租（返回可见时再恢复）。
      this.#cancelAllPreparationRenewals();
      // 声明必须在状态提交后发出（且只在已认证连接上）；`#declareView` 自己处理这些前提。
      const lifecycle = { ...current, visibility: "hidden" as const };
      if (lifecycle !== current) this.#commit({ ...this.#state, lifecycle });
      this.#declareView(null, true);
      return;
    }
    this.#commit({ ...this.#state, lifecycle: { ...current, visibility: "visible" } });
    this.#ensureLifecycle("visible");
    this.#recoverPreparations();
  }

  /** 触发一次尝试的原因（决定是否重新派发 `session.resume`）。 */
  #ensureLifecycle(reason: "enter" | "visible" | "authenticated" | "retry"): void {
    if (this.#syncingLifecycle) return;
    const state = this.#state;
    const lifecycle = state.lifecycle;
    if (
      !lifecycle.negotiated ||
      state.connection.state !== "online" ||
      this.#identity.connectionId === null ||
      lifecycle.visibility !== "visible"
    ) {
      this.#cancelViewRenewal();
      return;
    }
    const sessionId = lifecycle.selectedSessionId;
    if (sessionId === null) {
      if (lifecycle.view !== null && lifecycle.view.sessionId !== null) this.#declareView(null, true);
      this.#cancelViewRenewal();
      return;
    }
    const summary = this.#findSession(sessionId);
    // imported 会话不在本机生命周期里：不发查看帧、不派发 resume，展示保持任务三态。
    if (!isOwnedSummary(summary)) {
      this.#cancelViewRenewal();
      return;
    }
    this.#syncingLifecycle = true;
    try {
      // 只有同一连接、同一选择上的声明才可复用；进入/返回/重连/重试都必须换新版本重新声明。
      const current = lifecycle.view;
      const declarationCurrent =
        current !== null &&
        current.sessionId === sessionId &&
        current.connectionGeneration === lifecycle.generation;
      this.#declareView(sessionId, reason !== "enter" || !declarationCurrent);
      // 返回可见/重连/重试必须重新申请激活（服务端对已 ready 的资源复用）；进入会话时若已有
      // 在途激活则不重复派发。
      this.#requestActivation(sessionId, reason !== "enter");
      this.#scheduleViewRenewal();
    } finally {
      this.#syncingLifecycle = false;
    }
  }

  /**
   * 发出一次查看声明。
   *
   * 版本必须**单调递增**（同版本不同选择会被服务端当 `stale`），且只有已认证在线的连接上才发。
   * 声明先提交、后发帧：即使 socket 同步失败，状态里也保留「已尝试的版本」，下一次续租只会更高。
   */
  #declareView(sessionId: Uuid | null, force: boolean): void {
    const state = this.#state;
    const lifecycle = state.lifecycle;
    if (!lifecycle.negotiated) return;
    if (state.connection.state !== "online" || this.#identity.connectionId === null) return;
    const current = lifecycle.view;
    if (
      !force &&
      current !== null &&
      current.sessionId === sessionId &&
      current.connectionGeneration === lifecycle.generation
    ) {
      return;
    }
    const previousVersion = current?.viewVersion ?? 0;
    if (!canAdvanceViewVersion(previousVersion)) return;
    const viewVersion = nextViewVersion(previousVersion);
    const declaration: SessionViewDeclaration = {
      sessionId,
      viewVersion,
      connectionGeneration: lifecycle.generation,
      leaseExpiresAt: null,
    };
    this.#commit({
      ...state,
      lifecycle: {
        ...lifecycle,
        view: declaration,
        // 离页声明不保留任何租约：应答里的 `leaseExpiresAt` 只表示清理残余查看状态的时限。
        leaseExpiresAt: sessionId === null ? null : lifecycle.leaseExpiresAt,
      },
    });
    this.#sync.setSessionView({ sessionId, viewVersion });
  }

  /**
   * 派发一次 `session.resume`（Sync 面 payload 固定 `{}`）。
   *
   * 同一连接代上的在途尝试不重复派发：同一会话的并发激活由服务端合并，客户端不该用第二条请求
   * 覆盖一个尚未收口的观察者。旧代的在途尝试已在连接断开/换代时终结，即便残留也不能当作
   * 「仍在等待」（它永远等不到终态）。
   *
   * `uncertain` 的旧请求**不会**被自动重派——重新进入、返回前台、重新认证都只返回原
   * `requestId`，不换号；重试走 {@link retrySessionActivation} 的 `command.status` 核实路径。
   */
  #requestActivation(sessionId: Uuid, force: boolean): Uuid | null {
    const lifecycle = this.#state.lifecycle;
    const existing = lifecycle.activations.get(sessionId) ?? null;
    if (
      !force &&
      existing !== null &&
      existing.phase === "activating" &&
      existing.requestId !== null &&
      existing.connectionGeneration === lifecycle.generation
    ) {
      return existing.requestId;
    }
    // 结果无法证明：结果未知，绝不自动换 `requestId` 重派；只有显式重试才处理。
    if (existing?.failure?.kind === "uncertain" && existing.requestId !== null) return existing.requestId;
    const requestId = this.#dispatchIntent({
      sessionId,
      build: (id): CommandBody => ({
        requestId: id,
        command: "session.resume",
        sessionId,
        payload: {},
      }),
      patch: (id): Partial<ClientState> => ({
        lifecycle: {
          ...this.#state.lifecycle,
          activations: withSessionActivation(this.#state.lifecycle.activations, {
            sessionId,
            phase: "activating",
            requestId: id,
            connectionGeneration: this.#state.lifecycle.generation,
            failure: null,
          }),
        },
      }),
    });
    return requestId;
  }

  /** 取消续租计时器（离页、隐藏、断开、换选择都走这里）。 */
  #cancelViewRenewal(): void {
    if (this.#viewRenewCancel === null) return;
    this.#viewRenewCancel();
    this.#viewRenewCancel = null;
  }

  /** 可见且选中本机会话时才排续租：每条连接最多一个计时器，重复调用不会叠加。 */
  #scheduleViewRenewal(): void {
    if (this.#viewRenewCancel !== null) return;
    if (this.#schedule === null) return;
    this.#viewRenewCancel = this.#schedule(SESSION_VIEW_RENEW_INTERVAL_MS, () => {
      this.#viewRenewCancel = null;
      this.#renewView();
    });
  }

  /** 续租 = 同一选择上递增版本重新声明；不派发 resume、不改变激活状态。 */
  #renewView(): void {
    const state = this.#state;
    const lifecycle = state.lifecycle;
    if (
      !lifecycle.negotiated ||
      state.connection.state !== "online" ||
      lifecycle.visibility !== "visible" ||
      this.#identity.connectionId === null
    ) {
      return;
    }
    const selected = lifecycle.selectedSessionId;
    if (selected === null) return;
    const view = lifecycle.view;
    if (view === null || view.sessionId !== selected || view.connectionGeneration !== lifecycle.generation) {
      return;
    }
    if (!isOwnedSummary(this.#findSession(selected))) return;
    this.#declareView(selected, true);
    this.#scheduleViewRenewal();
  }

  /**
   * 显式重试激活（页面按钮）。
   *
   * `uncertain` 的旧请求先走 `command.status` 核实，**不**换新 `requestId` 自动重派；
   * 其余失败直接用新请求重新申请（服务端对已 ready 的资源会复用，不重建会话）。
   */
  retrySessionActivation(sessionId: Uuid): CommandOutcome {
    const lifecycle = this.#state.lifecycle;
    if (!lifecycle.negotiated) return { ok: false, reason: ACTIVATION_UNAVAILABLE_NOTICE };
    const summary = this.#findSession(sessionId);
    if (!isOwnedSummary(summary)) return { ok: false, reason: "只有本机会话需要激活；远程导入会话由来源节点管理" };
    if (this.#state.connection.state !== "online" || this.#identity.connectionId === null) return NOT_CONNECTED;
    const existing = lifecycle.activations.get(sessionId) ?? null;
    if (existing?.failure?.kind === "uncertain" && existing.requestId !== null) {
      const target = existing.requestId;
      const requestId = this.#dispatchIntent({
        sessionId,
        build: (id): CommandBody => ({
          requestId: id,
          command: "command.status",
          payload: { targetRequestId: target },
        }),
      });
      if (requestId === null) return NOT_CONNECTED;
      return { ok: true, requestId };
    }
    this.#ensureLifecycle("retry");
    const current = this.#state.lifecycle.activations.get(sessionId);
    return current?.requestId === null || current?.requestId === undefined
      ? { ok: false, reason: "当前无法派发恢复请求" }
      : { ok: true, requestId: current.requestId };
  }

  /**
   * `command.result`（`session.resume`）的归并。
   *
   * 旧连接上的终态不得改当前状态：先把 `connectionId` 与当前连接核对，再按 `requestId` 找到
   * 那一次尝试。完成结果只接受「同一会话标识」的摘要——迟到的 A 结果不会把页面切回 A，也不会
   * 为 B 续租。
   */
  #withActivationResult(state: ClientState, result: CommandResultMessage): ClientState {
    if (result.body.command !== "session.resume") return state;
    const connectionId = this.#identity.connectionId;
    if (connectionId === null || result.connectionId !== connectionId) return state;
    const lifecycle = state.lifecycle;
    const entry = findActivationByRequest(lifecycle.activations, result.body.requestId);
    if (entry === null) return state;
    if (result.body.status === "accepted") {
      if (entry.phase === "activating") return state;
      return {
        ...state,
        lifecycle: {
          ...lifecycle,
          activations: withSessionActivation(lifecycle.activations, {
            ...entry,
            phase: "activating",
            // 受理是在这条连接上观察到的：后续终态也按这条连接核对。
            connectionGeneration: lifecycle.generation,
            failure: null,
          }),
        },
      };
    }
    if (result.body.status === "completed") {
      const summary = resumedSummaryOf(result.body.result, entry.sessionId);
      if (summary === null) {
        // 形状不符不得被当成成功：保留激活尝试并给出明确失败（历史仍可读）。
        return {
          ...state,
          lifecycle: {
            ...lifecycle,
            activations: withSessionActivation(lifecycle.activations, {
              ...entry,
              phase: "failed",
              failure: activationShapeFailure(),
            }),
          },
        };
      }
      return {
        ...state,
        resources: withSessionSummary(state.resources, summary),
        lifecycle: {
          ...lifecycle,
          activations: withSessionActivation(lifecycle.activations, {
            ...entry,
            phase: "ready",
            failure: null,
          }),
        },
      };
    }
    const failure = classifyActivationFailure({ status: result.body.status, error: result.body.error });
    if (failure === null) return state;
    return {
      ...state,
      lifecycle: {
        ...lifecycle,
        activations: withSessionActivation(lifecycle.activations, {
          ...entry,
          phase: "failed",
          failure,
        }),
      },
    };
  }

  /** `command.status` 核实旧 `session.resume` 的结果（`uncertain` 的显式重试路径）。 */
  #withActivationStatus(state: ClientState, record: CommandStatusRecord): ClientState {
    const lifecycle = state.lifecycle;
    const entry = findActivationByRequest(lifecycle.activations, record.targetRequestId);
    if (entry === null) return state;
    if (record.state === "completed") {
      // 与 `#withActivationResult` 同一判据：形状不符不得打开发送门，按明确失败处理。
      const summary = resumedSummaryOf(record.result, entry.sessionId);
      if (summary === null) {
        return {
          ...state,
          lifecycle: {
            ...lifecycle,
            activations: withSessionActivation(lifecycle.activations, {
              ...entry,
              phase: "failed",
              failure: activationShapeFailure(),
            }),
          },
        };
      }
      return {
        ...state,
        resources: withSessionSummary(state.resources, summary),
        lifecycle: {
          ...lifecycle,
          activations: withSessionActivation(lifecycle.activations, {
            ...entry,
            phase: "ready",
            failure: null,
          }),
        },
      };
    }
    if (record.state === "accepted") {
      return {
        ...state,
        lifecycle: {
          ...lifecycle,
          activations: withSessionActivation(lifecycle.activations, {
            ...entry,
            phase: "activating",
            // 核实是在这条连接上做的：原请求的终态也由这条连接投递。
            connectionGeneration: lifecycle.generation,
            failure: null,
          }),
        },
      };
    }
    const failure = classifyActivationFailure({ status: record.state, error: record.error });
    if (failure === null) return state;
    return {
      ...state,
      lifecycle: {
        ...lifecycle,
        activations: withSessionActivation(lifecycle.activations, {
          ...entry,
          phase: "failed",
          failure,
        }),
      },
    };
  }

  /**
   * 本机活跃程度事件/摘要到达：`active`/`pending` 是节点确认资源可用的权威事实。
   *
   * 它只用于把「仍在恢复中」收敛成 `ready`（例如 `resume` 终态丢失但活动事件到了）；`idle`
   * **不**用来撤销一次成功激活——资源释放后发送仍由下一次激活/服务端拒绝决定，不能靠事件
   * 循环触发自动恢复。
   */
  #withActivityActivation(state: ClientState, summary: SessionSummary): ClientState {
    const lifecycle = state.lifecycle;
    if (!lifecycle.negotiated) return state;
    if (lifecycle.selectedSessionId !== summary.sessionId) return state;
    if (!isOwnedSummary(summary)) return state;
    const activity = localActivityOf(summary, true);
    if (activity !== "active" && activity !== "pending") return state;
    const entry = lifecycle.activations.get(summary.sessionId) ?? null;
    if (entry !== null && entry.phase === "ready") return state;
    return {
      ...state,
      lifecycle: {
        ...lifecycle,
        activations: withSessionActivation(lifecycle.activations, {
          sessionId: summary.sessionId,
          phase: "ready",
          requestId: entry?.requestId ?? null,
          connectionGeneration: lifecycle.generation,
          failure: null,
        }),
      },
    };
  }

  /** 来源订阅已（重新）发出：登记运行期模型并排队一次原子读取重建。 */
  #withRemoteSubscribing(event: Extract<SyncClientEvent, { kind: "remote_subscribing" }>): ClientState {
    const { ownerNodeId, sessionId } = event.sessionRef;
    const previous = this.#state.remoteSessions.get(sessionId);
    const existing = previous?.ownerNodeId === ownerNodeId ? previous : undefined;
    const sourceState = existing?.sourceState ?? this.#state.remoteSources.get(ownerNodeId)?.state ?? "online";
    const runtime: RemoteSessionRuntime = {
      ownerNodeId,
      sessionId,
      phase: "subscribing",
      sourceState,
      cursor: event.cursor ?? existing?.cursor ?? null,
      caughtUp: false,
      rebuilt: false,
      coverage: existing?.coverage ?? EMPTY_REMOTE_COVERAGE,
      error: null,
      generation: sourceGenerationOf(this.#state, ownerNodeId),
    };
    const remoteSessions = new Map(this.#state.remoteSessions);
    remoteSessions.set(sessionId, runtime);
    const remoteSources = new Map(this.#state.remoteSources);
    if (!remoteSources.has(ownerNodeId)) {
      remoteSources.set(ownerNodeId, { state: "online", generation: runtime.generation });
    }
    this.#remoteRebuilds.add(sessionId);
    return { ...this.#state, remoteSessions, remoteSources };
  }

  /**
   * 一条来源事件：先按覆盖位置过滤已被读取页包含的增量，再走与 live 事件同一套记录/易失归并。
   *
   * 未返回消息的覆盖项不存在，因此事件照常保留；同一 `originEventId` 的去重已在门面完成，
   * 这里再用记录键兜底（重订阅窗口）。
   */
  #withRemoteEvent(event: Extract<SyncClientEvent, { kind: "remote_event" }>): ClientState {
    const { ownerNodeId, sessionId } = event.sessionRef;
    let runtime = this.#state.remoteSessions.get(sessionId);
    if (runtime === undefined || runtime.ownerNodeId !== ownerNodeId) {
      runtime = {
        ownerNodeId,
        sessionId,
        phase: "subscribing",
        sourceState: this.#state.remoteSources.get(ownerNodeId)?.state ?? "online",
        cursor: null,
        caughtUp: false,
        rebuilt: false,
        coverage: EMPTY_REMOTE_COVERAGE,
        error: null,
        generation: sourceGenerationOf(this.#state, ownerNodeId),
      };
    }
    const remoteEvent = event.event;
    const view = remoteEvent.payload.view ?? {};
    if (remoteEvent.eventType === "session.origin.online_changed" && view["online"] === false) {
      return clearRemoteOwnerData(
        this.#state,
        ownerNodeId,
        "offline",
        remoteUnavailableError(ownerNodeId, "来源节点当前不可达，远程会话已清除。"),
      );
    }
    const nextCursor = advanceOriginCursor(runtime.cursor, {
      originEpoch: remoteEvent.originEpoch,
      originSequence: remoteEvent.originSequence,
    });
    const deltaChannel = deltaChannelOf(remoteEvent.eventType);
    if (deltaChannel !== null) {
      const messageId = viewText(view, "messageId");
      const deltaIndex = viewText(view, "deltaIndex");
      if (messageId !== null && deltaIndex !== null && isDeltaCovered(runtime.coverage, messageId, deltaIndex)) {
        // 已被读取页覆盖：不再拼接，也不产生第二条记录；位置仍前进（这条事件确实处理过）。
        const remoteSessions = new Map(this.#state.remoteSessions);
        remoteSessions.set(sessionId, { ...runtime, cursor: nextCursor });
        return { ...this.#state, remoteSessions };
      }
    }

    let next: ClientState = { ...this.#state, remoteSessions: new Map(this.#state.remoteSessions) };
    const record = recordFromRemoteEvent(remoteEvent);
    const key = recordKey(record);
    if (!next.structured.has(key)) {
      const structured = new Map(next.structured);
      structured.set(key, record);
      next = { ...next, structured };
    }
    next = this.#withMessageLane(next, record);

    if (remoteEvent.eventType === "session.created" || remoteEvent.eventType === "session.updated") {
      const summary = view["session"];
      if (isRemoteSummary(summary, sessionId, ownerNodeId)) {
        next = { ...next, resources: withSessionSummary(next.resources, summary) };
      }
    }
    if (remoteEvent.eventType === "session.usage.changed") {
      const used = view["used"];
      const size = view["size"];
      if (typeof used === "string" && typeof size === "string") {
        const usage = new Map(next.usage);
        usage.set(sessionId, { used, size });
        next = { ...next, usage };
      }
    }
    const runtimeWithCursor: RemoteSessionRuntime = {
      ...(next.remoteSessions.get(sessionId) ?? runtime),
      cursor: nextCursor,
    };
    const remoteSessions = new Map(next.remoteSessions);
    remoteSessions.set(sessionId, runtimeWithCursor);
    return { ...next, remoteSessions };
  }

  #withRemoteCaughtUp(event: Extract<SyncClientEvent, { kind: "remote_caught_up" }>): ClientState {
    const runtime = this.#state.remoteSessions.get(event.sessionRef.sessionId);
    if (runtime === undefined) return this.#state;
    const remoteSessions = new Map(this.#state.remoteSessions);
    const caughtUp: RemoteSessionRuntime = {
      ...runtime,
      caughtUp: true,
      cursor: advanceOriginCursor(runtime.cursor, event.cursor),
      phase: runtime.rebuilt ? "ready" : "replaying",
      error: null,
    };
    remoteSessions.set(runtime.sessionId, caughtUp);
    if (!runtime.rebuilt) this.#remoteRebuilds.add(runtime.sessionId);
    return { ...this.#state, remoteSessions };
  }

  #withRemoteReset(event: Extract<SyncClientEvent, { kind: "remote_reset_required" }>): ClientState {
    const runtime = this.#state.remoteSessions.get(event.sessionRef.sessionId);
    if (runtime === undefined) return this.#state;
    // C 已不可恢复：先清展示数据与旧覆盖，再回源读取重建（D4 的首次重建路径）。
    this.#remoteRebuilds.add(runtime.sessionId);
    return clearRemoteSessionData(this.#state, runtime, "subscribing", null);
  }

  #withRemoteUnavailable(event: Extract<SyncClientEvent, { kind: "remote_unavailable" }>): ClientState {
    return clearRemoteOwnerData(this.#state, event.sessionRef.ownerNodeId, "unknown", event.error);
  }

  #withRemoteUnsubscribed(event: Extract<SyncClientEvent, { kind: "remote_unsubscribed" }>): ClientState {
    const runtime = this.#state.remoteSessions.get(event.sessionRef.sessionId);
    if (runtime === undefined) return this.#state;
    return clearRemoteSessionData(
      this.#state,
      runtime,
      "unavailable",
      remoteUnavailableError(runtime.ownerNodeId, "该来源订阅已结束，请重新打开会话。"),
    );
  }

  #withRemoteSourceState(ownerNodeId: Uuid, state: RemoteSourceKind): ClientState {
    if (state === "online") {
      const remoteSources = new Map(this.#state.remoteSources);
      const generation = sourceGenerationOf(this.#state, ownerNodeId);
      remoteSources.set(ownerNodeId, { state, generation });
      const remoteSessions = new Map(this.#state.remoteSessions);
      for (const [sessionId, runtime] of remoteSessions) {
        if (runtime.ownerNodeId !== ownerNodeId) continue;
        const revived: RemoteSessionRuntime = {
          ...runtime,
          sourceState: state,
          phase: "subscribing",
          caughtUp: false,
          rebuilt: false,
          coverage: EMPTY_REMOTE_COVERAGE,
          error: null,
        };
        remoteSessions.set(sessionId, revived);
        this.#remoteRebuilds.add(sessionId);
      }
      // 来源恢复后重新读取清单，才能把此前清除的摘要重新取回（不保留旧副本）。
      this.#sync.refreshSnapshot();
      return { ...this.#state, remoteSources, remoteSessions };
    }
    return clearRemoteOwnerData(this.#state, ownerNodeId, state, remoteUnavailableError(ownerNodeId));
  }

  /** 排队一次远程原子读取；真正派发在提交之后（避免归并函数内嵌套提交）。 */
  #queueRemoteRebuild(sessionId: Uuid): void {
    this.#remoteRebuilds.add(sessionId);
  }

  /** 提交之后补发排队的远程重建；同一会话已有读取在途时保留队列。 */
  #drainQueuedRemoteRebuilds(): void {
    if (this.#remoteRebuilds.size === 0) return;
    // 认证前没有 connectionId / deviceId：保留队列，待认证结果到达后再判定 feature 并派发。
    if (this.#identity.connectionId === null || this.#state.deviceId === null) return;
    for (const sessionId of [...this.#remoteRebuilds]) {
      const runtime = this.#state.remoteSessions.get(sessionId);
      if (runtime === undefined || runtime.sourceState === "offline" || runtime.sourceState === "revoked") {
        this.#remoteRebuilds.delete(sessionId);
        continue;
      }
      if (runtime.rebuilt && runtime.caughtUp) {
        this.#remoteRebuilds.delete(sessionId);
        continue;
      }
      const inFlight = [...this.#pendingReads.values()].some((pending) => pending.sessionId === sessionId);
      if (inFlight) continue;
      if (!this.#state.selectedFeatures.includes(STATELESS_ORIGIN_FEATURE)) {
        this.#remoteRebuilds.delete(sessionId);
        this.#sync.unsubscribeRemote({ ownerNodeId: runtime.ownerNodeId, sessionId });
        const remoteSessions = new Map(this.#state.remoteSessions);
        remoteSessions.set(sessionId, {
          ...runtime,
          phase: "unavailable",
          error: {
            code: "capability.unsupported_by_broker",
            message: "此主机未协商 resource.stateless-origin.v1，远程会话不可用。",
            retryable: false,
            details: {},
          },
        });
        this.#commit({ ...this.#state, remoteSessions });
        continue;
      }
      this.#remoteRebuilds.delete(sessionId);
      this.#requestPage(sessionId, "rebuild_remote");
    }
  }

  /**
   * 一条事件的状态归并。
   *
   * 顺序即优先级：先归一成结构化记录（永不丢事件——降级卡片与诊断都从它派生），再按事件类型
   * 更新它自己的通道（摘要/用量/覆盖层/易失叠加层）。**不存在**「既不呈现也不降级」的分支。
   */
  #withEvent(event: EventMessage): ClientState {
    let next = this.#state;

    // 1) 归一记录：实时事件按 `eventId` 先到先留（重投不产生第二条）。
    const record = recordFromEvent(event);
    const key = recordKey(record);
    if (!next.structured.has(key)) {
      const structured = new Map(next.structured);
      structured.set(key, record);
      next = { ...next, structured };
    }

    // 独立 mkdir 的终态来自持久化全局事件，accepted 不是完成。
    const folder = next.folderRequest;
    const view = event.body.payload.view;
    const folderRecord = folder === null ? undefined : next.commands.get(folder.requestId);
    if (folderRecord?.command === "workspace.mkdir" && folderRecord.sessionId === null &&
      next.deviceId !== null && event.body.origin.kind === "device" && event.body.origin.deviceId === next.deviceId &&
      event.body.sessionId === null && event.body.remoteOrigin === null &&
      (view["command"] === undefined || view["command"] === "workspace.mkdir") &&
      view["requestId"] === folder?.requestId &&
      ["command.completed", "command.failed", "command.uncertain"].includes(event.body.eventType)) {
      const status = event.body.eventType === "command.completed" ? "completed"
        : event.body.eventType === "command.failed" ? "failed" : "uncertain";
      const commands = new Map(next.commands);
      commands.set(folderRecord.requestId, applyCommandResult(folderRecord, {
        requestId: folderRecord.requestId, status, acceptedAt: folderRecord.acceptedAt,
        terminalEventId: event.body.eventId, terminalAt: event.body.createdAt,
        result: toResultRecord(view["result"]),
        error: status === "failed" ? view["error"] as PublicError : null,
      }));
      next = { ...next, commands,
        uncertainRequestIds: status === "uncertain" && !next.uncertainRequestIds.includes(folderRecord.requestId)
          ? [...next.uncertainRequestIds, folderRecord.requestId] : next.uncertainRequestIds };
    }

    // 2) 会话摘要（`session.created`/`session.updated`）。
    const summary = sessionSummaryIn(event);
    if (summary !== null) {
      next = { ...next, resources: withSessionSummary(next.resources, summary) };
      // `active`/`pending` 是节点确认资源可用的权威事实：把可能已经丢失终态的「恢复中」
      // 收敛为 ready；`idle` 不反向撤销（避免事件循环触发自动恢复）。
      next = this.#withActivityActivation(next, summary);
    }

    // 3) 来源可达性：只改 `origin.online` 这一条标注（摘要其余字段逐字段保持）。
    const online = originOnlineIn(event);
    if (online !== null && event.body.sessionId !== null) {
      next = { ...next, resources: withOriginOnline(next.resources, event.body.sessionId, online) };
    }

    // 4) 用量（R17 上下文条）。
    const usage = usageIn(event);
    const sessionId = event.body.sessionId;
    if (usage !== null && sessionId !== null) {
      const usageMap = new Map(next.usage);
      usageMap.set(sessionId, usage);
      next = { ...next, usage: usageMap };
    }

    // 5) R19：连接事件只进**覆盖层**；列表本体始终来自快照。
    if (this.#overlayStore.ingest(event) !== null) {
      next = { ...next, overlay: this.#overlayStore.entries() };
    }

    // 6) 消息通道：流式增量累加成**易失**正文，完成标记只改「增量流结束」，压缩事件作废轮次。
    next = this.#withMessageLane(next, record);

    // 7) 文件栏：当前会话的 `file.changed` 只触发正文「过时」标记，不取消 Agent，
    //    也不擅自用新内容覆盖正在阅读的视图（design D6）。
    if (record.eventType === "file.changed" && record.sessionId !== null) {
      next = this.#withFileChangedEvent(next, record);
    }

    return next;
  }

  /**
   * 消息通道的归并（D6）。
   *
   * 只有三类增量事件会累加；`agent.message.completed` **不**贡献正文（正文权威是
   * `session.read`，D3 也因此把消息类事件排除在历史条目之外），它只把该条消息标为
   * 「增量流结束」。`turn.delta_compacted` 作废该轮次的累加内容，并让迟到的增量不再累加。
   */
  #withMessageLane(state: ClientState, record: StructuredRecord): ClientState {
    const channel = deltaChannelOf(record.eventType);
    const sessionId = record.sessionId;
    // 实时交互可能没有请求版本；提交后自动回源取得当前有效选项，不能要求用户猜测刷新。
    if (sessionId !== null && (record.eventType === "permission.requested" || record.eventType === "elicitation.requested") &&
      state.selectedFeatures.includes(PENDING_INTERACTIONS_FEATURE) && viewText(record.view, "requestVersion") === null) {
      this.#interactionRefreshQueued.add(sessionId);
    }
    // ACP 思考通常没有独立结束通知；同轮开始工具、交互或下一条 Agent 消息即结束前一段。
    const nextMessageId = viewText(record.view, "messageId");
    const phaseTurnId = viewText(record.view, "turnId");
    const advancesPhase = channel === "agent" || channel === "thought" ||
      record.eventType === "tool.call.started" || record.eventType === "permission.requested" ||
      record.eventType === "elicitation.requested";
    if (advancesPhase && sessionId !== null && phaseTurnId !== null) {
      const volatile = new Map(state.volatile);
      let changed = false;
      for (const [messageId, tail] of volatile) {
        if (tail.channel !== "thought" || tail.completed || tail.sessionId !== sessionId ||
            tail.turnId !== phaseTurnId || messageId === nextMessageId ||
            (record.sessionSequence !== null && tail.sequence !== undefined &&
             BigInt(record.sessionSequence) < BigInt(tail.sequence))) continue;
        volatile.set(messageId, { ...tail, completed: true });
        changed = true;
      }
      if (changed) state = { ...state, volatile };
    }
    if (channel !== null) {
      const messageId = viewText(record.view, "messageId");
      const deltaIndex = viewText(record.view, "deltaIndex");
      // 纯媒体增量：schema 的 `text` 无 `minLength`，空串是合法取值，媒体块正是「仅非文本内容」
      // 的载体。先解析块再判空，否则图片/音频/文件增量会被整体丢掉（实时里永远不出现）。
      const rawBlock = record.view["block"];
      const blockMedia = rawBlock === undefined ? [] : mediaBlocksOfDelta(rawBlock);
      const text = viewText(record.view, "text");
      if (messageId === null || deltaIndex === null || (text === null && blockMedia.length === 0)) return state;
      const turnId = viewText(record.view, "turnId");
      // 被压缩过的轮次：迟到的增量不再累加（作废的内容 MUST NOT 重新出现）。
      if (turnId !== null && sessionId !== null && state.discardedTurns.has(`${sessionId}|${turnId}`)) {
        return state;
      }
      const volatile = new Map(state.volatile);
      volatile.set(
        messageId,
        appendDelta(volatile.get(messageId), {
          messageId,
          sessionId,
          channel,
          turnId,
          deltaIndex,
          text: text ?? "",
          createdAt: record.createdAt,
          ...(record.sessionSequence === null ? {} : { sequence: record.sessionSequence }),
          mediaBlocks: blockMedia,
        }),
      );
      // 用户增量是本次提交的正式投影：同一次更新里移除本地回显并登记 `messageId → requestId`
      // 锚点（不按文字推断），正式消息因此在时间线上原位接替同一条目。
      const taken = channel === "user" && sessionId !== null
        ? withPromptTakeover(state, sessionId, [{
          messageId,
          turnId,
          causationRequestId: record.causationRequestId ?? null,
        }])
        : state;
      return { ...taken, volatile };
    }

    if (record.eventType === "agent.message.completed") {
      const messageId = viewText(record.view, "messageId");
      if (messageId === null) return state;
      const existing = state.volatile.get(messageId);
      if (existing === undefined) return state;
      const announcedChannel = messageChannelOf(record.view["channel"]);
      // 协议通道是 `reply|thought`，时间线通道是 `user|agent|thought`：`reply` 属于 Agent 消息。
      const channel =
        announcedChannel === "thought" ? "thought" : announcedChannel === "reply" ? "agent" : existing.channel;
      const partial = partialViewFrom(record.view["partial"]);
      if (
        existing.completed &&
        existing.channel === channel &&
        existing.partial?.state === partial?.state &&
        existing.partial?.reason === partial?.reason
      ) {
        return state;
      }
      const volatile = new Map(state.volatile);
      volatile.set(messageId, { ...existing, completed: true, channel, partial: partial ?? existing.partial });
      return { ...state, volatile };
    }

    if (record.eventType === "turn.completed" || record.eventType === "turn.failed" || record.eventType === "turn.cancelled") {
      const turnId = viewText(record.view, "turnId");
      if (turnId === null || sessionId === null) return state;
      const partial = record.eventType === "turn.completed" ? null : partialViewFrom({
        state: record.eventType === "turn.failed" ? "failed" : "cancelled",
        reason: record.eventType === "turn.failed" ? "agent_failed" : "turn_cancelled",
      });
      const volatile = new Map(state.volatile);
      let changed = false;
      for (const [messageId, tail] of volatile) {
        if (tail.turnId !== turnId || tail.sessionId !== sessionId) continue;
        volatile.set(messageId, { ...tail, completed: true, partial });
        changed = true;
      }
      return changed ? { ...state, volatile } : state;
    }

    if (record.eventType === "turn.delta_compacted") {
      const turnId = viewText(record.view, "turnId");
      if (turnId === null || sessionId === null) return state;
      const discardedTurns = new Set(state.discardedTurns);
      discardedTurns.add(`${sessionId}|${turnId}`);
      // Keep the visible stream mounted until `session.read` returns the authoritative
      // rows. Removing it here creates a blank frame; the read result atomically swaps
      // the two representations in `#applyReadResultInner`.
      if (this.#activeConversationSession === sessionId) {
        this.#messageRefreshQueued.add(sessionId);
      }
      return { ...state, discardedTurns };
    }

    return state;
  }

  /** 命令结果归并：终态回填 + `session.read`/`session.mode.list` 结果并入 + 结果不确定登记。 */
  #withCommandResult(result: CommandResultMessage): ClientState {
    // 文件栏查询没有命令记录：正文（尤其 `session.files.read` 的 `dataBase64`）只活在本次
    // 操作的闭包与在途表里，绝不能先落进 `commands`——那会把整份下载永久留在公开状态。
    // 迟到或无 pending 的结果同样只走 `#withSessionFilesResult`，由它丢弃。
    if (isSessionFilesCommandName(result.body.command)) {
      return this.#withSessionFilesResult(this.#state, result);
    }
    const record =
      this.#state.commands.get(result.body.requestId) ??
      createCommand({ requestId: result.body.requestId, command: result.body.command });
    const updated = applyCommandResult(record, {
      requestId: result.body.requestId,
      status: result.body.status,
      acceptedAt: result.body.acceptedAt,
      terminalEventId: result.body.terminalEventId,
      // `CommandResultValue` 是开放联合；命令记录只保存可展开成键值对象的那部分结果。
      result: toResultRecord(result.body.result),
      error: result.body.error,
    });
    const commands = new Map(this.#state.commands);
    commands.set(updated.requestId, updated);

    let next: ClientState = { ...this.#state, commands };
    // 准备首条（`session.start`）与它的原请求查询（`command.status`）：只改对应准备会话。
    if (result.body.command === "session.start") {
      next = this.#withPreparationStartResult(next, result, { statusQuery: false });
    }
    if (result.body.command === "command.status" && isCommandStatusRecord(result.body.result)) {
      next = this.#withPreparationStartResult(next, result, { statusQuery: true });
    }
    // 本机激活的受理/终态：只改对应会话的尝试（迟到的旧连接结果在归并里被丢弃）。
    if (result.body.command === "session.resume") next = this.#withActivationResult(next, result);
    if (result.body.command === "session.prompt") {
      const requestId = result.body.requestId;
      const promptBefore = next.sentPrompts.get(requestId) ?? null;
      // 已经受理过（或结果不确定）的输入不再被当作「可修正草稿」放回：它已经保存过，
      // 放回输入区会诱导用户再发一条。受理前失败才恢复草稿与选择。
      // 命令记录的先前状态也是证据：正式用户消息可能已经把本地条目接替掉（那时只剩命令历史）。
      const previousCommandState = this.#state.commands.get(requestId)?.state ?? null;
      // 快速失败可能只回一条终态（没有先前的 accepted）：终态带 `acceptedAt` 就是「已受理」的证据。
      const acceptedBefore = (result.body.status === "failed" && result.body.acceptedAt !== null) ||
        previousCommandState === "accepted" || previousCommandState === "uncertain" ||
        (promptBefore !== null && (promptBefore.phase === "accepted" || promptBefore.phase === "uncertain"));
      const snapshot = this.#promptDrafts.get(requestId);
      if (snapshot !== undefined && (result.body.status === "rejected" || result.body.status === "failed") &&
        !acceptedBefore) {
        // 受理前拒绝（如 Agent 不支持该媒体）或执行失败（尚未受理）：把派发前清掉的草稿/选择完整放回。
        const attachments = new Map(next.attachments);
        attachments.set(snapshot.sessionId, snapshot.attachments);
        next = {
          ...next,
          drafts: withEntry(next.drafts, snapshot.sessionId, snapshot.text),
          pathRefs: withEntry(next.pathRefs, snapshot.sessionId, snapshot.pathRefs),
          attachments,
        };
      }
      if (["rejected", "failed", "completed", "uncertain"].includes(result.body.status)) {
        this.#promptDrafts.delete(requestId);
      }
      switch (result.body.status) {
        case "accepted":
          // 服务端受理后给出本次 `turnId`：本地条目此后只按它与正式用户消息关联。
          next = withPromptPhase(next, requestId, (prompt) => ({
            ...prompt,
            phase: "accepted",
            turnId: acceptedTurnIdOf(result.body.result) ?? prompt.turnId,
            notice: null,
          }));
          break;
        case "uncertain":
          next = withPromptPhase(next, requestId, (prompt) => ({
            ...prompt,
            phase: "uncertain",
            turnId: acceptedTurnIdOf(result.body.result) ?? prompt.turnId,
            notice: prompt.notice ?? "受理结果不确定：可能已经保存；可以核实，不会自动重发。",
          }));
          break;
        case "rejected":
          // 从未受理：本地回声不能继续冒充「已发送」。
          next = { ...next, sentPrompts: withoutEntry(next.sentPrompts, requestId) };
          break;
        case "failed":
          // `failed` 是**已受理后**的执行失败（未受理是 `rejected`）：输入已由 Owner 保存，
          // 必须保留并进入「已受理」阶段；只有终态缺少 acceptedAt（从未受理）才移除。
          if (!acceptedBefore) {
            next = { ...next, sentPrompts: withoutEntry(next.sentPrompts, requestId) };
            break;
          }
          next = withPromptPhase(next, requestId, (prompt) => ({
            ...prompt,
            phase: "accepted",
            turnId: acceptedTurnIdOf(result.body.result) ?? prompt.turnId,
          }));
          break;
        case "completed":
          // 完成不改变「这条输入已显示」的事实；等权威用户消息按 `turnId` 接替它。
          next = withPromptPhase(next, requestId, (prompt) => ({
            ...prompt,
            turnId: acceptedTurnIdOf(result.body.result) ?? prompt.turnId,
          }));
          break;
      }
      // `user.message.delta` 可能先于受理结果到达。那时提交还没有 `turnId`，事件处理无法建立
      // `messageId → requestId` 的接替锚点；受理结果补齐 turnId 后必须立刻补做一次，否则本地条目
      // 会先按 `message:<id>` 让位，权威读取返回时又改回 `sent:<requestId>`，造成同一条正文的 DOM
      // 节点被卸下再挂回。这里只按服务端给出的同一 turnId 关联，不按文字或到达顺序猜测。
      const acceptedPrompt = next.sentPrompts.get(result.body.requestId);
      if (acceptedPrompt?.turnId != null) {
        const liveUserMessages = [...next.volatile.values()].flatMap((tail) =>
          tail.channel === "user" &&
            tail.sessionId === acceptedPrompt.sessionId &&
            tail.turnId === acceptedPrompt.turnId
            ? [{ messageId: tail.messageId, turnId: tail.turnId }]
            : []);
        // 权威消息页也可能先于受理结果返回：`session.read` 与 `session.prompt` 是两条独立命令，
        // 结果到达顺序不受页面控制。只扫实时增量会让本地回显状态一直留着；随后页替换若暂时不含
        // 这条消息，旧回显就会重新出现。这里同样只使用已经确认的 `turnId`，不按文字或时间推断。
        const readUserMessages = (next.threads.get(acceptedPrompt.sessionId)?.page?.messages ?? [])
          .flatMap((message) =>
            message.role === "user" && message.turnId === acceptedPrompt.turnId
              ? [{ messageId: message.id, turnId: message.turnId }]
              : []);
        const confirmedUserMessages = [...liveUserMessages, ...readUserMessages];
        next = withPromptTakeover(next, acceptedPrompt.sessionId, confirmedUserMessages);
      }
    }
    if (result.body.command === "session.start") {
      const requestId = result.body.requestId;
      const promptBefore = next.sentPrompts.get(requestId) ?? null;
      const previousCommandState = this.#state.commands.get(requestId)?.state ?? null;
      const acceptedBefore = previousCommandState === "accepted" || previousCommandState === "uncertain" ||
        (promptBefore !== null && (promptBefore.phase === "accepted" || promptBefore.phase === "uncertain"));
      if (result.body.status === "accepted") {
        next = withPromptPhase(next, requestId, (prompt) => ({ ...prompt, phase: "accepted", notice: null }));
      } else if (result.body.status === "uncertain") {
        next = withPromptPhase(next, requestId, (prompt) => ({
          ...prompt,
          phase: "uncertain",
          notice: prompt.notice ?? "受理结果不确定：可能已经保存；可以核实，不会自动重发。",
        }));
      } else if (result.body.status === "rejected" || (result.body.status === "failed" && !acceptedBefore)) {
        // 首条从未成立：草稿留在准备页，本地回声移除。
        next = { ...next, sentPrompts: withoutEntry(next.sentPrompts, requestId) };
      }
    }
    // 创建完成回复自带权威摘要；无需等待另一个 session.created 才让列表可见。
    // 只补缺失项：重投的创建结果不能覆盖之后的实时更新。
    if (result.body.command === "session.create" && result.body.status === "completed") {
      const value = toResultRecord(result.body.result);
      const summary = value?.["session"] as SessionSummary | undefined;
      if (summary !== undefined && summary !== null &&
        typeof summary.sessionId === "string" && summary.sessionId === value?.["sessionId"] &&
        !next.resources?.sessions.some((item) => item.sessionId === summary.sessionId)) {
        next = { ...next, resources: withSessionSummary(next.resources, summary) };
      }
    }
    if (result.body.command === "session.title.set") {
      next = this.#withTitleTerminal(next, {
        requestId: result.body.requestId,
        status: result.body.status,
        error: result.body.error,
      });
    }
    const folder = next.folderRequest;
    const statusRecord = result.body.result;
    if (result.body.command === "command.status" && result.body.status === "completed" &&
      isCommandStatusRecord(statusRecord) && folder?.requestId === statusRecord.targetRequestId) {
      const original = commands.get(folder.requestId);
      if (original !== undefined) {
        commands.set(folder.requestId, applyCommandResult(original, {
          requestId: folder.requestId, status: statusRecord.state, acceptedAt: statusRecord.acceptedAt,
          terminalEventId: statusRecord.terminalEventId, terminalAt: statusRecord.terminalAt,
          result: statusRecord.result, error: statusRecord.error,
        }));
        if (statusRecord.state === "uncertain" && !next.uncertainRequestIds.includes(folder.requestId)) {
          next = { ...next, uncertainRequestIds: [...next.uncertainRequestIds, folder.requestId] };
        }
      }
    }
    // `session.resume` 的 `uncertain` 只走显式重试的 `command.status` 核实：终态按同一份
    // 激活尝试归并，不换新 `requestId` 自动重派。
    if (result.body.command === "command.status" && result.body.status === "completed" &&
      isCommandStatusRecord(statusRecord)) {
      next = this.#withActivationStatus(next, statusRecord);
    }
    // 结果未知的标题命令在重连后用原 `requestId` 查回；查询结果按同一份命令记录归并。
    if (result.body.command === "command.status" && result.body.status === "completed" &&
      isCommandStatusRecord(statusRecord)) {
      const original = commands.get(statusRecord.targetRequestId);
      if (original?.command === "session.title.set") {
        commands.set(original.requestId, applyCommandResult(original, {
          requestId: original.requestId, status: statusRecord.state, acceptedAt: statusRecord.acceptedAt,
          terminalEventId: statusRecord.terminalEventId, terminalAt: statusRecord.terminalAt,
          result: statusRecord.result, error: statusRecord.error,
        }));
        this.#titleStatusQueries.delete(original.requestId);
        next = this.#withTitleTerminal(next, {
          requestId: original.requestId,
          status: statusRecord.state,
          error: statusRecord.error,
        });
      }
    }
    if (updated.state === "uncertain" && !next.uncertainRequestIds.includes(updated.requestId)) {
      // 结果不确定**只**在服务端确认后登记，且没有任何自动清除路径（R16 场景 4）。
      next = { ...next, uncertainRequestIds: [...next.uncertainRequestIds, updated.requestId] };
    }
    next = this.#withBrowseResult(next, updated, result);
    next = this.#withModeListResult(next, updated, result);
    return this.#applyReadResult(next, result);
  }

  /**
   * `workspace.browse` 的完成结果 → 浏览状态。
   *
   * 只认**当前那一层**的在途请求（`pendingRequestId`）：换层之后迟到的结果不再改界面（否则用户已经
   * 在别处，界面却被旧层的条目改写）。结果形状不符时必须给出一条明确的失败，而不是当成「这一层没有
   * 子目录」——那会把服务端答非所问呈现成正常的空层。
   */
  #withBrowseResult(state: ClientState, record: CommandRecord, result: CommandResultMessage): ClientState {
    const browse = state.directoryBrowse;
    if (record.command !== "workspace.browse" || browse.pendingRequestId !== record.requestId) return state;
    const settled: DirectoryBrowseState = {
      ...browse,
      loaded: true,
      pendingRequestId: null,
      entries: [],
      truncated: false,
      error: null,
    };
    if (result.body.status !== "completed") {
      return {
        ...state,
        directoryBrowse: {
          ...settled,
          error:
            record.error ?? {
              code: "internal.unavailable",
              message: `服务端未给出目录浏览失败的原因（${result.body.status}）。`,
              retryable: false,
              details: {},
            },
        },
      };
    }
    const parsed = parseBrowseResult(result.body.result);
    if (parsed === null) return { ...state, directoryBrowse: { ...settled, error: BROWSE_RESULT_INVALID_ERROR } };
    return {
      ...state,
      directoryBrowse: {
        ...settled,
        here: parsed.path,
        entries: parsed.entries,
        truncated: parsed.truncated,
      },
    };
  }

  /** `session.mode.list` 的完成结果 → 模式清单（模型/模式分节的「可选项」来源）。 */
  #withModeListResult(state: ClientState, record: CommandRecord, result: CommandResultMessage): ClientState {
    if (record.command !== "session.mode.list" || result.body.status !== "completed") return state;
    const sessionId = record.sessionId;
    const value = result.body.result;
    if (sessionId === null || typeof value !== "object" || value === null || Array.isArray(value)) return state;
    const currentModeId = (value as Readonly<Record<string, unknown>>)["currentModeId"];
    const version = (value as Readonly<Record<string, unknown>>)["version"];
    const availableModes = (value as Readonly<Record<string, unknown>>)["availableModes"];
    if (typeof version !== "string" || !Array.isArray(availableModes)) return state;
    if (currentModeId !== null && typeof currentModeId !== "string") return state;
    const modes = availableModes.filter(
      (mode): mode is { readonly modeId: string; readonly displayName: string } =>
        typeof mode === "object" &&
        mode !== null &&
        typeof (mode as Readonly<Record<string, unknown>>)["modeId"] === "string" &&
        typeof (mode as Readonly<Record<string, unknown>>)["displayName"] === "string",
    );
    const modeStates = new Map(state.modeStates);
    modeStates.set(sessionId, { currentModeId, availableModes: modes, version });
    return { ...state, modeStates };
  }

  /**
   * `session.read` 的外层：先走既有的分页合并，再把本次读取的原子来源信息并入运行期模型。
   *
   * - `readCursor` 推进最后连续位置 C（只用于衔接/去重，不持久化）；
   * - `messageCoverage` 只覆盖本次返回的消息：已被覆盖的增量从易失层移除，未覆盖的保留；
   * - `sessionSummary` 只有版本不更旧时才替换清单条目（D6，防止读取与实时更新竞态回滚标题）。
   */
  #applyReadResult(state: ClientState, result: CommandResultMessage): ClientState {
    const pending = this.#pendingReads.get(result.body.requestId);
    if (pending === undefined) return state;
    if (pending.remote !== null) {
      const currentGeneration = sourceGenerationOf(state, pending.remote.ownerNodeId);
      const runtime = state.remoteSessions.get(pending.sessionId);
      // 来源被清除/重置后，迟到结果不得把远程正文重新填回（design D3/D4）。
      if (
        currentGeneration !== pending.generation ||
        runtime === undefined ||
        runtime.ownerNodeId !== pending.remote.ownerNodeId
      ) {
        this.#pendingReads.delete(result.body.requestId);
        return state;
      }
    }
    const next = this.#applyReadResultInner(state, result);
    if (result.body.command !== "session.read" || result.body.status !== "completed") return next;
    const read = result.body.result as SessionReadResult;
    const runtime = next.remoteSessions.get(pending.sessionId);
    if (runtime === undefined) return next;
    const coverage = coverageFromRead(read.messageCoverage);
    const merged = mergeRemoteCoverage(runtime.coverage, coverage);
    let cursor = runtime.cursor;
    if (read.readCursor !== undefined) cursor = advanceOriginCursor(cursor, read.readCursor);
    const rebuilt = runtime.rebuilt || read.readCursor !== undefined;
    const phase = runtime.caughtUp && rebuilt ? "ready" : runtime.phase;
    const remoteSessions = new Map(next.remoteSessions);
    remoteSessions.set(pending.sessionId, { ...runtime, coverage: merged, cursor, rebuilt, phase });
    const volatile = new Map(next.volatile);
    if (read.resources.messages !== undefined) {
      for (const message of read.resources.messages) {
        const tail = volatile.get(message.messageId);
        if (tail === undefined) continue;
        const deltas = pruneCoveredDeltas(tail.deltas, coverage.get(message.messageId));
        if (deltas.size === 0) volatile.delete(message.messageId);
        else volatile.set(message.messageId, { ...tail, deltas });
      }
    }
    const resources =
      read.sessionSummary === undefined
        ? next.resources
        : withVersionedSummary(next.resources, read.sessionSummary);
    return { ...next, remoteSessions, volatile, resources };
  }

  /**
   * `session.read` 结果并入状态。
   *
   * 五种用途各自合并自己那一份资源（D2）：
   *
   * - `first`：整页替换消息、记下读回的明细资源（待处理交互/配置项）、记录历史页标记；
   *   并把读取结果里出现的 `messageId` 从易失叠加层里清掉——**读取结果为准**（D6）。
   * - `earlier`：消息向上翻页（不重不漏由 `assertPagesDoNotOverlap` 把关）。
   * - `earlier_history`：历史条目向上翻页（`assertHistoryPagesDoNotOverlap`）；
   *   它**不**动消息页，消息分页也**不**动历史页——两套游标互不影响。
   * - `refresh_interactions`：只替换服务端确认的当前有效请求，保留首屏的配置项与能力声明。
   */
  #applyReadResultInner(state: ClientState, result: CommandResultMessage): ClientState {
    const pending = this.#pendingReads.get(result.body.requestId);
    if (pending === undefined) return state;
    this.#pendingReads.delete(result.body.requestId);
    const threads = new Map(state.threads);
    const sessionId = pending.sessionId;
    let existing = threads.get(sessionId) ?? EMPTY_THREAD;

    if (result.body.command !== "session.read" || result.body.status !== "completed") {
      threads.set(sessionId, {
        ...existing,
        loadingEarlier: false,
        loadingHistory: false,
        // 刷新有效交互失败时保持原分页/历史错误，不把别的读取的失败借用过来。
        pagingError:
          pending.intent === "refresh_interactions"
            ? existing.pagingError
            : describeCommandFailure(result),
      });
      return { ...state, threads };
    }

    const read = result.body.result as SessionReadResult;
    if (read.resources.pending_interactions !== undefined) {
      existing = { ...existing, checkedInteractionEvents: new Set([
        ...(existing.checkedInteractionEvents ?? []), ...pending.interactionEventsAtRequest,
      ]) };
    }
    if (pending.intent === "refresh_interactions") {
      // 这一次读取本身就是「服务端当前有效请求」的确认：结果里没有的历史卡片从此标「不再有效」。
      threads.set(sessionId, { ...existing, interactionsRefreshed: true });
      return { ...withPendingInteractions(state, sessionId, read), threads };
    }
    if (pending.intent === "earlier_history") {
      const loaded = toLoadedHistoryPage(read);
      // 不重：新页不得与已有历史条目重叠（同一 `eventId` 出现两次说明游标算错）。
      assertHistoryPagesDoNotOverlap(loaded.items, [...state.history.values()]);
      threads.set(sessionId, {
        ...existing,
        historyHasEarlier: loaded.hasEarlier,
        nextBeforeHistory: loaded.nextBeforeHistory,
        loadingHistory: false,
        pagingError: null,
      });
      return {
        ...withHistory(state, sessionId, loaded.items, pending.remote?.ownerNodeId ?? null),
        threads,
      };
    }

    const loaded = toLoadedPage(read);
    // 读取屏障：只清掉**发起读取时已经存在**的增量；之后新到的增量留在叠加层里，由时间线接在
    // 权威正文之后。因此较早发出的读取结果不会覆盖较新的实时内容（design D5）。
    const volatileAfterRead = new Map(state.volatile);
    for (const message of loaded.messages) {
      const tail = volatileAfterRead.get(message.messageId);
      if (tail === undefined) continue;
      const confirmed = pending.volatileAtRequest.get(message.messageId);
      if (confirmed === undefined || confirmed.size === 0) continue;
      const deltas = new Map(tail.deltas);
      for (const deltaIndex of confirmed) deltas.delete(deltaIndex);
      if (deltas.size === 0) volatileAfterRead.delete(message.messageId);
      else volatileAfterRead.set(message.messageId, { ...tail, deltas });
    }
    // 权威消息页里属于本设备已提交输入的消息：同一次更新里移除本地回显并登记锚点。
    const confirmedUsers = loaded.messages.flatMap((message) =>
      message.role === "user" ? [{ messageId: message.messageId, turnId: message.turnId }] : []);
    if (pending.intent === "refresh_messages") {
      // 压缩后的权威回填：用最新消息页替换同 id 的旧条目，保留更早已加载的页。
      const loadedIds = new Set(loaded.messages.map((message) => message.messageId));
      const items = [
        ...loaded.messages,
        ...existing.items.filter((item) => !loadedIds.has(item.messageId)),
      ];
      // The completed authoritative page is the handoff point for compacted
      // streams. Rows it did not return must leave with the volatile tail;
      // otherwise an empty page leaves stale content from a discarded turn.
      const volatileAfterAuthoritativeRefresh = new Map(volatileAfterRead);
      for (const [messageId, tail] of volatileAfterAuthoritativeRefresh) {
        if (
          tail.sessionId === sessionId &&
          state.discardedTurns.has(`${sessionId}|${tail.turnId}`)
        ) {
          volatileAfterAuthoritativeRefresh.delete(messageId);
        }
      }
      threads.set(sessionId, {
        ...existing,
        page: buildSessionReadPageView(loaded.result),
        items,
        loadingEarlier: false,
        pagingError: null,
      });
      return {
        ...withPromptTakeover(state, sessionId, confirmedUsers),
        threads,
        volatile: volatileAfterAuthoritativeRefresh,
      };
    }

    // 首屏/远程重建把历史首页与明细资源落进来；翻页请求只并入自己那一份。
    const replacing = pending.intent === "first" || pending.intent === "rebuild_remote";
    let staged = state;
    if (replacing) {
      let base = state;
      if (pending.intent === "rebuild_remote" && pending.remote !== null) {
        // 重建是来源的最新权威视图：先清掉该来源该会话的旧历史，再并入本次首页。
        const history = new Map(state.history);
        for (const [key, record] of history) {
          if (
            record.ownerNodeId === pending.remote.ownerNodeId &&
            record.sessionId === sessionId
          ) {
            history.delete(key);
          }
        }
        base = { ...state, history };
      }
      staged = withReadResources(
        withHistory(base, sessionId, read.resources.history ?? [], pending.remote?.ownerNodeId ?? null),
        sessionId,
        read,
      );
    }

    if (
      pending.intent === "first" ||
      pending.intent === "rebuild_remote" ||
      existing.page === null
    ) {
      // 首屏/远程重建都是**最新权威页**：直接替换消息页，绝不当成向上翻页做重叠断言。
      const historyPage =
        pending.intent === "first" || pending.intent === "rebuild_remote"
          ? toLoadedHistoryPage(read)
          : null;
      threads.set(sessionId, {
        ...existing,
        page: buildSessionReadPageView(loaded.result),
        items: loaded.messages,
        historyHasEarlier: historyPage?.hasEarlier ?? existing.historyHasEarlier,
        nextBeforeHistory: historyPage?.nextBeforeHistory ?? existing.nextBeforeHistory,
        loadingEarlier: false,
        loadingHistory: false,
        pagingError: null,
      });
      return {
        ...withPromptTakeover(staged, sessionId, confirmedUsers),
        threads,
        volatile: volatileAfterRead,
      };
    }

    // 向上翻页：不重不漏由 `assertPagesDoNotOverlap` 把关，违反即抛而不是静默拼接。
    assertPagesDoNotOverlap(existing.items, loaded.messages);
    threads.set(sessionId, {
      ...existing,
      page: buildSessionReadPageView({
        sessionId,
        resources: { messages: [...loaded.messages, ...existing.items] },
        hasEarlier: loaded.hasEarlier,
      }),
      items: [...loaded.messages, ...existing.items],
      historyHasEarlier: existing.historyHasEarlier,
      nextBeforeHistory: existing.nextBeforeHistory,
      loadingEarlier: false,
      loadingHistory: false,
      pagingError: null,
    });
    return {
      ...withPromptTakeover(staged, sessionId, confirmedUsers),
      threads,
      volatile: volatileAfterRead,
    };
  }

  // ── 选择器 ────────────────────────────────────────────────────────────────

  /**
   * 仓库的**时钟端口**：页面（或任何消费方）读一次，再把同一个值传给需要它的选择器。
   *
   * 视图模型不读全局时钟（D3）：这一读一传让「同一帧里的所有时间文案用同一个基准」成为
   * 调用方可以保证的事，测试与 e2e 也能注入固定时刻。本类内部的时钟来自构造输入 `now`。
   */
  nowMs(): number {
    return this.#now();
  }

  /** 目录页模型（R15）。数据只来自快照，不发任何命令；查询与筛选只是纯渲染状态。 */
  directoryPage(): DirectoryPageModel {
    return buildDirectoryPageModel({
      resources: this.#state.resources,
      query: this.#state.directoryQuery,
      filter: this.#state.directoryFilter,
      pageInput: this.#state.directoryListPage,
      localLifecycleNegotiated: this.#state.lifecycle.negotiated,
    });
  }

  /**
   * 目录详情模型（R15 + 分页）。
   *
   * 页码是**单份**状态（跨路由存活），但它的含义与目录绑定：只有它属于**这个**目录时才采用，
   * 否则按「没有输入」（第一页）算。这样「进入另一个目录时从第一页开始」在**首帧**就成立，
   * 不依赖挂载后的复位——复位晚一帧时，上一次的页码会先被渲染出来再被跳回去。
   * 同一个目录内翻页不受影响：`setDirectoryDetailPage` 每次都把持有者写成它。
   *
   * @param nowMs 相对时间的基准；缺省读本仓库注入的时钟（{@link nowMs}）。
   */
  directoryDetail(directoryId: string, nowMs?: number): DirectoryDetailModel {
    return buildDirectoryDetailModel({
      resources: this.#state.resources,
      directoryId,
      ownerLabelOf: this.#ownerLabelOf,
      nowMs: nowMs ?? this.#now(),
      pageInput: this.#state.directoryDetailDirectoryId === directoryId ? this.#state.directoryDetailPage : "",
      localLifecycleNegotiated: this.#state.lifecycle.negotiated,
    });
  }

  /**
   * 目录页的内联会话行：按目录别名分组、组内按时间倒序。
   *
   * 只依赖快照，**不读** `directoryQuery`/`directoryFilter`/`directoryDetailPage`，
   * 也不派发任何命令——因此目录页的查询与筛选、详情页翻到的页码都不会改变目录页看到的内容。
   *
   * @param nowMs 相对时间的基准；缺省读本仓库注入的时钟（{@link nowMs}）。
   */
  directoryPageSessions(nowMs?: number): ReadonlyMap<string, readonly SessionRowModel[]> {
    return buildSessionRowsByDirectoryId({
      resources: this.#state.resources,
      ownerLabelOf: this.#ownerLabelOf,
      nowMs: nowMs ?? this.#now(),
      localLifecycleNegotiated: this.#state.lifecycle.negotiated,
    });
  }

  /** 本机已配置的 Agent 目录（快照 `agents` + 连接覆盖层，R19）。 */
  agentCatalog(): readonly AgentCatalogEntryView[] {
    return buildAgentCatalogView(this.#state.resources?.agents ?? [], this.#state.overlay).entries;
  }

  /**
   * 创建弹层模型（R16）。入口没有权限维度：创建入口恒可用。
   *
   * @param directoryId 入口所属的已记录目录标识；顶部入口（目录由浏览/选择给出）传 `null`。
   *   标识不在快照里时目录块退回浏览/选择形态——标识只是机器标识，不拿它当展示名。
   */
  createEntry(directoryId: string | null): CreateSessionEntryModel {
    const sheetKey = directoryId ?? CREATE_SHEET_BROWSE_KEY;
    const requestId = this.#state.createRequestIds.get(sheetKey);
    const workspaces = this.#state.resources?.workspaces ?? [];
    const workspace =
      directoryId === null ? undefined : workspaces.find((item) => item.directoryId === directoryId);
    const derived =
      directoryId === null ? undefined : directoryNamesOf(workspaces).get(directoryId);
    return buildCreateSessionEntryModel({
      directoryId,
      knownDirectory:
        workspace === undefined
          ? null
          : { displayName: derived?.displayName ?? workspace.path, path: workspace.path },
      browsingSupported: this.#state.selectedFeatures.includes(WORKSPACE_BROWSE_FEATURE),
      selection: this.#state.directoryBrowse.selection,
      agents: this.agentCatalog(),
      record: requestId === undefined ? null : (this.#state.commands.get(requestId) ?? null),
      connectionOnline: this.#state.connection.state === "online",
      preparation: this.preparationEntryAvailability(),
    });
  }

  /**
   * 本机激活的呈现（仅 owned 会话）。
   *
   * 未协商该 feature 时给出**明确的不可用**，而不是让页面以为「没有激活这件事」；imported
   * 会话返回 `null`（由来源节点管理，保持原任务三态合同）。
   */
  #activationViewOf(sessionId: Uuid, summary: SessionSummary | undefined): SessionActivationView | null {
    if (summary === undefined || summary.origin.kind !== "local") return null;
    const lifecycle = this.#state.lifecycle;
    if (!lifecycle.negotiated) {
      return { kind: "unavailable", notice: ACTIVATION_UNAVAILABLE_NOTICE };
    }
    const entry = lifecycle.activations.get(sessionId);
    if (entry === undefined) return { kind: "inactive", notice: ACTIVATION_INACTIVE_NOTICE };
    return activationViewOf({ phase: entry.phase, failure: entry.failure });
  }

  /**
   * 发送闸门用的「会话是否已可用」判定。
   *
   * 未协商 / imported / 旧合同：沿用原判定（`ready: true`，由连接与任务事实把关）。
   * 协商后的 owned 会话：只有 `ready` 才放行；恢复中/失败/尚未尝试都给出具体原因。
   */
  #activationReadiness(
    sessionId: Uuid,
    summary: SessionSummary | undefined,
  ): { readonly ready: boolean; readonly reason: string | null } {
    if (summary === undefined || summary.origin.kind !== "local") return { ready: true, reason: null };
    if (!this.#state.lifecycle.negotiated) return { ready: true, reason: null };
    const entry = this.#state.lifecycle.activations.get(sessionId);
    const view: SessionActivationView = entry === undefined
      ? { kind: "inactive", notice: ACTIVATION_INACTIVE_NOTICE }
      : activationViewOf({ phase: entry.phase, failure: entry.failure });
    switch (view.kind) {
      case "ready":
        return { ready: true, reason: null };
      case "activating":
        return { ready: false, reason: view.notice };
      case "failed":
        return { ready: false, reason: view.reason };
      case "inactive":
        return { ready: false, reason: view.notice };
      case "unavailable":
        return { ready: true, reason: null };
    }
  }

  /**
   * 对话页模型（R17 + R18 + 时间线 + 输入/交互/设置 + 两套分页 + 左栏 + 右栏差异区）。
   *
   * 会话既不在快照里也没有线程时为 `null`（没有任何可呈现的东西）。判定全部在这里与
   * `conversation-model.ts` / `conversation-timeline.ts` / `interaction-model.ts` 里完成，
   * 组件只消费结果。
   *
   * @param nowMs 相对时间的基准；缺省读本仓库注入的时钟（{@link nowMs}）。
   */
  conversationPage(sessionId: Uuid, nowMs?: number): ConversationPageModel | null {
    const preparation = this.preparationPage(sessionId, nowMs);
    if (preparation !== null && preparation.phase !== "committed" &&
        (preparation.phase !== "accepted" || this.#state.threads.get(sessionId)?.page == null)) {
      return this.#preparationConversation(preparation, nowMs ?? this.#now());
    }
    const thread = this.#state.threads.get(sessionId);
    const summary = this.#findSession(sessionId);
    const runtime = this.#state.remoteSessions.get(sessionId);
    if (summary === undefined && runtime !== undefined) {
      // 来源已清除且读取失败：给出一页明确的不可用，而不是继续展示缓存标题或假装空历史。
      const error = runtime.error;
      const origin = { kind: "remote" as const, online: false };
      const input = buildConversationInputModel({
        connectionOnline: false,
        originLocal: false,
        originOnline: false,
        turnInProgress: false,
        canCancel: false,
        promptCommand: null,
        cancelCommand: null,
        attachmentsNegotiated: false,
        attachments: buildAttachmentBatch({
          attachments: [],
          pathRefs: [],
          capabilityOf: (): AttachmentCapability => ({ kind: "supported" }),
        }),
        pathRefs: [],
      });
      return buildConversationModel({
        header: {
          sessionId,
          title: "远程会话不可用",
          agentName: "",
          state: null,
          directoryId: null,
        },
        page: null,
        usage: null,
        degraded: [],
        timeline: {
          entries: [],
          // 来源不可用时没有轮次/提交事实可归约：不呈现过程行。
          process: { phase: "none", label: null, detail: null },
          structuredHistory: structuredHistoryAvailability({
            origin,
            historyFeatureNegotiated: this.#state.selectedFeatures.includes(SESSION_HISTORY_FEATURE),
          }),
        },
        loadingEarlier: false,
        pagingError: error?.message ?? "来源节点当前不可用。",
        input,
        interactions: [],
        capabilities: conversationCapabilities({
          connectionOnline: false,
          turnInProgress: false,
          origin,
        }),
      });
    }

    if (thread === undefined && summary === undefined) return null;

    const remoteOrigin = summary?.origin.kind === "remote" ? summary.origin : null;
    const sourceOnline =
      remoteOrigin === null ||
      (remoteOrigin.online && runtime?.sourceState !== "offline" && runtime?.sourceState !== "revoked");
    const remoteReady = remoteOrigin === null || runtime === undefined || runtime.phase === "ready";
    const origin: "local" | { readonly kind: "remote"; readonly online: boolean } =
      remoteOrigin === null ? "local" : { kind: "remote", online: sourceOnline };

    const localLifecycleNegotiated = this.#state.lifecycle.negotiated;
    const header: ConversationHeaderModel = {
      sessionId,
      title: titleOf(summary),
      agentName: summary?.agent.name ?? "",
      // 没有摘要时是「未知」，不是空闲：MUST NOT 编造主状态。
      state: summary?.state ?? null,
      // 主标签：协商后的 owned 会话用本机活跃程度；旧合同/imported 保持任务三态。
      localActivity: summary === undefined ? null : localActivityOf(summary, localLifecycleNegotiated),
      activation: this.#activationViewOf(sessionId, summary),
      directoryId: summary?.workspace?.directoryId ?? null,
    };
    const connectionOnline = this.#state.connection.state === "online";
    const readiness = this.#activationReadiness(sessionId, summary);

    // 结构化记录：实时与历史按来源 + `eventId` 去重（实时优先），再筛出本会话。
    // 来源尚未重建完成时一律不呈现，避免把部分事件误当成完整视图。
    const records = remoteReady ? this.#recordsOf(sessionId) : [];
    const effectiveNow = nowMs ?? this.#now();
    const interactions = remoteReady
      ? buildInteractionViews({
          records,
          pending: this.#state.readResources.get(sessionId)?.pendingInteractions ?? [],
          responseRequestIds: this.#state.responseRequestIds,
          commands: this.#state.commands,
          deviceId: this.#state.deviceId,
          effectiveAt: new Date(effectiveNow).toISOString(),
          nowMs: effectiveNow,
          pendingChecked: thread?.interactionsRefreshed === true,
          checkedRequestEvents: thread?.checkedInteractionEvents,
          requestVersionRequired: this.#state.selectedFeatures.includes(PENDING_INTERACTIONS_FEATURE),
        })
      : [];
    const activeTurn = activeTurnOf(records);
    const sentPrompts = this.#sentPromptsOf(sessionId);
    const entries = remoteReady
      ? this.#stabilizeTimeline(sessionId, buildTimeline({
          messages: thread?.page?.messages ?? [],
          records,
          volatile: [...this.#state.volatile.values()].filter((tail) => tail.sessionId === sessionId),
          interactions,
          sentPrompts,
          promptAnchors: this.#state.sentPromptAnchors.get(sessionId),
        }))
      : [];
    // 过程阶段与时间线**同一份输入**（条目 + 本地提交 + 活动轮次），因此「等待」「恢复」
    // 不会与实际已经出现的思考/工具/回复各说一套。
    const process = buildConversationProcess({
      restoring: header.activation?.kind === "activating",
      prompts: sentPrompts,
      activeTurnId: activeTurn === null ? null : activeTurn.turnId,
      entries,
    });
    // 降级条目与时间线**同源**：扁平列表是时间线上 `kind === "degraded"` 的投影，
    // 因此不可能出现「时间线没有降级卡而诊断列表有」这类不一致。
    const degraded = entries.flatMap((entry) => (entry.kind === "degraded" ? [entry.degraded] : []));

    const configSource = this.#configSourceOf(sessionId);
    const mode = this.#state.modeStates.get(sessionId) ?? null;
    const settings: ConversationSettingsModel = buildConversationSettingsModel({
      configOptions: configSource.options,
      mode,
      modeChange: this.#modeChangeOf(sessionId),
      summaryMode: summary?.currentMode ?? null,
      modelSwitchCommand: this.#switchCommandOf(sessionId, "model"),
      modeSwitchCommand: this.#switchCommandOf(sessionId, "mode"),
    });
    const draftAttachments = this.#state.attachments.get(sessionId) ?? [];
    const pathRefs = this.#state.pathRefs.get(sessionId) ?? [];
    const agentCapabilities = this.#state.readResources.get(sessionId)?.agentCapabilities ?? null;
    const attachmentsNegotiated =
      this.#state.selectedFeatures.includes(ATTACHMENT_FEATURE) &&
      this.#attachmentTransfer !== null &&
      this.#attachmentMedia !== null;
    const attachmentBatch = buildAttachmentBatch({
      attachments: draftAttachments,
      pathRefs,
      capabilityOf: (kind: AttachmentKind): AttachmentCapability =>
        attachmentCapabilityOf(kind, agentCapabilities),
    });
    const input: ConversationInputModel = buildConversationInputModel({
      connectionOnline,
      originLocal: origin === "local",
      originOnline: origin === "local" ? true : origin.online,
      turnInProgress: activeTurn !== null,
      // 取消只看「有活动轮次 + 连接可用」；主状态（idle/running/pending）不参与。
      canCancel: connectionOnline && activeTurn !== null,
      promptCommand: this.#lastCommandOf(this.#state.promptRequestIds.get(sessionId)),
      cancelCommand:
        activeTurn === null ? null : this.#lastCommandOf(this.#state.cancelRequestIds.get(activeTurn.turnId)),
      draft: this.#state.drafts.get(sessionId) ?? "",
      attachmentsNegotiated,
      attachments: attachmentBatch,
      pathRefs,
      // 恢复中/失败时保留历史与草稿，但发送闸门关闭；`active` 标签本身不是发送依据。
      sessionNotReadyReason: readiness.reason,
    });

    return buildConversationModel({
      header,
      page: remoteReady ? (thread?.page ?? null) : null,
      usage: remoteReady ? (this.#state.usage.get(sessionId) ?? null) : null,
      degraded,
      timeline: {
        entries,
        process,
        windowLoading: this.#timelineFloors.has(sessionId),
        structuredHistory: structuredHistoryAvailability({
          origin,
          historyFeatureNegotiated: this.#state.selectedFeatures.includes(SESSION_HISTORY_FEATURE),
        }),
      },
      loadingEarlier: remoteReady ? (thread?.loadingEarlier ?? false) : false,
      pagingError: remoteReady
        ? (thread?.pagingError ?? null)
        : (runtime?.error?.message ?? "正在从来源重建会话，完成后才呈现内容。"),
      historyHasEarlier: remoteReady ? (thread?.historyHasEarlier ?? false) : false,
      loadingHistory: remoteReady ? (thread?.loadingHistory ?? false) : false,
      nextBeforeHistory: remoteReady ? (thread?.nextBeforeHistory ?? null) : null,
      input,
      interactions,
      media: this.#state.attachmentMedia,
      settings,
      fileChanges: buildFileChangeViews(records),
      // 左栏只读快照：同目录会话、目录上下文与「已呈现 / 总数」都由快照推出，没有命令出口。
      sidebar: buildConversationSidebar({
        resources: this.#state.resources,
        workspace: summary?.workspace ?? null,
        pendingRows: this.#unsentSidebarRows(summary?.workspace?.directoryId ?? null),
        sessionId,
        ownerLabelOf: this.#ownerLabelOf,
        query: this.#state.conversationSidebarQuery,
        pageInput: this.#state.conversationSidebarPageInput,
        nowMs: nowMs ?? this.#now(),
        localLifecycleNegotiated,
      }),
      capabilities: conversationCapabilities({
        connectionOnline,
        turnInProgress: activeTurn !== null,
        origin,
        sessionReady: readiness.ready,
      }),
    });
  }

  #unsentSidebarRows(directoryId: string | null, path?: string): readonly SessionRowModel[] {
    const directoryPath = path ?? this.#state.resources?.workspaces.find((item) => item.directoryId === directoryId)?.path;
    if (directoryPath === undefined) return [];
    return [...this.#state.preparations.values()].filter((session) =>
      !["committed", "released"].includes(session.phase) &&
      displayDirectoryPath(session.workspace.path) === displayDirectoryPath(directoryPath)
    ).map((session) => ({
      id: session.sessionId, title: "新会话",
      agentName: this.agentCatalog().find((agent) => agent.id === session.agentId)?.displayName ?? session.agentId,
      state: "idle", activity: { kind: "task", value: "idle" }, localActivity: null,
      directoryId, updatedAt: "", updatedAtLabel: session.association === null ? "未发送消息" : "",
    }));
  }

  /** 首条发送前沿用正常布局；不伪造持久化会话或历史。 */
  #preparationConversation(page: PreparationPageModel, nowMs: number): ConversationPageModel {
    const attachmentsNegotiated = this.#state.selectedFeatures.includes(ATTACHMENT_FEATURE) &&
      this.#attachmentTransfer !== null && this.#attachmentMedia !== null;
    const attachments = buildAttachmentBatch({
      attachments: page.attachments,
      pathRefs: [],
      capabilityOf: (): AttachmentCapability => ({ kind: "supported" }),
    });
    // 分配、恢复、保存和等待同页接管都属于正常流程。它们会暂时关闭发送，但不能作为
    // `session_not_ready` 原因多画一行；那一行会在首次回车后改变输入区高度，并一直留到
    // 正式会话页接管。失败、结果不确定、设置或租约等真实阻塞仍按原状态呈现。
    const preparationInFlight =
      page.phase === "allocating" ||
      page.phase === "restoring" ||
      page.phase === "submitting" ||
      page.phase === "accepted" ||
      page.phase === "committed";
    const input = buildConversationInputModel({
      connectionOnline: this.#state.connection.state === "online",
      originLocal: true,
      originOnline: true,
      turnInProgress: preparationInFlight,
      canCancel: false,
      promptCommand: null,
      cancelCommand: null,
      draft: page.text,
      attachmentsNegotiated,
      attachments,
      pathRefs: [],
      sessionNotReadyReason:
        page.input.canSend || preparationInFlight ? null : page.input.reason,
    });
    const settings = buildConversationSettingsModel({
      configOptions: page.settings.configOptions.map(({ option, value }) => ({ ...option, currentValue: value })),
      mode: null,
      modeChange: null,
      summaryMode: null,
      modelSwitchCommand: null,
      modeSwitchCommand: null,
    });
    const selectedMode = page.settings.modes.find((mode) => mode.modeId === page.settings.selectedMode);
    const preparationSettings: ConversationSettingsModel = { ...settings, mode: {
      state: page.settings.modes.length === 0 ? "unavailable" : "available",
      currentId: page.settings.selectedMode,
      currentName: selectedMode?.displayName ?? null,
      choices: page.settings.modes.map((mode) => ({ id: mode.modeId, name: mode.displayName,
        description: null, current: mode.modeId === page.settings.selectedMode })),
      notice: page.settings.modes.length === 0 ? "该会话的 Agent 未提供模式可选项。" : null,
      configId: null,
    } };
    const settingsEditable = page.phase === "ready" || page.phase === "failed";
    // 首条派发后（`submitting`/`accepted`）Agent 的思考、工具调用与权限请求已经会经实时事件到达；
    // 正式读取完成前也必须呈现它们，不能因为「页面还挂在准备态」就把整条时间线画成空的。
    const records = this.#recordsOf(page.sessionId);
    const interactions = buildInteractionViews({
      records,
      pending: this.#state.readResources.get(page.sessionId)?.pendingInteractions ?? [],
      responseRequestIds: this.#state.responseRequestIds,
      commands: this.#state.commands,
      deviceId: this.#state.deviceId,
      effectiveAt: new Date(nowMs).toISOString(),
      nowMs,
      pendingChecked: this.#state.threads.get(page.sessionId)?.interactionsRefreshed === true,
      checkedRequestEvents: this.#state.threads.get(page.sessionId)?.checkedInteractionEvents,
      requestVersionRequired: this.#state.selectedFeatures.includes(PENDING_INTERACTIONS_FEATURE),
    });
    const sentPrompts = this.#sentPromptsOf(page.sessionId);
    const entries = this.#stabilizeTimeline(page.sessionId, buildTimeline({
      messages: [],
      records,
      volatile: [...this.#state.volatile.values()].filter((tail) => tail.sessionId === page.sessionId),
      interactions,
      sentPrompts,
      promptAnchors: this.#state.sentPromptAnchors.get(page.sessionId),
    }));
    const activeTurn = activeTurnOf(records);
    // 准备态没有正式会话的激活事实：恢复提示不适用；提交/等待/内容阶段照常归约。
    const process = buildConversationProcess({
      restoring: false,
      prompts: sentPrompts,
      activeTurnId: activeTurn === null ? null : activeTurn.turnId,
      entries,
    });
    const degraded = entries.flatMap((entry) => (entry.kind === "degraded" ? [entry.degraded] : []));
    return buildConversationModel({
      header: {
        sessionId: page.sessionId,
        title: "新会话",
        newSession: true,
        agentName: this.agentCatalog().find((agent) => agent.id === page.agentId)?.displayName ?? page.agentId,
        state: null,
        directoryId: page.workspace.directoryId,
      },
      page: null,
      usage: this.#state.usage.get(page.sessionId) ?? null,
      degraded,
      loadingEarlier: false,
      pagingError: null,
      timeline: { entries, process, structuredHistory: structuredHistoryAvailability({
        origin: "local", historyFeatureNegotiated: this.#state.selectedFeatures.includes(SESSION_HISTORY_FEATURE),
      }) },
      input: { ...input, canSubmit: page.input.canSend && input.canSubmit,
        persistenceWarning: page.error === null ? null : page.notice },
      settings: settingsEditable ? preparationSettings : {
        model: { ...preparationSettings.model, state: "switching", notice: page.input.reason },
        mode: { ...preparationSettings.mode, state: "switching", notice: page.input.reason },
      },
      interactions,
      sidebar: buildConversationSidebar({
        resources: this.#state.resources,
        pendingRows: this.#unsentSidebarRows(page.workspace.directoryId, page.workspace.path),
        workspace: page.workspace.directoryId === null ? null : { directoryId: page.workspace.directoryId },
        workspacePath: page.workspace.path,
        sessionId: page.sessionId,
        ownerLabelOf: this.#ownerLabelOf,
        query: this.#state.conversationSidebarQuery,
        pageInput: this.#state.conversationSidebarPageInput,
        nowMs,
        localLifecycleNegotiated: this.#state.lifecycle.negotiated,
      }),
      capabilities: { canSend: page.input.canSend, canCancel: false, canViewHistory: false },
    });
  }

  /** 主机与连接面板模型（R19）。 */
  hostPanel(): HostPanelModel {
    return buildHostPanelModel({
      resources: this.#state.resources,
      overlay: this.#state.overlay,
      connection: this.#state.connection.state,
      blocking: this.#state.connection.blocking,
      host: this.#state.host,
      // 节点级降级事件（没有会话可挂的）挂在这一面：它们的出口只有这里（D5/R18）。
      nodeDegraded: this.nodeDegraded(),
    });
  }

  /**
   * 节点级降级条目：**没有会话**的事件（如 `device.revoked`）的降级呈现。
   *
   * `conversationPage()` 只呈现本会话的记录，节点级事件没有会话时间线可挂；只把它登记进
   * `UNCOVERED_EVENT_TYPES` 而不给出口，就是「被消费却既不呈现也不降级」——D5/R18 明令禁止的
   * 第三种结果。它们按时间位置升序交出去，由主机/连接面承载（`HostPanelModel.nodeDegraded`），
   * 诊断抽屉照旧按 `eventId` 打开（`openDiagnostics`）。
   */
  nodeDegraded(): readonly DegradedEventModel[] {
    const records = [...this.#state.structured.values()].filter(
      (record) => record.sessionId === null && isDegradedRecord(record),
    );
    // 到达顺序不等于时间顺序（重投、批次交错），按时间位置排序让这一面的内容可复现。
    records.sort(compareRecordOrder);
    return records.map((record) => buildDegradedRecordModel(record));
  }

  /** 配对页模型。 */
  pairingPage(): PairingPageModel {
    const page = this.#state.pairing;
    if (page.kind === "sync" && page.phase === "awaiting_code" && this.#state.host !== null) {
      return {
        ...page,
        presentation: {
          ...page.presentation,
          title: "重新配对",
          lede: "此浏览器已有这台电脑的配对记录。请在电脑上重新发起配对，输入终端里显示的 8 位配对码。",
        },
      };
    }
    return page;
  }

  /**
   * 诊断抽屉模型：选中记录的原文与元数据（R18）。
   *
   * 记录从结构化记录里按 `eventId` 取（实时与历史都在内），因此历史条目的降级同样可诊断。
   * 记录存在时一定给出模型：降级记录走 {@link buildDegradedRecordModel}（保留原有的降级
   * 归因），其余记录走 {@link buildRecordDiagnosticsModel}（`reason.kind === "explicit"`）。
   * 后者是必须的——`ConversationTimeline` 的「查看 ACP 原文」对正常工具调用也会调用
   * `openDiagnostics(eventId)`，若这里只服务降级记录，那些按钮会打开一个空抽屉。
   *
   * 只有记录本身不存在时才返回 `null`（例如事件已被淘汰）。
   */
  diagnostics(): DegradedEventModel | null {
    const id = this.#state.diagnosticsEventId;
    if (id === null) return null;
    const record =
      this.#state.structured.get(id) ??
      this.#state.history.get(id) ??
      [...this.#state.structured.values(), ...this.#state.history.values()].find(
        (candidate) => candidate.eventId === id,
      );
    if (record === undefined) return null;
    return isDegradedRecord(record)
      ? buildDegradedRecordModel(record)
      : buildRecordDiagnosticsModel(record);
  }

  /** 该会话最新的配置项来源：配置事件优先，其次是首屏读回的 `config_options`。 */
  #configSourceOf(sessionId: Uuid): { readonly options: readonly ConfigOptionView[]; readonly version: string | null } {
    const latest = newestRecordOf(
      [...this.#state.structured.values(), ...this.#state.history.values()].filter(
        (record) => record.sessionId === sessionId && record.eventType === "session.config.changed",
      ),
    );
    if (latest !== null) {
      const card = buildTimelineCard(recordGroupOf(latest), () => null);
      if (card !== null && card.kind === "config_changed") {
        return { options: card.configOptions, version: card.version };
      }
    }
    const staged = this.#state.readResources.get(sessionId);
    return staged === undefined ? { options: [], version: null } : { options: staged.configOptions, version: staged.configVersion };
  }

  /**
   * 该会话最新的模式变更来源（`session.mode.changed`）。
   *
   * 与 `#configSourceOf` 对称：实时事件与历史条目合并后取时间位置最新的一条（重开对话页时
   * 历史里的变更同样是「当前模式」的来源）。服务端不为模式变更产出 `session.updated`，因此
   * 这份事件是当前模式与并发版本的**唯一**活动信号：`session.mode.set` 的 `expectedVersion`
   * 也从这里取，否则服务端确认过的变更会让版本停在旧值上、切换被版本冲突直接拒掉。
   */
  #modeChangeOf(sessionId: Uuid): ModeChangedTimelineCard | null {
    const latest = newestRecordOf(
      [...this.#state.structured.values(), ...this.#state.history.values()].filter(
        (record) => record.sessionId === sessionId && record.eventType === "session.mode.changed",
      ),
    );
    if (latest === null) return null;
    const card = buildTimelineCard(recordGroupOf(latest), () => null);
    return card !== null && card.kind === "mode_changed" ? card : null;
  }

  /** 最近一次模型/模式切换命令（只在意图是对应的那一类时返回）。 */
  #switchCommandOf(sessionId: Uuid, kind: "model" | "mode"): CommandRecord | null {
    const intent = this.#state.switchIntents.get(sessionId);
    if (intent === undefined || intent.kind !== kind) return null;
    return this.#lastCommandOf(intent.requestId);
  }

  /** 按 `requestId` 取命令记录（`undefined` 即没有提交过）。 */
  #lastCommandOf(requestId: Uuid | undefined): CommandRecord | null {
    if (requestId === undefined) return null;
    return this.#state.commands.get(requestId) ?? null;
  }

  /** 是否处于阻断态（页面据此停止一切主动动作）。 */
  get blocked(): boolean {
    return isBlockingState(this.#state.connection.state);
  }

  // ── 用户意图 ──────────────────────────────────────────────────────────────

  /**
   * 打开创建弹层（目录层级入口：目录**已知**）。
   *
   * 三个写入方都从这里或 {@link openCreateSheetToBrowse} 走，因此「弹层是否打开」永远只有一处判据。
   * 同时复位浏览状态：换一个目录入口就是新一次创建，上一次的选定不该被带过来。
   */
  openCreateSheet(directoryId: string): void {
    this.#commit({
      ...this.#state,
      createSheetDirectoryId: directoryId,
      createSheetBrowses: false,
      directoryBrowse: this.#state.folderRequest === null ? initialDirectoryBrowseState() : { ...this.#state.directoryBrowse, open: false, selection: null },
    });
  }

  /**
   * 打开创建弹层（顶部入口：目录**未知**，要先浏览/选择一层）。
   *
   * 这条入口同样**恒可用**（创建入口没有权限维度）；服务端没提供目录浏览能力时，
   * 弹层按「不可用并保留已登记目录这条路」呈现（见 `create-session-model.ts` 的目录块模型）。
   */
  openCreateSheetToBrowse(): void {
    this.#commit({
      ...this.#state,
      createSheetDirectoryId: null,
      createSheetBrowses: true,
      directoryBrowse: this.#state.folderRequest === null ? initialDirectoryBrowseState() : { ...this.#state.directoryBrowse, open: false, selection: null },
    });
  }

  /** 当前会话目录尚未登记时，沿用 Owner 已确认的路径打开新建入口。 */
  openCreateSheetAtPath(path: string): void {
    this.#commit({
      ...this.#state,
      createSheetDirectoryId: null,
      createSheetBrowses: true,
      directoryBrowse: { ...initialDirectoryBrowseState(), selection: {
        kind: "path", path, name: path.replace(/[\\/]+$/, "").split(/[\\/]/).at(-1) ?? path,
        registered: false, willCreate: false,
      } },
    });
  }

  /** 关闭创建弹层（连带关闭并复位它的目录选择器：选定只在一次弹层会话里有效）。 */
  closeCreateSheet(): void {
    if (this.#state.createSheetDirectoryId === null && !this.#state.createSheetBrowses) return;
    const directoryId = this.#state.createSheetDirectoryId;
    const createRequestIds = new Map(this.#state.createRequestIds);
    // 只解除已成功创建的弹层关联；命令记录继续保留，未完成和不确定结果不被清除。
    if (this.createEntry(directoryId).submission.kind === "created") {
      createRequestIds.delete(directoryId ?? CREATE_SHEET_BROWSE_KEY);
    }
    this.#commit({
      ...this.#state,
      createRequestIds,
      createSheetDirectoryId: null,
      createSheetBrowses: false,
      directoryBrowse: this.#state.folderRequest === null ? initialDirectoryBrowseState() : { ...this.#state.directoryBrowse, open: false, selection: null },
    });
  }

  /**
   * 打开目录浏览选择器。
   *
   * 这里是**只**翻开关：第一层数据由选择器挂载时请求（`initialPath` → `onBrowse`），
   * 因此「打开」不依赖页面先知道该请求哪一层。已确认的选定保留，用户取消后重开能从原处继续。
   */
  openDirectoryPicker(): void {
    this.#commit({ ...this.#state, directoryBrowse: openedDirectoryBrowseState(this.#state.directoryBrowse) });
  }

  /** 关闭目录浏览选择器（保留选定与浏览轨迹）。 */
  closeDirectoryPicker(): void {
    if (!this.#state.directoryBrowse.open) return;
    this.#commit({
      ...this.#state,
      directoryBrowse: { ...this.#state.directoryBrowse, open: false, folderError: null },
    });
  }

  /**
   * 请求某一层目录（`null` = 该节点的根列表）。
   *
   * 四道闸门与 `loadEarlierHistory` 同一个口径：未协商 `core.workspace-browse.v1` 时**不派发**——
   * 服务端会在触碰文件系统之前回 `protocol.feature_required`，靠「发了再看反应」只会白跑一趟；
   * 未连接同理（命令信封需要 `connectionId`）。面包屑由 {@link crumbsFor} 从**服务端下发**的条目
   * 或当前选定推出，路径一律是服务端给过的那一份。
   */
  browseDirectory(path: string | null): CommandOutcome {
    if (!this.#state.selectedFeatures.includes(WORKSPACE_BROWSE_FEATURE)) {
      return { ok: false, reason: "服务端未提供目录浏览能力（未协商 core.workspace-browse.v1）" };
    }
    const browse = this.#state.directoryBrowse;
    const crumbs = crumbsFor({
      current: browse.crumbs,
      entries: browse.entries,
      selection: browse.selection,
      path,
    });
    const requestId = this.#dispatchIntent({
      sessionId: null,
      build: (id) => ({ requestId: id, command: "workspace.browse", payload: { path } }),
      patch: (id) => ({
        directoryBrowse: {
          ...browse,
          here: path,
          crumbs,
          // 换层时先清空条目：不能拿上一层的条目冒充这一层（结果回来前呈现「读取中」）。
          entries: [],
          truncated: false,
          loaded: false,
          pendingRequestId: id,
          error: null,
          folderError: null,
        },
      }),
    });
    if (requestId !== null) return { ok: true, requestId };
    // 没有可用的已认证连接：命令发不出去。这**不是**「正在读取」，因此留下一条明确的原因，
    // 否则选择器会永远停在「正在读取这一层的目录……」（静默失败）。
    this.#commit({
      ...this.#state,
      directoryBrowse: {
        ...browse,
        loaded: true,
        pendingRequestId: null,
        entries: [],
        truncated: false,
        error: {
          code: "internal.unavailable",
          message: "尚未建立已认证连接，无法读取主机上的目录。",
          retryable: true,
          details: {},
        },
      },
    });
    return NOT_CONNECTED;
  }

  /** 独立创建一层目录；只有主机完成后才重新读取真实列表。 */
  createFolderInBrowse(name: string): CreateFolderOutcome {
    if (!this.#state.selectedFeatures.includes("core.workspace-mkdir.v1")) {
      return { ok: false, reason: "主机未提供独立新建文件夹能力，仍可选择已有目录。" };
    }
    const browse = this.#state.directoryBrowse;
    if (browse.here === null || !browse.loaded || browse.pendingRequestId !== null || browse.error !== null) {
      return { ok: false, reason: "请先读取一个可用目录。" };
    }
    const previous = this.#state.folderRequest;
    const prior = previous === null ? null : this.#lastCommandOf(previous.requestId);
    if (prior !== null && ["submitting", "accepted", "uncertain"].includes(prior.state)) {
      return { ok: false, reason: "上次新建尚未确认，请先确认结果，不要重复创建。" };
    }
    const built = buildCreateFolderPath({ parent: browse.here, name });
    if (!built.ok) {
      this.#commit({ ...this.#state, directoryBrowse: { ...browse, folderError: built.reason } });
      return { ok: false, reason: built.reason };
    }
    const parentPath = browse.here;
    const requestId = this.#dispatchIntent({
      sessionId: null,
      build: (id) => ({ requestId: id, command: "workspace.mkdir", payload: { parentPath, name: built.name } }),
      patch: (id) => ({
        folderRequest: { requestId: id, parentPath, name: built.name, refreshed: false },
        directoryBrowse: { ...browse, folderError: null },
      }),
    });
    return requestId === null ? NOT_CONNECTED : { ok: true, requestId };
  }

  /** 确认选定并关掉选择器（选定内容逐字来自服务端下发或用户输入的新层名字）。 */
  selectBrowsedDirectory(selection: DirectorySelection): void {
    this.#commit({
      ...this.#state,
      directoryBrowse: { ...this.#state.directoryBrowse, open: false, folderError: null, selection },
    });
  }

  /** 目录浏览选择器的视图模型。 */
  directoryBrowser(): DirectoryBrowserModel {
    return buildDirectoryBrowserModel({
      supported: this.#state.selectedFeatures.includes(WORKSPACE_BROWSE_FEATURE),
      browse: this.#state.directoryBrowse,
      mkdirSupported: this.#state.selectedFeatures.includes("core.workspace-mkdir.v1"),
      folder: this.#state.folderRequest === null ? null : {
        ...this.#state.folderRequest,
        state: this.#lastCommandOf(this.#state.folderRequest.requestId)?.state ?? "submitting",
        error: this.#lastCommandOf(this.#state.folderRequest.requestId)?.error?.message ?? null,
      },
    });
  }

  /** 查询原命令，不重复执行文件系统操作。 */
  checkFolderCreation(): void {
    const folder = this.#state.folderRequest;
    if (folder === null) return;
    this.#dispatchIntent({ sessionId: null,
      build: (requestId) => ({ requestId, command: "command.status", payload: { targetRequestId: folder.requestId } }),
    });
  }

  /** 用户核对主机后显式解除 uncertain 阻挡；只刷新，不重发 mkdir。 */
  resolveFolderUncertain(): void {
    const folder = this.#state.folderRequest;
    if (folder === null || this.#lastCommandOf(folder.requestId)?.state !== "uncertain") return;
    this.dismissUncertain(folder.requestId);
    this.#commit({ ...this.#state, folderRequest: null });
    this.browseDirectory(folder.parentPath);
  }

  /** 本机已记录的目录（快照 `workspaces`）：不支持浏览时的回退选择。 */
  registeredDirectories(): readonly RegisteredDirectoryModel[] {
    const workspaces = this.#state.resources?.workspaces ?? [];
    const names = directoryNamesOf(workspaces);
    return workspaces.map((workspace) => ({
      directoryId: workspace.directoryId,
      displayName: names.get(workspace.directoryId)?.displayName ?? workspace.path,
      path: workspace.path,
    }));
  }

  /** 打开诊断抽屉。 */
  openDiagnostics(eventId: string): void {
    this.#commit({ ...this.#state, diagnosticsEventId: eventId });
  }

  /** 关闭诊断抽屉。 */
  closeDiagnostics(): void {
    this.#commit({ ...this.#state, diagnosticsEventId: null });
  }

  /**
   * 更新目录页查询词。
   *
   * 它是**目录页独有**的 UI 状态：详情页没有搜索框（规格把「目录详情的目录内搜索」移除了），
   * 因此进入详情页不得清空它，返回时它仍在。
   */
  setDirectoryQuery(query: string): void {
    this.#commit({ ...this.#state, directoryQuery: query, directoryListPage: "" });
  }

  /** 更新目录页状态筛选；筛选改变后同样回到第一页。 */
  setDirectoryFilter(filter: DirectoryFilter): void {
    this.#commit({ ...this.#state, directoryFilter: filter, directoryListPage: "" });
  }

  /**
   * 提交目录页页码输入（回车/失焦时调用）。
   *
   * 与详情页同一套纠正规则：非法输入回落到当前生效页，越界值夹到 `[1, pageCount]`。
   */
  setDirectoryListPage(requested: string): void {
    const model = this.directoryPage();
    const current = model.kind === "ready" || model.kind === "empty" ? model.page : 1;
    const pageCount = model.kind === "ready" || model.kind === "empty" ? model.pageCount : 1;
    this.#commit({
      ...this.#state,
      directoryListPage: String(correctPageNumber({ requested, pageCount, fallback: current })),
    });
  }

  /**
   * 提交详情页的页码输入（回车/失焦时调用）。
   *
   * 纠正**在提交时**完成，且回落值是**当前生效页**：因此敲一段非数字不会把列表换掉（规格
   * 「列表内容不因一次非法输入而重画」——纠正后的值就是原来的值），越界数字则夹紧到边界。
   * 存回状态的也是纠正后的字符串，于是输入框的回填与列表永远一致。持有者同时记为目录标识：
   * 这个页码从此只属于这个目录（见 `directoryDetail`）。
   *
   * @param directoryId 正在看的目录：页数与生效页码都取决于它（`total` 是**该目录**的行数）。
   */
  setDirectoryDetailPage(directoryId: string, requested: string): void {
    const model = this.directoryDetail(directoryId);
    const current = model.kind === "ready" ? model.page : 1;
    const pageCount = model.kind === "ready" ? model.pageCount : 1;
    this.#commit({
      ...this.#state,
      directoryDetailPage: String(correctPageNumber({ requested, pageCount, fallback: current })),
      directoryDetailDirectoryId: directoryId,
    });
  }

  /** 更新对话页的次级视图（窄屏切换控件）。 */
  setConversationSecondaryView(view: ConversationSecondaryView): void {
    this.#commit({ ...this.#state, conversationSecondaryView: view });
  }

  /** 更新对话页左栏的目录内搜索词（只影响左栏列表）。 */
  setConversationSidebarQuery(query: string): void {
    this.#commit({ ...this.#state, conversationSidebarQuery: query, conversationSidebarPageInput: "" });
  }

  /**
   * 切换左栏页码，每页条数由左栏模型统一定义。
   *
   * 它只改左栏呈现页，不派发任何命令、也不改中栏当前会话。
   */
  setConversationSidebarPage(pageInput: string): void {
    this.#commit({
      ...this.#state,
      conversationSidebarPageInput: pageInput,
    });
  }

  /**
   * 丢掉详情页的页码状态（页码复位成 `""` = 第一页，持有者清空）。
   *
   * 页码挂在 store 单例上、跨路由存活；不复位的话，在目录 A 翻到的页码会带进目录 B。
   * 页面在**进入某个目录详情页时**调用它（含再次进入同一个目录），把「进入即从第一页开始」
   * 这条约定落实；原型在进入目录与 `hashchange` 时同样把页码重置为 1。
   *
   * 它与 `directoryDetail` 的读取侧判定分工明确：读取侧保证「跨目录进入的首帧就是第一页」
   * （遗留页码在别的目录里直接不被采用），本方法保证「同目录再次进入也回到第一页」。
   *
   * `""` 不是「上一次的页」，而是一份没有输入的状态，由 `correctPageNumber` 得出 1。目录页的查询词与
   * 筛选不在复位范围内：它们是**目录页**的状态，返回目录页时要原样还在。
   */
  resetDirectoryDetailView(): void {
    this.#commit({
      ...this.#state,
      directoryDetailPage: "",
      directoryDetailDirectoryId: null,
    });
  }

  /** 更新配对页状态。 */
  setPairing(pairing: PairingPageModel): void {
    this.#commit({ ...this.#state, pairing });
  }

  /**
   * 配对页的入口（页面进页时调用一次）。
   *
   * 地址片段只可能是**敏感的设备二维码**或 node-link 二维码；组合根在返回之前已经清除
   * 设备 `q2=` / `data=` 片段（design D6）。本方法据此分流：
   *
   * - `sync` 且没有片段：进入配对码输入（原型 S0），不发任何请求；
   * - `sync` + `q2=`：固定 App 指引，不读载荷、不发请求；
   * - `sync` + `data=`：分类（只读协议常量，不解析设备字段）；node-link 载荷给「入口不匹配」，
   *   其余一律「旧格式已停用」；
   * - `node-link` 且没有片段：仍是「这个地址里没有节点配对信息」（该入口不提供配对码）；
   * - `node-link` + `q2=`：同上，固定 App 指引（设备二维码不属于节点入口）；
   * - `node-link` + `data=`：读 node-link 载荷并只读展示，或给「入口不匹配」。
   *
   * `kind` 是**路由**声明的期望类型（design D2）：载荷与它不一致时明确失败，绝不交给另一类流程。
   * 片段读取与清除都在组合根（地址栏是平台能力，`AGENTS.md` §6），本方法只消费它的结果。
   *
   * 会话进行中再次进入本方法（例如开发模式下 effect 重跑）时不覆盖当前状态：片段此时已被清除，
   * 若按「无载荷」重写，会把正在等待确认的页面误切回输入态。
   */
  async startPairingFromAddress(kind: PairingKind): Promise<void> {
    const wiring = this.#pairing;
    if (wiring === null) {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, {
          code: "internal.unavailable",
          message: "配对链路未装配：本页面拿不到配对传输。",
          retryable: true,
          details: {},
        }),
      });
      return;
    }
    if (this.#pairingSession !== null || this.#state.pairing.phase === "looking_up") return;
    // 组合根读取片段时已同步 `replaceState`：敏感内容在进入任何 UI 或请求之前就离开了地址栏。
    const text = wiring.takeQrTextFromFragment();
    if (text === null) {
      if (kind === "node-link") {
        this.#commit({ ...this.#state, pairing: applyNoPayload(this.#state.pairing, kind) });
        return;
      }
      const origin = canonicalOriginOf(wiring.currentOrigin()) ?? wiring.currentOrigin();
      this.#commit({
        ...this.#state,
        pairing: applyAwaitingCode(this.#state.pairing, kind, origin, wiring.isSecureContext()),
      });
      return;
    }
    // 新设备短二维码只供 ACP Remote App 使用：PWA 不解析、不取回、不认领。
    if (text.startsWith(DEVICE_QR_FRAGMENT_PREFIX)) {
      this.#commit({
        ...this.#state,
        pairing: applyDeviceQrAppGuide(this.#state.pairing, kind),
      });
      return;
    }
    // 其余被组合根识别并清除的片段只有旧设备 `data=`。
    if (kind === "sync") {
      // 只做**类型分类**（读协议常量），不解析设备字段；node-link 载荷仍给入口不匹配的专用文案。
      let read: PairingQrRead | null = null;
      try {
        read = wiring.transport.readQrPayload(text);
      } catch {
        read = null;
      }
      if (read?.kind === "node-link") {
        this.#commit({
          ...this.#state,
          pairing: applyPairingFailure(
            this.#state.pairing,
            kindMismatchFailure("sync", "node-link"),
            "kind_mismatch",
          ),
        });
        return;
      }
      this.#commit({
        ...this.#state,
        pairing: applyLegacyDeviceQrUnsupported(this.#state.pairing, kind),
      });
      return;
    }
    this.#beginNodeLinkPairing(text);
  }

  /**
   * 配对码入口的提交（原型 S0 → S1 → S2）：先做**本地**格式与安全上下文校验，再调用同源
   * lookup；成功后把响应拼成与配对资料同形的 `qr`，交给**同一条** claim/轮询/落盘链路。
   *
   * 失败面是固定的（design D6）：格式不符、lookup 的 HTTP/结构/网络失败、lookup 返回已过期，
   * 都落到「配对码不对或已过期，请重新生成配对码」。不区分后端原因、不回显 `message`、
   * 不自动重试，也不把码写进任何状态或存储。
   */
  async submitPairingCode(raw: string): Promise<void> {
    const wiring = this.#pairing;
    if (wiring === null) {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, {
          code: "internal.unavailable",
          message: "配对链路未装配：本页面拿不到配对传输。",
          retryable: true,
          details: {},
        }),
      });
      return;
    }
    // 只有输入态可以提交：lookup 在途（S1）与任何后续阶段都不接受第二份输入。
    if (this.#state.pairing.phase !== "awaiting_code" || this.#pairingSession !== null) return;

    // 安全上下文（design D6）：非安全上下文不发送 lookup；失败文案沿用 S11 的固定呈现。
    if (!wiring.isSecureContext()) {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, insecureContextFailure(), "insecure_context"),
      });
      return;
    }

    const code = normalizePairingCode(raw);
    if (code === null) {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, codeInvalidFailure()),
      });
      return;
    }

    const origin = canonicalOriginOf(wiring.currentOrigin());
    if (origin === null) {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, originMismatchFailure(), "origin_mismatch"),
      });
      return;
    }

    this.#commit({ ...this.#state, pairing: applyLookingUp(this.#state.pairing) });

    // 响应丢失（抛出）与业务失败都收敛成 `null`：成功 lookup 已消耗码，重试没有意义。
    let lookup: PairingLookupOutcome | null = null;
    try {
      lookup = await wiring.transport.lookup({ canonicalOrigin: origin, pairingCode: code });
    } catch {
      lookup = null;
    }
    // 输入提交后页面不再接受另一份码；这里仍按阶段收口，避免迟到的结果覆盖别的流程。
    // 显式拓宽类型：`#commit` 之后 TS 的旧窄化不会自动失效。
    const phase: string = this.#state.pairing.phase;
    if (phase !== "looking_up") return;
    if (lookup === null || lookup.kind !== "ok") {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, codeInvalidFailure()),
      });
      return;
    }

    const qr = lookup.qr;
    // lookup 是**同源**请求：响应里的来源必须仍是本页来源，否则就是另一台主机（design D5）。
    if (canonicalOriginOf(qr.canonicalOrigin) !== origin) {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, originMismatchFailure(), "origin_mismatch"),
      });
      return;
    }
    // 服务端时间优先：lookup 成功但码在响应到达时已过期 → 不 claim、不显示本地估算。
    const expiresAtMs = Date.parse(qr.expiresAt);
    if (Number.isNaN(expiresAtMs) || expiresAtMs <= this.#now()) {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, codeInvalidFailure()),
      });
      return;
    }

    // `expiresAtMs` 只用于这里的到期判定；会话内的绝对截止时间由 `applyClaiming` 与到期
    // 定时器从同一个 `qr.expiresAt` 复算，不另存第二份时间。

    await this.#startPairingSession(qr);
  }

  /**
   * node-link 地址片段 → 结构校验 → 安全上下文 → 类型校验 → 只读呈现。
   *
   * 任一校验不过都在这里收口，绝不发请求。设备（sync）载荷只被**分类**：它不会被解析成
   * 配对材料，也不会进入 claim（design D6）。
   */
  #beginNodeLinkPairing(text: string): void {
    const wiring = this.#pairing;
    if (wiring === null) return;

    // 路由声明了入口类型：本方法的每次收口（含读取失败）都带着它。过期呈现按入口分流
    // （`applyPairingFailure`），少了这一步，node-link 页面会被当成配对码链路。
    const pairing = applyPairingKind(this.#state.pairing, "node-link");

    let read: PairingQrRead;
    try {
      read = wiring.transport.readQrPayload(text);
    } catch (error) {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(pairing, publicErrorForQrInput(error)),
      });
      return;
    }

    // 环境事实（design D1）：读完载荷之后、发任何请求之前，先把「这一页根本不该配对」的
    // 前置条件收口。类型校验保持在最后（它需要已读出的载荷）。
    if (!wiring.isSecureContext()) {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(pairing, insecureContextFailure(), "insecure_context"),
      });
      return;
    }

    // 类型校验（design D2/D3）：载荷的类型由组合根在读取时标注，这里只把它与**路由声明的**类型
    // 对上——不猜载荷形状。设备载荷（旧设备 `data=`）喂给 node-link 入口时在这里停住：
    // 另一类流程拿不到它，也不会有任何请求发出去。
    if (read.kind !== "node-link") {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(
          pairing,
          kindMismatchFailure("node-link", "sync"),
          "kind_mismatch",
        ),
      });
      return;
    }

    // node-link 载荷：本客户端不是 node-link 配对的参与方（§13.2/§13.3 的 claim 与 status
    // 由节点发起），因此只把公开的对端信息呈现给入口，**不发任何请求**，也不开配对会话
    // （含 `pairingSecret` 的载荷在这里即弃，不进仓库的任何字段）。
    this.#commit({
      ...this.#state,
      pairing: applyScannedNodeLink(pairing, read.qr),
    });
  }

  /**
   * 配对码 lookup 成功后的会话起点：claim → 首次轮询。这是浏览器唯一的配对会话。
   *
   * 载荷从这一刻起由私有字段持有；`applyClaiming` 一次写入**公开**的对端信息与绝对截止时间。
   */
  async #startPairingSession(qr: PairingLookupResponse): Promise<void> {
    const wiring = this.#pairing;
    if (wiring === null) return;
    const session: PairingSession = { qr, cancel: null, expiryCancel: null };
    this.#pairingSession = session;
    this.#commit({ ...this.#state, pairing: applyClaiming(this.#state.pairing, qr) });

    // 到期定时器（design D3）：用载荷里的绝对 `expiresAt` 安排一次回调，归零即停止轮询并进入
    // 过期页。服务端仍是过期判定的权威；这里只保证页面不会在本地过期之后继续查询。
    const remaining = Math.max(0, Date.parse(qr.expiresAt) - this.#now());
    session.expiryCancel = wiring.schedule(remaining, () => {
      this.#expirePairing(qr);
    });

    // 传输层的两次 await 都必须在本方法里收口：端口约定用返回值表达业务失败，但**抛出**
    // （网络不可达、响应不是 JSON、设备身份绑在另一个来源）同样是真实结果。让它冒泡会留下
    // 一个非 null 的 `#pairingSession`：后续进入本方法会被提前 return、定时器也不会取消，
    // 页面里没有任何恢复路径。
    let claimError: PublicError | null;
    try {
      claimError = await wiring.transport.claim({ qr, canonicalOrigin: qr.canonicalOrigin });
    } catch (error) {
      // 到期定时器可能已经结束会话并提交了 `expired`（claim 在途时到期）：此时迟到的失败
      // 只是这次请求没能完成，不能把页面从 `expired` 覆盖成 `failed`——与 `#pollOnce` 的
      // catch 同一收口：会话已不在就保持现状。
      if (this.#pairingSession === null) return;
      this.#endPairing();
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, publicErrorForTransportFailure(error)),
      });
      return;
    }
    if (this.#pairingSession === null) return;
    if (claimError !== null) {
      this.#endPairing();
      this.#commit({ ...this.#state, pairing: applyPairingFailure(this.#state.pairing, claimError) });
      return;
    }
    this.#schedulePoll(qr);
  }

  /**
   * 到期回调：会话仍是同一次配对时结束它，并落到固定的「配对码不对或已过期」呈现（S6）。
   *
   * 状态迁移与秘密生命周期都属于仓库，页面只负责显示倒计时；因此即使页面标签被挂起、
   * 每秒刷新没有跑，这里也会按绝对时间把状态推进过去。
   */
  #expirePairing(qr: PairingLookupResponse): void {
    const session = this.#pairingSession;
    if (session === null || session.qr.pairingId !== qr.pairingId) return;
    this.#endPairing();
    this.#commit({
      ...this.#state,
      pairing: applyPairingFailure(this.#state.pairing, {
        code: "pairing.expired",
        message: PAIRING_ERROR_HINTS["pairing.expired"] ?? "配对码已过期：请在电脑上重新生成。",
        retryable: false,
        details: {},
      }),
    });
  }

  /** 安排下一次状态轮询；会话已被取代时不再安排。 */
  #schedulePoll(qr: PairingLookupResponse): void {
    const wiring = this.#pairing;
    const session = this.#pairingSession;
    if (wiring === null || session === null || session.qr.pairingId !== qr.pairingId) return;
    session.cancel = wiring.schedule(PAIRING_POLL_INTERVAL_MS, () => {
      void this.#pollOnce(qr);
    });
  }

  /**
   * 一次状态轮询：`pending_confirmation` 继续等，`approved` 收口成已配对，终态与错误停止轮询。
   *
   * `requestNonce` 每次轮询换新（§7.3）：随机源由组合根给出，feature 层不持有 CSPRNG。
   */
  async #pollOnce(qr: PairingLookupResponse): Promise<void> {
    const wiring = this.#pairing;
    const session = this.#pairingSession;
    if (wiring === null || session === null || session.qr.pairingId !== qr.pairingId) return;
    session.cancel = null;

    // 与 `claim` 同样必须收口：轮询抛出时留下「进行中」的状态会让页面一直停在转圈、
    // 也不会有下一次轮询。策略与 claim 一致——**失败即结束**（见 `#endPairing` 的注释）：
    // 页面因此拿到固定的「配对请求未能完成」，而不是把含秘密的载荷无限期留在内存里等一个
    // 可能永远不会好的网络。
    let result: PairingPollResult;
    try {
      result = await wiring.transport.poll({ qr, requestNonce: wiring.nextRequestNonce() });
    } catch (error) {
      if (this.#pairingSession === null || this.#pairingSession.qr.pairingId !== qr.pairingId) return;
      this.#endPairing();
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, publicErrorForTransportFailure(error)),
      });
      return;
    }
    if (this.#pairingSession === null || this.#pairingSession.qr.pairingId !== qr.pairingId) return;

    if (result.kind === "pending_confirmation") {
      this.#commit({ ...this.#state, pairing: applyPollResult(this.#state.pairing, result) });
      this.#schedulePoll(qr);
      return;
    }
    if (result.kind === "approved") {
      await this.#completePairing(qr, result);
      return;
    }
    // `terminal` / `error`：停止轮询并丢掉载荷——终态之后 secret 不再有用途（§7.3）。
    this.#endPairing();
    this.#commit({ ...this.#state, pairing: applyPollResult(this.#state.pairing, result) });
  }

  /**
   * 已批准 → 写入已配对主机记录 → 连接机推进为已配对 → 交给组合根装配并触发连接。
   *
   * 顺序是**先落盘再算已配对**：记录写不进去就没有可验证的主机信任根，此时留在 `failed`
   * 并说明原因，比「已配对但此后每次连接都进身份变化阻断态」可诊断得多（design D2）。
   * 主机身份与配对模型在同一次 commit 里写，避免渲染出「已配对但主机面板说未配对」的中间帧。
   */
  async #completePairing(
    qr: PairingLookupResponse,
    approved: Extract<PairingPollResult, { readonly kind: "approved" }>,
  ): Promise<void> {
    const wiring = this.#pairing;
    if (wiring === null) return;
    const record = pairedHostRecordFor(approved, qr.canonicalOrigin);
    try {
      await wiring.rememberPairedHost(record);
    } catch (error) {
      this.#endPairing();
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, pairedHostWriteFailure(error)),
      });
      return;
    }

    this.#endPairing();
    this.#commit({
      ...this.#state,
      pairing: applyPollResult(this.#state.pairing, approved),
      host: { address: qr.canonicalOrigin, hostId: approved.hostId, deviceId: approved.deviceId },
    });
    // 连接机要求 `unpaired --pair_started--> pairing --pair_succeeded--> disconnected`，
    // 之后组合根才被允许发起连接：跳过这两步，页面会一直停在「未配对」。
    this.pairSucceeded();
    // 收口交给组合根：装配连接并触发一次连接（MUST NOT 停在读地址阶段）。
    try {
      await wiring.onPaired(record);
    } catch (error) {
      // 记录已落盘、连接机已推进、页面已显示「已通过审批，正在进入目录」——收口失败只意味着
      // 这条连接没起来，因此**不**回退配对结果（那会与主机面板刚显示的主机身份自相矛盾），
      // 只把可读原因补上；异常也不冒泡成未处理拒绝（这里的调用方是
      // `void store.startPairingFromAddress(...)`）。原因走**收口专用**的映射，而不是
      // `claim` / `poll` 用的通用文案：这一页的呈现仍是「配对已完成」，通用文案的
      // 「配对请求未能完成」会与它正面对撞。恢复路径仍在：连接机此刻是 `disconnected`，
      // 组合根的退避重连会再试一次；页面照常自动进入目录。
      this.#commit({
        ...this.#state,
        pairing: applyPairingCloseoutFailure(this.#state.pairing, publicErrorForCloseoutFailure(error)),
      });
    }
  }

  /**
   * 结束这次配对：取消在途轮询与到期定时器，并丢掉含秘密的载荷（§7.3）。
   *
   * 传输层**拒绝**（`claim` / `poll` 抛出）时选择「失败即结束」而不是「重排一次轮询」：
   * 端口用返回值区分业务失败，抛出只说明「这次请求没能完成」，仓库据此分不出「偶发断网」
   * 与「确定性拒绝」；重排还要求一份重试预算，且会把含 `pairingSecret` 的载荷无限期留在
   * 内存里。结束会话反而保证页面立刻拿到固定的失败原因，而恢复路径是电脑重新生成地址。
   */
  #endPairing(): void {
    this.#pairingSession?.cancel?.();
    this.#pairingSession?.expiryCancel?.();
    this.#pairingSession = null;
  }

  /**
   * 记录已配对的主机身份。
   *
   * 它是**状态**而不是构造参数：组合根先装配 store、再异步读 IndexedDB 里的设备身份与配对
   * 记录，主机身份因此在 store 构造之后才变得可用；构造期定格会让主机面板在真实运行时永远
   * 显示「未配对」。未配对时保持 `null`——不编造地址与指纹（R19）。
   */
  setHostIdentity(host: HostIdentityModel): void {
    this.#commit({ ...this.#state, host });
    // 页面可能先于主机身份挂载：定位恢复需要 origin/节点/设备三项，到这里补读一次。
    if (this.#tabRestoreRequested) void this.restoreTabPreparation();
  }

  /**
   * 配对完成（由配对用例在写入已配对主机记录后调用）。
   *
   * 连接机要求 `unpaired --pair_started--> pairing --pair_succeeded--> disconnected`，
   * 之后才允许发起连接；跳过这两步的话，后续的 `connect_started` 会被迁移表拒绝，
   * 页面就会一直停在「未配对」。阻断态（被撤销 / 身份变化 / 版本不兼容）也接受
   * `pair_started`，因此重新配对能从这些状态里出来。
   */
  pairSucceeded(): void {
    const pairing = applyConnectionEvent(this.#state.connection, { kind: "pair_started" });
    this.#commit({ ...this.#state, connection: applyConnectionEvent(pairing, { kind: "pair_succeeded" }) });
  }

  /**
   * 记录一次「门面拒绝连接」的阻断：组合根尝试连接但 `SyncClient.connect()` 抛出时调用。
   *
   * 与 {@link apply} 分开：`apply` 消费的是连接层的**事件流**（每条 `blocked` 都带关闭码），
   * 而组合根从 `SyncClient.blocked` 拿到的只有**原因**——它不该编一个关闭码来凑形状。
   * 参数就是连接机的 `BlockingCause`，阻断说明（原因 + 下一步）由连接机唯一给出。
   *
   * 适用范围与连接机一致：这条事件只接得住「允许发起连接」的 `disconnected` / `reconnecting`
   * （`shouldConnect` 的白名单），其余状态由迁移表拒绝、状态保持不变（与其它迟到事件同一口径）。
   */
  noteConnectionBlocked(cause: BlockingCause): void {
    this.#commit(this.#withConnection({ kind: "blocked", cause }));
  }

  /** 用户主动处理结果不确定——**唯一**能把它移出列表的动作。 */
  dismissUncertain(requestId: Uuid): void {
    this.#commit({
      ...this.#state,
      uncertainRequestIds: this.#state.uncertainRequestIds.filter((id) => id !== requestId),
    });
  }

  /**
   * 提交创建会话。
   *
   * 四道闸门，任一不过即**不发命令**：
   * 1. 已建立连接（命令信封需要 `connectionId`）；
   * 2. Agent 必须在本机已配置的目录里（快照 `agents`），不是「用户输入什么就发什么」；
   * 3. 目录引用必须能定下来（入口目录标识、选定结果，或表单带回来的路径，三者都不为空）；
   * 4. 载荷恰好是 `agentId` 加一个目录引用（加可选的 `createIfMissing`，由 `assertCreateSessionRefs` 把关）。
   *
   * 目录**不再有权限门控**（设备权限层已删除）：入口与提交都不看任何 scope。
   * `directoryPath` 是表单带回来的那一份（顶部入口选定/新建出来的路径，逐字是用户看到的那一份）；
   * 目录标识不跟着表单走，而是从本仓库的浏览器状态里读——顶部入口在回退列表里选中的那一项就是它。
   * 两者都换算成 `session.create` 的两个合法形状之一，含 `createIfMissing` 的那一支只跟路径搭配。
   *
   * `createRequestIds` 一律以**弹层的机器键**为键（已记录目录用目录标识，顶部入口用
   * `CREATE_SHEET_BROWSE_KEY`）：用户改目录不改弹层，因此提交状态不会与界面失联。
   */
  submitCreateSession(input: {
    readonly directoryId: string | null;
    readonly agentId: string;
    readonly directoryPath?: string;
    readonly createIfMissing?: boolean;
  }): CreateOutcome {
    const connectionId = this.#identity.connectionId;
    if (connectionId === null) return NOT_CONNECTED;

    const entry = this.createEntry(input.directoryId);
    if (!entry.agents.some((agent) => agent.id === input.agentId)) {
      return { ok: false, reason: "所选 Agent 不在本机已配置的 Agent 目录里" };
    }
    const path = input.directoryPath ?? "";
    const createIfMissing = input.createIfMissing === true;
    const selected = this.#state.directoryBrowse.selection;
    const registeredDirectoryId =
      input.directoryId ?? (selected !== null && selected.kind === "registered" ? selected.directoryId : null);
    let refs: CreateSessionRefs;
    if (path.length > 0) {
      refs = createIfMissing
        ? { workspacePath: path, agentId: input.agentId, createIfMissing: true }
        : { workspacePath: path, agentId: input.agentId };
    } else {
      if (registeredDirectoryId === null) {
        return { ok: false, reason: "还没有选定目录：先浏览并选中一层，或改用已记录的目录。" };
      }
      if (createIfMissing) {
        return { ok: false, reason: "已记录的目录必然存在，不能对它启用「不存在则先创建」。" };
      }
      refs = { directoryId: registeredDirectoryId, agentId: input.agentId };
    }
    assertCreateSessionRefs(refs);

    const requestId = this.#ids.uuid();
    const commands = new Map(this.#state.commands);
    commands.set(requestId, createCommand({ requestId, command: "session.create" }));
    const createRequestIds = new Map(this.#state.createRequestIds);
    createRequestIds.set(input.directoryId ?? CREATE_SHEET_BROWSE_KEY, requestId);
    this.#commit({ ...this.#state, commands, createRequestIds });

    this.#sync.dispatch(
      buildCreateSessionCommand({
        refs,
        requestId,
        messageId: this.#ids.uuid(),
        connectionId,
      }),
    );
    return { ok: true, requestId };
  }

  // ── 会话准备（`core.session-preparation.v1`，WP7）────────────────────────────
  //
  // 准备是首条提交前的临时阶段：Owner 分配最终 `sessionId` 并签发凭据；没有正式会话行，
  // 页面始终停在 `/session/<id>`。正式受理之后同一项仍保留（承载请求关联与阶段），
  // 但页面改读正式会话。草稿只在用户点击发送后才上传。

  /**
   * 新建入口的可用性。
   *
   * 未连上主机时本机不知道服务端支持什么（`selectedFeatures` 为空只是「还没协商」），
   * 因此只说「还没连上」；已 `online` 却没选中准备 feature 才是「服务端未提供」。
   */
  preparationEntryAvailability(): { readonly available: boolean; readonly reason: string | null } {
    if (this.#preparationTabNonce === null) {
      return { available: false, reason: PREPARATION_NO_TAB_NONCE_NOTICE };
    }
    if (this.#state.connection.state !== "online") {
      return { available: false, reason: "还没连上主机" };
    }
    if (!this.#state.selectedFeatures.includes(SESSION_PREPARATION_FEATURE)) {
      return { available: false, reason: PREPARATION_UNSUPPORTED_NOTICE };
    }
    return { available: true, reason: null };
  }

  /**
   * 准备页模型；不是准备会话时 `null`。
   *
   * `@param nowMs` 是租约过期的判定基准（缺省读本仓库注入的时钟）。
   */
  preparationPage(sessionId: Uuid, nowMs?: number): PreparationPageModel | null {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined) return null;
    // 会话已进入权威快照：首条已转正，正式会话视图优先；准备页不得继续遮蔽它。
    if (this.#findSession(sessionId) !== undefined) return null;
    const record = this.#state.commands.get(session.association?.requestId ?? "");
    const commandState =
      record === undefined || record.state === "draft" || record.state === "submitting"
        ? session.phase === "ready"
          ? null
          : "submitting"
        : record.state === "accepted" || record.state === "completed"
          ? "accepted"
          : record.state === "failed" || record.state === "rejected"
            ? "failed"
            : record.state === "uncertain"
              ? "uncertain"
              : null;
    const leaseExpiresAtMs =
      session.leaseExpiresAt === null ? null : Date.parse(session.leaseExpiresAt);
    const leaseExpired =
      session.phase === "ready" &&
      leaseExpiresAtMs !== null &&
      Number.isFinite(leaseExpiresAtMs) &&
      leaseExpiresAtMs <= (nowMs ?? this.#now());
    return buildPreparationPageModel({
      session,
      text: this.#state.drafts.get(sessionId) ?? "",
      attachments: this.#state.attachments.get(sessionId) ?? [],
      connected: this.#state.connection.state === "online",
      attachmentsSupported:
        this.#state.selectedFeatures.includes(ATTACHMENT_FEATURE) &&
        this.#attachmentMedia !== null &&
        this.#attachmentTransfer !== null,
      commandState,
      leaseExpired,
    });
  }

  /**
   * 普通新建入口的**统一判定**（目录页、目录详情页、会话左栏共用）。
   *
   * 同一标签页在该节点/设备下至多一个未转正的临时会话：已有槽位就只导航回它，分配在途就
   * 等同一个结果。只有 `none` 才允许页面打开创建弹层并发 `allocate`。
   *
   * 只承载说明的条目（复制标签页的「另一个标签页正在使用这份准备」）不是槽位：提示照常渲染，
   * 但本标签页的普通新建入口仍然放行（合同 §7）。
   */
  preparationSlotRoute(): PreparationSlotRoute {
    if (this.#pendingSlotAllocation !== null) {
      return { kind: "allocating", outcome: this.#pendingSlotAllocation };
    }
    const slot = this.#activePreparation();
    return slot === null
      ? { kind: "none" }
      : { kind: "session", sessionId: slot.sessionId };
  }

  /** 当前标签页唯一未转正临时会话的 ID；没有则为 `null`。 */
  activePreparationSessionId(): Uuid | null {
    return this.#activePreparation()?.sessionId ?? null;
  }

  /**
   * 从目录/详情页发起 prepare → allocate（`session.preparation` 的 `allocate`）。
   *
   * 只提交 Agent、目录与标签页归属：正式 `sessionId` 由 Owner 分配，客户端不指定。
   * 成功时页面立即导航到 `/session/<id>`（准备模式），**不**新增任何目录会话行。
   *
   * 已有未转正槽位时**不**再分配：无论入口属于哪个目录、选了什么 Agent、槽位是否 ready，
   * 都返回现有 ID；换 Agent 只能走 {@link changePreparationAgent}，其他目录不得原位改绑。
   */
  async beginPreparation(input: {
    readonly directoryId: string | null;
    readonly agentId: string;
    readonly directoryPath?: string;
  }): Promise<PreparationBeginOutcome> {
    const existing = this.#activePreparation();
    if (existing !== null) return { ok: true, sessionId: existing.sessionId };
    if (this.#pendingSlotAllocation !== null) return this.#pendingSlotAllocation;
    // 刷新后定位恢复在途：先等它落地，避免恢复中的槽位被当成「没有」再分配一个。
    if (this.#tabRestoreInFlight !== null) await this.#tabRestoreInFlight;
    // 等待期间另一个入口可能已经发出了分配：闸门必须在 await 后复查，否则两帧 `allocate`。
    if (this.#pendingSlotAllocation !== null) return this.#pendingSlotAllocation;
    const restored = this.#activePreparation();
    if (restored !== null) return { ok: true, sessionId: restored.sessionId };

    const availability = this.preparationEntryAvailability();
    if (!availability.available) {
      return { ok: false, reason: availability.reason ?? "准备不可用" };
    }
    if (this.#identity.connectionId === null) {
      return { ok: false, reason: "还没连上主机" };
    }
    const entry = this.createEntry(input.directoryId);
    if (!entry.agents.some((agent) => agent.id === input.agentId)) {
      return Promise.resolve({ ok: false, reason: "所选 Agent 不在本机已配置的 Agent 目录里" });
    }
    const path = input.directoryPath ?? "";
    const selected = this.#state.directoryBrowse.selection;
    const registeredDirectoryId =
      input.directoryId ?? (selected !== null && selected.kind === "registered" ? selected.directoryId : null);
    let workspaceInput: PreparationWorkspaceInput;
    let workspace: PreparationWorkspace;
    if (path.length > 0) {
      workspaceInput = { workspacePath: path };
      workspace = { directoryId: registeredDirectoryId, path, source: "workspacePath" };
    } else if (registeredDirectoryId !== null) {
      const item = this.#state.resources?.workspaces.find(
        (candidate) => candidate.directoryId === registeredDirectoryId,
      );
      if (item === undefined) {
        return Promise.resolve({ ok: false, reason: "目录标识不在最近一次快照里，先刷新目录" });
      }
      workspaceInput = { directoryId: registeredDirectoryId };
      workspace = { directoryId: registeredDirectoryId, path: item.path, source: "directoryId" };
    } else {
      return Promise.resolve({
        ok: false,
        reason: "还没有选定目录：先浏览并选中一层，或改用已记录的目录。",
      });
    }

    const tabNonceSource = this.#preparationTabNonce;
    if (tabNonceSource === null) {
      return { ok: false, reason: PREPARATION_NO_TAB_NONCE_NOTICE };
    }
    return this.#allocatePreparation({
      operationId: this.#ids.uuid(),
      agentId: input.agentId,
      workspace,
      workspaceInput,
      tabNonce: tabNonceSource(),
    });
  }

  /**
   * 发出唯一槽位的 `allocate`，并把 Promise 记成在途闸门。
   *
   * 分配结果由 `#withPreparationResult` 按 `operationId` 收口；连接断开/重建时
   * `#dropPendingSlotAllocations` 结束在途 Promise，避免它永久挡住后续新建。
   */
  #allocatePreparation(input: {
    readonly operationId: Uuid;
    readonly agentId: string;
    readonly workspace: PreparationWorkspace;
    readonly workspaceInput: PreparationWorkspaceInput;
    readonly tabNonce: string;
  }): Promise<PreparationBeginOutcome> {
    let resolveOutcome!: (outcome: PreparationBeginOutcome) => void;
    const promise = new Promise<PreparationBeginOutcome>((resolve) => {
      resolveOutcome = resolve;
    });
    let settled = false;
    const settle = (outcome: PreparationBeginOutcome): void => {
      if (settled) return;
      settled = true;
      if (this.#pendingSlotAllocation === promise) this.#pendingSlotAllocation = null;
      resolveOutcome(outcome);
    };
    this.#pendingAllocations.set(input.operationId, {
      resolve: settle,
      agentId: input.agentId,
      workspace: input.workspace,
      tabNonce: input.tabNonce,
    });
    this.#pendingSlotAllocation = promise;
    const sent = this.#sync.sendPreparation(
      buildAllocateBody({
        operationId: input.operationId,
        agentId: input.agentId,
        workspace: input.workspaceInput,
        tabNonce: input.tabNonce,
      }),
    );
    if (!sent) {
      this.#pendingAllocations.delete(input.operationId);
      settle({ ok: false, reason: "命令通道尚未就绪，请稍后重试" });
    }
    return promise;
  }

  /** 唯一未转正临时会话（`accepted`/`committed`/`released` 已退出槽位角色）。 */
  #activePreparation(state: ClientState = this.#state): PreparationSessionState | null {
    for (const session of state.preparations.values()) {
      if (this.#occupiesPreparationSlot(session)) return session;
    }
    return null;
  }

  /**
   * 条目是否占着唯一槽位。
   *
   * `accepted`/`committed`/`released` 已退出槽位角色。**只承载说明的条目**（复制标签页得到的
   * 「另一个标签页正在使用这份准备」）没有凭据、没有关联请求、也没有 Agent：本标签页从未持有
   * 它的任何资源，它只用于渲染提示。把它算作槽位会让复制标签页的普通新建入口全被劫持到原会话
   * （合同 §7「复制标签页与其他身份不得接管该槽位」），因此这里同样排除。
   */
  #occupiesPreparationSlot(session: PreparationSessionState): boolean {
    if (session.phase === "accepted" || session.phase === "committed" || session.phase === "released") {
      return false;
    }
    // 权威快照里已有这个会话 = 首条已转正：准备角色退出（刷新后不能把正式会话再当成未转正新会话）。
    if (this.#findSession(session.sessionId) !== undefined) return false;
    return !(session.receipt === null && session.association === null && session.agentId.length === 0);
  }

  /** 连接失效后结束在途分配：旧连接上的应答不会再来，闸门不能永久挡住新建。 */
  #dropPendingSlotAllocations(reason: string): void {
    if (this.#pendingAllocations.size === 0) {
      this.#pendingSlotAllocation = null;
      return;
    }
    const pending = [...this.#pendingAllocations.values()];
    this.#pendingAllocations.clear();
    this.#pendingSlotAllocation = null;
    for (const item of pending) item.resolve({ ok: false, reason });
  }

  /**
   * 打开发送前已存在的准备（刷新 / 深链 / 重连）。
   *
   * 只有**本标签页**的合法草稿才能恢复：没有草稿就不是准备会话，页面随后按正式会话处理
   * （未知 URL 只在权威快照确认不存在后回目录）。此方法是幂等的——已有状态直接返回 `true`。
   *
   * @returns 本页当前是否有准备（含正在恢复 / 被另一标签页占用的明确不可用态）。`false` 表示
   * 没有可恢复的草稿，调用方可以开始正式读取——正式读取**只**在没有准备时派发。
   */
  async openPreparationSession(sessionId: Uuid): Promise<boolean> {
    // 会话已在权威快照里：首条已转正，正式读取优先；准备说明/条目不得遮蔽它。
    if (this.#findSession(sessionId) !== undefined) return false;
    if (this.#state.preparations.has(sessionId)) return true;
    const drafts = this.#preparationDrafts;
    if (drafts === null) return false;
    // 页面可能同时发起启动恢复：等它落地，避免同一会话两条并发恢复路径。
    if (this.#tabRestoreInFlight !== null) await this.#tabRestoreInFlight;
    if (this.#state.preparations.has(sessionId)) return true;
    if (this.#preparationOpening.has(sessionId)) return this.#state.preparations.has(sessionId);
    const locator = await this.#readTabLocator();
    // 没有本标签页定位就不是可恢复的准备：陌生 URL 不发起任何准备，也不写定位。
    if (locator === null) return false;
    // 本标签页的槽位在别的会话上（例如 A→B 后刷新 B）：只恢复那个槽位，绝不把它改绑到当前 URL，
    // 也不给当前 URL 造一个准备。
    if (locator.sessionId !== sessionId) {
      await this.restoreTabPreparation();
      return this.#state.preparations.has(sessionId);
    }
    if (!this.#locatorMatchesIdentity(locator)) return false;
    const scope = this.#preparationScope(sessionId);
    if (scope === null) return false;
    this.#preparationOpening.add(sessionId);
    try {
      await this.#restoreFromDraft(sessionId, scope);
    } finally {
      this.#preparationOpening.delete(sessionId);
    }
    return this.#state.preparations.has(sessionId);
  }

  /** 当前标签页是否仍在尝试恢复这个会话的准备（未知 URL 的跳转要等它落地）。 */
  preparationRestoring(sessionId: Uuid): boolean {
    return this.#preparationOpening.has(sessionId);
  }

  /**
   * 启动时按本标签页的非敏感定位恢复唯一准备槽位（目录页与正式会话页都可以调用）。
   *
   * 只读 sessionStorage 定位与 `PreparationDraftPort`；不分配新 ID、不改当前 URL、不接管
   * 别的标签页。幂等：已有槽位直接返回；主机身份/认证尚未就绪时返回 `null`，并在
   * `setHostIdentity`/连接事件后由 `#tabRestoreRequested` 自动补一次。
   */
  async restoreTabPreparation(): Promise<Uuid | null> {
    this.#tabRestoreRequested = true;
    const drafts = this.#preparationDrafts;
    if (drafts === null) return null;
    const existing = this.#activePreparation();
    if (existing !== null) return existing.sessionId;
    if (this.#tabRestoreInFlight !== null) return this.#tabRestoreInFlight;
    const task = this.#restoreTabSlot();
    this.#tabRestoreInFlight = task;
    try {
      return await task;
    } finally {
      if (this.#tabRestoreInFlight === task) this.#tabRestoreInFlight = null;
    }
  }

  async #restoreTabSlot(): Promise<Uuid | null> {
    const locator = await this.#readTabLocator();
    if (locator === null) return null;
    // 已经处理过这个定位：只有真正占槽位的条目才算恢复成功；转正式或只承载占用说明时不重复读草稿。
    const known = this.#state.preparations.get(locator.sessionId);
    if (known !== undefined) return this.#occupiesPreparationSlot(known) ? known.sessionId : null;
    if (!this.#locatorMatchesIdentity(locator)) return null;
    const scope = this.#preparationScope(locator.sessionId);
    if (scope === null) return null;
    if (this.#preparationOpening.has(locator.sessionId)) {
      const opening = this.#state.preparations.get(locator.sessionId);
      return opening !== undefined && this.#occupiesPreparationSlot(opening) ? locator.sessionId : null;
    }
    this.#preparationOpening.add(locator.sessionId);
    try {
      await this.#restoreFromDraft(locator.sessionId, scope);
    } finally {
      this.#preparationOpening.delete(locator.sessionId);
    }
    const restored = this.#state.preparations.get(locator.sessionId);
    return restored !== undefined && this.#occupiesPreparationSlot(restored) ? locator.sessionId : null;
  }

  async #readTabLocator(): Promise<PreparationTabLocatorSnapshot | null> {
    const drafts = this.#preparationDrafts;
    if (drafts === null) return null;
    try {
      return await drafts.readTabLocator();
    } catch {
      return null;
    }
  }

  /** 定位必须属于当前 canonical origin、本机节点与设备；别的主机/设备的残留不恢复。 */
  #locatorMatchesIdentity(locator: PreparationTabLocatorSnapshot): boolean {
    const host = this.#state.host;
    const deviceId = this.#state.deviceId;
    if (host === null || deviceId === null) return false;
    return locator.canonicalOrigin === host.address &&
      locator.ownerNodeId === (host.hostId ?? deviceId) &&
      locator.deviceId === deviceId;
  }

  /** 更新准备草稿正文（只在内存；落草稿合并成一次异步写）。 */
  setPreparationDraft(sessionId: Uuid, text: string): void {
    if (!this.#state.preparations.has(sessionId)) return;
    if (this.#state.drafts.get(sessionId) === text) return;
    this.#commit({ ...this.#state, drafts: withEntry(this.#state.drafts, sessionId, text) });
    this.#schedulePreparationSave(sessionId);
  }

  /** 选择模式：立即把选择交给临时端点（`configure`），以 Agent 确认结果为准。 */
  selectPreparationMode(sessionId: Uuid, modeId: string): CommandOutcome {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined) return { ok: false, reason: "这不是准备会话" };
    if (session.phase !== "ready" && session.phase !== "failed") {
      return { ok: false, reason: PREPARATION_PHASE_LABELS[session.phase] };
    }
    this.#updatePreparation(sessionId, (current) => ({ ...current, selectedMode: modeId }));
    const outcome = this.#sendConfigure(sessionId, { mode: modeId });
    this.#schedulePreparationSave(sessionId);
    return outcome;
  }

  /** 设置一个配置项：同样以 Agent 确认结果为准；不可用的选择保留并阻止发送。 */
  setPreparationConfigValue(sessionId: Uuid, configId: string, value: string | boolean): CommandOutcome {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined) return { ok: false, reason: "这不是准备会话" };
    if (session.phase !== "ready" && session.phase !== "failed") {
      return { ok: false, reason: PREPARATION_PHASE_LABELS[session.phase] };
    }
    this.#updatePreparation(sessionId, (current) => ({
      ...current,
      selectedConfig: { ...current.selectedConfig, [configId]: value },
    }));
    const outcome = this.#sendConfigure(sessionId, { configOptions: { [configId]: value } });
    this.#schedulePreparationSave(sessionId);
    return outcome;
  }

  /**
   * 提交首条：先上传附件（准备目标），再发**一次** `session.start` 的 `initial`。
   *
   * 一个在途尝试一个 `requestId`；双击不会产生第二次执行（`#preparationSubmissions` 闸门）。
   * 成功与失败都停在同一 URL/同一页面实例：失败保留完整草稿，受理后改读正式会话。
   *
   * 失败后的分支是闭合的（D8/D8.1）：
   * - 本地失败（附件/保存从未派发）→ 同一 `requestId` 重试 `initial`；
   * - 服务端报告的 `failed` → 用**新的** `requestId`、`firstSubmissionId` 与 `retryOfRequestId`
   *   发 `kind:"continue"`；首次提交身份未知时先核实原请求，绝不用旧 `requestId` 再发 `initial`；
   * - `uncertain` 不可发送，只提供「核实原请求」。
   */
  submitPreparation(sessionId: Uuid, text: string): CommandOutcome {
    const page = this.preparationPage(sessionId);
    if (page === null) return { ok: false, reason: "这不是准备会话" };
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined || session.receipt === null || session.runtimeEpoch === null || session.generation === null) {
      return { ok: false, reason: "准备资源尚未就绪，请稍后重试" };
    }
    if (session.phase === "failed" && session.failureOrigin === "server_start") {
      return this.#continueAfterFailure(sessionId, session);
    }
    if (!page.input.canSend) return { ok: false, reason: page.input.reason ?? "当前不能发送" };
    if (this.#preparationSubmissions.has(sessionId)) {
      return { ok: false, reason: "首条消息正在保存，等待这一批结束" };
    }
    if (text.length > 0 && this.#state.drafts.get(sessionId) !== text) {
      this.#commit({ ...this.#state, drafts: withEntry(this.#state.drafts, sessionId, text) });
    }
    // 本地上传失败从未派发：同一 `requestId` 重试是安全的（服务端从未见过它）。
    const requestId = session.association?.requestId ?? this.#ids.uuid();
    const attachments = this.#state.attachments.get(sessionId) ?? [];
    const attachmentNames = attachments.map((attachment) => attachment.fileName);
    this.#preparationRequestSessions.set(requestId, sessionId);
    this.#preparationSubmissions.add(sessionId);
    this.#updatePreparation(sessionId, (current) => ({
      ...current,
      phase: "submitting",
      error: null,
      notice: null,
      failureOrigin: null,
      canContinue: false,
      association: { requestId, firstSubmissionId: null, firstRequestId: null, currentRequestId: null, firstMessageId: null },
    }));
    // 带附件时先展示本次输入与「上传中」：Agent 此时尚未收到任何内容。没有附件时命令在同一
    // 次提交里同步派出，由 `#dispatchPrompt` 的派发补丁登记为「已提交」。
    this.#dropUnsettledPrompts(sessionId);
    if (attachments.length > 0) {
      this.#commit({
        ...this.#state,
        sentPrompts: withEntry(this.#state.sentPrompts, requestId, sentPromptOf({
          requestId,
          sessionId,
          text,
          attachmentNames,
          createdAt: new Date(this.#now()).toISOString(),
          phase: "uploading",
          // 准备关联已知的首条正文标识（刷新恢复的 committed 状态带回）：有了它就不靠位置猜。
          messageId: session.association?.firstMessageId ?? null,
        })),
      });
    }
    this.#cancelPreparationRenew(sessionId);
    this.#schedulePreparationSave(sessionId);
    void this.#uploadAndStart({ sessionId, requestId, text });
    return { ok: true, requestId };
  }

  /**
   * 服务端已报告首条失败后的显式继续。
   *
   * `kind:"continue"` 必须带首次提交身份：还不知道时先按原 `requestId` 查 `command.status`
   * （投影会带回 `firstSubmissionId` 与 `canContinue`），下一次显式点击再继续——**不**用旧
   * `requestId` 重发 `initial`（那只会重放原失败终态）。
   */
  #continueAfterFailure(sessionId: Uuid, session: PreparationSessionState): CommandOutcome {
    if (this.#preparationSubmissions.has(sessionId)) {
      return { ok: false, reason: "首条消息正在保存，等待这一批结束" };
    }
    const original = session.association?.requestId ?? null;
    if (original === null) {
      return { ok: false, reason: "没有可继续的原请求；请离开后重新准备" };
    }
    const firstSubmissionId = session.association?.firstSubmissionId ?? null;
    if (firstSubmissionId === null || !session.canContinue) {
      const outcome = this.#queryPreparationRequest(sessionId, original);
      if (!outcome.ok) return outcome;
      this.#updatePreparation(sessionId, (current) => ({
        ...current,
        notice: "正在核实原请求；服务端确认可继续后再点一次发送（不会重发旧请求）。",
      }));
      return { ok: true, requestId: outcome.requestId };
    }
    const requestId = this.#ids.uuid();
    this.#preparationRequestSessions.set(requestId, sessionId);
    this.#preparationSubmissions.add(sessionId);
    this.#updatePreparation(sessionId, (current) => ({
      ...current,
      phase: "submitting",
      error: null,
      notice: null,
      failureOrigin: null,
      canContinue: false,
      association: {
        requestId,
        firstSubmissionId,
        firstRequestId: current.association?.firstRequestId ?? null,
        currentRequestId: requestId,
        firstMessageId: current.association?.firstMessageId ?? null,
      },
    }));
    this.#cancelPreparationRenew(sessionId);
    this.#schedulePreparationSave(sessionId);
    try {
      const dispatched = this.#dispatchIntent({
        sessionId,
        requestId,
        build: (id) =>
          buildContinueStartBody({
            requestId: id,
            sessionId,
            firstSubmissionId,
            retryOfRequestId: original,
            mode: session.selectedMode,
            configOptions: session.selectedConfig,
          }),
        patch: (id) => ({
          sentPrompts: withEntry(this.#state.sentPrompts, id, sentPromptOf({
            requestId: id,
            sessionId,
            text: this.#state.drafts.get(sessionId) ?? "",
            attachmentNames: (this.#state.attachments.get(sessionId) ?? []).map((attachment) => attachment.fileName),
            createdAt: new Date(this.#now()).toISOString(),
            phase: "submitted",
            // 续发的首条沿用准备关联的首条正文标识：正式消息到达时按它原位接替。
            messageId: session.association?.firstMessageId ?? null,
          })),
        }),
      });
      if (dispatched === null) {
        this.#updatePreparation(sessionId, (current) => ({
          ...current,
          phase: "failed",
          failureOrigin: "server_start",
          error: { code: "internal.unavailable", message: "命令通道尚未就绪", retryable: true, details: {} },
        }));
        return { ok: false, reason: "命令通道尚未就绪" };
      }
      return { ok: true, requestId };
    } finally {
      this.#preparationSubmissions.delete(sessionId);
    }
  }

  /**
   * 首条失败后的显式核实：先查原请求，绝不自动重派。
   *
   * 查询结果会带回 `firstSubmissionId` / `canContinue`：可继续时「发送首条」改走
   * `kind:"continue"`（新请求 ID）；`uncertain` 仍然只查不发。
   */
  retryPreparation(sessionId: Uuid): CommandOutcome {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined) return { ok: false, reason: "这不是准备会话" };
    const original = session.association?.requestId ?? null;
    if (original !== null) {
      const outcome = this.#queryPreparationRequest(sessionId, original);
      if (!outcome.ok) return outcome;
      return { ok: true, requestId: original };
    }
    const page = this.preparationPage(sessionId);
    if (page === null || !page.input.canSend) {
      return { ok: false, reason: page?.input.reason ?? "当前不能发送" };
    }
    return this.submitPreparation(sessionId, this.#state.drafts.get(sessionId) ?? "");
  }

  /** 更换 Agent：释放旧临时资源、作废旧草稿，再由后端分配新 ID 与新默认设置。 */
  async changePreparationAgent(sessionId: Uuid, agentId: string): Promise<PreparationBeginOutcome> {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined) return { ok: false, reason: "这不是准备会话" };
    if (session.agentId === agentId) return { ok: true, sessionId };
    if (this.#preparationBusy.has(sessionId)) return { ok: false, reason: "正在更换 Agent，等待这一次结束" };
    this.#preparationBusy.add(sessionId);
    try {
      const tabNonce = session.tabNonce;
      await this.#discardPreparation(sessionId, "agent_changed");
      return await this.#allocateReplacement({ agentId, workspace: session.workspace, tabNonce });
    } finally {
      this.#preparationBusy.delete(sessionId);
    }
  }

  /**
   * 旧版页面动作所依赖的槽位判定。当前新会话页不展示放弃动作；保留该方法只为旧调用方和
   * 资源清理回归，不能再把它接回用户界面。
   */
  canAbandonPreparation(sessionId: Uuid): boolean {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined) return false;
    // 与普通新建入口同一判据：只承载说明的条目（复制标签页的提示）不占槽，本标签页从未持有
    // 它的资源，因此也不能 cleanup 它——否则会顺手清掉本标签页自己槽位的定位（合同 §7）。
    return this.#occupiesPreparationSlot(session);
  }

  /**
   * 旧版新会话页使用的槽位清理原语：作废草稿、请求释放临时资源、清本标签页定位与槽位。
   *
   * 当前界面不再提供这个动作；普通导航、刷新和目录入口都不会调用它。
   */
  async abandonPreparation(sessionId: Uuid): Promise<void> {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined || !this.canAbandonPreparation(sessionId)) return;
    const drafts = this.#preparationDrafts;
    const held = this.#preparationDraftAccess.get(sessionId);
    if (drafts !== null) {
      if (held !== undefined) {
        try {
          await drafts.invalidateDraft({
            access: held.access,
            expectedRevision: held.revision,
            reason: "abandoned",
            nowMs: this.#now(),
          });
        } catch {
          // 草稿已不可写：仍继续释放资源并退出槽位，不让清理停在这里。
        }
      }
      try {
        await drafts.clearTabLocator();
      } catch {
        // 定位清理失败不阻塞；`#forgetPreparation` 后本页不再有任何入口指向旧槽位。
      }
      if (held !== undefined) {
        try {
          await drafts.releaseTab({ access: held.access, nowMs: this.#now() });
        } catch {
          // 归属释放失败由孤儿清理收口。
        }
      }
    }
    // 上面的 await 可能让本会话的 `restore` 结果落地并写入资源代：release 必须用最新快照，
    // 否则旧快照的 `runtimeEpoch`/`generation` 为 `null`，释放帧会被静默跳过。
    const current = this.#state.preparations.get(sessionId);
    if (current !== undefined) this.#releasePreparationResource(current);
    this.#forgetPreparation(sessionId);
  }

  /** 发出一次 `release`（凭据/资源代齐全时）；不修改槽位状态。 */
  #releasePreparationResource(session: PreparationSessionState): void {
    if (session.receipt === null || session.runtimeEpoch === null || session.generation === null) return;
    this.#sendPreparationFrame(
      session.sessionId,
      "release",
      buildReleaseBody({
        operationId: this.#ids.uuid(),
        sessionId: session.sessionId,
        receipt: session.receipt,
        runtimeEpoch: session.runtimeEpoch,
        generation: session.generation,
      }),
    );
  }

  /**
   * 身份/存储失效后的内部清理：作废草稿、请求释放临时资源、清掉本标签页归属。
   *
   * **普通返回目录/切换会话不调用它**；旧版用户动作曾走 {@link abandonPreparation}。
   */
  leavePreparation(sessionId: Uuid): void {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined) return;
    const drafts = this.#preparationDrafts;
    const held = this.#preparationDraftAccess.get(sessionId);
    if (drafts !== null && held !== undefined) {
      const nowMs = this.#now();
      // 只有身份/存储失效才走到这里（普通导航不再清理）；墓碑原因按 `replaced` 记录。
      void drafts
        .invalidateDraft({ access: held.access, expectedRevision: held.revision, reason: "replaced", nowMs })
        .catch(() => undefined)
        .then(() => drafts.releaseTab({ access: held.access, nowMs }))
        .catch(() => undefined);
    }
    this.#releasePreparationResource(session);
    this.#forgetPreparation(sessionId);
  }

  #forgetPreparation(sessionId: Uuid): void {
    this.#cancelPreparationRenew(sessionId);
    this.#preparationVerifyThenRestore.delete(sessionId);
    const timer = this.#preparationSaveTimers.get(sessionId);
    if (timer !== undefined) {
      timer();
      this.#preparationSaveTimers.delete(sessionId);
    }
    this.#preparationDraftAccess.delete(sessionId);
    this.#preparationOps.delete(sessionId);
    this.#preparationSubmissions.delete(sessionId);
    this.#preparationSubmissionIds.delete(sessionId);
    const attachments = new Map(this.#state.attachments);
    for (const attachment of attachments.get(sessionId) ?? []) {
      if (attachment.previewUrl !== null) this.#attachmentMedia?.releaseUrl(attachment.previewUrl);
      this.#attachmentHandles.delete(attachment.localId);
      this.#restoredAttachmentBytes.delete(attachment.localId);
    }
    attachments.delete(sessionId);
    const drafts = new Map(this.#state.drafts);
    drafts.delete(sessionId);
    this.#commit({
      ...this.#state,
      preparations: withoutEntry(this.#state.preparations, sessionId),
      attachments,
      drafts,
    });
  }

  /** 换 Agent 的前半段：释放旧资源、作废旧草稿、清空输入与附件。 */
  async #discardPreparation(sessionId: Uuid, reason: "agent_changed"): Promise<void> {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined) return;
    const drafts = this.#preparationDrafts;
    const held = this.#preparationDraftAccess.get(sessionId);
    if (drafts !== null) {
      if (held !== undefined) {
        try {
          await drafts.invalidateDraft({
            access: held.access,
            expectedRevision: held.revision,
            reason,
            nowMs: this.#now(),
          });
        } catch {
          // 草稿已不可写（配额/存储不可用）：内存输入仍按主动更换清空，不因此中断换 ID。
        }
      }
      // 换 Agent 后旧定位不再指向任何槽位；新分配成功保存草稿时会写回新定位。
      try {
        await drafts.clearTabLocator();
      } catch {
        // 定位清理由新 `beginTab` 覆盖，失败不阻塞换 ID。
      }
    }
    this.#releasePreparationResource(session);
    this.#forgetPreparation(sessionId);
  }

  /** 以既有目录与新的 Agent 分配一个**新** ID；旧响应因会话键不同而无法覆盖它。 */
  #allocateReplacement(input: {
    readonly agentId: string;
    readonly workspace: PreparationWorkspace;
    readonly tabNonce: string;
  }): Promise<PreparationBeginOutcome> {
    const availability = this.preparationEntryAvailability();
    if (!availability.available) return Promise.resolve({ ok: false, reason: availability.reason ?? "准备不可用" });
    const workspaceInput: PreparationWorkspaceInput =
      input.workspace.source === "directoryId" && input.workspace.directoryId !== null
        ? { directoryId: input.workspace.directoryId }
        : { workspacePath: input.workspace.path };
    return this.#allocatePreparation({
      operationId: this.#ids.uuid(),
      agentId: input.agentId,
      workspace: input.workspace,
      workspaceInput,
      tabNonce: input.tabNonce,
    });
  }

  /** 草稿分区键：客户端不知道 core 的 nodeId，用已配对 Host 身份作为本机节点作用域。 */
  #preparationScope(sessionId: Uuid): PreparationDraftScope | null {
    const host = this.#state.host;
    const deviceId = this.#state.deviceId;
    if (host === null || deviceId === null) return null;
    return {
      canonicalOrigin: host.address,
      ownerNodeId: host.hostId ?? deviceId,
      deviceId,
      // 首次加载时只可能知道来源/节点/设备/会话；端口会沿用定位里的 tabSlotId（刷新即同一格）。
      tabSlotId: this.#ids.uuid(),
      sessionId,
    };
  }

  #updatePreparation(sessionId: Uuid, update: (current: PreparationSessionState) => PreparationSessionState): void {
    const current = this.#state.preparations.get(sessionId);
    if (current === undefined) return;
    const next = new Map(this.#state.preparations);
    next.set(sessionId, update(current));
    this.#commit({ ...this.#state, preparations: next });
  }

  /**
   * 发送一条准备控制帧并把 `operationId` 记为当前在飞操作。
   *
   * 未发出（没有已认证连接）时只留下可读说明：不假装资源已释放/设置已生效。
   */
  #sendPreparationFrame(
    sessionId: Uuid,
    kind: "restore" | "configure" | "renew" | "release",
    body: Parameters<SyncGateway["sendPreparation"]>[0],
  ): CommandOutcome {
    if (kind !== "release") {
      this.#preparationOps.set(sessionId, { operationId: body.operationId, kind });
    }
    if (!this.#sync.sendPreparation(body)) {
      this.#preparationOps.delete(sessionId);
      this.#updatePreparation(sessionId, (current) => ({
        ...current,
        notice: "命令通道尚未就绪，这次准备操作没有发出",
      }));
      return { ok: false, reason: "命令通道尚未就绪" };
    }
    return { ok: true, requestId: body.operationId };
  }

  #sendConfigure(
    sessionId: Uuid,
    values: { readonly mode?: string; readonly configOptions?: Readonly<Record<string, string | boolean>> },
  ): CommandOutcome {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined || session.receipt === null || session.runtimeEpoch === null || session.generation === null) {
      return { ok: false, reason: "准备资源尚未就绪" };
    }
    return this.#sendPreparationFrame(
      sessionId,
      "configure",
      buildConfigureBody({
        operationId: this.#ids.uuid(),
        sessionId,
        receipt: session.receipt,
        runtimeEpoch: session.runtimeEpoch,
        generation: session.generation,
        ...(values.mode === undefined ? {} : { mode: values.mode }),
        ...(values.configOptions === undefined
          ? {}
          : { configOptions: values.configOptions as Readonly<Record<string, string | boolean>> }),
      }),
    );
  }

  #sendRestore(sessionId: Uuid): CommandOutcome {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined || session.receipt === null) {
      return { ok: false, reason: "没有可用的准备凭据" };
    }
    return this.#sendPreparationFrame(
      sessionId,
      "restore",
      buildRestoreBody({
        operationId: this.#ids.uuid(),
        sessionId,
        receipt: session.receipt,
        knownRuntimeEpoch: session.runtimeEpoch,
        knownGeneration: session.generation,
      }),
    );
  }

  /** 可见的 ready 准备每 10 秒续租；隐藏/断开/受理/释放都取消（旧连接不续命）。 */
  #schedulePreparationRenew(sessionId: Uuid): void {
    this.#cancelPreparationRenew(sessionId);
    if (this.#schedule === null) return;
    const cancel = this.#schedule(PREPARATION_RENEW_INTERVAL_MS, () => {
      this.#preparationRenewTimers.delete(sessionId);
      this.#renewPreparation(sessionId);
    });
    this.#preparationRenewTimers.set(sessionId, cancel);
  }

  #cancelPreparationRenew(sessionId: Uuid): void {
    const cancel = this.#preparationRenewTimers.get(sessionId);
    if (cancel === undefined) return;
    cancel();
    this.#preparationRenewTimers.delete(sessionId);
  }

  #cancelAllPreparationRenewals(): void {
    for (const cancel of [...this.#preparationRenewTimers.values()]) cancel();
    this.#preparationRenewTimers.clear();
  }

  /** 发一条 `renew`（当前 receipt/epoch/generation）；失租由服务端回 `stale` 走核实+restore。 */
  #renewPreparation(sessionId: Uuid): void {
    const state = this.#state;
    const session = state.preparations.get(sessionId);
    if (session === undefined) return;
    // 失败/不确定但资源仍在时也要续租：继续显式派发前资源不能先过期。
    if (session.phase !== "ready" && session.phase !== "failed") return;
    if (session.receipt === null || session.runtimeEpoch === null || session.generation === null) return;
    if (state.connection.state !== "online" || this.#identity.connectionId === null) return;
    if (state.lifecycle.visibility !== "visible") return;
    const sent = this.#sendPreparationFrame(
      sessionId,
      "renew",
      buildRenewBody({
        operationId: this.#ids.uuid(),
        sessionId,
        receipt: session.receipt,
        runtimeEpoch: session.runtimeEpoch,
        generation: session.generation,
      }),
    );
    if (sent.ok) this.#schedulePreparationRenew(sessionId);
  }

  /**
   * 重连 / 服务重启 / 返回可见后的准备恢复（与标题状态查询同一条排空路径）。
   *
   * - `submitting`：响应丢失的请求只能查 `command.status`（原 `requestId`），不重发；
   * - `unavailable`：只有「打开草稿时还没连上」这一种不可用会自愈（见下），其余保持不可用；
   * - 其余存活阶段：`restore` 同 ID 的临时资源（服务端可能回 `committed`），并重新排续租。
   *
   * 隐藏时不恢复：后台不保活临时 Agent（design D3）。
   */
  #recoverPreparations(): void {
    const state = this.#state;
    if (state.connection.state !== "online" || this.#identity.connectionId === null) return;
    if (state.lifecycle.visibility !== "visible") return;
    for (const session of state.preparations.values()) {
      const sessionId = session.sessionId;
      if (session.phase === "unavailable") {
        // 刷新时页面先于认证挂载：`#restoreFromDraft` 已读出草稿与凭据，却只能把入口记成
        // 「还没连上主机」。连接就绪后按原凭据恢复，否则刷新一次就永久停在不可用。
        //
        // 另外两种不可用**不**重试：存储整体重建（`restartRequired`，页面随后回目录）与
        // 「当前服务端未协商该能力」——后者重连也不会自愈，发帧只会被门控拒掉。
        if (session.restartRequired || session.receipt === null) continue;
        if (!this.preparationEntryAvailability().available) continue;
        this.#updatePreparation(sessionId, (current) => ({ ...current, phase: "restoring", notice: null }));
        this.#sendRestore(sessionId);
        continue;
      }
      if (session.phase === "accepted" || session.phase === "committed" ||
          session.phase === "released" || session.phase === "allocating") {
        continue;
      }
      if (session.phase === "submitting") {
        const target = session.association?.requestId ?? null;
        if (target === null || this.#preparationStatusQueries.has(target)) continue;
        this.#preparationStatusQueries.add(target);
        this.#queryPreparationRequest(sessionId, target);
        continue;
      }
      if (session.receipt === null) continue;
      if (this.#preparationOps.has(sessionId)) continue;
      this.#sendRestore(sessionId);
    }
  }

  /** 用原 `requestId` 查首条请求的权威状态（响应丢失/结果不明时的唯一动作）。 */
  #queryPreparationRequest(sessionId: Uuid, targetRequestId: Uuid): CommandOutcome {
    const requestId = this.#ids.uuid();
    this.#preparationRequestSessions.set(requestId, sessionId);
    const dispatched = this.#dispatchIntent({
      sessionId,
      requestId,
      build: (id) => ({ requestId: id, command: "command.status", payload: { targetRequestId } }),
    });
    if (dispatched === null) return NOT_CONNECTED;
    return { ok: true, requestId };
  }

  /** 草稿写合并：同一会话 350ms 内只写一次（正文输入逐键触发）。 */
  #schedulePreparationSave(sessionId: Uuid): void {
    const existing = this.#preparationSaveTimers.get(sessionId);
    if (existing !== undefined) existing();
    const task = (): void => {
      this.#preparationSaveTimers.delete(sessionId);
      void this.#savePreparationDraft(sessionId);
    };
    if (this.#schedule === null) {
      task();
      return;
    }
    this.#preparationSaveTimers.set(sessionId, this.#schedule(350, task));
  }

  /** 读出附件字节：恢复过的草稿用内存字节，本次选择的用平台句柄（都按块读）。 */
  async #readSelectedAttachment(localId: string, offset: number, length: number): Promise<Uint8Array> {
    const restored = this.#restoredAttachmentBytes.get(localId);
    if (restored !== undefined) return restored.slice(offset, offset + length);
    const handle = this.#attachmentHandles.get(localId);
    const media = this.#attachmentMedia;
    if (handle === undefined || media === null) throw new Error("附件在本机的句柄已丢失，请重新选择");
    return media.readChunk(handle, BigInt(offset), length);
  }

  /** 把当前准备草稿（正文/设置/凭据/附件字节）原子写入浏览器的准备库。 */
  async #savePreparationDraft(sessionId: Uuid): Promise<void> {
    const drafts = this.#preparationDrafts;
    const session = this.#state.preparations.get(sessionId);
    if (drafts === null || session === undefined || session.receipt === null) return;
    const held = this.#preparationDraftAccess.get(sessionId);
    let access = held?.access;
    if (access === undefined) {
      const scope = this.#preparationScope(sessionId);
      if (scope === null) return;
      try {
        const begun = await drafts.beginTab({ scope, nowMs: this.#now() });
        if (begun.status !== "owner") {
          this.#updatePreparation(sessionId, (current) => ({
            ...current,
            notice: PREPARATION_DUPLICATE_TAB_NOTICE,
          }));
          return;
        }
        access = begun.access;
        this.#preparationDraftAccess.set(sessionId, {
          access,
          revision: null,
          restorable: begun.restorable,
        });
      } catch {
        this.#notePreparationStorageUnavailable(sessionId);
        return;
      }
    }
    const attachments = this.#state.attachments.get(sessionId) ?? [];
    const inputs: PreparationDraftAttachmentInput[] = attachments.map((attachment) => ({
      localId: attachment.localId,
      kind: attachment.kind,
      fileName: attachment.fileName,
      mediaType: attachment.mediaType,
      byteLength: attachment.byteLength,
      chunkSize: 128 * 1024,
      chunkCount: Math.max(1, Math.ceil(Number(attachment.byteLength) / (128 * 1024))),
      readChunk: (offset, length) => this.#readSelectedAttachment(attachment.localId, offset, length),
    }));
    const content: PreparationDraftContent = {
      text: this.#state.drafts.get(sessionId) ?? "",
      mode: session.selectedMode,
      configOptions: session.selectedConfig,
      agentId: session.agentId,
      workspace: session.workspace,
      receipt: session.receipt,
      pendingRequestId: session.association?.requestId ?? null,
    };
    const currentHeld = this.#preparationDraftAccess.get(sessionId);
    try {
      const saved = await drafts.saveDraft({
        access,
        expectedRevision: currentHeld?.revision ?? null,
        content,
        attachments: inputs,
        nowMs: this.#now(),
      });
      this.#preparationDraftAccess.set(sessionId, {
        access,
        revision: saved.revision,
        restorable: saved.restorable,
      });
      if (!saved.restorable) {
        this.#updatePreparation(sessionId, (current) =>
          current.notice === PREPARATION_NOT_RESTORABLE_NOTICE
            ? current
            : { ...current, notice: PREPARATION_NOT_RESTORABLE_NOTICE },
        );
      }
    } catch (error) {
      const detail = error instanceof Error ? error.message : "浏览器不能保存准备草稿";
      this.#updatePreparation(sessionId, (current) => ({
        ...current,
        notice: `${PREPARATION_STORAGE_FAILED_NOTICE}${detail.length > 0 ? `：${detail}` : ""}`,
      }));
    }
  }

  #notePreparationStorageUnavailable(sessionId: Uuid): void {
    this.#updatePreparation(sessionId, (current) =>
      current.notice === PREPARATION_STORAGE_FAILED_NOTICE
        ? current
        : { ...current, notice: PREPARATION_STORAGE_FAILED_NOTICE },
    );
  }

  /** 打开发送前草稿：active → `restore`（或先核实原 start）；accepted → 同页读正式会话。 */
  async #restoreFromDraft(sessionId: Uuid, scope: PreparationDraftScope): Promise<void> {
    const drafts = this.#preparationDrafts;
    if (drafts === null) return;
    let begun;
    try {
      begun = await drafts.beginTab({ scope, nowMs: this.#now() });
    } catch {
      return;
    }
    if (begun.status === "duplicate") {
      this.#setPreparationNotice(sessionId, PREPARATION_DUPLICATE_TAB_NOTICE);
      return;
    }
    const access = begun.access;
    this.#preparationDraftAccess.set(sessionId, { access, revision: null, restorable: begun.restorable });
    let loaded;
    try {
      loaded = await drafts.loadDraft({ access, nowMs: this.#now() });
    } catch {
      this.#preparationDraftAccess.delete(sessionId);
      return;
    }
    if (loaded.status === "absent" || loaded.status === "invalidated") {
      this.#preparationDraftAccess.delete(sessionId);
      return;
    }
    this.#preparationDraftAccess.set(sessionId, {
      access,
      revision: loaded.record.revision,
      restorable: begun.restorable,
    });
    if (loaded.status === "accepted") {
      // 保存响应丢失后的恢复记录：正文/Blob 已清，只留请求关联；先核实原请求再读正式会话。
      const association: PreparationAssociation = {
        requestId: loaded.association.requestId,
        firstSubmissionId: loaded.association.firstSubmissionId,
        firstRequestId: null,
        currentRequestId: null,
        firstMessageId: loaded.association.firstMessageId,
      };
      this.#commit({
        ...this.#state,
        preparations: withEntry(this.#state.preparations, sessionId, {
          sessionId,
          agentId: "",
          workspace: { directoryId: null, path: "", source: "workspacePath" },
          tabNonce: "",
          receipt: null,
          runtimeEpoch: null,
          generation: null,
          leaseExpiresAt: null,
          modes: [],
          configOptions: [],
          selectedMode: null,
          selectedConfig: {},
          phase: "accepted",
          association,
          error: null,
          failureOrigin: null,
          canContinue: false,
          notice: "首条消息已保存；正在读取正式会话。",
          restartRequired: false,
        }),
      });
      this.#preparationRequestSessions.set(association.requestId, sessionId);
      this.#queryPreparationRequest(sessionId, association.requestId);
      this.#enterFormalSession(sessionId);
      return;
    }

    const payload = loaded.record.payload;
    if (payload.status !== "active") {
      // 读取器把已受理记录归到 `accepted` 分支；走到这里说明记录在两次读之间被替换，按无草稿处理。
      this.#preparationDraftAccess.delete(sessionId);
      return;
    }
    const content = payload.content;
    const receipt = content.receipt;
    const association: PreparationAssociation | null =
      content.pendingRequestId === null
        ? null
        : {
            requestId: content.pendingRequestId,
            firstSubmissionId: null,
            firstRequestId: null,
            currentRequestId: null,
            firstMessageId: null,
          };
    const restoredAttachments: DraftAttachment[] = [];
    for (const meta of payload.attachments) {
      try {
        const read = await drafts.readAttachment({ access, localId: meta.localId });
        this.#restoredAttachmentBytes.set(meta.localId, read.bytes);
        restoredAttachments.push({
          localId: meta.localId,
          transferId: meta.localId,
          attachmentId: null,
          kind: meta.kind,
          fileName: meta.fileName,
          mediaType: meta.mediaType,
          byteLength: meta.byteLength,
          status: "selected",
          uploadedBytes: "0",
          sha256: null,
          error: null,
          previewUrl: null,
        });
      } catch {
        // 单项字节读不出来时明确标注该项按未选中处理（不伪造一个空的「已恢复」附件）。
      }
    }
    const session: PreparationSessionState = {
      sessionId,
      agentId: content.agentId,
      workspace: content.workspace,
      tabNonce: receipt.claims.tabNonce,
      receipt,
      runtimeEpoch: null,
      generation: null,
      leaseExpiresAt: null,
      modes: [],
      configOptions: [],
      selectedMode: content.mode,
      selectedConfig: content.configOptions,
      phase: "restoring",
      association,
      error: null,
      failureOrigin: null,
      canContinue: false,
      notice: null,
      restartRequired: false,
    };
    // 与 `#withPreparationResult` 同形的单槽位守卫：另一次分配/恢复已经占住槽位时，本次慢恢复
    // 不落库——否则同一标签页会同时出现两条「新会话」。本次恢复还没有向服务端发任何帧，
    // 丢弃它只需清掉刚取得的草稿访问权。
    const occupied = this.#activePreparation();
    if (occupied !== null && occupied.sessionId !== sessionId) {
      this.#preparationDraftAccess.delete(sessionId);
      for (const attachment of restoredAttachments) this.#restoredAttachmentBytes.delete(attachment.localId);
      return;
    }
    const attachments = new Map(this.#state.attachments);
    if (restoredAttachments.length > 0) attachments.set(sessionId, restoredAttachments);
    this.#commit({
      ...this.#state,
      preparations: withEntry(this.#state.preparations, sessionId, session),
      drafts: withEntry(this.#state.drafts, sessionId, content.text),
      attachments,
    });
    if (association !== null) {
      this.#preparationRequestSessions.set(association.requestId, sessionId);
      this.#queryPreparationRequest(sessionId, association.requestId);
      return;
    }
    const availability = this.preparationEntryAvailability();
    if (!availability.available) {
      this.#updatePreparation(sessionId, (current) => ({
        ...current,
        phase: "unavailable",
        notice: availability.reason,
      }));
      return;
    }
    this.#sendRestore(sessionId);
  }

  #setPreparationNotice(sessionId: Uuid, notice: string): void {
    const preparations = new Map(this.#state.preparations);
    if (!preparations.has(sessionId)) {
      this.#commit({
        ...this.#state,
        preparations: withEntry(preparations, sessionId, {
          sessionId,
          agentId: "",
          workspace: { directoryId: null, path: "", source: "workspacePath" },
          tabNonce: "",
          receipt: null,
          runtimeEpoch: null,
          generation: null,
          leaseExpiresAt: null,
          modes: [],
          configOptions: [],
          selectedMode: null,
          selectedConfig: {},
          phase: "unavailable",
          association: null,
          error: null,
          failureOrigin: null,
          canContinue: false,
          notice,
          restartRequired: false,
        }),
      });
      return;
    }
    preparations.set(sessionId, { ...preparations.get(sessionId)!, notice });
    this.#commit({ ...this.#state, preparations });
  }

  /** 同一页面改读正式会话：首次读取 + 权威清单刷新（只为让目录/摘要跟上）。 */
  #enterFormalSession(sessionId: Uuid): void {
    this.#cancelPreparationRenew(sessionId);
    this.loadSession(sessionId);
    this.#sync.refreshSnapshot();
  }

  /** 准备控制应答 → 状态。迟到的旧操作（`operationId` 不匹配）不改变任何状态。 */
  #withPreparationResult(state: ClientState, result: SyncSessionPreparationResult): ClientState {
    const body = result.body;
    const resultSessionId = body.sessionId;
    if (body.action === "allocate") {
      const pending = this.#pendingAllocations.get(body.operationId);
      if (pending === undefined) return state;
      this.#pendingAllocations.delete(body.operationId);
      if (body.status !== "ready") {
        pending.resolve({
          ok: false,
          reason: body.status === "failed" ? body.error.message : "服务端没有分配会话",
        });
        return state;
      }
      // 竞赛防御：恢复/另一分配已经占住唯一槽位时，不把新分配插进来；把新资源当场释放，
      // 两个入口都只指向既有槽位。
      const existing = this.#activePreparation(state);
      if (existing !== null && existing.sessionId !== resultSessionId) {
        pending.resolve({ ok: true, sessionId: existing.sessionId });
        this.#sync.sendPreparation(buildReleaseBody({
          operationId: this.#ids.uuid(),
          sessionId: resultSessionId,
          receipt: body.receipt,
          runtimeEpoch: body.runtimeEpoch,
          generation: body.generation,
        }));
        return state;
      }
      const session: PreparationSessionState = {
        sessionId: resultSessionId,
        agentId: pending.agentId,
        workspace: { ...pending.workspace,
          directoryId: body.receipt.claims.workspaceSource === "directoryId" ? pending.workspace.directoryId :
            state.resources?.workspaces.find((item) => displayDirectoryPath(item.path) ===
              displayDirectoryPath(body.receipt.claims.normalizedWorkspacePath))?.directoryId ?? null,
          path: body.receipt.claims.normalizedWorkspacePath,
          source: body.receipt.claims.workspaceSource },
        tabNonce: pending.tabNonce,
        receipt: body.receipt,
        runtimeEpoch: body.runtimeEpoch,
        generation: body.generation,
        leaseExpiresAt: body.leaseExpiresAt,
        modes: body.modes,
        configOptions: body.configOptions,
        selectedMode: null,
        selectedConfig: {},
        phase: "ready",
        association: null,
        error: null,
        failureOrigin: null,
        canContinue: false,
        notice: null,
        restartRequired: false,
      };
      pending.resolve({ ok: true, sessionId: resultSessionId });
      this.#preparationEffects.push(() => {
        this.#schedulePreparationSave(resultSessionId);
        this.#schedulePreparationRenew(resultSessionId);
      });
      return { ...state, preparations: withEntry(state.preparations, resultSessionId, session) };
    }

    const session = state.preparations.get(resultSessionId);
    if (session === undefined) return state;
    const pendingOp = this.#preparationOps.get(resultSessionId);
    if (pendingOp !== undefined && pendingOp.operationId !== body.operationId) return state;
    this.#preparationOps.delete(resultSessionId);
    const preparations = new Map(state.preparations);

    switch (body.status) {
      case "ready": {
        // 恢复临时资源不改变「首条是否可继续」的事实：已有首次提交身份的 failed/uncertain
        // 保持原阶段（`canContinue` 不由 restore 重置），否则 ready 会把不可继续的失败洗成可发送。
        const keepPhase = session.phase === "failed" || session.phase === "uncertain";
        preparations.set(resultSessionId, {
          ...session,
          workspace: { ...session.workspace, path: body.receipt.claims.normalizedWorkspacePath,
            source: body.receipt.claims.workspaceSource },
          receipt: body.receipt,
          runtimeEpoch: body.runtimeEpoch,
          generation: body.generation,
          leaseExpiresAt: body.leaseExpiresAt,
          modes: body.modes,
          configOptions: body.configOptions,
          phase: keepPhase ? session.phase : "ready",
          error: keepPhase ? session.error : null,
          failureOrigin: keepPhase ? session.failureOrigin : null,
        });
        break;
      }
      case "committed": {
        const association: PreparationAssociation = {
          requestId: body.currentRequestId,
          firstSubmissionId: body.firstSubmissionId,
          firstRequestId: body.firstRequestId,
          currentRequestId: body.currentRequestId,
          firstMessageId: body.firstMessageId,
        };
        preparations.set(resultSessionId, {
          ...session,
          phase: "committed",
          association,
          error: null,
          failureOrigin: null,
          canContinue: false,
          notice: null,
        });
        this.#preparationRequestSessions.set(body.currentRequestId, resultSessionId);
        this.#preparationEffects.push(() => {
          this.#cancelPreparationRenew(resultSessionId);
          this.#preparationAwaitingRead.add(resultSessionId);
          this.#enterFormalSession(resultSessionId);
        });
        break;
      }
      case "stale": {
        preparations.set(resultSessionId, {
          ...session,
          notice: "临时资源已失效，正在重新准备",
        });
        // 先核实原 start，再用 receipt 恢复（restore 对有效凭据不会返回 stale）。
        const staleRequestId = session.association?.requestId ?? null;
        if (staleRequestId !== null) {
          this.#preparationRequestSessions.set(staleRequestId, resultSessionId);
        }
        this.#preparationEffects.push(() => {
          this.#cancelPreparationRenew(resultSessionId);
          if (staleRequestId !== null) {
            this.#preparationVerifyThenRestore.add(resultSessionId);
            this.#queryPreparationRequest(resultSessionId, staleRequestId);
          } else {
            this.#sendRestore(resultSessionId);
          }
        });
        break;
      }
      case "released": {
        preparations.set(resultSessionId, { ...session, phase: "released", receipt: null, canContinue: false });
        this.#preparationEffects.push(() => {
          this.#cancelPreparationRenew(resultSessionId);
        });
        break;
      }
      case "failed": {
        preparations.set(
          resultSessionId,
          this.#preparationFailureState(session, body.error, "server_start", this.#preparationEffects),
        );
        break;
      }
    }
    if (body.status === "ready") {
      this.#preparationEffects.push(() => {
        this.#schedulePreparationSave(resultSessionId);
        this.#schedulePreparationRenew(resultSessionId);
      });
    }
    return { ...state, preparations };
  }

  /**
   * 一条准备失败 → D6.3 的**固定动作**（控制面 `failed`、`session.start` 失败与附件错误共用）。
   *
   * `effects` 是归并阶段推迟副作用的收集器：调用方在提交之后统一执行。
   */
  #preparationFailureState(
    session: PreparationSessionState,
    error: PublicError,
    origin: "local_upload" | "server_start",
    effects: { push(effect: () => void): unknown },
  ): PreparationSessionState {
    const sessionId = session.sessionId;
    const action = preparationErrorAction(error);
    switch (action) {
      case "restart":
        // 存储整体重建：旧 receipt 不能重放/restore，草稿关联也一并作废；页面回目录。
        effects.push(() => {
          this.#invalidatePreparationDraft(sessionId);
          this.#cancelPreparationRenew(sessionId);
        });
        return {
          ...session,
          phase: "unavailable",
          error,
          notice: PREPARATION_STORE_REBUILT_NOTICE,
          restartRequired: true,
          receipt: null,
          canContinue: false,
        };
      case "already_committed":
        // 已正式受理：停止上传，restore 取 committed 关联，同页读正式会话，不重传。
        effects.push(() => {
          this.#cancelPreparationRenew(sessionId);
          this.#sendRestore(sessionId);
        });
        return {
          ...session,
          phase: "restoring",
          error,
          notice: "首条消息已保存（或已受理），正在读取正式会话；不会重传附件。",
          failureOrigin: origin,
          canContinue: false,
        };
      case "restore": {
        // 过代/已释放：先核实原 start，再 restore（restore 对有效凭据不会 stale）。
        const target = session.association?.requestId ?? null;
        effects.push(() => {
          this.#cancelPreparationRenew(sessionId);
          if (target !== null) {
            this.#preparationVerifyThenRestore.add(sessionId);
            this.#queryPreparationRequest(sessionId, target);
          } else {
            this.#sendRestore(sessionId);
          }
        });
        return {
          ...session,
          phase: session.phase === "ready" ? "restoring" : session.phase,
          error,
          notice: "临时资源已失效，正在核实原请求后重新准备",
          failureOrigin: origin,
          canContinue: false,
        };
      }
      case "verify": {
        const target = session.association?.requestId ?? null;
        effects.push(() => {
          if (target !== null) this.#queryPreparationRequest(sessionId, target);
        });
        return {
          ...session,
          phase: "uncertain",
          error: null,
          notice: PREPARATION_UNCERTAIN_NOTICE,
          failureOrigin: origin,
          canContinue: false,
        };
      }
      case "retry":
        return {
          ...session,
          phase: session.phase === "submitting" ? "failed" : session.phase,
          error,
          failureOrigin: origin,
          canContinue: false,
        };
      case "stop":
      default:
        return {
          ...session,
          phase: "failed",
          error,
          failureOrigin: origin,
          canContinue: false,
        };
    }
  }

  /** 归并之外的失败入口（附件传输/本地保存）：立刻提交并执行固定动作。 */
  #applyPreparationFailureNow(
    sessionId: Uuid,
    error: PublicError,
    origin: "local_upload" | "server_start" = "local_upload",
  ): void {
    const session = this.#state.preparations.get(sessionId);
    if (session === undefined) return;
    const effects: (() => void)[] = [];
    const next = this.#preparationFailureState(session, error, origin, effects);
    this.#updatePreparation(sessionId, () => next);
    for (const effect of effects) {
      try {
        effect();
      } catch {
        // 单个副作用失败（例如命令通道刚断）不应让失败处理中断。
      }
    }
  }

  /** 执行归并阶段推迟的副作用（每项只执行一次；异常不打断后续项）。 */
  #drainPreparationEffects(): void {
    if (this.#preparationEffects.length === 0) return;
    const effects = this.#preparationEffects.splice(0);
    for (const effect of effects) {
      try {
        effect();
      } catch {
        // 单个副作用失败（例如连接刚断）不应让整条事件流中断。
      }
    }
  }

  #invalidatePreparationDraft(sessionId: Uuid): void {
    const drafts = this.#preparationDrafts;
    const held = this.#preparationDraftAccess.get(sessionId);
    if (drafts === null || held === undefined) return;
    this.#preparationDraftAccess.delete(sessionId);
    void drafts
      .invalidateDraft({
        access: held.access,
        expectedRevision: held.revision,
        reason: "replaced",
        nowMs: this.#now(),
      })
      .catch(() => undefined);
  }

  /** `session.start` / `command.status` 的结果 → 准备阶段（未知请求不改变状态）。 */
  #withPreparationStartResult(
    state: ClientState,
    result: CommandResultMessage,
    options: { readonly statusQuery: boolean },
  ): ClientState {
    const requestId = options.statusQuery
      ? isCommandStatusRecord(result.body.result)
        ? result.body.result.targetRequestId
        : null
      : result.body.requestId;
    if (requestId === null) return state;
    const sessionId = this.#preparationRequestSessions.get(requestId);
    if (sessionId === undefined) return state;
    const session = state.preparations.get(sessionId);
    if (session === undefined) return state;
    const status = options.statusQuery
      ? isCommandStatusRecord(result.body.result)
        ? result.body.result.state
        : result.body.status
      : result.body.status;
    const projection =
      result.body.command === "session.start" &&
      result.body.status === "completed" &&
      isSessionStartProjection(result.body.result)
        ? result.body.result
        : options.statusQuery && isCommandStatusRecord(result.body.result)
          ? isSessionStartProjection(result.body.result.preparation)
            ? result.body.result.preparation
            : null
          : null;
    const association: PreparationAssociation = {
      requestId,
      firstSubmissionId: projection?.firstSubmissionId ?? session.association?.firstSubmissionId ?? null,
      firstRequestId: projection?.firstRequestId ?? session.association?.firstRequestId ?? null,
      currentRequestId: projection?.currentRequestId ?? session.association?.currentRequestId ?? null,
      firstMessageId: session.association?.firstMessageId ?? null,
    };
    const canContinue = projection?.canContinue === true;
    let next: PreparationSessionState;
    switch (status) {
      case "accepted":
      case "completed":
        next = {
          ...session,
          phase: "accepted",
          association,
          error: null,
          failureOrigin: null,
          canContinue: false,
          notice: null,
        };
        break;
      case "failed": {
        // 「先核实原 start，再 restore」的第二步：查询已经落地，现在才恢复临时资源。
        const verifyThenRestore =
          options.statusQuery && this.#preparationVerifyThenRestore.has(sessionId);
        if (verifyThenRestore) this.#preparationVerifyThenRestore.delete(sessionId);
        const failureError = result.body.error ?? session.error;
        if (options.statusQuery && verifyThenRestore) {
          next = {
            ...session,
            phase: "restoring",
            association,
            error: failureError,
            failureOrigin: "server_start",
            canContinue,
            notice: "原请求已核实，正在重新准备",
          };
          this.#preparationEffects.push(() => {
            this.#cancelPreparationRenew(sessionId);
            this.#sendRestore(sessionId);
          });
        } else if (options.statusQuery) {
          // 普通核实：只落投影。服务端没有确认「可证明未派发」时不许继续，也不自动恢复。
          next = {
            ...session,
            phase: "failed",
            association,
            error: failureError,
            failureOrigin: "server_start",
            canContinue,
            notice: canContinue
              ? "服务端确认首条未派发：可以显式继续。"
              : "服务端未确认可以继续（可能已派发）；只能核实原请求，不能重发。",
          };
        } else if (failureError === null) {
          next = {
            ...session,
            phase: "failed",
            association,
            error: null,
            failureOrigin: "server_start",
            canContinue,
            notice: null,
          };
        } else {
          next = this.#preparationFailureState(
            { ...session, association },
            failureError,
            "server_start",
            this.#preparationEffects,
          );
          next = { ...next, canContinue };
        }
        break;
      }
      case "rejected":
        next = {
          ...session,
          phase: "ready",
          association: null,
          error: result.body.error ?? session.error,
          failureOrigin: null,
          canContinue: false,
          notice: null,
        };
        this.#preparationEffects.push(() => {
          this.#schedulePreparationRenew(sessionId);
        });
        break;
      case "uncertain":
        next = {
          ...session,
          phase: "uncertain",
          association,
          error: null,
          failureOrigin: "server_start",
          canContinue: false,
          notice: PREPARATION_UNCERTAIN_NOTICE,
        };
        break;
      default:
        return state;
    }
    const preparations = new Map(state.preparations);
    preparations.set(sessionId, next);
    if (status === "accepted" || status === "completed") {
      this.#preparationEffects.push(() => {
        this.#cancelPreparationRenew(sessionId);
        this.#preparationAwaitingRead.add(sessionId);
        this.#enterFormalSession(sessionId);
      });
    }
    if (status === "completed") {
      this.#preparationEffects.push(() => {
        this.#completePreparationRequest(sessionId, requestId);
      });
    }
    return { ...state, preparations };
  }

  /** 首条命令终结后删掉恢复记录（正文/Blob 已在 `markAccepted` 清除）。 */
  #completePreparationRequest(sessionId: Uuid, requestId: Uuid): void {
    const drafts = this.#preparationDrafts;
    const held = this.#preparationDraftAccess.get(sessionId);
    const session = this.#state.preparations.get(sessionId);
    if (drafts === null || held === undefined || held.revision === null || session === undefined) return;
    if (session.phase !== "accepted" && session.phase !== "committed") return;
    void drafts
      .completeRequest({
        access: held.access,
        expectedRevision: held.revision,
        requestId,
        nowMs: this.#now(),
      })
      .catch(() => undefined);
  }

  /** 首条受理后：等到正式读取带回完整会话，再清准备草稿的正文/Blob。 */
  #drainPreparationsAwaitingRead(): void {
    if (this.#preparationAwaitingRead.size === 0) return;
    for (const sessionId of [...this.#preparationAwaitingRead]) {
      const thread = this.#state.threads.get(sessionId);
      if (thread === undefined || thread.page == null) continue;
      this.#preparationAwaitingRead.delete(sessionId);
      this.#linkFirstPromptMessage(sessionId, thread.page);
      this.#finalizeAcceptedPreparation(sessionId);
    }
  }

  /**
   * 把首条正文的**正式消息标识**登记到首条本地提交上。
   *
   * 首条 `session.start` 的受理结果没有 `turnId`，但正在转正的这个会话由这次首条提交创建：
   * 权威读取里**最早的一条用户消息**就是它。只在读取页确认没有更早内容（`hasEarlier === false`）
   * 时才这样关联——有更早内容时位置不能证明身份，宁可保留本地条目也不猜。
   */
  #linkFirstPromptMessage(sessionId: Uuid, page: SessionReadPageView): void {
    if (page.hasEarlier) return;
    const firstUser = page.messages.find((message) => message.role === "user");
    if (firstUser === undefined) return;
    const session = this.#state.preparations.get(sessionId);
    const requestId = session?.association?.requestId ?? null;
    if (requestId === null) return;
    this.#patchSentPrompt(requestId, (prompt) =>
      prompt.messageId === firstUser.id
        ? prompt
        : { ...prompt, messageId: firstUser.id, phase: prompt.phase === "failed" ? prompt.phase : "accepted" },
    );
  }

  /** 权威受理 + 正式读取之后：清浏览器正文/Blob，只保留请求关联。 */
  #finalizeAcceptedPreparation(sessionId: Uuid): void {
    const session = this.#state.preparations.get(sessionId);
    const drafts = this.#preparationDrafts;
    const held = this.#preparationDraftAccess.get(sessionId);
    if (session === undefined || session.association === null) return;
    const association = session.association;
    if (drafts !== null && held !== undefined && held.revision !== null) {
      this.#preparationDraftAccess.set(sessionId, {
        access: held.access,
        revision: null,
        restorable: held.restorable,
      });
      void drafts
        .markAccepted({
          access: held.access,
          expectedRevision: held.revision,
          association: {
            requestId: association.requestId,
            firstSubmissionId: association.firstSubmissionId,
            firstMessageId: association.firstMessageId,
            acceptedAtMs: this.#now(),
          },
          nowMs: this.#now(),
        })
        .then((saved) => {
          const current = this.#preparationDraftAccess.get(sessionId);
          if (current !== undefined && current.access === held.access) {
            this.#preparationDraftAccess.set(sessionId, {
              access: current.access,
              revision: saved.revision,
              restorable: current.restorable,
            });
          }
        })
        .catch(() => undefined);
    }
    // 内存里的正文/附件同样清掉：正式会话已成为唯一来源。
    const attachments = new Map(this.#state.attachments);
    for (const attachment of attachments.get(sessionId) ?? []) {
      if (attachment.previewUrl !== null) this.#attachmentMedia?.releaseUrl(attachment.previewUrl);
      this.#attachmentHandles.delete(attachment.localId);
      this.#restoredAttachmentBytes.delete(attachment.localId);
    }
    attachments.delete(sessionId);
    const draftsMap = new Map(this.#state.drafts);
    draftsMap.delete(sessionId);
    this.#commit({ ...this.#state, attachments, drafts: draftsMap });
  }

  /** 上传全部附件（准备目标）→ 封存清单 → 派发一次 `session.start`（`initial`）。 */
  async #uploadAndStart(input: { readonly sessionId: Uuid; readonly requestId: Uuid; readonly text: string }): Promise<void> {
    const sessionId = input.sessionId;
    try {
      const session = this.#state.preparations.get(sessionId);
      if (session === undefined || session.receipt === null || session.runtimeEpoch === null || session.generation === null) {
        throw new Error("准备资源尚未就绪");
      }
      const transfer = this.#attachmentTransfer;
      const subjectId = this.#state.deviceId;
      const attachments = this.#state.attachments.get(sessionId) ?? [];
      const target = {
        kind: "preparation" as const,
        sessionId,
        receipt: session.receipt,
        runtimeEpoch: session.runtimeEpoch,
        generation: session.generation,
      };
      let submissionId: string | null = null;
      if (attachments.length > 0) {
        if (transfer === null) throw new Error("附件能力尚未装配");
        if (subjectId === null) throw new Error("尚未认证设备，不能上传附件");
        if (!this.#state.selectedFeatures.includes(SESSION_PREPARATION_FEATURE)) {
          throw new Error(PREPARATION_UNSUPPORTED_NOTICE);
        }
        submissionId = this.#preparationSubmissionIds.get(sessionId) ?? this.#ids.uuid();
        this.#preparationSubmissionIds.set(sessionId, submissionId);
        const transferIds: string[] = [];
        for (const attachment of attachments) {
          this.#patchAttachment(sessionId, attachment.localId, (current) => ({
            ...current,
            status: "uploading",
            uploadedBytes: "0",
            error: null,
          }));
          let offset = 0;
          const total = Number(attachment.byteLength);
          const sealed = await transfer.upload({
            sessionId,
            subjectId,
            transferId: attachment.transferId,
            target,
            fileName: attachment.fileName,
            mediaType: attachment.mediaType,
            totalLength: attachment.byteLength,
            readChunk: async (chunkOffset, length) => {
              const bytes = await this.#readSelectedAttachment(attachment.localId, Number(chunkOffset), length);
              offset += bytes.length;
              if (offset > total) throw new Error("附件读取越界");
              return bytes;
            },
          });
          this.#patchAttachment(sessionId, attachment.localId, (current) => ({
            ...current,
            status: "sealed",
            uploadedBytes: current.byteLength,
            attachmentId: sealed.attachmentId,
            sha256: sealed.sha256,
            error: null,
          }));
          transferIds.push(attachment.transferId);
        }
        // 附件全部上传完、清单尚未登记：如实标「封存中」（Agent 仍未收到内容）。
        this.#patchSentPrompt(input.requestId, (prompt) => ({ ...prompt, phase: "sealing", notice: null }));
        await transfer.sealManifest({
          sessionId,
          subjectId,
          submissionId,
          transferIds,
          target,
        });
      }
      const sealedAttachments = this.#state.attachments.get(sessionId) ?? [];
      const content = buildInitialContent({
        text: input.text,
        attachments: sealedAttachments,
        pathRefs: this.#state.pathRefs.get(sessionId) ?? [],
      });
      if (!content.ok) throw new Error(content.reason);
      const currentSession = this.#state.preparations.get(sessionId);
      if (currentSession === undefined || currentSession.receipt === null ||
          currentSession.runtimeEpoch === null || currentSession.generation === null) {
        throw new Error("准备资源在提交前失效");
      }
      const mode = currentSession.selectedMode;
      const dispatched = this.#dispatchIntent({
        sessionId,
        requestId: input.requestId,
        build: (id) =>
          buildInitialStartBody({
            requestId: id,
            sessionId,
            receipt: currentSession.receipt!,
            runtimeEpoch: currentSession.runtimeEpoch!,
            generation: currentSession.generation!,
            content: content.content,
            submissionId,
            mode,
            configOptions: currentSession.selectedConfig,
          }),
        // 首条在准备态也要立刻可见。身份只用可靠关联：准备关联已知的首条正文标识，或
        // 附件上传阶段已登记的本地条目；**不**在视图层按相同文字去重。
        patch: (id) => {
          const existing = this.#state.sentPrompts.get(id);
          return {
            sentPrompts: withEntry(this.#state.sentPrompts, id, sentPromptOf({
              requestId: id,
              sessionId,
              text: input.text,
              attachmentNames: sealedAttachments.map((attachment) => attachment.fileName),
              createdAt: existing?.createdAt ?? new Date(this.#now()).toISOString(),
              phase: "submitted",
              messageId: existing?.messageId ?? currentSession.association?.firstMessageId ?? null,
            })),
          };
        },
      });
      if (dispatched === null) throw new Error("命令通道尚未就绪");
      this.#schedulePreparationSave(sessionId);
    } catch (error) {
      const detail = error instanceof Error ? error.message : "首条消息未能保存";
      for (const attachment of this.#state.attachments.get(sessionId) ?? []) {
        if (attachment.status === "sealed") continue;
        this.#patchAttachment(sessionId, attachment.localId, (current) =>
          current.status === "uploading"
            ? { ...current, status: "failed", error: detail }
            : current,
        );
      }
      // 服务端码（`attachment.error.code`）原样交给固定动作；其余本地失败按可重试处理。
      const failure: PublicError =
        error instanceof AttachmentTransferError && error.code !== null
          ? {
              code: error.code,
              message: error.message.length > 0 ? error.message : detail,
              retryable: error.retryable,
              details: error.details,
            }
          : { code: "internal.unavailable", message: detail, retryable: true, details: {} };
      // 已显示的输入不消失：标成「未发送」并保留原因（Agent 从未收到内容）。
      this.#patchSentPrompt(input.requestId, (prompt) => ({
        ...prompt,
        phase: "failed",
        notice: detail,
      }));
      this.#applyPreparationFailureNow(sessionId, failure, "local_upload");
    } finally {
      this.#preparationSubmissions.delete(sessionId);
    }
  }

  /**
   * 首次读取会话明细（正文 + 配置项 + 待处理交互，协商到 feature 时含结构化历史）。
   *
   * 一次只允许一个在途读取：两页并发返回时无法判断谁该前置谁。
   */
  loadSession(sessionId: Uuid): void {
    // 只拦**同一会话**的在途读取：切换会话/打开另一个会话时，旧的读取不应把新的首读永久挡掉
    // （那会让新会话只剩实时事件、看不到已保存的用户输入与历史）。
    const inFlight = [...this.#pendingReads.values()].some((pending) => pending.sessionId === sessionId);
    if (inFlight) return;
    const remote = remoteRefOf(this.#state, sessionId);
    const runtime = this.#state.remoteSessions.get(sessionId);
    if (remote !== null && runtime !== undefined &&
      (runtime.sourceState === "offline" || runtime.sourceState === "revoked")) {
      return;
    }
    if (
      remote !== null &&
      this.#state.deviceId !== null &&
      !this.#state.selectedFeatures.includes(STATELESS_ORIGIN_FEATURE)
    ) {
      return;
    }
    const requestId = this.#requestPage(sessionId, remote === null ? "first" : "rebuild_remote");
    if (requestId === null) {
      // 页面可能在认证完成前挂载（直接打开 `/session/<id>` 或刷新）；此时不能静默丢掉首次读取。
      this.#deferredReads.add(sessionId);
    } else {
      this.#deferredReads.delete(sessionId);
    }
  }

  /** 认证完成后补发被推迟的首次读取（只处理当前对话页的会话）。 */
  #drainDeferredReads(): void {
    // 快照未到时无法判定会话来源：此刻按 local 派发会把远程会话的 include 选错。
    if (this.#state.resources === null) return;
    for (const sessionId of [...this.#deferredReads]) {
      if (this.#activeConversationSession !== sessionId) continue;
      this.#deferredReads.delete(sessionId);
      this.loadSession(sessionId);
    }
  }

  /** 向上翻页（消息）：续取游标取上一页最早一条的 `(createdAt, messageId)`。 */
  loadEarlier(sessionId: Uuid): void {
    if ([...this.#pendingReads.values()].some((pending) => pending.sessionId === sessionId)) return;
    const thread = this.#state.threads.get(sessionId);
    const before = thread?.page?.earliestCursor ?? null;
    // Catch up the stream whose boundary is newer before extending the other stream.
    // An exhausted message stream must not hide older tools/history from the load action.
    const history = thread?.nextBeforeHistory ?? null;
    if (thread?.historyHasEarlier === true && history !== null &&
        (thread.page?.hasEarlier !== true ||
         (before?.sequence !== undefined && BigInt(history) > BigInt(before.sequence)))) {
      this.loadEarlierHistory(sessionId);
      return;
    }
    if (thread?.page?.hasEarlier !== true || before === null) return;
    this.#requestPage(sessionId, "earlier", { before });
  }

  #alignTimelineWindow(sessionId: Uuid): void {
    const floor = this.#timelineFloors.get(sessionId);
    if (floor === undefined || [...this.#pendingReads.values()].some(pending => pending.sessionId === sessionId)) return;
    const thread = this.#state.threads.get(sessionId);
    const message = thread?.page?.earliestCursor;
    const history = thread?.nextBeforeHistory;
    if (thread?.historyHasEarlier === true && history != null && BigInt(history) > BigInt(floor)) {
      this.loadEarlierHistory(sessionId);
    } else if (thread?.page?.hasEarlier === true && message?.sequence !== undefined && BigInt(message.sequence) > BigInt(floor)) {
      this.#requestPage(sessionId, "earlier", { before: message });
    } else {
      this.#timelineFloors.delete(sessionId);
      this.#commit({ ...this.#state });
    }
  }

  /**
   * 向上翻页（结构化历史）：续取游标是上一页最早一条的**会话内序号**。
   *
   * 与消息翻页是两套独立游标：它不动消息页（D2）。未协商到 `core.session-history.v1`
   * 时不发请求——未协商却请求该 include 会得到明确的不支持类错误，而不是空结果。
   */
  loadEarlierHistory(sessionId: Uuid): void {
    if (!this.#state.selectedFeatures.includes(SESSION_HISTORY_FEATURE)) return;
    const beforeHistory = this.#state.threads.get(sessionId)?.nextBeforeHistory ?? null;
    if (beforeHistory === null ||
      [...this.#pendingReads.values()].some((pending) => pending.sessionId === sessionId)) return;
    this.#requestPage(sessionId, "earlier_history", { beforeHistory });
  }

  /**
   * 重新取得服务端确认的当前有效交互（刷新 / 换设备后）。
   *
   * 走**专门的** `refresh_interactions` 读取：协商 `core.pending-interactions.v1` 后只请求
   * `pending_interactions`，结果只替换待处理交互这一份资源——`config_options`/`capabilities`
   * 来自首屏读取，不能被这次刷新顺手清空。`effectiveInteraction` 会重建卡片的可用性；
   * 服务端不再确认的卡片改标「不再有效」，因此点击一定有可见结果。
   *
   * 未协商到该 feature、已有读取在途或没有已认证连接时不派发命令，返回可呈现的原因。
   */
  refreshInteractions(sessionId: Uuid): CommandOutcome {
    if (!this.#state.selectedFeatures.includes(PENDING_INTERACTIONS_FEATURE)) {
      return { ok: false, reason: "当前连接未协商 core.pending-interactions.v1，无法重新取得服务端确认的请求" };
    }
    if ([...this.#pendingReads.values()].some((pending) => pending.sessionId === sessionId)) {
      return { ok: false, reason: "已有一次会话读取在进行中，请等它完成" };
    }
    const requestId = this.#requestPage(sessionId, "refresh_interactions");
    if (requestId === null) return NOT_CONNECTED;
    return { ok: true, requestId };
  }

  /** 加载模式清单（`session.mode.list`）：模型/模式分节的「可选项」来源。 */
  loadModes(sessionId: Uuid): CommandOutcome {
    const requestId = this.#dispatchIntent({
      sessionId,
      build: (id) => ({ requestId: id, command: "session.mode.list", sessionId, payload: {} }),
    });
    return requestId === null ? NOT_CONNECTED : { ok: true, requestId };
  }

  /**
   * 取消当前轮次。
   *
   * 取消的呈现以服务端终态为准：这里只登记 `requestId` 并派发命令，`confirmed` 只能由终态产生
   * （`conversation-input-model.ts` 的 `cancelOf`）。没有进行中的轮次时**不发命令**。
   */
  cancelTurn(sessionId: Uuid): CommandOutcome {
    const active = activeTurnOf(this.#recordsOf(sessionId));
    if (active === null) return { ok: false, reason: "当前没有进行中的轮次，不能取消" };
    const turnId = active.turnId;
    const requestId = this.#dispatchIntent({
      sessionId,
      build: (id) => ({ requestId: id, command: "session.cancel", sessionId, payload: { turnId } }),
      patch: (id) => ({ cancelRequestIds: withEntry(this.#state.cancelRequestIds, turnId, id) }),
    });
    return requestId === null ? NOT_CONNECTED : { ok: true, requestId };
  }

  /**
   * 应答授权请求（D7）。
   *
   * 只有 `answerable`（本设备亲历、请求下发了可选项）才派发命令；其余边界各自给出
   * 明确原因，因此「已被其它设备处理」不可能被重复提交、「仅历史读到」不可能有可点的按钮。
   * 提交之后呈现为**待确认**，直到服务端终态（`submitting` → `handled_here`）。
   */
  resolvePermission(sessionId: Uuid, interactionId: string, optionId: string): CommandOutcome {
    const interaction = this.#interactionOf(sessionId, interactionId);
    const blocked = responseGate(interaction);
    if (blocked !== null) return blocked;
    // 协商后服务端不得因 `requestVersion` 缺席放宽校验：版本未知就**不发**命令。
    if (interaction?.requestVersion == null && this.#state.selectedFeatures.includes(PENDING_INTERACTIONS_FEATURE)) {
      return { ok: false, reason: INTERACTION_UNAVAILABLE_LABELS.request_version_unknown };
    }
    const answers = interaction?.availability.kind === "answerable" ? interaction.availability.answers : [];
    if (!answers.some((answer) => answer.via === "option" && answer.optionId === optionId)) {
      return { ok: false, reason: "这次请求没有下发该可选项，不能提交" };
    }
    const requestId = this.#dispatchIntent({
      sessionId,
      build: (id) => ({
        requestId: id,
        command: "permission.resolve",
        sessionId,
        payload: {
          interactionId,
          optionId,
          ...(interaction?.requestVersion == null ? {} : { requestVersion: interaction.requestVersion }),
        },
      }),
      patch: (id) => ({ responseRequestIds: withEntry(this.#state.responseRequestIds, interactionId, id) }),
    });
    return requestId === null ? NOT_CONNECTED : { ok: true, requestId };
  }

  /**
   * 按当前有效请求的表单 schema 校验并提交 `values`。
   *
   * 未知必需字段/不可表达类型时**整体拒绝**（不派发命令），保留拒绝/取消等合法动作；
   * 精确整数包成 `JsonNumberLiteral`，序列化时保持原文。
   */
  submitElicitationForm(
    sessionId: Uuid,
    interactionId: string,
    input: Readonly<Record<string, FormInputValue>>,
  ): CommandOutcome {
    const interaction = this.#interactionOf(sessionId, interactionId);
    if (interaction === null) return { ok: false, reason: "这条待处理交互不在本设备的状态里" };
    if (interaction.kind !== "elicitation") return { ok: false, reason: "这不是一张表单" };
    const form = buildElicitationForm(interaction.schema);
    const validated = validateElicitationForm(form, input);
    if (!validated.ok) return { ok: false, reason: validated.detail };
    return this.respondToElicitation(sessionId, interactionId, "submit", validated.values);
  }

  /** 应答提问请求（`elicitation.respond`；`submit` 才带表单值，其余两支的 `values` 固定为 `null`）。 */
  respondToElicitation(
    sessionId: Uuid,
    interactionId: string,
    action: ElicitationResolutionAction,
    values: Record<string, unknown> | null,
  ): CommandOutcome {
    const interaction = this.#interactionOf(sessionId, interactionId);
    const blocked = responseGate(interaction);
    if (blocked !== null) return blocked;
    // 同上：协商后应答必须携带 `requestVersion`，未知时不发命令。
    if (interaction?.requestVersion == null && this.#state.selectedFeatures.includes(PENDING_INTERACTIONS_FEATURE)) {
      return { ok: false, reason: INTERACTION_UNAVAILABLE_LABELS.request_version_unknown };
    }
    const answers = interaction?.availability.kind === "answerable" ? interaction.availability.answers : [];
    if (!answers.some((answer) => answer.via === "action" && answer.action === action)) {
      return { ok: false, reason: "这次请求不支持该动作，不能提交" };
    }
    const payload: ElicitationRespondPayload =
      action === "submit"
        ? {
            interactionId,
            action,
            values,
            ...(interaction?.requestVersion == null ? {} : { requestVersion: interaction.requestVersion }),
          }
        : {
            interactionId,
            action,
            values: null,
            ...(interaction?.requestVersion == null ? {} : { requestVersion: interaction.requestVersion }),
          };
    const requestId = this.#dispatchIntent({
      sessionId,
      build: (id) => ({ requestId: id, command: "elicitation.respond", sessionId, payload }),
      patch: (id) => ({ responseRequestIds: withEntry(this.#state.responseRequestIds, interactionId, id) }),
    });
    return requestId === null ? NOT_CONNECTED : { ok: true, requestId };
  }

  /**
   * 切换模型取值（`session.config.set`）。
   *
   * 乐观并发由 `expectedVersion` 承担：版本还没取到时**不发命令**（编一个版本号会让服务端把
   * 它当成真实的并发基线）。切换的呈现是 `switching`，直到服务端确认。
   */
  setConfigValue(sessionId: Uuid, configId: string, value: string | boolean): CommandOutcome {
    const version = this.#configSourceOf(sessionId).version;
    if (version === null) return { ok: false, reason: "尚未取到配置版本，无法提交切换" };
    const requestId = this.#dispatchIntent({
      sessionId,
      build: (id) => ({
        requestId: id,
        command: "session.config.set",
        sessionId,
        expectedVersion: version,
        payload: { configId, value },
      }),
      patch: (id) => ({ switchIntents: withEntry(this.#state.switchIntents, sessionId, { requestId: id, kind: "model" }) }),
    });
    return requestId === null ? NOT_CONNECTED : { ok: true, requestId };
  }

  /**
   * 切换模式（`session.mode.set`）；版本与当前值同源：最新的 `session.mode.changed` 优先于
   * `session.mode.list` 的结果（模式清单未加载即不发命令——没有可选项的切换不是切换）。
   */
  setMode(sessionId: Uuid, modeId: string): CommandOutcome {
    const mode = this.#state.modeStates.get(sessionId);
    if (mode === undefined) return { ok: false, reason: "尚未取到模式清单，无法提交切换" };
    const requestId = this.#dispatchIntent({
      sessionId,
      build: (id) => ({
        requestId: id,
        command: "session.mode.set",
        sessionId,
        expectedVersion: this.#modeChangeOf(sessionId)?.version ?? mode.version,
        payload: { modeId },
      }),
      patch: (id) => ({ switchIntents: withEntry(this.#state.switchIntents, sessionId, { requestId: id, kind: "mode" }) }),
    });
    return requestId === null ? NOT_CONNECTED : { ok: true, requestId };
  }

  /** 更新草稿（只在内存：`no-content-cache` 边界，D10）。空串即清空。 */
  setDraft(sessionId: Uuid, text: string): void {
    const drafts = new Map(this.#state.drafts);
    if (text.length === 0) drafts.delete(sessionId);
    else drafts.set(sessionId, text);
    this.#commit({ ...this.#state, drafts });
  }

  /** 当前草稿；未输入过为空串。 */
  draftOf(sessionId: Uuid): string {
    return this.#state.drafts.get(sessionId) ?? "";
  }

  // ── 会话标题（`session.title.set`，design D4） ──────────────────────────────

  /**
   * 顶部标题的编辑视图。
   *
   * 只返回「需要呈现点什么」时的模型：编辑中/提交中/失败，或一条可归因原会话的离页失败
   * 提示。读取的永远是 `SessionSummary.title`（服务端下发的有效标题），客户端不另存
   * Agent 标题、也不推导第二套优先级。
   */
  titleEditorView(sessionId: Uuid): SessionTitleEditorView | null {
    return buildSessionTitleEditorView({
      sessionId,
      editor: this.#state.titleEditor,
      notice: this.#state.titleNotices.get(sessionId) ?? null,
    });
  }

  /**
   * 双击顶部标题：进入原地编辑。
   *
   * 输入框为空，当前有效标题只作为灰色占位（占位不是真实输入）；不输入直接确认就保留旧标题，
   * 不派发命令。未协商 `core.session-title.v1`、离线或来源离线时只记录不可用原因，阶段仍是
   * `viewing`——不进入可保存状态，也就不可能显示本地保存成功。
   */
  beginTitleEdit(sessionId: Uuid): void {
    const open = this.#state.titleEditor;
    if (open.sessionId === sessionId && open.phase !== "viewing") return;
    // 路由切换的一帧窗口里仍可能握着上一会话的编辑：先按离页规则收口原会话，再进入新会话。
    if (open.sessionId !== null && open.sessionId !== sessionId && open.phase !== "viewing") {
      this.leaveConversationSession(open.sessionId);
    }
    const editor = this.#state.titleEditor;
    const summary = this.#findSession(sessionId);
    const titleNotices = new Map(this.#state.titleNotices);
    titleNotices.delete(sessionId);
    this.#commit({
      ...this.#state,
      titleNotices,
      titleEditor: beginTitleEditing(editor, {
        sessionId,
        effectiveTitle: summary?.title ?? null,
        unavailableReason: this.#titleUnavailableReason(sessionId),
      }),
    });
  }

  /** 修改草稿（只影响当前会话的编辑器，不碰权威摘要）。 */
  setTitleDraft(sessionId: Uuid, draft: string): void {
    const next = editTitleDraft(this.#state.titleEditor, sessionId, draft);
    if (next === this.#state.titleEditor) return;
    this.#commit({ ...this.#state, titleEditor: next });
  }

  /**
   * 提交一次编辑（Enter、失焦与离页共用这一条出口）。
   *
   * 非法输入、离线/未协商都**不派发命令**，保留输入框并显示原因；提交中再调用是 no-op，
   * 因此 Enter 紧随其后的失焦不会产生第二条命令或第二个 `requestId`。同值错误后的重复
   * 失焦/离页同样不再派发（显式 Enter 才重试）。
   */
  commitTitleEdit(sessionId: Uuid, reason: SessionTitleCommitReason = "enter"): void {
    const editor = this.#state.titleEditor;
    if (!shouldAttemptTitleCommit(editor, sessionId, reason)) return;
    // 不输入（含只输入空白）直接确认 = 使用旧标题：退出编辑、不派发命令。旧标题只作为灰色
    // 占位呈现，不是真实输入；只有用户输入非空文本才提交。
    if (editor.draft.trim().length === 0) {
      this.#commit({
        ...this.#state,
        titleEditor: abandonTitleEditing(editor, sessionId),
      });
      return;
    }
    const unavailable = this.#titleUnavailableReason(sessionId);
    if (unavailable !== null) {
      this.#commit({
        ...this.#state,
        titleEditor: markTitleEditError(editor, { sessionId, reason: unavailable }),
      });
      return;
    }
    const validation = validateSessionTitle(editor.draft);
    if (!validation.ok) {
      this.#commit({
        ...this.#state,
        titleEditor: markTitleEditError(editor, { sessionId, reason: validation.reason }),
      });
      return;
    }
    const baseVersion = this.#findSession(sessionId)?.version ?? null;
    const requestId = this.#dispatchIntent({
      sessionId,
      build: (id) => ({
        requestId: id,
        command: "session.title.set",
        sessionId,
        payload: { title: validation.title },
      }),
      patch: (id) => {
        const titleCommits = new Map(this.#state.titleCommits);
        titleCommits.set(id, {
          requestId: id,
          sessionId,
          title: validation.title,
          baseVersion,
          leftPage: false,
          terminal: null,
        });
        return {
          titleCommits,
          titleEditor: markTitleSubmitting(this.#state.titleEditor, {
            sessionId,
            requestId: id,
            title: validation.title,
          }),
        };
      },
    });
    if (requestId === null) {
      this.#commit({
        ...this.#state,
        titleEditor: markTitleEditError(this.#state.titleEditor, {
          sessionId,
          reason: "尚未建立已认证连接，无法提交标题修改。",
        }),
      });
    }
  }

  /**
   * Escape：取消编辑并丢弃草稿；已发出的命令不在客户端撤销。
   *
   * 提交中按 Escape 同样退出编辑（只是关掉输入框，不撤回命令）；该次提交转为「用户已离开」
   * 归属，失败只留可归因提示，成功则等摘要落地后由列表/头部呈现。
   */
  cancelTitleEdit(sessionId: Uuid): void {
    const editor = this.#state.titleEditor;
    if (editor.phase === "submitting" && editor.sessionId === sessionId && editor.requestId !== null) {
      const titleCommits = new Map(this.#state.titleCommits);
      const commit = titleCommits.get(editor.requestId);
      if (commit !== undefined) titleCommits.set(editor.requestId, { ...commit, leftPage: true });
      this.#commit({
        ...this.#state,
        titleCommits,
        titleEditor: abandonTitleEditing(editor, sessionId),
      });
      return;
    }
    const next = cancelTitleEditing(editor, sessionId);
    if (next === editor) return;
    this.#commit({ ...this.#state, titleEditor: next });
  }

  /**
   * 离开当前对话页（切换会话/返回目录/卸载）。
   *
   * 先为原会话**尝试**提交一次，再让调用方照常导航——不等待网络结果锁住页面。随后放弃
   * 输入（不保存跨页草稿）；仍在途的命令按原 `requestId`/`sessionId` 归属，失败只记一条
   * 可归因原会话的提示，不跳回、不恢复草稿。切回原会话时总是只读当前标题。
   */
  leaveConversationSession(sessionId: Uuid): void {
    if (this.#activeConversationSession === sessionId) {
      const remote = remoteRefOf(this.#state, sessionId);
      if (remote !== null) this.#sync.unsubscribeRemote(remote);
      this.#activeConversationSession = null;
      this.#activeFileRefreshNeeded = false;
      this.#expandedFileDirectories.delete(sessionId);
      // 离页立即停止续租并声明离开；只清当前选择，历史与草稿保留在内存里。
      if (this.#state.lifecycle.selectedSessionId === sessionId) {
        this.#cancelViewRenewal();
        this.#commit({
          ...this.#state,
          lifecycle: { ...this.#state.lifecycle, selectedSessionId: null },
        });
        this.#declareView(null, true);
      }
    }
    const editor = this.#state.titleEditor;
    if (editor.sessionId !== sessionId) return;
    if (editor.phase === "editing" || editor.phase === "editing-error") {
      this.commitTitleEdit(sessionId, "navigation");
    }
    const after = this.#state.titleEditor;
    if (after.sessionId !== sessionId || after.phase === "viewing") return;
    const titleCommits = new Map(this.#state.titleCommits);
    if (after.requestId !== null) {
      const commit = titleCommits.get(after.requestId);
      if (commit !== undefined && !commit.leftPage) {
        titleCommits.set(after.requestId, { ...commit, leftPage: true });
      }
    }
    this.#commit({
      ...this.#state,
      titleCommits,
      titleEditor: abandonTitleEditing(after, sessionId),
    });
  }

  /** 当前是否允许发起一次新的标题提交；`null` 表示允许。 */
  #titleUnavailableReason(sessionId: Uuid): string | null {
    if (!this.#state.selectedFeatures.includes(SESSION_TITLE_FEATURE)) {
      return "此主机未协商标题修改能力（core.session-title.v1），当前不能编辑标题。";
    }
    if (this.#state.connection.state !== "online") {
      return "当前离线，标题修改不可用；恢复连接后再试。";
    }
    const summary = this.#findSession(sessionId);
    if (summary?.origin.kind === "remote" && !summary.origin.online) {
      return "会话所属节点当前离线，标题修改不可用。";
    }
    return null;
  }

  /**
   * 连接在提交期间断开：仍在当前页的编辑器保留输入并说明「结果未知」。
   *
   * 这不是终态失败——命令记录保持未完成，重连后用原 `requestId` 查询；提示里明确写清
   * 不会自动重发，避免用户把「已发出」误读成「一定没保存」。
   */
  #withTitleConnectionLost(state: ClientState): ClientState {
    const editor = state.titleEditor;
    if (editor.phase !== "submitting" || editor.requestId === null) return state;
    const commit = state.titleCommits.get(editor.requestId);
    if (commit === undefined || commit.leftPage) return state;
    return {
      ...state,
      titleEditor: markTitleEditError(editor, {
        sessionId: commit.sessionId,
        reason: "连接已断开，标题修改结果未知；恢复连接后会查询原请求，不会自动重发。",
        requestId: commit.requestId,
        submittedTitle: commit.title,
      }),
    };
  }

  /**
   * 标题命令的终态归并。
   *
   * - `accepted` 只说明 Owner 收到了命令，**不**退出编辑、也不显示完成；
   * - `completed` 进入「等待权威摘要」，由 {@link #settleTitleCommits} 在摘要反映这次保存时
   *   退出编辑（`terminalEventId` 可能是 Owner 的事件标识，不能拿去本地事件表里找）；
   * - `failed`/`rejected` 保留输入并显示原因；`uncertain` 同样保留，并在重连后查询原请求；
   * - 只有 `requestId` 与编辑器都指向同一次提交时才改编辑器——迟到结果不会覆盖新会话/新提交。
   */
  #withTitleTerminal(
    state: ClientState,
    result: {
      readonly requestId: Uuid;
      readonly status: CommandResultStatus;
      readonly error: PublicError | null;
    },
  ): ClientState {
    const commit = state.titleCommits.get(result.requestId);
    if (commit === undefined || result.status === "accepted") return state;
    this.#titleStatusQueries.delete(result.requestId);
    const titleCommits = new Map(state.titleCommits);
    if (result.status === "completed") {
      titleCommits.set(commit.requestId, { ...commit, terminal: "completed" });
      return { ...state, titleCommits };
    }
    const reason = titleFailureText(result.status, result.error);
    let next: ClientState = { ...state, titleCommits };
    if (result.status === "uncertain") {
      // 结果未知：保留原请求，重连后用 `command.status` 查询；不自动换号重发。
      titleCommits.set(commit.requestId, { ...commit, terminal: "uncertain" });
    } else {
      titleCommits.delete(commit.requestId);
    }
    const editor = state.titleEditor;
    if (
      editor.sessionId === commit.sessionId &&
      editor.requestId === commit.requestId &&
      editor.phase !== "viewing"
    ) {
      next = {
        ...next,
        titleEditor: markTitleEditError(editor, {
          sessionId: commit.sessionId,
          reason,
          requestId: result.status === "uncertain" ? commit.requestId : null,
          submittedTitle: commit.title,
        }),
      };
    } else if (commit.leftPage) {
      const titleNotices = new Map(next.titleNotices);
      titleNotices.set(commit.sessionId, reason);
      next = { ...next, titleNotices };
    }
    return next;
  }

  /**
   * 一次提交收束：`completed` 的提交遇到「标题等于保存值或版本更新」的权威摘要后才退出编辑。
   *
   * 摘要在终态之前到达也没关系——终态到达时这里会看到当前摘要；摘要在终态之后到达则在下一次
   * 快照/`session.updated` 归并后收束。离页的提交只清掉在途记录与失败提示，不恢复任何草稿。
   */
  #settleTitleCommits(state: ClientState): ClientState {
    if (state.titleCommits.size === 0) return state;
    const summaries = state.resources?.sessions ?? [];
    let titleCommits: Map<Uuid, SessionTitlePendingCommit> | null = null;
    let titleEditor = state.titleEditor;
    let titleNotices: Map<Uuid, string> | null = null;
    for (const commit of state.titleCommits.values()) {
      if (commit.terminal !== "completed") continue;
      const summary = summaries.find((item) => item.sessionId === commit.sessionId);
      if (summary === undefined || !titleCommitReflected(commit, summary)) continue;
      if (titleCommits === null) titleCommits = new Map(state.titleCommits);
      titleCommits.delete(commit.requestId);
      if (
        titleEditor.sessionId === commit.sessionId &&
        titleEditor.requestId === commit.requestId &&
        (titleEditor.phase === "submitting" || titleEditor.phase === "editing-error")
      ) {
        titleEditor = abandonTitleEditing(titleEditor, commit.sessionId);
      }
      if (state.titleNotices.has(commit.sessionId)) {
        if (titleNotices === null) titleNotices = new Map(state.titleNotices);
        titleNotices.delete(commit.sessionId);
      }
    }
    if (titleCommits === null) return state;
    return {
      ...state,
      titleCommits,
      titleEditor,
      ...(titleNotices === null ? {} : { titleNotices }),
    };
  }

  /**
   * 重连后查询结果未知的标题命令（原 `requestId`，不重发、不换号）。
   *
   * 每条命令在**当前连接**上只查一次；新认证后清空标记，因此再次断线重连还会再查。
   */
  #drainTitleStatusQueries(): void {
    for (const commit of this.#state.titleCommits.values()) {
      if (commit.terminal === "completed" || this.#titleStatusQueries.has(commit.requestId)) continue;
      const remote = remoteRefOf(this.#state, commit.sessionId);
      const payload: {
        readonly targetRequestId: Uuid;
        readonly remoteTarget?: { readonly ownerNodeId: Uuid; readonly sessionId: Uuid | null };
      } = remote !== null && this.#state.selectedFeatures.includes(STATELESS_ORIGIN_FEATURE)
        ? {
            targetRequestId: commit.requestId,
            remoteTarget: { ownerNodeId: remote.ownerNodeId, sessionId: commit.sessionId },
          }
        : { targetRequestId: commit.requestId };
      const dispatched = this.#dispatchIntent({
        sessionId: commit.sessionId,
        build: (id) => ({ requestId: id, command: "command.status", payload }),
      });
      if (dispatched !== null) this.#titleStatusQueries.add(commit.requestId);
    }
  }

  // ── 附件与媒体（WP6） ─────────────────────────────────────────────────────

  /**
   * 打开平台选择器并把选中项加入草稿（**不上传**）。
   *
   * 选择阶段只在内存：附件的平台句柄保存在私有表，公开状态只有元数据与本机预览 URL。
   * 取消选择器返回空数组，不产生任何状态变化。
   */
  async pickAttachments(sessionId: Uuid, kind: AttachmentKind): Promise<CommandOutcome> {
    const unavailable = this.#attachmentsUnavailable();
    if (unavailable !== null) return unavailable;
    const handles = await this.#attachmentMedia!.pick(kind);
    return this.#addAttachmentHandles(sessionId, handles);
  }

  /** 从粘贴事件取出图片/文件（页面只上抛事件，不解析 DataTransfer）。 */
  async pasteAttachments(sessionId: Uuid, event: unknown): Promise<CommandOutcome> {
    const unavailable = this.#attachmentsUnavailable();
    if (unavailable !== null) return unavailable;
    const handles = await this.#attachmentMedia!.fromPaste(event);
    if (handles.length === 0) return { ok: false, reason: "粘贴内容里没有可添加的图片或文件" };
    return this.#addAttachmentHandles(sessionId, handles);
  }

  /** 移除一个已选附件（立即撤销本机预览 URL；不触碰已封存的服务端内容）。 */
  removeAttachment(sessionId: Uuid, localId: string): void {
    const attachments = this.#state.attachments.get(sessionId) ?? [];
    const removed = attachments.find((item) => item.localId === localId);
    if (removed === undefined) return;
    if (removed.previewUrl !== null) this.#attachmentMedia?.releaseUrl(removed.previewUrl);
    this.#attachmentHandles.delete(localId);
    const remaining = attachments.filter((item) => item.localId !== localId);
    const next = new Map(this.#state.attachments);
    if (remaining.length === 0) next.delete(sessionId);
    else next.set(sessionId, remaining);
    // 选择变化使上一次发送尝试的稳定 requestId 不再适用（内容不同，不能复用幂等键）。
    const runs = new Map(this.#state.attachmentRuns);
    runs.delete(sessionId);
    this.#commit({ ...this.#state, attachments: next, attachmentRuns: runs });
    // 移除同样要落草稿：否则刷新会把已移除的附件带回来（迟到的旧版本不得复活内容）。
    if (this.#state.preparations.has(sessionId)) this.#schedulePreparationSave(sessionId);
  }

  /** 添加一个工作目录相对路径引用（只在本机校验形状；边界由服务端最终判定）。 */
  addPathRef(sessionId: Uuid, path: string): CommandOutcome {
    const candidate = path.trim();
    if (!isWorkspacePathRef(candidate)) {
      return { ok: false, reason: "只能引用当前会话工作目录内的相对路径（不接受绝对路径、`..` 或冒号）" };
    }
    const existing = this.#state.pathRefs.get(sessionId) ?? [];
    if (existing.includes(candidate)) return { ok: false, reason: "这个路径已经在草稿里" };
    const pathRefs = new Map(this.#state.pathRefs);
    pathRefs.set(sessionId, [...existing, candidate]);
    const runs = new Map(this.#state.attachmentRuns);
    runs.delete(sessionId);
    this.#commit({ ...this.#state, pathRefs, attachmentRuns: runs });
    return { ok: true, requestId: this.#ids.uuid() };
  }

  /** 移除一个路径引用。 */
  removePathRef(sessionId: Uuid, path: string): void {
    const existing = this.#state.pathRefs.get(sessionId) ?? [];
    const remaining = existing.filter((item) => item !== path);
    const pathRefs = new Map(this.#state.pathRefs);
    if (remaining.length === 0) pathRefs.delete(sessionId);
    else pathRefs.set(sessionId, remaining);
    this.#commit({ ...this.#state, pathRefs });
  }

  /**
   * 读取一条已封存附件并生成本地对象 URL。
   *
   * 分块读取（单块 ≤32 KiB）后一次性创建 Blob URL：呈现图片/音频/下载都需要完整字节，但读取
   * 过程不把整个文件当作单个请求。失败是显式状态，不画空占位。
   */
  async loadAttachmentMedia(
    sessionId: Uuid,
    attachmentId: string,
    mimeType: string,
  ): Promise<void> {
    const existing = this.#mediaLoads.get(attachmentId);
    if (existing !== undefined) {
      // 同一代次：共享在途读取。释放后重建（代次已变）：先等旧读取结束，再发起新读取——
      // 绝不能并发发两个 `attachment.read offset=0`（单等待槽会互相覆盖）。
      if (existing.generation === (this.#mediaReads.get(attachmentId) ?? 0)) return existing.promise;
      await existing.promise;
    }
    const generation = (this.#mediaReads.get(attachmentId) ?? 0) + 1;
    this.#mediaReads.set(attachmentId, generation);
    const promise = this.#loadAttachmentMediaOnce(sessionId, attachmentId, mimeType, generation);
    this.#mediaLoads.set(attachmentId, { generation, promise });
    try {
      await promise;
    } finally {
      if (this.#mediaLoads.get(attachmentId)?.promise === promise) {
        this.#mediaLoads.delete(attachmentId);
      }
    }
  }

  async #loadAttachmentMediaOnce(
    sessionId: Uuid,
    attachmentId: string,
    mimeType: string,
    generation: number,
  ): Promise<void> {
    const media = this.#attachmentMedia;
    const transfer = this.#attachmentTransfer;
    if (media === null || transfer === null) {
      this.#setMedia(attachmentId, {
        kind: "failed",
        sessionId,
        detail: "当前连接未装配附件读取能力。",
      });
      return;
    }
    // 代次由调用方递增：上一次读取（或释放）之后到达的结果一律作废。
    const previous = this.#state.attachmentMedia.get(attachmentId);
    if (previous !== undefined && previous.kind === "ready") media.releaseUrl(previous.url);
    this.#setMedia(attachmentId, { kind: "loading", sessionId });
    try {
      const chunks: Uint8Array[] = [];
      let offset = 0n;
      for (;;) {
        let chunk: { readonly data: Uint8Array; readonly length: string; readonly eof: boolean; readonly totalLength: string } | null = null;
        for (let attempt = 0; ; attempt += 1) {
          try {
            chunk = await transfer.read({
              attachmentId,
              sessionId,
              offset: offset.toString(),
              maxLength: ATTACHMENT_READ_MAX_LENGTH,
            });
            break;
          } catch (error) {
            if (
              attempt + 1 >= ATTACHMENT_READY_ATTEMPTS ||
              !isTransientAttachmentReadiness(error) ||
              this.#mediaReads.get(attachmentId) !== generation
            ) {
              throw error;
            }
            await delay(ATTACHMENT_READY_DELAY_MS);
          }
        }
        if (chunk === null) throw new Error("读取附件失败");
        chunks.push(chunk.data);
        if (chunk.eof) break;
        const length = BigInt(chunk.length);
        if (length === 0n) throw new Error("服务端返回了零长度且未标记 EOF 的读取块");
        offset += length;
        if (BigInt(chunk.totalLength) > 0n && offset > BigInt(chunk.totalLength)) {
          throw new Error("读取偏移超过附件总长度");
        }
      }
      const bytes = concatBytes(chunks);
      const url = media.createObjectUrl(bytes, mimeType);
      if (url === null) throw new Error("当前平台不能创建 Blob URL");
      // 迟到的读取不能覆盖后一次读取，也不能把已释放的 URL 重新挂上：立即撤销自己的产物。
      if (this.#mediaReads.get(attachmentId) !== generation) {
        media.releaseUrl(url);
        return;
      }
      this.#setMedia(attachmentId, {
        kind: "ready",
        sessionId,
        url,
        mimeType,
        byteLength: String(bytes.length),
      });
    } catch (error) {
      if (this.#mediaReads.get(attachmentId) !== generation) return;
      this.#setMedia(attachmentId, {
        kind: "failed",
        sessionId,
        detail: error instanceof Error ? error.message : "读取附件失败",
      });
    }
  }

  /** 立即撤销一条媒体的对象 URL。 */
  releaseAttachmentMedia(attachmentId: string): void {
    // 作废在途读取：它回来时不能重新挂上已撤销的对象 URL。保留在途表项，让重建的加载
    // 等待旧读取结束再发起新读取（否则两个 read offset=0 会争用同一等待槽）。
    this.#mediaReads.set(attachmentId, (this.#mediaReads.get(attachmentId) ?? 0) + 1);
    const current = this.#state.attachmentMedia.get(attachmentId);
    if (current !== undefined && current.kind === "ready") this.#attachmentMedia?.releaseUrl(current.url);
    if (!this.#state.attachmentMedia.has(attachmentId)) return;
    const next = new Map(this.#state.attachmentMedia);
    next.delete(attachmentId);
    this.#commit({ ...this.#state, attachmentMedia: next });
  }

  /** 撤销某个会话的全部对象 URL（切换会话 / 关闭页面时调用）。 */
  releaseSessionMedia(sessionId: Uuid): void {
    const next = new Map(this.#state.attachmentMedia);
    let changed = false;
    for (const [id, state] of next) {
      if (state.sessionId !== sessionId) continue;
      this.#mediaReads.set(id, (this.#mediaReads.get(id) ?? 0) + 1);
      if (state.kind === "ready") this.#attachmentMedia?.releaseUrl(state.url);
      next.delete(id);
      changed = true;
    }
    if (changed) this.#commit({ ...this.#state, attachmentMedia: next });
  }

  #attachmentsUnavailable(): CommandOutcome | null {
    if (this.#identity.connectionId === null) return NOT_CONNECTED;
    if (!this.#state.selectedFeatures.includes(ATTACHMENT_FEATURE)) {
      return { ok: false, reason: "当前主机未协商附件传输（core.attachment-transfer.v1）" };
    }
    if (this.#attachmentMedia === null || this.#attachmentTransfer === null) {
      return { ok: false, reason: "附件能力尚未装配" };
    }
    return null;
  }

  #addAttachmentHandles(sessionId: Uuid, handles: readonly AttachmentFileHandle[]): CommandOutcome {
    if (handles.length === 0) return { ok: false, reason: "没有选中任何附件" };
    const existing = [...(this.#state.attachments.get(sessionId) ?? [])];
    for (const handle of handles) {
      const transferId = this.#ids.uuid();
      this.#attachmentHandles.set(handle.localId, handle);
      existing.push({
        localId: handle.localId,
        transferId,
        attachmentId: null,
        kind: handle.kind,
        fileName: handle.fileName,
        mediaType: handle.mediaType,
        byteLength: handle.byteLength,
        status: "selected",
        uploadedBytes: "0",
        sha256: null,
        error: null,
        previewUrl: this.#attachmentMedia?.previewUrl(handle) ?? null,
      });
    }
    const attachments = new Map(this.#state.attachments);
    attachments.set(sessionId, existing);
    const runs = new Map(this.#state.attachmentRuns);
    runs.delete(sessionId);
    this.#commit({ ...this.#state, attachments, attachmentRuns: runs });
    // 附件选择是准备草稿的一部分：刷新恢复必须包含它（不依赖用户再输入一次）。
    if (this.#state.preparations.has(sessionId)) this.#schedulePreparationSave(sessionId);
    return { ok: true, requestId: this.#ids.uuid() };
  }

  #setMedia(attachmentId: string, state: AttachmentMediaState): void {
    const attachmentMedia = new Map(this.#state.attachmentMedia);
    attachmentMedia.set(attachmentId, state);
    this.#commit({ ...this.#state, attachmentMedia });
  }

  #patchAttachment(
    sessionId: Uuid,
    localId: string,
    update: (attachment: DraftAttachment) => DraftAttachment,
  ): void {
    const current = this.#state.attachments.get(sessionId);
    if (current === undefined) return;
    let changed = false;
    const next = current.map((attachment) => {
      if (attachment.localId !== localId) return attachment;
      changed = true;
      return update(attachment);
    });
    if (!changed) return;
    const attachments = new Map(this.#state.attachments);
    attachments.set(sessionId, next);
    this.#commit({ ...this.#state, attachments });
  }

  #clearSessionAttachments(sessionId: Uuid): ClientState {
    const attachments = new Map(this.#state.attachments);
    for (const attachment of attachments.get(sessionId) ?? []) {
      if (attachment.previewUrl !== null) this.#attachmentMedia?.releaseUrl(attachment.previewUrl);
      this.#attachmentHandles.delete(attachment.localId);
    }
    attachments.delete(sessionId);
    const runs = new Map(this.#state.attachmentRuns);
    runs.delete(sessionId);
    return { ...this.#state, attachments, attachmentRuns: runs };
  }

  // ── 文件栏（WP4） ──────────────────────────────────────────────────────────

  /**
   * 文件栏 view model：两来源 + 互斥内容态。
   *
   * 会话改动不进文件栏（时间线继续从已加载记录投影结构化差异与诊断），因此这里不读历史标记，
   * 也不派发会话变更专属的历史刷新。
   */
  sessionFilesPage(sessionId: Uuid): SessionFilesPanelModel {
    const reason = this.#fileSourceUnsupported(sessionId);
    const sources = sourcesOf(this.#state.sessionFiles, sessionId);
    const state = reason === null ? this.#state.sessionFiles : {
      ...this.#state.sessionFiles,
      sources: new Map(this.#state.sessionFiles.sources).set(sessionId, {
        all: { ...sources.all, unsupported: reason },
        git: { ...sources.git, unsupported: reason },
      }),
    };
    return buildSessionFilesPanelModel({
      sessionId,
      state,
      connectionOnline: this.#state.connection.state === "online",
    });
  }

  /**
   * 通知仓库当前对话页属于哪个会话（路由切换时调用）。
   *
   * 会话变化时丢弃旧会话的文件读取并回到对话态：即使旧请求之后才返回，也会因为不在在途表
   * 而无法重新打开文件。草稿、时间线与展开状态不在本切片里，因此不受影响。
   */
  setActiveConversationSession(sessionId: Uuid): void {
    if (this.#activeConversationSession === sessionId) return;
    const previous = this.#activeConversationSession;
    this.#activeConversationSession = sessionId;
    this.#activeFileRefreshNeeded = true;
    this.#drainActiveFileRefresh();
    const content = this.#state.sessionFiles.content;
    if (content.kind === "file" && content.context.sessionId !== sessionId) {
      const generation = this.#state.sessionFiles.generation + 1;
      this.#dropPendingContentQueries();
      this.#commit(this.#withFileContent(this.#state, { kind: "conversation" }, generation));
    }
    // 本机生命周期：换选择即换查看者。先提交选择，再在同一连接上「先声明查看、后派发恢复」；
    // 旧会话的续租计时器立即停掉（旧 viewer 不得被迟到计时器重新续租）。
    if (this.#state.lifecycle.selectedSessionId !== sessionId) {
      this.#cancelViewRenewal();
      this.#commit({
        ...this.#state,
        lifecycle: { ...this.#state.lifecycle, selectedSessionId: sessionId },
      });
      this.#ensureLifecycle("enter");
    }
    if (previous !== null) {
      const previousRef = remoteRefOf(this.#state, previous);
      if (previousRef !== null) this.#sync.unsubscribeRemote(previousRef);
    }
    const ref = remoteRefOf(this.#state, sessionId);
    if (ref === null) return;
    if (this.#state.remoteSessions.get(sessionId)?.sourceState === "revoked") return;
    // 认证前先登记订阅意图（门面保存 wanted），feature 协商结果在认证后才可见。
    this.#sync.subscribeRemote(ref);
    this.#queueRemoteRebuild(sessionId);
    this.#drainQueuedRemoteRebuilds();
  }

  /** 切换文件栏来源；归一后的来源只会是全部文件或 Git 变更，进入时立即刷新。 */
  selectFileSource(sessionId: Uuid, source: FileSource): void {
    this.#activeConversationSession = sessionId;
    const normalized = normalizeFileSource(source);
    const selectedSources = new Map(this.#state.sessionFiles.selectedSources);
    selectedSources.set(sessionId, normalized);
    this.#commit({ ...this.#state, sessionFiles: { ...this.#state.sessionFiles, selectedSources } });
    this.#refreshFileSource(sessionId, normalized);
  }

  /** 手动刷新当前来源：只发对应来源的目录/Git 查询。 */
  setExpandedFileDirectories(sessionId: Uuid, paths: readonly string[]): void {
    this.#expandedFileDirectories.set(sessionId, [...new Set(paths)]);
  }

  /** 由平台生命周期端口通知恢复前台，不直接依赖 document/window。 */
  refreshActiveSessionFiles(): void {
    this.#activeFileRefreshNeeded = true;
    this.#drainActiveFileRefresh();
  }

  #drainActiveFileRefresh(): void {
    const sessionId = this.#activeConversationSession;
    if (!this.#activeFileRefreshNeeded || sessionId === null || this.#state.connection.state !== "online" ||
        this.#state.resources === null || (this.#findSession(sessionId) === undefined &&
          !(this.#state.selectedFeatures.includes(WORKSPACE_FILES_FEATURE) && this.#workspaceFilesTarget(sessionId) !== null))) return;
    this.#activeFileRefreshNeeded = false;
    this.refreshFiles(sessionId);
  }

  refreshFiles(sessionId: Uuid): void {
    this.#activeConversationSession = sessionId;
    const source = selectedSourceOf(this.#state.sessionFiles, sessionId);
    this.#cancelFileRefreshTimer(this.#fileKey(sessionId, source));
    this.#refreshFileSource(sessionId, source);
  }

  /** 切换「隐藏忽略文件」；改变过滤口径后清掉逐层缓存并从根重新枚举。 */
  toggleHideIgnored(sessionId: Uuid): void {
    const sources = sourcesOf(this.#state.sessionFiles, sessionId);
    const hideIgnored = !sources.all.hideIgnored;
    const nextAll = { hideIgnored, directories: new Map(), unsupported: sources.all.unsupported };
    this.#dropPendingSourceQueries(sessionId, "all");
    this.#commit(this.#withSources(this.#state, sessionId, { ...sources, all: nextAll }));
    this.#refreshFileSource(sessionId, "all");
  }

  /**
   * 「全部文件」的**唯一**目录加载入口：未加载的目录取第一页，已加载且有 `next` 时取下一页。
   *
   * 首次展开子目录时没有既有缓存，因此这里不能以「目录不存在」为由静默返回；只有加载中或
   * 没有下一页（含已到末尾）才不派发。`after: undefined` 即第一页。
   */
  loadMoreAllFiles(sessionId: Uuid, path: string): void {
    if (this.#fileRefreshRounds.has(this.#fileKey(sessionId, "all"))) return;
    // 旧主机/远程导入会话没有本机目录可枚举：即使调用方拿到一个路径也不派发。
    if (this.#fileSourceUnsupported(sessionId) !== null) return;
    const directory = sourcesOf(this.#state.sessionFiles, sessionId).all.directories.get(path);
    if (directory === undefined) {
      this.#dispatchAllFilesPage(sessionId, path, undefined, false);
      return;
    }
    if (directory.loading || directory.next === null) return;
    this.#dispatchAllFilesPage(sessionId, path, directory.next, true);
  }

  /** 继续加载「Git 变更」的下一页。 */
  loadMoreGitChanges(sessionId: Uuid): void {
    if (this.#fileRefreshRounds.has(this.#fileKey(sessionId, "git"))) return;
    const git = sourcesOf(this.#state.sessionFiles, sessionId).git;
    if (git.unsupported !== null || git.next === null || git.loading) return;
    this.#dispatchGitStatus(sessionId, git.next, true);
  }

  /**
   * 打开文件内容，替换中间对话区（互斥）。
   *
   * 关闭/切换会话后的迟到响应由代次与上下文丢弃；来源在入口归一为全部文件或 Git 变更。
   * 请求代次每次都递增，因此 A 的响应不可能覆盖随后打开的 B。
   */
  openFileContent(sessionId: Uuid, source: FileSource, path: string): void {
    this.#activeConversationSession = sessionId;
    const generation = this.#state.sessionFiles.generation + 1;
    this.#dropPendingContentQueries();
    // 旧页面状态可能仍传 `session`（类型上已不存在）：同样回到全部文件，不恢复专属查看。
    const normalized = normalizeFileSource(source);
    const context: FileViewContext = { sessionId, source: normalized, path };

    const unsupported = this.#fileSourceUnsupported(sessionId);
    if (unsupported !== null) {
      this.#commit(this.#withFileContent(this.#state, {
        kind: "file",
        context,
        generation,
        stale: false,
        status: "error",
        error: unsupportedSourceError(unsupported),
      }, generation));
      return;
    }
    if (this.#identity.connectionId === null) {
      this.#commit(this.#withFileContent(this.#state, {
        kind: "file",
        context,
        generation,
        stale: false,
        status: "error",
        error: notConnectedFileError(),
      }, generation));
      return;
    }

    this.#commit(this.#withFileContent(this.#state, {
      kind: "file",
      context,
      generation,
      stale: false,
      status: "loading",
    }, generation));

    if (normalized === "all") {
      this.#dispatchFileRead({
        sessionId,
        source: normalized,
        path,
        generation,
        purpose: "preview",
        offset: "0",
        chunks: [],
      });
      return;
    }
    this.#dispatchGitDiff({ sessionId, source: normalized, path, generation });
  }

  /** 关闭文件视图，恢复对话；递增代次使所有在途响应作废。 */
  closeFileContent(): void {
    if (this.#state.sessionFiles.content.kind === "conversation") return;
    const generation = this.#state.sessionFiles.generation + 1;
    this.#dropPendingContentQueries();
    this.#commit(this.#withFileContent(this.#state, { kind: "conversation" }, generation));
  }

  /**
   * 提交一条提示（文本 + 已选附件 + 工作目录路径引用）。
   *
   * 闸门（任一不过即**不发命令**）：当前输入可用性允许发送（执行中/离线/imported 来源都会
   * 给出原因）、内容不为空、附件不是上传中/失败/能力不支持。没有附件时同步派发；有附件时
   * **先上传**，全部封存并登记清单后才派发一条携带 `submissionId` 的 `session.prompt`——
   * 任一附件失败或取消都不会触发 Agent，也不会提交部分消息（完整草稿保留）。
   *
   * 稳定 `requestId`：一次发送尝试的 `promptRequestId` 在上传前就分配，并在重试时复用；
   * 附件选择变化会使它失效（内容不同不能复用幂等键）。
   */
  submitPrompt(sessionId: Uuid, text: string): CommandOutcome {
    const page = this.conversationPage(sessionId);
    if (page === null) return { ok: false, reason: "会话不在本设备的状态里" };
    if (!page.input.canSubmit) return { ok: false, reason: page.input.disabledReason ?? "当前不能提交" };
    const trimmed = text.trim();
    const attachments = page.input.attachments.attachments;
    const pathRefs = page.input.pathRefs;
    if (trimmed.length === 0 && attachments.length === 0 && pathRefs.length === 0) {
      return { ok: false, reason: "不能提交空内容" };
    }
    // 仓库是草稿的唯一内存持有者：把这次提交的原文记进草稿，附件失败/取消时页面才能原样保留。
    if (text.length > 0 && this.#state.drafts.get(sessionId) !== text) {
      this.#commit({ ...this.#state, drafts: withEntry(this.#state.drafts, sessionId, text) });
    }
    // 上一次未成立的提交（上传失败/取消）不再代表当前输入：清掉它的本地回声，避免时间线上
    // 留下一条永远不会被权威消息接替的失败条目。已受理/不确定的条目不清理。
    this.#dropUnsettledPrompts(sessionId);

    if (attachments.length === 0) {
      const outcome = this.#dispatchPrompt({
        sessionId,
        requestId: this.#ids.uuid(),
        submissionId: null,
        text: trimmed,
        pathRefs,
        attachments: [],
      });
      if (outcome.ok) {
        const cleared = this.#clearSessionAttachments(sessionId);
        // 已派发才清空草稿；失败时完整草稿留在输入区（spec「完整草稿保留」）。
        this.#commit({ ...cleared, drafts: withoutEntry(cleared.drafts, sessionId) });
      }
      return outcome;
    }

    if (this.#uploadingSessions.has(sessionId)) {
      return { ok: false, reason: "附件正在上传，等待这一批结束" };
    }
    this.#cancelledUploads.delete(sessionId);
    const existingRun = this.#state.attachmentRuns.get(sessionId);
    const promptRequestId = existingRun?.promptRequestId ?? this.#ids.uuid();
    const submissionId = existingRun?.submissionId ?? this.#ids.uuid();
    const runs = new Map(this.#state.attachmentRuns);
    runs.set(sessionId, {
      phase: "uploading",
      promptRequestId,
      submissionId,
      manifestSealed: false,
      error: null,
    });
    // 本次输入立刻可见，并如实标注「附件上传中」：此时 Agent 尚未收到任何内容。
    this.#commit({
      ...this.#state,
      attachmentRuns: runs,
      sentPrompts: withEntry(this.#state.sentPrompts, promptRequestId, sentPromptOf({
        requestId: promptRequestId,
        sessionId,
        text: trimmed,
        attachmentNames: attachments.map((attachment) => attachment.fileName),
        createdAt: new Date(this.#now()).toISOString(),
        phase: "uploading",
      })),
    });
    this.#uploadingSessions.add(sessionId);
    void this.#uploadAndSubmit({
      sessionId,
      promptRequestId,
      submissionId,
      text: trimmed,
      pathRefs,
    });
    return { ok: true, requestId: promptRequestId };
  }

  /**
   * 取消正在进行的附件上传。
   *
   * 语义是「这一批不再继续」：中止在途传输、把未封存的项标为已取消，并**不**派发
   * `session.prompt`（Agent 从未收到任何内容）。已封存的附件留在服务端，重试时可复用。
   */
  cancelAttachmentUpload(sessionId: Uuid): CommandOutcome {
    if (!this.#uploadingSessions.has(sessionId)) {
      return { ok: false, reason: "当前没有正在进行的附件上传" };
    }
    this.#cancelledUploads.add(sessionId);
    const active = this.#activeTransfers.get(sessionId);
    if (active !== undefined) {
      try {
        this.#attachmentTransfer?.abort(active, "client_cancelled");
      } catch {
        // 连接已断开时 abort 无处可发：服务端会按闲置策略清理临时内容，本地照常标为已取消。
      }
    }
    return { ok: true, requestId: this.#ids.uuid() };
  }

  /**
   * 重试失败的附件上传。
   *
   * 失败/取消/过期的项换一个新的 `transferId` 重新开始（旧传输服务端按 24 小时闲置清理），
   * 已封存的项保持不变。失败的上传**从未派发过 prompt**，因此重试分配新的稳定 `requestId`
   * 不会造成第二次副作用。
   */
  retryAttachments(sessionId: Uuid): CommandOutcome {
    if (this.#uploadingSessions.has(sessionId)) {
      return { ok: false, reason: "附件正在上传，等到这一批结束或先取消" };
    }
    const current = this.#state.attachments.get(sessionId) ?? [];
    const retryable = current.filter(
      (attachment) =>
        attachment.status === "failed" || attachment.status === "cancelled" || attachment.status === "expired",
    );
    if (retryable.length === 0) return { ok: false, reason: "没有需要重试的附件" };
    const attachments = new Map(this.#state.attachments);
    attachments.set(
      sessionId,
      current.map((attachment) =>
        retryable.includes(attachment)
          ? {
              ...attachment,
              transferId: this.#ids.uuid(),
              attachmentId: null,
              status: "selected" as const,
              uploadedBytes: "0",
              sha256: null,
              error: null,
            }
          : attachment,
      ),
    );
    const runs = new Map(this.#state.attachmentRuns);
    runs.delete(sessionId);
    this.#commit({ ...this.#state, attachments, attachmentRuns: runs });
    return { ok: true, requestId: this.#ids.uuid() };
  }

  /** 上传全部附件、封存清单，然后派发一条 prompt；失败时不派发任何命令。 */
  async #uploadAndSubmit(input: {
    readonly sessionId: Uuid;
    readonly promptRequestId: Uuid;
    readonly submissionId: Uuid;
    readonly text: string;
    readonly pathRefs: readonly string[];
  }): Promise<void> {
    const media = this.#attachmentMedia;
    const transfer = this.#attachmentTransfer;
    const subjectId = this.#state.deviceId;
    const sessionId = input.sessionId;
    try {
      if (media === null || transfer === null) throw new Error("附件能力尚未装配");
      if (subjectId === null) throw new Error("尚未认证设备，不能上传附件");
      const attachments = this.#state.attachments.get(sessionId) ?? [];
      const transferIds: string[] = [];
      for (const attachment of attachments) {
        if (this.#cancelledUploads.has(sessionId)) throw new Error("附件上传已取消");
        if (attachment.status === "sealed" && attachment.sha256 !== null) {
          transferIds.push(attachment.transferId);
          continue;
        }
        const handle = this.#attachmentHandles.get(attachment.localId);
        if (handle === undefined) throw new Error(`${attachment.fileName} 的本机句柄已丢失，请重新选择`);
        this.#patchAttachment(sessionId, attachment.localId, (current) => ({
          ...current,
          status: "uploading",
          uploadedBytes: "0",
          error: null,
        }));
        this.#activeTransfers.set(sessionId, attachment.transferId);
        const sealed = await transfer.upload({
          sessionId,
          subjectId,
          transferId: attachment.transferId,
          fileName: attachment.fileName,
          mediaType: attachment.mediaType,
          totalLength: attachment.byteLength,
          readChunk: (offset, length) => media.readChunk(handle, offset, length),
        });
        this.#activeTransfers.delete(sessionId);
        this.#patchAttachment(sessionId, attachment.localId, (current) => ({
          ...current,
          status: "sealed",
          uploadedBytes: current.byteLength,
          attachmentId: sealed.attachmentId,
          sha256: sealed.sha256,
          error: null,
        }));
        transferIds.push(attachment.transferId);
      }

      if (this.#cancelledUploads.has(sessionId)) throw new Error("附件上传已取消");
      // 附件都已封存，清单还没登记：如实说「封存中」，仍然没有任何内容到达 Agent。
      this.#patchSentPrompt(input.promptRequestId, (prompt) => ({ ...prompt, phase: "sealing", notice: null }));
      await transfer.sealManifest({
        sessionId,
        subjectId,
        submissionId: input.submissionId,
        transferIds,
      });
      if (this.#cancelledUploads.has(sessionId)) throw new Error("附件上传已取消");
      const runs = new Map(this.#state.attachmentRuns);
      runs.set(sessionId, {
        phase: "sealed",
        promptRequestId: input.promptRequestId,
        submissionId: input.submissionId,
        manifestSealed: true,
        error: null,
      });
      this.#commit({ ...this.#state, attachmentRuns: runs });

      const sealedAttachments = this.#state.attachments.get(sessionId) ?? [];
      const outcome = this.#dispatchPrompt({
        sessionId,
        requestId: input.promptRequestId,
        submissionId: input.submissionId,
        text: input.text,
        pathRefs: this.#state.pathRefs.get(sessionId) ?? input.pathRefs,
        attachments: sealedAttachments,
      });
      if (!outcome.ok) throw new Error(outcome.reason);
      const cleared = this.#clearSessionAttachments(sessionId);
      this.#commit({ ...cleared, drafts: withoutEntry(cleared.drafts, sessionId) });
    } catch (error) {
      const cancelled = this.#cancelledUploads.has(sessionId);
      const detail = cancelled ? "附件上传已取消" : error instanceof Error ? error.message : "附件上传失败";
      const attachments = this.#state.attachments.get(sessionId) ?? [];
      for (const attachment of attachments) {
        if (attachment.status === "sealed") continue;
        this.#patchAttachment(sessionId, attachment.localId, (current) =>
          current.status === "uploading" || current.status === "selected"
            ? cancelled
              ? { ...current, status: "cancelled", error: null }
              : { ...current, status: "failed", error: detail }
            : current,
        );
      }
      const runs = new Map(this.#state.attachmentRuns);
      runs.set(sessionId, {
        phase: "failed",
        promptRequestId: input.promptRequestId,
        submissionId: input.submissionId,
        manifestSealed: false,
        error: cancelled ? "上传已取消：未向 Agent 派发任何内容，草稿与选择仍保留。" : detail,
      });
      this.#commit({ ...this.#state, attachmentRuns: runs });
      // 已显示的输入**不消失**：标成「未发送」，并说明是失败还是取消（Agent 从未收到内容）。
      this.#patchSentPrompt(input.promptRequestId, (prompt) => ({
        ...prompt,
        phase: "failed",
        notice: cancelled
          ? "上传已取消：未向 Agent 派发任何内容，输入仍可修正后重新发送。"
          : detail,
      }));
    } finally {
      this.#uploadingSessions.delete(sessionId);
      this.#activeTransfers.delete(sessionId);
      this.#cancelledUploads.delete(sessionId);
    }
  }

  /** 组装内容块并派发 `session.prompt`；空内容块时拒绝（不编造空消息）。 */
  #dispatchPrompt(input: {
    readonly sessionId: Uuid;
    readonly requestId: Uuid;
    readonly submissionId: Uuid | null;
    readonly text: string;
    readonly pathRefs: readonly string[];
    readonly attachments: readonly DraftAttachment[];
  }): CommandOutcome {
    const content: PromptContentBlock[] = [];
    for (const attachment of input.attachments) {
      if (attachment.status !== "sealed" || attachment.sha256 === null || attachment.attachmentId === null) {
        return { ok: false, reason: `${attachment.fileName} 还没有封存，不能提交` };
      }
      content.push({
        type: "attachment_ref",
        attachmentId: attachment.attachmentId,
        mediaType: attachment.mediaType,
        byteLength: attachment.byteLength,
        sha256: attachment.sha256,
      });
    }
    if (input.text.length > 0) content.push({ type: "text", text: input.text });
    for (const path of input.pathRefs) {
      content.push({ type: "workspace_path_ref", path });
    }
    if (content.length === 0) return { ok: false, reason: "不能提交空内容" };

    const requestId = this.#dispatchIntent({
      requestId: input.requestId,
      sessionId: input.sessionId,
      build: (id) => ({
        requestId: id,
        command: "session.prompt",
        sessionId: input.sessionId,
        payload:
          input.submissionId === null
            ? { content }
            : { content, submissionId: input.submissionId },
      }),
      patch: (id) => {
        const existing = this.#state.sentPrompts.get(id);
        return {
          promptRequestIds: withEntry(this.#state.promptRequestIds, input.sessionId, id),
          drafts: withoutEntry(this.#state.drafts, input.sessionId),
          pathRefs: withoutEntry(this.#state.pathRefs, input.sessionId),
          // 用户消息不一定被 Agent 回显：先把「我刚发的正文」放进时间线。阶段是**已发出、
          // 等待服务端确认**；正式正文到达时只按 `turnId`/`messageId` 接替它。
          sentPrompts: withEntry(this.#state.sentPrompts, id, sentPromptOf({
            requestId: id,
            sessionId: input.sessionId,
            text: input.text,
            attachmentNames: input.attachments.map((attachment) => attachment.fileName),
            // 上传阶段已经显示过这条输入：保留它的首次可见时刻，不因进入下一阶段而跳动。
            createdAt: existing?.createdAt ?? new Date(this.#now()).toISOString(),
            phase: "submitted",
            turnId: existing?.turnId ?? null,
            messageId: existing?.messageId ?? null,
          })),
        };
      },
    });
    if (requestId === null) return NOT_CONNECTED;
    this.#promptDrafts.set(requestId, {
      sessionId: input.sessionId,
      text: input.text,
      pathRefs: input.pathRefs,
      attachments: input.attachments,
    });
    return { ok: true, requestId };
  }

  // ── 文件栏内部实现 ─────────────────────────────────────────────────────────

  /** 一个来源的定时器键。 */
  #fileKey(sessionId: Uuid, source: FileSource): string {
    return `${sessionId}|${source}`;
  }

  /**
   * 本机会话文件来源是否不可用。
   *
   * - 未协商 `core.session-files.v1`：旧主机明确降级，**不派发**（服务端也会在 I/O 前拒绝）；
   * - 远程 imported 会话：全部文件/Git 明确暂不支持，不查询本机替代目录；已收到的对话差异仍可查看。
   */
  #fileSourceUnsupported(sessionId: Uuid): FileSourceUnsupportedReason | null {
    if (!this.#state.selectedFeatures.includes(SESSION_FILES_FEATURE)) return "feature_unavailable";
    if (this.#state.preparations.has(sessionId) && this.#findSession(sessionId) === undefined &&
        !this.#state.selectedFeatures.includes(WORKSPACE_FILES_FEATURE)) return "workspace_feature_unavailable";
    return this.#findSession(sessionId)?.origin.kind === "remote" ? "remote_imported" : null;
  }

  #workspaceFilesTarget(sessionId: Uuid): WorkspaceFilesTarget | null {
    const preparation = this.#state.preparations.get(sessionId);
    if (preparation !== undefined && preparation.workspace.path.length > 0) {
      return { workspacePath: preparation.workspace.path };
    }
    const session = this.#findSession(sessionId);
    if (session?.origin.kind !== "local" || session.workspace == null) return null;
    return { directoryId: session.workspace.directoryId };
  }

  #cancelFileRefreshTimer(key: string): void {
    const cancel = this.#fileRefreshTimers.get(key);
    if (cancel === undefined) return;
    cancel();
    this.#fileRefreshTimers.delete(key);
  }

  /** 丢弃所有内容读取（关闭/切换/替换文件时调用；迟到响应因不在表中而被忽略）。 */
  #dropPendingContentQueries(): void {
    for (const [requestId, pending] of this.#pendingFileQueries) {
      if (pending.kind === "read" || pending.kind === "git_diff") {
        this.#pendingFileQueries.delete(requestId);
      }
    }
  }

  /** 丢弃某个来源的在途列表/状态查询（过滤口径或可用性变化后旧结果不再有效）。 */
  #dropPendingSourceQueries(sessionId: Uuid, source: FileSource): void {
    this.#fileRefreshRounds.delete(this.#fileKey(sessionId, source));
    for (const [requestId, pending] of this.#pendingFileQueries) {
      if (pending.sessionId !== sessionId || pending.source !== source) continue;
      if (pending.kind === "list" || pending.kind === "git_status") {
        this.#pendingFileQueries.delete(requestId);
      }
    }
  }

  /**
   * 丢弃**全部**在途文件查询、合并/退避定时器与排队刷新（连接 closed/blocked/authenticated）。
   *
   * 这些请求属于已经失效的连接；留着它们只会让 `#fileQueryInFlight` 永远为真。内容读取
   * （read/git_diff）一并丢弃：迟到的响应本来就会因不在在途表而被忽略。
   */
  #resetFileQueries(): void {
    this.#pendingFileQueries.clear();
    this.#fileRefreshRounds.clear();
    for (const cancel of this.#fileRefreshTimers.values()) cancel();
    this.#fileRefreshTimers.clear();
    this.#fileRefreshQueued.clear();
    this.#messageRefreshQueued.clear();
  }

  /**
   * 丢弃旧连接上的全部在途 `session.read`（closed/blocked/authenticated）。
   *
   * 查询结果只属于发起它的连接：换连接后旧结果不会回来。留着这些条目会永久挡住同一会话的
   * 后续读取（首读被跳过 → 页面只剩实时事件，看不到已保存的用户输入与历史）。被丢弃的会话
   * 交给 `#deferredReads`，认证完成后按当前页补读。
   */
  #resetPendingReads(): readonly Uuid[] {
    if (this.#pendingReads.size === 0) return [];
    const sessionIds = new Set<Uuid>();
    for (const pending of this.#pendingReads.values()) {
      sessionIds.add(pending.sessionId);
      this.#deferredReads.add(pending.sessionId);
    }
    this.#pendingReads.clear();
    return [...sessionIds];
  }

  /** 读取被丢弃后复位这些会话的翻页加载标记（否则按钮/滚动会永远停在加载中）。 */
  #withReadLoadingStopped(state: ClientState, sessionIds: readonly Uuid[]): ClientState {
    if (sessionIds.length === 0) return state;
    const threads = new Map(state.threads);
    let changed = false;
    for (const sessionId of sessionIds) {
      const thread = threads.get(sessionId);
      if (thread === undefined || (!thread.loadingEarlier && !thread.loadingHistory)) continue;
      threads.set(sessionId, { ...thread, loadingEarlier: false, loadingHistory: false });
      changed = true;
    }
    return changed ? { ...state, threads } : state;
  }

  /** 把各会话各来源的 `loading` 复位为 false（连接重置后不再有请求在为它们加载）。 */
  #withFileLoadingStopped(state: ClientState): ClientState {
    const sources = state.sessionFiles.sources;
    if (sources.size === 0) return state;
    let changed = false;
    const updated = new Map(sources);
    for (const [sessionId, sourcesState] of sources) {
      const directories = new Map(sourcesState.all.directories);
      let directoriesChanged = false;
      for (const [path, directory] of sourcesState.all.directories) {
        if (!directory.loading) continue;
        directories.set(path, { ...directory, loading: false });
        directoriesChanged = true;
      }
      const all = directoriesChanged
        ? { ...sourcesState.all, directories }
        : sourcesState.all;
      const git = sourcesState.git.loading ? { ...sourcesState.git, loading: false } : sourcesState.git;
      if (all === sourcesState.all && git === sourcesState.git) continue;
      changed = true;
      updated.set(sessionId, { ...sourcesState, all, git });
    }
    if (!changed) return state;
    return { ...state, sessionFiles: { ...state.sessionFiles, sources: updated } };
  }

  #withSources(state: ClientState, sessionId: Uuid, sources: SessionFilesSourcesState): ClientState {
    const map = new Map(state.sessionFiles.sources);
    map.set(sessionId, sources);
    return { ...state, sessionFiles: { ...state.sessionFiles, sources: map } };
  }

  #withFileContent(state: ClientState, content: FileContentState, generation: number): ClientState {
    return { ...state, sessionFiles: { ...state.sessionFiles, content, generation } };
  }

  /** 突发事件后的刷新排期：同一来源在同一窗口内只排一次（合并）。 */
  #scheduleFileRefresh(sessionId: Uuid, source: FileSource, delayMs: number): void {
    if (this.#schedule === null) return;
    const key = this.#fileKey(sessionId, source);
    if (this.#fileRefreshTimers.has(key)) return;
    const cancel = this.#schedule(delayMs, () => {
      this.#fileRefreshTimers.delete(key);
      this.#refreshFileSource(sessionId, source);
    });
    this.#fileRefreshTimers.set(key, cancel);
  }

  /** 限流错误按 `retryAfterMs` 退避；没有该字段时只保留可见失败，不自动猜退避。 */
  #scheduleRateLimitRetry(sessionId: Uuid, source: FileSource, error: PublicError | null): void {
    if (error === null || error.code !== "resource.rate_limited") return;
    const raw = error.details["retryAfterMs"];
    if (typeof raw !== "number" || !Number.isFinite(raw) || raw < 0) return;
    this.#scheduleFileRefresh(sessionId, source, Math.min(raw, FILE_REFRESH_MAX_RETRY_MS));
  }

  /**
   * 刷新一个来源（进入视图/手动刷新/事件触发共用）。
   *
   * 失败只补 `error`，保留此前的成功结果；同一来源已有在途查询时只记「排队」，由该次结果
   * 落地后再补一次，避免突发事件打出一串并发查询。
   */
  #refreshFileSource(sessionId: Uuid, source: FileSource): void {
    // 立即刷新取代尚未到期的合并/退避定时器，避免同一次意图排出两条查询。
    this.#cancelFileRefreshTimer(this.#fileKey(sessionId, source));
    const sources = sourcesOf(this.#state.sessionFiles, sessionId);
    const unsupported = this.#fileSourceUnsupported(sessionId);
    if (unsupported !== null) {
      this.#dropPendingSourceQueries(sessionId, source);
      const nextSources: SessionFilesSourcesState =
        source === "all"
          ? { ...sources, all: { ...sources.all, unsupported } }
          : { ...sources, git: { ...sources.git, unsupported, loading: false } };
      this.#commit(this.#withSources(this.#state, sessionId, nextSources));
      return;
    }
    if (this.#identity.connectionId === null) {
      const error = notConnectedFileError();
      const nextSources: SessionFilesSourcesState =
        source === "all"
          ? {
              ...sources,
              all: {
                ...sources.all,
                directories: withDirectoryError(sources.all.directories, "", error),
              },
            }
          : { ...sources, git: { ...sources.git, loading: false, error } };
      this.#commit(this.#withSources(this.#state, sessionId, nextSources));
      return;
    }
    const key = this.#fileKey(sessionId, source);
    if (this.#fileQueryInFlight(sessionId, source)) {
      this.#fileRefreshQueued.add(key);
      return;
    }
    if (source === "all") {
      const paths = ["", ...(this.#expandedFileDirectories.get(sessionId) ?? [])
        .filter(path => path !== "" && sources.all.directories.has(path))];
      this.#fileRefreshRounds.set(key, { id: ++this.#fileRefreshId, remaining: paths.length, failed: false });
      for (const path of paths) this.#dispatchAllFilesPage(sessionId, path, undefined, false);
    } else {
      this.#fileRefreshRounds.set(key, { id: ++this.#fileRefreshId, remaining: 1, failed: false });
      this.#dispatchGitStatus(sessionId, undefined, false);
    }
  }

  #fileQueryInFlight(sessionId: Uuid, source: FileSource): boolean {
    for (const pending of this.#pendingFileQueries.values()) {
      if (pending.sessionId === sessionId && pending.source === source &&
        (pending.kind === "list" || pending.kind === "git_status")) return true;
    }
    return false;
  }

  /** 目录枚举的一页；`after === undefined` 为第一页。 */
  #dispatchAllFilesPage(sessionId: Uuid, path: string, after: string | undefined, append: boolean): void {
    const sources = sourcesOf(this.#state.sessionFiles, sessionId);
    const existing = sources.all.directories.get(path) ?? emptyAllFilesDirectory(path);
    const directories = new Map(sources.all.directories);
    directories.set(path, { ...existing, loading: true, error: null });
    this.#commit(this.#withSources(this.#state, sessionId, {
      ...sources,
      all: { ...sources.all, unsupported: null, directories },
    }));

    const requestId = this.#ids.uuid();
    const payload: { path: string; hideIgnored: boolean; after?: string } = {
      path,
      hideIgnored: sources.all.hideIgnored,
    };
    if (after !== undefined) payload.after = after;
    const dispatched = this.#dispatchFileCommand(
      {
        requestId,
        command: "session.files.list",
        sessionId,
        payload,
      },
      {
        requestId,
        kind: "list",
        sessionId,
        source: "all",
        path,
        generation: this.#state.sessionFiles.generation,
        ...(after === undefined ? {} : { after }),
        append,
      },
    );
    if (!dispatched) {
      const error = notConnectedFileError();
      const afterDispatch = sourcesOf(this.#state.sessionFiles, sessionId);
      const directoriesAfter = new Map(afterDispatch.all.directories);
      const directory = directoriesAfter.get(path) ?? emptyAllFilesDirectory(path);
      directoriesAfter.set(path, { ...directory, loading: false, error });
      this.#commit(this.#withSources(this.#state, sessionId, {
        ...afterDispatch,
        all: { ...afterDispatch.all, directories: directoriesAfter },
      }));
    }
  }

  /** Git 未提交状态的一页。 */
  #dispatchGitStatus(sessionId: Uuid, after: string | undefined, append: boolean): void {
    const sources = sourcesOf(this.#state.sessionFiles, sessionId);
    this.#commit(this.#withSources(this.#state, sessionId, {
      ...sources,
      git: { ...sources.git, unsupported: null, loading: true, error: null },
    }));

    const requestId = this.#ids.uuid();
    const dispatched = this.#dispatchFileCommand(
      {
        requestId,
        command: "session.git.status",
        sessionId,
        payload: after === undefined ? {} : { after },
      },
      {
        requestId,
        kind: "git_status",
        sessionId,
        source: "git",
        path: null,
        generation: this.#state.sessionFiles.generation,
        ...(after === undefined ? {} : { after }),
        append,
      },
    );
    if (!dispatched) {
      const error = notConnectedFileError();
      const afterDispatch = sourcesOf(this.#state.sessionFiles, sessionId);
      this.#commit(this.#withSources(this.#state, sessionId, {
        ...afterDispatch,
        git: { ...afterDispatch.git, loading: false, error },
      }));
    }
  }

  /** 预览块读取（首块或同版本续块）。 */
  #dispatchFileRead(input: {
    readonly sessionId: Uuid;
    readonly source: FileSource;
    readonly path: string;
    readonly generation: number;
    readonly purpose: "preview" | "download";
    readonly offset: string;
    readonly chunks: readonly Uint8Array[];
    readonly version?: string;
    readonly sizeBytes?: string;
    readonly fileKind?: "text" | "image" | "binary";
    readonly previewTruncated?: boolean;
  }): void {
    const requestId = this.#ids.uuid();
    const payload: {
      path: string;
      purpose: "preview" | "download";
      offset: string;
      version?: string;
    } = { path: input.path, purpose: input.purpose, offset: input.offset };
    if (input.version !== undefined) payload.version = input.version;
    const dispatched = this.#dispatchFileCommand(
      {
        requestId,
        command: "session.files.read",
        sessionId: input.sessionId,
        payload,
      },
      {
        requestId,
        kind: "read",
        sessionId: input.sessionId,
        source: input.source,
        path: input.path,
        generation: input.generation,
        chunks: [...input.chunks],
        ...(input.version === undefined ? {} : { version: input.version }),
        ...(input.sizeBytes === undefined ? {} : { sizeBytes: input.sizeBytes }),
        ...(input.fileKind === undefined ? {} : { fileKind: input.fileKind }),
        ...(input.previewTruncated === undefined ? {} : { previewTruncated: input.previewTruncated }),
      },
    );
    if (!dispatched) this.#failCurrentFile(notConnectedFileError());
  }

  /** Git 差异查询。 */
  #dispatchGitDiff(input: {
    readonly sessionId: Uuid;
    readonly source: FileSource;
    readonly path: string;
    readonly generation: number;
  }): void {
    const requestId = this.#ids.uuid();
    const dispatched = this.#dispatchFileCommand(
      {
        requestId,
        command: "session.git.diff",
        sessionId: input.sessionId,
        payload: { path: input.path },
      },
      {
        requestId,
        kind: "git_diff",
        sessionId: input.sessionId,
        source: input.source,
        path: input.path,
        generation: input.generation,
      },
    );
    if (!dispatched) this.#failCurrentFile(notConnectedFileError());
  }

  /** 唯一文件查询出口：登记在途上下文后派发（查询没有 command 记录/终态事件）。 */
  #dispatchFileCommand(body: CommandBody, pending: PendingFileQuery): boolean {
    const connectionId = this.#identity.connectionId;
    if (connectionId === null) return false;
    body = this.directoryScopedFileQuery(body as SessionFilesCommandBody);
    const round = this.#fileRefreshRounds.get(this.#fileKey(pending.sessionId, pending.source));
    this.#pendingFileQueries.set(body.requestId,
      round !== undefined && (pending.kind === "list" || pending.kind === "git_status")
        ? { ...pending, refreshId: round.id } : pending);
    this.#sync.dispatch({
      protocolVersion: 1,
      type: "command",
      messageId: this.#ids.uuid(),
      connectionId,
      connectionSequence: "0",
      body,
    });
    return true;
  }

  /** Shared by file previews and downloads: the view id is not part of the directory query on the wire. */
  directoryScopedFileQuery(body: SessionFilesCommandBody): SessionFilesCommandBody {
    if (body.sessionId === undefined || !this.#state.selectedFeatures.includes(WORKSPACE_FILES_FEATURE)) return body;
    const workspace = this.#workspaceFilesTarget(body.sessionId);
    if (workspace === null) return body;
    const { sessionId: _, ...query } = body;
    return { ...query, payload: { ...query.payload, workspace } } as SessionFilesCommandBody;
  }

  /** 只把错误落到**当前**文件内容（代次不匹配的旧请求不能改界面）。 */
  #failCurrentFile(error: PublicError): void {
    const content = this.#state.sessionFiles.content;
    if (content.kind !== "file") return;
    this.#commit(this.#withFileContent(this.#state, {
      ...content,
      status: "error",
      error,
    }, content.generation));
  }

  /**
   * 文件栏查询结果归并。
   *
   * 先核对在途上下文与代次：不在表里（已关闭/已替换/已刷新）或代次不匹配的响应一律丢弃。
   * 解码失败与失败状态都保留此前成功结果，不把空数组当成新的成功。
   */
  #withSessionFilesResult(state: ClientState, result: CommandResultMessage): ClientState {
    if (!isSessionFilesCommandName(result.body.command)) return state;
    const pending = this.#pendingFileQueries.get(result.body.requestId);
    if (pending === undefined) return state;
    this.#pendingFileQueries.delete(result.body.requestId);

    const isContent = pending.kind === "read" || pending.kind === "git_diff";
    if (isContent) {
      const content = state.sessionFiles.content;
      if (
        pending.generation !== state.sessionFiles.generation ||
        content.kind !== "file" ||
        content.generation !== pending.generation ||
        content.context.sessionId !== pending.sessionId ||
        content.context.source !== pending.source ||
        content.context.path !== pending.path
      ) {
        return state;
      }
    }

    if (result.body.status !== "completed") {
      const error = resultError(result);
      const afterFailure = this.#applyFileQueryFailure(state, pending, error);
      if (pending.kind === "list" || pending.kind === "git_status") {
        this.#scheduleRateLimitRetry(pending.sessionId, pending.source, error);
      }
      return this.#finishFileRefresh(afterFailure, pending);
    }

    switch (pending.kind) {
      case "list":
        return this.#finishFileRefresh(this.#applyAllFilesPage(state, pending, result.body.result), pending);
      case "git_status":
        return this.#finishFileRefresh(this.#applyGitStatusPage(state, pending, result.body.result), pending);
      case "read":
        return this.#applyFilePreviewChunk(state, pending, result.body.result);
      case "git_diff":
        return this.#applyGitDiff(state, pending, result.body.result);
    }
  }

  /** 失败统一落到来源或当前内容；来源失败不清空已成功加载的条目。 */
  #finishFileRefresh(state: ClientState, pending: PendingFileQuery): ClientState {
    const key = this.#fileKey(pending.sessionId, pending.source);
    const round = this.#fileRefreshRounds.get(key);
    if (pending.refreshId === undefined || round === undefined || round.id !== pending.refreshId) return state;
    const sources = sourcesOf(state.sessionFiles, pending.sessionId);
    round.failed ||= pending.source === "all"
      ? sources.all.directories.get(pending.path ?? "")?.error != null
      : sources.git.error !== null || sources.git.gitState === "unavailable";
    round.remaining -= 1;
    if (round.remaining > 0) return state;
    this.#fileRefreshRounds.delete(key);
    const refreshResults = new Map(state.sessionFiles.refreshResults);
    refreshResults.set(key, { id: round.id, outcome: round.failed ? "failed" : "success" });
    return { ...state, sessionFiles: { ...state.sessionFiles, refreshResults } };
  }

  #applyFileQueryFailure(state: ClientState, pending: PendingFileQuery, error: PublicError): ClientState {
    if (pending.kind === "read" || pending.kind === "git_diff") {
      return this.#withFileContent(state, {
        kind: "file",
        context: {
          sessionId: pending.sessionId,
          source: pending.source,
          path: pending.path ?? "",
        },
        generation: pending.generation,
        stale: false,
        status: "error",
        error,
      }, pending.generation);
    }
    const sources = sourcesOf(state.sessionFiles, pending.sessionId);
    if (pending.kind === "list") {
      const path = pending.path ?? "";
      const directories = new Map(sources.all.directories);
      const directory = directories.get(path) ?? emptyAllFilesDirectory(path);
      directories.set(path, { ...directory, loading: false, error });
      return this.#withSources(state, pending.sessionId, { ...sources, all: { ...sources.all, directories } });
    }
    return this.#withSources(state, pending.sessionId, {
      ...sources,
      git: { ...sources.git, loading: false, error },
    });
  }

  #applyAllFilesPage(
    state: ClientState,
    pending: PendingFileQuery,
    value: unknown,
  ): ClientState {
    const decoded = decodeSessionFilesResult("session.files.list", value);
    if (!decoded.ok) {
      return this.#applyFileQueryFailure(state, pending, invalidFileResultError(decoded.detail));
    }
    if (decoded.value.path !== (pending.path ?? "")) {
      return this.#applyFileQueryFailure(state, pending, invalidFileResultError("结果回显的 path 与请求不一致"));
    }
    const sources = sourcesOf(state.sessionFiles, pending.sessionId);
    const path = pending.path ?? "";
    const existing = sources.all.directories.get(path) ?? emptyAllFilesDirectory(path);
    const entries = pending.append === true ? [...existing.entries, ...decoded.value.entries] : decoded.value.entries;
    const directories = new Map(sources.all.directories);
    directories.set(path, {
      path,
      entries,
      next: decoded.value.next ?? null,
      gitState: decoded.value.gitState,
      loading: false,
      error: null,
    });
    const nextState = this.#withSources(state, pending.sessionId, {
      ...sources,
      all: { ...sources.all, directories },
    });
    return nextState;
  }

  #applyGitStatusPage(
    state: ClientState,
    pending: PendingFileQuery,
    value: unknown,
  ): ClientState {
    const decoded = decodeSessionFilesResult("session.git.status", value);
    if (!decoded.ok) {
      return this.#applyFileQueryFailure(state, pending, invalidFileResultError(decoded.detail));
    }
    const sources = sourcesOf(state.sessionFiles, pending.sessionId);
    const entries = pending.append === true ? [...sources.git.entries, ...decoded.value.entries] : decoded.value.entries;
    const nextState = this.#withSources(state, pending.sessionId, {
      ...sources,
      git: {
        ...sources.git,
        gitState: decoded.value.gitState,
        entries,
        next: decoded.value.next ?? null,
        loading: false,
        error: null,
      },
    });
    return nextState;
  }

  #applyFilePreviewChunk(
    state: ClientState,
    pending: PendingFileQuery,
    value: unknown,
  ): ClientState {
    const decoded = decodeSessionFilesResult("session.files.read", value);
    if (!decoded.ok) {
      return this.#applyFileQueryFailure(state, pending, invalidFileResultError(decoded.detail));
    }
    const chunk = decoded.value;
    if (chunk.path !== pending.path) {
      return this.#applyFileQueryFailure(state, pending, invalidFileResultError("结果 path 与打开的上下文不一致"));
    }
    const previous = pending.chunks ?? [];
    const written = previous.reduce((total, part) => total + part.length, 0);
    if (BigInt(chunk.offset) !== BigInt(written)) {
      return this.#applyFileQueryFailure(state, pending, snapshotInvalidError());
    }
    if (pending.version !== undefined && pending.version !== chunk.version) {
      return this.#applyFileQueryFailure(state, pending, snapshotInvalidError());
    }
    if (pending.sizeBytes !== undefined && pending.sizeBytes !== chunk.sizeBytes) {
      return this.#applyFileQueryFailure(state, pending, snapshotInvalidError());
    }
    if (pending.fileKind !== undefined && pending.fileKind !== chunk.kind) {
      return this.#applyFileQueryFailure(state, pending, snapshotInvalidError());
    }
    let bytes: Uint8Array;
    try {
      bytes = decodeBase64Url(chunk.dataBase64);
    } catch {
      return this.#applyFileQueryFailure(state, pending, invalidFileResultError("dataBase64 不是合法的无填充 base64url"));
    }
    const total = written + bytes.length;
    // 图片走服务端的 4 MiB 预算；文本/二进制仍按 256 KiB。总大小、完成判定与截断阈值共用同一预算。
    const budget = (pending.fileKind ?? chunk.kind) === "image"
      ? FILE_IMAGE_PREVIEW_BUDGET_BYTES
      : FILE_PREVIEW_BUDGET_BYTES;
    if (total > budget) {
      return this.#applyFileQueryFailure(state, pending, previewBudgetError());
    }
    const endOffset = (BigInt(chunk.offset) + BigInt(bytes.length)).toString();
    const chunks = [...previous, bytes];
    const truncated = pending.previewTruncated === true || chunk.previewTruncated;
    const complete = chunk.eof || truncated || total >= budget || bytes.length === 0;
    if (bytes.length === 0 && !chunk.eof && !truncated) {
      return this.#applyFileQueryFailure(state, pending, invalidFileResultError("空块既非 EOF 也非截断，拒绝无限续读"));
    }
    if (!complete) {
      this.#dispatchFileRead({
        sessionId: pending.sessionId,
        source: pending.source,
        path: chunk.path,
        generation: pending.generation,
        purpose: "preview",
        offset: endOffset,
        chunks,
        version: chunk.version,
        sizeBytes: chunk.sizeBytes,
        fileKind: chunk.kind,
        previewTruncated: truncated,
      });
      return state;
    }
    const content: FileContentState = {
      kind: "file",
      context: {
        sessionId: pending.sessionId,
        source: pending.source,
        path: chunk.path,
      },
      generation: pending.generation,
      stale: false,
      status: "ready",
      content: {
        kind: "preview",
        path: chunk.path,
        fileKind: chunk.kind,
        sizeBytes: chunk.sizeBytes,
        version: chunk.version,
        offset: endOffset,
        bytes: concatBytes(chunks),
        eof: chunk.eof,
        previewTruncated: truncated || total >= budget,
      },
    };
    const nextState = this.#withFileContent(state, content, pending.generation);
    return nextState;
  }

  #applyGitDiff(
    state: ClientState,
    pending: PendingFileQuery,
    value: unknown,
  ): ClientState {
    const decoded = decodeSessionFilesResult("session.git.diff", value);
    if (!decoded.ok) {
      return this.#applyFileQueryFailure(state, pending, invalidFileResultError(decoded.detail));
    }
    if (decoded.value.path !== pending.path) {
      return this.#applyFileQueryFailure(state, pending, invalidFileResultError("结果 path 与打开的上下文不一致"));
    }
    const diff: GitDiffView = {
      path: decoded.value.path,
      base: decoded.value.base,
      kind: decoded.value.kind,
      ...(decoded.value.comparison === undefined ? {} : {comparison:decoded.value.comparison}),
      diffLines: decoded.value.diffLines ?? [],
      unavailable: decoded.value.unavailable ?? null,
      truncated: decoded.value.truncated,
    };
    const nextState = this.#withFileContent(state, {
      kind: "file",
      context: {
        sessionId: pending.sessionId,
        source: pending.source,
        path: decoded.value.path,
      },
      generation: pending.generation,
      stale: false,
      status: "ready",
      content: { kind: "git_diff", diff },
    }, pending.generation);
    return nextState;
  }

  /**
   * 一次状态提交后补发「在飞期间收到事件」排队的刷新。
   *
   * 放在 `apply` 的提交之后（而不是归并函数内部）是为了不产生嵌套提交：归并函数返回的
   * 状态先落地，排队的刷新再以新的当前状态为基线开始。
   */
  #drainQueuedFileRefreshes(): void {
    if (this.#fileRefreshQueued.size === 0) return;
    for (const key of [...this.#fileRefreshQueued]) {
      this.#fileRefreshQueued.delete(key);
      const separator = key.indexOf("|");
      if (separator < 0) continue;
      const sessionId = key.slice(0, separator);
      const source = key.slice(separator + 1) as FileSource;
      if (source !== "all" && source !== "git") continue;
      if (this.#schedule === null) this.#refreshFileSource(sessionId, source);
      else this.#scheduleFileRefresh(sessionId, source, 0);
    }
  }

  /**
   * 当前会话的 `file.changed`：只标正文「过时」，不刷新列表或覆盖正在阅读的内容。
   */
  #withFileChangedEvent(state: ClientState, record: StructuredRecord): ClientState {
    const sessionId = record.sessionId;
    if (sessionId === null) return state;
    // 只刷新当前对话页所属会话的来源；其他会话的事件不打断当前视图。
    if (this.#activeConversationSession !== null && this.#activeConversationSession !== sessionId) return state;
    let next = state;

    const content = state.sessionFiles.content;
    if (content.kind === "file" && content.context.sessionId === sessionId) {
      const displayPath = viewText(record.view, "displayPath");
      const outside = record.view["outsideWorkspace"] === true;
      if (!outside && displayPath !== null && displayPath === content.context.path) {
        next = { ...next, sessionFiles: { ...next.sessionFiles, content: { ...content, stale: true } } };
      }
    }

    return next;
  }

  /** 当前会话的易失增量快照（`messageId → deltaIndex`）：读取屏障只清掉这一批。 */
  #volatileSnapshot(sessionId: Uuid): ReadonlyMap<string, ReadonlySet<string>> {
    const snapshot = new Map<string, Set<string>>();
    for (const [messageId, tail] of this.#state.volatile) {
      if (tail.sessionId !== sessionId) continue;
      snapshot.set(messageId, new Set(tail.deltas.keys()));
    }
    return snapshot;
  }

  /** `turn.delta_compacted` 之后重读消息页；与文件刷新一样在提交后派发，避免嵌套提交。 */
  #drainQueuedMessageRefreshes(): void {
    if (this.#messageRefreshQueued.size === 0) return;
    for (const sessionId of [...this.#messageRefreshQueued]) {
      this.#messageRefreshQueued.delete(sessionId);
      // 没有在途读取才发；已有读取会在结果里带上权威正文。
      const inFlight = [...this.#pendingReads.values()].some((pending) => pending.sessionId === sessionId);
      if (inFlight) {
        this.#messageRefreshQueued.add(sessionId);
        continue;
      }
      this.#messageRefreshQueued.delete(sessionId);
      this.#requestPage(sessionId, "refresh_messages");
    }
  }

  #drainQueuedInteractionRefreshes(): void {
    if (this.#state.connection.state !== "online") return;
    for (const sessionId of this.#interactionRefreshQueued) {
      // 保留排队标记，等当前读取完成；其快照可能早于刚收到的请求。
      if ([...this.#pendingReads.values()].some((pending) => pending.sessionId === sessionId)) continue;
      this.#interactionRefreshQueued.delete(sessionId);
      this.#requestPage(sessionId, "refresh_interactions");
    }
  }

  #requestPage(
    sessionId: Uuid,
    intent: ReadIntent,
    cursors: { readonly before?: ReadCursor; readonly beforeHistory?: string } = {},
  ): Uuid | null {
    const volatileAtRequest = this.#volatileSnapshot(sessionId);
    const interactionEventsAtRequest = new Set(this.#recordsOf(sessionId)
      .filter((record) => record.eventType === "permission.requested" || record.eventType === "elicitation.requested")
      .map((record) => record.eventId));
    const remote = remoteRefOf(this.#state, sessionId);
    const generation = remote === null ? 0 : sourceGenerationOf(this.#state, remote.ownerNodeId);
    return this.#dispatchIntent({
      sessionId,
      build: (requestId) => {
        this.#pendingReads.set(requestId, { sessionId, intent, volatileAtRequest, interactionEventsAtRequest, remote, generation });
        // 与 `paging.ts` 的 `buildReadPayload` 同一形状；这里就地构造是为了让 `include`
        // 保持 `SessionReadInclude[]`（协议类型），而 `PageRequest` 声明的是更宽的 `string[]`。
        const payload: { include: readonly SessionReadInclude[]; before?: ReadCursor; beforeHistory?: string; limit?: number } = {
          include: this.#includesFor(intent, remote !== null),
        };
        // limit 按用户输入计数，服务端同时返回该范围内的回复与思考。
        if (intent === "first" || intent === "rebuild_remote" || intent === "earlier" || intent === "refresh_messages") payload.limit = 10;
        if (cursors.before !== undefined) payload.before = cursors.before;
        if (cursors.beforeHistory !== undefined) payload.beforeHistory = cursors.beforeHistory;
        return { requestId, command: "session.read", sessionId, payload };
      },
      patch: () => {
        const threads = new Map(this.#state.threads);
        const existing = threads.get(sessionId) ?? EMPTY_THREAD;
        threads.set(sessionId, {
          ...existing,
          loadingEarlier: intent === "earlier",
          loadingHistory: intent === "earlier_history",
          // 刷新有效交互不动消息/历史分页错误：它只重读 pending_interactions。
          pagingError: intent === "refresh_interactions" ? existing.pagingError : null,
        });
        return { threads };
      },
    });
  }

  /**
   * 本次读取要带哪些 include。
   *
   * 结构化历史只在服务端**选中**了 `core.session-history.v1` 时才要（`loadSession` 与
   * `loadEarlierHistory` 都先查这一点）：未协商却请求它时服务端会回明确的不支持类错误，
   * 而旧服务端连该取值都不认识。翻页请求只要自己那一份资源，两套分页因此互不影响（D2）。
   */
  #includesFor(intent: ReadIntent, remote: boolean): readonly SessionReadInclude[] {
    const historyNegotiated = this.#state.selectedFeatures.includes(SESSION_HISTORY_FEATURE);
    const pendingNegotiated = this.#state.selectedFeatures.includes(PENDING_INTERACTIONS_FEATURE);
    switch (intent) {
      case "earlier":
        return ["messages"];
      case "refresh_messages":
        // `turn.delta_compacted` 之后的权威正文回填：只要消息页，不重读历史/配置。
        return ["messages"];
      case "earlier_history":
        return ["history"];
      case "refresh_interactions":
        // 远程读取面不返回有效交互；本地路径只在协商后请求。
        if (remote) return [];
        return pendingNegotiated ? ["pending_interactions"] : [];
      case "first":
      case "rebuild_remote": {
        // 远程读取面只表达 messages/history：配置项与有效交互是 Owner 本机路由的能力，
        // 请求它们会让服务端显式回 capability.unsupported_by_broker（design D4/D6）。
        if (remote) {
          const includes: SessionReadInclude[] = ["messages"];
          if (historyNegotiated) includes.push("history");
          return includes;
        }
        const includes: SessionReadInclude[] = [...CONVERSATION_INCLUDES];
        if (historyNegotiated) includes.push("history");
        if (pendingNegotiated) includes.push("pending_interactions");
        return includes;
      }
    }
  }

  /**
   * 唯一派发出口：分配稳定 `requestId`、把命令记成 `submitting`、再发出去。
   *
   * 没有已认证连接时返回 `null` 且**什么都不做**（不派发、不登记状态、不调 `build`）。
   * `patch` 在派发前应用，因此调用方登记的 `pendingReads`/`submitting` 状态与出站命令同批提交。
   */
  #dispatchIntent(input: {
    readonly sessionId: Uuid | null;
    readonly requestId?: Uuid;
    readonly build: (requestId: Uuid) => CommandBody;
    readonly patch?: (requestId: Uuid) => Partial<ClientState>;
  }): Uuid | null {
    const connectionId = this.#identity.connectionId;
    if (connectionId === null) return null;

    const requestId = input.requestId ?? this.#ids.uuid();
    const body = input.build(requestId);
    const commands = new Map(this.#state.commands);
    // `markSubmitting`：未确认接受前不得显示已接受（R13 的第二条规则）。
    commands.set(
      requestId,
      markSubmitting(createCommand({ requestId, command: body.command, sessionId: input.sessionId })),
    );
    const extra = input.patch === undefined ? {} : input.patch(requestId);
    this.#commit({ ...this.#state, commands, ...extra });

    this.#sync.dispatch({
      protocolVersion: 1,
      type: "command",
      messageId: this.#ids.uuid(),
      connectionId,
      // 占位：真正的出站序号由 `SyncConnection.sendCommand` 覆盖（§4.1）。
      connectionSequence: "0",
      body,
    });
    return requestId;
  }

  /** 本会话在本设备已派发、尚未被读取/回显覆盖的用户消息。 */
  #sentPromptsOf(sessionId: Uuid): readonly SentPrompt[] {
    return [...this.#state.sentPrompts.values()].filter((prompt) => prompt.sessionId === sessionId);
  }

  /**
   * 时间线条目的引用复用（D1）：内容没变就交回上一次的对象。
   *
   * 复用是**结构化**的（`stabilizeTimelineEntries` 比较条目全部字段，且显式比较 `Date`/`Map`/
   * `Set`/`ArrayBuffer` 的内容），因此不会漏掉正文、状态、媒体或诊断的真实变化；命中时整份
   * 条目数组也保持同一引用。
   */
  #stabilizeTimeline(sessionId: Uuid, entries: readonly TimelineEntry[]): readonly TimelineEntry[] {
    const stabilized = stabilizeTimelineEntries(this.#timelineEntries.get(sessionId), entries);
    this.#timelineEntries.set(sessionId, stabilized);
    return stabilized;
  }

  /**
   * 释放**已不存在的会话**的时间线复用层。
   *
   * 这一层只是引用复用的比对基线，不是正文缓存：线程被删除（来源数据清除、会话被移除）后
   * `conversationPage` 再也构造不出该会话的页面，留着只会把整份条目数组挂到进程结束。只删除
   * 「归并前有线程、归并后没有」的会话，仍在使用的会话（含准备态页面）不受影响。
   */
  #pruneTimelineEntries(threadsBefore: ReadonlyMap<Uuid, ConversationThreadState>): void {
    if (this.#timelineEntries.size === 0 || threadsBefore === this.#state.threads) return;
    for (const sessionId of this.#timelineEntries.keys()) {
      if (threadsBefore.has(sessionId) && !this.#state.threads.has(sessionId)) {
        this.#timelineEntries.delete(sessionId);
      }
    }
  }

  /**
   * 更新一条本地提交的阶段（上传 → 封存 → 待确认 → 已受理…）。
   *
   * 条目已经让位（权威消息先到）时什么都不做：迟到的上传/命令结果不得复活它。
   */
  #patchSentPrompt(requestId: Uuid, update: (prompt: SentPrompt) => SentPrompt): void {
    const next = withPromptPhase(this.#state, requestId, update);
    if (next !== this.#state) this.#commit(next);
  }

  /**
   * 清掉本会话里**未成立**（上传失败/取消）的旧提交回声。
   *
   * 它们从未被服务端受理，不会有权威消息来接替；下一次提交开始时若还留着，时间线上就会留下
   * 一条永远显示「未发送」的孤立条目。已受理/不确定的条目**不**在这里清理。
   */
  #dropUnsettledPrompts(sessionId: Uuid): void {
    let changed = false;
    const sentPrompts = new Map(this.#state.sentPrompts);
    for (const [requestId, prompt] of sentPrompts) {
      if (prompt.sessionId === sessionId && prompt.phase === "failed") {
        sentPrompts.delete(requestId);
        changed = true;
      }
    }
    if (changed) this.#commit({ ...this.#state, sentPrompts });
  }

  /** 本会话的结构化记录（实时与历史按 `eventId` 去重，实时优先）。 */
  #recordsOf(sessionId: Uuid): readonly StructuredRecord[] {
    // 去重键绑定来源节点：不同 Owner 的同名 `eventId` 是不同事件，不能只按裸标识合并。
    const merged = new Map<string, StructuredRecord>();
    for (const record of this.#state.structured.values()) {
      if (record.sessionId === sessionId) merged.set(recordKey(record), record);
    }
    for (const record of this.#state.history.values()) {
      if (record.sessionId !== sessionId) continue;
      const key = recordKey(record);
      if (!merged.has(key)) merged.set(key, record);
    }
    return [...merged.values()];
  }

  /** 一条交互的呈现模型（可应答性的唯一判定入口）。 */
  #interactionOf(sessionId: Uuid, interactionId: string): InteractionView | null {
    const page = this.conversationPage(sessionId);
    if (page === null) return null;
    return page.interactions.find((item) => item.interactionId === interactionId) ?? null;
  }

  #findSession(sessionId: Uuid): SessionSummary | undefined {
    return this.#state.resources?.sessions.find((summary) => summary.sessionId === sessionId);
  }

  #commit(next: ClientState): void {
    if (next === this.#state) return;
    this.#state = next;
    for (const listener of [...this.#listeners]) listener();
  }
}

// ── 文件栏的纯辅助 ──────────────────────────────────────────────────────────

/** 一条没有连接时的明确失败（不静默停在「读取中」）。 */
function notConnectedFileError(): PublicError {
  return {
    code: "internal.unavailable",
    message: "尚未建立已认证连接，无法读取会话文件。",
    retryable: true,
    details: {},
  };
}

/** 服务端结果形状不符合合同：显式失败，不把错误形状当正确形状用。 */
function invalidFileResultError(detail: string): PublicError {
  return {
    code: "internal.unavailable",
    message: `服务端返回的会话文件结果不符合协议：${detail}`,
    retryable: false,
    details: {},
  };
}

/** 内容版本/偏移不一致：快照失效，必须重新打开，不能拼接两个版本。 */
function snapshotInvalidError(): PublicError {
  return {
    code: "internal.unavailable",
    message: "文件在读取过程中发生变化，内容快照已失效；请重新打开文件。",
    retryable: true,
    details: {},
  };
}

/** 预览超过客户端预算：明确按超大结果失败（不悄悄只显示前一段并冒充完整）。 */
function previewBudgetError(): PublicError {
  return {
    code: "resource.result_too_large",
    message: "文件超过预览预算，请改用下载。",
    retryable: false,
    details: {},
  };
}

/** 来源暂不可用（旧主机/远程导入会话）。 */
function unsupportedSourceError(reason: FileSourceUnsupportedReason): PublicError {
  return {
    code: "capability.unsupported_by_broker",
    message:
      reason === "workspace_feature_unavailable" ? "请更新主机以读取目录文件和 Git 变更。" : reason === "remote_imported"
        ? "远程导入会话不支持读取本机文件或 Git 变更。"
        : "此主机未协商会话文件能力（core.session-files.v1）。",
    retryable: false,
    details: {},
  };
}

/** 失败结果 → 公开错误；服务端没给原因时补一条可辨识的说明。 */
function resultError(result: CommandResultMessage): PublicError {
  if (result.body.error !== null) return result.body.error;
  return {
    code: "internal.unavailable",
    message: `服务端未给出会话文件查询失败的原因（${result.body.status}）。`,
    retryable: false,
    details: {},
  };
}

/** 在一层目录上记失败，保留它此前成功加载的条目。 */
function withDirectoryError(
  directories: ReadonlyMap<string, AllFilesDirectoryView>,
  path: string,
  error: PublicError,
): ReadonlyMap<string, AllFilesDirectoryView> {
  const map = new Map(directories);
  const existing = map.get(path) ?? emptyAllFilesDirectory(path);
  map.set(path, { ...existing, loading: false, error });
  return map;
}

/** 按到达顺序拼接预览块（单次预览预算内，内存在构造时已受上限约束）。 */
function concatBytes(chunks: readonly Uint8Array[]): Uint8Array {
  let total = 0;
  for (const chunk of chunks) total += chunk.length;
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}
