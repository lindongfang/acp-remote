/**
 * 富文本渲染：`Markdown.tsx` 的不可信输入边界与样式来源。
 *
 * 逐条对应 spec「Agent 产生的富文本按不可信输入渲染」的四个场景：
 *
 * - 脚本与原始 HTML 不生效 → 源里的 `<script>`/`<img onerror>`/`<b>` 都只作为**文本**出现，
 *   渲染结果里不出现这些元素（判据是标签名，不是「文本里没这个词」——降级呈现本来就该保留原文）；
 * - 远端资源不被加载 → 图片与链接都只出说明，渲染结果里**没有** `src`/`href`，也没有目标地址；
 * - 代码块可读且可复制 → `fenced code` 交给 `CodeBlock`，复制锚点与序号锚点都在；
 * - 样式全部来自 token → 渲染结果里没有任何字面色值（颜色都进了 `StyleSheet`），
 *   且浅/深两套调色板下**结构属性集合完全相同**（换个底色不该改结构）。
 *
 * 本文件用 `createElement` 而非 JSX：`vitest.config.ts` 的 `include` 只收 `*.test.ts`。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Markdown, MARKDOWN_CHUNK_CHARS, MARKDOWN_VISIBLE_CHUNKS, splitMarkdownSource } from "./Markdown";

/**
 * 深浅色：`useTheme()` 读 `react-native-web` 的 `useColorScheme()`，这里把它换成受测试控制的
 * 取值。被替换的**只有**这一个函数——`theme.ts` 的 token 与真实 `StyleSheet` 都照旧走，
 * 否则「样式来自 token」这条断言就失去判据。
 */
const scheme = vi.hoisted(() => ({ current: "light" as "light" | "dark" }));

vi.mock("react-native-web", async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>;
  return { ...actual, useColorScheme: () => scheme.current };
});

afterEach(() => {
  scheme.current = "light";
});

/** 一份把 spec 四个场景压在一起的 Markdown 样本（脚本、原始 HTML、远端图片与链接都含）。 */
const SAMPLE = [
  "# 标题",
  "",
  "段落里有 **强调**、*斜体*、~~删除线~~、`行内代码` 与 <b>原始 HTML</b>。",
  "",
  "<script>window.__pwned = 1</script>",
  "",
  '<img src=x onerror="window.__pwned=2">',
  "",
  "![remote](https://evil.example/x.png)",
  "",
  "[link](https://evil.example/page)",
  "",
  "> 引用里的 `代码` 与 <i>原文</i>",
  "",
  "- 无序项",
  "  - 嵌套项",
  "- [x] 已完成项",
  "",
  "3. 第三项",
  "4. 第四项",
  "",
  "| 左 | 右 |",
  "| :-- | --: |",
  "| a | b |",
  "",
  "---",
  "",
  "```typescript",
  "const answer = 42;",
  "```",
].join("\n");

function renderMarkdown(props: {
  readonly value?: string;
  readonly onLinkPress?: (href: string) => void;
  readonly onCopyCode?: (code: string, language: string) => void;
  readonly onFilePathPress?: (path: string) => void;
}): string {
  return renderToStaticMarkup(createElement(Markdown, props));
}

/** 标签结构（含开闭），用来比对两套调色板下的结构是否一致。 */
function structure(html: string): readonly string[] {
  return [...html.matchAll(/<(\/?)([a-z]+)/g)].map((match) => `${match[1] ?? ""}${match[2] ?? ""}`);
}

/** 渲染器会转义的实体（与实现同口径，用于把渲染结果还原成文本做逐字比对）。 */
const ENTITIES: Readonly<Record<string, string>> = {
  "&amp;": "&",
  "&lt;": "<",
  "&gt;": ">",
  "&quot;": '"',
  "&#x27;": "'",
  "&#39;": "'",
};

/**
 * 去标签 + 还原实体的可见文本。
 *
 * 判据必须是这个而不是原始 HTML 子串：原始 HTML 与行内代码会被渲染成**多个相邻元素**
 * （`<b>`、正文、`</b>` 各一段；高亮后的代码被切成若干 `span`），按子串比对会把正确的
 * 渲染判成失败。
 */
function visibleText(html: string): string {
  return html
    .replace(/<[^>]*>/g, "")
    .replace(/&(amp|lt|gt|quot|#x27|#39);/g, (entity) => ENTITIES[entity] ?? entity);
}

/** 全部 `data-*` 锚点（名 + 值），浅/深两套调色板下必须逐条相同。 */
function anchors(html: string): readonly string[] {
  return [...html.matchAll(/\s(data-[a-z-]+)="([^"]*)"/g)].map((match) => `${match[1] ?? ""}=${match[2] ?? ""}`);
}

/** 全部 class 取值，用来证明样式确实随调色板变化（而不是「没读主题」）。 */
function classes(html: string): readonly string[] {
  return [...html.matchAll(/class="([^"]*)"/g)].map((match) => match[1] ?? "");
}

describe("富文本：脚本与原始 HTML 不生效", () => {
  it("不产生 script/img/iframe 元素", () => {
    const html = renderMarkdown({ value: SAMPLE });

    expect(html).not.toMatch(/<script|<img|<iframe|<object|<embed/i);
  });

  it("原始 HTML 与脚本按纯文本保留，而不是被丢掉", () => {
    const html = renderMarkdown({ value: SAMPLE });
    const text = visibleText(html);

    // 降级呈现要求「不执行但也不消失」：原文以文本形式逐字出现。
    expect(text).toContain("<script>window.__pwned = 1</script>");
    expect(text).toContain('<img src=x onerror="window.__pwned=2">');
    expect(text).toContain("<b>原始 HTML</b>");
    expect(html).toContain('data-markdown-omitted="html"');
  });

  it("脚本与原始 HTML 不进入任何属性位置", () => {
    const html = renderMarkdown({ value: SAMPLE });

    // 源里的 `onerror=` 必须停在文本里（转义后的引号是 `&quot;`），不能变成元素的属性。
    expect(html).not.toMatch(/\son(click|error|load|mouseover|focus)="/i);
    expect(html).not.toMatch(/javascript:/i);
  });
});

describe("富文本：远端资源不被加载", () => {
  it("图片只出「未加载」说明，不写 src、不出现目标地址", () => {
    const html = renderMarkdown({ value: SAMPLE });

    expect(html).toContain('data-markdown-omitted="image"');
    expect(html).toContain("远端图片未加载：remote");
    expect(html).not.toContain("evil.example");
    expect(html).not.toMatch(/\ssrc="/);
    expect(html).not.toMatch(/url\(/);
  });

  it("链接只出标签与说明，不写 href、不出现目标地址", () => {
    const html = renderMarkdown({ value: SAMPLE });

    expect(html).toContain('data-markdown-omitted="link"');
    expect(html).toContain("外部链接未加载");
    expect(html).not.toMatch(/\shref="/);
    expect(html).not.toContain("evil.example");
  });

  it("整份渲染结果里没有任何远端地址", () => {
    expect(renderMarkdown({ value: SAMPLE })).not.toMatch(/https?:\/\//);
  });

  it("回调缺省时链接不是可点元素，给了回调才有 link 锚点", () => {
    const withoutCallback = renderMarkdown({ value: "[link](https://evil.example/page)" });
    const withCallback = renderMarkdown({
      value: "[link](https://evil.example/page)",
      onLinkPress: () => undefined,
    });

    expect(withoutCallback).not.toContain('data-markdown-link="true"');
    expect(withoutCallback).not.toContain("role=\"link\"");
    expect(withCallback).toContain('data-markdown-link="true"');
    expect(withCallback).toContain('role="link"');
    // 给了回调也不改变「不写 href、不加载」这条：目标地址仍不出现在输出里。
    expect(withCallback).not.toContain("evil.example");
    expect(withCallback).not.toMatch(/\shref="/);
  });
});

describe("富文本：结构覆盖", () => {
  it("标题、段落、引用、列表、表格、行内代码各有锚点", () => {
    const html = renderMarkdown({ value: SAMPLE });

    for (const block of ["heading", "paragraph", "blockquote", "list", "table", "hr"]) {
      expect(html).toContain(`data-markdown-block="${block}"`);
    }
    expect(html).toContain('data-markdown-codespan="true"');
    expect(html).toContain('data-markdown-cell="true"');
    // 表格的列对齐从 token 来：`textAlign` 经 `StyleSheet` 落地成原子 class，
    // 两列因此带两个不同的取值。
    expect([...new Set([...html.matchAll(/r-textAlign-[\w]+/g)].map((match) => match[0]))]).toHaveLength(2);
  });

  it("有序与无序列表的标记各自成立，嵌套项与任务项各就其位", () => {
    const html = renderMarkdown({ value: SAMPLE });

    expect(html).toContain("3."); // ordered list 的 start
    expect(html).toContain("[x]"); // 任务项（勾选态只在 marker 出现一次）
    expect(visibleText(html)).toContain("嵌套项"); // 嵌套列表不会被吞掉
    // 3 个无序项（含嵌套项与任务项）+ 2 个有序项。
    expect((html.match(/data-markdown-block="list-item"/g) ?? []).length).toBe(5);
  });

  it("fenced code 交给 CodeBlock，且按出现顺序编号", () => {
    const html = renderMarkdown({
      value: ["```ts", "const a = 1;", "```", "", "```json", "{}", "```"].join("\n"),
    });

    expect((html.match(/data-code-block="true"/g) ?? []).length).toBe(2);
    expect((html.match(/data-action="copy-code"/g) ?? []).length).toBe(2);
    expect(html).toContain('data-code-language="ts"');
    expect(html).toContain('data-code-language="json"');
    expect(html.indexOf('data-code-index="0"')).toBeLessThan(html.indexOf('data-code-index="1"'));
    // 高亮把代码切成多个 span，因此按**可见文本**断言正文，而不是按 HTML 子串。
    expect(visibleText(html)).toContain("const a = 1;");
  });

  it("Markdown 自己的渲染路径不写 href/src（代码块也不写）", () => {
    const html = renderMarkdown({ value: SAMPLE });

    expect(html).not.toMatch(/\s(src|href)="/);
  });
});

describe("富文本：props 全部可选", () => {
  it("不给 value 时渲染出空的根节点，不抛错、不留痕迹", () => {
    const html = renderMarkdown({});

    expect(html).toContain('data-markdown="true"');
    expect(structure(html)).toEqual(["div", "/div"]);
    expect(visibleText(html)).toBe("");
  });

  it("空串与 undefined 等价", () => {
    expect(renderMarkdown({ value: "" })).toBe(renderMarkdown({}));
  });
});

describe("富文本：样式全部来自 theme token", () => {
  it("渲染结果里没有字面色值", () => {
    for (const current of ["light", "dark"] as const) {
      scheme.current = current;
      const html = renderMarkdown({ value: SAMPLE });

      // 颜色一律经 `StyleSheet`（成为原子 class）；渲染路径上任何字面色值都会以这种形式露出来。
      expect(html).not.toMatch(/rgba?\(|#[0-9a-fA-F]{3,8}\b/);
    }
  });

  it("浅/深两套调色板下结构属性集合完全相同，只有调色板相关的样式不同", () => {
    scheme.current = "light";
    const light = renderMarkdown({ value: SAMPLE });
    scheme.current = "dark";
    const dark = renderMarkdown({ value: SAMPLE });

    expect(structure(dark)).toEqual(structure(light));
    expect(anchors(dark)).toEqual(anchors(light));
    // 并且确实读到了主题：两套调色板的原子 class 不可能相同。
    expect([...classes(dark)].sort()).not.toEqual([...classes(light)].sort());
  });
});

describe("富文本：工作目录路径联动", () => {
  it("只有文件形状的行内代码成为可点控件；命令、标识符、绝对路径与 .. 保持纯文本", () => {
    const html = renderMarkdown({
      value: "看 `src/a.ts`、`package.json`、`../evil.ts`、`/abs/x.ts`、`npm test`、`true`、`useState` 与 `C:secret`",
      onFilePathPress: () => undefined,
    });

    expect(html).toContain('data-markdown-file-path="src/a.ts"');
    // 无目录分隔符但有扩展名：仍是用户想联动查看的文件。
    expect(html).toContain('data-markdown-file-path="package.json"');
    expect(html).toContain('role="link"');
    // 不可解析/不是文件的目标既不是链接锚点，也不会把原值放进路径属性。
    expect(html).not.toContain('data-markdown-file-path="../evil.ts"');
    expect(html).not.toContain('data-markdown-file-path="/abs/x.ts"');
    expect(html).not.toContain('data-markdown-file-path="npm test"');
    // 命令与普通标识符不能被画成文件链接（否则点击会去读一个不存在的路径）。
    expect(html).not.toContain('data-markdown-file-path="true"');
    expect(html).not.toContain('data-markdown-file-path="useState"');
    expect(html).not.toContain('data-markdown-file-path="C:secret"');
    // 文本仍逐字保留（降级呈现不丢内容）。
    expect(visibleText(html)).toContain("../evil.ts");
    expect(visibleText(html)).toContain("npm test");
    expect(visibleText(html)).toContain("useState");
  });

  it("回调缺省时路径不渲染成可点控件（不制造点了没反应的入口）", () => {
    const html = renderMarkdown({ value: "看 `src/a.ts`" });

    expect(html).not.toContain("data-markdown-file-path");
    expect(html).not.toContain('role="link"');
    expect(visibleText(html)).toContain("src/a.ts");
  });
});

describe("富文本：长输出分块与可见范围", () => {
  /** 一个围栏未闭合的代码块（流式中途）：后文全部属于它，必须保持可读。 */
  it("未闭合代码块留在同一块里且渲染成可读代码", () => {
    const value = "```ts\nconst answer = 42;\nconst next =";
    expect(splitMarkdownSource(value)).toHaveLength(1);

    const html = renderMarkdown({ value });
    expect(html).toContain('data-code-block="true"');
    expect(visibleText(html)).toContain("const answer = 42;");
    expect(visibleText(html)).toContain("const next =");
  });

  it("切分点只在围栏之外的空行，拼接后与原文逐字一致", () => {
    const blocks = Array.from(
      { length: 4 },
      (_, index) => `## 第 ${String(index)} 节\n${"字".repeat(MARKDOWN_CHUNK_CHARS)}`,
    );
    const fenced = ["```text", "a", "", "b", "```"].join("\n");
    const value = `${blocks.join("\n\n")}\n\n${fenced}\n\n末尾`;
    const chunks = splitMarkdownSource(value);

    expect(chunks.length).toBeGreaterThan(1);
    expect(chunks.join("\n")).toBe(value);
    // 围栏内部不切：整段围栏在同一块里（含它前后的边界空行）。
    const fenceChunk = chunks.find((chunk) => chunk.includes("```text"));
    expect(fenceChunk).toContain("a\n\nb");
    expect(fenceChunk).toContain("```");
  });

  it("列表项的缩进续段不会被切开", () => {
    const head = `- 第一项\n\n${"  ".repeat(0)}${"x".repeat(MARKDOWN_CHUNK_CHARS)}`;
    const value = `${head}\n\n  续段属于同一项\n\n后续段落`;
    const chunks = splitMarkdownSource(value);

    expect(chunks.join("\n")).toBe(value);
    expect(chunks[0]).toContain("续段属于同一项");
  });

  it("长输出默认只渲染末尾若干块，更早内容有显式展开入口", () => {
    const total = MARKDOWN_VISIBLE_CHUNKS + 2;
    const value = Array.from(
      { length: total },
      (_, index) => `第${String(index)}段 ${"段".repeat(MARKDOWN_CHUNK_CHARS)}`,
    ).join("\n\n");
    const html = renderMarkdown({ value });

    expect(splitMarkdownSource(value)).toHaveLength(total);
    expect(html).toContain(`data-markdown-chunks-hidden="2"`);
    expect(visibleText(html)).toContain(`第${String(total - 1)}段`);
    expect(visibleText(html)).not.toContain("第0段");
    expect(visibleText(html)).toContain("还有 2 段更早内容未展开");
  });
});
