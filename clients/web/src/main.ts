import "./styles.css";
import { managementRequestErrorMessage } from "./management/error-messages";
import { sessionCapacityPanel } from "./management/session-capacity";

import {
  CatalogActionError,
  ManagementConnection,
  ManagementConnectionError,
  ManagementRequestError,
  SessionConsumeOutcomeError,
  buildBindTargetInitialDocument,
  buildInstallHarnessDocument,
} from "./management/client";
import {
  type DevicePage,
  type DeviceSummary,
  type ManagementErrorCode,
  type NodeInfo,
  ManagementProtocolError,
} from "./management/protocol";
import { ManagementReconnect } from "./management/reconnect";
import {
  requireSessionId,
  type ApprovalRegistration,
  type CatalogInventory,
  type ApprovalStatus,
  type ApprovalView,
  type NativeReleaseRegistered,
  type NativeReleaseStatus,
  type SessionCapacity,
  type SessionQueueSnapshot,
  type SessionQueueEntry,
  type SessionViewOpened,
  type SourceLaunchSelectionInput,
  type SourceOperationInput,
  type SourceSessionFact,
  type SourceSessionRegistration,
} from "./management/session-protocol";
import {
  SourceSessionResultReadError,
  SourceSessionResultReader,
  inspectSourceSessionResultPayload,
  type InspectedSourceSessionResult,
} from "./management/source-result-read";
import {
  clearPersistedSourceViewSpec,
  loadPersistedSourceViewSpec,
  persistSourceViewSpec,
  sourceSessionPanelView,
  validateSourceOperationInput,
  validateSourceSelectionInput,
  validateSourceViewTargetInput,
} from "./management/source-session-view";
import {
  CatalogDraftValidationError,
  createInitialInstallDraft,
  createInitialTargetDraft,
  validateBindTargetInput,
  validateInstallHarnessInput,
  workspaceCatalogPanelView,
  type CatalogInstallDraft,
  type CatalogTargetDraft,
} from "./management/workspace-catalog-view";
import type { RawInspector } from "./views/raw-inspector";
import { sessionQueueSnapshotView } from "./management/session-queue-view";
import { SessionResumeCoordinator } from "./management/session-resume";
import {
  SessionInputActionError,
  cancelSessionInput,
  editSessionText,
  prioritizeSessionInput,
  submitSessionText,
} from "./management/session-input-actions";
import {
  QueuedBodyReadError,
  QueuedBodyReader,
} from "./management/session-body-read";
import type { ViewDataFrame } from "./views/binary";
import { SessionViewDisplay } from "./management/session-view-display";
import { PairingApi, PairingApiError } from "./pairing/api";
import {
  IdentityStoreError,
  IdentityVault,
  IndexedDbIdentityRepository,
  type IdentityRecord,
} from "./pairing/identity";
import {
  PairingSession,
  PairingSessionError,
  isFailedPairingStatus,
  shouldRecoverPendingManagement,
} from "./pairing/session";
import {
  PairingProtocolError,
  type PairStatusResult,
  randomId128,
} from "./protocol/pairing";

const POLL_INTERVAL_MS = 3_000;
const app = document.querySelector<HTMLDivElement>("#app");

if (app === null) {
  throw new Error("application root is missing");
}

app.innerHTML = `
  <main class="shell">
    <button id="theme-toggle" class="theme-toggle" type="button" aria-label="切换为深色主题">深色</button>
    <section
      id="access-panel"
      class="panel"
      aria-labelledby="panel-title"
      aria-live="polite"
    ></section>
    <div id="session-view-cache"></div>
  </main>
`;

const panel = requiredElement("access-panel", HTMLElement);
const sharedViewHost = requiredElement("session-view-cache", HTMLElement);
const themeToggle = requiredElement("theme-toggle", HTMLButtonElement);
const systemTheme = globalThis.matchMedia("(prefers-color-scheme: dark)");
let themeOverride: boolean | null = null;
function updateTheme(): void {
  const dark = themeOverride ?? systemTheme.matches;
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  themeToggle.textContent = dark ? "浅色" : "深色";
  themeToggle.setAttribute("aria-label", dark ? "切换为浅色主题" : "切换为深色主题");
}
themeToggle.addEventListener("click", () => {
  themeOverride = document.documentElement.dataset.theme !== "dark";
  updateTheme();
});
systemTheme.addEventListener("change", updateTheme);
updateTheme();
let vault: IdentityVault | null = null;
let session: PairingSession | null = null;
let controller: AbortController | null = null;
let management: ManagementConnection | null = null;
let managementGeneration = 0;
let currentNodeInfo: NodeInfo | null = null;
let currentDevices: readonly DeviceSummary[] = [];
let currentRevision: number | null = null;
let nextDeviceCursor: string | null = null;
let currentRecord: IdentityRecord | null = null;
let devicesRefreshInFlight = false;
let refreshAfterCurrentRequest = false;
let pendingRecoveryAttempted = false;
let pageActive = true;
let manuallyDisconnected = false;
let automaticReconnectAllowed = true;
let lastManagementError: unknown = null;
let identityController = new AbortController();
let identityWork: Promise<unknown> = Promise.resolve();
let identityResetting = false;
let inspectedQueueTargetId: string | null = null;
let inspectedQueue: SessionQueueSnapshot | null = null;
let editingQueueEntry: SessionQueueEntry | null = null;
let openedSessionView: SessionViewOpened | null = null;
let sharedSessionDisplay: SessionViewDisplay | null = null;
let sharedSessionConnection: ManagementConnection | null = null;
let sharedViewTargetId: string | null = null;
let sharedViewAttempt = 0;
const staleSharedDisplays = new Set<SessionViewDisplay>();
let bodyPreview: string | null = null;
let approvalList: readonly ApprovalView[] | null = null;
let selectedApproval: ApprovalView | null = null;
let approvalRegistration: ApprovalRegistration | null = null;
let approvalStatus: ApprovalStatus | null = null;
let nativeReleaseTargetId: string | null = null;
let nativeReleaseRegistration: NativeReleaseRegistered | null = null;
let nativeReleaseStatus: NativeReleaseStatus | null = null;
let sessionCapacity: SessionCapacity | null = null;
let activeBodyRead: {
  readonly streamId: string;
  readonly reader: QueuedBodyReader;
} | null = null;
let bodyReadTimer: number | null = null;
let bodyReadAttempt = 0;
const sessionResume = new SessionResumeCoordinator();
type SharedViewSpec =
  | { readonly kind: "target"; readonly targetId: string }
  | {
      readonly kind: "source";
      readonly selection: SourceLaunchSelectionInput;
      readonly nativeSessionId: string;
    };
let sharedViewSpec: SharedViewSpec | null = null;
let sharedViewStaleNotice: string | null = null;
let sourceSelectionDraft: SourceLaunchSelectionInput = {
  sourceId: "",
  harnessId: "",
  profileId: "",
  profileRevision: "",
  workspaceId: "",
  launchPath: "",
};
let sourceDiscoverCursorDraft = "";
let sourceManualNativeIdDraft = "";
let sourceRegistration: SourceSessionRegistration | null = null;
let sourceFact: SourceSessionFact | null = null;
let activeSourceResultRead: {
  readonly streamId: string;
  readonly reader: SourceSessionResultReader;
  readonly fact: SourceSessionFact | null;
} | null = null;
let sourceResultTimer: number | null = null;
let sourceResultAttempt = 0;
let inspectedSourceResult: InspectedSourceSessionResult | null = null;
let sourceRawInspectorExpanded = false;
const sourceRawInspectors = new Set<RawInspector>();
let catalogInventory: CatalogInventory | null = null;
let catalogWorkspacePathDraft = "";
let catalogInstallDraft: CatalogInstallDraft = createInitialInstallDraft();
let catalogTargetDraft: CatalogTargetDraft = createInitialTargetDraft();
function runIdentityOperation<T>(operation: () => Promise<T>): Promise<T> {
  const result = identityWork.then(operation);
  identityWork = result.catch(() => undefined);
  return result;
}
const managementReconnect = new ManagementReconnect(connectSavedManagement);

window.addEventListener("pagehide", () => {
  pageActive = false;
  disposeSharedViews();
  resetSessionInputUi();
  identityController.abort();
  managementReconnect.cancel();
  controller?.abort();
  const connection = management;
  management = null;
  ++managementGeneration;
  connection?.disconnect();
});
window.addEventListener("pageshow", (event) => {
  pageActive = true;
  if (event.persisted && !manuallyDisconnected) {
    if (session === null || identityController.signal.aborted) void boot(true);
    else resumeManagement();
  }
});
window.addEventListener("online", resumeManagement);

function resumeManagement(): void {
  if (pageActive && !manuallyDisconnected && automaticReconnectAllowed) {
    managementReconnect.resume();
  }
}

function connectSavedManagement(): void {
  const record = currentRecord;
  if (
    pageActive &&
    !manuallyDisconnected &&
    automaticReconnectAllowed &&
    management === null &&
    record !== null &&
    (record.status === "approved" || record.status === "consumed")
  ) {
    void openManagement(record);
  }
}

void boot();

async function boot(restoringPage = false): Promise<void> {
  if (!pageActive) return;
  identityController.abort();
  identityController = new AbortController();
  const signal = identityController.signal;
  if (globalThis.location.protocol !== "https:") {
    showFatal(
      "需要通过节点的 HTTPS 地址打开",
      "当前页面不是安全来源。请从节点终端显示的 HTTPS 地址进入，本页不会在明文来源创建身份。",
    );
    return;
  }

  showLoading();
  try {
    vault ??= new IdentityVault(new IndexedDbIdentityRepository());
    const localVault = vault;
    const record = await runIdentityOperation(() => localVault.initialize(globalThis.location.origin, signal));
    if (signal.aborted || !pageActive) return;
    session = new PairingSession(vault, new PairingApi(), record);
  } catch (error) {
    if (signal.aborted || !pageActive) return;
    showFatal("无法读取浏览器身份", errorMessage(error));
    return;
  }

  const record = session.identity;
  if (record.status === "pending_claim") {
    showPairForm(
      record,
      session.canRetryClaim
        ? "已从本机 IndexedDB 恢复上次未完成的一次性认领证明。可逐字重试，不会重新生成公钥或挑战。"
        : undefined,
    );
    return;
  }
  if (record.status === "pending_confirmation") {
    showPending(record);
    void pollApproval();
    return;
  }
  if (record.status === "approved" || record.status === "consumed") {
    if (restoringPage) {
      currentRecord = record;
      if (!automaticReconnectAllowed) {
        showFatal("管理连接已停止", managementErrorMessage(lastManagementError));
        return;
      }
      showManagementConnecting(record);
      resumeManagement();
      return;
    }
    void openManagement(record);
    return;
  }
  showPairForm(record);
}

function showLoading(): void {
  panel.replaceChildren(
    eyebrow("LOCAL IDENTITY"),
    title("正在检查浏览器身份"),
    paragraph("密钥只保存在本站同源 IndexedDB 中，并会在认领前完成读回签验。"),
    statusLine("正在初始化不可导出 P-256 密钥…", "working"),
  );
}

function showFatal(heading: string, detail: string): void {
  panel.replaceChildren(
    eyebrow("STOPPED"),
    title(heading),
    paragraph(detail),
    statusLine("为保护已保存身份，本页不会自动生成替代密钥。", "error"),
  );
}

function showPairForm(
  record: IdentityRecord,
  notice?: string,
  failure?: string,
): void {
  const fragment = document.createDocumentFragment();
  const origin = element("div", "origin-pill");
  origin.append(element("span", "origin-dot"), element("span", "origin-host", globalThis.location.host));
  fragment.append(origin, title("配对到此节点"));
  fragment.append(
    paragraph(
      "在节点终端运行 acp-remote device pair，输入配对码后，在终端核对并批准。",
    ),
  );
  if (notice !== undefined) {
    fragment.append(statusLine(notice, "notice"));
  }
  if (failure !== undefined) {
    fragment.append(statusLine(failure, "error"));
  }

  if (session?.canRetryClaim === true) {
    const retry = button("重新提交同一认领");
    retry.addEventListener("click", () => {
      void retryPendingClaim(retry);
    });
    fragment.append(retry);
  }

  const form = element("form", "pair-form");
  const codeLabel = element("label", undefined, "配对码");
  codeLabel.htmlFor = "pairing-code";
  const codeInput = element("input") as HTMLInputElement;
  codeInput.id = "pairing-code";
  codeInput.name = "pairing-code";
  codeInput.type = "text";
  codeInput.maxLength = 9;
  codeInput.required = true;
  codeInput.autocomplete = "off";
  codeInput.spellcheck = false;
  codeInput.inputMode = "text";
  codeInput.placeholder = "XXXX-XXXX";
  codeInput.className = "code-input";
  codeInput.setAttribute(
    "pattern",
    "[0-9A-HJKMNP-TV-Za-hjkmnp-tv-z]{4}-?[0-9A-HJKMNP-TV-Za-hjkmnp-tv-z]{4}",
  );

  const nameLabel = element("label", undefined, "这台浏览器如何显示");
  nameLabel.htmlFor = "display-name";
  const nameInput = element("input") as HTMLInputElement;
  nameInput.id = "display-name";
  nameInput.name = "display-name";
  nameInput.type = "text";
  nameInput.maxLength = 80;
  nameInput.required = true;
  nameInput.autocomplete = "off";
  nameInput.value = record.displayName ?? "";
  nameInput.placeholder = "例如：客厅浏览器";

  const help = paragraph(
    "输入节点终端显示的 8 位配对码。若清除了浏览器数据或丢失身份，请先在节点终端撤销旧设备，再重新配对。",
    "field-help",
  );
  const submit = button("认领这台浏览器", "submit");
  submit.type = "submit";
  form.append(codeLabel, codeInput, nameLabel, nameInput, help, submit);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const code = codeInput.value;
    const displayName = nameInput.value;
    codeInput.value = "";
    void submitPair(code, displayName);
  });
  fragment.append(form);
  panel.replaceChildren(fragment);
  codeInput.focus();
}

async function submitPair(code: string, displayName: string): Promise<void> {
  if (session === null) {
    return;
  }
  controller?.abort();
  controller = new AbortController();
  showWorking();
  try {
    await session.begin(code, displayName, controller.signal);
  } catch (error) {
    if (controller.signal.aborted) {
      return;
    }
    if (recoverChangedIdentity(error)) return;
    if (session.identity.status === "pending_claim") {
      showPairForm(
        session.identity,
        "节点可能已经接收认领，但响应没有完整返回。可逐字重试同一份证明；不会重新认领。",
        errorMessage(error),
      );
    } else {
      showPairForm(session.identity, undefined, errorMessage(error));
    }
    return;
  }
  pendingRecoveryAttempted = false;
  showPending(session.identity);
  void pollApproval();
}

async function retryPendingClaim(retryButton: HTMLButtonElement): Promise<void> {
  if (session === null) {
    return;
  }
  controller?.abort();
  controller = new AbortController();
  retryButton.disabled = true;
  showWorking();
  try {
    await session.retryClaim(controller.signal);
  } catch (error) {
    if (controller.signal.aborted) {
      return;
    }
    if (recoverChangedIdentity(error)) return;
    showPairForm(
      session.identity,
      "节点可能已经接收认领，但响应没有完整返回。重试不会更换公钥或挑战。",
      errorMessage(error),
    );
    return;
  }
  pendingRecoveryAttempted = false;
  showPending(session.identity);
  void pollApproval();
}

async function pollApproval(): Promise<void> {
  if (session === null) {
    return;
  }
  controller?.abort();
  controller = new AbortController();
  const active = controller;
  for (;;) {
    try {
      const result = await session.pollOnce(active.signal);
      if (active.signal.aborted) {
        return;
      }
      if (!handleStatusResult(result)) {
        return;
      }
    } catch (error) {
      if (active.signal.aborted) {
        return;
      }
      if (recoverChangedIdentity(error)) return;
      if (error instanceof PairingApiError && error.code === "rate_limited") {
        const waitSeconds = Math.max(error.retryAfterSeconds ?? 3, 1);
        showPending(
          session.identity,
          `查询过于频繁，将在 ${waitSeconds} 秒后继续检查。`,
        );
        await delay(waitSeconds * 1_000, active.signal);
        continue;
      }
      if (
        shouldRecoverPendingManagement(
          error,
          session.identity,
          pendingRecoveryAttempted,
        )
      ) {
        pendingRecoveryAttempted = true;
        await recoverPendingManagement(session.identity);
        return;
      }
      showPollingFailure(errorMessage(error));
      return;
    }
    await delay(POLL_INTERVAL_MS, active.signal);
  }
}

function handleStatusResult(result: PairStatusResult): boolean {
  if (result.status === "pending_confirmation") {
    showPending(session?.identity ?? null, "仍在等待本机终端明确批准。");
    return true;
  }
  const record = session?.identity ?? null;
  if (
    record !== null &&
    (record.status === "approved" || record.status === "consumed")
  ) {
    void openManagement(record);
  } else if (record !== null) {
    showTerminal(record);
  }
  return false;
}

function showWorking(): void {
  panel.replaceChildren(
    eyebrow("STEP 02 / VERIFY"),
    title("正在认领并验证节点"),
    paragraph(
      "浏览器会先验证节点签名、来源和公钥指纹。验证失败时不会显示待批准状态。",
    ),
    statusLine("正在提交一次性的配对证明…", "working"),
  );
}

function showPending(record: IdentityRecord | null, detail?: string): void {
  if (record === null) {
    showFatal("配对状态不可用", "没有找到当前浏览器身份。");
    return;
  }
  const fragment = document.createDocumentFragment();
  fragment.append(
    eyebrow("STEP 03 / CONFIRM"),
    title("已认领，等待本机批准"),
    paragraph(
      detail ??
        "请在运行节点的终端核对节点指纹和客户端信息，再明确批准。直接回车会拒绝。",
    ),
  );
  if (record.nodeId !== null) {
    fragment.append(detailBlock("节点 SHA-256 指纹", formatHex(record.nodeId)));
  }
  if (record.displayName !== null) {
    fragment.append(detailBlock("客户端名称", record.displayName));
  }
  const clientFingerprint = detailBlock("客户端公钥指纹", "正在计算…");
  fragment.append(clientFingerprint);
  fragment.append(statusLine("等待批准，不会跳过本机确认。", "working"));
  panel.replaceChildren(fragment);
  void fillFingerprint(clientFingerprint, record.publicKey);
}

function showPollingFailure(detail: string): void {
  const retry = button("重新检查批准状态");
  retry.addEventListener("click", () => void pollApproval());
  const actions = element("div", "action-row");
  actions.append(retry);
  const record = session?.identity ?? null;
  if (
    record !== null &&
    record.status === "pending_confirmation" &&
    record.nodeId !== null &&
    record.nodePublicKey !== null
  ) {
    const recover = button("验证已保存身份", "secondary-button");
    recover.addEventListener("click", () => {
      pendingRecoveryAttempted = true;
      void openManagement(record, true);
    });
    actions.append(recover);
  }
  panel.replaceChildren(
    eyebrow("PAUSED"),
    title("暂时无法检查批准结果"),
    paragraph(detail),
    actions,
  );
}

async function recoverPendingManagement(record: IdentityRecord): Promise<void> {
  if (
    record.status !== "pending_confirmation" ||
    record.nodeId === null ||
    record.nodePublicKey === null
  ) {
    showPollingFailure("本地身份缺少节点绑定资料，无法执行公钥恢复。");
    return;
  }
  await openManagement(record, true);
}

async function openManagement(
  record: IdentityRecord,
  recoveringPending = false,
): Promise<void> {
  if (!pageActive || manuallyDisconnected || identityResetting) return;
  if (
    vault === null ||
    session === null ||
    record.nodeId === null ||
    record.nodePublicKey === null
  ) {
    showFatal("身份记录不完整", "缺少节点绑定资料，不会发起管理认证。");
    return;
  }

  controller?.abort();
  controller = null;
  managementReconnect.cancel();
  const generation = ++managementGeneration;
  management?.disconnect();
  management = null;
  currentNodeInfo = null;
  currentDevices = [];
  currentRevision = null;
  catalogInventory = null;
  sessionCapacity = null;
  nextDeviceCursor = null;
  inspectedQueue = null;
  suspendSharedView();
  resetSessionInputUi();
  currentRecord = record;
  devicesRefreshInFlight = false;
  refreshAfterCurrentRequest = false;
  showManagementConnecting(record);

  let authenticated = false;
  const connection = new ManagementConnection(
    {
      origin: record.origin,
      nodeId: record.nodeId,
      nodePublicKey: record.nodePublicKey,
      clientId: record.clientId,
      clientPublicKey: record.publicKey,
    },
    record.privateKey,
    {
      origin: globalThis.location.origin,
      callbacks: {
        onDevicesChanged: () => {
          if (authenticated && generation === managementGeneration) {
            void refreshDevices(true, generation);
          }
        },
        onViewData: (frame) => {
          if (generation === managementGeneration) {
            handleSessionViewData(record, generation, frame);
          }
        },
        onClose: (close) => {
          if (
            authenticated &&
            generation === managementGeneration &&
            management === connection
          ) {
            management = null;
            handleManagementClose(currentRecord ?? record, close.code, close.error);
          }
        },
      },
    },
  );
  management = connection;

  try {
    const authentication = await connection.connect();
    if (generation !== managementGeneration) {
      connection.disconnect();
      return;
    }
    const currentSession = session;
    if (currentSession === null) {
      throw new PairingSessionError("client_mismatch");
    }
    const updated = await currentSession.acceptAuthenticated(
      authentication.clientId,
    );
    if (generation !== managementGeneration) {
      connection.disconnect();
      return;
    }
    currentRecord = updated;
    authenticated = true;
    const info = await connection.nodeInfo();
    if (generation !== managementGeneration) {
      return;
    }
    currentNodeInfo = info;
    showManagementDashboard(
      updated,
      "节点签名、来源和客户端身份均已验证。",
    );
    await connection.subscribeDevices();
    if (generation === managementGeneration && management === connection) {
      managementReconnect.reset();
      automaticReconnectAllowed = true;
    }
    await refreshDevices(true, generation);
    if (generation === managementGeneration && management === connection) {
      await refreshCatalogInventory(updated, generation, true);
    }
    const queueTargetId = inspectedQueueTargetId;
    if (
      queueTargetId !== null &&
      generation === managementGeneration &&
      management === connection
    ) {
      await loadSessionQueue(updated, queueTargetId, generation);
    }
    if (sharedViewSpec === null && sharedViewTargetId === null && inspectedQueueTargetId === null) {
      const persisted = loadPersistedSourceViewSpec();
      if (persisted !== null) {
        sharedViewSpec = persisted;
        syncSourceDraftFromSpec(persisted);
      }
    }
    if (generation === managementGeneration && management === connection) {
      const activeSpec = sharedViewSpec;
      if (activeSpec?.kind === "source") {
        await openSharedSourceView(updated, activeSpec.selection, activeSpec.nativeSessionId, null, "reconnect");
      } else {
        const viewTarget = activeSpec?.kind === "target" ? activeSpec.targetId : sharedViewTargetId;
        if (viewTarget !== null) {
          await openSharedSessionView(updated, viewTarget, null, "reconnect");
        }
      }
    }
  } catch (error) {
    if (generation !== managementGeneration) {
      return;
    }
    connection.disconnect();
    management = null;
    if (recoverChangedIdentity(error)) return;
    if (recoveringPending) {
      if (error instanceof ManagementConnectionError &&
          (error.closeCode === 4401 || error.closeCode === 4410)) {
        showManagementFailure(record, error);
        return;
      }
      if (
        error instanceof ManagementConnectionError &&
        error.code === "revoked" && error.nodeVerified
      ) {
        showRevokedAccess(record);
        return;
      }
      showPending(
        record,
        "节点暂未接受已保存身份，可能仍在等待本机批准或连接尚未恢复。可继续检查批准状态。",
      );
      void pollApproval();
      return;
    }
    showManagementFailure(currentRecord ?? record, error);
  }
}

function showManagementConnecting(record: IdentityRecord): void {
  const fragment = identityFragment(record);
  fragment.append(
    eyebrow("MANAGEMENT / AUTHENTICATE"),
    title("正在认证管理连接"),
    paragraph(
      "节点会先证明自己的身份，浏览器再使用不可导出的私钥签署一次性挑战。",
    ),
  );
  appendIdentityDetails(fragment, record);
  fragment.append(
    statusLine("正在通过同源 WSS 验证节点挑战…", "working"),
  );
  panel.replaceChildren(fragment);
}

async function refreshDevices(
  reset: boolean,
  generation: number,
): Promise<void> {
  const connection = management;
  const record = currentRecord;
  if (
    connection === null ||
    record === null ||
    generation !== managementGeneration
  ) {
    return;
  }
  if (devicesRefreshInFlight) {
    refreshAfterCurrentRequest ||= reset;
    return;
  }
  devicesRefreshInFlight = true;
  try {
    const cursor = reset ? null : nextDeviceCursor;
    const page = await connection.listDevices(cursor);
    if (generation !== managementGeneration) {
      return;
    }
    applyDevicePage(page, cursor === null);
    showManagementDashboard(
      currentRecord ?? record,
      reset ? "设备列表已更新。" : "已载入下一页客户端。",
    );
  } catch (error) {
    if (generation !== managementGeneration) {
      return;
    }
    if (
      error instanceof ManagementRequestError &&
      error.code === "stale_cursor" &&
      !reset
    ) {
      refreshAfterCurrentRequest = true;
      return;
    }
    showManagementDashboard(
      currentRecord ?? record,
      undefined,
      managementErrorMessage(error),
    );
  } finally {
    devicesRefreshInFlight = false;
    if (
      refreshAfterCurrentRequest &&
      generation === managementGeneration
    ) {
      refreshAfterCurrentRequest = false;
      void refreshDevices(true, generation);
    }
  }
}

function applyDevicePage(page: DevicePage, replace: boolean): void {
  if (replace) {
    currentDevices = page.items;
  } else {
    const known = new Set(currentDevices.map((item) => item.clientId));
    currentDevices = [
      ...currentDevices,
      ...page.items.filter((item) => !known.has(item.clientId)),
    ];
  }
  currentRevision = page.revision;
  nextDeviceCursor = page.nextCursor;
}

function showManagementDashboard(
  record: IdentityRecord,
  notice?: string,
  failure?: string,
): void {
  const focused = document.activeElement;
  const control = focused instanceof HTMLInputElement || focused instanceof HTMLTextAreaElement
    ? focused : null;
  const saved = control !== null && panel.contains(control) && control.id !== ""
    ? { id: control.id, value: control.value, start: control.selectionStart,
        end: control.selectionEnd, direction: control.selectionDirection }
    : null;
  disposeSourceRawInspectors();
  const fragment = document.createDocumentFragment();
  fragment.append(
    eyebrow("NODE CONTROL / AUTHENTICATED"),
    title("节点管理"),
    paragraph(
      "当前身份已通过公钥持有证明。这里只显示完成后续 Harness 权限审批所需的节点与客户端身份资料。",
    ),
  );
  if (notice !== undefined) {
    fragment.append(statusLine(notice, "notice"));
  }
  if (failure !== undefined) {
    fragment.append(statusLine(failure, "error"));
  }
  appendIdentityDetails(fragment, record, false);
  if (currentNodeInfo !== null) {
    fragment.append(
      detailBlock("节点来源", currentNodeInfo.origin),
      detailBlock("节点状态", currentNodeInfo.status),
    );
  }
  fragment.append(sessionQueuePanel(record));
  fragment.append(sessionApprovalPanel(record));
  fragment.append(sessionCapacityPanel(document, sessionCapacity, management !== null,
    (submit) => { void refreshSessionCapacity(record, submit); }));
  fragment.append(nativeReleasePanel(record));
  fragment.append(workspaceCatalogPanel(record));
  fragment.append(sourceSessionPanel(record));
  fragment.append(
    element("h3", "section-title", "已登记客户端"),
    paragraph(
      currentRevision === null
        ? "正在读取客户端列表…"
        : `列表 revision ${currentRevision}，单页最多显示 32 个客户端。`,
      "field-help",
    ),
  );
  if (currentDevices.length === 0) {
    fragment.append(paragraph("当前没有已登记客户端。", "empty-state"));
  } else {
    for (const device of currentDevices) {
      fragment.append(
        deviceCard(device, record.clientId, () =>
          showRevokeConfirmation(record, device),
        ),
      );
    }
  }

  const actions = element("div", "action-row");
  const refresh = button("刷新列表", "secondary-button");
  refresh.addEventListener("click", () => {
    void refreshDevices(true, managementGeneration);
  });
  actions.append(refresh);
  if (nextDeviceCursor !== null) {
    const more = button("载入下一页", "secondary-button");
    more.addEventListener("click", () => {
      void refreshDevices(false, managementGeneration);
    });
    actions.append(more);
  }
  const disconnect = button("断开管理连接", "secondary-button");
  disconnect.addEventListener("click", () => {
    manuallyDisconnected = true;
    managementReconnect.cancel();
    management?.disconnect();
  });
  actions.append(disconnect);
  fragment.append(actions);
  panel.replaceChildren(fragment);
  if (saved !== null) {
    const replacement = document.getElementById(saved.id);
    if (replacement instanceof HTMLInputElement || replacement instanceof HTMLTextAreaElement) {
      replacement.value = saved.value;
      replacement.focus({ preventScroll: true });
      if (saved.start !== null && saved.end !== null) {
        replacement.setSelectionRange(saved.start, saved.end, saved.direction ?? undefined);
      }
    }
  }
}

function sessionQueuePanel(record: IdentityRecord): HTMLElement {
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
  input.value = inspectedQueueTargetId ?? "";
  input.placeholder = "22 位 canonical base64url ID";
  const submit = button("读取会话队列", "submit");
  submit.type = "submit";
  submit.disabled = management === null;
  form.append(label, input, submit);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    let targetId: string;
    try {
      targetId = requireSessionId(input.value.trim());
    } catch {
      showManagementDashboard(record, undefined, "会话 target ID 格式无效。");
      return;
    }
    inspectedQueueTargetId = targetId;
    inspectedQueue = null;
    sessionResume.clear();
    disposeSharedViews();
    clearSharedSourceViewSpec();
    resetSessionInputUi();
    submit.disabled = true;
    void loadSessionQueue(record, targetId, managementGeneration);
  });
  section.append(form);
  if (inspectedQueueTargetId !== null) {
    section.append(sessionInputComposer(record, inspectedQueueTargetId));
    if (editingQueueEntry !== null) {
      section.append(
        sessionEditComposer(record, inspectedQueueTargetId, editingQueueEntry),
      );
    }
  }
  const openedView = openedSessionView;
  if (openedView !== null && sharedViewSpec?.kind !== "source") {
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
      void closeTargetSharedView(record, openedView, close);
    });
    opened.append(close);
    section.append(opened);
  }
  if (bodyPreview !== null) {
    const preview = element("pre", "body-preview", bodyPreview);
    preview.setAttribute("aria-label", "队列输入正文");
    section.append(preview);
  }

  const snapshot = inspectedQueue;
  if (inspectedQueueTargetId !== null && snapshot === null) {
    section.append(statusLine("正在读取会话队列…", "working"));
    return section;
  }
  if (snapshot === null) {
    return section;
  }
  if (snapshot.targetId !== inspectedQueueTargetId) {
    section.append(statusLine("队列响应与请求的会话不一致。", "error"));
    return section;
  }
  section.append(
    sessionQueueSnapshotView(snapshot, {
      onResume: (resume) => {
        void resumeSessionQueue(record, snapshot.targetId, resume);
      },
      onEdit: (entry) => {
        editingQueueEntry = entry;
        showManagementDashboard(record, "已选中队列项，请在编辑表单中提交替换正文。");
      },
      onCancel: (entry, actionButton) => {
        void runSessionQueueControl(
          record,
          snapshot.targetId,
          entry,
          "cancel",
          actionButton,
        );
      },
      onPrioritize: (entry, actionButton) => {
        void runSessionQueueControl(
          record,
          snapshot.targetId,
          entry,
          "prioritize",
          actionButton,
        );
      },
      onOpenBody: (entry, actionButton) => {
        void openQueueBody(
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
    void loadSessionQueue(record, snapshot.targetId, managementGeneration);
  });
  section.append(refresh);
  const openView = button("打开共享视图", "secondary-button");
  openView.addEventListener("click", () => {
    openView.disabled = true;
    void openTargetSharedView(record, snapshot.targetId, openView);
  });
  section.append(openView);
  return section;
}

function sessionApprovalPanel(record: IdentityRecord): HTMLElement {
  const section = element("section", "session-composer");
  section.append(
    element("h3", "section-title", "原生审批"),
    paragraph(
      "审批由节点从当前 Harness 连接读取。原始载荷只作为不可执行文本显示；登记和裁决都使用节点返回的规范 ID。",
      "field-help",
    ),
  );

  const refresh = button("刷新审批列表", "secondary-button");
  refresh.addEventListener("click", () => {
    refresh.disabled = true;
    void loadApprovalList(record, managementGeneration);
  });
  section.append(refresh);

  const approvals = approvalList;
  if (approvals === null) {
    section.append(paragraph("尚未读取审批列表。", "empty-state"));
    return section;
  }
  if (approvals.length === 0) {
    section.append(paragraph("当前没有待显示的审批。", "empty-state"));
    return section;
  }

  for (const approval of approvals) {
    const card = element("article", "device-card");
    card.append(
      detailBlock("审批", approval.approvalId),
      detailBlock("有效性", approval.status.validity),
      detailBlock(
        "来源协议",
        `ACP v${approval.source.protocolVersion}`,
      ),
      detailBlock(
        "处理状态",
        `${approval.status.delivery} / ${approval.status.sourceClose}`,
      ),
    );
    const select = button(
      selectedApproval?.approvalId === approval.approvalId
        ? "重新读取此审批"
        : "读取此审批",
      "secondary-button",
    );
    select.addEventListener("click", () => {
      select.disabled = true;
      void loadApproval(record, approval.approvalId, select);
    });
    card.append(select);
    section.append(card);
  }

  const approval = selectedApproval;
  if (approval === null) {
    return section;
  }

  const detail = element("article", "session-composer");
  detail.append(
    element("h4", undefined, "审批详情"),
    detailBlock("审批", approval.approvalId),
    detailBlock("来源实例", approval.source.instanceId),
    detailBlock("连接代际", approval.source.connectionEpoch),
    detailBlock(
      "关联",
      approval.context.kind === "verified_input"
        ? `已验证输入 ${approval.context.dispatchId}`
        : "会话请求",
    ),
    detailBlock(
      "当前裁决",
      approvalStatusText(approvalStatus ?? approval.status),
    ),
  );

  const limitations = element("div", "detail-block");
  limitations.append(element("span", "detail-label", "显示限制"));
  if (approval.limitations.length === 0) {
    limitations.append(element("code", undefined, "无"));
  } else {
    for (const limitation of approval.limitations) {
      limitations.append(element("code", undefined, limitation));
    }
  }
  detail.append(limitations);

  if (approval.content.availability === "available") {
    const raw = element(
      "pre",
      "body-preview approval-content",
      approval.content.rawJson,
    );
    raw.setAttribute("aria-label", "审批原始载荷");
    detail.append(raw);
  } else {
    detail.append(statusLine("原始审批载荷当前不可用。", "error"));
  }

  detail.append(element("h4", undefined, "可选决定"));
  const choices = element("div", "action-row");
  for (const choice of approval.choices) {
    const select = button(
      `${choice.ordinal}. ${choice.originalKind} (${choice.originalOptionId})`,
      choice.selectable ? "secondary-button" : undefined,
    );
    select.disabled = !choice.selectable;
    select.addEventListener("click", () => {
      select.disabled = true;
      void registerApprovalChoice(
        record,
        approval.approvalId,
        choice.choiceId,
        select,
      );
    });
    choices.append(select);
  }
  detail.append(choices);

  if (approvalRegistration !== null) {
    detail.append(
      detailBlock(
        "已登记决定",
        approvalRegistration.approvalRegistrationId,
      ),
    );
    const actions = element("div", "action-row");
    const decide = button("提交已登记决定", "submit");
    decide.addEventListener("click", () => {
      decide.disabled = true;
      void submitApprovalDecision(record, decide);
    });
    const inspect = button("刷新审批状态", "secondary-button");
    inspect.addEventListener("click", () => {
      inspect.disabled = true;
      void refreshApprovalStatus(record, inspect);
    });
    actions.append(decide, inspect);
    detail.append(actions);
  }

  section.append(detail);
  return section;
}

function nativeReleasePanel(record: IdentityRecord): HTMLElement {
  const section = element("section", "session-composer");
  section.append(
    element("h3", "section-title", "原生会话释放"),
    paragraph(
      "先登记释放操作，再明确执行。只有节点确认原会话关闭并可靠记录结果后，界面才显示已释放。",
      "field-help",
    ),
  );

  const form = element("form", "pair-form");
  const label = element("label", undefined, "待释放 target ID");
  label.htmlFor = "native-release-target";
  const input = element("input") as HTMLInputElement;
  input.id = "native-release-target";
  input.name = "native-release-target";
  input.type = "text";
  input.maxLength = 22;
  input.required = true;
  input.autocomplete = "off";
  input.spellcheck = false;
  input.value = nativeReleaseTargetId ?? inspectedQueueTargetId ?? "";
  input.placeholder = "22 位 canonical base64url ID";
  input.disabled = nativeReleaseRegistration !== null;
  const register = button("登记释放", "submit");
  register.type = "submit";
  register.disabled = nativeReleaseRegistration !== null;
  form.append(label, input, register);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    let targetId: string;
    try {
      targetId = requireSessionId(input.value.trim());
    } catch {
      showManagementDashboard(record, undefined, "释放 target ID 格式无效。");
      return;
    }
    register.disabled = true;
    void registerNativeRelease(record, targetId, register);
  });
  section.append(form);

  const registration = nativeReleaseRegistration;
  if (registration === null) {
    return section;
  }
  section.append(
    detailBlock("释放操作", registration.releaseOperationId),
    detailBlock("目标", registration.targetId),
  );
  if (nativeReleaseStatus !== null) {
    section.append(
      detailBlock(
        "释放状态",
        nativeReleaseStatus.state === "failed"
          ? `failed / ${nativeReleaseStatus.error}`
          : nativeReleaseStatus.state,
      ),
    );
  }
  const actions = element("div", "action-row");
  const release = button(
    nativeReleaseStatus?.state === "released" ? "重试释放协调" : "执行原生释放",
    "danger-button",
  );
  release.addEventListener("click", () => {
    release.disabled = true;
    void executeNativeRelease(record, release);
  });
  const inspect = button("刷新释放状态", "secondary-button");
  inspect.addEventListener("click", () => {
    inspect.disabled = true;
    void refreshNativeReleaseStatus(record, inspect);
  });
  const another = button("登记另一目标", "secondary-button");
  another.addEventListener("click", () => {
    nativeReleaseTargetId = null;
    nativeReleaseRegistration = null;
    nativeReleaseStatus = null;
    showManagementDashboard(record, "已清除本地释放操作显示。");
  });
  actions.append(release, inspect, another);
  section.append(actions);
  return section;
}

async function loadApprovalList(
  record: IdentityRecord,
  generation: number,
): Promise<void> {
  const connection = management;
  if (connection === null || generation !== managementGeneration) {
    showManagementDashboard(
      record,
      undefined,
      "管理连接已经关闭，无法读取审批列表。",
    );
    return;
  }
  try {
    const parsed = await connection.listApprovals();
    if (generation !== managementGeneration) return;
    approvalList = parsed.items;
    if (selectedApproval !== null) {
      selectedApproval =
        parsed.items.find(
          (item) => item.approvalId === selectedApproval?.approvalId,
        ) ?? null;
      if (selectedApproval === null) {
        approvalRegistration = null;
        approvalStatus = null;
      }
    }
    showManagementDashboard(
      record,
      `审批列表已更新，共 ${parsed.items.length} 项。`,
    );
  } catch (error) {
    if (generation !== managementGeneration) return;
    showManagementDashboard(
      record,
      undefined,
      `审批列表读取失败：${managementErrorMessage(error)}`,
    );
  }
}

async function loadApproval(
  record: IdentityRecord,
  approvalId: string,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法读取审批。");
    return;
  }
  try {
    const approval = await connection.readApproval(approvalId);
    if (generation !== managementGeneration) return;
    selectedApproval = approval;
    approvalRegistration = null;
    approvalStatus = approval.status;
    showManagementDashboard(record, "审批详情已读取。");
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `审批读取失败：${managementErrorMessage(error)}`,
    );
  }
}

async function registerApprovalChoice(
  record: IdentityRecord,
  approvalId: string,
  choiceId: string,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法登记决定。");
    return;
  }
  try {
    const registration = await connection.registerApproval(
      approvalId,
      choiceId,
    );
    if (generation !== managementGeneration) return;
    approvalRegistration = registration;
    showManagementDashboard(record, "审批决定已登记，尚未交付给 Harness。");
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `审批决定登记失败：${managementErrorMessage(error)}`,
    );
  }
}

async function submitApprovalDecision(
  record: IdentityRecord,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const approval = selectedApproval;
  const registration = approvalRegistration;
  const generation = managementGeneration;
  if (connection === null || approval === null || registration === null) {
    showManagementDashboard(record, undefined, "审批决定登记上下文不完整。");
    return;
  }
  try {
    const result = await connection.decideApproval(
      approval.approvalId,
      registration.approvalRegistrationId,
    );
    if (generation !== managementGeneration) return;
    const status = await connection.inspectApprovalStatus(approval.approvalId);
    if (generation !== managementGeneration) return;
    approvalStatus = status;
    selectedApproval = { ...approval, status };
    showManagementDashboard(
      record,
      `节点已${result.state === "accepted" ? "接受" : "确认已有"}该唯一决定；交付状态为 ${status.delivery}。`,
    );
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `审批决定提交失败：${managementErrorMessage(error)}`,
    );
  }
}

async function refreshApprovalStatus(
  record: IdentityRecord,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const approval = selectedApproval;
  const generation = managementGeneration;
  if (connection === null || approval === null) {
    showManagementDashboard(record, undefined, "没有可刷新的审批。");
    return;
  }
  try {
    const status = await connection.inspectApprovalStatus(
      approval.approvalId,
    );
    if (generation !== managementGeneration) return;
    approvalStatus = status;
    selectedApproval = { ...approval, status };
    showManagementDashboard(record, "审批状态已刷新。");
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `审批状态读取失败：${managementErrorMessage(error)}`,
    );
  }
}

async function registerNativeRelease(
  record: IdentityRecord,
  targetId: string,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法登记释放。");
    return;
  }
  try {
    const registration = await connection.registerNativeRelease(targetId);
    if (generation !== managementGeneration) return;
    nativeReleaseTargetId = registration.targetId;
    nativeReleaseRegistration = registration;
    nativeReleaseStatus = null;
    showManagementDashboard(record, "释放操作已登记，尚未执行原生关闭。");
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `释放登记失败：${managementErrorMessage(error)}`,
    );
  }
}

async function executeNativeRelease(
  record: IdentityRecord,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const registration = nativeReleaseRegistration;
  const generation = managementGeneration;
  if (connection === null || registration === null) {
    showManagementDashboard(record, undefined, "释放登记上下文不完整。");
    return;
  }
  try {
    const status = await connection.releaseNativeSession(
      registration.releaseOperationId,
    );
    if (generation !== managementGeneration) return;
    nativeReleaseStatus = status;
    sessionCapacity = null;
    showManagementDashboard(
      record,
      status.state === "released"
        ? "原生会话已关闭，必要管理结果已由节点确认。"
        : `原生释放当前状态为 ${status.state}。`,
    );
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `原生释放失败：${managementErrorMessage(error)}`,
    );
  }
}

async function refreshSessionCapacity(
  record: IdentityRecord, submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) return;
  try {
    const capacity = await connection.inspectSessionCapacity();
    if (generation !== managementGeneration) return;
    sessionCapacity = capacity;
    showManagementDashboard(record, "会话名额已从节点刷新。");
  } catch (error) {
    if (generation !== managementGeneration) return;
    sessionCapacity = null;
    submit.disabled = false;
    showManagementDashboard(record, undefined, `名额读取失败：${managementErrorMessage(error)}`);
  }
}

async function refreshNativeReleaseStatus(
  record: IdentityRecord,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const registration = nativeReleaseRegistration;
  const generation = managementGeneration;
  if (connection === null || registration === null) {
    showManagementDashboard(record, undefined, "没有可刷新的释放操作。");
    return;
  }
  try {
    const status = await connection.inspectNativeReleaseStatus(
      registration.releaseOperationId,
    );
    if (generation !== managementGeneration) return;
    nativeReleaseStatus = status;
    showManagementDashboard(record, "原生释放状态已刷新。");
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `释放状态读取失败：${managementErrorMessage(error)}`,
    );
  }
}

function workspaceCatalogPanel(record: IdentityRecord): HTMLElement {
  return workspaceCatalogPanelView(
    document,
    {
      connected: management !== null,
      inventory: catalogInventory,
      workspacePathDraft: catalogWorkspacePathDraft,
      installDraft: catalogInstallDraft,
      targetDraft: catalogTargetDraft,
    },
    {
      onRefreshInventory: (submit) => {
        void refreshCatalogInventory(record, managementGeneration, false, submit);
      },
      onWorkspacePathChange: (value) => {
        catalogWorkspacePathDraft = value;
      },
      onRegisterWorkspace: (submit) => {
        void registerCatalogWorkspace(record, submit);
      },
      onInstallFieldChange: (field, value) => {
        catalogInstallDraft = { ...catalogInstallDraft, [field]: value };
        if (field === "sourceMode") {
          showManagementDashboard(record);
        }
      },
      onInstallHarness: (submit) => {
        void installCatalogHarness(record, submit);
      },
      onTargetFieldChange: (field, value) => {
        catalogTargetDraft = { ...catalogTargetDraft, [field]: value };
      },
      onRegisterTarget: (submit) => {
        void registerCatalogTarget(record, submit);
      },
      onRegisterAndBindTarget: (submit) => {
        void registerAndBindCatalogTarget(record, submit);
      },
      onBindTarget: (submit) => {
        void bindExistingCatalogTarget(record, submit);
      },
      onUseWorkspace: (workspace) => {
        catalogTargetDraft = {
          ...catalogTargetDraft,
          workspaceId: workspace.workspaceId,
          launchPath: workspace.canonicalPath,
        };
        sourceSelectionDraft = {
          ...sourceSelectionDraft,
          workspaceId: workspace.workspaceId,
          launchPath: workspace.canonicalPath,
        };
        showManagementDashboard(record, "已将工作目录填入 Target 与来源表单。");
      },
      onUseHarnessProfile: (harnessId, sourceId, profileId, profileRevision) => {
        catalogTargetDraft = {
          ...catalogTargetDraft,
          sourceId,
          harnessId,
          profileId,
          profileRevision,
        };
        sourceSelectionDraft = {
          ...sourceSelectionDraft,
          sourceId,
          harnessId,
          profileId,
          profileRevision,
        };
        showManagementDashboard(record, "已将 Harness / Profile 填入 Target 与来源表单。");
      },
      onSelectTargetQueue: (targetId) => {
        inspectedQueueTargetId = targetId;
        inspectedQueue = null;
        sessionResume.clear();
        disposeSharedViews();
        clearSharedSourceViewSpec();
        resetSessionInputUi();
        void loadSessionQueue(record, targetId, managementGeneration);
      },
      onSelectTargetRelease: (targetId) => {
        nativeReleaseTargetId = targetId;
        showManagementDashboard(record, "已填入原生释放 Target ID。");
      },
      onSelectTargetForBind: (target) => {
        catalogTargetDraft = {
          ...catalogTargetDraft,
          targetId: target.targetId,
          sourceId: target.sourceId,
          workspaceId: target.workspaceId ?? catalogTargetDraft.workspaceId,
        };
        showManagementDashboard(record, "已填入待绑定 Target 信息。");
      },
      onFillSourceSelection: (selection, nativeSessionId) => {
        sourceSelectionDraft = selection;
        sourceManualNativeIdDraft = nativeSessionId;
        showManagementDashboard(record, "已将已绑定 Target 配置填入来源会话面板。");
      },
    },
  );
}

function catalogActionErrorText(error: unknown): string {
  if (error instanceof CatalogDraftValidationError) {
    return error.message;
  }
  if (error instanceof CatalogActionError) {
    if (error.code === "preparation_timeout") {
      return "目录文档上传校验超时，请检查节点状态后重试。";
    }
    if (error.code === "upload_failed") {
      if (error.detail !== null) {
        const mapped = managementRequestErrorMessage(error.detail as ManagementErrorCode);
        return mapped !== undefined
          ? `目录文档校验失败：${mapped}`
          : `目录文档校验失败：${error.detail}`;
      }
      return "目录文档上传失败。";
    }
    return "目录操作请求已被节点拒绝，请检查工作目录或 Launch Profile 配置。";
  }
  return managementErrorMessage(error);
}

async function refreshCatalogInventory(
  record: IdentityRecord,
  generation: number,
  silent = false,
  submit?: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  if (connection === null || generation !== managementGeneration) {
    if (submit !== undefined) submit.disabled = false;
    return;
  }
  try {
    const inventory = await connection.inspectCatalogInventory();
    if (generation !== managementGeneration) return;
    catalogInventory = inventory;
    showManagementDashboard(
      currentRecord ?? record,
      silent
        ? undefined
        : `目录清单已更新（Workspaces ${inventory.workspaces.length} · Targets ${inventory.targets.length}）。`,
    );
  } catch (error) {
    if (generation !== managementGeneration) return;
    if (submit !== undefined) submit.disabled = false;
    if (!silent) {
      showManagementDashboard(
        currentRecord ?? record,
        undefined,
        `目录清单读取失败：${catalogActionErrorText(error)}`,
      );
    }
  }
}

async function registerCatalogWorkspace(
  record: IdentityRecord,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法登记工作目录。");
    return;
  }
  const pathValue = catalogWorkspacePathDraft.trim();
  if (pathValue.length === 0 || pathValue.includes("\0")) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "工作目录路径不能为空。");
    return;
  }
  try {
    const workspace = await connection.registerWorkspace(pathValue);
    if (generation !== managementGeneration) return;
    catalogWorkspacePathDraft = "";
    catalogTargetDraft = {
      ...catalogTargetDraft,
      workspaceId: workspace.workspaceId,
      launchPath: workspace.canonicalPath,
    };
    sourceSelectionDraft = {
      ...sourceSelectionDraft,
      workspaceId: workspace.workspaceId,
      launchPath: workspace.canonicalPath,
    };
    try {
      const refreshed = await connection.inspectCatalogInventory();
      if (generation !== managementGeneration) return;
      catalogInventory = refreshed;
    } catch {
      // Keep current inventory if follow-up refresh fails transiently.
    }
    showManagementDashboard(
      record,
      `工作目录已登记：${workspace.workspaceId}（${workspace.canonicalPath}）。`,
    );
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(record, undefined, `工作目录登记失败：${catalogActionErrorText(error)}`);
  }
}

async function installCatalogHarness(
  record: IdentityRecord,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法安装 Harness。");
    return;
  }
  try {
    const input = validateInstallHarnessInput(catalogInstallDraft, catalogInventory);
    const receipt = await connection.executeCatalogDocument(
      buildInstallHarnessDocument(input),
    );
    if (generation !== managementGeneration) return;
    const result = receipt.fact.result;
    if (result !== null && (result.kind === "harness" || result.kind === "profile")) {
      catalogTargetDraft = {
        ...catalogTargetDraft,
        sourceId: result.sourceId,
        harnessId: result.harnessId,
        profileId: result.profileId,
        profileRevision: result.profileRevision,
      };
      sourceSelectionDraft = {
        ...sourceSelectionDraft,
        sourceId: result.sourceId,
        harnessId: result.harnessId,
        profileId: result.profileId,
        profileRevision: result.profileRevision,
      };
    }
    try {
      const refreshed = await connection.inspectCatalogInventory();
      if (generation !== managementGeneration) return;
      catalogInventory = refreshed;
    } catch {
      // Ignore transient refresh failure; receipt already succeeded.
    }
    showManagementDashboard(
      record,
      result !== null && (result.kind === "harness" || result.kind === "profile")
        ? `Harness / Profile 已安装：harness ${result.harnessId}，profile ${result.profileId} rev ${result.profileRevision}。`
        : "Harness / Profile 已安装并提交。",
    );
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(record, undefined, `Harness 安装失败：${catalogActionErrorText(error)}`);
  }
}

async function registerCatalogTarget(
  record: IdentityRecord,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法登记 Target。");
    return;
  }
  let sourceId: string;
  let workspaceId: string;
  const nativeSessionId = catalogTargetDraft.nativeSessionId.trim();
  try {
    sourceId = requireSessionId(catalogTargetDraft.sourceId.trim());
    workspaceId = requireSessionId(catalogTargetDraft.workspaceId.trim());
    if (nativeSessionId.length === 0 || nativeSessionId.includes("\0")) {
      throw new CatalogDraftValidationError("Native Session ID 不能为空。");
    }
  } catch (error) {
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      error instanceof CatalogDraftValidationError
        ? error.message
        : "Source ID 或 Workspace ID 格式无效。",
    );
    return;
  }
  try {
    const target = await connection.registerTarget(sourceId, nativeSessionId, workspaceId);
    if (generation !== managementGeneration) return;
    catalogTargetDraft = {
      ...catalogTargetDraft,
      targetId: target.targetId,
      sourceId: target.sourceId,
      nativeSessionId: target.nativeSessionId,
      workspaceId: target.workspaceId ?? workspaceId,
    };
    try {
      const refreshed = await connection.inspectCatalogInventory();
      if (generation !== managementGeneration) return;
      catalogInventory = refreshed;
    } catch {
      // Ignore transient refresh failure.
    }
    showManagementDashboard(record, `Target 已登记：${target.targetId}（尚未绑定 Profile）。`);
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(record, undefined, `Target 登记失败：${catalogActionErrorText(error)}`);
  }
}

async function registerAndBindCatalogTarget(
  record: IdentityRecord,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法登记并绑定 Target。");
    return;
  }
  const nativeSessionId = catalogTargetDraft.nativeSessionId.trim();
  if (nativeSessionId.length === 0 || nativeSessionId.includes("\0")) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "Native Session ID 不能为空。");
    return;
  }
  const selectedWorkspace = catalogInventory?.workspaces.find(
    (ws) => ws.workspaceId === catalogTargetDraft.workspaceId.trim(),
  );
  let prevalidatedBind: ReturnType<typeof validateBindTargetInput>;
  try {
    prevalidatedBind = validateBindTargetInput(
      catalogTargetDraft,
      "AAAAAAAAAAAAAAAAAAAAAA",
      selectedWorkspace?.canonicalPath,
    );
  } catch (error) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, `Target 绑定参数无效：${catalogActionErrorText(error)}`);
    return;
  }
  try {
    const target = await connection.registerTarget(
      prevalidatedBind.sourceId,
      nativeSessionId,
      prevalidatedBind.workspaceId,
    );
    if (generation !== managementGeneration) return;
    catalogTargetDraft = {
      ...catalogTargetDraft,
      targetId: target.targetId,
      launchPath: prevalidatedBind.launchPath,
    };
    await connection.executeCatalogDocument(
      buildBindTargetInitialDocument({
        ...prevalidatedBind,
        targetId: target.targetId,
      }),
    );
    if (generation !== managementGeneration) return;
    try {
      const refreshed = await connection.inspectCatalogInventory();
      if (generation !== managementGeneration) return;
      catalogInventory = refreshed;
    } catch {
      // Ignore transient refresh failure.
    }
    showManagementDashboard(
      record,
      `Target 已登记并绑定：${target.targetId}（profile ${prevalidatedBind.profileId} rev ${prevalidatedBind.profileRevision}）。`,
    );
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(record, undefined, `Target 登记或绑定失败：${catalogActionErrorText(error)}`);
  }
}

async function bindExistingCatalogTarget(
  record: IdentityRecord,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法绑定 Target。");
    return;
  }
  const selectedWorkspace = catalogInventory?.workspaces.find(
    (ws) => ws.workspaceId === catalogTargetDraft.workspaceId.trim(),
  );
  try {
    const input = validateBindTargetInput(
      catalogTargetDraft,
      undefined,
      selectedWorkspace?.canonicalPath,
    );
    await connection.executeCatalogDocument(buildBindTargetInitialDocument(input));
    if (generation !== managementGeneration) return;
    catalogTargetDraft = { ...catalogTargetDraft, launchPath: input.launchPath };
    try {
      const refreshed = await connection.inspectCatalogInventory();
      if (generation !== managementGeneration) return;
      catalogInventory = refreshed;
    } catch {
      // Ignore transient refresh failure.
    }
    showManagementDashboard(
      record,
      `Target 已绑定：${input.targetId}（profile ${input.profileId} rev ${input.profileRevision}）。`,
    );
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(record, undefined, `Target 绑定失败：${catalogActionErrorText(error)}`);
  }
}

function sourceSessionPanel(record: IdentityRecord): HTMLElement {
  return sourceSessionPanelView(
    document,
    {
      connected: management !== null,
      selection: sourceSelectionDraft,
      cursor: sourceDiscoverCursorDraft,
      manualNativeSessionId: sourceManualNativeIdDraft,
      registration: sourceRegistration,
      fact: sourceFact,
      result: inspectedSourceResult,
      rawExpanded: sourceRawInspectorExpanded,
      activeSourceView:
        sharedViewSpec?.kind === "source"
          ? { selection: sharedViewSpec.selection, nativeSessionId: sharedViewSpec.nativeSessionId }
          : null,
      openedView: sharedViewSpec?.kind === "source" ? openedSessionView : null,
      staleNotice: sharedViewSpec?.kind === "source" ? sharedViewStaleNotice : null,
    },
    {
      onSelectionFieldChange: (field, value) => {
        sourceSelectionDraft = { ...sourceSelectionDraft, [field]: value };
      },
      onCursorChange: (value) => { sourceDiscoverCursorDraft = value; },
      onManualNativeSessionIdChange: (value) => { sourceManualNativeIdDraft = value; },
      onRegister: (kind, submit) => { void registerSourceSessionOp(record, kind, submit); },
      onExecute: (submit) => { void executeSourceSessionOp(record, submit); },
      onRefreshStatus: (submit) => { void refreshSourceSessionStatus(record, submit); },
      onReconcile: (submit) => { void reconcileSourceSessionOp(record, submit); },
      onOpenResult: (submit) => { void openSourceSessionResultStream(record, submit); },
      onReleaseResult: (submit) => { void releaseSourceSessionResultOp(record, submit); },
      onClearOperation: () => {
        resetSourceResultRead();
        disposeSourceRawInspectors();
        sourceRegistration = null;
        sourceFact = null;
        inspectedSourceResult = null;
        sourceRawInspectorExpanded = false;
        showManagementDashboard(record, "已清除当前来源操作显示。");
      },
      onUseNextCursor: (nextCursor) => {
        sourceDiscoverCursorDraft = nextCursor;
        showManagementDashboard(record, "已填入下一页 discover cursor。");
      },
      onOpenDiscoveredView: (nativeSessionId, submit) => {
        sourceManualNativeIdDraft = nativeSessionId;
        void openSourceViewFromSelection(record, nativeSessionId, submit, "first_open");
      },
      onOpenManualView: (submit) => {
        void openSourceViewFromSelection(record, sourceManualNativeIdDraft, submit, "first_open");
      },
      onRefreshSourceView: (submit) => {
        if (sharedViewSpec?.kind !== "source") { submit.disabled = false; return; }
        void openSharedSourceView(record, sharedViewSpec.selection, sharedViewSpec.nativeSessionId, submit, "explicit_refresh");
      },
      onCloseSourceView: (submit) => {
        const opened = openedSessionView;
        if (opened === null) { submit.disabled = false; return; }
        void closeSharedSourceView(record, opened, submit);
      },
      onAbandonSourceView: () => {
        disposeSharedViews();
        clearSharedSourceViewSpec();
        showManagementDashboard(record, "已关闭本地来源只读视图。");
      },
      onToggleRawResult: (expanded) => {
        sourceRawInspectorExpanded = expanded;
        showManagementDashboard(record);
      },
      onRegisterInspector: (inspector) => { sourceRawInspectors.add(inspector); },
    },
  );
}

async function registerSourceSessionOp(record: IdentityRecord, kind: "discover" | "create", submit: HTMLButtonElement): Promise<void> {
  const connection = management, generation = managementGeneration;
  if (connection === null) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法登记来源操作。");
    return;
  }
  let selection: SourceLaunchSelectionInput;
  let operation: SourceOperationInput;
  try {
    selection = validateSourceSelectionInput(sourceSelectionDraft);
    operation = validateSourceOperationInput(selection, kind, sourceDiscoverCursorDraft);
    sourceSelectionDraft = selection;
  } catch {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "来源选择参数或 discover cursor 格式无效，请检查 ID、revision 与非空路径/游标。");
    return;
  }
  try {
    const registration = await connection.registerSourceSession(selection, operation);
    if (generation !== managementGeneration) return;
    resetSourceResultRead();
    disposeSourceRawInspectors();
    sourceRegistration = registration;
    sourceFact = null;
    inspectedSourceResult = null;
    sourceRawInspectorExpanded = false;
    showManagementDashboard(record, `来源操作已登记（${registration.action}），尚未执行。`);
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(record, undefined, `来源操作登记失败：${managementErrorMessage(error)}`);
  }
}

async function executeSourceSessionOp(record: IdentityRecord, submit: HTMLButtonElement): Promise<void> {
  const connection = management, registration = sourceRegistration, generation = managementGeneration;
  if (connection === null || registration === null) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "来源操作上下文不完整，无法执行。");
    return;
  }
  try {
    const fact = await connection.executeSourceSession(registration.sourceOperationId);
    if (generation !== managementGeneration) return;
    sourceFact = fact;
    showManagementDashboard(record, `来源操作已推进至 ${fact.state} / ${fact.outcome}。`);
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(record, undefined, `来源操作执行失败：${managementErrorMessage(error)}`);
  }
}

async function refreshSourceSessionStatus(record: IdentityRecord, submit: HTMLButtonElement): Promise<void> {
  const connection = management, registration = sourceRegistration, generation = managementGeneration;
  if (connection === null || registration === null) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "没有可查询的来源操作。");
    return;
  }
  try {
    const fact = await connection.inspectSourceSessionStatus(registration.sourceOperationId);
    if (generation !== managementGeneration) return;
    sourceFact = fact;
    showManagementDashboard(record, `来源操作状态已刷新：${fact.state} / ${fact.outcome}。`);
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(record, undefined, `来源状态读取失败：${managementErrorMessage(error)}`);
  }
}

async function reconcileSourceSessionOp(record: IdentityRecord, submit: HTMLButtonElement): Promise<void> {
  const connection = management, registration = sourceRegistration, generation = managementGeneration;
  if (connection === null || registration === null) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "没有可对账的来源操作。");
    return;
  }
  try {
    const fact = await connection.reconcileSourceSession(registration.sourceOperationId);
    if (generation !== managementGeneration) return;
    sourceFact = fact;
    showManagementDashboard(record, `来源操作已完成对账：${fact.state} / ${fact.outcome}。`);
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(record, undefined, `来源对账失败：${managementErrorMessage(error)}`);
  }
}

async function openSourceSessionResultStream(record: IdentityRecord, submit: HTMLButtonElement): Promise<void> {
  const connection = management, registration = sourceRegistration, generation = managementGeneration;
  if (connection === null || registration === null) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "没有可读取结果的来源操作。");
    return;
  }
  const attempt = ++sourceResultAttempt;
  resetSourceResultRead(false);
  disposeSourceRawInspectors();
  inspectedSourceResult = null;
  sourceRawInspectorExpanded = false;
  let openedStreamId: string | null = null;
  try {
    const descriptor = await connection.openSourceSessionResult(registration.sourceOperationId);
    if (generation !== managementGeneration || management !== connection ||
        sourceRegistration?.sourceOperationId !== registration.sourceOperationId || attempt !== sourceResultAttempt) {
      try { connection.detachSessionStream(descriptor.streamId); } catch {}
      return;
    }
    openedStreamId = descriptor.streamId;
    const reader = new SourceSessionResultReader(descriptor.streamId, BigInt(descriptor.totalBytes));
    activeSourceResultRead = { streamId: descriptor.streamId, reader, fact: sourceFact };
    sourceResultTimer = globalThis.setTimeout(() => {
      if (attempt === sourceResultAttempt && generation === managementGeneration && management === connection) {
        failSourceResultRead(record, generation, "读取来源结果超时。");
      }
    }, 30_000);
    connection.sendSessionViewCredit(descriptor.streamId, 0n);
    showManagementDashboard(record, "正在逐块读取来源操作原始结果…");
  } catch (error) {
    if (generation !== managementGeneration || management !== connection || attempt !== sourceResultAttempt) return;
    const detail = error instanceof SourceSessionResultReadError && error.code === "result_too_large"
      ? "来源结果体积超出当前页面安全显示上限。" : managementErrorMessage(error);
    if (openedStreamId !== null && activeSourceResultRead?.streamId === openedStreamId) {
      failSourceResultRead(record, generation, detail); submit.disabled = false; return;
    }
    if (openedStreamId !== null) { try { connection.detachSessionStream(openedStreamId); } catch {} }
    submit.disabled = false;
    showManagementDashboard(record, undefined, `来源结果读取未完成：${detail}`);
  }
}

async function releaseSourceSessionResultOp(record: IdentityRecord, submit: HTMLButtonElement): Promise<void> {
  const connection = management, registration = sourceRegistration, generation = managementGeneration;
  if (connection === null || registration === null) {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "没有可释放结果的来源操作。");
    return;
  }
  try {
    await connection.releaseSourceSessionResult(registration.sourceOperationId);
    if (generation !== managementGeneration) return;
    resetSourceResultRead();
    const updatedFact = await connection.inspectSourceSessionStatus(registration.sourceOperationId);
    if (generation !== managementGeneration) return;
    sourceFact = updatedFact;
    showManagementDashboard(record, "来源结果 offer 已释放。");
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(record, undefined, `释放来源结果失败：${managementErrorMessage(error)}`);
  }
}

async function openSourceViewFromSelection(
  record: IdentityRecord,
  rawNativeSessionId: string,
  submit: HTMLButtonElement,
  reason: "first_open" | "explicit_refresh" | "reopen" | "reconnect" | "reacquire",
): Promise<void> {
  let validated;
  try {
    const selection = validateSourceSelectionInput(sourceSelectionDraft);
    validated = validateSourceViewTargetInput(selection, rawNativeSessionId, reason);
    sourceSelectionDraft = validated.selection;
  } catch {
    submit.disabled = false;
    showManagementDashboard(record, undefined, "来源选择或 nativeSessionId 无效，请先补全有效选择信息与非空原生会话 ID。");
    return;
  }
  await openSharedSourceView(record, validated.selection, validated.nativeSessionId, submit, reason);
}

function approvalStatusText(status: ApprovalStatus): string {
  const decision =
    status.decision === null
      ? "未裁决"
      : `已裁决 ${status.decision.choiceId ?? "unknown"}`;
  return `${status.validity}; ${decision}; 交付 ${status.delivery}; 原连接 ${status.sourceClose}`;
}

function resetSessionInputUi(): void {
  ++bodyReadAttempt;
  editingQueueEntry = null;
  bodyPreview = null;
  activeBodyRead?.reader.seal();
  if (activeBodyRead !== null) {
    try { management?.detachSessionStream(activeBodyRead.streamId); } catch { /* Local read is released regardless. */ }
  }
  activeBodyRead = null;
  if (bodyReadTimer !== null) {
    globalThis.clearTimeout(bodyReadTimer);
    bodyReadTimer = null;
  }
  resetSourceResultRead();
  disposeSourceRawInspectors();
}

function sessionInputComposer(
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
    void runSubmitSessionText(
      record,
      targetId,
      textarea.value,
      submit,
    );
  });
  section.append(form);
  return section;
}

function sessionEditComposer(
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
    editingQueueEntry = null;
    showManagementDashboard(record, "已放弃本次编辑，队列未被修改。");
  });
  const actions = element("div", "action-row");
  actions.append(dismiss, submit);
  form.append(label, textarea, actions);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    submit.disabled = true;
    dismiss.disabled = true;
    void runEditSessionText(
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

async function runSubmitSessionText(
  record: IdentityRecord,
  targetId: string,
  text: string,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法提交输入。");
    return;
  }
  try {
    const result = await submitSessionText(connection, targetId, text);
    if (generation !== managementGeneration) return;
    const notice = result.fact.state === "settled"
      ? (result.fact.outcome.kind === "rejected"
        ? `输入已登记但被队列拒绝（${result.fact.outcome.reason}）；队列最新状态已重新读取。`
        : `输入已登记并结算为 ${result.fact.state}（${result.fact.outcome.kind}）；队列最新状态已重新读取。`)
      : `输入已登记并处于 ${result.fact.state}；队列最新状态已重新读取。`;
    await loadSessionQueue(record, targetId, generation, notice);
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `提交未完成：${sessionActionErrorMessage(error)}`,
    );
  }
}

async function runEditSessionText(
  record: IdentityRecord,
  targetId: string,
  entry: SessionQueueEntry,
  text: string,
  submit: HTMLButtonElement,
  dismiss: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法编辑输入。");
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
    if (generation !== managementGeneration) return;
    editingQueueEntry = null;
    const notice = result.fact.state === "settled"
      ? (result.fact.outcome.kind === "rejected"
        ? `队列项编辑被拒绝（${result.fact.outcome.reason}）。`
        : `队列项编辑已结算为 ${result.fact.state}（${result.fact.outcome.kind}）。`)
      : `队列项编辑当前状态为 ${result.fact.state}。`;
    await loadSessionQueue(record, targetId, generation, notice);
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    dismiss.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `编辑未完成：${sessionActionErrorMessage(error)}`,
    );
  }
}

async function runSessionQueueControl(
  record: IdentityRecord,
  targetId: string,
  entry: SessionQueueEntry,
  action: "cancel" | "prioritize",
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法更新队列。");
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
    if (generation !== managementGeneration) return;
    const label = action === "cancel" ? "取消" : "立即发送";
    const notice = fact.state === "settled"
      ? (fact.outcome.kind === "rejected"
        ? `${label}操作被队列拒绝（${fact.outcome.reason}）。`
        : `${label}操作已结算为 ${fact.state}（${fact.outcome.kind}）。`)
      : `${label}操作当前状态为 ${fact.state}。`;
    await loadSessionQueue(record, targetId, generation, notice);
  } catch (error) {
    if (generation !== managementGeneration) return;
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `${action === "cancel" ? "取消" : "立即发送"}未完成：${sessionActionErrorMessage(error)}`,
    );
  }
}

async function openQueueBody(
  record: IdentityRecord,
  targetId: string,
  entry: SessionQueueEntry,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法读取正文。");
    return;
  }
  const attempt = ++bodyReadAttempt;
  const previous = activeBodyRead;
  activeBodyRead = null;
  bodyPreview = null;
  if (bodyReadTimer !== null) {
    globalThis.clearTimeout(bodyReadTimer);
    bodyReadTimer = null;
  }
  if (previous !== null) {
    previous.reader.seal();
    try { connection.detachSessionStream(previous.streamId); } catch { /* Previous read is already locally sealed. */ }
  }
  let openedStreamId: string | null = null;
  try {
    const descriptor = await connection.openSessionBody(
      targetId,
      entry.itemId,
      entry.revision,
    );
    if (generation !== managementGeneration || management !== connection ||
        inspectedQueueTargetId !== targetId || attempt !== bodyReadAttempt) {
      try { connection.detachSessionStream(descriptor.streamId); } catch { /* Stale response belongs to original connection. */ }
      return;
    }
    openedStreamId = descriptor.streamId;
    const reader = new QueuedBodyReader(
      descriptor.streamId,
      BigInt(descriptor.totalBytes),
    );
    activeBodyRead = {
      streamId: descriptor.streamId,
      reader,
    };
    bodyPreview = null;
    if (bodyReadTimer !== null) {
      globalThis.clearTimeout(bodyReadTimer);
    }
    bodyReadTimer = globalThis.setTimeout(() => {
      if (attempt === bodyReadAttempt && generation === managementGeneration && management === connection) {
        failQueueBodyRead(record, generation, "读取正文超时。");
      }
    }, 30_000);
    connection.sendSessionViewCredit(descriptor.streamId, 0n);
    showManagementDashboard(record, "正在逐块读取队列正文…");
  } catch (error) {
    if (generation !== managementGeneration || management !== connection ||
        inspectedQueueTargetId !== targetId || attempt !== bodyReadAttempt) return;
    const detail =
      error instanceof QueuedBodyReadError &&
      error.code === "body_too_large"
        ? "正文过大，无法在当前页面安全显示。"
        : managementErrorMessage(error);
    if (
      openedStreamId !== null &&
      activeBodyRead?.streamId === openedStreamId
    ) {
      failQueueBodyRead(record, generation, detail);
      submit.disabled = false;
      return;
    }
    if (openedStreamId !== null) {
      try {
        connection.detachSessionStream(openedStreamId);
      } catch {
        // The connection may already have ended while opening the body.
      }
    }
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `正文读取未完成：${detail}`,
    );
  }
}

function handleSessionViewData(
  record: IdentityRecord,
  generation: number,
  frame: ViewDataFrame,
): void {
  if (generation !== managementGeneration) return;
  if (sharedSessionDisplay !== null && frame.streamId === sharedSessionDisplay.streamId) {
    sharedSessionDisplay.accept(frame);
    return;
  }
  if (activeSourceResultRead !== null && frame.streamId === activeSourceResultRead.streamId) {
    handleSourceResultViewData(record, generation, frame);
    return;
  }
  const read = activeBodyRead;
  if (read === null || frame.streamId !== read.streamId) return;
  let update;
  try {
    update = read.reader.accept(frame);
  } catch (error) {
    const reason =
      error instanceof QueuedBodyReadError &&
      error.code === "body_too_large"
        ? "正文过大，无法在当前页面安全显示。"
        : "正文流序号、偏移或内容不一致。";
    failQueueBodyRead(record, generation, reason);
    return;
  }
  try {
    management?.sendSessionViewCredit(frame.streamId, frame.packetSequence);
  } catch (error) {
    failQueueBodyRead(
      record,
      generation,
      `无法确认正文分块：${managementErrorMessage(error)}`,
    );
    return;
  }
  if (update.state === "complete") {
    bodyPreview = update.preview;
    read.reader.seal();
    activeBodyRead = null;
    if (bodyReadTimer !== null) {
      globalThis.clearTimeout(bodyReadTimer);
      bodyReadTimer = null;
    }
    showManagementDashboard(record, "队列输入正文已完整读取并显示。");
  }
}

function failQueueBodyRead(
  record: IdentityRecord,
  generation: number,
  detail: string,
): void {
  if (activeBodyRead !== null) {
    activeBodyRead.reader.seal();
    try {
      management?.detachSessionStream(activeBodyRead.streamId);
    } catch {
      // The local read is discarded even when detach cannot be delivered.
    }
  }
  activeBodyRead = null;
  bodyPreview = null;
  if (bodyReadTimer !== null) {
    globalThis.clearTimeout(bodyReadTimer);
    bodyReadTimer = null;
  }
  if (generation === managementGeneration) {
    showManagementDashboard(record, undefined, detail);
  }
}

async function openSharedSessionView(
  record: IdentityRecord,
  targetId: string,
  submit: HTMLButtonElement | null,
  reason: "first_open" | "explicit_refresh" | "reopen" | "reconnect" | "reacquire" = "first_open",
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  const attempt = ++sharedViewAttempt;
  sharedViewTargetId = targetId;
  if (connection === null) {
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法打开共享视图。");
    return;
  }
  try {
    const opened = await connection.openSessionView(targetId, reason);
    if (generation !== managementGeneration || management !== connection ||
        attempt !== sharedViewAttempt || sharedViewTargetId !== targetId ||
        inspectedQueueTargetId !== targetId) {
      try { connection.detachSessionStream(opened.streamId); } catch { /* Original connection may be closed. */ }
      void connection.closeSessionView(opened.streamId).catch(() => undefined);
      return;
    }
    const previous = sharedSessionDisplay;
    const previousConnection = sharedSessionConnection;
    const previousOpened = openedSessionView;
    let display: SessionViewDisplay | null = null;
    try {
      display = new SessionViewDisplay(document, opened, {
        credit: (streamId, sequence) => connection.sendSessionViewCredit(streamId, sequence),
        detach: (streamId) => connection.detachSessionStream(streamId),
        reopenRequested: () => {
          void rebindSharedView(record, opened);
        },
        snapshotReady: () => {
          if (sharedSessionDisplay !== display) return;
          for (const stale of staleSharedDisplays) stale.dispose();
          staleSharedDisplays.clear();
        },
      });
      sharedSessionDisplay = display;
      sharedSessionConnection = connection;
      openedSessionView = opened;
      sharedViewHost.append(display.element);
      display.start();
    }
    catch (error) {
      if (display !== null && sharedSessionDisplay === display) {
        sharedSessionDisplay = previous;
        sharedSessionConnection = previousConnection;
        openedSessionView = previousOpened;
      }
      releaseSharedDisplay(opened, connection);
      display?.dispose();
      throw error;
    }
    if (previous !== null) {
      previous.suspend();
      releaseSharedDisplay(previous, previousConnection);
      staleSharedDisplays.add(previous);
    }
    showManagementDashboard(record, "共享视图已打开，正在读取快照及后续增量。");
  } catch (error) {
    if (generation !== managementGeneration || attempt !== sharedViewAttempt) return;
    if (submit !== null) submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `共享视图未打开：${managementErrorMessage(error)}`,
    );
  }
}

async function closeSharedSessionView(
  record: IdentityRecord,
  opened: SessionViewOpened,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = sharedSessionConnection;
  const generation = managementGeneration;
  const display = sharedSessionDisplay;
  if (display === null || display.streamId !== opened.streamId) return;
  const attempt = ++sharedViewAttempt;
  sharedViewTargetId = null;
  sharedSessionDisplay = null; sharedSessionConnection = null; openedSessionView = null;
  display.dispose();
  for (const stale of staleSharedDisplays) stale.dispose();
  staleSharedDisplays.clear();
  if (connection === null) {
    showManagementDashboard(record, undefined, "本地视图已释放；管理连接已关闭，未确认远端关闭。");
    return;
  }
  try {
    const closed = await connection.closeSessionView(opened.streamId);
    if (generation !== managementGeneration || attempt !== sharedViewAttempt) return;
    showManagementDashboard(
      record,
      `共享视图已关闭，状态 ${closed.state}，最后确认分块 ${closed.lastCommittedPacket}。`,
    );
  } catch (error) {
    if (generation !== managementGeneration || attempt !== sharedViewAttempt) return;
    if (!display.detached) {
      try { connection.detachSessionStream(opened.streamId); } catch { /* Local disposal already completed. */ }
    }
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `共享视图关闭未完成：${managementErrorMessage(error)}`,
    );
  }
}

function releaseSharedDisplay(
  display: Pick<SessionViewDisplay, "streamId"> & { readonly detached?: boolean },
  connection: ManagementConnection | null,
): void {
  if (connection === null) return;
  if (display.detached !== true) {
    try { connection.detachSessionStream(display.streamId); } catch { /* Best effort on the original transport. */ }
  }
  void connection.closeSessionView(display.streamId).catch(() => undefined);
}

function suspendSharedView(): void {
  ++sharedViewAttempt;
  sharedSessionDisplay?.suspend();
}

function disposeSharedViews(): void {
  ++sharedViewAttempt;
  sharedViewTargetId = null;
  if (sharedSessionDisplay !== null) {
    releaseSharedDisplay(sharedSessionDisplay, sharedSessionConnection);
    sharedSessionDisplay.dispose();
  }
  for (const stale of staleSharedDisplays) stale.dispose();
  staleSharedDisplays.clear();
  sharedSessionDisplay = null; sharedSessionConnection = null; openedSessionView = null;
}

function sessionActionErrorMessage(error: unknown): string {
  if (error instanceof SessionInputActionError) {
    switch (error.code) {
      case "empty_input":
        return "正文不能为空。";
      case "upload_failed":
        return "上传准备失败，已尝试释放暂存正文。";
      case "preparation_timeout":
        return "正文准备超时，没有登记队列项。";
      case "consume_pending":
        return "请求仍在节点确认中，尚未完成结算；请刷新队列核验状态。";
      case "consume_refused":
        return error.detail !== null
          ? `请求被节点拒绝（${error.detail}）。`
          : "请求被节点拒绝。";
    }
  }
  return managementErrorMessage(error);
}

function handleSourceResultViewData(record: IdentityRecord, generation: number, frame: ViewDataFrame): void {
  const read = activeSourceResultRead;
  if (read === null || frame.streamId !== read.streamId) return;
  let update;
  try {
    update = read.reader.accept(frame);
  } catch (error) {
    const reason = error instanceof SourceSessionResultReadError && error.code === "result_too_large"
      ? "来源结果体积超出安全显示上限。" : "来源结果流序号、偏移或载荷结构无效。";
    failSourceResultRead(record, generation, reason);
    return;
  }
  try {
    management?.sendSessionViewCredit(frame.streamId, frame.packetSequence);
  } catch (error) {
    failSourceResultRead(record, generation, `无法确认来源结果分块：${managementErrorMessage(error)}`);
    return;
  }
  if (update.state === "complete") {
    read.reader.seal();
    activeSourceResultRead = null;
    if (sourceResultTimer !== null) { globalThis.clearTimeout(sourceResultTimer); sourceResultTimer = null; }
    disposeSourceRawInspectors();
    inspectedSourceResult = inspectSourceSessionResultPayload(update.rawBytes, read.fact ?? sourceFact);
    sourceRawInspectorExpanded = false;
    showManagementDashboard(
      record,
      inspectedSourceResult.kind === "fallback"
        ? "来源结果已读取，但未能按预期结构解析，已切换为只读安全文本回退。"
        : "来源操作原始结果已完整读取并解析。",
    );
  }
}

function failSourceResultRead(record: IdentityRecord, generation: number, detail: string): void {
  resetSourceResultRead(true);
  if (generation === managementGeneration) showManagementDashboard(record, undefined, detail);
}

function resetSourceResultRead(bumpAttempt = true): void {
  if (bumpAttempt) ++sourceResultAttempt;
  if (activeSourceResultRead !== null) {
    activeSourceResultRead.reader.seal();
    try { management?.detachSessionStream(activeSourceResultRead.streamId); } catch {}
  }
  activeSourceResultRead = null;
  if (sourceResultTimer !== null) { globalThis.clearTimeout(sourceResultTimer); sourceResultTimer = null; }
}

function disposeSourceRawInspectors(): void {
  for (const inspector of sourceRawInspectors) {
    try { inspector.dispose(); } catch {}
  }
  sourceRawInspectors.clear();
}

function clearSharedSourceViewSpec(): void {
  sharedViewSpec = null;
  sharedViewStaleNotice = null;
  clearPersistedSourceViewSpec();
}

function syncSourceDraftFromSpec(spec: Extract<SharedViewSpec, { readonly kind: "source" }>): void {
  if (sourceSelectionDraft.sourceId === "" && sourceSelectionDraft.harnessId === "" &&
      sourceSelectionDraft.profileId === "" && sourceSelectionDraft.profileRevision === "" &&
      sourceSelectionDraft.workspaceId === "" && sourceSelectionDraft.launchPath === "") {
    sourceSelectionDraft = spec.selection;
  }
  if (sourceManualNativeIdDraft === "") sourceManualNativeIdDraft = spec.nativeSessionId;
}

function clearSourceSessionViewState(): void {
  clearSharedSourceViewSpec();
  resetSourceResultRead();
  disposeSourceRawInspectors();
  sourceRegistration = null;
  sourceFact = null;
  inspectedSourceResult = null;
  sourceRawInspectorExpanded = false;
}

async function openTargetSharedView(
  record: IdentityRecord,
  targetId: string,
  submit: HTMLButtonElement | null,
  reason: "first_open" | "explicit_refresh" | "reopen" | "reconnect" | "reacquire" = "first_open",
): Promise<void> {
  clearSharedSourceViewSpec();
  sharedViewSpec = { kind: "target", targetId };
  sharedViewStaleNotice = null;
  await openSharedSessionView(record, targetId, submit, reason);
}

async function closeTargetSharedView(record: IdentityRecord, opened: SessionViewOpened, submit: HTMLButtonElement): Promise<void> {
  if (sharedViewSpec?.kind === "target") { sharedViewSpec = null; sharedViewStaleNotice = null; }
  await closeSharedSessionView(record, opened, submit);
}

async function openSharedSourceView(
  record: IdentityRecord,
  selection: SourceLaunchSelectionInput,
  nativeSessionId: string,
  submit: HTMLButtonElement | null,
  reason: "first_open" | "explicit_refresh" | "reopen" | "reconnect" | "reacquire" = "first_open",
): Promise<void> {
  const connection = management, generation = managementGeneration, attempt = ++sharedViewAttempt;
  sharedViewTargetId = null;
  sharedViewSpec = { kind: "source", selection, nativeSessionId };
  persistSourceViewSpec(selection, nativeSessionId);
  if (connection === null) {
    if (submit !== null) submit.disabled = false;
    sharedViewStaleNotice = (reason === "reconnect" || reason === "explicit_refresh") && sharedSessionDisplay !== null
      ? "管理连接已关闭，未能从来源重新校验；当前保留旧缓存视图。"
      : "管理连接已经关闭，无法打开来源只读视图。";
    if (sharedSessionDisplay !== null && (reason === "reconnect" || reason === "explicit_refresh")) {
      sharedSessionDisplay.element.dataset.stale = "true";
    }
    showManagementDashboard(record, undefined, sharedViewStaleNotice);
    return;
  }
  try {
    const opened = await connection.openSourceSessionView(selection, nativeSessionId, reason);
    if (generation !== managementGeneration || management !== connection || attempt !== sharedViewAttempt ||
        sharedViewSpec?.kind !== "source" || sharedViewSpec.nativeSessionId !== nativeSessionId) {
      try { connection.detachSessionStream(opened.streamId); } catch {}
      void connection.closeSessionView(opened.streamId).catch(() => undefined);
      return;
    }
    const previous = sharedSessionDisplay, previousConnection = sharedSessionConnection, previousOpened = openedSessionView;
    let display: SessionViewDisplay | null = null;
    try {
      display = new SessionViewDisplay(document, opened, {
        credit: (streamId, sequence) => connection.sendSessionViewCredit(streamId, sequence),
        detach: (streamId) => connection.detachSessionStream(streamId),
        reopenRequested: () => { void rebindSharedView(record, opened); },
        snapshotReady: () => {
          if (sharedSessionDisplay !== display) return;
          display?.element.removeAttribute("data-stale");
          sharedViewStaleNotice = null;
          for (const stale of staleSharedDisplays) stale.dispose();
          staleSharedDisplays.clear();
        },
      });
      sharedSessionDisplay = display;
      sharedSessionConnection = connection;
      openedSessionView = opened;
      sharedViewStaleNotice = null;
      sharedViewHost.append(display.element);
      display.start();
    } catch (error) {
      if (display !== null && sharedSessionDisplay === display) {
        sharedSessionDisplay = previous; sharedSessionConnection = previousConnection; openedSessionView = previousOpened;
      }
      releaseSharedDisplay(opened, connection);
      display?.dispose();
      throw error;
    }
    if (previous !== null) {
      previous.suspend();
      releaseSharedDisplay(previous, previousConnection);
      staleSharedDisplays.add(previous);
    }
    showManagementDashboard(
      record,
      opened.status.historyIncomplete
        ? "来源只读视图已打开（当前模式仅包含实时连接与有限缓存，无法补全未缓存前文）。"
        : "来源只读视图已打开，正在读取快照及后续增量。",
    );
  } catch (error) {
    if (generation !== managementGeneration || attempt !== sharedViewAttempt) return;
    if (submit !== null) submit.disabled = false;
    const failureText = managementErrorMessage(error);
    if (reason === "reconnect" || reason === "explicit_refresh") {
      if (sharedSessionDisplay !== null) sharedSessionDisplay.element.dataset.stale = "true";
      sharedViewStaleNotice = `来源只读视图重新校验失败，当前保留旧视图状态：${failureText}`;
      showManagementDashboard(record, undefined, sharedViewStaleNotice);
      return;
    }
    clearSharedSourceViewSpec();
    showManagementDashboard(record, undefined, `来源只读视图未打开：${failureText}`);
  }
}

async function closeSharedSourceView(record: IdentityRecord, opened: SessionViewOpened, submit: HTMLButtonElement): Promise<void> {
  clearSharedSourceViewSpec();
  await closeSharedSessionView(record, opened, submit);
}

async function rebindSharedView(record: IdentityRecord, previousOpened: SessionViewOpened): Promise<void> {
  const connection = management, generation = managementGeneration;
  const previous = sharedSessionDisplay, previousConnection = sharedSessionConnection;
  if (connection === null || previous === null || previous.streamId !== previousOpened.streamId) return;
  const attempt = ++sharedViewAttempt;
  try {
    const reopened = await connection.rebindSessionView(previousOpened.streamId);
    if (generation !== managementGeneration || management !== connection || attempt !== sharedViewAttempt || sharedSessionDisplay !== previous) {
      try { connection.detachSessionStream(reopened.streamId); } catch {}
      void connection.closeSessionView(reopened.streamId).catch(() => undefined);
      return;
    }
    let display: SessionViewDisplay | null = null;
    try {
      display = new SessionViewDisplay(document, reopened, {
        credit: (streamId, sequence) => connection.sendSessionViewCredit(streamId, sequence),
        detach: (streamId) => connection.detachSessionStream(streamId),
        reopenRequested: () => { void rebindSharedView(record, reopened); },
        snapshotReady: () => {
          if (sharedSessionDisplay !== display) return;
          display?.element.removeAttribute("data-stale");
          sharedViewStaleNotice = null;
          for (const stale of staleSharedDisplays) stale.dispose();
          staleSharedDisplays.clear();
        },
      });
      sharedSessionDisplay = display;
      sharedSessionConnection = connection;
      openedSessionView = reopened;
      sharedViewStaleNotice = null;
      sharedViewHost.append(display.element);
      display.start();
    } catch (error) {
      if (display !== null && sharedSessionDisplay === display) {
        sharedSessionDisplay = previous; sharedSessionConnection = previousConnection; openedSessionView = previousOpened;
      }
      releaseSharedDisplay(reopened, connection);
      display?.dispose();
      throw error;
    }
    previous.suspend();
    staleSharedDisplays.add(previous);
    showManagementDashboard(record, "共享视图已按最新状态重新绑定并刷新。");
  } catch (error) {
    if (generation !== managementGeneration || attempt !== sharedViewAttempt) return;
    previous.element.dataset.stale = "true";
    sharedViewStaleNotice = `共享视图重绑定失败，当前保留旧视图状态：${managementErrorMessage(error)}`;
    showManagementDashboard(record, undefined, sharedViewStaleNotice);
  }
}

async function loadSessionQueue(
  record: IdentityRecord,
  targetId: string,
  generation: number,
  notice?: string,
): Promise<void> {
  const connection = management;
  if (connection === null || generation !== managementGeneration) {
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法读取会话队列。");
    return;
  }
  try {
    const snapshot = await connection.inspectSessionQueue(targetId);
    if (generation !== managementGeneration || management !== connection || inspectedQueueTargetId !== targetId) return;
    inspectedQueue = snapshot;
    showManagementDashboard(record, notice ?? "会话队列已更新。");
  } catch (error) {
    if (generation !== managementGeneration || inspectedQueueTargetId !== targetId) return;
    showManagementDashboard(record, undefined, managementErrorMessage(error));
  }
}

async function resumeSessionQueue(
  record: IdentityRecord,
  targetId: string,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  const generation = managementGeneration;
  if (connection === null) {
    showManagementDashboard(record, undefined, "管理连接已经关闭，无法继续旧队列。");
    return;
  }
  try {
    const result = await sessionResume.run(
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
    if (generation !== managementGeneration) return;
    showManagementDashboard(
      record,
      `已显式恢复 ${result.restored} 个旧等待项；队列顺序由节点维护。`,
    );
    await loadSessionQueue(
      record,
      targetId,
      generation,
      `已显式恢复 ${result.restored} 个旧等待项。`,
    );
  } catch (error) {
    if (generation !== managementGeneration || management !== connection) return;
    submit.disabled = false;
    showManagementDashboard(
      record,
      undefined,
      `继续未完成：${managementErrorMessage(error)}。重试会沿用同一请求身份。`,
    );
  }
}

function deviceCard(
  device: DeviceSummary,
  currentClientId: string | null,
  revoke: () => void,
): HTMLElement {
  const card = element("article", "device-card");
  const header = element("div", "device-card-header");
  const name = element("div");
  name.append(
    element("h4", undefined, device.displayName),
    element(
      "span",
      "device-id",
      device.clientId === currentClientId ? "本设备" : device.clientId,
    ),
  );
  header.append(
    name,
    element(
      "span",
      `device-status device-status-${device.status}`,
      device.status === "trusted" ? "可信" : "已撤销",
    ),
  );
  card.append(
    header,
    detailBlock("公钥指纹", device.publicKeyFingerprint),
    detailBlock("创建时间", formatTimestamp(device.createdAt)),
  );
  if (device.revokedAt !== null) {
    card.append(detailBlock("撤销时间", formatTimestamp(device.revokedAt)));
  }
  if (device.status === "trusted") {
    const revokeButton = button(
      device.clientId === currentClientId ? "撤销本设备访问" : "撤销此客户端",
      "danger-button",
    );
    revokeButton.addEventListener("click", revoke);
    card.append(revokeButton);
  }
  return card;
}

function showRevokeConfirmation(
  record: IdentityRecord,
  device: DeviceSummary,
): void {
  const self = device.clientId === record.clientId;
  const fragment = document.createDocumentFragment();
  fragment.append(
    eyebrow("REVOKE / CONFIRM"),
    title(self ? "撤销本设备访问？" : "撤销此客户端？"),
    paragraph(
      self
        ? "提交后节点会立即关闭本设备的管理连接。此操作不会删除浏览器本地密钥；旧密钥被撤销后不能再次配对。"
        : `客户端“${device.displayName}”的现有连接和订阅将被关闭，之后不能再次认证。`,
    ),
    detailBlock("客户端 ID", device.clientId),
    detailBlock("公钥指纹", device.publicKeyFingerprint),
  );
  const actions = element("div", "action-row");
  const cancel = button("返回", "secondary-button");
  cancel.addEventListener("click", () =>
    showManagementDashboard(record, "未提交撤销。"),
  );
  const confirm = button("确认撤销", "danger-button");
  confirm.addEventListener("click", () => {
    confirm.disabled = true;
    cancel.disabled = true;
    void revokeSelectedDevice(record, device, confirm);
  });
  actions.append(cancel, confirm);
  fragment.append(actions);
  panel.replaceChildren(fragment);
}

async function revokeSelectedDevice(
  record: IdentityRecord,
  device: DeviceSummary,
  submit: HTMLButtonElement,
): Promise<void> {
  const connection = management;
  if (connection === null) {
    showManagementDashboard(record, undefined, "管理连接已经关闭。");
    return;
  }
  try {
    await connection.revokeDevice(device.clientId);
    if (device.clientId !== record.clientId) {
      await refreshDevices(true, managementGeneration);
    }
  } catch (error) {
    if (
      error instanceof ManagementConnectionError &&
      error.code === "revoked"
    ) {
      return;
    }
    submit.disabled = false;
    showManagementDashboard(record, undefined, managementErrorMessage(error));
  }
}

function showManagementFailure(record: IdentityRecord, error: unknown): void {
  suspendSharedView();
  lastManagementError = error;
  if (
    error instanceof ManagementConnectionError &&
    error.nodeVerified &&
    (error.code === "revoked" || error.closeCode === 4401)
  ) {
    showRevokedAccess(record);
    return;
  }
  const retryDelay =
    pageActive && !manuallyDisconnected
      ? managementReconnect.schedule(error)
      : null;
  if (pageActive && !manuallyDisconnected) {
    automaticReconnectAllowed = retryDelay !== null;
  }
  const fragment = identityFragment(record);
  fragment.append(
    eyebrow(retryDelay === null ? "MANAGEMENT / STOPPED" : "MANAGEMENT / RECONNECTING"),
    title(retryDelay === null ? "管理连接已中断" : "正在恢复连接"),
    paragraph(managementErrorMessage(error)),
  );
  if (retryDelay !== null) {
    fragment.append(
      statusLine(
        `将在 ${retryDelay / 1_000} 秒后使用已保存的身份自动重连，无需重新配对。`,
        "working",
      ),
    );
  } else if (manuallyDisconnected) {
    fragment.append(paragraph("已主动断开连接，刷新页面后会使用原身份重新连接。"));
  }
  appendIdentityDetails(fragment, record);
  panel.replaceChildren(fragment);
}

function handleManagementClose(record: IdentityRecord, code: number, error: Error): void {
  if (code === 4410 && error instanceof ManagementConnectionError && error.nodeVerified) {
    showRevokedAccess(record);
    return;
  }
  if (code === 4401) {
    showManagementFailure(
      record,
      error,
    );
    return;
  }
  showManagementFailure(
    record,
    error,
  );
}

function showRevokedAccess(record: IdentityRecord): void {
  disposeSharedViews();
  clearSourceSessionViewState();
  managementReconnect.cancel();
  automaticReconnectAllowed = false;
  ++managementGeneration;
  management = null;
  void resetLocalIdentity(record);
}

function recoverChangedIdentity(error: unknown): boolean {
  if (!(error instanceof IdentityStoreError) || error.code !== "identity_changed") return false;
  disposeSharedViews();
  clearSourceSessionViewState();
  resetSessionInputUi();
  controller?.abort();
  managementReconnect.cancel();
  management?.disconnect();
  management = null;
  ++managementGeneration;
  session = null;
  currentRecord = null;
  automaticReconnectAllowed = true;
  pendingRecoveryAttempted = false;
  inspectedQueueTargetId = null;
  inspectedQueue = null;
  catalogInventory = null;
  sessionResume.clear();
  if (pageActive && !manuallyDisconnected) void boot();
  return true;
}

async function resetLocalIdentity(rejected: IdentityRecord): Promise<void> {
  disposeSharedViews();
  clearSourceSessionViewState();
  resetSessionInputUi();
  if (!pageActive || identityResetting) return;
  identityResetting = true;
  identityController.abort();
  identityController = new AbortController();
  const signal = identityController.signal;
  session = null;
  showLoading();
  managementReconnect.reset();
  management?.disconnect();
  management = null;
  ++managementGeneration;
  if (vault === null) {
    identityResetting = false;
    showFatal("无法清除本地身份", "当前页面没有可用的身份存储。");
    return;
  }
  session = null;
  currentRecord = null;
  currentNodeInfo = null;
  currentDevices = [];
  currentRevision = null;
  catalogInventory = null;
  inspectedQueueTargetId = null;
  inspectedQueue = null;
  sessionResume.clear();
  nextDeviceCursor = null;
  controller?.abort();
  try {
    const localVault = vault;
    const record = await runIdentityOperation(() => localVault.replaceRejectedIdentity(rejected, signal));
    if (signal.aborted || !pageActive) return;
    session = new PairingSession(vault, new PairingApi(), record);
    manuallyDisconnected = false;
    pendingRecoveryAttempted = false;
    if (record.status === "unbound") {
      showPairForm(record, "原配对已失效，请输入新的配对码，并在节点终端批准。");
    } else {
      // Another page may already have paired the replacement. Restore its
      // actual state instead of displaying an unusable pairing form.
      identityResetting = false;
      automaticReconnectAllowed = true;
      await boot();
    }
  } catch (error) {
    if (signal.aborted || !pageActive) return;
    showFatal("无法准备重新配对", errorMessage(error));
  } finally {
    identityResetting = false;
  }
}

function identityFragment(record: IdentityRecord): DocumentFragment {
  const fragment = document.createDocumentFragment();
  if (record.nodeId !== null) {
    fragment.append(detailBlock("节点 SHA-256 指纹", formatHex(record.nodeId)));
  }
  return fragment;
}

function appendIdentityDetails(
  fragment: DocumentFragment,
  record: IdentityRecord,
  includeNode = true,
): void {
  if (includeNode && record.nodeId !== null) {
    fragment.append(detailBlock("节点 SHA-256 指纹", formatHex(record.nodeId)));
  }
  if (record.displayName !== null) {
    fragment.append(detailBlock("客户端名称", record.displayName));
  }
  if (record.clientId !== null) {
    fragment.append(detailBlock("客户端 ID", record.clientId));
  }
  const fingerprint = detailBlock("客户端公钥指纹", "正在计算…");
  fragment.append(fingerprint);
  void fillFingerprint(fingerprint, record.publicKey);
}

function formatTimestamp(seconds: number): string {
  const date = new Date(seconds * 1_000);
  return Number.isNaN(date.getTime())
    ? String(seconds)
    : date.toLocaleString("zh-CN", { timeZone: "Asia/Shanghai" });
}

function showTerminal(record: IdentityRecord): void {
  if (
    record.status === "approved" ||
    record.status === "consumed" ||
    record.clientId !== null
  ) {
    void openManagement(record);
    return;
  }
  if (!isFailedPairingStatus(record.status)) {
    showPending(record);
    return;
  }
  const messages = {
    rejected: "本机已拒绝这次配对。",
    cancelled: "本机已取消这次配对。",
    expired: "配对已到期，尚未获得批准。",
  } as const;
  showPairForm(
    record,
    `${messages[record.status]} 可以重新运行 device pair，并使用同一浏览器身份再次认领。`,
  );
}

async function fillFingerprint(
  target: HTMLElement,
  publicKey: string,
): Promise<void> {
  if (vault === null) {
    target.textContent = "不可用";
    return;
  }
  try {
    target.textContent = formatHex(await vault.fingerprint(publicKey));
  } catch {
    target.textContent = "无法计算";
  }
}

function errorMessage(error: unknown): string {
  if (error instanceof PairingApiError) {
    switch (error.code) {
      case "pairing_invalid":
        return "配对码无效、已到期，或这次配对已经不能继续。请在节点重新运行 device pair。";
      case "request_conflict":
        return "同一个认领请求标识对应了不同证明，节点已拒绝。请重新开始配对。";
      case "rate_limited":
        return `请求过于频繁，请 ${error.retryAfterSeconds ?? 60} 秒后重试。`;
      case "storage_unavailable":
        return "节点存储暂时不可用，未确认任何批准结果。";
      case "node_stopping":
        return "节点正在停止，未提交新的配对状态。";
      case "unsupported_version":
        return "节点与页面的接入协议版本不一致。";
      case "invalid_request":
        return "节点拒绝了请求格式。请刷新页面后重新开始。";
    }
  }
  if (error instanceof PairingProtocolError) {
    switch (error.code) {
      case "invalid_pairing_code":
        return "配对码格式不正确。请输入 8 位 Crockford 字符，可带一个中间连字符。";
      case "invalid_display_name":
        return "名称需要 1 到 80 个 Unicode 字符，且不能包含控制字符。";
      case "origin_mismatch":
        return "节点声明的来源与当前 HTTPS 页面不一致，已停止。";
      case "node_mismatch":
        return "节点公钥与此前固定的公钥不一致。不会自动信任替换后的节点。";
      case "host_proof_invalid":
        return "节点返回的签名证明验证失败，未更新本地状态。";
      case "crypto_unavailable":
        return "当前浏览器没有可用的 WebCrypto，无法安全配对。";
      case "invalid_key":
      case "invalid_response":
        return "节点返回的数据不符合接入合同，已停止处理。";
    }
  }
  if (error instanceof IdentityStoreError) {
    switch (error.code) {
      case "identity_changed":
        return "其他页面已更新浏览器身份，正在重新读取当前身份。";
      case "key_unreadable":
      case "key_self_check_failed":
      case "corrupt_record":
        return "浏览器保存的密钥无法读回或签验失败。本页没有生成替代密钥；请先在节点撤销旧设备，再清除本站点数据后重新配对。";
      case "storage_unavailable":
        return "同源 IndexedDB 不可用，无法保存或读取浏览器身份，因此不会发起认领。";
      case "write_failed":
        return "浏览器身份无法写入 IndexedDB，认领未继续。";
      case "origin_mismatch":
        return "保存身份时使用的 HTTPS 来源与当前页面不一致，不会迁移或替换密钥。";
      case "crypto_unavailable":
        return "当前浏览器没有可用的 WebCrypto。";
    }
  }
  if (error instanceof PairingSessionError) {
    switch (error.code) {
      case "already_bound":
        return "这个浏览器身份已经绑定到节点，不能重复认领。";
      case "node_mismatch":
        return "节点公钥与此前固定的公钥不一致，已拒绝继续。";
      case "client_mismatch":
        return "节点返回的客户端 ID 与本地固定身份不一致，已停止管理认证。";
      case "no_pending_claim":
        return "当前页面没有可重试的认领证明，请重新输入配对码。";
      case "pairing_not_pending":
        return "这次配对不在等待批准的状态。";
      case "crypto_unavailable":
        return "当前浏览器没有可用的 WebCrypto。";
    }
  }
  return "配对未完成。请检查节点状态后重试。";
}

function managementErrorMessage(error: unknown): string {
  if (error instanceof ManagementRequestError) {
    return managementRequestErrorMessage(error.code);
  }
  if (error instanceof ManagementConnectionError) {
    switch (error.code) {
      case "not_connected":
        return "管理连接尚未完成身份认证。";
      case "request_timeout":
        return "管理请求超时，结果未确认。重新连接后可读取列表核实。";
      case "revoked":
        return "节点已撤销这个客户端身份并关闭连接。";
      case "connection_closed":
        switch (error.closeCode) {
          case 1000:
            return "管理连接已断开。";
          case 1001:
            return "节点正在停止，管理连接已关闭。";
          case 4400:
            return "管理帧格式或协议版本不一致，连接已关闭。";
          case 4401:
            return "节点不认可这个客户端身份。它可能已被撤销，或本地身份记录已经失效。";
          case 4408:
            return "管理身份认证超时，连接已关闭。";
          case 4409:
          case 4429:
            return "管理连接或请求达到节点上限，请稍后重新连接。";
          case 4500:
            return "节点内部存储不可用，管理连接已关闭。";
          default:
            return "管理连接已关闭，未确认最后一条操作结果。";
        }
    }
  }
  if (error instanceof ManagementProtocolError) {
    switch (error.code) {
      case "origin_mismatch":
        return "节点声明的来源与当前 HTTPS 页面不一致，已停止认证。";
      case "node_mismatch":
        return "管理挑战中的节点公钥与已绑定节点不一致，已拒绝签署证明。";
      case "host_proof_invalid":
        return "节点挑战签名或挑战字段验证失败，未发送客户端证明。";
      case "invalid_key":
        return "管理挑战包含无效的节点公钥，已停止认证。";
      case "invalid_frame":
      case "invalid_response":
        return "节点返回的管理数据不符合已校验协议，连接已停止。";
    }
  }
  if (error instanceof SessionConsumeOutcomeError) {
    return error.code === "consume_pending"
      ? "请求仍在节点确认中，尚未完成结算；请刷新队列或重试。"
      : `请求被节点拒绝（${error.detail}）。`;
  }
  return errorMessage(error);
}

function delay(milliseconds: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) {
      resolve();
      return;
    }
    const timeout = window.setTimeout(finish, milliseconds);
    function finish(): void {
      window.clearTimeout(timeout);
      signal.removeEventListener("abort", finish);
      resolve();
    }
    signal.addEventListener("abort", finish, { once: true });
  });
}

function formatHex(value: string): string {
  return value.match(/.{1,8}/gu)?.join(" ") ?? value;
}

function eyebrow(text: string): HTMLElement {
  return element("p", "panel-eyebrow", text);
}

function title(text: string): HTMLElement {
  const heading = element("h2", undefined, text);
  heading.id = "panel-title";
  return heading;
}

function paragraph(text: string, className?: string): HTMLElement {
  return element("p", className, text);
}

function statusLine(
  text: string,
  tone: "working" | "notice" | "error",
): HTMLElement {
  const line = element("p", `status-line status-${tone}`);
  line.setAttribute("role", tone === "error" ? "alert" : "status");
  const marker = element("span", "status-marker");
  marker.setAttribute("aria-hidden", "true");
  line.append(marker, document.createTextNode(text));
  return line;
}

function detailBlock(label: string, value: string): HTMLElement {
  const block = element("div", "detail-block");
  block.append(
    element("span", "detail-label", label),
    element("code", undefined, value),
  );
  return block;
}

function button(text: string, className?: string): HTMLButtonElement {
  const value = element("button", className, text) as HTMLButtonElement;
  value.type = "button";
  return value;
}

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const value = document.createElement(tag);
  if (className !== undefined) {
    value.className = className;
  }
  if (text !== undefined) {
    value.textContent = text;
  }
  return value;
}

function requiredElement<T extends Element>(
  id: string,
  constructor: { new (): T },
): T {
  const value = document.getElementById(id);
  if (!(value instanceof constructor)) {
    throw new Error(`required element ${id} is missing`);
  }
  return value;
}
