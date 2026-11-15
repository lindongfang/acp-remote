#[cfg(test)]
mod independent {
    use crate::{
        adapters::product_requests::{ProductRequestSigner, issue, verify},
        claims::{self, PublicKeySec1},
        domain::{EntityId, native::RequestEpoch, requests::*},
        jws::{self, JwsError, JwsType, ParsedJws},
    };
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD as B64};
    use p256::ecdsa::{Signature, SigningKey, signature::Signer};
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::time::Duration;

    const NOW: u64 = 1_700_000_000_000;
    fn id(byte: u8) -> EntityId {
        EntityId::from_bytes([byte; 16])
    }
    fn epoch() -> RequestEpoch {
        RequestEpoch(id(2))
    }
    fn intent() -> RequestIntent {
        RequestIntent {
            actor: id(3),
            action: ProductAction::SessionInput,
            target: ProductTarget {
                kind: ProductTargetKind::Session,
                id: id(4),
            },
            payload_digest: [5; 32],
        }
    }
    struct SyntheticNode {
        key: SigningKey,
        node: String,
    }
    impl SyntheticNode {
        fn new(byte: u8) -> Self {
            let key = SigningKey::from_slice(&[byte; 32]).unwrap();
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
        fn raw(&self, header: &str, payload: &str) -> String {
            let input = format!("{}.{}", B64.encode(header), B64.encode(payload));
            let signature: Signature = self.key.sign(input.as_bytes());
            format!("{input}.{}", B64.encode(signature.to_bytes()))
        }
        fn claims(&self, claims: &Value) -> String {
            self.raw(
                r#"{"alg":"ES256","typ":"acpr-product-request+jwt"}"#,
                &claims.to_string(),
            )
        }
    }
    impl ProductRequestSigner for SyntheticNode {
        fn node_id(&self) -> &str {
            &self.node
        }
        fn sign_product_request(&self, payload: &[u8]) -> Result<String, JwsError> {
            jws::sign(JwsType::ProductRequest, payload, &self.key)
        }
    }
    fn time(now: u64) -> RequestTime {
        RequestClock::new(0, now)
            .unwrap()
            .observe(Some(now), Some(Duration::ZERO))
            .unwrap()
    }
    fn payload(node: &SyntheticNode) -> Value {
        json!({"nodeId":node.node,"requestEpoch":id(2).encode(),"actorId":id(3).encode(),
            "requestId":id(1).encode(),"action":"session.input","targetKind":"session",
            "targetId":id(4).encode(),"payloadDigest":hex::encode([5;32]),
            "issuedAt":NOW,"expiresAt":NOW+REQUEST_VALIDITY_MS})
    }
    fn valid(node: &SyntheticNode, value: &Value, now: u64) -> bool {
        verify(
            &node.claims(value),
            &node.public(),
            epoch(),
            &intent(),
            time(now),
        )
        .is_ok()
    }

    #[test]
    fn p_shape_exact_claims_default_lifetime_public_signer() {
        let node = SyntheticNode::new(7);
        let signed = issue(&node, epoch(), id(1), &intent(), time(NOW)).unwrap();
        let parts: Vec<_> = signed.compact().split('.').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(
            serde_json::from_slice::<Value>(&B64.decode(parts[0]).unwrap()).unwrap(),
            json!({"alg":"ES256","typ":"acpr-product-request+jwt"})
        );
        assert_eq!(B64.decode(parts[2]).unwrap().len(), 64);
        let actual: Value = serde_json::from_slice(&B64.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(actual.as_object().unwrap().len(), 10);
        assert_eq!(actual, payload(&node));
        let result = verify(
            signed.compact(),
            &node.public(),
            epoch(),
            &intent(),
            time(NOW),
        )
        .unwrap();
        assert_eq!(result.identity().request, id(1));
        assert_eq!(result.identity().issued_at, NOW);
        assert_eq!(result.identity().expires_at, NOW + 86_400_000);
        assert!(!format!("{signed:?}").contains(signed.compact()));
    }

    #[test]
    fn p_identifiers_canonical_lengths_and_hex() {
        let node = SyntheticNode::new(7);
        for field in ["requestEpoch", "actorId", "requestId", "targetId"] {
            for bad in [
                String::new(),
                "A".repeat(21),
                "A".repeat(23),
                format!("{}=", id(1).encode()),
                "!".repeat(22),
                format!("{}B", "A".repeat(21)),
            ] {
                let mut value = payload(&node);
                value[field] = json!(bad);
                assert!(!valid(&node, &value, NOW), "field={field}");
            }
        }
        for field in ["nodeId", "payloadDigest"] {
            for bad in [
                String::new(),
                "a".repeat(63),
                "a".repeat(65),
                "G".repeat(64),
                "A".repeat(64),
            ] {
                let mut value = payload(&node);
                value[field] = json!(bad);
                assert!(!valid(&node, &value, NOW), "field={field}");
            }
        }
        assert!(valid(&node, &payload(&node), NOW));
    }

    #[test]
    fn p_enums_and_exact_schema() {
        let node = SyntheticNode::new(7);
        let actions = [
            "harness.register",
            "workspace.register",
            "session.create",
            "session.load",
            "session.input",
            "session.cancel",
            "session.release",
            "session.resume",
            "approval.decide",
        ];
        for label in actions {
            let mut expected = intent();
            expected.action = ProductAction::from_label(label).unwrap();
            assert_eq!(expected.action.label(), label);
            let issued = issue(&node, epoch(), id(1), &expected, time(NOW)).unwrap();
            assert!(
                verify(
                    issued.compact(),
                    &node.public(),
                    epoch(),
                    &expected,
                    time(NOW)
                )
                .is_ok()
            );
        }
        for label in ["harness", "workspace", "source", "session", "approval"] {
            let mut expected = intent();
            expected.target.kind = ProductTargetKind::from_label(label).unwrap();
            assert_eq!(expected.target.kind.label(), label);
            let issued = issue(&node, epoch(), id(1), &expected, time(NOW)).unwrap();
            assert!(
                verify(
                    issued.compact(),
                    &node.public(),
                    epoch(),
                    &expected,
                    time(NOW)
                )
                .is_ok()
            );
        }
        assert!(ProductAction::from_label("session.snapshot").is_none());
        assert!(ProductTargetKind::from_label("native").is_none());
        for field in payload(&node).as_object().unwrap().keys() {
            let mut missing = payload(&node);
            missing.as_object_mut().unwrap().remove(field);
            assert!(!valid(&node, &missing, NOW), "missing {field}");
            for bad in [Value::Null, json!([]), json!(false)] {
                let mut value = payload(&node);
                value[field] = bad;
                assert!(!valid(&node, &value, NOW), "type {field}");
            }
        }
        for field in ["action", "targetKind"] {
            let mut value = payload(&node);
            value[field] = json!("unknown");
            assert!(!valid(&node, &value, NOW));
        }
        let mut extra = payload(&node);
        extra["prompt"] = json!("SYNTHETIC_SECRET_MARKER");
        assert!(!valid(&node, &extra, NOW));
        let raw = payload(&node).to_string();
        let duplicated = format!("{{\"actorId\":\"{}\",{}", id(3).encode(), &raw[1..]);
        assert!(
            verify(
                &node.raw(
                    r#"{"alg":"ES256","typ":"acpr-product-request+jwt"}"#,
                    &duplicated
                ),
                &node.public(),
                epoch(),
                &intent(),
                time(NOW)
            )
            .is_err()
        );
    }

    #[test]
    fn p_signature_purpose_isolation_and_envelope_regressions() {
        let node = SyntheticNode::new(7);
        let other = SyntheticNode::new(8);
        let issued = issue(&node, epoch(), id(1), &intent(), time(NOW)).unwrap();
        let compact = issued.compact();
        assert!(verify(compact, &other.public(), epoch(), &intent(), time(NOW)).is_err());
        let parts: Vec<_> = compact.split('.').collect();
        for segment in 0..3 {
            let mut changed: Vec<String> = parts.iter().map(|p| (*p).to_string()).collect();
            let mut bytes = B64.decode(parts[segment]).unwrap();
            let last = bytes.len() - 1;
            bytes[last] ^= 1;
            changed[segment] = B64.encode(bytes);
            assert!(
                verify(
                    &changed.join("."),
                    &node.public(),
                    epoch(),
                    &intent(),
                    time(NOW)
                )
                .is_err()
            );
        }
        for purpose in [
            JwsType::PairClaim,
            JwsType::PairStatus,
            JwsType::HostClaim,
            JwsType::HostStatus,
            JwsType::AuthClient,
            JwsType::AuthHost,
        ] {
            let old = jws::sign(purpose, payload(&node).to_string().as_bytes(), &node.key).unwrap();
            assert!(verify(&old, &node.public(), epoch(), &intent(), time(NOW)).is_err());
            assert!(ParsedJws::parse(compact, &[purpose]).is_err());
        }
        let pk = PublicKeySec1::new(node.public().try_into().unwrap()).unwrap();
        assert!(claims::verify_pair_claim(compact).is_err());
        assert!(claims::verify_pair_status(compact).is_err());
        assert!(claims::verify_claim_result(compact, &pk).is_err());
        assert!(claims::verify_status_result(compact, &pk).is_err());
        assert!(claims::verify_auth_client(compact).is_err());
        assert!(claims::verify_auth_host(compact, &pk).is_err());
        for header in [
            r#"{"alg":"none","typ":"acpr-product-request+jwt"}"#,
            r#"{"alg":"ES256","typ":"acpr-product-request+jwt","extra":true}"#,
            r#"{"alg":"ES256","alg":"ES256","typ":"acpr-product-request+jwt"}"#,
        ] {
            assert!(
                verify(
                    &node.raw(header, &payload(&node).to_string()),
                    &node.public(),
                    epoch(),
                    &intent(),
                    time(NOW)
                )
                .is_err()
            );
        }
        let input = format!("{}.{}", parts[0], parts[1]);
        let signature: Signature = node.key.sign(input.as_bytes());
        let der = format!("{input}.{}", B64.encode(signature.to_der().as_bytes()));
        assert!(verify(&der, &node.public(), epoch(), &intent(), time(NOW)).is_err());
        for malformed in [
            format!("{compact}.x"),
            format!("{}.{}.{}=", parts[0], parts[1], parts[2]),
            "x".repeat(16385),
        ] {
            assert!(verify(&malformed, &node.public(), epoch(), &intent(), time(NOW)).is_err());
        }
    }

    #[test]
    fn p_bindings_independent_expected_context() {
        let node = SyntheticNode::new(7);
        let compact = node.claims(&payload(&node));
        let mut cases = Vec::new();
        let mut value = intent();
        value.actor = id(9);
        cases.push(value);
        let mut value = intent();
        value.action = ProductAction::SessionCreate;
        cases.push(value);
        let mut value = intent();
        value.target.kind = ProductTargetKind::Source;
        cases.push(value);
        let mut value = intent();
        value.target.id = id(9);
        cases.push(value);
        let mut value = intent();
        value.payload_digest = [9; 32];
        cases.push(value);
        for expected in cases {
            assert!(verify(&compact, &node.public(), epoch(), &expected, time(NOW)).is_err());
        }
        assert!(
            verify(
                &compact,
                &node.public(),
                RequestEpoch(id(9)),
                &intent(),
                time(NOW)
            )
            .is_err()
        );
        let mut wrong_node = payload(&node);
        wrong_node["nodeId"] = json!("a".repeat(64));
        assert!(!valid(&node, &wrong_node, NOW));
        assert!(valid(&node, &payload(&node), NOW));
    }

    #[test]
    fn p_millis_raw_integer_forms_safe_bounds() {
        let node = SyntheticNode::new(7);
        for field in ["issuedAt", "expiresAt"] {
            for representation in ["-1", "1.0", "1e3", "\"1000\"", "null", "9007199254740992"] {
                let value = payload(&node);
                let original = value[field].to_string();
                let raw = value.to_string().replace(
                    &format!("\"{field}\":{original}"),
                    &format!("\"{field}\":{representation}"),
                );
                assert!(
                    verify(
                        &node.raw(r#"{"alg":"ES256","typ":"acpr-product-request+jwt"}"#, &raw),
                        &node.public(),
                        epoch(),
                        &intent(),
                        time(NOW)
                    )
                    .is_err(),
                    "{field}={representation}"
                );
            }
        }
        let mut zero = payload(&node);
        zero["issuedAt"] = json!(0);
        zero["expiresAt"] = json!(1);
        assert!(valid(&node, &zero, 0));
        let mut edge = payload(&node);
        edge["issuedAt"] = json!(MAX_REQUEST_TIME - 1);
        edge["expiresAt"] = json!(MAX_REQUEST_TIME);
        assert!(valid(&node, &edge, MAX_REQUEST_TIME - 1));
        assert!(!valid(&node, &edge, MAX_REQUEST_TIME));
        assert!(
            issue(
                &node,
                epoch(),
                id(1),
                &intent(),
                time(MAX_REQUEST_TIME - REQUEST_VALIDITY_MS)
            )
            .is_ok()
        );
        assert!(
            issue(
                &node,
                epoch(),
                id(1),
                &intent(),
                time(MAX_REQUEST_TIME - REQUEST_VALIDITY_MS + 1)
            )
            .is_err()
        );
        assert!(issue(&node, epoch(), id(1), &intent(), time(MAX_REQUEST_TIME)).is_err());
    }

    #[test]
    fn p_window_future_iat_short_and_default_cutoffs() {
        let node = SyntheticNode::new(7);
        for (issued, expires, now, accept) in [
            (NOW, NOW + 1, NOW, true),
            (NOW + 1, NOW + 2, NOW, false),
            (NOW, NOW, NOW, false),
            (NOW, NOW - 1, NOW, false),
            (NOW, NOW + REQUEST_VALIDITY_MS, NOW, true),
            (NOW, NOW + REQUEST_VALIDITY_MS + 1, NOW, false),
        ] {
            let mut value = payload(&node);
            value["issuedAt"] = json!(issued);
            value["expiresAt"] = json!(expires);
            assert_eq!(valid(&node, &value, now), accept);
        }
    }

    #[test]
    fn p_expiry_exact_millisecond_without_renewal() {
        let node = SyntheticNode::new(7);
        let mut value = payload(&node);
        value["expiresAt"] = json!(NOW + 10);
        let token = node.claims(&value);
        for (now, accepted) in [(NOW + 9, true), (NOW + 10, false), (NOW + 11, false)] {
            assert_eq!(
                verify(&token, &node.public(), epoch(), &intent(), time(now)).is_ok(),
                accepted
            );
        }
        assert_eq!(value["expiresAt"], json!(NOW + 10));
    }

    #[test]
    fn p_clock_start_imported_floor() {
        assert!(RequestClock::new(100, 99).is_err());
        assert!(RequestClock::new(MAX_REQUEST_TIME + 1, MAX_REQUEST_TIME).is_err());
        assert!(RequestClock::new(0, MAX_REQUEST_TIME + 1).is_err());
        for (floor, wall) in [(100, 100), (100, 101)] {
            let sample = RequestClock::new(floor, wall)
                .unwrap()
                .observe(Some(wall), Some(Duration::ZERO))
                .unwrap();
            assert_eq!(sample.millis(), wall);
            assert_eq!(sample.imported_watermark(), floor);
        }
    }

    #[test]
    fn p_clock_effective_max_wall_and_elapsed() {
        let mut clock = RequestClock::new(900, 1000).unwrap();
        for (wall, elapsed, expected) in [
            (1000, 0, 1000),
            (1000, 10, 1010),
            (1030, 11, 1030),
            (1030, 40, 1040),
            (1030, 40, 1040),
        ] {
            let sample = clock
                .observe(Some(wall), Some(Duration::from_millis(elapsed)))
                .unwrap();
            assert_eq!(sample.millis(), expected);
            assert_eq!(sample.candidate_watermark(), expected);
            assert_eq!(sample.imported_watermark(), 900);
        }
        let node = SyntheticNode::new(7);
        let mut value = payload(&node);
        value["expiresAt"] = json!(NOW + 10);
        let mut clock = RequestClock::new(NOW, NOW).unwrap();
        let elapsed = clock
            .observe(Some(NOW), Some(Duration::from_millis(10)))
            .unwrap();
        assert!(
            verify(
                &node.claims(&value),
                &node.public(),
                epoch(),
                &intent(),
                elapsed
            )
            .is_err()
        );
    }

    #[test]
    fn p_clock_sticky_failures_missing_regressing_overflowing() {
        let failures = [
            (Some(1000), Some(Duration::from_millis(9))),
            (Some(999), Some(Duration::from_millis(11))),
            (None, Some(Duration::from_millis(11))),
            (Some(1001), None),
            (Some(MAX_REQUEST_TIME + 1), Some(Duration::from_millis(11))),
            (Some(1001), Some(Duration::MAX)),
        ];
        for (wall, elapsed) in failures {
            let mut clock = RequestClock::new(0, 1000).unwrap();
            clock
                .observe(Some(1000), Some(Duration::from_millis(10)))
                .unwrap();
            assert!(clock.observe(wall, elapsed).is_err());
            assert!(
                clock
                    .observe(Some(2000), Some(Duration::from_millis(100)))
                    .is_err()
            );
        }
        let mut edge = RequestClock::new(0, MAX_REQUEST_TIME).unwrap();
        assert!(
            edge.observe(Some(MAX_REQUEST_TIME), Some(Duration::from_millis(1)))
                .is_err()
        );
        assert!(
            edge.observe(Some(MAX_REQUEST_TIME), Some(Duration::ZERO))
                .is_err()
        );
    }

    #[test]
    fn p_watermark_candidate_never_claims_imported_commit() {
        let mut clock = RequestClock::new(900, 1000).unwrap();
        for (wall, elapsed, expected) in [(1000, 1, 1001), (1000, 2, 1002), (1005, 3, 1005)] {
            let sample = clock
                .observe(Some(wall), Some(Duration::from_millis(elapsed)))
                .unwrap();
            assert_eq!(sample.candidate_watermark(), expected);
            assert_eq!(sample.imported_watermark(), 900);
        }
        let sample = RequestClock::new(MAX_REQUEST_TIME, MAX_REQUEST_TIME)
            .unwrap()
            .observe(Some(MAX_REQUEST_TIME), Some(Duration::ZERO))
            .unwrap();
        assert_eq!(sample.candidate_watermark(), MAX_REQUEST_TIME);
        assert_eq!(sample.imported_watermark(), MAX_REQUEST_TIME);
    }
}
