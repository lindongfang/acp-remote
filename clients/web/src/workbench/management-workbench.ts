import {
  ManagementConnection,
  ManagementConnectionError,
  type ManagementConnectionOptions,
} from "../management/client";
import type { ExpectedIdentity, NodeInfo } from "../management/protocol";
import { ManagementReconnect } from "../management/reconnect";
import {
  clearPersistedSourceViewSpec,
  loadPersistedSourceViewSpec,
  persistSourceViewSpec,
  type PersistedSourceViewSpec,
} from "../management/source-session-view";
import type { IdentityRecord, IdentityVault } from "../pairing/identity";
import type { PairingApi } from "../pairing/api";
import { PairingSessionError } from "../pairing/session";
import type { ViewDataFrame } from "../views/binary";
import { ApprovalController } from "./approval-controller";
import { CapacityReleaseController } from "./capacity-release-controller";
import { DeviceTrustController } from "./device-trust-panel";
import {
  captureActiveControlSelection, detailBlock, eyebrow,
  paragraph, restoreActiveControlSelection, statusLine, title,
} from "./dom-primitives";
import { managementErrorMessage } from "./error-formatting";
import { PairingAccessController } from "./pairing-access-controller";
import { QueuedBodyController, type QueuedBodyReaderFactory } from "./queued-body-controller";
import { SessionControlController } from "./session-control-controller";
import { SessionQueueController } from "./session-queue-controller";
import { SharedViewController, type SharedViewDisplayFactory } from "./shared-view-controller";
import { SourceSessionController } from "./source-session-controller";
import { WorkspaceCatalogController } from "./workspace-catalog-controller";

export type ManagementConnectionFactory = (
  identity: ExpectedIdentity,
  privateKey: CryptoKey,
  options: ManagementConnectionOptions,
) => ManagementConnection;

export interface ManagementWorkbenchOptions {
  readonly panel: HTMLElement;
  readonly sharedViewHost: HTMLElement;
  readonly doc?: Document;
  readonly location?: Pick<Location, "protocol" | "origin" | "host">;
  readonly connectionFactory?: ManagementConnectionFactory;
  readonly displayFactory?: SharedViewDisplayFactory;
  readonly bodyReaderFactory?: QueuedBodyReaderFactory;
  readonly persistSourceView?: (
    selection: PersistedSourceViewSpec["selection"],
    nativeSessionId: string,
  ) => void;
  readonly loadPersistedSourceView?: () => PersistedSourceViewSpec | null;
  readonly clearPersistedSourceView?: () => void;
  readonly createVault?: () => IdentityVault;
  readonly createPairingApi?: () => PairingApi;
  readonly pollIntervalMs?: number;
}

export class ManagementWorkbench {
  readonly #panel: HTMLElement;
  readonly #sharedViewHost: HTMLElement;
  readonly #doc?: Document;
  readonly #location?: Pick<Location, "protocol" | "origin" | "host">;
  readonly #connectionFactory: ManagementConnectionFactory;
  readonly #loadPersistedSourceView: () => PersistedSourceViewSpec | null;
  readonly #managementReconnect: ManagementReconnect;

  #management: ManagementConnection | null = null;
  #managementGeneration = 0;
  #currentNodeInfo: NodeInfo | null = null;
  #currentRecord: IdentityRecord | null = null;
  #pageActive = true;
  #manuallyDisconnected = false;
  #automaticReconnectAllowed = true;
  #lastManagementError: unknown = null;

  readonly pairing: PairingAccessController;
  readonly devices: DeviceTrustController;
  readonly sharedView: SharedViewController;
  readonly queuedBody: QueuedBodyController;
  readonly sessionControl: SessionControlController;
  readonly sessionQueue: SessionQueueController;
  readonly approval: ApprovalController;
  readonly capacityRelease: CapacityReleaseController;
  readonly workspaceCatalog: WorkspaceCatalogController;
  readonly sourceSession: SourceSessionController;

  constructor(options: ManagementWorkbenchOptions) {
    this.#panel = options.panel;
    this.#sharedViewHost = options.sharedViewHost;
    this.#doc = options.doc;
    this.#location = options.location;
    this.#connectionFactory =
      options.connectionFactory ??
      ((identity, key, opts) => new ManagementConnection(identity, key, opts));
    this.#loadPersistedSourceView =
      options.loadPersistedSourceView ?? (() => loadPersistedSourceViewSpec());
    this.#managementReconnect = new ManagementReconnect(() =>
      this.connectSavedManagement(),
    );

    const getDoc = () => this.#doc ?? globalThis.document;
    const getManagement = () => this.#management;
    const getManagementGeneration = () => this.#managementGeneration;
    const getInspectedQueueTargetId = () => this.sessionQueue.inspectedQueueTargetId;
    const renderDashboard = (rec: IdentityRecord, notice?: string, failure?: string) =>
      this.showManagementDashboard(rec, notice, failure);
    const getCurrentRecord = () => this.#currentRecord;
    const basePort = {
      getManagement,
      getManagementGeneration,
      getDocument: getDoc,
      renderDashboard,
    };

    this.sessionControl = new SessionControlController({
      ...basePort,
      getCurrentRecord,
      getActiveControlTargetId: () => this.#activeControlTargetId(),
    });
    const control = this.sessionControl.createIntegrationPort();

    this.sharedView = new SharedViewController({
      ...basePort,
      getInspectedQueueTargetId,
      getSharedViewHost: () => this.#sharedViewHost,
      displayFactory: options.displayFactory,
      persistSourceViewSpec:
        options.persistSourceView ?? ((sel, id) => persistSourceViewSpec(sel, id)),
      clearPersistedSourceViewSpec:
        options.clearPersistedSourceView ?? (() => clearPersistedSourceViewSpec()),
    });

    this.queuedBody = new QueuedBodyController({
      ...basePort,
      getInspectedQueueTargetId,
      readerFactory: options.bodyReaderFactory,
    });

    this.sessionQueue = new SessionQueueController({
      ...basePort,
      control,
      getOpenedSessionView: () => this.sharedView.openedSessionView,
      getSharedViewSpec: () => this.sharedView.sharedViewSpec,
      getBodyPreview: () => this.queuedBody.bodyPreview,
      onTargetSelected: () => {
        this.sharedView.disposeSharedViews();
        this.sharedView.clearSharedSourceViewSpec();
        this.resetSessionInputUi();
      },
      onCloseTargetSharedView: (rec, opened, sub) =>
        this.sharedView.closeTargetSharedView(rec, opened, sub),
      onOpenQueueBody: (rec, tid, entry, sub) =>
        this.queuedBody.openQueueBody(rec, tid, entry, sub),
      onOpenTargetSharedView: (rec, tid, sub) =>
        this.sharedView.openTargetSharedView(rec, tid, sub),
    });

    this.approval = new ApprovalController({ ...basePort, control });

    this.capacityRelease = new CapacityReleaseController({
      ...basePort,
      getInspectedQueueTargetId,
    });

    this.sourceSession = new SourceSessionController({
      ...basePort,
      getSharedViewSpec: () => this.sharedView.sharedViewSpec,
      getOpenedSessionView: () => this.sharedView.openedSessionView,
      getSharedViewStaleNotice: () => this.sharedView.sharedViewStaleNotice,
      onOpenSharedSourceView: (rec, sel, id, sub, reason) =>
        this.sharedView.openSharedSourceView(rec, sel, id, sub, reason),
      onCloseSharedSourceView: (rec, opened, sub) =>
        this.sharedView.closeSharedSourceView(rec, opened, sub),
      onAbandonSharedSourceView: (rec) => {
        this.sharedView.disposeSharedViews();
        this.sharedView.clearSharedSourceViewSpec();
        this.showManagementDashboard(rec, "已关闭本地来源只读视图。");
      },
    });

    this.workspaceCatalog = new WorkspaceCatalogController({
      ...basePort,
      getCurrentRecord: () => this.#currentRecord,
      onSyncWorkspaceSelection: (wid, path) =>
        this.sourceSession.syncWorkspaceSelection(wid, path),
      onSyncHarnessProfileSelection: (sid, hid, pid, rev) =>
        this.sourceSession.syncHarnessProfileSelection(sid, hid, pid, rev),
      onSelectTargetQueue: (rec, tid) => {
        this.sessionQueue.inspectedQueueTargetId = tid;
        this.sessionQueue.inspectedQueue = null;
        this.sessionQueue.sessionResume.clear();
        this.sharedView.disposeSharedViews();
        this.sharedView.clearSharedSourceViewSpec();
        this.resetSessionInputUi();
        void this.sessionQueue.loadSessionQueue(rec, tid, this.#managementGeneration);
      },
      onSelectTargetRelease: (rec, tid) => {
        this.capacityRelease.nativeReleaseTargetId = tid;
        this.showManagementDashboard(rec, "已填入原生释放 Target ID。");
      },
      onFillSourceSelection: (rec, selection, nativeId) => {
        this.sourceSession.fillSelectionAndNativeId(selection, nativeId);
        this.showManagementDashboard(rec, "已将已绑定 Target 配置填入来源会话面板。");
      },
    });

    this.devices = new DeviceTrustController({
      ...basePort,
      getCurrentRecord: () => this.#currentRecord,
      getPanel: () => this.#panel,
      onManualDisconnect: () => {
        this.#manuallyDisconnected = true;
        this.#managementReconnect.cancel();
        this.#management?.disconnect();
      },
    });

    this.pairing = new PairingAccessController({
      getPanel: () => this.#panel,
      getDocument: getDoc,
      getLocation: () => this.#location ?? globalThis.location,
      isPageActive: () => this.#pageActive,
      isManuallyDisconnected: () => this.#manuallyDisconnected,
      setManuallyDisconnected: (val) => { this.#manuallyDisconnected = val; },
      isAutomaticReconnectAllowed: () => this.#automaticReconnectAllowed,
      setAutomaticReconnectAllowed: (val) => { this.#automaticReconnectAllowed = val; },
      getLastManagementError: () => this.#lastManagementError,
      setCurrentRecord: (rec) => { this.#currentRecord = rec; },
      showManagementConnecting: (rec) => this.showManagementConnecting(rec),
      resumeManagement: () => this.resumeManagement(),
      openManagement: (rec, recovering) => this.openManagement(rec, recovering),
      onResetManagementAndSessionState: (resetReconnect) => {
        this.resetAllSessionState();
        if (resetReconnect) this.#managementReconnect.reset();
        else this.#managementReconnect.cancel();
        this.#management?.disconnect();
        this.#management = null;
        ++this.#managementGeneration;
        this.#currentRecord = null;
        this.#currentNodeInfo = null;
      },
      createVault: options.createVault,
      createPairingApi: options.createPairingApi,
      pollIntervalMs: options.pollIntervalMs,
    });
  }

  get management(): ManagementConnection | null {
    return this.#management;
  }

  get managementGeneration(): number {
    return this.#managementGeneration;
  }

  boot(restoringPage = false): Promise<void> {
    return this.pairing.boot(restoringPage);
  }

  suspendForDisconnect(): void {
    this.sharedView.suspendSharedView();
  }

  resetSessionInputUi(): void {
    this.sessionQueue.clearEditingEntry();
    this.queuedBody.reset();
    this.sourceSession.resetSourceResultRead();
    this.sourceSession.disposeSourceRawInspectors();
  }

  disposeAllSessionResources(): void {
    this.sharedView.disposeSharedViews();
    this.resetSessionInputUi();
  }

  resetAllSessionState(): void {
    this.sharedView.disposeSharedViews();
    this.sharedView.clearSharedSourceViewSpec();
    this.sourceSession.clearOperationViewState();
    this.resetSessionInputUi();
    this.sessionControl.resetAll();
    this.sessionQueue.reset();
    this.approval.reset();
    this.workspaceCatalog.resetInventory();
    this.capacityRelease.resetAll();
    this.devices.reset();
  }

  handlePageHide(): void {
    this.#pageActive = false;
    this.disposeAllSessionResources();
    this.pairing.abortForPageHide();
    this.#managementReconnect.cancel();
    const connection = this.#management;
    this.#management = null;
    ++this.#managementGeneration;
    connection?.disconnect();
  }

  handlePageShow(persisted: boolean): void {
    this.#pageActive = true;
    if (!persisted || this.#manuallyDisconnected) return;
    if (this.pairing.session === null || this.pairing.isIdentityAborted) {
      void this.boot(true);
    } else {
      this.resumeManagement();
    }
  }

  handleOnline(): void {
    this.resumeManagement();
  }

  resumeManagement(): void {
    if (this.#pageActive && !this.#manuallyDisconnected && this.#automaticReconnectAllowed) {
      this.#managementReconnect.resume();
    }
  }

  connectSavedManagement(): void {
    const record = this.#currentRecord;
    if (
      this.#pageActive &&
      !this.#manuallyDisconnected &&
      this.#automaticReconnectAllowed &&
      this.#management === null &&
      record !== null &&
      (record.status === "approved" || record.status === "consumed")
    ) {
      void this.openManagement(record);
    }
  }

  handleSessionViewData(record: IdentityRecord, generation: number, frame: ViewDataFrame): void {
    if (generation !== this.#managementGeneration) return;
    if (this.sharedView.acceptViewFrame(frame)) return;
    if (this.sourceSession.acceptViewFrame(record, generation, frame)) return;
    this.queuedBody.acceptViewFrame(record, generation, frame);
  }

  #isActiveConnection(generation: number, connection: ManagementConnection): boolean {
    return generation === this.#managementGeneration && this.#management === connection;
  }

  #activeControlTargetId(): string | null {
    return (
      this.sessionQueue.inspectedQueueTargetId ??
      this.approval.selectedApprovalTargetId() ??
      (this.sharedView.sharedViewSpec?.kind === "target"
        ? this.sharedView.sharedViewSpec.targetId
        : null)
    );
  }

  async openManagement(record: IdentityRecord, recoveringPending = false): Promise<void> {
    if (!this.#pageActive || this.#manuallyDisconnected || this.pairing.identityResetting) {
      return;
    }
    if (
      this.pairing.vault === null ||
      this.pairing.session === null ||
      record.nodeId === null ||
      record.nodePublicKey === null
    ) {
      this.pairing.showFatal("身份记录不完整", "缺少节点绑定资料，不会发起管理认证。");
      return;
    }

    this.pairing.abortPairing();
    this.#managementReconnect.cancel();
    const generation = ++this.#managementGeneration;
    this.#management?.disconnect();
    this.#management = null;
    this.#currentNodeInfo = null;
    this.devices.reset();
    this.workspaceCatalog.resetInventory();
    this.capacityRelease.resetCapacity();
    this.sessionControl.resetForConnection();
    this.sessionQueue.inspectedQueue = null;
    this.suspendForDisconnect();
    this.resetSessionInputUi();
    this.#currentRecord = record;
    this.showManagementConnecting(record);

    const loc = this.#location ?? globalThis.location;
    let authenticated = false;
    const connection = this.#connectionFactory(
      {
        origin: record.origin,
        nodeId: record.nodeId,
        nodePublicKey: record.nodePublicKey,
        clientId: record.clientId,
        clientPublicKey: record.publicKey,
      },
      record.privateKey,
      {
        origin: loc.origin,
        callbacks: {
          onDevicesChanged: () => {
            if (authenticated && generation === this.#managementGeneration) {
              void this.devices.refreshDevices(true, generation);
            }
          },
          onSessionControlChanged: (frame) => {
            if (authenticated && this.#isActiveConnection(generation, connection)) {
              this.sessionControl.handleControlChanged(
                this.#currentRecord ?? record,
                generation,
                frame,
              );
            }
          },
          onHistoryInvalidated: id => {
            if (authenticated && this.#isActiveConnection(generation, connection)) this.sharedView.historyInvalidated(id);
          },
          onViewData: (frame) => {
            if (generation === this.#managementGeneration) {
              this.handleSessionViewData(record, generation, frame);
            }
          },
          onClose: (close) => {
            if (authenticated && this.#isActiveConnection(generation, connection)) {
              this.#management = null;
              this.handleManagementClose(this.#currentRecord ?? record, close.code, close.error);
            }
          },
        },
      },
    );
    this.#management = connection;

    try {
      const authentication = await connection.connect();
      if (generation !== this.#managementGeneration) {
        connection.disconnect();
        return;
      }
      const currentSession = this.pairing.session;
      if (currentSession === null) throw new PairingSessionError("client_mismatch");
      const updated = await currentSession.acceptAuthenticated(authentication.clientId);
      if (generation !== this.#managementGeneration) {
        connection.disconnect();
        return;
      }
      this.#currentRecord = updated;
      authenticated = true;
      const info = await connection.nodeInfo();
      if (generation !== this.#managementGeneration) return;
      this.#currentNodeInfo = info;
      this.showManagementDashboard(updated, "节点签名、来源和客户端身份均已验证。");
      await connection.subscribeDevices();
      if (this.#isActiveConnection(generation, connection)) {
        this.#managementReconnect.reset();
        this.#automaticReconnectAllowed = true;
      }
      await this.devices.refreshDevices(true, generation);
      if (this.#isActiveConnection(generation, connection)) {
        await this.workspaceCatalog.refreshCatalogInventory(updated, generation, true);
      }
      const queueTargetId = this.sessionQueue.inspectedQueueTargetId;
      if (queueTargetId !== null && this.#isActiveConnection(generation, connection)) {
        await this.sessionQueue.loadSessionQueue(updated, queueTargetId, generation);
      }
      const restored = this.sharedView.restorePersistedSourceViewIfIdle(
        this.#loadPersistedSourceView,
        this.sessionQueue.inspectedQueueTargetId,
      );
      if (restored !== null) this.sourceSession.syncSourceDraftFromSpec(restored);
      if (this.#isActiveConnection(generation, connection)) {
        const activeSpec = this.sharedView.sharedViewSpec;
        if (activeSpec?.kind === "source") {
          await this.sharedView.openSharedSourceView(
            updated, activeSpec.selection, activeSpec.nativeSessionId, null, "reconnect",
          );
        } else {
          const viewTarget =
            activeSpec?.kind === "target" ? activeSpec.targetId : this.sharedView.sharedViewTargetId;
          if (viewTarget !== null) {
            await this.sharedView.openSharedSessionView(updated, viewTarget, null, "reconnect");
          }
        }
      }
    } catch (error) {
      if (generation !== this.#managementGeneration) return;
      connection.disconnect();
      this.#management = null;
      if (this.pairing.recoverChangedIdentity(error)) return;
      if (recoveringPending) {
        if (
          error instanceof ManagementConnectionError &&
          (error.closeCode === 4401 || error.closeCode === 4410)
        ) {
          this.showManagementFailure(record, error);
          return;
        }
        if (
          error instanceof ManagementConnectionError &&
          error.code === "revoked" &&
          error.nodeVerified
        ) {
          this.showRevokedAccess(record);
          return;
        }
        this.pairing.showPending(
          record,
          "节点暂未接受已保存身份，可能仍在等待本机批准或连接尚未恢复。可继续检查批准状态。",
        );
        void this.pairing.pollApproval();
        return;
      }
      this.showManagementFailure(this.#currentRecord ?? record, error);
    }
  }

  showManagementConnecting(record: IdentityRecord): void {
    const doc = this.#doc ?? globalThis.document;
    const fragment = this.pairing.identityFragment(record);
    fragment.append(
      eyebrow("MANAGEMENT / AUTHENTICATE", doc),
      title("正在认证管理连接", doc),
      paragraph(
        "节点会先证明自己的身份，浏览器再使用不可导出的私钥签署一次性挑战。",
        undefined,
        doc,
      ),
    );
    this.pairing.appendIdentityDetails(fragment, record);
    fragment.append(statusLine("正在通过同源 WSS 验证节点挑战…", "working", doc));
    this.#panel.replaceChildren(fragment);
  }

  showManagementDashboard(record: IdentityRecord, notice?: string, failure?: string): void {
    const doc = this.#doc ?? globalThis.document;
    const saved = captureActiveControlSelection(doc, this.#panel);
    this.sourceSession.disposeSourceRawInspectors();
    const fragment = doc.createDocumentFragment();
    fragment.append(
      eyebrow("NODE CONTROL / AUTHENTICATED", doc),
      title("节点管理", doc),
      paragraph(
        "当前身份已通过公钥持有证明。这里只显示完成后续 Harness 权限审批所需的节点与客户端身份资料。",
        undefined,
        doc,
      ),
    );
    if (notice !== undefined) fragment.append(statusLine(notice, "notice", doc));
    if (failure !== undefined) fragment.append(statusLine(failure, "error", doc));
    this.pairing.appendIdentityDetails(fragment, record, false);
    if (this.#currentNodeInfo !== null) {
      fragment.append(
        detailBlock("节点来源", this.#currentNodeInfo.origin, doc),
        detailBlock("节点状态", this.#currentNodeInfo.status, doc),
      );
    }
    fragment.append(
      this.sessionQueue.sessionQueuePanel(record),
      this.approval.sessionApprovalPanel(record),
      this.capacityRelease.capacityPanel(record),
      this.capacityRelease.nativeReleasePanel(record),
      this.workspaceCatalog.workspaceCatalogPanel(record),
      this.sourceSession.sourceSessionPanel(record),
    );
    this.devices.appendDevicesAndFooter(fragment, record);
    this.#panel.replaceChildren(fragment);
    restoreActiveControlSelection(doc, saved);
  }

  showManagementFailure(record: IdentityRecord, error: unknown): void {
    this.suspendForDisconnect();
    this.#lastManagementError = error;
    if (
      error instanceof ManagementConnectionError &&
      error.nodeVerified &&
      (error.code === "revoked" || error.closeCode === 4401)
    ) {
      this.showRevokedAccess(record);
      return;
    }
    const retryDelay =
      this.#pageActive && !this.#manuallyDisconnected
        ? this.#managementReconnect.schedule(error)
        : null;
    if (this.#pageActive && !this.#manuallyDisconnected) {
      this.#automaticReconnectAllowed = retryDelay !== null;
    }
    const doc = this.#doc ?? globalThis.document;
    const fragment = this.pairing.identityFragment(record);
    fragment.append(
      eyebrow(retryDelay === null ? "MANAGEMENT / STOPPED" : "MANAGEMENT / RECONNECTING", doc),
      title(retryDelay === null ? "管理连接已中断" : "正在恢复连接", doc),
      paragraph(managementErrorMessage(error), undefined, doc),
    );
    if (retryDelay !== null) {
      fragment.append(
        statusLine(
          `将在 ${retryDelay / 1_000} 秒后使用已保存的身份自动重连，无需重新配对。`,
          "working",
          doc,
        ),
      );
    } else if (this.#manuallyDisconnected) {
      fragment.append(
        paragraph("已主动断开连接，刷新页面后会使用原身份重新连接。", undefined, doc),
      );
    }
    this.pairing.appendIdentityDetails(fragment, record);
    this.#panel.replaceChildren(fragment);
  }

  handleManagementClose(record: IdentityRecord, code: number, error: Error): void {
    if (
      code === 4410 &&
      error instanceof ManagementConnectionError &&
      error.nodeVerified
    ) {
      this.showRevokedAccess(record);
      return;
    }
    this.showManagementFailure(record, error);
  }

  showRevokedAccess(record: IdentityRecord): void {
    this.sharedView.disposeSharedViews();
    this.sharedView.clearSharedSourceViewSpec();
    this.sourceSession.clearOperationViewState();
    this.#managementReconnect.cancel();
    this.#automaticReconnectAllowed = false;
    ++this.#managementGeneration;
    this.#management = null;
    void this.pairing.resetLocalIdentity(record);
  }
}
