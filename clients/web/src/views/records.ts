import {
  VIEW_U64_MAX,
  type ViewDataFrame,
  type ViewRecordKind,
  validateViewData,
  viewStreamBytes,
} from "./binary";
import { allocateViewBytes, copyViewBytes, ViewDecodeError } from "./errors";

export interface CompletedViewRecord {
  readonly streamId: string;
  readonly recordId: bigint;
  readonly kind: ViewRecordKind;
  readonly length: bigint;
  bytes(): Uint8Array;
}

class OwnedViewRecord implements CompletedViewRecord {
  readonly #chunks: readonly Uint8Array[];
  constructor(
    readonly streamId: string,
    readonly recordId: bigint,
    readonly kind: ViewRecordKind,
    readonly length: bigint,
    chunks: readonly Uint8Array[],
  ) {
    this.#chunks = chunks;
    Object.freeze(this);
  }
  bytes(): Uint8Array {
    if (this.length > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new ViewDecodeError("length_unrepresentable");
    }
    const output = allocateViewBytes(Number(this.length));
    let offset = 0;
    for (const chunk of this.#chunks) {
      output.set(chunk, offset);
      offset += chunk.length;
    }
    if (BigInt(offset) !== this.length) {
      throw new ViewDecodeError("invalid_record");
    }
    return output;
  }
}

interface PartialRecord {
  readonly recordId: bigint;
  readonly kind: ViewRecordKind;
  readonly total: bigint;
  offset: bigint;
  readonly chunks: Uint8Array[];
}

export class ViewRecordAssembler {
  readonly #streamId: string;
  #nextPacket = 1n;
  #partial: PartialRecord | null = null;
  #sealed = false;

  constructor(streamId: string) {
    viewStreamBytes(streamId);
    this.#streamId = streamId;
  }

  accept(frame: ViewDataFrame): CompletedViewRecord | null {
    if (this.#sealed) throw new ViewDecodeError("sealed");
    try {
      validateViewData(frame);
      if (frame.streamId !== this.#streamId) {
        throw new ViewDecodeError("invalid_stream");
      }
      if (frame.packetSequence !== this.#nextPacket) {
        throw new ViewDecodeError("invalid_sequence");
      }
      const previous = this.#partial;
      if (
        previous === null
          ? frame.byteOffset !== 0n
          : previous.recordId !== frame.recordId ||
            previous.kind !== frame.kind ||
            previous.total !== frame.totalLength ||
            previous.offset !== frame.byteOffset
      ) {
        throw new ViewDecodeError("invalid_record");
      }
      const chunk = copyViewBytes(frame.payload);
      const partial: PartialRecord = previous ?? {
        recordId: frame.recordId,
        kind: frame.kind,
        total: frame.totalLength,
        offset: 0n,
        chunks: [],
      };
      try {
        partial.chunks.push(chunk);
      } catch (error: unknown) {
        if (error instanceof RangeError) {
          throw new ViewDecodeError("allocation_failed");
        }
        throw error;
      }
      partial.offset += BigInt(chunk.length);
      const complete = frame.last
        ? new OwnedViewRecord(
            this.#streamId, partial.recordId, partial.kind,
            partial.total, partial.chunks,
          )
        : null;
      this.#partial = complete === null ? partial : null;
      ++this.#nextPacket;
      return complete;
    } catch (error: unknown) {
      this.seal();
      if (error instanceof ViewDecodeError) throw error;
      throw new ViewDecodeError("invalid_record");
    }
  }

  seal(): void {
    this.#sealed = true;
    this.#partial = null;
  }
}

const TYPED_ARRAY = Object.getPrototypeOf(Uint8Array.prototype) as object;

interface CallerRecord {
  readonly stream: unknown;
  readonly recordId: unknown;
  readonly length: unknown;
  readonly kind: unknown;
  readonly read: unknown;
}
function callerRecord(record: CompletedViewRecord): CallerRecord {
  try {
    return {
      stream: record.streamId, recordId: record.recordId, length: record.length,
      kind: record.kind, read: record.bytes,
    };
  } catch { throw new ViewDecodeError("invalid_record"); }
}
function typedArrayProperty(name: string | symbol, value: object): unknown {
  const getter = Object.getOwnPropertyDescriptor(TYPED_ARRAY, name)?.get;
  if (getter === undefined) throw new ViewDecodeError("invalid_record");
  return Reflect.apply(getter, value, []);
}
function callerBytes(record: CompletedViewRecord, header: CallerRecord): Uint8Array {
  let buffer: ArrayBufferLike;
  let offset: number;
  let length: number;
  try {
    if (typeof header.read !== "function") throw new Error();
    const value: unknown = Reflect.apply(header.read, record, []);
    if (!ArrayBuffer.isView(value) || typedArrayProperty(Symbol.toStringTag, value) !== "Uint8Array") {
      throw new Error();
    }
    buffer = typedArrayProperty("buffer", value) as ArrayBufferLike;
    offset = typedArrayProperty("byteOffset", value) as number;
    length = typedArrayProperty("byteLength", value) as number;
    if (BigInt(length) !== header.length) throw new Error();
  } catch { throw new ViewDecodeError("invalid_record"); }
  try {
    const view = new Uint8Array(buffer, offset, length);
    const owned = new Uint8Array(length);
    owned.set(view);
    return owned;
  } catch (error: unknown) {
    throw new ViewDecodeError(error instanceof RangeError ? "allocation_failed" : "invalid_record");
  }
}

export interface ViewRecordBody {
  readonly streamId: string;
  readonly recordId: bigint;
  readonly length: bigint;
  readonly bytes: Uint8Array;
}

/** Validates the caller boundary and returns bytes owned by this invocation. */
export function readViewRecordBody(
  record: CompletedViewRecord, expectedKind: "original_raw" | "item_projection",
): ViewRecordBody {
  const header = callerRecord(record);
  if ((expectedKind !== "original_raw" && expectedKind !== "item_projection") ||
      header.kind !== expectedKind ||
      typeof header.recordId !== "bigint" || header.recordId < 1n || header.recordId > VIEW_U64_MAX ||
      typeof header.length !== "bigint" || header.length < 1n || header.length > VIEW_U64_MAX) {
    throw new ViewDecodeError("invalid_record");
  }
  if (typeof header.stream !== "string") throw new ViewDecodeError("invalid_stream");
  viewStreamBytes(header.stream);
  const bytes = callerBytes(record, header);
  try {
    return Object.freeze({
      streamId: header.stream, recordId: header.recordId, length: header.length, bytes,
    });
  } catch (error: unknown) {
    throw new ViewDecodeError(error instanceof RangeError ? "allocation_failed" : "invalid_record");
  }
}
