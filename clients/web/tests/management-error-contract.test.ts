import { describe, expect, it } from "vitest";
import { ManagementProtocolError, parseManagementFrame } from "../src/management/protocol";
import { encodeBase64url } from "../src/protocol/jws";
import { MANAGEMENT_ERROR_CASES } from "./error-contract-vectors";

function failure(code: unknown): Record<string, unknown> {
  return {
    v: 1, type: "result", requestId: encodeBase64url(new Uint8Array(16).fill(1)),
    ok: false, result: null, error: { code },
  };
}

describe("C50 management error contract", () => {
  it("fixes fifty distinct client codes and keeps rate_limited separate from P49", () => {
    const codes = MANAGEMENT_ERROR_CASES.map(([code]) => code);
    expect(codes).toHaveLength(50);
    expect(new Set(codes).size).toBe(50);
    expect(codes.filter((code) => code !== "rate_limited")).toHaveLength(49);
    expect(codes).not.toContain("revoked");
  });

  it.each(MANAGEMENT_ERROR_CASES)("accepts top-level business code %s", (code) => {
    expect(parseManagementFrame(JSON.stringify(failure(code)))).toEqual(failure(code));
  });

  it.each([
    "unknown_future_error", "", "INVALID_REQUEST", "invalid_request ", " invalid_request",
    "revoked", null, true, 42, {}, [],
  ])("rejects unknown or malformed code %#", (code) => {
    expect(() => parseManagementFrame(JSON.stringify(failure(code)))).toThrow(ManagementProtocolError);
  });

  it.each([
    { ...failure("invalid_item"), unexpected: true },
    { ...failure("invalid_item"), error: { code: "invalid_item", detail: "do not expose" } },
    { ...failure("invalid_item"), error: {} },
    { ...failure("invalid_item"), ok: true },
    { ...failure("invalid_item"), result: {} },
    { ...failure("invalid_item"), error: null },
    { ...failure("invalid_item"), requestId: "invalid" },
    { ...failure("invalid_item"), v: 2 },
  ])("preserves closed envelope shape %#", (frame) => {
    expect(() => parseManagementFrame(JSON.stringify(frame))).toThrow(ManagementProtocolError);
  });

  it("does not interpret a successful payload's nested error as a top-level code", () => {
    const frame = { ...failure("invalid_request"), ok: true, error: null,
      result: { phase: "failed", error: "not_a_top_level_code" } };
    expect(parseManagementFrame(JSON.stringify(frame))).toEqual(frame);
  });
});
