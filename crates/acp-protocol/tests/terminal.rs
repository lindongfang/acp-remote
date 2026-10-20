//! `terminal/*` 的 wire DTO：方法登记、形状校验、上限语义与响应编码。
//!
//! 这些测试只证明**编解码合同**（谁发、字段是什么、什么形状被拒）；真实子进程、输出预算与
//! kill/release 的收敛属于 `agent-host` 的终端服务，在那边用真实子进程证明。

mod support;

use acp_protocol::error::AcpError;
use acp_protocol::methods::{self, MethodDirection, MethodStatus};
use acp_protocol::terminal::{
    self, CreateTerminalRequest, CreateTerminalResponse, TerminalExitStatus,
    TerminalOutputResponse, TerminalRefRequest, WaitForTerminalExitResponse,
};
use acp_protocol::{Envelope, MessageClass, RawDocument};
use serde_json::{Value, json};

fn envelope(bytes: &[u8]) -> Envelope {
    Envelope::classify(RawDocument::parse_bytes(bytes).expect("合法消息")).expect("分类")
}

const CREATE: &[u8] = br#"{"jsonrpc":"2.0","id":11,"method":"terminal/create","params":{"sessionId":"s-1","command":"node","args":["-e","console.log(1)"],"env":[{"name":"LANG","value":"zh_CN.UTF-8","_meta":{"example.dev/k":"v"}}],"cwd":"C:\\work\\demo","outputByteLimit":4096,"futureFieldFromNewerAcp":{"nested":[1,2]},"_meta":{"example.dev/note":"create"}}}"#;

#[test]
fn the_five_terminal_methods_are_implemented_agent_to_client_requests() {
    for method in [
        "terminal/create",
        "terminal/output",
        "terminal/wait_for_exit",
        "terminal/kill",
        "terminal/release",
    ] {
        assert_eq!(
            methods::status_of(method),
            MethodStatus::Implemented,
            "{method} 必须已登记为可实现（终端服务已接线）"
        );
        assert_eq!(
            methods::direction_of(method),
            Some(MethodDirection::AgentToClient),
            "{method} 是 agent→client 的请求"
        );
        let envelope = envelope(
            format!(r#"{{"jsonrpc":"2.0","id":1,"method":"{method}","params":{{}}}}"#).as_bytes(),
        );
        assert!(
            envelope
                .ensure_direction(MethodDirection::AgentToClient)
                .is_ok()
        );
        assert!(matches!(
            envelope.ensure_direction(MethodDirection::ClientToAgent),
            Err(AcpError::WrongDirection { .. })
        ));
    }
}

#[test]
fn create_request_decodes_the_pinned_fields_and_re_encodes_byte_exact() {
    let envelope = envelope(CREATE);
    assert_eq!(envelope.class(), MessageClass::Request);
    let request = terminal::create_request(&envelope).expect("terminal/create 可解码");
    assert_eq!(request.session_id, "s-1");
    assert_eq!(request.command, "node");
    assert_eq!(request.args, vec!["-e", "console.log(1)"]);
    assert_eq!(request.env.len(), 1);
    assert_eq!(request.env[0].name, "LANG");
    assert_eq!(request.env[0].value, "zh_CN.UTF-8");
    assert_eq!(
        request.env[0].meta,
        Some(json!({ "example.dev/k": "v" })),
        "env 条目上的 `_meta` 必须保留"
    );
    assert_eq!(request.cwd.as_deref(), Some(r"C:\work\demo"));
    assert_eq!(request.output_byte_limit, Some(4096));

    // 保真路径是原文：未知字段与 `_meta` 仍在，且再编码逐字节等于原始字节。
    let text = envelope.document().encode();
    assert_eq!(text.as_bytes(), CREATE);
    assert!(text.contains("futureFieldFromNewerAcp"));
    assert!(text.contains("example.dev/note"));
}

#[test]
fn create_request_omits_absent_optionals_when_encoded() {
    let request = CreateTerminalRequest {
        session_id: "s-1".to_owned(),
        command: "echo".to_owned(),
        args: Vec::new(),
        env: Vec::new(),
        cwd: None,
        output_byte_limit: None,
        meta: None,
    };
    assert_eq!(
        serde_json::to_value(&request).expect("可序列化"),
        json!({ "sessionId": "s-1", "command": "echo" }),
        "空参数/缺席的可选项不得被写成空数组或 null 占位"
    );
}

#[test]
fn create_request_rejects_missing_and_mistyped_fields() {
    let cases: Vec<(Value, &str, bool)> = vec![
        (json!({ "command": "echo" }), "sessionId", false),
        (
            json!({ "sessionId": 3, "command": "echo" }),
            "sessionId",
            true,
        ),
        (json!({ "sessionId": "s" }), "command", false),
        (
            json!({ "sessionId": "s", "command": null }),
            "command",
            true,
        ),
        (
            json!({ "sessionId": "s", "command": "echo", "args": "-n" }),
            "args",
            true,
        ),
        (
            json!({ "sessionId": "s", "command": "echo", "args": ["ok", 1] }),
            "args",
            true,
        ),
        (
            json!({ "sessionId": "s", "command": "echo", "env": {} }),
            "env",
            true,
        ),
        (
            json!({ "sessionId": "s", "command": "echo", "env": [{"name": "A"}] }),
            "env",
            true,
        ),
        (
            json!({ "sessionId": "s", "command": "echo", "cwd": 3 }),
            "cwd",
            true,
        ),
    ];
    for (params, field, mistyped) in cases {
        match CreateTerminalRequest::from_params(&params) {
            Err(AcpError::MissingField { field: actual }) if !mistyped && actual == field => {}
            Err(AcpError::InvalidField { field: actual, .. }) if mistyped && actual == field => {}
            other => panic!("{params}: 期望指向 {field} 的失败，实际 {other:?}"),
        }
    }
}

#[test]
fn output_byte_limit_is_uint64_only() {
    for (value, ok) in [
        (json!(0), true),
        (json!(1), true),
        (json!(18446744073709551615u64), true),
        (json!(-1), false),
        (json!(1.5), false),
        (json!(-0.0), false),
        (json!("4096"), false),
        (json!(true), false),
    ] {
        let params = json!({
            "sessionId": "s",
            "command": "echo",
            "outputByteLimit": value,
        });
        match CreateTerminalRequest::from_params(&params) {
            Ok(request) => {
                assert!(
                    ok,
                    "{value} 必须被拒绝，实际解出 {:?}",
                    request.output_byte_limit
                );
            }
            Err(AcpError::InvalidField { field, .. }) => {
                assert!(!ok, "{value} 必须被接受");
                assert_eq!(field, "outputByteLimit");
            }
            Err(other) => panic!("{value}: 期望 InvalidField，实际 {other:?}"),
        }
    }
}

#[test]
fn terminal_ref_requests_require_both_ids_and_the_right_method() {
    for (method, decode) in [
        (
            "terminal/output",
            terminal::output_request as fn(&Envelope) -> acp_protocol::Result<TerminalRefRequest>,
        ),
        (
            "terminal/wait_for_exit",
            terminal::wait_for_exit_request
                as fn(&Envelope) -> acp_protocol::Result<TerminalRefRequest>,
        ),
        (
            "terminal/kill",
            terminal::kill_request as fn(&Envelope) -> acp_protocol::Result<TerminalRefRequest>,
        ),
        (
            "terminal/release",
            terminal::release_request as fn(&Envelope) -> acp_protocol::Result<TerminalRefRequest>,
        ),
    ] {
        let params = terminal::terminal_ref_params("s-1", "term-1");
        let document = RawDocument::parse(
            json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params }).to_string(),
        )
        .expect("合法消息");
        let envelope = Envelope::classify(document).expect("分类");
        let request = decode(&envelope).expect("两个必填字段都在");
        assert_eq!(request.session_id, "s-1");
        assert_eq!(request.terminal_id, "term-1");

        // 方法名不符：同一个形状不能冒充别的方法。
        let wrong = RawDocument::parse(
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "terminal/create",
                "params": terminal::terminal_ref_params("s-1", "term-1"),
            })
            .to_string(),
        )
        .expect("合法消息");
        let wrong = Envelope::classify(wrong).expect("分类");
        assert!(matches!(
            decode(&wrong),
            Err(AcpError::InvalidField { field, .. }) if field == "method"
        ));

        // 缺 terminalId 是显式缺字段，不是「空终端」。
        let missing = RawDocument::parse(
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": method,
                "params": { "sessionId": "s-1" },
            })
            .to_string(),
        )
        .expect("合法消息");
        let missing = Envelope::classify(missing).expect("分类");
        assert!(matches!(
            decode(&missing),
            Err(AcpError::MissingField { field }) if field == "terminalId"
        ));
    }
}

#[test]
fn responses_encode_the_pinned_shapes() {
    assert_eq!(
        serde_json::to_value(CreateTerminalResponse {
            terminal_id: "term-1".to_owned(),
            meta: None,
        })
        .expect("可序列化"),
        json!({ "terminalId": "term-1" })
    );

    let running = TerminalOutputResponse {
        output: "第一段".to_owned(),
        truncated: false,
        exit_status: None,
        meta: None,
    };
    assert_eq!(
        serde_json::to_value(&running).expect("可序列化"),
        json!({ "output": "第一段", "truncated": false }),
        "未退出时 exitStatus 必须缺席（不是 null、不是伪造的 0）"
    );

    let exited = TerminalOutputResponse {
        output: "boom".to_owned(),
        truncated: true,
        exit_status: Some(TerminalExitStatus::from_code(3)),
        meta: None,
    };
    assert_eq!(
        serde_json::to_value(&exited).expect("可序列化"),
        json!({
            "output": "boom",
            "truncated": true,
            "exitStatus": { "exitCode": 3 },
        })
    );

    let wait = WaitForTerminalExitResponse::from_status(&TerminalExitStatus::from_code(0));
    assert_eq!(
        serde_json::to_value(&wait).expect("可序列化"),
        json!({ "exitCode": 0 })
    );
}

#[test]
fn terminal_create_stays_a_request_not_a_notification() {
    // 无 `id` 的同名消息不是请求：分类后没有 id，调用方无法回包。
    let notification = envelope(
        br#"{"jsonrpc":"2.0","method":"terminal/create","params":{"sessionId":"s","command":"echo"}}"#,
    );
    assert_eq!(notification.class(), MessageClass::Notification);
    assert!(notification.id().is_none());
}
