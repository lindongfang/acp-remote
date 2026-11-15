import { decodeBase64urlStrict, encodeBase64url } from "../protocol/jws";

export const INPUT_HEADER_BYTES = 32;
export const INPUT_CHUNK_BYTES = 16_384;
const U64_MAX = 0xffff_ffff_ffff_ffffn;

type InputTransferErrorCode =
  | "invalid_header"
  | "invalid_direction"
  | "invalid_length"
  | "invalid_upload_id"
  | "invalid_offset"
  | "allocation_failed";

export class InputTransferError extends Error {
  readonly code: InputTransferErrorCode;

  constructor(code: InputTransferErrorCode) {
    super(code);
    this.name = "InputTransferError";
    this.code = code;
  }
}

export interface InputAcknowledgement {
  readonly uploadId: string;
  readonly nextOffset: bigint;
}

function uploadBytes(uploadId: string): Uint8Array {
  if (typeof uploadId !== "string" || uploadId.length !== 22) {
    throw new InputTransferError("invalid_upload_id");
  }
  try {
    const bytes = decodeBase64urlStrict(uploadId);
    if (bytes.length !== 16 || encodeBase64url(bytes) !== uploadId) {
      throw new InputTransferError("invalid_upload_id");
    }
    return bytes;
  } catch (error) {
    if (error instanceof RangeError) {
      throw new InputTransferError("allocation_failed");
    }
    throw new InputTransferError("invalid_upload_id");
  }
}

/** Encodes one opaque upload chunk; this frame conveys no admission or authority. */
export function encodeInputData(
  uploadId: string,
  offset: bigint,
  payload: Uint8Array,
): Uint8Array {
  const id = uploadBytes(uploadId);
  if (typeof offset !== "bigint" || offset < 0n || offset > U64_MAX) {
    throw new InputTransferError("invalid_offset");
  }
  if (
    !(payload instanceof Uint8Array) ||
    payload.length === 0 ||
    payload.length > INPUT_CHUNK_BYTES
  ) {
    throw new InputTransferError("invalid_length");
  }
  try {
    const wire = new Uint8Array(INPUT_HEADER_BYTES + payload.length);
    wire.set([65, 67, 80, 73, 1, 1, 0, 0]);
    wire.set(id, 8);
    new DataView(wire.buffer, wire.byteOffset, wire.byteLength)
      .setBigUint64(24, offset, false);
    wire.set(payload, INPUT_HEADER_BYTES);
    return wire;
  } catch {
    throw new InputTransferError("allocation_failed");
  }
}

/** Reads only an upload-buffer acknowledgement, with no queue or delivery side effect. */
export function decodeInputAcknowledgement(wire: Uint8Array): InputAcknowledgement {
  if (!(wire instanceof Uint8Array) || wire.length < INPUT_HEADER_BYTES) {
    throw new InputTransferError("invalid_header");
  }
  if (
    wire[0] !== 65 || wire[1] !== 67 || wire[2] !== 80 || wire[3] !== 73 ||
    wire[4] !== 1 || wire[6] !== 0 || wire[7] !== 0 ||
    (wire[5] !== 1 && wire[5] !== 2)
  ) {
    throw new InputTransferError("invalid_header");
  }
  if (wire[5] !== 2) {
    throw new InputTransferError("invalid_direction");
  }
  if (wire.length !== INPUT_HEADER_BYTES) {
    throw new InputTransferError("invalid_length");
  }
  try {
    return Object.freeze({
      uploadId: encodeBase64url(wire.subarray(8, 24)),
      nextOffset: new DataView(wire.buffer, wire.byteOffset, wire.byteLength)
        .getBigUint64(24, false),
    });
  } catch {
    throw new InputTransferError("allocation_failed");
  }
}
