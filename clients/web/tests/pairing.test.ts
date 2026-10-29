import { describe, expect, it } from "vitest";

import { PairingApi, PairingApiError } from "../src/pairing/api";
import {
  IdentityStoreError,
  IdentityVault,
  type IdentityRecord,
  type IdentityRepository,
  type IdentityLockManager,
} from "../src/pairing/identity";
import {
  PairingSession,
  shouldRecoverPendingManagement,
} from "../src/pairing/session";
import { encodeBase64url, signCompactJws } from "../src/protocol/jws";
import {
  type PairLookup,
  normalizePairingCode,
  verifyClaimResult,
} from "../src/protocol/pairing";

const ORIGIN = "https://node.example.test";

describe("identity initialization cancellation", () => {
  it("does not generate or save an identity after a suspended read completes", async () => {
    const controller = new AbortController();
    let reads = 0;
    let writes = 0;
    const repository: IdentityRepository = {
      load: async () => { reads++; controller.abort(); return null; },
      save: async () => { writes++; },
    };
    await expect(new IdentityVault(repository).initialize(ORIGIN, controller.signal)).rejects.toMatchObject({ name: "AbortError" });
    expect(reads).toBe(1);
    expect(writes).toBe(0);
  });
});

class MemoryIdentityRepository implements IdentityRepository {
  #value: unknown = null;

  async load(): Promise<unknown> {
    return this.#value;
  }

  async save(record: IdentityRecord): Promise<void> {
    this.#value = structuredClone(record);
  }

  set(value: unknown): void {
    this.#value = value;
  }

  async delete(): Promise<void> {
    this.#value = null;
  }
}

class SharedIdentityLocks implements IdentityLockManager {
  #tail: Promise<unknown> = Promise.resolve();
  request<T>(_name: string, operation: () => Promise<T>): Promise<T> {
    const result = this.#tail.then(operation);
    this.#tail = result.catch(() => undefined);
    return result;
  }
}

describe("shared browser identity coordination", () => {
  it("initializes two page vaults with a single shared key", async () => {
    const repository = new MemoryIdentityRepository();
    const locks = new SharedIdentityLocks();
    const pages = [new IdentityVault(repository, crypto, locks), new IdentityVault(repository, crypto, locks)] as const;
    const [first, second] = await Promise.all([pages[0].initialize(ORIGIN), pages[1].initialize(ORIGIN)]);
    expect(first.publicKey).toBe(second.publicKey);
    expect(first.privateKey.extractable).toBe(false);
    await pages[0].selfCheck(first);
    await pages[1].selfCheck(second);
  });

  it("rotates a rejected key once and does not delete another page's replacement", async () => {
    const repository = new MemoryIdentityRepository();
    const locks = new SharedIdentityLocks();
    const first = new IdentityVault(repository, crypto, locks);
    const second = new IdentityVault(repository, crypto, locks);
    const old = await first.initialize(ORIGIN);
    let deletions = 0;
    repository.delete = async () => { deletions++; repository.set(null); };
    const [a, b] = await Promise.all([first.replaceRejectedIdentity(old), second.replaceRejectedIdentity(old)]);
    expect(a.publicKey).not.toBe(old.publicKey);
    expect(a.publicKey).toBe(b.publicKey);
    expect(a.status).toBe("unbound");
    expect(a.clientId).toBeNull();
    expect(a.privateKey.extractable).toBe(false);
    expect((await second.replaceRejectedIdentity(old)).publicKey).toBe(a.publicKey);
    expect(deletions).toBe(1);
    expect((await first.initialize(ORIGIN)).publicKey).toBe(a.publicKey);
    const paired = await first.save({...a, nodeId:"b".repeat(64), nodePublicKey:"fixture-node-public-key", pairingId:encodedBytes(16, 7),
      clientId:encodedBytes(16, 8), status:"consumed", updatedAt:a.updatedAt + 1});
    const late = await second.replaceRejectedIdentity(old);
    expect(late.publicKey).toBe(paired.publicKey);
    expect(late.status).toBe("consumed");
    expect(late.clientId).toBe(paired.clientId);
    expect(deletions).toBe(1);
  });

  it("does not create an identity if the page leaves while waiting for the shared lock", async () => {
    const repository = new MemoryIdentityRepository();
    const locks = new SharedIdentityLocks();
    let release: () => void = () => { throw new Error("lock has not started"); };
    const held = locks.request("acp-remote-access-identity", () => new Promise<void>(resolve => { release = resolve; }));
    await Promise.resolve();
    const controller = new AbortController();
    const initialized = new IdentityVault(repository, crypto, locks).initialize(ORIGIN, controller.signal);
    controller.abort();
    release();
    await held;
    await expect(initialized).rejects.toMatchObject({name:"AbortError"});
    expect(await repository.load()).toBeNull();
  });

  it("rejects a late old-session save without replacing the current key", async () => {
    const repository = new MemoryIdentityRepository();
    const locks = new SharedIdentityLocks();
    const oldPage = new IdentityVault(repository, crypto, locks);
    const newPage = new IdentityVault(repository, crypto, locks);
    const initial = await oldPage.initialize(ORIGIN);
    const pending = await oldPage.save({
      ...initial, nodeId:"b".repeat(64), nodePublicKey:"fixture-node-public-key",
      pairingId:encodedBytes(16, 7), status:"pending_confirmation", updatedAt:initial.updatedAt + 1,
    });
    const stale = new PairingSession(oldPage, new PairingApi(ORIGIN), pending);
    const replacement = await newPage.replaceRejectedIdentity(pending);
    await expect(stale.acceptAuthenticated(encodedBytes(16, 8))).rejects.toMatchObject({code:"identity_changed"});
    await expect(oldPage.save({...pending, updatedAt:pending.updatedAt + 2})).rejects.toMatchObject({code:"identity_changed"});
    expect((await newPage.initialize(ORIGIN)).publicKey).toBe(replacement.publicKey);
    const updated = await newPage.save({...replacement, updatedAt:replacement.updatedAt + 1});
    expect(updated.publicKey).toBe(replacement.publicKey);
    await oldPage.selfCheck(await oldPage.initialize(ORIGIN));
  });
});

function encodedBytes(length: number, fill: number): string {
  return encodeBase64url(new Uint8Array(length).fill(fill));
}

function hex(bytes: Uint8Array): string {
  return [...bytes]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

function jsonResponse(value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

describe("pairing protocol input", () => {
  it("accepts only the exact Crockford alphabet", () => {
    expect(normalizePairingCode("abcd-efgh")).toBe("ABCD-EFGH");
    expect(normalizePairingCode("0123-4567")).toBe("0123-4567");
    for (const invalid of [
      "ABCD-EFGI",
      "ABCD-EFGL",
      "ABCD-EFGO",
      "ABCD-EFGU",
      "ABCD--EFG",
    ]) {
      expect(() => normalizePairingCode(invalid)).toThrowError(
        expect.objectContaining({ code: "invalid_pairing_code" }),
      );
    }
  });
});

describe("pairing host proofs", () => {
  it.each([
    ["another valid pairing", encodedBytes(16, 9), "host_proof_invalid"],
    ["an invalid identifier", "not-an-id", "invalid_response"],
  ])(
    "rejects a claim result bound to %s",
    async (_label, claimedPairingId, expectedCode) => {
    const nodeKeys = (await crypto.subtle.generateKey(
      { name: "ECDSA", namedCurve: "P-256" },
      true,
      ["sign", "verify"],
    )) as CryptoKeyPair;
    const clientKeys = (await crypto.subtle.generateKey(
      { name: "ECDSA", namedCurve: "P-256" },
      true,
      ["sign", "verify"],
    )) as CryptoKeyPair;
    const nodePublicKeyBytes = new Uint8Array(
      await crypto.subtle.exportKey("raw", nodeKeys.publicKey),
    );
    const clientPublicKey = encodeBase64url(
      new Uint8Array(await crypto.subtle.exportKey("raw", clientKeys.publicKey)),
    );
    const lookup: PairLookup = {
      v: 1,
      pairingId: encodedBytes(16, 1),
      nodeId: hex(
        new Uint8Array(await crypto.subtle.digest("SHA-256", nodePublicKeyBytes)),
      ),
      nodePublicKey: encodeBase64url(nodePublicKeyBytes),
      origin: ORIGIN,
      requestId: encodedBytes(16, 2),
      expiresAt: 1_700_000_300,
      challengeId: encodedBytes(16, 3),
      nonce: encodedBytes(32, 4),
      iat: 1_700_000_000,
      exp: 1_700_000_020,
    };
    const payload = new TextEncoder().encode(
      JSON.stringify({
        v: 1,
        purpose: "claim-result",
        nodeId: lookup.nodeId,
        origin: lookup.origin,
        pairingId: claimedPairingId,
        requestId: lookup.requestId,
        clientPublicKey,
        status: "pending_confirmation",
        expiresAt: lookup.expiresAt,
      }),
    );
    const hostProof = await signCompactJws(
      "acpr-host-claim+jwt",
      payload,
      nodeKeys.privateKey,
    );

    await expect(
      verifyClaimResult(hostProof, lookup, clientPublicKey),
    ).rejects.toMatchObject({ code: expectedCode });
    },
  );
});

describe("browser identity and pending claim recovery", () => {
  it("recovers a lost approval result by public-key authentication", () => {
    const pending = {
      status: "pending_confirmation" as const,
      clientId: null,
      nodeId: "a".repeat(64),
      nodePublicKey: "node-public-key",
    };
    const invalidPairing = new PairingApiError("pairing_invalid", 401, null);

    expect(
      shouldRecoverPendingManagement(invalidPairing, pending, false),
    ).toBe(true);
    expect(
      shouldRecoverPendingManagement(invalidPairing, pending, true),
    ).toBe(false);
    expect(
      shouldRecoverPendingManagement(
        invalidPairing,
        { ...pending, nodePublicKey: null },
        false,
      ),
    ).toBe(false);
    expect(
      shouldRecoverPendingManagement(
        new PairingApiError("rate_limited", 429, 3),
        pending,
        false,
      ),
    ).toBe(false);
  });

  it("marks a pending browser consumed after public-key recovery", async () => {
    const repository = new MemoryIdentityRepository();
    const vault = new IdentityVault(repository);
    const initial = await vault.initialize(ORIGIN);
    const pending = await vault.save({
      ...initial,
      nodeId: "b".repeat(64),
      nodePublicKey: "node-public-key",
      pairingId: encodedBytes(16, 7),
      displayName: "Recovered browser",
      status: "pending_confirmation",
      updatedAt: initial.updatedAt + 1,
    });
    const pairing = new PairingSession(
      vault,
      new PairingApi(ORIGIN, async () => {
        throw new Error("network is not used by this transition");
      }),
      pending,
    );

    const recovered = await pairing.acceptAuthenticated(
      encodedBytes(16, 8),
    );
    expect(recovered.status).toBe("consumed");
    expect(recovered.clientId).toBe(encodedBytes(16, 8));
    expect((await vault.initialize(ORIGIN)).clientId).toBe(
      encodedBytes(16, 8),
    );
  });

  it("persists a non-exportable P-256 key and refuses a changed origin", async () => {
    const repository = new MemoryIdentityRepository();
    const vault = new IdentityVault(repository);
    const first = await vault.initialize(ORIGIN);

    expect(first.privateKey.extractable).toBe(false);
    expect(first.publicKey).toMatch(/^[A-Za-z0-9_-]+$/u);
    await expect(
      crypto.subtle.exportKey("pkcs8", first.privateKey),
    ).rejects.toThrow();

    const readBack = await vault.initialize(ORIGIN);
    expect(readBack.publicKey).toBe(first.publicKey);
    await expect(
      vault.initialize("https://other.example.test"),
    ).rejects.toMatchObject({ code: "origin_mismatch" });

    repository.set({ ...first, publicKey: "not-a-point" });
    await expect(vault.initialize(ORIGIN)).rejects.toBeInstanceOf(
      IdentityStoreError,
    );
  });

  it("rehydrates and retries the exact claim proof after a page reload", async () => {
    const repository = new MemoryIdentityRepository();
    const vault = new IdentityVault(repository);
    const initial = await vault.initialize(ORIGIN);
    const nodeKeys = (await crypto.subtle.generateKey(
      { name: "ECDSA", namedCurve: "P-256" },
      true,
      ["sign", "verify"],
    )) as CryptoKeyPair;
    const nodePublicKeyBytes = new Uint8Array(
      await crypto.subtle.exportKey("raw", nodeKeys.publicKey),
    );
    const nodePublicKey = encodeBase64url(nodePublicKeyBytes);
    const nodeId = hex(
      new Uint8Array(
        await crypto.subtle.digest("SHA-256", nodePublicKeyBytes),
      ),
    );
    const pairingId = encodedBytes(16, 1);
    const challengeId = encodedBytes(16, 2);
    const nonce = encodedBytes(32, 3);
    let lookupRequestId = "";
    let firstProof = "";
    let claimCalls = 0;

    const fetchImpl: typeof fetch = async (input, init) => {
      const path = new URL(String(input), ORIGIN).pathname;
      const body = JSON.parse(String(init?.body)) as Record<string, unknown>;
      if (path.endsWith("/lookup")) {
        lookupRequestId = String(body.requestId);
        return jsonResponse({
          v: 1,
          pairingId,
          nodeId,
          nodePublicKey,
          origin: ORIGIN,
          requestId: lookupRequestId,
          expiresAt: 1_700_000_300,
          challengeId,
          nonce,
          iat: 1_700_000_000,
          exp: 1_700_000_020,
        });
      }
      if (path.endsWith("/claim")) {
        claimCalls += 1;
        const proof = String(body.proof);
        if (claimCalls === 1) {
          firstProof = proof;
          throw new TypeError("simulated dropped claim response");
        }
        expect(proof).toBe(firstProof);
        const payload = new TextEncoder().encode(
          JSON.stringify({
            v: 1,
            purpose: "claim-result",
            nodeId,
            origin: ORIGIN,
            pairingId,
            requestId: lookupRequestId,
            clientPublicKey: initial.publicKey,
            status: "pending_confirmation",
            expiresAt: 1_700_000_300,
          }),
        );
        const hostProof = await signCompactJws(
          "acpr-host-claim+jwt",
          payload,
          nodeKeys.privateKey,
        );
        return jsonResponse({ v: 1, hostProof });
      }
      throw new Error(`unexpected request path: ${path}`);
    };

    const firstSession = new PairingSession(
      vault,
      new PairingApi(ORIGIN, fetchImpl),
      initial,
    );
    await expect(
      firstSession.begin("ABCD-EFGH", "Browser Alice"),
    ).rejects.toMatchObject({ code: "invalid_response" });
    expect(firstSession.identity.status).toBe("pending_claim");
    expect(firstSession.canRetryClaim).toBe(true);
    expect(firstProof).not.toBe("");

    const reloaded = await vault.initialize(ORIGIN);
    expect(reloaded.pendingClaim?.proof).toBe(firstProof);
    const resumedSession = new PairingSession(
      vault,
      new PairingApi(ORIGIN, fetchImpl),
      reloaded,
    );
    expect(resumedSession.canRetryClaim).toBe(true);

    await resumedSession.retryClaim();
    expect(claimCalls).toBe(2);
    expect(resumedSession.identity.status).toBe("pending_confirmation");
    expect(resumedSession.identity.pendingClaim).toBeNull();
    expect(resumedSession.canRetryClaim).toBe(false);
  });
});
