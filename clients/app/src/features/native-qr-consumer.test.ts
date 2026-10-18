/**
 * 未来原生扫码消费方的共享合同测试（design D6）。
 *
 * ## 判别力从哪来
 *
 * - 指纹用共享向量里的**真实** SHA-256：换掉主机公钥、截断/伪造指纹、或只比较前几字节都会红。
 * - 来源绑定与重定向：响应换来源、传输层报告 3xx 都必须立即停止，且不再发第二次请求。
 * - 有限重试：伪时钟证明 3 次上限、8 秒单请求与 30 秒总预算、500/1000ms 退避、429 的
 *   `Retry-After` 不被超预算执行，以及已知 `expiresAt` 后的停止。
 * - claim 响应丢失：重试的是**同一份** `claimBody`，不重新取回材料、不换 nonce。
 * - 边界是源码级的：PWA 页面、组件、组合根与业务 store 都不 import 本模块。
 *
 * **这不是原生 App 验收**：相机、keystore、真机扫码均未实现；这里只有伪时钟 + 模拟传输。
 */

import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import {
  buildDevicePairingUrl,
  DEVICE_QR_VERSION,
} from "../protocol";
import {
  QR_CONSUMER_POLICY,
  consumeDeviceQrReference,
  isValidP256PublicKey,
  retryClaim,
  webCryptoSha256,
} from "./native-qr-consumer";
import type {
  ClaimTransport,
  QrConsumerClock,
  QrLookupTransport,
  QrTransportResult,
} from "./native-qr-consumer";

/** 共享向量 `fixtures/sync/v1/device-qr/vectors.json` 的规范值。 */
const CANONICAL_ORIGIN = "https://faraday.tail74842e.ts.net:8765";
const QR_TOKEN_TEXT = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
const HOST_PUBLIC_KEY_TEXT =
  "BFJtc8UddTaNGZ6P9Dmvpr5IhKZbCihAwZ-K6LY-gJBqKIb_fCSFjJtFWkQbO-AGx0AXaZPhwTZbaa5qYnYv4eo";
const HOST_KEY_FINGERPRINT_TEXT = "USpZeYIpHWiVwqrdwRuVFov6KCpsWZfofCn__9Gcw3w";

const NOW_MS = Date.parse("2026-10-18T00:00:00.000Z");

const b64Url = (text: string): Uint8Array => new Uint8Array(Buffer.from(text, "base64url"));

const QR_TOKEN = b64Url(QR_TOKEN_TEXT);
const HOST_PUBLIC_KEY = b64Url(HOST_PUBLIC_KEY_TEXT);
const HOST_KEY_FINGERPRINT = b64Url(HOST_KEY_FINGERPRINT_TEXT);

const SCANNED_URL = buildDevicePairingUrl(CANONICAL_ORIGIN, {
  version: DEVICE_QR_VERSION,
  qrToken: QR_TOKEN,
  hostKeyFingerprint: HOST_KEY_FINGERPRINT,
});

/** 一份合法取回响应（字段与 `schemas/sync/v1/pairing.schema.json` 的 `qrLookupResponse` 同形）。 */
const VALID_BODY: Readonly<Record<string, unknown>> = {
  protocolVersion: 1,
  pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  hostId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  hostPublicKey: HOST_PUBLIC_KEY_TEXT,
  canonicalOrigin: CANONICAL_ORIGIN,
  pairingSecret: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
  expiresAt: "2026-10-18T00:10:00.000Z",
};

/** 伪时钟：`sleep` 推进时间并记账，测试因此不依赖真实等待。 */
class FakeClock implements QrConsumerClock {
  #nowMs: number;
  readonly sleeps: number[] = [];

  constructor(startMs: number) {
    this.#nowMs = startMs;
  }

  now(): number {
    return this.#nowMs;
  }

  async sleep(ms: number): Promise<void> {
    this.sleeps.push(ms);
    this.#nowMs += ms;
  }

  advance(ms: number): void {
    this.#nowMs += ms;
  }
}

/** 一次模拟请求：返回结果前可以推进时钟，模拟网络耗时或超时。 */
type TransportScript = (clock: FakeClock) => QrTransportResult;

class ScriptedLookupTransport implements QrLookupTransport {
  readonly calls: {
    canonicalOrigin: string;
    qrToken: Uint8Array;
    timeoutMs: number;
  }[] = [];
  readonly #clock: FakeClock;
  readonly #script: readonly TransportScript[];

  constructor(clock: FakeClock, script: readonly TransportScript[]) {
    this.#clock = clock;
    this.#script = script;
  }

  async qrLookup(input: {
    canonicalOrigin: string;
    qrToken: Uint8Array;
    timeoutMs: number;
  }): Promise<QrTransportResult> {
    this.calls.push({ ...input, qrToken: Uint8Array.from(input.qrToken) });
    const step = this.#script[Math.min(this.calls.length - 1, this.#script.length - 1)];
    if (step === undefined) throw new Error("测试脚本没有这一步");
    return step(this.#clock);
  }
}

/** 一次模拟 claim：记录收到的正文（断言重试原样复用）。 */
class ScriptedClaimTransport implements ClaimTransport {
  readonly calls: { canonicalOrigin: string; claimBody: string; timeoutMs: number }[] = [];
  readonly #clock: FakeClock;
  readonly #script: readonly TransportScript[];

  constructor(clock: FakeClock, script: readonly TransportScript[]) {
    this.#clock = clock;
    this.#script = script;
  }

  async claim(input: {
    canonicalOrigin: string;
    claimBody: string;
    timeoutMs: number;
  }): Promise<QrTransportResult> {
    this.calls.push(input);
    const step = this.#script[Math.min(this.calls.length - 1, this.#script.length - 1)];
    if (step === undefined) throw new Error("测试脚本没有这一步");
    return step(this.#clock);
  }
}

/** 一次成功响应；`spentMs` 模拟请求耗时。 */
function okResponse(
  body: unknown,
  spentMs = 10,
): TransportScript {
  return (clock) => {
    clock.advance(spentMs);
    return { kind: "response", status: 200, retryAfterSeconds: null, body };
  };
}

function statusResponse(status: number, retryAfterSeconds: number | null = null, spentMs = 10): TransportScript {
  return (clock) => {
    clock.advance(spentMs);
    return { kind: "response", status, retryAfterSeconds, body: { code: "pairing.qr_invalid" } };
  };
}

describe("设备短二维码取回：指纹与来源绑定", () => {
  it("规范向量：只向扫码地址请求，公钥、指纹与绑定材料全部通过", async () => {
    const clock = new FakeClock(NOW_MS);
    const transport = new ScriptedLookupTransport(clock, [okResponse(VALID_BODY)]);

    const outcome = await consumeDeviceQrReference({
      scannedUrl: SCANNED_URL,
      clock,
      transport,
      sha256: webCryptoSha256,
    });

    expect(outcome.kind).toBe("material");
    if (outcome.kind !== "material") return;
    expect(outcome.attempts).toBe(1);
    expect(outcome.material.canonicalOrigin).toBe(CANONICAL_ORIGIN);
    expect(outcome.material.pairingId).toBe(VALID_BODY["pairingId"]);
    expect(outcome.material.hostId).toBe(VALID_BODY["hostId"]);
    expect([...outcome.material.hostPublicKey]).toEqual([...HOST_PUBLIC_KEY]);
    expect(outcome.material.expiresAtMs).toBe(Date.parse("2026-10-18T00:10:00.000Z"));
    // 请求只发往固定来源，凭据在 body 里而不是 URL 里。
    expect(transport.calls).toHaveLength(1);
    expect(transport.calls[0]?.canonicalOrigin).toBe(CANONICAL_ORIGIN);
    expect([...(transport.calls[0]?.qrToken ?? new Uint8Array())]).toEqual([...QR_TOKEN]);
    expect(transport.calls[0]?.timeoutMs).toBe(QR_CONSUMER_POLICY.requestTimeoutMs);
  });

  it("真实 SHA-256 指纹与共享向量一致", async () => {
    const digest = await webCryptoSha256(HOST_PUBLIC_KEY);
    expect([...digest]).toEqual([...HOST_KEY_FINGERPRINT]);
  });

  it("非法公钥（非未压缩前缀 / 不在曲线上）先于指纹失败", async () => {
    expect(isValidP256PublicKey(HOST_PUBLIC_KEY)).toBe(true);
    expect(isValidP256PublicKey(new Uint8Array(65))).toBe(false);
    expect(isValidP256PublicKey(HOST_PUBLIC_KEY.slice(0, 64))).toBe(false);

    for (const hostPublicKey of [
      // 共享向量的 `non-uncompressed-prefix`（同一把公钥换成 0xA1 前缀）。
      "A1Jtc8UddTaNGZ6P9Dmvpr5IhKZbCihAwZ-K6LY-gJBqKIb_fCSFjJtFWkQbO-AGx0AXaZPhwTZbaa5qYnYv4eo",
      // `0x04 || 64 个零`：长度与前缀合法，但不在 P-256 曲线上。
      `04${"00".repeat(64)}`,
    ]) {
      const clock = new FakeClock(NOW_MS);
      const transport = new ScriptedLookupTransport(clock, [
        okResponse({ ...VALID_BODY, hostPublicKey }),
      ]);
      const outcome = await consumeDeviceQrReference({
        scannedUrl: SCANNED_URL,
        clock,
        transport,
        sha256: webCryptoSha256,
      });
      expect(outcome).toEqual({ kind: "stopped", reason: "invalid_public_key", attempts: 1 });
    }
  });

  it("指纹不符：立即停止，不重试", async () => {
    // 同一个公钥，换掉 QR 里固定的完整指纹。
    const mismatchedUrl = buildDevicePairingUrl(CANONICAL_ORIGIN, {
      version: DEVICE_QR_VERSION,
      qrToken: QR_TOKEN,
      hostKeyFingerprint: b64Url(QR_TOKEN_TEXT),
    });
    const clock = new FakeClock(NOW_MS);
    const transport = new ScriptedLookupTransport(clock, [okResponse(VALID_BODY)]);

    const outcome = await consumeDeviceQrReference({
      scannedUrl: mismatchedUrl,
      clock,
      transport,
      sha256: webCryptoSha256,
    });

    expect(outcome).toEqual({ kind: "stopped", reason: "fingerprint_mismatch", attempts: 1 });
    expect(transport.calls).toHaveLength(1);
    expect(clock.sleeps).toEqual([]);
  });

  it("响应来源与扫码来源不一致：立即停止，不 claim 也不重试", async () => {
    const clock = new FakeClock(NOW_MS);
    const transport = new ScriptedLookupTransport(clock, [
      okResponse({ ...VALID_BODY, canonicalOrigin: "https://other.example.test" }),
    ]);

    const outcome = await consumeDeviceQrReference({
      scannedUrl: SCANNED_URL,
      clock,
      transport,
      sha256: webCryptoSha256,
    });

    expect(outcome).toEqual({ kind: "stopped", reason: "origin_mismatch", attempts: 1 });
    expect(clock.sleeps).toEqual([]);
  });

  it("重定向与 HTTP 认证失败：直接失败，不跟随、不重试", async () => {
    const cases: readonly { readonly script: TransportScript; readonly reason: string }[] = [
      {
        script: (clock) => {
          clock.advance(10);
          return { kind: "redirect" };
        },
        reason: "redirected",
      },
      { script: statusResponse(401), reason: "unauthorized" },
      { script: statusResponse(400), reason: "rejected" },
    ];

    for (const scenario of cases) {
      const clock = new FakeClock(NOW_MS);
      const transport = new ScriptedLookupTransport(clock, [scenario.script]);
      const outcome = await consumeDeviceQrReference({
        scannedUrl: SCANNED_URL,
        clock,
        transport,
        sha256: webCryptoSha256,
      });
      expect(outcome.kind, scenario.reason).toBe("stopped");
      if (outcome.kind !== "stopped") continue;
      expect(outcome.reason, scenario.reason).toBe(scenario.reason);
      expect(outcome.attempts, scenario.reason).toBe(1);
      expect(transport.calls, scenario.reason).toHaveLength(1);
      expect(clock.sleeps, scenario.reason).toEqual([]);
    }
  });

  it("不是设备短二维码来源：不发任何请求", async () => {
    const clock = new FakeClock(NOW_MS);
    const transport = new ScriptedLookupTransport(clock, [okResponse(VALID_BODY)]);

    for (const scannedUrl of [
      `${CANONICAL_ORIGIN}/pair#data=eyJwYWlyaW5nUHJvdG9jb2wiOiJhY3AtcmVtb3RlLXBhaXJpbmctdjEifQ`,
      `${CANONICAL_ORIGIN}/pair`,
      `http://${CANONICAL_ORIGIN.slice("https://".length)}/pair#q2=AgAB`,
    ]) {
      const outcome = await consumeDeviceQrReference({
        scannedUrl,
        clock,
        transport,
        sha256: webCryptoSha256,
      });
      expect(outcome, scannedUrl).toEqual({ kind: "stopped", reason: "invalid_qr", attempts: 0 });
    }
    expect(transport.calls).toEqual([]);
  });
});

describe("设备短二维码取回：有限重试与预算", () => {
  it("断网/超时：最多 3 次、退避 500/1000ms，预算内停止", async () => {
    const clock = new FakeClock(NOW_MS);
    const transport = new ScriptedLookupTransport(clock, [
      (c) => {
        c.advance(QR_CONSUMER_POLICY.requestTimeoutMs);
        return { kind: "timeout" };
      },
    ]);

    const outcome = await consumeDeviceQrReference({
      scannedUrl: SCANNED_URL,
      clock,
      transport,
      sha256: webCryptoSha256,
    });

    expect(outcome).toEqual({ kind: "stopped", reason: "attempts_exhausted", attempts: 3 });
    expect(transport.calls).toHaveLength(3);
    // 每次请求都拿满 8 秒上限；等待是 500ms、1000ms，总共 25.5 秒，仍在 30 秒预算内。
    expect(transport.calls.map((call) => call.timeoutMs)).toEqual([8_000, 8_000, 8_000]);
    expect(clock.sleeps).toEqual([500, 1_000]);
    expect(clock.now() - NOW_MS).toBe(25_500);
  });

  it("502/503/504 可重试，其它 5xx 不重试", async () => {
    for (const status of QR_CONSUMER_POLICY.retryableStatuses) {
      const clock = new FakeClock(NOW_MS);
      const transport = new ScriptedLookupTransport(clock, [
        statusResponse(status),
        okResponse(VALID_BODY),
      ]);
      const outcome = await consumeDeviceQrReference({
        scannedUrl: SCANNED_URL,
        clock,
        transport,
        sha256: webCryptoSha256,
      });
      expect(outcome.kind, String(status)).toBe("material");
      expect(transport.calls, String(status)).toHaveLength(2);
      expect(clock.sleeps, String(status)).toEqual([500]);
    }

    const clock = new FakeClock(NOW_MS);
    const transport = new ScriptedLookupTransport(clock, [statusResponse(500)]);
    const outcome = await consumeDeviceQrReference({
      scannedUrl: SCANNED_URL,
      clock,
      transport,
      sha256: webCryptoSha256,
    });
    expect(outcome).toEqual({ kind: "stopped", reason: "rejected", attempts: 1 });
  });

  it("429：按 Retry-After 等待；超出剩余预算立即停止且不睡过预算", async () => {
    const clock = new FakeClock(NOW_MS);
    const transport = new ScriptedLookupTransport(clock, [
      statusResponse(429, 2),
      okResponse(VALID_BODY),
    ]);
    const outcome = await consumeDeviceQrReference({
      scannedUrl: SCANNED_URL,
      clock,
      transport,
      sha256: webCryptoSha256,
    });
    expect(outcome.kind).toBe("material");
    expect(clock.sleeps).toEqual([2_000]);

    const overBudgetClock = new FakeClock(NOW_MS);
    const overBudget = new ScriptedLookupTransport(overBudgetClock, [
      statusResponse(429, 45),
    ]);
    const stopped = await consumeDeviceQrReference({
      scannedUrl: SCANNED_URL,
      clock: overBudgetClock,
      transport: overBudget,
      sha256: webCryptoSha256,
    });
    expect(stopped).toEqual({ kind: "stopped", reason: "rate_limited", attempts: 1 });
    expect(overBudget.calls).toHaveLength(1);
    // 不执行会越过 30 秒预算的等待。
    expect(overBudgetClock.sleeps).toEqual([]);
  });
});

describe("claim 的有限重试：复用原正文", () => {
  const CLAIM_BODY = JSON.stringify({
    protocolVersion: 1,
    pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
    deviceId: "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
    proof: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
  });

  it("首次响应丢失：重试同一份正文，不重新取回材料", async () => {
    const clock = new FakeClock(NOW_MS);
    const transport = new ScriptedClaimTransport(clock, [
      (c) => {
        c.advance(1_000);
        return { kind: "network_error" };
      },
      okResponse({ pairingRequestId: "6b6f9c2e-1f1a-4b7c-9c3d-2f0a5b7d8e90" }),
    ]);

    const outcome = await retryClaim({
      canonicalOrigin: CANONICAL_ORIGIN,
      claimBody: CLAIM_BODY,
      expiresAtMs: NOW_MS + 60_000,
      clock,
      transport,
    });

    expect(outcome.kind).toBe("accepted");
    if (outcome.kind !== "accepted") return;
    expect(outcome.attempts).toBe(2);
    expect(transport.calls.map((call) => call.claimBody)).toEqual([CLAIM_BODY, CLAIM_BODY]);
    expect(transport.calls.map((call) => call.canonicalOrigin)).toEqual([
      CANONICAL_ORIGIN,
      CANONICAL_ORIGIN,
    ]);
    expect(clock.sleeps).toEqual([500]);
  });

  it("已知 expiresAt 后停止：不再发第 3 次 claim", async () => {
    const clock = new FakeClock(NOW_MS);
    const transport = new ScriptedClaimTransport(clock, [
      (c) => {
        c.advance(400);
        return { kind: "network_error" };
      },
    ]);

    const outcome = await retryClaim({
      canonicalOrigin: CANONICAL_ORIGIN,
      claimBody: CLAIM_BODY,
      expiresAtMs: NOW_MS + 1_000,
      clock,
      transport,
    });

    expect(outcome).toEqual({ kind: "stopped", reason: "expired", attempts: 2 });
    expect(transport.calls).toHaveLength(2);
    expect(clock.sleeps).toEqual([500]);
  });

  it("claim 的重定向同样禁止：不跟随、不重试", async () => {
    const clock = new FakeClock(NOW_MS);
    const transport = new ScriptedClaimTransport(clock, [
      (c) => {
        c.advance(10);
        return { kind: "redirect" };
      },
    ]);

    const outcome = await retryClaim({
      canonicalOrigin: CANONICAL_ORIGIN,
      claimBody: CLAIM_BODY,
      expiresAtMs: NOW_MS + 60_000,
      clock,
      transport,
    });

    expect(outcome).toEqual({ kind: "stopped", reason: "redirected", attempts: 1 });
    expect(transport.calls).toHaveLength(1);
    expect(clock.sleeps).toEqual([]);
  });
});

describe("边界：共享模块不接入 PWA", () => {
  it("只有本模块自己的实现与测试提到它", () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const appRoot = resolve(here, "..", "..");
    const srcRoot = resolve(here, "..");
    const files: string[] = [];
    const collect = (directory: string): void => {
      for (const entry of readdirSync(directory, { withFileTypes: true })) {
        const full = join(directory, entry.name);
        if (entry.isDirectory()) {
          if (entry.name === "node_modules" || entry.name.startsWith(".")) continue;
          collect(full);
        } else if (/\.tsx?$/.test(entry.name)) {
          files.push(full);
        }
      }
    };
    collect(srcRoot);
    collect(join(appRoot, "app"));

    const referencing = files
      .filter((file) => readFileSync(file, "utf8").includes("native-qr-consumer"))
      .map((file) => relative(appRoot, file).replaceAll("\\", "/"))
      .sort();

    expect(referencing).toEqual([
      "src/features/native-qr-consumer.test.ts",
      "src/features/native-qr-consumer.ts",
    ]);
  });
});
