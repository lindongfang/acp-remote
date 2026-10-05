/**
 * `schemas/sync/v1/event.schema.json` 与 `schemas/sync/v1/event-views.schema.json` 的严格镜像。
 *
 * 两类对象的开放程度不同，镜像必须如实反映：
 *
 * - **事件信封**（`event.schema.json`）是 closed object（`additionalProperties: false`），
 *   逐字段精确建模；
 * - **事件视图**（`event-views.schema.json`）是 open object（`additionalProperties: true`），
 *   每个 `$defs` 只约束该事件类型的**最低必填字段**。因此视图类型都带一个 `extra` 索引签名，
 *   未知字段按值保留——`AGENTS.md` §3 要求未知字段与扩展 payload 往返保真，协议层不得
 *   丢弃或改写它们。
 *
 * 未知事件类型**不得**导致整个会话渲染崩溃：`projectView` 返回一个明确的 `unknown` 分支。
 */

import type {
  AgentContentBlock,
  ConfigOptionView,
  DecimalString,
  InteractionOption,
  PublicError,
  RawAcp,
  RemoteOrigin,
  SessionSummary,
  Timestamp,
  Uuid,
} from "./common";

/** `event.schema.json` 的 `body.origin.kind` 封闭词表。 */
export type EventOriginKind = "agent" | "device" | "daemon" | "local_cli";

/** `body.origin`（内嵌对象）。 */
export interface EventOrigin {
  readonly kind: EventOriginKind;
  readonly deviceId: Uuid | null;
}

/** `body.payload`：`view` 是开放对象，`acp` 是可缺席的 ACP 原文。 */
export interface EventPayload {
  readonly view: Record<string, unknown>;
  readonly acp?: RawAcp;
}

/** `event.schema.json`（持久化事件信封）。 */
export interface EventMessage {
  readonly protocolVersion: 1;
  readonly type: "event";
  readonly messageId: Uuid;
  readonly connectionId: Uuid;
  readonly connectionSequence: DecimalString;
  readonly body: {
    readonly globalSequence: DecimalString;
    readonly sessionSequence: DecimalString | null;
    readonly eventId: Uuid;
    readonly sessionId: Uuid | null;
    readonly eventType: string;
    readonly causationRequestId: Uuid | null;
    readonly origin: EventOrigin;
    readonly remoteOrigin: RemoteOrigin | null;
    readonly createdAt: Timestamp;
    readonly payload: EventPayload;
  };
}

/** `event-views.schema.json` 的 `$defs` 键集合（34 个 eventType）。 */
export type EventType =
  | "session.created"
  | "session.updated"
  | "session.info.changed"
  | "session.config.changed"
  | "session.mode.changed"
  | "session.plan.changed"
  | "session.commands.changed"
  | "session.usage.changed"
  | "session.origin.online_changed"
  | "turn.queued"
  | "turn.started"
  | "turn.completed"
  | "turn.cancelled"
  | "turn.failed"
  | "turn.delta_compacted"
  | "user.message.delta"
  | "agent.message.delta"
  | "agent.thought.delta"
  | "agent.message.completed"
  | "tool.call.started"
  | "tool.call.updated"
  | "tool.call.completed"
  | "permission.requested"
  | "permission.resolved"
  | "elicitation.requested"
  | "elicitation.resolved"
  | "terminal.output"
  | "file.changed"
  | "agent.connected"
  | "agent.disconnected"
  | "command.completed"
  | "command.failed"
  | "command.uncertain"
  | "device.revoked";

/** `session.plan.changed.entries[].priority`（`VIEW_ENUMS` 已登记）。 */
export type PlanPriority = "high" | "medium" | "low";

/** `session.plan.changed.entries[].status`（`VIEW_ENUMS` 已登记）。 */
export type PlanStatus = "pending" | "in_progress" | "completed";

/** `elicitation.resolved.action`（`VIEW_ENUMS` 已登记）。 */
export type ElicitationResolutionAction = "submit" | "decline" | "cancel";

/** `terminal.output.stream`（`VIEW_ENUMS` 已登记）。 */
export type TerminalStream = "stdout" | "stderr";

/** `agent.connected.state`（`VIEW_ENUMS` 已登记，取值唯一）。 */
export type AgentConnectedState = "connected";

/** `agent.disconnected.state`（`VIEW_ENUMS` 已登记，取值唯一）。 */
export type AgentDisconnectedState = "disconnected";

/** 视图共有的开放对象尾巴：未知键按值保留，供诊断或未来组件使用。 */
export type OpenView = Record<string, unknown>;

/** `$defs.session.created` / `$defs.session.updated`。 */
export type SessionCreatedView = { readonly session: SessionSummary } & OpenView;
export type SessionUpdatedView = { readonly session: SessionSummary } & OpenView;

/** `$defs.session.info.changed`：标题单向由 Agent 通知写入（`design.md` D7）。 */
export type SessionInfoChangedView = {
  readonly title: string | null;
  readonly updatedAt: Timestamp;
} & OpenView;

/** `$defs.session.config.changed`。 */
export type SessionConfigChangedView = {
  readonly configOptions: readonly ConfigOptionView[];
  readonly version: DecimalString;
} & OpenView;

/** `$defs.session.mode.changed`。 */
export type SessionModeChangedView = {
  readonly currentModeId: string | null;
  readonly version: DecimalString;
} & OpenView;

/** `$defs.session.plan.changed`。 */
export type SessionPlanChangedView = {
  readonly entries: readonly ({
    readonly content: string;
    readonly priority: PlanPriority;
    readonly status: PlanStatus;
  } & OpenView)[];
} & OpenView;

/** `$defs.session.commands.changed`。 */
export type SessionCommandsChangedView = {
  readonly commands: readonly ({
    readonly name: string;
    readonly description: string;
  } & OpenView)[];
} & OpenView;

/**
 * `$defs.session.usage.changed`：`used`/`size` 都是 `decimalString`。
 * 窗口大小为零时客户端 MUST 呈现为「未上报」（`pwa-web-client` 的「未上报的上下文不得以零冒充」）。
 */
export type SessionUsageChangedView = {
  readonly used: DecimalString;
  readonly size: DecimalString;
} & OpenView;

/** `$defs.session.origin.online_changed`。 */
export type SessionOriginOnlineChangedView = {
  readonly ownerNodeId: Uuid;
  readonly exportId: string;
  readonly originEpoch: Uuid;
  readonly online: boolean;
} & OpenView;

/** 五个 turn 事件共享的 `{turnId, state}` 形状。 */
export interface TurnStateFields {
  readonly turnId: Uuid;
  readonly state: string;
}

/** `$defs.turn.queued` / `.started` / `.completed` / `.cancelled`。 */
export type TurnQueuedView = TurnStateFields & OpenView;
export type TurnStartedView = TurnStateFields & OpenView;
export type TurnCompletedView = TurnStateFields & OpenView;
export type TurnCancelledView = TurnStateFields & OpenView;

/** `$defs.turn.failed`：`state` 是 `const "failed"`。 */
export type TurnFailedView = {
  readonly turnId: Uuid;
  readonly state: "failed";
  readonly error: PublicError;
} & OpenView;

/** `$defs.turn.delta_compacted`。 */
export type TurnDeltaCompactedView = {
  readonly turnId: Uuid;
  readonly deltaCount: DecimalString;
} & OpenView;

/** `user.message.delta` / `agent.message.delta` / `agent.thought.delta` 的公共字段。 */
export interface MessageDeltaFields {
  readonly messageId: Uuid;
  readonly turnId: Uuid;
  readonly deltaIndex: DecimalString;
  readonly text: string;
}

/** `$defs.user.message.delta`。 */
export type UserMessageDeltaView = MessageDeltaFields & OpenView;

/** `$defs.agent.message.delta`：可带一个结构化的内容块（如图片/资源引用）。 */
export type AgentMessageDeltaView = MessageDeltaFields & {
  readonly block?: AgentContentBlock;
} & OpenView;

/** `$defs.agent.thought.delta`。 */
export type AgentThoughtDeltaView = MessageDeltaFields & OpenView;

/** `$defs.agent.message.completed`。 */
export type AgentMessageCompletedView = {
  readonly messageId: Uuid;
  readonly turnId: Uuid;
  readonly content: readonly AgentContentBlock[];
} & OpenView;

/** `tool.call.started` / `.updated` / `.completed` 的公共字段。 */
export interface ToolCallFields {
  readonly toolCallId: string;
  readonly turnId: Uuid;
  readonly title: string;
  readonly state: string;
}

/** `$defs.tool.call.started` / `.updated` / `.completed`。 */
export type ToolCallStartedView = ToolCallFields & OpenView;
export type ToolCallUpdatedView = ToolCallFields & OpenView;
export type ToolCallCompletedView = ToolCallFields & OpenView;

/** `$defs.permission.requested`。 */
export type PermissionRequestedView = {
  readonly interactionId: Uuid;
  readonly turnId: Uuid;
  readonly title: string;
  readonly description: string | null;
  readonly options: readonly InteractionOption[];
} & OpenView;

/** `$defs.permission.resolved`。 */
export type PermissionResolvedView = {
  readonly interactionId: Uuid;
  readonly resolution: string;
  readonly resolvedByDeviceId: Uuid | null;
} & OpenView;

/** `$defs.elicitation.requested`：`schema`/`initialValues` 是开放对象。 */
export type ElicitationRequestedView = {
  readonly interactionId: Uuid;
  readonly turnId: Uuid;
  readonly title: string;
  readonly schema: Record<string, unknown>;
  readonly initialValues: Record<string, unknown> | null;
} & OpenView;

/** `$defs.elicitation.resolved`。 */
export type ElicitationResolvedView = {
  readonly interactionId: Uuid;
  readonly action: ElicitationResolutionAction;
  readonly resolvedByDeviceId: Uuid | null;
} & OpenView;

/** `$defs.terminal.output`。 */
export type TerminalOutputView = {
  readonly terminalId: string;
  readonly chunkIndex: DecimalString;
  readonly stream: TerminalStream;
  readonly text: string;
  readonly truncated: boolean;
} & OpenView;

/**
 * `$defs.file.changed`：`addedLines`/`deletedLines`/`outsideWorkspace` 都是可选——
 * 「判定不出时省略而非填零」（`design.md` D4）。`displayPath` 由 core 相对化，
 * 越界时只给文件名且置 `outsideWorkspace: true`（D5）。
 */
export type FileChangedView = {
  readonly changeId: Uuid;
  readonly kind: string;
  readonly displayPath: string;
  readonly summary: string;
  readonly addedLines?: DecimalString;
  readonly deletedLines?: DecimalString;
  readonly outsideWorkspace?: boolean;
} & OpenView;

/** `$defs.agent.connected`：节点级事件，`state` 取值唯一。 */
export type AgentConnectedView = {
  readonly agentId: string;
  readonly state: AgentConnectedState;
} & OpenView;

/** `$defs.agent.disconnected`：可携带本次退出的公开错误。 */
export type AgentDisconnectedView = {
  readonly agentId: string;
  readonly state: AgentDisconnectedState;
  readonly error?: PublicError;
} & OpenView;

/** `$defs.command.completed`。 */
export type CommandCompletedView = {
  readonly requestId: Uuid;
  readonly result: Record<string, unknown> | null;
} & OpenView;

/** `$defs.command.failed`。 */
export type CommandFailedView = {
  readonly requestId: Uuid;
  readonly error: PublicError;
} & OpenView;

/** `$defs.command.uncertain`：`mayHaveReachedAgent` 是 `const true`。 */
export type CommandUncertainView = {
  readonly requestId: Uuid;
  readonly reason: string;
  readonly mayHaveReachedAgent: true;
} & OpenView;

/** `$defs.device.revoked`。 */
export type DeviceRevokedView = {
  readonly deviceId: Uuid;
  readonly revokedAt: Timestamp;
} & OpenView;

/** eventType → 视图类型的映射。 */
export interface EventViewByType {
  readonly "session.created": SessionCreatedView;
  readonly "session.updated": SessionUpdatedView;
  readonly "session.info.changed": SessionInfoChangedView;
  readonly "session.config.changed": SessionConfigChangedView;
  readonly "session.mode.changed": SessionModeChangedView;
  readonly "session.plan.changed": SessionPlanChangedView;
  readonly "session.commands.changed": SessionCommandsChangedView;
  readonly "session.usage.changed": SessionUsageChangedView;
  readonly "session.origin.online_changed": SessionOriginOnlineChangedView;
  readonly "turn.queued": TurnQueuedView;
  readonly "turn.started": TurnStartedView;
  readonly "turn.completed": TurnCompletedView;
  readonly "turn.cancelled": TurnCancelledView;
  readonly "turn.failed": TurnFailedView;
  readonly "turn.delta_compacted": TurnDeltaCompactedView;
  readonly "user.message.delta": UserMessageDeltaView;
  readonly "agent.message.delta": AgentMessageDeltaView;
  readonly "agent.thought.delta": AgentThoughtDeltaView;
  readonly "agent.message.completed": AgentMessageCompletedView;
  readonly "tool.call.started": ToolCallStartedView;
  readonly "tool.call.updated": ToolCallUpdatedView;
  readonly "tool.call.completed": ToolCallCompletedView;
  readonly "permission.requested": PermissionRequestedView;
  readonly "permission.resolved": PermissionResolvedView;
  readonly "elicitation.requested": ElicitationRequestedView;
  readonly "elicitation.resolved": ElicitationResolvedView;
  readonly "terminal.output": TerminalOutputView;
  readonly "file.changed": FileChangedView;
  readonly "agent.connected": AgentConnectedView;
  readonly "agent.disconnected": AgentDisconnectedView;
  readonly "command.completed": CommandCompletedView;
  readonly "command.failed": CommandFailedView;
  readonly "command.uncertain": CommandUncertainView;
  readonly "device.revoked": DeviceRevokedView;
}

/**
 * `projectView` 的结果：已识别的事件类型收窄到具体视图，未识别的事件类型走 `unknown` 分支
 * 并保留原始 `view` 与 `eventType`——降级卡片据此渲染，整个会话渲染不因此崩溃。
 */
export type ProjectedView =
  | { readonly kind: "known"; readonly eventType: EventType; readonly view: EventViewByType[EventType] }
  | { readonly kind: "unknown"; readonly eventType: string; readonly view: Record<string, unknown> };

/**
 * 把 `event.payload.view` 按 `eventType` 投影。
 *
 * **不做宽容解析**：这里不猜测服务端语义、不补默认值、不吞字段。识别与否只决定走哪条分支，
 * 两分支都原样持有 `view`（按值保留未知字段）。收窄的可信前提是 Rust 侧
 * `crates/sync-protocol/src/views.rs` 的 `VIEW_TYPES` 已按同一 schema 投影；本函数只做
 * eventType 的分派，把「是否已知」与「视图形状」分开表达。
 */
export function projectView(
  eventType: string,
  view: Record<string, unknown>,
): ProjectedView {
  if (isKnownEventType(eventType)) {
    return { kind: "known", eventType, view: view as EventViewByType[EventType] };
  }
  return { kind: "unknown", eventType, view };
}

/** `EventViewByType` 的键集合，运行期用作 eventType 的封闭判别。 */
const KNOWN_EVENT_TYPES: readonly EventType[] = [
  "session.created",
  "session.updated",
  "session.info.changed",
  "session.config.changed",
  "session.mode.changed",
  "session.plan.changed",
  "session.commands.changed",
  "session.usage.changed",
  "session.origin.online_changed",
  "turn.queued",
  "turn.started",
  "turn.completed",
  "turn.cancelled",
  "turn.failed",
  "turn.delta_compacted",
  "user.message.delta",
  "agent.message.delta",
  "agent.thought.delta",
  "agent.message.completed",
  "tool.call.started",
  "tool.call.updated",
  "tool.call.completed",
  "permission.requested",
  "permission.resolved",
  "elicitation.requested",
  "elicitation.resolved",
  "terminal.output",
  "file.changed",
  "agent.connected",
  "agent.disconnected",
  "command.completed",
  "command.failed",
  "command.uncertain",
  "device.revoked",
];

/** Type guard：收窄到 `EventType`（保持 `projectView` 的分支可判别）。 */
export function isKnownEventType(value: string): value is EventType {
  return (KNOWN_EVENT_TYPES as readonly string[]).includes(value);
}

/** 已登记的 eventType 数量；契约测试据此与 `event-views.schema.json` 的 `$defs` 对齐。 */
export const KNOWN_EVENT_TYPE_COUNT: number = KNOWN_EVENT_TYPES.length;
