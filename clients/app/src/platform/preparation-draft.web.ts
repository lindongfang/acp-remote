/**
 * 浏览器准备草稿端口的 **Web/PWA** 实现（`web-session-preparation` D5）。
 *
 * ## 存储形状
 *
 * 独立数据库 `acp-remote-preparation-v1`，三个对象存储：
 * - `drafts`：草稿记录或逻辑删除墓碑（键 = 五段分区键）；
 * - `chunks`：附件 `Uint8Array` 分块（键含 `storageId`，新旧版本互不覆盖）；
 * - `ownership`：标签页归属心跳（键 = origin + tabSlotId）。
 *
 * sessionStorage 只保存**非敏感**定位（来源/节点/设备/标签页/会话和文档实例），
 * 不含凭据、文字、配置或附件字节。页面与 feature 只经 `PreparationDraftPort` 访问，
 * 不直接触碰这些 API。
 *
 * ## 原子版本替换
 *
 * 每次保存先在新 `storageId` 下写完全部分块，再在一个 IndexedDB 读写事务里比较
 * `expectedRevision` 并提交记录。旧记录在新记录提交前一直指向旧 `storageId`，因此旧
 * 完整版本持续可读；提交成功后才清理旧分块。比较与提交在同一事务里完成，删除后的
 * 迟到写看到墓碑版本不匹配即失败，不会复活内容。
 *
 * ## 配额
 *
 * 不设附件产品大小/数量上限。保存前用 `navigator.storage.estimate()`（可注入）做一次
 * 预算检查，写入时把 `QuotaExceededError` 映射成带 `itemLocalIds` 的明确错误；任何一项
 * 失败都整体不提交，不截断、不改走服务端上传。
 */

import type {
  PreparationDraftAccess,
  PreparationDraftAttachment,
  PreparationDraftAttachmentInput,
  PreparationDraftCleanupReport,
  PreparationDraftContent,
  PreparationDraftEnvironment,
  PreparationDraftInvalidationReason,
  PreparationDraftLoadResult,
  PreparationDraftPort,
  PreparationDraftRecord,
  PreparationDraftSaveResult,
  PreparationDraftScope,
  PreparationLifecycleEvent,
  PreparationNavigationKind,
  PreparationRequestAssociation,
  PreparationSessionStorageLike,
  PreparationStoredChunk,
  PreparationStoredDraft,
  PreparationTabBeginResult,
  PreparationTabLocator,
  PreparationTabOwnership,
} from "./preparation-draft.shared";
import {
  PREPARATION_DRAFT_CHUNK_SIZE,
  PREPARATION_DRAFT_CHUNK_STORE,
  PREPARATION_DRAFT_DB_NAME,
  PREPARATION_DRAFT_DB_VERSION,
  PREPARATION_DRAFT_ORPHAN_WINDOW_MS,
  PREPARATION_DRAFT_OWNERSHIP_STORE,
  PREPARATION_DRAFT_RECORD_VERSION,
  PREPARATION_DRAFT_STORE,
  PREPARATION_TAB_STORAGE_KEY,
  PreparationDraftError,
  assertPreparationAttachmentInput,
  assertPreparationDraftAttachment,
  assertPreparationDraftContent,
  assertPreparationDraftScope,
  decideTabClaim,
  orphanCleanupPlan,
  parsePreparationByteLength,
  preparationChunkKey,
  preparationDraftKey,
  preparationTabOwnershipKey,
} from "./preparation-draft.shared";

// Web 平台下 `./preparation-draft` 解析到本文件，共享符号必须在这里可见。
export * from "./preparation-draft.shared";

/** 独立的附件元数据开销估算（只用于配额预检，不是产品额度）。 */
const RECORD_OVERHEAD_BYTES = 256;
const CHUNK_OVERHEAD_BYTES = 128;

/** 打开浏览器准备草稿端口。纯构造，不做 IO。 */
export function openPreparationDraftStore(
  environment: PreparationDraftEnvironment = {},
): PreparationDraftPort {
  return new WebPreparationDraftStore(environment);
}

interface StagedAttachment {
  readonly input: PreparationDraftAttachmentInput;
  readonly byteLength: number;
  readonly chunkCount: number;
}

/** `#activeRecord` 的收窄返回：正文与附件清单一定存在。 */
type ActivePreparationDraftRecord = Omit<PreparationDraftRecord, "payload"> & {
  readonly payload: Extract<PreparationDraftRecord["payload"], { status: "active" }>;
};

class WebPreparationDraftStore implements PreparationDraftPort {
  readonly #environment: PreparationDraftEnvironment;
  /** 本标签页定位的内存副本；sessionStorage 不可用时仍用于本页内操作。 */
  #locator: PreparationTabLocator | null = null;
  #locatorPersisted = false;
  #locatorLoaded = false;
  /** 本文件实例是否亲自写过定位；用于同页重复握手幂等，复制标签页不满足。 */
  #locatorOwnedInMemory = false;

  constructor(environment: PreparationDraftEnvironment) {
    this.#environment = environment;
  }

  // ── 环境 ────────────────────────────────────────────────────────────────

  #newId(): string {
    if (this.#environment.newId !== undefined) return this.#environment.newId();
    const crypto: Crypto | undefined = globalThis.crypto;
    if (typeof crypto?.randomUUID === "function") return crypto.randomUUID();
    throw new PreparationDraftError(
      "storage_unavailable",
      "浏览器缺少 crypto.randomUUID，无法生成标签页定位",
    );
  }

  #navigationKind(): PreparationNavigationKind {
    if (this.#environment.navigationKind !== undefined) {
      return this.#environment.navigationKind();
    }
    try {
      // `getEntriesByType` 的静态类型是 `PerformanceEntry`，但 `"navigation"`
      // 在运行时返回 `PerformanceNavigationTiming`；先按实际类型收窄再读 `type`。
      const entry = globalThis.performance?.getEntriesByType?.("navigation")?.[0] as
        | PerformanceNavigationTiming
        | undefined;
      if (entry?.type === "reload") return "reload";
      if (entry?.type === "back_forward") return "back_forward";
    } catch {
      // 无 Navigation Timing 的环境按普通导航处理：绝不恢复残留。
    }
    return "navigate";
  }

  #indexedDbFactory(): IDBFactory | null {
    const configured = this.#environment.indexedDb;
    if (configured === null) return null;
    if (configured !== undefined) return configured;
    // 非浏览器/无安全上下文时全局可能没有 indexedDB。
    const factory: IDBFactory | undefined = globalThis.indexedDB;
    return factory ?? null;
  }

  #sessionStorage(): PreparationSessionStorageLike | null {
    const configured = this.#environment.sessionStorage;
    if (configured === null) return null;
    if (configured !== undefined) return configured;
    try {
      const storage: PreparationSessionStorageLike | undefined = globalThis.sessionStorage;
      return storage ?? null;
    } catch {
      return null;
    }
  }

  async #quotaEstimate(): Promise<{ readonly usage: number; readonly quota: number } | null> {
    if (this.#environment.estimateQuota !== undefined) {
      try {
        return await this.#environment.estimateQuota();
      } catch {
        // 估算失败不是保存失败：退回到浏览器实际写入时的配额错误。
        return null;
      }
    }
    try {
      const storage = globalThis.navigator?.storage;
      if (storage?.estimate === undefined) return null;
      const estimate = await storage.estimate();
      if (typeof estimate.usage !== "number" || typeof estimate.quota !== "number") return null;
      return { usage: estimate.usage, quota: estimate.quota };
    } catch {
      return null;
    }
  }

  // ── sessionStorage 定位 ─────────────────────────────────────────────────

  #currentLocator(): PreparationTabLocator | null {
    if (this.#locatorLoaded) return this.#locator;
    this.#locatorLoaded = true;
    const storage = this.#sessionStorage();
    if (storage === null) return null;
    try {
      const raw = storage.getItem(PREPARATION_TAB_STORAGE_KEY);
      if (raw === null) return null;
      const parsed = parseTabLocator(JSON.parse(raw));
      if (parsed === null) {
        storage.removeItem(PREPARATION_TAB_STORAGE_KEY);
        return null;
      }
      this.#locator = parsed;
      this.#locatorPersisted = true;
      return parsed;
    } catch {
      // 损坏/不可读的定位按不存在处理，绝不凭残留恢复。
      try {
        storage.removeItem(PREPARATION_TAB_STORAGE_KEY);
      } catch {
        // ignore
      }
      return null;
    }
  }

  #writeLocator(locator: PreparationTabLocator): boolean {
    this.#locator = locator;
    this.#locatorLoaded = true;
    this.#locatorOwnedInMemory = true;
    const storage = this.#sessionStorage();
    if (storage === null) {
      this.#locatorPersisted = false;
      return false;
    }
    try {
      storage.setItem(PREPARATION_TAB_STORAGE_KEY, JSON.stringify(locator));
      this.#locatorPersisted = true;
      return true;
    } catch {
      this.#locatorPersisted = false;
      return false;
    }
  }

  #clearLocator(): void {
    this.#locator = null;
    this.#locatorLoaded = true;
    this.#locatorPersisted = false;
    this.#locatorOwnedInMemory = false;
    const storage = this.#sessionStorage();
    if (storage === null) return;
    try {
      storage.removeItem(PREPARATION_TAB_STORAGE_KEY);
    } catch {
      // ignore
    }
  }

  // ── IndexedDB 基础 ──────────────────────────────────────────────────────

  #openDatabase(): Promise<IDBDatabase> {
    const factory = this.#indexedDbFactory();
    if (factory === null) {
      return Promise.reject(
        new PreparationDraftError(
          "storage_unavailable",
          "IndexedDB 不可用（非安全上下文或浏览器不支持）",
        ),
      );
    }
    return new Promise<IDBDatabase>((resolve, reject) => {
      const request = factory.open(PREPARATION_DRAFT_DB_NAME, PREPARATION_DRAFT_DB_VERSION);
      request.onupgradeneeded = () => {
        const database = request.result;
        for (const store of [
          PREPARATION_DRAFT_STORE,
          PREPARATION_DRAFT_CHUNK_STORE,
          PREPARATION_DRAFT_OWNERSHIP_STORE,
        ]) {
          if (!database.objectStoreNames.contains(store)) database.createObjectStore(store);
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () =>
        reject(new PreparationDraftError("storage_unavailable", "打开准备草稿数据库失败"));
      request.onblocked = () =>
        reject(new PreparationDraftError("storage_unavailable", "准备草稿数据库被其他版本占用"));
    });
  }

  async #withDatabase<T>(operation: (database: IDBDatabase) => Promise<T>): Promise<T> {
    const database = await this.#openDatabase();
    try {
      return await operation(database);
    } finally {
      database.close();
    }
  }

  #readValue(
    database: IDBDatabase,
    storeName: string,
    key: string,
  ): Promise<unknown> {
    return new Promise<unknown>((resolve, reject) => {
      const transaction = database.transaction(storeName, "readonly");
      const request = transaction.objectStore(storeName).get(key);
      let value: unknown = undefined;
      request.onsuccess = () => {
        value = request.result;
      };
      request.onerror = () =>
        reject(new PreparationDraftError("storage_unavailable", "读取准备草稿失败"));
      transaction.oncomplete = () => resolve(value);
      transaction.onerror = () =>
        reject(new PreparationDraftError("storage_unavailable", "读取准备草稿失败"));
      transaction.onabort = () =>
        reject(new PreparationDraftError("storage_unavailable", "读取准备草稿被中止"));
    });
  }

  #readAll(database: IDBDatabase, storeName: string): Promise<readonly unknown[]> {
    return new Promise<readonly unknown[]>((resolve, reject) => {
      const transaction = database.transaction(storeName, "readonly");
      const request = transaction.objectStore(storeName).openCursor();
      const values: unknown[] = [];
      request.onsuccess = () => {
        const cursor = request.result;
        if (cursor !== null) {
          values.push(cursor.value);
          cursor.continue();
        }
      };
      request.onerror = () =>
        reject(new PreparationDraftError("storage_unavailable", "扫描准备草稿失败"));
      transaction.oncomplete = () => resolve(values);
      transaction.onerror = () =>
        reject(new PreparationDraftError("storage_unavailable", "扫描准备草稿失败"));
      transaction.onabort = () =>
        reject(new PreparationDraftError("storage_unavailable", "扫描准备草稿被中止"));
    });
  }

  #putValue(
    database: IDBDatabase,
    storeName: string,
    key: string,
    value: unknown,
  ): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      const transaction = database.transaction(storeName, "readwrite");
      transaction.objectStore(storeName).put(value, key);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () =>
        reject(storageFailure("写入准备草稿失败", transaction.error));
      transaction.onabort = () =>
        reject(storageFailure("写入准备草稿被中止", transaction.error));
    });
  }

  #deleteValues(
    database: IDBDatabase,
    storeName: string,
    keys: readonly string[],
  ): Promise<void> {
    if (keys.length === 0) return Promise.resolve();
    return new Promise<void>((resolve, reject) => {
      const transaction = database.transaction(storeName, "readwrite");
      const store = transaction.objectStore(storeName);
      for (const key of keys) store.delete(key);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => reject(new PreparationDraftError("storage_unavailable", "删除准备草稿失败"));
      transaction.onabort = () => reject(new PreparationDraftError("storage_unavailable", "删除准备草稿被中止"));
    });
  }

  async #readOwnership(
    database: IDBDatabase,
    locator: PreparationTabLocator,
  ): Promise<PreparationTabOwnership | null> {
    const raw = await this.#readValue(
      database,
      PREPARATION_DRAFT_OWNERSHIP_STORE,
      preparationTabOwnershipKey(locator.canonicalOrigin, locator.tabSlotId),
    );
    return parseOwnership(raw);
  }

  #writeOwnership(
    database: IDBDatabase,
    ownership: PreparationTabOwnership,
  ): Promise<void> {
    return this.#putValue(
      database,
      PREPARATION_DRAFT_OWNERSHIP_STORE,
      preparationTabOwnershipKey(ownership.canonicalOrigin, ownership.tabSlotId),
      ownership,
    );
  }

  async #assertOwner(
    database: IDBDatabase,
    access: PreparationDraftAccess,
    allowReleased = false,
  ): Promise<PreparationTabOwnership> {
    assertPreparationDraftScope(access.scope);
    if (typeof access.instanceId !== "string" || !isUuid(access.instanceId)) {
      throw new PreparationDraftError("invalid_argument", "instanceId 必须是 UUID");
    }
    const raw = await this.#readValue(
      database,
      PREPARATION_DRAFT_OWNERSHIP_STORE,
      preparationTabOwnershipKey(access.scope.canonicalOrigin, access.scope.tabSlotId),
    );
    const ownership = parseOwnership(raw);
    if (
      ownership === null ||
      (!allowReleased && ownership.releasedAtMs !== null) ||
      ownership.instanceId !== access.instanceId ||
      ownership.canonicalOrigin !== access.scope.canonicalOrigin ||
      ownership.tabSlotId !== access.scope.tabSlotId ||
      ownership.ownerNodeId !== access.scope.ownerNodeId ||
      ownership.deviceId !== access.scope.deviceId ||
      ownership.sessionId !== access.scope.sessionId
    ) {
      throw new PreparationDraftError(
        "tab_not_owner",
        "当前文档实例不再拥有该标签页的准备草稿",
      );
    }
    return ownership;
  }

  // ── 标签页握手 ──────────────────────────────────────────────────────────

  async beginTab(input: {
    readonly scope: PreparationDraftScope;
    readonly navigationKind?: PreparationNavigationKind;
    readonly nowMs: number;
  }): Promise<PreparationTabBeginResult> {
    assertPreparationDraftScope(input.scope);
    const navigationKind = input.navigationKind ?? this.#navigationKind();
    return this.#withDatabase(async (database) => {
      const locator = this.#currentLocator();
      const ownership =
        locator === null ? null : await this.#readOwnership(database, locator);
      // 调用方在首次加载时只可能知道来源/节点/设备/会话，不知道 tabSlotId；
      // 定位与这四项一致时以定位里的 tabSlotId 为准，否则会把刷新误判成换分区。
      const effectiveScope: PreparationDraftScope =
        locator !== null && sameLocatorIdentity(locator, input.scope)
          ? { ...input.scope, tabSlotId: locator.tabSlotId }
          : input.scope;

      // 同一文档实例重复调用（例如页面 effect 重跑）不能再把自己判成复制标签页。
      if (
        this.#locatorOwnedInMemory &&
        locator !== null &&
        ownership !== null &&
        ownership.releasedAtMs === null &&
        ownership.instanceId === locator.instanceId &&
        sameLocatorScope(locator, effectiveScope)
      ) {
        await this.#writeOwnership(database, {
          ...ownership,
          heartbeatAtMs: input.nowMs,
          suspendedAtMs: null,
        });
        return {
          status: "owner",
          access: { scope: effectiveScope, instanceId: ownership.instanceId },
          locator,
          restorable: this.#locatorPersisted,
          restored: await this.#hasDraft(database, effectiveScope),
        };
      }

      const decision = decideTabClaim({
        locator,
        ownership,
        scope: effectiveScope,
        navigationKind,
      });

      if (decision.kind === "duplicate") {
        // 复制/opener 标签页：只清自己这份定位，绝不接管原准备或凭据。
        this.#clearLocator();
        return { status: "duplicate", reason: decision.reason };
      }

      if (decision.kind === "resume" && locator !== null && ownership !== null) {
        const resumed: PreparationTabOwnership = {
          ...ownership,
          heartbeatAtMs: input.nowMs,
          suspendedAtMs: null,
        };
        await this.#writeOwnership(database, resumed);
        const restored = await this.#hasDraft(database, effectiveScope);
        return {
          status: "owner",
          access: { scope: effectiveScope, instanceId: ownership.instanceId },
          locator,
          restorable: this.#locatorPersisted,
          restored,
        };
      }

      const tabSlotId =
        decision.kind === "reload_takeover" && locator !== null
          ? locator.tabSlotId
          : this.#newId();
      const instanceId = this.#newId();
      const nextScope: PreparationDraftScope = { ...input.scope, tabSlotId };
      const nextLocator: PreparationTabLocator = {
        version: PREPARATION_DRAFT_RECORD_VERSION,
        canonicalOrigin: nextScope.canonicalOrigin,
        ownerNodeId: nextScope.ownerNodeId,
        deviceId: nextScope.deviceId,
        tabSlotId,
        instanceId,
        sessionId: nextScope.sessionId,
        savedAtMs: input.nowMs,
      };
      const restorable = this.#writeLocator(nextLocator);
      const ownershipRecord: PreparationTabOwnership = {
        recordVersion: PREPARATION_DRAFT_RECORD_VERSION,
        canonicalOrigin: nextScope.canonicalOrigin,
        tabSlotId,
        instanceId,
        ownerNodeId: nextScope.ownerNodeId,
        deviceId: nextScope.deviceId,
        sessionId: nextScope.sessionId,
        claimedAtMs: input.nowMs,
        heartbeatAtMs: input.nowMs,
        suspendedAtMs: null,
        releasedAtMs: null,
      };
      await this.#writeOwnership(database, ownershipRecord);
      const restored =
        decision.kind === "reload_takeover"
          ? await this.#hasDraft(database, nextScope)
          : false;
      return {
        status: "owner",
        access: { scope: nextScope, instanceId },
        locator: restorable ? nextLocator : null,
        restorable,
        restored,
      };
    });
  }

  async #hasDraft(
    database: IDBDatabase,
    scope: PreparationDraftScope,
  ): Promise<boolean> {
    const raw = await this.#readValue(database, PREPARATION_DRAFT_STORE, preparationDraftKey(scope));
    const parsed = parseStoredDraft(raw);
    return parsed !== null && parsed.status !== "invalidated";
  }

  // ── 读取 ────────────────────────────────────────────────────────────────

  async loadDraft(input: {
    readonly access: PreparationDraftAccess;
    readonly nowMs: number;
  }): Promise<PreparationDraftLoadResult> {
    return this.#withDatabase(async (database) => {
      await this.#assertOwner(database, input.access);
      const raw = await this.#readValue(
        database,
        PREPARATION_DRAFT_STORE,
        preparationDraftKey(input.access.scope),
      );
      const stored = parseStoredDraft(raw);
      if (stored === null) return { status: "absent" };
      if (stored.status === "invalidated") {
        return {
          status: "invalidated",
          revision: stored.revision,
          reason: stored.reason,
          invalidatedAtMs: stored.invalidatedAtMs,
        };
      }
      if (stored.payload.status === "accepted") {
        return {
          status: "accepted",
          record: stored,
          association: stored.payload.association,
        };
      }
      return { status: "active", record: stored };
    });
  }

  // ── 保存（原子版本替换）─────────────────────────────────────────────────

  async saveDraft(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number | null;
    readonly content: PreparationDraftContent;
    readonly attachments?: readonly PreparationDraftAttachmentInput[];
    readonly nowMs: number;
  }): Promise<PreparationDraftSaveResult> {
    assertExpectedRevision(input.expectedRevision);
    assertPreparationDraftContent(input.content, input.access.scope);
    const attachmentInputs = input.attachments ?? [];
    const staged = this.#prepareAttachments(attachmentInputs);
    await this.#preflightQuota(input.content.text, staged);

    return this.#withDatabase(async (database) => {
      const ownership = await this.#assertOwner(database, input.access);
      const draftKey = preparationDraftKey(input.access.scope);
      await this.#writeOwnership(database, {
        ...ownership,
        heartbeatAtMs: input.nowMs,
      });
      const storageId = this.#newId();
      const intendedRevision = (input.expectedRevision ?? 0) + 1;

      try {
        for (const item of staged) {
          await this.#stageAttachment(
            database,
            draftKey,
            storageId,
            item,
            input.nowMs,
            intendedRevision,
          );
        }
      } catch (error) {
        // 清理失败不能让原始错误变形成功/另一种失败；残留由 24 小时孤儿清理回收。
        try {
          await this.#deleteChunksForStorage(database, storageId);
        } catch {
          // ignore
        }
        throw error;
      }

      let record: PreparationDraftRecord;
      try {
        record = await this.#commitDraftRecord(database, {
          scope: input.access.scope,
          expectedRevision: input.expectedRevision,
          content: input.content,
          attachments: staged.map((item) => this.#metadataOf(item)),
          storageId,
          nowMs: input.nowMs,
        });
      } catch (error) {
        try {
          await this.#deleteChunksForStorage(database, storageId);
        } catch {
          // ignore
        }
        throw error;
      }

      // 提交已成功；旧版本分块清理失败只留下无引用字节，由孤儿清理回收。
      try {
        await this.#deleteChunksForScope(database, draftKey, record.revision);
      } catch {
        // ignore
      }
      return {
        revision: record.revision,
        restorable:
          this.#locator !== null &&
          this.#locatorPersisted &&
          sameLocatorScope(this.#locator, input.access.scope),
      };
    });
  }

  #prepareAttachments(
    inputs: readonly PreparationDraftAttachmentInput[],
  ): readonly StagedAttachment[] {
    const seen = new Set<string>();
    const staged: StagedAttachment[] = [];
    for (const input of inputs) {
      assertPreparationAttachmentInput(input);
      if (seen.has(input.localId)) {
        throw new PreparationDraftError(
          "invalid_argument",
          `附件 localId 重复：${input.localId}`,
          [input.localId],
        );
      }
      seen.add(input.localId);
      const byteLength = parsePreparationByteLength(input.byteLength);
      staged.push({
        input,
        byteLength,
        chunkCount:
          byteLength === 0 ? 0 : Math.ceil(byteLength / PREPARATION_DRAFT_CHUNK_SIZE),
      });
    }
    return staged;
  }

  #metadataOf(item: StagedAttachment): PreparationDraftAttachment {
    return {
      localId: item.input.localId,
      kind: item.input.kind,
      fileName: item.input.fileName,
      mediaType: item.input.mediaType,
      byteLength: item.input.byteLength,
      chunkSize: PREPARATION_DRAFT_CHUNK_SIZE,
      chunkCount: item.chunkCount,
    };
  }

  async #stageAttachment(
    database: IDBDatabase,
    draftKey: string,
    storageId: string,
    item: StagedAttachment,
    nowMs: number,
    revision: number,
  ): Promise<void> {
    let offset = 0;
    let index = 0;
    while (offset < item.byteLength) {
      const length = Math.min(PREPARATION_DRAFT_CHUNK_SIZE, item.byteLength - offset);
      let chunk: Uint8Array;
      try {
        chunk = await item.input.readChunk(offset, length);
      } catch (error) {
        throw new PreparationDraftError(
          "attachment_unreadable",
          `读取附件 ${item.input.localId} 失败`,
          [item.input.localId],
        );
      }
      if (!(chunk instanceof Uint8Array)) {
        throw new PreparationDraftError(
          "attachment_incomplete",
          `附件 ${item.input.localId} 读取结果不是字节`,
          [item.input.localId],
        );
      }
      if (chunk.length !== length) {
        throw new PreparationDraftError(
          "attachment_incomplete",
          `附件 ${item.input.localId} 长度不足：期望 ${length}，实际 ${chunk.length}`,
          [item.input.localId],
        );
      }
      const value: PreparationStoredChunk = {
        scopeKey: draftKey,
        storageId,
        localId: item.input.localId,
        index,
        revision,
        // 复制一份：调用方可复用缓冲区，持久字节不得随后被改写。
        bytes: chunk.slice(),
        storedAtMs: nowMs,
      };
      try {
        await this.#putValue(
          database,
          PREPARATION_DRAFT_CHUNK_STORE,
          preparationChunkKey(draftKey, storageId, item.input.localId, index),
          value,
        );
      } catch (error) {
        throw this.#wrapWriteError(error, item.input.localId);
      }
      offset += length;
      index += 1;
    }
  }

  async #commitDraftRecord(
    database: IDBDatabase,
    input: {
      readonly scope: PreparationDraftScope;
      readonly expectedRevision: number | null;
      readonly content: PreparationDraftContent;
      readonly attachments: readonly PreparationDraftAttachment[];
      readonly storageId: string;
      readonly nowMs: number;
    },
  ): Promise<PreparationDraftRecord> {
    const key = preparationDraftKey(input.scope);
    return new Promise<PreparationDraftRecord>((resolve, reject) => {
      const transaction = database.transaction(PREPARATION_DRAFT_STORE, "readwrite");
      const store = transaction.objectStore(PREPARATION_DRAFT_STORE);
      const request = store.get(key);
      let failure: PreparationDraftError | null = null;
      let committed: PreparationDraftRecord | null = null;

      request.onsuccess = () => {
        try {
          const current = parseStoredDraft(request.result);
          if (current !== null) {
            if (current.status === "invalidated") {
              throw new PreparationDraftError(
                "invalidated",
                "草稿已作废，迟到的保存不能复活内容",
              );
            }
            if (current.status === "accepted") {
              throw new PreparationDraftError(
                "already_accepted",
                "首条已受理并清除正文，不能再保存新的准备正文",
              );
            }
            if (
              input.expectedRevision === null ||
              current.revision !== input.expectedRevision
            ) {
              throw new PreparationDraftError(
                "revision_conflict",
                `草稿版本不匹配：期望 ${String(input.expectedRevision)}，当前 ${current.revision}`,
              );
            }
          } else if (input.expectedRevision !== null) {
            throw new PreparationDraftError(
              "revision_conflict",
              `草稿不存在，无法匹配期望版本 ${input.expectedRevision}`,
            );
          }

          const record: PreparationDraftRecord = {
            recordVersion: PREPARATION_DRAFT_RECORD_VERSION,
            revision: (current?.revision ?? 0) + 1,
            scope: input.scope,
            status: "active",
            storageId: input.storageId,
            createdAtMs: current?.createdAtMs ?? input.nowMs,
            updatedAtMs: input.nowMs,
            payload: {
              status: "active",
              content: input.content,
              attachments: input.attachments,
            },
          };
          store.put(record, key);
          committed = record;
        } catch (error) {
          failure =
            error instanceof PreparationDraftError
              ? error
              : new PreparationDraftError("invalid_record", "草稿写入校验失败");
        }
      };
      request.onerror = () => {
        failure = new PreparationDraftError("storage_unavailable", "读取草稿版本失败");
      };
      transaction.oncomplete = () => {
        if (failure !== null) {
          reject(failure);
          return;
        }
        if (committed === null) {
          reject(new PreparationDraftError("storage_unavailable", "草稿提交未完成"));
          return;
        }
        resolve(committed);
      };
      transaction.onerror = () =>
        reject(storageFailure("提交草稿失败", transaction.error));
      transaction.onabort = () =>
        reject(storageFailure("提交草稿被中止", transaction.error));
    });
  }

  #wrapWriteError(error: unknown, localId?: string): PreparationDraftError {
    const ids = localId === undefined ? [] : [localId];
    if (isQuotaExceeded(error)) {
      return new PreparationDraftError(
        "quota_exceeded",
        "浏览器存储配额不足，未能保存该项；未提交部分内容",
        ids,
      );
    }
    if (error instanceof PreparationDraftError) {
      if (
        error.kind === "quota_exceeded" &&
        ids.length > 0 &&
        error.itemLocalIds.length === 0
      ) {
        return new PreparationDraftError(
          "quota_exceeded",
          "浏览器存储配额不足，未能保存该项；未提交部分内容",
          ids,
        );
      }
      return error;
    }
    return new PreparationDraftError("storage_unavailable", "写入准备草稿失败", ids);
  }

  async #preflightQuota(
    text: string,
    staged: readonly StagedAttachment[],
  ): Promise<void> {
    const estimate = await this.#quotaEstimate();
    if (estimate === null) return;
    const available = estimate.quota - estimate.usage;
    let used = new TextEncoder().encode(text).length + RECORD_OVERHEAD_BYTES;
    const failed: string[] = [];
    for (const item of staged) {
      const bytes = item.byteLength + item.chunkCount * CHUNK_OVERHEAD_BYTES;
      if (used + bytes > available) failed.push(item.input.localId);
      used += bytes;
    }
    if (failed.length > 0 || used > available) {
      throw new PreparationDraftError(
        "quota_exceeded",
        "浏览器存储配额不足，草稿未保存；未截断附件，也未改走服务端上传",
        failed,
      );
    }
  }

  async #deleteChunksForStorage(database: IDBDatabase, storageId: string): Promise<void> {
    const raw = await this.#readAll(database, PREPARATION_DRAFT_CHUNK_STORE);
    const keys: string[] = [];
    for (const value of raw) {
      const chunk = parseStoredChunk(value);
      if (chunk === null || chunk.storageId !== storageId) continue;
      keys.push(
        preparationChunkKey(chunk.scopeKey, chunk.storageId, chunk.localId, chunk.index),
      );
    }
    await this.#deleteValues(database, PREPARATION_DRAFT_CHUNK_STORE, keys);
  }

  async #deleteChunksForScope(
    database: IDBDatabase,
    draftKey: string,
    keepRevision?: number,
  ): Promise<void> {
    const raw = await this.#readAll(database, PREPARATION_DRAFT_CHUNK_STORE);
    const keys: string[] = [];
    for (const value of raw) {
      const chunk = parseStoredChunk(value);
      if (chunk === null || chunk.scopeKey !== draftKey) continue;
      // 有存活版本时只删严格更旧的版本；同版本（并发保存的败者）留给孤儿清理，
      // 这样提交后的清理不会删掉另一个仍在写的新版本分块。
      if (keepRevision !== undefined && chunk.revision >= keepRevision) continue;
      keys.push(
        preparationChunkKey(chunk.scopeKey, chunk.storageId, chunk.localId, chunk.index),
      );
    }
    await this.#deleteValues(database, PREPARATION_DRAFT_CHUNK_STORE, keys);
  }

  // ── 附件读取 ────────────────────────────────────────────────────────────

  async readAttachmentChunk(input: {
    readonly access: PreparationDraftAccess;
    readonly localId: string;
    readonly offset: number;
    readonly length: number;
  }): Promise<Uint8Array> {
    if (
      !Number.isSafeInteger(input.offset) ||
      input.offset < 0 ||
      !Number.isSafeInteger(input.length) ||
      input.length < 0
    ) {
      throw new PreparationDraftError("invalid_argument", "附件读取区间非法");
    }
    return this.#withDatabase(async (database) => {
      await this.#assertOwner(database, input.access);
      const record = await this.#activeRecord(database, input.access.scope);
      const attachment = record.payload.attachments.find(
        (item) => item.localId === input.localId,
      );
      if (attachment === undefined) {
        throw new PreparationDraftError("not_found", "附件不在当前草稿中", [input.localId]);
      }
      const storageId = record.storageId;
      if (storageId === null) {
        throw new PreparationDraftError("attachment_unreadable", "草稿附件存储缺失", [input.localId]);
      }
      const byteLength = parsePreparationByteLength(attachment.byteLength);
      const end = input.offset + input.length;
      if (!Number.isSafeInteger(end) || end > byteLength) {
        throw new PreparationDraftError("invalid_argument", "附件读取区间越界", [input.localId]);
      }
      if (input.length === 0) return new Uint8Array(0);
      return this.#readChunkRange(
        database,
        preparationDraftKey(input.access.scope),
        storageId,
        attachment,
        input.offset,
        input.length,
      );
    });
  }

  async readAttachment(input: {
    readonly access: PreparationDraftAccess;
    readonly localId: string;
  }): Promise<{ readonly attachment: PreparationDraftAttachment; readonly bytes: Uint8Array }> {
    return this.#withDatabase(async (database) => {
      await this.#assertOwner(database, input.access);
      const record = await this.#activeRecord(database, input.access.scope);
      const storageId = record.storageId;
      if (storageId === null) {
        throw new PreparationDraftError("attachment_unreadable", "草稿附件存储缺失", [input.localId]);
      }
      const attachment = record.payload.attachments.find(
        (item) => item.localId === input.localId,
      );
      if (attachment === undefined) {
        throw new PreparationDraftError("not_found", "附件不在当前草稿中", [input.localId]);
      }
      const byteLength = parsePreparationByteLength(attachment.byteLength);
      const bytes = new Uint8Array(byteLength);
      if (byteLength > 0) {
        const assembled = await this.#readChunkRange(
          database,
          preparationDraftKey(input.access.scope),
          storageId,
          attachment,
          0,
          byteLength,
        );
        bytes.set(assembled);
      }
      return { attachment, bytes };
    });
  }

  async #activeRecord(
    database: IDBDatabase,
    scope: PreparationDraftScope,
  ): Promise<ActivePreparationDraftRecord> {
    const raw = await this.#readValue(
      database,
      PREPARATION_DRAFT_STORE,
      preparationDraftKey(scope),
    );
    const stored = parseStoredDraft(raw);
    if (stored === null) throw new PreparationDraftError("not_found", "草稿不存在");
    if (stored.status === "invalidated") {
      throw new PreparationDraftError("invalidated", "草稿已作废");
    }
    if (stored.payload.status !== "active") {
      throw new PreparationDraftError("already_accepted", "草稿正文已清除");
    }
    // 边界断言：上面的状态检查保证 payload 为 active。
    return stored as ActivePreparationDraftRecord;
  }

  async #readChunkRange(
    database: IDBDatabase,
    draftKey: string,
    storageId: string,
    attachment: PreparationDraftAttachment,
    offset: number,
    length: number,
  ): Promise<Uint8Array> {
    const startIndex = Math.floor(offset / PREPARATION_DRAFT_CHUNK_SIZE);
    const endIndex = Math.floor((offset + length - 1) / PREPARATION_DRAFT_CHUNK_SIZE);
    const byteLength = parsePreparationByteLength(attachment.byteLength);
    const output = new Uint8Array(length);
    let written = 0;
    for (let index = startIndex; index <= endIndex; index += 1) {
      const raw = await this.#readValue(
        database,
        PREPARATION_DRAFT_CHUNK_STORE,
        preparationChunkKey(draftKey, storageId, attachment.localId, index),
      );
      const chunk = parseStoredChunk(raw);
      if (chunk === null || !(chunk.bytes instanceof Uint8Array)) {
        throw new PreparationDraftError(
          "attachment_unreadable",
          "草稿附件分块缺失或损坏",
          [attachment.localId],
        );
      }
      const chunkStart = index * PREPARATION_DRAFT_CHUNK_SIZE;
      const expectedLength = Math.min(PREPARATION_DRAFT_CHUNK_SIZE, byteLength - chunkStart);
      if (chunk.bytes.length !== expectedLength) {
        throw new PreparationDraftError(
          "attachment_unreadable",
          "草稿附件分块长度不符",
          [attachment.localId],
        );
      }
      const from = Math.max(offset, chunkStart);
      const to = Math.min(offset + length, chunkStart + expectedLength);
      const slice = chunk.bytes.subarray(from - chunkStart, to - chunkStart);
      output.set(slice, written);
      written += slice.length;
    }
    if (written !== length) {
      throw new PreparationDraftError(
        "attachment_unreadable",
        "草稿附件读取不完整",
        [attachment.localId],
      );
    }
    return output;
  }

  // ── 受理/终结/作废 ──────────────────────────────────────────────────────

  async markAccepted(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number;
    readonly association: PreparationRequestAssociation;
    readonly nowMs: number;
  }): Promise<{ readonly revision: number }> {
    assertExpectedRevision(input.expectedRevision);
    assertAssociation(input.association);
    return this.#withDatabase(async (database) => {
      await this.#assertOwner(database, input.access);
      const key = preparationDraftKey(input.access.scope);
      const revision = await this.#replaceWithAccepted(database, key, input);
      try {
        await this.#deleteChunksForScope(database, key);
      } catch {
        // 记录已不含附件引用；残留字节由孤儿清理回收。
      }
      return { revision };
    });
  }

  #replaceWithAccepted(
    database: IDBDatabase,
    key: string,
    input: {
      readonly expectedRevision: number;
      readonly association: PreparationRequestAssociation;
      readonly nowMs: number;
    },
  ): Promise<number> {
    return new Promise<number>((resolve, reject) => {
      const transaction = database.transaction(PREPARATION_DRAFT_STORE, "readwrite");
      const store = transaction.objectStore(PREPARATION_DRAFT_STORE);
      const request = store.get(key);
      let failure: PreparationDraftError | null = null;
      let revision: number | null = null;

      request.onsuccess = () => {
        try {
          const current = parseStoredDraft(request.result);
          if (current === null) throw new PreparationDraftError("not_found", "草稿不存在");
          if (current.status === "invalidated") {
            throw new PreparationDraftError("invalidated", "草稿已作废");
          }
          if (current.revision !== input.expectedRevision) {
            throw new PreparationDraftError("revision_conflict", "草稿版本不匹配");
          }
          if (current.payload.status !== "active") {
            throw new PreparationDraftError("already_accepted", "草稿已受理");
          }
          if (
            current.payload.content.pendingRequestId !== null &&
            current.payload.content.pendingRequestId !== input.association.requestId
          ) {
            throw new PreparationDraftError(
              "invalid_argument",
              "请求关联与草稿中未确认的 requestId 不一致",
            );
          }
          const next: PreparationDraftRecord = {
            recordVersion: PREPARATION_DRAFT_RECORD_VERSION,
            revision: current.revision + 1,
            scope: current.scope,
            status: "accepted",
            storageId: null,
            createdAtMs: current.createdAtMs,
            updatedAtMs: input.nowMs,
            payload: { status: "accepted", association: input.association },
          };
          store.put(next, key);
          revision = next.revision;
        } catch (error) {
          failure =
            error instanceof PreparationDraftError
              ? error
              : new PreparationDraftError("invalid_record", "受理记录写入校验失败");
        }
      };
      request.onerror = () => {
        failure = new PreparationDraftError("storage_unavailable", "读取草稿版本失败");
      };
      transaction.oncomplete = () => {
        if (failure !== null) {
          reject(failure);
          return;
        }
        if (revision === null) {
          reject(new PreparationDraftError("storage_unavailable", "受理记录未提交"));
          return;
        }
        resolve(revision);
      };
      transaction.onerror = () =>
        reject(new PreparationDraftError("storage_unavailable", "写入受理记录失败"));
      transaction.onabort = () =>
        reject(new PreparationDraftError("storage_unavailable", "写入受理记录被中止"));
    });
  }

  async completeRequest(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number;
    readonly requestId: string;
    readonly nowMs: number;
  }): Promise<void> {
    assertExpectedRevision(input.expectedRevision);
    if (typeof input.requestId !== "string" || !isUuid(input.requestId)) {
      throw new PreparationDraftError("invalid_argument", "requestId 必须是 UUID");
    }
    return this.#withDatabase(async (database) => {
      await this.#assertOwner(database, input.access);
      const key = preparationDraftKey(input.access.scope);
      await new Promise<void>((resolve, reject) => {
        const transaction = database.transaction(PREPARATION_DRAFT_STORE, "readwrite");
        const store = transaction.objectStore(PREPARATION_DRAFT_STORE);
        const request = store.get(key);
        let failure: PreparationDraftError | null = null;
        let deleted = false;

        request.onsuccess = () => {
          try {
            const current = parseStoredDraft(request.result);
            if (current === null) throw new PreparationDraftError("not_found", "恢复记录不存在");
            if (current.status === "invalidated") {
              throw new PreparationDraftError("invalidated", "草稿已作废");
            }
            if (current.revision !== input.expectedRevision) {
              throw new PreparationDraftError("revision_conflict", "恢复记录版本不匹配");
            }
            if (
              current.payload.status !== "accepted" ||
              current.payload.association.requestId !== input.requestId
            ) {
              throw new PreparationDraftError(
                "invalid_argument",
                "请求关联尚未终结或 requestId 不匹配",
              );
            }
            store.delete(key);
            deleted = true;
          } catch (error) {
            failure =
              error instanceof PreparationDraftError
                ? error
                : new PreparationDraftError("invalid_record", "恢复记录校验失败");
          }
        };
        request.onerror = () => {
          failure = new PreparationDraftError("storage_unavailable", "读取恢复记录失败");
        };
        transaction.oncomplete = () => {
          if (failure !== null) {
            reject(failure);
            return;
          }
          if (!deleted) {
            reject(new PreparationDraftError("storage_unavailable", "恢复记录未删除"));
            return;
          }
          resolve();
        };
        transaction.onerror = () =>
          reject(new PreparationDraftError("storage_unavailable", "删除恢复记录失败"));
        transaction.onabort = () =>
          reject(new PreparationDraftError("storage_unavailable", "删除恢复记录被中止"));
      });
    });
  }

  async invalidateDraft(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number | null;
    readonly reason: PreparationDraftInvalidationReason;
    readonly nowMs: number;
  }): Promise<{ readonly revision: number }> {
    assertExpectedRevision(input.expectedRevision);
    assertInvalidationReason(input.reason);
    return this.#withDatabase(async (database) => {
      await this.#assertOwner(database, input.access);
      const key = preparationDraftKey(input.access.scope);
      const revision = await this.#writeInvalidation(database, key, input);
      try {
        await this.#deleteChunksForScope(database, key);
      } catch {
        // 墓碑已经让内容逻辑作废；物理残留由孤儿清理回收。
      }
      return { revision };
    });
  }

  #writeInvalidation(
    database: IDBDatabase,
    key: string,
    input: {
      readonly access: PreparationDraftAccess;
      readonly expectedRevision: number | null;
      readonly reason: PreparationDraftInvalidationReason;
      readonly nowMs: number;
    },
  ): Promise<number> {
    return new Promise<number>((resolve, reject) => {
      const transaction = database.transaction(PREPARATION_DRAFT_STORE, "readwrite");
      const store = transaction.objectStore(PREPARATION_DRAFT_STORE);
      const request = store.get(key);
      let failure: PreparationDraftError | null = null;
      let revision: number | null = null;

      request.onsuccess = () => {
        try {
          const current = parseStoredDraft(request.result);
          if (current !== null && current.status === "invalidated") {
            if (
              input.expectedRevision !== null &&
              input.expectedRevision !== current.revision
            ) {
              throw new PreparationDraftError("revision_conflict", "草稿版本不匹配");
            }
            revision = current.revision;
            return;
          }
          if (current !== null && current.status === "accepted") {
            throw new PreparationDraftError(
              "already_accepted",
              "已受理的请求关联只能在命令终结后删除",
            );
          }
          if (current !== null && current.revision !== input.expectedRevision) {
            throw new PreparationDraftError(
              "revision_conflict",
              `草稿版本不匹配：期望 ${String(input.expectedRevision)}，当前 ${current.revision}`,
            );
          }
          if (current === null && input.expectedRevision !== null) {
            throw new PreparationDraftError("revision_conflict", "草稿已不存在");
          }
          const next: PreparationStoredDraft = {
            recordVersion: PREPARATION_DRAFT_RECORD_VERSION,
            revision: (current?.revision ?? 0) + 1,
            scope: input.access.scope,
            status: "invalidated",
            reason: input.reason,
            invalidatedAtMs: input.nowMs,
          };
          store.put(next, key);
          revision = next.revision;
        } catch (error) {
          failure =
            error instanceof PreparationDraftError
              ? error
              : new PreparationDraftError("invalid_record", "作废记录写入校验失败");
        }
      };
      request.onerror = () => {
        failure = new PreparationDraftError("storage_unavailable", "读取草稿版本失败");
      };
      transaction.oncomplete = () => {
        if (failure !== null) {
          reject(failure);
          return;
        }
        if (revision === null) {
          reject(new PreparationDraftError("storage_unavailable", "作废记录未提交"));
          return;
        }
        resolve(revision);
      };
      transaction.onerror = () =>
        reject(new PreparationDraftError("storage_unavailable", "写入作废记录失败"));
      transaction.onabort = () =>
        reject(new PreparationDraftError("storage_unavailable", "写入作废记录被中止"));
    });
  }

  // ── 生命周期/归属 ───────────────────────────────────────────────────────

  async noteLifecycle(input: {
    readonly access: PreparationDraftAccess;
    readonly event: PreparationLifecycleEvent;
    readonly nowMs: number;
  }): Promise<void> {
    assertLifecycleEvent(input.event);
    return this.#withDatabase(async (database) => {
      const ownership = await this.#assertOwner(database, input.access);
      const suspended =
        input.event === "visibility_hidden" || input.event === "frozen";
      await this.#writeOwnership(database, {
        ...ownership,
        heartbeatAtMs: input.nowMs,
        suspendedAtMs: suspended ? input.nowMs : null,
      });
    });
  }

  async releaseTab(input: {
    readonly access: PreparationDraftAccess;
    readonly nowMs: number;
  }): Promise<void> {
    return this.#withDatabase(async (database) => {
      const ownership = await this.#assertOwner(database, input.access, true);
      await this.#writeOwnership(database, {
        ...ownership,
        heartbeatAtMs: input.nowMs,
        suspendedAtMs: null,
        releasedAtMs: ownership.releasedAtMs ?? input.nowMs,
      });
      if (
        this.#locator !== null &&
        this.#locator.tabSlotId === input.access.scope.tabSlotId &&
        this.#locator.instanceId === input.access.instanceId
      ) {
        this.#clearLocator();
      }
    });
  }

  // ── 清理 ────────────────────────────────────────────────────────────────

  async cleanupOrphans(input: {
    readonly nowMs: number;
  }): Promise<PreparationDraftCleanupReport> {
    return this.#withDatabase(async (database) => {
      const rawDrafts = await this.#readAll(database, PREPARATION_DRAFT_STORE);
      const drafts: PreparationStoredDraft[] = [];
      const allDraftKeys = new Set<string>();
      for (const value of rawDrafts) {
        const parsed = parseStoredDraft(value);
        if (parsed === null) continue;
        drafts.push(parsed);
        allDraftKeys.add(preparationDraftKey(parsed.scope));
      }
      const rawOwnership = await this.#readAll(
        database,
        PREPARATION_DRAFT_OWNERSHIP_STORE,
      );
      const ownership: PreparationTabOwnership[] = [];
      for (const value of rawOwnership) {
        const parsed = parseOwnership(value);
        if (parsed !== null) ownership.push(parsed);
      }

      const plan = orphanCleanupPlan({ drafts, ownership, nowMs: input.nowMs });
      const removedDrafts = new Set(plan.draftKeys);
      const survivors = new Map<string, PreparationDraftRecord>();
      for (const draft of drafts) {
        if (draft.status !== "active" || removedDrafts.has(preparationDraftKey(draft.scope))) {
          continue;
        }
        survivors.set(preparationDraftKey(draft.scope), draft);
      }

      await this.#deleteValues(database, PREPARATION_DRAFT_STORE, plan.draftKeys);
      await this.#deleteValues(
        database,
        PREPARATION_DRAFT_OWNERSHIP_STORE,
        plan.ownershipKeys,
      );

      const rawChunks = await this.#readAll(database, PREPARATION_DRAFT_CHUNK_STORE);
      const chunkKeys: string[] = [];
      for (const value of rawChunks) {
        const chunk = parseStoredChunk(value);
        if (chunk === null) continue;
        const key = preparationChunkKey(
          chunk.scopeKey,
          chunk.storageId,
          chunk.localId,
          chunk.index,
        );
        if (removedDrafts.has(chunk.scopeKey)) {
          chunkKeys.push(key);
          continue;
        }
        const survivor = survivors.get(chunk.scopeKey);
        if (survivor !== undefined) {
          if (chunk.revision < survivor.revision) {
            chunkKeys.push(key);
            continue;
          }
          if (
            chunk.storageId !== survivor.storageId &&
            input.nowMs - chunk.storedAtMs >= PREPARATION_DRAFT_ORPHAN_WINDOW_MS
          ) {
            chunkKeys.push(key);
          }
          continue;
        }
        if (
          !allDraftKeys.has(chunk.scopeKey) &&
          input.nowMs - chunk.storedAtMs >= PREPARATION_DRAFT_ORPHAN_WINDOW_MS
        ) {
          chunkKeys.push(key);
        }
        // 记录存在但不是 active（已受理/墓碑）：正文已清除，残留块可直接回收。
        if (allDraftKeys.has(chunk.scopeKey)) chunkKeys.push(key);
      }
      await this.#deleteValues(database, PREPARATION_DRAFT_CHUNK_STORE, chunkKeys);

      return {
        draftsDeleted: plan.draftKeys.length,
        ownershipDeleted: plan.ownershipKeys.length,
        chunksDeleted: chunkKeys.length,
      };
    });
  }
}

// ── 纯校验/解析辅助 ────────────────────────────────────────────────────────

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const CANONICAL_ORIGIN_PATTERN = /^https:\/\/[^/?#]+$/;

function isUuid(value: unknown): value is string {
  return typeof value === "string" && UUID_PATTERN.test(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function invalidRecord(message: string): PreparationDraftError {
  return new PreparationDraftError("invalid_record", message);
}

function parseTabLocator(value: unknown): PreparationTabLocator | null {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  if (record["version"] !== PREPARATION_DRAFT_RECORD_VERSION) return null;
  const origin = record["canonicalOrigin"];
  if (typeof origin !== "string" || !CANONICAL_ORIGIN_PATTERN.test(origin)) {
    return null;
  }
  for (const key of ["ownerNodeId", "deviceId", "tabSlotId", "instanceId", "sessionId"] as const) {
    if (!isUuid(record[key])) return null;
  }
  if (!isFiniteNumber(record["savedAtMs"])) return null;
  return value as PreparationTabLocator;
}

function parseStoredDraft(value: unknown): PreparationStoredDraft | null {
  if (value === null || value === undefined) return null;
  if (typeof value !== "object" || Array.isArray(value)) throw invalidRecord("草稿记录不是对象");
  const record = value as Record<string, unknown>;
  if (record["recordVersion"] !== PREPARATION_DRAFT_RECORD_VERSION) {
    throw invalidRecord("草稿记录版本未知，拒绝恢复");
  }
  const revision = record["revision"];
  if (
    typeof revision !== "number" ||
    !Number.isSafeInteger(revision) ||
    revision <= 0
  ) {
    throw invalidRecord("草稿记录版本号非法");
  }
  const scopeValue = record["scope"];
  try {
    assertPreparationDraftScope(scopeValue);
  } catch {
    throw invalidRecord("草稿分区非法");
  }
  // 断言在 try 内完成；这里沿用同一检查结果，避免把未校验值继续当分区使用。
  const scope = scopeValue as PreparationDraftScope;
  const status = record["status"];
  if (status === "invalidated") {
    assertInvalidationReason(record["reason"]);
    if (!isFiniteNumber(record["invalidatedAtMs"])) {
      throw invalidRecord("草稿墓碑时间非法");
    }
    return value as PreparationStoredDraft;
  }
  if (status !== "active" && status !== "accepted") {
    throw invalidRecord("草稿状态非法");
  }
  if (!isFiniteNumber(record["createdAtMs"]) || !isFiniteNumber(record["updatedAtMs"])) {
    throw invalidRecord("草稿时间非法");
  }
  if (status === "active" && typeof record["storageId"] !== "string") {
    throw invalidRecord("草稿存储标识缺失");
  }
  if (status === "accepted" && record["storageId"] !== null) {
    throw invalidRecord("已受理记录不应再引用附件分块");
  }
  const payload = record["payload"];
  if (payload === null || typeof payload !== "object" || Array.isArray(payload)) {
    throw invalidRecord("草稿载荷非法");
  }
  const payloadRecord = payload as Record<string, unknown>;
  if (payloadRecord["status"] === "active") {
    if (status !== "active") throw invalidRecord("草稿载荷状态不一致");
    assertPreparationDraftContent(payloadRecord["content"], scope);
    const attachmentsValue = payloadRecord["attachments"];
    if (!Array.isArray(attachmentsValue)) throw invalidRecord("草稿附件清单非法");
    const attachments: unknown[] = attachmentsValue;
    const seen = new Set<string>();
    for (const attachment of attachments) {
      assertPreparationDraftAttachment(attachment);
      if (seen.has(attachment.localId)) throw invalidRecord("草稿附件 localId 重复");
      seen.add(attachment.localId);
    }
  } else if (payloadRecord["status"] === "accepted") {
    if (status !== "accepted") throw invalidRecord("草稿载荷状态不一致");
    assertAssociation(payloadRecord["association"]);
  } else {
    throw invalidRecord("草稿载荷状态未知");
  }
  return value as PreparationStoredDraft;
}

function parseOwnership(value: unknown): PreparationTabOwnership | null {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  if (record["recordVersion"] !== PREPARATION_DRAFT_RECORD_VERSION) return null;
  const origin = record["canonicalOrigin"];
  if (typeof origin !== "string" || !CANONICAL_ORIGIN_PATTERN.test(origin)) {
    return null;
  }
  for (const key of [
    "tabSlotId",
    "instanceId",
    "ownerNodeId",
    "deviceId",
    "sessionId",
  ] as const) {
    if (!isUuid(record[key])) return null;
  }
  if (
    !isFiniteNumber(record["claimedAtMs"]) ||
    !isFiniteNumber(record["heartbeatAtMs"])
  ) {
    return null;
  }
  if (record["suspendedAtMs"] !== null && !isFiniteNumber(record["suspendedAtMs"])) {
    return null;
  }
  if (record["releasedAtMs"] !== null && !isFiniteNumber(record["releasedAtMs"])) {
    return null;
  }
  return value as PreparationTabOwnership;
}

function parseStoredChunk(value: unknown): PreparationStoredChunk | null {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  const index = record["index"];
  const revision = record["revision"];
  if (
    typeof record["scopeKey"] !== "string" ||
    typeof record["storageId"] !== "string" ||
    typeof record["localId"] !== "string" ||
    typeof index !== "number" ||
    !Number.isSafeInteger(index) ||
    index < 0 ||
    typeof revision !== "number" ||
    !Number.isSafeInteger(revision) ||
    revision <= 0 ||
    !(record["bytes"] instanceof Uint8Array) ||
    !isFiniteNumber(record["storedAtMs"])
  ) {
    return null;
  }
  return value as PreparationStoredChunk;
}

function assertExpectedRevision(value: number | null): void {
  if (value === null) return;
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value <= 0) {
    throw new PreparationDraftError("invalid_argument", "expectedRevision 必须是正安全整数或 null");
  }
}

function assertAssociation(
  value: unknown,
): asserts value is PreparationRequestAssociation {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new PreparationDraftError("invalid_argument", "请求关联必须是对象");
  }
  const record = value as Record<string, unknown>;
  const allowed = ["requestId", "firstSubmissionId", "firstMessageId", "acceptedAtMs"];
  for (const key of Object.keys(record)) {
    if (!allowed.includes(key)) {
      throw new PreparationDraftError("invalid_argument", `请求关联不允许字段 ${key}`);
    }
  }
  if (!isUuid(record["requestId"])) {
    throw new PreparationDraftError("invalid_argument", "requestId 必须是 UUID");
  }
  for (const key of ["firstSubmissionId", "firstMessageId"] as const) {
    if (record[key] !== null && !isUuid(record[key])) {
      throw new PreparationDraftError("invalid_argument", `${key} 必须是 UUID 或 null`);
    }
  }
  if (!isFiniteNumber(record["acceptedAtMs"])) {
    throw new PreparationDraftError("invalid_argument", "acceptedAtMs 必须是数字");
  }
}

function assertInvalidationReason(
  value: unknown,
): asserts value is PreparationDraftInvalidationReason {
  if (
    value !== "navigation" &&
    value !== "agent_changed" &&
    value !== "directory_changed" &&
    value !== "replaced"
  ) {
    throw new PreparationDraftError("invalid_argument", "草稿作废原因非法");
  }
}

function assertLifecycleEvent(value: unknown): asserts value is PreparationLifecycleEvent {
  if (
    value !== "visibility_hidden" &&
    value !== "visibility_visible" &&
    value !== "frozen" &&
    value !== "resumed"
  ) {
    throw new PreparationDraftError("invalid_argument", "生命周期事件非法");
  }
}

function sameLocatorIdentity(
  locator: PreparationTabLocator,
  scope: PreparationDraftScope,
): boolean {
  return (
    locator.canonicalOrigin === scope.canonicalOrigin &&
    locator.ownerNodeId === scope.ownerNodeId &&
    locator.deviceId === scope.deviceId &&
    locator.sessionId === scope.sessionId
  );
}

function sameLocatorScope(
  locator: PreparationTabLocator,
  scope: PreparationDraftScope,
): boolean {
  return sameLocatorIdentity(locator, scope) && locator.tabSlotId === scope.tabSlotId;
}

function isQuotaExceeded(error: unknown): boolean {
  if (typeof error !== "object" || error === null || !("name" in error)) return false;
  return typeof error.name === "string" && error.name === "QuotaExceededError";
}

function storageFailure(
  message: string,
  cause: unknown,
): PreparationDraftError {
  if (isQuotaExceeded(cause)) {
    return new PreparationDraftError(
      "quota_exceeded",
      "浏览器存储配额不足，草稿未保存；未截断附件",
    );
  }
  return new PreparationDraftError("storage_unavailable", message);
}
