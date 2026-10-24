/**
 * 附件上传闸门与媒体生命周期（经 `ClientStore` 的生产管线，不起网络）。
 *
 * 钉住的四件事对应 tasks 6.1/6.2/6.6 的完成判据：
 *
 * 1. **失败不发文字**：任一附件失败/取消时，`session.prompt` 一条都不派发，草稿与选择保留；
 * 2. **取消不是失败**：取消后附件是 `cancelled`，运行态说明「未向 Agent 派发任何内容」；
 * 3. **重试不重复副作用**：重试只重置失败项，重新提交只封存一次清单（因此只派发一次 prompt）；
 * 4. **Blob 生命周期**：读取成功生成对象 URL，释放时撤销且从状态里移除；读取失败是显式状态。
 *
 * 端口替身只实现被测路径：上传/封存/读取/中止与平台媒体的选择/对象 URL。
 */

import { describe, expect, it } from "vitest";

import { ATTACHMENT_FEATURE } from "../sync-client";
import { ClientStore } from "./client-store";
import type {
  AttachmentFileHandle,
  AttachmentManifestRequest,
  AttachmentManifestSealedResult,
  AttachmentMediaPort,
  AttachmentReadRequest,
  AttachmentReadResult,
  AttachmentTransferGateway,
  AttachmentUploadRequest,
  AttachmentUploadSealed,
} from "./ports";
import type { RecordingGateway } from "./testing/fixtures";
import { recordingGateway, resources, session } from "./testing/fixtures";

const SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000001";
const SERVER_EPOCH = "00384a03-bc90-4095-b65d-82fb8cc47e13";
const DEVICE_ID = "cccccccc-0000-4000-8000-000000000001";

/** 让在途的 Promise 链全部落地（上传是 `submitPrompt` 之后启动的异步工作）。 */
async function settle(): Promise<void> {
  for (let index = 0; index < 20; index += 1) {
    await Promise.resolve();
  }
}

/** 派发记录里的 `session.prompt` 载荷（失败用例断言它为空）。 */
function promptDispatches(gateway: RecordingGateway): readonly unknown[] {
  return gateway.dispatches.filter((command) => command.body.command === "session.prompt");
}

/** 可控制失败点与中止的传输替身。 */
class FakeTransfer implements AttachmentTransferGateway {
  readonly uploads: { readonly transferId: string; readonly fileName: string }[] = [];
  readonly aborted: { readonly transferId: string; readonly reason: string }[] = [];
  readonly manifestRequests: { readonly submissionId: string; readonly transferIds: readonly string[] }[] = [];
  /** 第几次 `upload` 抛错（从 1 起；`null` = 从不失败）。 */
  failAt: number | null = null;
  /** 非空时上传等待它落地（取消用例的观测窗口）。 */
  gate: { readonly promise: Promise<void>; readonly release: () => void } | null = null;
  readFails = false;
  /** 非空时按块读取这段字节，并把每次 `read` 挂起直到测试显式放行（用于观察并发）。 */
  chunkedBytes: Uint8Array | null = null;
  /** 每次已发出的 `read`（按发出顺序），`offset` 保持 wire 的十进制文本。 */
  readonly reads: { readonly attachmentId: string; readonly offset: string }[] = [];
  /** 尚未放行的读取；测试按顺序 `resolve()` 一块。 */
  readonly pendingReads: { readonly offset: string; readonly chunk: AttachmentReadResult; readonly resolve: () => void }[] = [];
  inFlightReads = 0;
  maxInFlightReads = 0;

  async upload(input: AttachmentUploadRequest): Promise<AttachmentUploadSealed> {
    this.uploads.push({ transferId: input.transferId, fileName: input.fileName });
    if (this.failAt !== null && this.uploads.length === this.failAt) {
      throw new Error("网络中断：上传未完成");
    }
    if (this.gate !== null) await this.gate.promise;
    return {
      attachmentId: "33333333-0000-4000-8000-000000000003",
      sha256: "A".repeat(43),
      sealedAt: "2026-10-01T00:00:01.000Z",
    };
  }

  async sealManifest(input: AttachmentManifestRequest): Promise<AttachmentManifestSealedResult> {
    this.manifestRequests.push({ submissionId: input.submissionId, transferIds: input.transferIds });
    return { attachmentCount: String(input.transferIds.length), manifestDigest: "B".repeat(43) };
  }

  async read(input: AttachmentReadRequest): Promise<AttachmentReadResult> {
    if (this.readFails) throw new Error("附件已被清理");
    this.reads.push({ attachmentId: input.attachmentId, offset: input.offset });
    if (this.chunkedBytes === null) {
      const bytes = new Uint8Array([1, 2, 3]);
      return {
        offset: input.offset,
        length: String(bytes.length),
        totalLength: String(bytes.length),
        data: bytes,
        eof: true,
      };
    }
    const total = this.chunkedBytes;
    const start = Number(input.offset);
    const end = Math.min(start + 32 * 1024, total.length);
    const chunk = total.slice(start, end);
    const result: AttachmentReadResult = {
      offset: input.offset,
      length: String(chunk.length),
      totalLength: String(total.length),
      data: chunk,
      eof: end >= total.length,
    };
    this.inFlightReads += 1;
    this.maxInFlightReads = Math.max(this.maxInFlightReads, this.inFlightReads);
    return await new Promise<AttachmentReadResult>((resolve) => {
      this.pendingReads.push({
        offset: input.offset,
        chunk: result,
        resolve: () => {
          this.inFlightReads -= 1;
          resolve(result);
        },
      });
    });
  }

  abort(transferId: string, reason: "client_cancelled" | "budget_exceeded" | "revoked" | "digest_mismatch"): void {
    this.aborted.push({ transferId, reason });
  }
}

/** 平台媒体替身：选择返回固定句柄，对象 URL 记账。 */
class FakeMedia implements AttachmentMediaPort {
  readonly createdAt: { readonly mimeType: string; readonly byteLength: number }[] = [];
  readonly createdBytes: Uint8Array[] = [];
  readonly released: string[] = [];
  readonly handles: readonly AttachmentFileHandle[] = [
    { localId: "local-1", kind: "file", fileName: "a.txt", mediaType: "text/plain", byteLength: "5" },
    { localId: "local-2", kind: "file", fileName: "b.txt", mediaType: "text/plain", byteLength: "7" },
  ];

  async pick(): Promise<readonly AttachmentFileHandle[]> {
    return this.handles;
  }

  async fromPaste(): Promise<readonly AttachmentFileHandle[]> {
    return this.handles;
  }

  async readChunk(): Promise<Uint8Array> {
    return new Uint8Array([97, 98]);
  }

  previewUrl(): string | null {
    return null;
  }

  createObjectUrl(bytes: Uint8Array, mimeType: string): string | null {
    this.createdAt.push({ mimeType, byteLength: bytes.length });
    this.createdBytes.push(bytes.slice());
    return `blob:${mimeType}:${String(bytes.length)}`;
  }

  releaseUrl(url: string): void {
    this.released.push(url);
  }
}

/** 一个在线、协商到附件传输、快照里有空闲会话的仓库。 */
function buildStore(): {
  readonly store: ClientStore;
  readonly gateway: RecordingGateway;
  readonly transfer: FakeTransfer;
  readonly media: FakeMedia;
} {
  const transfer = new FakeTransfer();
  const media = new FakeMedia();
  const gateway = recordingGateway({ resources: resources({ sessions: [session({ id: SESSION_ID, state: "idle" })] }) });
  let counter = 0;
  const store = new ClientStore({
    sync: gateway,
    ids: {
      uuid(): string {
        counter += 1;
        return `00000000-0000-4000-8000-${counter.toString().padStart(12, "0")}`;
      },
    },
    now: () => 1_700_000_000_000,
    identity: { connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637" },
    attachments: { media, transfer },
  });
  store.apply({
    kind: "snapshot_verified",
    snapshot: {
      ...(gateway.current as NonNullable<typeof gateway.current>),
      chunkCount: 3,
      receivedChunks: 3,
      snapshotDigest: "kQ8x2mB7yV1cJ0nP5",
    },
  });
  store.apply({
    kind: "authenticated",
    info: {
      deviceId: DEVICE_ID,
      serverEpoch: SERVER_EPOCH,
      headGlobalSequence: "10",
      heartbeatIntervalMs: 30000,
      limits: { maxMessageBytes: 1048576, maxPromptBytes: 262144, maxReplayEventsPerBatch: 500 },
    },
    selectedFeatures: [ATTACHMENT_FEATURE],
  });
  store.pairSucceeded();
  store.apply({ kind: "authenticating" });
  store.apply({ kind: "replaying", cursor: null });
  store.apply({ kind: "online", cursor: { serverEpoch: SERVER_EPOCH, globalSequence: "10" } });
  return { store, gateway, transfer, media };
}

describe("附件上传：失败与取消都不发文字", () => {
  it("第二个附件失败：Agent 一条 prompt 都没收到，草稿与选择保留", async () => {
    const { store, gateway, transfer } = buildStore();
    await store.pickAttachments(SESSION_ID, "file");
    transfer.failAt = 2;

    const outcome = store.submitPrompt(SESSION_ID, "请读取这两个文件");
    expect(outcome.ok).toBe(true);
    await settle();

    const page = store.conversationPage(SESSION_ID);
    // 失败前第一项可能已封存，但清单从未封存、prompt 从未派发。
    expect(transfer.manifestRequests).toHaveLength(0);
    expect(promptDispatches(gateway)).toHaveLength(0);
    expect(page?.input.draft).toBe("请读取这两个文件");
    expect(page?.input.attachments.attachments.map((item) => item.status)).toEqual(["sealed", "failed"]);
    expect(page?.input.attachments.phase).toBe("failed");
    expect(page?.input.attachmentNotice).toContain("附件不可用");
    expect(page?.input.canSubmit).toBe(false);
  });

  it("取消上传：附件标记已取消、调用 abort，且未封存清单、未派发 prompt", async () => {
    const { store, gateway, transfer } = buildStore();
    await store.pickAttachments(SESSION_ID, "file");
    const gate: { resolve: () => void } = { resolve: () => undefined };
    const promise = new Promise<void>((resolve) => {
      gate.resolve = resolve;
    });
    transfer.gate = { promise, release: gate.resolve };

    store.submitPrompt(SESSION_ID, "这段文字不能发出去");
    await settle();
    const cancel = store.cancelAttachmentUpload(SESSION_ID);
    expect(cancel.ok).toBe(true);
    expect(transfer.aborted.length).toBeGreaterThan(0);
    gate.resolve();
    await settle();

    const page = store.conversationPage(SESSION_ID);
    expect(transfer.manifestRequests).toHaveLength(0);
    expect(promptDispatches(gateway)).toHaveLength(0);
    expect(page?.input.draft).toBe("这段文字不能发出去");
    // 已封存的第一项留在服务端（可复用），未封存的第二项标记已取消。
    expect(page?.input.attachments.attachments.map((item) => item.status)).toEqual(["sealed", "cancelled"]);
    expect(page?.input.attachments.phase).toBe("failed");
    expect(page?.input.attachments.error).toContain("已取消");
  });

  it("重试只重置失败项，重新提交恰好派发一次 prompt", async () => {
    const { store, gateway, transfer } = buildStore();
    await store.pickAttachments(SESSION_ID, "file");
    transfer.failAt = 2;
    store.submitPrompt(SESSION_ID, "重试一次");
    await settle();
    expect(store.conversationPage(SESSION_ID)?.input.attachments.attachments[1]?.status).toBe("failed");

    transfer.failAt = null;
    const retry = store.retryAttachments(SESSION_ID);
    expect(retry.ok).toBe(true);
    expect(store.conversationPage(SESSION_ID)?.input.attachments.attachments.map((item) => item.status)).toEqual([
      "sealed",
      "selected",
    ]);

    store.submitPrompt(SESSION_ID, "重试一次");
    await settle();
    const page = store.conversationPage(SESSION_ID);
    // 提交成功后草稿与选择清空；清单只封存一次（重试试次），意味着 prompt 只可能派发一次。
    expect(page?.input.draft).toBe("");
    expect(page?.input.attachments.attachments).toHaveLength(0);
    expect(transfer.manifestRequests).toHaveLength(1);
    expect(promptDispatches(gateway)).toHaveLength(1);
  });
});

describe("附件上传阶段在时间线上的实情", () => {
  it("点击发送后立即显示本次输入与「上传中」，Agent 尚未收到内容", async () => {
    const { store, gateway, transfer } = buildStore();
    await store.pickAttachments(SESSION_ID, "file");
    const gate: { resolve: () => void } = { resolve: () => undefined };
    const promise = new Promise<void>((resolve) => {
      gate.resolve = resolve;
    });
    transfer.gate = { promise, release: gate.resolve };

    store.submitPrompt(SESSION_ID, "带附件的输入");
    await settle();

    const page = store.conversationPage(SESSION_ID);
    const entry = (page?.timeline.entries ?? []).find(
      (item) => item.kind === "message" && item.channel === "user",
    );
    expect(entry?.kind === "message" ? entry.message.text : null).toContain("带附件的输入");
    expect(entry?.kind === "message" ? entry.local?.phase : null).toBe("uploading");
    expect(page?.timeline.process.phase).toBe("submitting");
    expect(page?.timeline.process.label).toBe("附件上传中");
    // 上传完成前一条 prompt 都没有派发。
    expect(promptDispatches(gateway)).toHaveLength(0);

    gate.resolve();
    await settle();
    // 封存/派发之后转到「已提交」，而不是继续显示上传中。
    const dispatched = store.conversationPage(SESSION_ID);
    expect(dispatched?.timeline.process.phase).not.toBe("uploading");
    expect(promptDispatches(gateway)).toHaveLength(1);
  });

  it("上传失败：已显示的输入标「未发送」并保留文字，不假装 Agent 收到", async () => {
    const { store, gateway, transfer } = buildStore();
    await store.pickAttachments(SESSION_ID, "file");
    transfer.failAt = 1;

    store.submitPrompt(SESSION_ID, "上传会失败");
    await settle();

    const page = store.conversationPage(SESSION_ID);
    const entry = (page?.timeline.entries ?? []).find(
      (item) => item.kind === "message" && item.channel === "user",
    );
    expect(entry?.kind === "message" ? entry.local?.phase : null).toBe("failed");
    // 阶段标签本身已经是「未发送」，notice 只留真实原因，不重复前缀。
    expect(entry?.kind === "message" ? entry.local?.notice : null).toBe("网络中断：上传未完成");
    expect(entry?.kind === "message" ? entry.local?.notice : null).not.toContain("未发送");
    // 文字仍在输入区（可修正），且没有派发任何 prompt。
    expect(page?.input.draft).toBe("上传会失败");
    expect(promptDispatches(gateway)).toHaveLength(0);
    // 过程行不再宣称「提交中」。
    expect(page?.timeline.process.phase).not.toBe("submitting");
  });

  it("同一请求重试只保留一条本地条目", async () => {
    const { store, transfer } = buildStore();
    await store.pickAttachments(SESSION_ID, "file");
    transfer.failAt = 1;
    store.submitPrompt(SESSION_ID, "重试的输入");
    await settle();
    transfer.failAt = null;
    store.retryAttachments(SESSION_ID);
    store.submitPrompt(SESSION_ID, "重试的输入");
    await settle();

    const locals = (store.conversationPage(SESSION_ID)?.timeline.entries ?? []).filter(
      (item) => item.kind === "message" && item.channel === "user",
    );
    expect(locals).toHaveLength(1);
  });
});

describe("媒体读取与 Blob 生命周期", () => {
  it("读取一次生成对象 URL，释放时撤销并移除状态", async () => {
    const { store, media } = buildStore();
    await store.loadAttachmentMedia(SESSION_ID, "att-1", "image/png");

    const ready = store.conversationPage(SESSION_ID)?.media.get("att-1");
    expect(ready?.kind).toBe("ready");
    if (ready?.kind !== "ready") throw new Error("读取应成功");
    expect(ready.url).toBe("blob:image/png:3");
    expect(media.createdAt).toEqual([{ mimeType: "image/png", byteLength: 3 }]);

    store.releaseAttachmentMedia("att-1");
    expect(media.released).toEqual(["blob:image/png:3"]);
    expect(store.conversationPage(SESSION_ID)?.media.has("att-1")).toBe(false);
  });

  it("读取失败是显式状态，不生成对象 URL；离开会话时回收本会话全部 URL", async () => {
    const { store, transfer, media } = buildStore();
    await store.loadAttachmentMedia(SESSION_ID, "att-2", "audio/ogg");
    expect(store.conversationPage(SESSION_ID)?.media.get("att-2")?.kind).toBe("ready");

    transfer.readFails = true;
    await store.loadAttachmentMedia(SESSION_ID, "att-3", "image/png");
    const failed = store.conversationPage(SESSION_ID)?.media.get("att-3");
    expect(failed?.kind).toBe("failed");
    expect(media.createdAt).toHaveLength(1);

    store.releaseSessionMedia(SESSION_ID);
    expect(media.released).toEqual(["blob:audio/ogg:3"]);
    expect(store.conversationPage(SESSION_ID)?.media.size).toBe(0);
  });

  it("释放后重建等待旧读取结束：多块图片任何时刻只有一个 offset=0 在途，重建字节完整", async () => {
    const { store, transfer, media } = buildStore();
    const total = new Uint8Array(219_661);
    for (let index = 0; index < total.length; index += 1) total[index] = index % 251;
    transfer.chunkedBytes = total;
    const expectedOffsets = ["0", "32768", "65536", "98304", "131072", "163840", "196608"];

    const first = store.loadAttachmentMedia(SESSION_ID, "att-big", "image/png");
    await settle();
    expect(transfer.pendingReads.map((read) => read.offset)).toEqual(["0"]);

    // 组件卸载：作废这一代读取，随后重建再读一次。
    store.releaseAttachmentMedia("att-big");
    const second = store.loadAttachmentMedia(SESSION_ID, "att-big", "image/png");
    await settle();
    // 断言点：旧读取还在途时，新读取不得再发一个 offset=0。
    expect(transfer.pendingReads.map((read) => read.offset)).toEqual(["0"]);

    const firstOffsets: string[] = [];
    for (let index = 0; index < expectedOffsets.length; index += 1) {
      const pending = transfer.pendingReads.shift();
      if (pending === undefined) throw new Error(`第 ${index + 1} 块缺失`);
      firstOffsets.push(pending.offset);
      pending.resolve();
      await settle();
    }
    await first;
    await settle();

    // 第一代读取完整结束后，第二代才从 0 开始。
    expect(transfer.pendingReads.map((read) => read.offset)).toEqual(["0"]);
    const secondOffsets: string[] = [];
    for (let index = 0; index < expectedOffsets.length; index += 1) {
      const pending = transfer.pendingReads.shift();
      if (pending === undefined) throw new Error(`重建第 ${index + 1} 块缺失`);
      secondOffsets.push(pending.offset);
      pending.resolve();
      await settle();
    }
    await second;

    expect(firstOffsets).toEqual(expectedOffsets);
    expect(secondOffsets).toEqual(expectedOffsets);
    expect(transfer.maxInFlightReads).toBe(1);
    expect(transfer.reads.every((read) => read.attachmentId === "att-big")).toBe(true);
    expect(transfer.reads).toHaveLength(expectedOffsets.length * 2);

    const ready = store.conversationPage(SESSION_ID)?.media.get("att-big");
    expect(ready?.kind).toBe("ready");
    if (ready?.kind !== "ready") throw new Error("读取应成功");
    expect(ready.byteLength).toBe("219661");
    expect(ready.mimeType).toBe("image/png");
    expect(ready.url).toBe("blob:image/png:219661");
    // 第一代产物因代次作废被撤销；最终对象 URL 的字节必须逐字节等于源文件。
    expect(media.createdBytes).toHaveLength(2);
    expect(media.createdBytes.every((bytes) => bytes.length === total.length)).toBe(true);
    expect(media.createdBytes[1]).toEqual(total);
    expect(media.released).toEqual(["blob:image/png:219661"]);
  });
});
