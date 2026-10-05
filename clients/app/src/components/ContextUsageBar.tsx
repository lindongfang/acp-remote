/**
 * 上下文占用条（R17「未上报的上下文不得以零冒充」）。
 *
 * ## 三态在渲染上如何保持可区分
 *
 * `ContextUsageView` 是判别联合，本组件按 `kind` 渲染三个**结构不同**的分支：
 *
 * - `unreported`：只有「未上报」一个词，**没有**进度条、没有数字——因为没有数字可显示；
 * - `zero_window`：明确说明是「窗口为零」，与「从未上报」区分；
 * - `known`：已用 / 上限 / 占比。
 *
 * 关键在于**未上报分支里根本没有数字槽位**：把它渲染成 `0 / 0` 或 `0%` 在这个组件里
 * 没有表达方式，因此「未上报以零冒充」不是靠约定避免的。
 *
 * 本组件**不**禁用任何交互（也不接收能力参数）：R17 要求未上报不得阻断其它交互，
 * 而能力由对话页模型单独呈现，两者不混在一起。
 */

import type { ReactElement } from "react";

import type { ContextUsageView } from "../domain";
import { contextUsageLabel } from "../domain";

export interface ContextUsageBarProps {
  readonly usage: ContextUsageView;
}

/** 上下文占用条。 */
export function ContextUsageBar({ usage }: ContextUsageBarProps): ReactElement {
  if (usage.kind === "unreported") {
    return (
      <div data-context-state="unreported" data-context-label={contextUsageLabel(usage)}>
        <span>上下文未上报</span>
      </div>
    );
  }

  if (usage.kind === "zero_window") {
    return (
      <div data-context-state="zero_window" data-context-label={contextUsageLabel(usage)}>
        <span>上下文未上报（窗口为零）</span>
      </div>
    );
  }

  return (
    <div data-context-state="known" data-context-label={contextUsageLabel(usage)}>
      <progress data-context-percent={usage.percent.toFixed(1)} max={100} />
      <span data-context-used={usage.used}>{usage.used}</span>
      <span data-context-size={usage.size}>{usage.size}</span>
      <span>{contextUsageLabel(usage)}</span>
    </div>
  );
}
