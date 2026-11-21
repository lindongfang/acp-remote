import { expect, it } from "vitest";
import { decodeViewFrame, VIEW_CHUNK_BYTES, VIEW_HEADER_BYTES, viewStreamBytes } from "../src/views/binary";
import { ViewContentStore } from "../src/views/content-store";
import { ViewRecordAssembler } from "../src/views/records";
import { createStructuredTimeline } from "../src/views/timeline";
import { FakeDocument } from "./support/fake-dom";
import { evidence, item, ref, SEGMENT, STREAM } from "./support/timeline-fixtures";

it("renders the exact off-page field across credit chunks without exposing an original record", () => {
  const store = new ViewContentStore(STREAM);
  const assembler = new ViewRecordAssembler(STREAM);
  const output = "<script>原样</script>".repeat(2400);
  const token = JSON.stringify(output);
  const original = '{"unrelated":"页外保密","rawOutput":' + token + '}';
  const raw = ref(55n, original, token);
  const value = new TextEncoder().encode(token);
  const body = new Uint8Array(8 + value.length);
  new DataView(body.buffer).setBigUint64(0, BigInt(raw.byteStart));
  body.set(value, 8);
  let sequence = 0n;
  for (let offset = 0; offset < body.length; offset += VIEW_CHUNK_BYTES) {
    const chunk = body.subarray(offset, offset + VIEW_CHUNK_BYTES);
    const wire = new Uint8Array(VIEW_HEADER_BYTES + chunk.length);
    const header = new DataView(wire.buffer);
    wire.set([65, 67, 80, 86, 1, 1]);
    header.setUint16(6, Number(offset + chunk.length === body.length));
    wire.set(viewStreamBytes(STREAM), 8);
    header.setBigUint64(24, ++sequence);
    header.setBigUint64(32, 55n);
    header.setBigUint64(40, BigInt(offset));
    header.setBigUint64(48, BigInt(body.length));
    wire[56] = 6;
    header.setUint32(64, chunk.length);
    wire.set(chunk, VIEW_HEADER_BYTES);
    const frame = decodeViewFrame(wire);
    if (frame.type !== "data") throw new Error("expected data");
    const completed = assembler.accept(frame);
    if (frame.last) {
      expect(completed).not.toBeNull();
      expect(store.accept(completed!)).toBe(true);
      expect(store.accept(completed!)).toBe(false);
    } else {
      expect(completed).toBeNull();
    }
  }
  store.accept(item(0n, 3n, 5n, 55n, { rawOutput: { knowledge: "known", raw } }));
  const projected = store.items(SEGMENT)[0]!;
  expect(store.rawDocument(projected)).toBeNull();
  const field = store.field(projected, "rawOutput");
  expect(field.referencesReady).toBe(true);
  expect(field.primary?.value?.document.tokenLexeme(0)).toBe(token);
  const timeline = createStructuredTimeline(new FakeDocument() as unknown as Document, store, SEGMENT);
  expect(timeline.element.textContent).toContain(output);
  expect(timeline.element.textContent).not.toContain("页外保密");
});
it("resolves each evidence token by its exact range and rejects conflicting bytes", () => {
  const store = new ViewContentStore(STREAM);
  const source = '{"first":"\\u0061","second":"b","unrelated":0}';
  const first = ref(9n, source, '"\\u0061"');
  const second = ref(9n, source, '"b"', true);
  const accept = (start: string, token: string) => {
    const value = new TextEncoder().encode(token);
    const bytes = new Uint8Array(8 + value.length);
    new DataView(bytes.buffer).setBigUint64(0, BigInt(start));
    bytes.set(value, 8);
    return store.accept({ kind: "field_bytes", streamId: STREAM, recordId: 9n,
      length: BigInt(bytes.length), bytes: () => bytes });
  };
  store.accept(item(0n, 1n, 2n, 9n, { content: { knowledge: "known", raw: second, evidenceCount: "2" } }));
  store.accept(evidence(0n, 2n, 0n, first));
  store.accept(evidence(0n, 2n, 1n, second));
  accept(second.byteStart, '"b"');
  const material = () => store.field(store.items(SEGMENT)[0]!, "content");
  expect(material().referencesReady).toBe(false);
  expect(() => accept(first.byteStart, ' 0')).toThrow();
  accept(first.byteStart, '"\\u0061"');
  expect(() => accept(second.byteStart, '"x"')).toThrow();
  expect(material().referencesReady).toBe(true);
  expect(material().evidence.map(({ value }) => value?.document.tokenLexeme(0))).toEqual(['"\\u0061"', '"b"']);
});
