/**
 * 视图模型的结构化判定（R10「视图模型不暴露协议载荷」、R15 目录汇总、R17 三态、R19 两层）。
 *
 * 两个断言方向：
 *
 * 1. **行为**：给定快照资源，视图模型给出正确的汇总、空态、状态未知与三态用量；
 * 2. **结构**：视图模型的键集合**不包含**协议字段名（例如 `origin`、`ownerNodeId`、
 *    `currentMode`、`version`），防止有人把 wire 载荷直接塞进视图类型。
 */

import { describe, expect, it } from "vitest";

import type { AgentCatalogEntry, SessionSummary } from "../protocol/common";
import { buildAgentCatalogView } from "./agent-catalog";
import { buildSessionReadPageView, buildContextUsageView, contextUsageLabel } from "./conversation";
import { buildDirectoryViews } from "./directory";
import { buildSessionSummaryView, UNTITLED_SESSION_LABEL } from "./session-summary";

/** 造一个最小合法的 `SessionSummary`。 */
function summary(overrides: Partial<SessionSummary> & Pick<SessionSummary, "sessionId">): SessionSummary {
  return {
    title: null,
    agent: { agentId: "codex", name: "Codex" },
    state: "idle",
    origin: { kind: "local" },
    currentMode: null,
    version: "1",
    createdAt: "2026-10-05T00:00:00.000Z",
    updatedAt: "2026-10-05T00:00:00.000Z",
    ...overrides,
  };
}

describe("R15 目录视图模型", () => {
  const workspaces = [
    { alias: "work-api", displayName: "api" },
    { alias: "site", displayName: "site" },
  ];

  it("目录项只含展示名与别名，不含任何路径信息", () => {
    const state = buildDirectoryViews(workspaces, [summary({ sessionId: "s1", workspace: workspaces[0]! })]);
    expect(state.kind).toBe("ready");
    if (state.kind !== "ready") return;
    for (const directory of state.directories) {
      const keys = Object.keys(directory).sort();
      expect(keys).toEqual(["alias", "counts", "displayName", "id"]);
      expect(keys).not.toContain("path");
      expect(keys).not.toContain("cwd");
      expect(keys).not.toContain("displayPath");
    }
  });

  it("会话数、运行中、待处理由摘要得出", () => {
    const state = buildDirectoryViews(workspaces, [
      summary({ sessionId: "s1", state: "running", workspace: workspaces[0]! }),
      summary({ sessionId: "s2", state: "queued", workspace: workspaces[0]! }),
      summary({ sessionId: "s3", state: "waiting_permission", workspace: workspaces[0]! }),
      summary({ sessionId: "s4", state: "idle", workspace: workspaces[1]! }),
    ]);
    expect(state.kind).toBe("ready");
    if (state.kind !== "ready") return;
    const api = state.directories.find((d) => d.alias === "work-api")!;
    expect(api.counts).toEqual({ total: 3, active: 2, pending: 1 });
    const site = state.directories.find((d) => d.alias === "site")!;
    expect(site.counts).toEqual({ total: 1, active: 0, pending: 0 });
  });

  it("没有任何目录时是 no_directories 空态", () => {
    const state = buildDirectoryViews([], []);
    expect(state).toEqual({ kind: "empty", empty: "no_directories" });
  });

  it("有目录但都无会话时是 directories_without_sessions 空态（与上者可区分）", () => {
    const state = buildDirectoryViews(workspaces, []);
    expect(state).toEqual({ kind: "empty", empty: "directories_without_sessions" });
  });

  it("同名目录以别名区分，不做路径消歧", () => {
    const duplicateNamed = [
      { alias: "api-work", displayName: "api" },
      { alias: "api-personal", displayName: "api" },
    ];
    const state = buildDirectoryViews(duplicateNamed, []);
    expect(state.kind).toBe("empty");
    // 空态下仍能证明两项未被合并：换成有会话的输入再核对。
    const withSessions = buildDirectoryViews(duplicateNamed, [
      summary({ sessionId: "s1", workspace: duplicateNamed[0]! }),
      summary({ sessionId: "s2", workspace: duplicateNamed[1]! }),
    ]);
    expect(withSessions.kind).toBe("ready");
    if (withSessions.kind !== "ready") return;
    expect(withSessions.directories).toHaveLength(2);
    expect(withSessions.directories.map((d) => d.id)).toEqual(["api-work", "api-personal"]);
  });
});

describe("会话摘要视图模型", () => {
  it("空标题呈现为未命名，且不拼装替代标题", () => {
    const view = buildSessionSummaryView(summary({ sessionId: "s1", title: null }), "");
    expect(view.title).toEqual({ kind: "untitled" });
    expect(UNTITLED_SESSION_LABEL).toBe("未命名会话");
  });

  it("本地会话的 origin 分支不含 online 字段", () => {
    const view = buildSessionSummaryView(summary({ sessionId: "s1" }), "");
    expect(view.origin).toEqual({ kind: "local" });
    expect(Object.keys(view.origin)).not.toContain("online");
  });

  it("imported 会话携带来源在线标记", () => {
    const view = buildSessionSummaryView(
      summary({
        sessionId: "s1",
        origin: { kind: "remote", ownerNodeId: "n1", exportId: "e1", originEpoch: "ep1", online: false },
      }),
      "工作电脑",
    );
    expect(view.origin).toEqual({ kind: "remote", online: false, ownerLabel: "工作电脑" });
  });

  it("视图键集合是视图模型自己的形状，不照搬 wire 字段", () => {
    const view = buildSessionSummaryView(summary({ sessionId: "s1" }), "");
    const keys = Object.keys(view).sort();
    // wire 上的 `SessionSummary` 以 `sessionId` 为标识、以 `currentMode`/`version` 承载协议状态；
    // 视图模型换成 `id` 这类展示向字段，且不携带这两个 wire 字段。
    for (const wireOnly of ["sessionId", "currentMode", "version"] as const) {
      expect(keys).not.toContain(wireOnly);
    }
    expect(keys).toContain("id");
    expect(keys).toContain("directoryAlias");
    // `origin` 在视图层是一个判别联合（local / remote），不是 wire 的 `originBlock`：
    // wire 的本地分支只有 `{kind:"local"}`，而视图层在 remote 分支上多出 `ownerLabel`。
    expect(view.origin).toEqual({ kind: "local" });
  });
});

describe("R19 Agent 目录两层呈现", () => {
  const agents: AgentCatalogEntry[] = [
    { agentId: "codex", displayName: "Codex", default: true },
    { agentId: "omp", displayName: "Oh My Pi", default: false },
  ];

  it("覆盖层缺席时状态未知，列表不受影响", () => {
    const view = buildAgentCatalogView(agents, []);
    expect(view.overlayPresent).toBe(false);
    expect(view.entries).toHaveLength(2);
    for (const entry of view.entries) {
      expect(entry.connection).toBe("unknown");
    }
    expect(view.entries.every((entry) => entry.connection !== "disconnected")).toBe(true);
  });

  it("覆盖层到达时只覆盖对应条目", () => {
    const view = buildAgentCatalogView(agents, [{ agentId: "codex", state: "connected", reason: null }]);
    expect(view.overlayPresent).toBe(true);
    expect(view.entries.find((entry) => entry.id === "codex")?.connection).toBe("connected");
    expect(view.entries.find((entry) => entry.id === "omp")?.connection).toBe("unknown");
  });

  it("断开事件可携带原因", () => {
    const view = buildAgentCatalogView(agents, [
      { agentId: "omp", state: "disconnected", reason: "进程退出" },
    ]);
    const omp = view.entries.find((entry) => entry.id === "omp")!;
    expect(omp.connection).toBe("disconnected");
    expect(omp.disconnectReason).toBe("进程退出");
  });

  it("同名 Agent 以标识区分，不按名合并", () => {
    const duplicated: AgentCatalogEntry[] = [
      { agentId: "a1", displayName: "agent", default: false },
      { agentId: "a2", displayName: "agent", default: false },
    ];
    const view = buildAgentCatalogView(duplicated, []);
    expect(view.entries.map((entry) => entry.id)).toEqual(["a1", "a2"]);
  });
});

describe("R17 上下文用量三态", () => {
  it("尚未收到用量事件时是未上报", () => {
    const usage = buildContextUsageView(null);
    expect(usage.kind).toBe("unreported");
    expect(contextUsageLabel(usage)).toBe("未上报");
  });

  it("窗口大小为零时同样是未上报，不算出占比", () => {
    const usage = buildContextUsageView({ used: "0", size: "0" });
    expect(usage.kind).toBe("zero_window");
    expect(contextUsageLabel(usage)).toBe("未上报（窗口为零）");
  });

  it("已知用量给出占比", () => {
    const usage = buildContextUsageView({ used: "25", size: "100" });
    expect(usage).toEqual({ kind: "known", used: "25", size: "100", percent: 25 });
    expect(contextUsageLabel(usage)).toBe("25.0%");
  });
});

describe("session.read 分页视图", () => {
  it("hasEarlier 直传，最早游标取首条", () => {
    const page = buildSessionReadPageView({
      sessionId: "s1",
      hasEarlier: true,
      resources: {
        messages: [
          {
            messageId: "m1",
            sessionId: "s1",
            role: "user",
            content: [{ type: "text", text: "hi" }],
            status: "final",
            createdAt: "2026-10-05T00:00:00.000Z",
            turnId: null,
          },
        ],
      },
    });
    expect(page.hasEarlier).toBe(true);
    expect(page.earliestCursor).toEqual({ createdAt: "2026-10-05T00:00:00.000Z", messageId: "m1" });
    expect(page.messages[0]!.text).toBe("hi");
    expect(page.messages[0]!.hasStructuredContent).toBe(false);
  });

  it("非文本块不冒充文本，标记为结构化内容", () => {
    const page = buildSessionReadPageView({
      sessionId: "s1",
      hasEarlier: false,
      resources: {
        messages: [
          {
            messageId: "m1",
            sessionId: "s1",
            role: "agent",
            content: [{ type: "image_ref", mimeType: "image/png", byteLength: null, displayState: "not_fetched" }],
            status: "final",
            createdAt: "2026-10-05T00:00:00.000Z",
            turnId: null,
          },
        ],
      },
    });
    expect(page.messages[0]!.text).toBe("");
    expect(page.messages[0]!.hasStructuredContent).toBe(true);
  });

  it("空一页的 earliestCursor 为 null", () => {
    const page = buildSessionReadPageView({ sessionId: "s1", hasEarlier: false, resources: {} });
    expect(page.messages).toEqual([]);
    expect(page.earliestCursor).toBeNull();
  });
});
