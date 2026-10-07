/**
 * 会话正文与上下文用量的视图模型。
 *
 * 两个要点：
 *
 * - **明细一律现取**（`design.md` D8）：`session.read` 的分页结果直接投影成消息视图，
 *   不存在「快照里的明细」与「读回来的明细」的合并问题。
 * - **上下文用量三态**（R17）：已上报 / 未上报 / 零窗口——都**不**以零冒充已用量，
 *   也**不**由客户端推算。
 */

import type { DecimalString, Uuid } from "../protocol/common";
import type { SessionReadResult } from "../protocol/command";

/** 消息的稳定标识。 */
export type MessageId = Uuid;

/** 消息角色。 */
export type MessageRole = "user" | "agent";

/**
 * 一条消息的视图模型。
 *
 * `content` 只保留渲染所需的文本与明确的「未提供专用视图」标记；结构化 ACP 内容的
 * 完整原文与元数据由降级卡片消费（WP7），本层不把它塞进消息正文冒充文本。
 */
export interface MessageView {
  readonly id: MessageId;
  readonly role: MessageRole;
  /** 纯文本段落；结构化块（图片/资源）不在此列，由降级卡片承载。 */
  readonly text: string;
  /** 存在非文本内容块（图片引用/资源引用/不支持的块）。 */
  readonly hasStructuredContent: boolean;
  readonly createdAt: string;
  readonly turnId: Uuid | null;
}

/**
 * `session.read` 一页的视图模型。
 *
 * `hasEarlier` 直接来自 wire（`sessionReadResult.hasEarlier`），
 * 客户端据此用最早一条的 `(createdAt, messageId)` 构造 `before` 翻页。
 */
export interface SessionReadPageView {
  readonly sessionId: Uuid;
  readonly messages: readonly MessageView[];
  readonly hasEarlier: boolean;
  /** 本页最早一条的复合游标；无消息时为 `null`。 */
  readonly earliestCursor: { readonly createdAt: string; readonly messageId: MessageId } | null;
}

/**
 * 上下文用量视图：三态判别联合。
 *
 * `unreported` 覆盖「尚未收到任何用量事件」；`zero_window` 覆盖「窗口大小为零」；
 * `known` 才给出占比。**没有**「零占用」这一态——未上报不等于零（R17）。
 */
export type ContextUsageView =
  | { readonly kind: "unreported" }
  | { readonly kind: "zero_window" }
  | {
      readonly kind: "known";
      readonly used: DecimalString;
      readonly size: DecimalString;
      /** 已用占上限的百分比（0–100，一位小数）。 */
      readonly percent: number;
    };

/** 三态的展示文案（原型与 `pwa-web-client` 的「未上报」口径）。 */
export function contextUsageLabel(usage: ContextUsageView): string {
  switch (usage.kind) {
    case "unreported":
      return "未上报";
    case "zero_window":
      return "未上报（窗口为零）";
    case "known":
      return `${usage.percent.toFixed(1)}%`;
  }
}

/**
 * 从 `session.usage.changed` 视图构造上下文用量视图。
 *
 * 参数是三态输入：`null` 表示尚未收到任何用量事件。`size` 为零时同样返回未上报，
 * **不**计算无意义的占比。
 */
export function buildContextUsageView(
  usage: { readonly used: DecimalString; readonly size: DecimalString } | null,
): ContextUsageView {
  if (usage === null) return { kind: "unreported" };
  const size = Number(usage.size);
  if (size === 0) return { kind: "zero_window" };
  const used = Number(usage.used);
  return {
    kind: "known",
    used: usage.used,
    size: usage.size,
    percent: (used / size) * 100,
  };
}

/**
 * 把 `session.read` 的结果投影成视图模型。
 *
 * 只消费 `resources.messages`（v1 对话页所需的唯一明细资源）；`hasEarlier` 与最早游标
 * 由本函数算好，供 WP6 的翻页逻辑直接使用。
 */
export function buildSessionReadPageView(result: SessionReadResult): SessionReadPageView {
  const messages = result.resources.messages ?? [];
  const views: MessageView[] = messages.map((message) => {
    const textParts: string[] = [];
    let hasStructuredContent = false;
    for (const block of message.content) {
      if (block.type === "text") {
        textParts.push(block.text);
      } else {
        hasStructuredContent = true;
      }
    }
    return {
      id: message.messageId,
      role: message.role,
      text: textParts.join("\n"),
      hasStructuredContent,
      createdAt: message.createdAt,
      turnId: message.turnId,
    };
  });

  return {
    sessionId: result.sessionId,
    messages: views,
    hasEarlier: result.hasEarlier,
    earliestCursor:
      messages.length === 0
        ? null
        : {
            createdAt: messages[0]!.createdAt,
            messageId: messages[0]!.messageId,
          },
  };
}
