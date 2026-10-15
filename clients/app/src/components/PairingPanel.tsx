/**
 * 配对面板（首次配对 + 重新配对入口）。
 *
 * 页面**没有**任何凭据、MCP 配置或路径输入：设备私钥由 `src/platform/` 生成且不可导出，
 * 这些配置只能在电脑上完成。本组件只呈现配对状态机的结果，并提供唯一的输入入口——
 * 粘贴一段二维码内容（design D3 的取舍：v1 用粘贴而不是扫码，不需要摄像头也不必新增解码依赖）。
 *
 * ## 组件不判定、不解析
 *
 * 输入框只把**文本**交给 `onSubmit`（页面转给 `ClientStore`），结构校验、claim、轮询与落盘
 * 都在 store 与组合根里。提交后立刻清空输入框：粘进来的内容含一次性的配对秘密，
 * 不留在组件状态里，也不出现在呈现上。
 */

import { useState } from "react";
import type { ReactElement } from "react";

import type { PairingPageModel } from "../features/pairing-model";
import { isPairingInFlight } from "../features/pairing-model";

export interface PairingPanelProps {
  readonly model: PairingPageModel;
  /** 提交一段二维码内容（地址 / 片段 / 载荷）。 */
  readonly onSubmit: (text: string) => void;
  readonly onRestart: () => void;
}

/** 配对面板。 */
export function PairingPanel({ model, onSubmit, onRestart }: PairingPanelProps): ReactElement {
  const [pasted, setPasted] = useState("");
  // 进行中（内容已读到、还没到终态）：这一刻仓库里持有一份含 `pairingSecret` 的载荷，
  // 因此不再接受第二次提交，输入也随之禁用——避免两条并行的 claim 与轮询。
  const inFlight = isPairingInFlight(model);
  const canSubmit = !inFlight && pasted.trim().length > 0;

  return (
    <section data-route="pair" data-pairing-phase={model.phase}>
      <h1>与这台电脑配对</h1>
      <p data-pairing-guidance="true">{model.guidance}</p>

      <div data-pairing-entry="true">
        <input
          type="text"
          data-pairing-input="true"
          aria-label="配对二维码内容"
          placeholder="粘贴二维码内容"
          value={pasted}
          disabled={inFlight}
          onChange={(event) => {
            setPasted(event.target.value);
          }}
        />
        <button
          type="button"
          data-action="submit-pairing-qr"
          disabled={!canSubmit}
          onClick={() => {
            onSubmit(pasted.trim());
            setPasted("");
          }}
        >
          开始配对
        </button>
      </div>

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
