/**
 * 附件的**选择与上传状态机**（`design.md` D3；spec「上传完成后整体提交」「无附件产品额度与有界传输」）。
 *
 * ## 两个状态机刻意分开
 *
 * - `AttachmentUploadPhase`：`selected → uploading → sealed`，旁路 `failed` / `cancelled` / `expired`。
 * - 业务提交状态：仍由 `conversation-input-model.ts` 从**服务端命令终态**推导。
 *
 * 上传成功**不等于**消息被受理，更不等于 Agent 已收到内容；本模块因此不提供任何「上传完就把
 * 输入标为已发送」的字段。只有「全部附件封存」这一条会让 `canSubmit` 为真。
 *
 * ## 能力门控
 *
 * 端到端协商到 `core.attachment-transfer.v1` 才可能出现附件；某一种类是否可用取决于 Agent
 * 宣告的 prompt 能力（`promptCapabilities.image/audio/embeddedContext`）。能力**未知**时给出
 * 明确提示（服务端会在接受前再校验），而不是假装支持或静默删掉附件。
 *
 * ## 产品额度不存在
 *
 * 这里没有附件数量/单文件/总量上限；只有「全部封存才能提交」「失败保留完整草稿」与展示进度。
 */

import type { DecimalString } from "../protocol/common";

/** 附件种类（与 `mediaReference.kind` 同义）。 */
export type AttachmentKind = "image" | "audio" | "file";

/** 选择阶段只存在内存；上传期间按块推进。 */
export type AttachmentUploadStatus =
  | "selected"
  | "uploading"
  | "sealed"
  | "failed"
  | "cancelled"
  | "expired";

/** 一份待发送附件的公开状态（平台句柄不进这里）。 */
export interface DraftAttachment {
  /** 页面内稳定标识（与 `transferId` 同值即可；重试复用）。 */
  readonly localId: string;
  readonly transferId: string;
  /** 封存后服务端返回的内容寻址标识；未封存为 `null`。 */
  readonly attachmentId: string | null;
  readonly kind: AttachmentKind;
  readonly fileName: string;
  readonly mediaType: string;
  readonly byteLength: DecimalString;
  readonly status: AttachmentUploadStatus;
  /** 已确认落盘的字节数（十进制字符串）。 */
  readonly uploadedBytes: DecimalString;
  /** 封存后的整段摘要；未封存为 `null`。 */
  readonly sha256: string | null;
  /** 失败原因（面向用户的一句话）；其余状态为 `null`。 */
  readonly error: string | null;
  /** 选择阶段的本机预览对象 URL（仅内存；移除时立即撤销）。 */
  readonly previewUrl: string | null;
}

/** 附件能力的判定结果。 */
export type AttachmentCapability =
  | { readonly kind: "supported" }
  | { readonly kind: "unsupported"; readonly reason: string }
  | { readonly kind: "unknown"; readonly notice: string };

/** 一次发送尝试的上传状态（与业务命令记录分离）。 */
export interface AttachmentBatchModel {
  readonly attachments: readonly DraftAttachment[];
  readonly phase: "empty" | "selected" | "uploading" | "sealed" | "failed";
  /** 全部附件封存且工作目录路径引用合法时才能提交。 */
  readonly canSubmit: boolean;
  /** 不能提交时的明确原因；可提交为 `null`。 */
  readonly blockedReason: string | null;
  /** 能力未知/不支持的提示；没有提示为 `null`。 */
  readonly capabilityNotice: string | null;
  /** 上传进度（字节）：只用于呈现，不参与闸门。 */
  readonly uploadedBytes: DecimalString;
  readonly totalBytes: DecimalString;
  /** 当前上传尝试的错误；没有为 `null`。 */
  readonly error: string | null;
}

/** 工作目录路径引用是否可以被客户端接受（服务端还会再校验。 */
export function isWorkspacePathRef(value: string): boolean {
  if (value.length === 0 || value.length > 4096) return false;
  if (value.startsWith("/") || value.startsWith("\\")) return false;
  // 盘符、UNC 与 ADS 的冒号一律拒绝；控制字符同样拒绝（与协议 `promptContentBlock` 同口径）。
  if (/[\u0000-\u001f:]/.test(value)) return false;
  const parts = value.replaceAll("\\", "/").split("/");
  return parts.every((part) => part.length > 0 && part !== "." && part !== "..");
}

/**
 * Markdown 行内代码 → 可点文件路径的**窄**判据。
 *
 * `isWorkspacePathRef` 只回答「可不可以作为上传前的路径引用」，它放行一切合法相对路径形状，
 * 因此 `npm test`、`true`、`useState` 也会通过。行内代码大多数是标识符或命令：把它们都画成
 * 文件链接，点击就会切到文件查看并读取一个不存在的路径。这里在形状校验之上再要求
 * **无空白**，且**含目录分隔符 `/` 或带扩展名**——这两条覆盖了用户真正想联动查看的文件名，
 * 同时把命令与普通标识符留在纯文本。
 */
export function isMarkdownFilePath(value: string): boolean {
  if (!isWorkspacePathRef(value)) return false;
  if (/\s/.test(value)) return false;
  return value.includes("/") || /\.[A-Za-z0-9]+$/.test(value);
}

/**
 * 单一种类的能力判定。
 *
 * `capabilities` 是 `session.read` 的 `agentCapabilities`（开放对象）。路径：
 * `promptCapabilities.image` / `promptCapabilities.audio` / `promptCapabilities.embeddedContext`。
 * 形状缺失或取值不是布尔时返回 `unknown`——客户端不把「没读到」说成「不支持」。
 */
export function attachmentCapabilityOf(
  kind: AttachmentKind,
  capabilities: Readonly<Record<string, unknown>> | null,
): AttachmentCapability {
  if (capabilities === null) {
    return {
      kind: "unknown",
      notice: "尚未取得这个 Agent 的输入能力声明；发送时服务端会再校验，可能整条被拒绝。",
    };
  }
  const promptCapabilities = capabilities["promptCapabilities"];
  if (typeof promptCapabilities !== "object" || promptCapabilities === null) {
    return {
      kind: "unknown",
      notice: "这个 Agent 没有宣告 prompt 输入能力；发送时服务端会再校验。",
    };
  }
  const key = kind === "file" ? "embeddedContext" : kind;
  const declared: unknown = Reflect.get(promptCapabilities, key);
  if (typeof declared !== "boolean") {
    return {
      kind: "unknown",
      notice: `这个 Agent 没有明确宣告 ${kind === "file" ? "文件/嵌入内容" : kind === "image" ? "图片" : "音频"} 输入能力；发送时服务端会再校验。`,
    };
  }
  if (!declared) {
    return {
      kind: "unsupported",
      reason: `这个 Agent 未宣告 ${kind === "file" ? "文件/嵌入内容" : kind === "image" ? "图片" : "音频"} 输入能力，不能发送该附件。`,
    };
  }
  return { kind: "supported" };
}

/**
 * 计算发送闸门与进度。
 *
 * `capabilities` 对每种附件分别判定；任何一个 **unsupported** 都会阻止整条提交（且保留草稿），
 * 不会「删掉不支持的附件只发文字」（spec MUST NOT）。
 */
export function buildAttachmentBatch(input: {
  readonly attachments: readonly DraftAttachment[];
  readonly capabilityOf: (kind: AttachmentKind) => AttachmentCapability;
  readonly pathRefs?: readonly string[];
}): AttachmentBatchModel {
  let uploaded = 0n;
  let total = 0n;
  const failures: string[] = [];
  const unsupported: string[] = [];
  const notices = new Set<string>();

  for (const attachment of input.attachments) {
    try {
      total += BigInt(attachment.byteLength);
      uploaded += BigInt(attachment.uploadedBytes);
    } catch {
      failures.push(`${attachment.fileName} 的长度不是合法数字。`);
    }
    if (attachment.status === "failed") failures.push(attachment.error ?? `${attachment.fileName} 上传失败。`);
    if (attachment.status === "cancelled") failures.push(`${attachment.fileName} 已取消。`);
    if (attachment.status === "expired") failures.push(`${attachment.fileName} 的临时上传已过期，需要重新选择。`);
    const capability = input.capabilityOf(attachment.kind);
    if (capability.kind === "unsupported") unsupported.push(capability.reason);
    if (capability.kind === "unknown") notices.add(capability.notice);
  }

  for (const path of input.pathRefs ?? []) {
    if (!isWorkspacePathRef(path)) failures.push(`「${path}」不是当前工作目录内的合法相对路径。`);
  }

  const uploading = input.attachments.some((attachment) => attachment.status === "uploading");
  const allSealed = input.attachments.every((attachment) => attachment.status === "sealed");
  const phase: AttachmentBatchModel["phase"] =
    input.attachments.length === 0
      ? "empty"
      : uploading
        ? "uploading"
        : failures.length > 0
          ? "failed"
          : allSealed
            ? "sealed"
            : "selected";

  let blockedReason: string | null = null;
  if (unsupported.length > 0) blockedReason = unsupported[0] ?? "当前端到端能力不支持选中的附件。";
  else if (failures.length > 0) blockedReason = failures[0] ?? "有附件上传失败。";
  else if (uploading) blockedReason = "附件正在上传，全部封存后才能提交，Agent 尚未收到任何内容。";
  // `selected` 是**可以点发送**的状态：点击后才开始上传（spec「发送后才上传」）。
  // 全部封存后同样可提交；只有上传中、失败与能力不支持会挡住发送。

  return {
    attachments: input.attachments,
    phase,
    canSubmit:
      input.attachments.length === 0
        ? true
        : blockedReason === null && (allSealed || phase === "selected"),
    blockedReason,
    capabilityNotice: notices.size === 0 ? null : [...notices].join(" "),
    uploadedBytes: uploaded.toString(),
    totalBytes: total.toString(),
    error: failures[0] ?? unsupported[0] ?? null,
  };
}

/** 把一次已确认的块推进到附件状态（`duplicate` 只影响呈现，不重复计数）。 */
export function withUploadedBytes(
  attachment: DraftAttachment,
  nextOffset: DecimalString,
  sealed?: { readonly sha256: string; readonly attachmentId: string },
): DraftAttachment {
  return {
    ...attachment,
    uploadedBytes: nextOffset,
    status: sealed === undefined ? attachment.status : "sealed",
    ...(sealed === undefined ? {} : { sha256: sealed.sha256, attachmentId: sealed.attachmentId }),
  };
}
