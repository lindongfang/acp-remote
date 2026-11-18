//! A conservative evidence check, not a replacement ACP decoder.
//! Unknown data stays in the original reply but cannot establish a new independent root.
use super::super::{
    Version,
    raw_json::{RawKind, RawObject, RawValueRef},
};

#[derive(Clone, Copy)]
enum Shape {
    Created(Version),
    Modes,
    Mode,
    Config(Version),
    SelectValue,
    SelectGroup(Version),
    Command,
    CommandInput,
}
impl Shape {
    const fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Created(Version::V1) => &["sessionId", "modes", "configOptions", "_meta"],
            Self::Created(Version::V2) => {
                &["sessionId", "configOptions", "availableCommands", "_meta"]
            }
            Self::Modes => &["currentModeId", "availableModes", "_meta"],
            Self::Mode => &["id", "name", "description", "_meta"],
            Self::Config(Version::V1) => &[
                "id",
                "name",
                "description",
                "category",
                "type",
                "currentValue",
                "options",
                "_meta",
            ],
            Self::Config(Version::V2) => &[
                "configId",
                "name",
                "description",
                "category",
                "type",
                "currentValue",
                "options",
                "_meta",
            ],
            Self::SelectValue => &["value", "name", "description", "_meta"],
            Self::SelectGroup(Version::V1) => &["group", "name", "options", "_meta"],
            Self::SelectGroup(Version::V2) => &["groupId", "name", "options", "_meta"],
            Self::Command => &["type", "name", "description", "input", "_meta"],
            Self::CommandInput => &["type", "hint", "_meta"],
        }
    }
    const fn required(self) -> &'static [&'static str] {
        match self {
            Self::Created(_) => &["sessionId"],
            Self::Modes => &["currentModeId", "availableModes"],
            Self::Mode => &["id", "name"],
            Self::Config(Version::V1) => &["id", "name", "type", "currentValue"],
            Self::Config(Version::V2) => &["configId", "name", "type", "currentValue"],
            Self::SelectValue => &["value", "name"],
            Self::SelectGroup(Version::V1) => &["group", "name", "options"],
            Self::SelectGroup(Version::V2) => &["groupId", "name", "options"],
            Self::Command => &["type", "name", "description"],
            Self::CommandInput => &["type", "hint"],
        }
    }
}

pub(super) fn unextended_created(version: Version, result: RawValueRef<'_>) -> bool {
    check_object(result, Shape::Created(version))
}
pub(super) fn unextended_setup(extra: RawValueRef<'_>) -> bool {
    extra.object().is_some_and(|object| {
        object
            .iter()
            .all(|(key, value)| key == "_meta" && object.get(key).is_ok() && empty_meta(value))
    })
}
pub(super) fn unextended_mcp(server: &crate::adapters::acp::configuration::McpServer) -> bool {
    use crate::adapters::acp::configuration::McpTransport;
    let (fields, pairs) = match server.transport() {
        McpTransport::Stdio => (
            &["type", "name", "command", "args", "env", "_meta"][..],
            "env",
        ),
        McpTransport::Http | McpTransport::Sse => {
            (&["type", "name", "url", "headers", "_meta"][..], "headers")
        }
        McpTransport::Other(_) => return false,
    };
    server.as_raw().object().is_some_and(|object| {
        object.iter().all(|(key, value)| {
            if !fields.contains(&key) || object.get(key).is_err() {
                return false;
            }
            if key == "_meta" {
                return empty_meta(value);
            }
            if key == pairs {
                return value.array().is_some_and(|items| {
                    items.iter().all(|pair| {
                        pair.object().is_some_and(|pair| {
                            pair.iter().all(|(key, value)| {
                                pair.get(key).is_ok()
                                    && match key {
                                        "name" | "value" => value.kind() == RawKind::String,
                                        "_meta" => empty_meta(value),
                                        _ => false,
                                    }
                            })
                        })
                    })
                });
            }
            true
        })
    })
}
fn empty_meta(value: RawValueRef<'_>) -> bool {
    value.kind() == RawKind::Null || value.object().is_some_and(RawObject::is_empty)
}
fn text(object: RawObject<'_>, key: &str, expected: &str) -> bool {
    object
        .get(key)
        .ok()
        .flatten()
        .is_some_and(|value| value.string().is_ok_and(|value| value == expected))
}
fn array(value: RawValueRef<'_>, shape: Shape) -> bool {
    value
        .array()
        .is_some_and(|items| items.iter().all(|item| check_object(item, shape)))
}
fn nullable(value: RawValueRef<'_>, check: impl FnOnce(RawValueRef<'_>) -> bool) -> bool {
    value.kind() == RawKind::Null || check(value)
}
fn check_object(value: RawValueRef<'_>, shape: Shape) -> bool {
    let Some(object) = value.object() else {
        return false;
    };
    if !shape.required().iter().all(|key| {
        object
            .get(key)
            .ok()
            .flatten()
            .is_some_and(|value| value.kind() != RawKind::Null)
    }) {
        return false;
    }
    let fields_valid = object.iter().all(|(key, value)| {
        if !shape.fields().contains(&key) || object.get(key).is_err() {
            return false;
        }
        match key {
            "_meta" => empty_meta(value),
            "modes" => nullable(value, |value| check_object(value, Shape::Modes)),
            "availableModes" => array(value, Shape::Mode),
            "configOptions" => {
                let Shape::Created(version) = shape else {
                    return false;
                };
                nullable(value, |value| array(value, Shape::Config(version)))
            }
            "availableCommands" => nullable(value, |value| array(value, Shape::Command)),
            "input" => nullable(value, |value| check_object(value, Shape::CommandInput)),
            "description" | "category" => nullable(value, |value| value.kind() == RawKind::String),
            "type" => match shape {
                Shape::Config(_) => text(object, key, "select") || text(object, key, "boolean"),
                Shape::Command => text(object, key, "command"),
                Shape::CommandInput => text(object, key, "text"),
                _ => false,
            },
            "currentValue" => {
                if text(object, "type", "boolean") {
                    value.boolean().is_some()
                } else {
                    text(object, "type", "select") && value.kind() == RawKind::String
                }
            }
            "options" => match shape {
                Shape::Config(version) => select_options(value, version),
                Shape::SelectGroup(version) => {
                    array(value, Shape::SelectValue)
                        && (version == Version::V1
                            || value.array().is_some_and(|items| !items.is_empty()))
                }
                _ => false,
            },
            _ => value.kind() == RawKind::String,
        }
    });
    fields_valid
        && match shape {
            Shape::Config(_) if text(object, "type", "select") => {
                object.get("options").ok().flatten().is_some()
            }
            Shape::Config(_) => object.get("options").ok().flatten().is_none(),
            _ => true,
        }
}
fn select_options(value: RawValueRef<'_>, version: Version) -> bool {
    let Some(items) = value.array() else {
        return false;
    };
    if version == Version::V2 && items.is_empty() {
        return false;
    }
    array(value, Shape::SelectValue) || array(value, Shape::SelectGroup(version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::acp::raw_json::RawDocument;
    fn verifies(version: Version, input: &[u8]) -> bool {
        let document = RawDocument::parse(input.to_vec()).unwrap();
        unextended_created(version, document.root())
    }
    #[test]
    fn standard_versioned_ui_fields_preserve_root_evidence() {
        assert!(verifies(Version::V1, br#"{"sessionId":"root","_meta":{},"modes":{"currentModeId":"parent","availableModes":[{"id":"parent","name":"A parent in UI text","description":null,"_meta":null}]},"configOptions":[{"id":"a","name":"A","type":"boolean","currentValue":true}]}"#));
        assert!(verifies(Version::V2, br#"{"sessionId":"root","configOptions":[{"configId":"a","name":"A","type":"select","currentValue":"child","options":[{"groupId":"g","name":"G","options":[{"value":"child","name":"UI value","_meta":{}}]}]}],"availableCommands":[{"type":"command","name":"parent","description":"UI text only","input":{"type":"text","hint":"child","_meta":null}}]}"#));
        for version in [Version::V1, Version::V2] {
            assert!(verifies(
                version,
                br#"{"sessionId":"root","_meta":null,"configOptions":null}"#
            ));
        }
    }
    #[test]
    fn unknown_top_level_nested_metadata_and_future_variants_do_not_prove_roots() {
        for input in [
            &br#"{"sessionId":"root","parentId":"other"}"#[..],
            &br#"{"sessionId":"root","_meta":{"unknown":{"n":1}}}"#[..],
            &br#"{"sessionId":"root","configOptions":[{"configId":"a","name":"A","type":"boolean","currentValue":true,"_meta":{"x":null}}]}"#[..],
            &br#"{"sessionId":"root","configOptions":[{"configId":"a","name":"A","type":"_future","currentValue":true}]}"#[..],
            &br#"{"sessionId":"root","availableCommands":[{"type":"command","name":"a","description":"b","input":{"type":"text","hint":"c","_meta":{"x":1}}}]}"#[..],
            &br#"{"sessionId":"root","sessionId":"root"}"#[..],
        ] {
            assert!(!verifies(Version::V2, input));
        }
        assert!(!verifies(
            Version::V2,
            br#"{"sessionId":"root","modes":null}"#
        ));
        assert!(!verifies(
            Version::V1,
            br#"{"sessionId":"root","availableCommands":[]}"#
        ));
    }
    #[test]
    fn nested_mcp_extensions_do_not_supply_standard_create_evidence() {
        use crate::adapters::acp::configuration::McpServer;
        for version in [Version::V1, Version::V2] {
            for (input, expected) in [
                (&br#"{"type":"stdio","name":"m","command":"C:/m.exe","args":[],"env":[{"name":"A","value":"B","_meta":{}}]}"#[..], true),
                (&br#"{"type":"stdio","name":"m","command":"C:/m.exe","args":[],"env":[{"name":"A","value":"B","x":null}]}"#[..], false),
                (&br#"{"type":"http","name":"m","url":"https://example.test","headers":[{"name":"A","value":"B","_meta":{"x":1}}]}"#[..], false),
                (&br#"{"type":"http","name":"m","url":"https://example.test","headers":[],"x":null}"#[..], false),
            ] {
                let document = RawDocument::parse(input.to_vec()).unwrap();
                let server = McpServer::from_raw(version, document.root_owned()).unwrap();
                assert_eq!(unextended_mcp(&server), expected);
            }
        }
    }
}
