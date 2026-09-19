//! fixture 驱动：`fixtures/sync/v1/manifest.json` 的每条 WSS 消息用例都必须按它在合同里的角色通过。
//!
//! 覆盖传输层与全部 18 个消息类型（`auth`/`sync`/`control`/`error`/`event`/`command` 六个家族的 body）。
//! 它同时证明两件事：合法消息的 body 字节**逐字节**不被信封层改动；非法消息在正确的层被拒绝（信封层或
//! body 层）。`TypedBody::decode` 的 `match` 是穷尽的——新增消息类型必然在此编译失败，不会静默漏掉。

mod support;

use std::collections::HashSet;

use sync_protocol::auth;
use sync_protocol::command;
use sync_protocol::common::{Base64Url, ProtocolVersionV1, Uuid};
use sync_protocol::control;
use sync_protocol::envelope::{
    ConnectionFields, Envelope, EnvelopeError, MessageType, Phase, encode_body,
};
use sync_protocol::error;
use sync_protocol::event;
use sync_protocol::sync;

const FIXTURE_ROOT: &str = "fixtures/sync/v1/";
const MANIFEST: &str = "fixtures/sync/v1/manifest.json";

const EXPECTED_VALID_MESSAGE_CASES: usize = 60;
const EXPECTED_ENVELOPE_REJECTED: usize = 2;
const EXPECTED_BODY_REJECTED: usize = 5;
/// 非 WSS 的 HTTPS 载荷（配对）用例数；它们由 `tests/pairing_fixtures.rs` 覆盖。
const EXPECTED_SKIPPED_PAIRING: usize = 5;
const PAIRING_SCHEMA_SUFFIX: &str = "schemas/sync/v1/pairing.schema.json";

fn parse<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, String> {
    serde_json::from_str(body).map_err(|error| error.to_string())
}

/// 六个家族的类型化 body。`decode` 的 match 穷尽 v1 的 18 个消息类型。
enum TypedBody {
    ClientHello(Box<auth::ClientHello>),
    ServerChallenge(Box<auth::ServerChallenge>),
    ClientProof(Box<auth::ClientProof>),
    Authenticated(Box<auth::Authenticated>),
    Subscribe(Box<sync::Subscribe>),
    CaughtUp(Box<sync::CaughtUp>),
    ResetRequired(Box<sync::ResetRequired>),
    SnapshotRequest(Box<sync::SnapshotRequest>),
    SnapshotBegin(Box<sync::SnapshotBegin>),
    SnapshotChunk(Box<sync::SnapshotChunk>),
    SnapshotEnd(Box<sync::SnapshotEnd>),
    Ack(Box<sync::Ack>),
    Control(control::Body),
    Error(Box<error::Body>),
    Event(Box<event::Body>),
    Command(Box<command::Command>),
    CommandResult(Box<command::CommandResult>),
}

impl TypedBody {
    fn decode(message_type: MessageType, body: &str) -> Result<Self, String> {
        let decoded = match message_type {
            MessageType::AuthClientHello => TypedBody::ClientHello(Box::new(parse(body)?)),
            MessageType::AuthServerChallenge => TypedBody::ServerChallenge(Box::new(parse(body)?)),
            MessageType::AuthClientProof => TypedBody::ClientProof(Box::new(parse(body)?)),
            MessageType::AuthAuthenticated => TypedBody::Authenticated(Box::new(parse(body)?)),
            MessageType::SyncSubscribe => TypedBody::Subscribe(Box::new(parse(body)?)),
            MessageType::SyncCaughtUp => TypedBody::CaughtUp(Box::new(parse(body)?)),
            MessageType::SyncResetRequired => TypedBody::ResetRequired(Box::new(parse(body)?)),
            MessageType::SyncSnapshotRequest => TypedBody::SnapshotRequest(Box::new(parse(body)?)),
            MessageType::SyncSnapshotBegin => TypedBody::SnapshotBegin(Box::new(parse(body)?)),
            MessageType::SyncSnapshotChunk => TypedBody::SnapshotChunk(Box::new(parse(body)?)),
            MessageType::SyncSnapshotEnd => TypedBody::SnapshotEnd(Box::new(parse(body)?)),
            MessageType::SyncAck => TypedBody::Ack(Box::new(parse(body)?)),
            MessageType::ControlPing | MessageType::ControlPong => TypedBody::Control(parse(body)?),
            MessageType::Error => TypedBody::Error(Box::new(parse(body)?)),
            MessageType::Event => TypedBody::Event(Box::new(parse(body)?)),
            MessageType::Command => TypedBody::Command(Box::new(parse(body)?)),
            MessageType::CommandResult => TypedBody::CommandResult(Box::new(parse(body)?)),
        };
        Ok(decoded)
    }

    fn to_json(&self) -> String {
        match self {
            TypedBody::ClientHello(body) => serde_json::to_string(body),
            TypedBody::ServerChallenge(body) => serde_json::to_string(body),
            TypedBody::ClientProof(body) => serde_json::to_string(body),
            TypedBody::Authenticated(body) => serde_json::to_string(body),
            TypedBody::Subscribe(body) => serde_json::to_string(body),
            TypedBody::CaughtUp(body) => serde_json::to_string(body),
            TypedBody::ResetRequired(body) => serde_json::to_string(body),
            TypedBody::SnapshotRequest(body) => serde_json::to_string(body),
            TypedBody::SnapshotBegin(body) => serde_json::to_string(body),
            TypedBody::SnapshotChunk(body) => serde_json::to_string(body),
            TypedBody::SnapshotEnd(body) => serde_json::to_string(body),
            TypedBody::Ack(body) => serde_json::to_string(body),
            TypedBody::Control(body) => serde_json::to_string(body),
            TypedBody::Error(body) => serde_json::to_string(body),
            TypedBody::Event(body) => serde_json::to_string(body),
            TypedBody::Command(body) => serde_json::to_string(body),
            TypedBody::CommandResult(body) => serde_json::to_string(body),
        }
        .expect("已解码的 body 必然可序列化")
    }
}

fn is_wss_message(schema: &str) -> bool {
    schema.ends_with("schemas/sync/v1/message.schema.json")
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

                // 信封层保真：body 切片逐字节等于原文。
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
                let reserialized = typed.to_json();
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&reserialized)
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
                    "invalid/ping-without-connection.json" => matches!(
                        error,
                        EnvelopeError::ConnectionFieldsRequired {
                            message_type: MessageType::ControlPing
                        }
                    ),
                    "invalid/sequence-is-number.json" => {
                        matches!(error, EnvelopeError::Malformed(_))
                    }
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
                // 信封合法即非法点在 body：每条负例都必须被类型化层拒绝，且不得被接受。
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
    assert_eq!(
        skipped_pairing, EXPECTED_SKIPPED_PAIRING,
        "非 WSS 用例数变化"
    );

    println!(
        "sync envelope: {valid_message_cases} 条合法消息全部保真并类型化往返一致，\
{envelope_rejected} 条被信封层拒绝，{body_rejected} 条被 body 层拒绝，跳过 {skipped_pairing} 条非 WSS 用例"
    );
}

#[test]
fn every_message_type_is_covered_by_a_valid_fixture() {
    // 18 个类型都必须有至少一条合法夹具真正走通"信封 + 类型化 body"，而不是只出现在分派表里。
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
    assert_eq!(covered.len(), 18, "覆盖到的消息类型数变化");
}

#[test]
fn envelope_preserves_body_bytes_verbatim() {
    // body 内部含空白、超大整数（超出 u64/i64）与嵌套结构：任何"先解析成通用 DTO 再重新序列化"
    // 的实现都会改动它（空白丢失、大整数变浮点）。
    let text = "{\"protocolVersion\":1,\"type\":\"error\",\"messageId\":\"3a4b5c6d-7e80-4192-a3b4-c5d6e7f8091a\",\"body\": {  \"code\" : \"protocol.invalid_json\" , \"message\":\"x\", \"retryable\":false, \"correlationId\":null, \"details\":{\"huge\":123456789012345678901234567890,\"nested\":[1,2,{\"a\":true}]} } }";

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
        serde_json::from_str(envelope.body().get()).expect("error body 可类型化");
    assert!(
        body.details
            .get()
            .contains("123456789012345678901234567890")
    );
}

#[test]
fn envelope_rules_are_enforced_on_construction() {
    let message_id = Uuid::parse("ed93263a-3628-4668-82aa-c0f551589fec").expect("uuid");
    let connection = ConnectionFields {
        connection_id: Uuid::parse("2de54db7-74ae-4c21-88e3-05df0dc2e637").expect("uuid"),
        connection_sequence: sync_protocol::common::DecimalString::parse("1").expect("decimal"),
    };
    let body = encode_body(&control::Body {
        nonce: Base64Url::parse("AAECAwQFBgcICQoLDA0ODw").expect("nonce"),
    })
    .expect("body");

    assert!(
        Envelope::new(
            MessageType::ControlPing,
            message_id.clone(),
            None,
            body.clone()
        )
        .is_err(),
        "control.ping 必须携带连接字段"
    );
    assert!(
        Envelope::new(
            MessageType::AuthClientHello,
            message_id.clone(),
            Some(connection.clone()),
            body.clone()
        )
        .is_err(),
        "auth.client_hello 不得携带连接字段"
    );
    assert!(
        Envelope::new(MessageType::Error, message_id.clone(), None, body.clone()).is_ok(),
        "error 允许认证前的信封"
    );

    let envelope = Envelope::new(MessageType::ControlPong, message_id, Some(connection), body)
        .expect("合法信封");
    let reencoded = envelope.encode().expect("可编码");
    let round_tripped = Envelope::decode(&reencoded).expect("可解码");
    assert_eq!(round_tripped, envelope);
    assert_eq!(
        round_tripped
            .connection()
            .expect("post-auth")
            .connection_sequence
            .as_str(),
        "1"
    );
    assert_eq!(ProtocolVersionV1::new(1).expect("1 合法").get(), 1);
}

#[test]
fn connection_fields_must_appear_together() {
    let text = "{\"protocolVersion\":1,\"type\":\"control.pong\",\"messageId\":\"1728394a-5c6d-4e7f-88a9-b0c1d2e3f405\",\"connectionId\":\"2de54db7-74ae-4c21-88e3-05df0dc2e637\",\"body\":{\"nonce\":\"AAECAwQFBgcICQoLDA0ODw\"}}";
    assert!(
        Envelope::decode(text).is_err(),
        "缺 connectionSequence 必须被拒"
    );
}

#[test]
fn unknown_message_type_and_unknown_envelope_field_are_rejected() {
    let unknown_type = "{\"protocolVersion\":1,\"type\":\"auth.nonexistent\",\"messageId\":\"ed93263a-3628-4668-82aa-c0f551589fec\",\"body\":{}}";
    assert!(Envelope::decode(unknown_type).is_err());

    let unknown_field = "{\"protocolVersion\":1,\"type\":\"control.pong\",\"messageId\":\"1728394a-5c6d-4e7f-88a9-b0c1d2e3f405\",\"connectionId\":\"2de54db7-74ae-4c21-88e3-05df0dc2e637\",\"connectionSequence\":\"50\",\"extra\":1,\"body\":{\"nonce\":\"AAECAwQFBgcICQoLDA0ODw\"}}";
    assert!(
        Envelope::decode(unknown_field).is_err(),
        "未知信封字段必须被拒"
    );

    let wrong_version = "{\"protocolVersion\":2,\"type\":\"control.pong\",\"messageId\":\"1728394a-5c6d-4e7f-88a9-b0c1d2e3f405\",\"connectionId\":\"2de54db7-74ae-4c21-88e3-05df0dc2e637\",\"connectionSequence\":\"50\",\"body\":{\"nonce\":\"AAECAwQFBgcICQoLDA0ODw\"}}";
    assert!(
        Envelope::decode(wrong_version).is_err(),
        "protocolVersion 只能是 1"
    );
}

#[test]
fn phase_rules_match_the_connection_state_machine() {
    let pre_auth = "{\"protocolVersion\":1,\"type\":\"error\",\"messageId\":\"3a4b5c6d-7e80-4192-a3b4-c5d6e7f8091a\",\"body\":{\"code\":\"protocol.invalid_json\",\"message\":\"x\",\"retryable\":false,\"correlationId\":null,\"details\":{}}}";
    let post_auth = "{\"protocolVersion\":1,\"type\":\"error\",\"messageId\":\"3a4b5c6d-7e80-4192-a3b4-c5d6e7f8091a\",\"connectionId\":\"2de54db7-74ae-4c21-88e3-05df0dc2e637\",\"connectionSequence\":\"13\",\"body\":{\"code\":\"authorization.scope_denied\",\"message\":\"x\",\"retryable\":false,\"correlationId\":null,\"details\":{}}}";

    // `error` 是唯一两种形状都合法的类型，阶段由连接状态机给出。
    assert!(Envelope::decode_in_phase(pre_auth, Phase::PreAuth).is_ok());
    assert!(Envelope::decode_in_phase(post_auth, Phase::PostAuth).is_ok());
    assert!(
        Envelope::decode_in_phase(pre_auth, Phase::PostAuth).is_err(),
        "认证完成后的 error 必须携带连接字段"
    );
    assert!(
        Envelope::decode_in_phase(post_auth, Phase::PreAuth).is_err(),
        "认证完成前的 error 不得携带连接字段"
    );

    // 业务消息的阶段是固定的。
    let pong = "{\"protocolVersion\":1,\"type\":\"control.pong\",\"messageId\":\"1728394a-5c6d-4e7f-88a9-b0c1d2e3f405\",\"connectionId\":\"2de54db7-74ae-4c21-88e3-05df0dc2e637\",\"connectionSequence\":\"50\",\"body\":{\"nonce\":\"AAECAwQFBgcICQoLDA0ODw\"}}";
    assert!(Envelope::decode_in_phase(pong, Phase::PostAuth).is_ok());
    assert!(Envelope::decode_in_phase(pong, Phase::PreAuth).is_err());

    let hello = "{\"protocolVersion\":1,\"type\":\"auth.client_hello\",\"messageId\":\"ed93263a-3628-4668-82aa-c0f551589fec\",\"body\":{}}";
    assert!(Envelope::decode_in_phase(hello, Phase::PreAuth).is_ok());
    assert!(Envelope::decode_in_phase(hello, Phase::PostAuth).is_err());
}
