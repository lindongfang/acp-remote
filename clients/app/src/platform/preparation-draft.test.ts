/**
 * WP6：浏览器准备草稿的 Node 侧证据。
 *
 * 全部通过注入替身运行（fake IndexedDB + 内存 sessionStorage + 固定时钟/ID），
 * 不访问真实浏览器 API。覆盖 6.1–6.4 的完成判据：
 * 版本化/原子替换、逐项配额失败、刷新后附件字节可读、复制标签页隔离、迟到写不复活、
 * 后台/冻结保留、活标签页不被孤儿清理误删、受理后清正文但保留请求关联、禁止字段不可写。
 */

import { beforeEach, describe, expect, it } from "vitest";

import {
  PREPARATION_DRAFT_CHUNK_STORE,
  PREPARATION_DRAFT_ORPHAN_WINDOW_MS,
  PREPARATION_DRAFT_STORE,
  PREPARATION_TAB_STORAGE_KEY,
  PreparationDraftError,
  type PreparationDraftAccess,
  type PreparationDraftAttachmentInput,
  type PreparationDraftContent,
  type PreparationDraftPort,
  type PreparationDraftScope,
  type PreparationSessionStorageLike,
  type PreparationTabBeginResult,
} from "./preparation-draft";
import { openPreparationDraftStore } from "./preparation-draft.web";
import { installFakeIndexedDB } from "./testing/fake-indexeddb";

const ORIGIN = "https://node.example";
const OWNER_NODE_ID = uuid(1);
const DEVICE_ID = uuid(2);
const SESSION_ID = uuid(3);
const REQUEST_ID = uuid(4);
const FIRST_SUBMISSION_ID = uuid(5);
const FIRST_MESSAGE_ID = uuid(6);

const T0 = 1_700_000_000_000;

function uuid(value: number): string {
  return `00000000-0000-4000-8000-${String(value).padStart(12, "0")}`;
}

/** 内存 sessionStorage 替身（每个标签页一份）。 */
class MemoryStorage implements PreparationSessionStorageLike {
  readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }

  removeItem(key: string): void {
    this.values.delete(key);
  }

  snapshot(): MemoryStorage {
    const copy = new MemoryStorage();
    for (const [key, value] of this.values) copy.setItem(key, value);
    return copy;
  }
}

function deferred<T = void>(): {
  readonly promise: Promise<T>;
  readonly resolve: (value: T) => void;
} {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => {
    resolve = accept;
  });
  return { promise, resolve };
}

let now = T0;
let idCounter = 100;
let quota: { readonly usage: number; readonly quota: number } | null = null;

function openPort(
  storage: MemoryStorage,
  overrides: {
    readonly navigationKind?: () => "reload" | "navigate" | "back_forward";
    readonly indexedDb?: IDBFactory;
  } = {},
): PreparationDraftPort {
  return openPreparationDraftStore({
    indexedDb: overrides.indexedDb ?? globalThis.indexedDB,
    sessionStorage: storage,
    now: () => now,
    newId: () => uuid(idCounter++),
    navigationKind: overrides.navigationKind ?? (() => "navigate"),
    estimateQuota: async () => quota,
  });
}

function scopeFor(sessionId = SESSION_ID): PreparationDraftScope {
  return {
    canonicalOrigin: ORIGIN,
    ownerNodeId: OWNER_NODE_ID,
    deviceId: DEVICE_ID,
    // beginTab 对全新标签页会生成自己的 tabSlotId；这个值只是输入占位。
    tabSlotId: uuid(999),
    sessionId,
  };
}

function contentFor(
  access: PreparationDraftAccess,
  overrides: Partial<PreparationDraftContent> = {},
): PreparationDraftContent {
  const scope = access.scope;
  return {
    text: "准备中的首条",
    mode: "default",
    configOptions: { model: "gpt-5", fast: true, effort: "high" },
    agentId: "agent.codex",
    workspace: { directoryId: "dir-1", path: "/work/tree", source: "directoryId" },
    receipt: {
      claims: {
        version: 1,
        ownerNodeId: scope.ownerNodeId,
        storeEpoch: uuid(8),
        origin: scope.canonicalOrigin,
        deviceId: scope.deviceId,
        tabNonce: "A".repeat(43),
        sessionId: scope.sessionId,
        agentId: "agent.codex",
        normalizedWorkspacePath: "/work/tree",
        workspaceSource: "directoryId",
      },
      signature: "B".repeat(86),
    },
    pendingRequestId: null,
    ...overrides,
  };
}

function attachmentInput(
  localId: string,
  bytes: Uint8Array,
  readChunk?: (offset: number, length: number) => Promise<Uint8Array>,
): PreparationDraftAttachmentInput {
  return {
    localId,
    kind: "file",
    fileName: "note.bin",
    mediaType: "application/octet-stream",
    byteLength: String(bytes.length),
    readChunk:
      readChunk ?? (async (offset, length) => bytes.slice(offset, offset + length)),
  };
}

async function beginOwner(
  port: PreparationDraftPort,
  scope: PreparationDraftScope,
  navigationKind: "reload" | "navigate" | "back_forward" = "navigate",
): Promise<Extract<PreparationTabBeginResult, { status: "owner" }>> {
  const result = await port.beginTab({ scope, navigationKind, nowMs: now });
  expect(result.status).toBe("owner");
  if (result.status !== "owner") throw new Error("expected owner");
  return result;
}

async function expectDraftError(
  operation: Promise<unknown>,
  kind: PreparationDraftError["kind"],
): Promise<PreparationDraftError> {
  try {
    await operation;
  } catch (error) {
    if (!(error instanceof PreparationDraftError)) {
      throw new Error(`expected PreparationDraftError ${kind}`);
    }
    expect(error.kind).toBe(kind);
    return error;
  }
  throw new Error(`expected PreparationDraftError ${kind}`);
}

function chunkBytes(): readonly { readonly key: string; readonly value: unknown }[] {
  return globalThis.__fakeIndexedDBDump();
}

/** 替身里已持久化的附件分块视图（用于断言清理/残留）。 */
interface StoredChunkView {
  readonly storageId: string;
  readonly localId: string;
  readonly bytes: Uint8Array;
}

function storedChunks(): readonly StoredChunkView[] {
  return chunkBytes()
    .map((entry) => entry.value)
    .filter((value): value is StoredChunkView => {
      if (typeof value !== "object" || value === null) return false;
      if (!("storageId" in value) || !("localId" in value) || !("bytes" in value)) {
        return false;
      }
      return (
        typeof value.storageId === "string" &&
        typeof value.localId === "string" &&
        value.bytes instanceof Uint8Array
      );
    });
}

/** 失败注入钩子：最小替身不会自己抛配额错误，也不会中止事务。 */
interface IndexedDbFaults {
  /** 返回错误 → 该次 `put` 失败（模拟写入时的 `QuotaExceededError`）。 */
  readonly putError?: (input: {
    readonly storeName: string;
    readonly key: unknown;
    readonly value: unknown;
  }) => unknown | null;
  /** 返回错误 → 该事务以 `onabort` 结束，事务内写入不进入数据库。 */
  readonly abortError?: (
    storeName: string,
    mode: IDBTransactionMode,
  ) => unknown | null;
}

/**
 * 在最小替身之上**逐事务**注入失败。
 *
 * 依赖替身 `open()` 同步给出 `request.result`（真实 IndexedDB 只在 `onsuccess` 后才有结果），
 * 因此只用于本文件的 Node 侧测试。
 */
function faultyIndexedDb(faults: IndexedDbFaults): IDBFactory {
  const base = globalThis.indexedDB;
  return {
    open(name: string, version?: number) {
      const request = base.open(name, version);
      const database = request.result;
      const openTransaction = database.transaction.bind(database);
      database.transaction = ((storeNames: string | string[], mode?: IDBTransactionMode) => {
        const currentStoreName = String(storeNames);
        const transaction = openTransaction(storeNames, mode);
        const abortError =
          faults.abortError?.(currentStoreName, mode ?? "readonly") ?? null;
        if (abortError !== null) {
          let onAbort: (() => void) | null = null;
          let onError: (() => void) | null = null;
          // 模拟回滚：不触发 oncomplete，仅以 onabort + error 结束。
          Object.defineProperty(transaction, "oncomplete", {
            configurable: true,
            get: () => null,
            set: () => {},
          });
          Object.defineProperty(transaction, "onerror", {
            configurable: true,
            get: () => onError,
            set: (handler: (() => void) | null) => {
              onError = handler;
            },
          });
          Object.defineProperty(transaction, "onabort", {
            configurable: true,
            get: () => onAbort,
            set: (handler: (() => void) | null) => {
              onAbort = handler;
            },
          });
          Object.defineProperty(transaction, "error", {
            configurable: true,
            get: () => abortError,
          });
          queueMicrotask(() => queueMicrotask(() => onAbort?.()));
        }
        const openObjectStore = transaction.objectStore.bind(transaction);
        transaction.objectStore = ((storeName: string) => {
          const store = openObjectStore(storeName);
          const put = store.put.bind(store);
          store.put = ((value: unknown, key?: IDBValidKey) => {
            const putError = faults.putError?.({ storeName, key, value }) ?? null;
            if (putError !== null) throw putError;
            // 中止事务里的写入与真实 IndexedDB 一样不生效。
            if (abortError !== null) return undefined as unknown as IDBRequest;
            return put(value, key);
          }) as IDBObjectStore["put"];
          return store;
        }) as IDBTransaction["objectStore"];
        return transaction;
      }) as IDBDatabase["transaction"];
      return request;
    },
  } as unknown as IDBFactory;
}

beforeEach(() => {
  installFakeIndexedDB();
  now = T0;
  idCounter = 100;
  quota = null;
});

describe("6.1 分区、版本与禁止字段", () => {
  it("保存并读回文字、精确配置、路径、凭据与未确认 requestId", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    const content = contentFor(access, { pendingRequestId: REQUEST_ID });

    const saved = await port.saveDraft({
      access,
      expectedRevision: null,
      content,
      nowMs: now,
    });
    expect(saved.revision).toBe(1);
    expect(saved.restorable).toBe(true);

    const loaded = await port.loadDraft({ access, nowMs: now });
    expect(loaded.status).toBe("active");
    if (loaded.status !== "active") throw new Error("expected active");
    expect(loaded.record.payload.status).toBe("active");
    if (loaded.record.payload.status !== "active") throw new Error("expected active payload");
    expect(loaded.record.payload.content.text).toBe("准备中的首条");
    expect(loaded.record.payload.content.configOptions).toEqual({
      model: "gpt-5",
      fast: true,
      effort: "high",
    });
    expect(loaded.record.payload.content.workspace).toEqual({
      directoryId: "dir-1",
      path: "/work/tree",
      source: "directoryId",
    });
    expect(loaded.record.payload.content.receipt.claims.sessionId).toBe(access.scope.sessionId);
    expect(loaded.record.payload.content.pendingRequestId).toBe(REQUEST_ID);
  });

  it("sessionStorage 只保存非敏感定位，不含凭据/正文/配置", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access),
      nowMs: now,
    });

    const raw = storage.getItem(PREPARATION_TAB_STORAGE_KEY);
    expect(raw).not.toBeNull();
    const parsed = JSON.parse(raw ?? "{}") as Record<string, unknown>;
    expect(Object.keys(parsed).sort()).toEqual([
      "canonicalOrigin",
      "deviceId",
      "instanceId",
      "ownerNodeId",
      "savedAtMs",
      "sessionId",
      "tabSlotId",
      "version",
    ]);
    expect(raw).not.toContain("signature");
    expect(raw).not.toContain("receipt");
    expect(raw).not.toContain("准备中的首条");
    expect(raw).not.toContain("configOptions");
    expect(raw).not.toContain("pendingRequestId");
  });

  const forbiddenCases: readonly (readonly [string, Record<string, unknown>])[] = [
    ["privateKey", { privateKey: "secret" }],
    ["pairingSecret", { pairingSecret: "secret" }],
    ["providerCredentials", { providerCredentials: { apiKey: "secret" } }],
    ["mcpCredentials", { mcpCredentials: { token: "secret" } }],
    ["imported", { imported: true }],
  ];
  for (const [label, extra] of forbiddenCases) {
    it(`拒绝把 ${label} 写入准备草稿`, async () => {
      const storage = new MemoryStorage();
      const port = openPort(storage);
      const { access } = await beginOwner(port, scopeFor());
      const content = { ...contentFor(access), ...extra } as unknown as PreparationDraftContent;

      const error = await expectDraftError(
        port.saveDraft({ access, expectedRevision: null, content, nowMs: now }),
        "forbidden_content",
      );
      expect(error.message).toContain("不允许字段");

      const loaded = await port.loadDraft({ access, nowMs: now });
      expect(loaded.status).toBe("absent");
    });
  }

  it("拒绝工作目录里的未知/句柄字段", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    const content = contentFor(access);
    const tampered = {
      ...content,
      workspace: {
        ...content.workspace,
        fileHandle: { name: "opaque" },
      },
    } as unknown as PreparationDraftContent;

    await expectDraftError(
      port.saveDraft({ access, expectedRevision: null, content: tampered, nowMs: now }),
      "forbidden_content",
    );
  });

  it("同一标签页的不同准备分区互不影响", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const first = await beginOwner(port, scopeFor(uuid(21)));
    const second = await beginOwner(port, scopeFor(uuid(22)));

    await port.saveDraft({
      access: first.access,
      expectedRevision: null,
      content: contentFor(first.access, { text: "A" }),
      nowMs: now,
    });
    await port.saveDraft({
      access: second.access,
      expectedRevision: null,
      content: contentFor(second.access, { text: "B" }),
      nowMs: now,
    });

    const loadedA = await port.loadDraft({ access: first.access, nowMs: now });
    const loadedB = await port.loadDraft({ access: second.access, nowMs: now });
    expect(loadedA.status).toBe("active");
    expect(loadedB.status).toBe("active");
    if (loadedA.status === "active" && loadedA.record.payload.status === "active") {
      expect(loadedA.record.payload.content.text).toBe("A");
    }
    if (loadedB.status === "active" && loadedB.record.payload.status === "active") {
      expect(loadedB.record.payload.content.text).toBe("B");
    }
  });
});

describe("6.2 附件字节、原子版本与配额", () => {
  it("刷新后附件字节仍可读，而不是失效 File 句柄或 Blob URL", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    const bytes = new Uint8Array(300_001);
    for (let index = 0; index < bytes.length; index += 1) bytes[index] = index % 251;

    // 远大于一个分块，证明确实分块持久化且没有产品级上限。
    const saved = await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access),
      attachments: [attachmentInput("file-1", bytes)],
      nowMs: now,
    });
    expect(saved.revision).toBe(1);

    // 模拟刷新：同一 sessionStorage/IndexedDB，全新的端口实例。
    const reloadedPort = openPort(storage, { navigationKind: () => "reload" });
    const reloaded = await beginOwner(reloadedPort, access.scope, "reload");
    expect(reloaded.restored).toBe(true);

    const loaded = await reloadedPort.loadDraft({ access: reloaded.access, nowMs: now });
    expect(loaded.status).toBe("active");
    if (loaded.status !== "active" || loaded.record.payload.status !== "active") {
      throw new Error("expected active draft");
    }
    const attachment = loaded.record.payload.attachments[0];
    expect(attachment?.chunkCount).toBe(3);

    const restored = await reloadedPort.readAttachment({
      access: reloaded.access,
      localId: "file-1",
    });
    expect(restored.bytes).toEqual(bytes);
    expect(restored.attachment.byteLength).toBe("300001");
  });

  it("新版本提交前旧完整版本仍可读；提交后切换", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    const oldBytes = new TextEncoder().encode("OLD-VERSION");
    const newBytes = new TextEncoder().encode("NEW-VERSION-LONGER");

    const first = await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { text: "v1" }),
      attachments: [attachmentInput("same", oldBytes)],
      nowMs: now,
    });

    const gate = deferred();
    const started = deferred();
    let blocked = false;
    const slow = attachmentInput("same", newBytes, async (offset, length) => {
      if (!blocked) {
        blocked = true;
        started.resolve(undefined);
        await gate.promise;
      }
      return newBytes.slice(offset, offset + length);
    });
    const pending = port.saveDraft({
      access,
      expectedRevision: first.revision,
      content: contentFor(access, { text: "v2" }),
      attachments: [slow],
      nowMs: now,
    });
    await started.promise;

    // 新版本还在写：旧版本仍然完整可读。
    const stillOld = await port.loadDraft({ access, nowMs: now });
    expect(stillOld.status).toBe("active");
    if (stillOld.status === "active" && stillOld.record.payload.status === "active") {
      expect(stillOld.record.payload.content.text).toBe("v1");
      expect(stillOld.record.payload.attachments[0]?.byteLength).toBe(String(oldBytes.length));
    }
    const oldRead = await port.readAttachment({ access, localId: "same" });
    expect(new TextDecoder().decode(oldRead.bytes)).toBe("OLD-VERSION");

    gate.resolve(undefined);
    const second = await pending;
    expect(second.revision).toBe(first.revision + 1);

    const current = await port.loadDraft({ access, nowMs: now });
    expect(current.status).toBe("active");
    if (current.status === "active" && current.record.payload.status === "active") {
      expect(current.record.payload.content.text).toBe("v2");
    }
    const newRead = await port.readAttachment({ access, localId: "same" });
    expect(new TextDecoder().decode(newRead.bytes)).toBe("NEW-VERSION-LONGER");

    // 旧版本分块已清理：只剩当前版本的字节块。
    const chunkValues = chunkBytes()
      .map((entry) => entry.value)
      .filter((value): value is { readonly bytes: Uint8Array; readonly storageId: string } =>
        typeof value === "object" &&
        value !== null &&
        "bytes" in value &&
        value.bytes instanceof Uint8Array,
      );
    expect(chunkValues.length).toBe(1);
    expect(new TextDecoder().decode(chunkValues[0]?.bytes)).toBe("NEW-VERSION-LONGER");
  });

  it("配额不足逐项报错、整体不提交、旧版本保留", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    const first = await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { text: "v1" }),
      nowMs: now,
    });

    quota = { usage: 1_000, quota: 1_020 };
    const error = await expectDraftError(
      port.saveDraft({
        access,
        expectedRevision: first.revision,
        content: contentFor(access, { text: "v2" }),
        attachments: [
          attachmentInput("file-a", new Uint8Array(64)),
          attachmentInput("file-b", new Uint8Array(64)),
        ],
        nowMs: now,
      }),
      "quota_exceeded",
    );
    expect(error.itemLocalIds).toEqual(["file-a", "file-b"]);

    const loaded = await port.loadDraft({ access, nowMs: now });
    expect(loaded.status).toBe("active");
    if (loaded.status === "active" && loaded.record.payload.status === "active") {
      expect(loaded.record.payload.content.text).toBe("v1");
      expect(loaded.record.payload.attachments).toEqual([]);
    }
  });

  it("写入分块时配额耗尽：逐项报出失败附件，不产生部分可见版本", async () => {
    const storage = new MemoryStorage();
    let failChunkWrites = false;
    const indexedDb = faultyIndexedDb({
      putError: ({ storeName, value }) => {
        if (!failChunkWrites || storeName !== PREPARATION_DRAFT_CHUNK_STORE) return null;
        if (typeof value !== "object" || value === null || !("localId" in value)) {
          return null;
        }
        if (value.localId !== "file-b") return null;
        const error = new Error("QuotaExceededError");
        error.name = "QuotaExceededError";
        return error;
      },
    });
    const port = openPort(storage, { indexedDb });
    const { access } = await beginOwner(port, scopeFor());
    const oldBytes = new Uint8Array(64).fill(7);

    const first = await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { text: "v1" }),
      attachments: [attachmentInput("file-a", oldBytes)],
      nowMs: now,
    });
    expect(storedChunks()).toHaveLength(1);

    // 配额不足发生在**第二项**附件写入时：第一项已落盘的分块必须回滚清理。
    failChunkWrites = true;
    const error = await expectDraftError(
      port.saveDraft({
        access,
        expectedRevision: first.revision,
        content: contentFor(access, { text: "v2" }),
        attachments: [
          attachmentInput("file-a", new Uint8Array(64).fill(9)),
          attachmentInput("file-b", new Uint8Array(64)),
        ],
        nowMs: now,
      }),
      "quota_exceeded",
    );
    expect(error.itemLocalIds).toEqual(["file-b"]);

    // 没有部分可见的新版本：仍是 v1 及其原附件元数据。
    const loaded = await port.loadDraft({ access, nowMs: now });
    expect(loaded.status).toBe("active");
    if (loaded.status !== "active" || loaded.record.payload.status !== "active") {
      throw new Error("expected active draft");
    }
    expect(loaded.record.revision).toBe(first.revision);
    expect(loaded.record.payload.content.text).toBe("v1");
    expect(loaded.record.payload.attachments.map((item) => item.localId)).toEqual([
      "file-a",
    ]);

    const restored = await port.readAttachment({ access, localId: "file-a" });
    expect(restored.bytes).toEqual(oldBytes);

    // 失败版本写入的 file-a 分块已按 storageId 回滚，只剩旧版本分块。
    const remaining = storedChunks();
    expect(remaining).toHaveLength(1);
    expect(remaining[0]?.bytes).toEqual(oldBytes);
  });

  it("提交事务中止：上一完整版本及其分块保持可读", async () => {
    const storage = new MemoryStorage();
    let abortCommits = false;
    const indexedDb = faultyIndexedDb({
      abortError: (storeName, mode) => {
        if (!abortCommits || storeName !== PREPARATION_DRAFT_STORE) return null;
        if (mode !== "readwrite") return null;
        const error = new Error("transaction aborted");
        error.name = "AbortError";
        return error;
      },
    });
    const port = openPort(storage, { indexedDb });
    const { access } = await beginOwner(port, scopeFor());
    const oldBytes = new TextEncoder().encode("OLD-VERSION");

    const first = await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { text: "v1" }),
      attachments: [attachmentInput("same", oldBytes)],
      nowMs: now,
    });
    const baseline = await port.loadDraft({ access, nowMs: now });
    if (baseline.status !== "active") throw new Error("expected active draft");
    const oldStorageId = baseline.record.storageId;
    expect(oldStorageId).not.toBeNull();

    abortCommits = true;
    const newBytes = new TextEncoder().encode("NEW-VERSION-LONGER");
    const error = await expectDraftError(
      port.saveDraft({
        access,
        expectedRevision: first.revision,
        content: contentFor(access, { text: "v2" }),
        attachments: [attachmentInput("same", newBytes)],
        nowMs: now,
      }),
      "storage_unavailable",
    );
    expect(error.message).toContain("中止");
    abortCommits = false;

    // 提交失败后旧版本记录与旧分块都还在：清理只针对本次尝试的 storageId。
    const loaded = await port.loadDraft({ access, nowMs: now });
    if (loaded.status !== "active" || loaded.record.payload.status !== "active") {
      throw new Error("expected active draft");
    }
    expect(loaded.record.revision).toBe(first.revision);
    expect(loaded.record.payload.content.text).toBe("v1");
    expect(loaded.record.storageId).toBe(oldStorageId);
    expect(loaded.record.payload.attachments[0]?.byteLength).toBe(
      String(oldBytes.length),
    );

    const restored = await port.readAttachment({ access, localId: "same" });
    expect(restored.bytes).toEqual(oldBytes);

    const remaining = storedChunks();
    expect(remaining).toHaveLength(1);
    expect(remaining[0]?.storageId).toBe(oldStorageId);
    expect(new TextDecoder().decode(remaining[0]?.bytes)).toBe("OLD-VERSION");
  });

  it("附件读取不足明确失败，不截断提交", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    const bytes = new Uint8Array(10);
    const short = attachmentInput("short", bytes, async () => bytes.slice(0, 9));

    const error = await expectDraftError(
      port.saveDraft({
        access,
        expectedRevision: null,
        content: contentFor(access),
        attachments: [short],
        nowMs: now,
      }),
      "attachment_incomplete",
    );
    expect(error.itemLocalIds).toEqual(["short"]);

    const loaded = await port.loadDraft({ access, nowMs: now });
    expect(loaded.status).toBe("absent");
  });
});

describe("6.3 标签页归属、刷新恢复与迟到写", () => {
  it("复制/opener 标签页得到 duplicate，不接管原准备或凭据", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { text: "original" }),
      nowMs: now,
    });

    // sessionStorage 被复制到新标签页；导航类型是普通导航，不是刷新。
    const duplicateStorage = storage.snapshot();
    const duplicatePort = openPort(duplicateStorage, { navigationKind: () => "navigate" });
    const result = await duplicatePort.beginTab({
      scope: access.scope,
      navigationKind: "navigate",
      nowMs: now,
    });
    expect(result.status).toBe("duplicate");
    if (result.status === "duplicate") {
      expect(result.reason).toBe("live_tab_owner");
    }
    // 复制页自己的定位被清掉，原标签页的定位不受影响。
    expect(duplicateStorage.getItem(PREPARATION_TAB_STORAGE_KEY)).toBeNull();
    expect(storage.getItem(PREPARATION_TAB_STORAGE_KEY)).not.toBeNull();

    const original = await port.loadDraft({ access, nowMs: now });
    expect(original.status).toBe("active");
  });

  it("只有刷新能沿同一标签页定位恢复", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { text: "reload-me" }),
      nowMs: now,
    });

    const reloadedPort = openPort(storage, { navigationKind: () => "reload" });
    const reloaded = await beginOwner(reloadedPort, access.scope, "reload");
    expect(reloaded.restored).toBe(true);
    expect(reloaded.access.scope.tabSlotId).toBe(access.scope.tabSlotId);
    const loaded = await reloadedPort.loadDraft({ access: reloaded.access, nowMs: now });
    expect(loaded.status).toBe("active");
  });

  it("首次加载不知道 tabSlotId 时仍能按定位恢复（调用方只提供会话四项）", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { text: "placeholder-restore" }),
      nowMs: now,
    });

    const reloadedPort = openPort(storage, { navigationKind: () => "reload" });
    // 页面只知道来源/节点/设备/会话；tabSlotId 是占位值。
    const result = await reloadedPort.beginTab({
      scope: scopeFor(),
      navigationKind: "reload",
      nowMs: now,
    });
    expect(result.status).toBe("owner");
    if (result.status !== "owner") throw new Error("expected owner");
    expect(result.restored).toBe(true);
    expect(result.access.scope.tabSlotId).toBe(access.scope.tabSlotId);
    const loaded = await reloadedPort.loadDraft({ access: result.access, nowMs: now });
    expect(loaded.status).toBe("active");
  });

  it("陌生导航不恢复数据库残留", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access),
      nowMs: now,
    });

    // 同一标签页但普通导航：不凭残留恢复，创建全新的标签页分区。
    const navigated = openPort(storage, { navigationKind: () => "navigate" });
    const result = await navigated.beginTab({
      scope: access.scope,
      navigationKind: "navigate",
      nowMs: now,
    });
    expect(result.status).toBe("duplicate");
    expect(storage.getItem(PREPARATION_TAB_STORAGE_KEY)).toBeNull();

    // 没有定位的新标签页拿到新分区，旧草稿不会出现。
    const fresh = openPort(new MemoryStorage(), { navigationKind: () => "navigate" });
    const freshResult = await beginOwner(fresh, access.scope);
    expect(freshResult.access.scope.tabSlotId).not.toBe(access.scope.tabSlotId);
    expect(freshResult.restored).toBe(false);
    const loaded = await fresh.loadDraft({ access: freshResult.access, nowMs: now });
    expect(loaded.status).toBe("absent");
  });

  it("删除后的迟到写被版本检查拒绝，不能复活内容", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    const first = await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { text: "v1" }),
      nowMs: now,
    });

    const gate = deferred();
    const started = deferred();
    let blocked = false;
    const slow = attachmentInput("late", new Uint8Array(8), async (offset, length) => {
      if (!blocked) {
        blocked = true;
        started.resolve(undefined);
        await gate.promise;
      }
      return new Uint8Array(length).slice(offset, offset + length);
    });
    const pending = port.saveDraft({
      access,
      expectedRevision: first.revision,
      content: contentFor(access, { text: "late" }),
      attachments: [slow],
      nowMs: now,
    });
    await started.promise;

    const invalidation = await port.invalidateDraft({
      access,
      expectedRevision: first.revision,
      reason: "navigation",
      nowMs: now,
    });
    expect(invalidation.revision).toBe(first.revision + 1);

    gate.resolve(undefined);
    const error = await expectDraftError(pending, "invalidated");
    expect(error.message).toContain("作废");

    const loaded = await port.loadDraft({ access, nowMs: now });
    expect(loaded.status).toBe("invalidated");
    // 迟到的分块也已清理。
    const stillChunked = chunkBytes().some(
      (entry) =>
        typeof entry.value === "object" &&
        entry.value !== null &&
        "bytes" in entry.value,
    );
    expect(stillChunked).toBe(false);
  });

  it("墓碑跨过 24 小时仍阻止同分区首次保存复活", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    const saved = await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { text: "v1" }),
      nowMs: now,
    });
    await port.invalidateDraft({
      access,
      expectedRevision: saved.revision,
      reason: "navigation",
      nowMs: now,
    });

    now = T0 + PREPARATION_DRAFT_ORPHAN_WINDOW_MS + 1;
    await port.noteLifecycle({ access, event: "visibility_visible", nowMs: now });
    await port.cleanupOrphans({ nowMs: now });

    // 归属仍活、墓碑仍在：删除前的首次保存（expectedRevision=null）不能重建正文。
    await expectDraftError(
      port.saveDraft({
        access,
        expectedRevision: null,
        content: contentFor(access, { text: "resurrected" }),
        nowMs: now,
      }),
      "invalidated",
    );
    expect((await port.loadDraft({ access, nowMs: now })).status).toBe("invalidated");
  });

  it("清理标签页 A 不影响标签页 B", async () => {
    const storageA = new MemoryStorage();
    const storageB = new MemoryStorage();
    const portA = openPort(storageA);
    const portB = openPort(storageB);
    const a = await beginOwner(portA, scopeFor());
    const b = await beginOwner(portB, scopeFor());

    const savedA = await portA.saveDraft({
      access: a.access,
      expectedRevision: null,
      content: contentFor(a.access, { text: "A" }),
      nowMs: now,
    });
    await portB.saveDraft({
      access: b.access,
      expectedRevision: null,
      content: contentFor(b.access, { text: "B" }),
      nowMs: now,
    });

    await portA.invalidateDraft({
      access: a.access,
      expectedRevision: savedA.revision,
      reason: "agent_changed",
      nowMs: now,
    });
    await portA.releaseTab({ access: a.access, nowMs: now });

    const loadedB = await portB.loadDraft({ access: b.access, nowMs: now });
    expect(loadedB.status).toBe("active");
    if (loadedB.status === "active" && loadedB.record.payload.status === "active") {
      expect(loadedB.record.payload.content.text).toBe("B");
    }
  });
});

describe("6.4 后台/冻结、孤儿清理、受理后清理", () => {
  it("后台隐藏与冻结不删除草稿，BFCache 恢复仍持有归属", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { text: "background" }),
      nowMs: now,
    });

    now += 1_000;
    await port.noteLifecycle({ access, event: "visibility_hidden", nowMs: now });
    await port.noteLifecycle({ access, event: "frozen", nowMs: now });
    const hidden = await port.loadDraft({ access, nowMs: now });
    expect(hidden.status).toBe("active");

    // BFCache 恢复：同一文档实例（locator.instanceId 未变）继续持有。
    const resumedPort = openPort(storage, { navigationKind: () => "back_forward" });
    const resumed = await resumedPort.beginTab({
      scope: access.scope,
      navigationKind: "back_forward",
      nowMs: now,
    });
    expect(resumed.status).toBe("owner");
    if (resumed.status === "owner") {
      expect(resumed.access.instanceId).toBe(access.instanceId);
      const loaded = await resumedPort.loadDraft({ access: resumed.access, nowMs: now });
      expect(loaded.status).toBe("active");
    }
  });

  it("24 小时孤儿清理不删除仍由活标签页持有的草稿", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { text: "live" }),
      nowMs: now,
    });

    // 草稿已超过 24 小时，但标签页刚报告过后台隐藏（心跳在窗口内）。
    now = T0 + PREPARATION_DRAFT_ORPHAN_WINDOW_MS + 1_000;
    await port.noteLifecycle({ access, event: "visibility_hidden", nowMs: now });
    const kept = await port.cleanupOrphans({ nowMs: now });
    expect(kept.draftsDeleted).toBe(0);

    const loaded = await port.loadDraft({ access, nowMs: now });
    expect(loaded.status).toBe("active");
  });

  it("无人持有的过期草稿会被物理回收", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access),
      attachments: [attachmentInput("orphan", new Uint8Array(16))],
      nowMs: now,
    });

    now = T0 + PREPARATION_DRAFT_ORPHAN_WINDOW_MS + 1;
    const report = await port.cleanupOrphans({ nowMs: now });
    expect(report.draftsDeleted).toBe(1);
    expect(report.chunksDeleted).toBe(1);

    // 归属也已回收；刷新定位仍在，但同 ID 没有可恢复草稿。
    const reloadPort = openPort(storage, { navigationKind: () => "reload" });
    const reloaded = await reloadPort.beginTab({
      scope: access.scope,
      navigationKind: "reload",
      nowMs: now,
    });
    expect(reloaded.status).toBe("owner");
    if (reloaded.status === "owner") {
      expect(reloaded.restored).toBe(false);
      const loaded = await reloadPort.loadDraft({ access: reloaded.access, nowMs: now });
      expect(loaded.status).toBe("absent");
    }
  });

  it("权威受理并验证后清除正文/Blob，但保留请求关联直到终结", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    const saved = await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { pendingRequestId: REQUEST_ID }),
      attachments: [attachmentInput("first", new Uint8Array(32))],
      nowMs: now,
    });

    const accepted = await port.markAccepted({
      access,
      expectedRevision: saved.revision,
      association: {
        requestId: REQUEST_ID,
        firstSubmissionId: FIRST_SUBMISSION_ID,
        firstMessageId: FIRST_MESSAGE_ID,
        acceptedAtMs: now,
      },
      nowMs: now,
    });
    expect(accepted.revision).toBe(saved.revision + 1);

    const loaded = await port.loadDraft({ access, nowMs: now });
    expect(loaded.status).toBe("accepted");
    if (loaded.status === "accepted") {
      expect(loaded.association.requestId).toBe(REQUEST_ID);
      expect(loaded.association.firstSubmissionId).toBe(FIRST_SUBMISSION_ID);
    }
    const noBody = chunkBytes().some(
      (entry) => typeof entry.value === "object" && entry.value !== null && "bytes" in entry.value,
    );
    expect(noBody).toBe(false);
    await expectDraftError(
      port.readAttachment({ access, localId: "first" }),
      "already_accepted",
    );

    // 已受理关联不按 24 小时草稿窗口删除。
    const report = await port.cleanupOrphans({
      nowMs: now + PREPARATION_DRAFT_ORPHAN_WINDOW_MS * 2,
    });
    expect(report.draftsDeleted).toBe(0);
    const stillAccepted = await port.loadDraft({ access, nowMs: now });
    expect(stillAccepted.status).toBe("accepted");

    // 已受理的关联不能被主动作废：必须等命令终结后 completeRequest。
    await expectDraftError(
      port.invalidateDraft({
        access,
        expectedRevision: accepted.revision,
        reason: "navigation",
        nowMs: now,
      }),
      "already_accepted",
    );
    expect((await port.loadDraft({ access, nowMs: now })).status).toBe("accepted");

    // requestId 不匹配时不删除恢复记录。
    await expectDraftError(
      port.completeRequest({
        access,
        expectedRevision: accepted.revision,
        requestId: uuid(77),
        nowMs: now,
      }),
      "invalid_argument",
    );
    await port.completeRequest({
      access,
      expectedRevision: accepted.revision,
      requestId: REQUEST_ID,
      nowMs: now,
    });
    expect((await port.loadDraft({ access, nowMs: now })).status).toBe("absent");
  });

  it("保存响应丢失：保留完整草稿与未确认 requestId，先核实再清理", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { pendingRequestId: REQUEST_ID }),
      nowMs: now,
    });

    // 模拟刷新后仍能看到完整正文与未确认请求，不自动清理。
    const reloadPort = openPort(storage, { navigationKind: () => "reload" });
    const reloaded = await beginOwner(reloadPort, access.scope, "reload");
    const loaded = await reloadPort.loadDraft({ access: reloaded.access, nowMs: now });
    expect(loaded.status).toBe("active");
    if (loaded.status === "active" && loaded.record.payload.status === "active") {
      expect(loaded.record.payload.content.text).toBe("准备中的首条");
      expect(loaded.record.payload.content.pendingRequestId).toBe(REQUEST_ID);
    }
  });

  it("请求关联必须匹配草稿里的未确认 requestId", async () => {
    const storage = new MemoryStorage();
    const port = openPort(storage);
    const { access } = await beginOwner(port, scopeFor());
    const saved = await port.saveDraft({
      access,
      expectedRevision: null,
      content: contentFor(access, { pendingRequestId: REQUEST_ID }),
      nowMs: now,
    });

    await expectDraftError(
      port.markAccepted({
        access,
        expectedRevision: saved.revision,
        association: {
          requestId: uuid(78),
          firstSubmissionId: FIRST_SUBMISSION_ID,
          firstMessageId: FIRST_MESSAGE_ID,
          acceptedAtMs: now,
        },
        nowMs: now,
      }),
      "invalid_argument",
    );
    const loaded = await port.loadDraft({ access, nowMs: now });
    expect(loaded.status).toBe("active");
  });
});
