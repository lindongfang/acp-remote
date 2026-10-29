import { describe, expect, it } from "vitest";

describe("web test harness", () => {
  it("runs real assertions", () => {
    expect("acp-remote").toMatch(/^acp-/);
  });
});
