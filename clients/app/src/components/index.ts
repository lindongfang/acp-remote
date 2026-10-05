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

export type { ConnectionBadgeProps } from "./ConnectionBadge";
export { ConnectionBadge } from "./ConnectionBadge";
export type { ContextUsageBarProps } from "./ContextUsageBar";
export { ContextUsageBar } from "./ContextUsageBar";
export type { ConversationStreamProps } from "./ConversationStream";
export { ConversationStream } from "./ConversationStream";
export type { CreateSessionSheetProps } from "./CreateSessionSheet";
export { CreateSessionSheet } from "./CreateSessionSheet";
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
export type { RuntimeUnavailableProps } from "./RuntimeUnavailable";
export { RuntimeUnavailable } from "./RuntimeUnavailable";
