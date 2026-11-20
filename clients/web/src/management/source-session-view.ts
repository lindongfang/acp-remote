import { randomId128 } from "../protocol/pairing";
import { createRawInspector, type RawInspector } from "../views/raw-inspector";
import { visibleRawText } from "../views/raw-inspector-data";
import {
  parseSessionCommand,
  SessionProtocolError,
  type SessionViewOpened,
  type SourceLaunchSelectionInput,
  type SourceOperationInput,
  type SourceSessionFact,
  type SourceSessionRegistration,
} from "./session-protocol";
import type { InspectedSourceSessionResult } from "./source-result-read";

export const SOURCE_SHARED_VIEW_STORAGE_KEY = "acp-remote.shared-source-view.v1";

export type PersistedSourceViewSpec = {
  readonly kind: "source";
  readonly selection: SourceLaunchSelectionInput;
  readonly nativeSessionId: string;
};

export interface SourceSessionPanelState {
  readonly connected: boolean;
  readonly selection: SourceLaunchSelectionInput;
  readonly cursor: string;
  readonly manualNativeSessionId: string;
  readonly registration: SourceSessionRegistration | null;
  readonly fact: SourceSessionFact | null;
  readonly result: InspectedSourceSessionResult | null;
  readonly rawExpanded: boolean;
  readonly activeSourceView: {
    readonly selection: SourceLaunchSelectionInput;
    readonly nativeSessionId: string;
  } | null;
  readonly openedView: SessionViewOpened | null;
  readonly staleNotice: string | null;
}

export interface SourceSessionPanelCallbacks {
  readonly onSelectionFieldChange: (
    field: keyof SourceLaunchSelectionInput,
    value: string,
  ) => void;
  readonly onCursorChange: (value: string) => void;
  readonly onManualNativeSessionIdChange: (value: string) => void;
  readonly onRegister: (
    kind: "discover" | "create",
    button: HTMLButtonElement,
  ) => void;
  readonly onExecute: (button: HTMLButtonElement) => void;
  readonly onRefreshStatus: (button: HTMLButtonElement) => void;
  readonly onReconcile: (button: HTMLButtonElement) => void;
  readonly onOpenResult: (button: HTMLButtonElement) => void;
  readonly onReleaseResult: (button: HTMLButtonElement) => void;
  readonly onClearOperation: () => void;
  readonly onUseNextCursor: (nextCursor: string) => void;
  readonly onOpenDiscoveredView: (
    nativeSessionId: string,
    button: HTMLButtonElement,
  ) => void;
  readonly onOpenManualView: (button: HTMLButtonElement) => void;
  readonly onRefreshSourceView: (button: HTMLButtonElement) => void;
  readonly onCloseSourceView: (button: HTMLButtonElement) => void;
  readonly onAbandonSourceView: () => void;
  readonly onToggleRawResult: (expanded: boolean) => void;
  readonly onRegisterInspector: (inspector: RawInspector) => void;
}

export interface SourceResultViewCallbacks {
  readonly rawExpanded: boolean;
  readonly onUseNextCursor: (nextCursor: string) => void;
  readonly onOpenView: (nativeSessionId: string, button: HTMLButtonElement) => void;
  readonly onToggleRaw: (expanded: boolean) => void;
  readonly onRegisterInspector: (inspector: RawInspector) => void;
}

export function validateSourceSelectionInput(
  draft: SourceLaunchSelectionInput,
): SourceLaunchSelectionInput {
  const command = parseSessionCommand(
    JSON.stringify({
      v: 1,
      type: "command",
      requestId: randomId128(),
      action: "view.open_source",
      payload: {
        selection: {
          sourceId: draft.sourceId.trim(),
          harnessId: draft.harnessId.trim(),
          profileId: draft.profileId.trim(),
          profileRevision: draft.profileRevision.trim(),
          workspaceId: draft.workspaceId.trim(),
          launchPath: draft.launchPath,
        },
        nativeSessionId: "probe",
        reason: "first_open",
      },
    }),
  );
  if (command.action !== "view.open_source") {
    throw new SessionProtocolError("invalid_command");
  }
  return command.payload.selection;
}

export function validateSourceOperationInput(
  selection: SourceLaunchSelectionInput,
  kind: "discover" | "create",
  rawCursor: string,
): SourceOperationInput {
  const operation: SourceOperationInput =
    kind === "discover"
      ? { kind: "discover", cursor: rawCursor === "" ? null : rawCursor }
      : { kind: "create" };
  const command = parseSessionCommand(
    JSON.stringify({
      v: 1,
      type: "command",
      requestId: randomId128(),
      action: "source.session.register",
      payload: { selection, operation },
    }),
  );
  if (command.action !== "source.session.register") {
    throw new SessionProtocolError("invalid_command");
  }
  return command.payload.operation;
}

export function validateSourceViewTargetInput(
  selection: SourceLaunchSelectionInput,
  nativeSessionId: string,
  reason: "first_open" | "explicit_refresh" | "reopen" | "reconnect" | "reacquire",
): { readonly selection: SourceLaunchSelectionInput; readonly nativeSessionId: string } {
  const command = parseSessionCommand(
    JSON.stringify({
      v: 1,
      type: "command",
      requestId: randomId128(),
      action: "view.open_source",
      payload: { selection, nativeSessionId, reason },
    }),
  );
  if (command.action !== "view.open_source") {
    throw new SessionProtocolError("invalid_command");
  }
  return {
    selection: command.payload.selection,
    nativeSessionId: command.payload.nativeSessionId,
  };
}

export function clearPersistedSourceViewSpec(
  storage: Pick<Storage, "removeItem"> | undefined = globalThis.sessionStorage,
): void {
  try {
    storage?.removeItem(SOURCE_SHARED_VIEW_STORAGE_KEY);
  } catch {
    // Ignore storage unavailability in restricted contexts.
  }
}

export function persistSourceViewSpec(
  selection: SourceLaunchSelectionInput,
  nativeSessionId: string,
  storage: Pick<Storage, "setItem"> | undefined = globalThis.sessionStorage,
): void {
  try {
    storage?.setItem(
      SOURCE_SHARED_VIEW_STORAGE_KEY,
      JSON.stringify({ selection, nativeSessionId }),
    );
  } catch {
    // Ignore storage unavailability in restricted contexts.
  }
}

export function loadPersistedSourceViewSpec(
  storage: Pick<Storage, "getItem" | "removeItem"> | undefined = globalThis.sessionStorage,
): PersistedSourceViewSpec | null {
  try {
    const raw = storage?.getItem(SOURCE_SHARED_VIEW_STORAGE_KEY);
    if (raw === null || raw === undefined) return null;
    const parsed = JSON.parse(raw) as { selection?: unknown; nativeSessionId?: unknown };
    const command = parseSessionCommand(
      JSON.stringify({
        v: 1,
        type: "command",
        requestId: randomId128(),
        action: "view.open_source",
        payload: {
          selection: parsed.selection,
          nativeSessionId: parsed.nativeSessionId,
          reason: "reconnect",
        },
      }),
    );
    if (command.action !== "view.open_source") return null;
    return {
      kind: "source",
      selection: command.payload.selection,
      nativeSessionId: command.payload.nativeSessionId,
    };
  } catch {
    clearPersistedSourceViewSpec(storage);
    return null;
  }
}

export function sourceSessionResultView(
  owner: Document,
  inspected: InspectedSourceSessionResult,
  callbacks: SourceResultViewCallbacks,
): HTMLElement {
  const container = el(owner, "div", "source-session-result");
  container.append(el(owner, "h4", undefined, "来源操作结果"));

  switch (inspected.kind) {
    case "sessions": {
      if (inspected.sessions.length === 0) {
        container.append(el(owner, "p", "empty-state", "来源未返回任何原生会话。"));
      } else {
        for (const session of inspected.sessions) {
          const card = el(owner, "article", "device-card");
          card.append(
            detail(owner, "原生会话 ID", visibleRawText(session.sessionId)),
          );
          if (session.title !== null) {
            card.append(detail(owner, "标题", visibleRawText(session.title)));
          }
          if (session.cwd !== null) {
            card.append(detail(owner, "工作目录", visibleRawText(session.cwd)));
          }
          if (session.updatedAt !== null) {
            card.append(detail(owner, "更新时间", visibleRawText(session.updatedAt)));
          }
          const openBtn = btn(owner, "打开只读视图", "secondary-button");
          openBtn.addEventListener("click", () => {
            openBtn.disabled = true;
            callbacks.onOpenView(session.sessionId, openBtn);
          });
          card.append(openBtn);
          container.append(card);
        }
      }
      if (inspected.nextCursor !== null) {
        const next = inspected.nextCursor;
        container.append(detail(owner, "下一页游标", visibleRawText(next)));
        const useNext = btn(owner, "使用下一页游标", "secondary-button");
        useNext.addEventListener("click", () => callbacks.onUseNextCursor(next));
        container.append(useNext);
      }
      break;
    }
    case "created": {
      container.append(
        detail(owner, "已创建原生会话 ID", visibleRawText(inspected.sessionId)),
      );
      const openBtn = btn(owner, "打开只读视图", "secondary-button");
      openBtn.addEventListener("click", () => {
        openBtn.disabled = true;
        callbacks.onOpenView(inspected.sessionId, openBtn);
      });
      container.append(openBtn);
      break;
    }
    case "remote_error": {
      container.append(
        status(
          owner,
          `来源远端错误 (${inspected.errorCode})：${visibleRawText(inspected.message)}`,
          "error",
        ),
      );
      break;
    }
    case "fallback": {
      container.append(status(owner, inspected.reason, "error"));
      const rawPre = el(owner, "pre", "body-preview");
      rawPre.setAttribute("aria-label", "来源结果只读安全文本");
      rawPre.textContent = inspected.safeRawText;
      container.append(rawPre);
      break;
    }
  }

  if (inspected.rawDocument !== null) {
    const rawDoc = inspected.rawDocument;
    const toggle = btn(owner, "查看结果原始结构", "secondary-button");
    toggle.setAttribute("aria-expanded", String(callbacks.rawExpanded));
    toggle.addEventListener("click", () => {
      callbacks.onToggleRaw(!callbacks.rawExpanded);
    });
    container.append(toggle);
    if (callbacks.rawExpanded) {
      try {
        const inspector = createRawInspector(owner, rawDoc);
        callbacks.onRegisterInspector(inspector);
        container.append(inspector.element);
      } catch {
        container.append(status(owner, "原始 JSON 结构视图暂不可用。", "error"));
      }
    }
  }

  return container;
}

export function sourceSessionPanelView(
  owner: Document,
  state: SourceSessionPanelState,
  callbacks: SourceSessionPanelCallbacks,
): HTMLElement {
  const section = el(owner, "section", "session-composer");
  section.dataset.panel = "source-session";
  section.append(
    el(owner, "h3", "section-title", "原生来源发现与新建"),
    el(
      owner,
      "p",
      "field-help",
      "仅基于显式选择与输入执行 source.session.* 及只读 view.open_source，不依赖 product.status。",
    ),
  );

  const form = el(owner, "form", "pair-form");
  const fields: ReadonlyArray<readonly [keyof SourceLaunchSelectionInput, string, string, string]> = [
    ["sourceId", "source-id", "Source ID", "22 位 canonical base64url ID"],
    ["harnessId", "source-harness-id", "Harness ID", "22 位 canonical base64url ID"],
    ["profileId", "source-profile-id", "Profile ID", "22 位 canonical base64url ID"],
    ["profileRevision", "source-profile-revision", "Profile Revision", "十进制无符号整数"],
    ["workspaceId", "source-workspace-id", "Workspace ID", "22 位 canonical base64url ID"],
    ["launchPath", "source-launch-path", "Launch Path", "工作目录或启动路径"],
  ];

  for (const [key, id, labelText, placeholder] of fields) {
    const label = el(owner, "label", undefined, labelText);
    label.htmlFor = id;
    const input = el(owner, "input");
    input.id = id;
    input.name = id;
    input.type = "text";
    input.autocomplete = "off";
    input.spellcheck = false;
    input.placeholder = placeholder;
    input.value = state.selection[key];
    input.addEventListener("input", () => {
      callbacks.onSelectionFieldChange(key, input.value);
    });
    form.append(label, input);
  }

  const cursorLabel = el(owner, "label", undefined, "Discover Cursor（可选）");
  cursorLabel.htmlFor = "source-discover-cursor";
  const cursorInput = el(owner, "input");
  cursorInput.id = "source-discover-cursor";
  cursorInput.name = "source-discover-cursor";
  cursorInput.type = "text";
  cursorInput.autocomplete = "off";
  cursorInput.spellcheck = false;
  cursorInput.placeholder = "留空表示从头分页";
  cursorInput.value = state.cursor;
  cursorInput.addEventListener("input", () => {
    callbacks.onCursorChange(cursorInput.value);
  });
  form.append(cursorLabel, cursorInput);

  const registerRow = el(owner, "div", "action-row");
  const registerDiscover = btn(owner, "登记发现", "submit");
  registerDiscover.disabled = !state.connected;
  registerDiscover.addEventListener("click", () => {
    registerDiscover.disabled = true;
    callbacks.onRegister("discover", registerDiscover);
  });
  const registerCreate = btn(owner, "登记新建", "secondary-button");
  registerCreate.disabled = !state.connected;
  registerCreate.addEventListener("click", () => {
    registerCreate.disabled = true;
    callbacks.onRegister("create", registerCreate);
  });
  registerRow.append(registerDiscover, registerCreate);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (!registerDiscover.disabled) {
      registerDiscover.disabled = true;
      callbacks.onRegister("discover", registerDiscover);
    }
  });
  form.append(registerRow);
  section.append(form);

  if (state.registration !== null) {
    section.append(
      detail(owner, "来源操作", state.registration.sourceOperationId),
      detail(owner, "动作", state.registration.action),
      detail(owner, "登记时间", state.registration.registeredAtMs),
    );
    const opActions = el(owner, "div", "action-row");
    const execBtn = btn(owner, "执行操作", "submit");
    execBtn.disabled = !state.connected;
    execBtn.addEventListener("click", () => {
      execBtn.disabled = true;
      callbacks.onExecute(execBtn);
    });
    const statusBtn = btn(owner, "刷新状态", "secondary-button");
    statusBtn.disabled = !state.connected;
    statusBtn.addEventListener("click", () => {
      statusBtn.disabled = true;
      callbacks.onRefreshStatus(statusBtn);
    });
    const reconcileBtn = btn(owner, "对账", "secondary-button");
    reconcileBtn.disabled = !state.connected;
    reconcileBtn.addEventListener("click", () => {
      reconcileBtn.disabled = true;
      callbacks.onReconcile(reconcileBtn);
    });
    opActions.append(execBtn, statusBtn, reconcileBtn);

    if (state.fact?.resultAvailable === true) {
      const readBtn = btn(owner, "读取结果", "secondary-button");
      readBtn.disabled = !state.connected;
      readBtn.addEventListener("click", () => {
        readBtn.disabled = true;
        callbacks.onOpenResult(readBtn);
      });
      const releaseBtn = btn(owner, "释放结果", "secondary-button");
      releaseBtn.disabled = !state.connected;
      releaseBtn.addEventListener("click", () => {
        releaseBtn.disabled = true;
        callbacks.onReleaseResult(releaseBtn);
      });
      opActions.append(readBtn, releaseBtn);
    }
    const clearBtn = btn(owner, "清除来源操作", "secondary-button");
    clearBtn.addEventListener("click", () => callbacks.onClearOperation());
    opActions.append(clearBtn);
    section.append(opActions);
  }

  if (state.fact !== null) {
    const summary = `${state.fact.state} / ${state.fact.outcome}${
      state.fact.failure !== null ? ` / ${state.fact.failure}` : ""
    }`;
    section.append(detail(owner, "操作状态", summary));
    if (state.fact.response !== null) {
      section.append(
        detail(
          owner,
          "响应确认",
          `${state.fact.response.kind} / ack=${state.fact.response.ack} / ${state.fact.response.resultBytes}B`,
        ),
      );
    }
  }

  if (state.result !== null) {
    section.append(
      sourceSessionResultView(owner, state.result, {
        rawExpanded: state.rawExpanded,
        onUseNextCursor: callbacks.onUseNextCursor,
        onOpenView: callbacks.onOpenDiscoveredView,
        onToggleRaw: callbacks.onToggleRawResult,
        onRegisterInspector: callbacks.onRegisterInspector,
      }),
    );
  }

  const manualBox = el(owner, "div", "pair-form");
  const manualLabel = el(
    owner,
    "label",
    undefined,
    "直接按原生会话 ID 打开只读视图（无列表能力回退）",
  );
  manualLabel.htmlFor = "source-manual-native-id";
  const manualInput = el(owner, "input");
  manualInput.id = "source-manual-native-id";
  manualInput.name = "source-manual-native-id";
  manualInput.type = "text";
  manualInput.autocomplete = "off";
  manualInput.spellcheck = false;
  manualInput.placeholder = "输入外部原生 sessionId";
  manualInput.value = state.manualNativeSessionId;
  manualInput.addEventListener("input", () => {
    callbacks.onManualNativeSessionIdChange(manualInput.value);
  });
  const openManual = btn(owner, "按原生 ID 打开只读视图", "secondary-button");
  openManual.disabled = !state.connected;
  openManual.addEventListener("click", () => {
    openManual.disabled = true;
    callbacks.onOpenManualView(openManual);
  });
  manualBox.append(manualLabel, manualInput, openManual);
  section.append(manualBox);

  if (state.activeSourceView !== null) {
    const viewBanner = el(owner, "div", "session-view-state");
    if (state.staleNotice !== null) {
      viewBanner.dataset.stale = "true";
      viewBanner.append(status(owner, state.staleNotice, "error"));
    }
    if (state.openedView !== null) {
      const cacheSummary = state.openedView.status.transcriptSource
        ? `；来源=${
            state.openedView.status.transcriptSource === "transcript_cache"
              ? "本地缓存"
              : state.openedView.status.transcriptSource === "hybrid_resumed"
                ? "缓存+实时续接"
                : "实时"
          }` +
          (state.openedView.status.syncState ? `，同步=${state.openedView.status.syncState}` : "") +
          (state.openedView.status.capabilityBoundary ? `，能力=${state.openedView.status.capabilityBoundary}` : "") +
          (state.openedView.status.cacheWarning ? "（本地缓存写入告警）" : "")
        : "";
      viewBanner.append(
        status(
          owner,
          `来源只读视图已打开：${state.openedView.status.mode}，${
            state.openedView.status.historyIncomplete
              ? "历史不完整（仅当前实时连接/缓存，无法补全未缓存前文）"
              : "支持重放"
          }${cacheSummary}。`,
          "notice",
        ),
      );
      const actions = el(owner, "div", "action-row");
      const refreshBtn = btn(owner, "刷新来源只读视图", "secondary-button");
      refreshBtn.disabled = !state.connected;
      refreshBtn.addEventListener("click", () => {
        refreshBtn.disabled = true;
        callbacks.onRefreshSourceView(refreshBtn);
      });
      const closeBtn = btn(owner, "关闭只读视图", "secondary-button");
      closeBtn.addEventListener("click", () => {
        closeBtn.disabled = true;
        callbacks.onCloseSourceView(closeBtn);
      });
      actions.append(refreshBtn, closeBtn);
      viewBanner.append(actions);
    } else if (state.staleNotice !== null) {
      const actions = el(owner, "div", "action-row");
      const retryBtn = btn(owner, "重试来源只读视图", "secondary-button");
      retryBtn.disabled = !state.connected;
      retryBtn.addEventListener("click", () => {
        retryBtn.disabled = true;
        callbacks.onRefreshSourceView(retryBtn);
      });
      const abandonBtn = btn(owner, "放弃来源只读视图", "secondary-button");
      abandonBtn.addEventListener("click", () => callbacks.onAbandonSourceView());
      actions.append(retryBtn, abandonBtn);
      viewBanner.append(actions);
    }
    section.append(viewBanner);
  }

  return section;
}

function el<K extends keyof HTMLElementTagNameMap>(
  owner: Document,
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = owner.createElement(tag);
  if (className !== undefined) {
    node.className = className;
  }
  if (text !== undefined) {
    node.textContent = text;
  }
  return node;
}

function btn(
  owner: Document,
  text: string,
  className?: string,
): HTMLButtonElement {
  const value = el(owner, "button", className, text);
  value.type = "button";
  return value;
}

function detail(owner: Document, label: string, value: string): HTMLElement {
  const block = el(owner, "div", "detail-block");
  block.append(
    el(owner, "span", "detail-label", label),
    el(owner, "code", undefined, value),
  );
  return block;
}

function status(
  owner: Document,
  text: string,
  tone: "working" | "notice" | "error",
): HTMLElement {
  const line = el(owner, "p", `status-line status-${tone}`);
  line.setAttribute("role", tone === "error" ? "alert" : "status");
  const marker = el(owner, "span", "status-marker");
  marker.setAttribute("aria-hidden", "true");
  line.append(marker, el(owner, "span", undefined, text));
  return line;
}
