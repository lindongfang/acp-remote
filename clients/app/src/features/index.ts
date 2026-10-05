/**
 * feature 控制器层（`docs/FRONTEND_DESIGN.md` §3 的 `features/`）。
 *
 * **归属 WP7**（`plan.md` 的 WP7 行）：`pairing/`、`hosts/`、`sessions/`、`conversation/`、
 * `permissions/`、`models/` 六个 feature 的控制器与页面由 WP7 实现。
 * WP5a 只固定这一层的**位置**与依赖方向（见 `../layers.ts`）。
 *
 * 该层可以消费 `src/domain/` 的视图模型与 `src/state/` 的状态类型，
 * 但 MUST NOT 直接触碰 WebSocket/IndexedDB/CryptoKey（`AGENTS.md` §5）。
 */

import type {
  AgentCatalogView,
  DirectoryListState,
  SessionReadPageView,
  SessionSummaryView,
} from "../domain";

/**
 * feature 层消费的视图模型面。
 *
 * 这是一份**只读**的契约导出口：WP7 的六个 feature 从这里取类型，
 * 而不是各自去 import `src/protocol/` 的原始字段——这正是
 * `pwa-web-client` 的「视图模型不暴露协议载荷」在结构上的落点。
 */
export interface FeatureViewModels {
  readonly directories: DirectoryListState;
  readonly sessions: readonly SessionSummaryView[];
  readonly agents: AgentCatalogView;
  readonly conversation: SessionReadPageView;
}
