/**
 * `schemas/sync/v1/common.schema.json` 的**严格** TypeScript 镜像。
 *
 * 纪律（`design.md` D8「协议层严格 typed，不做宽容解析」）：
 *
 * - 每个 `$defs` 条目对应一个命名导出类型，字段名、可选性与取值域与 schema 一一对应；
 * - 不用 `any`，不用可选链把类型错误吞成 `undefined`；`T | null` 写成 `T | null` 而不是 `T?`；
 * - `additionalProperties: false` 的对象用精确接口（无索引签名），因此多写一个字段即编译失败；
 * - 唯一的例外是**开放对象**：`event.payload.view`（`event-views` 是开放对象）与
 *   `error.details`，它们按 schema 允许额外键，因此显式保留。
 *
 * 镜像只描述 wire 形状，不承载业务规则：解码后的校验（base64url 真实字节长度、P-256 曲线点、
 * transcript、hash）按 `schemas/sync/v1/README.md` 的说明属于 Rust/TypeScript 契约测试与运行时。
 */

// ── 标量与值对象 ────────────────────────────────────────────────────────────

/** `$defs.uuid`：`^[0-9a-f]{8}-…$`。用品牌类型防止裸 `string` 混入。 */
export type Uuid = string;

/** `$defs.decimalString`：`^(0|[1-9][0-9]*)$`。超过 JavaScript safe integer 的计数以此承载。 */
export type DecimalString = string;

/** `$defs.timestamp`：`^\\d{4}-…\\.\\d{3}Z$`（Daemon 持久化时间的固定形状）。 */
export type Timestamp = string;

/** `$defs.base64url`：无填充 base64url 字母表。 */
export type Base64Url = string;

/** `$defs.base64url16` / `32` / `64` / `65`：长度由 schema 约束的固定宽度编码。 */
export type Base64Url16 = Base64Url;
export type Base64Url32 = Base64Url;
export type Base64Url64 = Base64Url;
export type Base64Url65 = Base64Url;

/** `$defs.featureId`：`^[a-z0-9.-]+$`，长度 1–64。 */
export type FeatureId = string;

/** `$defs.featureList`：去重、至多 64 项的 feature 列表。 */
export type FeatureList = readonly FeatureId[];

/** `$defs.errorCode`：封闭词表，唯一来源是 `compatibility/errors/v1/errors.json` 的 sync 列表。 */
export type ErrorCode =
  | "protocol.invalid_json"
  | "protocol.schema_invalid"
  | "protocol.message_too_large"
  | "protocol.version_unsupported"
  | "protocol.feature_required"
  | "protocol.type_unsupported"
  | "protocol.sequence_invalid"
  | "auth.required"
  | "auth.host_mismatch"
  | "auth.device_unknown"
  | "auth.device_revoked"
  | "auth.origin_mismatch"
  | "auth.proof_invalid"
  | "authorization.scope_denied"
  | "pairing.already_claimed"
  | "pairing.expired"
  | "pairing.consumed"
  | "sync.cursor_invalid"
  | "command.unsupported"
  | "command.not_found"
  | "command.idempotency_conflict"
  | "command.uncertain"
  | "state.version_conflict"
  | "session.not_found"
  | "session.busy"
  | "interaction.already_resolved"
  | "capability.unsupported_by_client"
  | "capability.unsupported_by_broker"
  | "capability.unsupported_by_agent"
  | "resource.rate_limited"
  | "resource.backpressure"
  | "resource.result_too_large"
  | "resource.remote_unavailable"
  | "internal.unavailable";

// ── 游标与错误 ──────────────────────────────────────────────────────────────

/** `$defs.cursor`：`{serverEpoch, globalSequence}`，两键都必填。 */
export interface Cursor {
  readonly serverEpoch: Uuid;
  readonly globalSequence: DecimalString;
}

/**
 * `$defs.publicError`：`details` 是 schema 里明确的**开放对象**（`{"type":"object"}`），
 * 因此这里保留 `Record<string, unknown>` 而不是精确接口。
 */
export interface PublicError {
  readonly code: ErrorCode;
  readonly message: string;
  readonly retryable: boolean;
  readonly details: Record<string, unknown>;
}

// ── 模式与配置项 ────────────────────────────────────────────────────────────

/** `$defs.modeRef`。 */
export interface ModeRef {
  readonly modeId: string;
  readonly displayName: string;
}

/** `$defs.modeState`：`currentModeId` 是 `string | null`（键必填）。 */
export interface ModeState {
  readonly currentModeId: string | null;
  readonly availableModes: readonly ModeRef[];
  readonly version: DecimalString;
}

/** `$defs.configOptionView.options[]`。 */
export interface ConfigOptionChoice {
  readonly value: string;
  readonly name: string;
  readonly description: string | null;
}

/**
 * `$defs.configOptionView`。
 *
 * `description`/`category`/`options` 是必填键 —— schema 的 `required` 不含 `options`，
 * 但含 `description` 与 `category`；`options` 只在 `type === "select"` 时才有意义，
 * schema 未做条件必填，因此这里按 schema 原样写成可选键。
 */
export interface ConfigOptionView {
  readonly id: string;
  readonly name: string;
  readonly description: string | null;
  readonly category: string | null;
  readonly type: "select" | "boolean";
  readonly currentValue: string | boolean;
  readonly options?: readonly ConfigOptionChoice[];
}

// ── 来源与会话摘要 ──────────────────────────────────────────────────────────

/** `$defs.originBlock`：本地会话只有 `kind`，远程会话另带四个必填字段。 */
export type OriginBlock =
  | { readonly kind: "local" }
  | {
      readonly kind: "remote";
      readonly ownerNodeId: Uuid;
      readonly exportId: string;
      readonly originEpoch: Uuid;
      readonly online: boolean;
    };

/** `$defs.remoteOrigin`：事件的跨节点溯源坐标。 */
export interface RemoteOrigin {
  readonly ownerNodeId: Uuid;
  readonly exportId: string;
  readonly originEpoch: Uuid;
  readonly originEventId: Uuid;
  readonly originSequence: DecimalString;
}

/** `$defs.workspaceRef`：目录项**只有**别名与展示名，规范化路径从不出现在 wire 上。 */
export interface WorkspaceRef {
  readonly alias: string;
  readonly displayName: string;
}

/** `$defs.agentCatalogEntry`：快照 `agents` 资源的元素形状。 */
export interface AgentCatalogEntry {
  readonly agentId: string;
  readonly displayName: string;
  readonly default: boolean;
}

/** `$defs.sessionSummary.state`：封闭词表（目录页的「待处理」徽标由此得出）。 */
export type SessionState =
  | "idle"
  | "queued"
  | "running"
  | "waiting_input"
  | "waiting_permission"
  | "failed"
  | "closed";

/** `$defs.sessionSummary.agent`（内嵌对象，无独立 `$defs` 条目）。 */
export interface SessionSummaryAgent {
  readonly agentId: string;
  readonly name: string;
}

/** `$defs.sessionSummary`：快照 `sessions` 资源与 `session.list` 结果的元素形状。 */
export interface SessionSummary {
  readonly sessionId: Uuid;
  readonly title: string | null;
  readonly agent: SessionSummaryAgent;
  readonly state: SessionState;
  readonly origin: OriginBlock;
  readonly currentMode: ModeRef | null;
  readonly version: DecimalString;
  readonly createdAt: Timestamp;
  readonly updatedAt: Timestamp;
  readonly workspace?: WorkspaceRef | null;
}

// ── 内容块 ──────────────────────────────────────────────────────────────────

/** `$defs.agentContentBlock`：四分支 `oneOf`，每支的必填集合不同。 */
export type AgentContentBlock =
  | { readonly type: "text"; readonly text: string }
  | {
      readonly type: "image_ref";
      readonly mimeType: string;
      readonly byteLength: DecimalString | null;
      readonly displayState: "available" | "not_fetched" | "unsupported";
    }
  | {
      readonly type: "resource_ref";
      readonly uri: string;
      readonly name: string | null;
      readonly displayState: "available" | "not_fetched" | "unsupported";
    }
  | {
      readonly type: "unsupported";
      readonly originalType: string;
      readonly reason: "unsupported_by_client";
    };

/** `$defs.interactionOption`。 */
export interface InteractionOption {
  readonly optionId: string;
  readonly label: string;
  readonly kind: string;
}

/** `$defs.promptContentBlock`：v1 只允许文本输入。 */
export interface PromptContentBlock {
  readonly type: "text";
  readonly text: string;
}

// ── ACP 原文 ────────────────────────────────────────────────────────────────

/**
 * `$defs.rawAcp`：原文可用时携带 `rawJson` 与摘要；未下发时携带 `rawUnavailable`
 * 说明原因、字节长度与可空的 sha256。**两种情形都不允许先解析再覆盖原文**（`AGENTS.md` §3）。
 */
export type RawAcp =
  | {
      readonly mediaType: "application/json";
      readonly rawJson: string;
      readonly byteLength: DecimalString;
      readonly sha256: Base64Url32;
    }
  | {
      readonly mediaType: "application/json";
      readonly rawUnavailable: {
        readonly reason: "size_limit" | "retention_expired" | "storage_failure";
        readonly byteLength: DecimalString;
        readonly sha256: Base64Url32 | null;
      };
    };
