use acp_remote::{
    challenge::{
        ChallengeClock, ChallengeError, ChallengeRandom, ChallengeRegistry, IssuedChallenge,
    },
    claims::{AuthProof, Id128, NodeId, Nonce256, PairClaim, PublicKeySec1, UnixSeconds},
    jws::{JwsType, sign},
};
use p256::ecdsa::SigningKey;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

struct Clock(Mutex<(u64, Instant)>);
impl Clock {
    fn new() -> Self {
        Self(Mutex::new((1_700_000_000, Instant::now())))
    }
    fn advance(&self, seconds: u64, rollback: bool) {
        let mut state = self.0.lock().expect("fixture clock");
        state.1 += Duration::from_secs(seconds);
        if rollback {
            state.0 -= 100
        } else {
            state.0 += seconds
        }
    }
}
impl ChallengeClock for Clock {
    fn unix_seconds(&self) -> Result<u64, String> {
        Ok(self.0.lock().map_err(|_| "fixture poison")?.0)
    }
    fn monotonic_now(&self) -> Result<Instant, String> {
        Ok(self.0.lock().map_err(|_| "fixture poison")?.1)
    }
}
struct Random {
    fail: AtomicBool,
    next: AtomicU64,
}
impl ChallengeRandom for Random {
    fn fill(&self, output: &mut [u8]) -> Result<(), String> {
        if self.fail.load(Ordering::SeqCst) {
            return Err("explicit fixture entropy failure".into());
        }
        output.fill(0);
        output[..8].copy_from_slice(&self.next.fetch_add(1, Ordering::SeqCst).to_le_bytes());
        Ok(())
    }
}
fn key(byte: u8) -> SigningKey {
    SigningKey::from_slice(&[byte; 32]).expect("public fixture key")
}
fn public(key: &SigningKey) -> PublicKeySec1 {
    PublicKeySec1::new(
        key.verifying_key()
            .to_sec1_point(false)
            .as_bytes()
            .try_into()
            .expect("SEC1 length"),
    )
    .expect("SEC1")
}
fn registry(random: Arc<Random>, clock: Arc<Clock>) -> ChallengeRegistry {
    let node = public(&key(7));
    ChallengeRegistry::with_sources(
        NodeId::from_public_key(&node).expect("node id"),
        node,
        "https://node.example.test".into(),
        random,
        clock,
    )
    .expect("registry")
}
fn sources() -> (Arc<Random>, Arc<Clock>) {
    (
        Arc::new(Random {
            fail: AtomicBool::new(false),
            next: AtomicU64::new(1),
        }),
        Arc::new(Clock::new()),
    )
}
fn auth(challenge: &IssuedChallenge, connection: Id128, client: &SigningKey) -> AuthProof {
    AuthProof {
        v: 1,
        node_id: challenge.node_id,
        node_public_key: challenge.node_public_key,
        origin: challenge.origin.clone(),
        connection_id: connection,
        challenge_id: challenge.challenge_id,
        nonce: challenge.nonce,
        iat: challenge.iat,
        exp: challenge.exp,
        client_id: Some(Id128::new([8; 16])),
        client_public_key: public(client),
        client_nonce: Nonce256::new([9; 32]),
    }
}
fn signed(proof: &AuthProof, key: &SigningKey, typ: JwsType) -> String {
    sign(typ, &serde_json::to_vec(proof).expect("payload"), key).expect("fixture signature")
}
fn main() {
    let client = key(8);
    let other = key(9);
    let connection = Id128::new([10; 16]);
    let mut count = 0;
    for variant in 0..13 {
        let (r, c) = sources();
        let store = registry(r, c);
        let issued = store
            .issue_auth(
                connection,
                Some(Id128::new([8; 16])),
                public(&client),
                Nonce256::new([9; 32]),
            )
            .expect("auth challenge");
        let mut proof = auth(&issued, connection, &client);
        let mut signing = &client;
        let mut typ = JwsType::AuthClient;
        match variant {
            0 => proof.node_id = NodeId::from_public_key(&public(&other)).expect("other node"),
            1 => proof.origin = "https://other.example.test".into(),
            2 => proof.connection_id = Id128::new([20; 16]),
            3 => proof.client_id = Some(Id128::new([21; 16])),
            4 => {
                proof.client_public_key = public(&other);
                signing = &other
            }
            5 => proof.client_nonce = Nonce256::new([22; 32]),
            6 => proof.challenge_id = Id128::new([23; 16]),
            7 => proof.nonce = Nonce256::new([24; 32]),
            8 => proof.iat = UnixSeconds::from_unix_seconds(proof.iat.get() + 1).expect("time"),
            9 => proof.exp = UnixSeconds::from_unix_seconds(proof.exp.get() + 1).expect("time"),
            10 => proof.v = 2,
            11 => proof.node_public_key = public(&other),
            _ => typ = JwsType::PairClaim,
        }
        assert!(
            store
                .verify_and_consume_auth_client(connection, &signed(&proof, signing, typ))
                .is_err(),
            "mutation {variant} accepted"
        );
        let original = auth(&issued, connection, &client);
        assert!(
            store
                .verify_and_consume_auth_client(
                    connection,
                    &signed(&original, &client, JwsType::AuthClient)
                )
                .is_err(),
            "failed authentication did not discard its scope"
        );
        count += 1;
    }
    for (age, rollback, allowed) in [
        (14, false, true),
        (15, false, false),
        (14, true, true),
        (15, true, false),
    ] {
        let (r, c) = sources();
        let store = registry(r, c.clone());
        let issued = store
            .issue_auth(
                connection,
                Some(Id128::new([8; 16])),
                public(&client),
                Nonce256::new([9; 32]),
            )
            .expect("challenge");
        let proof = signed(
            &auth(&issued, connection, &client),
            &client,
            JwsType::AuthClient,
        );
        c.advance(age, rollback);
        assert_eq!(
            store
                .verify_and_consume_auth_client(connection, &proof)
                .is_ok(),
            allowed,
            "age {age}, rollback {rollback}"
        );
        assert!(
            store
                .verify_and_consume_auth_client(connection, &proof)
                .is_err(),
            "replay accepted"
        );
        count += 1;
    }
    for (age, allowed) in [(29, true), (30, false)] {
        let (r, c) = sources();
        let store = registry(r, c.clone());
        let pairing = Id128::new([30; 16]);
        let request = Id128::new([31; 16]);
        let issued = store
            .issue_pair_claim(pairing, request)
            .expect("claim challenge");
        let payload = PairClaim {
            v: 1,
            node_id: issued.node_id,
            origin: issued.origin,
            request_id: request,
            challenge_id: issued.challenge_id,
            nonce: issued.nonce,
            iat: issued.iat,
            exp: issued.exp,
            pairing_id: pairing,
            client_public_key: public(&client),
            display_name: "Controlled fixture".into(),
        };
        let proof = sign(
            JwsType::PairClaim,
            &serde_json::to_vec(&payload).expect("payload"),
            &client,
        )
        .expect("signature");
        c.advance(age, true);
        assert_eq!(store.verify_and_consume_pair_claim(&proof).is_ok(), allowed);
        assert!(store.verify_and_consume_pair_claim(&proof).is_err());
        count += 1;
    }
    for purpose in 0..3 {
        let (r, c) = sources();
        r.fail.store(true, Ordering::SeqCst);
        let store = registry(r.clone(), c);
        let result = match purpose {
            0 => store.issue_pair_claim(Id128::new([1; 16]), Id128::new([2; 16])),
            1 => store.issue_pair_status(Id128::new([1; 16]), public(&client), Id128::new([2; 16])),
            _ => store.issue_auth(connection, None, public(&client), Nonce256::new([3; 32])),
        };
        assert!(matches!(result, Err(ChallengeError::RandomUnavailable(_))));
        r.fail.store(false, Ordering::SeqCst);
        assert!(
            store
                .issue_pair_claim(Id128::new([1; 16]), Id128::new([2; 16]))
                .is_ok()
        );
        count += 1;
    }
    println!(
        "PROBE COMPLETE: {count} controlled challenge scenarios passed; real signatures and product registry, injected clock/random only; no browser/server or live authorization claim"
    );
}
