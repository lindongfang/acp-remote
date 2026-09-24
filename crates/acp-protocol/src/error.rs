//! 解码与分类失败的分类。
//!
//! 这里的错误只覆盖**协议层**：结构不合法、方向不对、缺少 required 字段、超过固定上限、
//! 方法未登记（显式不支持）。上层语义（能力未宣告、Agent 不可用等）不属于本 crate，
//! 以免把「结构错误」和「能力不支持」混成同一个错误（`docs/ACP_COMPATIBILITY_MATRIX.md` §3.3 的
//! `explicit_unsupported` 与 `visible_degradation` 是两种不同的处置）。
//!
//! 错误消息只包含方法名、字段名、字节数等协议元数据，**不含**消息正文，因此可以安全进入日志。

use crate::methods::MethodDirection;

/// 协议层结果。
pub type Result<T> = std::result::Result<T, AcpError>;

/// 协议层错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AcpError {
    /// 不是合法 JSON document。
    #[error("消息不是合法 JSON：{detail}")]
    Json {
        /// 解析器给出的位置与原因（不含正文）。
        detail: String,
    },

    /// 是合法 JSON，但不是本协议要求的形状（顶层不是对象、`jsonrpc` 不是 `"2.0"`、
    /// 既没有 `method` 也没有 `result`/`error` 等）。
    #[error("消息不是合法的 JSON-RPC 形状：{detail}")]
    Malformed {
        /// 违反的结构约束。
        detail: String,
    },

    /// 缺少该方法/该类型要求的字段。
    #[error("缺少 required 字段 {field}")]
    MissingField {
        /// 字段名（JSON 中的键名）。
        field: String,
    },

    /// 字段存在但类型或取值不合法。
    #[error("字段 {field} 的取值不合法：{detail}")]
    InvalidField {
        /// 字段名。
        field: String,
        /// 违反的约束。
        detail: String,
    },

    /// 超过单条消息的固定上限（`docs/SECURITY_DESIGN.md` §12.2：1 MiB）。
    #[error("单条消息超过 {limit} 字节上限（实际 {actual} 字节）")]
    Oversize {
        /// 上限（字节）。
        limit: usize,
        /// 实际长度（字节）。
        actual: usize,
    },

    /// 消息方向与方法登记的方向不一致。
    #[error("方法 {method} 的方向是 {actual:?}，不能按 {expected:?} 处理")]
    WrongDirection {
        /// 方法名。
        method: String,
        /// 方法与登记表给出的真实方向。
        actual: MethodDirection,
        /// 调用方假定的方向。
        expected: MethodDirection,
    },

    /// 方法未在本阶段实现（含上游快照中不存在的 `_` 前缀扩展方法）。
    ///
    /// 这是**显式不支持**：调用方必须把它转成「方法未找到」等价的错误，不得转发或静默丢弃。
    #[error("方法 {method} 在本阶段没有实现（{status:?}）")]
    Unsupported {
        /// 方法名。
        method: String,
        /// 该方法的登记状态。
        status: crate::methods::MethodStatus,
    },
}
