/**
 * 输入区与提交/取消的呈现模型（`design.md` D5/D6；spec「输入与提交支持协商内容且以终态为准」
 * 「取消当前轮次以终态为准」）。
 *
 * ## 三条硬规则
 *
 * 1. **提交不等于完成**：命令在服务端**终态**到达之前只能是 `pending_confirmation`
 *    （「已发出，等待服务器确认」）。`accepted` 需要服务端的 `command.result {status:"accepted"}`
 *    或 `command.status` 的 `accepted`——本模块不提供任何「本地先当成已发送」的分支。
 * 2. **取消同理**：用户点了取消只得到 `requested`，`confirmed` 只能由终态产生，
 *    未确认前 MUST NOT 呈现为空闲。
 * 3. **输入可用性与会话主状态无关**：`idle`/`running`/`pending` 都不推导只读，也不存在
 *    「会话已关闭所以只读」的分支。三态都允许编辑草稿；`in_progress`（轮次执行中或提交等待
 *    终态）与 `unavailable`（设备离线、来源离线、imported 来源、附件能力未协商等）只是当前不能提交，
 *    并各自给出**当前实际原因**（`blockedBy` + 一句话说明）。设备离线时说明交给连接问题弹层，
 *    输入区只关闭提交闸门、不重复原因文案。提交闸门在
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

/**
 * 输入区的三种可用性：可发送 / 执行中 / 不可用。
 *
 * 这是**当前能不能发**的三态，不是会话状态：三种取值都允许编辑草稿。
 */
export type ConversationInputState = "editable" | "in_progress" | "unavailable";

/** 当前不可提交的**实际原因**（`editable` 时为 `null`）。每条都来自当前事实，不来自主状态。 */
export type ConversationInputBlock =
  /** 设备与电脑的连接不可用（未连上/已断开）。提交闸门关闭，但不额外堆一句原因文案。 */
  | "connection_offline"
  /** 设备连接在线，但来源节点离线（imported 会话）。 */
  | "source_offline"
  /** 上一次提交还在等服务端终态。 */
  | "submission_in_flight"
  /** 本轮的 Agent 执行仍在进行。 */
  | "turn_in_progress"
  /** 已选附件，但当前连接未协商附件传输。 */
  | "attachments_not_negotiated"
  /** 已选附件还在上传。 */
  | "attachment_uploading"
  /** 附件/路径引用被能力或失败挡住。 */
  | "attachment_unavailable";

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
  /** 当前不可提交的原因；可提交时为 `null`（与 `state !== "editable"` 一一对应）。 */
  readonly blockedBy: ConversationInputBlock | null;
  /** 当前草稿原文（只在内存；提交后由调用方清空）。 */
  readonly draft: string;
  /** 能否提交（唯一决定提交按钮可用性的字段）。 */
  readonly canSubmit: boolean;
  /** 不可提交的说明；可提交时为 `null`（设备离线也交给连接问题弹层，这里同样为 `null`）。 */
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

/** 当前挡住提交的原因（`null` = 可提交）。 */
function blockOf(input: {
  readonly connectionOnline: boolean;
  readonly originOnline: boolean;
  readonly awaitingTerminal: boolean;
  readonly turnInProgress: boolean;
  readonly attachmentsNegotiated: boolean;
  readonly attachments: AttachmentBatchModel;
}): ConversationInputBlock | null {
  if (!input.connectionOnline) return "connection_offline";
  if (!input.originOnline) return "source_offline";
  if (input.awaitingTerminal) return "submission_in_flight";
  if (input.turnInProgress) return "turn_in_progress";
  // 已选了附件、却没协商到附件传输：整条不能发（不得静默删掉附件只发文本）。
  if (!input.attachmentsNegotiated && input.attachments.attachments.length > 0) {
    return "attachments_not_negotiated";
  }
  if (input.attachments.phase === "uploading") return "attachment_uploading";
  if (input.attachments.blockedReason !== null) return "attachment_unavailable";
  return null;
}

/** 原因的界面说明（每条都描述**当前事实**，不引用会话主状态）。 */
function blockText(block: ConversationInputBlock, attachments: AttachmentBatchModel): string | null {
  switch (block) {
    case "connection_offline":
      // 设备离线已经由连接问题弹层完整说明；输入区不再重复一句「连接不可用」。
      return null;
    case "source_offline":
      return "来源节点离线，不能提交";
    case "submission_in_flight":
      return "消息已提交，等待服务端确认后才能继续发送";
    case "turn_in_progress":
      return "会话进行中，完成后可继续输入";
    case "attachments_not_negotiated":
      return "当前连接未协商 core.attachment-transfer.v1，已选附件无法上传；移除附件后才能发送文本";
    case "attachment_uploading":
      return "附件正在上传：全部封存后才能提交，Agent 尚未收到任何内容";
    case "attachment_unavailable":
      return attachments.blockedReason ?? "有附件或路径引用不能提交";
  }
}

/**
 * 构造输入区模型。
 *
 * @param connectionOnline 连接是否在线（离线时无法把任何输入标记为已提交）。
 * @param originLocal 会话是否本节点自有（远程会话的草稿只留内存）。
 * @param originOnline 来源是否可达；本节点自有会话恒为真，远程会话取来源状态。
 * @param turnInProgress 是否有进行中的轮次（只决定「执行中」，不决定只读）。
 * @param canCancel 当前轮次是否可取消（有活动轮次且连接可用；与主状态无关）。
 * @param promptCommand 最近一次提示提交的命令记录。
 * @param cancelCommand 最近一次取消命令的记录。
 * @param draft 当前草稿原文（只在内存）。
 */
export function buildConversationInputModel(input: {
  readonly connectionOnline: boolean;
  readonly originLocal: boolean;
  /** 来源可达性；缺省为真（本节点自有会话）。 */
  readonly originOnline?: boolean;
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
  const attachments = input.attachments ?? EMPTY_ATTACHMENTS;
  const blockedBy = blockOf({
    connectionOnline: input.connectionOnline,
    originOnline: input.originOnline ?? true,
    awaitingTerminal: submission.kind === "pending_confirmation",
    turnInProgress: input.turnInProgress,
    attachmentsNegotiated: input.attachmentsNegotiated === true,
    attachments,
  });
  const state: ConversationInputState =
    blockedBy === null
      ? "editable"
      : blockedBy === "submission_in_flight" || blockedBy === "turn_in_progress"
        ? "in_progress"
        : "unavailable";

  return {
    state,
    blockedBy,
    draft: input.draft ?? "",
    // 提交只发生在 `editable`：执行中与不可用都不派发命令。
    canSubmit: blockedBy === null,
    disabledReason: blockedBy === null ? null : blockText(blockedBy, attachments),
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
