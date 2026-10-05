/**
 * R14/D1：快照暂存与原子替换。
 *
 * 每个用例都有对应的「反例会红」实现：
 * - 把暂存内容直接写进已完成状态 → 「验证失败保留旧状态」用例变红；
 * - 只按 chunk 数、不校验 digest → digest 篡改用例变红；
 * - 用重新序列化的 JSON 算 digest → 「digest 按原始字节」用例变红；
 * - 允许明细资源进快照 → 「明细资源被拒绝」用例变红。
 */

import { describe, expect, it } from "vitest";

import { SnapshotStaging, SnapshotValidationError } from "./snapshot";
import type { SnapshotDiscardReason } from "./snapshot";
import { decodeWireMessage } from "./wire";
import { encodeBase64Url } from "./base64url";
import { SnapshotRepository } from "./client";
import {
  expectedSnapshotDigest,
  fakeDigest,
  makeSnapshotBegin,
  makeSnapshotChunk,
  makeSnapshotEnd,
} from "./testing/harness";

const SNAPSHOT_ID = "8194de43-e213-423d-acf4-2e3549304566";

const SESSIONS_ITEM = [
  {
    sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
    title: "修复测试",
    agent: { agentId: "claude", name: "Claude" },
    state: "idle",
    origin: { kind: "local" },
    currentMode: null,
    version: "1",
    createdAt: "2026-10-01T00:00:00.000Z",
    updatedAt: "2026-10-01T00:00:00.000Z",
  },
];

describe("R14：快照验证通过后才产出可提交结果", () => {
  it("三 chunk 齐全时验证通过并带全部清单资源", async () => {
    const digest = fakeDigest();
    const staging = new SnapshotStaging(digest);
    staging.begin(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 3 }));

    const chunks = [
      makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM }),
      makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "1", resource: "workspaces", items: [{ alias: "api", displayName: "API" }] }),
      makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "2", resource: "agents", items: [{ agentId: "claude", displayName: "Claude", default: true }] }),
    ];
    for (const chunk of chunks) {
      await staging.acceptChunk(chunk.rawText, chunk.message);
    }

    const verified = await staging.complete(
      makeSnapshotEnd({
        snapshotId: SNAPSHOT_ID,
        chunkCount: 3,
        digest: expectedSnapshotDigest(chunks.map((chunk) => chunk.rawText)),
      }),
    );

    expect(verified.resources.sessions).toHaveLength(1);
    expect(verified.resources.workspaces).toEqual([{ alias: "api", displayName: "API" }]);
    expect(verified.resources.agents).toEqual([{ agentId: "claude", displayName: "Claude", default: true }]);
    expect(staging.hasPending).toBe(false);
  });

  it("同一语义的紧凑文本与缩进文本 digest 不同（digest 覆盖字节而非语义）", async () => {
    const digest = fakeDigest();
    const compact = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM });
    const pretty = JSON.stringify(JSON.parse(compact.rawText), null, 2);
    expect(expectedSnapshotDigest([compact.rawText])).not.toBe(expectedSnapshotDigest([pretty]));
    void digest;
  });

  it("digest 不匹配时抛错并丢弃暂存区", async () => {
    const digest = fakeDigest();
    const staging = new SnapshotStaging(digest);
    staging.begin(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 1 }));
    const chunk = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM });
    await staging.acceptChunk(chunk.rawText, chunk.message);

    await expect(
      staging.complete(
        makeSnapshotEnd({ snapshotId: SNAPSHOT_ID, chunkCount: 1, digest: encodeBase64Url(new Uint8Array(32).fill(9)) }),
      ),
    ).rejects.toMatchObject({ name: "SnapshotValidationError", reason: "digest_mismatch" });

    // 反例：若 digest 校验缺失，这里不会抛错 → 断言变红。
    expect(staging.hasPending).toBe(false);
  });

  it("digest 按原始字节计算：带缩进的同一语义文本仍能验证通过", async () => {
    const digest = fakeDigest();
    const staging = new SnapshotStaging(digest);
    staging.begin(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 1 }));
    const chunk = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM });
    // 送入带缩进的原始文本（语义与紧凑形式相同、字节不同），期望 digest 按**这串字节**复算。
    const pretty = JSON.stringify(JSON.parse(chunk.rawText), null, 2);
    await staging.acceptChunk(pretty, chunk.message);

    const verified = await staging.complete(
      makeSnapshotEnd({ snapshotId: SNAPSHOT_ID, chunkCount: 1, digest: expectedSnapshotDigest([pretty]) }),
    );
    expect(verified.resources.sessions).toHaveLength(1);
    // 两级哈希各一次：每 chunk 一次，再对连接结果一次。
    expect(digest.calls).toHaveLength(2);
  });

  it("chunk 数不足时抛 chunk_count_mismatch", async () => {
    const digest = fakeDigest();
    const staging = new SnapshotStaging(digest);
    staging.begin(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 3 }));
    const chunk = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM });
    await staging.acceptChunk(chunk.rawText, chunk.message);

    await expect(
      staging.complete(makeSnapshotEnd({ snapshotId: SNAPSHOT_ID, chunkCount: 3, digest: encodeBase64Url(new Uint8Array(32)) })),
    ).rejects.toMatchObject({ reason: "chunk_count_mismatch" });
  });

  it("chunk 序号不连续时立即丢弃暂存区", async () => {
    const digest = fakeDigest();
    const staging = new SnapshotStaging(digest);
    staging.begin(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 3 }));
    const second = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "1", resource: "workspaces", items: [] });

    await expect(staging.acceptChunk(second.rawText, second.message)).rejects.toMatchObject({
      reason: "chunk_out_of_order",
    });
    expect(staging.hasPending).toBe(false);
  });

  it("begin 与 end 的 cursor 不一致时判 cursor_mismatch", async () => {
    const digest = fakeDigest();
    const staging = new SnapshotStaging(digest);
    staging.begin(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 1 }));
    const chunk = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM });
    await staging.acceptChunk(chunk.rawText, chunk.message);

    await expect(
      staging.complete(
        makeSnapshotEnd({
          snapshotId: SNAPSHOT_ID,
          chunkCount: 1,
          digest: expectedSnapshotDigest([chunk.rawText]),
          cursor: { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "11" },
        }),
      ),
    ).rejects.toMatchObject({ reason: "cursor_mismatch" });
  });
});

describe("R14：reset 与新快照丢弃未完成暂存", () => {
  it("收到新的 snapshot_begin 时旧暂存区被丢弃", async () => {
    const digest = fakeDigest();
    const staging = new SnapshotStaging(digest);
    staging.begin(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 3 }));
    const chunk = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM });
    await staging.acceptChunk(chunk.rawText, chunk.message);
    expect(staging.hasPending).toBe(true);

    staging.begin(makeSnapshotBegin({ snapshotId: "00000000-1111-4222-8333-444444444444", chunkCount: 1 }));

    // 反例：若 begin 不丢弃旧暂存，`snapshotId` 会仍是上一个 → 断言变红。
    expect(staging.staged?.snapshotId).toBe("00000000-1111-4222-8333-444444444444");
    expect(staging.staged?.receivedChunks).toBe(0);
  });

  it("discard 之后 complete 抛错（不得用未完成内容完成替换）", async () => {
    const digest = fakeDigest();
    const staging = new SnapshotStaging(digest);
    staging.begin(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 1 }));
    staging.discard("reset_required");

    await expect(
      staging.complete(makeSnapshotEnd({ snapshotId: SNAPSHOT_ID, chunkCount: 1, digest: encodeBase64Url(new Uint8Array(32)) })),
    ).rejects.toBeInstanceOf(SnapshotValidationError);
  });
});

describe("R14：验证失败不破坏已完成状态", () => {
  it("第二次快照验证失败时仓库仍保留第一次的已完成状态", async () => {
    const digest = fakeDigest();
    const staging = new SnapshotStaging(digest);
    const repository = new SnapshotRepository();

    // 第一次：验证通过并替换。
    staging.begin(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 1 }));
    const first = makeSnapshotChunk({ snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource: "sessions", items: SESSIONS_ITEM });
    await staging.acceptChunk(first.rawText, first.message);
    const verified = await staging.complete(
      makeSnapshotEnd({ snapshotId: SNAPSHOT_ID, chunkCount: 1, digest: expectedSnapshotDigest([first.rawText]) }),
    );
    repository.replace(verified, 1_700_000_000_000);
    expect(repository.current?.resources.sessions).toHaveLength(1);

    // 第二次：digest 被篡改 → 验证失败，仓库**不得**被覆盖。
    const otherId = "00000000-1111-4222-8333-444444444444";
    staging.begin(makeSnapshotBegin({ snapshotId: otherId, chunkCount: 1 }));
    const second = makeSnapshotChunk({ snapshotId: otherId, chunkIndex: "0", resource: "sessions", items: [] });
    await staging.acceptChunk(second.rawText, second.message);
    await expect(
      staging.complete(
        makeSnapshotEnd({ snapshotId: otherId, chunkCount: 1, digest: encodeBase64Url(new Uint8Array(32).fill(7)) }),
      ),
    ).rejects.toMatchObject({ reason: "digest_mismatch" });

    // 反例：若实现把未完成内容写进仓库，这里会变成 0 条或 snapshotId 变化 → 断言变红。
    expect(repository.current?.snapshotId).toBe(SNAPSHOT_ID);
    expect(repository.current?.resources.sessions).toHaveLength(1);
  });
});

describe("D1：会话明细资源不得进入快照", () => {
  it("五类明细资源的 chunk 在解码处被拒绝", () => {
    const forbidden = ["messages", "turns", "pending_interactions", "config_options", "capabilities"];
    for (const resource of forbidden) {
      const decoded = decodeWireMessage(
        JSON.stringify({
          protocolVersion: 1,
          type: "sync.snapshot_chunk",
          messageId: "aaaaaaaa-1111-4111-8111-000000000002",
          connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
          connectionSequence: "1",
          body: { snapshotId: SNAPSHOT_ID, chunkIndex: "0", resource, items: [] },
        }),
      );
      expect(decoded.ok, `未拦截：${resource}`).toBe(false);
      if (!decoded.ok) expect(decoded.reason).toBe("field_type");
    }
  });

  it("chunkCount 为 0 或 4 时被拒绝（只能是 1..3）", () => {
    for (const chunkCount of [0, 4]) {
      const decoded = decodeWireMessage(
        JSON.stringify({
          protocolVersion: 1,
          type: "sync.snapshot_begin",
          messageId: "aaaaaaaa-1111-4111-8111-000000000001",
          connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
          connectionSequence: "1",
          body: {
            snapshotId: SNAPSHOT_ID,
            cursor: { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" },
            schemaVersion: 1,
            chunkCount,
          },
        }),
      );
      expect(decoded.ok, `未拦截 chunkCount=${chunkCount}`).toBe(false);
    }
  });
});

describe("暂存丢弃原因", () => {
  it("discard 接受全部六个结构化原因且不抛错", () => {
    const staging = new SnapshotStaging(fakeDigest());
    const reasons: readonly SnapshotDiscardReason[] = [
      "reset_required",
      "superseded_by_new_snapshot",
      "chunk_out_of_order",
      "chunk_count_mismatch",
      "cursor_mismatch",
      "digest_mismatch",
      "snapshot_id_mismatch",
    ];
    for (const reason of reasons) {
      staging.begin(makeSnapshotBegin({ snapshotId: SNAPSHOT_ID, chunkCount: 1 }));
      staging.discard(reason);
      expect(staging.hasPending).toBe(false);
    }
  });
});