import { describe, expect, it } from "vitest";
import { ViewRecordAssembler } from "../../src/views/records";
import { STREAM, frame, reject } from "./support";

describe("SV-068 local WC-IMPL-ALLOCATION-001 classification", () => {
  it("R13 caller getter RangeError is invalid_record, private, and seals fresh or partial state", () => {
    const codes: string[] = [];
    for (const partial of [false, true]) {
      const assembler = new ViewRecordAssembler(STREAM);
      if (partial) expect(assembler.accept(frame({ total: 2n }))).toBeNull();
      const canary = "LOCAL-synthetic-range-canary-https://invalid.test/private";
      let reads = 0;
      const supplied = Object.defineProperty({}, "type", {
        get() { ++reads; throw new RangeError(canary); },
      });
      const error = reject(() => assembler.accept(supplied as ReturnType<typeof frame>));
      expect(reads).toBe(1);
      expect(String(error)).not.toContain(canary);
      expect(JSON.stringify(error)).not.toContain(canary);
      expect(Object.hasOwn(error, "stack") ? error.stack : "").not.toContain(canary);
      codes.push(error.code);
      reject(() => assembler.accept(frame({ packet: partial ? 2n : 1n, offset: partial ? 1n : 0n, total: partial ? 2n : 1n })), "sealed");
      assembler.seal();
      reject(() => assembler.accept(frame()), "sealed");
    }
    expect(codes).toEqual(["invalid_record", "invalid_record"]);
  });
});
