import type { IdentityRecord } from "../pairing/identity";
import type { ManagementConnection } from "../management/client";
import {
  type SessionQueueEntry,
  type SessionQueueSnapshot,
  type SessionViewOpened,
  requireSessionId,
} from "../management/session-protocol";
import { SessionResumeCoordinator } from "../management/session-resume";
import { sessionQueueSnapshotView } from "../management/session-queue-view";
import {
  cancelSessionInput,
  editSessionText,
  prioritizeSessionInput,
  submitSessionText,
} from "../management/session-input-actions";
import { randomId128 } from "../protocol/pairing";
import {
  button,
  detailBlock,
  element,
  paragraph,
  statusLine,
} from "./dom-primitives";
import {
  managementErrorMessage,
  sessionActionErrorMessage,
} from "./error-formatting";
import type { SharedViewSpec } from "./shared-view-controller";

export interface SessionQueueHostPort {
  readonly getManagement: () => ManagementConnection | null;
  readonly getManagementGeneration: () => number;
  readonly getOpenedSessionView: () => SessionViewOpened | null;
  readonly getSharedViewSpec: () => SharedViewSpec | null;
  readonly getBodyPreview: () => string | null;
  readonly onTargetSelected: (targetId: string) => void;
  readonly onCloseTargetSharedView: (
    record: IdentityRecord,
    opened: SessionViewOpened,
    submit: HTMLButtonElement,
  ) => Promise<void>;
  readonly onOpenQueueBody: (
    record: IdentityRecord,
    targetId: string,
    entry: SessionQueueEntry,
    submit: HTMLButtonElement,
  ) => Promise<void>;
  readonly onOpenTargetSharedView?: (
    record: IdentityRecord,
    targetId: string,
    submit: HTMLButtonElement | null,
  ) => Promise<void>;
  readonly renderDashboard: (
    record: IdentityRecord,
    notice?: string,
    failure?: string,
  ) => void;
  readonly formatError?: (error: unknown) => string;
}

export class SessionQueueController {
  readonly #port: SessionQueueHostPort;
  readonly sessionResume = new SessionResumeCoordinator();

  #inspectedQueueTargetId: string | null = null;
  #inspectedQueue: SessionQueueSnapshot | null = null;
  #editingQueueEntry: SessionQueueEntry | null = null;

  constructor(port: SessionQueueHostPort) {
    this.#port = port;
  }

  get inspectedQueueTargetId(): string | null {
    return this.#inspectedQueueTargetId;
  }

  set inspectedQueueTargetId(value: string | null) {
    this.#inspectedQueueTargetId = value;
  }

  get inspectedQueue(): SessionQueueSnapshot | null {
    return this.#inspectedQueue;
  }

  set inspectedQueue(value: SessionQueueSnapshot | null) {
    this.#inspectedQueue = value;
  }

  get editingQueueEntry(): SessionQueueEntry | null {
    return this.#editingQueueEntry;
  }

  set editingQueueEntry(value: SessionQueueEntry | null) {
    this.#editingQueueEntry = value;
  }

  reset(): void {
    this.#inspectedQueueTargetId = null;
    this.#inspectedQueue = null;
    this.#editingQueueEntry = null;
    this.sessionResume.clear();
  }

  clearEditingEntry(): void {
    this.#editingQueueEntry = null;
  }

  sessionQueuePanel(record: IdentityRecord): HTMLElement {
    const section = element("section");
    section.append(
      element("h3", "section-title", "会话输入队列"),
      paragraph(
        "输入一个会话 target ID 可查看共享队列、停止原因和等待项。这里只显示队列元数据，不读取输入正文。",
        "field-help",
      ),
    );
    const form = element("form", "pair-form");
    const label = element("label", undefined, "会话 target ID");
    label.htmlFor = "session-target-id";
    const input = element("input") as HTMLInputElement;
    input.id = "session-target-id";
    input.name = "session-target-id";
    input.type = "text";
    input.maxLength = 22;
    input.required = true;
    input.autocomplete = "off";
    input.spellcheck = false;
    input.value = this.#inspectedQueueTargetId ?? "";
    input.placeholder = "22 位 canonical base64url ID";
    const submit = button("读取会话队列", "submit");
    submit.type = "submit";
    submit.disabled = this.#port.getManagement() === null;
    form.append(label, input, submit);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      let targetId: string;
      try {
        targetId = requireSessionId(input.value.trim());
      } catch {
        this.#port.renderDashboard(
          record,
          undefined,
          "会话 target ID 格式无效。",
        );
        return;
      }
      this.#inspectedQueueTargetId = targetId;
      this.#inspectedQueue = null;
      this.sessionResume.clear();
      this.#port.onTargetSelected(targetId);
      submit.disabled = true;
      void this.loadSessionQueue(
        record,
        targetId,
        this.#port.getManagementGeneration(),
      );
    });
    section.append(form);

    if (this.#inspectedQueueTargetId !== null) {
      section.append(
        this.sessionInputComposer(record, this.#inspectedQueueTargetId),
      );
      if (this.#editingQueueEntry !== null) {
        section.append(
          this.sessionEditComposer(
            record,
            this.#inspectedQueueTargetId,
            this.#editingQueueEntry,
          ),
        );
      }
    }

    const openedView = this.#port.getOpenedSessionView();
    const sharedSpec = this.#port.getSharedViewSpec();
    if (openedView !== null && sharedSpec?.kind !== "source") {
      const opened = element("div", "session-view-state");
      opened.append(
        statusLine(
          `共享视图已打开：${openedView.status.mode}，snapshot revision ${openedView.snapshotRevision}。`,
          "notice",
        ),
      );
      const close = button("关闭共享视图", "secondary-button");
      close.addEventListener("click", () => {
        close.disabled = true;
        void this.#port.onCloseTargetSharedView(record, openedView, close);
      });
      opened.append(close);
      section.append(opened);
    }

    const previewText = this.#port.getBodyPreview();
    if (previewText !== null) {
      const preview = element("pre", "body-preview", previewText);
      preview.setAttribute("aria-label", "队列输入正文");
      section.append(preview);
    }

    const snapshot = this.#inspectedQueue;
    if (this.#inspectedQueueTargetId !== null && snapshot === null) {
      section.append(statusLine("正在读取会话队列…", "working"));
      return section;
    }
    if (snapshot === null) {
      return section;
    }
    if (snapshot.targetId !== this.#inspectedQueueTargetId) {
      section.append(statusLine("队列响应与请求的会话不一致。", "error"));
      return section;
    }

    section.append(
      sessionQueueSnapshotView(snapshot, {
        onResume: (resumeButton) => {
          void this.resumeSessionQueue(record, snapshot.targetId, resumeButton);
        },
        onEdit: (entry) => {
          this.#editingQueueEntry = entry;
          this.#port.renderDashboard(
            record,
            "已选中队列项，请在编辑表单中提交替换正文。",
          );
        },
        onCancel: (entry, actionButton) => {
          void this.runSessionQueueControl(
            record,
            snapshot.targetId,
            entry,
            "cancel",
            actionButton,
          );
        },
        onPrioritize: (entry, actionButton) => {
          void this.runSessionQueueControl(
            record,
            snapshot.targetId,
            entry,
            "prioritize",
            actionButton,
          );
        },
        onOpenBody: (entry, actionButton) => {
          void this.#port.onOpenQueueBody(
            record,
            snapshot.targetId,
            entry,
            actionButton,
          );
        },
      }),
    );

    const refresh = button("刷新此队列", "secondary-button");
    refresh.addEventListener("click", () => {
      refresh.disabled = true;
      void this.loadSessionQueue(
        record,
        snapshot.targetId,
        this.#port.getManagementGeneration(),
      );
    });
    section.append(refresh);

    const openView = button("打开共享视图", "secondary-button");
    openView.addEventListener("click", () => {
      openView.disabled = true;
      if (this.#port.onOpenTargetSharedView) {
        void this.#port.onOpenTargetSharedView(
          record,
          snapshot.targetId,
          openView,
        );
      }
    });
    section.append(openView);
    return section;
  }

  sessionInputComposer(
    record: IdentityRecord,
    targetId: string,
  ): HTMLElement {
    const section = element("section", "session-composer");
    section.append(element("h4", undefined, "提交新输入"));
    const form = element("form", "pair-form");
    const label = element("label", undefined, "正文");
    label.htmlFor = "session-input-body";
    const textarea = element("textarea") as HTMLTextAreaElement;
    textarea.id = "session-input-body";
    textarea.name = "session-input-body";
    textarea.rows = 6;
    textarea.required = true;
    textarea.placeholder = "输入要发送给会话的文本";
    const submit = button("上传并提交", "submit");
    submit.type = "submit";
    form.append(label, textarea, submit);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      submit.disabled = true;
      void this.runSubmitSessionText(record, targetId, textarea.value, submit);
    });
    section.append(form);
    return section;
  }

  sessionEditComposer(
    record: IdentityRecord,
    targetId: string,
    entry: SessionQueueEntry,
  ): HTMLElement {
    const section = element("section", "session-composer");
    section.append(
      element("h4", undefined, "编辑队列项"),
      detailBlock("队列项", entry.itemId),
      detailBlock("expected revision", entry.revision),
    );
    const form = element("form", "pair-form");
    const label = element("label", undefined, "替换正文");
    label.htmlFor = "session-edit-body";
    const textarea = element("textarea") as HTMLTextAreaElement;
    textarea.id = "session-edit-body";
    textarea.name = "session-edit-body";
    textarea.rows = 6;
    textarea.required = true;
    const submit = button("上传并保存编辑", "submit");
    submit.type = "submit";
    const dismiss = button("放弃编辑", "secondary-button");
    dismiss.addEventListener("click", () => {
      this.#editingQueueEntry = null;
      this.#port.renderDashboard(record, "已放弃本次编辑，队列未被修改。");
    });
    const actions = element("div", "action-row");
    actions.append(dismiss, submit);
    form.append(label, textarea, actions);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      submit.disabled = true;
      dismiss.disabled = true;
      void this.runEditSessionText(
        record,
        targetId,
        entry,
        textarea.value,
        submit,
        dismiss,
      );
    });
    section.append(form);
    return section;
  }

  async runSubmitSessionText(
    record: IdentityRecord,
    targetId: string,
    text: string,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法提交输入。",
      );
      return;
    }
    try {
      const result = await submitSessionText(connection, targetId, text);
      if (generation !== this.#port.getManagementGeneration()) return;
      const notice =
        result.fact.state === "settled"
          ? result.fact.outcome.kind === "rejected"
            ? `输入已登记但被队列拒绝（${result.fact.outcome.reason}）；队列最新状态已重新读取。`
            : `输入已登记并结算为 ${result.fact.state}（${result.fact.outcome.kind}）；队列最新状态已重新读取。`
          : `输入已登记并处于 ${result.fact.state}；队列最新状态已重新读取。`;
      await this.loadSessionQueue(record, targetId, generation, notice);
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? sessionActionErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `提交未完成：${format(error)}`,
      );
    }
  }

  async runEditSessionText(
    record: IdentityRecord,
    targetId: string,
    entry: SessionQueueEntry,
    text: string,
    submit: HTMLButtonElement,
    dismiss: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法编辑输入。",
      );
      return;
    }
    try {
      const result = await editSessionText(
        connection,
        targetId,
        entry.itemId,
        entry.revision,
        text,
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#editingQueueEntry = null;
      const notice =
        result.fact.state === "settled"
          ? result.fact.outcome.kind === "rejected"
            ? `队列项编辑被拒绝（${result.fact.outcome.reason}）。`
            : `队列项编辑已结算为 ${result.fact.state}（${result.fact.outcome.kind}）。`
          : `队列项编辑当前状态为 ${result.fact.state}。`;
      await this.loadSessionQueue(record, targetId, generation, notice);
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      dismiss.disabled = false;
      const format = this.#port.formatError ?? sessionActionErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `编辑未完成：${format(error)}`,
      );
    }
  }

  async runSessionQueueControl(
    record: IdentityRecord,
    targetId: string,
    entry: SessionQueueEntry,
    action: "cancel" | "prioritize",
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法更新队列。",
      );
      return;
    }
    try {
      const fact =
        action === "cancel"
          ? await cancelSessionInput(
              connection,
              targetId,
              entry.itemId,
              entry.revision,
            )
          : await prioritizeSessionInput(
              connection,
              targetId,
              entry.itemId,
              entry.revision,
            );
      if (generation !== this.#port.getManagementGeneration()) return;
      const label = action === "cancel" ? "取消" : "立即发送";
      const notice =
        fact.state === "settled"
          ? fact.outcome.kind === "rejected"
            ? `${label}操作被队列拒绝（${fact.outcome.reason}）。`
            : `${label}操作已结算为 ${fact.state}（${fact.outcome.kind}）。`
          : `${label}操作当前状态为 ${fact.state}。`;
      await this.loadSessionQueue(record, targetId, generation, notice);
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? sessionActionErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `${action === "cancel" ? "取消" : "立即发送"}未完成：${format(error)}`,
      );
    }
  }

  async loadSessionQueue(
    record: IdentityRecord,
    targetId: string,
    generation: number,
    notice?: string,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    if (
      connection === null ||
      generation !== this.#port.getManagementGeneration()
    ) {
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法读取会话队列。",
      );
      return;
    }
    try {
      const snapshot = await connection.inspectSessionQueue(targetId);
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#port.getManagement() !== connection ||
        this.#inspectedQueueTargetId !== targetId
      ) {
        return;
      }
      this.#inspectedQueue = snapshot;
      this.#port.renderDashboard(record, notice ?? "会话队列已更新。");
    } catch (error) {
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#inspectedQueueTargetId !== targetId
      ) {
        return;
      }
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(record, undefined, format(error));
    }
  }

  async resumeSessionQueue(
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
        "管理连接已经关闭，无法继续旧队列。",
      );
      return;
    }
    try {
      const result = await this.sessionResume.run(
        targetId,
        () => ({
          registrationId: randomId128(),
          consumeRequestId: randomId128(),
        }),
        (resumeTargetId, registrationId) =>
          connection.registerSessionResume(resumeTargetId, registrationId),
        (resumeTargetId, registrationId, consumeRequestId) =>
          connection.consumeSessionResume(
            resumeTargetId,
            registrationId,
            consumeRequestId,
          ),
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#port.renderDashboard(
        record,
        `已显式恢复 ${result.restored} 个旧等待项；队列顺序由节点维护。`,
      );
      await this.loadSessionQueue(
        record,
        targetId,
        generation,
        `已显式恢复 ${result.restored} 个旧等待项。`,
      );
    } catch (error) {
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#port.getManagement() !== connection
      ) {
        return;
      }
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `继续未完成：${format(error)}。重试会沿用同一请求身份。`,
      );
    }
  }
}
