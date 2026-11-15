import { parseStrictJsonObject, type JsonObject, type JsonValue } from "../protocol/jws";
import { VIEW_U64_MAX, viewStreamBytes } from "./binary";
import { ViewDecodeError } from "./errors";
import { readViewRecordBody, type CompletedViewRecord } from "./records";

export const PROJECTION_FIELDS = Object.freeze([
  "content", "metadata", "name", "title", "kind", "status", "locations",
  "rawInput", "rawOutput",
] as const);
export type ProjectionField = (typeof PROJECTION_FIELDS)[number];
export type ProjectionKnowledge =
  | "unknown" | "known" | "knownAbsent" | "knownEmpty" | "partial"
  | "uninterpreted" | "deferred";
export type ProjectionKind = "tool" | "message" | "state" | "unknownStructured";
export interface ProjectionRawReference {
  readonly rawRecordId: bigint;
  readonly byteStart: bigint;
  readonly byteEnd: bigint;
  readonly append: boolean;
}
export interface ProjectedField {
  readonly field: ProjectionField;
  readonly knowledge: ProjectionKnowledge;
  readonly raw: ProjectionRawReference | null;
  readonly enumCode: number | null;
  readonly evidenceCount: bigint;
}
export interface ProjectedItem {
  readonly type: "item_projection";
  readonly streamId: string;
  readonly segmentId: string;
  readonly anchorId: bigint;
  readonly firstPosition: bigint;
  readonly revision: bigint;
  readonly rawRecordId: bigint;
  readonly kind: ProjectionKind;
  readonly fields: readonly ProjectedField[];
}
export interface ProjectionEvidence {
  readonly type: "projection_evidence";
  readonly streamId: string;
  readonly segmentId: string;
  readonly anchorId: bigint;
  readonly revision: bigint;
  readonly field: ProjectionField;
  readonly index: bigint;
  readonly raw: ProjectionRawReference;
}
export type ProjectionRecord = ProjectedItem | ProjectionEvidence;

const DECODED_ITEMS = new WeakSet<object>();
const UTF8 = new TextDecoder("utf-8", { fatal: true });
const KNOWLEDGE: readonly string[] = [
  "unknown", "known", "knownAbsent", "knownEmpty", "partial", "uninterpreted", "deferred",
];
const KINDS: readonly string[] = ["tool", "message", "state", "unknownStructured"];

function invalidJson(): never {
  throw new ViewDecodeError("invalid_json");
}
function object(value: JsonValue | undefined, keys: readonly string[]): JsonObject {
  if (value === null || typeof value !== "object" || Array.isArray(value)) invalidJson();
  const actual = Object.keys(value);
  if (actual.length !== keys.length || actual.some((key) => !keys.includes(key))) invalidJson();
  return value;
}
function integer(value: JsonValue | undefined, nonzero = false): bigint {
  if (typeof value !== "string" || !/^(0|[1-9][0-9]*)$/.test(value) || value.length > 20) {
    invalidJson();
  }
  const parsed = BigInt(value);
  if (parsed > VIEW_U64_MAX || (nonzero && parsed === 0n)) invalidJson();
  return parsed;
}
function segment(value: JsonValue | undefined): string {
  if (typeof value !== "string") invalidJson();
  try { viewStreamBytes(value); } catch { invalidJson(); }
  return value;
}
function fieldName(value: JsonValue | undefined): ProjectionField {
  if (typeof value !== "string" || !PROJECTION_FIELDS.some((field) => field === value)) {
    invalidJson();
  }
  return value as ProjectionField;
}
function rawReference(value: JsonValue | undefined): ProjectionRawReference {
  const input = object(value, ["rawRecordId", "byteStart", "byteEnd", "append"]);
  const rawRecordId = integer(input.rawRecordId, true);
  const byteStart = integer(input.byteStart);
  const byteEnd = integer(input.byteEnd);
  if (byteStart > byteEnd || typeof input.append !== "boolean") invalidJson();
  return Object.freeze({ rawRecordId, byteStart, byteEnd, append: input.append });
}
function projectedField(value: JsonValue, index: number): ProjectedField {
  const input = object(value, ["field", "knowledge", "raw", "enumCode", "evidenceCount"]);
  const field = fieldName(input.field);
  if (field !== PROJECTION_FIELDS[index]) invalidJson();
  if (typeof input.knowledge !== "string" || !KNOWLEDGE.includes(input.knowledge)) invalidJson();
  const enumCode = input.enumCode;
  if (enumCode !== null && (
    typeof enumCode !== "number" || !Number.isInteger(enumCode) || enumCode < 0 || enumCode > 255
  )) invalidJson();
  return Object.freeze({
    field, knowledge: input.knowledge as ProjectionKnowledge,
    raw: input.raw === null ? null : rawReference(input.raw),
    enumCode, evidenceCount: integer(input.evidenceCount),
  });
}
function metadata(input: JsonObject, streamId: string): ProjectionRecord {
  if (input.type === "item_projection") {
    object(input, ["segmentId", "type", "anchorId", "firstPosition", "revision", "rawRecordId", "kind", "fields"]);
    if (typeof input.kind !== "string" || !KINDS.includes(input.kind)) invalidJson();
    if (!Array.isArray(input.fields) || input.fields.length !== PROJECTION_FIELDS.length) invalidJson();
    const item: ProjectedItem = Object.freeze({
      type: "item_projection", streamId, segmentId: segment(input.segmentId),
      anchorId: integer(input.anchorId), firstPosition: integer(input.firstPosition),
      revision: integer(input.revision), rawRecordId: integer(input.rawRecordId, true),
      kind: input.kind as ProjectionKind,
      fields: Object.freeze(input.fields.map(projectedField)),
    });
    DECODED_ITEMS.add(item);
    return item;
  }
  if (input.type === "projection_evidence") {
    object(input, ["segmentId", "type", "anchorId", "revision", "field", "index", "raw"]);
    return Object.freeze({
      type: "projection_evidence", streamId, segmentId: segment(input.segmentId),
      anchorId: integer(input.anchorId), revision: integer(input.revision),
      field: fieldName(input.field), index: integer(input.index), raw: rawReference(input.raw),
    });
  }
  return invalidJson();
}

/** Decodes platform metadata only. Native values remain exact raw-byte references. */
export function decodeProjectionRecord(record: CompletedViewRecord): ProjectionRecord {
  const { streamId, bytes } = readViewRecordBody(record, "item_projection");
  try { UTF8.decode(bytes); } catch (error: unknown) {
    throw new ViewDecodeError(error instanceof RangeError ? "allocation_failed" : "invalid_utf8");
  }
  try {
    return metadata(parseStrictJsonObject(bytes), streamId);
  } catch (error: unknown) {
    if (error instanceof ViewDecodeError) throw error;
    throw new ViewDecodeError(error instanceof RangeError ? "allocation_failed" : "invalid_json");
  }
}

/** Local validation marker; it conveys no session permission or ACP capability. */
export function isDecodedProjectedItem(value: unknown): value is ProjectedItem {
  return typeof value === "object" && value !== null && DECODED_ITEMS.has(value);
}
