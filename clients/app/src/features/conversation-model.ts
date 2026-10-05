/**
 * 对话页模型（R17「未上报的上下文不得以零冒充」+ 会话明细现取与游标翻页）。
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
 * ## 翻页游标
 *
 * 续取游标取自上一页**最早**一条消息的 `(createdAt, messageId)`（`src/sync-client/paging.ts`
 * 的 `earliestCursor`）。本层不自己拼游标、不用 `messageId` 排序，也不在缺 `hasEarlier`
 * 时猜「是否还有更早内容」。
 */

import type { ContextUsageView, SessionReadPageView, SessionStateView } from "../domain";
import { buildContextUsageView } from "../domain";
import type { ReadCursor } from "../sync-client";
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

/** 翻页状态。 */
export interface ConversationPagingModel {
  /** 是否仍有更早内容（来自 wire 的 `hasEarlier`，不由客户端推断）。 */
  readonly hasEarlier: boolean;
  /** 当前是否有一次翻页请求在途。 */
  readonly loading: boolean;
  /** 下一页的续取游标；无更早内容或本页无消息时为 `null`。 */
  readonly nextBefore: ReadCursor | null;
  readonly error: string | null;
}

/** 会话可用能力。与上下文用量**无关**（R17 的第三条 MUST）。 */
export interface ConversationCapabilitiesModel {
  readonly canSend: boolean;
  readonly canCancel: boolean;
  readonly canViewHistory: boolean;
  readonly canLoadEarlier: boolean;
}

/** 对话页模型。 */
export interface ConversationPageModel {
  readonly header: ConversationHeaderModel;
  readonly usage: ContextUsageView;
  readonly messages: readonly ConversationMessageModel[];
  readonly degraded: readonly DegradedEventModel[];
  readonly paging: ConversationPagingModel;
  readonly capabilities: ConversationCapabilitiesModel;
}

/** 上一次翻页请求的游标；续取时原样带回（`(createdAt, messageId)` 复合游标，D2）。 */
export function nextBeforeCursor(
  page: SessionReadPageView,
  hasEarlier: boolean,
): ReadCursor | null {
  if (!hasEarlier) return null;
  return page.earliestCursor;
}

/**
 * 构造对话页模型。
 *
 * @param page `session.read` 最近一页（`src/domain` 的投影）；`null` 表示还没读过。
 * @param usage 最近一次 `session.usage.changed`；`null` 表示**从未**收到用量事件（R17 第一态）。
 * @param degraded 该会话的降级事件：与消息列表分开，未知事件因此不会让整个会话渲染失败。
 */
export function buildConversationModel(input: {
  readonly header: ConversationHeaderModel;
  readonly page: SessionReadPageView | null;
  readonly usage: UsageReading | null;
  readonly degraded: readonly DegradedEventModel[];
  readonly loadingEarlier: boolean;
  readonly pagingError: string | null;
  readonly capabilities: Omit<ConversationCapabilitiesModel, "canLoadEarlier">;
}): ConversationPageModel {
  const page = input.page;
  const hasEarlier = page?.hasEarlier ?? false;
  return {
    header: input.header,
    // 三态由 domain 层给出：`null` → unreported；`size` 为零 → zero_window；否则 known。
    usage: buildContextUsageView(input.usage),
    messages:
      page?.messages.map((message) => ({
        id: message.id,
        role: message.role,
        text: message.text,
        hasStructuredContent: message.hasStructuredContent,
        createdAt: message.createdAt,
      })) ?? [],
    degraded: input.degraded,
    paging: {
      hasEarlier,
      loading: input.loadingEarlier,
      nextBefore: page === null ? null : nextBeforeCursor(page, hasEarlier),
      error: input.pagingError,
    },
    // 「还能不能翻更早」由 wire 的 `hasEarlier` 决定，与用量状态无关（R17 第三条 MUST）。
    capabilities: { ...input.capabilities, canLoadEarlier: hasEarlier && !input.loadingEarlier },
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
