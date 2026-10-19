/**
 * 下载端口的 **Web/PWA** 实现（v1 只交付 Web，不写 `.native.ts`）。
 *
 * 优先使用 File System Access（`showSaveFilePicker` + writable stream）直接落盘；环境不支持
 * 时回退到内存 Blob + 对象 URL，并强制 {@link BLOB_DOWNLOAD_LIMIT_BYTES} 上限。两种模式都
 * 在取消时立即丢掉已收字节；Blob URL 由调用方用 `revoke` 撤销（端口负责实际撤销调用）。
 *
 * 本文件不写 localStorage/IndexedDB：正文只活在当前操作的闭包里。
 */

import {
  BLOB_DOWNLOAD_LIMIT_BYTES,
  DownloadError,
  type DownloadArtifact,
  type DownloadEnvironment,
  type DownloadPort,
  type DownloadSink,
  type DownloadTarget,
  type PreviewArtifact,
  type WritableFileLike,
} from "./download.shared";

/** 断言一个可写流的形状（浏览器对象不进入上层）。 */
function isWritableFileLike(value: unknown): value is WritableFileLike {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Readonly<Record<string, unknown>>;
  return typeof record["write"] === "function" && typeof record["close"] === "function";
}

/** 默认浏览器环境；能力缺失时在 `open`/`finish` 处以结构化错误失败。 */
export function browserDownloadEnvironment(): DownloadEnvironment {
  const environment: DownloadEnvironment = {
    createObjectURL(blob: Blob): string {
      const create = globalThis.URL?.createObjectURL;
      if (typeof create !== "function") {
        throw new DownloadError("unsupported", "当前环境不支持 Blob 对象 URL");
      }
      return create.call(globalThis.URL, blob);
    },
    revokeObjectURL(url: string): void {
      const revoke = globalThis.URL?.revokeObjectURL;
      if (typeof revoke === "function") revoke.call(globalThis.URL, url);
    },
    createBlob(parts: readonly Uint8Array[], type: string): Blob {
      // lib.dom 的 `BlobPart` 对 `Uint8Array<ArrayBufferLike>` 声明过窄；运行时 Blob 接受视图。
      const blobParts = parts as unknown as BlobPart[];
      return new Blob(blobParts, { type });
    },
  };

  // File System Access 不在 lib.dom 的稳定类型面里：按结构化最小面读取，并在下面逐项确认函数形状。
  const globalWithPicker = globalThis as unknown as { readonly showSaveFilePicker?: unknown };
  const picker = globalWithPicker.showSaveFilePicker;
  if (typeof picker === "function") {
    environment.openSaveFile = async (target: DownloadTarget): Promise<WritableFileLike | null> => {
      let handle: unknown;
      try {
        handle = await Reflect.apply(picker, globalThis, [{ suggestedName: target.fileName }]);
      } catch (error) {
        const name =
          typeof error === "object" && error !== null && "name" in error ? error.name : null;
        if (name === "AbortError") throw new DownloadError("aborted", "用户取消了保存");
        // 权限/策略拒绝不是「不支持流式落盘」：回退 Blob，仍受 64 MiB 上限约束。
        return null;
      }
      if (typeof handle !== "object" || handle === null || !("createWritable" in handle)) return null;
      const createWritable = handle.createWritable;
      if (typeof createWritable !== "function") return null;
      const writable: unknown = await Reflect.apply(createWritable, handle, []);
      return isWritableFileLike(writable) ? writable : null;
    };
  }
  return environment;
}

/** 打开下载端口。 */
export function openDownloadPort(
  environment: DownloadEnvironment = browserDownloadEnvironment(),
): DownloadPort {
  return {
    blobLimitBytes: BLOB_DOWNLOAD_LIMIT_BYTES,
    async open(target: DownloadTarget): Promise<DownloadSink> {
      const openSaveFile = environment.openSaveFile;
      if (openSaveFile !== undefined) {
        const writable = await openSaveFile(target);
        if (writable !== null) return new StreamDownloadSink(writable);
      }
      return new BlobDownloadSink(environment, target);
    },
    revoke(url: string): void {
      environment.revokeObjectURL(url);
    },
    createPreviewUrl(parts: readonly Uint8Array[], mimeType: string): PreviewArtifact {
      const blob = environment.createBlob(parts, mimeType);
      const url = environment.createObjectURL(blob);
      let live = true;
      return {
        url,
        revoke(): void {
          // 重复撤销是幂等的：对象 URL 只能被撤销一次，第二次调用不再碰环境。
          if (!live) return;
          live = false;
          environment.revokeObjectURL(url);
        },
      };
    },
  };
}

/** 内存 Blob 回退：单次操作内持有字节，取消/结束即释放。 */
class BlobDownloadSink implements DownloadSink {
  readonly mode = "blob" as const;
  readonly #environment: DownloadEnvironment;
  readonly #target: DownloadTarget;
  readonly #parts: Uint8Array[] = [];
  #bytesWritten = 0;
  #url: string | null = null;
  #state: "open" | "finished" | "cancelled" = "open";

  constructor(environment: DownloadEnvironment, target: DownloadTarget) {
    this.#environment = environment;
    this.#target = target;
  }

  async write(bytes: Uint8Array): Promise<void> {
    if (this.#state !== "open") throw new DownloadError("aborted", "这次下载已经结束或取消");
    if (this.#bytesWritten + bytes.length > BLOB_DOWNLOAD_LIMIT_BYTES) {
      throw new DownloadError("budget", "Blob 回退超过 64 MiB 上限");
    }
    this.#parts.push(bytes);
    this.#bytesWritten += bytes.length;
  }

  async finish(): Promise<DownloadArtifact> {
    if (this.#state === "cancelled") throw new DownloadError("aborted", "这次下载已经取消");
    if (this.#state === "finished") throw new DownloadError("io", "这次下载已经完成");
    // 交给 Blob 的必须是独立快照：调用方/替身可能持有该数组引用，清空原数组不能改变已交付内容。
    const parts = this.#parts.slice();
    const blob = this.#environment.createBlob(parts, this.#target.mimeType);
    const url = this.#environment.createObjectURL(blob);
    this.#url = url;
    this.#parts.length = 0;
    this.#state = "finished";
    return { kind: "blob", url, bytesWritten: this.#bytesWritten };
  }

  async cancel(): Promise<void> {
    if (this.#state === "cancelled") return;
    this.#state = "cancelled";
    this.#parts.length = 0;
    if (this.#url !== null) {
      this.#environment.revokeObjectURL(this.#url);
      this.#url = null;
    }
  }
}

/** 流式落盘：直接写用户选择的文件；取消时中止并关闭。 */
class StreamDownloadSink implements DownloadSink {
  readonly mode = "stream" as const;
  readonly #writable: WritableFileLike;
  #bytesWritten = 0;
  #state: "open" | "finished" | "cancelled" = "open";

  constructor(writable: WritableFileLike) {
    this.#writable = writable;
  }

  async write(bytes: Uint8Array): Promise<void> {
    if (this.#state !== "open") throw new DownloadError("aborted", "这次下载已经结束或取消");
    if (this.#bytesWritten + bytes.length > BLOB_DOWNLOAD_LIMIT_BYTES) {
      throw new DownloadError("budget", "单次下载超过 64 MiB 上限");
    }
    await this.#writable.write(bytes);
    this.#bytesWritten += bytes.length;
  }

  async finish(): Promise<DownloadArtifact> {
    if (this.#state === "cancelled") throw new DownloadError("aborted", "这次下载已经取消");
    if (this.#state === "finished") throw new DownloadError("io", "这次下载已经完成");
    await this.#writable.close();
    this.#state = "finished";
    return { kind: "stream", url: null, bytesWritten: this.#bytesWritten };
  }

  async cancel(): Promise<void> {
    if (this.#state === "cancelled") return;
    this.#state = "cancelled";
    try {
      const abort = this.#writable.abort;
      if (abort !== undefined) await abort.call(this.#writable);
      else await this.#writable.close();
    } catch {
      // 取消路径尽力释放；写流已经不可用时没有第二个可做的动作。
    }
  }
}
