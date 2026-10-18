/**
 * 组合根：把平台端口装进同步客户端与客户端状态仓库。
 *
 * ## 为什么落在 `src/composition.ts`
 *
 * `src/layers.test.ts` 把 `src/platform/` 定为横切层——七层中的任何一层 import 它都是违规，
 * 平台能力**只能**由更外层的装配点接入（`AGENTS.md` §6）。七层都排除了，唯一能承接这件事的
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
 * - 配对链路：`ClientStore` 的配对用例（`startPairingFromAddress`）经这里的接线拿到网络
 *   （`HttpPairingTransport`）、地址栏（读片段并**立即**清除）、本地存储（已配对主机记录）
 *   与时钟（轮询节奏与到期定时）。页面与组件因此不碰这些能力；配对完成后同一条装配路径
 *   （`adoptPairedHost`）把连接建起来，不会出现第二份连接装配。
 * - 环境事实：配对入口的「安全上下文」与「来源一致」两条校验经 `CompositionInputs` 的
 *   `secureContext` / `origin` 注入（缺省读浏览器全局 `isSecureContext` 与 `location.origin`）。
 *   它们与 `fetch` 同属一次性能力的注入面，因此**不**占 `src/platform/**` 的冻结成员表（design D2）。
 */

import {
  DECLARED_SUBPROTOCOLS,
  encodeBase64Url,
  decodeBase64Url,
  openWebSocket,
  sortedFeatures,
  SyncClient,
  syncSocketUrl,
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
import type {
  PairingLookupOutcome,
  PairingPollResult,
  PairingQrRead,
  PairingTransport,
  PairingWiring,
} from "./features/pairing-model";
import { PairingQrInvalidError } from "./features/pairing-model";
import type { ErrorCode, NodeLinkQrPayload, PairingLookupResponse, PublicError } from "./protocol";
import { DEVICE_QR_FRAGMENT_PREFIX } from "./protocol";
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
  hostPublicKey: 7,
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
  pairingHostProof: "acp-remote/pairing-host-proof/v1",
  hostChallenge: "acp-remote/host-challenge/v1",
  pairingStatus: "acp-remote/pairing-status/v1",
} as const;

/** `$defs.canonicalOrigin`：`^https://[^/?#]+$`。 */
const CANONICAL_ORIGIN_PATTERN = /^https:\/\/[^/?#]+$/;

/**
 * `$defs.uuid`：`^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`。
 *
 * 与 schema 逐字相同（小写、固定宽度、无花括号）：lookup 响应的 `pairingId`/`hostId` 之后会
 * 进入 transcript 的 `uuid16` 编码，形状不符必须在响应解析处就判为失败，而不是拖到 claim 才炸。
 */
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** 两类二维码载荷的 `pairingProtocol` 常量（`schemas/sync/v1/pairing.schema.json`、`schemas/node-link/v1/pairing.schema.json`）。 */
const SYNC_PAIRING_PROTOCOL = "acp-remote-pairing-v1";
const NODE_LINK_PAIRING_PROTOCOL = "acp-remote-nodelink-v1";

/**
 * `schemas/node-link/v1/pairing.schema.json` 的 `$defs.endpoint`：`^wss://[^/?#]+/node-link/v1$`。
 *
 * 端点形状是 §13.1 的既有口径（HTTP origin 由它派生），因此在这里按同一口径校验，
 * 不放宽成「任意 wss 地址」。
 */
const NODE_LINK_ENDPOINT_PATTERN = /^wss:\/\/[^/?#]+\/node-link\/v1$/;

/**
 * 二维码地址里承载载荷的片段标记（节点协议 §13.1：`https://<origin>/node-link/pair#data=<base64url-json>`）。
 *
 * 秘密**必须**在 fragment 而不是 query：query 会进服务器日志、Referer 与浏览器历史。
 * 新设备短二维码的 `q2=` 标记来自 `src/protocol/device-qr.ts`（浏览器只清除它，不解析）。
 */
const QR_DATA_MARKER = "data=";

/** 裸载荷的字符集：无填充 base64url。命中它才当成「只给了载荷」。 */
const QR_BASE64URL_PATTERN = /^[A-Za-z0-9_-]+$/;

/** 二维码里两条 base64url 字段的字节长度（§7.1 要求 PWA 校验字段长度；wire 形状是 32 / 65）。 */
const QR_SECRET_BYTES = 32;
const QR_HOST_PUBLIC_KEY_BYTES = 65;
/** §7.2 的 `hostProof`：P-1363 的 `r||s`，64 字节（`common.schema.json#/$defs/base64url64`）。 */
const HOST_PROOF_SIGNATURE_BYTES = 64;

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
  /** claim 响应（§7.2）里本客户端必须消费的两个字段：`serverNonce` 与主机证明 `hostProof`。 */
  readonly serverNonce?: unknown;
  readonly hostProof?: unknown;
  readonly device?: { readonly deviceId?: unknown } | undefined;
  /** 已批准时的主机身份材料（§7.3：`approved` 响应返回 Host identity）。 */
  readonly host?: { readonly hostId?: unknown; readonly hostPublicKey?: unknown } | undefined;
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

/**
 * 本地摘要文本 → 已配对主机记录。
 *
 * - 摘要不是对象 / 缺 `hostId`：返回 `null`——**从不是**一次配对（记录被清或被改坏），
 *   按未配对处理，页面引导重新配对；验签因此不会被放行（e2e 的 CR-4 就钉这一条）。
 * - `hostId` 在、公钥缺失：返回**公钥为空**的记录，而不是 `null`。依据 design D2 的迁移口径
 *   （旧路径写入的记录没有主机公钥，其表现必须是「连接时进入主机身份变化阻断态」）：
 *   空公钥在 `verifyHostChallenge` 里解不出 `CryptoKey`，因此必然进阻断态而不是被放行，
 *   而「按未配对处理」会让用户看到的表象与真实原因不符，且静默不连接无从诊断。
 */
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
  if (typeof hostId !== "string" || hostId.length === 0) return null;
  return { hostId, publicKeyBase64Url: typeof publicKeyBase64Url === "string" ? publicKeyBase64Url : "" };
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

// ── 认领响应的主机证明（design D2 的 L1）──────────────────────────────────────

/**
 * §7.2 的主机证明校验：**用二维码里固定的主机公钥**验证认领响应里的 `hostProof`。
 *
 * ## 为什么信任根只能来自二维码
 *
 * 配对载荷是**带外**（二维码）送达本设备的：它同时携带主机公钥与一次性配对秘密。认领响应
 * 走的是网络，中间人可以对它做任何事——因此它 MUST NOT 成为「这台电脑是谁」的依据。
 * 这里用的公钥来自 `qr.hostPublicKey`，响应的任何字段都不参与「验证谁」的判定（响应里
 * 根本没有公钥字段）。被签的 transcript 覆盖 hostId、deviceId、pairingId、来源、两把公钥、
 * 两个 nonce 与 pairingRequestId，因此「换一台电脑」「换一个来源」「重放到别的会话」
 * 都会让验签失败。
 *
 * ## 失败即停止
 *
 * 缺材料、材料形状不符、公钥导不进来、签名验不过——四种情形一律返回明确失败。调用方
 * （`#claim`）据此**不**登记 `pairingRequestId`，页面因此不会进入轮询，也不会有任何已配对
 * 主机记录被写入。
 */
async function verifyClaimHostProof(input: {
  readonly body: PairingStatusBody;
  readonly qr: PairingLookupResponse;
  readonly canonicalOrigin: string;
  readonly deviceId: string;
  readonly devicePublicKeyBase64Url: string;
  readonly clientNonce: ByteArray;
}): Promise<{ readonly kind: "ok"; readonly pairingRequestId: string } | { readonly kind: "error"; readonly error: PublicError }> {
  const pairingRequestId = input.body.pairingRequestId;
  const serverNonce = input.body.serverNonce;
  const hostProof = input.body.hostProof;
  const missing = [
    typeof pairingRequestId === "string" ? null : "pairingRequestId",
    typeof serverNonce === "string" && isBase64UrlLength(serverNonce, QR_SECRET_BYTES) ? null : "serverNonce",
    typeof hostProof === "string" && isBase64UrlLength(hostProof, HOST_PROOF_SIGNATURE_BYTES) ? null : "hostProof",
  ].filter((field): field is string => field !== null);
  if (
    missing.length > 0 ||
    typeof pairingRequestId !== "string" ||
    typeof serverNonce !== "string" ||
    typeof hostProof !== "string"
  ) {
    return {
      kind: "error",
      error: {
        code: "protocol.schema_invalid",
        message: `认领响应缺少可用的主机证明材料（${missing.join("、")}）：本次配对不会被继续。`,
        retryable: false,
        details: {},
      },
    };
  }

  const publicKey = await importHostPublicKey(input.qr.hostPublicKey);
  if (publicKey === null) {
    return {
      kind: "error",
      error: {
        code: "auth.proof_invalid",
        message: "二维码里的主机公钥无法用于验签：请在电脑上重新生成二维码后再扫。",
        retryable: false,
        details: {},
      },
    };
  }

  const transcript = encodeTranscript(DOMAIN.pairingHostProof, [
    { tag: TAG.protocolVersion, bytes: transcriptCodec.u16be(PAIRING_PROTOCOL_VERSION) },
    { tag: TAG.hostId, bytes: transcriptCodec.uuid16(input.qr.hostId) },
    { tag: TAG.deviceId, bytes: transcriptCodec.uuid16(input.deviceId) },
    { tag: TAG.pairingId, bytes: transcriptCodec.uuid16(input.qr.pairingId) },
    { tag: TAG.canonicalOrigin, bytes: transcriptCodec.utf8(input.canonicalOrigin) },
    { tag: TAG.hostPublicKey, bytes: decodeBase64Url(input.qr.hostPublicKey) },
    { tag: TAG.devicePublicKey, bytes: decodeBase64Url(input.devicePublicKeyBase64Url) },
    { tag: TAG.clientNonce, bytes: input.clientNonce },
    { tag: TAG.serverNonce, bytes: decodeBase64Url(serverNonce) },
    { tag: TAG.pairingRequestId, bytes: transcriptCodec.uuid16(pairingRequestId) },
  ]);

  const verified = await crypto.subtle
    .verify(
      { name: "ECDSA", hash: "SHA-256" },
      publicKey,
      decodeBase64Url(hostProof) as BufferSource,
      transcript as BufferSource,
    )
    .catch(() => false);
  if (!verified) {
    return {
      kind: "error",
      error: {
        code: "auth.proof_invalid",
        message: "认领响应里的主机证明无法用二维码里的主机公钥验证通过：这台电脑不是二维码所指的那台，配对已停止。",
        retryable: false,
        details: {},
      },
    };
  }
  return { kind: "ok", pairingRequestId };
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
 * 2. `proof` 是 `HMAC-SHA256(pairingSecret, transcript)`（§7.2/§7.3），key 只从配对码 lookup
 *    的成功响应读，每次轮询换新 `requestNonce`（schema 要求）。
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

  /**
   * 二维码/片段内容 → 它的**类型**（design D3 的读取口）。
   *
   * ## 只有 node-link 载荷会被解析
   *
   * 用户能拿到的文本有三种：节点二维码里的完整地址
   * （`https://<origin>/node-link/pair#data=<base64url-json>`）、地址里的片段本身、以及裸
   * base64url 载荷。容器的宽松**不**等于载荷的宽松：无论从哪一种容器取出 JSON 文本，
   * node-link 字段校验都逐项照旧（字段齐全、端点形状、字段长度、过期时间），不符即抛
   * `PairingQrInvalidError`，**绝不**尽力解析。
   *
   * 设备（sync）协议常量只用于**分类**：返回 `{ kind: "sync" }`，不读取任何设备字段。
   * 浏览器已不再消费设备资料（design D6），分类结果只用来让 node-link 入口说清
   * 「这不是本入口的配对地址」。
   *
   * ## 类型在这里只被**标注**，不被判定
   *
   * 入口把这里标出的 `kind` 与路由声明的类型对上，因此判定不依赖任何对载荷形状的猜测，
   * 也不在这里替入口做取舍。
   */
  readQrPayload(text: string): PairingQrRead {
    const payload = parseJsonObject(qrPayloadJsonOf(text));
    if (payload === null) {
      throw new PairingQrInvalidError("protocol.invalid_json", "二维码内容不是 JSON 对象");
    }
    const protocol = payload["pairingProtocol"];
    if (typeof protocol !== "string" || protocol.length === 0) {
      throw new PairingQrInvalidError("protocol.schema_invalid", "二维码缺少字段 pairingProtocol");
    }
    // 设备载荷只分类、不解析：任何字段缺失、长度不符或已过期都不构成浏览器侧的成功/失败判定，
    // 浏览器对它只有「App 专用」这一条路径（`q2=` 甚至不会走到这里）。
    if (protocol === SYNC_PAIRING_PROTOCOL) return { kind: "sync" };
    if (protocol === NODE_LINK_PAIRING_PROTOCOL) {
      return { kind: "node-link", qr: readNodeLinkQrPayload(payload, this.#now()) };
    }
    throw new PairingQrInvalidError("protocol.schema_invalid", "二维码协议版本不匹配");
  }

  /**
   * `POST /sync/v1/pairing/lookup`（§7.5）：用 8 位配对码一次性换回绑定与秘密。
   *
   * 所有失败收敛成 `{ kind: "error" }`：HTTP 非 200、正文不是 JSON、字段缺失/长度不符、
   * 响应里的 `canonicalOrigin` 与请求来源不同，都不把任何正文内容带出去（design D4）。
   */
  async lookup(input: {
    readonly canonicalOrigin: string;
    readonly pairingCode: string;
  }): Promise<PairingLookupOutcome> {
    const response = await this.#fetch(`${input.canonicalOrigin}/sync/v1/pairing/lookup`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        protocolVersion: PAIRING_PROTOCOL_VERSION,
        pairingCode: input.pairingCode,
      }),
    }).catch(() => null);
    if (response === null || !response.ok) return { kind: "error" };
    const body: unknown = await response.json().catch(() => null);
    const qr = readLookupResponse(body, input.canonicalOrigin);
    return qr === null ? { kind: "error" } : { kind: "ok", qr };
  }

  /** `POST /sync/v1/pairing/claim`（§7.2）。 */
  async claim(input: {
    readonly qr: PairingLookupResponse;
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
    // §7.2：认领响应携带主机证明 `hostProof`。设备 MUST 用二维码里**固定**的主机公钥验证它，
    // 绝不拿响应自带的任何公钥做「自证」（响应里本就没有公钥）。失败即明确失败——既不写任何
    // 已配对主机记录，也不进入轮询。
    const claim = await verifyClaimHostProof({
      body,
      qr: input.qr,
      canonicalOrigin: input.canonicalOrigin,
      deviceId: identity.deviceId,
      devicePublicKeyBase64Url: identity.publicKeyBase64Url,
      clientNonce,
    });
    if (claim.kind === "error") return claim.error;
    this.#requestIdByPairing.set(input.qr.pairingId, claim.pairingRequestId);
    return null;
  }

  /** `POST /sync/v1/pairing/status`（§7.3）。业务状态只由 body 表达，HTTP 永远 200。 */
  async poll(input: {
    readonly qr: PairingLookupResponse;
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
      // §7.3：`approved` 响应必须同时给出设备元数据与 Host identity；两者缺一就不算已批准
      // （design D2：已配对记录缺公钥会让此后**每次**连接都进「主机身份变化」阻断态，
      // 且公钥补不出来——只能重配对，因此宁可在配对当场明确失败）。
      const deviceId = body.device?.deviceId;
      const hostId = body.host?.hostId;
      const hostPublicKey = body.host?.hostPublicKey;
      const missing = [
        typeof deviceId === "string" ? null : "device.deviceId",
        typeof hostId === "string" ? null : "host.hostId",
        typeof hostPublicKey === "string" && isBase64UrlLength(hostPublicKey, QR_HOST_PUBLIC_KEY_BYTES)
          ? null
          : "host.hostPublicKey",
        // 主机标识必须与二维码里的是同一台：否则「已批准」指的是另一台主机，落盘的记录会张冠李戴。
        typeof hostId === "string" && hostId !== input.qr.hostId ? `host.hostId != ${input.qr.hostId}` : null,
        // 状态响应**没有**主机签名（§7.3），因此主机公钥只能靠与二维码里那把**逐字节相同**来
        // 确立——不 trim、不折叠大小写、不做前缀匹配。这是此后每次 WSS 连接验签的唯一信任根，
        // 这里放水会让设备此后长期信任一把不是二维码带来的公钥。
        typeof hostPublicKey === "string" && hostPublicKey !== input.qr.hostPublicKey
          ? "host.hostPublicKey != 二维码里的主机公钥"
          : null,
      ].filter((field): field is string => field !== null);
      if (
        missing.length > 0 ||
        typeof deviceId !== "string" ||
        typeof hostId !== "string" ||
        typeof hostPublicKey !== "string"
      ) {
        return {
          kind: "error",
          error: {
            code: "protocol.schema_invalid",
            message: `已批准响应缺少可用的主机身份材料（${missing.join("、")}）：该设备不会被当作已配对。`,
            retryable: false,
            details: {},
          },
        };
      }
      return { kind: "approved", deviceId, hostId, hostPublicKey };
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

// ── 二维码内容与地址片段 ────────────────────────────────────────────────────

/**
 * node-link 配对二维码载荷（`NODE_LINK_PROTOCOL.md` §13.1 / `schemas/node-link/v1/pairing.schema.json`
 * 的 `$defs.qrPayload`）的字段校验。
 *
 * 容器已经解开（`qrPayloadJsonOf`），这里只管**载荷**：字段齐全、端点形状、两条 base64url 字段的
 * 长度（32 / 65 字节）、过期时间逐项照旧，不尽力解析。
 * 这里**不**判定「入口是否接受它」——入口按声明的类型决定（design D2/D3）。
 */
function readNodeLinkQrPayload(payload: Record<string, unknown>, now: number): NodeLinkQrPayload {
  const qr = payload as unknown as NodeLinkQrPayload;
  for (const key of ["ownerNodeId", "ownerPublicKey", "endpoint", "pairingId", "pairingSecret", "expiresAt"] as const) {
    if (typeof qr[key] !== "string" || qr[key].length === 0) {
      throw new PairingQrInvalidError("protocol.schema_invalid", `二维码缺少字段 ${key}`);
    }
  }
  if (!NODE_LINK_ENDPOINT_PATTERN.test(qr.endpoint)) {
    throw new PairingQrInvalidError("protocol.schema_invalid", "二维码端点形状非法");
  }
  if (!isBase64UrlLength(qr.pairingSecret, QR_SECRET_BYTES)) {
    throw new PairingQrInvalidError("protocol.schema_invalid", "二维码字段 pairingSecret 长度不符");
  }
  if (!isBase64UrlLength(qr.ownerPublicKey, QR_HOST_PUBLIC_KEY_BYTES)) {
    throw new PairingQrInvalidError("protocol.schema_invalid", "二维码字段 ownerPublicKey 长度不符");
  }
  const expiresAt = Date.parse(qr.expiresAt);
  if (Number.isNaN(expiresAt)) {
    throw new PairingQrInvalidError("protocol.schema_invalid", "二维码过期时间不是时间戳");
  }
  if (expiresAt <= now) {
    throw new PairingQrInvalidError("pairing.expired", "二维码已过期");
  }
  return qr;
}

/**
 * 用户给的文本 → 待校验的 JSON 文本（只解**容器**，不做任何载荷级放宽）。
 *
 * 依次判定三种容器：JSON 文本本身、地址（或片段）里的 `data=`、裸 base64url 载荷。
 * 三者都不匹配即抛错——不猜、不截断、不把任意文本当 JSON 试。
 */
function qrPayloadJsonOf(text: string): string {
  const trimmed = text.trim();
  if (trimmed.startsWith("{")) return trimmed;
  const marker = trimmed.indexOf(QR_DATA_MARKER);
  if (marker >= 0) {
    const data = (trimmed.slice(marker + QR_DATA_MARKER.length).split("&")[0] ?? "").trim();
    return decodeQrPayloadBase64Url(data);
  }
  if (QR_BASE64URL_PATTERN.test(trimmed)) return decodeQrPayloadBase64Url(trimmed);
  throw new PairingQrInvalidError("protocol.schema_invalid", "不是配对二维码内容");
}

/** base64url 载荷 → UTF-8 文本；不是 base64url、或解出的字节不是 UTF-8 即抛错。 */
function decodeQrPayloadBase64Url(data: string): string {
  let bytes: ByteArray;
  try {
    bytes = decodeBase64Url(data);
  } catch {
    throw new PairingQrInvalidError("protocol.schema_invalid", "二维码载荷不是 base64url");
  }
  try {
    // `fatal: true`：非法字节序列直接抛错，而不是替换成 U+FFFD 后继续——后者会让乱码看起来像合法 JSON。
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new PairingQrInvalidError("protocol.schema_invalid", "二维码载荷不是 UTF-8 文本");
  }
}

/**
 * lookup 响应正文 → 与二维码载荷同形的 `qr`；任何一项不符返回 `null`。
 *
 * 响应里的 `canonicalOrigin` 必须与**请求**来源相同：lookup 是同源请求，返回另一来源就意味着
 * 这不是本页要配对的电脑（design D5 的首次信任边界）。字段长度按 §7.1 的 32 / 65 字节校验；
 * `pairingId`/`hostId` 与 schema 的 `$defs.uuid` 同口径校验形状，而不只是非空——它们随后会
 * 进入 transcript 与 claim 正文，形状不符在这里就该失败。
 */
function readLookupResponse(body: unknown, requestedOrigin: string): PairingLookupResponse | null {
  if (typeof body !== "object" || body === null) return null;
  const source = body as Record<string, unknown>;
  if (source["protocolVersion"] !== PAIRING_PROTOCOL_VERSION) return null;
  const pairingId = source["pairingId"];
  const hostId = source["hostId"];
  const hostPublicKey = source["hostPublicKey"];
  const canonicalOrigin = source["canonicalOrigin"];
  const pairingSecret = source["pairingSecret"];
  const expiresAt = source["expiresAt"];
  if (
    typeof pairingId !== "string" ||
    typeof hostId !== "string" ||
    typeof hostPublicKey !== "string" ||
    typeof canonicalOrigin !== "string" ||
    typeof pairingSecret !== "string" ||
    typeof expiresAt !== "string"
  ) {
    return null;
  }
  if (!UUID_PATTERN.test(pairingId) || !UUID_PATTERN.test(hostId)) return null;
  if (!CANONICAL_ORIGIN_PATTERN.test(canonicalOrigin)) return null;
  if (canonicalOrigin !== requestedOrigin) return null;
  if (!isBase64UrlLength(pairingSecret, QR_SECRET_BYTES)) return null;
  if (!isBase64UrlLength(hostPublicKey, QR_HOST_PUBLIC_KEY_BYTES)) return null;
  if (Number.isNaN(Date.parse(expiresAt))) return null;
  return {
    protocolVersion: PAIRING_PROTOCOL_VERSION,
    hostId,
    hostPublicKey,
    canonicalOrigin,
    pairingId,
    pairingSecret,
    expiresAt,
  };
}

/** 文本是否恰好是 `bytes` 字节的 base64url（§7.1：PWA 必须校验字段长度）。 */
function isBase64UrlLength(value: string, bytes: number): boolean {
  try {
    decodeBase64Url(value, bytes);
    return true;
  } catch {
    return false;
  }
}

/**
 * 读取地址片段并**立即**清除它（design D6：敏感内容位于 fragment，读取后立即
 * `history.replaceState`，先于任何 UI 呈现、持久化或请求）。
 *
 * 只把设备 `q2=` 与 `data=` 片段当作配对内容：其余片段既不被解析、也不被清除——清除别人的
 * 片段可能破坏它承载的语义。`q2=` 只交给调用方判断「App 专用」，浏览器不解析其载荷；
 * `data=` 只用于 node-link 读取口径。没有地址栏（非浏览器环境）或没有配对片段时返回 `null`。
 */
function takePairingFragment(): string | null {
  if (typeof globalThis.location === "undefined") return null;
  const hash = globalThis.location.hash;
  const fragment = hash.startsWith("#") ? hash.slice(1) : hash;
  const recognized =
    fragment.startsWith(DEVICE_QR_FRAGMENT_PREFIX) || fragment.startsWith(QR_DATA_MARKER);
  if (!recognized) return null;
  if (typeof globalThis.history !== "undefined") {
    // `replaceState` 不触发导航，因此页面不会被带离当前路由；片段之外的路径与查询原样保留。
    globalThis.history.replaceState(
      null,
      "",
      `${globalThis.location.pathname}${globalThis.location.search}`,
    );
  }
  return fragment;
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
   * 环境事实：本页是否处于安全上下文（design D2）；缺省读浏览器全局 `isSecureContext`。
   *
   * 与 `fetch` 同属「一次性能力」的注入面（而不是有状态的平台端口）：非安全上下文与跨来源
   * 这两个形态在正常测试环境里造不出来，必须可注入才能写成断言；`src/platform/**` 的成员表
   * 是冻结的装配面，环境事实因此不走平台端口。
   */
  readonly secureContext?: () => boolean;
  /** 环境事实：本页实际来源（§3.2 的 canonical origin）；缺省读浏览器全局 `location.origin`。 */
  readonly origin?: () => string;
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
  /** 配对 HTTPS 传输（二维码读取、配对码 lookup、claim 与状态轮询）。 */
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
  const secureContext = inputs.secureContext ?? defaultSecureContext;
  const currentOrigin = inputs.origin ?? defaultOrigin;

  const clock = inputs.ports?.clock ?? clockPort;
  const random = inputs.ports?.random ?? randomPort;
  const transcript = inputs.ports?.transcript ?? transcriptCodec;
  const digest = inputs.ports?.digest ?? digestPort;
  const pairedHost = inputs.ports?.host ?? new PairedHostIdentity(platform.localCache, () => clock.now());
  const pairing = new HttpPairingTransport(fetchImpl, platform.deviceIdentity, () => clock.now(), random);
  const ids: IdentifierSource = { uuid: () => random.uuid() };

  // 未配对时不编造主机身份：`hostId` 与 `url` 要等 `start()` 读到配对记录才有意义。
  let sync: SyncClient | null = null;
  /**
   * 装配 `sync` 时用的那台主机（标识 + 规范化来源）。
   *
   * `SyncClient` 把 `hostId` 与出站 URL 在**构造期**定格，而 `hostId` 会被写进每一次
   * `auth.client_hello`；因此「能不能复用既有实例」必须按这两个值判定，不能只看实例是否存在。
   */
  let assembledHost: { readonly hostId: string; readonly canonicalOrigin: string } | null = null;

  const identity: ConnectionIdentity = {
    // 只读转发：连接不存在或未认证时是 `null`，组合根**不**缓存也不编造标识。
    get connectionId(): string | null {
      return sync?.connectionId ?? null;
    },
  };

  const syncGateway: SyncGateway = {
    refreshSnapshot(): void {
      sync?.refreshSnapshot();
    },
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

  /**
   * 配对链路：把「配对状态机与用例」（`ClientStore`）接到网络、地址栏、本地存储与时钟上。
   *
   * 页面因此只调 store 的用例，不碰 `fetch`、`location` 或任何存储 API（`AGENTS.md` §6）。
   */
  const pairingWiring: PairingWiring = {
    transport: pairing,
    takeQrTextFromFragment: takePairingFragment,
    rememberPairedHost,
    onPaired: pairingCompleted,
    schedule: (delayMs, task) => clock.schedule(delayMs, task),
    // 每次轮询换新的 nonce（§7.3）：随机源只由组合根持有（CSPRNG 端口）。
    nextRequestNonce: () => encodeBase64Url(random.bytes(32)),
    // 环境事实（design D2）：缺省读浏览器全局，配对码入口据此校验安全上下文与请求来源。
    isSecureContext: secureContext,
    currentOrigin,

  };

  const store = new ClientStore({
    sync: syncGateway,
    ids,
    now: () => clock.now(),
    identity,
    pairing: pairingWiring,
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

  /**
   * 只在状态机允许时连接。
   *
   * `sync.connect()` 在门面仍处阻断态时**抛错**（`SyncClient.blocked` 非空）。这不是可以
   * 静默丢掉的：把异常吞掉会留下「已配对、状态机也说可以连，却一条连接都不建、也没有任何
   * 原因可见」的不可诊断态。这里把阻断原因交回连接机——它是阻断说明的唯一权威来源。
   */
  function connectOnce(): void {
    if (!shouldConnect()) return;
    try {
      sync?.connect();
    } catch {
      const cause = sync?.blocked ?? null;
      if (cause !== null) store.noteConnectionBlocked(cause);
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
    adoptPairedHost(record, deviceIdentity.canonicalOrigin);
    // 连接机要求 `unpaired → pair_started → pair_succeeded → disconnected` 才允许发起连接。
    store.pairSucceeded();
    connectOnce();
  }

  /**
   * 配对完成后的收口（由 `ClientStore` 在写入已配对记录后调用）：装配连接并**触发一次连接**。
   *
   * 与 `start()` 共用 `adoptPairedHost`，因此不存在第二份连接装配。这里**不**再驱动连接机的
   * 配对迁移——`ClientStore` 已经把它推到「已配对」（含从阻断态重新配对的情形）。
   */
  async function pairingCompleted(record: PairedHostRecord): Promise<void> {
    if (disposed) return;
    const deviceIdentity = await platform.deviceIdentity.loadIdentity();
    // 来源不符即不接上：设备身份绑定在 `canonicalOrigin` 上，身份不得跨来源迁移（§4.5）。
    if (deviceIdentity === null || deviceIdentity.canonicalOrigin !== record.canonicalOrigin) return;
    adoptPairedHost(record, deviceIdentity.canonicalOrigin);
    // 重新配对是**唯一**的阻断态恢复路径（design D2）：连接机的阻断态已由 `ClientStore` 的
    // `pairSucceeded()` 解除，门面那一份必须在这里同步解除——`SyncClient.connect()` 在
    // `blocked` 非空时抛错，而全仓库没有别处调用 `clearBlocking()`。漏掉这一句，
    // 「配对已完成」之后一条连接都不会建立（`connectOnce` 的异常会把它落成阻断可见）。
    sync?.clearBlocking();
    connectOnce();
  }

  /**
   * 已配对起点齐备 → 装配 `SyncClient` 与两条订阅。
   *
   * **同一台主机的同一来源**上复用既有实例：`SyncClient` 每次验签都从已配对记录里读主机公钥
   * （`PairedHostIdentity`），因此重新配对写入的新公钥对既有连接对象立即可见，重建只会丢掉
   * 在途连接与恢复点。
   *
   * 主机标识或来源变了就必须重建：`hostId` 与出站 URL 都是构造期定格的，而 `hostId` 会被写进
   * `auth.client_hello`——沿用旧实例会让 hello 自报旧主机，服务端以 `auth.host_mismatch` 拒绝，
   * 客户端又映射回身份变化阻断态，同一次页面会话里再出不去。重建时旧订阅、在途退避与旧连接
   * 一并作废，否则一条状态变化会触发两次重连。
   */
  function adoptPairedHost(record: PairedHostRecord, canonicalOrigin: string): void {
    if (
      sync !== null &&
      assembledHost?.hostId === record.hostId &&
      assembledHost.canonicalOrigin === canonicalOrigin
    ) {
      return;
    }
    if (sync !== null) {
      unsubscribeStore?.();
      unsubscribeStore = null;
      unsubscribeLifecycle?.();
      unsubscribeLifecycle = null;
      reconnectCancel?.();
      reconnectCancel = null;
      reconnectAttempts = 0;
      sync.disconnect();
      sync = null;
    }

    sync = new SyncClient({
      hostId: record.hostId,
      // 出站地址与声明的子协议都由连接层给出唯一来源（`syncSocketUrl` / `DECLARED_SUBPROTOCOLS`）：
      // 组合根只负责在装配时把它们**显式**交出去，绝不在自己这里拼 URL 或抄子协议字面量。
      url: syncSocketUrl(canonicalOrigin),
      subprotocols: DECLARED_SUBPROTOCOLS,
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
    assembledHost = { hostId: record.hostId, canonicalOrigin };
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
    assembledHost = null;
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

/**
 * 默认环境事实：`isSecureContext` 是页面自己的安全上下文判定（PWA 形态即 https 或 loopback）。
 *
 * 惰性读全局、不在装配期取值：测试环境下 `globalThis` 上可能没有这两个成员，而只有在入口
 * 真正做校验时（注入替身或真实浏览器）才会被调用。
 */
const defaultSecureContext = (): boolean => globalThis.isSecureContext === true;

/** 默认环境事实：本页实际来源——`location.origin` 本身就是 §3.2 的 canonical origin 形态。 */
const defaultOrigin = (): string => globalThis.location?.origin ?? "";
