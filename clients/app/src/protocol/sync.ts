/**
 * `schemas/sync/v1/sync.schema.json` 的严格 TypeScript 镜像：订阅、快照与 ACK。
 *
 * 快照资源词表按 `design.md` D1 收窄为 `sessions`/`workspaces`/`agents` 三类**清单**资源；
 * 五类会话明细资源（`messages`/`turns`/`pending_interactions`/`config_options`/`capabilities`）
 * **不进快照**，其元素形状仍由下面的 `SnapshotItem*` 定义供 `session.read` 结果复用。
 */

import type {
  AgentCatalogEntry,
  AgentContentBlock,
  ConfigOptionView,
  Cursor,
  DecimalString,
  PublicError,
  SessionSummary,
  Timestamp,
  Uuid,
  WorkspaceRef,
} from "./common";

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
 * `$defs/snapshotItem.capabilities`：`agentCapabilities` 与 `brokerAdditions` 是 schema 里的
 * 开放对象（`{"type":"object"}`），因此保留 `Record<string, unknown>`。
 */
export interface SnapshotItemCapabilities {
  readonly sessionId: Uuid;
  readonly agentCapabilities: Record<string, unknown>;
  readonly brokerAdditions: Record<string, unknown>;
}

/** `$defs.snapshotItem.workspaces` / `.agents` 的别名（同一份值对象）。 */
export type SnapshotItemWorkspaces = WorkspaceRef;
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

/** `sync.schema.json` 的顶层 `oneOf`。 */
export type SyncMessage =
  | SyncSubscribe
  | SyncCaughtUp
  | SyncResetRequired
  | SyncSnapshotRequest
  | SyncSnapshotBegin
  | SyncSnapshotChunk
  | SyncSnapshotEnd
  | SyncAck;
