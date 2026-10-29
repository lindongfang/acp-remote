import {
  type JsonObject,
  type JsonValue,
  decodeBase64urlStrict,
  encodeBase64url,
  parseStrictJsonObject,
  signCompactJws,
  verifyCompactJws,
} from "../protocol/jws";
import {
  publicKeyFingerprint,
  validateDisplayName,
} from "../protocol/pairing";

export const CONTROL_PATH = "/api/access/v1/control";
export const CONTROL_PROTOCOL = "acpr.access.v1";
export const MAX_MANAGEMENT_FRAME_BYTES = 32 * 1024;

const ID128_BYTES = 16;
const NONCE256_BYTES = 32;
const PUBLIC_KEY_BYTES = 65;
const MAX_CHALLENGE_SECONDS = 30;
const NODE_ID_PATTERN = /^[0-9a-f]{64}$/u;
const FINGERPRINT_PATTERN = /^[0-9a-f]{64}$/u;
const TEXT_ENCODER = new TextEncoder();

const AUTH_BINDING_KEYS = [
  "challengeId",
  "clientId",
  "clientNonce",
  "clientPublicKey",
  "connectionId",
  "exp",
  "iat",
  "nodeId",
  "nodePublicKey",
  "nonce",
  "origin",
  "v",
] as const;
const CHALLENGE_KEYS = [
  "challengeId",
  "connectionId",
  "exp",
  "hostProof",
  "iat",
  "nodeId",
  "nodePublicKey",
  "nonce",
  "origin",
  "type",
  "v",
] as const;
const AUTHENTICATED_KEYS = [
  "clientId",
  "connectionId",
  "type",
  "v",
] as const;
const RESULT_KEYS = [
  "error",
  "ok",
  "requestId",
  "result",
  "type",
  "v",
] as const;
const DEVICES_CHANGED_KEYS = ["revision", "type", "v"] as const;

export type ManagementErrorCode =
  | "invalid_request"
  | "not_found"
  | "request_conflict"
  | "stale_cursor"
  | "rate_limited"
  | "storage_unavailable"
  | "node_stopping";

export type ManagementProtocolErrorCode =
  | "invalid_frame"
  | "invalid_response"
  | "origin_mismatch"
  | "node_mismatch"
  | "host_proof_invalid"
  | "invalid_key";

export class ManagementProtocolError extends Error {
  readonly code: ManagementProtocolErrorCode;

  constructor(code: ManagementProtocolErrorCode) {
    super(code);
    this.name = "ManagementProtocolError";
    this.code = code;
  }
}

export interface HelloFrame {
  readonly v: 1;
  readonly type: "hello";
  readonly clientId: string | null;
  readonly clientPublicKey: string;
  readonly clientNonce: string;
}

export interface AuthBinding {
  readonly v: 1;
  readonly nodeId: string;
  readonly nodePublicKey: string;
  readonly origin: string;
  readonly connectionId: string;
  readonly challengeId: string;
  readonly nonce: string;
  readonly iat: number;
  readonly exp: number;
  readonly clientId: string | null;
  readonly clientPublicKey: string;
  readonly clientNonce: string;
}

export interface ChallengeFrame {
  readonly v: 1;
  readonly type: "challenge";
  readonly nodeId: string;
  readonly nodePublicKey: string;
  readonly origin: string;
  readonly connectionId: string;
  readonly challengeId: string;
  readonly nonce: string;
  readonly iat: number;
  readonly exp: number;
  readonly hostProof: string;
}

export interface AuthenticatedFrame {
  readonly v: 1;
  readonly type: "authenticated";
  readonly clientId: string;
  readonly connectionId: string;
}

export interface DevicesChangedFrame {
  readonly v: 1;
  readonly type: "devices.changed";
  readonly revision: number;
}

export interface CommandResultFrame {
  readonly v: 1;
  readonly type: "result";
  readonly requestId: string;
  readonly ok: boolean;
  readonly result: JsonValue;
  readonly error: { readonly code: ManagementErrorCode } | null;
}

export type ManagementFrame =
  | ChallengeFrame
  | AuthenticatedFrame
  | DevicesChangedFrame
  | CommandResultFrame;

export interface NodeInfo {
  readonly nodeId: string;
  readonly publicKey: string;
  readonly origin: string;
  readonly status: "ready";
}

export type DeviceStatus = "trusted" | "revoked";

export interface DeviceSummary {
  readonly clientId: string;
  readonly displayName: string;
  readonly status: DeviceStatus;
  readonly publicKeyFingerprint: string;
  readonly createdAt: number;
  readonly revokedAt: number | null;
}

export interface DevicePage {
  readonly items: readonly DeviceSummary[];
  readonly nextCursor: string | null;
  readonly revision: number;
}

export interface RevokeResult {
  readonly clientId: string;
  readonly status: "revoked";
}

export interface ExpectedIdentity {
  readonly origin: string;
  readonly nodeId: string;
  readonly nodePublicKey: string;
  readonly clientId: string | null;
  readonly clientPublicKey: string;
}

export function parseManagementFrame(input: string): ManagementFrame {
  if (TEXT_ENCODER.encode(input).length > MAX_MANAGEMENT_FRAME_BYTES) {
    throw new ManagementProtocolError("invalid_frame");
  }

  let object: JsonObject;
  try {
    object = parseStrictJsonObject(TEXT_ENCODER.encode(input));
  } catch {
    throw new ManagementProtocolError("invalid_frame");
  }
  if (object.v !== 1 || typeof object.type !== "string") {
    throw new ManagementProtocolError("invalid_frame");
  }

  switch (object.type) {
    case "challenge":
      return parseChallenge(object);
    case "authenticated":
      return parseAuthenticated(object);
    case "devices.changed":
      return parseDevicesChanged(object);
    case "result":
      return parseCommandResult(object);
    default:
      throw new ManagementProtocolError("invalid_frame");
  }
}

export function createHello(
  clientId: string | null,
  clientPublicKey: string,
  nonce: string,
): HelloFrame {
  expectPublicKey(clientPublicKey);
  expectNonce(nonce);
  if (clientId !== null) {
    expectId128(clientId);
  }
  return {
    v: 1,
    type: "hello",
    clientId,
    clientPublicKey,
    clientNonce: nonce,
  };
}

export function randomNonce(source = globalThis.crypto): string {
  if (source?.getRandomValues === undefined) {
    throw new ManagementProtocolError("invalid_frame");
  }
  return encodeBase64url(
    source.getRandomValues(new Uint8Array(NONCE256_BYTES)),
  );
}

export async function verifyChallenge(
  challenge: ChallengeFrame,
  expected: ExpectedIdentity,
  hello: HelloFrame,
): Promise<AuthBinding> {
  if (
    challenge.origin !== expected.origin ||
    challenge.nodeId !== expected.nodeId ||
    challenge.nodePublicKey !== expected.nodePublicKey ||
    hello.clientPublicKey !== expected.clientPublicKey ||
    hello.clientId !== expected.clientId
  ) {
    throw new ManagementProtocolError(
      challenge.origin !== expected.origin
        ? "origin_mismatch"
        : "node_mismatch",
    );
  }
  validateChallengeWindow(challenge);
  if (
    (await publicKeyFingerprint(challenge.nodePublicKey)) !== challenge.nodeId
  ) {
    throw new ManagementProtocolError("node_mismatch");
  }

  let payload: JsonObject;
  try {
    payload = (
      await verifyCompactJws(
        challenge.hostProof,
        decodePublicKey(challenge.nodePublicKey),
        ["acpr-auth-host+jwt"],
      )
    ).payload;
  } catch {
    throw new ManagementProtocolError("host_proof_invalid");
  }
  assertExactKeys(payload, AUTH_BINDING_KEYS, "host_proof_invalid");
  const binding: AuthBinding = {
    v: expectLiteral(payload.v, 1),
    nodeId: expectNodeId(payload.nodeId),
    nodePublicKey: expectPublicKey(payload.nodePublicKey),
    origin: expectString(payload.origin),
    connectionId: expectId128(payload.connectionId),
    challengeId: expectId128(payload.challengeId),
    nonce: expectNonce(payload.nonce),
    iat: expectSafeInteger(payload.iat),
    exp: expectSafeInteger(payload.exp),
    clientId:
      payload.clientId === null ? null : expectId128(payload.clientId),
    clientPublicKey: expectPublicKey(payload.clientPublicKey),
    clientNonce: expectNonce(payload.clientNonce),
  };
  if (
    binding.nodeId !== challenge.nodeId ||
    binding.nodePublicKey !== challenge.nodePublicKey ||
    binding.origin !== challenge.origin ||
    binding.connectionId !== challenge.connectionId ||
    binding.challengeId !== challenge.challengeId ||
    binding.nonce !== challenge.nonce ||
    binding.iat !== challenge.iat ||
    binding.exp !== challenge.exp ||
    binding.clientId !== hello.clientId ||
    binding.clientPublicKey !== hello.clientPublicKey ||
    binding.clientNonce !== hello.clientNonce
  ) {
    throw new ManagementProtocolError("host_proof_invalid");
  }
  validateChallengeWindow(binding);
  return binding;
}

export async function createClientProof(
  binding: AuthBinding,
  privateKey: CryptoKey,
): Promise<string> {
  return await signCompactJws(
    "acpr-auth-client+jwt",
    TEXT_ENCODER.encode(JSON.stringify(binding)),
    privateKey,
  );
}

export function parseAuthenticated(
  object: JsonObject,
): AuthenticatedFrame {
  assertExactKeys(object, AUTHENTICATED_KEYS, "invalid_response");
  return {
    v: expectLiteral(object.v, 1),
    type: expectLiteral(object.type, "authenticated"),
    clientId: expectId128(object.clientId),
    connectionId: expectId128(object.connectionId),
  };
}

export async function parseNodeInfoResult(
  value: JsonValue,
  expected: ExpectedIdentity,
): Promise<NodeInfo> {
  const object = expectObject(value);
  assertExactKeys(
    object,
    ["nodeId", "origin", "publicKey", "status"],
    "invalid_response",
  );
  const info: NodeInfo = {
    nodeId: expectNodeId(object.nodeId),
    publicKey: expectPublicKey(object.publicKey),
    origin: expectOrigin(object.origin, expected.origin),
    status: expectLiteral(object.status, "ready"),
  };
  if (
    info.nodeId !== expected.nodeId ||
    info.publicKey !== expected.nodePublicKey ||
    (await publicKeyFingerprint(info.publicKey)) !== info.nodeId
  ) {
    throw new ManagementProtocolError("node_mismatch");
  }
  return info;
}

export function parseDevicePage(value: JsonValue): DevicePage {
  const object = expectObject(value);
  assertExactKeys(
    object,
    ["items", "nextCursor", "revision"],
    "invalid_response",
  );
  if (!Array.isArray(object.items) || object.items.length > 32) {
    throw new ManagementProtocolError("invalid_response");
  }
  const items = object.items.map(parseDeviceSummary);
  const nextCursor =
    object.nextCursor === null
      ? null
      : expectCursor(object.nextCursor);
  return {
    items,
    nextCursor,
    revision: expectSafeInteger(object.revision),
  };
}

export function parseRevokeResult(value: JsonValue): RevokeResult {
  const object = expectObject(value);
  assertExactKeys(
    object,
    ["clientId", "status"],
    "invalid_response",
  );
  return {
    clientId: expectId128(object.clientId),
    status: expectLiteral(object.status, "revoked"),
  };
}

export function parseSubscribeResult(value: JsonValue): void {
  const object = expectObject(value);
  assertExactKeys(object, ["resource"], "invalid_response");
  expectLiteral(object.resource, "devices");
}

export function expectManagementError(
  error: CommandResultFrame["error"],
): ManagementErrorCode | null {
  if (error === null) {
    return null;
  }
  return knownManagementError(error.code);
}

function parseChallenge(object: JsonObject): ChallengeFrame {
  assertExactKeys(object, CHALLENGE_KEYS, "invalid_frame");
  return {
    v: expectLiteral(object.v, 1),
    type: expectLiteral(object.type, "challenge"),
    nodeId: expectNodeId(object.nodeId),
    nodePublicKey: expectPublicKey(object.nodePublicKey),
    origin: expectString(object.origin),
    connectionId: expectId128(object.connectionId),
    challengeId: expectId128(object.challengeId),
    nonce: expectNonce(object.nonce),
    iat: expectSafeInteger(object.iat),
    exp: expectSafeInteger(object.exp),
    hostProof: expectString(object.hostProof),
  };
}

function parseDevicesChanged(object: JsonObject): DevicesChangedFrame {
  assertExactKeys(object, DEVICES_CHANGED_KEYS, "invalid_frame");
  return {
    v: expectLiteral(object.v, 1),
    type: expectLiteral(object.type, "devices.changed"),
    revision: expectSafeInteger(object.revision),
  };
}

function parseCommandResult(object: JsonObject): CommandResultFrame {
  assertExactKeys(object, RESULT_KEYS, "invalid_frame");
  if (typeof object.ok !== "boolean") {
    throw new ManagementProtocolError("invalid_frame");
  }
  let error: CommandResultFrame["error"] = null;
  if (object.error !== null) {
    const errorObject = expectObject(object.error);
    assertExactKeys(errorObject, ["code"], "invalid_frame");
    error = {
      code: knownManagementError(expectString(errorObject.code)),
    };
  }
  const result = object.result;
  if (result === undefined) {
    throw new ManagementProtocolError("invalid_frame");
  }
  if (
    (object.ok && (error !== null || result === null)) ||
    (!object.ok && (error === null || result !== null))
  ) {
    throw new ManagementProtocolError("invalid_frame");
  }
  return {
    v: expectLiteral(object.v, 1),
    type: expectLiteral(object.type, "result"),
    requestId: expectId128(object.requestId),
    ok: object.ok,
    result,
    error,
  };
}

function parseDeviceSummary(value: JsonValue): DeviceSummary {
  const object = expectObject(value);
  assertExactKeys(
    object,
    [
      "clientId",
      "createdAt",
      "displayName",
      "publicKeyFingerprint",
      "revokedAt",
      "status",
    ],
    "invalid_response",
  );
  const status = expectDeviceStatus(object.status);
  const revokedAt =
    object.revokedAt === null
      ? null
      : expectSafeInteger(object.revokedAt);
  let displayName: string;
  try {
    displayName = validateDisplayName(expectString(object.displayName));
  } catch {
    throw new ManagementProtocolError("invalid_response");
  }
  if (
    typeof object.publicKeyFingerprint !== "string" ||
    !FINGERPRINT_PATTERN.test(object.publicKeyFingerprint) ||
    (status === "trusted" && revokedAt !== null) ||
    (status === "revoked" && revokedAt === null)
  ) {
    throw new ManagementProtocolError("invalid_response");
  }
  return {
    clientId: expectId128(object.clientId),
    displayName,
    status,
    publicKeyFingerprint: object.publicKeyFingerprint,
    createdAt: expectSafeInteger(object.createdAt),
    revokedAt,
  };
}

function expectDeviceStatus(value: JsonValue | undefined): DeviceStatus {
  if (value !== "trusted" && value !== "revoked") {
    throw new ManagementProtocolError("invalid_response");
  }
  return value;
}

function expectCursor(value: JsonValue | undefined): string {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.length > 256 ||
    /\p{Cc}/u.test(value)
  ) {
    throw new ManagementProtocolError("invalid_response");
  }
  return value;
}

function validateChallengeWindow(value: {
  readonly iat: number;
  readonly exp: number;
}): void {
  if (
    value.exp <= value.iat ||
    value.exp - value.iat > MAX_CHALLENGE_SECONDS
  ) {
    throw new ManagementProtocolError("invalid_response");
  }
}

function decodePublicKey(value: string): Uint8Array<ArrayBuffer> {
  let bytes: Uint8Array;
  try {
    bytes = decodeBase64urlStrict(value);
  } catch {
    throw new ManagementProtocolError("invalid_key");
  }
  if (bytes.length !== PUBLIC_KEY_BYTES || bytes[0] !== 0x04) {
    throw new ManagementProtocolError("invalid_key");
  }
  const copy = new Uint8Array(bytes.length);
  copy.set(bytes);
  return copy;
}

function expectPublicKey(value: JsonValue | undefined): string {
  const key = expectString(value);
  decodePublicKey(key);
  return key;
}

function expectNodeId(value: JsonValue | undefined): string {
  const nodeId = expectString(value);
  if (!NODE_ID_PATTERN.test(nodeId)) {
    throw new ManagementProtocolError("invalid_response");
  }
  return nodeId;
}

function expectOrigin(
  value: JsonValue | undefined,
  expected: string,
): string {
  const origin = expectString(value);
  if (origin !== expected) {
    throw new ManagementProtocolError("origin_mismatch");
  }
  return origin;
}

function expectId128(value: JsonValue | undefined): string {
  const encoded = expectString(value);
  try {
    if (decodeBase64urlStrict(encoded).length !== ID128_BYTES) {
      throw new ManagementProtocolError("invalid_response");
    }
  } catch {
    throw new ManagementProtocolError("invalid_response");
  }
  return encoded;
}

function expectNonce(value: JsonValue | undefined): string {
  const encoded = expectString(value);
  try {
    if (decodeBase64urlStrict(encoded).length !== NONCE256_BYTES) {
      throw new ManagementProtocolError("invalid_response");
    }
  } catch {
    throw new ManagementProtocolError("invalid_response");
  }
  return encoded;
}

function expectString(value: JsonValue | undefined): string {
  if (typeof value !== "string") {
    throw new ManagementProtocolError("invalid_response");
  }
  return value;
}

function expectObject(value: JsonValue | undefined): JsonObject {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new ManagementProtocolError("invalid_response");
  }
  return value;
}

function expectSafeInteger(value: JsonValue | undefined): number {
  if (
    typeof value !== "number" ||
    !Number.isSafeInteger(value) ||
    value < 0
  ) {
    throw new ManagementProtocolError("invalid_response");
  }
  return value;
}

function expectLiteral<T extends string | number | boolean>(
  value: JsonValue | undefined,
  expected: T,
): T {
  if (value !== expected) {
    throw new ManagementProtocolError("invalid_response");
  }
  return expected;
}

function assertExactKeys(
  object: JsonObject,
  expected: readonly string[],
  code: ManagementProtocolErrorCode,
): void {
  const actual = Object.keys(object).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((key, index) => key !== wanted[index])
  ) {
    throw new ManagementProtocolError(code);
  }
}

function knownManagementError(value: string): ManagementErrorCode {
  switch (value) {
    case "invalid_request":
    case "not_found":
    case "request_conflict":
    case "stale_cursor":
    case "rate_limited":
    case "storage_unavailable":
    case "node_stopping":
      return value;
    default:
      throw new ManagementProtocolError("invalid_response");
  }
}
