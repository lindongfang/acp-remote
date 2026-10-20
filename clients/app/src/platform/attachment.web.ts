/**
 * `PlatformAttachmentPort` 的 Web/PWA 实现。
 *
 * ## 字节留在平台层
 *
 * 选择/粘贴得到的 `File` 只存进本实现的私有表：上层拿到的是带元数据的句柄，字节只在
 * `readChunk` 里按请求区间读出（上传因此是「一块一块读」而不是「整份读进内存」）。
 *
 * ## 取消不是错误
 *
 * 原生文件对话框没有可靠的「已取消」事件（`cancel` 事件较新，且焦点回窗的时间不固定），
 * 因此同时监听 `cancel` 与窗口重新获得焦点：两者任一发生且没有文件即返回空数组。选择器
 * 元素在两种结局里都会从 DOM 移除。
 *
 * ## 不触碰任何东西直到被调用
 *
 * 模块求值期不访问 `document`/`URL`：vitest 跑在 node 环境，构造端口本身必须安全。
 */

import type { PlatformAttachmentHandle, PlatformAttachmentKind, PlatformAttachmentPort } from "./attachment";

/** 选择器的 `accept` 过滤（`file` 不限制）。 */
const ACCEPT: Readonly<Record<PlatformAttachmentKind, string>> = {
  image: "image/*",
  audio: "audio/*",
  file: "",
};

/** 文件对话框关闭后等待焦点回窗的宽限（毫秒）：太短会在慢的宿主上误判为取消。 */
const CANCEL_GRACE_MS = 300;

/** 文件对象在平台层的最小形状（测试可注入同形替身）。 */
interface FileLike {
  readonly name: string;
  readonly type: string;
  readonly size: number;
  slice(start: number, end: number): Blob;
}

function isFileLike(value: unknown): value is FileLike {
  if (typeof value !== "object" || value === null) return false;
  const shape = value as Partial<FileLike>;
  return typeof shape.size === "number" && typeof shape.type === "string" && typeof shape.slice === "function";
}

/** 一次选择的种类判定：图片/音频按 MIME 归类，其余一律按普通文件。 */
function kindOfMime(mediaType: string): PlatformAttachmentKind {
  if (mediaType.startsWith("image/")) return "image";
  if (mediaType.startsWith("audio/")) return "audio";
  return "file";
}

class WebAttachmentMediaPort implements PlatformAttachmentPort {
  readonly #newId: () => string;
  readonly #blobs = new Map<string, FileLike>();

  constructor(newId: () => string) {
    this.#newId = newId;
  }

  async pick(kind: PlatformAttachmentKind): Promise<readonly PlatformAttachmentHandle[]> {
    if (typeof document === "undefined") return [];
    const files = await this.#chooseFiles(ACCEPT[kind]);
    return this.#register(files, () => kind);
  }

  async fromPaste(event: unknown): Promise<readonly PlatformAttachmentHandle[]> {
    const files = this.#filesOf(event);
    return this.#register(files, (file) => kindOfMime(file.type));
  }

  async readChunk(handle: PlatformAttachmentHandle, offset: bigint, length: number): Promise<Uint8Array> {
    const file = this.#blobs.get(handle.localId);
    if (file === undefined) throw new Error(`${handle.fileName} 的本机内容已不可用，请重新选择`);
    if (offset > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new Error("当前平台无法定位超过 2^53-1 字节的偏移");
    }
    const start = Number(offset);
    const end = Math.min(start + Math.max(0, length), file.size);
    const buffer = await file.slice(start, end).arrayBuffer();
    return new Uint8Array(buffer);
  }

  previewUrl(handle: PlatformAttachmentHandle): string | null {
    if (handle.kind === "file") return null;
    const file = this.#blobs.get(handle.localId);
    if (file === undefined) return null;
    try {
      return typeof URL.createObjectURL === "function"
        ? URL.createObjectURL(file as unknown as Blob)
        : null;
    } catch {
      return null;
    }
  }

  createObjectUrl(bytes: Uint8Array, mimeType: string): string | null {
    try {
      if (typeof URL.createObjectURL !== "function") return null;
      // 复制到本实现的 `ArrayBuffer`：来源切片是 `Uint8Array<ArrayBufferLike>`，BlobPart 只接受
      // `ArrayBuffer` 之上的视图（SharedArrayBuffer 不能直接交给 Blob）。
      const copy = new Uint8Array(bytes);
      return URL.createObjectURL(new Blob([copy], { type: mimeType }));
    } catch {
      return null;
    }
  }

  releaseUrl(url: string): void {
    try {
      if (typeof URL.revokeObjectURL === "function") URL.revokeObjectURL(url);
    } catch {
      // 撤销失败不改变任何用户可见事实：URL 随页面生命周期回收。
    }
  }

  /** 打开文件对话框并等到它关闭；取消返回空数组。 */
  #chooseFiles(accept: string): Promise<readonly FileLike[]> {
    return new Promise((resolve) => {
      const input = document.createElement("input");
      input.type = "file";
      input.multiple = true;
      if (accept.length > 0) input.accept = accept;
      input.style.display = "none";

      let settled = false;
      const cleanup = (): void => {
        input.removeEventListener("change", onChange);
        input.removeEventListener(CANCEL_EVENT, onCancel);
        window.removeEventListener("focus", onFocus);
        input.remove();
      };
      const finish = (files: readonly FileLike[]): void => {
        if (settled) return;
        settled = true;
        cleanup();
        resolve(files);
      };
      const onChange = (): void => {
        finish(filesOfInput(input));
      };
      const onCancel = (): void => {
        finish([]);
      };
      // 某些宿主不派发 `cancel`：焦点回窗后稍等一刻再看有没有文件。
      const onFocus = (): void => {
        setTimeout(() => {
          if (settled) return;
          finish(filesOfInput(input));
        }, CANCEL_GRACE_MS);
      };

      input.addEventListener("change", onChange);
      input.addEventListener(CANCEL_EVENT, onCancel);
      window.addEventListener("focus", onFocus);
      document.body.append(input);
      input.click();
    });
  }

  /** 把文件登记成句柄；同一次选择内 `localId` 唯一。 */
  #register(files: readonly FileLike[], kindOf: (file: FileLike) => PlatformAttachmentKind): readonly PlatformAttachmentHandle[] {
    const handles: PlatformAttachmentHandle[] = [];
    for (const file of files) {
      const localId = this.#newId();
      this.#blobs.set(localId, file);
      handles.push({
        localId,
        kind: kindOf(file),
        fileName: file.name.length > 0 ? file.name : "未命名文件",
        mediaType: file.type.length > 0 ? file.type : "application/octet-stream",
        byteLength: String(file.size),
      });
    }
    return handles;
  }

  /** 从粘贴/拖放事件里取出文件（`DataTransfer.items` 优先，其次 `files`）。 */
  #filesOf(event: unknown): readonly FileLike[] {
    if (typeof event !== "object" || event === null) return [];
    const transfer: unknown = Reflect.get(event, "clipboardData") ?? Reflect.get(event, "dataTransfer");
    if (typeof transfer !== "object" || transfer === null) return [];
    const files: FileLike[] = [];
    const items: unknown = Reflect.get(transfer, "items");
    if (items !== null && typeof items === "object" && Symbol.iterator in items) {
      const itemList = items as Iterable<unknown>;
      for (const item of itemList) {
        if (typeof item !== "object" || item === null) continue;
        if (Reflect.get(item, "kind") !== "file") continue;
        const file: unknown = typeof Reflect.get(item, "getAsFile") === "function"
          ? (Reflect.get(item, "getAsFile") as () => unknown).call(item)
          : null;
        if (isFileLike(file)) files.push(file);
      }
    }
    if (files.length > 0) return files;
    const list: unknown = Reflect.get(transfer, "files");
    if (list !== null && typeof list === "object" && Symbol.iterator in list) {
      for (const file of list as Iterable<unknown>) {
        if (isFileLike(file)) files.push(file);
      }
    }
    return files;
  }
}

/** `cancel` 事件名：部分 TS lib 的 `HTMLElementEventMap` 尚未登记它，用常量传入字符串重载。 */
const CANCEL_EVENT = "cancel";

/** 读 `input.files`；没有文件时返回空数组（取消/未选）。 */
function filesOfInput(input: HTMLInputElement): readonly FileLike[] {
  const list = input.files;
  if (list === null) return [];
  const files: FileLike[] = [];
  for (const file of list) {
    if (isFileLike(file)) files.push(file);
  }
  return files;
}

/**
 * 装配 Web 平台的附件端口。
 *
 * `newId` 由组合根注入（与 `RandomPort.uuid` 同一来源），因此句柄标识可被确定性测试，
 * 也让平台层不必自己决定「页内标识」的生成策略。
 */
export function openAttachmentMediaPort(newId: () => string): PlatformAttachmentPort {
  return new WebAttachmentMediaPort(newId);
}
