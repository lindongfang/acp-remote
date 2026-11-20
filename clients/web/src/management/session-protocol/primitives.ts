import {
  decodeBase64urlStrict,
  encodeBase64url,
  type JsonObject,
  type JsonValue,
} from "../../protocol/jws";

const ENCODER = new TextEncoder();
export const MAX_SOURCE_TEXT_BYTES = 4096;
const U64_MAX = "18446744073709551615";
const I64_MAX = "9223372036854775807";
const I64_MIN_MAGNITUDE = "9223372036854775808";

export type SessionProtocolErrorCode =
  | "invalid_command"
  | "invalid_id"
  | "invalid_decimal"
  | "invalid_frame"
  | "invalid_direction"
  | "binary_before_auth"
  | "allocation_failed";

export class SessionProtocolError extends Error {
  readonly code: SessionProtocolErrorCode;

  constructor(code: SessionProtocolErrorCode) {
    super(code);
    this.name = "SessionProtocolError";
    this.code = code;
  }
}

/** A syntactically valid platform ID grants no authority or resource ownership. */
export function requireSessionId(value: unknown): string {
  if (typeof value !== "string" || value.length !== 22) {
    throw new SessionProtocolError("invalid_id");
  }
  try {
    const bytes = decodeBase64urlStrict(value);
    if (bytes.length !== 16 || encodeBase64url(bytes) !== value) {
      throw new SessionProtocolError("invalid_id");
    }
    return value;
  } catch (error) {
    if (error instanceof RangeError) {
      throw new SessionProtocolError("allocation_failed");
    }
    throw new SessionProtocolError("invalid_id");
  }
}

function magnitudeWithin(value: string, maximum: string): boolean {
  return (
    value.length < maximum.length ||
    (value.length === maximum.length && value <= maximum)
  );
}

/** Keep the original canonical decimal string, including values above 2^53. */
export function requireSessionU64(value: unknown): string {
  if (
    typeof value !== "string" ||
    value.length > U64_MAX.length ||
    !/^(?:0|[1-9][0-9]*)$/u.test(value) ||
    !magnitudeWithin(value, U64_MAX)
  ) {
    throw new SessionProtocolError("invalid_decimal");
  }
  return value;
}

export function requireSessionI64(value: unknown): string {
  if (typeof value !== "string" || value.length > 20) {
    throw new SessionProtocolError("invalid_decimal");
  }
  if (value.startsWith("-")) {
    const magnitude = value.slice(1);
    if (
      /^[1-9][0-9]*$/u.test(magnitude) &&
      magnitudeWithin(magnitude, I64_MIN_MAGNITUDE)
    ) {
      return value;
    }
  } else if (
    /^(?:0|[1-9][0-9]*)$/u.test(value) &&
    magnitudeWithin(value, I64_MAX)
  ) {
    return value;
  }
  throw new SessionProtocolError("invalid_decimal");
}

export function responseResult<T>(parse: () => T): T {
  try {
    return parse();
  } catch (error) {
    if (
      error instanceof SessionProtocolError &&
      error.code === "allocation_failed"
    ) {
      throw error;
    }
    throw new SessionProtocolError("invalid_frame");
  }
}

export type Validator = (value: JsonValue | undefined) => unknown;
export type Shape = Readonly<Record<string, Validator>>;
export type ShapeValue<S extends Shape> = {
  readonly [K in keyof S]: ReturnType<S[K]>;
};

export function object(value: JsonValue | undefined): JsonObject {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new SessionProtocolError("invalid_command");
  }
  return value;
}

export function closed<S extends Shape>(
  value: JsonValue | undefined,
  shape: S,
): ShapeValue<S> {
  const source = object(value);
  const actual = Object.keys(source);
  const keys = Object.keys(shape);
  if (
    actual.length !== keys.length ||
    keys.some((key) => !Object.hasOwn(source, key))
  ) {
    throw new SessionProtocolError("invalid_command");
  }
  // Every returned key is constructed by its validator; never return unvalidated input.
  return Object.freeze(
    Object.fromEntries(
      keys.map((key) => [key, shape[key]!(source[key])]),
    ),
  ) as ShapeValue<S>;
}

export function closedWithOptional<S extends Shape, O extends Shape>(
  value: JsonValue | undefined,
  requiredShape: S,
  optionalShape: O,
): ShapeValue<S> & { readonly [K in keyof O]?: ReturnType<O[K]> } {
  const source = object(value);
  const requiredKeys = Object.keys(requiredShape);
  const optionalKeys = Object.keys(optionalShape);
  const actual = Object.keys(source);
  if (
    requiredKeys.some((key) => !Object.hasOwn(source, key)) ||
    actual.some(
      (key) =>
        !Object.hasOwn(requiredShape, key) && !Object.hasOwn(optionalShape, key),
    )
  ) {
    throw new SessionProtocolError("invalid_command");
  }
  const validated: Record<string, unknown> = {};
  for (const key of requiredKeys) {
    validated[key] = requiredShape[key]!(source[key]);
  }
  for (const key of optionalKeys) {
    if (Object.hasOwn(source, key)) {
      validated[key] = optionalShape[key]!(source[key]);
    }
  }
  return Object.freeze(validated) as ShapeValue<S> & {
    readonly [K in keyof O]?: ReturnType<O[K]>;
  };
}

export function oneOf<const T extends readonly string[]>(...values: T) {
  return (value: JsonValue | undefined): T[number] => {
    if (typeof value !== "string" || !values.includes(value)) {
      throw new SessionProtocolError("invalid_command");
    }
    return value;
  };
}

export function literal<const T extends string | number | boolean>(
  expected: T,
): (value: JsonValue | undefined) => T {
  return (value) => {
    if (value !== expected) {
      throw new SessionProtocolError("invalid_frame");
    }
    return expected;
  };
}

export function requireText(value: JsonValue | undefined): string {
  if (typeof value !== "string") {
    throw new SessionProtocolError("invalid_frame");
  }
  return value;
}

export function requireTrue(value: JsonValue | undefined): true {
  if (value !== true) {
    throw new SessionProtocolError("invalid_frame");
  }
  return true;
}

export function requireBoolean(value: JsonValue | undefined): boolean {
  if (typeof value !== "boolean") {
    throw new SessionProtocolError("invalid_frame");
  }
  return value;
}

export function requireNull(value: JsonValue | undefined): null {
  if (value !== null) {
    throw new SessionProtocolError("invalid_frame");
  }
  return null;
}

export function requireNullable<T>(
  parse: (value: JsonValue | undefined) => T,
): (value: JsonValue | undefined) => T | null {
  return (value) => (value === null ? null : parse(value));
}

export function requireArray<T>(
  parse: (value: JsonValue | undefined, index: number) => T,
): (value: JsonValue | undefined) => readonly T[] {
  return (value) => {
    if (!Array.isArray(value) || value.length > 3) {
      throw new SessionProtocolError("invalid_frame");
    }
    return Object.freeze(value.map(parse));
  };
}

export function approvalQuantity<T>(
  value: JsonValue | undefined,
  parse: (entry: JsonValue | undefined) => T,
): readonly T[] {
  if (!Array.isArray(value)) {
    throw new SessionProtocolError("invalid_frame");
  }
  return Object.freeze(value.map(parse));
}

export function requireBoundedSourceText(
  rejectNul: boolean,
): (value: JsonValue | undefined) => string {
  return (value: JsonValue | undefined): string => {
    if (typeof value !== "string" || (rejectNul && value.includes("\0"))) {
      throw new SessionProtocolError("invalid_command");
    }
    const bytes = ENCODER.encode(value);
    if (bytes.length === 0 || bytes.length > MAX_SOURCE_TEXT_BYTES) {
      throw new SessionProtocolError("invalid_command");
    }
    return value;
  };
}
