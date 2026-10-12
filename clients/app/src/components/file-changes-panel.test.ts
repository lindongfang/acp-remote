/**
 * WP5 · 变更文件右栏（spec「变更文件的呈现边界」；design D8/D10）。
 *
 * ## 判别力从哪来
 *
 * - **不含差异正文**：默认渲染里既没有逐行锚点（`data-diff-line` 之类），也没有任何形式的内容
 *   ——断言的是「原文块根本不在 DOM 里」：`data-raw-json` 计数为 0，且 `rawJson` 原文串**一次都不出现**。
 * - **原文只做诊断入口**：点开的入口是 `data-action="open-file-change-diagnostics"`，展开后原文
 *   以**单块不可信文本**出现（`data-raw-json="available"`，整串原样、没有被切成行）。
 *   用一段「看起来像差异」的原文（JSON 数组里带 `+/−` 文本）证明它没有被解析：
 *   渲染结果里仍然没有逐行锚点，只有那一整块文本。
 * - **判定不出的统计省略而不是填零**：`addedLines`/`deletedLines` 缺席时渲染结果里没有
 *   `data-change-added-lines`，也没有 `+0`/`-0`。
 * - **原文未下发时没有原文槽位**：只有原因与元数据（R18 同一条纪律）。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ConversationStream } from "./index";
import type { ConversationPageModel } from "../features/conversation-model";
import type { ClientStore } from "../features/client-store";
import type { RawAcp } from "../protocol";
import { ONLINE_SESSION_ID, eventMessage, onlineStore, resources, session } from "../features/testing/fixtures";
import type { StoreFixture } from "../features/testing/fixtures";

/** 一段「看起来像差异」的 ACP 原文：若实现解析它，渲染结果里会出现逐行结构。 */
const RAW_DIFF = '[{"type":"add","line":12,"text":"+const idleTimeout = 30000"},{"type":"remove","line":11,"text":"-const T = 30000"}]';

/** 一个已在线、快照里有目标会话的仓库。 */
function storeOf(): StoreFixture {
  return onlineStore({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "idle" })] }) });
}

/** 取对话页模型。 */
function pageOf(store: ClientStore): ConversationPageModel {
  const model = store.conversationPage(ONLINE_SESSION_ID);
  if (model === null) throw new Error("会话页模型不应为 null");
  return model;
}

/** 渲染对话页右栏（可注入「哪一处改动的诊断已展开」）。 */
function renderPage(store: ClientStore, openDiagnosticsEventId: string | null = null): string {
  return renderToStaticMarkup(
    createElement(ConversationStream, {
      model: pageOf(store),
      onLoadEarlier: () => undefined,
      onOpenDiagnostics: () => undefined,
      onOpenChangeDiagnostics: () => undefined,
      openChangeDiagnosticsEventId: openDiagnosticsEventId,
      // 宽屏：右栏与时间线并列（窄屏的形态见 `conversation-responsive.test.ts`）。
      availableWidth: 1024,
      secondaryView: "both",
    }),
  );
}

/** 注入一条 `file.changed`。 */
function applyFileChanged(
  store: ClientStore,
  input: {
    readonly eventId: string;
    readonly view: Record<string, unknown>;
    readonly acp?: RawAcp;
  },
): void {
  store.apply({
    kind: "event",
    event: eventMessage({
      eventType: "file.changed",
      eventId: input.eventId,
      sessionId: ONLINE_SESSION_ID,
      globalSequence: "61",
      view: input.view,
      ...(input.acp === undefined ? {} : { acp: input.acp }),
    }),
  });
}

/** 含 `marker` 的那个标签的原文（从 `<` 到 `>`）。 */
function tagContaining(html: string, marker: string): string {
  const at = html.indexOf(marker);
  expect(at, `渲染结果里没有 ${marker}`).toBeGreaterThanOrEqual(0);
  return html.slice(html.lastIndexOf("<", at), html.indexOf(">", at) + 1);
}

describe("变更文件右栏：只有结构化统计与相对化路径", () => {
  it("呈现展示路径、类型、摘要与行数统计，并给每条改动一个诊断入口", () => {
    const { store } = storeOf();
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000401",
      view: {
        changeId: "11111111-0000-4000-8000-0000000000d1",
        kind: "update",
        displayPath: "src/auth/session.ts",
        summary: "替换 1 行",
        addedLines: "2",
        deletedLines: "1",
        outsideWorkspace: false,
      },
      acp: { mediaType: "application/json", rawJson: RAW_DIFF, byteLength: "120", sha256: "a".repeat(43) },
    });
    const html = renderPage(store);

    expect(html).toContain('data-file-changes="list"');
    expect(html).toContain('data-file-change="11111111-0000-4000-8000-0000000000d1"');
    expect(html).toContain('data-change-path="src/auth/session.ts"');
    expect(html).toContain("src/auth/session.ts");
    expect(html).toContain('data-change-kind="update"');
    expect(html).toContain('data-change-summary="true"');
    expect(html).toContain('data-change-added-lines="2"');
    expect(html).toContain('data-change-deleted-lines="1"');
    expect(html).toContain('data-change-outside-workspace="false"');
    expect(html).toContain('data-action="open-file-change-diagnostics"');
    expect(html).toContain('data-event-id="dddddddd-0000-4000-8000-000000000401"');
    expect(html).toContain("查看 ACP 原文（诊断）");

    // 列表语义：容器 `role="list"` → `<ul>`，每条改动 `role="listitem"` → `<li>`。
    expect(tagContaining(html, 'data-file-change-list="changes"').startsWith("<ul")).toBe(true);
    expect(tagContaining(html, 'data-file-change="11111111-0000-4000-8000-0000000000d1"').startsWith("<li")).toBe(true);
  });

  it("右栏不含差异正文：原文块与原文串在默认渲染里都不出现", () => {
    const { store } = storeOf();
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000402",
      view: {
        changeId: "11111111-0000-4000-8000-0000000000d2",
        kind: "update",
        displayPath: "src/auth/session.ts",
        summary: "替换 1 行",
        addedLines: "2",
        deletedLines: "1",
        outsideWorkspace: false,
      },
      acp: { mediaType: "application/json", rawJson: RAW_DIFF, byteLength: "120", sha256: "a".repeat(43) },
    });
    const html = renderPage(store);

    // 反例：为了填满右栏去解析原文（或把原文当差异渲染）时，下面每一条都会变红。
    expect(html.match(/data-raw-json=/g) ?? []).toHaveLength(0);
    expect(html).not.toContain("const idleTimeout");
    expect(html).not.toContain("data-diff-line");
    expect(html).not.toContain("data-diff-body");
    // 统计范围的说明必须在（「统计只覆盖 Agent 声明的改动」这句是如实的副作用说明）。
    expect(html).toContain('data-change-statistics-scope="true"');
  });

  it("展开诊断：原文以单块不可信文本出现（没有被切成差异行）", () => {
    const { store } = storeOf();
    const eventId = "dddddddd-0000-4000-8000-000000000403";
    applyFileChanged(store, {
      eventId,
      view: {
        changeId: "11111111-0000-4000-8000-0000000000d3",
        kind: "update",
        displayPath: "src/auth/session.ts",
        summary: "替换 1 行",
        addedLines: "2",
        deletedLines: "1",
        outsideWorkspace: false,
      },
      acp: { mediaType: "application/json", rawJson: RAW_DIFF, byteLength: "120", sha256: "a".repeat(43) },
    });
    const html = renderPage(store, eventId);

    expect(html).toContain(`data-change-diagnostics="${eventId}"`);
    // 原文整串只出现一次，且只以「原文块」的身份出现。
    expect(html.match(/data-raw-json="available"/g) ?? []).toHaveLength(1);
    expect(html.match(/const idleTimeout/g) ?? []).toHaveLength(1);
    // 反例：解析成差异体验证（每行一个元素）→ 这里会出现行锚点。
    expect(html).not.toContain("data-diff-line");
    // 元数据在（诊断需要它们）。
    expect(html).toContain('data-diagnostics-event-type="file.changed"');
  });

  it("原文未下发：只给原因与元数据，没有原文槽位", () => {
    const { store } = storeOf();
    const eventId = "dddddddd-0000-4000-8000-000000000404";
    applyFileChanged(store, {
      eventId,
      view: {
        changeId: "11111111-0000-4000-8000-0000000000d4",
        kind: "update",
        displayPath: "src/auth/config.ts",
        summary: "改了默认值",
        addedLines: "1",
        deletedLines: "1",
        outsideWorkspace: false,
      },
      acp: {
        mediaType: "application/json",
        rawUnavailable: { reason: "size_limit", byteLength: "281344", sha256: "b".repeat(43) },
      },
    });
    const html = renderPage(store, eventId);

    expect(html).toContain('data-raw-unavailable="size_limit"');
    expect(html).toContain("281344");
    // 反例：渲染占位原文假装完整 → 这里会出现原文块。
    expect(html).not.toContain('data-raw-json="available"');
  });
});

describe("变更文件右栏：判定不出的统计省略而不是填零", () => {
  it("没有行数统计时不渲染加/减标记，也不写 +0/-0", () => {
    const { store } = storeOf();
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000405",
      view: {
        changeId: "11111111-0000-4000-8000-0000000000d5",
        kind: "update",
        displayPath: "src/auth/session.ts",
        summary: "改了但没声明行数",
        outsideWorkspace: false,
      },
    });
    const html = renderPage(store);

    expect(html).toContain('data-change-line-count-unavailable="true"');
    expect(html).not.toContain("data-change-added-lines");
    expect(html).not.toContain("data-change-deleted-lines");
    expect(html).not.toContain(">+0<");
    expect(html).not.toContain(">-0<");
  });

  it("工作区之外的改动用标注说明（不是悄悄丢掉）", () => {
    const { store } = storeOf();
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000406",
      view: {
        changeId: "11111111-0000-4000-8000-0000000000d6",
        kind: "create",
        displayPath: "../outside/notes.md",
        summary: "写到工作区之外",
        addedLines: "3",
        deletedLines: "0",
        outsideWorkspace: true,
      },
    });
    const html = renderPage(store);

    expect(html).toContain('data-change-outside-workspace="true"');
    expect(html).toContain("工作区之外");
    expect(html).toContain('data-change-path="../outside/notes.md"');
  });

  it("没有改动时给出空态说明", () => {
    const { store } = storeOf();
    const html = renderPage(store);

    expect(html).toContain('data-file-changes="empty"');
    expect(html).toContain('data-file-change-count="0"');
    expect(html).toContain("还没有已呈现的文件改动");
  });
});
