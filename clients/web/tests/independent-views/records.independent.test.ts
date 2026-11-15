import { describe, expect, it } from "vitest";
import { ViewRecordAssembler, type CompletedViewRecord } from "../../src/views/records";
import { RawJsonDocument } from "../../src/views/raw-json";
import { OTHER, STREAM, MAX, encoder, frame, reject, sha256 } from "./support";

function complete(value: CompletedViewRecord | null): CompletedViewRecord {
  expect(value).not.toBeNull();
  return value as CompletedViewRecord;
}
describe("SV-068 local record bytes only", () => {
  it("R01 four-byte UTF8 split at every boundary remains exact and parses after completion", () => {
    const original = encoder.encode('{"v":"A💩B\\u0061","n":12345678901234567890123456789}');
    const escape = original.indexOf(92);
    for (const split of [7, 8, 9, 10, escape + 1, escape + 3, original.length - 10]) {
      const assembler = new ViewRecordAssembler(STREAM);
      expect(assembler.accept(frame({ payload: original.slice(0, split), total: BigInt(original.length) }))).toBeNull();
      const record = complete(assembler.accept(frame({ packet: 2n, offset: BigInt(split), total: BigInt(original.length), payload: original.slice(split) })));
      expect(record.bytes()).toEqual(original);
      const doc = RawJsonDocument.parse(record.bytes());
      const strings = doc.tokens.flatMap((token, index) => token.kind === "string" && token.role === "value" ? [doc.stringValue(index)] : []);
      expect(strings).toEqual(["A💩Ba"]);
      expect(doc.tokenLexeme(4)).toBe("12345678901234567890123456789");
    }
  });
  it("R02 accepted bytes and completed copies have independent ownership", () => {
    const assembler = new ViewRecordAssembler(STREAM);
    const first = frame({ payload: Uint8Array.of(1, 2), total: 4n });
    expect(assembler.accept(first)).toBeNull(); first.payload.fill(9);
    const second = frame({ packet: 2n, payload: Uint8Array.of(3, 4), offset: 2n, total: 4n });
    const record = complete(assembler.accept(second)); second.payload.fill(8);
    expect(record.bytes()).toEqual(Uint8Array.of(1, 2, 3, 4));
    const returned = record.bytes(); returned.fill(0);
    expect(record.bytes()).toEqual(Uint8Array.of(1, 2, 3, 4));
    expect(Object.isFrozen(record)).toBe(true);
    expect(Reflect.set(record, "length", 100n)).toBe(false);
  });
  it.each([0n, 2n, 100n, MAX])("R03 initial packet %s fails and seals", (packet) => {
    const assembler = new ViewRecordAssembler(STREAM);
    const candidate = packet === 0n ? { ...frame(), packetSequence: 0n } : frame({ packet });
    reject(() => assembler.accept(candidate));
    reject(() => assembler.accept(frame()), "sealed");
  });
  it("R04 packet duplicate and gap fail even after a completed record", () => {
    for (const next of [1n, 3n]) {
      const assembler = new ViewRecordAssembler(STREAM);
      complete(assembler.accept(frame()));
      reject(() => assembler.accept(frame({ packet: next })), "invalid_sequence");
      reject(() => assembler.accept(frame({ packet: 2n })), "sealed");
    }
  });
  it.each(["record", "kind", "total", "overlap", "gap"])("R05 partial record cannot change %s", (attack) => {
    const assembler = new ViewRecordAssembler(STREAM);
    expect(assembler.accept(frame({ payload: Uint8Array.of(1), total: 3n }))).toBeNull();
    const options = { packet: 2n, record: attack === "record" ? 8n : 7n, kind: attack === "kind" ? 3 : 1, total: attack === "total" ? 4n : 3n, offset: attack === "overlap" ? 0n : attack === "gap" ? 2n : 1n, payload: Uint8Array.of(2) };
    reject(() => assembler.accept(frame(options)), "invalid_record");
    reject(() => assembler.accept(frame({ packet: 2n, total: 3n, offset: 1n, payload: Uint8Array.of(2) })), "sealed");
  });
  it("R06 initial offset and wrong stream seal; new stream needs a new instance", () => {
    const offset = new ViewRecordAssembler(STREAM);
    reject(() => offset.accept(frame({ offset: 1n, total: 2n })), "invalid_record");
    reject(() => offset.accept(frame()), "sealed");
    const wrong = new ViewRecordAssembler(OTHER);
    reject(() => wrong.accept(frame()), "invalid_stream");
    reject(() => wrong.accept(frame()), "sealed");
    const fresh = new ViewRecordAssembler(STREAM);
    expect(complete(fresh.accept(frame())).bytes()).toEqual(encoder.encode("x"));
  });
  it("R07 record namespaces do not impose a global increasing record ID", () => {
    const assembler = new ViewRecordAssembler(STREAM);
    for (const [index, id] of [100n, 1n, 100n].entries()) {
      const kind = index === 1 ? 3 : 1;
      const record = complete(assembler.accept(frame({ packet: BigInt(index + 1), record: id, kind })));
      expect(record.recordId).toBe(id);
      expect(record.kind).toBe(index === 1 ? "status" : "original_raw");
    }
  });
  it("R08 totalLength u64MAX does not preallocate body or truncate BigInt", () => {
    const assembler = new ViewRecordAssembler(STREAM);
    expect(assembler.accept(frame({ total: MAX, payload: Uint8Array.of(1) }))).toBeNull();
    expect(assembler.accept(frame({ packet: 2n, total: MAX, offset: 1n, payload: Uint8Array.of(2) }))).toBeNull();
    assembler.seal(); reject(() => assembler.accept(frame({ packet: 3n })), "sealed");
  });
  it("R09 32MiB-plus record has no total quota and bytes are complete", async () => {
    const length = 32 * 1024 * 1024 + 19;
    const assembler = new ViewRecordAssembler(STREAM);
    const expected = new Uint8Array(length);
    let result: CompletedViewRecord | null = null;
    let offset = 0; let packet = 1n;
    while (offset < length) {
      const chunk = new Uint8Array(Math.min(16_384, length - offset)).fill(Number(packet & 0xffn));
      expected.set(chunk, offset);
      result = assembler.accept(frame({ packet, offset: BigInt(offset), total: BigInt(length), payload: chunk }));
      offset += chunk.length; ++packet;
    }
    const record = complete(result);
    expect(record.length).toBe(BigInt(length));
    const first = record.bytes(); expect(first.length).toBe(length);
    expect(await sha256(first)).toBe(await sha256(expected));
    first.fill(0); expect(record.bytes()[16_384]).toBe(2);
  }, 30_000);
  it("R10 malformed runtime data and caller errors seal without leaking payload", () => {
    const assembler = new ViewRecordAssembler(STREAM);
    const canary = "REMOTE-secret-https://127.0.0.1/approval";
    const supplied = Object.defineProperty({}, "type", { get() { throw new Error(canary); } });
    const error = reject(() => assembler.accept(supplied as ReturnType<typeof frame>), "invalid_record");
    expect(String(error)).not.toContain(canary);
    reject(() => assembler.accept(frame()), "sealed");
  });
  it("R11 seal is permanent/idempotent and does not corrupt completed data", () => {
    for (const partial of [false, true]) {
      const assembler = new ViewRecordAssembler(STREAM);
      if (partial) expect(assembler.accept(frame({ total: 2n }))).toBeNull();
      assembler.seal(); assembler.seal();
      reject(() => assembler.accept(frame()), "sealed");
    }
    const assembler = new ViewRecordAssembler(STREAM);
    const record = complete(assembler.accept(frame({ kind: 4, payload: new Uint8Array() })));
    assembler.seal(); expect(record.bytes()).toEqual(new Uint8Array());
  });
  it("R12 two assemblers sharing a caller frame never share their owned chunks", () => {
    const first = new ViewRecordAssembler(STREAM); const second = new ViewRecordAssembler(STREAM);
    const shared = frame({ payload: Uint8Array.of(4, 5) });
    const a = complete(first.accept(shared)); const b = complete(second.accept(shared));
    shared.payload.fill(9); const copy = a.bytes(); copy.fill(8);
    expect(b.bytes()).toEqual(Uint8Array.of(4, 5));
  });
});
