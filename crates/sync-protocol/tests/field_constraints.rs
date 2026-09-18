//! 字段级约束的边界用例。
//!
//! `docs/SYNC_PROTOCOL.md` §4.3 要求「ID、sequence、timestamp、枚举和长度限制必须在 wire DTO
//! 边界验证」。本文件把本次三个家族在 schema 里声明的 pattern / minLength / maxLength /
//! minimum / maximum / uniqueItems / enum / required 逐条钉成边界值——只断言"形状对得上"的实现
//! 会在这里失败。base64url 采用 `docs/SYNC_PROTOCOL.md` §6.1 的**规范无填充**口径：schema 的
//! `pattern` + 定长是必要非充分条件。

mod support;

use sync_protocol::auth;
use sync_protocol::common::{Base64Url, DecimalString, Uuid};
use sync_protocol::control;
use sync_protocol::envelope::Envelope;
use sync_protocol::error::{Body as ErrorBody, ErrorCode};

const MESSAGE_ID: &str = "1728394a-5c6d-4e7f-88a9-b0c1d2e3f405";
const CONNECTION_ID: &str = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const HOST_ID: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const DEVICE_ID: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
const SERVER_EPOCH: &str = "00384a03-bc90-4095-b65d-82fb8cc47e13";
const NONCE_16: &str = "AAECAwQFBgcICQoLDA0ODw";
const NONCE_32: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

fn post_auth(message_type: &str, body: &str) -> String {
    format!(
        "{{\"protocolVersion\":1,\"type\":\"{message_type}\",\"messageId\":\"{MESSAGE_ID}\",\"connectionId\":\"{CONNECTION_ID}\",\"connectionSequence\":\"50\",\"body\":{body}}}"
    )
}

fn pre_auth(message_type: &str, body: &str) -> String {
    format!(
        "{{\"protocolVersion\":1,\"type\":\"{message_type}\",\"messageId\":\"{MESSAGE_ID}\",\"body\":{body}}}"
    )
}

/// 解出信封 body 并类型化；任何一层失败都返回错误文本。
fn decode_body<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, String> {
    let envelope = Envelope::decode(text).map_err(|error| error.to_string())?;
    serde_json::from_str(envelope.body().get()).map_err(|error| error.to_string())
}

fn client_hello(client_nonce: &str, supported_features: &str) -> String {
    format!(
        "{{\"minProtocolVersion\":1,\"maxProtocolVersion\":1,\"hostId\":\"{HOST_ID}\",\"deviceId\":\"{DEVICE_ID}\",\"clientKind\":\"pwa\",\"clientNonce\":\"{client_nonce}\",\"supportedFeatures\":{supported_features},\"requiredFeatures\":[]}}"
    )
}

fn authenticated(heartbeat_interval_ms: u64, max_message_bytes: u64, scopes: &str) -> String {
    format!(
        "{{\"deviceId\":\"{DEVICE_ID}\",\"scopes\":{scopes},\"serverEpoch\":\"{SERVER_EPOCH}\",\"headGlobalSequence\":\"2318\",\"heartbeatIntervalMs\":{heartbeat_interval_ms},\"limits\":{{\"maxMessageBytes\":{max_message_bytes},\"maxPromptBytes\":262144,\"maxReplayEventsPerBatch\":500}}}}"
    )
}

fn error_text(code: &str, message: &str, correlation_id: &str, details: &str) -> String {
    format!(
        "{{\"code\":\"{code}\",\"message\":\"{message}\",\"retryable\":false,\"correlationId\":{correlation_id},\"details\":{details}}}"
    )
}

#[test]
fn heartbeat_interval_bounds_are_enforced() {
    for (value, accepted) in [
        (999_u64, false),
        (1000, true),
        (300_000, true),
        (300_001, false),
    ] {
        let text = post_auth("auth.authenticated", &authenticated(value, 1_048_576, "[]"));
        let decoded = decode_body::<auth::Authenticated>(&text);
        assert_eq!(
            decoded.is_ok(),
            accepted,
            "heartbeatIntervalMs={value} 的接受性不符（schema: 1000..=300000）"
        );
    }
}

#[test]
fn limits_minimums_are_enforced() {
    for (value, accepted) in [(1023_u64, false), (1024, true)] {
        let text = post_auth("auth.authenticated", &authenticated(30_000, value, "[]"));
        assert_eq!(
            decode_body::<auth::Authenticated>(&text).is_ok(),
            accepted,
            "maxMessageBytes={value} 的接受性不符（schema: minimum 1024）"
        );
    }

    for value in [0_u64, 1] {
        let body = format!(
            "{{\"deviceId\":\"{DEVICE_ID}\",\"scopes\":[],\"serverEpoch\":\"{SERVER_EPOCH}\",\"headGlobalSequence\":\"0\",\"heartbeatIntervalMs\":30000,\"limits\":{{\"maxMessageBytes\":1048576,\"maxPromptBytes\":{value},\"maxReplayEventsPerBatch\":{value}}}}}"
        );
        let text = post_auth("auth.authenticated", &body);
        assert_eq!(
            decode_body::<auth::Authenticated>(&text).is_ok(),
            value == 1,
            "limits 的 0 值必须被拒（schema: minimum 1）"
        );
    }
}

#[test]
fn protocol_version_minimums_are_enforced() {
    for (minimum, maximum, accepted) in [(0_u64, 1_u64, false), (1, 0, false), (1, 1, true)] {
        let body = format!(
            "{{\"minProtocolVersion\":{minimum},\"maxProtocolVersion\":{maximum},\"hostId\":\"{HOST_ID}\",\"deviceId\":\"{DEVICE_ID}\",\"clientKind\":\"pwa\",\"clientNonce\":\"{NONCE_32}\",\"supportedFeatures\":[],\"requiredFeatures\":[]}}"
        );
        let text = pre_auth("auth.client_hello", &body);
        assert_eq!(
            decode_body::<auth::ClientHello>(&text).is_ok(),
            accepted,
            "min={minimum} max={maximum} 的接受性不符（schema: minimum 1）"
        );
    }
}

#[test]
fn scopes_reject_duplicates_empty_and_overlong_entries() {
    let duplicated = post_auth(
        "auth.authenticated",
        &authenticated(30_000, 1_048_576, "[\"session.read\",\"session.read\"]"),
    );
    assert!(
        decode_body::<auth::Authenticated>(&duplicated).is_err(),
        "scopes 的 uniqueItems 必须被执行"
    );

    let empty = post_auth(
        "auth.authenticated",
        &authenticated(30_000, 1_048_576, "[\"\"]"),
    );
    assert!(
        decode_body::<auth::Authenticated>(&empty).is_err(),
        "scopes 单项 minLength 1 必须被执行"
    );

    let overlong = format!("[\"{}\"]", "s".repeat(129));
    let text = post_auth(
        "auth.authenticated",
        &authenticated(30_000, 1_048_576, &overlong),
    );
    assert!(
        decode_body::<auth::Authenticated>(&text).is_err(),
        "scopes 单项 maxLength 128 必须被执行"
    );
}

#[test]
fn feature_lists_reject_duplicates_overlong_and_bad_ids() {
    let duplicated = pre_auth(
        "auth.client_hello",
        &client_hello(NONCE_32, "[\"core.event-ack.v1\",\"core.event-ack.v1\"]"),
    );
    assert!(
        decode_body::<auth::ClientHello>(&duplicated).is_err(),
        "featureList 的 uniqueItems 必须被执行"
    );

    let overlong: Vec<String> = (0..65).map(|index| format!("\"f{index}\"")).collect();
    let text = pre_auth(
        "auth.client_hello",
        &client_hello(NONCE_32, &format!("[{}]", overlong.join(","))),
    );
    assert!(
        decode_body::<auth::ClientHello>(&text).is_err(),
        "featureList 的 maxItems 64 必须被执行"
    );

    for bad in ["\"Core.Event-ack.v1\"", "\"\"", "\"has space\""] {
        let text = pre_auth(
            "auth.client_hello",
            &client_hello(NONCE_32, &format!("[{bad}]")),
        );
        assert!(
            decode_body::<auth::ClientHello>(&text).is_err(),
            "featureId {bad} 必须被拒（pattern ^[a-z0-9.-]+$）"
        );
    }
}

#[test]
fn error_body_requires_correlation_id_key_and_object_details() {
    let ok = error_text("protocol.invalid_json", "x", "null", "{}");
    assert!(decode_body::<ErrorBody>(&post_auth("error", &ok)).is_ok());
    assert!(decode_body::<ErrorBody>(&pre_auth("error", &ok)).is_ok());

    let with_uuid = error_text(
        "authorization.scope_denied",
        "x",
        &format!("\"{MESSAGE_ID}\""),
        "{}",
    );
    let body = decode_body::<ErrorBody>(&post_auth("error", &with_uuid)).expect("合法 error body");
    assert!(body.correlation_id.as_ref().is_some());
    assert!(!body.correlation_id.is_null());

    // `correlationId` 是 required 且可 null：键缺失必须被拒。
    let missing_key =
        "{\"code\":\"protocol.invalid_json\",\"message\":\"x\",\"retryable\":false,\"details\":{}}";
    let missing_code =
        "{\"message\":\"x\",\"retryable\":false,\"correlationId\":null,\"details\":{}}";
    let missing_details = "{\"code\":\"protocol.invalid_json\",\"message\":\"x\",\"retryable\":false,\"correlationId\":null}";
    let decoded = decode_body::<ErrorBody>(&post_auth("error", missing_key));
    assert!(
        decoded.is_err(),
        "缺 correlationId 键必须被拒，而不是解成 None；实际 {decoded:?}"
    );
    assert!(decode_body::<ErrorBody>(&post_auth("error", missing_code)).is_err());
    assert!(decode_body::<ErrorBody>(&post_auth("error", missing_details)).is_err());

    for details in ["[]", "\"text\"", "3"] {
        let text = error_text("protocol.invalid_json", "x", "null", details);
        assert!(
            decode_body::<ErrorBody>(&post_auth("error", &text)).is_err(),
            "details={details} 不是 object，必须被拒"
        );
    }

    let overlong = error_text("protocol.invalid_json", &"m".repeat(1025), "null", "{}");
    assert!(
        decode_body::<ErrorBody>(&post_auth("error", &overlong)).is_err(),
        "message 的 maxLength 1024 必须被执行"
    );

    let unknown_code = error_text("protocol.nonexistent", "x", "null", "{}");
    assert!(
        decode_body::<ErrorBody>(&post_auth("error", &unknown_code)).is_err(),
        "未登记的错误码必须被拒"
    );

    let bad_retryable = "{\"code\":\"protocol.invalid_json\",\"message\":\"x\",\"retryable\":\"false\",\"correlationId\":null,\"details\":{}}";
    assert!(decode_body::<ErrorBody>(&post_auth("error", bad_retryable)).is_err());
}

#[test]
fn base64url_fields_require_canonical_unpadded_encoding_of_the_declared_width() {
    // 22 字符满足 schema 的 pattern 与定长，但最后一个字符的非零尾部位不是规范编码。
    let non_canonical = "AAECAwQFBgcICQoLDA0ODx";
    assert_eq!(non_canonical.len(), 22);
    assert!(
        Base64Url::<16>::parse(non_canonical).is_err(),
        "非规范 base64url 必须被拒（§6.1：解码后重编码必须等于输入）"
    );
    assert!(Base64Url::<16>::parse(NONCE_16).is_ok());

    let padded = format!("{NONCE_16}=");
    assert!(Base64Url::<16>::parse(&padded).is_err(), "填充必须被拒");

    // 宽度不符：16 字节的文本不能当 32 字节字段用。
    assert!(Base64Url::<32>::parse(NONCE_16).is_err());
    assert!(Base64Url::<32>::parse(NONCE_32).is_ok());

    let text = pre_auth("auth.client_hello", &client_hello(NONCE_16, "[]"));
    assert!(
        decode_body::<auth::ClientHello>(&text).is_err(),
        "clientNonce 必须是 32 字节"
    );

    let non_canonical_nonce = format!(
        "{{\"selectedProtocolVersion\":1,\"hostId\":\"{HOST_ID}\",\"connectionId\":\"{CONNECTION_ID}\",\"serverNonce\":\"{non_canonical}\",\"selectedFeatures\":[],\"hostProof\":\"{}\"}}",
        "A".repeat(86)
    );
    let challenge = pre_auth("auth.server_challenge", &non_canonical_nonce);
    assert!(
        decode_body::<auth::ServerChallenge>(&challenge).is_err(),
        "非规范 serverNonce 必须被拒"
    );
}

#[test]
fn uuid_and_decimal_string_patterns_are_enforced() {
    assert!(
        Uuid::parse("BDB2EC20-F98C-4D87-B789-E540D527EF87").is_err(),
        "uuid 必须是小写"
    );
    assert!(Uuid::parse("urn:uuid:bdb2ec20-f98c-4d87-b789-e540d527ef87").is_err());
    assert!(
        Uuid::parse("bdb2ec20f98c4d87b789e540d527ef870").is_err(),
        "uuid 长度固定"
    );
    assert!(Uuid::parse(HOST_ID).is_ok());

    for bad in ["", "01", "00", "+1", "-1", "1 ", "1.0"] {
        assert!(
            DecimalString::parse(bad).is_err(),
            "decimalString {bad:?} 必须被拒（^ (0|[1-9][0-9]*)$）"
        );
    }
    assert!(DecimalString::parse("0").is_ok());
    assert!(DecimalString::parse("2318").is_ok());

    // 信封的 connectionSequence 是 decimalString：数字字面量必须被拒。
    let numeric = format!(
        "{{\"protocolVersion\":1,\"type\":\"control.pong\",\"messageId\":\"{MESSAGE_ID}\",\"connectionId\":\"{CONNECTION_ID}\",\"connectionSequence\":50,\"body\":{{\"nonce\":\"{NONCE_16}\"}}}}"
    );
    assert!(Envelope::decode(&numeric).is_err());
}

#[test]
fn control_ping_and_pong_bodies_round_trip() {
    for message_type in ["control.ping", "control.pong"] {
        let text = post_auth(message_type, &format!("{{\"nonce\":\"{NONCE_16}\"}}"));
        let envelope = Envelope::decode(&text).expect("信封合法");
        let body =
            serde_json::from_str::<control::Body>(envelope.body().get()).expect("nonce 合法");
        assert_eq!(
            acpr_transcript::encode_base64url(body.nonce.as_bytes()),
            NONCE_16
        );
    }

    let wrong_width = post_auth("control.ping", &format!("{{\"nonce\":\"{NONCE_32}\"}}"));
    assert!(
        decode_body::<control::Body>(&wrong_width).is_err(),
        "control 的 nonce 必须是 16 字节"
    );
}

#[test]
fn error_code_default_retryable_matches_documented_semantics() {
    assert!(ErrorCode::InternalUnavailable.default_retryable());
    assert!(ErrorCode::ResourceRateLimited.default_retryable());
    assert!(!ErrorCode::ProtocolInvalidJson.default_retryable());
    assert!(!ErrorCode::CommandUnsupported.default_retryable());

    let body = ErrorBody::new(ErrorCode::SyncCursorInvalid, "cursor", true).expect("可构造");
    assert!(body.retryable);
    assert_eq!(body.details.get(), "{}");
    assert!(body.correlation_id.is_null());
}
