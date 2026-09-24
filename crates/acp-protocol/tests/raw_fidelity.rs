//! 原文保真与固定上限。
//!
//! 这里刻意放了一条**反例**断言：`serde_json::Value` 的再序列化与 fixture 原始字节**不相等**。
//! 它固定住「保真来自 `RawDocument`，不是来自类型化往返」这件事——任何人把实现改成
//! 「parse → to_string」都会先在这条断言上暴露，而不是在某个更远的集成测试里。

mod support;

use acp_protocol::error::AcpError;
use acp_protocol::methods::{self, MethodStatus};
use acp_protocol::{Envelope, IdLiteral, RawDocument, limits};

#[test]
fn byte_exact_round_trip_uses_raw_bytes_not_value_projection() {
    let bytes = support::fixture_bytes("unknown-fields-and-large-integer.json");
    let document = RawDocument::parse_bytes(&bytes).expect("合法消息");
    assert_eq!(
        document.encode().as_bytes(),
        bytes,
        "再编码必须逐字节等于原始字节"
    );

    // 反例：走通用 Value 再序列化不能作为保真路径（键顺序/空白/数字文本都会变）。
    let multi_line = support::fixture_bytes("meta-and-unknown-fields.json");
    let projected = RawDocument::parse_bytes(&multi_line)
        .expect("合法消息")
        .value()
        .expect("合法 JSON")
        .to_string();
    assert_ne!(
        projected.as_bytes(),
        multi_line.as_slice(),
        "通用 Value 的再序列化本就不等于原文——保真只能靠原文承载"
    );
}

#[test]
fn unknown_fields_and_meta_survive_in_the_original_text() {
    let bytes = support::fixture_bytes("meta-and-unknown-fields.json");
    let document = RawDocument::parse_bytes(&bytes).expect("合法消息");
    let text = document.encode();
    for needle in [
        "example.dev/note",
        "example.dev/counters",
        "futureFieldFromNewerAcp",
        "9007199254740991",
    ] {
        assert!(text.contains(needle), "原文里必须仍有 {needle}");
    }
    // 原文仍是良构 JSON，未知字段在投影里也是结构化的（不是被文本化的字符串）。
    let value = document.value().expect("合法 JSON");
    assert_eq!(
        value["params"]["futureFieldFromNewerAcp"]["nested"][2]["deep"],
        true
    );
    assert_eq!(
        value["params"]["update"]["content"]["_meta"]["example.dev/note"],
        "content level"
    );
}

#[test]
fn id_literal_is_preserved_as_written() {
    // 转义与超大整数都必须在 id 上原样保留：不能规范化成 "A"，也不能转成浮点。
    let text = r#"{"jsonrpc":"2.0","id":"\u0041","method":"session/cancel","params":{}}"#;
    let document = RawDocument::parse(text).expect("合法消息");
    let literal = document.member_literal("id").expect("id 存在");
    assert_eq!(literal, r#""\u0041""#, "id 的字面量必须原样取出");
    let id = IdLiteral::from_literal(literal).expect("合法 id");
    assert_eq!(id.as_json(), r#""\u0041""#);
    assert_eq!(id.string_value().as_deref(), Some("A"));
    assert!(id.matches_value(&serde_json::json!("A")));

    let big = RawDocument::parse(
        r#"{"jsonrpc":"2.0","id":9007199254740993,"method":"session/cancel","params":{}}"#,
    )
    .expect("合法消息");
    let literal = big.member_literal("id").expect("id 存在");
    assert_eq!(literal, "9007199254740993");
    let id = IdLiteral::from_literal(literal).expect("合法 id");
    assert_eq!(id.number_text(), Some("9007199254740993"));
    assert_eq!(id.as_json(), "9007199254740993");
}

#[test]
fn oversize_message_is_rejected_without_partial_result() {
    let padded = format!(
        r#"{{"jsonrpc":"2.0","method":"session/cancel","params":{{"sessionId":"{}"}}}}"#,
        "x".repeat(limits::MAX_MESSAGE_BYTES)
    );
    let error = RawDocument::parse(padded).expect_err("超限必须失败");
    match error {
        AcpError::Oversize { limit, actual } => {
            assert_eq!(limit, limits::MAX_MESSAGE_BYTES);
            assert!(actual > limit);
        }
        other => panic!("期望 Oversize，实际 {other}"),
    }

    // 边界内仍然可解析（不能把上限实现成「大于等于就拒绝」）。
    let inside = format!(
        r#"{{"jsonrpc":"2.0","method":"session/cancel","params":{{"sessionId":"{}"}}}}"#,
        "x".repeat(1024)
    );
    let document = RawDocument::parse(inside).expect("远小于上限的消息必须可解析");
    assert!(document.len() < limits::MAX_MESSAGE_BYTES);
}

#[test]
fn non_object_and_non_json_inputs_are_rejected() {
    assert!(matches!(
        RawDocument::parse("[1,2,3]"),
        Err(AcpError::Malformed { .. })
    ));
    assert!(matches!(
        RawDocument::parse("{not json}"),
        Err(AcpError::Json { .. })
    ));
    assert!(matches!(
        RawDocument::parse_bytes(&[0xff, 0xfe]),
        Err(AcpError::Malformed { .. })
    ));
}

#[test]
fn unimplemented_and_unknown_methods_are_explicitly_unsupported() {
    let cases = [
        ("session/load", MethodStatus::NotImplemented),
        ("fs/read_text_file", MethodStatus::NotImplemented),
        ("terminal/create", MethodStatus::NotImplemented),
        ("authenticate", MethodStatus::NotImplemented),
        ("$/cancel_request", MethodStatus::NotImplemented),
        ("_vendor/thing", MethodStatus::Extension),
        ("example/vendor_thing", MethodStatus::Unknown),
    ];
    for (method, status) in cases {
        let document = RawDocument::parse(format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"{method}","params":{{}}}}"#
        ))
        .expect("合法消息");
        let envelope = Envelope::classify(document).expect("分类");
        assert_eq!(methods::status_of(method), status);
        let error = envelope
            .ensure_direction(acp_protocol::MethodDirection::ClientToAgent)
            .expect_err("未实现的方法必须显式不支持");
        assert!(
            matches!(error, AcpError::Unsupported { .. }),
            "{method}: 期望 Unsupported，实际 {error}"
        );
    }
}

#[test]
fn direction_mismatch_is_reported_as_direction_error() {
    let document = RawDocument::parse(
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"current_mode_update","currentModeId":"m"}}}"#,
    )
    .expect("合法消息");
    let envelope = Envelope::classify(document).expect("分类");
    let error = envelope
        .ensure_direction(acp_protocol::MethodDirection::ClientToAgent)
        .expect_err("agent→client 的方法不能按 client→agent 处理");
    match error {
        AcpError::WrongDirection {
            method,
            actual,
            expected,
        } => {
            assert_eq!(method, "session/update");
            assert_eq!(actual, acp_protocol::MethodDirection::AgentToClient);
            assert_eq!(expected, acp_protocol::MethodDirection::ClientToAgent);
        }
        other => panic!("期望 WrongDirection，实际 {other}"),
    }
}
