import { VIEW_U64_MAX, viewStreamBytes } from "./binary";
import { ProjectionCollection } from "./projection-collection";
import {
  decodeProjectionRecord, isDecodedProjectedItem, PROJECTION_FIELDS,
  type ProjectedItem, type ProjectionEvidence, type ProjectionField,
  type ProjectionKnowledge, type ProjectionRawReference,
} from "./projection";
import { RawJsonDocument } from "./raw-json";
import { readViewRecordBody, type CompletedViewRecord } from "./records";
import { ViewDecodeError } from "./errors";

export interface RawValue {
  readonly document: RawJsonDocument;
  readonly token: number;
}
export interface ReferencedValue {
  readonly raw: ProjectionRawReference;
  readonly value: RawValue | null;
  readonly fieldSlice?: boolean;
}
export interface FieldMaterial {
  readonly knowledge: ProjectionKnowledge;
  readonly enumCode: number | null;
  readonly primary: ReferencedValue | null;
  readonly expectedEvidenceCount: bigint;
  readonly evidence: readonly (ReferencedValue & { readonly index: bigint })[];
  readonly referencesReady: boolean;
}
interface RawEntry {
  readonly bytes: Uint8Array;
  readonly document: RawJsonDocument;
}

function sameReference(a: ProjectionRawReference, b: ProjectionRawReference): boolean {
  return a.rawRecordId === b.rawRecordId && a.byteStart === b.byteStart &&
    a.byteEnd === b.byteEnd && a.append === b.append;
}
function evidenceKey(segment: string, anchor: bigint, revision: bigint, field: ProjectionField): string {
  return `${segment}:${anchor}:${revision}:${field}`;
}
function storeError(error: unknown): never {
  if (error instanceof ViewDecodeError) throw error;
  throw new ViewDecodeError(error instanceof RangeError ? "allocation_failed" : "invalid_record");
}
function requireIdentifier(value: string): void {
  if (typeof value !== "string") throw new ViewDecodeError("invalid_stream");
  viewStreamBytes(value);
}

/** Owned, nonpersistent display data. This store has no authority or native control port. */
export class ViewContentStore {
  readonly #streamId: string;
  #raw: Map<bigint, RawEntry>;
  readonly #ranges: Map<string, RawEntry>;
  #segments: Map<string, ProjectionCollection>;
  #evidence: Map<string, ReadonlyMap<bigint, ProjectionRawReference>>;
  #closed = false;

  constructor(streamId: string) {
    requireIdentifier(streamId);
    this.#streamId = streamId;
    try {
      this.#raw = new Map();
      this.#ranges = new Map();
      this.#segments = new Map();
      this.#evidence = new Map();
    } catch (error: unknown) { storeError(error); }
  }

  private requireOpen(): void {
    if (this.#closed) throw new ViewDecodeError("sealed");
  }
  private requireStream(streamId: string): void {
    if (streamId !== this.#streamId) throw new ViewDecodeError("invalid_stream");
  }

  accept(record: CompletedViewRecord): boolean {
    this.requireOpen();
    let kind: unknown;
    try { kind = record.kind; } catch { throw new ViewDecodeError("invalid_record"); }
    try {
      if (kind === "field_bytes") {
        const body = readViewRecordBody(record, "field_bytes");
        this.requireStream(body.streamId);
        if (body.bytes.length <= 8) throw new ViewDecodeError("invalid_record");
        const start = new DataView(body.bytes.buffer, body.bytes.byteOffset).getBigUint64(0);
        const bytes = body.bytes.subarray(8);
        const end = start + BigInt(bytes.length);
        if (end > VIEW_U64_MAX) throw new ViewDecodeError("invalid_record");
        const key = `${body.recordId}:${start}:${end}`;
        const previous = this.#ranges.get(key);
        if (previous) {
          if (!bytes.every((b, i) => b === previous.bytes[i])) throw new ViewDecodeError("invalid_record");
          return false;
        }
        const document = RawJsonDocument.parse(bytes);
        const root = document.tokens[0];
        if (root?.startByte !== 0 || root.endByte !== bytes.length) throw new ViewDecodeError("invalid_record");
        this.#ranges.set(key, { bytes, document });
        return true;
      }
      if (kind === "original_raw") {
        const body = readViewRecordBody(record, "original_raw");
        this.requireStream(body.streamId);
        const previous = this.#raw.get(body.recordId);
        if (previous !== undefined) {
          if (previous.bytes.length !== body.bytes.length ||
              !previous.bytes.every((byte, index) => byte === body.bytes[index])) {
            throw new ViewDecodeError("invalid_record");
          }
          return false;
        }
        const entry = Object.freeze({ bytes: body.bytes, document: RawJsonDocument.parse(body.bytes) });
        const raw = new Map(this.#raw);
        raw.set(body.recordId, entry);
        this.#raw = raw;
        return true;
      }
      if (kind !== "item_projection") throw new ViewDecodeError("invalid_record");
      const value = decodeProjectionRecord(record);
      this.requireStream(value.streamId);
      if (value.type === "projection_evidence") return this.acceptEvidence(value);
      const previous = this.#segments.get(value.segmentId);
      if (previous !== undefined) return previous.accept(value);
      const collection = new ProjectionCollection(this.#streamId, value.segmentId);
      const changed = collection.accept(value);
      const segments = new Map(this.#segments);
      segments.set(value.segmentId, collection);
      this.#segments = segments;
      return changed;
    } catch (error: unknown) { return storeError(error); }
  }

  private acceptEvidence(value: ProjectionEvidence): boolean {
    const key = evidenceKey(value.segmentId, value.anchorId, value.revision, value.field);
    const existing = this.#evidence.get(key);
    const previous = existing?.get(value.index);
    if (previous !== undefined) {
      if (!sameReference(previous, value.raw)) throw new ViewDecodeError("invalid_record");
      return false;
    }
    const group = new Map(existing);
    group.set(value.index, value.raw);
    const evidence = new Map(this.#evidence);
    evidence.set(key, group);
    this.#evidence = evidence;
    return true;
  }

  items(segmentId: string): readonly ProjectedItem[] {
    this.requireOpen();
    requireIdentifier(segmentId);
    try { return this.#segments.get(segmentId)?.items() ?? Object.freeze([]); }
    catch (error: unknown) { return storeError(error); }
  }

  rawDocument(item: ProjectedItem): RawJsonDocument | null {
    this.requireOpen();
    if (!isDecodedProjectedItem(item)) throw new ViewDecodeError("invalid_record");
    this.requireStream(item.streamId);
    return this.#raw.get(item.rawRecordId)?.document ?? null;
  }

  private reference(raw: ProjectionRawReference): ReferencedValue {
    const entry = this.#raw.get(raw.rawRecordId);
    if (entry === undefined) {
      const range = this.#ranges.get(`${raw.rawRecordId}:${raw.byteStart}:${raw.byteEnd}`);
      return Object.freeze({ raw, fieldSlice: true, value: range ? Object.freeze({ document: range.document, token: 0 }) : null });
    }
    if (raw.byteStart > BigInt(entry.bytes.length) || raw.byteEnd > BigInt(entry.bytes.length)) {
      throw new ViewDecodeError("invalid_record");
    }
    const token = entry.document.tokens.findIndex((value) => value.role === "value" &&
      BigInt(value.startByte) === raw.byteStart && BigInt(value.endByte) === raw.byteEnd);
    if (token < 0) throw new ViewDecodeError("invalid_record");
    return Object.freeze({ raw, value: Object.freeze({ document: entry.document, token }) });
  }

  field(item: ProjectedItem, fieldName: ProjectionField): FieldMaterial {
    this.requireOpen();
    if (!isDecodedProjectedItem(item)) throw new ViewDecodeError("invalid_record");
    this.requireStream(item.streamId);
    if (typeof fieldName !== "string" || !PROJECTION_FIELDS.some((value) => value === fieldName)) {
      throw new ViewDecodeError("invalid_record");
    }
    try {
      const field = item.fields.find((value) => value.field === fieldName);
      if (field === undefined) throw new ViewDecodeError("invalid_record");
      const group = this.#evidence.get(evidenceKey(item.segmentId, item.anchorId, item.revision, fieldName));
      const ordered = [...(group?.entries() ?? [])].sort(([a], [b]) => a === b ? 0 : a < b ? -1 : 1);
      if (ordered.some(([index]) => index >= field.evidenceCount)) throw new ViewDecodeError("invalid_record");
      const primary = field.raw === null ? null : this.reference(field.raw);
      if (fieldName === "content" && field.evidenceCount > 0n && field.raw !== null) {
        const last = group?.get(field.evidenceCount - 1n);
        if (last !== undefined && !sameReference(field.raw, last)) throw new ViewDecodeError("invalid_record");
      }
      const evidence = Object.freeze(ordered.map(([index, raw]) =>
        Object.freeze({ index, ...this.reference(raw) })));
      const referencesReady = (primary === null || primary.value !== null) &&
        BigInt(evidence.length) === field.evidenceCount &&
        evidence.every((value, index) => value.index === BigInt(index) && value.value !== null);
      return Object.freeze({
        knowledge: field.knowledge, enumCode: field.enumCode, primary,
        expectedEvidenceCount: field.evidenceCount, evidence, referencesReady,
      });
    } catch (error: unknown) { return storeError(error); }
  }

  dispose(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#raw.clear();
    this.#ranges.clear();
    this.#segments.clear();
    this.#evidence.clear();
  }
}
