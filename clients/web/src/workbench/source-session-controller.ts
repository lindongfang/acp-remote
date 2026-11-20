import type { IdentityRecord } from "../pairing/identity";
import type { ManagementConnection } from "../management/client";
import type {
  SessionViewOpened,
  SourceLaunchSelectionInput,
  SourceOperationInput,
  SourceSessionFact,
  SourceSessionRegistration,
} from "../management/session-protocol";
import {
  SourceSessionResultReadError,
  SourceSessionResultReader,
  inspectSourceSessionResultPayload,
  type InspectedSourceSessionResult,
} from "../management/source-result-read";
import {
  sourceSessionPanelView,
  validateSourceOperationInput,
  validateSourceSelectionInput,
  validateSourceViewTargetInput,
} from "../management/source-session-view";
import type { ViewDataFrame } from "../views/binary";
import type { RawInspector } from "../views/raw-inspector";
import { managementErrorMessage } from "./error-formatting";
import type { SharedViewSpec } from "./shared-view-controller";

const SOURCE_RESULT_TIMEOUT_MS = 30_000;

export type SourceViewOpenReason =
  | "first_open"
  | "explicit_refresh"
  | "reopen"
  | "reconnect"
  | "reacquire";

export interface SourceSessionHostPort {
  readonly getManagement: () => ManagementConnection | null;
  readonly getManagementGeneration: () => number;
  readonly getSharedViewSpec: () => SharedViewSpec | null;
  readonly getOpenedSessionView: () => SessionViewOpened | null;
  readonly getSharedViewStaleNotice: () => string | null;
  readonly getDocument?: () => Document;
  readonly onOpenSharedSourceView: (
    record: IdentityRecord,
    selection: SourceLaunchSelectionInput,
    nativeSessionId: string,
    submit: HTMLButtonElement | null,
    reason: SourceViewOpenReason,
  ) => Promise<void>;
  readonly onCloseSharedSourceView: (
    record: IdentityRecord,
    opened: SessionViewOpened,
    submit: HTMLButtonElement,
  ) => Promise<void>;
  readonly onAbandonSharedSourceView: (record: IdentityRecord) => void;
  readonly renderDashboard: (
    record: IdentityRecord,
    notice?: string,
    failure?: string,
  ) => void;
  readonly formatError?: (error: unknown) => string;
}

export class SourceSessionController {
  readonly #port: SourceSessionHostPort;

  #sourceSelectionDraft: SourceLaunchSelectionInput = {
    sourceId: "",
    harnessId: "",
    profileId: "",
    profileRevision: "",
    workspaceId: "",
    launchPath: "",
  };
  #sourceDiscoverCursorDraft = "";
  #sourceManualNativeIdDraft = "";
  #sourceRegistration: SourceSessionRegistration | null = null;
  #sourceFact: SourceSessionFact | null = null;
  #activeSourceResultRead: {
    readonly streamId: string;
    readonly reader: SourceSessionResultReader;
    readonly fact: SourceSessionFact | null;
  } | null = null;
  #sourceResultTimer: ReturnType<typeof globalThis.setTimeout> | null = null;
  #sourceResultAttempt = 0;
  #inspectedSourceResult: InspectedSourceSessionResult | null = null;
  #sourceRawInspectorExpanded = false;
  readonly #sourceRawInspectors = new Set<RawInspector>();

  constructor(port: SourceSessionHostPort) {
    this.#port = port;
  }

  get activeSourceResultStreamId(): string | null {
    return this.#activeSourceResultRead?.streamId ?? null;
  }

  get manualNativeSessionIdDraft(): string {
    return this.#sourceManualNativeIdDraft;
  }

  syncWorkspaceSelection(workspaceId: string, canonicalPath: string): void {
    this.#sourceSelectionDraft = { ...this.#sourceSelectionDraft, workspaceId, launchPath: canonicalPath };
  }

  syncHarnessProfileSelection(sourceId: string, harnessId: string, profileId: string, profileRevision: string): void {
    this.#sourceSelectionDraft = { ...this.#sourceSelectionDraft, sourceId, harnessId, profileId, profileRevision };
  }

  fillSelectionAndNativeId(selection: SourceLaunchSelectionInput, nativeSessionId: string): void {
    this.#sourceSelectionDraft = selection;
    this.#sourceManualNativeIdDraft = nativeSessionId;
  }

  syncSourceDraftFromSpec(
    spec: Extract<SharedViewSpec, { readonly kind: "source" }>,
  ): void {
    const d = this.#sourceSelectionDraft;
    if (
      d.sourceId === "" &&
      d.harnessId === "" &&
      d.profileId === "" &&
      d.profileRevision === "" &&
      d.workspaceId === "" &&
      d.launchPath === ""
    ) {
      this.#sourceSelectionDraft = spec.selection;
    }
    if (this.#sourceManualNativeIdDraft === "") {
      this.#sourceManualNativeIdDraft = spec.nativeSessionId;
    }
  }

  disposeSourceRawInspectors(): void {
    for (const inspector of this.#sourceRawInspectors) {
      try {
        inspector.dispose();
      } catch {}
    }
    this.#sourceRawInspectors.clear();
  }

  resetSourceResultRead(bumpAttempt = true): void {
    if (bumpAttempt) ++this.#sourceResultAttempt;
    if (this.#activeSourceResultRead !== null) {
      this.#activeSourceResultRead.reader.seal();
      try {
        this.#port
          .getManagement()
          ?.detachSessionStream(this.#activeSourceResultRead.streamId);
      } catch {}
    }
    this.#activeSourceResultRead = null;
    if (this.#sourceResultTimer !== null) {
      globalThis.clearTimeout(this.#sourceResultTimer);
      this.#sourceResultTimer = null;
    }
  }

  clearOperationViewState(): void {
    this.resetSourceResultRead();
    this.disposeSourceRawInspectors();
    this.#sourceRegistration = null;
    this.#sourceFact = null;
    this.#inspectedSourceResult = null;
    this.#sourceRawInspectorExpanded = false;
  }

  sourceSessionPanel(record: IdentityRecord): HTMLElement {
    const doc = this.#port.getDocument?.() ?? globalThis.document;
    const sharedViewSpec = this.#port.getSharedViewSpec();
    const isSource = sharedViewSpec?.kind === "source";
    return sourceSessionPanelView(
      doc,
      {
        connected: this.#port.getManagement() !== null,
        selection: this.#sourceSelectionDraft,
        cursor: this.#sourceDiscoverCursorDraft,
        manualNativeSessionId: this.#sourceManualNativeIdDraft,
        registration: this.#sourceRegistration,
        fact: this.#sourceFact,
        result: this.#inspectedSourceResult,
        rawExpanded: this.#sourceRawInspectorExpanded,
        activeSourceView: isSource
          ? {
              selection: sharedViewSpec.selection,
              nativeSessionId: sharedViewSpec.nativeSessionId,
            }
          : null,
        openedView: isSource ? this.#port.getOpenedSessionView() : null,
        staleNotice: isSource ? this.#port.getSharedViewStaleNotice() : null,
      },
      {
        onSelectionFieldChange: (field, value) => {
          this.#sourceSelectionDraft = {
            ...this.#sourceSelectionDraft,
            [field]: value,
          };
        },
        onCursorChange: (value) => {
          this.#sourceDiscoverCursorDraft = value;
        },
        onManualNativeSessionIdChange: (value) => {
          this.#sourceManualNativeIdDraft = value;
        },
        onRegister: (kind, submit) =>
          void this.registerSourceSessionOp(record, kind, submit),
        onExecute: (submit) => void this.executeSourceSessionOp(record, submit),
        onRefreshStatus: (submit) =>
          void this.refreshSourceSessionStatus(record, submit),
        onReconcile: (submit) =>
          void this.reconcileSourceSessionOp(record, submit),
        onOpenResult: (submit) =>
          void this.openSourceSessionResultStream(record, submit),
        onReleaseResult: (submit) =>
          void this.releaseSourceSessionResultOp(record, submit),
        onClearOperation: () => {
          this.clearOperationViewState();
          this.#port.renderDashboard(record, "已清除当前来源操作显示。");
        },
        onUseNextCursor: (nextCursor) => {
          this.#sourceDiscoverCursorDraft = nextCursor;
          this.#port.renderDashboard(record, "已填入下一页 discover cursor。");
        },
        onOpenDiscoveredView: (nativeSessionId, submit) => {
          this.#sourceManualNativeIdDraft = nativeSessionId;
          void this.openSourceViewFromSelection(
            record,
            nativeSessionId,
            submit,
            "first_open",
          );
        },
        onOpenManualView: (submit) =>
          void this.openSourceViewFromSelection(
            record,
            this.#sourceManualNativeIdDraft,
            submit,
            "first_open",
          ),
        onRefreshSourceView: (submit) => {
          const spec = this.#port.getSharedViewSpec();
          if (spec?.kind !== "source") {
            submit.disabled = false;
            return;
          }
          void this.#port.onOpenSharedSourceView(
            record,
            spec.selection,
            spec.nativeSessionId,
            submit,
            "explicit_refresh",
          );
        },
        onCloseSourceView: (submit) => {
          const opened = this.#port.getOpenedSessionView();
          if (opened === null) {
            submit.disabled = false;
            return;
          }
          void this.#port.onCloseSharedSourceView(record, opened, submit);
        },
        onAbandonSourceView: () => this.#port.onAbandonSharedSourceView(record),
        onToggleRawResult: (expanded) => {
          this.#sourceRawInspectorExpanded = expanded;
          this.#port.renderDashboard(record);
        },
        onRegisterInspector: (inspector) => {
          this.#sourceRawInspectors.add(inspector);
        },
      },
    );
  }

  acceptViewFrame(
    record: IdentityRecord,
    generation: number,
    frame: ViewDataFrame,
  ): boolean {
    if (generation !== this.#port.getManagementGeneration()) return false;
    const read = this.#activeSourceResultRead;
    if (read === null || frame.streamId !== read.streamId) return false;
    let update;
    try {
      update = read.reader.accept(frame);
    } catch (error) {
      const reason =
        error instanceof SourceSessionResultReadError &&
        error.code === "result_too_large"
          ? "来源结果体积超出安全显示上限。"
          : "来源结果流序号、偏移或载荷结构无效。";
      this.failSourceResultRead(record, generation, reason);
      return true;
    }
    try {
      this.#port
        .getManagement()
        ?.sendSessionViewCredit(frame.streamId, frame.packetSequence);
    } catch (error) {
      const format = this.#port.formatError ?? managementErrorMessage;
      this.failSourceResultRead(
        record,
        generation,
        `无法确认来源结果分块：${format(error)}`,
      );
      return true;
    }
    if (update.state === "complete") {
      read.reader.seal();
      this.#activeSourceResultRead = null;
      if (this.#sourceResultTimer !== null) {
        globalThis.clearTimeout(this.#sourceResultTimer);
        this.#sourceResultTimer = null;
      }
      this.disposeSourceRawInspectors();
      this.#inspectedSourceResult = inspectSourceSessionResultPayload(
        update.rawBytes,
        read.fact ?? this.#sourceFact,
      );
      this.#sourceRawInspectorExpanded = false;
      this.#port.renderDashboard(
        record,
        this.#inspectedSourceResult.kind === "fallback"
          ? "来源结果已读取，但未能按预期结构解析，已切换为只读安全文本回退。"
          : "来源操作原始结果已完整读取并解析。",
      );
    }
    return true;
  }

  failSourceResultRead(
    record: IdentityRecord,
    generation: number,
    detail: string,
  ): void {
    this.resetSourceResultRead(true);
    if (generation === this.#port.getManagementGeneration()) {
      this.#port.renderDashboard(record, undefined, detail);
    }
  }

  async registerSourceSessionOp(
    record: IdentityRecord,
    kind: "discover" | "create",
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法登记来源操作。",
      );
      return;
    }
    let selection: SourceLaunchSelectionInput;
    let operation: SourceOperationInput;
    try {
      selection = validateSourceSelectionInput(this.#sourceSelectionDraft);
      operation = validateSourceOperationInput(
        selection,
        kind,
        this.#sourceDiscoverCursorDraft,
      );
      this.#sourceSelectionDraft = selection;
    } catch {
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        "来源选择参数或 discover cursor 格式无效，请检查 ID、revision 与非空路径/游标。",
      );
      return;
    }
    try {
      const registration = await connection.registerSourceSession(
        selection,
        operation,
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      this.resetSourceResultRead();
      this.disposeSourceRawInspectors();
      this.#sourceRegistration = registration;
      this.#sourceFact = null;
      this.#inspectedSourceResult = null;
      this.#sourceRawInspectorExpanded = false;
      this.#port.renderDashboard(
        record,
        `来源操作已登记（${registration.action}），尚未执行。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `来源操作登记失败：${format(error)}`,
      );
    }
  }

  async #runSourceFactAction(
    record: IdentityRecord,
    submit: HTMLButtonElement,
    missingMsg: string,
    call: (conn: ManagementConnection, opId: string) => Promise<SourceSessionFact>,
    noticePrefix: string,
    errorPrefix: string,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const registration = this.#sourceRegistration;
    const generation = this.#port.getManagementGeneration();
    if (connection === null || registration === null) {
      submit.disabled = false;
      this.#port.renderDashboard(record, undefined, missingMsg);
      return;
    }
    try {
      const fact = await call(connection, registration.sourceOperationId);
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#sourceFact = fact;
      this.#port.renderDashboard(
        record,
        `${noticePrefix}${fact.state} / ${fact.outcome}。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `${errorPrefix}${format(error)}`,
      );
    }
  }

  executeSourceSessionOp(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    return this.#runSourceFactAction(
      record,
      submit,
      "来源操作上下文不完整，无法执行。",
      (conn, opId) => conn.executeSourceSession(opId),
      "来源操作已推进至 ",
      "来源操作执行失败：",
    );
  }

  refreshSourceSessionStatus(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    return this.#runSourceFactAction(
      record,
      submit,
      "没有可查询的来源操作。",
      (conn, opId) => conn.inspectSourceSessionStatus(opId),
      "来源操作状态已刷新：",
      "来源状态读取失败：",
    );
  }

  reconcileSourceSessionOp(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    return this.#runSourceFactAction(
      record,
      submit,
      "没有可对账的来源操作。",
      (conn, opId) => conn.reconcileSourceSession(opId),
      "来源操作已完成对账：",
      "来源对账失败：",
    );
  }

  async openSourceSessionResultStream(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const registration = this.#sourceRegistration;
    const generation = this.#port.getManagementGeneration();
    if (connection === null || registration === null) {
      submit.disabled = false;
      this.#port.renderDashboard(record, undefined, "没有可读取结果的来源操作。");
      return;
    }
    const attempt = ++this.#sourceResultAttempt;
    this.resetSourceResultRead(false);
    this.disposeSourceRawInspectors();
    this.#inspectedSourceResult = null;
    this.#sourceRawInspectorExpanded = false;
    let openedStreamId: string | null = null;
    try {
      const descriptor = await connection.openSourceSessionResult(
        registration.sourceOperationId,
      );
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#port.getManagement() !== connection ||
        this.#sourceRegistration?.sourceOperationId !==
          registration.sourceOperationId ||
        attempt !== this.#sourceResultAttempt
      ) {
        try {
          connection.detachSessionStream(descriptor.streamId);
        } catch {}
        return;
      }
      openedStreamId = descriptor.streamId;
      const reader = new SourceSessionResultReader(
        descriptor.streamId,
        BigInt(descriptor.totalBytes),
      );
      this.#activeSourceResultRead = {
        streamId: descriptor.streamId,
        reader,
        fact: this.#sourceFact,
      };
      this.#sourceResultTimer = globalThis.setTimeout(() => {
        if (
          attempt === this.#sourceResultAttempt &&
          generation === this.#port.getManagementGeneration() &&
          this.#port.getManagement() === connection
        ) {
          this.failSourceResultRead(record, generation, "读取来源结果超时。");
        }
      }, SOURCE_RESULT_TIMEOUT_MS);
      connection.sendSessionViewCredit(descriptor.streamId, 0n);
      this.#port.renderDashboard(record, "正在逐块读取来源操作原始结果…");
    } catch (error) {
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#port.getManagement() !== connection ||
        attempt !== this.#sourceResultAttempt
      ) {
        return;
      }
      const format = this.#port.formatError ?? managementErrorMessage;
      const detail =
        error instanceof SourceSessionResultReadError &&
        error.code === "result_too_large"
          ? "来源结果体积超出当前页面安全显示上限。"
          : format(error);
      if (
        openedStreamId !== null &&
        this.#activeSourceResultRead?.streamId === openedStreamId
      ) {
        this.failSourceResultRead(record, generation, detail);
        submit.disabled = false;
        return;
      }
      if (openedStreamId !== null) {
        try {
          connection.detachSessionStream(openedStreamId);
        } catch {}
      }
      submit.disabled = false;
      this.#port.renderDashboard(record, undefined, `来源结果读取未完成：${detail}`);
    }
  }

  async releaseSourceSessionResultOp(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const registration = this.#sourceRegistration;
    const generation = this.#port.getManagementGeneration();
    if (connection === null || registration === null) {
      submit.disabled = false;
      this.#port.renderDashboard(record, undefined, "没有可释放结果的来源操作。");
      return;
    }
    try {
      await connection.releaseSourceSessionResult(registration.sourceOperationId);
      if (generation !== this.#port.getManagementGeneration()) return;
      this.resetSourceResultRead();
      const updatedFact = await connection.inspectSourceSessionStatus(
        registration.sourceOperationId,
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#sourceFact = updatedFact;
      this.#port.renderDashboard(record, "来源结果 offer 已释放。");
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(
        record,
        undefined,
        `释放来源结果失败：${format(error)}`,
      );
    }
  }

  async openSourceViewFromSelection(
    record: IdentityRecord,
    rawNativeSessionId: string,
    submit: HTMLButtonElement,
    reason: SourceViewOpenReason,
  ): Promise<void> {
    let validated;
    try {
      const selection = validateSourceSelectionInput(this.#sourceSelectionDraft);
      validated = validateSourceViewTargetInput(
        selection,
        rawNativeSessionId,
        reason,
      );
      this.#sourceSelectionDraft = validated.selection;
    } catch {
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        "来源选择或 nativeSessionId 无效，请先补全有效选择信息与非空原生会话 ID。",
      );
      return;
    }
    await this.#port.onOpenSharedSourceView(
      record,
      validated.selection,
      validated.nativeSessionId,
      submit,
      reason,
    );
  }
}
