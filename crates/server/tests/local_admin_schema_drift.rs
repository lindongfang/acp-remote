//! `schemas/local-admin/v1/envelope.schema.json` 与 `fixtures/local-admin/v1/` 的常驻漂移测试。
//!
//! 这是「词表只有一处机器定义」这条规则的判据（`schemas/local-admin/v1/README.md`）：Rust 侧的方法集、
//! 错误码、framing 常量与信封形状必须与 schema 逐项相等；specs `local-admin-methods` 的「管理信封校验」
//! 需求（R33–R36）由 fixture 的 valid/invalid 逐条往返/拒绝测试覆盖。
//!
//! fixture 清单在 Rust 侧显式列出（`include_str!` 需要字面量路径），并与 `manifest.json` 逐条比对：
//! 新增或改动 fixture 而不同步这里会让本测试失败，因此不会出现「清单漂移但没人发现」。

use serde_json::Value;
use server::local_admin::{
    AdminOutcome, AdminResponse, CHANNEL_VERSION, LocalErrorCode, Method, RequestDecodeError,
    ResponseDecodeError, decode_request,
};
use server::transport::local::{
    CHANNEL_ACP_STREAM_BYTE, CHANNEL_LOCAL_ADMIN_BYTE, LENGTH_PREFIX_BYTES,
    MAX_FRAME_PAYLOAD_BYTES, MAX_IN_FLIGHT_REQUESTS,
};

const SCHEMA: &str = include_str!("../../../schemas/local-admin/v1/envelope.schema.json");
const MANIFEST: &str = include_str!("../../../fixtures/local-admin/v1/manifest.json");

/// fixture 在 Rust 侧期望的行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expectation {
    /// 请求信封可解码（且方法、params 与文档 §5 的形状一致）。
    RequestDecodes,
    /// 响应信封可解码且可再编码（再编码后再次解码必须相等）。
    ResponseDecodes,
    /// 连接级关闭（版本不可判定，不发错误帧）。
    ConnectionCloses,
    /// 回 `local.invalid_request`（信封非法，包括方法名命名非法）。
    InvalidRequest,
    /// 回 `local.unsupported`（方法名语法合法但不在 v1 方法集里，§4 规则 3）。
    UnsupportedMethod,
    /// 响应解码失败（响应信封非法）。
    ResponseRejected,
}

/// 一条 fixture 的声明：路径、内容、manifest 的 valid/expectedKeyword，以及 Rust 侧的期望。
struct Fixture {
    path: &'static str,
    content: &'static str,
    valid: bool,
    keyword: Option<&'static str>,
    expectation: Expectation,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        path: "valid/request-daemon-status.json",
        content: include_str!("../../../fixtures/local-admin/v1/valid/request-daemon-status.json"),
        valid: true,
        keyword: None,
        expectation: Expectation::RequestDecodes,
    },
    Fixture {
        path: "valid/request-agent-configure.json",
        content: include_str!(
            "../../../fixtures/local-admin/v1/valid/request-agent-configure.json"
        ),
        valid: true,
        keyword: None,
        expectation: Expectation::RequestDecodes,
    },
    Fixture {
        path: "valid/request-device-pair-confirm.json",
        content: include_str!(
            "../../../fixtures/local-admin/v1/valid/request-device-pair-confirm.json"
        ),
        valid: true,
        keyword: None,
        expectation: Expectation::RequestDecodes,
    },
    Fixture {
        path: "valid/request-node-rotate-key-begin.json",
        content: include_str!(
            "../../../fixtures/local-admin/v1/valid/request-node-rotate-key-begin.json"
        ),
        valid: true,
        keyword: None,
        expectation: Expectation::RequestDecodes,
    },
    Fixture {
        path: "valid/success-response.json",
        content: include_str!("../../../fixtures/local-admin/v1/valid/success-response.json"),
        valid: true,
        keyword: None,
        expectation: Expectation::ResponseDecodes,
    },
    Fixture {
        path: "valid/error-response.json",
        content: include_str!("../../../fixtures/local-admin/v1/valid/error-response.json"),
        valid: true,
        keyword: None,
        expectation: Expectation::ResponseDecodes,
    },
    Fixture {
        path: "invalid/request-missing-method.json",
        content: include_str!(
            "../../../fixtures/local-admin/v1/invalid/request-missing-method.json"
        ),
        valid: false,
        keyword: Some("required"),
        expectation: Expectation::InvalidRequest,
    },
    Fixture {
        path: "invalid/request-unknown-method.json",
        content: include_str!(
            "../../../fixtures/local-admin/v1/invalid/request-unknown-method.json"
        ),
        valid: false,
        keyword: Some("enum"),
        // schema 用 `enum` 拒绝它（方法集是封闭词表）。运行期答案由 §4 规则 3 决定：`device.rotate-key.begin`
        // 语法合法（§4 的方法名正则允许段内连字符）但不在集内 → `local.unsupported`；集内未实现的
        // `node.rotate-key.begin` 走同一条路径，由 §5.7 规定。
        expectation: Expectation::UnsupportedMethod,
    },
    Fixture {
        path: "invalid/request-method-name-leading-hyphen.json",
        content: include_str!(
            "../../../fixtures/local-admin/v1/invalid/request-method-name-leading-hyphen.json"
        ),
        valid: false,
        keyword: Some("pattern"),
        // 连字符只允许出现在段的内部：段首连字符不匹配 §4 的方法名正则 → 「命名非法」→ `local.invalid_request`。
        expectation: Expectation::InvalidRequest,
    },
    Fixture {
        path: "invalid/request-params-null.json",
        content: include_str!("../../../fixtures/local-admin/v1/invalid/request-params-null.json"),
        valid: false,
        keyword: Some("type"),
        expectation: Expectation::InvalidRequest,
    },
    Fixture {
        path: "invalid/request-version-two.json",
        content: include_str!("../../../fixtures/local-admin/v1/invalid/request-version-two.json"),
        valid: false,
        keyword: Some("const"),
        expectation: Expectation::ConnectionCloses,
    },
    Fixture {
        path: "invalid/response-unknown-error-code.json",
        content: include_str!(
            "../../../fixtures/local-admin/v1/invalid/response-unknown-error-code.json"
        ),
        valid: false,
        keyword: Some("enum"),
        expectation: Expectation::ResponseRejected,
    },
    Fixture {
        path: "invalid/response-ok-without-result.json",
        content: include_str!(
            "../../../fixtures/local-admin/v1/invalid/response-ok-without-result.json"
        ),
        valid: false,
        keyword: Some("required"),
        expectation: Expectation::ResponseRejected,
    },
];

fn schema() -> Value {
    serde_json::from_str(SCHEMA).expect("schema 是合法 JSON")
}

fn manifest() -> Value {
    serde_json::from_str(MANIFEST).expect("manifest 是合法 JSON")
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("数组")
        .iter()
        .map(|item| item.as_str().expect("字符串").to_string())
        .collect()
}

#[test]
fn framing_constants_match_the_schema() {
    let schema = schema();
    let framing = &schema["$defs"]["framing"]["const"];
    assert_eq!(
        framing["lengthPrefixBytes"].as_u64(),
        Some(LENGTH_PREFIX_BYTES as u64)
    );
    assert_eq!(
        framing["maxPayloadBytes"].as_u64(),
        Some(MAX_FRAME_PAYLOAD_BYTES as u64)
    );
    assert_eq!(
        framing["maxInFlightRequests"].as_u64(),
        Some(MAX_IN_FLIGHT_REQUESTS as u64)
    );
    assert_eq!(
        framing["channelLocalAdmin"].as_u64(),
        Some(u64::from(CHANNEL_LOCAL_ADMIN_BYTE))
    );
    assert_eq!(
        framing["channelAcpStream"].as_u64(),
        Some(u64::from(CHANNEL_ACP_STREAM_BYTE))
    );
}

#[test]
fn method_set_matches_the_schema_enum() {
    let schema = schema();
    let declared = string_array(&schema["$defs"]["methodName"]["enum"]);
    let rust: Vec<String> = Method::ALL
        .iter()
        .map(|method| method.as_str().to_string())
        .collect();
    assert_eq!(declared, rust, "方法集必须与 schema 的 enum 逐项相等");
    assert_eq!(
        schema["$defs"]["methodName"]["pattern"].as_str(),
        Some("^[a-z][a-z0-9]*(\\.[a-z0-9]+(-[a-z0-9]+)*)*$"),
        "方法名语法必须与文档 §4 的表格一致（段内允许连字符，段首/段尾不得为连字符）"
    );
    for name in &declared {
        assert_eq!(
            Method::from_name(name).map(Method::as_str),
            Some(name.as_str())
        );
    }
}

#[test]
fn error_codes_match_the_schema_enum() {
    let schema = schema();
    let declared = string_array(&schema["$defs"]["errorCode"]["enum"]);
    let rust: Vec<String> = LocalErrorCode::ALL
        .iter()
        .map(|code| code.as_str().to_string())
        .collect();
    assert_eq!(declared, rust, "本地错误码必须与 schema 的 enum 逐项相等");
    for code in LocalErrorCode::ALL {
        assert_eq!(LocalErrorCode::parse(code.as_str()), Some(code));
    }
}

#[test]
fn envelope_shapes_match_the_schema() {
    let schema = schema();
    assert_eq!(
        schema["$defs"]["uuid"]["pattern"].as_str(),
        Some("^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
    );

    let request = &schema["$defs"]["request"];
    assert_eq!(request["additionalProperties"], Value::Bool(false));
    assert_eq!(
        string_array(&request["required"]),
        vec!["v", "id", "method", "params"]
    );
    assert_eq!(
        request["properties"]["params"]["type"].as_str(),
        Some("object")
    );

    let success = &schema["$defs"]["successResponse"];
    assert_eq!(success["additionalProperties"], Value::Bool(false));
    assert_eq!(
        string_array(&success["required"]),
        vec!["v", "id", "ok", "result"]
    );
    assert_eq!(success["properties"]["ok"]["const"], Value::Bool(true));
    assert_eq!(
        success["properties"]["result"]["type"].as_str(),
        Some("object")
    );

    let error = &schema["$defs"]["errorResponse"];
    assert_eq!(error["additionalProperties"], Value::Bool(false));
    assert_eq!(
        string_array(&error["required"]),
        vec!["v", "id", "ok", "error"]
    );
    assert_eq!(error["properties"]["ok"]["const"], Value::Bool(false));
    let error_body = &error["properties"]["error"];
    assert_eq!(error_body["additionalProperties"], Value::Bool(false));
    assert_eq!(
        string_array(&error_body["required"]),
        vec!["code", "message"]
    );
    assert_eq!(
        error_body["properties"]["message"]["minLength"].as_u64(),
        Some(1)
    );
    assert_eq!(
        error_body["properties"]["message"]["maxLength"].as_u64(),
        Some(512)
    );

    for branch in ["request", "successResponse", "errorResponse"] {
        assert_eq!(
            schema["$defs"][branch]["properties"]["v"]["const"].as_u64(),
            Some(CHANNEL_VERSION),
            "{branch} 的 v 必须固定为 1"
        );
    }
}

#[test]
fn fixture_table_matches_the_manifest() {
    let manifest = manifest();
    let cases = manifest["cases"].as_array().expect("cases 数组");
    assert_eq!(
        cases.len(),
        FIXTURES.len(),
        "fixture 清单与 manifest 数量不一致"
    );

    for case in cases {
        let path = case["fixture"].as_str().expect("fixture 路径");
        let entry = FIXTURES
            .iter()
            .find(|fixture| fixture.path == path)
            .unwrap_or_else(|| panic!("manifest 声明了 {path}，但 Rust 侧清单里没有"));
        assert_eq!(
            case["valid"].as_bool().expect("valid"),
            entry.valid,
            "{path} 的 valid 声明不一致"
        );
        assert_eq!(
            case["expectedKeyword"].as_str(),
            entry.keyword,
            "{path} 的 expectedKeyword 不一致"
        );
        assert!(
            case["schema"]
                .as_str()
                .expect("schema 路径")
                .ends_with("schemas/local-admin/v1/envelope.schema.json"),
            "{path} 必须由信封 schema 校验"
        );
    }
    for entry in FIXTURES {
        assert!(
            cases
                .iter()
                .any(|case| case["fixture"].as_str() == Some(entry.path)),
            "Rust 侧清单里的 {} 未在 manifest 声明",
            entry.path
        );
    }
}

#[test]
fn fixtures_behave_as_declared() {
    for fixture in FIXTURES {
        let payload = fixture.content.as_bytes();
        match fixture.expectation {
            Expectation::RequestDecodes => {
                let request = decode_request(payload)
                    .unwrap_or_else(|error| panic!("{} 应可解码：{error:?}", fixture.path));
                let expected_method = match fixture.path {
                    "valid/request-daemon-status.json" => "daemon.status",
                    "valid/request-agent-configure.json" => "agent.configure",
                    "valid/request-device-pair-confirm.json" => "device.pair.confirm",
                    "valid/request-node-rotate-key-begin.json" => "node.rotate-key.begin",
                    other => panic!("未登记的请求 fixture：{other}"),
                };
                assert_eq!(
                    request.method().as_str(),
                    expected_method,
                    "{} 的方法名",
                    fixture.path
                );
                // `params` 是开放容器（形状以文档 §5 为权威）：这里只断言 fixture 声明的字段名原样存在，
                // 无参数方法必须是 `{}`（§4 的表格）。
                let expected_params: &[&str] = match fixture.path {
                    "valid/request-daemon-status.json" => &[],
                    "valid/request-agent-configure.json" => &[
                        "agentId",
                        "displayName",
                        "command",
                        "args",
                        "envAllowlist",
                        "default",
                    ],
                    "valid/request-device-pair-confirm.json" => &["pairingId", "scopes"],
                    // §5.7 的方法只登记了名字，无参数（`{}`）。
                    "valid/request-node-rotate-key-begin.json" => &[],
                    other => panic!("未登记的请求 fixture：{other}"),
                };
                let mut actual: Vec<&str> = request.params().keys().map(String::as_str).collect();
                actual.sort_unstable();
                let mut expected = expected_params.to_vec();
                expected.sort_unstable();
                assert_eq!(actual, expected, "{} 的 params 字段", fixture.path);
            }
            Expectation::ResponseDecodes => {
                let response = AdminResponse::decode(payload)
                    .unwrap_or_else(|error| panic!("{} 应可解码：{error:?}", fixture.path));
                let reencoded = response.encode().expect("响应可再编码");
                assert_eq!(
                    AdminResponse::decode(&reencoded).expect("再编码后可解码"),
                    response,
                    "{} 的往返必须相等",
                    fixture.path
                );
            }
            Expectation::ConnectionCloses => {
                assert_eq!(
                    decode_request(payload),
                    Err(RequestDecodeError::ChannelVersionUnsupported),
                    "{} 必须触发连接级关闭",
                    fixture.path
                );
            }
            Expectation::InvalidRequest => {
                let error = decode_request(payload).expect_err("必须被拒绝");
                match error.into_outcome() {
                    server::local_admin::RequestDecodeOutcome::Respond(response) => {
                        assert!(matches!(
                            response.outcome(),
                            AdminOutcome::Failure { error }
                                if error.code() == LocalErrorCode::InvalidRequest
                        ));
                    }
                    server::local_admin::RequestDecodeOutcome::CloseConnection => {
                        panic!("{} 应回错误帧而不是关闭连接", fixture.path)
                    }
                }
            }
            Expectation::UnsupportedMethod => {
                let error = decode_request(payload).expect_err("必须被拒绝");
                match error.into_outcome() {
                    server::local_admin::RequestDecodeOutcome::Respond(response) => {
                        assert!(
                            matches!(
                                response.outcome(),
                                AdminOutcome::Failure { error }
                                    if error.code() == LocalErrorCode::Unsupported
                            ),
                            "{} 应回 local.unsupported（§4 规则 3）",
                            fixture.path
                        );
                    }
                    server::local_admin::RequestDecodeOutcome::CloseConnection => {
                        panic!("{} 应回错误帧而不是关闭连接", fixture.path)
                    }
                }
            }
            Expectation::ResponseRejected => {
                assert!(
                    matches!(
                        AdminResponse::decode(payload),
                        Err(ResponseDecodeError::ErrorCodeUnknown)
                            | Err(ResponseDecodeError::ResultInvalid)
                    ),
                    "{} 必须被响应解码器拒绝",
                    fixture.path
                );
            }
        }
    }
}
