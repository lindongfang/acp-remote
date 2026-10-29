use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;

use acp_remote::jws::{JwsError, JwsType, ParsedJws};

const FIXTURE: &str = include_str!("../../clients/web/tests/fixtures/jws-v1.json");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    node_id: String,
    node_public_key: String,
    client_public_key: String,
    header: String,
    header_segment: String,
    payload: String,
    payload_segment: String,
    rust: SignatureVector,
    webcrypto: SignatureVector,
    high_s: SignatureVector,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignatureVector {
    signature_segment: String,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("fixed JWS fixture")
}

fn public_key(value: &str) -> Vec<u8> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .expect("fixed SEC1 public key");
    assert_eq!(decoded.len(), 65);
    decoded
}

fn high_s_compact(fixture: &Fixture) -> String {
    format!(
        "{}.{}.{}",
        fixture.header_segment, fixture.payload_segment, fixture.high_s.signature_segment
    )
}

#[test]
fn fixed_vector_preserves_exact_header_payload_and_node_identity() {
    let fixture = fixture();
    assert_eq!(
        URL_SAFE_NO_PAD
            .decode(&fixture.header_segment)
            .expect("header segment"),
        fixture.header.as_bytes()
    );
    assert_eq!(
        URL_SAFE_NO_PAD
            .decode(&fixture.payload_segment)
            .expect("payload segment"),
        fixture.payload.as_bytes()
    );

    let parsed = ParsedJws::parse(
        &format!("{}.{}", fixture.header_segment, fixture.payload_segment),
        &[JwsType::AuthClient],
    )
    .expect_err("two segments are not a JWS");
    assert_eq!(parsed, JwsError::InvalidCompact);

    let node_key = public_key(&fixture.node_public_key);
    let node_id = acp_remote::claims::NodeId::from_public_key(
        &acp_remote::claims::PublicKeySec1::new(node_key.try_into().expect("65-byte node key"))
            .expect("valid node key"),
    )
    .expect("node id");
    assert_eq!(node_id.to_hex(), fixture.node_id);
}

#[test]
fn rust_and_webcrypto_signatures_verify_in_rust() {
    let fixture = fixture();
    let client_key = public_key(&fixture.client_public_key);
    for signature in [&fixture.rust, &fixture.webcrypto] {
        let compact = format!(
            "{}.{}.{}",
            fixture.header_segment, fixture.payload_segment, signature.signature_segment
        );
        let parsed = ParsedJws::parse(&compact, &[JwsType::AuthClient]).expect("parse vector");
        parsed.verify(&client_key).expect("verify fixed signature");
    }
}

#[test]
fn rust_accepts_fixed_high_s_signature() {
    let fixture = fixture();
    assert_ne!(
        fixture.rust.signature_segment,
        fixture.high_s.signature_segment
    );
    let compact = high_s_compact(&fixture);
    let parsed = ParsedJws::parse(&compact, &[JwsType::AuthClient]).expect("parse high-S");
    parsed
        .verify(&public_key(&fixture.client_public_key))
        .expect("high-S must verify");
}

#[test]
fn rejects_tampering_noncanonical_encoding_and_wrong_purposes() {
    let fixture = fixture();
    let client_key = public_key(&fixture.client_public_key);
    let rust_compact = format!(
        "{}.{}.{}",
        fixture.header_segment, fixture.payload_segment, fixture.rust.signature_segment
    );

    let changed_payload = fixture.payload.replace("1700000015", "1700000016");
    let changed_segment = URL_SAFE_NO_PAD.encode(changed_payload.as_bytes());
    let tampered = format!(
        "{}.{}.{}",
        fixture.header_segment, changed_segment, fixture.rust.signature_segment
    );
    let parsed = ParsedJws::parse(&tampered, &[JwsType::AuthClient]).expect("parse tampered");
    assert_eq!(parsed.verify(&client_key), Err(JwsError::InvalidSignature));

    assert!(matches!(
        ParsedJws::parse(&rust_compact, &[JwsType::AuthHost]),
        Err(JwsError::UnsupportedType)
    ));

    let padded = format!("{rust_compact}=");
    assert!(matches!(
        ParsedJws::parse(&padded, &[JwsType::AuthClient]),
        Err(JwsError::InvalidBase64)
    ));

    let duplicate_payload = URL_SAFE_NO_PAD.encode(br#"{"v":1,"v":1}"#);
    let duplicate = format!(
        "{}.{}.{}",
        fixture.header_segment, duplicate_payload, fixture.rust.signature_segment
    );
    assert!(matches!(
        ParsedJws::parse(&duplicate, &[JwsType::AuthClient]),
        Err(JwsError::DuplicateField)
    ));

    let array_payload = URL_SAFE_NO_PAD.encode(b"[]");
    let non_object = format!(
        "{}.{}.{}",
        fixture.header_segment, array_payload, fixture.rust.signature_segment
    );
    assert!(matches!(
        ParsedJws::parse(&non_object, &[JwsType::AuthClient]),
        Err(JwsError::NonObjectPayload)
    ));

    let invalid_utf8 = URL_SAFE_NO_PAD.encode([0xff_u8]);
    let invalid_utf8 = format!(
        "{}.{}.{}",
        fixture.header_segment, invalid_utf8, fixture.rust.signature_segment
    );
    assert!(matches!(
        ParsedJws::parse(&invalid_utf8, &[JwsType::AuthClient]),
        Err(JwsError::InvalidJson)
    ));

    let duplicate_header =
        URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256","alg":"ES256","typ":"acpr-auth-client+jwt"}"#);
    let duplicate_header = format!(
        "{}.{}.{}",
        duplicate_header, fixture.payload_segment, fixture.rust.signature_segment
    );
    assert!(matches!(
        ParsedJws::parse(&duplicate_header, &[JwsType::AuthClient]),
        Err(JwsError::DuplicateField)
    ));

    let unexpected_header =
        URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256","typ":"acpr-auth-client+jwt","crit":[]}"#);
    let unexpected_header = format!(
        "{}.{}.{}",
        unexpected_header, fixture.payload_segment, fixture.rust.signature_segment
    );
    assert!(matches!(
        ParsedJws::parse(&unexpected_header, &[JwsType::AuthClient]),
        Err(JwsError::InvalidHeader)
    ));

    let der_signature = URL_SAFE_NO_PAD.encode([0_u8; 70]);
    let der = format!(
        "{}.{}.{}",
        fixture.header_segment, fixture.payload_segment, der_signature
    );
    assert!(matches!(
        ParsedJws::parse(&der, &[JwsType::AuthClient]),
        Err(JwsError::InvalidSignatureEncoding)
    ));
}
