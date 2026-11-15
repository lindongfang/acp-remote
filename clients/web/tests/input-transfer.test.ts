import { describe, expect, it } from "vitest";
import {
  decodeInputAcknowledgement,
  encodeInputData,
  InputTransferError,
} from "../src/management/input-transfer";

const UPLOAD = "AAECAwQFBgcICQoLDA0ODw";
const hex = (value: string): Uint8Array =>
  Uint8Array.from(value.match(/../gu) ?? [], (byte) => Number.parseInt(byte, 16));
const ACK = hex("4143504901020000000102030405060708090a0b0c0d0e0f0102030405060708");

function failure(action: () => unknown, code: InputTransferError["code"]): void {
  expect(action).toThrowError(InputTransferError);
  try {
    action();
  } catch (error) {
    expect(error).toBeInstanceOf(InputTransferError);
    expect((error as InputTransferError).code).toBe(code);
    expect(String(error)).toBe("InputTransferError: " + code);
    expect(Object.hasOwn(error as object, "cause")).toBe(false);
  }
}

describe("input transfer codec", () => {
  it("matches literal binary vectors and keeps a full u64 offset", () => {
    const payload = Uint8Array.of(0, 255, 128);
    expect(encodeInputData(UPLOAD, 0x0102_0304_0506_0708n, payload)).toEqual(
      hex("4143504901010000000102030405060708090a0b0c0d0e0f010203040506070800ff80"),
    );
    const decoded = decodeInputAcknowledgement(ACK);
    expect(decoded).toEqual({ uploadId: UPLOAD, nextOffset: 0x0102_0304_0506_0708n });
    expect(Object.isFrozen(decoded)).toBe(true);
    const extreme = encodeInputData(UPLOAD, 0xffff_ffff_ffff_ffffn, payload);
    expect(new DataView(extreme.buffer).getBigUint64(24)).toBe(0xffff_ffff_ffff_ffffn);
  });

  it("uses the selected byte view and returns independent encoded storage", () => {
    const source = Uint8Array.of(91, 0, 255, 128, 92);
    const selected = source.subarray(1, 4);
    const encoded = encodeInputData(UPLOAD, 9_007_199_254_740_993n, selected);
    expect(source).toEqual(Uint8Array.of(91, 0, 255, 128, 92));
    expect(encoded.subarray(32)).toEqual(Uint8Array.of(0, 255, 128));
    selected[0] = 33;
    expect(encoded[32]).toBe(0);
    encoded[33] = 44;
    expect(selected[1]).toBe(255);
    const surrounding = new Uint8Array(40).fill(77);
    surrounding.set(ACK, 3);
    expect(decodeInputAcknowledgement(surrounding.subarray(3, 35))).toEqual(
      decodeInputAcknowledgement(ACK),
    );
  });

  it("checks the known kind before direction and acknowledgement length", () => {
    for (const size of [32, 33]) {
      const wire = new Uint8Array(size);
      wire.set(ACK);
      wire[5] = 1;
      failure(() => decodeInputAcknowledgement(wire), "invalid_direction");
      wire[5] = 3;
      failure(() => decodeInputAcknowledgement(wire), "invalid_header");
      if (size === 33) {
        wire[5] = 2;
        failure(() => decodeInputAcknowledgement(wire), "invalid_length");
      }
    }
  });

  it("rejects invalid ids, offsets and opaque chunk lengths with static errors", () => {
    for (const id of ["", "AAAAAAAAAAAAAAAAAAAAAB", "secret-canary-upload=="]) {
      failure(() => encodeInputData(id, 0n, Uint8Array.of(1)), "invalid_upload_id");
    }
    for (const offset of [-1n, 0x1_0000_0000_0000_0000n, 1 as unknown as bigint]) {
      failure(() => encodeInputData(UPLOAD, offset, Uint8Array.of(1)), "invalid_offset");
    }
    for (const size of [0, 16_385]) {
      failure(() => encodeInputData(UPLOAD, 0n, new Uint8Array(size)), "invalid_length");
    }
    expect(encodeInputData(UPLOAD, 0n, new Uint8Array(16_384)).length).toBe(16_416);
    failure(() => decodeInputAcknowledgement(ACK.subarray(1)), "invalid_header");
    failure(() => decodeInputAcknowledgement(null as unknown as Uint8Array), "invalid_header");
    failure(() => encodeInputData(UPLOAD, 0n, [] as unknown as Uint8Array), "invalid_length");
  });
});
