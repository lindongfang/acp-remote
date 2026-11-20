import type { IdentityRecord } from "../pairing/identity";
import type { ManagementConnection } from "../management/client";
import { sessionCapacityPanel } from "../management/session-capacity";
import {
  requireSessionId,
  type NativeReleaseRegistered,
  type NativeReleaseStatus,
  type SessionCapacity,
} from "../management/session-protocol";
import { button, detailBlock, element, paragraph } from "./dom-primitives";
import { managementErrorMessage } from "./error-formatting";

export interface CapacityReleaseHostPort {
  readonly getManagement: () => ManagementConnection | null;
  readonly getManagementGeneration: () => number;
  readonly getInspectedQueueTargetId: () => string | null;
  readonly getDocument?: () => Document;
  readonly renderDashboard: (
    record: IdentityRecord,
    notice?: string,
    failure?: string,
  ) => void;
  readonly formatError?: (error: unknown) => string;
}

export class CapacityReleaseController {
  readonly #port: CapacityReleaseHostPort;

  #sessionCapacity: SessionCapacity | null = null;
  #nativeReleaseTargetId: string | null = null;
  #nativeReleaseRegistration: NativeReleaseRegistered | null = null;
  #nativeReleaseStatus: NativeReleaseStatus | null = null;

  constructor(port: CapacityReleaseHostPort) {
    this.#port = port;
  }

  get sessionCapacity(): SessionCapacity | null {
    return this.#sessionCapacity;
  }

  get nativeReleaseTargetId(): string | null {
    return this.#nativeReleaseTargetId;
  }

  set nativeReleaseTargetId(value: string | null) {
    this.#nativeReleaseTargetId = value;
  }

  get nativeReleaseRegistration(): NativeReleaseRegistered | null {
    return this.#nativeReleaseRegistration;
  }

  resetCapacity(): void {
    this.#sessionCapacity = null;
  }

  resetAll(): void {
    this.#sessionCapacity = null;
    this.#nativeReleaseTargetId = null;
    this.#nativeReleaseRegistration = null;
    this.#nativeReleaseStatus = null;
  }

  capacityPanel(record: IdentityRecord): HTMLElement {
    const doc = this.#port.getDocument?.() ?? globalThis.document;
    return sessionCapacityPanel(
      doc,
      this.#sessionCapacity,
      this.#port.getManagement() !== null,
      (submit) => {
        void this.refreshSessionCapacity(record, submit);
      },
    );
  }

  nativeReleasePanel(record: IdentityRecord): HTMLElement {
    const doc = this.#port.getDocument?.() ?? globalThis.document;
    const section = element("section", "session-composer", undefined, doc);
    section.append(
      element("h3", "section-title", "原生会话释放", doc),
      paragraph(
        "先登记释放操作，再明确执行。只有节点确认原会话关闭并可靠记录结果后，界面才显示已释放。",
        "field-help",
        doc,
      ),
    );

    const form = element("form", "pair-form", undefined, doc);
    const label = element("label", undefined, "待释放 target ID", doc);
    label.htmlFor = "native-release-target";
    const input = element("input", undefined, undefined, doc) as HTMLInputElement;
    input.id = "native-release-target";
    input.name = "native-release-target";
    input.type = "text";
    input.maxLength = 22;
    input.required = true;
    input.autocomplete = "off";
    input.spellcheck = false;
    input.value =
      this.#nativeReleaseTargetId ??
      this.#port.getInspectedQueueTargetId() ??
      "";
    input.placeholder = "22 位 canonical base64url ID";
    input.disabled = this.#nativeReleaseRegistration !== null;
    const register = button("登记释放", "submit", doc);
    register.type = "submit";
    register.disabled = this.#nativeReleaseRegistration !== null;
    form.append(label, input, register);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      let targetId: string;
      try {
        targetId = requireSessionId(input.value.trim());
      } catch {
        this.#port.renderDashboard(
          record,
          undefined,
          "释放 target ID 格式无效。",
        );
        return;
      }
      register.disabled = true;
      void this.registerNativeRelease(record, targetId, register);
    });
    section.append(form);

    const registration = this.#nativeReleaseRegistration;
    if (registration === null) {
      return section;
    }
    section.append(
      detailBlock("释放操作", registration.releaseOperationId, doc),
      detailBlock("目标", registration.targetId, doc),
    );
    if (this.#nativeReleaseStatus !== null) {
      section.append(
        detailBlock(
          "释放状态",
          this.#nativeReleaseStatus.state === "failed"
            ? `failed / ${this.#nativeReleaseStatus.error}`
            : this.#nativeReleaseStatus.state,
          doc,
        ),
      );
    }
    const actions = element("div", "action-row", undefined, doc);
    const release = button(
      this.#nativeReleaseStatus?.state === "released"
        ? "重试释放协调"
        : "执行原生释放",
      "danger-button",
      doc,
    );
    release.addEventListener("click", () => {
      release.disabled = true;
      void this.executeNativeRelease(record, release);
    });
    const inspect = button("刷新释放状态", "secondary-button", doc);
    inspect.addEventListener("click", () => {
      inspect.disabled = true;
      void this.refreshNativeReleaseStatus(record, inspect);
    });
    const another = button("登记另一目标", "secondary-button", doc);
    another.addEventListener("click", () => {
      this.#nativeReleaseTargetId = null;
      this.#nativeReleaseRegistration = null;
      this.#nativeReleaseStatus = null;
      this.#port.renderDashboard(record, "已清除本地释放操作显示。");
    });
    actions.append(release, inspect, another);
    section.append(actions);
    return section;
  }

  async refreshSessionCapacity(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) return;
    try {
      const capacity = await connection.inspectSessionCapacity();
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#sessionCapacity = capacity;
      this.#port.renderDashboard(record, "会话名额已从节点刷新。");
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#sessionCapacity = null;
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `名额读取失败：${format(error)}`,
      );
    }
  }

  async registerNativeRelease(
    record: IdentityRecord,
    targetId: string,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法登记释放。",
      );
      return;
    }
    try {
      const registration = await connection.registerNativeRelease(targetId);
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#nativeReleaseTargetId = registration.targetId;
      this.#nativeReleaseRegistration = registration;
      this.#nativeReleaseStatus = null;
      this.#port.renderDashboard(record, "释放操作已登记，尚未执行原生关闭。");
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `释放登记失败：${format(error)}`,
      );
    }
  }

  async executeNativeRelease(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const registration = this.#nativeReleaseRegistration;
    const generation = this.#port.getManagementGeneration();
    if (connection === null || registration === null) {
      this.#port.renderDashboard(record, undefined, "释放登记上下文不完整。");
      return;
    }
    try {
      const status = await connection.releaseNativeSession(
        registration.releaseOperationId,
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#nativeReleaseStatus = status;
      this.#sessionCapacity = null;
      this.#port.renderDashboard(
        record,
        status.state === "released"
          ? "原生会话已关闭，必要管理结果已由节点确认。"
          : `原生释放当前状态为 ${status.state}。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `原生释放失败：${format(error)}`,
      );
    }
  }

  async refreshNativeReleaseStatus(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const registration = this.#nativeReleaseRegistration;
    const generation = this.#port.getManagementGeneration();
    if (connection === null || registration === null) {
      this.#port.renderDashboard(record, undefined, "没有可刷新的释放操作。");
      return;
    }
    try {
      const status = await connection.inspectNativeReleaseStatus(
        registration.releaseOperationId,
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#nativeReleaseStatus = status;
      this.#port.renderDashboard(record, "原生释放状态已刷新。");
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `释放状态读取失败：${format(error)}`,
      );
    }
  }
}
