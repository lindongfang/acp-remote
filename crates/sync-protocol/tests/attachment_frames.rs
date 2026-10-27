//! 附件传输帧的合同测试：同一份 `fixtures/sync/v1/manifest.json` 正反样例、registry 的帧表与预算、
//! 以及解码错误的分类（`docs/SYNC_PROTOCOL.md` §3.4）。
//!
//! 附件帧**不是** `message.schema.json` 的信封消息，因此 `tests/envelope_fixtures.rs` 显式跳过它们；
//! 本文件是它们的 Rust 侧消费者：每条 manifest 用例都必须按声明的 `valid` 通过或被拒，合法帧还必须
//! 编解码往返保真。

mod support;

use serde_json::Value;
use sync_protocol::attachment::{
    ChunkData, DecodeError, Frame, FrameBody, FrameType, MAX_ACTIVE_TRANSFERS_PER_DEVICE,
    MAX_ACTIVE_TRANSFERS_PER_NODE, MAX_DATA_CHARS, MAX_UNACKNOWLEDGED_CHUNKS,
};
use sync_protocol::common::{DecimalString, Uuid};

const FIXTURE_ROOT: &str = "fixtures/sync/v1";
const MANIFEST: &str = "fixtures/sync/v1/manifest.json";
const ATTACHMENT_SCHEMA_SUFFIX: &str = "schemas/sync/v1/attachment.schema.json";
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
    // 准备附件目标（`begin`/`manifest_begin` 的 `target` 分支与五个准备错误码夹具）已随准备能力
    // 整体移除，合法用例由 24 降为 17、非法用例由 9 降为 7。
    assert_eq!(valid, 17, "合法附件帧用例数变化");
    assert_eq!(invalid, 7, "非法附件帧用例数变化");

    for case in &cases {
        let text = fixture_text(&case.fixture);
        let decoded = Frame::decode(&text);
        if case.valid {
            let frame = decoded
                .unwrap_or_else(|error| panic!("{} 应当解码成功，实际 {error:?}", case.fixture));
            // 重新编码后与 fixture 的 JSON 值必须相等（成员顺序由结构体固定，值与嵌套逐字保留）。
            let encoded = frame
                .encode()
                .unwrap_or_else(|error| panic!("{} 无法重新编码：{error}", case.fixture));
            let expected: Value = serde_json::from_str(&text).expect("fixture JSON");
            let actual: Value = serde_json::from_str(&encoded).expect("encoded JSON");
            assert_eq!(actual, expected, "{} 编解码往返改变了值", case.fixture);
            // 再解一次同值：round-trip 稳定。
            let again = Frame::decode(&encoded).expect("重新编码的帧必须仍可解码");
            assert_eq!(again, frame, "{} 往返后类型不收敛", case.fixture);
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
        .find(|entry| entry["protocol"].as_str() == Some("sync"))
        .expect("sync transportFrames");
    assert_eq!(
        entry["feature"].as_str(),
        Some("core.attachment-transfer.v1")
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

    // schema 的 `frameType.enum` 与 `transferBudget` 常量同样只此一份。
    let schema = support::read_json(&support::repo_path(
        "schemas/sync/v1/attachment.schema.json",
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
    assert_eq!(
        schema["$defs"]["transferBudget"]["properties"]["chunkSize"]["const"],
        32768
    );
}

#[test]
fn unknown_type_is_type_unsupported() {
    let text = fixture_text("attachment/valid/begin.json")
        .replace("attachment.begin", "attachment.teleport");
    assert_eq!(Frame::decode(&text), Err(DecodeError::TypeUnsupported));
}

#[test]
fn version_two_is_version_unsupported() {
    let text = fixture_text("attachment/valid/begin.json")
        .replace("\"protocolVersion\": 1", "\"protocolVersion\": 2");
    assert_eq!(Frame::decode(&text), Err(DecodeError::VersionUnsupported));
}

#[test]
fn invalid_json_and_seq_bound_are_distinguished() {
    assert_eq!(Frame::decode("{"), Err(DecodeError::InvalidJson));
    let text = fixture_text("attachment/valid/chunk.json").replace(
        "\"connectionSequence\": \"2\"",
        "\"connectionSequence\": \"9223372036854775808\"",
    );
    assert_eq!(Frame::decode(&text), Err(DecodeError::Schema));
}

#[test]
fn chunk_data_is_bounded_to_32k() {
    let too_big = acpr_transcript::encode_base64url(&vec![0_u8; 32769]);
    assert!(too_big.len() > MAX_DATA_CHARS, "测试前提：超出字符上限");
    let text = format!(
        r#"{{"protocolVersion":1,"type":"attachment.chunk","messageId":"7a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d","connectionId":"2de54db7-74ae-4c21-88e3-05df0dc2e637","connectionSequence":"2","body":{{"transferId":"9b2f4c1d-6a3e-4f57-8b21-0c4d5e6f7a89","offset":"0","data":"{too_big}"}}}}"#
    );
    assert_eq!(Frame::decode(&text), Err(DecodeError::Schema));
    assert!(ChunkData::from_bytes(vec![0_u8; 32768]).is_ok());
    assert!(ChunkData::from_bytes(vec![0_u8; 32769]).is_err());
    assert!(ChunkData::from_bytes(Vec::new()).is_err());
}

#[test]
fn subject_kind_is_protocol_specific() {
    // Sync 面只接受 `device`；同一份正例换成 `node` 是跨主体冒用（manifest 的负例已覆盖）。
    let text = fixture_text("attachment/valid/begin.json");
    let frame = Frame::decode(&text).expect("begin");
    let FrameBody::Begin(body) = frame.body else {
        panic!("begin 帧的 body 分类错误");
    };
    assert_eq!(
        body.subject.id.as_str(),
        "7c1a2b3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d"
    );
    let node_subject = text.replace(
        r#"{ "kind": "device", "id": "7c1a2b3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d" }"#,
        r#"{ "kind": "node", "id": "7c1a2b3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d" }"#,
    );
    assert_eq!(Frame::decode(&node_subject), Err(DecodeError::Schema));
}

#[test]
fn frame_sequence_and_ids_are_constructed_explicitly() {
    let frame = Frame::new(
        Uuid::parse("6f1c2d3e-4a5b-4c6d-8e9f-0a1b2c3d4e5f").expect("uuid"),
        Uuid::parse("2de54db7-74ae-4c21-88e3-05df0dc2e637").expect("uuid"),
        DecimalString::parse("1").expect("decimal"),
        FrameBody::Error(sync_protocol::attachment::ErrorBody::new(
            sync_protocol::error::ErrorCode::AttachmentBindingMismatch,
            "the transfer does not belong to this session",
        )),
    );
    let encoded = frame.encode().expect("encode");
    assert_eq!(Frame::decode(&encoded).expect("decode"), frame);
    assert_eq!(frame.frame_type(), FrameType::Error);
}
