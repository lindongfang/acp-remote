/**
 * 真实浏览器里的 R11/R20 校验脚本。
 *
 * 只在 Chromium 中运行（由 `scripts/run-browser-check.mjs` 注入页面）。它存在的理由是
 * Node 侧测试**无法**覆盖两件事：
 *
 * 1. 真实 IndexedDB 的**结构化克隆**语义——`CryptoKey` 存进去、读出来是否仍 `extractable === false`；
 * 2. 真实 `crypto.subtle` 对不可导出私钥的 `exportKey` 拒绝行为。
 *
 * Node 替代与浏览器证据的边界写在这里，避免把替身当浏览器结论
 * （`src/platform/secure-storage.test.ts` 的注释同此口径）。
 */

import { openPlatform } from "../index";
import { DEVICE_PROOF_DOMAIN, encodeTranscript, type ByteArray, type TranscriptField } from "../transcript";
import { encodeBase64Url, encodeNulJoinedUtf8, encodeU16be, encodeUtf8, encodeUuid16 } from "../transcript";

export interface BrowserCheckResult {
  readonly name: string;
  readonly passed: boolean;
  readonly detail: string;
}

/** device-proof 向量输入（与 fixtures/sync/v1/transcripts/device-proof.json 的 input 同值）。 */
const DEVICE_PROOF_INPUT = {
  protocolVersion: 1,
  hostId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
  deviceId: "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
  canonicalOrigin: "https://work-pc.example.ts.net",
  clientNonceHex: "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
  serverNonceHex: "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f",
  connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
  negotiatedFeatures: [
    "acp.raw-payload.v1",
    "core.command-status.v1",
    "core.event-ack.v1",
    "core.snapshot.v1",
  ],
} as const;

const EXPECTED_TRANSCRIPT_BASE64URL =
  "QUNQUgEAGmFjcC1yZW1vdGUvZGV2aWNlLXByb29mL3YxAAgAAQAAAAIAAQACAAAAEL2y7CD5jE2Ht4nlQNUn74cAAwAAABAq4cB8kkJG6anSTsWMEw9JAAYAAAAeaHR0cHM6Ly93b3JrLXBjLmV4YW1wbGUudHMubmV0AAkAAAAgAAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8ACgAAACAgISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0-PwALAAAAEC3lTbd0rkwhiOMF3w3C5jcADAAAAExhY3AucmF3LXBheWxvYWQudjEAY29yZS5jb21tYW5kLXN0YXR1cy52MQBjb3JlLmV2ZW50LWFjay52MQBjb3JlLnNuYXBzaG90LnYx";

function hexToBytes(hex: string): ByteArray {
  const bytes: ByteArray = new Uint8Array(hex.length / 2);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}

function base64UrlToBytes(text: string): ByteArray {
  const padded = text.replaceAll("-", "+").replaceAll("_", "/");
  const binary = atob(padded + "=".repeat((4 - (padded.length % 4)) % 4));
  return Uint8Array.from(binary, (char) => char.charCodeAt(0));
}

/** 用真实 WebCrypto 完成全套 R11/R20 断言。 */
export async function runBrowserChecks(): Promise<BrowserCheckResult[]> {
  const results: BrowserCheckResult[] = [];
  const record = (name: string, passed: boolean, detail: string) => {
    results.push({ name, passed, detail });
  };

  const platform = openPlatform();
  const origin = DEVICE_PROOF_INPUT.canonicalOrigin;

  // ── R11：生成 + 不可导出 ────────────────────────────────────────────────
  const identity = await platform.deviceIdentity.ensureIdentity({
    deviceId: DEVICE_PROOF_INPUT.deviceId,
    canonicalOrigin: origin,
  });

  record(
    "R11 私钥句柄 extractable === false",
    identity.privateKey.extractable === false,
    `extractable=${String(identity.privateKey.extractable)}`,
  );

  let pkcs8Rejected = false;
  try {
    await crypto.subtle.exportKey("pkcs8", identity.privateKey);
  } catch (error) {
    pkcs8Rejected = true;
    record("R11 exportKey(pkcs8) 抛错", true, `拒绝类型 ${(error as Error).name}`);
  }
  if (!pkcs8Rejected) {
    record("R11 exportKey(pkcs8) 抛错", false, "可导出私钥：R11 未被满足");
  }

  let jwkRejected = false;
  try {
    await crypto.subtle.exportKey("jwk", identity.privateKey);
  } catch {
    jwkRejected = true;
  }
  record("R11 exportKey(jwk) 抛错", jwkRejected, jwkRejected ? "已拒绝" : "可导出私钥");

  record(
    "R11 公钥是 65 字节 SEC1 未压缩点",
    (() => {
      const raw = base64UrlToBytes(identity.publicKeyBase64Url);
      return raw.length === 65 && raw[0] === 0x04;
    })(),
    identity.publicKeyBase64Url.slice(0, 12),
  );

  // ── R11：真实 IndexedDB 往返后仍不可导出 ─────────────────────────────────
  const reopened = openPlatform();
  const rehydrated = await reopened.deviceIdentity.loadIdentity();
  record(
    "R11 真实 IndexedDB 往返后 extractable 仍为 false",
    rehydrated !== null && rehydrated.privateKey.extractable === false,
    rehydrated === null ? "读取为空" : `extractable=${String(rehydrated.privateKey.extractable)}`,
  );

  const dump = await new Promise<unknown[]>((resolve, reject) => {
    const request = indexedDB.open("acp-remote-secure-v1", 1);
    request.onsuccess = () => {
      const database = request.result;
      const transaction = database.transaction("acp-remote/device-identity/v1", "readonly");
      const all = transaction.objectStore("acp-remote/device-identity/v1").getAll();
      all.onsuccess = () => {
        resolve(all.result as unknown[]);
        database.close();
      };
      all.onerror = () => reject(new Error("getAll failed"));
    };
    request.onerror = () => reject(new Error("open failed"));
  });
  const serialized = JSON.stringify(dump);
  record(
    "R11 落盘记录不含私钥字节（无 JWK d / pkcs8）",
    !serialized.includes('"d"') && !serialized.includes("pkcs8"),
    `记录数 ${dump.length}`,
  );

  // ── R11：签名可被自身公钥验证，且是 64 字节 P1363 ────────────────────────
  const fields: TranscriptField[] = [
    { tag: 1, bytes: encodeU16be(DEVICE_PROOF_INPUT.protocolVersion) },
    { tag: 2, bytes: encodeUuid16(DEVICE_PROOF_INPUT.hostId) },
    { tag: 3, bytes: encodeUuid16(DEVICE_PROOF_INPUT.deviceId) },
    { tag: 6, bytes: encodeUtf8(DEVICE_PROOF_INPUT.canonicalOrigin) },
    { tag: 9, bytes: hexToBytes(DEVICE_PROOF_INPUT.clientNonceHex) },
    { tag: 10, bytes: hexToBytes(DEVICE_PROOF_INPUT.serverNonceHex) },
    { tag: 11, bytes: encodeUuid16(DEVICE_PROOF_INPUT.connectionId) },
    { tag: 12, bytes: encodeNulJoinedUtf8(DEVICE_PROOF_INPUT.negotiatedFeatures) },
  ];
  const transcript = encodeTranscript(DEVICE_PROOF_DOMAIN, fields);
  record(
    "AC3 浏览器内编码与 device-proof 向量逐字节一致",
    encodeBase64Url(transcript) === EXPECTED_TRANSCRIPT_BASE64URL,
    `长度 ${transcript.length}`,
  );

  const signature = await platform.deviceIdentity.signDeviceProof(transcript);
  record("R11 签名为 64 字节 P1363", signature.length === 64, `长度 ${signature.length}`);

  const publicKey = await crypto.subtle.importKey(
    "raw",
    base64UrlToBytes(identity.publicKeyBase64Url),
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["verify"],
  );
  const verified = await crypto.subtle.verify(
    { name: "ECDSA", hash: "SHA-256" },
    publicKey,
    signature,
    transcript,
  );
  record("R11 签名可被设备公钥验证", verified, `verify=${String(verified)}`);

  // ── R20：imported 正文不落盘 ─────────────────────────────────────────────
  await platform.localCache.putImportedMetadata({
    key: "remote-1",
    metadata: {
      ownerNodeId: DEVICE_PROOF_INPUT.hostId,
      exportId: "export-1",
      sessionId: "session-1",
      originCursor: "1-2",
      localSequence: "7",
      contentDigestSha256: "a".repeat(64),
      acks: ["1", "2"],
    },
    nowMs: Date.now(),
  });
  platform.importedContent.put("remote-1", "IMPORTED-BODY-SENTINEL");
  const persistedImported = JSON.stringify(await platform.localCache.get("remote-1", Date.now()));
  record(
    "R20 imported 正文不进入持久存储",
    !persistedImported.includes("IMPORTED-BODY-SENTINEL"),
    "sentinel 未出现在持久层",
  );
  platform.importedContent.clear();
  record(
    "R20 imported 正文只在内存（clear 后消失）",
    platform.importedContent.get("remote-1") === null,
    "内存载体已清空",
  );

  // ── 清理：清除身份与缓存，避免同一 profile 残留 ──────────────────────────
  await platform.deviceIdentity.clearIdentity();
  await platform.localCache.clear();

  return results;
}
