import {
  type JsonObject,
  type JsonValue,
  JwsError,
  decodeBase64urlStrict,
  encodeBase64url,
  parseStrictJsonObject,
  signCompactJws,
  verifyCompactJws,
} from "./jws";

const ID128_BYTES = 16;
const NONCE256_BYTES = 32;
const PUBLIC_KEY_BYTES = 65;
const MAX_DISPLAY_NAME_SCALARS = 80;
const MAX_CHALLENGE_SECONDS = 30;
const NODE_ID_PATTERN = /^[0-9a-f]{64}$/u;
const PAIRING_CODE_PATTERN =
  /^[0-9A-HJKMNP-TV-Z]{4}-?[0-9A-HJKMNP-TV-Z]{4}$/u;
const CONTROL_CHARACTER_PATTERN = /\p{Cc}/u;

export type PairingProtocolErrorCode =
  | "invalid_response"
  | "origin_mismatch"
  | "node_mismatch"
  | "host_proof_invalid"
  | "invalid_display_name"
  | "invalid_pairing_code"
  | "invalid_key"
  | "crypto_unavailable";

export class PairingProtocolError extends Error {
  readonly code: PairingProtocolErrorCode;

  constructor(code: PairingProtocolErrorCode) {
    super(code);
    this.name = "PairingProtocolError";
    this.code = code;
  }
}

export interface PairLookup {
  readonly v: 1;
  readonly pairingId: string;
  readonly nodeId: string;
  readonly nodePublicKey: string;
  readonly origin: string;
  readonly requestId: string;
  readonly expiresAt: number;
  readonly challengeId: string;
  readonly nonce: string;
  readonly iat: number;
  readonly exp: number;
}

export interface PairChallenge {
  readonly v: 1;
  readonly nodeId: string;
  readonly nodePublicKey: string;
  readonly origin: string;
  readonly requestId: string;
  readonly challengeId: string;
  readonly nonce: string;
  readonly iat: number;
  readonly exp: number;
}

export type PairStatusValue =
  | "pending_confirmation"
  | "approved"
  | "rejected"
  | "cancelled"
  | "expired"
  | "consumed";

export interface HostProofResponse {
  readonly v: 1;
  readonly hostProof: string;
}

export interface PairClaimResult {
  readonly status: "pending_confirmation";
  readonly expiresAt: number;
}

export interface PairStatusResult {
  readonly status: PairStatusValue;
  readonly clientId: string | null;
  readonly expiresAt: number;
}

export interface PairingApiErrorBody {
  readonly v: 1;
  readonly code: string;
  readonly retryAfterSeconds: number | null;
}

export function normalizePairingCode(value: string): string {
  const normalized = value.trim().toUpperCase();
  if (!PAIRING_CODE_PATTERN.test(normalized)) {
    throw new PairingProtocolError("invalid_pairing_code");
  }
  return normalized;
}

export function validateDisplayName(value: string): string {
  if (
    value.length === 0 ||
    hasUnpairedSurrogate(value) ||
    CONTROL_CHARACTER_PATTERN.test(value) ||
    Array.from(value).length > MAX_DISPLAY_NAME_SCALARS
  ) {
    throw new PairingProtocolError("invalid_display_name");
  }
  return value;
}

export function randomId128(source = globalThis.crypto): string {
  if (source?.getRandomValues === undefined) {
    throw new PairingProtocolError("crypto_unavailable");
  }
  const bytes = source.getRandomValues(new Uint8Array(ID128_BYTES));
  return encodeBase64url(bytes);
}

export async function parseLookupResponse(
  bytes: Uint8Array,
  expectedOrigin: string,
  expectedRequestId: string,
): Promise<PairLookup> {
  const object = parseResponseObject(bytes);
  assertExactKeys(object, [
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
  ]);
  const lookup: PairLookup = {
    v: expectLiteral(object.v, 1),
    pairingId: expectId128(object.pairingId),
    nodeId: expectNodeId(object.nodeId),
    nodePublicKey: expectPublicKey(object.nodePublicKey),
    origin: expectOrigin(object.origin, expectedOrigin),
    requestId: expectId128(object.requestId),
    expiresAt: expectSafeInteger(object.expiresAt),
    challengeId: expectId128(object.challengeId),
    nonce: expectNonce(object.nonce),
    iat: expectSafeInteger(object.iat),
    exp: expectSafeInteger(object.exp),
  };
  if (lookup.requestId !== expectedRequestId) {
    throw new PairingProtocolError("invalid_response");
  }
  validateChallengeWindow(lookup);
  await verifyNodeIdentity(lookup.nodeId, lookup.nodePublicKey);
  return lookup;
}

export async function parseChallengeResponse(
  bytes: Uint8Array,
  expectedOrigin: string,
  expectedNodeId: string,
  expectedNodePublicKey: string,
  expectedRequestId: string,
): Promise<PairChallenge> {
  const object = parseResponseObject(bytes);
  assertExactKeys(object, [
    "challengeId",
    "exp",
    "iat",
    "nodeId",
    "nodePublicKey",
    "nonce",
    "origin",
    "requestId",
    "v",
  ]);
  const challenge: PairChallenge = {
    v: expectLiteral(object.v, 1),
    nodeId: expectNodeId(object.nodeId),
    nodePublicKey: expectPublicKey(object.nodePublicKey),
    origin: expectOrigin(object.origin, expectedOrigin),
    requestId: expectId128(object.requestId),
    challengeId: expectId128(object.challengeId),
    nonce: expectNonce(object.nonce),
    iat: expectSafeInteger(object.iat),
    exp: expectSafeInteger(object.exp),
  };
  validateChallengeWindow(challenge);
  if (
    challenge.nodeId !== expectedNodeId ||
    challenge.nodePublicKey !== expectedNodePublicKey
  ) {
    throw new PairingProtocolError("node_mismatch");
  }
  if (challenge.requestId !== expectedRequestId) {
    throw new PairingProtocolError("invalid_response");
  }
  await verifyNodeIdentity(challenge.nodeId, challenge.nodePublicKey);
  return challenge;
}

export function parseHostProofResponse(bytes: Uint8Array): HostProofResponse {
  const object = parseResponseObject(bytes);
  assertExactKeys(object, ["hostProof", "v"]);
  return {
    v: expectLiteral(object.v, 1),
    hostProof: expectString(object.hostProof),
  };
}

export function parseApiErrorBody(bytes: Uint8Array): PairingApiErrorBody {
  const object = parseResponseObject(bytes);
  assertExactKeys(object, ["error", "v"]);
  const error = expectObject(object.error);
  assertExactKeys(error, ["code", "retryAfterSeconds"]);
  const retryAfterSeconds =
    error.retryAfterSeconds === null
      ? null
      : expectNonnegativeSafeInteger(error.retryAfterSeconds);
  return {
    v: expectLiteral(object.v, 1),
    code: expectString(error.code),
    retryAfterSeconds,
  };
}

export async function createPairClaim(
  lookup: PairLookup,
  clientPublicKey: string,
  displayName: string,
  signingKey: CryptoKey,
): Promise<string> {
  const payload = utf8Json({
    v: 1,
    nodeId: lookup.nodeId,
    origin: lookup.origin,
    requestId: lookup.requestId,
    challengeId: lookup.challengeId,
    nonce: lookup.nonce,
    iat: lookup.iat,
    exp: lookup.exp,
    pairingId: lookup.pairingId,
    clientPublicKey,
    displayName: validateDisplayName(displayName),
  });
  return signCompactJws("acpr-pair-claim+jwt", payload, signingKey);
}

export async function createPairStatus(
  challenge: PairChallenge,
  pairingId: string,
  clientPublicKey: string,
  signingKey: CryptoKey,
): Promise<string> {
  const payload = utf8Json({
    v: 1,
    nodeId: challenge.nodeId,
    origin: challenge.origin,
    requestId: challenge.requestId,
    challengeId: challenge.challengeId,
    nonce: challenge.nonce,
    iat: challenge.iat,
    exp: challenge.exp,
    pairingId,
    clientPublicKey,
  });
  return signCompactJws("acpr-pair-status+jwt", payload, signingKey);
}

export async function verifyClaimResult(
  compact: string,
  lookup: PairLookup,
  clientPublicKey: string,
): Promise<PairClaimResult> {
  const parsed = await verifyHostProof(
    compact,
    lookup.nodePublicKey,
    "acpr-host-claim+jwt",
  );
  assertExactKeys(parsed, [
    "clientPublicKey",
    "expiresAt",
    "nodeId",
    "origin",
    "pairingId",
    "purpose",
    "requestId",
    "status",
    "v",
  ]);
  expectPayloadIdentity(parsed, lookup, clientPublicKey);
  const pairingId = expectId128(parsed.pairingId);
  if (
    parsed.purpose !== "claim-result" ||
    pairingId !== lookup.pairingId
  ) {
    throw new PairingProtocolError("host_proof_invalid");
  }
  if (parsed.status !== "pending_confirmation") {
    throw new PairingProtocolError("host_proof_invalid");
  }
  return {
    status: "pending_confirmation",
    expiresAt: expectSafeInteger(parsed.expiresAt),
  };
}

export async function verifyStatusResult(
  compact: string,
  challenge: PairChallenge,
  pairingId: string,
  clientPublicKey: string,
): Promise<PairStatusResult> {
  const parsed = await verifyHostProof(
    compact,
    challenge.nodePublicKey,
    "acpr-host-status+jwt",
  );
  assertExactKeys(parsed, [
    "clientId",
    "clientPublicKey",
    "expiresAt",
    "nodeId",
    "origin",
    "pairingId",
    "purpose",
    "requestId",
    "status",
    "v",
  ]);
  expectPayloadIdentity(parsed, challenge, clientPublicKey);
  if (
    parsed.purpose !== "status-result" ||
    parsed.pairingId !== pairingId
  ) {
    throw new PairingProtocolError("host_proof_invalid");
  }
  const status = expectStatus(parsed.status);
  const clientId = parsed.clientId === null ? null : expectId128(parsed.clientId);
  const approved = status === "approved" || status === "consumed";
  if (approved !== (clientId !== null)) {
    throw new PairingProtocolError("host_proof_invalid");
  }
  return {
    status,
    clientId,
    expiresAt: expectSafeInteger(parsed.expiresAt),
  };
}

export async function publicKeyFingerprint(
  publicKey: string,
): Promise<string> {
  const bytes = decodePublicKey(publicKey);
  return bytesToHex(await sha256(bytes));
}

function parseResponseObject(bytes: Uint8Array): JsonObject {
  try {
    return parseStrictJsonObject(bytes);
  } catch (error) {
    if (error instanceof JwsError) {
      throw new PairingProtocolError("invalid_response");
    }
    throw error;
  }
}

async function verifyHostProof(
  compact: string,
  nodePublicKey: string,
  type: "acpr-host-claim+jwt" | "acpr-host-status+jwt",
): Promise<JsonObject> {
  try {
    return (
      await verifyCompactJws(compact, decodePublicKey(nodePublicKey), [type])
    ).payload;
  } catch {
    throw new PairingProtocolError("host_proof_invalid");
  }
}

function expectPayloadIdentity(
  payload: JsonObject,
  challenge: Pick<PairChallenge, "nodeId" | "origin" | "requestId">,
  clientPublicKey: string,
): void {
  if (
    payload.v !== 1 ||
    payload.nodeId !== challenge.nodeId ||
    payload.origin !== challenge.origin ||
    payload.requestId !== challenge.requestId ||
    payload.clientPublicKey !== clientPublicKey
  ) {
    throw new PairingProtocolError("host_proof_invalid");
  }
}

async function verifyNodeIdentity(
  nodeId: string,
  nodePublicKey: string,
): Promise<void> {
  const fingerprint = await publicKeyFingerprint(nodePublicKey);
  if (fingerprint !== nodeId) {
    throw new PairingProtocolError("node_mismatch");
  }
}

function validateChallengeWindow(value: {
  readonly iat: number;
  readonly exp: number;
}): void {
  if (
    value.exp <= value.iat ||
    value.exp - value.iat > MAX_CHALLENGE_SECONDS
  ) {
    throw new PairingProtocolError("invalid_response");
  }
}

function decodePublicKey(value: string): Uint8Array<ArrayBuffer> {
  let bytes: Uint8Array;
  try {
    bytes = decodeBase64urlStrict(value);
  } catch {
    throw new PairingProtocolError("invalid_key");
  }
  if (bytes.length !== PUBLIC_KEY_BYTES || bytes[0] !== 0x04) {
    throw new PairingProtocolError("invalid_key");
  }
  const copy = new Uint8Array(bytes.length);
  copy.set(bytes);
  return copy;
}

async function sha256(
  bytes: Uint8Array<ArrayBuffer>,
): Promise<Uint8Array<ArrayBuffer>> {
  if (globalThis.crypto?.subtle === undefined) {
    throw new PairingProtocolError("crypto_unavailable");
  }
  return new Uint8Array(await globalThis.crypto.subtle.digest("SHA-256", bytes));
}

function utf8Json(value: Record<string, JsonValue>): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(value));
}

function assertExactKeys(
  object: JsonObject,
  expected: readonly string[],
): void {
  const actual = Object.keys(object).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((key, index) => key !== wanted[index])
  ) {
    throw new PairingProtocolError("invalid_response");
  }
}

function expectObject(value: JsonValue | undefined): JsonObject {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new PairingProtocolError("invalid_response");
  }
  return value;
}

function expectString(value: JsonValue | undefined): string {
  if (typeof value !== "string") {
    throw new PairingProtocolError("invalid_response");
  }
  return value;
}

function expectLiteral<T extends number | string | boolean>(
  value: JsonValue | undefined,
  expected: T,
): T {
  if (value !== expected) {
    throw new PairingProtocolError("invalid_response");
  }
  return expected;
}

function expectSafeInteger(value: JsonValue | undefined): number {
  if (
    typeof value !== "number" ||
    !Number.isSafeInteger(value) ||
    value < 0
  ) {
    throw new PairingProtocolError("invalid_response");
  }
  return value;
}

function expectNonnegativeSafeInteger(
  value: JsonValue | undefined,
): number {
  return expectSafeInteger(value);
}

function expectId128(value: JsonValue | undefined): string {
  const encoded = expectString(value);
  try {
    if (decodeBase64urlStrict(encoded).length !== ID128_BYTES) {
      throw new PairingProtocolError("invalid_response");
    }
  } catch {
    throw new PairingProtocolError("invalid_response");
  }
  return encoded;
}

function expectNonce(value: JsonValue | undefined): string {
  const encoded = expectString(value);
  try {
    if (decodeBase64urlStrict(encoded).length !== NONCE256_BYTES) {
      throw new PairingProtocolError("invalid_response");
    }
  } catch {
    throw new PairingProtocolError("invalid_response");
  }
  return encoded;
}

function expectPublicKey(value: JsonValue | undefined): string {
  const encoded = expectString(value);
  decodePublicKey(encoded);
  return encoded;
}

function expectNodeId(value: JsonValue | undefined): string {
  const nodeId = expectString(value);
  if (!NODE_ID_PATTERN.test(nodeId)) {
    throw new PairingProtocolError("invalid_response");
  }
  return nodeId;
}

function expectOrigin(
  value: JsonValue | undefined,
  expectedOrigin: string,
): string {
  const origin = expectString(value);
  if (origin !== expectedOrigin) {
    throw new PairingProtocolError("origin_mismatch");
  }
  return origin;
}

function expectStatus(value: JsonValue | undefined): PairStatusValue {
  if (
    value !== "pending_confirmation" &&
    value !== "approved" &&
    value !== "rejected" &&
    value !== "cancelled" &&
    value !== "expired" &&
    value !== "consumed"
  ) {
    throw new PairingProtocolError("host_proof_invalid");
  }
  return value;
}

function bytesToHex(bytes: Uint8Array): string {
  let result = "";
  for (const byte of bytes) {
    result += byte.toString(16).padStart(2, "0");
  }
  return result;
}

function hasUnpairedSurrogate(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (next < 0xdc00 || next > 0xdfff) {
        return true;
      }
      index += 1;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      return true;
    }
  }
  return false;
}
