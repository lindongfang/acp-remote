/**
 * 目录卡片（目录页的目录项骨架）。
 *
 * **不接收模型对象**：只收扁平 props（展示名、别名、三项汇总、最近活动时间、折叠态与几个
 * 回调）。这样 feature 层模型的形状变化不会穿透到这个展示原语——卡片只认得它渲染的那几个值。
 *
 * R15 的三条 MUST 在渲染上的落点：
 *
 * - **只显示展示名与别名**：卡片没有任何接收路径的 props，别名**始终**渲染（不放进
 *   「同名时才显示」的分支——那正是路径消歧得以藏身的地方）。
 * - **同名以别名区分**：`data-directory-alias` 只出现在卡片根节点这一个元素上，子元素不再
 *   重复这个属性，e2e 的 `[data-directory-alias]` 计数才不会因标签迁移而失真。
 *   标题入口与「⊕ 新建」（都在卡片内）**不得**把那属性挪到按钮上——计数会被它们扰乱。
 * - **折叠不影响其它目录项**：折叠态由外部传入，卡片自身不持有状态。
 *
 * ## 两个进入详情页的入口（design D3）
 *
 * - **标题**：`data-action="open-directory"` → `onOpen`。它是「每个目录项都可进入详情页」的
 *   保证——**与会话数无关**，0 会话的目录也点得进去。
 * - **「查看全部 N 个 ›」**：`data-action="open-directory-all"` → `onOpenAll`，只在有会话时由
 *   调用方给出（`null` 即不呈现）。两者指向同一页面但语义不同，因此动作取值必须可区分。
 *
 * ## 另外两处呈现（原型的 `dwhen` 与 `data-new`）
 *
 * - **最近活动时间**：`latestActivityAt` 由 feature 层取该目录下会话 `updatedAt` 的**最大值**
 *   （卡片不做任何比较，只把值画出来）；`null`/缺省表示该目录没有会话，**不呈现**该元素，
 *   也不拿当前时间顶替。
 * - **「⊕ 新建」**：`onCreate` 为 `null`/缺省时不呈现。设备不再有创建会话的权限维度后，
 *   页面无条件给出回调，卡片刻意不认权限——判定本就不该在这里。
 */

import type { ReactElement, ReactNode } from "react";

import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface DirectoryCardProps {
  readonly displayName: string;
  readonly alias: string;
  readonly counts: { readonly total: number; readonly active: number; readonly pending: number };
  /** 该目录下最晚的会话时间（ISO 原值）；`null`/缺省表示无会话，不呈现该元素。 */
  readonly latestActivityAt?: string | null;
  readonly collapsed: boolean;
  readonly onToggleCollapsed: () => void;
  /** 进入该目录详情页（标题入口）；必需——它就是「每个目录项都可进入」的保证。 */
  readonly onOpen: () => void;
  /** 「查看全部 N 个 ›」入口；`null` 表示不呈现（是否呈现由调用方按内联上限决定）。 */
  readonly onOpenAll: (() => void) | null;
  /** 「⊕ 新建」入口；`null`/缺省表示不呈现（页面无条件给出，不再有权限维度）。 */
  readonly onCreate?: (() => void) | null;
  readonly children?: ReactNode;
}

/**
 * 最近活动时间的展示形式：与目录页的同步时刻**同一口径**——按 UTC 值固定位置切片
 * （`YYYY-MM-DD HH:MM UTC`）。
 *
 * 刻意不用 `Date` 的本地时区格式化、也不画「N 分钟前」：前者会在不同时区的机器之间漂移，
 * 后者需要一个时钟，而 `renderToStaticMarkup` 与浏览器断言都不该依赖运行时刻——同一份
 * 视图模型必须在任何机器上渲染出同一段文本。精确值另经 `data-directory-latest-at` 原样进 DOM。
 */
function formatLatestActivity(latestActivityAt: string): string {
  return `最近 ${new Date(latestActivityAt).toISOString().slice(0, 16).replace("T", " ")} UTC`;
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
    nameButton: {
      // 标题从 `Text` 变成可点元素后，截断仍要生效：与 `SessionRow` 的 `main` 同一手法。
      flexShrink: 1,
      minWidth: 0,
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
    latestAt: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    create: {
      paddingVertical: 4,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      borderWidth: 1,
      borderColor: colors.line2,
      backgroundColor: colors.panel2,
    },
    createText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
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
  latestActivityAt = null,
  collapsed,
  onToggleCollapsed,
  onOpen,
  onOpenAll,
  onCreate = null,
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
        {/* 标题是进入详情页的入口，且**与会话数无关**（0 会话的目录也点得进去）。
            `data-directory-alias` 不挂在这里：它只在卡片根节点出现一次（R15-1 数它的个数）。 */}
        <Pressable
          role="button"
          style={styles.nameButton}
          dataSet={{ action: "open-directory" }}
          onPress={onOpen}
        >
          <Text style={styles.name} dataSet={{ directoryDisplayName: displayName }} numberOfLines={1}>
            {displayName}
          </Text>
        </Pressable>
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
        {/* 最近活动时间：值是 feature 层取的**最大值**，卡片自己不做比较；无会话时不呈现
            （也不拿当前时间顶替——spec 的 MUST NOT）。 */}
        {latestActivityAt === null ? null : (
          <Text style={styles.latestAt} dataSet={{ directoryLatestAt: latestActivityAt }}>
            {formatLatestActivity(latestActivityAt)}
          </Text>
        )}
        {/* 目录内创建会话：与详情页的创建入口共用同一条命令；设备不再有创建权限维度，
            因此这里只是个入口回调——页面无条件给出。 */}
        {onCreate === null ? null : (
          <Pressable
            role="button"
            style={styles.create}
            dataSet={{ action: "create-session" }}
            onPress={onCreate}
          >
            <Text style={styles.createText}>⊕ 新建</Text>
          </Pressable>
        )}
        {onOpenAll === null ? null : (
          <Pressable
            role="button"
            style={styles.openAll}
            dataSet={{ action: "open-directory-all" }}
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
