import { describe, expect, it } from "vitest";
import { activeConversationTodo } from "./conversation-todo";
import type { TimelineEntry } from "./conversation-timeline";

const plan = (sequence: string, statuses: readonly ("pending" | "in_progress" | "completed")[]): TimelineEntry => ({
  kind: "plan", key: `plan:${sequence}`, createdAt: "2026-10-24T00:00:00Z", sequence,
  entries: statuses.map((status, index) => ({ content: `任务 ${index}`, status, priority: "medium" })),
});
const turn = (state: string, sequence = "1"): TimelineEntry => ({
  kind: "turn", key: "turn:1", turnId: "1", state, error: null,
  createdAt: "2026-10-24T00:00:00Z", sequence,
});

describe("输入区任务清单", () => {
  it("取最新计划并跟随进度更新，不修改历史", () => {
    const history = [turn("running"), plan("2", ["pending"]), plan("3", ["completed", "in_progress"])];
    expect(activeConversationTodo(history, "running")).toMatchObject({ key: "plan:3", completed: 1 });
    expect(history).toHaveLength(3);
  });
  it.each(["completed", "cancelled", "failed", "uncertain"])("轮次 %s 即移除，即使摘要还没刷新", (state) => {
    expect(activeConversationTodo([turn(state), plan("2", ["in_progress"])], "running")).toBeNull();
  });
  it("清单完成或被清空即移除", () => {
    expect(activeConversationTodo([plan("2", ["completed"])], "running")).toBeNull();
    expect(activeConversationTodo([plan("2", ["pending"]), plan("3", [])], "running")).toBeNull();
  });
  it("新轮次不沿用上一轮任务", () => {
    expect(activeConversationTodo([plan("2", ["pending"]), turn("running", "3")], "running")).toBeNull();
  });
  it("等待用户时保留，空闲或状态未知时隐藏", () => {
    const entries = [plan("2", ["pending"])];
    expect(activeConversationTodo(entries, "pending")).not.toBeNull();
    expect(activeConversationTodo(entries, "idle")).toBeNull();
    expect(activeConversationTodo(entries, null)).toBeNull();
  });
});
