/**
 * 对话页输入区（spec「输入与提交只支持文本且以终态为准」「取消当前轮次以终态为准」
 * 「未上报的上下文不得以零冒充」；`design.md` D6/D10）。
 *
 * ## 三态与「提交不等于完成」
 *
 * 输入区只呈现模型给的三态（`editable` / `in_progress` / `closed`）与两个呈现值
 * （`submission` / `cancel`）：**本组件不构造命令、不本地置态**。因此「提交后立刻显示已发送」
 * 在这里没有表达方式——`submission.kind` 为 `pending_confirmation` 时只能说「等待服务端确认」，
 * `cancel.kind` 为 `requested` 时只能说「取消请求已提交，等待终态」。
 *
 * 三态对应三种可观察形态：`editable` 可改草稿并呈现发送；`in_progress` 草稿只读、**不渲染**
 * 发送按钮，只在轮次可取消时呈现停止；`closed` 只读、发送禁用并给出关闭原因。
 *
 * ## 回车提交 / Shift+回车换行
 *
 * 回车走 `onKeyPress`（RNW 把宿主的 `keydown` 原样交给它，因此 `nativeEvent` 上读得到
 * `key`/`shiftKey`，`preventDefault()` 也拦得住换行）。判定收敛在 {@link composerKeyAction}：
 * **不可提交**（会话已关闭 / 来源离线 / 空草稿 / 没接线回调）时回车被拦下且**不派发任何命令**，
 * 连换行也不插入——「回车」在这里没有第三条含义。
 *
 * ## 上下文胶囊与告警条
 *
 * 上下文占用是**胶囊 + 明细**（`ContextUsageBar`，未上报时不给数字），占用告警条由
 * {@link contextWarningOf} 给出：只有 `known` 且占比达到阈值才出现，未上报与零窗口都**不**触发
 * ——没有数字就没有可推算的占比（R17）。未上报也不禁用任何交互。阈值（60 / 85 两档）与胶囊
 * 共用 `src/domain/context-usage.ts` 的一处定义，本组件不再自带数字。
 *
 * ## 附件不受支持是显式说明
 *
 * 附件在 v1 没有表达方式（`attachmentsSupported` 恒为假），说明文案由模型给出且**始终渲染**：
 * 「没提附件」与「说了不支持」是两件事，spec 要求后者。
 *
 * ## 草稿的内存边界
 *
 * 来源为其它节点的会话，草稿只在内存保留并标注未提交（`draftInMemoryOnly` / `draftNotice`）。
 * 组件只呈现这两个标记，不落任何存储（组件也不该碰存储）。
 */

import type { ReactElement } from "react";

import type { ContextUsageView } from "../domain";
import { CONTEXT_CRIT_THRESHOLD_PERCENT } from "../domain";
import type {
  CancelPresentation,
  ConversationInputModel,
  SubmissionPresentation,
} from "../features/conversation-input-model";
import type { ConversationSettingsModel } from "../features/conversation-settings-model";
import { ContextUsageBar } from "./ContextUsageBar";
import { ConversationSettingsPanel } from "./ConversationSettingsPanel";
import type { Palette, TextInputProps } from "./theme";
import {
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
  fontFamily,
  fontSize,
  palettes,
  radius,
  spacing,
  useTheme,
} from "./theme";

export interface ConversationComposerProps {
  readonly input: ConversationInputModel;
  /** 上下文用量：胶囊 + 明细；缺省时不呈现上下文条。 */
  readonly usage?: ContextUsageView | null;
  /** 模型与模式：胶囊 + 弹层；缺省时不呈现（对话页一律传模型给的那一份）。 */
  readonly settings?: ConversationSettingsModel | null;
  /** 草稿变化（页面写进仓库；草稿只在内存）。 */
  readonly onDraftChange?: ((text: string) => void) | undefined;
  /** 提交当前草稿（页面负责派发 `session.prompt`）；缺省时提交按钮不可点。 */
  readonly onSubmit?: ((draft: string) => void) | undefined;
  /** 取消当前轮次（页面负责派发 `session.cancel`）；缺省时取消按钮不可点。 */
  readonly onCancel?: (() => void) | undefined;
  /** 切换模型（`configId` 与取值由设置模型给出）。 */
  readonly onSelectModel?: ((configId: string, value: string) => void) | undefined;
  /** 切换模式。 */
  readonly onSelectMode?: ((modeId: string) => void) | undefined;
}

/** 输入区的状态文案：三态措辞互不相同。 */
const STATE_LABELS: Readonly<Record<ConversationInputModel["state"], string>> = {
  editable: "可输入",
  in_progress: "进行中",
  closed: "会话已关闭",
};

/** 输入框的占位文案：与状态对应，避免关闭的会话看起来还能输入。 */
const STATE_PLACEHOLDERS: Readonly<Record<ConversationInputModel["state"], string>> = {
  editable: "给 Agent 发消息（只支持文本）",
  in_progress: "进行中，完成后可继续输入",
  closed: "会话已关闭",
};

/** 提示行里的状态补充；`editable` 不需要额外措辞。 */
const STATE_HINTS: Readonly<Partial<Record<ConversationInputModel["state"], string>>> = {
  in_progress: "进行中 · 输入只读，按钮变为停止",
  closed: "会话已关闭",
};

/**
 * 输入区上方的占用告警条（原型 `ctxWarn`）。
 *
 * 只有 `known` 才判得出占比：未上报与窗口为零时**不**呈现告警条，也**不**由客户端推算一个
 * 占比出来（R17 的两条场景）。阈值取 {@link CONTEXT_CRIT_THRESHOLD_PERCENT}——原型在
 * `ctxWarn()` 与 `ctxChip()` 里把 85 各写了一次，这里与胶囊共用同一处定义
 * （`src/domain/context-usage.ts`）。
 */
export function contextWarningOf(usage: ContextUsageView | null | undefined): string | null {
  if (usage === null || usage === undefined) return null;
  if (usage.kind !== "known") return null;
  if (usage.percent < CONTEXT_CRIT_THRESHOLD_PERCENT) return null;
  return (
    `上下文已用 ${usage.percent.toFixed(0)}%。继续发送时 Agent 可能截断较早的内容，建议先开个新会话。` +
    "这是 Agent 的行为，网页端只做提示。"
  );
}

/** 输入框按键的处理方式。 */
export type ComposerKeyAction =
  /** 与本输入区无关的键：原样交给输入框。 */
  | "ignore"
  /** Shift+回车：换行（不提交、不拦截）。 */
  | "newline"
  /** 回车且可提交：提交当前草稿。 */
  | "submit"
  /** 回车但**不可**提交：拦下换行，且不派发任何命令。 */
  | "blocked";

/**
 * 回车的判定（`shiftKey` 一起给，因为「回车提交」与「Shift+回车换行」是同一个按键的两支）。
 *
 * `submittable` 由调用方按模型的 `canSubmit`、草稿是否为空与回调是否接线算出——判定只有这一处，
 * 组件里不再写第二个 `if`。
 */
export function composerKeyAction(
  submittable: boolean,
  key: { readonly key?: string; readonly shiftKey?: boolean },
): ComposerKeyAction {
  if (key.key !== "Enter") return "ignore";
  if (key.shiftKey === true) return "newline";
  return submittable ? "submit" : "blocked";
}

/** `onKeyPress` 的签名（类型从 `theme.ts` 的 `TextInputProps` 取，不直接把 react-native 引进组件）。 */
type ComposerKeyPress = NonNullable<TextInputProps["onKeyPress"]>;

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    composer: {
      gap: spacing.xs,
      paddingTop: 10,
      paddingBottom: 14,
      paddingHorizontal: 20,
      borderTopWidth: 1,
      borderTopColor: colors.line,
      backgroundColor: colors.panel,
    },
    warnBar: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.danger,
      borderRadius: radius.sm,
      backgroundColor: colors.dangerSoft,
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    bar: {
      justifyContent: "flex-end",
      marginBottom: 8,
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    inputRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "flex-end",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel2,
    },
    input: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 0,
      minHeight: 44,
      paddingVertical: spacing.xs,
      paddingHorizontal: 0,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    inputClosed: {
      color: colors.ink3,
    },
    primary: {
      flexShrink: 0,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.lg,
      borderWidth: 1,
      borderColor: colors.accent,
      borderRadius: radius.sm,
      backgroundColor: colors.accent,
    },
    primaryText: {
      color: "#ffffff",
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    buttonDisabled: {
      borderColor: colors.line,
      backgroundColor: colors.panel2,
    },
    buttonDisabledText: {
      color: colors.ink3,
    },
    stop: {
      flexShrink: 0,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.danger,
      borderRadius: radius.sm,
      backgroundColor: colors.danger,
    },
    stopText: {
      color: "#ffffff",
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    hintRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.lg,
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
    kbd: {
      paddingHorizontal: spacing.xs,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: 4,
      backgroundColor: colors.panel2,
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    hintWarn: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    hintDanger: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    pending: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    alert: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.warn,
      borderRadius: radius.sm,
      backgroundColor: colors.warnSoft,
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    danger: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    ok: {
      color: colors.ok,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/**
 * 提交的一行呈现。
 *
 * `pending_confirmation` 的文案刻意不写「已发送 / 已排队」——那正是 spec MUST NOT 的两种措辞；
 * `uncertain` 用 `role="alert"` 显著保留，直到用户处理（不随时间消失）。
 */
function SubmissionLine({ submission }: { readonly submission: SubmissionPresentation }): ReactElement | null {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  if (submission.kind === "none") return null;
  if (submission.kind === "pending_confirmation") {
    return (
      <Text style={styles.pending} dataSet={{ submission: "pending_confirmation", submissionRequestId: submission.requestId }}>
        已提交给服务端，等待终态确认（尚未确认）。
      </Text>
    );
  }
  if (submission.kind === "accepted") {
    return (
      <Text
        style={styles.note}
        dataSet={{ submission: "accepted", ...(submission.acceptedAt === null ? {} : { submissionAcceptedAt: submission.acceptedAt }) }}
      >
        服务端已接受。
      </Text>
    );
  }
  if (submission.kind === "completed") {
    return (
      <Text style={styles.ok} dataSet={{ submission: "completed" }}>
        服务端已处理完成。
      </Text>
    );
  }
  if (submission.kind === "uncertain") {
    return (
      <Text role="alert" style={styles.alert} dataSet={{ submission: "uncertain" }}>
        {`结果不确定：${submission.detail}。请先到电脑端核对，再决定是否重发。`}
      </Text>
    );
  }
  return (
    <Text style={styles.danger} dataSet={{ submission: "failed" }}>
      {`提交失败：${submission.detail}`}
    </Text>
  );
}

/** 取消的一行呈现。 */
function CancelLine({ cancel }: { readonly cancel: CancelPresentation }): ReactElement | null {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  if (cancel.kind === "unavailable") {
    return (
      <Text style={styles.hint} dataSet={{ cancel: "unavailable" }}>
        当前没有可取消的轮次。
      </Text>
    );
  }
  if (cancel.kind === "requested") {
    return (
      <Text style={styles.pending} dataSet={{ cancel: "requested", cancelRequestId: cancel.requestId }}>
        取消请求已提交，等待服务端终态（未确认前不呈现为空闲）。
      </Text>
    );
  }
  if (cancel.kind === "confirmed") {
    return (
      <Text style={styles.ok} dataSet={{ cancel: "confirmed" }}>
        服务端已确认取消。
      </Text>
    );
  }
  return (
    <Text style={styles.note} dataSet={{ cancel: "available", ...(cancel.failure === null ? {} : { cancelFailure: cancel.failure }) }}>
      {cancel.failure === null ? "当前轮次可取消。" : `上一次取消失败：${cancel.failure}`}
    </Text>
  );
}

/** 输入区。 */
export function ConversationComposer({
  input,
  usage,
  settings,
  onDraftChange,
  onSubmit,
  onCancel,
  onSelectModel,
  onSelectMode,
}: ConversationComposerProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const editable = input.state === "editable";
  const warning = contextWarningOf(usage);
  // 进行中的提示随「停止」是否可用变化：没有可取消的轮次时不能暗示有一个停止按钮。
  // 只有 `unavailable` 才说「不可取消」：`requested`/`confirmed` 是在等终态/已确认，
  // 取消行自己给出精确措辞，这里回落通用提示，避免与「取消请求已提交」自相矛盾。
  const stateHint =
    input.state === "in_progress" && input.cancel.kind === "unavailable"
      ? "进行中 · 输入只读，当前轮次不可取消"
      : STATE_HINTS[input.state];
  // 提交闸门只有这一处：模型的 `canSubmit`（会话关闭 / 连接离线 / 进行中 / imported 来源都在模型里
  // 变成假）加上「草稿非空」与「回调已接线」。仓库侧 `submitPrompt` 还会再拦一次，不靠组件记得禁用。
  const submittable = editable && input.canSubmit && input.draft.trim().length > 0;

  const onKeyPress: ComposerKeyPress = (event) => {
    // RNW 把宿主 `keydown` 原样交给 `onKeyPress`：`nativeEvent` 上读得到 `key` 与 `shiftKey`。
    const native = event.nativeEvent as { readonly key?: string; readonly shiftKey?: boolean };
    const action = composerKeyAction(submittable && onSubmit !== undefined, native);
    if (action === "ignore" || action === "newline") return;
    // 回车一律不落换行（原型同样在 keydown 上 preventDefault）：提交与「不可提交」两支都不该
    // 往草稿里塞一个换行——后者正是「不得产生副作用」的那一条。
    event.preventDefault();
    if (action === "blocked") return;
    onSubmit?.(input.draft);
  };

  return (
    <View style={styles.composer} dataSet={{ composer: input.state, composerDraftInMemoryOnly: String(input.draftInMemoryOnly) }}>
      {/* 接近上限的告警条：只有已上报且达到阈值才出现（未上报 / 零窗口不推算占比）。 */}
      {warning === null ? null : (
        <Text role="alert" style={styles.warnBar} dataSet={{ contextWarning: "true" }}>
          {warning}
        </Text>
      )}

      <View style={styles.bar} dataSet={{ composerBar: "true" }}>
        {/* 上下文胶囊 + 明细（未上报时不给数字）；未上报不影响任何交互。 */}
        {usage === null || usage === undefined ? null : <ContextUsageBar usage={usage} />}
        {/* 模型 / 模式胶囊与弹层。 */}
        {settings === null || settings === undefined ? null : (
          <ConversationSettingsPanel
            settings={settings}
            onSelectModel={onSelectModel}
            onSelectMode={onSelectMode}
          />
        )}
      </View>

      <View style={styles.inputRow}>
        <TextInput
          value={input.draft}
          // 只有 `editable` 能改草稿：进行中与已关闭都只读（进行中的变化由停止按钮表达）。
          editable={editable}
          multiline
          placeholder={STATE_PLACEHOLDERS[input.state]}
          style={editable ? styles.input : [styles.input, styles.inputClosed]}
          dataSet={{ composerInput: input.state }}
          onChangeText={(text: string) => {
            // 只读输入不再接受草稿写入：平台差异下即使触发了 `onChangeText`，也不上抛。
            if (!editable) return;
            onDraftChange?.(text);
          }}
          onKeyPress={onKeyPress}
        />

        {/* 进行中只呈现停止（取消可用时）；发送按钮只在可输入与已关闭两态出现（后者禁用并说明原因）。 */}
        {input.state === "in_progress" ? null : (
          <Pressable
            role="button"
            // 闸门是模型给的 `canSubmit`：关闭 / 离线 / imported 来源都会在这里变为不可点
            // （仓库侧 `submitPrompt` 还会再拦一次，不靠组件记得禁用）。
            disabled={!input.canSubmit || onSubmit === undefined}
            style={!input.canSubmit || onSubmit === undefined ? [styles.primary, styles.buttonDisabled] : styles.primary}
            dataSet={{ action: "submit-prompt", submitEnabled: String(input.canSubmit) }}
            onPress={() => {
              onSubmit?.(input.draft);
            }}
          >
            <Text style={input.canSubmit ? styles.primaryText : [styles.primaryText, styles.buttonDisabledText]}>发送</Text>
          </Pressable>
        )}

        {input.cancel.kind === "available" ? (
          <Pressable
            role="button"
            disabled={onCancel === undefined}
            style={styles.stop}
            dataSet={{ action: "cancel-turn" }}
            onPress={() => {
              onCancel?.();
            }}
          >
            <Text style={styles.stopText}>■ 停止（取消当前轮次）</Text>
          </Pressable>
        ) : null}
      </View>

      {input.canSubmit ? null : (
        <Text style={styles.note} dataSet={{ submitUnavailable: "true" }}>
          {input.disabledReason ?? "当前不能提交。"}
        </Text>
      )}

      {/* 提示行：按键口径与状态补充（原型 `.hint`）。 */}
      <View style={styles.hintRow} dataSet={{ composerHint: "true" }}>
        <Text style={styles.hint} dataSet={{ composerState: input.state }}>
          {STATE_LABELS[input.state]}
        </Text>
        {/* 附件不受支持的显式说明：始终渲染，不因状态改变。 */}
        <Text style={styles.hint} dataSet={{ attachmentsSupported: String(input.attachmentsSupported), attachmentNotice: "true" }}>
          {input.attachmentNotice}
        </Text>
        <Text style={styles.hint} dataSet={{ composerKeys: "enter-submits" }}>
          <Text style={styles.kbd}>Enter</Text> 发送 · <Text style={styles.kbd}>Shift</Text>+
          <Text style={styles.kbd}>Enter</Text> 换行
        </Text>
        {stateHint === undefined ? null : (
          <Text
            style={input.state === "closed" ? styles.hintDanger : styles.hintWarn}
            dataSet={{ composerStateHint: input.state }}
          >
            {stateHint}
          </Text>
        )}
      </View>

      <SubmissionLine submission={input.submission} />
      {/* 没有可取消轮次时不占一行：原型也没有这条静息文案，窄高度下它会挤掉时间线。 */}
      {input.cancel.kind === "unavailable" ? null : <CancelLine cancel={input.cancel} />}

      {input.draftNotice === null ? null : (
        <Text style={styles.note} dataSet={{ draftNotice: "true" }}>
          {input.draftNotice}
        </Text>
      )}
    </View>
  );
}
