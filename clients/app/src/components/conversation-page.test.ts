/**
 * WP6 · 对话页整页渲染（spec 的呈现层落点；tasks 6.2–6.5）。
 *
 * ## 为什么是「页面正文」而不是挂路由组件
 *
 * 路由文件（`app/session/[id].tsx`）只做四件事：取参数、取 store、取模型、把交互写回仓库——
 * 它的页面内容全部由 `ConversationStream` 渲染（本用例渲染的就是它）。**页面文件本身**的挂载
 * 渲染由浏览器用例负责（`clients/app/e2e/**` 用真实路由组件 + 真实组合根），
 * `src/**` 的单测**不得** import `app/**`：`src/layers.test.ts` 把「跨层反向 import」作为
 * 硬判据（组件层不得依赖页面层）。
 *
 * 因此这里钉住的是页面正文的这几件事：
 *
 * 1. **整页锚点齐全**：三栏、会话头、输入区、设置、能力脚注与次级视图切换全部出现在同一份渲染里；
 * 2. **页面只是模型的函数**：同一份代码在「已关闭」与「运行中」两种快照下渲染出不同的输入区状态；
 *    「快照里没有这个会话」在模型层是 `null`（页面据此呈现明确说明，而不是伪造空数据）；
 * 3. **渲染不派发命令**：渲染期间网关没有任何新的派发记录（读取在 `useEffect` 里、命令只在交互里上抛）；
 * 4. **左栏与输入区的组合**：目录上下文 / 目录内搜索（**只**影响左栏）/ 同目录会话 / 已显示数与
 *    继续加载 / 吸底新建；上下文胶囊 + 告警条、模型与模式胶囊、提示行；
 * 5. **连接事实**：未在线时给出内容来源与上次同步时刻（在线时不给），会话头的 Agent 胶囊在
 *    覆盖层缺席时呈现「状态未知」。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ConversationStreamProps } from "./index";
import { ConversationStream, RuntimeUnavailable, StatusPill } from "./index";
import { composerKeyAction } from "./ConversationComposer";
import { classifyUserInputsInBand, jumpPillOf } from "./ConversationStream";
import type { ConversationPageModel } from "../features/conversation-model";
import type { ClientStore } from "../features/client-store";
import type { SessionSummary } from "../protocol";
import { SESSION_HISTORY_FEATURE } from "../sync-client";
import {
  ONLINE_SESSION_ID,
  WORKSPACES,
  WORKSPACES_WITH_PATHS,
  eventMessage,
  historyItem,
  lastDispatch,
  messageItem,
  onlineStore,
  readResult,
  resources,
  session,
} from "../features/testing/fixtures";
import type { StoreFixture } from "../features/testing/fixtures";

/** 渲染页面正文。 */
function renderPage(model: ConversationPageModel, extra: Partial<ConversationStreamProps> = {}): string {
  return renderToStaticMarkup(
    createElement(ConversationStream, {
      model,
      onLoadEarlier: () => undefined,
      onLoadEarlierHistory: () => undefined,
      onOpenDiagnostics: () => undefined,
      onAnswerInteraction: () => undefined,
      onSelectModel: () => undefined,
      onSelectMode: () => undefined,
      onOpenChangeDiagnostics: () => undefined,
      openChangeDiagnosticsEventId: null,
      onSecondaryViewChange: () => undefined,
      onOpenDirectoryList: () => undefined,
      onOpenSession: () => undefined,
      onSidebarQueryChange: () => undefined,
      onSidebarPageChange: () => undefined,
      onCreateSession: () => undefined,
      availableWidth: 1024,
      ...extra,
    }),
  );
}

/** 取对话页模型（夹具会话一定在快照里）。 */
function pageOf(store: ClientStore): ConversationPageModel {
  const model = store.conversationPage(ONLINE_SESSION_ID);
  if (model === null) throw new Error("会话页模型不应为 null：快照里有该会话");
  return model;
}

/** 一个内容齐全的仓库：轮次、工具调用、改动、消息。 */
function filledStore(state: SessionSummary["state"]): StoreFixture {
  const fixture = onlineStore({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state })] }) });
  const { store, gateway } = fixture;
  store.apply({
    kind: "event",
    event: eventMessage({
      eventType: "turn.started",
      eventId: "dddddddd-0000-4000-8000-000000000700",
      sessionId: ONLINE_SESSION_ID,
      globalSequence: "90",
      view: { turnId: "turn-1", state: "running" },
    }),
  });
  store.apply({
    kind: "event",
    event: eventMessage({
      eventType: "tool.call.started",
      eventId: "dddddddd-0000-4000-8000-000000000701",
      sessionId: ONLINE_SESSION_ID,
      globalSequence: "91",
      view: { toolCallId: "tool-1", title: "读取 src/a.ts", state: "running" },
    }),
  });
  store.apply({
    kind: "event",
    event: eventMessage({
      eventType: "file.changed",
      eventId: "dddddddd-0000-4000-8000-000000000702",
      sessionId: ONLINE_SESSION_ID,
      globalSequence: "92",
      view: {
        changeId: "11111111-0000-4000-8000-0000000000f1",
        kind: "update",
        displayPath: "src/a.ts",
        summary: "改了 2 行",
        addedLines: "2",
        deletedLines: "0",
        outsideWorkspace: false,
      },
    }),
  });
  store.loadSession(ONLINE_SESSION_ID);
  store.apply({
    kind: "command_result",
    result: readResult({
      requestId: lastDispatch(gateway).body.requestId,
      sessionId: ONLINE_SESSION_ID,
      messages: [messageItem({ id: "m-1", createdAt: "2026-10-01T00:00:02.000Z" })],
    }),
  });
  return fixture;
}

/** 一段文本里两个标记之间的内容（用于只看某一栏、或输入区内部的组合）。 */
function sliceBetween(html: string, from: string, to: string): string {
  const start = html.indexOf(from);
  expect(start, `渲染结果里没有 ${from}`).toBeGreaterThanOrEqual(0);
  const end = html.indexOf(to, start);
  expect(end, `${from} 之后没有 ${to}`).toBeGreaterThanOrEqual(0);
  return html.slice(start, end);
}

describe("对话页正文：整页锚点", () => {
  it("一页里三栏、时间线、输入区、设置、能力脚注与次级视图切换都在", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store));

    expect(html).toContain('data-route="session"');
    expect(html).toContain(`data-session-id="${ONLINE_SESSION_ID}"`);
    expect(html).toContain("data-session-header=");
    expect(html).toContain('data-conversation-page="true"');
    expect(html).toContain('data-conversation-column="sessions"');
    expect(html).toContain('data-timeline="entries"');
    expect(html).toContain('data-message-id="m-1"');
    expect(html).toContain('data-timeline-card="turn"');
    expect(html).toContain('data-timeline-card="tool-call"');
    expect(html).toContain('data-conversation-column="timeline"');
    expect(html).toContain('data-conversation-column="files"');
    expect(html).toContain('data-file-changes="list"');
    expect(html).toContain('data-change-path="src/a.ts"');
    expect(html).toContain('data-composer="in_progress"');
    expect(html).toContain('data-settings="true"');
    expect(html).not.toContain('data-segment="conversation-secondary-view"');
    expect(html).toContain('data-capabilities="true"');
    expect(html).toContain('data-can-send="true"');
  });

  it("三栏各有滚动容器；中栏自上而下是会话头、内容来源条、时间线滚动区、输入区", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store), {
      connectionState: "reconnecting",
      lastSyncedAtMs: 1_700_000_000_000,
    });

    expect(html).toContain('data-conversation-scroll="sessions"');
    expect(html).toContain('data-conversation-scroll="timeline"');
    expect(html).toContain('data-conversation-scroll="files"');

    // 输入区在**中栏**里（不再横跨三栏，也不在正文之外）。
    const timeline = sliceBetween(
      html,
      'data-conversation-column="timeline"',
      'data-conversation-column="files"',
    );
    expect(timeline).toContain('data-composer="in_progress"');
    expect(timeline.indexOf('data-session-header="')).toBeLessThan(timeline.indexOf('data-connection-strip="true"'));
    expect(timeline.indexOf('data-connection-strip="true"')).toBeLessThan(
      timeline.indexOf('data-conversation-scroll="timeline"'),
    );
    expect(timeline.indexOf('data-conversation-scroll="timeline"')).toBeLessThan(timeline.indexOf('data-composer="'));

    // 切换控件在三栏之外：它出现在正文容器之前。
    expect(html.indexOf('data-segment="conversation-secondary-view"')).toBeLessThan(
      html.indexOf('data-conversation-body="true"'),
    );
  });

  it("页面不构造载荷：渲染期间没有任何新的派发（命令只在交互里上抛）", () => {
    const { store, gateway } = filledStore("running");
    const before = gateway.dispatches.length;

    renderPage(pageOf(store));

    // 反例：页面在渲染里顺手构造命令（例如提交/取消）→ 这里会多出派发记录。
    expect(gateway.dispatches.length).toBe(before);
  });

  it("页面只是模型的函数：同一份代码在「已关闭」快照下渲染出关闭的输入区", () => {
    const running = renderPage(pageOf(filledStore("running").store));
    const closed = renderPage(pageOf(filledStore("closed").store));

    expect(running).toContain('data-composer="in_progress"');
    expect(closed).toContain('data-composer="closed"');
    expect(closed).toContain("会话已关闭，不能提交");
    expect(running).not.toEqual(closed);
  });

  it("快照里没有这个会话时模型给 null（页面据此呈现明确说明，而不是伪造空数据）", () => {
    const { store } = onlineStore({
      resources: resources({ sessions: [session({ id: "aaaaaaaa-0000-4000-8000-000000000777" })] }),
    });

    expect(store.conversationPage(ONLINE_SESSION_ID)).toBeNull();
    // 路由在 `store === null` 时走的就是这个组件（组合根尚未装配 ≠ 本机没有数据）。
    const unavailable = renderToStaticMarkup(createElement(RuntimeUnavailable, { route: "session" }));
    expect(unavailable).toContain('data-runtime="unavailable"');
    expect(unavailable).toContain("客户端初始化失败");
  });
});

describe("对话页正文：历史条目与实时事件同一套呈现（重开页面）", () => {
  it("历史条目（只由 `session.read` 带回）也渲染成卡片", () => {
    const { store, gateway } = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "idle" })] }),
      features: [SESSION_HISTORY_FEATURE],
    });
    store.loadSession(ONLINE_SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        history: [
          historyItem({
            eventId: "dddddddd-0000-4000-8000-000000000703",
            eventType: "tool.call.completed",
            sessionSequence: "9",
            view: { toolCallId: "tool-1", title: "读取 src/a.ts", state: "completed" },
          }),
        ],
      }),
    });

    const html = renderPage(pageOf(store));
    expect(html).toContain('data-timeline-card="tool-call"');
    expect(html).toContain('data-tool-call-id="tool-1"');
    expect(html).toContain('data-tool-state="completed"');
  });

  it("结构化历史不可用（未协商 feature）时给出明确说明", () => {
    const { store } = filledStore("idle");
    const html = renderPage(pageOf(store));

    expect(html).toContain('data-structured-history="unavailable"');
    expect(html).toContain("未协商 core.session-history.v1");
  });

  it("两套翻页各自有入口（消息与结构化历史互不影响）", () => {
    const { store, gateway } = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "idle" })] }),
      features: [SESSION_HISTORY_FEATURE],
    });
    store.loadSession(ONLINE_SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        messages: [messageItem({ id: "m-1", createdAt: "2026-10-01T00:00:02.000Z" })],
        history: [
          historyItem({
            eventId: "dddddddd-0000-4000-8000-000000000704",
            eventType: "turn.started",
            sessionSequence: "4",
            view: { turnId: "turn-1", state: "running" },
          }),
        ],
        hasEarlier: true,
        historyHasEarlier: true,
      }),
    });
    const html = renderPage(pageOf(store));

    expect(html).toContain('data-action="load-earlier"');
    expect(html).toContain('data-paging="earlier"');
    expect(html).toContain('data-action="load-earlier-history"');
    expect(html).toContain('data-history-paging="earlier"');
  });
});

describe("会话头与 Agent 胶囊（同一份连接覆盖层）", () => {
  it("覆盖层缺席时逐个呈现标识与「状态未知」，MUST NOT 写成已断开", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store), { hostPanel: store.hostPanel() });

    // 会话头三项：标题、状态徽标、Agent 名。
    expect(html).toContain('data-session-title="true"');
    expect(html).toContain("data-session-state=");
    expect(html).toContain('data-session-agent="Claude"');
    // 胶囊的弹层内容**始终在 DOM**（收起时只 `display: none`），因此静置状态就断言得到。
    expect(html).toContain('data-agent-chip="agents"');
    expect(html).toContain('data-agent-popover="closed"');
    expect(html).toContain('data-agent-popover-title="本会话涉及的 Agent"');
    expect(html).toContain("本会话涉及的 Agent");
    // 覆盖层缺席 = 没有任何 Agent 处于连接态：胶囊圆点取空闲色（不冒充运行中）。
    expect(html).toContain('data-agent-chip-dot="idle"');
    expect(html.match(/data-agent-row-dot="unknown"/g) ?? []).toHaveLength(2);
    expect(html).toContain('data-agent-row="codex-a"');
    expect(html).toContain('data-agent-row="codex-b"');
    expect(html).toContain('data-agent-row-id="codex-a"');
    expect(html).toContain('data-agent-row-connection="状态未知"');
    expect(html).toContain("状态未知");
    // 反例：把「覆盖层缺席」写成「已断开」→ 下面两条会红（判定看的是属性取值，不是文案子串）。
    expect(html).not.toContain('data-agent-connection="disconnected"');
    // 两个 Agent（`codex-a` / `codex-b`）各一行，两行的连接状态都是「未知」。
    expect(html.match(/data-agent-connection="unknown"/g) ?? []).toHaveLength(2);
    // 名单来源必须说明清楚（连接事件，不是静态配置）。
    expect(html).toContain('data-agent-chip-source="connection-events"');
    expect(html).toContain("连接事件");
  });

  it("会话头是单行：标题、状态胶囊、Agent 名、弹性空白、Agent 胶囊依次排列", () => {
    const fixture = filledStore("running");
    const html = renderPage(pageOf(fixture.store), { hostPanel: fixture.store.hostPanel() });
    const header = sliceBetween(html, 'data-session-header="', 'data-conversation-scroll="timeline"');

    expect(header).toContain('data-session-header-layout="single-line"');
    expect(header).toContain('data-session-title="true"');
    expect(header).toContain('data-session-state="running"');
    expect(header).toContain('data-session-agent="Claude"');
    expect(header).toContain('data-agent-chip="agents"');
    // 顺序：标题 → 状态胶囊 → Agent 名 → Agent 胶囊（弹性空白把它们推成左右两段）。
    expect(header.indexOf('data-session-title="true"')).toBeLessThan(header.indexOf('data-session-state="running"'));
    expect(header.indexOf('data-session-state="running"')).toBeLessThan(header.indexOf('data-session-agent="Claude"'));
    expect(header.indexOf('data-session-agent="Claude"')).toBeLessThan(header.indexOf('data-agent-chip="agents"'));
  });

  it("状态胶囊带字形：运行/排队 ●、等待 ⏸、失败 ✕、关闭/空闲 ○；等你回复用警告色", () => {
    const cases = [
      ["running", "●", "okSoft"],
      ["queued", "●", "accentSoft"],
      ["waiting_permission", "⏸", "warnSoft"],
      ["waiting_input", "⏸", "warnSoft"],
      ["failed", "✕", "dangerSoft"],
      ["closed", "○", "panel2"],
      ["idle", "○", "panel2"],
    ] as const;
    for (const [state, glyph, tone] of cases) {
      const html = renderToStaticMarkup(createElement(StatusPill, { state }));
      expect(html, state).toContain(`data-session-state="${state}"`);
      expect(html, state).toContain(`data-session-state-glyph="${glyph}"`);
      expect(html, state).toContain(`data-session-state-tone="${tone}"`);
      // 既有词表不动：字形只是前缀，文案仍是那七个词之一。
      expect(html, state).toContain(glyph);
    }
    // 反例：等你回复沿用思考紫 → 上面 `waiting_input` 的 warnSoft 断言会红。
  });

  it("没有主机面板数据时不呈现胶囊（不编一份 Agent 名单）", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store));

    expect(html).not.toContain("data-agent-chip=");
  });
});

describe("内容来源条：断连时标注上次同步", () => {
  it("未在线时给出「内容来自上次同步」与该次同步时刻；在线时不呈现", () => {
    const { store } = filledStore("running");
    const offline = renderPage(pageOf(store), { connectionState: "disconnected", lastSyncedAtMs: 1_700_000_000_000 });

    expect(offline).toContain('data-connection-strip="true"');
    expect(offline).toContain('data-content-origin="last_sync"');
    expect(offline).toContain("内容来自上次同步");
    expect(offline).toContain('data-synced-at="1700000000000"');
    expect(offline).toContain("已同步 2023-11-14 22:13 UTC");

    // 在线时不呈现来源条（那时内容就是本次同步来的）。
    const online = renderPage(pageOf(store), { connectionState: "online", lastSyncedAtMs: 1_700_000_000_000 });
    expect(online).not.toContain("data-connection-strip");
    expect(online).not.toContain("data-synced-at");
  });

  it("按连接状态换说明句与色调；来源条不是第二个状态徽标", () => {
    const { store } = filledStore("running");

    const reconnecting = renderPage(pageOf(store), {
      connectionState: "reconnecting",
      lastSyncedAtMs: 1_700_000_000_000,
    });
    const reconnectingStrip = sliceBetween(
      reconnecting,
      'data-connection-strip="true"',
      'data-conversation-scroll="timeline"',
    );
    expect(reconnectingStrip).toContain('data-connection-strip-state="reconnecting"');
    expect(reconnectingStrip).toContain('data-connection-strip-tone="warn"');
    expect(reconnectingStrip).toContain("正在重连电脑");
    expect(reconnectingStrip).toContain('data-synced-at="1700000000000"');
    // 来源条里没有设备胶囊的圆点/短词结构。
    expect(reconnectingStrip).not.toContain("data-device-capsule");
    expect(reconnectingStrip).not.toContain("data-capsule-short-word");

    const disconnected = renderPage(pageOf(store), {
      connectionState: "disconnected",
      lastSyncedAtMs: 1_700_000_000_000,
    });
    const disconnectedStrip = sliceBetween(
      disconnected,
      'data-connection-strip="true"',
      'data-conversation-scroll="timeline"',
    );
    expect(disconnectedStrip).toContain('data-connection-strip-state="disconnected"');
    expect(disconnectedStrip).toContain('data-connection-strip-tone="danger"');
    expect(disconnectedStrip).toContain("连接已断开");

    // 在线时不呈现来源条（既有断言在上一用例；这里再钉一次状态变化的差别）。
    const online = renderPage(pageOf(store), { connectionState: "online" });
    expect(online).not.toContain("data-connection-strip");
  });

  it("从未成功同步过时不给时刻（不编造一个）", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store), { connectionState: "reconnecting", lastSyncedAtMs: null });

    expect(html).toContain('data-connection-strip="true"');
    expect(html).not.toContain("data-synced-at");
  });
});

describe("对话页左栏：目录上下文与同目录会话", () => {
  /** 同目录两条 + 另一个目录一条；可选注入带路径的目录引用。 */
  function sidebarStore(options: { readonly withPaths?: boolean } = {}): StoreFixture {
    return onlineStore({
      resources: resources({
        sessions: [
          session({ id: ONLINE_SESSION_ID, title: "当前这条", state: "running" }),
          session({ id: "dddddddd-0000-4000-8000-0000000000b3", title: "同目录的另一条", state: "idle" }),
          session({ id: "dddddddd-0000-4000-8000-0000000000b2", title: "别的目录的一条", directoryId: "personal-api" }),
        ],
        workspaces: options.withPaths === true ? WORKSPACES_WITH_PATHS : WORKSPACES,
      }),
    });
  }

  it("目录上下文、搜索、同目录会话与原型分页文案都在", () => {
    const { store } = sidebarStore({ withPaths: true });
    const html = renderPage(pageOf(store));

    expect(html).toContain('data-conversation-sidebar="true"');
    expect(html).toContain('data-sidebar-directory="work-api"');
    expect(html).toContain('data-sidebar-directory-name="api"');
    // 路径逐字取自服务端下发的引用。
    expect(html).toContain('data-sidebar-directory-path="D:\\work\\api"');
    expect(html).not.toContain('data-action="open-directory"');
    expect(html).toContain('data-sidebar-search="true"');
    // 只列同目录会话（当前会话被标记为选中）。
    expect(html).toContain(`data-sidebar-session="${ONLINE_SESSION_ID}"`);
    expect(html).toContain('data-sidebar-session="dddddddd-0000-4000-8000-0000000000b3"');
    expect(html).not.toContain('data-sidebar-session="dddddddd-0000-4000-8000-0000000000b2"');
    expect(html).toContain(`data-sidebar-session-selected="true"`);
    expect(html).toContain('data-sidebar-shown="2"');
    expect(html).toContain('data-sidebar-total="2"');
    expect(html).not.toContain("已显示该目录全部会话");
    expect(html).toContain('data-sidebar-page="1"');
    expect(html).toContain('data-sidebar-page-count="1"');
    // 吸底的新建入口与副作用提示。
    expect(html).toContain('data-action="create-session-in-directory"');
    expect(html).not.toContain("将在电脑上启动 Agent");
    // 两条会话都够一页，因此没有「加载更多」。
    expect(html).not.toContain('data-action="load-more-sidebar"');
  });

  it("左栏保留返回目录入口，移除多余的全部按钮，目录头与新建按钮位于列表滚动区外", () => {
    const { store } = sidebarStore({ withPaths: true });
    const html = renderPage(pageOf(store));
    const sidebar = sliceBetween(html, 'data-conversation-column="sessions"', 'data-conversation-column="timeline"');

    // 与原型一致，只保留返回目录列表入口。
    expect(sidebar).toContain('data-action="open-directory-list"');
    expect(sidebar).toContain("‹ 目录");
    expect(sidebar).not.toContain('data-action="open-directory"');
    expect(sidebar).not.toContain("全部 ›");
    // 路径是带标签的一行，不是展示名的一部分。
    expect(sidebar).toContain('data-sidebar-directory-path-label="路径"');
    expect(sidebar).toContain('data-sidebar-search-wrap="true"');
    const scroll = sidebar.indexOf('data-conversation-scroll="sessions"');
    expect(sidebar.indexOf('data-sidebar-search="true"')).toBeLessThan(scroll);
    expect(sidebar.indexOf('data-sidebar-sessions="list"')).toBeGreaterThan(scroll);
    expect(sidebar.indexOf('data-action="create-session-in-directory"')).toBeGreaterThan(
      sidebar.indexOf('data-sidebar-shown='),
    );
  });

  it("左栏会话行只呈现圆点、标题和时间，不附加状态词与 Agent 名", () => {
    const { store } = onlineStore({
      resources: resources({
        sessions: [
          session({ id: ONLINE_SESSION_ID, title: "当前这条", state: "running" }),
          session({ id: "dddddddd-0000-4000-8000-0000000000b3", title: "等你回复的一条", state: "waiting_input" }),
          session({ id: "dddddddd-0000-4000-8000-0000000000b4", title: "空闲的一条", state: "idle" }),
        ],
      }),
    });
    const html = renderPage(pageOf(store));
    const list = sliceBetween(html, 'data-sidebar-sessions="list"', 'data-sidebar-shown=');

    // 三条都是紧凑行；圆点、标题、时间仍在，等待中的那条还带状态词。
    expect(list.match(/data-session-row-variant="compact"/g) ?? []).toHaveLength(3);
    expect(list).toContain('data-session-state="running"');
    expect(list).toContain('data-session-row-title="true"');
    expect(list).not.toContain('data-session-title="true"');
    expect(list).toContain('data-sidebar-session-selected="true"');
    expect(list).toContain("当前这条");
    expect(list).toContain("等你回复");
    expect(list).toContain('data-session-updated-at="2026-10-01T00:00:00.000Z"');
    // 反例：把目录页的五字段行原样搬进左栏 → Agent 名会出现在这里，下面两条会红。
    expect(list).not.toContain("Claude");
    expect(list).not.toContain("data-session-agent=");
    expect(list.match(/data-session-state="waiting_input"/g) ?? []).toHaveLength(1);
  });

  it("路径始终在场：逐字来自快照条目（不拿展示名拼一段看起来像路径的文本）", () => {
    const { store } = sidebarStore();
    const html = renderPage(pageOf(store));

    expect(html).toContain('data-sidebar-directory-name="api"');
    expect(html).toContain('data-sidebar-directory-path="D:\\work\\api"');
  });

  it("超过一批时呈现「已显示 N / M」与继续加载；加载后分母不变、分子变大", () => {
    const many = [
      session({ id: ONLINE_SESSION_ID, title: "当前这条", state: "running" }),
      ...Array.from({ length: 17 }, (_, index) =>
        session({ id: `dddddddd-0000-4000-8000-0000000000${String(index + 10)}`, title: `会话 ${String(index)}` }),
      ),
    ];
    const { store } = onlineStore({ resources: resources({ sessions: many }) });
    const first = renderPage(pageOf(store));

    expect(first).toContain('data-sidebar-shown="15"');
    expect(first).toContain('data-sidebar-total="18"');
    expect(first).toContain('data-sidebar-page="1"');
    expect(first).toContain('data-sidebar-page-count="2"');
    expect(first).toContain('data-session-ordinal="1"');
    expect(first).toContain('data-session-ordinal="15"');
    expect(first).toContain('data-action="page-next"');
    expect(first).not.toContain('data-action="load-more-sidebar"');

    store.setConversationSidebarPage("2");
    const second = renderPage(pageOf(store));
    expect(second).toContain('data-sidebar-shown="3"');
    expect(second).toContain('data-sidebar-page="2"');
    expect(second).toContain('data-session-ordinal="16"');
    expect(second).toContain('data-session-ordinal="18"');
    expect(second).not.toContain('data-session-ordinal="1"');
    expect(second).toContain('data-sidebar-total="18"');
    expect(second).not.toContain('data-action="load-more-sidebar"');
  });

  it("目录内搜索只影响左栏：命中集合变化，中栏一字不变", () => {
    const { store } = sidebarStore();
    const before = renderPage(pageOf(store));
    const midBefore = sliceBetween(before, 'data-conversation-column="timeline"', 'data-conversation-column="files"');

    store.setConversationSidebarQuery("另一条");
    const after = renderPage(pageOf(store));
    const midAfter = sliceBetween(after, 'data-conversation-column="timeline"', 'data-conversation-column="files"');

    // 左栏真的过滤了（当前会话不在命中里，只剩另一条）。
    expect(after).toContain('data-sidebar-shown="1"');
    expect(after).toContain('data-sidebar-total="1"');
    expect(after).not.toContain(`data-sidebar-session="${ONLINE_SESSION_ID}"`);
    // 中栏（会话头 + 时间线 + 输入区）完全没变：搜索不是「重新读一次会话」。
    expect(midAfter).toEqual(midBefore);
    // 反例：把搜索词也带进中栏（例如按搜索词过滤时间线）→ 上面这条会红。
    expect(midBefore).toContain('data-timeline="entries"');
  });

  it("没有目录归属时说明它，而不是隐藏左栏；中栏照常", () => {
    const { store } = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, title: "没有目录", directoryId: null })] }),
    });
    const html = renderPage(pageOf(store));

    expect(html).toContain('data-sidebar-notice="no_directory"');
    expect(html).toContain("没有目录归属");
    expect(html).not.toContain("data-sidebar-directory=");
    expect(html).not.toContain('data-action="create-session-in-directory"');
    expect(html).toContain('data-conversation-column="timeline"');
    expect(html).toContain('data-timeline="entries"');
  });
});

describe("输入区组合：上下文胶囊、告警条、模型与模式胶囊", () => {
  /** 一条用量事件（`session.usage.changed` 的两项计数都是十进制字符串）。 */
  function applyUsage(store: ClientStore, used: string, size: string): void {
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.usage.changed",
        eventId: "dddddddd-0000-4000-8000-000000000705",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "95",
        view: { used, size },
      }),
    });
  }

  it("上下文胶囊与模型/模式胶囊都在输入区里；≥85% 才出告警条", () => {
    const { store } = filledStore("running");
    applyUsage(store, "188000", "200000");
    const html = renderPage(pageOf(store));
    const bar = sliceBetween(html, 'data-composer-bar="true"', 'data-composer-input');

    expect(bar).toContain('data-context-state="known"');
    expect(bar).toContain('data-settings="true"');
    expect(bar).toContain('data-settings-pill="model"');
    expect(bar).toContain('data-settings-pill="mode"');
    expect(html).toContain('data-context-warning="true"');
    expect(html).toContain("可能截断较早的内容");
    expect(html).toContain("这是 Agent 的行为，网页端只做提示");

    // 未达阈值（12%）：不呈现告警条。
    const low = filledStore("running");
    applyUsage(low.store, "24000", "200000");
    const lowHtml = renderPage(pageOf(low.store));
    expect(lowHtml).toContain('data-context-state="known"');
    expect(lowHtml).not.toContain("data-context-warning");
  });

  it("未上报时不给数字（不推算），也不阻断其它交互", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store));

    expect(html).toContain('data-context-state="unreported"');
    expect(html).not.toContain("data-context-used");
    expect(html).not.toContain("data-context-percent");
    expect(html).not.toContain("data-context-warning");
    // 未上报不是禁用理由：会话能力与输入区都不受影响。进行中的输入区是只读 + 停止：
    // 渲染停止而不渲染发送（WP4 的口径；本条与「输入区只读」的用例互相印证）。
    expect(html).toContain('data-can-send="true"');
    expect(html).not.toContain('data-action="submit-prompt"');
    expect(html).not.toContain("data-submit-enabled");
    expect(html).toContain('data-action="cancel-turn"');
  });

  it("模型与模式胶囊的弹层内容始终在 DOM（收起时只是不显示）", () => {
    const { store } = filledStore("running");
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.config.changed",
        eventId: "dddddddd-0000-4000-8000-000000000706",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "96",
        view: {
          version: "3",
          configOptions: [
            {
              id: "model",
              name: "模型",
              description: null,
              category: "model",
              type: "select",
              currentValue: "fast",
              options: [
                { value: "fast", name: "快", description: null },
                { value: "smart", name: "聪明", description: null },
              ],
            },
          ],
        },
      }),
    });
    const html = renderPage(pageOf(store), { onSelectModel: () => undefined });

    expect(html).toContain('data-settings-pill="model"');
    expect(html).toContain('data-settings-pill-open="false"');
    expect(html).toContain('data-settings-popover="model"');
    expect(html).toContain('data-settings-popover-open="false"');
    // 收起状态下的弹层里仍有当前值与两个可选项（「未提供」与「有可选项」因此不可能混淆）。
    expect(html).toContain('data-settings-current-name="快"');
    expect(html).toContain('data-model-choice="fast"');
    expect(html).toContain('data-model-choice="smart"');
  });

  it("待处理跳转条：会话在等授权时呈现，条数只数「本设备可应答」的", () => {
    const { store } = filledStore("running");
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "permission.requested",
        eventId: "dddddddd-0000-4000-8000-000000000707",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "97",
        view: {
          interactionId: "perm-2",
          title: "写入工作区",
          options: [{ optionId: "allow_once", label: "允许一次", kind: "allow_once" }],
        },
      }),
    });
    const html = renderPage(pageOf(store), { hostPanel: store.hostPanel() });

    // 会话状态仍是 `running`（没有在等我们），因此不呈现跳转条。
    expect(html).not.toContain("data-pending-jump-bar");

    const waiting = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "waiting_permission" })] }),
    });
    waiting.store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "permission.requested",
        eventId: "dddddddd-0000-4000-8000-000000000708",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "98",
        view: {
          interactionId: "perm-3",
          title: "写入工作区",
          options: [{ optionId: "allow_once", label: "允许一次", kind: "allow_once" }],
        },
      }),
    });
    const waitingHtml = renderPage(pageOf(waiting.store));
    expect(waitingHtml).toContain('data-pending-jump-bar="waiting_permission"');
    expect(waitingHtml).toContain("正在等待授权");
    expect(waitingHtml).toContain("待处理 1 项");
    expect(waitingHtml).toContain('data-action="jump-to-pending"');
  });

  it("提示行给出回车口径与附件说明", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store));

    expect(html).toContain('data-composer-hint="true"');
    expect(html).toContain('data-composer-keys="enter-submits"');
    expect(html).toContain("Enter");
    expect(html).toContain("Shift");
    expect(html).toContain("换行");
    expect(html).toContain('data-attachment-notice="true"');
    expect(html).toContain("附件暂不支持");
  });
});

describe("回车提交的判定（不可提交时不得发命令）", () => {
  it("可提交才提交；Shift+回车换行；不可提交时拦下且不发命令", () => {
    expect(composerKeyAction(true, { key: "Enter" })).toBe("submit");
    expect(composerKeyAction(true, { key: "Enter", shiftKey: true })).toBe("newline");
    expect(composerKeyAction(true, { key: "a" })).toBe("ignore");
    expect(composerKeyAction(false, { key: "Backspace" })).toBe("ignore");
    // 反例：把不可提交也当成 `submit`（组件一按回车就派发命令）→ 下面两条会红。
    expect(composerKeyAction(false, { key: "Enter" })).toBe("blocked");
    expect(composerKeyAction(false, { key: "Enter", shiftKey: false })).toBe("blocked");
  });
});

/**
 * 可见带换算（r2 检视 F1/F3）。
 *
 * `getBoundingClientRect()` 给的是**视口坐标**，跳转判定却以中栏滚动容器的可见带为原点；
 * `classifyUserInputsInBand` 是这一步换算的**唯一**实现（`measure` 直接调用它）。这里直接喂
 * 视口几何，不读源码：容器顶边以上被裁掉的消息即使还落在视口里，也不算「看得到」。
 */
describe("可见带换算：以中栏滚动容器的顶边为原点", () => {
  it("容器顶边以上被裁掉的消息不算 visible", () => {
    // 容器从视口 y=120 开始、高 600；两条消息都在容器顶边之上（虽然仍在视口内）。
    const distribution = classifyUserInputsInBand(
      [
        { top: 40, bottom: 100 },
        { top: -20, bottom: 30 },
      ],
      120,
      600,
    );
    expect(distribution).toEqual({ visible: 0, above: [0, 1], below: [] });
    expect(jumpPillOf(distribution)).toEqual({ direction: "up", remaining: 2 });
  });

  it("落在容器内的消息算 visible，即使它在视口里不靠上", () => {
    // 视口坐标 200..320，换算到容器内是 80..200（容器高 600）——可见。
    const distribution = classifyUserInputsInBand([{ top: 200, bottom: 320 }], 120, 600);
    expect(distribution).toEqual({ visible: 1, above: [], below: [] });
    expect(jumpPillOf(distribution)).toBeNull();
  });
});
