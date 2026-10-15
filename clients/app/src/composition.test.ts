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

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { PairingPanel } from "./components";
import { createComposition } from "./composition";
import type { Composition, CompositionPorts, FetchLike, PairingHttpResponse } from "./composition";
import { PAIRING_POLL_INTERVAL_MS, isPairingInFlight } from "./features/pairing-model";
import { encodeBase64Url } from "./sync-client";
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

/**
 * 二维码里的主机公钥：65 字节 SEC1 未压缩点的 base64url（87 字符）。
 *
 * §7.1 要求 PWA 校验字段长度，因此夹具的形状必须真（截短的字符串会被判成结构错误）。
 */
const QR_HOST_PUBLIC_KEY = "BAcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";

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

function setup(
  input: {
    readonly fetch?: FetchLike;
    readonly realHostIdentity?: boolean;
    /**
     * 设备身份替身：只给需要「读到身份这一步**本身**失败」的用例替换（收口失败的文案）。
     * 缺省仍是有身份的真实形状替身。
     */
    readonly deviceIdentity?: DeviceIdentityPort;
  } = {},
): Harness {
  const sockets = new FakeSocketFactory();
  const cache = new MemoryLocalCache();
  const lifecycle = new FakeLifecycle();
  const clock = new FakeClock(1_700_000_000_000);
  const fetches: { url: string; method: string; body: string }[] = [];
  // 主机身份端口只替换「验签」这一步（需要真实密钥，另见「真实主机身份端口」一组用例）；
  // 「读已配对记录」仍走真实路径，因此未配对时它会返回 null。
  // `realHostIdentity` 则连验签也用生产实现（`PairedHostIdentity` + WebCrypto），
  // 于是「记录缺公钥 → 验不过」这条反例可以被真的走到。
  const ports: Partial<CompositionPorts> = input.realHostIdentity === true
    ? { clock, digest: fakeDigest() }
    : { clock, digest: fakeDigest(), host: cacheBackedHost(cache) };
  // 默认替身：claim 成功给 `pairingRequestId`。注入的替身同样经下面那一层记账，
  // 因此断言「发了什么」与「一个都没发」都落在 `fetches` 上（不因替换而失去证据）。
  const respond: FetchLike =
    input.fetch ??
    (async (): Promise<PairingHttpResponse> => ({
      ok: true,
      status: 201,
      json: () => Promise.resolve({ pairingRequestId: PAIRING_REQUEST_ID }),
    }));
  const composition = createComposition({
    platform: {
      deviceIdentity: input.deviceIdentity ?? fakeDeviceIdentity(true),
      localCache: cache,
      importedContent: createVolatileImportedContent(),
      lifecycle,
    },
    openSocket: sockets.open,
    // 端口经 `ports` 接缝注入：退避定时与快照 digest 因此不需要真实时间与真实 SHA-256，
    // 而被测的仍然是组合根真实的装配代码（不是另写一份装配）。
    ports,
    fetch: async (url, init) => {
      fetches.push({ url, method: init.method, body: init.body });
      return respond(url, init);
    },
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

  it("连接机说可以连、门面却仍在阻断态：把拒绝原因交回 store，而不是静默停在「已断开」", async () => {
    const context = setup({ realHostIdentity: true });
    await startBlockedOnMissingHostKey(context);
    expect(context.composition.store.state.connection.state).toBe("identity_changed");

    // 只推进连接机（`pairSucceeded` 就是 `ClientStore` 自己的配对收口动作）：状态回到
    // `disconnected`，于是退避重连会去调 `sync.connect()`——而门面的阻断还没有人解除。
    context.composition.store.pairSucceeded();
    expect(context.composition.store.state.connection.state).toBe("disconnected");

    context.clock.advanceBy(RECONNECT_BASE_DELAY_MS);
    // 反例：若 `connectOnce` 把 `connect()` 的异常吞掉（`catch { return; }`），这里会停在
    // `disconnected`（既没连接、也看不到任何原因）→ 两条断言都变红。
    expect(context.composition.store.state.connection.state).toBe("identity_changed");
    expect(context.composition.store.state.connection.blocking?.cause).toBe("host_identity_changed");
    expect(context.sockets.sockets).toHaveLength(1);
    context.composition.dispose();
  });
});

describe("组合根：配对 HTTPS 面按 §7.2/§7.3 的路径与形状发请求", () => {
  const QR = {
    pairingProtocol: "acp-remote-pairing-v1",
    hostId: HOST_ID,
    hostPublicKey: QR_HOST_PUBLIC_KEY,
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
    // §7.1 的字段长度校验：把公钥截短（旧的「尽力解析」会照收）→ 断言变红。
    expect(() =>
      context.composition.pairing.readQrPayload(JSON.stringify({ ...QR, hostPublicKey: "AQID" })),
    ).toThrow(/hostPublicKey/);
    context.composition.dispose();
  });

  it("二维码内容接受地址 / 片段 / 载荷三种容器，载荷校验不放宽", () => {
    const context = setup();
    const json = JSON.stringify(QR);
    const payload = encodeBase64Url(new TextEncoder().encode(json));

    // 三种容器都解析出同一份载荷：地址（§7.1 的二维码形态）、片段（页面自己读片段时给它）、裸载荷。
    expect(context.composition.pairing.readQrPayload(`${ORIGIN}/pair#data=${payload}`)).toEqual(QR);
    expect(context.composition.pairing.readQrPayload(`data=${payload}`)).toEqual(QR);
    expect(context.composition.pairing.readQrPayload(payload)).toEqual(QR);
    // 容器对了但载荷不对（版本不符）仍然明确失败——容器宽松不等于载荷宽松。
    expect(() =>
      context.composition.pairing.readQrPayload(`${ORIGIN}/pair#data=${encodeBase64Url(new TextEncoder().encode("{}"))}`),
    ).toThrow(/字段/);
    // 既不是 JSON、也不是 base64url、也没有 `data=`：直接失败，不去猜。
    expect(() => context.composition.pairing.readQrPayload("把这一段贴进来")).toThrow(/不是配对二维码内容/);
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
    // 已批准时设备元数据与主机身份材料**一起**返回（缺任何一项都是 `kind: "error"`，见下一组用例）。
    expect(await context.composition.pairing.poll({ qr, requestNonce })).toEqual({
      kind: "approved",
      deviceId: DEVICE_ID,
      hostId: HOST_ID,
      hostPublicKey: APPROVED_HOST_PUBLIC_KEY,
    });
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

  it("已批准缺主机材料 / 主机标识不是同一台：可区分的失败，而不是「已批准」", async () => {
    const approvedWith = (host: Record<string, unknown>): Record<string, unknown> => ({
      status: "approved",
      device: { deviceId: DEVICE_ID },
      host,
    });
    const cases: readonly { readonly name: string; readonly body: Record<string, unknown> }[] = [
      { name: "缺 host", body: approvedWith({}) },
      { name: "缺公钥", body: approvedWith({ hostId: HOST_ID }) },
      { name: "公钥长度不符", body: approvedWith({ hostId: HOST_ID, hostPublicKey: "AQID" }) },
      { name: "主机标识不是同一台", body: approvedWith({ hostId: DEVICE_ID, hostPublicKey: QR_HOST_PUBLIC_KEY }) },
      { name: "缺设备元数据", body: { status: "approved", host: { hostId: HOST_ID, hostPublicKey: QR_HOST_PUBLIC_KEY } } },
    ];

    for (const scenario of cases) {
      let statusPolls = 0;
      const context = setup({
        fetch: async (_url, init): Promise<PairingHttpResponse> => {
          if (JSON.parse(init.body)["pairingRequestId"] === undefined) {
            return { ok: true, status: 201, json: () => Promise.resolve({ pairingRequestId: PAIRING_REQUEST_ID }) };
          }
          statusPolls += 1;
          return { ok: true, status: 200, json: () => Promise.resolve(scenario.body) };
        },
      });
      const qr = context.composition.pairing.readQrPayload(JSON.stringify(QR));
      await context.composition.pairing.claim({ qr, canonicalOrigin: ORIGIN });
      const result = await context.composition.pairing.poll({ qr, requestNonce: "AAECAwQFBgcICQoLDA0ODw" });

      // 反例：若沿用旧的「只取 deviceId」口径（或对缺材料回落到「已批准」），这里会是
      // `kind: "approved"`，客户端随后带着没有公钥的记录去连接 → 断言变红。
      expect(result.kind, scenario.name).toBe("error");
      // 这条 `continue` 只为类型收窄：真返回了 `approved` 时上一条断言已经记为失败，不会被跳过。
      if (result.kind !== "error") continue;
      expect(result.error.code, scenario.name).toBe("protocol.schema_invalid");
      expect(result.error.message, scenario.name).toContain("主机身份材料");
      context.composition.dispose();
    }
  });
});

/** 轮询响应的第四种取值：正文里的 `status` 不在封闭词表内。 */
function bodyFor(index: number): Record<string, unknown> {
  if (index === 1) return { status: "pending_confirmation" };
  if (index === 2) {
    return {
      status: "approved",
      device: { deviceId: DEVICE_ID },
      host: { hostId: HOST_ID, hostPublicKey: APPROVED_HOST_PUBLIC_KEY },
    };
  }
  if (index === 3) return { status: "expired", message: "这次配对二维码已过期。" };
  return { status: "weird_value" };
}

/**
 * 状态响应里的主机公钥（65 字节 base64url）。
 *
 * 与二维码里的那份**故意不同**：已配对记录必须取状态响应给出的主机身份材料（§7.3），
 * 取错来源（例如抄二维码）时「记录含主机公钥」这条断言会变红。
 */
const APPROVED_HOST_PUBLIC_KEY = "BAYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgY";

/** 一次成功配对的 HTTPS 面：claim 给 `pairingRequestId`，首次 status 待确认，第二次已批准。 */
function successfulPairingFetch(): FetchLike {
  let statusPolls = 0;
  return async (_url, init): Promise<PairingHttpResponse> => {
    if (JSON.parse(init.body)["pairingRequestId"] === undefined) {
      return { ok: true, status: 201, json: () => Promise.resolve({ pairingRequestId: PAIRING_REQUEST_ID }) };
    }
    statusPolls += 1;
    const body =
      statusPolls === 1
        ? { status: "pending_confirmation" }
        : {
            status: "approved",
            device: { deviceId: DEVICE_ID },
            host: { hostId: HOST_ID, hostPublicKey: APPROVED_HOST_PUBLIC_KEY },
          };
    return { ok: true, status: 200, json: () => Promise.resolve(body) };
  };
}

/** 合法二维码内容：地址形态（§7.1），片段里是 base64url 的 JSON 载荷。 */
function pairingUrl(overrides: Record<string, unknown> = {}): string {
  const payload = {
    pairingProtocol: "acp-remote-pairing-v1",
    hostId: HOST_ID,
    hostPublicKey: QR_HOST_PUBLIC_KEY,
    canonicalOrigin: ORIGIN,
    pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
    pairingSecret: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
    expiresAt: new Date(Date.now() + 600_000).toISOString(),
    ...overrides,
  };
  return `${ORIGIN}/pair#data=${encodeBase64Url(new TextEncoder().encode(JSON.stringify(payload)))}`;
}

/** 一次成功配对里那个一次性秘密（断言它不出现在任何旁路里的字面量）。 */
const PAIRING_SECRET = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

/**
 * 反复让出事件循环，直到条件成立。
 *
 * 与 harness 的 `waitUntil`（纯微任务轮转）不同：配对链路的 HMAC proof 与主机验签走
 * `crypto.subtle`，它们的完成回调落在**宏任务**上，微任务轮转驱动不了这条链。
 * 这里每一轮让出一次事件循环——`setImmediate` 表达的是「下一轮」，不是「等一段时长」，
 * 因此不引入真实时间依赖（轮数上限只把「条件永不成立」变成一条明确的失败）。
 */
async function waitForEventLoopTurn(
  predicate: () => boolean,
  label: string,
  maxTurns = 50,
): Promise<void> {
  for (let turn = 0; turn < maxTurns; turn += 1) {
    if (predicate()) return;
    // 用执行器形式而不是 `Promise.withResolvers`：本工程的 `lib` 是 ES2023，没有那个成员。
    await new Promise<void>((resolve) => {
      setImmediate(resolve);
    });
  }
  throw new Error(`等待条件未成立：${label}`);
}

/**
 * 在测试期间装一个假地址栏：片段可读，`replaceState` 记账并真的清掉片段。
 *
 * 真实的清除行为只能证明在**地址栏**这一层：`location` 是平台能力，组合根读它、页面碰不到。
 */
function stubAddressBar(hash: string): { readonly replaced: string[]; restore(): void } {
  const replaced: string[] = [];
  const current = { hash, pathname: "/pair", search: "" };
  Object.defineProperty(globalThis, "location", {
    configurable: true,
    value: {
      get hash() {
        return current.hash;
      },
      get pathname() {
        return current.pathname;
      },
      get search() {
        return current.search;
      },
    },
  });
  Object.defineProperty(globalThis, "history", {
    configurable: true,
    value: {
      replaceState: (_data: unknown, _title: string, url: string) => {
        replaced.push(url);
        current.hash = "";
      },
    },
  });
  return {
    replaced,
    restore() {
      Reflect.deleteProperty(globalThis, "location");
      Reflect.deleteProperty(globalThis, "history");
    },
  };
}

describe("组合根：配对完成 → 写入已配对主机记录 → 触发连接（§7.3 / design D2/D3）", () => {
  it("已批准：记录含主机标识 / 主机公钥 / 规范化来源，且立刻连接（不等第二次入口）", async () => {
    const context = setup({ fetch: successfulPairingFetch() });
    await context.composition.start();
    // 未配对的起点：一条连接都不建。
    expect(context.sockets.sockets).toHaveLength(0);
    expect(context.composition.store.state.connection.state).toBe("unpaired");

    await context.composition.store.submitPairingQrText(pairingUrl());
    expect(context.composition.store.pairingPage().phase).toBe("claiming");

    // 轮询节奏由注入的假时钟驱动：推进一个窗口才有第一次状态查询（否则页面会空转打服务端）。
    context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
    await waitForEventLoopTurn(() => context.composition.store.pairingPage().phase === "pending_confirmation", "等待本机确认");

    context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
    await waitForEventLoopTurn(() => context.composition.store.pairingPage().phase === "approved", "已批准");

    // 2.2 三个字段：主机标识与主机公钥取自**状态响应**，规范化来源是缓存键的一部分
    // （取二维码里的那份、或只写 deviceId 都会让这条断言变红）。
    // 写入只落在「规范化来源派生出的那一个键」上：键取错（或另写了一份额外记录）→ 变红。
    expect([...context.cache.entries.keys()]).toEqual([`paired-host/${ORIGIN}`]);
    const entry = context.cache.entries.get(`paired-host/${ORIGIN}`);
    expect(entry?.kind === "local" ? entry.summary : null).toBe(
      JSON.stringify({ hostId: HOST_ID, publicKeyBase64Url: APPROVED_HOST_PUBLIC_KEY }),
    );
    expect(context.composition.store.hostPanel().host).toEqual({
      address: ORIGIN,
      hostId: HOST_ID,
      deviceId: DEVICE_ID,
    });

    // 2.5 已配对并触发连接：状态机离开 unpaired，出站地址指向刚配对的那台主机。
    // 连接机把「开始一次连接尝试」与「socket 打开」并成一步（`authenticating`），
    // 因此这里断言它不在未配对侧；停在等待扫码（`unpaired`）或没触发连接（0 条 socket）都会红。
    expect(context.composition.store.state.connection.state).toBe("authenticating");
    expect(context.sockets.sockets).toHaveLength(1);
    expect(context.sockets.urls[0]?.startsWith("wss://work-pc.example.ts.net")).toBe(true);
    expect(context.sockets.urls[0]?.endsWith("/sync/v1")).toBe(true);

    // 载荷用完即弃：这一页不再持有秘密（已配对结果里只有公开材料）。
    expect(JSON.stringify(context.composition.store.state)).not.toContain(PAIRING_SECRET);
    context.composition.dispose();
  });

  it("非法内容明确失败且不发任何请求（不尽力解析）", async () => {
    const context = setup({ fetch: successfulPairingFetch() });
    await context.composition.start();

    for (const [name, text] of [
      ["JSON 非法", "{"],
      ["协议版本不符", pairingUrl({ pairingProtocol: "acp-remote-pairing-v2" })],
      ["字段缺失", pairingUrl({ hostId: undefined })],
      ["字段长度不符", pairingUrl({ pairingSecret: "AQID" })],
      ["不是二维码内容", "把这一段贴进来"],
    ] as const) {
      await context.composition.store.submitPairingQrText(text);
      const page = context.composition.store.pairingPage();
      expect(page.phase, name).toBe("failed");
      expect(page.error?.code, name).toBe(
        name === "JSON 非法" ? "protocol.invalid_json" : "protocol.schema_invalid",
      );
      // 反例：尽力解析（补字段、放过版本、把任意文本当 JSON）会走到 claim → 这里会出现请求。
      expect(context.fetches, name).toHaveLength(0);
    }
    expect(context.sockets.sockets).toHaveLength(0);
    context.composition.dispose();
  });

  it("地址片段里的配对秘密：读取后立即清除，且不进缓存摘要、出站 URL 与可导出状态快照", async () => {
    const fragment = pairingUrl().replace(`${ORIGIN}/pair#`, "#");
    const addressBar = stubAddressBar(fragment);
    const context = setup({ fetch: successfulPairingFetch() });

    try {
      await context.composition.start();
      await context.composition.store.startPairingFromAddress();

      // 2.4 片段被清除：`replaceState` 收到的是「路径 + 查询」，片段（含 secret）已不在地址里。
      expect(addressBar.replaced).toEqual(["/pair"]);
      expect(globalThis.location.hash).toBe("");
      // 但它**先**被读到并进入了认领流程（清早了就读不到了）。
      expect(context.composition.store.pairingPage().phase).toBe("claiming");
      expect(context.fetches[0]?.url).toBe(`${ORIGIN}/sync/v1/pairing/claim`);

      context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
      await waitForEventLoopTurn(() => context.composition.store.pairingPage().phase === "pending_confirmation", "等待本机确认");
      context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
      await waitForEventLoopTurn(() => context.composition.store.pairingPage().phase === "approved", "已批准");

      // 秘密不进入两条**真实存在**的旁路：本地缓存摘要（IndexedDB 上是唯一的持久面）与
      // 可导出的状态快照。
      //
      // 「不进日志 / 不进 localStorage」不在这里断言：生产源码没有任何 `console.*` 调用，
      // 也从不使用 localStorage，那两条 `not.toContain` 只是对空集合断言——实现再怎么改
      // 都不会变红，没有判别力。将来真的引入日志器（或前端存储）时，再在那一处补断言。
      expect(JSON.stringify([...context.cache.entries])).not.toContain(PAIRING_SECRET);
      expect(JSON.stringify(context.composition.store.state)).not.toContain(PAIRING_SECRET);
      // 出站请求也不把秘密放进 URL（它只走 body 里的 HMAC proof）。
      expect(context.fetches.every((request) => !request.url.includes(PAIRING_SECRET))).toBe(true);
      // 已配对记录只有两项公开材料：主机标识与主机公钥。
      const record = context.cache.entries.get(`paired-host/${ORIGIN}`);
      expect(record?.kind === "local" ? record.summary : null).toBe(
        JSON.stringify({ hostId: HOST_ID, publicKeyBase64Url: APPROVED_HOST_PUBLIC_KEY }),
      );
    } finally {
      addressBar.restore();
      context.composition.dispose();
    }
  });
});

/**
 * 收口失败（组合根的 `pairingCompleted` 抛出）时的**页面**：配对确实完成了，只有这条同步连接没起来。
 *
 * 判别力集中在**同一屏上的两句话**：`approved` 阶段的引导语说「配对已完成，正在建立同步连接。」，
 * 原因句因此不能再是 claim / poll 那条「配对请求未能完成」。断言落在真实 `PairingPanel` 的渲染
 * 结果上，而不是只看模型字段——矛盾本来就是在页面上出现的。
 */
describe("组合根：配对成功但收口失败（连接没起来）的页面文案", () => {
  it("原因句承认配对已完成，且同屏不再出现「配对请求未能完成」", async () => {
    const PLATFORM_FAILURE = "PlatformStorageError: identity 读取失败";
    const respond = successfulPairingFetch();
    // 收口（`pairingCompleted` 读设备身份）在**已批准**响应之后才失败：翻转开关放在 `fetch` 里，
    // 而轮询自己的 `loadIdentity` 发生在 `fetch` **之前**，因此受影响的只可能是收口那一次读取。
    let closeoutReadFails = false;
    const identity = fakeDeviceIdentity(true);
    const context = setup({
      deviceIdentity: {
        ...identity,
        async loadIdentity(): Promise<DeviceIdentity | null> {
          if (closeoutReadFails) throw new Error(PLATFORM_FAILURE);
          return identity.loadIdentity();
        },
      },
      fetch: async (url, init): Promise<PairingHttpResponse> => {
        const response = await respond(url, init);
        const body: unknown = await response.json();
        // `status` 只按 `in` 收窄后做一次字符串比较（同 `pairing-model.ts` 收窄平台错误的写法）：
        // 这是替身自己的一次记账，因此不对响应正文做任何形状假设。
        if (typeof body === "object" && body !== null && "status" in body && body.status === "approved") {
          closeoutReadFails = true;
        }
        return response;
      },
    });

    await context.composition.store.submitPairingQrText(pairingUrl());
    context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
    await waitForEventLoopTurn(
      () => context.composition.store.pairingPage().phase === "pending_confirmation",
      "等待本机确认",
    );
    context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
    // 直接等「原因已落到页面」：收口失败本身就是这条用例的被测对象，等它比等中间的 `approved` 稳。
    await waitForEventLoopTurn(
      () => context.composition.store.pairingPage().error !== null,
      "收口失败的原因落到页面",
    );

    // 配对本身成功了：记录已落盘、连接机已推进到「已配对」——原因句必须承认这一点。
    expect(context.cache.entries.has(`paired-host/${ORIGIN}`)).toBe(true);
    const page = context.composition.store.pairingPage();
    expect(page.phase).toBe("approved");
    expect(context.composition.store.state.connection.state).toBe("disconnected");
    const notice = page.error?.message ?? "";
    expect(notice).toContain("配对已完成");
    expect(notice).not.toContain(PLATFORM_FAILURE);

    const markup = renderToStaticMarkup(
      createElement(PairingPanel, { model: page, onSubmit: () => {}, onRestart: () => {} }),
    );
    // 引导语就是 `approved` 的那一条，且与原因句同屏：两句说的都是「配对成了」。
    expect(page.guidance).toContain("配对已完成");
    expect(markup).toContain(page.guidance);
    expect(markup).toContain(notice);
    expect(markup).toContain('data-pairing-error="internal.unavailable"');
    // 反例：若收口路径继续复用通用文案，同一屏上会同时出现引导语「配对已完成…」与原因句
    // 「配对请求未能完成」两句互相否定的话 → 下面这两条变红。
    expect(markup).not.toContain("配对请求未能完成");
    expect(markup).not.toContain(PLATFORM_FAILURE);
    context.composition.dispose();
  });
});

describe("组合根：配对请求被拒绝时收口（不把状态留在「进行中」）", () => {
  it("claim 的 `fetch` 抛出（断网）：落到可重试的失败态，输入解禁，且能立刻重试成功", async () => {
    let claims = 0;
    const context = setup({
      fetch: async (_url, init): Promise<PairingHttpResponse> => {
        if (JSON.parse(init.body)["pairingRequestId"] === undefined) {
          claims += 1;
          // 第一次断网：`fetch` 直接拒绝（真实实现里没有第二层 catch，异常从 claim 冒泡上来）。
          if (claims === 1) throw new Error("Failed to fetch");
          return { ok: true, status: 201, json: () => Promise.resolve({ pairingRequestId: PAIRING_REQUEST_ID }) };
        }
        return { ok: true, status: 200, json: () => Promise.resolve({ status: "pending_confirmation" }) };
      },
    });
    await context.composition.start();

    await context.composition.store.submitPairingQrText(pairingUrl());

    const failed = context.composition.store.pairingPage();
    // 反例：若 `claim` 的拒绝直接冒泡（没有收口），阶段会停在 `claiming` → 断言变红。
    expect(failed.phase).toBe("failed");
    expect(failed.error?.code).toBe("internal.unavailable");
    expect(failed.error?.retryable).toBe(true);
    // 这两条就是 `PairingPanel` 的「输入框解禁 + 显示重新扫码入口」判据。
    expect(isPairingInFlight(failed)).toBe(false);
    expect(failed.canRestart).toBe(true);
    // 没有连接被建立，也没有任何已配对记录被写入。
    expect(context.sockets.sockets).toHaveLength(0);
    expect([...context.cache.entries.keys()]).toEqual([]);

    // 会话真的结束了：同一次页面会话里可以立刻重试，且这次走到了轮询。
    await context.composition.store.submitPairingQrText(pairingUrl());
    expect(context.composition.store.pairingPage().phase).toBe("claiming");
    context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
    await waitForEventLoopTurn(
      () => context.composition.store.pairingPage().phase === "pending_confirmation",
      "重试后进入等待确认",
    );
    context.composition.dispose();
  });
});

/**
 * 让组合根进入「主机身份变化」阻断态：旧口径的已配对记录只有主机标识、没有公钥。
 *
 * 起点与下面「记录缺主机公钥」一组用例一致——`start()` 照常建立连接，服务端下发 challenge 后
 * 验签失败，连接层阻断（`identity_changed`）并停止自动重连。重新配对是从这里出来的唯一路径
 * （design D2），因此这两条用例都从同一个起点出发。
 */
async function startBlockedOnMissingHostKey(context: Harness): Promise<void> {
  await context.cache.putLocalSummary({
    key: `paired-host/${ORIGIN}`,
    summary: JSON.stringify({ hostId: HOST_ID }),
    nowMs: 0,
  });
  await context.composition.start();
  const socket = context.sockets.latest;
  socket.open();
  await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
  socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
  await waitForEventLoopTurn(
    () => context.composition.store.state.connection.state === "identity_changed",
    "进入身份变化阻断态",
  );
}

describe("组合根：从阻断态重新配对后真的建立连接（design D2 的恢复路径）", () => {
  it("同一台主机重新配对成功：解除门面阻断并新建连接（socket 1→2、离开阻断态）", async () => {
    const context = setup({ realHostIdentity: true, fetch: successfulPairingFetch() });
    await startBlockedOnMissingHostKey(context);
    expect(context.composition.store.blocked).toBe(true);

    // 起点：阻断态不自动重连，推进整个退避窗口也不会有第二条连接。
    context.clock.advanceBy(RECONNECT_MAX_WINDOW_MS);
    expect(context.sockets.sockets).toHaveLength(1);

    await context.composition.store.submitPairingQrText(pairingUrl());
    context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
    await waitForEventLoopTurn(() => context.composition.store.pairingPage().phase === "pending_confirmation", "等待本机确认");
    context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
    await waitForEventLoopTurn(() => context.composition.store.pairingPage().phase === "approved", "已批准");

    // 反例：若 `pairingCompleted` 不解除门面的阻断（`SyncClient.connect()` 抛错后又被吞掉），
    // 这里仍是 1 条 socket、状态仍停在 `identity_changed` → 两条断言都变红。
    expect(context.sockets.sockets).toHaveLength(2);
    expect(context.composition.store.state.connection.state).toBe("authenticating");
    expect(context.composition.store.blocked).toBe(false);
    context.composition.dispose();
  });

  it("换了一台主机重新配对：按新记录重建门面，`client_hello` 自报新的 hostId", async () => {
    const NEW_HOST_ID = "3b1e6b1d-3c0a-4a6f-9f4e-2c1a5b7d8e90";
    let statusPolls = 0;
    const context = setup({
      realHostIdentity: true,
      fetch: async (_url, init): Promise<PairingHttpResponse> => {
        if (JSON.parse(init.body)["pairingRequestId"] === undefined) {
          return { ok: true, status: 201, json: () => Promise.resolve({ pairingRequestId: PAIRING_REQUEST_ID }) };
        }
        statusPolls += 1;
        const body =
          statusPolls === 1
            ? { status: "pending_confirmation" }
            : {
                status: "approved",
                device: { deviceId: DEVICE_ID },
                host: { hostId: NEW_HOST_ID, hostPublicKey: APPROVED_HOST_PUBLIC_KEY },
              };
        return { ok: true, status: 200, json: () => Promise.resolve(body) };
      },
    });
    await startBlockedOnMissingHostKey(context);

    await context.composition.store.submitPairingQrText(pairingUrl({ hostId: NEW_HOST_ID }));
    context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
    await waitForEventLoopTurn(() => context.composition.store.pairingPage().phase === "pending_confirmation", "等待本机确认");
    context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
    await waitForEventLoopTurn(() => context.composition.store.pairingPage().phase === "approved", "已批准");

    expect(context.sockets.sockets).toHaveLength(2);
    const rebuilt = context.sockets.latest;
    rebuilt.open();
    await waitUntil(() => rebuilt.types().includes("auth.client_hello"), "新门面发出 clientHello");
    const helloBody = rebuilt.last("auth.client_hello")?.["body"];
    const sentHostId =
      typeof helloBody === "object" && helloBody !== null && "hostId" in helloBody ? helloBody.hostId : null;

    // 反例：若 `adoptPairedHost` 仍在 `sync !== null` 时直接返回（复用旧实例），这条 hello 会
    // 自报**旧** hostId → 断言变红（服务端随后会以 `auth.host_mismatch` 把它打回阻断态，
    // 同一次页面会话里再也出不去）。
    expect(sentHostId).toBe(NEW_HOST_ID);
    expect(context.composition.store.state.connection.state).toBe("authenticating");
    expect(context.composition.store.blocked).toBe(false);
    context.composition.dispose();
  });
});

describe("组合根：已配对记录缺主机公钥不可用（旧口径的反例）", () => {
  it("记录只有主机标识时的握手：阻断为身份变化，且绝不发出设备证明", async () => {
    // 旧实现只写设备/主机标识：摘要里没有主机公钥——`PairedHostIdentity` 因此解不出验签用的 key。
    const context = setup({ realHostIdentity: true });
    await context.cache.putLocalSummary({
      key: `paired-host/${ORIGIN}`,
      summary: JSON.stringify({ hostId: HOST_ID }),
      nowMs: 0,
    });
    await context.composition.start();

    const socket = context.sockets.latest;
    socket.open();
    await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
    socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));

    // 反例一：若实现「记录缺公钥即按从未配对处理」，状态会停在 `unpaired` 且一条 socket 都不建。
    // 反例二：若实现跳过主机验签，这里会走完握手（`auth.client_proof`）并进入已认证。
    await waitForEventLoopTurn(
      () => context.composition.store.state.connection.state === "identity_changed",
      "进入身份变化阻断态",
    );
    expect(context.composition.store.state.connection.blocking?.cause).toBe("host_identity_changed");
    expect(socket.types()).not.toContain("auth.client_proof");

    // 阻断态不自动重连：推进整个退避窗口也不会有第二条连接。
    context.clock.advanceBy(RECONNECT_MAX_WINDOW_MS);
    expect(context.sockets.sockets).toHaveLength(1);
    context.composition.dispose();
  });
});

/** 让 `FakeSocketFactory` 的 `open` 与本文件用到的类型保持一致（`SocketPort` 的最小面）。 */
