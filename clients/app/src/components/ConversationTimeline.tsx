/**
 * 对话时间线：消息与结构化条目在**同一个**按时间排序的容器里呈现（spec「对话页按时间线呈现消息
 * 与结构化条目」；`design.md` D1/D5/D6/D8/D10）。
 *
 * ## 组件只做呈现
 *
 * 条目序列、时间顺序、`eventId`/`messageId` 去重、生命周期归并、授权可应答性全部由
 * `features/conversation-timeline.ts` 与 `features/interaction-model.ts` 给出。本文件按
 * `entry.kind` 分支渲染，**不**做业务判定：需要判定的地方（例如「这条授权能不能应答」）在这里
 * 表现为读取模型已经算好的判别值（`availability.kind`）。
 *
 * ## 为什么只有一个列表容器
 *
 * 消息与结构化条目必须在同一条时间线上（spec），因此它们进的是**同一份** `entries`：
 * 容器带 `data-timeline="entries"`（同时保留既有消费方的 `data-message-list="messages"`——
 * 时间线就是那份列表），每条条目是它的直接子项（`role="list"`/`role="listitem"` → `<ul>`/`<li>`）。
 *
 * ## 每类卡片一个唯一锚点
 *
 * `data-timeline-card` 的取值按卡片种类区分（`turn` / `tool-call` / `terminal-output` / `plan` /
 * `file-changed` / `mode-changed` / `config-changed` / `permission-requested` /
 * `elicitation-requested` / `permission-resolved` / `elicitation-resolved` / `degraded`），
 * 加上每类自己的标识（`data-turn-id`、`data-tool-call-id`、`data-interaction-id`…）。消息用
 * `data-message-id`（既有锚点），降级用 `data-degraded-entry`（既有锚点）。
 *
 * ## 折叠块：默认值由状态决定，显式操作覆盖它（D10）
 *
 * 思考、工具调用、终端输出、计划四类可折叠（spec「结构化条目的展开收起与右栏联动」）。默认展开
 * 值由 {@link entryInProgress} 从**该条目自己的状态**推出（不是呈现顺序）。用户点表头的显式操作
 * 记在 `expandedEntries[entry.key]` 里——它覆盖默认值，且只影响这一条。这份状态住在页面级视图
 * 状态（与改动的诊断展开同构），本组件只读它、只上抛 {@link ConversationTimelineProps.onToggleEntry}。
 *
 * 收起时正文仍留在 DOM 里（`display: none`，与三栏隐藏同一口径），因此「收起」不会让内容凭空
 * 消失、也不会改变任何数据判定。
 *
 * ## 正文按不可信输入渲染
 *
 * 消息正文走 `Markdown`（原始 HTML 与远端资源都被降级）；终端输出的正文走 `CodeBlock`
 * （等宽 + 语言条 + 复制）。结构化卡片的字段一律经 `Text` 原样呈现，`acp.rawJson` 不参与
 * 渲染（D8）——它的入口是诊断抽屉（降级条目 / 工具调用）或右栏（改动）。
 */

import type { ReactElement, ReactNode } from "react";

import type { FormInputValue } from "../domain/form";
import { buildElicitationForm } from "../domain/form";
import type { InteractionAnswer, InteractionAvailability } from "../domain/interaction";
import type { AttachmentMediaView } from "../domain/media";
import type { FileChangeDiagnostics, LocalPromptState, SentPromptPhase, TimelineEntry } from "../features/conversation-timeline";
import { groupTimelineEntries } from "../features/conversation-timeline";
import type { ConversationProcessModel } from "../features/conversation-process-model";
import { ChangeDiagnosticsPanel } from "./ChangeDiagnosticsPanel";
import { ChangeDiff } from "./ChangeDiff";
import { CodeBlock } from "./CodeBlock";
import { PendingInteractionsPanel } from "./PendingInteractionsPanel";
import { DegradedEntryContent } from "./DegradedEntryContent";
import { ElicitationForm } from "./ElicitationForm";
import { InteractionAnswerButtons, InteractionAvailabilityNote } from "./InteractionAnswering";
import { Markdown } from "./Markdown";
import { TODO_STATUS_LABELS } from "./ConversationTodo";
import { MessageMedia } from "./MessageMedia";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

/** `kind` 取某一类的时间线条目类型（判别联合的成员）。 */
type EntryOf<K extends TimelineEntry["kind"]> = Extract<TimelineEntry, { readonly kind: K }>;

export interface ConversationTimelineProps {
  readonly diagnosticsMode?: boolean | undefined;
  /** 时间线条目（唯一渲染列表，已按时间排序、已去重）。 */
  readonly entries: readonly TimelineEntry[];
  /** 归属会话标识：媒体读取要求会话绑定（页面从 `model.header.sessionId` 给出）。 */
  readonly sessionId: string;
  /** 会话头显示的 Agent 名：消息署名直接显示「omp」等名称。 */
  readonly agentName?: string | undefined;
  /** 打开某条条目（降级 / 工具调用）的诊断抽屉（页面负责把 `eventId` 交给仓库）。 */
  readonly onOpenDiagnostics: (eventId: string) => void;
  /** 授权/提问的应答上抛；缺省时不渲染任何应答按钮（不是「渲染了但点了没反应」）。 */
  readonly onAnswerInteraction?: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
  /** 代码块复制入口的落点（页面接平台剪贴板能力）。 */
  readonly onCopyCode?: ((code: string, language: string) => void) | undefined;
  /** 链接被点选时上抛的目标（本组件不打开任何地址）。 */
  readonly onLinkPress?: ((href: string) => void) | undefined;
  /**
   * 折叠块的**显式**展开状态（键为 `entry.key`）：在场的条目覆盖由状态推出的默认值，缺席的条目
   * 用默认值。它只影响呈现，不参与任何数据判定（D10）。
   */
  readonly expandedEntries?: Readonly<Record<string, boolean>> | undefined;
  /**
   * 折叠块表头被点选：上抛条目键与**切换后的**展开值（页面据此写覆盖值——默认值由条目状态推出，
   * 所以翻转移位必须发生在知道有效值的那一侧）。缺省时表头不是可点控件。
   */
  readonly onToggleEntry?: ((key: string, expanded: boolean) => void) | undefined;
  /** 逐条改动的诊断数据（按 `changeId` 取）；缺省时不渲染文件改动的原文入口。 */
  readonly changeDiagnostics?: ReadonlyMap<string, FileChangeDiagnostics> | undefined;
  /** 当前展开诊断的改动事件（`eventId`）；缺省 `null` = 全部收起。 */
  readonly openChangeDiagnosticsEventId?: string | null;
  /** 打开 / 收起某处文件改动的诊断（`null` = 收起）；缺省时不渲染该入口。 */
  readonly onOpenChangeDiagnostics?: ((eventId: string | null) => void) | undefined;
  /** 已读取的消息媒体（`attachmentId → 状态`）；缺省时媒体块显示「读取中」以外的显式状态。 */
  readonly media?: ReadonlyMap<string, AttachmentMediaView> | undefined;
  /** 请求读取一条已封存附件（页面接到仓库的 `loadAttachmentMedia`）。 */
  readonly onLoadMedia?: ((attachmentId: string, sessionId: string, mimeType: string) => void) | undefined;
  /** 释放一条媒体的对象 URL（消息卸载时）。 */
  readonly onReleaseMedia?: ((attachmentId: string) => void) | undefined;
  /** 下载消息里的文件附件（页面负责生成 Blob URL 并触发保存）。 */
  readonly onDownloadAttachment?:
    | ((attachmentId: string, mimeType: string, fileName: string) => void)
    | undefined;
  /** 提交提问表单（值已通过客户端校验）。 */
  readonly onSubmitElicitation?:
    | ((interactionId: string, values: Readonly<Record<string, FormInputValue>>) => void)
    | undefined;
  /** 重新取得服务端确认的当前有效请求（刷新/换设备后用 `session.read`）。 */
  readonly onRefreshInteractions?: (() => void) | undefined;
  /** 打开 Markdown 里可解析的工作目录相对路径。 */
  readonly onFilePathPress?: ((path: string) => void) | undefined;
  /**
   * 互斥过程阶段（`model.timeline.process`）：只在提交中/恢复中/等待响应时渲染**一条**紧凑
   * 过程行，放在条目之后（不是顶部重复条）。有实际内容时模型给的 `label` 为 `null`，这里就不画。
   */
  readonly process?: ConversationProcessModel | null | undefined;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    timeline: {
      width: "100%",
      maxWidth: 840,
      alignSelf: "center",
      gap: spacing.md,
      paddingVertical: spacing.sm,
    },
    /** 轮次分组：继承时间线的条目间距（原条目是时间线的直接子节点）。 */
    turnGroup: {
      gap: spacing.md,
    },
    turnGroupEntry: {
      gap: spacing.xs,
    },
    entryHeader: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
    },
    entryHeaderUser: {
      flexDirection: "row-reverse",
    },
    waitingCursor: {
      width: 2,
      height: 20,
      backgroundColor: colors.accent,
    },
    waitingRow: {
      alignItems: "flex-start",
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
    },
    entry: {
      gap: spacing.xs,
      paddingBottom: 0,
    },
    message: {
      gap: spacing.sm,
      paddingVertical: spacing.sm,
    },
    /** 用户消息靠右：整条条目右对齐，气泡宽度跟着内容走（原型 `.msg-user`）。 */
    messageUser: {
      alignItems: "flex-end",
    },
    who: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    whoUser: {
      textAlign: "right",
    },
    body: {
      gap: spacing.xs,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel,
    },
    bodyUser: {
      maxWidth: "88%",
      paddingVertical: spacing.md,
      borderTopRightRadius: radius.sm,
      backgroundColor: colors.accentSoft,
      borderColor: colors.accentSoft,
    },
    bodyAgent: {
      maxWidth: "100%",
      paddingVertical: spacing.md,
    },
    card: {
      gap: spacing.sm,
      paddingVertical: spacing.md,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel2,
    },
    cardHead: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    kicker: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    title: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: "600",
    },
    mono: {
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
    },
    /** 工具详情：三个部分各自一小块，标签在上、值在下（值按不可信文本逐字呈现）。 */
    detail: {
      gap: spacing.sm,
    },
    detailSection: {
      gap: 2,
      padding: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    detailLabel: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
      fontWeight: "600",
    },
    detailValue: {
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
      lineHeight: 21,
    },
    /** 诊断入口（ACP 原文）：与右栏的原文入口同一语义。 */
    diagnosticsEntry: {
      alignSelf: "flex-start",
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    diagnosticsEntryText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    meta: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    time: {
      marginLeft: "auto",
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    messageTimeUser: {
      alignSelf: "flex-end",
      textAlign: "right",
    },
    messageTimeAgent: {
      marginLeft: 0,
      alignSelf: "flex-start",
      textAlign: "left",
    },
    chip: {
      paddingHorizontal: spacing.sm,
      paddingVertical: 2,
      borderRadius: radius.sm,
      color: colors.ink2,
      backgroundColor: colors.line2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    chipWarn: {
      color: colors.warn,
      backgroundColor: colors.warnSoft,
    },
    chipOk: {
      color: colors.ok,
      backgroundColor: colors.okSoft,
    },
    chipDanger: {
      color: colors.danger,
      backgroundColor: colors.dangerSoft,
    },
    chipAccent: {
      color: colors.accentInk,
      backgroundColor: colors.accentSoft,
    },
    row: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    planRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
    },
    note: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    dimNote: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    /** 本地提交条目的阶段说明（上传中/待确认/未发送）：只是实情，不冒充已受理。 */
    localState: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
      textAlign: "right",
    },
    localStateWarn: {
      color: colors.warn,
    },
    /** 过程行：时间线末尾的一条紧凑说明（等待响应/恢复中/提交中），不是顶部条。 */
    processRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderRadius: radius.md,
      backgroundColor: colors.panel2,
    },
    processLabel: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    processDetail: {
      flexShrink: 1,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    /** 失败条：整条流程的失败不塞进折叠块（原型 `.streamfail`）。 */
    failureStrip: {
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.dangerSoft,
      borderLeftWidth: 3,
      borderLeftColor: colors.danger,
      borderRadius: radius.sm,
      backgroundColor: colors.dangerSoft,
    },
    failureText: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    /** 折叠块（原型 `.fold`）：左侧一道 3px 语义色，头部一行，正文在下方。 */
    fold: {
      gap: 0,
      borderWidth: 1,
      borderColor: colors.line,
      borderLeftWidth: 3,
      borderLeftColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel,
      overflow: "hidden",
    },
    foldThink: {
      borderLeftColor: colors.think,
      backgroundColor: colors.thinkSoft,
    },
    foldTool: {
      borderLeftColor: colors.tool,
      backgroundColor: colors.toolSoft,
    },
    foldHead: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      minHeight: 44,
      flexWrap: "nowrap",
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
    },
    foldCaret: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    foldKind: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
      flexShrink: 0,
    },
    foldKindThink: {
      color: colors.think,
    },
    foldKindTool: {
      color: colors.tool,
    },
    foldTitle: {
      flexShrink: 1,
      minWidth: 0,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    foldState: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    foldStateRun: {
      color: colors.accent,
    },
    foldStateFailed: {
      color: colors.danger,
      fontWeight: "600",
    },
    foldTerminal: {
      borderLeftColor: colors.ink3,
      backgroundColor: colors.panel2,
    },
    fileChange: {
      borderLeftWidth: 3,
      borderLeftColor: colors.ok,
    },
    /** 折叠块的正文：收起时用 `display: none` 隐藏（留在 DOM 里）。 */
    foldBody: {
      gap: spacing.xs,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderTopWidth: 1,
      borderTopColor: colors.line2,
      backgroundColor: colors.panel,
    },
    hidden: {
      display: "none",
    },
    /** 授权卡（原型 `.perm`）：琥珀色的一整块，三个应答按钮排在 `opts` 行里。 */
    permission: {
      gap: spacing.md,
      paddingVertical: spacing.lg,
      paddingHorizontal: spacing.lg,
      borderWidth: 1,
      borderColor: colors.warn,
      borderLeftWidth: 3,
      borderLeftColor: colors.warn,
      borderRadius: radius.md,
      backgroundColor: colors.warnSoft,
    },
    permissionTitle: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
      flexShrink: 1,
      minWidth: 0,
    },
    permissionInactive: {
      borderColor: colors.line,
      borderLeftColor: colors.ink3,
      backgroundColor: colors.panel2,
    },
    permissionStatus: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: "700",
    },
    permissionStatusInactive: {
      color: colors.ink3,
    },
    /** 请求自身的说明：等宽块（原型 `.pdesc`），内容按不可信文本处理。 */
    permissionDescription: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    question: {
      borderLeftWidth: 3,
      borderLeftColor: colors.accent,
      backgroundColor: colors.accentSoft,
    },
    cancelled: {
      padding: spacing.sm,
      borderLeftWidth: 3,
      borderLeftColor: colors.ink3,
      backgroundColor: colors.panel2,
      borderRadius: radius.sm,
    },
    permissionOptions: {
      flexDirection: "row",
      flexWrap: "wrap",
      gap: spacing.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/**
 * 条目时刻的展示形式：只做**位置固定**的字符串截取（`HH:MM`）。
 *
 * 与 `SessionRow` 同一条纪律：`renderToStaticMarkup` 没有时钟，同一份模型必须渲染出同一段文本。
 * 原始时刻经 `data-timeline-entry-at` 原样进 DOM，需要精确值时从属性取。
 */
function formatEntryTime(createdAt: string): string {
  const timestamp = Date.parse(createdAt);
  if (!Number.isFinite(timestamp)) return "—";
  return new Intl.DateTimeFormat("zh-CN", {
    timeZone: "Asia/Shanghai", hour: "2-digit", minute: "2-digit", hourCycle: "h23",
  }).format(timestamp);
}

/** 消息条目的作者标签。 */
const CHANNEL_LABELS: Readonly<Record<EntryOf<"message">["channel"], string>> = {
  user: "你",
  agent: "Agent",
  thought: "Agent · 思考",
};

/** 本地提交阶段的固定说明（`SentPromptPhase` → 一句实情）。 */
const LOCAL_PROMPT_LABELS: Readonly<Record<SentPromptPhase, string>> = {
  uploading: "附件上传中…",
  sealing: "附件封存中…",
  submitted: "已提交，等待服务端确认",
  accepted: "已受理",
  uncertain: "结果不确定",
  failed: "未发送",
};

/** 阶段自带的补充说明；失败/取消用本地登记的 `notice`（真实原因）。 */
function localPromptDetail(local: LocalPromptState): string | null {
  if (local.notice !== null) return local.notice;
  switch (local.phase) {
    case "uploading":
    case "sealing":
      return "Agent 尚未收到任何内容";
    case "uncertain":
      return "可能已经保存；可以核实，不会自动重发";
    default:
      return null;
  }
}

/** 工具调用「仍在进行中」的状态取值（ACP `ToolCallStatus` 的两个非终态）。 */
const TOOL_CALL_IN_PROGRESS: readonly string[] = ["pending", "in_progress"];

/**
 * 条目的「仍在进行中」判定——**折叠块默认展开值的唯一依据**（D10）。
 *
 * 它只读条目自己的状态字段，不看它在列表里的位置，因此「默认值由状态决定而不由呈现顺序决定」
 * 是可断言的性质（同一份条目换个次序结论不变）。
 *
 * - 思考消息：易失且增量流尚未标记结束（`volatile && !streamComplete`）＝仍在输出；
 * - 工具调用：`pending` / `in_progress`；
 * - 计划：有 `in_progress` 的一步；
 * - 其余（终端输出块、已落地的思考、完成的计划）：不是「进行中」——终端块的进行中由轮次条上的
 *   运行指示表达，它自己只有「这一块已经落地」这一个事实。
 */
function entryInProgress(entry: TimelineEntry): boolean {
  switch (entry.kind) {
    case "message":
      return entry.channel === "thought" && entry.volatile && !entry.streamComplete;
    case "tool_call":
      return TOOL_CALL_IN_PROGRESS.includes(entry.state);
    case "plan":
      return entry.entries.some((item) => item.status === "in_progress");
    default:
      return false;
  }
}

/** 可折叠的结构化条目：思考、工具调用、终端输出、计划（spec 列举的那四类）。 */
type FoldClass = "think" | "tool" | "plan" | "terminal";

function foldableOf(entry: TimelineEntry): FoldClass | null {
  if (entry.kind === "message") return entry.channel === "thought" ? "think" : null;
  if (entry.kind === "tool_call") return "tool";
  if (entry.kind === "terminal_output") return "tool";
  if (entry.kind === "plan") return "plan";
  return null;
}

/** 折叠块的展开判定：显式状态在场就用它，否则用状态推出的默认值（D10）。 */
function isExpanded(
  entry: TimelineEntry,
  expandedEntries: Readonly<Record<string, boolean>> | undefined,
): boolean {
  const explicit = expandedEntries?.[entry.key];
  if (explicit !== undefined) return explicit;
  return foldableOf(entry) !== null && (entryInProgress(entry) ||
    (entry.kind === "tool_call" && entry.state === "failed"));
}

/** 轮次/工具调用状态的中性呈现：状态取值原样显示，不翻译成可能出错的措辞。 */
function StateChip({ state, style }: { readonly state: string; readonly style: object }): ReactElement {
  return (
    <Text style={[STYLES.light.chip, style]} dataSet={{ timelineState: state }}>
      {state}
    </Text>
  );
}

/**
 * 一个折叠块：表头（可点则点）+ 正文（收起时 `display: none`，仍在 DOM 里）。
 *
 * `onToggle` 缺省时表头不是按钮、也没有折叠箭头——「点了没反应」的可点控件在这里不存在。
 */
function Fold({
  entryKey,
  foldClass,
  label,
  title,
  titleMono,
  state,
  inProgress,
  expanded,
  onToggle,
  titleDataSet,
  children,
}: {
  readonly entryKey: string;
  readonly foldClass: FoldClass;
  readonly label: string;
  readonly title: string;
  readonly titleMono: boolean;
  readonly state: string | null;
  readonly inProgress: boolean;
  readonly expanded: boolean;
  readonly onToggle: ((key: string, expanded: boolean) => void) | undefined;
  readonly time: string;
  readonly titleDataSet: Readonly<Record<string, string>>;
  readonly children: ReactNode;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  // 思考一族用 think 色、工具一族（含终端输出）与计划用 tool 色：两档语义色与上面那四个 class 一一对应。
  const boxTint = foldClass === "think" ? styles.foldThink : foldClass === "terminal" ? styles.foldTerminal : styles.foldTool;
  const kindTint = foldClass === "think" ? styles.foldKindThink : styles.foldKindTool;

  const head = (
    <>
      {onToggle === undefined ? null : <Text style={styles.foldCaret}>{expanded ? "▾" : "▸"}</Text>}
      <Text style={[styles.foldKind, kindTint]}>{label}</Text>
      <Text
        style={titleMono ? [styles.foldTitle, styles.mono] : styles.foldTitle}
        dataSet={titleDataSet}
      >
        {title}
      </Text>
      {state === null ? null : (
        <Text
          style={[styles.foldState, inProgress ? styles.foldStateRun : null,
            state === "failed" ? styles.foldStateFailed : null]}
          dataSet={{ entryState: state }}
        >
          {({ pending: "待执行", in_progress: "进行中", running: "进行中", completed: "已完成", failed: "失败", cancelled: "已取消" } as Readonly<Record<string, string>>)[state] ?? state}
        </Text>
      )}

    </>
  );

  return (
    <View
      style={[styles.fold, boxTint]}
      dataSet={{ foldEntry: entryKey, entryExpanded: String(expanded) }}
    >
      {onToggle === undefined ? (
        <View style={styles.foldHead}>{head}</View>
      ) : (
        <Pressable
          role="button"
          accessibilityState={{ expanded }}
          aria-expanded={expanded}
          accessibilityLabel={`${expanded ? "收起" : "展开"}${label}${title ? `：${title}` : ""}`}
          style={styles.foldHead}
          dataSet={{ action: "toggle-timeline-entry", entryKey, entryExpanded: String(expanded) }}
          onPress={() => {
            onToggle(entryKey, !expanded);
          }}
        >
          {head}
        </Pressable>
      )}
      <View
        style={expanded ? styles.foldBody : [styles.foldBody, styles.hidden]}
        dataSet={{ foldBody: expanded ? "expanded" : "collapsed" }}
      >
        {children}
      </View>
    </View>
  );
}

/** 一条消息条目。 */
function MessageEntry({
  entry,
  expanded,
  onToggle,
  onCopyCode,
  onLinkPress,
  media,
  onLoadMedia,
  onReleaseMedia,
  onDownloadAttachment,
  onFilePathPress,
  sessionId,
  agentName,
  showSender,
}: {
  readonly entry: EntryOf<"message">;
  readonly showSender: boolean;
  readonly expanded: boolean;
  readonly onToggle: ((key: string, expanded: boolean) => void) | undefined;
  readonly onCopyCode: ((code: string, language: string) => void) | undefined;
  readonly onLinkPress: ((href: string) => void) | undefined;
  readonly media: ReadonlyMap<string, AttachmentMediaView>;
  readonly onLoadMedia: ConversationTimelineProps["onLoadMedia"];
  readonly onReleaseMedia: ConversationTimelineProps["onReleaseMedia"];
  readonly onDownloadAttachment: ConversationTimelineProps["onDownloadAttachment"];
  readonly onFilePathPress: ConversationTimelineProps["onFilePathPress"];
  readonly sessionId: string;
  readonly agentName?: string | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const message = entry.message;
  const local = entry.local ?? null;
  const localDetail = local === null ? null : localPromptDetail(local);

  const body = (
    <>
      {/* 正文按不可信输入渲染：`Markdown` 不渲染原始 HTML、不加载远端资源。 */}
      <View
        style={entry.channel === "user" ? [styles.body, styles.bodyUser] : [styles.body, styles.bodyAgent]}
        dataSet={{ messageText: "true" }}
      >
        <Markdown
          value={message.text}
          onLinkPress={onLinkPress}
          onCopyCode={onCopyCode}
          onFilePathPress={onFilePathPress}
        />
      </View>
      {/* 类型化媒体：图片预览 / 音频播放 / 文件下载；不可用状态各自说明。 */}
      <MessageMedia
        media={message.content.media}
        sources={media}
        sessionId={sessionId}
        onLoad={onLoadMedia}
        onRelease={onReleaseMedia}
        onDownload={onDownloadAttachment}
      />
      {entry.partial === null || entry.partial.label === null ? null : (
        <Text
          role="alert"
          style={styles.note}
          dataSet={{ messagePartial: entry.partial.state, messagePartialReason: entry.partial.reason ?? "none" }}
        >
          {entry.partial.label}
        </Text>
      )}
      {/* 本地提交（还没有权威正文接替）：如实说明上传/待确认/未发送，Agent 未收到时不假装收到。 */}
      {local === null || local.phase === "accepted" || local.phase === "submitted" ? null : (
        <Text
          style={[
            styles.localState,
            local.phase === "failed" || local.phase === "uncertain" ? styles.localStateWarn : null,
          ]}
          dataSet={{ localPromptPhase: local.phase, localPromptDetail: local.notice ?? "none" }}
        >
          {localDetail === null
            ? LOCAL_PROMPT_LABELS[local.phase]
            : `${LOCAL_PROMPT_LABELS[local.phase]} · ${localDetail}`}
        </Text>
      )}
    </>
  );

  return (
    <View
      role="listitem"
      style={[styles.entry, entry.channel === "user" ? styles.messageUser : styles.message]}
      dataSet={{
        timelineEntry: "message",
        messageId: message.id,
        messageRole: message.role,
        messageChannel: entry.channel,
        messageVolatile: String(entry.volatile),
        messageStreamComplete: String(entry.streamComplete),
        messageLocal: local === null ? "false" : String(local.phase),
        timelineEntryAt: entry.createdAt,
      }}
    >
      {!showSender ? null : (
        <Text
          style={entry.channel === "user" ? [styles.who, styles.whoUser] : styles.who}
          dataSet={{ messageWho: entry.channel }}
        >
          {entry.channel !== "user" && agentName !== undefined && agentName.length > 0
            ? agentName
            : CHANNEL_LABELS[entry.channel === "thought" ? "agent" : entry.channel]}
        </Text>
      )}
      {/* 思考通道是一条可折叠的结构化条目（spec 列举的四类之一）；用户与 Agent 气泡不需要折叠。 */}
      {entry.channel === "thought" ? (
        <Fold
          entryKey={entry.key}
          foldClass="think"
          label="思考"
          title=""
          titleMono={false}
          state={entry.partial !== null && entry.partial.state !== "completed" ? "已中断" : entry.volatile && !entry.streamComplete ? "进行中" : null}
          inProgress={entryInProgress(entry)}
          expanded={expanded}
          onToggle={onToggle}
          time={formatEntryTime(entry.createdAt)}
          titleDataSet={{ messageThoughtTitle: "true" }}
        >
          {body}
        </Fold>
      ) : (
        body
      )}

    </View>
  );
}

/** 轮次：只在失败/取消时留下可见条；进行中由实际思考/工具/回复条目表达，不另写文案。 */
function TurnEntry({ entry }: { readonly entry: EntryOf<"turn"> }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const error = entry.error;
  const cancelled = entry.state === "cancelled" && error === null;
  const anchor = {
    timelineCard: "turn",
    turnId: entry.turnId,
    turnState: entry.state,
    timelineEntryAt: entry.createdAt,
  };

  if (error === null && !cancelled) {
    // 进行中/完成态都不占视觉行：消息、思考、工具与回复本身就是输出证据。
    return <View role="listitem" style={styles.hidden} dataSet={anchor} />;
  }

  return (
    <View role="listitem" style={styles.entry} dataSet={anchor}>
      {error === null ? null : (
        <View style={styles.failureStrip} dataSet={{ turnFailure: "true" }}>
          <Text style={styles.failureText} dataSet={{ turnError: error.code, turnErrorRetryable: String(error.retryable) }}>
            {`轮次失败：${error.message}`}
          </Text>
        </View>
      )}
      {cancelled ? (
        <Text style={[styles.note, styles.cancelled]} dataSet={{ turnCancelled: "true" }}>
          本轮已取消
        </Text>
      ) : null}

    </View>
  );
}

/** 工具详情三部分的标签（顺序与 `ToolCallDetail.truncatedParts` 的固定顺序一致）。 */
const TOOL_DETAIL_LABELS: Readonly<Record<"input" | "output" | "text", string>> = {
  input: "参数",
  output: "输出",
  text: "文本内容",
};

/**
 * 工具详情的一个值 → 稳定文本。
 *
 * JSON 值按 `JSON.stringify(value, null, 2)` 逐字呈现（客户端不解析、不改写内容）；`undefined`、
 * 函数等不可序列化的值（只可能来自非 JSON 的测试夹具）按 `String` 兜底，不抛异常。
 */
function toolDetailValueText(value: unknown): string {
  try {
    const text = JSON.stringify(value, null, 2);
    return text === undefined ? String(value) : text;
  } catch {
    // 循环引用等序列化失败：按 `String` 呈现，仍然不抛异常、不伪造结构。
    return String(value);
  }
}

/** 一个工具详情部分（参数 / 输出 / 文本内容）；该部分缺席时不渲染任何区块。 */
function ToolDetailSection({
  part,
  valueText,
}: {
  readonly part: "input" | "output" | "text";
  readonly valueText: string;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View style={styles.detailSection} dataSet={{ toolDetailPart: part }}>
      <Text style={styles.detailLabel}>{TOOL_DETAIL_LABELS[part]}</Text>
      <Text style={styles.detailValue} dataSet={{ toolDetailValue: part }}>
        {valueText}
      </Text>
    </View>
  );
}

/** 工具调用卡片（可折叠：进行中的默认展开；详情只来自 `entry.detail`，不读 `acp.rawJson`）。 */
function ToolCallEntry({
  diagnosticsMode,
  entry,
  expanded,
  onToggle,
  onOpenDiagnostics,
}: {
  readonly entry: EntryOf<"tool_call">;
  readonly diagnosticsMode: boolean;
  readonly expanded: boolean;
  readonly onToggle: ((key: string, expanded: boolean) => void) | undefined;
  readonly onOpenDiagnostics: (eventId: string) => void;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const inProgress = entryInProgress(entry);
  const detail = entry.detail;

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{
        timelineCard: "tool-call",
        toolCallId: entry.toolCallId,
        toolState: entry.state,
        timelineEntryAt: entry.createdAt,
      }}
    >
      <Fold
        entryKey={entry.key}
        foldClass="tool"
        label="工具调用"
        title=""
        titleMono={false}
        state={entry.state === "completed" ? null : entry.state}
        inProgress={inProgress}
        expanded={expanded}
        onToggle={onToggle}
        time={formatEntryTime(entry.createdAt)}
        titleDataSet={{ toolTitle: entry.title }}
      >
        <Text style={styles.title} dataSet={{ toolDetailTitle: entry.title }}>{entry.title}</Text>
        {detail === null ? null : (
          <View style={styles.detail} dataSet={{ toolDetail: "present" }}>
            {detail.input === null ? null : (
              <ToolDetailSection part="input" valueText={toolDetailValueText(detail.input.value)} />
            )}
            {detail.output === null ? null : (
              <ToolDetailSection part="output" valueText={toolDetailValueText(detail.output.value)} />
            )}
            {detail.text === null ? null : (
              <ToolDetailSection part="text" valueText={detail.text.join("\n\n")} />
            )}
            {detail.truncatedParts.map((part) => (
              <Text
                key={`truncated:${part}`}
                style={styles.chipWarn}
                dataSet={{ toolDetailTruncated: part }}
              >
                {`${TOOL_DETAIL_LABELS[part]}因超过上限未下发`}
              </Text>
            ))}
          </View>
        )}
        {/* 原文仅在诊断模式提供，不参与正常工具详情展示。 */}
        {diagnosticsMode ? <Pressable
          role="button"
          style={styles.diagnosticsEntry}
          dataSet={{ action: "open-tool-call-diagnostics", eventId: entry.eventId }}
          onPress={() => {
            onOpenDiagnostics(entry.eventId);
          }}
        >
          <Text style={styles.diagnosticsEntryText}>查看 ACP 原文</Text>
        </Pressable> : null}
      </Fold>
    </View>
  );
}

/** 终端输出卡片（可折叠；正文走 `CodeBlock`：等宽 + 语言条 + 复制）。 */
function TerminalOutputEntry({
  entry,
  expanded,
  onToggle,
}: {
  readonly entry: EntryOf<"terminal_output">;
  readonly expanded: boolean;
  readonly onToggle: ((key: string, expanded: boolean) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{
        timelineCard: "terminal-output",
        terminalId: entry.terminalId,
        terminalChunk: entry.chunkIndex,
        terminalStream: entry.stream,
        terminalTruncated: String(entry.truncated),
        timelineEntryAt: entry.createdAt,
      }}
    >
      <Fold
        entryKey={entry.key}
        foldClass="terminal"
        label={entry.stream === "stderr" ? "标准错误输出" : "终端输出"}
        title=""
        titleMono={false}
        state={null}
        inProgress={false}
        expanded={expanded}
        onToggle={onToggle}
        time={formatEntryTime(entry.createdAt)}
        titleDataSet={{ terminalIdLabel: entry.terminalId }}
      >
        <Text style={styles.dimNote}>{`终端 ${entry.terminalId} · 输出块 ${entry.chunkIndex}`}</Text>
        {entry.truncated ? (
          <Text style={styles.chipWarn} dataSet={{ terminalTruncatedNote: "true" }}>
            已截断
          </Text>
        ) : null}
        <CodeBlock code={entry.text} language={entry.stream} />
      </Fold>
    </View>
  );
}

/** 计划卡片（可折叠：有进行中的一步时默认展开）。 */
function PlanEntryView({
  entry,
  expanded,
  onToggle,
}: {
  readonly entry: EntryOf<"plan">;
  readonly expanded: boolean;
  readonly onToggle: ((key: string, expanded: boolean) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{ timelineCard: "plan", planEntryCount: entry.entries.length, timelineEntryAt: entry.createdAt }}
    >
      <Fold
        entryKey={entry.key}
        foldClass="plan"
        label="计划"
        title={`${entry.entries.filter((item) => item.status === "completed").length}/${entry.entries.length} 项已完成`}
        titleMono={false}
        state={null}
        inProgress={entryInProgress(entry)}
        expanded={expanded}
        onToggle={onToggle}
        time={formatEntryTime(entry.createdAt)}
        titleDataSet={{ planEntryCountLabel: String(entry.entries.length) }}
      >
        {entry.entries.map((item, index) => (
          <View
            // 计划项没有标识字段（协议里没有 `entryId`），因此按声明次序做键。
            key={`plan:${String(index)}`}
            style={styles.planRow}
            dataSet={{ planEntryStatus: item.status, planEntryPriority: item.priority }}
          >
            <Text style={[styles.chip, item.status === "completed" ? styles.chipOk : item.status === "in_progress" ? styles.chipAccent : null]}>{TODO_STATUS_LABELS[item.status]}</Text>
            <Text style={styles.meta} dataSet={{ planEntryContent: "true" }}>
              {item.content}
            </Text>
          </View>
        ))}
      </Fold>
    </View>
  );
}

/**
 * 文件改动卡片：结构化统计、相对化路径（D8）与结构化差异区（D6）。
 *
 * 差异行只来自服务端下发的结构化来源，三种形态互不冒充；没有解析 `acp.rawJson` 的通路。
 * 完整原文仍是**诊断入口**：展开后以单块不可信文本呈现，不作为差异渲染源。文件栏已移除
 * 会话变更来源，因此这条卡片不再联动右栏。
 */
function FileChangedEntry({
  entry,
  diagnostics,
  diagnosticsOpen,
  onOpenChangeDiagnostics,
}: {
  readonly entry: EntryOf<"file_changed">;
  readonly diagnostics: FileChangeDiagnostics | undefined;
  readonly diagnosticsOpen: boolean;
  readonly onOpenChangeDiagnostics: ((eventId: string | null) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{
        timelineCard: "file-changed",
        changeId: entry.changeId,
        changeKind: entry.changeKind,
        changePath: entry.displayPath,
        timelineEntryAt: entry.createdAt,
      }}
    >
      <View style={[styles.card, styles.fileChange]}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>文件改动</Text>
          <Text style={styles.title} dataSet={{ changePathLabel: entry.displayPath }}>
            {entry.displayPath}
          </Text>
          <Text style={styles.chip}>{entry.changeKind}</Text>
          {entry.outsideWorkspace ? (
            <Text style={styles.chipWarn} dataSet={{ changeOutsideWorkspace: "true" }}>
              工作区之外
            </Text>
          ) : null}

        </View>
        <Text style={styles.meta} dataSet={{ changeSummary: "true" }}>
          {entry.summary}
        </Text>
        {/* 行数统计只呈现有依据的那一侧：「判定不出」时没有对应属性，而不是 `0`。 */}
        <View style={styles.row}>
          {entry.addedLines === null ? null : (
            <Text style={styles.chipOk} dataSet={{ changeAddedLines: entry.addedLines }}>
              {`+${entry.addedLines}`}
            </Text>
          )}
          {entry.deletedLines === null ? null : (
            <Text style={styles.chipDanger} dataSet={{ changeDeletedLines: entry.deletedLines }}>
              {`-${entry.deletedLines}`}
            </Text>
          )}
          {entry.addedLines === null && entry.deletedLines === null ? (
            <Text style={styles.note} dataSet={{ changeLineCountUnavailable: "true" }}>
              这次改动没有下发行数统计。
            </Text>
          ) : null}
        </View>
        <ChangeDiff diff={entry.diff} />
        <Text style={styles.dimNote} dataSet={{ changeStatisticsScope: "true" }}>
          统计只覆盖 Agent 在工具调用里声明的改动；差异行只来自服务端下发的结构化来源，本卡片不解析 ACP 原文。
        </Text>
        {diagnostics === undefined || onOpenChangeDiagnostics === undefined ? null : (
          <Pressable
            role="button"
            style={styles.diagnosticsEntry}
            dataSet={{
              action: "open-file-change-diagnostics",
              eventId: diagnostics.eventId,
              diagnosticsOpen: String(diagnosticsOpen),
            }}
            onPress={() => {
              onOpenChangeDiagnostics(diagnosticsOpen ? null : diagnostics.eventId);
            }}
          >
            <Text style={styles.diagnosticsEntryText}>
              {diagnosticsOpen ? "收起 ACP 原文（诊断）" : "查看 ACP 原文（诊断）"}
            </Text>
          </Pressable>
        )}
        {diagnosticsOpen && diagnostics !== undefined ? <ChangeDiagnosticsPanel diagnostics={diagnostics} /> : null}
        {diagnosticsOpen && diagnostics === undefined ? (
          <Text style={styles.note} dataSet={{ changeDiagnostics: entry.changeId }}>
            这条改动没有可用的原文诊断数据（客户端未保留它的逐条投影）。
          </Text>
        ) : null}
      </View>
    </View>
  );
}

/** 模式变更卡片。 */
function ModeChangedEntry({ entry }: { readonly entry: EntryOf<"mode_changed"> }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{
        timelineCard: "mode-changed",
        modeVersion: entry.version,
        timelineEntryAt: entry.createdAt,
        ...(entry.currentModeId === null ? {} : { currentModeId: entry.currentModeId }),
      }}
    >
      <View style={styles.card}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>模式变更</Text>
          <Text style={styles.title} dataSet={{ currentModeIdLabel: entry.currentModeId ?? "unknown" }}>
            {entry.currentModeId ?? "未提供当前模式"}
          </Text>
          <StateChip state={`v${entry.version}`} style={styles.chip} />

        </View>
      </View>
    </View>
  );
}

/** 配置变更卡片（模型等配置项）。 */
function ConfigChangedEntry({ entry }: { readonly entry: EntryOf<"config_changed"> }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{
        timelineCard: "config-changed",
        configVersion: entry.version,
        configOptionCount: entry.configOptions.length,
        timelineEntryAt: entry.createdAt,
      }}
    >
      <View style={styles.card}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>配置变更</Text>
          <StateChip state={`v${entry.version}`} style={styles.chip} />

        </View>
        {entry.configOptions.map((option) => (
          <View
            key={option.id}
            style={styles.row}
            dataSet={{ configOption: option.id, configOptionCurrentValue: String(option.currentValue) }}
          >
            <Text style={styles.meta} dataSet={{ configOptionName: option.name }}>
              {option.name}
            </Text>
            <Text style={styles.chip}>{String(option.currentValue)}</Text>
          </View>
        ))}
      </View>
    </View>
  );
}

/**
 * 「重新取得当前有效请求」入口。
 *
 * 只在**版本/有效性未知**的不可应答态出现：仅历史读到、刷新后服务端不再确认、以及协商了
 * `core.pending-interactions.v1` 但实时请求没带 `requestVersion`。这三种情况下卡片本身没有
 * 可提交的内容，刷新是唯一能让它变得可应答或有可见结论的动作。
 */
export function RefreshInteractionsAction({
  availability,
  onRefreshInteractions,
}: {
  readonly availability: InteractionAvailability;
  readonly onRefreshInteractions: (() => void) | undefined;
}): ReactElement | null {
  // hook 必须在所有提前返回之前：这个组件在卡片里是**无条件渲染**的，卡片从不可应答态
  // 变为可应答态（刷新成功）时 React 会复用同一 fiber——若这次渲染跳过 `useTheme()`，
  // hook 数量就会变化并抛错。`onRefreshInteractions` 从缺省变为接线时同理。
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  if (onRefreshInteractions === undefined) return null;
  if (availability.kind !== "not_answerable") return null;
  const reason = availability.reason;
  if (reason !== "read_from_history" && reason !== "not_effective" && reason !== "request_version_unknown") {
    return null;
  }
  return (
    <Pressable
      role="button"
      style={styles.foldHead}
      dataSet={{ action: "refresh-interactions", refreshInteractionsReason: reason }}
      onPress={onRefreshInteractions}
    >
      <Text style={styles.note}>重新取得当前有效请求</Text>
    </Pressable>
  );
}

/**
 * 授权请求卡片（原型 `.perm` 的形态：标题 + 说明块 + 三个应答按钮）。
 *
 * 三种可区分呈现来自 `availability` 这一个判别值：本设备亲历（`answerable`，有按钮）／仅历史读到
 * （`not_answerable read_from_history`，没有按钮 + 说明）／已被其它设备处理（`handled_elsewhere`，
 * 没有按钮 + 处理设备标识）——判定本身在 `domain/interaction.ts`，本卡片只照着画。
 *
 * 协商 `core.pending-interactions.v1` 后版本未知（实时事件没带 `requestVersion`）时同样是
 * `not_answerable`，并给出刷新入口：没有版本就发出的应答必然被服务端拒绝。
 */
function interactionStatusLabel(availability: InteractionAvailability, subject: "授权" | "提问"): string {
  switch (availability.kind) {
    case "answerable": return subject === "授权" ? "等待授权" : "等待你回答";
    case "submitting": return `${subject}应答提交中`;
    case "handled_here":
    case "handled_elsewhere": return `${subject}已处理`;
    case "not_answerable":
      switch (availability.reason) {
        case "read_from_history": return `历史${subject} · 待核实`;
        case "request_version_unknown": return `${subject}请求 · 待核实`;
        case "expired": return `${subject}已过期`;
        case "turn_ended": return `${subject}已失效 · 本轮已结束`;
        case "not_effective": return `${subject}当前无效`;
        case "options_missing": return `${subject}暂不可操作`;
      }
  }
}

function PermissionRequestedEntry({
  entry,
  onAnswerInteraction,
  onRefreshInteractions,
}: {
  readonly entry: EntryOf<"permission_requested">;
  readonly onAnswerInteraction: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
  readonly onRefreshInteractions: (() => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const availability = entry.availability;
  const actionable = availability.kind === "answerable";
  const handled = availability.kind === "handled_here" || availability.kind === "handled_elsewhere";
  const statusLabel = handled
    ? `${permissionResolutionLabel(availability.resolution)} · ${availability.kind === "handled_here" ? "本设备" : "其他设备"}`
    : interactionStatusLabel(availability, "授权");
  const detail = entry.detail;
  const input = detail?.input?.value;
  const fields = typeof input === "object" && input !== null && !Array.isArray(input)
    ? input as Readonly<Record<string, unknown>> : null;
  const inputText = (key: string): string | null => {
    const value = fields?.[key];
    return typeof value === "string" && value.trim().length > 0 ? value : null;
  };
  const command = inputText("command");

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{
        timelineCard: "permission-requested",
        interactionId: entry.interactionId,
        interactionKind: "permission",
        interactionAvailability: entry.availability.kind,
        timelineEntryAt: entry.createdAt,
      }}
    >
      <View style={[styles.permission, actionable ? null : styles.permissionInactive]}
        dataSet={{ permissionTone: actionable ? "action-required" : "neutral" }}>
        <View style={styles.cardHead}>
          <Text style={[styles.permissionStatus, actionable ? null : styles.permissionStatusInactive]}
            dataSet={{ permissionStatus: statusLabel,
              ...(handled ? { interactionHandled: availability.kind === "handled_here" ? "this-device" : "other-device",
                resolvedByDeviceId: availability.kind === "handled_elsewhere" ? availability.resolvedByDeviceId ?? "unknown" : "unknown" } : {}),
            }}>{statusLabel}</Text>

        </View>
        <View style={styles.detailSection} dataSet={{ approvalContext: "true" }}>
          <Text selectable style={styles.detailValue} dataSet={{ interactionTitle: entry.title }}>{command ?? entry.title}</Text>
        </View>
        {/* 应答按钮排在 opts 行里：可选项只来自这次请求自身的下发内容（逐字呈现，不翻译）。 */}
        <View style={styles.permissionOptions}>
          <InteractionAnswerButtons
            interactionId={entry.interactionId}
            availability={entry.availability}
            onAnswerInteraction={onAnswerInteraction}
          />
        </View>
        {handled ? null : <InteractionAvailabilityNote availability={entry.availability} />}
        <RefreshInteractionsAction
          availability={entry.availability}
          onRefreshInteractions={onRefreshInteractions}
        />
      </View>
    </View>
  );
}

/** 提问请求卡片：按当前有效请求的 schema 渲染可填写控件；不可应答时只说明原因。 */
function ElicitationRequestedEntry({
  entry,
  onAnswerInteraction,
  onSubmitElicitation,
  onRefreshInteractions,
}: {
  readonly entry: EntryOf<"elicitation_requested">;
  readonly onAnswerInteraction: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
  readonly onSubmitElicitation:
    | ((interactionId: string, values: Readonly<Record<string, FormInputValue>>) => void)
    | undefined;
  readonly onRefreshInteractions: (() => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const form = buildElicitationForm(entry.schema);
  const answerable = entry.availability.kind === "answerable";
  const formSignature =
    form.kind === "form"
      ? `${form.fields.map((field) => field.name).join(",")}|${JSON.stringify(form.initial)}`
      : `unavailable:${form.reason}`;
  const decline = (): void => {
    onAnswerInteraction?.(entry.interactionId, { via: "action", action: "decline", label: "拒绝" });
  };
  const cancel = (): void => {
    onAnswerInteraction?.(entry.interactionId, { via: "action", action: "cancel", label: "取消" });
  };
  /** 不可应答时只列出字段名与类型：让用户看得出「这里曾要求填什么」，但不给可点的控件。 */
  const fieldSummary =
    form.kind === "form" && form.fields.length > 0
      ? `表单字段：${form.fields
          .map((field) => `${field.label}（${field.kind}${field.required ? "，必填" : ""}）`)
          .join("、")}`
      : null;

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{
        timelineCard: "elicitation-requested",
        interactionId: entry.interactionId,
        interactionKind: "elicitation",
        interactionAvailability: entry.availability.kind,
        timelineEntryAt: entry.createdAt,
      }}
    >
      <View style={[styles.card, answerable ? styles.question : styles.permissionInactive]}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>{interactionStatusLabel(entry.availability, "提问")}</Text>
          <Text style={styles.title} dataSet={{ interactionTitle: entry.title }}>
            {entry.title}
          </Text>

        </View>
        {answerable ? (
          // 表单自己渲染「提交」（带值），按钮组只保留拒绝/取消：同一动作只有一个入口。
          <ElicitationForm
            key={formSignature}
            form={form}
            {...(onSubmitElicitation === undefined
              ? {}
              : {
                  onSubmit: (values: Readonly<Record<string, FormInputValue>>) => {
                    onSubmitElicitation(entry.interactionId, values);
                  },
                })}
            {...(onAnswerInteraction === undefined ? {} : { onDecline: decline, onCancel: cancel })}
          />
        ) : (
          <>
            {fieldSummary === null ? null : (
              <Text style={styles.kicker} dataSet={{ elicitationFieldSummary: "true" }}>
                {fieldSummary}
              </Text>
            )}
          </>
        )}
        <InteractionAnswerButtons
          interactionId={entry.interactionId}
          availability={entry.availability}
          onAnswerInteraction={onAnswerInteraction}
          omitActions={answerable ? ["submit", "decline", "cancel"] : []}
        />
        <InteractionAvailabilityNote availability={entry.availability} />
        <RefreshInteractionsAction
          availability={entry.availability}
          onRefreshInteractions={onRefreshInteractions}
        />
      </View>
    </View>
  );
}

function permissionResolutionLabel(resolution: string): string {
  return ({ allow_once: "已允许一次", allow_always: "已始终允许", reject_once: "已拒绝一次", reject_always: "已始终拒绝" } as Readonly<Record<string, string>>)[resolution] ?? resolution;
}

/** 历史窗口缺少原请求时保留独立结果卡片。 */
function PermissionResolvedEntry({ entry }: { readonly entry: EntryOf<"permission_resolved"> }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{
        timelineCard: "permission-resolved",
        interactionId: entry.interactionId,
        interactionResolution: entry.resolution,
        resolvedByDeviceId: entry.resolvedByDeviceId ?? "unknown",
        timelineEntryAt: entry.createdAt,
      }}
    >
      <View style={styles.card}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>授权已处理</Text>
          <Text style={styles.title} dataSet={{ interactionResolutionLabel: entry.resolution }}>
            {permissionResolutionLabel(entry.resolution)}
          </Text>

        </View>
      </View>
    </View>
  );
}

/** 提问已处理卡片。 */
function ElicitationResolvedEntry({ entry }: { readonly entry: EntryOf<"elicitation_resolved"> }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{
        timelineCard: "elicitation-resolved",
        interactionId: entry.interactionId,
        interactionResolution: entry.action,
        resolvedByDeviceId: entry.resolvedByDeviceId ?? "unknown",
        timelineEntryAt: entry.createdAt,
      }}
    >
      <View style={styles.card}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>提问已处理</Text>
          <Text style={styles.title} dataSet={{ interactionResolutionLabel: entry.action }}>
            {entry.action}
          </Text>

        </View>
      </View>
    </View>
  );
}

/**
 * 降级条目：覆盖未命中或内容缺失时的呈现（R18 / D5）。
 *
 * 内容由 `DegradedEntryContent` 承担——节点级降级面（`HostConnectionPanel`）用的是同一份，
 * 两个面因此不会在归因上分叉；这里只提供时间线的容器与锚点。
 */
function DegradedEntry({
  diagnosticsMode,
  entry,
  onOpenDiagnostics,
}: {
  readonly entry: EntryOf<"degraded">;
  readonly diagnosticsMode: boolean;
  readonly onOpenDiagnostics: (eventId: string) => void;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const model = entry.degraded;

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{
        timelineCard: "degraded",
        degradedEntry: model.eventId,
        degradedEventType: model.eventType,
        timelineEntryAt: entry.createdAt,
      }}
    >
      <DegradedEntryContent model={model} onOpenDiagnostics={diagnosticsMode ? onOpenDiagnostics : undefined} />
    </View>
  );
}

/** 一条时间线条目的呈现。 */
function TimelineEntryView(props: {
  readonly diagnosticsMode: boolean;
  readonly entry: TimelineEntry;
  readonly showSender: boolean;
  readonly sessionId: string;
  readonly expandedEntries: Readonly<Record<string, boolean>> | undefined;
  /**
   * 折叠块表头被点选：上抛条目键与**切换后的**值。子组件拿到的是当前**生效**值
   * （`isExpanded`：显式覆盖优先、否则由条目状态推出），因此翻转必须在这里发生——页面只存覆盖值，
   * 它看不到「这条默认值是多少」（D10）。
   */
  readonly onToggleEntry: ((key: string, expanded: boolean) => void) | undefined;
  readonly changeDiagnostics: ReadonlyMap<string, FileChangeDiagnostics> | undefined;
  readonly openChangeDiagnosticsEventId: string | null;
  readonly onOpenChangeDiagnostics: ((eventId: string | null) => void) | undefined;
  readonly agentName?: string | undefined;
  readonly onOpenDiagnostics: (eventId: string) => void;
  readonly onAnswerInteraction: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
  readonly onCopyCode: ((code: string, language: string) => void) | undefined;
  readonly onLinkPress: ((href: string) => void) | undefined;
  readonly media: ReadonlyMap<string, AttachmentMediaView>;
  readonly onLoadMedia: ConversationTimelineProps["onLoadMedia"];
  readonly onReleaseMedia: ConversationTimelineProps["onReleaseMedia"];
  readonly onDownloadAttachment: ConversationTimelineProps["onDownloadAttachment"];
  readonly onSubmitElicitation: ConversationTimelineProps["onSubmitElicitation"];
  readonly onRefreshInteractions: ConversationTimelineProps["onRefreshInteractions"];
  readonly onFilePathPress: ConversationTimelineProps["onFilePathPress"];
}): ReactElement {
  const entry = props.entry;
  const expanded = isExpanded(entry, props.expandedEntries);
  switch (entry.kind) {
    case "message":
      return (
        <MessageEntry
          showSender={props.showSender}
          entry={entry}
          agentName={props.agentName}
          expanded={expanded}
          onToggle={props.onToggleEntry}
          onCopyCode={props.onCopyCode}
          onLinkPress={props.onLinkPress}
          media={props.media}
          onLoadMedia={props.onLoadMedia}
          onReleaseMedia={props.onReleaseMedia}
          onDownloadAttachment={props.onDownloadAttachment}
          onFilePathPress={props.onFilePathPress}
          sessionId={props.sessionId}
        />
      );
    case "turn":
      return <TurnEntry entry={entry} />;
    case "tool_call":
      return (
        <ToolCallEntry
          diagnosticsMode={props.diagnosticsMode}
          entry={entry}
          expanded={expanded}
          onToggle={props.onToggleEntry}
          onOpenDiagnostics={props.onOpenDiagnostics}
        />
      );
    case "terminal_output":
      return <TerminalOutputEntry entry={entry} expanded={expanded} onToggle={props.onToggleEntry} />;
    case "plan":
      return <PlanEntryView entry={entry} expanded={expanded} onToggle={props.onToggleEntry} />;
    case "file_changed": {
      const diagnostics = props.changeDiagnostics?.get(entry.changeId);
      return (
        <FileChangedEntry
          entry={entry}
          diagnostics={props.diagnosticsMode ? diagnostics : undefined}
          diagnosticsOpen={
            diagnostics !== undefined && props.openChangeDiagnosticsEventId === diagnostics.eventId
          }
          onOpenChangeDiagnostics={props.onOpenChangeDiagnostics}
        />
      );
    }
    case "mode_changed":
      return <ModeChangedEntry entry={entry} />;
    case "config_changed":
      return <ConfigChangedEntry entry={entry} />;
    case "permission_requested":
      return (
        <PermissionRequestedEntry
          entry={entry}
          onAnswerInteraction={props.onAnswerInteraction}
          onRefreshInteractions={props.onRefreshInteractions}
        />
      );
    case "elicitation_requested":
      return (
        <ElicitationRequestedEntry
          entry={entry}
          onAnswerInteraction={props.onAnswerInteraction}
          onSubmitElicitation={props.onSubmitElicitation}
          onRefreshInteractions={props.onRefreshInteractions}
        />
      );
    case "permission_resolved":
      return <PermissionResolvedEntry entry={entry} />;
    case "elicitation_resolved":
      return <ElicitationResolvedEntry entry={entry} />;
    case "pending_interaction":
      return <PendingInteractionsPanel interactions={[entry.interaction]} onAnswerInteraction={props.onAnswerInteraction} />;
    case "degraded":
      return <DegradedEntry entry={entry} diagnosticsMode={props.diagnosticsMode} onOpenDiagnostics={props.onOpenDiagnostics} />;
  }
}

/**
 * 过程行（时间线末尾的一条紧凑说明）。
 *
 * 只在模型给出了 `label`（提交中/恢复中/等待响应）时渲染；有实际思考/工具/回复/待处理卡片时
 * 模型给 `label: null`，这里什么也不画——过程反馈不重复占行，也不是顶部状态条。
 */
function ProcessRow({ process }: { readonly process: ConversationProcessModel }): ReactElement | null {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  if (process.label === null) return null;
  if (process.phase === "awaiting") return null;
  return (
    <View role="listitem" style={styles.processRow} dataSet={{ processRow: process.phase }}>
      <Text style={styles.processLabel} dataSet={{ processLabel: "true" }}>
        {process.label}
      </Text>
      {process.detail === null ? null : (
        <Text style={styles.processDetail} dataSet={{ processDetail: "true" }}>
          {process.detail}
        </Text>
      )}
    </View>
  );
}

/** 等待回复时显示无背景光标；减少动态效果时保持静态，卸载时停止动画。 */

/**
 * 时间线。
 *
 * 容器同时带 `data-timeline="entries"`（新的一等锚点）与 `data-message-list="messages"`（既有
 * 消费方的锚点）：消息与结构化条目进的是同一份列表，因此两者都在它里面。
 *
 * 条目按**可靠轮次标识**分组（`groupTimelineEntries`）：同一 `turnId` 的连续条目进一个组，
 * 没有标识的条目独立成组。分组只来自标识，不按呈现顺序猜测。每个条目外层带
 * `data-timeline-key`：滚动锚点按它定位（内容在组内原位更新时节点不换）。
 */
export function ConversationTimeline({
  diagnosticsMode = false,
  entries,
  sessionId,
  agentName,
  onOpenDiagnostics,
  onAnswerInteraction,
  onCopyCode,
  onLinkPress,
  expandedEntries,
  onToggleEntry,
  changeDiagnostics,
  openChangeDiagnosticsEventId,
  onOpenChangeDiagnostics,
  media,
  onLoadMedia,
  onReleaseMedia,
  onDownloadAttachment,
  onSubmitElicitation,
  onRefreshInteractions,
  onFilePathPress,
  process,
}: ConversationTimelineProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const sources = media ?? new Map<string, AttachmentMediaView>();
  const groups = groupTimelineEntries(entries);
  const timeStarts = new Set<string>();
  let previous: { side: string } | null = null;
  for (const entry of entries) {
    // Invisible turn markers do not split a visible run.
    if (entry.kind === "turn" && entry.error === null && entry.state !== "cancelled") continue;
    const side = entry.kind === "message" && entry.channel === "user" ? "user" : "agent";
    if (previous === null || previous.side !== side || side === "user") {
      timeStarts.add(entry.key);
    }
    previous = { side };
  }


  return (
    <View
      role="list"
      style={styles.timeline}
      dataSet={{ timeline: "entries", messageList: "messages", timelineEntryCount: entries.length }}
    >
      {entries.some(entry => entry.kind === "message" && entry.sequence === null && entry.local == null) ? (
        <Text role="status" style={styles.kicker} dataSet={{ timelineOrderIncomplete: "true" }}>
          部分旧历史的顺序无法完整还原，已保留原有内容。
        </Text>
      ) : null}
      {groups.map((group) => (
        <View
          key={group.key}
          style={styles.turnGroup}
          role="group"
          dataSet={{
            turnGroup: group.key,
            turnGroupId: group.turnId ?? "unassigned",
            turnGroupState: group.state ?? "unknown",
            turnGroupEntryCount: String(group.entries.length),
          }}
        >
          {group.entries.map((entry) => (
            <View key={entry.key} style={styles.turnGroupEntry} dataSet={{ timelineKey: entry.key }}>
              {timeStarts.has(entry.key) ? (
                <View style={[styles.entryHeader, entry.kind === "message" && entry.channel === "user"
                  ? styles.entryHeaderUser : null]} dataSet={{ timelineHeader: "true" }}>
                  <Text style={styles.who} dataSet={{ messageWho: entry.kind === "message" && entry.channel === "user" ? "user" : "agent" }}>
                    {entry.kind === "message" && entry.channel === "user" ? "你" : agentName || "Agent"}
                  </Text>
                  <Text style={styles.kicker} dataSet={{ timelineTime: entry.createdAt }}>
                    {formatEntryTime(entry.createdAt)}
                  </Text>
                </View>
              ) : null}
              <TimelineEntryView
                diagnosticsMode={diagnosticsMode}
                showSender={false}
                entry={entry}
                sessionId={sessionId}
                agentName={agentName}
                expandedEntries={expandedEntries}
                onToggleEntry={onToggleEntry}
                changeDiagnostics={changeDiagnostics}
                openChangeDiagnosticsEventId={openChangeDiagnosticsEventId ?? null}
                onOpenChangeDiagnostics={onOpenChangeDiagnostics}
                onOpenDiagnostics={onOpenDiagnostics}
                onAnswerInteraction={onAnswerInteraction}
                onCopyCode={onCopyCode}
                onLinkPress={onLinkPress}
                media={sources}
                onLoadMedia={onLoadMedia}
                onReleaseMedia={onReleaseMedia}
                onDownloadAttachment={onDownloadAttachment}
                onSubmitElicitation={onSubmitElicitation}
                onRefreshInteractions={onRefreshInteractions}
                onFilePathPress={onFilePathPress}
              />
            </View>
          ))}
        </View>
      ))}
      {process === null || process === undefined ? null : <ProcessRow process={process} />}
    </View>
  );
}
