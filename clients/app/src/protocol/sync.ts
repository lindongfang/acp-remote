/**
 * `schemas/sync/v1/sync.schema.json` 的严格 TypeScript 镜像：订阅、快照、ACK 与远程来源流。
 *
 * 快照资源词表按 `design.md` D1 收窄为 `sessions`/`workspaces`/`agents` 三类**清单**资源；
 * 六类会话明细资源（`messages`/`turns`/`pending_interactions`/`config_options`/`capabilities`/
 * `history`）**不进快照**，其元素形状仍由下面的 `SnapshotItem*` 定义供 `session.read` 结果复用。
 *
 * `sync.remote_*` 是 `resource.stateless-origin.v1` 的加性帧族（`docs/SYNC_PROTOCOL.md` §9.7）：
 * 它直接以 Owner 的 `originCursor` 订阅与 ACK，**不**分配 Access 本机序号。本文件只镜像 wire 形状；
 * 订阅状态机、重建缓冲与 persistence 边界属于 `src/sync-client/`。
 */

import type {
  AgentCatalogEntry,
  AgentContentBlock,
  Base64Url32,
  ConfigOptionView,
  Cursor,
  DecimalString,
  ModeRef,
  OriginCursor,
  PositiveDecimalString,
  PreparationReceipt,
  PublicError,
  RawAcp,
  SessionRef,
  SessionSummary,
  Timestamp,
  Uuid,
  WorkspaceCatalogEntry,
} from "./common";
import type { PreparationConfigSelection } from "./command";

/** 认证后消息的公共信封（`$defs.postAuthBase`）。 */
export interface PostAuthEnvelope {
  readonly protocolVersion: 1;
  readonly messageId: Uuid;
  readonly connectionId: Uuid;
  readonly connectionSequence: DecimalString;
}

/** `$defs.resetRequired.reason` 与 `$defs.snapshotRequest.body.reason` 共用的封闭词表。 */
export type ResetReason =
  | "initial_sync"
  | "epoch_mismatch"
  | "cursor_expired"
  | "cache_incompatible";

/** `$defs.subscribe`。 */
export interface SyncSubscribe extends PostAuthEnvelope {
  readonly type: "sync.subscribe";
  readonly body: {
    readonly cursor: Cursor | null;
    readonly scope: "machine";
  };
}

/** `$defs.caughtUp`：追平完成，此后转入实时事件。 */
export interface SyncCaughtUp extends PostAuthEnvelope {
  readonly type: "sync.caught_up";
  readonly body: { readonly cursor: Cursor };
}

/** `$defs.resetRequired`。 */
export interface SyncResetRequired extends PostAuthEnvelope {
  readonly type: "sync.reset_required";
  readonly body: {
    readonly reason: ResetReason;
    readonly snapshotAvailable: boolean;
  };
}

/** `$defs.snapshotRequest`。 */
export interface SyncSnapshotRequest extends PostAuthEnvelope {
  readonly type: "sync.snapshot_request";
  readonly body: { readonly reason: ResetReason };
}

/** `$defs.snapshotResource`：封闭词表，只含三类清单资源（`design.md` D1）。 */
export type SnapshotResource = "sessions" | "workspaces" | "agents";

/** `$defs.snapshotChunkCount`：`1..3`，与 `SnapshotResource` 的取值数一致。 */
export type SnapshotChunkCount = 1 | 2 | 3;

/** `$defs.snapshotBegin`。 */
export interface SyncSnapshotBegin extends PostAuthEnvelope {
  readonly type: "sync.snapshot_begin";
  readonly body: {
    readonly snapshotId: Uuid;
    readonly cursor: Cursor;
    readonly schemaVersion: 1;
    readonly chunkCount: SnapshotChunkCount;
  };
}

/** `$defs.snapshotItem.messages`：`session.read` 的 `resources.messages` 元素。 */
export interface SnapshotItemMessages {
  readonly channel?: import("./media").MessageChannel;
  readonly partial?: import("./media").PartialResult;
  readonly messageId: Uuid;
  readonly sessionId: Uuid;
  readonly role: "user" | "agent";
  readonly content: readonly AgentContentBlock[];
  readonly status: string;
  readonly createdAt: Timestamp;
  readonly turnId: Uuid | null;
}

/** `$defs.snapshotItem.turns`。 */
export interface SnapshotItemTurns {
  readonly turnId: Uuid;
  readonly sessionId: Uuid;
  readonly state: string;
  readonly createdAt: Timestamp;
  readonly startedAt: Timestamp | null;
  readonly completedAt: Timestamp | null;
  readonly terminalError: PublicError | null;
}

/** `$defs.snapshotItem.pending_interactions`。 */
export interface SnapshotItemPendingInteractions {
  readonly interactionId: Uuid;
  readonly sessionId: Uuid;
  readonly kind: "permission" | "elicitation";
  readonly state: string;
  readonly schema: Record<string, unknown>;
  readonly createdAt: Timestamp;
}

/** `$defs.snapshotItem.config_options`。 */
export interface SnapshotItemConfigOptions {
  readonly sessionId: Uuid;
  readonly configOptions: readonly ConfigOptionView[];
  readonly version: DecimalString;
}

/**
 * `$defs.snapshotItem.history`：会话的**结构化历史条目**（`session.read` 的 `resources.history`）。
 *
 * 它与 `event.schema.json` 的 `body.payload` 同形——`view` 是与实时事件**同一套**解析入口
 * （`projectView`）的结构化视图，`acp` 按 `acp.raw-payload.v1` 的协商结果出现（缺席或
 * `rawUnavailable` 都表示原文不可用，服务端 MUST NOT 用占位内容冒充）。因此客户端只需
 * 一套卡片与一套降级判定，实时与历史只剩**数据来源**的差异（`design.md` D1）。
 *
 * 两处与事件信封的差异是刻意的：历史条目没有 `origin`/`globalSequence`（它们不是本次连接的
 * 投递记录），只有**会话内序号** `sessionSequence`——分页游标 `beforeHistory` 就是它（D2）。
 */
export interface SnapshotItemHistory {
  readonly eventId: Uuid;
  readonly eventType: string;
  readonly sessionSequence: DecimalString;
  readonly createdAt: Timestamp;
  readonly view: Record<string, unknown>;
  readonly acp?: RawAcp;
}

/**
 * `$defs.snapshotItem.capabilities`：`agentCapabilities` 与 `brokerAdditions` 是 schema 里的
 * 开放对象（`{"type":"object"}`），因此保留 `Record<string, unknown>`。
 */
export interface SnapshotItemCapabilities {
  readonly sessionId: Uuid;
  readonly agentCapabilities: Record<string, unknown>;
  readonly brokerAdditions: Record<string, unknown>;
}

/**
 * `$defs.snapshotItem.workspaces` / `.agents`：`workspaces` 的元素是 `workspaceCatalogEntry`
 * ——`directoryId` 与规范化路径 `path` 始终成对出现（`docs/SYNC_PROTOCOL.md` §9.4）。`agents`
 * 的元素是 `agentCatalogEntry`，与快照之外的用途共用同一份值对象。
 */
export type SnapshotItemWorkspaces = WorkspaceCatalogEntry;
export type SnapshotItemAgents = AgentCatalogEntry;

/**
 * `$defs.snapshotChunk`：`resource` 与 `items` 的元素类型由 schema 的 `allOf`/`if`/`then`
 * 按 `resource` 取值绑定，这里用判别联合表达同一约束。
 */
export type SyncSnapshotChunk =
  | (PostAuthEnvelope & {
      readonly type: "sync.snapshot_chunk";
      readonly body: {
        readonly snapshotId: Uuid;
        readonly chunkIndex: DecimalString;
        readonly resource: "sessions";
        readonly items: readonly SessionSummary[];
      };
    })
  | (PostAuthEnvelope & {
      readonly type: "sync.snapshot_chunk";
      readonly body: {
        readonly snapshotId: Uuid;
        readonly chunkIndex: DecimalString;
        readonly resource: "workspaces";
        readonly items: readonly SnapshotItemWorkspaces[];
      };
    })
  | (PostAuthEnvelope & {
      readonly type: "sync.snapshot_chunk";
      readonly body: {
        readonly snapshotId: Uuid;
        readonly chunkIndex: DecimalString;
        readonly resource: "agents";
        readonly items: readonly SnapshotItemAgents[];
      };
    });

/** `$defs.snapshotEnd`：`snapshotDigest` 是整份快照拼完后的 sha256（无填充 base64url）。 */
export interface SyncSnapshotEnd extends PostAuthEnvelope {
  readonly type: "sync.snapshot_end";
  readonly body: {
    readonly snapshotId: Uuid;
    readonly cursor: Cursor;
    readonly chunkCount: SnapshotChunkCount;
    readonly snapshotDigest: string;
  };
}

/** `$defs.ack`。 */
export interface SyncAck extends PostAuthEnvelope {
  readonly type: "sync.ack";
  readonly body: { readonly cursor: Cursor };
}

// ── 查看控制帧（`core.local-session-lifecycle.v1`，§11.8） ─────────────────────

/**
 * `$defs.viewVersion`：客户端单调递增的整数（0..=2^53−1）。
 *
 * 同版本同载荷的重试幂等；低于当前版本的声明被服务端忽略并回 `stale`。改变选择（含
 * `A → B`、`A → null`、`null → A`）MUST 递增版本。
 */
export type ViewVersion = number;

/**
 * `$defs.sessionView`：已认证连接声明自己当前查看哪个会话（`sessionId: null` 表示离开）。
 *
 * 它只更新运行期的查看者与租约，**不**启动 Agent、不写事件、不改变会话状态；激活必须另行
 * 派发 `session.resume`。同一连接最多一个当前查看者。
 */
export interface SyncSessionView extends PostAuthEnvelope {
  readonly type: "session.view";
  readonly body: {
    readonly sessionId: Uuid | null;
    readonly viewVersion: ViewVersion;
  };
}

/** `$defs.sessionViewResult.body.status` 的封闭词表。 */
export type SessionViewStatus = "accepted" | "stale";

/**
 * `$defs.sessionViewResult`：服务端对同一个 `viewVersion` 的应答。
 *
 * `accepted` 必须给出**未来**的 `leaseExpiresAt`；`stale` 的 `leaseExpiresAt` 必须为 `null`，
 * 客户端不得据此恢复查看状态。`sessionId` 逐字回显被应答的声明，因此离页应答是 `sessionId: null`
 * ——它**不是**「正在查看空会话」。
 */
export interface SyncSessionViewResult extends PostAuthEnvelope {
  readonly type: "session.view.result";
  readonly body: {
    readonly sessionId: Uuid | null;
    readonly viewVersion: ViewVersion;
    readonly status: SessionViewStatus;
    readonly leaseExpiresAt: Timestamp | null;
  };
}

// ── 远程来源流（`resource.stateless-origin.v1`，§9.7） ──────────────────────
//
// 共享形状 `originCursor`/`sessionRef`/`messageCoverage` 的镜像在 `./common`；本模块只镜像
// `sync.schema.json` 自己的 `$defs`。

/** `$defs.remoteResetRequired.body.reason`：复用 `reset_required` 词表，但不含 `initial_sync`。 */
export type RemoteResetReason = "epoch_mismatch" | "cursor_expired" | "cache_incompatible";

/** `$defs.remoteSourceState.body.state`。 */
export type RemoteSourceStateKind = "online" | "offline" | "revoked";

/**
 * `$defs.remoteEvent` 的 `event.payload`：与 Node Link `resource.event.payload` 同义。
 *
 * schema 用 `anyOf` 要求 `view`/`acp` **至少出现一个**，这里用联合表达同一约束：`view` 是开放视图
 * （未登记字段原样保留），`acp` 是逐字保留的 ACP 原文。`payload` 内部取值域归 `projectView()` 与
 * 降级路径判定，本层只描述形状。
 */
export type RemoteEventPayload =
  | { readonly view: Record<string, unknown>; readonly acp?: RawAcp }
  | { readonly view?: Record<string, unknown>; readonly acp: RawAcp };

/** `$defs.remoteEvent` 的 `event`：**不带** Access 的 `globalSequence`/`sessionSequence`。 */
export interface RemoteEventItem {
  readonly originEventId: Uuid;
  readonly originEpoch: Uuid;
  readonly originSequence: DecimalString;
  readonly sessionRef: SessionRef;
  /** 与 `resource.event.eventType` 同一形状：`^[a-z0-9_.-]{1,128}$`。 */
  readonly eventType: string;
  readonly payloadDigest: Base64Url32;
  readonly createdAt: Timestamp;
  readonly payload: RemoteEventPayload;
}

/** `$defs.remoteSubscribe`：`cursor` 为 `null` 表示首次重建，非空表示从来源位置增量重放。 */
export interface SyncRemoteSubscribe extends PostAuthEnvelope {
  readonly type: "sync.remote_subscribe";
  readonly body: {
    readonly subscriptionId: Uuid;
    readonly sessionRef: SessionRef;
    readonly cursor: OriginCursor | null;
  };
}

/** `$defs.remoteEvent`。 */
export interface SyncRemoteEvent extends PostAuthEnvelope {
  readonly type: "sync.remote_event";
  readonly body: {
    readonly subscriptionId: Uuid;
    readonly event: RemoteEventItem;
  };
}

/** `$defs.remoteCaughtUp`：Owner 已完成来源重放屏障，此后才是实时事件。 */
export interface SyncRemoteCaughtUp extends PostAuthEnvelope {
  readonly type: "sync.remote_caught_up";
  readonly body: {
    readonly subscriptionId: Uuid;
    readonly cursor: OriginCursor;
  };
}

/** `$defs.remoteAck`：仅本次连接该订阅的连续处理进度，不落盘、不表示可恢复索引。 */
export interface SyncRemoteAck extends PostAuthEnvelope {
  readonly type: "sync.remote_ack";
  readonly body: {
    readonly subscriptionId: Uuid;
    readonly cursor: OriginCursor;
  };
}

/**
 * `$defs.remoteResetRequired`：客户端必须回源 `session.read` 重建。
 *
 * `cursor` 只在 Owner 已验证有效 head 时出现，否则为 `null`；`cache_incompatible` 只能由这一帧
 * 表达，`sync.cursor_invalid` 的 `details.reason` 没有该取值。
 */
export interface SyncRemoteResetRequired extends PostAuthEnvelope {
  readonly type: "sync.remote_reset_required";
  readonly body: {
    readonly subscriptionId: Uuid;
    readonly reason: RemoteResetReason;
    readonly cursor: OriginCursor | null;
  };
}

/** `$defs.remoteUnsubscribe`：取消本连接上的订阅，不改变 Owner 侧持久状态。 */
export interface SyncRemoteUnsubscribe extends PostAuthEnvelope {
  readonly type: "sync.remote_unsubscribe";
  readonly body: { readonly subscriptionId: Uuid };
}

/** `$defs.remoteUnavailable`：断线、撤销与不支持分别复用既有错误码，不新造词表。 */
export interface SyncRemoteUnavailable extends PostAuthEnvelope {
  readonly type: "sync.remote_unavailable";
  readonly body: {
    readonly subscriptionId: Uuid;
    readonly error: PublicError;
  };
}

/** `$defs.remoteCatalogChanged`：来源清单失效，客户端重新查询，不携带旧资源副本。 */
export interface SyncRemoteCatalogChanged extends PostAuthEnvelope {
  readonly type: "sync.remote_catalog_changed";
  readonly body: {
    readonly ownerNodeId: Uuid;
    readonly revision: DecimalString;
  };
}

/** `$defs.remoteSourceState`：来源连接事实，不是 owned 业务事件，只存在于连接期。 */
export interface SyncRemoteSourceState extends PostAuthEnvelope {
  readonly type: "sync.remote_source_state";
  readonly body: {
    readonly ownerNodeId: Uuid;
    readonly state: RemoteSourceStateKind;
  };
}

// ── 准备控制帧（`core.session-preparation.v1`，§11.9；计划/待实现） ──────────
//
// 准备是**首条消息提交前**的临时阶段：Owner 分配最终 `sessionId` 并签发凭据，可以建立临时
// Agent 端点取得真实模式/配置，但不持久化会话记录。本节只镜像 wire 形状；生命周期、租约与
// 容量属于 `core`/`server::sync`。未协商该 feature 的连接不发送也不接收这些帧（§5.3）。

/** `$defs.preparationAction`：准备控制面的闭合 action 词表。 */
export type PreparationAction = "allocate" | "restore" | "configure" | "renew" | "release";

/**
 * `$defs.preparationWorkspaceRef`：`allocate` 的目录输入，`directoryId` 与 `workspacePath`
 * **恰好一个**（与 `session.create` 的目录来源同义）。规范化结果进入凭据 claims。
 */
export type PreparationWorkspaceRef =
  | { readonly directoryId: string }
  | { readonly workspacePath: string };

/**
 * `$defs.sessionPreparation.body` 的 action 分支：每个分支都带 `operationId`/`action`，
 * 但只有非 `allocate` 分支带 `sessionId`——正式 ID 由 Owner 在 `allocate` 时分配并在结果中
 * 回传，客户端不得自行指定。`restore` 的 `knownRuntimeEpoch`/`knownGeneration` 只作旧状态提示
 * 与操作去重，不参与拒绝判定；`configure` 至少要给 `mode` 或 `configOptions` 之一。
 */
export type SessionPreparationDetail =
  | {
      readonly action: "allocate";
      readonly agentId: string;
      readonly workspace: PreparationWorkspaceRef;
      readonly tabNonce: Base64Url32;
    }
  | {
      readonly action: "restore";
      readonly sessionId: Uuid;
      readonly receipt: PreparationReceipt;
      readonly knownRuntimeEpoch?: Uuid;
      readonly knownGeneration?: PositiveDecimalString;
    }
  | {
      readonly action: "configure";
      readonly sessionId: Uuid;
      readonly receipt: PreparationReceipt;
      readonly runtimeEpoch: Uuid;
      readonly generation: PositiveDecimalString;
      readonly mode?: string;
      readonly configOptions?: PreparationConfigSelection;
    }
  | {
      readonly action: "renew";
      readonly sessionId: Uuid;
      readonly receipt: PreparationReceipt;
      readonly runtimeEpoch: Uuid;
      readonly generation: PositiveDecimalString;
    }
  | {
      readonly action: "release";
      readonly sessionId: Uuid;
      readonly receipt: PreparationReceipt;
      readonly runtimeEpoch: Uuid;
      readonly generation: PositiveDecimalString;
    };

/**
 * 把公共 body 字段（`operationId`/`action`）**分配到每个 action/status 分支**。
 *
 * 直接写 `DetailUnion & { ... }` 会让 `Extract<Body, { action: "allocate" }>` 无法按键收窄；
 * 这里用分配条件类型得到真正的判别联合，`keyof` 与 `Extract` 才能逐分支工作。
 */
type WithPreparationEnvelope<T> = T extends unknown
  ? T & { readonly operationId: Uuid; readonly action: PreparationAction }
  : never;

/** `$defs.sessionPreparation.body`：`operationId`/`action` + 该 action 的闭合字段。 */
export type SessionPreparationBody = WithPreparationEnvelope<SessionPreparationDetail>;

/**
 * `$defs.sessionPreparation`：准备控制请求。
 *
 * 所有分支都带 `operationId`（连接期有界去重；同 id 不同参数按 `command.idempotency_conflict`
 * 拒绝）。它只作用于**临时**控制面：不产生 owned 命令记录或全局事件。
 */
export interface SyncSessionPreparation extends PostAuthEnvelope {
  readonly type: "session.preparation";
  readonly body: SessionPreparationBody;
}

/** `$defs.sessionPreparationResult.body.status` 的封闭五态。 */
export type PreparationStatus = "ready" | "committed" | "stale" | "released" | "failed";

/** `status = "stale"` 的资源代失效原因（`$defs.sessionPreparationResult` 的内联词表）。 */
export type PreparationStaleReason = "runtime_epoch_mismatch" | "generation_mismatch" | "released";

/**
 * `$defs.sessionPreparationResult` 的 status 判别分支（闭合，不能用多个可空字段拼出非法组合）。
 *
 * - `ready`：`modes`/`configOptions` 是 Agent 确认过的**真实**值；未提供的选项是空数组，不伪造。
 * - `committed`：同 ID 已被首次提交正式受理。**不**携带临时代/租约/配置/凭据；`firstSubmissionId`
 *   只在这一支出现，它是服务端在受理事务生成的关联键（不是附件清单的 `submissionId`）。
 * - `stale`：唯一来源是 `configure`/`renew`/`release` 与附件 `begin`/`manifest_begin`。
 * - `released`：临时资源释放，不代表 ID 已提交。
 * - `failed`：认证后的业务失败（含 `preparation.store_rebuilt`），不能隐含表达「已提交」。
 */
export type PreparationOutcome =
  | {
      readonly status: "ready";
      readonly receipt: PreparationReceipt;
      readonly runtimeEpoch: Uuid;
      readonly generation: PositiveDecimalString;
      readonly modes: readonly ModeRef[];
      readonly configOptions: readonly ConfigOptionView[];
      readonly leaseExpiresAt: Timestamp;
    }
  | {
      readonly status: "committed";
      readonly firstSubmissionId: Uuid;
      readonly firstRequestId: Uuid;
      readonly currentRequestId: Uuid;
      readonly firstMessageId: Uuid;
    }
  | { readonly status: "stale"; readonly reason: PreparationStaleReason }
  | { readonly status: "released" }
  | { readonly status: "failed"; readonly error: PublicError };

/**
 * 结果信封：`operationId`/`action`/`sessionId` 在**每个** status 分支上恒在（schema 的必填集合）。
 *
 * 与请求侧的 {@link WithPreparationEnvelope} 分开：`allocate` 请求没有 `sessionId`（由 Owner 分配
 * 后回显），因此不能共用一个信封。
 */
type WithPreparationResultEnvelope<T> = T extends unknown
  ? T & { readonly operationId: Uuid; readonly action: PreparationAction; readonly sessionId: Uuid }
  : never;

/** `$defs.sessionPreparationResult.body`：回显字段 + 该 status 的闭合结果。 */
export type PreparationResultBody = WithPreparationResultEnvelope<PreparationOutcome>;

/** `$defs.sessionPreparationResult`：`operationId`/`action`/`sessionId` 逐字回显 + 闭合结果。 */
export interface SyncSessionPreparationResult extends PostAuthEnvelope {
  readonly type: "session.preparation.result";
  readonly body: PreparationResultBody;
}

/** `sync.schema.json` 的顶层 `oneOf`。 */
export type SyncMessage =
  | SyncSubscribe
  | SyncCaughtUp
  | SyncResetRequired
  | SyncSnapshotRequest
  | SyncSnapshotBegin
  | SyncSnapshotChunk
  | SyncSnapshotEnd
  | SyncAck
  | SyncSessionView
  | SyncSessionViewResult
  | SyncRemoteSubscribe
  | SyncRemoteEvent
  | SyncRemoteCaughtUp
  | SyncRemoteAck
  | SyncRemoteResetRequired
  | SyncRemoteUnsubscribe
  | SyncRemoteUnavailable
  | SyncRemoteCatalogChanged
  | SyncRemoteSourceState
  | SyncSessionPreparation
  | SyncSessionPreparationResult;
