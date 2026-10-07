/**
 * 未识别/不可用结构化内容的降级卡片（共享展示原语，WP5a 只建骨架，WP7 接管实现）。
 *
 * `pwa-web-client` 的「未识别与不可用的结构化内容显式降级」要求：
 *
 * - 没有专用视图的结构化事件呈现明确的降级卡片，并保留原文与元数据供诊断；
 * - **不**静默隐藏该事件，**不**把结构化事件降级成普通文本；
 * - 原文未下发时明确说明原因，**不**渲染占位佯装完整；
 * - 能区分「客户端不支持 / 服务端不支持 / Agent 不支持」三类原因。
 *
 * `unsupportedBy` 与 `noDedicatedView` 是**两个**独立信号而不是一个：前者是三类能力不支持
 * 之一（没有它就没有归因），后者只回答「客户端确实没有这个视图」。原文未下发与二进制未下发
 * 都属于后者为假、前者为 `null` 的情形——客户端有视图，缺的是内容。
 */
import type { ReactElement } from "react";


/** 不支持的三种原因（`capability.unsupported_by_*` 的前端对应）。 */
export type UnsupportedBy = "client" | "broker" | "agent";

/** 原文可用性：可用 / 因大小上限或保留期未下发 / 二进制内容未下发。 */
export type RawAvailability =
  | { readonly kind: "available"; readonly rawJson: string }
  | { readonly kind: "not_synced"; readonly reason: "size_limit" | "retention_expired" | "storage_failure" }
  | { readonly kind: "binary_not_fetched"; readonly mimeType: string };

export interface DegradedEventCardProps {
  /** eventType（原样保留，未识别的事件也照实显示）。 */
  readonly eventType: string;
  /**
   * 三类不支持原因之一；`null` 表示**本次降级与「客户端能力不支持」无关**。
   *
   * 「原文未下发」与「二进制内容未下发」都必须传 `null`：客户端**有**对应视图，缺的是
   * 内容。把它兜底成某个原因会让同一张卡片同时写着「客户端不支持」与「内容没下发」，
   * 用户只能按错误原因排查（R18 要求三类原因可区分）。
   */
  readonly unsupportedBy: UnsupportedBy | null;
  /**
   * 客户端确实没有该事件的专用视图（事件类型未登记，或能力不受支持）。
   *
   * 与 `unsupportedBy` 分开而不是由它推导：前两个原因（未登记、能力不支持）都要呈现
   * 「当前客户端未提供专用视图」，而内容未下发的原因**不**能这样宣称。
   */
  readonly noDedicatedView: boolean;
  /** 原文可用性；`not_synced` 时必须渲染原因而不是占位。 */
  readonly raw: RawAvailability;
}

/** 渲染一张降级卡片。 */
export function DegradedEventCard({
  eventType,
  unsupportedBy,
  noDedicatedView,
  raw,
}: DegradedEventCardProps): ReactElement {
  return (
    <div data-degraded-event={eventType} data-unsupported-by={unsupportedBy ?? undefined}>
      <strong>{eventType}</strong>
      {noDedicatedView ? (
        <span data-degraded-view="no-dedicated-view">当前客户端未提供专用视图</span>
      ) : null}
      {unsupportedBy === null ? null : <span data-unsupported-reason={unsupportedBy} />}
      {raw.kind === "not_synced" ? (
        <span data-raw-unavailable={raw.reason}>原文未同步（{raw.reason}）</span>
      ) : null}
    </div>
  );
}
