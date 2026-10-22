/**
 * 对话页输入区（spec「输入与提交支持协商内容且以终态为准」「取消当前轮次以终态为准」
 * 「未上报的上下文不得以零冒充」；`design.md` D3/D6/D9/D10）。
 *
 * ## 三态与「提交不等于完成」
 *
 * 输入区只呈现模型给的三态（`editable` / `in_progress` / `unavailable`）与两个呈现值
 * （`submission` / `cancel`）：**本组件不构造命令、不本地置态**。因此「提交后立刻显示已发送」
 * 在这里没有表达方式——`submission.kind` 为 `pending_confirmation` 时只能说「等待服务端确认」，
 * `cancel.kind` 为 `requested` 时只能说「取消请求已提交，等待终态」。
 *
 * 三态都**可编辑草稿**：`in_progress`（轮次执行中或提交等待终态）与 `unavailable`（来源离线、
 * imported 来源、附件能力未协商等）只是不能发送，且必须说明当前实际原因。组件不把会话主状态
 * 读成只读——`idle`/`running`/`pending` 都不构成永久禁提交的理由。
 *
 * ## 附件：选择只在内存，点击发送才上传
 *
 * 图片/文件/已有音频的选择与粘贴都只把句柄放进草稿（本机预览），**不上传**；上传由仓库在
 * 点击发送后启动，全部封存并登记清单才派发一条消息。任一失败/取消都不发文字。因此这里只呈现
 * 附件清单、进度与失败/取消/重试入口，不做任何网络或存储调用。
 *
 * ## 离开提醒只提示、不承诺恢复
 *
 * 有未发送内容（草稿、附件或路径引用）时注册 `beforeunload`，并在页面上给出一句说明：
 * 刷新/关闭后不恢复草稿与上传，也不支持续传。没有未发送内容时两者都不出现。
 *
 * ## 回车提交 / Shift+回车换行
 *
 * 回车走 `onKeyPress`（RNW 把宿主的 `keydown` 原样交给它，因此 `nativeEvent` 上读得到
 * `key`/`shiftKey`，`preventDefault()` 也拦得住换行）。判定收敛在 {@link composerKeyAction}：
 * **不可提交**（执行中 / 来源离线 / imported 来源 / 文字、附件与路径引用全空 / 没接线回调）时
 * 回车被拦下且**不派发任何命令**，连换行也不插入——「回车」在这里没有第三条含义。
 *
 * ## 上下文胶囊与告警条
 *
 * 上下文占用是**胶囊 + 明细**（`ContextUsageBar`，未上报时不给数字），占用告警条由
 * {@link contextWarningOf} 给出：只有 `known` 且占比达到阈值才出现，未上报与零窗口都**不**触发
 * ——没有数字就没有可推算的占比（R17）。未上报也不禁用任何交互。阈值（60 / 85 两档）与胶囊
 * 共用 `src/domain/context-usage.ts` 的一处定义，本组件不再自带数字。
 *
 * ## 草稿的内存边界
 *
 * 来源为其它节点的会话，草稿只在内存保留并标注未提交（`draftInMemoryOnly` / `draftNotice`）。
 * 组件只呈现这两个标记，不落任何存储（组件也不该碰存储）。
 */

import { useEffect, useState } from "react";
import type { ReactElement } from "react";

import type { ContextUsageView } from "../domain";
import { CONTEXT_CRIT_THRESHOLD_PERCENT } from "../domain";
import type { AttachmentKind, DraftAttachment } from "../features/attachment-model";
import type {
  CancelPresentation,
  ConversationInputModel,
  SubmissionPresentation,
} from "../features/conversation-input-model";
import type { ConversationSettingsModel } from "../features/conversation-settings-model";
import { ContextUsageBar } from "./ContextUsageBar";
import { ConversationSettingsPanel } from "./ConversationSettingsPanel";
import { formatBytes } from "./FileContentViewer";
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
  /** 打开平台选择器（选择阶段只入草稿，不上传）；缺省时不呈现对应按钮。 */
  readonly onPickAttachment?: ((kind: AttachmentKind) => void) | undefined;
  /** 粘贴事件（页面交给平台端口解析；组件只上抛原始事件）。 */
  readonly onPasteAttachments?: ((event: unknown) => void) | undefined;
  /** 移除一个已选附件（本机预览 URL 由仓库撤销）。 */
  readonly onRemoveAttachment?: ((localId: string) => void) | undefined;
  /** 重试失败/取消的附件（只重置这些项，不动已封存的）。 */
  readonly onRetryAttachments?: (() => void) | undefined;
  /** 取消正在进行的上传（不派发消息、保留草稿）。 */
  readonly onCancelAttachmentUpload?: (() => void) | undefined;
  /** 添加工作目录相对路径引用；返回拒绝原因（`null` = 已加入）。 */
  readonly onAddPathRef?: ((path: string) => string | null) | undefined;
  /** 移除一个路径引用。 */
  readonly onRemovePathRef?: ((path: string) => void) | undefined;
}

const STATE_PLACEHOLDERS = { editable: "输入消息…", in_progress: "编辑下一条消息…", unavailable: "当前无法发送，仍可编辑草稿…" };

/** 附件状态的展示文案（每种状态一句，互不混用）。 */
const ATTACHMENT_STATUS_LABELS: Readonly<Record<DraftAttachment["status"], string>> = {
  selected: "待上传（点击发送后上传）",
  uploading: "上传中",
  sealed: "已封存（整条消息发送前不会执行）",
  failed: "上传失败",
  cancelled: "已取消",
  expired: "临时上传已过期，需要重新选择或重试",
};

/** 附件种类的展示文案。 */
const ATTACHMENT_KIND_LABELS: Readonly<Record<AttachmentKind, string>> = {
  image: "图片",
  audio: "音频",
  file: "文件",
};

/** 上传进度百分比（十进制字符串，不经浮点累加）；总长为零时返回 `null`。 */
function uploadPercent(uploaded: string, total: string): number | null {
  let uploadedBytes: bigint;
  let totalBytes: bigint;
  try {
    uploadedBytes = BigInt(uploaded);
    totalBytes = BigInt(total);
  } catch {
    return null;
  }
  if (totalBytes === 0n) return null;
  const scaled = (uploadedBytes * 100n) / totalBytes;
  return Number(scaled > 100n ? 100n : scaled);
}

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
 * `submittable` 由调用方按模型的 `canSubmit`、「文字/附件/路径是否全空」与回调是否接线算出
 * ——判定只有这一处，组件里不再写第二个 `if`。
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
    attachmentBar: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.xs,
    },
    attachmentButton: {
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    attachmentButtonText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    attachmentList: {
      gap: spacing.xs,
    },
    attachmentItem: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    attachmentPreview: {
      width: 36,
      height: 36,
      borderRadius: radius.sm,
      objectFit: "cover",
    },
    attachmentName: {
      flexGrow: 1,
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    attachmentMeta: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    attachmentFailure: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    pathInput: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 120,
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
      backgroundColor: colors.panel,
    },
    pathRef: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
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

/** 一份已选附件的清单：名称、种类、大小、状态、进度与失败/取消入口。 */
function AttachmentList({
  attachments,
  onRemove,
  onRetry,
  onCancelUpload,
}: {
  readonly attachments: readonly DraftAttachment[];
  readonly onRemove: ((localId: string) => void) | undefined;
  readonly onRetry: (() => void) | undefined;
  readonly onCancelUpload: (() => void) | undefined;
}): ReactElement | null {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  if (attachments.length === 0) return null;
  const retryable = attachments.some(
    (attachment) =>
      attachment.status === "failed" || attachment.status === "cancelled" || attachment.status === "expired",
  );
  const uploading = attachments.some((attachment) => attachment.status === "uploading");

  return (
    <View style={styles.attachmentList} dataSet={{ attachmentList: "true", attachmentCount: String(attachments.length) }}>
      {attachments.map((attachment) => {
        const percent = uploadPercent(attachment.uploadedBytes, attachment.byteLength);
        return (
          <View
            key={attachment.localId}
            style={styles.attachmentItem}
            dataSet={{
              attachmentId: attachment.localId,
              attachmentKind: attachment.kind,
              attachmentStatus: attachment.status,
              ...(percent === null ? {} : { uploadPercent: String(percent) }),
            }}
          >
            {/* 预览只对图片有意义：音频/文件的 Blob 交给 `<img>` 会解码失败成破图；
                它们的名称、种类与大小由下面两行呈现。 */}
            {attachment.kind === "image" && attachment.previewUrl !== null ? (
              <img
                src={attachment.previewUrl}
                alt={attachment.fileName}
                data-attachment-preview={attachment.kind}
                style={{ width: 36, height: 36, borderRadius: 6, objectFit: "cover" }}
              />
            ) : null}
            <Text style={styles.attachmentName} dataSet={{ attachmentName: attachment.fileName }}>
              {attachment.fileName}
            </Text>
            <Text style={styles.attachmentMeta} dataSet={{ attachmentMeta: "true" }}>
              {`${ATTACHMENT_KIND_LABELS[attachment.kind]} · ${formatBytes(attachment.byteLength)} · ${ATTACHMENT_STATUS_LABELS[attachment.status]}`}
              {attachment.status === "uploading" && percent !== null ? ` ${String(percent)}%` : ""}
            </Text>
            {attachment.error === null ? null : (
              <Text style={styles.attachmentFailure} dataSet={{ attachmentError: "true" }}>
                {attachment.error}
              </Text>
            )}
            {onRemove === undefined ? null : (
              <Pressable
                role="button"
                style={styles.attachmentButton}
                dataSet={{ action: "remove-attachment", attachmentId: attachment.localId }}
                onPress={() => {
                  onRemove(attachment.localId);
                }}
              >
                <Text style={styles.attachmentButtonText}>移除</Text>
              </Pressable>
            )}
          </View>
        );
      })}
      {retryable && onRetry !== undefined ? (
        <Pressable
          role="button"
          style={styles.attachmentButton}
          dataSet={{ action: "retry-attachments" }}
          onPress={onRetry}
        >
          <Text style={styles.attachmentButtonText}>重试失败的附件</Text>
        </Pressable>
      ) : null}
      {uploading && onCancelUpload !== undefined ? (
        <Pressable
          role="button"
          style={styles.attachmentButton}
          dataSet={{ action: "cancel-attachment-upload" }}
          onPress={onCancelUpload}
        >
          <Text style={styles.attachmentButtonText}>取消上传</Text>
        </Pressable>
      ) : null}
    </View>
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
  onPickAttachment,
  onPasteAttachments,
  onRemoveAttachment,
  onRetryAttachments,
  onCancelAttachmentUpload,
  onRemovePathRef,
}: ConversationComposerProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  // 三态都允许编辑草稿：`in_progress` 与 `unavailable` 只关掉提交闸门，不关输入框。
  const [dragging, setDragging] = useState(false);
  const attachments = input.attachments.attachments;
  const hasUnsent = input.draft.trim().length > 0 || attachments.length > 0 || input.pathRefs.length > 0;
  // 提交闸门只有这一处：模型的 `canSubmit`（执行中 / 连接离线 / imported 来源 / 附件未就绪都在
  // 模型里变成假）加上「有可提交内容」与「回调已接线」。可提交内容与仓库 `submitPrompt` 同源：
  // 文字、附件、路径引用三者有任一非空即可——纯附件消息不能被 UI 单独挡死。
  const submittable = input.canSubmit && hasUnsent;

  // 有未发送内容时才提示：刷新/关闭后不恢复（不承诺草稿与上传续传）。
  useEffect(() => {
    if (!hasUnsent || typeof window === "undefined") return;
    const handler = (event: BeforeUnloadEvent): void => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", handler);
    return () => {
      window.removeEventListener("beforeunload", handler);
    };
  }, [hasUnsent]);

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
      <View style={styles.bar} dataSet={{ composerBar: "true" }}>
        <Pressable role="button" aria-label={input.attachmentsSupported ? "添加文件" : `添加文件不可用：${input.attachmentNotice}`}
          disabled={!input.attachmentsSupported || onPickAttachment === undefined}
          style={[styles.attachmentButton, { marginRight: "auto", paddingVertical: 6 }]}
          dataSet={{ action: "pick-attachment", attachmentKind: "file", attachmentsSupported: String(input.attachmentsSupported) }}
          onPress={() => onPickAttachment?.("file")}>
          <Text style={styles.attachmentButtonText}>＋ 添加</Text>
        </Pressable>
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

      <AttachmentList
        attachments={attachments}
        onRemove={onRemoveAttachment}
        onRetry={onRetryAttachments}
        onCancelUpload={onCancelAttachmentUpload}
      />

      {input.pathRefs.length === 0 ? null : (
        <View style={styles.attachmentList} dataSet={{ pathRefList: "true" }}>
          {input.pathRefs.map((path) => (
            <View key={path} style={styles.pathRef} dataSet={{ pathRef: path }}>
              <Text style={styles.attachmentMeta} dataSet={{ pathRefText: "true" }}>
                {`工作目录路径：${path}`}
              </Text>
              {onRemovePathRef === undefined ? null : (
                <Pressable
                  role="button"
                  style={styles.attachmentButton}
                  dataSet={{ action: "remove-path-ref", pathRef: path }}
                  onPress={() => {
                    onRemovePathRef(path);
                  }}
                >
                  <Text style={styles.attachmentButtonText}>移除</Text>
                </Pressable>
              )}
            </View>
          ))}
        </View>
      )}

      <div data-composer-drop-zone="true"
        onDragOver={(event) => {
          if (!event.dataTransfer.types.includes("Files")) return;
          event.preventDefault();
          const enabled = input.attachmentsSupported && onPasteAttachments !== undefined;
          event.dataTransfer.dropEffect = enabled ? "copy" : "none";
          setDragging(enabled);
        }}
        onDragLeave={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDragging(false);
        }}
        onDrop={(event) => {
          event.preventDefault(); setDragging(false);
          if (!input.attachmentsSupported || onPasteAttachments === undefined || event.dataTransfer.files.length === 0) return;
          onPasteAttachments(event.nativeEvent);
        }}
        onPaste={(event) => {
          if (!input.attachmentsSupported || onPasteAttachments === undefined || event.clipboardData.files.length === 0) return;
          event.preventDefault(); onPasteAttachments(event.nativeEvent);
        }}>
      <View style={[styles.inputRow, dragging ? { borderColor: palettes[theme.dark ? "dark" : "light"].accent } : {}]}>
        <TextInput
          value={input.draft}
          multiline
          placeholder={STATE_PLACEHOLDERS[input.state]}
          style={styles.input}
          dataSet={{ composerInput: input.state }}
          onChangeText={(text: string) => {
            onDraftChange?.(text);
          }}
          onKeyPress={onKeyPress}
        />

        {/* 发送按钮始终呈现，是否可点只由闸门决定（执行中/不可用禁用并说明原因，不排队）。 */}
        <Pressable
          role="button"
          // 闸门是模型给的 `canSubmit`：执行中 / 离线 / imported 来源 / 附件未就绪都会在这里
          // 变为不可点（仓库侧 `submitPrompt` 还会再拦一次，不靠组件记得禁用）。
          disabled={!submittable || onSubmit === undefined}
          style={!submittable || onSubmit === undefined ? [styles.primary, styles.buttonDisabled] : styles.primary}
          dataSet={{ action: "submit-prompt", submitEnabled: String(submittable && onSubmit !== undefined) }}
          onPress={() => {
            onSubmit?.(input.draft);
          }}
        >
          <Text style={submittable ? styles.primaryText : [styles.primaryText, styles.buttonDisabledText]}>发送</Text>
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
            <Text style={styles.stopText}>■ 停止</Text>
          </Pressable>
        ) : null}
      </View>

      </div>

      {input.canSubmit ? null : (
        <Text style={styles.note} dataSet={{ submitUnavailable: "true" }}>
          {input.disabledReason ?? "当前不能提交。"}
        </Text>
      )}

      {["failed", "uncertain", "pending_confirmation"].includes(input.submission.kind) ? <SubmissionLine submission={input.submission} /> : null}
      {/* 没有可取消轮次时不占一行：原型也没有这条静息文案，窄高度下它会挤掉时间线。 */}
      {input.cancel.kind === "requested" || (input.cancel.kind === "available" && input.cancel.failure !== null) ? <CancelLine cancel={input.cancel} /> : null}

    </View>
  );
}
