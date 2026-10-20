/**
 * §8 的 challenge-response 握手（`docs/SYNC_PROTOCOL.md` §8.1–§8.3）的**唯一**实现。
 *
 * ## 为什么不是一个连接类的一部分
 *
 * 客户端现在有两条已认证 WSS：业务连接（`/sync/v1`）与附件连接（`/sync/v1/attachments`，
 * §3.4）。两者都必须**完整执行同一套握手**——同一设备身份、同一 Host 验签、同一子协议与
 * feature 校验、同一撤销与阻断判定。把这段逻辑留在业务连接里，另一条要么复制一份（两处
 * 安全判定必然漂移），要么让附件连接 import 业务连接（连接实现互相依赖）。抽成独立对象后，
 * 两条连接各自持有自己的实例，nonce 与认证状态天然不跨连接共享（§8.4）。
 *
 * ## 边界
 *
 * 本对象只负责「发什么、核什么、什么时候进入阻断态」；socket 的建立、出站序号、订阅与
 * 事件处理仍归各自的连接。因此它只通过回调交付三件事：发送两条认证消息、上抛拒绝/阻断、
 * 关闭连接。
 *
 * 每次 `begin()` 重新生成 `clientNonce`，绝不复用（§8.1）；身份材料在一次握手内只读一次。
 */

import type { AuthAuthenticated, AuthClientHello, AuthClientProof, AuthServerChallenge, Uuid } from "../protocol";
import { encodeBase64Url, decodeBase64Url } from "./base64url";
import type { IncompatibleReason } from "./blocking";
import { CLOSE } from "./blocking";
import type { ByteArray, DeviceIdentityLike, DeviceIdentityMaterial, HostIdentityPort, RandomPort, TranscriptCodec } from "./ports";

/** 本实现的协议版本（`schemas/sync/v1` 的 `const 1`）。 */
export const PROTOCOL_VERSION = 1;

/** Device proof 的 domain tag（§6.3；Host challenge 的校验在 `HostIdentityPort` 实现侧）。 */
const DEVICE_PROOF_DOMAIN = "acp-remote/device-proof/v1";

/** transcript 全局字段 tag（§6.3）。 */
const TAG = {
  protocolVersion: 1,
  hostId: 2,
  deviceId: 3,
  canonicalOrigin: 6,
  clientNonce: 9,
  serverNonce: 10,
  connectionId: 11,
  negotiatedFeatures: 12,
} as const;

/** feature 列表按 UTF-8 字节升序去重（transcript 的 `negotiatedFeatures` 编码要求，§5.2）。 */
export function sortedFeatures(features: readonly string[]): string[] {
  return [...new Set(features)].sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
}

/** 握手需要的端口与连接策略（全部由调用方的连接装配给出）。 */
export interface HandshakeOptions {
  readonly hostId: Uuid;
  readonly identity: DeviceIdentityLike;
  readonly host: HostIdentityPort;
  readonly random: RandomPort;
  readonly transcript: TranscriptCodec;
  /** 本连接在升级请求里声明的子协议；服务端必须选中其中之一（§3.1）。 */
  readonly subprotocols: readonly string[];
  readonly supportedFeatures: readonly string[];
  readonly requiredFeatures: readonly string[];
  readonly clientKind?: "pwa" | "android" | "ios" | "desktop";
  /** 发送 `auth.client_hello` / `auth.client_proof`。 */
  readonly send: (message: AuthClientHello | AuthClientProof) => void;
  readonly close: (code: number, reason: string) => void;
  readonly onRejected: (reason: string, detail: string) => void;
  readonly onBlocked: (cause: "device_revoked" | "version_incompatible" | "host_identity_changed" | "replaced_by_other_connection", code: number, reason?: IncompatibleReason) => void;
}

/** 一次已通过的握手事实。 */
export interface HandshakeState {
  readonly connectionId: Uuid;
  readonly selectedFeatures: readonly string[];
  readonly authenticated: AuthAuthenticated["body"];
}

/**
 * 握手的进度。
 *
 * `begin()` 通过后进入 `hello_sent`；`acceptChallenge()` 发出 proof 后进入 `proof_sent`；
 * `acceptAuthenticated()` 成功后才交付出 `HandshakeState`。未认证前 `connectionId` 可能已有值
 * （challenge 带它来），但认证状态与 feature 只有在 `authenticated` 之后才算数（§8.4）。
 */
export class AuthHandshake {
  readonly #options: HandshakeOptions;
  #identityMaterial: DeviceIdentityMaterial | null = null;
  #clientNonce: ByteArray | null = null;
  #connectionId: Uuid | null = null;
  #selectedFeatures: readonly string[] | null = null;
  #state: HandshakeState | null = null;

  constructor(options: HandshakeOptions) {
    this.#options = options;
  }

  /** 认证完成后的事实；未完成时为 `null`。 */
  get state(): HandshakeState | null {
    return this.#state;
  }

  /** 本连接已由 challenge 分配的标识；未收到 challenge 时为 `null`。 */
  get connectionId(): Uuid | null {
    return this.#connectionId;
  }

  /** 服务端在 challenge 里选中的 feature；未收到 challenge 时为 `null`。 */
  get selectedFeatures(): readonly string[] | null {
    return this.#selectedFeatures;
  }

  /**
   * socket 打开后调用：核对子协议、加载身份并发出 `auth.client_hello`。
   *
   * 返回 `false` 表示这条连接已经不能再往下走（身份缺失或被阻断）；调用方不应再发任何业务消息。
   */
  async begin(negotiatedSubprotocol: string): Promise<boolean> {
    // §3.1 的最后一句：「服务端必须选择该 subprotocol，否则客户端终止连接」。
    //
    // 这一步不能省：服务端不选 subprotocol 时升级**仍然成功**，内核只在 `WebSocket.protocol`
    // 上留一个空串——没有 error 事件、没有非 1000 的关闭码，于是「未选中」与「网络断开」在
    // socket 层完全同形。不在 open 之后核一次，客户端就会在一条没有协商过的连接上开始握手，
    // 失败后按普通断网退避重连（重连会得到同样的升级结果，只会把配置问题刷成无限重试）。
    if (!this.#options.subprotocols.includes(negotiatedSubprotocol)) {
      this.#options.onRejected("subprotocol_not_selected", negotiatedSubprotocol);
      this.#options.onBlocked("version_incompatible", CLOSE.versionIncompatible, "subprotocol_not_selected");
      return false;
    }
    const identity = await this.#loadIdentityMaterial();
    if (identity === null) {
      this.#options.close(CLOSE.authRequired, "identity_missing");
      return false;
    }
    // nonce 每次连接重新生成，绝不复用（§8.1）。
    const nonce = this.#options.random.bytes(32);
    this.#clientNonce = nonce;
    const hello: AuthClientHello = {
      protocolVersion: PROTOCOL_VERSION,
      type: "auth.client_hello",
      messageId: this.#options.random.uuid(),
      body: {
        minProtocolVersion: PROTOCOL_VERSION,
        maxProtocolVersion: PROTOCOL_VERSION,
        hostId: this.#options.hostId,
        deviceId: identity.deviceId,
        clientKind: this.#options.clientKind ?? "pwa",
        clientNonce: encodeBase64Url(nonce),
        supportedFeatures: [...this.#options.supportedFeatures],
        requiredFeatures: [...this.#options.requiredFeatures],
      },
    };
    this.#options.send(hello);
    return true;
  }

  /**
   * `auth.server_challenge`：校验 hostProof 与协商结果，然后签设备证明。
   *
   * 任一校验失败都落到阻断态（身份变化 / 版本不兼容），并**不**发出 `client_proof`：
   * proof 只发一次，且只在本方法末尾发出。
   */
  async acceptChallenge(message: AuthServerChallenge): Promise<void> {
    const identity = await this.#loadIdentityMaterial();
    const clientNonce = this.#clientNonce;
    if (identity === null || clientNonce === null) {
      this.#options.close(CLOSE.authRequired, "identity_missing");
      return;
    }
    const serverNonce = decodeBase64Url(message.body.serverNonce, 32);
    this.#connectionId = message.body.connectionId;
    this.#selectedFeatures = [...message.body.selectedFeatures];

    // §8.2：Host ID、两个 nonce、connection ID、版本与 features 全部要核对，
    // 且 hostProof 必须用**已配对**的 Host key 验证。
    const paired = await this.#options.host.loadPairedHost(identity.canonicalOrigin);
    if (paired === null || paired.hostId !== message.body.hostId) {
      this.#options.onBlocked("host_identity_changed", CLOSE.identityConflict);
      return;
    }
    if (message.body.selectedProtocolVersion !== PROTOCOL_VERSION) {
      this.#options.onBlocked("version_incompatible", CLOSE.versionIncompatible, "protocol_version_unsupported");
      return;
    }
    // §5.2：hello 里声明的 `requiredFeatures` 服务端**必须**全选中，否则这条连接缺少客户端
    // 赖以成立的能力（ACK 与命令状态查询）。校验放在验签**之前**：这条连接已经要被拒，
    // 没必要为它做一次主机验签——也绝不在这条连接上签设备证明。
    const missing = this.#options.requiredFeatures.filter((feature) => !message.body.selectedFeatures.includes(feature));
    if (missing.length > 0) {
      this.#options.onRejected("required_features_missing", missing.join(","));
      this.#options.onBlocked("version_incompatible", CLOSE.versionIncompatible, "required_features_missing");
      return;
    }
    const verified = await this.#options.host.verifyHostChallenge({
      canonicalOrigin: identity.canonicalOrigin,
      hostId: message.body.hostId,
      deviceId: identity.deviceId,
      hostPublicKeyBase64Url: paired.publicKeyBase64Url,
      connectionId: message.body.connectionId,
      serverNonce,
      clientNonce,
      negotiatedFeatures: message.body.selectedFeatures,
      signatureBase64Url: message.body.hostProof,
    });
    if (!verified) {
      this.#options.onBlocked("host_identity_changed", CLOSE.identityConflict);
      return;
    }

    const transcript = this.#options.transcript.encode(DEVICE_PROOF_DOMAIN, [
      { tag: TAG.protocolVersion, bytes: this.#options.transcript.u16be(PROTOCOL_VERSION) },
      { tag: TAG.hostId, bytes: this.#options.transcript.uuid16(message.body.hostId) },
      { tag: TAG.deviceId, bytes: this.#options.transcript.uuid16(identity.deviceId) },
      { tag: TAG.canonicalOrigin, bytes: this.#options.transcript.utf8(identity.canonicalOrigin) },
      { tag: TAG.clientNonce, bytes: clientNonce },
      { tag: TAG.serverNonce, bytes: serverNonce },
      { tag: TAG.connectionId, bytes: this.#options.transcript.uuid16(message.body.connectionId) },
      {
        tag: TAG.negotiatedFeatures,
        bytes: this.#options.transcript.nulJoinedUtf8(sortedFeatures(message.body.selectedFeatures)),
      },
    ]);
    const proof = await this.#options.identity.signDeviceProof(transcript);
    const clientProof: AuthClientProof = {
      protocolVersion: PROTOCOL_VERSION,
      type: "auth.client_proof",
      messageId: this.#options.random.uuid(),
      body: {
        connectionId: message.body.connectionId,
        deviceId: identity.deviceId,
        deviceProof: encodeBase64Url(proof),
      },
    };
    this.#options.send(clientProof);
  }

  /**
   * `auth.authenticated`：核对设备标识并定格本次认证事实。
   *
   * 返回 `false` 表示标识不符，连接已请求关闭，调用方不应再进入「已认证」分支。
   */
  acceptAuthenticated(message: AuthAuthenticated): boolean {
    const identity = this.#identityMaterial;
    if (identity === null || message.body.deviceId !== identity.deviceId) {
      this.#options.onRejected("device_mismatch", message.body.deviceId);
      this.#options.close(CLOSE.identityConflict, "device_mismatch");
      return false;
    }
    this.#connectionId = message.connectionId;
    this.#state = {
      connectionId: message.connectionId,
      selectedFeatures: this.#selectedFeatures ?? [],
      authenticated: message.body,
    };
    return true;
  }

  /** 读取并缓存设备身份材料；本次握手只读一次。 */
  async #loadIdentityMaterial(): Promise<DeviceIdentityMaterial | null> {
    if (this.#identityMaterial !== null) return this.#identityMaterial;
    const identity = await this.#options.identity.loadIdentity();
    if (identity === null) {
      // 本地无身份 = 需要重新配对（浏览器清过站点数据），不是可重试的连接失败。
      this.#options.onRejected("identity_missing", "本地无设备身份");
      return null;
    }
    this.#identityMaterial = identity;
    return identity;
  }
}
