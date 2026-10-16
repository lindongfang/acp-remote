/**
 * WP6 · 对话页的宽窄两形态与跳转入口（spec「对话页的宽窄两种形态」「跳转到最近的用户输入」；
 * design D7/D10）。
 *
 * ## 判别力从哪来
 *
 * - **同一交互状态下，窄屏与宽屏渲染出完全相同的 `data-*` 集合**（键与取值都比）：窄屏只是把
 *   两侧栏目降为**可切换**的次级视图——三栏内容始终都在 DOM 里，形态差别落在样式上（两条渲染的
 *   字符串不相等即证明宽度真的换过一档，否则「集合相同」在宽度被忽略时也会绿）。四个次级视图取值
 *   逐个比一遍。
 * - **切换取值不增删信息**：同一宽度下，四个取值渲染出的 `data-*` 集合彼此相同——取值只决定
 *   **藏哪一栏**（`display: none`），不决定「渲染不渲染」。
 * - **缺省取值在宽屏等同于「全部」**：`DEFAULT_CONVERSATION_SECONDARY_VIEW` 是窄屏的默认形态，
 *   而 spec 说宽屏**呈现三栏**；这条规则由 `effectiveSecondaryView` 单独钉住（三条规则各一条断言）。
 * - **未接线切换控件时不降级**：没有 `onSecondaryViewChange` 时三栏都呈现，而不是默认藏两栏又
 *   没有切换入口。
 * - **跳转入口只在「有用户输入且一条都看不见」时呈现**：判定是纯函数（{@link classifyUserInputs} /
 *   {@link jumpPillOf}），组件测试直接喂几何；node 侧没有布局可测，因此渲染结果里**不**呈现入口
 *   ——「没有用户输入时不呈现」在这一层断言的就是它不出现。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { CONVERSATION_SECONDARY_VIEWS, DEFAULT_CONVERSATION_SECONDARY_VIEW } from "../features/conversation-model";
import type { ConversationPageModel, ConversationSecondaryView } from "../features/conversation-model";
import type { ClientStore } from "../features/client-store";
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
import {
  JUMP_VIEWPORT_GUARD_PX,
  classifyUserInputs,
  effectiveSecondaryView,
  jumpPillOf,
  jumpTargetIndexOf,
} from "./ConversationStream";
import { ConversationStream } from "./index";

/** 宽屏那一档的注入宽度（组件内部读 `breakpoints.wide` = 720）。 */
const WIDE_WIDTH = 1024;
/** 窄屏那一档的注入宽度。 */
const NARROW_WIDTH = 375;

/** 一个内容齐全的仓库：消息、结构化卡片、改动、授权、设置、降级各一份。 */
function storeOf(): StoreFixture {
  return onlineStore({
    resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "running" })] }),
    features: [SESSION_HISTORY_FEATURE],
  });
}

/** 喂入一份内容齐全的模型原料。 */
function fill(fixture: StoreFixture, messageRole: "user" | "agent" = "user"): void {
  const { store, gateway } = fixture;
  store.apply({
    kind: "event",
    event: eventMessage({
      eventType: "tool.call.started",
      eventId: "dddddddd-0000-4000-8000-000000000501",
      sessionId: ONLINE_SESSION_ID,
      globalSequence: "71",
      view: { toolCallId: "tool-1", title: "读取 src/a.ts", state: "running" },
    }),
  });
  store.apply({
    kind: "event",
    event: eventMessage({
      eventType: "file.changed",
      eventId: "dddddddd-0000-4000-8000-000000000502",
      sessionId: ONLINE_SESSION_ID,
      globalSequence: "72",
      view: {
        changeId: "11111111-0000-4000-8000-0000000000e1",
        kind: "update",
        displayPath: "src/a.ts",
        summary: "改了 2 行",
        addedLines: "1",
        deletedLines: "1",
        outsideWorkspace: false,
      },
    }),
  });
  store.apply({
    kind: "event",
    event: eventMessage({
      eventType: "permission.requested",
      eventId: "dddddddd-0000-4000-8000-000000000503",
      sessionId: ONLINE_SESSION_ID,
      globalSequence: "73",
      view: { interactionId: "perm-1", title: "运行 npm test", options: [{ optionId: "reject", label: "拒绝", kind: "reject_once" }] },
    }),
  });
  store.apply({
    kind: "event",
    event: eventMessage({
      eventType: "session.info.changed",
      eventId: "dddddddd-0000-4000-8000-000000000504",
      sessionId: ONLINE_SESSION_ID,
      globalSequence: "74",
      view: { title: "改了标题" },
    }),
  });
  store.loadSession(ONLINE_SESSION_ID);
  store.apply({
    kind: "command_result",
    result: readResult({
      requestId: lastDispatch(gateway).body.requestId,
      sessionId: ONLINE_SESSION_ID,
      messages: [messageItem({ id: "m-1", role: messageRole, createdAt: "2026-10-01T00:00:02.000Z" })],
      history: [historyItem({ eventId: "dddddddd-0000-4000-8000-000000000505", eventType: "turn.started", sessionSequence: "3", view: { turnId: "turn-1", state: "running" } })],
      hasEarlier: true,
      historyHasEarlier: true,
    }),
  });
}

/** 取对话页模型。 */
function pageOf(store: ClientStore): ConversationPageModel {
  const model = store.conversationPage(ONLINE_SESSION_ID);
  if (model === null) throw new Error("会话页模型不应为 null");
  return model;
}

/** 渲染对话页（两种形态用**同一份** props，只有宽度不同）。 */
function renderAt(
  store: ClientStore,
  availableWidth: number,
  secondaryView?: ConversationSecondaryView | null,
): string {
  return renderToStaticMarkup(
    createElement(ConversationStream, {
      model: pageOf(store),
      onLoadEarlier: () => undefined,
      onLoadEarlierHistory: () => undefined,
      onOpenDiagnostics: () => undefined,
      onAnswerInteraction: () => undefined,
      onOpenChangeDiagnostics: () => undefined,
      openChangeDiagnosticsEventId: null,
      onSecondaryViewChange: () => undefined,
      ...(secondaryView === undefined ? {} : { secondaryView }),
      availableWidth,
    }),
  );
}

/** 渲染结果里全部 `data-*` 属性（键与取值都在内），排序后便于按集合比较。 */
function dataAttributes(html: string): readonly string[] {
  return (html.match(/data-[a-zA-Z0-9-]+="[^"]*"/g) ?? []).slice().sort();
}

/** 含 `marker` 的那个标签的原文（从 `<` 到 `>`）。 */
function tagContaining(html: string, marker: string): string {
  const at = html.indexOf(marker);
  expect(at, `渲染结果里没有 ${marker}`).toBeGreaterThanOrEqual(0);
  return html.slice(html.lastIndexOf("<", at), html.indexOf(">", at) + 1);
}

describe("对话页宽窄两形态：信息集合相同", () => {
  it("同一交互状态下，窄屏与宽屏的 data-* 集合完全相同（差异只在样式），四个取值逐个比", () => {
    for (const view of CONVERSATION_SECONDARY_VIEWS) {
      const fixture = storeOf();
      fill(fixture);
      const narrow = renderAt(fixture.store, NARROW_WIDTH, view);
      const wide = renderAt(fixture.store, WIDE_WIDTH, view);

      expect(dataAttributes(narrow), `次级视图 ${view}`).toEqual(dataAttributes(wide));
      // 判别力：宽度真的换过一档（否则上面那条在宽度被忽略时也会绿）。
      expect(narrow, `次级视图 ${view}`).not.toBe(wide);
    }
  });

  it("三栏在两档下都在：同目录会话栏、时间线、变更文件栏同时呈现", () => {
    const fixture = storeOf();
    fill(fixture);
    const wide = renderAt(fixture.store, WIDE_WIDTH, "all");
    const narrow = renderAt(fixture.store, NARROW_WIDTH, "all");

    for (const html of [wide, narrow]) {
      expect(html).toContain('data-conversation-column="sessions"');
      expect(html).toContain('data-conversation-column="timeline"');
      expect(html).toContain('data-conversation-column="files"');
      expect(html).toContain('data-timeline="entries"');
      expect(html).toContain('data-file-changes="list"');
      expect(html).toContain('data-composer="in_progress"');
      expect(html).toContain('data-settings="true"');
      expect(html).toContain('data-capabilities="true"');
    }
  });

  it("切换次级视图只改样式：四个取值下三栏锚点一个不少，且 data-* 集合彼此相同", () => {
    const fixture = storeOf();
    fill(fixture);
    const rendered = CONVERSATION_SECONDARY_VIEWS.map((view) => ({
      view,
      html: renderAt(fixture.store, NARROW_WIDTH, view),
    }));

    for (const { view, html } of rendered) {
      // 反例：把未选中的栏从 DOM 里删掉（而不是 `display: none`）→ 下面三条会红。
      expect(html, `次级视图 ${view}`).toContain('data-conversation-column="sessions"');
      expect(html, `次级视图 ${view}`).toContain('data-conversation-column="timeline"');
      expect(html, `次级视图 ${view}`).toContain('data-conversation-column="files"');
      expect(html, `次级视图 ${view}`).toContain('data-change-path="src/a.ts"');
    }

    const baseline = dataAttributes(rendered[0]?.html ?? "");
    for (const { view, html } of rendered) {
      expect(dataAttributes(html), `次级视图 ${view} 与 ${rendered[0]?.view ?? "?"} 的信息集合`).toEqual(baseline);
    }
    // 取值本身确实变了：选中的那一档随取值走（否则「集合相同」在取值被忽略时也会绿）。
    expect(tagContaining(rendered[2]?.html ?? "", 'data-segment-option="sessions"')).toContain('data-segment-selected="true"');
    expect(tagContaining(rendered[2]?.html ?? "", 'data-segment-option="timeline"')).toContain('data-segment-selected="false"');
  });

  it("窄屏缺省选「对话」：三栏锚点仍在，且切换控件就在旁边（四个选项都在）", () => {
    const fixture = storeOf();
    const wideFixture = storeOf();
    fill(fixture);
    const html = renderAt(fixture.store, NARROW_WIDTH);

    expect(html).toContain('data-segment="conversation-secondary-view"');
    for (const view of CONVERSATION_SECONDARY_VIEWS) {
      expect(html, `选项 ${view}`).toContain(`data-segment-option="${view}"`);
    }
    expect(tagContaining(html, 'data-segment-option="timeline"')).toContain('data-segment-selected="true"');
    // 反例：窄屏把变更文件从 DOM 里删掉 → 下面三条会红（那是「静默丢弃信息」）。
    expect(html).toContain('data-conversation-column="files"');
    expect(html).toContain('data-change-path="src/a.ts"');
    expect(html).toContain(`data-sidebar-session="${ONLINE_SESSION_ID}"`);
    // 宽度不改变选项集合：同一模型在宽屏下也是这四项（宽屏缺省即三栏，因此选中的是「全部」）。
    fill(wideFixture);
    const wide = renderAt(wideFixture.store, WIDE_WIDTH);
    expect(wide).toContain('data-segment-option="files"');
    expect(wide).toContain('data-segment-option="sessions"');
    expect(tagContaining(wide, 'data-segment-option="all"')).toContain('data-segment-selected="true"');
  });

  it("切到「变更文件」只改样式：三栏的锚点都在，选中项随交互状态走", () => {
    const fixture = storeOf();
    fill(fixture);
    const onTimeline = renderAt(fixture.store, NARROW_WIDTH, "timeline");
    const onFiles = renderAt(fixture.store, NARROW_WIDTH, "files");

    // 选中的档不同（交互状态真的变了）。
    expect(tagContaining(onTimeline, 'data-segment-option="timeline"')).toContain('data-segment-selected="true"');
    expect(tagContaining(onFiles, 'data-segment-option="files"')).toContain('data-segment-selected="true"');
    // 两次渲染的信息集合仍然相同（切换只改样式，不增删信息）。
    expect(dataAttributes(onFiles)).toEqual(dataAttributes(onTimeline));
    // 变更文件栏的样式确实换过（切换有效果），但锚点没变。
    expect(tagContaining(onFiles, 'data-conversation-column="files"')).not.toEqual(
      tagContaining(onTimeline, 'data-conversation-column="files"'),
    );
    expect(dataAttributes(onFiles).filter((attribute) => attribute.startsWith("data-change-path="))).toEqual(
      dataAttributes(onTimeline).filter((attribute) => attribute.startsWith("data-change-path=")),
    );
  });

  it("未接线切换控件时不降级：三栏都呈现（不出现「藏了栏又没有入口」）", () => {
    const fixture = storeOf();
    fill(fixture);
    for (const width of [NARROW_WIDTH, WIDE_WIDTH]) {
      const html = renderToStaticMarkup(
        createElement(ConversationStream, {
          model: pageOf(fixture.store),
          onLoadEarlier: () => undefined,
          onOpenDiagnostics: () => undefined,
          availableWidth: width,
        }),
      );

      expect(html, `宽度 ${width}`).not.toContain('data-segment="conversation-secondary-view"');
      expect(html, `宽度 ${width}`).toContain('data-conversation-column="sessions"');
      expect(html, `宽度 ${width}`).toContain('data-conversation-column="timeline"');
      expect(html, `宽度 ${width}`).toContain('data-conversation-column="files"');
      expect(html, `宽度 ${width}`).toContain('data-change-path="src/a.ts"');
    }
  });
});

describe("生效的次级视图：三条规则", () => {
  it("没有切换控件时三栏都呈现", () => {
    expect(effectiveSecondaryView({ requested: "timeline", switchable: false, wide: true })).toBe("all");
    expect(effectiveSecondaryView({ requested: null, switchable: false, wide: false })).toBe("all");
  });

  it("宽屏 + 缺省取值 = 三栏（spec：宽屏 SHALL 呈现三栏）", () => {
    expect(effectiveSecondaryView({ requested: null, switchable: true, wide: true })).toBe("all");
    expect(effectiveSecondaryView({ requested: DEFAULT_CONVERSATION_SECONDARY_VIEW, switchable: true, wide: true })).toBe("all");
    // 反例：把缺省取值原样当成宽屏的取值 → 上面两条会得到 `timeline`（只剩一栏）。
  });

  it("窄屏 + 缺省取值 = 对话本体；用户显式选过别的取值后两档都照办", () => {
    expect(effectiveSecondaryView({ requested: null, switchable: true, wide: false })).toBe(DEFAULT_CONVERSATION_SECONDARY_VIEW);
    expect(effectiveSecondaryView({ requested: DEFAULT_CONVERSATION_SECONDARY_VIEW, switchable: true, wide: false })).toBe("timeline");
    for (const view of ["all", "sessions", "files"] as const) {
      expect(effectiveSecondaryView({ requested: view, switchable: true, wide: false })).toBe(view);
      expect(effectiveSecondaryView({ requested: view, switchable: true, wide: true })).toBe(view);
    }
  });
});

describe("跳转入口的判定（纯函数）", () => {
  it("没有用户输入时不呈现", () => {
    expect(jumpPillOf(classifyUserInputs([], 800))).toBeNull();
    expect(jumpPillOf({ visible: 0, above: [], below: [] })).toBeNull();
    // 反例：无条件呈现（例如把方向固定成 up、条数取 1）→ 上面两条会红。
  });

  it("有用户输入但一条都看不到时呈现，方向与条数都来自几何", () => {
    const above = classifyUserInputs(
      [
        { top: -300, bottom: -200 },
        { top: -100, bottom: -20 },
      ],
      800,
    );
    expect(above).toEqual({ visible: 0, above: [0, 1], below: [] });
    expect(jumpPillOf(above)).toEqual({ direction: "up", remaining: 2 });
    // 目标取「最近的一条」：上方的最后一条。
    expect(jumpTargetIndexOf(above)).toBe(1);

    const below = classifyUserInputs([{ top: 900, bottom: 1000 }], 800);
    expect(jumpPillOf(below)).toEqual({ direction: "down", remaining: 1 });
    expect(jumpTargetIndexOf(below)).toBe(0);
  });

  it("至少一条在视口内时不呈现（全部可见或部分可见都一样）", () => {
    const partially = classifyUserInputs(
      [
        { top: -300, bottom: -200 },
        { top: 100, bottom: 220 },
      ],
      800,
    );
    expect(partially.visible).toBe(1);
    expect(jumpPillOf(partially)).toBeNull();
    expect(jumpPillOf(classifyUserInputs([{ top: 10, bottom: 120 }], 800))).toBeNull();
  });

  it("贴着视口边界的条目不重复计入两侧（容差与原型同一口径）", () => {
    const edge = classifyUserInputs(
      [
        { top: -50, bottom: JUMP_VIEWPORT_GUARD_PX },
        { top: 800 - JUMP_VIEWPORT_GUARD_PX, bottom: 900 },
      ],
      800,
    );
    expect(edge).toEqual({ visible: 0, above: [0], below: [1] });
    expect(JUMP_VIEWPORT_GUARD_PX).toBe(10);
  });
});

describe("跳转入口在渲染结果里的形态", () => {
  it("没有用户输入时不呈现（node 侧没有布局可测，因此不猜一个「在视口外」）", () => {
    const fixture = storeOf();
    fill(fixture, "agent");
    const html = renderAt(fixture.store, NARROW_WIDTH, "timeline");

    expect(html).not.toContain("data-jump-pill");
    // 反例：无条件画一个胶囊（例如固定方向与条数）→ 上面这条会红。
    expect(html).toContain('data-timeline="entries"');
  });

  it("有用户输入也不会凭空呈现：没有测量结果就没有入口", () => {
    const fixture = storeOf();
    fill(fixture, "user");
    const html = renderAt(fixture.store, NARROW_WIDTH, "timeline");

    expect(html).toContain('data-message-role="user"');
    expect(html).not.toContain("data-jump-pill");
  });
});
