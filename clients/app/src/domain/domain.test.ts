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

import type { AgentCatalogEntry, RawAcp, SessionSummary, WorkspaceCatalogEntry } from "../protocol/common";
import { buildAgentCatalogView } from "./agent-catalog";
import { buildSessionReadPageView, buildContextUsageView, contextUsageLabel } from "./conversation";
import { buildDirectoryViews, directoryNamesOf } from "./directory";
import { buildSessionSummaryView, UNTITLED_SESSION_LABEL } from "./session-summary";
import { buildTimelineCard, groupStructuredRecords, toolDetailOf } from "./timeline";
import type { StructuredRecord, ToolCallTimelineCard } from "./timeline";

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
    { directoryId: "work-api", path: "D:\\work\\api" },
    { directoryId: "site", path: "/home/dev/site" },
  ] as const satisfies readonly WorkspaceCatalogEntry[];

  it("目录项以 directoryId 为 id，展示名取路径尾段，路径逐字带出", () => {
    const state = buildDirectoryViews(workspaces, [
      summary({ sessionId: "s1", workspace: { directoryId: workspaces[0].directoryId } }),
    ]);
    expect(state.kind).toBe("ready");
    if (state.kind !== "ready") return;
    for (const directory of state.directories) {
      expect(Object.keys(directory).sort()).toEqual(["counts", "displayName", "id", "path", "pathTail"]);
    }
    // 服务端下发的路径被**逐字**带出（不规范化、不拼接、不反查）。
    expect(state.directories.map((directory) => directory.path)).toEqual(["D:\\work\\api", "/home/dev/site"]);
    expect(state.directories.map((directory) => directory.displayName)).toEqual(["api", "site"]);
    // 尾段不同：不需要消歧段。
    expect(state.directories.map((directory) => directory.pathTail)).toEqual([null, null]);
  });

  it("尾段相同的目录用更多路径段消歧（按整份快照计算）", () => {
    const duplicateNamed = [
      { directoryId: "api-work", path: "D:\\work\\api" },
      { directoryId: "api-personal", path: "D:\\personal\\api" },
    ] as const satisfies readonly WorkspaceCatalogEntry[];
    const state = buildDirectoryViews(duplicateNamed, []);
    expect(state.kind).toBe("empty");
    // 空态下仍能证明两项未被合并：换成有会话的输入再核对。
    const withSessions = buildDirectoryViews(duplicateNamed, [
      summary({ sessionId: "s1", workspace: { directoryId: duplicateNamed[0].directoryId } }),
      summary({ sessionId: "s2", workspace: { directoryId: duplicateNamed[1].directoryId } }),
    ]);
    expect(withSessions.kind).toBe("ready");
    if (withSessions.kind !== "ready") return;
    expect(withSessions.directories).toHaveLength(2);
    expect(withSessions.directories.map((d) => d.id)).toEqual(["api-work", "api-personal"]);
    expect(withSessions.directories.map((d) => d.path)).toEqual(["D:\\work\\api", "D:\\personal\\api"]);
    // 展示名仍是尾段；消歧段是父目录那一段（逐字取自路径）。
    expect(withSessions.directories.map((d) => d.displayName)).toEqual(["api", "api"]);
    expect(withSessions.directories.map((d) => d.pathTail)).toEqual(["work", "personal"]);
  });

  it("父目录也同名时继续往前取段，直到能区分", () => {
    const nested = [
      { directoryId: "a", path: "D:\\alpha\\vault\\api" },
      { directoryId: "b", path: "D:\\beta\\vault\\api" },
    ] as const satisfies readonly WorkspaceCatalogEntry[];
    const state = buildDirectoryViews(nested, []);
    expect(state.kind).toBe("empty");
    const names = directoryNamesOf(nested);
    expect(names.get("a")).toEqual({ displayName: "api", pathTail: "alpha\\vault" });
    expect(names.get("b")).toEqual({ displayName: "api", pathTail: "beta\\vault" });
  });

  it("会话数、运行中、待处理由摘要得出", () => {
    const state = buildDirectoryViews(workspaces, [
      summary({ sessionId: "s1", state: "running", workspace: { directoryId: "work-api" } }),
      summary({ sessionId: "s2", state: "running", workspace: { directoryId: "work-api" } }),
      summary({ sessionId: "s3", state: "pending", workspace: { directoryId: "work-api" } }),
      summary({ sessionId: "s4", state: "idle", workspace: { directoryId: "site" } }),
      // 未归属会话（没有 workspace 引用）不计入任何目录。
      summary({ sessionId: "s5", state: "running" }),
    ]);
    expect(state.kind).toBe("ready");
    if (state.kind !== "ready") return;
    const api = state.directories.find((d) => d.id === "work-api")!;
    expect(api.counts).toEqual({ total: 3, active: 2, pending: 1 });
    const site = state.directories.find((d) => d.id === "site")!;
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
    expect(keys).toContain("directoryId");
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

/** 造一条工具调用的结构化记录（实时来源即可：卡片构造不看来源）。 */
function toolCallRecord(input: {
  readonly eventId: string;
  readonly eventType: string;
  readonly view: Record<string, unknown>;
  readonly globalSequence?: string;
  readonly createdAt?: string;
  readonly acp?: RawAcp;
}): StructuredRecord {
  return {
    source: "realtime",
    eventId: input.eventId,
    eventType: input.eventType,
    sessionId: null,
    createdAt: input.createdAt ?? "2026-10-01T00:00:02.000Z",
    sessionSequence: null,
    globalSequence: input.globalSequence ?? "1",
    originKind: "agent",
    view: input.view,
    ...(input.acp === undefined ? {} : { acp: input.acp }),
  };
}

/** 工具调用的公共视图字段（详情由用例单独补）。 */
function toolCallView(toolDetail?: Record<string, unknown>): Record<string, unknown> {
  return {
    toolCallId: "tc-1",
    turnId: "t-1",
    title: "读取文件",
    state: "running",
    ...(toolDetail === undefined ? {} : { toolDetail }),
  };
}

/** 记录 → 唯一的一张工具调用卡片（同一 `toolCallId` 的记录组）。 */
function toolCallCard(records: readonly StructuredRecord[]): ToolCallTimelineCard {
  const group = groupStructuredRecords(records)[0];
  if (group === undefined) throw new Error("没有记录组");
  const card = buildTimelineCard(group, () => null);
  if (card === null || card.kind !== "tool_call") throw new Error("卡片不是工具调用");
  return card;
}

describe("WP2 工具详情：跨事件部分归并与严格形状", () => {
  it("updated/completed 的空 title 不降级，沿用 started 的非空标题", () => {
    const card = toolCallCard([
      toolCallRecord({
        eventId: "e-1",
        eventType: "tool.call.started",
        view: toolCallView(),
        createdAt: "2026-10-01T00:00:02.000Z",
      }),
      toolCallRecord({
        eventId: "e-2",
        eventType: "tool.call.updated",
        view: { ...toolCallView(), title: "" },
        createdAt: "2026-10-01T00:00:03.000Z",
      }),
    ]);
    expect(card.title).toBe("读取文件");
  });

  it("started 只有输入、updated 只有输出：按时间归并后两部分都保留，eventId 取最新记录", () => {
    // 故意按**到达顺序**倒置：updated 先到、started 后到；归并必须按时间而不是到达顺序。
    const card = toolCallCard([
      toolCallRecord({
        eventId: "e-updated",
        eventType: "tool.call.updated",
        globalSequence: "2",
        view: toolCallView({ output: { bytes: 3 }, text: ["新正文"] }),
      }),
      toolCallRecord({
        eventId: "e-started",
        eventType: "tool.call.started",
        globalSequence: "1",
        view: toolCallView({ input: { path: "src/a.ts" }, text: ["旧正文"] }),
      }),
    ]);

    expect(card.toolCallId).toBe("tc-1");
    expect(card.eventId).toBe("e-updated");
    expect(card.detail?.input).toEqual({ present: true, value: { path: "src/a.ts" } });
    expect(card.detail?.output).toEqual({ present: true, value: { bytes: 3 } });
    // 同一部分在两条记录里都出现：必须按时间取 updated 的「新正文」；
    // 若按到达顺序覆盖，会错误取到后到的 started 的「旧正文」。
    expect(card.detail?.text).toEqual(["新正文"]);
    expect(card.detail?.truncatedParts).toEqual([]);
  });

  it("text 取最新出现的非缺席值（跨事件归并，不回退到旧正文）", () => {
    const card = toolCallCard([
      toolCallRecord({
        eventId: "e1",
        eventType: "tool.call.started",
        globalSequence: "1",
        view: toolCallView({ input: { path: "a" }, text: ["旧正文"] }),
      }),
      toolCallRecord({
        eventId: "e2",
        eventType: "tool.call.updated",
        globalSequence: "2",
        view: toolCallView({ text: ["新正文一", "新正文二"] }),
      }),
    ]);

    expect(card.detail?.text).toEqual(["新正文一", "新正文二"]);
    // 后一条没有 input，不能因此把先前的输入丢掉。
    expect(card.detail?.input).toEqual({ present: true, value: { path: "a" } });
  });

  it("truncatedParts 取所有记录的并集，按 input→output→text 固定顺序去重", () => {
    const card = toolCallCard([
      toolCallRecord({
        eventId: "e1",
        eventType: "tool.call.started",
        globalSequence: "1",
        view: toolCallView({ truncatedParts: ["output"] }),
      }),
      toolCallRecord({
        eventId: "e2",
        eventType: "tool.call.updated",
        globalSequence: "2",
        view: toolCallView({ truncatedParts: ["text", "input"] }),
      }),
    ]);

    expect(card.detail?.truncatedParts).toEqual(["input", "output", "text"]);
    // 只有截断登记、没有内容部分时 detail 仍非 null（「数据存在但超限」可被区分）。
    expect(card.detail?.input).toBeNull();
    expect(card.detail?.output).toBeNull();
    expect(card.detail?.text).toBeNull();
  });

  it("没有 toolDetail（或内容部分全缺席且无截断）时 detail 为 null，eventId 仍保留", () => {
    const withoutDetail = toolCallCard([
      toolCallRecord({
        eventId: "e-only",
        eventType: "tool.call.completed",
        view: toolCallView(),
      }),
    ]);
    expect(withoutDetail.detail).toBeNull();
    expect(withoutDetail.eventId).toBe("e-only");

    const emptyDetail = toolCallCard([
      toolCallRecord({
        eventId: "e-empty",
        eventType: "tool.call.completed",
        view: toolCallView({}),
      }),
    ]);
    expect(emptyDetail.detail).toBeNull();
  });

  it("非法 toolDetail 形状逐部分降级为缺席，不抛异常、不伪造", () => {
    // 整个 toolDetail 不是普通对象。
    expect(toolDetailOf({ toolDetail: null })).toBeNull();
    expect(toolDetailOf({ toolDetail: [] })).toBeNull();
    expect(toolDetailOf({ toolDetail: "input" })).toBeNull();
    // text 不是字符串数组：整部分缺席（不把数字文本化）。
    expect(toolDetailOf({ toolDetail: { text: "不是数组" } })).toBeNull();
    expect(toolDetailOf({ toolDetail: { text: ["ok", 3] } })).toBeNull();
    // truncatedParts 不是数组：没有截断登记。
    expect(toolDetailOf({ toolDetail: { truncatedParts: "output" } })).toBeNull();
    // 合法部分不因同块里的非法部分或未知枚举值一起消失。
    expect(
      toolDetailOf({
        toolDetail: { input: { a: 1 }, text: 5, truncatedParts: ["output", "future"] },
      }),
    ).toEqual({
      input: { present: true, value: { a: 1 } },
      output: null,
      text: null,
      truncatedParts: ["output"],
    });
  });

  it("JSON null / false / 0 / 空字符串是存在的取值，不按缺席处理", () => {
    expect(toolDetailOf({ toolDetail: { input: null } })).toEqual({
      input: { present: true, value: null },
      output: null,
      text: null,
      truncatedParts: [],
    });
    expect(toolDetailOf({ toolDetail: { output: false } })?.output).toEqual({ present: true, value: false });
    expect(toolDetailOf({ toolDetail: { output: 0 } })?.output).toEqual({ present: true, value: 0 });
    expect(toolDetailOf({ toolDetail: { text: [""] } })?.text).toEqual([""]);
  });

  it("只读 view.toolDetail，不从 acp.rawJson 补造详情", () => {
    const acp: RawAcp = {
      mediaType: "application/json",
      rawJson: JSON.stringify({ rawInput: { path: "来自原文" }, rawOutput: { ok: true } }),
      byteLength: "60",
      sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    };
    const card = toolCallCard([
      toolCallRecord({
        eventId: "e-acp",
        eventType: "tool.call.completed",
        view: toolCallView(),
        acp,
      }),
    ]);

    // 原文里有 rawInput/rawOutput，但视图没有 toolDetail：不解析、不伪造。
    expect(card.detail).toBeNull();
  });
});
