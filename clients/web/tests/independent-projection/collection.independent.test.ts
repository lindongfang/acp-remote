import { describe, expect, it } from "vitest";
import { ProjectionCollection } from "../../src/views/projection-collection";
import type { ProjectedItem } from "../../src/views/projection";
import { bytes, decoded, decodedItem, evidence, item, MAX, OTHER_SEGMENT, OTHER_STREAM, ref, reject, SEGMENT, STREAM, type ItemWire } from "./support";

describe("WP05 immutable anchor collection", () => {
  it("sorts out-of-order new anchors by numeric first position then numeric anchor", () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT);
    for (const [anchor, position] of [["9", "20"], ["10", "5"], ["2", "5"], ["4", "10"]]) expect(collection.accept(decodedItem(item({ anchorId: anchor, firstPosition: position })))).toBe(true);
    expect(collection.items().map((v) => v.anchorId)).toEqual([2n, 10n, 4n, 9n]);
  });
  it("does not round neighboring large anchors or positions and accepts revision zero", () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT);
    for (const n of [MAX, 9_007_199_254_740_993n, 9_007_199_254_740_992n, 0n]) collection.accept(decodedItem(item({ anchorId: String(n), firstPosition: String(n), revision: "0" })));
    expect(collection.items().map((v) => v.anchorId)).toEqual([0n, 9_007_199_254_740_992n, 9_007_199_254_740_993n, MAX]);
  });
  it("updates T at the first slot while A/B/C objects and relative order remain unchanged", () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT);
    const tool = decodedItem(item({ anchorId: "1", firstPosition: "1" }));
    const a = decodedItem(item({ anchorId: "2", firstPosition: "2", kind: "message" }));
    const b = decodedItem(item({ anchorId: "3", firstPosition: "3", kind: "message" }));
    const c = decodedItem(item({ anchorId: "4", firstPosition: "4", kind: "message" }));
    for (const value of [tool, a, b]) collection.accept(value);
    for (const revision of ["2", "3", String(MAX)]) {
      const wire = item({ anchorId: "1", firstPosition: "1", revision }); wire.fields[5]!.knowledge = "known"; wire.fields[5]!.enumCode = revision === String(MAX) ? 2 : 1;
      expect(collection.accept(decodedItem(wire))).toBe(true);
      if (revision === "2") collection.accept(c);
      const all = collection.items(); expect(all.map((v) => v.anchorId)).toEqual([1n, 2n, 3n, 4n]);
      expect(all[1]).toBe(a); expect(all[2]).toBe(b); expect(all[3]).toBe(c); expect(all[0]!.firstPosition).toBe(1n);
    }
    expect(collection.accept(decodedItem(item({ anchorId: "1", firstPosition: "1", revision: "0" })))).toBe(false);
  });
  it("validates position and kind before ignoring even older revisions", () => {
    for (const revision of ["0", "5", "6"]) for (const change of [{ firstPosition: "6" }, { kind: "message" }]) {
      const collection = new ProjectionCollection(STREAM, SEGMENT);
      const original = decodedItem(item({ revision: "5" })); collection.accept(original);
      reject(() => collection.accept(decodedItem(item({ ...change, revision }))), "invalid_record");
      expect(collection.items()).toEqual([original]); expect(collection.items()[0]).toBe(original);
      expect(collection.accept(decodedItem(item({ revision: "6" })))).toBe(true);
    }
  });
  it("ignores a legal older field snapshot and logical same-revision metadata despite JSON key order", () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT);
    const currentWire = item({ revision: "5" }); currentWire.fields[0]!.knowledge = "knownEmpty";
    const current = decodedItem(currentWire); collection.accept(current);
    const old = item({ revision: "4", rawRecordId: "12" }); old.fields[0]!.raw = ref();
    expect(collection.accept(decodedItem(old))).toBe(false);
    const reordered = Object.fromEntries(Object.entries(currentWire).reverse()) as unknown as ItemWire;
    reordered.fields = currentWire.fields.map((f) => Object.fromEntries(Object.entries(f).reverse())) as ItemWire["fields"];
    expect(collection.accept(decodedItem(reordered))).toBe(false); expect(collection.accept(current)).toBe(false); expect(collection.items()[0]).toBe(current);
  });
  it("rejects every same-revision metadata conflict without partial state mutation", () => {
    const mutators: ((wire: ItemWire) => void)[] = [
      (wire) => { wire.rawRecordId = "12"; }, (wire) => { wire.fields[0]!.knowledge = "known"; },
      (wire) => { wire.fields[0]!.enumCode = 0; }, (wire) => { wire.fields[0]!.evidenceCount = "1"; },
      (wire) => { wire.fields[0]!.raw = ref({ byteStart: "4" }); },
      (wire) => { wire.fields[0]!.raw = ref({ byteEnd: "9" }); },
      (wire) => { wire.fields[0]!.raw = ref({ rawRecordId: "12" }); },
      (wire) => { wire.fields[0]!.raw = ref({ append: true }); },
    ];
    for (const change of mutators) {
      const collection = new ProjectionCollection(STREAM, SEGMENT);
      const initial = item(); initial.fields[0]!.raw = ref(); const original = decodedItem(initial); collection.accept(original);
      const incoming = item(); incoming.fields[0]!.raw = ref(); change(incoming);
      reject(() => collection.accept(decodedItem(incoming)), "invalid_record"); expect(collection.items()[0]).toBe(original);
    }
  });
  it("does not impose knowledge progression, a global revision counter or a 4096 lifetime cap", () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT);
    const seed = item(); seed.fields[0]!.knowledge = "known"; collection.accept(decodedItem(seed));
    expect(collection.accept(decodedItem(item({ revision: "2" })))).toBe(true); expect(collection.items()[0]!.fields[0]!.knowledge).toBe("unknown");
    for (let i = 0; i < 4_101; ++i) collection.accept(decodedItem(item({ anchorId: String(100 + i), firstPosition: String(100 + i), revision: i % 2 ? "0" : String(MAX) })));
    expect(collection.items()).toHaveLength(4_102); expect(collection.items()[0]!.anchorId).toBe(7n);
  });
});

describe("WP06 scope and local validation brand", () => {
  it("separates stream and segment and makes a new generation a new empty collection", () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT); const original = decodedItem(); collection.accept(original);
    for (const wrong of [decodedItem(item(), OTHER_STREAM), decodedItem(item({ segmentId: OTHER_SEGMENT })), decodedItem(item({ segmentId: OTHER_SEGMENT }), OTHER_STREAM)]) {
      reject(() => collection.accept(wrong), "invalid_stream"); expect(collection.items()[0]).toBe(original);
    }
    const other = new ProjectionCollection(OTHER_STREAM, OTHER_SEGMENT); expect(other.items()).toHaveLength(0); expect(other.accept(decodedItem(item({ segmentId: OTHER_SEGMENT }), OTHER_STREAM))).toBe(true);
    expect(collection.items()).toEqual([original]);
  });
  it("constructor rejects wrong IDs without invoking coercion", () => {
    let calls = 0; const poison = { toString() { ++calls; throw new Error(); }, [Symbol.toPrimitive]() { ++calls; throw new Error(); } };
    for (const id of ["x", `${STREAM}=`, "AAAAAAAAAAAAAAAAAAAAAB", poison, null, 4]) for (const side of [0, 1]) reject(() => new ProjectionCollection((side === 0 ? id : STREAM) as string, (side === 1 ? id : SEGMENT) as string), "invalid_stream");
    expect(calls).toBe(0);
  });
  it("does not trust structurally matching or inherited objects and never reads their getters", () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT); const original = decodedItem(); collection.accept(original);
    let reads = 0; const poison = new Proxy(original, { get() { ++reads; throw new Error(); } });
    const unknowns = [null, 3, "text", Symbol("x"), {}, { ...original }, Object.freeze({ ...original }), Object.create(original), structuredClone(original), poison];
    for (const value of unknowns) reject(() => collection.accept(value as ProjectedItem), "invalid_record");
    expect(reads).toBe(0); expect(collection.items()[0]).toBe(original);
  });
  it("evidence can be decoded and routed, but never accepted as an item", () => {
    const output = decoded(evidence()); expect(output.type).toBe("projection_evidence");
    const collection = new ProjectionCollection(STREAM, SEGMENT); reject(() => collection.accept(output as unknown as ProjectedItem), "invalid_record"); expect(collection.items()).toHaveLength(0);
    // No raw retrieval, authorization or execution claim is implied by this local check.
    expect(bytes(item()).length).toBeGreaterThan(0);
  });
});

describe("WP07 immutable snapshots", () => {
  it("returns a fresh frozen array every time and keeps old objects unchanged", () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT);
    const empty = collection.items(); expect(empty).not.toBe(collection.items()); expect(Object.isFrozen(empty)).toBe(true);
    const original = decodedItem(); collection.accept(original); const old = collection.items();
    expect(Object.isFrozen(old)).toBe(true); expect(Reflect.set(old, "0", null)).toBe(false);
    const incoming = decodedItem(item({ revision: "2" })); collection.accept(incoming); const fresh = collection.items();
    expect(fresh).not.toBe(old); expect(old[0]).toBe(original); expect(old[0]!.revision).toBe(1n); expect(fresh[0]).toBe(incoming); expect(empty).toHaveLength(0);
  });
});
