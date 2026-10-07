/**
 * 主机与连接面板（R19）。
 *
 * ## 两条 MUST 的渲染落点
 *
 * - **Agent 列表来自快照**：本组件渲染的 `model.agents` 来自 `HostPanelModel`，
 *   而后者只由「快照 agents + 事件覆盖层」构造。断连时列表照常渲染并标注来源。
 * - **覆盖层缺席即状态未知**：每行都渲染 `data-agent-connection`，缺席时是
 *   `unknown`（文案「状态未知」），并且面板给出缺席原因；MUST NOT 渲染成「已断开」，
 *   MUST NOT 因为缺席把行删掉。
 *
 * 同名展示名以标识区分：每行都渲染 `agent.id`，因此两个同名 Agent 是两行而不是一行。
 */

import type { ReactElement } from "react";

import type { HostPanelModel } from "../features/host-panel-model";

export interface HostConnectionPanelProps {
  readonly model: HostPanelModel;
}


/** Agent 行。 */
function AgentRow({ agent }: { readonly agent: HostPanelModel["agents"][number] }): ReactElement {
  return (
    <li data-agent-row={agent.id} data-agent-connection={agent.connection}>
      <span data-agent-display-name={agent.displayName}>{agent.displayName}</span>
      {/* 同名 Agent 以标识区分：id 始终呈现。 */}
      <span data-agent-id-label>{agent.id}</span>
      {agent.isDefault ? <span data-agent-default="true">默认</span> : null}
      <span data-agent-connection-label={agent.connection}>{agent.connectionLabel}</span>
      {agent.disconnectReason === null ? null : (
        <span data-agent-disconnect-reason="true">{agent.disconnectReason}</span>
      )}
    </li>
  );
}

/** 主机与连接面板。 */
export function HostConnectionPanel({ model }: HostConnectionPanelProps): ReactElement {
  return (
    <section data-route="hosts">
      <header data-connection-state={model.connectionState}>
        <span data-connection-label="true">{model.connectionLabel}</span>
        {model.offlineSnapshotNote === null ? null : (
          <p data-offline-snapshot-note="true">{model.offlineSnapshotNote}</p>
        )}
        {model.blocking === null ? null : (
          <div data-blocking-detail={model.blocking.cause} role="alert">
            <p>{model.blocking.reason}</p>
            <p>{model.blocking.nextStep}</p>
          </div>
        )}
      </header>

      {model.unknownStateNote === null ? null : <p data-unknown-state-note="true">{model.unknownStateNote}</p>}

      <ul data-agent-list="configured">
        {model.agents.map((agent) => (
          <AgentRow key={agent.id} agent={agent} />
        ))}
      </ul>

      {model.host === null ? (
        <p data-host-identity="unpaired">尚未配对，没有可显示的主机身份信息。</p>
      ) : (
        <dl data-host-identity="paired">
          <dt>地址</dt>
          <dd data-host-address={model.host.address}>{model.host.address}</dd>
          <dt>主机 ID</dt>
          <dd>{model.host.hostId ?? "未知"}</dd>
          <dt>本机设备 ID</dt>
          <dd>{model.host.deviceId ?? "未知"}</dd>
        </dl>
      )}
    </section>
  );
}
