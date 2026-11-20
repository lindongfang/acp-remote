import {
  buildBindTargetInitialDocument as directBuildBind,
  buildInstallHarnessDocument as directBuildInstall,
  CatalogActionError as DirectCatalogActionError,
  utf16leBase64 as directUtf16leBase64,
} from "../src/management/catalog-documents";
import { describe, expect, it, vi } from "vitest";
import {
  buildBindTargetInitialDocument,
  buildInstallHarnessDocument,
  CatalogActionError,
  ManagementConnection,
  utf16leBase64,
} from "../src/management/client";
import { encodeInputData } from "../src/management/input-transfer";
import type { ExpectedIdentity } from "../src/management/protocol";
import type { CatalogInventory } from "../src/management/session-protocol";
import {
  CatalogDraftValidationError,
  createInitialInstallDraft,
  createInitialTargetDraft,
  parseAdditionalDirectoriesLines,
  parseLaunchArgsLines,
  parseLaunchEnvLines,
  validateBindTargetInput,
  validateInstallHarnessInput,
  workspaceCatalogPanelView,
} from "../src/management/workspace-catalog-view";
import {
  decodeBase64urlStrict,
  encodeBase64url,
  signCompactJws,
  verifyCompactJws,
} from "../src/protocol/jws";
import { publicKeyFingerprint } from "../src/protocol/pairing";

const ORIGIN = "https://node.example.test";
const TEXT_ENCODER = new TextEncoder();

function id(byte: number): string {
  return encodeBase64url(new Uint8Array(16).fill(byte));
}

function encodeAck(uploadId: string, nextOffset: bigint): Uint8Array {
  const wire = encodeInputData(uploadId, nextOffset, Uint8Array.of(1)).slice(0, 32);
  wire[5] = 2;
  return wire;
}

class FakeSocket extends EventTarget {
  readyState = 0;
  readonly sent: Array<string | Uint8Array> = [];
  onSend: ((value: Record<string, unknown>) => void | Promise<void>) | null = null;
  onBinary: ((value: Uint8Array) => void | Promise<void>) | null = null;

  constructor(
    readonly url: string,
    readonly protocols: readonly string[],
  ) {
    super();
    queueMicrotask(() => {
      if (this.readyState === 0) {
        this.readyState = 1;
        this.dispatchEvent(new Event("open"));
      }
    });
  }

  send(value: string | Uint8Array): void {
    this.sent.push(value);
    if (value instanceof Uint8Array) {
      void Promise.resolve(this.onBinary?.(value));
      return;
    }
    const parsed = JSON.parse(value) as Record<string, unknown>;
    void Promise.resolve(this.onSend?.(parsed));
  }

  receive(value: unknown): void {
    const event = new Event("message") as MessageEvent<unknown>;
    Object.defineProperty(event, "data", { value: JSON.stringify(value) });
    this.dispatchEvent(event);
  }

  receiveBinary(value: Uint8Array): void {
    const event = new Event("message") as MessageEvent<unknown>;
    Object.defineProperty(event, "data", { value });
    this.dispatchEvent(event);
  }

  close(code?: number, reason?: string): void {
    if (this.readyState >= 2) return;
    this.readyState = 2;
    queueMicrotask(() => {
      if (this.readyState === 3) return;
      this.readyState = 3;
      const event = new Event("close") as CloseEvent;
      Object.defineProperties(event, {
        code: { value: code ?? 1000 },
        reason: { value: reason ?? "" },
        wasClean: { value: true },
      });
      this.dispatchEvent(event);
    });
  }
}

async function connectedCatalogClient() {
  const nodePair = await globalThis.crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  );
  const clientPair = await globalThis.crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  );
  const nodePub = encodeBase64url(
    new Uint8Array(await globalThis.crypto.subtle.exportKey("raw", nodePair.publicKey)),
  );
  const clientPub = encodeBase64url(
    new Uint8Array(await globalThis.crypto.subtle.exportKey("raw", clientPair.publicKey)),
  );
  const clientId = id(10);
  const expected: ExpectedIdentity = {
    origin: ORIGIN,
    nodeId: await publicKeyFingerprint(nodePub),
    nodePublicKey: nodePub,
    clientId,
    clientPublicKey: clientPub,
  };
  const fake = new FakeSocket(`${ORIGIN}/unused`, []);
  const connection = new ManagementConnection(expected, clientPair.privateKey, {
    origin: ORIGIN,
    socketFactory: () => fake as unknown as WebSocket,
  });
  fake.onSend = async (frame) => {
    if (frame.type === "hello") {
      const now = Math.floor(Date.now() / 1_000);
      const binding = {
        v: 1,
        nodeId: expected.nodeId,
        nodePublicKey: expected.nodePublicKey,
        origin: expected.origin,
        connectionId: id(6),
        challengeId: id(4),
        nonce: encodeBase64url(new Uint8Array(32).fill(5)),
        iat: now,
        exp: now + 20,
        clientId: frame.clientId,
        clientPublicKey: frame.clientPublicKey,
        clientNonce: frame.clientNonce,
      };
      const hostProof = await signCompactJws(
        "acpr-auth-host+jwt",
        TEXT_ENCODER.encode(JSON.stringify(binding)),
        nodePair.privateKey,
      );
      fake.receive({
        v: 1,
        type: "challenge",
        nodeId: binding.nodeId,
        nodePublicKey: binding.nodePublicKey,
        origin: binding.origin,
        connectionId: binding.connectionId,
        challengeId: binding.challengeId,
        nonce: binding.nonce,
        iat: binding.iat,
        exp: binding.exp,
        hostProof,
      });
    }
    if (frame.type === "proof") {
      await verifyCompactJws(
        String(frame.proof),
        decodeBase64urlStrict(clientPub),
        ["acpr-auth-client+jwt"],
      );
      fake.receive({ v: 1, type: "authenticated", clientId, connectionId: id(6) });
    }
  };
  await connection.connect();
  fake.onSend = null;
  return { connection, fake };
}

class StrictDomElement {
  textContent = "";
  className = "";
  type = "";
  id = "";
  name = "";
  htmlFor = "";
  value = "";
  placeholder = "";
  autocomplete = "";
  spellcheck = false;
  required = false;
  selected = false;
  rows = 0;
  disabled = false;
  dataset: Record<string, string> = {};
  children: StrictDomElement[] = [];
  listeners = new Map<string, Array<(event: unknown) => void>>();

  constructor(readonly tagName: string) {}

  set innerHTML(_value: string) {
    throw new Error("innerHTML assignment is forbidden");
  }

  append(...elements: Array<StrictDomElement | string>): void {
    for (const item of elements) {
      if (typeof item === "string") {
        const span = new StrictDomElement("#text");
        span.textContent = item;
        this.children.push(span);
      } else {
        this.children.push(item);
      }
    }
  }

  addEventListener(type: string, listener: (event: unknown) => void): void {
    const existing = this.listeners.get(type) ?? [];
    existing.push(listener);
    this.listeners.set(type, existing);
  }

  dispatch(type: string, event: Record<string, unknown> = {}): void {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }
}

function findBy(
  root: StrictDomElement,
  predicate: (el: StrictDomElement) => boolean,
): StrictDomElement[] {
  const out: StrictDomElement[] = [];
  if (predicate(root)) out.push(root);
  for (const child of root.children) {
    out.push(...findBy(child, predicate));
  }
  return out;
}

describe("catalog document helpers and UTF-16LE base64 encoding", () => {
  it("encodes empty, ASCII, CJK, and surrogate pairs in canonical UTF-16LE padded Base64", () => {
    expect(utf16leBase64("")).toBe("");
    // 'A' = 0x0041 -> [0x41, 0x00] -> QQ==
    expect(utf16leBase64("A")).toBe("QQA=");
    // 'node' -> [0x6e,0x00, 0x6f,0x00, 0x64,0x00, 0x65,0x00]
    expect(utf16leBase64("node")).toBe("bgBvAGQAZQA=");
    // '中' (U+4E2D) -> LE [0x2D, 0x4E] -> LU4=
    expect(utf16leBase64("中")).toBe("LU4=");
    // '🦀' (U+1F980) -> UTF-16 surrogates [0xD83E, 0xDD80] -> LE bytes [0x3E, 0xD8, 0x80, 0xDD]
    expect(utf16leBase64("🦀")).toBe("PtiA3Q==");
  });

  it("builds install_harness and bind_target_initial catalog documents matching node_configuration_profile", () => {
    const doc = buildInstallHarnessDocument({
      name: "Codex ACP",
      source: { kind: "new", name: "Codex Source" },
      executable: "D:\\Application\\Node\\nodejs\\node.exe",
      args: ["C:\\acp\\index.js"],
      environment: [{ name: "NO_BROWSER", value: "1" }],
      recipes: [
        {
          wire: 2,
          mcpServers: [],
          additionalDirectories: [{ workspaceId: id(1), path: "D:\\ws" }],
          extra: {},
        },
      ],
    });
    expect(doc).toEqual({
      kind: "install_harness",
      name: "Codex ACP",
      source: { kind: "new", name: "Codex Source" },
      profile: {
        format: 1,
        launch: {
          executable_utf16le_base64: utf16leBase64("D:\\Application\\Node\\nodejs\\node.exe"),
          arguments_utf16le_base64: [utf16leBase64("C:\\acp\\index.js")],
          environment: [
            {
              name_utf16le_base64: utf16leBase64("NO_BROWSER"),
              value_utf16le_base64: utf16leBase64("1"),
            },
          ],
        },
        recipes: [
          {
            wire: 2,
            mcp_servers: [],
            additional_directories: [{ workspace_id: id(1), path: "D:\\ws" }],
            extra: {},
          },
        ],
      },
    });

    const bindDoc = buildBindTargetInitialDocument({
      targetId: id(2),
      sourceId: id(3),
      harnessId: id(4),
      profileId: id(5),
      profileRevision: "1",
      workspaceId: id(6),
      launchPath: "D:\\ws",
    });
    expect(bindDoc).toEqual({
      kind: "bind_target_initial",
      targetId: id(2),
      sourceId: id(3),
      harnessId: id(4),
      profileId: id(5),
      profileRevision: "1",
      workspaceId: id(6),
      launchPath: "D:\\ws",
    });
  });
});

describe("ManagementConnection catalog client methods", () => {
  it("handles workspace.register, target.register, and catalog.inventory", async () => {
    const { connection, fake } = await connectedCatalogClient();
    const wsId = id(21);
    const srcId = id(22);
    const tgtId = id(23);

    fake.onSend = (frame) => {
      if (frame.action === "workspace.register") {
        expect(frame.payload).toEqual({ path: "D:\\Project\\demo" });
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: { workspaceId: wsId, canonicalPath: "D:\\Project\\demo" },
          error: null,
        });
      } else if (frame.action === "target.register") {
        expect(frame.payload).toEqual({
          sourceId: srcId,
          nativeSessionId: "codex-1",
          workspaceId: wsId,
        });
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: {
            targetId: tgtId,
            sourceId: srcId,
            nativeSessionId: "codex-1",
            workspaceId: wsId,
            binding: null,
          },
          error: null,
        });
      } else if (frame.action === "catalog.inventory") {
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: {
            workspaces: [{ workspaceId: wsId, canonicalPath: "D:\\Project\\demo" }],
            sources: [{ sourceId: srcId, name: "Codex Source" }],
            harnesses: [],
            profiles: [],
            targets: [
              {
                targetId: tgtId,
                sourceId: srcId,
                nativeSessionId: "codex-1",
                workspaceId: wsId,
                binding: null,
              },
            ],
          },
          error: null,
        });
      }
    };

    const ws = await connection.registerWorkspace("D:\\Project\\demo");
    expect(ws).toEqual({ workspaceId: wsId, canonicalPath: "D:\\Project\\demo" });
    const tgt = await connection.registerTarget(srcId, "codex-1", wsId);
    expect(tgt.targetId).toBe(tgtId);
    const inv = await connection.inspectCatalogInventory();
    expect(inv.workspaces).toHaveLength(1);
    expect(inv.targets[0]?.targetId).toBe(tgtId);
    connection.disconnect();
  });

  it("uploads multi-chunk catalog document, polls preparation, registers, and commits apply", async () => {
    const { connection, fake } = await connectedCatalogClient();
    const uploadId = id(31);
    const requestId = id(32);
    const seenActions: string[] = [];
    const seenOffsets: bigint[] = [];
    let statusPolls = 0;

    fake.onBinary = (frame) => {
      const view = new DataView(frame.buffer, frame.byteOffset, frame.byteLength);
      const offset = view.getBigUint64(24, false);
      const payloadLen = frame.byteLength - 32;
      seenOffsets.push(offset);
      fake.receiveBinary(encodeAck(uploadId, offset + BigInt(payloadLen)));
    };

    fake.onSend = (frame) => {
      const action = String(frame.action);
      seenActions.push(action);
      if (action === "catalog.upload.begin") {
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: { uploadId },
          error: null,
        });
      } else if (action === "catalog.upload.finish") {
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: { uploadId },
          error: null,
        });
      } else if (action === "catalog.upload.status") {
        statusPolls += 1;
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: {
            uploadId,
            phase: statusPolls === 1 ? "preparing" : "ready",
            nextOffset: "25000",
          },
          error: null,
        });
      } else if (action === "catalog.register") {
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: { catalogRequestId: requestId, state: "registered" },
          error: null,
        });
      } else if (action === "catalog.apply") {
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: {
            fact: {
              catalogRequestId: requestId,
              actorId: id(10),
              action: "install_harness",
              state: "committed",
              registeredAtMs: "1",
              registerAuthGeneration: "1",
              admissionAuthGeneration: "1",
              resultAtMs: "2",
              result: {
                kind: "harness",
                harnessId: id(41),
                sourceId: id(42),
                profileId: id(43),
                profileRevision: "1",
              },
            },
            auditWarning: "none",
            sqlObservation: { operation: "apply", slots: [null, null] },
          },
          error: null,
        });
      }
    };

    const bigPadding = "x".repeat(20_000);
    const receipt = await connection.executeCatalogDocument({
      kind: "install_harness",
      padding: bigPadding,
    });
    expect(receipt.fact.state).toBe("committed");
    expect(seenOffsets.length).toBeGreaterThan(1);
    expect(seenActions).toEqual([
      "catalog.upload.begin",
      "catalog.upload.finish",
      "catalog.upload.status",
      "catalog.upload.status",
      "catalog.register",
      "catalog.apply",
    ]);
    connection.disconnect();
  });

  it("releases upload buffer when catalog upload preparation fails", async () => {
    const { connection, fake } = await connectedCatalogClient();
    const uploadId = id(51);
    let released = false;

    fake.onBinary = (frame) => {
      const view = new DataView(frame.buffer, frame.byteOffset, frame.byteLength);
      const offset = view.getBigUint64(24, false);
      const payloadLen = frame.byteLength - 32;
      fake.receiveBinary(encodeAck(uploadId, offset + BigInt(payloadLen)));
    };

    fake.onSend = (frame) => {
      if (frame.action === "catalog.upload.begin") {
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: { uploadId },
          error: null,
        });
      } else if (frame.action === "catalog.upload.finish") {
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: { uploadId },
          error: null,
        });
      } else if (frame.action === "catalog.upload.status") {
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: {
            uploadId,
            phase: "failed",
            nextOffset: "10",
            error: "invalid_document",
          },
          error: null,
        });
      } else if (frame.action === "catalog.upload.release") {
        released = true;
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: true,
          result: { released: true },
          error: null,
        });
      }
    };

    const rejection = connection.executeCatalogDocument({ kind: "install_harness" });
    await expect(rejection).rejects.toBeInstanceOf(CatalogActionError);
    await expect(rejection).rejects.toMatchObject({
      name: "CatalogActionError",
      code: "upload_failed",
      detail: "invalid_document",
    });
    expect(released).toBe(true);
    connection.disconnect();
  });
});

describe("workspace-catalog-view validation and DOM rendering", () => {
  it("parses launch args, env lines, and additional directories with inventory lookup", () => {
    expect(parseLaunchArgsLines("  --stdio \r\n\n --flag=1 ")).toEqual(["--stdio", "--flag=1"]);
    expect(parseLaunchEnvLines("CODEX_HOME=D:\\codex\nNO_BROWSER=1\n")).toEqual([
      { name: "CODEX_HOME", value: "D:\\codex" },
      { name: "NO_BROWSER", value: "1" },
    ]);
    expect(() => parseLaunchEnvLines("BROKEN_LINE")).toThrow(CatalogDraftValidationError);

    const wsId = id(1);
    const inv: CatalogInventory = {
      workspaces: [{ workspaceId: wsId, canonicalPath: "D:\\Project\\repo" }],
      sources: [],
      harnesses: [],
      profiles: [],
      targets: [],
    };
    expect(parseAdditionalDirectoriesLines(`${wsId}\n${id(2)}=D:\\extra`, inv)).toEqual([
      { workspaceId: wsId, path: "D:\\Project\\repo" },
      { workspaceId: id(2), path: "D:\\extra" },
    ]);
    expect(() => parseAdditionalDirectoriesLines(id(9), inv)).toThrow(CatalogDraftValidationError);
  });

  it("validates install and bind drafts and wires DOM callbacks without innerHTML", () => {
    const draft = {
      ...createInitialInstallDraft(),
      executable: "node.exe",
      argsText: "index.js",
      envText: "NO_BROWSER=1",
    };
    const installDoc = validateInstallHarnessInput(draft, null);
    expect(installDoc.name).toBe("Codex ACP");
    expect(installDoc.source).toEqual({ kind: "new", name: "Codex Source" });

    const targetDraft = {
      ...createInitialTargetDraft(),
      sourceId: id(1),
      harnessId: id(2),
      profileId: id(3),
      profileRevision: "1",
      workspaceId: id(4),
      targetId: id(5),
    };
    const bindInput = validateBindTargetInput(targetDraft, undefined, "D:\\fallback");
    expect(bindInput.launchPath).toBe("D:\\fallback");

    const owner = {
      createElement: (tag: string) => new StrictDomElement(tag),
    } as unknown as Document;

    const inv: CatalogInventory = {
      workspaces: [{ workspaceId: id(4), canonicalPath: "D:\\ws" }],
      sources: [{ sourceId: id(1), name: "Codex Source" }],
      harnesses: [{ harnessId: id(2), name: "Codex ACP", sourceId: id(1) }],
      profiles: [{ profileId: id(3), revision: "1", harnessId: id(2), sourceId: id(1) }],
      targets: [
        {
          targetId: id(5),
          sourceId: id(1),
          nativeSessionId: "native-1",
          workspaceId: id(4),
          binding: {
            profileId: id(3),
            profileRevision: "1",
            workspaceId: id(4),
            launchPath: "D:\\ws",
            observedCanonicalPath: "D:\\ws",
          },
        },
      ],
    };

    const onFillSourceSelection = vi.fn();
    const onSelectTargetQueue = vi.fn();
    const onRefreshInventory = vi.fn();
    const onUseWorkspace = vi.fn();

    const panel = workspaceCatalogPanelView(
      owner,
      {
        connected: true,
        inventory: inv,
        workspacePathDraft: "",
        installDraft: draft,
        targetDraft,
      },
      {
        onRefreshInventory,
        onWorkspacePathChange: vi.fn(),
        onRegisterWorkspace: vi.fn(),
        onInstallFieldChange: vi.fn(),
        onInstallHarness: vi.fn(),
        onTargetFieldChange: vi.fn(),
        onRegisterTarget: vi.fn(),
        onRegisterAndBindTarget: vi.fn(),
        onBindTarget: vi.fn(),
        onUseWorkspace,
        onUseHarnessProfile: vi.fn(),
        onSelectTargetQueue,
        onSelectTargetRelease: vi.fn(),
        onSelectTargetForBind: vi.fn(),
        onFillSourceSelection,
      },
    ) as unknown as StrictDomElement;

    const buttons = findBy(panel, (el) => el.tagName === "button");
    buttons.find((b) => b.textContent === "刷新目录清单")?.dispatch("click");
    expect(onRefreshInventory).toHaveBeenCalledOnce();

    buttons.find((b) => b.textContent === "填入 Target / Source 表单")?.dispatch("click");
    expect(onUseWorkspace).toHaveBeenCalledWith(inv.workspaces[0]);

    buttons.find((b) => b.textContent === "载入队列")?.dispatch("click");
    expect(onSelectTargetQueue).toHaveBeenCalledWith(id(5));

    buttons.find((b) => b.textContent === "填入来源面板")?.dispatch("click");
    expect(onFillSourceSelection).toHaveBeenCalledWith(
      {
        sourceId: id(1),
        harnessId: id(2),
        profileId: id(3),
        profileRevision: "1",
        workspaceId: id(4),
        launchPath: "D:\\ws",
      },
      "native-1",
    );
  });
});

describe("catalog-documents extraction and re-export compatibility", () => {
  it("maintains 100% referential identity between direct and client.ts exports", () => {
    expect(directBuildBind).toBe(buildBindTargetInitialDocument);
    expect(directBuildInstall).toBe(buildInstallHarnessDocument);
    expect(DirectCatalogActionError).toBe(CatalogActionError);
    expect(directUtf16leBase64).toBe(utf16leBase64);
  });

  it("constructs CatalogActionError with and without detail", () => {
    const errorWithDetail = new DirectCatalogActionError(
      "upload_failed",
      "chunk error",
    );
    expect(errorWithDetail.name).toBe("CatalogActionError");
    expect(errorWithDetail.code).toBe("upload_failed");
    expect(errorWithDetail.detail).toBe("chunk error");
    expect(errorWithDetail.message).toBe("upload_failed: chunk error");

    const errorWithoutDetail = new DirectCatalogActionError(
      "preparation_timeout",
    );
    expect(errorWithoutDetail.name).toBe("CatalogActionError");
    expect(errorWithoutDetail.code).toBe("preparation_timeout");
    expect(errorWithoutDetail.detail).toBeNull();
    expect(errorWithoutDetail.message).toBe("preparation_timeout");
  });

  it("encodes string to UTF-16LE Base64 properly", () => {
    const encoded = directUtf16leBase64("node");
    const binary = globalThis.atob(encoded);
    expect(binary.length).toBe(8);
    expect(binary.charCodeAt(0)).toBe(110);
    expect(binary.charCodeAt(1)).toBe(0);
    expect(binary.charCodeAt(2)).toBe(111);
    expect(binary.charCodeAt(3)).toBe(0);
  });
});
