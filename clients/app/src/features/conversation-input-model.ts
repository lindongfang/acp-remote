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
 * 3. **关闭或进行中的会话不可提交**：`state: "closed"` / `"in_progress"` 时 `canSubmit` 恒假并
 *    给出原因；提交闸门在 `ClientStore.submitPrompt` 里再拦一次（不派发命令），不靠组件记得禁用。
 *
 * ## 草稿与附件
 *
 * 附件在 v1 不受支持，因此模型里 `attachmentsSupported` 恒为 `false`，并给出一句**显式**说明
 * （spec MUST NOT 静默丢弃附件后提交剩余文本）。来源为其它节点的会话，草稿只在内存保留并标注
 * 未提交（`no-content-cache` 边界，D10）。
 */

import type { CommandRecord } from "../state";
import { isTerminalState } from "../state";

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
  /** 能否提交（唯一决定提交按钮可用性的字段）。 */
  readonly canSubmit: boolean;
  /** 不可提交的原因；可提交时为 `null`。 */
  readonly disabledReason: string | null;
  /** v1 恒为 `false`：附件不受支持。 */
  readonly attachmentsSupported: boolean;
  /** 附件不受支持的显式说明（恒非空）。 */
  readonly attachmentNotice: string;
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
}): ConversationInputModel {
  const submission = submissionOf(input.promptCommand);
  const awaitingTerminal = submission.kind === "pending_confirmation";
  const state: ConversationInputState = !input.sessionOpen
    ? "closed"
    : awaitingTerminal || input.turnInProgress
      ? "in_progress"
      : "editable";

  const disabledReason = !input.sessionOpen
    ? "会话已关闭，不能提交"
    : !input.connectionOnline
      ? "连接不可用，不能提交"
      : !input.originLocal
        ? "来源为其它节点的会话不能提交（本设备只承载元数据）"
        : state === "in_progress"
          ? "会话进行中，完成后可继续输入"
          : null;

  return {
    state,
    draft: input.draft ?? "",
    // 提交只发生在 `editable`：进行中（轮次运行或等待命令终态）与已关闭一视同仁。
    canSubmit: state === "editable" && disabledReason === null,
    disabledReason,
    attachmentsSupported: false,
    attachmentNotice: "📎 附件暂不支持 · 仅文本",
    draftInMemoryOnly: !input.originLocal,
    draftNotice: input.originLocal
      ? null
      : "草稿只在内存中保留、尚未提交：来源为其它节点的会话不落盘。",
    submission,
    cancel: cancelOf(input.cancelCommand, input.canCancel),
  };
}
