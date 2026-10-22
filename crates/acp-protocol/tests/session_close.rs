//! `session/close` 的类型化解码、往返保真与条件登记（`local-agent-host` 增量）。
//!
//! 这些用例只钉协议层事实，**不**证明任何 Agent 真的支持关闭：真实能力由 `initialize` 协商并在
//! `agent-host` 门控（`AgentCapabilities::supports_session_close`），释放是否发生由 `agent-host` 的
//! 线级用例断言（`crates/agent-host/tests/close.rs`）。

use acp_protocol::AcpError;
use acp_protocol::capability::AgentCapabilities;
use acp_protocol::envelope::Envelope;
use acp_protocol::message::{
    SessionCloseRequest, SessionCloseResponse, decode_response, session_close_request,
    session_resume_request,
};
use acp_protocol::methods::{self, MethodDirection, MethodKind, MethodStatus};
use acp_protocol::raw::RawDocument;
use serde_json::json;

fn envelope_of(bytes: &[u8]) -> Envelope {
    Envelope::classify(RawDocument::parse_bytes(bytes).expect("合法消息")).expect("分类")
}

/// 带 id、未知字段、`_meta` 与超出 JS 安全整数取值的 `session/close` 请求。
const CLOSE_REQUEST: &[u8] = br#"{"jsonrpc":"2.0","id":9,"method":"session/close","params":{"sessionId":"acp-1","futureFieldFromNewerAcp":{"nested":[1,2],"big":9007199254740993},"_meta":{"example.dev/note":"close"}}}"#;

#[test]
fn close_request_decodes_typed_fields_and_re_encodes_byte_exact() {
    let envelope = envelope_of(CLOSE_REQUEST);
    assert_eq!(envelope.method(), Some("session/close"));

    let request = session_close_request(&envelope).expect("session/close 请求可解码");
    assert_eq!(request.session_id, "acp-1");
    assert_eq!(request.meta, Some(json!({ "example.dev/note": "close" })));

    // 保真：再编码就是原文；未知字段与 `_meta` 仍在原文里。
    assert_eq!(envelope.document().encode().as_bytes(), CLOSE_REQUEST);
    let text = envelope.document().encode();
    assert!(text.contains("futureFieldFromNewerAcp"), "未知字段被丢弃");
    assert!(text.contains("9007199254740993"), "大整数文本被改写");
    assert!(text.contains("example.dev/note"), "`_meta` 被丢弃");
}

#[test]
fn close_request_encode_is_limited_to_the_pinned_fields() {
    let request = SessionCloseRequest::new("acp-1");
    let params = serde_json::to_value(&request).expect("可序列化");
    assert_eq!(
        params,
        json!({ "sessionId": "acp-1" }),
        "`_meta` 缺省时不得凭空出现，也不得补 `cwd` 等恢复字段"
    );
    let decoded = SessionCloseRequest::from_params(&params).expect("自己编码的请求必须可解码");
    assert_eq!(decoded, request);
}

#[test]
fn close_request_rejects_missing_and_mistyped_session_id() {
    let cases = [
        (json!({}), "sessionId"),
        (json!({ "sessionId": null }), "sessionId"),
        (json!({ "sessionId": 7 }), "sessionId"),
        (json!({ "sessionId": ["acp-1"] }), "sessionId"),
    ];
    for (params, field) in cases {
        match SessionCloseRequest::from_params(&params) {
            Err(AcpError::MissingField { field: actual }) if actual == field => {}
            Err(AcpError::InvalidField { field: actual, .. }) if actual == field => {}
            other => {
                panic!("{params}: 期望指向 {field} 的 MissingField/InvalidField，实际 {other:?}")
            }
        }
    }

    assert_eq!(
        SessionCloseRequest::from_params(&json!({})).unwrap_err(),
        AcpError::MissingField {
            field: "sessionId".to_owned()
        }
    );
    assert_eq!(
        SessionCloseRequest::from_params(&json!({ "sessionId": 7 })).unwrap_err(),
        AcpError::InvalidField {
            field: "sessionId".to_owned(),
            detail: "必须是字符串，收到 number".to_owned()
        }
    );
}

#[test]
fn close_response_is_optional_and_preserves_unknown_fields() {
    // 空对象是合法响应（上游 `CloseSessionResponse` 只有可选 `_meta`）。
    let empty = envelope_of(br#"{"jsonrpc":"2.0","id":9,"result":{}}"#);
    let response: SessionCloseResponse = decode_response(&empty).expect("空对象必须可解码");
    assert_eq!(response.meta, None);

    // `_meta` 与未知字段都必须能读；原文逐字节保留。
    let bytes = br#"{"jsonrpc":"2.0","id":9,"result":{"futureField":true,"_meta":{"example.dev/note":"closed"}}}"#;
    let envelope = envelope_of(bytes);
    let response: SessionCloseResponse =
        decode_response(&envelope).expect("带未知字段的响应必须可解码");
    assert_eq!(response.meta, Some(json!({ "example.dev/note": "closed" })));
    assert_eq!(envelope.document().encode().as_bytes(), bytes);
}

#[test]
fn close_is_registered_as_a_conditional_client_to_agent_request() {
    let spec = methods::find("session/close").expect("session/close 必须已登记");
    assert_eq!(spec.wire_name, "session/close");
    assert_eq!(spec.direction, MethodDirection::ClientToAgent);
    assert_eq!(spec.kind, MethodKind::Request);
    assert_eq!(spec.delivery, methods::Delivery::ConditionalMvp);
    assert!(
        spec.implemented,
        "有类型化 DTO 的方法必须登记为已实现，否则解码边界会先报不支持"
    );
    assert_eq!(
        methods::status_of("session/close"),
        MethodStatus::Implemented
    );
    assert_eq!(
        methods::direction_of("session/close"),
        Some(MethodDirection::ClientToAgent)
    );

    // 提升 close 不得顺带提升 `session/load`（它没有类型化 DTO，也没有原身份恢复语义）。
    assert_eq!(
        methods::status_of("session/load"),
        MethodStatus::NotImplemented
    );
}

#[test]
fn close_capability_gate_matches_the_resume_gate() {
    let declared: AgentCapabilities = serde_json::from_value(json!({
        "sessionCapabilities": {
            "resume": {},
            "close": {},
            "_meta": { "example.dev/close": 1 }
        }
    }))
    .expect("能力可解码");
    assert!(declared.supports_session_close());
    assert!(declared.supports_session_resume());

    for (label, capabilities) in [
        (
            "omitted",
            json!({ "sessionCapabilities": { "resume": {} } }),
        ),
        ("null", json!({ "sessionCapabilities": { "close": null } })),
        ("absent", json!({})),
    ] {
        let capabilities: AgentCapabilities =
            serde_json::from_value(capabilities).expect("能力可解码");
        assert!(
            !capabilities.supports_session_close(),
            "{label}: 未宣告 close 时不得判为支持"
        );
    }
}

/// 方法名不符与方向不符都由 `client_params` 显式拒绝，不静默复用恢复路径。
#[test]
fn close_request_rejects_a_different_client_to_agent_method() {
    let resume = envelope_of(
        br#"{"jsonrpc":"2.0","id":10,"method":"session/resume","params":{"sessionId":"acp-1","cwd":"/work"}}"#,
    );
    let error = session_close_request(&resume).expect_err("方法名不符必须失败");
    assert!(
        matches!(error, AcpError::InvalidField { ref field, .. } if field == "method"),
        "期望 InvalidField(method)，实际 {error}"
    );
    // 反向也成立：close 的信封不能被恢复路径当成 resume 请求。
    let close = envelope_of(CLOSE_REQUEST);
    assert!(session_resume_request(&close).is_err());
}
