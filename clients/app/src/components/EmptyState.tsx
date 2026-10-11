/**
 * 空态展示（共享展示原语）。
 *
 * `pwa-web-client` 的「两种空态可区分」要求「目录存在但无会话」与「本机没有任何目录」
 * 在文案与操作指引上不同。本组件承载这个**区分**，文案由调用方（目录页）给出，
 * 因此两种空态无法被合并成一句话。
 *
 * 迁移到 RNW 原语（`design.md` D1）：`View`/`Text` + `StyleSheet`，颜色/间距/字号来自
 * 主题（`useTheme()` 选浅/深两套调色板）；`data-empty-state` 原样经 `dataSet` 进 DOM。
 */

import type { ReactElement } from "react";
import type { TextProps } from "react-native";

import type { Palette } from "./theme";
import { StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface EmptyStateProps {
  /** 空态的种类标识；与 `src/domain/` 的 `DirectoryEmptyState` 对齐。 */
  readonly kind: string;
  /** 主文案。 */
  readonly title: string;
  /** 操作指引；两种空态必须给出不同内容。 */
  readonly guidance: string;
}

/**
 * 标题与指引仍然渲染成 `<h2>` / `<p>`，不能退化成 `<div>`。
 *
 * RNW 是**按 `role` 选语义标签**的（`AccessibilityUtil.propsToAccessibilityComponent`）：
 * `heading` + `aria-level: 2` → `<h2>`，`paragraph` → `<p>`。而 `react-native` 的公开类型里
 * 既没有 `paragraph` 这个 role、也没有 `aria-level` 这个键（RNW 比 RN 多认它们），因此这里
 * 显式声明一次、把这两个**运行时**属性原样透传：属性名与取值不得改名。
 *
 * 为什么必须保住这两个标签：R15 的浏览器用例在 `[data-empty-state]` 之下用 `h2`/`p` 读标题
 * 与指引（`e2e/page/checks/r15-directory.ts:174-175`），退化成 `div` 会让它读到空串，
 * 「两种空态可区分」这条负向断言的判别力随之消失。
 */
const TITLE_PROPS = { role: "heading", "aria-level": 2 } as unknown as TextProps;
const GUIDANCE_PROPS = { role: "paragraph" } as unknown as TextProps;

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    root: {
      backgroundColor: colors.panel,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      padding: spacing.lg,
      gap: spacing.sm,
      boxShadow: colors.shadow,
    },
    title: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.lg,
      fontWeight: "600",
    },
    guidance: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 渲染一个空态。 */
export function EmptyState({ kind, title, guidance }: EmptyStateProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View style={styles.root} dataSet={{ emptyState: kind }}>
      <Text {...TITLE_PROPS} style={styles.title}>
        {title}
      </Text>
      {/* 指引单独成段：两种空态的指引必须不同，合并成一句即违反 R15。 */}
      <Text {...GUIDANCE_PROPS} style={styles.guidance}>
        {guidance}
      </Text>
    </View>
  );
}
