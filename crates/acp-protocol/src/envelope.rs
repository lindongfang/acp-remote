//! JSON-RPC 信封：类别（request/notification/response）与方向。
//!
//! 类别由结构决定：有 `method` 时看有没有 `id`（有 = request，无 = notification）；没有 `method` 时
//! 看有没有 `result` 或 `error`（有 = response）。方向由**方法名**决定，登记表在 [`crate::methods`]，
//! 与 `compatibility/acp/v1/matrix.json` 逐条对应。
//!
//! 这一层不做「上游快照里不存在的方法就报错」的判断：那是 [`MethodStatus`] 的职责，调用方通过
//! [`Envelope::ensure_direction`] 得到 `Unsupported`（显式不支持）而不是结构错误。

use serde_json::Value;

use crate::error::{AcpError, Result};
use crate::methods::{self, MethodDirection, MethodStatus};
use crate::raw::{IdLiteral, RawDocument};

/// JSON-RPC 消息类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageClass {
    /// 请求：有 `id`，需要响应。
    Request,
    /// 通知：没有 `id`。
    Notification,
    /// 响应：有 `id` 与 `result` 或 `error`。
    Response,
}

/// 一条已分类的消息。
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    document: RawDocument,
    class: MessageClass,
    method: Option<String>,
    id: Option<IdLiteral>,
}

impl Envelope {
    /// 分类一条原文消息。
    ///
    /// 校验：`jsonrpc` 必须存在且等于 `"2.0"`；`method` 存在时必须是字符串；`method` 不存在时必须
    /// 是带 `result` 或 `error` 的响应；`id` 存在时必须是合法 JSON 标量。
    pub fn classify(document: RawDocument) -> Result<Self> {
        let value = document.value()?;
        let object = value.as_object().ok_or_else(|| AcpError::Malformed {
            detail: "顶层不是 JSON 对象".to_owned(),
        })?;

        match object.get("jsonrpc") {
            None => {
                return Err(AcpError::MissingField {
                    field: "jsonrpc".to_owned(),
                });
            }
            Some(Value::String(version)) if version == "2.0" => {}
            Some(other) => {
                return Err(AcpError::InvalidField {
                    field: "jsonrpc".to_owned(),
                    detail: format!("只支持 \"2.0\"，收到 {other}"),
                });
            }
        }

        let id = match document.member_literal("id") {
            Some(literal) => Some(IdLiteral::from_literal(literal)?),
            None => None,
        };

        let method = match object.get("method") {
            Some(Value::String(name)) => Some(name.clone()),
            Some(other) => {
                return Err(AcpError::InvalidField {
                    field: "method".to_owned(),
                    detail: format!("必须是字符串，收到 {other}"),
                });
            }
            None => None,
        };

        let class = match &method {
            Some(_) if id.is_some() => MessageClass::Request,
            Some(_) => MessageClass::Notification,
            None => {
                let has_result = object.contains_key("result");
                let has_error = object.contains_key("error");
                if id.is_none() || (!has_result && !has_error) {
                    return Err(AcpError::Malformed {
                        detail: "既不是请求/通知（缺 method），也不是响应（缺 id 或 result/error）"
                            .to_owned(),
                    });
                }
                if has_result && has_error {
                    return Err(AcpError::Malformed {
                        detail: "响应同时带有 result 与 error".to_owned(),
                    });
                }
                MessageClass::Response
            }
        };

        Ok(Self {
            document,
            class,
            method,
            id,
        })
    }

    /// 原文。
    #[must_use]
    pub fn document(&self) -> &RawDocument {
        &self.document
    }

    /// 类别。
    #[must_use]
    pub fn class(&self) -> MessageClass {
        self.class
    }

    /// 方法名（响应没有方法名）。
    #[must_use]
    pub fn method(&self) -> Option<&str> {
        self.method.as_deref()
    }

    /// `id` 字面量。
    #[must_use]
    pub fn id(&self) -> Option<&IdLiteral> {
        self.id.as_ref()
    }

    /// 方法在本阶段的状态；响应没有方法名时返回 `None`。
    #[must_use]
    pub fn status(&self) -> Option<MethodStatus> {
        self.method.as_deref().map(methods::status_of)
    }

    /// 方法方向；未登记的方法返回 `None`。
    #[must_use]
    pub fn direction(&self) -> Option<MethodDirection> {
        self.method.as_deref().and_then(methods::direction_of)
    }

    /// 校验「这条消息确实是发往/来自该方向的已实现方法」。
    ///
    /// 失败分两类，且刻意不合并：方向不符是 [`AcpError::WrongDirection`]（结构/语义错误），
    /// 方法未实现（含 `_` 前缀扩展方法与未登记方法）是 [`AcpError::Unsupported`]
    /// （显式不支持，`docs/ACP_COMPATIBILITY_MATRIX.md` 的 `explicit_never_silent`）。
    pub fn ensure_direction(&self, expected: MethodDirection) -> Result<&str> {
        let method = self.method.as_deref().ok_or_else(|| AcpError::Malformed {
            detail: "该消息没有方法名（响应不能按方法处理）".to_owned(),
        })?;
        match methods::status_of(method) {
            MethodStatus::Implemented => {}
            status => {
                return Err(AcpError::Unsupported {
                    method: method.to_owned(),
                    status,
                });
            }
        }
        let actual = methods::direction_of(method).ok_or_else(|| AcpError::Unsupported {
            method: method.to_owned(),
            status: MethodStatus::Unknown,
        })?;
        if actual == expected || actual == MethodDirection::Bidirectional {
            Ok(method)
        } else {
            Err(AcpError::WrongDirection {
                method: method.to_owned(),
                actual,
                expected,
            })
        }
    }

    /// `params` 对象；缺失或不是对象时返回 [`AcpError::MissingField`]。
    pub fn params(&self) -> Result<Value> {
        match self.document.member("params")? {
            Some(value) if value.is_object() => Ok(value),
            _ => Err(AcpError::MissingField {
                field: "params".to_owned(),
            }),
        }
    }

    /// 响应里的 `result`；`error` 响应返回 [`AcpError::InvalidField`]，缺两者返回
    /// [`AcpError::MissingField`]。
    pub fn result(&self) -> Result<Value> {
        if let Some(error) = self.document.member("error")? {
            return Err(AcpError::InvalidField {
                field: "error".to_owned(),
                detail: summarize_error(&error),
            });
        }
        self.document
            .member("result")?
            .ok_or_else(|| AcpError::MissingField {
                field: "result".to_owned(),
            })
    }

    /// 响应里是否带 `error`。
    pub fn is_error_response(&self) -> bool {
        matches!(self.document.member("error"), Ok(Some(_)))
    }

    /// 响应里的 JSON-RPC 错误（`code` + `message`）。
    ///
    /// 没有 `error` 成员时返回 `None`；`code` 缺失时按 JSON-RPC 的保留语义记 `0`。`message` 是对端给的
    /// 文本，只用于错误分类与日志，不进入我们自己的 wire 字段。
    #[must_use]
    pub fn error_info(&self) -> Option<(i64, String)> {
        let error = self.document.member("error").ok().flatten()?;
        let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Some((code, message))
    }
}

/// 把 JSON-RPC 错误对象压成一行**不含正文**的描述（只保留 `code`）。
fn summarize_error(error: &Value) -> String {
    match error.get("code").and_then(Value::as_i64) {
        Some(code) => format!("JSON-RPC 错误码 {code}"),
        None => "JSON-RPC 错误（无 code）".to_owned(),
    }
}
