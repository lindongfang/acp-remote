import {
  decodeBase64urlStrict,
  encodeBase64url,
  parseCompactJws,
} from "../protocol/jws";
import type { PairLookup } from "../protocol/pairing";

const DATABASE_NAME = "acp-remote-access-v1";
const DATABASE_VERSION = 1;
const IDENTITY_STORE = "identity";
const PRIMARY_ID = "primary";
const PUBLIC_KEY_BYTES = 65;
const SELF_CHECK_BYTES = 32;
const ID128_BYTES = 16;
const NONCE256_BYTES = 32;
const MAX_CHALLENGE_SECONDS = 30;
const NODE_ID_PATTERN = /^[0-9a-f]{64}$/u;
const PAIR_LOOKUP_KEYS = [
  "challengeId",
  "exp",
  "expiresAt",
  "iat",
  "nodeId",
  "nodePublicKey",
  "nonce",
  "origin",
  "pairingId",
  "requestId",
  "v",
] as const;
const PAIR_CLAIM_KEYS = [
  "challengeId",
  "clientPublicKey",
  "displayName",
  "exp",
  "iat",
  "nodeId",
  "nonce",
  "origin",
  "pairingId",
  "requestId",
  "v",
] as const;

export type StoredPairingStatus =
  | "unbound"
  | "pending_claim"
  | "pending_confirmation"
  | "approved"
  | "rejected"
  | "cancelled"
  | "expired"
  | "consumed";

export interface PendingPairClaim {
  readonly lookup: PairLookup;
  readonly proof: string;
}

export interface IdentityRecord {
  readonly schemaVersion: 1;
  readonly id: typeof PRIMARY_ID;
  readonly origin: string;
  readonly privateKey: CryptoKey;
  readonly publicKey: string;
  readonly nodeId: string | null;
  readonly nodePublicKey: string | null;
  readonly pairingId: string | null;
  readonly pendingClaim: PendingPairClaim | null;
  readonly clientId: string | null;
  readonly displayName: string | null;
  readonly status: StoredPairingStatus;
  readonly createdAt: number;
  readonly updatedAt: number;
}

export interface IdentityRepository {
  load(): Promise<unknown>;
  save(record: IdentityRecord): Promise<void>;
  delete?(): Promise<void>;
}

export interface IdentityLockManager {
  request<T>(name: string, operation: () => Promise<T>): Promise<T>;
}

export type IdentityStoreErrorCode =
  | "storage_unavailable"
  | "write_failed"
  | "key_unreadable"
  | "key_self_check_failed"
  | "corrupt_record"
  | "origin_mismatch"
  | "identity_changed"
  | "crypto_unavailable";

export class IdentityStoreError extends Error {
  readonly code: IdentityStoreErrorCode;

  constructor(code: IdentityStoreErrorCode) {
    super(code);
    this.name = "IdentityStoreError";
    this.code = code;
  }
}

export class IdentityVault {
  readonly #repository: IdentityRepository;
  readonly #crypto: Crypto;
  readonly #locks: IdentityLockManager | undefined;

  constructor(
    repository: IdentityRepository,
    cryptoSource = globalThis.crypto,
    locks: IdentityLockManager | undefined = globalThis.navigator?.locks,
  ) {
    if (cryptoSource?.subtle === undefined) {
      throw new IdentityStoreError("crypto_unavailable");
    }
    this.#repository = repository;
    this.#crypto = cryptoSource;
    this.#locks = locks;
  }

  async initialize(origin: string, signal?: AbortSignal): Promise<IdentityRecord> {
    return this.#exclusive(() => this.#initialize(origin, signal));
  }

  async replaceRejectedIdentity(rejected: IdentityRecord, signal?: AbortSignal): Promise<IdentityRecord> {
    return this.#exclusive(async () => {
      const current = await this.#initialize(rejected.origin, signal);
      signal?.throwIfAborted();
      // Another page may already have rotated the rejected identity. Reuse its
      // valid replacement instead of deleting a different page's new key.
      if (current.publicKey !== rejected.publicKey) return current;
      await this.#reset();
      return this.#initialize(rejected.origin, signal);
    });
  }

  #exclusive<T>(operation: () => Promise<T>): Promise<T> {
    return this.#locks === undefined
      ? operation()
      : this.#locks.request("acp-remote-access-identity", operation);
  }

  async #initialize(origin: string, signal?: AbortSignal): Promise<IdentityRecord> {
    signal?.throwIfAborted();
    validateOrigin(origin);
    let existing: unknown;
    try {
      existing = await this.#repository.load();
    } catch {
      throw new IdentityStoreError("storage_unavailable");
    }
    signal?.throwIfAborted();
    if (existing !== null && existing !== undefined) {
      const record = parseStoredRecord(existing);
      if (record.origin !== origin) {
        throw new IdentityStoreError("origin_mismatch");
      }
      await this.selfCheck(record);
      signal?.throwIfAborted();
      return record;
    }

    const pair = await this.#generateKeyPair();
    signal?.throwIfAborted();
    const now = Date.now();
    const record: IdentityRecord = {
      schemaVersion: 1,
      id: PRIMARY_ID,
      origin,
      privateKey: pair.privateKey,
      publicKey: pair.publicKey,
      nodeId: null,
      nodePublicKey: null,
      pairingId: null,
      pendingClaim: null,
      clientId: null,
      displayName: null,
      status: "unbound",
      createdAt: now,
      updatedAt: now,
    };
    try {
      await this.#repository.save(record);
    } catch {
      throw new IdentityStoreError("write_failed");
    }

    let readBack: unknown;
    signal?.throwIfAborted();
    try {
      readBack = await this.#repository.load();
    } catch {
      throw new IdentityStoreError("storage_unavailable");
    }
    if (readBack === null || readBack === undefined) {
      throw new IdentityStoreError("key_self_check_failed");
    }
    const validated = parseStoredRecord(readBack);
    if (
      validated.origin !== origin ||
      validated.publicKey !== record.publicKey
    ) {
      throw new IdentityStoreError("key_self_check_failed");
    }
    await this.selfCheck(validated);
    return validated;
  }

  async save(record: IdentityRecord): Promise<IdentityRecord> {
    const validated = parseStoredRecord(record);
    return this.#exclusive(async () => {
      let stored: unknown;
      try { stored = await this.#repository.load(); }
      catch { throw new IdentityStoreError("storage_unavailable"); }
      if (stored === null || stored === undefined) throw new IdentityStoreError("identity_changed");
      const current = parseStoredRecord(stored);
      if (current.publicKey !== validated.publicKey || current.origin !== validated.origin) {
        throw new IdentityStoreError("identity_changed");
      }
      await this.selfCheck(current);
      await this.selfCheck(validated);
      try { await this.#repository.save(validated); }
      catch { throw new IdentityStoreError("write_failed"); }
      return validated;
    });
  }

  async #reset(): Promise<void> {
    if (this.#repository.delete === undefined) {
      throw new IdentityStoreError("storage_unavailable");
    }
    try {
      await this.#repository.delete();
    } catch {
      throw new IdentityStoreError("storage_unavailable");
    }
  }

  async fingerprint(publicKey: string): Promise<string> {
    const bytes = decodePublicKey(publicKey);
    if (this.#crypto.subtle === undefined) {
      throw new IdentityStoreError("crypto_unavailable");
    }
    const digest = new Uint8Array(
      await this.#crypto.subtle.digest("SHA-256", bytes),
    );
    return [...digest]
      .map((byte) => byte.toString(16).padStart(2, "0"))
      .join("");
  }

  async #generateKeyPair(): Promise<{
    privateKey: CryptoKey;
    publicKey: string;
  }> {
    if (this.#crypto.subtle === undefined) {
      throw new IdentityStoreError("crypto_unavailable");
    }
    let pair: CryptoKeyPair;
    try {
      pair = await this.#crypto.subtle.generateKey(
        { name: "ECDSA", namedCurve: "P-256" },
        false,
        ["sign", "verify"],
      );
    } catch {
      throw new IdentityStoreError("crypto_unavailable");
    }
    if (pair.privateKey.extractable || !pair.publicKey.extractable) {
      throw new IdentityStoreError("key_self_check_failed");
    }
    try {
      const raw = new Uint8Array(
        await this.#crypto.subtle.exportKey("raw", pair.publicKey),
      );
      if (raw.length !== PUBLIC_KEY_BYTES || raw[0] !== 0x04) {
        throw new IdentityStoreError("key_self_check_failed");
      }
      return {
        privateKey: pair.privateKey,
        publicKey: encodeBase64url(raw),
      };
    } catch (error) {
      if (error instanceof IdentityStoreError) {
        throw error;
      }
      throw new IdentityStoreError("key_self_check_failed");
    }
  }

  async selfCheck(record: IdentityRecord): Promise<void> {
    const publicKeyBytes = decodePublicKey(record.publicKey);
    if (this.#crypto.subtle === undefined) {
      throw new IdentityStoreError("crypto_unavailable");
    }
    const challenge = this.#crypto.getRandomValues(
      new Uint8Array(SELF_CHECK_BYTES),
    );
    try {
      const signature = new Uint8Array(
        await this.#crypto.subtle.sign(
          { name: "ECDSA", hash: "SHA-256" },
          record.privateKey,
          challenge,
        ),
      );
      const verificationKey = await this.#crypto.subtle.importKey(
        "raw",
        publicKeyBytes,
        { name: "ECDSA", namedCurve: "P-256" },
        false,
        ["verify"],
      );
      const valid = await this.#crypto.subtle.verify(
        { name: "ECDSA", hash: "SHA-256" },
        verificationKey,
        signature,
        challenge,
      );
      if (!valid) {
        throw new IdentityStoreError("key_self_check_failed");
      }
    } catch (error) {
      if (error instanceof IdentityStoreError) {
        throw error;
      }
      throw new IdentityStoreError("key_unreadable");
    }
  }
}

export class IndexedDbIdentityRepository implements IdentityRepository {
  readonly #factory: IDBFactory;

  constructor(factory = globalThis.indexedDB) {
    if (factory === undefined) {
      throw new IdentityStoreError("storage_unavailable");
    }
    this.#factory = factory;
  }

  async load(): Promise<unknown> {
    const database = await this.#open();
    try {
      return await new Promise<unknown>((resolve, reject) => {
        const transaction = database.transaction(IDENTITY_STORE, "readonly");
        const request = transaction
          .objectStore(IDENTITY_STORE)
          .get(PRIMARY_ID);
        request.addEventListener("success", () => resolve(request.result));
        request.addEventListener("error", () => reject(request.error));
        transaction.addEventListener("abort", () =>
          reject(transaction.error),
        );
      });
    } finally {
      database.close();
    }
  }

  async save(record: IdentityRecord): Promise<void> {
    const database = await this.#open();
    try {
      await new Promise<void>((resolve, reject) => {
        const transaction = database.transaction(IDENTITY_STORE, "readwrite");
        transaction.objectStore(IDENTITY_STORE).put(record);
        transaction.addEventListener("complete", () => resolve());
        transaction.addEventListener("abort", () =>
          reject(transaction.error),
        );
        transaction.addEventListener("error", () =>
          reject(transaction.error),
        );
      });
    } finally {
      database.close();
    }
  }

  async delete(): Promise<void> {
    const database = await this.#open();
    try {
      await new Promise<void>((resolve, reject) => {
        const transaction = database.transaction(IDENTITY_STORE, "readwrite");
        transaction.objectStore(IDENTITY_STORE).delete(PRIMARY_ID);
        transaction.addEventListener("complete", () => resolve());
        transaction.addEventListener("abort", () =>
          reject(transaction.error),
        );
        transaction.addEventListener("error", () =>
          reject(transaction.error),
        );
      });
    } finally {
      database.close();
    }
  }

  async #open(): Promise<IDBDatabase> {
    return await new Promise<IDBDatabase>((resolve, reject) => {
      const request = this.#factory.open(DATABASE_NAME, DATABASE_VERSION);
      request.addEventListener("upgradeneeded", () => {
        const database = request.result;
        if (!database.objectStoreNames.contains(IDENTITY_STORE)) {
          database.createObjectStore(IDENTITY_STORE, { keyPath: "id" });
        }
      });
      request.addEventListener("success", () => {
        const database = request.result;
        database.addEventListener("versionchange", () => database.close());
        resolve(database);
      });
      request.addEventListener("error", () => reject(request.error));
      request.addEventListener("blocked", () =>
        reject(new IdentityStoreError("storage_unavailable")),
      );
    });
  }
}

function parseStoredRecord(value: unknown): IdentityRecord {
  if (
    typeof value !== "object" ||
    value === null ||
    !("schemaVersion" in value) ||
    value.schemaVersion !== 1 ||
    !("id" in value) ||
    value.id !== PRIMARY_ID ||
    !("origin" in value) ||
    typeof value.origin !== "string" ||
    !("privateKey" in value) ||
    !isPrivateCryptoKey(value.privateKey) ||
    !("publicKey" in value) ||
    typeof value.publicKey !== "string" ||
    !("nodeId" in value) ||
    !isNullableString(value.nodeId) ||
    !("nodePublicKey" in value) ||
    !isNullableString(value.nodePublicKey) ||
    !("pairingId" in value) ||
    !isNullableString(value.pairingId) ||
    !("clientId" in value) ||
    !isNullableString(value.clientId) ||
    !("displayName" in value) ||
    !isNullableString(value.displayName) ||
    !("status" in value) ||
    !isPairingStatus(value.status) ||
    !("createdAt" in value) ||
    !isTimestamp(value.createdAt) ||
    !("updatedAt" in value) ||
    !isTimestamp(value.updatedAt)
  ) {
    throw new IdentityStoreError("corrupt_record");
  }
  try {
    validateOrigin(value.origin);
    decodePublicKey(value.publicKey);
  } catch {
    throw new IdentityStoreError("corrupt_record");
  }
  if (
    value.privateKey.type !== "private" ||
    value.privateKey.algorithm.name !== "ECDSA" ||
    !("namedCurve" in value.privateKey.algorithm) ||
    value.privateKey.algorithm.namedCurve !== "P-256" ||
    value.privateKey.extractable
  ) {
    throw new IdentityStoreError("key_unreadable");
  }
  if (
    (value.nodeId === null) !== (value.nodePublicKey === null) ||
    (value.pairingId === null && value.status !== "unbound") ||
    (value.clientId !== null &&
      value.status !== "approved" &&
      value.status !== "consumed")
  ) {
    throw new IdentityStoreError("corrupt_record");
  }
  const pendingClaim =
    "pendingClaim" in value &&
    value.pendingClaim !== null &&
    value.pendingClaim !== undefined
      ? parsePendingClaim(
          value.pendingClaim,
          value.origin,
          value.publicKey,
          value.nodeId,
          value.nodePublicKey,
          value.pairingId,
          value.displayName,
        )
      : null;
  if (pendingClaim !== null && value.status !== "pending_claim") {
    throw new IdentityStoreError("corrupt_record");
  }
  return {
    schemaVersion: 1,
    id: PRIMARY_ID,
    origin: value.origin,
    privateKey: value.privateKey,
    publicKey: value.publicKey,
    nodeId: value.nodeId,
    nodePublicKey: value.nodePublicKey,
    pairingId: value.pairingId,
    pendingClaim,
    clientId: value.clientId,
    displayName: value.displayName,
    status: value.status,
    createdAt: value.createdAt,
    updatedAt: value.updatedAt,
  };
}

function parsePendingClaim(
  value: unknown,
  origin: string,
  publicKey: string,
  nodeId: string | null,
  nodePublicKey: string | null,
  pairingId: string | null,
  displayName: string | null,
): PendingPairClaim {
  if (
    !isObject(value) ||
    !hasExactKeys(value, ["lookup", "proof"]) ||
    typeof value.proof !== "string"
  ) {
    throw new IdentityStoreError("corrupt_record");
  }
  const lookup = parsePersistedLookup(value.lookup, origin);
  if (
    nodeId === null ||
    nodePublicKey === null ||
    pairingId === null ||
    displayName === null ||
    lookup.nodeId !== nodeId ||
    lookup.nodePublicKey !== nodePublicKey ||
    lookup.pairingId !== pairingId
  ) {
    throw new IdentityStoreError("corrupt_record");
  }

  let payload;
  try {
    payload = parseCompactJws(value.proof, ["acpr-pair-claim+jwt"]).payload;
  } catch {
    throw new IdentityStoreError("corrupt_record");
  }
  if (
    !hasExactKeys(payload, PAIR_CLAIM_KEYS) ||
    payload.v !== 1 ||
    payload.nodeId !== lookup.nodeId ||
    payload.origin !== lookup.origin ||
    payload.requestId !== lookup.requestId ||
    payload.challengeId !== lookup.challengeId ||
    payload.nonce !== lookup.nonce ||
    payload.iat !== lookup.iat ||
    payload.exp !== lookup.exp ||
    payload.pairingId !== lookup.pairingId ||
    payload.clientPublicKey !== publicKey ||
    payload.displayName !== displayName
  ) {
    throw new IdentityStoreError("corrupt_record");
  }

  return {
    lookup,
    proof: value.proof,
  };
}

function parsePersistedLookup(value: unknown, expectedOrigin: string): PairLookup {
  if (
    !isObject(value) ||
    !hasExactKeys(value, PAIR_LOOKUP_KEYS) ||
    value.v !== 1 ||
    typeof value.pairingId !== "string" ||
    typeof value.nodeId !== "string" ||
    !NODE_ID_PATTERN.test(value.nodeId) ||
    typeof value.nodePublicKey !== "string" ||
    typeof value.origin !== "string" ||
    value.origin !== expectedOrigin ||
    typeof value.requestId !== "string" ||
    typeof value.challengeId !== "string" ||
    typeof value.nonce !== "string" ||
    !isTimestamp(value.expiresAt) ||
    !isTimestamp(value.iat) ||
    !isTimestamp(value.exp)
  ) {
    throw new IdentityStoreError("corrupt_record");
  }
  try {
    validateOrigin(value.origin);
    decodePublicKey(value.nodePublicKey);
  } catch {
    throw new IdentityStoreError("corrupt_record");
  }
  if (
    !isBase64urlLength(value.pairingId, ID128_BYTES) ||
    !isBase64urlLength(value.requestId, ID128_BYTES) ||
    !isBase64urlLength(value.challengeId, ID128_BYTES) ||
    !isBase64urlLength(value.nonce, NONCE256_BYTES) ||
    value.exp <= value.iat ||
    value.exp - value.iat > MAX_CHALLENGE_SECONDS
  ) {
    throw new IdentityStoreError("corrupt_record");
  }
  return {
    v: 1,
    pairingId: value.pairingId,
    nodeId: value.nodeId,
    nodePublicKey: value.nodePublicKey,
    origin: value.origin,
    requestId: value.requestId,
    expiresAt: value.expiresAt,
    challengeId: value.challengeId,
    nonce: value.nonce,
    iat: value.iat,
    exp: value.exp,
  };
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  expected: readonly string[],
): boolean {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return (
    actual.length === wanted.length &&
    actual.every((key, index) => key === wanted[index])
  );
}

function isBase64urlLength(value: string, expected: number): boolean {
  try {
    return decodeBase64urlStrict(value).length === expected;
  } catch {
    return false;
  }
}

function isPrivateCryptoKey(value: unknown): value is CryptoKey {
  return (
    typeof value === "object" &&
    value !== null &&
    "type" in value &&
    value.type === "private" &&
    "algorithm" in value &&
    typeof value.algorithm === "object" &&
    value.algorithm !== null &&
    "usages" in value &&
    Array.isArray(value.usages) &&
    "extractable" in value &&
    typeof value.extractable === "boolean"
  );
}

function isNullableString(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

function isPairingStatus(value: unknown): value is StoredPairingStatus {
  return (
    value === "unbound" ||
    value === "pending_claim" ||
    value === "pending_confirmation" ||
    value === "approved" ||
    value === "rejected" ||
    value === "cancelled" ||
    value === "expired" ||
    value === "consumed"
  );
}

function isTimestamp(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function decodePublicKey(value: string): Uint8Array<ArrayBuffer> {
  let bytes: Uint8Array;
  try {
    bytes = decodeBase64urlStrict(value);
  } catch {
    throw new IdentityStoreError("corrupt_record");
  }
  if (bytes.length !== PUBLIC_KEY_BYTES || bytes[0] !== 0x04) {
    throw new IdentityStoreError("corrupt_record");
  }
  const copy = new Uint8Array(bytes.length);
  copy.set(bytes);
  return copy;
}

function validateOrigin(value: string): void {
  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    throw new IdentityStoreError("origin_mismatch");
  }
  if (
    parsed.protocol !== "https:" ||
    parsed.origin !== value ||
    parsed.username !== "" ||
    parsed.password !== ""
  ) {
    throw new IdentityStoreError("origin_mismatch");
  }
}

export function publicKeyBase64url(publicKey: CryptoKey): Promise<string> {
  return exportPublicKey(publicKey).then((bytes) => encodeBase64url(bytes));
}

async function exportPublicKey(publicKey: CryptoKey): Promise<Uint8Array> {
  if (globalThis.crypto?.subtle === undefined) {
    throw new IdentityStoreError("crypto_unavailable");
  }
  try {
    const raw = await globalThis.crypto.subtle.exportKey("raw", publicKey);
    const bytes = new Uint8Array(raw);
    decodePublicKey(encodeBase64url(bytes));
    return bytes;
  } catch (error) {
    if (error instanceof IdentityStoreError) {
      throw error;
    }
    throw new IdentityStoreError("key_unreadable");
  }
}
