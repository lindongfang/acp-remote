/**
 * 本地缓存端口的 **Web/PWA** 实现（R20）。
 *
 * 归属 WP5b。持久层是 IndexedDB，淘汰策略复用 `local-cache.shared.ts` 的纯函数
 * （`evictToFit`），因此「固定上限 + TTL + LRU」可在 Node 测试里直接断言，
 * 而不必把策略绑在浏览器事务上。
 *
 * **只持久化本节点自有会话的精简摘要。** 远程资源、正文、标题、来源游标、ACK 与命令状态
 * 都只存在于运行期内存（`features/client-store.ts` 的远程运行期模型），本文件没有任何
 * 接收它们的入口；`assertLocalSummary` 在运行期拒绝夹带 `metadata` 的写入。
 *
 * v1 只交付 Web：**不写** `.native.ts`（`tasks.md` 2.6）。
 */

import {
  SUMMARY_CACHE_MAX_BYTES,
  SUMMARY_CACHE_TTL_MS,
  type CacheEntry,
  type CacheHit,
  type CacheStats,
  type LocalCachePort,
  assertLocalSummary,
  estimateEntryBytes,
  evictToFit,
} from "./local-cache.shared";

// 转出共享声明与纯策略：`platform/index.ts` 在 Web 平台从 `./local-cache` 取到的就是本文件，
// 因此这些符号必须在这里可见（否则会取到 `undefined`，见 `local-cache.shared.ts` 的说明）。
export * from "./local-cache.shared";

/** IndexedDB 数据库名。 */
const DATABASE_NAME = "acp-remote-cache-v1";

/** 对象存储：持久缓存条目。 */
const CACHE_STORE = "entries";

/** 打开持久缓存端口。 */
export function openLocalCache(): LocalCachePort {
  return new WebLocalCache();
}

class WebLocalCache implements LocalCachePort {
  /**
   * 写路径串行化队列（F2）。
   *
   * `#commit` 与 `get` 的过期/命中回写都是**全量读-改-写**（`#readAll` → 收敛 → `#replaceAll`），
   * 两次重叠操作会在读与写之间交错，后写者以自己读到的旧快照覆盖先写者已落盘的条目
   * （`putLocalSummary` 并发时二者只存其一；`clear()` 与 `put` 并发时 `clear()` 会被抢跑留下
   * 残余）。同一实例内的**持久写操作**因此必须串行：这里用一条 promise 链把每个写操作接在
   * 上一个之后，使「读快照 → 收敛 → 原子写回」成为临界区。
   *
   * 只串行化会改持久状态的操作（两个 `put`/`clear` 与 `get` 的回写）；只读的 `stats` 不必排队。
   */
  #writeChain: Promise<unknown> = Promise.resolve();

  /** 把 `operation` 接到串行队列尾部；前序失败不阻断后续操作。 */
  #serialize<T>(operation: () => Promise<T>): Promise<T> {
    const run = this.#writeChain.then(operation, operation);
    this.#writeChain = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  }

  #openDatabase(): Promise<IDBDatabase> {
    if (typeof indexedDB === "undefined") {
      return Promise.reject(new Error("IndexedDB 不可用（非安全上下文或浏览器不支持）"));
    }
    // IndexedDB 只提供回调式事件，executor 形式是唯一可用形状。
    return new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open(DATABASE_NAME, 2);
      request.onupgradeneeded = () => {
        const database = request.result;
        // v1 允许 imported 元数据；v2 起该形状不再属于本端口，升级时整表清空，
        // 不把远程标识/游标留在磁盘上等下一次写回。
        if (!database.objectStoreNames.contains(CACHE_STORE)) {
          database.createObjectStore(CACHE_STORE);
        } else if (request.transaction !== null) {
          request.transaction.objectStore(CACHE_STORE).clear();
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
          // 旧版本可能留下 imported 元数据记录（无 `summary`）：它们不再属于本端口，
          // 读取时忽略；下一次写回会整表替换、把它们清掉。
          const value = cursor.value as Partial<CacheEntry>;
          if (typeof value.key === "string" && typeof value.summary === "string" &&
            typeof value.updatedAtMs === "number" && typeof value.lastUsedAtMs === "number") {
            entries.push(value as CacheEntry);
          }
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
    const kept = evictToFit({
      entries: all,
      maxBytes: SUMMARY_CACHE_MAX_BYTES,
      ttlMs: SUMMARY_CACHE_TTL_MS,
      nowMs,
    }).kept;
    await this.#replaceAll(database, kept);
  }

  async putLocalSummary(input: {
    readonly key: string;
    readonly summary: string;
    readonly nowMs: number;
  }): Promise<void> {
    // R20 的落盘守门必须真的接线：转发**整个**入参（而非只转发得到类型许可的 `summary`），
    // 结构化类型允许更宽的对象经变量流入，多带的 `metadata` 必须在运行期被拒绝。
    assertLocalSummary(input);
    const entry: CacheEntry = {
      key: input.key,
      summary: input.summary,
      updatedAtMs: input.nowMs,
      lastUsedAtMs: input.nowMs,
    };
    await this.#serialize(async () => {
      const database = await this.#openDatabase();
      try {
        await this.#commit(database, entry, input.nowMs);
      } finally {
        database.close();
      }
    });
  }

  async get(key: string, nowMs: number): Promise<CacheHit | null> {
    return this.#serialize(async () => {
      const database = await this.#openDatabase();
      let entry: CacheEntry | undefined;
      try {
        entry = (await this.#readAll(database)).find((candidate) => candidate.key === key);
        if (entry === undefined) return null;
        const remainingTtlMs = SUMMARY_CACHE_TTL_MS - (nowMs - entry.updatedAtMs);
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
    });
  }

  async clear(): Promise<void> {
    return this.#serialize(async () => {
      const database = await this.#openDatabase();
      try {
        await this.#replaceAll(database, []);
      } finally {
        database.close();
      }
    });
  }

  async stats(nowMs: number): Promise<CacheStats> {
    const database = await this.#openDatabase();
    try {
      const entries = await this.#readAll(database);
      let bytes = 0;
      let expiredEntries = 0;
      for (const entry of entries) {
        bytes += estimateEntryBytes(entry);
        if (nowMs - entry.updatedAtMs >= SUMMARY_CACHE_TTL_MS) expiredEntries += 1;
      }
      return { entries: entries.length, bytes, expiredEntries };
    } finally {
      database.close();
    }
  }
}
