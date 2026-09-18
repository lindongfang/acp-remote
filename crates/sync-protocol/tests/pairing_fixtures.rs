//! Pairing v1：`fixtures/sync/v1/manifest.json` 的配对夹具 + 合同级边界。
//!
//! 配对是 HTTPS 载荷而不是 WSS 消息，因此不走 `envelope_fixtures` 的信封路径；本文件是它自己的驱动与
//! 判据。每条夹具都声明它对应 `pairing.schema.json` 的哪个 `$defs`——新增夹具必须在此登记，否则失败。

mod support;

use std::collections::BTreeSet;

use sync_protocol::pairing::{
    CanonicalOrigin, ClaimRequest, ClaimResponse, HttpError, PairingStatus, QrPayload,
    StatusRequest, StatusResponse,
};

const FIXTURE_ROOT: &str = "fixtures/sync/v1/";
const MANIFEST: &str = "fixtures/sync/v1/manifest.json";
const PAIRING_SCHEMA_SUFFIX: &str = "schemas/sync/v1/pairing.schema.json";

/// 配对夹具数（`pairing.schema.json` 的六个 `$defs` 中，`httpError` 与连接级 `error` body 同形状，
/// 由 `error_body_uses_the_registered_error_codes` 覆盖，因此没有独立夹具）。
const EXPECTED_PAIRING_CASES: usize = 5;
const EXPECTED_COVERED_DEFS: usize = 5;

fn is_pairing(schema: &str) -> bool {
    schema.ends_with(PAIRING_SCHEMA_SUFFIX)
}

fn fixture_value(relative: &str) -> serde_json::Value {
    let text = support::read_text(&support::repo_path(&format!("{FIXTURE_ROOT}{relative}")));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{relative}：不是合法 JSON：{error}"))
}

fn qr_payload() -> serde_json::Value {
    fixture_value("valid/pairing-qr-payload.json")
}

fn claim_response() -> serde_json::Value {
    fixture_value("valid/pairing-claim-response.json")
}

fn approved_status_response() -> serde_json::Value {
    fixture_value("valid/pairing-status-response.json")
}

/// 夹具 → 它必须解析成功的形状；返回值是 `pairing.schema.json` 的 `$defs` 名。
fn parse_registered(fixture: &str, text: &str) -> Result<&'static str, String> {
    let parsed = |result: Result<(), serde_json::Error>| result.map_err(|error| error.to_string());
    match fixture {
        "valid/pairing-qr-payload.json" => {
            parsed(serde_json::from_str::<QrPayload>(text).map(|_| ())).map(|()| "qrPayload")
        }
        "valid/pairing-claim-request.json" => {
            parsed(serde_json::from_str::<ClaimRequest>(text).map(|_| ())).map(|()| "claimRequest")
        }
        "valid/pairing-claim-response.json" => {
            parsed(serde_json::from_str::<ClaimResponse>(text).map(|_| ()))
                .map(|()| "claimResponse")
        }
        "valid/pairing-status-request.json" => {
            parsed(serde_json::from_str::<StatusRequest>(text).map(|_| ()))
                .map(|()| "statusRequest")
        }
        "valid/pairing-status-response.json" => {
            parsed(serde_json::from_str::<StatusResponse>(text).map(|_| ()))
                .map(|()| "statusResponse")
        }
        other => panic!("{other}：新增的配对夹具必须在本表登记它对应的 $defs"),
    }
}

#[test]
fn every_pairing_fixture_parses_as_its_declared_shape() {
    let mut cases = 0;
    let mut covered: BTreeSet<&'static str> = BTreeSet::new();

    for case in support::manifest_cases(MANIFEST) {
        if !is_pairing(&case.schema) {
            continue;
        }
        assert!(case.valid, "{}：配对夹具目前只有正向样例", case.fixture);
        let text = support::read_text(&support::repo_path(&format!(
            "{FIXTURE_ROOT}{}",
            case.fixture
        )));
        let def = parse_registered(&case.fixture, &text)
            .unwrap_or_else(|error| panic!("{}：解析失败：{error}", case.fixture));
        covered.insert(def);
        cases += 1;
    }

    assert_eq!(cases, EXPECTED_PAIRING_CASES, "配对夹具数变化");
    assert_eq!(covered.len(), EXPECTED_COVERED_DEFS, "覆盖的 $defs 数变化");
    println!("sync pairing: {cases} 条夹具解析成功，覆盖 {EXPECTED_COVERED_DEFS} 个 $defs");
}

#[test]
fn canonical_origin_requires_a_bare_https_authority() {
    assert!(CanonicalOrigin::parse("https://pc.example.test").is_ok());
    assert!(CanonicalOrigin::parse("https://pc.example.test:8443").is_ok());

    for rejected in [
        "http://pc.example.test",
        "https://pc.example.test/",
        "https://pc.example.test/path",
        "https://pc.example.test?x=1",
        "https://pc.example.test#frag",
        "https://",
        "https://pc example.test",
        "https://pc.example.test\u{0}",
        "pc.example.test",
    ] {
        assert!(
            CanonicalOrigin::parse(rejected).is_err(),
            "canonicalOrigin {rejected:?} 必须被拒（schema 的 pattern + 无空白/控制字符）"
        );
    }

    let overlong = format!("https://{}.test", "a".repeat(2048));
    assert!(
        CanonicalOrigin::parse(&overlong).is_err(),
        "canonicalOrigin 的 maxLength 2048 必须被执行"
    );

    // 参与身份绑定，因此必须逐字保留大小写与端口。
    let origin = CanonicalOrigin::parse("https://PC.Example.Test:8443").expect("合法");
    assert_eq!(origin.as_str(), "https://PC.Example.Test:8443");
}

#[test]
fn qr_payload_constants_and_widths_are_enforced() {
    let mut payload = qr_payload();
    assert!(serde_json::from_value::<QrPayload>(payload.clone()).is_ok());

    payload["pairingProtocol"] = serde_json::json!("acp-remote-pairing-v2");
    assert!(
        serde_json::from_value::<QrPayload>(payload.clone()).is_err(),
        "pairingProtocol 只接受 const 取值"
    );

    // hostPublicKey 是 65 字节的 SEC1 公钥：32 字节的 nonce 不能顶替。
    let mut payload = qr_payload();
    payload["hostPublicKey"] = serde_json::json!("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8");
    assert!(
        serde_json::from_value::<QrPayload>(payload).is_err(),
        "hostPublicKey 必须是 65 字节"
    );

    let mut payload = qr_payload();
    payload["futureField"] = serde_json::json!(1);
    assert!(
        serde_json::from_value::<QrPayload>(payload).is_err(),
        "配对载荷是关闭形状：未知字段必须被拒"
    );
}

#[test]
fn claim_response_status_is_pinned_to_pending_confirmation() {
    let mut response = claim_response();
    assert!(serde_json::from_value::<ClaimResponse>(response.clone()).is_ok());

    response["status"] = serde_json::json!("approved");
    assert!(
        serde_json::from_value::<ClaimResponse>(response.clone()).is_err(),
        "claimResponse.status 只有 pending_confirmation 一个取值"
    );

    let mut response = claim_response();
    response["hostProof"] = serde_json::json!("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8");
    assert!(
        serde_json::from_value::<ClaimResponse>(response).is_err(),
        "hostProof 必须是 64 字节"
    );
}

#[test]
fn status_response_device_and_host_follow_the_status() {
    let parsed: StatusResponse =
        serde_json::from_value(approved_status_response()).expect("approved");
    assert_eq!(parsed.status, PairingStatus::Approved);
    assert!(parsed.device.is_some() && parsed.host.is_some());

    let mut without_host = approved_status_response();
    without_host.as_object_mut().expect("object").remove("host");
    assert!(
        serde_json::from_value::<StatusResponse>(without_host).is_err(),
        "approved 必须同时给出 device 与 host"
    );

    let mut without_device = approved_status_response();
    without_device
        .as_object_mut()
        .expect("object")
        .remove("device");
    assert!(
        serde_json::from_value::<StatusResponse>(without_device).is_err(),
        "approved 必须同时给出 device 与 host"
    );

    let mut pending = approved_status_response();
    pending["status"] = serde_json::json!("pending_confirmation");
    assert!(
        serde_json::from_value::<StatusResponse>(pending).is_err(),
        "非 approved 状态不得携带 device/host（schema 的 if/then/else 的 else 分支）"
    );

    let mut null_device = approved_status_response();
    null_device["status"] = serde_json::json!("expired");
    null_device["device"] = serde_json::Value::Null;
    assert!(
        serde_json::from_value::<StatusResponse>(null_device).is_err(),
        "device 可选但没有 null 分支：显式 null 必须被拒"
    );

    let mut duplicated_scope = approved_status_response();
    duplicated_scope["device"]["scopes"] = serde_json::json!(["session.read", "session.read"]);
    assert!(
        serde_json::from_value::<StatusResponse>(duplicated_scope).is_err(),
        "scopes 的 uniqueItems 必须被执行"
    );

    let mut empty_scope = approved_status_response();
    empty_scope["device"]["scopes"] = serde_json::json!([""]);
    assert!(
        serde_json::from_value::<StatusResponse>(empty_scope).is_err(),
        "scopes 单项 minLength 1 必须被执行"
    );
}

#[test]
fn http_error_reuses_the_registered_error_codes() {
    let ok = "{\"code\":\"pairing.expired\",\"message\":\"x\",\"retryable\":false,\"correlationId\":null,\"details\":{}}";
    let parsed: HttpError = serde_json::from_str(ok).expect("配对 HTTP 错误体与 error body 同形状");
    assert!(parsed.correlation_id.is_null());
    assert_eq!(parsed.details.get(), "{}");

    let unknown_code = ok.replace("pairing.expired", "pairing.nonexistent");
    assert!(
        serde_json::from_str::<HttpError>(&unknown_code).is_err(),
        "未登记的错误码必须被拒（与 WSS error body 共用同一枚举）"
    );

    let missing_key =
        "{\"code\":\"pairing.expired\",\"message\":\"x\",\"retryable\":false,\"details\":{}}";
    assert!(
        serde_json::from_str::<HttpError>(missing_key).is_err(),
        "correlationId 是 required（可 null）：键缺失必须被拒"
    );
}
