//! Classifies original ACP frames without inspecting message content.
use super::{
    AcpError, RawMessage, Version,
    model::{field, text},
    strategy::update_message_id,
};
use crate::domain::history_page::FrameClass;

#[must_use]
pub fn classify(version: Version, raw: &RawMessage) -> FrameClass {
    classify_frame(version, raw).unwrap_or(FrameClass::Unclassifiable)
}

fn classify_frame(version: Version, raw: &RawMessage) -> Result<FrameClass, AcpError> {
    if raw
        .field("method")?
        .and_then(|v| v.string().ok())
        .as_deref()
        != Some("session/update")
    {
        return Ok(FrameClass::Notification);
    }
    let update = field(
        raw.field("params")?.ok_or(AcpError::InvalidMessage)?,
        "update",
    )?;
    let tag = text(field(update, "sessionUpdate")?)?;
    Ok(match tag.as_ref() {
        "user_message" | "user_message_chunk" => {
            let id = update_message_id(version, update)?;
            FrameClass::User(
                id.map(|id| id.as_str().to_owned())
                    .filter(|id| !id.is_empty()),
            )
        }
        "agent_message" | "agent_message_chunk" | "agent_thought" | "agent_thought_chunk" => {
            update_message_id(version, update)?;
            FrameClass::Assistant
        }
        _ => FrameClass::Notification,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn frame(tag: &str, id: Option<Value>) -> RawMessage {
        let mut update = json!({"sessionUpdate":tag});
        if let Some(id) = id {
            update["messageId"] = id;
        }
        RawMessage::parse(serde_json::to_vec(&json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":update}})).unwrap()).unwrap()
    }
    #[test]
    fn history_page_classifies_negotiated_user_ids_responses_and_side_events() {
        for tag in ["user_message", "user_message_chunk"] {
            for version in [Version::V1, Version::V2] {
                assert!(
                    classify(version, &frame(tag, Some(json!("x"))))
                        == FrameClass::User(Some("x".into()))
                );
                assert!(classify(version, &frame(tag, None)) == FrameClass::User(None));
            }
            assert!(
                classify(Version::V1, &frame(tag, Some(Value::Null))) == FrameClass::User(None)
            );
            assert!(
                classify(Version::V2, &frame(tag, Some(Value::Null))) == FrameClass::Unclassifiable
            );
        }
        for tag in [
            "agent_message",
            "agent_message_chunk",
            "agent_thought",
            "agent_thought_chunk",
        ] {
            assert!(classify(Version::V1, &frame(tag, None)) == FrameClass::Assistant);
            assert!(
                classify(Version::V2, &frame(tag, Some(Value::Null))) == FrameClass::Unclassifiable
            );
        }
        for tag in ["tool_call", "tool_call_update", "state_update", "future"] {
            assert!(classify(Version::V2, &frame(tag, None)) == FrameClass::Notification);
        }
    }
}
