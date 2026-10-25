/**
 * 附件传输的客户端状态机（`docs/SYNC_PROTOCOL.md` §3.4）。
 *
 * ## 它负责什么、不负责什么
 *
 * - **负责**：帧信封（`messageId`/`connectionId`/`connectionSequence`）、一次一块未确认的
 *   发送节流、偏移与长度用 **bigint/十进制字符串** 承载、重复块确认、偏移错误后的续传、
 *   流式摘要、清单分批封存、读取分块与超时。
 * - **不负责**：建立 socket、认证。附件连接是一条**独立的已认证 WSS**，由组合根用与业务
 *   连接相同的 challenge-response 建好，再把本类当帧编解码器接上去。因此本类可在 Node 里用
 *   假 `send` 完整单测，而无需真实网络。
 *
 * ## 为什么正文从不明文进内存
 *
 * `upload()` 一次只调用 `readChunk(offset, length)` 取一块（≤32 KiB），把同一块喂给
 * `Sha256` 后立即编码发送；不累积已发送块。内存工作集因此是常数级，与文件大小无关。
 *
 * ## 协商
 *
 * 连接未协商 `core.attachment-transfer.v1` 时，本类**拒绝开始任何传输**（`AttachmentTransferError`
 * 的 `negotiation_required`）。这不是「发了再说」的探测：旧 peer 收到附件帧会关闭连接。
 */

import type {
  AttachmentFrame,
  AttachmentFrameType,
  AttachmentTransferState,
  ErrorCode,
  PreparationAttachmentTarget,
} from "../protocol";
import {
  ATTACHMENT_CHUNK_SIZE,
  ATTACHMENT_FEATURE,
  ATTACHMENT_MANIFEST_PAGE_SIZE,
  ATTACHMENT_MAX_UNACKNOWLEDGED_CHUNKS,
  ATTACHMENT_READ_MAX_LENGTH,
  decodeAttachmentChunk,
  decodeAttachmentFrame,
  encodeAttachmentChunk,
} from "../protocol";
import type { CancelTimer, ClockPort } from "./ports";
import { Sha256 } from "./sha256";

/** 单条连接上允许同时存在的活动传输上限（由服务端 budget 决定，客户端照此排队）。 */
export const ATTACHMENT_DEVICE_CONCURRENCY = ATTACHMENT_MAX_UNACKNOWLEDGED_CHUNKS * 2;

/**
 * 清单摘要：`SHA-256(Σ u32be(len(transferId)) || transferId)`，无填充 base64url。
 * 与 `SYNC_PROTOCOL.md` §3.4 及服务端 `manifest_page`/`manifest_sealed` 的复算口径一致。
 */
export function manifestDigestOf(transferIds: readonly string[]): string {
  const hasher = new Sha256();
  const length = new Uint8Array(4);
  const view = new DataView(length.buffer);
  const encoder = new TextEncoder();
  for (const id of transferIds) {
    const bytes = encoder.encode(id);
    view.setUint32(0, bytes.length, false);
    hasher.update(length);
    hasher.update(bytes);
  }
  return encodeAttachmentChunk(hasher.digest());
}

/** 失败分类：与协议错误码同义，另加客户端侧的 `timeout` / `protocol`。 */
export type AttachmentTransferFailure =
  | "negotiation_required"
  | "timeout"
  | "protocol"
  | "offset_mismatch"
  | "chunk_conflict"
  | "digest_mismatch"
  | "binding_mismatch"
  | "transfer_not_found"
  | "budget_exhausted"
  | "revoked"
  | "expired"
  | "aborted"
  | "already_sealed"
  /** 准备运行期/代失效：先核实原 start，再 restore（D6.3）。 */
  | "preparation_stale"
  /** 首条已被正式受理：停止上传，restore 取关联，不重传。 */
  | "already_committed"
  /** 凭据所属存储已整体重建：旧材料彻底作废，回目录重新分配。 */
  | "store_rebuilt";

/** 结构化失败：`details` 原样来自 `attachment.error`（offset 等）。 */
export class AttachmentTransferError extends Error {
  readonly failure: AttachmentTransferFailure;
  readonly retryable: boolean;
  readonly details: Readonly<Record<string, unknown>>;
  /**
   * 服务端 `attachment.error.code` 原文（客户端侧失败为 `null`）。
   *
   * 仓库按这个码走 D6.3 的固定动作；只看 `failure` 会把 `preparation.*` 与同名的传输
   * 失败混在一起，也无法把 `details` 原样交给页面。
   */
  readonly code: ErrorCode | null;

  constructor(
    failure: AttachmentTransferFailure,
    message: string,
    retryable = false,
    details: Readonly<Record<string, unknown>> = {},
    code: ErrorCode | null = null,
  ) {
    super(message);
    this.name = "AttachmentTransferError";
    this.failure = failure;
    this.retryable = retryable;
    this.details = details;
    this.code = code;
  }
}

/** 上传来源：正文只在 `readChunk` 里被读取，调用方不得整份缓存。 */
export interface AttachmentUploadSource {
  /** 总长度（十进制字符串；附件可超过 JS safe integer）。 */
  readonly totalLength: string;
  readonly mediaType: string;
  readonly fileName: string;
  /** 读取 `[offset, offset+length)`；返回的字节数可少于 `length`（最后一块）。 */
  readChunk(offset: bigint, length: number): Promise<Uint8Array>;
}

/** 封存结果（`attachment.sealed`）。 */
export interface AttachmentSealed {
  readonly transferId: string;
  /** 内容寻址后的附件标识；`session.prompt` 的 `attachment_ref` 用它。 */
  readonly attachmentId: string;
  readonly length: string;
  readonly sha256: string;
  readonly sealedAt: string;
}

/** 清单封存结果（`attachment.manifest_sealed`）。 */
export interface AttachmentManifestSealed {
  readonly submissionId: string;
  readonly attachmentCount: string;
  readonly manifestDigest: string;
  readonly sealedAt: string;
}

/** 读取的一块（`attachment.read_chunk`）。 */
export interface AttachmentReadChunk {
  readonly attachmentId: string;
  readonly sessionId: string;
  readonly offset: string;
  readonly length: string;
  readonly totalLength: string;
  readonly data: Uint8Array;
  readonly eof: boolean;
}

export interface AttachmentTransferOptions {
  readonly connectionId: string;
  /** 本连接协商到的 feature；必须包含 `core.attachment-transfer.v1`。 */
  readonly selectedFeatures: readonly string[];
  /** 发送一帧（组合根接到附件 socket 的 `send(JSON.stringify(frame))`）。 */
  readonly send: (frame: AttachmentFrame) => void;
  readonly uuid: () => string;
  readonly clock: ClockPort;
  /** 单块确认超时；缺省 30 秒（服务端关闭由 socket 关闭事件另行处理）。 */
  readonly chunkTimeoutMs?: number;
}

/** 一次传输的信道绑定（每条帧都重复它，不因连接复用而省略）。 */
export interface AttachmentBinding {
  readonly sessionId: string;
  readonly subjectId: string;
  /**
   * 准备目标（`design.md` D6.1）：出现时 `attachment.begin` 与
   * `attachment.manifest_begin` 走 `target` 分支，**不再**带顶层 `sessionId`。
   *
   * 首条提交之前的附件只能这样上传：没有正式会话行，也不能用空会话绕过外键。
   */
  readonly target?: PreparationAttachmentTarget;
}

interface Waiter<T> {
  readonly resolve: (value: T) => void;
  readonly reject: (error: AttachmentTransferError) => void;
  readonly cancel: CancelTimer;
}

/** 一个在途请求的等待槽：每种响应按标识各一条，避免慢响应配错请求。 */
interface Waiters {
  chunkAck: { readonly offset: string; readonly waiter: Waiter<{ duplicate: boolean; nextOffset: string }> } | undefined;
  sealed: Waiter<AttachmentSealed> | undefined;
  manifest: Waiter<AttachmentManifestSealed> | undefined;
  status:
    | Waiter<{
        state: AttachmentTransferState;
        receivedLength: string;
        nextOffset: string;
        sealedSha256: string | null;
      }>
    | undefined;
  read: Waiter<AttachmentReadChunk> | undefined;
}

function failureOfCode(code: string): AttachmentTransferFailure {
  switch (code) {
    case "attachment.negotiation_required":
      return "negotiation_required";
    case "attachment.offset_mismatch":
      return "offset_mismatch";
    case "attachment.chunk_conflict":
      return "chunk_conflict";
    case "attachment.digest_mismatch":
      return "digest_mismatch";
    case "attachment.binding_mismatch":
      return "binding_mismatch";
    case "attachment.transfer_not_found":
      return "transfer_not_found";
    case "attachment.budget_exhausted":
      return "budget_exhausted";
    case "attachment.revoked":
      return "revoked";
    case "attachment.expired":
      return "expired";
    case "attachment.sealed":
      return "already_sealed";
    case "preparation.stale":
      return "preparation_stale";
    case "preparation.already_committed":
      return "already_committed";
    case "preparation.store_rebuilt":
      return "store_rebuilt";
    default:
      return "protocol";
  }
}

export class AttachmentTransferClient {
  readonly #connectionId: string;
  readonly #features: readonly string[];
  readonly #send: (frame: AttachmentFrame) => void;
  readonly #uuid: () => string;
  readonly #clock: ClockPort;
  readonly #chunkTimeoutMs: number;
  #sequence = 0n;
  readonly #waiters = new Map<string, Waiters>();

  constructor(options: AttachmentTransferOptions) {
    this.#connectionId = options.connectionId;
    this.#features = options.selectedFeatures;
    this.#send = options.send;
    this.#uuid = options.uuid;
    this.#clock = options.clock;
    this.#chunkTimeoutMs = options.chunkTimeoutMs ?? 30_000;
  }

  /** 本连接是否协商到附件能力。未协商时所有传输入口都会拒绝。 */
  get negotiated(): boolean {
    return this.#features.includes(ATTACHMENT_FEATURE);
  }

  /** 处理一条入站帧；返回 `false` 表示帧形状不合法（调用方应关闭连接）。 */
  handleFrame(value: unknown): boolean {
    const decoded = decodeAttachmentFrame(value);
    if (!decoded.ok) return false;
    const frame = decoded.frame;
    switch (frame.type) {
      case "attachment.chunk_ack": {
        const waiters = this.#waiters.get(frame.body.transferId);
        const pending = waiters?.chunkAck;
        if (waiters === undefined || pending === undefined) return true;
        if (pending.offset !== frame.body.offset) return true;
        waiters.chunkAck = undefined;
        pending.waiter.cancel();
        pending.waiter.resolve({ duplicate: frame.body.duplicate, nextOffset: frame.body.nextOffset });
        return true;
      }
      case "attachment.sealed": {
        const waiters = this.#waiters.get(frame.body.transferId);
        const pending = waiters?.sealed;
        if (waiters === undefined || pending === undefined) return true;
        waiters.sealed = undefined;
        pending.cancel();
        pending.resolve({
          transferId: frame.body.transferId,
          attachmentId: frame.body.attachmentId,
          length: frame.body.length,
          sha256: frame.body.sha256,
          sealedAt: frame.body.sealedAt,
        });
        return true;
      }
      case "attachment.manifest_sealed": {
        const waiters = this.#waiters.get(frame.body.submissionId);
        const pending = waiters?.manifest;
        if (waiters === undefined || pending === undefined) return true;
        waiters.manifest = undefined;
        pending.cancel();
        pending.resolve({
          submissionId: frame.body.submissionId,
          attachmentCount: frame.body.attachmentCount,
          manifestDigest: frame.body.manifestDigest,
          sealedAt: frame.body.sealedAt,
        });
        return true;
      }
      case "attachment.status": {
        const waiters = this.#waiters.get(frame.body.transferId);
        const pending = waiters?.status;
        if (waiters === undefined || pending === undefined) return true;
        waiters.status = undefined;
        pending.cancel();
        pending.resolve({
          state: frame.body.state,
          receivedLength: frame.body.receivedLength,
          nextOffset: frame.body.nextOffset,
          sealedSha256: frame.body.sealedSha256,
        });
        return true;
      }
      case "attachment.read_chunk": {
        const waiters = this.#waiters.get(frame.body.attachmentId);
        const pending = waiters?.read;
        if (waiters === undefined || pending === undefined) return true;
        waiters.read = undefined;
        pending.cancel();
        try {
          pending.resolve({
            attachmentId: frame.body.attachmentId,
            sessionId: frame.body.sessionId,
            offset: frame.body.offset,
            length: frame.body.length,
            totalLength: frame.body.totalLength,
            data: decodeAttachmentChunk(frame.body.data),
            eof: frame.body.eof,
          });
        } catch (error) {
          pending.reject(new AttachmentTransferError("protocol", String(error), false));
        }
        return true;
      }
      case "attachment.error": {
        const transferId = frame.body.transferId;
        if (transferId !== null) {
          const waiters = this.#waiters.get(transferId);
          const failure = failureOfCode(frame.body.code);
          const error = new AttachmentTransferError(
            failure,
            frame.body.message,
            frame.body.retryable,
            frame.body.details,
            frame.body.code,
          );
          // 一个 transfer 可能同时有块确认与封存等待者；错误帧同时结束它们。
          this.#rejectAll(waiters, error);
        }
        return true;
      }
      default:
        // begin/chunk/seal/abort/read/manifest_begin/manifest_page 是客户端发往服务端的帧；
        // 客户端收到它们说明对端实现异常。形状合法但不处理，返回 true 不让解码层背锅。
        return true;
    }
  }

  /** 主动取消所有等待者（连接关闭 / 设备撤销）。 */
  abortAll(reason: AttachmentTransferFailure = "aborted"): void {
    for (const [key, waiters] of this.#waiters) {
      this.#rejectAll(waiters, new AttachmentTransferError(reason, "附件连接已关闭", reason !== "revoked"));
      this.#waiters.delete(key);
    }
  }

  /**
   * 上传一个附件并封存。
   *
   * 步骤：`begin` → 逐块 `chunk`（每块等确认）→ `seal`。`transferId` 由调用方给出（= 附件标识），
   * 这样失败重试可以复用同一标识；重复的块由服务端返回原确认而不重复写入。
   */
  async upload(
    binding: AttachmentBinding,
    transferId: string,
    source: AttachmentUploadSource,
  ): Promise<AttachmentSealed> {
    this.#requireNegotiated();
    const totalLength = parseDecimal(source.totalLength, "totalLength");
    if (totalLength > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new AttachmentTransferError("protocol", "当前实现无法定位超过 2^53-1 字节的单个附件");
    }
    this.#sendFrame("attachment.begin", {
      transferId,
      // 两个分支各自闭合（`oneOf`）：准备上传带 `target`，正式上传带顶层 `sessionId`。
      ...(binding.target === undefined
        ? { sessionId: binding.sessionId }
        : { target: binding.target }),
      subject: { kind: "device", id: binding.subjectId },
      totalLength: source.totalLength,
      chunkSize: ATTACHMENT_CHUNK_SIZE,
      mediaType: source.mediaType,
      fileName: source.fileName,
    });

    const hash = new Sha256();
    let offset = 0n;
    while (offset < totalLength) {
      const remaining = totalLength - offset;
      const length = Number(remaining > BigInt(ATTACHMENT_CHUNK_SIZE) ? BigInt(ATTACHMENT_CHUNK_SIZE) : remaining);
      const bytes = await source.readChunk(offset, length);
      if (bytes.length === 0 || bytes.length > ATTACHMENT_CHUNK_SIZE) {
        throw new AttachmentTransferError("protocol", "来源返回了非法块长度");
      }
      hash.update(bytes);
      const next = await this.#sendChunk(transferId, offset, bytes);
      if (next <= offset) {
        throw new AttachmentTransferError("protocol", "服务端确认的 nextOffset 未前进");
      }
      offset = next;
    }

    const digest = encodeAttachmentChunk(hash.digest());
    this.#sendFrame("attachment.seal", {
      transferId,
      length: source.totalLength,
      sha256: digest,
    });
    const sealed = await this.#wait<AttachmentSealed>(this.#slot(transferId), "sealed", transferId);
    if (sealed.length !== source.totalLength || sealed.sha256 !== digest) {
      throw new AttachmentTransferError("digest_mismatch", "服务端封存的长度或摘要与本地不一致", false);
    }
    return sealed;
  }

  /**
   * 一份已封存附件的清单分批封存。
   *
   * `attachmentCount` 与页数由本地算出；每页最多 64 个标识。只有最后一页带 `final: true`，
   * 服务端据此知道清单完整后才回 `manifest_sealed`。
   */
  async sealManifest(
    binding: AttachmentBinding,
    submissionId: string,
    transferIds: readonly string[],
  ): Promise<AttachmentManifestSealed> {
    this.#requireNegotiated();
    if (transferIds.length === 0) {
      throw new AttachmentTransferError("protocol", "空清单不需要也不允许封存");
    }
    this.#sendFrame("attachment.manifest_begin", {
      submissionId,
      ...(binding.target === undefined
        ? { sessionId: binding.sessionId }
        : { target: binding.target }),
      subject: { kind: "device", id: binding.subjectId },
      attachmentCount: String(transferIds.length),
    });
    for (let index = 0; index < transferIds.length; index += ATTACHMENT_MANIFEST_PAGE_SIZE) {
      const page = transferIds.slice(index, index + ATTACHMENT_MANIFEST_PAGE_SIZE);
      this.#sendFrame("attachment.manifest_page", {
        submissionId,
        transferIds: page,
        final: index + ATTACHMENT_MANIFEST_PAGE_SIZE >= transferIds.length,
      });
    }
    // 合同要求分页后再发一次 `attachment.manifest_sealed`（带整份清单的摘要）；
    // 服务端只在收到这一帧后才校验摘要并回 `manifest_sealed`，不会从 `final: true` 自动封存。
    this.#sendFrame("attachment.manifest_sealed", {
      submissionId,
      attachmentCount: String(transferIds.length),
      manifestDigest: manifestDigestOf(transferIds),
      sealedAt: new Date().toISOString(),
    });
    return this.#wait<AttachmentManifestSealed>(this.#slot(submissionId), "manifest", submissionId);
  }

  /** 读一段已封存附件；`maxLength` 不得超过 32 KiB。 */
  async read(input: {
    readonly attachmentId: string;
    readonly sessionId: string;
    readonly offset: string;
    readonly maxLength?: number;
  }): Promise<AttachmentReadChunk> {
    this.#requireNegotiated();
    const maxLength = Math.min(input.maxLength ?? ATTACHMENT_READ_MAX_LENGTH, ATTACHMENT_READ_MAX_LENGTH);
    this.#sendFrame("attachment.read", {
      attachmentId: input.attachmentId,
      sessionId: input.sessionId,
      offset: input.offset,
      maxLength: String(maxLength),
    });
    const chunk = await this.#wait<AttachmentReadChunk>(this.#slot(input.attachmentId), "read", input.attachmentId);
    if (chunk.attachmentId !== input.attachmentId || chunk.sessionId !== input.sessionId) {
      throw new AttachmentTransferError("binding_mismatch", "读取响应的归属与请求不一致", false);
    }
    return chunk;
  }

  /** 查询传输状态（重连后继续上传的入口）。 */
  async status(transferId: string): Promise<{
    readonly state: AttachmentTransferState;
    readonly receivedLength: string;
    readonly nextOffset: string;
    readonly sealedSha256: string | null;
  }> {
    this.#requireNegotiated();
    this.#sendFrame("attachment.status", { transferId });
    return this.#wait<{
      readonly state: AttachmentTransferState;
      readonly receivedLength: string;
      readonly nextOffset: string;
      readonly sealedSha256: string | null;
    }>(this.#slot(transferId), "status", transferId);
  }

  /** 取消一个传输；服务端不会为它产生可引用附件。 */
  abort(transferId: string, reason: "client_cancelled" | "budget_exceeded" | "revoked" | "digest_mismatch"): void {
    this.#requireNegotiated();
    this.#sendFrame("attachment.abort", { transferId, reason });
    const waiters = this.#waiters.get(transferId);
    this.#rejectAll(waiters, new AttachmentTransferError("aborted", "传输已取消", false));
    this.#waiters.delete(transferId);
  }

  async #sendChunk(transferId: string, offset: bigint, bytes: Uint8Array): Promise<bigint> {
    const offsetText = offset.toString();
    this.#sendFrame("attachment.chunk", {
      transferId,
      offset: offsetText,
      data: encodeAttachmentChunk(bytes),
    });
    try {
      const ack = await this.#wait<{ duplicate: boolean; nextOffset: string }>(
        this.#slot(transferId),
        "chunkAck",
        transferId,
        offsetText,
      );
      const next = parseDecimal(ack.nextOffset, "nextOffset");
      return next;
    } catch (error) {
      if (error instanceof AttachmentTransferError && error.failure === "offset_mismatch") {
        const expected = error.details["expectedOffset"];
        if (typeof expected === "string") return parseDecimal(expected, "expectedOffset");
      }
      throw error;
    }
  }

  #sendFrame(type: AttachmentFrameType, body: Record<string, unknown>): void {
    this.#sequence += 1n;
    this.#send({
      protocolVersion: 1,
      type,
      messageId: this.#uuid(),
      connectionId: this.#connectionId,
      connectionSequence: this.#sequence.toString(),
      body,
    } as unknown as AttachmentFrame);
  }

  #requireNegotiated(): void {
    if (!this.negotiated) {
      throw new AttachmentTransferError(
        "negotiation_required",
        "当前连接未协商 core.attachment-transfer.v1，不能传输附件",
      );
    }
  }

  #slot(key: string): Waiters {
    const existing = this.#waiters.get(key);
    if (existing !== undefined) return existing;
    const created: Waiters = {
      chunkAck: undefined,
      sealed: undefined,
      manifest: undefined,
      status: undefined,
      read: undefined,
    };
    this.#waiters.set(key, created);
    return created;
  }

  #wait<T>(
    slot: Waiters,
    kind: "chunkAck" | "sealed" | "manifest" | "status" | "read",
    _key: string,
    offset?: string,
  ): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      let cancel: CancelTimer = () => undefined;
      const settle = (error: AttachmentTransferError): void => {
        cancel();
        reject(error);
      };
      const timer = this.#clock.schedule(this.#chunkTimeoutMs, () => {
        settle(new AttachmentTransferError("timeout", "等待附件帧超时", true));
      });
      cancel = timer;
      const waiter: Waiter<T> = {
        resolve: (value) => {
          cancel();
          resolve(value);
        },
        reject: settle,
        cancel,
      };
      switch (kind) {
        case "chunkAck":
          slot.chunkAck = {
            offset: offset ?? "0",
            waiter: waiter as Waiter<{ duplicate: boolean; nextOffset: string }>,
          };
          return;
        case "sealed":
          slot.sealed = waiter as Waiter<AttachmentSealed>;
          return;
        case "manifest":
          slot.manifest = waiter as Waiter<AttachmentManifestSealed>;
          return;
        case "status":
          slot.status = waiter as Waiter<{
            state: AttachmentTransferState;
            receivedLength: string;
            nextOffset: string;
            sealedSha256: string | null;
          }>;
          return;
        case "read":
          slot.read = waiter as Waiter<AttachmentReadChunk>;
          return;
      }
    });
  }

  #rejectAll(waiters: Waiters | undefined, error: AttachmentTransferError): void {
    if (waiters === undefined) return;
    waiters.chunkAck?.waiter.reject(error);
    waiters.sealed?.reject(error);
    waiters.manifest?.reject(error);
    waiters.status?.reject(error);
    waiters.read?.reject(error);
    waiters.chunkAck = undefined;
    waiters.sealed = undefined;
    waiters.manifest = undefined;
    waiters.status = undefined;
    waiters.read = undefined;
  }
}

function parseDecimal(value: string, field: string): bigint {
  if (!/^(0|[1-9][0-9]*)$/.test(value)) throw new AttachmentTransferError("protocol", `${field} 不是十进制字符串`);
  return BigInt(value);
}
