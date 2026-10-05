/**
 * 连接状态徽标（R12 的连接状态在每一页都可见）。
 *
 * 徽标的数据来自连接状态机（R19 的主机面板把同一份状态渲染得更完整），
 * 组件本身不判定连接状态，也不从会话活跃度推断任何东西。
 */

import type { ReactElement } from "react";

import type { HostPanelModel } from "../features/host-panel-model";

export interface ConnectionBadgeProps {
  readonly panel: HostPanelModel;
  readonly onOpenHosts?: () => void;
}

/** 连接状态徽标。 */
export function ConnectionBadge({ panel, onOpenHosts }: ConnectionBadgeProps): ReactElement {
  return (
    <button
      type="button"
      data-connection-state={panel.connectionState}
      data-action="open-hosts"
      onClick={() => {
        onOpenHosts?.();
      }}
    >
      <span data-connection-label="true">{panel.connectionLabel}</span>
    </button>
  );
}
