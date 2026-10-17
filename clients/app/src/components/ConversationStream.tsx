/**
 * 对话页正文（三栏骨架 + 会话头 + 输入区；spec「对话页的宽窄两种形态」「对话页左栏呈现同目录会话」
 * 「跳转到最近的用户输入」「未上报的上下文不得以零冒充」「Agent 目录与连接状态分两层呈现」
 * 「断连时标注上次同步时间」；`design.md` D7/D8/D9/D10）。
 *
 * ## 一页 = 三栏，三栏内容**始终在 DOM**
 *
 * 宽屏的正文是 `300 / 1fr / 360` 三栏（同目录会话栏、时间线、变更文件栏），窄屏把两侧栏目降为
 * **可切换的次级视图**。两种形态的差别只在样式上：三栏始终都在 DOM 里，未选中的栏只是
 * `display: none`（藏哪几栏由 `hiddenConversationColumns` 这**唯一**一份判据给出，组件不另写
 * 一套）。因此「窄屏不丢信息」是一条可断言的事实（两种形态的 `data-*` 集合在同一交互状态下完全
 * 相同），而不是靠自觉。
 *
 * 会话头、切换控件与输入区留在三栏**之外**：次级视图把中栏藏起来时，标题、切回去的入口与
 * 发送消息的地方都不能跟着消失。
 *
 * ## 判定不在本组件里
 *
 * 输入区能不能提交、编辑框是否只读、模型能不能切、差异行有没有、左栏列哪些会话，全部由
 * `ConversationPageModel` 里的判别值决定（`src/features/**` 与 `src/domain/**` 的纯函数）。
 * 本组件只做两件事：按模型渲染、把交互原样上抛。唯一落在组件里的判定是**视口测量**（跳转入口）
 * ——它读的是滚动位置这种只属于渲染现场的事实，与数据判定无关。
 *
 * ## 跳转入口与「短暂高亮」
 *
 * 时间线在视口外还有用户输入时呈现跳转胶囊（原型 `.jumppill`）；方向与剩余条数由
 * {@link classifyUserInputs} / {@link jumpPillOf} 给出（纯函数，组件测试直接喂几何）。跳转后用
 * 一圈描边把目标框住 `motion.pulse` 毫秒——目标条目属于 `ConversationTimeline` 的子树，本组件
 * 不为它加样式，因此高亮是一层 `position: fixed` 的覆盖层，坐标取自目标自己的
 * `getBoundingClientRect()`（网页专有面；node 侧不做测量，那时跳转入口不呈现）。
 *
 * ## 内容来源
 *
 * 未在线时顶部呈现条状标注：内容来自上次同步 + 该次同步的时刻（spec「断连时标注上次同步时间」）。
 * 「来自上次同步」这一段文案取自 `directory-model.ts` 的**唯一**词表，不在本文件另写一份；
 * 连接状态仍然只由共享外壳的设备胶囊表达，这里呈现的是**内容来源**，不是第二个状态徽标。
 */

import { useEffect, useRef, useState } from "react";
import type { ReactElement } from "react";

import type { InteractionAnswer } from "../domain/interaction";
import {
  CONVERSATION_SECONDARY_VIEWS,
  DEFAULT_CONVERSATION_SECONDARY_VIEW,
  hiddenConversationColumns,
} from "../features/conversation-model";
import type { ConversationPageModel, ConversationSecondaryView, ConversationSidebarModel } from "../features/conversation-model";
import { CONTENT_ORIGIN_LABELS } from "../features/directory-model";
import type { HostPanelModel } from "../features/host-panel-model";
import type { ConnectionStateName } from "../state";
import { ConversationComposer } from "./ConversationComposer";
import { ConversationTimeline } from "./ConversationTimeline";
import { FileChangesPanel } from "./FileChangesPanel";
import { PendingInteractionsPanel } from "./PendingInteractionsPanel";
import { SegmentControl } from "./SegmentControl";
import { SessionRow } from "./SessionRow";
import { StatusPill } from "./StatusPill";
import type { Palette } from "./theme";
import {
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
  breakpoints,
  fontFamily,
  fontSize,
  fontWeight,
  motion,
  palettes,
  radius,
  spacing,
  useTheme,
  useWindowDimensions,
  webStyle,
  zIndex,
} from "./theme";

/**
 * 次级视图类型的**定义**在视图模型层（`features/conversation-model.ts`：`all | timeline | sessions
 * | files`）；这里只把它转发出去，让既有消费方（`./index` 的导出面）不必换 import 路径。
 */
export type { ConversationSecondaryView } from "../features/conversation-model";

export interface ConversationStreamProps {
  readonly model: ConversationPageModel;
  /** 向上翻页（消息）：续取 `(createdAt, messageId)` 复合游标。 */
  readonly onLoadEarlier: () => void;
  /** 打开某条降级事件的诊断抽屉。 */
  readonly onOpenDiagnostics: (eventId: string) => void;
  /** 向上翻页（结构化历史）：续取会话内事件序号；缺省时不渲染历史翻页入口。 */
  readonly onLoadEarlierHistory?: (() => void) | undefined;
  /** 草稿变化（页面写进仓库；草稿只在内存）。 */
  readonly onDraftChange?: ((text: string) => void) | undefined;
  /** 提交当前草稿（页面派发 `session.prompt`）。 */
  readonly onSubmitPrompt?: ((draft: string) => void) | undefined;
  /** 取消当前轮次（页面派发 `session.cancel`）。 */
  readonly onCancelTurn?: (() => void) | undefined;
  /** 授权/提问的应答（页面派发 `permission.resolve` / `elicitation.respond`）。 */
  readonly onAnswerInteraction?: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
  /** 切换模型（`configId` 由设置模型给出，页面派发 `session.config.set`）。 */
  readonly onSelectModel?: ((configId: string, value: string) => void) | undefined;
  /** 切换模式（页面派发 `session.mode.set`）。 */
  readonly onSelectMode?: ((modeId: string) => void) | undefined;
  /** 打开 / 收起某处改动的诊断（`null` = 收起）。 */
  readonly onOpenChangeDiagnostics?: ((eventId: string | null) => void) | undefined;
  /** 当前展开诊断的改动（`diagnostics.eventId`）。 */
  readonly openChangeDiagnosticsEventId?: string | null;
  /**
   * 折叠块的显式展开状态（键为 `entry.key`）：覆盖由条目状态推出的默认值。
   * 与 `openChangeDiagnosticsEventId` 同构——页面级视图状态，本组件只透传（D10）。
   */
  readonly expandedTimelineEntries?: Readonly<Record<string, boolean>> | undefined;
  /** 折叠块表头被点选（上抛 `entry.key` 与切换后的展开值）；缺省时表头不可点。 */
  readonly onToggleTimelineEntry?: ((key: string, expanded: boolean) => void) | undefined;
  /**
   * 已选中的那一处改动（`changeId`）。时间线上带文件标识的条目与右栏改动清单**共用**这一份选中
   * 状态（spec「从条目联动到右栏」）；缺省 `null` = 没有选中。
   */
  readonly selectedChangeId?: string | null;
  /** 选中一处改动（上抛 `changeId`）；缺省时不渲染对应的可点控件。 */
  readonly onSelectChange?: ((changeId: string) => void) | undefined;
  /** 次级视图（仓库里的单份 UI 状态）；缺省时按宽度取默认值。 */
  readonly secondaryView?: ConversationSecondaryView | null;
  /** 次级视图切换；缺省时不渲染切换控件（那时三栏都呈现，见文件头）。 */
  readonly onSecondaryViewChange?: ((view: ConversationSecondaryView) => void) | undefined;
  /** 可用宽度（像素）；缺省取 `useWindowDimensions()`。渲染测试据此注入窄/宽两档。 */
  readonly availableWidth?: number | undefined;
  /** 代码块复制入口的落点。 */
  readonly onCopyCode?: ((code: string, language: string) => void) | undefined;
  /** 链接被点选时上抛的目标。 */
  readonly onLinkPress?: ((href: string) => void) | undefined;
  /** 左栏「全部 ›」：进入当前会话所属目录的详情页。 */
  readonly onOpenDirectory?: ((directoryId: string) => void) | undefined;
  /** 左栏会话行：打开同目录的另一个会话。 */
  readonly onOpenSession?: ((sessionId: string) => void) | undefined;
  /** 左栏目录内搜索词变化（**只**影响左栏列表）。 */
  readonly onSidebarQueryChange?: ((query: string) => void) | undefined;
  /** 左栏「加载更多」：再呈现一批同目录会话。 */
  readonly onLoadMoreSidebar?: (() => void) | undefined;
  /** 左栏底部的「⊕ 新建会话」：在该目录内创建会话。 */
  readonly onCreateSession?: ((directoryId: string) => void) | undefined;
  /**
   * 主机与连接面板模型：会话头的 Agent 胶囊**沿用同一份**连接事件覆盖层
   * （`agents` 列表本体来自快照，连接状态是可选覆盖层；覆盖层缺席时每行是「状态未知」）。
   */
  readonly hostPanel?: HostPanelModel | null;
  /** 连接状态机的当前状态（内容来源条据此判定）；`null` = 运行时尚未装配。 */
  readonly connectionState?: ConnectionStateName | null;
  /** 上次成功同步的时刻（epoch 毫秒）；从未成功替换过快照时为 `null`。 */
  readonly lastSyncedAtMs?: number | null;
}

/** 次级视图 → 切换控件上的文案（四个取值一个不少，键集合由类型保证）。 */
const SECONDARY_VIEW_LABELS: Readonly<Record<ConversationSecondaryView, string>> = {
  all: "全部",
  timeline: "对话",
  sessions: "会话",
  files: "变更文件",
};

/** 次级视图切换控件的选项：取值的清单与顺序都来自模型层的登记表，不在这里另列一份。 */
const SECONDARY_VIEW_OPTIONS: readonly { readonly value: ConversationSecondaryView; readonly label: string }[] =
  CONVERSATION_SECONDARY_VIEWS.map((value) => ({ value, label: SECONDARY_VIEW_LABELS[value] }));

/** 会话状态 → 待处理跳转条的文案；只有「在等我们」的那两档给得出。 */
const PENDING_BAR_LABELS: Readonly<Partial<Record<ConversationPageModel["header"]["state"], string>>> = {
  waiting_permission: "正在等待授权",
  waiting_input: "正在等待你的回复",
};

/**
 * 视口里的一条用户输入：相对**视口**的上边与下边（像素）。
 *
 * 浏览器给的是 `getBoundingClientRect()` 的原始值；组件测试直接喂假几何，因此下面的判定函数
 * 不需要 DOM。
 */
export interface ViewportRect {
  readonly top: number;
  readonly bottom: number;
}

/** 视口边界的容差：与原型 `updateJump` 的 10px 同一口径（贴着边不算看到）。 */
export const JUMP_VIEWPORT_GUARD_PX = 10;

/** 用户输入在视口里的分布：看得到的条数，以及上方 / 下方各是哪几条（按时间线次序的下标）。 */
export interface UserInputDistribution {
  readonly visible: number;
  readonly above: readonly number[];
  readonly below: readonly number[];
}

/** 把用户输入条目按「视口上方 / 视口内 / 视口下方」分类（跳转判定的**唯一**一处输入）。 */
export function classifyUserInputs(rects: readonly ViewportRect[], viewportHeight: number): UserInputDistribution {
  const above: number[] = [];
  const below: number[] = [];
  let visible = 0;
  rects.forEach((rect, index) => {
    if (rect.bottom <= JUMP_VIEWPORT_GUARD_PX) above.push(index);
    else if (rect.top >= viewportHeight - JUMP_VIEWPORT_GUARD_PX) below.push(index);
    else visible += 1;
  });
  return { visible, above, below };
}

/**
 * 跳转胶囊的呈现：**没有用户输入**、或至少有一条在视口内时不呈现（spec 的两条场景）。
 *
 * 方向取「还有输入的那一侧」：上方有则向上，否则向下；条数是那一侧剩下的条数。
 */
export function jumpPillOf(
  distribution: UserInputDistribution,
): { readonly direction: "up" | "down"; readonly remaining: number } | null {
  if (distribution.above.length + distribution.below.length === 0) return null;
  if (distribution.visible > 0) return null;
  if (distribution.above.length > 0) return { direction: "up", remaining: distribution.above.length };
  return { direction: "down", remaining: distribution.below.length };
}

/** 跳转的目标：向上取最近的一条（上方的最后一个），否则取最近的下一条（下方的第一个）。 */
export function jumpTargetIndexOf(distribution: UserInputDistribution): number | null {
  const nearestAbove = distribution.above[distribution.above.length - 1];
  if (nearestAbove !== undefined) return nearestAbove;
  return distribution.below[0] ?? null;
}

/** 没有测量结果时的分布：没有用户输入（因此不呈现跳转入口）。 */
const NO_USER_INPUTS: UserInputDistribution = { visible: 0, above: [], below: [] };

/**
 * 生效的次级视图（三栏里哪几栏被藏起来由它决定）。
 *
 * 三条规则合起来保证「窄屏不丢信息」与「宽屏照旧三栏」：
 *
 * 1. **没有接线切换控件** → 三栏都呈现。藏了一栏却没有任何入口，就是静默丢弃信息；
 * 2. **宽屏 + 缺省取值** → 三栏都呈现。`DEFAULT_CONVERSATION_SECONDARY_VIEW` 是**窄屏**的默认
 *    形态（模型层的注释同样这么写），而宽屏的默认形态就是三栏并列（spec「对话页的宽屏两种形态」
 *    的第一句是 MUST）；用户在宽屏显式选过别的取值后照办；
 * 3. 其余情形照取值来。
 */
export function effectiveSecondaryView(input: {
  readonly requested: ConversationSecondaryView | null | undefined;
  readonly switchable: boolean;
  readonly wide: boolean;
}): ConversationSecondaryView {
  if (!input.switchable) return "all";
  const requested = input.requested ?? DEFAULT_CONVERSATION_SECONDARY_VIEW;
  if (input.wide && requested === DEFAULT_CONVERSATION_SECONDARY_VIEW) return "all";
  return requested;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    root: {
      gap: spacing.md,
      padding: spacing.lg,
      backgroundColor: colors.bg,
    },
    originStrip: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.md,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    originText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    syncedAt: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    header: {
      gap: spacing.xs,
      paddingBottom: spacing.sm,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    headerRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    title: {
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xl,
      fontWeight: "600",
    },
    agent: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    offline: {
      paddingVertical: 2,
      paddingHorizontal: spacing.xs,
      borderRadius: radius.sm,
      color: colors.warn,
      backgroundColor: colors.warnSoft,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    chip: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.xs,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.lg,
      backgroundColor: colors.panel2,
    },
    chipText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    chipWrap: {
      position: "relative",
    },
    popover: {
      // 弹层浮在胶囊**下方**（原型 `.agentchip .pop`：`position:absolute;top:calc(100% + 8px)`），
      // 因此展开不推动会话头与时间线。收起时 `display:none`，布局没有变化。
      position: "absolute",
      top: "100%",
      right: 0,
      gap: spacing.xs,
      minWidth: 290,
      marginTop: spacing.sm,
      padding: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel,
      zIndex: zIndex.topBar,
    },
    popoverHidden: {
      display: "none",
    },
    agentRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.xs,
      borderBottomWidth: 1,
      borderBottomColor: colors.line2,
    },
    dot: {
      width: 8,
      height: 8,
      borderRadius: 4,
      backgroundColor: colors.ink3,
    },
    agentName: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    agentId: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    agentState: {
      marginLeft: "auto",
      paddingVertical: 2,
      paddingHorizontal: spacing.xs,
      borderRadius: radius.sm,
      color: colors.ink2,
      backgroundColor: colors.line2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    toolbar: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    paging: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    pagingButton: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    pagingButtonDisabled: {
      borderColor: colors.line2,
      backgroundColor: colors.panel2,
    },
    pagingText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    pagingTextDisabled: {
      color: colors.ink3,
    },
    note: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    hint: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    historyNotice: {
      padding: spacing.sm,
      borderWidth: 1,
      borderColor: colors.warn,
      borderRadius: radius.sm,
      backgroundColor: colors.warnSoft,
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    bodyWide: {
      flexDirection: "row",
      // 三栏等高：两侧栏目的「吸底」元素才有底可吸（窄屏是堆叠，不受影响）。
      alignItems: "stretch",
      gap: spacing.md,
    },
    bodyNarrow: {
      flexDirection: "column",
      gap: spacing.md,
    },
    // 三栏的列宽就是原型的 `300 / 1fr / 360`：两侧用 `flexBasis` 定宽（RN 没有 grid），中栏吃掉余量。
    sessionsColumn: {
      flexBasis: 300,
      flexGrow: 0,
      flexShrink: 0,
      minWidth: 0,
      borderRightWidth: 1,
      borderRightColor: colors.line,
      paddingRight: spacing.md,
    },
    timelineColumn: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 0,
      gap: spacing.md,
    },
    filesColumn: {
      flexBasis: 360,
      flexGrow: 0,
      flexShrink: 0,
      minWidth: 0,
      gap: spacing.md,
      borderLeftWidth: 1,
      borderLeftColor: colors.line,
      paddingLeft: spacing.md,
    },
    hidden: {
      display: "none",
    },
    sidebar: {
      flexGrow: 1,
      gap: spacing.sm,
    },
    sidebarHead: {
      gap: spacing.sm,
      paddingBottom: spacing.sm,
      borderBottomWidth: 1,
      borderBottomColor: colors.line2,
    },
    sidebarDirRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    sidebarDirNames: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 0,
    },
    sidebarName: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: "600",
    },
    sidebarPath: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
      // 等宽「值」行取 500 档（原型 `.dirctx .pt`），不在这里写字面量。
      fontWeight: fontWeight.value,
    },
    search: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    sidebarList: {
      gap: spacing.xs,
    },
    sidebarRow: {
      borderRadius: radius.sm,
    },
    sidebarRowSelected: {
      backgroundColor: colors.accentSoft,
    },
    sidebarPager: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
      paddingTop: spacing.sm,
    },
    sidebarFoot: {
      marginTop: "auto",
      gap: spacing.xs,
      paddingTop: spacing.sm,
      borderTopWidth: 1,
      borderTopColor: colors.line2,
    },
    linkButton: {
      flexShrink: 0,
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    linkButtonText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    jumpBar: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.warn,
      borderRadius: radius.sm,
      backgroundColor: colors.warnSoft,
    },
    jumpBarText: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    jumpPill: {
      // 固定在**视口**底部居中（原型把胶囊绝对定位在滚动栏内；本页的滚动者是文档，因此取
      // `position: fixed` 才能一直停在可见区底部）。它渲染在中栏里：中栏被次级视图藏起来时，
      // 这个入口也一起消失（那时时间线本来就不在台上）。
      left: "50%",
      bottom: spacing.lg,
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.lg,
      backgroundColor: colors.panel,
      zIndex: zIndex.topBar,
      ...webStyle({ position: "fixed", transform: "translateX(-50%)" }),
    },
    jumpPillText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    // 跳转后的短暂高亮：一层不吃点击的固定覆盖层（目标条目的样式不归本组件管）。
    flashRing: {
      borderWidth: 2,
      borderColor: colors.accent,
      borderRadius: radius.md,
      zIndex: zIndex.topBar,
      ...webStyle({ position: "fixed" }),
    },
    footer: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
      paddingTop: spacing.sm,
      borderTopWidth: 1,
      borderTopColor: colors.line2,
    },
    capability: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 翻页入口（消息与历史各一个，锚点分别为 `data-paging` / `data-history-paging`）。 */
function PagingButton({
  marker,
  action,
  label,
  disabled,
  loading,
  onPress,
}: {
  readonly marker: "paging" | "historyPaging";
  readonly action: "load-earlier" | "load-earlier-history";
  readonly label: string;
  readonly disabled: boolean;
  readonly loading: boolean;
  readonly onPress: (() => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <Pressable
      role="button"
      disabled={disabled || onPress === undefined}
      style={disabled || onPress === undefined ? [styles.pagingButton, styles.pagingButtonDisabled] : styles.pagingButton}
      dataSet={{ [marker]: "earlier", action, pagingDisabled: String(disabled) }}
      onPress={() => {
        onPress?.();
      }}
    >
      <Text style={disabled ? [styles.pagingText, styles.pagingTextDisabled] : styles.pagingText}>
        {loading ? "正在加载……" : label}
      </Text>
    </Pressable>
  );
}

/**
 * 会话头的 Agent 胶囊（原型 `.agentchip`）。
 *
 * 名单与连接状态都来自 `HostPanelModel`，也就是**主机面板用的那一份**：列表本体是快照里的
 * `agents` 资源，连接状态是可选覆盖层。因此覆盖层缺席时每行给出的是「状态未知」，**不**会写成
 * 「已断开」，也不会把该 Agent 从名单里拿掉（spec 的两条 MUST NOT）。
 *
 * 弹层内容**始终在 DOM**（收起时只 `display: none`，与原型 `.pop` 同一形态）：这样
 * 「状态未知不等于已断开」在静置的渲染结果里就能被断言，而不是要先把弹层点开。
 */
function AgentChip({ panel }: { readonly panel: HostPanelModel }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const [open, setOpen] = useState(false);

  return (
    <View style={styles.chipWrap} dataSet={{ agentChip: "agents", agentChipOpen: String(open) }}>
      <Pressable
        role="button"
        style={styles.chip}
        dataSet={{ action: "toggle-agent-chip", agentChipCount: String(panel.agents.length) }}
        onPress={() => {
          setOpen(!open);
        }}
      >
        <View style={styles.dot} />
        <Text style={styles.chipText}>{`${String(panel.agents.length)} 个 Agent ▾`}</Text>
      </Pressable>

      <View
        style={open ? styles.popover : [styles.popover, styles.popoverHidden]}
        dataSet={{ agentPopover: open ? "open" : "closed" }}
      >
        {panel.agents.length === 0 ? (
          <Text style={styles.note} dataSet={{ agentChipEmpty: "true" }}>
            最近一次同步的 Agent 目录里没有 Agent。
          </Text>
        ) : (
          panel.agents.map((agent) => (
            <View
              key={agent.id}
              style={styles.agentRow}
              dataSet={{ agentRow: agent.id, agentConnection: agent.connection }}
            >
              <View style={styles.dot} />
              <Text style={styles.agentName} dataSet={{ agentRowName: agent.displayName }}>
                {agent.displayName}
              </Text>
              {/* 标识：同名 Agent 靠它区分，不按展示名合并。 */}
              <Text style={styles.agentId} dataSet={{ agentRowId: agent.id }}>
                {agent.id}
              </Text>
              <Text style={styles.agentState} dataSet={{ agentRowConnection: agent.connectionLabel }}>
                {agent.connectionLabel}
              </Text>
              {agent.disconnectReason === null ? null : (
                <Text style={styles.hint} dataSet={{ agentRowReason: agent.disconnectReason }}>
                  {agent.disconnectReason}
                </Text>
              )}
            </View>
          ))
        )}
        <Text style={styles.hint} dataSet={{ agentChipSource: "connection-events" }}>
          来源：agent.connected / agent.disconnected 连接事件，不是静态配置。
        </Text>
        {panel.unknownStateNote === null ? null : (
          <Text style={styles.hint} dataSet={{ agentChipUnknownNote: "true" }}>
            {panel.unknownStateNote}
          </Text>
        )}
      </View>
    </View>
  );
}

/**
 * 左栏：目录上下文（展示名 + 逐字路径 + 进详情页的入口）+ 目录内搜索 + 同目录会话列表 +
 * 「已呈现 N / M」与继续加载 + 吸底的新建会话入口。
 *
 * 搜索框是**受控**的（值来自 `model.query`，输入回到仓库的 `conversationSidebarQuery`）：它只
 * 影响本栏列表（模型层保证），因此「搜索不改中栏」在这里没有表达方式。
 */
function Sidebar({
  model,
  onOpenDirectory,
  onOpenSession,
  onQueryChange,
  onLoadMore,
  onCreateSession,
}: {
  readonly model: ConversationSidebarModel;
  readonly onOpenDirectory: ((directoryId: string) => void) | undefined;
  readonly onOpenSession: ((sessionId: string) => void) | undefined;
  readonly onQueryChange: ((query: string) => void) | undefined;
  readonly onLoadMore: (() => void) | undefined;
  readonly onCreateSession: ((directoryId: string) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const directory = model.directory;
  const pagerLabel = model.canLoadMore
    ? `已呈现 ${String(model.shown)} / ${String(model.total)}`
    : model.total === 0
      ? "这个目录还没有会话"
      : `已呈现该目录全部 ${String(model.total)} 个会话`;

  return (
    <View style={styles.sidebar} dataSet={{ conversationSidebar: "true" }}>
      {/* 没有目录归属：说明它，而不是把左栏藏起来（中栏照常呈现）。 */}
      {model.notice === null ? null : (
        <Text style={styles.note} dataSet={{ sidebarNotice: "no_directory" }}>
          {model.notice}
        </Text>
      )}

      {directory === null ? null : (
        <View style={styles.sidebarHead} dataSet={{ sidebarDirectory: directory.directoryId }}>
          <View style={styles.sidebarDirRow}>
            <View style={styles.sidebarDirNames}>
              <Text style={styles.sidebarName} dataSet={{ sidebarDirectoryName: directory.displayName }}>
                {directory.displayName}
              </Text>
              {/* 路径逐字来自快照目录条目，始终在场。 */}
              <Text style={styles.sidebarPath} numberOfLines={1} dataSet={{ sidebarDirectoryPath: directory.path }}>
                {directory.path}
              </Text>
            </View>
            <Pressable
              role="button"
              style={styles.linkButton}
              dataSet={{ action: "open-directory", directoryId: directory.directoryId }}
              onPress={() => {
                onOpenDirectory?.(directory.directoryId);
              }}
            >
              <Text style={styles.linkButtonText}>全部 ›</Text>
            </Pressable>
          </View>

          {onQueryChange === undefined ? null : (
            <TextInput
              value={model.query}
              placeholder="在本目录内搜索"
              style={styles.search}
              dataSet={{ sidebarSearch: "true" }}
              onChangeText={(text: string) => {
                onQueryChange(text);
              }}
            />
          )}
        </View>
      )}

      {directory === null || model.rows.length === 0 ? null : (
        <View role="list" style={styles.sidebarList} dataSet={{ sidebarSessions: "list" }}>
          {model.rows.map((row) => {
            const selected = row.id === model.selectedSessionId;
            return (
              // 选中态只在行的外框上（`SessionRow` 没有 selected 参数，也不该为对话页长出第二种会话行）。
              <View
                key={row.id}
                style={selected ? [styles.sidebarRow, styles.sidebarRowSelected] : styles.sidebarRow}
                dataSet={{ sidebarSession: row.id, sidebarSessionSelected: String(selected) }}
              >
                {onOpenSession === undefined ? (
                  <SessionRow session={row} />
                ) : (
                  <SessionRow
                    session={row}
                    onOpen={() => {
                      onOpenSession(row.id);
                    }}
                  />
                )}
              </View>
            );
          })}
        </View>
      )}

      {directory === null ? null : (
        <View style={styles.sidebarPager} dataSet={{ sidebarShown: String(model.shown), sidebarTotal: String(model.total) }}>
          <Text style={styles.note} dataSet={{ sidebarPagerLabel: model.canLoadMore ? "partial" : "complete" }}>
            {pagerLabel}
          </Text>
          {!model.canLoadMore || onLoadMore === undefined ? null : (
            <Pressable
              role="button"
              style={styles.linkButton}
              dataSet={{ action: "load-more-sidebar" }}
              onPress={onLoadMore}
            >
              <Text style={styles.linkButtonText}>{`加载更多（还有 ${String(model.total - model.shown)} 个）`}</Text>
            </Pressable>
          )}
        </View>
      )}

      {directory === null || onCreateSession === undefined ? null : (
        <View style={styles.sidebarFoot}>
          <Pressable
            role="button"
            style={styles.linkButton}
            dataSet={{ action: "create-session-in-directory", directoryId: directory.directoryId }}
            onPress={() => {
              onCreateSession(directory.directoryId);
            }}
          >
            <Text style={styles.linkButtonText}>⊕ 新建会话</Text>
          </Pressable>
          <Text style={styles.hint}>将在电脑上启动 Agent</Text>
        </View>
      )}
    </View>
  );
}

/**
 * 视口里的用户输入节点（按时间线次序）。
 *
 * 定位方式是消息条目的既有锚点 `data-message-id`（`ConversationTimeline` 发的那个；
 * `conversation-timeline` 的浏览器用例也逐字断言它），并只取**本会话的用户消息**那一批
 * ——因此 Agent 气泡与思考条目不会被当成「你的输入」。
 */
function userInputNodes(host: HTMLElement, ids: readonly string[]): readonly HTMLElement[] {
  const wanted = new Set(ids);
  const nodes: HTMLElement[] = [];
  host.querySelectorAll("[data-message-id]").forEach((node) => {
    const id = node.getAttribute("data-message-id");
    if (id !== null && wanted.has(id)) nodes.push(node as HTMLElement);
  });
  return nodes;
}

/** 对话页正文。 */
export function ConversationStream({
  model,
  onLoadEarlier,
  onOpenDiagnostics,
  onLoadEarlierHistory,
  onDraftChange,
  onSubmitPrompt,
  onCancelTurn,
  onAnswerInteraction,
  onSelectModel,
  onSelectMode,
  onOpenChangeDiagnostics,
  openChangeDiagnosticsEventId,
  expandedTimelineEntries,
  onToggleTimelineEntry,
  selectedChangeId,
  onSelectChange,
  secondaryView,
  onSecondaryViewChange,
  availableWidth,
  onCopyCode,
  onLinkPress,
  onOpenDirectory,
  onOpenSession,
  onSidebarQueryChange,
  onLoadMoreSidebar,
  onCreateSession,
  hostPanel,
  connectionState,
  lastSyncedAtMs,
}: ConversationStreamProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const windowDimensions = useWindowDimensions();
  const width = availableWidth ?? windowDimensions.width;
  // 断点取自唯一一份 token（此前这里与另外两处各写了一次字面量 720）。
  const wide = width >= breakpoints.wide;
  const switchable = onSecondaryViewChange !== undefined;
  // 生效取值的三条规则（含「宽屏 + 缺省取值 = 三栏」与「没有切换控件时不藏栏」）见 `effectiveSecondaryView`。
  const view = effectiveSecondaryView({ requested: secondaryView, switchable, wide });
  const hiddenColumns = hiddenConversationColumns(view);
  const paging = model.paging;
  const history = model.timeline.structuredHistory;
  const sidebar = model.sidebar;
  /**
   * 只把「时间线上没有对应卡片」的交互交给待处理清单：有卡片的那条已经在时间线上带着同一份
   * 可用性呈现（同一 `interactionId` 不重复呈现）。没有请求事件的读回待处理项因此不会消失。
   */
  const shownInteractionIds = new Set(
    model.timeline.entries.flatMap((entry) =>
      entry.kind === "permission_requested" || entry.kind === "elicitation_requested" ? [entry.interactionId] : [],
    ),
  );
  const pendingOnly = model.interactions.filter((interaction) => !shownInteractionIds.has(interaction.interactionId));
  /**
   * 还等在本设备手上的交互条数（会话头下方的待处理跳转条用）。
   *
   * `answerable` 是「本设备亲历且持有可选项」那一档：只有它能确定「确实在等我们处理」，其余三档
   * （不可应答 / 已被其它设备处理 / 应答中）都不该被数进「待处理 N 项」。
   */
  const answerableCount = model.interactions.filter(
    (interaction) => interaction.availability.kind === "answerable",
  ).length;
  const pendingBarLabel = PENDING_BAR_LABELS[model.header.state] ?? null;

  // ── 跳转入口的测量（只属于渲染现场的事实：滚动位置与元素几何）───────────────────
  const rootRef = useRef<HTMLElement | null>(null);
  const [distribution, setDistribution] = useState<UserInputDistribution>(NO_USER_INPUTS);
  const [flash, setFlash] = useState<{
    readonly messageId: string;
    readonly top: number;
    readonly left: number;
    readonly width: number;
    readonly height: number;
  } | null>(null);
  /** 本会话用户消息的标识（时间线次序）：测量与跳转都以它定位 DOM 节点。 */
  const userMessageIds = model.timeline.entries.flatMap((entry) =>
    entry.kind === "message" && entry.channel === "user" ? [entry.message.id] : [],
  );
  // 依赖用**稳定**的字符串而不用数组：array 每次渲染都是新引用，effect 会反复重挂监听。
  const userMessageKey = userMessageIds.join(",");

  useEffect(() => {
    const host = rootRef.current;
    // node 侧（`renderToStaticMarkup`）没有布局可测：那时 `distribution` 保持「没有用户输入」，
    // 因此跳转入口不呈现（不猜、也不编一个假的「在视口外」）。
    if (host === null || typeof window === "undefined") return;
    const ids = userMessageKey === "" ? [] : userMessageKey.split(",");
    const measure = (): void => {
      const rects = userInputNodes(host, ids).map((node) => {
        const rect = node.getBoundingClientRect();
        return { top: rect.top, bottom: rect.bottom };
      });
      setDistribution(classifyUserInputs(rects, window.innerHeight));
    };
    measure();
    window.addEventListener("scroll", measure, { passive: true });
    window.addEventListener("resize", measure);
    return () => {
      window.removeEventListener("scroll", measure);
      window.removeEventListener("resize", measure);
    };
  }, [userMessageKey]);

  useEffect(() => {
    if (flash === null) return;
    const timer = setTimeout(() => {
      setFlash(null);
    }, motion.pulse);
    return () => {
      clearTimeout(timer);
    };
  }, [flash]);

  const jump = jumpPillOf(distribution);

  /** 跳到最近的用户输入，并把它短暂框住（原型 `jumpToUser`）。 */
  const jumpToUserInput = (): void => {
    const host = rootRef.current;
    if (host === null) return;
    const index = jumpTargetIndexOf(distribution);
    if (index === null) return;
    const node = userInputNodes(host, userMessageIds)[index];
    if (node === undefined) return;
    node.scrollIntoView({ block: "center" });
    const rect = node.getBoundingClientRect();
    setFlash({
      messageId: node.getAttribute("data-message-id") ?? "",
      top: rect.top,
      left: rect.left,
      width: rect.width,
      height: rect.height,
    });
  };

  /** 跳到第一张待处理卡片（原型 `data-jump` 的占位行为在这里落到真实的卡片上）。 */
  const jumpToPending = (): void => {
    const host = rootRef.current;
    if (host === null) return;
    host.querySelector("[data-interaction-id], [data-pending-interaction]")?.scrollIntoView({ block: "center" });
  };

  return (
    <section ref={rootRef} data-route="session" data-session-id={model.header.sessionId}>
      <View style={styles.root} dataSet={{ conversationPage: "true" }}>
        {/* 内容来源条：未在线时说明内容是上次同步来的，并给出那一次的时刻（spec 的断连标注）。 */}
        {connectionState === null || connectionState === "online" ? null : (
          <View style={styles.originStrip} dataSet={{ connectionStrip: "true", contentOrigin: "last_sync" }}>
            <Text style={styles.originText} dataSet={{ contentOriginLabel: "last_sync" }}>
              {CONTENT_ORIGIN_LABELS.last_sync}
            </Text>
            {/* 从未成功替换过快照时没有时刻可呈现（那时也不编造一个）。
                时刻的形态与目录页/详情页同一段 UTC 切片（`YYYY-MM-DD HH:MM UTC`）。 */}
            {lastSyncedAtMs === null || lastSyncedAtMs === undefined ? null : (
              <Text style={styles.syncedAt} dataSet={{ syncedAt: String(lastSyncedAtMs) }}>
                {`已同步 ${new Date(lastSyncedAtMs).toISOString().slice(0, 16).replace("T", " ")} UTC`}
              </Text>
            )}
          </View>
        )}

        <View style={styles.header} dataSet={{ sessionHeader: model.header.sessionId }}>
          <View style={styles.headerRow}>
            <Text style={styles.title} dataSet={{ sessionTitle: "true" }}>
              {model.header.title}
            </Text>
            <StatusPill state={model.header.state} />
            {model.header.originOfflineLabel === null ? null : (
              <Text style={styles.offline} dataSet={{ originOffline: "true" }}>
                {model.header.originOfflineLabel}
              </Text>
            )}
          </View>
          <View style={styles.headerRow}>
            <Text style={styles.agent} dataSet={{ sessionAgent: model.header.agentName }}>
              {model.header.agentName}
            </Text>
            {hostPanel === null || hostPanel === undefined ? null : <AgentChip panel={hostPanel} />}
          </View>
        </View>

        {/* 切换控件在三栏之外：中栏被次级视图藏起来时，这里仍是切回去的唯一入口。 */}
        {switchable ? (
          <SegmentControl
            label="conversation-secondary-view"
            value={view}
            options={SECONDARY_VIEW_OPTIONS}
            onChange={(next) => {
              onSecondaryViewChange?.(next);
            }}
          />
        ) : null}

        <View style={wide ? styles.bodyWide : styles.bodyNarrow} dataSet={{ conversationBody: "true" }}>
          {/* 左栏：目录上下文 + 目录内搜索 + 同目录会话 + 已呈现数 + 继续加载 + 吸底新建会话。 */}
          <View
            style={hiddenColumns.includes("sessions") ? [styles.sessionsColumn, styles.hidden] : styles.sessionsColumn}
            dataSet={{ conversationColumn: "sessions" }}
          >
            <Sidebar
              model={sidebar}
              onOpenDirectory={onOpenDirectory}
              onOpenSession={onOpenSession}
              onQueryChange={onSidebarQueryChange}
              onLoadMore={onLoadMoreSidebar}
              onCreateSession={onCreateSession}
            />
          </View>

          {/* 中栏：待处理跳转条 → 两套翻页 → 结构化历史说明 → 时间线 → 跳转胶囊。 */}
          <View
            style={hiddenColumns.includes("timeline") ? [styles.timelineColumn, styles.hidden] : styles.timelineColumn}
            dataSet={{ conversationColumn: "timeline" }}
          >
            {pendingBarLabel === null ? null : (
              <View style={styles.jumpBar} dataSet={{ pendingJumpBar: model.header.state }}>
                <Text style={styles.jumpBarText}>
                  {answerableCount === 0
                    ? `⏸ ${pendingBarLabel}`
                    : `⏸ ${pendingBarLabel}，待处理 ${String(answerableCount)} 项`}
                </Text>
                <Pressable
                  role="button"
                  style={styles.linkButton}
                  dataSet={{ action: "jump-to-pending", pendingJumpCount: String(answerableCount) }}
                  onPress={jumpToPending}
                >
                  <Text style={styles.linkButtonText}>查看 ↓</Text>
                </Pressable>
              </View>
            )}

            <View style={styles.toolbar}>
              <View style={styles.paging} dataSet={{ pagingGroup: "messages" }}>
                <PagingButton
                  marker="paging"
                  action="load-earlier"
                  label="加载更早的消息"
                  disabled={!model.capabilities.canLoadEarlier}
                  loading={paging.loading}
                  onPress={onLoadEarlier}
                />
                {paging.hasEarlier ? (
                  <Text style={styles.note} dataSet={{ pagingPageSize: String(model.messages.length) }}>
                    {`本页 ${String(model.messages.length)} 条消息。`}
                  </Text>
                ) : (
                  <Text style={styles.note} dataSet={{ paging: "earlier-exhausted" }}>
                    已经是最早的消息。
                  </Text>
                )}
                {paging.error === null ? null : (
                  <Text style={styles.note} dataSet={{ pagingError: "true" }}>
                    {paging.error}
                  </Text>
                )}
              </View>

              {/* 结构化历史与消息是两套独立分页（D2）：这里只描述历史那一页。 */}
              {history.state === "available" && onLoadEarlierHistory !== undefined ? (
                <View style={styles.paging} dataSet={{ pagingGroup: "history" }}>
                  <PagingButton
                    marker="historyPaging"
                    action="load-earlier-history"
                    label="加载更早的结构化条目"
                    disabled={!paging.hasEarlierHistory || paging.loadingHistory}
                    loading={paging.loadingHistory}
                    onPress={onLoadEarlierHistory}
                  />
                  {paging.hasEarlierHistory ? (
                    <Text style={styles.note} dataSet={{ historyPagingRemaining: "true" }}>
                      还有更早的结构化条目。
                    </Text>
                  ) : (
                    <Text style={styles.note} dataSet={{ historyPaging: "earlier-exhausted" }}>
                      已经是最早的结构化条目。
                    </Text>
                  )}
                </View>
              ) : null}
            </View>

            {/* 结构化历史不可用必须有明确说明，且不可用 ≠ 「这个会话没有结构化内容」（D11）。 */}
            {history.notice === null ? null : (
              <Text style={styles.historyNotice} dataSet={{ structuredHistory: history.state }}>
                {history.notice}
              </Text>
            )}

            {pendingOnly.length === 0 ? null : (
              <PendingInteractionsPanel interactions={pendingOnly} onAnswerInteraction={onAnswerInteraction} />
            )}
            <ConversationTimeline
              entries={model.timeline.entries}
              onOpenDiagnostics={onOpenDiagnostics}
              onAnswerInteraction={onAnswerInteraction}
              onCopyCode={onCopyCode}
              onLinkPress={onLinkPress}
              expandedEntries={expandedTimelineEntries}
              onToggleEntry={onToggleTimelineEntry}
              selectedChangeId={selectedChangeId ?? null}
              onSelectChange={onSelectChange}
            />

            {/* 没有用户输入、或全部用户输入都在视口内时**不**呈现（spec 的两条场景）。 */}
            {jump === null ? null : (
              <Pressable
                role="button"
                style={styles.jumpPill}
                dataSet={{ jumpPill: jump.direction, jumpRemaining: String(jump.remaining) }}
                onPress={jumpToUserInput}
              >
                <Text style={styles.jumpPillText}>
                  {jump.direction === "up"
                    ? `↑ 跳到你最近的输入（上面还有 ${String(jump.remaining)} 条）`
                    : `↓ 跳到你的下一条输入（下面还有 ${String(jump.remaining)} 条）`}
                </Text>
              </Pressable>
            )}
          </View>

          {/* 右栏：改动清单与差异行。 */}
          <View
            style={hiddenColumns.includes("files") ? [styles.filesColumn, styles.hidden] : styles.filesColumn}
            dataSet={{ conversationColumn: "files" }}
          >
            <FileChangesPanel
              changes={model.fileChanges}
              openDiagnosticsEventId={openChangeDiagnosticsEventId ?? null}
              onOpenChangeDiagnostics={onOpenChangeDiagnostics}
              selectedChangeId={selectedChangeId ?? null}
              onSelectChange={onSelectChange}
            />
          </View>
        </View>

        {/* 输入区在正文之外：次级视图把中栏藏起来时它仍要可用（否则切到「会话」就发不出消息）。 */}
        <ConversationComposer
          input={model.input}
          usage={model.usage}
          settings={model.settings}
          onDraftChange={onDraftChange}
          onSubmit={onSubmitPrompt}
          onCancel={onCancelTurn}
          onSelectModel={onSelectModel}
          onSelectMode={onSelectMode}
        />

        <View style={styles.footer} dataSet={{ capabilities: "true" }}>
          <Text style={styles.capability} dataSet={{ canSend: String(model.capabilities.canSend) }}>
            {`发送：${model.capabilities.canSend ? "可用" : "不可用"}`}
          </Text>
          <Text style={styles.capability} dataSet={{ canCancel: String(model.capabilities.canCancel) }}>
            {`取消：${model.capabilities.canCancel ? "可用" : "不可用"}`}
          </Text>
          <Text style={styles.capability} dataSet={{ canViewHistory: String(model.capabilities.canViewHistory) }}>
            {`查看历史：${model.capabilities.canViewHistory ? "可用" : "不可用"}`}
          </Text>
          {model.capabilities.canSend ? null : (
            <Text style={styles.note} dataSet={{ sendDisabledReason: "true" }}>
              当前不能发送消息（原因见输入区）。
            </Text>
          )}
        </View>

        {/* 跳转后的短暂高亮：目标条目的坐标取自它自己的视口矩形，本组件不为它加样式。 */}
        {flash === null ? null : (
          <View
            pointerEvents="none"
            style={[styles.flashRing, { top: flash.top, left: flash.left, width: flash.width, height: flash.height }]}
            dataSet={{ jumpFlash: flash.messageId }}
          />
        )}
      </View>
    </section>
  );
}
