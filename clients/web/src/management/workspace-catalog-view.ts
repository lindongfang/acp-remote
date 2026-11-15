import {
  type BindTargetInitialDocumentInput,
  type CatalogAdditionalDirectoryInput,
  type CatalogLaunchEnvInput,
  type InstallHarnessDocumentInput,
} from "./client";
import {
  requireSessionId,
  requireSessionU64,
  SessionProtocolError,
  type CatalogInventory,
  type ProductTargetRecord,
  type SourceLaunchSelectionInput,
  type WorkspaceRecord,
} from "./session-protocol";

export interface CatalogInstallDraft {
  readonly harnessName: string;
  readonly sourceMode: "new" | "existing";
  readonly sourceName: string;
  readonly existingSourceId: string;
  readonly executable: string;
  readonly argsText: string;
  readonly envText: string;
  readonly wire: "2" | "1";
  readonly additionalDirsText: string;
}

export interface CatalogTargetDraft {
  readonly sourceId: string;
  readonly nativeSessionId: string;
  readonly workspaceId: string;
  readonly targetId: string;
  readonly harnessId: string;
  readonly profileId: string;
  readonly profileRevision: string;
  readonly launchPath: string;
}

export interface WorkspaceCatalogPanelState {
  readonly connected: boolean;
  readonly inventory: CatalogInventory | null;
  readonly workspacePathDraft: string;
  readonly installDraft: CatalogInstallDraft;
  readonly targetDraft: CatalogTargetDraft;
}

export interface WorkspaceCatalogPanelCallbacks {
  readonly onRefreshInventory: (button: HTMLButtonElement) => void;
  readonly onWorkspacePathChange: (value: string) => void;
  readonly onRegisterWorkspace: (button: HTMLButtonElement) => void;
  readonly onInstallFieldChange: <K extends keyof CatalogInstallDraft>(
    field: K,
    value: CatalogInstallDraft[K],
  ) => void;
  readonly onInstallHarness: (button: HTMLButtonElement) => void;
  readonly onTargetFieldChange: <K extends keyof CatalogTargetDraft>(
    field: K,
    value: CatalogTargetDraft[K],
  ) => void;
  readonly onRegisterTarget: (button: HTMLButtonElement) => void;
  readonly onRegisterAndBindTarget: (button: HTMLButtonElement) => void;
  readonly onBindTarget: (button: HTMLButtonElement) => void;
  readonly onUseWorkspace: (workspace: WorkspaceRecord) => void;
  readonly onUseHarnessProfile: (
    harnessId: string,
    sourceId: string,
    profileId: string,
    profileRevision: string,
  ) => void;
  readonly onSelectTargetQueue: (targetId: string) => void;
  readonly onSelectTargetRelease: (targetId: string) => void;
  readonly onSelectTargetForBind: (target: ProductTargetRecord) => void;
  readonly onFillSourceSelection: (selection: SourceLaunchSelectionInput, nativeSessionId: string) => void;
}

export class CatalogDraftValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CatalogDraftValidationError";
  }
}

export function createInitialInstallDraft(): CatalogInstallDraft {
  return {
    harnessName: "Codex ACP",
    sourceMode: "new",
    sourceName: "Codex Source",
    existingSourceId: "",
    executable: "",
    argsText: "",
    envText: "",
    wire: "2",
    additionalDirsText: "",
  };
}

export function createInitialTargetDraft(): CatalogTargetDraft {
  return {
    sourceId: "",
    nativeSessionId: "",
    workspaceId: "",
    targetId: "",
    harnessId: "",
    profileId: "",
    profileRevision: "",
    launchPath: "",
  };
}

export function parseLaunchArgsLines(argsText: string): string[] {
  if (argsText.trim() === "") return [];
  return argsText
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export function parseLaunchEnvLines(envText: string): CatalogLaunchEnvInput[] {
  const result: CatalogLaunchEnvInput[] = [];
  for (const rawLine of envText.split(/\r?\n/u)) {
    const line = rawLine.trim();
    if (line.length === 0) continue;
    const eq = line.indexOf("=");
    if (eq <= 0) {
      throw new CatalogDraftValidationError("环境变量格式无效，请使用每行 NAME=VALUE。");
    }
    const name = line.slice(0, eq).trim();
    const value = line.slice(eq + 1);
    if (name.length === 0 || name.includes("\0") || value.includes("\0")) {
      throw new CatalogDraftValidationError("环境变量名称不能为空，且不能包含 NUL 字符。");
    }
    result.push({ name, value });
  }
  return result;
}

export function parseAdditionalDirectoriesLines(
  text: string,
  inventory: CatalogInventory | null,
): CatalogAdditionalDirectoryInput[] {
  const result: CatalogAdditionalDirectoryInput[] = [];
  for (const rawLine of text.split(/\r?\n/u)) {
    const line = rawLine.trim();
    if (line.length === 0) continue;
    const eq = line.indexOf("=");
    if (eq >= 0) {
      const workspaceId = validateId(line.slice(0, eq).trim(), "附加目录 workspaceId 无效。");
      const path = line.slice(eq + 1).trim();
      if (path.length === 0 || path.includes("\0")) {
        throw new CatalogDraftValidationError("附加目录路径不能为空。");
      }
      result.push({ workspaceId, path });
    } else {
      const workspaceId = validateId(line, "附加目录 workspaceId 无效。");
      const found = inventory?.workspaces.find((ws) => ws.workspaceId === workspaceId);
      if (found === undefined) {
        throw new CatalogDraftValidationError(
          `附加目录 ${workspaceId} 未在当前目录清单中找到；请使用 workspaceId=canonicalPath 格式或先刷新清单。`,
        );
      }
      result.push({ workspaceId, path: found.canonicalPath });
    }
  }
  return result;
}

export function validateInstallHarnessInput(
  draft: CatalogInstallDraft,
  inventory: CatalogInventory | null,
): InstallHarnessDocumentInput {
  const harnessName = draft.harnessName.trim();
  if (harnessName.length === 0 || harnessName.includes("\0")) {
    throw new CatalogDraftValidationError("Harness 名称不能为空。");
  }
  const executable = draft.executable.trim();
  if (executable.length === 0 || executable.includes("\0")) {
    throw new CatalogDraftValidationError("启动可执行文件路径不能为空。");
  }
  const source =
    draft.sourceMode === "existing"
      ? {
          kind: "existing" as const,
          sourceId: validateId(draft.existingSourceId.trim(), "Existing Source ID 格式无效。"),
        }
      : (() => {
          const name = draft.sourceName.trim();
          if (name.length === 0 || name.includes("\0")) {
            throw new CatalogDraftValidationError("新 Source 名称不能为空。");
          }
          return { kind: "new" as const, name };
        })();

  const args = parseLaunchArgsLines(draft.argsText);
  const environment = parseLaunchEnvLines(draft.envText);
  const additionalDirectories = parseAdditionalDirectoriesLines(
    draft.additionalDirsText,
    inventory,
  );
  const wire = draft.wire === "1" ? 1 : 2;
  return {
    name: harnessName,
    source,
    executable,
    args,
    environment,
    recipes: [
      {
        wire,
        mcpServers: [],
        additionalDirectories,
        extra: {},
      },
    ],
  };
}

export function validateBindTargetInput(
  draft: CatalogTargetDraft,
  fallbackTargetId?: string,
  fallbackLaunchPath?: string,
): BindTargetInitialDocumentInput {
  const targetId = validateId(
    (fallbackTargetId ?? draft.targetId).trim(),
    "绑定所需的 Target ID 无效。",
  );
  const sourceId = validateId(draft.sourceId.trim(), "Source ID 格式无效。");
  const harnessId = validateId(draft.harnessId.trim(), "Harness ID 格式无效。");
  const profileId = validateId(draft.profileId.trim(), "Profile ID 格式无效。");
  let profileRevision: string;
  try {
    profileRevision = requireSessionU64(draft.profileRevision.trim());
    if (profileRevision === "0") {
      throw new SessionProtocolError("invalid_decimal");
    }
  } catch {
    throw new CatalogDraftValidationError("Profile Revision 必须为正整数。");
  }
  const workspaceId = validateId(draft.workspaceId.trim(), "Workspace ID 格式无效。");
  const launchPath = (draft.launchPath.trim() !== ""
    ? draft.launchPath.trim()
    : (fallbackLaunchPath ?? "").trim());
  if (launchPath.length === 0 || launchPath.includes("\0")) {
    throw new CatalogDraftValidationError("Launch Path 不能为空（可留空并从目标 Workspace 自动填充）。");
  }
  return {
    targetId,
    sourceId,
    harnessId,
    profileId,
    profileRevision,
    workspaceId,
    launchPath,
  };
}

function validateId(raw: string, message: string): string {
  try {
    return requireSessionId(raw);
  } catch {
    throw new CatalogDraftValidationError(message);
  }
}

export function workspaceCatalogPanelView(
  owner: Document,
  state: WorkspaceCatalogPanelState,
  callbacks: WorkspaceCatalogPanelCallbacks,
): HTMLElement {
  const section = el(owner, "section", "session-composer");
  section.dataset.panel = "workspace-catalog";
  section.append(
    el(owner, "h3", "section-title", "工作目录与 Harness 配置"),
    el(
      owner,
      "p",
      "field-help",
      "通过节点真实目录协议登记工作目录、安装 Codex/ACP Harness 与 Launch Profile，并创建或绑定会话 Target。",
    ),
  );

  const inventoryActions = el(owner, "div", "action-row");
  const refreshInvBtn = btn(owner, "刷新目录清单", "secondary-button");
  refreshInvBtn.disabled = !state.connected;
  refreshInvBtn.addEventListener("click", () => {
    refreshInvBtn.disabled = true;
    callbacks.onRefreshInventory(refreshInvBtn);
  });
  inventoryActions.append(refreshInvBtn);
  section.append(inventoryActions);

  // 1. Register Workspace
  const wsForm = el(owner, "form", "pair-form");
  const wsLabel = el(owner, "label", undefined, "工作目录路径（绝对路径）");
  wsLabel.htmlFor = "catalog-workspace-path";
  const wsInput = el(owner, "input");
  wsInput.id = "catalog-workspace-path";
  wsInput.name = "catalog-workspace-path";
  wsInput.type = "text";
  wsInput.autocomplete = "off";
  wsInput.spellcheck = false;
  wsInput.required = true;
  wsInput.placeholder = "例如：D:\\Project\\my-workspace";
  wsInput.value = state.workspacePathDraft;
  wsInput.addEventListener("input", () => {
    callbacks.onWorkspacePathChange(wsInput.value);
  });
  const wsSubmit = btn(owner, "登记工作目录", "submit");
  wsSubmit.type = "submit";
  wsSubmit.disabled = !state.connected;
  wsForm.addEventListener("submit", (event) => {
    event.preventDefault();
    wsSubmit.disabled = true;
    callbacks.onRegisterWorkspace(wsSubmit);
  });
  wsForm.append(wsLabel, wsInput, wsSubmit);
  section.append(wsForm);

  // 2. Install Harness & Launch Profile
  const installBox = el(owner, "div", "session-composer");
  installBox.append(el(owner, "h4", undefined, "安装 Harness 与 Launch Profile"));
  const installForm = el(owner, "form", "pair-form");

  const harnessNameInput = textField(
    owner,
    installForm,
    "catalog-harness-name",
    "Harness 名称",
    "例如：Codex ACP",
    state.installDraft.harnessName,
    (value) => callbacks.onInstallFieldChange("harnessName", value),
  );
  harnessNameInput.required = true;

  const sourceModeLabel = el(owner, "label", undefined, "Source 来源模式");
  sourceModeLabel.htmlFor = "catalog-source-mode";
  const sourceModeSelect = owner.createElement("select");
  sourceModeSelect.id = "catalog-source-mode";
  sourceModeSelect.name = "catalog-source-mode";
  for (const [value, label] of [
    ["new", "创建新 Source"],
    ["existing", "复用已有 Source ID"],
  ] as const) {
    const option = owner.createElement("option");
    option.value = value;
    option.textContent = label;
    option.selected = state.installDraft.sourceMode === value;
    sourceModeSelect.append(option);
  }
  sourceModeSelect.addEventListener("change", () => {
    callbacks.onInstallFieldChange(
      "sourceMode",
      sourceModeSelect.value === "existing" ? "existing" : "new",
    );
  });
  installForm.append(sourceModeLabel, sourceModeSelect);

  if (state.installDraft.sourceMode === "existing") {
    textField(
      owner,
      installForm,
      "catalog-existing-source-id",
      "已有 Source ID",
      "22 位 canonical base64url ID",
      state.installDraft.existingSourceId,
      (value) => callbacks.onInstallFieldChange("existingSourceId", value),
    );
  } else {
    textField(
      owner,
      installForm,
      "catalog-source-name",
      "新 Source 名称",
      "例如：Codex Source",
      state.installDraft.sourceName,
      (value) => callbacks.onInstallFieldChange("sourceName", value),
    );
  }

  const exeInput = textField(
    owner,
    installForm,
    "catalog-launch-executable",
    "启动可执行文件（Executable）",
    "例如：D:\\Application\\Node\\nodejs\\node.exe",
    state.installDraft.executable,
    (value) => callbacks.onInstallFieldChange("executable", value),
  );
  exeInput.required = true;

  textAreaField(
    owner,
    installForm,
    "catalog-launch-args",
    "启动参数（每行一个，可选）",
    "例如：D:\\Project\\acp-remote\\testing\\probes\\codex-session-loop\\node_modules\\@agentclientprotocol\\codex-acp\\dist\\index.js",
    state.installDraft.argsText,
    2,
    (value) => callbacks.onInstallFieldChange("argsText", value),
  );

  textAreaField(
    owner,
    installForm,
    "catalog-launch-env",
    "环境变量 NAME=VALUE（每行一个，可选；不会在清单中回显）",
    "CODEX_HOME=D:\\path\\to\\codex-home\nNO_BROWSER=1",
    state.installDraft.envText,
    3,
    (value) => callbacks.onInstallFieldChange("envText", value),
  );

  const wireLabel = el(owner, "label", undefined, "ACP 协议版本 (recipe wire)");
  wireLabel.htmlFor = "catalog-recipe-wire";
  const wireSelect = owner.createElement("select");
  wireSelect.id = "catalog-recipe-wire";
  wireSelect.name = "catalog-recipe-wire";
  for (const [value, label] of [
    ["2", "ACP v2 (wire: 2)"],
    ["1", "ACP v1 (wire: 1)"],
  ] as const) {
    const option = owner.createElement("option");
    option.value = value;
    option.textContent = label;
    option.selected = state.installDraft.wire === value;
    wireSelect.append(option);
  }
  wireSelect.addEventListener("change", () => {
    callbacks.onInstallFieldChange(
      "wire",
      wireSelect.value === "1" ? "1" : "2",
    );
  });
  installForm.append(wireLabel, wireSelect);

  textAreaField(
    owner,
    installForm,
    "catalog-additional-dirs",
    "附加工作目录（每行 workspaceId 或 workspaceId=canonicalPath，可选）",
    "留空表示不额外授权目录",
    state.installDraft.additionalDirsText,
    2,
    (value) => callbacks.onInstallFieldChange("additionalDirsText", value),
  );

  const installBtn = btn(owner, "上传并安装 Harness", "submit");
  installBtn.type = "submit";
  installBtn.disabled = !state.connected;
  installForm.addEventListener("submit", (event) => {
    event.preventDefault();
    installBtn.disabled = true;
    callbacks.onInstallHarness(installBtn);
  });
  installForm.append(installBtn);
  installBox.append(installForm);
  section.append(installBox);

  // 3. Register / Bind Target
  const targetBox = el(owner, "div", "session-composer");
  targetBox.append(el(owner, "h4", undefined, "登记并绑定会话 Target"));
  const targetForm = el(owner, "form", "pair-form");

  textField(
    owner,
    targetForm,
    "catalog-target-source-id",
    "Source ID",
    "22 位 canonical base64url ID",
    state.targetDraft.sourceId,
    (value) => callbacks.onTargetFieldChange("sourceId", value),
  );
  textField(
    owner,
    targetForm,
    "catalog-target-native-id",
    "Native Session ID",
    "例如：codex-main-1 或 discover 返回的原生会话 ID",
    state.targetDraft.nativeSessionId,
    (value) => callbacks.onTargetFieldChange("nativeSessionId", value),
  );
  textField(
    owner,
    targetForm,
    "catalog-target-workspace-id",
    "Workspace ID",
    "22 位 canonical base64url ID",
    state.targetDraft.workspaceId,
    (value) => callbacks.onTargetFieldChange("workspaceId", value),
  );
  textField(
    owner,
    targetForm,
    "catalog-bind-harness-id",
    "Harness ID（绑定所需）",
    "22 位 canonical base64url ID",
    state.targetDraft.harnessId,
    (value) => callbacks.onTargetFieldChange("harnessId", value),
  );
  textField(
    owner,
    targetForm,
    "catalog-bind-profile-id",
    "Profile ID（绑定所需）",
    "22 位 canonical base64url ID",
    state.targetDraft.profileId,
    (value) => callbacks.onTargetFieldChange("profileId", value),
  );
  textField(
    owner,
    targetForm,
    "catalog-bind-profile-revision",
    "Profile Revision（绑定所需）",
    "正整数，例如：1",
    state.targetDraft.profileRevision,
    (value) => callbacks.onTargetFieldChange("profileRevision", value),
  );
  textField(
    owner,
    targetForm,
    "catalog-bind-launch-path",
    "Launch Path（绑定所需；留空则使用所选 Workspace 规范路径）",
    "工作目录内的规范路径",
    state.targetDraft.launchPath,
    (value) => callbacks.onTargetFieldChange("launchPath", value),
  );
  textField(
    owner,
    targetForm,
    "catalog-bind-target-id",
    "Target ID（仅用于绑定已有 Target）",
    "仅绑定已有未绑定 Target 时填写",
    state.targetDraft.targetId,
    (value) => callbacks.onTargetFieldChange("targetId", value),
  );

  const targetRow = el(owner, "div", "action-row");
  const regBindBtn = btn(owner, "登记并绑定 Target", "submit");
  regBindBtn.type = "submit";
  regBindBtn.disabled = !state.connected;
  targetForm.addEventListener("submit", (event) => {
    event.preventDefault();
    regBindBtn.disabled = true;
    callbacks.onRegisterAndBindTarget(regBindBtn);
  });

  const regOnlyBtn = btn(owner, "仅登记 Target", "secondary-button");
  regOnlyBtn.disabled = !state.connected;
  regOnlyBtn.addEventListener("click", () => {
    regOnlyBtn.disabled = true;
    callbacks.onRegisterTarget(regOnlyBtn);
  });

  const bindOnlyBtn = btn(owner, "绑定已有 Target", "secondary-button");
  bindOnlyBtn.disabled = !state.connected;
  bindOnlyBtn.addEventListener("click", () => {
    bindOnlyBtn.disabled = true;
    callbacks.onBindTarget(bindOnlyBtn);
  });

  targetRow.append(regBindBtn, regOnlyBtn, bindOnlyBtn);
  targetForm.append(targetRow);
  targetBox.append(targetForm);
  section.append(targetBox);

  // 4. Current Catalog Inventory
  const invBox = el(owner, "div", "session-composer");
  invBox.append(el(owner, "h4", undefined, "当前目录清单"));
  const inv = state.inventory;
  if (inv === null) {
    invBox.append(el(owner, "p", "empty-state", "尚未读取目录清单，或正在读取…"));
    section.append(invBox);
    return section;
  }

  invBox.append(
    detail(
      owner,
      "统计",
      `Workspaces ${inv.workspaces.length} · Sources ${inv.sources.length} · Harnesses ${inv.harnesses.length} · Profiles ${inv.profiles.length} · Targets ${inv.targets.length}`,
    ),
  );

  if (inv.workspaces.length > 0) {
    invBox.append(el(owner, "p", "field-help", "已登记工作目录："));
    for (const ws of inv.workspaces) {
      const card = el(owner, "article", "device-card");
      card.append(
        detail(owner, "Workspace ID", ws.workspaceId),
        detail(owner, "Canonical Path", ws.canonicalPath),
      );
      const useBtn = btn(owner, "填入 Target / Source 表单", "secondary-button");
      useBtn.addEventListener("click", () => callbacks.onUseWorkspace(ws));
      card.append(useBtn);
      invBox.append(card);
    }
  }

  if (inv.profiles.length > 0) {
    invBox.append(el(owner, "p", "field-help", "已安装 Harness 与 Profile："));
    for (const profile of inv.profiles) {
      const harness = inv.harnesses.find((h) => h.harnessId === profile.harnessId);
      const source = inv.sources.find((s) => s.sourceId === profile.sourceId);
      const card = el(owner, "article", "device-card");
      card.append(
        detail(
          owner,
          "Harness / Source",
          `${harness?.name ?? profile.harnessId} / ${source?.name ?? profile.sourceId}`,
        ),
        detail(owner, "Source ID", profile.sourceId),
        detail(owner, "Harness ID", profile.harnessId),
        detail(owner, "Profile", `${profile.profileId} (rev ${profile.revision})`),
      );
      const useBtn = btn(owner, "填入 Harness/Profile", "secondary-button");
      useBtn.addEventListener("click", () => {
        callbacks.onUseHarnessProfile(
          profile.harnessId,
          profile.sourceId,
          profile.profileId,
          profile.revision,
        );
      });
      card.append(useBtn);
      invBox.append(card);
    }
  }

  if (inv.targets.length > 0) {
    invBox.append(el(owner, "p", "field-help", "已登记会话 Target："));
    for (const target of inv.targets) {
      const card = el(owner, "article", "device-card");
      card.append(
        detail(owner, "Target ID", target.targetId),
        detail(owner, "Source ID", target.sourceId),
        detail(owner, "Native Session ID", target.nativeSessionId),
      );
      if (target.workspaceId !== null) {
        card.append(detail(owner, "Workspace ID", target.workspaceId));
      }
      if (target.binding !== null) {
        card.append(
          detail(
            owner,
            "绑定配置",
            `profile ${target.binding.profileId} rev ${target.binding.profileRevision} @ ${target.binding.launchPath}`,
          ),
        );
      } else {
        card.append(detail(owner, "绑定配置", "未绑定"));
      }
      const row = el(owner, "div", "action-row");
      const queueBtn = btn(owner, "载入队列", "secondary-button");
      queueBtn.addEventListener("click", () => callbacks.onSelectTargetQueue(target.targetId));
      const releaseBtn = btn(owner, "填入原生释放", "secondary-button");
      releaseBtn.addEventListener("click", () => callbacks.onSelectTargetRelease(target.targetId));
      row.append(queueBtn, releaseBtn);
      if (target.binding === null) {
        const bindBtn = btn(owner, "填入绑定 ID", "secondary-button");
        bindBtn.addEventListener("click", () => callbacks.onSelectTargetForBind(target));
        row.append(bindBtn);
      } else {
        const profileHarnessId =
          inv.profiles.find(
            (p) =>
              p.profileId === target.binding!.profileId &&
              p.revision === target.binding!.profileRevision,
          )?.harnessId ??
          inv.harnesses.find((h) => h.sourceId === target.sourceId)?.harnessId;
        if (profileHarnessId !== undefined) {
          const fillSourceBtn = btn(owner, "填入来源面板", "secondary-button");
          fillSourceBtn.addEventListener("click", () => {
            callbacks.onFillSourceSelection(
              {
                sourceId: target.sourceId,
                harnessId: profileHarnessId,
                profileId: target.binding!.profileId,
                profileRevision: target.binding!.profileRevision,
                workspaceId: target.binding!.workspaceId,
                launchPath: target.binding!.launchPath,
              },
              target.nativeSessionId,
            );
          });
          row.append(fillSourceBtn);
        }
      }
      card.append(row);
      invBox.append(card);
    }
  }

  section.append(invBox);
  return section;
}

function textField(
  owner: Document,
  form: HTMLElement,
  id: string,
  labelText: string,
  placeholder: string,
  value: string,
  onChange: (value: string) => void,
): HTMLInputElement {
  const label = el(owner, "label", undefined, labelText);
  label.htmlFor = id;
  const input = el(owner, "input");
  input.id = id;
  input.name = id;
  input.type = "text";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.placeholder = placeholder;
  input.value = value;
  input.addEventListener("input", () => onChange(input.value));
  form.append(label, input);
  return input;
}

function textAreaField(
  owner: Document,
  form: HTMLElement,
  id: string,
  labelText: string,
  placeholder: string,
  value: string,
  rows: number,
  onChange: (value: string) => void,
): HTMLTextAreaElement {
  const label = el(owner, "label", undefined, labelText);
  label.htmlFor = id;
  const textarea = el(owner, "textarea");
  textarea.id = id;
  textarea.name = id;
  textarea.rows = rows;
  textarea.spellcheck = false;
  textarea.placeholder = placeholder;
  textarea.value = value;
  textarea.addEventListener("input", () => onChange(textarea.value));
  form.append(label, textarea);
  return textarea;
}

function detail(owner: Document, label: string, value: string): HTMLElement {
  const block = el(owner, "div", "detail-block");
  block.append(el(owner, "span", "detail-label", label), el(owner, "code", undefined, value));
  return block;
}

function btn(owner: Document, text: string, className?: string): HTMLButtonElement {
  const value = el(owner, "button", className, text);
  value.type = "button";
  return value;
}

function el<K extends keyof HTMLElementTagNameMap>(
  owner: Document,
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const value = owner.createElement(tag);
  if (className !== undefined) value.className = className;
  if (text !== undefined) value.textContent = text;
  return value;
}
