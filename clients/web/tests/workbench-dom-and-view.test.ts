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

  it("captures and restores active control selection (WV-15 contract)", () => {
    const container = document.createElement("div");
    const input = document.createElement("input");
    input.id = "test-input";
    input.value = "hello world";
    container.append(input);
    document.body.append(container);

    input.focus();
    input.setSelectionRange(2, 8, "forward");

    const snapshot = captureActiveControlSelection(document, container);
    expect(snapshot).not.toBeNull();
    expect(snapshot?.id).toBe("test-input");
    expect(snapshot?.value).toBe("hello world");
    expect(snapshot?.start).toBe(2);
    expect(snapshot?.end).toBe(8);
    expect(snapshot?.direction).toBe("forward");

    // Replace container content with identical ID input
    container.replaceChildren();
    const replacement = document.createElement("input");
    replacement.id = "test-input";
    replacement.value = "hello world";
    container.append(replacement);

    restoreActiveControlSelection(document, snapshot);
    expect(document.activeElement).toBe(replacement);
    expect(replacement.selectionStart).toBe(2);
    expect(replacement.selectionEnd).toBe(8);
  });
});

describe("SessionQueueController and SharedViewController integration", () => {
  it("session queue controller manages target lifecycle and UI reset", async () => {
    const dashboard = vi.fn();
    const onTargetSelected = vi.fn();
    const port: SessionQueueHostPort = {
      getManagement: () =>
        ({
          inspectSessionQueue: vi.fn(async () => ({
            items: [],
            stopReason: null,
            paused: false,
          })),
        }) as unknown as ManagementConnection,
      getManagementGeneration: () => 1,
      getOpenedSessionView: () => null,
      getSharedViewSpec: () => null,
      getBodyPreview: () => null,
      onTargetSelected,
      onCloseTargetSharedView: vi.fn(),
      onOpenQueueBody: vi.fn(),
      renderDashboard: dashboard,
    };

    const queue = new SessionQueueController(port);
    expect(queue.inspectedQueueTargetId).toBeNull();
    expect(queue.inspectedQueue).toBeNull();

    const record: IdentityRecord = {
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

    const panel = queue.sessionQueuePanel(record);
    expect(panel.tagName).toBe("SECTION");
    expect(panel.querySelector("#session-target-id")).not.toBeNull();

    queue.reset();
    expect(queue.inspectedQueueTargetId).toBeNull();
    expect(queue.inspectedQueue).toBeNull();
  });

  it("shared view controller persists, restores, and clears source view spec via sessionStorage", async () => {
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

    // Simulate reload: fresh controller restores from sessionStorage
    const restoredSpec = loadPersistedSourceViewSpec(fakeStorage);
    expect(restoredSpec).toEqual({
      kind: "source",
      selection,
      nativeSessionId: "native-123",
    });
    const reloadedController = new SharedViewController(makePort());
    reloadedController.restoreSharedViewSpec(restoredSpec!);
    expect(reloadedController.sharedViewSpec).toEqual(restoredSpec);

    reloadedController.clearSharedSourceViewSpec();
    expect(loadPersistedSourceViewSpec(fakeStorage)).toBeNull();
    expect(reloadedController.sharedViewSpec).toBeNull();
  });
});
