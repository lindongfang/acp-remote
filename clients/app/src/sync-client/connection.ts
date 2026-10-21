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
 * 每个阻断态的触发源各不相同，但都收敛到 `SyncClientEvent.blocked`：
 * close code 4411（被顶替）、4410（撤销）、4406（版本不兼容）、`hostProof` 校验失败
 * 或 host key 与配对记录不一致（身份变化），以及**服务端没有选中约定的子协议**
 * （§3.1，在 socket 的 `onOpen` 回调之后核 `SocketPort.negotiatedSubprotocol()`）与**必需 feature 未被选中**
 * （§5.2，在 `#onServerChallenge` 里比对 `REQUIRED_FEATURES`）。后两种与版本不兼容落到同一个
 * 阻断态，成因由事件的 `reason` 分开（见 `IncompatibleReason`）。判定完成后 `SyncClient`
 * 停止自动重连。
 */

import type {
  AuthAuthenticated,
  AuthServerChallenge,
  CommandMessage,
  CommandResultMessage,
  Cursor,
  EventMessage,
  OriginCursor,
  RemoteEventItem,
  RemoteResetReason,
  RemoteSourceStateKind,
  SessionRef,
  SyncAck,
  SyncCaughtUp,
  SyncRemoteCaughtUp,
  SyncRemoteEvent,
  SyncRemoteResetRequired,
  SyncRemoteUnavailable,
  SyncRemoteUnsubscribe,
  SyncResetRequired,
  SyncSnapshotBegin,
  SyncSnapshotChunk,
  SyncSnapshotEnd,
  SyncWireMessage,
  Uuid,
} from "../protocol";
import { stringifyJson } from "./json";
import { decodeWireMessage } from "./wire";
import { EventLedger, isImportedEventIdConsistent } from "./dedupe";
import { compareOriginCursors } from "./remote-merge";
import type {
  ClockPort,
  CancelTimer,
  DeviceIdentityLike,
  HostIdentityPort,
  RandomPort,
  SocketFactory,
  SocketPort,
  TranscriptCodec,
} from "./ports";
import { SnapshotStaging, SnapshotValidationError, type SnapshotDiscardReason, type VerifiedSnapshot } from "./snapshot";
import type { ClientBlockCause, IncompatibleReason } from "./blocking";
import { blockCauseForCloseCode, CLOSE, isRetryableCloseCode } from "./blocking";
import { AuthHandshake, PROTOCOL_VERSION, sortedFeatures } from "./handshake";

/**
 * Sync WSS 端点路径（`docs/SYNC_PROTOCOL.md` §3.1）。
 *
 * 权威值有两处同源出处：`docs/SYNC_PROTOCOL.md` 「WebSocket endpoint 为同源 `/sync/v1`」，
 * 与 `crates/server/src/sync/conn/mod.rs` 的 `pub const WS_PATH: &str = "/sync/v1";`。
 * 客户端只在**这里**写一次：出站地址的唯一构造点是下面的 `syncSocketUrl`（组合根调它），
 * e2e 检查用它写期望值——两边各自抄一个字面量就会各改各的、漏一处。
 */
export const SYNC_WS_PATH = "/sync/v1";

/**
 * Sync 的 WebSocket subprotocol（`docs/SYNC_PROTOCOL.md` §3.1）。
 *
 * 权威值有两处同源出处：`docs/SYNC_PROTOCOL.md` 「客户端必须请求 WebSocket subprotocol
 * `acp-remote.sync.v1.json`；服务端必须选择该 subprotocol，否则客户端终止连接」，与
 * `crates/server/src/sync/conn/mod.rs` 的 `pub const WS_SUBPROTOCOL: &str = "acp-remote.sync.v1.json";`
 * （接入层按它做 upgrade 白名单：`crates/server/src/transport/net/route.rs` 的
 * `register_ws(path, required_subprotocol, handler)`，未声明该 subprotocol 的升级请求被拒绝）。
 *
 * 客户端只在**这里**写一次（并以本常量作为 `DECLARED_SUBPROTOCOLS` 的唯一成员）：
 * 唯一的升级点 `socket.web.ts` 的 `new WebSocket(url, protocols)` 声明它，`endpoint-contract.test.ts`
 * 断言它与机器登记（`compatibility/errors/v1/errors.json` 的 `protocols.sync.subprotocol`）逐字相等。
 * 抄成第二份字面量就会出现「一处改了、另一处没改」的静默失败——服务端会直接拒掉升级。
 */
export const SYNC_WS_SUBPROTOCOL = "acp-remote.sync.v1.json";

/**
 * 本客户端在升级请求里声明的子协议列表（§3.1 的 v1 只约定一条）。
 *
 * 它是端口的**入参**（`SocketFactory` 的 `protocols`）：声明了什么必须可被注入、可被断言，
 * 因此这里给出唯一一份默认值，既由组合根显式传给连接（`SyncClientOptions.subprotocols`
 * 是必填字段），也作为 Web 实现在调用方未给声明时的兜底——两条路径取同一个常量，
 * 不会各自抄一份。
 */
export const DECLARED_SUBPROTOCOLS: readonly string[] = [SYNC_WS_SUBPROTOCOL];

/**
 * 规范化来源 → 同步连接地址（`https://host[:port]` → `wss://host[:port]/sync/v1`，§3.1/§8）。
 *
 * 只替换 scheme，其余原样保留：规范化来源不含 path/query/fragment（§3.2 的 `$defs.canonicalOrigin`），
 * 因此拼接不会丢掉任何来源信息。路径取自 `SYNC_WS_PATH`（唯一出处见该常量的说明），
 * 这里不重写 `/sync/v1` 字面量；调用方（组合根）也不自己拼——出站地址只有这一个构造点，
 * 断言因此可以落在**构造结果**上（`endpoint-contract.test.ts`）。
 */
export function syncSocketUrl(canonicalOrigin: string): string {
  return `${canonicalOrigin.replace(/^https:\/\//, "wss://")}${SYNC_WS_PATH}`;
}

/**
 * 发现丢失区间后的重新订阅：退避参数与连续次数上限。
 *
 * ## 触发条件（与上一轮相比已收紧）
 *
 * 触发源只有一个：账本判出 `late`——**服务端声明过「这条序号之前都已交付」，却又送来一条
 * 客户端从未呈现过的事件**（详见 `dedupe.ts` 文件头）。保留策略造成的合法空洞**不再**触发
 * 重订阅：§9.2 明文「仍可能出现的序号空洞只来自保留策略（已压缩或已清理的事件），不能据此
 * 推断隐藏事件」——按空洞重订阅只会拿回同一条事件、原地打转，并对服务端构成持续的
 * `sync.subscribe` 洪泛。
 *
 * ## 为什么仍然必须有上限而不是「一直重订阅」
 *
 * 服务端违反 barrier 声明是可能持续发生的（同一个漏投区间会被反复重投）。若不设上限，
 * 每次重订阅都只是把同一条迟到事件再送一遍，永远补不齐新的东西。因此重订阅既有退避
 * （一次比一次久）又有次数上限（到顶就停，把后续恢复交给状态层按重连退避重新来）。
 *
 * 取值依据：
 * - `baseDelayMs = 250`：能救回「服务端补投只晚了几百毫秒」这类最常见的抖动，又低于用户
 *   感知的亚秒级停顿阈值，不会把一次服务端抖动放大成恢复风暴。
 * - 倍率 2、`maxDelayMs = 4000`：250/500/1000/2000/4000，连续 5 次累计 7.75s，
 *   足以覆盖服务端一次短暂重放抖动，又不会在几秒内把服务端打穿。
 * - `maxAttempts = 5`：单条连接最多补发 5 次；到顶后上报 `gap_resubscribe_exhausted`
 *   并停止，恢复路径回到「重连 + 快照重建」，那是有全局退避的通道。
 *
 * ## 预算何时清零（P1-B）
 *
 * 只有**水位被屏障真正抬高**（`advanceTo` 返回 `advanced`）或**有新事件被呈现**才清零。
 * §9.3 要求服务端每次重放后都发一条 `sync.caught_up`，所以「收到过 `caught_up` 就清零」会让
 * 预算每轮复位、退避恒为 250ms、`maxAttempts` 永远不可达——这条安全网等于不存在。
 */
const GAP_RESUBSCRIBE = {
  baseDelayMs: 250,
  maxDelayMs: 4_000,
  maxAttempts: 5,
} as const;

/** 阻断原因与细化成因的**定义**在 `blocking.ts`（两条已认证连接共用同一份判定）。 */
export type { ClientBlockCause, IncompatibleReason };

/** 客户端向消费方（`src/state/` 与 WP7 的 feature 层）发出的事件。 */
export type SyncClientEvent =
  /** socket 已建立，开始逐连接握手。 */
  | { readonly kind: "authenticating" }
  /** 认证成功：连接级信息（`deviceId`、`serverEpoch`、限额）在此交付。 */
  | {
      readonly kind: "authenticated";
      readonly info: AuthenticatedInfo;
      /**
       * 服务端在 `auth.server_challenge` 里选中的 feature 列表。
       *
       * 消费方据此决定**要不要**带某个可选 include（例如会话历史的 `history`）：未选中的
       * feature 不能靠「发了再看反应」探测，旧服务端连取值都不认识。可选是为了不让既有
       * 构造点（测试与组合根）被迫编一份列表——缺席等价于「什么都没协商出来」。
       */
      readonly selectedFeatures?: readonly string[];
    }
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
  /**
   * 检测到丢失区间：服务端声明过「`expected` 之前都已交付」，却送来了 `received`（低于水位）。
   *
   * 该事件随后仍会被 `event` 补齐呈现（R14 不允许丢失），同时触发有界重订阅。
   * 保留策略造成的合法空洞**不会**产生本事件（§9.2）。
   */
  | { readonly kind: "gap_detected"; readonly expected: string; readonly received: string }
  /** 命令结果。 */
  | { readonly kind: "command_result"; readonly result: CommandResultMessage }
  /** 连接关闭。`retryable` 为假时不自动重连。 */
  | { readonly kind: "closed"; readonly code: number; readonly retryable: boolean; readonly detail?: string }
  /** 进入阻断态：必须停止自动重连。 */
  | {
      readonly kind: "blocked";
      readonly cause: ClientBlockCause;
      readonly code: number;
      /**
       * 细化成因，仅在 `cause === "version_incompatible"` 时给出（见 `IncompatibleReason`）。
       *
       * 它随事件一起交付，而不是等消费方从 `code` 反推：`4406` 与握手期的 feature 缺失
       * 落到同一个关闭码与同一个阻断态，只有这里能分辨是「子协议 / 版本」还是「必需 feature」。
       */
      readonly reason?: IncompatibleReason;
    }
  /** 消息被解码拒绝（诊断用；不进入阻断态，除非调用方据此升级）。 */
  | { readonly kind: "rejected"; readonly reason: string; readonly detail: string }
  // ── 远程来源流（`resource.stateless-origin.v1`，§9.7） ────────────────────
  //
  // 这些事件都只在**当前连接**上有意义：连接关闭后订阅与游标不再由连接持有（客户端运行期
  // 的连续位置由门面保存），消费方必须清除远程展示数据。远程序号是 Owner 的 `originCursor`，
  // 与 Access 本机的 `Cursor` 互不翻译。
  /** 已为某个来源会话发出（或重发）`sync.remote_subscribe`；消费方应回源读取重建。 */
  | {
      readonly kind: "remote_subscribing";
      readonly sessionRef: SessionRef;
      readonly subscriptionId: Uuid;
      readonly cursor: OriginCursor | null;
    }
  /** 一条已通过本订阅去重的来源事件。 */
  | {
      readonly kind: "remote_event";
      readonly sessionRef: SessionRef;
      readonly subscriptionId: Uuid;
      readonly event: RemoteEventItem;
    }
  /** Owner 已完成来源重放屏障；此后才是实时事件。 */
  | {
      readonly kind: "remote_caught_up";
      readonly sessionRef: SessionRef;
      readonly subscriptionId: Uuid;
      readonly cursor: OriginCursor;
    }
  /** 来源要求重建（epoch/保留期/不兼容）；`cursor` 只在 Owner 已验证 head 时出现。 */
  | {
      readonly kind: "remote_reset_required";
      readonly sessionRef: SessionRef;
      readonly subscriptionId: Uuid;
      readonly reason: RemoteResetReason;
      readonly cursor: OriginCursor | null;
    }
  /** 来源当前不可用（断线/撤销/不支持），必须清除该来源展示数据。 */
  | {
      readonly kind: "remote_unavailable";
      readonly sessionRef: SessionRef;
      readonly subscriptionId: Uuid;
      readonly error: SyncRemoteUnavailable["body"]["error"];
    }
  /** 本连接上的订阅已结束（服务端确认或客户端主动取消）。 */
  | {
      readonly kind: "remote_unsubscribed";
      readonly sessionRef: SessionRef;
      readonly subscriptionId: Uuid;
    }
  /** 某个 Owner 的来源清单失效：客户端重新查询（快照），不保留旧副本。 */
  | { readonly kind: "remote_catalog_changed"; readonly ownerNodeId: Uuid; readonly revision: string }
  /** 某个 Owner 的连接事实：`offline`/`revoked` 时清除该来源展示数据。 */
  | { readonly kind: "remote_source_state"; readonly ownerNodeId: Uuid; readonly state: RemoteSourceStateKind };

/** 客户端需要的 feature 词表（`compatibility/features/v1/features.json` 的 sync 段）。 */
export const SUPPORTED_FEATURES = [
  "acp.raw-payload.v1",
  "core.command-status.v1",
  "core.event-ack.v1",
  "core.session-activity.v1",
  "core.file-diff.v1",
  "core.local-catalog.v1",
  "core.session-create.v1",
  "core.session-history.v1",
  "core.snapshot.v1",
  "core.workspace-browse.v1",
  "core.workspace-mkdir.v1",
  "core.session-files.v1",
  "core.session-git-stats.v1",
    "core.session-git-compare.v1",
  "core.attachment-transfer.v1",
  "core.typed-media.v1",
  "core.pending-interactions.v1",
  "core.remote-history.v1",
  "resource.remote-origin.v1",
  "resource.stateless-origin.v1",
] as const;

/**
 * 客户端必需 feature：ACK 与命令状态查询缺失则无法满足 R13/R14；
 * `core.session-activity.v1` 是三态会话摘要的合同，缺失时两端对 `state` 的词表可能不一致。
 */
export const REQUIRED_FEATURES = [
  "core.command-status.v1",
  "core.event-ack.v1",
  "core.session-activity.v1",
] as const;

/**
 * 结构化历史（`session.read` 的 `history` include）所需的 feature ID。
 *
 * 客户端只在**服务端选中**它时才带这个 include：未协商却请求它，服务端会回明确的
 * 不支持类错误（`design.md` 的 Compatibility → feature 门控），而旧服务端连该取值都不认识，
 * 客户端因此不能靠「发了再看反应」来探测。
 */
export const SESSION_HISTORY_FEATURE = "core.session-history.v1";

/** 类型化媒体/消息通道（§10.4）。 */
export const TYPED_MEDIA_FEATURE = "core.typed-media.v1";

/** 可恢复的有效待处理交互（§11.7）。 */
export const PENDING_INTERACTIONS_FEATURE = "core.pending-interactions.v1";

/** 远程结构化历史回源（§9.6/§11.5）。 */
export const REMOTE_HISTORY_FEATURE = "core.remote-history.v1";

/** 附件传输（§3.4）。与 `src/protocol/attachment.ts` 的常量同值，只在这里导出便于消费方取用。 */
export { ATTACHMENT_FEATURE } from "../protocol";

/** 一条连接的构造输入（端口由组合根注入）。 */
export interface SyncConnectionOptions {
  readonly hostId: Uuid;
  readonly url: string;
  /**
   * 本连接在升级请求里声明的子协议列表（由组合根给出，见 `DECLARED_SUBPROTOCOLS`）。
   *
   * 它有两处用途，缺一都不可：交给 socket 端口作为升级请求的声明（§3.1），
   * 以及核对服务端**实际选中**的那一条（未选中即终止连接，见 socket 的 `onOpen` 回调）。
   * 必填而不是给缺省值：声明错一条与不声明都表现为「连接被拒」，只有让组合根显式给出，
   * 这两件事才不会在装配处被悄悄省略。
   */
  readonly subprotocols: readonly string[];
  readonly identity: DeviceIdentityLike;
  readonly host: HostIdentityPort;
  readonly random: RandomPort;
  readonly transcript: TranscriptCodec;
  /** 建立 socket 的工厂；Web 实现见 `socket.web.ts`。 */
  readonly openSocket: SocketFactory;
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
  readonly #onEvent: (event: SyncClientEvent) => void;
  /** §8 的握手状态机（与附件连接共用同一份实现，见 `handshake.ts`）。 */
  readonly #handshake: AuthHandshake;
  /**
   * 客户端方向的 `connectionSequence` 计数器（§4.1）。
   *
   * 认证完成后每条出站消息都必须带它，且从 `1` 开始**严格加一**；两个方向各自维护，
   * 因此这里既不能抄服务端 `auth.authenticated` 里的序号（那是服务端方向的计数器），
   * 也不能让认证前的消息消耗它（§4.1 要求认证前省略该字段）。
   */
  #outboundSequence = 0;
  /** 本连接的认证结果（serverEpoch 与限额据此生效）。 */
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
  #validatingSnapshot = false;
  #snapshotBacklog: string[] = [];
  #snapshotBacklogChars = 0;
  /** 已被确认处理的游标；ACK 只能前进（§9.5）。 */
  #acked: Cursor | null;
  #closed = false;
  /** 已 ACK 的游标，用于保证「只能前进」。 */
  #lastAcked: Cursor | null;
  /**
   * 连续丢失恢复的次数：只有「真的有进展」才清零——即呈现了一条 `new` 事件，或屏障**确实抬高**
   * 了水位（`advanceTo` 返回 `advanced`）。
   *
   * 清零是必要的：一次已闭合的丢失说明恢复通道是通的，后续零星抖动应当拿到**完整**的重试预算。
   * 而「收到过 `caught_up`」**不**算进展：§9.3 要求每次重放后都发一条，按它清零会让预算每轮
   * 复位、退避上限永远不可达（见 `GAP_RESUBSCRIBE` 的注释）。
   */
  #gapAttempts = 0;
  /** 待触发的缺口重订阅定时器；同一时刻至多一个。 */
  #gapRetry: CancelTimer | null = null;
  /**
   * 本连接上已登记的远程订阅：`subscriptionId → sessionRef`。
   *
   * 映射随连接消亡（关闭/阻断即清空）。客户端运行期的连续位置不在这里，而是在门面：
   * 连接只是投递通道，重连必须换一条新订阅。
   */
  readonly #remoteSubscriptions = new Map<Uuid, SessionRef>();
  /** 每个远程订阅在本连接上已确认的连续位置（只用于让 `remote_ack` 单调前进）。 */
  readonly #remoteAcked = new Map<Uuid, OriginCursor>();

  constructor(options: SyncConnectionOptions, staging: SnapshotStaging) {
    this.#options = options;
    this.#random = options.random;
    this.#onEvent = options.onEvent;
    this.#staging = staging;
    this.#acked = options.resumeCursor;
    this.#lastAcked = options.resumeCursor;
    this.#handshake = new AuthHandshake({
      hostId: options.hostId,
      identity: options.identity,
      host: options.host,
      random: options.random,
      transcript: options.transcript,
      subprotocols: options.subprotocols,
      supportedFeatures: [...SUPPORTED_FEATURES],
      requiredFeatures: [...REQUIRED_FEATURES],
      ...(options.clientKind === undefined ? {} : { clientKind: options.clientKind }),
      send: (message) => {
        this.#send(message);
      },
      close: (code, reason) => {
        this.close(code, reason);
      },
      onRejected: (reason, detail) => {
        this.#onEvent({ kind: "rejected", reason, detail });
      },
      onBlocked: (cause, code, reason) => {
        this.#block(cause, code, reason);
      },
    });
    this.#socket = options.openSocket({
      url: options.url,
      protocols: options.subprotocols,
      handlers: {
        onOpen: () => {
          void this.#handshake.begin(this.#socket.negotiatedSubprotocol());
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
    return this.#handshake.connectionId;
  }

  /**
   * socket 是否已经关闭。
   *
   * 认证信息与 `connectionId` 只属于**活着的**连接（§8.4：认证状态关闭后立即失效），
   * 因此消费方需要判活而不是只判「是否持有连接对象」——连接对象在关闭后仍然存在。
   */
  get closed(): boolean {
    return this.#closed;
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
    this.#clearSnapshotBacklog();
    this.#cancelGapResubscribe();
    this.#forgetRemoteSubscriptions();
    this.#socket.close(code, reason);
  }

  /** 丢弃未完成的暂存快照；`reason` 记录「为什么这次快照没提交」（§9.4）。 */
  discardStaging(reason: SnapshotDiscardReason = "reset_required"): void {
    this.#staging.discard(reason);
  }

  /** 无游标重新订阅，由服务端按现有 reset → snapshot 流程刷新清单。 */
  refreshSnapshot(): void {
    if (this.#closed || this.#authenticated === null) return;
    this.#sendSubscribe(null);
  }

  // ── 握手 ────────────────────────────────────────────────────────────────
  /** `auth.server_challenge` 的验签与 `client_proof` 由共享握手状态机完成。 */
  async #onServerChallenge(message: AuthServerChallenge): Promise<void> {
    await this.#handshake.acceptChallenge(message);
  }

  /** `auth.authenticated`：按最后确认游标订阅，进入重放追平。 */
  #onAuthenticated(message: AuthAuthenticated): void {
    if (!this.#handshake.acceptAuthenticated(message)) return;
    const handshake = this.#handshake.state;
    if (handshake === null) return;
    this.#authenticated = message.body;
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
    this.#onEvent({
      kind: "authenticated",
      info: message.body,
      selectedFeatures: handshake.selectedFeatures,
    });
    this.#onEvent({ kind: "replaying", cursor: this.#acked });
  }

  /**
   * 从最后确认游标发出 `sync.subscribe`。
   *
   * 认证后首次订阅与「序号缺口后的恢复」走**同一条**路径：两者都要从 `#acked` 重新订阅，
   * 区别只在于游标是否为空。缺口恢复必须真的重发，否则事件流会静默停摆（R14「连续恢复」）。
   */
  #sendSubscribe(cursor: Cursor | null = this.#acked): void {
    this.#send({
      protocolVersion: PROTOCOL_VERSION,
      type: "sync.subscribe",
      messageId: this.#random.uuid(),
      connectionId: this.#requireConnectionId(),
      // 客户端方向自己的计数器（§4.1），不抄服务端信封里的序号。
      connectionSequence: this.#nextOutboundSequence(),
      body: { cursor, scope: "machine" },
    });
  }

  // ── 入站分派 ──────────────────────────────────────────────────────────────

  #onMessage(text: string): void {
    if (this.#closed) return;
    if (this.#validatingSnapshot) {
      // 异步摘要校验期间仍按 wire 到达顺序处理后续帧。队列最多 256 帧、4M UTF-16
      // 字符（约 8 MiB），超限关闭后从持久游标恢复，不能无限缓存或先 ACK 后丢弃。
      if (this.#snapshotBacklog.length >= 256 || this.#snapshotBacklogChars + text.length > 4 * 1024 * 1024) {
        this.close(CLOSE.serverUnavailable, "snapshot_validation_backlog");
        this.#onEvent({ kind: "closed", code: CLOSE.serverUnavailable, retryable: true, detail: "snapshot_validation_backlog" });
        return;
      }
      this.#snapshotBacklog.push(text);
      this.#snapshotBacklogChars += text.length;
      return;
    }
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
        this.#validatingSnapshot = true;
        void this.#onSnapshotEnd(message as SyncSnapshotEnd).then(
          () => this.#drainSnapshotBacklog(),
          () => {
            this.close(CLOSE.serverUnavailable, "snapshot_validation_failed");
            this.#onEvent({ kind: "closed", code: CLOSE.serverUnavailable, retryable: true, detail: "snapshot_validation_failed" });
          },
        );
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
      case "sync.remote_subscribe":
        // 服务端不会向客户端发起订阅；形状虽可解码，角色是错的。
        this.#onEvent({ kind: "rejected", reason: "unexpected_type", detail: message.type });
        return;
      case "sync.remote_event":
        this.#onRemoteEvent(message);
        return;
      case "sync.remote_caught_up":
        this.#onRemoteCaughtUp(message);
        return;
      case "sync.remote_ack":
        // 客户端→Access 的累计确认；服务端回送一帧不改变任何状态，也不进入阻断态。
        return;
      case "sync.remote_reset_required":
        this.#onRemoteResetRequired(message);
        return;
      case "sync.remote_unsubscribe":
        this.#onRemoteUnsubscribe(message);
        return;
      case "sync.remote_unavailable":
        this.#onRemoteUnavailable(message);
        return;
      case "sync.remote_catalog_changed":
        this.#onEvent({
          kind: "remote_catalog_changed",
          ownerNodeId: message.body.ownerNodeId,
          revision: message.body.revision,
        });
        return;
      case "sync.remote_source_state":
        this.#onEvent({
          kind: "remote_source_state",
          ownerNodeId: message.body.ownerNodeId,
          state: message.body.state,
        });
        return;
      default:
        this.#onEvent({ kind: "rejected", reason: "unexpected_type", detail: message.type });
    }
  }

  #clearSnapshotBacklog(): void {
    this.#snapshotBacklog = [];
    this.#snapshotBacklogChars = 0;
  }

  #drainSnapshotBacklog(): void {
    this.#validatingSnapshot = false;
    if (this.#closed) { this.#clearSnapshotBacklog(); return; }
    while (!this.#validatingSnapshot && !this.#closed && this.#snapshotBacklog.length > 0) {
      const text = this.#snapshotBacklog.shift();
      if (text === undefined) break;
      this.#snapshotBacklogChars -= text.length;
      this.#onMessage(text);
    }
  }

  /**
   * 事件去重 + 丢失处理。
   *
   * 重复事件**不**推进水位也不触发呈现；低于权威水位却从未呈现过的事件（`late`）必须**补齐
   * 呈现**并触发有界重订阅。保留策略造成的合法空洞（§9.2）不判为丢失，也不触发重订阅。
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
    if (verdict.kind === "rejected") {
      this.#onEvent({ kind: "rejected", reason: verdict.reason, detail: message.body.globalSequence });
      return;
    }
    if (verdict.kind === "late") {
      // 水位之下却从未呈现过：这是**真实的**丢失区间证据（服务端违反 barrier 声明，
      // 或客户端此前误丢）。两条 R14 要求在这里冲突，冲突时选「不丢」——
      // 重复呈现是可见且可去重的，丢弃是静默且不可逆的。
      ledger.commit(message);
      this.#onEvent({ kind: "gap_detected", expected: verdict.watermark, received: verdict.received });
      this.#onEvent({ kind: "event", event: message });
      // 恢复必须真的做：重新订阅能确认服务端是否还压着别的未交付事件。但预算**不**因这条
      // 补齐而清零——丢失区间尚未闭合，正是要用满退避预算的时候。
      this.#scheduleGapResubscribe();
      // 不发 ACK：水位不前进，ACK 只能前进（§9.5）；该序号已被更高的已确认游标覆盖。
      return;
    }
    const cursor = ledger.commit(message);
    // 真的有进展：把重试预算清零，后续零星抖动拿到的应是完整预算。
    this.#gapAttempts = 0;
    this.#onEvent({ kind: "event", event: message });
    this.#sendAck(cursor);
  }

  /**
   * 发现丢失区间后**带退避与上限**地重新订阅。
   *
   * 只重发是不够的：重订阅能确认服务端是否还压着别的未交付事件。而不设上限则会让同一个
   * 漏投区间被无限次重投——无界重订阅既补不齐，又是对服务端的持续洪泛。因此每次恢复都比
   * 上一次更久，且到顶后停止并上报，由状态层按重连退避重新进入。
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

  /** 连接结束：远程订阅映射与连接级确认一并作废（运行期连续位置由门面保留）。 */
  #forgetRemoteSubscriptions(): void {
    this.#remoteSubscriptions.clear();
    this.#remoteAcked.clear();
  }

  /**
   * `sync.caught_up`：服务端给出的**权威屏障**（§9.3 第 4 步）。
   *
   * 屏障声明「这条序号之前对本设备可见的事件都已交付」。因此：
   *
   * - 屏障**之下**未投递的空洞已被服务端消解（§9.5：即使某段序号全部因权限过滤而不可见，
   *   客户端也可以 ACK `caught_up.cursor`），水位必须采纳它——否则「已确认游标」与账本水位
   *   成为两份会漂移的拷贝（详见 `EventLedger.advanceTo` 的注释）。
   * - 屏障**之上**到达的未呈现事件仍会被判 `late` 并补齐，因此采纳屏障不关闭丢失检测。
   * - 跨 epoch 的屏障**不可采纳**（§9.4：epoch 变更意味着事件库重建，序号不可比）。账本
   *   与本方法必须给出**同一个**判断：账本拒绝时既不 ACK 也不宣告 online，只上报原因。
   */
  #onCaughtUp(message: SyncCaughtUp): void {
    const barrier = message.body.cursor;
    const ledger = this.#ledger;
    if (ledger === null) {
      this.#onEvent({ kind: "rejected", reason: "caught_up_before_auth", detail: barrier.globalSequence });
      return;
    }
    const advance = ledger.advanceTo(barrier);
    if (advance.kind === "rejected") {
      this.#onEvent({ kind: "rejected", reason: `caught_up_${advance.reason}`, detail: barrier.globalSequence });
      return;
    }
    // 预算清零的条件是「水位确实被抬高」，不是「收到过 caught_up」：§9.3 要求每次重订阅后都
    // 有一条 `caught_up`，按后者清零会让预算每轮复位、退避上限永远不可达（P1-B）。
    if (advance.kind === "advanced") this.#gapAttempts = 0;
    // ACK 只能前进（§9.5）：重复或更旧的屏障不重发 ACK，也不让 `#acked` 回退。
    if (this.#moveAcked(barrier)) this.#sendAck(barrier);
    this.#onEvent({ kind: "online", cursor: this.#acked ?? barrier });
  }

  /**
   * 只前进不后退地采纳一条屏障游标为已确认游标（§9.5）。
   *
   * epoch 不同一律拒绝：跨 epoch 的序号不可比（§9.4），把外来游标写进 `#acked` 会让随后每条
   * 真实事件的 ACK 都被判回退，一条也发不出去。
   *
   * @returns 已确认游标是否真的前进了。
   */
  #moveAcked(barrier: Cursor): boolean {
    const current = this.#acked;
    if (current !== null && (current.serverEpoch !== barrier.serverEpoch || compareCursors(barrier, current) <= 0)) {
      return false;
    }
    this.#acked = barrier;
    return true;
  }

  /**
   * `sync.remote_event`：只接受当前连接上已登记订阅的帧。
   *
   * 订阅已被替换（新 `subscriptionId`）或已取消后迟到的帧不属于任何当前订阅，按未知订阅拒绝；
   * 这不是协议错误，也不进入阻断态。`originEventId` 的去重在门面（跨连接窗口）完成，
   * 这里只保证「帧属于当前订阅、来源与登记一致」。
   */
  #onRemoteEvent(message: SyncRemoteEvent): void {
    const ref = this.#remoteSubscriptions.get(message.body.subscriptionId);
    if (ref === undefined) {
      this.#onEvent({
        kind: "rejected",
        reason: "unknown_remote_subscription",
        detail: message.body.subscriptionId,
      });
      return;
    }
    const event = message.body.event;
    if (event.sessionRef.ownerNodeId !== ref.ownerNodeId || event.sessionRef.sessionId !== ref.sessionId) {
      this.#onEvent({
        kind: "rejected",
        reason: "remote_source_mismatch",
        detail: `${event.sessionRef.ownerNodeId}/${event.sessionRef.sessionId}`,
      });
      return;
    }
    this.#onEvent({
      kind: "remote_event",
      sessionRef: ref,
      subscriptionId: message.body.subscriptionId,
      event,
    });
    this.#sendRemoteAck(message.body.subscriptionId, {
      originEpoch: event.originEpoch,
      originSequence: event.originSequence,
    });
  }

  #onRemoteCaughtUp(message: SyncRemoteCaughtUp): void {
    const ref = this.#remoteSubscriptions.get(message.body.subscriptionId);
    if (ref === undefined) {
      this.#onEvent({
        kind: "rejected",
        reason: "unknown_remote_subscription",
        detail: message.body.subscriptionId,
      });
      return;
    }
    this.#onEvent({
      kind: "remote_caught_up",
      sessionRef: ref,
      subscriptionId: message.body.subscriptionId,
      cursor: message.body.cursor,
    });
    this.#sendRemoteAck(message.body.subscriptionId, message.body.cursor);
  }

  #onRemoteResetRequired(message: SyncRemoteResetRequired): void {
    const ref = this.#remoteSubscriptions.get(message.body.subscriptionId);
    if (ref === undefined) {
      this.#onEvent({
        kind: "rejected",
        reason: "unknown_remote_subscription",
        detail: message.body.subscriptionId,
      });
      return;
    }
    // 重置后的旧确认不可再用于任何位置：本连接对这条订阅的累计确认从零开始。
    this.#remoteAcked.delete(message.body.subscriptionId);
    this.#onEvent({
      kind: "remote_reset_required",
      sessionRef: ref,
      subscriptionId: message.body.subscriptionId,
      reason: message.body.reason,
      cursor: message.body.cursor,
    });
  }

  #onRemoteUnsubscribe(message: SyncRemoteUnsubscribe): void {
    const ref = this.#remoteSubscriptions.get(message.body.subscriptionId);
    if (ref === undefined) return;
    this.#remoteSubscriptions.delete(message.body.subscriptionId);
    this.#remoteAcked.delete(message.body.subscriptionId);
    this.#onEvent({
      kind: "remote_unsubscribed",
      sessionRef: ref,
      subscriptionId: message.body.subscriptionId,
    });
  }

  #onRemoteUnavailable(message: SyncRemoteUnavailable): void {
    const ref = this.#remoteSubscriptions.get(message.body.subscriptionId);
    if (ref === undefined) {
      this.#onEvent({
        kind: "rejected",
        reason: "unknown_remote_subscription",
        detail: message.body.subscriptionId,
      });
      return;
    }
    // 该订阅在当前连接上已无法继续：移除映射，后续迟到帧不再应用（运行期位置仍由门面保留）。
    this.#remoteSubscriptions.delete(message.body.subscriptionId);
    this.#remoteAcked.delete(message.body.subscriptionId);
    this.#onEvent({
      kind: "remote_unavailable",
      sessionRef: ref,
      subscriptionId: message.body.subscriptionId,
      error: message.body.error,
    });
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
      if (this.#closed) return;
      // 快照 cursor 之后的事件会补发，随后 `sync.caught_up`（§9.4）。
      // 快照 cursor 同样是屏障：快照涵盖的序号之前的内容都已落进已完成状态，账本水位必须一并
      // 推进，否则恢复游标与水位分叉（与 #onCaughtUp 同理）。跨 epoch 或序号非法的快照游标
      // 不采纳：它与账本对同一条游标的判断必须一致。
      //
      // 判定必须**先于** `snapshot_verified`：消费方（门面）一收到该事件就把快照的 cursor
      // 写成恢复点，若账本随后拒绝这条屏障，被写入的就是一条越界或跨 epoch 的游标——它会在
      // 下次连接的 `resumeFrom` 构造期让 `new EventLedger(...)` 抛 `sequence_out_of_range`，
      // 异常逃出 socket 回调后连接再也建立不起来。因此「未被账本采纳的快照」根本不交付。
      const barrier = verified.cursor;
      const advance = this.#ledger?.advanceTo(barrier);
      if (advance !== undefined && advance.kind === "rejected") {
        this.#onEvent({ kind: "rejected", reason: `snapshot_barrier_${advance.reason}`, detail: barrier.globalSequence });
        return;
      }
      this.#onEvent({ kind: "snapshot_verified", snapshot: verified });
      this.#moveAcked(barrier);
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
        this.#block("version_incompatible", CLOSE.versionIncompatible, "protocol_version_unsupported");
        return;
      case "protocol.feature_required":
        // 服务端在握手期明确回「缺必需 feature」（§5.2）：与「版本不兼容」落到同一个阻断态
        // （两者都要换一端），但成因不同，`reason` 必须区分开。
        this.#block("version_incompatible", CLOSE.versionIncompatible, "required_features_missing");
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
    // socket 已由服务端关闭：认证状态随之失效（§8.4），消费方据此不再拿 `connectionId`。
    // 之前只置位客户端主动 `close()` 的路径，服务端关闭这条会留下一个「仍在线」的连接对象。
    this.#closed = true;
    this.#clearSnapshotBacklog();
    this.#forgetRemoteSubscriptions();
    // 连接已结束：撤销待触发的重订阅，恢复交给重连（那里有全局退避）。
    this.#cancelGapResubscribe();
    this.discardStaging("connection_closed");
    const blocked = blockCauseForCloseCode(code);
    if (blocked !== null) {
      // 这里**不**经 `#block`：socket 已经关了，再调一次 `close()` 只会对一条死连接发关闭帧。
      // §12.3 的 `4406` 就是「协议版本不兼容」本身，因此这一条带细化原因；其余阻断关闭码
      // （4410/4409/4411）没有可再分的取值。
      const reason = blocked === "version_incompatible" ? "protocol_version_unsupported" : null;
      if (reason === null) {
        this.#onEvent({ kind: "blocked", cause: blocked, code });
      } else {
        this.#onEvent({ kind: "blocked", cause: blocked, code, reason });
      }
      return;
    }
    // 1000/4500 等可重试；4400/4401/4408/4429 视为本次连接失败但可退避重试。
    this.#onEvent({ kind: "closed", code, retryable: isRetryableCloseCode(code), detail: reason });
  }

  /**
   * 进入阻断态。
   *
   * `reason` 是 `version_incompatible` 一族下的细化成因，随事件一起交付（其余原因没有可再分的
   * 取值，省略它）。阻断的语义不变：停止补发、丢弃未完成快照、并**终止**这条 socket。
   */
  #block(cause: ClientBlockCause, code: number, reason?: IncompatibleReason): void {
    this.#closed = true;
    // 阻断态下不再补发任何东西：重订阅定时器必须随阻断一起撤销。
    this.#cancelGapResubscribe();
    this.#forgetRemoteSubscriptions();
    this.#staging.discard("connection_blocked");
    if (reason === undefined) {
      this.#onEvent({ kind: "blocked", cause, code });
    } else {
      this.#onEvent({ kind: "blocked", cause, code, reason });
    }
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
    if (this.#handshake.connectionId === null) {
      this.#send(command);
      return;
    }
    this.#send({ ...command, connectionSequence: this.#nextOutboundSequence() });
  }

  /**
   * 在当前连接上订阅一个远程来源会话（§9.7）。
   *
   * `subscriptionId` 由门面生成：同一条连接上重新订阅必须换新 ID，旧 ID 的后续帧随即
   * 不再属于任何当前订阅（`#remoteSubscriptions` 只保留一份映射）。未认证时**不登记、
   * 不发送**——认证前的信封没有 `connectionId`，服务端也无从绑定设备。
   */
  sendRemoteSubscribe(input: {
    readonly subscriptionId: Uuid;
    readonly sessionRef: SessionRef;
    readonly cursor: OriginCursor | null;
  }): boolean {
    if (this.#closed || this.#authenticated === null) return false;
    this.#remoteSubscriptions.set(input.subscriptionId, input.sessionRef);
    this.#remoteAcked.delete(input.subscriptionId);
    this.#send({
      protocolVersion: PROTOCOL_VERSION,
      type: "sync.remote_subscribe",
      messageId: this.#random.uuid(),
      connectionId: this.#requireConnectionId(),
      connectionSequence: this.#nextOutboundSequence(),
      body: {
        subscriptionId: input.subscriptionId,
        sessionRef: input.sessionRef,
        cursor: input.cursor,
      },
    });
    return true;
  }

  /** 取消本连接上的一个远程订阅；映射立即移除，迟到帧不会再被当成有效事件。 */
  sendRemoteUnsubscribe(subscriptionId: Uuid): void {
    if (!this.#remoteSubscriptions.has(subscriptionId)) return;
    this.#remoteSubscriptions.delete(subscriptionId);
    this.#remoteAcked.delete(subscriptionId);
    if (this.#closed || this.#authenticated === null) return;
    this.#send({
      protocolVersion: PROTOCOL_VERSION,
      type: "sync.remote_unsubscribe",
      messageId: this.#random.uuid(),
      connectionId: this.#requireConnectionId(),
      connectionSequence: this.#nextOutboundSequence(),
      body: { subscriptionId },
    });
  }

  /**
   * 本连接上对某个订阅的累计确认（`sync.remote_ack`）。
   *
   * 它只表示「这条连接已把该位置之前的事件交给上层」，不落盘、不代表用户已读、也不推进
   * Owner head（§9.7/D5）。因此回退或跨 epoch 的确认直接忽略，不报错也不发送。
   */
  #sendRemoteAck(subscriptionId: Uuid, cursor: OriginCursor): void {
    const last = this.#remoteAcked.get(subscriptionId);
    if (last !== undefined) {
      const comparison = compareOriginCursors(cursor, last);
      if (comparison === null || comparison <= 0) return;
    }
    this.#remoteAcked.set(subscriptionId, cursor);
    this.#send({
      protocolVersion: PROTOCOL_VERSION,
      type: "sync.remote_ack",
      messageId: this.#random.uuid(),
      connectionId: this.#requireConnectionId(),
      connectionSequence: this.#nextOutboundSequence(),
      body: { subscriptionId, cursor },
    });
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
    // 精确整数（表单值）必须以裸数字落进 wire：`JSON.stringify` 会把它变成对象或改值。
    this.#socket.send(stringifyJson(message));
  }

  #requireConnectionId(): Uuid {
    const id = this.#handshake.connectionId;
    if (id === null) {
      throw new Error("连接尚未通过认证：缺少 connectionId");
    }
    return id;
  }
}

/** 关闭码判定在 `blocking.ts`（附件连接共用同一份表）。 */
export { blockCauseForCloseCode, isRetryableCloseCode };

/** 游标比较（同一 epoch 内按序号；跨 epoch 不可比，返回 `-1` 触发重新订阅）。 */
export function compareCursors(left: Cursor, right: Cursor): number {
  if (left.serverEpoch !== right.serverEpoch) return -1;
  const l = BigInt(left.globalSequence);
  const r = BigInt(right.globalSequence);
  if (l < r) return -1;
  if (l > r) return 1;
  return 0;
}

/** feature 升序去重的定义在 `handshake.ts`（transcript 编码与连接对外导出共用一份）。 */
export { sortedFeatures };

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
