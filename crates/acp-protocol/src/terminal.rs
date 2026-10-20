//! agent→client 的 `terminal/*` 服务方法：请求与响应的类型化 wire 形状。
//!
//! 五个方法都是上游固定快照（`schemas/acp/v1/upstream/schema.json` 的 `CreateTerminalRequest`、
//! `TerminalOutputRequest`、`WaitForTerminalExitRequest`、`KillTerminalRequest`、
//! `ReleaseTerminalRequest`）的逐字段镜像：
//!
//! - 请求里 `sessionId` 与 `terminalId`/`command` 是必填项，缺失与类型不符可区分；
//! - `outputByteLimit` 是 `uint64`：负值与小数在解码边界被拒（不能悄悄截成 0 或忽略）；
//! - 未知字段不进类型化视图，但保真路径仍是 [`crate::RawDocument`] 承载的原文。
//!
//! 本模块只做编解码与形状校验。**执行语义**（真实子进程、输出预算、kill/release 竞争收敛、
//! 进程树清理）由 `agent-host` 的终端服务拥有；是否宣告 `clientCapabilities.terminal` 也由组合层决定。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Envelope;
use crate::error::{AcpError, Result};
use crate::message::agent_params;

/// `terminal/create` 的请求参数（`CreateTerminalRequest`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateTerminalRequest {
    /// 会话标识（终端任务绑定所属会话）。
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// 要执行的命令（可执行文件或命令名；**不经 shell**，由服务实现按参数数组启动）。
    pub command: String,
    /// 命令参数（字符串数组；缺省为空数组）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// 额外环境变量（缺省为空数组；服务实现须校验变量名）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<EnvVariable>,
    /// 工作目录（绝对路径；缺省由服务实现使用会话工作目录）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// 保留输出字节上限（`uint64`；`0` 表示不保留正文）。
    #[serde(
        rename = "outputByteLimit",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub output_byte_limit: Option<u64>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl CreateTerminalRequest {
    /// 从 `terminal/create` 的 `params` 解码。
    ///
    /// 缺字段是 [`AcpError::MissingField`]，存在但类型不符是 [`AcpError::InvalidField`]
    /// （`outputByteLimit` 为负或小数也归入后者）——两种都不构造 DTO。
    pub fn from_params(params: &Value) -> Result<Self> {
        let args = match params.get("args") {
            None => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| AcpError::InvalidField {
                            field: "args".to_owned(),
                            detail: "数组元素必须是字符串".to_owned(),
                        })
                })
                .collect::<Result<Vec<_>>>()?,
            Some(other) => {
                return Err(AcpError::InvalidField {
                    field: "args".to_owned(),
                    detail: format!("必须是数组，收到 {}", value_kind(other)),
                });
            }
        };
        let env = match params.get("env") {
            None => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .enumerate()
                .map(|(index, item)| EnvVariable::from_value(item, index))
                .collect::<Result<Vec<_>>>()?,
            Some(other) => {
                return Err(AcpError::InvalidField {
                    field: "env".to_owned(),
                    detail: format!("必须是数组，收到 {}", value_kind(other)),
                });
            }
        };
        let cwd = match params.get("cwd") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) => Some(value.clone()),
            Some(other) => {
                return Err(AcpError::InvalidField {
                    field: "cwd".to_owned(),
                    detail: format!("必须是字符串，收到 {}", value_kind(other)),
                });
            }
        };
        let output_byte_limit = match params.get("outputByteLimit") {
            None | Some(Value::Null) => None,
            Some(Value::Number(number)) => {
                Some(number.as_u64().ok_or_else(|| AcpError::InvalidField {
                    field: "outputByteLimit".to_owned(),
                    detail: "必须是 uint64（非负整数）".to_owned(),
                })?)
            }
            Some(other) => {
                return Err(AcpError::InvalidField {
                    field: "outputByteLimit".to_owned(),
                    detail: format!("必须是整数，收到 {}", value_kind(other)),
                });
            }
        };
        Ok(Self {
            session_id: required_string(params, "sessionId")?,
            command: required_string(params, "command")?,
            args,
            env,
            cwd,
            output_byte_limit,
            meta: params.get("_meta").cloned(),
        })
    }
}

/// `terminal/create` 的请求（含方向与方法名校验）。
pub fn create_request(envelope: &Envelope) -> Result<CreateTerminalRequest> {
    CreateTerminalRequest::from_params(&agent_params(envelope, "terminal/create")?)
}

/// 一个环境变量（`EnvVariable`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvVariable {
    /// 变量名。
    pub name: String,
    /// 变量值。
    pub value: String,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段，原样保留。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl EnvVariable {
    /// 从数组元素解码：`name`/`value` 都是必填字符串。
    fn from_value(value: &Value, index: usize) -> Result<Self> {
        let Some(object) = value.as_object() else {
            return Err(AcpError::InvalidField {
                field: "env".to_owned(),
                detail: format!("第 {index} 项必须是对象"),
            });
        };
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| AcpError::InvalidField {
                field: "env".to_owned(),
                detail: format!("第 {index} 项缺少字符串 name"),
            })?
            .to_owned();
        let value = object
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| AcpError::InvalidField {
                field: "env".to_owned(),
                detail: format!("第 {index} 项缺少字符串 value"),
            })?
            .to_owned();
        Ok(Self {
            name,
            value,
            meta: object.get("_meta").cloned(),
            extra: Map::new(),
        })
    }
}

/// 只带 `sessionId` + `terminalId` 的请求（`terminal/output`、`terminal/wait_for_exit`、
/// `terminal/kill`、`terminal/release` 的同形参数）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalRefRequest {
    /// 会话标识。
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// 终端标识。
    #[serde(rename = "terminalId")]
    pub terminal_id: String,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl TerminalRefRequest {
    /// 从 `params` 解码（两个字段都是必填字符串）。
    pub fn from_params(params: &Value) -> Result<Self> {
        Ok(Self {
            session_id: required_string(params, "sessionId")?,
            terminal_id: required_string(params, "terminalId")?,
            meta: params.get("_meta").cloned(),
        })
    }
}

/// `terminal/create` 的响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateTerminalResponse {
    /// 新建终端的标识。
    #[serde(rename = "terminalId")]
    pub terminal_id: String,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 退出状态（`TerminalExitStatus`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalExitStatus {
    /// 退出码（被信号终止时为 `null`）。
    #[serde(rename = "exitCode", default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u32>,
    /// 终止信号（正常退出时为 `null` 或缺席）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl TerminalExitStatus {
    /// 只带退出码的退出状态。
    #[must_use]
    pub fn from_code(code: u32) -> Self {
        Self {
            exit_code: Some(code),
            signal: None,
            meta: None,
        }
    }
}

/// `terminal/output` 的响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalOutputResponse {
    /// 到目前为止的输出（stdout 与 stderr 按实际接收次序拼接；必要时按预算截断）。
    pub output: String,
    /// 输出是否因字节预算被截断（截断只从开头丢弃，且落在字符边界）。
    pub truncated: bool,
    /// 已退出时的退出状态；仍在运行时缺席。
    #[serde(
        rename = "exitStatus",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub exit_status: Option<TerminalExitStatus>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `terminal/wait_for_exit` 的响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitForTerminalExitResponse {
    /// 退出码。
    #[serde(rename = "exitCode", default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u32>,
    /// 终止信号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl WaitForTerminalExitResponse {
    /// 从退出状态构造。
    #[must_use]
    pub fn from_status(status: &TerminalExitStatus) -> Self {
        Self {
            exit_code: status.exit_code,
            signal: status.signal.clone(),
            meta: None,
        }
    }
}

/// `terminal/output` 的请求。
pub fn output_request(envelope: &Envelope) -> Result<TerminalRefRequest> {
    TerminalRefRequest::from_params(&agent_params(envelope, "terminal/output")?)
}

/// `terminal/wait_for_exit` 的请求。
pub fn wait_for_exit_request(envelope: &Envelope) -> Result<TerminalRefRequest> {
    TerminalRefRequest::from_params(&agent_params(envelope, "terminal/wait_for_exit")?)
}

/// `terminal/kill` 的请求。
pub fn kill_request(envelope: &Envelope) -> Result<TerminalRefRequest> {
    TerminalRefRequest::from_params(&agent_params(envelope, "terminal/kill")?)
}

/// `terminal/release` 的请求。
pub fn release_request(envelope: &Envelope) -> Result<TerminalRefRequest> {
    TerminalRefRequest::from_params(&agent_params(envelope, "terminal/release")?)
}

// -------------------------------------------------------------------------------------------
// 客户端侧（我们自己的调用）：仅用于契约测试与自编码校验。
// -------------------------------------------------------------------------------------------

/// 构造 `terminal/*` 请求的 `params` 形状（测试与将来的客户端实现共用）。
///
/// 生产路径里这些方法是 Agent 发来的；本函数存在的意义是让「我们自己写出的形状」与「我们解码的形状」
/// 由同一个类型定义，避免测试里手抄一份字段名。
pub fn terminal_ref_params(session_id: &str, terminal_id: &str) -> Value {
    serde_json::json!({ "sessionId": session_id, "terminalId": terminal_id })
}

fn required_string(params: &Value, field: &str) -> Result<String> {
    match params.get(field) {
        Some(Value::String(value)) => Ok(value.clone()),
        Some(other) => Err(AcpError::InvalidField {
            field: field.to_owned(),
            detail: format!("必须是字符串，收到 {}", value_kind(other)),
        }),
        None => Err(AcpError::MissingField {
            field: field.to_owned(),
        }),
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
