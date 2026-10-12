/**
 * WP5 · 对话页整页渲染（spec 的呈现层落点；tasks 5.5）。
 *
 * ## 为什么是「页面正文」而不是挂路由组件
 *
 * 路由文件（`app/session/[id].tsx`）只做四件事：取参数、取 store、取模型、把交互写回仓库——
 * 它的页面内容全部由 `ConversationStream` 渲染（本用例渲染的就是它）。**页面文件本身**的挂载
 * 渲染由浏览器用例负责（`clients/app/e2e/**` 用真实路由组件 + 真实组合根），
 * `src/**` 的单测**不得** import `app/**`：`src/layers.test.ts` 把「跨层反向 import」作为
 * 硬判据（组件层不得依赖页面层）。
 *
 * 因此这里钉住的是页面正文的三件事：
 *
 * 1. **整页锚点齐全**：时间线、输入区、设置、右栏、能力脚注、次级视图切换全部出现在同一份渲染里；
 * 2. **页面只是模型的函数**：同一份代码在「已关闭」与「运行中」两种快照下渲染出不同的输入区状态；
 *    「快照里没有这个会话」在模型层是 `null`（页面据此呈现明确说明，而不是伪造空数据）；
 * 3. **渲染不派发命令**：渲染期间网关没有任何新的派发记录（读取在 `useEffect` 里、命令只在交互里上抛）。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ConversationStream, RuntimeUnavailable } from "./index";
import type { ConversationPageModel } from "../features/conversation-model";
import type { ClientStore } from "../features/client-store";
import type { SessionSummary } from "../protocol";
import { SESSION_HISTORY_FEATURE } from "../sync-client";
import {
  ONLINE_SESSION_ID,
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
function renderPage(model: ConversationPageModel): string {
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
      availableWidth: 1024,
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

describe("对话页正文：整页锚点", () => {
  it("一页里时间线、输入区、设置、右栏、能力脚注与次级视图切换都在", () => {
    const { store } = filledStore("running");
    const html = renderPage(pageOf(store));

    expect(html).toContain('data-route="session"');
    expect(html).toContain(`data-session-id="${ONLINE_SESSION_ID}"`);
    expect(html).toContain("data-session-header=");
    expect(html).toContain('data-conversation-page="true"');
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
    expect(html).toContain('data-segment="conversation-secondary-view"');
    expect(html).toContain('data-capabilities="true"');
    expect(html).toContain('data-can-send="true"');
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
    expect(unavailable).toContain("同步运行时未装配");
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
