/**
 * 输入区与提交/取消的呈现模型（`design.md` D6；spec「输入与提交只支持文本且以终态为准」
 * 「取消当前轮次以终态为准」）。
 *
 * ## 三条硬规则
 *
 * 1. **提交不等于完成**：命令在服务端**终态**到达之前只能是 `pending_confirmation`
 *    （「已发出，等待服务器确认」）。`accepted` 需要服务端的 `command.result {status:"accepted"}`
 *    或 `command.status` 的 `accepted`——本模块不提供任何「本地先当成已发送」的分支。
 * 2. **取消同理**：用户点了取消只得到 `requested`，`confirmed` 只能由终态产生，
 *    未确认前 MUST NOT 呈现为空闲。
 * 3. **已关闭的会话不可提交且只读；执行中可编辑但不可提交**：`state: "closed"` 时
 *    `canSubmit` 恒假、`draftEditable` 也为假；`in_progress` 时 `canSubmit` 恒假但
 *    `draftEditable` 为真（写好下一条草稿，不建立队列）。提交闸门在
 *    `ClientStore.submitPrompt` 里再拦一次（不派发命令），不靠组件记得禁用。
 *
 * ## 草稿与附件
 *
 * 附件按端到端协商结果可用：只有协商到 `core.attachment-transfer.v1` 且平台端口已装配时
 * `attachmentsSupported` 才为真，并给出一句**显式**说明（支持与不支持都说清楚，
 * spec MUST NOT 静默丢弃附件后提交剩余文本）。来源为其它节点的会话，草稿只在内存保留并标注
 * 未提交（`no-content-cache` 边界，D10）。
 */

import type { CommandRecord } from "../state";
import { isTerminalState } from "../state";
import type { AttachmentBatchModel } from "./attachment-model";

/** 输入区的三种状态（spec 的「可输入 / 进行中 / 已关闭」）。 */
export type ConversationInputState = "editable" | "in_progress" | "closed";

/** 提交的呈现（终态之前只能是待确认）。 */
export type SubmissionPresentation =
  | { readonly kind: "none" }
  | { readonly kind: "pending_confirmation"; readonly requestId: string }
  | { readonly kind: "accepted"; readonly acceptedAt: string | null }
  | { readonly kind: "completed" }
  | { readonly kind: "failed"; readonly detail: string }
  | { readonly kind: "uncertain"; readonly detail: string };

/** 取消的呈现（未收到终态前 MUST NOT 呈现为空闲）。 */
export type CancelPresentation =
  | { readonly kind: "unavailable" }
  | { readonly kind: "available"; readonly failure: string | null }
  | { readonly kind: "requested"; readonly requestId: string }
  | { readonly kind: "confirmed" };

/** 输入区模型。 */
export interface ConversationInputModel {
  readonly state: ConversationInputState;
  /** 当前草稿原文（只在内存；提交后由调用方清空）。 */
  readonly draft: string;
  /**
   * 输入框是否接受编辑。
   *
   * 与 `state` 分开：执行中仍可编辑下一条草稿（可先写好，轮次结束后发送），但 `canSubmit`
   * 恒假、不排队；只有已关闭的会话才只读。
   */
  readonly draftEditable: boolean;
  /** 能否提交（唯一决定提交按钮可用性的字段）。 */
  readonly canSubmit: boolean;
  /** 不可提交的原因；可提交时为 `null`。 */
  readonly disabledReason: string | null;
  /** 当前连接是否协商到附件传输。 */
  readonly attachmentsSupported: boolean;
  /** 附件/工作目录路径的显式说明（非空：支持与不支持都说清楚）。 */
  readonly attachmentNotice: string;
  /** 已选择的附件与它们各自的上传状态（选择只在内存，发送后才上传）。 */
  readonly attachments: AttachmentBatchModel;
  /** 已添加的工作目录路径引用（相对路径；服务端再校验边界与链接逃逸）。 */
  readonly pathRefs: readonly string[];
  /** 草稿是否只在内存（来源为其它节点的会话）。 */
  readonly draftInMemoryOnly: boolean;
  /** 草稿未提交/不落盘的标注；无需标注时为 `null`。 */
  readonly draftNotice: string | null;
  readonly submission: SubmissionPresentation;
  readonly cancel: CancelPresentation;
}

/** 命令失败的一句话说明（结构化：码 + 消息）。 */
function failureDetail(record: CommandRecord): string {
  const error = record.error;
  if (error === null) return `命令未成功（${record.state}）`;
  return `${error.code}：${error.message}`;
}

/** 提交的呈现：只由命令记录的**服务端状态**推得，没有任何本地乐观分支。 */
function submissionOf(record: CommandRecord | null): SubmissionPresentation {
  if (record === null) return { kind: "none" };
  switch (record.state) {
    case "draft":
    case "submitting":
      return { kind: "pending_confirmation", requestId: record.requestId };
    case "accepted":
      return { kind: "accepted", acceptedAt: record.acceptedAt };
    case "completed":
      return { kind: "completed" };
    case "uncertain":
      return { kind: "uncertain", detail: failureDetail(record) };
    default:
      return { kind: "failed", detail: failureDetail(record) };
  }
}

/** 取消的呈现：终态之前是 `requested`；失败后回到可取消（并带上失败说明），而不是假装成功。 */
function cancelOf(record: CommandRecord | null, canCancel: boolean): CancelPresentation {
  if (record === null) return canCancel ? { kind: "available", failure: null } : { kind: "unavailable" };
  if (record.state === "completed") return { kind: "confirmed" };
  // `uncertain` 是服务端说「不知道有没有生效」：那时继续呈现待确认比放开重提更诚实。
  if (!isTerminalState(record.state) || record.state === "uncertain") {
    return { kind: "requested", requestId: record.requestId };
  }
  return canCancel ? { kind: "available", failure: failureDetail(record) } : { kind: "unavailable" };
}

/**
 * 构造输入区模型。
 *
 * @param sessionOpen 会话是否未关闭（`state === "closed"` 时为假）。
 * @param connectionOnline 连接是否在线（离线时无法把任何输入标记为已提交）。
 * @param originLocal 会话是否本节点自有（imported 会话不可提交，R20）。
 * @param turnInProgress 是否有进行中的轮次（决定「进行中」这一态）。
 * @param canCancel 当前轮次是否可取消（由时间线上最新一条轮次卡片决定）。
 * @param promptCommand 最近一次提示提交的命令记录。
 * @param cancelCommand 最近一次取消命令的记录。
 * @param draft 当前草稿原文（只在内存）。
 */
export function buildConversationInputModel(input: {
  readonly sessionOpen: boolean;
  readonly connectionOnline: boolean;
  readonly originLocal: boolean;
  readonly turnInProgress: boolean;
  readonly canCancel: boolean;
  readonly promptCommand: CommandRecord | null;
  readonly cancelCommand: CommandRecord | null;
  readonly draft?: string;
  /** 是否协商到附件传输；缺省 `false`（与旧调用点等价）。 */
  readonly attachmentsNegotiated?: boolean;
  /** 已选附件与上传状态的模型；缺省为空批。 */
  readonly attachments?: AttachmentBatchModel;
  /** 已添加的工作目录路径引用。 */
  readonly pathRefs?: readonly string[];
}): ConversationInputModel {
  const submission = submissionOf(input.promptCommand);
  const awaitingTerminal = submission.kind === "pending_confirmation";
  const state: ConversationInputState = !input.sessionOpen
    ? "closed"
    : awaitingTerminal || input.turnInProgress
      ? "in_progress"
      : "editable";
  const attachments = input.attachments ?? EMPTY_ATTACHMENTS;
  const uploading = attachments.phase === "uploading";

  const disabledReason = !input.sessionOpen
    ? "会话已关闭，不能提交"
    : !input.connectionOnline
      ? "连接不可用，不能提交"
      : !input.originLocal
        ? "来源为其它节点的会话不能提交（本设备只承载元数据）"
        : awaitingTerminal || input.turnInProgress
          ? "会话进行中，完成后可继续输入"
          : uploading
            ? "附件正在上传：全部封存后才能提交，Agent 尚未收到任何内容"
            : attachments.blockedReason;

  return {
    state,
    draft: input.draft ?? "",
    draftEditable: input.sessionOpen,
    // 提交只发生在 `editable`：进行中（轮次运行或等待命令终态）与已关闭一视同仁。
    // 附件未全部封存时同样不可提交；上传失败保留完整草稿。
    canSubmit: state === "editable" && disabledReason === null,
    disabledReason,
    attachmentsSupported: input.attachmentsNegotiated === true,
    attachmentNotice: attachmentNoticeOf(input.attachmentsNegotiated === true, attachments),
    attachments,
    pathRefs: input.pathRefs ?? [],
    draftInMemoryOnly: !input.originLocal,
    draftNotice: input.originLocal
      ? null
      : "草稿只在内存中保留、尚未提交：来源为其它节点的会话不落盘。",
    submission,
    cancel: cancelOf(input.cancelCommand, input.canCancel),
  };
}

/** 空批：未选择任何附件。 */
const EMPTY_ATTACHMENTS: AttachmentBatchModel = {
  attachments: [],
  phase: "empty",
  canSubmit: true,
  blockedReason: null,
  capabilityNotice: null,
  uploadedBytes: "0",
  totalBytes: "0",
  error: null,
};

/** 附件区的显式说明：支持与不支持都必须说清楚（不得静默删附件后只发文字）。 */
function attachmentNoticeOf(negotiated: boolean, batch: AttachmentBatchModel): string {
  if (!negotiated) {
    return "当前连接不支持附件传输（未协商 core.attachment-transfer.v1）：只能发送文本。";
  }
  if (batch.error !== null) return `附件不可用：${batch.error}`;
  if (batch.capabilityNotice !== null) return batch.capabilityNotice;
  if (batch.attachments.length > 0) {
    return `已选择 ${String(batch.attachments.length)} 个附件；点击发送后才会上传，全部封存后整条提交。`;
  }
  return "可添加图片、文件或已有音频，也可引用当前工作目录内的相对路径；点击发送后才会上传。";
}
