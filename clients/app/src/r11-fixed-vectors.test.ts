/**
 * R11 / AC3：与冻结固定向量的**跨语言字节对拍**（`tasks.md` 2.6 划给 TP3 的核心交付）。
 *
 * ## 为什么既有测试不算数
 *
 * 在本文件之前，全仓**没有一条用例让真实的 `digestPort` 参与快照校验**，也没有一条用例让
 * 真实的 `HostIdentityPort` 验过一支真实签名的字节：
 *
 * - `src/platform/transcript.test.ts` 用**自己按 registry 重建**的字段顺序调 `encodeTranscript`，
 *   断言的其实是「同一个编码器对自己一致」，没有覆盖生产代码里 `TAG` → 字段的**选择与顺序**；
 * - `src/composition.test.ts` 的 `cacheBackedHost` 把 `verifyHostChallenge` 直接 `return true`，
 *   摘要则注入 `fakeDigest`（FNV-1a，不是 SHA-256）。
 *
 * 因此「TypeScript 与 Rust 编出同一串字节」这件事在本文件之前**从未被验证过**：字段 tag 写错、
 * 顺序错、features 未排序、摘要算法换成 FNV——任何一种都能让既有 359 条用例全绿。
 *
 * ## 本文件走的三条真实路径
 *
 * 1. **device proof**：真实 `openDeviceIdentityStore()`（真 `crypto.subtle.generateKey`，
 *    `extractable: false`）被真实 `SyncConnection` 喂进 transcript，签出的字节与向量的
 *    `transcriptBase64url` / `transcriptSha256Hex` 逐字节相同；且**向量自带的 `p1363Signature`
 *    （Rust 侧签的）能验过这些字节**——它能验过就等于两侧字节相同。
 * 2. **host challenge**：真实 `PairedHostIdentity`（组合根的生产实现，未被替身替换）用
 *    `host-challenge.json` 的 `p1363Signature` 验签通过，握手因此继续到 `auth.client_proof`。
 *    任何一个字节不同 → 验签失败 → `identity_changed` 阻断。
 * 3. **快照 digest**：真实 `digestPort`（`crypto.subtle.digest("SHA-256")`）参与 §9.4 的两级
 *    哈希校验；期望值由 **Node `crypto.createHash`（独立实现）** 算出，因此真实端口若被换成
 *    FNV-1a，本组用例会红。
 *
 * ## 替身的边界
 *
 * 本文件只替换**输入**（时钟、随机源、socket 传输），不替换任何被验证对象：transcript 编码器、
 * 设备身份、主机身份、摘要端口全部是生产实现（`digest`/`transcript`/`host` 三个端口一律不注入）。
 *
 * 唯一的包装是 `recordingIdentity`：它把 `signDeviceProof` 的入参记下来后**原样转发**给真实
 * 端口，以便断言被签的字节——签名本身仍由真实的 `secure-storage.web.ts` 完成。
 */

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { beforeEach, describe, expect, it } from "vitest";

import { createComposition } from "./composition";
import type { Composition, FetchLike } from "./composition";
import { encodeBase64Url, sortedFeatures } from "./sync-client";
import { encodeNulJoinedUtf8, encodeTranscript, encodeU16be, encodeUtf8, encodeUuid16 } from "./platform/transcript";
import type { TranscriptField } from "./platform/transcript";
import { openDeviceIdentityStore } from "./platform/secure-storage.web";
import type { DeviceIdentity, DeviceIdentityPort } from "./platform/secure-storage";
import { createVolatileImportedContent, openLocalCache } from "./platform/local-cache.web";
import { openLifecycle } from "./platform/lifecycle.web";
import type { PlatformPorts } from "./platform";
import { installFakeIndexedDB } from "./platform/testing/fake-indexeddb";
import { repoRoot } from "./protocol/paths";
import { FakeClock, FakeSocketFactory } from "./sync-client/testing/harness";
import type { FakeSocket } from "./sync-client/testing/harness";

/**
 * 等待一个可观察条件成立（**让出到宏任务**）。
 *
 * 不能复用 `sync-client/testing/harness.ts` 的 `waitUntil`：它只轮转微任务，而真实的
 * `crypto.subtle.verify` / `digest` 的 promise 在 Node 里要等到底层 libuv 的完成回调
 * （一次宏任务）才 resolve。只轮转微任务会永远等不到——本文件必须自己写等待器。
 *
 * 等的是**条件**而不是时长：`setTimeout(…, 0)` 的 0ms 只表示「排到下一轮事件循环」，
 * 条件一旦成立立即返回。因此它与固定 sleep 不同：不掩盖竞态，也不引入固定延迟。
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

/**
 * Node 22 的全局 `crypto` 与浏览器同源（同一份 WebCrypto 实现），因此直接用它，不做任何替身。
 *
 * 用全局而不是 `node:crypto` 的 `webcrypto`：后者在类型面上是一套独立的声明，
 * 与浏览器侧的 `CryptoKey` 不兼容，会在 `tsc --noEmit` 上报错。运行时是同一份实现。
 */
const subtle = globalThis.crypto.subtle;

// ── 冻结资产（只读消费，绝不复制）─────────────────────────────────────────

/** `compatibility/transcripts/v1/transcripts.json` 里的一条字段登记。 */
interface RegistryField {
  readonly tag: number;
  readonly name: string;
  readonly type: string;
  readonly inputKey: string;
}

/** registry 里的一个 domain 登记。 */
interface RegistryDomain {
  readonly domain: string;
  readonly protocol: string;
  readonly fields: readonly RegistryField[];
}

/** `fixtures/sync/v1/transcripts/*.json` 的一条固定向量。 */
interface TranscriptVector {
  readonly domain: string;
  readonly codec: string;
  readonly input: Readonly<Record<string, unknown>>;
  readonly expected: {
    readonly transcriptBase64url: string;
    readonly transcriptSha256Hex: string;
    readonly publicKey: string;
    readonly privateJwk: JsonWebKey;
    readonly p1363Signature: string;
  };
}

const registry = JSON.parse(
  readFileSync(join(repoRoot, "compatibility", "transcripts", "v1", "transcripts.json"), "utf8"),
) as { readonly domains: readonly RegistryDomain[] };

const deviceProofVector = JSON.parse(
  readFileSync(join(repoRoot, "fixtures", "sync", "v1", "transcripts", "device-proof.json"), "utf8"),
) as TranscriptVector;

const hostChallengeVector = JSON.parse(
  readFileSync(join(repoRoot, "fixtures", "sync", "v1", "transcripts", "host-challenge.json"), "utf8"),
) as TranscriptVector;

/** 每次对拍新建的库/连接标识；这里只需要一个合法 UUID 形状。 */
const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const NOW_MS = 1_700_000_000_000;

// ── 字节工具 ──────────────────────────────────────────────────────────

/** 无填充 base64url 解码（只用于把向量里的文本还原成字节参与验签）。 */
function decodeBase64Url(text: string): Uint8Array<ArrayBuffer> {
  const padded = text.replaceAll("-", "+").replaceAll("_", "/");
  const binary = atob(padded + "=".repeat((4 - (padded.length % 4)) % 4));
  const out = new Uint8Array(new ArrayBuffer(binary.length));
  for (let index = 0; index < binary.length; index += 1) out[index] = binary.charCodeAt(index);
  return out;
}

/** 十六进制文本 → 字节。 */
function hexToBytes(hex: string): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(new ArrayBuffer(hex.length / 2));
  for (let index = 0; index < out.length; index += 1) {
    out[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  return out;
}

/** 字节 → 小写十六进制文本（用于逐字节比对，避免 base64 文本差异掩盖单字节偏移）。 */
function bytesToHex(bytes: Uint8Array): string {
  let out = "";
  for (const byte of bytes) out += byte.toString(16).padStart(2, "0");
  return out;
}

/** 真实 `crypto.subtle.digest("SHA-256")` 的十六进制结果。 */
async function sha256Hex(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  return bytesToHex(new Uint8Array(await subtle.digest("SHA-256", bytes)));
}

// ── 「服务端视角」的 transcript 编码 ──────────────────────────────────

/**
 * 按 `compatibility/transcripts/v1/transcripts.json` 的字段表编码。
 *
 * **这是「Rust 侧会编出哪些字节」的模型，不是被测对象**：被测对象是生产代码里
 * `TAG` → 字段的**选择与顺序**。两者若不一致，`p1363Signature` 立刻验不过。
 */
function encodeAsRegistryDomain(
  domain: RegistryDomain,
  input: Readonly<Record<string, unknown>>,
): Uint8Array<ArrayBuffer> {
  const fields: TranscriptField[] = domain.fields.map((field) => {
    const raw = input[field.inputKey];
    switch (field.type) {
      case "u16be":
        return { tag: field.tag, bytes: encodeU16be(Number(raw)) };
      case "uuid16":
        return { tag: field.tag, bytes: encodeUuid16(String(raw)) };
      case "utf8":
        return { tag: field.tag, bytes: encodeUtf8(String(raw)) };
      case "bytes32":
        return { tag: field.tag, bytes: hexToBytes(String(raw)) };
      case "nul-joined-utf8":
        return { tag: field.tag, bytes: encodeNulJoinedUtf8([...(raw as readonly string[])]) };
      default:
        throw new Error(`本组用例未覆盖的字段类型 ${field.type}`);
    }
  });
  return encodeTranscript(domain.domain, fields);
}

/** registry 里按 domain 名取登记；缺项即失败（fixture 与 registry 漂移时立刻可见）。 */
function registryDomain(domain: string): RegistryDomain {
  const found = registry.domains.find((candidate) => candidate.domain === domain);
  if (found === undefined) throw new Error(`registry 缺少 ${domain}`);
  return found;
}

/** P-256 SPKI 的 DER 前缀：后接 65 字节 SEC1 未压缩点即可被 `importKey("spki")` 接受。 */
const P256_SPKI_PREFIX = new Uint8Array([
  0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86, 0x48,
  0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
]);

/** 65 字节 SEC1 公钥 → 可验签的 `CryptoKey`（真实 `crypto.subtle.importKey`）。 */
async function importSec1PublicKey(publicKeyBase64Url: string): Promise<CryptoKey> {
  const raw = decodeBase64Url(publicKeyBase64Url);
  if (raw.length !== 65 || raw[0] !== 0x04) throw new Error("公钥不是 65 字节 SEC1 未压缩点");
  const spki = new Uint8Array(new ArrayBuffer(P256_SPKI_PREFIX.length + 65));
  spki.set(P256_SPKI_PREFIX, 0);
  spki.set(raw, P256_SPKI_PREFIX.length);
  return subtle.importKey("spki", spki, { name: "ECDSA", namedCurve: "P-256" }, false, ["verify"]);
}

// ── 只控制输入的替身 ──────────────────────────────────────────────────

/**
 * 确定性随机源：首个 `bytes(32)` 给出向量里的 `clientNonceHex`，其后给可复现的字节。
 *
 * 它**只**决定 transcript 里的 nonce 输入，不参与任何编码或签名。
 */
class FixedRandom {
  #counter = 0;
  readonly #firstNonce: Uint8Array<ArrayBuffer>;
  constructor(firstNonce: Uint8Array) {
    this.#firstNonce = Uint8Array.from(firstNonce, (byte) => byte);
  }
  bytes(length: number): Uint8Array<ArrayBuffer> {
    this.#counter += 1;
    if (this.#counter === 1) {
      if (length !== this.#firstNonce.length) throw new Error(`clientNonce 长度应为 ${this.#firstNonce.length}`);
      return this.#firstNonce;
    }
    const out = new Uint8Array(new ArrayBuffer(length));
    for (let index = 0; index < length; index += 1) out[index] = (this.#counter + index) & 0xff;
    return out;
  }
  uuid(): string {
    this.#counter += 1;
    return `00000000-0000-4000-8000-${this.#counter.toString(16).padStart(12, "0")}`;
  }
}

/**
 * 记录 `signDeviceProof` 入参的**转发式**包装。
 *
 * 签名仍由真实的 `secure-storage.web.ts` 完成（真 `crypto.subtle.sign`、不可导出私钥），
 * 这里只多存一份字节副本供断言。
 */
function recordingIdentity(inner: DeviceIdentityPort, signed: Uint8Array<ArrayBuffer>[]): DeviceIdentityPort {
  return {
    loadIdentity: (): Promise<DeviceIdentity | null> => inner.loadIdentity(),
    ensureIdentity: (input: { deviceId: string; canonicalOrigin: string }): Promise<DeviceIdentity> =>
      inner.ensureIdentity(input),
    clearIdentity: (): Promise<void> => inner.clearIdentity(),
    signDeviceProof: async (transcript: Uint8Array<ArrayBuffer>): Promise<Uint8Array<ArrayBuffer>> => {
      signed.push(Uint8Array.from(transcript, (byte) => byte));
      return inner.signDeviceProof(transcript);
    },
  };
}

// ── 被测环境 ──────────────────────────────────────────────────────────

/** 一次对拍所需的全部输入。 */
interface HandshakeInput {
  readonly deviceId: string;
  readonly canonicalOrigin: string;
  readonly hostId: string;
  readonly hostPublicKeyBase64Url: string;
  readonly clientNonceHex: string;
  readonly connectionId: string;
  readonly serverNonceHex: string;
  readonly features: readonly string[];
}

/** 装配好的被测环境。 */
interface Harness {
  readonly signed: Uint8Array<ArrayBuffer>[];
  readonly sockets: FakeSocketFactory;
  readonly composition: Composition;
  readonly store: Composition["store"];
}

/** 装配真实平台端口 + 真实组合根；`digest`/`transcript`/`host` 三个端口一律用生产实现。 */
async function setup(input: HandshakeInput): Promise<Harness> {
  const platform: PlatformPorts = {
    deviceIdentity: openDeviceIdentityStore(),
    localCache: openLocalCache(),
    importedContent: createVolatileImportedContent(),
    lifecycle: openLifecycle(),
  };
  const signed: Uint8Array<ArrayBuffer>[] = [];
  const wired: PlatformPorts = { ...platform, deviceIdentity: recordingIdentity(platform.deviceIdentity, signed) };

  await platform.deviceIdentity.ensureIdentity({
    deviceId: input.deviceId,
    canonicalOrigin: input.canonicalOrigin,
  });

  const sockets = new FakeSocketFactory();
  const composition = createComposition({
    platform: wired,
    openSocket: sockets.open,
    // 只替换时间与随机源（输入）；五个平台端口的生产实现全部保留。
    ports: { clock: new FakeClock(NOW_MS), random: new FixedRandom(hexToBytes(input.clientNonceHex)) },
    fetch: (async (): Promise<never> => {
      throw new Error("本组用例不触达配对 HTTPS 面");
    }) as unknown as FetchLike,
  });

  await composition.rememberPairedHost({
    hostId: input.hostId,
    publicKeyBase64Url: input.hostPublicKeyBase64Url,
    canonicalOrigin: input.canonicalOrigin,
  });
  await composition.start();

  return { signed, sockets, composition, store: composition.store };
}

/** `auth.server_challenge`（`hostProof` 由调用方给出）。 */
function serverChallenge(input: HandshakeInput, hostProof: string): Record<string, unknown> {
  return {
    protocolVersion: 1,
    type: "auth.server_challenge",
    messageId: "cbb91891-8f84-4b47-bdf3-2c902c338367",
    body: {
      selectedProtocolVersion: 1,
      hostId: input.hostId,
      connectionId: input.connectionId,
      serverNonce: encodeBase64Url(hexToBytes(input.serverNonceHex)),
      selectedFeatures: [...input.features],
      hostProof,
    },
  };
}

/** 一条向量的 `input` → 对拍输入（host 侧字段留空，由调用方补）。 */
function inputOf(vector: TranscriptVector): HandshakeInput {
  return {
    deviceId: String(vector.input["deviceId"]),
    canonicalOrigin: String(vector.input["canonicalOrigin"]),
    hostId: String(vector.input["hostId"]),
    hostPublicKeyBase64Url: "",
    clientNonceHex: String(vector.input["clientNonceHex"]),
    connectionId: String(vector.input["connectionId"]),
    serverNonceHex: String(vector.input["serverNonceHex"]),
    features: vector.input["negotiatedFeatures"] as readonly string[],
  };
}

/**
 * 生成一对真实的 Host 密钥，并为给定的握手输入签一支 host proof。
 *
 * 被签的 transcript 用 registry 视图编码——即「服务端会编出的字节」。
 * 客户端用真实 `PairedHostIdentity` 验它：两者不一致就阻断，因此这条签发只提供**输入**。
 */
async function signHostProof(
  input: HandshakeInput,
): Promise<{ readonly publicKeyBase64Url: string; readonly hostProof: string }> {
  const keyPair = await subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, ["sign", "verify"]);
  const raw = new Uint8Array(await subtle.exportKey("raw", keyPair.publicKey));
  const hostTranscript = encodeAsRegistryDomain(registryDomain(hostChallengeVector.domain), {
    protocolVersion: 1,
    hostId: input.hostId,
    deviceId: input.deviceId,
    canonicalOrigin: input.canonicalOrigin,
    clientNonceHex: input.clientNonceHex,
    serverNonceHex: input.serverNonceHex,
    connectionId: input.connectionId,
    negotiatedFeatures: input.features,
  });
  const signature = await subtle.sign({ name: "ECDSA", hash: "SHA-256" }, keyPair.privateKey, hostTranscript);
  return {
    publicKeyBase64Url: encodeBase64Url(raw),
    hostProof: encodeBase64Url(new Uint8Array(signature)),
  };
}

/**
 * 跑完整握手直到 `auth.client_proof` 发出；返回被真实设备身份端口签过的 transcript 字节。
 *
 * @throws 握手被阻断（真实 host 验签失败）时由调用方的 `waitUntil` 抛出。
 */
async function handshakeAndCaptureDeviceProof(input: HandshakeInput): Promise<Uint8Array<ArrayBuffer>> {
  const host = await signHostProof(input);
  const harness = await setup({ ...input, hostPublicKeyBase64Url: host.publicKeyBase64Url });
  try {
    const socket = harness.sockets.latest;
    socket.open();
    await waitFor(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
    socket.deliver(serverChallenge(input, host.hostProof));
    // 反例：若真实 transcript 与「服务端字节」差一个字节，这里超时失败（进入 identity_changed）。
    await waitFor(() => socket.types().includes("auth.client_proof"), "真实验签通过并签发 clientProof");
    expect(harness.store.state.connection.blocking).toBeNull();
    expect(harness.signed).toHaveLength(1);
    return harness.signed[0] as Uint8Array<ArrayBuffer>;
  } finally {
    harness.composition.dispose();
  }
}

/** 最近建立的假 socket（只替换传输，握手与验签都是生产实现）。 */
function socketOf(harness: Harness): FakeSocket {
  return harness.sockets.latest;
}

/**
 * 跑完 `clientHello → serverChallenge → clientProof → authenticated → subscribe`。
 *
 * `hostProof` 必须是按「服务端字节」签出来的：真实 `PairedHostIdentity` 验不过就阻断。
 */
async function authenticate(harness: Harness, input: HandshakeInput, hostProof: string): Promise<void> {
  const socket = socketOf(harness);
  socket.open();
  await waitFor(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
  socket.deliver(serverChallenge(input, hostProof));
  // 反例：若真实 transcript 与「服务端字节」差一个字节，这里超时失败（进入 identity_changed）。
  await waitFor(() => socket.types().includes("auth.client_proof"), "真实验签通过并签发 clientProof");
  socket.deliver({
    protocolVersion: 1,
    type: "auth.authenticated",
    messageId: "8a536862-9a18-4766-b4ce-a2e478734980",
    connectionId: input.connectionId,
    connectionSequence: "1",
    body: {
      deviceId: input.deviceId,
      scopes: ["session.list", "session.read"],
      serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
      headGlobalSequence: "10",
      heartbeatIntervalMs: 30000,
      limits: {
        maxMessageBytes: 1048576,
        maxPromptBytes: 262144,
        maxReplayEventsPerBatch: 500,
      },
    },
  });
  await waitFor(() => socket.types().includes("sync.subscribe"), "发出 subscribe");
}

/**
 * 让出若干轮宏任务，使在途的 `crypto.subtle` promise 有机会 resolve。
 *
 * 用途是**否定断言**（"不该入仓"）：没有可等待的正面信号时，必须先把事件循环跑完若干轮，
 * 否则断言会在被测代码还没走完之前就通过——那是一条恒真断言。
 */
async function settle(rounds = 8): Promise<void> {
  for (let round = 0; round < rounds; round += 1) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
  }
}

/**
 * 用 `sync-client/testing/harness.ts` 的 FNV-1a 算法复算两级摘要。
 *
 * 存在的唯一目的是**证伪**：仓库里既有 359 条用例都用这个算法当期望值，
 * 因此把它喂给真实的 SHA-256 端口必须被拒绝——否则「真实端口从未参与校验」就成立。
 */
function fnvTwoLevelDigest(rawTexts: readonly string[]): string {
  const joined = new Uint8Array(rawTexts.length * 32);
  rawTexts.forEach((text, index) => {
    joined.set(fnv1a(new TextEncoder().encode(text)), index * 32);
  });
  return encodeBase64Url(fnv1a(joined));
}

/** `fakeDigest` 的算法本体（FNV-1a 派生 32 字节），独立复算以便证伪。 */
function fnv1a(data: Uint8Array): Uint8Array {
  const out = new Uint8Array(32);
  let hash = 0x811c9dc5;
  for (const byte of data) {
    hash ^= byte;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  for (let index = 0; index < 4; index += 1) {
    out[index] = (hash >>> (index * 8)) & 0xff;
  }
  return out;
}

/**
 * 把一条 chunk 的 JSON 重新序列化，且**反转 body 内的键序**。
 *
 * 语义完全相同、字节完全不同：`JSON.parse` 再 `JSON.stringify` 会按插入序输出，
 * 因此这里显式构造一个键序颠倒的对象，使两次文本必然不同。
 */
function reserializeWithReorderedKeys(text: string): string {
  const parsed = JSON.parse(text) as Record<string, unknown>;
  const body = parsed["body"] as Record<string, unknown>;
  const reversed: Record<string, unknown> = {};
  for (const key of Object.keys(body).reverse()) reversed[key] = body[key];
  return JSON.stringify({ ...parsed, body: reversed });
}

// ── 1. device proof：真实编码 + 真实验签 ─────────────────────────────

describe("R11/AC3：device-proof 向量与生产 transcript 逐字节对拍", () => {
  beforeEach(() => {
    installFakeIndexedDB();
  });

  it("生产 transcript 端口编出的字节与向量逐字节相同，SHA-256 也相同", async () => {
    const input = inputOf(deviceProofVector);
    const production = await handshakeAndCaptureDeviceProof(input);

    // 与 registry 视图逐字节比对：先排除 base64 文本差异掩盖单字节偏移的可能。
    const asRegistry = encodeAsRegistryDomain(registryDomain(deviceProofVector.domain), deviceProofVector.input);
    expect(bytesToHex(production)).toBe(bytesToHex(asRegistry));
    expect(encodeBase64Url(production)).toBe(deviceProofVector.expected.transcriptBase64url);
    expect(await sha256Hex(production)).toBe(deviceProofVector.expected.transcriptSha256Hex);
  });

  it("向量自带的 p1363Signature（Rust 侧签的）能验过生产编出的字节", async () => {
    const input = inputOf(deviceProofVector);
    const production = await handshakeAndCaptureDeviceProof(input);
    const publicKey = await importSec1PublicKey(deviceProofVector.expected.publicKey);
    const signature = decodeBase64Url(deviceProofVector.expected.p1363Signature);

    // 决定性判据：Rust 签的那支签名能验过我们编出的字节。
    expect(await subtle.verify({ name: "ECDSA", hash: "SHA-256" }, publicKey, signature, production)).toBe(true);

    // 判别力：翻一个 bit，签名立刻验不过——因此上面的 `true` 不是恒真断言。
    const mutated = Uint8Array.from(production);
    mutated[mutated.length - 1] = (mutated[mutated.length - 1] ?? 0) ^ 0x01;
    expect(await subtle.verify({ name: "ECDSA", hash: "SHA-256" }, publicKey, signature, mutated)).toBe(false);
  });

  it("向量的私钥以不可导出形式导入后，真实 crypto.subtle 签出的 P1363 签名可被向量公钥验证", async () => {
    // 向量的私钥只作为**输入**导入：客户端实现从不在运行时接触它（生产路径用的是
    // `crypto.subtle.generateKey` 出来的不可导出句柄，见下一条断言）。
    const privateKey = await subtle.importKey(
      "jwk",
      deviceProofVector.expected.privateJwk,
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["sign"],
    );
    expect(privateKey.extractable).toBe(false);

    const transcript = encodeAsRegistryDomain(registryDomain(deviceProofVector.domain), deviceProofVector.input);
    const signature = await subtle.sign({ name: "ECDSA", hash: "SHA-256" }, privateKey, transcript);
    // P1363：64 字节 `r || s`，不是 DER。
    expect(signature.byteLength).toBe(64);

    const publicKey = await importSec1PublicKey(deviceProofVector.expected.publicKey);
    expect(
      await subtle.verify({ name: "ECDSA", hash: "SHA-256" }, publicKey, signature, transcript),
    ).toBe(true);
  });

  it("真实设备身份端口用不可导出密钥签名，且导出被 crypto.subtle 拒绝", async () => {
    const store = openDeviceIdentityStore();
    await store.ensureIdentity({
      deviceId: String(deviceProofVector.input["deviceId"]),
      canonicalOrigin: String(deviceProofVector.input["canonicalOrigin"]),
    });
    const transcript = encodeAsRegistryDomain(registryDomain(deviceProofVector.domain), deviceProofVector.input);
    const signature = await store.signDeviceProof(transcript);

    expect(signature).toHaveLength(64);
    // 生成的密钥与向量里的密钥无关，因此这里断言的是「自洽」：用它自己的公钥验它自己的签名。
    const identity = await store.loadIdentity();
    const publicKey = await importSec1PublicKey(identity?.publicKeyBase64Url ?? "");
    expect(
      await subtle.verify({ name: "ECDSA", hash: "SHA-256" }, publicKey, signature, transcript),
    ).toBe(true);

    // 反例：若 `signDeviceProof` 导出私钥或用可导出句柄，这里会红。
    expect(identity?.privateKey.extractable).toBe(false);
    await expect(subtle.exportKey("pkcs8", identity?.privateKey as CryptoKey)).rejects.toThrow();
  });
});

// ── 2. host challenge：真实 HostIdentityPort 验真实签名 ────────────────

describe("R11/AC3：host-challenge 向量经真实 HostIdentityPort 验签", () => {
  beforeEach(() => {
    installFakeIndexedDB();
  });

  it("真实 PairedHostIdentity 接受向量签名，握手继续到 clientProof", async () => {
    const input = inputOf(hostChallengeVector);
    const harness = await setup({ ...input, hostPublicKeyBase64Url: hostChallengeVector.expected.publicKey });
    try {
      const socket = harness.sockets.latest;
      socket.open();
      await waitFor(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
      socket.deliver(serverChallenge(input, hostChallengeVector.expected.p1363Signature));

      // 反例：若生产 transcript 与向量差一个字节，真实验签失败 → 阻断且不发 clientProof。
      await waitFor(() => socket.types().includes("auth.client_proof"), "真实验签通过并签发 clientProof");
      expect(harness.store.state.connection.blocking).toBeNull();
    } finally {
      harness.composition.dispose();
    }
  });

  it("同一向量换一个字节即被真实验签拒绝并进入 identity_changed", async () => {
    const input = inputOf(hostChallengeVector);
    // 保持 hostId/公钥不变，把 serverNonce 换成向量之外的值：transcript 必然不同。
    const tampered: HandshakeInput = { ...input, serverNonceHex: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff" };
    const harness = await setup({ ...tampered, hostPublicKeyBase64Url: hostChallengeVector.expected.publicKey });
    try {
      const socket = harness.sockets.latest;
      socket.open();
      await waitFor(() => socket.types().includes("auth.client_hello"), "发出 clientHello");
      socket.deliver(serverChallenge(tampered, hostChallengeVector.expected.p1363Signature));

      await waitFor(() => harness.store.state.connection.blocking !== null, "字节不符进入阻断态");
      expect(harness.store.state.connection.state).toBe("identity_changed");
      // 反例：若验签被跳过，这里会出现 clientProof → 断言变红。
      expect(socket.types()).not.toContain("auth.client_proof");
      expect(harness.signed).toHaveLength(0);
    } finally {
      harness.composition.dispose();
    }
  });

  it("向量自洽：registry 视图的字节与其 base64url / SHA-256 一致，features 已按 UTF-8 升序", () => {
    const encoded = encodeAsRegistryDomain(registryDomain(hostChallengeVector.domain), hostChallengeVector.input);
    expect(encodeBase64Url(encoded)).toBe(hostChallengeVector.expected.transcriptBase64url);

    const domain = registryDomain(hostChallengeVector.domain);
    const view = new DataView(encoded.buffer, encoded.byteOffset, encoded.byteLength);
    expect(new TextDecoder().decode(encoded.subarray(0, 4))).toBe("ACPR");
    expect(encoded[4]).toBe(1);
    const domainLength = view.getUint16(5, false);
    expect(new TextDecoder().decode(encoded.subarray(7, 7 + domainLength))).toBe(hostChallengeVector.domain);
    expect(view.getUint16(7 + domainLength, false)).toBe(domain.fields.length);

    // 未排序的 features 会被 `encodeNulJoinedUtf8` 硬拒绝（§5.2 的守卫，不是约定）。
    const features = hostChallengeVector.input["negotiatedFeatures"] as readonly string[];
    expect(sortedFeatures(features)).toEqual([...features]);
  });
});

// ── 3. 真实 digestPort 参与快照校验 ──────────────────────────────────

/**
 * 用**独立实现**（Node `crypto.createHash`）复算 §9.4 的两级快照摘要。
 *
 * 期望值不经过被测的 `digestPort`：真实端口若被换成 FNV-1a，本函数算出的值不会变，
 * 于是快照校验必然失败——这就是「真实 digestPort 参与校验」的判别力来源。
 */
function independentSnapshotDigest(rawTexts: readonly string[]): string {
  const joined = new Uint8Array(new ArrayBuffer(rawTexts.length * 32));
  rawTexts.forEach((text, index) => {
    joined.set(new Uint8Array(createHash("sha256").update(text, "utf8").digest()), index * 32);
  });
  return createHash("sha256").update(joined).digest("base64url");
}

/** 三 chunk 快照的原始 wire 文本（digest 按原始字节计算）。 */
function snapshotTexts(snapshotId: string, title: string): readonly string[] {
  const chunk = (chunkIndex: string, resource: string, items: readonly unknown[]): string =>
    JSON.stringify({
      protocolVersion: 1,
      type: "sync.snapshot_chunk",
      messageId: "aaaaaaaa-1111-4111-8111-000000000002",
      connectionId: CONNECTION_ID,
      connectionSequence: "1",
      body: { snapshotId, chunkIndex, resource, items },
    });
  return [
    chunk("0", "sessions", [
      {
        sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
        title,
        agent: { agentId: "claude", name: "Claude" },
        state: "idle",
        origin: { kind: "local" },
        currentMode: null,
        version: "1",
        createdAt: "2026-10-01T00:00:00.000Z",
        updatedAt: "2026-10-01T00:00:00.000Z",
        workspace: { alias: "work-api", displayName: "api" },
      },
    ]),
    chunk("1", "workspaces", [{ alias: "work-api", displayName: "api" }]),
    chunk("2", "agents", [{ agentId: "codex-a", displayName: "codex", default: true }]),
  ];
}

function snapshotBegin(snapshotId: string, cursor: Readonly<CursorShape>): unknown {
  return {
    protocolVersion: 1,
    type: "sync.snapshot_begin",
    messageId: "aaaaaaaa-1111-4111-8111-000000000001",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: { snapshotId, cursor, schemaVersion: 1, chunkCount: 3 },
  };
}

function snapshotEnd(snapshotId: string, cursor: Readonly<CursorShape>, digest: string): unknown {
  return {
    protocolVersion: 1,
    type: "sync.snapshot_end",
    messageId: "aaaaaaaa-1111-4111-8111-000000000003",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: { snapshotId, cursor, chunkCount: 3, snapshotDigest: digest },
  };
}

/** 本文件用到的游标形状。 */
interface CursorShape {
  readonly serverEpoch: string;
  readonly globalSequence: string;
}

describe("R14/R11：真实 digestPort 参与快照校验（既有测试全部用 FNV 替身）", () => {
  beforeEach(() => {
    installFakeIndexedDB();
  });

  it("摘要不符的快照不入仓；同一 wire 配上独立实现算出的 SHA-256 摘要则入仓", async () => {
    const input = inputOf(hostChallengeVector);
    const harness = await setup({ ...input, hostPublicKeyBase64Url: hostChallengeVector.expected.publicKey });
    try {
      const socket = socketOf(harness);
      await authenticate(harness, input, hostChallengeVector.expected.p1363Signature);

      const cursor: CursorShape = { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" };

      // 第一份：wire 完全合法，但摘要取一个明显错误的值。
      const bad = snapshotTexts("8194de43-e213-423d-acf4-2e3549304566", "错误摘要的会话");
      socket.deliver(snapshotBegin("8194de43-e213-423d-acf4-2e3549304566", cursor));
      for (const text of bad) socket.deliver(text);
      socket.deliver(snapshotEnd("8194de43-e213-423d-acf4-2e3549304566", cursor, "0".repeat(43)));
      // 让出足够多的宏任务轮次，使真实 `crypto.subtle.digest` 的 promise 有机会 resolve：
      // 摘要不符时不得有任何东西入仓（bad 的结构、cursor、chunkCount 全部合法，
      // 因此「未入仓」只能由 digest 判定失败解释）。
      await settle();
      expect(harness.store.state.resources).toBeNull();
      expect(harness.store.state.lastSyncedAtMs).toBeNull();



      // 第二份：结构相同，摘要由独立实现算出 → 真实 digestPort 校验通过并入仓。
      const good = snapshotTexts("1c1a0f6e-4b39-4a1f-9d2e-7f2c5b8a1d40", "真实 SHA-256 的会话");
      socket.deliver(snapshotBegin("1c1a0f6e-4b39-4a1f-9d2e-7f2c5b8a1d40", cursor));
      for (const text of good) socket.deliver(text);
      socket.deliver(snapshotEnd("1c1a0f6e-4b39-4a1f-9d2e-7f2c5b8a1d40", cursor, independentSnapshotDigest(good)));

      await waitFor(() => harness.store.state.resources !== null, "正确摘要的快照入仓");
      expect(harness.store.state.resources?.sessions.map((item) => item.title)).toEqual(["真实 SHA-256 的会话"]);
      expect(harness.store.state.resources?.agents).toHaveLength(1);
    } finally {
      harness.composition.dispose();
    }
  });

  it("真实 digestPort 拒收 FNV 摘要：把 fakeDigest 的算法当期望值必然入不了仓", async () => {
    const input = inputOf(hostChallengeVector);
    const harness = await setup({ ...input, hostPublicKeyBase64Url: hostChallengeVector.expected.publicKey });
    try {
      await authenticate(harness, input, hostChallengeVector.expected.p1363Signature);
      const cursor: CursorShape = { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" };
      const texts = snapshotTexts("7d1e0b6a-9c2f-4c85-b0a3-2f5d8e91c3aa", "FNV 摘要的会话");

      // 期望值改用 `fakeDigest` 的 FNV-1a 派生值：既有测试全用它，因此若真实端口被换成
      // 替身，这一条会绿；而它现在必须红——真实端口是 SHA-256。
      socketOf(harness).deliver(snapshotBegin("7d1e0b6a-9c2f-4c85-b0a3-2f5d8e91c3aa", cursor));
      for (const text of texts) socketOf(harness).deliver(text);
      socketOf(harness).deliver(
        snapshotEnd("7d1e0b6a-9c2f-4c85-b0a3-2f5d8e91c3aa", cursor, fnvTwoLevelDigest(texts)),
      );
      await settle();

      expect(harness.store.state.resources).toBeNull();
    } finally {
      harness.composition.dispose();
    }
  });

  it("真实 digestPort 的摘要跟着**送来的原始字节**走，而不是重新序列化后的规范形", async () => {
    const input = inputOf(hostChallengeVector);
    const harness = await setup({ ...input, hostPublicKeyBase64Url: hostChallengeVector.expected.publicKey });
    try {
      await authenticate(harness, input, hostChallengeVector.expected.p1363Signature);
      const snapshotId = "b3d5f7a1-2c4e-4a6b-8d0f-1e2a3b4c5d6e";
      const cursor: CursorShape = { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" };
      const texts = snapshotTexts(snapshotId, "键序对照");
      const reordered = texts.map(reserializeWithReorderedKeys);
      // 前提：两者语义相同、字节不同（否则本用例什么也证明不了）。
      expect(reordered).not.toEqual(texts);
      expect(reordered.map((text) => JSON.parse(text))).toEqual(texts.map((text) => JSON.parse(text)));

      // 第一轮：送**键序颠倒**的字节，摘要也按这批字节算 → 必须被采纳。
      // 反例：若实现在算摘要前把消息重新序列化成规范形，算出的会等于另一批字节 → 这里会红。
      socketOf(harness).deliver(snapshotBegin(snapshotId, cursor));
      for (const text of reordered) socketOf(harness).deliver(text);
      socketOf(harness).deliver(snapshotEnd(snapshotId, cursor, independentSnapshotDigest(reordered)));
      await waitFor(() => harness.store.state.resources !== null, "按送达字节算的摘要被采纳");
      expect(harness.store.state.resources?.sessions.map((item) => item.title)).toEqual(["键序对照"]);

      // 第二轮：送原始字节，却沿用上一轮的摘要 → 必须被拒，且**不覆盖**已完成状态。
      const rejectedSnapshotId = "c4e6a8b2-3d5f-4b7c-9e1a-2f3b4c5d6e7f";
      const other = snapshotTexts(rejectedSnapshotId, "不该出现的会话");
      socketOf(harness).deliver(snapshotBegin(rejectedSnapshotId, cursor));
      for (const text of other) socketOf(harness).deliver(text);
      socketOf(harness).deliver(snapshotEnd(rejectedSnapshotId, cursor, independentSnapshotDigest(reordered)));
      await settle();
      // 反例：若实现按语义（重新序列化后）比对摘要，这一轮会被采纳 → 断言变红。
      expect(harness.store.state.resources?.sessions.map((item) => item.title)).toEqual(["键序对照"]);
    } finally {
      harness.composition.dispose();
    }
  });
});