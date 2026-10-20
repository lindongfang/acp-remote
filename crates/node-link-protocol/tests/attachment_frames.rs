//! 附件传输帧的合同测试（Node Link 面）：同一份 `fixtures/node-link/v1/manifest.json` 正反样例、
//! registry 的帧表与预算、以及节点主体与解码错误分类（`docs/NODE_LINK_PROTOCOL.md` §12.9）。

mod support;

use node_link_protocol::attachment::{
    BeginBody, ChunkData, DecodeError, Frame, FrameBody, FrameType,
    MAX_ACTIVE_TRANSFERS_PER_DEVICE, MAX_ACTIVE_TRANSFERS_PER_NODE, MAX_UNACKNOWLEDGED_CHUNKS,
};
use serde_json::Value;

const FIXTURE_ROOT: &str = "fixtures/node-link/v1";
const MANIFEST: &str = "fixtures/node-link/v1/manifest.json";
const ATTACHMENT_SCHEMA_SUFFIX: &str = "schemas/node-link/v1/attachment.schema.json";
const REGISTRY: &str = "compatibility/commands/v1/commands.json";

#[derive(Debug, Clone)]
struct Case {
    fixture: String,
    valid: bool,
}

fn manifest_cases() -> Vec<Case> {
    let manifest = support::read_json(&support::repo_path(MANIFEST));
    manifest["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .filter(|case| {
            case["schema"]
                .as_str()
                .is_some_and(|schema| schema.ends_with(ATTACHMENT_SCHEMA_SUFFIX))
        })
        .map(|case| Case {
            fixture: case["fixture"].as_str().expect("fixture").to_owned(),
            valid: case["valid"].as_bool().expect("valid"),
        })
        .collect()
}

fn fixture_text(relative: &str) -> String {
    let path = support::repo_path(&format!("{FIXTURE_ROOT}/{relative}"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("无法读取 {}：{error}", path.display()))
}

#[test]
fn every_manifest_case_behaves_as_declared() {
    let cases = manifest_cases();
    let valid = cases.iter().filter(|case| case.valid).count();
    let invalid = cases.len() - valid;
    assert_eq!(valid, 17, "合法附件帧用例数变化");
    assert_eq!(invalid, 7, "非法附件帧用例数变化");

    for case in &cases {
        let text = fixture_text(&case.fixture);
        let decoded = Frame::decode(&text);
        if case.valid {
            let frame = decoded
                .unwrap_or_else(|error| panic!("{} 应当解码成功，实际 {error:?}", case.fixture));
            let encoded = frame
                .encode()
                .unwrap_or_else(|error| panic!("{} 无法重新编码：{error}", case.fixture));
            let expected: Value = serde_json::from_str(&text).expect("fixture JSON");
            let actual: Value = serde_json::from_str(&encoded).expect("encoded JSON");
            assert_eq!(actual, expected, "{} 编解码往返改变了值", case.fixture);
            assert_eq!(
                Frame::decode(&encoded).expect("重新编码的帧必须仍可解码"),
                frame,
                "{} 往返后类型不收敛",
                case.fixture
            );
        } else {
            assert!(
                decoded.is_err(),
                "{} 应当被拒绝，实际解码成功",
                case.fixture
            );
            assert_ne!(
                decoded.expect_err("已断言错误"),
                DecodeError::InvalidJson,
                "{} 是 schema 形状错误，不应被归为语法错误",
                case.fixture
            );
        }
    }
}

#[test]
fn frame_types_and_budget_match_registry() {
    let registry = support::read_json(&support::repo_path(REGISTRY));
    let entry = registry["transportFrames"]
        .as_array()
        .expect("transportFrames")
        .iter()
        .find(|entry| entry["protocol"].as_str() == Some("node_link"))
        .expect("node_link transportFrames");
    assert_eq!(
        entry["feature"].as_str(),
        Some("node-link.attachment-transfer.v1")
    );
    let frames: Vec<&str> = entry["frames"]
        .as_array()
        .expect("frames")
        .iter()
        .map(|frame| frame.as_str().expect("frame name"))
        .collect();
    let expected: Vec<&str> = FrameType::ALL.iter().map(|frame| frame.as_str()).collect();
    assert_eq!(frames, expected, "registry 的帧表与 Rust 词表必须逐项相等");

    let budget = &entry["budget"];
    assert_eq!(budget["chunkSize"], 32768);
    assert_eq!(budget["maxUnacknowledgedChunks"], MAX_UNACKNOWLEDGED_CHUNKS);
    assert_eq!(
        budget["maxActiveTransfersPerDevice"],
        MAX_ACTIVE_TRANSFERS_PER_DEVICE
    );
    assert_eq!(
        budget["maxActiveTransfersPerNode"],
        MAX_ACTIVE_TRANSFERS_PER_NODE
    );

    let schema = support::read_json(&support::repo_path(
        "schemas/node-link/v1/attachment.schema.json",
    ));
    let enum_values: Vec<&str> = schema["$defs"]["frameType"]["enum"]
        .as_array()
        .expect("frameType.enum")
        .iter()
        .map(|value| value.as_str().expect("frame name"))
        .collect();
    assert_eq!(
        enum_values, expected,
        "schema 的帧表与 Rust 词表必须逐项相等"
    );
}

#[test]
fn subject_kind_is_node_only() {
    let text = fixture_text("attachment/valid/begin.json");
    let frame = Frame::decode(&text).expect("begin");
    let FrameBody::Begin(BeginBody { subject, .. }) = frame.body else {
        panic!("begin 帧的 body 分类错误");
    };
    assert_eq!(subject.id.as_str(), "3a4b5c6d-7e8f-4a9b-8c0d-1e2f3a4b5c6d");

    // 同一份正例换成 `device` 是跨主体冒用（manifest 的 `begin-subject-device` 已覆盖）。
    let device_subject = text.replace(
        r#"{ "kind": "node", "id": "3a4b5c6d-7e8f-4a9b-8c0d-1e2f3a4b5c6d" }"#,
        r#"{ "kind": "device", "id": "3a4b5c6d-7e8f-4a9b-8c0d-1e2f3a4b5c6d" }"#,
    );
    assert_eq!(Frame::decode(&device_subject), Err(DecodeError::Schema));
}

#[test]
fn unknown_type_version_and_sequence_are_distinguished() {
    let begin = fixture_text("attachment/valid/begin.json");
    assert_eq!(
        Frame::decode(&begin.replace("attachment.begin", "attachment.teleport")),
        Err(DecodeError::TypeUnsupported)
    );
    assert_eq!(
        Frame::decode(&begin.replace("\"protocolVersion\": 1", "\"protocolVersion\": 2")),
        Err(DecodeError::VersionUnsupported)
    );
    assert_eq!(Frame::decode("{"), Err(DecodeError::InvalidJson));
}

#[test]
fn chunk_data_is_bounded_to_32k() {
    assert!(ChunkData::from_bytes(vec![0_u8; 32768]).is_ok());
    assert!(ChunkData::from_bytes(vec![0_u8; 32769]).is_err());
    assert!(ChunkData::from_bytes(Vec::new()).is_err());
}
