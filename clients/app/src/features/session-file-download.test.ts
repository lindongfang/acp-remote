/**
 * 下载应用服务测试（task 4.5）：
 * 多块字节一致、版本/偏移一致性、取消、预算、限流退避、对象 URL 撤销与「无正文持久缓存」。
 */

import { describe, expect, it } from "vitest";

import type { CommandMessage, CommandResultMessage, PublicError } from "../protocol";
import { encodeBase64Url } from "../sync-client";
import type { DownloadPortLike, DownloadSinkLike } from "./session-file-download";
import {
  SESSION_FILE_DOWNLOAD_BUDGET_BYTES,
  SessionFileDownloader,
  safeDownloadFileName,
} from "./session-file-download";

const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const SESSION_ID = "5d73cd10-a465-43cd-b3f1-704e2d49e99e";

function memorySink(): DownloadSinkLike & {
  readonly writes: Uint8Array[];
  readonly cancelled: number;
  finished: number;
} {
  const sink = {
    mode: "blob" as const,
    writes: [] as Uint8Array[],
    cancelled: 0,
    finished: 0,
    async write(bytes: Uint8Array): Promise<void> {
      sink.writes.push(new Uint8Array(bytes));
    },
    async finish() {
      sink.finished += 1;
      return {
        kind: "blob" as const,
        url: "blob:download-1",
        bytesWritten: sink.writes.reduce((total, part) => total + part.length, 0),
      };
    },
    async cancel(): Promise<void> {
      sink.cancelled += 1;
    },
  };
  return sink;
}

function memoryPort(sink: DownloadSinkLike): DownloadPortLike & { readonly opened: number; readonly revoked: string[] } {
  const port = {
    blobLimitBytes: SESSION_FILE_DOWNLOAD_BUDGET_BYTES,
    opened: 0,
    revoked: [] as string[],
    async open() {
      port.opened += 1;
      return sink;
    },
    revoke(url: string) {
      port.revoked.push(url);
    },
  };
  return port;
}

function readResult(input: {
  readonly requestId: string;
  readonly path: string;
  readonly text: string;
  readonly version: string;
  readonly offset: string;
  readonly eof: boolean;
  readonly sizeBytes?: string;
}): CommandResultMessage {
  const sizeBytes = input.sizeBytes ?? String(input.text.length);
  return {
    protocolVersion: 1,
    type: "command.result",
    messageId: "33333333-3333-4333-8333-000000000001",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: {
      requestId: input.requestId,
      command: "session.files.read",
      status: "completed",
      acceptedAt: "2026-10-01T00:00:01.000Z",
      terminalEventId: null,
      result: {
        path: input.path,
        kind: "binary",
        sizeBytes,
        version: input.version,
        offset: input.offset,
        dataBase64: encodeBase64Url(new TextEncoder().encode(input.text)),
        eof: input.eof,
        previewTruncated: false,
      },
      error: null,
    },
  };
}

function rejectedResult(requestId: string, error: PublicError): CommandResultMessage {
  return {
    protocolVersion: 1,
    type: "command.result",
    messageId: "33333333-3333-4333-8333-000000000002",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: {
      requestId,
      command: "session.files.read",
      status: "rejected",
      acceptedAt: null,
      terminalEventId: null,
      result: null,
      error,
    },
  };
}

function scheduler() {
  const tasks: { delayMs: number; task: () => void }[] = [];
  return {
    schedule(delayMs: number, task: () => void): () => void {
      tasks.push({ delayMs, task });
      return () => {};
    },
    get delays(): readonly number[] {
      return tasks.map((entry) => entry.delayMs);
    },
    runAll(): void {
      for (const entry of [...tasks]) entry.task();
      tasks.length = 0;
    },
  };
}

function downloader(input: {
  readonly port: DownloadPortLike;
  readonly gate?: () => { ok: true } | { ok: false; error: PublicError };
}) {
  const dispatches: CommandMessage[] = [];
  let counter = 0;
  const timer = scheduler();
  const service = new SessionFileDownloader({
    gateway: { dispatch: (command) => dispatches.push(command) },
    ids: {
      uuid(): string {
        counter += 1;
        return `00000000-0000-4000-8000-${counter.toString().padStart(12, "0")}`;
      },
    },
    identity: { connectionId: CONNECTION_ID },
    ports: input.port,
    gate: () => input.gate?.() ?? { ok: true as const },
    schedule: timer.schedule,
  });
  return { service, dispatches, timer };
}

/** 等保存链上的微任务落地（保存句柄全是已解决的 Promise，不依赖墙钟）。 */
async function flush(): Promise<void> {
  for (let step = 0; step < 8; step += 1) await Promise.resolve();
}

describe("下载：多块一致性与对象 URL", () => {
  it("第一块确定版本，第二块续读同一版本，最终字节与文件一致，URL 可撤销", async () => {
    const sink = memorySink();
    const port = memoryPort(sink);
    const { service, dispatches } = downloader({ port });

    expect(await service.start({ sessionId: SESSION_ID, path: "src/a.bin", mimeType: "application/octet-stream", sizeBytes: "10" })).toBe(true);
    expect(port.opened).toBe(1);
    expect(dispatches).toHaveLength(1);
    const first = dispatches[0];
    expect(first?.body.command === "session.files.read" ? first.body.payload.purpose : null).toBe("download");
    expect(first?.body.command === "session.files.read" ? first.body.payload.offset : null).toBe("0");
    expect(first?.body.command === "session.files.read" ? first.body.payload.version : "missing").toBeUndefined();

    if (first === undefined) throw new Error("没有首块请求");
    service.apply(readResult({ requestId: first.body.requestId, path: "src/a.bin", text: "hello", version: "v1", offset: "0", eof: false, sizeBytes: "10" }));
    await flush();
    expect(dispatches).toHaveLength(2);
    const second = dispatches[1];
    expect(second?.body.command === "session.files.read" ? second.body.payload.offset : null).toBe("5");
    expect(second?.body.command === "session.files.read" ? second.body.payload.version : null).toBe("v1");

    if (second === undefined) throw new Error("没有续块请求");
    service.apply(readResult({ requestId: second.body.requestId, path: "src/a.bin", text: "world", version: "v1", offset: "5", eof: true, sizeBytes: "10" }));
    await flush();

    expect(sink.writes.map((part) => new TextDecoder().decode(part)).join("")).toBe("helloworld");
    expect(sink.finished).toBe(1);
    const state = service.state;
    expect(state.kind).toBe("saved");
    expect(state.kind === "saved" ? state.url : null).toBe("blob:download-1");
    expect(state.kind === "saved" ? state.bytesWritten : null).toBe(10);

    service.releaseArtifact();
    expect(port.revoked).toEqual(["blob:download-1"]);
    expect(service.state.kind === "saved" ? service.state.url : "missing").toBeNull();
  });
});

describe("下载：取消、预算与快照失效", () => {
  it("取消立即释放保存句柄，迟到的块不再写入", async () => {
    const sink = memorySink();
    const port = memoryPort(sink);
    const { service, dispatches } = downloader({ port });
    await service.start({ sessionId: SESSION_ID, path: "src/a.bin", mimeType: "application/octet-stream", sizeBytes: null });
    const first = dispatches[0];
    if (first === undefined) throw new Error("没有首块请求");
    await service.cancel();
    expect(sink.cancelled).toBe(1);
    expect(service.state.kind).toBe("cancelled");
    service.apply(readResult({ requestId: first.body.requestId, path: "src/a.bin", text: "late", version: "v1", offset: "0", eof: true }));
    await flush();
    expect(sink.writes).toHaveLength(0);
    expect(sink.finished).toBe(0);
  });

  it("已知大小超过 64 MiB 时不建落点、不派发", async () => {
    const sink = memorySink();
    const port = memoryPort(sink);
    const { service, dispatches } = downloader({ port });
    const tooLarge = (BigInt(SESSION_FILE_DOWNLOAD_BUDGET_BYTES) + 1n).toString();
    expect(await service.start({ sessionId: SESSION_ID, path: "src/big.bin", mimeType: "application/octet-stream", sizeBytes: tooLarge })).toBe(false);
    expect(port.opened).toBe(0);
    expect(dispatches).toHaveLength(0);
    expect(service.state.kind === "failed" ? service.state.error.code : null).toBe("resource.result_too_large");
  });

  it("流中版本/偏移不一致立即失败，不拼接两个版本", async () => {
    const sink = memorySink();
    const port = memoryPort(sink);
    const { service, dispatches } = downloader({ port });
    await service.start({ sessionId: SESSION_ID, path: "src/a.bin", mimeType: "application/octet-stream", sizeBytes: "10" });
    const first = dispatches[0];
    if (first === undefined) throw new Error("没有首块请求");
    service.apply(readResult({ requestId: first.body.requestId, path: "src/a.bin", text: "hello", version: "v1", offset: "0", eof: false, sizeBytes: "10" }));
    await flush();
    const second = dispatches[1];
    if (second === undefined) throw new Error("没有续块请求");
    // 偏移应为 5，却声称 6：快照失效，必须重新下载。
    service.apply(readResult({ requestId: second.body.requestId, path: "src/a.bin", text: "world", version: "v1", offset: "6", eof: true, sizeBytes: "10" }));
    await flush();
    expect(service.state.kind).toBe("failed");
    expect(sink.cancelled).toBe(1);
    expect(sink.writes.map((part) => new TextDecoder().decode(part)).join("")).toBe("hello");
  });

  it("限流按 retryAfterMs 重试同一偏移，不失败", async () => {
    const sink = memorySink();
    const port = memoryPort(sink);
    const { service, dispatches, timer } = downloader({ port });
    await service.start({ sessionId: SESSION_ID, path: "src/a.bin", mimeType: "application/octet-stream", sizeBytes: "5" });
    const first = dispatches[0];
    if (first === undefined) throw new Error("没有首块请求");
    service.apply(rejectedResult(first.body.requestId, {
      code: "resource.rate_limited",
      message: "太频繁",
      retryable: true,
      details: { retryAfterMs: 250 },
    }));
    expect(service.state.kind).toBe("downloading");
    expect(timer.delays).toEqual([250]);
    timer.runAll();
    expect(dispatches).toHaveLength(2);
    const retried = dispatches[1];
    expect(retried?.body.command === "session.files.read" ? retried.body.payload.offset : null).toBe("0");
  });

  it("准入被拒（远程/旧主机）时不打开落点、不派发", async () => {
    const sink = memorySink();
    const port = memoryPort(sink);
    const { service, dispatches } = downloader({
      port,
      gate: () => ({
        ok: false,
        error: { code: "capability.unsupported_by_broker", message: "暂不支持", retryable: false, details: {} },
      }),
    });
    expect(await service.start({ sessionId: SESSION_ID, path: "src/a.bin", mimeType: "application/octet-stream", sizeBytes: null })).toBe(false);
    expect(port.opened).toBe(0);
    expect(dispatches).toHaveLength(0);
  });
});

describe("下载：无正文持久缓存", () => {
  it("运行整条下载链路时不访问 localStorage", async () => {
    const sink = memorySink();
    const port = memoryPort(sink);
    const { service, dispatches } = downloader({ port });
    const storageDescriptor = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      get() {
        throw new Error("下载不得访问 localStorage");
      },
    });
    try {
      await service.start({ sessionId: SESSION_ID, path: "src/a.bin", mimeType: "application/octet-stream", sizeBytes: "5" });
      const first = dispatches[0];
      if (first === undefined) throw new Error("没有首块请求");
      service.apply(readResult({ requestId: first.body.requestId, path: "src/a.bin", text: "hello", version: "v1", offset: "0", eof: true }));
      await flush();
      expect(service.state.kind).toBe("saved");
    } finally {
      if (storageDescriptor === undefined) {
        const globalWithStorage: { localStorage?: unknown } = globalThis;
        delete globalWithStorage.localStorage;
      } else Object.defineProperty(globalThis, "localStorage", storageDescriptor);
    }
  });

  it("安全 basename：只取最后一段并去掉控制字符", () => {
    expect(safeDownloadFileName("dir/../evil.txt")).toBe("evil.txt");
    expect(safeDownloadFileName("dir\\evil\u0000.txt")).toBe("evil.txt");
    expect(safeDownloadFileName("..")).toBe("download");
  });
});
