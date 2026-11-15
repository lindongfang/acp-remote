import { describe, expect, it } from "vitest";
import { decodeProjectionRecord } from "../../src/views/projection";
import type { CompletedViewRecord } from "../../src/views/records";
import { ViewDecodeError } from "../../src/views/errors";
import { bytes, CANARY, decoded, decodedItem, evidence, FIELD_NAMES, item, KNOWLEDGES, MAX, record, ref, reject, SEGMENT, STREAM } from "./support";

describe("WP01 caller boundary", () => {
  it("accepts independent transport IDs at u64 edges and rejects non-projection transport kinds", () => {
    for (const recordId of [1n, MAX]) expect(decodeProjectionRecord(record(bytes(item()), { recordId })).anchorId).toBe(7n);
    for (const kind of ["original_raw", "status", "snapshot_end", "future"]) reject(() => decodeProjectionRecord(record(bytes(item()), { kind: kind as CompletedViewRecord["kind"] })), "invalid_record");
  });
  it("checks recordId and length without Number conversion", () => {
    for (const key of ["recordId", "length"] as const) {
      for (const value of [0n, -1n, MAX + 1n, 1, "1", null, undefined]) {
        const supplied = { ...record(bytes(item())), [key]: value } as unknown as CompletedViewRecord;
        reject(() => decodeProjectionRecord(supplied), "invalid_record");
      }
    }
    const payload = bytes(item());
    for (const length of [BigInt(payload.length - 1), BigInt(payload.length + 1), MAX]) reject(() => decodeProjectionRecord(record(payload, { length })), "invalid_record");
  });
  it("external stream format is invalid_stream and never coerces", () => {
    for (const streamId of ["", "x", `${STREAM}=`, "AAAAAAAAAAAAAAAAAAAAAB", "!".repeat(22), 1, null]) {
      reject(() => decodeProjectionRecord({ ...record(bytes(item())), streamId } as unknown as CompletedViewRecord), "invalid_stream");
    }
  });
  it("sanitizes every caller getter and bytes invocation, including forged known errors", () => {
    for (const key of ["streamId", "recordId", "length", "kind", "bytes"]) {
      for (const error of [new Error(CANARY), new RangeError(CANARY), new ViewDecodeError("allocation_failed")]) {
        const supplied = Object.defineProperty(record(bytes(item())), key, { get() { throw error; } });
        const output = reject(() => decodeProjectionRecord(supplied), "invalid_record");
        expect(output).not.toBe(error);
      }
    }
    for (const error of [new Error(CANARY), new RangeError(CANARY), new ViewDecodeError("invalid_utf8")]) reject(() => decodeProjectionRecord(record(bytes(item()), { bytes() { throw error; } })), "invalid_record");
  });
  it("rejects false byte owners, Proxy and detached buffers", () => {
    const payload = bytes(item());
    for (const result of ["json", [...payload], payload.buffer, new Int8Array(payload), new Uint8ClampedArray(payload), new Proxy(payload, {})]) {
      reject(() => decodeProjectionRecord({ ...record(payload), bytes: () => result } as unknown as CompletedViewRecord), "invalid_record");
    }
    const detached = new Uint8Array(payload);
    structuredClone(detached.buffer, { transfer: [detached.buffer] });
    reject(() => decodeProjectionRecord(record(payload, { bytes: () => detached })), "invalid_record");
    reject(() => decodeProjectionRecord({ ...record(payload), bytes: 3 } as unknown as CompletedViewRecord), "invalid_record");
  });
  it("uses real offset/range and ignores executable TypedArray overrides", () => {
    const payload = bytes(item());
    const backing = new Uint8Array(payload.length + 16); backing.set(payload, 8);
    const supplied = backing.subarray(8, 8 + payload.length);
    let calls = 0;
    for (const key of [Symbol.iterator, "constructor", "toString", "byteLength", "byteOffset", "buffer", "length"]) {
      Object.defineProperty(supplied, key, { get() { ++calls; throw new RangeError(CANARY); } });
    }
    const result = decodeProjectionRecord(record(payload, { bytes: () => supplied }));
    expect(result.anchorId).toBe(7n); expect(calls).toBe(0);
  });
});

describe("WP02 closed platform metadata", () => {
  it("rejects missing/extra top-level keys for both variants", () => {
    for (const seed of [item(), evidence()]) {
      for (const key of Object.keys(seed)) { const value = { ...seed }; delete (value as Record<string, unknown>)[key]; reject(() => decoded(value), "invalid_json"); }
      reject(() => decoded({ ...seed, extra: null }), "invalid_json");
    }
  });
  it("rejects wrong top-level shapes and discriminator, kind and knowledge types", () => {
    for (const value of [null, [], 1, "text", { ...item(), type: "future" }, { ...item(), kind: "Tool" }, { ...item(), fields: {} }]) reject(() => decoded(value), "invalid_json");
    for (const value of [null, 1, true, "missingRaw", "future"]) { const wire = item(); (wire.fields[0] as unknown as Record<string, unknown>).knowledge = value; reject(() => decoded(wire), "invalid_json"); }
  });
  it("requires exact nine ordered fields and exact nested field/reference keys", () => {
    for (const make of [() => item().fields.slice(0, 8), () => [...item().fields, item().fields[0]], () => { const fields = item().fields; [fields[0], fields[1]] = [fields[1]!, fields[0]!]; return fields; }, () => { const fields = item().fields; fields[1] = fields[0]!; return fields; }]) reject(() => decoded({ ...item(), fields: make() }), "invalid_json");
    for (const fieldKey of ["field", "knowledge", "raw", "enumCode", "evidenceCount"]) { const wire = item(); delete (wire.fields[0] as unknown as Record<string, unknown>)[fieldKey]; reject(() => decoded(wire), "invalid_json"); }
    { const wire = item(); (wire.fields[0] as unknown as Record<string, unknown>).extra = 1; reject(() => decoded(wire), "invalid_json"); }
    for (const key of ["rawRecordId", "byteStart", "byteEnd", "append"]) { const raw = { ...ref() } as Record<string, unknown>; delete raw[key]; reject(() => decoded(evidence({ raw })), "invalid_json"); }
    reject(() => decoded(evidence({ raw: { ...ref(), extra: null } })), "invalid_json");
    for (const value of [null, 0, "true", [], {}]) reject(() => decoded(evidence({ raw: { ...ref(), append: value } })), "invalid_json");
  });
  it("validates enum codes without coercion", () => {
    for (const enumCode of [-1, 256, 1.5, "0", false, {}, []]) { const wire = item(); (wire.fields[0] as unknown as Record<string, unknown>).enumCode = enumCode; reject(() => decoded(wire), "invalid_json"); }
    for (const enumCode of [null, 0, 255]) { const wire = item(); wire.fields[0]!.enumCode = enumCode; expect(decodedItem(wire).fields[0]!.enumCode).toBe(enumCode); }
  });
  it("strict JSON rejects duplicate decoded keys at every layer and pollution keys", () => {
    const valid = JSON.stringify(item());
    const invalid = [valid.replace('"type":', '"type":"item_projection","\\u0074ype":'), valid.replace('"field":"content"', '"field":"content","field":"content"'), JSON.stringify(evidence({ raw: ref() })).replace('"append":false', '"append":false,"append":true'), JSON.stringify(evidence()).replace('"append":true', '"append":true,"append":false'), valid.slice(0, -1) + ',"__proto__":{"polluted":true}}', valid.slice(0, -1) + ',"constructor":{}}'];
    for (const source of invalid) reject(() => decodeProjectionRecord(record(new TextEncoder().encode(source))), "invalid_json");
    expect(Object.hasOwn(Object.prototype, "polluted")).toBe(false);
  });
  it("separates fatal UTF8 from JSON errors and does not apply compact-JWS size limits", () => {
    for (const payload of [Uint8Array.of(0xc0, 0x80), Uint8Array.of(0x80), Uint8Array.of(0xe2, 0x82)]) reject(() => decodeProjectionRecord(record(payload)), "invalid_utf8");
    for (const source of ["{", "{'a':1}", `${JSON.stringify(item())}x`, "1e9999", '{"type":NaN}', '{"type":"\\uD800"}']) reject(() => decodeProjectionRecord(record(new TextEncoder().encode(source))), "invalid_json");
    const expanded = new TextEncoder().encode(" ".repeat(20_000) + JSON.stringify(item()));
    expect(expanded.length).toBeGreaterThan(16_384); expect(decodeProjectionRecord(record(expanded)).anchorId).toBe(7n);
  });
});

describe("WP03 u64 identities and exact references", () => {
  it("retains the full u64 range across all item and field counters", () => {
    for (const n of [0n, 1n, 9_007_199_254_740_991n, 9_007_199_254_740_992n, 9_007_199_254_740_993n, MAX]) {
      const wire = item({ anchorId: String(n), firstPosition: String(n), revision: String(n), rawRecordId: String(n || 1n) });
      for (const field of wire.fields) { field.evidenceCount = String(n); field.raw = ref({ rawRecordId: String(n || 1n), byteStart: String(n), byteEnd: String(n), append: true }); }
      const output = decodedItem(wire);
      expect([output.anchorId, output.firstPosition, output.revision]).toEqual([n, n, n]);
      expect(output.rawRecordId).toBe(n || 1n);
      for (const f of output.fields) { expect(f.evidenceCount).toBe(n); expect(f.raw).toEqual({ rawRecordId: n || 1n, byteStart: n, byteEnd: n, append: true }); }
    }
  });
  it("rejects noncanonical/overflow integer strings in every location", () => {
    const values = ["00", "01", "-0", "+1", "-1", "1.0", "1e3", " 1", "", String(MAX + 1n), 1, false, null, []];
    for (const value of values) {
      for (const key of ["anchorId", "firstPosition", "revision", "rawRecordId"]) reject(() => decoded({ ...item(), [key]: value }), "invalid_json");
      const wire = item(); (wire.fields[0] as unknown as Record<string, unknown>).evidenceCount = value; reject(() => decoded(wire), "invalid_json");
      for (const key of ["rawRecordId", "byteStart", "byteEnd"]) reject(() => decoded(evidence({ raw: { ...ref(), [key]: value } })), "invalid_json");
      reject(() => decoded(evidence({ index: value })), "invalid_json");
    }
    reject(() => decoded({ ...item(), rawRecordId: "0" }), "invalid_json");
    reject(() => decoded(evidence({ raw: ref({ rawRecordId: "0" }) })), "invalid_json");
    reject(() => decoded(evidence({ raw: ref({ byteStart: "9", byteEnd: "8" }) })), "invalid_json");
  });
  it("segment canonical encoding is metadata, not an outer stream error", () => {
    for (const segmentId of ["", `${SEGMENT}=`, "AAAAAAAAAAAAAAAAAAAAAB", "!".repeat(22), 1, null]) reject(() => decoded({ ...item(), segmentId }), "invalid_json");
  });
});

describe("WP04 knowledge/evidence and immutability", () => {
  it("preserves all producer knowledge/kind combinations without semantic inference", () => {
    for (const kind of ["tool", "message", "state", "unknownStructured"]) for (const knowledge of KNOWLEDGES) for (const raw of [null, ref({ append: true })]) for (const enumCode of [null, 255]) {
      const wire = item({ kind }); for (const f of wire.fields) { f.knowledge = knowledge; f.raw = raw; f.enumCode = enumCode; f.evidenceCount = String(MAX); }
      const result = decodedItem(wire); expect(result.kind).toBe(kind);
      for (const f of result.fields) { expect(f.knowledge).toBe(knowledge); expect(f.enumCode).toBe(enumCode); expect(f.evidenceCount).toBe(MAX); expect(f.raw).toEqual(raw === null ? null : { rawRecordId: 11n, byteStart: 3n, byteEnd: 8n, append: true }); }
    }
  });
  it("evidence remains a distinct complete variant, with append and no premature association", () => {
    for (const field of FIELD_NAMES) for (const index of ["0", String(MAX)]) for (const append of [false, true]) {
      const result = decoded(evidence({ field, index, raw: ref({ append }) }));
      expect(result.type).toBe("projection_evidence"); if (result.type !== "projection_evidence") throw new Error();
      expect(result.field).toBe(field); expect(result.index).toBe(BigInt(index)); expect(result.raw.append).toBe(append);
      expect(Object.isFrozen(result)).toBe(true); expect(Object.isFrozen(result.raw)).toBe(true);
    }
  });
  it("deep freezes owned metadata and has no caller byte alias", () => {
    const wire = item(); wire.fields[0]!.raw = ref(); const payload = bytes(wire);
    const original = record(payload, { bytes: () => payload }); const output = decodeProjectionRecord(original);
    if (output.type !== "item_projection") throw new Error();
    expect(Object.isFrozen(output)).toBe(true); expect(Object.isFrozen(output.fields)).toBe(true);
    for (const field of output.fields) { expect(Object.isFrozen(field)).toBe(true); if (field.raw) expect(Object.isFrozen(field.raw)).toBe(true); }
    expect(Reflect.set(output, "revision", 99n)).toBe(false); expect(Reflect.set(output.fields[0]!, "knowledge", "known")).toBe(false); expect(Reflect.set(output.fields[0]!.raw!, "append", true)).toBe(false);
    payload.fill(0); (original as { recordId: bigint }).recordId = MAX;
    expect(output.revision).toBe(1n); expect(output.fields[0]!.raw?.append).toBe(false); expect(output.fields[0]!.knowledge).toBe("unknown");
  });
});
