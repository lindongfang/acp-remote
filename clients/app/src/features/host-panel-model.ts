/**
 * 主机与连接面板（R19「Agent 目录与连接状态分两层呈现」）。
 *
 * ## 两层语义在构造输入上就是分开的
 *
 * `buildHostPanelModel` 的 `agents` 与 `overlay` 是**两个独立参数**：
 *
 * - `agents` 来自快照的 `agents` 资源（列表本体）；
 * - `overlay` 来自 `agent.connected`/`agent.disconnected` 事件（可选覆盖层）。
 *
 * 构造函数**不接收会话**。因此「MUST NOT 以会话活跃程度等其它信号推断连接状态」
 * 不是一条需要靠自觉遵守的约定，而是这份构造签名本身没有可推断的输入：
 * 一个 Agent 有正在跑的会话，与它有没有连接状态覆盖层无关。
 *
 * ## 覆盖层缺席 = 状态未知
 *
 * `overlay` 为空时全部条目的状态是 `unknown`（`src/domain/agent-catalog.ts` 的
 * `buildAgentCatalogView`），且 `overlayPresent` 为假——组件据此说明**为什么**是未知
 * （事件通道尚未就绪 / 本次连接没有收到任何 Agent 连接事件），
 * 而不是渲染成「已断开」，也不是把该 Agent 从列表里拿掉。
 *
 * ## 同名 Agent 以标识区分
 *
 * 列表按 `agentId` 索引，展示名可以重复；每行都渲染标识，因此两个同名条目不会合并成一项。
 */

import type { AgentConnectionState, AgentCatalogView } from "../domain";
import { UNKNOWN_CONNECTION_LABEL, buildAgentCatalogView } from "../domain";
import type { StagedResources } from "../sync-client";
import type { BlockingDetail, ConnectionStateName } from "../state";
import type { AgentConnectionOverlay } from "../domain/agent-catalog";

/** 连接状态的展示文案。`unknown` 的措辞与 `disconnected` 明确不同。 */
export const CONNECTION_STATE_LABELS: Readonly<Record<ConnectionStateName, string>> = {
  unpaired: "未配对",
  pairing: "配对中",
  disconnected: "已断开",
  connecting: "连接中",
  authenticating: "认证中",
  replaying: "同步中（尚未追平）",
  online: "在线",
  reconnecting: "重连中",
  revoked: "已被撤销",
  incompatible: "版本不兼容",
  identity_changed: "主机身份已变化",
  replaced: "已被其它标签页顶替",
};

/** Agent 行的展示模型。 */
export interface AgentRowModel {
  readonly id: string;
  readonly displayName: string;
  readonly isDefault: boolean;
  readonly connection: AgentConnectionState;
  readonly connectionLabel: string;
  readonly disconnectReason: string | null;
}

/** 主机身份信息。`null` 表示尚未配对（此时不编造地址与指纹）。 */
export interface HostIdentityModel {
  readonly address: string;
  readonly hostId: string | null;
  readonly deviceId: string | null;
}

/** 主机与连接面板模型。 */
export interface HostPanelModel {
  readonly connectionState: ConnectionStateName;
  readonly connectionLabel: string;
  readonly blocking: BlockingDetail | null;
  /** 断连时标注「列表来自上次同步的快照」；在线时为 `null`。 */
  readonly offlineSnapshotNote: string | null;
  /** Agent 目录：**永远**来自快照，断连时也非空（快照里没有 Agent 才会为空）。 */
  readonly agents: readonly AgentRowModel[];
  /** 覆盖层是否到达过至少一条信息。 */
  readonly overlayPresent: boolean;
  /** 覆盖层缺席时的说明（`null` 表示覆盖层在场）。 */
  readonly unknownStateNote: string | null;
  readonly host: HostIdentityModel | null;
}

/** 覆盖层缺席时的固定说明：状态未知 ≠ 已断开，且不得据此把条目移除。 */
export const UNKNOWN_STATE_NOTE = "尚未收到 Agent 连接事件，连接状态未知（不等于已断开）。";

const CONNECTION_LABEL_BY_AGENT_STATE: Readonly<Record<AgentConnectionState, string>> = {
  connected: "已连接",
  disconnected: "已断开",
  unknown: UNKNOWN_CONNECTION_LABEL,
};

/**
 * 构造主机与连接面板模型。
 *
 * @param resources 快照资源（`agents` 是列表的唯一来源）。
 * @param overlay 连接事件覆盖层；缺席（空数组）即「状态未知」。
 * @param connection 连接状态机的当前状态。
 * @param host 主机身份；未配对时传 `null`。
 */
export function buildHostPanelModel(input: {
  readonly resources: StagedResources | null;
  readonly overlay: readonly AgentConnectionOverlay[];
  readonly connection: ConnectionStateName;
  readonly blocking: BlockingDetail | null;
  readonly host: HostIdentityModel | null;
}): HostPanelModel {
  const agents = input.resources?.agents ?? [];
  const catalog: AgentCatalogView = buildAgentCatalogView(agents, input.overlay);
  const rows: AgentRowModel[] = catalog.entries.map((entry) => ({
    id: entry.id,
    displayName: entry.displayName,
    isDefault: entry.isDefault,
    connection: entry.connection,
    connectionLabel: CONNECTION_LABEL_BY_AGENT_STATE[entry.connection],
    disconnectReason: entry.disconnectReason,
  }));

  const online = input.connection === "online";
  return {
    connectionState: input.connection,
    connectionLabel: CONNECTION_STATE_LABELS[input.connection],
    blocking: input.blocking,
    offlineSnapshotNote: online ? null : "当前未连接：下面的 Agent 列表来自上次成功同步的快照。",
    agents: rows,
    overlayPresent: catalog.overlayPresent,
    unknownStateNote: catalog.overlayPresent ? null : UNKNOWN_STATE_NOTE,
    host: input.host,
  };
}
