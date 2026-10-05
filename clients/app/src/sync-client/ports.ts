/**
 * 同步客户端的**平台端口**（`src/sync-client/` 自声明的结构化端口）。
 *
 * ## 为什么这里重新声明而不是 import `src/platform/`
 *
 * `src/layers.test.ts` 把 `src/platform/` 定为「任何层都不得 import」的横切层
 * （平台能力只能由更外层装配），而 `src/sync-client/` 是第 5 层。因此本层**自行声明**
 * 它需要的端口形状，由组合根（`app/`，WP7）以依赖注入方式把 `PlatformPorts` 的实现传进来。
 *
 * 这些接口都是**结构化类型**：`PlatformPorts` 的对应成员天然满足它们，装配处不需要包装类，
 * 只有形状真正不同的能力（下面标注「平台暂无对应成员」的三项）才需要组合根写一个薄适配。
 *
 * 端口清单与各自的替代来源：
 *
 * | 端口 | 平台对应成员 | 说明 |
 * | --- | --- | --- |
 * | `DeviceIdentityLike` | `PlatformPorts.deviceIdentity` | `loadIdentity`/`signDeviceProof` 形状一致，直接可传 |
 * | `TranscriptCodec` | `PlatformPorts` **无**（`src/platform/transcript.ts` 未在 `PlatformPorts` 里导出） | 组合根用该模块的导出拼成对象字面量 |
 * | `HostIdentityPort` | 平台暂无对应成员 | 已配对主机记录与 Host challenge 校验；组合根提供 |
 * | `DigestPort` | 平台暂无对应成员 | SHA-256（快照 digest 需要两级哈希） |
 * | `RandomPort` | 平台暂无对应成员 | nonce 与 UUID（§6.1 要求 CSPRNG） |
 * | `ClockPort` | 平台暂无对应成员 | 现在时间与退避定时（`LifecyclePort` 是可见性事件，不是时钟） |
 *
 * WebSocket 本身由 `socket.web.ts` 在本层提供实现（与 WP5b 的 `*.web.ts` + 端口分离惯例一致），
 * 因此不通过端口注入；测试注入假 socket。
 */

/**
 * 字节视图。
 *
 * 与 `src/platform/transcript.ts` 的 `ByteArray` 是**同一形状**（`Uint8Array<ArrayBuffer>`），
 * 因此平台实现可以直接满足本层的端口签名，而不需要转换层。
 */
export type ByteArray = Uint8Array<ArrayBuffer>;

/** 单个 transcript 字段：tag + 已编码的原始字节（`docs/SYNC_PROTOCOL.md` §6.2）。 */
export interface TranscriptField {
  readonly tag: number;
  readonly bytes: ByteArray;
}

/**
 * transcript codec 端口：结构编码 + 五种字段值编码。
 *
 * 编码规则（magic、tag 严格递增、长度前缀、UTF-8 字节序校验）只有一份实现，
 * 因此本层**不复制**它：这里只声明需要的函数形状，组合根把 `src/platform/transcript.ts`
 * 的同名导出装配进来。字节级跨语言对拍由 `fixtures/sync/v1/transcripts/device-proof.json`
 * 与 TP3 负责（`tasks.md` 2.6 的裁定）。
 */
export interface TranscriptCodec {
  /** 按 domain 与字段拼出 transcript 字节。 */
  encode(domain: string, fields: readonly TranscriptField[]): ByteArray;
  /** `u16be`（protocol version）。 */
  u16be(value: number): ByteArray;
  /** UUID 文本 → 16 个原始字节。 */
  uuid16(value: string): ByteArray;
  /** UTF-8 原始字节（不含 NUL）。 */
  utf8(value: string): ByteArray;
  /** 以单个 NUL 连接、输入须按 UTF-8 字节升序（`negotiatedFeatures` 用）。 */
  nulJoinedUtf8(parts: readonly string[]): ByteArray;
}

/** 设备身份的公开材料（本层只需要这三项，私钥句柄不进握手逻辑）。 */
export interface DeviceIdentityMaterial {
  readonly deviceId: string;
  /** 身份绑定的规范化来源（`https://host[:port]`）。 */
  readonly canonicalOrigin: string;
  readonly publicKeyBase64Url: string;
}

/**
 * 设备身份端口。
 *
 * 形状与 `PlatformPorts.deviceIdentity` 一致：`Promise<DeviceIdentity | null>` 可赋给
 * `Promise<DeviceIdentityMaterial | null>`（协变），因此装配处可直接传入，无需包装。
 */
export interface DeviceIdentityLike {
  /** 读取已持久化的身份；不存在返回 `null`（浏览器清过站点数据 = 需重新配对）。 */
  loadIdentity(): Promise<DeviceIdentityMaterial | null>;
  /** 用设备私钥对已编码 transcript 签名（唯一使用点，端口不提供导出）。 */
  signDeviceProof(transcript: ByteArray): Promise<ByteArray>;
}

/** 已配对主机记录。 */
export interface PairedHostRecord {
  readonly hostId: string;
  /** 已配对的 Host 公钥（65 字节 SEC1，无填充 base64url）。 */
  readonly publicKeyBase64Url: string;
  readonly canonicalOrigin: string;
}

/** Host challenge 校验输入（字段集与 Device proof 相同、domain 不同，§6.3）。 */
export interface HostChallengeInput {
  readonly canonicalOrigin: string;
  readonly hostId: string;
  readonly deviceId: string;
  readonly hostPublicKeyBase64Url: string;
  readonly connectionId: string;
  /** 32 字节原始 nonce（本层已解码，不接受 base64url 文本以免二次解码出错）。 */
  readonly serverNonce: ByteArray;
  readonly clientNonce: ByteArray;
  /** 服务端选中的 feature 列表（编码进 transcript 的 `negotiatedFeatures`）。 */
  readonly negotiatedFeatures: readonly string[];
  /** `hostProof`：64 字节 P1363 的无填充 base64url。 */
  readonly signatureBase64Url: string;
}

/**
 * 已配对主机与 Host challenge 校验端口。
 *
 * **为什么需要它**：`docs/SYNC_PROTOCOL.md` §8.2 要求客户端必须用已配对的 Host key 验证
 * `hostProof`，且 key 与配对记录不一致时进入 `identity_changed`、**不得**询问后自动接受新 key
 * （`pwa-web-client` 的「来源变化后不再自动信任」）。WP5b 的 `PlatformPorts` 没有主机记录与
 * 验签端口，因此这里是本层声明、组合根提供的接缝；缺失时构造 `SyncClient` 直接失败，
 * 绝不允许「跳过校验」这种降级路径。
 */
export interface HostIdentityPort {
  /** 取该规范化来源下的已配对主机记录；从未配对成功返回 `null`。 */
  loadPairedHost(canonicalOrigin: string): Promise<PairedHostRecord | null>;
  /** 校验 `hostProof`；返回 `false` 即视为身份变化，不进入业务阶段。 */
  verifyHostChallenge(input: HostChallengeInput): Promise<boolean>;
}

/** 摘要端口：SHA-256。快照 digest 需要两级哈希（§9.4）。 */
export interface DigestPort {
  /**
   * 返回 32 字节摘要。
   *
   * 入参类型放宽到 `Uint8Array<ArrayBufferLike>`：本层自己产出的字节都是本 realm 的
   * `ArrayBuffer`，但调用方（组合根接的 `crypto.subtle.digest`）返回值的底层 buffer 类型
   * 由 lib.dom 声明决定，收窄到 `ArrayBuffer` 会让真实实现无法满足端口。
   */
  sha256(data: Uint8Array): Promise<Uint8Array>;
}

/** 随机源端口：§6.1 要求 nonce 来自 CSPRNG。 */
export interface RandomPort {
  /** `length` 字节的随机值。 */
  bytes(length: number): ByteArray;
  /** 一个 v4 UUID 文本（消息 `messageId` / `requestId`）。 */
  uuid(): string;
}

/** 取消句柄（`ClockPort.schedule` 的返回值）。 */
export type CancelTimer = () => void;

/** 时钟端口：现在时间与退避/ACK 定时。 */
export interface ClockPort {
  now(): number;
  /** 延迟 `delayMs` 后调用 `task`；返回取消句柄。 */
  schedule(delayMs: number, task: () => void): CancelTimer;
}

/** socket 事件回调束。 */
export interface SocketHandlers {
  onOpen(): void;
  /** 原始 UTF-8 文本：快照 digest 必须按**原始字节**计算，因此这里不做 JSON 预解析。 */
  onMessage(text: string): void;
  /** WebSocket close code（§12.3 的封闭表）。 */
  onClose(code: number, reason: string): void;
  onError(reason: string): void;
}

/** 一条已建立的连接。 */
export interface SocketPort {
  /** 发送一条 JSON 文本。 */
  send(text: string): void;
  close(code: number, reason: string): void;
}

/** 连接工厂：由组合根注入（Web 实现见 `socket.web.ts`，测试注入假 socket）。 */
export type SocketFactory = (input: { readonly url: string; readonly handlers: SocketHandlers }) => SocketPort;