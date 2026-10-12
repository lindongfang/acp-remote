/**
 * `src/domain/` 的公开入口：**目录、会话摘要、Agent 目录条目**的视图模型。
 *
 * 分层的硬约束（`AGENTS.md` §6、`pwa-web-client` 的「视图模型不暴露协议载荷」）：
 * 组件消费这里导出的类型，**不**直接读 `event.payload.view` 或任何 `src/protocol/` 的原始字段。
 * 协议类型只作为本层**构造输入**出现在 `build*` 函数的参数里，不出现在输出类型上。
 */

export type {
  DirectoryId,
  DirectoryView,
  DirectoryListState,
  DirectorySessionCounts,
  DirectoryEmptyState,
} from "./directory";
export { buildDirectoryViews, DIRECTION_EMPTY_STATES } from "./directory";

export type {
  SessionSummaryView,
  SessionId,
  SessionStateView,
  SessionOriginView,
  SessionTitle,
} from "./session-summary";
export { buildSessionSummaryView, UNTITLED_SESSION_LABEL } from "./session-summary";

export type {
  AgentCatalogView,
  AgentCatalogEntryView,
  AgentId,
  AgentConnectionState,
} from "./agent-catalog";
export { buildAgentCatalogView, UNKNOWN_CONNECTION_LABEL } from "./agent-catalog";

export type {
  SessionReadPageView,
  ContextUsageView,
  MessageView,
  MessageRole,
} from "./conversation";
export { buildSessionReadPageView, buildContextUsageView, contextUsageLabel } from "./conversation";

export type {
  InteractionAnswer,
  InteractionAvailability,
  InteractionKind,
  InteractionResolution,
  InteractionUnavailableReason,
  InteractionView,
} from "./interaction";
export { ELICITATION_ANSWERS, INTERACTION_UNAVAILABLE_LABELS, buildInteractionAvailability } from "./interaction";

export type {
  ConfigChangedTimelineCard,
  ElicitationRequestedTimelineCard,
  ElicitationResolvedTimelineCard,
  FileChangedTimelineCard,
  ModeChangedTimelineCard,
  PermissionRequestedTimelineCard,
  PermissionResolvedTimelineCard,
  PlanEntryView,
  PlanTimelineCard,
  RecordGroup,
  StructuredRecord,
  StructuredSource,
  TerminalOutputTimelineCard,
  TimelineCard,
  TimelinePosition,
  ToolCallTimelineCard,
  TurnTimelineCard,
} from "./timeline";
export {
  buildTimelineCard,
  compareTimelinePosition,
  groupStructuredRecords,
  mergeStructuredRecords,
  positionOf,
  recordFromEvent,
  recordFromHistoryItem,
  recordGroupOf,
  viewText,
} from "./timeline";
