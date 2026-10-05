/**
 * 本地缓存端口的 **Web/PWA** 实现（R20）。
 *
 * 归属 WP5b。持久层是 IndexedDB，淘汰策略复用 `local-cache.ts` 的纯函数
 * （`evictToFit`），因此「固定上限 + TTL + LRU」可在 Node 测试里直接断言，
 * 而不必把策略绑在浏览器事务上。
 *
 * imported 正文**没有**落盘路径：本文件把 imported 正文只放进一个内存 `Map`
 * （{@link createVolatileImportedContent}），并且持久侧的写入入口
 * （`putImportedMetadata`）只接受 {@link ImportedMetadata}——它按类型就没有正文字段。
 *
 * v1 只交付 Web：**不写** `.native.ts`（`tasks.md` 2.6）。
 */

import {
  IMPORTED_METADATA_MAX_BYTES,
  IMPORTED_METADATA_TTL_MS,
  SUMMARY_CACHE_MAX_BYTES,
  SUMMARY_CACHE_TTL_MS,
  type CacheEntry,
  type CacheHit,
  type CacheStats,
  type ImportedMetadata,
  type LocalCachePort,
  type VolatileImportedContent,
  estimateEntryBytes,
  evictToFit,
} from "./local-cache";

/** IndexedDB 数据库名。 */
const DATABASE_NAME = "acp-remote-cache-v1";

/** 对象存储：持久缓存条目。 */
const CACHE_STORE = "entries";

/** imported 正文的内存载体：进程内，刷新即失效。 */
export function createVolatileImportedContent(): VolatileImportedContent {
  const contents = new Map<string, string>();
  return {
    put(key, content) {
      contents.set(key, content);
    },
    get(key) {
      return contents.get(key) ?? null;
    },
    drop(key) {
      contents.delete(key);
    },
    clear() {
      contents.clear();
    },
    keys() {
      return [...contents.keys()];
    },
  };
}

/** 打开持久缓存端口。 */
export function openLocalCache(): LocalCachePort {
  return new WebLocalCache();
}

class WebLocalCache implements LocalCachePort {
  #openDatabase(): Promise<IDBDatabase> {
    if (typeof indexedDB === "undefined") {
      return Promise.reject(new Error("IndexedDB 不可用（非安全上下文或浏览器不支持）"));
    }
    // IndexedDB 只提供回调式事件，executor 形式是唯一可用形状。
    return new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open(DATABASE_NAME, 1);
      request.onupgradeneeded = () => {
        const database = request.result;
        if (!database.objectStoreNames.contains(CACHE_STORE)) {
          database.createObjectStore(CACHE_STORE);
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(new Error("打开缓存数据库失败"));
    });
  }

  /** 读全部条目（配额收敛需要全量视角）。 */
  #readAll(database: IDBDatabase): Promise<CacheEntry[]> {
    return new Promise((resolve, reject) => {
      const transaction = database.transaction(CACHE_STORE, "readonly");
      const store = transaction.objectStore(CACHE_STORE);
      const request = store.openCursor();
      const entries: CacheEntry[] = [];
      request.onsuccess = () => {
        const cursor = request.result;
        if (cursor !== null) {
          entries.push(cursor.value as CacheEntry);
          cursor.continue();
          return;
        }
        resolve(entries);
      };
      request.onerror = () => reject(new Error("读取缓存条目失败"));
    });
  }

  /** 覆盖写回：把收敛后的集合同步为持久状态。 */
  #replaceAll(database: IDBDatabase, entries: readonly CacheEntry[]): Promise<void> {
    return new Promise((resolve, reject) => {
      const transaction = database.transaction(CACHE_STORE, "readwrite");
      const store = transaction.objectStore(CACHE_STORE);
      store.clear();
      for (const entry of entries) store.put(entry, entry.key);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => reject(new Error("写回缓存条目失败"));
      transaction.onabort = () => reject(new Error("写回缓存条目被中止"));
    });
  }

  /** 按配额与 TTL 收敛后落盘。 */
  async #commit(database: IDBDatabase, candidate: CacheEntry, nowMs: number): Promise<void> {
    const all = (await this.#readAll(database)).filter((entry) => entry.key !== candidate.key);
    all.push(candidate);
    const local = all.filter((entry) => entry.kind === "local");
    const imported = all.filter((entry) => entry.kind === "imported");
    const keptLocal = evictToFit({
      entries: local,
      maxBytes: SUMMARY_CACHE_MAX_BYTES,
      ttlMs: SUMMARY_CACHE_TTL_MS,
      nowMs,
    }).kept;
    const keptImported = evictToFit({
      entries: imported,
      maxBytes: IMPORTED_METADATA_MAX_BYTES,
      ttlMs: IMPORTED_METADATA_TTL_MS,
      nowMs,
    }).kept;
    await this.#replaceAll(database, [...keptLocal, ...keptImported]);
  }

  async putLocalSummary(input: {
    readonly key: string;
    readonly summary: string;
    readonly nowMs: number;
  }): Promise<void> {
    const entry: CacheEntry = {
      kind: "local",
      key: input.key,
      summary: input.summary,
      updatedAtMs: input.nowMs,
      lastUsedAtMs: input.nowMs,
    };
    const database = await this.#openDatabase();
    try {
      await this.#commit(database, entry, input.nowMs);
    } finally {
      database.close();
    }
  }

  async putImportedMetadata(input: {
    readonly key: string;
    readonly metadata: ImportedMetadata;
    readonly nowMs: number;
  }): Promise<void> {
    const entry: CacheEntry = {
      kind: "imported",
      key: input.key,
      metadata: input.metadata,
      updatedAtMs: input.nowMs,
      lastUsedAtMs: input.nowMs,
    };
    const database = await this.#openDatabase();
    try {
      await this.#commit(database, entry, input.nowMs);
    } finally {
      database.close();
    }
  }

  async get(key: string, nowMs: number): Promise<CacheHit | null> {
    const database = await this.#openDatabase();
    let entry: CacheEntry | undefined;
    try {
      entry = (await this.#readAll(database)).find((candidate) => candidate.key === key);
      if (entry === undefined) return null;
      const ttlMs = entry.kind === "local" ? SUMMARY_CACHE_TTL_MS : IMPORTED_METADATA_TTL_MS;
      const remainingTtlMs = ttlMs - (nowMs - entry.updatedAtMs);
      if (remainingTtlMs <= 0) {
        // 过期即丢弃；读取路径不返回过期数据。
        await this.#replaceAll(
          database,
          (await this.#readAll(database)).filter((candidate) => candidate.key !== key),
        );
        return null;
      }
      const touched: CacheEntry = { ...entry, lastUsedAtMs: nowMs };
      await this.#replaceAll(
        database,
        (await this.#readAll(database)).map((candidate) =>
          candidate.key === key ? touched : candidate,
        ),
      );
      return { entry: touched, remainingTtlMs };
    } finally {
      database.close();
    }
  }

  async clear(): Promise<void> {
    const database = await this.#openDatabase();
    try {
      await this.#replaceAll(database, []);
    } finally {
      database.close();
    }
  }

  async stats(nowMs: number): Promise<CacheStats> {
    const database = await this.#openDatabase();
    try {
      const entries = await this.#readAll(database);
      let localEntries = 0;
      let localBytes = 0;
      let importedEntries = 0;
      let importedBytes = 0;
      let expiredEntries = 0;
      for (const entry of entries) {
        const bytes = estimateEntryBytes(entry);
        const ttlMs = entry.kind === "local" ? SUMMARY_CACHE_TTL_MS : IMPORTED_METADATA_TTL_MS;
        if (nowMs - entry.updatedAtMs >= ttlMs) expiredEntries += 1;
        if (entry.kind === "local") {
          localEntries += 1;
          localBytes += bytes;
        } else {
          importedEntries += 1;
          importedBytes += bytes;
        }
      }
      return { localEntries, localBytes, importedEntries, importedBytes, expiredEntries };
    } finally {
      database.close();
    }
  }
}
