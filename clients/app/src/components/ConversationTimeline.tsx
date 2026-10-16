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
 * 渲染（D8）——它的入口是诊断抽屉（降级条目）或右栏（改动）。
 */

import type { ReactElement, ReactNode } from "react";

import type { InteractionAnswer } from "../domain/interaction";
import type { TimelineEntry } from "../features/conversation-timeline";
import { CodeBlock } from "./CodeBlock";
import { DegradedEntryContent } from "./DegradedEntryContent";
import { InteractionAnswerButtons, InteractionAvailabilityNote } from "./InteractionAnswering";
import { Markdown } from "./Markdown";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

/** `kind` 取某一类的时间线条目类型（判别联合的成员）。 */
type EntryOf<K extends TimelineEntry["kind"]> = Extract<TimelineEntry, { readonly kind: K }>;

export interface ConversationTimelineProps {
  /** 时间线条目（唯一渲染列表，已按时间排序、已去重）。 */
  readonly entries: readonly TimelineEntry[];
  /** 打开某条降级条目的诊断抽屉（页面负责把 `eventId` 交给仓库）。 */
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
  /** 已被选中的那一处改动（`file_changed` 条目的 `changeId`）；缺省 `null` = 没有选中。 */
  readonly selectedChangeId?: string | null;
  /**
   * 选中一处改动（上抛 `changeId`）——右栏据此选中同一个文件（spec「从条目联动到右栏」）。
   * 缺省时 `file_changed` 条目不是可点控件（不渲染点了没反应的按钮）。
   */
  readonly onSelectChange?: ((changeId: string) => void) | undefined;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    timeline: {
      gap: spacing.md,
      paddingVertical: spacing.sm,
    },
    entry: {
      gap: spacing.xs,
      paddingBottom: spacing.md,
      borderBottomWidth: 1,
      borderBottomColor: colors.line2,
    },
    message: {
      gap: spacing.xs,
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
      maxWidth: "78%",
      backgroundColor: colors.accentSoft,
      borderColor: colors.accentSoft,
    },
    bodyAgent: {
      maxWidth: "92%",
    },
    card: {
      gap: spacing.sm,
      paddingVertical: spacing.sm,
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
      paddingVertical: 2,
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
    /** 运行指示：轮次仍在进行中时的那一枚胶囊（原型 `.live`）。 */
    live: {
      alignSelf: "flex-start",
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.ok,
      borderRadius: radius.lg,
      backgroundColor: colors.okSoft,
      color: colors.ok,
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
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
    },
    foldCaret: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    foldKind: {
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    foldKindThink: {
      color: colors.think,
    },
    foldKindTool: {
      color: colors.tool,
    },
    foldTitle: {
      flexShrink: 1,
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
    /** 折叠块的正文：收起时用 `display: none` 隐藏（留在 DOM 里）。 */
    foldBody: {
      gap: spacing.xs,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.sm,
      borderTopWidth: 1,
      borderTopColor: colors.line2,
      backgroundColor: colors.panel,
    },
    hidden: {
      display: "none",
    },
    /** 授权卡（原型 `.perm`）：琥珀色的一整块，三个应答按钮排在 `opts` 行里。 */
    permission: {
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
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
    permissionOptions: {
      flexDirection: "row",
      flexWrap: "wrap",
      gap: spacing.sm,
    },
    selectablePath: {
      paddingVertical: 2,
      paddingHorizontal: spacing.xs,
      borderRadius: radius.sm,
      borderWidth: 1,
      borderColor: colors.line,
      backgroundColor: colors.panel,
    },
    selectablePathSelected: {
      borderColor: colors.accent,
      backgroundColor: colors.accentSoft,
    },
    selectablePathText: {
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
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
  return createdAt.length < 16 ? createdAt : createdAt.slice(11, 16);
}

/** 消息条目的作者标签。 */
const CHANNEL_LABELS: Readonly<Record<EntryOf<"message">["channel"], string>> = {
  user: "你",
  agent: "Agent",
  thought: "Agent · 思考",
};

/** 工具调用「仍在进行中」的状态取值（ACP `ToolCallStatus` 的两个非终态）。 */
const TOOL_CALL_IN_PROGRESS: readonly string[] = ["pending", "in_progress"];

/** 轮次「正在输出」的状态取值（`waiting_*` 是在等我们，不算输出中）。 */
const TURN_RUNNING: readonly string[] = ["queued", "running"];

/**
 * 条目的「仍在进行中」判定——**折叠块默认展开值的唯一依据**（D10）。
 *
 * 它只读条目自己的状态字段，不看它在列表里的位置，因此「默认值由状态决定而不由呈现顺序决定」
 * 是可断言的性质（同一份条目换个次序结论不变）。
 *
 * - 思考消息：易失且增量流尚未标记结束（`volatile && !streamComplete`）＝仍在输出；
 * - 工具调用：`pending` / `in_progress`；
 * - 计划：有 `in_progress` 的一步；
 * - 其余（终端输出块、已落地的思考、完成的计划）：不是「进行中」——终端块的进行中由轮次卡片的
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
type FoldClass = "think" | "tool" | "plan";

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
  return foldableOf(entry) !== null && entryInProgress(entry);
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
  time,
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
  const boxTint = foldClass === "think" ? styles.foldThink : styles.foldTool;
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
          style={inProgress ? [styles.foldState, styles.foldStateRun] : styles.foldState}
          dataSet={{ entryState: state }}
        >
          {state}
        </Text>
      )}
      <Text style={styles.time}>{time}</Text>
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
}: {
  readonly entry: EntryOf<"message">;
  readonly expanded: boolean;
  readonly onToggle: ((key: string, expanded: boolean) => void) | undefined;
  readonly onCopyCode: ((code: string, language: string) => void) | undefined;
  readonly onLinkPress: ((href: string) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const message = entry.message;

  const body = (
    <>
      {/* 正文按不可信输入渲染：`Markdown` 不渲染原始 HTML、不加载远端资源。 */}
      <View
        style={entry.channel === "user" ? [styles.body, styles.bodyUser] : [styles.body, styles.bodyAgent]}
        dataSet={{ messageText: "true" }}
      >
        <Markdown value={message.text} onLinkPress={onLinkPress} onCopyCode={onCopyCode} />
      </View>
      {message.hasStructuredContent ? (
        <Text style={styles.note} dataSet={{ hasStructuredContent: "true" }}>
          这条消息还有非文本内容块，逐条说明见时间线上的结构化卡片。
        </Text>
      ) : null}
      {entry.volatile ? (
        <Text style={styles.note} dataSet={{ messageVolatileNote: "true" }}>
          流式增量（尚未持久化）：正文以按会话读取的结果为准。
        </Text>
      ) : null}
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
        timelineEntryAt: entry.createdAt,
      }}
    >
      <Text
        style={entry.channel === "user" ? [styles.who, styles.whoUser] : styles.who}
        dataSet={{ messageWho: entry.channel }}
      >
        {CHANNEL_LABELS[entry.channel]}
      </Text>
      {/* 思考通道是一条可折叠的结构化条目（spec 列举的四类之一）；用户与 Agent 气泡不需要折叠。 */}
      {entry.channel === "thought" ? (
        <Fold
          entryKey={entry.key}
          foldClass="think"
          label="思考"
          title="思考"
          titleMono={false}
          state={entry.volatile ? (entry.streamComplete ? "增量已结束" : "进行中") : "已持久化"}
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
      <Text
        style={entry.channel === "user" ? [styles.time, styles.whoUser] : styles.time}
        dataSet={{ messageCreatedAt: entry.createdAt }}
      >
        {formatEntryTime(entry.createdAt)}
      </Text>
    </View>
  );
}

/** 轮次卡片：失败时是一条失败条，进行中时带运行指示。 */
function TurnEntry({ entry }: { readonly entry: EntryOf<"turn"> }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const error = entry.error;
  const running = TURN_RUNNING.includes(entry.state);

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{ timelineCard: "turn", turnId: entry.turnId, turnState: entry.state, timelineEntryAt: entry.createdAt }}
    >
      <View style={styles.card}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>轮次</Text>
          <Text style={styles.title} dataSet={{ turnIdLabel: entry.turnId }}>
            {entry.turnId}
          </Text>
          <StateChip
            state={entry.state}
            style={entry.state === "failed" ? styles.chipDanger : entry.state === "completed" ? styles.chipOk : styles.chipAccent}
          />
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
        </View>
        {/* 运行指示：只看这条轮次自己的状态，不看它在时间线上的位置。 */}
        {running ? (
          <Text style={styles.live} dataSet={{ turnRunning: "true" }}>
            ● Agent 正在输出…
          </Text>
        ) : null}
        {error === null ? null : (
          <View style={styles.failureStrip} dataSet={{ turnFailure: "true" }}>
            <Text style={styles.failureText} dataSet={{ turnError: error.code, turnErrorRetryable: String(error.retryable) }}>
              {`轮次失败：${error.message}`}
            </Text>
          </View>
        )}
      </View>
    </View>
  );
}

/** 工具调用卡片（可折叠：进行中的默认展开）。 */
function ToolCallEntry({
  entry,
  expanded,
  onToggle,
}: {
  readonly entry: EntryOf<"tool_call">;
  readonly expanded: boolean;
  readonly onToggle: ((key: string, expanded: boolean) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const inProgress = entryInProgress(entry);

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
        title={entry.title}
        titleMono={false}
        state={entry.state}
        inProgress={inProgress}
        expanded={expanded}
        onToggle={onToggle}
        time={formatEntryTime(entry.createdAt)}
        titleDataSet={{ toolTitle: entry.title }}
      >
        {entry.turnId === null ? null : (
          <Text style={styles.meta} dataSet={{ toolTurnId: entry.turnId }}>
            {`所属轮次：${entry.turnId}`}
          </Text>
        )}
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
        foldClass="tool"
        label="终端输出"
        title={`${entry.terminalId} · 第 ${entry.chunkIndex} 块`}
        titleMono={true}
        state={null}
        inProgress={false}
        expanded={expanded}
        onToggle={onToggle}
        time={formatEntryTime(entry.createdAt)}
        titleDataSet={{ terminalIdLabel: entry.terminalId }}
      >
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
        title={`${String(entry.entries.length)} 项`}
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
            <Text style={styles.chip}>{item.status}</Text>
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
 * 文件改动卡片：结构化统计与相对化路径（D8）。
 *
 * 差异行**不**在这里渲染——它属于右栏（D6：差异行只来自结构化来源，且没有下发时的说明也在
 * 那里）。这条条目携带文件标识（`changeId`），因此选中它会让右栏选中同一处改动
 * （spec「从条目联动到右栏」）；缺省回调时不渲染这个入口。
 */
function FileChangedEntry({
  entry,
  selected,
  onSelectChange,
}: {
  readonly entry: EntryOf<"file_changed">;
  readonly selected: boolean;
  readonly onSelectChange: ((changeId: string) => void) | undefined;
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
        changeSelected: String(selected),
        timelineEntryAt: entry.createdAt,
      }}
    >
      <View style={styles.card}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>文件改动</Text>
          {onSelectChange === undefined ? (
            <Text style={styles.title} dataSet={{ changePathLabel: entry.displayPath }}>
              {entry.displayPath}
            </Text>
          ) : (
            <Pressable
              role="button"
              style={selected ? [styles.selectablePath, styles.selectablePathSelected] : styles.selectablePath}
              dataSet={{ action: "select-file-change", changeId: entry.changeId, changePathLabel: entry.displayPath }}
              onPress={() => {
                onSelectChange(entry.changeId);
              }}
            >
              <Text style={styles.selectablePathText}>{entry.displayPath}</Text>
            </Pressable>
          )}
          <Text style={styles.chip}>{entry.changeKind}</Text>
          {entry.outsideWorkspace ? (
            <Text style={styles.chipWarn} dataSet={{ changeOutsideWorkspace: "true" }}>
              工作区之外
            </Text>
          ) : null}
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
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
        <Text style={styles.dimNote} dataSet={{ changeDiagnosticsHint: "true" }}>
          差异行与完整原文的入口都在「变更文件」栏（本卡片只呈现有结构化来源的统计，不解析原文）。
        </Text>
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
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
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
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
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
 * 授权请求卡片（原型 `.perm` 的形态：标题 + 说明块 + 三个应答按钮）。
 *
 * 三种可区分呈现来自 `availability` 这一个判别值：本设备亲历（`answerable`，有按钮）／仅历史读到
 * （`not_answerable read_from_history`，没有按钮 + 说明）／已被其它设备处理（`handled_elsewhere`，
 * 没有按钮 + 处理设备标识）——判定本身在 `domain/interaction.ts`，本卡片只照着画。
 */
function PermissionRequestedEntry({
  entry,
  onAnswerInteraction,
}: {
  readonly entry: EntryOf<"permission_requested">;
  readonly onAnswerInteraction: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

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
      <View style={styles.permission}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>授权请求</Text>
          <Text style={styles.permissionTitle} dataSet={{ interactionTitle: entry.title }}>
            {entry.title}
          </Text>
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
        </View>
        {entry.description === null ? null : (
          <Text style={styles.permissionDescription} dataSet={{ interactionDescription: "true" }}>
            {entry.description}
          </Text>
        )}
        {entry.turnId === null ? null : (
          <Text style={styles.meta} dataSet={{ interactionTurnId: entry.turnId }}>
            {`所属轮次：${entry.turnId}`}
          </Text>
        )}
        {/* 应答按钮排在 opts 行里：可选项只来自这次请求自身的下发内容（逐字呈现，不翻译）。 */}
        <View style={styles.permissionOptions}>
          <InteractionAnswerButtons
            interactionId={entry.interactionId}
            availability={entry.availability}
            onAnswerInteraction={onAnswerInteraction}
          />
        </View>
        <InteractionAvailabilityNote availability={entry.availability} />
      </View>
    </View>
  );
}

/** 提问请求卡片。 */
function ElicitationRequestedEntry({
  entry,
  onAnswerInteraction,
}: {
  readonly entry: EntryOf<"elicitation_requested">;
  readonly onAnswerInteraction: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

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
      <View style={styles.card}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>提问</Text>
          <Text style={styles.title} dataSet={{ interactionTitle: entry.title }}>
            {entry.title}
          </Text>
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
        </View>
        {/* 表单结构是开放对象：原样作为不可信文本呈现，不据此推断控件、也不构造载荷。 */}
        <Text style={styles.kicker} dataSet={{ elicitationSchema: "true" }}>
          {JSON.stringify(entry.schema)}
        </Text>
        {/* 本客户端不构造表单值：`submit` 以空内容提交（协议允许 `values: null`）。把这句话写出来，
            用户才不会以为「提交」会带上他刚看到的那份表单结构。 */}
        <Text style={styles.note} dataSet={{ elicitationValues: "not-collected" }}>
          本客户端不收集表单值：直接提交时以空内容应答（协议允许 `values: null`）。
        </Text>
        <InteractionAnswerButtons
          interactionId={entry.interactionId}
          availability={entry.availability}
          onAnswerInteraction={onAnswerInteraction}
        />
        <InteractionAvailabilityNote availability={entry.availability} />
      </View>
    </View>
  );
}

/** 授权已处理卡片（事实原样呈现，不推断是谁处理的——那是 `availability` 的判定）。 */
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
            {entry.resolution}
          </Text>
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
        </View>
        <Text style={styles.meta} dataSet={{ resolvedByDeviceLabel: entry.resolvedByDeviceId ?? "unknown" }}>
          处理设备标识原样呈现（缺省即未下发）。
        </Text>
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
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
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
  entry,
  onOpenDiagnostics,
}: {
  readonly entry: EntryOf<"degraded">;
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
      <DegradedEntryContent model={model} onOpenDiagnostics={onOpenDiagnostics} />
    </View>
  );
}

/** 一条时间线条目的呈现。 */
function TimelineEntryView(props: {
  readonly entry: TimelineEntry;
  readonly expandedEntries: Readonly<Record<string, boolean>> | undefined;
  /**
   * 折叠块表头被点选：上抛条目键与**切换后的**值。子组件拿到的是当前**生效**值
   * （`isExpanded`：显式覆盖优先、否则由条目状态推出），因此翻转必须在这里发生——页面只存覆盖值，
   * 它看不到「这条默认值是多少」（D10）。
   */
  readonly onToggleEntry: ((key: string, expanded: boolean) => void) | undefined;
  readonly selectedChangeId: string | null;
  readonly onSelectChange: ((changeId: string) => void) | undefined;
  readonly onOpenDiagnostics: (eventId: string) => void;
  readonly onAnswerInteraction: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
  readonly onCopyCode: ((code: string, language: string) => void) | undefined;
  readonly onLinkPress: ((href: string) => void) | undefined;
}): ReactElement {
  const entry = props.entry;
  const expanded = isExpanded(entry, props.expandedEntries);
  switch (entry.kind) {
    case "message":
      return (
        <MessageEntry
          entry={entry}
          expanded={expanded}
          onToggle={props.onToggleEntry}
          onCopyCode={props.onCopyCode}
          onLinkPress={props.onLinkPress}
        />
      );
    case "turn":
      return <TurnEntry entry={entry} />;
    case "tool_call":
      return <ToolCallEntry entry={entry} expanded={expanded} onToggle={props.onToggleEntry} />;
    case "terminal_output":
      return <TerminalOutputEntry entry={entry} expanded={expanded} onToggle={props.onToggleEntry} />;
    case "plan":
      return <PlanEntryView entry={entry} expanded={expanded} onToggle={props.onToggleEntry} />;
    case "file_changed":
      return (
        <FileChangedEntry
          entry={entry}
          selected={props.selectedChangeId === entry.changeId}
          onSelectChange={props.onSelectChange}
        />
      );
    case "mode_changed":
      return <ModeChangedEntry entry={entry} />;
    case "config_changed":
      return <ConfigChangedEntry entry={entry} />;
    case "permission_requested":
      return <PermissionRequestedEntry entry={entry} onAnswerInteraction={props.onAnswerInteraction} />;
    case "elicitation_requested":
      return <ElicitationRequestedEntry entry={entry} onAnswerInteraction={props.onAnswerInteraction} />;
    case "permission_resolved":
      return <PermissionResolvedEntry entry={entry} />;
    case "elicitation_resolved":
      return <ElicitationResolvedEntry entry={entry} />;
    case "degraded":
      return <DegradedEntry entry={entry} onOpenDiagnostics={props.onOpenDiagnostics} />;
  }
}

/**
 * 时间线。
 *
 * 容器同时带 `data-timeline="entries"`（新的一等锚点）与 `data-message-list="messages"`（既有
 * 消费方的锚点）：消息与结构化条目进的是同一份列表，因此两者都在它里面。
 */
export function ConversationTimeline({
  entries,
  onOpenDiagnostics,
  onAnswerInteraction,
  onCopyCode,
  onLinkPress,
  expandedEntries,
  onToggleEntry,
  selectedChangeId,
  onSelectChange,
}: ConversationTimelineProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View
      role="list"
      style={styles.timeline}
      dataSet={{ timeline: "entries", messageList: "messages", timelineEntryCount: entries.length }}
    >
      {entries.map((entry) => (
        <TimelineEntryView
          key={entry.key}
          entry={entry}
          expandedEntries={expandedEntries}
          onToggleEntry={onToggleEntry}
          selectedChangeId={selectedChangeId ?? null}
          onSelectChange={onSelectChange}
          onOpenDiagnostics={onOpenDiagnostics}
          onAnswerInteraction={onAnswerInteraction}
          onCopyCode={onCopyCode}
          onLinkPress={onLinkPress}
        />
      ))}
    </View>
  );
}
