import type { ViewDataFrame } from "../views/binary";
import { ViewDecodeError } from "../views/errors";
import {
  buildRawIndex,
  visibleRawText,
  type RawIndex,
  type RawMember,
} from "../views/raw-inspector-data";
import { RawJsonDocument } from "../views/raw-json";
import {
  ViewRecordAssembler,
  type CompletedViewRecord,
} from "../views/records";
import type { SourceSessionFact } from "./session-protocol";

export const MAX_SOURCE_RESULT_DISPLAY_BYTES = 2 * 1024 * 1024;
const MAX_SOURCE_JSON_DEPTH = 32;
const INTEGER_LEXEME = /^-?(?:0|[1-9][0-9]*)$/u;
const UTF8 = new TextDecoder("utf-8", { fatal: true });

export type SourceSessionResultReadErrorCode =
  | "result_too_large"
  | "invalid_stream"
  | "invalid_result_json"
  | "invalid_result_shape";

export class SourceSessionResultReadError extends Error {
  readonly code: SourceSessionResultReadErrorCode;

  constructor(code: SourceSessionResultReadErrorCode, message?: string) {
    super(message ?? code);
    this.name = "SourceSessionResultReadError";
    this.code = code;
  }
}

export interface DiscoveredNativeSession {
  readonly sessionId: string;
  readonly title: string | null;
  readonly cwd: string | null;
  readonly updatedAt: string | null;
}

export type ParsedSourceSessionResult =
  | {
      readonly kind: "sessions";
      readonly sessions: readonly DiscoveredNativeSession[];
      readonly nextCursor: string | null;
      readonly rawDocument: RawJsonDocument;
      readonly safeRawText: string;
    }
  | {
      readonly kind: "created";
      readonly sessionId: string;
      readonly rawDocument: RawJsonDocument;
      readonly safeRawText: string;
    }
  | {
      readonly kind: "remote_error";
      readonly errorCode: string;
      readonly message: string;
      readonly rawDocument: RawJsonDocument;
      readonly safeRawText: string;
    };

export type InspectedSourceSessionResult =
  | ParsedSourceSessionResult
  | {
      readonly kind: "fallback";
      readonly code: SourceSessionResultReadErrorCode;
      readonly reason: string;
      readonly rawDocument: RawJsonDocument | null;
      readonly safeRawText: string;
    };

export type SourceSessionResultReadUpdate =
  | { readonly state: "pending" }
  | { readonly state: "complete"; readonly rawBytes: Uint8Array };

/**
 * Reassembles the single bare ACP JSON `original_raw` + `snapshot_end`
 * `native_session_result` stream without unwrapping JSON-RPC or losing
 * arbitrary-precision numeric lexemes.
 */
export class SourceSessionResultReader {
  readonly #expectedTotal: bigint;
  readonly #assembler: ViewRecordAssembler;
  #result: CompletedViewRecord | null = null;
  #sealed = false;

  constructor(streamId: string, expectedTotal: bigint) {
    if (
      typeof expectedTotal !== "bigint" ||
      expectedTotal < 0n ||
      expectedTotal > BigInt(MAX_SOURCE_RESULT_DISPLAY_BYTES) ||
      expectedTotal > BigInt(Number.MAX_SAFE_INTEGER)
    ) {
      throw new SourceSessionResultReadError("result_too_large");
    }
    try {
      this.#assembler = new ViewRecordAssembler(streamId);
    } catch {
      throw new SourceSessionResultReadError("invalid_stream");
    }
    this.#expectedTotal = expectedTotal;
  }

  accept(frame: ViewDataFrame): SourceSessionResultReadUpdate {
    if (this.#sealed) {
      throw new SourceSessionResultReadError("invalid_stream");
    }
    try {
      const completed = this.#assembler.accept(frame);
      if (completed === null) return { state: "pending" };
      if (completed.kind === "original_raw") {
        if (this.#result !== null) {
          throw new SourceSessionResultReadError("invalid_stream");
        }
        if (completed.length > BigInt(MAX_SOURCE_RESULT_DISPLAY_BYTES)) {
          throw new SourceSessionResultReadError("result_too_large");
        }
        this.#result = completed;
        return { state: "pending" };
      }
      if (completed.kind !== "snapshot_end") {
        throw new SourceSessionResultReadError("invalid_stream");
      }
      const result = this.#result;
      if (result === null || result.length !== this.#expectedTotal) {
        throw new SourceSessionResultReadError("invalid_stream");
      }
      const rawBytes = result.bytes();
      this.#sealed = true;
      this.#assembler.seal();
      return Object.freeze({ state: "complete", rawBytes });
    } catch (error) {
      this.#sealed = true;
      this.#assembler.seal();
      if (error instanceof SourceSessionResultReadError) throw error;
      if (error instanceof ViewDecodeError) {
        throw new SourceSessionResultReadError("invalid_stream");
      }
      throw new SourceSessionResultReadError("invalid_stream");
    }
  }

  seal(): void {
    if (this.#sealed) return;
    this.#sealed = true;
    this.#assembler.seal();
    this.#result = null;
  }
}

export function sanitizeSourceDisplayText(value: string): string {
  return visibleRawText(value);
}

export function decodeSafeRawFallbackText(bytes: Uint8Array): string {
  try {
    return visibleRawText(UTF8.decode(bytes));
  } catch {
    return "[无法按 UTF-8 解码原始结果字节]";
  }
}

export function parseSourceSessionResultPayload(
  bytes: Uint8Array,
  fact: SourceSessionFact | null,
): ParsedSourceSessionResult {
  if (bytes.byteLength === 0) {
    throw new SourceSessionResultReadError("invalid_result_json", "empty payload");
  }
  if (bytes.byteLength > MAX_SOURCE_RESULT_DISPLAY_BYTES) {
    throw new SourceSessionResultReadError("result_too_large");
  }
  let rawDocument: RawJsonDocument;
  let index: RawIndex;
  try {
    rawDocument = RawJsonDocument.parse(bytes);
    index = buildRawIndex(rawDocument.tokens);
  } catch {
    throw new SourceSessionResultReadError("invalid_result_json");
  }
  validateRawJsonTree(rawDocument, index, 0, 0);

  const rootToken = rawDocument.tokens[0];
  if (rootToken === undefined || rootToken.kind !== "object") {
    throw new SourceSessionResultReadError("invalid_result_shape", "root must be object");
  }
  const fields = objectFieldMap(rawDocument, index, 0);
  if (fields.has("jsonrpc")) {
    throw new SourceSessionResultReadError(
      "invalid_result_shape",
      "outer jsonrpc wrapper is forbidden",
    );
  }
  const safeRawText = decodeSafeRawFallbackText(bytes);
  const response = fact?.response ?? null;
  if (response === null) {
    throw new SourceSessionResultReadError("invalid_result_shape", "missing response fact");
  }

  switch (response.kind) {
    case "sessions": {
      const sessionsToken = fields.get("sessions");
      if (
        sessionsToken === undefined ||
        rawDocument.tokens[sessionsToken]?.kind !== "array"
      ) {
        throw new SourceSessionResultReadError("invalid_result_shape", "missing sessions array");
      }
      const memberTokens = index.children[sessionsToken] ?? [];
      const sessions: DiscoveredNativeSession[] = memberTokens.map((member) => {
        const entryToken = member.value;
        if (rawDocument.tokens[entryToken]?.kind !== "object") {
          throw new SourceSessionResultReadError("invalid_result_shape", "session entry must be object");
        }
        const entryFields = objectFieldMap(rawDocument, index, entryToken);
        const sessionId = requireNonEmptyStringField(
          rawDocument,
          entryFields.get("sessionId"),
          "sessionId",
        );
        return Object.freeze({
          sessionId,
          title: optionalSanitizedStringField(rawDocument, entryFields.get("title")),
          cwd: optionalSanitizedStringField(rawDocument, entryFields.get("cwd")),
          updatedAt: optionalSanitizedStringField(rawDocument, entryFields.get("updatedAt")),
        });
      });
      const nextCursorToken = fields.get("nextCursor");
      let nextCursor: string | null = null;
      if (nextCursorToken !== undefined) {
        const kind = rawDocument.tokens[nextCursorToken]?.kind;
        if (kind === "null") {
          nextCursor = null;
        } else if (kind === "string") {
          nextCursor = rawDocument.stringValue(nextCursorToken);
        } else {
          throw new SourceSessionResultReadError("invalid_result_shape", "invalid nextCursor");
        }
      }
      return Object.freeze({
        kind: "sessions",
        sessions: Object.freeze(sessions),
        nextCursor,
        rawDocument,
        safeRawText,
      });
    }
    case "created": {
      const sessionId = requireNonEmptyStringField(
        rawDocument,
        fields.get("sessionId"),
        "sessionId",
      );
      if (
        response.nativeSessionId !== null &&
        response.nativeSessionId !== sessionId
      ) {
        throw new SourceSessionResultReadError("invalid_result_shape", "sessionId mismatch");
      }
      return Object.freeze({
        kind: "created",
        sessionId,
        rawDocument,
        safeRawText,
      });
    }
    case "remote_error": {
      const codeToken = fields.get("code");
      if (codeToken === undefined || rawDocument.tokens[codeToken]?.kind !== "number") {
        throw new SourceSessionResultReadError("invalid_result_shape", "missing error code");
      }
      const errorCode = rawDocument.tokenLexeme(codeToken);
      if (!INTEGER_LEXEME.test(errorCode)) {
        throw new SourceSessionResultReadError("invalid_result_shape", "non-integer error code");
      }
      if (
        response.remoteErrorCode !== null &&
        response.remoteErrorCode !== errorCode
      ) {
        throw new SourceSessionResultReadError("invalid_result_shape", "remote error code mismatch");
      }
      const messageToken = fields.get("message");
      if (messageToken === undefined || rawDocument.tokens[messageToken]?.kind !== "string") {
        throw new SourceSessionResultReadError("invalid_result_shape", "missing error message");
      }
      const message = sanitizeSourceDisplayText(rawDocument.stringValue(messageToken));
      return Object.freeze({
        kind: "remote_error",
        errorCode,
        message,
        rawDocument,
        safeRawText,
      });
    }
  }
}

export function inspectSourceSessionResultPayload(
  bytes: Uint8Array,
  fact: SourceSessionFact | null,
): InspectedSourceSessionResult {
  try {
    return parseSourceSessionResultPayload(bytes, fact);
  } catch (error) {
    const code =
      error instanceof SourceSessionResultReadError
        ? error.code
        : "invalid_result_json";
    let rawDocument: RawJsonDocument | null = null;
    if (
      bytes.byteLength > 0 &&
      bytes.byteLength <= MAX_SOURCE_RESULT_DISPLAY_BYTES
    ) {
      try {
        rawDocument = RawJsonDocument.parse(bytes);
      } catch {
        rawDocument = null;
      }
    }
    const reason =
      code === "result_too_large"
        ? "原始结果体积超出安全展示上限。"
        : code === "invalid_result_json"
          ? "原始结果不是合法的 UTF-8 / RFC 8259 JSON 对象。"
          : "原始结果结构与操作状态事实不匹配，已按只读安全文本回退展示。";
    return Object.freeze({
      kind: "fallback",
      code,
      reason,
      rawDocument,
      safeRawText: decodeSafeRawFallbackText(bytes),
    });
  }
}

function validateRawJsonTree(
  document: RawJsonDocument,
  index: RawIndex,
  tokenIndex: number,
  depth: number,
): void {
  if (depth > MAX_SOURCE_JSON_DEPTH) {
    throw new SourceSessionResultReadError("invalid_result_json", "json depth exceeded");
  }
  const token = document.tokens[tokenIndex];
  if (token === undefined) {
    throw new SourceSessionResultReadError("invalid_result_json");
  }
  if (token.kind === "object") {
    const members = index.children[tokenIndex] ?? [];
    const seen = new Set<string>();
    for (const member of members) {
      if (member.key === null) {
        throw new SourceSessionResultReadError("invalid_result_json");
      }
      const key = document.stringValue(member.key);
      if (seen.has(key)) {
        throw new SourceSessionResultReadError("invalid_result_json", "duplicate field");
      }
      seen.add(key);
      validateRawJsonTree(document, index, member.value, depth + 1);
    }
  } else if (token.kind === "array") {
    const members = index.children[tokenIndex] ?? [];
    for (const member of members) {
      validateRawJsonTree(document, index, member.value, depth + 1);
    }
  }
}

function objectFieldMap(
  document: RawJsonDocument,
  index: RawIndex,
  objectTokenIndex: number,
): Map<string, number> {
  const members: readonly RawMember[] = index.children[objectTokenIndex] ?? [];
  const map = new Map<string, number>();
  for (const member of members) {
    if (member.key !== null) {
      map.set(document.stringValue(member.key), member.value);
    }
  }
  return map;
}

function requireNonEmptyStringField(
  document: RawJsonDocument,
  tokenIndex: number | undefined,
  field: string,
): string {
  if (tokenIndex === undefined || document.tokens[tokenIndex]?.kind !== "string") {
    throw new SourceSessionResultReadError("invalid_result_shape", `missing ${field}`);
  }
  const value = document.stringValue(tokenIndex);
  if (value.length === 0) {
    throw new SourceSessionResultReadError("invalid_result_shape", `empty ${field}`);
  }
  return value;
}

function optionalSanitizedStringField(
  document: RawJsonDocument,
  tokenIndex: number | undefined,
): string | null {
  if (tokenIndex === undefined) return null;
  const token = document.tokens[tokenIndex];
  if (token === undefined || token.kind === "null") return null;
  if (token.kind !== "string") {
    throw new SourceSessionResultReadError("invalid_result_shape", "expected string field");
  }
  return sanitizeSourceDisplayText(document.stringValue(tokenIndex));
}
