/**
 * R20 平台侧的可核对证据。
 *
 * 两类断言：
 *
 * 1. **纯策略**（`evictToFit` / `assertPersistableForOrigin`）：固定上限、TTL、LRU 顺序、
 *    imported 正文拒绝落盘——不依赖任何浏览器 API，直接在 Node 里判定；
 * 2. **端口行为**（`openLocalCache` + imported 内存载体）：写入、读取、LRU 刷新、清除，
 *    以及 imported 正文只进内存 Map、`clear()` 后消失。
 */

import { beforeEach, describe, expect, it } from "vitest";

import {
  CachePolicyError,
  IMPORTED_METADATA_MAX_BYTES,
  IMPORTED_METADATA_TTL_MS,
  SUMMARY_CACHE_MAX_BYTES,
  SUMMARY_CACHE_TTL_MS,
  assertPersistableForOrigin,
  estimateEntryBytes,
  evictToFit,
  type CacheEntry,
  type ImportedMetadata,
} from "./local-cache";
import { createVolatileImportedContent, openLocalCache } from "./local-cache.web";
import { installFakeIndexedDB } from "./testing/fake-indexeddb";

const NOW = 1_700_000_000_000;

/** 造一条本地摘要条目。 */
function localEntry(key: string, summary: string, lastUsedAtMs: number): CacheEntry {
  return {
    kind: "local",
    key,
    summary,
    updatedAtMs: NOW,
    lastUsedAtMs,
  };
}

const PERSISTABLE_METADATA: ImportedMetadata = {
  ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  exportId: "export-1",
  sessionId: "session-1",
  originCursor: "1-2",
  localSequence: "7",
  contentDigestSha256: "a".repeat(64),
  acks: ["1", "2"],
};

describe("R20：固定上限 / TTL / LRU 是固定常量", () => {
  it("上限与存活期是文档冻结的固定值", () => {
    expect(SUMMARY_CACHE_MAX_BYTES).toBe(8 * 1024 * 1024);
    expect(SUMMARY_CACHE_TTL_MS).toBe(30 * 24 * 60 * 60 * 1000);
    expect(IMPORTED_METADATA_TTL_MS).toBe(7 * 24 * 60 * 60 * 1000);
    expect(IMPORTED_METADATA_MAX_BYTES).toBeLessThan(SUMMARY_CACHE_MAX_BYTES);
  });

  it("超出上限时按最近最少使用淘汰到配额以内", () => {
    const entries = [
      localEntry("oldest", "x".repeat(100), NOW - 3000),
      localEntry("middle", "x".repeat(100), NOW - 2000),
      localEntry("newest", "x".repeat(100), NOW - 1000),
    ];
    // 配额只够放下两条。
    const maxBytes = estimateEntryBytes(entries[0]!) * 2;

    const { kept, evicted } = evictToFit({ entries, maxBytes, ttlMs: SUMMARY_CACHE_TTL_MS, nowMs: NOW });

    expect(evicted.map((entry) => entry.key)).toEqual(["oldest"]);
    expect(kept.map((entry) => entry.key)).toEqual(["middle", "newest"]);
    expect(kept.reduce((total, entry) => total + estimateEntryBytes(entry), 0)).toBeLessThanOrEqual(maxBytes);
  });

  it("过期条目先于 LRU 被丢弃", () => {
    const entries = [
      localEntry("expired-but-recent", "x", NOW), // 未过期
      { ...localEntry("expired", "x", NOW - 5000), updatedAtMs: NOW - SUMMARY_CACHE_TTL_MS - 1 },
    ];
    const { kept, evicted } = evictToFit({
      entries,
      maxBytes: SUMMARY_CACHE_MAX_BYTES,
      ttlMs: SUMMARY_CACHE_TTL_MS,
      nowMs: NOW,
    });
    expect(evicted.map((entry) => entry.key)).toEqual(["expired"]);
    expect(kept.map((entry) => entry.key)).toEqual(["expired-but-recent"]);
  });

  it("imported 正文在任何来源下都被拒绝进入持久层", () => {
    expect(() =>
      assertPersistableForOrigin({ kind: "imported" }, { summary: "会话正文" }),
    ).toThrow(CachePolicyError);
    // 元数据形状允许。
    expect(() =>
      assertPersistableForOrigin({ kind: "imported" }, { metadata: PERSISTABLE_METADATA }),
    ).not.toThrow();
    // 本节点自有条目不得使用 imported 元数据形状。
    expect(() =>
      assertPersistableForOrigin({ kind: "local" }, { metadata: PERSISTABLE_METADATA }),
    ).toThrow(CachePolicyError);
  });
});

describe("R20：缓存端口行为与 imported 不落盘", () => {
  beforeEach(() => {
    installFakeIndexedDB();
  });

  it("写入并可读回本地摘要；读取会刷新 LRU 时间", async () => {
    const cache = openLocalCache();
    await cache.putLocalSummary({ key: "s1", summary: "第一条", nowMs: NOW });

    const hit = await cache.get("s1", NOW + 1000);
    expect(hit?.entry.kind).toBe("local");
    expect(hit?.remainingTtlMs).toBe(SUMMARY_CACHE_TTL_MS - 1000);

    const stats = await cache.stats(NOW);
    expect(stats.localEntries).toBe(1);
    expect(stats.importedEntries).toBe(0);
  });

  it("过期条目在读取时被丢弃", async () => {
    const cache = openLocalCache();
    await cache.putLocalSummary({ key: "s1", summary: "旧", nowMs: NOW });
    expect(await cache.get("s1", NOW + SUMMARY_CACHE_TTL_MS)).toBeNull();
  });

  it("imported 只落无正文元数据，正文只进内存且清除后消失", async () => {
    const cache = openLocalCache();
    const volatile = createVolatileImportedContent();

    await cache.putImportedMetadata({ key: "remote-1", metadata: PERSISTABLE_METADATA, nowMs: NOW });
    volatile.put("remote-1", "imported 的正文只在内存");

    // 持久侧只拿到元数据。
    const hit = await cache.get("remote-1", NOW + 1000);
    expect(hit?.entry.kind).toBe("imported");
    const persisted = JSON.stringify(hit?.entry);
    expect(persisted).not.toContain("imported 的正文只在内存");
    expect(persisted).toContain(PERSISTABLE_METADATA.contentDigestSha256);

    // 内存侧持有正文，清除后即消失。
    expect(volatile.get("remote-1")).toBe("imported 的正文只在内存");
    volatile.clear();
    expect(volatile.get("remote-1")).toBeNull();
    expect(volatile.keys()).toEqual([]);
  });

  it("清除数据清空持久缓存", async () => {
    const cache = openLocalCache();
    await cache.putLocalSummary({ key: "s1", summary: "x", nowMs: NOW });
    await cache.putImportedMetadata({ key: "r1", metadata: PERSISTABLE_METADATA, nowMs: NOW });
    await cache.clear();

    const stats = await cache.stats(NOW);
    expect(stats.localEntries).toBe(0);
    expect(stats.importedEntries).toBe(0);
  });

  it("持久配额按 LRU 收敛（本节点摘要）", async () => {
    const cache = openLocalCache();
    const big = "x".repeat(SUMMARY_CACHE_MAX_BYTES / 2);
    await cache.putLocalSummary({ key: "a", summary: big, nowMs: NOW - 2000 });
    await cache.putLocalSummary({ key: "b", summary: big, nowMs: NOW - 1000 });
    await cache.putLocalSummary({ key: "c", summary: big, nowMs: NOW });

    const stats = await cache.stats(NOW);
    expect(stats.localBytes).toBeLessThanOrEqual(SUMMARY_CACHE_MAX_BYTES);
    // 最久未使用的 `a` 应已被淘汰。
    expect(await cache.get("a", NOW)).toBeNull();
  });
});
