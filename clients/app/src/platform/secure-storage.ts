/**
 * 设备身份与安全存储的**端口**（`docs/FRONTEND_DESIGN.md` §3 的 Platform Ports）。
 *
 * 归属 WP5b。本模块只声明接口、领域取值与错误类型，**不触碰任何平台 API**——
 * WebCrypto 与 IndexedDB 只出现在 `secure-storage.web.ts`。页面、组件、feature 与
 * 同步客户端都只从本接口取用（`AGENTS.md` §5「页面不能直接操作 WebSocket、浏览器数据库、
 * SecureStore、SQLite 或平台生命周期」）。
 *
 * 为什么接口与实现分成两个文件（`secure-storage.ts` / `secure-storage.web.ts`）：
 * `docs/FRONTEND_DESIGN.md` §3 与 §8.2 要求**原生端**（Android/iOS）用 Keychain/Keystore
 * 替换同一组能力。但本次变更（`tasks.md` 2.6）已裁定 **v1 只交付 Web/PWA，原生端不在本次范围**，
 * 因此这里**只有** `.web.ts` 实现，**不写** `.native.ts` 变体——那是需要独立变更与独立验收的事，
 * 不是「等别人来补」的缺失。接口按平台中立形状声明，正是为了让将来那份实现无需改动上层。
 */

import type { ByteArray } from "./transcript";

/**
 * 设备身份的公开材料与不透明私钥句柄。
 *
 * 私钥**从不出现在返回值里**：`privateKey` 是不透明句柄，只有 `signDeviceProof` 能使用它；
 * 端口**不提供**导出私钥的方法（与 `docs/IDENTITY_AND_AUTH_CONTRACT.md` §7 的 keystore 端口
 * 「私钥不出端口——`sign` 是唯一使用点」同一口径）。
 */
export interface DeviceIdentity {
  /** 设备标识（wire 上的 `deviceId`，UUID 文本）。 */
  readonly deviceId: string;
  /** 身份绑定的规范化来源（`https://host[:port]`，无路径/查询/片段）。 */
  readonly canonicalOrigin: string;
  /** 65 字节 SEC1 未压缩公钥，无填充 base64url（wire 上的 `devicePublicKey`）。 */
  readonly publicKeyBase64Url: string;
  /** 不透明私钥句柄；本类型不暴露其内部结构。 */
  readonly privateKey: CryptoKey;
}

/**
 * 设备身份的持久条目标识：`privateKey` 是不可导出的 `CryptoKey`，因此**不可能**放进普通字符串
 * 存储（`localStorage`、日志、URL、可导出配置）。它们只能进 IndexedDB 的结构化克隆存储。
 */
export const DEVICE_IDENTITY_STORE = "acp-remote/device-identity/v1";

/** 与设备身份绑定的 epoch 记录键：来源变化必须重新配对（R11 场景 3）。 */
export const DEVICE_IDENTITY_ORIGIN_KEY = "canonical-origin";

/**
 * 安全存储错误。
 *
 * 只带分类与不含密钥材料的说明：错误会被记录、上报与显示，因此**不得**携带私钥字节、
 * 导出结果或堆栈里的密钥材料（`AGENTS.md` §3、`docs/IDENTITY_AND_AUTH_CONTRACT.md` §8）。
 */
export class SecureStorageError extends Error {
  readonly kind: SecureStorageErrorKind;

  constructor(kind: SecureStorageErrorKind, message: string) {
    super(message);
    this.name = "SecureStorageError";
    this.kind = kind;
  }
}

export type SecureStorageErrorKind =
  /** 所需密码学能力不可用（非 secure context、浏览器不支持 P-256/WebCrypto）。 */
  | "crypto_unavailable"
  /** 持久化层不可用或写入失败。 */
  | "storage_unavailable"
  /** 持久化条目形状非法（rehydrate 时校验失败）。 */
  | "corrupt_entry"
  /** 已绑定的规范化来源与本次来源不符：不得继续使用该身份（R11 场景 3）。 */
  | "origin_mismatch"
  /** 本设备尚无身份（浏览器已清除站点数据，按身份丢失处理，R11 场景 4）。 */
  | "identity_missing";

/**
 * 设备身份端口。
 *
 * 消费方（WP6 的握手、WP7 的配对页）只依赖这里的形状；`.web.ts` 的 `openDeviceIdentityStore()`
 * 是唯一的装配入口。
 */
export interface DeviceIdentityPort {
  /**
   * 读取已持久化的设备身份；不存在时返回 `null`（**不**自动生成）。
   *
   * 读取后必须核对私钥句柄仍然 `extractable === false`（实现负责），
   * 否则按 `corrupt_entry` 失败关闭——绝不允许一个可导出的句柄被当成设备身份使用。
   */
  loadIdentity(): Promise<DeviceIdentity | null>;

  /**
   * 首次配对：用 WebCrypto 生成**不可导出**的 P-256 密钥对并连同绑定来源一起持久化。
   *
   * 若已存在身份且绑定的来源与 `canonicalOrigin` 相同，返回既有身份（幂等）；
   * 来源不同则抛 `origin_mismatch`——身份**不得**跨来源迁移（R11 场景 3）。
   */
  ensureIdentity(input: {
    readonly deviceId: string;
    readonly canonicalOrigin: string;
  }): Promise<DeviceIdentity>;

  /**
   * 用设备私钥对已编码的 transcript 签名，返回 64 字节 P1363（`r || s`）原始字节。
   *
   * 这是私钥的**唯一**使用点；端口不提供导出。私钥句柄缺失时抛 `identity_missing`。
   */
  signDeviceProof(transcript: ByteArray): Promise<ByteArray>;

  /**
   * 清除本地设备密钥与绑定来源（R20 场景 4「清除数据」）。
   *
   * 这是**客户端本地**操作：主机侧对该设备的记录不被它改变；
   * 清除后 `loadIdentity()` 必须返回 `null`，且不自动生成新身份。
   */
  clearIdentity(): Promise<void>;
}
