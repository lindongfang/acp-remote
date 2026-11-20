import { describe, expect, it } from "vitest";
import {
  ManagementConnectionError,
  ManagementRequestError,
  SessionConsumeOutcomeError,
} from "../src/management/client";
import { managementRequestErrorMessage } from "../src/management/error-messages";
import {
  ManagementProtocolError,
  type ManagementErrorCode,
} from "../src/management/protocol";
import { SessionInputActionError } from "../src/management/session-input-actions";
import { PairingApiError } from "../src/pairing/api";
import { IdentityStoreError } from "../src/pairing/identity";
import { PairingSessionError } from "../src/pairing/session";
import { PairingProtocolError } from "../src/protocol/pairing";
import {
  managementErrorMessage,
  pairingErrorMessage,
  sessionActionErrorMessage,
} from "../src/workbench/error-formatting";
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
      managementErrorMessage(new ManagementConnectionError("request_timeout")),
    ).toBe("管理请求超时，结果未确认。重新连接后可读取列表核实。");
    expect(
      managementErrorMessage(new ManagementConnectionError("revoked")),
    ).toBe("节点已撤销这个客户端身份并关闭连接。");
    expect(
      managementErrorMessage(
        new ManagementConnectionError("connection_closed", 1000),
      ),
    ).toBe("管理连接已断开。");
    expect(
      managementErrorMessage(
        new ManagementConnectionError("connection_closed", 1001),
      ),
    ).toBe("节点正在停止，管理连接已关闭。");
    expect(
      managementErrorMessage(
        new ManagementConnectionError("connection_closed", 4400),
      ),
    ).toBe("管理帧格式或协议版本不一致，连接已关闭。");
    expect(
      managementErrorMessage(
        new ManagementConnectionError("connection_closed", 4401),
      ),
    ).toBe("节点不认可这个客户端身份。它可能已被撤销，或本地身份记录已经失效。");
    expect(
      managementErrorMessage(
        new ManagementConnectionError("connection_closed", 4408),
      ),
    ).toBe("管理身份认证超时，连接已关闭。");
    expect(
      managementErrorMessage(
        new ManagementConnectionError("connection_closed", 4429),
      ),
    ).toBe("管理连接或请求达到节点上限，请稍后重新连接。");
    expect(
      managementErrorMessage(
        new ManagementConnectionError("connection_closed", 4500),
      ),
    ).toBe("节点内部存储不可用，管理连接已关闭。");
    expect(
      managementErrorMessage(
        new ManagementConnectionError("connection_closed", 1006),
      ),
    ).toBe("管理连接已关闭，未确认最后一条操作结果。");

    expect(
      managementErrorMessage(new ManagementProtocolError("origin_mismatch")),
    ).toBe("节点声明的来源与当前 HTTPS 页面不一致，已停止认证。");
    expect(
      managementErrorMessage(new ManagementProtocolError("node_mismatch")),
    ).toBe("管理挑战中的节点公钥与已绑定节点不一致，已拒绝签署证明。");
    expect(
      managementErrorMessage(new ManagementProtocolError("host_proof_invalid")),
    ).toBe("节点挑战签名或挑战字段验证失败，未发送客户端证明。");
    expect(
      managementErrorMessage(new ManagementProtocolError("invalid_key")),
    ).toBe("管理挑战包含无效的节点公钥，已停止认证。");
    expect(
      managementErrorMessage(new ManagementProtocolError("invalid_frame")),
    ).toBe("节点返回的管理数据不符合已校验协议，连接已停止。");
    expect(
      managementErrorMessage(new ManagementProtocolError("invalid_response")),
    ).toBe("节点返回的管理数据不符合已校验协议，连接已停止。");

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

  it("formats pairing API, protocol, identity store, session, and fallback errors via pairingErrorMessage", () => {
    expect(
      pairingErrorMessage(new PairingApiError("pairing_invalid", 400, null)),
    ).toBe("配对码无效、已到期，或这次配对已经不能继续。请在节点重新运行 device pair。");
    expect(
      pairingErrorMessage(new PairingApiError("request_conflict", 409, null)),
    ).toBe("同一个认领请求标识对应了不同证明，节点已拒绝。请重新开始配对。");
    expect(
      pairingErrorMessage(new PairingApiError("rate_limited", 429, 15)),
    ).toBe("请求过于频繁，请 15 秒后重试。");
    expect(
      pairingErrorMessage(new PairingApiError("rate_limited", 429, null)),
    ).toBe("请求过于频繁，请 60 秒后重试。");
    expect(
      pairingErrorMessage(new PairingApiError("storage_unavailable", 503, null)),
    ).toBe("节点存储暂时不可用，未确认任何批准结果。");
    expect(
      pairingErrorMessage(new PairingApiError("node_stopping", 503, null)),
    ).toBe("节点正在停止，未提交新的配对状态。");
    expect(
      pairingErrorMessage(new PairingApiError("unsupported_version", 400, null)),
    ).toBe("节点与页面的接入协议版本不一致。");
    expect(
      pairingErrorMessage(new PairingApiError("invalid_request", 400, null)),
    ).toBe("节点拒绝了请求格式。请刷新页面后重新开始。");

    expect(
      pairingErrorMessage(new PairingProtocolError("invalid_pairing_code")),
    ).toBe("配对码格式不正确。请输入 8 位 Crockford 字符，可带一个中间连字符。");
    expect(
      pairingErrorMessage(new PairingProtocolError("invalid_display_name")),
    ).toBe("名称需要 1 到 80 个 Unicode 字符，且不能包含控制字符。");
    expect(
      pairingErrorMessage(new PairingProtocolError("host_proof_invalid")),
    ).toBe("节点返回的签名证明验证失败，未更新本地状态。");
    expect(
      pairingErrorMessage(new PairingProtocolError("invalid_response")),
    ).toBe("节点返回的数据不符合接入合同，已停止处理。");

    expect(
      pairingErrorMessage(new IdentityStoreError("identity_changed")),
    ).toBe("其他页面已更新浏览器身份，正在重新读取当前身份。");
    expect(
      pairingErrorMessage(new IdentityStoreError("key_unreadable")),
    ).toContain("浏览器保存的密钥无法读回或签验失败。");
    expect(
      pairingErrorMessage(new IdentityStoreError("storage_unavailable")),
    ).toContain("同源 IndexedDB 不可用");

    expect(
      pairingErrorMessage(new PairingSessionError("already_bound")),
    ).toBe("这个浏览器身份已经绑定到节点，不能重复认领。");
    expect(
      pairingErrorMessage(new PairingSessionError("client_mismatch")),
    ).toBe("节点返回的客户端 ID 与本地固定身份不一致，已停止管理认证。");

    expect(pairingErrorMessage(new TypeError("network failed"))).toBe(
      "配对未完成。请检查节点状态后重试。",
    );
    expect(pairingErrorMessage(new DOMException("aborted", "AbortError"))).toBe(
      "配对未完成。请检查节点状态后重试。",
    );
    expect(pairingErrorMessage("non-error-value")).toBe(
      "配对未完成。请检查节点状态后重试。",
    );
  });

  it("formats session input action errors and delegates unknown errors via sessionActionErrorMessage", () => {
    expect(
      sessionActionErrorMessage(new SessionInputActionError("empty_input")),
    ).toBe("正文不能为空。");
    expect(
      sessionActionErrorMessage(new SessionInputActionError("upload_failed")),
    ).toBe("上传准备失败，已尝试释放暂存正文。");
    expect(
      sessionActionErrorMessage(
        new SessionInputActionError("preparation_timeout"),
      ),
    ).toBe("正文准备超时，没有登记队列项。");
    expect(
      sessionActionErrorMessage(new SessionInputActionError("consume_pending")),
    ).toBe("请求仍在节点确认中，尚未完成结算；请刷新队列核验状态。");
    expect(
      sessionActionErrorMessage(
        new SessionInputActionError("consume_refused", "queue_full"),
      ),
    ).toBe("请求被节点拒绝（queue_full）。");
    expect(
      sessionActionErrorMessage(
        new SessionInputActionError("consume_refused", null),
      ),
    ).toBe("请求被节点拒绝。");

    expect(
      sessionActionErrorMessage(
        new ManagementProtocolError("host_proof_invalid"),
      ),
    ).toBe("节点挑战签名或挑战字段验证失败，未发送客户端证明。");
  });
});
