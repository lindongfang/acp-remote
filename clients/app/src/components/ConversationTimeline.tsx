/**
 * 对话时间线：消息与结构化条目在**同一个**按时间排序的容器里呈现（spec「对话页按时间线呈现消息
 * 与结构化条目」；`design.md` D1/D5/D6/D8）。
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
 * ## 正文按不可信输入渲染
 *
 * 消息正文走 `Markdown`（原始 HTML 与远端资源都被降级，见 WP4 的组件）；终端输出的正文走
 * `CodeBlock`（等宽 + 复制）。结构化卡片的字段一律经 `Text` 原样呈现，`acp.rawJson` 不参与
 * 渲染（D8）——它的入口是诊断抽屉（降级条目）或右栏（改动）。
 */

import type { ReactElement } from "react";

import type { InteractionAnswer } from "../domain/interaction";
import type { TimelineEntry } from "../features/conversation-timeline";
import { CodeBlock } from "./CodeBlock";
import { DegradedEntryContent } from "./DegradedEntryContent";
import { InteractionAnswerButtons, InteractionAvailabilityNote } from "./InteractionAnswering";
import { Markdown } from "./Markdown";
import type { Palette } from "./theme";
import { StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

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
    messageUser: {
      alignItems: "flex-end",
    },
    who: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
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
      backgroundColor: colors.accentSoft,
      borderColor: colors.accentSoft,
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
    chipThink: {
      color: colors.think,
      backgroundColor: colors.thinkSoft,
    },
    chipTool: {
      color: colors.tool,
      backgroundColor: colors.toolSoft,
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
    danger: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
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

/** 轮次/工具调用状态的中性呈现：状态取值原样显示，不翻译成可能出错的措辞。 */
function StateChip({ state, style }: { readonly state: string; readonly style: object }): ReactElement {
  return (
    <Text style={[STYLES.light.chip, style]} dataSet={{ timelineState: state }}>
      {state}
    </Text>
  );
}

/** 一条消息条目。 */
function MessageEntry({
  entry,
  onCopyCode,
  onLinkPress,
}: {
  readonly entry: EntryOf<"message">;
  readonly onCopyCode: ((code: string, language: string) => void) | undefined;
  readonly onLinkPress: ((href: string) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const message = entry.message;

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
      <Text style={styles.who} dataSet={{ messageWho: entry.channel }}>
        {CHANNEL_LABELS[entry.channel]}
      </Text>
      {/* 正文按不可信输入渲染：`Markdown` 不渲染原始 HTML、不加载远端资源。 */}
      <View style={[styles.body, entry.channel === "user" ? styles.bodyUser : styles.body]} dataSet={{ messageText: "true" }}>
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
      <Text style={styles.time} dataSet={{ messageCreatedAt: entry.createdAt }}>
        {formatEntryTime(entry.createdAt)}
      </Text>
    </View>
  );
}

/** 轮次卡片。 */
function TurnEntry({ entry }: { readonly entry: EntryOf<"turn"> }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const error = entry.error;

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
        {error === null ? null : (
          <Text style={styles.danger} dataSet={{ turnError: error.code, turnErrorRetryable: String(error.retryable) }}>
            {`轮次失败：${error.message}`}
          </Text>
        )}
      </View>
    </View>
  );
}

/** 工具调用卡片。 */
function ToolCallEntry({ entry }: { readonly entry: EntryOf<"tool_call"> }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

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
      <View style={styles.card}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>工具调用</Text>
          <Text style={styles.title} dataSet={{ toolTitle: entry.title }}>
            {entry.title}
          </Text>
          <StateChip state={entry.state} style={styles.chipTool} />
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
        </View>
        {entry.turnId === null ? null : (
          <Text style={styles.meta} dataSet={{ toolTurnId: entry.turnId }}>
            {`所属轮次：${entry.turnId}`}
          </Text>
        )}
      </View>
    </View>
  );
}

/** 终端输出卡片（正文走 `CodeBlock`：等宽 + 复制）。 */
function TerminalOutputEntry({ entry }: { readonly entry: EntryOf<"terminal_output"> }): ReactElement {
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
      <View style={styles.card}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>终端输出</Text>
          <Text style={styles.meta} dataSet={{ terminalIdLabel: entry.terminalId }}>
            {`${entry.terminalId} · 第 ${entry.chunkIndex} 块`}
          </Text>
          {entry.truncated ? (
            <Text style={styles.chipWarn} dataSet={{ terminalTruncatedNote: "true" }}>
              已截断
            </Text>
          ) : null}
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
        </View>
        <CodeBlock code={entry.text} language={entry.stream} />
      </View>
    </View>
  );
}

/** 计划卡片。 */
function PlanEntryView({ entry }: { readonly entry: EntryOf<"plan"> }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View
      role="listitem"
      style={styles.entry}
      dataSet={{ timelineCard: "plan", planEntryCount: entry.entries.length, timelineEntryAt: entry.createdAt }}
    >
      <View style={styles.card}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>计划</Text>
          <Text style={styles.meta} dataSet={{ planEntryCountLabel: String(entry.entries.length) }}>
            {`${entry.entries.length} 项`}
          </Text>
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
        </View>
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
      </View>
    </View>
  );
}

/** 文件改动卡片：只有结构化统计与相对化路径（D8）——差异正文没有来源，因此不呈现。 */
function FileChangedEntry({ entry }: { readonly entry: EntryOf<"file_changed"> }): ReactElement {
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
      <View style={styles.card}>
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
        <Text style={styles.note} dataSet={{ changeDiagnosticsHint: "true" }}>
          完整原文的入口在「变更文件」栏（本卡片只呈现有结构化来源的统计，不解析原文）。
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

/** 授权请求卡片。 */
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
      <View style={styles.card}>
        <View style={styles.cardHead}>
          <Text style={styles.kicker}>授权请求</Text>
          <Text style={styles.title} dataSet={{ interactionTitle: entry.title }}>
            {entry.title}
          </Text>
          <Text style={styles.time}>{formatEntryTime(entry.createdAt)}</Text>
        </View>
        {entry.description === null ? null : (
          <Text style={styles.meta} dataSet={{ interactionDescription: "true" }}>
            {entry.description}
          </Text>
        )}
        {entry.turnId === null ? null : (
          <Text style={styles.meta} dataSet={{ interactionTurnId: entry.turnId }}>
            {`所属轮次：${entry.turnId}`}
          </Text>
        )}
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
  readonly onOpenDiagnostics: (eventId: string) => void;
  readonly onAnswerInteraction: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
  readonly onCopyCode: ((code: string, language: string) => void) | undefined;
  readonly onLinkPress: ((href: string) => void) | undefined;
}): ReactElement {
  const entry = props.entry;
  switch (entry.kind) {
    case "message":
      return <MessageEntry entry={entry} onCopyCode={props.onCopyCode} onLinkPress={props.onLinkPress} />;
    case "turn":
      return <TurnEntry entry={entry} />;
    case "tool_call":
      return <ToolCallEntry entry={entry} />;
    case "terminal_output":
      return <TerminalOutputEntry entry={entry} />;
    case "plan":
      return <PlanEntryView entry={entry} />;
    case "file_changed":
      return <FileChangedEntry entry={entry} />;
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
          onOpenDiagnostics={onOpenDiagnostics}
          onAnswerInteraction={onAnswerInteraction}
          onCopyCode={onCopyCode}
          onLinkPress={onLinkPress}
        />
      ))}
    </View>
  );
}
