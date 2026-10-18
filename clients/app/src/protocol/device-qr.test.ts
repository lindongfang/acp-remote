/**
 * 新设备短二维码：TypeScript 与 Rust 消费**同一份**固定向量
 * `fixtures/sync/v1/device-qr/vectors.json`（经 `manifest.deviceQrVectors` 登记）。
 *
 * 覆盖：规范 65 字节布局与 87 字符编码、旧 `#data=`、未知版本、错误长度、非规范 base64url、
 * 测试地址 134 字节与 58×29 矩阵预算，以及原始 65 字节主机公钥的 SHA-256 与二维码指纹一致。
 */

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import {
  DEVICE_QR_MATRIX_MAX_COLUMNS,
  DEVICE_QR_MATRIX_MAX_ROWS,
  DEVICE_QR_MAX_TEST_URL_LENGTH,
  DEVICE_QR_PAYLOAD_ASCII_LENGTH,
  DEVICE_QR_PAYLOAD_BYTES,
  DEVICE_QR_TEST_URL_LENGTH,
  DeviceQrError,
  buildDevicePairingUrl,
  decodeDeviceQrPayload,
  encodeDeviceQrPayload,
  parseDevicePairingUrl,
  type DeviceQrErrorCode,
} from "./device-qr";
import { readManifest } from "./manifest";
import { syncFixtureDir } from "./paths";

interface VectorCase {
  readonly name: string;
  readonly expectedError: string;
  readonly payload?: string;
  readonly url?: string;
  readonly hostPublicKey?: string;
  readonly hostKeyFingerprint?: string;
}

interface DeviceQrVectors {
  readonly canonical: {
    readonly canonicalOrigin: string;
    readonly qrToken: string;
    readonly hostPublicKey: string;
    readonly hostKeyFingerprint: string;
    readonly payload: string;
    readonly pairingUrl: string;
  };
  readonly budget: {
    readonly testOrigin: string;
    readonly testPairingUrlLength: number;
    readonly maxTestPairingUrlLength: number;
    readonly maxMatrixColumns: number;
    readonly maxMatrixRows: number;
  };
  readonly invalidPayloads: readonly VectorCase[];
  readonly invalidUrls: readonly VectorCase[];
  readonly hostPublicKeys: readonly VectorCase[];
}

function vectors(): DeviceQrVectors {
  const manifest = readManifest();
  const registered = manifest.deviceQrVectors;
  expect(registered).toBeDefined();
  expect(registered).toHaveLength(1);
  const relative = registered?.[0];
  expect(relative).toBeTruthy();
  return JSON.parse(
    readFileSync(resolve(syncFixtureDir, relative as string), "utf8"),
  ) as DeviceQrVectors;
}

function bytes(text: string): Buffer {
  return Buffer.from(text, "base64url");
}

function errorCode(action: () => unknown): DeviceQrErrorCode {
  try {
    action();
  } catch (error) {
    expect(error).toBeInstanceOf(DeviceQrError);
    return (error as DeviceQrError).code;
  }
  throw new Error("期望抛出 DeviceQrError");
}

async function sha256(bytesToHash: Uint8Array): Promise<Buffer> {
  const digest = await crypto.subtle.digest("SHA-256", bytesToHash as BufferSource);
  return Buffer.from(digest);
}

describe("设备短二维码共享向量", () => {
  it("规范载荷编码 65 字节布局与 87 个 ASCII 字符", () => {
    const { canonical } = vectors();
    const payload = decodeDeviceQrPayload(canonical.payload);

    expect(canonical.payload).toHaveLength(DEVICE_QR_PAYLOAD_ASCII_LENGTH);
    expect(payload.version).toBe(2);
    expect(Buffer.from(payload.qrToken).toString("base64url")).toBe(canonical.qrToken);
    expect(Buffer.from(payload.hostKeyFingerprint).toString("base64url")).toBe(
      canonical.hostKeyFingerprint,
    );
    expect(encodeDeviceQrPayload(payload)).toBe(canonical.payload);
    expect(Buffer.from(payload.qrToken)).toHaveLength(32);
    expect(Buffer.from(payload.hostKeyFingerprint)).toHaveLength(32);
    expect(DEVICE_QR_PAYLOAD_BYTES).toBe(65);

    const pairingUrl = buildDevicePairingUrl(canonical.canonicalOrigin, payload);
    expect(pairingUrl).toBe(canonical.pairingUrl);
    expect(pairingUrl).toHaveLength(DEVICE_QR_TEST_URL_LENGTH);
    expect(pairingUrl.length).toBeLessThanOrEqual(DEVICE_QR_MAX_TEST_URL_LENGTH);

    const parsed = parseDevicePairingUrl(pairingUrl);
    expect(parsed.canonicalOrigin).toBe(canonical.canonicalOrigin);
    expect(encodeDeviceQrPayload(parsed.payload)).toBe(canonical.payload);
  });

  it("非法载荷按共享错误码拒绝", () => {
    for (const testCase of vectors().invalidPayloads) {
      expect(
        errorCode(() => decodeDeviceQrPayload(testCase.payload ?? "")),
        testCase.name,
      ).toBe(testCase.expectedError);
    }
  });

  it("非法地址与旧设备 #data= 按共享错误码拒绝", () => {
    for (const testCase of vectors().invalidUrls) {
      expect(errorCode(() => parseDevicePairingUrl(testCase.url ?? "")), testCase.name).toBe(
        testCase.expectedError,
      );
    }
    const legacy = vectors().invalidUrls.find((testCase) => testCase.name === "legacy-device-data");
    expect(legacy).toBeDefined();
    expect(errorCode(() => parseDevicePairingUrl(legacy?.url ?? ""))).toBe("legacy_data_format");
  });

  it("主机公钥形状非法或指纹不符都不能进入认领路径", async () => {
    for (const testCase of vectors().hostPublicKeys) {
      const publicKey = bytes(testCase.hostPublicKey ?? "");
      const shapeValid = publicKey.length === 65 && publicKey[0] === 0x04;

      if (testCase.expectedError === "host_public_key_invalid") {
        expect(shapeValid, testCase.name).toBe(false);
        continue;
      }
      if (testCase.expectedError === "fingerprint_mismatch") {
        expect(shapeValid, testCase.name).toBe(true);
        const declared = bytes(testCase.hostKeyFingerprint ?? "");
        expect(await sha256(publicKey), testCase.name).not.toEqual(declared);
        continue;
      }
      expect(shapeValid, testCase.name).toBe(true);
      expect(await sha256(publicKey), testCase.name).toEqual(
        bytes(testCase.hostKeyFingerprint ?? ""),
      );
    }
  });

  it("URL 与矩阵预算与共享向量一致", () => {
    const { budget } = vectors();
    expect(budget.testPairingUrlLength).toBe(DEVICE_QR_TEST_URL_LENGTH);
    expect(budget.maxTestPairingUrlLength).toBe(DEVICE_QR_MAX_TEST_URL_LENGTH);
    expect(budget.maxMatrixColumns).toBe(DEVICE_QR_MATRIX_MAX_COLUMNS);
    expect(budget.maxMatrixRows).toBe(DEVICE_QR_MATRIX_MAX_ROWS);
    expect(DEVICE_QR_TEST_URL_LENGTH).toBe(134);
    expect(DEVICE_QR_MAX_TEST_URL_LENGTH).toBe(150);
    expect(DEVICE_QR_MATRIX_MAX_COLUMNS).toBe(58);
    expect(DEVICE_QR_MATRIX_MAX_ROWS).toBe(29);
  });
});
