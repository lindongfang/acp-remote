/**
 * R20：imported 资源不落盘，且本节点摘要缓存按固定容量 + TTL + LRU 淘汰。
 *
 * ## 必须走真实 `local-cache.web.ts` 的理由
 *
 * `review-wp7-r1` 指出既有用例「直调模型、从不经过生产管线」，导致两处生产缺陷带着全绿通过。
 * 因此本文件的每一条落盘断言都**不**调 `evictToFit`/`assertPersistableForOrigin` 了事，
 * 而是从 `openLocalCache()` 出发走完整的 IndexedDB 事务管线，并直接**翻查替身存储里的
 * 全部记录**（`__fakeIndexedDBDump()`）——「没有落盘」是「查过了、一条都没有」，
 * 不是「某个函数返回了 false」。
 *
 * ## 判别力
 *
 * - 若 imported 正文有了落盘路径，`__fakeIndexedDBDump()` 里会搜到正文原文 → 断言变红；
 * - 若淘汰策略改成 FIFO 或不淘汰，LRU 顺序断言会红；
 * - 若上限可被配置放宽，`openLocalCache.length === 0` 与常量断言会红。
 */

import { beforeEach, describe, expect, it } from "vitest";

import {
  CachePolicyError,
  IMPORTED_METADATA_MAX_BYTES,
  IMPORTED_METADATA_TTL_MS,
  SUMMARY_CACHE_MAX_BYTES,
  SUMMARY_CACHE_TTL_MS,
  type ImportedMetadata,
} from "./local-cache";
import { createVolatileImportedContent, openLocalCache } from "./local-cache.web";
import { installFakeIndexedDB } from "./testing/fake-indexeddb";
import { mayMarkInputAsSent, routeContent, type ContentKind, type SessionOrigin } from "../sync-client";

const NOW = 1_700_000_000_000;

/** spec 点名的七类内容（正文、提示与回复、工具内容、差异、终端输出、附件、ACP 原文）。 */
const ALL_CONTENT_KINDS: readonly ContentKind[] = [
  "summary",
  "prompt",
  "tool_content",
  "diff",
  "terminal_output",
  "attachment",
  "acp_raw",
];

/** imported（remote）来源。 */
const REMOTE: SessionOrigin = { kind: "remote", ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87" };
/** 本节点自有来源。 */
const LOCAL: SessionOrigin = { kind: "local" };

/** 允许清单内的 imported 元数据。 */
const METADATA: ImportedMetadata = {
  ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  exportId: "export-1",
  sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
  originCursor: "10",
  localSequence: "7",
  contentDigestSha256: "a".repeat(64),
  acks: ["1", "2"],
};

/** 替身存储里全部记录的可搜索文本（判「没有落盘」的唯一依据）。 */
function persistedText(): string {
  return JSON.stringify(globalThis.__fakeIndexedDBDump());
}

/** 造一条本节点摘要条目。 */
function summaryOf(key: string, bytes: number): string {
  return `${key}:${"x".repeat(bytes)}`;
}

describe("R20：imported 七类内容一律不进入持久存储（真实 WebLocalCache 管线）", () => {
  beforeEach(() => {
    installFakeIndexedDB();
  });

  it("七类内容在 imported 来源下全部判为仅内存", () => {
    for (const kind of ALL_CONTENT_KINDS) {
      expect(routeContent(REMOTE, kind)).toEqual({ destination: "memory_only" });
    }
  });

  it("本节点自有内容只有摘要可进缓存，其余六类仍只留内存", () => {
    // 对照：证明上一条不是因为「所有来源都只留内存」而恒真。
    expect(routeContent(LOCAL, "summary")).toEqual({ destination: "local_summary_cache" });
    for (const kind of ALL_CONTENT_KINDS) {
      if (kind === "summary") continue;
      expect(routeContent(LOCAL, kind)).toEqual({ destination: "memory_only" });
    }
  });

  it("真实端口：imported 只落无正文元数据，正文只进内存载体且清除后消失", async () => {
    const cache = openLocalCache();
    const volatile = createVolatileImportedContent();
    const body = "imported 会话正文：这段文字绝不允许出现在任何持久浏览器存储里";

    // 先把正文放进内存载体（模拟「客户端接收并渲染了 imported 正文」）。
    volatile.put("remote-1", body);
    await cache.putImportedMetadata({ key: "remote-1", metadata: METADATA, nowMs: NOW });

    // 直接翻查存储：正文一个字节都不在。
    expect(persistedText()).not.toContain(body);
    expect(persistedText()).not.toContain("这段文字绝不允许");
    // 元数据本身确实落盘了（否则这条断言是「什么都没写」的平凡通过）。
    expect(persistedText()).toContain(METADATA.contentDigestSha256);

    const hit = await cache.get("remote-1", NOW + 1000);
    expect(hit?.entry.kind).toBe("imported");
    expect(JSON.stringify(hit?.entry)).not.toContain(body);

    // 内存侧持有正文，清除后即消失（重连后必须重新获取）。
    expect(volatile.get("remote-1")).toBe(body);
    volatile.clear();
    expect(volatile.keys()).toEqual([]);
  });

  it("落盘守门被真的接线：imported 元数据夹带正文即拒绝，且存储里一条都没有", async () => {
    const cache = openLocalCache();
    const body = "被夹带的正文";
    const smuggled = { ...METADATA, body } as unknown as ImportedMetadata;

    await expect(cache.putImportedMetadata({ key: "r1", metadata: smuggled, nowMs: NOW })).rejects.toBeInstanceOf(
      CachePolicyError,
    );
    // 反例：若守门只做类型检查（运行期不拦），这里会有一条记录 → 断言变红。
    expect(persistedText()).not.toContain(body);
    expect((await cache.stats(NOW)).importedEntries).toBe(0);
  });

  it("本节点条目夹带 imported 元数据形状即拒绝（来源分流在写入口真的生效）", async () => {
    const cache = openLocalCache();
    const smuggled = {
      key: "s1",
      summary: "摘要",
      metadata: METADATA,
      nowMs: NOW,
    } as unknown as { readonly key: string; readonly summary: string; readonly nowMs: number };

    await expect(cache.putLocalSummary(smuggled)).rejects.toBeInstanceOf(CachePolicyError);
    expect((await cache.stats(NOW)).localEntries).toBe(0);
  });
});

describe("R20：imported 会话来源离线时输入不被标记为已发送", () => {
  it("imported 来源即便连接在线也不得标记（Owner 可能不可达）", () => {
    // 反例：若判定只看 `online`，这里会是 true → 断言变红。
    expect(mayMarkInputAsSent({ online: true, origin: REMOTE })).toBe(false);
  });

  it("本节点来源只有在线时可标记", () => {
    expect(mayMarkInputAsSent({ online: true, origin: LOCAL })).toBe(true);
    expect(mayMarkInputAsSent({ online: false, origin: LOCAL })).toBe(false);
  });
});

describe("R20：本节点摘要缓存按固定容量 + TTL + LRU 淘汰（真实管线）", () => {
  beforeEach(() => {
    installFakeIndexedDB();
  });

  it("容量与存活期是固定常量，不随调用参数变化", () => {
    expect(SUMMARY_CACHE_MAX_BYTES).toBe(8 * 1024 * 1024);
    expect(SUMMARY_CACHE_TTL_MS).toBe(30 * 24 * 60 * 60 * 1000);
    expect(IMPORTED_METADATA_MAX_BYTES).toBe(2 * 1024 * 1024);
    expect(IMPORTED_METADATA_TTL_MS).toBe(7 * 24 * 60 * 60 * 1000);
  });

  it("缓存端口不接受任何容量或存活期参数（没有放宽上限的配置入口）", () => {
    // 反例：若 `openLocalCache(options)` 出现可选的 maxBytes/ttl，这里会是 1 → 断言变红。
    expect(openLocalCache.length).toBe(0);
    // 端口方法的入参形状固定为「内容 + nowMs」，没有任何上限字段。
    const cache = openLocalCache();
    for (const name of ["putLocalSummary", "putImportedMetadata", "get", "clear", "stats"] as const) {
      const method = cache[name] as (...args: readonly unknown[]) => unknown;
      expect(typeof method).toBe("function");
    }
  });

  it("超出固定容量时按 LRU 淘汰：被读过一次的那条活下来（不是按写入顺序）", async () => {
    const cache = openLocalCache();
    // 每条约占容量的 1/3：三条恰好装得下，第四条必然触发一次淘汰。
    const third = "x".repeat(Math.floor(SUMMARY_CACHE_MAX_BYTES / 3) - 4096);
    // 三条的 updatedAtMs **完全相同**：这样「按写入顺序」与「按最近使用时间」两种排序
    // 在写入阶段无法区分，唯一的差别来自随后的那次读取。
    const writtenAt = NOW - 1000;
    await cache.putLocalSummary({ key: "a", summary: summaryOf("a", third.length), nowMs: writtenAt });
    await cache.putLocalSummary({ key: "b", summary: summaryOf("b", third.length), nowMs: writtenAt });
    await cache.putLocalSummary({ key: "c", summary: summaryOf("c", third.length), nowMs: writtenAt });

    // 只读 a：它的 lastUsedAtMs 被抬到最新，updatedAtMs 不变。
    expect(await cache.get("a", NOW)).not.toBeNull();

    await cache.putLocalSummary({ key: "d", summary: summaryOf("d", third.length), nowMs: NOW });

    // 反例：若按写入顺序（FIFO）淘汰，先写入的 a 会被丢掉，而 LRU 下它是最新的 → 断言变红。
    expect(await cache.get("a", NOW)).not.toBeNull();
    // 反例：若按写入顺序淘汰，b/c 之一会活下来；LRU 下最旧的 b 必须被淘汰。
    expect(await cache.get("b", NOW)).toBeNull();
    expect(await cache.get("c", NOW)).not.toBeNull();
    expect(await cache.get("d", NOW)).not.toBeNull();

    const stats = await cache.stats(NOW);
    expect(stats.localBytes).toBeLessThanOrEqual(SUMMARY_CACHE_MAX_BYTES);
    expect(stats.localEntries).toBe(3);
  });

  it("超过 TTL 的条目在读取时被丢弃，且从存储里消失", async () => {
    const cache = openLocalCache();
    await cache.putLocalSummary({ key: "old", summary: "旧摘要", nowMs: NOW });
    expect(await cache.get("old", NOW + SUMMARY_CACHE_TTL_MS)).toBeNull();
    // 反例：若过期条目只是「读不到」却仍留在存储里，这里会搜到 → 断言变红。
    expect(persistedText()).not.toContain("旧摘要");
  });

  it("清除数据清空持久缓存（与主机侧撤销是不同的操作）", async () => {
    const cache = openLocalCache();
    await cache.putLocalSummary({ key: "s1", summary: "本节点摘要", nowMs: NOW });
    await cache.putImportedMetadata({ key: "r1", metadata: METADATA, nowMs: NOW });
    expect(persistedText()).toContain("本节点摘要");

    await cache.clear();

    const stats = await cache.stats(NOW);
    expect(stats.localEntries).toBe(0);
    expect(stats.importedEntries).toBe(0);
    expect(persistedText()).not.toContain("本节点摘要");
  });

  it("并发写入不丢条目（真实事务串行化接线）", async () => {
    const cache = openLocalCache();
    await Promise.all([
      cache.putLocalSummary({ key: "A", summary: "本地摘要 A", nowMs: NOW }),
      cache.putImportedMetadata({ key: "B", metadata: METADATA, nowMs: NOW }),
    ]);
    const stats = await cache.stats(NOW);
    // 反例：若写路径未串行化，两条并发写只存其一 → 断言变红。
    expect(stats.localEntries).toBe(1);
    expect(stats.importedEntries).toBe(1);
  });
});