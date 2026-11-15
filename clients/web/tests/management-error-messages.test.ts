import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { ManagementRequestError } from "../src/management/client";
import type { ManagementErrorCode } from "../src/management/protocol";
import { managementRequestErrorMessage } from "../src/management/error-messages";
import { MANAGEMENT_ERROR_CASES } from "./error-contract-vectors";

describe("management request error messages", () => {
  it.each(MANAGEMENT_ERROR_CASES)("displays the fixed neutral message for %s", (code, message) => {
    expect(managementRequestErrorMessage(code as ManagementErrorCode)).toBe(message);
  });

  it("delegates the actual main request-error branch to the pure message helper", () => {
    const main = new TextDecoder().decode(readFileSync(new URL("../src/main.ts", import.meta.url)));
    expect(main).toContain('import { managementRequestErrorMessage } from "./management/error-messages";');
    const start = main.indexOf("function managementErrorMessage(error: unknown): string {");
    const end = main.indexOf("  if (error instanceof ManagementConnectionError)", start);
    expect(start).toBeGreaterThan(0);
    expect(end).toBeGreaterThan(start);
    const branch = main.slice(start + "function managementErrorMessage(error: unknown): string {".length, end);
    const actualBranch = new Function("error", "ManagementRequestError", "managementRequestErrorMessage", branch + '\nthrow new Error("request branch did not return");');
    for (const [code] of MANAGEMENT_ERROR_CASES) {
      const delegated = (actualCode: string): string => `delegated:${actualCode}`;
      expect(actualBranch(new ManagementRequestError(code as ManagementErrorCode), ManagementRequestError, delegated)).toBe(`delegated:${code}`);
    }
  });
});
