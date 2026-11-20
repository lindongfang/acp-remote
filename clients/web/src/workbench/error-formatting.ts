import { managementRequestErrorMessage } from "../management/error-messages";
import {
  ManagementConnectionError,
  ManagementRequestError,
  SessionConsumeOutcomeError,
} from "../management/client";
import { ManagementProtocolError } from "../management/protocol";
import { SessionInputActionError } from "../management/session-input-actions";
import { PairingApiError } from "../pairing/api";
import { IdentityStoreError } from "../pairing/identity";
import { PairingSessionError } from "../pairing/session";
import { PairingProtocolError } from "../protocol/pairing";

export function pairingErrorMessage(error: unknown): string {
  if (error instanceof PairingApiError) {
    switch (error.code) {
      case "pairing_invalid":
        return "配对码无效、已到期，或这次配对已经不能继续。请在节点重新运行 device pair。";
      case "request_conflict":
        return "同一个认领请求标识对应了不同证明，节点已拒绝。请重新开始配对。";
      case "rate_limited":
        return `请求过于频繁，请 ${error.retryAfterSeconds ?? 60} 秒后重试。`;
      case "storage_unavailable":
        return "节点存储暂时不可用，未确认任何批准结果。";
      case "node_stopping":
        return "节点正在停止，未提交新的配对状态。";
      case "unsupported_version":
        return "节点与页面的接入协议版本不一致。";
      case "invalid_request":
        return "节点拒绝了请求格式。请刷新页面后重新开始。";
    }
  }
  if (error instanceof PairingProtocolError) {
    switch (error.code) {
      case "invalid_pairing_code":
        return "配对码格式不正确。请输入 8 位 Crockford 字符，可带一个中间连字符。";
      case "invalid_display_name":
        return "名称需要 1 到 80 个 Unicode 字符，且不能包含控制字符。";
      case "origin_mismatch":
        return "节点声明的来源与当前 HTTPS 页面不一致，已停止。";
      case "node_mismatch":
        return "节点公钥与此前固定的公钥不一致。不会自动信任替换后的节点。";
      case "host_proof_invalid":
        return "节点返回的签名证明验证失败，未更新本地状态。";
      case "crypto_unavailable":
        return "当前浏览器没有可用的 WebCrypto，无法安全配对。";
      case "invalid_key":
      case "invalid_response":
        return "节点返回的数据不符合接入合同，已停止处理。";
    }
  }
  if (error instanceof IdentityStoreError) {
    switch (error.code) {
      case "identity_changed":
        return "其他页面已更新浏览器身份，正在重新读取当前身份。";
      case "key_unreadable":
      case "key_self_check_failed":
      case "corrupt_record":
        return "浏览器保存的密钥无法读回或签验失败。本页没有生成替代密钥；请先在节点撤销旧设备，再清除本站点数据后重新配对。";
      case "storage_unavailable":
        return "同源 IndexedDB 不可用，无法保存或读取浏览器身份，因此不会发起认领。";
      case "write_failed":
        return "浏览器身份无法写入 IndexedDB，认领未继续。";
      case "origin_mismatch":
        return "保存身份时使用的 HTTPS 来源与当前页面不一致，不会迁移或替换密钥。";
      case "crypto_unavailable":
        return "当前浏览器没有可用的 WebCrypto。";
    }
  }
  if (error instanceof PairingSessionError) {
    switch (error.code) {
      case "already_bound":
        return "这个浏览器身份已经绑定到节点，不能重复认领。";
      case "node_mismatch":
        return "节点公钥与此前固定的公钥不一致，已拒绝继续。";
      case "client_mismatch":
        return "节点返回的客户端 ID 与本地固定身份不一致，已停止管理认证。";
      case "no_pending_claim":
        return "当前页面没有可重试的认领证明，请重新输入配对码。";
      case "pairing_not_pending":
        return "这次配对不在等待批准的状态。";
      case "crypto_unavailable":
        return "当前浏览器没有可用的 WebCrypto。";
    }
  }
  return "配对未完成。请检查节点状态后重试。";
}

export function managementErrorMessage(error: unknown): string {
  if (error instanceof ManagementRequestError) {
    return managementRequestErrorMessage(error.code);
  }
  if (error instanceof ManagementConnectionError) {
    switch (error.code) {
      case "not_connected":
        return "管理连接尚未完成身份认证。";
      case "request_timeout":
        return "管理请求超时，结果未确认。重新连接后可读取列表核实。";
      case "revoked":
        return "节点已撤销这个客户端身份并关闭连接。";
      case "connection_closed":
        switch (error.closeCode) {
          case 1000:
            return "管理连接已断开。";
          case 1001:
            return "节点正在停止，管理连接已关闭。";
          case 4400:
            return "管理帧格式或协议版本不一致，连接已关闭。";
          case 4401:
            return "节点不认可这个客户端身份。它可能已被撤销，或本地身份记录已经失效。";
          case 4408:
            return "管理身份认证超时，连接已关闭。";
          case 4409:
          case 4429:
            return "管理连接或请求达到节点上限，请稍后重新连接。";
          case 4500:
            return "节点内部存储不可用，管理连接已关闭。";
          default:
            return "管理连接已关闭，未确认最后一条操作结果。";
        }
    }
  }
  if (error instanceof ManagementProtocolError) {
    switch (error.code) {
      case "origin_mismatch":
        return "节点声明的来源与当前 HTTPS 页面不一致，已停止认证。";
      case "node_mismatch":
        return "管理挑战中的节点公钥与已绑定节点不一致，已拒绝签署证明。";
      case "host_proof_invalid":
        return "节点挑战签名或挑战字段验证失败，未发送客户端证明。";
      case "invalid_key":
        return "管理挑战包含无效的节点公钥，已停止认证。";
      case "invalid_frame":
      case "invalid_response":
        return "节点返回的管理数据不符合已校验协议，连接已停止。";
    }
  }
  if (error instanceof SessionConsumeOutcomeError) {
    return error.code === "consume_pending"
      ? "请求仍在节点确认中，尚未完成结算；请刷新队列或重试。"
      : `请求被节点拒绝（${error.detail}）。`;
  }
  return pairingErrorMessage(error);
}

export function sessionActionErrorMessage(error: unknown): string {
  if (error instanceof SessionInputActionError) {
    switch (error.code) {
      case "empty_input":
        return "正文不能为空。";
      case "upload_failed":
        return "上传准备失败，已尝试释放暂存正文。";
      case "preparation_timeout":
        return "正文准备超时，没有登记队列项。";
      case "consume_pending":
        return "请求仍在节点确认中，尚未完成结算；请刷新队列核验状态。";
      case "consume_refused":
        return error.detail !== null
          ? `请求被节点拒绝（${error.detail}）。`
          : "请求被节点拒绝。";
    }
  }
  return managementErrorMessage(error);
}
