/**
 * 一条 Sync 连接的生命周期：逐连接签名握手、`subscribe`、增量重放、ACK、命令派发。
 *
 * ## 握手为什么逐连接重做
 *
 * R11 场景「每次连接重新握手」+ §8.4「认证状态只属于当前 WSS 连接，关闭后立即失效。
 * 协议不签发 session token 或 refresh token」：因此本类**不保存**任何跨连接的凭据，
 * 每次 `start()` 都新建 `clientNonce` 并重跑 `clientHello → serverChallenge → clientProof → authenticated`。
 * nonce 由 `RandomPort` 每次重新生成，绝不复用（§8.1）。
 *
 * ## 阻断态在这里判定
 *
 * 四个阻断态的触发源各不相同，但都收敛到 `SyncClientEvent.blocked`：
 * close code 4411（被顶替）、4410（撤销）、4406（版本不兼容）、`hostProof` 校验失败
 * 或 host key 与配对记录不一致（身份变化）。判定完成后 `SyncClient` 停止自动重连。
 */

import type {
  AuthAuthenticated,
  AuthClientHello,
  AuthClientProof,
  AuthServerChallenge,
  CommandMessage,
  CommandResultMessage,
  Cursor,
  EventMessage,
  SyncAck,
  SyncCaughtUp,
  SyncResetRequired,
  SyncSnapshotBegin,
  SyncSnapshotChunk,
  SyncSnapshotEnd,
  SyncWireMessage,
  Uuid,
} from "../protocol";
import { decodeBase64Url, encodeBase64Url } from "./base64url";
import { decodeWireMessage } from "./wire";
import { EventLedger, isImportedEventIdConsistent } from "./dedupe";
import type {
  ByteArray,
  ClockPort,
  CancelTimer,
  DeviceIdentityLike,
  DeviceIdentityMaterial,
  HostIdentityPort,
  RandomPort,
  SocketHandlers,
  SocketPort,
  TranscriptCodec,
} from "./ports";
import { SnapshotStaging, SnapshotValidationError, type SnapshotDiscardReason, type VerifiedSnapshot } from "./snapshot";

/** 本实现的协议版本（`schemas/sync/v1` 的 `const 1`）。 */
const PROTOCOL_VERSION = 1;

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

/** 关闭码（§12.3 的封闭表）。 */
const CLOSE = {
  normal: 1000,
  protocolError: 4400,
  authRequired: 4401,
  versionIncompatible: 4406,
  identityConflict: 4409,
  deviceRevoked: 4410,
  connectionReplaced: 4411,
  serverUnavailable: 4500,
} as const;

/**
 * 序号缺口后的重新订阅：退避参数与连续次数上限。
 *
 * ## 为什么必须有上限而不是「一直重订阅」
 *
 * §9.2/§9.5 明文允许全局 sequence 对单个设备存在**因权限过滤而不可见**的空洞。
 * 一旦缺口落在这样的空洞上，从最后确认位置重订阅会拿回**同一条**事件，缺口判定原地复现——
 * 无界重订阅既补不齐缺口，又对服务端构成持续 `sync.subscribe` 洪泛（与 §8.5 要避免的
 * 重连风暴同类）。因此重订阅必须既有退避（一次比一次久）又有次数上限（到顶就停，
 * 把后续恢复交给状态层按重连退避重新来）。
 *
 * 取值依据：
 * - `baseDelayMs = 250`：既能救回「偶发一次乱序」这种最常见的真缺口，又低于用户感知的
 *   亚秒级停顿阈值，不会让正常抖动被放大成恢复风暴。
 * - 倍率 2、`maxDelayMs = 4000`：250/500/1000/2000/4000，连续 5 次累计 7.75s，
 *   足以覆盖服务端一次短暂重放抖动，又不会在几秒内把服务端打穿。
 * - `maxAttempts = 5`：单条连接最多补发 5 次；到顶后上报 `gap_resubscribe_exhausted`
 *   并停止，恢复路径回到「重连 + 快照重建」，那是有全局退避的通道。
 */
const GAP_RESUBSCRIBE = {
  baseDelayMs: 250,
  maxDelayMs: 4_000,
  maxAttempts: 5,
} as const;

/** 阻断原因（与 `src/state/connection-machine.ts` 的 `BlockingCause` 一一对应）。 */
export type ClientBlockCause =
  | "device_revoked"
  | "version_incompatible"
  | "host_identity_changed"
  | "replaced_by_other_connection";

/** 客户端向消费方（`src/state/` 与 WP7 的 feature 层）发出的事件。 */
export type SyncClientEvent =
  /** socket 已建立，开始逐连接握手。 */
  | { readonly kind: "authenticating" }
  /** 认证成功：连接级信息（scopes、serverEpoch、限额）在此交付。 */
  | { readonly kind: "authenticated"; readonly info: AuthenticatedInfo }
  /** 已按最后确认游标发出 `subscribe`；此时处于重放追平。 */
  | { readonly kind: "replaying"; readonly cursor: Cursor | null }
  /** `sync.caught_up`：此后为实时事件。 */
  | { readonly kind: "online"; readonly cursor: Cursor }
  /** 服务端要求重建快照：未完成的暂存区已被丢弃。 */
  | { readonly kind: "reset_required"; readonly reason: SyncResetRequired["body"]["reason"] }
  /** 一份快照已验证通过并可原子替换（调用方替换失败时保持旧状态）。 */
  | { readonly kind: "snapshot_verified"; readonly snapshot: VerifiedSnapshot }
  /** 一条首次见到的事件（已去重）。 */
  | { readonly kind: "event"; readonly event: EventMessage }
  /** 一条重复事件被丢弃（诊断用，**不**触发任何副作用）。 */
  | { readonly kind: "duplicate_dropped"; readonly eventId: Uuid }
  /** 序号出现缺口：客户端从最后确认位置重新订阅，不推进游标。 */
  | { readonly kind: "gap_detected"; readonly expected: string; readonly received: string }
  /** 命令结果。 */
  | { readonly kind: "command_result"; readonly result: CommandResultMessage }
  /** 连接关闭。`retryable` 为假时不自动重连。 */
  | { readonly kind: "closed"; readonly code: number; readonly retryable: boolean; readonly detail?: string }
  /** 进入阻断态：必须停止自动重连。 */
  | { readonly kind: "blocked"; readonly cause: ClientBlockCause; readonly code: number }
  /** 消息被解码拒绝（诊断用；不进入阻断态，除非调用方据此升级）。 */
  | { readonly kind: "rejected"; readonly reason: string; readonly detail: string };

/** 客户端需要的 feature 词表（`compatibility/features/v1/features.json` 的 sync 段）。 */
export const SUPPORTED_FEATURES = [
  "acp.raw-payload.v1",
  "core.command-status.v1",
  "core.event-ack.v1",
  "core.local-catalog.v1",
  "core.session-create.v1",
  "core.snapshot.v1",
  "resource.remote-origin.v1",
] as const;

/** 客户端必需 feature：ACK 与命令状态查询缺失则无法满足 R13/R14。 */
export const REQUIRED_FEATURES = ["core.command-status.v1", "core.event-ack.v1"] as const;

/** 一条连接的构造输入（端口由组合根注入）。 */
export interface SyncConnectionOptions {
  readonly hostId: Uuid;
  readonly url: string;
  readonly identity: DeviceIdentityLike;
  readonly host: HostIdentityPort;
  readonly random: RandomPort;
  readonly transcript: TranscriptCodec;
  /** 建立 socket 的工厂；Web 实现见 `socket.web.ts`。 */
  readonly openSocket: (input: { readonly url: string; readonly handlers: SocketHandlers }) => SocketPort;
  /** 上一次已确认的持久游标；首次为 `null`（§9.2）。 */
  readonly resumeCursor: Cursor | null;
  /**
   * 时钟端口：缺口恢复的退避定时。
   *
   * 用端口而不是直接 `setTimeout`，是为了让退避可被测试确定性地推进（真实计时器会让
   * 「订阅次数是否有界」这类断言依赖竞态窗口）；它同时也是 §8.5 退避策略的既定接缝。
   */
  readonly clock: ClockPort;
  readonly onEvent: (event: SyncClientEvent) => void;
  readonly clientKind?: "pwa" | "android" | "ios" | "desktop";
  /**
   * 取本 epoch 的去重账本。
   *
   * 账本必须**跨连接**存活（§8.5 的切换窗口里同一事件会被重复投递），因此由调用方持有：
   * epoch 相同时返回既有账本，epoch 变化时返回新账本（此时必须重建快照）。
   */
  readonly ledgerFor: (serverEpoch: Uuid) => EventLedger;
}
/** 认证后的连接级信息（`auth.authenticated.body`）。 */
export type AuthenticatedInfo = AuthAuthenticated["body"];


/**
 * 一条 Sync 连接。
 *
 * 一个实例对应一条 socket：重建连接时创建新实例，因此 nonce、握手与认证状态天然不跨连接。
 */
export class SyncConnection {
  readonly #options: SyncConnectionOptions;
  readonly #socket: SocketPort;
  readonly #random: RandomPort;
  readonly #transcript: TranscriptCodec;
  readonly #identity: DeviceIdentityLike;
  readonly #host: HostIdentityPort;
  readonly #onEvent: (event: SyncClientEvent) => void;

  /** 本连接的 `clientNonce`；每次 `start()` 重新生成，绝不复用。 */
  #clientNonce: ByteArray | null = null;
  /** 已加载的设备身份材料（握手期间缓存，避免每条消息重复读 IndexedDB）。 */
  #identityMaterial: DeviceIdentityMaterial | null = null;
  #connectionId: Uuid | null = null;
  /**
   * 客户端方向的 `connectionSequence` 计数器（§4.1）。
   *
   * 认证完成后每条出站消息都必须带它，且从 `1` 开始**严格加一**；两个方向各自维护，
   * 因此这里既不能抄服务端 `auth.authenticated` 里的序号（那是服务端方向的计数器），
   * 也不能让认证前的消息消耗它（§4.1 要求认证前省略该字段）。
   */
  #outboundSequence = 0;
  /** 本连接的认证结果（scopes 与限额据此生效）。 */
  #authenticated: AuthenticatedInfo | null = null;
  /**
   * 去重账本。
   *
   * 在  之后才创建： 由服务端下发，构造连接时客户端并不掌握，
   * 而账本的 epoch 判错会把合法事件误判为缺口。跨连接保留由调用方持有账本实例实现
   * （§8.5 切换窗口仍需识别重复投递）。
   */
  #ledger: EventLedger | null = null;
  readonly #staging: SnapshotStaging;
  /** 已被确认处理的游标；ACK 只能前进（§9.5）。 */
  #acked: Cursor | null;
  #closed = false;
  /** 已 ACK 的游标，用于保证「只能前进」。 */
  #lastAcked: Cursor | null;
  /**
   * 连续缺口恢复的次数：每成功处理一条 `new` 事件或收到 `caught_up` 即清零。
   *
   * 清零是必要的：一次已补齐的缺口说明恢复通道是通的，后续零星抖动应当拿到**完整**的
   * 重试预算，而不是继承上一轮的余额。
   */
  #gapAttempts = 0;
  /** 待触发的缺口重订阅定时器；同一时刻至多一个。 */
  #gapRetry: CancelTimer | null = null;

  constructor(options: SyncConnectionOptions, staging: SnapshotStaging) {
    this.#options = options;
    this.#random = options.random;
    this.#transcript = options.transcript;
    this.#identity = options.identity;
    this.#host = options.host;
    this.#onEvent = options.onEvent;
    this.#staging = staging;
    this.#acked = options.resumeCursor;
    this.#lastAcked = options.resumeCursor;
    this.#socket = options.openSocket({
      url: options.url,
      handlers: {
        onOpen: () => {
          void this.#onOpen();
        },
        onMessage: (text) => {
          this.#onMessage(text);
        },
        onClose: (code, reason) => {
          this.#onClose(code, reason);
        },
        onError: (reason) => {
          this.#onEvent({ kind: "rejected", reason: "socket_error", detail: reason });
        },
      },
    });
  }

  /** 本连接的身份信息（认证成功后可用）。 */
  get connectionId(): Uuid | null {
    return this.#connectionId;
  }

  /** 认证结果；未认证时为 `null`（消费方据此判断能否派发命令）。 */
  get authenticatedInfo(): AuthenticatedInfo | null {
    return this.#authenticated;
  }

  /** 已确认游标。 */
  get ackedCursor(): Cursor | null {
    return this.#acked;
  }

  /** 启动连接（socket 已在构造时建立，这里只等 `onOpen`）。 */
  start(): void {
    this.#onEvent({ kind: "authenticating" });
  }

  /** 主动关闭（用户断开）。 */
  close(code: number = CLOSE.normal, reason = "client_closed"): void {
    this.#closed = true;
    this.#cancelGapResubscribe();
    this.#socket.close(code, reason);
  }

  /** 丢弃未完成的暂存快照；`reason` 记录「为什么这次快照没提交」（§9.4）。 */
  discardStaging(reason: SnapshotDiscardReason = "reset_required"): void {
    this.#staging.discard(reason);
  }
  /** 读取并缓存设备身份材料；本连接只读一次（读一次即可覆盖整个握手）。 */
  async #loadIdentityMaterial(): Promise<DeviceIdentityMaterial | null> {
    if (this.#identityMaterial !== null) return this.#identityMaterial;
    const identity = await this.#identity.loadIdentity();
    if (identity === null) {
      // 本地无身份 = 需要重新配对（浏览器清过站点数据），不是可重试的连接失败。
      this.#onEvent({ kind: "rejected", reason: "identity_missing", detail: "本地无设备身份" });
      return null;
    }
    this.#identityMaterial = identity;
    return identity;
  }


  // ── 握手 ────────────────────────────────────────────────────────────────
  async #onOpen(): Promise<void> {
    const identity = await this.#loadIdentityMaterial();
    if (identity === null) {
      this.close(CLOSE.authRequired, "identity_missing");
      return;
    }
    // nonce 每次连接重新生成，绝不复用（§8.1）。
    const nonce = this.#random.bytes(32);
    this.#clientNonce = nonce;
    const hello: AuthClientHello = {
      protocolVersion: PROTOCOL_VERSION,
      type: "auth.client_hello",
      messageId: this.#random.uuid(),
      body: {
        minProtocolVersion: PROTOCOL_VERSION,
        maxProtocolVersion: PROTOCOL_VERSION,
        hostId: this.#options.hostId,
        deviceId: identity.deviceId,
        clientKind: this.#options.clientKind ?? "pwa",
        clientNonce: encodeBase64Url(nonce),
        supportedFeatures: [...SUPPORTED_FEATURES],
        requiredFeatures: [...REQUIRED_FEATURES],
      },
    };
    this.#send(hello);
  }

  /** `auth.server_challenge`：校验 hostProof，然后签设备证明。 */
  async #onServerChallenge(message: AuthServerChallenge): Promise<void> {
    const identity = await this.#loadIdentityMaterial();
    const clientNonce = this.#clientNonce;
    if (identity === null || clientNonce === null) {
      this.close(CLOSE.authRequired, "identity_missing");
      return;
    }
    const serverNonce = decodeBase64Url(message.body.serverNonce, 32);
    this.#connectionId = message.body.connectionId;

    // §8.2：Host ID、两个 nonce、connection ID、版本与 features 全部要核对，
    // 且 hostProof 必须用**已配对**的 Host key 验证。
    const paired = await this.#host.loadPairedHost(identity.canonicalOrigin);
    if (paired === null || paired.hostId !== message.body.hostId) {
      this.#block("host_identity_changed", CLOSE.identityConflict);
      return;
    }
    if (message.body.selectedProtocolVersion !== PROTOCOL_VERSION) {
      this.#block("version_incompatible", CLOSE.versionIncompatible);
      return;
    }
    const verified = await this.#host.verifyHostChallenge({
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
      this.#block("host_identity_changed", CLOSE.identityConflict);
      return;
    }

    const transcript = this.#transcript.encode(DEVICE_PROOF_DOMAIN, [
      { tag: TAG.protocolVersion, bytes: this.#transcript.u16be(PROTOCOL_VERSION) },
      { tag: TAG.hostId, bytes: this.#transcript.uuid16(message.body.hostId) },
      { tag: TAG.deviceId, bytes: this.#transcript.uuid16(identity.deviceId) },
      { tag: TAG.canonicalOrigin, bytes: this.#transcript.utf8(identity.canonicalOrigin) },
      { tag: TAG.clientNonce, bytes: clientNonce },
      { tag: TAG.serverNonce, bytes: serverNonce },
      { tag: TAG.connectionId, bytes: this.#transcript.uuid16(message.body.connectionId) },
      { tag: TAG.negotiatedFeatures, bytes: this.#transcript.nulJoinedUtf8(sortedFeatures(message.body.selectedFeatures)) },
    ]);
    const proof = await this.#identity.signDeviceProof(transcript);
    const clientProof: AuthClientProof = {
      protocolVersion: PROTOCOL_VERSION,
      type: "auth.client_proof",
      messageId: this.#random.uuid(),
      body: {
        connectionId: message.body.connectionId,
        deviceId: identity.deviceId,
        deviceProof: encodeBase64Url(proof),
      },
    };
    this.#send(clientProof);
  }

  /** `auth.authenticated`：按最后确认游标订阅，进入重放追平。 */
  #onAuthenticated(message: AuthAuthenticated): void {
    if (message.body.deviceId !== this.#identityMaterial?.deviceId) {
      this.#onEvent({ kind: "rejected", reason: "device_mismatch", detail: "authenticated 的 deviceId 不符" });
      this.close(CLOSE.identityConflict, "device_mismatch");
      return;
    }
    this.#authenticated = message.body;
    this.#connectionId = message.connectionId;
    // 账本的 epoch 必须来自本次认证结果；换 epoch 即意味着必须重建快照。
    this.#ledger = this.#options.ledgerFor(message.body.serverEpoch);
    // 恢复游标必须与账本对 epoch 的处理保持一致：`ledgerFor` 在 epoch 变化时把 `resumeFrom`
    // 置 null（旧 epoch 的序号与新事件库不可比），连接侧这两份游标也必须一起丢掉。
    // 不丢的话会同时坏掉两条链路：`sync.subscribe` 带着旧 epoch 游标 → 服务端回
    // `sync.cursor_invalid`（§9.4）只上报不恢复；且 `compareCursors` 跨 epoch 恒判回退，
    // 于是此后每条事件都产出 `ack_not_advancing`，一条 ACK 也发不出去（违反 §9.5）。
    if (this.#acked !== null && this.#acked.serverEpoch !== message.body.serverEpoch) {
      this.#acked = null;
      this.#lastAcked = null;
    }
    this.#sendSubscribe();
    this.#onEvent({ kind: "authenticated", info: message.body });
    this.#onEvent({ kind: "replaying", cursor: this.#acked });
  }

  /**
   * 从最后确认游标发出 `sync.subscribe`。
   *
   * 认证后首次订阅与「序号缺口后的恢复」走**同一条**路径：两者都要从 `#acked` 重新订阅，
   * 区别只在于游标是否为空。缺口恢复必须真的重发，否则事件流会静默停摆（R14「连续恢复」）。
   */
  #sendSubscribe(): void {
    this.#send({
      protocolVersion: PROTOCOL_VERSION,
      type: "sync.subscribe",
      messageId: this.#random.uuid(),
      connectionId: this.#requireConnectionId(),
      // 客户端方向自己的计数器（§4.1），不抄服务端信封里的序号。
      connectionSequence: this.#nextOutboundSequence(),
      body: { cursor: this.#acked, scope: "machine" },
    });
  }

  // ── 入站分派 ──────────────────────────────────────────────────────────────

  #onMessage(text: string): void {
    const message = decodeOrReject(text, this.#onEvent);
    if (message === null) return;
    switch (message.type) {
      case "auth.server_challenge":
        void this.#onServerChallenge(message);
        return;
      case "auth.authenticated":
        this.#onAuthenticated(message);
        return;
      case "sync.reset_required":
        this.#onResetRequired(message);
        return;
      case "sync.snapshot_begin":
        this.#staging.begin(message as SyncSnapshotBegin);
        return;
      case "sync.snapshot_chunk":
        void this.#onSnapshotChunk(text, message as SyncSnapshotChunk);
        return;
      case "sync.snapshot_end":
        void this.#onSnapshotEnd(message as SyncSnapshotEnd);
        return;
      case "sync.caught_up":
        this.#onCaughtUp(message);
        return;
      case "event":
        this.#onEventMessage(message);
        return;
      case "command.result":
        this.#onCommandResult(message);
        return;
      case "error":
        this.#onProtocolError(message.body.code, message.body.message);
        return;
      case "control.ping":
        // 浏览器不能发 Ping frame，因此应用层 ping 必须原样回 pong（§13）。
        this.#send({
          protocolVersion: PROTOCOL_VERSION,
          type: "control.pong",
          messageId: this.#random.uuid(),
          connectionId: this.#requireConnectionId(),
          connectionSequence: this.#nextOutboundSequence(),
          body: { nonce: message.body.nonce },
        });
        return;
      case "control.pong":
      case "sync.ack":
      case "sync.subscribe":
      case "sync.snapshot_request":
        return;
      default:
        this.#onEvent({ kind: "rejected", reason: "unexpected_type", detail: message.type });
    }
  }

  /**
   * 事件去重 + 缺口处理。
   *
   * 重复事件**不**推进游标也不触发呈现；缺口要求从最后确认位置恢复，不静默跳过。
   */
  #onEventMessage(message: EventMessage): void {
    if (!isImportedEventIdConsistent(message)) {
      this.#onEvent({ kind: "rejected", reason: "imported_event_id_mismatch", detail: message.body.eventId });
      return;
    }
    const ledger = this.#ledger;
    if (ledger === null) {
      this.#onEvent({ kind: "rejected", reason: "event_before_auth", detail: message.body.eventId });
      return;
    }
    const verdict = ledger.evaluate(message);
    if (verdict.kind === "duplicate") {
      this.#onEvent({ kind: "duplicate_dropped", eventId: message.body.eventId });
      return;
    }
    if (verdict.kind === "gap") {
      this.#onEvent({ kind: "gap_detected", expected: verdict.expected, received: verdict.received });
      // 缺口必须**真的**恢复：只上报而不重新订阅，事件流会静默停摆，
      // 此后每条事件都判 gap（R14「从最后确认的位置连续恢复」）。但恢复必须退避且有上限，
      // 否则补不齐的缺口会变成对服务端的无界 `sync.subscribe` 洪泛。
      this.#scheduleGapResubscribe();
      return;
    }
    const cursor = ledger.commit(message);
    // 缺口已补齐：把重试预算清零，后续零星抖动拿到的应是完整预算。
    this.#gapAttempts = 0;
    this.#onEvent({ kind: "event", event: message });
    this.#sendAck(cursor);
  }

  /**
   * 缺口后**带退避与上限**地重新订阅。
   *
   * 只重发是不够的：缺口可能落在 §9.2/§9.5 允许的「因权限过滤不可见」空洞上，此时重订阅
   * 会拿回同一条事件、缺口原地复现——无界重订阅既补不齐，又是对服务端的持续洪泛。
   * 因此每次恢复都比上一次更久，且到顶后停止并上报，由状态层按重连退避重新进入。
   */
  #scheduleGapResubscribe(): void {
    if (this.#gapAttempts >= GAP_RESUBSCRIBE.maxAttempts) {
      this.#onEvent({
        kind: "rejected",
        reason: "gap_resubscribe_exhausted",
        detail: `连续 ${this.#gapAttempts} 次缺口恢复仍未补齐，等待重连`,
      });
      return;
    }
    this.#gapAttempts += 1;
    const delayMs = Math.min(
      GAP_RESUBSCRIBE.baseDelayMs * 2 ** (this.#gapAttempts - 1),
      GAP_RESUBSCRIBE.maxDelayMs,
    );
    this.#cancelGapResubscribe();
    this.#gapRetry = this.#options.clock.schedule(delayMs, () => {
      this.#gapRetry = null;
      // 连接已关闭时不再打扰服务端：恢复路径交给重连（那里有全局退避）。
      if (this.#closed) return;
      this.#sendSubscribe();
    });
  }

  /** 取消尚未触发的缺口重订阅。 */
  #cancelGapResubscribe(): void {
    if (this.#gapRetry === null) return;
    this.#gapRetry();
    this.#gapRetry = null;
  }

  #onCaughtUp(message: SyncCaughtUp): void {
    // 即使某段序号因权限过滤不可见，也可以 ACK caught_up 的 cursor（§9.5）。
    this.#acked = message.body.cursor;
    // `caught_up` 是服务端给的**屏障**：它声明这条序号之前对本设备可见的事件都已交付。
    // 必须同样喂给账本，否则「已确认游标」与账本水位成为两份会漂移的拷贝——重连时账本按旧值
    // 起步，紧接的第一条可见事件（屏障之后）必被判缺口，重订阅又拿回同一条事件，
    // 事件流一条都不呈现（详见 EventLedger.advanceTo 的注释）。
    this.#ledger?.advanceTo(message.body.cursor);
    // 追平完成：本条连接的重试预算重新计满。
    this.#gapAttempts = 0;
    this.#sendAck(message.body.cursor);
    this.#onEvent({ kind: "online", cursor: message.body.cursor });
  }

  /**
   * `sync.reset_required`：丢弃未完成的暂存快照并重新请求。
   *
   * §9.4 明确「验证失败不得损坏最后一个已完成缓存」——因此本方法**只**丢弃暂存区，
   * 已完成状态由消费方（快照仓库）持有，本层不碰。
   */
  #onResetRequired(message: SyncResetRequired): void {
    this.discardStaging();
    this.#onEvent({ kind: "reset_required", reason: message.body.reason });
    if (message.body.snapshotAvailable) {
      this.#send({
        protocolVersion: PROTOCOL_VERSION,
        type: "sync.snapshot_request",
        messageId: this.#random.uuid(),
        connectionId: this.#requireConnectionId(),
        connectionSequence: this.#nextOutboundSequence(),
        body: { reason: message.body.reason },
      });
    }
  }

  async #onSnapshotChunk(rawText: string, message: SyncSnapshotChunk): Promise<void> {
    try {
      await this.#staging.acceptChunk(rawText, message);
    } catch (error) {
      if (error instanceof SnapshotValidationError) {
        this.#onEvent({ kind: "rejected", reason: error.reason, detail: error.message });
        return;
      }
      throw error;
    }
  }

  async #onSnapshotEnd(message: SyncSnapshotEnd): Promise<void> {
    try {
      const verified = await this.#staging.complete(message);
      this.#onEvent({ kind: "snapshot_verified", snapshot: verified });
      // 快照 cursor 之后的事件会补发，随后 `sync.caught_up`；ACK 推进到快照 cursor。
      this.#acked = verified.cursor;
      // 快照 cursor 同样是屏障：快照涵盖的序号之前的内容都已落进已完成状态，
      // 账本必须一并推进，否则恢复游标与账本水位分叉（与 #onCaughtUp 同理）。
      this.#ledger?.advanceTo(verified.cursor);
    } catch (error) {
      if (error instanceof SnapshotValidationError) {
        // 保留上一次已完成状态；重连后重新请求（§9.4）。
        this.#onEvent({ kind: "rejected", reason: error.reason, detail: error.message });
        return;
      }
      throw error;
    }
  }

  #onCommandResult(message: CommandResultMessage): void {
    this.#onEvent({ kind: "command_result", result: message });
  }

  /** 结构化错误映射到阻断态或普通失败。 */
  #onProtocolError(code: string, message: string): void {
    switch (code) {
      case "auth.device_revoked":
        this.#block("device_revoked", CLOSE.deviceRevoked);
        return;
      case "protocol.version_unsupported":
      case "protocol.feature_required":
        this.#block("version_incompatible", CLOSE.versionIncompatible);
        return;
      case "auth.origin_mismatch":
      case "auth.host_mismatch":
        this.#block("host_identity_changed", CLOSE.identityConflict);
        return;
      case "auth.required":
      case "auth.proof_invalid":
        this.#onEvent({ kind: "rejected", reason: code, detail: message });
        this.close(CLOSE.authRequired, code);
        return;
      default:
        this.#onEvent({ kind: "rejected", reason: code, detail: message });
    }
  }

  #onClose(code: number, reason: string): void {
    if (this.#closed) {
      this.#onEvent({ kind: "closed", code, retryable: false });
      return;
    }
    // 连接已结束：撤销待触发的重订阅，恢复交给重连（那里有全局退避）。
    this.#cancelGapResubscribe();
    this.discardStaging("connection_closed");
    const blocked = blockCauseForCloseCode(code);
    if (blocked !== null) {
      this.#onEvent({ kind: "blocked", cause: blocked, code });
      return;
    }
    // 1000/4500 等可重试；4400/4401/4408/4429 视为本次连接失败但可退避重试。
    this.#onEvent({ kind: "closed", code, retryable: isRetryableCloseCode(code), detail: reason });
  }

  #block(cause: ClientBlockCause, code: number): void {
    this.#closed = true;
    // 阻断态下不再补发任何东西：重订阅定时器必须随阻断一起撤销。
    this.#cancelGapResubscribe();
    this.#staging.discard("connection_blocked");
    this.#onEvent({ kind: "blocked", cause, code });
    this.#socket.close(code, cause);
  }

  // ── 出站 ────────────────────────────────────────────────────────────────

  /**
   * 派发一条命令（稳定 `requestId` 由调用方给出，重试复用同一 ID）。
   *
   * 信封的 `connectionSequence` 在这里**覆盖**成客户端方向计数器的下一个值：命令的出站
   * 序号属于连接层的事，调用方构造的命令体不该也不需要自己维护计数器。未认证时不覆盖——
   * 此时 §4.1 要求省略该字段，调用方给什么就发什么。
   */
  sendCommand(command: CommandMessage): void {
    if (this.#connectionId === null) {
      this.#send(command);
      return;
    }
    this.#send({ ...command, connectionSequence: this.#nextOutboundSequence() });
  }

  /** ACK 只能前进：回退值被忽略并记录诊断（§9.5）。 */
  #sendAck(cursor: Cursor): void {
    const last = this.#lastAcked;
    if (last !== null && compareCursors(cursor, last) <= 0) {
      this.#onEvent({ kind: "rejected", reason: "ack_not_advancing", detail: cursor.globalSequence });
      return;
    }
    this.#lastAcked = cursor;
    this.#acked = cursor;
    const ack: SyncAck = {
      protocolVersion: PROTOCOL_VERSION,
      type: "sync.ack",
      messageId: this.#random.uuid(),
      connectionId: this.#requireConnectionId(),
      connectionSequence: this.#nextOutboundSequence(),
      body: { cursor },
    };
    this.#send(ack);
  }

  /** 取客户端方向的下一个出站序号（从 `1` 开始严格加一，§4.1）。 */
  #nextOutboundSequence(): string {
    this.#outboundSequence += 1;
    return String(this.#outboundSequence);
  }

  #send(message: SyncWireMessage): void {
    this.#socket.send(JSON.stringify(message));
  }

  #requireConnectionId(): Uuid {
    const id = this.#connectionId;
    if (id === null) {
      throw new Error("连接尚未通过认证：缺少 connectionId");
    }
    return id;
  }
}

/** 已认证信息（`auth.authenticated.body`）。 */
export interface AuthenticatedBody {
  readonly deviceId: Uuid;
  readonly scopes: readonly string[];
  readonly serverEpoch: Uuid;
  readonly headGlobalSequence: string;
  readonly heartbeatIntervalMs: number;
  readonly limits: {
    readonly maxMessageBytes: number;
    readonly maxPromptBytes: number;
    readonly maxReplayEventsPerBatch: number;
  };
}

/** 关闭码 → 阻断原因；`null` 表示不是阻断（可退避重连）。 */
export function blockCauseForCloseCode(code: number): ClientBlockCause | null {
  if (code === CLOSE.connectionReplaced) return "replaced_by_other_connection";
  if (code === CLOSE.deviceRevoked) return "device_revoked";
  if (code === CLOSE.versionIncompatible) return "version_incompatible";
  if (code === CLOSE.identityConflict) return "host_identity_changed";
  return null;
}

/** 关闭码是否值得退避重试。 */
export function isRetryableCloseCode(code: number): boolean {
  return code === CLOSE.normal || code === CLOSE.serverUnavailable || code === CLOSE.protocolError || code === CLOSE.authRequired;
}

/** 游标比较（同一 epoch 内按序号；跨 epoch 不可比，返回 `-1` 触发重新订阅）。 */
export function compareCursors(left: Cursor, right: Cursor): number {
  if (left.serverEpoch !== right.serverEpoch) return -1;
  const l = BigInt(left.globalSequence);
  const r = BigInt(right.globalSequence);
  if (l < r) return -1;
  if (l > r) return 1;
  return 0;
}

/** feature 列表按 UTF-8 字节升序去重（transcript 的 `negotiatedFeatures` 编码要求，§5.2）。 */
export function sortedFeatures(features: readonly string[]): string[] {
  return [...new Set(features)].sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
}

/**
 * 解码一条入站文本；失败时发 `rejected` 事件并返回 `null`。
 *
 * 拒绝**不**升级为阻断态：一条坏消息不应让连接停摆，真正需要阻断的情况
 * （撤销/版本/身份）由结构化 error 与 close code 单独判定。
 */
function decodeOrReject(
  text: string,
  onEvent: (event: SyncClientEvent) => void,
): SyncWireMessage | null {
  const decoded = decodeWireMessage(text);
  if (decoded.ok) return decoded.message;
  onEvent({ kind: "rejected", reason: decoded.reason, detail: decoded.detail });
  return null;
}
