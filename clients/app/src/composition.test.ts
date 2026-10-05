/**
 * 组合根的接线正确性（node 级，注入替身端口）。
 *
 * ## 覆盖什么、不覆盖什么
 *
 * 覆盖：五个平台端口的**装配**是否正确、`SyncClient` 的事件流是否真的到达 `ClientStore`、
 * `ConnectionIdentity` 是否只在认证后才给出 `connectionId`、配对 HTTPS 面是否按 §7.2/§7.3
 * 的路径与形状发请求、以及重连是否由状态机结论驱动。
 *
 * **不覆盖**：真实 IndexedDB、真实 WebCrypto 密钥生成与真实 `WebSocket`。那些属于
 * `src/platform/` 的浏览器验证（`scripts/run-browser-check.mjs` 的 16 条断言）与 TP4 的
 * 浏览器证据。本文件因此注入**端口替身**而不是替换浏览器环境——替身只替换平台能力，
 * 被测的仍然是组合根真实的装配代码。
 *
 * ## 判别力来源
 *
 * - `connectionId` 一条：若组合根用 `null` 或编造值填充 `ConnectionIdentity`，
 *   `submitCreateSession` 会在认证前就派发命令（断言红），认证后才派发（断言绿）。
 * - 事件流一条：若 `onEvent` 没接到 `store.apply`，快照永远进不了 `store.state.resources`，
 *   目录页渲染的是空态。
 * - 配对 HTTPS 一条：断言 URL、method 与请求体字段——路径写错、字段缺失都变红。
 * - 重连一条：注入假时钟，只在 `advanceBy` 后才到期；退避期间断言「未重连」，
 *   因此「无条件立刻重连」与「永不重连」两种实现都会红。
 */

import { describe, expect, it } from "vitest";

import { createComposition } from "./composition";
import type { Composition, FetchLike, PairingHttpResponse } from "./composition";
import {
  FakeClock,
  FakeSocketFactory,
  expectedSnapshotDigest,
  fakeDigest,
  makeAuthenticated,
  makeChallenge,
  makeSnapshotBegin,
  makeSnapshotChunk,
  makeSnapshotEnd,
  waitUntil,
  type FakeSocket,
} from "./sync-client/testing/harness";
import type {
  CacheEntry,
  CacheHit,
  CacheStats,
  DeviceIdentity,
  DeviceIdentityPort,
  LifecycleEvent,
  LifecyclePort,
  LocalCachePort,
  VisibilityState,
} from "./platform";
import { createVolatileImportedContent } from "./platform";
import type { HostIdentityPort, PairedHostRecord } from "./sync-client";

const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const ORIGIN = "https://work-pc.example.ts.net";
const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const DEVICE_ID = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
const SESSION_ID = "5d73cd10-a465-43cd-b3f1-704e2d49e99e";
/** claim 响应的 `pairingRequestId`：状态轮询请求必须带它（§7.3），因此是合法 UUID。 */
const PAIRING_REQUEST_ID = "b2f3fbfa-c2c6-4f49-9e0c-643d152de18d";

/** 组合根的首个退避窗口与上限（`src/composition.ts` 的 `RECONNECT`）；这里只引用，不复制实现。 */
const RECONNECT_BASE_DELAY_MS = 500;
const RECONNECT_MAX_WINDOW_MS = 60_000;

/** 内存版本地缓存：只有组合根用到的三个方法有真实语义，其余是占位。 */
class MemoryLocalCache implements LocalCachePort {
  readonly entries = new Map<string, CacheEntry>();

  async putLocalSummary(input: { key: string; summary: string; nowMs: number }): Promise<void> {
    this.entries.set(input.key, {
      kind: "local",
      key: input.key,
      summary: input.summary,
      updatedAtMs: input.nowMs,
      lastUsedAtMs: input.nowMs,
    });
  }

  async putImportedMetadata(): Promise<void> {
    throw new Error("组合根不写 imported 元数据");
  }

  async get(key: string): Promise<CacheHit | null> {
    const entry = this.entries.get(key);
    return entry === undefined ? null : { entry, remainingTtlMs: 1 };
  }

  async clear(): Promise<void> {
    this.entries.clear();
  }

  async stats(): Promise<CacheStats> {
    return { localEntries: this.entries.size, localBytes: 0, importedEntries: 0, importedBytes: 0, expiredEntries: 0 };
  }
}

/** 替身设备身份：有身份时 `signDeviceProof` 产出一段可辨识的字节。 */
function fakeDeviceIdentity(present: boolean): DeviceIdentityPort {
  const material = (): DeviceIdentity =>
    ({
      deviceId: DEVICE_ID,
      canonicalOrigin: ORIGIN,
      publicKeyBase64Url: "AQIDBA",
      privateKey: {} as CryptoKey,
    }) as DeviceIdentity;
  return {
    async loadIdentity(): Promise<DeviceIdentity | null> {
      return present ? material() : null;
    },
    async ensureIdentity(): Promise<DeviceIdentity> {
      return material();
    },
    async signDeviceProof(): Promise<Uint8Array<ArrayBuffer>> {
      return new Uint8Array(64);
    },
    async clearIdentity(): Promise<void> {
      return;
    },
  };
}

/** 替身生命周期：只保留订阅，回调由测试显式触发（平台事件无法在 node 里伪造出真实来源）。 */
class FakeLifecycle implements LifecyclePort {
  #listener: ((event: LifecycleEvent) => void) | null = null;

  subscribe(listener: (event: LifecycleEvent) => void): () => void {
    this.#listener = listener;
    return () => {
      this.#listener = null;
    };
  }

  currentVisibility(): VisibilityState {
    return "visible";
  }

  async requestPersistentStorage(): Promise<boolean> {
    return false;
  }

  /** 测试专用：投递一次真实形状的生命周期事件。 */
  emit(event: LifecycleEvent): void {
    this.#listener?.(event);
  }
}

/** 组合根的被测环境。 */
interface Harness {
  readonly composition: Composition;
  readonly sockets: FakeSocketFactory;
  readonly cache: MemoryLocalCache;
  readonly lifecycle: FakeLifecycle;
  readonly fetches: { url: string; method: string; body: string }[];
}

/**
 * 只做「读已配对记录」这一步的主机身份端口替身。
 *
 * 记录**从同一个内存缓存读**，因此「未配对」这条路径仍被真实地走到；被替换掉的只是 ECDSA
 * 验签——它需要一对真实密钥，由下面「真实主机身份端口」那组用例单独覆盖。
 */
function cacheBackedHost(cache: MemoryLocalCache): HostIdentityPort {
  return {
    async loadPairedHost(canonicalOrigin: string): Promise<PairedHostRecord | null> {
      const hit = await cache.get(`paired-host/${canonicalOrigin}`);
      if (hit === null || hit.entry.kind !== "local") return null;
      const parsed = JSON.parse(hit.entry.summary) as PairedHostRecord;
      return parsed;
    },
    async verifyHostChallenge(): Promise<boolean> {
      return true;
    },
  };
}

/** 组合根的被测环境；`clock` 是注入的假时钟，因此退避可在确定性时间下推进。 */
interface Harness {
  readonly composition: Composition;
  readonly sockets: FakeSocketFactory;
  readonly cache: MemoryLocalCache;
  readonly lifecycle: FakeLifecycle;
  readonly clock: FakeClock;
  readonly fetches: { url: string; method: string; body: string }[];
}

function setup(input: { readonly fetch?: FetchLike } = {}): Harness {
  const sockets = new FakeSocketFactory();
  const cache = new MemoryLocalCache();
  const lifecycle = new FakeLifecycle();
  const clock = new FakeClock(1_700_000_000_000);
  const fetches: { url: string; method: string; body: string }[] = [];
  const composition = createComposition({
    platform: {
      deviceIdentity: fakeDeviceIdentity(true),
      localCache: cache,
      importedContent: createVolatileImportedContent(),
      lifecycle,
    },
    openSocket: sockets.open,
    // 端口经 `ports` 接缝注入：退避定时与快照 digest 因此不需要真实时间与真实 SHA-256，
    // 而被测的仍然是组合根真实的装配代码（不是另写一份装配）。
    // 主机身份端口只替换「验签」这一步（需要真实密钥，另见「真实主机身份端口」一组用例）；
    // 「读已配对记录」仍走真实路径，因此未配对时它会返回 null。
    ports: { clock, digest: fakeDigest(), host: cacheBackedHost(cache) },
    fetch:
      input.fetch ??
      (async (url, init): Promise<PairingHttpResponse> => {
        fetches.push({ url, method: init.method, body: init.body });
        return { ok: true, status: 201, json: () => Promise.resolve({ pairingRequestId: PAIRING_REQUEST_ID }) };
      }),
  });
  return { composition, sockets, cache, lifecycle, clock, fetches };
}

/** 交付一份三 chunk 快照（会话 / 目录 / Agent）并等待它进入 store。 */
async function deliverSnapshot(socket: FakeSocket): Promise<void> {
  const snapshotId = "8194de43-e213-423d-acf4-2e3549304566";
  const cursor = { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" };
  socket.deliver(makeSnapshotBegin({ snapshotId, chunkCount: 3, cursor }));
  const chunks = [
    makeSnapshotChunk({
      snapshotId,
      chunkIndex: "0",
      resource: "sessions",
      items: [
        {
          sessionId: SESSION_ID,
          title: "组合根接线",
          agent: { agentId: "claude", name: "Claude" },
          state: "idle",
          origin: { kind: "local" },
          currentMode: null,
          version: "1",
          createdAt: "2026-10-01T00:00:00.000Z",
          updatedAt: "2026-10-01T00:00:00.000Z",
          workspace: { alias: "work-api", displayName: "api" },
        },
      ],
    }),
    makeSnapshotChunk({ snapshotId, chunkIndex: "1", resource: "workspaces", items: [{ alias: "work-api", displayName: "api" }] }),
    makeSnapshotChunk({ snapshotId, chunkIndex: "2", resource: "agents", items: [{ agentId: "codex-a", displayName: "codex", default: true }] }),
  ];
  for (const chunk of chunks) socket.deliver(chunk.rawText);
  socket.deliver(
    makeSnapshotEnd({ snapshotId, chunkCount: 3, digest: expectedSnapshotDigest(chunks.map((chunk) => chunk.rawText)), cursor }),
  );
}

/**
 * 跑完握手并停在「已认证、已发 subscribe」。
 *
 * `scopes` 显式包含 `session.create`：创建会话有四道闸门，要证明**第一道**（已建立连接）
 * 在起作用，其余三道就必须放行，否则「被拒」可能来自权限而不是缺少 `connectionId`，
 * 用例就没有判别力（它会被任何一种拒绝理由满足）。
 */
async function authenticate(socket: FakeSocket, scopes: readonly string[] = ["session.create"]): Promise<void> {
  socket.open();
  await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
  socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
  await waitUntil(() => socket.types().includes("auth.client_proof"), "发出 clientProof");
  const authenticated = makeAuthenticated({ connectionId: CONNECTION_ID });
  socket.deliver({ ...authenticated, body: { ...authenticated.body, scopes: [...scopes] } });
  await waitUntil(() => socket.types().includes("sync.subscribe"), "发出 subscribe");
}

describe("组合根：未配对时不连接、不编造主机身份", () => {
  it("缓存里没有配对记录：store 可渲染且停在未配对，一条 socket 都不建", async () => {
    const context = setup();
    await context.composition.start();

    // 反例：若组合根无条件连接，这里会是 1 条 → 断言变红。
    expect(context.sockets.sockets).toHaveLength(0);
    expect(context.composition.store.state.connection.state).toBe("unpaired");
    expect(context.composition.store.hostPanel().host).toBeNull();
    context.composition.dispose();
  });
});

describe("组合根：已配对时接线成立", () => {
  it("事件流到达 store：快照入仓后目录页渲染出该会话", async () => {
    const context = setup();
    await context.composition.rememberPairedHost({ hostId: HOST_ID, publicKeyBase64Url: "AQID", canonicalOrigin: ORIGIN });
    await context.composition.start();

    const socket = context.sockets.latest;
    await authenticate(socket);
    await deliverSnapshot(socket);
    await waitUntil(
      () => context.composition.store.state.resources !== null,
      "快照经 onEvent 到达 store",
    );

    // 反例：若 `onEvent` 没接到 `store.apply`，这里仍是 null（目录页渲染空态）→ 变红。
    const page = context.composition.store.directoryPage();
    expect(page.kind).toBe("ready");
    expect(page.kind === "ready" ? page.directories.map((row) => row.displayName) : []).toEqual(["api"]);
    expect(context.composition.store.hostPanel().host).toEqual({
      address: ORIGIN,
      hostId: HOST_ID,
      deviceId: DEVICE_ID,
    });
    context.composition.dispose();
  });

  it("connectionId 只在认证后可用：认证前创建会话被拒且不派发任何命令", async () => {
    const context = setup();
    await context.composition.rememberPairedHost({ hostId: HOST_ID, publicKeyBase64Url: "AQID", canonicalOrigin: ORIGIN });
    await context.composition.start();

    const socket = context.sockets.latest;
    await authenticate(socket);
    await deliverSnapshot(socket);
    await waitUntil(() => context.composition.store.state.resources !== null, "快照入仓");

    // 认证之后：四道闸门全过（scopes 含 `session.create`、Agent 在快照目录里、载荷两键）。
    // 这条同时证明 `connectionId` 真的取到了值——否则这里会是 `ok: false`。
    const allowed = context.composition.store.submitCreateSession({ workspaceAlias: "work-api", agentId: "codex-a" });
    expect(allowed.ok).toBe(true);
    expect(socket.last("command")?.["body"]).toMatchObject({ command: "session.create" });

    // 断连之后必须回到「不可提交」：认证态失效时不得沿用旧标识。
    socket.serverClose(4500, "service_unavailable");
    await waitUntil(() => context.composition.store.state.connection.state === "reconnecting", "进入重连中");
    // 反例：若 `ConnectionIdentity` 编造一个固定值，这里仍会是 `ok: true` → 断言变红。
    expect(context.composition.store.submitCreateSession({ workspaceAlias: "work-api", agentId: "codex-a" }).ok).toBe(false);
    context.composition.dispose();
  });

  it("连接进行中（已认证 / 追平中 / 已在线）不安排也不发起第二条连接", async () => {
    const context = setup();
    await context.composition.rememberPairedHost({ hostId: HOST_ID, publicKeyBase64Url: "AQID", canonicalOrigin: ORIGIN });
    await context.composition.start();
    const socket = context.sockets.latest;
    await authenticate(socket);
    await deliverSnapshot(socket);
    await waitUntil(() => context.composition.store.state.resources !== null, "快照入仓");

    // 推进两轮退避窗口（2×、4× baseDelay）。组合根在这段时间里经历了 connecting /
    // authenticating / replaying / online 四次状态变化。
    context.clock.advanceBy(RECONNECT_BASE_DELAY_MS * 10);
    // 反例：若闸门只问「能不能自动重连」而不问「当前是否已经在连」，
    // 每次状态变化都会安排一次 connect()，这里会是 ≥2 条 → 断言变红。
    expect(context.sockets.sockets).toHaveLength(1);
    expect(context.composition.store.state.connection.state).toBe("replaying");
    context.composition.dispose();
  });

  it("pagehide 断开连接（未完成的暂存快照随之丢弃）", async () => {
    const context = setup();
    await context.composition.rememberPairedHost({ hostId: HOST_ID, publicKeyBase64Url: "AQID", canonicalOrigin: ORIGIN });
    await context.composition.start();
    await authenticate(context.sockets.latest);

    context.lifecycle.emit({ kind: "pagehide" });
    // 反例：若组合根不订阅生命周期，这里不会关闭 → 断言变红。
    expect(context.sockets.latest.closedWith).not.toBeNull();
    context.composition.dispose();
  });
});

describe("组合根：重连由状态机结论驱动，且退避有界", () => {
  it("可重试的断开后按退避重连；阻断态下不再重连", async () => {
    const context = setup();
    await context.composition.rememberPairedHost({ hostId: HOST_ID, publicKeyBase64Url: "AQID", canonicalOrigin: ORIGIN });
    await context.composition.start();
    await authenticate(context.sockets.latest);
    expect(context.sockets.sockets).toHaveLength(1);

    // 退避由注入的假时钟驱动：到期前断言「还没重连」，因此「立刻重连」与「永不重连」都会红。
    context.sockets.latest.serverClose(4500, "service_unavailable");
    await waitUntil(() => context.composition.store.state.connection.state === "reconnecting", "进入重连中");
    expect(context.sockets.sockets).toHaveLength(1);

    context.clock.advanceBy(RECONNECT_BASE_DELAY_MS);
    await waitUntil(() => context.sockets.sockets.length >= 2, "退避到期后重连");
    const second = context.sockets.latest;
    second.open();
    await waitUntil(() => second.types().includes("auth.client_hello"), "第二条连接发出 clientHello");

    // 阻断：4411 → `replaced_by_other_connection`，状态机要求停止自动重连（R12）。
    second.serverClose(4411, "connection_replaced");
    await waitUntil(() => context.composition.store.blocked, "进入阻断态");
    context.clock.advanceBy(RECONNECT_MAX_WINDOW_MS);
    // 反例：若组合根不看状态机结论就重连，这里会 ≥3 条 → 断言变红。
    expect(context.sockets.sockets).toHaveLength(2);
    context.composition.dispose();
  });
});

describe("组合根：配对 HTTPS 面按 §7.2/§7.3 的路径与形状发请求", () => {
  const QR = {
    pairingProtocol: "acp-remote-pairing-v1",
    hostId: HOST_ID,
    hostPublicKey: "AQID",
    canonicalOrigin: ORIGIN,
    pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
    pairingSecret: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
    expiresAt: new Date(Date.now() + 600_000).toISOString(),
  };

  it("二维码形状校验：过期、协议不符、缺字段一律抛错（不尽力解析）", () => {
    const context = setup();
    expect(() => context.composition.pairing.readQrPayload("{")).toThrow(/JSON/);
    expect(() => context.composition.pairing.readQrPayload(JSON.stringify({ ...QR, pairingProtocol: "other" }))).toThrow(/协议/);
    expect(() => context.composition.pairing.readQrPayload(JSON.stringify({ ...QR, hostId: undefined }))).toThrow(/hostId/);
    expect(() =>
      context.composition.pairing.readQrPayload(JSON.stringify({ ...QR, expiresAt: new Date(0).toISOString() })),
    ).toThrow(/过期/);
    context.composition.dispose();
  });

  it("claim 发往 /sync/v1/pairing/claim，body 含 pairingId/deviceId/clientNonce/proof", async () => {
    const context = setup();
    const error = await context.composition.pairing.claim({
      qr: context.composition.pairing.readQrPayload(JSON.stringify(QR)),
      canonicalOrigin: ORIGIN,
    });

    expect(error).toBeNull();
    expect(context.fetches).toHaveLength(1);
    const request = context.fetches[0];
    // 反例：路径或方法写错（§7.2）→ 断言变红。
    expect(request?.url).toBe(`${ORIGIN}/sync/v1/pairing/claim`);
    expect(request?.method).toBe("POST");
    const body = JSON.parse(request?.body ?? "{}") as Record<string, unknown>;
    expect(body["pairingId"]).toBe(QR.pairingId);
    expect(body["hostId"]).toBe(QR.hostId);
    expect(body["deviceId"]).toBe(DEVICE_ID);
    expect(body["clientKind"]).toBe("pwa");
    expect(typeof body["proof"]).toBe("string");
    // 凭据只走 body，不进 URL（§7.1）。
    expect(request?.url).not.toContain(QR.pairingSecret);
    context.composition.dispose();
  });

  it("claim 失败：HTTP 状态映射为封闭词表内的配对错误码", async () => {
    const context = setup({
      fetch: async (): Promise<PairingHttpResponse> => ({
        ok: false,
        status: 410,
        json: () => Promise.resolve({ message: "已过期" }),
      }),
    });
    const error = await context.composition.pairing.claim({
      qr: context.composition.pairing.readQrPayload(JSON.stringify(QR)),
      canonicalOrigin: ORIGIN,
    });

    // 反例：若不映射状态而直接透传未知码，R18/配对页的文案分支就落空 → 断言变红。
    expect(error?.code).toBe("pairing.expired");
    expect(error?.message).toBe("已过期");
    context.composition.dispose();
  });

  it("状态轮询：pending/approved/终态/未知各自分派（业务状态只由 body 表达）", async () => {
    let polls = 0;
    const recorded: { url: string; method: string; body: string }[] = [];
    const context = setup({
      // claim 与 status 是两个端点（§7.2/§7.3）：claim 给 `pairingRequestId`，status 给业务状态。
      fetch: async (_url, init): Promise<PairingHttpResponse> => {
        recorded.push({ url: _url, method: init.method, body: init.body });
        if (JSON.parse(init.body)["pairingRequestId"] === undefined) {
          return { ok: true, status: 201, json: () => Promise.resolve({ pairingRequestId: PAIRING_REQUEST_ID }) };
        }
        polls += 1;
        return { ok: true, status: 200, json: () => Promise.resolve(bodyFor(polls)) };
      },
    });
    const qr = context.composition.pairing.readQrPayload(JSON.stringify(QR));
    await context.composition.pairing.claim({ qr, canonicalOrigin: ORIGIN });

    // 未声明过配对时轮询明确失败，而不是发一个缺 `pairingRequestId` 的请求。
    const fresh = setup();
    const freshQr = fresh.composition.pairing.readQrPayload(JSON.stringify(QR));
    expect(await fresh.composition.pairing.poll({ qr: freshQr, requestNonce: "AAECAwQFBgcICQoLDA0ODw" })).toMatchObject({
      kind: "error",
    });

    const requestNonce = "AAECAwQFBgcICQoLDA0ODw";
    expect(await context.composition.pairing.poll({ qr, requestNonce })).toEqual({ kind: "pending_confirmation" });
    expect(await context.composition.pairing.poll({ qr, requestNonce })).toEqual({ kind: "approved", deviceId: DEVICE_ID });
    expect(await context.composition.pairing.poll({ qr, requestNonce })).toMatchObject({ kind: "terminal", status: "expired" });
    expect(await context.composition.pairing.poll({ qr, requestNonce })).toMatchObject({ kind: "error" });

    const last = recorded[recorded.length - 1];
    // 反例：状态轮询端点写错（§7.3）→ 断言变红。
    expect(last?.url).toBe(`${ORIGIN}/sync/v1/pairing/status`);
    const statusRequest = JSON.parse(last?.body ?? "{}") as Record<string, unknown>;
    expect(statusRequest["pairingRequestId"]).toBe(PAIRING_REQUEST_ID);
    expect(statusRequest["requestNonce"]).toBe(requestNonce);
    context.composition.dispose();
    fresh.composition.dispose();
  });
});

/** 轮询响应的第四种取值：正文里的 `status` 不在封闭词表内。 */
function bodyFor(index: number): Record<string, unknown> {
  if (index === 1) return { status: "pending_confirmation" };
  if (index === 2) return { status: "approved", device: { deviceId: DEVICE_ID } };
  if (index === 3) return { status: "expired", message: "这次配对二维码已过期。" };
  return { status: "weird_value" };
}

/** 让 `FakeSocketFactory` 的 `open` 与本文件用到的类型保持一致（`SocketPort` 的最小面）。 */
