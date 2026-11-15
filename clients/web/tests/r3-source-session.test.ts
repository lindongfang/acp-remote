import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { describe, expect, it, vi } from "vitest";
import {
  ManagementConnection,
  ManagementConnectionError,
  ManagementRequestError,
} from "../src/management/client";
import { managementRequestErrorMessage } from "../src/management/error-messages";
import {
  parseManagementFrame,
  type ExpectedIdentity,
} from "../src/management/protocol";
import {
  encodeSessionCommand,
  parseSessionCommand,
  parseSessionViewOpened,
  parseSourceSessionFact,
  parseSourceSessionRegistration,
  parseSourceSessionResultDescriptor,
  parseSourceSessionResultReleaseResult,
  SessionProtocolError,
  type SessionCommand,
  type SourceSessionFact,
} from "../src/management/session-protocol";
import { SessionViewDisplay } from "../src/management/session-view-display";
import {
  inspectSourceSessionResultPayload,
  MAX_SOURCE_RESULT_DISPLAY_BYTES,
  SourceSessionResultReader,
  SourceSessionResultReadError,
} from "../src/management/source-result-read";
import {
  loadPersistedSourceViewSpec,
  persistSourceViewSpec,
  SOURCE_SHARED_VIEW_STORAGE_KEY,
  sourceSessionPanelView,
  sourceSessionResultView,
} from "../src/management/source-session-view";
import {
  decodeBase64urlStrict,
  encodeBase64url,
  signCompactJws,
  verifyCompactJws,
} from "../src/protocol/jws";
import { publicKeyFingerprint } from "../src/protocol/pairing";
import type { ViewDataFrame } from "../src/views/binary";
import { MANAGEMENT_ERROR_CASES } from "./error-contract-vectors";

const ORIGIN = "https://node.example.test";
const ENCODER = new TextEncoder();

function id(byte: number): string {
  return encodeBase64url(new Uint8Array(16).fill(byte));
}

function validSelection() {
  return {
    sourceId: id(1),
    harnessId: id(2),
    profileId: id(3),
    profileRevision: "7",
    workspaceId: id(4),
    launchPath: "/workspace/demo",
  } as const;
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
  disabled = false;
  tabIndex = 0;
  hidden = false;
  dataset: Record<string, string> = {};
  children: StrictDomElement[] = [];
  attributes = new Map<string, string>();
  listeners = new Map<string, Array<(event: unknown) => void>>();

  set innerHTML(_value: string) {
    throw new Error("innerHTML assignment is forbidden in R3 source session DOM");
  }

  setAttribute(key: string, value: string): void {
    this.attributes.set(key, value);
  }

  removeAttribute(key: string): void {
    this.attributes.delete(key);
  }

  append(...elements: Array<StrictDomElement | string>): void {
    for (const item of elements) {
      if (typeof item === "string") {
        const textNode = new StrictDomElement();
        textNode.textContent = item;
        this.children.push(textNode);
      } else {
        this.children.push(item);
      }
    }
  }

  replaceChildren(...elements: StrictDomElement[]): void {
    this.children = [...elements];
  }

  remove(): void {}

  addEventListener(type: string, listener: (event: unknown) => void): void {
    const existing = this.listeners.get(type) ?? [];
    existing.push(listener);
    this.listeners.set(type, existing);
  }

  click(): void {
    for (const listener of this.listeners.get("click") ?? []) {
      listener({ preventDefault: () => {} });
    }
  }
}

function collectText(node: StrictDomElement): string {
  return [node.textContent, ...node.children.map(collectText)].join("\n");
}

function findButtons(
  node: StrictDomElement,
  label: string,
): StrictDomElement[] {
  const hits: StrictDomElement[] = [];
  if (node.textContent === label) hits.push(node);
  for (const child of node.children) {
    hits.push(...findButtons(child, label));
  }
  return hits;
}

function createFakeDocument(): Document {
  return {
    createElement: () => new StrictDomElement(),
    createDocumentFragment: () => new StrictDomElement(),
  } as unknown as Document;
}

function resultFrame(
  streamId: string,
  packet: bigint,
  offset: bigint,
  total: bigint,
  bytes: Uint8Array,
  overrides: Partial<ViewDataFrame> = {},
): ViewDataFrame {
  return {
    type: "data",
    streamId,
    packetSequence: packet,
    recordId: 1n,
    byteOffset: offset,
    totalLength: total,
    kind: "original_raw",
    last: offset + BigInt(bytes.length) === total,
    payload: bytes,
    ...overrides,
  };
}

function snapshotEndFrame(streamId: string, packet: bigint): ViewDataFrame {
  return resultFrame(streamId, packet, 0n, 0n, new Uint8Array(), {
    recordId: 2n,
    kind: "snapshot_end",
  });
}

class FakeSocket extends EventTarget {
  readyState = 0;
  readonly sent: Array<string | Uint8Array> = [];
  onSend: ((value: Record<string, unknown>) => void | Promise<void>) | null =
    null;

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
    if (typeof value !== "string") return;
    const parsed = JSON.parse(value) as Record<string, unknown>;
    void Promise.resolve(this.onSend?.(parsed));
  }

  receive(value: unknown): void {
    const event = new Event("message") as MessageEvent<unknown>;
    Object.defineProperty(event, "data", { value: JSON.stringify(value) });
    this.dispatchEvent(event);
  }

  close(code = 1000, reason = ""): void {
    this.serverClose(code, reason, true);
  }

  serverClose(code: number, reason: string, clean = false): void {
    if (this.readyState === 3) return;
    this.readyState = 3;
    const event = new Event("close") as CloseEvent;
    Object.defineProperties(event, {
      code: { value: code },
      reason: { value: reason },
      wasClean: { value: clean },
    });
    this.dispatchEvent(event);
  }
}

async function generateKeyPair() {
  const pair = await globalThis.crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  );
  const raw = await globalThis.crypto.subtle.exportKey("raw", pair.publicKey);
  return {
    privateKey: pair.privateKey,
    publicKey: encodeBase64url(new Uint8Array(raw)),
  };
}

async function connectFakeManagement(
  handler: (
    frame: Record<string, unknown>,
    socket: FakeSocket,
    expected: ExpectedIdentity,
    clientKey: string,
  ) => void | Promise<void>,
) {
  const node = await generateKeyPair();
  const client = await generateKeyPair();
  const clientId = id(9);
  const expected: ExpectedIdentity = {
    origin: ORIGIN,
    nodeId: await publicKeyFingerprint(node.publicKey),
    nodePublicKey: node.publicKey,
    clientId,
    clientPublicKey: client.publicKey,
  };
  const fake = new FakeSocket(
    "wss://node.example.test/api/access/v1/control",
    ["acpr.access.v1"],
  );
  fake.onSend = async (frame) => {
    if (frame.type === "hello") {
      const now = Math.floor(Date.now() / 1_000);
      const binding = {
        v: 1,
        nodeId: expected.nodeId,
        nodePublicKey: expected.nodePublicKey,
        origin: expected.origin,
        connectionId: id(10),
        challengeId: id(11),
        nonce: encodeBase64url(new Uint8Array(32).fill(5)),
        iat: now,
        exp: now + 20,
        clientId: frame.clientId,
        clientPublicKey: frame.clientPublicKey,
        clientNonce: frame.clientNonce,
      };
      const hostProof = await signCompactJws(
        "acpr-auth-host+jwt",
        ENCODER.encode(JSON.stringify(binding)),
        node.privateKey,
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
      return;
    }
    if (frame.type === "proof") {
      await verifyCompactJws(
        String(frame.proof),
        decodeBase64urlStrict(client.publicKey),
        ["acpr-auth-client+jwt"],
      );
      fake.receive({
        v: 1,
        type: "authenticated",
        clientId,
        connectionId: id(10),
      });
      return;
    }
    await handler(frame, fake, expected, client.publicKey);
  };
  const connection = new ManagementConnection(expected, client.privateKey, {
    origin: ORIGIN,
    socketFactory: () => fake as unknown as WebSocket,
  });
  await connection.connect();
  return { connection, fake };
}

describe("R3-01 Web session-protocol closed commands and parsers", () => {
  it("round-trips all 7 R3 commands plus view.rebind including UTF-8 4096-byte bounds", () => {
    const boundaryText = "x".repeat(4096);
    const selection = validSelection();
    const validCommands: SessionCommand[] = [
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "source.session.register",
        payload: {
          selection: { ...selection, launchPath: boundaryText },
          operation: { kind: "discover", cursor: null },
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(11),
        action: "source.session.register",
        payload: {
          selection,
          operation: { kind: "discover", cursor: boundaryText },
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(12),
        action: "source.session.register",
        payload: {
          selection,
          operation: { kind: "create" },
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(13),
        action: "source.session.execute",
        payload: { sourceOperationId: id(20) },
      },
      {
        v: 1,
        type: "command",
        requestId: id(14),
        action: "source.session.status",
        payload: { sourceOperationId: id(20) },
      },
      {
        v: 1,
        type: "command",
        requestId: id(15),
        action: "source.session.reconcile",
        payload: { sourceOperationId: id(20) },
      },
      {
        v: 1,
        type: "command",
        requestId: id(16),
        action: "source.session.result.open",
        payload: { sourceOperationId: id(20) },
      },
      {
        v: 1,
        type: "command",
        requestId: id(17),
        action: "source.session.result.release",
        payload: { sourceOperationId: id(20) },
      },
      {
        v: 1,
        type: "command",
        requestId: id(18),
        action: "view.open_source",
        payload: {
          selection,
          nativeSessionId: boundaryText,
          reason: "reconnect",
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(19),
        action: "view.rebind",
        payload: { streamId: id(21) },
      },
    ];
    for (const command of validCommands) {
      const encoded = encodeSessionCommand(command);
      expect(parseSessionCommand(encoded)).toEqual(command);
    }
  });

  it("rejects missing fields, extra fields, bad Id128, bad reason, NUL, empty and >4096B strings", () => {
    const baseSelection = validSelection();
    const badInputs: unknown[] = [
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "source.session.register",
        payload: {
          selection: { ...baseSelection, launchPath: "" },
          operation: { kind: "create" },
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "source.session.register",
        payload: {
          selection: { ...baseSelection, launchPath: "a\0b" },
          operation: { kind: "create" },
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "source.session.register",
        payload: {
          selection: { ...baseSelection, launchPath: "é".repeat(2049) },
          operation: { kind: "create" },
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "source.session.register",
        payload: {
          selection: baseSelection,
          operation: { kind: "discover", cursor: "" },
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "source.session.register",
        payload: {
          selection: baseSelection,
          operation: { kind: "discover", cursor: "a".repeat(4097) },
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "source.session.register",
        payload: {
          selection: { ...baseSelection, extra: true },
          operation: { kind: "create" },
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "source.session.execute",
        payload: { sourceOperationId: "not-base64url-id" },
      },
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "view.open_source",
        payload: {
          selection: baseSelection,
          nativeSessionId: "",
          reason: "first_open",
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "view.open_source",
        payload: {
          selection: baseSelection,
          nativeSessionId: "x\0y",
          reason: "first_open",
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "view.open_source",
        payload: {
          selection: baseSelection,
          nativeSessionId: "z".repeat(4097),
          reason: "first_open",
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "view.open_source",
        payload: {
          selection: baseSelection,
          nativeSessionId: "native-1",
          reason: "invalid_reason",
        },
      },
      {
        v: 1,
        type: "command",
        requestId: id(10),
        action: "view.rebind",
        payload: { streamId: id(21), extra: 1 },
      },
    ];
    for (const input of badInputs) {
      expect(() => parseSessionCommand(JSON.stringify(input))).toThrow(
        SessionProtocolError,
      );
    }
  });

  it("parses strict R3 response DTOs and enforces null targetId/revision on native_session_result", () => {
    const reg = parseSourceSessionRegistration({
      sourceOperationId: id(1),
      actorId: id(2),
      action: "discover",
      sourceId: id(3),
      harnessId: id(4),
      profileId: id(5),
      profileRevision: "7",
      workspaceId: id(6),
      registeredAtMs: "1700000000000",
      state: "registered",
    });
    expect(reg.action).toBe("discover");

    const fact = parseSourceSessionFact({
      registration: {
        sourceOperationId: id(1),
        actorId: id(2),
        action: "discover",
        sourceId: id(3),
        harnessId: id(4),
        profileId: id(5),
        profileRevision: "7",
        workspaceId: id(6),
        registeredAtMs: "1700000000000",
      },
      resultAvailable: true,
      state: "terminal",
      outcome: "succeeded",
      runtimeId: id(7),
      admissionAuthGeneration: "3",
      sourceInstanceId: id(8),
      connectionEpoch: id(9),
      protocolVersion: "2",
      rpcId: { kind: "integer", value: "42" },
      frameLength: "128",
      transport: { kind: "written" },
      response: {
        kind: "sessions",
        nativeSessionId: null,
        nativeKeyAvailable: false,
        remoteErrorCode: null,
        sequence: "1",
        receipt: "2",
        resultBytes: "64",
        ack: "confirmed",
      },
      failure: null,
    });
    expect(fact.resultAvailable).toBe(true);

    const desc = parseSourceSessionResultDescriptor({
      streamKind: "native_session_result",
      streamId: id(10),
      documentId: id(11),
      targetId: null,
      revision: null,
      totalBytes: "64",
      encoding: "utf8_json",
    });
    expect(desc.streamKind).toBe("native_session_result");

    expect(() =>
      parseSourceSessionResultDescriptor({
        streamKind: "native_session_result",
        streamId: id(10),
        documentId: id(11),
        targetId: id(12),
        revision: null,
        totalBytes: "64",
        encoding: "utf8_json",
      }),
    ).toThrow(SessionProtocolError);

    expect(parseSourceSessionResultReleaseResult({ released: true })).toEqual({
      released: true,
    });
  });
});

describe("R3-05 unsupported_capability separation with untouched C50 baseline", () => {
  it("keeps C50 vector baseline at 50 entries and parses/formats unsupported_capability", () => {
    expect(MANAGEMENT_ERROR_CASES).toHaveLength(50);
    const frame = parseManagementFrame(
      JSON.stringify({
        v: 1,
        type: "result",
        requestId: id(5),
        ok: false,
        result: null,
        error: { code: "unsupported_capability" },
      }),
    );
    expect(frame).toMatchObject({
      type: "result",
      ok: false,
      error: { code: "unsupported_capability" },
    });
    const unsupportedMsg = managementRequestErrorMessage("unsupported_capability");
    const platformMsg = managementRequestErrorMessage("platform_unimplemented");
    expect(unsupportedMsg).not.toBe(platformMsg);
    expect(unsupportedMsg).toContain("能力");
  });
});

describe("R3-08 ManagementConnection, bare ACP result chunk reader, and XSS/BiDi neutralization", () => {
  it("executes all 7 R3 ManagementConnection commands, view.rebind, and error propagation", async () => {
    const selection = validSelection();
    const { connection, fake } = await connectFakeManagement((frame, socket) => {
      if (frame.type !== "command") return;
      const reqId = String(frame.requestId);
      switch (frame.action) {
        case "source.session.register":
          socket.receive({
            v: 1,
            type: "result",
            requestId: reqId,
            ok: true,
            result: {
              sourceOperationId: id(40),
              actorId: id(9),
              action: "discover",
              sourceId: selection.sourceId,
              harnessId: selection.harnessId,
              profileId: selection.profileId,
              profileRevision: selection.profileRevision,
              workspaceId: selection.workspaceId,
              registeredAtMs: "1700000000123",
              state: "registered",
            },
            error: null,
          });
          break;
        case "source.session.execute":
        case "source.session.status":
        case "source.session.reconcile":
          socket.receive({
            v: 1,
            type: "result",
            requestId: reqId,
            ok: true,
            result: {
              registration: {
                sourceOperationId: id(40),
                actorId: id(9),
                action: "discover",
                sourceId: selection.sourceId,
                harnessId: selection.harnessId,
                profileId: selection.profileId,
                profileRevision: selection.profileRevision,
                workspaceId: selection.workspaceId,
                registeredAtMs: "1700000000123",
              },
              resultAvailable: true,
              state: "terminal",
              outcome: "succeeded",
              runtimeId: id(41),
              admissionAuthGeneration: "1",
              sourceInstanceId: id(42),
              connectionEpoch: id(43),
              protocolVersion: "2",
              rpcId: { kind: "integer", value: "1" },
              frameLength: "80",
              transport: { kind: "written" },
              response: {
                kind: "sessions",
                nativeSessionId: null,
                nativeKeyAvailable: false,
                remoteErrorCode: null,
                sequence: "1",
                receipt: "1",
                resultBytes: "80",
                ack: "confirmed",
              },
              failure: null,
            },
            error: null,
          });
          break;
        case "source.session.result.open":
          socket.receive({
            v: 1,
            type: "result",
            requestId: reqId,
            ok: true,
            result: {
              streamKind: "native_session_result",
              streamId: id(50),
              documentId: id(51),
              targetId: null,
              revision: null,
              totalBytes: "32",
              encoding: "utf8_json",
            },
            error: null,
          });
          break;
        case "source.session.result.release":
          socket.receive({
            v: 1,
            type: "result",
            requestId: reqId,
            ok: true,
            result: { released: true },
            error: null,
          });
          break;
        case "view.open_source":
          socket.receive({
            v: 1,
            type: "result",
            requestId: reqId,
            ok: false,
            result: null,
            error: { code: "unsupported_capability" },
          });
          break;
        case "view.rebind":
          socket.receive({
            v: 1,
            type: "result",
            requestId: reqId,
            ok: true,
            result: {
              v: 1,
              type: "view.opened",
              requestId: reqId,
              streamId: id(61),
              viewEpoch: id(62),
              snapshotRevision: "1",
              capabilities: { opaqueChunks: 1 },
              status: {
                mode: "current_only",
                freshness: "current_observation",
                projection: "available",
                join: "unassociated_segments",
                historyIncomplete: true,
                cacheRecoverable: false,
              },
            },
            error: null,
          });
          break;
      }
    });

    const reg = await connection.registerSourceSession(selection, {
      kind: "discover",
      cursor: null,
    });
    expect(reg.sourceOperationId).toBe(id(40));
    expect((await connection.executeSourceSession(reg.sourceOperationId)).outcome).toBe("succeeded");
    expect((await connection.inspectSourceSessionStatus(reg.sourceOperationId)).resultAvailable).toBe(true);
    expect((await connection.reconcileSourceSession(reg.sourceOperationId)).state).toBe("terminal");
    expect((await connection.openSourceSessionResult(reg.sourceOperationId)).streamId).toBe(id(50));
    expect((await connection.releaseSourceSessionResult(reg.sourceOperationId)).released).toBe(true);
    await expect(
      connection.openSourceSessionView(selection, "ext-1", "first_open"),
    ).rejects.toMatchObject({ code: "unsupported_capability" });
    expect((await connection.rebindSessionView(id(60))).streamId).toBe(id(61));
    fake.serverClose(1006, "drop");
    await expect(connection.reconcileSourceSession(reg.sourceOperationId)).rejects.toBeInstanceOf(
      ManagementConnectionError,
    );
  });

  it("reassembles bare ACP result/error JSON chunks, rejects jsonrpc wrappers, and renders untrusted strings strictly via textContent", () => {
    const streamId = id(70);
    const sessionsFact: SourceSessionFact = {
      registration: {
        sourceOperationId: id(1),
        actorId: id(2),
        action: "discover",
        sourceId: id(3),
        harnessId: id(4),
        profileId: id(5),
        profileRevision: "7",
        workspaceId: id(6),
        registeredAtMs: "1",
      },
      resultAvailable: true,
      state: "terminal",
      outcome: "succeeded",
      runtimeId: id(7),
      admissionAuthGeneration: "1",
      sourceInstanceId: id(8),
      connectionEpoch: id(9),
      protocolVersion: "2",
      rpcId: { kind: "integer", value: "1" },
      frameLength: "100",
      transport: { kind: "written" },
      response: {
        kind: "sessions",
        nativeSessionId: null,
        nativeKeyAvailable: false,
        remoteErrorCode: null,
        sequence: "1",
        receipt: "1",
        resultBytes: "100",
        ack: "confirmed",
      },
      failure: null,
    };

    const evilTitle = "<script>alert(1)</script><svg onload=x>\u202eevil";
    const payload = ENCODER.encode(
      JSON.stringify({
        sessions: [
          {
            sessionId: "sess-real-1",
            title: evilTitle,
            cwd: "/tmp/\u2066dir",
            updatedAt: "2026-11-15T00:00:00Z",
          },
        ],
        nextCursor: "cursor-2",
      }),
    );

    const reader = new SourceSessionResultReader(streamId, BigInt(payload.length));
    expect(
      reader.accept(
        resultFrame(streamId, 1n, 0n, BigInt(payload.length), payload.slice(0, 17)),
      ),
    ).toEqual({ state: "pending" });
    expect(
      reader.accept(
        resultFrame(
          streamId,
          2n,
          17n,
          BigInt(payload.length),
          payload.slice(17),
        ),
      ),
    ).toEqual({ state: "pending" });
    const done = reader.accept(snapshotEndFrame(streamId, 3n));
    expect(done.state).toBe("complete");
    if (done.state !== "complete") throw new Error("expected complete");

    const inspected = inspectSourceSessionResultPayload(done.rawBytes, sessionsFact);
    expect(inspected.kind).toBe("sessions");
    if (inspected.kind !== "sessions") throw new Error("expected sessions");

    const openedIds: string[] = [];
    const doc = createFakeDocument();
    const dom = sourceSessionResultView(doc, inspected, {
      rawExpanded: false,
      onUseNextCursor: vi.fn(),
      onOpenView: (sid) => openedIds.push(sid),
      onToggleRaw: vi.fn(),
      onRegisterInspector: vi.fn(),
    }) as unknown as StrictDomElement;

    const fullText = collectText(dom);
    expect(fullText).toContain("<script>alert(1)</script>");
    expect(fullText).toContain("U+202E");
    expect(fullText).toContain("U+2066");
    const openBtn = findButtons(dom, "打开只读视图")[0];
    openBtn?.click();
    expect(openedIds).toEqual(["sess-real-1"]);

    // Wrapped jsonrpc must fall back safely rather than accepting outer envelope
    const wrapped = inspectSourceSessionResultPayload(
      ENCODER.encode('{"jsonrpc":"2.0","id":1,"result":{"sessions":[]}}'),
      sessionsFact,
    );
    expect(wrapped.kind).toBe("fallback");
    if (wrapped.kind === "fallback") {
      expect(wrapped.code).toBe("invalid_result_shape");
    }

    // Remote error preserves exact large integer code lexeme
    const errFact: SourceSessionFact = {
      ...sessionsFact,
      response: {
        kind: "remote_error",
        nativeSessionId: null,
        nativeKeyAvailable: false,
        remoteErrorCode: "-320009007199254740993",
        sequence: "1",
        receipt: "1",
        resultBytes: "60",
        ack: "confirmed",
      },
    };
    const errParsed = inspectSourceSessionResultPayload(
      ENCODER.encode('{"code":-320009007199254740993,"message":"denied"}'),
      errFact,
    );
    expect(errParsed).toMatchObject({
      kind: "remote_error",
      errorCode: "-320009007199254740993",
      message: "denied",
    });

    // Declared oversized result stream is rejected before accepting frames
    expect(
      () =>
        new SourceSessionResultReader(
          streamId,
          BigInt(MAX_SOURCE_RESULT_DISPLAY_BYTES + 1),
        ),
    ).toThrowError(new SourceSessionResultReadError("result_too_large"));
  });
});

describe("R3-07 / R3-09 / R3-10 panel isolation, stale preservation, CurrentOnly, and WV15 preservation", () => {
  it("renders source panel purely from user input without product.status and persists source view spec safely", () => {
    const doc = createFakeDocument();
    const dom = sourceSessionPanelView(
      doc,
      {
        connected: true,
        selection: validSelection(),
        cursor: "",
        manualNativeSessionId: "manual-ext-1",
        registration: null,
        fact: null,
        result: null,
        rawExpanded: false,
        activeSourceView: {
          selection: validSelection(),
          nativeSessionId: "manual-ext-1",
        },
        openedView: parseSessionViewOpened({
          v: 1,
          type: "view.opened",
          requestId: id(1),
          streamId: id(2),
          viewEpoch: id(3),
          snapshotRevision: "0",
          capabilities: { opaqueChunks: 1 },
          status: {
            mode: "current_only",
            freshness: "current_observation",
            projection: "available",
            join: "unassociated_segments",
            historyIncomplete: true,
            cacheRecoverable: false,
          },
        }),
        staleNotice: "重新校验失败，保留旧视图",
      },
      {
        onSelectionFieldChange: vi.fn(),
        onCursorChange: vi.fn(),
        onManualNativeSessionIdChange: vi.fn(),
        onRegister: vi.fn(),
        onExecute: vi.fn(),
        onRefreshStatus: vi.fn(),
        onReconcile: vi.fn(),
        onOpenResult: vi.fn(),
        onReleaseResult: vi.fn(),
        onClearOperation: vi.fn(),
        onUseNextCursor: vi.fn(),
        onOpenDiscoveredView: vi.fn(),
        onOpenManualView: vi.fn(),
        onRefreshSourceView: vi.fn(),
        onCloseSourceView: vi.fn(),
        onAbandonSourceView: vi.fn(),
        onToggleRawResult: vi.fn(),
        onRegisterInspector: vi.fn(),
      },
    ) as unknown as StrictDomElement;

    const text = collectText(dom);
    expect(text).toContain("current_only");
    expect(text).toContain("历史不完整");
    expect(findButtons(dom, "按原生 ID 打开只读视图")).toHaveLength(1);
    expect(findButtons(dom, "刷新来源只读视图")).toHaveLength(1);
    expect(findButtons(dom, "关闭只读视图")).toHaveLength(1);
    const staleBanner = dom.children.find(
      (child) => child.dataset.stale === "true",
    );
    expect(staleBanner).toBeDefined();

    // Persistence round-trip rejects forged data cleanly
    const map = new Map<string, string>();
    const storage = {
      getItem: (k: string) => map.get(k) ?? null,
      setItem: (k: string, v: string) => { map.set(k, v); },
      removeItem: (k: string) => { map.delete(k); },
    };
    persistSourceViewSpec(validSelection(), "ext-42", storage);
    expect(loadPersistedSourceViewSpec(storage)).toEqual({
      kind: "source",
      selection: validSelection(),
      nativeSessionId: "ext-42",
    });
    map.set(SOURCE_SHARED_VIEW_STORAGE_KEY, "{bad-json");
    expect(loadPersistedSourceViewSpec(storage)).toBeNull();
    expect(map.has(SOURCE_SHARED_VIEW_STORAGE_KEY)).toBe(false);
  });

  it("invokes reopenRequested when SessionViewDisplay receives stale_rebind status", () => {
    const reopenRequested = vi.fn();
    const credit = vi.fn();
    const detach = vi.fn();
    const doc = createFakeDocument();
    const opened = parseSessionViewOpened({
      v: 1,
      type: "view.opened",
      requestId: id(1),
      streamId: id(80),
      viewEpoch: id(81),
      snapshotRevision: "0",
      capabilities: { opaqueChunks: 1 },
      status: {
        mode: "current_only",
        freshness: "current_observation",
        projection: "available",
        join: "unassociated_segments",
        historyIncomplete: true,
        cacheRecoverable: false,
      },
    });
    const display = new SessionViewDisplay(doc, opened, {
      credit,
      detach,
      reopenRequested,
    });
    display.start();
    const statusJson = ENCODER.encode(
      JSON.stringify({
        type: "stream_rebound",
        previousStreamId: id(80),
        streamId: id(80),
        viewEpoch: id(81),
        snapshotRevision: "0",
      }),
    );
    display.accept({
      type: "data",
      streamId: id(80),
      packetSequence: 1n,
      recordId: 1n,
      byteOffset: 0n,
      totalLength: BigInt(statusJson.length),
      kind: "status",
      last: true,
      payload: statusJson,
    });
    expect(detach).toHaveBeenCalledWith(id(80));
    expect(detach).toHaveBeenCalledOnce();
    expect(display.detached).toBe(true);
    expect(reopenRequested).toHaveBeenCalledOnce();
  });

  it("keeps SessionViewDisplay.detached idempotent and skips duplicate detach in releaseSharedDisplay", () => {
    const reopenRequested = vi.fn();
    const credit = vi.fn();
    const detach = vi.fn();
    const doc = createFakeDocument();
    const opened = parseSessionViewOpened({
      v: 1,
      type: "view.opened",
      requestId: id(1),
      streamId: id(82),
      viewEpoch: id(83),
      snapshotRevision: "0",
      capabilities: { opaqueChunks: 1 },
      status: {
        mode: "current_only",
        freshness: "current_observation",
        projection: "available",
        join: "unassociated_segments",
        historyIncomplete: true,
        cacheRecoverable: false,
      },
    });
    const display = new SessionViewDisplay(doc, opened, { credit, detach, reopenRequested });
    expect(display.detached).toBe(false);
    display.start();
    const statusJson = ENCODER.encode(
      JSON.stringify({
        type: "stream_rebound",
        previousStreamId: id(82),
        streamId: id(82),
        viewEpoch: id(83),
        snapshotRevision: "0",
      }),
    );
    display.accept({
      type: "data",
      streamId: id(82),
      packetSequence: 1n,
      recordId: 1n,
      byteOffset: 0n,
      totalLength: BigInt(statusJson.length),
      kind: "status",
      last: true,
      payload: statusJson,
    });
    expect(display.detached).toBe(true);
    expect(detach).toHaveBeenCalledTimes(1);
    display.accept({
      type: "data",
      streamId: id(82),
      packetSequence: 2n,
      recordId: 2n,
      byteOffset: 0n,
      totalLength: 1n,
      kind: "status",
      last: true,
      payload: new Uint8Array([0]),
    });
    expect(detach).toHaveBeenCalledTimes(1);

    const main = new TextDecoder().decode(readFileSync(new URL("../src/main.ts", import.meta.url)));
    const releaseSection = stripTypeScriptTypes(
      main.slice(
        main.indexOf("function releaseSharedDisplay("),
        main.indexOf("\nfunction suspendSharedView("),
      ),
    );
    const makeRelease = new Function(`${releaseSection}\nreturn releaseSharedDisplay;`);
    const releaseSharedDisplay = makeRelease() as (
      d: { readonly streamId: string; readonly detached?: boolean },
      c: { detachSessionStream: (id: string) => void; closeSessionView: (id: string) => Promise<unknown> } | null,
    ) => void;
    const conn = {
      detachSessionStream: vi.fn(),
      closeSessionView: vi.fn(async () => ({})),
    };
    releaseSharedDisplay(display, conn);
    expect(conn.detachSessionStream).not.toHaveBeenCalled();
    expect(conn.closeSessionView).toHaveBeenCalledWith(id(82));

    conn.detachSessionStream.mockClear();
    conn.closeSessionView.mockClear();
    releaseSharedDisplay({ streamId: id(84), detached: false }, conn);
    expect(conn.detachSessionStream).toHaveBeenCalledWith(id(84));
    expect(conn.closeSessionView).toHaveBeenCalledWith(id(84));
  });

  it("isolates sharedViewSpec source reconnect/stale preservation from inspectedQueueTargetId in main.ts", async () => {
    const main = new TextDecoder().decode(readFileSync(new URL("../src/main.ts", import.meta.url)));
    expect(main).not.toContain("product.status");
    const sourceSection = stripTypeScriptTypes(
      main.slice(
        main.indexOf("function handleSourceResultViewData("),
        main.indexOf("\nasync function loadSessionQueue("),
      ),
    );
    const displays: Array<{ streamId: string; element: { dataset: Record<string, string>; removeAttribute: (k: string) => void } }> = [];
    const connection = {
      openSourceSessionView: vi.fn(),
      rebindSessionView: vi.fn(),
      sendSessionViewCredit: vi.fn(),
      detachSessionStream: vi.fn(),
      closeSessionView: vi.fn(async () => ({ state: "closed", lastCommittedPacket: "0" })),
    };
    const dashboard = vi.fn();
    const releaseSpy = vi.fn();
    const make = new Function(
      "connection",
      "SessionViewDisplay",
      "document",
      "sharedViewHost",
      "showManagementDashboard",
      "managementErrorMessage",
      "clearPersistedSourceViewSpec",
      "persistSourceViewSpec",
      "releaseSharedDisplay",
      "openSharedSessionView",
      "closeSharedSessionView",
      "ManagementConnection",
      `
        let management = connection, managementGeneration = 1;
        let inspectedQueueTargetId = "queue-target-kept";
        let sharedViewTargetId = null, sharedViewSpec = null, sharedViewStaleNotice = null;
        let sharedSessionDisplay = null, sharedSessionConnection = null, openedSessionView = null;
        let sharedViewAttempt = 0, activeSourceResultRead = null, sourceResultTimer = null, sourceResultAttempt = 0;
        let sourceRegistration = null, sourceFact = null, inspectedSourceResult = null, sourceRawInspectorExpanded = false;
        let sourceSelectionDraft = { sourceId: "", harnessId: "", profileId: "", profileRevision: "", workspaceId: "", launchPath: "" };
        let sourceManualNativeIdDraft = "";
        const staleSharedDisplays = new Set(), sourceRawInspectors = new Set();
        ${sourceSection}
        return {
          openSource: openSharedSourceView,
          rebind: rebindSharedView,
          state: () => ({ sharedViewSpec, sharedViewStaleNotice, inspectedQueueTargetId, openedSessionView })
        };
      `,
    );
    const api = make(
      connection,
      class {
        element = {
          dataset: {} as Record<string, string>,
          removeAttribute(key: string) { delete this.dataset[key]; },
        };
        streamId: string;
        start = vi.fn();
        suspend = vi.fn();
        dispose = vi.fn();
        constructor(_owner: unknown, opened: { streamId: string }) {
          this.streamId = opened.streamId;
          displays.push(this);
        }
      },
      {},
      { append: vi.fn() },
      dashboard,
      () => "unavailable",
      vi.fn(),
      vi.fn(),
      releaseSpy,
      vi.fn(),
      vi.fn(),
    );

    const firstOpened = parseSessionViewOpened({
      v: 1,
      type: "view.opened",
      requestId: id(1),
      streamId: id(90),
      viewEpoch: id(91),
      snapshotRevision: "0",
      capabilities: { opaqueChunks: 1 },
      status: {
        mode: "current_only",
        freshness: "current_observation",
        projection: "available",
        join: "unassociated_segments",
        historyIncomplete: true,
        cacheRecoverable: false,
      },
    });
    connection.openSourceSessionView.mockResolvedValueOnce(firstOpened);
    await api.openSource({}, validSelection(), "ext-1", null, "first_open");
    expect(api.state().sharedViewSpec?.kind).toBe("source");
    expect(api.state().inspectedQueueTargetId).toBe("queue-target-kept");

    // On reconnect failure, preserve existing display and mark data-stale="true"
    connection.openSourceSessionView.mockRejectedValueOnce(
      new ManagementRequestError("read_failed"),
    );
    await api.openSource({}, validSelection(), "ext-1", null, "reconnect");
    expect(displays[0]?.element.dataset.stale).toBe("true");
    expect(api.state().sharedViewStaleNotice).toContain("重新校验失败");
    expect(api.state().inspectedQueueTargetId).toBe("queue-target-kept");

    // Successful view.rebind replaces stream cleanly without sending duplicate detach/close for old stream
    const reboundOpened = { ...firstOpened, streamId: id(92) };
    connection.rebindSessionView.mockResolvedValueOnce(reboundOpened);
    await api.rebind({}, firstOpened);
    expect(api.state().openedSessionView.streamId).toBe(id(92));
    expect(api.state().sharedViewStaleNotice).toBeNull();
    expect(releaseSpy).not.toHaveBeenCalled();
    expect(connection.detachSessionStream).not.toHaveBeenCalled();
    expect(connection.closeSessionView).not.toHaveBeenCalled();
  });

  it("preserves sequential WV15 input/Range/textarea stages and WV16 source-session DOM coverage", () => {
    const browserFixture = new TextDecoder().decode(readFileSync(new URL("./browser/session-view-main.ts", import.meta.url)));
    expect(browserFixture).toContain("restoredInput.selectionStart === 2 && restoredInput.selectionEnd === 8");
    expect(browserFixture).toContain("selection.addRange(range)");
    expect(browserFixture).toContain("globalThis.getSelection()?.toString() === selectedText");
    expect(browserFixture).toContain("restoredTextarea.selectionStart === 3 && restoredTextarea.selectionEnd === 9");
    expect(browserFixture).toContain("runWV16");
    expect(browserFixture).toContain('[data-panel="source-session"]');
    expect(browserFixture).toContain("view.open_source");
  });
});
