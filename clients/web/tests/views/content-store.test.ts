import { describe, expect, it } from "vitest";
import { ViewContentStore } from "../../src/views/content-store";
import { ViewDecodeError } from "../../src/views/errors";
import { STREAM, SEGMENT, record, ref, item, evidence } from "../support/timeline-fixtures";

function current(store: ViewContentStore) {
  const value = store.items(SEGMENT)[0];
  if (value === undefined) throw new Error("Missing fixture item");
  return value;
}
function code(action: () => unknown, expected: string): void {
  let error: unknown;
  try { action(); } catch (caught: unknown) { error = caught; }
  expect(error).toBeInstanceOf(ViewDecodeError);
  expect((error as ViewDecodeError).code).toBe(expected);
}
describe("owned display content", () => {
  it("resolves a delayed exact UTF-8 value while preserving original number and key bytes", () => {
    const store = new ViewContentStore(STREAM);
    const raw = '{"z":1234567890123456789012345678901234567890,"a":"中文🦀","a":null}';
    store.accept(item(0n, 1n, 1n, 1n, { content: { knowledge: "known", raw: ref(1n, raw, '"中文🦀"') } }));
    const before = store.field(current(store), "content");
    expect(before.referencesReady).toBe(false);
    expect(before.primary?.value).toBeNull();
    expect(store.accept(record("original_raw", 1n, raw))).toBe(true);
    const after = store.field(current(store), "content");
    expect(after.referencesReady).toBe(true);
    const value = after.primary?.value;
    expect(value?.document.stringValue(value.token)).toBe("中文🦀");
    expect(store.rawDocument(current(store))?.tokenLexeme(0)).toBe(raw);
    expect(before.primary?.value).toBeNull();
    expect(store.accept(record("original_raw", 1n, raw))).toBe(false);
    code(() => store.accept(record("original_raw", 1n, "{}")), "invalid_record");
    expect(store.rawDocument(current(store))?.tokenLexeme(0)).toBe(raw);
  });
  it("retains sparse evidence without allocating by a u64 count and separates revisions", () => {
    const store = new ViewContentStore(STREAM);
    const raw = '["one","two"]';
    const one = ref(1n, raw, '"one"', true), two = ref(1n, raw, '"two"', true);
    store.accept(record("original_raw", 1n, raw));
    store.accept(item(0n, 1n, 2n, 1n, { content: { knowledge: "partial", raw: two, evidenceCount: "2" } }));
    const old = current(store);
    store.accept(evidence(0n, 2n, 1n, two));
    expect(store.field(old, "content").referencesReady).toBe(false);
    store.accept(evidence(0n, 2n, 0n, one));
    const complete = store.field(old, "content");
    expect(complete.evidence.map((value) => value.index)).toEqual([0n, 1n]);
    expect(complete.referencesReady).toBe(true);
    expect(complete.knowledge).toBe("partial");
    expect(store.accept(evidence(0n, 2n, 0n, one))).toBe(false);
    code(() => store.accept(evidence(0n, 2n, 0n, two)), "invalid_record");
    store.accept(item(0n, 1n, 3n, 1n, { content: { evidenceCount: "18446744073709551615" } }));
    expect(store.field(current(store), "content").evidence).toEqual([]);
    expect(store.field(current(store), "content").referencesReady).toBe(false);
    expect(store.field(old, "content").referencesReady).toBe(true);
  });
  it("rejects non-value spans, conflicting last evidence and out-of-range evidence", () => {
    const raw = '{"key":"value"}';
    for (const literal of ['"key"', 'value']) {
      const store = new ViewContentStore(STREAM);
      store.accept(record("original_raw", 1n, raw));
      store.accept(item(0n, 1n, 1n, 1n, { content: { raw: ref(1n, raw, literal) } }));
      code(() => store.field(current(store), "content"), "invalid_record");
    }
    const store = new ViewContentStore(STREAM);
    store.accept(record("original_raw", 1n, '["a","b"]'));
    const a = ref(1n, '["a","b"]', '"a"'), b = ref(1n, '["a","b"]', '"b"');
    store.accept(item(0n, 1n, 1n, 1n, { content: { raw: a, evidenceCount: "1" } }));
    store.accept(evidence(0n, 1n, 0n, b));
    code(() => store.field(current(store), "content"), "invalid_record");
    store.accept(item(0n, 1n, 2n, 1n));
    store.accept(evidence(0n, 2n, 0n, b));
    code(() => store.field(current(store), "content"), "invalid_record");
  });
  it("reads the exact latest whole raw record even when a revision changes no fields", () => {
    const store = new ViewContentStore(STREAM);
    store.accept(record("original_raw", 1n, '{"old":1}'));
    store.accept(item(0n, 1n, 1n, 1n));
    const old = current(store), oldDocument = store.rawDocument(old);
    store.accept(item(0n, 1n, 2n, 2n));
    expect(store.rawDocument(current(store))).toBeNull();
    store.accept(record("original_raw", 2n, '{"toolCallId":"only-id"}'));
    expect(store.rawDocument(current(store))?.tokenLexeme(0)).toContain("only-id");
    expect(store.rawDocument(old)).toBe(oldDocument);
    const foreign = new ViewContentStore("Dw4NDAsKCQgHBgUEAwIBAA");
    code(() => foreign.rawDocument(old), "invalid_stream");
    code(() => store.rawDocument({ ...old }), "invalid_record");
    store.dispose(); store.dispose();
    code(() => store.rawDocument(old), "sealed");
    code(() => store.accept(record("original_raw", 3n, "{}")), "sealed");
    expect(oldDocument?.tokenLexeme(0)).toBe('{"old":1}');
  });
  it("takes ownership of raw input and rejects caller faults without leaking their details", () => {
    const store = new ViewContentStore(STREAM);
    const bytes = new TextEncoder().encode('{"safe":true}');
    store.accept({ kind: "original_raw", streamId: STREAM, recordId: 1n, length: BigInt(bytes.length), bytes: () => bytes });
    bytes.fill(0);
    store.accept(item(0n, 1n, 1n, 1n));
    expect(store.rawDocument(current(store))?.tokenLexeme(0)).toBe('{"safe":true}');
    const bad = record("original_raw", 2n, "{}");
    code(() => store.accept(new Proxy(bad, { get() { throw new Error("secret canary"); } })), "invalid_record");
    expect(store.rawDocument(current(store))?.tokenLexeme(0)).toBe('{"safe":true}');
  });
});
