//! ACP 事件到 core 事件的映射。
//!
//! 三条约束：
//!
//! 1. **结构化不文本化**：tool call、diff、terminal、plan、commands 都按结构放进 `view`，
//!    同时把**逐字节原文**放进 `payload.acp`（`docs/SYNC_PROTOCOL.md` §10.3 的 `rawJson`）。
//!    工具调用内容里的类型化 `Diff` 元素额外投影成 view 的 `diff` 键（见 [`diff_elements`]）：
//!    core 的文件改动派生**只能**来自它，因此这里既不改写 ACP 原文，也**绝不**从自由形状的
//!    `rawInput` 推断文件改动。
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
                tool_call_view(tool, "pending", raw),
                acp,
                at,
            )?);
        }
        SessionUpdate::ToolCallUpdate(update) => {
            events.push(view_event(
                EventKind::Structured,
                "tool.call.updated",
                tool_call_update_view(update, "in_progress", raw),
                acp.clone(),
                at.clone(),
            )?);
            // 终态额外生成 `tool.call.completed`（Sync 合同要求）。
            if matches!(update.status.as_deref(), Some("completed" | "failed")) {
                events.push(view_event(
                    EventKind::Structured,
                    "tool.call.completed",
                    tool_call_update_view(update, "completed", raw),
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

fn tool_call_view(tool: &ToolCall, default_state: &str, raw: &RawDocument) -> Value {
    let mut view = json!({
        "toolCallId": tool.tool_call_id,
        "title": tool.title,
        "state": tool.status.clone().unwrap_or_else(|| default_state.to_owned()),
    });
    attach_diffs(&mut view, tool.content.as_deref(), raw);
    view
}

fn tool_call_update_view(update: &ToolCallUpdate, default_state: &str, raw: &RawDocument) -> Value {
    let mut view = json!({
        "toolCallId": update.tool_call_id,
        "title": update.title.clone().unwrap_or_default(),
        "state": update.status.clone().unwrap_or_else(|| default_state.to_owned()),
    });
    attach_diffs(&mut view, update.content.as_deref(), raw);
    view
}

/// 把工具调用内容里的**类型化 Diff 元素**挂到 view 的 `diff` 键上。
///
/// 这是 core 派生文件改动事件的**唯一**来源（`specs/local-agent-host/spec.md` 的「文件改动的转发现源
/// 限于类型化 Diff」）：只在确实存在类型化 `diff` 元素时挂键，`content` 里没有 Diff 元素时**整个键
/// 缺席**（既不出现 `null` 也不出现空数组），因此「不含 Diff 的工具调用」在视图层就是不可派生的。
///
/// 每个元素带三项**按来源结构解码**的字段（`path`/`oldText`/`newText`，缺席即不写该键——不能用于计算
/// 的输入必须以「缺席」表达，`MUST NOT` 填零或占位）与一项 `raw`：该元素在原始 document 里的
/// **逐字节切片**。切片口径的成立条件是 core 的 `ViewJson` 字节保真（`view_events` 用
/// `Value::to_string()` 序列化，字符串值按 JSON 语义等价往返），而 `raw` 让「逐字节」可直接断言。
///
/// 元素**不按 `rawInput` 推断**：`rawInput` 是自由形状的 ACP 扩展容器，本函数从不读它。
fn attach_diffs(view: &mut Value, content: Option<&[ToolCallContent]>, raw: &RawDocument) {
    let Some(content) = content else {
        return;
    };
    if !content.iter().any(has_diff) {
        return;
    }
    let elements = diff_elements(raw);
    if elements.is_empty() {
        // 类型化元素存在却在原文里定位不到（例如原始 document 被替换过）：不发明取值。
        return;
    }
    if let Some(object) = view.as_object_mut() {
        object.insert("diff".to_owned(), Value::Array(elements));
    }
}

/// 原始 document 里 `update.content[]` 的 `diff` 元素原文（逐字节子串 + 按 JSON 语义解码的三个字段）。
///
/// 路径固定三层（`params.update.content`）。**全程只做结构扫描**：每个元素都按它在原文里的字节偏移
/// 切出，因此 `raw` 必然是原文的子串（`Value::to_string()` 的还原只用于取 `path`/`oldText`/`newText`
/// 的**取值**，不作为切片来源——空格与转义写法都可能与还原结果不同）。
fn diff_elements(raw: &RawDocument) -> Vec<Value> {
    let text = raw.as_str();
    let Some(update) =
        member_literal(text, "params").and_then(|params| member_literal(params, "update"))
    else {
        return Vec::new();
    };
    let Some(content) = member_literal(update, "content") else {
        return Vec::new();
    };
    let mut elements = Vec::new();
    for element in array_elements(content).unwrap_or_default() {
        if decoded_string(member_literal(element, "type")).as_deref() != Some("diff") {
            continue;
        }
        let mut object = serde_json::Map::new();
        for name in ["path", "oldText", "newText"] {
            if let Some(value) = decoded_string(member_literal(element, name)) {
                object.insert(name.to_owned(), Value::String(value));
            }
        }
        object.insert("raw".to_owned(), Value::String(element.to_owned()));
        elements.push(Value::Object(object));
    }
    elements
}

/// 一个 JSON 字符串字面量按 JSON 语义解码后的取值（不是原文，用于比较与字段取值）。
fn decoded_string(literal: Option<&str>) -> Option<String> {
    match serde_json::from_str::<Value>(literal?).ok()? {
        Value::String(text) => Some(text),
        _ => None,
    }
}

/// 在一个 JSON object 的**原文**里取顶层成员的值原文（含引号/括号，不含前后空白）。
///
/// `acp-protocol` 的 [`RawDocument::member_literal`] 只覆盖顶层，而 Diff 元素在
/// `params.update.content[]` 三层之下；这里补一个同口径的结构扫描器。输入已由 `RawDocument::parse`
/// 校验为合法 JSON object，因此只做扫描、不重复做语法校验。
fn member_literal<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'{') {
        return None;
    }
    cursor += 1;
    loop {
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        match bytes.get(cursor)? {
            b',' => cursor += 1,
            b'}' => return None,
            b'"' => {
                let name_end = scan_string_end(bytes, cursor)?;
                let name = text.get(cursor + 1..name_end.checked_sub(1)?)?;
                cursor = name_end;
                while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                    cursor += 1;
                }
                if bytes.get(cursor) != Some(&b':') {
                    return None;
                }
                cursor += 1;
                while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                    cursor += 1;
                }
                let value_end = scan_value_end(bytes, cursor)?;
                if name == key {
                    return text.get(cursor..value_end);
                }
                cursor = value_end;
            }
            _ => return None,
        }
    }
}

/// 取一个 JSON **数组**原文的每个元素原文（元素间按结构扫描切分，不做语法校验）。
fn array_elements(text: &str) -> Option<Vec<&str>> {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'[') {
        return None;
    }
    cursor += 1;
    let mut elements = Vec::new();
    loop {
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        match bytes.get(cursor)? {
            b',' => cursor += 1,
            b']' => return Some(elements),
            _ => {
                let end = scan_value_end(bytes, cursor)?;
                elements.push(text.get(cursor..end)?);
                cursor = end;
            }
        }
    }
}

/// 从开引号扫描到闭引号之后的位置。
fn scan_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start + 1;
    let mut escaped = false;
    while let Some(byte) = bytes.get(cursor) {
        if escaped {
            escaped = false;
        } else if *byte == b'\\' {
            escaped = true;
        } else if *byte == b'"' {
            return Some(cursor + 1);
        }
        cursor += 1;
    }
    None
}

/// 从值的起点扫描到值的结尾之后（索引只落在 ASCII 结构位置上，不会切进多字节字符）。
fn scan_value_end(bytes: &[u8], start: usize) -> Option<usize> {
    match bytes.get(start)? {
        b'"' => scan_string_end(bytes, start),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut cursor = start;
            let mut in_string = false;
            let mut escaped = false;
            while let Some(byte) = bytes.get(cursor) {
                if in_string {
                    if escaped {
                        escaped = false;
                    } else if *byte == b'\\' {
                        escaped = true;
                    } else if *byte == b'"' {
                        in_string = false;
                    }
                } else {
                    match byte {
                        b'"' => in_string = true,
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth = depth.checked_sub(1)?;
                            if depth == 0 {
                                return Some(cursor + 1);
                            }
                        }
                        _ => {}
                    }
                }
                cursor += 1;
            }
            None
        }
        _ => {
            let mut cursor = start;
            while let Some(byte) = bytes.get(cursor) {
                if byte.is_ascii_whitespace() || matches!(byte, b',' | b'}' | b']') {
                    break;
                }
                cursor += 1;
            }
            Some(cursor)
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 一段含 Diff 元素的 `session/update` 通知原文（**刻意保留原样的空白、转义与大整数字面量**：
    /// 切片必须来自它，而不是重新序列化的结果）。
    const RAW_WITH_DIFF: &str = concat!(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s-1","update":{"sessionUpdate":"tool_call","toolCallId":"t-1","title":"编辑","content":["#,
        r#"{"type":"content","content":{"type":"text","text":"前缀"}},"#,
        r#"{"type":"diff","path":"src\/main.rs","oldText":"let a = 1;\n","newText":"let a = 2;\n"},{ "type" : "terminal" , "terminalId" : "term-1" }]}}}"#,
    );

    fn document(text: &str) -> RawDocument {
        RawDocument::parse(text.to_owned()).expect("raw document")
    }

    /// 含 Diff 的工具调用：view 带 `diff` 数组，元素是**原文子串** + 按 JSON 语义解码的三个字段。
    #[test]
    fn diff_elements_carry_raw_slices_and_decoded_paths() {
        let raw = document(RAW_WITH_DIFF);
        let elements = diff_elements(&raw);
        assert_eq!(elements.len(), 1, "恰好一个 diff 元素");
        let element = elements.first().expect("element");
        assert_eq!(element["path"], "src/main.rs", "转义按 JSON 语义解码");
        assert_eq!(element["oldText"], "let a = 1;\n");
        assert_eq!(element["newText"], "let a = 2;\n");
        let slice = element["raw"].as_str().expect("raw");
        assert!(
            raw.as_str().contains(slice),
            "raw 必须是原文的子串（逐字节保真）"
        );
        // 切片保留原样的空白与转义写法：那正是「逐字节」与「重新序列化」的区别。
        assert!(slice.contains("src\\/main.rs"), "原文里的转义必须原样保留");
        assert!(!slice.contains("term-1"), "只切该元素，不带上邻居");
    }

    /// 多个 Diff 元素全部交付（不静默只发第一个）。
    #[test]
    fn multiple_diff_elements_are_all_delivered() {
        let raw = document(
            r#"{"params":{"update":{"sessionUpdate":"tool_call","content":[{"type":"diff","path":"a.txt","newText":"a"},{"type":"diff","path":"b.txt","newText":"b"}]}}}"#,
        );
        let elements = diff_elements(&raw);
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0]["path"], "a.txt");
        assert_eq!(elements[1]["path"], "b.txt");
    }

    /// 不含 Diff 元素时不产生 `diff` 键（`rawInput` 里看似文件编辑的结构**不得**被采纳）。
    #[test]
    fn views_without_typed_diff_carry_no_diff_key() {
        let raw = document(
            r#"{"params":{"update":{"sessionUpdate":"tool_call","toolCallId":"t","title":"看文件","content":[{"type":"content","content":{"type":"text","text":"x"}},{"type":"terminal","terminalId":"q"}],"rawInput":{"path":"src/secret.rs","oldText":"a","newText":"b"}}}}"#,
        );
        let elements = diff_elements(&raw);
        assert!(
            elements.is_empty(),
            "rawInput 里的编辑结构不得被当成 Diff 元素"
        );

        // 视图层同样：只有普通内容块与终端引用时不挂 `diff` 键。
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t",
            "title": "看文件",
            "content": [
                {"type": "content", "content": {"type": "text", "text": "x"}},
                {"type": "terminal", "terminalId": "q"}
            ],
            "rawInput": {"path": "src/secret.rs", "oldText": "a", "newText": "b"}
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "pending", &raw);
        assert!(
            view.get("diff").is_none(),
            "没有类型化 Diff 元素时 diff 键必须缺席：{view}"
        );
        assert!(
            view.get("rawInput").is_none(),
            "适配器不把 rawInput 投影进视图：{view}"
        );
    }

    /// 有类型化 Diff 元素时视图带 `diff`；`oldText`/`newText` 缺席时元素里也没有对应键
    /// （判定不出就是缺席，`MUST NOT` 以占位值代替）。
    #[test]
    fn view_attaches_diff_only_for_typed_elements() {
        let raw = document(RAW_WITH_DIFF);
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-1",
            "title": "编辑",
            "content": [{"type": "diff", "path": "src/main.rs", "oldText": "a", "newText": "b"}]
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "pending", &raw);
        let diff = view
            .get("diff")
            .and_then(Value::as_array)
            .expect("diff array");
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0]["path"], "src/main.rs");

        let raw = document(
            r#"{"params":{"update":{"sessionUpdate":"tool_call","content":[{"type":"diff","path":"new.txt","newText":"x"}]}}}"#,
        );
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-2",
            "title": "新建",
            "content": [{"type": "diff", "path": "new.txt", "newText": "x"}]
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "pending", &raw);
        let element = &view["diff"][0];
        assert_eq!(element["newText"], "x");
        assert!(
            element.get("oldText").is_none(),
            "新建文件的 oldText 缺席（不是空串、不是 null）：{element}"
        );
    }

    /// 切片扫描器不吃多字节字符：非 ASCII 的文件内容同样按字节切出**原文子串**。
    #[test]
    fn diff_slices_survive_multibyte_content() {
        let raw = document(
            r#"{"params":{"update":{"sessionUpdate":"tool_call","content":[{"type":"diff","path":"文档/说明.md","oldText":"第一行\n第二行","newText":"第一行\n第三行"}]}}}"#,
        );
        let elements = diff_elements(&raw);
        assert_eq!(elements.len(), 1);
        let slice = elements[0]["raw"].as_str().expect("raw");
        assert!(raw.as_str().contains(slice));
        assert!(slice.contains("说明.md"));
        // 解码后的取值是原文里的码点，不是被重写过的转义。
        assert_eq!(elements[0]["newText"], "第一行\n第三行");
    }
}
