/**
 * 本地缓存与生命周期端口的**声明**（平台侧）。
 *
 * 归属 WP5b。本模块只有接口、固定常量与**纯策略**（LRU/TTL/配额），不触碰任何平台 API——
 * IndexedDB 只在 `local-cache.web.ts`，`document`/`window` 只在 `lifecycle.web.ts`。
 *
 * **为什么共享声明住在这个名字里**：Metro 在 Web 平台把 `./local-cache` 解析到
 * `local-cache.web.ts`（平台扩展名优先），因此 `.web.ts` 与 `index.ts` 里那句「从 `./local-cache`
 * 取共享符号」在**真实 Web 产物**里会解析到 Web 实现本身，取到的值是 `undefined`
 * （实测：配对写缓存时报 `assertLocalSummary is not a function`；单测与假主机 e2e 都走
 * 另一套解析，抓不到）。共享声明必须住在一个**没有平台变体**的模块里，两个实现各自从它转出。
 *
 * ## 只保留本节点自有资源的摘要
 *
 * 远程资源、正文、标题、来源游标、ACK 与命令状态都只在运行期内存中存在，**没有**任何持久化
 * 入口（`docs/FRONTEND_DESIGN.md` §7、`design.md` D1）。缓存端口因此只接受本节点自有会话的
 * 精简摘要文本，并按固定容量与 TTL 淘汰；本机设备身份与已配对主机记录使用各自的平台端口，
 * 不混入摘要配额。
 */

/** 节点自有会话摘要缓存的固定上限（`docs/FRONTEND_DESIGN.md` §7）。 */
export const SUMMARY_CACHE_MAX_BYTES = 8 * 1024 * 1024;

/** 节点自有会话摘要缓存的固定存活期：30 天（`docs/FRONTEND_DESIGN.md` §7）。 */
export const SUMMARY_CACHE_TTL_MS = 30 * 24 * 60 * 60 * 1000;

/**
 * 一条可持久化的缓存条目。
 *
 * 形状本身就把「远程正文落盘」变成不可能构造的值：没有 `origin`/`kind` 分支，也没有任何
 * 正文、游标或命令字段。
 */
export interface CacheEntry {
  readonly key: string;
  /** 精简摘要文本（只用于本节点自有会话）。 */
  readonly summary: string;
  readonly updatedAtMs: number;
  readonly lastUsedAtMs: number;
}

/** 缓存读命中的条目。 */
export interface CacheHit {
  readonly entry: CacheEntry;
  /** 距离 TTL 过期的剩余毫秒；已过期时为 0。 */
  readonly remainingTtlMs: number;
}

/** 持久缓存端口。 */
export interface LocalCachePort {
  /** 写入一条本机摘要条目；超容量时按 LRU 淘汰到配额以内。 */
  putLocalSummary(input: {
    readonly key: string;
    readonly summary: string;
    readonly nowMs: number;
  }): Promise<void>;

  /** 读取一条未过期条目（命中时刷新 `lastUsedAtMs`）。 */
  get(key: string, nowMs: number): Promise<CacheHit | null>;

  /** 清除全部持久缓存（R20 场景 4「清除数据」）。 */
  clear(): Promise<void>;

  /** 提示诊断信息：条目数、字节数、过期数。 */
  stats(nowMs: number): Promise<CacheStats>;
}

/** 缓存诊断（只统计本机摘要；远程资源不在持久层，因此没有对应计数）。 */
export interface CacheStats {
  readonly entries: number;
  readonly bytes: number;
  readonly expiredEntries: number;
}

/** 一条条目的字节估算（UTF-8 长度；用于配额判定）。 */
export function estimateEntryBytes(entry: CacheEntry): number {
  return new TextEncoder().encode(entry.summary).length + entry.key.length;
}

/**
 * 纯策略：把一组条目收敛到配额与 TTL 以内。
 *
 * 输入输出都是普通数据，因此配额、TTL 与 **LRU 顺序**可以脱离 IndexedDB 直接断言
 * （`local-cache.test.ts`）。淘汰顺序固定为：先丢过期条目，再按 `lastUsedAtMs`
 * 由旧到新淘汰，直到总字节数不超过 `maxBytes`。
 */
export function evictToFit(input: {
  readonly entries: readonly CacheEntry[];
  readonly maxBytes: number;
  readonly ttlMs: number;
  readonly nowMs: number;
}): { readonly kept: readonly CacheEntry[]; readonly evicted: readonly CacheEntry[] } {
  const { entries, maxBytes, ttlMs, nowMs } = input;
  const kept: CacheEntry[] = [];
  const evicted: CacheEntry[] = [];

  for (const entry of entries) {
    if (nowMs - entry.updatedAtMs >= ttlMs) {
      evicted.push(entry);
    } else {
      kept.push(entry);
    }
  }

  // LRU：最近最少使用的排在最前（`lastUsedAtMs` 越小越先淘汰）。
  kept.sort((left, right) => left.lastUsedAtMs - right.lastUsedAtMs);

  let totalBytes = 0;
  for (const entry of kept) totalBytes += estimateEntryBytes(entry);

  let index = 0;
  while (totalBytes > maxBytes && index < kept.length) {
    const entry = kept[index];
    if (entry === undefined) break;
    totalBytes -= estimateEntryBytes(entry);
    evicted.push(entry);
    index += 1;
  }
  if (index > 0) kept.splice(0, index);

  return { kept, evicted };
}

/**
 * 持久层的**唯一**落盘守门：本机摘要之外的一切形状都拒绝。
 *
 * 结构类型允许更宽的对象经变量流入（多余字段不被类型检查拦下），因此守门必须在运行期
 * 拒绝 `metadata` 以及任何非摘要内容，避免一次错误的调用把远程资料写进摘要缓存。
 */
export function assertLocalSummary(content: {
  readonly summary?: string;
  readonly metadata?: unknown;
}): void {
  if (content.metadata !== undefined) {
    throw new CachePolicyError(
      "content_not_allowed",
      "持久缓存只接受本机会话摘要；来源节点资料、游标与命令状态都不得落盘。",
    );
  }
}

/** 缓存策略错误（分类明确，便于上层区分「策略拒绝」与「存储不可用」）。 */
export class CachePolicyError extends Error {
  readonly kind: "content_not_allowed" | "quota_exceeded";

  constructor(kind: CachePolicyError["kind"], message: string) {
    super(message);
    this.name = "CachePolicyError";
    this.kind = kind;
  }
}
