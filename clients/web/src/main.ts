import "./styles.css";

import {
  ManagementConnection,
  ManagementConnectionError,
  ManagementRequestError,
} from "./management/client";
import {
  type DevicePage,
  type DeviceSummary,
  type NodeInfo,
  ManagementProtocolError,
} from "./management/protocol";
import { ManagementReconnect } from "./management/reconnect";
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
  </main>
`;

const panel = requiredElement("access-panel", HTMLElement);
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
function runIdentityOperation<T>(operation: () => Promise<T>): Promise<T> {
  const result = identityWork.then(operation);
  identityWork = result.catch(() => undefined);
  return result;
}
const managementReconnect = new ManagementReconnect(connectSavedManagement);

window.addEventListener("pagehide", () => {
  pageActive = false;
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
  nextDeviceCursor = null;
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
  managementReconnect.cancel();
  automaticReconnectAllowed = false;
  ++managementGeneration;
  management = null;
  void resetLocalIdentity(record);
}

function recoverChangedIdentity(error: unknown): boolean {
  if (!(error instanceof IdentityStoreError) || error.code !== "identity_changed") return false;
  controller?.abort();
  managementReconnect.cancel();
  management?.disconnect();
  management = null;
  ++managementGeneration;
  session = null;
  currentRecord = null;
  automaticReconnectAllowed = true;
  pendingRecoveryAttempted = false;
  if (pageActive && !manuallyDisconnected) void boot();
  return true;
}

async function resetLocalIdentity(rejected: IdentityRecord): Promise<void> {
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
    switch (error.code) {
      case "invalid_request":
        return "节点拒绝了管理请求格式，没有提交新的管理结果。";
      case "not_found":
        return "目标客户端不存在，列表可能已经变化。";
      case "request_conflict":
        return "同一管理请求标识对应了不同内容，节点已拒绝。";
      case "stale_cursor":
        return "分页游标已失效，已停止沿用旧列表。请从第一页重新读取。";
      case "rate_limited":
        return "管理请求过于频繁，节点已限制本次操作。";
      case "storage_unavailable":
        return "节点存储暂时不可用，未确认撤销或列表结果。";
      case "node_stopping":
        return "节点正在停止，不再受理新的管理操作。";
    }
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
