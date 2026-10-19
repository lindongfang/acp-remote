/**
 * 平台下载端口（`docs/FRONTEND_DESIGN.md` §3、design D5）。
 *
 * 端口只承担三件事：把有界字节块**保存**到用户选择的落点、**取消**并释放已占资源、管理
 * Blob 对象 URL 的生命周期。它不知道 Sync 查询、会话、路径或预算调度——那些属于 feature/应用
 * 服务。这样组件与 feature 都不直接碰 `showSaveFilePicker`/`URL.createObjectURL`，
 * 原生端将来只需替换本端口的实现。
 *
 * 浏览器不支持流式落盘（File System Access）时回退到内存 Blob，并受
 * {@link BLOB_DOWNLOAD_LIMIT_BYTES} 的硬上限约束；端口**不**写 localStorage/IndexedDB，
 * 正文只在本次操作的内存里，取消或结束后释放。
 */

/** 单次下载预算：与服务端 `session.files.read` 的下载预算一致（64 MiB）。 */
export const BLOB_DOWNLOAD_LIMIT_BYTES = 64 * 1024 * 1024;

/** 端口错误的封闭分类（不把浏览器内部错误原文暴露给上层）。 */
export type DownloadErrorKind = "unsupported" | "budget" | "aborted" | "io";

/** 下载端口的结构化错误。 */
export class DownloadError extends Error {
  readonly kind: DownloadErrorKind;

  constructor(kind: DownloadErrorKind, message: string) {
    super(message);
    this.name = "DownloadError";
    this.kind = kind;
  }
}

/** 一次下载的目标：安全 basename 与 MIME 类型（由调用方从已校验的结果得出）。 */
export interface DownloadTarget {
  readonly fileName: string;
  readonly mimeType: string;
}

/** 落盘方式：`stream`（直接写用户选择的文件）或 `blob`（内存 + 对象 URL 回退）。 */
export type DownloadMode = "stream" | "blob";

/** 完成后的可交付物：Blob 回退给出对象 URL，流式落盘没有 URL。 */
export interface DownloadArtifact {
  readonly kind: DownloadMode;
  readonly url: string | null;
  readonly bytesWritten: number;
}

/**
 * 一次下载的保存句柄。
 *
 * `write` 必须按调用顺序保存字节；预算超限抛 `DownloadError("budget")`。`cancel` 之后
 * 任何 `write` 都失败且不再占内存/URL；`finish` 之后再次写入同样失败。
 */
export interface DownloadSink {
  readonly mode: DownloadMode;
  write(bytes: Uint8Array): Promise<void>;
  finish(): Promise<DownloadArtifact>;
  cancel(): Promise<void>;
}

/** 预览对象 URL：由平台端口创建，调用方用完必须 `revoke()`（正文只活在 Blob 里）。 */
export interface PreviewArtifact {
  readonly url: string;
  readonly revoke: () => void;
}

/** 平台下载端口。 */
export interface DownloadPort {
  /** Blob 回退的硬上限（与 {@link BLOB_DOWNLOAD_LIMIT_BYTES} 同值，供上层显示与预判）。 */
  readonly blobLimitBytes: number;
  /** 打开一次保存；不支持流式落盘时返回 Blob 回退。 */
  open(target: DownloadTarget): Promise<DownloadSink>;
  /** 撤销一个已交付的对象 URL（重复撤销是幂等的）。 */
  revoke(url: string): void;
  /**
   * 为**只读预览**创建同源 Blob URL（图片预览唯一合法的地址来源）。
   *
   * 可选：不具备 Blob/对象 URL 能力的平台可以缺席，调用方据此显式降级（不伪造 data URL，也不
   * 把远端地址塞进 `<img>`）。撤销由返回的 `revoke` 负责，重复调用幂等。
   */
  createPreviewUrl?(parts: readonly Uint8Array[], mimeType: string): PreviewArtifact;
}

/** 平台实现的最小环境面（Web 用浏览器全局；测试注入替身）。 */
export interface DownloadEnvironment {
  createObjectURL(blob: Blob): string;
  revokeObjectURL(url: string): void;
  createBlob(parts: readonly Uint8Array[], type: string): Blob;
  /**
   * 打开用户选择的落点；返回 `null` 表示此环境不支持流式落盘（回退 Blob）。
   * 用户取消时抛 `DownloadError("aborted")`。
   */
  openSaveFile?(target: DownloadTarget): Promise<WritableFileLike | null>;
}

/** `FileSystemWritableFileStream` 的最小形状（不依赖 DOM 的版本差异）。 */
export interface WritableFileLike {
  write(data: Uint8Array): Promise<void>;
  close(): Promise<void>;
  abort?(): Promise<void>;
}
