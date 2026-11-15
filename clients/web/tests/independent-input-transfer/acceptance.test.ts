import { describe, expect, it } from "vitest";
import { INPUT_CHUNK_BYTES, INPUT_HEADER_BYTES, InputTransferError,
  decodeInputAcknowledgement, encodeInputData } from "../../src/management/input-transfer";

// Independent literal vectors; no developer test or product encoder is an oracle.
const IDS = [
  ["AAECAwQFBgcICQoLDA0ODw", "000102030405060708090a0b0c0d0e0f"],
  ["gIGCg4SFhoeIiYqLjI2Ojw", "808182838485868788898a8b8c8d8e8f"],
  ["AAAAAAAAAAAAAAAAAAAAAA", "00000000000000000000000000000000"],
  ["_____________________w", "ffffffffffffffffffffffffffffffff"],
] as const;
const ID = "AAECAwQFBgcICQoLDA0ODw";
const RAW = "000102030405060708090a0b0c0d0e0f";
const MAX = 0xffff_ffff_ffff_ffffn;
const FIXED = 0x0102_0304_0506_0708n;
const ACK = "4143504901020000000102030405060708090a0b0c0d0e0f0102030405060708";
const DATA = "4143504901010000000102030405060708090a0b0c0d0e0f010203040506070800ff80";
const CANARY = "SYNTHETIC_PRIVATE_BODY_2b2c6f";
const CODES = new Set(["invalid_header", "invalid_direction", "invalid_length", "invalid_upload_id", "invalid_offset", "allocation_failed"]);
type Code = InputTransferError["code"];

function hex(text: string): Uint8Array {
  const bytes = new Uint8Array(text.length / 2);
  for (let i = 0; i < bytes.length; ++i) bytes[i] = Number.parseInt(text.slice(i * 2, i * 2 + 2), 16);
  return bytes;
}
function same(actual: Uint8Array, wanted: Uint8Array): void {
  expect(actual).toBeInstanceOf(Uint8Array);
  expect(actual.length).toBe(wanted.length);
  for (let i = 0; i < wanted.length; ++i) {
    if (actual[i] !== wanted[i]) throw new Error("independent byte mismatch at " + i);
  }
}
function header(kind: number, raw: string, offset: bigint): Uint8Array {
  const out = new Uint8Array(32);
  out.set([65, 67, 80, 73, 1, kind, 0, 0]); out.set(hex(raw), 8);
  // Only bounded individual bytes become Number; full u64 never does.
  let left = offset;
  for (let i = 31; i >= 24; --i) { out[i] = Number(left & 255n); left >>= 8n; }
  return out;
}
function data(raw: string, offset: bigint, payload: Uint8Array): Uint8Array {
  const out = new Uint8Array(32 + payload.length);
  out.set(header(1, raw, offset)); out.set(payload, 32); return out;
}
function reject(run: () => unknown, code?: Code): void {
  let caught: unknown;
  try { run(); } catch (error) { caught = error; }
  expect(caught).toBeInstanceOf(InputTransferError);
  if (!(caught instanceof InputTransferError)) throw new Error("expected actual typed codec error");
  expect(CODES.has(caught.code)).toBe(true);
  if (code !== undefined) expect(caught.code).toBe(code);
  expect(Object.hasOwn(caught, "cause")).toBe(false);
  for (const key of Object.getOwnPropertyNames(caught)) {
    expect(["name", "message", "code", "stack"]).toContain(key);
    const descriptor = Object.getOwnPropertyDescriptor(caught, key);
    expect(descriptor).toBeDefined();
    if (descriptor !== undefined && "value" in descriptor) {
      expect(typeof descriptor.value).toBe("string");
      expect(String(descriptor.value)).not.toContain(CANARY);
      expect(String(descriptor.value)).not.toContain(ID);
    }
  }
  expect(Object.getOwnPropertySymbols(caught)).toHaveLength(0);
  for (const text of [caught.name, caught.message, caught.code, String(caught), JSON.stringify(caught)]) {
    expect(text).not.toContain(CANARY); expect(text).not.toContain(ID);
  }
}
function pureAck(ack: ReturnType<typeof decodeInputAcknowledgement>): void {
  expect(Object.isFrozen(ack)).toBe(true);
  expect(Object.keys(ack).sort()).toEqual(["nextOffset", "uploadId"]);
  expect(Object.getOwnPropertySymbols(ack)).toHaveLength(0);
  expect(typeof ack.uploadId).toBe("string"); expect(typeof ack.nextOffset).toBe("bigint");
}

describe("independent E1W ACPI codec acceptance", () => {
  it("EWI01 fixed literal Data and Ack vectors", () => {
    expect(INPUT_HEADER_BYTES).toBe(32); expect(INPUT_CHUNK_BYTES).toBe(16_384);
    same(encodeInputData(ID, FIXED, new Uint8Array([0, 255, 128])), hex(DATA));
    const wire = hex(ACK); const before = wire.slice();
    const ack = decodeInputAcknowledgement(wire);
    expect(ack).toEqual({ uploadId: ID, nextOffset: FIXED }); pureAck(ack); same(wire, before);
  });
  it("EWI01 four canonical IDs, full u64 and nonzero Ack byteOffset", () => {
    const offsets = [0n, 1n, FIXED, (1n << 53n) - 1n, 1n << 53n, (1n << 53n) + 1n, MAX];
    for (const [id, raw] of IDS) for (const offset of offsets) {
      for (const size of [1, 16_384]) {
        const payload = new Uint8Array(size).fill(255);
        same(encodeInputData(id, offset, payload), data(raw, offset, payload));
      }
      const outer = new Uint8Array(49).fill(0xa5); outer.set(header(2, raw, offset), 7);
      const before = outer.slice(); const ack = decodeInputAcknowledgement(outer.subarray(7, 39));
      expect(ack).toEqual({ uploadId: id, nextOffset: offset }); pureAck(ack); same(outer, before);
    }
  });
  it("EWI02 malformed headers and known-kind then direction then length", () => {
    const good = hex(ACK);
    for (let size = 0; size < 32; ++size) reject(() => decodeInputAcknowledgement(good.subarray(0, size)));
    for (let i = 0; i < 4; ++i) {
      const bad = good.slice(); bad[i] = 0;
      reject(() => decodeInputAcknowledgement(bad), "invalid_header");
    }
    for (const [at, value] of [[3,86],[4,0],[4,2],[4,255],[5,0],[5,3],[5,255],[6,1],[6,255],[7,1],[7,255]] as const) {
      const bad = good.slice(); bad[at] = value;
      reject(() => decodeInputAcknowledgement(bad), "invalid_header");
    }
    for (const size of [32, 33, 64]) {
      const wire = new Uint8Array(size); wire.set(header(1, RAW, 0n));
      reject(() => decodeInputAcknowledgement(wire), "invalid_direction");
      for (const kind of [0, 3, 255]) {
        wire[5] = kind; reject(() => decodeInputAcknowledgement(wire), "invalid_header");
      }
    }
    const extra = new Uint8Array(33); extra.set(good);
    reject(() => decodeInputAcknowledgement(extra), "invalid_length");
    reject(() => decodeInputAcknowledgement(hex(DATA)), "invalid_direction");
    const secret = new Uint8Array(32 + CANARY.length); secret.set(good);
    secret.set(new TextEncoder().encode(CANARY), 32);
    reject(() => decodeInputAcknowledgement(secret), "invalid_length");
  });
  it("EWI02 runtime wrong types, canonical ID and bigint-only offset errors", () => {
    const good = hex(ACK);
    const badWires: unknown[] = [good.buffer, new DataView(good.buffer), [...good], null, undefined, CANARY, 32, { length: 32 }];
    for (const value of badWires) reject(() => decodeInputAcknowledgement(value as Uint8Array));
    const badIds: unknown[] = ["", "A".repeat(21), "A".repeat(23), ID + "==", "!".repeat(22), "+".repeat(21)+"w", "/".repeat(21)+"w", "AAAAAAAAAAAAAAAAAAAAAB", CANARY, 22, null, undefined, new Uint8Array(16)];
    for (const id of badIds) reject(() => encodeInputData(id as string, 0n, new Uint8Array([1])), "invalid_upload_id");
    const badOffsets: unknown[] = [0, 1, -1n, MAX + 1n, 0.25, Number.NaN, Number.POSITIVE_INFINITY, "0", null, undefined];
    for (const offset of badOffsets) reject(() => encodeInputData(ID, offset as bigint, new Uint8Array([1])), "invalid_offset");
    const badPayloads: unknown[] = [new ArrayBuffer(1), new DataView(new ArrayBuffer(1)), [1], null, undefined, CANARY];
    for (const payload of badPayloads) reject(() => encodeInputData(ID, 0n, payload as Uint8Array));
  });
  it("EWI03 exact chunk bounds and opaque split UTF8 bytes", () => {
    for (const size of [1, 16_384]) {
      const payload = new Uint8Array(size);
      for (let i = 0; i < size; ++i) payload[i] = i % 256;
      same(encodeInputData(ID, 0n, payload), data(RAW, 0n, payload));
    }
    for (const size of [0, 16_385]) reject(() => encodeInputData(ID, 0n, new Uint8Array(size)), "invalid_length");
    for (const payload of [new Uint8Array([0, 255, 128, 240]), new Uint8Array([159, 166, 128])]) {
      same(encodeInputData(ID, MAX, payload), data(RAW, MAX, payload));
    }
  });
  it("EWI03 selected subarray owns no shared mutable bytes and Ack is a snapshot", () => {
    for (const size of [1, 16_384]) {
      const source = new Uint8Array(size + 11).fill(0xa7); const payload = source.subarray(5, 5 + size);
      for (let i = 0; i < size; ++i) payload[i] = (i * 29 + 3) % 256;
      const before = source.slice(); const expected = data(RAW, 1n, payload);
      const first = encodeInputData(ID, 1n, payload); const second = encodeInputData(ID, 1n, payload);
      same(first, expected); same(second, expected); same(source, before);
      payload.fill(0x51); same(first, expected); same(second, expected);
      first.fill(0x23); same(second, expected); same(payload, new Uint8Array(size).fill(0x51));
      same(source.subarray(0, 5), before.subarray(0, 5)); same(source.subarray(5 + size), before.subarray(5 + size));
      const reverseSource = before.slice();
      const reverse = encodeInputData(ID, 1n, reverseSource.subarray(5, 5 + size)); reverse.fill(0);
      same(reverseSource, before);
    }
    const wire = hex(ACK); const ack = decodeInputAcknowledgement(wire); wire.fill(0);
    expect(ack).toEqual({ uploadId: ID, nextOffset: FIXED }); pureAck(ack);
  });
  it("EWI04 actual 33554433 bytes and 2049 packets", () => {
    const total = 32 * 1024 * 1024 + 1; const original = new Uint8Array(total);
    for (let i = 0; i < total; ++i) original[i] = (i * 131 + 17) % 256;
    original.set([240,159,166,128], 16_383);
    const reconstructed = new Uint8Array(total); // Test-only, no product total-body cache.
    let position = 0; let packets = 0; let last = 0;
    while (position < total) {
      const payload = original.subarray(position, Math.min(position + 16_384, total));
      const frame = encodeInputData(ID, BigInt(position), payload);
      same(frame.subarray(0, 32), header(1, RAW, BigInt(position))); same(frame.subarray(32), payload);
      reconstructed.set(frame.subarray(32), position); position += payload.length; last = payload.length; ++packets;
    }
    expect(position).toBe(33_554_433); expect(packets).toBe(2_049); expect(last).toBe(1);
    same(reconstructed, original);
    console.info("EWI04 actual bytes=33554433 packets=2049 lastPayload=1; codec-only");
  });
  it("EWI04 actual 4097 calls without an accumulated limit", () => {
    let calls = 0;
    for (let i = 0; i < 4_097; ++i) {
      const pair = IDS[i % IDS.length]; if (pair === undefined) throw new Error("missing test ID");
      const [id, raw] = pair; const payload = new Uint8Array([i % 251]);
      same(encodeInputData(id, BigInt(i), payload), data(raw, BigInt(i), payload)); ++calls;
    }
    expect(calls).toBe(4_097); console.info("EWI04 actual calls=4097; no socket or upload admission");
  });
});
