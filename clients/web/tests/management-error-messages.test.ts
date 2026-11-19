import { describe, expect, it } from "vitest";
import {
  ManagementConnectionError,
  ManagementRequestError,
  SessionConsumeOutcomeError,
} from "../src/management/client";
import { managementRequestErrorMessage } from "../src/management/error-messages";
import type { ManagementErrorCode } from "../src/management/protocol";
import { managementErrorMessage } from "../src/workbench/error-formatting";
import { MANAGEMENT_ERROR_CASES } from "./error-contract-vectors";

describe("management request error messages", () => {
  it.each(MANAGEMENT_ERROR_CASES)(
    "displays the fixed neutral message for %s",
    (code, message) => {
      expect(managementRequestErrorMessage(code as ManagementErrorCode)).toBe(
        message,
      );
    },
  );

  it("delegates ManagementRequestError to managementRequestErrorMessage pure helper", () => {
    for (const [code, expected] of MANAGEMENT_ERROR_CASES) {
      const error = new ManagementRequestError(code as ManagementErrorCode);
      expect(managementErrorMessage(error)).toBe(expected);
    }
  });

  it("formats connection, protocol, and consume outcome errors", () => {
    expect(
      managementErrorMessage(new ManagementConnectionError("not_connected")),
    ).toBe("管理连接尚未完成身份认证。");
    expect(
      managementErrorMessage(
        new ManagementConnectionError("connection_closed", 4401),
      ),
    ).toBe(
      "节点不认可这个客户端身份。它可能已被撤销，或本地身份记录已经失效。",
    );
    expect(
      managementErrorMessage(
        new SessionConsumeOutcomeError("consume_pending", "wait"),
      ),
    ).toBe("请求仍在节点确认中，尚未完成结算；请刷新队列或重试。");
    expect(
      managementErrorMessage(
        new SessionConsumeOutcomeError("consume_refused", "invalid_target"),
      ),
    ).toBe("请求被节点拒绝（invalid_target）。");
  });
});
