#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use std::sync::Arc;
use support::*;

#[test]
fn rt17_toolnamepatch_transport_configstamp_and_public_projection_debug_redact() {
    let secret = "SENSITIVE_SENTINEL_UNTRUSTED";
    let text: Arc<str> = Arc::from(secret);
    let native = raw_value(&json!(secret));
    let message = MessageId::parse(native.as_ref()).unwrap();
    let session = SessionId::parse(native.as_ref()).unwrap();
    let tool = ToolCallId::parse(native.as_ref()).unwrap();
    let option = OptionId::parse(native.as_ref()).unwrap();
    let stamp = ConfigSourceStamp {
        reference: text.clone(),
        revision: 7,
    };
    let snapshot = SetupSnapshot {
        wire: Version::V2,
        source: stamp.clone(),
        cwd: Known::Known(text.clone()),
        mcp_servers: Known::Known(vec![]),
        additional_directories: Known::Known(vec![text.clone()]),
        extra: ExtensionFields::empty().unwrap(),
    };
    let debug = [
        format!("{:?}", ToolNamePatch::Set(text.clone())),
        format!("{:?}", McpTransport::Other(text.clone())),
        format!("{stamp:?}"),
        format!("{snapshot:?}"),
        format!("{:?}", Known::Known(text.clone())),
        format!("{message:?}"),
        format!("{session:?}"),
        format!("{tool:?}"),
        format!("{option:?}"),
        format!("{:?}", RpcId::String(text.clone())),
        format!("{:?}", ReportedDirectories::Known(vec![text.clone()])),
        format!(
            "{:?}",
            PromptResult::Completed {
                stop_reason: text.clone()
            }
        ),
        format!("{:?}", PromptResult::Inserted { message }),
        format!(
            "{:?}",
            SessionEvent::State {
                session: session.clone(),
                state: text.clone(),
                stop_reason: Some(text.clone())
            }
        ),
        format!(
            "{:?}",
            SessionEvent::Other {
                session,
                kind: text
            }
        ),
    ];
    for rendered in debug {
        assert!(!rendered.contains(secret));
    }
}
