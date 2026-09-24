//! capability：解码保真 + 宣告必须真实。

mod support;

use acp_protocol::capability::{AgentCapabilities, ClientCapabilities, PROTOCOL_VERSION_V1};
use acp_protocol::message::InitializeRequest;

#[test]
fn agent_capabilities_are_read_truthfully_and_preserve_unknown_fields() {
    let bytes = support::fixture_bytes("initialize-capabilities.json");
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("fixture 是合法 JSON");
    let capabilities: AgentCapabilities =
        serde_json::from_value(value["result"]["agentCapabilities"].clone()).expect("能力可解码");

    assert!(capabilities.supports_load_session());
    assert!(capabilities.supports_prompt_image());
    assert!(
        !capabilities.supports_prompt_audio(),
        "fixture 明确写了 false"
    );
    assert!(capabilities.supports_embedded_context());
    assert!(capabilities.supports_session_list());
    assert!(capabilities.supports_session_resume());
    assert!(capabilities.supports_session_close());
    assert!(!capabilities.supports_session_delete(), "未宣告即未支持");

    // 未识别的能力字段与 `_meta` 必须保留（保真）。
    assert_eq!(
        capabilities
            .meta
            .as_ref()
            .and_then(|meta| meta.get("example.com/agent-feature")),
        Some(&serde_json::json!(true))
    );
    let mcp = capabilities.mcp_capabilities.as_ref().expect("mcp");
    assert_eq!(mcp.http, Some(true));
    assert_eq!(mcp.sse, Some(false));
}

#[test]
fn unknown_agent_capability_fields_survive_re_serialization() {
    let input = serde_json::json!({
        "loadSession": true,
        "exampleComUnknownCapability": { "nested": [1, 2, 3] },
        "_meta": { "example.com/x": "y" }
    });
    let capabilities: AgentCapabilities = serde_json::from_value(input.clone()).expect("可解码");
    let round_trip = serde_json::to_value(&capabilities).expect("可序列化");
    assert_eq!(
        round_trip["exampleComUnknownCapability"], input["exampleComUnknownCapability"],
        "未识别能力字段必须原样保留"
    );
    assert_eq!(round_trip["_meta"], input["_meta"]);
}

#[test]
fn default_client_capabilities_advertise_nothing() {
    // 这是「不能虚报支持」的落点：没有实现的能力不得以默认值/空对象表达成已支持。
    let capabilities = ClientCapabilities::none();
    let value = serde_json::to_value(&capabilities).expect("可序列化");
    assert_eq!(value, serde_json::json!({}), "未宣告任何能力时必须是空对象");
    assert!(!capabilities.supports_elicitation_form());

    for forbidden in ["fs", "terminal", "elicitation", "session", "auth"] {
        assert!(
            value.get(forbidden).is_none(),
            "不得为未实现的能力输出字段 {forbidden}"
        );
    }
}

#[test]
fn initialize_request_declares_only_what_it_has() {
    let request = InitializeRequest::new(ClientCapabilities::none());
    assert_eq!(request.protocol_version, PROTOCOL_VERSION_V1);
    let value = serde_json::to_value(&request).expect("可序列化");
    assert_eq!(value["protocolVersion"], serde_json::json!(1));
    assert_eq!(
        value["clientCapabilities"],
        serde_json::json!({}),
        "能力声明必须是空对象，而不是省略字段或用 null 占位"
    );
    assert!(value.get("clientInfo").is_none());
}

#[test]
fn explicitly_declared_capabilities_are_emitted() {
    // 反过来：真的宣告时必须真的出现在 wire 上（这里的 elicitation.form 由组合根显式选择）。
    let mut capabilities = ClientCapabilities::none();
    capabilities.elicitation =
        Some(serde_json::from_value(serde_json::json!({ "form": {} })).expect("elicitation"));
    assert!(capabilities.supports_elicitation_form());
    let value = serde_json::to_value(&capabilities).expect("可序列化");
    assert_eq!(value["elicitation"]["form"], serde_json::json!({}));
    assert!(value.get("fs").is_none());
}
