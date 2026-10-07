/**
 * 配对面板（首次配对 + 重新配对入口）。
 *
 * 页面**没有**任何凭据、MCP 配置或路径输入：设备私钥由 `src/platform/` 生成且不可导出，
 * 这些配置只能在电脑上完成。本组件只呈现扫码结果、等待确认与四种非成功终态。
 */

import type { ReactElement } from "react";

import type { PairingPageModel } from "../features/pairing-model";

export interface PairingPanelProps {
  readonly model: PairingPageModel;
  readonly onRestart: () => void;
}

/** 配对面板。 */
export function PairingPanel({ model, onRestart }: PairingPanelProps): ReactElement {
  return (
    <section data-route="pair" data-pairing-phase={model.phase}>
      <h1>与这台电脑配对</h1>
      <p data-pairing-guidance="true">{model.guidance}</p>

      {model.host === null ? null : (
        <dl data-pairing-host="true">
          <dt>地址</dt>
          <dd data-host-address={model.host.address}>{model.host.address}</dd>
          <dt>主机 ID</dt>
          <dd>{model.host.hostId ?? "未知"}</dd>
        </dl>
      )}

      {model.awaitingDesktopConfirmation ? (
        <p data-pairing-awaiting-desktop="true">请到电脑上确认这次配对请求。</p>
      ) : null}

      {model.deviceId === null ? null : (
        <p data-pairing-device-id={model.deviceId}>本机设备 ID：{model.deviceId}</p>
      )}

      {model.error === null ? null : (
        <p data-pairing-error={model.error.code} role="alert">
          {model.error.message}
        </p>
      )}

      {model.canRestart ? (
        <button type="button" data-action="restart-pairing" onClick={onRestart}>
          重新扫码
        </button>
      ) : null}
    </section>
  );
}
