/**
 * 代码块渲染：`CodeBlock.tsx` 的等宽呈现、语言标记、复制入口、序号锚点与不可信内容边界。
 *
 * 关键判据：
 *
 * - **高亮不改文本**：同一段代码在「注册过的语言」与「未知语言」两条路径下，去掉标签后的
 *   文本必须逐字相同——高亮只加颜色，不加也不丢字符；
 * - **不注入 HTML**：`highlight.js` 的 `hljs-*` 类名只能停在组件内部，渲染结果里既没有
 *   它们，也没有代码正文里的 `<script>`；正文里的 `</span>` 不可能伪装成标签；hljs 的三种
 *   开标签（单类名、多类名、子语言 `language-*`）都被切掉，字面标签不会漏进正文；
 * - **颜色归属**：多类名作用域（`hljs-title function_` 等）各自按家族上色，只有未知类名才
 *   回落到正文色——run 的 class 是这条的机器证据；
 * - **复制入口与序号**：`data-action="copy-code"` 每个代码块恰好一处，`data-code-index`
 *   按出现顺序递增；
 * - **样式来自 token**：渲染结果里没有字面色值，浅/深两套调色板下结构属性集合相同。
 *
 * 本文件用 `createElement` 而非 JSX：`vitest.config.ts` 的 `include` 只收 `*.test.ts`。
 */

import { createElement } from "react";
import type { ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

import { CodeBlock } from "./CodeBlock";

/**
 * 深浅色：`useTheme()` 读 `react-native-web` 的 `useColorScheme()`，这里把它换成受测试控制的
 * 取值。被替换的**只有**这一个函数——`theme.ts` 的 token 与真实 `StyleSheet` 都照旧走。
 *
 * 注意：`useTheme()` 现在还会**订阅主题偏好**（`useThemePreference()`，一个 React hook），因此
 * 不能在渲染之外直接调用组件函数；要核对复制入口上抛的载荷时，用下面的 `captureCodeBlock`
 * 在一次真实渲染里把元素树截下来。
 */
const scheme = vi.hoisted(() => ({ current: "light" as "light" | "dark" }));

vi.mock("react-native-web", async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>;
  return { ...actual, useColorScheme: () => scheme.current };
});

afterEach(() => {
  scheme.current = "light";
});

/** 含 HTML 与远端地址的代码正文：三者都不得变成标签或资源引用。 */
const HOSTILE_CODE = ["const url = \"https://evil.example/x\";", "// </span><script>alert(1)</script>"].join("\n");

/** hljs 会转义的实体（与实现同口径，用于把渲染结果还原成文本做逐字比对）。 */
const ENTITIES: Readonly<Record<string, string>> = {
  "&amp;": "&",
  "&lt;": "<",
  "&gt;": ">",
  "&quot;": '"',
  "&#x27;": "'",
  "&#39;": "'",
};

/** 去掉标签并把实体还原成文本——比对的判据是「用户看到的字」。 */
function visibleText(html: string): string {
  return html
    .replace(/<[^>]*>/g, "")
    .replace(/&(amp|lt|gt|quot|#x27|#39);/g, (entity) => ENTITIES[entity] ?? entity);
}

function structure(html: string): readonly string[] {
  return [...html.matchAll(/<(\/?)([a-z]+)/g)].map((match) => `${match[1] ?? ""}${match[2] ?? ""}`);
}

function anchors(html: string): readonly string[] {
  return [...html.matchAll(/\s(data-[a-z-]+)="([^"]*)"/g)].map((match) => `${match[1] ?? ""}=${match[2] ?? ""}`);
}

function classes(html: string): readonly string[] {
  return [...html.matchAll(/class="([^"]*)"/g)].map((match) => match[1] ?? "");
}

function renderCodeBlock(props: {
  readonly code?: string;
  readonly language?: string;
  readonly index?: number;
}): string {
  return renderToStaticMarkup(createElement(CodeBlock, props));
}

/**
 * 取 `CodeBlock` 的**元素树**：静态 HTML 里看不到 `onPress`，因此「复制入口上抛了什么载荷」这类
 * 断言必须拿到元素而不是字符串。
 *
 * 为什么不能直接写 `CodeBlock({...})`：`useTheme()` 会订阅主题偏好，那是一个 React hook，
 * 在渲染之外调用会抛 `Invalid hook call`。这里借一次真实渲染把元素截下来——`CodeBlock` 的 hook
 * 落到 `Capture` 的 fiber 上，顺序与它在渲染里被调用时一致（每次渲染恰好一次）。
 */
function captureCodeBlock(props: Parameters<typeof CodeBlock>[0]): unknown {
  let captured: unknown = null;
  function Capture(): ReactElement {
    captured = CodeBlock(props);
    return createElement("div");
  }
  renderToStaticMarkup(createElement(Capture));
  return captured;
}

interface ElementLike {
  readonly props?: {
    readonly dataSet?: Record<string, string | number> | undefined;
    readonly onPress?: (() => void) | undefined;
    readonly children?: unknown;
  };
}

/** 在元素树里找出复制入口（宽搜首个 `data-action="copy-code"`）并返回它的 props。 */
function findCopyButtonProps(root: unknown): ElementLike["props"] {
  const queue: unknown[] = [root];

  while (queue.length > 0) {
    const node = queue.shift();
    if (typeof node !== "object" || node === null) continue;
    const props = (node as ElementLike).props;
    if (props === undefined) continue;
    if (props.dataSet?.["action"] === "copy-code") return props;
    if (Array.isArray(props.children)) {
      queue.push(...props.children);
    } else if (props.children !== undefined) {
      queue.push(props.children);
    }
  }

  return undefined;
}

/** 代码块正文（`data-code-text="true"` 那个元素内部）的可见文本：不掺入语言标记与按钮。 */
function codeBodyText(html: string): string {
  const at = html.indexOf('data-code-text="true"');
  if (at === -1) return "";
  const open = html.indexOf(">", at);
  return open === -1 ? "" : visibleText(html.slice(open + 1));
}

/**
 * 正文里的高亮 run：`(可见文本, class 属性)`。
 *
 * 每个 run 是一个叶子 `Text`（渲染成不带嵌套的 `<span>`），因此非贪婪匹配就能取到它的文本——
 * 这是「颜色归属」可断言的机器证据（run 的 class 与家族一一对应）。
 */
function codeRuns(html: string): readonly (readonly [string, string])[] {
  const at = html.indexOf('data-code-text="true"');
  const open = at === -1 ? -1 : html.indexOf(">", at);
  const body = open === -1 ? "" : html.slice(open + 1);

  return [...body.matchAll(/<span class="([^"]*)"[^>]*>([\s\S]*?)<\/span>/g)].map(
    (match) => [visibleText(match[2] ?? ""), match[1] ?? ""] as const,
  );
}

describe("代码块：等宽呈现与语言标记", () => {
  it("呈现语言标记，正文逐字保留", () => {
    const html = renderCodeBlock({ code: "const a = 1;", language: "typescript" });

    expect(html).toContain('data-code-language="typescript"');
    expect(codeBodyText(html)).toBe("const a = 1;");
  });

  it("无语言标记时按「文本」呈现，且不做高亮", () => {
    const labelled = renderCodeBlock({ code: HOSTILE_CODE, language: "typescript" });
    const unlabelled = renderCodeBlock({ code: HOSTILE_CODE });

    expect(unlabelled).toContain('data-code-language="文本"');
    expect((labelled.match(/<span/g) ?? []).length).toBeGreaterThan(
      (unlabelled.match(/<span/g) ?? []).length,
    );
    // 高亮只加颜色：两条路径的代码正文必须逐字相同。
    expect(codeBodyText(labelled)).toBe(codeBodyText(unlabelled));
    expect(codeBodyText(unlabelled)).toBe(HOSTILE_CODE);
  });

  it("未注册的语言（含别名之外的写法）退化成纯文本，不抛错", () => {
    const html = renderCodeBlock({ code: "++++[>++++++++<-]>", language: "brainfuck" });

    expect(html).toContain('data-code-language="brainfuck"');
    expect(codeBodyText(html)).toBe("++++[>++++++++<-]>");
  });

  it("`ts` 这类别名走同一份 TypeScript 语法", () => {
    const alias = renderCodeBlock({ code: "const a: number = 1;", language: "ts" });
    const full = renderCodeBlock({ code: "const a: number = 1;", language: "typescript" });

    expect((alias.match(/<span/g) ?? []).length).toBe((full.match(/<span/g) ?? []).length);
  });
});

describe("代码块：复制入口与序号锚点", () => {
  it("每个代码块恰好一个复制锚点", () => {
    const html = renderCodeBlock({ code: "a\nb\nc", language: "text" });

    expect((html.match(/data-action="copy-code"/g) ?? []).length).toBe(1);
  });

  it("序号进 `data-code-index`；未给序号时不留空锚点", () => {
    expect(renderCodeBlock({ code: "x", index: 2 })).toContain('data-code-index="2"');
    expect(renderCodeBlock({ code: "x" })).not.toContain("data-code-index");
  });

  it("复制入口上抛正文与语言标记（回调缺省时按钮仍在）", () => {
    const payloads: (readonly [string, string])[] = [];
    const withCallback = findCopyButtonProps(
      captureCodeBlock({
        code: "const a = 1;",
        language: "typescript",
        onCopy: (code, language) => {
          payloads.push([code, language]);
        },
      }),
    );

    withCallback?.onPress?.();
    expect(payloads).toEqual([["const a = 1;", "typescript"]]);

    // 缺省（页面还没接平台能力）时按钮仍然渲染，只是没有落点。
    const withoutCallback = findCopyButtonProps(captureCodeBlock({ code: "const a = 1;" }));
    expect(withoutCallback).not.toBeUndefined();
    expect(withoutCallback?.onPress).not.toBeUndefined();
    expect(findCopyButtonProps(captureCodeBlock({}))).not.toBeUndefined();
  });
});

describe("代码块：不可信代码内容", () => {
  it("代码里的 script/span 不产生元素，hljs 类名也不进 DOM", () => {
    const html = renderCodeBlock({ code: HOSTILE_CODE, language: "typescript" });

    expect(html).not.toMatch(/<script|<img|<iframe/i);
    expect(html).not.toContain("hljs-");
    expect(html).not.toMatch(/\s(src|href)="/);
    expect(html).not.toMatch(/url\(/);
  });

  it("代码里的 `</span>` 不会截断渲染，正文仍然逐字可见", () => {
    const html = renderCodeBlock({ code: HOSTILE_CODE, language: "typescript" });

    expect(visibleText(html)).toContain("</span><script>alert(1)</script>");
    expect(visibleText(html)).toContain("https://evil.example/x");
    // 逐字不差：切分只按 hljs 自己的 span 边界，正文里的 `</span>` 伪装不成标签。
    expect(codeBodyText(html)).toBe(HOSTILE_CODE);
    // 代码正文里的地址是**文本**：不变成属性，也不被当成资源引用。
    expect(html).not.toMatch(/\s(src|href)="/);
  });
});

describe("代码块：高亮切分（多类名 span 与子语言容器）", () => {
  it("多类名 span（`hljs-title function_`）不会把标签文本漏进正文", () => {
    const code = "function greet(name) { return name; }";
    const html = renderCodeBlock({ code, language: "typescript" });

    expect(codeBodyText(html)).toBe(code);
    expect(html).not.toContain("&lt;span");
    expect(html).not.toContain("hljs-");
  });

  it("多类名作用域按家族复用同一个色：`title.class`/`title.function` 同色，`variable.language` 与关键字同色", () => {
    const html = renderCodeBlock({ code: "class Foo { bar() { return this.x; } }", language: "typescript" });
    const runs = codeRuns(html);
    const colours: Record<string, string> = Object.fromEntries(runs);

    // 切分必须逐个 token 成段：修前多类名开标签与 `Foo` 挤在同一段里，这段断言直接红。
    expect(runs.some(([runText]) => runText.includes("<span"))).toBe(false);
    expect(colours["Foo"]).toBeDefined();
    expect(colours["Foo"]).toBe(colours["bar"]); // title.class / title.function 同家族
    expect(colours["this"]).toBeDefined();
    expect(colours["this"]).toBe(colours["return"]); // variable.language 与 keyword 同家族
    expect(colours["Foo"]).not.toBe(colours["return"]); // 不同家族不串色
  });

  it("子语言容器（`language-*`，无 `hljs-` 前缀）也被消费，容器内文本不冒充标签色", () => {
    const html = renderCodeBlock({ code: "<script>a<b</script>", language: "xml" });
    const colours: Record<string, string> = Object.fromEntries(codeRuns(html));

    expect(codeBodyText(html)).toBe("<script>a<b</script>");
    expect(html).not.toContain("&lt;span");
    // `a<b` 是容器内的一段独立 run；容器外的标签名 `script` 才吃标签色。
    expect(colours["a<b"]).toBeDefined();
    expect(colours["a<b"]).not.toBe(colours["script"]);
  });
});

describe("代码块：样式全部来自 theme token", () => {
  it("渲染结果里没有字面色值", () => {
    for (const current of ["light", "dark"] as const) {
      scheme.current = current;
      const html = renderCodeBlock({ code: HOSTILE_CODE, language: "typescript" });

      expect(html).not.toMatch(/rgba?\(|#[0-9a-fA-F]{3,8}\b/);
    }
  });

  it("浅/深两套调色板下结构属性集合相同，只有调色板相关的样式不同", () => {
    scheme.current = "light";
    const light = renderCodeBlock({ code: "const a = 1;", language: "typescript", index: 0 });
    scheme.current = "dark";
    const dark = renderCodeBlock({ code: "const a = 1;", language: "typescript", index: 0 });

    expect(structure(dark)).toEqual(structure(light));
    expect(anchors(dark)).toEqual(anchors(light));
    expect([...classes(dark)].sort()).not.toEqual([...classes(light)].sort());
  });
});
