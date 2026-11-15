import { copyViewBytes, ViewDecodeError } from "./errors";

export type RawJsonKind =
  | "object" | "array" | "string" | "number" | "true" | "false" | "null";
export interface RawJsonToken {
  readonly kind: RawJsonKind;
  readonly startByte: number;
  readonly endByte: number;
  readonly parent: number | null;
  readonly role: "value" | "key";
}
interface MutableToken {
  kind: RawJsonKind;
  startByte: number;
  endByte: number;
  parent: number | null;
  role: "value" | "key";
}
type Frame =
  | {
      kind: "object";
      token: number;
      state: "key_or_end" | "key" | "colon" | "value" | "separator";
    }
  | {
      kind: "array";
      token: number;
      state: "value_or_end" | "value" | "separator";
    };

function invalidJson(): never {
  throw new ViewDecodeError("invalid_json");
}
function digit(byte: number | undefined): boolean {
  return byte !== undefined && byte >= 48 && byte <= 57;
}
function hex(byte: number | undefined): boolean {
  return byte !== undefined && (
    (byte >= 48 && byte <= 57) || (byte >= 65 && byte <= 70) ||
    (byte >= 97 && byte <= 102)
  );
}
function validateUtf8(bytes: Uint8Array): void {
  try {
    const decoder = new TextDecoder("utf-8", { fatal: true });
    for (let offset = 0; offset < bytes.length; offset += 16_384) {
      decoder.decode(bytes.subarray(offset, offset + 16_384), { stream: true });
    }
    decoder.decode();
  } catch (error: unknown) {
    throw new ViewDecodeError(
      error instanceof RangeError ? "allocation_failed" : "invalid_utf8",
    );
  }
}

class Scanner {
  #at = 0;
  readonly #tokens: MutableToken[] = [];
  readonly #frames: Frame[] = [];
  constructor(readonly bytes: Uint8Array) {}

  scan(): readonly RawJsonToken[] {
    this.value(null);
    while (this.#frames.length !== 0) {
      const frame = this.#frames.at(-1);
      if (frame === undefined) invalidJson();
      this.whitespace();
      const byte = this.bytes[this.#at];
      if (frame.kind === "object") {
        switch (frame.state) {
          case "key_or_end":
            if (byte === 125) {
              this.close(frame);
              break;
            }
            this.key(frame);
            break;
          case "key":
            this.key(frame);
            break;
          case "colon":
            if (byte !== 58) invalidJson();
            ++this.#at;
            frame.state = "value";
            break;
          case "value":
            frame.state = "separator";
            this.value(frame.token);
            break;
          case "separator":
            if (byte === 125) this.close(frame);
            else if (byte === 44) {
              ++this.#at;
              frame.state = "key";
            } else invalidJson();
            break;
        }
      } else {
        switch (frame.state) {
          case "value_or_end":
            if (byte === 93) {
              this.close(frame);
              break;
            }
            frame.state = "separator";
            this.value(frame.token);
            break;
          case "value":
            frame.state = "separator";
            this.value(frame.token);
            break;
          case "separator":
            if (byte === 93) this.close(frame);
            else if (byte === 44) {
              ++this.#at;
              frame.state = "value";
            } else invalidJson();
            break;
        }
      }
    }
    this.whitespace();
    if (this.#at !== this.bytes.length) invalidJson();
    for (const token of this.#tokens) Object.freeze(token);
    return Object.freeze(this.#tokens);
  }

  private key(frame: Extract<Frame, { kind: "object" }>): void {
    if (this.bytes[this.#at] !== 34) invalidJson();
    const start = this.#at;
    this.string();
    this.token("string", start, frame.token, "key");
    frame.state = "colon";
  }
  private close(frame: Frame): void {
    const token = this.#tokens[frame.token];
    if (token === undefined) invalidJson();
    token.endByte = ++this.#at;
    this.#frames.pop();
  }
  private token(
    kind: RawJsonKind, start: number, parent: number | null,
    role: "value" | "key" = "value",
  ): number {
    const index = this.#tokens.length;
    this.#tokens.push({ kind, startByte: start, endByte: this.#at, parent, role });
    return index;
  }
  private whitespace(): void {
    while (true) {
      const byte = this.bytes[this.#at];
      if (byte !== 32 && byte !== 9 && byte !== 10 && byte !== 13) return;
      ++this.#at;
    }
  }
  private value(parent: number | null): void {
    this.whitespace();
    const start = this.#at;
    const byte = this.bytes[this.#at];
    if (byte === 123 || byte === 91) {
      ++this.#at;
      const kind = byte === 123 ? "object" : "array";
      const token = this.token(kind, start, parent);
      this.#frames.push(kind === "object"
        ? { kind, token, state: "key_or_end" }
        : { kind, token, state: "value_or_end" });
    } else if (byte === 34) {
      this.string();
      this.token("string", start, parent);
    } else if (byte === 45 || digit(byte)) {
      this.number();
      this.token("number", start, parent);
    } else {
      let kind: "true" | "false" | "null";
      if (byte === 116) kind = "true";
      else if (byte === 102) kind = "false";
      else if (byte === 110) kind = "null";
      else invalidJson();
      for (let index = 0; index < kind.length; ++index) {
        if (this.bytes[this.#at++] !== kind.charCodeAt(index)) invalidJson();
      }
      this.token(kind, start, parent);
    }
  }
  private string(): void {
    ++this.#at;
    while (this.#at < this.bytes.length) {
      const byte = this.bytes[this.#at++];
      if (byte === undefined || byte < 32) invalidJson();
      if (byte === 34) return;
      if (byte !== 92) continue;
      const escape = this.bytes[this.#at++];
      if (escape === 117) {
        for (let index = 0; index < 4; ++index) {
          if (!hex(this.bytes[this.#at++])) invalidJson();
        }
      } else if (
        escape !== 34 && escape !== 92 && escape !== 47 &&
        escape !== 98 && escape !== 102 && escape !== 110 &&
        escape !== 114 && escape !== 116
      ) invalidJson();
    }
    invalidJson();
  }
  private number(): void {
    if (this.bytes[this.#at] === 45) ++this.#at;
    if (this.bytes[this.#at] === 48) ++this.#at;
    else {
      const first = this.bytes[this.#at];
      if (first === undefined || first < 49 || first > 57) invalidJson();
      while (digit(this.bytes[this.#at])) ++this.#at;
    }
    if (this.bytes[this.#at] === 46) {
      ++this.#at;
      if (!digit(this.bytes[this.#at])) invalidJson();
      while (digit(this.bytes[this.#at])) ++this.#at;
    }
    if (this.bytes[this.#at] === 69 || this.bytes[this.#at] === 101) {
      ++this.#at;
      if (this.bytes[this.#at] === 43 || this.bytes[this.#at] === 45) ++this.#at;
      if (!digit(this.bytes[this.#at])) invalidJson();
      while (digit(this.bytes[this.#at])) ++this.#at;
    }
  }
}

export class RawJsonDocument {
  readonly #bytes: Uint8Array;
  readonly #tokens: readonly RawJsonToken[];
  private constructor(bytes: Uint8Array, tokens: readonly RawJsonToken[]) {
    this.#bytes = bytes;
    this.#tokens = tokens;
    Object.freeze(this);
  }
  static parse(bytes: Uint8Array): RawJsonDocument {
    try {
      const owned = copyViewBytes(bytes);
      validateUtf8(owned);
      return new RawJsonDocument(owned, new Scanner(owned).scan());
    } catch (error: unknown) {
      if (error instanceof ViewDecodeError) throw error;
      throw new ViewDecodeError(
        error instanceof RangeError ? "allocation_failed" : "invalid_json",
      );
    }
  }
  get tokens(): readonly RawJsonToken[] {
    return this.#tokens;
  }
  tokenBytes(index: number): Uint8Array {
    const token = this.requireToken(index);
    return copyViewBytes(this.#bytes.subarray(token.startByte, token.endByte));
  }
  tokenLexeme(index: number): string {
    try {
      return new TextDecoder("utf-8", { fatal: true }).decode(this.tokenBytes(index));
    } catch (error: unknown) {
      if (error instanceof ViewDecodeError) throw error;
      throw new ViewDecodeError(
        error instanceof RangeError ? "allocation_failed" : "invalid_utf8",
      );
    }
  }
  stringValue(index: number): string {
    if (this.requireToken(index).kind !== "string") {
      throw new ViewDecodeError("invalid_token");
    }
    try {
      const value: unknown = JSON.parse(this.tokenLexeme(index));
      if (typeof value !== "string") invalidJson();
      return value;
    } catch (error: unknown) {
      if (error instanceof ViewDecodeError) throw error;
      throw new ViewDecodeError(
        error instanceof RangeError ? "allocation_failed" : "invalid_json",
      );
    }
  }
  private requireToken(index: number): RawJsonToken {
    if (!Number.isSafeInteger(index) || index < 0) {
      throw new ViewDecodeError("invalid_token");
    }
    const token = this.#tokens[index];
    if (token === undefined) throw new ViewDecodeError("invalid_token");
    return token;
  }
}
