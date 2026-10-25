import type { SessionStateView } from "../domain/session-summary";
import { compareTimelinePosition } from "../domain/timeline";
import type { TimelineEntry } from "./conversation-timeline";

/** 输入区只显示最新计划；历史计划仍由时间线保留，不保存第二份任务状态。 */
export function activeConversationTodo(entries: readonly TimelineEntry[], state: SessionStateView | null) {
  if (state === "idle" || state === null) return null;
  const latest = entries.filter((entry) => entry.kind === "plan").sort(compareTimelinePosition).at(-1);
  if (latest === undefined || latest.entries.length === 0 || latest.entries.every((item) => item.status === "completed")) return null;
  // 轮次卡按开始位置排序，但状态已归并到终态，不能拿它的时间当作结束时间。
  const turn = entries.filter((entry) => entry.kind === "turn").sort(compareTimelinePosition).at(-1);
  if (turn !== undefined && (["completed", "cancelled", "failed", "uncertain"].includes(turn.state) ||
    compareTimelinePosition(turn, latest) > 0)) return null;
  return { key: latest.key, entries: latest.entries,
    completed: latest.entries.filter((item) => item.status === "completed").length };
}
