import { expect } from "vitest";
import { readFileSync } from "node:fs";
import { decodeProjectionRecord, type ProjectedItem, type ProjectionRecord } from "../../src/views/projection";
import type { CompletedViewRecord } from "../../src/views/records";
import { ViewDecodeError, type ViewDecodeErrorCode } from "../../src/views/errors";

export const STREAM = "AAECAwQFBgcICQoLDA0ODw";
export const OTHER_STREAM = "Dw4NDAsKCQgHBgUEAwIBAA";
export const SEGMENT = "AAAAAAAAAAAAAAAAAAAAAA";
export const OTHER_SEGMENT = "AQEBAQEBAQEBAQEBAQEBAQ";
export const MAX = 0xffff_ffff_ffff_ffffn;
export const CANARY = "SYNTHETIC-private-C:/never-access/secret?native-body=not-a-log";
export const FIELD_NAMES = ["content", "metadata", "name", "title", "kind", "status", "locations", "rawInput", "rawOutput"] as const;
export const KNOWLEDGES = ["unknown", "known", "knownAbsent", "knownEmpty", "partial", "uninterpreted", "deferred"] as const;
export interface RefWire { rawRecordId: string; byteStart: string; byteEnd: string; append: boolean }
export interface FieldWire { field: string; knowledge: string; raw: RefWire | null; enumCode: number | null; evidenceCount: string }
export interface ItemWire {
  type: string; segmentId: string; anchorId: string; firstPosition: string;
  revision: string; rawRecordId: string; kind: string; fields: FieldWire[];
}
export function ref(overrides: Partial<RefWire> = {}): RefWire {
  return { rawRecordId: "11", byteStart: "3", byteEnd: "8", append: false, ...overrides };
}
export function item(overrides: Partial<ItemWire> = {}): ItemWire {
  return {
    type: "item_projection", segmentId: SEGMENT, anchorId: "7", firstPosition: "5",
    revision: "1", rawRecordId: "11", kind: "tool",
    fields: FIELD_NAMES.map((field) => ({ field, knowledge: "unknown", raw: null, enumCode: null, evidenceCount: "0" })),
    ...overrides,
  };
}
export function evidence(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return { type: "projection_evidence", segmentId: SEGMENT, anchorId: "7", revision: "1", field: "content", index: "0", raw: ref({ append: true }), ...overrides };
}
export function bytes(value: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(value));
}
export function record(payload: Uint8Array, overrides: Partial<CompletedViewRecord> = {}): CompletedViewRecord {
  return { streamId: STREAM, recordId: 19n, kind: "item_projection", length: BigInt(payload.byteLength), bytes: () => new Uint8Array(payload), ...overrides };
}
export function decoded(value: unknown = item(), stream = STREAM): ProjectionRecord {
  return decodeProjectionRecord(record(bytes(value), { streamId: stream }));
}
export function decodedItem(value: ItemWire = item(), stream = STREAM): ProjectedItem {
  const result = decoded(value, stream);
  if (result.type !== "item_projection") throw new Error("synthetic item expected");
  return result;
}
export function reject(action: () => unknown, code: ViewDecodeErrorCode): ViewDecodeError {
  let error: unknown;
  try { action(); } catch (caught) { error = caught; }
  expect(error).toBeInstanceOf(ViewDecodeError);
  const known = error as ViewDecodeError;
  expect(known.name).toBe("ViewDecodeError");
  expect(known.code).toBe(code);
  expect(known.message).toBe(code);
  expect(Object.hasOwn(known, "cause")).toBe(false);
  for (const output of [String(known), JSON.stringify(known), known.stack ?? ""]) expect(output).not.toContain(CANARY);
  return known;
}
export function fixture(path: string): Uint8Array {
  return new Uint8Array(readFileSync(new URL(`./fixtures/rust-v7/${path}`, import.meta.url)));
}
