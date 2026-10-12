/**
 * 对话页正文（时间线 + 输入区 + 设置 + 变更文件栏；spec「对话页按时间线呈现消息与结构化条目」
 * 「输入与提交只支持文本且以终态为准」「模型与模式的呈现与切换」「变更文件的呈现边界」
 * 「对话页的宽窄两种形态」；`design.md` D7/D8/D9/D10）。
 *
 * ## 一页 = 时间线 + 输入 + 设置 + 改动栏
 *
 * 判定全部在 `src/features/` 与 `src/domain/` 的纯函数里完成（`conversation-model.ts` 的
 * `ConversationPageModel` 是唯一输入），本组件只做两件事：按模型渲染、把交互原样上抛。
 * 因此页面（`app/session/[id].tsx`）里没有业务分支，组件里也没有——需要决策的地方在这里都
 * 表现为读取模型已经算好的判别值。
 *
 * ## 宽窄两形态只差样式
 *
 * `availableWidth` 缺省取 `useWindowDimensions()`（RNW 没有媒体查询）。窄屏下变更文件栏降为
 * **可切换**的次级视图：切换控件在两种形态下都渲染，窄屏用它把变更文件升为当前视图——但两列
 * 内容始终都在 DOM 里（窄屏那一列只是 `display: none`），因此「窄屏不丢信息」是一条可断言的事实
 * （两种形态的 `data-*` 集合在同一交互状态下完全相同），而不是靠自觉。
 *
 * 未接线切换回调时**不**降级成「只显示时间线」：那种默认会让用户看不到变更文件又没有切换入口
 * （等于静默丢弃），因此那时两列都呈现。
 *
 * ## 正文按不可信输入渲染
 *
 * 消息走 `Markdown`、终端输出与代码块走 `CodeBlock`（WP4）：原始 HTML 不生效、远端资源不加载、
 * 样式全走 `theme.ts` 的 token。
 */

import type { ReactElement } from "react";

import type { InteractionAnswer } from "../domain/interaction";
import type { ConversationPageModel } from "../features/conversation-model";
import { ConversationComposer } from "./ConversationComposer";
import { ConversationSettingsPanel } from "./ConversationSettingsPanel";
import { ConversationTimeline } from "./ConversationTimeline";
import { ContextUsageBar } from "./ContextUsageBar";
import { FileChangesPanel } from "./FileChangesPanel";
import { PendingInteractionsPanel } from "./PendingInteractionsPanel";
import { SegmentControl } from "./SegmentControl";
import { StatusPill } from "./StatusPill";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme, useWindowDimensions } from "./theme";

/** 宽屏断点（像素）：与目录页/目录详情页同一个取值（D7/D10）。 */
const WIDE_LAYOUT_MIN_WIDTH = 720;

/**
 * 次级视图的取值：两列都看 / 只看时间线 / 只看变更文件。
 *
 * `both` 是宽屏缺省（并列呈现）；窄屏缺省是 `timeline`，用户经切换控件把它换成 `files`。
 */
export type ConversationSecondaryView = "both" | "timeline" | "files";

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
  /** 次级视图（页面持有的 UI 状态）；缺省时按宽度取默认值。 */
  readonly secondaryView?: ConversationSecondaryView | null;
  /** 次级视图切换；缺省时不渲染切换控件（那时两列都呈现，见文件头）。 */
  readonly onSecondaryViewChange?: ((view: ConversationSecondaryView) => void) | undefined;
  /** 可用宽度（像素）；缺省取 `useWindowDimensions()`。渲染测试据此注入窄/宽两档。 */
  readonly availableWidth?: number | undefined;
  /** 代码块复制入口的落点。 */
  readonly onCopyCode?: ((code: string, language: string) => void) | undefined;
  /** 链接被点选时上抛的目标。 */
  readonly onLinkPress?: ((href: string) => void) | undefined;
}

/** 次级视图切换控件的选项（两种形态下同一份，保证「信息集合相同」可断言）。 */
const SECONDARY_VIEW_OPTIONS: readonly { readonly value: ConversationSecondaryView; readonly label: string }[] = [
  { value: "both", label: "全部" },
  { value: "timeline", label: "对话" },
  { value: "files", label: "变更文件" },
];

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    root: {
      gap: spacing.md,
      padding: spacing.lg,
      backgroundColor: colors.bg,
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
      alignItems: "flex-start",
      gap: spacing.md,
    },
    bodyNarrow: {
      flexDirection: "column",
      gap: spacing.md,
    },
    timelineColumn: {
      flexGrow: 2,
      flexShrink: 1,
      minWidth: 0,
      gap: spacing.md,
    },
    filesColumn: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 0,
      gap: spacing.md,
    },
    hidden: {
      display: "none",
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
  secondaryView,
  onSecondaryViewChange,
  availableWidth,
  onCopyCode,
  onLinkPress,
}: ConversationStreamProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const windowDimensions = useWindowDimensions();
  const width = availableWidth ?? windowDimensions.width;
  const wide = width >= WIDE_LAYOUT_MIN_WIDTH;
  const switchable = onSecondaryViewChange !== undefined;
  // 未接线切换控件时两列都呈现（见文件头）：那时没有「次级视图」这一形态，绝不能默认藏起一列。
  const view: ConversationSecondaryView = secondaryView ?? (switchable && !wide ? "timeline" : "both");
  const hideTimeline = view === "files";
  const hideFiles = view === "timeline";
  const paging = model.paging;
  const history = model.timeline.structuredHistory;
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

  return (
    <section data-route="session" data-session-id={model.header.sessionId}>
      <View style={styles.root} dataSet={{ conversationPage: "true" }}>
        <View style={styles.header} dataSet={{ sessionHeader: model.header.sessionId }}>
          <View style={styles.headerRow}>
            <Text style={styles.title} dataSet={{ sessionTitle: "true" }}>
              {model.header.title}
            </Text>
            <StatusPill state={model.header.state} />
          </View>
          <View style={styles.headerRow}>
            <Text style={styles.agent} dataSet={{ sessionAgent: model.header.agentName }}>
              {model.header.agentName}
            </Text>
            {model.header.originOfflineLabel === null ? null : (
              <Text style={styles.offline} dataSet={{ originOffline: "true" }}>
                {model.header.originOfflineLabel}
              </Text>
            )}
          </View>
        </View>

        {/* 上下文条：三种状态各自独立呈现，未上报不是零（R17）。 */}
        <ContextUsageBar usage={model.usage} />

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
                {`本页 ${model.messages.length} 条消息。`}
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

        {/* 结构化历史不可用时必须有明确说明，且不可用 ≠ 「这个会话没有结构化内容」（D11）。 */}
        {history.notice === null ? null : (
          <Text style={styles.historyNotice} dataSet={{ structuredHistory: history.state }}>
            {history.notice}
          </Text>
        )}

        {/* 切换控件在两种形态下都渲染：窄屏用它把变更文件升为当前视图。 */}
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
          <View
            style={hideTimeline ? [styles.timelineColumn, styles.hidden] : styles.timelineColumn}
            dataSet={{ conversationColumn: "timeline" }}
          >
            {pendingOnly.length === 0 ? null : (
              <PendingInteractionsPanel interactions={pendingOnly} onAnswerInteraction={onAnswerInteraction} />
            )}
            <ConversationTimeline
              entries={model.timeline.entries}
              onOpenDiagnostics={onOpenDiagnostics}
              onAnswerInteraction={onAnswerInteraction}
              onCopyCode={onCopyCode}
              onLinkPress={onLinkPress}
            />
          </View>

          <View
            style={hideFiles ? [styles.filesColumn, styles.hidden] : styles.filesColumn}
            dataSet={{ conversationColumn: "files" }}
          >
            <FileChangesPanel
              changes={model.fileChanges}
              openDiagnosticsEventId={openChangeDiagnosticsEventId ?? null}
              onOpenChangeDiagnostics={onOpenChangeDiagnostics}
            />
          </View>
        </View>

        <ConversationComposer
          input={model.input}
          onDraftChange={onDraftChange}
          onSubmit={onSubmitPrompt}
          onCancel={onCancelTurn}
        />

        <ConversationSettingsPanel settings={model.settings} onSelectModel={onSelectModel} onSelectMode={onSelectMode} />

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
      </View>
    </section>
  );
}
