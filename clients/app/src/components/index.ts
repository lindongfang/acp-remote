/**
 * `src/components/` 的公开入口：**共享展示组件**。
 *
 * 区域收窄（`plan.md` 的 Shared File Ownership，`clients/app/app/` 与 `src/components/` 行）：
 * WP5a 只建骨架，WP7 接管这两处的页面实现与后续全部改动。
 *
 * 组件 MUST NOT 直接触碰 WebSocket/IndexedDB/CryptoKey（`pwa-web-client` 的
 * 「页面不直接接触平台能力」）；它们只接收 `src/features/` 的视图模型，
 * 并且是**纯函数**——所有交互通过回调上抛，导航由页面负责。
 */

export type { StatusPillProps } from "./StatusPill";
export { StatusPill } from "./StatusPill";
export type { EmptyStateProps } from "./EmptyState";
export { EmptyState } from "./EmptyState";
export type { DegradedEventCardProps } from "./DegradedEventCard";
export { DegradedEventCard } from "./DegradedEventCard";

export type { ContextUsageBarProps } from "./ContextUsageBar";
export { ContextUsageBar } from "./ContextUsageBar";
export type { ConversationStreamProps, ConversationSecondaryView } from "./ConversationStream";
export { ConversationStream } from "./ConversationStream";
export type { ConversationTimelineProps } from "./ConversationTimeline";
export { ConversationTimeline } from "./ConversationTimeline";
export type { ConversationComposerProps } from "./ConversationComposer";
export { ConversationComposer } from "./ConversationComposer";
export type { ConversationSettingsPanelProps } from "./ConversationSettingsPanel";
export { ConversationSettingsPanel } from "./ConversationSettingsPanel";
export type { FileChangesPanelProps } from "./FileChangesPanel";
export { FileChangesPanel } from "./FileChangesPanel";
export type { ChangeDiagnosticsPanelProps } from "./ChangeDiagnosticsPanel";
export { ChangeDiagnosticsPanel } from "./ChangeDiagnosticsPanel";
export type { DegradedEntryContentProps } from "./DegradedEntryContent";
export { DegradedEntryContent } from "./DegradedEntryContent";
export type { PendingInteractionsPanelProps } from "./PendingInteractionsPanel";
export { PendingInteractionsPanel } from "./PendingInteractionsPanel";
export type { InteractionAnswerButtonsProps } from "./InteractionAnswering";
export { InteractionAnswerButtons, InteractionAvailabilityNote } from "./InteractionAnswering";
export type { CreateSessionSheetProps } from "./CreateSessionSheet";
export { CreateSessionSheet } from "./CreateSessionSheet";
export type { DirectoryPickerProps } from "./DirectoryPicker";
export { DirectoryPicker } from "./DirectoryPicker";
export type { DiagnosticsDrawerProps } from "./DiagnosticsDrawer";
export { DiagnosticsDrawer } from "./DiagnosticsDrawer";
export type { DirectoryDetailProps } from "./DirectoryDetail";
export { DirectoryDetail } from "./DirectoryDetail";
export type { DirectoryListProps } from "./DirectoryList";
export { DirectoryList } from "./DirectoryList";
export type { HostConnectionPanelProps } from "./HostConnectionPanel";
export { HostConnectionPanel } from "./HostConnectionPanel";
export type { PairingPanelProps } from "./PairingPanel";
export { PairingPanel } from "./PairingPanel";
export { PairingThemeToggle } from "./PairingThemeToggle";
export type { RuntimeUnavailableProps } from "./RuntimeUnavailable";
export { RuntimeUnavailable } from "./RuntimeUnavailable";

// 样式基座（design D1）：token 与 `useTheme()` 是这两个页面的公共样式面。
// 三个新原语只吃消费方的扁平数据，交互一律回调上抛。
export type { ColorToken, Palette, Theme } from "./theme";
export { fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";
export type { DirectoryCardProps } from "./DirectoryCard";
export { DirectoryCard } from "./DirectoryCard";
export type { SegmentControlProps } from "./SegmentControl";
export { SegmentControl } from "./SegmentControl";
export type { SessionRowProps } from "./SessionRow";
export { SessionRow } from "./SessionRow";
export type { PagerDataKeys, PagerProps } from "./Pager";
export { Pager } from "./Pager";
