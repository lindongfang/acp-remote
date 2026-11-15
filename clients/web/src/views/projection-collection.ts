import { viewStreamBytes } from "./binary";
import { ViewDecodeError } from "./errors";
import {
  isDecodedProjectedItem, type ProjectedField, type ProjectedItem,
  type ProjectionRawReference,
} from "./projection";

function sameRaw(a: ProjectionRawReference | null, b: ProjectionRawReference | null): boolean {
  return a === null || b === null ? a === b :
    a.rawRecordId === b.rawRecordId && a.byteStart === b.byteStart &&
    a.byteEnd === b.byteEnd && a.append === b.append;
}
function sameField(a: ProjectedField, b: ProjectedField | undefined): boolean {
  return b !== undefined && a.field === b.field && a.knowledge === b.knowledge &&
    a.enumCode === b.enumCode && a.evidenceCount === b.evidenceCount && sameRaw(a.raw, b.raw);
}
function sameMetadata(a: ProjectedItem, b: ProjectedItem): boolean {
  return a.rawRecordId === b.rawRecordId && a.fields.length === b.fields.length &&
    a.fields.every((field, index) => sameField(field, b.fields[index]));
}
function position(a: ProjectedItem, b: ProjectedItem): number {
  if (a.firstPosition !== b.firstPosition) return a.firstPosition < b.firstPosition ? -1 : 1;
  return a.anchorId === b.anchorId ? 0 : a.anchorId < b.anchorId ? -1 : 1;
}
function allocationError(error: unknown): never {
  if (error instanceof ViewDecodeError) throw error;
  throw new ViewDecodeError(error instanceof RangeError ? "allocation_failed" : "invalid_record");
}

export class ProjectionCollection {
  readonly #streamId: string;
  readonly #segmentId: string;
  #anchors: Map<bigint, ProjectedItem>;
  #ordered: readonly ProjectedItem[];

  constructor(streamId: string, segmentId: string) {
    if (typeof streamId !== "string" || typeof segmentId !== "string") {
      throw new ViewDecodeError("invalid_stream");
    }
    viewStreamBytes(streamId);
    viewStreamBytes(segmentId);
    this.#streamId = streamId;
    this.#segmentId = segmentId;
    try {
      this.#anchors = new Map<bigint, ProjectedItem>();
      this.#ordered = Object.freeze([]);
    } catch (error: unknown) { allocationError(error); }
  }

  accept(item: ProjectedItem): boolean {
    if (!isDecodedProjectedItem(item)) throw new ViewDecodeError("invalid_record");
    if (item.streamId !== this.#streamId || item.segmentId !== this.#segmentId) {
      throw new ViewDecodeError("invalid_stream");
    }
    const previous = this.#anchors.get(item.anchorId);
    if (previous !== undefined) {
      if (previous.firstPosition !== item.firstPosition || previous.kind !== item.kind) {
        throw new ViewDecodeError("invalid_record");
      }
      if (item.revision < previous.revision) return false;
      if (item.revision === previous.revision) {
        if (!sameMetadata(item, previous)) throw new ViewDecodeError("invalid_record");
        return false;
      }
    }
    try {
      const anchors = new Map(this.#anchors);
      anchors.set(item.anchorId, item);
      const ordered = Object.freeze(previous === undefined
        ? [...this.#ordered, item].sort(position)
        : this.#ordered.map((value) => value.anchorId === item.anchorId ? item : value));
      this.#anchors = anchors;
      this.#ordered = ordered;
      return true;
    } catch (error: unknown) { return allocationError(error); }
  }

  items(): readonly ProjectedItem[] {
    try { return Object.freeze([...this.#ordered]); }
    catch (error: unknown) { return allocationError(error); }
  }
}
