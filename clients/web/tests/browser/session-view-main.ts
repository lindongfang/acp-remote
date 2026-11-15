import "../../src/styles.css";
import { IndexedDbIdentityRepository, IdentityVault } from "../../src/pairing/identity";
import { randomId128 } from "../../src/protocol/pairing";
import { MainFixturePeer, type OpenRequest, type SourceOpenRequest } from "./independent-dom/management-peer";
import { recordFrame, snapshotEndFrame, statusRecord, timelinePair } from "./independent-dom/view-wire";

import { encodeBase64url } from "../../src/protocol/jws";

const TEST_ORIGIN = "https://127.0.0.1:4174";
const result = document.querySelector<HTMLOutputElement>("#fixture-result");
if (result === null) throw new Error("fixture output is missing");
const resultNode = result;
if (location.origin !== TEST_ORIGIN || location.protocol !== "https:") {
  result.textContent = `停止：夹具只允许专用 origin ${TEST_ORIGIN}，当前 ${location.origin}。`;
  throw new Error("session-view main fixture refuses a non-dedicated origin");
}

const peer = new MainFixturePeer(location.origin);
const packetSequence = new Map<string, bigint>();

interface InputControlSnapshot {
  readonly present: boolean;
  readonly value: string | null;
  readonly start: number | null;
  readonly end: number | null;
  readonly direction: string | null;
}

interface DocumentSelectionSnapshot {
  readonly rangeCount: number;
  readonly anchorText: string | null;
  readonly anchorOffset: number | null;
  readonly focusText: string | null;
  readonly focusOffset: number | null;
  readonly text: string;
}

interface FocusSelectionStageSnapshot {
  readonly stage: string;
  readonly activeId: string | null;
  readonly activeTag: string | null;
  readonly targetInput: InputControlSnapshot;
  readonly composerTextarea: InputControlSnapshot;
  readonly docSelection: DocumentSelectionSnapshot;
}

interface MainDomDiagnostics {
  readonly caseName: string;
  readonly wv15Stages?: FocusSelectionStageSnapshot[];
  readonly wv15Classification?: string;
}

async function prepareIdentity(): Promise<void> {
  try {
    sessionStorage.removeItem("acp-remote.shared-source-view.v1");
  } catch {
    // Ignore storage unavailability.
  }
  await deleteFixtureIdentityDatabase();
  const repository = new IndexedDbIdentityRepository();
  const vault = new IdentityVault(repository);
  const unbound = await vault.initialize(location.origin);
  const approved = await vault.save({
    ...unbound,
    nodeId: peer.identity.nodeId,
    nodePublicKey: peer.identity.nodePublicKey,
    pairingId: randomId128(),
    clientId: peer.identity.clientId,
    displayName: "Synthetic production-main fixture",
    pendingClaim: null,
    status: "approved",
    updatedAt: Date.now(),
  });
  if (approved.privateKey.extractable || approved.publicKey.length === 0) {
    throw new Error("fixture client private key must remain non-exportable");
  }
}

function deleteFixtureIdentityDatabase(): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.deleteDatabase("acp-remote-access-v1");
    request.addEventListener("success", () => resolve(), { once: true });
    request.addEventListener("error", () => reject(request.error), { once: true });
    request.addEventListener("blocked", () => reject(new Error("fixture identity database is blocked")), { once: true });
  });
}

function check(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}
function fixtureId(byte: number): string {
  return encodeBase64url(new Uint8Array(16).fill(byte));
}

async function until(predicate: () => boolean, label: string, timeoutMs = 5000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${label}`);
    await new Promise((resolve) => globalThis.setTimeout(resolve, 10));
  }
}

async function submitTarget(
  targetId: string,
  previousOpenButton: HTMLButtonElement | null = null,
): Promise<void> {
  const start = peer.commands.length;
  const input = document.querySelector<HTMLInputElement>("#session-target-id");
  check(input !== null, "production main target input is missing");
  input.value = targetId;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  const form = input.form;
  check(form !== null, "production main target form is missing");
  form.requestSubmit();
  await until(() => peer.commands.slice(start).some((entry) =>
    entry.action === "input.queue" && entry.payload.targetId === targetId), "main input.queue");
  await until(() => {
    const currentButton = findButton("打开共享视图");
    const currentInput = document.querySelector<HTMLInputElement>("#session-target-id");
    return currentButton !== null &&
      currentButton !== previousOpenButton &&
      !currentButton.disabled &&
      currentInput?.value === targetId;
  }, "production main fresh enabled open-view button");
}

function findButton(label: string, rootSelector = "#access-panel"): HTMLButtonElement | null {
  return [...document.querySelectorAll<HTMLButtonElement>(`${rootSelector} button`)]
    .find((button) => button.textContent?.trim() === label) ?? null;
}
function clickMainButton(label: string, rootSelector = "#access-panel"): void {
  const button = findButton(label, rootSelector);
  check(button !== null && !button.disabled, `production main button unavailable: ${label}`);
  button.click();
}
async function clickOpenAndWaitForRequest(): Promise<void> {
  const previous = peer.pendingOpens.length + peer.openedViews.length;
  clickMainButton("打开共享视图");
  await until(() => peer.pendingOpens.length + peer.openedViews.length > previous, "main view.open");
}
function latestOpen(): OpenRequest {
  const opened = peer.openedViews.at(-1);
  check(opened !== undefined, "fixture peer has no opened view");
  return opened;
}
function latestSourceOpen(): SourceOpenRequest {
  const opened = peer.openedSourceViews.at(-1);
  check(opened !== undefined, "fixture peer has no opened source view");
  return opened;
}
function creditExists(streamId: string, sequence: bigint): boolean {
  return peer.sockets.some((socket) => socket.sentBinary.some((frame) =>
    frame.type === "credit" && frame.streamId === streamId && frame.packetSequence === sequence));
}
function sharedViews(): readonly HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>("#session-view-cache > [aria-label='共享会话视图']")];
}
function nextPacket(streamId: string): bigint {
  const next = (packetSequence.get(streamId) ?? 0n) + 1n;
  packetSequence.set(streamId, next);
  return next;
}
function pushRecord(opened: { readonly streamId: string }, kind: "status" | "original_raw" | "item_projection", id: bigint, json: string): void {
  peer.push(opened.streamId, recordFrame(opened.streamId, nextPacket(opened.streamId), id, kind, json));
}
function pushSnapshotEnd(opened: { readonly streamId: string }, recordId: bigint): void {
  peer.push(opened.streamId, snapshotEndFrame(opened.streamId, nextPacket(opened.streamId), recordId));
}

function captureInputControl(control: HTMLInputElement | HTMLTextAreaElement | null): InputControlSnapshot {
  if (control === null) {
    return { present: false, value: null, start: null, end: null, direction: null };
  }
  return {
    present: true,
    value: control.value,
    start: control.selectionStart,
    end: control.selectionEnd,
    direction: control.selectionDirection,
  };
}

function captureFocusAndSelections(stage: string): FocusSelectionStageSnapshot {
  const active = document.activeElement;
  const sel = globalThis.getSelection();
  return {
    stage,
    activeId: active instanceof HTMLElement && active.id !== "" ? active.id : null,
    activeTag: active?.tagName ?? null,
    targetInput: captureInputControl(document.querySelector<HTMLInputElement>("#session-target-id")),
    composerTextarea: captureInputControl(document.querySelector<HTMLTextAreaElement>("#session-input-body")),
    docSelection: {
      rangeCount: sel?.rangeCount ?? 0,
      anchorText: sel?.anchorNode?.textContent ?? null,
      anchorOffset: sel !== null && sel.rangeCount > 0 ? sel.anchorOffset : null,
      focusText: sel?.focusNode?.textContent ?? null,
      focusOffset: sel !== null && sel.rangeCount > 0 ? sel.focusOffset : null,
      text: sel?.toString() ?? "",
    },
  };
}

async function runWV13(): Promise<string> {
  peer.holdOpens = true;
  const targetA = fixtureId(201);
  const targetB = fixtureId(202);
  await submitTarget(targetA);
  const buttonA = findButton("打开共享视图");
  check(buttonA !== null, "buttonA missing before first view.open");
  await clickOpenAndWaitForRequest();
  check(buttonA.disabled, "buttonA should be disabled while view.open is pending");
  await submitTarget(targetB, buttonA);
  await clickOpenAndWaitForRequest();
  check(peer.pendingOpens.length === 2, "fixture must hold both overlapping view.open responses");
  const openedB = await peer.answerOpen(1);
  await until(() => creditExists(openedB.streamId, 0n), "new stream initial credit");
  const openedA = await peer.answerOpen(0);
  await until(() => peer.commands.some((entry) =>
    entry.action === "view.close" && entry.payload.streamId === openedA.streamId), "stale open cleanup");
  await until(() => peer.sockets.some((socket) => socket.sentBinary.some((frame) =>
    frame.type === "detach" && frame.streamId === openedA.streamId)), "stale stream detach");
  check(!creditExists(openedA.streamId, 0n), "stale open must not receive initial credit");
  check(sharedViews().length === 1, "stale response replaced or removed the current display");
  check(sharedViews()[0]?.dataset.streamId === openedB.streamId,
    "stale open response changed the production display stream binding");

  peer.holdOpens = false;
  peer.holdCloses = true;
  clickMainButton("关闭共享视图");
  await until(() => peer.pendingCloses.length === 1, "held close response");
  await clickOpenAndWaitForRequest();
  const replacement = latestOpen();
  check(replacement.streamId !== openedB.streamId, "replacement must use a fresh stream");
  await until(() => creditExists(replacement.streamId, 0n), "replacement initial credit");
  peer.answerClose(0);
  await new Promise((resolve) => globalThis.setTimeout(resolve, 30));
  check(sharedViews().length === 1, "late close response cleared the replacement display");
  check(sharedViews()[0]?.dataset.streamId === replacement.streamId,
    "late close response changed the replacement DOM stream marker");
  peer.holdCloses = false;
  return "WV-13 通过：真实 main DOM 保留最新 open；旧 open/close 响应未覆盖新 stream。";
}

async function runWV14(): Promise<string> {
  const target = fixtureId(203);
  await submitTarget(target);
  peer.failNextInitialCredit = true;
  const commandStart = peer.commands.length;
  await clickOpenAndWaitForRequest();
  const failed = latestOpen();
  await until(() => peer.commands.slice(commandStart).some((entry) =>
    entry.action === "view.close" && entry.payload.streamId === failed.streamId), "failed-open local release");
  await until(() => sharedViews().length === 0, "failed initial credit display cleanup");
  check(!creditExists(failed.streamId, 0n), "failed initial credit was recorded as delivered");
  check(peer.sockets.some((socket) => socket.sentBinary.some((frame) =>
    frame.type === "detach" && frame.streamId === failed.streamId)), "failed stream was not detached");
  check(document.querySelector("#access-panel")?.textContent?.includes("共享视图未打开"),
    "main did not report the initial-credit failure");
  peer.push(failed.streamId, recordFrame(
    failed.streamId, 1n, 90n, "status", statusRecord(failed.streamId, 92),
  ));
  await new Promise((resolve) => globalThis.setTimeout(resolve, 30));
  check(sharedViews().length === 0, "late old-stream frame recreated a disposed display");
  check(!creditExists(failed.streamId, 1n), "late old-stream frame received credit after failed open");
  await clickOpenAndWaitForRequest();
  const replacement = latestOpen();
  await until(() => creditExists(replacement.streamId, 0n), "reopened stream initial credit");
  check(replacement.streamId !== failed.streamId, "reopen reused the failed stream identity");
  check(sharedViews()[0]?.dataset.streamId === replacement.streamId,
    "reopen did not install a usable production display");
  return "WV-14 通过：credit(0) 失败后迟到旧帧无显示/credit，重新打开的新 stream 正常启动。";
}

async function runWV15(): Promise<string> {
  const stages: FocusSelectionStageSnapshot[] = [];
  window.__mainDomDiagnostics = { caseName: "WV15", wv15Stages: stages, wv15Classification: "running" };

  const target = fixtureId(204);
  await submitTarget(target);
  await clickOpenAndWaitForRequest();
  const opened = latestOpen();
  await until(() => creditExists(opened.streamId, 0n), "WV-15 initial credit");

  pushRecord(opened, "status", 50n, statusRecord(opened.streamId, 91));
  pushSnapshotEnd(opened, 51n);
  for (const item of timelinePair("Tool WV15", "first content", opened.streamId, 1n, 60n)) {
    pushRecord(opened, item.kind, item.id, item.json);
  }
  await until(() => document.querySelector("#session-view-cache")?.textContent?.includes("Tool WV15") === true,
    "first production timeline item");
  const rawToggle = [...document.querySelectorAll<HTMLButtonElement>("#session-view-cache button")]
    .find((button) => button.textContent?.trim() === "查看视图状态原始结构");
  check(rawToggle !== undefined, "status raw-inspector control is missing");
  rawToggle.click();
  check(rawToggle.getAttribute("aria-expanded") === "true", "raw status expansion did not open");

  // Phase A: input focus & selection preservation across incremental view updates and devices.changed
  const targetInput = document.querySelector<HTMLInputElement>("#session-target-id");
  check(targetInput !== null, "production target input is missing for focus assertion");
  targetInput.focus();
  targetInput.setSelectionRange(2, 8, "forward");
  check(document.activeElement === targetInput, "fixture could not focus the real target input");
  stages.push(captureFocusAndSelections("1_after_input_setSelectionRange_2_8"));

  const updated = timelinePair("Tool WV15", "WV15 updated content", opened.streamId, 2n, 70n);
  for (const item of updated) pushRecord(opened, item.kind, item.id, item.json);
  await until(() => document.querySelector("#session-view-cache")?.textContent?.includes("WV15 updated content") === true,
    "updated production timeline content");
  check(document.activeElement === targetInput, "view update moved focus from the main input");
  check(targetInput.selectionStart === 2 && targetInput.selectionEnd === 8,
    "view update changed the main input selection");
  const currentToggle = [...document.querySelectorAll<HTMLButtonElement>("#session-view-cache button")]
    .find((button) => button.textContent?.trim() === "查看视图状态原始结构");
  check(currentToggle?.getAttribute("aria-expanded") === "true", "view update collapsed raw status inspection");
  check(creditExists(opened.streamId, 6n), "valid incremental packets were not credited through the main transport");

  const inputValue = targetInput.value;
  const beforeInputRefresh = captureFocusAndSelections("2_before_input_devices_changed");
  stages.push(beforeInputRefresh);
  check(
    beforeInputRefresh.activeId === "session-target-id" &&
    beforeInputRefresh.targetInput.start === 2 &&
    beforeInputRefresh.targetInput.end === 8,
    "phase A precondition missing on target input before devices.changed",
  );
  const revisionA = peer.notifyDevicesChanged();
  await until(() => document.querySelector("#access-panel")?.textContent?.includes(`列表 revision ${revisionA}`) === true,
    "phase A dashboard refresh after devices.changed");
  stages.push(captureFocusAndSelections("3_after_input_devices_changed"));

  const restoredInput = document.querySelector<HTMLInputElement>("#session-target-id");
  check(restoredInput !== null && document.activeElement === restoredInput,
    "phase A dashboard refresh did not restore target input focus");
  check(restoredInput.value === inputValue, "phase A dashboard refresh changed target input value");
  check(restoredInput.selectionStart === 2 && restoredInput.selectionEnd === 8,
    "phase A dashboard refresh changed target input selection");
  check([...document.querySelectorAll<HTMLButtonElement>("#session-view-cache button")]
    .find((button) => button.textContent?.trim() === "查看视图状态原始结构")?.getAttribute("aria-expanded") === "true",
    "phase A dashboard refresh collapsed raw status inspection");
  check(sharedViews()[0]?.dataset.streamId === opened.streamId,
    "phase A dashboard refresh replaced the live shared view stream");

  // Phase B: DOM Range selection on timeline text across devices.changed (blur input first as user clicks outside input to select timeline text)
  restoredInput.blur();
  const body = document.querySelector<HTMLElement>("#session-view-cache .acp-timeline__body");
  check(body !== null, "production timeline body is missing for text selection check");
  const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT);
  let textNode: Text | null = null;
  while (walker.nextNode()) {
    if (walker.currentNode.textContent?.includes("WV15 updated content")) {
      textNode = walker.currentNode as Text;
      break;
    }
  }
  check(textNode !== null, "updated production timeline text node is missing");
  const selection = globalThis.getSelection();
  check(selection !== null, "document selection API is unavailable");
  const start = textNode.data.indexOf("WV15 updated content");
  const range = document.createRange();
  range.setStart(textNode, start);
  range.setEnd(textNode, start + "WV15 updated content".length);
  selection.removeAllRanges();
  selection.addRange(range);
  const selectedText = selection.toString();
  check(selectedText === "WV15 updated content", "fixture could not create a stable timeline text selection");
  stages.push(captureFocusAndSelections("4_before_timeline_devices_changed"));

  const revisionB = peer.notifyDevicesChanged();
  await until(() => document.querySelector("#access-panel")?.textContent?.includes(`列表 revision ${revisionB}`) === true,
    "phase B dashboard refresh after devices.changed");
  stages.push(captureFocusAndSelections("5_after_timeline_devices_changed"));

  check(globalThis.getSelection()?.toString() === selectedText,
    "phase B dashboard refresh lost the selection in production timeline text");
  check([...document.querySelectorAll<HTMLButtonElement>("#session-view-cache button")]
    .find((button) => button.textContent?.trim() === "查看视图状态原始结构")?.getAttribute("aria-expanded") === "true",
    "phase B dashboard refresh collapsed raw status inspection");
  check(sharedViews()[0]?.dataset.streamId === opened.streamId,
    "phase B dashboard refresh replaced the live shared view stream");

  // Phase C: unsent composer textarea draft, focus & selection across devices.changed
  const draft = "unsent WV15 draft";
  const textarea = document.querySelector<HTMLTextAreaElement>("#session-input-body");
  check(textarea !== null, "production queue composer textarea is missing");
  textarea.value = draft;
  textarea.dispatchEvent(new Event("input", { bubbles: true }));
  textarea.focus();
  textarea.setSelectionRange(3, 9, "forward");
  stages.push(captureFocusAndSelections("6_before_textarea_devices_changed"));
  const revisionC = peer.notifyDevicesChanged();
  await until(() => document.querySelector("#access-panel")?.textContent?.includes(`列表 revision ${revisionC}`) === true,
    "phase C dashboard refresh after devices.changed");
  stages.push(captureFocusAndSelections("7_after_textarea_devices_changed"));
  window.__mainDomDiagnostics = { caseName: "WV15", wv15Stages: stages, wv15Classification: "passed" };

  const restoredTextarea = document.querySelector<HTMLTextAreaElement>("#session-input-body");
  check(restoredTextarea !== null && document.activeElement === restoredTextarea,
    "phase C dashboard refresh did not restore composer focus");
  check(restoredTextarea.value === draft, "phase C dashboard refresh changed unsent textarea content");
  check(restoredTextarea.selectionStart === 3 && restoredTextarea.selectionEnd === 9,
    "phase C dashboard refresh changed textarea selection");
  check([...document.querySelectorAll<HTMLButtonElement>("#session-view-cache button")]
    .find((button) => button.textContent?.trim() === "查看视图状态原始结构")?.getAttribute("aria-expanded") === "true",
    "phase C dashboard refresh collapsed raw status inspection");
  check(sharedViews()[0]?.dataset.streamId === opened.streamId,
    "phase C dashboard refresh replaced the shared view stream");
  return "WV-15 通过：真实增量及连续 dashboard 刷新分别保留 raw 展开、input/textarea 焦点/value/selection、timeline 文本选择及 stream DOM。";
}

function setFieldValue(selector: string, value: string): void {
  const input = document.querySelector<HTMLInputElement>(selector);
  check(input !== null, `source input missing: ${selector}`);
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

async function runWV16(): Promise<string> {
  const panelSelector = '[data-panel="source-session"]';
  const sourcePanel = document.querySelector<HTMLElement>(panelSelector);
  check(sourcePanel !== null, "source-session panel is missing");

  const sourceId = fixtureId(210);
  const harnessId = fixtureId(211);
  const profileId = fixtureId(212);
  const workspaceId = fixtureId(213);
  const launchPath = "D:/Project/acp-remote";

  setFieldValue("#source-id", sourceId);
  setFieldValue("#source-harness-id", harnessId);
  setFieldValue("#source-profile-id", profileId);
  setFieldValue("#source-profile-revision", "3");
  setFieldValue("#source-workspace-id", workspaceId);
  setFieldValue("#source-launch-path", launchPath);

  const registerStart = peer.commands.length;
  clickMainButton("登记发现", panelSelector);
  await until(() => peer.commands.slice(registerStart).some((entry) => entry.action === "source.session.register"),
    "WV-16 source.session.register");
  await until(() => findButton("执行操作", panelSelector) !== null,
    "WV-16 source operation execute control");

  const executeStart = peer.commands.length;
  clickMainButton("执行操作", panelSelector);
  await until(() => peer.commands.slice(executeStart).some((entry) => entry.action === "source.session.execute"),
    "WV-16 source.session.execute");
  await until(() => {
    const panelText = document.querySelector(panelSelector)?.textContent ?? "";
    return panelText.includes("terminal / succeeded") && findButton("读取结果", panelSelector) !== null;
  }, "WV-16 resultAvailable terminal state");

  const resultOpenStart = peer.commands.length;
  clickMainButton("读取结果", panelSelector);
  await until(() => peer.commands.slice(resultOpenStart).some((entry) => entry.action === "source.session.result.open"),
    "WV-16 source.session.result.open");
  await until(() => {
    const resultText = document.querySelector(`${panelSelector} .source-session-result`)?.textContent ?? "";
    return resultText.includes("ext-wv16-1") && resultText.includes("WV16 Discovered Session");
  }, "WV-16 chunked source result rendered");

  clickMainButton("查看结果原始结构", panelSelector);
  const rawToggle = findButton("查看结果原始结构", panelSelector);
  check(rawToggle?.getAttribute("aria-expanded") === "true", "WV-16 raw result inspector failed to expand");

  clickMainButton("使用下一页游标", panelSelector);
  await until(() => document.querySelector<HTMLInputElement>("#source-discover-cursor")?.value === "cursor-wv16-2",
    "WV-16 nextCursor applied to cursor input");

  const openViewStart = peer.commands.length;
  const openSourceCountBefore = peer.openedSourceViews.length;
  clickMainButton("打开只读视图", `${panelSelector} .source-session-result`);
  await until(() => peer.openedSourceViews.length > openSourceCountBefore,
    "WV-16 view.open_source");
  const openCommands = peer.commands.slice(openViewStart).filter((entry) =>
    entry.action === "view.open" || entry.action === "view.open_source");
  check(openCommands.length === 1 && openCommands[0]?.action === "view.open_source",
    "WV-16 source discovery card must invoke view.open_source instead of view.open");

  const opened = latestSourceOpen();
  check(opened.nativeSessionId === "ext-wv16-1", "WV-16 opened wrong nativeSessionId");
  check(opened.reason === "first_open", "WV-16 expected first_open reason");
  await until(() => creditExists(opened.streamId, 0n), "WV-16 view.open_source initial credit");

  pushRecord(opened, "status", 50n, statusRecord(opened.streamId, 95));
  pushSnapshotEnd(opened, 51n);
  for (const item of timelinePair("Source WV16", "source stream content", opened.streamId, 1n, 80n)) {
    pushRecord(opened, item.kind, item.id, item.json);
  }
  await until(() => document.querySelector("#session-view-cache")?.textContent?.includes("source stream content") === true,
    "WV-16 source stream timeline item");
  const sourceBannerText = document.querySelector(panelSelector)?.textContent ?? "";
  check(sourceBannerText.includes("current_only") && sourceBannerText.includes("历史不完整"),
    "WV-16 source view banner missing CurrentOnly / incomplete history notice");

  const revision = peer.notifyDevicesChanged();
  await until(() => document.querySelector("#access-panel")?.textContent?.includes(`列表 revision ${revision}`) === true,
    "WV-16 dashboard refresh after devices.changed");
  check(document.querySelector<HTMLInputElement>("#source-id")?.value === sourceId,
    "WV-16 devices.changed lost #source-id draft");
  check(document.querySelector<HTMLInputElement>("#source-launch-path")?.value === launchPath,
    "WV-16 devices.changed lost #source-launch-path draft");
  check(document.querySelector<HTMLInputElement>("#source-discover-cursor")?.value === "cursor-wv16-2",
    "WV-16 devices.changed lost #source-discover-cursor draft");
  check(document.querySelector(`${panelSelector} .source-session-result`)?.textContent?.includes("ext-wv16-1") === true,
    "WV-16 devices.changed lost discovered session result");
  check(findButton("查看结果原始结构", panelSelector)?.getAttribute("aria-expanded") === "true",
    "WV-16 devices.changed collapsed source result raw inspector");
  check(sharedViews()[0]?.dataset.streamId === opened.streamId,
    "WV-16 devices.changed replaced the live source shared view stream");

  const closeStart = peer.commands.length;
  clickMainButton("关闭只读视图", panelSelector);
  await until(() => peer.commands.slice(closeStart).some((entry) =>
    entry.action === "view.close" && entry.payload.streamId === opened.streamId), "WV-16 view.close");
  await until(() => sharedViews().length === 0, "WV-16 shared view removal after close");

  const releaseStart = peer.commands.length;
  clickMainButton("释放结果", panelSelector);
  await until(() => peer.commands.slice(releaseStart).some((entry) => entry.action === "source.session.result.release"),
    "WV-16 source.session.result.release");
  await until(() => findButton("读取结果", panelSelector) === null,
    "WV-16 result controls removed after release");

  window.__mainDomDiagnostics = { caseName: "WV16" };
  return "WV-16 通过：source-session 面板完成登记、执行、分块读取、view.open_source、devices.changed 状态保留及释放闭环。";
}

async function runCase(name: string): Promise<void> {
  resultNode.textContent = `${name} 运行中…`;
  try {
    const message = name === "WV13"
      ? await runWV13()
      : name === "WV14"
        ? await runWV14()
        : name === "WV15"
          ? await runWV15()
          : await runWV16();
    resultNode.textContent = message;
  } catch (error) {
    resultNode.textContent = `${name} 未通过：${error instanceof Error ? error.message : "未知错误"}`;
    throw error;
  }
}

async function start(): Promise<void> {
  await prepareIdentity();
  peer.install();
  await import("../../src/main.ts");
  window.__mainDomFixture = { peer, run: runCase };
  await until(() => document.querySelector("#session-target-id") !== null, "authenticated production main dashboard", 15_000);
  resultNode.textContent = "已启动真实 main 与真实 ManagementConnection；FakeSocket peer 使用测试节点签名密钥。选择 WV 用例运行。";
  const selected = new URL(location.href).searchParams.get("case");
  if (selected === "WV13" || selected === "WV14" || selected === "WV15" || selected === "WV16") await runCase(selected);
}

interface MainDomFixture {
  readonly peer: MainFixturePeer;
  run(name: string): Promise<void>;
}
declare global {
  interface Window {
    __mainDomFixture: MainDomFixture;
    __mainDomDiagnostics?: MainDomDiagnostics;
  }
}

void start().catch((error: unknown) => {
  resultNode.textContent = `夹具未启动：${error instanceof Error ? error.message : "未知错误"}`;
});
