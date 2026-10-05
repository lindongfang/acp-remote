/**
 * `src/components/` 的公开入口：**共享展示组件骨架**。
 *
 * 区域收窄（`plan.md` 的 Shared File Ownership，`clients/app/app/` 与 `src/components/` 行）：
 * WP5a 只建骨架，WP7 接管这两处的页面实现与后续全部改动。因此本目录当前只有
 * 「不依赖任何 feature、任何状态机」的最小展示原语，WP7 在其上组合页面。
 *
 * 组件 MUST NOT 直接触碰 WebSocket/IndexedDB/CryptoKey（`pwa-web-client` 的
 * 「页面不直接接触平台能力」）；它们只接收 `src/domain/` 的视图模型。
 */

export type { StatusPillProps } from "./StatusPill";
export { StatusPill } from "./StatusPill";
export type { EmptyStateProps } from "./EmptyState";
export { EmptyState } from "./EmptyState";
export type { DegradedEventCardProps } from "./DegradedEventCard";
export { DegradedEventCard } from "./DegradedEventCard";
