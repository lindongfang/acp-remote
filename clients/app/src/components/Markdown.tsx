/**
 * Agent 产出的 Markdown → RN 视图（`design.md` D9 / spec「Agent 产生的富文本按不可信输入渲染」）。
 *
 * ## 为什么是「marked 的 token + 自建 RNW 映射」
 *
 * design D9 的探针顺序是 `react-native-marked` → `@ronradtke/react-native-markdown-display`
 * → `marked` + 自建映射。前两档在本工程的硬约束下都被探针判否：
 *
 * - `react-native-marked@8.3.2`：`MDImage` 默认就 `ImageBackground source={{ uri }}` +
 *   `Image.getSize(uri)`，即**默认以远端地址加载图片**；样式自带第二套配色（实测输出
 *   `#1976D2`/`rgba(51,51,51,1.00)`，`Markdown` 根还把 `FlatList` 底色写死成 `#ffffff`）；
 *   peer 要求 `react-native-svg`（第三个依赖，且是原生模块）；根节点是虚拟化 `FlatList`。
 * - `@ronradtke/react-native-markdown-display@9.0.3`：`allowedImageHandlers` 默认含
 *   `https://`/`http://`（同样默认加载远端图片，关掉则是**静默丢弃**而不是「降级或说明」）；
 *   代码块走 prism 自带主题 + 一套图标字体依赖。
 *
 * 两档要满足约束，就得把它们唯一有分量的那部分（图片/链接/代码/样式的渲染）全部替换掉，
 * 却仍要背上 2–4 MB 的包体与额外依赖。于是落到第三档：`marked` 只做**词法**
 * （零运行时依赖，不调用 `marked.parse`，因此不存在「先生成 HTML 再想办法不渲染它」这条路），
 * 本文件把 token 映射成 RN 视图；`highlight.js` 同理只用作 `CodeBlock` 的 token 源。
 *
 * ## 不可信输入边界（逐条对应 spec 的场景）
 *
 * - **原始 HTML 不生效**：`html` token 一律按**纯文本**渲染（`data-markdown-omitted="html"`），
 *   既不执行也不注入；`<script>` 与事件处理属性因此只会作为字面文本出现。
 * - **远端资源不被加载**：图片不渲染任何 image 元素，只出「未加载」说明；链接只出文本标签，
 *   不写 `href`、不调 `Linking`——本文件没有任何网络/导航调用。可选回调 `onLinkPress` 把目标
 *   上抛给页面，由页面决定安全口径（复制/内页跳转）。
 * - 代码块交给 `CodeBlock.tsx`（等宽 + 语言标记 + 行号 + 复制入口 + 高亮）。
 * - 不解析 `acp.rawJson`：本组件的输入只有 Markdown 文本本身。
 *
 * ## 样式
 *
 * 全部取自 `theme.ts` 的 token（`palettes` + `spacing`/`radius`/`fontSize`/`fontFamily`），
 * 浅/深两套样式表在模块加载时各建一次；渲染路径上不出现任何字面色值。
 *
 * 组件是纯函数：内容与回调都从 props 来，缺省等价于「不启用」。
 */

import { Fragment } from "react";
import type { ReactElement, ReactNode } from "react";
import type { TextStyle, ViewStyle } from "react-native";
import { lexer } from "marked";
import type { MarkedToken, Token, Tokens } from "marked";
import { CodeBlock } from "./CodeBlock";
import type { Palette } from "./theme";
import {
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

export interface MarkdownProps {
  /** Agent 产出的 Markdown 原文；缺省为空串（渲染成空视图）。 */
  readonly value?: string | undefined;
  /**
   * 链接被点选时上抛的目标；缺省时链接只呈现为文本。本组件**不**打开任何地址。
   */
  readonly onLinkPress?: ((href: string) => void) | undefined;
  /** 代码块的复制入口落点（页面接平台剪贴板能力）；缺省时复制按钮仍渲染，只是没有落点。 */
  readonly onCopyCode?: ((code: string, language: string) => void) | undefined;
}

/** 渲染上下文：一次渲染内共享的样式、回调与代码块计数器。 */
interface RenderContext {
  readonly styles: MarkdownStyles;
  readonly onLinkPress: ((href: string) => void) | undefined;
  readonly onCopyCode: ((code: string, language: string) => void) | undefined;
  readonly codeIndex: { next: number };
}

/**
 * 一套 Markdown 样式（浅/深各一份，由 `buildStyles` 产出）。
 *
 * 显式写出来而不是从 `buildStyles` 的返回值取：这是本模块的样式面名字，`RenderContext`
 * 与后续要复用的调用方都按它取用，不必反过来耦合到构造函数。
 */
export interface MarkdownStyles {
  readonly root: ViewStyle;
  readonly heading: TextStyle;
  readonly paragraph: TextStyle;
  readonly blockquote: ViewStyle;
  readonly list: ViewStyle;
  readonly listItem: ViewStyle;
  readonly listMarker: TextStyle;
  readonly listItemBody: ViewStyle;
  readonly table: ViewStyle;
  readonly tableRow: ViewStyle;
  readonly tableCell: ViewStyle;
  readonly tableHeadCell: ViewStyle;
  readonly tableCellText: TextStyle;
  readonly tableHeadText: TextStyle;
  readonly alignLeft: TextStyle;
  readonly alignCenter: TextStyle;
  readonly alignRight: TextStyle;
  readonly hr: ViewStyle;
  readonly strong: TextStyle;
  readonly em: TextStyle;
  readonly del: TextStyle;
  readonly codespan: TextStyle;
  readonly link: TextStyle;
  readonly omitted: TextStyle;
}

/** 链接的降级说明：说明「有一个链接、没有加载它」，不泄露也不写入目标地址。 */
const LINK_NOTE = "［外部链接未加载］";

/** 标题深度 → 字号档位（`depth` 从 1 起；1/2/3 各有档，4–6 归到 `sm`）。 */
const HEADING_SIZES: readonly number[] = [fontSize.xl, fontSize.lg, fontSize.md, fontSize.sm, fontSize.sm, fontSize.sm];

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette): MarkdownStyles {
  return StyleSheet.create({
    root: {
      flexDirection: "column",
      gap: spacing.sm,
    },
    heading: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontWeight: "bold",
    },
    paragraph: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: fontSize.md + 6,
    },
    blockquote: {
      gap: spacing.xs,
      borderLeftWidth: 3,
      borderLeftColor: colors.line,
      paddingLeft: spacing.md,
    },
    list: {
      gap: spacing.xs,
    },
    listItem: {
      flexDirection: "row",
      gap: spacing.sm,
    },
    listMarker: {
      minWidth: spacing.lg,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    listItemBody: {
      flexGrow: 1,
      flexShrink: 1,
      gap: spacing.xs,
    },
    table: {
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      overflow: "hidden",
    },
    tableRow: {
      flexDirection: "row",
      borderBottomColor: colors.line2,
      borderBottomWidth: 1,
    },
    tableCell: {
      flexGrow: 1,
      flexShrink: 1,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
    },
    tableHeadCell: {
      backgroundColor: colors.panel2,
    },
    tableCellText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    tableHeadText: {
      color: colors.ink,
      fontWeight: "bold",
    },
    alignLeft: {
      textAlign: "left",
    },
    alignCenter: {
      textAlign: "center",
    },
    alignRight: {
      textAlign: "right",
    },
    hr: {
      height: 1,
      backgroundColor: colors.line2,
    },
    strong: {
      fontWeight: "bold",
      color: colors.ink,
    },
    em: {
      fontStyle: "italic",
    },
    del: {
      textDecorationLine: "line-through",
    },
    codespan: {
      color: colors.ink,
      backgroundColor: colors.panel2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
      paddingHorizontal: spacing.xs,
      borderRadius: radius.sm,
    },
    link: {
      color: colors.accentInk,
      textDecorationLine: "underline",
    },
    /** 降级呈现（原始 HTML / 未加载的图片 / 未知 token）：等宽、弱化，只表「这里有内容」。 */
    omitted: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/**
 * 块级 token → RN 视图。未在下面的 `switch` 里显式处理的类型走 `default`：把 `raw` 当纯文本
 * 呈现（宁可多出一段原文，也不静默丢内容）。
 *
 * 先把 `Token` 收窄成 `MarkedToken`：`Token` 联合里还有 `Tokens.Generic`（`tokens?: Token[]`、
 * 属性全是 `any`），它会让每个分支里的 `token.tokens` 仍然可能是 `undefined`。本工程不注册任何
 * markdown 扩展，词法器只会产出下面这些形状；真出现未知形状时由 `default` 兜底。
 */
function renderBlocks(tokens: readonly Token[], key: string, ctx: RenderContext): readonly ReactNode[] {
  return tokens.map((token, index) => renderBlock(token, `${key}.${index}`, ctx));
}

function renderBlock(raw: Token, key: string, ctx: RenderContext): ReactNode {
  const token = raw as MarkedToken;
  const { styles } = ctx;

  switch (token.type) {
    case "space":
    case "def":
      // `space` 只是排版留白；`def` 是链接定义（引用型链接的 href 已在 link token 上），
      // 两者都不产生可见内容。
      return null;

    case "heading":
      return (
        <Text
          key={key}
          style={[styles.heading, { fontSize: HEADING_SIZES[token.depth - 1] ?? fontSize.sm }]}
          dataSet={{ markdownBlock: "heading" }}
        >
          {renderInlines(token.tokens, key, ctx)}
        </Text>
      );

    case "paragraph":
    case "text":
      return (
        <Text key={key} style={styles.paragraph} dataSet={{ markdownBlock: "paragraph" }}>
          {token.tokens === undefined ? token.text : renderInlines(token.tokens, key, ctx)}
        </Text>
      );

    case "blockquote":
      return (
        <View key={key} style={styles.blockquote} dataSet={{ markdownBlock: "blockquote" }}>
          {renderBlocks(token.tokens, key, ctx)}
        </View>
      );

    case "list": {
      const start = typeof token.start === "number" ? token.start : 1;
      return (
        <View key={key} style={styles.list} dataSet={{ markdownBlock: "list" }}>
          {token.items.map((item, index) => {
            // 任务列表的勾选态只在 marker 上出现一次（`checkbox` 内联 token 因此不渲染）。
            const marker = item.task
              ? item.checked === true
                ? "[x]"
                : "[ ]"
              : token.ordered
                ? `${start + index}.`
                : "•";

            return (
              <View key={index} style={styles.listItem} dataSet={{ markdownBlock: "list-item" }}>
                <Text style={styles.listMarker}>{marker}</Text>
                <View style={styles.listItemBody}>{renderBlocks(item.tokens, `${key}.${index}`, ctx)}</View>
              </View>
            );
          })}
        </View>
      );
    }

    case "code": {
      const codeIndex = ctx.codeIndex.next;
      ctx.codeIndex.next += 1;
      return (
        <CodeBlock
          key={key}
          code={token.text}
          language={token.lang ?? ""}
          index={codeIndex}
          onCopy={ctx.onCopyCode}
        />
      );
    }

    case "table":
      return (
        <View key={key} style={styles.table} dataSet={{ markdownBlock: "table" }}>
          <View style={styles.tableRow}>
            {token.header.map((cell, index) => renderTableCell(cell, `${key}.h${index}`, ctx))}
          </View>
          {token.rows.map((row, rowIndex) => (
            <View key={rowIndex} style={styles.tableRow}>
              {row.map((cell, cellIndex) => renderTableCell(cell, `${key}.${rowIndex}.${cellIndex}`, ctx))}
            </View>
          ))}
        </View>
      );

    case "hr":
      return <View key={key} style={styles.hr} dataSet={{ markdownBlock: "hr" }} />;

    case "html":
      // 原始 HTML 片段：按纯文本呈现，不解析、不执行。
      return (
        <Text key={key} style={styles.omitted} dataSet={{ markdownBlock: "raw", markdownOmitted: "html" }}>
          {token.text}
        </Text>
      );

    default:
      return (
        <Text key={key} style={styles.omitted} dataSet={{ markdownBlock: "unknown", markdownOmitted: "unknown" }}>
          {token.raw}
        </Text>
      );
  }
}

function renderTableCell(cell: Tokens.TableCell, key: string, ctx: RenderContext): ReactElement {
  const { styles } = ctx;
  const alignStyle =
    cell.align === "center" ? styles.alignCenter : cell.align === "right" ? styles.alignRight : styles.alignLeft;

  return (
    <View
      key={key}
      style={cell.header ? [styles.tableCell, styles.tableHeadCell] : styles.tableCell}
      dataSet={{ markdownCell: "true" }}
    >
      <Text style={[styles.tableCellText, cell.header ? styles.tableHeadText : null, alignStyle]}>
        {renderInlines(cell.tokens, key, ctx)}
      </Text>
    </View>
  );
}

/**
 * 行内 token → RN 视图。
 *
 * `text` 只转发子节点（不额外包一层 `Text`），`strong`/`em`/`del` 用嵌套 `Text` 表达——
 * 这样一段话在 DOM 里的层级与 Markdown 的结构一一对应，且不产生多余的容器。
 */
function renderInlines(tokens: readonly Token[], key: string, ctx: RenderContext): readonly ReactNode[] {
  return tokens.map((token, index) => renderInline(token, `${key}.${index}`, ctx));
}

function renderInline(raw: Token, key: string, ctx: RenderContext): ReactNode {
  // 同 `renderBlock`：收窄掉 `Tokens.Generic`，否则每个分支里的 `tokens` 都还挂着 `undefined`。
  const token = raw as MarkedToken;
  const { styles } = ctx;

  switch (token.type) {
    case "text":
      return (
        <Fragment key={key}>
          {token.tokens === undefined ? token.text : renderInlines(token.tokens, key, ctx)}
        </Fragment>
      );

    case "strong":
      return (
        <Text key={key} style={styles.strong}>
          {renderInlines(token.tokens, key, ctx)}
        </Text>
      );

    case "em":
      return (
        <Text key={key} style={styles.em}>
          {renderInlines(token.tokens, key, ctx)}
        </Text>
      );

    case "del":
      return (
        <Text key={key} style={styles.del}>
          {renderInlines(token.tokens, key, ctx)}
        </Text>
      );

    case "codespan":
      return (
        <Text key={key} style={styles.codespan} dataSet={{ markdownCodespan: "true" }}>
          {token.text}
        </Text>
      );

    case "br":
      return <Text key={key}>{"\n"}</Text>;

    case "escape":
      return <Text key={key}>{token.text}</Text>;

    case "checkbox":
      // 勾选态由所在列表项的 marker 呈现，这里不重复。
      return null;

    case "image":
      // 不渲染 image 元素、不引用任何地址：只说明「这里有一张远端图片未被加载」。
      return (
        <Text key={key} style={styles.omitted} dataSet={{ markdownOmitted: "image" }}>
          {`［远端图片未加载：${token.text.length === 0 ? "无描述" : token.text}］`}
        </Text>
      );

    case "link": {
      const label =
        token.tokens.length === 0 ? token.text : renderInlines(token.tokens, `${key}.label`, ctx);
      const note = <Text style={styles.omitted}>{LINK_NOTE}</Text>;
      const press = ctx.onLinkPress;

      if (press === undefined) {
        return (
          <Text key={key} style={styles.link} dataSet={{ markdownOmitted: "link" }}>
            {label}
            {note}
          </Text>
        );
      }

      // 只上抛目标（页面决定安全口径）；本组件不写 `href`、不打开地址。
      return (
        <Text
          key={key}
          role="link"
          style={styles.link}
          dataSet={{ markdownOmitted: "link", markdownLink: "true" }}
          onPress={() => {
            press(token.href);
          }}
        >
          {label}
          {note}
        </Text>
      );
    }

    case "html":
      return (
        <Text key={key} style={styles.omitted} dataSet={{ markdownOmitted: "html" }}>
          {token.text}
        </Text>
      );

    default:
      return (
        <Text key={key} style={styles.omitted} dataSet={{ markdownOmitted: "unknown" }}>
          {token.raw}
        </Text>
      );
  }
}

/** 一段 Agent 产出的 Markdown。 */
export function Markdown({ value, onLinkPress, onCopyCode }: MarkdownProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  // 代码块序号只在这一次渲染里有意义，因此放在渲染上下文里而不是模块作用域（保持纯函数）。
  const ctx: RenderContext = { styles, onLinkPress, onCopyCode, codeIndex: { next: 0 } };

  return (
    <View style={styles.root} dataSet={{ markdown: "true" }}>
      {renderBlocks(lexer(value ?? "", { gfm: true }), "md", ctx)}
    </View>
  );
}
