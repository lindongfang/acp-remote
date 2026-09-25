//! `local.*` 错误码与 `error` 对象（`docs/LOCAL_ADMIN_PROTOCOL.md` §4 规则 6、§6）。
//!
//! 错误码的机器定义在 `schemas/local-admin/v1/envelope.schema.json#/$defs/errorCode`，本枚举是它在 Rust 侧
//! 的唯一镜像，由常驻漂移测试逐项断言。本地错误码**不进入** `compatibility/errors/v1/errors.json`
//! （那份 registry 只登记 WSS wire 上的错误码）。

use crate::local_admin::method::Method;

/// `error.message` 允许的最大字符数（schema 的 `maxLength`）。
const MAX_MESSAGE_CHARS: usize = 512;

/// §6 的九个本地错误码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalErrorCode {
    /// `local.unsupported`：未知 `method`，或方法在当前 delivery 阶段尚未实现。
    Unsupported,
    /// `local.invalid_request`：信封非法（`id` 缺失/非 UUID/重复、`method` 缺失或命名非法、`params` 不是 object）。
    InvalidRequest,
    /// `local.invalid_params`：方法参数非法（缺字段、类型不符、越界或未知字段）。
    InvalidParams,
    /// `local.not_found`：目标不存在，或引用的 workspace alias 未建立。
    NotFound,
    /// `local.conflict`：目标存在但当前状态不允许该操作。
    Conflict,
    /// `local.expired`：`pairingId` 已过期或已被拒绝。
    Expired,
    /// `local.remote_error`：对 Owner 的 HTTPS 调用失败或返回错误。
    RemoteError,
    /// `local.unavailable`：Daemon 正在关闭、存储/keystore 不可用或依赖未就绪（可重试）。
    Unavailable,
    /// `local.internal`：未预期内部错误（可重试）。
    Internal,
}

impl LocalErrorCode {
    /// 全部错误码；漂移测试断言它与 schema 的 `errorCode.enum` 逐项相等。
    pub const ALL: [Self; 9] = [
        Self::Unsupported,
        Self::InvalidRequest,
        Self::InvalidParams,
        Self::NotFound,
        Self::Conflict,
        Self::Expired,
        Self::RemoteError,
        Self::Unavailable,
        Self::Internal,
    ];

    /// 错误的 wire 文本（§6 的表格）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "local.unsupported",
            Self::InvalidRequest => "local.invalid_request",
            Self::InvalidParams => "local.invalid_params",
            Self::NotFound => "local.not_found",
            Self::Conflict => "local.conflict",
            Self::Expired => "local.expired",
            Self::RemoteError => "local.remote_error",
            Self::Unavailable => "local.unavailable",
            Self::Internal => "local.internal",
        }
    }

    /// 由 wire 文本解析（客户端解码响应用）。
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|code| code.as_str() == text)
    }

    /// 该错误码的默认简短英文描述（`error.message` 兜底值）。
    pub fn default_message(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported method",
            Self::InvalidRequest => "invalid request envelope",
            Self::InvalidParams => "invalid method parameters",
            Self::NotFound => "target not found",
            Self::Conflict => "target state does not allow this operation",
            Self::Expired => "pairing has expired",
            Self::RemoteError => "remote owner reported an error",
            Self::Unavailable => "daemon dependency is not available",
            Self::Internal => "internal error",
        }
    }
}

/// `error` 对象：只含 `code` 与 `message`（§4 的表格）。
///
/// `message` 是简短英文描述，只用于本地日志与 CLI 输出：不得包含 secret、凭据值、堆栈或完整敏感路径
/// （§4 规则 6）。构造时把它收窄到 schema 允许的 `1..=512` 字符，因此实现缺陷不会产出 schema 非法的响应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminError {
    code: LocalErrorCode,
    message: String,
}

impl AdminError {
    /// 由错误码与消息构造（消息会 trim、截断到 512 字符，空消息回落到该码的默认描述）。
    pub fn new(code: LocalErrorCode, message: impl AsRef<str>) -> Self {
        Self {
            code,
            message: clamp_message(code, message.as_ref()),
        }
    }

    /// 错误码。
    pub fn code(&self) -> LocalErrorCode {
        self.code
    }

    /// 简短英文描述。
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 不在 v1 方法集里的方法名（§4 规则 3）。
    ///
    /// 消息里回显对端给的**方法名**（诊断必需：新 CLI 连上旧 Daemon 时要看出是哪个方法不被支持），
    /// 但这不是秘密、也不含 params；长度由 [`AdminError::new`] 收窄到 schema 允许的范围。
    pub fn unknown_method(method: &str) -> Self {
        Self::new(
            LocalErrorCode::Unsupported,
            format!("method {method} is not part of the v1 local method set"),
        )
    }

    /// v1 方法集内、但当前 delivery 阶段尚未实现的方法（§6）。
    pub fn unsupported_method(method: Method) -> Self {
        Self::new(
            LocalErrorCode::Unsupported,
            format!("method {} is not implemented yet", method.as_str()),
        )
    }

    /// 同一连接上未完成请求的 `id` 重复（§4 规则 2 → §6 的 `local.invalid_request`）。
    pub fn duplicate_request_id() -> Self {
        Self::new(
            LocalErrorCode::InvalidRequest,
            "duplicate request id on this connection",
        )
    }
}

/// 把消息收窄到 schema 允许的范围，并保证非空。
fn clamp_message(code: LocalErrorCode, message: &str) -> String {
    let head: String = message.trim().chars().take(MAX_MESSAGE_CHARS).collect();
    if head.is_empty() {
        code.default_message().to_string()
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_the_documented_text_and_have_default_messages() {
        assert_eq!(LocalErrorCode::ALL.len(), 9);
        for code in LocalErrorCode::ALL {
            assert!(code.as_str().starts_with("local."));
            assert_eq!(LocalErrorCode::parse(code.as_str()), Some(code));
            assert!(!code.default_message().is_empty());
        }
        assert_eq!(LocalErrorCode::Unsupported.as_str(), "local.unsupported");
        assert_eq!(LocalErrorCode::Internal.as_str(), "local.internal");
        assert_eq!(LocalErrorCode::parse("local.whatever"), None);
        assert_eq!(LocalErrorCode::parse("sync.invalid_request"), None);
    }

    #[test]
    fn message_is_clamped_to_the_schema_bounds() {
        let long = "x".repeat(600);
        let error = AdminError::new(LocalErrorCode::Internal, long);
        assert_eq!(error.message().chars().count(), MAX_MESSAGE_CHARS);
        assert_eq!(
            AdminError::new(LocalErrorCode::Internal, "   ").message(),
            LocalErrorCode::Internal.default_message()
        );
        assert_eq!(
            AdminError::new(LocalErrorCode::Internal, " short ").message(),
            "short"
        );
        // 回显方法名的错误同样被收窄（对端可能发一个很长的名字）。
        let huge = format!("m{}", "a".repeat(600));
        assert_eq!(
            AdminError::unknown_method(&huge).message().chars().count(),
            MAX_MESSAGE_CHARS
        );
        assert!(
            AdminError::unsupported_method(Method::DaemonStatus)
                .message()
                .contains("daemon.status")
        );
    }
}
