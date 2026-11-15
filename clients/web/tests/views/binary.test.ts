import { describe, expect, it } from "vitest";
import { encodeBase64url } from "../../src/protocol/jws";
import {
  decodeViewFrame, encodeViewCredit, encodeViewDetach,
  VIEW_CHUNK_BYTES, VIEW_U64_MAX,
} from "../../src/views/binary";
import { ViewDecodeError } from "../../src/views/errors";

const STREAM = encodeBase64url(new Uint8Array(16).fill(4));
function hex(value: string): Uint8Array {
  return Uint8Array.from(value.match(/../g) ?? [], (part) => parseInt(part, 16));
}
const GOLDEN = hex(
  "4143505601010001" + "04".repeat(16) +
  "0020000000000001" + "0000000000000005" +
  "fffffffffffffffb" + "ffffffffffffffff" +
  "0100000000000000" + "0000000400000000" + "80f09f92",
);
function invalid(wire: Uint8Array): void {
  expect(() => decodeViewFrame(wire)).toThrow(ViewDecodeError);
}

describe("opaque ACPV binary format", () => {
  it("decodes an independent big-endian vector without converting u64 to Number", () => {
    const frame = decodeViewFrame(GOLDEN);
    expect(frame).toMatchObject({
      type: "data", streamId: STREAM, packetSequence: 9_007_199_254_740_993n,
      recordId: 5n, byteOffset: VIEW_U64_MAX - 4n,
      totalLength: VIEW_U64_MAX, kind: "original_raw", last: true,
    });
    if (frame.type !== "data") throw new Error("expected data");
    expect(frame.payload).toEqual(hex("80f09f92"));
    const wire = GOLDEN.slice();
    const owned = decodeViewFrame(wire);
    wire[72] = 0;
    if (owned.type !== "data") throw new Error("expected data");
    expect(owned.payload[0]).toBe(128);
    expect(Object.isFrozen(owned)).toBe(true);
    const padded = new Uint8Array(GOLDEN.length + 7);
    padded.set(GOLDEN, 3);
    expect(decodeViewFrame(padded.subarray(3, 3 + GOLDEN.length))).toEqual(frame);
  });

  it.each([
    [0, 0], [4, 2], [5, 0], [5, 4], [6, 1], [7, 2],
    [56, 0], [56, 5], ...Array.from({ length: 7 }, (_, i) => [57 + i, 1]),
    ...Array.from({ length: 4 }, (_, i) => [68 + i, 1]),
  ])("rejects a malformed fixed field at %i", (offset, value) => {
    const wire = GOLDEN.slice();
    if (offset === undefined || value === undefined) throw new Error("fixture");
    wire[offset] = value;
    invalid(wire);
  });

  it("rejects truncated, trailing and oversized payload shapes", () => {
    invalid(GOLDEN.subarray(0, 71));
    invalid(GOLDEN.subarray(0, GOLDEN.length - 1));
    invalid(new Uint8Array([...GOLDEN, 0]));
    const large = new Uint8Array(72 + VIEW_CHUNK_BYTES + 1);
    large.set(GOLDEN.subarray(0, 72));
    new DataView(large.buffer).setUint32(64, VIEW_CHUNK_BYTES + 1);
    invalid(large);
  });

  it("rejects zero identities, overflow, inconsistent last and byte positions", () => {
    for (const offset of [24, 32]) {
      const wire = GOLDEN.slice();
      new DataView(wire.buffer).setBigUint64(offset, 0n);
      invalid(wire);
    }
    for (const [offset, value] of [[40, VIEW_U64_MAX], [48, 3n]] as const) {
      const wire = GOLDEN.slice();
      new DataView(wire.buffer).setBigUint64(offset, value);
      invalid(wire);
    }
    const notLast = GOLDEN.slice();
    notLast[7] = 0;
    invalid(notLast);
  });

  it("accepts only empty SnapshotEnd and requires its exact last position", () => {
    const empty = GOLDEN.slice(0, 72);
    const header = new DataView(empty.buffer);
    header.setBigUint64(40, 0n);
    header.setBigUint64(48, 0n);
    header.setUint32(64, 0);
    empty[56] = 4;
    expect(decodeViewFrame(empty)).toMatchObject({ kind: "snapshot_end", last: true });
    empty[56] = 1;
    invalid(empty);
    empty[56] = 4;
    empty[7] = 0;
    invalid(empty);
    const nonempty = GOLDEN.slice();
    nonempty[56] = 4;
    invalid(nonempty);
  });

  it("encodes canonical Credit and zero-only Detach", () => {
    for (const packet of [0n, 1n, 9_007_199_254_740_993n, VIEW_U64_MAX]) {
      const wire = encodeViewCredit(STREAM, packet);
      expect(wire.length).toBe(72);
      expect(decodeViewFrame(wire)).toEqual({
        type: "credit", streamId: STREAM, packetSequence: packet,
      });
      expect(new DataView(wire.buffer).getBigUint64(24)).toBe(packet);
    }
    const detach = encodeViewDetach(STREAM);
    expect(decodeViewFrame(detach)).toEqual({
      type: "detach", streamId: STREAM, packetSequence: 0n,
    });
    new DataView(detach.buffer).setBigUint64(24, 1n);
    invalid(detach);
  });

  it.each([6, 7, 32, 40, 48, 56, 57, 63, 64, 68, 71])(
    "rejects control fields that carry nonzero data at %i", (offset) => {
      const wire = encodeViewCredit(STREAM, 0n);
      wire[offset] = 1;
      invalid(wire);
    },
  );

  it("rejects noncanonical stream IDs and out-of-range encoder values", () => {
    for (const stream of ["", STREAM + "=", STREAM + "A", STREAM.slice(0, 21) + "R"]) {
      expect(() => encodeViewCredit(stream, 0n)).toThrow(ViewDecodeError);
      expect(() => encodeViewDetach(stream)).toThrow(ViewDecodeError);
    }
    for (const packet of [-1n, VIEW_U64_MAX + 1n]) {
      expect(() => encodeViewCredit(STREAM, packet)).toThrow(ViewDecodeError);
    }
    expect(() => encodeViewCredit(STREAM, 1 as unknown as bigint))
      .toThrow(ViewDecodeError);
  });

  it("reports only static errors for untrusted byte canaries", () => {
    const canary = new TextEncoder().encode("secret-native-config-canary");
    try {
      decodeViewFrame(canary);
      throw new Error("expected invalid header");
    } catch (error: unknown) {
      expect(error).toBeInstanceOf(ViewDecodeError);
      expect(String(error)).toBe("ViewDecodeError: invalid_header");
    }
  });
});
