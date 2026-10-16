/**
 * 上下文占用的告警档位：两档阈值与「占比 → 档位」判定的**唯一**定义处（`design.md` D7；R17）。
 *
 * 原型把同一个 85% 写了两遍——`ctxChip()` 的 `pct>=85?" crit"` 与 `ctxWarn()` 的 `pct<85`
 * ——胶囊的 warn 档还有第三个数字（`pct>=60`）。本变更把阈值下沉成一处定义：胶囊
 * （`ContextUsageBar`）与输入区的告警条（`ConversationComposer` 的 `contextWarningOf`）都从
 * 这里取，因此调整阈值只会改一个数字，不会再出现两处各说各话。
 *
 * 三档与原型的状态类一一对应：`ok` ↔ 无类、`warn` ↔ `.ctx.warn`、`crit` ↔ `.ctx.crit`。
 *
 * 本模块只有常量与纯函数，不认识占比从哪来——「未上报 / 窗口为零没有占比」由调用方保证：
 * 那两态不参与判定，也就不会凭 0 落到某一档。
 */

/** 上下文占用的档位。 */
export type ContextUsageLevel = "ok" | "warn" | "crit";

/** 警告档的下界（原型 `.ctx.warn`：`pct>=60`）。 */
export const CONTEXT_WARN_THRESHOLD_PERCENT = 60;

/** 危险档的下界（原型 `.ctx.crit` 与告警条 `ctxWarn()`：`pct>=85`）。 */
export const CONTEXT_CRIT_THRESHOLD_PERCENT = 85;

/**
 * 已上报的占比 → 档位。
 *
 * 用**未取整**的占比判定（与告警条同口径），而不是原型那样先 `Math.round`：本仓的占比是
 * 一位小数的精确值，先取整会让 84.6% 提前一档变红。
 */
export function contextUsageLevel(percent: number): ContextUsageLevel {
  if (percent >= CONTEXT_CRIT_THRESHOLD_PERCENT) return "crit";
  if (percent >= CONTEXT_WARN_THRESHOLD_PERCENT) return "warn";
  return "ok";
}
