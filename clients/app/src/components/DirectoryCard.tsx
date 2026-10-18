/**
 * 目录卡片（目录页的目录项骨架）。
 *
 * **不接收模型对象**：只收扁平 props（展示名、同名消歧尾段、三项汇总、折叠态与几个回调）。
 * 这样 feature 层模型的形状变化不会穿透到这个展示原语——卡片只认得它渲染的那几个值。
 *
 * ## 卡片头的槽位与原型 `viewDirs()` 的 `dhead` 逐项对齐
 *
 * 折叠箭头 → 目录图标 → 展示名（进入详情页的入口）→ 同名时的路径尾段 → 计数胶囊 →
 * 运行中徽标 → 待处理徽标 → 弹性空白 → `⊕ 新建` → `查看全部 ›`。
 *
 * - **目录标识不进界面**：卡片只把 `directoryId` 挂在根节点的 `data-directory-id` 上（路由与命令引用的
 *   内部标识、e2e 的锚点），**不**渲染成任何文字。相邻目录是否同名由别的槽位（路径尾段）区分。
 * - **路径尾段只逐字搬运**：`pathTail` 由 feature 层判定并在「同名」时给出（展示名唯一或没有
 *   下发路径时为 `null`）。卡片不拼、不规范化、也不从展示名或别名反推它——因此「展示名唯一时
 *   MUST NOT 呈现路径」这条不需要卡片再判断一次。
 * - **三项汇总由会话摘要得出**（断连时仍可呈现）：计数胶囊恒呈现会话总数；运行中与待处理
 *   徽标只在**非零**时追加（原型同此），零值不占槽位。
 * - **目录标识只出现一次**：`data-directory-id` 只在根节点这**一个**元素上，子元素（含标题入口与
 *   「⊕ 新建」）不再重复——e2e 的 `[data-directory-id]` 计数才不会因标签迁移而失真。
 *
 * ## 两个进入详情页的入口
 *
 * - **展示名**：`data-action="open-directory"` → `onOpen`。它是「每个目录项都可进入详情页」的
 *   保证——**与会话数无关**，0 会话的目录也点得进去。
 * - **「查看全部 ›」**：`data-action="open-directory-all"` → `onOpenAll`，只在有会话时由调用方
 *   给出（`null` 即不呈现）。两者指向同一页面但语义不同，因此动作取值必须可区分。
 *
 * ## 卡体是缩进树
 *
 * `children`（内联会话行）挂在带左缩进与左边线的卡体里（原型 `.dbody`）；折叠时卡体整块不渲染，
 * 且折叠态由外部传入，卡片自身不持有状态——一个目录项的折叠因此不影响其它目录项。
 */

import type { ReactElement, ReactNode } from "react";

import type { Palette } from "./theme";
import {
  Pressable,
  StyleSheet,
  Text,
  View,
  fontFamily,
  fontSize,
  fontWeight,
  palettes,
  radius,
  spacing,
  useTheme,
} from "./theme";

export interface DirectoryCardProps {
  readonly displayName: string;
  /**
   * 目录标识：**只**作路由与命令引用的内部标识（渲染成根节点的 `data-directory-id`），
   * MUST NOT 呈现为界面文字。
   */
  readonly directoryId: string;
  /**
   * 同名目录的消歧尾段（逐字取自服务端下发的路径，feature 层已判定「同名才给」）；
   * 展示名唯一或没有下发路径时为 `null`，此时**不呈现**任何路径片段。
   */
  readonly pathTail: string | null;
  readonly counts: { readonly total: number; readonly active: number; readonly pending: number };
  readonly collapsed: boolean;
  readonly onToggleCollapsed: () => void;
  /** 进入该目录详情页（展示名入口）；必需——它就是「每个目录项都可进入」的保证。 */
  readonly onOpen: () => void;
  /** 「查看全部 ›」入口；`null` 表示不呈现（是否呈现由调用方按会话数决定）。 */
  readonly onOpenAll: (() => void) | null;
  /** 「⊕ 新建」入口；`null`/缺省表示不呈现（页面无条件给出，不再有权限维度）。 */
  readonly onCreate?: (() => void) | null;
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
      overflow: "hidden",
      boxShadow: colors.shadow,
    },
    // 原型 `.dhead`：卡片头自成一栏（`panel-2` 底 + 下边框），不再跟卡片共用一层内边距。
    head: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.md,
      paddingVertical: 15,
      paddingHorizontal: 18,
      backgroundColor: colors.panel2,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    // 原型 `.caret`：只是一个 14px 的箭头，没有底色块（折叠态由字形切换表达）。
    caret: {
      width: 14,
      flexShrink: 0,
      alignItems: "center",
      justifyContent: "center",
    },
    caretGlyph: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
    },
    // 目录图标：原型 `.dicon` 是「accent-soft 底 + accent-ink 字」的小方块。
    icon: {
      width: 26,
      height: 26,
      alignItems: "center",
      justifyContent: "center",
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
    },
    iconGlyph: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    nameButton: {
      // 展示名从 `Text` 变成可点元素后，截断仍要生效：与 `SessionRow` 的 `main` 同一手法。
      flexShrink: 1,
      minWidth: 0,
    },
    name: {
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      // 原型 `.dname`：15px（比正文大一档，但小于详情页标题）。
      fontSize: 15,
      lineHeight: 24,
      letterSpacing: -0.15,
      fontWeight: "600",
    },
    // 同名消歧尾段：等宽小字、弱化，紧跟在展示名之后（原型 `.dpath`）。
    pathTail: {
      flexShrink: 1,
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: 11.5,
      lineHeight: 18.4,
    },
    countChip: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.xs,
      paddingVertical: 2,
      paddingHorizontal: 9,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.lg,
      backgroundColor: colors.panel,
    },
    countChipText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: 12,
      lineHeight: 19.2,
    },
    badgeOk: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: 6,
      paddingVertical: spacing.xs,
      paddingHorizontal: 10,
      borderRadius: radius.lg,
      backgroundColor: colors.okSoft,
    },
    badgeWarn: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: 6,
      paddingVertical: spacing.xs,
      paddingHorizontal: 10,
      borderRadius: radius.lg,
      backgroundColor: colors.warnSoft,
    },
    badgeOkText: {
      color: colors.ok,
      fontFamily: fontFamily.sans,
      fontSize: 12,
      lineHeight: 19.2,
    },
    badgeWarnText: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: 12,
      lineHeight: 19.2,
    },
    // 弹性空白：把创建与「查看全部」两个入口推到卡片头右侧（原型 `.spacer`）。
    spacer: {
      flexGrow: 1,
    },
    // 原型 `.linkbtn.add`：淡主色底 + 主色字 + 半粗，是卡片头里唯一有底色的入口。
    create: {
      paddingVertical: 5,
      paddingHorizontal: 11,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
    },
    createText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
      fontWeight: fontWeight.medium,
    },
    // 原型 `.linkbtn.more`：透明底、弱化文字；与「⊕ 新建」不抢同一个视觉层级。
    openAll: {
      paddingVertical: spacing.xs,
      paddingHorizontal: 6,
      borderRadius: radius.sm,
    },
    openAllText: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    // 卡体：缩进 + 左边线（原型 `.dbody{margin-left:30px;border-left:2px solid var(--line-2)}`）。
    body: {
      marginLeft: 30,
      borderLeftWidth: 2,
      borderLeftColor: colors.line2,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 一个目录项。 */
export function DirectoryCard({
  displayName,
  directoryId,
  pathTail,
  counts,
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
      dataSet={{ directoryId, directoryCollapsed: String(collapsed) }}
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
        <View style={styles.icon}>
          <Text style={styles.iconGlyph}>▤</Text>
        </View>
        {/* 展示名是进入详情页的入口，且**与会话数无关**（0 会话的目录也点得进去）。
            `data-directory-id` 不挂在这里：它只在卡片根节点出现一次（R15 数它的个数）。 */}
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
        {/* 同名消歧：只呈现 feature 层给出的路径尾段（展示名唯一时它必为 `null`——那时这一格
            整块不存在，因此「展示名唯一时不呈现路径」是一条结构上的性质，不是这里的分支）。 */}
        {pathTail === null ? null : (
          <Text style={styles.pathTail} dataSet={{ directoryPathTail: pathTail }} numberOfLines={1}>
            {pathTail}
          </Text>
        )}
        {/* 会话总数：三项汇总都来自会话摘要，因此断连时仍然呈现；筛选也不改变它们。 */}
        <View style={styles.countChip}>
          <Text style={styles.countChipText} dataSet={{ directoryCount: "total" }}>
            {String(counts.total)}
          </Text>
          <Text style={styles.countChipText}>个</Text>
        </View>
        {/* 运行中 / 待处理：只在非零时追加（原型同此），零值不占槽位。 */}
        {counts.active === 0 ? null : (
          <View style={styles.badgeOk}>
            <Text style={styles.badgeOkText}>● 运行中</Text>
            <Text style={styles.badgeOkText} dataSet={{ directoryCount: "active" }}>
              {String(counts.active)}
            </Text>
          </View>
        )}
        {counts.pending === 0 ? null : (
          <View style={styles.badgeWarn}>
            <Text style={styles.badgeWarnText}>⏸ 待处理</Text>
            <Text style={styles.badgeWarnText} dataSet={{ directoryCount: "pending" }}>
              {String(counts.pending)}
            </Text>
          </View>
        )}
        <View style={styles.spacer} />
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
            <Text style={styles.openAllText}>查看全部 ›</Text>
          </Pressable>
        )}
      </View>
      {/* 折叠时只保留展示名与汇总：卡体（会话行或空行）整块不再渲染。 */}
      {collapsed ? null : <View style={styles.body}>{children}</View>}
    </View>
  );
}
