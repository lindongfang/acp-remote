import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  button,
  captureActiveControlSelection,
  delay,
  detailBlock,
  element,
  eyebrow,
  formatHex,
  formatTimestamp,
  paragraph,
  requiredElement,
  restoreActiveControlSelection,
  statusLine,
  title,
} from "../src/workbench/dom-primitives";
import { ApprovalController } from "../src/workbench/approval-controller";
import { CapacityReleaseController } from "../src/workbench/capacity-release-controller";
import {
  SessionQueueController,
  type SessionQueueHostPort,
} from "../src/workbench/session-queue-controller";
import {
  SharedViewController,
  type SharedViewHostPort,
} from "../src/workbench/shared-view-controller";
import {
  clearPersistedSourceViewSpec,
  loadPersistedSourceViewSpec,
  persistSourceViewSpec,
} from "../src/management/source-session-view";
import { encodeBase64url } from "../src/protocol/jws";
import type { ManagementConnection } from "../src/management/client";
import type { IdentityRecord } from "../src/pairing/identity";
import {
  FakeButtonElement,
  FakeDivElement,
  FakeDocument,
  installFakeDocument,
} from "./support/fake-dom";

let restoreDoc: (() => void) | null = null;

beforeEach(() => {
  const installed = installFakeDocument();
  restoreDoc = installed.restore;
});

afterEach(() => {
  restoreDoc?.();
  restoreDoc = null;
});

describe("workbench dom-primitives", () => {
  it("creates standard DOM elements with classes and text", () => {
    const el = element("span", "test-class", "hello");
    expect(el.tagName).toBe("SPAN");
    expect(el.className).toBe("test-class");
    expect(el.textContent).toBe("hello");

    const btn = button("Click Me", "primary");
    expect(btn.type).toBe("button");
    expect(btn.className).toBe("primary");
    expect(btn.textContent).toBe("Click Me");

    const eye = eyebrow("HEADER");
    expect(eye.className).toBe("panel-eyebrow");

    const h = title("My Title");
    expect(h.id).toBe("panel-title");
    expect(h.textContent).toBe("My Title");

    const p = paragraph("description", "field-help");
    expect(p.className).toBe("field-help");

    const line = statusLine("Processing", "working");
    expect(line.className).toContain("status-working");
    expect(line.getAttribute("role")).toBe("status");

    const errLine = statusLine("Failed", "error");
    expect(errLine.getAttribute("role")).toBe("alert");

    const block = detailBlock("Label", "Val");
    expect(block.className).toBe("detail-block");
  });

  it("requiredElement returns element when found and throws when missing or wrong type", () => {
    const div = document.createElement("div");
    div.id = "find-me";
    document.body.append(div);

    expect(
      requiredElement(
        "find-me",
        FakeDivElement as unknown as { new (): HTMLDivElement },
      ),
    ).toBe(div);
    expect(() =>
      requiredElement(
        "find-me",
        FakeButtonElement as unknown as { new (): HTMLButtonElement },
      ),
    ).toThrow();
    expect(() =>
      requiredElement(
        "not-found",
        FakeDivElement as unknown as { new (): HTMLDivElement },
      ),
    ).toThrow();
  });

  it("formats hex and timestamps reliably", () => {
    expect(formatHex("0123456789abcdef0123456789abcdef")).toBe(
      "01234567 89abcdef 01234567 89abcdef",
    );
    expect(formatTimestamp(1700000000)).toContain("2023");
    expect(formatTimestamp(NaN)).toBe("NaN");
  });

  it("delay resolves on timeout and early on abort", async () => {
    const controller = new AbortController();
    const delayed = delay(1000, controller.signal);
    controller.abort();
    await expect(delayed).resolves.toBeUndefined();
  });

  it("captures and restores input and textarea selection without scrolling, including value-drift and removed-control boundaries (WV-15)", () => {
    const container = document.createElement("div");
    const outside = document.createElement("input");
    outside.id = "outside-input";
    outside.value = "outside";
    const input = document.createElement("input");
    input.id = "test-input";
    input.value = "hello world";
    container.append(input);
    document.body.append(outside, container);

    // 1. HTMLInputElement [2, 8] selection & preventScroll focus
    input.focus();
    input.setSelectionRange(2, 8, "forward");
    const inputSnapshot = captureActiveControlSelection(document, container);
    expect(inputSnapshot).toEqual({
      id: "test-input",
      value: "hello world",
      start: 2,
      end: 8,
      direction: "forward",
    });

    container.replaceChildren();
    const replacementInput = document.createElement("input");
    replacementInput.id = "test-input";
    replacementInput.value = "hello world";
    const inputFocusSpy = vi.spyOn(replacementInput, "focus");
    container.append(replacementInput);

    restoreActiveControlSelection(document, inputSnapshot);
    expect(inputFocusSpy).toHaveBeenCalledWith({ preventScroll: true });
    expect(document.activeElement).toBe(replacementInput);
    expect(replacementInput.selectionStart).toBe(2);
    expect(replacementInput.selectionEnd).toBe(8);
    expect(replacementInput.selectionDirection).toBe("forward");

    // 2. HTMLTextAreaElement [3, 9] backward selection + unsaved value preservation over default value
    const textarea = document.createElement("textarea");
    textarea.id = "session-input-body";
    textarea.value = "unsaved multiline draft";
    container.replaceChildren(textarea);
    textarea.focus();
    textarea.setSelectionRange(3, 9, "backward");

    const textareaSnapshot = captureActiveControlSelection(document, container);
    expect(textareaSnapshot).toEqual({
      id: "session-input-body",
      value: "unsaved multiline draft",
      start: 3,
      end: 9,
      direction: "backward",
    });

    const replacementTextarea = document.createElement("textarea");
    replacementTextarea.id = "session-input-body";
    replacementTextarea.value = "";
    const textareaFocusSpy = vi.spyOn(replacementTextarea, "focus");
    container.replaceChildren(replacementTextarea);

    restoreActiveControlSelection(document, textareaSnapshot);
    expect(textareaFocusSpy).toHaveBeenCalledWith({ preventScroll: true });
    expect(document.activeElement).toBe(replacementTextarea);
    expect(replacementTextarea.value).toBe("unsaved multiline draft");
    expect(replacementTextarea.selectionStart).toBe(3);
    expect(replacementTextarea.selectionEnd).toBe(9);
    expect(replacementTextarea.selectionDirection).toBe("backward");

    // 3. Outside container or missing ID returns null; removed control safely no-ops
    outside.focus();
    expect(captureActiveControlSelection(document, container)).toBeNull();

    const nameless = document.createElement("input");
    container.replaceChildren(nameless);
    nameless.focus();
    expect(captureActiveControlSelection(document, container)).toBeNull();

    container.replaceChildren();
    expect(() =>
      restoreActiveControlSelection(document, textareaSnapshot),
    ).not.toThrow();
  });
});

describe("SessionQueueController, ApprovalController, CapacityReleaseController, and SharedViewController integration", () => {
  const sampleRecord: IdentityRecord = {
    schemaVersion: 1,
    id: "primary",
    origin: "https://node.local",
    nodeId: "node-id",
    nodePublicKey: "node-key",
    clientId: "client-id",
    publicKey: "client-key",
    privateKey: {} as CryptoKey,
    displayName: "Web Client",
    status: "approved",
    pairingId: "pair-1",
    pendingClaim: null,
    createdAt: 1000,
    updatedAt: 1000,
  };

  it("session queue, approval, and capacity release panels respect injected getDocument without global document", () => {
    restoreDoc?.();
    restoreDoc = null;
    expect((globalThis as { document?: unknown }).document).toBeUndefined();

    const customDoc = new FakeDocument() as unknown as Document;
    const port: SessionQueueHostPort = {
      getManagement: () => null,
      getManagementGeneration: () => 1,
      getOpenedSessionView: () => null,
      getSharedViewSpec: () => null,
      getBodyPreview: () => "preview-text",
      getDocument: () => customDoc,
      onTargetSelected: vi.fn(),
      onCloseTargetSharedView: vi.fn(),
      onOpenQueueBody: vi.fn(),
      renderDashboard: vi.fn(),
    };

    const queue = new SessionQueueController(port);
    queue.inspectedQueueTargetId = encodeBase64url(new Uint8Array(16).fill(7));
    const queuePanel = queue.sessionQueuePanel(sampleRecord);
    expect(queuePanel.tagName).toBe("SECTION");
    expect(queuePanel.querySelector("#session-target-id")).not.toBeNull();
    expect(queuePanel.querySelector("#session-input-body")).not.toBeNull();

    const approval = new ApprovalController({
      getManagement: () => null,
      getManagementGeneration: () => 1,
      getDocument: () => customDoc,
      renderDashboard: vi.fn(),
    });
    expect(approval.sessionApprovalPanel(sampleRecord).tagName).toBe("SECTION");

    const release = new CapacityReleaseController({
      getManagement: () => null,
      getManagementGeneration: () => 1,
      getInspectedQueueTargetId: () => null,
      getDocument: () => customDoc,
      renderDashboard: vi.fn(),
    });
    expect(release.nativeReleasePanel(sampleRecord).tagName).toBe("SECTION");

    queue.reset();
    expect(queue.inspectedQueueTargetId).toBeNull();
  });

  it("shared view controller persists, auto-restores via restorePersistedSourceViewIfIdle when idle, skips when busy, and clears on close", async () => {
    const map = new Map<string, string>();
    const fakeStorage = {
      getItem: (key: string) => map.get(key) ?? null,
      setItem: (key: string, val: string) => {
        map.set(key, val);
      },
      removeItem: (key: string) => {
        map.delete(key);
      },
    };
    const dashboard = vi.fn();
    const connection = {
      openSessionView: vi.fn(async () => ({
        streamId: "stream-target",
        snapshotRevision: "0",
        status: { mode: "current_only", historyIncomplete: false },
      })),
      openSourceSessionView: vi.fn(async () => ({
        streamId: "stream-source",
        snapshotRevision: "0",
        status: { mode: "current_only", historyIncomplete: false },
      })),
      sendSessionViewCredit: vi.fn(),
      detachSessionStream: vi.fn(),
      closeSessionView: vi.fn(async () => ({})),
    };

    const makePort = (): SharedViewHostPort => ({
      getManagement: () => connection as unknown as ManagementConnection,
      getManagementGeneration: () => 1,
      getInspectedQueueTargetId: () => null,
      getSharedViewHost: () => ({ append: vi.fn() }),
      renderDashboard: dashboard,
      persistSourceViewSpec: (sel, id) =>
        persistSourceViewSpec(sel, id, fakeStorage),
      clearPersistedSourceViewSpec: () =>
        clearPersistedSourceViewSpec(fakeStorage),
      displayFactory: (_doc, opened) => ({
        streamId: opened.streamId,
        element: { dataset: {}, removeAttribute: vi.fn() },
        start: vi.fn(),
        suspend: vi.fn(),
        dispose: vi.fn(),
      }),
    });

    const id16 = (byte: number) =>
      encodeBase64url(new Uint8Array(16).fill(byte));
    const selection = {
      sourceId: id16(1),
      harnessId: id16(2),
      profileId: id16(3),
      profileRevision: "1",
      workspaceId: id16(4),
      launchPath: "/workspace/demo",
    };

    const controller = new SharedViewController(makePort());
    await controller.openSharedSourceView(
      {} as never,
      selection,
      "native-123",
      null,
    );
    expect(controller.sharedViewSpec).toEqual({
      kind: "source",
      selection,
      nativeSessionId: "native-123",
    });

    // 1. Idle fresh controller restores persisted source view via restorePersistedSourceViewIfIdle
    const idleController = new SharedViewController(makePort());
    const loadSpy = vi.fn(() => loadPersistedSourceViewSpec(fakeStorage));
    const restored = idleController.restorePersistedSourceViewIfIdle(
      loadSpy,
      null,
    );
    expect(loadSpy).toHaveBeenCalledOnce();
    expect(restored).toEqual({
      kind: "source",
      selection,
      nativeSessionId: "native-123",
    });
    expect(idleController.sharedViewSpec).toEqual(restored);

    // 2. Non-idle: inspectedQueueTargetId !== null skips restore without reading storage
    const queueBusyController = new SharedViewController(makePort());
    const skippedForQueue = vi.fn(() =>
      loadPersistedSourceViewSpec(fakeStorage),
    );
    expect(
      queueBusyController.restorePersistedSourceViewIfIdle(
        skippedForQueue,
        id16(9),
      ),
    ).toBeNull();
    expect(skippedForQueue).not.toHaveBeenCalled();
    expect(queueBusyController.sharedViewSpec).toBeNull();

    // 3. Non-idle: active target sharedViewSpec already present skips restore
    const viewBusyController = new SharedViewController(makePort());
    await viewBusyController.openTargetSharedView(
      {} as never,
      id16(8),
      null,
    );
    const skippedForActiveView = vi.fn(() =>
      loadPersistedSourceViewSpec(fakeStorage),
    );
    expect(
      viewBusyController.restorePersistedSourceViewIfIdle(
        skippedForActiveView,
        null,
      ),
    ).toBeNull();
    expect(skippedForActiveView).not.toHaveBeenCalled();
    expect(viewBusyController.sharedViewSpec).toEqual({
      kind: "target",
      targetId: id16(8),
    });

    // 4. Closing source view clears persisted storage so subsequent idle restore returns null
    await controller.closeSharedSourceView(
      {} as never,
      controller.openedSessionView!,
      { disabled: true } as HTMLButtonElement,
    );
    expect(loadPersistedSourceViewSpec(fakeStorage)).toBeNull();
    const afterCloseController = new SharedViewController(makePort());
    expect(
      afterCloseController.restorePersistedSourceViewIfIdle(
        () => loadPersistedSourceViewSpec(fakeStorage),
        null,
      ),
    ).toBeNull();
  });
});
