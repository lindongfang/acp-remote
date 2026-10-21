/**
 * 对话页正文（三栏骨架 + 会话头 + 输入区；spec「对话页的宽窄两种形态」「对话页左栏呈现同目录会话」
 * 「跳转到最近的用户输入」「未上报的上下文不得以零冒充」「Agent 目录与连接状态分两层呈现」；
 * `design.md` D7/D8/D9/D10）。
 *
 * ## 一页 = 三栏，三栏内容**始终在 DOM**
 *
 * 宽屏的正文是 `300 / 1fr / 360` 三栏（同目录会话栏、时间线、变更文件栏），窄屏把两侧栏目降为
 * **可切换的次级视图**。两种形态的差别只在样式上：三栏始终都在 DOM 里，未选中的栏只是
 * `display: none`（藏哪几栏由 `hiddenConversationColumns` 这**唯一**一份判据给出，组件不另写
 * 一套）。因此「窄屏不丢信息」是一条可断言的事实（两种形态的 `data-*` 集合在同一交互状态下完全
 * 相同），而不是靠自觉。
 *
 * 宽屏下三栏**各自滚动**（左栏会话、中栏时间线、右栏变更文件各一个 `ScrollView`）；中栏自上而下
 * 是会话头 → 时间线滚动区 → 输入区（输入区吸在中栏底部）。窄屏保留次级视图切换，
 * 切换控件留在三栏**之外**：中栏被藏起来时，切回去的入口不能跟着消失。
 *
 * ## 判定不在本组件里
 *
 * 输入区能不能提交、编辑框是否只读、模型能不能切、差异行有没有、左栏列哪些会话，以及顶部标题
 * 是否处于编辑/提交/失败，全部由 `ConversationPageModel` / `titleEditorView` 里的判别值决定
 * （`src/features/**` 与 `src/domain/**` 的纯函数）。本组件只做两件事：按模型渲染、把交互原样
 * 上抛。唯一落在组件里的判定是**可见带测量**（跳转入口）与**双击时间窗/宿主按键形态**
 * ——前者读滚动位置，后者是渲染现场的事件语义（PWA 没有统一的 `dblclick`），都不改变数据判定。
 *
 * ## 跳转入口与「短暂高亮」
 *
 * 时间线可见带外还有用户输入时呈现跳转胶囊（原型 `.jumppill`）；可见带换算与方向/条数由
 * {@link classifyUserInputsInBand} / {@link classifyUserInputs} / {@link jumpPillOf} 给出
 * （纯函数，组件测试直接喂几何）。跳转后用
 * 一圈描边把目标框住 `motion.pulse` 毫秒——目标条目属于 `ConversationTimeline` 的子树，本组件
 * 不为它加样式，因此高亮是一层 `position: fixed` 的覆盖层，坐标取自目标自己的
 * `getBoundingClientRect()`（网页专有面；node 侧不做测量，那时跳转入口不呈现）。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { ReactElement } from "react";

import type { InteractionAnswer } from "../domain/interaction";
import type { AttachmentMediaView, FormInputValue } from "../domain";
import type { AttachmentKind } from "../features/attachment-model";
import {
  CONVERSATION_SECONDARY_VIEWS,
  DEFAULT_CONVERSATION_SECONDARY_VIEW,
  hiddenConversationColumns,
} from "../features/conversation-model";
import type { ConversationPageModel, ConversationSecondaryView, ConversationSidebarModel } from "../features/conversation-model";
import type { HostPanelModel } from "../features/host-panel-model";
import type { SessionTitleCommitReason, SessionTitleEditorView } from "../features/session-title-editor";
import { isSessionTitleDoubleClick, sessionTitleKeyAction } from "../features/session-title-editor";
import { ConversationComposer } from "./ConversationComposer";
import { ConversationTimeline } from "./ConversationTimeline";
import { ConversationColumnResizer } from "./ConversationColumnResizer";
import { CONVERSATION_SIDEBAR_PAGE_SIZE } from "../features/conversation-model";
import { FileContentViewer } from "./FileContentViewer";
import type { FilePreviewArtifact } from "./FileContentViewer";
import { SessionFilesPanel } from "./SessionFilesPanel";
import type { FileSource, SessionFileDownloadState, SessionFilesPanelModel } from "../features";
import { PendingInteractionsPanel } from "./PendingInteractionsPanel";
import { Pager } from "./Pager";
import { SegmentControl } from "./SegmentControl";
import { SessionRow } from "./SessionRow";
import { StatusPill } from "./StatusPill";
import type { Palette, TextInputProps } from "./theme";
import {
  Pressable,
  ScrollView,
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
  /** 打开 Markdown 里可解析的工作目录相对路径（页面负责校验并打开文件）。 */
  readonly onFilePathPress?: ((path: string) => void) | undefined;
  /** 已读取的消息媒体（`attachmentId → 状态`）。 */
  readonly media?: ReadonlyMap<string, AttachmentMediaView> | undefined;
  /** 请求读取一条已封存的媒体（页面接到仓库）。 */
  readonly onLoadMedia?: ((attachmentId: string, sessionId: string, mimeType: string) => void) | undefined;
  /** 释放一条媒体的对象 URL（消息卸载时）。 */
  readonly onReleaseMedia?: ((attachmentId: string) => void) | undefined;
  /** 下载消息里的文件附件。 */
  readonly onDownloadAttachment?:
    | ((attachmentId: string, mimeType: string, fileName: string) => void)
    | undefined;
  /** 提交提问表单（值已通过客户端校验）。 */
  readonly onSubmitElicitation?:
    | ((interactionId: string, values: Readonly<Record<string, FormInputValue>>) => void)
    | undefined;
  /** 重新取得服务端确认的当前有效请求。 */
  readonly onRefreshInteractions?: (() => void) | undefined;
  /** 打开平台选择器选择附件（选择阶段只在本机预览，发送后才上传）。 */
  readonly onPickAttachment?: ((kind: AttachmentKind) => void) | undefined;
  /** 粘贴事件（交给平台端口解析）。 */
  readonly onPasteAttachments?: ((event: unknown) => void) | undefined;
  /** 移除一个已选附件。 */
  readonly onRemoveAttachment?: ((localId: string) => void) | undefined;
  /** 重试失败/取消的附件。 */
  readonly onRetryAttachments?: (() => void) | undefined;
  /** 取消正在进行的附件上传（保留草稿，不派发消息）。 */
  readonly onCancelAttachmentUpload?: (() => void) | undefined;
  /** 添加工作目录相对路径引用；返回拒绝原因。 */
  readonly onAddPathRef?: ((path: string) => string | null) | undefined;
  /** 移除一个路径引用。 */
  readonly onRemovePathRef?: ((path: string) => void) | undefined;
  /** 左栏「‹ 目录」：返回目录列表页（`/`）。 */
  readonly onOpenDirectoryList?: (() => void) | undefined;
  /** 左栏会话行：打开同目录的另一个会话。 */
  readonly onOpenSession?: ((sessionId: string) => void) | undefined;
  /** 左栏目录内搜索词变化（**只**影响左栏列表）。 */
  readonly onSidebarQueryChange?: ((query: string) => void) | undefined;
  /** 左栏「加载更多」：再呈现一批同目录会话。 */
  readonly onSidebarPageChange?: ((pageInput: string) => void) | undefined;
  /** 左栏底部的「⊕ 新建会话」：在该目录内创建会话。 */
  readonly onCreateSession?: ((directoryId: string) => void) | undefined;
  /**
   * 顶部标题编辑视图（`store.titleEditorView(sessionId)`）。
   *
   * 只有双击、提交中/失败或一条可归因提示时才非空；`null` 表示标题只是普通文本。
   * 标题值本身在 `model.header.title`（服务端下发的有效标题），这里只带编辑现场。
   */
  readonly titleEditor?: SessionTitleEditorView | null | undefined;
  /** 双击标题：进入原地编辑（组件只按两次点击的时间窗判定，不依赖 dblclick 事件）。 */
  readonly onStartTitleEdit?: (() => void) | undefined;
  /** 编辑框文字变化。 */
  readonly onTitleDraftChange?: ((draft: string) => void) | undefined;
  /** Enter 或失焦保存（仓库保证一次意图只派发一个 `requestId`）。 */
  readonly onCommitTitle?: ((reason: SessionTitleCommitReason) => void) | undefined;
  /** Escape 取消编辑。 */
  readonly onCancelTitleEdit?: (() => void) | undefined;
  /**
   * 主机与连接面板模型：会话头的 Agent 胶囊**沿用同一份**连接事件覆盖层
   * （`agents` 列表本体来自快照，连接状态是可选覆盖层；覆盖层缺席时每行是「状态未知」）。
   */
  readonly hostPanel?: HostPanelModel | null;
  /**
   * 文件栏两来源与互斥内容态（`store.sessionFilesPage(sessionId)`）。
   *
   * 缺省 `null` 时右栏只给一句中性说明：未接入文件查询的消费者不再回到旧的文件改动清单，
   * 也不伪造来源。
   */
  readonly filesPanel?: SessionFilesPanelModel | null;
  /** 切换文件来源。 */
  readonly onSelectFileSource?: ((source: FileSource) => void) | undefined;
  /** 手动刷新当前来源。 */
  readonly onRefreshFiles?: (() => void) | undefined;
  readonly onExpandedFileDirectoriesChange?: ((paths: readonly string[]) => void) | undefined;
  /** 切换「隐藏忽略文件」。 */
  readonly onToggleHideIgnored?: (() => void) | undefined;
  /** 展开/分页加载某一层目录。 */
  readonly onLoadMoreAllFiles?: ((path: string) => void) | undefined;
  /** 继续加载 Git 变更。 */
  readonly onLoadMoreGitChanges?: (() => void) | undefined;
  /** 打开文件内容（来源 + 根内相对路径）。 */
  readonly onOpenFile?: ((source: FileSource, path: string) => void) | undefined;
  /** 关闭文件查看（焦点与窄屏返回由页面负责）。 */
  readonly onCloseFile?: (() => void) | undefined;
  /** 重新读取当前文件（过时/失败时）。 */
  readonly onRetryFile?: (() => void) | undefined;
  /** 主动下载当前预览文件。 */
  readonly onDownloadFile?: ((path: string, sizeBytes: string | null, mimeType: string) => void) | undefined;
  readonly onCancelDownload?: (() => void) | undefined;
  readonly onReleaseDownload?: (() => void) | undefined;
  /** 下载服务状态（`SessionFileDownloader.state`）；缺省 `null` = 尚未装配。 */
  readonly downloadState?: SessionFileDownloadState | null;
  /** 平台端口创建图片预览 Blob URL；缺省时查看区显式说明不支持。 */
  readonly createPreviewUrl?:
    | ((bytes: Uint8Array, mimeType: string) => FilePreviewArtifact | null)
    | undefined;
}

/** 次级视图 → 切换控件上的文案（四个取值一个不少，键集合由类型保证）。 */
const SECONDARY_VIEW_LABELS: Readonly<Record<ConversationSecondaryView, string>> = {
  all: "全部",
  timeline: "对话",
  sessions: "会话",
  files: "文件",
};

/** 次级视图切换控件的选项：取值的清单与顺序都来自模型层的登记表，不在这里另列一份。 */
const SECONDARY_VIEW_OPTIONS: readonly { readonly value: ConversationSecondaryView; readonly label: string }[] =
  CONVERSATION_SECONDARY_VIEWS.map((value) => ({ value, label: SECONDARY_VIEW_LABELS[value] }));

/**
 * 窄屏的切换选项：**没有「全部」**。窄屏的三栏是纵向堆叠，均分高度会把时间线挤成 0 高，
 * 因此窄屏只提供「一栏占主体」的三个取值（`effectiveSecondaryView` 也把 `all` 归一为 `timeline`）。
 */
const NARROW_SECONDARY_VIEW_OPTIONS = SECONDARY_VIEW_OPTIONS.filter((option) => option.value !== "all");

/**
 * 一条用户输入的上下边像素值。`getBoundingClientRect()` 给的是**视口坐标**；
 * {@link classifyUserInputsInBand} 把它换算到可见带原点后再交给 {@link classifyUserInputs}
 * （后者只认相对可见带顶端的坐标）。组件测试直接喂假几何，因此判定函数不需要 DOM。
 */
export interface ViewportRect {
  readonly top: number;
  readonly bottom: number;
}

/** 可见带边界的容差：与原型 `updateJump` 的 10px 同一口径（贴着边不算看到）。 */
export const JUMP_VIEWPORT_GUARD_PX = 10;

/**
 * 距底部多少像素内仍算「在底部」（跟随新内容的阈值）。
 *
 * 取 24px：小于它时新内容会立刻进入视口，继续跟随比停住更符合阅读预期；超过它才视为
 * 「用户主动上读」，此后不自动滚动。
 */
export const SCROLL_FOLLOW_GUARD_PX = 24;

/** 时间线滚动容器的几何读数（`onScroll` 给的原值）。 */
export interface ScrollMetrics {
  readonly offsetY: number;
  readonly contentHeight: number;
  readonly viewportHeight: number;
}

/**
 * 是否在底部（跟随新内容的**唯一**判定）。
 *
 * 只看几何，不看「上一次是不是在底部」：用户手动滚回底部后立刻恢复跟随，无需额外状态。
 */
export function isFollowingBottom(metrics: ScrollMetrics): boolean {
  const distance = metrics.contentHeight - (metrics.offsetY + metrics.viewportHeight);
  return distance <= SCROLL_FOLLOW_GUARD_PX;
}

/**
 * 锚点补偿的滚动增量：内容在锚点上方增长时（更早历史、媒体加载）`scrollTop` 要加上同样多的
 * 像素，阅读位置才不动；内容只增在下方时差值为 0（不需要滚动）。
 */
export function anchorScrollDelta(offsetBefore: number, offsetAfter: number): number {
  return offsetAfter - offsetBefore;
}

/** 滚动容器的窄接口：只依赖真实用到的几个成员（RNW 的 `ScrollView` 结构上满足它）。 */
interface ScrollHandle {
  scrollTo?: (options: { readonly y?: number; readonly animated?: boolean }) => void;
  getScrollableNode?: () => unknown;
}

/** 用户输入在可见带里的分布：看得到的条数，以及上方 / 下方各是哪几条（按时间线次序的下标）。 */
export interface UserInputDistribution {
  readonly visible: number;
  readonly above: readonly number[];
  readonly below: readonly number[];
}

/**
 * 把用户输入条目按「可见带上方 / 可见带内 / 可见带下方」分类（跳转判定的**唯一**一处输入）。
 * `visibleHeight` 是可见带的高度（中栏滚动容器的可视高度；回退时是视口高度）。
 */
export function classifyUserInputs(rects: readonly ViewportRect[], visibleHeight: number): UserInputDistribution {
  const above: number[] = [];
  const below: number[] = [];
  let visible = 0;
  rects.forEach((rect, index) => {
    if (rect.bottom <= JUMP_VIEWPORT_GUARD_PX) above.push(index);
    else if (rect.top >= visibleHeight - JUMP_VIEWPORT_GUARD_PX) below.push(index);
    else visible += 1;
  });
  return { visible, above, below };
}

/**
 * 把**视口坐标**（`getBoundingClientRect()` 的原值）换算到可见带内再分类：`containerTop` 是
 * 可见带（中栏时间线滚动容器）的顶边，`containerHeight` 是它的高度。容器顶边以上被裁掉的
 * 条目因此落在「上方」，不会因为还在视口里就算成看得到。
 *
 * 容器缺失时调用方传 `0` 与视口高度，于是退化为视口坐标——与 {@link classifyUserInputs} 同口径。
 * 这一步换算是测量（`measure`）与判定之间**唯一**的落点，页面测试直接喂视口几何。
 */
export function classifyUserInputsInBand(
  rects: readonly ViewportRect[],
  containerTop: number,
  containerHeight: number,
): UserInputDistribution {
  return classifyUserInputs(
    rects.map((rect) => ({ top: rect.top - containerTop, bottom: rect.bottom - containerTop })),
    containerHeight,
  );
}

/**
 * 跳转胶囊的呈现：**没有用户输入**、或至少有一条在可见带内时不呈现（spec 的两条场景）。
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
 * 四条规则合起来保证「窄屏不丢信息」「窄屏单栏吃掉高度」与「宽屏照旧三栏」：
 *
 * 1. **没有接线切换控件** → 三栏都呈现。藏了一栏却没有任何入口，就是静默丢弃信息；
 * 2. **宽屏 + 缺省取值** → 三栏都呈现。`DEFAULT_CONVERSATION_SECONDARY_VIEW` 是**窄屏**的默认
 *    形态（模型层的注释同样这么写），而宽屏的默认形态就是三栏并列（spec「对话页的宽屏两种形态」
 *    的第一句是 MUST）；用户在宽屏显式选过别的取值后照办；
 * 3. **窄屏 + 「全部」** → 归一为对话本体。窄屏是纵向堆叠，三栏均分会把时间线挤成 0 高；
 *    「全部」只属于宽屏的三栏并列形态；
 * 4. 其余情形照取值来。
 */
export function effectiveSecondaryView(input: {
  readonly requested: ConversationSecondaryView | null | undefined;
  readonly switchable: boolean;
  readonly wide: boolean;
}): ConversationSecondaryView {
  if (input.wide || !input.switchable) return "all";
  const requested = input.requested ?? DEFAULT_CONVERSATION_SECONDARY_VIEW;
  if (input.wide && requested === DEFAULT_CONVERSATION_SECONDARY_VIEW) return "all";
  if (!input.wide && requested === "all") return "timeline";
  return requested;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    // 路由容器：外壳按视口高度固定后，它必须作为弹性子项吃掉剩余高度（`<section>` 原样是
    // 块级元素，只靠内层 `flexGrow` 填不满父级）。语义用 `role="region"` 补回。
    section: {
      flexGrow: 1,
      flexShrink: 1,
      flexBasis: 0,
      minHeight: 0,
    },
    // 对话路由由外壳按视口高度固定（`contentFill`）：这里只把剩余高度逐层传下去，滚动交给
    // 三栏各自的 `ScrollView`。`minHeight: 0` 让弹性子项真的能收缩（否则内容会把输入区挤出视口）。
    root: {
      flexGrow: 1,
      flexShrink: 1,
      minHeight: 0,
      flexDirection: "column",
      gap: 0,
      padding: 0,
      backgroundColor: colors.panel,
    },
    header: {
      flexShrink: 0,
      paddingVertical: 12,
      paddingHorizontal: 20,
      minHeight: 56,
      backgroundColor: colors.panel,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    // 单行会话头（原型 `.convhead`）：标题、状态胶囊、Agent 名、弹性空白、Agent 胶囊靠右。
    headerRow: {
      flexDirection: "row",
      flexWrap: "nowrap",
      alignItems: "center",
      gap: spacing.sm,
      minWidth: 0,
    },
    headerSpacer: {
      flexGrow: 1,
      flexShrink: 1,
      flexBasis: 0,
    },
    title: {
      flexShrink: 1,
      minWidth: 0,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      // 原型会话标题 15.5px：比正文大一档但不超过 17（`fontSize.lg`），取 md + 1.5。
      fontSize: fontSize.md + 1.5,
      fontWeight: "600",
    },
    // 双击命中区与文本同样式：没有额外的编辑按钮/铅笔图标，只有一个可双击的标题。
    titleHit: {
      flexShrink: 1,
      minWidth: 0,
    },
    titleInput: {
      flexShrink: 1,
      minWidth: 120,
      maxWidth: 360,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md + 1.5,
      fontWeight: "600",
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.accent,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    titleError: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      marginTop: spacing.xs,
    },
    titleNotice: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      marginTop: spacing.xs,
    },
    agent: {
      flexShrink: 1,
      minWidth: 0,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
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
    popoverTitle: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
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
    // 有 Agent 处于连接态时用「运行中」色，否则用空闲色（数据来自连接事件覆盖层；未知状态
    // 不等于已断开，因此只区分「至少一个在跑」与「没有」）。
    dotRunning: {
      backgroundColor: colors.ok,
    },
    dotIdle: {
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
      flexGrow: 1,
      flexShrink: 1,
      minHeight: 0,
      flexDirection: "row",
      // 三栏等高：两侧栏目的「吸底」元素才有底可吸（窄屏是堆叠，不受影响）。
      alignItems: "stretch",
      gap: 0,
    },
    bodyNarrow: {
      flexGrow: 1,
      flexShrink: 1,
      minHeight: 0,
      flexDirection: "column",
      gap: spacing.md,
    },
    // 三栏的列宽就是原型的 `300 / 1fr / 360`：两侧用 `flexBasis` 定宽（RN 没有 grid），中栏吃掉余量。
    sessionsColumn: {
      overflow: "hidden",
      flexBasis: 300,
      flexGrow: 0,
      // 宽屏但视口不足时允许两侧收缩：固定 300 + 360 在 720–900 区间会把中栏压成几十像素，
      // 输入区随之换行成一条竖线。大屏仍按原型 300 / 1fr / 360 取宽。
      flexShrink: 1,
      minWidth: 160,
      minHeight: 0,
      flexDirection: "column",
      borderRightWidth: 1,
      borderRightColor: colors.line,
      backgroundColor: colors.panel,
    },
    timelineColumn: {
      flexGrow: 1,
      flexShrink: 1,
      // 中栏的可用下限：低于它时输入区会被压成竖条，时间线滚动区也会失去高度。
      minWidth: 300,
      minHeight: 0,
      flexDirection: "column",
      gap: 0,
      backgroundColor: colors.panel,
    },
    filesColumn: {
      flexBasis: 360,
      flexGrow: 0,
      flexShrink: 1,
      minWidth: 200,
      minHeight: 0,
      flexDirection: "column",
      gap: spacing.md,
      borderLeftWidth: 1,
      borderLeftColor: colors.line,
      backgroundColor: colors.panel2,
    },
    // 窄屏：唯一可见的那一栏吃掉正文高度（次级视图把其余两栏 `display: none`）。
    narrowColumn: {
      flexBasis: 0,
      flexGrow: 1,
      flexShrink: 1,
    },
    // 三栏各自的滚动容器：宽屏下左右两栏固定宽、中栏吃掉余量，滚动互不影响。
    columnScroll: {
      flexGrow: 1,
      flexShrink: 1,
      flexBasis: 0,
      minHeight: 0,
      width: "100%",
    },
    columnScrollBody: {
      flexGrow: 1,
      gap: spacing.sm,
    },
    timelineScrollBody: {
      gap: spacing.md,
      paddingTop: 22,
      paddingHorizontal: 20,
      paddingBottom: 8,
    },
    // 输入区吸在中栏底部：不参与中栏的弹性分配，时间线滚到哪它都留在原位。
    composerDock: {
      flexShrink: 0,
    },
    hidden: {
      display: "none",
    },
    sidebar: {
      overflow: "hidden",
      flexGrow: 1,
      flexShrink: 1,
      flexBasis: 0,
      minHeight: 0,
      gap: 0,
    },
    sidebarFixed: {
      flexShrink: 0,
      backgroundColor: colors.panel,
    },
    sidebarHead: {
      flexShrink: 0,
      gap: 0,
      paddingVertical: 16,
      paddingHorizontal: 16,
      borderBottomWidth: 1,
      borderBottomColor: colors.line2,
    },
    sidebarTopRow: {
      flexDirection: "column",
      alignItems: "stretch",
      gap: 10,
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
      fontSize: 15,
      lineHeight: 22,
      fontWeight: "600",
    },
    sidebarPathRow: {
      marginTop: 10,
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.sm,
      minWidth: 0,
    },
    // 「路径」是一个标签，不是值：与等宽路径取同一行，但用正文字族与低对比色。
    sidebarPathLabel: {
      flexShrink: 0,
      minWidth: 34,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    sidebarPath: {
      flexShrink: 1,
      minWidth: 0,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: 13,
      lineHeight: 20,
      // 等宽「值」行取 500 档（原型 `.dirctx .pt`），不在这里写字面量。
      fontWeight: fontWeight.value,
    },
    searchWrap: {
      marginTop: 10,
      flexDirection: "row",
      alignItems: "center",
      gap: 8,
      paddingHorizontal: 10,
      paddingVertical: 5,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    searchIcon: {
      flexShrink: 0,
      color: colors.ink3,
      fontSize: fontSize.sm,
    },
    search: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 0,
      padding: 0,
      borderWidth: 0,
      backgroundColor: "transparent",
      lineHeight: 20.8,
      outlineWidth: 0,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    sidebarList: {
      gap: 0,
    },
    // 选中行：左侧 2px accent 边（原型 `.srow.on` 的 `border-left-color`）。未选中时保留同宽的
    // 透明边，行内容不会因选中而左右跳动。
    sidebarRow: {
      borderRadius: 0,
      borderLeftWidth: 2,
      borderLeftColor: "transparent",
    },
    sidebarRowSelected: {
      backgroundColor: colors.accentSoft,
      borderLeftColor: colors.accent,
    },
    sidebarPager: {
      flexShrink: 0,
      justifyContent: "center",
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: 10,
      paddingHorizontal: 16,
      borderTopWidth: 1,
      borderTopColor: colors.line2,
    },
    sidebarFoot: {
      flexShrink: 0,
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: 10,
      paddingVertical: 10,
      paddingHorizontal: 16,
      borderTopWidth: 1,
      borderTopColor: colors.line2,
      backgroundColor: colors.panel,
    },
    sidebarBack: {
      alignSelf: "flex-start",
      paddingVertical: 5,
      paddingHorizontal: 11,
    },
    sidebarCreate: {
      paddingVertical: 5,
      paddingHorizontal: 11,
      borderRadius: 6,
      backgroundColor: colors.accentSoft,
    },
    sidebarCreateText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: 13,
      fontWeight: "600",
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
      // 固定在**视口**底部居中（原型把胶囊绝对定位在滚动栏内）。它是中栏滚动容器的兄弟节点：
      // 包含块因此是视口，胶囊不随内容滚动、也不会被滚动容器的 overflow 裁掉；中栏被次级视图
      // 藏起来时，这个入口随中栏一起消失（那时时间线本来就不在台上）。
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
    /** 「回到底部」入口：与跳转胶囊同形，放在它上方一档（两者可同时出现：方向不同）。 */
    scrollPill: {
      left: "50%",
      bottom: spacing.lg + 48,
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderWidth: 1,
      borderColor: colors.accent,
      borderRadius: radius.lg,
      backgroundColor: colors.accentSoft,
      zIndex: zIndex.topBar,
      ...webStyle({ position: "fixed", transform: "translateX(-50%)" }),
    },
    scrollPillText: {
      color: colors.accentInk,
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
      display: "none",
      flexShrink: 0,
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
  disabled: requestedDisabled,
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

  const disabled = requestedDisabled || loading;
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
        {label}
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
  // 「有没有 Agent 在跑」只从面板已有的连接状态得出：`connected` 视为运行中，其余（含未知）
  // 取空闲色。未知状态不会被降级成「已断开」——那一条仍由每行的连接文案承担。
  const anyRunning = panel.agents.some((agent) => agent.connection === "connected");
  const dotTone = anyRunning ? "running" : "idle";

  return (
    <View style={styles.chipWrap} dataSet={{ agentChip: "agents", agentChipOpen: String(open) }}>
      <Pressable
        role="button"
        style={styles.chip}
        dataSet={{ action: "toggle-agent-chip", agentChipCount: String(panel.agents.length), agentChipDot: dotTone }}
        onPress={() => {
          setOpen(!open);
        }}
      >
        <View style={[styles.dot, anyRunning ? styles.dotRunning : styles.dotIdle]} />
        <Text style={styles.chipText}>{`${String(panel.agents.length)} 个 Agent ▾`}</Text>
      </Pressable>

      <View
        style={open ? styles.popover : [styles.popover, styles.popoverHidden]}
        dataSet={{ agentPopover: open ? "open" : "closed" }}
      >
        <Text style={styles.popoverTitle} dataSet={{ agentPopoverTitle: "本会话涉及的 Agent" }}>
          本会话涉及的 Agent
        </Text>
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
              <View
                style={[styles.dot, agent.connection === "connected" ? styles.dotRunning : styles.dotIdle]}
                dataSet={{ agentRowDot: agent.connection }}
              />
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
 * 左栏：目录上下文（返回目录列表、展示名、进本目录详情、带「路径」标签的逐字路径）+ 带图标的
 * 目录内搜索 + 同目录会话列表 + 「已显示 N / M」与继续加载 + 吸底的新建会话入口。
 *
 * 搜索框是**受控**的（值来自 `model.query`，输入回到仓库的 `conversationSidebarQuery`）：它只
 * 影响本栏列表（模型层保证），因此「搜索不改中栏」在这里没有表达方式。
 */
function Sidebar({
  model,
  onOpenDirectoryList,
  onOpenSession,
  onQueryChange,
  onPageChange,
  onCreateSession,
}: {
  readonly model: ConversationSidebarModel;
  readonly onOpenDirectoryList: (() => void) | undefined;
  readonly onOpenSession: ((sessionId: string) => void) | undefined;
  readonly onQueryChange: ((query: string) => void) | undefined;
  readonly onPageChange: ((pageInput: string) => void) | undefined;
  readonly onCreateSession: ((directoryId: string) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const directory = model.directory;
  return (
    <View style={styles.sidebar} dataSet={{ conversationSidebar: "true" }}>
      <View style={styles.sidebarFixed} dataSet={{sidebarRegion:"header"}}>
      {directory === null ? null : (
        <View style={styles.sidebarHead} dataSet={{ sidebarDirectory: directory.directoryId }}>
          {/* 返回目录列表，目录名独占下一行。 */}
          <View style={styles.sidebarTopRow}>
            <Pressable
              role="button"
              style={[styles.linkButton, styles.sidebarBack]}
              dataSet={{ action: "open-directory-list" }}
              onPress={() => {
                onOpenDirectoryList?.();
              }}
            >
              <Text style={styles.linkButtonText}>‹ 目录</Text>
            </Pressable>
            <View style={styles.sidebarDirNames}>
              <Text style={styles.sidebarName} dataSet={{ sidebarDirectoryName: directory.displayName }}>
                {directory.displayName}
              </Text>
            </View>
          </View>

          {/* 路径逐字来自快照目录条目，始终在场；「路径」是标签，不是值的一部分。 */}
          <View style={styles.sidebarPathRow}>
            <Text style={styles.sidebarPathLabel} dataSet={{ sidebarDirectoryPathLabel: "路径" }}>
              路径
            </Text>
            <Text style={styles.sidebarPath} numberOfLines={1} dataSet={{ sidebarDirectoryPath: directory.path }}>
              {directory.path}
            </Text>
          </View>

          {onQueryChange === undefined ? null : (
            <View style={styles.searchWrap} dataSet={{ sidebarSearchWrap: "true" }}>
              <Text style={styles.searchIcon}>🔍</Text>
              <TextInput
                placeholderTextColor={theme.colors.ink3}
                value={model.query}
                placeholder="在本目录内搜索"
                style={styles.search}
                dataSet={{ sidebarSearch: "true" }}
                onChangeText={(text: string) => {
                  onQueryChange(text);
                }}
              />
            </View>
          )}
        </View>
      )}

      </View>
      <ScrollView style={styles.columnScroll} contentContainerStyle={{flexGrow:0}} dataSet={{ conversationScroll: "sessions", sidebarRegion:"body" }}>
      {directory === null || model.rows.length === 0 ? null : (
        <View role="list" style={styles.sidebarList} dataSet={{ sidebarSessions: "list" }}>
          {model.rows.map((row, index) => {
            const selected = row.id === model.selectedSessionId;
            const ordinal = (model.page - 1) * CONVERSATION_SIDEBAR_PAGE_SIZE + index + 1;
            return (
              // 选中态只在行的外框上（`SessionRow` 不做选中判定，也不该为对话页长出第二种选中逻辑）。
              // 左栏用紧凑形态：只有圆点/标题/状态词/时间，Agent 名与行底边不呈现。
              <View
                key={row.id}
                style={selected ? [styles.sidebarRow, styles.sidebarRowSelected] : styles.sidebarRow}
                dataSet={{ sidebarSession: row.id, sidebarSessionSelected: String(selected) }}
              >
                {onOpenSession === undefined ? (
                  <SessionRow session={row} variant="compact" ordinal={ordinal} />
                ) : (
                  <SessionRow
                    session={row}
                    variant="compact"
                    ordinal={ordinal}
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

      </ScrollView>
      <View style={styles.sidebarFixed} dataSet={{sidebarRegion:"footer"}}>
      {directory === null || onPageChange === undefined ? null : (
        <View style={styles.sidebarPager} dataSet={{ sidebarShown: String(model.shown), sidebarTotal: String(model.total) }}>
          <Pager
            page={model.page}
            pageCount={model.pageCount}
            onPageChange={onPageChange}
            data={{ pager: "sidebarPager", page: "sidebarPage", pageCount: "sidebarPageCount", input: "sidebarPageInput", inputValue: "sidebarPageInputValue" }}
          />
        </View>
      )}


      {directory === null || onCreateSession === undefined ? null : (
        <View style={styles.sidebarFoot}>
          <Pressable
            role="button"
            style={styles.sidebarCreate}
            dataSet={{ action: "create-session-in-directory", directoryId: directory.directoryId }}
            onPress={() => {
              onCreateSession(directory.directoryId);
            }}
          >
            <Text style={styles.sidebarCreateText}>⊕ 新建会话</Text>
          </Pressable>
        </View>
      )}
      </View>
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
  secondaryView,
  onSecondaryViewChange,
  availableWidth,
  onCopyCode,
  onLinkPress,
  onFilePathPress,
  media,
  onLoadMedia,
  onReleaseMedia,
  onDownloadAttachment,
  onSubmitElicitation,
  onRefreshInteractions,
  onPickAttachment,
  onPasteAttachments,
  onRemoveAttachment,
  onRetryAttachments,
  onCancelAttachmentUpload,
  onAddPathRef,
  onRemovePathRef,
  onOpenDirectoryList,
  onOpenSession,
  onSidebarQueryChange,
  onSidebarPageChange,
  onCreateSession,
  titleEditor,
  onStartTitleEdit,
  onTitleDraftChange,
  onCommitTitle,
  onCancelTitleEdit,
  hostPanel,
  filesPanel,
  onSelectFileSource,
  onRefreshFiles,
  onExpandedFileDirectoriesChange,
  onToggleHideIgnored,
  onLoadMoreAllFiles,
  onLoadMoreGitChanges,
  onOpenFile,
  onCloseFile,
  onRetryFile,
  onDownloadFile,
  onCancelDownload,
  onReleaseDownload,
  downloadState,
  createPreviewUrl,
}: ConversationStreamProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const windowDimensions = useWindowDimensions();
  const width = availableWidth ?? windowDimensions.width;
  // 断点取自唯一一份 token（此前这里与另外两处各写了一次字面量 720）。
  const wide = width >= breakpoints.wide;
  const [leftWidth, setLeftWidth] = useState(300);
  const [rightWidth, setRightWidth] = useState(360);
  // 为中栏保留 300px；透明拖动命中区不占栏宽。
  const sideBudget = Math.max(360, width - 300);
  const displayedLeftWidth = Math.max(160, Math.min(leftWidth, sideBudget - 200));
  const displayedRightWidth = Math.max(200, Math.min(rightWidth, sideBudget - displayedLeftWidth));
  const switchable = onSecondaryViewChange !== undefined;
  // 生效取值的四条规则（含「宽屏 + 缺省取值 = 三栏」「窄屏不提供全部」与「没有切换控件时不藏栏」）
  // 见 `effectiveSecondaryView`。
  const view = effectiveSecondaryView({ requested: secondaryView, switchable, wide });
  const hiddenColumns = hiddenConversationColumns(view);
  /**
   * 窄屏的「单栏吃高度」只在**确实藏了栏**时应用：三栏并列（没有切换控件，或宽屏的「全部」）
   * 仍按各自的定宽/堆叠样式走。窄屏请求「全部」已被 `effectiveSecondaryView` 归一为单栏。
   */
  const singleColumn = !wide && hiddenColumns.length > 0;
  // 时间线的文件改动条目复用逐条改动的诊断投影（原文可用性/事件视图都在那里）。
  const changeDiagnostics = new Map(model.fileChanges.map((change) => [change.changeId, change.diagnostics]));
  const paging = model.paging;
  const history = model.timeline.structuredHistory;
  const sidebar = model.sidebar;
  const activeFilesPanel = filesPanel ?? null;
  /**
   * 中间查看区与对话**互斥但都留在 DOM**：文件打开时时间线与输入区只是 `display: none`，
   * 因此时间线的滚动锚点（元素自身的 `scrollTop`）不被卸载重置；草稿与条目展开状态本来就在
   * 页面/仓库里，组件不拥有它们。关闭后恢复的正是这一份现场，不强制滚到底。
   */
  const fileContent =
    activeFilesPanel !== null &&
    activeFilesPanel.content.kind === "file" &&
    onCloseFile !== undefined
      ? activeFilesPanel.content
      : null;
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

  // ── 会话标题的原地编辑（`design.md` D4） ───────────────────────────────────
  const titleView = titleEditor ?? null;
  const titleEditing = titleView !== null && titleView.phase !== "viewing";
  /** 上一次单击标题的时刻：双击只按时间窗判定，PWA/触屏不依赖浏览器 `dblclick`。 */
  const titleClickAt = useRef<number | null>(null);
  /** Escape 之后的 blur 被抑制一次；否则取消会被失焦保存覆盖。 */
  const suppressTitleBlur = useRef(false);
  const handleTitlePress = (): void => {
    if (titleEditing) return;
    const now = Date.now();
    const doubleClick = isSessionTitleDoubleClick(titleClickAt.current, now);
    titleClickAt.current = doubleClick ? null : now;
    if (!doubleClick) return;
    suppressTitleBlur.current = false;
    onStartTitleEdit?.();
  };
  const handleTitleKeyPress: NonNullable<TextInputProps["onKeyPress"]> = (event) => {
    // RNW 把宿主 `keydown` 交给 `onKeyPress`：composition/229 期间 Enter 属于组词，不提交。
    const native = event.nativeEvent as { readonly key?: string; readonly isComposing?: boolean; readonly keyCode?: number };
    const action = sessionTitleKeyAction(native);
    if (action === "ignore") return;
    event.preventDefault();
    if (action === "cancel") {
      suppressTitleBlur.current = true;
      onCancelTitleEdit?.();
      return;
    }
    onCommitTitle?.("enter");
  };
  const handleTitleBlur = (): void => {
    if (suppressTitleBlur.current) {
      suppressTitleBlur.current = false;
      return;
    }
    // 提交中不再接受第二个保存意图（Enter 紧随其后的失焦因此不会派发第二条命令）。
    if (titleView?.submitting === true) return;
    onCommitTitle?.("blur");
  };

  // ── 滚动跟随与阅读锚点（spec「流式阅读保持位置」）────────────────────────────
  const sessionId = model.header.sessionId;
  const scrollRef = useRef<ScrollHandle | null>(null);
  /** 稳定的 ref 回调：每次渲染换一个函数会让 React 先摘再挂同一个节点。 */
  const assignScrollRef = useCallback((node: unknown) => {
    scrollRef.current = node as ScrollHandle | null;
  }, []);
  /** 右栏文件列表的滚动容器；窄屏打开文件时它被 `display:none`，需要单独保存位置。 */
  const filesScrollRef = useRef<ScrollHandle | null>(null);
  const assignFilesScrollRef = useCallback((node: unknown) => {
    filesScrollRef.current = node as ScrollHandle | null;
  }, []);
  const followingRef = useRef(true);
  const [following, setFollowing] = useState(true);
  /** 上读期间有新内容到达（只影响提示措辞，不改变滚动位置）。 */
  const [unseen, setUnseen] = useState(false);
  /** 上读时的锚点：第一条仍在可视带内的条目键与它相对容器顶边的偏移。 */
  const anchorRef = useRef<{ readonly key: string; readonly offset: number } | null>(null);
  /** 会话切换前的位置（按会话标识）：切回时恢复，不强制回到最新。 */
  const savedScroll = useRef<Map<string, number> | null>(null);
  /** 首次真正记录位置时才建表：渲染路径上不为此分配。 */
  const scrollPositions = (): Map<string, number> => {
    if (savedScroll.current === null) savedScroll.current = new Map<string, number>();
    return savedScroll.current;
  };
  const previousSession = useRef(sessionId);
  /** 打开文件查看前的时间线位置；关闭时恢复（查看期间时间线只是被隐藏）。 */
  const savedBeforeFile = useRef<{ readonly sessionId: string; readonly offsetY: number } | null>(null);
  /** 打开文件查看前的右栏文件列表位置；窄屏关闭后恢复文件列表而不是回到顶部。 */
  const savedFilesScroll = useRef<{ readonly sessionId: string; readonly offsetY: number } | null>(null);

  /** 真实滚动容器（RNW 的 `ScrollView` 在网页侧就是可滚动的宿主元素）。node 侧为 `null`。 */
  const scroller = (): HTMLElement | null => {
    const node = scrollRef.current?.getScrollableNode?.();
    if (typeof HTMLElement === "undefined") return null;
    return node instanceof HTMLElement ? node : null;
  };
  const filesScroller = (): HTMLElement | null => {
    const node = filesScrollRef.current?.getScrollableNode?.();
    if (typeof HTMLElement === "undefined") return null;
    return node instanceof HTMLElement ? node : null;
  };
  const scrollToY = (y: number): void => {
    scrollRef.current?.scrollTo?.({ y, animated: false });
  };
  /**
   * 打开文件前同步记下两个滚动容器的位置。
   *
   * 不能在文件视图的 effect 里再读：那时两栏已经 `display:none`，浏览器会把 `scrollTop`
   * 报成 0。捕获放在触发打开的同一次事件里，关闭后再恢复。
   */
  const captureOpenFileScroll = (): void => {
    const host = scroller();
    if (host !== null) savedBeforeFile.current = { sessionId, offsetY: host.scrollTop };
    const filesHost = filesScroller();
    if (filesHost !== null) savedFilesScroll.current = { sessionId, offsetY: filesHost.scrollTop };
  };
  /** 记下当前阅读锚点（上读时才会用到；没有条目时锚点为 `null`）。 */
  const captureAnchor = (): void => {
    const host = scroller();
    if (host === null) return;
    const hostTop = host.getBoundingClientRect().top;
    for (const node of host.querySelectorAll<HTMLElement>("[data-timeline-key]")) {
      const rect = node.getBoundingClientRect();
      if (rect.bottom <= hostTop + 1) continue;
      anchorRef.current = { key: node.dataset["timelineKey"] ?? "", offset: rect.top - hostTop };
      return;
    }
    anchorRef.current = null;
  };
  /** 内容高度变化后把锚点移回原偏移：内容在上方增长（更早历史、媒体加载）时补偿 `scrollTop`。 */
  const restoreAnchor = (): void => {
    const anchor = anchorRef.current;
    const host = scroller();
    if (anchor === null || host === null) return;
    const hostTop = host.getBoundingClientRect().top;
    for (const node of host.querySelectorAll<HTMLElement>("[data-timeline-key]")) {
      if (node.dataset["timelineKey"] !== anchor.key) continue;
      const delta = anchorScrollDelta(anchor.offset, node.getBoundingClientRect().top - hostTop);
      if (delta !== 0) scrollToY(host.scrollTop + delta);
      return;
    }
  };
  const handleContentSizeChange = (): void => {
    // 查看文件期间时间线被隐藏，维度变化不是「新内容」，不能借机滚到底。
    if (fileContent !== null) return;
    const host = scroller();
    if (host === null) return;
    if (followingRef.current) {
      scrollToY(host.scrollHeight);
      return;
    }
    restoreAnchor();
  };
  const handleTimelineScroll = (): void => {
    measureRef.current();
    const host = scroller();
    if (host === null) return;
    const next = isFollowingBottom({
      offsetY: host.scrollTop,
      contentHeight: host.scrollHeight,
      viewportHeight: host.clientHeight,
    });
    if (next === followingRef.current) {
      if (next && unseen) setUnseen(false);
      return;
    }
    followingRef.current = next;
    setFollowing(next);
    if (next) setUnseen(false);
    else captureAnchor();
  };
  const scrollToBottom = (): void => {
    const host = scroller();
    if (host === null) return;
    followingRef.current = true;
    setFollowing(true);
    setUnseen(false);
    scrollToY(host.scrollHeight);
  };

  /**
   * 新内容到达（条目数、最后一条标识或流式正文长度变化）且用户上读时，只点亮提示，
   * 不动滚动位置。展开/收起详情不改变这些键，因此不会把用户自己的操作误报成新内容。
   */
  const lastEntry = model.timeline.entries[model.timeline.entries.length - 1];
  const lastMessage = model.messages[model.messages.length - 1];
  const contentKey = `${String(model.timeline.entries.length)}|${lastEntry?.key ?? ""}|${String(
    model.messages.length,
  )}|${lastMessage?.text.length ?? 0}`;
  const firstContent = useRef(true);
  useEffect(() => {
    if (firstContent.current) {
      firstContent.current = false;
      return;
    }
    if (!followingRef.current) setUnseen(true);
  }, [contentKey]);

  /** 会话切换：先记下旧会话的位置，再在新会话的布局完成后恢复（没有记录即跟随最新）。 */
  useEffect(() => {
    if (previousSession.current === sessionId) return undefined;
    const host = scroller();
    if (host !== null) scrollPositions().set(previousSession.current, host.scrollTop);
    previousSession.current = sessionId;
    anchorRef.current = null;
    const saved = scrollPositions().get(sessionId);
    followingRef.current = saved === undefined;
    setFollowing(saved === undefined);
    setUnseen(false);
    if (saved === undefined) return undefined;
    const timer = setTimeout(() => {
      scrollToY(saved);
    }, 0);
    return () => {
      clearTimeout(timer);
    };
  }, [sessionId]);

  /** 打开文件查看时保存时间线位置，关闭时恢复（不强制滚到底）。 */
  const fileOpen = fileContent !== null;
  useEffect(() => {
    if (fileOpen) return;
    const host = scroller();
    const saved = savedBeforeFile.current;
    savedBeforeFile.current = null;
    if (host !== null && saved !== null && saved.sessionId === sessionId) {
      // 关闭后这次恢复必须同步落到节点上：`scrollTo` 在刚取消 `display:none` 的同一帧
      // 可能排到布局之后，而内容尺寸回调会先按「跟随底部」改写位置。
      host.scrollTop = saved.offsetY;
      const next = isFollowingBottom({
        offsetY: saved.offsetY,
        contentHeight: host.scrollHeight,
        viewportHeight: host.clientHeight,
      });
      followingRef.current = next;
      setFollowing(next);
    }
    const filesSaved = savedFilesScroll.current;
    savedFilesScroll.current = null;
    if (filesSaved !== null && filesSaved.sessionId === sessionId) {
      const filesHost = filesScroller();
      if (filesHost !== null) filesHost.scrollTop = filesSaved.offsetY;
    }
  }, [fileOpen, sessionId]);

  // ── 跳转入口的测量（只属于渲染现场的事实：滚动位置与元素几何）───────────────────
  const rootRef = useRef<HTMLElement | null>(null);
  // RNW 的 `View` 在网页侧就是宿主元素；测量代码只依赖它的 `querySelector` 与几何 API。
  // 稳定的回调避免每次渲染都让 React 先摘再挂同一个节点。
  const assignRootRef = useCallback((node: unknown) => {
    rootRef.current = node as HTMLElement | null;
  }, []);
  /**
   * 中栏自己的滚动容器（`ScrollView`）滚动的测量入口。
   *
   * 对话页的滚动者是三栏各自的 `ScrollView`，`window` 的 scroll 事件不再代表「时间线在滚」；
   * 由 `ScrollView.onScroll` 调到这里，跳转入口才会随中栏滚动更新。node 侧没有触屏/滚动事件，
   * 这个函数保持空实现（跳转入口因此不呈现）。
   */
  const measureRef = useRef<() => void>(() => undefined);
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
      // 可见带是**中栏时间线滚动容器**的上下边：会话头与来源条在它上面、输入区在它下面，
      // 用视口高度会把这两块里露出的（或被容器裁掉的）消息算进「看得到」。容器缺失时退回
      // 视口坐标，三栏未进入滚动布局（例如旧窄屏高度不足）时行为不变。
      const container = host.querySelector<HTMLElement>('[data-conversation-scroll="timeline"]');
      const bounds = container === null ? null : container.getBoundingClientRect();
      const rects = userInputNodes(host, ids).map((node) => {
        const rect = node.getBoundingClientRect();
        return { top: rect.top, bottom: rect.bottom };
      });
      setDistribution(
        classifyUserInputsInBand(
          rects,
          bounds === null ? 0 : bounds.top,
          bounds === null ? window.innerHeight : bounds.height,
        ),
      );
    };
    measureRef.current = measure;
    measure();
    window.addEventListener("scroll", measure, { passive: true });
    window.addEventListener("resize", measure);
    return () => {
      measureRef.current = () => undefined;
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
    <View
      ref={assignRootRef}
      role="region"
      style={styles.section}
      dataSet={{ route: "session", sessionId: model.header.sessionId }}
    >
      <View style={styles.root} dataSet={{ conversationPage: "true" }}>
        {/* 切换控件在三栏之外：中栏被次级视图藏起来时，这里仍是切回去的唯一入口。 */}
        {switchable && !wide ? (
          <SegmentControl
            label="conversation-secondary-view"
            value={view}
            options={NARROW_SECONDARY_VIEW_OPTIONS}
            onChange={(next) => {
              onSecondaryViewChange?.(next);
            }}
          />
        ) : null}

        <View style={wide ? styles.bodyWide : styles.bodyNarrow} dataSet={{ conversationBody: "true" }}>
          {/* 左栏：目录上下文（两个目录入口 + 路径标签 + 搜索图标）+ 同目录会话 + 已显示数 + 新建。 */}
          <View
            style={
              hiddenColumns.includes("sessions")
                ? [styles.sessionsColumn, singleColumn ? styles.narrowColumn : null, styles.hidden]
                : [styles.sessionsColumn, singleColumn ? styles.narrowColumn : null,
                    wide ? { flexBasis: displayedLeftWidth, flexShrink: 0 } : null]
            }
            dataSet={{ conversationColumn: "sessions" }}
          >
            <View
              style={styles.columnScroll}
            >
              <Sidebar
                model={sidebar}
                onOpenDirectoryList={onOpenDirectoryList}
                onOpenSession={onOpenSession}
                onQueryChange={onSidebarQueryChange}
                onPageChange={onSidebarPageChange}
                onCreateSession={onCreateSession}
              />
            </View>
          </View>

          {wide ? <ConversationColumnResizer side="left" width={displayedLeftWidth}
            maxWidth={sideBudget - displayedRightWidth} onChange={setLeftWidth} /> : null}

          {/* 中栏：会话头 → 时间线（独立滚动）→ 输入区（吸底）。 */}
          <View
            style={
              hiddenColumns.includes("timeline")
                ? [styles.timelineColumn, singleColumn ? styles.narrowColumn : null, styles.hidden]
                : [styles.timelineColumn, singleColumn ? styles.narrowColumn : null]
            }
            dataSet={{ conversationColumn: "timeline" }}
          >
            <View
              style={styles.header}
              dataSet={{ sessionHeader: model.header.sessionId, sessionHeaderLayout: "single-line" }}
            >
              <View style={styles.headerRow}>
                {titleView !== null && titleView.phase !== "viewing" ? (
                  // 原地输入：输入框为空，当前有效标题只作为灰色占位（占位不是真实输入）。
                  // 不输入直接确认 = 保留旧标题；提交中保留输入框但不再可编辑。
                  <TextInput
                    value={titleView.draft}
                    placeholder={titleView.placeholder}
                    placeholderTextColor={theme.colors.ink3}
                    editable={!titleView.submitting}
                    autoFocus
                    style={styles.titleInput}
                    dataSet={{ sessionTitleInput: "true", sessionTitlePhase: titleView.phase }}
                    onChangeText={(text: string) => {
                      onTitleDraftChange?.(text);
                    }}
                    onKeyPress={handleTitleKeyPress}
                    onBlur={handleTitleBlur}
                  />
                ) : (
                  // 唯一入口是双击这段标题：没有编辑按钮、铅笔图标或恢复 Agent 标题的操作。
                  <Pressable
                    style={styles.titleHit}
                    dataSet={{ sessionTitleEntry: "double-click" }}
                    onPress={handleTitlePress}
                  >
                    <Text style={styles.title} numberOfLines={1} dataSet={{ sessionTitle: "true" }}>
                      {model.header.title}
                    </Text>
                  </Pressable>
                )}
                {model.header.state === null ? (
                  // 摘要尚未取得：说明「未知」，MUST NOT 编造空闲（更没有旧七态的关闭）。
                  <Text style={styles.agent} dataSet={{ sessionState: "unknown" }}>
                    状态未知
                  </Text>
                ) : (
                  <StatusPill state={model.header.state} />
                )}
                <Text style={styles.agent} numberOfLines={1} dataSet={{ sessionAgent: model.header.agentName }}>
                  {model.header.agentName}
                </Text>
                <View style={styles.headerSpacer} />
                {hostPanel === null || hostPanel === undefined ? null : <AgentChip panel={hostPanel} />}
              </View>
              {/* 失败/不可用原因就在会话头内：仍在当前页时保留输入并显示原因；离页后只留下
                  可归因原会话的提示，不跳回也不恢复草稿。 */}
              {titleView !== null && titleView.error !== null ? (
                <Text style={styles.titleError} dataSet={{ sessionTitleError: "true" }}>
                  {titleView.error}
                </Text>
              ) : null}
              {titleView !== null && titleView.notice !== null ? (
                <Text style={styles.titleNotice} dataSet={{ sessionTitleNotice: "true" }}>
                  {titleView.notice}
                </Text>
              ) : null}
            </View>

            {/* 中间互斥查看区：文件打开时它占据时间线的位置；时间线/输入区仍在 DOM（隐藏）。 */}
            {fileContent === null || onCloseFile === undefined ? null : (
              <FileContentViewer
                content={fileContent}
                onClose={onCloseFile}
                onRetry={onRetryFile}
                onDownload={onDownloadFile}
                onCancelDownload={onCancelDownload}
                onReleaseDownload={onReleaseDownload}
                downloadState={downloadState ?? null}
                createPreviewUrl={createPreviewUrl}
              />
            )}

            <ScrollView
              ref={assignScrollRef}
              style={fileContent === null ? styles.columnScroll : [styles.columnScroll, styles.hidden]}
              contentContainerStyle={styles.timelineScrollBody}
              dataSet={{
                conversationScroll: "timeline",
                scrollFollow: String(following),
                scrollUnseen: String(unseen),
              }}
              onScroll={handleTimelineScroll}
              onContentSizeChange={handleContentSizeChange}
              scrollEventThrottle={32}
            >
            {answerableCount === 0 ? null : (
              <View style={styles.jumpBar} dataSet={{ pendingJumpBar: String(answerableCount) }}>
                <Text style={styles.jumpBarText}>
                  {`⏸ 待处理 ${String(answerableCount)} 项`}
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
                  onPress={() => {
                    captureAnchor();
                    onLoadEarlier();
                  }}
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
                    onPress={() => {
                      captureAnchor();
                      onLoadEarlierHistory();
                    }}
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
              sessionId={sessionId}
              onOpenDiagnostics={onOpenDiagnostics}
              onAnswerInteraction={onAnswerInteraction}
              onCopyCode={onCopyCode}
              onLinkPress={onLinkPress}
              expandedEntries={expandedTimelineEntries}
              onToggleEntry={onToggleTimelineEntry}
              changeDiagnostics={changeDiagnostics}
              openChangeDiagnosticsEventId={openChangeDiagnosticsEventId ?? null}
              onOpenChangeDiagnostics={onOpenChangeDiagnostics}
              media={media}
              onLoadMedia={onLoadMedia}
              onReleaseMedia={onReleaseMedia}
              onDownloadAttachment={onDownloadAttachment}
              onSubmitElicitation={onSubmitElicitation}
              onRefreshInteractions={onRefreshInteractions}
              onFilePathPress={onFilePathPress}
            />
            </ScrollView>

            {/* 上读时的新内容/回到底部入口：不自动改变滚动位置，只有用户主动点它才恢复跟随。 */}
            {following || fileContent !== null ? null : (
              <Pressable
                role="button"
                style={styles.scrollPill}
                dataSet={{ action: "scroll-to-bottom", scrollUnseen: String(unseen) }}
                onPress={scrollToBottom}
              >
                <Text style={styles.scrollPillText}>
                  {unseen ? "↓ 有新内容 · 回到底部" : "↓ 回到底部"}
                </Text>
              </Pressable>
            )}

            {/* 没有用户输入、或全部用户输入都在视口内时**不**呈现（spec 的两条场景）。
                放在滚动容器**之后**：`position: fixed` 的包含块必须是视口——RNW 的 `ScrollView`
                自带 `transform` 与 `overflow`，放进里面会把它变成随内容滚动、被裁切的定位容器。 */}
            {jump === null || fileContent !== null ? null : (
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

            {/* 输入区吸在中栏底部：时间线独立滚动时它不跟着走（宽屏与窄屏同一份结构）。
                查看文件时它仍在 DOM（草稿与输入焦点不丢），只是不占主体。 */}
            <View
              style={fileContent === null ? styles.composerDock : [styles.composerDock, styles.hidden]}
              dataSet={{ composerDock: "true" }}
            >
              <ConversationComposer
                input={model.input}
                usage={model.usage}
                settings={model.settings}
                onDraftChange={onDraftChange}
                onSubmit={onSubmitPrompt}
                onCancel={onCancelTurn}
                onSelectModel={onSelectModel}
                onSelectMode={onSelectMode}
                onPickAttachment={onPickAttachment}
                onPasteAttachments={onPasteAttachments}
                onRemoveAttachment={onRemoveAttachment}
                onRetryAttachments={onRetryAttachments}
                onCancelAttachmentUpload={onCancelAttachmentUpload}
                onAddPathRef={onAddPathRef}
                onRemovePathRef={onRemovePathRef}
              />
            </View>
          </View>

          {wide ? <ConversationColumnResizer side="right" width={displayedRightWidth}
            maxWidth={sideBudget - displayedLeftWidth} onChange={setRightWidth} /> : null}

          {/* 右栏：改动清单与差异行（独立滚动）。 */}
          <View
            style={
              hiddenColumns.includes("files")
                ? [styles.filesColumn, singleColumn ? styles.narrowColumn : null, styles.hidden]
                : [styles.filesColumn, singleColumn ? styles.narrowColumn : null,
                    wide ? { flexBasis: displayedRightWidth, flexShrink: 0 } : null]
            }
            dataSet={{ conversationColumn: "files" }}
          >
            <View style={styles.columnScroll}>
              {activeFilesPanel === null ? (
                <ScrollView style={styles.columnScroll} dataSet={{conversationScroll:"files"}}>
                <View dataSet={{ filesPanelUnavailable: "true" }} />
                </ScrollView>
              ) : (
                <SessionFilesPanel
                  onScrollRef={assignFilesScrollRef}
                  model={activeFilesPanel}
                  onOpenFile={onOpenFile === undefined ? undefined : (source, path) => {
                    captureOpenFileScroll();
                    onOpenFile(source, path);
                  }}
                  onSelectSource={onSelectFileSource}
                  onRefresh={onRefreshFiles}
                  onExpandedDirectoriesChange={onExpandedFileDirectoriesChange}
                  onToggleHideIgnored={onToggleHideIgnored}
                  onLoadMoreAllFiles={onLoadMoreAllFiles}
                  onLoadMoreGitChanges={onLoadMoreGitChanges}
                />
              )}
            </View>
          </View>
        </View>

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
    </View>
  );
}
