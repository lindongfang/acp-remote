//! Node Link 配对 v1：`fixtures/node-link/v1/manifest.json` 的配对夹具 + 合同级边界。
//!
//! 配对是 HTTPS 载荷而不是 WSS 消息，因此不走 `envelope_fixtures` 的信封路径；本文件是它自己的驱动与
//! 判据。每条夹具都声明它对应 `pairing.schema.json` 的哪个 `$defs`——新增夹具必须在此登记，否则失败。

mod support;

use std::collections::BTreeSet;

use node_link_protocol::pairing::{
    ClaimRequest, ClaimResponse, Endpoint, HttpError, PairingStatus, QrPayload, StatusRequest,
    StatusResponse,
};

const FIXTURE_ROOT: &str = "fixtures/node-link/v1/";
const MANIFEST: &str = "fixtures/node-link/v1/manifest.json";
const PAIRING_SCHEMA_SUFFIX: &str = "schemas/node-link/v1/pairing.schema.json";

/// 配对夹具数（`pairing.schema.json` 的六个 `$defs` 中，`httpError` 与 `link.error` body 同形状，
/// 由 `http_error_reuses_the_registered_error_codes` 覆盖，因此没有独立夹具）。
const EXPECTED_PAIRING_CASES: usize = 2;
const EXPECTED_COVERED_DEFS: usize = 2;

fn is_pairing(schema: &str) -> bool {
    schema.ends_with(PAIRING_SCHEMA_SUFFIX)
}

fn fixture_value(relative: &str) -> serde_json::Value {
    let text = support::read_text(&support::repo_path(&format!("{FIXTURE_ROOT}{relative}")));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{relative}：不是合法 JSON：{error}"))
}

fn approved_status_response() -> serde_json::Value {
    fixture_value("valid/pairing-status-approved.json")
}

/// 夹具 → 它必须解析成功的形状；返回值是 `pairing.schema.json` 的 `$defs` 名。
fn parse_registered(fixture: &str, text: &str) -> Result<&'static str, String> {
    let parsed = |result: Result<(), serde_json::Error>| result.map_err(|error| error.to_string());
    match fixture {
        "valid/pairing-claim-request.json" => {
            parsed(serde_json::from_str::<ClaimRequest>(text).map(|_| ())).map(|()| "claimRequest")
        }
        "valid/pairing-status-approved.json" => {
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
    println!("node-link pairing: {cases} 条夹具解析成功，覆盖 {EXPECTED_COVERED_DEFS} 个 $defs");
}

#[test]
fn endpoint_requires_a_wss_url() {
    assert!(Endpoint::parse("wss://work-pc.example.ts.net/node-link/v1").is_ok());
    assert!(Endpoint::parse("wss://work-pc.example.ts.net:8443/node-link/v1").is_ok());

    for rejected in [
        "ws://work-pc.example.ts.net/node-link/v1",
        "https://work-pc.example.ts.net/node-link/v1",
        "node-link/v1",
        "",
    ] {
        assert!(
            Endpoint::parse(rejected).is_err(),
            "endpoint {rejected:?} 必须被拒（Node Link 走 WSS）"
        );
    }

    // 身份绑定与 URL 语义都要求逐字保留。
    let endpoint = Endpoint::parse("wss://PC.Example.Test:8443/node-link/v1").expect("合法");
    assert_eq!(endpoint.as_str(), "wss://PC.Example.Test:8443/node-link/v1");

    let overlong = format!("wss://example.test/{}", "a".repeat(2048));
    assert!(
        Endpoint::parse(&overlong).is_err(),
        "endpoint 的 maxLength 2048 必须被执行"
    );
}

#[test]
fn claim_request_constants_and_widths_are_enforced() {
    let mut claim = fixture_value("valid/pairing-claim-request.json");
    assert!(serde_json::from_value::<ClaimRequest>(claim.clone()).is_ok());

    claim["nodeKind"] = serde_json::json!("owner");
    assert!(
        serde_json::from_value::<ClaimRequest>(claim.clone()).is_err(),
        "Access 的 claim 里 nodeKind 只能是 access"
    );

    let mut claim = fixture_value("valid/pairing-claim-request.json");
    claim["accessPublicKey"] = serde_json::json!("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8");
    assert!(
        serde_json::from_value::<ClaimRequest>(claim).is_err(),
        "accessPublicKey 必须是 65 字节"
    );

    let mut unknown = fixture_value("valid/pairing-claim-request.json");
    unknown["futureField"] = serde_json::json!(1);
    assert!(
        serde_json::from_value::<ClaimRequest>(unknown).is_err(),
        "配对载荷是关闭形状：未知字段必须被拒"
    );
}

#[test]
fn status_response_node_and_owner_follow_the_status() {
    let parsed: StatusResponse =
        serde_json::from_value(approved_status_response()).expect("approved 响应");
    assert_eq!(parsed.status, PairingStatus::Approved);
    assert!(parsed.node.is_some() && parsed.owner.is_some());

    let mut without_owner = approved_status_response();
    without_owner
        .as_object_mut()
        .expect("object")
        .remove("owner");
    assert!(
        serde_json::from_value::<StatusResponse>(without_owner).is_err(),
        "approved 必须同时给出 node 与 owner"
    );

    let mut pending = approved_status_response();
    pending["status"] = serde_json::json!("pending_confirmation");
    assert!(
        serde_json::from_value::<StatusResponse>(pending).is_err(),
        "非 approved 状态不得携带 node/owner（schema 的 if/then/else 的 else 分支）"
    );

    let mut null_node = approved_status_response();
    null_node["status"] = serde_json::json!("expired");
    null_node["node"] = serde_json::Value::Null;
    assert!(
        serde_json::from_value::<StatusResponse>(null_node).is_err(),
        "node 可选但没有 null 分支：显式 null 必须被拒"
    );
}

#[test]
fn status_request_and_claim_response_shapes_are_enforced() {
    // statusRequest 的 `requestNonce` 与 `proof` 都是 32 字节。
    let request = "{\"protocolVersion\":1,\"ownerNodeId\":\"00010203-0405-4607-8809-0a0b0c0d0e0f\",\"accessNodeId\":\"10111213-1415-4617-9819-1a1b1c1d1e1f\",\"pairingId\":\"20212223-2425-4627-a829-2a2b2c2d2e2f\",\"pairingRequestId\":\"30313233-3435-4637-8839-3a3b3c3d3e3f\",\"requestNonce\":\"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8\",\"proof\":\"SSeJrzv-KTZYs8z6EBlOIuS9g2ykm2YUnVV8IzdGVtg\"}";
    assert!(serde_json::from_str::<StatusRequest>(request).is_ok());

    let wrong_width = request.replace(
        "\"proof\":\"SSeJrzv-KTZYs8z6EBlOIuS9g2ykm2YUnVV8IzdGVtg\"",
        "\"proof\":\"AAECAwQFBgcICQoLDA0ODw\"",
    );
    assert!(
        serde_json::from_str::<StatusRequest>(&wrong_width).is_err(),
        "proof 必须是 32 字节"
    );

    // claimResponse.status 只有 pending_confirmation 一个取值。
    let response = "{\"protocolVersion\":1,\"pairingRequestId\":\"30313233-3435-4637-8839-3a3b3c3d3e3f\",\"serverNonce\":\"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8\",\"ownerProof\":\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\",\"status\":\"pending_confirmation\",\"expiresAt\":\"2026-09-18T12:15:00.000Z\"}";
    assert!(serde_json::from_str::<ClaimResponse>(response).is_ok());

    let approved = response.replace("\"pending_confirmation\"", "\"approved\"");
    assert!(
        serde_json::from_str::<ClaimResponse>(&approved).is_err(),
        "claimResponse.status 只有 pending_confirmation 一个取值"
    );
}

#[test]
fn qr_payload_carries_the_owner_endpoint_and_secret() {
    let qr = "{\"pairingProtocol\":\"acp-remote-nodelink-v1\",\"ownerNodeId\":\"00010203-0405-4607-8809-0a0b0c0d0e0f\",\"ownerPublicKey\":\"BG1C7G0Jmvz40KI6ZLjCB4_g5EC8ECzTRVWU6w0tjgIAEfkJqm0DLZX8aVyyMhZbkcA4cCkBCjzw1MlN0RnxBGo\",\"endpoint\":\"wss://work-pc.example.ts.net/node-link/v1\",\"pairingId\":\"20212223-2425-4627-a829-2a2b2c2d2e2f\",\"pairingSecret\":\"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8\",\"expiresAt\":\"2026-09-18T12:15:00.000Z\"}";
    assert!(serde_json::from_str::<QrPayload>(qr).is_ok());

    let wrong_protocol = qr.replace("acp-remote-nodelink-v1", "acp-remote-pairing-v1");
    assert!(
        serde_json::from_str::<QrPayload>(&wrong_protocol).is_err(),
        "pairingProtocol 只接受 node-link 的 const 取值"
    );

    let not_wss = qr.replace(
        "wss://work-pc.example.ts.net/node-link/v1",
        "https://work-pc.example.ts.net/node-link/v1",
    );
    assert!(
        serde_json::from_str::<QrPayload>(&not_wss).is_err(),
        "endpoint 必须是 wss://…/node-link/v1"
    );

    let short_secret = qr.replace(
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
        "AAECAwQFBgcICQoLDA0ODw",
    );
    assert!(
        serde_json::from_str::<QrPayload>(&short_secret).is_err(),
        "pairingSecret 必须是 32 字节"
    );

    // 二维码载荷不含 `nodeKind`（那是 claimRequest 的字段）：未知字段必须被拒。
    let with_kind = qr.replace("\"endpoint\"", "\"nodeKind\":\"access\",\"endpoint\"");
    assert!(
        serde_json::from_str::<QrPayload>(&with_kind).is_err(),
        "二维码载荷是关闭形状"
    );
}

#[test]
fn http_error_reuses_the_registered_error_codes() {
    let ok = "{\"code\":\"nodelink.export.revoked\",\"message\":\"x\",\"retryable\":false,\"correlationId\":null,\"details\":{}}";
    let parsed: HttpError =
        serde_json::from_str(ok).expect("配对 HTTP 错误体与 link.error body 同形状");
    assert!(parsed.correlation_id.is_null());
    assert_eq!(parsed.details.get(), "{}");

    let unknown_code = ok.replace("nodelink.export.revoked", "nodelink.export.nonexistent");
    assert!(
        serde_json::from_str::<HttpError>(&unknown_code).is_err(),
        "未登记的错误码必须被拒（与 WSS link.error body 共用同一枚举）"
    );

    let missing_key = "{\"code\":\"nodelink.export.revoked\",\"message\":\"x\",\"retryable\":false,\"details\":{}}";
    assert!(
        serde_json::from_str::<HttpError>(missing_key).is_err(),
        "correlationId 是 required（可 null）：键缺失必须被拒"
    );
}
