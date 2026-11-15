import { encodeBase64url, decodeBase64urlStrict } from "../../../src/protocol/jws";
import { PROJECTION_FIELDS, type ProjectionField } from "../../../src/views/projection";
import { VIEW_HEADER_BYTES, decodeViewFrame, type ViewRecordKind } from "../../../src/views/binary";

const encoder = new TextEncoder();
const SEGMENT = "AQEBAQEBAQEBAQEBAQEBAQ";
const BINDING = "FhYWFhYWFhYWFhYWFhYWFg";
const SOURCE = "GRkZGRkZGRkZGRkZGRkZGQ";

function buildFrame(
  streamId: string,
  packet: bigint,
  recordId: bigint,
  byteOffset: bigint,
  totalLength: bigint,
  kind: ViewRecordKind,
  last: boolean,
  payload: Uint8Array,
): Uint8Array {
  const bytes = new Uint8Array(VIEW_HEADER_BYTES + payload.length);
  bytes.set([65, 67, 80, 86, 1, 1]);
  bytes.set(decodeBase64urlStrict(streamId), 8);
  const header = new DataView(bytes.buffer);
  header.setUint16(6, last ? 1 : 0);
  header.setBigUint64(24, packet);
  header.setBigUint64(32, recordId);
  header.setBigUint64(40, byteOffset);
  header.setBigUint64(48, totalLength);
  bytes[56] = ({ original_raw: 1, item_projection: 2, status: 3, snapshot_end: 4 })[kind];
  header.setUint32(64, payload.length);
  bytes.set(payload, VIEW_HEADER_BYTES);
  const decoded = decodeViewFrame(bytes);
  if (decoded.type !== "data" || decoded.packetSequence !== packet || decoded.kind !== kind) {
    throw new Error("fixture ACPV encoder did not self-decode");
  }
  return bytes;
}

export function recordFrame(streamId: string, packet: bigint, recordId: bigint,
  kind: ViewRecordKind, payloadText = ""): Uint8Array {
  const payload = encoder.encode(payloadText);
  return buildFrame(streamId, packet, recordId, 0n, BigInt(payload.length), kind, true, payload);
}

export function chunkedRecordFrames(
  streamId: string,
  startPacket: bigint,
  recordId: bigint,
  kind: Exclude<ViewRecordKind, "snapshot_end">,
  payloadText: string,
  chunkSize: number,
): readonly Uint8Array[] {
  const payload = encoder.encode(payloadText);
  if (payload.length === 0 || chunkSize <= 0) {
    throw new Error("chunkedRecordFrames requires non-empty payload and positive chunkSize");
  }
  const frames: Uint8Array[] = [];
  let offset = 0;
  let packet = startPacket;
  const totalLength = BigInt(payload.length);
  while (offset < payload.length) {
    const nextEnd = Math.min(offset + chunkSize, payload.length);
    const slice = payload.subarray(offset, nextEnd);
    const last = nextEnd === payload.length;
    frames.push(buildFrame(streamId, packet, recordId, BigInt(offset), totalLength, kind, last, slice));
    offset = nextEnd;
    packet += 1n;
  }
  return frames;
}

export function statusRecord(streamId: string, streamByte: number, phase: "start" | "update" | "end" = "start"): string {
  const id = encodeBase64url(new Uint8Array(16).fill(streamByte));
  return JSON.stringify({
    type: "view_segment", phase, segmentId: SEGMENT, segmentViewEpoch: SEGMENT, readEpoch: id,
    origin: "retained_current", readMode: "CurrentOnly", freshness: "CurrentObservation",
    join: "ProvenNativeIds", sourceBindingId: BINDING, sourceId: SOURCE,
    beginSequence: "1", throughSequence: null, failure: null, projection: "Available",
    completionResponseSequence: null, open: true, readContext: "unique_receipt_window",
    relation: "connection_observation_only", limitation: null,
    fixtureStream: streamId,
  });
}

export function snapshotEndFrame(streamId: string, packet: bigint, recordId: bigint): Uint8Array {
  return recordFrame(streamId, packet, recordId, "snapshot_end");
}

export function timelinePair(title: string, content: string, streamId: string, revision: bigint,
  recordBase: bigint): readonly { kind: "original_raw" | "item_projection"; id: bigint; json: string }[] {
  const raw = JSON.stringify({ title, content });
  const titleLiteral = JSON.stringify(title);
  const contentLiteral = JSON.stringify(content);
  const reference = (literal: string) => {
    const index = raw.indexOf(literal);
    if (index < 0) throw new Error("projection fixture field is absent from raw record");
    return {
      rawRecordId: String(recordBase),
      byteStart: String(encoder.encode(raw.slice(0, index)).length),
      byteEnd: String(encoder.encode(raw.slice(0, index + literal.length)).length),
      append: false,
    };
  };
  const fields = PROJECTION_FIELDS.map((field: ProjectionField) => {
    const rawRef = field === "title" ? reference(titleLiteral) : field === "content" ? reference(contentLiteral) : null;
    return {
      field,
      knowledge: rawRef === null ? "unknown" : "known",
      raw: rawRef,
      enumCode: null,
      evidenceCount: "0",
    };
  });
  const projection = JSON.stringify({
    type: "item_projection", segmentId: SEGMENT, anchorId: "1", firstPosition: "1",
    revision: String(revision), rawRecordId: String(recordBase), kind: "message", fields,
  });
  if (decodeBase64urlStrict(streamId).length !== 16) throw new Error("invalid fixture stream ID");
  return [
    { kind: "original_raw", id: recordBase, json: raw },
    { kind: "item_projection", id: recordBase + 1n, json: projection },
  ];
}
