//! fixture 驱动：`fixtures/node-link/v1/manifest.json` 的每条 WSS 消息用例都必须按它在合同里的角色通过。
//!
//! 覆盖传输层与全部 29 个消息类型（`handshake`/`catalog`/`resource`/`command`/`error` 五个家族的 body）。
//! 它同时证明两件事：合法消息的 body 字节**逐字节**不被信封层改动；非法消息在正确的层被拒绝。
//! `TypedBody::decode` 的 `match` 是穷尽的——新增消息类型必然在此编译失败，不会静默漏掉。
//!
//! 配对 HTTPS 载荷（`pairing.schema.json`）不是 WSS 消息，由 `tests/pairing_fixtures.rs` 覆盖。

mod support;

use std::collections::HashSet;

use node_link_protocol::catalog::{
    CatalogChanged, CatalogSnapshot, CatalogSubscribe, ExportRevoked, NodeRotateKeyRequest,
    NodeRotateKeyResult, NodeTrustRevoked,
};
use node_link_protocol::command::{
    CommandAccepted, CommandRejected, CommandStatus, CommandSubmit, CommandTerminal,
};
use node_link_protocol::envelope::{
    ConnectionFields, Envelope, EnvelopeError, MessageType, Phase, encode_body,
};
use node_link_protocol::error;
use node_link_protocol::handshake::{NodeChallenge, NodeHello, NodeProof, NodeReady};
use node_link_protocol::resource::{
    Ack, Attach, Attached, Detach, Event, SnapshotBegin, SnapshotChunk, SnapshotEnd, Subscribe,
};

const FIXTURE_ROOT: &str = "fixtures/node-link/v1/";
const MANIFEST: &str = "fixtures/node-link/v1/manifest.json";
const PAIRING_SCHEMA_SUFFIX: &str = "schemas/node-link/v1/pairing.schema.json";

const EXPECTED_VALID_MESSAGE_CASES: usize = 38;
const EXPECTED_ENVELOPE_REJECTED: usize = 1;
const EXPECTED_BODY_REJECTED: usize = 9;
const EXPECTED_SKIPPED_PAIRING: usize = 2;

fn parse<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, String> {
    serde_json::from_str(body).map_err(|error| error.to_string())
}

/// 五个家族的类型化 body。`decode` 的 `match` 穷尽 v1 的 29 个消息类型。
enum TypedBody {
    NodeHello(Box<NodeHello>),
    NodeChallenge(Box<NodeChallenge>),
    NodeProof(Box<NodeProof>),
    NodeReady(Box<NodeReady>),
    CatalogSubscribe(Box<CatalogSubscribe>),
    CatalogSnapshot(Box<CatalogSnapshot>),
    CatalogChanged(Box<CatalogChanged>),
    ExportRevoked(Box<ExportRevoked>),
    NodeTrustRevoked(Box<NodeTrustRevoked>),
    NodeRotateKeyRequest(Box<NodeRotateKeyRequest>),
    NodeRotateKeyResult(Box<NodeRotateKeyResult>),
    ResourceAttach(Box<Attach>),
    ResourceAttached(Box<Attached>),
    ResourceDetach(Box<Detach>),
    ResourceSubscribe(Box<Subscribe>),
    ResourceSnapshotBegin(Box<SnapshotBegin>),
    ResourceSnapshotChunk(Box<SnapshotChunk>),
    ResourceSnapshotEnd(Box<SnapshotEnd>),
    ResourceEvent(Box<Event>),
    ResourceAck(Box<Ack>),
    CommandSubmit(Box<CommandSubmit>),
    CommandAccepted(Box<CommandAccepted>),
    CommandRejected(Box<CommandRejected>),
    CommandTerminal(Box<CommandTerminal>),
    CommandStatus(Box<CommandStatus>),
    LinkError(Box<error::Body>),
    LinkPing(Box<error::Ping>),
    LinkPong(Box<error::Pong>),
    LinkBackpressure(Box<error::Backpressure>),
}

impl TypedBody {
    fn decode(message_type: MessageType, body: &str) -> Result<Self, String> {
        let decoded = match message_type {
            MessageType::NodeHello => TypedBody::NodeHello(Box::new(parse(body)?)),
            MessageType::NodeChallenge => TypedBody::NodeChallenge(Box::new(parse(body)?)),
            MessageType::NodeProof => TypedBody::NodeProof(Box::new(parse(body)?)),
            MessageType::NodeReady => TypedBody::NodeReady(Box::new(parse(body)?)),
            MessageType::CatalogSubscribe => TypedBody::CatalogSubscribe(Box::new(parse(body)?)),
            MessageType::CatalogSnapshot => TypedBody::CatalogSnapshot(Box::new(parse(body)?)),
            MessageType::CatalogChanged => TypedBody::CatalogChanged(Box::new(parse(body)?)),
            MessageType::ExportRevoked => TypedBody::ExportRevoked(Box::new(parse(body)?)),
            MessageType::NodeTrustRevoked => TypedBody::NodeTrustRevoked(Box::new(parse(body)?)),
            MessageType::NodeRotateKeyRequest => {
                TypedBody::NodeRotateKeyRequest(Box::new(parse(body)?))
            }
            MessageType::NodeRotateKeyResult => {
                TypedBody::NodeRotateKeyResult(Box::new(parse(body)?))
            }
            MessageType::ResourceAttach => TypedBody::ResourceAttach(Box::new(parse(body)?)),
            MessageType::ResourceAttached => TypedBody::ResourceAttached(Box::new(parse(body)?)),
            MessageType::ResourceDetach => TypedBody::ResourceDetach(Box::new(parse(body)?)),
            MessageType::ResourceSubscribe => TypedBody::ResourceSubscribe(Box::new(parse(body)?)),
            MessageType::ResourceSnapshotBegin => {
                TypedBody::ResourceSnapshotBegin(Box::new(parse(body)?))
            }
            MessageType::ResourceSnapshotChunk => {
                TypedBody::ResourceSnapshotChunk(Box::new(parse(body)?))
            }
            MessageType::ResourceSnapshotEnd => {
                TypedBody::ResourceSnapshotEnd(Box::new(parse(body)?))
            }
            MessageType::ResourceEvent => TypedBody::ResourceEvent(Box::new(parse(body)?)),
            MessageType::ResourceAck => TypedBody::ResourceAck(Box::new(parse(body)?)),
            MessageType::CommandSubmit => TypedBody::CommandSubmit(Box::new(parse(body)?)),
            MessageType::CommandAccepted => TypedBody::CommandAccepted(Box::new(parse(body)?)),
            MessageType::CommandRejected => TypedBody::CommandRejected(Box::new(parse(body)?)),
            MessageType::CommandTerminal => TypedBody::CommandTerminal(Box::new(parse(body)?)),
            MessageType::CommandStatus => TypedBody::CommandStatus(Box::new(parse(body)?)),
            MessageType::LinkError => TypedBody::LinkError(Box::new(parse(body)?)),
            MessageType::LinkPing => TypedBody::LinkPing(Box::new(parse(body)?)),
            MessageType::LinkPong => TypedBody::LinkPong(Box::new(parse(body)?)),
            MessageType::LinkBackpressure => TypedBody::LinkBackpressure(Box::new(parse(body)?)),
        };
        Ok(decoded)
    }

    fn to_json(&self) -> String {
        match self {
            TypedBody::NodeHello(body) => serde_json::to_string(body),
            TypedBody::NodeChallenge(body) => serde_json::to_string(body),
            TypedBody::NodeProof(body) => serde_json::to_string(body),
            TypedBody::NodeReady(body) => serde_json::to_string(body),
            TypedBody::CatalogSubscribe(body) => serde_json::to_string(body),
            TypedBody::CatalogSnapshot(body) => serde_json::to_string(body),
            TypedBody::CatalogChanged(body) => serde_json::to_string(body),
            TypedBody::ExportRevoked(body) => serde_json::to_string(body),
            TypedBody::NodeTrustRevoked(body) => serde_json::to_string(body),
            TypedBody::NodeRotateKeyRequest(body) => serde_json::to_string(body),
            TypedBody::NodeRotateKeyResult(body) => serde_json::to_string(body),
            TypedBody::ResourceAttach(body) => serde_json::to_string(body),
            TypedBody::ResourceAttached(body) => serde_json::to_string(body),
            TypedBody::ResourceDetach(body) => serde_json::to_string(body),
            TypedBody::ResourceSubscribe(body) => serde_json::to_string(body),
            TypedBody::ResourceSnapshotBegin(body) => serde_json::to_string(body),
            TypedBody::ResourceSnapshotChunk(body) => serde_json::to_string(body),
            TypedBody::ResourceSnapshotEnd(body) => serde_json::to_string(body),
            TypedBody::ResourceEvent(body) => serde_json::to_string(body),
            TypedBody::ResourceAck(body) => serde_json::to_string(body),
            TypedBody::CommandSubmit(body) => serde_json::to_string(body),
            TypedBody::CommandAccepted(body) => serde_json::to_string(body),
            TypedBody::CommandRejected(body) => serde_json::to_string(body),
            TypedBody::CommandTerminal(body) => serde_json::to_string(body),
            TypedBody::CommandStatus(body) => serde_json::to_string(body),
            TypedBody::LinkError(body) => serde_json::to_string(body),
            TypedBody::LinkPing(body) => serde_json::to_string(body),
            TypedBody::LinkPong(body) => serde_json::to_string(body),
            TypedBody::LinkBackpressure(body) => serde_json::to_string(body),
        }
        .expect("已解码的 body 必然可序列化")
    }
}

fn is_wss_message(schema: &str) -> bool {
    schema.ends_with("schemas/node-link/v1/message.schema.json")
}

fn read_case(case: &support::ManifestCase) -> String {
    support::read_text(&support::repo_path(&format!(
        "{FIXTURE_ROOT}{}",
        case.fixture
    )))
}

#[test]
fn every_manifest_case_behaves_as_declared() {
    let cases = support::manifest_cases(MANIFEST);

    let mut valid_message_cases = 0;
    let mut envelope_rejected = 0;
    let mut body_rejected = 0;
    let mut skipped_pairing = 0;

    for case in &cases {
        if !is_wss_message(&case.schema) {
            assert!(
                case.schema.ends_with(PAIRING_SCHEMA_SUFFIX),
                "{}：非 WSS 用例只允许是 pairing 载荷（其余需在本测试内覆盖）",
                case.fixture
            );
            skipped_pairing += 1;
            continue;
        }

        let text = read_case(case);
        let decoded = Envelope::decode(&text);

        match (case.valid, decoded) {
            (true, Ok(envelope)) => {
                valid_message_cases += 1;

                assert_eq!(
                    envelope.body().get(),
                    support::body_slice(&text),
                    "{}：信封层改动了 body 字节",
                    case.fixture
                );

                let typed = TypedBody::decode(envelope.message_type(), envelope.body().get())
                    .unwrap_or_else(|error| {
                        panic!("{}：合法 body 被类型化层拒绝：{error}", case.fixture)
                    });
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&typed.to_json())
                        .expect("重编码结果是 JSON"),
                    serde_json::from_str::<serde_json::Value>(support::body_slice(&text))
                        .expect("原文 body 是 JSON"),
                    "{}：类型化 body 往返改变了字段",
                    case.fixture
                );
            }
            (true, Err(error)) => panic!("{}：合法信封被拒：{error}", case.fixture),
            (false, Err(error)) => {
                let matched = match case.fixture.as_str() {
                    "invalid/link-ping-without-connection.json" => matches!(
                        error,
                        EnvelopeError::ConnectionFieldsRequired {
                            message_type: MessageType::LinkPing
                        }
                    ),
                    other => panic!("{other}：新增的信封层负例必须在此登记期望的错误变体"),
                };
                assert!(
                    matched,
                    "{}：信封层拒绝，但错误变体与期望不符：{error:?}",
                    case.fixture
                );
                envelope_rejected += 1;
            }
            (false, Ok(envelope)) => {
                match TypedBody::decode(envelope.message_type(), envelope.body().get()) {
                    Err(_) => body_rejected += 1,
                    Ok(typed) => panic!("{}：非法 body 被接受：{}", case.fixture, typed.to_json()),
                }
            }
        }
    }

    assert_eq!(
        valid_message_cases, EXPECTED_VALID_MESSAGE_CASES,
        "合法消息用例数变化"
    );
    assert_eq!(
        envelope_rejected, EXPECTED_ENVELOPE_REJECTED,
        "信封层拒绝的负例数变化"
    );
    assert_eq!(
        body_rejected, EXPECTED_BODY_REJECTED,
        "body 层拒绝的负例数变化"
    );
    assert_eq!(skipped_pairing, EXPECTED_SKIPPED_PAIRING, "配对用例数变化");

    println!(
        "node-link envelope: {valid_message_cases} 条合法消息全部保真并类型化往返一致，\
{envelope_rejected} 条被信封层拒绝，{body_rejected} 条被 body 层拒绝，跳过 {skipped_pairing} 条配对载荷"
    );
}

#[test]
fn every_message_type_is_covered_by_a_valid_fixture() {
    let mut covered: HashSet<MessageType> = HashSet::new();

    for case in support::manifest_cases(MANIFEST) {
        if !case.valid || !is_wss_message(&case.schema) {
            continue;
        }
        let text = read_case(&case);
        let envelope = Envelope::decode(&text)
            .unwrap_or_else(|error| panic!("{}：合法信封被拒：{error}", case.fixture));
        TypedBody::decode(envelope.message_type(), envelope.body().get())
            .unwrap_or_else(|error| panic!("{}：类型化 body 失败：{error}", case.fixture));
        covered.insert(envelope.message_type());
    }

    let missing: Vec<&str> = MessageType::ALL
        .iter()
        .filter(|message_type| !covered.contains(message_type))
        .map(|message_type| message_type.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "以下消息类型没有任何合法夹具覆盖：{missing:?}"
    );
}

#[test]
fn envelope_preserves_body_bytes_verbatim() {
    // body 内含空白与超出 u64/i64 的整数：任何"先解析成通用 DTO 再重新序列化"的实现都会改动它。
    let text = "{\"protocolVersion\":1,\"type\":\"link.error\",\"messageId\":\"3a4b5c6d-7e80-4192-a3b4-c5d6e7f8091a\",\"body\": {  \"code\" : \"nodelink.protocol.invalid_json\" , \"message\":\"x\", \"retryable\":false, \"correlationId\":null, \"details\":{\"huge\":123456789012345678901234567890} } }";

    let envelope = Envelope::decode(text).expect("信封合法");
    let expected_body = support::body_slice(text);
    assert_eq!(envelope.body().get(), expected_body);

    let encoded = envelope.encode().expect("可重新编码");
    assert_eq!(
        support::body_slice(&encoded),
        expected_body,
        "重新编码后 body 字节必须完全一致"
    );
    assert!(encoded.contains("123456789012345678901234567890"));

    let body: error::Body =
        serde_json::from_str(envelope.body().get()).expect("link.error body 可类型化");
    assert!(
        body.details
            .get()
            .contains("123456789012345678901234567890")
    );
}

#[test]
fn envelope_rules_are_enforced_on_construction() {
    let message_id =
        node_link_protocol::common::Uuid::parse("ed93263a-3628-4668-82aa-c0f551589fec")
            .expect("uuid");
    let connection = ConnectionFields {
        connection_id: node_link_protocol::common::Uuid::parse(
            "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        )
        .expect("uuid"),
        connection_sequence: node_link_protocol::common::DecimalString::parse("1")
            .expect("decimal"),
    };
    let body = encode_body(&error::Pong {
        nonce: node_link_protocol::common::Base64Url::parse("AAECAwQFBgcICQoLDA0ODw")
            .expect("nonce"),
    })
    .expect("body");

    assert!(
        Envelope::new(
            MessageType::LinkPing,
            message_id.clone(),
            None,
            body.clone()
        )
        .is_err(),
        "link.ping 必须携带连接字段"
    );
    assert!(
        Envelope::new(
            MessageType::NodeHello,
            message_id.clone(),
            Some(connection.clone()),
            body.clone()
        )
        .is_err(),
        "node.hello 不得携带连接字段"
    );
    assert!(
        Envelope::new(
            MessageType::LinkError,
            message_id.clone(),
            None,
            body.clone()
        )
        .is_ok(),
        "link.error 在握手阶段允许省略连接字段"
    );

    let envelope =
        Envelope::new(MessageType::LinkPong, message_id, Some(connection), body).expect("合法信封");
    let reencoded = envelope.encode().expect("可编码");
    assert_eq!(
        Envelope::decode(&reencoded).expect("可解码"),
        envelope,
        "信封往返必须相等"
    );
}

#[test]
fn phase_rules_match_the_connection_state_machine() {
    let pre_auth = "{\"protocolVersion\":1,\"type\":\"link.error\",\"messageId\":\"3a4b5c6d-7e80-4192-a3b4-c5d6e7f8091a\",\"body\":{\"code\":\"nodelink.protocol.invalid_json\",\"message\":\"x\",\"retryable\":false,\"correlationId\":null,\"details\":{}}}";
    let post_auth = "{\"protocolVersion\":1,\"type\":\"link.error\",\"messageId\":\"3a4b5c6d-7e80-4192-a3b4-c5d6e7f8091a\",\"connectionId\":\"2de54db7-74ae-4c21-88e3-05df0dc2e637\",\"connectionSequence\":\"13\",\"body\":{\"code\":\"nodelink.command.not_found\",\"message\":\"x\",\"retryable\":false,\"correlationId\":null,\"details\":{}}}";

    assert!(Envelope::decode_in_phase(pre_auth, Phase::PreAuth).is_ok());
    assert!(Envelope::decode_in_phase(post_auth, Phase::PostAuth).is_ok());
    assert!(
        Envelope::decode_in_phase(pre_auth, Phase::PostAuth).is_err(),
        "认证完成后的 link.error 必须携带连接字段"
    );
    assert!(
        Envelope::decode_in_phase(post_auth, Phase::PreAuth).is_err(),
        "认证完成前的 link.error 不得携带连接字段"
    );

    let pong = "{\"protocolVersion\":1,\"type\":\"link.pong\",\"messageId\":\"1728394a-5c6d-4e7f-88a9-b0c1d2e3f405\",\"connectionId\":\"2de54db7-74ae-4c21-88e3-05df0dc2e637\",\"connectionSequence\":\"50\",\"body\":{\"nonce\":\"AAECAwQFBgcICQoLDA0ODw\"}}";
    assert!(Envelope::decode_in_phase(pong, Phase::PostAuth).is_ok());
    assert!(Envelope::decode_in_phase(pong, Phase::PreAuth).is_err());

    let hello = "{\"protocolVersion\":1,\"type\":\"node.hello\",\"messageId\":\"ed93263a-3628-4668-82aa-c0f551589fec\",\"body\":{}}";
    assert!(Envelope::decode_in_phase(hello, Phase::PreAuth).is_ok());
    assert!(Envelope::decode_in_phase(hello, Phase::PostAuth).is_err());
}

#[test]
fn unknown_message_type_and_unknown_envelope_field_are_rejected() {
    let unknown_type = "{\"protocolVersion\":1,\"type\":\"node.nonexistent\",\"messageId\":\"ed93263a-3628-4668-82aa-c0f551589fec\",\"body\":{}}";
    assert!(Envelope::decode(unknown_type).is_err());

    let unknown_field = "{\"protocolVersion\":1,\"type\":\"link.pong\",\"messageId\":\"1728394a-5c6d-4e7f-88a9-b0c1d2e3f405\",\"connectionId\":\"2de54db7-74ae-4c21-88e3-05df0dc2e637\",\"connectionSequence\":\"50\",\"extra\":1,\"body\":{\"nonce\":\"AAECAwQFBgcICQoLDA0ODw\"}}";
    assert!(
        Envelope::decode(unknown_field).is_err(),
        "未知信封字段必须被拒"
    );

    let wrong_version = "{\"protocolVersion\":2,\"type\":\"link.pong\",\"messageId\":\"1728394a-5c6d-4e7f-88a9-b0c1d2e3f405\",\"connectionId\":\"2de54db7-74ae-4c21-88e3-05df0dc2e637\",\"connectionSequence\":\"50\",\"body\":{\"nonce\":\"AAECAwQFBgcICQoLDA0ODw\"}}";
    assert!(
        Envelope::decode(wrong_version).is_err(),
        "protocolVersion 只能是 1"
    );
}
