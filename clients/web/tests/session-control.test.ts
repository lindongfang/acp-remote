import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import {
  ManagementRequestError,
  SessionConsumeOutcomeError,
  type ManagementConnection,
} from "../src/management/client";
import { managementRequestErrorMessage } from "../src/management/error-messages";
import {
  parseManagementFrame,
  type SessionControlChangedFrame,
} from "../src/management/protocol";
import {
  editSessionText,
  SessionInputActionError,
  submitSessionText,
} from "../src/management/session-input-actions";
import {
  encodeSessionCommand,
  parseSessionCommand,
  parseSessionControlSnapshot,
  SessionProtocolError,
  type SessionControlSnapshot,
} from "../src/management/session-protocol";
import {
  captureActiveControlSelection,
  restoreActiveControlSelection,
} from "../src/workbench/dom-primitives";
import {
  managementErrorMessage,
  sessionActionErrorMessage,
} from "../src/workbench/error-formatting";
import { ApprovalController } from "../src/workbench/approval-controller";
import { ManagementWorkbench } from "../src/workbench/management-workbench";
import { SessionControlController } from "../src/workbench/session-control-controller";
import { SessionQueueController } from "../src/workbench/session-queue-controller";
import type { IdentityRecord, IdentityVault } from "../src/pairing/identity";
import { encodeBase64url, type JsonValue } from "../src/protocol/jws";
import { FakeElement, installFakeDocument } from "./support/fake-dom";

const DECODER = new TextDecoder();

function readRepoFile(relPath: string): Uint8Array {
  return readFileSync(new URL(`../../../${relPath}`, import.meta.url));
}

function readRepoText(relPath: string): string {
  return DECODER.decode(readRepoFile(relPath));
}

interface FsEntryLike {
  readonly name: string;
  isDirectory(): boolean;
  isFile(): boolean;
}

async function listSrcTsRelPaths(relDir: string): Promise<string[]> {
  const fsPromisesSpecifier = ["node", "fs/promises"].join(":");
  const fsPromises = (await import(
    /* @vite-ignore */ fsPromisesSpecifier
  )) as {
    readdir(
      url: URL,
      opts: { withFileTypes: true },
    ): Promise<readonly FsEntryLike[]>;
  };
  const entries = await fsPromises.readdir(
    new URL(`../../../${relDir}/`, import.meta.url),
    { withFileTypes: true },
  );
  const out: string[] = [];
  for (const entry of [...entries].sort((a, b) => a.name.localeCompare(b.name))) {
    const child = `${relDir}/${entry.name}`;
    if (entry.isDirectory()) {
      out.push(...(await listSrcTsRelPaths(child)));
    } else if (entry.isFile() && entry.name.endsWith(".ts")) {
      out.push(child);
    }
  }
  return out;
}

function extractRelativeImports(sourceText: string): string[] {
  const stripped = sourceText
    .replace(/\/\*[\s\S]*?\*\//gu, "")
    .replace(/\/\/[^\n]*/gu, "");
  const specifiers: string[] = [];
  const re = /(?:^|[;\n])\s*(?:import|export)\s+(?:type\s+)?(?:[\s\S]*?\s+from\s+)?["'](\.[^"']+)["']/gu;
  for (const match of stripped.matchAll(re)) {
    const spec = match[1];
    if (spec !== undefined) specifiers.push(spec);
  }
  return specifiers;
}

function resolveRelativeTs(fromRel: string, spec: string): string | undefined {
  const parts = fromRel.split("/").slice(0, -1);
  for (const seg of spec.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") parts.pop();
    else parts.push(seg);
  }
  return parts.join("/");
}

function id16(byte: number): string {
  return encodeBase64url(new Uint8Array(16).fill(byte));
}

function makeRecord(clientId = id16(2)): IdentityRecord {
  return {
    schemaVersion: 1,
    id: "primary",
    origin: "https://node.example.test",
    nodeId: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    nodePublicKey: id16(1),
    clientId,
    publicKey: id16(3),
    privateKey: {} as CryptoKey,
    displayName: "Client A",
    status: "approved",
    pairingId: id16(4),
    pendingClaim: null,
    createdAt: 1700000000,
    updatedAt: 1700000000,
  };
}

function collectButtons(root: FakeElement): FakeElement[] {
  const buttons: FakeElement[] = [];
  if (root.tagName === "BUTTON") {
    buttons.push(root);
  }
  for (const child of root.children) {
    if (child instanceof FakeElement) {
      buttons.push(...collectButtons(child));
    }
  }
  return buttons;
}

describe("SCT-001 Web protocol closure, expectedControlEpoch matrix, and error mapping", () => {
  it("validates closed SessionControlSnapshot and rejects malformed/mismatched fields", () => {
    const valid: SessionControlSnapshot = {
      targetId: id16(10),
      rootTargetId: id16(10),
      sourceId: id16(11),
      rootNativeSessionId: "root-native-1",
      controllerClientId: id16(2),
      controllerDisplayName: "Desktop A",
      controlEpoch: "18446744073709551615",
    };
    expect(parseSessionControlSnapshot(valid as unknown as JsonValue)).toEqual(valid);

    const uncontrolled = parseSessionControlSnapshot({
      targetId: null,
      rootTargetId: null,
      sourceId: id16(11),
      rootNativeSessionId: "root-native-1",
      controllerClientId: null,
      controllerDisplayName: null,
      controlEpoch: "0",
    });
    expect(uncontrolled.controllerClientId).toBeNull();
    expect(uncontrolled.controlEpoch).toBe("0");

    expect(() =>
      parseSessionControlSnapshot({
        ...valid,
        controllerDisplayName: null,
      } as unknown as JsonValue),
    ).toThrow(SessionProtocolError);
    expect(() =>
      parseSessionControlSnapshot({
        ...valid,
        controllerClientId: null,
      } as unknown as JsonValue),
    ).toThrow(SessionProtocolError);

    expect(() =>
      parseSessionControlSnapshot({ ...valid, extra: true } as unknown as JsonValue),
    ).toThrow(SessionProtocolError);
    expect(() =>
      parseSessionControlSnapshot({ ...valid, controlEpoch: "01" } as unknown as JsonValue),
    ).toThrow(SessionProtocolError);
    expect(() =>
      parseSessionControlSnapshot({
        ...valid,
        controlEpoch: "18446744073709551616",
      } as unknown as JsonValue),
    ).toThrow(SessionProtocolError);
    expect(() =>
      parseSessionControlSnapshot({ ...valid, rootNativeSessionId: "" } as unknown as JsonValue),
    ).toThrow(SessionProtocolError);
  });

  it("enforces closed wire-codec rules for session.control.* and expectedControlEpoch (omitted vs valid vs explicit null)", () => {
    const targetId = id16(10);
    const requestId = id16(20);
    const bodyId = id16(21);
    const approvalId = id16(22);
    const choiceId = id16(23);
    const approvalRegistrationId = id16(24);

    for (const action of [
      "session.control.inspect",
      "session.control.takeover",
      "session.control.release",
    ] as const) {
      const encoded = encodeSessionCommand({
        v: 1,
        type: "command",
        requestId,
        action,
        payload: { targetId },
      });
      const parsed = parseSessionCommand(encoded);
      expect(parsed.action).toBe(action);
      expect(parsed.payload).toEqual({ targetId });
    }

    const releaseWithEpoch = parseSessionCommand(
      JSON.stringify({
        v: 1,
        type: "command",
        requestId,
        action: "session.control.release",
        payload: { targetId, expectedControlEpoch: "42" },
      }),
    );
    expect(releaseWithEpoch.payload).toEqual({
      targetId,
      expectedControlEpoch: "42",
    });

    const legalSamples = [
      {
        action: "input.register",
        payload: { targetId, operation: { kind: "submit", bodyId } },
      },
      {
        action: "input.register",
        payload: {
          targetId,
          operation: { kind: "submit", bodyId },
          expectedControlEpoch: "18446744073709551615",
        },
      },
      {
        action: "input.consume",
        payload: {
          targetId,
          registrationId: requestId,
          bodyId: null,
          expectedControlEpoch: "3",
        },
      },
      {
        action: "approval.register",
        payload: { approvalId, choiceId, expectedControlEpoch: "1" },
      },
      {
        action: "approval.decide",
        payload: { approvalRegistrationId, expectedControlEpoch: "1" },
      },
    ];
    for (const sample of legalSamples) {
      expect(() =>
        parseSessionCommand(
          JSON.stringify({ v: 1, type: "command", requestId, ...sample }),
        ),
      ).not.toThrow();
    }

    const badEpochs: unknown[] = [null, 1, "01", "-1", "18446744073709551616", ""];
    for (const bad of badEpochs) {
      for (const base of [
        {
          action: "session.control.release",
          payload: { targetId, expectedControlEpoch: bad },
        },
        {
          action: "input.register",
          payload: {
            targetId,
            operation: { kind: "resume" },
            expectedControlEpoch: bad,
          },
        },
        {
          action: "input.consume",
          payload: {
            targetId,
            registrationId: requestId,
            bodyId: null,
            expectedControlEpoch: bad,
          },
        },
        {
          action: "approval.register",
          payload: { approvalId, choiceId, expectedControlEpoch: bad },
        },
        {
          action: "approval.decide",
          payload: { approvalRegistrationId, expectedControlEpoch: bad },
        },
      ]) {
        expect(() =>
          parseSessionCommand(
            JSON.stringify({ v: 1, type: "command", requestId, ...base }),
          ),
        ).toThrow(SessionProtocolError);
      }
    }

    expect(() =>
      parseSessionCommand(
        JSON.stringify({
          v: 1,
          type: "command",
          requestId,
          action: "session.control.takeover",
          payload: { targetId, force: true },
        }),
      ),
    ).toThrow(SessionProtocolError);
  });

  it("parses session.control.changed push frame, single-key session_not_controlled error, and Chinese messages", () => {
    const push: SessionControlChangedFrame = {
      v: 1,
      type: "session.control.changed",
      rootTargetId: id16(10),
      sourceId: id16(11),
      rootNativeSessionId: "root-native-1",
      controllerClientId: id16(2),
      controllerDisplayName: "Client A",
      controlEpoch: "1",
    };
    expect(parseManagementFrame(JSON.stringify(push))).toEqual(push);
    expect(() =>
      parseManagementFrame(JSON.stringify({ ...push, extra: 1 })),
    ).toThrow();
    expect(() =>
      parseManagementFrame(JSON.stringify({ ...push, controllerDisplayName: null })),
    ).toThrow();

    const errResult = parseManagementFrame(
      JSON.stringify({
        v: 1,
        type: "result",
        requestId: id16(20),
        ok: false,
        result: null,
        error: { code: "session_not_controlled" },
      }),
    );
    expect(errResult.type).toBe("result");
    if (errResult.type === "result") {
      expect(errResult.ok).toBe(false);
      expect(errResult.error).toEqual({ code: "session_not_controlled" });
    }

    const msg = managementRequestErrorMessage("session_not_controlled");
    expect(msg).toContain("控制权");
    expect(
      managementErrorMessage(new ManagementRequestError("session_not_controlled")),
    ).toBe(msg);
    expect(
      sessionActionErrorMessage(
        new SessionInputActionError("consume_refused", "session_not_controlled"),
      ),
    ).toBe(msg);
    expect(
      managementErrorMessage(
        new SessionConsumeOutcomeError("consume_refused", "session_not_controlled"),
      ),
    ).toBe(msg);
  });
});

describe("SCT-002 Web line budgets, zero cycles, fixture hash sync, and read-only draft protection", () => {
  it("enforces <= 120 line barrel, <= 650 line module limits, zero cycles/reverse dependencies, and FIXTURE-SHA256 match", async () => {
    const barrelLines = readRepoText("clients/web/src/management/session-protocol.ts").split("\n").length;
    expect(barrelLines).toBeLessThanOrEqual(120);

    for (const subdir of [
      "clients/web/src/management/session-protocol",
      "clients/web/src/workbench",
    ]) {
      for (const file of await listSrcTsRelPaths(subdir)) {
        const lines = readRepoText(file).split("\n").length;
        expect(lines, `${file} exceeds 650 lines (${lines})`).toBeLessThanOrEqual(650);
      }
    }

    const files = await listSrcTsRelPaths("clients/web/src");
    const fileSet = new Set(files);
    const adj = new Map<string, string[]>();
    const layerOf = (relFile: string): number => {
      const sub = relFile.slice("clients/web/src/".length);
      if (sub === "main.ts") return 4;
      if (sub.startsWith("workbench/")) return 3;
      if (sub.startsWith("management/session-protocol")) return 2;
      if (
        sub === "management/protocol.ts" ||
        sub === "management/error-messages.ts" ||
        sub === "management/input-transfer.ts" ||
        sub === "management/session-body-read.ts"
      ) {
        return 1.5;
      }
      if (sub.startsWith("management/")) return 2.5;
      return 1;
    };

    for (const file of files) {
      const content = readRepoText(file);
      const targets: string[] = [];
      for (const spec of extractRelativeImports(content)) {
        const base = resolveRelativeTs(file, spec);
        if (base === undefined) continue;
        const candidates = [`${base}.ts`, base, `${base}/index.ts`];
        const hit = candidates.find((c) => fileSet.has(c));
        expect(hit, `unresolved import ${spec} in ${file}`).toBeDefined();
        if (hit !== undefined) {
          targets.push(hit);
          if (
            file === "clients/web/src/management/protocol.ts" &&
            hit.includes("/session-protocol")
          ) {
            throw new Error(`protocol.ts must not depend on session-protocol: ${hit}`);
          }
          expect(
            layerOf(file),
            `reverse layer import ${file} -> ${hit}`,
          ).toBeGreaterThanOrEqual(layerOf(hit));
        }
      }
      adj.set(file, targets);
    }

    const color = new Map<string, 0 | 1 | 2>();
    const visit = (node: string, stack: string[]): void => {
      color.set(node, 1);
      for (const next of adj.get(node) ?? []) {
        const state = color.get(next) ?? 0;
        if (state === 1) {
          throw new Error(`dependency cycle detected: ${[...stack, next].join(" -> ")}`);
        }
        if (state === 0) visit(next, [...stack, next]);
      }
      color.set(node, 2);
    };
    for (const file of files) {
      if ((color.get(file) ?? 0) === 0) visit(file, [file]);
    }

    const shaDoc = readRepoText("clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md");
    for (const line of shaDoc.trim().split(/\r?\n/u)) {
      const m = line.match(/^(\d+)\s+bytes\s+([0-9a-f]{64})\s+(\S+)$/u);
      expect(m, `malformed FIXTURE-SHA256 line: ${line}`).not.toBeNull();
      const expectedBytes = m?.[1];
      const expectedHex = m?.[2];
      const relPath = m?.[3];
      if (expectedBytes === undefined || expectedHex === undefined || relPath === undefined) {
        throw new Error(`missing capture group on FIXTURE-SHA256 line: ${line}`);
      }
      const raw = readRepoFile(relPath);
      expect(String(raw.length)).toBe(expectedBytes);
      expect(createHash("sha256").update(raw).digest("hex")).toBe(expectedHex);
    }
  });

  it("converges control snapshots by monotonic BigInt epoch and resets across reconnects", async () => {
    const installed = installFakeDocument();
    try {
      const record = makeRecord(id16(2));
      const t1 = id16(30);
      const tChild = id16(31);
      const src = id16(40);
      let inspectResolve!: (snap: SessionControlSnapshot) => void;
      const connection = {
        clientId: id16(2),
        inspectSessionControl: vi.fn(
          () =>
            new Promise<SessionControlSnapshot>((resolve) => {
              inspectResolve = resolve;
            }),
        ),
        takeoverSessionControl: vi.fn(),
        releaseSessionControl: vi.fn(),
      } as unknown as ManagementConnection;

      const controller = new SessionControlController({
        getManagement: () => connection,
        getManagementGeneration: () => 1,
        getCurrentRecord: () => record,
        getActiveControlTargetId: () => tChild,
        getDocument: () => installed.doc,
        renderDashboard: vi.fn(),
      });

      const pendingInspect = controller.inspectTarget(tChild, 1);
      controller.handleControlChanged(record, 1, {
        v: 1,
        type: "session.control.changed",
        rootTargetId: t1,
        sourceId: src,
        rootNativeSessionId: "root-mono",
        controllerClientId: id16(99),
        controllerDisplayName: "Tablet B",
        controlEpoch: "18446744073709551615",
      });
      inspectResolve({
        targetId: tChild,
        rootTargetId: t1,
        sourceId: src,
        rootNativeSessionId: "root-mono",
        controllerClientId: id16(2),
        controllerDisplayName: "Client A",
        controlEpoch: "18446744073709551614",
      });
      await pendingInspect;

      expect(controller.controlEpochForTarget(tChild)).toBe("18446744073709551615");
      expect(controller.canWriteTarget(tChild, record)).toBe(false);

      controller.handleControlChanged(record, 1, {
        v: 1,
        type: "session.control.changed",
        rootTargetId: t1,
        sourceId: src,
        rootNativeSessionId: "root-mono",
        controllerClientId: null,
        controllerDisplayName: null,
        controlEpoch: "0",
      });
      expect(controller.controlEpochForTarget(t1)).toBe("18446744073709551615");

      controller.resetForConnection();
      controller.handleControlChanged(record, 1, {
        v: 1,
        type: "session.control.changed",
        rootTargetId: t1,
        sourceId: src,
        rootNativeSessionId: "root-mono",
        controllerClientId: null,
        controllerDisplayName: null,
        controlEpoch: "0",
      });
      expect(controller.controlEpochForTarget(t1)).toBe("0");
    } finally {
      installed.restore();
    }
  });

  it("disables write buttons in non-controller states while keeping #session-input-body focusable and preserving draft/selection", async () => {
    const installed = installFakeDocument();
    try {
      const record = makeRecord(id16(2));
      const targetId = id16(50);
      const itemId = id16(51);
      const sourceId = id16(52);
      const approvalId = id16(53);
      const choiceId = id16(54);
      const regId = id16(55);

      const panel = installed.fakeDoc.createElement("section") as unknown as HTMLElement;
      panel.id = "access-panel";
      const sharedViewHost = installed.fakeDoc.createElement("div") as unknown as HTMLElement;
      sharedViewHost.id = "session-view-cache";
      installed.fakeDoc.body.append(panel as never, sharedViewHost as never);

      const currentControl: SessionControlSnapshot = {
        targetId,
        rootTargetId: targetId,
        sourceId,
        rootNativeSessionId: "root-ui-1",
        controllerClientId: record.clientId,
        controllerDisplayName: "Client A",
        controlEpoch: "1",
      };

      const sampleApproval = {
        approvalId,
        source: { protocolVersion: 1 as const, instanceId: id16(60), connectionEpoch: "1" },
        context: { kind: "session_request" as const, targetId },
        status: {
          approvalId,
          changeSequence: "1",
          validity: "valid" as const,
          association: { kind: "registered_session" as const, targetId },
          decision: null,
          delivery: "pending" as const,
          sourceClose: "open" as const,
          necessaryFactsMissing: false,
          reconciliationRunning: false,
        },
        limitations: [],
        content: { availability: "available" as const, rawJson: '{"tool":"exec"}' },
        choices: [
          {
            ordinal: 1,
            choiceId,
            originalOptionId: "allow",
            originalKind: "allow_once",
            selectable: true,
          },
        ],
      };

      const fakeConnection = {
        clientId: record.clientId,
        connect: vi.fn(async () => ({ clientId: record.clientId!, connectionId: id16(9) })),
        disconnect: vi.fn(),
        nodeInfo: vi.fn(async () => ({
          nodeId: record.nodeId!,
          publicKey: record.nodePublicKey!,
          origin: record.origin,
          status: "ready" as const,
        })),
        subscribeDevices: vi.fn(async () => undefined),
        listDevices: vi.fn(async () => ({ revision: 1, items: [], nextCursor: null })),
        inspectCatalogInventory: vi.fn(async () => ({
          sources: [],
          harnesses: [],
          profiles: [],
          workspaces: [],
          targets: [],
        })),
        inspectSessionControl: vi.fn(async () => currentControl),
        takeoverSessionControl: vi.fn(async () => currentControl),
        releaseSessionControl: vi.fn(async () => currentControl),
        inspectSessionQueue: vi.fn(async () => ({
          targetId,
          failureSuspended: false,
          stopReason: "cancelled" as const,
          priorityFailure: null,
          waiting: [
            {
              itemId,
              revision: "1",
              submitterId: record.clientId!,
              editorId: record.clientId!,
              suspended: true,
            },
          ],
          active: null,
          priority: null,
          processing: null,
        })),
        listApprovals: vi.fn(async () => ({ items: [sampleApproval] })),
      };

      let onControlChanged: ((f: SessionControlChangedFrame) => void) | undefined;
      const workbench = new ManagementWorkbench({
        panel,
        sharedViewHost,
        doc: installed.doc,
        location: {
          protocol: "https:",
          origin: record.origin,
          host: "node.example.test",
        },
        createVault: () =>
          ({
            initialize: vi.fn(async () => record),
            save: vi.fn(async (n: IdentityRecord) => n),
            fingerprint: vi.fn(async () => "fp"),
          }) as unknown as IdentityVault,
        connectionFactory: (_id, _key, opts) => {
          onControlChanged = opts.callbacks?.onSessionControlChanged;
          return fakeConnection as unknown as ManagementConnection;
        },
        loadPersistedSourceView: () => null,
      });

      await workbench.boot();
      workbench.sessionQueue.inspectedQueueTargetId = targetId;
      await workbench.sessionQueue.loadSessionQueue(
        record,
        targetId,
        workbench.managementGeneration,
      );
      workbench.sessionQueue.editingQueueEntry = {
        itemId,
        revision: "1",
        submitterId: record.clientId!,
        editorId: record.clientId!,
        suspended: true,
      };
      await workbench.approval.loadApprovalList(record, workbench.managementGeneration);
      (fakeConnection as Record<string, unknown>).readApproval = vi.fn(async () => sampleApproval);
      (fakeConnection as Record<string, unknown>).registerApproval = vi.fn(async () => ({
        approvalId,
        approvalRegistrationId: regId,
        choiceId,
      }));
      await workbench.approval.loadApproval(
        record,
        approvalId,
        installed.fakeDoc.createElement("button") as unknown as HTMLButtonElement,
      );
      await workbench.approval.registerApprovalChoice(
        record,
        approvalId,
        choiceId,
        installed.fakeDoc.createElement("button") as unknown as HTMLButtonElement,
      );

      const fakePanel = panel as unknown as FakeElement;
      expect(fakePanel.textContent).toContain("当前由本设备接管 · 代际 1");
      for (const label of ["上传并提交", "上传并保存编辑", "继续旧队列", "编辑", "立即发送", "取消", "提交已登记决定", "allow_once"]) {
        const btn = collectButtons(fakePanel).find((b) =>
          label === "allow_once" ? b.textContent.includes(label) : b.textContent === label,
        );
        expect(btn, `self missing button ${label}`).toBeDefined();
        expect(btn!.disabled, `self button ${label} should be enabled`).toBe(false);
      }

      const textarea = fakePanel.findById("session-input-body")!;
      textarea.value = "unsent draft text";
      textarea.focus();
      textarea.setSelectionRange(2, 9, "forward");
      expect(captureActiveControlSelection(installed.doc, panel)).not.toBeNull();

      onControlChanged?.({
        v: 1,
        type: "session.control.changed",
        rootTargetId: targetId,
        sourceId,
        rootNativeSessionId: "root-ui-1",
        controllerClientId: id16(99),
        controllerDisplayName: "Laptop B",
        controlEpoch: "2",
      });
      expect(fakePanel.textContent).toContain("只读观察 · 当前由 Laptop B 接管 · 代际 2");
      for (const label of ["上传并提交", "上传并保存编辑", "继续旧队列", "编辑", "立即发送", "取消", "提交已登记决定", "allow_once"]) {
        const btn = collectButtons(fakePanel).find((b) =>
          label === "allow_once" ? b.textContent.includes(label) : b.textContent === label,
        );
        expect(btn, `other missing button ${label}`).toBeDefined();
        expect(btn!.disabled, `other button ${label} must be disabled`).toBe(true);
      }
      for (const readLabel of ["读取正文", "刷新此队列", "打开共享视图", "刷新审批列表", "顶号接管会话"]) {
        const btn = collectButtons(fakePanel).find((b) => b.textContent.includes(readLabel));
        expect(btn, `other missing read/takeover button ${readLabel}`).toBeDefined();
        expect(btn!.disabled, `other button ${readLabel} must remain enabled`).toBe(false);
      }

      const restoredTextarea = fakePanel.findById("session-input-body")!;
      expect(restoredTextarea.disabled).toBe(false);
      expect(installed.fakeDoc.activeElement).toBe(restoredTextarea);
      expect(restoredTextarea.value).toBe("unsent draft text");
      expect(restoredTextarea.selectionStart).toBe(2);
      expect(restoredTextarea.selectionEnd).toBe(9);

      onControlChanged?.({
        v: 1,
        type: "session.control.changed",
        rootTargetId: targetId,
        sourceId,
        rootNativeSessionId: "root-ui-1",
        controllerClientId: null,
        controllerDisplayName: null,
        controlEpoch: "3",
      });
      expect(fakePanel.textContent).toContain("只读观察 · 当前无人接管 · 代际 3");
      const takeBtn = collectButtons(fakePanel).find((b) => b.textContent === "接管会话");
      expect(takeBtn?.disabled).toBe(false);
      const stillTextarea = fakePanel.findById("session-input-body")!;
      expect(stillTextarea.value).toBe("unsent draft text");
      expect(stillTextarea.selectionStart).toBe(2);
      expect(stillTextarea.selectionEnd).toBe(9);

      restoreActiveControlSelection(installed.doc, captureActiveControlSelection(installed.doc, panel));
      expect(SessionQueueController).toBeDefined();
      expect(ApprovalController).toBeDefined();
    } finally {
      installed.restore();
    }
  });

  it("resolves approval control target independently from queue target and enforces read-only gate", async () => {
    const installed = installFakeDocument();
    try {
      const record = makeRecord(id16(2));
      const tSelf = id16(70);
      const tOther = id16(71);
      const src = id16(72);
      const approvalId = id16(73);
      const choiceId = id16(74);

      const panel = installed.fakeDoc.createElement("section") as unknown as HTMLElement;
      panel.id = "access-panel";
      const sharedViewHost = installed.fakeDoc.createElement("div") as unknown as HTMLElement;
      sharedViewHost.id = "session-view-cache";
      installed.fakeDoc.body.append(panel as never, sharedViewHost as never);

      const approvalForOther = {
        approvalId,
        source: {
          sourceId: src,
          runtimeId: id16(75),
          instanceId: id16(76),
          connectionEpoch: "1",
          protocolVersion: "2" as const,
        },
        context: { kind: "session_request" as const },
        choices: [
          {
            choiceId,
            ordinal: "1",
            originalOptionId: "allow",
            originalKind: "allow_once",
            selectable: true,
          },
        ],
        content: { availability: "available" as const, rawJson: '{"tool":"exec"}' },
        limitations: [],
        status: {
          approvalId,
          changeSequence: "1",
          validity: "current_pending" as const,
          association: { kind: "registered_session" as const, targetId: tOther },
          decision: null,
          delivery: "not_attempted" as const,
          sourceClose: "open" as const,
          necessaryFactsMissing: false,
          reconciliationRunning: false,
        },
      };

      const inspectCalls: string[] = [];
      const fakeConnection = {
        clientId: record.clientId,
        connect: vi.fn(async () => ({ clientId: record.clientId!, connectionId: id16(9) })),
        disconnect: vi.fn(),
        nodeInfo: vi.fn(async () => ({
          nodeId: record.nodeId!,
          publicKey: record.nodePublicKey!,
          origin: record.origin,
          status: "ready" as const,
        })),
        subscribeDevices: vi.fn(async () => undefined),
        listDevices: vi.fn(async () => ({ revision: 1, items: [], nextCursor: null })),
        inspectCatalogInventory: vi.fn(async () => ({
          sources: [],
          harnesses: [],
          profiles: [],
          workspaces: [],
          targets: [],
        })),
        inspectSessionControl: vi.fn(async (targetId: string) => {
          inspectCalls.push(targetId);
          return targetId === tSelf
            ? {
                targetId: tSelf,
                rootTargetId: tSelf,
                sourceId: src,
                rootNativeSessionId: "root-self",
                controllerClientId: record.clientId,
                controllerDisplayName: "Client A",
                controlEpoch: "1",
              }
            : {
                targetId: tOther,
                rootTargetId: tOther,
                sourceId: src,
                rootNativeSessionId: "root-other",
                controllerClientId: id16(99),
                controllerDisplayName: "Tablet B",
                controlEpoch: "2",
              };
        }),
        listApprovals: vi.fn(async () => ({ items: [approvalForOther] })),
        readApproval: vi.fn(async () => approvalForOther),
        inspectSessionQueue: vi.fn(async () => ({
          targetId: tSelf,
          failureSuspended: false,
          stopReason: null,
          priorityFailure: null,
          waiting: [],
          active: null,
          priority: null,
          processing: null,
        })),
      };

      const workbench = new ManagementWorkbench({
        panel,
        sharedViewHost,
        doc: installed.doc,
        location: { protocol: "https:", origin: record.origin, host: "node.example.test" },
        createVault: () =>
          ({
            initialize: vi.fn(async () => record),
            save: vi.fn(async (n: IdentityRecord) => n),
            fingerprint: vi.fn(async () => "fp"),
          }) as unknown as IdentityVault,
        connectionFactory: () => fakeConnection as unknown as ManagementConnection,
        loadPersistedSourceView: () => null,
      });

      await workbench.boot();
      expect(workbench.sessionQueue.inspectedQueueTargetId).toBeNull();
      await workbench.approval.loadApprovalList(record, workbench.managementGeneration);
      await workbench.approval.loadApproval(
        record,
        approvalId,
        installed.fakeDoc.createElement("button") as unknown as HTMLButtonElement,
      );
      expect(inspectCalls).toEqual([tOther]);
      const fakePanel = panel as unknown as FakeElement;
      expect(fakePanel.textContent).toContain("只读观察 · 当前由 Tablet B 接管 · 代际 2");
      const choiceBtn = collectButtons(fakePanel).find((b) => b.textContent.includes("allow_once"));
      expect(choiceBtn?.disabled).toBe(true);

      // Even when the queue panel inspects self-controlled tSelf, approval for tOther stays read-only!
      workbench.sessionQueue.inspectedQueueTargetId = tSelf;
      await workbench.sessionQueue.loadSessionQueue(record, tSelf, workbench.managementGeneration);
      const choiceBtnWithSelfQueue = collectButtons(fakePanel).find((b) =>
        b.textContent.includes("allow_once"),
      );
      expect(choiceBtnWithSelfQueue?.disabled).toBe(true);
    } finally {
      installed.restore();
    }
  });

  it("SCT-005 releases private input upload staging when input.register rejects with session_not_controlled", async () => {
    const uploadId = id16(80);
    const bodyId = id16(81);
    const targetId = id16(82);
    const itemId = id16(83);
    const releasedUploads: string[] = [];
    const fakeConnection = {
      beginInputUpload: vi.fn(async () => ({ uploadId })),
      sendInputData: vi.fn(async (_id: string, _offset: bigint, chunk: Uint8Array) => ({
        uploadId,
        nextOffset: BigInt(chunk.length),
      })),
      finishInputUpload: vi.fn(async () => undefined),
      inputUploadStatus: vi.fn(async () => ({
        uploadId,
        phase: "ready" as const,
        bodyId,
      })),
      registerSessionInput: vi.fn(async () => {
        throw new ManagementRequestError("session_not_controlled");
      }),
      releaseInputUpload: vi.fn(async (id: string) => {
        releasedUploads.push(id);
      }),
    } as unknown as ManagementConnection;

    await expect(
      submitSessionText(fakeConnection, targetId, "hello after preemption", "1"),
    ).rejects.toThrow(ManagementRequestError);
    await expect(
      editSessionText(fakeConnection, targetId, itemId, "1", "edited after preemption", "1"),
    ).rejects.toThrow(ManagementRequestError);
    expect(releasedUploads).toEqual([uploadId, uploadId]);
  });
});
