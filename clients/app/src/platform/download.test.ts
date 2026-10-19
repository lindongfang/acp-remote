/**
 * 平台下载端口测试（task 4.5）：Blob/流式两种保存方式、64 MiB 回退上限、取消释放与
 * 对象 URL 撤销。环境面用注入替身，因此测试不依赖真实浏览器或墙钟。
 */

import { describe, expect, it } from "vitest";

import {
  BLOB_DOWNLOAD_LIMIT_BYTES,
  DownloadError,
  type DownloadEnvironment,
  type WritableFileLike,
} from "./download.shared";
import { openDownloadPort } from "./download.web";

function environment(overrides: Partial<DownloadEnvironment> = {}): DownloadEnvironment & {
  readonly blobs: { parts: readonly Uint8Array[]; type: string }[];
  readonly urls: string[];
  readonly revoked: string[];
} {
  const blobs: { parts: readonly Uint8Array[]; type: string }[] = [];
  const urls: string[] = [];
  const revoked: string[] = [];
  return {
    blobs,
    urls,
    revoked,
    createBlob(parts, type) {
      blobs.push({ parts, type });
      return {} as Blob;
    },
    createObjectURL() {
      const url = `blob:test-${urls.length + 1}`;
      urls.push(url);
      return url;
    },
    revokeObjectURL(url) {
      revoked.push(url);
    },
    ...overrides,
  };
}

/** 记录写入顺序的可写流替身。 */
function writable(): WritableFileLike & { readonly writes: Uint8Array[]; closes: number; aborts: number } {
  const stream = {
    writes: [] as Uint8Array[],
    closes: 0,
    aborts: 0,
    async write(data: Uint8Array) {
      stream.writes.push(new Uint8Array(data));
    },
    async close() {
      stream.closes += 1;
    },
    async abort() {
      stream.aborts += 1;
    },
  };
  return stream;
}

describe("平台下载端口：Blob 回退", () => {
  it("按顺序保存字节，完成后给对象 URL，撤销是显式调用", async () => {
    const env = environment();
    const port = openDownloadPort(env);
    const sink = await port.open({ fileName: "a.bin", mimeType: "application/octet-stream" });
    expect(sink.mode).toBe("blob");
    await sink.write(new Uint8Array([1, 2]));
    await sink.write(new Uint8Array([3]));
    const artifact = await sink.finish();
    expect(artifact).toEqual({ kind: "blob", url: "blob:test-1", bytesWritten: 3 });
    expect(env.blobs[0]?.type).toBe("application/octet-stream");
    expect(env.blobs[0]?.parts.map((part) => [...part])).toEqual([[1, 2], [3]]);
    if (artifact.url !== null) port.revoke(artifact.url);
    expect(env.revoked).toEqual(["blob:test-1"]);
  });

  it("超过 64 MiB 的 Blob 回退明确失败，不产生对象 URL", async () => {
    const env = environment();
    const port = openDownloadPort(env);
    expect(port.blobLimitBytes).toBe(BLOB_DOWNLOAD_LIMIT_BYTES);
    const sink = await port.open({ fileName: "big.bin", mimeType: "application/octet-stream" });
    await sink.write(new Uint8Array(BLOB_DOWNLOAD_LIMIT_BYTES));
    await expect(sink.write(new Uint8Array(1))).rejects.toMatchObject({ kind: "budget" });
    expect(env.urls).toEqual([]);
  });

  it("取消后不再接受写入，也不保留对象 URL", async () => {
    const env = environment();
    const port = openDownloadPort(env);
    const sink = await port.open({ fileName: "a.bin", mimeType: "text/plain" });
    await sink.write(new Uint8Array([1]));
    await sink.cancel();
    await expect(sink.write(new Uint8Array([2]))).rejects.toMatchObject({ kind: "aborted" });
    await expect(sink.finish()).rejects.toMatchObject({ kind: "aborted" });
    expect(env.urls).toEqual([]);
  });
});

describe("平台下载端口：流式落盘", () => {
  it("支持流式时直接写用户选择的对象，不创建 Blob URL", async () => {
    const target = writable();
    const env = environment({
      async openSaveFile() {
        return target;
      },
    });
    const port = openDownloadPort(env);
    const sink = await port.open({ fileName: "a.bin", mimeType: "application/octet-stream" });
    expect(sink.mode).toBe("stream");
    await sink.write(new Uint8Array([9, 8]));
    const artifact = await sink.finish();
    expect(artifact).toEqual({ kind: "stream", url: null, bytesWritten: 2 });
    expect(target.writes.map((part) => [...part])).toEqual([[9, 8]]);
    expect(target.closes).toBe(1);
    expect(env.blobs).toEqual([]);
  });

  it("取消流式落盘会中止写入句柄", async () => {
    const target = writable();
    const env = environment({
      async openSaveFile() {
        return target;
      },
    });
    const port = openDownloadPort(env);
    const sink = await port.open({ fileName: "a.bin", mimeType: "text/plain" });
    await sink.cancel();
    expect(target.aborts).toBe(1);
  });

  it("用户取消保存时把 aborted 原样交给上层；落点不可用（null）才回退 Blob", async () => {
    const aborted = new DownloadError("aborted", "用户取消");
    const throwing = openDownloadPort(environment({
      async openSaveFile() {
        throw aborted;
      },
    }));
    await expect(throwing.open({ fileName: "a.bin", mimeType: "text/plain" })).rejects.toMatchObject({ kind: "aborted" });

    const fallback = openDownloadPort(environment({
      async openSaveFile() {
        return null;
      },
    }));
    const sink = await fallback.open({ fileName: "a.bin", mimeType: "text/plain" });
    expect(sink.mode).toBe("blob");
  });
});

describe("平台下载端口：无持久缓存", () => {
  it("保存/撤销过程中不访问 localStorage", async () => {
    const descriptor = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      get() {
        throw new Error("下载端口不得访问 localStorage");
      },
    });
    try {
      const port = openDownloadPort(environment());
      const sink = await port.open({ fileName: "a.bin", mimeType: "text/plain" });
      await sink.write(new Uint8Array([1]));
      const artifact = await sink.finish();
      if (artifact.url !== null) port.revoke(artifact.url);
    } finally {
      if (descriptor === undefined) {
        const globalWithStorage: { localStorage?: unknown } = globalThis;
        delete globalWithStorage.localStorage;
      } else Object.defineProperty(globalThis, "localStorage", descriptor);
    }
  });
});
