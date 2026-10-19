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
import { describe, expect, it, vi } from "vitest";

import { PairingPanel } from "./components";
import { createComposition } from "./composition";
import type { Composition, CompositionPorts, FetchLike, PairingHttpResponse } from "./composition";
import { PAIRING_POLL_INTERVAL_MS } from "./features/pairing-model";
import type { PairingLookupResponse } from "./protocol";
import { decodeBase64Url, encodeBase64Url } from "./sync-client";
import { encodeTranscript, encodeU16be, encodeUtf8, encodeUuid16 } from "./platform/transcript";
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
  DownloadPort,
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
 * 二维码里那把主机公钥：65 字节 SEC1 未压缩点的 base64url（87 字符）。
 *
 * §7.1 要求 PWA 校验字段长度，因此夹具的形状必须真（截短的字符串会被判成结构错误）。
 * 同时它必须真的能验签——认领响应里的 `hostProof` 由下面 `HOST_PRIVATE_JWK` 对应的私钥签出，
 * 客户端只用**这把**公钥验证（design D2 的 L1）。换成别的公钥，验签就会失败。
 */
const QR_HOST_PUBLIC_KEY =
  "BIClOUrR8wOLSGIX7G7E1Xnepu8XTVE43CTCgGfuX4lN7NK-YeWPyyGT33ANPqAyHHCAaM7minO-H0aLYytpk74";

/**
 * 与 {@link QR_HOST_PUBLIC_KEY} 配对的固定 P-256 私钥（测试向量，只用于签出被验的那一支证明）。
 *
 * 硬编码而不是现场生成：二维码载荷与断言都在模块加载期就需要那把公钥，现场生成会把整份夹具
 * 变成异步初始化。
 */
const HOST_PRIVATE_JWK: JsonWebKey = {
  key_ops: ["sign"],
  ext: true,
  kty: "EC",
  x: "gKU5StHzA4tIYhfsbsTVed6m7xdNUTjcJMKAZ-5fiU0",
  y: "7NK-YeWPyyGT33ANPqAyHHCAaM7minO-H0aLYytpk74",
  crv: "P-256",
  d: "IVpAYQYFb_p-d93BzOWR4J4QdwPfBl_iydLKlOOsKwE",
};

/** 认领响应里的服务端随机数（32 字节 base64url，§7.2）。 */
const CLAIM_SERVER_NONCE = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

/**
 * 按 `pairing-host-proof` 域（§6.3，字段 tag `1,2,3,4,6,7,8,9,10,13`）现编一支主机证明。
 *
 * 字段值全部取自认领请求本身（`hostId`/`deviceId`/`pairingId`/`canonicalOrigin`/
 * `devicePublicKey`/`clientNonce`），再加二维码里的主机公钥、本次的 `serverNonce` 与
 * `pairingRequestId`——与客户端验签时重建的 transcript 逐字段对应。这里改错任一字段、
 * 或换一把私钥，客户端的验签都会失败。
 */
async function signClaimHostProof(requestBody: Record<string, unknown>): Promise<string> {
  const key = await crypto.subtle.importKey(
    "jwk",
    HOST_PRIVATE_JWK,
    { name: "ECDSA", namedCurve: "P-256" },
    false,
    ["sign"],
  );
  const transcript = encodeTranscript("acp-remote/pairing-host-proof/v1", [
    { tag: 1, bytes: encodeU16be(1) },
    { tag: 2, bytes: encodeUuid16(String(requestBody["hostId"])) },
    { tag: 3, bytes: encodeUuid16(String(requestBody["deviceId"])) },
    { tag: 4, bytes: encodeUuid16(String(requestBody["pairingId"])) },
    { tag: 6, bytes: encodeUtf8(String(requestBody["canonicalOrigin"])) },
    { tag: 7, bytes: decodeBase64Url(QR_HOST_PUBLIC_KEY) },
    { tag: 8, bytes: decodeBase64Url(String(requestBody["devicePublicKey"])) },
    { tag: 9, bytes: decodeBase64Url(String(requestBody["clientNonce"])) },
    { tag: 10, bytes: decodeBase64Url(CLAIM_SERVER_NONCE) },
    { tag: 13, bytes: encodeUuid16(PAIRING_REQUEST_ID) },
  ]);
  const signature = await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, key, transcript);
  return encodeBase64Url(new Uint8Array(signature));
}

/** 一次**合法**的认领响应（§7.2）：`pairingRequestId` + `serverNonce` + 能被二维码公钥验证的主机证明。 */
async function claimResponseFor(init: { readonly body: string }): Promise<PairingHttpResponse> {
  const requestBody = JSON.parse(init.body) as Record<string, unknown>;
  return {
    ok: true,
    status: 201,
    json: async () =>
      Promise.resolve({
        pairingRequestId: PAIRING_REQUEST_ID,
        serverNonce: CLAIM_SERVER_NONCE,
        hostProof: await signClaimHostProof(requestBody),
      }),
  };
}

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

/** 组合根装配用的下载端口替身：只证明接线存在，不复制平台实现。 */
function fakeDownloadPort(): DownloadPort {
  return {
    blobLimitBytes: 64 * 1024 * 1024,
    async open() {
      return {
        mode: "blob" as const,
        async write() {},
        async finish() {
          return { kind: "blob" as const, url: null, bytesWritten: 0 };
        },
        async cancel() {},
      };
    },
    revoke() {},
  };
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
  // 默认替身：claim 成功给一条**能被二维码公钥验签通过**的认领响应（§7.2：`pairingRequestId`
  // + `serverNonce` + `hostProof`）。注入的替身同样经下面那一层记账，因此断言「发了什么」与
  // 「一个都没发」都落在 `fetches` 上（不因替换而失去证据）。
  const respond: FetchLike =
    input.fetch ??
    ((_url, init) => claimResponseFor(init));
  const composition = createComposition({
    platform: {
      deviceIdentity: input.deviceIdentity ?? fakeDeviceIdentity(true),
      localCache: cache,
      importedContent: createVolatileImportedContent(),
      lifecycle,
      download: fakeDownloadPort(),
    },
    openSocket: sockets.open,
    // 端口经 `ports` 接缝注入：退避定时与快照 digest 因此不需要真实时间与真实 SHA-256，
    // 而被测的仍然是组合根真实的装配代码（不是另写一份装配）。
    ports,
    fetch: async (url, init) => {
      fetches.push({ url, method: init.method, body: init.body });
      return respond(url, init);
    },
    // 环境事实（design D2）：配对入口据此校验安全上下文与来源一致。夹具二维码的
    // `canonicalOrigin` 是 `ORIGIN`，因此替身默认「安全上下文 + 来源等于 ORIGIN」，
    // 让这些用例判的是配对链路的接线而不是环境前置条件（后者由 pairing-model.test.ts 覆盖）。
    secureContext: () => true,
    origin: () => ORIGIN,
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
          workspace: { directoryId: "0123456789abcdef0123456789abcdef" },
        },
      ],
    }),
    makeSnapshotChunk({ snapshotId, chunkIndex: "1", resource: "workspaces", items: [{ directoryId: "0123456789abcdef0123456789abcdef", path: "D:\\work\\api" }] }),
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
 * 认证结果里**没有**任何授权集合（设备权限层已删除），因此这里不需要为「放行其余闸门」准备
 * 任何 scope——创建会话只剩连接与载荷两道判定，缺 `connectionId` 就是唯一可能的拒绝理由。
 */
async function authenticate(socket: FakeSocket): Promise<void> {
  socket.open();
  await waitUntil(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
  socket.deliver(makeChallenge({ connectionId: CONNECTION_ID }));
  await waitUntil(() => socket.types().includes("auth.client_proof"), "发出 clientProof");
  socket.deliver(makeAuthenticated({ connectionId: CONNECTION_ID }));
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

    // 认证之后：闸门全过（Agent 在快照目录里、载荷恰好一个目录引用）。设备不再有创建权限维度，
    // 因此这里不再需要任何 scope。这条同时证明 `connectionId` 真的取到了值——否则这里会是 `ok: false`。
    const allowed = context.composition.store.submitCreateSession({ directoryId: "0123456789abcdef0123456789abcdef", agentId: "codex-a" });
    expect(allowed.ok).toBe(true);
    expect(socket.last("command")?.["body"]).toMatchObject({ command: "session.create" });

    // 断连之后必须回到「不可提交」：认证态失效时不得沿用旧标识。
    socket.serverClose(4500, "service_unavailable");
    await waitUntil(() => context.composition.store.state.connection.state === "reconnecting", "进入重连中");
    // 反例：若 `ConnectionIdentity` 编造一个固定值，这里仍会是 `ok: true` → 断言变红。
    expect(context.composition.store.submitCreateSession({ directoryId: "0123456789abcdef0123456789abcdef", agentId: "codex-a" }).ok).toBe(false);
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

describe("组合根：配对 HTTPS 面按 §7.2/§7.3/§7.5 的路径与形状发请求", () => {
  /** 与配对码 lookup 成功响应同形的绑定材料（claim/poll 的输入）。 */
  const QR: PairingLookupResponse = {
    protocolVersion: 1,
    hostId: HOST_ID,
    hostPublicKey: QR_HOST_PUBLIC_KEY,
    canonicalOrigin: ORIGIN,
    pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
    pairingSecret: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
    expiresAt: new Date(Date.now() + 600_000).toISOString(),
  };

  it("设备（sync）载荷只分类、不解析：浏览器不会从 data= 得到配对材料", () => {
    const context = setup();
    const payload = encodeBase64Url(
      new TextEncoder().encode(JSON.stringify({ pairingProtocol: "acp-remote-pairing-v1" })),
    );
    // 三种容器都只标注类型：没有 `canonicalOrigin`、没有秘密、没有 claim 可用的字段。
    expect(context.composition.pairing.readQrPayload(`${ORIGIN}/pair#data=${payload}`)).toEqual({
      kind: "sync",
    });
    expect(context.composition.pairing.readQrPayload(`data=${payload}`)).toEqual({ kind: "sync" });
    expect(context.composition.pairing.readQrPayload(payload)).toEqual({ kind: "sync" });
    // 字段缺失、长度不符、已过期都不影响分类：浏览器不会解析设备资料，因此也没有字段校验。
    expect(
      context.composition.pairing.readQrPayload(
        JSON.stringify({
          pairingProtocol: "acp-remote-pairing-v1",
          hostId: undefined,
          hostPublicKey: "AQID",
          expiresAt: new Date(0).toISOString(),
        }),
      ),
    ).toEqual({ kind: "sync" });
    // 容器与协议常量仍严格：非法 JSON、未知协议、任意文本都明确失败。
    expect(() => context.composition.pairing.readQrPayload("{")).toThrow(/JSON/);
    expect(() =>
      context.composition.pairing.readQrPayload(JSON.stringify({ pairingProtocol: "other" })),
    ).toThrow(/协议/);
    expect(() => context.composition.pairing.readQrPayload("把这一段贴进来")).toThrow(
      /不是配对二维码内容/,
    );
    context.composition.dispose();
  });

  it("node-link 载荷按同一口径读出并标注类型；字段校验不放宽（design D3）", () => {
    const context = setup();
    const nodeLink = {
      pairingProtocol: "acp-remote-nodelink-v1",
      ownerNodeId: HOST_ID,
      ownerPublicKey: QR_HOST_PUBLIC_KEY,
      endpoint: "wss://work-pc.example.ts.net/node-link/v1",
      pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
      pairingSecret: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
      expiresAt: new Date(Date.now() + 600_000).toISOString(),
    };
    const payload = encodeBase64Url(new TextEncoder().encode(JSON.stringify(nodeLink)));

    // 三种容器读出的都是同一份 node-link 载荷，且类型被标注出来——**不**在这里拒收：
    // 「哪一类入口接受哪一类载荷」是入口的判定（design D2/D3）。
    expect(context.composition.pairing.readQrPayload(`${ORIGIN}/node-link/pair#data=${payload}`)).toEqual({
      kind: "node-link",
      qr: nodeLink,
    });
    expect(context.composition.pairing.readQrPayload(`data=${payload}`)).toEqual({ kind: "node-link", qr: nodeLink });
    expect(context.composition.pairing.readQrPayload(payload)).toEqual({ kind: "node-link", qr: nodeLink });

    // node-link 自己的失败形态：端点形状、两条 base64url 字段的长度、过期时间逐项照旧。
    for (const [name, payloadWith] of [
      ["端点形状非法", { ...nodeLink, endpoint: "wss://work-pc.example.ts.net/other" }],
      ["端点不是 wss", { ...nodeLink, endpoint: "https://work-pc.example.ts.net/node-link/v1" }],
      ["缺字段", { ...nodeLink, ownerNodeId: undefined }],
      ["公钥长度不符", { ...nodeLink, ownerPublicKey: "AQID" }],
      ["秘密长度不符", { ...nodeLink, pairingSecret: "AQID" }],
      ["已过期", { ...nodeLink, expiresAt: new Date(0).toISOString() }],
    ] as const) {
      expect(
        () => context.composition.pairing.readQrPayload(JSON.stringify(payloadWith)),
        name,
      ).toThrow(/二维码/);
    }
    // 协议常量不在登记表内（既不是 sync 也不是 node-link）：仍然明确失败。
    expect(() =>
      context.composition.pairing.readQrPayload(JSON.stringify({ ...nodeLink, pairingProtocol: "acp-remote-other-v1" })),
    ).toThrow(/协议/);
    context.composition.dispose();
  });

  it("lookup 发往 /sync/v1/pairing/lookup：body 只带配对码，成功返回配对绑定", async () => {
    const context = setup({
      fetch: async (_url, init): Promise<PairingHttpResponse> => {
        // 该替身只应答 lookup；走到 claim 说明调用方没有在 lookup 处停下。
        if (init.body.includes("pairingCode")) return lookupResponseFor();
        return claimResponseFor(init);
      },
    });

    const result = await context.composition.pairing.lookup({
      canonicalOrigin: ORIGIN,
      pairingCode: "ABCD-EFGH",
    });

    expect(result).toEqual({
      kind: "ok",
      qr: {
        protocolVersion: 1,
        pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
        hostId: HOST_ID,
        hostPublicKey: QR_HOST_PUBLIC_KEY,
        canonicalOrigin: ORIGIN,
        pairingSecret: PAIRING_SECRET,
        expiresAt: expect.any(String) as string,
      },
    });
    expect(context.fetches).toHaveLength(1);
    expect(context.fetches[0]?.url).toBe(`${ORIGIN}/sync/v1/pairing/lookup`);
    expect(context.fetches[0]?.method).toBe("POST");
    expect(JSON.parse(context.fetches[0]?.body ?? "{}")).toEqual({
      protocolVersion: 1,
      pairingCode: "ABCD-EFGH",
    });
    // 码只走 body，不进 URL。
    expect(context.fetches[0]?.url).not.toContain("ABCD-EFGH");
    context.composition.dispose();
  });

  it("lookup 的失败形态统一为 error：HTTP 非 200、正文形状/长度不符、来源不同", async () => {
    const cases: readonly { readonly name: string; readonly respond: () => PairingHttpResponse }[] = [
      {
        name: "HTTP 401（码无效）",
        respond: () => ({
          ok: false,
          status: 401,
          json: () => Promise.resolve({ code: "pairing.code_invalid", message: "invalid" }),
        }),
      },
      {
        name: "正文不是对象",
        respond: () => ({ ok: true, status: 200, json: () => Promise.resolve("oops") }),
      },
      {
        name: "缺 pairingSecret",
        respond: () => lookupResponseFor({ pairingSecret: undefined }),
      },
      {
        // UUID 形状（§7.1 `$defs.uuid`）与 schema 同口径：非空但形状不符同样判失败。
        name: "pairingId 不是 UUID",
        respond: () => lookupResponseFor({ pairingId: "pairing-1" }),
      },
      {
        name: "hostId 不是 UUID",
        respond: () => lookupResponseFor({ hostId: "host" }),
      },
      {
        name: "pairingId 是大写 UUID（schema 只允许小写）",
        respond: () =>
          lookupResponseFor({ pairingId: "2BC8B944-2A4F-46A7-8C31-B2C40923F60A" }),
      },
      {
        name: "秘密长度不符",
        respond: () => lookupResponseFor({ pairingSecret: "AQID" }),
      },
      {
        name: "公钥长度不符",
        respond: () => lookupResponseFor({ hostPublicKey: "AQID" }),
      },
      {
        name: "协议版本不符",
        respond: () => lookupResponseFor({ protocolVersion: 2 }),
      },
      {
        name: "过期时间不是时间戳",
        respond: () => lookupResponseFor({ expiresAt: "不是时间" }),
      },
      {
        name: "来源与请求来源不同",
        respond: () => lookupResponseFor({ canonicalOrigin: "https://other.example.ts.net" }),
      },
      {
        name: "JSON 读取抛出",
        respond: () => ({
          ok: true,
          status: 200,
          json: () => Promise.reject(new Error("bad json")),
        }),
      },
    ];

    for (const scenario of cases) {
      const context = setup({ fetch: async () => scenario.respond() });
      const result = await context.composition.pairing.lookup({
        canonicalOrigin: ORIGIN,
        pairingCode: "ABCD-EFGH",
      });
      // 反例：任一失败被当成成功，页面会拿半份材料进入 claim → 断言变红。
      expect(result, scenario.name).toEqual({ kind: "error" });
      context.composition.dispose();
    }
  });

  it("claim 发往 /sync/v1/pairing/claim，body 含 pairingId/deviceId/clientNonce/proof", async () => {
    const context = setup();
    const error = await context.composition.pairing.claim({
      qr: QR,
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
      qr: QR,
      canonicalOrigin: ORIGIN,
    });

    // 反例：若不映射状态而直接透传未知码，R18/配对页的文案分支就落空 → 断言变红。
    expect(error?.code).toBe("pairing.expired");
    expect(error?.message).toBe("已过期");
    context.composition.dispose();
  });

  it("认领响应的主机证明验不过 / 缺材料：明确失败、不轮询、不落盘（design D2 的 L1）", async () => {
    // 三条拒绝路径各自一条：签名不匹配（被换了一台电脑）与两种缺材料（协议形状就不完整）。
    const variants: readonly {
      readonly name: string;
      readonly respond: () => PairingHttpResponse;
      readonly code: string;
    }[] = [
      {
        name: "签名不匹配",
        // 有效长度（64 字节）但由别的密钥签出来：任何公钥都验不过。
        respond: () => ({
          ok: true,
          status: 201,
          json: () =>
            Promise.resolve({
              pairingRequestId: PAIRING_REQUEST_ID,
              serverNonce: CLAIM_SERVER_NONCE,
              hostProof: encodeBase64Url(new Uint8Array(64).fill(0x5a)),
            }),
        }),
        code: "auth.proof_invalid",
      },
      {
        name: "缺 hostProof",
        respond: () => ({
          ok: true,
          status: 201,
          json: () => Promise.resolve({ pairingRequestId: PAIRING_REQUEST_ID, serverNonce: CLAIM_SERVER_NONCE }),
        }),
        code: "protocol.schema_invalid",
      },
      {
        name: "缺 serverNonce",
        respond: () => ({
          ok: true,
          status: 201,
          json: () => Promise.resolve({ pairingRequestId: PAIRING_REQUEST_ID, hostProof: encodeBase64Url(new Uint8Array(64)) }),
        }),
        code: "protocol.schema_invalid",
      },
    ];

    for (const variant of variants) {
      let statusPolls = 0;
      const context = setup({
        fetch: async (url, init): Promise<PairingHttpResponse> => {
          if (url.endsWith("/pairing/lookup")) return lookupResponseFor();
          if (JSON.parse(init.body)["pairingRequestId"] === undefined) return variant.respond();
          statusPolls += 1;
          return {
            ok: true,
            status: 200,
            json: () =>
              Promise.resolve({
                status: "approved",
                device: { deviceId: DEVICE_ID },
                host: { hostId: HOST_ID, hostPublicKey: APPROVED_HOST_PUBLIC_KEY },
              }),
          };
        },
      });
      await context.composition.start();
      await enterPairingByCode(context);

      const page = context.composition.store.pairingPage();
      // 反例：若 claim 跳过验签（或拿响应自带材料「自证」），这里会走到 pending_confirmation/approved。
      expect(page.phase, variant.name).toBe("failed");
      expect(page.error?.code, variant.name).toBe(variant.code);
      // 反例：验签失败后若仍排下一次轮询，这里会 > 0。
      expect(statusPolls, variant.name).toBe(0);
      // 反例：若失败后仍写已配对记录，这里会出现 `paired-host/...`；连接也不该被带起来。
      expect([...context.cache.entries.keys()], variant.name).toEqual([]);
      expect(context.sockets.sockets, variant.name).toHaveLength(0);
      context.composition.dispose();
    }
  });

  it("状态轮询：pending/approved/终态/未知各自分派（业务状态只由 body 表达）", async () => {
    let polls = 0;
    const recorded: { url: string; method: string; body: string }[] = [];
    const context = setup({
      // claim 与 status 是两个端点（§7.2/§7.3）：claim 给 `pairingRequestId` + `serverNonce` +
      // 可验签的 `hostProof`，status 给业务状态。
      fetch: async (_url, init): Promise<PairingHttpResponse> => {
        recorded.push({ url: _url, method: init.method, body: init.body });
        if (JSON.parse(init.body)["pairingRequestId"] === undefined) {
          return claimResponseFor(init);
        }
        polls += 1;
        return { ok: true, status: 200, json: () => Promise.resolve(bodyFor(polls)) };
      },
    });
    const qr = QR;
    await context.composition.pairing.claim({ qr, canonicalOrigin: ORIGIN });

    // 未声明过配对时轮询明确失败，而不是发一个缺 `pairingRequestId` 的请求。
    const fresh = setup();
    const freshQr = QR;
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

  it("HTTP 200 的 expired 正文：transport 只合成 pairing.failed，页面固定呈现 S6", async () => {
    // §7.3：`expired` 也是 HTTP 200，`toPublicError` 因此只能按状态推导出 `pairing.failed`。
    // 这条用例把**真实** `HttpPairingTransport.poll` 的结果接到真实状态机上：若模型照
    // `error.code` 分类，配对码入口会落到「配对请求未能完成」（S12）而不是 S6。
    const SERVER_MESSAGE = "这次配对已过期。";
    const expiredStatus = (): PairingHttpResponse => ({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ status: "expired", message: SERVER_MESSAGE }),
    });

    // 传输层：`status: "expired"` 与 `error.code: "pairing.failed"` 同时成立，这正是模型不能照码分类的原因。
    {
      const context = setup({
        fetch: async (_url, init): Promise<PairingHttpResponse> => {
          if (JSON.parse(init.body)["pairingRequestId"] === undefined) return claimResponseFor(init);
          return expiredStatus();
        },
      });
      await context.composition.pairing.claim({ qr: QR, canonicalOrigin: ORIGIN });
      const raw = await context.composition.pairing.poll({
        qr: QR,
        requestNonce: "AAECAwQFBgcICQoLDA0ODw",
      });
      expect(raw).toMatchObject({ kind: "terminal", status: "expired" });
      if (raw.kind !== "terminal") throw new Error("状态轮询应当返回终态");
      expect(raw.error.code).toBe("pairing.failed");
      context.composition.dispose();
    }

    // 配对码入口 → S6「配对码不对或已过期，请重新生成配对码」。
    const context = setup({
      fetch: async (url, init): Promise<PairingHttpResponse> => {
        if (url.endsWith("/pairing/lookup")) return lookupResponseFor();
        if (JSON.parse(init.body)["pairingRequestId"] === undefined) return claimResponseFor(init);
        return expiredStatus();
      },
    });
    await context.composition.start();

    await enterPairingByCode(context);
    await advanceUntil(
      context,
      () => context.composition.store.pairingPage().phase === "failed",
      "服务端过期（配对码入口）",
    );

    const page = context.composition.store.pairingPage();
    expect(page.failure).toBe("code_invalid");
    expect(page.error?.code).toBe("pairing.code_invalid");
    expect(page.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    // 配对码入口只呈现固定文案（design D6）：服务端的原因文本不进入页面。
    expect(page.presentation.lede).not.toContain(SERVER_MESSAGE);
    expect(context.sockets.sockets).toHaveLength(0);
    context.composition.dispose();
  });

  it("已批准缺主机材料 / 标识不符 / 公钥与载荷不一致：可区分的失败，而不是「已批准」", async () => {
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
      // 状态响应没有主机签名，因此公钥必须与二维码里那把**逐字节相同**；这里换一把等长的别的
      // 公钥（长度、字母表都合法）→ 必须明确失败且不落盘（design D2 的 L3）。
      {
        name: "主机公钥与载荷不一致",
        body: approvedWith({ hostId: HOST_ID, hostPublicKey: MISMATCHED_HOST_PUBLIC_KEY }),
      },
      { name: "缺设备元数据", body: { status: "approved", host: { hostId: HOST_ID, hostPublicKey: QR_HOST_PUBLIC_KEY } } },
    ];

    for (const scenario of cases) {
      let statusPolls = 0;
      const context = setup({
        fetch: async (_url, init): Promise<PairingHttpResponse> => {
          if (JSON.parse(init.body)["pairingRequestId"] === undefined) {
            return claimResponseFor(init);
          }
          statusPolls += 1;
          return { ok: true, status: 200, json: () => Promise.resolve(scenario.body) };
        },
      });
      const qr = QR;
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
 * 状态响应里的主机公钥。
 *
 * **必须与二维码里的那把逐字节相同**：状态响应没有主机签名，因此此后每次连接验签的信任根
 * 只能靠这条比对来确立（design D2 的 L3）。因此它就是 {@link QR_HOST_PUBLIC_KEY}——两处
 * 各自写一份字面量会让「逐字节相同」这条要求看起来像巧合。
 */
const APPROVED_HOST_PUBLIC_KEY = QR_HOST_PUBLIC_KEY;

/**
 * 一把**等长但不同**的公钥：用途只有一个——`approved` 响应里拿它冒充二维码里的那把。
 *
 * 长度与字母表都合法（65 字节 base64url），因此它会一路通过形状校验，只被**逐字节比对**拦下。
 * 这正是「不一致路径」要证的：比对一旦放水（trim、大小写折叠、前缀匹配、干脆不比），
 * 这个夹具就会落盘成一条不属于二维码的主机记录。
 */
const MISMATCHED_HOST_PUBLIC_KEY =
  "BAYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgY";

/** 一次成功配对的 HTTPS 面：lookup 给合法绑定，claim 给合法的认领响应，前两次 status 待确认/已批准。 */
function successfulPairingFetch(): FetchLike {
  let statusPolls = 0;
  return async (url, init): Promise<PairingHttpResponse> => {
    if (url.endsWith("/pairing/lookup")) return lookupResponseFor();
    if (JSON.parse(init.body)["pairingRequestId"] === undefined) {
      return claimResponseFor(init);
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

/**
 * 新设备短二维码的完整地址（共享向量 `fixtures/sync/v1/device-qr/vectors.json` 的 canonical）。
 *
 * 测试只断言浏览器**清除**它并显示 App 指引；绝不解码、不取回。
 */
const DEVICE_QR_URL =
  "https://faraday.tail74842e.ts.net:8765/pair#q2=AgABAgMEBQYHCAkKCwwNDg8QERITFBUWFxgZGhscHR4fUSpZeYIpHWiVwqrdwRuVFov6KCpsWZfofCn__9Gcw3w";

/** 旧设备完整资料地址（本变更后浏览器只清除并提示已停用）。 */
function legacyDevicePairingUrl(overrides: Record<string, unknown> = {}): string {
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

/** 一次**合法**的 lookup 响应（§7.5）：与二维码载荷同形。 */
function lookupResponseFor(overrides: Record<string, unknown> = {}): PairingHttpResponse {
  return {
    ok: true,
    status: 200,
    json: () =>
      Promise.resolve({
        protocolVersion: 1,
        pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
        hostId: HOST_ID,
        hostPublicKey: QR_HOST_PUBLIC_KEY,
        canonicalOrigin: ORIGIN,
        pairingSecret: PAIRING_SECRET,
        expiresAt: new Date(Date.now() + 600_000).toISOString(),
        ...overrides,
      }),
  };
}

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
 * 推进假时钟 + 让出事件循环，直到条件成立。
 *
 * 为什么不能写成「先 `advanceBy` 再等条件」：轮询计时器只有在**认领那条链跑完之后**才会被装上，
 * 而那条链里有 WebCrypto（HMAC proof）与 fetch 的宏任务。于是「先推进」有机会落在计时器装上之前，
 * 那次推进被白白吃掉，条件就永不成立——CI 上正是这样挂了三条用例（本地快机每次都赢，所以看不见）。
 * 这里每一轮都「先让出一次事件循环、再推进一个窗口」，因此既不会漏掉尚未装上的计时器，
 * 也不引入任何真实时间依赖（`setImmediate` 表达的是「下一轮」，不是「等一段时长」）。
 * 轮数上限只把「条件永不成立」变成一条明确的失败。
 */
async function advanceUntil(
  context: { readonly clock: FakeClock },
  predicate: () => boolean,
  label: string,
  maxTurns = 2000,
): Promise<void> {
  for (let turn = 0; turn < maxTurns; turn += 1) {
    if (predicate()) return;
    await new Promise<void>((resolve) => {
      setImmediate(resolve);
    });
    context.clock.advanceBy(PAIRING_POLL_INTERVAL_MS);
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

/**
 * 打开一条配对地址：把它的 `#…` 挂到假地址栏上，再走**真实组合根**的片段读取与清除。
 *
 * 浏览器只在同步入口处理设备 `q2=` / `data=` 两类片段；其余片段不进入配对流程。
 */
async function openPairingAddress(
  context: Harness,
  text: string,
  kind: "sync" | "node-link" = "sync",
): Promise<{ readonly replaced: string[] }> {
  const fragment = text.slice(text.indexOf("#") + 1);
  const addressBar = stubAddressBar(`#${fragment}`);
  try {
    await context.composition.store.startPairingFromAddress(kind);
    return { replaced: addressBar.replaced };
  } finally {
    addressBar.restore();
  }
}

/**
 * 用真实组合根完成配对码入口：裸 `/pair` → 提交 8 位码 → lookup → claim/轮询。
 *
 * 这是浏览器**唯一**会发出配对请求的入口（design D6）。
 */
async function enterPairingByCode(context: Harness, code = "ABCD-EFGH"): Promise<void> {
  await context.composition.store.startPairingFromAddress("sync");
  await context.composition.store.submitPairingCode(code);
}

describe("组合根：配对完成 → 写入已配对主机记录 → 触发连接（§7.3 / design D2/D3）", () => {
  it("已批准：记录含主机标识 / 主机公钥 / 规范化来源，且立刻连接（不等第二次入口）", async () => {
    const context = setup({ fetch: successfulPairingFetch() });
    await context.composition.start();
    // 未配对的起点：一条连接都不建。
    expect(context.sockets.sockets).toHaveLength(0);
    expect(context.composition.store.state.connection.state).toBe("unpaired");

    await enterPairingByCode(context);
    expect(context.composition.store.pairingPage().phase).toBe("claiming");

    // 轮询节奏由注入的假时钟驱动：推进一个窗口才有第一次状态查询（否则页面会空转打服务端）。
    await advanceUntil(
      context,
      () => context.composition.store.pairingPage().phase === "pending_confirmation",
      "等待本机确认",
    );

    await advanceUntil(
      context,
      () => context.composition.store.pairingPage().phase === "approved",
      "已批准",
    );

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
    // 因此这里断言它不在未配对侧；仍停在 `unpaired`（没开始配对）或没触发连接（0 条 socket）都会红。
    expect(context.composition.store.state.connection.state).toBe("authenticating");
    expect(context.sockets.sockets).toHaveLength(1);
    expect(context.sockets.urls[0]?.startsWith("wss://work-pc.example.ts.net")).toBe(true);
    expect(context.sockets.urls[0]?.endsWith("/sync/v1")).toBe(true);

    // 载荷用完即弃：这一页不再持有秘密（已配对结果里只有公开材料）。
    expect(JSON.stringify(context.composition.store.state)).not.toContain(PAIRING_SECRET);
    context.composition.dispose();
  });

  it("配对码入口：lookup → 同一套 claim/轮询 → 落盘并触发连接", async () => {
    let statusPolls = 0;
    const context = setup({
      fetch: async (url, init): Promise<PairingHttpResponse> => {
        if (url.endsWith("/pairing/lookup")) return lookupResponseFor();
        if (JSON.parse(init.body)["pairingRequestId"] === undefined) {
          return claimResponseFor(init);
        }
        statusPolls += 1;
        return {
          ok: true,
          status: 200,
          json: () =>
            Promise.resolve(
              statusPolls === 1
                ? { status: "pending_confirmation" }
                : {
                    status: "approved",
                    device: { deviceId: DEVICE_ID },
                    host: { hostId: HOST_ID, hostPublicKey: QR_HOST_PUBLIC_KEY },
                  },
            ),
        };
      },
    });
    await context.composition.start();

    await context.composition.store.startPairingFromAddress("sync");
    expect(context.composition.store.pairingPage().phase).toBe("awaiting_code");

    await context.composition.store.submitPairingCode("abcd-efgh");

    // 第一条请求是 lookup（码在 body 里），第二条才是既有 claim 链路。
    expect(context.fetches[0]?.url).toBe(`${ORIGIN}/sync/v1/pairing/lookup`);
    expect(JSON.parse(context.fetches[0]?.body ?? "{}")).toEqual({
      protocolVersion: 1,
      pairingCode: "ABCD-EFGH",
    });
    expect(context.fetches[1]?.url).toBe(`${ORIGIN}/sync/v1/pairing/claim`);

    // lookup 返回的 `expiresAt` 立即成为页面的绝对截止时间（不是本地 +5 分钟）。
    const claiming = context.composition.store.pairingPage();
    expect(claiming.phase).toBe("claiming");
    expect(claiming.presentation.countdown).toBe(true);
    expect(typeof claiming.expiresAtMs).toBe("number");

    await advanceUntil(
      context,
      () => context.composition.store.pairingPage().phase === "pending_confirmation",
      "等待本机确认",
    );
    await advanceUntil(
      context,
      () => context.composition.store.pairingPage().phase === "approved",
      "已批准",
    );

    expect([...context.cache.entries.keys()]).toEqual([`paired-host/${ORIGIN}`]);
    const entry = context.cache.entries.get(`paired-host/${ORIGIN}`);
    expect(entry?.kind === "local" ? entry.summary : null).toBe(
      JSON.stringify({ hostId: HOST_ID, publicKeyBase64Url: QR_HOST_PUBLIC_KEY }),
    );
    expect(context.sockets.sockets).toHaveLength(1);
    // 码不进公开状态，也不进任何持久面。
    expect(JSON.stringify(context.composition.store.state)).not.toContain("ABCD-EFGH");
    expect(JSON.stringify([...context.cache.entries])).not.toContain("ABCD-EFGH");
    context.composition.dispose();
  });

  it("重新配对：进页与等待确认保留旧记录，批准后替换公钥", async () => {
    const context = setup({ fetch: successfulPairingFetch() });
    await context.composition.rememberPairedHost({
      hostId: HOST_ID, publicKeyBase64Url: MISMATCHED_HOST_PUBLIC_KEY, canonicalOrigin: ORIGIN,
    });
    const key = "paired-host/" + ORIGIN;
    const before = context.cache.entries.get(key);
    await context.composition.start();
    await context.composition.store.startPairingFromAddress("sync");
    expect(context.composition.store.pairingPage().presentation.title).toBe("重新配对");
    expect(context.cache.entries.get(key)).toEqual(before);
    expect(context.fetches).toHaveLength(0);
    await context.composition.store.submitPairingCode("ABCD-EFGH");
    await advanceUntil(context, () => context.composition.store.pairingPage().phase === "pending_confirmation", "等待确认");
    expect(context.cache.entries.get(key)).toEqual(before);
    await advanceUntil(context, () => context.composition.store.pairingPage().phase === "approved", "批准");
    const entry = context.cache.entries.get(key);
    expect(entry?.kind === "local" ? entry.summary : null).toBe(
      JSON.stringify({ hostId: HOST_ID, publicKeyBase64Url: QR_HOST_PUBLIC_KEY }),
    );
    context.composition.dispose();
  });

  it("旧设备 data= 内容：先清除片段，再提示已停用；一个配对请求都不发", async () => {
    const context = setup({ fetch: successfulPairingFetch() });
    await context.composition.start();

    for (const [name, text] of [
      ["JSON 非法", `${ORIGIN}/pair#data=not-base64url`],
      ["协议版本不符", legacyDevicePairingUrl({ pairingProtocol: "acp-remote-pairing-v2" })],
      ["字段缺失", legacyDevicePairingUrl({ hostId: undefined })],
      ["字段长度不符", legacyDevicePairingUrl({ pairingSecret: "AQID" })],
      ["内容不是 JSON", `${ORIGIN}/pair#data=${encodeBase64Url(new TextEncoder().encode("把这一段贴进来"))}`],
    ] as const) {
      const { replaced } = await openPairingAddress(context, text);
      const page = context.composition.store.pairingPage();
      // 反例：恢复旧解码（或尽力解析）会进入 claim → `fetches` 非空、页面也不是「已停用」。
      expect(page.phase, name).toBe("legacy_device_qr_unsupported");
      expect(page.presentation.title, name).toBe("设备二维码旧格式已停用");
      expect(page.presentation.warning, name).toContain("ACP Remote App");
      expect(replaced, name).toEqual(["/pair"]);
      expect(context.fetches, name).toHaveLength(0);
    }
    expect(context.sockets.sockets).toHaveLength(0);
    context.composition.dispose();
  });

  it("/pair 收到 node-link 载荷：专用入口不匹配文案，且不发请求", async () => {
    const context = setup({ fetch: successfulPairingFetch() });
    await context.composition.start();

    const nodeLink = {
      pairingProtocol: "acp-remote-nodelink-v1",
      ownerNodeId: HOST_ID,
      ownerPublicKey: QR_HOST_PUBLIC_KEY,
      endpoint: "wss://work-pc.example.ts.net/node-link/v1",
      pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
      pairingSecret: PAIRING_SECRET,
      expiresAt: new Date(Date.now() + 600_000).toISOString(),
    };
    const payload = encodeBase64Url(new TextEncoder().encode(JSON.stringify(nodeLink)));

    const { replaced } = await openPairingAddress(context, `${ORIGIN}/pair#data=${payload}`);

    const page = context.composition.store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("kind_mismatch");
    expect(page.presentation.title).toBe("这不是本入口的配对地址");
    expect(replaced).toEqual(["/pair"]);
    expect(context.fetches).toHaveLength(0);
    context.composition.dispose();
  });

  /**
   * 打开一条**已过期**的 node-link `#data=` 地址：读取层抛 `pairing.expired`，入口收口。
   *
   * 这个页面没有配对码输入，因此过期必须呈现节点专用的「配对地址已过期」；落到配对码口径
   * （「配对码不对或已过期」+「生成新的配对码后再输入」）会给出无法执行的恢复路径，并把
   * 真实诊断码改写成 `pairing.code_invalid`。
   */
  it("node-link 地址已过期：节点专用的过期呈现，不落到配对码失败、不发请求", async () => {
    const context = setup({
      fetch: async () => {
        throw new Error("过期 node-link 地址不得发任何配对请求");
      },
    });
    await context.composition.start();

    const expiredNodeLink = {
      pairingProtocol: "acp-remote-nodelink-v1",
      ownerNodeId: HOST_ID,
      ownerPublicKey: QR_HOST_PUBLIC_KEY,
      endpoint: "wss://work-pc.example.ts.net/node-link/v1",
      pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
      pairingSecret: PAIRING_SECRET,
      // 已经早于当前时刻：读取层据此明确失败（`readNodeLinkQrPayload`）。
      expiresAt: new Date(0).toISOString(),
    };
    const payload = encodeBase64Url(new TextEncoder().encode(JSON.stringify(expiredNodeLink)));

    const { replaced } = await openPairingAddress(
      context,
      `${ORIGIN}/node-link/pair#data=${payload}`,
      "node-link",
    );

    const page = context.composition.store.pairingPage();
    // 反例：若过期仍按配对码口径收口 → 阶段是 `failed`、标题变成「配对码不对或已过期」。
    expect(page.phase).toBe("expired");
    expect(page.kind).toBe("node-link");
    expect(page.failure).toBeNull();
    expect(page.presentation.kind).toBe("failure");
    expect(page.presentation.title).toBe("配对地址已过期");
    expect(page.presentation.warning).toBe("请在电脑上重新发起配对，并用新的节点地址打开本页。");
    // 本页没有配对码入口：固定文案里不得出现「配对码」这条走不通的指引。
    expect(`${page.presentation.lede}${page.presentation.warning}`).not.toContain("配对码");
    // 诊断码保留真实原因，不被改写成 `pairing.code_invalid`。
    expect(page.error?.code).toBe("pairing.expired");
    // 片段在任何请求之前清除；一个配对请求都不发。
    expect(replaced).toEqual(["/pair"]);
    expect(context.fetches).toHaveLength(0);
    context.composition.dispose();
  });

  it("配对码 lookup 返回已过期：固定 S6 呈现，不 claim、不轮询、不落盘", async () => {
    const context = setup({
      fetch: async (url): Promise<PairingHttpResponse> => {
        if (url.endsWith("/pairing/lookup")) {
          return lookupResponseFor({ expiresAt: new Date(0).toISOString() });
        }
        throw new Error("过期绑定之后不应再发任何请求");
      },
    });
    await context.composition.start();

    await enterPairingByCode(context);

    const page = context.composition.store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("code_invalid");
    expect(page.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    // 只有那一次 lookup：没有 claim、没有 status、没有已配对记录。
    expect(context.fetches).toHaveLength(1);
    expect(context.fetches[0]?.url).toBe(`${ORIGIN}/sync/v1/pairing/lookup`);
    expect([...context.cache.entries.keys()]).toEqual([]);
    expect(context.sockets.sockets).toHaveLength(0);
    context.composition.dispose();
  });

  it("直接打开 /pair（无片段）：进入配对码输入（S0），且一个配对请求都不发", async () => {
    const context = setup({ fetch: successfulPairingFetch() });
    await context.composition.start();

    await context.composition.store.startPairingFromAddress("sync");

    const page = context.composition.store.pairingPage();
    expect(page.phase).toBe("awaiting_code");
    expect(page.presentation.kind).toBe("code_entry");
    expect(page.origin).toBe(ORIGIN);
    expect(page.secureContext).toBe(true);
    expect(page.presentation.note).toBe("配对码 5 分钟内有效。");
    expect(page.presentation.countdown).toBe(false);
    expect(context.fetches).toHaveLength(0);
    context.composition.dispose();
  });

  it("直接打开 /node-link/pair（无片段）：仍是无载荷页，不提供配对码", async () => {
    const context = setup({ fetch: successfulPairingFetch() });
    await context.composition.start();

    await context.composition.store.startPairingFromAddress("node-link");

    const page = context.composition.store.pairingPage();
    expect(page.phase).toBe("idle");
    expect(page.failure).toBe("no_payload");
    expect(page.presentation.title).toBe("这个地址里没有节点配对信息");
    expect(context.fetches).toHaveLength(0);
    context.composition.dispose();
  });

  it("误扫新设备短二维码：先清除片段，只显示 App 指引；凭据不进页面、日志、历史或存储", async () => {
    const fragment = DEVICE_QR_URL.slice(DEVICE_QR_URL.indexOf("#"));
    const payloadText = fragment.slice("#q2=".length);
    const addressBar = stubAddressBar(fragment);
    const logged: string[] = [];
    const record = (value: unknown): void => {
      logged.push(String(value));
    };
    const spies = [
      vi.spyOn(console, "debug").mockImplementation(record),
      vi.spyOn(console, "info").mockImplementation(record),
      vi.spyOn(console, "log").mockImplementation(record),
      vi.spyOn(console, "warn").mockImplementation(record),
      vi.spyOn(console, "error").mockImplementation(record),
    ];
    const context = setup({
      fetch: async () => {
        throw new Error("误扫设备二维码不得发任何配对请求");
      },
    });

    try {
      await context.composition.start();
      await context.composition.store.startPairingFromAddress("sync");

      const page = context.composition.store.pairingPage();
      expect(page.phase).toBe("device_qr_app_only");
      expect(page.presentation.title).toBe("请在 ACP Remote App 内扫码");
      expect(page.presentation.lede).toContain("请在 ACP Remote App 内扫码，或返回网页使用配对码");
      // 页面只呈现固定指引：没有对端信息、来源或有效期从片段里被读出。
      expect(page.host).toBeNull();
      expect(page.origin).toBeNull();
      expect(page.expiresAtMs).toBeNull();

      // 片段在任何 UI/请求之前清除；只调用 `replaceState`，因此没有新增历史条目。
      expect(addressBar.replaced).toEqual(["/pair"]);
      expect(globalThis.location.hash).toBe("");

      // 敏感载荷不出现在页面模型、可导出状态、缓存、日志或出站请求里。
      const surfaces = JSON.stringify({
        page,
        state: context.composition.store.state,
        cache: [...context.cache.entries],
        logged,
      });
      expect(surfaces).not.toContain(payloadText);
      expect(context.fetches).toHaveLength(0);

      // 刷新（再次进入裸 /pair）恢复配对码输入；凭据不从历史或存储恢复。
      await context.composition.store.startPairingFromAddress("sync");
      const refreshed = context.composition.store.pairingPage();
      expect(refreshed.phase).toBe("awaiting_code");
      expect(refreshed.presentation.kind).toBe("code_entry");
    } finally {
      for (const spy of spies) spy.mockRestore();
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

    await enterPairingByCode(context);
    await advanceUntil(
      context,
      () => context.composition.store.pairingPage().phase === "pending_confirmation",
      "等待本机确认",
    );
    // 直接等「原因已落到页面」：收口失败本身就是这条用例的被测对象，等它比等中间的 `approved` 稳。
    await advanceUntil(
      context,
      () => context.composition.store.pairingPage().error !== null,
      "收口失败的原因落到页面",
    );

    // 配对本身成功了：记录已落盘、连接机已推进到「已配对」——呈现必须承认这一点。
    expect(context.cache.entries.has(`paired-host/${ORIGIN}`)).toBe(true);
    const page = context.composition.store.pairingPage();
    expect(page.phase).toBe("approved");
    expect(context.composition.store.state.connection.state).toBe("disconnected");
    const notice = page.error?.message ?? "";
    expect(page.error?.code).toBe("internal.unavailable");
    expect(notice.length).toBeGreaterThan(0);
    expect(notice).not.toContain(PLATFORM_FAILURE);

    const markup = renderToStaticMarkup(createElement(PairingPanel, { model: page }));
    // 呈现仍是 approved 的进度：标题与说明说的都是「配对成了、正在进目录」。
    expect(page.presentation.kind).toBe("progress");
    expect(page.presentation.title).toBe("已通过审批，正在进入目录");
    expect(page.presentation.lede).toContain("配对已完成");
    expect(markup).toContain(page.presentation.title);
    expect(markup).toContain(page.presentation.lede);
    expect(markup).toContain('data-pairing-error="internal.unavailable"');
    // 固定文案之外不渲染 `error.message`；也不会出现通用失败页的「配对请求未能完成」。
    expect(markup).not.toContain(notice);
    expect(markup).not.toContain("配对请求未能完成");
    expect(markup).not.toContain(PLATFORM_FAILURE);
    context.composition.dispose();
  });
});

describe("组合根：配对请求被拒绝时收口（不把状态留在「进行中」）", () => {
  it("claim 的 `fetch` 抛出（断网）：落到请求失败的固定呈现，且能立刻重试成功", async () => {
    let claims = 0;
    const context = setup({
      fetch: async (url, init): Promise<PairingHttpResponse> => {
        if (url.endsWith("/pairing/lookup")) return lookupResponseFor();
        if (JSON.parse(init.body)["pairingRequestId"] === undefined) {
          claims += 1;
          // 第一次断网：`fetch` 直接拒绝（真实实现里没有第二层 catch，异常从 claim 冒泡上来）。
          if (claims === 1) throw new Error("Failed to fetch");
          return claimResponseFor(init);
        }
        return { ok: true, status: 200, json: () => Promise.resolve({ status: "pending_confirmation" }) };
      },
    });
    await context.composition.start();

    await enterPairingByCode(context);

    const failed = context.composition.store.pairingPage();
    // 反例：若 `claim` 的拒绝直接冒泡（没有收口），阶段会停在 `claiming` → 断言变红。
    expect(failed.phase).toBe("failed");
    expect(failed.failure).toBe("request_failed");
    expect(failed.presentation.title).toBe("配对请求未能完成");
    expect(failed.error?.code).toBe("internal.unavailable");
    expect(failed.error?.retryable).toBe(true);
    // 没有连接被建立，也没有任何已配对记录被写入。
    expect(context.sockets.sockets).toHaveLength(0);
    expect([...context.cache.entries.keys()]).toEqual([]);

    // 会话真的结束了：同一次页面会话里可以立刻重试，且这次走到了轮询。
    await enterPairingByCode(context);
    expect(context.composition.store.pairingPage().phase).toBe("claiming");
    await advanceUntil(
      context,
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

    await enterPairingByCode(context);
    await advanceUntil(
      context,
      () => context.composition.store.pairingPage().phase === "pending_confirmation",
      "等待本机确认",
    );
    await advanceUntil(
      context,
      () => context.composition.store.pairingPage().phase === "approved",
      "已批准",
    );

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
      fetch: async (url, init): Promise<PairingHttpResponse> => {
        if (url.endsWith("/pairing/lookup")) return lookupResponseFor({ hostId: NEW_HOST_ID });
        if (JSON.parse(init.body)["pairingRequestId"] === undefined) {
          return claimResponseFor(init);
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

    await enterPairingByCode(context);
    await advanceUntil(
      context,
      () => context.composition.store.pairingPage().phase === "pending_confirmation",
      "等待本机确认",
    );
    await advanceUntil(
      context,
      () => context.composition.store.pairingPage().phase === "approved",
      "已批准",
    );

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

describe("组合根：下载服务经平台端口装配", () => {
  it("未认证时不打开落点、不派发，并给出显式原因", async () => {
    const context = setup();
    const started = await context.composition.downloads.start({
      sessionId: SESSION_ID,
      path: "src/a.ts",
      mimeType: "text/plain",
      sizeBytes: "10",
    });
    expect(started).toBe(false);
    const state = context.composition.downloads.state;
    expect(state.kind).toBe("failed");
    expect(state.kind === "failed" ? state.error.code : null).toBe("internal.unavailable");
    // 未配对时没有任何 socket；下载也没有绕开连接闸门。
    expect(context.sockets.sockets).toHaveLength(0);
    context.composition.dispose();
  });
});

/** 让 `FakeSocketFactory` 的 `open` 与本文件用到的类型保持一致（`SocketPort` 的最小面）。 */
