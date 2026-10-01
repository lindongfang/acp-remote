//! R1–R4：`session/resume` 的类型化解码与往返保真（`acp-wire-protocol` 增量）。
//!
//! 入口是本 crate 的**真实公开面**：`RawDocument::parse_bytes` → `Envelope::classify` →
//! `session_resume_request`/`decode_response`，断言只绑定**可观察结果**（解码出的字段、再编码的字节、
//! 错误分类、方法登记表状态），不绑定内部实现。
//!
//! **不新增 `fixtures/acp/v1/` 内容**：全部用内联字节串，既避免动 `manifest.json` 的计数门禁，
//! 也让「未知字段逐字节保真」有一个固定可读的输入。
//!
//! 判别力说明：若实现改走 `serde_json::Value` 往返再重新序列化，`session_resume_wire_round_trip_keeps_unknown_fields_byte_exact`
//! 会因大整数与键序而失败；若实现给 `cwd` 加默认值，`missing_required_fields_are_rejected_without_defaults` 会失败。

use acp_protocol::envelope::Envelope;
use acp_protocol::message::{
    SessionResumeRequest, SessionResumeResponse, decode_response, session_resume_request,
};
use acp_protocol::methods::{self, MethodDirection, MethodKind, MethodStatus};
use acp_protocol::raw::RawDocument;
use acp_protocol::{AcpError, MethodStatus as ReExportedStatus};
use serde_json::json;

/// 分类一条内联报文（真实入口的第一步）。
fn envelope_of(bytes: &[u8]) -> Envelope {
    Envelope::classify(RawDocument::parse_bytes(bytes).expect("合法 JSON-RPC 文档"))
        .expect("可分类")
}

/// 一条带 id、未知字段、`_meta` 与**超出 JS 安全整数**取值的 `session/resume` 请求。
///
/// `9007199254740993`（2^53+1）是保真判别的关键：经 `serde_json::Value` 往返的实现在多数配置下会写成
/// `9007199254740992`，因此本文件的逐字节断言能区分「走原文」与「走通用 JSON 值」。
const RESUME_REQUEST: &[u8] = br#"{"jsonrpc":"2.0","id":7,"method":"session/resume","params":{"sessionId":"acp-1","cwd":"C:\\work\\demo","futureFieldFromNewerAcp":{"nested":[1,2],"big":9007199254740993},"_meta":{"example.dev/note":"resume"}}}"#;

/// R1 + R2：类型化解码出 `sessionId`/`cwd`，再编码与输入**逐字节相同**（未知字段、`_meta` 与超大整数原样保留）。
#[test]
fn session_resume_wire_round_trip_keeps_unknown_fields_byte_exact() {
    let envelope = envelope_of(RESUME_REQUEST);
    assert_eq!(
        envelope.method(),
        Some("session/resume"),
        "报文方法名必须被识别为 session/resume"
    );

    let request = session_resume_request(&envelope).expect("session/resume 请求可解码");
    assert_eq!(request.session_id, "acp-1");
    assert_eq!(request.cwd, r"C:\work\demo");
    assert_eq!(
        request.meta,
        Some(json!({ "example.dev/note": "resume" })),
        "`_meta` 必须被解码为扩展容器而不是丢弃"
    );

    // 保真的判别式：再编码就是**输入字节**，而不是「一个语义等价但字节不同」的文档。
    assert_eq!(
        envelope.document().encode().as_bytes(),
        RESUME_REQUEST,
        "重编码必须与输入逐字节相同（未知字段/键序/超大整数都不得被改写）"
    );
    let text = envelope.document().encode();
    assert!(
        text.contains("futureFieldFromNewerAcp"),
        "上游快照未定义字段被丢弃：{text}"
    );
    assert!(
        text.contains("9007199254740993"),
        "超出 JS 安全整数的取值被改写（说明走了 serde_json::Value 往返）：{text}"
    );
}

/// R1：类型化编码 ↔ 解码互逆；编码只含 pinned 字段（`_meta` 缺省时不得凭空出现）。
#[test]
fn session_resume_typed_encoding_and_decoding_are_inverse() {
    let request = SessionResumeRequest::new("acp-1", "/work/demo");
    let params = serde_json::to_value(&request).expect("请求可序列化");
    assert_eq!(
        params,
        json!({ "sessionId": "acp-1", "cwd": "/work/demo" }),
        "类型化编码只能产出两个 required 字段，`_meta` 缺省时不得凭空出现"
    );
    assert_eq!(
        SessionResumeRequest::from_params(&params).expect("自己编码的请求必须可解码"),
        request,
        "编码 → 解码必须得到同一个 DTO"
    );

    // 确定性：同一输入两次编码字节相同（同一会话不会因为重编码而漂移）。
    let again = serde_json::to_vec(&request).expect("可序列化");
    assert_eq!(again, serde_json::to_vec(&request).expect("可序列化"));

    // 响应 DTO：恢复的响应不返回会话标识（只有 modes/configOptions/_meta），因此 `{}` 也是合法响应。
    let empty: SessionResumeResponse =
        decode_response(&envelope_of(br#"{"jsonrpc":"2.0","id":7,"result":{}}"#))
            .expect("空响应是合法的恢复响应");
    assert!(empty.modes.is_none() && empty.config_options.is_none());
}

/// R2（响应侧）：恢复响应同样逐字节保真，未知字段不被丢弃。
#[test]
fn session_resume_response_round_trip_keeps_unknown_fields() {
    let bytes = br#"{"jsonrpc":"2.0","id":7,"result":{"modes":{"currentModeId":"ask","availableModes":[]},"configOptions":[],"futureField":true,"_meta":{"example.dev/note":"resume"}}}"#;
    let envelope = envelope_of(bytes);
    let response: SessionResumeResponse =
        decode_response(&envelope).expect("session/resume 响应可解码");
    assert_eq!(
        response
            .modes
            .as_ref()
            .map(|modes| modes.current_mode_id.as_str()),
        Some("ask")
    );
    assert_eq!(response.config_options, Some(Vec::new()));
    assert_eq!(response.meta, Some(json!({ "example.dev/note": "resume" })));
    assert_eq!(
        envelope.document().encode().as_bytes(),
        bytes,
        "响应的未知字段也必须逐字节保真"
    );
}

/// R3：缺少 required 字段、类型不符、`null` 都在解码边界被拒，且**不用默认值补齐**。
///
/// 判别力：`cwd: null` 与 `cwd: 3` 都必须失败；若实现把 `null` 当「缺省并取默认值」，本用例会失败。
#[test]
fn missing_required_fields_are_rejected_without_defaults() {
    let cases = [
        (json!({ "sessionId": "acp-1" }), "cwd"),
        (json!({ "sessionId": "acp-1", "cwd": null }), "cwd"),
        (json!({ "sessionId": "acp-1", "cwd": 3 }), "cwd"),
        (json!({ "sessionId": "acp-1", "cwd": ["/work"] }), "cwd"),
        (json!({ "cwd": "/work" }), "sessionId"),
        (json!({ "cwd": "/work", "sessionId": null }), "sessionId"),
        (json!({ "cwd": "/work", "sessionId": 7 }), "sessionId"),
    ];
    for (params, field) in cases {
        match SessionResumeRequest::from_params(&params) {
            Err(AcpError::MissingField { field: actual }) if actual == field => {}
            Err(AcpError::InvalidField { field: actual, .. }) if actual == field => {}
            other => {
                panic!("{params}: 期望指向 {field} 的 MissingField/InvalidField，实际 {other:?}")
            }
        }
    }

    // 「缺失」与「类型不符」是两种可区分的分类，不得合并成一个不合法。
    assert_eq!(
        SessionResumeRequest::from_params(&json!({ "sessionId": "acp-1" })).expect_err("缺 cwd"),
        AcpError::MissingField {
            field: "cwd".to_owned()
        }
    );
    assert_eq!(
        SessionResumeRequest::from_params(&json!({ "sessionId": "acp-1", "cwd": 3 }))
            .expect_err("cwd 类型不符"),
        AcpError::InvalidField {
            field: "cwd".to_owned(),
            detail: "必须是字符串，收到 number".to_owned()
        }
    );
}

/// R3 的报文级形态：解一条缺 `cwd` 的真实报文，错误必须来自解码边界（`client_params` → `from_params`），
/// 而不是「未知方法」或「JSON 语法错误」这两类可混淆的失败。
#[test]
fn a_resume_request_missing_cwd_is_rejected_at_the_decode_boundary() {
    let envelope = envelope_of(
        br#"{"jsonrpc":"2.0","id":7,"method":"session/resume","params":{"sessionId":"acp-1"}}"#,
    );
    let error = session_resume_request(&envelope).expect_err("缺 cwd 必须被拒");
    assert_eq!(
        error,
        AcpError::MissingField {
            field: "cwd".to_owned()
        },
        "错误必须指向缺失的 required 字段本身"
    );
}

/// R4：`session/load` 仍显式不支持，且**不因**新增 `session/resume` 而被一起提升或降级。
#[test]
fn session_load_stays_unsupported_and_resume_is_implemented() {
    let load = envelope_of(
        br#"{"jsonrpc":"2.0","id":8,"method":"session/load","params":{"sessionId":"acp-1","cwd":"/work","mcpServers":[]}}"#,
    );
    assert_eq!(load.status(), Some(MethodStatus::NotImplemented));
    let error = load
        .ensure_direction(MethodDirection::ClientToAgent)
        .expect_err("session/load 必须仍是显式不支持");
    assert!(
        matches!(
            error,
            AcpError::Unsupported { ref method, status: MethodStatus::NotImplemented }
                if method == "session/load"
        ),
        "期望 Unsupported(NotImplemented)，实际 {error}"
    );

    // 同一张登记表里 `session/resume` 是已实现的方法，两者的状态不得互相漂移。
    assert_eq!(
        methods::status_of("session/resume"),
        MethodStatus::Implemented
    );
    assert_eq!(ReExportedStatus::Implemented, MethodStatus::Implemented);

    // 其余三个既有方法同样未被顺带提升（回归护栏）。
    for method in ["session/list", "session/delete", "session/close"] {
        assert_eq!(
            methods::status_of(method),
            MethodStatus::NotImplemented,
            "{method} 的既有语义不得改变"
        );
    }
}

/// R1 的登记形态：`session/resume` 是 **条件启用**（`ConditionalMvp`）的 client→agent request。
///
/// 这一条是 R8「只有宣告能力才允许发出」在协议层的登记依据：若实现把它登记成 `Mvp`（无条件），
/// 能力门控在协议层就没有依据，本用例会失败。
#[test]
fn session_resume_is_registered_as_a_conditional_client_to_agent_request() {
    let spec = methods::find("session/resume").expect("session/resume 必须已登记");
    assert_eq!(spec.wire_name, "session/resume");
    assert_eq!(spec.direction, MethodDirection::ClientToAgent);
    assert_eq!(spec.kind, MethodKind::Request);
    assert_eq!(spec.delivery, methods::Delivery::ConditionalMvp);
    assert!(
        spec.implemented,
        "有类型化 DTO 的方法必须登记为已实现（否则解码边界会先报不支持）"
    );
    assert_eq!(
        methods::direction_of("session/resume"),
        Some(MethodDirection::ClientToAgent)
    );
}
