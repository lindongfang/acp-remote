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
  ContextUsageView,
  InteractionView,
  MessageView,
  SessionReadPageView,
  SessionStateView,
} from "../domain";
import { buildContextUsageView } from "../domain";
import type { ReadCursor } from "../sync-client";
import type { ConversationInputModel } from "./conversation-input-model";
import type { ConversationSettingsModel } from "./conversation-settings-model";
import type { FileChangeView, TimelineEntry } from "./conversation-timeline";
import type { DegradedEventModel } from "./degradation-model";

/** 一次用量事件的两项计数（`$defs.session.usage.changed`）。 */
export interface UsageReading {
  readonly used: string;
  readonly size: string;
}

/** 会话头（对话页顶部）。 */
export interface ConversationHeaderModel {
  readonly sessionId: string;
  readonly title: string;
  readonly agentName: string;
  readonly state: SessionStateView;
  readonly directoryAlias: string | null;
  /** imported 会话来源离线时的标注；本节点自有会话为 `null`。 */
  readonly originOfflineLabel: string | null;
}

/** 一条消息的展示模型。结构化内容不进正文——由降级卡片承载（R18）。 */
export interface ConversationMessageModel {
  readonly id: string;
  readonly role: "user" | "agent";
  readonly text: string;
  /** 存在非文本内容块：正文只渲染文本，其余由降级卡片说明。 */
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
  state: "editable",
  draft: "",
  canSubmit: false,
  disabledReason: "本页模型未提供输入区数据",
  attachmentsSupported: false,
  attachmentNotice: "v1 只支持文本输入：附件（图片/文件）不受支持，请改用文本描述。",
  draftInMemoryOnly: false,
  draftNotice: null,
  submission: { kind: "none" },
  cancel: { kind: "unavailable" },
};

const ABSENT_SETTINGS: ConversationSettingsModel = {
  model: { state: "unavailable", currentId: null, currentName: null, choices: [], notice: "本页模型未提供模型可选项。" },
  mode: { state: "unavailable", currentId: null, currentName: null, choices: [], notice: "本页模型未提供模式可选项。" },
};

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
  readonly settings: ConversationSettingsModel;
  readonly fileChanges: readonly FileChangeView[];
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
    return {
      state: "unavailable",
      notice: input.origin.online
        ? "这是来源为其它节点的会话：结构化历史在本设备不可用（本机只承载消息正文）。"
        : "来源节点当前不可达，无法获取结构化历史；这不代表该会话没有结构化内容。",
    };
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
  readonly settings?: ConversationSettingsModel;
  readonly fileChanges?: readonly FileChangeView[];
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
        text: message.text,
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
    settings: input.settings ?? ABSENT_SETTINGS,
    fileChanges: input.fileChanges ?? [],
  };
}

/**
 * 会话可用能力（与用量无关）。
 *
 * R20：来源离线的 imported 会话不得把输入标记为已发送或已排队，因此 `canSend` 为假。
 */
export function conversationCapabilities(input: {
  readonly connectionOnline: boolean;
  readonly sessionState: SessionStateView;
  readonly origin: "local" | { readonly kind: "remote"; readonly online: boolean };
}): ConversationCapabilitiesModel {
  const open = input.sessionState !== "closed";
  const sendable = input.connectionOnline && open && input.origin === "local";
  return {
    // 历史与取消不受连接状态限制：未上报、断连都不是禁用理由。
    canViewHistory: open,
    canCancel: open && input.sessionState === "running",
    canSend: sendable,
    canLoadEarlier: false,
  };
}
