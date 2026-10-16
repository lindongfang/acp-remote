/**
 * WP7 · 变更文件右栏（spec「变更文件的呈现边界」；design D6/D8/D10）。
 *
 * ## 判别力从哪来
 *
 * - **差异区只有三态，且互不冒充**：有结构化差异行 → `data-diff-region="lines"` + 逐行
 *   `data-diff-line-kind`；有不可用标记 → `data-diff-unavailable="<reason>"` + `FILE_DIFF_UNAVAILABLE_LABELS`
 *   里该原因的已登记文案；两者都没有 → `data-diff-not-delivered="true"` 的明确说明，且**不**渲染
 *   `data-diff-region`（不画占位差异行假装完整）。
 * - **不含 ACP 原文**：一段「看起来像差异」的原文（JSON 数组里带 `+/−` 文本）在默认渲染里既不成
 *   为逐行锚点、也不出现在任何位置（`rawJson` 原文串一次都不出现）。
 * - **原文只做诊断入口**：点开的入口是 `data-action="open-file-change-diagnostics"`，展开后原文
 *   以**单块不可信文本**出现（`data-raw-json="available"`，整串原样、没有被切成行）。
 * - **行内容按不可信文本处理**：差异行里的标签与链接只作为**文本**出现，不产生元素、不写地址属性。
 * - **判定不出的统计省略而不是填零**：`addedLines`/`deletedLines` 缺席时渲染结果里没有
 *   `data-change-added-lines`，也没有 `+0`/`-0`。
 * - **选中项与时间线条目共用同一个值**：`selectedChangeId` 命中的那一行带
 *   `data-file-change-selected="true"`，其余行为 `false`。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ConversationStream } from "./index";
import type { ConversationStreamProps } from "./index";
import type { ConversationPageModel } from "../features/conversation-model";
import type { ClientStore } from "../features/client-store";
import { FILE_DIFF_UNAVAILABLE_LABELS } from "../domain";
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

/** 渲染对话页右栏（可注入诊断展开值、选中项与回调）。 */
function renderPage(store: ClientStore, extra: Partial<ConversationStreamProps> = {}): string {
  return renderToStaticMarkup(
    createElement(ConversationStream, {
      model: pageOf(store),
      onLoadEarlier: () => undefined,
      onOpenDiagnostics: () => undefined,
      onOpenChangeDiagnostics: () => undefined,
      // 宽屏：三栏并列（窄屏的形态见 `conversation-responsive.test.ts`）。
      availableWidth: 1024,
      secondaryView: "all",
      ...extra,
    }),
  );
}

/** 注入事件用的投递序号：同一仓库里多次注入要各不相同（`eventId` 各异，序号按到达次序递增）。 */
let eventSequence = 61;

/** 注入一条 `file.changed`。 */
function applyFileChanged(
  store: ClientStore,
  input: {
    readonly eventId: string;
    readonly view: Record<string, unknown>;
    readonly acp?: RawAcp;
  },
): void {
  eventSequence += 1;
  store.apply({
    kind: "event",
    event: eventMessage({
      eventType: "file.changed",
      eventId: input.eventId,
      sessionId: ONLINE_SESSION_ID,
      globalSequence: String(eventSequence),
      view: input.view,
      ...(input.acp === undefined ? {} : { acp: input.acp }),
    }),
  });
}

/** 一条改动的视图（只有结构化统计与相对化展示路径）。 */
function changeView(changeId: string, extra: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    changeId,
    kind: "update",
    displayPath: "src/auth/session.ts",
    summary: "替换 1 行",
    addedLines: "2",
    deletedLines: "1",
    outsideWorkspace: false,
    ...extra,
  };
}

/** 含 `marker` 的那个标签的原文（从 `<` 到 `>`）。 */
function tagContaining(html: string, marker: string): string {
  const at = html.indexOf(marker);
  expect(at, `渲染结果里没有 ${marker}`).toBeGreaterThanOrEqual(0);
  return html.slice(html.lastIndexOf("<", at), html.indexOf(">", at) + 1);
}

/** 标签原文里某个属性的值（读判别值用；缺席即判红）。 */
function attributeOf(tag: string, name: string): string {
  const match = new RegExp(`${name}="([^"]*)"`).exec(tag);
  expect(match, `${name} 不在 ${tag} 里`).not.toBeNull();
  return match?.[1] ?? "";
}

describe("变更文件右栏：只有结构化统计与相对化路径", () => {
  it("呈现展示路径、类型、摘要与行数统计，并给每条改动一个诊断入口", () => {
    const { store } = storeOf();
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000401",
      view: changeView("11111111-0000-4000-8000-0000000000d1"),
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

  it("右栏不解析 ACP 原文：原文块与原文串在默认渲染里都不出现", () => {
    const { store } = storeOf();
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000402",
      view: changeView("11111111-0000-4000-8000-0000000000d2"),
      acp: { mediaType: "application/json", rawJson: RAW_DIFF, byteLength: "120", sha256: "a".repeat(43) },
    });
    const html = renderPage(store);

    // 反例：为了填满右栏去解析原文（或把原文当差异渲染）时，下面每一条都会变红。
    expect(html.match(/data-raw-json=/g) ?? []).toHaveLength(0);
    expect(html).not.toContain("const idleTimeout");
    expect(html).not.toContain("data-diff-line");
    expect(html).not.toContain('data-diff-region="lines"');
    // 没有结构化差异行时必须明确说明，而不是留一块空的差异区（spec 的第三条场景）。
    expect(html).toContain('data-diff-not-delivered="true"');
    expect(html).toContain("原文未同步");
    // 统计范围的说明必须在（「统计只覆盖 Agent 声明的改动」+「差异行只来自结构化来源」）。
    expect(html).toContain('data-change-statistics-scope="true"');
    expect(html).toContain("不解析 ACP 原文");
  });

  it("展开诊断：原文以单块不可信文本出现（没有被切成差异行）", () => {
    const { store } = storeOf();
    const eventId = "dddddddd-0000-4000-8000-000000000403";
    applyFileChanged(store, {
      eventId,
      view: changeView("11111111-0000-4000-8000-0000000000d3"),
      acp: { mediaType: "application/json", rawJson: RAW_DIFF, byteLength: "120", sha256: "a".repeat(43) },
    });
    const html = renderPage(store, { openChangeDiagnosticsEventId: eventId });

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
      view: changeView("11111111-0000-4000-8000-0000000000d4", {
        displayPath: "src/auth/config.ts",
        summary: "改了默认值",
      }),
      acp: {
        mediaType: "application/json",
        rawUnavailable: { reason: "size_limit", byteLength: "281344", sha256: "b".repeat(43) },
      },
    });
    const html = renderPage(store, { openChangeDiagnosticsEventId: eventId });

    expect(html).toContain('data-raw-unavailable="size_limit"');
    expect(html).toContain("281344");
    // 反例：渲染占位原文假装完整 → 这里会出现原文块。
    expect(html).not.toContain('data-raw-json="available"');
  });
});

describe("变更文件右栏：差异区的三种形态互不冒充", () => {
  it("有结构化差异行：逐行呈现并区分新增 / 删除 / 上下文", () => {
    const { store } = storeOf();
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000411",
      view: changeView("11111111-0000-4000-8000-0000000000e1", {
        diffLines: [
          { kind: "context", text: "  const opts = resolveOptions(input);" },
          { kind: "deleted", text: "- const idleTimeout = 30_000;" },
          { kind: "added", text: "+ const idleTimeout = opts.idleTimeoutMs ?? 10_000;" },
        ],
      }),
    });
    const html = renderPage(store);

    const region = tagContaining(html, 'data-diff-region="lines"');
    expect(attributeOf(region, "data-diff-line-count")).toBe("3");
    expect(html.match(/data-diff-line-kind="context"/g) ?? []).toHaveLength(1);
    expect(html.match(/data-diff-line-kind="deleted"/g) ?? []).toHaveLength(1);
    expect(html.match(/data-diff-line-kind="added"/g) ?? []).toHaveLength(1);
    // 行内容逐字来自结构化来源（客户端不拼、不复原）。
    expect(html).toContain("const idleTimeout = opts.idleTimeoutMs ?? 10_000;");
    // 有差异行时不再说「没有下发」。
    expect(html).not.toContain("data-diff-not-delivered");
    expect(html).not.toContain("data-diff-unavailable");
  });

  it("差异行按不可信文本处理：标签与链接只作为文本出现，不产生元素、不写地址属性", () => {
    const { store } = storeOf();
    const hostile = '+ <script>alert(1)</script> [点我](https://evil.example/x)';
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000412",
      view: changeView("11111111-0000-4000-8000-0000000000e2", {
        diffLines: [{ kind: "added", text: hostile }],
      }),
    });
    const html = renderPage(store);

    expect(html).not.toContain("<script");
    expect(html).not.toContain("<a ");
    expect(html).not.toMatch(/\s(href|src)="/);
    // 不执行也不消失：原文以转义后的文本逐字出现。
    expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(html).toContain("[点我](https://evil.example/x)");
  });

  it("有不可用标记：呈现已登记原因对应的文案", () => {
    const { store } = storeOf();
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000413",
      view: changeView("11111111-0000-4000-8000-0000000000e3", {
        diffUnavailable: { reason: "size_limit" },
      }),
    });
    const html = renderPage(store);

    expect(html).toContain('data-diff-unavailable="size_limit"');
    expect(html).toContain(FILE_DIFF_UNAVAILABLE_LABELS.size_limit);
    // 不得冒充成「有差异行」或「未同步」。
    expect(html).not.toContain("data-diff-not-delivered");
    expect(html).not.toContain('data-diff-region="lines"');
  });

  it("两者都没有：明确说明而不是渲染占位差异行", () => {
    const { store } = storeOf();
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000414",
      view: changeView("11111111-0000-4000-8000-0000000000e4"),
    });
    const html = renderPage(store);

    expect(html).toContain('data-diff-not-delivered="true"');
    expect(html).toContain("不渲染占位差异行");
    expect(html).not.toContain('data-diff-region="lines"');
    expect(html).not.toContain("data-diff-unavailable");
  });

  it("形状不合规的下发（空数组与未知原因）落回「两者都没有」，不伪造成另一种形态", () => {
    const { store } = storeOf();
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000415",
      view: changeView("11111111-0000-4000-8000-0000000000e5", { diffLines: [] }),
    });
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000416",
      view: changeView("11111111-0000-4000-8000-0000000000e6", {
        displayPath: "src/b.ts",
        diffUnavailable: { reason: "too_many" },
      }),
    });
    const html = renderPage(store);

    expect(html.match(/data-diff-not-delivered="true"/g) ?? []).toHaveLength(2);
    expect(html).not.toContain('data-diff-region="lines"');
    // 未知原因没有文案，不得拿 `size_limit` 的文案顶替。
    expect(html).not.toContain("data-diff-unavailable");
    expect(html).not.toContain(FILE_DIFF_UNAVAILABLE_LABELS.size_limit);
  });
});

describe("变更文件右栏：选中项与时间线条目共用同一个值", () => {
  it("选中的那一行标出选中，其余行不标出", () => {
    const { store } = storeOf();
    const selectedId = "11111111-0000-4000-8000-0000000000f1";
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000421",
      view: changeView(selectedId),
    });
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000422",
      view: changeView("11111111-0000-4000-8000-0000000000f2", { displayPath: "src/b.ts" }),
    });
    const html = renderPage(store, { selectedChangeId: selectedId });

    expect(attributeOf(tagContaining(html, `data-file-change="${selectedId}"`), "data-file-change-selected")).toBe(
      "true",
    );
    expect(
      attributeOf(tagContaining(html, 'data-file-change="11111111-0000-4000-8000-0000000000f2"'), "data-file-change-selected"),
    ).toBe("false");
  });

  it("给了选中回调时路径是可点入口，缺省时不渲染（不是点了没反应的按钮）", () => {
    const { store } = storeOf();
    const changeId = "11111111-0000-4000-8000-0000000000f3";
    applyFileChanged(store, {
      eventId: "dddddddd-0000-4000-8000-000000000423",
      view: changeView(changeId),
    });

    const withCallback = renderPage(store, { onSelectChange: () => undefined });
    expect(withCallback).toContain('data-action="select-file-change"');
    expect(tagContaining(withCallback, 'data-action="select-file-change"')).toContain(`data-change-id="${changeId}"`);

    const withoutCallback = renderPage(store);
    expect(withoutCallback).not.toContain('data-action="select-file-change"');
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
