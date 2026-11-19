import type { IdentityRecord } from "../pairing/identity";
import type { ManagementConnection } from "../management/client";
import type {
  ApprovalRegistration,
  ApprovalStatus,
  ApprovalView,
} from "../management/session-protocol";
import {
  button,
  detailBlock,
  element,
  paragraph,
  statusLine,
} from "./dom-primitives";
import { managementErrorMessage } from "./error-formatting";

export interface ApprovalHostPort {
  readonly getManagement: () => ManagementConnection | null;
  readonly getManagementGeneration: () => number;
  readonly getDocument?: () => Document;
  readonly renderDashboard: (
    record: IdentityRecord,
    notice?: string,
    failure?: string,
  ) => void;
  readonly formatError?: (error: unknown) => string;
}

export function approvalStatusText(status: ApprovalStatus): string {
  const decision =
    status.decision === null
      ? "未裁决"
      : `已裁决 ${status.decision.choiceId ?? "unknown"}`;
  return `${status.validity}; ${decision}; 交付 ${status.delivery}; 原连接 ${status.sourceClose}`;
}

export class ApprovalController {
  readonly #port: ApprovalHostPort;

  #approvalList: readonly ApprovalView[] | null = null;
  #selectedApproval: ApprovalView | null = null;
  #approvalRegistration: ApprovalRegistration | null = null;
  #approvalStatus: ApprovalStatus | null = null;

  constructor(port: ApprovalHostPort) {
    this.#port = port;
  }

  get approvalList(): readonly ApprovalView[] | null {
    return this.#approvalList;
  }

  get selectedApproval(): ApprovalView | null {
    return this.#selectedApproval;
  }

  get approvalRegistration(): ApprovalRegistration | null {
    return this.#approvalRegistration;
  }

  get approvalStatus(): ApprovalStatus | null {
    return this.#approvalStatus;
  }

  reset(): void {
    this.#approvalList = null;
    this.#selectedApproval = null;
    this.#approvalRegistration = null;
    this.#approvalStatus = null;
  }

  sessionApprovalPanel(record: IdentityRecord): HTMLElement {
    const doc = this.#port.getDocument?.() ?? globalThis.document;
    const section = element("section", "session-composer", undefined, doc);
    section.append(
      element("h3", "section-title", "原生审批", doc),
      paragraph(
        "审批由节点从当前 Harness 连接读取。原始载荷只作为不可执行文本显示；登记和裁决都使用节点返回的规范 ID。",
        "field-help",
        doc,
      ),
    );

    const refresh = button("刷新审批列表", "secondary-button", doc);
    refresh.addEventListener("click", () => {
      refresh.disabled = true;
      void this.loadApprovalList(record, this.#port.getManagementGeneration());
    });
    section.append(refresh);

    const approvals = this.#approvalList;
    if (approvals === null) {
      section.append(paragraph("尚未读取审批列表。", "empty-state", doc));
      return section;
    }
    if (approvals.length === 0) {
      section.append(paragraph("当前没有待显示的审批。", "empty-state", doc));
      return section;
    }

    for (const approval of approvals) {
      const card = element("article", "device-card", undefined, doc);
      card.append(
        detailBlock("审批", approval.approvalId, doc),
        detailBlock("有效性", approval.status.validity, doc),
        detailBlock("来源协议", `ACP v${approval.source.protocolVersion}`, doc),
        detailBlock(
          "处理状态",
          `${approval.status.delivery} / ${approval.status.sourceClose}`,
          doc,
        ),
      );
      const select = button(
        this.#selectedApproval?.approvalId === approval.approvalId
          ? "重新读取此审批"
          : "读取此审批",
        "secondary-button",
        doc,
      );
      select.addEventListener("click", () => {
        select.disabled = true;
        void this.loadApproval(record, approval.approvalId, select);
      });
      card.append(select);
      section.append(card);
    }

    const approval = this.#selectedApproval;
    if (approval === null) {
      return section;
    }

    const detail = element("article", "session-composer", undefined, doc);
    detail.append(
      element("h4", undefined, "审批详情", doc),
      detailBlock("审批", approval.approvalId, doc),
      detailBlock("来源实例", approval.source.instanceId, doc),
      detailBlock("连接代际", approval.source.connectionEpoch, doc),
      detailBlock(
        "关联",
        approval.context.kind === "verified_input"
          ? `已验证输入 ${approval.context.dispatchId}`
          : "会话请求",
        doc,
      ),
      detailBlock(
        "当前裁决",
        approvalStatusText(this.#approvalStatus ?? approval.status),
        doc,
      ),
    );

    const limitations = element("div", "detail-block", undefined, doc);
    limitations.append(element("span", "detail-label", "显示限制", doc));
    if (approval.limitations.length === 0) {
      limitations.append(element("code", undefined, "无", doc));
    } else {
      for (const limitation of approval.limitations) {
        limitations.append(element("code", undefined, limitation, doc));
      }
    }
    detail.append(limitations);

    if (approval.content.availability === "available") {
      const raw = element(
        "pre",
        "body-preview approval-content",
        approval.content.rawJson,
        doc,
      );
      raw.setAttribute("aria-label", "审批原始载荷");
      detail.append(raw);
    } else {
      detail.append(statusLine("原始审批载荷当前不可用。", "error", doc));
    }

    detail.append(element("h4", undefined, "可选决定", doc));
    const choices = element("div", "action-row", undefined, doc);
    for (const choice of approval.choices) {
      const select = button(
        `${choice.ordinal}. ${choice.originalKind} (${choice.originalOptionId})`,
        choice.selectable ? "secondary-button" : undefined,
        doc,
      );
      select.disabled = !choice.selectable;
      select.addEventListener("click", () => {
        select.disabled = true;
        void this.registerApprovalChoice(
          record,
          approval.approvalId,
          choice.choiceId,
          select,
        );
      });
      choices.append(select);
    }
    detail.append(choices);

    if (this.#approvalRegistration !== null) {
      detail.append(
        detailBlock(
          "已登记决定",
          this.#approvalRegistration.approvalRegistrationId,
          doc,
        ),
      );
      const actions = element("div", "action-row", undefined, doc);
      const decide = button("提交已登记决定", "submit", doc);
      decide.addEventListener("click", () => {
        decide.disabled = true;
        void this.submitApprovalDecision(record, decide);
      });
      const inspect = button("刷新审批状态", "secondary-button", doc);
      inspect.addEventListener("click", () => {
        inspect.disabled = true;
        void this.refreshApprovalStatus(record, inspect);
      });
      actions.append(decide, inspect);
      detail.append(actions);
    }

    section.append(detail);
    return section;
  }

  async loadApprovalList(
    record: IdentityRecord,
    generation: number,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    if (
      connection === null ||
      generation !== this.#port.getManagementGeneration()
    ) {
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法读取审批列表。",
      );
      return;
    }
    try {
      const parsed = await connection.listApprovals();
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#approvalList = parsed.items;
      if (this.#selectedApproval !== null) {
        this.#selectedApproval =
          parsed.items.find(
            (item) => item.approvalId === this.#selectedApproval?.approvalId,
          ) ?? null;
        if (this.#selectedApproval === null) {
          this.#approvalRegistration = null;
          this.#approvalStatus = null;
        }
      }
      this.#port.renderDashboard(
        record,
        `审批列表已更新，共 ${parsed.items.length} 项。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `审批列表读取失败：${format(error)}`,
      );
    }
  }

  async loadApproval(
    record: IdentityRecord,
    approvalId: string,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法读取审批。",
      );
      return;
    }
    try {
      const approval = await connection.readApproval(approvalId);
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#selectedApproval = approval;
      this.#approvalRegistration = null;
      this.#approvalStatus = approval.status;
      this.#port.renderDashboard(record, "审批详情已读取。");
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `审批读取失败：${format(error)}`,
      );
    }
  }

  async registerApprovalChoice(
    record: IdentityRecord,
    approvalId: string,
    choiceId: string,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法登记决定。",
      );
      return;
    }
    try {
      const registration = await connection.registerApproval(
        approvalId,
        choiceId,
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#approvalRegistration = registration;
      this.#port.renderDashboard(record, "审批决定已登记，尚未交付给 Harness。");
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `审批决定登记失败：${format(error)}`,
      );
    }
  }

  async submitApprovalDecision(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const approval = this.#selectedApproval;
    const registration = this.#approvalRegistration;
    const generation = this.#port.getManagementGeneration();
    if (connection === null || approval === null || registration === null) {
      this.#port.renderDashboard(record, undefined, "审批决定登记上下文不完整。");
      return;
    }
    try {
      const result = await connection.decideApproval(
        approval.approvalId,
        registration.approvalRegistrationId,
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      const status = await connection.inspectApprovalStatus(approval.approvalId);
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#approvalStatus = status;
      this.#selectedApproval = { ...approval, status };
      this.#port.renderDashboard(
        record,
        `节点已${result.state === "accepted" ? "接受" : "确认已有"}该唯一决定；交付状态为 ${status.delivery}。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `审批决定提交失败：${format(error)}`,
      );
    }
  }

  async refreshApprovalStatus(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const approval = this.#selectedApproval;
    const generation = this.#port.getManagementGeneration();
    if (connection === null || approval === null) {
      this.#port.renderDashboard(record, undefined, "没有可刷新的审批。");
      return;
    }
    try {
      const status = await connection.inspectApprovalStatus(approval.approvalId);
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#approvalStatus = status;
      this.#selectedApproval = { ...approval, status };
      this.#port.renderDashboard(record, "审批状态已刷新。");
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `审批状态读取失败：${format(error)}`,
      );
    }
  }
}
