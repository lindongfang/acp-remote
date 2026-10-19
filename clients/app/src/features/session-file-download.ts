/**
 * 会话文件下载的应用服务（WP4）。
 *
 * ## 职责边界
 *
 * 本服务负责**调度**：按 `session.files.read` 的 `download` 用途逐块读取、核对路径/版本/偏移、
 * 累计预算、取消、处理限流退避与失败，把字节交给平台下载端口。它不碰 DOM、Blob 或
 * localStorage；保存/取消/对象 URL 撤销都在 `src/platform/download.web.ts`。
 *
 * ## 有界与一致性
 *
 * - 单次下载上限 64 MiB（与服务端预算一致）：已知 `sizeBytes` 超限时不派发、不建落点；
 *   流中越过上限立即取消并显式失败，不悄悄只保存前一段。
 * - 连续块必须来自**同一版本**：第一块确定 `version`，后续块版本或偏移不符即失败，绝不拼接
 *   两个版本的内容。
 * - 取消、连接断开或失败都会取消保存句柄；Blob 回退的 URL 由调用方在交付后 `release` 撤销。
 * - 正文只活在本次操作的闭包与平台保存句柄里，不写持久缓存。
 */

import type { CommandMessage, CommandResultMessage, PublicError, Uuid } from "../protocol";
import { decodeSessionFilesResult } from "../protocol/session-files";
import { decodeBase64Url } from "../sync-client";
import type { CommandGateway, ConnectionIdentity, IdentifierSource } from "./ports";

/** 单次下载预算（与服务端 `docs/SYNC_PROTOCOL.md` §14 的 64 MiB 一致）。 */
export const SESSION_FILE_DOWNLOAD_BUDGET_BYTES = 64 * 1024 * 1024;

/** 限流退避上限（与服务端给出的 `retryAfterMs` 取较小值）。 */
export const DOWNLOAD_RETRY_MAX_MS = 30_000;

/** 平台端口的预览对象 URL 形状（与平台端口同名成员，无需包装）。 */
export interface PreviewArtifactLike {
  readonly url: string;
  readonly revoke: () => void;
}

/** 平台下载端口的结构面（features 不 import `src/platform/`，组合根做结构化装配）。 */
export interface DownloadPortLike {
  readonly blobLimitBytes: number;
  open(target: {
    readonly fileName: string;
    readonly mimeType: string;
  }): Promise<DownloadSinkLike>;
  revoke(url: string): void;
  /** 图片预览的对象 URL；平台不具备该能力时缺席（调用方显式降级）。 */
  createPreviewUrl?(parts: readonly Uint8Array[], mimeType: string): PreviewArtifactLike;
}

/** 保存句柄的结构面（与平台端口同名成员，无需包装）。 */
export interface DownloadSinkLike {
  readonly mode: "stream" | "blob";
  write(bytes: Uint8Array): Promise<void>;
  finish(): Promise<{
    readonly kind: "stream" | "blob";
    readonly url: string | null;
    readonly bytesWritten: number;
  }>;
  cancel(): Promise<void>;
}

/** 下载前的准入判定（连接、feature、远程会话）。 */
export type DownloadGate =
  | { readonly ok: true }
  | { readonly ok: false; readonly error: PublicError };

/** 下载状态（UI 只读这一份）。 */
export type SessionFileDownloadState =
  | { readonly kind: "idle" }
  | {
      readonly kind: "downloading";
      readonly path: string;
      readonly fileName: string;
      readonly bytesWritten: number;
      readonly totalBytes: string | null;
    }
  | {
      readonly kind: "saved";
      readonly path: string;
      readonly fileName: string;
      readonly bytesWritten: number;
      readonly url: string | null;
      readonly mode: "stream" | "blob";
    }
  | { readonly kind: "cancelled"; readonly path: string; readonly fileName: string; readonly bytesWritten: number }
  | { readonly kind: "failed"; readonly path: string; readonly fileName: string; readonly error: PublicError };

/** 一次在飞下载的私有上下文（不进入公开状态）。 */
interface ActiveDownload {
  readonly sessionId: Uuid;
  readonly path: string;
  readonly fileName: string;
  readonly mimeType: string;
  readonly sink: DownloadSinkLike;
  /** 当前块请求标识；取消后旧结果因不匹配被忽略。 */
  requestId: Uuid;
  bytesWritten: number;
  totalBytes: string | null;
  version: string | null;
  nextOffset: string;
  /** 写入串行化：保存句柄的写入顺序就是到达顺序。 */
  chain: Promise<void>;
  cancelled: boolean;
}

/** 下载服务构造输入。 */
export interface SessionFileDownloaderOptions {
  readonly gateway: CommandGateway;
  readonly ids: IdentifierSource;
  /** 已建立连接的标识；未连接为 `null`（此时不打开落点、不派发）。 */
  readonly identity: ConnectionIdentity;
  readonly ports: DownloadPortLike;
  readonly gate: (sessionId: Uuid) => DownloadGate;
  /** 可选时钟/退避：缺省时限流失败只呈现、不自动重试。 */
  readonly schedule?: ((delayMs: number, task: () => void) => () => void) | undefined;
}

type Listener = () => void;

/**
 * 组合根装配的下载服务（模块作用域单份）。
 *
 * 预览与下载按钮只需要一个**已装配**的实例：它由 `createComposition` 构造一次，而页面运行在
 * 同一个浏览器文档里。这里因此采用与 `components/shell/toast` 同一形态的模块级访问点，避免
 * 为了一个只读能力把整份平台端口塞进 React Context（页面与组件仍拿不到任何浏览器 API）。
 *
 * 单测/多条组合根并存时，最后一个构造的实例生效；`dispose` 只清理自己那一份，不会误清后继实例。
 */
let activeDownloader: SessionFileDownloader | null = null;

/** 当前组合根装配的下载服务；尚未装配时为 `null`（按钮据此显式禁用）。 */
export function currentSessionFileDownloader(): SessionFileDownloader | null {
  return activeDownloader;
}

/** 从结果路径取安全 basename（不信任服务端路径里的目录部分）。 */
export function safeDownloadFileName(path: string): string {
  const normalized = path.replaceAll("\\", "/");
  const last = normalized.slice(normalized.lastIndexOf("/") + 1);
  const cleaned = last.replace(/[\u0000-\u001F\u007F]/g, "").trim();
  if (cleaned.length === 0 || cleaned === "." || cleaned === "..") return "download";
  return cleaned;
}

function notConnectedError(): PublicError {
  return {
    code: "internal.unavailable",
    message: "尚未建立已认证连接，无法下载文件。",
    retryable: true,
    details: {},
  };
}

function tooLargeError(limit: number): PublicError {
  return {
    code: "resource.result_too_large",
    message: `文件超过单次下载上限（${Math.floor(limit / (1024 * 1024))} MiB）。`,
    retryable: false,
    details: { limitBytes: limit },
  };
}

function invalidResultError(detail: string): PublicError {
  return {
    code: "internal.unavailable",
    message: `服务端返回的下载块不符合协议：${detail}`,
    retryable: false,
    details: {},
  };
}

function snapshotInvalidError(): PublicError {
  return {
    code: "internal.unavailable",
    message: "文件在下载过程中发生变化，快照已失效；请重新下载。",
    retryable: true,
    details: {},
  };
}

function resultError(result: CommandResultMessage): PublicError {
  if (result.body.error !== null) return result.body.error;
  return {
    code: "internal.unavailable",
    message: `服务端未给出下载失败的原因（${result.body.status}）。`,
    retryable: false,
    details: {},
  };
}

/** 下载服务：一次只允许一个主动下载（与设计的「每设备一个主动读取」一致）。 */
export class SessionFileDownloader {
  readonly #gateway: CommandGateway;
  readonly #ids: IdentifierSource;
  readonly #identity: ConnectionIdentity;
  readonly #ports: DownloadPortLike;
  readonly #gate: (sessionId: Uuid) => DownloadGate;
  readonly #schedule: ((delayMs: number, task: () => void) => () => void) | null;
  readonly #listeners = new Set<Listener>();
  #active: ActiveDownload | null = null;
  #starting = false;
  #state: SessionFileDownloadState = { kind: "idle" };

  constructor(options: SessionFileDownloaderOptions) {
    this.#gateway = options.gateway;
    this.#ids = options.ids;
    this.#identity = options.identity;
    this.#ports = options.ports;
    this.#gate = options.gate;
    this.#schedule = options.schedule ?? null;
    activeDownloader = this;
  }

  get state(): SessionFileDownloadState {
    return this.#state;
  }

  subscribe(listener: Listener): () => void {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  }

  /**
   * 开始一次主动下载。返回 `false` 表示没有开始（原因已写入 `state`）。
   *
   * `sizeBytes` 是预览/列表已知的原始大小；未知时传 `null`，由流中预算兜底。
   */
  async start(input: {
    readonly sessionId: Uuid;
    readonly path: string;
    readonly mimeType: string;
    readonly sizeBytes: string | null;
  }): Promise<boolean> {
    if (this.#active !== null || this.#starting) {
      this.#setState({
        kind: "failed",
        path: input.path,
        fileName: safeDownloadFileName(input.path),
        error: {
          code: "internal.unavailable",
          message: "已有一次下载在进行，请等待它完成或取消。",
          retryable: true,
          details: {},
        },
      });
      return false;
    }
    const gate = this.#gate(input.sessionId);
    if (!gate.ok) {
      this.#setState({
        kind: "failed",
        path: input.path,
        fileName: safeDownloadFileName(input.path),
        error: gate.error,
      });
      return false;
    }
    if (this.#identity.connectionId === null) {
      this.#setState({
        kind: "failed",
        path: input.path,
        fileName: safeDownloadFileName(input.path),
        error: notConnectedError(),
      });
      return false;
    }
    if (input.sizeBytes !== null) {
      let knownSize: bigint;
      try {
        knownSize = BigInt(input.sizeBytes);
      } catch {
        this.#setState({
          kind: "failed",
          path: input.path,
          fileName: safeDownloadFileName(input.path),
          error: invalidResultError("已知大小不是规范十进制字符串"),
        });
        return false;
      }
      if (knownSize > BigInt(SESSION_FILE_DOWNLOAD_BUDGET_BYTES)) {
        this.#setState({
          kind: "failed",
          path: input.path,
          fileName: safeDownloadFileName(input.path),
          error: tooLargeError(SESSION_FILE_DOWNLOAD_BUDGET_BYTES),
        });
        return false;
      }
    }

    const fileName = safeDownloadFileName(input.path);
    this.#starting = true;
    let sink: DownloadSinkLike;
    try {
      sink = await this.#ports.open({ fileName, mimeType: input.mimeType });
    } catch (error) {
      this.#starting = false;
      this.#setState({
        kind: "failed",
        path: input.path,
        fileName,
        error: downloadOpenError(error),
      });
      return false;
    }
    this.#starting = false;

    const requestId = this.#ids.uuid();
    const active: ActiveDownload = {
      sessionId: input.sessionId,
      path: input.path,
      fileName,
      mimeType: input.mimeType,
      sink,
      requestId,
      bytesWritten: 0,
      totalBytes: input.sizeBytes,
      version: null,
      nextOffset: "0",
      chain: Promise.resolve(),
      cancelled: false,
    };
    this.#active = active;
    this.#setState({
      kind: "downloading",
      path: input.path,
      fileName,
      bytesWritten: 0,
      totalBytes: input.sizeBytes,
    });
    if (!this.#dispatchChunk(active)) {
      await this.#fail(active, notConnectedError());
      return false;
    }
    return true;
  }

  /** 摄入一条命令结果；不匹配当前块的结果（关闭/取消后的迟到块）被忽略。 */
  apply(result: CommandResultMessage): void {
    const active = this.#active;
    if (active === null) return;
    if (result.body.command !== "session.files.read") return;
    if (result.body.requestId !== active.requestId) return;
    if (active.cancelled) return;

    if (result.body.status !== "completed") {
      const error = resultError(result);
      if (error.code === "resource.rate_limited") {
        this.#retryAfterRateLimit(active, error);
        return;
      }
      void this.#fail(active, error);
      return;
    }
    const decoded = decodeSessionFilesResult("session.files.read", result.body.result);
    if (!decoded.ok) {
      void this.#fail(active, invalidResultError(decoded.detail));
      return;
    }
    const chunk = decoded.value;
    if (chunk.path !== active.path) {
      void this.#fail(active, invalidResultError("结果 path 与下载目标不一致"));
      return;
    }
    if (chunk.offset !== active.nextOffset) {
      void this.#fail(active, snapshotInvalidError());
      return;
    }
    if (active.version !== null && active.version !== chunk.version) {
      void this.#fail(active, snapshotInvalidError());
      return;
    }
    if (active.totalBytes !== null && active.totalBytes !== chunk.sizeBytes) {
      void this.#fail(active, snapshotInvalidError());
      return;
    }
    if (BigInt(chunk.sizeBytes) > BigInt(SESSION_FILE_DOWNLOAD_BUDGET_BYTES)) {
      void this.#fail(active, tooLargeError(SESSION_FILE_DOWNLOAD_BUDGET_BYTES));
      return;
    }
    if (active.version === null) active.version = chunk.version;
    if (active.totalBytes === null) {
      active.totalBytes = chunk.sizeBytes;
      this.#setState({
        kind: "downloading",
        path: active.path,
        fileName: active.fileName,
        bytesWritten: active.bytesWritten,
        totalBytes: active.totalBytes,
      });
    }

    let bytes: Uint8Array;
    try {
      bytes = decodeBase64Url(chunk.dataBase64);
    } catch {
      void this.#fail(active, invalidResultError("dataBase64 不是合法的无填充 base64url"));
      return;
    }
    const endOffset = (BigInt(chunk.offset) + BigInt(bytes.length)).toString();
    active.nextOffset = endOffset;

    active.chain = active.chain.then(async () => {
      if (active.cancelled) return;
      if (active.bytesWritten + bytes.length > SESSION_FILE_DOWNLOAD_BUDGET_BYTES) {
        await this.#fail(active, tooLargeError(SESSION_FILE_DOWNLOAD_BUDGET_BYTES));
        return;
      }
      await active.sink.write(bytes);
      if (active.cancelled) return;
      active.bytesWritten += bytes.length;
      this.#setState({
        kind: "downloading",
        path: active.path,
        fileName: active.fileName,
        bytesWritten: active.bytesWritten,
        totalBytes: active.totalBytes,
      });
      if (chunk.eof) {
        await this.#finish(active);
        return;
      }
      if (bytes.length === 0) {
        await this.#fail(active, invalidResultError("空块既非 EOF，拒绝无限续读"));
        return;
      }
      if (!this.#dispatchChunk(active)) {
        await this.#fail(active, notConnectedError());
      }
    }).catch(async (error: unknown) => {
      await this.#fail(active, downloadWriteError(error));
    });
  }

  /** 取消并释放：丢弃响应、取消保存句柄；之后迟到的块不会写入。 */
  async cancel(): Promise<void> {
    const active = this.#active;
    if (active === null) return;
    active.cancelled = true;
    this.#active = null;
    try {
      await active.sink.cancel();
    } catch {
      // 取消路径尽力释放，不再把浏览器内部错误抛给 UI。
    }
    this.#setState({
      kind: "cancelled",
      path: active.path,
      fileName: active.fileName,
      bytesWritten: active.bytesWritten,
    });
  }

  /** 撤销已交付的 Blob URL（流式落盘没有 URL，是无操作）。 */
  releaseArtifact(): void {
    const state = this.#state;
    if (state.kind !== "saved" || state.url === null) return;
    this.#ports.revoke(state.url);
    this.#setState({ ...state, url: null });
  }

  /**
   * 为图片预览创建一个同源 Blob URL（唯一的地址来源）。
   *
   * 平台端口不支持对象 URL 时返回 `null`：调用方据此显示「当前平台不支持图片预览」，绝不退回
   * data URL 或远端地址。撤销由返回值的 `revoke` 负责（预览关闭/替换时调用）。
   */
  previewUrl(bytes: Uint8Array, mimeType: string): PreviewArtifactLike | null {
    const create = this.#ports.createPreviewUrl;
    if (create === undefined) return null;
    try {
      return create([bytes], mimeType);
    } catch {
      return null;
    }
  }

  /** 释放服务（组合根 dispose 调用）。 */
  async dispose(): Promise<void> {
    await this.cancel();
    this.#listeners.clear();
    if (activeDownloader === this) activeDownloader = null;
  }

  /** 派发当前块的读取；没有已认证连接时返回 `false`（调用方落显式失败）。 */
  #dispatchChunk(active: ActiveDownload): boolean {
    const connectionId = this.#identity.connectionId;
    if (connectionId === null) return false;
    const requestId = this.#ids.uuid();
    active.requestId = requestId;
    const command: CommandMessage = {
      protocolVersion: 1,
      type: "command",
      messageId: this.#ids.uuid(),
      connectionId,
      connectionSequence: "0",
      body: {
        requestId,
        command: "session.files.read",
        sessionId: active.sessionId,
        payload: {
          path: active.path,
          purpose: "download",
          offset: active.nextOffset,
          ...(active.version === null ? {} : { version: active.version }),
        },
      },
    };
    this.#gateway.dispatch(command);
    return true;
  }

  #retryAfterRateLimit(active: ActiveDownload, error: PublicError): void {
    // 同一条限流结果的重复投递不应再排一次重试。
    active.requestId = "";
    if (this.#schedule === null) {
      void this.#fail(active, error);
      return;
    }
    const raw = error.details["retryAfterMs"];
    const delay = typeof raw === "number" && Number.isFinite(raw) && raw >= 0 ? Math.min(raw, DOWNLOAD_RETRY_MAX_MS) : 1000;
    this.#schedule(delay, () => {
      if (this.#active !== active || active.cancelled) return;
      if (!this.#dispatchChunk(active)) void this.#fail(active, notConnectedError());
    });
  }

  async #finish(active: ActiveDownload): Promise<void> {
    if (active.cancelled) return;
    try {
      const artifact = await active.sink.finish();
      if (this.#active !== active) return;
      this.#active = null;
      this.#setState({
        kind: "saved",
        path: active.path,
        fileName: active.fileName,
        bytesWritten: artifact.bytesWritten,
        url: artifact.url,
        mode: artifact.kind,
      });
    } catch (error) {
      await this.#fail(active, downloadWriteError(error));
    }
  }

  async #fail(active: ActiveDownload, error: PublicError): Promise<void> {
    // 取消或新一次下载已经接管：旧链路的失败不得覆盖当前状态。
    if (this.#active !== active) return;
    this.#active = null;
    active.cancelled = true;
    try {
      await active.sink.cancel();
    } catch {
      // 失败路径同时释放句柄。
    }
    this.#setState({
      kind: "failed",
      path: active.path,
      fileName: active.fileName,
      error,
    });
  }

  #setState(state: SessionFileDownloadState): void {
    this.#state = state;
    for (const listener of [...this.#listeners]) listener();
  }
}

/** 打开落点失败：取消/权限/内存都给出明确分类。 */
function downloadOpenError(error: unknown): PublicError {
  const kind =
    typeof error === "object" && error !== null && "kind" in error ? error.kind : null;
  if (kind === "aborted") {
    return {
      code: "internal.unavailable",
      message: "你取消了保存。",
      retryable: false,
      details: {},
    };
  }
  if (kind === "budget") {
    return tooLargeError(SESSION_FILE_DOWNLOAD_BUDGET_BYTES);
  }
  return {
    code: "internal.unavailable",
    message: "无法在这台设备上创建下载落点。",
    retryable: true,
    details: {},
  };
}

/** 写盘失败：明确告知文件不完整、可重试。 */
function downloadWriteError(error: unknown): PublicError {
  const kind =
    typeof error === "object" && error !== null && "kind" in error ? error.kind : null;
  if (kind === "budget") return tooLargeError(SESSION_FILE_DOWNLOAD_BUDGET_BYTES);
  return {
    code: "internal.unavailable",
    message: "写入下载内容失败；文件可能不完整，请重新下载。",
    retryable: true,
    details: {},
  };
}

