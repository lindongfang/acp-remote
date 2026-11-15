import { describe, expect, it } from "vitest";
import { ProjectionCollection } from "../../src/views/projection-collection";
import type { ProjectedItem } from "../../src/views/projection";
import { CANARY, decodedItem, item, reject, SEGMENT, STREAM } from "./support";

// Ordinary synchronous tests are sequential within this isolated file; no concurrent modifier.
describe("WP07 exact single internal freeze failure", () => {
  for (const operation of ["insert", "replace"] as const) it(`${operation}: refunds the candidate and commits neither Map nor array before freeze`, () => {
    const collection = new ProjectionCollection(STREAM, SEGMENT); const original = decodedItem(); collection.accept(original);
    const old = collection.items();
    const incoming = decodedItem(item(operation === "insert" ? { anchorId: "8", firstPosition: "4" } : { revision: "2" }));
    const originalFreeze = Object.freeze;
    let hit = 0; let caught: unknown;
    try {
      Object.freeze = ((value: unknown) => {
        if (Array.isArray(value) && value.some((entry: unknown) => typeof entry === "object" && entry !== null && (entry as ProjectedItem).anchorId === incoming.anchorId && (entry as ProjectedItem).revision === incoming.revision)) {
          ++hit; throw new RangeError(CANARY);
        }
        return originalFreeze(value);
      }) as typeof Object.freeze;
      // Deliberately no assertion/framework work in the replaced built-in's synchronous window.
      try { collection.accept(incoming); } catch (error) { caught = error; }
    } finally { Object.freeze = originalFreeze; }
    expect(hit).toBe(1); reject(() => { throw caught; }, "allocation_failed");
    expect(collection.items()).toEqual(old); expect(collection.items()[0]).toBe(original); expect(old[0]).toBe(original);
    expect(collection.accept(incoming)).toBe(true); expect(collection.accept(incoming)).toBe(false);
    const after = collection.items(); expect(after).toHaveLength(operation === "insert" ? 2 : 1);
    expect(after[0]).toBe(incoming); expect(old[0]!.revision).toBe(1n);
    console.info(`WP07 internal ${operation} freeze RangeError hit=${hit}; retry=true; replay=false`);
  });
  it("constructor internal Map allocation uses the same sanitized allocation_failed boundary", () => {
    const OriginalMap = globalThis.Map;
    let hit = 0; let caught: unknown;
    try {
      globalThis.Map = class extends OriginalMap {
        constructor() { super(); ++hit; throw new RangeError(CANARY); }
      } as typeof Map;
      // No framework/caller-object execution inside this internal allocation fault window.
      try { new ProjectionCollection(STREAM, SEGMENT); } catch (error) { caught = error; }
    } finally { globalThis.Map = OriginalMap; }
    expect(hit).toBe(1); reject(() => { throw caught; }, "allocation_failed");
    const fresh = new ProjectionCollection(STREAM, SEGMENT); expect(fresh.accept(decodedItem())).toBe(true);
  });
  it("constructor empty-array freeze uses the sanitized allocation_failed boundary", () => {
    const originalFreeze = Object.freeze;
    let hit = 0; let caught: unknown;
    try {
      Object.freeze = ((value: unknown) => {
        if (Array.isArray(value) && value.length === 0) { ++hit; throw new RangeError(CANARY); }
        return originalFreeze(value);
      }) as typeof Object.freeze;
      try { new ProjectionCollection(STREAM, SEGMENT); } catch (error) { caught = error; }
    } finally { Object.freeze = originalFreeze; }
    expect(hit).toBe(1); reject(() => { throw caught; }, "allocation_failed");
    const fresh = new ProjectionCollection(STREAM, SEGMENT); expect(fresh.accept(decodedItem())).toBe(true);
    console.info(`WP07 constructor empty-freeze RangeError hit=${hit}; fresh instance accepted`);
  });
});
