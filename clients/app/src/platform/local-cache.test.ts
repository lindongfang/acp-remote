/**
 * R20 平台侧的可核对证据（只保留本机摘要缓存）。
 *
 * 两类断言：
 *
 * 1. **纯策略**（`evictToFit` / `assertLocalSummary`）：固定上限、TTL、LRU 顺序，以及
 *    任何夹带来源元数据/正文的写入在运行期被拒绝；不依赖浏览器 API；
 * 2. **端口行为**（`openLocalCache`）：写入、读取、LRU 刷新、配额收敛、清除，以及并发写
 *    不丢条目的串行化。
 */

import { beforeEach, describe, expect, it } from "vitest";

import {
  CachePolicyError,
  SUMMARY_CACHE_MAX_BYTES,
  SUMMARY_CACHE_TTL_MS,
  assertLocalSummary,
  estimateEntryBytes,
  evictToFit,
  type CacheEntry,
} from "./local-cache";
import { openLocalCache } from "./local-cache.web";
import { installFakeIndexedDB } from "./testing/fake-indexeddb";

const NOW = 1_700_000_000_000;

/** 造一条本机摘要条目。 */
function localEntry(key: string, summary: string, lastUsedAtMs: number): CacheEntry {
  return { key, summary, updatedAtMs: NOW, lastUsedAtMs };
}

describe("R20：固定上限 / TTL / LRU 是固定常量", () => {
  it("上限与存活期是文档冻结的固定值", () => {
    expect(SUMMARY_CACHE_MAX_BYTES).toBe(8 * 1024 * 1024);
    expect(SUMMARY_CACHE_TTL_MS).toBe(30 * 24 * 60 * 60 * 1000);
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

  it("落盘守门只在运行期接受本机摘要：夹带来源元数据即拒绝", () => {
    expect(() => assertLocalSummary({ summary: "一段会话摘要" })).not.toThrow();
    // 反例：把远程元数据/游标/ACK 挂在入参上 → 这里必须抛策略错误。
    expect(() =>
      assertLocalSummary({
        summary: "摘要",
        metadata: { ownerNodeId: "node-1", originCursor: "1-2", acks: ["1"] },
      }),
    ).toThrow(CachePolicyError);
  });
});

describe("R20：缓存端口只保留本机摘要", () => {
  beforeEach(() => {
    installFakeIndexedDB();
  });

  it("写入并可读回本地摘要；读取会刷新 LRU 时间", async () => {
    const cache = openLocalCache();
    await cache.putLocalSummary({ key: "s1", summary: "第一条", nowMs: NOW });

    const hit = await cache.get("s1", NOW + 1000);
    expect(hit?.entry.summary).toBe("第一条");
    expect(hit?.remainingTtlMs).toBe(SUMMARY_CACHE_TTL_MS - 1000);

    const stats = await cache.stats(NOW);
    expect(stats.entries).toBe(1);
    expect(stats).not.toHaveProperty("importedEntries");
  });

  it("过期条目在读取时被丢弃", async () => {
    const cache = openLocalCache();
    await cache.putLocalSummary({ key: "s1", summary: "旧", nowMs: NOW });
    expect(await cache.get("s1", NOW + SUMMARY_CACHE_TTL_MS)).toBeNull();
  });

  it("清除数据清空持久缓存", async () => {
    const cache = openLocalCache();
    await cache.putLocalSummary({ key: "s1", summary: "x", nowMs: NOW });
    await cache.clear();

    const stats = await cache.stats(NOW);
    expect(stats.entries).toBe(0);
  });

  it("持久配额按 LRU 收敛（本机摘要）", async () => {
    const cache = openLocalCache();
    const big = "x".repeat(SUMMARY_CACHE_MAX_BYTES / 2);
    await cache.putLocalSummary({ key: "a", summary: big, nowMs: NOW - 2000 });
    await cache.putLocalSummary({ key: "b", summary: big, nowMs: NOW - 1000 });
    await cache.putLocalSummary({ key: "c", summary: big, nowMs: NOW });

    const stats = await cache.stats(NOW);
    expect(stats.bytes).toBeLessThanOrEqual(SUMMARY_CACHE_MAX_BYTES);
    // 最久未使用的 `a` 应已被淘汰。
    expect(await cache.get("a", NOW)).toBeNull();
  });

  it("并发写入不丢条目：两条并发 put 都落盘（F2）", async () => {
    const cache = openLocalCache();
    await Promise.all([
      cache.putLocalSummary({ key: "A", summary: "本机摘要 A", nowMs: NOW }),
      cache.putLocalSummary({ key: "B", summary: "本机摘要 B", nowMs: NOW }),
    ]);

    const stats = await cache.stats(NOW);
    expect(stats.entries).toBe(2);
    expect((await cache.get("A", NOW))?.entry.summary).toBe("本机摘要 A");
    expect((await cache.get("B", NOW))?.entry.summary).toBe("本机摘要 B");
  });

  it("clear 与并发 put 竞争时 clear 胜出，存储为空（F2）", async () => {
    const cache = openLocalCache();
    await cache.putLocalSummary({ key: "A", summary: "旧摘要", nowMs: NOW });

    await Promise.all([
      cache.putLocalSummary({ key: "A", summary: "新摘要", nowMs: NOW }),
      cache.clear(),
    ]);

    const stats = await cache.stats(NOW);
    expect(stats.entries).toBe(0);
  });

  it("落盘守门被真的接线：本机摘要夹带来源元数据即拒绝且未落盘（F4）", async () => {
    const cache = openLocalCache();
    const smuggled = {
      key: "s1",
      summary: "摘要",
      metadata: { ownerNodeId: "node-1", originCursor: "1-2", acks: ["1"] },
      nowMs: NOW,
    } as unknown as { readonly key: string; readonly summary: string; readonly nowMs: number };

    await expect(cache.putLocalSummary(smuggled)).rejects.toBeInstanceOf(CachePolicyError);
    expect((await cache.stats(NOW)).entries).toBe(0);
  });

  it("端口没有远程元数据写入口（来源资料不落盘）", () => {
    const cache = openLocalCache() as unknown as Record<string, unknown>;
    expect(cache["putImportedMetadata"]).toBeUndefined();
    expect("importedContent" in cache).toBe(false);
  });
});
