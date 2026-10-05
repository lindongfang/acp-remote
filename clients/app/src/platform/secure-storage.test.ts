/**
 * R11 的可核对证据：设备身份由**不可导出**密钥承载（Node 侧）。
 *
 * `clients/app` 的 vitest 是 node 环境，而 Node 22 没有内置 `indexedDB`——因此这里提供一个
 * **最小**的 IndexedDB 替身（只实现 `open`/事务/`get`/`put`/`delete`/`clear`/`openCursor`），
 * 装在 `globalThis` 上，用来核对端口的**协议行为**与**落盘内容**。
 *
 * 两条最关键的断言：
 *
 * 1. `crypto.subtle.exportKey("pkcs8", privateKey)` 在 `extractable === false` 时**抛错**
 *    （`InvalidAccessException`）——这就是「私钥不可导出」在行为上的可核对形式；
 * 2. 经过持久化往返（写入替身 → 清进程内缓存 → 重新读出）后，私钥句柄仍然
 *    `extractable === false`，且存储中的记录**不含**任何私钥字节形式。
 *
 * 真实浏览器的 IndexedDB 结构化克隆语义由 `secure-storage.browser.test.ts`（Chromium）核对；
 * 本文件是可在 CI 里常驻运行的那一半。
 */

import { beforeEach, describe, expect, it } from "vitest";

import { SecureStorageError } from "./secure-storage";
import { openDeviceIdentityStore } from "./secure-storage.web";
import { installFakeIndexedDB } from "./testing/fake-indexeddb";

const ORIGIN = "https://work-pc.example.ts.net";
const DEVICE_ID = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";

/** 重新打开端口，模拟「进程重启 / 页面刷新」后从存储重建。 */
function reopenStore() {
  return openDeviceIdentityStore();
}

describe("R11：设备身份由不可导出密钥承载", () => {
  beforeEach(() => {
    installFakeIndexedDB();
  });

  it("生成的私钥 extractable === false，且 exportKey 对它抛错", async () => {
    const store = reopenStore();
    const identity = await store.ensureIdentity({ deviceId: DEVICE_ID, canonicalOrigin: ORIGIN });

    expect(identity.privateKey.extractable).toBe(false);
    expect(identity.privateKey.type).toBe("private");
    expect(identity.privateKey.algorithm.name).toBe("ECDSA");

    // ← 核心判据：不可导出的私钥在 exportKey 上抛错，而不是返回密钥材料。
    await expect(crypto.subtle.exportKey("pkcs8", identity.privateKey)).rejects.toThrow();
    await expect(crypto.subtle.exportKey("jwk", identity.privateKey)).rejects.toThrow();
    await expect(crypto.subtle.exportKey("raw", identity.privateKey)).rejects.toThrow();
  });

  it("公钥是 65 字节 SEC1 未压缩点，且可被独立解析", async () => {
    const store = reopenStore();
    const identity = await store.ensureIdentity({ deviceId: DEVICE_ID, canonicalOrigin: ORIGIN });

    const padded = identity.publicKeyBase64Url.replaceAll("-", "+").replaceAll("_", "/");
    const raw = Uint8Array.from(atob(padded + "=".repeat((4 - (padded.length % 4)) % 4)), (char) =>
      char.charCodeAt(0),
    );
    expect(raw.length).toBe(65);
    expect(raw[0]).toBe(0x04);
  });

  it("持久化往返后私钥句柄仍不可导出（重建路径重新核对标志）", async () => {
    const first = reopenStore();
    const created = await first.ensureIdentity({ deviceId: DEVICE_ID, canonicalOrigin: ORIGIN });
    const signature = await first.signDeviceProof(new TextEncoder().encode("probe"));
    expect(signature.length).toBe(64); // P1363 r || s

    // 模拟刷新：新实例、无进程内缓存，只能从存储重建。
    const second = reopenStore();
    const rehydrated = await second.loadIdentity();
    expect(rehydrated).not.toBeNull();
    expect(rehydrated?.privateKey.extractable).toBe(false);
    expect(rehydrated?.publicKeyBase64Url).toBe(created.publicKeyBase64Url);
    expect(rehydrated?.canonicalOrigin).toBe(ORIGIN);
    await expect(crypto.subtle.exportKey("pkcs8", rehydrated!.privateKey)).rejects.toThrow();
  });

  it("存储中的记录只含不可导出句柄与公开材料，不含私钥字节形式", async () => {
    const store = reopenStore();
    const identity = await store.ensureIdentity({ deviceId: DEVICE_ID, canonicalOrigin: ORIGIN });

    const raw = globalThis.__fakeIndexedDBDump();
    // 记录里出现的字符串只能是公开材料；任何 base64/hex 形状的私钥都不得出现。
    const serialized = JSON.stringify(raw);
    expect(serialized).not.toContain('"d"'); // JWK 私钥分量
    expect(serialized).not.toContain("pkcs8");
    expect(serialized).toContain(identity.publicKeyBase64Url); // 公钥是允许落盘的公开材料

    // 记录里的私钥字段是 CryptoKey 对象本身，而不是字节。
    const record = raw[0]?.value as { privateKey?: unknown };
    expect(record?.privateKey).toBeInstanceOf(CryptoKey);
    expect((record?.privateKey as CryptoKey).extractable).toBe(false);
  });

  it("来源变化时拒绝继续使用该身份（R11 场景 3）", async () => {
    const store = reopenStore();
    await store.ensureIdentity({ deviceId: DEVICE_ID, canonicalOrigin: ORIGIN });

    await expect(
      store.ensureIdentity({
        deviceId: DEVICE_ID,
        canonicalOrigin: "https://other-host.example.ts.net",
      }),
    ).rejects.toBeInstanceOf(SecureStorageError);
  });

  it("清除数据后按身份丢失处理，且不自动生成新身份（R11 场景 4 / R20 场景 4）", async () => {
    const store = reopenStore();
    await store.ensureIdentity({ deviceId: DEVICE_ID, canonicalOrigin: ORIGIN });
    await store.clearIdentity();

    const after = reopenStore();
    expect(await after.loadIdentity()).toBeNull();
    await expect(after.signDeviceProof(new Uint8Array([1]))).rejects.toMatchObject({
      kind: "identity_missing",
    });
  });

  it("被篡改为可导出私钥的持久记录被拒绝（fail closed）", async () => {
    const store = reopenStore();
    await store.ensureIdentity({ deviceId: DEVICE_ID, canonicalOrigin: ORIGIN });

    // 用一把**可导出**的私钥替换记录，读取路径必须拒绝它。
    const extractablePair = await crypto.subtle.generateKey(
      { name: "ECDSA", namedCurve: "P-256" },
      true,
      ["sign", "verify"],
    );
    globalThis.__fakeIndexedDBMutate(0, (value) => ({
      ...(value as object),
      privateKey: extractablePair.privateKey,
    }));

    expect(extractablePair.privateKey.extractable).toBe(true);
    const reopened = reopenStore();
    await expect(reopened.loadIdentity()).rejects.toMatchObject({ kind: "corrupt_entry" });
  });
});
