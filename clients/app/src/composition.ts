/**
 * 组合根：把平台端口装进同步客户端与客户端状态仓库。
 *
 * ## 为什么落在 `src/composition.ts`
 *
 * `src/layers.test.ts` 把 `src/platform/` 定为横切层——七层中的任何一层 import 它都是违规，
 * 平台能力**只能**由更外层的装配点接入（`AGENTS.md` §5）。七层都排除了，唯一能承接这件事的
 * 就是一个层级中立的文件：`layerOfSourcePath("src/composition.ts")` 返回 `null`，因此它在机检里
 * 不参与方向判定，同时**可以** import `src/platform/`、`src/sync-client/`、`src/state/` 与
 * `src/features/`——这正是装配点该有的样子。
 *
 * 在此之前 `app/_layout.tsx` 写死 `store={null}`，每个路由都渲染 `RuntimeUnavailable`：
 * R15/R16/R18/R19 的页面在真实运行时一条都跑不到，TP4 的浏览器验证无从执行。
 *
 * ## 五个「平台暂无对应成员」的端口
 *
 * `src/sync-client/ports.ts` 的端口表登记了五项 `PlatformPorts` 没有的能力。这里逐项给出实现
 * 及其理由，**不**在 `src/platform/` 里加成员（那是 WP5b 的写入范围，且会改变已冻结的装配面）：
 *
 * | 端口 | 实现 | 为什么这样实现 |
 * | --- | --- | --- |
 * | `TranscriptCodec` | 直接转出 `src/platform/transcript.ts` 的五个函数 | 编码规则只有一份实现，包装只会引入第二处 |
 * | `DigestPort` | `crypto.subtle.digest("SHA-256", …)` | §9.4 的两级哈希要的是真 SHA-256，不是任何确定性替身 |
 * | `RandomPort` | `crypto.getRandomValues` / `crypto.randomUUID` | §6.1 要求 nonce 来自 CSPRNG |
 * | `ClockPort` | `Date.now()` / `setTimeout` | 退避定时与快照时间戳只保留一个时间来源 |
 * | `HostIdentityPort` | 已配对主机记录落 `LocalCachePort` + WebCrypto ECDSA 验签 | §8.2 要求用**已配对的** Host key 验 `hostProof`，key 不符即 `identity_changed`，不得跳过校验 |
 *
 * ## 接线与它的可观察后果
 *
 * - `ClientStore` 的 `ConnectionIdentity` 委托给 `SyncClient.connectionId`（只读 getter）：
 *   未认证时为 `null`，`submitCreateSession` 的第一道闸门因此不会被绕过，也不会编造标识。
 * - `onEvent` 单向流向 `store.apply`；**何时**重连由 `shouldAutoReconnect` 判定，
 *   组合根只执行结论，不自己判断。
 * - `pagehide` 时断开连接，未完成的暂存快照随之丢弃（§9.4）。
 */

import {
  encodeBase64Url,
  decodeBase64Url,
  openWebSocket,
  sortedFeatures,
  SyncClient,
} from "./sync-client";
import type {
  ByteArray,
  CancelTimer,
  ClockPort,
  DigestPort,
  HostChallengeInput,
  HostIdentityPort,
  PairedHostRecord,
  RandomPort,
  SocketFactory,
  TranscriptCodec,
} from "./sync-client";
import { ClientStore } from "./features/client-store";
import type { ConnectionIdentity, IdentifierSource, SyncGateway } from "./features/ports";
import type { PairingPollResult, PairingTransport } from "./features/pairing-model";
import type { ErrorCode, PairingQrPayload, PublicError } from "./protocol";
import { openPlatform } from "./platform";
import type { LifecycleEvent, PlatformPorts } from "./platform";
import {
  encodeNulJoinedUtf8,
  encodeTranscript,
  encodeU16be,
  encodeUtf8,
  encodeUuid16,
} from "./platform/transcript";

/** transcript 的全局字段 tag（`docs/SYNC_PROTOCOL.md` §6.3）。 */
const TAG = {
  protocolVersion: 1,
  hostId: 2,
  deviceId: 3,
  pairingId: 4,
  pairingExpiresAt: 5,
  canonicalOrigin: 6,
  devicePublicKey: 8,
  clientNonce: 9,
  serverNonce: 10,
  connectionId: 11,
  pairingRequestId: 13,
  deviceName: 14,
  clientKind: 15,
  requestNonce: 16,
} as const;

/** 本实现使用的 transcript domain（§6.3 表）。 */
const DOMAIN = {
  pairingProof: "acp-remote/pairing-proof/v1",
  hostChallenge: "acp-remote/host-challenge/v1",
  pairingStatus: "acp-remote/pairing-status/v1",
} as const;

/** `$defs.canonicalOrigin`：`^https://[^/?#]+$`。 */
const CANONICAL_ORIGIN_PATTERN = /^https:\/\/[^/?#]+$/;

/** P-256 SPKI 的 DER 前缀：后接 65 字节 SEC1 未压缩点即可被 `importKey("spki")` 接受。 */
const P256_SPKI_PREFIX = new Uint8Array([
  0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86,
  0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
]);

/** 配对用的协议版本与客户端种类（§6.3 的 v1 取值）。 */
const PAIRING_PROTOCOL_VERSION = 1;
const CLIENT_KIND = "pwa";

/** 自动重连的退避参数（毫秒）：上限让长断连不至于无限重试。 */
const RECONNECT = { baseDelayMs: 500, maxDelayMs: 30_000 } as const;

/** 已配对主机记录在本地缓存里的键前缀。 */
const PAIRED_HOST_KEY_PREFIX = "paired-host/";

/** 同步协议版本（`schemas/sync/v1` 的 `const 1`）。 */
const SYNC_PROTOCOL_VERSION = 1;

/** 配对失败时本客户端出示的设备名。页面**没有**任何名称输入，因此这是固定值而不是用户输入回显。 */
const PAIRING_DEVICE_NAME = "本浏览器设备";

/** `PairingTerminalStatus`：`rejected` / `expired` / `consumed` 三种终态（§7.3）。 */
type PairingTerminalStatus = "rejected" | "expired" | "consumed";

/** 轮询响应正文里本客户端消费的字段。 */
interface PairingStatusBody {
  readonly status?: unknown;
  readonly pairingRequestId?: unknown;
  readonly device?: { readonly deviceId?: unknown } | undefined;
  readonly code?: unknown;
  readonly message?: unknown;
  readonly retryable?: unknown;
  readonly correlationId?: unknown;
  readonly details?: unknown;
}

// ── 五个平台端口的实现 ───────────────────────────────────────────────────────

/**
 * `TranscriptCodec`：`src/platform/transcript.ts` 的导出直接构成端口形状。
 *
 * 这里只做**改名对齐**，不包装、不复制：包装会让人误以为还有第二种编码规则。
 */
const transcriptCodec: TranscriptCodec = {
  encode: encodeTranscript,
  u16be: encodeU16be,
  uuid16: encodeUuid16,
  utf8: encodeUtf8,
  nulJoinedUtf8: encodeNulJoinedUtf8,
};

/** 摘要端口：真实 SHA-256（快照 digest 的两级哈希都要真算法，§9.4）。 */
const digestPort: DigestPort = {
  async sha256(data: Uint8Array): Promise<Uint8Array> {
    return new Uint8Array(await crypto.subtle.digest("SHA-256", data as BufferSource));
  },
};

/** 随机源端口：CSPRNG（§6.1）。 */
const randomPort: RandomPort = {
  bytes(length: number): ByteArray {
    return crypto.getRandomValues(new Uint8Array(length));
  },
  uuid(): string {
    return crypto.randomUUID();
  },
};

/**
 * 时钟端口：`Date.now()` 与 `setTimeout`。
 *
 * 退避定时与快照时间戳共用这一个来源：两套时钟一旦来自不同的时基，同一份快照的
 * `verifiedAtMs` 与 ACK 时序就会互相矛盾，诊断无从下手。
 */
const clockPort: ClockPort = {
  now(): number {
    return Date.now();
  },
  schedule(delayMs: number, task: () => void): CancelTimer {
    const handle = setTimeout(task, delayMs);
    return () => {
      clearTimeout(handle);
    };
  },
};

/**
 * `HostIdentityPort`：已配对主机记录 + Host challenge 验签。
 *
 * ## 记录为什么落在 `LocalCachePort`
 *
 * 平台层没有「已配对主机记录」这个端口，而写入范围不允许新增一个。`LocalCachePort` 的
 * 本节点条目（`kind: "local"`）是唯一可用的持久面：它按来源分流且只允许精简摘要，
 * 写入的是公开材料（`hostId` + Host 公钥），不含任何会话内容。
 *
 * **已登记的局限**：本地摘要有固定 TTL（30 天）与 LRU 配额，条目过期或被淘汰后
 * `loadPairedHost` 返回 `null`，连接按 §8.2 进入 `identity_changed` 并要求重新配对。
 * 这不是静默降级（验签绝不被跳过），但「已配对」的时长被一条缓存策略限定了。
 * 彻底解法是给平台层加一个专用持久端口，属独立变更。
 *
 * ## 为什么必须真的验签
 *
 * §8.2 与 R11「来源变化后不再自动信任」都要求用**已配对**的 Host key 验 `hostProof`，
 * key 与记录不一致即 `identity_changed`。因此这里没有「跳过校验」的分支：记录缺失、
 * key 无法导入、签名解不开、验签抛错一律返回 `false`，由连接层进入阻断态。
 */
class PairedHostIdentity implements HostIdentityPort {
  readonly #cache: PlatformPorts["localCache"];
  readonly #now: () => number;

  constructor(cache: PlatformPorts["localCache"], now: () => number) {
    this.#cache = cache;
    this.#now = now;
  }

  async loadPairedHost(canonicalOrigin: string): Promise<PairedHostRecord | null> {
    const hit = await this.#cache.get(`${PAIRED_HOST_KEY_PREFIX}${canonicalOrigin}`, this.#now());
    if (hit === null || hit.entry.kind !== "local") return null;
    const parsed = parseStoredPairedHost(hit.entry.summary);
    // 记录缺失或被外部改坏：按「从未配对」处理，用户重新配对即可恢复——不在解析上崩。
    if (parsed === null) return null;
    return { hostId: parsed.hostId, publicKeyBase64Url: parsed.publicKeyBase64Url, canonicalOrigin };
  }

  async verifyHostChallenge(input: HostChallengeInput): Promise<boolean> {
    const paired = await this.loadPairedHost(input.canonicalOrigin);
    // 无记录即无从校验；返回 false 让连接层进入 `identity_changed`，绝不「默认通过」。
    if (paired === null || paired.hostId !== input.hostId) return false;

    const publicKey = await importHostPublicKey(paired.publicKeyBase64Url);
    if (publicKey === null) return false;

    const transcript = encodeTranscript(DOMAIN.hostChallenge, [
      { tag: TAG.protocolVersion, bytes: transcriptCodec.u16be(SYNC_PROTOCOL_VERSION) },
      { tag: TAG.hostId, bytes: transcriptCodec.uuid16(input.hostId) },
      { tag: TAG.deviceId, bytes: transcriptCodec.uuid16(input.deviceId) },
      { tag: TAG.canonicalOrigin, bytes: transcriptCodec.utf8(input.canonicalOrigin) },
      { tag: TAG.clientNonce, bytes: input.clientNonce },
      { tag: TAG.serverNonce, bytes: input.serverNonce },
      { tag: TAG.connectionId, bytes: transcriptCodec.uuid16(input.connectionId) },
      { tag: 12, bytes: transcriptCodec.nulJoinedUtf8(sortedFeatures(input.negotiatedFeatures)) },
    ]);

    let signature: ByteArray;
    try {
      signature = decodeBase64Url(input.signatureBase64Url);
    } catch {
      return false;
    }
    return crypto.subtle
      .verify({ name: "ECDSA", hash: "SHA-256" }, publicKey, signature as BufferSource, transcript as BufferSource)
      .catch(() => false);
  }
}

/** 已配对主机记录在本地缓存里的形状（只存公开材料）。 */
interface StoredPairedHost {
  readonly hostId: string;
  readonly publicKeyBase64Url: string;
}

/** 本地摘要文本 → 已配对主机记录；形状不符返回 `null`（而不是抛错或部分取值）。 */
function parseStoredPairedHost(summary: string): StoredPairedHost | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(summary) as unknown;
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const record = parsed as Record<string, unknown>;
  const hostId = record["hostId"];
  const publicKeyBase64Url = record["publicKeyBase64Url"];
  if (typeof hostId !== "string" || typeof publicKeyBase64Url !== "string") return null;
  return { hostId, publicKeyBase64Url };
}

/** 65 字节 SEC1 未压缩点 + P-256 SPKI 前缀 → 可验签的 `CryptoKey`；形状不符返回 `null`。 */
async function importHostPublicKey(publicKeyBase64Url: string): Promise<CryptoKey | null> {
  let raw: ByteArray;
  try {
    raw = decodeBase64Url(publicKeyBase64Url);
  } catch {
    return null;
  }
  if (raw.length !== 65 || raw[0] !== 0x04) return null;
  const spki = new Uint8Array(P256_SPKI_PREFIX.length + 65);
  spki.set(P256_SPKI_PREFIX, 0);
  spki.set(raw, P256_SPKI_PREFIX.length);
  return crypto.subtle
    .importKey("spki", spki as BufferSource, { name: "ECDSA", namedCurve: "P-256" }, false, ["verify"])
    .catch(() => null);
}

// ── 配对传输（HTTPS 面）────────────────────────────────────────────────────

/** 配对 HTTPS 响应的最小形状。 */
export interface PairingHttpResponse {
  readonly ok: boolean;
  readonly status: number;
  json(): Promise<unknown>;
}

/**
 * `fetch` 的最小形状。
 *
 * 组合根的 HTTPS 面只有这一个网络出口；声明成最小形状后，注入替身只需给这三个方法。
 */
export type FetchLike = (
  url: string,
  init: { readonly method: string; readonly headers: Record<string, string>; readonly body: string },
) => Promise<PairingHttpResponse>;

/** 配对错误码的封闭词表：服务端正文里的码不在表内时退回按 HTTP 状态推导（不从正文取新码）。 */
const PAIRING_ERROR_CODES: readonly string[] = [
  "pairing.failed",
  "pairing.qr_invalid",
  "pairing.origin_mismatch",
  "pairing.expired",
  "pairing.not_found",
  "pairing.rate_limited",
  "pairing.proof_invalid",
];

/** HTTP 状态 → 配对错误码（§7.4 的封闭表）。 */
function codeForPairingStatus(status: number): string {
  if (status === 401) return "pairing.proof_invalid";
  if (status === 403) return "pairing.origin_mismatch";
  if (status === 404) return "pairing.not_found";
  if (status === 410) return "pairing.expired";
  if (status === 429) return "pairing.rate_limited";
  return "pairing.failed";
}

/** 把响应正文收敛成一个 `PublicError`；码不在词表内时按 HTTP 状态推导。 */
function toPublicError(body: unknown, status: number, fallbackMessage: string): PublicError {
  const source = (typeof body === "object" && body !== null ? body : {}) as Record<string, unknown>;
  const declared = typeof source["code"] === "string" ? source["code"] : null;
  const code = declared !== null && PAIRING_ERROR_CODES.includes(declared) ? declared : codeForPairingStatus(status);
  const message = typeof source["message"] === "string" && source["message"].length > 0 ? source["message"] : fallbackMessage;
  const rawDetails = source["details"];
  const details: Record<string, unknown> =
    typeof rawDetails === "object" && rawDetails !== null ? { ...(rawDetails as Record<string, unknown>) } : {};
  // `correlationId` 不是 `PublicError` 的字段（wire 上是 `null | Uuid`），因此按开放对象
  // 的 `details` 透传，不为它在协议类型上加一个不存在的字段。
  if (typeof source["correlationId"] === "string") details["correlationId"] = source["correlationId"];
  return {
    code: code as ErrorCode,
    message,
    retryable: source["retryable"] === true,
    details,
  };
}

/**
 * `PairingTransport` 的 HTTPS 实现。
 *
 * 三条协议要求在这里落地，且都**不**有降级分支：
 *
 * 1. 只接受同源 `application/json`（§7.1）：URL 由 `canonicalOrigin` 拼出，body 是 JSON，
 *    不使用 Cookie、Basic Auth 或 URL query 承载凭据。
 * 2. `proof` 是 `HMAC-SHA256(pairingSecret, transcript)`（§7.2/§7.3），key 只从二维码读，
 *    每次轮询换新 `requestNonce`（schema 要求）。
 * 3. 业务状态只由 body 的 `status` 表达，`expired`/`consumed` 同样是 HTTP 200（§7.3）——
 *    因此这里**不**用 `ok` 判定业务结果。
 */
class HttpPairingTransport implements PairingTransport {
  readonly #fetch: FetchLike;
  readonly #device: PlatformPorts["deviceIdentity"];
  readonly #now: () => number;
  /** CSPRNG：nonce 与设备标识都从这里来（§6.1），因此由组合根注入而不是各写一份。 */
  readonly #random: RandomPort;
  /**
   * claim 响应给出的 `pairingRequestId`，按 `pairingId` 索引。
   *
   * 状态轮询请求必须带它（§7.3），而它只在 claim 响应里出现过一次；因此它属于**这次配对会话**
   * 的中间态，存在实例内而不是每次调用现查。缺它时轮询明确失败，不发缺字段的请求。
   */
  readonly #requestIdByPairing = new Map<string, string>();

  constructor(
    fetchImpl: FetchLike,
    device: PlatformPorts["deviceIdentity"],
    now: () => number,
    random: RandomPort,
  ) {
    this.#fetch = fetchImpl;
    this.#device = device;
    this.#now = now;
    this.#random = random;
  }

  /** 二维码内容 → `pairing.qr` 载荷。形状不符即抛错，绝不「尽力解析」。 */
  readQrPayload(text: string): PairingQrPayload {
    const payload = parseJsonObject(text);
    if (payload === null) throw new Error("二维码内容不是 JSON 对象");
    const qr = payload as unknown as PairingQrPayload;
    for (const key of ["hostId", "hostPublicKey", "canonicalOrigin", "pairingId", "pairingSecret", "expiresAt"] as const) {
      if (typeof qr[key] !== "string" || qr[key].length === 0) throw new Error(`二维码缺少字段 ${key}`);
    }
    if (qr.pairingProtocol !== "acp-remote-pairing-v1") throw new Error("二维码协议版本不匹配");
    if (!CANONICAL_ORIGIN_PATTERN.test(qr.canonicalOrigin)) throw new Error("二维码来源形状非法");
    const expiresAt = Date.parse(qr.expiresAt);
    if (Number.isNaN(expiresAt)) throw new Error("二维码过期时间不是时间戳");
    if (expiresAt <= this.#now()) throw new Error("二维码已过期");
    return qr;
  }

  /** `POST /sync/v1/pairing/claim`（§7.2）。 */
  async claim(input: {
    readonly qr: PairingQrPayload;
    readonly canonicalOrigin: string;
  }): Promise<PublicError | null> {
    // `ensureIdentity` 是幂等的：已绑定同一来源时返回既有身份，来源不同则抛 `origin_mismatch`
    // （R11 场景 3：身份不得跨来源迁移）。
    const identity = await this.#device.ensureIdentity({
      deviceId: this.#random.uuid(),
      canonicalOrigin: input.canonicalOrigin,
    });
    const clientNonce = this.#random.bytes(32);
    const transcript = encodeTranscript(DOMAIN.pairingProof, [
      { tag: TAG.protocolVersion, bytes: transcriptCodec.u16be(PAIRING_PROTOCOL_VERSION) },
      { tag: TAG.hostId, bytes: transcriptCodec.uuid16(input.qr.hostId) },
      { tag: TAG.deviceId, bytes: transcriptCodec.uuid16(identity.deviceId) },
      { tag: TAG.pairingId, bytes: transcriptCodec.uuid16(input.qr.pairingId) },
      { tag: TAG.pairingExpiresAt, bytes: encodeUnixSeconds(input.qr.expiresAt) },
      { tag: TAG.canonicalOrigin, bytes: transcriptCodec.utf8(input.canonicalOrigin) },
      { tag: TAG.devicePublicKey, bytes: decodeBase64Url(identity.publicKeyBase64Url) },
      { tag: TAG.clientNonce, bytes: clientNonce },
      { tag: TAG.deviceName, bytes: transcriptCodec.utf8(PAIRING_DEVICE_NAME) },
      { tag: TAG.clientKind, bytes: transcriptCodec.utf8(CLIENT_KIND) },
    ]);

    const response = await this.#fetch(`${input.canonicalOrigin}/sync/v1/pairing/claim`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        protocolVersion: PAIRING_PROTOCOL_VERSION,
        pairingId: input.qr.pairingId,
        hostId: input.qr.hostId,
        deviceId: identity.deviceId,
        deviceName: PAIRING_DEVICE_NAME,
        clientKind: CLIENT_KIND,
        canonicalOrigin: input.canonicalOrigin,
        devicePublicKey: identity.publicKeyBase64Url,
        clientNonce: encodeBase64Url(clientNonce),
        proof: await hmacBase64Url(decodeBase64Url(input.qr.pairingSecret), transcript),
      }),
    });

    const body = (await response.json()) as PairingStatusBody & Record<string, unknown>;
    if (!response.ok) return toPublicError(body, response.status, "配对声明未成功");
    if (typeof body.pairingRequestId === "string") {
      this.#requestIdByPairing.set(input.qr.pairingId, body.pairingRequestId);
    }
    return null;
  }

  /** `POST /sync/v1/pairing/status`（§7.3）。业务状态只由 body 表达，HTTP 永远 200。 */
  async poll(input: {
    readonly qr: PairingQrPayload;
    readonly requestNonce: string;
  }): Promise<PairingPollResult> {
    const identity = await this.#device.loadIdentity();
    if (identity === null) {
      return { kind: "error", error: toPublicError(null, 0, "本设备没有身份：需要重新配对") };
    }
    const pairingRequestId = this.#requestIdByPairing.get(input.qr.pairingId);
    if (pairingRequestId === undefined) {
      return { kind: "error", error: toPublicError(null, 0, "尚未声明过这次配对：没有可查询的配对请求") };
    }

    const transcript = encodeTranscript(DOMAIN.pairingStatus, [
      { tag: TAG.protocolVersion, bytes: transcriptCodec.u16be(PAIRING_PROTOCOL_VERSION) },
      { tag: TAG.hostId, bytes: transcriptCodec.uuid16(input.qr.hostId) },
      { tag: TAG.deviceId, bytes: transcriptCodec.uuid16(identity.deviceId) },
      { tag: TAG.pairingId, bytes: transcriptCodec.uuid16(input.qr.pairingId) },
      { tag: TAG.pairingRequestId, bytes: transcriptCodec.uuid16(pairingRequestId) },
      { tag: TAG.requestNonce, bytes: decodeBase64Url(input.requestNonce) },
    ]);

    const response = await this.#fetch(`${input.qr.canonicalOrigin}/sync/v1/pairing/status`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        protocolVersion: PAIRING_PROTOCOL_VERSION,
        hostId: input.qr.hostId,
        deviceId: identity.deviceId,
        pairingId: input.qr.pairingId,
        pairingRequestId,
        requestNonce: input.requestNonce,
        proof: await hmacBase64Url(decodeBase64Url(input.qr.pairingSecret), transcript),
      }),
    });

    const body = (await response.json()) as PairingStatusBody;
    if (!response.ok) return { kind: "error", error: toPublicError(body, response.status, "配对状态查询失败") };

    if (body.status === "pending_confirmation") return { kind: "pending_confirmation" };
    if (body.status === "approved") {
      const deviceId = body.device?.deviceId;
      return { kind: "approved", deviceId: typeof deviceId === "string" ? deviceId : identity.deviceId };
    }
    if (body.status === "rejected" || body.status === "expired" || body.status === "consumed") {
      const status: PairingTerminalStatus = body.status;
      return { kind: "terminal", status, error: toPublicError(body, response.status, "配对未获批准") };
    }
    return {
      kind: "error",
      error: toPublicError(null, response.status, `未知的配对状态：${String(body.status)}`),
    };
  }
}

/** JSON 文本 → 顶层对象；不是对象返回 `null`。 */
function parseJsonObject(text: string): Record<string, unknown> | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text) as unknown;
  } catch {
    return null;
  }
  return typeof parsed === "object" && parsed !== null ? (parsed as Record<string, unknown>) : null;
}

/** `HMAC-SHA256(key, message)` → 无填充 base64url。 */
async function hmacBase64Url(key: ByteArray, message: ByteArray): Promise<string> {
  const cryptoKey = await crypto.subtle.importKey("raw", key as BufferSource, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return encodeBase64Url(new Uint8Array(await crypto.subtle.sign("HMAC", cryptoKey, message as BufferSource)));
}

/** 毫秒精度 RFC 3339 → Unix 秒的 u64be（§6.3 的 tag 5）。 */
function encodeUnixSeconds(timestamp: string): ByteArray {
  const out = new Uint8Array(8);
  let remaining = BigInt(Math.floor(Date.parse(timestamp) / 1000));
  for (let index = 7; index >= 0; index -= 1) {
    out[index] = Number(remaining & 0xffn);
    remaining >>= 8n;
  }
  return out;
}

// ── 运行时 ────────────────────────────────────────────────────────────────

/** 组合根的可注入输入：每项都有生产实现，测试替身只需替换关心的那一项。 */
export interface CompositionInputs {
  /** 平台端口束；缺省 `openPlatform()`。 */
  readonly platform?: PlatformPorts;
  /** socket 工厂；缺省 `openWebSocket`。 */
  readonly openSocket?: SocketFactory;
  /** 配对 HTTPS 面；缺省全局 `fetch`。 */
  readonly fetch?: FetchLike;
  /**
   * 五个端口的覆盖入口；每一项缺省即用本文件的生产实现。
   *
   * 这是组合根作为**唯一装配点**该有的接缝：退避定时、随机源与摘要在这里可被替身接管，
   * 于是重连退避、nonce 生成与快照 digest 都能在不依赖真实时间与真实 CSPRNG 的条件下断言。
   */
  readonly ports?: Partial<CompositionPorts>;
}

/** 组合根装配的五个平台端口（生产实现见本文件上文）。 */
export interface CompositionPorts {
  readonly transcript: TranscriptCodec;
  readonly digest: DigestPort;
  readonly random: RandomPort;
  readonly clock: ClockPort;
  readonly host: HostIdentityPort;
}

/** 组合根装配出的运行时：页面只拿 `store`，其余是装配细节。 */
export interface Composition {
  /** 客户端状态仓库（`app/_layout.tsx` 注入 `ClientStoreProvider`）。 */
  readonly store: ClientStore;
  /** 配对 HTTPS 传输（`pairing.qr` 的读取、claim 与状态轮询）。 */
  readonly pairing: PairingTransport;
  /** 已配对主机记录的写入口：配对批准后登记 Host 公钥，`start()` 与验签都读它。 */
  rememberPairedHost(record: PairedHostRecord): Promise<void>;
  /** 异步启动：读设备身份与已配对主机记录，两者齐备才开始连接。 */
  start(): Promise<void>;
  /** 释放订阅与定时器。 */
  dispose(): void;
}

/**
 * 装配运行时。
 *
 * 端口实现全部在这里落地，页面与 feature 层看不到任何平台 API。`start()` 是异步的：
 * 设备身份与配对记录都在 IndexedDB 里，读取完成前 `store` 已经可渲染（呈现「未配对」），
 * 但**不会**发任何命令——`submitCreateSession` 与 `loadSession` 的第一道闸门是 `connectionId`，
 * 它在认证之前恒为 `null`。
 */
export function createComposition(inputs: CompositionInputs = {}): Composition {
  const platform = inputs.platform ?? openPlatform();
  const openSocket = inputs.openSocket ?? openWebSocket;
  const fetchImpl = inputs.fetch ?? defaultFetch;

  const clock = inputs.ports?.clock ?? clockPort;
  const random = inputs.ports?.random ?? randomPort;
  const transcript = inputs.ports?.transcript ?? transcriptCodec;
  const digest = inputs.ports?.digest ?? digestPort;
  const pairedHost = inputs.ports?.host ?? new PairedHostIdentity(platform.localCache, () => clock.now());
  const pairing = new HttpPairingTransport(fetchImpl, platform.deviceIdentity, () => clock.now(), random);
  const ids: IdentifierSource = { uuid: () => random.uuid() };

  // 未配对时不编造主机身份：`hostId` 与 `url` 要等 `start()` 读到配对记录才有意义。
  let sync: SyncClient | null = null;

  const identity: ConnectionIdentity = {
    // 只读转发：连接不存在或未认证时是 `null`，组合根**不**缓存也不编造标识。
    get connectionId(): string | null {
      return sync?.connectionId ?? null;
    },
  };

  const syncGateway: SyncGateway = {
    get current() {
      return sync?.snapshots.current ?? null;
    },
    get authenticatedInfo() {
      return sync?.authenticatedInfo ?? null;
    },
    dispatch(command): void {
      sync?.dispatch(command);
    },
  };

  const store = new ClientStore({
    sync: syncGateway,
    ids,
    now: () => clock.now(),
    identity,
  });

  let unsubscribeStore: (() => void) | null = null;
  let unsubscribeLifecycle: (() => void) | null = null;
  let reconnectCancel: CancelTimer | null = null;
  let reconnectAttempts = 0;
  let disposed = false;

  /**
   * 当前是否**应当**建立一条新连接。
   *
   * 判据是连接机的**状态名**而不是 `SyncClient.connected`，也不是 `shouldAutoReconnect`：
   *
   * - `connected` 只表示「手上有一个连接对象」，socket 被服务端关闭后它仍然是 `true`，
   *   用它当闸门会让重连永不发生；
   * - `shouldAutoReconnect` 只区分「阻断 / 未配对」与「其余」，答不出「此刻是否已经在连」。
   *
   * 白名单 `disconnected` / `reconnecting` 一条就够：它同时排除了「正在连接 / 正在认证 /
   * 正在追平 / 已在线」（否则每次状态变化都会再开一条连接）与四个阻断态与未配对。
   */
  function shouldConnect(): boolean {
    if (disposed || sync === null) return false;
    const state = store.state.connection.state;
    return state === "disconnected" || state === "reconnecting";
  }

  /** 状态机允许连接且当前没有在途连接时，按有界退避安排一次 `connect()`。 */
  function scheduleReconnect(): void {
    if (reconnectCancel !== null || !shouldConnect()) return;
    const delay = Math.min(RECONNECT.baseDelayMs * 2 ** reconnectAttempts, RECONNECT.maxDelayMs);
    reconnectAttempts += 1;
    reconnectCancel = clock.schedule(delay, () => {
      reconnectCancel = null;
      connectOnce();
    });
  }

  /** 只在状态机允许时连接；阻断态下 `connect()` 抛错，此时不再安排下一次。 */
  function connectOnce(): void {
    if (!shouldConnect()) return;
    try {
      sync?.connect();
    } catch {
      return;
    }
  }

  async function rememberPairedHost(record: PairedHostRecord): Promise<void> {
    const stored: StoredPairedHost = { hostId: record.hostId, publicKeyBase64Url: record.publicKeyBase64Url };
    await platform.localCache.putLocalSummary({
      key: `${PAIRED_HOST_KEY_PREFIX}${record.canonicalOrigin}`,
      summary: JSON.stringify(stored),
      nowMs: clock.now(),
    });
  }

  async function start(): Promise<void> {
    if (disposed) return;
    const deviceIdentity = await platform.deviceIdentity.loadIdentity();
    if (deviceIdentity === null) return;
    const record = await pairedHost.loadPairedHost(deviceIdentity.canonicalOrigin);
    // 未配对：停在 `unpaired`，页面引导去配对页；连接机的迁移表也不允许从这里发起连接。
    if (record === null) return;

    store.setHostIdentity({
      address: deviceIdentity.canonicalOrigin,
      hostId: record.hostId,
      deviceId: deviceIdentity.deviceId,
    });

    sync = new SyncClient({
      hostId: record.hostId,
      url: syncUrlFor(deviceIdentity.canonicalOrigin),
      identity: platform.deviceIdentity,
      host: pairedHost,
      random,
      transcript,
      digest,
      openSocket,
      // 恢复点跨**连接**由门面的 `#ackedCursor` 承接；跨进程需要持久化，本组合根不持有
      // 第二份恢复点存储，因此首次装配从无游标开始（服务端会补发全量快照）。
      resumeCursor: null,
      clock,
      onEvent: (event) => {
        store.apply(event);
        // 追平完成才算真有进展：据此重置退避预算，与 `connection.ts` 的缺口预算同一口径。
        if (event.kind === "online") reconnectAttempts = 0;
        scheduleReconnect();
      },
    });

    unsubscribeStore = store.subscribe(scheduleReconnect);
    unsubscribeLifecycle = platform.lifecycle.subscribe((event: LifecycleEvent) => {
      // `pagehide` 断开连接：未完成的暂存快照随之丢弃（§9.4），恢复路径交给下次连接。
      if (event.kind === "pagehide") sync?.disconnect();
    });

    // 连接机要求 `unpaired → pair_started → pair_succeeded → disconnected` 才允许发起连接。
    store.pairSucceeded();
    connectOnce();
  }

  function dispose(): void {
    disposed = true;
    reconnectCancel?.();
    reconnectCancel = null;
    unsubscribeStore?.();
    unsubscribeStore = null;
    unsubscribeLifecycle?.();
    unsubscribeLifecycle = null;
    sync?.disconnect();
    sync = null;
  }

  return { store, pairing, rememberPairedHost, start, dispose };
}

/** 默认 `fetch`：组合根的 HTTPS 面只有这一个网络出口，注入替身时整条替换。 */
const defaultFetch: FetchLike = async (url, init) => {
  const response = await globalThis.fetch(url, init as RequestInit);
  return {
    ok: response.ok,
    status: response.status,
    json: () => response.json() as Promise<unknown>,
  };
};

/** `https://host[:port]` → `wss://host[:port]/sync`（§8：正式连接只允许 `wss://`）。 */
function syncUrlFor(canonicalOrigin: string): string {
  return `${canonicalOrigin.replace(/^https:\/\//, "wss://")}/sync`;
}