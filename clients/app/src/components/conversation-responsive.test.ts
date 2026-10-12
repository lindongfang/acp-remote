/**
 * WP5 · 对话页的宽窄两形态（spec「对话页的宽窄两种形态」；design D7/D10）。
 *
 * ## 判别力从哪来
 *
 * - **同一交互状态下，窄屏与宽屏渲染出完全相同的 `data-*` 集合**（键与取值都比）：窄屏只是把
 *   变更文件降为**可切换**的次级视图——两列内容始终都在 DOM 里，形态差别落在样式上（两条渲染的
 *   字符串不相等即证明宽度真的换过一档，否则「集合相同」在宽度被忽略时也会绿）。
 * - **默认值不丢信息**：窄屏缺省选「对话」时，变更文件那一列**仍然在 DOM 里**（只是被样式隐藏），
 *   且切换控件就在旁边——这是「不得因窄屏静默丢弃信息」的可断言形式。
 * - **未接线切换控件时不降级**：没有 `onSecondaryViewChange` 时两列都呈现，而不是默认藏一列又
 *   没有切换入口。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ConversationStream } from "./index";
import type { ConversationSecondaryView } from "./index";
import type { ConversationPageModel } from "../features/conversation-model";
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

/** 宽屏那一档的注入宽度（组件内部断点是 720）。 */
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
function fill(fixture: StoreFixture): void {
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
      messages: [messageItem({ id: "m-1", createdAt: "2026-10-01T00:00:02.000Z" })],
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
  it("同一交互状态下，窄屏与宽屏的 data-* 集合完全相同（差异只在样式）", () => {
    const fixture = storeOf();
    fill(fixture);
    const narrow = renderAt(fixture.store, NARROW_WIDTH, "both");
    const wide = renderAt(fixture.store, WIDE_WIDTH, "both");

    expect(dataAttributes(narrow)).toEqual(dataAttributes(wide));
    // 判别力：宽度真的换过一档（否则上面那条在宽度被忽略时也会绿）。
    expect(narrow).not.toBe(wide);
  });

  it("两列在两档下都在：时间线列与变更文件列同时呈现", () => {
    const fixture = storeOf();
    fill(fixture);
    const wide = renderAt(fixture.store, WIDE_WIDTH, "both");
    const narrow = renderAt(fixture.store, NARROW_WIDTH, "both");

    for (const html of [wide, narrow]) {
      expect(html).toContain('data-conversation-column="timeline"');
      expect(html).toContain('data-conversation-column="files"');
      expect(html).toContain('data-timeline="entries"');
      expect(html).toContain('data-file-changes="list"');
      expect(html).toContain('data-composer="in_progress"');
      expect(html).toContain('data-settings="true"');
      expect(html).toContain('data-capabilities="true"');
    }
  });

  it("窄屏缺省选「对话」：变更文件仍然在 DOM 里，且旁边就是切换控件", () => {
    const fixture = storeOf();
    const wideFixture = storeOf();
    fill(fixture);
    const html = renderAt(fixture.store, NARROW_WIDTH);

    // 切换控件在（它就是「次级视图」的入口），且选项集合与宽屏同一份。
    expect(html).toContain('data-segment="conversation-secondary-view"');
    expect(html).toContain('data-segment-option="timeline"');
    expect(html).toContain('data-segment-option="files"');
    expect(html).toContain('data-segment-option="both"');
    // 反例：窄屏把变更文件从 DOM 里删掉 → 下面三条会红（那是「静默丢弃信息」）。
    expect(html).toContain('data-conversation-column="files"');
    expect(html).toContain('data-change-path="src/a.ts"');
    expect(html).toContain('data-change-added-lines="1"');
    // 宽度不改变选项集合：同一模型在宽屏下也是这三项。
    fill(wideFixture);
    expect(renderAt(wideFixture.store, WIDE_WIDTH)).toContain('data-segment-option="files"');
  });

  it("切到「变更文件」只改样式：两列的锚点都在，选中项随交互状态走", () => {
    const fixture = storeOf();
    fill(fixture);
    const onTimeline = renderAt(fixture.store, NARROW_WIDTH, "timeline");
    const onFiles = renderAt(fixture.store, NARROW_WIDTH, "files");

    // 选中的档不同（交互状态真的变了）。
    expect(tagContaining(onTimeline, 'data-segment-option="timeline"')).toContain('data-segment-selected="true"');
    expect(tagContaining(onFiles, 'data-segment-option="files"')).toContain('data-segment-selected="true"');
    // 两次渲染的信息集合仍然相同（切换只改样式，不增删信息）。
    expect(dataAttributes(onFiles)).toEqual(dataAttributes(onTimeline));
    // 变更文件列的样式确实换过（切换有效果），但锚点没变。
    expect(tagContaining(onFiles, 'data-conversation-column="files"')).not.toEqual(
      tagContaining(onTimeline, 'data-conversation-column="files"'),
    );
    expect(dataAttributes(onFiles).filter((attribute) => attribute.startsWith("data-change-path="))).toEqual(
      dataAttributes(onTimeline).filter((attribute) => attribute.startsWith("data-change-path=")),
    );
  });

  it("未接线切换控件时不降级：两列都呈现（不出现「藏了一列又没有入口」）", () => {
    const fixture = storeOf();
    fill(fixture);
    const html = renderToStaticMarkup(
      createElement(ConversationStream, {
        model: pageOf(fixture.store),
        onLoadEarlier: () => undefined,
        onOpenDiagnostics: () => undefined,
        availableWidth: NARROW_WIDTH,
      }),
    );

    expect(html).not.toContain('data-segment="conversation-secondary-view"');
    expect(html).toContain('data-conversation-column="timeline"');
    expect(html).toContain('data-conversation-column="files"');
    expect(html).toContain('data-change-path="src/a.ts"');
  });
});
