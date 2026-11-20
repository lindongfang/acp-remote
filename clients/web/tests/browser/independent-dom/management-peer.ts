import {
  CONTROL_PATH,
  CONTROL_PROTOCOL,
} from "../../../src/management/protocol";
import {
  decodeBase64urlStrict,
  encodeBase64url,
  signCompactJws,
  verifyCompactJws,
} from "../../../src/protocol/jws";
import { publicKeyFingerprint } from "../../../src/protocol/pairing";
import { decodeViewFrame, type ViewBinaryFrame } from "../../../src/views/binary";
import { chunkedRecordFrames, snapshotEndFrame } from "./view-wire";

const encoder = new TextEncoder();
const nodeKey = await crypto.subtle.generateKey(
  { name: "ECDSA", namedCurve: "P-256" }, true, ["sign", "verify"],
);
const nodePublicKey = encodeBase64url(new Uint8Array(await crypto.subtle.exportKey("raw", nodeKey.publicKey)));
const nodeId = await publicKeyFingerprint(nodePublicKey);
const clientId = encodeBase64url(new Uint8Array(16).fill(171));
let nextId = 180;
const nextId128 = (): string => encodeBase64url(new Uint8Array(16).fill(nextId++ & 255));

export interface FixtureIdentity {
  readonly origin: string;
  readonly nodeId: string;
  readonly nodePublicKey: string;
  readonly clientId: string;
}

export interface OpenRequest {
  readonly requestId: string;
  readonly targetId: string;
  readonly streamId: string;
}

export interface SourceOpenRequest {
  readonly requestId: string;
  readonly nativeSessionId: string;
  readonly streamId: string;
  readonly selection: Record<string, unknown>;
  readonly reason: string;
}

export interface CommandRecord {
  readonly action: string;
  readonly requestId: string;
  readonly payload: Record<string, unknown>;
}

interface SourceOperationRecord {
  readonly registration: {
    readonly sourceOperationId: string;
    readonly actorId: string;
    readonly action: "discover" | "create";
    readonly sourceId: string;
    readonly harnessId: string;
    readonly profileId: string;
    readonly profileRevision: string;
    readonly workspaceId: string;
    readonly registeredAtMs: string;
  };
  executed: boolean;
  resultReleased: boolean;
  readonly resultJsonText: string;
  readonly resultBytes: Uint8Array;
}

export class MainFixturePeer {
  readonly identity: FixtureIdentity;
  readonly commands: CommandRecord[] = [];
  readonly binaryFromClient: ViewBinaryFrame[] = [];
  readonly pendingOpens: OpenRequest[] = [];
  readonly openedViews: OpenRequest[] = [];
  readonly openedSourceViews: SourceOpenRequest[] = [];
  readonly pendingCloses: Array<{ requestId: string; streamId: string }> = [];
  readonly sockets: MainFixtureSocket[] = [];
  holdOpens = false;
  holdCloses = false;
  failNextInitialCredit = false;
  #clientPublicKey: string | null = null;
  #deviceRevision = 1;
  readonly #sourceOps = new Map<string, SourceOperationRecord>();
  readonly #sourceResultStreams = new Map<string, readonly Uint8Array[]>();
  readonly #sessionControl = new Map<
    string,
    { controllerClientId: string | null; controllerDisplayName: string | null; controlEpoch: number }
  >();

  constructor(origin: string) {
    this.identity = { origin, nodeId, nodePublicKey, clientId };
  }

  install(): void {
    const peer = this;
    class FixtureWebSocket extends MainFixtureSocket {
      constructor(url: string | URL, protocols?: string | string[]) {
        super(peer, String(url), typeof protocols === "string" ? [protocols] : protocols ?? []);
      }
    }
    Object.defineProperty(globalThis, "WebSocket", {
      configurable: true,
      value: FixtureWebSocket,
    });
  }

  createSocket(url: string, protocols: readonly string[]): void {
    if (url !== `${this.identity.origin.replace(/^https:/u, "wss:")}${CONTROL_PATH}` ||
        protocols.length !== 1 || protocols[0] !== CONTROL_PROTOCOL) {
      throw new Error("production main requested an unexpected management WebSocket");
    }
    const socket = this.sockets.at(-1);
    if (socket === undefined || socket.url !== url || socket.readyState >= 2) {
      throw new Error("fixture socket registration mismatch");
    }
  }

  register(socket: MainFixtureSocket): void { this.sockets.push(socket); }

  handleBinaryCredit(socket: MainFixtureSocket, streamId: string, packetSequence: bigint): void {
    const frames = this.#sourceResultStreams.get(streamId);
    if (frames === undefined) return;
    const index = Number(packetSequence);
    if (!Number.isSafeInteger(index) || index < 0 || index >= frames.length) return;
    const nextFrame = frames[index];
    if (nextFrame === undefined) return;
    queueMicrotask(() => {
      if (socket.readyState === 1) {
        socket.receiveBinary(nextFrame);
      }
    });
  }

  async receiveText(socket: MainFixtureSocket, text: string): Promise<void> {
    let frame: Record<string, unknown>;
    try {
      const value: unknown = JSON.parse(text);
      if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error();
      frame = value as Record<string, unknown>;
    } catch { throw new Error("main sent invalid JSON to fixture peer"); }

    if (frame.type === "hello") {
      if (typeof frame.clientPublicKey !== "string" || typeof frame.clientNonce !== "string" ||
          (frame.clientId !== null && typeof frame.clientId !== "string")) {
        throw new Error("hello did not contain production client identity fields");
      }
      this.#clientPublicKey = frame.clientPublicKey;
      await this.sendChallenge(socket, frame);
      return;
    }
    if (frame.type === "proof") {
      if (this.#clientPublicKey === null || typeof frame.proof !== "string") {
        throw new Error("client proof arrived before hello");
      }
      const proof = await verifyCompactJws(
        frame.proof, decodeBase64urlStrict(this.#clientPublicKey), ["acpr-auth-client+jwt"],
      );
      if (proof.payload.nodeId !== this.identity.nodeId || proof.payload.clientId !== this.identity.clientId ||
          proof.payload.origin !== this.identity.origin) {
        throw new Error("client proof did not bind the fixture identity");
      }
      socket.receive({ v: 1, type: "authenticated", clientId: this.identity.clientId, connectionId: idFor(181) });
      return;
    }
    if (frame.type === "subscribe") {
      socket.receive(result(frame.requestId, { resource: "devices" }));
      socket.receive({ v: 1, type: "devices.changed", revision: 1 });
      return;
    }
    if (frame.type !== "command" || typeof frame.action !== "string" ||
        typeof frame.requestId !== "string") {
      throw new Error("unexpected management request shape");
    }
    const payload = isRecord(frame.payload) ? frame.payload : {};
    const request = { action: frame.action, requestId: frame.requestId, payload };
    this.commands.push(request);
    switch (frame.action) {
      case "node.info":
        socket.receive(result(request.requestId, {
          nodeId: this.identity.nodeId, publicKey: this.identity.nodePublicKey,
          origin: this.identity.origin, status: "ready",
        }));
        break;
      case "device.list":
        socket.receive(result(request.requestId, {
          items: [{
            clientId: this.identity.clientId, displayName: "Synthetic main fixture",
            status: "trusted", publicKeyFingerprint: await publicKeyFingerprint(this.#clientPublicKey ?? ""),
            createdAt: 1_800_000_000, revokedAt: null,
          }], nextCursor: null, revision: this.#deviceRevision,
        }));
        break;
      case "catalog.inventory":
        socket.receive(result(request.requestId, {
          sources: [], harnesses: [], profiles: [], workspaces: [], targets: [],
        }));
        break;
      case "input.queue":
        if (typeof payload.targetId !== "string") throw new Error("queue target missing");
        socket.receive(result(request.requestId, {
          targetId: payload.targetId, failureSuspended: false, stopReason: null,
          priorityFailure: null, waiting: [], active: null, priority: null, processing: null,
        }));
        break;
      case "view.open": {
        if (typeof payload.targetId !== "string") throw new Error("view target missing");
        const pending = { requestId: request.requestId, targetId: payload.targetId, streamId: nextId128() };
        if (this.holdOpens) this.pendingOpens.push(pending);
        else this.sendOpened(socket, pending);
        break;
      }
      case "view.open_source": {
        if (!isRecord(payload.selection) || typeof payload.nativeSessionId !== "string" || typeof payload.reason !== "string") {
          throw new Error("view.open_source payload invalid");
        }
        const opened: SourceOpenRequest = {
          requestId: request.requestId,
          nativeSessionId: payload.nativeSessionId,
          streamId: nextId128(),
          selection: payload.selection,
          reason: payload.reason,
        };
        this.openedSourceViews.push(opened);
        this.openedViews.push({
          requestId: opened.requestId,
          targetId: opened.nativeSessionId,
          streamId: opened.streamId,
        });
        socket.receive(result(opened.requestId, {
          v: 1, type: "view.opened", requestId: opened.requestId,
          streamId: opened.streamId, viewEpoch: nextId128(), snapshotRevision: "0",
          capabilities: { opaqueChunks: 1 },
          status: {
            mode: "current_only", freshness: "current_observation", projection: "available",
            join: "unassociated_segments", historyIncomplete: true, cacheRecoverable: false,
          },
        }));
        break;
      }
      case "source.session.register": {
        if (!isRecord(payload.selection) || !isRecord(payload.operation)) {
          throw new Error("source.session.register payload invalid");
        }
        const action = payload.operation.kind;
        if (action !== "discover" && action !== "create") {
          throw new Error("source.session.register operation kind invalid");
        }
        const selection = payload.selection;
        for (const key of ["sourceId", "harnessId", "profileId", "profileRevision", "workspaceId", "launchPath"]) {
          if (typeof selection[key] !== "string" || selection[key].length === 0) {
            throw new Error(`source.session.register missing ${key}`);
          }
        }
        const sourceOperationId = nextId128();
        const resultJsonText = action === "discover"
          ? JSON.stringify({
              sessions: [
                {
                  sessionId: "ext-wv16-1",
                  title: "WV16 Discovered Session",
                  cwd: String(selection.launchPath),
                  updatedAt: "2026-11-15T08:00:00Z",
                },
              ],
              nextCursor: "cursor-wv16-2",
            })
          : JSON.stringify({ sessionId: "ext-wv16-created" });
        const resultBytes = encoder.encode(resultJsonText);
        const registration = {
          sourceOperationId,
          actorId: this.identity.clientId,
          action,
          sourceId: String(selection.sourceId),
          harnessId: String(selection.harnessId),
          profileId: String(selection.profileId),
          profileRevision: String(selection.profileRevision),
          workspaceId: String(selection.workspaceId),
          registeredAtMs: "1800000000000",
        } as const;
        this.#sourceOps.set(sourceOperationId, {
          registration,
          executed: false,
          resultReleased: false,
          resultJsonText,
          resultBytes,
        });
        socket.receive(result(request.requestId, {
          ...registration,
          state: "registered",
        }));
        break;
      }
      case "source.session.execute":
      case "source.session.status":
      case "source.session.reconcile": {
        if (typeof payload.sourceOperationId !== "string") {
          throw new Error(`${frame.action} missing sourceOperationId`);
        }
        const op = this.#sourceOps.get(payload.sourceOperationId);
        if (op === undefined) throw new Error(`unknown sourceOperationId ${payload.sourceOperationId}`);
        if (frame.action === "source.session.execute" || frame.action === "source.session.reconcile") {
          op.executed = true;
        }
        if (!op.executed) {
          socket.receive(result(request.requestId, {
            registration: op.registration,
            resultAvailable: false,
            state: "registered",
            outcome: "pending",
            runtimeId: null,
            admissionAuthGeneration: null,
            sourceInstanceId: null,
            connectionEpoch: null,
            protocolVersion: null,
            rpcId: null,
            frameLength: null,
            transport: null,
            response: null,
            failure: null,
          }));
        } else {
          socket.receive(result(request.requestId, {
            registration: op.registration,
            resultAvailable: !op.resultReleased,
            state: "terminal",
            outcome: "succeeded",
            runtimeId: idFor(191),
            admissionAuthGeneration: "1",
            sourceInstanceId: idFor(192),
            connectionEpoch: idFor(193),
            protocolVersion: "1",
            rpcId: { kind: "integer", value: "1" },
            frameLength: String(op.resultBytes.length),
            transport: { kind: "written" },
            response: {
              kind: op.registration.action === "discover" ? "sessions" : "created",
              nativeSessionId: op.registration.action === "create" ? "ext-wv16-created" : null,
              nativeKeyAvailable: true,
              remoteErrorCode: null,
              sequence: "1",
              receipt: "1",
              resultBytes: String(op.resultBytes.length),
              ack: "confirmed",
            },
            failure: null,
          }));
        }
        break;
      }
      case "source.session.result.open": {
        if (typeof payload.sourceOperationId !== "string") {
          throw new Error("source.session.result.open missing sourceOperationId");
        }
        const op = this.#sourceOps.get(payload.sourceOperationId);
        if (op === undefined || !op.executed || op.resultReleased) {
          throw new Error("source.session.result.open unavailable");
        }
        const streamId = nextId128();
        const chunkSize = Math.max(16, Math.ceil(op.resultBytes.length / 2));
        const dataFrames = chunkedRecordFrames(streamId, 1n, 1n, "original_raw", op.resultJsonText, chunkSize);
        const endFrame = snapshotEndFrame(streamId, BigInt(dataFrames.length + 1), 2n);
        this.#sourceResultStreams.set(streamId, [...dataFrames, endFrame]);
        socket.receive(result(request.requestId, {
          streamKind: "native_session_result",
          streamId,
          documentId: nextId128(),
          targetId: null,
          revision: null,
          totalBytes: String(op.resultBytes.length),
          encoding: "utf8_json",
        }));
        break;
      }
      case "source.session.result.release": {
        if (typeof payload.sourceOperationId !== "string") {
          throw new Error("source.session.result.release missing sourceOperationId");
        }
        const op = this.#sourceOps.get(payload.sourceOperationId);
        if (op === undefined) throw new Error(`unknown sourceOperationId ${payload.sourceOperationId}`);
        op.resultReleased = true;
        socket.receive(result(request.requestId, { released: true }));
        break;
      }
      case "view.close": {
        if (typeof payload.streamId !== "string") throw new Error("close stream missing");
        const pending = { requestId: request.requestId, streamId: payload.streamId };
        if (this.holdCloses) this.pendingCloses.push(pending);
        else this.sendClosed(socket, pending);
        break;
      }
      case "session.control.inspect":
      case "session.control.takeover":
      case "session.control.release": {
        if (typeof payload.targetId !== "string") throw new Error(`${frame.action} target missing`);
        const current = this.#sessionControl.get(payload.targetId) ?? {
          controllerClientId: null,
          controllerDisplayName: null,
          controlEpoch: 0,
        };
        if (frame.action === "session.control.takeover") {
          if (current.controllerClientId !== this.identity.clientId) {
            current.controllerClientId = this.identity.clientId;
            current.controllerDisplayName = "Synthetic main fixture";
            current.controlEpoch += 1;
            this.#sessionControl.set(payload.targetId, current);
          }
        } else if (frame.action === "session.control.release") {
          if (current.controllerClientId !== this.identity.clientId) {
            socket.receive({
              v: 1, type: "result", requestId: request.requestId, ok: false,
              result: null, error: { code: "session_not_controlled" },
            });
            break;
          }
          current.controllerClientId = null;
          current.controllerDisplayName = null;
          current.controlEpoch += 1;
          this.#sessionControl.set(payload.targetId, current);
        }
        socket.receive(result(request.requestId, {
          targetId: payload.targetId,
          rootTargetId: payload.targetId,
          sourceId: idFor(200),
          rootNativeSessionId: `fixture-root-${payload.targetId}`,
          controllerClientId: current.controllerClientId,
          controllerDisplayName: current.controllerDisplayName,
          controlEpoch: String(current.controlEpoch),
        }));
        break;
      }
      default:
        throw new Error(`fixture peer has no response for ${frame.action}`);
    }
  }

  async answerOpen(index: number): Promise<OpenRequest> {
    const pending = this.pendingOpens[index];
    if (pending === undefined) throw new Error("no held view.open response");
    this.pendingOpens.splice(index, 1);
    const socket = this.sockets.find((candidate) => candidate.readyState === 1);
    if (socket === undefined) throw new Error("management socket is absent");
    this.sendOpened(socket, pending);
    return pending;
  }

  private sendOpened(socket: MainFixtureSocket, pending: OpenRequest): void {
    this.openedViews.push(pending);
    socket.receive(result(pending.requestId, {
      v: 1, type: "view.opened", requestId: pending.requestId,
      streamId: pending.streamId, viewEpoch: nextId128(), snapshotRevision: "0",
      capabilities: { opaqueChunks: 1 },
      status: {
        mode: "current_only", freshness: "current_observation", projection: "available",
        join: "proven_native_ids", historyIncomplete: false, cacheRecoverable: true,
      },
    }));
  }

  answerClose(index: number): void {
    const pending = this.pendingCloses[index];
    if (pending === undefined) throw new Error("no held view.close response");
    this.pendingCloses.splice(index, 1);
    const socket = this.sockets.find((candidate) => candidate.readyState === 1);
    if (socket === undefined) throw new Error("management socket is absent");
    this.sendClosed(socket, pending);
  }

  private sendClosed(socket: MainFixtureSocket, pending: { requestId: string; streamId: string }): void {
    socket.receive(result(pending.requestId, {
      streamId: pending.streamId, alreadyClosed: false,
      lastCommittedPacket: "0", startedPacket: "0", state: "closed",
    }));
  }

  push(streamId: string, wire: Uint8Array): void {
    const socket = this.sockets.at(-1);
    if (socket === undefined) throw new Error("management socket is absent");
    const decoded = decodeViewFrame(wire);
    if (decoded.type !== "data" || decoded.streamId !== streamId) {
      throw new Error("fixture tried to inject a frame on the wrong stream");
    }
    socket.receiveBinary(wire);
  }

  notifyDevicesChanged(): number {
    const socket = this.sockets.find((candidate) => candidate.readyState === 1);
    if (socket === undefined) throw new Error("management socket is absent");
    const revision = ++this.#deviceRevision;
    socket.receive({ v: 1, type: "devices.changed", revision });
    return revision;
  }

  private async sendChallenge(socket: MainFixtureSocket, hello: Record<string, unknown>): Promise<void> {
    const now = Math.floor(Date.now() / 1_000);
    const binding = {
      v: 1, nodeId: this.identity.nodeId, nodePublicKey: this.identity.nodePublicKey,
      origin: this.identity.origin, connectionId: idFor(181), challengeId: idFor(182),
      nonce: encodeBase64url(new Uint8Array(32).fill(183)), iat: now, exp: now + 20,
      clientId: hello.clientId, clientPublicKey: hello.clientPublicKey, clientNonce: hello.clientNonce,
    };
    const hostProof = await signCompactJws(
      "acpr-auth-host+jwt", encoder.encode(JSON.stringify(binding)), nodeKey.privateKey,
    );
    const challenge = {
      v: 1, type: "challenge", nodeId: binding.nodeId,
      nodePublicKey: binding.nodePublicKey, origin: binding.origin,
      connectionId: binding.connectionId, challengeId: binding.challengeId,
      nonce: binding.nonce, iat: binding.iat, exp: binding.exp, hostProof,
    };
    const expectedChallengeKeys = [
      "challengeId", "connectionId", "exp", "hostProof", "iat", "nodeId",
      "nodePublicKey", "nonce", "origin", "type", "v",
    ].sort();
    if (JSON.stringify(Object.keys(challenge).sort()) !== JSON.stringify(expectedChallengeKeys)) {
      throw new Error("fixture emitted challenge fields outside the production closed set");
    }
    socket.receive(challenge);
  }
}

export class MainFixtureSocket extends EventTarget {
  readyState = 0;
  binaryType: BinaryType = "blob";
  readonly sentText: Record<string, unknown>[] = [];
  readonly sentBinary: ViewBinaryFrame[] = [];
  readonly closeEvents: Array<{ code: number; reason: string }> = [];
  #peer: MainFixturePeer;
  readonly url: string;
  readonly protocols: readonly string[];

  constructor(peer: MainFixturePeer, url: string, protocols: readonly string[]) {
    super(); this.#peer = peer; this.url = url; this.protocols = protocols;
    peer.register(this);
    peer.createSocket(url, protocols);
    queueMicrotask(() => {
      if (this.readyState !== 0) return;
      this.readyState = 1;
      this.dispatchEvent(new Event("open"));
    });
  }

  send(value: string | ArrayBuffer | ArrayBufferView | Blob): void {
    if (this.readyState !== 1) throw new Error("fixture socket is not open");
    if (typeof value === "string") {
      const parsed: unknown = JSON.parse(value);
      if (!isRecord(parsed)) throw new Error("main sent non-object frame");
      this.sentText.push(parsed);
      void this.#peer.receiveText(this, value).catch((error: unknown) => {
        this.serverClose(1011, error instanceof Error ? error.message : "fixture peer error");
      });
      return;
    }
    const bytes = asBytes(value);
    const frame = decodeViewFrame(bytes);
    if (frame.type === "credit" && frame.packetSequence === 0n && this.#peer.failNextInitialCredit) {
      this.#peer.failNextInitialCredit = false;
      throw new Error("fixture initial credit fault");
    }
    this.sentBinary.push(frame);
    if (frame.type === "credit") {
      this.#peer.handleBinaryCredit(this, frame.streamId, frame.packetSequence);
    }
  }

  receive(value: unknown): void {
    const event = new Event("message") as MessageEvent<string>;
    Object.defineProperty(event, "data", { value: JSON.stringify(value) });
    this.dispatchEvent(event);
  }

  receiveBinary(bytes: Uint8Array): void {
    const event = new Event("message") as MessageEvent<ArrayBuffer>;
    Object.defineProperty(event, "data", { value: bytes.slice().buffer });
    this.dispatchEvent(event);
  }

  close(code = 1000, reason = ""): void {
    if (this.readyState >= 2) return;
    this.readyState = 2;
    queueMicrotask(() => this.serverClose(code, reason, true));
  }

  serverClose(code: number, reason: string, clean = false): void {
    if (this.readyState === 3) return;
    this.readyState = 3;
    this.closeEvents.push({ code, reason });
    const event = new Event("close") as CloseEvent;
    Object.defineProperties(event, { code: { value: code }, reason: { value: reason }, wasClean: { value: clean } });
    this.dispatchEvent(event);
  }
}

function result(requestId: unknown, value: unknown): Record<string, unknown> {
  return { v: 1, type: "result", requestId, ok: true, result: value, error: null };
}
function idFor(value: number): string { return encodeBase64url(new Uint8Array(16).fill(value)); }
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function asBytes(value: ArrayBuffer | ArrayBufferView | Blob): Uint8Array {
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  throw new Error("unexpected Blob from production ManagementConnection");
}
