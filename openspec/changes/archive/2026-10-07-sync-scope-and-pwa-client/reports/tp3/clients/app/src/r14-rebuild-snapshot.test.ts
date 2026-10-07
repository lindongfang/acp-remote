// 【交接副本】来源 worktree .worktrees/tp3 的 clients/app/src/r14-rebuild-snapshot.test.ts
// 交付提交 6e8307096629dcfd3fa63ac20559e25b0a3e7a7a（分支 feat/tp3），基线 f6ea252。
// 用途：premerge 门禁按「相对变更目录」解析 test_delivery.artifacts 的可读性；本副本在权威交付物之外。
// 除顶部本 4 行说明外与来源逐字节相同（权威可运行副本在 worktree 内，由 PV3 执行）。
/**
 * R14：服务端要求重建快照时，**未完成的暂存快照被丢弃且不覆盖已完成状态**。
 *
 * ## 为什么既有测试不够
 *
 * `snapshot-barrier.test.ts`（WP7）覆盖的是「快照的**游标屏障**被账本拒绝时不污染恢复点」，
 * `snapshot.test.ts` 覆盖的是 `SnapshotStaging` 的纯校验分支。两者都不覆盖 spec 的那条
 * 场景原话：「客户端在新的快照验证通过之前不覆盖该状态，**验证失败时仍保留上一次
 * 已完成的状态**」。
 *
 * 本文件把这条场景放在**完整组合根**上验证：真实 `SyncClient` + 真实 `SnapshotStaging`
 * + 真实 `ClientStore`，服务端依次给出
 * 「一份验证通过 → 一份**未完成**（只到 chunk 1/3）→ `sync.reset_required`」。
 * 断言仓库里始终是第一次那份已完成的快照。
 *
 * ## 判别力
 *
 * 若暂存区在 `reset_required` 时不被丢弃，后续那份残缺快照的 chunk 会与新快照的
 * chunkIndex 混在一起；更关键的是——断言的是**仓库内容**，若实现用未完成内容覆盖了
 * 已完成状态，本文件的 `titles` 断言会红。
 */

import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";

import { createComposition } from "./composition";
import type { Composition, FetchLike } from "./composition";
import { decodeBase64Url, encodeBase64Url, sortedFeatures } from "./sync-client";
import { encodeNulJoinedUtf8, encodeTranscript, encodeU16be, encodeUtf8, encodeUuid16 } from "./platform/transcript";

/** `host-challenge` 的 domain（§6.3）。 */
const HOST_CHALLENGE_DOMAIN = "acp-remote/host-challenge/v1";
import { createVolatileImportedContent, openLocalCache } from "./platform/local-cache.web";
import { openLifecycle } from "./platform/lifecycle.web";
import type { PlatformPorts } from "./platform";
import { installFakeIndexedDB } from "./platform/testing/fake-indexeddb";
import { FakeClock, FakeSocketFactory } from "./sync-client/testing/harness";

/**
 * 等待一个可观察条件成立（**让出到宏任务**）。
 *
 * 不能用 harness 的 `waitUntil`：它只轮转微任务，而真实 `crypto.subtle.verify`
 * 的 promise 在 Node 里要等到底层 libuv 的完成回调（一次宏任务）才 resolve。
 * 等的是**条件**而非时长——`setTimeout(…, 0)` 的 0ms 只表示排到下一轮事件循环。
 */
async function waitFor(predicate: () => boolean, label: string): Promise<void> {
  for (let round = 0; round < 4000; round += 1) {
    if (predicate()) return;
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
  }
  throw new Error(`等待条件未成立：${label}`);
}

const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const DEVICE_ID = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
const ORIGIN = "https://work-pc.example.ts.net";
const SERVER_EPOCH = "00384a03-bc90-4095-b65d-82fb8cc47e13";
const NOW_MS = 1_700_000_000_000;
const FEATURES = ["acp.raw-payload.v1", "core.command-status.v1", "core.event-ack.v1", "core.snapshot.v1"];

/** 已完成快照的会话标题：整条用例都靠它证明「仓库没被未完成内容覆盖」。 */
const COMPLETED_TITLE = "已完成的那次同步";
/** 未完成快照的会话标题：若它出现在仓库里，说明未完成内容覆盖了已完成状态。 */
const INCOMPLETE_TITLE = "未完成的暂存快照";

/** 确定性随机源：nonce 按计数器给，UUID 形状合法。 */
class DeterministicRandom {
  #counter = 0;
  bytes(length: number): Uint8Array<ArrayBuffer> {
    this.#counter += 1;
    const out = new Uint8Array(new ArrayBuffer(length));
    for (let index = 0; index < length; index += 1) out[index] = (this.#counter + index) & 0xff;
    return out;
  }
  uuid(): string {
    this.#counter += 1;
    return `00000000-0000-4000-8000-${this.#counter.toString(16).padStart(12, "0")}`;
  }
}

/** 真实平台端口 + 真实组合根（只替换时钟、随机源与传输）。 */
interface Harness {
  readonly composition: Composition;
  readonly sockets: FakeSocketFactory;
}

/** 真实的 Host 密钥对：本文件不注入 host 端口替身，握手走真实验签路径。 */
interface HostKey {
  readonly privateKey: CryptoKey;
  readonly publicKeyBase64Url: string;
}

/** 生成一对真实 P-256 Host 密钥（真 `crypto.subtle.generateKey`）。 */
async function generateHostKey(): Promise<HostKey> {
  const keyPair = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, ["sign", "verify"]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", keyPair.publicKey));
  return { privateKey: keyPair.privateKey, publicKeyBase64Url: encodeBase64Url(raw) };
}

/** 已配对主机 + 已持有设备身份的组合根。 */
async function setupPaired(hostKey: HostKey): Promise<Harness> {
  const material = {
    deviceId: DEVICE_ID,
    canonicalOrigin: ORIGIN,
    publicKeyBase64Url: "AQID",
    privateKey: {} as CryptoKey,
  };
  const platform: PlatformPorts = {
    // 设备身份用内存实现：本组用例的被测对象是快照替换与 store，密钥生命周期由 R11 组覆盖。
    deviceIdentity: {
      async loadIdentity() {
        return material;
      },
      async ensureIdentity() {
        return material;
      },
      async signDeviceProof() {
        return new Uint8Array(64);
      },
      async clearIdentity() {
        return undefined;
      },
    },
    localCache: openLocalCache(),
    importedContent: createVolatileImportedContent(),
    lifecycle: openLifecycle(),
  };

  const sockets = new FakeSocketFactory();
  const composition = createComposition({
    platform,
    openSocket: sockets.open,
    ports: { clock: new FakeClock(NOW_MS), random: new DeterministicRandom() },
    fetch: (async (): Promise<never> => {
      throw new Error("本组用例不触达配对 HTTPS 面");
    }) as unknown as FetchLike,
  });

  await composition.rememberPairedHost({
    hostId: HOST_ID,
    publicKeyBase64Url: hostKey.publicKeyBase64Url,
    canonicalOrigin: ORIGIN,
  });
  await composition.start();
  return { composition, sockets };
}

/** `host-challenge` transcript：与生产实现同一套字段与顺序（§6.3）。 */
function hostChallengeTranscript(input: {
  readonly clientNonce: Uint8Array<ArrayBuffer>;
  readonly serverNonce: Uint8Array<ArrayBuffer>;
  readonly connectionId: string;
  readonly features: readonly string[];
}): Uint8Array<ArrayBuffer> {
  return encodeTranscript(HOST_CHALLENGE_DOMAIN, [
    { tag: 1, bytes: encodeU16be(1) },
    { tag: 2, bytes: encodeUuid16(HOST_ID) },
    { tag: 3, bytes: encodeUuid16(DEVICE_ID) },
    { tag: 6, bytes: encodeUtf8(ORIGIN) },
    { tag: 9, bytes: input.clientNonce },
    { tag: 10, bytes: input.serverNonce },
    { tag: 11, bytes: encodeUuid16(input.connectionId) },
    { tag: 12, bytes: encodeNulJoinedUtf8(sortedFeatures(input.features)) },
  ]);
}

/** 跑完握手与认证（真实验签：host proof 由本测试持有的真实私钥签出）。 */
async function authenticate(harness: Harness, hostKey: HostKey): Promise<void> {
  const socket = harness.sockets.latest;
  socket.open();
  await waitFor(() => socket.types().includes("auth.client_hello"), "发出 clientHello");

  const serverNonce = new Uint8Array(new ArrayBuffer(32)).fill(7);
  const transcript = hostChallengeTranscript({
    clientNonce: await clientNonceOf(harness),
    serverNonce,
    connectionId: CONNECTION_ID,
    features: FEATURES,
  });
  const proof = await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, hostKey.privateKey, transcript);

  socket.deliver({
    protocolVersion: 1,
    type: "auth.server_challenge",
    messageId: "cbb91891-8f84-4b47-bdf3-2c902c338367",
    body: {
      selectedProtocolVersion: 1,
      hostId: HOST_ID,
      connectionId: CONNECTION_ID,
      serverNonce: encodeBase64Url(serverNonce),
      selectedFeatures: [...sortedFeatures(FEATURES)],
      hostProof: encodeBase64Url(new Uint8Array(proof)),
    },
  });
  await waitFor(() => socket.types().includes("auth.client_proof"), "发出 clientProof");
  socket.deliver({
    protocolVersion: 1,
    type: "auth.authenticated",
    messageId: "8a536862-9a18-4766-b4ce-a2e478734980",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: {
      deviceId: DEVICE_ID,
      scopes: ["session.list", "session.read"],
      serverEpoch: SERVER_EPOCH,
      headGlobalSequence: "10",
      heartbeatIntervalMs: 30000,
      limits: { maxMessageBytes: 1048576, maxPromptBytes: 262144, maxReplayEventsPerBatch: 500 },
    },
  });
  await waitFor(() => socket.types().includes("sync.subscribe"), "发出 subscribe");
}

/** 从 `auth.client_hello` 里取回客户端 nonce（握手输入的只读回读）。 */
async function clientNonceOf(harness: Harness): Promise<Uint8Array<ArrayBuffer>> {
  const hello = harness.sockets.latest.last("auth.client_hello");
  const nonce = (hello?.["body"] as Record<string, unknown> | undefined)?.["clientNonce"];
  return decodeBase64Url(String(nonce));
}

/** 一个游标。 */
function cursor(globalSequence: string): { readonly serverEpoch: string; readonly globalSequence: string } {
  return { serverEpoch: SERVER_EPOCH, globalSequence };
}

/** 一条 `sync.snapshot_chunk` 的**原始 wire 文本**（digest 按原始字节计算）。 */
function chunkText(input: {
  readonly snapshotId: string;
  readonly chunkIndex: string;
  readonly resource: "sessions" | "workspaces" | "agents";
  readonly items: readonly unknown[];
}): string {
  return JSON.stringify({
    protocolVersion: 1,
    type: "sync.snapshot_chunk",
    messageId: "aaaaaaaa-1111-4111-8111-000000000002",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: {
      snapshotId: input.snapshotId,
      chunkIndex: input.chunkIndex,
      resource: input.resource,
      items: input.items,
    },
  });
}

/** 一条会话摘要条目。 */
function sessionItem(sessionId: string, title: string): Record<string, unknown> {
  return {
    sessionId,
    title,
    agent: { agentId: "claude", name: "Claude" },
    state: "idle",
    origin: { kind: "local" },
    currentMode: null,
    version: "1",
    createdAt: "2026-10-01T00:00:00.000Z",
    updatedAt: "2026-10-01T00:00:00.000Z",
    workspace: { alias: "work-api", displayName: "api" },
  };
}

/**
 * 用**独立实现**（Node `crypto.createHash`）算 §9.4 的两级快照摘要。
 *
 * 期望值不经过组合根的 `digestPort`：真实端口若被换成替身，本函数的结果不会变，
 * 于是快照校验必然失败——这让「真实 digestPort 参与替换」成为可判定的。
 */
async function twoLevelDigest(rawTexts: readonly string[]): Promise<string> {
  const joined = new Uint8Array(rawTexts.length * 32);
  rawTexts.forEach((text, index) => {
    joined.set(new Uint8Array(createHash("sha256").update(text, "utf8").digest()), index * 32);
  });
  return createHash("sha256").update(joined).digest("base64url");
}

function snapshotBegin(snapshotId: string, at: ReturnType<typeof cursor>): unknown {
  return {
    protocolVersion: 1,
    type: "sync.snapshot_begin",
    messageId: "aaaaaaaa-1111-4111-8111-000000000001",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: { snapshotId, cursor: at, schemaVersion: 1, chunkCount: 3 },
  };
}

function snapshotEnd(snapshotId: string, at: ReturnType<typeof cursor>, digest: string): unknown {
  return {
    protocolVersion: 1,
    type: "sync.snapshot_end",
    messageId: "aaaaaaaa-1111-4111-8111-000000000003",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: { snapshotId, cursor: at, chunkCount: 3, snapshotDigest: digest },
  };
}


/** 让出若干轮宏任务，使在途的真实 `crypto.subtle.digest` promise 有机会 resolve。 */
async function settle(rounds = 8): Promise<void> {
  for (let round = 0; round < rounds; round += 1) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
  }
}

/** 仓库里当前会话标题序列。 */
function storedTitles(composition: Composition): readonly (string | null)[] {
  return (composition.store.state.resources?.sessions ?? []).map((item) => item.title);
}

describe("R14：重建快照时未完成的暂存内容不得覆盖已完成状态", () => {
  it("reset_required 丢弃未完成的 3-chunk 暂存区，仓库仍持有上一次已完成的快照", async () => {
    installFakeIndexedDB();
    const hostKey = await generateHostKey();
    const harness = await setupPaired(hostKey);
    try {
      const socket = harness.sockets.latest;
      await authenticate(harness, hostKey);

      // 第一份：验证通过，替换仓库。
      const completedId = "8194de43-e213-423d-acf4-2e3549304566";
      const completedAt = cursor("10");
      const completedChunks = [
        chunkText({ snapshotId: completedId, chunkIndex: "0", resource: "sessions", items: [sessionItem("5d73cd10-a465-43cd-b3f1-704e2d49e99e", COMPLETED_TITLE)] }),
        chunkText({ snapshotId: completedId, chunkIndex: "1", resource: "workspaces", items: [{ alias: "work-api", displayName: "api" }] }),
        chunkText({ snapshotId: completedId, chunkIndex: "2", resource: "agents", items: [{ agentId: "codex-a", displayName: "codex", default: true }] }),
      ];
      socket.deliver(snapshotBegin(completedId, completedAt));
      for (const text of completedChunks) socket.deliver(text);
      socket.deliver(snapshotEnd(completedId, completedAt, await twoLevelDigest(completedChunks)));
      await waitFor(() => harness.composition.store.state.resources !== null, "第一份快照入仓");
      expect(storedTitles(harness.composition)).toEqual([COMPLETED_TITLE]);

      // 第二份：只到 chunk 1/3 —— 未完成的暂存区。
      const pendingId = "5b0d4c1e-6d4a-4a1e-9c3d-2f7b6c8d9e01";
      const pendingAt = cursor("11");
      socket.deliver(snapshotBegin(pendingId, pendingAt));
      socket.deliver(
        chunkText({ snapshotId: pendingId, chunkIndex: "0", resource: "sessions", items: [sessionItem("6b62cd10-a465-43cd-b3f1-704e2d49e99f", INCOMPLETE_TITLE)] }),
      );
      socket.deliver(chunkText({ snapshotId: pendingId, chunkIndex: "1", resource: "workspaces", items: [] }));
      await settle();
      // 未完成 → 仓库**没有**被碰过。
      expect(storedTitles(harness.composition)).toEqual([COMPLETED_TITLE]);

      // 服务端要求重建快照（§9.4）：未完成的暂存区必须丢弃，并重新请求快照。
      socket.deliver({
        protocolVersion: 1,
        type: "sync.reset_required",
        messageId: "dddddddd-1111-4111-8111-000000000001",
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: { reason: "cursor_expired", snapshotAvailable: true },
      });
      await waitFor(() => socket.types().includes("sync.snapshot_request"), "发出 snapshot_request");
      await settle();
      // 反例：若 reset 时未完成内容被提交，或已完成状态被覆盖，这里会红。
      expect(storedTitles(harness.composition)).toEqual([COMPLETED_TITLE]);

      // 补刀：把那份被 reset 打断的快照的**剩余 chunk** 补齐，并按完整三 chunk 的摘要收尾。
      // 反例：若 `reset_required` 没有丢弃暂存区，这份未完成的快照此刻会补齐并通过校验，
      // 从而**覆盖**已完成状态 → 下面的 titles 断言会红。这条把「暂存区真的被丢弃了」
      // 变成可观测的事实，而不是从「仓库没变」反推。
      const pendingChunks = [
        chunkText({ snapshotId: pendingId, chunkIndex: "0", resource: "sessions", items: [sessionItem("6b62cd10-a465-43cd-b3f1-704e2d49e99f", INCOMPLETE_TITLE)] }),
        chunkText({ snapshotId: pendingId, chunkIndex: "1", resource: "workspaces", items: [] }),
        chunkText({ snapshotId: pendingId, chunkIndex: "2", resource: "agents", items: [] }),
      ];
      socket.deliver(chunkText({ snapshotId: pendingId, chunkIndex: "2", resource: "agents", items: [] }));
      socket.deliver(snapshotEnd(pendingId, pendingAt, await twoLevelDigest(pendingChunks)));
      await settle();
      expect(storedTitles(harness.composition)).toEqual([COMPLETED_TITLE]);

      // 第三份：重建后的快照验证通过 → 才允许替换（证明上一步确实只是「未提交」而非「已损坏」）。
      const rebuiltId = "c7e8f9a0-1b2c-4d3e-8f5a-6b7c8d9e0f1a";
      const rebuiltAt = cursor("20");
      const rebuiltChunks = [
        chunkText({ snapshotId: rebuiltId, chunkIndex: "0", resource: "sessions", items: [sessionItem("7c93de21-b576-4cf2-a1f0-815c26e5aff00", "重建后的会话")] }),
        chunkText({ snapshotId: rebuiltId, chunkIndex: "1", resource: "workspaces", items: [{ alias: "work-api", displayName: "api" }] }),
        chunkText({ snapshotId: rebuiltId, chunkIndex: "2", resource: "agents", items: [{ agentId: "codex-a", displayName: "codex", default: true }] }),
      ];
      socket.deliver(snapshotBegin(rebuiltId, rebuiltAt));
      for (const text of rebuiltChunks) socket.deliver(text);
      socket.deliver(snapshotEnd(rebuiltId, rebuiltAt, await twoLevelDigest(rebuiltChunks)));
      await waitFor(() => storedTitles(harness.composition)[0] === "重建后的会话", "重建后的快照替换仓库");

      // 重建后从未出现过那份未完成的内容：它被丢弃了，而不是延后提交。
      expect(storedTitles(harness.composition)).toEqual(["重建后的会话"]);
    } finally {
      harness.composition.dispose();
    }
  });

  it("新的 snapshot_begin 取代仍在的暂存区：被取代的那份内容永不提交", async () => {
    installFakeIndexedDB();
    const hostKey = await generateHostKey();
    const harness = await setupPaired(hostKey);
    try {
      const socket = harness.sockets.latest;
      await authenticate(harness, hostKey);

      // 第一份完成态。
      const completedId = "8194de43-e213-423d-acf4-2e3549304566";
      const completedAt = cursor("10");
      const completedChunks = [
        chunkText({ snapshotId: completedId, chunkIndex: "0", resource: "sessions", items: [sessionItem("5d73cd10-a465-43cd-b3f1-704e2d49e99e", COMPLETED_TITLE)] }),
        chunkText({ snapshotId: completedId, chunkIndex: "1", resource: "workspaces", items: [] }),
        chunkText({ snapshotId: completedId, chunkIndex: "2", resource: "agents", items: [] }),
      ];
      socket.deliver(snapshotBegin(completedId, completedAt));
      for (const text of completedChunks) socket.deliver(text);
      socket.deliver(snapshotEnd(completedId, completedAt, await twoLevelDigest(completedChunks)));
      await waitFor(() => harness.composition.store.state.resources !== null, "第一份快照入仓");

      // 送一份只到 chunk 1/3 的快照，然后**不**发 reset，而是直接来新的 begin。
      const staleId = "5b0d4c1e-6d4a-4a1e-9c3d-2f7b6c8d9e01";
      socket.deliver(snapshotBegin(staleId, cursor("11")));
      socket.deliver(
        chunkText({ snapshotId: staleId, chunkIndex: "0", resource: "sessions", items: [sessionItem("6b62cd10-a465-43cd-b3f1-704e2d49e99f", INCOMPLETE_TITLE)] }),
      );

      const freshId = "c7e8f9a0-1b2c-4d3e-8f5a-6b7c8d9e0f1a";
      const freshAt = cursor("12");
      const freshChunks = [
        chunkText({ snapshotId: freshId, chunkIndex: "0", resource: "sessions", items: [sessionItem("8da4ef32-c687-4d03-b201-926d37f6b0011", "取代后的会话")] }),
        chunkText({ snapshotId: freshId, chunkIndex: "1", resource: "workspaces", items: [] }),
        chunkText({ snapshotId: freshId, chunkIndex: "2", resource: "agents", items: [] }),
      ];
      socket.deliver(snapshotBegin(freshId, freshAt));
      for (const text of freshChunks) socket.deliver(text);
      socket.deliver(snapshotEnd(freshId, freshAt, await twoLevelDigest(freshChunks)));
      await waitFor(() => storedTitles(harness.composition)[0] === "取代后的会话", "新快照替换仓库");

      // 反例：若被取代的暂存区内容被合并进仓库，标题会是两项 → 断言变红。
      expect(storedTitles(harness.composition)).toEqual(["取代后的会话"]);
    } finally {
      harness.composition.dispose();
    }
  });

  it("验证失败的快照不提交，且不覆盖已完成状态（摘要不符）", async () => {
    installFakeIndexedDB();
    const hostKey = await generateHostKey();
    const harness = await setupPaired(hostKey);
    try {
      const socket = harness.sockets.latest;
      await authenticate(harness, hostKey);

      const completedId = "8194de43-e213-423d-acf4-2e3549304566";
      const completedAt = cursor("10");
      const completedChunks = [
        chunkText({ snapshotId: completedId, chunkIndex: "0", resource: "sessions", items: [sessionItem("5d73cd10-a465-43cd-b3f1-704e2d49e99e", COMPLETED_TITLE)] }),
        chunkText({ snapshotId: completedId, chunkIndex: "1", resource: "workspaces", items: [] }),
        chunkText({ snapshotId: completedId, chunkIndex: "2", resource: "agents", items: [] }),
      ];
      socket.deliver(snapshotBegin(completedId, completedAt));
      for (const text of completedChunks) socket.deliver(text);
      socket.deliver(snapshotEnd(completedId, completedAt, await twoLevelDigest(completedChunks)));
      await waitFor(() => harness.composition.store.state.resources !== null, "第一份快照入仓");

      // 第二份：结构合法但摘要取错值。
      const badId = "5b0d4c1e-6d4a-4a1e-9c3d-2f7b6c8d9e01";
      const badAt = cursor("11");
      const badChunks = [
        chunkText({ snapshotId: badId, chunkIndex: "0", resource: "sessions", items: [sessionItem("6b62cd10-a465-43cd-b3f1-704e2d49e99f", INCOMPLETE_TITLE)] }),
        chunkText({ snapshotId: badId, chunkIndex: "1", resource: "workspaces", items: [] }),
        chunkText({ snapshotId: badId, chunkIndex: "2", resource: "agents", items: [] }),
      ];
      socket.deliver(snapshotBegin(badId, badAt));
      for (const text of badChunks) socket.deliver(text);
      socket.deliver(snapshotEnd(badId, badAt, "A".repeat(43)));
      await settle();

      // 反例：若验证失败仍提交（或用未完成内容覆盖），这里会红。
      expect(storedTitles(harness.composition)).toEqual([COMPLETED_TITLE]);
      expect(harness.composition.store.state.resources?.agents).toHaveLength(0);
    } finally {
      harness.composition.dispose();
    }
  });
});