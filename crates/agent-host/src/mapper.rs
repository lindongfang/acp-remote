//! ACP 事件到 core 事件的映射。
//!
//! 三条约束：
//!
//! 1. **结构化不文本化**：tool call、diff、terminal、plan、commands 都按结构放进 `view`，
//!    同时把**逐字节原文**放进 `payload.acp`（`docs/SYNC_PROTOCOL.md` §10.3 的 `rawJson`）。
//!    工具调用内容里的类型化 `Diff` 元素额外投影成 view 的 `diff` 键（见 [`diff_elements`]）：
//!    core 的文件改动派生**只能**来自它，因此这里既不改写 ACP 原文，也**绝不**从自由形状的
//!    `rawInput` 推断文件改动。工具调用的 `rawInput`/`rawOutput` 与文本 content 另行投影成可选
//!    `toolDetail`（见 [`attach_tool_detail`]），预算受 `docs/SYNC_PROTOCOL.md` §14 限制。
//! 2. **turn 归属交给 core**：`EndpointEvent.turn` 一律 `None`，由 broker 用当前派发的 turn 补齐
//!    （`SessionEndpoint::prompt` 的签名不携带 `TurnId`）。
//! 3. **只有 core 真会消费的字段是硬要求**：`interactionId`（交互配对）与 `messageId`/`deltaIndex`/
//!    `text`（delta 折叠）。`turnId`/`version` 在适配器侧不可知，因此不出现在 view 里——
//!    见本变更 `verification.md` 的 Check Plan Changes。

use std::collections::HashMap;

use acp_core::model::{
    AcpRaw, Digest, DispatchContext, EchoAssociation, EndpointEvent, EventKind, EventPayload,
    EventType, InteractionOption, JsonValueText, MessageId, PublicError, Timestamp, UserEchoEvent,
    UserEchoLink, ViewJson, associate_user_echo,
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
    active: Option<(String, MessageId)>,
    counters: HashMap<MessageId, u64>,
    turn_seq: u64,
}

impl UpdateState {
    /// 新 turn 开始时重置消息分组（新 turn 的流是新消息）。
    pub fn begin_turn(&mut self) {
        self.active = None;
        self.counters.clear();
        self.turn_seq += 1;
    }

    fn message_for(&mut self, key: &str, ids: &dyn IdGenerator) -> MessageId {
        if let Some((active_key, existing)) = &self.active {
            if active_key == key {
                return existing.clone();
            }
        }
        let id = ids.message_id();
        self.active = Some((key.to_owned(), id.clone()));
        id
    }

    /// A visible non-message entry terminates the current continuous fragment.
    pub fn interrupt(&mut self) {
        self.active = None;
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
///
/// `dispatch` 是**本次 prompt 调用**的派发上下文（`design.md` D7），只在 `user.message.delta` 上使用：
/// 可证明与已保存首条内容一致的回显**不**再产出一条用户消息事件（见 [`user_echo_is_duplicate`]）。
/// 它不进入任何事件视图或 wire 字段；普通 prompt 传 `None`。
pub fn update_events(
    update: &SessionUpdate,
    raw: &RawDocument,
    state: &mut UpdateState,
    ids: &dyn IdGenerator,
    clock: &dyn Clock,
    dispatch: Option<&DispatchContext>,
) -> Result<Vec<EndpointEvent>, HostError> {
    let acp = Some(acp_raw(raw)?);
    let at = clock.now();
    let mut events = Vec::new();

    if !matches!(
        update,
        SessionUpdate::UserMessageChunk(_)
            | SessionUpdate::AgentMessageChunk(_)
            | SessionUpdate::AgentThoughtChunk(_)
    ) {
        state.interrupt();
    }
    match update {
        SessionUpdate::UserMessageChunk(chunk) => {
            // 重复回显：持久化的首条正文才是权威。这条既不产出事件，也不占用该消息的 delta 状态
            // （`messageId`/`deltaIndex`），被丢弃的回显因此不会在后续片段里留下空洞。
            if !user_echo_is_duplicate(
                dispatch,
                update_content_literal(raw),
                chunk.message_id.as_deref(),
            ) {
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

/// 本次派发的用户输入回显是否可证明是**已保存输入的重复**（`design.md` D7）。
///
/// 关联只用派发上下文与事件自身：`UserEchoLink` 的八个固定字段取自该次调用传入的上下文，内容数组与
/// 摘要则由这条 `user_message_chunk` 的**原文**内容块按首条输入的同一口径算出（`[<block 原文>]` 的
/// UTF-8 字节 SHA-256 → 无填充 base64url），再交给唯一规则 [`associate_user_echo`] 判定。
///
/// 返回 `true`（`EchoAssociation::Duplicate`）时调用方**不产出**这条 `user.message.delta`：持久消息
/// 正文才是权威，回显不能再造第二条用户消息。其余两种结论照常下发：
///
/// - `Differs`：已登记的事件词表里没有「Agent 来源的用户输入事件」（`EndpointEvent` 不携带
///   `EventOrigin`，唯一被登记的映射仍是 `user_message_chunk` → `user.message.delta`），因此不发明新键、
///   也不改写判别子映射；该回显保留为独立的用户输入事件，由 WP5 的投影决定展示方式。
/// - `Unattributed`：没有上下文、取不到原文或摘要算不出，保留为有来源说明的独立事件，绝不吞掉或
///   改写首条。
///
/// 已知边界（不新增 wire 字段的代价）：摘要按**逐字节**比较，且以内容块的原文为准。Agent 把首条的
/// 一个文本块拆成多个 chunk、或对块重新编码（键序/转义/附加字段变化）时，单块摘要不会等于整条输入的
/// 摘要，结论是 `Differs`——宁可多显示一条独立回显，也绝不错删内容。
fn user_echo_is_duplicate(
    dispatch: Option<&DispatchContext>,
    content_literal: Option<&str>,
    agent_message_id: Option<&str>,
) -> bool {
    let Some(context) = dispatch else {
        return false;
    };
    let Some(content_literal) = content_literal else {
        return false;
    };
    let Ok(echo) = user_echo_event(context, content_literal, agent_message_id) else {
        // 内容/摘要不可构造：不猜关联，照常下发。
        return false;
    };
    matches!(
        associate_user_echo(Some(context), &echo),
        EchoAssociation::Duplicate { .. }
    )
}

/// 由派发上下文与一条用户输入回显的**原文**构造 [`UserEchoEvent`]。
///
/// `link` 的全部固定字段来自**本次调用**的上下文（会话/首条/用户消息/请求/turn/Agent 身份/资源代），
/// `agent_message` 是 Agent 自己给这条回显的标识（不存在或不是本实现的 `MessageId` 形状时为 `None`；
/// 它从不作为匹配依据）。内容数组与摘要与 [`acp_core::model::FirstSubmissionInput`] 同源口径：内容块
/// 原文按顺序以 `,` 连接、外层加 `[...]`，摘要对同一份字节计算。
fn user_echo_event(
    context: &DispatchContext,
    content_literal: &str,
    agent_message_id: Option<&str>,
) -> Result<UserEchoEvent, HostError> {
    let json = format!("[{content_literal}]");
    let content = JsonValueText::new(&json).map_err(|_| HostError::IdUnavailable)?;
    let digest = Sha256::digest(json.as_bytes());
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    let content_digest = Digest::new(&encoded).map_err(|_| HostError::IdUnavailable)?;
    Ok(UserEchoEvent {
        link: Some(UserEchoLink {
            session: context.session.clone(),
            user_message: context.user_message.clone(),
            request: context.request.clone(),
            turn: context.turn.clone(),
            agent: context.agent.clone(),
            agent_session: context.agent_session.clone(),
            resource_generation: context.resource_generation,
        }),
        agent_message: agent_message_id.and_then(|id| MessageId::new(id).ok()),
        content,
        content_digest,
    })
}

/// 一条 `session/update` 通知里 `params.update.content` 成员的**原文**（内容块的逐字节载体）。
fn update_content_literal(raw: &RawDocument) -> Option<&str> {
    let update = member_literal(raw.as_str(), "params")
        .and_then(|params| member_literal(params, "update"))?;
    member_literal(update, "content")
}

/// 一个 ACP 内容块的可序列化 JSON（未登记类型用原始 payload：那是「可见降级」而不是降成 `null`）。
fn content_block_json(content: &ContentBlock) -> Option<Value> {
    match content {
        ContentBlock::Unknown { payload, .. } => Some(payload.clone()),
        other => serde_json::to_value(other).ok(),
    }
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
        if let (Some(block), Some(object)) = (content_block_json(content), view.as_object_mut()) {
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
    attach_tool_detail(
        &mut view,
        tool.raw_input.as_ref(),
        tool.raw_output.as_ref(),
        tool.content.as_deref(),
    );
    view
}

fn tool_call_update_view(update: &ToolCallUpdate, default_state: &str, raw: &RawDocument) -> Value {
    let mut view = json!({
        "toolCallId": update.tool_call_id,
        "title": update.title.clone().unwrap_or_default(),
        "state": update.status.clone().unwrap_or_else(|| default_state.to_owned()),
    });
    attach_diffs(&mut view, update.content.as_deref(), raw);
    attach_tool_detail(
        &mut view,
        update.raw_input.as_ref(),
        update.raw_output.as_ref(),
        update.content.as_deref(),
    );
    view
}

/// `toolDetail` 每个部分的固定上限（`docs/SYNC_PROTOCOL.md` §14）：32 KiB，按 JSON 序列化后的
/// UTF-8 字节计。
const TOOL_DETAIL_PART_LIMIT: usize = 32 * 1024;

/// `toolDetail` 整体的固定上限（§14）：64 KiB。
const TOOL_DETAIL_TOTAL_LIMIT: usize = 64 * 1024;

/// ACPR-CJ1 允许的最大嵌套深度（容器层数，顶层容器记 1）。与 `acpr-wire` 的
/// `cj1::MAX_DEPTH` 同值；`agent-host` 不依赖那个 crate，这里保留同值常量。
const CJ1_MAX_DEPTH: u16 = 128;

/// ACPR-CJ1 允许的最大整数绝对值（2^53-1，即 JS 的 `Number.MAX_SAFE_INTEGER`）。
const CJ1_MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// `toolDetail` 的某个部分在 `payload.view` 里的容器深度：`view` 记 1、`toolDetail` 记 2、
/// 部分本身记 3。判定从 3 起算，把外层两级一并计入 128 的预算——宁可少带一个值，也不让整条
/// 事件因 `view` 的 ACPR-CJ1 校验失败而无法提交。
const TOOL_DETAIL_VALUE_DEPTH: u16 = 3;

/// 值能否以 ACPR-CJ1 的值域表达（判定口径与 `crates/acpr-wire/src/cj1.rs` 一致）。
///
/// `payload.view` 落库前要过 ACPR-CJ1：数字必须有限、无小数部分且 `|n| <= 2^53-1`，容器嵌套不超过
/// 128 层（本模块不依赖 `acpr-wire`，因此就地实现，不新增 crate 依赖）。
///
/// `depth` 是 `value` **作为容器**时所在的容器层数；标量不占用新的一层，也不受深度限制。
fn cj1_compatible(value: &Value, depth: u16) -> bool {
    match value {
        Value::Object(members) => {
            depth <= CJ1_MAX_DEPTH
                && members
                    .values()
                    .all(|member| cj1_compatible(member, depth + 1))
        }
        Value::Array(items) => {
            depth <= CJ1_MAX_DEPTH && items.iter().all(|item| cj1_compatible(item, depth + 1))
        }
        Value::Number(number) => cj1_compatible_number(number),
        Value::String(_) | Value::Bool(_) | Value::Null => true,
    }
}

/// 单个 JSON 数字是否落在 ACPR-CJ1 值域内：按**值**判定，与 `cj1` 的「先取 f64 再要求有限、整数、
/// 在 2^53-1 内」一致（因此 `1.0`/`1e2` 合法，`0.2`/`1e400`/`9007199254740992` 不合法）。
fn cj1_compatible_number(number: &serde_json::Number) -> bool {
    if let Some(value) = number.as_i64() {
        return value.unsigned_abs() <= CJ1_MAX_SAFE_INTEGER;
    }
    if let Some(value) = number.as_u64() {
        return value <= CJ1_MAX_SAFE_INTEGER;
    }
    number.as_f64().is_some_and(|value| {
        value.is_finite() && value.fract() == 0.0 && value.abs() <= CJ1_MAX_SAFE_INTEGER as f64
    })
}

/// 把 ACP 工具调用的输入、输出与文本内容挂到 view 的 `toolDetail` 键上。
///
/// 只读**类型化**字段：`rawInput`、`rawOutput`，以及 content 里 `Content` 包装的 `Text` 块。
/// Diff、terminal 与未识别的 content 不进入这里：差异行由 [`attach_diffs`] 与 core 的派生承担，
/// 终端输出走 `terminal.output`，未识别类型仍可经 `payload.acp` 的原文诊断查看。
///
/// 预算按 §14 执行（均按 JSON 序列化后的 UTF-8 字节）：`input`/`output` 各 32 KiB、`text` 合计
/// 32 KiB、整个对象 64 KiB。超过部分上限的部分**整体省略**；整体预算不足时按 `output` → `text`
/// → `input` 的优先序保留（结果与内容优先，输入通常已由 `tool.call.started` 下发），让出的部分
/// 同样整体省略。被省略的部分名登记在 `truncatedParts`（按 `input`/`output`/`text` 的固定顺序），
/// **绝不**发送被截断的 JSON 片段。
///
/// 另有值域约束：`input`/`output` 是自由 JSON，可能含浮点或超出 2^53-1 的整数；`payload.view` 落库前
/// 要过 ACPR-CJ1，因此无法以该值域表达的部分（见 [`cj1_compatible`]）同样**整体缺席**并登记，
/// 绝不让整条事件因 view 校验失败而无法提交。`text` 是字符串，不需要数值判定。
///
/// 完全没有详情时整个 `toolDetail` 键缺席。
fn attach_tool_detail(
    view: &mut Value,
    raw_input: Option<&Value>,
    raw_output: Option<&Value>,
    content: Option<&[ToolCallContent]>,
) {
    let mut input = None;
    let mut output = None;
    let mut text = None;
    let mut truncated = [false; 3];

    if let Some(value) = raw_input {
        if !cj1_compatible(value, TOOL_DETAIL_VALUE_DEPTH) {
            // 无法以 ACPR-CJ1 值域表达：整体省略并登记，避免整个 payload 校验失败。
            truncated[0] = true;
        } else if value_json_len(value) <= TOOL_DETAIL_PART_LIMIT {
            input = Some(value);
        } else {
            truncated[0] = true;
        }
    }
    if let Some(value) = raw_output {
        if !cj1_compatible(value, TOOL_DETAIL_VALUE_DEPTH) {
            truncated[1] = true;
        } else if value_json_len(value) <= TOOL_DETAIL_PART_LIMIT {
            output = Some(value);
        } else {
            truncated[1] = true;
        }
    }
    if let Some(blocks) = text_contents(content) {
        let bytes: usize = blocks.iter().map(|block| text_json_len(block)).sum();
        if bytes <= TOOL_DETAIL_PART_LIMIT {
            text = Some(blocks);
        } else {
            truncated[2] = true;
        }
    }

    loop {
        let Some(candidate) = tool_detail_json(input, output, text.as_deref(), truncated) else {
            return;
        };
        if value_json_len(&candidate) <= TOOL_DETAIL_TOTAL_LIMIT {
            insert_tool_detail(view, candidate);
            return;
        }
        // 整体预算不足：先牺牲输入，再牺牲文本，最后才牺牲输出（每个让出的部分都登记）。
        if input.is_some() {
            input = None;
            truncated[0] = true;
        } else if text.is_some() {
            text = None;
            truncated[2] = true;
        } else if output.is_some() {
            output = None;
            truncated[1] = true;
        } else {
            // 只剩 `truncatedParts`：仍如实下发，说明数据存在但超过整体预算。
            insert_tool_detail(view, candidate);
            return;
        }
    }
}

/// 组装 `toolDetail` 对象；三个内容部分都缺席且没有截断登记时返回 `None`（整个键缺席）。
fn tool_detail_json(
    input: Option<&Value>,
    output: Option<&Value>,
    text: Option<&[String]>,
    truncated: [bool; 3],
) -> Option<Value> {
    let mut object = serde_json::Map::new();
    if let Some(value) = input {
        object.insert("input".to_owned(), value.clone());
    }
    if let Some(value) = output {
        object.insert("output".to_owned(), value.clone());
    }
    if let Some(blocks) = text {
        object.insert(
            "text".to_owned(),
            Value::Array(blocks.iter().cloned().map(Value::String).collect()),
        );
    }
    let parts: Vec<Value> = ["input", "output", "text"]
        .into_iter()
        .zip(truncated)
        .filter(|(_, truncated)| *truncated)
        .map(|(name, _)| Value::String(name.to_owned()))
        .collect();
    if !parts.is_empty() {
        object.insert("truncatedParts".to_owned(), Value::Array(parts));
    }
    (!object.is_empty()).then_some(Value::Object(object))
}

/// content 里的文本块，按 Agent 提供的顺序收集；没有文本块时返回 `None`（不是空数组）。
///
/// 只取 `ToolCallContent::Content` 包装的 `ContentBlock::Text`：其它 content block（图片、资源、
/// 未识别）与 `Diff`/`Terminal` 元素都不文本化。
fn text_contents(content: Option<&[ToolCallContent]>) -> Option<Vec<String>> {
    let mut texts = Vec::new();
    for element in content? {
        if let ToolCallContent::Content(wrapper) = element {
            if let ContentBlock::Text(text) = &wrapper.content {
                texts.push(text.text.clone());
            }
        }
    }
    (!texts.is_empty()).then_some(texts)
}

fn insert_tool_detail(view: &mut Value, detail: Value) {
    if let Some(object) = view.as_object_mut() {
        object.insert("toolDetail".to_owned(), detail);
    }
}

/// JSON 值的序列化字节数；序列化失败按「超限」处理（宁可省略也不发送无法保真的值）。
fn value_json_len(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

/// 一个文本块按 JSON 字符串序列化后的字节数（含引号与转义），用于 `text` 的合计预算。
fn text_json_len(text: &str) -> usize {
    serde_json::to_vec(text).map_or(usize::MAX, |bytes| bytes.len())
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

/// 一条 `terminal.output` 事件（`SYNC_PROTOCOL.md` §10.3 的字段：`terminalId`、十进制 `chunkIndex`、
/// 可区分来源的 `stream`、`text` 与 `truncated`）。
///
/// `text` 是**本次读取**的真实内容（≤ `limits::TERMINAL_CHUNK_BYTES`），因此这里的 `truncated`
/// 永远是 `false`：截断只发生在 `terminal/output` 的**保留正文**上（由响应里的 `truncated` 表达），
/// 不会影响事件本身，也不会把事件内容裁成半截。`terminalId` 是工具调用内容里 `{type:"terminal"}`
/// 引用的同一个标识，客户端据此把输出挂到对应工具卡片上。
pub fn terminal_output_event(
    terminal_id: &str,
    chunk_index: u64,
    stream: &str,
    text: &str,
    truncated: bool,
    at: Timestamp,
) -> Result<EndpointEvent, HostError> {
    view_event(
        EventKind::Delta,
        "terminal.output",
        json!({
            "terminalId": terminal_id,
            "chunkIndex": chunk_index.to_string(),
            "stream": stream,
            "text": text,
            "truncated": truncated,
        }),
        None,
        at,
    )
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
    let mut view = json!({
        "interactionId": interaction_id.as_str(),
        "title": tool_call.title.clone().unwrap_or_default(),
        "description": tool_call.name.clone().unwrap_or_default(),
        "options": options,
        "toolCallId": tool_call.tool_call_id,
    });
    if let Some(kind) = &tool_call.kind {
        view["toolKind"] = json!(kind);
    }
    attach_tool_detail(
        &mut view,
        tool_call.raw_input.as_ref(),
        None,
        tool_call.content.as_deref(),
    );
    view_event(
        EventKind::Interaction,
        "permission.requested",
        view,
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

    #[test]
    fn permission_carries_approval_context() {
        let tool: ToolCallUpdate = serde_json::from_value(json!({
            "toolCallId": "approval-tool", "kind": "execute", "title": "Check workspace",
            "rawInput": { "command": "pwd", "cwd": "/workspace" },
            "content": [{ "type": "content", "content": { "type": "text", "text": "Show working directory" } }]
        }))
        .expect("tool update");
        let event = permission_event(
            &acp_core::model::InteractionId::new("e0000000-0000-4000-8000-000000000501")
                .expect("interaction"),
            &tool,
            &[],
            None,
            Timestamp::new("2026-10-24T00:00:00.000Z").expect("timestamp"),
        )
        .expect("permission event");
        let view: Value = serde_json::from_str(event.payload.view.as_str()).expect("view");
        assert_eq!(view["toolKind"], "execute");
        assert_eq!(view["toolCallId"], "approval-tool");
        assert_eq!(view["toolDetail"]["input"]["command"], "pwd");
        assert_eq!(view["toolDetail"]["input"]["cwd"], "/workspace");
        assert_eq!(view["toolDetail"]["text"][0], "Show working directory");
        assert!(view["toolDetail"].get("output").is_none());
    }

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

    /// `tool.call.started`：`rawInput` 原样进入 `toolDetail.input`，不发明其它部分。
    #[test]
    fn tool_detail_carries_started_input() {
        let raw =
            document(r#"{"params":{"update":{"sessionUpdate":"tool_call","toolCallId":"t-1"}}}"#);
        let before = raw.as_str().to_owned();
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-1",
            "title": "读取 schema",
            "rawInput": {"path": "schemas/sync/v1/event-views.schema.json", "limit": 20}
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "pending", &raw);
        let detail = view.get("toolDetail").expect("toolDetail");
        assert_eq!(
            detail["input"]["path"],
            "schemas/sync/v1/event-views.schema.json"
        );
        assert_eq!(detail["input"]["limit"], 20);
        assert!(detail.get("output").is_none(), "没有 rawOutput 时该键缺席");
        assert!(
            detail.get("text").is_none(),
            "没有文本 content 时该键缺席（不是空数组）"
        );
        assert!(
            detail.get("truncatedParts").is_none(),
            "未超限时 truncatedParts 缺席（不是空数组）"
        );
        assert_eq!(view["state"], "pending");
        assert_eq!(raw.as_str(), before, "携带 toolDetail 不改变 ACP 原文");
    }

    /// `tool.call.completed`：`rawOutput` 与文本块都进入 `toolDetail`，文本顺序与 Agent 一致；
    /// diff 与 terminal 元素不进入（它们由既有派生与 `terminal.output` 承担）。
    #[test]
    fn tool_detail_carries_completed_output_and_text() {
        let raw = document(
            r#"{"params":{"update":{"sessionUpdate":"tool_call_update","toolCallId":"t-2"}}}"#,
        );
        let update: ToolCallUpdate = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-2",
            "title": "编译",
            "status": "completed",
            "rawOutput": {"exitCode": 0, "stdout": "ok"},
            "content": [
                {"type": "content", "content": {"type": "text", "text": "第一段"}},
                {"type": "diff", "path": "src/main.rs", "newText": "x"},
                {"type": "terminal", "terminalId": "term-1"},
                {"type": "content", "content": {"type": "text", "text": "第二段"}}
            ]
        }))
        .expect("tool call update");
        let view = tool_call_update_view(&update, "in_progress", &raw);
        let detail = view.get("toolDetail").expect("toolDetail");
        assert_eq!(detail["output"]["exitCode"], 0);
        assert_eq!(detail["output"]["stdout"], "ok");
        assert_eq!(
            detail["text"],
            serde_json::json!(["第一段", "第二段"]),
            "文本块顺序与 Agent 提供的一致"
        );
        assert!(detail.get("input").is_none(), "没有 rawInput 时该键缺席");
        assert!(detail.get("truncatedParts").is_none());
        assert_eq!(view["state"], "completed");
    }

    /// `rawInput` 含浮点（自由 JSON 允许）：无法以 ACPR-CJ1 值域表达，`input` 整体缺席并登记，
    /// 而不是把会令整条事件落库失败的值放进 view。
    #[test]
    fn tool_detail_omits_fractional_input() {
        let raw = document(r#"{"params":{"update":{"sessionUpdate":"tool_call"}}}"#);
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-float",
            "title": "比例",
            "rawInput": {"ratio": 0.2}
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "pending", &raw);
        let detail = view.get("toolDetail").expect("截断登记仍须下发");
        assert!(
            detail.get("input").is_none(),
            "非整数无法以 ACPR-CJ1 表达，必须整体缺席：{detail}"
        );
        assert_eq!(detail["truncatedParts"], serde_json::json!(["input"]));
        assert!(
            !view.to_string().contains("ratio"),
            "不得下发无法保真的输入片段"
        );
    }

    /// `rawInput` 含超出 2^53-1 的整数：同样整体缺席并登记 `input`。
    #[test]
    fn tool_detail_omits_integer_outside_cj1_range() {
        let raw = document(r#"{"params":{"update":{"sessionUpdate":"tool_call"}}}"#);
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-big-int",
            "title": "大整数",
            "rawInput": {"id": 9007199254740992_i64}
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "pending", &raw);
        let detail = view.get("toolDetail").expect("截断登记仍须下发");
        assert!(
            detail.get("input").is_none(),
            "超过 2^53-1 的整数必须整体缺席：{detail}"
        );
        assert_eq!(detail["truncatedParts"], serde_json::json!(["input"]));
        assert!(
            !view.to_string().contains("9007199254740992"),
            "不得下发无法保真的输入片段"
        );
    }

    /// 边界内的值仍保留：`2^53-1`、整数值浮点与嵌套容器都通过 CJ1 判定。
    #[test]
    fn tool_detail_keeps_cj1_compatible_input() {
        let raw = document(r#"{"params":{"update":{"sessionUpdate":"tool_call"}}}"#);
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-cj1-ok",
            "title": "边界内",
            "rawInput": {
                "maxSafe": 9007199254740991_i64,
                "negativeMaxSafe": -9007199254740991_i64,
                "integral": 1.0,
                "nested": [1, {"zero": 0}]
            }
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "pending", &raw);
        let detail = view.get("toolDetail").expect("toolDetail");
        assert_eq!(detail["input"]["maxSafe"], 9007199254740991_i64);
        assert_eq!(detail["input"]["negativeMaxSafe"], -9007199254740991_i64);
        assert_eq!(detail["input"]["integral"], 1.0);
        assert_eq!(
            detail["input"]["nested"],
            serde_json::json!([1, {"zero": 0}])
        );
        assert!(detail.get("truncatedParts").is_none());
    }

    /// 嵌套超过 ACPR-CJ1 的 128 层上限：整体缺席并登记 `input`（不递归到栈溢出）。
    #[test]
    fn tool_detail_omits_overdeep_input() {
        let raw = document(r#"{"params":{"update":{"sessionUpdate":"tool_call"}}}"#);
        let mut nested = serde_json::json!(1);
        for _ in 0..130 {
            nested = serde_json::Value::Array(vec![nested]);
        }
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-deep",
            "title": "过深",
            "rawInput": {"nested": nested}
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "pending", &raw);
        let detail = view.get("toolDetail").expect("截断登记仍须下发");
        assert!(
            detail.get("input").is_none(),
            "超过 128 层的输入必须整体缺席"
        );
        assert_eq!(detail["truncatedParts"], serde_json::json!(["input"]));
    }

    /// 单部分超限：该部分**整体缺席**并登记，其余未超限部分照常下发；不发送被截断的片段。
    #[test]
    fn tool_detail_omits_oversized_output_but_keeps_other_parts() {
        let raw = document(r#"{"params":{"update":{"sessionUpdate":"tool_call"}}}"#);
        let big = "x".repeat(33 * 1024);
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-big",
            "title": "大输出",
            "rawInput": {"query": "small"},
            "rawOutput": {"blob": big},
            "content": [{"type": "content", "content": {"type": "text", "text": "摘要"}}]
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "completed", &raw);
        let detail = view.get("toolDetail").expect("toolDetail");
        assert!(
            detail.get("output").is_none(),
            "超过 32 KiB 的输出必须整体缺席：{detail}"
        );
        assert_eq!(detail["truncatedParts"], serde_json::json!(["output"]));
        assert_eq!(detail["input"]["query"], "small", "未超限部分照常下发");
        assert_eq!(detail["text"], serde_json::json!(["摘要"]));
        assert!(
            !view.to_string().contains("xxxxxxxxxx"),
            "绝不发送被截断的 JSON 片段"
        );
    }

    /// 多个部分各自超限：`truncatedParts` 是并集（不是只登记第一个）。
    #[test]
    fn tool_detail_registers_every_oversized_part() {
        let raw = document(r#"{"params":{"update":{"sessionUpdate":"tool_call"}}}"#);
        let big = "x".repeat(33 * 1024);
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-big-2",
            "title": "大输入与大文本",
            "rawInput": {"blob": big.clone()},
            "rawOutput": {"ok": true},
            "content": [{"type": "content", "content": {"type": "text", "text": big}}]
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "completed", &raw);
        let detail = view.get("toolDetail").expect("toolDetail");
        assert!(detail.get("input").is_none());
        assert!(detail.get("text").is_none());
        assert_eq!(
            detail["truncatedParts"],
            serde_json::json!(["input", "text"])
        );
        assert_eq!(
            detail["output"]["ok"],
            serde_json::json!(true),
            "未超限的输出照常下发"
        );
    }

    /// 整体 64 KiB 预算：两个部分各自未超 32 KiB、合计超限时，按 `output` → `text` → `input`
    /// 的优先序保留，让出的输入整体缺席并登记。
    #[test]
    fn tool_detail_enforces_total_budget() {
        let raw = document(r#"{"params":{"update":{"sessionUpdate":"tool_call"}}}"#);
        // 序列化后各约 32 KiB（字符串长度 + `{"blob":…}` 包装），两项合计超过 64 KiB。
        let near = "y".repeat(32 * 1024 - 18);
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-total",
            "title": "接近上限",
            "rawInput": {"blob": near.clone()},
            "rawOutput": {"blob": near}
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "completed", &raw);
        let detail = view.get("toolDetail").expect("toolDetail");
        assert!(detail.get("input").is_none(), "整体预算不足时先让出输入");
        assert!(detail.get("output").is_some(), "输出优先保留");
        assert_eq!(detail["truncatedParts"], serde_json::json!(["input"]));
        assert!(
            value_json_len(detail) <= TOOL_DETAIL_TOTAL_LIMIT,
            "toolDetail 整体不得超过 64 KiB"
        );
    }

    /// 没有 `rawInput`/`rawOutput`/文本 content 时 `toolDetail` 整个键缺席，视图仍完整。
    #[test]
    fn tool_detail_absent_without_acp_detail() {
        let raw = document(r#"{"params":{"update":{"sessionUpdate":"tool_call"}}}"#);
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-plain",
            "title": "看文件",
            "content": [{"type": "terminal", "terminalId": "q"}]
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "pending", &raw);
        assert!(
            view.get("toolDetail").is_none(),
            "没有详情时不得出现空对象占位：{view}"
        );
        assert_eq!(view["toolCallId"], "t-plain", "视图本身不得缺失");

        let update: ToolCallUpdate = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-plain",
            "title": "看文件",
            "status": "completed"
        }))
        .expect("tool call update");
        let view = tool_call_update_view(&update, "completed", &raw);
        assert!(view.get("toolDetail").is_none());
    }

    /// 未识别的 content 类型不被文本化；可映射的文本块仍照常交付。
    #[test]
    fn tool_detail_ignores_unrecognized_content() {
        let raw = document(r#"{"params":{"update":{"sessionUpdate":"tool_call"}}}"#);
        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-unknown",
            "title": "未知内容",
            "content": [{"type": "mystery", "secret": "不可文本化"}]
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "pending", &raw);
        assert!(
            view.get("toolDetail").is_none(),
            "只有未识别 content 时不携带 toolDetail：{view}"
        );

        let tool: ToolCall = serde_json::from_value(serde_json::json!({
            "toolCallId": "t-mixed",
            "title": "混合内容",
            "content": [
                {"type": "mystery", "secret": "不可文本化"},
                {"type": "content", "content": {"type": "text", "text": "可读文本"}}
            ]
        }))
        .expect("tool call");
        let view = tool_call_view(&tool, "pending", &raw);
        let detail = view.get("toolDetail").expect("toolDetail");
        assert_eq!(detail["text"], serde_json::json!(["可读文本"]));
        assert!(
            !view.to_string().contains("不可文本化"),
            "未识别 content 不得被文本化或透传进 toolDetail"
        );
    }
}

#[cfg(test)]
mod order_tests {
    use super::*;
    use acp_core::model::{InteractionId, OriginEpoch, PairingId, RequestId, SessionId, TurnId};
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Ids(AtomicU64);
    impl IdGenerator for Ids {
        fn message_id(&self) -> MessageId {
            MessageId::new(&format!(
                "00000000-0000-4000-8000-{:012}",
                self.0.fetch_add(1, Ordering::Relaxed)
            ))
            .expect("id")
        }
        fn session_id(&self) -> SessionId {
            unreachable!()
        }
        fn turn_id(&self) -> TurnId {
            unreachable!()
        }
        fn interaction_id(&self) -> InteractionId {
            unreachable!()
        }
        fn pairing_id(&self) -> PairingId {
            unreachable!()
        }
        fn origin_epoch(&self) -> OriginEpoch {
            unreachable!()
        }
        fn request_id(&self) -> RequestId {
            unreachable!()
        }
    }
    struct FixedClock;
    impl Clock for FixedClock {
        fn now(&self) -> Timestamp {
            Timestamp::new("2026-10-24T00:00:00.000Z").expect("time")
        }
    }
    fn send(
        state: &mut UpdateState,
        ids: &Ids,
        kind: &str,
        external: Option<&str>,
    ) -> EndpointEvent {
        let payload = if kind == "tool_call" {
            json!({"sessionUpdate":kind,"toolCallId":"tool-1","title":"tool"})
        } else {
            let mut p = json!({"sessionUpdate":kind,"content":{"type":"text","text":"x"},"future":9007199254740993_u64});
            if let Some(id) = external {
                p["messageId"] = json!(id);
            }
            p
        };
        let raw = RawDocument::parse(payload.to_string()).expect("raw");
        let update = SessionUpdate::decode(&payload).expect("update");
        let event = update_events(&update, &raw, state, ids, &FixedClock, None)
            .expect("mapped")
            .remove(0);
        assert_eq!(
            event
                .payload
                .acp
                .as_ref()
                .expect("raw ACP")
                .as_available()
                .expect("available")
                .1,
            raw.as_str()
        );
        event
    }
    #[test]
    fn fragments_preserve_interleaving_with_and_without_external_ids() {
        for external in [Some("same-external-id"), None] {
            let mut state = UpdateState::default();
            let ids = Ids(AtomicU64::new(1));
            let mut views = Vec::new();
            for kind in [
                "agent_thought_chunk",
                "agent_thought_chunk",
                "tool_call",
                "agent_thought_chunk",
                "agent_message_chunk",
                "agent_thought_chunk",
            ] {
                let e = send(&mut state, &ids, kind, external);
                if kind != "tool_call" {
                    views.push(
                        serde_json::from_str::<Value>(e.payload.view.as_str()).expect("view"),
                    );
                }
            }
            assert_eq!(views[0]["messageId"], views[1]["messageId"]);
            assert_eq!(views[1]["deltaIndex"], "1");
            for index in 2..views.len() {
                assert_ne!(views[index - 1]["messageId"], views[index]["messageId"]);
                assert_eq!(views[index]["deltaIndex"], "0");
            }
            state.interrupt(); // permission/terminal emitted outside session/update
            let next = send(&mut state, &ids, "agent_thought_chunk", external);
            let next: Value = serde_json::from_str(next.payload.view.as_str()).expect("view");
            assert_ne!(next["messageId"], views.last().expect("last")["messageId"]);
        }
    }
}
