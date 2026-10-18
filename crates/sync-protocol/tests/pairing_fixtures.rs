//! Pairing v1：`fixtures/sync/v1/manifest.json` 的配对夹具 + 合同级边界。
//!
//! 配对是 HTTPS 载荷而不是 WSS 消息，因此不走 `envelope_fixtures` 的信封路径；本文件是它自己的驱动与
//! 判据。每条合法夹具都声明它对应 `pairing.schema.json` 的哪个 `$defs`、每条非法夹具都声明被哪个
//! DTO 拒绝——新增夹具必须在此登记，否则失败。

mod support;

use std::collections::BTreeSet;

use sync_protocol::pairing::{
    CanonicalOrigin, ClaimRequest, ClaimResponse, HttpError, LookupRequest, LookupResponse,
    PairingCode, PairingStatus, QrLookupRequest, QrLookupResponse, StatusRequest, StatusResponse,
};

const FIXTURE_ROOT: &str = "fixtures/sync/v1/";
const MANIFEST: &str = "fixtures/sync/v1/manifest.json";
const PAIRING_SCHEMA_SUFFIX: &str = "schemas/sync/v1/pairing.schema.json";

/// 配对夹具数：`pairing.schema.json` 的 `$defs` 中，`canonicalOrigin` 是被引用的取值域而不是
/// 顶层 payload。lookup 与 qr-lookup 的 DTO 各有正例与负例，`httpError` 由统一的
/// `401 pairing.qr_invalid` 失败夹具覆盖。
const EXPECTED_VALID_PAIRING_CASES: usize = 9;
const EXPECTED_INVALID_PAIRING_CASES: usize = 9;
const EXPECTED_COVERED_DEFS: usize = 9;

fn is_pairing(schema: &str) -> bool {
    schema.ends_with(PAIRING_SCHEMA_SUFFIX)
}

fn fixture_value(relative: &str) -> serde_json::Value {
    let text = support::read_text(&support::repo_path(&format!("{FIXTURE_ROOT}{relative}")));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{relative}：不是合法 JSON：{error}"))
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
        "valid/pairing-qr-lookup-request.json" => {
            parsed(serde_json::from_str::<QrLookupRequest>(text).map(|_| ()))
                .map(|()| "qrLookupRequest")
        }
        "valid/pairing-qr-lookup-response.json" => {
            parsed(serde_json::from_str::<QrLookupResponse>(text).map(|_| ()))
                .map(|()| "qrLookupResponse")
        }
        "valid/pairing-qr-lookup-error.json" => {
            parsed(serde_json::from_str::<HttpError>(text).map(|_| ())).map(|()| "httpError")
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
        "valid/pairing-lookup-request.json" => {
            parsed(serde_json::from_str::<LookupRequest>(text).map(|_| ()))
                .map(|()| "lookupRequest")
        }
        "valid/pairing-lookup-response.json" => {
            parsed(serde_json::from_str::<LookupResponse>(text).map(|_| ()))
                .map(|()| "lookupResponse")
        }
        other => panic!("{other}：新增的配对夹具必须在本表登记它对应的 $defs"),
    }
}

/// 非法夹具 → 它们必须被哪个 Rust 形状拒绝（DTO 与 schema 逐条对齐，负例不能只是 JSON 语法错）。
fn rejects_registered(fixture: &str, text: &str) -> bool {
    match fixture {
        "invalid/pairing-lookup-request-missing-code.json"
        | "invalid/pairing-lookup-request-short-code.json"
        | "invalid/pairing-lookup-request-invalid-character.json"
        | "invalid/pairing-lookup-request-unknown-field.json" => {
            serde_json::from_str::<LookupRequest>(text).is_err()
        }
        "invalid/pairing-lookup-response-missing-secret.json" => {
            serde_json::from_str::<LookupResponse>(text).is_err()
        }
        "invalid/pairing-lookup-error-unknown-code.json" => {
            serde_json::from_str::<HttpError>(text).is_err()
        }
        "invalid/pairing-qr-lookup-request-short-token.json"
        | "invalid/pairing-qr-lookup-request-unknown-field.json" => {
            serde_json::from_str::<QrLookupRequest>(text).is_err()
        }
        "invalid/pairing-qr-lookup-response-missing-secret.json" => {
            serde_json::from_str::<QrLookupResponse>(text).is_err()
        }
        other => panic!("{other}：新增的配对负例必须在本表登记它被哪个 DTO 拒绝"),
    }
}

#[test]
fn every_pairing_fixture_parses_as_its_declared_shape() {
    let mut valid_cases = 0;
    let mut invalid_cases = 0;
    let mut covered: BTreeSet<&'static str> = BTreeSet::new();

    for case in support::manifest_cases(MANIFEST) {
        if !is_pairing(&case.schema) {
            continue;
        }
        let text = support::read_text(&support::repo_path(&format!(
            "{FIXTURE_ROOT}{}",
            case.fixture
        )));
        if case.valid {
            let def = parse_registered(&case.fixture, &text)
                .unwrap_or_else(|error| panic!("{}：解析失败：{error}", case.fixture));
            covered.insert(def);
            valid_cases += 1;
        } else {
            assert!(
                rejects_registered(&case.fixture, &text),
                "{}：非法夹具被对应的 DTO 接受",
                case.fixture
            );
            invalid_cases += 1;
        }
    }

    assert_eq!(
        valid_cases, EXPECTED_VALID_PAIRING_CASES,
        "合法配对夹具数变化"
    );
    assert_eq!(
        invalid_cases, EXPECTED_INVALID_PAIRING_CASES,
        "非法配对夹具数变化"
    );
    assert_eq!(covered.len(), EXPECTED_COVERED_DEFS, "覆盖的 $defs 数变化");
    println!(
        "sync pairing: {valid_cases} 条合法夹具解析成功，{invalid_cases} 条非法夹具被拒，\
覆盖 {EXPECTED_COVERED_DEFS} 个 $defs"
    );
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
fn qr_lookup_request_round_trips_and_rejects_unknown_keys() {
    let fixture = fixture_value("valid/pairing-qr-lookup-request.json");
    let parsed: QrLookupRequest =
        serde_json::from_value(fixture.clone()).expect("qr-lookup 请求夹具必须可解析");
    assert_eq!(
        parsed.qr_token.as_bytes().len(),
        32,
        "qrToken 必须是 32 字节扫码取回凭据"
    );
    assert_eq!(
        serde_json::to_value(&parsed).expect("必须可序列化"),
        fixture,
        "QrLookupRequest 的序列化形状必须与夹具逐字一致"
    );

    let mut wrong_version = fixture.clone();
    wrong_version["protocolVersion"] = serde_json::json!(2);
    assert!(
        serde_json::from_value::<QrLookupRequest>(wrong_version).is_err(),
        "protocolVersion 只接受常量 1"
    );

    let mut short_token = fixture.clone();
    short_token["qrToken"] = serde_json::json!("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh");
    assert!(
        serde_json::from_value::<QrLookupRequest>(short_token).is_err(),
        "qrToken 必须是 32 字节（43 个 base64url 字符）"
    );

    let mut unknown_field = fixture;
    unknown_field["pairingCode"] = serde_json::json!("ABCD-EFGH");
    assert!(
        serde_json::from_value::<QrLookupRequest>(unknown_field).is_err(),
        "qr-lookup 请求是关闭形状：旧入口的 pairingCode 也必须被拒"
    );
}

#[test]
fn qr_lookup_response_has_the_same_binding_as_code_lookup() {
    let qr: QrLookupResponse =
        serde_json::from_value(fixture_value("valid/pairing-qr-lookup-response.json"))
            .expect("qr-lookup 响应夹具必须可解析");
    let code: LookupResponse =
        serde_json::from_value(fixture_value("valid/pairing-lookup-response.json"))
            .expect("lookup 响应夹具必须可解析");

    assert_eq!(qr, code, "两个入口交付同一份七字段配对资料");
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

/// 序列化往返：夹具 → `StatusResponse` → 再序列化，键名与值都必须与夹具逐字一致。
///
/// 本条补的是一个**真实漏检**：本文件的既有用例只做反序列化（走 `RawStatusResponse` 那侧的
/// `#[serde(rename)]`），因此 `StatusResponse` 的序列化侧曾长期漏掉 `rename`——它把
/// `protocolVersion` / `pairingRequestId` / `expiresAt` 写成 snake_case，而
/// `schemas/sync/v1/pairing.schema.json` 的 `statusResponse.required` 正是那三个 camelCase 名。
/// 缺了本条，两侧可以长期不一致而无人发现（配对状态响应是 HTTPS 载荷，不走信封夹具那条路）。
#[test]
fn status_response_round_trips_through_the_wire_shape() {
    let fixture = approved_status_response();
    let parsed: StatusResponse =
        serde_json::from_value(fixture.clone()).expect("approved 夹具必须可解析");
    let serialized = serde_json::to_value(&parsed).expect("必须可序列化");
    assert_eq!(
        serialized, fixture,
        "StatusResponse 的序列化形状必须与夹具逐字一致（wire 字段名是 camelCase）"
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

    // 设备方向没有授权维度（`pairing-model` 删除设备权限层）：`device` 块不再有 `scopes`，
    // 它是未知字段，必须被拒（`pairing.schema.json` 的 `device.additionalProperties: false`）。
    let mut with_scope = approved_status_response();
    with_scope["device"]["scopes"] = serde_json::json!(["session.read"]);
    assert!(
        serde_json::from_value::<StatusResponse>(with_scope).is_err(),
        "device 块携带 scopes 必须被拒（设备权限层已删除）"
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

    let lookup_failure = ok.replace("pairing.expired", "pairing.code_invalid");
    assert!(
        serde_json::from_str::<HttpError>(&lookup_failure).is_ok(),
        "lookup 的统一失败码 pairing.code_invalid 必须已登记"
    );

    let qr_lookup_failure = ok.replace("pairing.expired", "pairing.qr_invalid");
    assert!(
        serde_json::from_str::<HttpError>(&qr_lookup_failure).is_ok(),
        "qr-lookup 的统一失败码 pairing.qr_invalid 必须已登记"
    );

    let missing_key =
        "{\"code\":\"pairing.expired\",\"message\":\"x\",\"retryable\":false,\"details\":{}}";
    assert!(
        serde_json::from_str::<HttpError>(missing_key).is_err(),
        "correlationId 是 required（可 null）：键缺失必须被拒"
    );
}

#[test]
fn lookup_request_round_trips_and_enforces_the_code_shape() {
    let fixture = fixture_value("valid/pairing-lookup-request.json");
    let parsed: LookupRequest =
        serde_json::from_value(fixture.clone()).expect("lookup 请求夹具必须可解析");
    assert_eq!(parsed.pairing_code.as_str(), "ABCD-EFGH");
    assert_eq!(
        serde_json::to_value(&parsed).expect("必须可序列化"),
        fixture,
        "LookupRequest 的序列化形状必须与夹具逐字一致"
    );

    for accepted in [
        "ABCD-EFGH",
        "abcdefgh",
        "ABC1EFG0",
        "ABCD-EFGI",
        "ABCD-EFGL",
        "ABCD-EFGO",
    ] {
        assert!(
            PairingCode::parse(accepted).is_ok(),
            "配对码 {accepted:?} 属于 wire 允许的字符集（大小写不敏感，允许一个分隔符）"
        );
    }

    for rejected in [
        "",
        "ABCD-EFG",
        "ABCD-EFGHI",
        "ABCD-EFGU",
        "ABCD--EFG",
        "ABCDE-FGH",
        "ABC-EFGH",
        "ABCD EFGH",
        "ABCD_EFGH",
        "ABCD-EFGH\n",
    ] {
        assert!(
            PairingCode::parse(rejected).is_err(),
            "配对码 {rejected:?} 必须被拒（8 位、可带一个 4+4 分隔符、不含 U）"
        );
    }

    let mut wrong_version = fixture.clone();
    wrong_version["protocolVersion"] = serde_json::json!(2);
    assert!(
        serde_json::from_value::<LookupRequest>(wrong_version).is_err(),
        "protocolVersion 只接受常量 1"
    );

    let mut unknown_field = fixture.clone();
    unknown_field["hostId"] = serde_json::json!("bdb2ec20-f98c-4d87-b789-e540d527ef87");
    assert!(
        serde_json::from_value::<LookupRequest>(unknown_field).is_err(),
        "lookup 请求是关闭形状：未知字段必须被拒"
    );
}

#[test]
fn lookup_response_round_trips_through_the_wire_shape() {
    let fixture = fixture_value("valid/pairing-lookup-response.json");
    let parsed: LookupResponse =
        serde_json::from_value(fixture.clone()).expect("lookup 响应夹具必须可解析");
    assert_eq!(
        serde_json::to_value(&parsed).expect("必须可序列化"),
        fixture,
        "LookupResponse 的序列化形状必须与夹具逐字一致（wire 字段名是 camelCase）"
    );

    let mut short_key = fixture.clone();
    short_key["hostPublicKey"] = fixture["pairingSecret"].clone();
    assert!(
        serde_json::from_value::<LookupResponse>(short_key).is_err(),
        "hostPublicKey 必须是 65 字节：32 字节的 pairingSecret 不能顶替"
    );

    let mut long_secret = fixture.clone();
    long_secret["pairingSecret"] = fixture["hostPublicKey"].clone();
    assert!(
        serde_json::from_value::<LookupResponse>(long_secret).is_err(),
        "pairingSecret 必须是 32 字节"
    );

    let mut plain_http = fixture.clone();
    plain_http["canonicalOrigin"] = serde_json::json!("http://pc.example.test");
    assert!(
        serde_json::from_value::<LookupResponse>(plain_http).is_err(),
        "canonicalOrigin 必须是 https origin"
    );

    let mut unknown_field = fixture.clone();
    unknown_field["scopes"] = serde_json::json!(["session.read"]);
    assert!(
        serde_json::from_value::<LookupResponse>(unknown_field).is_err(),
        "lookup 响应不得携带授权集合或任何未登记字段"
    );
}

/// 旧设备完整资料载荷没有 DTO：`pairing.schema.json` 不再声明 `qrPayload`，这里用负例钉住
/// 「旧格式 JSON 不能按任何配对 HTTPS 载荷被接受」。
#[test]
fn legacy_full_qr_payload_is_not_a_pairing_payload() {
    let legacy = serde_json::json!({
        "pairingProtocol": "acp-remote-pairing-v1",
        "hostId": "bdb2ec20-f98c-4d87-b789-e540d527ef87",
        "hostPublicKey": "BFJtc8UddTaNGZ6P9Dmvpr5IhKZbCihAwZ-K6LY-gJBqKIb_fCSFjJtFWkQbO-AGx0AXaZPhwTZbaa5qYnYv4eo",
        "canonicalOrigin": "https://pc.example.test",
        "pairingId": "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
        "pairingSecret": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
        "expiresAt": "2026-09-18T09:14:33.000Z"
    });
    assert!(
        serde_json::from_value::<QrLookupRequest>(legacy.clone()).is_err(),
        "旧完整资料不能被解释成 qr-lookup 请求"
    );
    assert!(
        serde_json::from_value::<LookupResponse>(legacy).is_err(),
        "旧完整资料不能被解释成 lookup 响应"
    );
}
