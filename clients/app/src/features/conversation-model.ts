/**
 * 对话页模型（R17「未上报的上下文不得以零冒充」+ R18 + 时间线/输入/交互/设置）。
 *
 * ## 三态上下文为什么不会退化成「零」
 *
 * `ContextUsageView`（`src/domain/conversation.ts`）是判别联合：`unreported` / `zero_window` /
 * `known`。本层**不**新增任何 `used = 0` 的默认值，也没有「按消息数推算」的分支：
 * `buildConversationModel` 收到 `usage` 就原样交给 `buildContextUsageView`，
 * 未收到用量事件时传 `null`。因此「未上报」不可能被显示成零占用。
 *
 * ## 未上报不得阻断其它交互
 *
 * `capabilities` 与 `usage` **互不相干**：翻历史、发消息、取消当前轮次是否可用只取决于
 * 连接状态、会话状态与来源在线与否。R17 的场景「未上报不影响其它交互」因此是结构性成立的，
 * 而不是靠某个组件记得别去禁用按钮。
 *
 * ## 一页 = 时间线 + 输入 + 交互 + 设置 + 改动清单
 *
 * 判定全部落在本层与 `src/domain/` 的纯函数里：组件拿到的是「已经决定好的」模型，
 * 页面与组件不做业务判定（`AGENTS.md` §6）。`timeline.entries` 是时间线的**唯一**渲染列表；
 * `degraded` 是它的降级条目扁平投影（既有诊断消费方的兼容面），两者不会各自新增内容。
 *
 * ## 结构化历史的可用性必须显式说明
 *
 * imported（来源为其它节点）的会话在本设备**没有**结构化历史（D11 的 Node Link 面缺口），
 * 服务端未协商 `core.session-history.v1` 时同样取不到。两种情形都给出明确说明，
 * MUST NOT 让用户以为「该会话没有结构化内容」。
 */

import type {
  AgentMessageContent,
  AttachmentMediaView,
  ContextUsageView,
  InteractionView,
  MessageChannelView,
  LocalActivityView,
  MessageView,
  PartialResultView,
  SessionReadPageView,
  SessionStateView,
} from "../domain";
import { buildContextUsageView, directoryNamesOf } from "../domain";
import type { WorkspaceRef } from "../protocol";
import type { ReadCursor, StagedResources } from "../sync-client";
import type { ConversationInputModel } from "./conversation-input-model";
import type { ConversationSettingsModel } from "./conversation-settings-model";
import type { FileChangeView, TimelineEntry } from "./conversation-timeline";
import type { DegradedEventModel } from "./degradation-model";
import type { SessionRowModel } from "./directory-model";
import { buildSessionRowsByDirectoryId, correctPageNumber } from "./directory-model";
import type { SessionActivationView } from "./session-lifecycle";

/** 一次用量事件的两项计数（`$defs.session.usage.changed`）。 */
export interface UsageReading {
  readonly used: string;
  readonly size: string;
}

/**
 * 对话页窄屏的**次级视图**（D5）：它决定哪一栏占主体，而三栏内容**始终在 DOM**，
 * 未选中的栏只是 `display: none`，因此窄屏与宽屏呈现的信息集合一致（规格「对话页的宽窄两种形态」）。
 *
 * 取值：`all` 不隐藏任何一栏；`timeline` / `sessions` / `files` 各只留下对应的一栏。它住在
 * store（跨路由存活），页面把它读出来交给组件；`hiddenConversationColumns` 是「哪个视图隐藏哪几栏」
 * 的**唯一**判据，组件不该另写一套。
 */
export type ConversationSecondaryView = "all" | "timeline" | "sessions" | "files";

/** 四个取值的登记表（穷尽 switch 与用例都从它出发，避免散落字符串字面量）。 */
export const CONVERSATION_SECONDARY_VIEWS: readonly ConversationSecondaryView[] = [
  "all",
  "timeline",
  "sessions",
  "files",
];

/** 缺省次级视图：进对话页先看对话本体（原型窄屏的默认形态）。 */
export const DEFAULT_CONVERSATION_SECONDARY_VIEW: ConversationSecondaryView = "timeline";

/** 对话页的三列（`data-conversation-column` 的三个取值）。 */
export type ConversationColumnName = "sessions" | "timeline" | "files";

/**
 * 次级视图 → 被隐藏的列。
 *
 * 三个取值各自**互补**：任一时刻最多一列成为「唯一可见的列」，而 `all` 一列都不隐藏——
 * 「四列内容都在 DOM」因此不是靠组件记得别删，而是这条映射的结果。
 */
export function hiddenConversationColumns(view: ConversationSecondaryView): readonly ConversationColumnName[] {
  switch (view) {
    case "all":
      return [];
    case "timeline":
      return ["sessions", "files"];
    case "sessions":
      return ["timeline", "files"];
    case "files":
      return ["sessions", "timeline"];
  }
}

/** 会话头（对话页顶部）。 */
export interface ConversationHeaderModel {
  readonly sessionId: string;
  readonly title: string;
  readonly agentName: string;
  /**
   * 主状态；`null` = 尚未取得权威摘要（不编造空闲）。
   *
   * 三态词表由 `domain/session-summary.ts` 给出；这里没有第四个取值，也没有旧七态。
   */
  readonly state: SessionStateView | null;
  /**
   * 本机活跃程度（协商后的 owned 会话）；`null` 时主标签回落任务三态。
   *
   * 与 `state` 并列而不是替换它：任务执行/排队/等待原因仍读任务事实。
   */
  readonly localActivity?: LocalActivityView | null | undefined;
  /**
   * 本机激活状态（仅协商后的 owned 会话有意义）；`null`/缺席 = 不适用（旧合同/imported）。
   *
   * 页面据此显示「自动激活不可用 / 正在恢复 / 失败原因 + 重试」，并在 `activating` 时
   * 保持历史可读、发送不可用。
   */
  readonly activation?: SessionActivationView | null | undefined;
  /** 会话所属目录的标识；只用于导航与键，MUST NOT 呈现为界面文本。 */
  readonly directoryId: string | null;
}

/** 一条消息的展示模型。结构化内容不混进正文——由时间线按媒体块呈现。 */
export interface ConversationMessageModel {
  readonly id: string;
  readonly role: "user" | "agent";
  readonly channel: MessageChannelView;
  readonly text: string;
  readonly content: AgentMessageContent;
  readonly partial: PartialResultView | null;
  /** 存在非文本内容块：正文之外还有媒体/资源块。 */
  readonly hasStructuredContent: boolean;
  readonly createdAt: string;
}

/**
 * 翻页状态：**两套独立分页**（D2）。
 *
 * 消息用 `before`/`hasEarlier`（`(createdAt, messageId)` 复合游标），结构化历史用
 * `beforeHistory`/`historyHasEarlier`（会话内事件序号）。任一方翻页都不改变另一方的页边界。
 */
export interface ConversationPagingModel {
  /** 是否仍有更早内容（来自 wire 的 `hasEarlier`，不由客户端推断）。 */
  readonly hasEarlier: boolean;
  /** 当前是否有一次翻页请求在途。 */
  readonly loading: boolean;
  /** 下一页的续取游标；无更早内容或本页无消息时为 `null`。 */
  readonly nextBefore: ReadCursor | null;
  readonly error: string | null;
  /** 结构化历史页是否还有更早条目（`historyHasEarlier`，只描述历史那一页）。 */
  readonly hasEarlierHistory: boolean;
  readonly loadingHistory: boolean;
  /** 历史条的续取游标（会话内事件序号）；无更早条目时为 `null`。 */
  readonly nextBeforeHistory: string | null;
}

/** 会话可用能力。与上下文用量**无关**（R17 的第三条 MUST）。 */
export interface ConversationCapabilitiesModel {
  readonly canSend: boolean;
  readonly canCancel: boolean;
  readonly canViewHistory: boolean;
  readonly canLoadEarlier: boolean;
}

/**
 * 时间线模型。`buildConversationModel` 缺省构造时为空（见下面的 `ABSENT_TIMELINE`）。
 */
export interface ConversationTimelineModel {
  /** 唯一渲染列表：消息、结构化卡片、降级卡片按时间排序。 */
  readonly entries: readonly TimelineEntry[];
  /** 结构化历史的可用性与说明（不可用时必须有说明）。 */
  readonly structuredHistory: {
    readonly state: "available" | "unavailable";
    readonly notice: string | null;
  };
}

/**
 * 直接构造页模型时的缺省（**只**影响「为了渲染某个子组件而手工拼一份页模型」的调用点）。
 *
 * 生产路径 `ClientStore.conversationPage()` 一律显式传入全部字段，因此这些缺省不会出现在
 * 真实运行时；它们的措辞刻意写成「未提供」，而不是冒充某种真实状态。
 */
const ABSENT_TIMELINE: ConversationTimelineModel = {
  entries: [],
  structuredHistory: { state: "unavailable", notice: "本页模型未提供结构化历史信息。" },
};

const ABSENT_INPUT: ConversationInputModel = {
  // 手拼模型没有真实输入区：三态里只能是「不可用」，原因槽位由 `disabledReason` 的
  // 「未提供」措辞说明；`blockedBy` 只是该占位模型的机器槽位，生产路径不会走到这里。
  state: "unavailable",
  blockedBy: "source_offline",
  draft: "",
  canSubmit: false,
  disabledReason: "本页模型未提供输入区数据",
  attachmentsSupported: false,
  attachmentNotice: "本页模型未提供附件能力信息。",
  attachments: {
    attachments: [],
    phase: "empty",
    canSubmit: true,
    blockedReason: null,
    capabilityNotice: null,
    uploadedBytes: "0",
    totalBytes: "0",
    error: null,
  },
  pathRefs: [],
  draftInMemoryOnly: false,
  draftNotice: null,
  submission: { kind: "none" },
  cancel: { kind: "unavailable" },
};

const ABSENT_SETTINGS: ConversationSettingsModel = {
  model: { state: "unavailable", currentId: null, currentName: null, choices: [], notice: "本页模型未提供模型可选项。", configId: null },
  mode: { state: "unavailable", currentId: null, currentName: null, choices: [], notice: "本页模型未提供模式可选项。", configId: null },
};

/**
 * 手工拼页模型时的左栏缺省（**只**影响「为了渲染某个子组件而拼一份页模型」的调用点）。
 *
 * `notice` 的措辞刻意是「未提供」：它不能被读成「这个会话没有目录归属」——那是真实数据的结论，
 * 由 {@link buildConversationSidebar} 给出。
 */
const ABSENT_SIDEBAR: ConversationSidebarModel = {
  directory: null,
  notice: "本页模型未提供左栏数据。",
  query: "",
  total: 0,
  shown: 0,
  page: 1,
  pageCount: 1,
  canNextPage: false,
  selectedSessionId: "",
  rows: [],
};

/** 对话页左栏的目录上下文（由路径派生的展示名 + 逐字路径；`directoryId` 是进入详情页的入口）。 */
export interface ConversationSidebarDirectoryModel {
  readonly directoryId: string;
  readonly displayName: string;
  /** 服务端下发的逐字路径。 */
  readonly path: string;
}

/**
 * 对话页左栏：同目录会话列表与它的目录上下文。
 *
 * 「没有目录归属」与「有目录但暂时一条会话都没有」是两件事：前者 `directory` 为 `null` 且
 * `notice` 必为文案（规格场景「没有目录归属的会话」），后者 `directory` 在场、`rows` 为空。
 */
export interface ConversationSidebarModel {
  /** 目录上下文；会话没有目录归属时为 `null`。 */
  readonly directory: ConversationSidebarDirectoryModel | null;
  /** 没有目录归属时的说明；`directory` 在场时为 `null`（两者互斥，中栏不受影响）。 */
  readonly notice: string | null;
  /** 目录内搜索词（受控输入的回填值）；只影响本栏的 `rows`/`total`。 */
  readonly query: string;
  /** 搜索命中后的同目录会话总数。 */
  readonly total: number;
  /** 本页会话数（`rows.length`）。 */
  readonly shown: number;
  readonly page: number;
  readonly pageCount: number;
  /** 是否还有下一页。 */
  readonly canNextPage: boolean;
  /** 当前会话在列表里的选中标记（行本身不带 `selected` 字段，组件按此值比对）。 */
  readonly selectedSessionId: string;
  /** 同目录会话行：时间倒序、按搜索过滤、按页截取。 */
  readonly rows: readonly SessionRowModel[];
}

/** 左栏每页的会话条数。 */
export const CONVERSATION_SIDEBAR_PAGE_SIZE = 15;

/** 会话没有目录归属时左栏的说明（规格：说明它没有目录归属，而不是静默隐藏左栏）。 */
export const CONVERSATION_NO_DIRECTORY_NOTICE =
  "这个会话没有目录归属：它没有可用的目录引用，因此这里列不出同目录会话。";

/** 会话引用的目录不在最近一次同步的目录列表里（无从派生展示名）时的说明。 */
export const CONVERSATION_UNKNOWN_DIRECTORY_NOTICE =
  "这个会话所属的目录不在最近一次同步的目录列表里，因此这里列不出目录上下文与同目录会话。";

/** 对话页模型。 */
export interface ConversationPageModel {
  readonly header: ConversationHeaderModel;
  readonly usage: ContextUsageView;
  /** 消息（兼容面）：与时间线里的消息条目同一批数据。 */
  readonly messages: readonly ConversationMessageModel[];
  /** 降级条目（兼容面）：时间线上 `kind === "degraded"` 的扁平投影。 */
  readonly degraded: readonly DegradedEventModel[];
  readonly timeline: ConversationTimelineModel;
  readonly paging: ConversationPagingModel;
  readonly capabilities: ConversationCapabilitiesModel;
  readonly input: ConversationInputModel;
  readonly interactions: readonly InteractionView[];
  /** 已读取的附件媒体（`attachmentId → 状态`）：组件按它渲染图片/音频/下载。 */
  readonly media: ReadonlyMap<string, AttachmentMediaView>;
  readonly settings: ConversationSettingsModel;
  readonly fileChanges: readonly FileChangeView[];
  /** 左栏：目录上下文与同目录会话列表（宽屏常驻、窄屏为次级视图之一）。 */
  readonly sidebar: ConversationSidebarModel;
}

/**
 * 左栏模型（对话页）：目录上下文 + 同目录会话列表 + 已呈现数与总数。
 *
 * 数据只从**快照资源**来（与目录页共用 `buildSessionRowsByDirectoryId` 这一条行投影），因此本函数
 * 没有命令出口——「只读快照、不派发新命令」在结构上成立。目录上下文里的展示名由服务端下发的路径
 * 派生（与目录页同一份口径），路径逐字透传，不拼接、不反查。
 */
export function buildConversationSidebar(input: {
  readonly resources: StagedResources | null;
  /** 当前会话自己的目录引用（`SessionSummary.workspace`）；`null` = 没有目录归属。 */
  readonly workspace: WorkspaceRef | null;
  readonly sessionId: string;
  readonly ownerLabelOf: (ownerNodeId: string) => string;
  /** 左栏目录内搜索词；空串即不过滤。 */
  readonly query: string;
  /** 请求的页码；越界或无效输入由模型纠正。 */
  readonly pageInput: string;
  /** 相对时间的基准（D3：时钟是**输入**）。 */
  readonly nowMs: number;
  /** 连接是否协商了本机生命周期（决定同目录会话行的主状态词表）。 */
  readonly localLifecycleNegotiated?: boolean;
}): ConversationSidebarModel {
  const workspace = input.workspace;
  if (workspace === null) {
    return {
      directory: null,
      notice: CONVERSATION_NO_DIRECTORY_NOTICE,
      query: input.query,
      total: 0,
      shown: 0,
      page: 1,
      pageCount: 1,
      canNextPage: false,
      selectedSessionId: input.sessionId,
      rows: [],
    };
  }

  // 展示名与路径都来自快照目录条目：展示名由同一份快照的 `path` 派生（与目录页一致）。
  // 目录未出现在最近一次快照里时无从派生展示名，MUST NOT 退回目录标识或编一段文本。
  const workspaces = input.resources?.workspaces ?? [];
  const registered = workspaces.find((item) => item.directoryId === workspace.directoryId);
  if (registered === undefined) {
    return {
      directory: null,
      notice: CONVERSATION_UNKNOWN_DIRECTORY_NOTICE,
      query: input.query,
      total: 0,
      shown: 0,
      page: 1,
      pageCount: 1,
      canNextPage: false,
      selectedSessionId: input.sessionId,
      rows: [],
    };
  }
  const derived = directoryNamesOf(workspaces).get(registered.directoryId);
  const directory: ConversationSidebarDirectoryModel = {
    directoryId: registered.directoryId,
    displayName: derived?.displayName ?? registered.path,
    path: registered.path,
  };

  const needle = input.query.trim().toLowerCase();
  const rowsOfDirectory =
    buildSessionRowsByDirectoryId({
      resources: input.resources,
      ownerLabelOf: input.ownerLabelOf,
      nowMs: input.nowMs,
      ...(input.localLifecycleNegotiated === undefined
        ? {}
        : { localLifecycleNegotiated: input.localLifecycleNegotiated }),
    }).get(directory.directoryId) ?? [];
  const matched = rowsOfDirectory.filter((row) => needle.length === 0 || row.title.toLowerCase().includes(needle));
  const pageCount = Math.max(1, Math.ceil(matched.length / CONVERSATION_SIDEBAR_PAGE_SIZE));
  const page = correctPageNumber({ requested: input.pageInput, pageCount, fallback: 1 });
  const offset = (page - 1) * CONVERSATION_SIDEBAR_PAGE_SIZE;
  const rows = matched.slice(offset, offset + CONVERSATION_SIDEBAR_PAGE_SIZE);

  return {
    directory,
    notice: null,
    query: input.query,
    total: matched.length,
    shown: rows.length,
    page,
    pageCount,
    canNextPage: page < pageCount,
    selectedSessionId: input.sessionId,
    rows,
  };
}

/**
 * 结构化历史的可用性判定（纯函数）。
 *
 * 三种不可用各自有**互不相同**的说明：来源是别的节点、来源节点当前不可达、服务端没协商这个
 * 能力。第三种是本地会话也会遇到的（旧服务端），因此不能并进前两种。
 */
export function structuredHistoryAvailability(input: {
  readonly origin: "local" | { readonly kind: "remote"; readonly online: boolean };
  readonly historyFeatureNegotiated: boolean;
}): ConversationTimelineModel["structuredHistory"] {
  if (input.origin !== "local") {
    if (!input.historyFeatureNegotiated) {
      return {
        state: "unavailable",
        notice: "服务端未提供结构化历史（未协商 core.session-history.v1），当前只呈现消息正文。",
      };
    }
    if (!input.origin.online) {
      return {
        state: "unavailable",
        notice: "来源节点当前不可达，无法获取结构化历史；这不代表该会话没有结构化内容。",
      };
    }
    return { state: "available", notice: null };
  }
  if (!input.historyFeatureNegotiated) {
    return {
      state: "unavailable",
      notice: "服务端未提供结构化历史（未协商 core.session-history.v1），当前只呈现消息正文。",
    };
  }
  return { state: "available", notice: null };
}

/** 上一次翻页请求的游标；续取时原样带回（`(createdAt, messageId)` 复合游标，D2）。 */
export function nextBeforeCursor(page: SessionReadPageView, hasEarlier: boolean): ReadCursor | null {
  if (!hasEarlier) return null;
  return page.earliestCursor;
}

/** 构造对话页模型。 */
export function buildConversationModel(input: {
  readonly header: ConversationHeaderModel;
  readonly page: SessionReadPageView | null;
  readonly usage: UsageReading | null;
  readonly degraded: readonly DegradedEventModel[];
  readonly timeline?: ConversationTimelineModel;
  readonly loadingEarlier: boolean;
  readonly pagingError: string | null;
  readonly historyHasEarlier?: boolean;
  readonly loadingHistory?: boolean;
  readonly nextBeforeHistory?: string | null;
  readonly input?: ConversationInputModel;
  readonly interactions?: readonly InteractionView[];
  readonly media?: ReadonlyMap<string, AttachmentMediaView>;
  readonly settings?: ConversationSettingsModel;
  readonly fileChanges?: readonly FileChangeView[];
  /** 左栏模型（目录上下文 + 同目录会话列表）；缺省时用「未提供」的缺省支。 */
  readonly sidebar?: ConversationSidebarModel;
  readonly capabilities: Omit<ConversationCapabilitiesModel, "canLoadEarlier">;
}): ConversationPageModel {
  const page = input.page;
  const hasEarlier = page?.hasEarlier ?? false;
  return {
    header: input.header,
    // 三态由 domain 层给出：`null` → unreported；`size` 为零 → zero_window；否则 known。
    usage: buildContextUsageView(input.usage),
    messages:
      page?.messages.map((message: MessageView) => ({
        id: message.id,
        role: message.role,
        channel: message.channel,
        text: message.text,
        content: message.content,
        partial: message.partial,
        hasStructuredContent: message.hasStructuredContent,
        createdAt: message.createdAt,
      })) ?? [],
    degraded: input.degraded,
    timeline: input.timeline ?? ABSENT_TIMELINE,
    paging: {
      hasEarlier,
      loading: input.loadingEarlier,
      nextBefore: page === null ? null : nextBeforeCursor(page, hasEarlier),
      error: input.pagingError,
      hasEarlierHistory: input.historyHasEarlier ?? false,
      loadingHistory: input.loadingHistory ?? false,
      nextBeforeHistory: input.nextBeforeHistory ?? null,
    },
    // 「还能不能翻更早」由 wire 的 `hasEarlier` 决定，与用量状态无关（R17 第三条 MUST）。
    capabilities: { ...input.capabilities, canLoadEarlier: hasEarlier && input.loadingEarlier === false },
    input: input.input ?? ABSENT_INPUT,
    interactions: input.interactions ?? [],
    media: input.media ?? new Map<string, AttachmentMediaView>(),
    settings: input.settings ?? ABSENT_SETTINGS,
    fileChanges: input.fileChanges ?? [],
    sidebar: input.sidebar ?? ABSENT_SIDEBAR,
  };
}

/**
 * 会话可用能力（与用量无关）。
 *
 * 没有「会话已关闭」这一档：三态都不构成永久禁提交的理由。发送看连接与来源是否可达
 * （远程来源离线时不得把输入标记为已发送或排队），取消看是否存在未结束的轮次且连接可用——
 * 两者都不从「会话是否关闭」推导，也不由会话主状态回推。
 */
export function conversationCapabilities(input: {
  readonly connectionOnline: boolean;
  /** 时间线上是否有未结束的轮次（与输入模型的 `canCancel` 同源）。 */
  readonly turnInProgress: boolean;
  readonly origin: "local" | { readonly kind: "remote"; readonly online: boolean };
  /**
   * 本机会话是否已确认可用。缺省 `true`（旧合同/imported）；协商后只有 `ready` 才是 `true`。
   *
   * 活跃标签不参与：资源未恢复时任务/交互仍可能让本机显示 `active`。
   */
  readonly sessionReady?: boolean;
}): ConversationCapabilitiesModel {
  return {
    // 历史不受连接状态限制：未上报、断连都不是禁用理由。
    canViewHistory: true,
    canCancel: input.connectionOnline && input.turnInProgress,
    canSend:
      input.connectionOnline &&
      (input.origin === "local" || input.origin.online) &&
      (input.sessionReady ?? true),
    canLoadEarlier: false,
  };
}
