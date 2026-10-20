/**
 * 结构化差异行的**唯一**呈现组件（spec「变更文件的呈现边界」；design D6/D8）。
 *
 * 差异行只来自服务端随事件下发的结构化来源，三种形态互不冒充：
 *
 * - `lines`：逐行呈现，按 `kind` 补 `+ ` / `- ` / 两个空格前缀（`text` 本身不带前缀），
 *   新增/删除/上下文各一档底色；行内容按**不可信文本**处理（只经 `Text` 呈现，不解析标记、
 *   不执行链接、不插 HTML）。
 * - `unavailable`：服务端说明了为什么没有 → 呈现 `FILE_DIFF_UNAVAILABLE_LABELS` 里该原因的
 *   已登记文案。
 * - `null`：两者都没有 → **明确说明**原文未同步或超出单条上限，不渲染占位差异行假装完整。
 *
 * 本组件不解析 `acp.rawJson`：原文的唯一去处是各条目自己的诊断入口。时间线与其他消费方复用
 * 同一份呈现，不在两个组件里各写一套。
 */

import type { ReactElement } from "react";

import type { DiffLineView, FileDiffView } from "../domain";
import { FILE_DIFF_UNAVAILABLE_LABELS } from "../domain";
import type { Palette } from "./theme";
import { StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

function buildStyles(colors: Palette) {
  return StyleSheet.create({
    note: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    /** 差异区：一列等宽行，新增/删除各一档底色、上下文行用弱化色（原型 `.diff`）。 */
    diff: {
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      overflow: "hidden",
    },
    diffLine: {
      paddingVertical: 1,
      paddingHorizontal: spacing.sm,
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    diffAdded: {
      color: colors.ok,
      backgroundColor: colors.okSoft,
    },
    diffDeleted: {
      color: colors.danger,
      backgroundColor: colors.dangerSoft,
    },
    diffContext: {
      color: colors.ink3,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 差异行的三档底色（词表封闭：`domain/timeline.ts` 的 `DiffLineKind`）。 */
const DIFF_LINE_STYLES: Readonly<Record<DiffLineView["kind"], "diffAdded" | "diffDeleted" | "diffContext">> = {
  added: "diffAdded",
  deleted: "diffDeleted",
  context: "diffContext",
};

/**
 * 差异行的行首前缀：`text` 是**不带前缀**的行内容（`crates/core/src/derive.rs` 的 `DiffLine`），
 * 因此 `+` / `-` / 两个空格由客户端按判别子补上——三种档位在纯文本复制后也分得清。
 */
const DIFF_LINE_PREFIXES: Readonly<Record<DiffLineView["kind"], string>> = {
  added: "+ ",
  deleted: "- ",
  context: "  ",
};

/** 一处改动的差异区：三种形态各一条分支，且**互不冒充**。 */
export function ChangeDiff({ diff }: { readonly diff: FileDiffView | null }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  if (diff === null) {
    return (
      <Text style={styles.note} dataSet={{ diffNotDelivered: "true" }}>
        这一处没有随事件下发结构化差异行：原文未同步，或超出单条上限。这里不渲染占位差异行，完整原文的入口在下方诊断里。
      </Text>
    );
  }

  if (diff.kind === "unavailable") {
    return (
      <Text style={styles.note} dataSet={{ diffUnavailable: diff.reason }}>
        {FILE_DIFF_UNAVAILABLE_LABELS[diff.reason]}
      </Text>
    );
  }

  return (
    <View style={styles.diff} dataSet={{ diffRegion: "lines", diffLineCount: diff.lines.length }}>
      {diff.lines.map((line, index) => (
        <Text
          // 差异行没有标识字段，按声明次序做键（它只是呈现，不参与任何判定）。
          key={`diff:${String(index)}`}
          style={[styles.diffLine, styles[DIFF_LINE_STYLES[line.kind]]]}
          dataSet={{ diffLineKind: line.kind, diffLinePrefix: DIFF_LINE_PREFIXES[line.kind] }}
        >
          {`${DIFF_LINE_PREFIXES[line.kind]}${line.text}`}
        </Text>
      ))}
    </View>
  );
}
