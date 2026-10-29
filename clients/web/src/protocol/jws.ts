export const MAX_COMPACT_JWS_BYTES = 16 * 1024;
export const MAX_PROTOCOL_JSON_DEPTH = 8;

export const JWS_TYPES = [
  "acpr-pair-claim+jwt",
  "acpr-pair-status+jwt",
  "acpr-host-claim+jwt",
  "acpr-host-status+jwt",
  "acpr-auth-client+jwt",
  "acpr-auth-host+jwt",
] as const;

export type JwsType = (typeof JWS_TYPES)[number];

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export type JsonObject = Readonly<Record<string, JsonValue>>;

export type JwsErrorCode =
  | "too_large"
  | "invalid_compact"
  | "invalid_base64"
  | "invalid_json"
  | "duplicate_field"
  | "non_object_payload"
  | "invalid_header"
  | "unsupported_algorithm"
  | "unsupported_type"
  | "invalid_public_key"
  | "invalid_signature_encoding"
  | "invalid_signature"
  | "crypto_unavailable"
  | "invalid_key";

export class JwsError extends Error {
  readonly code: JwsErrorCode;

  constructor(code: JwsErrorCode) {
    super(code);
    this.name = "JwsError";
    this.code = code;
  }
}

export interface ParsedJws {
  readonly typ: JwsType;
  readonly payload: JsonObject;
  readonly signingInput: Uint8Array;
  readonly signature: Uint8Array;
}

const UTF8_ENCODER = new TextEncoder();
const UTF8_DECODER = new TextDecoder("utf-8", { fatal: true });
const BASE64URL_PATTERN = /^[A-Za-z0-9_-]+$/;
const P256_ORDER =
  0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551n;

export function parseCompactJws(
  compact: string,
  acceptedTypes: readonly JwsType[],
): ParsedJws {
  if (UTF8_ENCODER.encode(compact).length > MAX_COMPACT_JWS_BYTES) {
    throw new JwsError("too_large");
  }

  const segments = compact.split(".");
  if (segments.length !== 3) {
    throw new JwsError("invalid_compact");
  }
  const [headerSegment, payloadSegment, signatureSegment] = segments;
  if (
    headerSegment === undefined ||
    payloadSegment === undefined ||
    signatureSegment === undefined ||
    headerSegment.length === 0 ||
    payloadSegment.length === 0 ||
    signatureSegment.length === 0
  ) {
    throw new JwsError("invalid_compact");
  }

  const headerBytes = decodeBase64url(headerSegment);
  const payloadBytes = decodeBase64url(payloadSegment);
  const signatureBytes = decodeBase64url(signatureSegment);
  if (signatureBytes.length !== 64) {
    throw new JwsError("invalid_signature_encoding");
  }
  validateP1363Scalars(signatureBytes);

  const header = parseStrictJson(headerBytes);
  if (!isJsonObject(header)) {
    throw new JwsError("invalid_header");
  }
  const typ = parseHeader(header);
  if (!acceptedTypes.includes(typ)) {
    throw new JwsError("unsupported_type");
  }

  const payload = parseStrictJson(payloadBytes);
  if (!isJsonObject(payload)) {
    throw new JwsError("non_object_payload");
  }

  return {
    typ,
    payload,
    signingInput: UTF8_ENCODER.encode(`${headerSegment}.${payloadSegment}`),
    signature: signatureBytes,
  };
}

export async function verifyCompactJws(
  compact: string,
  publicKeySec1: Uint8Array,
  acceptedTypes: readonly JwsType[],
): Promise<ParsedJws> {
  const parsed = parseCompactJws(compact, acceptedTypes);
  if (publicKeySec1.length !== 65 || publicKeySec1[0] !== 0x04) {
    throw new JwsError("invalid_public_key");
  }
  const subtle = getSubtleCrypto();
  const publicKeyBytes = new Uint8Array(publicKeySec1);
  const signature = new Uint8Array(parsed.signature);
  const signingInput = new Uint8Array(parsed.signingInput);
  let key: CryptoKey;
  try {
    key = await subtle.importKey(
      "raw",
      publicKeyBytes,
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["verify"],
    );
  } catch {
    throw new JwsError("invalid_public_key");
  }
  const valid = await subtle.verify(
    { name: "ECDSA", hash: "SHA-256" },
    key,
    signature,
    signingInput,
  );
  if (!valid) {
    throw new JwsError("invalid_signature");
  }
  return parsed;
}

export async function signCompactJws(
  typ: JwsType,
  payloadBytes: Uint8Array,
  signingKey: CryptoKey,
): Promise<string> {
  if (payloadBytes.length > MAX_COMPACT_JWS_BYTES) {
    throw new JwsError("too_large");
  }
  const payload = parseStrictJson(payloadBytes);
  if (!isJsonObject(payload)) {
    throw new JwsError("non_object_payload");
  }
  if (
    signingKey.type !== "private" ||
    signingKey.algorithm.name !== "ECDSA" ||
    !("namedCurve" in signingKey.algorithm) ||
    signingKey.algorithm.namedCurve !== "P-256"
  ) {
    throw new JwsError("invalid_key");
  }

  const headerSegment = bytesToBase64url(
    UTF8_ENCODER.encode(`{"alg":"ES256","typ":"${typ}"}`),
  );
  const payloadSegment = bytesToBase64url(payloadBytes);
  const signingInput = UTF8_ENCODER.encode(
    `${headerSegment}.${payloadSegment}`,
  );
  const signature = new Uint8Array(
    await getSubtleCrypto().sign(
      { name: "ECDSA", hash: "SHA-256" },
      signingKey,
      signingInput,
    ),
  );
  if (signature.length !== 64) {
    throw new JwsError("invalid_signature_encoding");
  }
  validateP1363Scalars(signature);
  const compact = `${headerSegment}.${payloadSegment}.${bytesToBase64url(signature)}`;
  if (UTF8_ENCODER.encode(compact).length > MAX_COMPACT_JWS_BYTES) {
    throw new JwsError("too_large");
  }
  return compact;
}

export function encodeBase64url(bytes: Uint8Array): string {
  return bytesToBase64url(bytes);
}

export function decodeBase64urlStrict(value: string): Uint8Array {
  return decodeBase64url(value);
}

export function parseStrictJsonObject(
  bytes: Uint8Array,
  maxDepth = MAX_PROTOCOL_JSON_DEPTH,
): JsonObject {
  const value = parseStrictJson(bytes, maxDepth);
  if (!isJsonObject(value)) {
    throw new JwsError("non_object_payload");
  }
  return value;
}

function parseHeader(header: JsonObject): JwsType {
  const keys = Object.keys(header).sort();
  if (keys.length !== 2 || keys[0] !== "alg" || keys[1] !== "typ") {
    throw new JwsError("invalid_header");
  }
  const algorithm = header.alg;
  if (algorithm !== "ES256") {
    throw new JwsError("unsupported_algorithm");
  }
  const typ = header.typ;
  if (typeof typ !== "string" || !isJwsType(typ)) {
    throw new JwsError("unsupported_type");
  }
  return typ;
}

function isJwsType(value: string): value is JwsType {
  return (JWS_TYPES as readonly string[]).includes(value);
}

function bytesToBase64url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary)
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/u, "");
}

function decodeBase64url(value: string): Uint8Array {
  if (!BASE64URL_PATTERN.test(value) || value.length % 4 === 1) {
    throw new JwsError("invalid_base64");
  }
  const base64 = value.replaceAll("-", "+").replaceAll("_", "/");
  const padded = base64.padEnd(Math.ceil(base64.length / 4) * 4, "=");
  let binary: string;
  try {
    binary = atob(padded);
  } catch {
    throw new JwsError("invalid_base64");
  }
  const decoded = Uint8Array.from(binary, (character) =>
    character.charCodeAt(0),
  );
  if (bytesToBase64url(decoded) !== value) {
    throw new JwsError("invalid_base64");
  }
  return decoded;
}

function validateP1363Scalars(signature: Uint8Array): void {
  const r = bytesToBigInt(signature.subarray(0, 32));
  const s = bytesToBigInt(signature.subarray(32, 64));
  if (r === 0n || s === 0n || r >= P256_ORDER || s >= P256_ORDER) {
    throw new JwsError("invalid_signature_encoding");
  }
}

function bytesToBigInt(bytes: Uint8Array): bigint {
  let value = 0n;
  for (const byte of bytes) {
    value = (value << 8n) | BigInt(byte);
  }
  return value;
}

function getSubtleCrypto(): SubtleCrypto {
  const subtle = globalThis.crypto?.subtle;
  if (subtle === undefined) {
    throw new JwsError("crypto_unavailable");
  }
  return subtle;
}

function isJsonObject(value: JsonValue): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseStrictJson(
  bytes: Uint8Array,
  maxDepth = MAX_PROTOCOL_JSON_DEPTH,
): JsonValue {
  let source: string;
  try {
    source = UTF8_DECODER.decode(bytes);
  } catch {
    throw new JwsError("invalid_json");
  }
  if (!Number.isSafeInteger(maxDepth) || maxDepth < 1) {
    throw new JwsError("invalid_json");
  }
  return new StrictJsonParser(source, maxDepth).parse();
}

class StrictJsonParser {
  readonly #source: string;
  readonly #maxDepth: number;
  #position = 0;

  constructor(source: string, maxDepth: number) {
    this.#source = source;
    this.#maxDepth = maxDepth;
  }

  parse(): JsonValue {
    this.#skipWhitespace();
    const value = this.#parseValue(0);
    this.#skipWhitespace();
    if (this.#position !== this.#source.length) {
      throw new JwsError("invalid_json");
    }
    return value;
  }

  #parseValue(depth: number): JsonValue {
    const character = this.#source[this.#position];
    switch (character) {
      case "{":
        this.#checkDepth(depth);
        return this.#parseObject(depth + 1);
      case "[":
        this.#checkDepth(depth);
        return this.#parseArray(depth + 1);
      case '"':
        return this.#parseString();
      case "t":
        this.#consumeLiteral("true");
        return true;
      case "f":
        this.#consumeLiteral("false");
        return false;
      case "n":
        this.#consumeLiteral("null");
        return null;
      default:
        if (character === "-" || isDigit(character)) {
          return this.#parseNumber();
        }
        throw new JwsError("invalid_json");
    }
  }

  #parseObject(depth: number): JsonObject {
    this.#position += 1;
    const result = Object.create(null) as Record<string, JsonValue>;
    this.#skipWhitespace();
    if (this.#source[this.#position] === "}") {
      this.#position += 1;
      return result;
    }
    for (;;) {
      this.#skipWhitespace();
      if (this.#source[this.#position] !== '"') {
        throw new JwsError("invalid_json");
      }
      const key = this.#parseString();
      if (Object.hasOwn(result, key)) {
        throw new JwsError("duplicate_field");
      }
      this.#skipWhitespace();
      if (this.#source[this.#position] !== ":") {
        throw new JwsError("invalid_json");
      }
      this.#position += 1;
      this.#skipWhitespace();
      Object.defineProperty(result, key, {
        value: this.#parseValue(depth),
        enumerable: true,
        configurable: true,
        writable: true,
      });
      this.#skipWhitespace();
      const separator = this.#source[this.#position];
      if (separator === "}") {
        this.#position += 1;
        return result;
      }
      if (separator !== ",") {
        throw new JwsError("invalid_json");
      }
      this.#position += 1;
    }
  }

  #parseArray(depth: number): JsonValue[] {
    this.#position += 1;
    const result: JsonValue[] = [];
    this.#skipWhitespace();
    if (this.#source[this.#position] === "]") {
      this.#position += 1;
      return result;
    }
    for (;;) {
      this.#skipWhitespace();
      result.push(this.#parseValue(depth));
      this.#skipWhitespace();
      const separator = this.#source[this.#position];
      if (separator === "]") {
        this.#position += 1;
        return result;
      }
      if (separator !== ",") {
        throw new JwsError("invalid_json");
      }
      this.#position += 1;
    }
  }

  #parseString(): string {
    if (this.#source[this.#position] !== '"') {
      throw new JwsError("invalid_json");
    }
    this.#position += 1;
    let result = "";
    for (;;) {
      const character = this.#source[this.#position];
      if (character === undefined) {
        throw new JwsError("invalid_json");
      }
      this.#position += 1;
      if (character === '"') {
        return result;
      }
      if (character === "\\") {
        result += this.#parseEscape();
        continue;
      }
      const code = character.charCodeAt(0);
      if (code < 0x20) {
        throw new JwsError("invalid_json");
      }
      if (isHighSurrogate(code)) {
        const low = this.#source.charCodeAt(this.#position);
        if (!isLowSurrogate(low)) {
          throw new JwsError("invalid_json");
        }
        result += character + this.#source[this.#position];
        this.#position += 1;
        continue;
      }
      if (isLowSurrogate(code)) {
        throw new JwsError("invalid_json");
      }
      result += character;
    }
  }

  #parseEscape(): string {
    const escape = this.#source[this.#position];
    this.#position += 1;
    switch (escape) {
      case '"':
      case "\\":
      case "/":
        return escape;
      case "b":
        return "\b";
      case "f":
        return "\f";
      case "n":
        return "\n";
      case "r":
        return "\r";
      case "t":
        return "\t";
      case "u":
        return this.#parseUnicodeEscape();
      default:
        throw new JwsError("invalid_json");
    }
  }

  #parseUnicodeEscape(): string {
    const high = this.#readHexCodeUnit();
    if (isLowSurrogate(high)) {
      throw new JwsError("invalid_json");
    }
    if (!isHighSurrogate(high)) {
      return String.fromCharCode(high);
    }
    if (
      this.#source[this.#position] !== "\\" ||
      this.#source[this.#position + 1] !== "u"
    ) {
      throw new JwsError("invalid_json");
    }
    this.#position += 2;
    const low = this.#readHexCodeUnit();
    if (!isLowSurrogate(low)) {
      throw new JwsError("invalid_json");
    }
    return String.fromCharCode(high, low);
  }

  #readHexCodeUnit(): number {
    const digits = this.#source.slice(this.#position, this.#position + 4);
    if (digits.length !== 4 || !/^[0-9A-Fa-f]{4}$/u.test(digits)) {
      throw new JwsError("invalid_json");
    }
    this.#position += 4;
    return Number.parseInt(digits, 16);
  }

  #parseNumber(): number {
    const start = this.#position;
    if (this.#source[this.#position] === "-") {
      this.#position += 1;
    }
    const first = this.#source[this.#position];
    if (first === "0") {
      this.#position += 1;
      if (isDigit(this.#source[this.#position])) {
        throw new JwsError("invalid_json");
      }
    } else if (first !== undefined && /[1-9]/u.test(first)) {
      this.#position += 1;
      while (isDigit(this.#source[this.#position])) {
        this.#position += 1;
      }
    } else {
      throw new JwsError("invalid_json");
    }

    if (this.#source[this.#position] === ".") {
      this.#position += 1;
      if (!isDigit(this.#source[this.#position])) {
        throw new JwsError("invalid_json");
      }
      while (isDigit(this.#source[this.#position])) {
        this.#position += 1;
      }
    }

    if (
      this.#source[this.#position] === "e" ||
      this.#source[this.#position] === "E"
    ) {
      this.#position += 1;
      if (
        this.#source[this.#position] === "+" ||
        this.#source[this.#position] === "-"
      ) {
        this.#position += 1;
      }
      if (!isDigit(this.#source[this.#position])) {
        throw new JwsError("invalid_json");
      }
      while (isDigit(this.#source[this.#position])) {
        this.#position += 1;
      }
    }

    const raw = this.#source.slice(start, this.#position);
    if (/[.eE]/u.test(raw)) {
      throw new JwsError("invalid_json");
    }
    const value = Number(raw);
    if (!Number.isSafeInteger(value)) {
      throw new JwsError("invalid_json");
    }
    return value;
  }

  #consumeLiteral(literal: string): void {
    if (this.#source.slice(this.#position, this.#position + literal.length) !== literal) {
      throw new JwsError("invalid_json");
    }
    this.#position += literal.length;
  }

  #skipWhitespace(): void {
    while (true) {
      const character = this.#source[this.#position];
      if (
        character !== " " &&
        character !== "\t" &&
        character !== "\n" &&
        character !== "\r"
      ) {
        return;
      }
      this.#position += 1;
    }
  }

  #checkDepth(depth: number): void {
    if (depth >= this.#maxDepth) {
      throw new JwsError("invalid_json");
    }
  }
}

function isDigit(value: string | undefined): boolean {
  return value !== undefined && value >= "0" && value <= "9";
}

function isHighSurrogate(value: number): boolean {
  return value >= 0xd800 && value <= 0xdbff;
}

function isLowSurrogate(value: number): boolean {
  return value >= 0xdc00 && value <= 0xdfff;
}
