import { describe, expect, it } from "vitest";
import { ViewDecodeError } from "../../src/views/errors";
import {
  decodeProjectionRecord, PROJECTION_FIELDS, type ProjectedItem,
} from "../../src/views/projection";
import { ProjectionCollection } from "../../src/views/projection-collection";
import type { CompletedViewRecord } from "../../src/views/records";

const STREAM = "AAECAwQFBgcICQoLDA0ODw";
const OTHER_STREAM = "Dw4NDAsKCQgHBgUEAwIBAA";
const SEGMENT = "AQEBAQEBAQEBAQEBAQEBAQ";
const ENCODER = new TextEncoder();
function payload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    segmentId: SEGMENT, type: "item_projection", anchorId: "0", firstPosition: "5",
    revision: "5", rawRecordId: "3", kind: "tool",
    fields: PROJECTION_FIELDS.map((field) => ({
      field, knowledge: "unknown", raw: null, enumCode: null, evidenceCount: "0",
    })), ...overrides,
  };
}
function record(bytes: Uint8Array, streamId = STREAM): CompletedViewRecord {
  return { streamId, recordId: 17n, kind: "item_projection", length: BigInt(bytes.length),
    bytes: () => bytes.slice() };
}
function decode(input: Record<string, unknown>, streamId = STREAM): ProjectedItem {
  const value = decodeProjectionRecord(record(ENCODER.encode(JSON.stringify(input)), streamId));
  if (value.type !== "item_projection") throw new Error("Expected item");
  return value;
}
function errorCode(action: () => unknown, code: string): void {
  let actual: unknown;
  try { action(); } catch (error: unknown) { actual = error; }
  expect(actual).toBeInstanceOf(ViewDecodeError);
  expect((actual as ViewDecodeError).code).toBe(code);
}

describe("platform projection metadata and collection", () => {
  it.each(["map", "freeze"])("maps internal constructor %s allocation errors", (point) => {
    const originalMap = globalThis.Map;
    const originalFreeze = Object.freeze;
    let hit = 0;
    let actual: unknown;
    try {
      if (point === "map") {
        globalThis.Map = new Proxy(originalMap, {
          construct() { ++hit; throw new RangeError("internal allocation canary"); },
        });
      } else {
        Object.freeze = ((value: unknown) => {
          if (Array.isArray(value) && value.length === 0) {
            ++hit;
            throw new RangeError("internal allocation canary");
          }
          return originalFreeze(value);
        }) as typeof Object.freeze;
      }
      try { new ProjectionCollection(STREAM, SEGMENT); } catch (error: unknown) { actual = error; }
    } finally {
      globalThis.Map = originalMap;
      Object.freeze = originalFreeze;
    }
    expect(hit).toBe(1);
    expect(actual).toBeInstanceOf(ViewDecodeError);
    expect((actual as ViewDecodeError).code).toBe("allocation_failed");
    expect((actual as Error).message).not.toContain("canary");
  });

  it("updates a late tool at its first position while keeping later messages and old snapshots", () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT);
    const tool = decode(payload());
    const a = decode(payload({ anchorId: "1", firstPosition: "6", revision: "6", kind: "message" }));
    const b = decode(payload({ anchorId: "2", firstPosition: "7", revision: "7", kind: "message" }));
    collection.accept(tool); collection.accept(a); collection.accept(b);
    const before = collection.items();
    const completed = decode(payload({ revision: "20", rawRecordId: "12" }));
    expect(collection.accept(completed)).toBe(true);
    expect(collection.items()).toEqual([completed, a, b]);
    expect(before).toEqual([tool, a, b]);
    expect(collection.items()[1]).toBe(a);
    expect(collection.accept(tool)).toBe(false);
    expect(collection.accept(decode(payload({ revision: "20", rawRecordId: "12" })))).toBe(false);
    errorCode(() => collection.accept(decode(payload({ revision: "1", firstPosition: "99" }))), "invalid_record");
    errorCode(() => collection.accept(decode(payload({ revision: "20", rawRecordId: "13" }))), "invalid_record");
    expect(collection.items()).toEqual([completed, a, b]);
  });

  it("preserves u64 precision and sorts numeric positions without Number conversion", () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT);
    const later = decode(payload({ anchorId: "18446744073709551615", firstPosition: "9007199254740993" }));
    const earlier = decode(payload({ anchorId: "9007199254740993", firstPosition: "9007199254740992" }));
    collection.accept(later); collection.accept(earlier);
    expect(collection.items()).toEqual([earlier, later]);
    expect(later.anchorId).toBe(0xffff_ffff_ffff_ffffn);
    errorCode(() => decode(payload({ revision: "18446744073709551616" })), "invalid_json");
    errorCode(() => decode(payload({ rawRecordId: "0" })), "invalid_json");
  });

  it("separates decoded scope from forged objects and evidence", () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT);
    const item = decode(payload());
    errorCode(() => collection.accept({ ...item }), "invalid_record");
    errorCode(() => collection.accept(decode(payload(), OTHER_STREAM)), "invalid_stream");
    const evidence = decodeProjectionRecord(record(ENCODER.encode(JSON.stringify({
      type: "projection_evidence", segmentId: SEGMENT, anchorId: "0", revision: "5",
      field: "rawOutput", index: "0",
      raw: { rawRecordId: "3", byteStart: "11", byteEnd: "19", append: true },
    }))));
    expect(evidence.type).toBe("projection_evidence");
    if (evidence.type !== "projection_evidence") throw new Error("Expected evidence");
    expect(evidence.raw.append).toBe(true);
    expect(Object.isFrozen(evidence.raw)).toBe(true);
    errorCode(() => collection.accept(evidence as unknown as ProjectedItem), "invalid_record");
    expect(collection.items()).toEqual([]);
  });

  it("rejects duplicate metadata keys and invalid UTF8 without treating raw references as native JSON", () => {
    const json = JSON.stringify(payload());
    const duplicate = json.replace('"type":"item_projection"', '"type":"item_projection","type":"item_projection"');
    errorCode(() => decodeProjectionRecord(record(ENCODER.encode(duplicate))), "invalid_json");
    errorCode(() => decodeProjectionRecord(record(Uint8Array.of(0xc0, 0x80))), "invalid_utf8");
    const padded = ENCODER.encode(" ".repeat(20_000) + json);
    expect(decodeProjectionRecord(record(padded)).type).toBe("item_projection");
  });

  it("sanitizes caller exceptions and reads real Uint8Array ranges without custom conversions", () => {
    const bytes = ENCODER.encode(JSON.stringify(payload()));
    const base = record(bytes);
    errorCode(() => decodeProjectionRecord({ ...base, bytes: () => { throw new RangeError("private canary"); } }), "invalid_record");
    const padded = new Uint8Array(bytes.length + 9);
    padded.set(bytes, 4);
    const view = padded.subarray(4, bytes.length + 4);
    let calls = 0;
    const fail = () => { ++calls; throw new Error("Caller conversion"); };
    for (const name of ["byteLength", "byteOffset", "buffer", "length", "constructor"]) {
      Object.defineProperty(view, name, { get: fail });
    }
    Object.defineProperty(view, Symbol.iterator, { get: fail });
    Object.defineProperty(view, "toString", { get: fail });
    expect(decodeProjectionRecord({ ...base, bytes: () => view }).anchorId).toBe(0n);
    expect(calls).toBe(0);
    const collection = new ProjectionCollection(STREAM, SEGMENT);
    collection.accept(decode(payload()));
    expect(Object.isFrozen(collection.items())).toBe(true);
    expect(collection.items()).not.toBe(collection.items());
  });
});
