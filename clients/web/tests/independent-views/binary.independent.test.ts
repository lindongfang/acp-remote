import { describe, expect, it } from "vitest";
import { decodeViewFrame, encodeViewCredit, encodeViewDetach, requireViewU64, validateViewData, viewStreamBytes } from "../../src/views/binary";
import { allocateViewBytes, copyViewBytes } from "../../src/views/errors";
import { STREAM, MAX, controlWire, dataWire, frame, reject } from "./support";

describe("SV-067 local format only", () => {
  it("B01 golden full header decodes exact big-endian u64 without Number", () => {
    const payload = Uint8Array.of(0xf0, 0x9f, 0x92, 0xa9);
    const wire = dataWire({ packet: 9_007_199_254_740_993n, record: MAX, offset: MAX - 4n, total: MAX, payload });
    const decoded = decodeViewFrame(wire);
    expect(decoded).toMatchObject({ type: "data", streamId: STREAM, packetSequence: 9_007_199_254_740_993n, recordId: MAX, byteOffset: MAX - 4n, totalLength: MAX, last: true, kind: "original_raw" });
    expect(Object.isFrozen(decoded)).toBe(true);
    if (decoded.type !== "data") throw new Error("expected Data");
    expect(decoded.payload).toEqual(payload);
    wire.fill(0);
    expect(decoded.payload).toEqual(payload);
  });
  it("B02 decoder honors nonzero view offset and owns payload independently", () => {
    const wire = dataWire({ payload: Uint8Array.of(1, 2, 3) });
    const containing = new Uint8Array(wire.length + 19).fill(0xff);
    containing.set(wire, 11);
    const decoded = decodeViewFrame(containing.subarray(11, 11 + wire.length));
    expect(decoded.type).toBe("data");
    containing.fill(0);
    if (decoded.type === "data") expect(decoded.payload).toEqual(Uint8Array.of(1, 2, 3));
  });
  it.each([[1, "original_raw"], [2, "item_projection"], [3, "status"]] as const)("B03 nonempty kind %s permits exact 16384 payload", (kind, name) => {
    const decoded = decodeViewFrame(dataWire({ kind, payload: new Uint8Array(16_384).fill(kind) }));
    expect(decoded.type).toBe("data");
    if (decoded.type === "data") { expect(decoded.kind).toBe(name); expect(decoded.payload.length).toBe(16_384); }
  });
  it("B04 SnapshotEnd only is empty and all positions are exact", () => {
    expect(decodeViewFrame(dataWire({ kind: 4, payload: new Uint8Array() }))).toMatchObject({ type: "data", kind: "snapshot_end", totalLength: 0n, byteOffset: 0n, last: true });
    for (const kind of [1, 2, 3]) reject(() => decodeViewFrame(dataWire({ kind, payload: new Uint8Array() })));
    reject(() => decodeViewFrame(dataWire({ kind: 4 })));
    reject(() => decodeViewFrame(dataWire({ kind: 4, payload: new Uint8Array(), total: 1n })));
  });
  it("B05 Credit and Detach encoders equal independently built 72-byte vectors", () => {
    for (const packet of [0n, 1n, 9_007_199_254_740_993n, MAX]) {
      const wire = encodeViewCredit(STREAM, packet);
      expect(wire).toEqual(controlWire(2, packet));
      expect(decodeViewFrame(wire)).toEqual({ type: "credit", streamId: STREAM, packetSequence: packet });
    }
    expect(encodeViewDetach(STREAM)).toEqual(controlWire(3, 0n));
    expect(decodeViewFrame(encodeViewDetach(STREAM))).toEqual({ type: "detach", streamId: STREAM, packetSequence: 0n });
    reject(() => decodeViewFrame(controlWire(3, 1n)));
  });
  it.each([0, 1, 2, 3, 4, 5, 6, 7, 56, 57, 58, 59, 60, 61, 62, 63, 68, 69, 70, 71])("B06 rejects mutated control/header byte %s", (index) => {
    const wire = dataWire(); wire[index] = 0xff; reject(() => decodeViewFrame(wire));
  });
  it("B07 every truncated header and mismatched payload declaration rejects", () => {
    const good = dataWire();
    for (let length = 0; length < 72; ++length) reject(() => decodeViewFrame(good.slice(0, length)), "invalid_header");
    reject(() => decodeViewFrame(good.slice(0, 72)));
    const extra = new Uint8Array(good.length + 1); extra.set(good); reject(() => decodeViewFrame(extra));
    for (const declared of [0, 2, 16_385, 0xffff_ffff]) {
      const changed = good.slice(); new DataView(changed.buffer).setUint32(64, declared); reject(() => decodeViewFrame(changed));
    }
    reject(() => decodeViewFrame(dataWire({ payload: new Uint8Array(16_385) })));
  });
  it("B08 zero data IDs, overflow, wrong last and outside total all reject", () => {
    for (const options of [{ packet: 0n }, { record: 0n }, { offset: MAX, total: MAX }, { offset: 2n, total: 1n }, { flags: 0 }, { flags: 1, total: 2n }, { kind: 0 }]) reject(() => decodeViewFrame(dataWire(options)));
    expect(decodeViewFrame(dataWire({ packet: MAX, record: MAX }))).toMatchObject({ packetSequence: MAX, recordId: MAX });
  });
  it("B09 every nonstream control field must be zero", () => {
    for (const kind of [2, 3] as const) {
      for (const offset of [6, 7, 32, 39, 40, 47, 48, 55, 56, 57, 63, 64, 67, 68, 71]) {
        const wire = controlWire(kind, 0n); wire[offset] = 1; reject(() => decodeViewFrame(wire));
      }
    }
  });
  it.each(["", "A", STREAM + "=", STREAM + "A", STREAM.slice(1), STREAM.slice(0, -1) + "x", "!".repeat(22), " ".repeat(22), "é".repeat(22)])("B10 rejects noncanonical stream %s", (stream) => {
    reject(() => encodeViewCredit(stream, 0n), "invalid_stream");
    reject(() => encodeViewDetach(stream), "invalid_stream");
    reject(() => viewStreamBytes(stream), "invalid_stream");
  });
  it("B11 public scalar/runtime validators reject nonbigint and invalid frame fields", () => {
    for (const value of [-1n, MAX + 1n, 0, 1, NaN, "1", null, undefined, true]) {
      reject(() => requireViewU64(value as bigint));
      reject(() => encodeViewCredit(STREAM, value as bigint));
    }
    for (const value of [0n, 1n, MAX]) expect(() => requireViewU64(value)).not.toThrow();
    const good = frame();
    for (const field of ["packetSequence", "recordId", "byteOffset", "totalLength"]) {
      for (const invalid of [-1n, MAX + 1n, 1, "1", NaN]) reject(() => validateViewData({ ...good, [field]: invalid } as unknown as typeof good));
    }
    for (const change of [{ type: "credit" }, { payload: [] }, { last: 1 }, { packetSequence: 1 }, { recordId: -1n }, { kind: "other" }, { byteOffset: MAX }]) reject(() => validateViewData({ ...good, ...change } as unknown as typeof good));
    const decoded = viewStreamBytes(STREAM); decoded.fill(0);
    expect(viewStreamBytes(STREAM)).toEqual(Uint8Array.from({ length: 16 }, (_, i) => i));
  });
  it("B12 checked public allocation representation and copying do not expose aliases", () => {
    for (const bad of [-1, NaN, Infinity, 1.5, Number.MAX_SAFE_INTEGER + 1]) reject(() => allocateViewBytes(bad), "length_unrepresentable");
    expect(allocateViewBytes(0)).toEqual(new Uint8Array());
    const original = Uint8Array.of(1, 2); const copy = copyViewBytes(original); original.fill(9); expect(copy).toEqual(Uint8Array.of(1, 2));
  });
});
