import { describe, expect, it, vi } from "vitest";
import {
  ManagementRequestError,
  type ManagementConnection,
} from "../src/management/client";
import type { DevicePage } from "../src/management/protocol";
import {
  IdentityStoreError,
  type IdentityRecord,
  type IdentityVault,
} from "../src/pairing/identity";
import type { ViewDataFrame } from "../src/views/binary";
import { encodeBase64url } from "../src/protocol/jws";
import {
  FakeDocument,
  FakeElement,
  installFakeDocument,
} from "./support/fake-dom";

// Importing all 12 workbench modules at top level in pure Node (where globalThis.document is undefined)
// verifies SC1 / RWW-001 zero-DOM-side-effect import purity.
import * as DomPrimitives from "../src/workbench/dom-primitives";
import * as ErrorFormatting from "../src/workbench/error-formatting";
import { SharedViewController } from "../src/workbench/shared-view-controller";
import { QueuedBodyController } from "../src/workbench/queued-body-controller";
import { SessionQueueController } from "../src/workbench/session-queue-controller";
import {
  ApprovalController,
  approvalStatusText,
} from "../src/workbench/approval-controller";
import { CapacityReleaseController } from "../src/workbench/capacity-release-controller";
import {
  WorkspaceCatalogController,
  catalogActionErrorText,
} from "../src/workbench/workspace-catalog-controller";
import { SourceSessionController } from "../src/workbench/source-session-controller";
import { DeviceTrustController } from "../src/workbench/device-trust-panel";
import { PairingAccessController } from "../src/workbench/pairing-access-controller";
import { ManagementWorkbench } from "../src/workbench/management-workbench";

function id16(byte: number): string {
  return encodeBase64url(new Uint8Array(16).fill(byte));
}

function makeApprovedRecord(): IdentityRecord {
  return {
    schemaVersion: 1,
    id: "primary",
    origin: "https://node.example.test",
    nodeId: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    nodePublicKey: id16(1),
    clientId: id16(2),
    publicKey: id16(3),
    privateKey: {} as CryptoKey,
    displayName: "Workbench Test Client",
    status: "approved",
    pairingId: id16(4),
    pendingClaim: null,
    createdAt: 1700000000,
    updatedAt: 1700000000,
  };
}

function makeFrame(streamId: string, payloadBytes: Uint8Array): ViewDataFrame {
  return {
    type: "data",
    streamId,
    packetSequence: 1n,
    recordId: 1n,
    byteOffset: 0n,
    totalLength: BigInt(payloadBytes.length),
    kind: "original_raw",
    last: true,
    payload: payloadBytes,
  };
}

function findDataPanel(node: FakeElement, name: string): FakeElement | null {
  if (node.dataset.panel === name) return node;
  for (const child of node.children) {
    if (!(child instanceof FakeElement)) continue;
    const hit = findDataPanel(child, name);
    if (hit !== null) return hit;
  }
  return null;
}

describe("workbench import purity and module boundaries", () => {
  it("imports all 12 workbench modules in Node with zero global document requirement", () => {
    expect((globalThis as { document?: unknown }).document).toBeUndefined();
    expect(typeof DomPrimitives.captureActiveControlSelection).toBe("function");
    expect(typeof ErrorFormatting.managementErrorMessage).toBe("function");
    expect(typeof SharedViewController).toBe("function");
    expect(typeof QueuedBodyController).toBe("function");
    expect(typeof SessionQueueController).toBe("function");
    expect(typeof ApprovalController).toBe("function");
    expect(typeof CapacityReleaseController).toBe("function");
    expect(typeof WorkspaceCatalogController).toBe("function");
    expect(typeof SourceSessionController).toBe("function");
    expect(typeof DeviceTrustController).toBe("function");
    expect(typeof PairingAccessController).toBe("function");
    expect(typeof ManagementWorkbench).toBe("function");
  });

  it("formats approval status and catalog errors deterministically", () => {
    expect(
      approvalStatusText({
        validity: "valid",
        decision: { choiceId: "allow" },
        delivery: "delivered",
        sourceClose: "open",
      } as never),
    ).toContain("已裁决 allow");

    expect(catalogActionErrorText(new Error("boom"), () => "fallback")).toBe(
      "fallback",
    );
  });
});

describe("PairingAccessController and DeviceTrustController boundaries (RWW-007)", () => {
  it("serializes IdentityVault operations, self-heals on identity_changed, and resets session state on resetLocalIdentity", async () => {
    const installed = installFakeDocument();
    try {
      const panel = installed.fakeDoc.createElement(
        "section",
      ) as unknown as HTMLElement;
      const record = makeApprovedRecord();
      const order: string[] = [];
      let releaseFirst!: () => void;
      const firstGate = new Promise<void>((resolve) => {
        releaseFirst = resolve;
      });

      const fakeVault = {
        initialize: vi.fn(async () => record),
        replaceRejectedIdentity: vi.fn(async () => ({
          ...record,
          status: "unbound" as const,
          nodeId: null,
          nodePublicKey: null,
          clientId: null,
        })),
        fingerprint: vi.fn(async () => "abcd"),
      } as unknown as IdentityVault;

      const onResetManagementAndSessionState = vi.fn();
      const openManagement = vi.fn(async () => undefined);

      const pairing = new PairingAccessController({
        getPanel: () => panel,
        getDocument: () => installed.doc,
        getLocation: () => ({
          protocol: "https:",
          origin: "https://node.example.test",
          host: "node.example.test",
        }),
        isPageActive: () => true,
        isManuallyDisconnected: () => false,
        setManuallyDisconnected: vi.fn(),
        isAutomaticReconnectAllowed: () => true,
        setAutomaticReconnectAllowed: vi.fn(),
        getLastManagementError: () => null,
        setCurrentRecord: vi.fn(),
        showManagementConnecting: vi.fn(),
        resumeManagement: vi.fn(),
        openManagement,
        onResetManagementAndSessionState,
        createVault: () => fakeVault,
      });

      // 1. Serial mutex queue on runIdentityOperation
      const op1 = pairing.runIdentityOperation(async () => {
        order.push("op1:start");
        await firstGate;
        order.push("op1:end");
        return 1;
      });
      const op2 = pairing.runIdentityOperation(async () => {
        order.push("op2:start");
        order.push("op2:end");
        return 2;
      });
      await Promise.resolve();
      expect(order).toEqual(["op1:start"]);
      releaseFirst();
      expect(await Promise.all([op1, op2])).toEqual([1, 2]);
      expect(order).toEqual(["op1:start", "op1:end", "op2:start", "op2:end"]);

      // 2. boot initializes vault and opens management
      await pairing.boot();
      expect(fakeVault.initialize).toHaveBeenCalledTimes(1);
      expect(openManagement).toHaveBeenCalledTimes(1);

      // 3. recoverChangedIdentity resets state (resetReconnect = false) and re-boots
      expect(
        pairing.recoverChangedIdentity(new IdentityStoreError("identity_changed")),
      ).toBe(true);
      expect(onResetManagementAndSessionState).toHaveBeenCalledWith(false);
      await vi.waitFor(() => {
        expect(fakeVault.initialize).toHaveBeenCalledTimes(2);
      });

      // 4. resetLocalIdentity triggers full reset (resetReconnect = true) and replaces rejected identity
      onResetManagementAndSessionState.mockClear();
      await pairing.resetLocalIdentity(record);
      expect(onResetManagementAndSessionState).toHaveBeenCalledWith(true);
      expect(fakeVault.replaceRejectedIdentity).toHaveBeenCalledTimes(1);
      expect(panel.textContent).toContain("原配对已失效");
    } finally {
      installed.restore();
    }
  });

  it("coalesces concurrent refreshDevices requests and falls back to first page on stale_cursor", async () => {
    const installed = installFakeDocument();
    try {
      const panel = installed.fakeDoc.createElement(
        "section",
      ) as unknown as HTMLElement;
      const record = makeApprovedRecord();
      const cursors: Array<string | null> = [];
      let resolveFirst!: (page: DevicePage) => void;

      const listDevices = vi.fn((cursor: string | null) => {
        cursors.push(cursor);
        if (cursors.length === 1) {
          return new Promise<DevicePage>((resolve) => {
            resolveFirst = resolve;
          });
        }
        if (cursor === "stale-cur") {
          return Promise.reject(new ManagementRequestError("stale_cursor"));
        }
        return Promise.resolve({
          revision: cursors.length,
          items: [],
          nextCursor: "stale-cur",
        });
      });

      const renderDashboard = vi.fn();
      const devices = new DeviceTrustController({
        getManagement: () =>
          ({ listDevices }) as unknown as ManagementConnection,
        getManagementGeneration: () => 1,
        getCurrentRecord: () => record,
        getPanel: () => panel,
        getDocument: () => installed.doc,
        onManualDisconnect: vi.fn(),
        renderDashboard,
      });

      // Concurrent refresh while first request is in-flight coalesces via refreshAfterCurrentRequest
      const p1 = devices.refreshDevices(true, 1);
      const p2 = devices.refreshDevices(true, 1);
      expect(listDevices).toHaveBeenCalledTimes(1);
      resolveFirst({ revision: 1, items: [], nextCursor: "stale-cur" });
      await p1;
      await p2;
      await vi.waitFor(() => {
        expect(listDevices).toHaveBeenCalledTimes(2);
      });
      expect(cursors).toEqual([null, null]);

      // Next-page request hitting stale_cursor automatically retries from cursor = null
      await devices.refreshDevices(false, 1);
      await vi.waitFor(() => {
        expect(cursors).toEqual([null, null, "stale-cur", null]);
      });
    } finally {
      installed.restore();
    }
  });
});

describe("ManagementWorkbench coordinator, dashboard order, frame routing, and lifecycle (RWW-007 / RWW-008)", () => {
  it("authenticates, renders all dashboard sections in exact order, routes view frames, and cleans up deterministically", async () => {
    const installed = installFakeDocument();
    try {
      const fakeDoc: FakeDocument = installed.fakeDoc;
      const panel = fakeDoc.createElement("section") as unknown as HTMLElement;
      panel.id = "access-panel";
      const sharedViewHost = fakeDoc.createElement(
        "div",
      ) as unknown as HTMLElement;
      sharedViewHost.id = "session-view-cache";
      fakeDoc.body.append(panel as never, sharedViewHost as never);

      const record = makeApprovedRecord();
      const fakeVault = {
        initialize: vi.fn(async () => record),
        save: vi.fn(async (next: IdentityRecord) => next),
        fingerprint: vi.fn(
          async () =>
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
        ),
        replaceRejectedIdentity: vi.fn(async () => ({
          ...record,
          status: "unbound" as const,
          nodeId: null,
          nodePublicKey: null,
          clientId: null,
        })),
      } as unknown as IdentityVault;

      const sharedAccept = vi.fn();
      const sharedDispose = vi.fn();
      const sharedSuspend = vi.fn();
      const bodyAccept = vi.fn(() => ({ state: "pending" as const }));
      const bodySeal = vi.fn();

      const fakeConnection = {
        connect: vi.fn(async () => ({
          clientId: record.clientId!,
          connectionId: id16(9),
        })),
        disconnect: vi.fn(),
        nodeInfo: vi.fn(async () => ({
          origin: "https://node.example.test",
          status: "ready",
        })),
        subscribeDevices: vi.fn(async () => undefined),
        listDevices: vi.fn(async () => ({
          revision: 1,
          items: [
            {
              clientId: record.clientId!,
              displayName: "Workbench Test Client",
              publicKeyFingerprint: "fp-1",
              status: "trusted" as const,
              createdAt: 1700000000,
              revokedAt: null,
            },
          ],
          nextCursor: null,
        })),
        inspectCatalogInventory: vi.fn(async () => ({
          sources: [],
          harnesses: [],
          profiles: [],
          workspaces: [],
          targets: [],
        })),
        openSessionView: vi.fn(async () => ({
          v: 1,
          type: "view.opened",
          requestId: id16(10),
          streamId: id16(20),
          viewEpoch: id16(21),
          snapshotRevision: "0",
          capabilities: { opaqueChunks: 1 },
          status: {
            mode: "current_only",
            freshness: "current_observation",
            projection: "available",
            join: "unassociated_segments",
            historyIncomplete: false,
            cacheRecoverable: false,
          },
        })),
        openSessionBody: vi.fn(async () => ({
          streamId: id16(30),
          totalBytes: "5",
        })),
        registerSourceSession: vi.fn(async () => ({
          sourceOperationId: id16(40),
          actorId: record.clientId!,
          action: "discover" as const,
          sourceId: id16(1),
          harnessId: id16(2),
          profileId: id16(3),
          profileRevision: "1",
          workspaceId: id16(4),
          registeredAtMs: "1700000000000",
          state: "registered" as const,
        })),
        openSourceSessionResult: vi.fn(async () => ({
          streamKind: "native_session_result" as const,
          streamId: id16(50),
          documentId: id16(51),
          targetId: null,
          revision: null,
          totalBytes: "2",
          encoding: "utf8_json" as const,
        })),
        sendSessionViewCredit: vi.fn(),
        detachSessionStream: vi.fn(),
        closeSessionView: vi.fn(async () => ({
          state: "closed" as const,
          lastCommittedPacket: "0",
        })),
      };

      let capturedOnViewData: ((frame: ViewDataFrame) => void) | undefined;
      const clearPersistedSourceView = vi.fn();

      const workbench = new ManagementWorkbench({
        panel,
        sharedViewHost,
        doc: installed.doc,
        location: {
          protocol: "https:",
          origin: "https://node.example.test",
          host: "node.example.test",
        },
        createVault: () => fakeVault,
        connectionFactory: (_identity, _key, options) => {
          capturedOnViewData = options.callbacks?.onViewData;
          return fakeConnection as unknown as ManagementConnection;
        },
        displayFactory: (_doc, opened) => ({
          streamId: opened.streamId,
          element: { dataset: {}, removeAttribute: vi.fn() },
          start: vi.fn(),
          suspend: sharedSuspend,
          dispose: sharedDispose,
          accept: sharedAccept,
        }),
        bodyReaderFactory: () => ({
          accept: bodyAccept,
          seal: bodySeal,
        }),
        loadPersistedSourceView: () => null,
        persistSourceView: vi.fn(),
        clearPersistedSourceView,
      });

      await workbench.boot();
      await vi.waitFor(() => {
        expect(fakeConnection.inspectCatalogInventory).toHaveBeenCalledOnce();
      });

      // Verify exact RWW-008 dashboard section order:
      // 身份详情 -> 节点信息 -> 会话输入队列 -> 严格权限审批 -> 会话容量 -> 原生会话释放 -> 工作区与目录 -> 来源会话发现 -> 已登记客户端 -> 底部操作栏
      const panelText = panel.textContent;
      const orderedSnippets = [
        "节点管理",
        "客户端名称",
        "节点来源",
        "节点状态",
        "会话输入队列",
        "原生审批",
        "原生会话名额",
        "原生会话释放",
        "工作目录与 Harness 配置",
        "原生来源发现与新建",
        "已登记客户端",
        "刷新列表",
        "断开管理连接",
      ];
      let lastIndex = -1;
      for (const snippet of orderedSnippets) {
        const idx = panelText.indexOf(snippet);
        expect(idx, `missing snippet: ${snippet}`).toBeGreaterThan(lastIndex);
        lastIndex = idx;
      }
      expect(
        findDataPanel(panel as unknown as FakeElement, "source-session"),
      ).not.toBeNull();

      // Set target and open shared view + body stream + source result stream
      const targetId = id16(77);
      workbench.sessionQueue.inspectedQueueTargetId = targetId;
      await workbench.sharedView.openSharedSessionView(record, targetId, null);
      await workbench.queuedBody.openQueueBody(
        record,
        targetId,
        { itemId: id16(78), revision: "1" } as never,
        { disabled: true } as HTMLButtonElement,
      );

      workbench.sourceSession.fillSelectionAndNativeId(
        {
          sourceId: id16(1),
          harnessId: id16(2),
          profileId: id16(3),
          profileRevision: "1",
          workspaceId: id16(4),
          launchPath: "/workspace/demo",
        },
        "native-1",
      );
      await workbench.sourceSession.registerSourceSessionOp(
        record,
        "create",
        { disabled: true } as HTMLButtonElement,
      );
      await workbench.sourceSession.openSourceSessionResultStream(record, {
        disabled: true,
      } as HTMLButtonElement);

      expect(capturedOnViewData).toBeDefined();

      // 1. Frame for shared view -> routed to SharedViewController only
      const sharedFrame = makeFrame(id16(20), new Uint8Array([1]));
      capturedOnViewData!(sharedFrame);
      expect(sharedAccept).toHaveBeenCalledWith(sharedFrame);
      expect(bodyAccept).not.toHaveBeenCalled();

      // 2. Frame for source result stream -> routed to SourceSessionController
      const sourceFrame = makeFrame(
        id16(50),
        new TextEncoder().encode("{}"),
      );
      capturedOnViewData!(sourceFrame);
      expect(fakeConnection.sendSessionViewCredit).toHaveBeenCalledWith(
        id16(50),
        1n,
      );
      expect(bodyAccept).not.toHaveBeenCalled();

      // 3. Frame for queued body stream -> routed to QueuedBodyController
      const bodyFrame = makeFrame(id16(30), new Uint8Array([65]));
      capturedOnViewData!(bodyFrame);
      expect(bodyAccept).toHaveBeenCalledWith(bodyFrame);

      // 4. Stale generation frame is ignored
      const staleGen = workbench.managementGeneration - 1;
      sharedAccept.mockClear();
      workbench.handleSessionViewData(record, staleGen, sharedFrame);
      expect(sharedAccept).not.toHaveBeenCalled();

      // 5. suspendForDisconnect suspends shared view without disposing
      workbench.suspendForDisconnect();
      expect(sharedSuspend).toHaveBeenCalledOnce();
      expect(sharedDispose).not.toHaveBeenCalled();

      // 6. resetAllSessionState clears queues, views, source operations, inventory, capacity, devices
      workbench.resetAllSessionState();
      expect(sharedDispose).toHaveBeenCalledOnce();
      expect(bodySeal).toHaveBeenCalledOnce();
      expect(clearPersistedSourceView).toHaveBeenCalled();
      expect(workbench.sessionQueue.inspectedQueueTargetId).toBeNull();
      expect(workbench.sourceSession.activeSourceResultStreamId).toBeNull();
      expect(workbench.devices.currentDevices).toHaveLength(0);

      // 7. handlePageHide bumps generation and disconnects active management transport
      const genBeforeHide = workbench.managementGeneration;
      workbench.handlePageHide();
      expect(fakeConnection.disconnect).toHaveBeenCalledOnce();
      expect(workbench.management).toBeNull();
      expect(workbench.managementGeneration).toBe(genBeforeHide + 1);
    } finally {
      installed.restore();
    }
  });
});
