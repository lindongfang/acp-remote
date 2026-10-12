/**
 * 对话页输入区（spec「输入与提交只支持文本且以终态为准」「取消当前轮次以终态为准」；
 * `design.md` D6/D10）。
 *
 * ## 三态与「提交不等于完成」
 *
 * 输入区只呈现模型给的三态（`editable` / `in_progress` / `closed`）与两个呈现值
 * （`submission` / `cancel`）：**本组件不构造命令、不本地置态**。因此「提交后立刻显示已发送」
 * 在这里没有表达方式——`submission.kind` 为 `pending_confirmation` 时只能说「等待服务端确认」，
 * `cancel.kind` 为 `requested` 时只能说「取消请求已提交，等待终态」。
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

import type { CancelPresentation, ConversationInputModel, SubmissionPresentation } from "../features/conversation-input-model";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, TextInput, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface ConversationComposerProps {
  readonly input: ConversationInputModel;
  /** 草稿变化（页面写进仓库；草稿只在内存）。 */
  readonly onDraftChange?: ((text: string) => void) | undefined;
  /** 提交当前草稿（页面负责派发 `session.prompt`）；缺省时提交按钮不可点。 */
  readonly onSubmit?: ((draft: string) => void) | undefined;
  /** 取消当前轮次（页面负责派发 `session.cancel`）；缺省时取消按钮不可点。 */
  readonly onCancel?: (() => void) | undefined;
}

/** 输入区的状态文案：三态措辞互不相同。 */
const STATE_LABELS: Readonly<Record<ConversationInputModel["state"], string>> = {
  editable: "可输入",
  in_progress: "进行中（当前轮次结束后可继续输入）",
  closed: "会话已关闭",
};

/** 输入框的占位文案：与状态对应，避免关闭的会话看起来还能输入。 */
const STATE_PLACEHOLDERS: Readonly<Record<ConversationInputModel["state"], string>> = {
  editable: "给 Agent 发消息（只支持文本）",
  in_progress: "进行中，完成后可继续输入",
  closed: "会话已关闭",
};

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    composer: {
      gap: spacing.sm,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel,
    },
    head: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    state: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    input: {
      minHeight: 64,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    inputClosed: {
      color: colors.ink3,
      backgroundColor: colors.panel2,
    },
    row: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    primary: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.lg,
      borderWidth: 1,
      borderColor: colors.accent,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
    },
    primaryText: {
      color: colors.accentInk,
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
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.danger,
      borderRadius: radius.sm,
      backgroundColor: colors.dangerSoft,
    },
    stopText: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
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
  onDraftChange,
  onSubmit,
  onCancel,
}: ConversationComposerProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const closed = input.state === "closed";

  return (
    <View style={styles.composer} dataSet={{ composer: input.state, composerDraftInMemoryOnly: String(input.draftInMemoryOnly) }}>
      <View style={styles.head}>
        <Text style={styles.state} dataSet={{ composerState: input.state }}>
          {STATE_LABELS[input.state]}
        </Text>
        {/* 附件不受支持的显式说明：始终渲染，不因状态改变。 */}
        <Text style={styles.hint} dataSet={{ attachmentsSupported: String(input.attachmentsSupported), attachmentNotice: "true" }}>
          {input.attachmentNotice}
        </Text>
      </View>

      <TextInput
        value={input.draft}
        editable={!closed}
        multiline
        placeholder={STATE_PLACEHOLDERS[input.state]}
        style={closed ? [styles.input, styles.inputClosed] : styles.input}
        dataSet={{ composerInput: input.state }}
        onChangeText={(text) => {
          onDraftChange?.(text);
        }}
      />

      <View style={styles.row}>
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

      <SubmissionLine submission={input.submission} />
      <CancelLine cancel={input.cancel} />

      {input.draftNotice === null ? null : (
        <Text style={styles.note} dataSet={{ draftNotice: "true" }}>
          {input.draftNotice}
        </Text>
      )}
    </View>
  );
}
