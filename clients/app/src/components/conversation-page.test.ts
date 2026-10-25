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
 * 2. **页面只是模型的函数**：同一份代码在「运行中」与「空闲」两种快照下渲染出各自的会话头，
 *    输入可用性只随轮次/连接变化、不随主状态变化；「快照里没有这个会话」在模型层是 `null`
 *    （页面据此呈现明确说明，而不是伪造空数据）；
 * 3. **渲染不派发命令**：渲染期间网关没有任何新的派发记录（读取在 `useEffect` 里、命令只在交互里上抛）；
 * 4. **左栏与输入区的组合**：目录上下文 / 目录内搜索（**只**影响左栏）/ 同目录会话 / 已显示数与
 *    继续加载 / 吸底新建；上下文胶囊 + 告警条、模型与模式胶囊、提示行；
 * 5. **连接事实**：页面不再呈现内容来源条或上次同步时刻；连接状态只由共享外壳的设备胶囊表达。
 *    会话头的 Agent 胶囊在覆盖层缺席时呈现「状态未知」。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ConversationStreamProps } from "./index";
import { ConversationStream, RuntimeUnavailable, StatusPill } from "./index";
import { composerKeyAction } from "./ConversationComposer";
import {
  CROSSING_FOLD_ENTER_PX,
  CROSSING_FOLD_EXIT_PX,
  CROSSING_FOLD_MIN_BODY_PX,
  SCROLL_FOLLOW_GUARD_PX,
  anchorScrollDelta,
  classifyUserInputsInBand,
  crossingFoldAtTop,
  isFollowingBottom,
  jumpPillOf,
  type CrossingFoldGeometry,
} from "./ConversationStream";
import type { ConversationPageModel } from "../features/conversation-model";
import type { ClientStore } from "../features/client-store";
import type { CommandResultMessage, ErrorCode, SessionSummary } from "../protocol";
import { LOCAL_SESSION_LIFECYCLE_FEATURE, SESSION_HISTORY_FEATURE } from "../sync-client";
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
    expect(html).toContain('data-timeline-card="file-changed"');
    expect(html).toContain('data-change-path="src/a.ts"');
    expect(html).toContain('data-composer="in_progress"');
    expect(html).toContain('data-settings="true"');
    expect(html).not.toContain('data-segment="conversation-secondary-view"');
    expect(html).toContain('data-capabilities="true"');
    expect(html).toContain('data-can-send="true"');
  });

  it("三栏各有滚动容器；中栏自上而下是会话头、时间线滚动区、输入区", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store));

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
    expect(timeline.indexOf('data-session-header="')).toBeLessThan(
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

  it("页面只是模型的函数：主状态决定会话头，不决定输入区可用性", () => {
    const running = renderPage(pageOf(filledStore("running").store));
    const idle = renderPage(pageOf(filledStore("idle").store));

    expect(running).toContain('data-session-state="running"');
    expect(idle).toContain('data-session-state="idle"');
    // 两个快照都有进行中的轮次：输入区都是「执行中」——可用性来自轮次与连接，不来自主状态。
    expect(running).toContain('data-composer="in_progress"');
    expect(idle).toContain('data-composer="in_progress"');
    // 三态里已经没有「已关闭」：既没有这个徽标，也没有这句只读说明。
    expect(running).not.toContain('data-session-state="closed"');
    expect(running).not.toContain("会话已关闭");
    expect(running).not.toEqual(idle);
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

describe("对话页：失败后空闲与页面切换", () => {
  it("失败后主状态回到空闲，本轮失败原因仍可见，且不呈现「已关闭」", () => {
    const { store } = filledStore("running");
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "turn.failed",
        eventId: "dddddddd-0000-4000-8000-00000000070b",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "93",
        view: {
          turnId: "turn-1",
          state: "failed",
          error: { code: "agent.crashed", message: "Agent 崩溃了", retryable: true, details: {} },
        },
      }),
    });
    // 服务端确认后主状态回到 idle（本轮失败不是「会话关闭」）。
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.updated",
        eventId: "dddddddd-0000-4000-8000-00000000070c",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "94",
        view: { session: { ...session({ id: ONLINE_SESSION_ID, state: "idle" }), version: "2" } },
      }),
    });

    const model = pageOf(store);
    expect(model.header.state).toBe("idle");
    expect(model.input.state).toBe("editable");
    expect(model.input.canSubmit).toBe(true);

    const html = renderPage(model);
    expect(html).toContain('data-session-state="idle"');
    expect(html).toContain('data-turn-error="agent.crashed"');
    expect(html).toContain("轮次失败：Agent 崩溃了");
    expect(html).not.toContain("已关闭");
  });

  it("摘要尚未取得时会话头说明「未知」，不编造空闲", () => {
    const target = "aaaaaaaa-0000-4000-8000-0000000000ba";
    const { store, gateway } = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "idle" })] }),
    });
    store.loadSession(target);
    store.apply({
      kind: "command_result",
      result: readResult({ requestId: lastDispatch(gateway).body.requestId, sessionId: target }),
    });

    const model = store.conversationPage(target);
    if (model === null) throw new Error("读回了线程，模型不应为 null");
    expect(model.header.state).toBeNull();
    const html = renderPage(model);
    expect(html).toContain('data-session-state="unknown"');
    expect(html).toContain("状态未知");
    // 反例：把「尚未取得」当成空闲 → 这里会出现空闲徽标。
    expect(html).not.toContain('data-session-state="idle"');
  });

  it("切换页面既不取消轮次也不再派发：另一条会话渲染后切回，状态与模型不变", () => {
    const other = "aaaaaaaa-0000-4000-8000-0000000000b9";
    const { store, gateway } = onlineStore({
      resources: resources({
        sessions: [
          session({ id: ONLINE_SESSION_ID, state: "running" }),
          session({ id: other, title: "另一条", state: "idle" }),
        ],
      }),
    });
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "turn.started",
        eventId: "dddddddd-0000-4000-8000-00000000070d",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "95",
        view: { turnId: "turn-1", state: "running" },
      }),
    });
    const before = pageOf(store);
    const dispatches = gateway.dispatches.length;

    renderPage(before);
    const otherModel = store.conversationPage(other);
    if (otherModel === null) throw new Error("另一个会话的模型不应为 null");
    renderPage(otherModel);
    const after = pageOf(store);

    // 页面切换只换渲染目标：主状态、输入可用性与渲染结果都不被改变。
    expect(after.header.state).toBe("running");
    expect(after.input.state).toBe("in_progress");
    expect(after.input.canSubmit).toBe(before.input.canSubmit);
    // 反例：切换时「顺手取消上一轮」或「重新派发」→ 下面两条会红。
    expect(gateway.dispatches.length).toBe(dispatches);
    expect(gateway.dispatches.some((dispatch) => dispatch.body.command === "session.cancel")).toBe(false);
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

  it("历史读取不再显示顶部翻页工具栏", () => {
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

    expect(html).not.toContain('data-action="load-earlier"');
    expect(html).not.toContain('data-paging="earlier"');
    expect(html).not.toContain('data-action="load-earlier-history"');
    expect(html).not.toContain('data-history-paging="earlier"');
  });
});

describe("会话头与子 Agent 数据边界", () => {
  it("节点 Agent 目录不能冒充会话子 Agent，缺少生命周期时明确未上报", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store), { hostPanel: store.hostPanel() });

    // 会话头三项：标题、状态徽标、Agent 名。
    expect(html).toContain('data-session-title="true"');
    expect(html).toContain("data-session-state=");
    expect(html).toContain('data-session-agent="Claude"');
    // 整台节点有两个 Agent，不意味着当前会话有两个存活子 Agent。
    expect(html).toContain('data-subagents="unavailable"');
    expect(html).not.toContain('data-agent-chip="agents"');
    expect(html).not.toContain('data-agent-row="codex-a"');
  });

  it("会话头是单行：标题、状态、Agent 名和子 Agent 槽位依次排列", () => {
    const fixture = filledStore("running");
    const html = renderPage(pageOf(fixture.store), { hostPanel: fixture.store.hostPanel() });
    const header = sliceBetween(html, 'data-session-header="', 'data-conversation-scroll="timeline"');

    expect(header).toContain('data-session-header-layout="single-line"');
    expect(header).toContain('data-session-title="true"');
    expect(header).toContain('data-session-state="running"');
    expect(header).toContain('data-session-agent="Claude"');
    expect(header).toContain('data-subagents="unavailable"');
    // 顺序：标题 → 状态胶囊 → Agent 名 → Agent 胶囊（弹性空白把它们推成左右两段）。
    expect(header.indexOf('data-session-title="true"')).toBeLessThan(header.indexOf('data-session-state="running"'));
    expect(header.indexOf('data-session-state="running"')).toBeLessThan(header.indexOf('data-session-agent="Claude"'));
    expect(header.indexOf('data-session-agent="Claude"')).toBeLessThan(header.indexOf('data-subagents="unavailable"'));
  });

  it("状态胶囊带字形：运行中 ●、待处理 ⏸、空闲 ○；三态词表之外没有旧七态", () => {
    const cases = [
      ["running", "●", "okSoft"],
      ["pending", "⏸", "warnSoft"],
      ["idle", "○", "panel2"],
    ] as const;
    for (const [state, glyph, tone] of cases) {
      const html = renderToStaticMarkup(createElement(StatusPill, { state }));
      expect(html, state).toContain(`data-session-state="${state}"`);
      expect(html, state).toContain(`data-session-state-glyph="${glyph}"`);
      expect(html, state).toContain(`data-session-state-tone="${tone}"`);
      expect(html, state).toContain(glyph);
    }
    const idle = renderToStaticMarkup(createElement(StatusPill, { state: "idle" }));
    // 旧标签不再有任何呈现（不是「换了个词」而是词表里没有它）。
    expect(idle).not.toContain("已关闭");
  });

  it("没有主机面板数据时不呈现胶囊（不编一份 Agent 名单）", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store));

    expect(html).not.toContain("data-agent-chip=");
  });
});

describe("页面不呈现内容来源与同步时间", () => {
  it("渲染结果里没有内容来源条、同步时刻与来源离线标注", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store));

    // 连接状态只由共享外壳的设备胶囊表达：页面正文不再有第二个状态徽标或来源条。
    expect(html).not.toContain("data-connection-strip");
    expect(html).not.toContain("data-content-origin");
    expect(html).not.toContain("data-synced-at");
    expect(html).not.toContain("内容来自上次同步");
    expect(html).not.toContain("已同步 ");
    expect(html).not.toContain("data-origin-offline");
    expect(html).not.toContain("当前离线");
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
    const header=sidebar.indexOf('data-sidebar-region="header"');
    const body=sidebar.indexOf('data-sidebar-region="body"');
    const footer=sidebar.indexOf('data-sidebar-region="footer"');
    expect(header).toBeGreaterThan(-1);
    expect(body).toBeGreaterThan(header);
    expect(footer).toBeGreaterThan(body);
    const fixedHeader=sidebar.slice(header,body);
    const scrollBody=sidebar.slice(body,footer);
    const fixedFooter=sidebar.slice(footer);
    expect(fixedHeader).toContain('data-action="open-directory-list"');
    expect(fixedHeader).toContain('data-sidebar-search="true"');
    expect(scrollBody).toContain('data-sidebar-sessions="list"');
    expect(scrollBody).not.toContain('data-sidebar-shown=');
    expect(scrollBody).not.toContain('data-action="create-session-in-directory"');
    expect(fixedFooter).toContain('data-sidebar-shown=');
    expect(fixedFooter).toContain('data-action="create-session-in-directory"');
  });

  it("左栏会话行只呈现圆点、标题和时间，不附加状态词与 Agent 名", () => {
    const { store } = onlineStore({
      resources: resources({
        sessions: [
          session({ id: ONLINE_SESSION_ID, title: "当前这条", state: "running" }),
          session({ id: "dddddddd-0000-4000-8000-0000000000b3", title: "等你回复的一条", state: "pending" }),
          session({ id: "dddddddd-0000-4000-8000-0000000000b4", title: "空闲的一条", state: "idle" }),
        ],
      }),
    });
    const html = renderPage(pageOf(store));
    const list = sliceBetween(html, 'data-sidebar-sessions="list"', 'data-sidebar-shown=');

    // 三条都是紧凑行；圆点、标题、时间仍在，待处理的那条还带状态词。
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
    expect(list.match(/data-session-state="pending"/g) ?? []).toHaveLength(1);
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

  it.each([null, "99999999-9999-4999-8999-999999999999"])("目录上下文不可用时左栏留空，不显示说明；中栏照常（%s）", (directoryId) => {
    const { store } = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, title: "没有目录", directoryId })] }),
    });
    const html = renderPage(pageOf(store));

    expect(html).not.toContain('data-sidebar-notice="no_directory"');
    expect(html).not.toContain("没有目录归属");
    expect(html).not.toContain("列不出目录上下文");
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

  it("上下文胶囊与模型/模式胶囊都在输入区里；不再额外显示告警条", () => {
    const { store } = filledStore("running");
    applyUsage(store, "188000", "200000");
    const html = renderPage(pageOf(store));
    expect(html.indexOf('data-composer-input')).toBeLessThan(html.indexOf('data-composer-bar="true"'));
    const bar = html.slice(html.indexOf('data-composer-bar="true"'));

    expect(bar).toContain('data-context-state="known"');
    expect(bar).toContain('data-settings="true"');
    expect(bar).toContain('data-settings-pill="model"');
    expect(bar).toContain('data-settings-pill="mode"');
    expect(html).not.toContain('data-context-warning="true"');
    expect(html).not.toContain("可能截断较早的内容");


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
    // 未上报不是禁用理由：会话能力与输入区都不受影响。进行中的输入区可编辑但发送禁用，
    // 同时呈现停止（spec「执行中可编辑但不可发送」；发送入口在、但闸门关着）。
    expect(html).toContain('data-can-send="true"');
    expect(html).not.toContain('data-action="submit-prompt"');
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

  it("待处理跳转条只数「本设备可应答」的交互，不读会话主状态", () => {
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

    // 会话主状态仍是 `running`，但本设备手上确有一条可应答的交互：跳转条按条数出现。
    expect(html).toContain('data-session-state="running"');
    expect(html).toContain('data-pending-jump-bar="1"');
    expect(html).toContain("待处理 1 项");
    expect(html).toContain('data-action="jump-to-pending"');

    // 对照：主状态是 `pending` 但没有可应答交互时不呈现跳转条（状态不是条数的来源）。
    const pendingOnly = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "pending" })] }),
    });
    const pendingHtml = renderPage(pageOf(pendingOnly.store));
    expect(pendingHtml).toContain('data-session-state="pending"');
    expect(pendingHtml).not.toContain("data-pending-jump-bar");
    expect(pendingHtml).not.toContain("待处理 1 项");

    // 已被其它设备处理的请求同样不计入。
    const handled = filledStore("pending");
    handled.store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "permission.requested",
        eventId: "dddddddd-0000-4000-8000-000000000709",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "99",
        view: {
          interactionId: "perm-4",
          title: "写入工作区",
          options: [{ optionId: "deny", label: "拒绝", kind: "deny" }],
        },
      }),
    });
    handled.store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "permission.resolved",
        eventId: "dddddddd-0000-4000-8000-00000000070a",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "100",
        view: { interactionId: "perm-4", resolution: "deny", resolvedByDeviceId: "another-device" },
      }),
    });
    const handledHtml = renderPage(pageOf(handled.store));
    expect(handledHtml).not.toContain("data-pending-jump-bar");
  });

  it("输入区不再显示常驻说明与额外入口", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store));

    expect(html).not.toContain('data-composer-hint="true"');
    expect(html).not.toContain('data-composer-keys="enter-submits"');
    expect(html).toContain('placeholder="输入消息，Enter 发送，Shift+Enter 换行"');
    expect(html).not.toContain('data-attachment-notice="true"');
    expect(html).toContain("添加文件不可用：");
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

describe("对话页：流式阅读保持位置", () => {
  it("距底部不超过防护值才算跟随；上读后立即停止跟随", () => {
    // 内容 1000、视口 400：offset 600 正好在底部。
    expect(
      isFollowingBottom({ offsetY: 600, contentHeight: 1000, viewportHeight: 400 }),
    ).toBe(true);
    expect(
      isFollowingBottom({
        offsetY: 600 - SCROLL_FOLLOW_GUARD_PX,
        contentHeight: 1000,
        viewportHeight: 400,
      }),
    ).toBe(true);
    // 再往上多读一像素就退出跟随：新内容不得把阅读位置拽走。
    expect(
      isFollowingBottom({
        offsetY: 600 - SCROLL_FOLLOW_GUARD_PX - 1,
        contentHeight: 1000,
        viewportHeight: 400,
      }),
    ).toBe(false);
    // 内容比视口短：没有可滚动的距离，视为在底部。
    expect(isFollowingBottom({ offsetY: 0, contentHeight: 200, viewportHeight: 400 })).toBe(true);
  });

  it("初始渲染处于跟随状态，不呈现「回到底部」入口", () => {
    const { store } = filledStore("idle");
    const html = renderPage(pageOf(store));

    expect(html).toContain('data-scroll-follow="true"');
    expect(html).not.toContain('data-action="scroll-to-bottom"');
  });
});

describe("对话页：激活失败的显式重试入口", () => {
  /** 一条 `session.resume` 的终态结果（页面只据 `retryable` 呈现重试入口）。 */
  function resumeOutcome(input: {
    readonly requestId: string;
    readonly status: "failed" | "rejected" | "uncertain";
    readonly code: ErrorCode | null;
  }): CommandResultMessage {
    return {
      protocolVersion: 1,
      type: "command.result",
      messageId: "bbbbbbbb-1111-4111-8111-0000000000fd",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
      connectionSequence: "1",
      body: {
        requestId: input.requestId,
        command: "session.resume",
        status: input.status,
        acceptedAt: "2026-10-01T00:00:01.000Z",
        terminalEventId: null,
        result: null,
        // 错误自身的 `retryable` 不参与入口判定：类别才是判据。
        error:
          input.code === null
            ? null
            : { code: input.code, message: "假主机的明确错误", retryable: true, details: {} },
      },
    };
  }

  /** 一条 owned 会话 + 协商了本机生命周期的仓库（进入即派发一次 resume）。 */
  function activationStore(): StoreFixture {
    const fixture = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "idle", localActivity: "active" })] }),
      features: [LOCAL_SESSION_LIFECYCLE_FEATURE],
    });
    fixture.store.setActiveConversationSession(ONLINE_SESSION_ID);
    return fixture;
  }

  it("激活失败不再占用会话头：只有标题、状态、Agent 与子 Agent", () => {
    const { store, gateway } = activationStore();
    const resume = lastDispatch(gateway).body;
    if (resume.command !== "session.resume") throw new Error("不是 session.resume");
    store.apply({
      kind: "command_result",
      result: resumeOutcome({ requestId: resume.requestId, status: "uncertain", code: null }),
    });

    const model = pageOf(store);
    expect(model.header.activation).toMatchObject({ kind: "failed", retryable: true });
    const html = renderPage(model);
    // 会话头不再渲染激活原因/重试入口；原因由输入区的不可提交说明承担。
    expect(html).not.toContain("data-session-activation");
    expect(html).not.toContain('data-action="retry-activation"');
    expect(html).toContain("data-session-activity");
  });

  it("Agent 不支持恢复时模型仍标记不可重试（只影响输入区原因，不影响会话头结构）", () => {
    const { store, gateway } = activationStore();
    const resume = lastDispatch(gateway).body;
    if (resume.command !== "session.resume") throw new Error("不是 session.resume");
    store.apply({
      kind: "command_result",
      result: resumeOutcome({
        requestId: resume.requestId,
        status: "rejected",
        code: "capability.unsupported_by_agent",
      }),
    });

    const model = pageOf(store);
    expect(model.header.activation).toMatchObject({ kind: "failed", retryable: false });
    const html = renderPage(model);
    expect(html).not.toContain("data-session-activation");
    expect(html).not.toContain('data-action="retry-activation"');
  });
});

describe("对话页：折叠表头是否可点只取决于页面给没给回调", () => {
  it("没有 onToggleTimelineEntry 时折叠块照旧呈现，但表头不是控件", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store));

    // 折叠块本身在（工具调用的内容仍按折叠块结构呈现）。
    expect(html).toContain("data-fold-entry=");
    // 没有回调时不得渲染按钮/折叠箭头/toggle 动作：一个点了没反应的可点控件比不可点更糟。
    expect(html).not.toContain('data-action="toggle-timeline-entry"');
  });

  it("给了 onToggleTimelineEntry 时表头是可点控件并带展开状态", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store), { onToggleTimelineEntry: () => undefined });

    expect(html).toContain('data-action="toggle-timeline-entry"');
    const toggle = html.slice(html.indexOf('data-action="toggle-timeline-entry"'));
    expect(toggle).toContain("data-entry-key=");
    expect(toggle).toContain("data-entry-expanded=");
  });
});

describe("对话页：阅读锚点的几何补偿", () => {
  it("内容在上方增长时补偿同样的像素；只在下方增长时不滚动", () => {
    // 更早历史/媒体加载把锚点向下推 120px：scrollTop 也要 +120，阅读位置才不动。
    expect(anchorScrollDelta(240, 360)).toBe(120);
    // 新内容追加在下方：锚点相对容器顶边的偏移不变，不产生滚动。
    expect(anchorScrollDelta(240, 240)).toBe(0);
    // 折叠块在上方收起：内容变短，阅读位置相应上移。
    expect(anchorScrollDelta(240, 200)).toBe(-40);
  });
});

describe("对话页：快捷收起入口的进入/退出容差", () => {
  /** 表头底边在容器顶边上方 `above` 像素（负数=已滚出）、折叠块底边在顶边下方 `body` 像素的几何。 */
  function fold(above: number, body: number): CrossingFoldGeometry {
    return { headerBottom: -above, bodyBottom: body };
  }

  it("未显示的目标按进入阈值判定：表头要真的滚出顶部，正文还要留够可见高度", () => {
    // 表头刚好滚出 `CROSSING_FOLD_ENTER_PX`：进入。
    expect(crossingFoldAtTop(fold(CROSSING_FOLD_ENTER_PX, 400), false)).toBe(true);
    // 少一像素就不进入——进入阈值不是「贴着顶部就算」。
    expect(crossingFoldAtTop(fold(CROSSING_FOLD_ENTER_PX - 1, 400), false)).toBe(false);
    // 正文露出量：恰好到下限算进入，差一像素不算（收起后标题没地方可见）。
    expect(crossingFoldAtTop(fold(80, CROSSING_FOLD_MIN_BODY_PX + CROSSING_FOLD_ENTER_PX), false)).toBe(true);
    expect(crossingFoldAtTop(fold(80, CROSSING_FOLD_MIN_BODY_PX + CROSSING_FOLD_ENTER_PX - 1), false)).toBe(false);
  });

  it("已显示的目标按更宽容的退出阈值保留：边界附近一两个像素不挂卸", () => {
    // 表头已经回到可见带里 1px：如果仍用进入阈值，这里会立刻移除入口。
    const justInside = fold(-1, 400);
    expect(crossingFoldAtTop(justInside, false)).toBe(false);
    expect(crossingFoldAtTop(justInside, true)).toBe(true);
    // 退出阈值之内保留，达到/超过才移除。
    expect(crossingFoldAtTop(fold(-(CROSSING_FOLD_EXIT_PX - 1), 400), true)).toBe(true);
    expect(crossingFoldAtTop(fold(-CROSSING_FOLD_EXIT_PX, 400), true)).toBe(false);
    // 正文侧同理：块只剩很少可见高度时，再看退出方向的容差。
    expect(crossingFoldAtTop(fold(80, CROSSING_FOLD_MIN_BODY_PX - CROSSING_FOLD_EXIT_PX + 1), true)).toBe(true);
    expect(crossingFoldAtTop(fold(80, CROSSING_FOLD_MIN_BODY_PX - CROSSING_FOLD_EXIT_PX), true)).toBe(false);
  });

  it("目标失效时彻底移除：折叠后（正文高度归零）或表头回到可见带内", () => {
    // 目标被折叠：正文高度变成 0，任何容差都不该保留入口。
    expect(crossingFoldAtTop({ headerBottom: -200, bodyBottom: 0 }, true)).toBe(false);
    // 表头回到可见带里：用户已经看得到它，快捷收起不再有意义（退出阈值之外）。
    expect(crossingFoldAtTop({ headerBottom: CROSSING_FOLD_EXIT_PX, bodyBottom: 500 }, true)).toBe(false);
  });

  it("容差只决定按钮展示，不改变「是否跟随底部」的真实几何判据", () => {
    const contentHeight = 1000;
    const viewportHeight = 400;
    // 同一时刻：折叠块仍在退出容差带里（按钮保留）。
    expect(crossingFoldAtTop({ headerBottom: -1, bodyBottom: 300 }, true)).toBe(true);
    // 但滚动位置距底部超过跟随闸门一像素——仍然**不**跟随，容差不放宽这个判据。
    expect(
      isFollowingBottom({
        offsetY: contentHeight - viewportHeight - (SCROLL_FOLLOW_GUARD_PX + 1),
        contentHeight,
        viewportHeight,
      }),
    ).toBe(false);
    // 闸门本身仍是 24px 的真实几何：正好到达时跟随，差一像素不跟随。
    expect(
      isFollowingBottom({
        offsetY: contentHeight - viewportHeight - SCROLL_FOLLOW_GUARD_PX,
        contentHeight,
        viewportHeight,
      }),
    ).toBe(true);
  });
});
