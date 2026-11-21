import { decodeBase64urlStrict, encodeBase64url } from "../protocol/jws";
import { allocateViewBytes, copyViewBytes, ViewDecodeError } from "./errors";

export const VIEW_HEADER_BYTES = 72;
export const VIEW_CHUNK_BYTES = 16_384;
export const VIEW_U64_MAX = 0xffff_ffff_ffff_ffffn;

export type ViewRecordKind =
  | "original_raw"
  | "item_projection"
  | "status"
  | "snapshot_end";

export type PagedViewRecordKind = ViewRecordKind | "turn_start" | "field_bytes";

export interface ViewDataFrame {
  readonly type: "data";
  readonly streamId: string;
  readonly packetSequence: bigint;
  readonly recordId: bigint;
  readonly byteOffset: bigint;
  readonly totalLength: bigint;
  readonly kind: PagedViewRecordKind;
  readonly last: boolean;
  readonly payload: Uint8Array;
}
export interface ViewCreditFrame {
  readonly type: "credit";
  readonly streamId: string;
  readonly packetSequence: bigint;
}
export interface ViewDetachFrame {
  readonly type: "detach";
  readonly streamId: string;
  readonly packetSequence: 0n;
}
export type ViewBinaryFrame = ViewDataFrame | ViewCreditFrame | ViewDetachFrame;

export function viewStreamBytes(streamId: string): Uint8Array {
  try {
    if (streamId.length !== 22) throw new ViewDecodeError("invalid_stream");
    const bytes = decodeBase64urlStrict(streamId);
    if (bytes.length !== 16 || encodeBase64url(bytes) !== streamId) {
      throw new ViewDecodeError("invalid_stream");
    }
    return bytes;
  } catch {
    throw new ViewDecodeError("invalid_stream");
  }
}

export function requireViewU64(value: bigint): void {
  if (typeof value !== "bigint" || value < 0n || value > VIEW_U64_MAX) {
    throw new ViewDecodeError("invalid_frame");
  }
}

export function validateViewData(frame: ViewDataFrame): void {
  if (
    frame.type !== "data" ||
    !(frame.payload instanceof Uint8Array) ||
    typeof frame.last !== "boolean"
  ) {
    throw new ViewDecodeError("invalid_frame");
  }
  viewStreamBytes(frame.streamId);
  for (const value of [
    frame.packetSequence,
    frame.recordId,
    frame.byteOffset,
    frame.totalLength,
  ]) {
    requireViewU64(value);
  }
  if (frame.packetSequence === 0n || frame.recordId === 0n) {
    throw new ViewDecodeError("invalid_frame");
  }
  if (frame.payload.length > VIEW_CHUNK_BYTES) {
    throw new ViewDecodeError("invalid_frame");
  }
  const end = frame.byteOffset + BigInt(frame.payload.length);
  if (
    end > VIEW_U64_MAX ||
    end > frame.totalLength ||
    frame.last !== (end === frame.totalLength)
  ) {
    throw new ViewDecodeError("invalid_frame");
  }
  if (frame.kind === "field_bytes" && frame.totalLength <= 8n) throw new ViewDecodeError("invalid_frame");
  switch (frame.kind) {
    case "snapshot_end":
      if (
        frame.totalLength !== 0n ||
        frame.byteOffset !== 0n ||
        frame.payload.length !== 0
      ) {
        throw new ViewDecodeError("invalid_frame");
      }
      break;
    case "original_raw":
    case "item_projection":
    case "status":
    case "field_bytes":
    case "turn_start":
      if (frame.totalLength === 0n || frame.payload.length === 0) {
        throw new ViewDecodeError("invalid_frame");
      }
      break;
    default:
      throw new ViewDecodeError("invalid_frame");
  }
}

export function decodeViewFrame(wire: Uint8Array): ViewBinaryFrame {
  if (wire.length < VIEW_HEADER_BYTES) {
    throw new ViewDecodeError("invalid_header");
  }
  if (
    wire[0] !== 65 || wire[1] !== 67 || wire[2] !== 80 ||
    wire[3] !== 86 || wire[4] !== 1
  ) {
    throw new ViewDecodeError("invalid_header");
  }
  const header = new DataView(wire.buffer, wire.byteOffset, wire.byteLength);
  const flags = header.getUint16(6);
  if (flags > 1) throw new ViewDecodeError("invalid_header");
  for (let i = 57; i < 64; ++i) {
    if (wire[i] !== 0) throw new ViewDecodeError("invalid_header");
  }
  if (header.getUint32(68) !== 0) {
    throw new ViewDecodeError("invalid_header");
  }
  const length = header.getUint32(64);
  if (length > VIEW_CHUNK_BYTES || wire.length !== VIEW_HEADER_BYTES + length) {
    throw new ViewDecodeError("invalid_frame");
  }
  const streamId = encodeBase64url(wire.subarray(8, 24));
  const packetSequence = header.getBigUint64(24);
  const recordId = header.getBigUint64(32);
  const byteOffset = header.getBigUint64(40);
  const totalLength = header.getBigUint64(48);
  const recordKind = wire[56];
  if (wire[5] === 2 || wire[5] === 3) {
    if (
      flags !== 0 || recordId !== 0n || byteOffset !== 0n ||
      totalLength !== 0n || recordKind !== 0 || length !== 0
    ) {
      throw new ViewDecodeError("invalid_header");
    }
    if (wire[5] === 3) {
      if (packetSequence !== 0n) throw new ViewDecodeError("invalid_header");
      return Object.freeze({ type: "detach", streamId, packetSequence: 0n });
    }
    return Object.freeze({ type: "credit", streamId, packetSequence });
  }
  if (wire[5] !== 1) throw new ViewDecodeError("invalid_header");
  let kind: PagedViewRecordKind;
  switch (recordKind) {
    case 1: kind = "original_raw"; break;
    case 2: kind = "item_projection"; break;
    case 3: kind = "status"; break;
    case 6: kind = "field_bytes"; break;
    case 5: kind = "turn_start"; break;
    case 4: kind = "snapshot_end"; break;
    default: throw new ViewDecodeError("invalid_header");
  }
  const frame: ViewDataFrame = Object.freeze({
    type: "data",
    streamId,
    packetSequence,
    recordId,
    byteOffset,
    totalLength,
    kind,
    last: flags === 1,
    payload: copyViewBytes(wire.subarray(VIEW_HEADER_BYTES)),
  });
  validateViewData(frame);
  return frame;
}

function encodeControl(streamId: string, packet: bigint, type: 2 | 3): Uint8Array {
  const stream = viewStreamBytes(streamId);
  requireViewU64(packet);
  const wire = allocateViewBytes(VIEW_HEADER_BYTES);
  wire.set([65, 67, 80, 86, 1, type]);
  wire.set(stream, 8);
  new DataView(wire.buffer, wire.byteOffset, wire.byteLength)
    .setBigUint64(24, packet);
  return wire;
}

export function encodeViewCredit(streamId: string, packetSequence: bigint): Uint8Array {
  return encodeControl(streamId, packetSequence, 2);
}

export function encodeViewDetach(streamId: string): Uint8Array {
  return encodeControl(streamId, 0n, 3);
}
