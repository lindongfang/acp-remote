import { PairingApi, PairingApiError } from "../pairing/api";
import {
  IdentityStoreError,
  IdentityVault,
  IndexedDbIdentityRepository,
  type IdentityRecord,
} from "../pairing/identity";
import {
  PairingSession,
  isFailedPairingStatus,
  shouldRecoverPendingManagement,
} from "../pairing/session";
import type { PairStatusResult } from "../protocol/pairing";
import {
  button,
  delay,
  detailBlock,
  element,
  eyebrow,
  formatHex,
  paragraph,
  statusLine,
  title,
} from "./dom-primitives";
import {
  managementErrorMessage,
  pairingErrorMessage,
} from "./error-formatting";

const POLL_INTERVAL_MS = 3_000;

export interface PairingAccessHostPort {
  readonly getPanel: () => HTMLElement;
  readonly getDocument?: () => Document;
  readonly getLocation?: () => Pick<Location, "protocol" | "origin" | "host">;
  readonly isPageActive: () => boolean;
  readonly isManuallyDisconnected: () => boolean;
  readonly setManuallyDisconnected: (value: boolean) => void;
  readonly isAutomaticReconnectAllowed: () => boolean;
  readonly setAutomaticReconnectAllowed: (value: boolean) => void;
  readonly getLastManagementError: () => unknown;
  readonly setCurrentRecord: (record: IdentityRecord | null) => void;
  readonly showManagementConnecting: (record: IdentityRecord) => void;
  readonly resumeManagement: () => void;
  readonly openManagement: (
    record: IdentityRecord,
    recoveringPending?: boolean,
  ) => Promise<void>;
  readonly onResetManagementAndSessionState: (resetReconnect: boolean) => void;
  readonly createVault?: () => IdentityVault;
  readonly createPairingApi?: () => PairingApi;
  readonly pollIntervalMs?: number;
}

export class PairingAccessController {
  readonly #port: PairingAccessHostPort;

  #vault: IdentityVault | null = null;
  #session: PairingSession | null = null;
  #controller: AbortController | null = null;
  #identityController = new AbortController();
  #identityWork: Promise<unknown> = Promise.resolve();
  #identityResetting = false;
  #pendingRecoveryAttempted = false;

  constructor(port: PairingAccessHostPort) {
    this.#port = port;
  }

  get vault(): IdentityVault | null {
    return this.#vault;
  }

  get session(): PairingSession | null {
    return this.#session;
  }

  get identityResetting(): boolean {
    return this.#identityResetting;
  }

  get isIdentityAborted(): boolean {
    return this.#identityController.signal.aborted;
  }

  #doc(): Document {
    return this.#port.getDocument?.() ?? globalThis.document;
  }

  #location(): Pick<Location, "protocol" | "origin" | "host"> {
    return this.#port.getLocation?.() ?? globalThis.location;
  }

  runIdentityOperation<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.#identityWork.then(operation);
    this.#identityWork = result.catch(() => undefined);
    return result;
  }

  abortPairing(): void {
    this.#controller?.abort();
    this.#controller = null;
  }

  abortForPageHide(): void {
    this.#identityController.abort();
    this.#controller?.abort();
  }

  async boot(restoringPage = false): Promise<void> {
    if (!this.#port.isPageActive()) return;
    this.#identityController.abort();
    this.#identityController = new AbortController();
    const signal = this.#identityController.signal;
    const loc = this.#location();
    if (loc.protocol !== "https:") {
      this.showFatal(
        "需要通过节点的 HTTPS 地址打开",
        "当前页面不是安全来源。请从节点终端显示的 HTTPS 地址进入，本页不会在明文来源创建身份。",
      );
      return;
    }

    this.showLoading();
    try {
      this.#vault ??=
        this.#port.createVault?.() ??
        new IdentityVault(new IndexedDbIdentityRepository());
      const localVault = this.#vault;
      const record = await this.runIdentityOperation(() =>
        localVault.initialize(loc.origin, signal),
      );
      if (signal.aborted || !this.#port.isPageActive()) return;
      const api = this.#port.createPairingApi?.() ?? new PairingApi(loc.origin);
      this.#session = new PairingSession(this.#vault, api, record);
    } catch (error) {
      if (signal.aborted || !this.#port.isPageActive()) return;
      this.showFatal("无法读取浏览器身份", pairingErrorMessage(error));
      return;
    }

    const record = this.#session.identity;
    if (record.status === "pending_claim") {
      this.showPairForm(
        record,
        this.#session.canRetryClaim
          ? "已从本机 IndexedDB 恢复上次未完成的一次性认领证明。可逐字重试，不会重新生成公钥或挑战。"
          : undefined,
      );
      return;
    }
    if (record.status === "pending_confirmation") {
      this.showPending(record);
      void this.pollApproval();
      return;
    }
    if (record.status === "approved" || record.status === "consumed") {
      if (restoringPage) {
        this.#port.setCurrentRecord(record);
        if (!this.#port.isAutomaticReconnectAllowed()) {
          this.showFatal(
            "管理连接已停止",
            managementErrorMessage(this.#port.getLastManagementError()),
          );
          return;
        }
        this.#port.showManagementConnecting(record);
        this.#port.resumeManagement();
        return;
      }
      void this.#port.openManagement(record);
      return;
    }
    this.showPairForm(record);
  }

  showLoading(): void {
    const doc = this.#doc();
    this.#port
      .getPanel()
      .replaceChildren(
        eyebrow("LOCAL IDENTITY", doc),
        title("正在检查浏览器身份", doc),
        paragraph(
          "密钥只保存在本站同源 IndexedDB 中，并会在认领前完成读回签验。",
          undefined,
          doc,
        ),
        statusLine("正在初始化不可导出 P-256 密钥…", "working", doc),
      );
  }

  showFatal(heading: string, detail: string): void {
    const doc = this.#doc();
    this.#port
      .getPanel()
      .replaceChildren(
        eyebrow("STOPPED", doc),
        title(heading, doc),
        paragraph(detail, undefined, doc),
        statusLine("为保护已保存身份，本页不会自动生成替代密钥。", "error", doc),
      );
  }

  showPairForm(record: IdentityRecord, notice?: string, failure?: string): void {
    const doc = this.#doc();
    const loc = this.#location();
    const fragment = doc.createDocumentFragment();
    const origin = element("div", "origin-pill", undefined, doc);
    origin.append(
      element("span", "origin-dot", undefined, doc),
      element("span", "origin-host", loc.host, doc),
    );
    fragment.append(
      origin,
      title("配对到此节点", doc),
      paragraph(
        "在节点终端运行 acp-remote device pair，输入配对码后，在终端核对并批准。",
        undefined,
        doc,
      ),
    );
    if (notice !== undefined) fragment.append(statusLine(notice, "notice", doc));
    if (failure !== undefined) fragment.append(statusLine(failure, "error", doc));

    if (this.#session?.canRetryClaim === true) {
      const retry = button("重新提交同一认领", undefined, doc);
      retry.addEventListener("click", () => void this.retryPendingClaim(retry));
      fragment.append(retry);
    }

    const form = element("form", "pair-form", undefined, doc);
    const codeLabel = element("label", undefined, "配对码", doc);
    codeLabel.htmlFor = "pairing-code";
    const codeInput = element("input", undefined, undefined, doc) as HTMLInputElement;
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

    const nameLabel = element("label", undefined, "这台浏览器如何显示", doc);
    nameLabel.htmlFor = "display-name";
    const nameInput = element("input", undefined, undefined, doc) as HTMLInputElement;
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
      doc,
    );
    const submit = button("认领这台浏览器", "submit", doc);
    submit.type = "submit";
    form.append(codeLabel, codeInput, nameLabel, nameInput, help, submit);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      const code = codeInput.value;
      const displayName = nameInput.value;
      codeInput.value = "";
      void this.submitPair(code, displayName);
    });
    fragment.append(form);
    this.#port.getPanel().replaceChildren(fragment);
    codeInput.focus();
  }

  async submitPair(code: string, displayName: string): Promise<void> {
    if (this.#session === null) return;
    this.#controller?.abort();
    this.#controller = new AbortController();
    this.showWorking();
    try {
      await this.#session.begin(code, displayName, this.#controller.signal);
    } catch (error) {
      if (this.#controller.signal.aborted) return;
      if (this.recoverChangedIdentity(error)) return;
      const notice =
        this.#session.identity.status === "pending_claim"
          ? "节点可能已经接收认领，但响应没有完整返回。可逐字重试同一份证明；不会重新认领。"
          : undefined;
      this.showPairForm(this.#session.identity, notice, pairingErrorMessage(error));
      return;
    }
    this.#pendingRecoveryAttempted = false;
    this.showPending(this.#session.identity);
    void this.pollApproval();
  }

  async retryPendingClaim(retryButton: HTMLButtonElement): Promise<void> {
    if (this.#session === null) return;
    this.#controller?.abort();
    this.#controller = new AbortController();
    retryButton.disabled = true;
    this.showWorking();
    try {
      await this.#session.retryClaim(this.#controller.signal);
    } catch (error) {
      if (this.#controller.signal.aborted) return;
      if (this.recoverChangedIdentity(error)) return;
      this.showPairForm(
        this.#session.identity,
        "节点可能已经接收认领，但响应没有完整返回。重试不会更换公钥或挑战。",
        pairingErrorMessage(error),
      );
      return;
    }
    this.#pendingRecoveryAttempted = false;
    this.showPending(this.#session.identity);
    void this.pollApproval();
  }

  async pollApproval(): Promise<void> {
    if (this.#session === null) return;
    this.#controller?.abort();
    this.#controller = new AbortController();
    const active = this.#controller;
    const pollMs = this.#port.pollIntervalMs ?? POLL_INTERVAL_MS;
    for (;;) {
      try {
        const result = await this.#session.pollOnce(active.signal);
        if (active.signal.aborted) return;
        if (!this.handleStatusResult(result)) return;
      } catch (error) {
        if (active.signal.aborted) return;
        if (this.recoverChangedIdentity(error)) return;
        if (error instanceof PairingApiError && error.code === "rate_limited") {
          const waitSeconds = Math.max(error.retryAfterSeconds ?? 3, 1);
          this.showPending(
            this.#session.identity,
            `查询过于频繁，将在 ${waitSeconds} 秒后继续检查。`,
          );
          await delay(waitSeconds * 1_000, active.signal);
          continue;
        }
        if (
          shouldRecoverPendingManagement(
            error,
            this.#session.identity,
            this.#pendingRecoveryAttempted,
          )
        ) {
          this.#pendingRecoveryAttempted = true;
          await this.recoverPendingManagement(this.#session.identity);
          return;
        }
        this.showPollingFailure(pairingErrorMessage(error));
        return;
      }
      await delay(pollMs, active.signal);
    }
  }

  handleStatusResult(result: PairStatusResult): boolean {
    if (result.status === "pending_confirmation") {
      this.showPending(this.#session?.identity ?? null, "仍在等待本机终端明确批准。");
      return true;
    }
    const record = this.#session?.identity ?? null;
    if (
      record !== null &&
      (record.status === "approved" || record.status === "consumed")
    ) {
      void this.#port.openManagement(record);
    } else if (record !== null) {
      this.showTerminal(record);
    }
    return false;
  }

  showWorking(): void {
    const doc = this.#doc();
    this.#port
      .getPanel()
      .replaceChildren(
        eyebrow("STEP 02 / VERIFY", doc),
        title("正在认领并验证节点", doc),
        paragraph(
          "浏览器会先验证节点签名、来源和公钥指纹。验证失败时不会显示待批准状态。",
          undefined,
          doc,
        ),
        statusLine("正在提交一次性的配对证明…", "working", doc),
      );
  }

  showPending(record: IdentityRecord | null, detail?: string): void {
    if (record === null) {
      this.showFatal("配对状态不可用", "没有找到当前浏览器身份。");
      return;
    }
    const doc = this.#doc();
    const fragment = doc.createDocumentFragment();
    fragment.append(
      eyebrow("STEP 03 / CONFIRM", doc),
      title("已认领，等待本机批准", doc),
      paragraph(
        detail ??
          "请在运行节点的终端核对节点指纹和客户端信息，再明确批准。直接回车会拒绝。",
        undefined,
        doc,
      ),
    );
    if (record.nodeId !== null) {
      fragment.append(detailBlock("节点 SHA-256 指纹", formatHex(record.nodeId), doc));
    }
    if (record.displayName !== null) {
      fragment.append(detailBlock("客户端名称", record.displayName, doc));
    }
    const clientFingerprint = detailBlock("客户端公钥指纹", "正在计算…", doc);
    fragment.append(
      clientFingerprint,
      statusLine("等待批准，不会跳过本机确认。", "working", doc),
    );
    this.#port.getPanel().replaceChildren(fragment);
    void this.fillFingerprint(clientFingerprint, record.publicKey);
  }

  showPollingFailure(detail: string): void {
    const doc = this.#doc();
    const retry = button("重新检查批准状态", undefined, doc);
    retry.addEventListener("click", () => void this.pollApproval());
    const actions = element("div", "action-row", undefined, doc);
    actions.append(retry);
    const record = this.#session?.identity ?? null;
    if (
      record !== null &&
      record.status === "pending_confirmation" &&
      record.nodeId !== null &&
      record.nodePublicKey !== null
    ) {
      const recover = button("验证已保存身份", "secondary-button", doc);
      recover.addEventListener("click", () => {
        this.#pendingRecoveryAttempted = true;
        void this.#port.openManagement(record, true);
      });
      actions.append(recover);
    }
    this.#port
      .getPanel()
      .replaceChildren(
        eyebrow("PAUSED", doc),
        title("暂时无法检查批准结果", doc),
        paragraph(detail, undefined, doc),
        actions,
      );
  }

  async recoverPendingManagement(record: IdentityRecord): Promise<void> {
    if (
      record.status !== "pending_confirmation" ||
      record.nodeId === null ||
      record.nodePublicKey === null
    ) {
      this.showPollingFailure("本地身份缺少节点绑定资料，无法执行公钥恢复。");
      return;
    }
    await this.#port.openManagement(record, true);
  }

  showTerminal(record: IdentityRecord): void {
    if (
      record.status === "approved" ||
      record.status === "consumed" ||
      record.clientId !== null
    ) {
      void this.#port.openManagement(record);
      return;
    }
    if (!isFailedPairingStatus(record.status)) {
      this.showPending(record);
      return;
    }
    const messages = {
      rejected: "本机已拒绝这次配对。",
      cancelled: "本机已取消这次配对。",
      expired: "配对已到期，尚未获得批准。",
    } as const;
    this.showPairForm(
      record,
      `${messages[record.status]} 可以重新运行 device pair，并使用同一浏览器身份再次认领。`,
    );
  }

  recoverChangedIdentity(error: unknown): boolean {
    if (!(error instanceof IdentityStoreError) || error.code !== "identity_changed") {
      return false;
    }
    this.#controller?.abort();
    this.#session = null;
    this.#pendingRecoveryAttempted = false;
    this.#port.setAutomaticReconnectAllowed(true);
    this.#port.onResetManagementAndSessionState(false);
    if (this.#port.isPageActive() && !this.#port.isManuallyDisconnected()) {
      void this.boot();
    }
    return true;
  }

  async resetLocalIdentity(rejected: IdentityRecord): Promise<void> {
    this.#port.onResetManagementAndSessionState(true);
    if (!this.#port.isPageActive() || this.#identityResetting) return;
    this.#identityResetting = true;
    this.#identityController.abort();
    this.#identityController = new AbortController();
    const signal = this.#identityController.signal;
    this.#session = null;
    this.showLoading();
    if (this.#vault === null) {
      this.#identityResetting = false;
      this.showFatal("无法清除本地身份", "当前页面没有可用的身份存储。");
      return;
    }
    this.#controller?.abort();
    try {
      const localVault = this.#vault;
      const loc = this.#location();
      const record = await this.runIdentityOperation(() =>
        localVault.replaceRejectedIdentity(rejected, signal),
      );
      if (signal.aborted || !this.#port.isPageActive()) return;
      const api = this.#port.createPairingApi?.() ?? new PairingApi(loc.origin);
      this.#session = new PairingSession(this.#vault, api, record);
      this.#port.setManuallyDisconnected(false);
      this.#pendingRecoveryAttempted = false;
      if (record.status === "unbound") {
        this.showPairForm(
          record,
          "原配对已失效，请输入新的配对码，并在节点终端批准。",
        );
      } else {
        this.#identityResetting = false;
        this.#port.setAutomaticReconnectAllowed(true);
        await this.boot();
      }
    } catch (error) {
      if (signal.aborted || !this.#port.isPageActive()) return;
      this.showFatal("无法准备重新配对", pairingErrorMessage(error));
    } finally {
      this.#identityResetting = false;
    }
  }

  identityFragment(record: IdentityRecord): DocumentFragment {
    const doc = this.#doc();
    const fragment = doc.createDocumentFragment();
    if (record.nodeId !== null) {
      fragment.append(detailBlock("节点 SHA-256 指纹", formatHex(record.nodeId), doc));
    }
    return fragment;
  }

  appendIdentityDetails(
    fragment: DocumentFragment,
    record: IdentityRecord,
    includeNode = true,
  ): void {
    const doc = this.#doc();
    if (includeNode && record.nodeId !== null) {
      fragment.append(detailBlock("节点 SHA-256 指纹", formatHex(record.nodeId), doc));
    }
    if (record.displayName !== null) {
      fragment.append(detailBlock("客户端名称", record.displayName, doc));
    }
    if (record.clientId !== null) {
      fragment.append(detailBlock("客户端 ID", record.clientId, doc));
    }
    const fingerprint = detailBlock("客户端公钥指纹", "正在计算…", doc);
    fragment.append(fingerprint);
    void this.fillFingerprint(fingerprint, record.publicKey);
  }

  async fillFingerprint(target: HTMLElement, publicKey: string): Promise<void> {
    if (this.#vault === null) {
      target.textContent = "不可用";
      return;
    }
    try {
      target.textContent = formatHex(await this.#vault.fingerprint(publicKey));
    } catch {
      target.textContent = "无法计算";
    }
  }
}
