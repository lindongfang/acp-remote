//! JSON-RPC 信封分类与 required 字段校验。

mod support;

use acp_protocol::RawDocument;
use acp_protocol::envelope::{Envelope, MessageClass};
use acp_protocol::error::AcpError;
use acp_protocol::raw::IdLiteral;

fn classify(text: &str) -> Result<Envelope, AcpError> {
    Envelope::classify(RawDocument::parse(text).expect("合法 JSON 对象"))
}

#[test]
fn classifies_request_notification_and_response() {
    let request = classify(
        r#"{"jsonrpc":"2.0","id":1,"method":"session/prompt","params":{"sessionId":"s","prompt":[]}}"#,
    )
    .expect("合法请求");
    assert_eq!(request.class(), MessageClass::Request);
    assert_eq!(request.method(), Some("session/prompt"));
    assert_eq!(request.id(), Some(&IdLiteral::Number("1".to_owned())));

    let notification =
        classify(r#"{"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"s"}}"#)
            .expect("合法通知");
    assert_eq!(notification.class(), MessageClass::Notification);
    assert!(notification.id().is_none());

    let response = classify(r#"{"jsonrpc":"2.0","id":1,"result":{"stopReason":"end_turn"}}"#)
        .expect("合法响应");
    assert_eq!(response.class(), MessageClass::Response);
    assert!(response.method().is_none());
    assert!(!response.is_error_response());
}

#[test]
fn id_literal_accepts_string_and_number_forms() {
    let with_string = classify(
        r#"{"jsonrpc":"2.0","id":"9f1c2d3e","method":"session/cancel","params":{"sessionId":"s"}}"#,
    )
    .expect("字符串 id（实测 Zed 使用 UUID 字符串）必须被接受");
    let id = with_string.id().expect("id");
    assert_eq!(id.string_value().as_deref(), Some("9f1c2d3e"));
    assert!(id.number_text().is_none());
}

#[test]
fn rejects_missing_or_invalid_jsonrpc() {
    assert!(matches!(
        classify(r#"{"id":1,"method":"session/cancel","params":{}}"#),
        Err(AcpError::MissingField { ref field }) if field == "jsonrpc"
    ));
    assert!(matches!(
        classify(r#"{"jsonrpc":"1.0","id":1,"method":"session/cancel","params":{}}"#),
        Err(AcpError::InvalidField { ref field, .. }) if field == "jsonrpc"
    ));
}

#[test]
fn rejects_non_string_method() {
    assert!(matches!(
        classify(r#"{"jsonrpc":"2.0","id":1,"method":42}"#),
        Err(AcpError::InvalidField { ref field, .. }) if field == "method"
    ));
}

#[test]
fn rejects_shapes_that_are_neither_request_nor_response() {
    assert!(matches!(
        classify(r#"{"jsonrpc":"2.0","id":1}"#),
        Err(AcpError::Malformed { .. })
    ));
    assert!(matches!(
        classify(r#"{"jsonrpc":"2.0","result":{}}"#),
        Err(AcpError::Malformed { .. })
    ));
    assert!(matches!(
        classify(r#"{"jsonrpc":"2.0","id":1,"result":{},"error":{"code":-1}}"#),
        Err(AcpError::Malformed { .. })
    ));
}

#[test]
fn params_and_result_require_the_declared_shape() {
    let missing_params = classify(r#"{"jsonrpc":"2.0","id":1,"method":"session/prompt"}"#)
        .expect("分类不管 required 字段");
    assert!(matches!(
        missing_params.params(),
        Err(AcpError::MissingField { ref field }) if field == "params"
    ));

    let error_response =
        classify(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"x"}}"#)
            .expect("分类");
    assert!(error_response.is_error_response());
    let error = error_response.result().expect_err("错误响应没有 result");
    assert!(matches!(error, AcpError::InvalidField { .. }));
    // 错误摘要只保留 code，不带正文（避免把对端 message 带进日志）。
    assert!(error.to_string().contains("-32601"));
    assert!(!error.to_string().contains("\"message\""));
}
