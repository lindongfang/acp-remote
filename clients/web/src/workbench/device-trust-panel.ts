import type { IdentityRecord } from "../pairing/identity";
import {
  ManagementConnectionError,
  ManagementRequestError,
  type ManagementConnection,
} from "../management/client";
import type { DevicePage, DeviceSummary } from "../management/protocol";
import {
  button,
  detailBlock,
  element,
  eyebrow,
  formatTimestamp,
  paragraph,
  title,
} from "./dom-primitives";
import { managementErrorMessage } from "./error-formatting";

export interface DeviceTrustHostPort {
  readonly getManagement: () => ManagementConnection | null;
  readonly getManagementGeneration: () => number;
  readonly getCurrentRecord: () => IdentityRecord | null;
  readonly getPanel: () => HTMLElement;
  readonly getDocument?: () => Document;
  readonly onManualDisconnect: () => void;
  readonly renderDashboard: (
    record: IdentityRecord,
    notice?: string,
    failure?: string,
  ) => void;
  readonly formatError?: (error: unknown) => string;
}

export function deviceCard(
  device: DeviceSummary,
  currentClientId: string | null,
  revoke: () => void,
  doc: Document = globalThis.document,
): HTMLElement {
  const card = element("article", "device-card", undefined, doc);
  const header = element("div", "device-card-header", undefined, doc);
  const name = element("div", undefined, undefined, doc);
  name.append(
    element("h4", undefined, device.displayName, doc),
    element(
      "span",
      "device-id",
      device.clientId === currentClientId ? "本设备" : device.clientId,
      doc,
    ),
  );
  header.append(
    name,
    element(
      "span",
      `device-status device-status-${device.status}`,
      device.status === "trusted" ? "可信" : "已撤销",
      doc,
    ),
  );
  card.append(
    header,
    detailBlock("公钥指纹", device.publicKeyFingerprint, doc),
    detailBlock("创建时间", formatTimestamp(device.createdAt), doc),
  );
  if (device.revokedAt !== null) {
    card.append(detailBlock("撤销时间", formatTimestamp(device.revokedAt), doc));
  }
  if (device.status === "trusted") {
    const revokeButton = button(
      device.clientId === currentClientId ? "撤销本设备访问" : "撤销此客户端",
      "danger-button",
      doc,
    );
    revokeButton.addEventListener("click", revoke);
    card.append(revokeButton);
  }
  return card;
}

export class DeviceTrustController {
  readonly #port: DeviceTrustHostPort;

  #currentDevices: readonly DeviceSummary[] = [];
  #currentRevision: number | null = null;
  #nextDeviceCursor: string | null = null;
  #devicesRefreshInFlight = false;
  #refreshAfterCurrentRequest = false;

  constructor(port: DeviceTrustHostPort) {
    this.#port = port;
  }

  get currentDevices(): readonly DeviceSummary[] {
    return this.#currentDevices;
  }

  get currentRevision(): number | null {
    return this.#currentRevision;
  }

  get nextDeviceCursor(): string | null {
    return this.#nextDeviceCursor;
  }

  reset(): void {
    this.#currentDevices = [];
    this.#currentRevision = null;
    this.#nextDeviceCursor = null;
    this.#devicesRefreshInFlight = false;
    this.#refreshAfterCurrentRequest = false;
  }

  applyDevicePage(page: DevicePage, replace: boolean): void {
    if (replace) {
      this.#currentDevices = page.items;
    } else {
      const known = new Set(this.#currentDevices.map((item) => item.clientId));
      this.#currentDevices = [
        ...this.#currentDevices,
        ...page.items.filter((item) => !known.has(item.clientId)),
      ];
    }
    this.#currentRevision = page.revision;
    this.#nextDeviceCursor = page.nextCursor;
  }

  appendDevicesAndFooter(
    fragment: DocumentFragment,
    record: IdentityRecord,
  ): void {
    const doc = this.#port.getDocument?.() ?? globalThis.document;
    fragment.append(
      element("h3", "section-title", "已登记客户端", doc),
      paragraph(
        this.#currentRevision === null
          ? "正在读取客户端列表…"
          : `列表 revision ${this.#currentRevision}，单页最多显示 32 个客户端。`,
        "field-help",
        doc,
      ),
    );
    if (this.#currentDevices.length === 0) {
      fragment.append(paragraph("当前没有已登记客户端。", "empty-state", doc));
    } else {
      for (const device of this.#currentDevices) {
        fragment.append(
          deviceCard(
            device,
            record.clientId,
            () => this.showRevokeConfirmation(record, device),
            doc,
          ),
        );
      }
    }

    const actions = element("div", "action-row", undefined, doc);
    const refresh = button("刷新列表", "secondary-button", doc);
    refresh.addEventListener("click", () => {
      void this.refreshDevices(true, this.#port.getManagementGeneration());
    });
    actions.append(refresh);
    if (this.#nextDeviceCursor !== null) {
      const more = button("载入下一页", "secondary-button", doc);
      more.addEventListener("click", () => {
        void this.refreshDevices(false, this.#port.getManagementGeneration());
      });
      actions.append(more);
    }
    const disconnect = button("断开管理连接", "secondary-button", doc);
    disconnect.addEventListener("click", () => {
      this.#port.onManualDisconnect();
    });
    actions.append(disconnect);
    fragment.append(actions);
  }

  showRevokeConfirmation(record: IdentityRecord, device: DeviceSummary): void {
    const doc = this.#port.getDocument?.() ?? globalThis.document;
    const self = device.clientId === record.clientId;
    const fragment = doc.createDocumentFragment();
    fragment.append(
      eyebrow("REVOKE / CONFIRM", doc),
      title(self ? "撤销本设备访问？" : "撤销此客户端？", doc),
      paragraph(
        self
          ? "提交后节点会立即关闭本设备的管理连接。此操作不会删除浏览器本地密钥；旧密钥被撤销后不能再次配对。"
          : `客户端“${device.displayName}”的现有连接和订阅将被关闭，之后不能再次认证。`,
        undefined,
        doc,
      ),
      detailBlock("客户端 ID", device.clientId, doc),
      detailBlock("公钥指纹", device.publicKeyFingerprint, doc),
    );
    const actions = element("div", "action-row", undefined, doc);
    const cancel = button("返回", "secondary-button", doc);
    cancel.addEventListener("click", () =>
      this.#port.renderDashboard(record, "未提交撤销。"),
    );
    const confirm = button("确认撤销", "danger-button", doc);
    confirm.addEventListener("click", () => {
      confirm.disabled = true;
      cancel.disabled = true;
      void this.revokeSelectedDevice(record, device, confirm);
    });
    actions.append(cancel, confirm);
    fragment.append(actions);
    this.#port.getPanel().replaceChildren(fragment);
  }

  async revokeSelectedDevice(
    record: IdentityRecord,
    device: DeviceSummary,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    if (connection === null) {
      this.#port.renderDashboard(record, undefined, "管理连接已经关闭。");
      return;
    }
    try {
      await connection.revokeDevice(device.clientId);
      if (device.clientId !== record.clientId) {
        await this.refreshDevices(true, this.#port.getManagementGeneration());
      }
    } catch (error) {
      if (
        error instanceof ManagementConnectionError &&
        error.code === "revoked"
      ) {
        return;
      }
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(record, undefined, format(error));
    }
  }

  async refreshDevices(reset: boolean, generation: number): Promise<void> {
    const connection = this.#port.getManagement();
    const record = this.#port.getCurrentRecord();
    if (
      connection === null ||
      record === null ||
      generation !== this.#port.getManagementGeneration()
    ) {
      return;
    }
    if (this.#devicesRefreshInFlight) {
      this.#refreshAfterCurrentRequest ||= reset;
      return;
    }
    this.#devicesRefreshInFlight = true;
    try {
      const cursor = reset ? null : this.#nextDeviceCursor;
      const page = await connection.listDevices(cursor);
      if (generation !== this.#port.getManagementGeneration()) {
        return;
      }
      this.applyDevicePage(page, cursor === null);
      this.#port.renderDashboard(
        this.#port.getCurrentRecord() ?? record,
        reset ? "设备列表已更新。" : "已载入下一页客户端。",
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) {
        return;
      }
      if (
        error instanceof ManagementRequestError &&
        error.code === "stale_cursor" &&
        !reset
      ) {
        this.#refreshAfterCurrentRequest = true;
        return;
      }
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        this.#port.getCurrentRecord() ?? record,
        undefined,
        format(error),
      );
    } finally {
      this.#devicesRefreshInFlight = false;
      if (
        this.#refreshAfterCurrentRequest &&
        generation === this.#port.getManagementGeneration()
      ) {
        this.#refreshAfterCurrentRequest = false;
        void this.refreshDevices(true, generation);
      }
    }
  }
}
