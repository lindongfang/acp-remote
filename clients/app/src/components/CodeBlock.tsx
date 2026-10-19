/**
 * 代码块（`design.md` D9 / spec「代码块可读且可复制」）。
 *
 * ## 为什么是「hljs 高亮 + 自建 RN 文本视图」而不是现成的高亮组件
 *
 * 高亮用 `highlight.js/lib/core`，语言按需注册（design D9 的开放项：先覆盖 Agent 最常产出的
 * 几类，后续按需增删）。`highlight.js` 的 `highlight()` 只产出 **HTML 字符串**，而本工程
 * MUST NOT 渲染原始 HTML（`docs/FRONTEND_DESIGN.md` §4.4）。因此这里把该字符串按 hljs 自己的
 * span 边界切成 `(文本, token 类名)` 序列，再逐个渲染成 RN `Text`——**没有任何 HTML 进 DOM**。
 * 这条切分是安全的：hljs 默认 `escapeHTML`，代码正文里的 `<` 已经被它转义成 `&lt;`，
 * 输出里出现的 `<` 只可能来自 hljs 自己的 `<span class="hljs-*">`（`codeblock.test.ts` 用
 * 含 `</span><script>` 的正文固定这一点）。
 *
 * ## 不可信输入边界
 *
 * - 代码正文与语言标记都是 Agent 产出的**纯文本**：只经 `Text` 渲染，不解析、不执行。
 * - 不发起任何远端加载：本组件没有图片/链接概念，也没有网络调用。
 * - 颜色全部来自 `theme.ts` 的 token（`hljs-*` 类名映射到 token 家族，未知类名回落到正文色）。
 *
 * 组件是纯函数：语言标记、正文、序号都从 props 来；正文按行渲染并带行号列（行号不进复制载荷），
 * 复制入口只把 `(code, language)` 上抛给页面，组件自身不触碰剪贴板等平台能力
 * （`docs/FRONTEND_DESIGN.md` §3 的分层口径，与 `SessionRow` 一致）。
 */

import type { ReactElement } from "react";
import type { TextStyle } from "react-native";
import hljs from "highlight.js/lib/core";
import bash from "highlight.js/lib/languages/bash";
import css from "highlight.js/lib/languages/css";
import diff from "highlight.js/lib/languages/diff";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import markdown from "highlight.js/lib/languages/markdown";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import sql from "highlight.js/lib/languages/sql";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";
import type { Palette } from "./theme";
import {
  Pressable,
  StyleSheet,
  Text,
  View,
  fontFamily,
  fontSize,
  palettes,
  radius,
  spacing,
  useTheme,
} from "./theme";

/**
 * 注册的语言：模块加载时一次性完成，渲染路径上不再注册（也不做 `highlightAuto`——那会把
 * 每种已注册语言都跑一遍，成本不可控且结果不稳定）。
 *
 * `typescript`/`javascript` 的定义自带 `ts`/`tsx`/`js`/`jsx` 等别名，`getLanguage` 会解析别名，
 * 因此 fenced code 写 ```ts 也能命中。
 */
hljs.registerLanguage("bash", bash);
hljs.registerLanguage("css", css);
hljs.registerLanguage("diff", diff);
hljs.registerLanguage("javascript", javascript);
hljs.registerLanguage("json", json);
hljs.registerLanguage("markdown", markdown);
hljs.registerLanguage("python", python);
hljs.registerLanguage("rust", rust);
hljs.registerLanguage("sql", sql);
hljs.registerLanguage("typescript", typescript);
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("yaml", yaml);

export interface CodeBlockProps {
  /** 代码正文；缺省为空串。 */
  readonly code?: string | undefined;
  /** fenced code 的语言标记（可为空）；缺省按「无标记」呈现且不做高亮。 */
  readonly language?: string | undefined;
  /** 同一段 Markdown 里的第几个代码块（自 0 起），只作为 `data-code-index` 锚点。 */
  readonly index?: number | undefined;
  /**
   * 复制入口的落点：页面在此接平台剪贴板能力。缺省时按钮仍渲染（e2e 的锚点不随接线状态消失），
   * 只是没有落点——组件不自己访问剪贴板。
   */
  readonly onCopy?: ((code: string, language: string) => void) | undefined;
}

/** 高亮 run：一段同色文本。`family` 为 `null` 表示「未归类」（正文色）。 */
interface CodeRun {
  readonly text: string;
  readonly family: TokenFamily | null;
}

/** 高亮配色的家族：每个家族对应一个既有颜色 token（不另造配色）。 */
type TokenFamily = "accent" | "ok" | "warn" | "tool" | "think" | "ink2" | "ink3";

/**
 * `hljs-*` 类名 → 颜色家族。
 *
 * 键是 hljs 11 的类名去掉 `hljs-` 前缀后的原文（含 `title.function` 这类带点的合成名）。
 * 未列出的类名回落到正文色（`family` 为 null），因此升级 hljs 引入新类名只会丢色，不会丢文本。
 */
const TOKEN_FAMILIES: Readonly<Record<string, TokenFamily>> = {
  keyword: "accent",
  literal: "accent",
  "variable.language": "accent",
  built_in: "accent",
  type: "accent",
  tag: "accent",
  name: "accent",
  attribute: "accent",
  attr: "accent",
  property: "accent",
  selector_tag: "accent",
  "selector-tag": "accent",
  "selector-class": "accent",
  "selector-id": "accent",
  string: "ok",
  regexp: "ok",
  "char.escape": "ok",
  symbol: "ok",
  bullet: "ok",
  quote: "ok",
  code: "ok",
  link: "ok",
  formula: "ok",
  addition: "ok",
  number: "think",
  title: "think",
  "title.class": "think",
  "title.function": "think",
  function: "think",
  class: "think",
  section: "think",
  meta: "warn",
  "meta-keyword": "warn",
  "meta-string": "ok",
  key: "accent",
  operator: "warn",
  punctuation: "ink2",
  params: "ink2",
  variable: "ink2",
  subst: "ink3",
  comment: "ink3",
  doctag: "ink3",
  emphasis: "ink3",
  strong: "ink3",
  deletion: "warn",
};

/**
 * 每个家族在浅/深两套调色板下的正文色样式。
 *
 * 同样走 `StyleSheet.create`：这样颜色以原子 class 落地（与其它样式一条口径），
 * 渲染结果里不会出现字面色值——「渲染路径无硬编码色值」这条因此是可断言的。
 */
function buildTokenStyles(colors: Palette): Readonly<Record<TokenFamily, TextStyle>> {
  return StyleSheet.create({
    accent: { color: colors.accent },
    ok: { color: colors.ok },
    warn: { color: colors.warn },
    tool: { color: colors.tool },
    think: { color: colors.think },
    ink2: { color: colors.ink2 },
    ink3: { color: colors.ink3 },
  });
}

/**
 * hljs 的 HTML 输出按 `<span class="…">` / `</span>` 切分。
 *
 * 开标签的 `class` 有**三种**形态（都由 hljs 的 `HTMLRenderer.span` 按 `scopeToCSSClass` 产出）：
 *
 * - 单层作用域 → `hljs-<scope>`；
 * - 分层作用域 `a.b.c` → `hljs-a b_ c__`（第 i 层尾随 i 个下划线，如 `hljs-title function_`、
 *   `hljs-variable language_`、`hljs-title class_ inherited__`）；
 * - 子语言容器 → `language-<alias>`，**没有** `hljs-` 前缀（如 xml 里 `<script>` 的内容）。
 *
 * 三种都必须被消费掉：只认「`hljs-` + 单个类名」会让后两种的开标签落进正文，字面标签文本
 * 就会显示成代码，同时它对应的 `</span>` 还会把家族栈多弹一层（`codeblock.test.ts` 逐字比对这些）。
 *
 * `hljs.highlight` 的 `value` 是**唯一**的输入来源，且它已对正文做过 HTML 转义——正文里的
 * `</span>` 会变成 `&lt;/span&gt;`，因此下面的扫描不会把正文误当成标签。
 */
const HIGHLIGHT_TAG = /<span class="([^"]*)">|<\/span>/g;

/** `hljs-` 前缀：只有带它的 class 参与配色。 */
const HLJS_PREFIX = "hljs-";

/**
 * 开标签的 `class` 属性 → 颜色家族；`null` 表示「未归类」（正文色）。
 *
 * 先把 `hljs-a b_ c__` 还原成作用域名 `a.b.c`（剥前缀、剥尾随下划线），再**由深到浅**查表：
 * `title.function` 命中就用它，未命中退回 `title`，再未命中才回落到正文色。这样 hljs 引入新的
 * 层级只会丢色、不会丢文本；子语言容器（`language-*`）无作用域语义，一律按正文色。
 */
function tokenFamily(className: string): TokenFamily | null {
  const [first, ...nested] = className.split(" ");
  if (first === undefined || !first.startsWith(HLJS_PREFIX)) return null;

  const pieces = [first.slice(HLJS_PREFIX.length), ...nested.map((piece) => piece.replace(/_+$/, ""))];
  for (let depth = pieces.length; depth > 0; depth -= 1) {
    const family = TOKEN_FAMILIES[pieces.slice(0, depth).join(".")];
    if (family !== undefined) return family;
  }
  return null;
}

/** hljs 会转义的实体（它的 `escapeHTML` 只产出这几个）。 */
const ENTITIES: Readonly<Record<string, string>> = {
  "&amp;": "&",
  "&lt;": "<",
  "&gt;": ">",
  "&quot;": '"',
  "&#x27;": "'",
  "&#39;": "'",
};

const ENTITY_PATTERN = /&(amp|lt|gt|quot|#x27|#39);/g;

/** 把 hljs 的高亮 HTML 切成 `(文本, 家族)` 序列；嵌套 span 用栈还原外层家族。 */
function toRuns(highlighted: string): readonly CodeRun[] {
  const runs: CodeRun[] = [];
  const stack: (TokenFamily | null)[] = [];
  let current: TokenFamily | null = null;
  let cursor = 0;

  for (const match of highlighted.matchAll(HIGHLIGHT_TAG)) {
    const at = match.index;
    if (at > cursor) {
      runs.push({
        text: highlighted.slice(cursor, at).replace(ENTITY_PATTERN, (entity) => ENTITIES[entity] ?? entity),
        family: current,
      });
    }
    cursor = at + match[0].length;
    if (match[1] === undefined) {
      // 与 `stack.push(current)` 成对：弹出即恢复**本 span 打开之前**的家族，
      // 否则嵌套深度 ≥2 时关闭后会跳到祖父家族（模板字符串、HTML 属性会中途掉色）。
      current = stack.pop() ?? null;
    } else {
      stack.push(current);
      current = tokenFamily(match[1]);
    }
  }
  if (cursor < highlighted.length) {
    runs.push({
      text: highlighted.slice(cursor).replace(ENTITY_PATTERN, (entity) => ENTITIES[entity] ?? entity),
      family: current,
    });
  }
  return runs;
}

/** 语言标记 → run 序列：未注册（或未标记）的语言不做高亮，正文原样呈现。 */
function highlightRuns(code: string, language: string): readonly CodeRun[] {
  if (language.length === 0 || hljs.getLanguage(language) === undefined) {
    return [{ text: code, family: null }];
  }
  return toRuns(hljs.highlight(code, { language, ignoreIllegals: true }).value);
}

/**
 * 把 run 序列按换行切成**行**：高亮 span 可以跨行（多行字符串、块注释），行号与逐行渲染因此
 * 必须在这里切分，而不是让 hljs 的 span 决定行边界。高亮只切文本、不改文本。
 */
function toLines(runs: readonly CodeRun[]): readonly (readonly CodeRun[])[] {
  const lines: CodeRun[][] = [[]];
  for (const run of runs) {
    const pieces = run.text.split("\n");
    pieces.forEach((piece, pieceIndex) => {
      if (pieceIndex > 0) lines.push([]);
      const current = lines[lines.length - 1];
      if (current !== undefined && piece.length > 0) {
        current.push({ text: piece, family: run.family });
      }
    });
  }
  return lines;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    root: {
      borderWidth: 1,
      borderColor: colors.line,
      // 左侧一道强调色（原型 `.code`）：代码卡与普通卡片在视觉上不是一回事。
      borderLeftWidth: 3,
      borderLeftColor: colors.accent,
      borderRadius: radius.md,
      backgroundColor: colors.panel,
      overflow: "hidden",
    },
    /** 语言条（原型 `.code .ch`）：与左侧强调色同族的一层浅底。 */
    header: {
      flexDirection: "row",
      alignItems: "center",
      justifyContent: "space-between",
      gap: spacing.sm,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      backgroundColor: colors.accentSoft,
      borderBottomWidth: 1,
      borderBottomColor: colors.line2,
    },
    language: {
      color: colors.accentInk,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    copy: {
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    copyText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    code: {
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
    },
    /** 一行：行号列 + 正文列（行号不进入复制载荷，也不能被选中复制）。 */
    codeLine: {
      flexDirection: "row",
      alignItems: "flex-start",
    },
    codeLineNumber: {
      minWidth: 28,
      paddingRight: spacing.sm,
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
      lineHeight: fontSize.sm + 5,
      textAlign: "right",
      userSelect: "none",
    },
    codeLineBody: {
      flexGrow: 1,
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
      lineHeight: fontSize.sm + 5,
    },
    /** 高亮 run：只带字面样式，不带容器内边距（否则每个 token 都会再缩进一次）。 */
    codeRun: {
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
      lineHeight: fontSize.sm + 5,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };
const TOKEN_STYLES = { light: buildTokenStyles(palettes.light), dark: buildTokenStyles(palettes.dark) };

/** 无语言标记时展示的标签（`data-code-language` 也是它，便于断言「有序且带标记」）。 */
const UNLABELLED = "文本";

/** 一个代码块：语言标记 + 复制入口 + 带行号的等宽正文。 */
export function CodeBlock({ code, language, index, onCopy }: CodeBlockProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const tokenStyles = theme.dark ? TOKEN_STYLES.dark : TOKEN_STYLES.light;

  const source = code ?? "";
  const label = (language ?? "").trim();
  const shownLabel = label.length === 0 ? UNLABELLED : label;
  const highlightLanguage = label.split(/\s+/)[0]?.toLowerCase() ?? "";
  const runs = highlightRuns(source, highlightLanguage);
  const lines = toLines(runs);

  return (
    <View
      style={styles.root}
      dataSet={index === undefined ? { codeBlock: "true" } : { codeBlock: "true", codeIndex: index }}
    >
      <View style={styles.header}>
        <Text style={styles.language} dataSet={{ codeLanguage: shownLabel }}>
          {shownLabel}
        </Text>
        <Pressable
          role="button"
          style={styles.copy}
          dataSet={{ action: "copy-code" }}
          onPress={() => {
            onCopy?.(source, label);
          }}
        >
          <Text style={styles.copyText}>复制</Text>
        </Pressable>
      </View>
      <View style={styles.code} dataSet={{ codeText: "true", codeLineCount: lines.length }}>
        {lines.map((line, lineIndex) => {
          const number = String(lineIndex + 1);
          return (
            <View key={number} style={styles.codeLine} dataSet={{ codeLine: number }}>
              {/* 行号列单独渲染：它是定位辅助，不是正文（复制载荷仍是 `source`）。 */}
              <Text style={styles.codeLineNumber} dataSet={{ codeLineNumber: number }}>
                {number}
              </Text>
              <Text style={styles.codeLineBody} dataSet={{ codeLineText: number }}>
                {line.map((run, runIndex) => (
                  <Text
                    key={runIndex}
                    style={run.family === null ? styles.codeRun : [styles.codeRun, tokenStyles[run.family]]}
                  >
                    {run.text}
                  </Text>
                ))}
              </Text>
            </View>
          );
        })}
      </View>
    </View>
  );
}
