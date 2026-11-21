import {
  InflightInputAcknowledgementTracker,
  safeReleaseUpload,
  uploadBinaryChunks,
  waitForCatalogUploadReady,
} from "../src/management/upload-transfer";
import { CatalogActionError } from "../src/management/catalog-documents";
import { describe, expect, it, vi } from "vitest";

import {
  ManagementConnection,
  ManagementConnectionError,
  ManagementRequestError,
  type ManagementClose,
} from "../src/management/client";
import {
  ManagementProtocolError,
  parseDevicePage,
  parseManagementFrame,
  type ExpectedIdentity,
} from "../src/management/protocol";
import {
  decodeBase64urlStrict,
  encodeBase64url,
  signCompactJws,
  verifyCompactJws,
} from "../src/protocol/jws";
import { encodeInputData } from "../src/management/input-transfer";
import {
  publicKeyFingerprint,
  randomId128,
} from "../src/protocol/pairing";

const ORIGIN = "https://node.example.test";
import { MANAGEMENT_ERROR_CASES } from "./error-contract-vectors";
import { ManagementReconnect } from "../src/management/reconnect";
const TEXT_ENCODER = new TextEncoder();

interface TestKey {
  readonly privateKey: CryptoKey;
  readonly publicKey: string;
}

class FakeSocket extends EventTarget {
  readyState = 0;
  readonly sent: Array<string | Uint8Array> = [];
  readonly closeCalls: Array<{
    readonly code: number | undefined;
    readonly reason: string | undefined;
  }> = [];
  onSend:
    | ((value: Record<string, unknown>) => void | Promise<void>)
    | null = null;
  onBinary:
    | ((value: Uint8Array) => void | Promise<void>)
    | null = null;
  sendError: unknown = null;

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
      void Promise.resolve(this.onBinary?.(value)).catch((error: unknown) => {
        this.sendError = error;
        this.serverClose(1011, "test server error");
      });
      return;
    }
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(value) as Record<string, unknown>;
    } catch (error) {
      this.sendError = error;
      return;
    }
    void Promise.resolve(this.onSend?.(parsed)).catch((error: unknown) => {
      this.sendError = error;
      this.serverClose(1011, "test server error");
    });
  }

  receive(value: unknown): void {
    const event = new Event("message") as MessageEvent<unknown>;
    Object.defineProperty(event, "data", {
      value: JSON.stringify(value),
    });
    this.dispatchEvent(event);
  }

  receiveBinary(value: Uint8Array): void {
    const event = new Event("message") as MessageEvent<unknown>;
    Object.defineProperty(event, "data", { value });
    this.dispatchEvent(event);
  }

  close(code?: number, reason?: string): void {
    this.closeCalls.push({ code, reason });
    if (this.readyState >= 2) {
      return;
    }
    this.readyState = 2;
    queueMicrotask(() => this.serverClose(code ?? 1000, reason ?? "", true));
  }

  serverClose(code: number, reason: string, clean = false): void {
    if (this.readyState === 3) {
      return;
    }
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

async function keyPair(): Promise<TestKey> {
  const pair = await globalThis.crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  );
  const raw = await globalThis.crypto.subtle.exportKey(
    "raw",
    pair.publicKey,
  );
  return {
    privateKey: pair.privateKey,
    publicKey: encodeBase64url(new Uint8Array(raw)),
  };
}

async function identity(
  node: TestKey,
  client: TestKey,
  clientId: string | null,
): Promise<ExpectedIdentity> {
  return {
    origin: ORIGIN,
    nodeId: await publicKeyFingerprint(node.publicKey),
    nodePublicKey: node.publicKey,
    clientId,
    clientPublicKey: client.publicKey,
  };
}

function id(byte: number): string {
  return encodeBase64url(new Uint8Array(16).fill(byte));
}

function authBinding(
  expected: ExpectedIdentity,
  hello: Record<string, unknown>,
  connectionId: string,
): Record<string, unknown> {
  const now = Math.floor(Date.now() / 1_000);
  return {
    v: 1,
    nodeId: expected.nodeId,
    nodePublicKey: expected.nodePublicKey,
    origin: expected.origin,
    connectionId,
    challengeId: id(4),
    nonce: encodeBase64url(new Uint8Array(32).fill(5)),
    iat: now,
    exp: now + 20,
    clientId: hello.clientId,
    clientPublicKey: hello.clientPublicKey,
    clientNonce: hello.clientNonce,
  };
}

async function challengeMessage(
  expected: ExpectedIdentity,
  signingNode: TestKey,
  hello: Record<string, unknown>,
  connectionId: string,
): Promise<Record<string, unknown>> {
  const binding = authBinding(expected, hello, connectionId);
  const hostProof = await signCompactJws(
    "acpr-auth-host+jwt",
    TEXT_ENCODER.encode(JSON.stringify(binding)),
    signingNode.privateKey,
  );
  return {
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
  };
}

function result(
  requestId: unknown,
  value: unknown,
): Record<string, unknown> {
  return {
    v: 1,
    type: "result",
    requestId,
    ok: true,
    result: value,
    error: null,
  };
}

async function connectedErrorFixture() {
  const node = await keyPair();
  const client = await keyPair();
  const clientId = id(10);
  const expected = await identity(node, client, clientId);
  const fake = new FakeSocket(`${ORIGIN}/unused`, []);
  const factory = vi.fn(() => fake as unknown as WebSocket);
  const connection = new ManagementConnection(expected, client.privateKey, {
    origin: ORIGIN, socketFactory: factory,
  });
  fake.onSend = async (frame) => {
    if (frame.type === "hello") fake.receive(await challengeMessage(expected, node, frame, id(6)));
    if (frame.type === "proof") {
      const proof = await verifyCompactJws(String(frame.proof), decodeBase64urlStrict(client.publicKey), ["acpr-auth-client+jwt"]);
      expect(proof.payload).toMatchObject({ clientId, nodeId: expected.nodeId });
      fake.receive({ v: 1, type: "authenticated", clientId, connectionId: id(6) });
    }
  };
  await connection.connect();
  fake.onSend = null;
  const commands = () => fake.sent.filter((value): value is string => typeof value === "string")
    .map((value) => JSON.parse(value) as Record<string, unknown>).filter((frame) => frame.type === "command");
  const answer = (requestId: unknown) => fake.receive(result(requestId, {
    nodeId: expected.nodeId, publicKey: expected.nodePublicKey, origin: ORIGIN, status: "ready",
  }));
  return { fake, factory, connection, commands, answer };
}

describe("C50 authenticated request isolation", () => {
  it.each(MANAGEMENT_ERROR_CASES)("rejects only mutation A for %s and never replays it", async (code) => {
    const fixture = await connectedErrorFixture();
    const { connection, fake, commands, answer } = fixture;
    let reconnect: ManagementReconnect | undefined;
    try {
      vi.useFakeTimers();
      const mutation = connection.revokeDevice(id(11));
      const capturedError = mutation.then(() => new Error("unexpected mutation success"), (error: unknown) => error);
      const read = connection.nodeInfo();
      let readSettled = false;
      const capturedRead = read.then((value) => { readSettled = true; return { value }; }, (error: unknown) => { readSettled = true; return { error }; });
      const [a, b] = commands();
      expect(a?.action).toBe("device.revoke");
      expect(b?.action).toBe("node.info");
      fake.receive({ v: 1, type: "result", requestId: a?.requestId, ok: false, result: null, error: { code } });
      const error = await capturedError;
      expect(error).toBeInstanceOf(ManagementRequestError);
      expect(error).toMatchObject({ code });
      expect(readSettled).toBe(false);
      expect(fake.closeCalls).toHaveLength(0);
      answer(b?.requestId);
      expect(await capturedRead).toMatchObject({ value: { status: "ready" } });
      const c = connection.nodeInfo();
      const capturedC = c.then((value) => ({ value }), (error: unknown) => ({ error }));
      answer(commands()[2]?.requestId);
      expect(await capturedC).toMatchObject({ value: { status: "ready" } });
      const recovery = vi.fn();
      reconnect = new ManagementReconnect(recovery);
      const delay = reconnect.schedule(error);
      expect(delay).toBe(code === "rate_limited" ? 60_000 : ["node_stopping", "storage_unavailable"].includes(code) ? 2_000 : null);
      vi.advanceTimersByTime(120_000);
      expect(recovery).toHaveBeenCalledTimes(delay === null ? 0 : 1);
      expect(commands()).toHaveLength(3);
      expect(commands().filter((frame) => frame.action === "device.revoke" &&
        (frame.payload as Record<string, unknown>).clientId === id(11))).toHaveLength(1);
      expect(fixture.factory).toHaveBeenCalledTimes(1);
      expect(fake.closeCalls).toHaveLength(0);
      expect(fake.sendError).toBeNull();
    } finally {
      reconnect?.reset();
      connection.disconnect();
      vi.useRealTimers();
    }
  });

  it.each(["unknown_future_error", "revoked", "INVALID_REQUEST", "invalid_request "])(
    "keeps unknown code %s fatal and rejects every pending request", async (code) => {
      const { connection, fake, commands } = await connectedErrorFixture();
      try {
        const first = connection.revokeDevice(id(11)).then(() => null, (error: unknown) => error);
        const second = connection.nodeInfo().then(() => null, (error: unknown) => error);
        fake.receive({ v: 1, type: "result", requestId: commands()[0]?.requestId, ok: false, result: null, error: { code } });
        for (const error of await Promise.all([first, second])) {
          expect(error).toBeInstanceOf(ManagementProtocolError);
          expect(error).toMatchObject({ code: "invalid_frame" });
        }
        expect(fake.closeCalls).toMatchObject([{ code: 4400 }]);
        expect(commands()).toHaveLength(2);
      } finally { connection.disconnect(); }
    },
  );

  it("isolates a late result for a retired manual history refresh", async () => {
    const { connection, fake, commands, answer } = await connectedErrorFixture();
    try {
      const first = connection.openSessionView(id(11), "explicit_refresh", { turns:"5" }, true);
      const oldId = commands()[0]!.requestId;
      fake.receive({ v:1, type:"result", requestId:oldId, ok:false, result:null, error:{code:"not_found"} });
      await expect(first).rejects.toMatchObject({code:"not_found"});
      const current = connection.nodeInfo().catch((error: unknown) => error);
      const currentId = commands()[1]!.requestId;
      const opened = (requestId: unknown, streamId: string) => ({
        v:1, type:"view.opened", requestId, streamId, viewEpoch:id(19), snapshotRevision:"1",
        capabilities:{opaqueChunks:1}, status:{mode:"current_only", freshness:"current_observation",
          projection:"available", join:"proven_native_ids", historyIncomplete:false, cacheRecoverable:true},
      });
      const retry = connection.openSessionView(id(11), "explicit_refresh", {}, true);
      const retryId = commands()[2]!.requestId;
      const active = opened(retryId, id(21));
      fake.receive(result(retryId, active));
      await retry;
      const other = connection.openSessionView(id(12), "first_open");
      const otherId = commands()[3]!.requestId;
      fake.receive(result(otherId, opened(otherId, id(22))));
      await other;
      for (const stream of [id(22), id(20)]) {
        const mismatched = connection.openSessionView(id(11), "explicit_refresh", {}, true);
        const waitingId = commands().at(-1)!.requestId;
        const crossed = result(waitingId, opened(oldId, stream));
        fake.receive(crossed);
        await expect(mismatched).rejects.toMatchObject({code:"invalid_response"});
        fake.receive(crossed);
      }
      const next = connection.openSessionView(id(11), "explicit_refresh", {}, true);
      const nextId = commands().at(-1)!.requestId;
      const repeated = result(nextId, opened(nextId, id(25)));
      fake.receive(repeated);
      fake.receive(repeated);
      await next;
      fake.receive(result(oldId, opened(oldId, id(22))));
      fake.receive(result(retryId, active));
      fake.receive(result(oldId, opened(otherId, id(23))));
      fake.receive(result(oldId, opened(oldId, id(21))));
      const abandoned = opened(oldId, id(20));
      fake.receive(result(oldId, abandoned));
      fake.receive(result(oldId, abandoned));
      answer(currentId);
      await expect(current).resolves.toMatchObject({status:"ready"});
      const closes = commands().filter(frame => frame.action === "view.close");
      expect(closes.map(frame => frame.payload)).toEqual([{streamId:id(20)}]);
      fake.receive(result(closes[0]!.requestId, {streamId:id(20), alreadyClosed:false,
        lastCommittedPacket:"0", startedPacket:"0", state:"closed"}));
      answer(oldId);
      const finalRead = connection.nodeInfo();
      answer(commands().at(-1)!.requestId);
      await finalRead;
      expect(fake.closeCalls).toHaveLength(0);
      const foreign = connection.nodeInfo();
      fake.receive(result(id(90), opened(id(90), id(24))));
      await expect(foreign).rejects.toBeInstanceOf(ManagementProtocolError);
      expect(commands().filter(frame => frame.action === "view.close")).toEqual(closes);
    } finally { connection.disconnect(); }
  });

  it.each(["unknown", "duplicate"] as const)("keeps %s result request identity fatal", async (kind) => {
    const { connection, fake, commands } = await connectedErrorFixture();
    try {
      const first = connection.nodeInfo().then((value) => ({ value }), (error: unknown) => ({ error }));
      const firstId = commands()[0]?.requestId;
      if (kind === "duplicate") {
        fake.receive({ v: 1, type: "result", requestId: firstId, ok: false, result: null, error: { code: "invalid_request" } });
        expect(await first).toMatchObject({ error: { code: "invalid_request" } });
      }
      const second = connection.nodeInfo().then(() => null, (error: unknown) => error);
      fake.receive({ v: 1, type: "result", requestId: kind === "unknown" ? id(90) : firstId,
        ok: false, result: null, error: { code: "invalid_request" } });
      expect(await second).toBeInstanceOf(ManagementProtocolError);
      await first;
      expect(fake.closeCalls).toMatchObject([{ code: 4400 }]);
    } finally { connection.disconnect(); }
  });

  it.each(["challenge", "authentication"] as const)("rejects result frames while awaiting %s", async (phase) => {
    const node = await keyPair();
    const client = await keyPair();
    const clientId = id(10);
    const expected = await identity(node, client, clientId);
    const fake = new FakeSocket(`${ORIGIN}/unused`, []);
    const connection = new ManagementConnection(expected, client.privateKey, { origin: ORIGIN, socketFactory: () => fake as unknown as WebSocket });
    fake.onSend = async (frame) => {
      if (frame.type === "hello" && phase === "authentication") fake.receive(await challengeMessage(expected, node, frame, id(6)));
      if ((phase === "challenge" && frame.type === "hello") || frame.type === "proof") {
        fake.receive({ v: 1, type: "result", requestId: id(8), ok: false, result: null, error: { code: "invalid_request" } });
      }
    };
    try {
      await expect(connection.connect()).rejects.toBeInstanceOf(ManagementProtocolError);
      expect(fake.closeCalls).toMatchObject([{ code: 4400 }]);
    } finally { connection.disconnect(); }
  });
});

describe("management frame parsing", () => {
  it("rejects duplicate, unknown, oversized, and malformed frames", () => {
    const cases = [
      '{"v":1,"type":"devices.changed","revision":1,"revision":2}',
      '{"v":1,"type":"devices.changed","revision":1,"extra":true}',
      JSON.stringify({
        v: 1,
        type: "devices.changed",
        revision: 1,
        padding: "x".repeat(33 * 1024),
      }),
      JSON.stringify({ v: 1, type: "history.invalidated", streamId: id(42), cursor: "extra" }),
      '{"v":1,"type":"history.invalidated","streamId":"bad"}',
      '{"v":1,"type":"unknown"}',
    ];
    for (const value of cases) {
      expect(() => parseManagementFrame(value)).toThrow(
        ManagementProtocolError,
      );
    }
  });

  it("accepts closed result shapes and rejects inconsistent statuses", () => {
    expect(
      parseManagementFrame(
        JSON.stringify({
          v: 1,
          type: "result",
          requestId: id(1),
          ok: false,
          result: null,
          error: { code: "stale_cursor" },
        }),
      ),
    ).toMatchObject({
      type: "result",
      ok: false,
      error: { code: "stale_cursor" },
    });
    expect(() =>
      parseManagementFrame(
        JSON.stringify({
          v: 1,
          type: "result",
          requestId: id(1),
          ok: true,
          result: { accepted: true },
          error: { code: "rate_limited" },
        }),
      ),
    ).toThrow(ManagementProtocolError);
    expect(() =>
      parseManagementFrame(
        JSON.stringify({
          v: 1,
          type: "result",
          requestId: id(1),
          ok: false,
          result: null,
          error: { code: "unknown" },
        }),
      ),
    ).toThrow(ManagementProtocolError);
  });

  it.each(["product_not_ready", "platform_unimplemented", "invalid_target", "invalid_registration", "request_retired", "read_stale", "read_failed", "allocation_failed", "necessary_storage_unavailable", "commit_unconfirmed"])(
    "preserves the closed session read error %s",
    (code) => {
      expect(parseManagementFrame(JSON.stringify({ v: 1, type: "result", requestId: id(1), ok: false,
        result: null, error: { code } }))).toMatchObject({ type: "result", ok: false, error: { code } });
    },
  );

  it("requires minimized device records and at most 32 entries", () => {
    const item = {
      clientId: id(2),
      displayName: "Browser A",
      status: "trusted",
      publicKeyFingerprint: "a".repeat(64),
      createdAt: 1_700_000_000,
      revokedAt: null,
    };
    expect(
      parseDevicePage({
        items: [item],
        nextCursor: null,
        revision: 3,
      }).items,
    ).toHaveLength(1);
    expect(() =>
      parseDevicePage({
        items: Array.from({ length: 33 }, (_, index) => ({
          ...item,
          clientId: id(index + 10),
        })),
        nextCursor: null,
        revision: 3,
      }),
    ).toThrow(ManagementProtocolError);
    expect(() =>
      parseDevicePage({
        items: [{ ...item, revokedAt: 1_700_000_100 }],
        nextCursor: null,
        revision: 3,
      }),
    ).toThrow(ManagementProtocolError);
    expect(() =>
      parseDevicePage({
        items: [{ ...item, publicKey: "full key must not be returned" }],
        nextCursor: null,
        revision: 3,
      }),
    ).toThrow(ManagementProtocolError);
  });
});

describe("management WebSocket client", () => {
  it.each([4401, 4410])("does not trust %s while host proof verification is pending", async (code) => {
    const node = await keyPair();
    const client = await keyPair();
    const expected = await identity(node, client, id(7));
    const fake = new FakeSocket("wss://node.example.test", []);
    fake.onSend = async (frame) => {
      fake.receive(await challengeMessage(expected, node, frame, id(6)));
      fake.serverClose(code, "closed during verification");
    };
    const connection = new ManagementConnection(expected, client.privateKey, {
      origin: ORIGIN,
      socketFactory: () => fake as unknown as WebSocket,
    });
    await expect(connection.connect()).rejects.toMatchObject({ closeCode: code, nodeVerified: false });
    expect(fake.sent).toHaveLength(1);
  });

  it("preserves host proof failure when the peer replies with revoked", async () => {
    const node = await keyPair();
    const otherNode = await keyPair();
    const client = await keyPair();
    const expected = await identity(node, client, id(7));
    const fake = new FakeSocket("wss://node.example.test", []);
    fake.close = () => fake.serverClose(4410, "untrusted revoked");
    fake.onSend = async (frame) => fake.receive(await challengeMessage(expected, otherNode, frame, id(6)));
    const connection = new ManagementConnection(expected, client.privateKey, {
      origin: ORIGIN,
      socketFactory: () => fake as unknown as WebSocket,
    });
    await expect(connection.connect()).rejects.toBeInstanceOf(ManagementProtocolError);
    expect(fake.sent).toHaveLength(1);
  });

  it.each([4401, 4410])("does not trust an early %s as a verified node rejection", async (code) => {
    const node = await keyPair();
    const client = await keyPair();
    const expected = await identity(node, client, id(7));
    const fake = new FakeSocket("wss://node.example.test", []);
    fake.onSend = () => fake.serverClose(code, "rejected");
    const connection = new ManagementConnection(expected, client.privateKey, {
      origin: ORIGIN,
      socketFactory: () => fake as unknown as WebSocket,
    });
    await expect(connection.connect()).rejects.toMatchObject({ closeCode: code, nodeVerified: false });
    expect(fake.sent).toHaveLength(1);
  });

  it.each([4401, 4410])("marks %s after a valid host proof as a verified rejection", async (code) => {
    const node = await keyPair();
    const client = await keyPair();
    const expected = await identity(node, client, id(7));
    const fake = new FakeSocket("wss://node.example.test", []);
    fake.onSend = async (frame) => {
      if (frame.type === "hello") fake.receive(await challengeMessage(expected, node, frame, id(6)));
      if (frame.type === "proof") fake.serverClose(code, "rejected");
    };
    const connection = new ManagementConnection(expected, client.privateKey, {
      origin: ORIGIN,
      socketFactory: () => fake as unknown as WebSocket,
    });
    await expect(connection.connect()).rejects.toMatchObject({ closeCode: code, nodeVerified: true });
  });

  it("verifies the node, signs the challenge, and correlates commands", async () => {
    const node = await keyPair();
    const client = await keyPair();
    const clientId = id(7);
    const expected = await identity(node, client, clientId);
    const fake = new FakeSocket(
      "wss://node.example.test/api/access/v1/control",
      ["acpr.access.v1"],
    );
    const changes: number[] = [];
    const invalidated: string[] = [];
    const connection = new ManagementConnection(
      expected,
      client.privateKey,
      {
        origin: ORIGIN,
        socketFactory: (url, protocols) => {
          expect(url).toBe(
            "wss://node.example.test/api/access/v1/control",
          );
          expect(protocols).toEqual(["acpr.access.v1"]);
          return fake as unknown as WebSocket;
        },
        callbacks: {
          onDevicesChanged: (revision) => changes.push(revision),
          onHistoryInvalidated: stream => invalidated.push(stream),
        },
      },
    );
    const approvalId = id(28);
    const choiceId = id(29);
    const approvalRegistrationId = id(30);
    const releaseOperationId = id(31);
    const approvalStatus = {
      approvalId,
      changeSequence: "4",
      validity: "current_pending",
      association: {
        kind: "registered_session",
        targetId: id(15),
      },
      decision: {
        registrationId: approvalRegistrationId,
        operationId: id(32),
        actorId: clientId,
        admissionAuthGeneration: "5",
        choiceId,
        knownKind: "allow_once",
        acceptedAtMs: "1700000000000",
      },
      delivery: "in_progress",
      sourceClose: "open",
      necessaryFactsMissing: false,
      reconciliationRunning: false,
    } as const;
    const approvalView = {
      approvalId,
      source: {
        sourceId: id(33),
        runtimeId: id(34),
        instanceId: id(35),
        connectionEpoch: id(36),
        protocolVersion: "2",
      },
      context: {
        kind: "verified_input",
        dispatchId: id(37),
      },
      choices: [
        {
          choiceId,
          ordinal: "0",
          originalOptionId: "allow-once",
          originalKind: "allow_once",
          selectable: true,
        },
      ],
      content: {
        availability: "available",
        rawJson: "{\"subject\":\"original\"}",
      },
      limitations: [],
      status: approvalStatus,
    } as const;

    fake.onSend = async (frame) => {
      if (frame.type === "hello") {
        fake.receive(
          await challengeMessage(expected, node, frame, id(6)),
        );
        return;
      }
      if (frame.type === "proof") {
        const proof = await verifyCompactJws(
          String(frame.proof),
          decodeBase64urlStrict(client.publicKey),
          ["acpr-auth-client+jwt"],
        );
        expect(proof.payload).toMatchObject({
          nodeId: expected.nodeId,
          clientId,
          clientPublicKey: client.publicKey,
        });
        fake.receive({
          v: 1,
          type: "authenticated",
          clientId,
          connectionId: id(6),
        });
        return;
      }
      if (frame.type === "command" && frame.action === "node.info") {
        fake.receive(
          result(frame.requestId, {
            nodeId: expected.nodeId,
            publicKey: expected.nodePublicKey,
            origin: ORIGIN,
            status: "ready",
          }),
        );
        return;
      }
      if (frame.type === "command" && frame.action === "device.list") {
        fake.receive(
          result(frame.requestId, {
            items: [
              {
                clientId,
                displayName: "Browser A",
                status: "trusted",
                publicKeyFingerprint: await publicKeyFingerprint(
                  client.publicKey,
                ),
                createdAt: 1_700_000_000,
                revokedAt: null,
              },
            ],
            nextCursor: null,
            revision: 9,
          }),
        );
        return;
      }
      if (frame.type === "subscribe") {
        fake.receive(result(frame.requestId, { resource: "devices" }));
        fake.receive({
          v: 1,
          type: "devices.changed",
          revision: 9,
        });
        return;
      }
      if (frame.type === "command" && frame.action === "input.queue") {
        expect(frame.payload).toEqual({ targetId: id(15) });
        fake.receive(result(frame.requestId, {
          targetId: id(15),
          failureSuspended: true,
          stopReason: "max_tokens",
          priorityFailure: null,
          waiting: [{
            itemId: id(18),
            revision: "2",
            submitterId: id(19),
            editorId: id(20),
            suspended: true,
          }],
          active: null,
          priority: null,
          processing: null,
        }));
        return;
      }
      if (frame.type === "command" && frame.action === "input.register") {
        expect(frame.requestId).toBe(id(16));
        expect(frame.payload).toEqual({
          targetId: id(15),
          operation: { kind: "resume" },
        });
        fake.receive(result(frame.requestId, {
          registrationId: id(16),
          state: "registered",
          action: "input.resume",
          targetId: id(15),
          registerAuthGeneration: "4",
          registeredAtMs: "1700000000000",
        }));
        return;
      }
      if (frame.type === "command" && frame.action === "input.consume") {
        expect(frame.requestId).toBe(id(17));
        expect(frame.payload).toEqual({
          targetId: id(15),
          registrationId: id(16),
          bodyId: null,
        });
        fake.receive(result(frame.requestId, {
          state: "settled",
          returned: false,
          request: {
            registrationId: id(16),
            state: "settled",
            outcome: { kind: "resumed", restored: "1" },
          },
        }));
      }
    };

    const authenticated = await connection.connect();
    expect(authenticated.clientId).toBe(clientId);
    expect(fake.url).toBe(
      "wss://node.example.test/api/access/v1/control",
    );
    expect(fake.protocols).toEqual(["acpr.access.v1"]);

    const info = await connection.nodeInfo();
    expect(info).toMatchObject({
      nodeId: expected.nodeId,
      origin: ORIGIN,
      status: "ready",
    });
    const page = await connection.listDevices(null);
    expect(page).toMatchObject({ revision: 9, nextCursor: null });
    expect(page.items[0]?.clientId).toBe(clientId);
    await connection.subscribeDevices();
    await vi.waitFor(() => expect(changes).toContain(9));
    fake.receive({ v: 1, type: "history.invalidated", streamId: id(42) });
    await vi.waitFor(() => expect(invalidated).toEqual([id(42)]));
    const queue = await connection.inspectSessionQueue(id(15));
    expect(queue).toMatchObject({
      stopReason: "max_tokens",
      priorityFailure: null,
      failureSuspended: true,
    });
    expect(queue.waiting[0]?.suspended).toBe(true);
    await connection.registerSessionResume(id(15), id(16));
    await expect(
      connection.consumeSessionResume(id(15), id(16), id(17)),
    ).resolves.toEqual({
      registrationId: id(16),
      restored: "1",
      returned: false,
    });
    const uploadId = id(21);
    fake.onSend = async (frame) => {
      if (frame.type === "command" && frame.action === "input.upload.begin") {
        expect(frame.payload).toEqual({ targetId: id(15), purpose: "submit" });
        fake.receive(result(frame.requestId, { uploadId }));
        return;
      }
      if (frame.type === "command" && frame.action === "input.upload.status") {
        expect(frame.payload).toEqual({ uploadId });
        fake.receive(result(frame.requestId, {
          uploadId,
          phase: "ready",
          nextOffset: "3",
          bodyId: id(22),
        }));
        return;
      }
      if (frame.type === "command" && frame.action === "input.upload.finish") {
        expect(frame.payload).toEqual({ uploadId, totalBytes: "3" });
        fake.receive(result(frame.requestId, { uploadId }));
        return;
      }
      if (frame.type === "command" && frame.action === "input.register") {
        expect(frame.payload).toEqual({
          targetId: id(15),
          operation: { kind: "submit", bodyId: id(22) },
        });
        fake.receive(result(frame.requestId, {
          registrationId: frame.requestId,
          state: "registered",
          action: "input.submit",
          targetId: id(15),
          registerAuthGeneration: "4",
          registeredAtMs: "1700000000000",
        }));
        return;
      }
      if (frame.type === "command" && frame.action === "input.consume") {
        expect(frame.payload).toEqual({
          targetId: id(15),
          registrationId: id(23),
          bodyId: id(22),
        });
        fake.receive(result(frame.requestId, {
          state: "settled",
          returned: false,
          request: {
            registrationId: id(23),
            state: "settled",
            outcome: { kind: "queued", itemId: id(28), revision: "1" },
          },
        }));
        return;
      }
      if (frame.type === "command" && frame.action === "input.body.open") {
        expect(frame.payload).toEqual({
          targetId: id(15),
          itemId: id(24),
          expectedRevision: "7",
        });
        fake.receive(result(frame.requestId, {
          streamKind: "queued_input_body",
          streamId: id(25),
          documentId: id(24),
          targetId: id(15),
          revision: "7",
          totalBytes: "3",
          encoding: "utf8_json",
        }));
        return;
      }
      if (frame.type === "command" && frame.action === "view.open") {
        expect(frame.payload).toEqual({
          targetId: id(15),
          reason: "first_open",
        });
        fake.receive(result(frame.requestId, {
          v: 1,
          type: "view.opened",
          requestId: frame.requestId,
          streamId: id(26),
          viewEpoch: id(27),
          snapshotRevision: "8",
          capabilities: { opaqueChunks: 1 },
          status: {
            mode: "current_only",
            freshness: "current_observation",
            projection: "available",
            join: "proven_native_ids",
            historyIncomplete: false,
            cacheRecoverable: true,
          },
        }));
        return;
      }
      if (frame.type === "command" && frame.action === "view.close") {
        expect(frame.payload).toEqual({ streamId: id(26) });
        fake.receive(result(frame.requestId, {
          streamId: id(26),
          alreadyClosed: false,
          lastCommittedPacket: "2",
          startedPacket: "2",
          state: "closed",
        }));
        return;
      }
      if (frame.type === "command" && frame.action === "approval.list") {
        expect(frame.payload).toEqual({});
        fake.receive(result(frame.requestId, { items: [approvalView] }));
        return;
      }
      if (frame.type === "command" && frame.action === "approval.read") {
        expect(frame.payload).toEqual({ approvalId });
        fake.receive(result(frame.requestId, approvalView));
        return;
      }
      if (frame.type === "command" && frame.action === "approval.register") {
        expect(frame.payload).toEqual({ approvalId, choiceId });
        fake.receive(result(frame.requestId, {
          approvalId,
          choiceId,
          approvalRegistrationId,
          state: "registered",
        }));
        return;
      }
      if (frame.type === "command" && frame.action === "approval.decide") {
        expect(frame.payload).toEqual({ approvalRegistrationId });
        fake.receive(result(frame.requestId, {
          approvalId,
          approvalRegistrationId,
          state: "accepted",
        }));
        return;
      }
      if (frame.type === "command" && frame.action === "approval.status") {
        expect(frame.payload).toEqual({ approvalId });
        fake.receive(result(frame.requestId, approvalStatus));
        return;
      }
      if (
        frame.type === "command" &&
        frame.action === "native.release.register"
      ) {
        expect(frame.payload).toEqual({ targetId: id(15) });
        fake.receive(result(frame.requestId, {
          releaseOperationId,
          targetId: id(15),
          state: "registered",
        }));
        return;
      }
      if (frame.type === "command" && frame.action === "native.release") {
        expect(frame.payload).toEqual({ releaseOperationId });
        fake.receive(result(frame.requestId, {
          releaseOperationId,
          targetId: id(15),
          state: "released",
          error: null,
        }));
        return;
      }
      if (
        frame.type === "command" &&
        frame.action === "native.release.status"
      ) {
        expect(frame.payload).toEqual({ releaseOperationId });
        fake.receive(result(frame.requestId, {
          releaseOperationId,
          targetId: id(15),
          state: "released",
          error: null,
        }));
      }
    };
    fake.onBinary = (data) => {
      expect(Array.from(data.subarray(0, 8))).toEqual([65, 67, 80, 73, 1, 1, 0, 0]);
      expect(data.subarray(8, 24)).toEqual(decodeBase64urlStrict(uploadId));
      expect(new DataView(data.buffer, data.byteOffset, data.byteLength).getBigUint64(24)).toBe(0n);
      const ack = encodeInputData(uploadId, 0n, Uint8Array.of(1)).slice(0, 32);
      ack[5] = 2;
      new DataView(ack.buffer).setBigUint64(24, BigInt(data.length - 32));
      fake.receiveBinary(ack);
    };
    await expect(connection.beginInputUpload(id(15), "submit")).resolves.toEqual({
      uploadId,
    });
    await expect(
      connection.sendInputData(uploadId, 0n, Uint8Array.of(91, 93, 32)),
    ).resolves.toEqual({ uploadId, nextOffset: 3n });
    await expect(connection.inputUploadStatus(uploadId)).resolves.toMatchObject({
      phase: "ready",
      bodyId: id(22),
      nextOffset: "3",
    });
    await expect(connection.finishInputUpload(uploadId, 3n)).resolves.toEqual({
      uploadId,
    });
    await expect(
      connection.registerSessionInput(id(15), {
        kind: "submit",
        bodyId: id(22),
      }, id(23)),
    ).resolves.toMatchObject({ registrationId: id(23), action: "input.submit" });
    await expect(
      connection.consumeSessionInput(id(15), id(23), id(22)),
    ).resolves.toMatchObject({
      kind: "resolved",
      returned: false,
      request: { registrationId: id(23), state: "settled" },
    });
    await expect(
      connection.openSessionBody(id(15), id(24), "7"),
    ).resolves.toMatchObject({
      streamKind: "queued_input_body",
      streamId: id(25),
      totalBytes: "3",
    });
    const view = await connection.openSessionView(id(15), "first_open");
    expect(view).toMatchObject({
      type: "view.opened",
      streamId: id(26),
      snapshotRevision: "8",
    });
    await expect(
      connection.closeSessionView(view.streamId),
    ).resolves.toMatchObject({
      streamId: id(26),
      state: "closed",
      lastCommittedPacket: "2",
    });
    await expect(connection.listApprovals()).resolves.toMatchObject({
      items: [{ approvalId, content: approvalView.content }],
    });
    await expect(connection.readApproval(approvalId)).resolves.toMatchObject({
      approvalId,
      status: { validity: "current_pending" },
    });
    await expect(
      connection.registerApproval(approvalId, choiceId),
    ).resolves.toEqual({
      approvalId,
      choiceId,
      approvalRegistrationId,
      state: "registered",
    });
    await expect(
      connection.decideApproval(approvalId, approvalRegistrationId),
    ).resolves.toEqual({
      approvalId,
      approvalRegistrationId,
      state: "accepted",
    });
    await expect(
      connection.inspectApprovalStatus(approvalId),
    ).resolves.toEqual(approvalStatus);
    await expect(
      connection.registerNativeRelease(id(15)),
    ).resolves.toEqual({
      releaseOperationId,
      targetId: id(15),
      state: "registered",
    });
    await expect(
      connection.releaseNativeSession(releaseOperationId),
    ).resolves.toEqual({
      releaseOperationId,
      targetId: id(15),
      state: "released",
      error: null,
    });
    await expect(
      connection.inspectNativeReleaseStatus(releaseOperationId),
    ).resolves.toEqual({
      releaseOperationId,
      targetId: id(15),
      state: "released",
      error: null,
    });
    expect(fake.sendError).toBeNull();
    connection.disconnect();
  });

  it("supports public-key identity recovery with clientId null", async () => {
    const node = await keyPair();
    const client = await keyPair();
    const recoveredId = id(8);
    const expected = await identity(node, client, null);
    const fake = new FakeSocket(`${ORIGIN}/unused`, []);
    const connection = new ManagementConnection(
      expected,
      client.privateKey,
      {
        origin: ORIGIN,
        socketFactory: () => fake as unknown as WebSocket,
      },
    );
    fake.onSend = async (frame) => {
      if (frame.type === "hello") {
        expect(frame.clientId).toBeNull();
        fake.receive(
          await challengeMessage(expected, node, frame, id(6)),
        );
      } else if (frame.type === "proof") {
        fake.receive({
          v: 1,
          type: "authenticated",
          clientId: recoveredId,
          connectionId: id(6),
        });
      }
    };

    await expect(connection.connect()).resolves.toMatchObject({
      clientId: recoveredId,
    });
    expect(connection.clientId).toBe(recoveredId);
    connection.disconnect();
  });

  it("refuses a replacement node before sending a client proof", async () => {
    const expectedNode = await keyPair();
    const replacementNode = await keyPair();
    const client = await keyPair();
    const expected = await identity(expectedNode, client, id(9));
    const fake = new FakeSocket(`${ORIGIN}/unused`, []);
    const connection = new ManagementConnection(
      expected,
      client.privateKey,
      {
        origin: ORIGIN,
        socketFactory: () => fake as unknown as WebSocket,
      },
    );
    fake.onSend = async (frame) => {
      if (frame.type === "hello") {
        fake.receive(
          await challengeMessage(
            await identity(replacementNode, client, id(9)),
            replacementNode,
            frame,
            id(6),
          ),
        );
      }
    };

    await expect(connection.connect()).rejects.toMatchObject({
      name: "ManagementProtocolError",
      code: "node_mismatch",
    });
    const messageTypes = fake.sent
      .filter((value): value is string => typeof value === "string")
      .map((value) => (JSON.parse(value) as { type: string }).type);
    expect(messageTypes).toEqual(["hello"]);
    expect(fake.closeCalls[0]?.code).toBe(4400);
  });

  it("preserves a local request timeout instead of reporting a permanent protocol error", async () => {
    const node = await keyPair();
    const client = await keyPair();
    const clientId = id(10);
    const expected = await identity(node, client, clientId);
    const fake = new FakeSocket(`${ORIGIN}/unused`, []);
    const closes: ManagementClose[] = [];
    const connection = new ManagementConnection(expected, client.privateKey, {
      origin: ORIGIN,
      socketFactory: () => fake as unknown as WebSocket,
      callbacks: { onClose: (close) => closes.push(close) },
    });
    fake.onSend = async (frame) => {
      if (frame.type === "hello") {
        fake.receive(await challengeMessage(expected, node, frame, id(6)));
      } else if (frame.type === "proof") {
        fake.receive({ v: 1, type: "authenticated", clientId, connectionId: id(6) });
      }
    };
    await connection.connect();
    vi.useFakeTimers();
    try {
      const pending = connection.nodeInfo();
      const rejection = expect(pending).rejects.toMatchObject({ code: "request_timeout" });
      await vi.advanceTimersByTimeAsync(15_000);
      await rejection;
      expect(closes).toHaveLength(1);
      expect(closes[0]?.code).toBe(4400);
      expect(closes[0]?.error).toMatchObject({ code: "request_timeout" });
      expect(
        fake.sent
          .filter((value): value is string => typeof value === "string")
          .filter((value) => (JSON.parse(value) as { type: string }).type === "command"),
      ).toHaveLength(1);
    } finally {
      connection.disconnect();
      vi.useRealTimers();
    }
  });

  it("surfaces command errors and a revoked close to pending requests", async () => {
    const node = await keyPair();
    const client = await keyPair();
    const clientId = id(10);
    const expected = await identity(node, client, clientId);
    const fake = new FakeSocket(`${ORIGIN}/unused`, []);
    const connection = new ManagementConnection(
      expected,
      client.privateKey,
      {
        origin: ORIGIN,
        socketFactory: () => fake as unknown as WebSocket,
      },
    );
    fake.onSend = async (frame) => {
      if (frame.type === "hello") {
        fake.receive(
          await challengeMessage(expected, node, frame, id(6)),
        );
      } else if (frame.type === "proof") {
        fake.receive({
          v: 1,
          type: "authenticated",
          clientId,
          connectionId: id(6),
        });
      } else if (frame.type === "command") {
        fake.receive({
          v: 1,
          type: "result",
          requestId: frame.requestId,
          ok: false,
          result: null,
          error: { code: "request_conflict" },
        });
      }
    };
    await connection.connect();
    await expect(connection.revokeDevice(id(11))).rejects.toMatchObject({
      name: "ManagementRequestError",
      code: "request_conflict",
    });

    fake.onSend = null;
    const pending = connection.nodeInfo();
    fake.serverClose(4410, "revoked");
    await expect(pending).rejects.toMatchObject({
      name: "ManagementConnectionError",
      code: "revoked",
    });
    expect(new ManagementRequestError("rate_limited").code).toBe(
      "rate_limited",
    );
    expect(
      new ManagementConnectionError("connection_closed", 4401).closeCode,
    ).toBe(4401);
    expect(id(4)).not.toBe(randomId128());
  });
});

describe("upload-transfer helper components", () => {
  it("InflightInputAcknowledgementTracker tracks and accepts matching acknowledgements", async () => {
    const tracker = new InflightInputAcknowledgementTracker();
    expect(tracker.pending).toBeNull();
    expect(tracker.accept({ uploadId: "abc", nextOffset: 10n })).toBe("no_pending");

    let resolvedAck: unknown = null;
    let rejectedError: unknown = null;
    const timeout = globalThis.setTimeout(() => {}, 10_000);
    tracker.track({
      uploadId: "test-upload-id",
      expectedNextOffset: 100n,
      resolve: (ack) => { resolvedAck = ack; },
      reject: (err) => { rejectedError = err; },
      timeout,
    });
    expect(tracker.pending).not.toBeNull();
    expect(tracker.accept({ uploadId: "wrong-upload-id", nextOffset: 100n })).toBe("mismatch");
    expect(tracker.pending).not.toBeNull();
    expect(tracker.accept({ uploadId: "test-upload-id", nextOffset: 50n })).toBe("mismatch");
    expect(tracker.pending).not.toBeNull();

    const matched = tracker.accept({ uploadId: "test-upload-id", nextOffset: 100n });
    expect(matched).toBe("matched");
    expect(tracker.pending).toBeNull();
    expect(resolvedAck).toEqual({ uploadId: "test-upload-id", nextOffset: 100n });
    expect(rejectedError).toBeNull();
  });

  it("InflightInputAcknowledgementTracker rejects and clears pending state", () => {
    const tracker = new InflightInputAcknowledgementTracker();
    let rejectedError: unknown = null;
    const timeout = globalThis.setTimeout(() => {}, 10_000);
    tracker.track({
      uploadId: "test-upload-id",
      expectedNextOffset: 100n,
      resolve: () => {},
      reject: (err) => { rejectedError = err; },
      timeout,
    });
    const err = new Error("connection failure");
    tracker.reject(err);
    expect(rejectedError).toBe(err);
    expect(tracker.pending).toBeNull();

    expect(() => tracker.reject(new Error("second"))).not.toThrow();
  });

  it("uploadBinaryChunks splits data and validates acknowledgements", async () => {
    const data = new Uint8Array(50);
    for (let i = 0; i < 50; i++) data[i] = i;

    const chunksSent: Uint8Array[] = [];
    const total = await uploadBinaryChunks(
      "upload-1",
      data,
      async (uploadId, offset, chunk) => {
        expect(uploadId).toBe("upload-1");
        chunksSent.push(chunk);
        return { uploadId, nextOffset: offset + BigInt(chunk.length) };
      },
      20,
    );
    expect(total).toBe(50n);
    expect(chunksSent).toHaveLength(3);
    expect(chunksSent[0]?.length).toBe(20);
    expect(chunksSent[1]?.length).toBe(20);
    expect(chunksSent[2]?.length).toBe(10);

    await expect(
      uploadBinaryChunks(
        "upload-1",
        data,
        async (_id, offset) => ({ uploadId: "wrong-id", nextOffset: offset + 10n }),
        20,
      ),
    ).rejects.toMatchObject({
      name: "CatalogActionError",
      code: "upload_failed",
    });

    await expect(
      uploadBinaryChunks(
        "upload-1",
        data,
        async (uploadId, offset) => ({ uploadId, nextOffset: offset }),
        20,
      ),
    ).rejects.toMatchObject({
      name: "CatalogActionError",
      code: "upload_failed",
    });
  });

  it("waitForCatalogUploadReady handles ready, failed, mismatch and timeout", async () => {
    await expect(
      waitForCatalogUploadReady(
        "u1",
        async () => ({ uploadId: "u1", phase: "ready", nextOffset: "10" }),
        1000,
        10,
      ),
    ).resolves.toBeUndefined();

    await expect(
      waitForCatalogUploadReady(
        "u1",
        async () => ({ uploadId: "u1", phase: "failed", error: "bad format", nextOffset: "0" }),
        1000,
        10,
      ),
    ).rejects.toMatchObject({
      name: "CatalogActionError",
      code: "upload_failed",
      detail: "bad format",
    });

    await expect(
      waitForCatalogUploadReady(
        "u1",
        async () => ({ uploadId: "u2", phase: "ready", nextOffset: "10" }),
        1000,
        10,
      ),
    ).rejects.toMatchObject({
      name: "CatalogActionError",
      code: "upload_failed",
    });

    await expect(
      waitForCatalogUploadReady(
        "u1",
        async () => ({ uploadId: "u1", phase: "preparing", nextOffset: "10" }),
        20,
        5,
      ),
    ).rejects.toMatchObject({
      name: "CatalogActionError",
      code: "preparation_timeout",
    });
  });

  it("safeReleaseUpload catches release exceptions without throwing", async () => {
    let called = false;
    await safeReleaseUpload(async () => {
      called = true;
      throw new Error("already closed");
    });
    expect(called).toBe(true);
    expect(new CatalogActionError("upload_failed")).toBeInstanceOf(CatalogActionError);
  });
});
