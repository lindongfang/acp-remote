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
import {
  publicKeyFingerprint,
  randomId128,
} from "../src/protocol/pairing";

const ORIGIN = "https://node.example.test";
const TEXT_ENCODER = new TextEncoder();

interface TestKey {
  readonly privateKey: CryptoKey;
  readonly publicKey: string;
}

class FakeSocket extends EventTarget {
  readyState = 0;
  readonly sent: string[] = [];
  readonly closeCalls: Array<{
    readonly code: number | undefined;
    readonly reason: string | undefined;
  }> = [];
  onSend:
    | ((value: Record<string, unknown>) => void | Promise<void>)
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

  send(value: string): void {
    this.sent.push(value);
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
        },
      },
    );

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
    const messageTypes = fake.sent.map(
      (value) => (JSON.parse(value) as { type: string }).type,
    );
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
      expect(fake.sent.filter((value) => JSON.parse(value).type === "command")).toHaveLength(1);
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
