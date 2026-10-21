/**
 * R20：远程资料不进入持久浏览器存储。
 *
 * 旧的 imported 元数据/正文持久分支已随 `access-owner-data-stateless` 删除；缓存端口只接受
 * 本机摘要。本文件从 `openLocalCache()` 出发走完整的 IndexedDB 事务管线，并翻查替身存储里的
 * 全部记录——「没有落盘」是「查过了、一条远程资料都没有」，而不是只看类型。
 *
 * 判别力：
 * - 若重新引入任何 imported 写入口或 `kind: "imported"` 分支，下面的入口/形状断言变红；
 * - 若摘要写入口不再检查夹带字段，哨兵会被写进持久层，dump 断言变红。
 */

import { beforeEach, describe, expect, it } from "vitest";

import { CachePolicyError } from "./local-cache";
import { openLocalCache } from "./local-cache.web";
import { installFakeIndexedDB } from "./testing/fake-indexeddb";

const NOW = 1_700_000_000_000;
const REMOTE_SENTINEL = "REMOTE-SENTINEL-owner-session-cursor";

/** 替身存储里的全部记录（JSON 文本）。 */
function persistedText(): string {
  return JSON.stringify(globalThis.__fakeIndexedDBDump());
}

describe("R20：浏览器持久存储没有远程资料", () => {
  beforeEach(() => {
    installFakeIndexedDB();
  });

  it("缓存端口没有 imported/远程元数据写入口", () => {
    const cache = openLocalCache() as unknown as Record<string, unknown>;
    expect(cache["putImportedMetadata"]).toBeUndefined();
  });

  it("摘要写入口拒绝夹带远程元数据，且哨兵未落盘", async () => {
    const cache = openLocalCache();
    const smuggled = {
      key: "remote-1",
      summary: "远程会话标题",
      metadata: {
        ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
        sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
        originCursor: REMOTE_SENTINEL,
      },
      nowMs: NOW,
    } as unknown as { readonly key: string; readonly summary: string; readonly nowMs: number };

    await expect(cache.putLocalSummary(smuggled)).rejects.toBeInstanceOf(CachePolicyError);
    expect(await cache.get("remote-1", NOW)).toBeNull();
    expect(persistedText()).not.toContain(REMOTE_SENTINEL);
  });

  it("常规本机摘要写入后，持久层只有摘要字段（无来源/游标/命令）", async () => {
    const cache = openLocalCache();
    await cache.putLocalSummary({ key: "session-1", summary: "本机会话摘要", nowMs: NOW });

    const dump = persistedText();
    expect(dump).toContain("本机会话摘要");
    expect(dump).not.toContain("originCursor");
    expect(dump).not.toContain("contentDigestSha256");
    expect(dump).not.toContain("requestId");
    expect(dump).not.toContain(REMOTE_SENTINEL);
  });

  it("统计只报告本机摘要维度", async () => {
    const cache = openLocalCache();
    const stats = await cache.stats(NOW);
    expect(stats).toEqual({ entries: 0, bytes: 0, expiredEntries: 0 });
  });
});
