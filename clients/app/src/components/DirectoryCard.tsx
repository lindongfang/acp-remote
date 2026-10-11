/**
 * 目录卡片（目录页的目录项骨架）。
 *
 * **不接收模型对象**：只收扁平 props（展示名、别名、三项汇总、折叠态与两个回调）。这样
 * feature 层模型的形状变化不会穿透到这个展示原语——卡片只认得它渲染的那几个值。
 *
 * R15 的三条 MUST 在渲染上的落点：
 *
 * - **只显示展示名与别名**：卡片没有任何接收路径的 props，别名**始终**渲染（不放进
 *   「同名时才显示」的分支——那正是路径消歧得以藏身的地方）。
 * - **同名以别名区分**：`data-directory-alias` 只出现在卡片根节点这一个元素上，子元素不再
 *   重复这个属性，e2e 的 `[data-directory-alias]` 计数才不会因标签迁移而失真。
 * - **折叠不影响其它目录项**：折叠态由外部传入，卡片自身不持有状态。
 */

import type { ReactElement, ReactNode } from "react";

import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface DirectoryCardProps {
  readonly displayName: string;
  readonly alias: string;
  readonly counts: { readonly total: number; readonly active: number; readonly pending: number };
  readonly collapsed: boolean;
  readonly onToggleCollapsed: () => void;
  /** 「查看全部 N 个 ›」入口；`null` 表示不呈现（是否呈现由调用方按内联上限决定）。 */
  readonly onOpenAll: (() => void) | null;
  readonly children?: ReactNode;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    card: {
      backgroundColor: colors.panel,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      padding: spacing.md,
      gap: spacing.sm,
      boxShadow: colors.shadow,
    },
    head: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    caret: {
      minWidth: 24,
      minHeight: 24,
      alignItems: "center",
      justifyContent: "center",
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    caretGlyph: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    name: {
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.lg,
      fontWeight: "600",
    },
    alias: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
    },
    counts: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    badgeNeutral: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.xs,
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      borderWidth: 1,
      borderColor: colors.line2,
    },
    badgeOk: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.xs,
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.okSoft,
    },
    badgeWarn: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.xs,
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.warnSoft,
    },
    badgeNeutralText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    badgeOkText: {
      color: colors.ok,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    badgeWarnText: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    openAll: {
      marginLeft: "auto",
      paddingVertical: 4,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
    },
    openAllText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    body: {
      gap: spacing.xs,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 一个目录项。 */
export function DirectoryCard({
  displayName,
  alias,
  counts,
  collapsed,
  onToggleCollapsed,
  onOpenAll,
  children,
}: DirectoryCardProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    // 根节点带 `role="listitem"`：RNW 渲染成 `<li>`，调用方的目录列表容器必须标 `role="list"`。
    <View
      role="listitem"
      style={styles.card}
      dataSet={{ directoryAlias: alias, directoryCollapsed: String(collapsed) }}
    >
      <View style={styles.head}>
        <Pressable
          role="button"
          accessibilityState={{ expanded: !collapsed }}
          accessibilityLabel={collapsed ? "展开目录" : "折叠目录"}
          style={styles.caret}
          dataSet={{ action: "toggle-directory" }}
          onPress={onToggleCollapsed}
        >
          <Text style={styles.caretGlyph}>{collapsed ? "▸" : "▾"}</Text>
        </Pressable>
        <Text style={styles.name} dataSet={{ directoryDisplayName: displayName }} numberOfLines={1}>
          {displayName}
        </Text>
        {/* 展示名可以重复；别名始终展示，是区分同名目录的唯一依据。 */}
        <Text style={styles.alias} dataSet={{ directoryAliasLabel: "true" }} numberOfLines={1}>
          {alias}
        </Text>
        {/* 三项汇总来自会话摘要，因此断连时仍然呈现；筛选也不改变它们（spec 的筛选场景）。 */}
        <View style={styles.counts}>
          <View style={styles.badgeNeutral}>
            <Text style={styles.badgeNeutralText}>会话</Text>
            <Text style={styles.badgeNeutralText} dataSet={{ directoryCount: "total" }}>
              {String(counts.total)}
            </Text>
          </View>
          <View style={styles.badgeOk}>
            <Text style={styles.badgeOkText}>运行中</Text>
            <Text style={styles.badgeOkText} dataSet={{ directoryCount: "active" }}>
              {String(counts.active)}
            </Text>
          </View>
          <View style={styles.badgeWarn}>
            <Text style={styles.badgeWarnText}>待处理</Text>
            <Text style={styles.badgeWarnText} dataSet={{ directoryCount: "pending" }}>
              {String(counts.pending)}
            </Text>
          </View>
        </View>
        {onOpenAll === null ? null : (
          <Pressable
            role="button"
            style={styles.openAll}
            dataSet={{ action: "open-directory" }}
            onPress={onOpenAll}
          >
            <Text style={styles.openAllText}>{`查看全部 ${String(counts.total)} 个 ›`}</Text>
          </Pressable>
        )}
      </View>
      {/* 折叠时只保留展示名与汇总：会话行（children）不再渲染。 */}
      {collapsed ? null : <View style={styles.body}>{children}</View>}
    </View>
  );
}
