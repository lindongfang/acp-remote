/**
 * Agent 连接事件 → `AgentConnectionOverlay`（R19「Agent 目录与连接状态分两层呈现」）。
 *
 * ## 三条硬规则（spec 场景逐条对应）
 *
 * 1. **列表来自快照，事件只作覆盖层。** 本模块只产出 `overlay`，从不产出列表；
 *    列表由 `src/domain/agent-catalog.ts` 的 `buildAgentCatalogView(agents, overlay)` 叠加。
 * 2. **覆盖层缺席即「状态未知」。** 没有任何 `agent.connected`/`agent.disconnected` 到达时，
 *    overlay 为空，`buildAgentCatalogView` 把全部条目标为 `unknown`——**不是** `disconnected`，
 *    也**不**移除条目（spec 场景「状态未知不等于已断开」「断连时仍列出已配置 Agent」）。
 * 3. **MUST NOT 以会话活跃程度等其它信号推断。** 本模块只接受这两类事件；
 *    `session.*`/`turn.*` 事件一律返回 `null`，不做任何推断。
 *
 * ## 同名 Agent 以标识区分
 *
 * 覆盖层按 `agentId` 建索引而非展示名，两个同名 Agent 因此不会互相覆盖——
 * 叠加后各自保留自己的连接状态（spec 场景「标识区分同名 Agent」）。
 */

// `AgentConnectionOverlay` 未在 `src/domain/index.ts` 的 barrel 里重导出（WP5a 的写入范围），
// 因此这里指向定义它的子模块；两者是同一个类型，不存在第二份定义。
import type { AgentConnectionOverlay } from "../domain/agent-catalog";
import type { AgentDisconnectedView, EventMessage, ProjectedView } from "../protocol";
import { projectView } from "../protocol";

/** 从一条事件抽取覆盖层条目；与 Agent 连接无关的事件返回 `null`。 */
export function overlayFromEvent(event: EventMessage): AgentConnectionOverlay | null {
  if (event.body.eventType !== "agent.connected" && event.body.eventType !== "agent.disconnected") {
    return null;
  }
  const projected: ProjectedView = projectView(event.body.eventType, event.body.payload.view);
  // 未识别的 eventType 已被上面的判定排除；此处仍走 `known` 分支以复用类型收窄。
  if (projected.kind !== "known") return null;
  const agentId = projected.view.agentId;
  if (typeof agentId !== "string" || agentId.length === 0) return null;
  if (projected.eventType === "agent.connected") {
    return { agentId, state: "connected", reason: null };
  }
  const error = (projected.view as AgentDisconnectedView).error;
  return {
    agentId,
    state: "disconnected",
    // 断开原因取公开错误的消息；缺席时为 `null`（「未知原因」不得被编造）。
    reason: error === undefined ? null : error.message,
  };
}

/** 覆盖层集合：按 `agentId` 保存最近一次已知状态。 */
export class AgentConnectionOverlayStore {
  readonly #byAgent = new Map<string, AgentConnectionOverlay>();

  /**
   * 摄入一条事件。
   *
   * @returns 更新后的条目；事件与 Agent 连接无关时返回 `null`（不改动任何状态）。
   */
  ingest(event: EventMessage): AgentConnectionOverlay | null {
    const entry = overlayFromEvent(event);
    if (entry === null) return null;
    this.#byAgent.set(entry.agentId, entry);
    return entry;
  }

  /** 当前覆盖层（缺席时为空数组）。 */
  entries(): readonly AgentConnectionOverlay[] {
    return [...this.#byAgent.values()];
  }

  /** 覆盖层是否至少有一项；据此组件说明「状态未知」的原因。 */
  get overlayPresent(): boolean {
    return this.#byAgent.size > 0;
  }
}