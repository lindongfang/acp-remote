/**
 * 设备身份端口的 **Web/PWA** 实现（R11）。
 *
 * 归属 WP5b，写入范围 `clients/app/src/platform/`。这是本包存在的核心理由：
 * 设备身份由**不可导出**的 P-256 密钥承载，持久化到 IndexedDB，
 * **MUST NOT** 以任何可导出形式落盘。
 *
 * 三条硬性做法：
 *
 * 1. `crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, false, ["sign","verify"])`
 *    ——`extractable: false` 在生成时就固定，之后**没有**任何 API 能让它变为可导出。
 * 2. 只把 `CryptoKey` 对象本身交给 IndexedDB。结构化克隆保留 `extractable: false`，
 *    因此重新读出的句柄仍是不可导出的（`loadIdentity` 会**再核对一次**这个标志）。
 * 3. 私钥的**唯一**使用点是 `signDeviceProof`；本模块不导出私钥，也不把它序列化成
 *    PKCS8/JWK/hex/base64 写入任何存储、日志或错误。
 *
 * `docs/FRONTEND_DESIGN.md` §4.5 与 `docs/IDENTITY_AND_AUTH_CONTRACT.md` §7 都明确：
 * PWA 设备密钥由**客户端平台自己持有**，不经 Rust 的 `identity-keystore`。
 *
 * v1 只交付 Web：本文件是唯一的身份实现，**不写** `.native.ts`（`tasks.md` 2.6）。
 */

import {
  DEVICE_IDENTITY_ORIGIN_KEY,
  DEVICE_IDENTITY_STORE,
  SecureStorageError,
  type DeviceIdentity,
  type DeviceIdentityPort,
} from "./secure-storage";
import type { ByteArray } from "./transcript";

/** IndexedDB 数据库名（本应用只用一个库，便于整体清除）。 */
const DATABASE_NAME = "acp-remote-secure-v1";

/** 身份记录键。 */
const IDENTITY_RECORD_KEY = "device-identity";

/** 允许的规范化来源形状（`schemas/sync/v1/pairing.schema.json#/$defs/canonicalOrigin`）。 */
const CANONICAL_ORIGIN_PATTERN = /^https:\/\/[^/?#]+$/;

/** 持久化的身份记录：**只**含不可导出的 `CryptoKey` 与公开材料，绝不含私钥字节。 */
interface PersistedIdentityRecord {
  readonly deviceId: string;
  readonly canonicalOrigin: string;
  /** 65 字节 SEC1 公钥（无填充 base64url）；公开材料，可落盘。 */
  readonly publicKeyBase64Url: string;
  /**
   * 不可导出的私钥句柄。
   *
   * 这是本记录里**唯一**与私钥有关的字段，且它是 `CryptoKey` 对象而不是字节：浏览器不会
   * 把它序列化成能重新导入的密钥材料（结构化克隆保留 `extractable: false`）。
   */
  readonly privateKey: CryptoKey;
}

/** 跨 realm 安全的 `CryptoKey` 判定（类型守卫，保留窄化）。 */
function isCryptoKey(value: unknown): value is CryptoKey {
  return typeof CryptoKey !== "undefined" && value instanceof CryptoKey;
}

/**
 * 打开设备身份端口。
 *
 * 不做 IO、不抛异常：真正的能力探测（`crypto.subtle`/`indexedDB`）推迟到首次用法，
 * 使「能力不可用」成为可分类的 `SecureStorageError` 而不是模块求值期崩溃。
 */
export function openDeviceIdentityStore(): DeviceIdentityPort {
  return new WebDeviceIdentityStore();
}

class WebDeviceIdentityStore implements DeviceIdentityPort {
  /** 进程内缓存：避免每次签名都读一遍 IndexedDB。 */
  #cached: DeviceIdentity | null = null;

  /** 打开数据库；连接由调用方 `close()`，避免长生命周期句柄泄漏。 */
  #openDatabase(): Promise<IDBDatabase> {
    if (typeof indexedDB === "undefined") {
      return Promise.reject(
        new SecureStorageError(
          "storage_unavailable",
          "IndexedDB 不可用（非安全上下文或浏览器不支持）",
        ),
      );
    }
    // IndexedDB 只提供回调式事件，executor 形式是唯一可用形状。
    return new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open(DATABASE_NAME, 1);
      request.onupgradeneeded = () => {
        const database = request.result;
        if (!database.objectStoreNames.contains(DEVICE_IDENTITY_STORE)) {
          database.createObjectStore(DEVICE_IDENTITY_STORE);
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () =>
        reject(new SecureStorageError("storage_unavailable", "打开 IndexedDB 失败"));
      request.onblocked = () =>
        reject(new SecureStorageError("storage_unavailable", "IndexedDB 被其它标签页阻塞"));
    });
  }

  /** 在一个事务里读一条记录；缺失返回 `undefined`。 */
  #readRecord(database: IDBDatabase): Promise<unknown> {
    return new Promise((resolve, reject) => {
      const transaction = database.transaction(DEVICE_IDENTITY_STORE, "readonly");
      const request = transaction.objectStore(DEVICE_IDENTITY_STORE).get(IDENTITY_RECORD_KEY);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () =>
        reject(new SecureStorageError("storage_unavailable", "读取身份记录失败"));
    });
  }

  /** 在一个事务里写一条记录。 */
  #writeRecord(database: IDBDatabase, value: PersistedIdentityRecord): Promise<void> {
    return new Promise((resolve, reject) => {
      const transaction = database.transaction(DEVICE_IDENTITY_STORE, "readwrite");
      transaction.objectStore(DEVICE_IDENTITY_STORE).put(value, IDENTITY_RECORD_KEY);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () =>
        reject(new SecureStorageError("storage_unavailable", "写入身份记录失败"));
      transaction.onabort = () =>
        reject(new SecureStorageError("storage_unavailable", "写入身份记录被中止"));
    });
  }

  /** 清除身份记录与绑定来源。 */
  #deleteRecords(database: IDBDatabase): Promise<void> {
    return new Promise((resolve, reject) => {
      const transaction = database.transaction(DEVICE_IDENTITY_STORE, "readwrite");
      const store = transaction.objectStore(DEVICE_IDENTITY_STORE);
      store.delete(IDENTITY_RECORD_KEY);
      store.delete(DEVICE_IDENTITY_ORIGIN_KEY);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () =>
        reject(new SecureStorageError("storage_unavailable", "清除身份记录失败"));
      transaction.onabort = () =>
        reject(new SecureStorageError("storage_unavailable", "清除身份记录被中止"));
    });
  }

  /**
   * 把一条可能来自旧版本或被篡改的记录**校验**成身份。
   *
   * 关键断言：`privateKey.extractable === false`。这是「不可导出」在读取路径上的**强制**点——
   * 即使有人手工往 IndexedDB 里塞了一个可导出的私钥，这里也会失败关闭，而不会把它当成设备身份。
   */
  #validateRecord(record: unknown): DeviceIdentity {
    if (record === null || typeof record !== "object") {
      throw new SecureStorageError("corrupt_entry", "身份记录不是对象");
    }
    const candidate = record as Partial<PersistedIdentityRecord>;
    if (
      typeof candidate.deviceId !== "string" ||
      typeof candidate.canonicalOrigin !== "string" ||
      typeof candidate.publicKeyBase64Url !== "string" ||
      !isCryptoKey(candidate.privateKey)
    ) {
      throw new SecureStorageError("corrupt_entry", "身份记录字段缺失或类型不符");
    }
    if (candidate.privateKey.extractable !== false) {
      throw new SecureStorageError(
        "corrupt_entry",
        "持久化的私钥句柄可导出：拒绝把它当作设备身份",
      );
    }
    if (candidate.privateKey.type !== "private") {
      throw new SecureStorageError("corrupt_entry", "持久化的密钥不是私钥");
    }
    if (candidate.privateKey.algorithm.name !== "ECDSA") {
      throw new SecureStorageError("corrupt_entry", "持久化的私钥不是 ECDSA 密钥");
    }
    if (!CANONICAL_ORIGIN_PATTERN.test(candidate.canonicalOrigin)) {
      throw new SecureStorageError("corrupt_entry", "绑定的规范化来源形状非法");
    }
    return {
      deviceId: candidate.deviceId,
      canonicalOrigin: candidate.canonicalOrigin,
      publicKeyBase64Url: candidate.publicKeyBase64Url,
      privateKey: candidate.privateKey,
    };
  }

  /** 生成不可导出的 P-256 密钥对；环境返回可导出私钥时失败关闭。 */
  async #generateNonExtractableKeyPair(): Promise<CryptoKeyPair> {
    if (typeof crypto === "undefined" || crypto.subtle === undefined) {
      throw new SecureStorageError(
        "crypto_unavailable",
        "WebCrypto 不可用（非安全上下文或浏览器不支持）",
      );
    }
    let keyPair: CryptoKeyPair;
    try {
      keyPair = await crypto.subtle.generateKey(
        { name: "ECDSA", namedCurve: "P-256" },
        false, // ← 不可导出：在生成时就固定，之后无法变为可导出
        ["sign", "verify"],
      );
    } catch {
      throw new SecureStorageError("crypto_unavailable", "生成 P-256 密钥对失败");
    }
    if (keyPair.privateKey.extractable !== false) {
      throw new SecureStorageError("crypto_unavailable", "环境返回了可导出的私钥");
    }
    return keyPair;
  }

  /**
   * 导出**公钥**为 65 字节 SEC1 未压缩点。
   *
   * 只对公钥调用 `exportKey("raw")`：它本就是公开材料（wire 上的 `devicePublicKey`），
   * 且 WebCrypto 的 P-256 `raw` 导出固定是 `0x04 || X(32) || Y(32)` 的 65 字节。
   * 私钥**从不**经过 `exportKey`。
   */
  async #exportPublicKey(publicKey: CryptoKey): Promise<ByteArray> {
    const raw = await crypto.subtle.exportKey("raw", publicKey);
    const bytes: ByteArray = new Uint8Array(raw);
    if (bytes.length !== 65 || bytes[0] !== 0x04) {
      throw new SecureStorageError("crypto_unavailable", "公钥不是 65 字节 SEC1 未压缩点");
    }
    return bytes;
  }

  async loadIdentity(): Promise<DeviceIdentity | null> {
    if (this.#cached !== null) return this.#cached;
    const database = await this.#openDatabase();
    let record: unknown;
    try {
      record = await this.#readRecord(database);
    } finally {
      database.close();
    }
    if (record === undefined) return null;
    const identity = this.#validateRecord(record);
    this.#cached = identity;
    return identity;
  }

  async ensureIdentity(input: {
    readonly deviceId: string;
    readonly canonicalOrigin: string;
  }): Promise<DeviceIdentity> {
    if (!CANONICAL_ORIGIN_PATTERN.test(input.canonicalOrigin)) {
      throw new SecureStorageError("corrupt_entry", "规范化来源形状非法");
    }
    const existing = await this.loadIdentity();
    if (existing !== null) {
      if (existing.canonicalOrigin !== input.canonicalOrigin) {
        // R11 场景 3：来源变化后不再自动信任，也不得把身份迁到新来源。
        throw new SecureStorageError(
          "origin_mismatch",
          "已绑定的规范化来源与本次来源不同：必须回到主机侧重新配对",
        );
      }
      return existing;
    }

    const keyPair = await this.#generateNonExtractableKeyPair();
    const rawPublicKey = await this.#exportPublicKey(keyPair.publicKey);
    const identity: DeviceIdentity = {
      deviceId: input.deviceId,
      canonicalOrigin: input.canonicalOrigin,
      publicKeyBase64Url: encodeBase64Url(rawPublicKey),
      privateKey: keyPair.privateKey,
    };

    const database = await this.#openDatabase();
    try {
      await this.#writeRecord(database, {
        deviceId: identity.deviceId,
        canonicalOrigin: identity.canonicalOrigin,
        publicKeyBase64Url: identity.publicKeyBase64Url,
        privateKey: identity.privateKey,
      });
    } finally {
      database.close();
    }
    this.#cached = identity;
    return identity;
  }

  async signDeviceProof(transcript: ByteArray): Promise<ByteArray> {
    const identity = await this.loadIdentity();
    if (identity === null) {
      throw new SecureStorageError("identity_missing", "本设备没有身份：需要重新配对");
    }
    if (typeof crypto === "undefined" || crypto.subtle === undefined) {
      throw new SecureStorageError("crypto_unavailable", "WebCrypto 不可用");
    }
    // P1363：WebCrypto 的 ECDSA 输出就是 64 字节 `r || s`，无需 DER 转换。
    const algorithm: EcdsaParams = { name: "ECDSA", hash: "SHA-256" };
    const signature = await crypto.subtle.sign(algorithm, identity.privateKey, transcript);
    return new Uint8Array(signature);
  }

  async clearIdentity(): Promise<void> {
    this.#cached = null;
    const database = await this.#openDatabase();
    try {
      await this.#deleteRecords(database);
    } finally {
      database.close();
    }
  }
}

/** 无填充 base64url 编码。 */
function encodeBase64Url(bytes: ByteArray): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}
