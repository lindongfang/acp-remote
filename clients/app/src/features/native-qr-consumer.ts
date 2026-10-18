/**
 * 未来原生扫码消费方的**纯协议/策略**模块（design D6、`specs/sync-pairing-http` 的「扫码消费方」）。
 *
 * ## 这不是原生 App
 *
 * App 相机、深链、keystore 与真机扫码**都未实现**，本变更也不交付它们。这里只有可在 Node 测试
 * 里用伪时钟 + 模拟传输验证的共享合同：短二维码解码、完整 65 字节公钥指纹校验、来源绑定与
 * 有限重试策略。PWA 页面与业务 store **不得** import 本模块——浏览器不使用该取回接口，
 * 也不持有扫码取回凭据（`src/features/native-qr-consumer.test.ts` 对这条边界有源码级断言）。
 *
 * ## 安全边界（缺一不可）
 *
 * - 扫码地址必须是 `https://<canonical-origin>/pair#q2=<65-byte payload>`；取回请求只发往
 *   该 canonical origin，响应里的 `canonicalOrigin` 必须与它**逐字相同**。
 * - HTTP 重定向一律失败：不得根据 `Location` 改目标来源。
 * - 主机公钥必须是合法的 65 字节未压缩 P-256 点；随后比较 QR 里的**完整** SHA-256 指纹。
 * - 指纹匹配只绑定主机公钥；`hostProof`、批准响应身份一致性与 WSS 认证仍沿用既有规则
 *   （由调用方执行，本模块不代替它们）。
 *
 * ## 有限重试（design D6）
 *
 * 断网、超时与 502/503/504 最多尝试 3 次，总墙钟 30 秒（含请求超时与等待），单请求最长
 * 8 秒；普通退避 500ms / 1000ms。429 按 `Retry-After` 等待，超出剩余预算立即停止；已知
 * `expiresAt` 之后停止且不续期。认证、格式、来源、指纹错误立即停止。取回响应丢失时重试
 * **同一次取回**；claim 响应丢失时由 {@link retryClaim} 复用**同一份** claim 正文，
 * 绝不回头重新取回材料或生成第二份设备身份。
 */

import { parseDevicePairingUrl } from "../protocol";
import type { DevicePairingUrl } from "../protocol";

/** 一次扫码消费的固定上限（design D6 的明确上界；原生交付前可另行调整）。 */
export const QR_CONSUMER_POLICY = {
  /** 同一请求最多尝试次数（含第一次）。 */
  maxAttempts: 3,
  /** 总墙钟预算：请求超时与退避等待都算在内。 */
  totalBudgetMs: 30_000,
  /** 单次请求最长等待时间。 */
  requestTimeoutMs: 8_000,
  /** 普通暂时故障的退避（第 1 次失败后、第 2 次失败后）。 */
  backoffMs: [500, 1_000],
  /** 可重试的 HTTP 暂时故障状态码。 */
  retryableStatuses: [502, 503, 504],
} as const;

/** 策略停止原因。错误类别不泄露服务端具体差异（服务端统一返回 `pairing.qr_invalid`）。 */
export type QrConsumerStopReason =
  /** 扫码地址本身不是合法的设备短二维码。 */
  | "invalid_qr"
  | "attempts_exhausted"
  | "budget_exhausted"
  | "expired"
  | "rate_limited"
  | "redirected"
  | "unauthorized"
  | "rejected"
  | "invalid_public_key"
  | "fingerprint_mismatch"
  | "origin_mismatch"
  | "invalid_response";

/** 可注入时钟：测试用伪时钟证明预算与退避，生产实现由原生运行时提供。 */
export interface QrConsumerClock {
  now(): number;
  sleep(ms: number): Promise<void>;
}

/**
 * 一次 HTTPS 请求的结果。
 *
 * `redirect` 由传输层在收到 3xx 时给出（实现必须使用 `redirect: "manual"` 语义，不发第二次
 * 请求）；`timeout` 表示传输层在 `timeoutMs` 内没有完成；不要用 `response` 表示这两种情况。
 */
export type QrTransportResult =
  | {
      readonly kind: "response";
      readonly status: number;
      /** `Retry-After` 的秒数；缺失或不可解析为 `null`。 */
      readonly retryAfterSeconds: number | null;
      readonly body: unknown;
    }
  | { readonly kind: "redirect" }
  | { readonly kind: "network_error" }
  | { readonly kind: "timeout" };

/** 扫码取回请求（`POST /sync/v1/pairing/qr-lookup`，§7.6）。 */
export interface QrLookupTransport {
  qrLookup(input: {
    readonly canonicalOrigin: string;
    readonly qrToken: Uint8Array;
    readonly timeoutMs: number;
  }): Promise<QrTransportResult>;
}

/** claim 请求；`claimBody` 是不可变正文，重试必须原样复用。 */
export interface ClaimTransport {
  claim(input: {
    readonly canonicalOrigin: string;
    readonly claimBody: string;
    readonly timeoutMs: number;
  }): Promise<QrTransportResult>;
}

/** 取回成功、且已通过公钥指纹与来源绑定的配对材料。 */
export interface DevicePairingMaterial {
  /** 固定自扫码地址的 canonical origin；之后所有 HTTPS 请求与 WSS 必须使用它。 */
  readonly canonicalOrigin: string;
  readonly pairingId: string;
  readonly hostId: string;
  /** 65 字节未压缩 P-256 公钥（原始字节，不是指纹）。 */
  readonly hostPublicKey: Uint8Array;
  /** 32 字节配对秘密；只进入私有会话状态。 */
  readonly pairingSecret: Uint8Array;
  readonly expiresAtMs: number;
}

/** 取回结果。 */
export type QrConsumerOutcome =
  | { readonly kind: "material"; readonly material: DevicePairingMaterial; readonly attempts: number }
  | { readonly kind: "stopped"; readonly reason: QrConsumerStopReason; readonly attempts: number };

/** claim 结果；正文交给调用方的既有 claim/status 校验。 */
export type ClaimOutcome =
  | { readonly kind: "accepted"; readonly body: unknown; readonly attempts: number }
  | { readonly kind: "stopped"; readonly reason: QrConsumerStopReason; readonly attempts: number };

/** 单次尝试的结论。 */
type RetryStep<T> =
  | { readonly kind: "value"; readonly value: T }
  | {
      readonly kind: "transient";
      readonly retryAfterSeconds: number | null;
      /** 暂时故障来自 429：等待时长必须取自 `Retry-After`。 */
      readonly rateLimited: boolean;
    }
  | { readonly kind: "stop"; readonly reason: QrConsumerStopReason };

type RetryOutcome<T> =
  | { readonly kind: "value"; readonly value: T; readonly attempts: number }
  | { readonly kind: "stopped"; readonly reason: QrConsumerStopReason; readonly attempts: number };

/** 未压缩点前缀（SEC1）。 */
const UNCOMPRESSED_POINT_PREFIX = 0x04;
/** 完整 P-256 公钥的字节长度（与 core / 协议模块一致）。 */
const PUBLIC_KEY_BYTES = 65;
/** 配对秘密与指纹的字节长度。 */
const SECRET_BYTES = 32;

/** P-256 质数域与曲线参数（只有校验用，不新增密码算法）。 */
const P256_P = 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffffn;
const P256_B = 0x5ac635d8aa3a93e7b3ebbd55769886bc651d06b0cc53b0f63bce3c3e27d2604bn;

const BASE64URL_ALPHABET =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const BASE64URL_PATTERN = /^[A-Za-z0-9_-]+$/;
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/**
 * 规范无填充 base64url 编码（与 `src/protocol/device-qr.ts` 同一算法；那份实现不导出内部函数）。
 */
function encodeBase64Url(bytes: Uint8Array): string {
  let out = "";
  for (let index = 0; index < bytes.length; index += 3) {
    const first = bytes[index] ?? 0;
    const second = bytes[index + 1];
    const third = bytes[index + 2];
    const triple = (first << 16) | ((second ?? 0) << 8) | (third ?? 0);
    out += BASE64URL_ALPHABET.charAt((triple >> 18) & 0x3f);
    out += BASE64URL_ALPHABET.charAt((triple >> 12) & 0x3f);
    if (second !== undefined) out += BASE64URL_ALPHABET.charAt((triple >> 6) & 0x3f);
    if (third !== undefined) out += BASE64URL_ALPHABET.charAt(triple & 0x3f);
  }
  return out;
}

/** 严格解码：拒绝填充、非法字符、非规范尾位；非法返回 `null`（不抛异常、不回显文本）。 */
function decodeBase64Url(text: string): Uint8Array | null {
  if (text.length === 0 || text.length % 4 === 1 || !BASE64URL_PATTERN.test(text)) return null;
  const bytes: number[] = [];
  let buffer = 0;
  let bits = 0;
  for (const character of text) {
    const value = BASE64URL_ALPHABET.indexOf(character);
    if (value < 0) return null;
    buffer = (buffer << 6) | value;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      bytes.push((buffer >> bits) & 0xff);
      buffer &= (1 << bits) - 1;
    }
  }
  const decoded = Uint8Array.from(bytes);
  return encodeBase64Url(decoded) === text ? decoded : null;
}

function bigintOf(bytes: Uint8Array, start: number, end: number): bigint {
  let value = 0n;
  for (let index = start; index < end; index += 1) {
    value = (value << 8n) | BigInt(bytes[index] ?? 0);
  }
  return value;
}

function mod(value: bigint, modulus: bigint): bigint {
  const remainder = value % modulus;
  return remainder < 0n ? remainder + modulus : remainder;
}

/**
 * 65 字节未压缩 P-256 公钥的合法性：长度、`0x04` 前缀、坐标在域内且满足曲线方程。
 *
 * 这是**校验**而不是新密码算法：指纹只对合法的完整公钥有意义，非法点必须在此之前失败。
 */
export function isValidP256PublicKey(bytes: Uint8Array): boolean {
  if (bytes.length !== PUBLIC_KEY_BYTES || (bytes[0] ?? 0) !== UNCOMPRESSED_POINT_PREFIX) return false;
  const x = bigintOf(bytes, 1, 33);
  const y = bigintOf(bytes, 33, 65);
  if (x >= P256_P || y >= P256_P) return false;
  const left = mod(y * y, P256_P);
  const right = mod(x * x * x - 3n * x + P256_B, P256_P);
  return left === right;
}

/** 逐字节比较（长度不同直接失败；不做提前返回，避免把指纹比较变成旁路）。 */
function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.length !== right.length) return false;
  let difference = 0;
  for (let index = 0; index < left.length; index += 1) {
    difference |= (left[index] ?? 0) ^ (right[index] ?? 0);
  }
  return difference === 0;
}

/**
 * WebCrypto SHA-256 的默认实现（Node 与浏览器可用）。
 *
 * 原生运行时没有 `crypto.subtle` 时由调用方注入自己的摘要实现；本模块不伪造平台能力。
 */
export async function webCryptoSha256(bytes: Uint8Array): Promise<Uint8Array> {
  // `crypto.subtle.digest` 只接受绑定在 `ArrayBuffer` 上的视图；`Uint8Array` 在类型层可能指向
  // `SharedArrayBuffer`，因此显式复制成一份普通缓冲区，而不是把类型断言塞进密码调用。
  const digest = await globalThis.crypto.subtle.digest("SHA-256", new Uint8Array(bytes));
  return new Uint8Array(digest);
}

function stop<T>(reason: QrConsumerStopReason): RetryStep<T> {
  return { kind: "stop", reason };
}

function transient<T>(retryAfterSeconds: number | null, rateLimited: boolean): RetryStep<T> {
  return { kind: "transient", retryAfterSeconds, rateLimited };
}

/**
 * 统一的重试执行器：次数、总预算、单请求超时、退避、429 与已知 `expiresAt` 都在这里收口。
 *
 * `attempt` 必须遵守传入的 `timeoutMs`；返回 `transient` 表示可重试，返回 `stop` 表示立即停止。
 */
async function runWithLimitedRetry<T>(input: {
  readonly clock: QrConsumerClock;
  readonly expiresAtMs: number | null;
  readonly attempt: (context: {
    readonly attempt: number;
    readonly timeoutMs: number;
  }) => Promise<RetryStep<T>>;
}): Promise<RetryOutcome<T>> {
  const deadlineMs = input.clock.now() + QR_CONSUMER_POLICY.totalBudgetMs;
  let attempts = 0;
  while (attempts < QR_CONSUMER_POLICY.maxAttempts) {
    const now = input.clock.now();
    if (input.expiresAtMs !== null && now >= input.expiresAtMs) {
      return { kind: "stopped", reason: "expired", attempts };
    }
    const remainingMs = deadlineMs - now;
    if (remainingMs <= 0) {
      return { kind: "stopped", reason: "budget_exhausted", attempts };
    }
    attempts += 1;
    const timeoutMs = Math.min(QR_CONSUMER_POLICY.requestTimeoutMs, remainingMs);
    const step = await input.attempt({ attempt: attempts, timeoutMs });
    if (step.kind === "value") return { kind: "value", value: step.value, attempts };
    if (step.kind === "stop") return { kind: "stopped", reason: step.reason, attempts };
    if (attempts >= QR_CONSUMER_POLICY.maxAttempts) {
      return {
        kind: "stopped",
        reason: input.clock.now() >= deadlineMs ? "budget_exhausted" : "attempts_exhausted",
        attempts,
      };
    }
    if (step.rateLimited && step.retryAfterSeconds === null) {
      // 429 没有可执行的 `Retry-After`：不能猜测时长，立即停止。
      return { kind: "stopped", reason: "rate_limited", attempts };
    }
    const waitMs =
      step.retryAfterSeconds === null
        ? (QR_CONSUMER_POLICY.backoffMs[attempts - 1] ??
          QR_CONSUMER_POLICY.backoffMs[QR_CONSUMER_POLICY.backoffMs.length - 1] ??
          0)
        : step.retryAfterSeconds * 1_000;
    const afterWaitMs = input.clock.now() + waitMs;
    if (afterWaitMs >= deadlineMs) {
      return {
        kind: "stopped",
        reason: step.rateLimited ? "rate_limited" : "budget_exhausted",
        attempts,
      };
    }
    if (input.expiresAtMs !== null && afterWaitMs >= input.expiresAtMs) {
      return { kind: "stopped", reason: "expired", attempts };
    }
    await input.clock.sleep(waitMs);
  }
  return { kind: "stopped", reason: "attempts_exhausted", attempts };
}

/** 传输结果 → 重试步骤（取回）。2xx 由调用方交给正文校验。 */
function classifyLookupTransport(outcome: QrTransportResult): RetryStep<never> {
  switch (outcome.kind) {
    case "redirect":
      return stop("redirected");
    case "network_error":
    case "timeout":
      return transient(null, false);
    case "response": {
      if (outcome.status === 429) {
        const retryAfter = outcome.retryAfterSeconds;
        return transient(
          typeof retryAfter === "number" && Number.isFinite(retryAfter) && retryAfter >= 0
            ? Math.ceil(retryAfter)
            : null,
          true,
        );
      }
      if (outcome.status === 502 || outcome.status === 503 || outcome.status === 504) {
        return transient(null, false);
      }
      if (outcome.status === 401 || outcome.status === 403) return stop("unauthorized");
      return stop("rejected");
    }
  }
}

/**
 * 取回响应正文 → 经过指纹与来源校验的材料。
 *
 * 顺序即安全顺序：合法公钥 → 完整指纹 → canonical origin → 其余绑定与有效期。
 */
async function materialFromBody(input: {
  readonly body: unknown;
  readonly paired: DevicePairingUrl;
  readonly nowMs: number;
  readonly sha256: (bytes: Uint8Array) => Promise<Uint8Array>;
}): Promise<RetryStep<DevicePairingMaterial>> {
  const body = input.body;
  if (typeof body !== "object" || body === null) return stop("invalid_response");
  const fields = body as Record<string, unknown>;
  if (fields["protocolVersion"] !== 1) return stop("invalid_response");

  const hostPublicKeyText = fields["hostPublicKey"];
  if (typeof hostPublicKeyText !== "string") return stop("invalid_response");
  const hostPublicKey = decodeBase64Url(hostPublicKeyText);
  if (hostPublicKey === null || hostPublicKey.length !== PUBLIC_KEY_BYTES) {
    return stop("invalid_public_key");
  }
  if (!isValidP256PublicKey(hostPublicKey)) return stop("invalid_public_key");

  const fingerprint = await input.sha256(hostPublicKey);
  if (!equalBytes(fingerprint, input.paired.payload.hostKeyFingerprint)) {
    return stop("fingerprint_mismatch");
  }

  if (fields["canonicalOrigin"] !== input.paired.canonicalOrigin) {
    return stop("origin_mismatch");
  }

  const pairingId = fields["pairingId"];
  const hostId = fields["hostId"];
  const pairingSecretText = fields["pairingSecret"];
  const expiresAtText = fields["expiresAt"];
  if (
    typeof pairingId !== "string" ||
    !UUID_PATTERN.test(pairingId) ||
    typeof hostId !== "string" ||
    !UUID_PATTERN.test(hostId) ||
    typeof pairingSecretText !== "string" ||
    typeof expiresAtText !== "string"
  ) {
    return stop("invalid_response");
  }
  const pairingSecret = decodeBase64Url(pairingSecretText);
  if (pairingSecret === null || pairingSecret.length !== SECRET_BYTES) {
    return stop("invalid_response");
  }
  const expiresAtMs = Date.parse(expiresAtText);
  if (Number.isNaN(expiresAtMs)) return stop("invalid_response");
  if (expiresAtMs <= input.nowMs) return stop("expired");

  return {
    kind: "value",
    value: {
      canonicalOrigin: input.paired.canonicalOrigin,
      pairingId,
      hostId,
      hostPublicKey,
      pairingSecret,
      expiresAtMs,
    },
  };
}

/**
 * 扫码取回一次设备配对材料（`POST /sync/v1/pairing/qr-lookup`）。
 *
 * 先解析并固定扫码地址的 canonical origin；之后只向该来源发请求，响应必须来自同一来源。
 * 成功返回材料后调用方应立即清除 URL 里的 `q2=` 片段，并把秘密只放进私有会话状态。
 */
export async function consumeDeviceQrReference(input: {
  readonly scannedUrl: string;
  readonly clock: QrConsumerClock;
  readonly transport: QrLookupTransport;
  readonly sha256: (bytes: Uint8Array) => Promise<Uint8Array>;
}): Promise<QrConsumerOutcome> {
  let paired: DevicePairingUrl;
  try {
    paired = parseDevicePairingUrl(input.scannedUrl);
  } catch {
    return { kind: "stopped", reason: "invalid_qr", attempts: 0 };
  }

  const outcome = await runWithLimitedRetry<DevicePairingMaterial>({
    clock: input.clock,
    expiresAtMs: null,
    attempt: async ({ timeoutMs }) => {
      const result = await input.transport.qrLookup({
        canonicalOrigin: paired.canonicalOrigin,
        qrToken: paired.payload.qrToken,
        timeoutMs,
      });
      if (result.kind !== "response" || result.status < 200 || result.status >= 300) {
        return classifyLookupTransport(result);
      }
      return materialFromBody({
        body: result.body,
        paired,
        nowMs: input.clock.now(),
        sha256: input.sha256,
      });
    },
  });
  return outcome.kind === "value"
    ? { kind: "material", material: outcome.value, attempts: outcome.attempts }
    : { kind: "stopped", reason: outcome.reason, attempts: outcome.attempts };
}

/** claim 传输结果 → 重试步骤。 */
function classifyClaimTransport(outcome: QrTransportResult): RetryStep<unknown> {
  switch (outcome.kind) {
    case "redirect":
      return stop("redirected");
    case "network_error":
    case "timeout":
      return transient(null, false);
    case "response": {
      if (outcome.status === 429) {
        const retryAfter = outcome.retryAfterSeconds;
        return transient(
          typeof retryAfter === "number" && Number.isFinite(retryAfter) && retryAfter >= 0
            ? Math.ceil(retryAfter)
            : null,
          true,
        );
      }
      if (outcome.status === 502 || outcome.status === 503 || outcome.status === 504) {
        return transient(null, false);
      }
      if (outcome.status >= 200 && outcome.status < 300) {
        return { kind: "value", value: outcome.body };
      }
      if (outcome.status === 401 || outcome.status === 403) return stop("unauthorized");
      return stop("rejected");
    }
  }
}

/**
 * claim 的有限重试：**原样复用**同一份 `claimBody`（同一 HMAC proof、同一 nonce）。
 *
 * 首次 claim 已被服务端接受但响应丢失时，重试必须按既有幂等规则返回首次结果；本函数因此
 * 不重新取回材料、不生成新 nonce，也不把 `claimBody` 交给任何日志。
 */
export async function retryClaim(input: {
  readonly canonicalOrigin: string;
  readonly claimBody: string;
  readonly expiresAtMs: number;
  readonly clock: QrConsumerClock;
  readonly transport: ClaimTransport;
}): Promise<ClaimOutcome> {
  const outcome = await runWithLimitedRetry<unknown>({
    clock: input.clock,
    expiresAtMs: input.expiresAtMs,
    attempt: async ({ timeoutMs }) => {
      const result = await input.transport.claim({
        canonicalOrigin: input.canonicalOrigin,
        claimBody: input.claimBody,
        timeoutMs,
      });
      return classifyClaimTransport(result);
    },
  });
  return outcome.kind === "value"
    ? { kind: "accepted", body: outcome.value, attempts: outcome.attempts }
    : { kind: "stopped", reason: outcome.reason, attempts: outcome.attempts };
}
