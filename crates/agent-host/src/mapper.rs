//! ACP 事件到 core 事件的映射。
//!
//! 三条约束：
//!
//! 1. **结构化不文本化**：tool call、diff、terminal、plan、commands 都按结构放进 `view`，
//!    同时把**逐字节原文**放进 `payload.acp`（`docs/SYNC_PROTOCOL.md` §10.3 的 `rawJson`）。
//! 2. **turn 归属交给 core**：`EndpointEvent.turn` 一律 `None`，由 broker 用当前派发的 turn 补齐
//!    （`SessionEndpoint::prompt` 的签名不携带 `TurnId`）。
//! 3. **只有 core 真会消费的字段是硬要求**：`interactionId`（交互配对）与 `messageId`/`deltaIndex`/
//!    `text`（delta 折叠）。`turnId`/`version` 在适配器侧不可知，因此不出现在 view 里——
//!    见本变更 `verification.md` 的 Check Plan Changes。

use std::collections::HashMap;

use acp_core::model::{
    AcpRaw, Digest, EndpointEvent, EventKind, EventPayload, EventType, InteractionOption,
    MessageId, PublicError, Timestamp, ViewJson,
};
use acp_core::ports::{Clock, IdGenerator};
use acp_protocol::RawDocument;
use acp_protocol::content::{ContentBlock, ToolCallContent};
use acp_protocol::update::{SessionUpdate, ToolCall, ToolCallUpdate};
use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::error::HostError;

/// delta 分组状态：同一消息内的 `deltaIndex` 从 0 起严格加一（由适配器分配）。
#[derive(Debug, Default)]
pub struct UpdateState {
    messages: HashMap<String, MessageId>,
    counters: HashMap<MessageId, u64>,
    turn_seq: u64,
}

impl UpdateState {
    /// 新 turn 开始时重置消息分组（新 turn 的流是新消息）。
    pub fn begin_turn(&mut self) {
        self.messages.clear();
        self.counters.clear();
        self.turn_seq += 1;
    }

    fn message_for(&mut self, key: &str, ids: &dyn IdGenerator) -> MessageId {
        if let Some(existing) = self.messages.get(key) {
            return existing.clone();
        }
        let id = ids.message_id();
        self.messages.insert(key.to_owned(), id.clone());
        id
    }

    fn next_index(&mut self, message: &MessageId) -> u64 {
        let entry = self.counters.entry(message.clone()).or_insert(0);
        let index = *entry;
        *entry += 1;
        index
    }
}

/// 把 `session/update` 映射成 core 事件。
///
/// 已知判别子映射到 Sync 事件类型（`docs/SYNC_PROTOCOL.md` §10.3 的判别子映射表）；
/// 未登记判别子按未知事件**可见降级**并保留原文。
pub fn update_events(
    update: &SessionUpdate,
    raw: &RawDocument,
    state: &mut UpdateState,
    ids: &dyn IdGenerator,
    clock: &dyn Clock,
) -> Result<Vec<EndpointEvent>, HostError> {
    let acp = Some(acp_raw(raw)?);
    let at = clock.now();
    let mut events = Vec::new();

    match update {
        SessionUpdate::UserMessageChunk(chunk) => {
            events.push(delta_event(
                DeltaSpec {
                    event_type: "user.message.delta",
                    acp_message_id: chunk.message_id.as_deref(),
                    kind: "user",
                },
                state,
                &chunk.content,
                acp,
                at,
                ids,
            )?);
        }
        SessionUpdate::AgentMessageChunk(chunk) => {
            events.push(delta_event(
                DeltaSpec {
                    event_type: "agent.message.delta",
                    acp_message_id: chunk.message_id.as_deref(),
                    kind: "agent",
                },
                state,
                &chunk.content,
                acp,
                at,
                ids,
            )?);
        }
        SessionUpdate::AgentThoughtChunk(chunk) => {
            events.push(delta_event(
                DeltaSpec {
                    event_type: "agent.thought.delta",
                    acp_message_id: chunk.message_id.as_deref(),
                    kind: "thought",
                },
                state,
                &chunk.content,
                acp,
                at,
                ids,
            )?);
        }
        SessionUpdate::ToolCall(tool) => {
            events.push(view_event(
                EventKind::Structured,
                "tool.call.started",
                tool_call_view(tool, "pending"),
                acp,
                at,
            )?);
        }
        SessionUpdate::ToolCallUpdate(update) => {
            events.push(view_event(
                EventKind::Structured,
                "tool.call.updated",
                tool_call_update_view(update, "in_progress"),
                acp.clone(),
                at.clone(),
            )?);
            // 终态额外生成 `tool.call.completed`（Sync 合同要求）。
            if matches!(update.status.as_deref(), Some("completed" | "failed")) {
                events.push(view_event(
                    EventKind::Structured,
                    "tool.call.completed",
                    tool_call_update_view(update, "completed"),
                    acp,
                    at,
                )?);
            }
        }
        SessionUpdate::Plan(plan) => {
            let entries: Vec<Value> = plan
                .entries
                .iter()
                .map(|entry| {
                    json!({
                        "content": entry.content,
                        "priority": entry.priority,
                        "status": entry.status,
                    })
                })
                .collect();
            events.push(view_event(
                EventKind::Structured,
                "session.plan.changed",
                json!({ "entries": entries }),
                acp,
                at,
            )?);
        }
        SessionUpdate::AvailableCommandsUpdate(commands) => {
            let items: Vec<Value> = commands
                .available_commands
                .iter()
                .map(|command| json!({ "name": command.name, "description": command.description }))
                .collect();
            events.push(view_event(
                EventKind::Structured,
                "session.commands.changed",
                json!({ "commands": items }),
                acp,
                at,
            )?);
        }
        SessionUpdate::CurrentModeUpdate(mode) => {
            events.push(view_event(
                EventKind::State,
                "session.mode.changed",
                json!({ "currentModeId": mode.current_mode_id }),
                acp,
                at,
            )?);
        }
        SessionUpdate::ConfigOptionUpdate(config) => {
            events.push(view_event(
                EventKind::State,
                "session.config.changed",
                json!({ "configOptions": config.config_options }),
                acp,
                at,
            )?);
        }
        SessionUpdate::SessionInfoUpdate(info) => {
            let updated_at = info
                .updated_at
                .clone()
                .unwrap_or_else(|| clock.now().as_str().to_owned());
            events.push(view_event(
                EventKind::State,
                "session.info.changed",
                json!({ "title": info.title, "updatedAt": updated_at }),
                acp,
                at,
            )?);
        }
        SessionUpdate::UsageUpdate(usage) => {
            events.push(view_event(
                EventKind::State,
                "session.usage.changed",
                json!({
                    "used": usage.used.to_string(),
                    "size": usage.size.to_string(),
                }),
                acp,
                at,
            )?);
        }
        SessionUpdate::Unknown { payload, .. } => {
            // 可见降级：保留原文，`view` 里给出判别子与原始 payload，绝不静默丢弃。
            events.push(view_event(
                EventKind::Structured,
                "session.update.unknown",
                json!({
                    "sessionUpdate": update.wire_value(),
                    "payload": payload,
                }),
                acp,
                at,
            )?);
        }
    }
    Ok(events)
}

/// 一条 delta 的标识（事件类型 + ACP 消息标识 + 分组类别）。
#[derive(Debug, Clone, Copy)]
struct DeltaSpec<'a> {
    event_type: &'a str,
    acp_message_id: Option<&'a str>,
    kind: &'a str,
}

fn delta_event(
    spec: DeltaSpec<'_>,
    state: &mut UpdateState,
    content: &ContentBlock,
    acp: Option<AcpRaw>,
    at: Timestamp,
    ids: &dyn IdGenerator,
) -> Result<EndpointEvent, HostError> {
    let DeltaSpec {
        event_type,
        acp_message_id,
        kind,
    } = spec;
    let key = match acp_message_id {
        Some(id) => format!("{kind}:{id}"),
        None => format!("{kind}:turn-{}", state.turn_seq),
    };
    let message = state.message_for(&key, ids);
    let index = state.next_index(&message);
    let text = content.text().unwrap_or_default().to_owned();
    let mut view = json!({
        "messageId": message.as_str(),
        "deltaIndex": index.to_string(),
        "text": text,
    });
    if !content.is_text() {
        // 非文本块必须带上结构化 `block`（core 折叠收尾消息时用它，纯文本客户端只读 `text`）。
        // 未登记类型走 `ContentBlock::Unknown`：它标记了 `skip_serializing`（重新编码没有意义），
        // 因此这里直接用 Agent 给的原始 payload——那才是「可见降级」而不是把它降成 null。
        let block = match content {
            ContentBlock::Unknown { payload, .. } => payload.clone(),
            other => serde_json::to_value(other).unwrap_or(Value::Null),
        };
        if let Some(object) = view.as_object_mut() {
            object.insert("block".to_owned(), block);
        }
    }
    view_event(EventKind::Delta, event_type, view, acp, at)
}

fn tool_call_view(tool: &ToolCall, default_state: &str) -> Value {
    json!({
        "toolCallId": tool.tool_call_id,
        "title": tool.title,
        "state": tool.status.clone().unwrap_or_else(|| default_state.to_owned()),
    })
}

fn tool_call_update_view(update: &ToolCallUpdate, default_state: &str) -> Value {
    json!({
        "toolCallId": update.tool_call_id,
        "title": update.title.clone().unwrap_or_default(),
        "state": update.status.clone().unwrap_or_else(|| default_state.to_owned()),
    })
}

/// 只带 `view` 的事件（适配器自己产生的事件，没有 ACP 原文）。
pub fn view_event_public(
    kind: EventKind,
    event_type: &str,
    view: Value,
    at: Timestamp,
) -> Result<EndpointEvent, HostError> {
    view_event(kind, event_type, view, None, at)
}

fn view_event(
    kind: EventKind,
    event_type: &str,
    view: Value,
    acp: Option<AcpRaw>,
    at: Timestamp,
) -> Result<EndpointEvent, HostError> {
    let text = view.to_string();
    let view = ViewJson::new(&text).map_err(|_| HostError::IdUnavailable)?;
    let event_type = EventType::new(event_type).map_err(|_| HostError::IdUnavailable)?;
    Ok(EndpointEvent::new(
        kind,
        event_type,
        EventPayload::new(view, acp),
        None,
        None,
        at,
    ))
}

/// 权限请求事件：`interactionId` 必须由适配器放进 view（broker 从这里读它，再原样回传
/// `resolve_interaction`）。
pub fn permission_event(
    interaction_id: &acp_core::model::InteractionId,
    tool_call: &ToolCallUpdate,
    options: &[acp_protocol::message::PermissionOption],
    acp: Option<AcpRaw>,
    at: Timestamp,
) -> Result<EndpointEvent, HostError> {
    let options: Vec<Value> = options
        .iter()
        .map(|option| {
            json!({
                "optionId": option.option_id,
                "label": option.name,
                "kind": option.kind,
            })
        })
        .collect();
    view_event(
        EventKind::Interaction,
        "permission.requested",
        json!({
            "interactionId": interaction_id.as_str(),
            "title": tool_call.title.clone().unwrap_or_default(),
            "description": tool_call.name.clone().unwrap_or_default(),
            "options": options,
        }),
        acp,
        at,
    )
}

/// elicitation 请求事件。
pub fn elicitation_event(
    interaction_id: &acp_core::model::InteractionId,
    request: &acp_protocol::message::ElicitationRequest,
    acp: Option<AcpRaw>,
    at: Timestamp,
) -> Result<EndpointEvent, HostError> {
    let schema = match &request.mode {
        acp_protocol::message::ElicitationMode::Form {
            requested_schema, ..
        } => requested_schema.clone(),
        _ => Value::Null,
    };
    view_event(
        EventKind::Interaction,
        "elicitation.requested",
        json!({
            "interactionId": interaction_id.as_str(),
            "title": request.message,
            "schema": schema,
            "initialValues": {},
        }),
        acp,
        at,
    )
}

/// turn 终态事件。
pub fn turn_event(
    event_type: &str,
    error: Option<&PublicError>,
    at: Timestamp,
) -> Result<EndpointEvent, HostError> {
    let mut view = json!({ "state": event_type.rsplit('.').next().unwrap_or("completed") });
    if let Some(error) = error {
        if let Some(object) = view.as_object_mut() {
            object.insert(
                "error".to_owned(),
                json!({
                    "code": error.code(),
                    "message": error.message(),
                    "retryable": error.retryable(),
                }),
            );
        }
    }
    view_event(EventKind::State, event_type, view, None, at)
}

/// 用已登记的公开错误码构造 `PublicError`（不发明词表外的 code）。
pub fn public_error(code: &'static str, message: &str, retryable: bool) -> Option<PublicError> {
    PublicError::coded(code, message, retryable).ok()
}

/// 构造 `AcpRaw`：原文逐字节 + SHA-256（规范无填充 base64url）。
pub fn acp_raw(raw: &RawDocument) -> Result<AcpRaw, HostError> {
    let digest = digest_of(raw.as_str())?;
    AcpRaw::available("application/json", raw.as_str(), digest)
        .map_err(|_| HostError::IdUnavailable)
}

fn digest_of(text: &str) -> Result<Digest, HostError> {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let bytes = hasher.finalize();
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    Digest::new(&encoded).map_err(|_| HostError::IdUnavailable)
}

/// 权限候选项（core 形状）。
pub fn interaction_options(
    options: &[acp_protocol::message::PermissionOption],
) -> Vec<InteractionOption> {
    options
        .iter()
        .filter_map(|option| {
            InteractionOption::try_new(&option.option_id, &option.name, &option.kind).ok()
        })
        .collect()
}

/// 非文本 content block 的结构判定（供测试与诊断使用）。
#[must_use]
pub fn is_structured_content(content: &ContentBlock) -> bool {
    !content.is_text()
}

/// `tool_call` 是否为 diff（结构化内容不被文本化的最小判据）。
#[must_use]
pub fn has_diff(content: &ToolCallContent) -> bool {
    matches!(content, ToolCallContent::Diff(_))
}
