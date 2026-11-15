import { PROJECTION_FIELDS, type ProjectionField, type ProjectionKind } from "../../src/views/projection";
import type { CompletedViewRecord } from "../../src/views/records";

export const STREAM = "AAECAwQFBgcICQoLDA0ODw";
export const SEGMENT = "AQEBAQEBAQEBAQEBAQEBAQ";
const encoder = new TextEncoder();
export interface WireReference {
  readonly rawRecordId: string;
  readonly byteStart: string;
  readonly byteEnd: string;
  readonly append: boolean;
}
export interface WireField {
  readonly knowledge?: string;
  readonly raw?: WireReference | null;
  readonly enumCode?: number | null;
  readonly evidenceCount?: string;
}
export function record(kind: "original_raw" | "item_projection", id: bigint, source: string,
  streamId = STREAM): CompletedViewRecord {
  const bytes = encoder.encode(source);
  return Object.freeze({ kind, streamId, recordId: id, length: BigInt(bytes.length), bytes: () => bytes.slice() });
}
export function ref(id: bigint, source: string, literal: string, append = false): WireReference {
  const offset = source.indexOf(literal);
  if (offset < 0) throw new Error("Fixture literal not found");
  return { rawRecordId: String(id), byteStart: String(encoder.encode(source.slice(0, offset)).length),
    byteEnd: String(encoder.encode(source.slice(0, offset + literal.length)).length), append };
}
export function item(anchor: bigint, first: bigint, revision: bigint, rawId: bigint,
  fields: Partial<Record<ProjectionField, WireField>> = {}, kind: ProjectionKind = "tool"): CompletedViewRecord {
  return record("item_projection", 100n + revision, JSON.stringify({
    type: "item_projection", segmentId: SEGMENT, anchorId: String(anchor), firstPosition: String(first),
    revision: String(revision), rawRecordId: String(rawId), kind,
    fields: PROJECTION_FIELDS.map((field) => ({ field, knowledge: "unknown", raw: null,
      enumCode: null, evidenceCount: "0", ...fields[field] })),
  }));
}
export function evidence(anchor: bigint, revision: bigint, index: bigint, raw: WireReference,
  field: ProjectionField = "content"): CompletedViewRecord {
  return record("item_projection", 1000n + index, JSON.stringify({
    type: "projection_evidence", segmentId: SEGMENT, anchorId: String(anchor), revision: String(revision),
    field, index: String(index), raw,
  }));
}
