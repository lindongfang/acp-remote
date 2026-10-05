/**
 * 诊断抽屉（R18：保留原文与事件元数据供诊断查看）。
 *
 * ## 原文未同步时这里**没有**原文分支
 *
 * `RawAvailability` 是判别联合：`not_synced` 分支在本组件里没有可渲染的正文槽位，
 * 因此「渲染一段占位 JSON 假装完整」在这里没有表达方式。取而代之的是一段明确的说明，
 * 外加字节长度与摘要（有的话）——它们是**元数据**，不是原文。
 *
 * 「原文与元数据」的元数据部分（事件类型、事件 ID、全局序号、来源、时间）无论原文是否
 * 可用都完整呈现：诊断需要它们。
 */

import type { ReactElement } from "react";

import type { DegradedEventModel } from "../features/degradation-model";
import { RAW_NOT_SYNCED_LABELS, UNSUPPORTED_LABELS } from "../features/degradation-model";

export interface DiagnosticsDrawerProps {
  /** 选中的降级事件；`null` 表示抽屉未打开。 */
  readonly model: DegradedEventModel | null;
  readonly onClose: () => void;
}

/** 元数据行（原文可用与否都呈现）。 */
function Metadata({ model }: { readonly model: DegradedEventModel }): ReactElement {
  return (
    <dl data-diagnostics-metadata="true">
      <dt>事件类型</dt>
      <dd data-metadata="event-type">{model.eventType}</dd>
      <dt>事件 ID</dt>
      <dd data-metadata="event-id">{model.eventId}</dd>
      <dt>全局序号</dt>
      <dd data-metadata="global-sequence">{model.globalSequence}</dd>
      <dt>来源</dt>
      <dd data-metadata="origin">{model.originKind}</dd>
      <dt>时间</dt>
      <dd data-metadata="created-at">{model.createdAt}</dd>
    </dl>
  );
}

/** 降级原因的一句话说明。 */
function ReasonNote({ model }: { readonly model: DegradedEventModel }): ReactElement {
  const reason = model.reason;
  if (reason.kind === "no_dedicated_view") {
    return <p data-degraded-reason="no_dedicated_view">当前客户端没有为该事件提供专用视图，以下是它的原始内容。</p>;
  }
  if (reason.kind === "unsupported") {
    return (
      <p data-degraded-reason="unsupported" data-unsupported-by={reason.by}>
        {UNSUPPORTED_LABELS[reason.by]}：{reason.detail}
      </p>
    );
  }
  if (reason.kind === "raw_not_synced") {
    return <p data-degraded-reason="raw_not_synced">{RAW_NOT_SYNCED_LABELS[reason.reason]}。</p>;
  }
  return (
    <p data-degraded-reason="binary_not_delivered">
      结构化事件已收到，但它引用的二进制内容没有下发到本设备。
    </p>
  );
}

/** 诊断抽屉。 */
export function DiagnosticsDrawer({ model, onClose }: DiagnosticsDrawerProps): ReactElement {
  if (model === null) return <></>;

  return (
    <aside data-diagnostics-drawer="open" aria-label="事件详情">
      <Metadata model={model} />
      <ReasonNote model={model} />
      {model.raw.kind === "available" ? (
        <pre data-raw-json="available">{model.raw.rawJson}</pre>
      ) : model.raw.kind === "not_synced" ? (
        <p data-raw-unavailable={model.raw.reason}>
          原文未同步（{RAW_NOT_SYNCED_LABELS[model.raw.reason]}）；字节长度 {model.raw.byteLength}
          {model.raw.sha256 === null ? "" : `；摘要 ${model.raw.sha256}`}
        </p>
      ) : (
        <p data-raw-unavailable="absent">该事件没有随附原文。</p>
      )}
      <details data-raw-view="event-view">
        <summary>事件视图</summary>
        <pre>{JSON.stringify(model.view, null, 2)}</pre>
      </details>
      <button type="button" data-action="close-diagnostics" onClick={onClose}>
        关闭
      </button>
    </aside>
  );
}
