use super::*;
use crate::{domain::requests::RequestClock, jws};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::SigningKey;
use serde_json::{Value, json};
use std::time::Duration;

struct Signer {
    key: SigningKey,
    node: String,
}
impl Signer {
    fn new(value: u8) -> Self {
        let key = SigningKey::from_slice(&[value; 32]).unwrap();
        let node = hex::encode(Sha256::digest(
            key.verifying_key().to_sec1_point(false).as_bytes(),
        ));
        Self { key, node }
    }
    fn public(&self) -> Vec<u8> {
        self.key
            .verifying_key()
            .to_sec1_point(false)
            .as_bytes()
            .to_vec()
    }
    fn sign_value(&self, value: &Value) -> String {
        self.sign_product_request(&serde_json::to_vec(value).unwrap())
            .unwrap()
    }
}
impl ProductRequestSigner for Signer {
    fn node_id(&self) -> &str {
        &self.node
    }
    fn sign_product_request(&self, payload: &[u8]) -> Result<String, JwsError> {
        jws::sign(JwsType::ProductRequest, payload, &self.key)
    }
}
fn id(value: u8) -> EntityId {
    EntityId::from_bytes([value; 16])
}
fn intent() -> RequestIntent {
    RequestIntent {
        actor: id(1),
        action: ProductAction::SessionInput,
        target: ProductTarget {
            kind: ProductTargetKind::Session,
            id: id(2),
        },
        payload_digest: [3; 32],
    }
}
fn time(now: u64) -> RequestTime {
    RequestClock::new(0, now)
        .unwrap()
        .observe(Some(now), Some(Duration::ZERO))
        .unwrap()
}
fn issued(signer: &Signer) -> SignedProductRequest {
    issue(signer, RequestEpoch(id(4)), id(5), &intent(), time(1000)).unwrap()
}
fn claims(signer: &Signer) -> Value {
    ParsedJws::parse(issued(signer).compact(), &[JwsType::ProductRequest])
        .unwrap()
        .payload()
        .clone()
}

#[test]
fn default_identity_is_fixed_24_hours_and_debug_does_not_emit_token() {
    let signer = Signer::new(7);
    let issued = issued(&signer);
    let verified = verify(
        issued.compact(),
        &signer.public(),
        RequestEpoch(id(4)),
        &intent(),
        time(1000),
    )
    .unwrap();
    assert_eq!(verified.identity().request, id(5));
    assert_eq!(
        verified.identity().expires_at - verified.identity().issued_at,
        REQUEST_VALIDITY_MS
    );
    assert!(!format!("{issued:?}").contains(issued.compact()));
}

#[test]
fn exact_expiry_is_rejected_even_without_a_deduplication_record() {
    let signer = Signer::new(7);
    let token = issued(&signer);
    let expires = 1000 + REQUEST_VALIDITY_MS;
    assert!(
        verify(
            token.compact(),
            &signer.public(),
            RequestEpoch(id(4)),
            &intent(),
            time(expires - 1)
        )
        .is_ok()
    );
    for now in [expires, expires + 1] {
        assert_eq!(
            verify(
                token.compact(),
                &signer.public(),
                RequestEpoch(id(4)),
                &intent(),
                time(now)
            )
            .err(),
            Some(RequestIdentityError::Expired)
        );
    }
    let mut short = claims(&signer);
    short["expiresAt"] = json!(1001);
    assert!(
        verify(
            &signer.sign_value(&short),
            &signer.public(),
            RequestEpoch(id(4)),
            &intent(),
            time(1000)
        )
        .is_ok()
    );
}

#[test]
fn node_signature_and_all_old_jws_purposes_are_isolated_in_both_directions() {
    let signer = Signer::new(7);
    let other = Signer::new(8);
    let token = issued(&signer);
    assert!(
        verify(
            token.compact(),
            &other.public(),
            RequestEpoch(id(4)),
            &intent(),
            time(1000)
        )
        .is_err()
    );
    for purpose in [
        JwsType::PairClaim,
        JwsType::PairStatus,
        JwsType::HostClaim,
        JwsType::HostStatus,
        JwsType::AuthClient,
        JwsType::AuthHost,
    ] {
        let other_purpose = jws::sign(
            purpose,
            &serde_json::to_vec(&claims(&signer)).unwrap(),
            &signer.key,
        )
        .unwrap();
        assert_eq!(
            verify(
                &other_purpose,
                &signer.public(),
                RequestEpoch(id(4)),
                &intent(),
                time(1000)
            )
            .err(),
            Some(RequestIdentityError::InvalidSignature)
        );
        assert!(ParsedJws::parse(token.compact(), &[purpose]).is_err());
    }
    let mut segments: Vec<_> = token.compact().split('.').map(str::to_owned).collect();
    let mut changed = claims(&signer);
    changed["expiresAt"] = json!(2000);
    segments[1] = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&changed).unwrap());
    assert_eq!(
        verify(
            &segments.join("."),
            &signer.public(),
            RequestEpoch(id(4)),
            &intent(),
            time(1000)
        )
        .err(),
        Some(RequestIdentityError::InvalidSignature)
    );
}

#[test]
fn epoch_actor_action_target_kind_target_id_and_digest_each_bind_the_request() {
    let signer = Signer::new(7);
    let token = issued(&signer);
    assert_eq!(
        verify(
            token.compact(),
            &signer.public(),
            RequestEpoch(id(99)),
            &intent(),
            time(1000)
        )
        .err(),
        Some(RequestIdentityError::EpochMismatch)
    );
    for field in 0..5 {
        let mut expected = intent();
        match field {
            0 => expected.actor = id(9),
            1 => expected.action = ProductAction::SessionCancel,
            2 => expected.target.kind = ProductTargetKind::Approval,
            3 => expected.target.id = id(9),
            _ => expected.payload_digest = [9; 32],
        }
        assert_eq!(
            verify(
                token.compact(),
                &signer.public(),
                RequestEpoch(id(4)),
                &expected,
                time(1000)
            )
            .err(),
            Some(RequestIdentityError::IntentMismatch)
        );
    }
}

#[test]
fn valid_signature_does_not_make_malformed_claims_valid() {
    let signer = Signer::new(7);
    for (field, value) in [
        ("nodeId", json!("0".repeat(64))),
        ("requestEpoch", json!("invalid")),
        ("actorId", json!(format!("{}=", id(1).encode()))),
        ("requestId", json!(null)),
        ("action", json!("future.action")),
        ("targetKind", json!("future")),
        ("targetId", json!(123)),
        ("payloadDigest", json!("AB".repeat(32))),
        ("issuedAt", json!(1001)),
        ("expiresAt", json!(1000)),
        ("expiresAt", json!(1000 + REQUEST_VALIDITY_MS + 1)),
        ("expiresAt", json!(MAX_REQUEST_TIME + 1)),
        ("extra", json!(true)),
    ] {
        let mut value_claims = claims(&signer);
        value_claims[field] = value;
        assert!(
            verify(
                &signer.sign_value(&value_claims),
                &signer.public(),
                RequestEpoch(id(4)),
                &intent(),
                time(1000)
            )
            .is_err(),
            "field {field}"
        );
    }
    let mut missing = claims(&signer);
    missing.as_object_mut().unwrap().remove("targetId");
    assert!(
        verify(
            &signer.sign_value(&missing),
            &signer.public(),
            RequestEpoch(id(4)),
            &intent(),
            time(1000)
        )
        .is_err()
    );
}

#[test]
fn float_timestamp_encodings_and_issue_time_overflow_are_rejected() {
    let signer = Signer::new(7);
    let payload = serde_json::to_string(&claims(&signer)).unwrap();
    for number in ["1000.0", "1e3", "1000.5", "-1"] {
        let changed = payload.replace("\"issuedAt\":1000", &format!("\"issuedAt\":{number}"));
        let token = signer.sign_product_request(changed.as_bytes()).unwrap();
        assert!(
            verify(
                &token,
                &signer.public(),
                RequestEpoch(id(4)),
                &intent(),
                time(1000)
            )
            .is_err()
        );
    }
    assert!(
        issue(
            &signer,
            RequestEpoch(id(4)),
            id(5),
            &intent(),
            time(MAX_REQUEST_TIME - REQUEST_VALIDITY_MS + 1)
        )
        .is_err()
    );
    let token = issue(
        &signer,
        RequestEpoch(id(4)),
        id(5),
        &intent(),
        time(MAX_REQUEST_TIME - REQUEST_VALIDITY_MS),
    )
    .unwrap();
    assert!(
        verify(
            token.compact(),
            &signer.public(),
            RequestEpoch(id(4)),
            &intent(),
            time(MAX_REQUEST_TIME - 1)
        )
        .is_ok()
    );
    assert_eq!(
        verify(
            token.compact(),
            &signer.public(),
            RequestEpoch(id(4)),
            &intent(),
            time(MAX_REQUEST_TIME)
        )
        .err(),
        Some(RequestIdentityError::Expired)
    );
}

#[test]
fn monotonic_progress_supplies_effective_time_but_candidate_is_not_persisted_floor() {
    let mut clock = RequestClock::new(900, 1000).unwrap();
    let sample = clock
        .observe(Some(1000), Some(Duration::from_millis(50)))
        .unwrap();
    assert_eq!(sample.millis(), 1050);
    assert_eq!(sample.candidate_watermark(), 1050);
    assert_eq!(sample.imported_watermark(), 900);
    assert_eq!(
        clock
            .observe(Some(1200), Some(Duration::from_millis(60)))
            .unwrap()
            .millis(),
        1200
    );
    assert_eq!(
        clock
            .observe(Some(1200), Some(Duration::from_millis(70)))
            .unwrap()
            .millis(),
        1200
    );
}

#[test]
fn wall_or_monotonic_regression_and_missing_time_permanently_fault_the_clock() {
    for failure in 0..4 {
        let mut clock = RequestClock::new(900, 1000).unwrap();
        clock
            .observe(Some(1100), Some(Duration::from_millis(100)))
            .unwrap();
        let (wall, elapsed) = match failure {
            0 => (Some(1099), Some(Duration::from_millis(101))),
            1 => (Some(1101), Some(Duration::from_millis(99))),
            2 => (None, Some(Duration::from_millis(101))),
            _ => (Some(1101), None),
        };
        assert_eq!(
            clock.observe(wall, elapsed),
            Err(RequestIdentityError::ClockUnreliable)
        );
        assert_eq!(
            clock.observe(Some(5000), Some(Duration::from_millis(5000))),
            Err(RequestIdentityError::ClockUnreliable)
        );
    }
    assert!(RequestClock::new(1001, 1000).is_err());
    let mut clock = RequestClock::new(0, MAX_REQUEST_TIME).unwrap();
    assert!(
        clock
            .observe(Some(MAX_REQUEST_TIME), Some(Duration::from_millis(1)))
            .is_err()
    );
    assert!(
        clock
            .observe(Some(MAX_REQUEST_TIME), Some(Duration::ZERO))
            .is_err()
    );
}
