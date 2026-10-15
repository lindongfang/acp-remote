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
  CommandResultValue,
  ConfigOptionView,
  ElicitationResolutionAction,
  ElicitationRespondPayload,
  EventMessage,
  ModeState,
  PairingQrPayload,
  PublicError,
  SessionReadInclude,
  SessionReadResult,
  SessionSummary,
  SnapshotItemHistory,
  SnapshotItemMessages,
  SnapshotItemPendingInteractions,
  Uuid,
} from "../protocol";
import type { ReadCursor, StagedResources, SyncClientEvent } from "../sync-client";
import {
  AgentConnectionOverlayStore,
  SESSION_HISTORY_FEATURE,
  assertHistoryPagesDoNotOverlap,
  assertPagesDoNotOverlap,
  toLoadedHistoryPage,
  toLoadedPage,
} from "../sync-client";
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
  mergeStructuredRecords,
  positionOf,
  recordFromEvent,
  recordFromHistoryItem,
  recordGroupOf,
  viewText,
} from "../domain";
import type { AgentConnectionOverlay } from "../domain/agent-catalog";
import type { ConnectionIdentity, IdentifierSource, SyncGateway } from "./ports";
import type { ConversationHeaderModel, ConversationPageModel, UsageReading } from "./conversation-model";
import { buildConversationModel, conversationCapabilities, structuredHistoryAvailability } from "./conversation-model";
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
import { buildDegradedRecordModel, isDegradedRecord } from "./degradation-model";
import { buildInteractionViews } from "./interaction-model";
import type { CreateSessionEntryModel, CreateSessionRefs } from "./create-session-model";
import {
  assertCreateSessionRefs,
  buildCreateSessionCommand,
  buildCreateSessionEntryModel,
} from "./create-session-model";
import type {
  DirectoryDetailModel,
  DirectoryFilter,
  DirectoryPageModel,
  SessionRowModel,
} from "./directory-model";
import { buildDirectoryDetailModel, buildDirectoryPageModel, buildSessionRowsByAlias } from "./directory-model";
import type { HostIdentityModel, HostPanelModel } from "./host-panel-model";
import { buildHostPanelModel } from "./host-panel-model";
import type { PairingPageModel, PairingPollResult, PairingWiring } from "./pairing-model";
import {
  PAIRING_POLL_INTERVAL_MS,
  applyClaiming,
  applyPairingCloseoutFailure,
  applyPairingFailure,
  applyPollResult,
  applyScannedQr,
  initialPairingPageModel,
  pairedHostRecordFor,
  pairedHostWriteFailure,
  publicErrorForCloseoutFailure,
  publicErrorForQrInput,
  publicErrorForTransportFailure,
} from "./pairing-model";

/**
 * 对话页首屏请求的明细资源。
 *
 * 正文（`messages`）之外还要两样：配置项（模型/模式的可选项，D5 的配置面）与待处理交互
 * （`pending_interactions`：历史读回的待处理项要呈现为「不可操作并说明原因」）。
 * 结构化历史（`history`）不在这里——它只在服务端协商到 `core.session-history.v1` 时才带上
 * （未协商却请求它会得到明确的不支持类错误，`design.md` 的 Compatibility）。
 */
const CONVERSATION_INCLUDES: readonly SessionReadInclude[] = ["messages", "config_options", "pending_interactions"];

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
}

/** 一次 `session.read` 的用途：首屏、向上翻消息、向上翻历史。三者合并结果的方式不同。 */
type ReadIntent = "first" | "earlier" | "earlier_history";

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
  /** 本次认证授予的 scopes；未认证为 `null`（创建入口据此判定不可用）。 */
  readonly scopes: readonly string[] | null;
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
  readonly createSheetAlias: string | null;
  readonly diagnosticsEventId: string | null;
  /** 目录页查询词；与详情页的查询词是**两个独立**状态（页面各有自己的搜索框）。 */
  readonly directoryQuery: string;
  readonly directoryFilter: DirectoryFilter;
  readonly directoryDetailQuery: string;
  /** 详情页当前月份档；`"all"` 表示呈现全部月份。 */
  readonly directoryDetailMonth: string | "all";
  readonly pairing: PairingPageModel;
  readonly host: HostIdentityModel | null;
}

/** 初始状态：未配对、没有快照。 */
export function initialClientState(): ClientState {
  return {
    connection: initialConnectionState(),
    scopes: null,
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
    createSheetAlias: null,
    diagnosticsEventId: null,
    directoryQuery: "",
    directoryFilter: "all",
    directoryDetailQuery: "",
    directoryDetailMonth: "all",
    pairing: initialPairingPageModel(),
    host: null,
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
function toResultRecord(value: CommandResultValue): Readonly<Record<string, unknown>> | null {
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
}

/**
 * 正在进行的一次配对。
 *
 * **不进 `ClientState`**：二维码载荷含 `pairingSecret`，而 `ClientState` 是可导出、可快照的
 * 公开状态（诊断与导出配置都会读它）。载荷只活在仓库的私有字段里，并在已批准或终态时立即
 * 丢掉（§7.3：成功或终态后清除二维码 payload 与内存中的 secret）。
 */
interface PairingSession {
  readonly qr: PairingQrPayload;
  cancel: (() => void) | null;
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
  #pairingSession: PairingSession | null = null;
  #state: ClientState = initialClientState();

  constructor(options: ClientStoreOptions) {
    this.#sync = options.sync;
    this.#ids = options.ids;
    this.#now = options.now;
    this.#identity = options.identity;
    this.#ownerLabelOf = options.ownerLabelOf ?? ((ownerNodeId: string) => ownerNodeId);
    this.#pairing = options.pairing ?? null;
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
    const next = this.#reduce(event);
    if (next === this.#state) return;
    this.#commit(next);
  }

  #reduce(event: SyncClientEvent): ClientState {
    switch (event.kind) {
      case "authenticated":
        return {
          ...this.#state,
          scopes: [...event.info.scopes],
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
    if (updated.state === "uncertain" && !next.uncertainRequestIds.includes(updated.requestId)) {
      // 结果不确定**只**在服务端确认后登记，且没有任何自动清除路径（R16 场景 4）。
      next = { ...next, uncertainRequestIds: [...next.uncertainRequestIds, updated.requestId] };
    }
    next = this.#withModeListResult(next, updated, result);
    return this.#applyReadResult(next, result);
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
    const existing = threads.get(sessionId) ?? {
      page: null,
      items: [] as readonly SnapshotItemMessages[],
      historyHasEarlier: false,
      nextBeforeHistory: null,
      loadingEarlier: false,
      loadingHistory: false,
      pagingError: null,
    };

    if (result.body.command !== "session.read" || result.body.status !== "completed") {
      threads.set(sessionId, {
        ...existing,
        loadingEarlier: false,
        loadingHistory: false,
        pagingError: describeCommandFailure(result),
      });
      return { ...state, threads };
    }

    const read = result.body.result as SessionReadResult;
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

  /** 目录页模型（R15）。数据只来自快照，不发任何命令；查询与筛选只是纯渲染状态。 */
  directoryPage(): DirectoryPageModel {
    return buildDirectoryPageModel({
      resources: this.#state.resources,
      connection: this.#state.connection.state,
      lastSyncedAtMs: this.#state.lastSyncedAtMs,
      query: this.#state.directoryQuery,
      filter: this.#state.directoryFilter,
    });
  }

  /** 目录详情模型（R15）。 */
  directoryDetail(alias: string): DirectoryDetailModel {
    return buildDirectoryDetailModel({
      resources: this.#state.resources,
      connection: this.#state.connection.state,
      alias,
      ownerLabelOf: this.#ownerLabelOf,
      query: this.#state.directoryDetailQuery,
      month: this.#state.directoryDetailMonth,
    });
  }

  /**
   * 目录页的内联会话行：按目录别名分组、组内按时间倒序。
   *
   * 只依赖快照，**不读** `directoryDetailQuery`/`directoryDetailMonth`，也不派发任何命令——
   * 因此详情页里敲过的关键词或选过的月份不会改变目录页看到的内容（spec「与目录页的查询词
   * 互不影响」的反向）。
   */
  directoryPageSessions(): ReadonlyMap<string, readonly SessionRowModel[]> {
    return buildSessionRowsByAlias({ resources: this.#state.resources, ownerLabelOf: this.#ownerLabelOf });
  }

  /** 本机已配置的 Agent 目录（快照 `agents` + 连接覆盖层，R19）。 */
  agentCatalog(): readonly AgentCatalogEntryView[] {
    return buildAgentCatalogView(this.#state.resources?.agents ?? [], this.#state.overlay).entries;
  }

  /** 创建弹层模型（R16）。 */
  createEntry(alias: string): CreateSessionEntryModel {
    const requestId = this.#state.createRequestIds.get(alias);
    return buildCreateSessionEntryModel({
      directoryAlias: alias,
      scopes: this.#state.scopes,
      agents: this.agentCatalog(),
      record: requestId === undefined ? null : (this.#state.commands.get(requestId) ?? null),
      connectionOnline: this.#state.connection.state === "online",
    });
  }

  /**
   * 对话页模型（R17 + R18 + 时间线 + 输入/交互/设置 + 两套分页）。
   *
   * 会话既不在快照里也没有线程时为 `null`（没有任何可呈现的东西）。判定全部在这里与
   * `conversation-model.ts` / `conversation-timeline.ts` / `interaction-model.ts` 里完成，
   * 组件只消费结果。
   */
  conversationPage(sessionId: Uuid): ConversationPageModel | null {
    const thread = this.#state.threads.get(sessionId);
    const summary = this.#findSession(sessionId);
    if (thread === undefined && summary === undefined) return null;

    const header: ConversationHeaderModel = {
      sessionId,
      title: titleOf(summary),
      agentName: summary?.agent.name ?? "",
      state: summary?.state ?? "idle",
      directoryAlias: summary?.workspace?.alias ?? null,
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
    return this.#state.pairing;
  }

  /**
   * 诊断抽屉模型：选中**降级条目**的原文与元数据（R18）。
   *
   * 记录从结构化记录里按 `eventId` 取（实时与历史都在内），因此历史条目的降级同样可诊断。
   * 内容齐备、走专用视图的记录**不**在这里返回模型：它没有降级原因可讲，硬造一个会让抽屉
   * 谎报归因。要看待呈现记录的原文，用它的视图模型自带的诊断数据
   * （例如 `FileChangeView.diagnostics`）。
   */
  diagnostics(): DegradedEventModel | null {
    const id = this.#state.diagnosticsEventId;
    if (id === null) return null;
    const record = this.#state.structured.get(id) ?? this.#state.history.get(id);
    if (record === undefined || !isDegradedRecord(record)) return null;
    return buildDegradedRecordModel(record);
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

  /** 打开创建弹层。 */
  openCreateSheet(alias: string): void {
    this.#commit({ ...this.#state, createSheetAlias: alias });
  }

  /** 关闭创建弹层。 */
  closeCreateSheet(): void {
    this.#commit({ ...this.#state, createSheetAlias: null });
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
   * 与 `setDirectoryDetailQuery` 分开，是因为两页各有独立搜索框：进入详情页不得清空
   * 目录页的查询词，返回时它仍在（spec「与目录页的查询词互不影响」）。
   */
  setDirectoryQuery(query: string): void {
    this.#commit({ ...this.#state, directoryQuery: query });
  }

  /** 更新目录页状态筛选。 */
  setDirectoryFilter(filter: DirectoryFilter): void {
    this.#commit({ ...this.#state, directoryFilter: filter });
  }

  /** 更新目录详情页的目录内查询词。 */
  setDirectoryDetailQuery(query: string): void {
    this.#commit({ ...this.#state, directoryDetailQuery: query });
  }

  /** 更新目录详情页的月份档；`"all"` 表示呈现全部月份。 */
  setDirectoryDetailMonth(month: string | "all"): void {
    this.#commit({ ...this.#state, directoryDetailMonth: month });
  }

  /**
   * 进入某个目录详情页时调用：目录内查询词与月份档都是**单份**状态，必须在新目录上复位。
   *
   * 两者都挂在 store 单例上、跨路由存活；不复位的话，在目录 A 选的月份档（或敲的查询词）
   * 会带进目录 B，把 B 的内容整片挡掉——而控件上已经看不到那个档了。原型在进入目录与
   * `hashchange` 时同样把 `A.month` 重置为 `all`。
   *
   * 一次 commit 改两处（形状同 {@link pairSucceeded}），避免出现「查询词已清、月份档还留着」
   * 的中间态被渲染出来。
   */
  resetDirectoryDetailView(): void {
    this.#commit({ ...this.#state, directoryDetailQuery: "", directoryDetailMonth: "all" });
  }

  /** 更新配对页状态。 */
  setPairing(pairing: PairingPageModel): void {
    this.#commit({ ...this.#state, pairing });
  }

  /**
   * 配对页的主用例（页面进页时调用一次）：地址片段里有配对内容就读取并**立即清除**该片段，
   * 然后进入认领流程；没有内容时不动状态——页面停在输入入口等用户粘贴。
   *
   * 片段读取与清除都在组合根（地址栏是平台能力，`AGENTS.md` §6），本方法只消费它的结果。
   */
  async startPairingFromAddress(): Promise<void> {
    const text = this.#pairing?.takeQrTextFromFragment() ?? null;
    if (text === null) return;
    await this.#beginPairing(text);
  }

  /**
   * 粘贴入口：一段二维码内容（地址 / 片段 / 载荷）→ 结构校验 → 认领 → 轮询 → 已配对。
   *
   * 结构校验不过时**明确失败且不发任何请求**（不尽力解析）；配对进行中重复提交被忽略，
   * 否则会出现两条并行的 claim 与轮询。
   */
  async submitPairingQrText(text: string): Promise<void> {
    if (this.#pairing === null) {
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
    if (this.#pairingSession !== null) return;
    await this.#beginPairing(text);
  }

  /** 结构校验 → claim → 首次轮询。校验不过时在这里收口，绝不发请求。 */
  async #beginPairing(text: string): Promise<void> {
    const wiring = this.#pairing;
    if (wiring === null) return;

    let qr: PairingQrPayload;
    try {
      qr = wiring.transport.readQrPayload(text);
    } catch (error) {
      this.#commit({
        ...this.#state,
        pairing: applyPairingFailure(this.#state.pairing, publicErrorForQrInput(error)),
      });
      return;
    }

    // 载荷从这一刻起由私有字段持有；`applyScannedQr` 只把**公开**的主机出示信息写进状态。
    this.#pairingSession = { qr, cancel: null };
    this.#commit({ ...this.#state, pairing: applyClaiming(applyScannedQr(this.#state.pairing, qr)) });

    // 传输层的两次 await 都必须在本方法里收口：端口约定用返回值表达业务失败，但**抛出**
    // （网络不可达、响应不是 JSON、设备身份绑在另一个来源）同样是真实结果。让它冒泡会留下
    // 一个非 null 的 `#pairingSession`：`isPairingInFlight` 恒真 → 输入框一直禁用、重复提交被
    // 提前 return、`canRestart` 为 false、被取消的定时器也不会重排——页面里没有任何恢复路径。
    let claimError: PublicError | null;
    try {
      claimError = await wiring.transport.claim({ qr, canonicalOrigin: qr.canonicalOrigin });
    } catch (error) {
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

  /** 安排下一次状态轮询；会话已被取代时不再安排。 */
  #schedulePoll(qr: PairingQrPayload): void {
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
  async #pollOnce(qr: PairingQrPayload): Promise<void> {
    const wiring = this.#pairing;
    const session = this.#pairingSession;
    if (wiring === null || session === null || session.qr.pairingId !== qr.pairingId) return;
    session.cancel = null;

    // 与 `claim` 同样必须收口：轮询抛出时留下「进行中」的状态会让输入框一直禁用、
    // 也不会有下一次轮询。策略与 claim 一致——**失败即结束**（见 `#endPairing` 的注释）：
    // 页面因此拿到可读、可重试的 `failed`（输入框解禁、出现「重新扫码」），
    // 而不是把含秘密的载荷无限期留在内存里等一个可能永远不会好的网络。
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
    qr: PairingQrPayload,
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
    // 收口交给组合根：装配连接并触发一次连接（MUST NOT 停在等待扫码）。
    try {
      await wiring.onPaired(record);
    } catch (error) {
      // 记录已落盘、连接机已推进、页面已显示「已配对」——收口失败只意味着这条连接没起来，
      // 因此**不**回退配对结果（那会与主机面板刚显示的主机身份自相矛盾），只把可读原因补上；
      // 异常也不冒泡成未处理拒绝（这里的调用方是 `void store.submitPairingQrText(...)`）。
      // 原因走**收口专用**的映射，而不是 `claim` / `poll` 用的通用文案：这一页的阶段仍是
      // `approved`，引导语说着「配对已完成」，通用文案的「配对请求未能完成」会与它正面对撞。
      // 恢复路径仍在：连接机此刻是 `disconnected`，组合根的退避重连会再试一次。
      this.#commit({
        ...this.#state,
        pairing: applyPairingCloseoutFailure(this.#state.pairing, publicErrorForCloseoutFailure(error)),
      });
    }
  }

  /**
   * 结束这次配对：取消在途轮询并丢掉含秘密的载荷（§7.3）。
   *
   * 传输层**拒绝**（`claim` / `poll` 抛出）时选择「失败即结束」而不是「重排一次轮询」：
   * 端口用返回值区分业务失败，抛出只说明「这次请求没能完成」，仓库据此分不出「偶发断网」
   * 与「确定性拒绝」；重排还要求一份重试预算，且会把含 `pairingSecret` 的载荷无限期留在
   * 内存里。结束会话反而保证页面立刻拿到可读、可重试的 `failed`（输入框解禁、有重新扫码
   * 入口），而重试一次配对的成本只是一次粘贴。
   */
  #endPairing(): void {
    this.#pairingSession?.cancel?.();
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
   * 2. 权限（scopes）——R16 要求无权限时入口不可用；
   * 3. Agent 必须在本机已配置的目录里（快照 `agents`），不是「用户输入什么就发什么」；
   * 4. 载荷精确两键（`assertCreateSessionRefs`）。
   */
  submitCreateSession(refs: CreateSessionRefs): CreateOutcome {
    const connectionId = this.#identity.connectionId;
    if (connectionId === null) return { ok: false, reason: "尚未建立已认证连接，无法提交命令" };

    const entry = this.createEntry(refs.workspaceAlias);
    if (entry.permission.kind === "denied") return { ok: false, reason: entry.permission.reason };
    if (!entry.agents.some((agent) => agent.id === refs.agentId)) {
      return { ok: false, reason: "所选 Agent 不在本机已配置的 Agent 目录里" };
    }
    assertCreateSessionRefs({ workspaceAlias: refs.workspaceAlias, agentId: refs.agentId });

    const requestId = this.#ids.uuid();
    const commands = new Map(this.#state.commands);
    commands.set(requestId, createCommand({ requestId, command: "session.create" }));
    const createRequestIds = new Map(this.#state.createRequestIds);
    createRequestIds.set(refs.workspaceAlias, requestId);
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
          loadingHistory: intent === "earlier_history",
          pagingError: null,
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
