//! ACP 方法登记表与 `session/update` 判别子登记表。
//!
//! 这两个表是 `compatibility/acp/v1/matrix.json` 在 Rust 侧的**手工镜像**（与 `core::broker` 镜像
//! `compatibility/commands/v1/commands.json` 同一做法）：本 crate 需要在不解析 JSON 的前提下判断
//! 「这个方法属于哪个方向」「是否已实现」。矩阵是唯一权威，`tests/matrix_tables.rs` 逐条比对两张表，
//! 漂移会让契约测试失败——不要在别处再抄一份。

/// 交付节奏（矩阵 `delivery` 字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// 第一阶段必须实现并通过。
    Mvp,
    /// 只在相关能力被宣告后启用，但必须有明确 gate。
    ConditionalMvp,
    /// 后续实现；第一阶段必须显式不支持。
    PostMvp,
}

/// 消息方向（矩阵 `direction` 字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodDirection {
    /// 客户端（本机的 ACP Client）发往 Agent。
    ClientToAgent,
    /// Agent 发往客户端。
    AgentToClient,
    /// 双向（例如 JSON-RPC 的 `$/cancel_request`）。
    Bidirectional,
}

/// JSON-RPC 类别（矩阵 `kind` 字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    /// 请求（有 `id`，需要响应）。
    Request,
    /// 通知（无 `id`）。
    Notification,
}

/// 一条方法在本阶段的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodStatus {
    /// 已实现（本 crate 能编码/解码并按语义处理）。
    Implemented,
    /// 矩阵已登记但本阶段未实现：必须显式不支持。
    NotImplemented,
    /// `_` 前缀扩展方法（上游快照中不存在）：必须显式不支持，不得转发。
    Extension,
    /// 未在矩阵登记的未知方法：必须显式不支持。
    Unknown,
}

/// 一条方法的登记项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodSpec {
    /// 线上方法名。
    pub wire_name: &'static str,
    /// 方向。
    pub direction: MethodDirection,
    /// 类别。
    pub kind: MethodKind,
    /// 交付节奏。
    pub delivery: Delivery,
    /// 本阶段是否已实现。
    pub implemented: bool,
}

const fn spec(
    wire_name: &'static str,
    direction: MethodDirection,
    kind: MethodKind,
    delivery: Delivery,
    implemented: bool,
) -> MethodSpec {
    MethodSpec {
        wire_name,
        direction,
        kind,
        delivery,
        implemented,
    }
}

/// 矩阵 `methods` 组的完整登记表（25 条，顺序与矩阵一致）。
pub const METHODS: &[MethodSpec] = &[
    spec(
        "initialize",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::Mvp,
        true,
    ),
    spec(
        "authenticate",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::ConditionalMvp,
        false,
    ),
    spec(
        "logout",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "session/new",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::Mvp,
        true,
    ),
    spec(
        "session/load",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "session/list",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "session/delete",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "session/resume",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "session/close",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "session/set_mode",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::ConditionalMvp,
        true,
    ),
    spec(
        "session/set_config_option",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::ConditionalMvp,
        true,
    ),
    spec(
        "session/prompt",
        MethodDirection::ClientToAgent,
        MethodKind::Request,
        Delivery::Mvp,
        true,
    ),
    spec(
        "session/cancel",
        MethodDirection::ClientToAgent,
        MethodKind::Notification,
        Delivery::Mvp,
        true,
    ),
    spec(
        "session/update",
        MethodDirection::AgentToClient,
        MethodKind::Notification,
        Delivery::Mvp,
        true,
    ),
    spec(
        "session/request_permission",
        MethodDirection::AgentToClient,
        MethodKind::Request,
        Delivery::Mvp,
        true,
    ),
    spec(
        "fs/read_text_file",
        MethodDirection::AgentToClient,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "fs/write_text_file",
        MethodDirection::AgentToClient,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "terminal/create",
        MethodDirection::AgentToClient,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "terminal/output",
        MethodDirection::AgentToClient,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "terminal/release",
        MethodDirection::AgentToClient,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "terminal/wait_for_exit",
        MethodDirection::AgentToClient,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "terminal/kill",
        MethodDirection::AgentToClient,
        MethodKind::Request,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "elicitation/create",
        MethodDirection::AgentToClient,
        MethodKind::Request,
        Delivery::ConditionalMvp,
        true,
    ),
    spec(
        "elicitation/complete",
        MethodDirection::AgentToClient,
        MethodKind::Notification,
        Delivery::PostMvp,
        false,
    ),
    spec(
        "$/cancel_request",
        MethodDirection::Bidirectional,
        MethodKind::Notification,
        Delivery::ConditionalMvp,
        false,
    ),
];

/// `session/update` 的已知判别子（矩阵 `sessionUpdates` 组的 `wireValue`，11 条，顺序与矩阵一致）。
pub const SESSION_UPDATE_WIRE_VALUES: &[&str] = &[
    "user_message_chunk",
    "agent_message_chunk",
    "agent_thought_chunk",
    "tool_call",
    "tool_call_update",
    "plan",
    "available_commands_update",
    "current_mode_update",
    "config_option_update",
    "session_info_update",
    "usage_update",
];

/// 按方法名查登记项。
#[must_use]
pub fn find(method: &str) -> Option<&'static MethodSpec> {
    METHODS.iter().find(|entry| entry.wire_name == method)
}

/// 一条方法在本阶段的状态。未登记的方法按 `_` 前缀区分「扩展方法」与「未知方法」。
#[must_use]
pub fn status_of(method: &str) -> MethodStatus {
    match find(method) {
        Some(entry) if entry.implemented => MethodStatus::Implemented,
        Some(_) => MethodStatus::NotImplemented,
        None if method.starts_with('_') => MethodStatus::Extension,
        None => MethodStatus::Unknown,
    }
}

/// 方法的方向；未登记的方法返回 `None`（调用方应先用 [`status_of`] 给出显式不支持）。
#[must_use]
pub fn direction_of(method: &str) -> Option<MethodDirection> {
    find(method).map(|entry| entry.direction)
}

/// `session/update` 的判别子是否已登记。
#[must_use]
pub fn is_known_session_update(wire_value: &str) -> bool {
    SESSION_UPDATE_WIRE_VALUES.contains(&wire_value)
}
