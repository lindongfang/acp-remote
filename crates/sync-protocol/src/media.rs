//! 类型化媒体、消息通道与有效待处理交互的公共值对象（`schemas/sync/v1/media.schema.json`）。
//!
//! 这些形状不是独立的 wire 消息：`mediaReference` 出现在事件 view 与 `session.read` 的结果里，
//! `effectiveInteraction` 出现在 `session.read` 的 `pending_interactions` 与应答命令的版本校验里，
//! `messageChannel`/`partialResult` 标记消息通道与正文完整性。它们都**不**替代 ACP 原文：同一事件的
//! `acp.rawJson` 字节保持不变，客户端只消费专用视图。
//!
//! 两条硬约束在这里只做形状保证：
//!
//! - `mediaReference` 不含 URL、临时路径或 bearer 凭据，只有内容标识、长度、摘要与可用性；
//! - 不可用是显式取值（`expired`/`not_fetched`/`unsupported`），调用方不得用空字节或空地址冒充内容。

use serde::{Deserialize, Serialize};

use crate::common::{DecimalString, InteractionOption, Nullable, RawObject, Timestamp, Uuid};

/// `messageChannel`：消息通道的封闭词表（`reply｜thought`）。
///
/// 回复与思考**分别**持久化、分别折叠；Agent 未提供思考时不得生成 `thought`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageChannel {
    /// Agent 的正式回复。
    Reply,
    /// Agent 明确发送的思考。
    Thought,
}

impl MessageChannel {
    /// wire 取值。
    pub fn as_str(self) -> &'static str {
        match self {
            MessageChannel::Reply => "reply",
            MessageChannel::Thought => "thought",
        }
    }
}

/// `partialResult.state`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    /// 完整成功。
    Completed,
    /// 已收到内容被保留，但上游未完成。
    Partial,
    /// 失败；已收到内容保留。
    Failed,
    /// 被取消；已收到内容保留。
    Cancelled,
}

/// `partialResult.reason` 的封闭词表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartialReason {
    /// Agent 主动停止。
    AgentStopped,
    /// Agent 失败。
    AgentFailed,
    /// 本端取消了 turn。
    TurnCancelled,
    /// 连接丢失。
    ConnectionLost,
    /// 内容超出保留期。
    RetentionExpired,
}

/// `partialResult`：turn 终态的正文完整性标记。
///
/// `partial` 表示已收到内容被保留但上游未完成；MUST NOT 被客户端当作完整成功回复。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialResult {
    /// 终态类别。
    pub state: MessageState,
    /// 不可用原因；`completed` 时为 `null`。
    pub reason: Nullable<PartialReason>,
}

impl PartialResult {
    /// 完整成功的正文。
    pub fn completed() -> Self {
        Self {
            state: MessageState::Completed,
            reason: Nullable::null(),
        }
    }
}

/// `mediaReference.kind`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    /// 图片。
    Image,
    /// 音频。
    Audio,
    /// 文件。
    File,
}

/// `mediaReference.availability`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaAvailability {
    /// 可经 §3.4 的已认证连接读取。
    Available,
    /// 超出保留期或已随消息清理。
    Expired,
    /// 只有外部 URI，未自动下载。
    NotFetched,
    /// 当前端到端能力不具备该类型。
    Unsupported,
}

/// `mediaReference`：Owner 侧已封存附件的读取引用。
///
/// 只标识内容与可用性，不带任何下载 URL、临时路径或 bearer 凭据；读取必须经认证附件连接并重新校验
/// 会话与主体归属。imported 会话的媒体引用由 Owner 持有，Access 只透传引用、不落正文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaReference {
    #[serde(rename = "attachmentId")]
    pub attachment_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub kind: MediaKind,
    #[serde(rename = "mediaType")]
    pub media_type: crate::common::NonEmptyText<255>,
    #[serde(rename = "byteLength")]
    pub byte_length: DecimalString,
    pub sha256: crate::common::Base64Url<32>,
    #[serde(rename = "displayName")]
    pub display_name: Nullable<crate::common::NonEmptyText<255>>,
    pub availability: MediaAvailability,
}

/// `effectiveInteraction.kind`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    /// 权限请求。
    Permission,
    /// 表单（elicitation）请求。
    Elicitation,
}

/// `effectiveInteraction`：服务端重新确认过的当前有效待处理交互。
///
/// `request_version` 是当前有效请求的版本；应答必须原样回传，版本不符或已失效时拒绝且不消费请求。
/// 历史条目只有定位信息，不能直接应答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveInteraction {
    #[serde(rename = "interactionId")]
    pub interaction_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub kind: InteractionKind,
    /// 当前有效请求的版本（十进制字符串）。
    #[serde(rename = "requestVersion")]
    pub request_version: DecimalString,
    #[serde(rename = "expiresAt")]
    pub expires_at: Nullable<Timestamp>,
    pub options: Vec<InteractionOption>,
    /// `elicitation` 的 ACP 表单 schema 原样投影；`permission` 时为 `null`。
    pub schema: Nullable<RawObject>,
}
