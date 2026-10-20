//! 类型化媒体与有效交互的值对象（`schemas/sync/v1/media.schema.json`）的合同测试。
//!
//! 与附件帧一样，这些值对象不是独立消息：它们的正反样例由 `fixtures/sync/v1/manifest.json` 的
//! `schemaPointer` 声明。这里按指针把样例交给对应的 Rust 类型，证明形状与 schema 同源。

mod support;

use serde_json::Value;
use sync_protocol::common::AgentContentBlock;
use sync_protocol::media::{EffectiveInteraction, MediaReference, MessageChannel, PartialResult};

const MEDIA_SCHEMA_SUFFIX: &str = "schemas/sync/v1/media.schema.json";
const COMMON_SCHEMA_SUFFIX: &str = "schemas/sync/v1/common.schema.json";
const MANIFEST: &str = "fixtures/sync/v1/manifest.json";

fn fixture_text(relative: &str) -> Value {
    support::read_json(&support::repo_path(&format!("fixtures/sync/v1/{relative}")))
}

#[derive(Clone, Copy)]
enum Kind {
    MediaReference,
    EffectiveInteraction,
    PartialResult,
    MessageChannel,
    AgentContentBlock,
}

fn cases() -> Vec<(String, String, bool, Kind)> {
    let manifest = support::read_json(&support::repo_path(MANIFEST));
    let mut out = Vec::new();
    for case in manifest["cases"].as_array().expect("cases") {
        let schema = case["schema"].as_str().expect("schema");
        let pointer = case["schemaPointer"].as_str().unwrap_or("");
        let kind = if schema.ends_with(MEDIA_SCHEMA_SUFFIX) {
            match pointer {
                "#/$defs/mediaReference" => Kind::MediaReference,
                "#/$defs/effectiveInteraction" => Kind::EffectiveInteraction,
                "#/$defs/partialResult" => Kind::PartialResult,
                "#/$defs/messageChannel" => Kind::MessageChannel,
                _ => continue,
            }
        } else if schema.ends_with(COMMON_SCHEMA_SUFFIX) && pointer == "#/$defs/agentContentBlock" {
            Kind::AgentContentBlock
        } else {
            continue;
        };
        out.push((
            case["fixture"].as_str().expect("fixture").to_owned(),
            pointer.to_owned(),
            case["valid"].as_bool().expect("valid"),
            kind,
        ));
    }
    out
}

#[test]
fn every_manifest_media_case_behaves_as_declared() {
    let cases = cases();
    assert_eq!(
        cases.len(),
        10,
        "媒体/内容块片段用例数变化（WP1 登记 10 条；另有 1 条 command 片段不属本文件）"
    );
    let mut seen = Vec::new();
    for (fixture, pointer, valid, kind) in cases {
        let text = fixture_text(&fixture);
        let json = serde_json::to_string(&text).expect("fixture JSON");
        let decoded: Result<(), String> = match kind {
            Kind::MediaReference => serde_json::from_str::<MediaReference>(&json)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Kind::EffectiveInteraction => serde_json::from_str::<EffectiveInteraction>(&json)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Kind::PartialResult => serde_json::from_str::<PartialResult>(&json)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Kind::MessageChannel => serde_json::from_str::<MessageChannel>(&json)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Kind::AgentContentBlock => serde_json::from_str::<AgentContentBlock>(&json)
                .map(|_| ())
                .map_err(|error| error.to_string()),
        };
        if valid {
            assert!(
                decoded.is_ok(),
                "{fixture}（{pointer}）应当解码成功：{:?}",
                decoded.err()
            );
            seen.push(pointer);
        } else {
            assert!(
                decoded.is_err(),
                "{fixture}（{pointer}）应当被拒绝，实际解码成功"
            );
        }
    }
    for expected in [
        "#/$defs/mediaReference",
        "#/$defs/effectiveInteraction",
        "#/$defs/partialResult",
        "#/$defs/messageChannel",
        "#/$defs/agentContentBlock",
    ] {
        assert!(
            seen.iter().any(|pointer| pointer == expected),
            "缺少 {expected} 的合法样例"
        );
    }
}

#[test]
fn channel_round_trips_and_unknown_is_rejected() {
    assert_eq!(
        serde_json::from_str::<MessageChannel>("\"thought\"").expect("thought"),
        MessageChannel::Thought
    );
    assert!(serde_json::from_str::<MessageChannel>("\"narration\"").is_err());
    assert_eq!(MessageChannel::Reply.as_str(), "reply");
}

#[test]
fn partial_result_cannot_be_completed_with_reason() {
    let decoded: PartialResult =
        serde_json::from_str(r#"{"state":"completed","reason":null}"#).expect("completed");
    assert_eq!(decoded, PartialResult::completed());
    // `reason` 是 required（可为 null）：缺键不是「没有原因」，而是形状错误。
    assert!(serde_json::from_str::<PartialResult>(r#"{"state":"completed"}"#).is_err());
}

#[test]
fn media_reference_has_no_url_or_credentials() {
    let reference: MediaReference = serde_json::from_str(
        r#"{
          "attachmentId": "9b2f4c1d-6a3e-4f57-8b21-0c4d5e6f7a89",
          "sessionId": "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
          "kind": "image",
          "mediaType": "image/png",
          "byteLength": "1048576",
          "sha256": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
          "displayName": "screenshot.png",
          "availability": "available"
        }"#,
    )
    .expect("media reference");
    assert!(reference.byte_length.as_str() == "1048576");
    // 任何下载地址/凭据字段都不是这个值对象的一部分（closed shape 直接拒绝）。
    assert!(
        serde_json::from_str::<MediaReference>(
            r#"{
              "attachmentId": "9b2f4c1d-6a3e-4f57-8b21-0c4d5e6f7a89",
              "sessionId": "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
              "kind": "image",
              "mediaType": "image/png",
              "byteLength": "1048576",
              "sha256": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
              "displayName": null,
              "availability": "available",
              "url": "https://example.invalid/secret"
            }"#
        )
        .is_err()
    );
}

#[test]
fn effective_interaction_requires_version_and_options() {
    let decoded: EffectiveInteraction = serde_json::from_str(
        r#"{
          "interactionId": "8a1b2c3d-4e5f-4a6b-8c9d-0e1f2a3b4c5d",
          "sessionId": "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
          "kind": "permission",
          "requestVersion": "1",
          "expiresAt": null,
          "options": [{ "optionId": "allow", "label": "Allow", "kind": "allow_once" }],
          "schema": null
        }"#,
    )
    .expect("effective interaction");
    assert_eq!(decoded.request_version.as_str(), "1");
    assert!(
        serde_json::from_str::<EffectiveInteraction>(
            r#"{
          "interactionId": "8a1b2c3d-4e5f-4a6b-8c9d-0e1f2a3b4c5d",
          "sessionId": "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
          "kind": "permission",
          "expiresAt": null,
          "options": [],
          "schema": null
        }"#
        )
        .is_err()
    );
}
