/**
 * 本地缓存与生命周期端口的**声明**（R20 平台侧）。
 *
 * 归属 WP5b。本模块只有接口、固定常量与**纯策略**（LRU/TTL/配额），不触碰任何平台 API——
 * IndexedDB 只在 `local-cache.web.ts`，`document`/`window` 只在 `lifecycle.web.ts`。
 *
 * ## R20 的核心：imported 资源正文不落盘
 *
 * 缓存端口按**来源**分流，而不是靠调用方自觉：
 *
 * - `local` —— 本节点自有资源。允许缓存**精简摘要**，受固定容量与 TTL 约束；
 * - `remote`（imported）—— 只允许**无正文元数据**，正文一律拒绝进入持久存储。
 *
 * 因此导入侧没有「写正文」的入口：`LocalCachePort` 只暴露无正文的元数据读写，
 * 正文只能进 `VolatileImportedContent`——它**只能**由 `local-cache.web.ts` 用内存 Map 实现，
 * 页面与 feature 拿不到任何「持久化 imported 正文」的路径。
 */

/** 节点自有会话摘要缓存的固定上限（`docs/FRONTEND_DESIGN.md` §7）。 */
export const SUMMARY_CACHE_MAX_BYTES = 8 * 1024 * 1024;

/** 节点自有会话摘要缓存的固定存活期：30 天（`docs/FRONTEND_DESIGN.md` §7）。 */
export const SUMMARY_CACHE_TTL_MS = 30 * 24 * 60 * 60 * 1000;

/**
 * imported 元数据的固定上限。
 *
 * `docs/FRONTEND_DESIGN.md` §7 要求 imported 元数据「也必须有容量与清理边界，
 * 不得以『不占正文配额』为由无限增长」。这里取 2 MiB：远小于正文配额，
 * 因为元数据条目只含标识、cursor、digest 与序号映射。
 */
export const IMPORTED_METADATA_MAX_BYTES = 2 * 1024 * 1024;

/** imported 元数据的存活期：7 天。 */
export const IMPORTED_METADATA_TTL_MS = 7 * 24 * 60 * 60 * 1000;

/**
 * 一条待缓存条目的来源。
 *
 * `imported` 是**判别式**而不是布尔：调用方无法用 `isLocal: true` 这类可同时成立的组合
 * 把 imported 正文混进本节点配额（`AGENTS.md` §5 的「不使用可能产生非法组合的一组布尔值」）。
 */
export type CacheOrigin = { readonly kind: "local" } | { readonly kind: "imported" };

/**
 * 可持久化的缓存条目。
 *
 * **正文绝不在本类型上**：`summary` 是精简摘要文本（节点自有资源专用），
 * imported 条目只能是 `metadata`。这个形状本身就把「imported 正文落盘」变成不可能构造的值。
 *
 * `kind` 放在**顶层**而不是嵌进 `origin`：判别式联合要求判别字段位于被判别层，
 * 嵌套一层会让窄化失效（`AGENTS.md` §7 的「优先使用具体类型」同理）。
 */
export type CacheEntry =
  | {
      readonly kind: "local";
      readonly key: string;
      readonly summary: string;
      readonly updatedAtMs: number;
      readonly lastUsedAtMs: number;
    }
  | {
      readonly kind: "imported";
      readonly key: string;
      /** 无正文元数据：owner/export/session 标识、cursor、digest、序号映射。 */
      readonly metadata: ImportedMetadata;
      readonly updatedAtMs: number;
      readonly lastUsedAtMs: number;
    };

/**
 * imported 资源的**无正文**元数据（`docs/FRONTEND_DESIGN.md` §7 的允许清单）。
 *
 * 这里的 `contentDigestSha256` 是**密码学摘要**，不是会话摘要/标题/消息预览/内容摘录——
 * 后三者只能留在内存（{@link VolatileImportedContent}）。
 */
export interface ImportedMetadata {
  readonly ownerNodeId: string;
  readonly exportId: string;
  readonly sessionId: string;
  readonly originCursor: string;
  readonly localSequence: string;
  /** 事件的 SHA-256 digest（hex），用于完整性核对。 */
  readonly contentDigestSha256: string;
  readonly acks: readonly string[];
}

/** 缓存读命中的条目。 */
export interface CacheHit {
  readonly entry: CacheEntry;
  /** 距离 TTL 过期的剩余毫秒；已过期时为 0。 */
  readonly remainingTtlMs: number;
}

/**
 * imported 正文的**内存**载体。
 *
 * 它没有持久化实现：值只在内存 Map 里，页面刷新即消失，重连后必须重新获取
 * （R20 场景 1「该正文不进入任何持久浏览器存储，重连后必须重新获取」）。
 */
export interface VolatileImportedContent {
  /** 写入一条 imported 正文（仅内存）。 */
  put(key: string, content: string): void;
  /** 读取；不存在时 `null`。 */
  get(key: string): string | null;
  /** 丢弃（会话切换、dispose 时调用）。 */
  drop(key: string): void;
  /** 丢弃全部。 */
  clear(): void;
  /** 当前持有的键，供诊断。 */
  keys(): readonly string[];
}

/** 持久缓存端口。 */
export interface LocalCachePort {
  /** 写入一条本地摘要条目；超容量时按 LRU 淘汰到配额以内。 */
  putLocalSummary(input: {
    readonly key: string;
    readonly summary: string;
    readonly nowMs: number;
  }): Promise<void>;

  /** 写入一条 imported 无正文元数据。 */
  putImportedMetadata(input: {
    readonly key: string;
    readonly metadata: ImportedMetadata;
    readonly nowMs: number;
  }): Promise<void>;

  /** 读取一条未过期条目（命中时刷新 `lastUsedAtMs`）。 */
  get(key: string, nowMs: number): Promise<CacheHit | null>;

  /** 清除全部持久缓存（R20 场景 4「清除数据」）。 */
  clear(): Promise<void>;

  /** 提示诊断信息：条目数、字节数、过期数。 */
  stats(nowMs: number): Promise<CacheStats>;
}

/** 缓存诊断。 */
export interface CacheStats {
  readonly localEntries: number;
  readonly localBytes: number;
  readonly importedEntries: number;
  readonly importedBytes: number;
  readonly expiredEntries: number;
}

/** 一条条目的字节估算（UTF-8 长度；用于配额判定）。 */
export function estimateEntryBytes(entry: CacheEntry): number {
  const text = entry.kind === "local" ? entry.summary : JSON.stringify(entry.metadata);
  return new TextEncoder().encode(text).length + entry.key.length;
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
 * R20 的**唯一**落盘守门函数：给定来源，返回该来源允许进入持久存储的内容形状。
 *
 * 这是一次显式的判别，而不是注释里的约定：任何想把 imported 正文写进持久层的调用路径
 * 都必须先过这里，而 imported 分支在类型上就拿不到 `summary`。
 */
export function assertPersistableForOrigin(
  origin: CacheOrigin,
  content: { readonly summary?: string; readonly metadata?: ImportedMetadata },
): void {
  if (origin.kind === "imported") {
    if (content.summary !== undefined) {
      throw new CachePolicyError(
        "content_not_allowed",
        "imported 资源正文 MUST NOT 写入任何持久浏览器存储（R20）",
      );
    }
    if (content.metadata === undefined) {
      throw new CachePolicyError("content_not_allowed", "imported 持久条目必须携带无正文元数据");
    }
    return;
  }
  if (content.metadata !== undefined) {
    throw new CachePolicyError("content_not_allowed", "本节点自有条目不得使用 imported 元数据形状");
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
