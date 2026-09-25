//! 管理信封（channel `0x01`）的值对象与编解码（`docs/LOCAL_ADMIN_PROTOCOL.md` §4、§1.1）。
//!
//! ```text
//! 请求  { "v": 1, "id": "<uuid>", "method": "<name>", "params": { … } }
//! 成功  { "v": 1, "id": "<same uuid>", "ok": true,  "result": { … } }
//! 失败  { "v": 1, "id": "<same uuid>", "ok": false, "error": { "code": "<code>", "message": "<text>" } }
//! ```
//!
//! - 三个分支都是 **closed object**：未知字段不接受（§1.1）。`params` 是明确的开放容器：字段形状以文档 §5
//!   为权威，本层只要求它是 object（无参数方法必须发 `{}`，不用 `null`）。
//! - `result` 是开放容器：客户端必须忽略未知字段（§1.1），因此解码不拒绝未知的 `result` 键。
//! - `v != 1`（含缺失/类型不符）→ [`RequestDecodeError::ChannelVersionUnsupported`]：调用方必须**关闭连接**
//!   且不返回错误帧（§4 规则 1）。
//! - 信封非法（`id`/`method` 缺失或命名非法、`params` 不是 object、未知字段）→ `local.invalid_request`；
//!   语法合法但不在 v1 方法集里的 `method` → `local.unsupported`（§4 规则 3、规则 4）。
//!
//! 数字保真：workspace 为 `serde_json` 开启 `arbitrary_precision`，因此 `params`/`result` 里超出 u64/i64
//! 范围的整数字面量按原文保留（30 位整数的往返由测试固定）。

use serde::Serialize;
use serde_json::{Map, Value};

use crate::local_admin::error::{AdminError, LocalErrorCode};
use crate::local_admin::method::{Method, is_method_name};

/// 通道版本：v1 只接受 `1`（§4 规则 1、§8）。
pub const CHANNEL_VERSION: u64 = 1;

/// 开放的 JSON 对象容器（`params`/`result` 的类型）。
pub type JsonObject = Map<String, Value>;

const FIELD_V: &str = "v";
const FIELD_ID: &str = "id";
const FIELD_METHOD: &str = "method";
const FIELD_PARAMS: &str = "params";
const FIELD_OK: &str = "ok";
const FIELD_RESULT: &str = "result";
const FIELD_ERROR: &str = "error";
const FIELD_CODE: &str = "code";
const FIELD_MESSAGE: &str = "message";

/// 请求标识：canonical 小写 UUID 文本（§1.1、schema 的 `uuid`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestId(String);

impl RequestId {
    /// 全零 UUID：信封的 `id` 缺失或非法时，`local.invalid_request` 响应没有可回带的关联值，
    /// 而 schema 要求响应必须带 UUID。nil UUID 是「无关联」哨兵，客户端按未知 `id` 处理即可。
    pub const NIL: &'static str = "00000000-0000-0000-0000-000000000000";

    /// 解析 canonical 小写 UUID 文本；其他形态（大写、无连字符、带括号、URN）一律拒绝。
    pub fn parse(text: &str) -> Option<Self> {
        let parsed = uuid::Uuid::parse_str(text).ok()?;
        if parsed.hyphenated().to_string() == text {
            Some(Self(text.to_string()))
        } else {
            None
        }
    }

    /// 无关联哨兵（见 [`RequestId::NIL`]）。
    pub fn nil() -> Self {
        Self(Self::NIL.to_string())
    }

    /// 文本形式。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 一条已通过信封校验的管理请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminRequest {
    id: RequestId,
    method: Method,
    params: JsonObject,
}

impl AdminRequest {
    /// 请求标识。
    pub fn id(&self) -> &RequestId {
        &self.id
    }

    /// 方法（v1 方法集内的枚举值）。
    pub fn method(&self) -> Method {
        self.method
    }

    /// `params`（开放容器；形状校验属方法层）。
    pub fn params(&self) -> &JsonObject {
        &self.params
    }
}

/// 响应结果分支。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminOutcome {
    /// `ok = true`：`result` 是开放容器（无返回值的方法返回 `{}`）。
    Success {
        /// 方法返回值。
        result: JsonObject,
    },
    /// `ok = false`：只含 `code` 与 `message`。
    Failure {
        /// 错误对象。
        error: AdminError,
    },
}

/// 恰好一个响应（§4 规则 5）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminResponse {
    id: RequestId,
    outcome: AdminOutcome,
}

impl AdminResponse {
    /// 成功响应。
    pub fn success(id: RequestId, result: JsonObject) -> Self {
        Self {
            id,
            outcome: AdminOutcome::Success { result },
        }
    }

    /// 失败响应。
    pub fn failure(id: RequestId, error: AdminError) -> Self {
        Self {
            id,
            outcome: AdminOutcome::Failure { error },
        }
    }

    /// 响应回带的请求标识。
    pub fn id(&self) -> &RequestId {
        &self.id
    }

    /// 成功或失败分支。
    pub fn outcome(&self) -> &AdminOutcome {
        &self.outcome
    }

    /// 编码成一条信封 JSON（不含 framing；channel `0x01` 的字节由传输层加）。
    pub fn encode(&self) -> Result<Vec<u8>, EnvelopeEncodeError> {
        let bytes = match &self.outcome {
            AdminOutcome::Success { result } => {
                let envelope = SuccessEnvelope {
                    v: CHANNEL_VERSION,
                    id: self.id.as_str(),
                    ok: true,
                    result,
                };
                serde_json::to_vec(&envelope)
            }
            AdminOutcome::Failure { error } => {
                let envelope = FailureEnvelope {
                    v: CHANNEL_VERSION,
                    id: self.id.as_str(),
                    ok: false,
                    error: ErrorBody {
                        code: error.code().as_str(),
                        message: error.message(),
                    },
                };
                serde_json::to_vec(&envelope)
            }
        };
        bytes.map_err(EnvelopeEncodeError::Serde)
    }

    /// 解码一条响应信封（CLI 侧使用；同一批类型不产生第二份 wire DTO）。
    pub fn decode(payload: &[u8]) -> Result<Self, ResponseDecodeError> {
        decode_response(payload)
    }
}

/// 成功信封的序列化形状。
#[derive(Serialize)]
struct SuccessEnvelope<'a> {
    v: u64,
    id: &'a str,
    ok: bool,
    result: &'a JsonObject,
}

/// 失败信封的序列化形状。
#[derive(Serialize)]
struct FailureEnvelope<'a> {
    v: u64,
    id: &'a str,
    ok: bool,
    error: ErrorBody<'a>,
}

/// `error` 对象的序列化形状。
#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}

/// 信封编码失败（正常类型不可序列化时才会发生，属实现缺陷）。
#[derive(Debug, thiserror::Error)]
pub enum EnvelopeEncodeError {
    /// 序列化失败。
    #[error("管理信封序列化失败：{0}")]
    Serde(#[source] serde_json::Error),
}

/// 信封非法或版本不可用的具体原因（§4 规则 4、§1.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidRequestReason {
    /// 载荷不是合法 JSON（不允许尾随内容）。
    PayloadNotJson,
    /// 载荷不是 JSON object。
    NotAnObject,
    /// 出现未知字段（closed object）。
    UnknownField,
    /// 缺少必需字段，或该字段类型不符。
    MissingField {
        /// 字段名（`id` / `method` / `params`）。
        field: &'static str,
    },
    /// `id` 不是 canonical 小写 UUID。
    IdNotCanonicalUuid,
    /// `method` 不匹配方法名语法（§4 的表格）。
    MethodNameInvalid,
    /// `params` 不是 object（无参数方法必须发 `{}`）。
    ParamsNotObject,
}

impl InvalidRequestReason {
    /// 结构化日志字段里的稳定名字。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PayloadNotJson => "payload_not_json",
            Self::NotAnObject => "not_an_object",
            Self::UnknownField => "unknown_field",
            Self::MissingField { .. } => "missing_or_illtyped_field",
            Self::IdNotCanonicalUuid => "id_not_canonical_uuid",
            Self::MethodNameInvalid => "method_name_invalid",
            Self::ParamsNotObject => "params_not_object",
        }
    }

    /// `error.message` 的内容：简短英文，不含客户端给的任何字符串。
    pub fn message(self) -> &'static str {
        match self {
            Self::PayloadNotJson => "request frame is not valid JSON",
            Self::NotAnObject => "request envelope is not a JSON object",
            Self::UnknownField => "request envelope contains an unknown field",
            Self::MissingField {
                field: FIELD_ID, ..
            } => "request field id is missing or not a string",
            Self::MissingField {
                field: FIELD_METHOD,
                ..
            } => "request field method is missing or not a string",
            Self::MissingField { .. } => "request field params is missing",
            Self::IdNotCanonicalUuid => "request field id is not a canonical lowercase UUID",
            Self::MethodNameInvalid => "request field method is not a valid method name",
            Self::ParamsNotObject => "request field params is not an object",
        }
    }
}

/// 请求信封解码失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestDecodeError {
    /// `v` 不是 1（含缺失或类型不符）：连接级关闭，不返回错误帧（§4 规则 1）。
    ChannelVersionUnsupported,
    /// 信封非法 → `local.invalid_request`（§4 规则 4）。
    InvalidRequest {
        /// 响应要回带的 `id`（无法解析时为 [`RequestId::nil`]）。
        id: RequestId,
        /// 非法原因。
        reason: InvalidRequestReason,
    },
    /// `method` 语法合法但不在 v1 方法集 → `local.unsupported`（§4 规则 3）。
    UnsupportedMethod {
        /// 响应要回带的 `id`。
        id: RequestId,
        /// 对端发来的方法名（只用于日志与诊断，不进入响应）。
        method: String,
    },
}

impl RequestDecodeError {
    /// 结构化日志字段里的稳定名字。
    pub fn log_reason(&self) -> &'static str {
        match self {
            Self::ChannelVersionUnsupported => "channel_version_unsupported",
            Self::InvalidRequest { reason, .. } => reason.as_str(),
            Self::UnsupportedMethod { .. } => "unsupported_method",
        }
    }

    /// 传输层要执行的动作：关闭连接，或回一个响应（§4 规则 1/3/4）。
    pub fn into_outcome(self) -> RequestDecodeOutcome {
        match self {
            Self::ChannelVersionUnsupported => RequestDecodeOutcome::CloseConnection,
            Self::InvalidRequest { id, reason } => {
                RequestDecodeOutcome::Respond(AdminResponse::failure(
                    id,
                    AdminError::new(LocalErrorCode::InvalidRequest, reason.message()),
                ))
            }
            Self::UnsupportedMethod { id, method } => RequestDecodeOutcome::Respond(
                AdminResponse::failure(id, AdminError::unknown_method(&method)),
            ),
        }
    }
}

/// [`RequestDecodeError::into_outcome`] 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestDecodeOutcome {
    /// 关闭连接且不发送任何帧。
    CloseConnection,
    /// 回一个响应并保持连接可用。
    Respond(AdminResponse),
}

/// 响应信封解码失败（客户端侧；同一批约束由 schema 表达）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseDecodeError {
    /// 载荷不是合法 JSON。
    PayloadNotJson,
    /// 载荷不是 JSON object。
    NotAnObject,
    /// `v` 不是 1。
    ChannelVersionUnsupported,
    /// `id` 缺失或不是 canonical 小写 UUID。
    IdNotCanonicalUuid,
    /// `ok` 缺失或不是 boolean。
    OkNotBoolean,
    /// 出现未知顶层字段（closed object）。
    UnknownField,
    /// `ok = true` 但 `result` 缺失或不是 object。
    ResultInvalid,
    /// `ok = false` 但 `error` 缺失或不是 object。
    ErrorInvalid,
    /// `error` 内部出现未知字段。
    ErrorUnknownField,
    /// `error.code` 不是 §6 的错误码。
    ErrorCodeUnknown,
    /// `error.message` 缺失、非 string 或长度不在 `1..=512` 内。
    ErrorMessageInvalid,
}

/// 解码一条请求信封（channel `0x01` 的载荷）。
///
/// 校验顺序：JSON 与 object 形状 → `v`（连接级）→ closed object → `id` → `method`（语法，再判集内）→ `params`。
pub fn decode_request(payload: &[u8]) -> Result<AdminRequest, RequestDecodeError> {
    let value: Value =
        serde_json::from_slice(payload).map_err(|_| RequestDecodeError::InvalidRequest {
            id: RequestId::nil(),
            reason: InvalidRequestReason::PayloadNotJson,
        })?;
    let Value::Object(map) = value else {
        return Err(RequestDecodeError::InvalidRequest {
            id: RequestId::nil(),
            reason: InvalidRequestReason::NotAnObject,
        });
    };

    // §4 规则 1：版本不可判定时不能用错误帧回答。
    match map.get(FIELD_V) {
        Some(Value::Number(number)) if number.as_u64() == Some(CHANNEL_VERSION) => {}
        _ => return Err(RequestDecodeError::ChannelVersionUnsupported),
    }

    // closed object（§1.1）：未知字段不接受。放在版本判断之后——未来版本的字段名不能反过来改变连接级行为。
    if map.keys().any(|key| {
        !matches!(
            key.as_str(),
            FIELD_V | FIELD_ID | FIELD_METHOD | FIELD_PARAMS
        )
    }) {
        return Err(RequestDecodeError::InvalidRequest {
            id: request_id_or_nil(&map),
            reason: InvalidRequestReason::UnknownField,
        });
    }

    let id = match map
        .get(FIELD_ID)
        .and_then(Value::as_str)
        .and_then(RequestId::parse)
    {
        Some(id) => id,
        None => {
            return Err(RequestDecodeError::InvalidRequest {
                id: RequestId::nil(),
                reason: InvalidRequestReason::IdNotCanonicalUuid,
            });
        }
    };

    let method_text = match map.get(FIELD_METHOD).and_then(Value::as_str) {
        Some(text) => text,
        None => {
            return Err(RequestDecodeError::InvalidRequest {
                id,
                reason: InvalidRequestReason::MissingField {
                    field: FIELD_METHOD,
                },
            });
        }
    };
    if !is_method_name(method_text) {
        return Err(RequestDecodeError::InvalidRequest {
            id,
            reason: InvalidRequestReason::MethodNameInvalid,
        });
    }
    let method = match Method::from_name(method_text) {
        Some(method) => method,
        None => {
            return Err(RequestDecodeError::UnsupportedMethod {
                id,
                method: method_text.to_string(),
            });
        }
    };

    let params = match map.get(FIELD_PARAMS) {
        Some(Value::Object(params)) => params.clone(),
        _ => {
            return Err(RequestDecodeError::InvalidRequest {
                id,
                reason: InvalidRequestReason::ParamsNotObject,
            });
        }
    };

    Ok(AdminRequest { id, method, params })
}

/// 解码一条响应信封。
pub fn decode_response(payload: &[u8]) -> Result<AdminResponse, ResponseDecodeError> {
    let value: Value =
        serde_json::from_slice(payload).map_err(|_| ResponseDecodeError::PayloadNotJson)?;
    let Value::Object(map) = value else {
        return Err(ResponseDecodeError::NotAnObject);
    };
    match map.get(FIELD_V) {
        Some(Value::Number(number)) if number.as_u64() == Some(CHANNEL_VERSION) => {}
        _ => return Err(ResponseDecodeError::ChannelVersionUnsupported),
    }
    let id = match map
        .get(FIELD_ID)
        .and_then(Value::as_str)
        .and_then(RequestId::parse)
    {
        Some(id) => id,
        None => return Err(ResponseDecodeError::IdNotCanonicalUuid),
    };
    let ok = match map.get(FIELD_OK) {
        Some(Value::Bool(ok)) => *ok,
        _ => return Err(ResponseDecodeError::OkNotBoolean),
    };

    if ok {
        if map.contains_key(FIELD_ERROR) {
            return Err(ResponseDecodeError::UnknownField);
        }
        let result = match map.get(FIELD_RESULT) {
            Some(Value::Object(result)) => result.clone(),
            _ => return Err(ResponseDecodeError::ResultInvalid),
        };
        return Ok(AdminResponse::success(id, result));
    }

    if map.contains_key(FIELD_RESULT) {
        return Err(ResponseDecodeError::UnknownField);
    }
    let error = match map.get(FIELD_ERROR) {
        Some(Value::Object(error)) => error,
        _ => return Err(ResponseDecodeError::ErrorInvalid),
    };
    if error
        .keys()
        .any(|key| !matches!(key.as_str(), FIELD_CODE | FIELD_MESSAGE))
    {
        return Err(ResponseDecodeError::ErrorUnknownField);
    }
    let code = match error
        .get(FIELD_CODE)
        .and_then(Value::as_str)
        .and_then(LocalErrorCode::parse)
    {
        Some(code) => code,
        None => return Err(ResponseDecodeError::ErrorCodeUnknown),
    };
    let message = match error.get(FIELD_MESSAGE).and_then(Value::as_str) {
        Some(message) if (1..=512).contains(&message.chars().count()) => message,
        _ => return Err(ResponseDecodeError::ErrorMessageInvalid),
    };
    Ok(AdminResponse::failure(id, AdminError::new(code, message)))
}

/// 信封里能解析出 `id` 就回带它，否则用无关联哨兵。
fn request_id_or_nil(map: &JsonObject) -> RequestId {
    map.get(FIELD_ID)
        .and_then(Value::as_str)
        .and_then(RequestId::parse)
        .unwrap_or_else(RequestId::nil)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes_of(body: &str) -> Vec<u8> {
        body.as_bytes().to_vec()
    }

    #[test]
    fn request_id_accepts_only_canonical_lowercase_uuid() {
        assert!(RequestId::parse("5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10").is_some());
        assert!(RequestId::parse(RequestId::NIL).is_some());
        assert!(RequestId::parse("5B1F0C2E-8A4D-4B6F-9C31-0D2A7E5F4B10").is_none());
        assert!(RequestId::parse("5b1f0c2e8a4d4b6f9c310d2a7e5f4b10").is_none());
        assert!(RequestId::parse("{5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10}").is_none());
        assert!(RequestId::parse("urn:uuid:5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10").is_none());
        assert!(RequestId::parse("").is_none());
    }

    #[test]
    fn version_other_than_one_closes_the_connection() {
        let payload = bytes_of(
            r#"{"v":2,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"daemon.status","params":{}}"#,
        );
        assert_eq!(
            decode_request(&payload),
            Err(RequestDecodeError::ChannelVersionUnsupported)
        );
        // v 缺失或类型不符同样无法判定对端语义。
        let missing = bytes_of(
            r#"{"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"daemon.status","params":{}}"#,
        );
        assert_eq!(
            decode_request(&missing),
            Err(RequestDecodeError::ChannelVersionUnsupported)
        );
    }

    #[test]
    fn envelope_is_a_closed_object() {
        let payload = bytes_of(
            r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"daemon.status","params":{},"extra":1}"#,
        );
        assert_eq!(
            decode_request(&payload),
            Err(RequestDecodeError::InvalidRequest {
                id: RequestId::parse("5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10").expect("uuid"),
                reason: InvalidRequestReason::UnknownField,
            })
        );
    }

    #[test]
    fn params_must_be_an_object() {
        for payload in [
            r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"daemon.status","params":null}"#,
            r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"daemon.status","params":[]}"#,
            r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"daemon.status"}"#,
        ] {
            assert!(matches!(
                decode_request(&bytes_of(payload)),
                Err(RequestDecodeError::InvalidRequest {
                    reason: InvalidRequestReason::ParamsNotObject,
                    ..
                })
            ));
        }
    }

    #[test]
    fn params_keep_integer_fidelity() {
        let payload = bytes_of(
            r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"agent.configure","params":{"n":123456789012345678901234567890}}"#,
        );
        let request = decode_request(&payload).expect("合法请求");
        let encoded = serde_json::to_string(request.params()).expect("params 可序列化");
        assert!(
            encoded.contains("123456789012345678901234567890"),
            "params 必须按字面保留超范围整数，实际 {encoded}"
        );
    }

    #[test]
    fn syntactically_valid_names_outside_the_set_are_unsupported() {
        let payload = bytes_of(
            r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"daemon.doctor","params":{}}"#,
        );
        match decode_request(&payload) {
            Err(RequestDecodeError::UnsupportedMethod { method, .. }) => {
                assert_eq!(method, "daemon.doctor");
            }
            other => panic!("语法合法但不在 v1 方法集里应回 local.unsupported，实际 {other:?}"),
        }

        // §4 的方法名语法不允许连字符，因此连字符名属「命名非法」→ `local.invalid_request`。
        // 这与文档 §5.7 把 `node.rotate-key.begin` 写成「返回 local.unsupported」存在措辞不一致
        // （该名字不匹配 §4 表格里的正则），已在交付报告中登记给文档所有者。
        let payload = bytes_of(
            r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"node.rotate-key.begin","params":{}}"#,
        );
        assert!(matches!(
            decode_request(&payload),
            Err(RequestDecodeError::InvalidRequest {
                reason: InvalidRequestReason::MethodNameInvalid,
                ..
            })
        ));
    }

    #[test]
    fn response_round_trips_and_ignores_unknown_result_fields() {
        let response = AdminResponse::success(
            RequestId::parse("5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10").expect("uuid"),
            JsonObject::new(),
        );
        let bytes = response.encode().expect("可编码");
        assert_eq!(AdminResponse::decode(&bytes).expect("可解码"), response);

        // `result` 是开放容器：客户端必须忽略未知字段（§1.1）。
        let payload = bytes_of(
            r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","ok":true,"result":{"unknown":{"a":1}}}"#,
        );
        let decoded = AdminResponse::decode(&payload).expect("未知 result 字段可读");
        assert!(
            matches!(decoded.outcome(), AdminOutcome::Success { result } if result.contains_key("unknown"))
        );
    }

    #[test]
    fn response_decoder_rejects_schema_violations() {
        let cases = [
            (
                r#"{"v":2,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","ok":true,"result":{}}"#,
                ResponseDecodeError::ChannelVersionUnsupported,
            ),
            (
                r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","ok":true}"#,
                ResponseDecodeError::ResultInvalid,
            ),
            (
                r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","ok":false,"error":{"code":"local.whatever","message":"x"}}"#,
                ResponseDecodeError::ErrorCodeUnknown,
            ),
            (
                r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","ok":false,"error":{"code":"local.internal"}}"#,
                ResponseDecodeError::ErrorMessageInvalid,
            ),
            (
                r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","ok":false,"error":{"code":"local.internal","message":""}}"#,
                ResponseDecodeError::ErrorMessageInvalid,
            ),
            (
                r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","ok":true,"result":{},"error":{"code":"local.internal","message":"x"}}"#,
                ResponseDecodeError::UnknownField,
            ),
            (
                r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","ok":"true","result":{}}"#,
                ResponseDecodeError::OkNotBoolean,
            ),
            (
                r#"{"v":1,"id":"not-a-uuid","ok":true,"result":{}}"#,
                ResponseDecodeError::IdNotCanonicalUuid,
            ),
            (r#"[]"#, ResponseDecodeError::NotAnObject),
            (r#"not json"#, ResponseDecodeError::PayloadNotJson),
        ];
        for (payload, expected) in cases {
            assert_eq!(
                AdminResponse::decode(&bytes_of(payload)),
                Err(expected),
                "{payload}"
            );
        }
    }

    #[test]
    fn error_responses_only_carry_code_and_message() {
        let response = AdminResponse::failure(
            RequestId::parse("5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10").expect("uuid"),
            AdminError::unknown_method("daemon.doctor"),
        );
        let text = String::from_utf8(response.encode().expect("可编码")).expect("UTF-8");
        assert_eq!(
            text,
            r#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","ok":false,"error":{"code":"local.unsupported","message":"method daemon.doctor is not part of the v1 local method set"}}"#
        );
    }
}
