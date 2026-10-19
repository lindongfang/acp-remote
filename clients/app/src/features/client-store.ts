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
  ConfigOptionView,
  ElicitationResolutionAction,
  ElicitationRespondPayload,
  EventMessage,
  ModeState,
  PairingLookupResponse,
  PublicError,
  SessionReadInclude,
  SessionReadResult,
  SessionSummary,
  SnapshotItemHistory,
  SnapshotItemMessages,
  SnapshotItemPendingInteractions,
  Uuid,
} from "../protocol";
import { isCommandStatusRecord } from "../sync-client/wire";
import type { ReadCursor, StagedResources, SyncClientEvent } from "../sync-client";
import {
  AgentConnectionOverlayStore,
  SESSION_HISTORY_FEATURE,
  assertHistoryPagesDoNotOverlap,
  assertPagesDoNotOverlap,
  decodeBase64Url,
  toLoadedHistoryPage,
  toLoadedPage,
} from "../sync-client";
import type { AllFilesDirectoryView, FileSource, GitDiffView } from "../domain/session-files";
import { buildSessionChangesModel } from "../domain/session-files";
import {
  SESSION_FILES_FEATURE,
  decodeSessionFilesResult,
  isSessionFilesCommandName,
} from "../protocol/session-files";
import type {
  FileContentState,
  FileViewContext,
  SessionFilesPanelModel,
  SessionFilesSourcesState,
  SessionFilesState,
} from "./session-files";
import {
  FILE_IMAGE_PREVIEW_BUDGET_BYTES,
  FILE_PREVIEW_BUDGET_BYTES,
  FILE_REFRESH_DEBOUNCE_MS,
  FILE_REFRESH_MAX_RETRY_MS,
  buildSessionFilesPanelModel,
  emptyAllFilesDirectory,
  initialSessionFilesState,
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
  InteractionView,
  ModeChangedTimelineCard,
  SessionReadPageView,
  StructuredRecord,
} from "../domain";
import {
  INTERACTION_UNAVAILABLE_LABELS,
  buildAgentCatalogView,
  buildSessionReadPageView,
  buildTimelineCard,
  compareTimelinePosition,
  directoryNamesOf,
  mergeStructuredRecords,
  positionOf,
  recordFromEvent,
  recordFromHistoryItem,
  recordGroupOf,
  viewText,
} from "../domain";
import type { AgentConnectionOverlay } from "../domain/agent-catalog";
import type { ConnectionIdentity, IdentifierSource, SyncGateway } from "./ports";
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
import type { ConversationSettingsModel } from "./conversation-settings-model";
import { buildConversationSettingsModel } from "./conversation-settings-model";
import type { VolatileMessageState } from "./conversation-timeline";
import {
  appendDelta,
  buildFileChangeViews,
  buildTimeline,
  deltaChannelOf,
} from "./conversation-timeline";
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
 * 正文（`messages`）之外还要两样：配置项（模型/模式的可选项，D5 的配置面）与待处理交互
 * （`pending_interactions`：历史读回的待处理项要呈现为「不可操作并说明原因」）。
 * 结构化历史（`history`）不在这里——它只在服务端协商到 `core.session-history.v1` 时才带上
 * （未协商却请求它会得到明确的不支持类错误，`design.md` 的 Compatibility）。
 */
// Sync v1 当前只支持这些首次读取资源；pending_interactions/turns/capabilities
// 会使服务端拒绝整条查询。实时交互与结构化历史仍按原有事件路径处理。
const CONVERSATION_INCLUDES: readonly SessionReadInclude[] = ["messages", "config_options"];

/** 一次 `session.read` 取回的明细资源（v1 对话页需要的两样）。 */
interface StagedReadResources {
  readonly pendingInteractions: readonly SnapshotItemPendingInteractions[];
  readonly configOptions: readonly ConfigOptionView[];
  readonly configVersion: string | null;
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
  /** 仅文件栏手动刷新会话变更的失败，不复用首次加载/消息翻页错误。 */
  readonly historyRefreshError?: string | null;
}

/** 一次 `session.read` 的用途：首屏、向上翻消息、向上翻历史。三者合并结果的方式不同。 */
type ReadIntent = "first" | "earlier" | "earlier_history" | "refresh_history";

/** 待完成的读取请求。 */
interface PendingRead {
  readonly sessionId: Uuid;
  readonly intent: ReadIntent;
}

/** 本设备提交的切换意图（模型/模式各一条），用于「切换中」的呈现。 */
interface SwitchIntent {
  readonly requestId: Uuid;
  readonly kind: "model" | "mode";
}

/** 客户端状态（不可变快照）。 */
export interface ClientState {
  readonly connection: ConnectionMachineState;
  /** 本设备标识（`auth.authenticated.deviceId`）；未认证为 `null`。 */
  readonly deviceId: string | null;
  /** 服务端本次协商选中的 feature（决定 `history` include 要不要带）。 */
  readonly selectedFeatures: readonly string[];
  /** 最后一次**验证通过并完成替换**的快照资源；断连时仍是目录页的唯一数据来源。 */
  readonly resources: StagedResources | null;
  readonly lastSyncedAtMs: number | null;
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
  /** 文件栏三来源、互斥内容视图与刷新状态（WP4；与草稿/时间线/展开态互不重叠）。 */
  readonly sessionFiles: SessionFilesState;
}

/** 初始状态：未配对、没有快照。 */
export function initialClientState(): ClientState {
  return {
    connection: initialConnectionState(),
    deviceId: null,
    selectedFeatures: [],
    resources: null,
    lastSyncedAtMs: null,
    overlay: [],
    structured: new Map(),
    history: new Map(),
    volatile: new Map(),
    discardedTurns: new Set(),
    drafts: new Map(),
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

/** 会话摘要 → 替换 `resources.sessions` 里的同 id 条目（目录汇总随之更新）。 */
function withSessionSummary(resources: StagedResources | null, summary: SessionSummary): StagedResources | null {
  if (resources === null) return resources;
  const rest = resources.sessions.filter((item) => item.sessionId !== summary.sessionId);
  return { ...resources, sessions: [summary, ...rest] };
}

/** `session.created`/`session.updated` 视图里的会话摘要。 */
function sessionSummaryIn(event: EventMessage): SessionSummary | null {
  if (event.body.eventType !== "session.created" && event.body.eventType !== "session.updated") return null;
  const summary = event.body.payload.view["session"];
  if (typeof summary !== "object" || summary === null || !("sessionId" in summary)) return null;
  const candidate = summary.sessionId;
  if (typeof candidate !== "string" || candidate.length === 0) return null;
  return summary as SessionSummary;
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

/** 会话标题（空标题呈现为未命名，`design.md` D7：标题单向由 Agent 通知写入）。 */
function titleOf(summary: SessionSummary | undefined): string {
  const title = summary?.title;
  return title === undefined || title === null || title.length === 0 ? "未命名会话" : title;
}

/** imported 会话来源离线时的标注（R20：只显示元数据）。 */
function originOfflineLabelOf(
  summary: SessionSummary | undefined,
  ownerLabelOf: (ownerNodeId: string) => string,
): string | null {
  if (summary?.origin.kind !== "remote" || summary.origin.online) return null;
  return `来源「${ownerLabelOf(summary.origin.ownerNodeId)}」当前离线`;
}

/** 命令失败的说明（结构化：码 + 消息 + 可重试）。 */
function describeCommandFailure(result: CommandResultMessage): string {
  const error = result.body.error;
  if (error === null) return `${result.body.command} 未成功（${result.body.status}）`;
  return error.retryable ? `${error.code}：${error.message}（可重试）` : `${error.code}：${error.message}`;
}

/**
 * 历史条目并入状态（按 `eventId` 去重）。
 *
 * 去重的两道口子都在这里：实时已收到的事件不因历史再收一遍而多出一条（首次呈现的
 * 那份记录更全——它带 `globalSequence` 与本次投递的原文字节）；同一历史页重复读取也不重复。
 */
function withHistory(state: ClientState, sessionId: Uuid, items: readonly SnapshotItemHistory[]): ClientState {
  const history = new Map(state.history);
  for (const item of items) {
    if (state.structured.has(item.eventId) || history.has(item.eventId)) continue;
    history.set(item.eventId, recordFromHistoryItem(item, sessionId));
  }
  return { ...state, history };
}

/** 首屏读回的明细资源落到会话上（待处理交互 + 配置项，D7 与 D5 的配置面）。 */
function withReadResources(state: ClientState, sessionId: Uuid, read: SessionReadResult): ClientState {
  const item = read.resources.config_options?.[0];
  const readResources = new Map(state.readResources);
  readResources.set(sessionId, {
    pendingInteractions: read.resources.pending_interactions ?? [],
    configOptions: item?.configOptions ?? [],
    configVersion: item?.version ?? null,
  });
  return { ...state, readResources };
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

/** 按时间位置取最新的一条记录；无记录时为 `null`。 */
function newestRecordOf(records: readonly StructuredRecord[]): StructuredRecord | null {
  let newest: StructuredRecord | null = null;
  for (const record of records) {
    if (newest === null || compareTimelinePosition(positionOf(record), positionOf(newest)) > 0) newest = record;
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
}

/** 一条在途的文件栏查询（请求代次与返回上下文在结果到达时逐项核对）。 */
interface PendingFileQuery {
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
  readonly #listeners = new Set<Listener>();
  readonly #directoryRefreshRequests = new Set<Uuid>();
  readonly #schedule: ((delayMs: number, task: () => void) => () => void) | null;
  /** 在途的文件栏查询（含请求代次与返回上下文）。 */
  readonly #pendingFileQueries = new Map<Uuid, PendingFileQuery>();
  /** 突发事件合并/限流退避定时器；键为 `<sessionId>|<source>`。 */
  readonly #fileRefreshTimers = new Map<string, () => void>();
  /** 已排队但在飞的刷新：一次只允许一个同来源查询。 */
  readonly #fileRefreshQueued = new Set<string>();
  /** 当前对话页的会话；切换时旧会话的文件读取必须作废（迟到的响应不能重新打开文件）。 */
  #activeConversationSession: Uuid | null = null;
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
  }

  /** 当前状态。 */
  get state(): ClientState {
    return this.#state;
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
    const reduced = this.#reduce(event);
    // 连接离开 online（closed/blocked）或建立新连接（authenticated）后，旧连接上的在途
    // 文件查询不会再回来：必须丢掉它们并把来源的 loading 复位，否则 in-flight 标记会让
    // 下一次刷新被永久排队、来源永久停在加载中。
    const resetFiles = event.kind === "authenticated" || event.kind === "closed" || event.kind === "blocked";
    if (resetFiles) this.#resetFileQueries();
    const next = resetFiles ? this.#withFileLoadingStopped(reduced) : reduced;
    if (next === this.#state) return;
    this.#commit(next);
    this.#drainQueuedFileRefreshes();
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
      case "authenticated":
        return {
          ...this.#state,
          deviceId: event.info.deviceId,
          selectedFeatures: [...(event.selectedFeatures ?? [])],
        };
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
        return this.#withConnection({ kind: "connection_closed", retryable: event.retryable });
      case "blocked":
        return this.#withConnection({ kind: "blocked", cause: event.cause });
      case "snapshot_verified":
        return { ...this.#state, resources: event.snapshot.resources, lastSyncedAtMs: this.#now() };
      case "event":
        return this.#withEvent(event.event);
      case "command_result":
        return this.#withCommandResult(event.result);
      default:
        return this.#state;
    }
  }

  #withConnection(event: ConnectionEvent): ClientState {
    return { ...this.#state, connection: applyConnectionEvent(this.#state.connection, event) };
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
    if (!next.structured.has(record.eventId)) {
      const structured = new Map(next.structured);
      structured.set(record.eventId, record);
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
    if (summary !== null) next = { ...next, resources: withSessionSummary(next.resources, summary) };

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

    // 7) 文件栏：当前会话的 `file.changed` 只触发来源刷新与「过时」标记，不取消 Agent，
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
    if (channel !== null) {
      const messageId = viewText(record.view, "messageId");
      const deltaIndex = viewText(record.view, "deltaIndex");
      const text = viewText(record.view, "text");
      if (messageId === null || deltaIndex === null || text === null) return state;
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
          text,
          createdAt: record.createdAt,
        }),
      );
      return { ...state, volatile };
    }

    if (record.eventType === "agent.message.completed") {
      const messageId = viewText(record.view, "messageId");
      if (messageId === null) return state;
      const existing = state.volatile.get(messageId);
      if (existing === undefined || existing.completed) return state;
      const volatile = new Map(state.volatile);
      volatile.set(messageId, { ...existing, completed: true });
      return { ...state, volatile };
    }

    if (record.eventType === "turn.delta_compacted") {
      const turnId = viewText(record.view, "turnId");
      if (turnId === null || sessionId === null) return state;
      const discardedTurns = new Set(state.discardedTurns);
      discardedTurns.add(`${sessionId}|${turnId}`);
      const volatile = new Map(state.volatile);
      for (const [messageId, tail] of volatile) {
        if (tail.turnId === turnId && tail.sessionId === sessionId) volatile.delete(messageId);
      }
      return { ...state, discardedTurns, volatile };
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
   * `session.read` 结果并入状态。
   *
   * 三种用途各自合并自己那一份资源（D2）：
   *
   * - `first`：整页替换消息、记下读回的明细资源（待处理交互/配置项）、记录历史页标记；
   *   并把读取结果里出现的 `messageId` 从易失叠加层里清掉——**读取结果为准**（D6）。
   * - `earlier`：消息向上翻页（不重不漏由 `assertPagesDoNotOverlap` 把关）。
   * - `earlier_history`：历史条目向上翻页（`assertHistoryPagesDoNotOverlap`）；
   *   它**不**动消息页，消息分页也**不**动历史页——两套游标互不影响。
   */
  #applyReadResult(state: ClientState, result: CommandResultMessage): ClientState {
    const pending = this.#pendingReads.get(result.body.requestId);
    if (pending === undefined) return state;
    this.#pendingReads.delete(result.body.requestId);
    const threads = new Map(state.threads);
    const sessionId = pending.sessionId;
    const existing = threads.get(sessionId) ?? EMPTY_THREAD;

    if (result.body.command !== "session.read" || result.body.status !== "completed") {
      threads.set(sessionId, {
        ...existing,
        loadingEarlier: false,
        loadingHistory: false,
        pagingError: pending.intent === "refresh_history" ? existing.pagingError : describeCommandFailure(result),
        historyRefreshError: pending.intent === "refresh_history" ? describeCommandFailure(result) : existing.historyRefreshError ?? null,
      });
      return { ...state, threads };
    }

    const read = result.body.result as SessionReadResult;
    if (pending.intent === "refresh_history") {
      const loaded = toLoadedHistoryPage(read);
      const hadHistory = existing.page !== null || [...state.history.values()].some(record => record.sessionId === sessionId);
      threads.set(sessionId, {
        ...existing,
        historyHasEarlier: hadHistory ? existing.historyHasEarlier : loaded.hasEarlier,
        nextBeforeHistory: hadHistory ? existing.nextBeforeHistory : loaded.nextBeforeHistory,
        loadingHistory: false,
        pagingError: existing.pagingError,
        historyRefreshError: null,
      });
      // 最新一页可以与旧记录重叠：按 eventId 去重合并，不重置消息页和历史翻页位置。
      return { ...withHistory(state, sessionId, loaded.items), threads };
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
      return { ...withHistory(state, sessionId, loaded.items), threads };
    }

    const loaded = toLoadedPage(read);
    // 读取结果胜出：同 id 的易失累加内容不再呈现（MUST NOT 叠加成重复文本）。
    const volatileAfterRead = new Map(state.volatile);
    for (const message of loaded.messages) volatileAfterRead.delete(message.messageId);
    // 首屏同时把历史条目与明细资源（待处理交互、配置项）落进来；翻页请求只并入自己那一份。
    const staged =
      pending.intent === "first"
        ? withReadResources(withHistory(state, sessionId, read.resources.history ?? []), sessionId, read)
        : state;

    if (pending.intent === "first" || existing.page === null) {
      // 首屏同时拿到历史那一页的页标记与续取游标；消息翻页请求不动它们（两套游标独立）。
      const historyPage = pending.intent === "first" ? toLoadedHistoryPage(read) : null;
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
      return { ...staged, threads, volatile: volatileAfterRead };
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
    return { ...staged, threads, volatile: volatileAfterRead };
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
      connection: this.#state.connection.state,
      lastSyncedAtMs: this.#state.lastSyncedAtMs,
      query: this.#state.directoryQuery,
      filter: this.#state.directoryFilter,
      pageInput: this.#state.directoryListPage,
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
      connection: this.#state.connection.state,
      directoryId,
      ownerLabelOf: this.#ownerLabelOf,
      nowMs: nowMs ?? this.#now(),
      pageInput: this.#state.directoryDetailDirectoryId === directoryId ? this.#state.directoryDetailPage : "",
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
    });
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
    const thread = this.#state.threads.get(sessionId);
    const summary = this.#findSession(sessionId);
    if (thread === undefined && summary === undefined) return null;

    const header: ConversationHeaderModel = {
      sessionId,
      title: titleOf(summary),
      agentName: summary?.agent.name ?? "",
      state: summary?.state ?? "idle",
      directoryId: summary?.workspace?.directoryId ?? null,
      originOfflineLabel: originOfflineLabelOf(summary, this.#ownerLabelOf),
    };
    const origin: "local" | { readonly kind: "remote"; readonly online: boolean } =
      summary?.origin.kind === "remote" ? { kind: "remote", online: summary.origin.online } : "local";
    const sessionOpen = (summary?.state ?? "idle") !== "closed";
    const connectionOnline = this.#state.connection.state === "online";

    // 结构化记录：实时与历史按 `eventId` 去重（实时优先），再筛出本会话。
    const records = this.#recordsOf(sessionId);
    const interactions = buildInteractionViews({
      records,
      pending: this.#state.readResources.get(sessionId)?.pendingInteractions ?? [],
      responseRequestIds: this.#state.responseRequestIds,
      commands: this.#state.commands,
      deviceId: this.#state.deviceId,
      sessionOpen,
    });
    const entries = buildTimeline({
      messages: thread?.page?.messages ?? [],
      records,
      volatile: [...this.#state.volatile.values()].filter((tail) => tail.sessionId === sessionId),
      interactions,
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
    const activeTurn = activeTurnOf(records);
    const input: ConversationInputModel = buildConversationInputModel({
      sessionOpen,
      connectionOnline,
      originLocal: origin === "local",
      turnInProgress: activeTurn !== null,
      canCancel: sessionOpen && activeTurn !== null,
      promptCommand: this.#lastCommandOf(this.#state.promptRequestIds.get(sessionId)),
      cancelCommand:
        activeTurn === null ? null : this.#lastCommandOf(this.#state.cancelRequestIds.get(activeTurn.turnId)),
      draft: this.#state.drafts.get(sessionId) ?? "",
    });

    return buildConversationModel({
      header,
      page: thread?.page ?? null,
      usage: this.#state.usage.get(sessionId) ?? null,
      degraded,
      timeline: {
        entries,
        structuredHistory: structuredHistoryAvailability({
          origin,
          historyFeatureNegotiated: this.#state.selectedFeatures.includes(SESSION_HISTORY_FEATURE),
        }),
      },
      loadingEarlier: thread?.loadingEarlier ?? false,
      pagingError: thread?.pagingError ?? null,
      historyHasEarlier: thread?.historyHasEarlier ?? false,
      loadingHistory: thread?.loadingHistory ?? false,
      nextBeforeHistory: thread?.nextBeforeHistory ?? null,
      input,
      interactions,
      settings,
      fileChanges: buildFileChangeViews(records),
      // 左栏只读快照：同目录会话、目录上下文与「已呈现 / 总数」都由快照推出，没有命令出口。
      sidebar: buildConversationSidebar({
        resources: this.#state.resources,
        workspace: summary?.workspace ?? null,
        sessionId,
        ownerLabelOf: this.#ownerLabelOf,
        query: this.#state.conversationSidebarQuery,
        pageInput: this.#state.conversationSidebarPageInput,
        nowMs: nowMs ?? this.#now(),
      }),
      capabilities: conversationCapabilities({
        connectionOnline,
        sessionState: summary?.state ?? "idle",
        origin,
      }),
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
    records.sort((left, right) => compareTimelinePosition(positionOf(left), positionOf(right)));
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
    const record = this.#state.structured.get(id) ?? this.#state.history.get(id);
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

  /**
   * 首次读取会话明细（正文 + 配置项 + 待处理交互，协商到 feature 时含结构化历史）。
   *
   * 一次只允许一个在途读取：两页并发返回时无法判断谁该前置谁。
   */
  loadSession(sessionId: Uuid): void {
    if (this.#pendingReads.size > 0) return;
    this.#requestPage(sessionId, "first");
  }

  /** 向上翻页（消息）：续取游标取上一页最早一条的 `(createdAt, messageId)`。 */
  loadEarlier(sessionId: Uuid): void {
    const before = this.#state.threads.get(sessionId)?.page?.earliestCursor ?? null;
    if (before === null) return;
    this.#requestPage(sessionId, "earlier", { before });
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
    if (beforeHistory === null || this.#pendingReads.size > 0) return;
    this.#requestPage(sessionId, "earlier_history", { beforeHistory });
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
   * 只有 `answerable`（本设备亲历、会话未关闭、请求下发了可选项）才派发命令；其余边界各自给出
   * 明确原因，因此「已被其它设备处理」不可能被重复提交、「仅历史读到」不可能有可点的按钮。
   * 提交之后呈现为**待确认**，直到服务端终态（`submitting` → `handled_here`）。
   */
  resolvePermission(sessionId: Uuid, interactionId: string, optionId: string): CommandOutcome {
    const interaction = this.#interactionOf(sessionId, interactionId);
    const blocked = responseGate(interaction);
    if (blocked !== null) return blocked;
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
        payload: { interactionId, optionId },
      }),
      patch: (id) => ({ responseRequestIds: withEntry(this.#state.responseRequestIds, interactionId, id) }),
    });
    return requestId === null ? NOT_CONNECTED : { ok: true, requestId };
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
    const answers = interaction?.availability.kind === "answerable" ? interaction.availability.answers : [];
    if (!answers.some((answer) => answer.via === "action" && answer.action === action)) {
      return { ok: false, reason: "这次请求不支持该动作，不能提交" };
    }
    const payload: ElicitationRespondPayload =
      action === "submit" ? { interactionId, action, values } : { interactionId, action, values: null };
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

  // ── 文件栏（WP4） ──────────────────────────────────────────────────────────

  /**
   * 文件栏 view model：三来源 + 会话改动归并 + 互斥内容态。
   *
   * 会话变更完全由已加载记录派生（实时 + 历史、按 `eventId` 去重后只留本会话），因此重放不会
   * 产生重复记录，手动修改/其它会话的记录也不会混入。`historyHasEarlier` 只在读过历史页时
   * 有值；未读时为 `null`，模型标注「范围未知」而不是声称完整。
   */
  sessionFilesPage(sessionId: Uuid): SessionFilesPanelModel {
    const thread = this.#state.threads.get(sessionId);
    // 未协商结构化历史时根本没有加载过历史页：范围是「未知」，不能声称完整。
    const historyLoaded = this.#state.selectedFeatures.includes(SESSION_HISTORY_FEATURE);
    return buildSessionFilesPanelModel({
      sessionId,
      state: this.#state.sessionFiles,
      records: this.#recordsOf(sessionId),
      historyHasEarlier: thread === undefined || !historyLoaded ? null : thread.historyHasEarlier,
      connectionOnline: this.#state.connection.state === "online",
      historyRefreshSupported: historyLoaded,
      historyReadPending: this.#pendingReads.size > 0,
      historyRefreshError: thread?.historyRefreshError ?? null,
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
    this.#activeConversationSession = sessionId;
    const content = this.#state.sessionFiles.content;
    if (content.kind === "file" && content.context.sessionId !== sessionId) {
      const generation = this.#state.sessionFiles.generation + 1;
      this.#dropPendingContentQueries();
      this.#commit(this.#withFileContent(this.#state, { kind: "conversation" }, generation));
    }
  }

  /** 切换文件栏来源；进入「全部文件」/「Git 变更」时立即刷新，会话变更不发命令。 */
  selectFileSource(sessionId: Uuid, source: FileSource): void {
    this.#activeConversationSession = sessionId;
    const selectedSources = new Map(this.#state.sessionFiles.selectedSources);
    selectedSources.set(sessionId, source);
    this.#commit({ ...this.#state, sessionFiles: { ...this.#state.sessionFiles, selectedSources } });
    if (source === "session") return;
    this.#refreshFileSource(sessionId, source);
  }

  /** 手动刷新当前来源；会话变更只重读最新结构化历史，不重置对话与历史分页。 */
  refreshFiles(sessionId: Uuid): void {
    this.#activeConversationSession = sessionId;
    const source = selectedSourceOf(this.#state.sessionFiles, sessionId);
    if (source === "session") {
      if (!this.#state.selectedFeatures.includes(SESSION_HISTORY_FEATURE) || this.#pendingReads.size > 0) return;
      this.#requestPage(sessionId, "refresh_history");
      return;
    }
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
    // 旧主机/远程导入会话没有本机目录可枚举：即使调用方拿到一个路径也不派发。
    if (this.#fileSourceUnsupported(sessionId, "all") !== null) return;
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
    const git = sourcesOf(this.#state.sessionFiles, sessionId).git;
    if (git.unsupported !== null || git.next === null || git.loading) return;
    this.#dispatchGitStatus(sessionId, git.next, true);
  }

  /**
   * 打开文件内容，替换中间对话区（互斥）。
   *
   * 关闭/切换会话后的迟到响应由代次与上下文丢弃；来源为「会话变更」时直接从记录构造，
   * 不派发查询。请求代次每次都递增，因此 A 的响应不可能覆盖随后打开的 B。
   */
  openFileContent(sessionId: Uuid, source: FileSource, path: string): void {
    this.#activeConversationSession = sessionId;
    const generation = this.#state.sessionFiles.generation + 1;
    this.#dropPendingContentQueries();
    const context: FileViewContext = { sessionId, source, path };

    if (source === "session") {
      const thread = this.#state.threads.get(sessionId);
      const model = buildSessionChangesModel(this.#recordsOf(sessionId), thread?.historyHasEarlier ?? null);
      const found = model.files.find((file) => file.path === path || file.key === `path:${path}`);
      if (found === undefined || !found.readable) {
        this.#commit(this.#withFileContent(this.#state, {
          kind: "file",
          context,
          generation,
          stale: false,
          status: "error",
          error: unreadableSessionChangeError(path),
        }, generation));
        return;
      }
      this.#commit(this.#withFileContent(this.#state, {
        kind: "file",
        context,
        generation,
        stale: false,
        status: "ready",
        content: { kind: "session_changes", path, records: found.records },
      }, generation));
      return;
    }

    const unsupported = this.#fileSourceUnsupported(sessionId, source);
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

    if (source === "all") {
      this.#dispatchFileRead({
        sessionId,
        source,
        path,
        generation,
        purpose: "preview",
        offset: "0",
        chunks: [],
      });
      return;
    }
    this.#dispatchGitDiff({ sessionId, source, path, generation });
  }

  /** 关闭文件视图，恢复对话；递增代次使所有在途响应作废。 */
  closeFileContent(): void {
    if (this.#state.sessionFiles.content.kind === "conversation") return;
    const generation = this.#state.sessionFiles.generation + 1;
    this.#dropPendingContentQueries();
    this.#commit(this.#withFileContent(this.#state, { kind: "conversation" }, generation));
  }

  /**
   * 提交一条**文本**提示。
   *
   * 闸门（任一不过即**不发命令**）：会话存在且可提交（关闭/离线/imported 来源都会给出原因）、
   * 文本去除首尾空白后非空。附件不在签名里——v1 只支持文本，附件 MUST NOT 被静默丢弃后提交
   * 剩余文本（因此本方法根本不接受附件参数）。
   */
  submitPrompt(sessionId: Uuid, text: string): CommandOutcome {
    const page = this.conversationPage(sessionId);
    if (page === null) return { ok: false, reason: "会话不在本设备的状态里" };
    if (!page.input.canSubmit) return { ok: false, reason: page.input.disabledReason ?? "当前不能提交" };
    const trimmed = text.trim();
    if (trimmed.length === 0) return { ok: false, reason: "不能提交空文本" };
    const requestId = this.#dispatchIntent({
      sessionId,
      build: (id) => ({
        requestId: id,
        command: "session.prompt",
        sessionId,
        payload: { content: [{ type: "text", text: trimmed }] },
      }),
      patch: (id) => ({
        promptRequestIds: withEntry(this.#state.promptRequestIds, sessionId, id),
        drafts: withoutEntry(this.#state.drafts, sessionId),
      }),
    });
    return requestId === null ? NOT_CONNECTED : { ok: true, requestId };
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
   * - 远程 imported 会话：保留已收到的会话变更，全部文件/Git 明确暂不支持，不查询本机目录。
   */
  #fileSourceUnsupported(sessionId: Uuid, source: FileSource): "feature_unavailable" | "remote_imported" | null {
    if (source === "session") return null;
    if (!this.#state.selectedFeatures.includes(SESSION_FILES_FEATURE)) return "feature_unavailable";
    return this.#findSession(sessionId)?.origin.kind === "remote" ? "remote_imported" : null;
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
    for (const cancel of this.#fileRefreshTimers.values()) cancel();
    this.#fileRefreshTimers.clear();
    this.#fileRefreshQueued.clear();
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
    if (source === "session" || this.#schedule === null) return;
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
    if (source === "session") return;
    // 立即刷新取代尚未到期的合并/退避定时器，避免同一次意图排出两条查询。
    this.#cancelFileRefreshTimer(this.#fileKey(sessionId, source));
    const sources = sourcesOf(this.#state.sessionFiles, sessionId);
    const unsupported = this.#fileSourceUnsupported(sessionId, source);
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
      this.#dispatchAllFilesPage(sessionId, "", undefined, false);
    } else {
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
    this.#pendingFileQueries.set(body.requestId, pending);
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
      return afterFailure;
    }

    switch (pending.kind) {
      case "list":
        return this.#applyAllFilesPage(state, pending, result.body.result);
      case "git_status":
        return this.#applyGitStatusPage(state, pending, result.body.result);
      case "read":
        return this.#applyFilePreviewChunk(state, pending, result.body.result);
      case "git_diff":
        return this.#applyGitDiff(state, pending, result.body.result);
    }
  }

  /** 失败统一落到来源或当前内容；来源失败不清空已成功加载的条目。 */
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
   * 当前会话的 `file.changed`：只标「过时」并按来源排一次合并刷新，不覆盖正在阅读的内容。
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

    const source = selectedSourceOf(state.sessionFiles, sessionId);
    if (source === "session") return next;
    const sources = sourcesOf(state.sessionFiles, sessionId);
    const loaded = source === "all" ? sources.all.directories.size > 0 : sources.git.gitState !== null;
    if (!loaded) return next;
    const unsupported = this.#fileSourceUnsupported(sessionId, source);
    if (unsupported !== null) return next;
    this.#scheduleFileRefresh(sessionId, source, FILE_REFRESH_DEBOUNCE_MS);
    return next;
  }

  #requestPage(
    sessionId: Uuid,
    intent: ReadIntent,
    cursors: { readonly before?: ReadCursor; readonly beforeHistory?: string } = {},
  ): void {
    this.#dispatchIntent({
      sessionId,
      build: (requestId) => {
        this.#pendingReads.set(requestId, { sessionId, intent });
        // 与 `paging.ts` 的 `buildReadPayload` 同一形状；这里就地构造是为了让 `include`
        // 保持 `SessionReadInclude[]`（协议类型），而 `PageRequest` 声明的是更宽的 `string[]`。
        const payload: { include: readonly SessionReadInclude[]; before?: ReadCursor; beforeHistory?: string } = {
          include: this.#includesFor(intent),
        };
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
          loadingHistory: intent === "earlier_history" || intent === "refresh_history",
          pagingError: intent === "refresh_history" ? existing.pagingError : null,
          historyRefreshError: intent === "refresh_history" ? null : existing.historyRefreshError ?? null,
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
  #includesFor(intent: ReadIntent): readonly SessionReadInclude[] {
    const historyNegotiated = this.#state.selectedFeatures.includes(SESSION_HISTORY_FEATURE);
    switch (intent) {
      case "earlier":
        return ["messages"];
      case "earlier_history":
      case "refresh_history":
        return ["history"];
      case "first":
        return historyNegotiated ? [...CONVERSATION_INCLUDES, "history"] : CONVERSATION_INCLUDES;
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
    readonly build: (requestId: Uuid) => CommandBody;
    readonly patch?: (requestId: Uuid) => Partial<ClientState>;
  }): Uuid | null {
    const connectionId = this.#identity.connectionId;
    if (connectionId === null) return null;

    const requestId = this.#ids.uuid();
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

  /** 本会话的结构化记录（实时与历史按 `eventId` 去重，实时优先）。 */
  #recordsOf(sessionId: Uuid): readonly StructuredRecord[] {
    return mergeStructuredRecords([...this.#state.structured.values()], [...this.#state.history.values()]).filter(
      (record) => record.sessionId === sessionId,
    );
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

/** 会话变更里不可读取/越界的条目。 */
function unreadableSessionChangeError(path: string): PublicError {
  return {
    code: "internal.unavailable",
    message: `无法读取工作目录外的文件：${path}`,
    retryable: false,
    details: {},
  };
}

/** 来源暂不可用（旧主机/远程导入会话）。 */
function unsupportedSourceError(reason: "feature_unavailable" | "remote_imported"): PublicError {
  return {
    code: "capability.unsupported_by_broker",
    message:
      reason === "remote_imported"
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
