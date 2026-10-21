/**
 * 会话头标题的渲染面（`design.md` D4；PWA spec「对话页顶部标题支持编辑」）。
 *
 * 组件是纯呈现层：双击、草稿与提交状态都在仓库里，这里只断言「模型 → 标记」——
 * 普通文本与编辑框互斥、编辑框带占位/阶段标记、错误与不可用原因出现在会话头内，
 * 且没有任何额外的编辑/恢复按钮。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ClientStore } from "../features/client-store";
import type { ConversationPageModel } from "../features/conversation-model";
import type { StoreFixture } from "../features/testing/fixtures";
import { onlineStore, resources, session } from "../features/testing/fixtures";
import { SESSION_TITLE_FEATURE } from "../sync-client";
import type { ConversationStreamProps } from "./index";
import { ConversationStream } from "./index";

const SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000001";

function titleStore(input: { readonly title?: string | null; readonly features?: readonly string[] } = {}): StoreFixture {
  return onlineStore({
    resources: resources({
      sessions: [session({ id: SESSION_ID, title: input.title === undefined ? "有效标题" : input.title })],
    }),
    features: input.features ?? [SESSION_TITLE_FEATURE],
  });
}

function pageOf(store: ClientStore): ConversationPageModel {
  const model = store.conversationPage(SESSION_ID);
  if (model === null) throw new Error("会话页模型不应为 null：快照里有该会话");
  return model;
}

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

describe("会话头标题的两种形态", () => {
  it("只读：标题是普通文本，带双击入口标记，没有输入框或额外按钮", () => {
    const { store } = titleStore();
    const html = renderPage(pageOf(store));
    expect(html).toContain('data-session-title="true"');
    expect(html).toContain('data-session-title-entry="double-click"');
    expect(html).not.toContain("data-session-title-input");
    // 没有编辑/恢复入口：唯一的标题相关动作就是双击文本本身。
    expect(html).not.toContain('data-action="edit-title"');
    expect(html).not.toContain("恢复 Agent 标题");
  });

  it("编辑中：渲染空输入框，旧标题只作为灰色占位", () => {
    const { store } = titleStore();
    store.beginTitleEdit(SESSION_ID);
    const html = renderPage(pageOf(store), { titleEditor: store.titleEditorView(SESSION_ID) });
    expect(html).toContain('data-session-title-input="true"');
    expect(html).toContain('data-session-title-phase="editing"');
    expect(html).toContain('placeholder="有效标题"');
    expect(html).not.toContain('value="有效标题"');
    expect(html).not.toContain('data-session-title="true"');
  });

  it("未命名会话：编辑框为空，占位不是真实输入", () => {
    const { store } = titleStore({ title: null });
    store.beginTitleEdit(SESSION_ID);
    const view = store.titleEditorView(SESSION_ID);
    expect(view?.draft).toBe("");
    const html = renderPage(pageOf(store), { titleEditor: view });
    expect(html).toContain('placeholder="未命名会话"');
    expect(html).not.toContain('value="未命名会话"');
  });

  it("非法输入的错误显示在会话头内，并保留输入框", () => {
    const { store } = titleStore();
    store.beginTitleEdit(SESSION_ID);
    store.setTitleDraft(SESSION_ID, "x".repeat(513));
    store.commitTitleEdit(SESSION_ID, "enter");
    const html = renderPage(pageOf(store), { titleEditor: store.titleEditorView(SESSION_ID) });
    expect(html).toContain('data-session-title-error="true"');
    expect(html).toContain("512");
    expect(html).toContain('data-session-title-input="true"');
  });

  it("离线/未协商：会话头明确说明不可用，没有输入框", () => {
    const { store } = titleStore({ features: [] });
    store.beginTitleEdit(SESSION_ID);
    const html = renderPage(pageOf(store), { titleEditor: store.titleEditorView(SESSION_ID) });
    expect(html).toContain('data-session-title-notice="true"');
    expect(html).toContain("core.session-title.v1");
    expect(html).not.toContain("data-session-title-input");
  });
});
