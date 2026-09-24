//! `session/update` 的 11 种判别子、未知判别子的可见降级、以及结构化内容不被文本化。

mod support;

use acp_protocol::content::{ContentBlock, ToolCallContent};
use acp_protocol::error::AcpError;
use acp_protocol::message::{
    self, ElicitationAction, ElicitationMode, ElicitationResponse, PermissionOutcome,
    RequestPermissionResponse, config_value_boolean, config_value_id,
};
use acp_protocol::update::{SessionUpdate, ToolCall};
use acp_protocol::{Envelope, RawDocument};

/// 11 种判别各一份最小合法 payload。
fn minimal_payloads() -> Vec<(&'static str, serde_json::Value)> {
    let text_chunk = |discriminator: &str| {
        serde_json::json!({
            "sessionUpdate": discriminator,
            "content": { "type": "text", "text": "hi" },
            "messageId": "m1"
        })
    };
    vec![
        ("user_message_chunk", text_chunk("user_message_chunk")),
        ("agent_message_chunk", text_chunk("agent_message_chunk")),
        ("agent_thought_chunk", text_chunk("agent_thought_chunk")),
        (
            "tool_call",
            serde_json::json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "t1",
                "title": "Edit",
                "kind": "edit",
                "status": "in_progress"
            }),
        ),
        (
            "tool_call_update",
            serde_json::json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "t1",
                "status": "completed"
            }),
        ),
        (
            "plan",
            serde_json::json!({
                "sessionUpdate": "plan",
                "entries": [
                    { "content": "第一步", "priority": "high", "status": "pending" }
                ]
            }),
        ),
        (
            "available_commands_update",
            serde_json::json!({
                "sessionUpdate": "available_commands_update",
                "availableCommands": [ { "name": "compact", "description": "压缩上下文" } ]
            }),
        ),
        (
            "current_mode_update",
            serde_json::json!({
                "sessionUpdate": "current_mode_update",
                "currentModeId": "ask"
            }),
        ),
        (
            "config_option_update",
            serde_json::json!({
                "sessionUpdate": "config_option_update",
                "configOptions": [
                    { "id": "thinking", "name": "Thinking", "type": "boolean", "currentValue": true }
                ]
            }),
        ),
        (
            "session_info_update",
            serde_json::json!({
                "sessionUpdate": "session_info_update",
                "title": "标题",
                "updatedAt": "2026-09-24T00:00:00Z"
            }),
        ),
        (
            "usage_update",
            serde_json::json!({
                "sessionUpdate": "usage_update",
                "used": 1234,
                "size": 200000,
                "cost": { "amount": 0.5, "currency": "USD" }
            }),
        ),
    ]
}

#[test]
fn all_eleven_known_discriminators_decode() {
    let payloads = minimal_payloads();
    assert_eq!(payloads.len(), 11, "第一阶段要求 11 种判别子全部可解码");
    for (discriminator, payload) in payloads {
        let update = SessionUpdate::decode(&payload)
            .unwrap_or_else(|error| panic!("{discriminator}: 解码失败 {error}"));
        assert_eq!(update.wire_value(), discriminator);
        assert!(!update.is_unknown(), "{discriminator} 是已知判别子");
        assert!(
            acp_protocol::methods::is_known_session_update(discriminator),
            "{discriminator} 必须在登记表里"
        );
    }
}

#[test]
fn unknown_discriminator_degrades_visibly_and_keeps_payload() {
    let payload = serde_json::json!({
        "sessionUpdate": "example.com/future_update",
        "payload": { "value": 9007199254740993u64 },
        "_meta": { "example.com/note": "future" }
    });
    let update = SessionUpdate::decode(&payload).expect("未知判别子不是错误");
    match update {
        SessionUpdate::Unknown {
            ref wire_value,
            ref payload,
        } => {
            assert_eq!(wire_value, "example.com/future_update");
            assert_eq!(
                payload["payload"]["value"],
                serde_json::json!(9007199254740993u64)
            );
            assert_eq!(payload["_meta"]["example.com/note"], "future");
        }
        ref other => panic!("期望 Unknown，实际 {other:?}"),
    }
}

#[test]
fn known_discriminator_missing_required_field_is_rejected() {
    // agent_message_chunk 缺 `content`：结构错误，不能被当作可见降级吞掉。
    let error =
        SessionUpdate::decode(&serde_json::json!({ "sessionUpdate": "agent_message_chunk" }))
            .expect_err("缺 required 字段必须失败");
    assert!(
        matches!(error, AcpError::InvalidField { .. }),
        "实际 {error}"
    );

    // 连判别子都没有。
    let error = SessionUpdate::decode(&serde_json::json!({ "content": {} }))
        .expect_err("缺 sessionUpdate 必须失败");
    assert!(matches!(error, AcpError::MissingField { ref field } if field == "sessionUpdate"));

    // tool_call 缺 toolCallId。
    let error = SessionUpdate::decode(&serde_json::json!({
        "sessionUpdate": "tool_call",
        "title": "no id"
    }))
    .expect_err("tool_call 缺 toolCallId 必须失败");
    assert!(matches!(error, AcpError::InvalidField { .. }));
}

#[test]
fn tool_call_content_stays_structured() {
    let payload = serde_json::json!({
        "sessionUpdate": "tool_call",
        "toolCallId": "t1",
        "title": "Run tests",
        "content": [
            { "type": "content", "content": { "type": "text", "text": "output" } },
            { "type": "diff", "path": "a.txt", "oldText": "a", "newText": "b" },
            { "type": "terminal", "terminalId": "term-1" },
            { "type": "example.com/future_content", "value": 1 }
        ],
        "locations": [ { "path": "a.txt", "line": 12 } ]
    });
    let update = SessionUpdate::decode(&payload).expect("解码");
    let SessionUpdate::ToolCall(ToolCall {
        content, locations, ..
    }) = update
    else {
        panic!("期望 tool_call");
    };
    let content = content.expect("content");
    assert_eq!(content.len(), 4);
    assert!(matches!(content[0], ToolCallContent::Content(_)));
    assert!(matches!(content[1], ToolCallContent::Diff(_)));
    assert!(matches!(content[2], ToolCallContent::Terminal(_)));
    // 未登记的 tool content 也走可见降级，而不是被丢弃或被文本化。
    match &content[3] {
        ToolCallContent::Unknown { wire_type, payload } => {
            assert_eq!(wire_type, "example.com/future_content");
            assert_eq!(payload["value"], serde_json::json!(1));
        }
        other => panic!("期望 Unknown，实际 {}", other.wire_type()),
    }
    assert_eq!(locations.expect("locations")[0].line, Some(12));

    // diff 的字段可读且仍是结构：没有被拼成一段文本。
    let ToolCallContent::Diff(diff) = &content[1] else {
        unreachable!()
    };
    assert_eq!(diff.path, "a.txt");
    assert_eq!(diff.new_text, "b");
}

#[test]
fn text_and_structured_blocks_are_distinguishable() {
    let text: ContentBlock =
        serde_json::from_value(serde_json::json!({ "type": "text", "text": "hello" }))
            .expect("文本块");
    assert!(text.is_text());
    assert_eq!(text.text(), Some("hello"));
    assert_eq!(text.wire_type(), "text");

    let unknown: ContentBlock = serde_json::from_value(serde_json::json!({
        "type": "example.com/future_block",
        "anything": [1, 2]
    }))
    .expect("未知块必须降级而不是报错");
    let (wire_type, payload) = unknown.unknown_payload().expect("可见降级");
    assert_eq!(wire_type, "example.com/future_block");
    assert_eq!(payload["anything"][1], serde_json::json!(2));

    // 缺 type 是结构错误。
    assert!(serde_json::from_value::<ContentBlock>(serde_json::json!({ "text": "x" })).is_err());
}

#[test]
fn interaction_responses_keep_the_original_identifier() {
    // 权限：回传必须带上 Agent 给的 optionId 原文，用 cancelled 表示取消。
    let selected = RequestPermissionResponse {
        outcome: PermissionOutcome::Selected {
            option_id: "allow-once".to_owned(),
        },
        meta: None,
    };
    assert_eq!(
        serde_json::to_value(&selected).expect("序列化"),
        serde_json::json!({ "outcome": { "outcome": "selected", "optionId": "allow-once" } })
    );
    let cancelled = RequestPermissionResponse {
        outcome: PermissionOutcome::Cancelled,
        meta: None,
    };
    assert_eq!(
        serde_json::to_value(&cancelled).expect("序列化"),
        serde_json::json!({ "outcome": { "outcome": "cancelled" } })
    );

    // elicitation：form 与 url 两种模式都能辨认，作动作为封闭词表。
    let form = message::ElicitationRequest::from_params(&serde_json::json!({
        "message": "需要名字",
        "mode": "form",
        "requestedSchema": { "type": "object", "properties": {} }
    }))
    .expect("form 模式");
    assert!(matches!(form.mode, ElicitationMode::Form { .. }));
    let url = message::ElicitationRequest::from_params(&serde_json::json!({
        "message": "打开链接",
        "mode": "url",
        "elicitationId": "e1",
        "url": "https://example.com"
    }))
    .expect("url 模式");
    assert!(matches!(url.mode, ElicitationMode::Url { .. }));

    let response = ElicitationResponse {
        action: ElicitationAction::Decline,
        content: None,
    };
    assert_eq!(
        response.to_result(),
        serde_json::json!({ "action": "decline" })
    );
}

#[test]
fn config_values_are_built_in_the_declared_shapes() {
    assert_eq!(
        config_value_boolean(true),
        serde_json::json!({ "type": "boolean", "value": true })
    );
    assert_eq!(
        config_value_id("gpt-5"),
        serde_json::json!({ "value": "gpt-5" })
    );
}

#[test]
fn notification_decoding_uses_the_declared_direction() {
    // `session/update` 的 params 解成通知；缺 sessionId 时明确报缺字段。
    let envelope = Envelope::classify(
        RawDocument::parse(
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"session_info_update","title":"t"}}}"#,
        )
        .expect("合法消息"),
    )
    .expect("分类");
    let error = message::session_notification(&envelope).expect_err("缺 sessionId 必须失败");
    assert!(matches!(error, AcpError::MissingField { ref field } if field == "sessionId"));

    // 方向不符时是方向错误，而不是「看不到方法」。
    let tool_call_update = Envelope::classify(
        RawDocument::parse(
            r#"{"jsonrpc":"2.0","id":1,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"t"},"options":[]}}"#,
        )
        .expect("合法消息"),
    )
    .expect("分类");
    let request: message::RequestPermissionRequest =
        serde_json::from_value(tool_call_update.params().expect("params")).expect("权限请求可解码");
    assert_eq!(request.session_id, "s");
    assert_eq!(request.tool_call.tool_call_id, "t");
    assert!(request.options.is_empty());
}
