export type ViewDecodeErrorCode =
  | "invalid_header"
  | "invalid_frame"
  | "invalid_stream"
  | "invalid_sequence"
  | "invalid_record"
  | "invalid_json"
  | "invalid_utf8"
  | "invalid_token"
  | "length_unrepresentable"
  | "allocation_failed"
  | "sealed";

export class ViewDecodeError extends Error {
  constructor(readonly code: ViewDecodeErrorCode) {
    super(code);
    this.name = "ViewDecodeError";
  }
}

export function copyViewBytes(bytes: Uint8Array): Uint8Array {
  try {
    return new Uint8Array(bytes);
  } catch (error: unknown) {
    throw new ViewDecodeError(
      error instanceof RangeError ? "allocation_failed" : "invalid_frame",
    );
  }
}

export function allocateViewBytes(length: number): Uint8Array {
  if (!Number.isSafeInteger(length) || length < 0) {
    throw new ViewDecodeError("length_unrepresentable");
  }
  try {
    return new Uint8Array(length);
  } catch (error: unknown) {
    throw new ViewDecodeError(
      error instanceof RangeError ? "allocation_failed" : "invalid_frame",
    );
  }
}
