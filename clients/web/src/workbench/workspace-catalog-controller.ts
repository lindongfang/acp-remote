import type { IdentityRecord } from "../pairing/identity";
import {
  CatalogActionError,
  buildBindTargetInitialDocument,
  buildInstallHarnessDocument,
  type ManagementConnection,
} from "../management/client";
import { managementRequestErrorMessage } from "../management/error-messages";
import type { ManagementErrorCode } from "../management/protocol";
import {
  requireSessionId,
  type CatalogInventory,
  type SourceLaunchSelectionInput,
} from "../management/session-protocol";
import {
  CatalogDraftValidationError,
  createInitialInstallDraft,
  createInitialTargetDraft,
  validateBindTargetInput,
  validateInstallHarnessInput,
  workspaceCatalogPanelView,
  type CatalogInstallDraft,
  type CatalogTargetDraft,
} from "../management/workspace-catalog-view";
import { managementErrorMessage } from "./error-formatting";

export interface WorkspaceCatalogHostPort {
  readonly getManagement: () => ManagementConnection | null;
  readonly getManagementGeneration: () => number;
  readonly getCurrentRecord: () => IdentityRecord | null;
  readonly getDocument?: () => Document;
  readonly onSyncWorkspaceSelection: (
    workspaceId: string,
    canonicalPath: string,
  ) => void;
  readonly onSyncHarnessProfileSelection: (
    sourceId: string,
    harnessId: string,
    profileId: string,
    profileRevision: string,
  ) => void;
  readonly onSelectTargetQueue: (
    record: IdentityRecord,
    targetId: string,
  ) => void;
  readonly onSelectTargetRelease: (
    record: IdentityRecord,
    targetId: string,
  ) => void;
  readonly onFillSourceSelection: (
    record: IdentityRecord,
    selection: SourceLaunchSelectionInput,
    nativeSessionId: string,
  ) => void;
  readonly renderDashboard: (
    record: IdentityRecord,
    notice?: string,
    failure?: string,
  ) => void;
  readonly formatError?: (error: unknown) => string;
}

export function catalogActionErrorText(
  error: unknown,
  fallbackFormat: (err: unknown) => string = managementErrorMessage,
): string {
  if (error instanceof CatalogDraftValidationError) {
    return error.message;
  }
  if (error instanceof CatalogActionError) {
    if (error.code === "preparation_timeout") {
      return "目录文档上传校验超时，请检查节点状态后重试。";
    }
    if (error.code === "upload_failed") {
      if (error.detail !== null) {
        const mapped = managementRequestErrorMessage(
          error.detail as ManagementErrorCode,
        );
        return mapped !== undefined
          ? `目录文档校验失败：${mapped}`
          : `目录文档校验失败：${error.detail}`;
      }
      return "目录文档上传失败。";
    }
    return "目录操作请求已被节点拒绝，请检查工作目录或 Launch Profile 配置。";
  }
  return fallbackFormat(error);
}

export class WorkspaceCatalogController {
  readonly #port: WorkspaceCatalogHostPort;

  #catalogInventory: CatalogInventory | null = null;
  #catalogWorkspacePathDraft = "";
  #catalogInstallDraft: CatalogInstallDraft = createInitialInstallDraft();
  #catalogTargetDraft: CatalogTargetDraft = createInitialTargetDraft();

  constructor(port: WorkspaceCatalogHostPort) {
    this.#port = port;
  }

  get catalogInventory(): CatalogInventory | null {
    return this.#catalogInventory;
  }

  resetInventory(): void {
    this.#catalogInventory = null;
  }

  workspaceCatalogPanel(record: IdentityRecord): HTMLElement {
    const doc = this.#port.getDocument?.() ?? globalThis.document;
    return workspaceCatalogPanelView(
      doc,
      {
        connected: this.#port.getManagement() !== null,
        inventory: this.#catalogInventory,
        workspacePathDraft: this.#catalogWorkspacePathDraft,
        installDraft: this.#catalogInstallDraft,
        targetDraft: this.#catalogTargetDraft,
      },
      {
        onRefreshInventory: (submit) => {
          void this.refreshCatalogInventory(
            record,
            this.#port.getManagementGeneration(),
            false,
            submit,
          );
        },
        onWorkspacePathChange: (value) => {
          this.#catalogWorkspacePathDraft = value;
        },
        onRegisterWorkspace: (submit) => {
          void this.registerCatalogWorkspace(record, submit);
        },
        onInstallFieldChange: (field, value) => {
          this.#catalogInstallDraft = {
            ...this.#catalogInstallDraft,
            [field]: value,
          };
          if (field === "sourceMode") {
            this.#port.renderDashboard(record);
          }
        },
        onInstallHarness: (submit) => {
          void this.installCatalogHarness(record, submit);
        },
        onTargetFieldChange: (field, value) => {
          this.#catalogTargetDraft = {
            ...this.#catalogTargetDraft,
            [field]: value,
          };
        },
        onRegisterTarget: (submit) => {
          void this.registerCatalogTarget(record, submit);
        },
        onRegisterAndBindTarget: (submit) => {
          void this.registerAndBindCatalogTarget(record, submit);
        },
        onBindTarget: (submit) => {
          void this.bindExistingCatalogTarget(record, submit);
        },
        onUseWorkspace: (workspace) => {
          this.#catalogTargetDraft = {
            ...this.#catalogTargetDraft,
            workspaceId: workspace.workspaceId,
            launchPath: workspace.canonicalPath,
          };
          this.#port.onSyncWorkspaceSelection(
            workspace.workspaceId,
            workspace.canonicalPath,
          );
          this.#port.renderDashboard(
            record,
            "已将工作目录填入 Target 与来源表单。",
          );
        },
        onUseHarnessProfile: (
          harnessId,
          sourceId,
          profileId,
          profileRevision,
        ) => {
          this.#catalogTargetDraft = {
            ...this.#catalogTargetDraft,
            sourceId,
            harnessId,
            profileId,
            profileRevision,
          };
          this.#port.onSyncHarnessProfileSelection(
            sourceId,
            harnessId,
            profileId,
            profileRevision,
          );
          this.#port.renderDashboard(
            record,
            "已将 Harness / Profile 填入 Target 与来源表单。",
          );
        },
        onSelectTargetQueue: (targetId) => {
          this.#port.onSelectTargetQueue(record, targetId);
        },
        onSelectTargetRelease: (targetId) => {
          this.#port.onSelectTargetRelease(record, targetId);
        },
        onSelectTargetForBind: (target) => {
          this.#catalogTargetDraft = {
            ...this.#catalogTargetDraft,
            targetId: target.targetId,
            sourceId: target.sourceId,
            workspaceId: target.workspaceId ?? this.#catalogTargetDraft.workspaceId,
          };
          this.#port.renderDashboard(record, "已填入待绑定 Target 信息。");
        },
        onFillSourceSelection: (selection, nativeSessionId) => {
          this.#port.onFillSourceSelection(record, selection, nativeSessionId);
        },
      },
    );
  }

  #formatCatalogError(error: unknown): string {
    return catalogActionErrorText(
      error,
      this.#port.formatError ?? managementErrorMessage,
    );
  }

  async refreshCatalogInventory(
    record: IdentityRecord,
    generation: number,
    silent = false,
    submit?: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    if (
      connection === null ||
      generation !== this.#port.getManagementGeneration()
    ) {
      if (submit !== undefined) submit.disabled = false;
      return;
    }
    try {
      const inventory = await connection.inspectCatalogInventory();
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#catalogInventory = inventory;
      this.#port.renderDashboard(
        this.#port.getCurrentRecord() ?? record,
        silent
          ? undefined
          : `目录清单已更新（Workspaces ${inventory.workspaces.length} · Targets ${inventory.targets.length}）。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      if (submit !== undefined) submit.disabled = false;
      if (!silent) {
        this.#port.renderDashboard(
          this.#port.getCurrentRecord() ?? record,
          undefined,
          `目录清单读取失败：${this.#formatCatalogError(error)}`,
        );
      }
    }
  }

  async registerCatalogWorkspace(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法登记工作目录。",
      );
      return;
    }
    const pathValue = this.#catalogWorkspacePathDraft.trim();
    if (pathValue.length === 0 || pathValue.includes("\0")) {
      submit.disabled = false;
      this.#port.renderDashboard(record, undefined, "工作目录路径不能为空。");
      return;
    }
    try {
      const workspace = await connection.registerWorkspace(pathValue);
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#catalogWorkspacePathDraft = "";
      this.#catalogTargetDraft = {
        ...this.#catalogTargetDraft,
        workspaceId: workspace.workspaceId,
        launchPath: workspace.canonicalPath,
      };
      this.#port.onSyncWorkspaceSelection(
        workspace.workspaceId,
        workspace.canonicalPath,
      );
      try {
        const refreshed = await connection.inspectCatalogInventory();
        if (generation !== this.#port.getManagementGeneration()) return;
        this.#catalogInventory = refreshed;
      } catch {
        // Keep current inventory if follow-up refresh fails transiently.
      }
      this.#port.renderDashboard(
        record,
        `工作目录已登记：${workspace.workspaceId}（${workspace.canonicalPath}）。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        `工作目录登记失败：${this.#formatCatalogError(error)}`,
      );
    }
  }

  async installCatalogHarness(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法安装 Harness。",
      );
      return;
    }
    try {
      const input = validateInstallHarnessInput(
        this.#catalogInstallDraft,
        this.#catalogInventory,
      );
      const receipt = await connection.executeCatalogDocument(
        buildInstallHarnessDocument(input),
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      const result = receipt.fact.result;
      if (
        result !== null &&
        (result.kind === "harness" || result.kind === "profile")
      ) {
        this.#catalogTargetDraft = {
          ...this.#catalogTargetDraft,
          sourceId: result.sourceId,
          harnessId: result.harnessId,
          profileId: result.profileId,
          profileRevision: result.profileRevision,
        };
        this.#port.onSyncHarnessProfileSelection(
          result.sourceId,
          result.harnessId,
          result.profileId,
          result.profileRevision,
        );
      }
      try {
        const refreshed = await connection.inspectCatalogInventory();
        if (generation !== this.#port.getManagementGeneration()) return;
        this.#catalogInventory = refreshed;
      } catch {
        // Ignore transient refresh failure; receipt already succeeded.
      }
      this.#port.renderDashboard(
        record,
        result !== null &&
          (result.kind === "harness" || result.kind === "profile")
          ? `Harness / Profile 已安装：harness ${result.harnessId}，profile ${result.profileId} rev ${result.profileRevision}。`
          : "Harness / Profile 已安装并提交。",
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        `Harness 安装失败：${this.#formatCatalogError(error)}`,
      );
    }
  }

  async registerCatalogTarget(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法登记 Target。",
      );
      return;
    }
    let sourceId: string;
    let workspaceId: string;
    const nativeSessionId = this.#catalogTargetDraft.nativeSessionId.trim();
    try {
      sourceId = requireSessionId(this.#catalogTargetDraft.sourceId.trim());
      workspaceId = requireSessionId(
        this.#catalogTargetDraft.workspaceId.trim(),
      );
      if (nativeSessionId.length === 0 || nativeSessionId.includes("\0")) {
        throw new CatalogDraftValidationError("Native Session ID 不能为空。");
      }
    } catch (error) {
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        error instanceof CatalogDraftValidationError
          ? error.message
          : "Source ID 或 Workspace ID 格式无效。",
      );
      return;
    }
    try {
      const target = await connection.registerTarget(
        sourceId,
        nativeSessionId,
        workspaceId,
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#catalogTargetDraft = {
        ...this.#catalogTargetDraft,
        targetId: target.targetId,
        sourceId: target.sourceId,
        nativeSessionId: target.nativeSessionId,
        workspaceId: target.workspaceId ?? workspaceId,
      };
      try {
        const refreshed = await connection.inspectCatalogInventory();
        if (generation !== this.#port.getManagementGeneration()) return;
        this.#catalogInventory = refreshed;
      } catch {
        // Ignore transient refresh failure.
      }
      this.#port.renderDashboard(
        record,
        `Target 已登记：${target.targetId}（尚未绑定 Profile）。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        `Target 登记失败：${this.#formatCatalogError(error)}`,
      );
    }
  }

  async registerAndBindCatalogTarget(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法登记并绑定 Target。",
      );
      return;
    }
    const nativeSessionId = this.#catalogTargetDraft.nativeSessionId.trim();
    if (nativeSessionId.length === 0 || nativeSessionId.includes("\0")) {
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        "Native Session ID 不能为空。",
      );
      return;
    }
    const selectedWorkspace = this.#catalogInventory?.workspaces.find(
      (ws) => ws.workspaceId === this.#catalogTargetDraft.workspaceId.trim(),
    );
    let prevalidatedBind: ReturnType<typeof validateBindTargetInput>;
    try {
      prevalidatedBind = validateBindTargetInput(
        this.#catalogTargetDraft,
        "AAAAAAAAAAAAAAAAAAAAAA",
        selectedWorkspace?.canonicalPath,
      );
    } catch (error) {
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        `Target 绑定参数无效：${this.#formatCatalogError(error)}`,
      );
      return;
    }
    try {
      const target = await connection.registerTarget(
        prevalidatedBind.sourceId,
        nativeSessionId,
        prevalidatedBind.workspaceId,
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#catalogTargetDraft = {
        ...this.#catalogTargetDraft,
        targetId: target.targetId,
        launchPath: prevalidatedBind.launchPath,
      };
      await connection.executeCatalogDocument(
        buildBindTargetInitialDocument({
          ...prevalidatedBind,
          targetId: target.targetId,
        }),
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      try {
        const refreshed = await connection.inspectCatalogInventory();
        if (generation !== this.#port.getManagementGeneration()) return;
        this.#catalogInventory = refreshed;
      } catch {
        // Ignore transient refresh failure.
      }
      this.#port.renderDashboard(
        record,
        `Target 已登记并绑定：${target.targetId}（profile ${prevalidatedBind.profileId} rev ${prevalidatedBind.profileRevision}）。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        `Target 登记或绑定失败：${this.#formatCatalogError(error)}`,
      );
    }
  }

  async bindExistingCatalogTarget(
    record: IdentityRecord,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法绑定 Target。",
      );
      return;
    }
    const selectedWorkspace = this.#catalogInventory?.workspaces.find(
      (ws) => ws.workspaceId === this.#catalogTargetDraft.workspaceId.trim(),
    );
    try {
      const input = validateBindTargetInput(
        this.#catalogTargetDraft,
        undefined,
        selectedWorkspace?.canonicalPath,
      );
      await connection.executeCatalogDocument(
        buildBindTargetInitialDocument(input),
      );
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#catalogTargetDraft = {
        ...this.#catalogTargetDraft,
        launchPath: input.launchPath,
      };
      try {
        const refreshed = await connection.inspectCatalogInventory();
        if (generation !== this.#port.getManagementGeneration()) return;
        this.#catalogInventory = refreshed;
      } catch {
        // Ignore transient refresh failure.
      }
      this.#port.renderDashboard(
        record,
        `Target 已绑定：${input.targetId}（profile ${input.profileId} rev ${input.profileRevision}）。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        `Target 绑定失败：${this.#formatCatalogError(error)}`,
      );
    }
  }
}
