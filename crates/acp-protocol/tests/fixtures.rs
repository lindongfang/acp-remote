//! 夹具驱动：`fixtures/acp/v1/manifest.json` 的每条用例都必须按它在合同里的角色通过。
//!
//! 这份测试做两件事，而且分别断言：
//!
//! 1. **逐字节保真**：`RawDocument` 的原文与 fixture 的原始字节逐字节相等（不是「parse 后再
//!    序列化看起来一样」——那份断言在 `raw_fidelity.rs` 里另有反例）；
//! 2. **类型化解码**：已知方法能解出结构化内容，未知判别子显式可见降级，扩展方法显式不支持。
//!
//! 用例数与被执行的用例数都被断言，因此「漏跑」和「全跳过」都会让它失败。

mod support;

use acp_protocol::RawDocument;
use acp_protocol::content::ToolCallContent;
use acp_protocol::content::ToolCallLocation;
use acp_protocol::envelope::{Envelope, MessageClass};
use acp_protocol::error::AcpError;
use acp_protocol::message::{self, InitializeResponse};
use acp_protocol::methods::MethodDirection;
use acp_protocol::update::SessionUpdate;

/// manifest 当前登记的用例数；新增或删除 fixture 必须同步这里与 manifest。
const EXPECTED_CASES: usize = 8;
const EXPECTED_VALID: usize = 7;
const EXPECTED_INVALID: usize = 1;

#[test]
fn manifest_cases_all_pass_in_their_declared_role() {
    let cases = support::manifest_cases();
    assert_eq!(
        cases.len(),
        EXPECTED_CASES,
        "manifest 用例数变化必须同步本测试"
    );

    let mut valid = 0usize;
    let mut invalid = 0usize;
    for case in &cases {
        let bytes = support::fixture_bytes(&case.fixture);
        match (case.valid, case.schema_pointer.as_deref()) {
            (true, None) => {
                valid += 1;
                assert_message(&case.fixture, &bytes);
            }
            (true, Some(pointer)) => {
                valid += 1;
                assert_fragment(&case.fixture, pointer, &bytes, true);
            }
            (false, Some(pointer)) => {
                invalid += 1;
                assert_eq!(
                    case.expected_keyword.as_deref(),
                    Some("format"),
                    "上游对 line 的约束是 format: uint32，负例必须声明它"
                );
                assert_fragment(&case.fixture, pointer, &bytes, false);
            }
            (false, None) => panic!("{}：消息级负例在当前 fixture 集里不存在", case.fixture),
        }
    }
    assert_eq!(valid, EXPECTED_VALID, "valid 用例必须全部执行");
    assert_eq!(invalid, EXPECTED_INVALID, "invalid 用例必须全部执行");
}

/// 消息级 fixture：分类、逐字节保真、按期角色解码。
fn assert_message(name: &str, bytes: &[u8]) {
    let document = RawDocument::parse_bytes(bytes)
        .unwrap_or_else(|error| panic!("{name}: 必须是合法 ACP 消息，实际 {error}"));
    let envelope =
        Envelope::classify(document).unwrap_or_else(|error| panic!("{name}: 分类失败 {error}"));

    // 保真：再编码就是原文。
    assert_eq!(
        envelope.document().encode().as_bytes(),
        bytes,
        "{name}: 再编码必须与 fixture 的原始字节逐字节相等"
    );
    assert_eq!(envelope.document().len(), bytes.len());

    // 未知字段仍在原文里（既没被丢弃，也没被改写）。
    let value = envelope.document().value().expect("原文仍是合法 JSON");
    match name {
        "initialize-capabilities.json" => {
            assert_eq!(envelope.class(), MessageClass::Response);
            let response: InitializeResponse =
                message::decode_response(&envelope).expect("响应可解码");
            assert_eq!(response.protocol_version, 1);
            let capabilities = response.capabilities();
            assert!(capabilities.supports_prompt_image());
            assert!(!capabilities.supports_prompt_audio());
            assert!(capabilities.supports_embedded_context());
            assert!(capabilities.supports_load_session());
            assert!(capabilities.supports_session_list());
            assert!(capabilities.supports_session_resume());
            assert!(capabilities.supports_session_close());
            // 未识别的能力字段（`_meta` 与 ext）必须保留。
            let agent_caps = value["result"]["agentCapabilities"].clone();
            assert_eq!(
                agent_caps["_meta"]["example.com/agent-feature"],
                serde_json::json!(true)
            );
        }
        "tool-call-with-diff.json" => {
            assert_eq!(envelope.class(), MessageClass::Notification);
            let notification =
                message::session_notification(&envelope).expect("session/update 可解码");
            let SessionUpdate::ToolCall(tool_call) = notification.update else {
                panic!("{name}: 期望 tool_call");
            };
            assert_eq!(tool_call.tool_call_id, "tool_1");
            assert_eq!(tool_call.status.as_deref(), Some("completed"));
            let content = tool_call.content.expect("diff 必须在 content 里");
            assert_eq!(content.len(), 1);
            match &content[0] {
                ToolCallContent::Diff(diff) => {
                    assert_eq!(diff.path, "C:\\workspace\\example.txt");
                    assert_eq!(diff.old_text.as_deref(), Some("before\n"));
                    assert_eq!(diff.new_text, "after\n");
                }
                other => panic!("{name}: diff 不得被改写为其它结构（{}）", other.wire_type()),
            }
        }
        "meta-and-unknown-fields.json" => {
            assert_eq!(
                envelope.class(),
                MessageClass::Request,
                "带 id 的 session/update 结构上是 request，分类按结构判定"
            );
            let notification =
                message::session_notification(&envelope).expect("方向仍是 agent→client");
            let SessionUpdate::AgentMessageChunk(chunk) = notification.update else {
                panic!("{name}: 期望 agent_message_chunk");
            };
            assert_eq!(chunk.content.text(), Some("示例内容"));
            assert!(value["params"]["futureFieldFromNewerAcp"].is_object());
            assert_eq!(
                value["params"]["_meta"]["example.dev/traceId"],
                "b7c8d9ea-fb0c-4d1e-8f20-8192a3b4c5d6"
            );
            assert_eq!(value["_meta"]["example.dev/envelope"], "request level");
        }
        "unknown-fields-and-large-integer.json" => {
            let notification =
                message::session_notification(&envelope).expect("session/update 可解码");
            let SessionUpdate::AgentMessageChunk(chunk) = notification.update else {
                panic!("{name}: 期望 agent_message_chunk");
            };
            assert_eq!(chunk.content.text(), Some("raw preservation"));
            // 文字形式必须原样保留：1.2300 不能变成 1.23，超大整数不能降级成浮点。
            let text = envelope.document().encode();
            assert!(text.contains("1.2300"), "数字文本形式被改写：{text}");
            assert!(text.contains("9007199254740993"), "超大整数被改写：{text}");
        }
        "extension-method.json" => {
            assert_eq!(envelope.class(), MessageClass::Request);
            let error = envelope
                .ensure_direction(MethodDirection::AgentToClient)
                .expect_err("下划线扩展方法必须显式不支持");
            assert!(
                matches!(
                    error,
                    AcpError::Unsupported {
                        ref method,
                        status: acp_protocol::methods::MethodStatus::Extension,
                    } if method == "_extension/example"
                ),
                "{name}: 期望 Unsupported(Extension)，实际 {error}"
            );
        }
        "future-session-update.json" => {
            let notification =
                message::session_notification(&envelope).expect("session/update 可解码");
            assert!(
                notification.update.is_unknown(),
                "{name}: 未登记判别子必须可见降级而不是报错"
            );
            let SessionUpdate::Unknown {
                wire_value,
                ref payload,
            } = notification.update
            else {
                panic!("{name}: 未登记判别子必须可见降级而不是报错");
            };
            assert_eq!(wire_value, "example.com/future_update");
            assert_eq!(
                payload["payload"]["value"],
                serde_json::json!(9007199254740993u64)
            );
        }
        other => panic!("{other}: manifest 新增了消息级用例，但断言表没有同步"),
    }
}

/// 片段级 fixture：按 `schemaPointer` 指向的定义解码。
fn assert_fragment(name: &str, pointer: &str, bytes: &[u8], valid: bool) {
    match pointer {
        "#/$defs/ToolCallLocation" => {
            let decoded = serde_json::from_slice::<ToolCallLocation>(bytes);
            if valid {
                let location = decoded.unwrap_or_else(|error| panic!("{name}: 应可解码 {error}"));
                assert_eq!(location.path, "C:\\workspace\\example.txt");
                assert_eq!(location.line, Some(12));
            } else {
                let error = decoded.expect_err("line 为负值必须被拒绝（uint32）");
                assert!(
                    error.to_string().contains("line") || error.to_string().contains("u32"),
                    "{name}: 错误必须指向 line 的宽度/符号，实际 {error}"
                );
            }
        }
        other => panic!("{name}: 未登记 schemaPointer {other}"),
    }
}

#[test]
fn tool_call_location_rejects_out_of_range_line() {
    // 上游把 line 声明为 `format: uint32`，因此负值、溢出与非整数都必须在解码边界被拒。
    for (label, line) in [
        ("负数", serde_json::json!(-1)),
        ("超出 u32", serde_json::json!(4_294_967_296u64)),
        ("非整数", serde_json::json!(1.5)),
    ] {
        let value = serde_json::json!({ "path": "C:\\x.txt", "line": line });
        // 刻意走 `from_slice`（而不是 `from_value`）：它保留字段上下文，错误消息里能看到 line 的宽度约束。
        let bytes = serde_json::to_vec(&value).expect("可序列化");
        let error = serde_json::from_slice::<ToolCallLocation>(&bytes)
            .err()
            .unwrap_or_else(|| panic!("{label} 应被拒绝"));
        assert!(
            error.to_string().contains("line") || error.to_string().contains("u32"),
            "{label}: 错误应指向 line 的宽度/符号，实际 {error}"
        );

        // 控制组：同一形状换成合法 line 必须成功（证明失败来自 line 而不是其它字段）。
        let control = serde_json::to_vec(&serde_json::json!({ "path": "C:\\x.txt", "line": 12 }))
            .expect("可序列化");
        assert!(
            serde_json::from_slice::<ToolCallLocation>(&control).is_ok(),
            "{label}: 控制组必须成功"
        );
    }
}
