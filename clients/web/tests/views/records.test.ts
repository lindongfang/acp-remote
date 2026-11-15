import { describe, expect, it } from "vitest";
import { encodeBase64url } from "../../src/protocol/jws";
import {
  type ViewDataFrame, VIEW_CHUNK_BYTES, VIEW_U64_MAX,
} from "../../src/views/binary";
import { ViewDecodeError } from "../../src/views/errors";
import { RawJsonDocument } from "../../src/views/raw-json";
import { ViewRecordAssembler } from "../../src/views/records";

const STREAM = encodeBase64url(new Uint8Array(16).fill(1));
const OTHER = encodeBase64url(new Uint8Array(16).fill(2));
function data(
  packet: bigint, offset: bigint, total: bigint,
  bytes: Uint8Array, overrides: Partial<ViewDataFrame> = {},
): ViewDataFrame {
  return {
    type: "data", streamId: STREAM, packetSequence: packet, recordId: 7n,
    byteOffset: offset, totalLength: total, kind: "original_raw",
    last: offset + BigInt(bytes.length) === total, payload: bytes, ...overrides,
  };
}

describe("one-stream contiguous opaque records", () => {
  it("reassembles split UTF8 and owns input/output copies", () => {
    const raw = new TextEncoder().encode('{"s":"🙂","n":9007199254740993}');
    const assembler = new ViewRecordAssembler(STREAM);
    const first = raw.slice(0, 8);
    expect(assembler.accept(data(1n, 0n, BigInt(raw.length), first))).toBeNull();
    first.fill(0);
    const record = assembler.accept(data(
      2n, 8n, BigInt(raw.length), raw.subarray(8),
    ));
    expect(record?.bytes()).toEqual(raw);
    if (record === null) throw new Error("expected completed record");
    const copy = record.bytes();
    copy.fill(0);
    expect(record.bytes()).toEqual(raw);
    expect(Object.isFrozen(record)).toBe(true);
    const document = RawJsonDocument.parse(record.bytes());
    expect(document.stringValue(2)).toBe("🙂");
    expect(document.tokenLexeme(4)).toBe("9007199254740993");
  });

  it("does not preallocate a declared u64 total or impose a total quota", () => {
    const assembler = new ViewRecordAssembler(STREAM);
    expect(assembler.accept(data(
      1n, 0n, VIEW_U64_MAX, new Uint8Array(VIEW_CHUNK_BYTES),
    ))).toBeNull();
    assembler.seal();
    assembler.seal();
    expect(() => assembler.accept(data(2n, 0n, 1n, new Uint8Array([0]))))
      .toThrowError(new ViewDecodeError("sealed"));
  });

  it("accepts a record over one MiB through fixed-size chunks", () => {
    const raw = new TextEncoder().encode('"' + "x".repeat(1_048_576 + 17) + '"');
    const assembler = new ViewRecordAssembler(STREAM);
    let completed = null;
    let packet = 1n;
    for (let offset = 0; offset < raw.length; offset += VIEW_CHUNK_BYTES) {
      completed = assembler.accept(data(
        packet++, BigInt(offset), BigInt(raw.length),
        raw.subarray(offset, offset + VIEW_CHUNK_BYTES),
      ));
      if (offset + VIEW_CHUNK_BYTES < raw.length) expect(completed).toBeNull();
    }
    expect(completed?.bytes()).toEqual(raw);
  }, 30_000);

  it.each([
    ["different stream", { streamId: OTHER }],
    ["duplicate packet", { packetSequence: 1n }],
    ["skipped packet", { packetSequence: 3n }],
    ["interleaved record", { recordId: 8n }],
    ["changed kind", { kind: "status" as const }],
    ["changed total", { totalLength: 7n }],
    ["skipped offset", { byteOffset: 3n }],
  ] as const)("rejects %s and permanently seals only this assembler", (_, changes) => {
    const assembler = new ViewRecordAssembler(STREAM);
    expect(assembler.accept(data(1n, 0n, 6n, new Uint8Array([1, 2])))).toBeNull();
    expect(() => assembler.accept(data(
      2n, 2n, 6n, new Uint8Array([3, 4]), changes,
    ))).toThrow(ViewDecodeError);
    expect(() => assembler.accept(data(2n, 2n, 6n, new Uint8Array([3, 4]))))
      .toThrowError(new ViewDecodeError("sealed"));
    expect(new ViewRecordAssembler(STREAM).accept(
      data(1n, 0n, 1n, new Uint8Array([1])),
    )?.bytes()).toEqual(new Uint8Array([1]));
  });

  it("rejects packet zero or a stream starting at two", () => {
    for (const packet of [0n, 2n]) {
      expect(() => new ViewRecordAssembler(STREAM)
        .accept(data(packet, 0n, 1n, new Uint8Array([1]))))
        .toThrow(ViewDecodeError);
    }
  });

  it("classifies an input getter RangeError as invalid data and still seals", () => {
    const assembler = new ViewRecordAssembler(STREAM);
    const frame = data(1n, 0n, 1n, new Uint8Array([1]));
    Object.defineProperty(frame, "type", {
      get() { throw new RangeError("synthetic-getter-private-canary"); },
    });
    expect(() => assembler.accept(frame))
      .toThrowError(new ViewDecodeError("invalid_record"));
    expect(() => assembler.accept(data(1n, 0n, 1n, new Uint8Array([1]))))
      .toThrowError(new ViewDecodeError("sealed"));
  });

  it("keeps record kind namespaces distinct and accepts empty SnapshotEnd", () => {
    const assembler = new ViewRecordAssembler(STREAM);
    expect(assembler.accept(data(1n, 0n, 1n, new Uint8Array([1]), {
      recordId: 20n,
    }))?.recordId).toBe(20n);
    expect(assembler.accept(data(2n, 0n, 1n, new Uint8Array([2]), {
      recordId: 1n, kind: "status",
    }))?.kind).toBe("status");
    expect(assembler.accept(data(3n, 0n, 0n, new Uint8Array(), {
      recordId: 1n, kind: "snapshot_end",
    }))?.bytes()).toEqual(new Uint8Array());
  });

  it("discards a half record when rebinding to a new stream", () => {
    const old = new ViewRecordAssembler(STREAM);
    old.accept(data(1n, 0n, 4n, new Uint8Array([1, 2])));
    old.seal();
    const current = new ViewRecordAssembler(OTHER);
    expect(current.accept(data(1n, 0n, 2n, new Uint8Array([3, 4]), {
      streamId: OTHER,
    }))?.bytes()).toEqual(new Uint8Array([3, 4]));
    expect(() => old.accept(data(2n, 2n, 4n, new Uint8Array([3, 4]))))
      .toThrowError(new ViewDecodeError("sealed"));
  });
});
