/**
 * 会话正文流（R17 上下文条 + R18 降级卡片 + 向上翻页）。
 *
 * ## 为什么降级卡片与消息列表是两块而不是一块
 *
 * R18 的场景「未知事件 MUST NOT 导致整个会话渲染失败」要求两者互不牵连：
 * 消息列表渲染失败不应该带走降级卡片。因此降级事件单独成段并带自己的选择器，
 * 页面也据此分别重渲染。
 *
 * 结构化内容不进消息正文（`MessageView` 只有纯文本），有非文本块时正文旁标一个
 * 「含结构化内容」的标记，真正的说明由降级卡片给出。
 */

import type { ReactElement } from "react";

import type { ConversationPageModel } from "../features/conversation-model";
import type { DegradedEventModel } from "../features/degradation-model";
import { RAW_NOT_SYNCED_LABELS } from "../features/degradation-model";
import { ContextUsageBar } from "./ContextUsageBar";
import { DegradedEventCard } from "./DegradedEventCard";
import { StatusPill } from "./StatusPill";

export interface ConversationStreamProps {
  readonly model: ConversationPageModel;
  /** 向上翻页（续取 `(createdAt, messageId)` 复合游标）。 */
  readonly onLoadEarlier: () => void;
  /** 打开某条降级事件的诊断抽屉。 */
  readonly onOpenDiagnostics: (eventId: string) => void;
}

/** 一条降级卡片 + 它的查看入口（R18 MUST 提供原文与元数据的入口）。 */
function DegradedEntry({
  model,
  onOpenDiagnostics,
}: {
  readonly model: DegradedEventModel;
  readonly onOpenDiagnostics: (eventId: string) => void;
}): ReactElement {
  // 非 `unsupported` 原因（原文未下发 / 二进制未下发 / 未登记事件）**不是**「客户端不支持」：
  // 兜底成 `"client"` 会把「内容没下发」误报成「客户端能力不支持」，让用户按错误原因排查。
  const unsupportedBy = model.reason.kind === "unsupported" ? model.reason.by : null;
  // 「客户端没有专用视图」只对前两类降级成立：原文未下发 / 二进制未下发时客户端**有**视图，
  // 缺的是内容，宣称「未提供专用视图」同样是错误归因。
  const noDedicatedView =
    model.reason.kind === "unsupported" || model.reason.kind === "no_dedicated_view";
  const raw =
    model.raw.kind === "available"
      ? ({ kind: "available", rawJson: model.raw.rawJson } as const)
      : model.raw.kind === "not_synced"
        ? ({ kind: "not_synced", reason: model.raw.reason } as const)
        : ({ kind: "binary_not_fetched", mimeType: describeBinary(model) } as const);

  return (
    <li data-degraded-entry={model.eventId}>
      <DegradedEventCard
        eventType={model.eventType}
        unsupportedBy={unsupportedBy}
        noDedicatedView={noDedicatedView}
        raw={raw}
      />
      {model.reason.kind === "raw_not_synced" ? (
        <p data-raw-not-synced={model.reason.reason}>{RAW_NOT_SYNCED_LABELS[model.reason.reason]}</p>
      ) : null}
      {model.reason.kind === "unsupported" ? <p data-unsupported-detail={model.reason.by}>{model.reason.detail}</p> : null}
      {model.reason.kind === "binary_not_delivered" ? (
        <p data-binary-not-delivered={model.reason.mimeType ?? "unknown"}>
          结构化事件已收到，但它引用的二进制内容没有下发到本设备。
        </p>
      ) : null}
      <button
        type="button"
        data-action="open-diagnostics"
        data-event-id={model.eventId}
        onClick={() => {
          onOpenDiagnostics(model.eventId);
        }}
      >
        查看原文与元数据
      </button>
    </li>
  );
}

/** 二进制引用的一句描述；未给出 mimeType 时不编造。 */
function describeBinary(model: DegradedEventModel): string {
  if (model.reason.kind !== "binary_not_delivered") return "unknown";
  return model.reason.mimeType ?? model.reason.uri ?? "unknown";
}

/** 会话正文流。 */
export function ConversationStream({ model, onLoadEarlier, onOpenDiagnostics }: ConversationStreamProps): ReactElement {
  return (
    <section data-route="session" data-session-id={model.header.sessionId}>
      <header data-session-header={model.header.sessionId}>
        <h1>{model.header.title}</h1>
        <span data-session-agent={model.header.agentName}>{model.header.agentName}</span>
        <StatusPill state={model.header.state} />
        {model.header.originOfflineLabel === null ? null : (
          <p data-origin-offline="true">{model.header.originOfflineLabel}</p>
        )}
      </header>

      {/* 上下文条：三种状态各自独立呈现，未上报不是零。 */}
      <ContextUsageBar usage={model.usage} />

      <div data-paging="earlier">
        {model.paging.hasEarlier ? (
          <button
            type="button"
            data-action="load-earlier"
            disabled={!model.capabilities.canLoadEarlier}
            onClick={onLoadEarlier}
          >
            {model.paging.loading ? "正在加载更早的消息……" : "加载更早的消息"}
          </button>
        ) : (
          <p data-paging="earlier-exhausted">已经是最早的消息。</p>
        )}
        {model.paging.error === null ? null : <p data-paging-error="true">{model.paging.error}</p>}
      </div>

      <ul data-message-list="messages">
        {model.messages.map((message) => (
          <li key={message.id} data-message-id={message.id} data-message-role={message.role}>
            <p data-message-text="true">{message.text}</p>
            {message.hasStructuredContent ? <span data-has-structured-content="true">含结构化内容</span> : null}
          </li>
        ))}
      </ul>

      <ul data-degraded-list="events">
        {model.degraded.map((item) => (
          <DegradedEntry key={item.eventId} model={item} onOpenDiagnostics={onOpenDiagnostics} />
        ))}
      </ul>

      <footer data-capabilities="true">
        <span data-can-send={String(model.capabilities.canSend)} />
        <span data-can-cancel={String(model.capabilities.canCancel)} />
        <span data-can-view-history={String(model.capabilities.canViewHistory)} />
        {model.capabilities.canSend ? null : <p data-send-disabled-reason="true">当前不能发送消息（原因见上方标注）。</p>}
      </footer>
    </section>
  );
}

