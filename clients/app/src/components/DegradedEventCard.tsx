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
 * 本组件因此把三类原因做成判别联合——调用方不能用一个笼统的字符串绕过区分。
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
  /** 三类不支持原因之一；已识别但因能力不可用时给出。 */
  readonly unsupportedBy: UnsupportedBy;
  /** 原文可用性；`not_synced` 时必须渲染原因而不是占位。 */
  readonly raw: RawAvailability;
}

/** 渲染一张降级卡片。 */
export function DegradedEventCard({
  eventType,
  unsupportedBy,
  raw,
}: DegradedEventCardProps): ReactElement {
  return (
    <div data-degraded-event={eventType} data-unsupported-by={unsupportedBy}>
      <strong>{eventType}</strong>
      <span data-degraded-view="no-dedicated-view">当前客户端未提供专用视图</span>
      <span data-unsupported-reason={unsupportedBy} />
      {raw.kind === "not_synced" ? (
        <span data-raw-unavailable={raw.reason}>原文未同步（{raw.reason}）</span>
      ) : null}
    </div>
  );
}
