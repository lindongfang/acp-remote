/**
 * Agent 目录视图模型（R19「Agent 目录与连接状态分两层呈现」的基础层）。
 *
 * 两层语义必须分开：
 *
 * - **列表**来自快照的 `agents` 资源（`AgentCatalogEntry`），因此断连时仍可呈现；
 *   列表**不**以 `agent.connected`/`agent.disconnected` 事件为来源。
 * - **连接状态**是可选覆盖层：覆盖层缺席——包括服务端尚不具备该类事件的投递通道时——
 *   呈现为 `unknown`，**不**当作 `disconnected`，也**不**因此移除该 Agent。
 *
 * 同名展示名的 Agent 以标识区分，视图层因此保留 `agentId` 且不做按名合并。
 */

import type { AgentCatalogEntry } from "../protocol/common";

/** Agent 的稳定标识（wire 上的 `agentId`）。 */
export type AgentId = string;

/**
 * 连接状态覆盖层。
 *
 * `unknown` 是**缺席**的表达，与 `disconnected` 是两件事：`design.md` D6 的现状是
 * 节点级事件没有投递通道，因此覆盖层在同步入站面落地前一律为 `unknown`。
 */
export type AgentConnectionState = "connected" | "disconnected" | "unknown";

/** 状态未知的展示文案。 */
export const UNKNOWN_CONNECTION_LABEL = "状态未知";

/** 一个 Agent 目录条目。 */
export interface AgentCatalogEntryView {
  readonly id: AgentId;
  /** 展示名；同名允许，由 `id` 区分。 */
  readonly displayName: string;
  /** 是否为本机默认 profile（`session.create` 选择器的默认选中项）。 */
  readonly isDefault: boolean;
  /** 来自连接事件覆盖层；缺席时为 `unknown`。 */
  readonly connection: AgentConnectionState;
  /** 覆盖层给出的最近一次断开原因（仅在 `disconnected` 且事件携带时存在）。 */
  readonly disconnectReason: string | null;
}

/** Agent 目录视图：列表本体 + 覆盖层是否曾到达。 */
export interface AgentCatalogView {
  readonly entries: readonly AgentCatalogEntryView[];
  /**
   * 覆盖层是否至少有一次性信息。
   *
   * 为假时全部条目的 `connection` 都是 `unknown`——组件据此说明「状态未知」的原因，
   * 而不是渲染成「已断开」。
   */
  readonly overlayPresent: boolean;
}

/** 覆盖层条目：由 `src/domain` 之外的消费方（WP6 的事件处理）构造。 */
export interface AgentConnectionOverlay {
  readonly agentId: AgentId;
  readonly state: "connected" | "disconnected";
  readonly reason: string | null;
}

/**
 * 从快照的 `agents` 资源构造 Agent 目录视图，并把连接覆盖层叠加其上。
 *
 * `overlay` 为空数组即「覆盖层缺席」：所有条目的 `connection` 保持 `unknown`，
 * 列表本身不受影响（`pwa-web-client` 的「状态未知不等于已断开」）。
 */
export function buildAgentCatalogView(
  agents: readonly AgentCatalogEntry[],
  overlay: readonly AgentConnectionOverlay[],
): AgentCatalogView {
  const byId = new Map<AgentId, AgentConnectionOverlay>();
  for (const entry of overlay) {
    byId.set(entry.agentId, entry);
  }

  const entries: AgentCatalogEntryView[] = agents.map((agent) => {
    const state = byId.get(agent.agentId);
    return {
      id: agent.agentId,
      displayName: agent.displayName,
      isDefault: agent.default,
      connection: state?.state ?? "unknown",
      disconnectReason: state?.state === "disconnected" ? (state.reason ?? null) : null,
    };
  });

  return { entries, overlayPresent: overlay.length > 0 };
}
