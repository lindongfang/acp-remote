use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use thiserror::Error;

use crate::{
    claims::{
        AuthProof, Id128, MAX_CHALLENGE_SECONDS, NodeId, Nonce256, PairClaim, PairStatus,
        PublicKeySec1, UnixSeconds, verify_auth_client, verify_pair_claim, verify_pair_status,
    },
    config::NodeConfig,
    jws::JwsError,
};

pub const PAIR_CHALLENGE_LIFETIME_SECONDS: u64 = 30;
pub const AUTH_CHALLENGE_LIFETIME_SECONDS: u64 = 15;
const MAX_ACTIVE_CHALLENGES: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChallengePurpose {
    PairClaim,
    PairStatus,
    Auth,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChallengeScope {
    PairClaim(Id128),
    PairStatus(Id128),
    Auth(Id128),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuedChallenge {
    pub node_id: NodeId,
    pub node_public_key: PublicKeySec1,
    pub origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<Id128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<Id128>,
    pub challenge_id: Id128,
    pub nonce: Nonce256,
    pub iat: UnixSeconds,
    pub exp: UnixSeconds,
    #[serde(skip)]
    purpose: ChallengePurpose,
}

impl IssuedChallenge {
    #[must_use]
    pub const fn purpose(&self) -> ChallengePurpose {
        self.purpose
    }
}

#[derive(Debug, Error)]
pub enum ChallengeError {
    #[error(transparent)]
    Jws(#[from] JwsError),
    #[error("node challenge identity is invalid")]
    InvalidNodeIdentity,
    #[error("challenge origin is not a canonical HTTPS origin")]
    InvalidOrigin,
    #[error("challenge lifetime is outside protocol bounds")]
    InvalidLifetime,
    #[error("challenge random source is unavailable: {0}")]
    RandomUnavailable(String),
    #[error("challenge clock is unavailable: {0}")]
    ClockUnavailable(String),
    #[error("challenge store is unavailable")]
    StoreUnavailable,
    #[error("challenge store capacity is exhausted")]
    CapacityExhausted,
    #[error("challenge identifier collided")]
    IdentifierCollision,
    #[error("challenge was not found")]
    NotFound,
    #[error("challenge has expired")]
    Expired,
    #[error("challenge binding does not match this proof")]
    BindingMismatch,
}

/// Supplies operating-system cryptographic randomness to the challenge cache.
pub trait ChallengeRandom: Send + Sync {
    /// Fills the complete output buffer or returns an error.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating-system random source is unavailable.
    fn fill(&self, output: &mut [u8]) -> Result<(), String>;
}

/// Supplies the two clocks used by challenge issuance and expiry.
pub trait ChallengeClock: Send + Sync {
    /// Returns the current UTC Unix timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error when the system clock is unavailable or outside the protocol range.
    fn unix_seconds(&self) -> Result<u64, String>;

    /// Returns a monotonic instant used only for local expiry.
    ///
    /// # Errors
    ///
    /// Returns an error when the monotonic clock cannot be read.
    fn monotonic_now(&self) -> Result<Instant, String>;
}

#[derive(Debug)]
struct SystemRandom;

impl ChallengeRandom for SystemRandom {
    fn fill(&self, output: &mut [u8]) -> Result<(), String> {
        getrandom::fill(output).map_err(|error| error.to_string())
    }
}

#[derive(Debug)]
struct SystemClock;

impl ChallengeClock for SystemClock {
    fn unix_seconds(&self) -> Result<u64, String> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .map_err(|error| error.to_string())
    }

    fn monotonic_now(&self) -> Result<Instant, String> {
        Ok(Instant::now())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChallengeBinding {
    PairClaim {
        pairing_id: Id128,
    },
    PairStatus {
        pairing_id: Id128,
        client_public_key: PublicKeySec1,
    },
    Auth {
        connection_id: Id128,
        client_id: Option<Id128>,
        client_public_key: PublicKeySec1,
        client_nonce: Nonce256,
    },
}

#[derive(Debug)]
struct Entry {
    challenge: IssuedChallenge,
    scope: ChallengeScope,
    binding: ChallengeBinding,
    deadline: Instant,
}

#[derive(Debug, Default)]
struct RegistryState {
    entries: HashMap<Id128, Entry>,
    by_scope: HashMap<ChallengeScope, Id128>,
}

impl RegistryState {
    fn remove_challenge(&mut self, challenge_id: Id128) -> Option<Entry> {
        let entry = self.entries.remove(&challenge_id)?;
        if self.by_scope.get(&entry.scope) == Some(&challenge_id) {
            self.by_scope.remove(&entry.scope);
        }
        Some(entry)
    }

    fn prune_expired(&mut self, now: Instant) {
        let expired: Vec<Id128> = self
            .entries
            .iter()
            .filter_map(|(id, entry)| (entry.deadline <= now).then_some(*id))
            .collect();
        for challenge_id in expired {
            self.remove_challenge(challenge_id);
        }
    }
}

pub struct ChallengeRegistry {
    node_id: NodeId,
    node_public_key: PublicKeySec1,
    origin: String,
    random: Arc<dyn ChallengeRandom>,
    clock: Arc<dyn ChallengeClock>,
    state: Mutex<RegistryState>,
}

impl ChallengeRegistry {
    /// Creates a registry backed by the operating-system random source and clocks.
    ///
    /// # Errors
    ///
    /// Returns an error when the node ID does not match its public key or the origin is not a
    /// canonical HTTPS origin.
    pub fn new(
        node_id: NodeId,
        node_public_key: PublicKeySec1,
        origin: String,
    ) -> Result<Self, ChallengeError> {
        Self::with_sources(
            node_id,
            node_public_key,
            origin,
            Arc::new(SystemRandom),
            Arc::new(SystemClock),
        )
    }

    /// Creates a registry with explicit randomness and time sources for deterministic tests.
    ///
    /// # Errors
    ///
    /// Returns an error when the node ID does not match its public key or the origin is not a
    /// canonical HTTPS origin.
    pub fn with_sources(
        node_id: NodeId,
        node_public_key: PublicKeySec1,
        origin: String,
        random: Arc<dyn ChallengeRandom>,
        clock: Arc<dyn ChallengeClock>,
    ) -> Result<Self, ChallengeError> {
        if NodeId::from_public_key(&node_public_key)? != node_id {
            return Err(ChallengeError::InvalidNodeIdentity);
        }
        let canonical =
            NodeConfig::normalize_origin(&origin).map_err(|_| ChallengeError::InvalidOrigin)?;
        if canonical != origin {
            return Err(ChallengeError::InvalidOrigin);
        }
        Ok(Self {
            node_id,
            node_public_key,
            origin,
            random,
            clock,
            state: Mutex::new(RegistryState::default()),
        })
    }

    /// Issues or replaces the single claim challenge for a pairing.
    ///
    /// # Errors
    ///
    /// Returns an error when randomness or time is unavailable, the store is unavailable, or its
    /// bounded capacity is exhausted.
    pub fn issue_pair_claim(
        &self,
        pairing_id: Id128,
        request_id: Id128,
    ) -> Result<IssuedChallenge, ChallengeError> {
        self.issue(
            ChallengeScope::PairClaim(pairing_id),
            ChallengePurpose::PairClaim,
            ChallengeBinding::PairClaim { pairing_id },
            Some(request_id),
            PAIR_CHALLENGE_LIFETIME_SECONDS,
        )
    }

    /// Issues or replaces the single status challenge for a pairing and public key.
    ///
    /// # Errors
    ///
    /// Returns an error when randomness or time is unavailable, the store is unavailable, or its
    /// bounded capacity is exhausted.
    pub fn issue_pair_status(
        &self,
        pairing_id: Id128,
        client_public_key: PublicKeySec1,
        request_id: Id128,
    ) -> Result<IssuedChallenge, ChallengeError> {
        self.issue(
            ChallengeScope::PairStatus(pairing_id),
            ChallengePurpose::PairStatus,
            ChallengeBinding::PairStatus {
                pairing_id,
                client_public_key,
            },
            Some(request_id),
            PAIR_CHALLENGE_LIFETIME_SECONDS,
        )
    }

    /// Issues or replaces the single authentication challenge for a connection.
    ///
    /// # Errors
    ///
    /// Returns an error when randomness or time is unavailable, the store is unavailable, or its
    /// bounded capacity is exhausted.
    pub fn issue_auth(
        &self,
        connection_id: Id128,
        client_id: Option<Id128>,
        client_public_key: PublicKeySec1,
        client_nonce: Nonce256,
    ) -> Result<IssuedChallenge, ChallengeError> {
        self.issue(
            ChallengeScope::Auth(connection_id),
            ChallengePurpose::Auth,
            ChallengeBinding::Auth {
                connection_id,
                client_id,
                client_public_key,
                client_nonce,
            },
            None,
            AUTH_CHALLENGE_LIFETIME_SECONDS,
        )
    }

    /// Verifies a claim signature and atomically consumes its challenge.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid signature or schema, a missing or expired challenge, or a
    /// binding mismatch. The challenge is not consumed on verification or binding failure.
    pub fn verify_and_consume_pair_claim(
        &self,
        compact: &str,
    ) -> Result<PairClaim, ChallengeError> {
        let claim = verify_pair_claim(compact)?;
        self.consume_pair_claim(&claim)?;
        Ok(claim)
    }

    /// Verifies a status signature and atomically consumes its challenge.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid signature or schema, a missing or expired challenge, or a
    /// binding mismatch. The pairing challenge remains available for a corrected retry.
    pub fn verify_and_consume_pair_status(
        &self,
        compact: &str,
    ) -> Result<PairStatus, ChallengeError> {
        let status = verify_pair_status(compact)?;
        self.consume_pair_status(&status)?;
        Ok(status)
    }

    /// Verifies a management proof and atomically consumes its connection challenge.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid signature or schema, a missing or expired challenge, or a
    /// binding mismatch. Any failed attempt on the connection clears that connection's challenge.
    pub fn verify_and_consume_auth_client(
        &self,
        connection_id: Id128,
        compact: &str,
    ) -> Result<AuthProof, ChallengeError> {
        let proof = match verify_auth_client(compact) {
            Ok(proof) => proof,
            Err(error) => {
                let _ = self.discard_scope(ChallengeScope::Auth(connection_id));
                return Err(error.into());
            }
        };
        if let Err(error) = self.consume_auth(connection_id, &proof) {
            let _ = self.discard_scope(ChallengeScope::Auth(connection_id));
            return Err(error);
        }
        Ok(proof)
    }

    /// Discards the challenge associated with one scope.
    ///
    /// # Errors
    ///
    /// Returns an error when the challenge store is unavailable.
    pub fn discard_scope(&self, scope: ChallengeScope) -> Result<(), ChallengeError> {
        let mut state = self.lock_state()?;
        if let Some(challenge_id) = state.by_scope.get(&scope).copied() {
            state.remove_challenge(challenge_id);
        }
        drop(state);
        Ok(())
    }

    /// Discards both claim and status challenges for a pairing.
    ///
    /// # Errors
    ///
    /// Returns an error when the challenge store is unavailable.
    pub fn discard_pairing(&self, pairing_id: Id128) -> Result<(), ChallengeError> {
        let mut state = self.lock_state()?;
        for scope in [
            ChallengeScope::PairClaim(pairing_id),
            ChallengeScope::PairStatus(pairing_id),
        ] {
            if let Some(challenge_id) = state.by_scope.get(&scope).copied() {
                state.remove_challenge(challenge_id);
            }
        }
        drop(state);
        Ok(())
    }

    /// Removes all temporary challenges, for example when node shutdown begins.
    ///
    /// # Errors
    ///
    /// Returns an error when the challenge store is unavailable.
    pub fn clear(&self) -> Result<(), ChallengeError> {
        let mut state = self.lock_state()?;
        state.entries.clear();
        state.by_scope.clear();
        drop(state);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn active_auth_count(&self) -> usize {
        self.state
            .lock()
            .expect("test challenge state lock")
            .entries
            .values()
            .filter(|entry| entry.challenge.purpose == ChallengePurpose::Auth)
            .count()
    }

    fn issue(
        &self,
        scope: ChallengeScope,
        purpose: ChallengePurpose,
        binding: ChallengeBinding,
        request_id: Option<Id128>,
        lifetime_seconds: u64,
    ) -> Result<IssuedChallenge, ChallengeError> {
        if lifetime_seconds == 0 || lifetime_seconds > MAX_CHALLENGE_SECONDS {
            return Err(ChallengeError::InvalidLifetime);
        }

        let mut challenge_id = [0_u8; 16];
        let mut nonce = [0_u8; 32];
        self.random
            .fill(&mut challenge_id)
            .map_err(ChallengeError::RandomUnavailable)?;
        self.random
            .fill(&mut nonce)
            .map_err(ChallengeError::RandomUnavailable)?;

        let iat = UnixSeconds::from_unix_seconds(
            self.clock
                .unix_seconds()
                .map_err(ChallengeError::ClockUnavailable)?,
        )?;
        let exp = iat.checked_add(lifetime_seconds)?;
        let now = self
            .clock
            .monotonic_now()
            .map_err(ChallengeError::ClockUnavailable)?;
        let deadline = now
            .checked_add(Duration::from_secs(lifetime_seconds))
            .ok_or_else(|| {
                ChallengeError::ClockUnavailable("monotonic deadline overflow".to_owned())
            })?;

        let challenge_id = Id128::new(challenge_id);
        let nonce = Nonce256::new(nonce);
        let (request_id, connection_id) = match purpose {
            ChallengePurpose::PairClaim | ChallengePurpose::PairStatus => (
                Some(request_id.ok_or(ChallengeError::BindingMismatch)?),
                None,
            ),
            ChallengePurpose::Auth => {
                let ChallengeBinding::Auth { connection_id, .. } = binding else {
                    return Err(ChallengeError::BindingMismatch);
                };
                (None, Some(connection_id))
            }
        };
        let challenge = IssuedChallenge {
            node_id: self.node_id,
            node_public_key: self.node_public_key,
            origin: self.origin.clone(),
            request_id,
            connection_id,
            challenge_id,
            nonce,
            iat,
            exp,
            purpose,
        };

        let mut state = self.lock_state()?;
        state.prune_expired(now);
        if state.entries.contains_key(&challenge_id) {
            return Err(ChallengeError::IdentifierCollision);
        }
        if !state.by_scope.contains_key(&scope) && state.entries.len() >= MAX_ACTIVE_CHALLENGES {
            return Err(ChallengeError::CapacityExhausted);
        }
        if let Some(previous) = state.by_scope.remove(&scope) {
            state.entries.remove(&previous);
        }
        state.entries.insert(
            challenge_id,
            Entry {
                challenge: challenge.clone(),
                scope,
                binding,
                deadline,
            },
        );
        state.by_scope.insert(scope, challenge_id);
        drop(state);
        Ok(challenge)
    }

    fn consume_pair_claim(&self, proof: &PairClaim) -> Result<IssuedChallenge, ChallengeError> {
        let now = self.monotonic_now()?;
        let mut state = self.lock_state()?;
        let entry = state
            .entries
            .get(&proof.challenge_id)
            .ok_or(ChallengeError::NotFound)?;
        if entry.deadline <= now {
            state.remove_challenge(proof.challenge_id);
            return Err(ChallengeError::Expired);
        }
        if entry.challenge.purpose != ChallengePurpose::PairClaim
            || !common_matches(
                &entry.challenge,
                proof.node_id,
                &proof.origin,
                Some(proof.request_id),
                proof.challenge_id,
                proof.nonce,
                proof.iat,
                proof.exp,
            )
        {
            return Err(ChallengeError::BindingMismatch);
        }
        let ChallengeBinding::PairClaim { pairing_id } = entry.binding else {
            return Err(ChallengeError::BindingMismatch);
        };
        if pairing_id != proof.pairing_id {
            return Err(ChallengeError::BindingMismatch);
        }
        state
            .remove_challenge(proof.challenge_id)
            .map(|entry| entry.challenge)
            .ok_or(ChallengeError::NotFound)
    }

    fn consume_pair_status(&self, proof: &PairStatus) -> Result<IssuedChallenge, ChallengeError> {
        let now = self.monotonic_now()?;
        let mut state = self.lock_state()?;
        let entry = state
            .entries
            .get(&proof.challenge_id)
            .ok_or(ChallengeError::NotFound)?;
        if entry.deadline <= now {
            state.remove_challenge(proof.challenge_id);
            return Err(ChallengeError::Expired);
        }
        let ChallengeBinding::PairStatus {
            pairing_id,
            client_public_key,
        } = entry.binding
        else {
            return Err(ChallengeError::BindingMismatch);
        };
        if entry.challenge.purpose != ChallengePurpose::PairStatus
            || !common_matches(
                &entry.challenge,
                proof.node_id,
                &proof.origin,
                Some(proof.request_id),
                proof.challenge_id,
                proof.nonce,
                proof.iat,
                proof.exp,
            )
            || pairing_id != proof.pairing_id
            || client_public_key != proof.client_public_key
        {
            return Err(ChallengeError::BindingMismatch);
        }
        state
            .remove_challenge(proof.challenge_id)
            .map(|entry| entry.challenge)
            .ok_or(ChallengeError::NotFound)
    }

    fn consume_auth(
        &self,
        connection_id: Id128,
        proof: &AuthProof,
    ) -> Result<IssuedChallenge, ChallengeError> {
        let now = self.monotonic_now()?;
        let mut state = self.lock_state()?;
        let entry = state
            .entries
            .get(&proof.challenge_id)
            .ok_or(ChallengeError::NotFound)?;
        if entry.scope != ChallengeScope::Auth(connection_id) {
            return Err(ChallengeError::BindingMismatch);
        }
        if entry.deadline <= now {
            state.remove_challenge(proof.challenge_id);
            return Err(ChallengeError::Expired);
        }
        let ChallengeBinding::Auth {
            connection_id,
            client_id,
            client_public_key,
            client_nonce,
        } = entry.binding
        else {
            return Err(ChallengeError::BindingMismatch);
        };
        if entry.challenge.purpose != ChallengePurpose::Auth
            || !common_matches(
                &entry.challenge,
                proof.node_id,
                &proof.origin,
                None,
                proof.challenge_id,
                proof.nonce,
                proof.iat,
                proof.exp,
            )
            || proof.node_public_key != entry.challenge.node_public_key
            || connection_id != proof.connection_id
            || client_id != proof.client_id
            || client_public_key != proof.client_public_key
            || client_nonce != proof.client_nonce
        {
            state.remove_challenge(proof.challenge_id);
            return Err(ChallengeError::BindingMismatch);
        }
        state
            .remove_challenge(proof.challenge_id)
            .map(|entry| entry.challenge)
            .ok_or(ChallengeError::NotFound)
    }

    fn monotonic_now(&self) -> Result<Instant, ChallengeError> {
        self.clock
            .monotonic_now()
            .map_err(ChallengeError::ClockUnavailable)
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, RegistryState>, ChallengeError> {
        self.state
            .lock()
            .map_err(|_| ChallengeError::StoreUnavailable)
    }
}

#[allow(clippy::too_many_arguments)]
fn common_matches(
    challenge: &IssuedChallenge,
    node_id: NodeId,
    origin: &str,
    request_id: Option<Id128>,
    challenge_id: Id128,
    nonce: Nonce256,
    iat: UnixSeconds,
    exp: UnixSeconds,
) -> bool {
    challenge.node_id == node_id
        && challenge.origin == origin
        && challenge.request_id == request_id
        && challenge.challenge_id == challenge_id
        && challenge.nonce == nonce
        && challenge.iat == iat
        && challenge.exp == exp
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

    use p256::ecdsa::SigningKey;

    use super::*;
    use crate::{
        claims::{PairStatusValue, PublicKeySec1},
        jws::{JwsType, sign},
    };

    #[derive(Debug)]
    struct TestClockState {
        unix_seconds: u64,
        monotonic: Instant,
    }

    #[derive(Debug)]
    struct TestClock {
        state: Mutex<TestClockState>,
    }

    impl TestClock {
        fn new() -> Self {
            Self {
                state: Mutex::new(TestClockState {
                    unix_seconds: 1_700_000_000,
                    monotonic: Instant::now(),
                }),
            }
        }

        fn advance_seconds(&self, seconds: u64) {
            let mut state = self.state.lock().expect("test clock lock");
            state.monotonic += Duration::from_secs(seconds);
        }

        fn set_wall_clock(&self, unix_seconds: u64) {
            self.state.lock().expect("test clock lock").unix_seconds = unix_seconds;
        }
    }

    impl ChallengeClock for TestClock {
        fn unix_seconds(&self) -> Result<u64, String> {
            Ok(self
                .state
                .lock()
                .map_err(|error| error.to_string())?
                .unix_seconds)
        }

        fn monotonic_now(&self) -> Result<Instant, String> {
            Ok(self
                .state
                .lock()
                .map_err(|error| error.to_string())?
                .monotonic)
        }
    }

    #[derive(Debug)]
    struct TestRandom {
        next: AtomicU8,
        fail: AtomicBool,
    }

    impl TestRandom {
        fn new() -> Self {
            Self {
                next: AtomicU8::new(1),
                fail: AtomicBool::new(false),
            }
        }

        fn failing() -> Self {
            Self {
                next: AtomicU8::new(1),
                fail: AtomicBool::new(true),
            }
        }
    }

    impl ChallengeRandom for TestRandom {
        fn fill(&self, output: &mut [u8]) -> Result<(), String> {
            if self.fail.load(Ordering::Relaxed) {
                return Err("test random failure".to_owned());
            }
            let value = self.next.fetch_add(1, Ordering::Relaxed);
            output.fill(value);
            Ok(())
        }
    }

    struct Fixture {
        registry: ChallengeRegistry,
        clock: Arc<TestClock>,
        node_key: SigningKey,
        node_public_key: PublicKeySec1,
        node_id: NodeId,
    }

    impl Fixture {
        fn new() -> Self {
            Self::with_random(Arc::new(TestRandom::new()))
        }

        fn with_random(random: Arc<TestRandom>) -> Self {
            let clock = Arc::new(TestClock::new());
            let node_key = signing_key([7_u8; 32]);
            let node_public_key = public_key(&node_key);
            let node_id = NodeId::from_public_key(&node_public_key).expect("node id");
            let registry = ChallengeRegistry::with_sources(
                node_id,
                node_public_key,
                "https://node.example.test".to_owned(),
                random,
                clock.clone(),
            )
            .expect("test registry");
            Self {
                registry,
                clock,
                node_key,
                node_public_key,
                node_id,
            }
        }
    }

    fn signing_key(bytes: [u8; 32]) -> SigningKey {
        SigningKey::from_slice(&bytes).expect("test scalar")
    }

    fn public_key(key: &SigningKey) -> PublicKeySec1 {
        let point = key.verifying_key().to_sec1_point(false);
        PublicKeySec1::new(point.as_bytes().try_into().expect("65-byte SEC1 key"))
            .expect("valid SEC1 key")
    }

    fn sign_payload<T: Serialize>(typ: JwsType, payload: &T, key: &SigningKey) -> String {
        sign(
            typ,
            &serde_json::to_vec(payload).expect("serialize payload"),
            key,
        )
        .expect("sign payload")
    }

    fn pair_claim(
        _fixture: &Fixture,
        challenge: &IssuedChallenge,
        pairing_id: Id128,
        client_key: &SigningKey,
    ) -> PairClaim {
        PairClaim {
            v: 1,
            node_id: challenge.node_id,
            origin: challenge.origin.clone(),
            request_id: challenge.request_id.expect("pair request id"),
            challenge_id: challenge.challenge_id,
            nonce: challenge.nonce,
            iat: challenge.iat,
            exp: challenge.exp,
            pairing_id,
            client_public_key: public_key(client_key),
            display_name: "Alice".to_owned(),
        }
    }

    fn pair_status(
        challenge: &IssuedChallenge,
        pairing_id: Id128,
        client_key: &SigningKey,
    ) -> PairStatus {
        PairStatus {
            v: 1,
            node_id: challenge.node_id,
            origin: challenge.origin.clone(),
            request_id: challenge.request_id.expect("status request id"),
            challenge_id: challenge.challenge_id,
            nonce: challenge.nonce,
            iat: challenge.iat,
            exp: challenge.exp,
            pairing_id,
            client_public_key: public_key(client_key),
        }
    }

    fn auth_proof(
        fixture: &Fixture,
        challenge: &IssuedChallenge,
        connection_id: Id128,
        client_id: Option<Id128>,
        client_key: &SigningKey,
        client_nonce: Nonce256,
    ) -> AuthProof {
        AuthProof {
            v: 1,
            node_id: challenge.node_id,
            node_public_key: fixture.node_public_key,
            origin: challenge.origin.clone(),
            connection_id,
            challenge_id: challenge.challenge_id,
            nonce: challenge.nonce,
            iat: challenge.iat,
            exp: challenge.exp,
            client_id,
            client_public_key: public_key(client_key),
            client_nonce,
        }
    }

    fn active_challenges(registry: &ChallengeRegistry) -> usize {
        registry
            .state
            .lock()
            .expect("test state lock")
            .entries
            .len()
    }

    #[test]
    fn claim_challenge_is_bound_and_single_use() {
        let fixture = Fixture::new();
        let pairing_id = Id128::new([4; 16]);
        let request_id = Id128::new([5; 16]);
        let client_key = signing_key([8; 32]);
        let challenge = fixture
            .registry
            .issue_pair_claim(pairing_id, request_id)
            .expect("issue challenge");

        assert_eq!(challenge.node_id, fixture.node_id);
        assert_eq!(challenge.request_id, Some(request_id));
        assert_eq!(challenge.connection_id, None);
        assert_eq!(active_challenges(&fixture.registry), 1);

        let mut wrong_node = pair_claim(&fixture, &challenge, pairing_id, &client_key);
        wrong_node.node_id =
            NodeId::from_public_key(&public_key(&signing_key([9; 32]))).expect("other node id");
        let proof = sign_payload(JwsType::PairClaim, &wrong_node, &client_key);
        assert!(matches!(
            fixture.registry.verify_and_consume_pair_claim(&proof),
            Err(ChallengeError::BindingMismatch)
        ));
        assert_eq!(active_challenges(&fixture.registry), 1);

        let proof = sign_payload(
            JwsType::PairClaim,
            &pair_claim(&fixture, &challenge, pairing_id, &client_key),
            &client_key,
        );
        fixture
            .registry
            .verify_and_consume_pair_claim(&proof)
            .expect("consume challenge");
        assert!(matches!(
            fixture.registry.verify_and_consume_pair_claim(&proof),
            Err(ChallengeError::NotFound)
        ));
        assert_eq!(active_challenges(&fixture.registry), 0);
    }

    #[test]
    fn reissue_invalidates_old_challenge_and_cross_purpose_is_rejected() {
        let fixture = Fixture::new();
        let pairing_id = Id128::new([4; 16]);
        let client_key = signing_key([8; 32]);
        let first = fixture
            .registry
            .issue_pair_claim(pairing_id, Id128::new([1; 16]))
            .expect("first challenge");
        fixture
            .registry
            .issue_pair_claim(pairing_id, Id128::new([2; 16]))
            .expect("replacement challenge");
        let old_claim = sign_payload(
            JwsType::PairClaim,
            &pair_claim(&fixture, &first, pairing_id, &client_key),
            &client_key,
        );
        assert!(matches!(
            fixture.registry.verify_and_consume_pair_claim(&old_claim),
            Err(ChallengeError::NotFound)
        ));

        let status_challenge = fixture
            .registry
            .issue_pair_status(pairing_id, public_key(&client_key), Id128::new([2; 16]))
            .expect("status challenge");

        let status_as_claim = sign_payload(
            JwsType::PairClaim,
            &pair_claim(&fixture, &status_challenge, pairing_id, &client_key),
            &client_key,
        );
        assert!(matches!(
            fixture
                .registry
                .verify_and_consume_pair_claim(&status_as_claim),
            Err(ChallengeError::BindingMismatch)
        ));

        let status = sign_payload(
            JwsType::PairStatus,
            &pair_status(&status_challenge, pairing_id, &client_key),
            &client_key,
        );
        fixture
            .registry
            .verify_and_consume_pair_status(&status)
            .expect("consume status");
    }

    #[test]
    fn auth_challenge_binds_connection_identity_and_replays_fail() {
        let fixture = Fixture::new();
        let connection_id = Id128::new([5; 16]);
        let client_id = Some(Id128::new([6; 16]));
        let client_nonce = Nonce256::new([7; 32]);
        let client_key = signing_key([8; 32]);
        let challenge = fixture
            .registry
            .issue_auth(
                connection_id,
                client_id,
                public_key(&client_key),
                client_nonce,
            )
            .expect("issue auth challenge");
        assert_eq!(challenge.request_id, None);
        assert_eq!(challenge.connection_id, Some(connection_id));

        let other_connection = auth_proof(
            &fixture,
            &challenge,
            Id128::new([9; 16]),
            client_id,
            &client_key,
            client_nonce,
        );
        let other_connection = sign_payload(JwsType::AuthClient, &other_connection, &client_key);
        assert!(matches!(
            fixture
                .registry
                .verify_and_consume_auth_client(connection_id, &other_connection),
            Err(ChallengeError::BindingMismatch)
        ));
        assert_eq!(active_challenges(&fixture.registry), 0);

        let challenge = fixture
            .registry
            .issue_auth(
                connection_id,
                client_id,
                public_key(&client_key),
                client_nonce,
            )
            .expect("replacement auth challenge");

        let proof = sign_payload(
            JwsType::AuthClient,
            &auth_proof(
                &fixture,
                &challenge,
                connection_id,
                client_id,
                &client_key,
                client_nonce,
            ),
            &client_key,
        );
        fixture
            .registry
            .verify_and_consume_auth_client(connection_id, &proof)
            .expect("consume auth proof");
        assert!(matches!(
            fixture
                .registry
                .verify_and_consume_auth_client(connection_id, &proof),
            Err(ChallengeError::NotFound)
        ));
    }

    #[test]
    fn invalid_auth_signature_clears_the_connection_challenge() {
        let fixture = Fixture::new();
        let connection_id = Id128::new([5; 16]);
        let client_key = signing_key([8; 32]);
        let client_nonce = Nonce256::new([7; 32]);
        let challenge = fixture
            .registry
            .issue_auth(connection_id, None, public_key(&client_key), client_nonce)
            .expect("issue auth challenge");
        let proof = sign_payload(
            JwsType::AuthClient,
            &auth_proof(
                &fixture,
                &challenge,
                connection_id,
                None,
                &client_key,
                client_nonce,
            ),
            &signing_key([9; 32]),
        );
        assert!(matches!(
            fixture
                .registry
                .verify_and_consume_auth_client(connection_id, &proof),
            Err(ChallengeError::Jws(JwsError::InvalidSignature))
        ));
        assert_eq!(active_challenges(&fixture.registry), 0);
    }

    #[test]
    fn failed_auth_attempt_clears_only_its_own_connection_challenge() {
        let fixture = Fixture::new();
        let first_connection = Id128::new([5; 16]);
        let second_connection = Id128::new([6; 16]);
        let client_key = signing_key([8; 32]);
        let first_nonce = Nonce256::new([1; 32]);
        let second_nonce = Nonce256::new([2; 32]);
        fixture
            .registry
            .issue_auth(first_connection, None, public_key(&client_key), first_nonce)
            .expect("first auth challenge");
        let second_challenge = fixture
            .registry
            .issue_auth(
                second_connection,
                None,
                public_key(&client_key),
                second_nonce,
            )
            .expect("second auth challenge");

        assert!(matches!(
            fixture
                .registry
                .verify_and_consume_auth_client(first_connection, "not-a-jws"),
            Err(ChallengeError::Jws(JwsError::InvalidCompact))
        ));
        assert_eq!(active_challenges(&fixture.registry), 1);

        let proof = sign_payload(
            JwsType::AuthClient,
            &auth_proof(
                &fixture,
                &second_challenge,
                second_connection,
                None,
                &client_key,
                second_nonce,
            ),
            &client_key,
        );
        fixture
            .registry
            .verify_and_consume_auth_client(second_connection, &proof)
            .expect("second connection remains usable");
    }

    #[test]
    fn proof_for_another_connection_cannot_clear_that_challenge() {
        let fixture = Fixture::new();
        let attacker_connection = Id128::new([5; 16]);
        let victim_connection = Id128::new([6; 16]);
        let client_key = signing_key([8; 32]);
        let attacker_nonce = Nonce256::new([1; 32]);
        let victim_nonce = Nonce256::new([2; 32]);
        fixture
            .registry
            .issue_auth(
                attacker_connection,
                None,
                public_key(&client_key),
                attacker_nonce,
            )
            .expect("attacker auth challenge");
        let victim_challenge = fixture
            .registry
            .issue_auth(
                victim_connection,
                None,
                public_key(&client_key),
                victim_nonce,
            )
            .expect("victim auth challenge");
        let victim_proof = sign_payload(
            JwsType::AuthClient,
            &auth_proof(
                &fixture,
                &victim_challenge,
                victim_connection,
                None,
                &client_key,
                victim_nonce,
            ),
            &client_key,
        );

        assert!(matches!(
            fixture
                .registry
                .verify_and_consume_auth_client(attacker_connection, &victim_proof),
            Err(ChallengeError::BindingMismatch)
        ));
        assert_eq!(active_challenges(&fixture.registry), 1);

        fixture
            .registry
            .verify_and_consume_auth_client(victim_connection, &victim_proof)
            .expect("victim challenge was not cleared by another connection");
    }

    #[test]
    fn status_challenge_binds_the_requested_public_key() {
        let fixture = Fixture::new();
        let pairing_id = Id128::new([3; 16]);
        let expected_key = signing_key([8; 32]);
        let other_key = signing_key([9; 32]);
        let challenge = fixture
            .registry
            .issue_pair_status(pairing_id, public_key(&expected_key), Id128::new([1; 16]))
            .expect("issue status challenge");

        let wrong = sign_payload(
            JwsType::PairStatus,
            &pair_status(&challenge, pairing_id, &other_key),
            &other_key,
        );
        assert!(matches!(
            fixture.registry.verify_and_consume_pair_status(&wrong),
            Err(ChallengeError::BindingMismatch)
        ));

        let proof = sign_payload(
            JwsType::PairStatus,
            &pair_status(&challenge, pairing_id, &expected_key),
            &expected_key,
        );
        fixture
            .registry
            .verify_and_consume_pair_status(&proof)
            .expect("consume status proof");
    }

    #[test]
    fn expiry_uses_monotonic_time_and_wall_clock_rollback_does_not_extend() {
        let fixture = Fixture::new();
        let pairing_id = Id128::new([3; 16]);
        let client_key = signing_key([8; 32]);

        let first = fixture
            .registry
            .issue_pair_claim(pairing_id, Id128::new([1; 16]))
            .expect("first challenge");
        fixture.clock.set_wall_clock(1);
        fixture.clock.advance_seconds(29);
        let proof = sign_payload(
            JwsType::PairClaim,
            &pair_claim(&fixture, &first, pairing_id, &client_key),
            &client_key,
        );
        fixture
            .registry
            .verify_and_consume_pair_claim(&proof)
            .expect("wall rollback must not expire early");

        let second = fixture
            .registry
            .issue_pair_claim(pairing_id, Id128::new([2; 16]))
            .expect("second challenge");
        fixture.clock.advance_seconds(30);
        let proof = sign_payload(
            JwsType::PairClaim,
            &pair_claim(&fixture, &second, pairing_id, &client_key),
            &client_key,
        );
        assert!(matches!(
            fixture.registry.verify_and_consume_pair_claim(&proof),
            Err(ChallengeError::Expired)
        ));
        assert_eq!(active_challenges(&fixture.registry), 0);
    }

    #[test]
    fn auth_challenge_expires_at_fifteen_seconds() {
        let fixture = Fixture::new();
        let connection_id = Id128::new([5; 16]);
        let client_key = signing_key([8; 32]);
        let client_nonce = Nonce256::new([7; 32]);

        let first = fixture
            .registry
            .issue_auth(connection_id, None, public_key(&client_key), client_nonce)
            .expect("first auth challenge");
        fixture.clock.advance_seconds(14);
        let first_proof = sign_payload(
            JwsType::AuthClient,
            &auth_proof(
                &fixture,
                &first,
                connection_id,
                None,
                &client_key,
                client_nonce,
            ),
            &client_key,
        );
        fixture
            .registry
            .verify_and_consume_auth_client(connection_id, &first_proof)
            .expect("auth challenge is valid before fifteen seconds");

        let second = fixture
            .registry
            .issue_auth(connection_id, None, public_key(&client_key), client_nonce)
            .expect("second auth challenge");
        fixture.clock.advance_seconds(15);
        let second_proof = sign_payload(
            JwsType::AuthClient,
            &auth_proof(
                &fixture,
                &second,
                connection_id,
                None,
                &client_key,
                client_nonce,
            ),
            &client_key,
        );
        assert!(matches!(
            fixture
                .registry
                .verify_and_consume_auth_client(connection_id, &second_proof),
            Err(ChallengeError::Expired)
        ));
        assert_eq!(active_challenges(&fixture.registry), 0);
    }

    #[test]
    fn pair_claim_rejects_binding_mutations_without_consuming_the_challenge() {
        let fixture = Fixture::new();
        let pairing_id = Id128::new([4; 16]);
        let client_key = signing_key([8; 32]);
        let challenge = fixture
            .registry
            .issue_pair_claim(pairing_id, Id128::new([1; 16]))
            .expect("claim challenge");
        let base = pair_claim(&fixture, &challenge, pairing_id, &client_key);

        let mut wrong_origin = base.clone();
        wrong_origin.origin = "https://other.example.test".to_owned();
        let mut wrong_request = base.clone();
        wrong_request.request_id = Id128::new([9; 16]);
        let mut wrong_challenge = base.clone();
        wrong_challenge.challenge_id = Id128::new([9; 16]);
        let mut wrong_nonce = base.clone();
        wrong_nonce.nonce = Nonce256::new([9; 32]);
        let mut wrong_expiry = base.clone();
        wrong_expiry.exp =
            UnixSeconds::from_unix_seconds(base.exp.get() + 1).expect("safe mutated expiry");
        let mut wrong_version = base.clone();
        wrong_version.v = 2;

        for mutated in [
            wrong_origin,
            wrong_request,
            wrong_challenge,
            wrong_nonce,
            wrong_expiry,
            wrong_version,
        ] {
            let proof = sign_payload(JwsType::PairClaim, &mutated, &client_key);
            assert!(matches!(
                fixture.registry.verify_and_consume_pair_claim(&proof),
                Err(ChallengeError::BindingMismatch
                    | ChallengeError::NotFound
                    | ChallengeError::Jws(JwsError::InvalidPayload))
            ));
            assert_eq!(active_challenges(&fixture.registry), 1);
        }

        let proof = sign_payload(JwsType::PairClaim, &base, &client_key);
        fixture
            .registry
            .verify_and_consume_pair_claim(&proof)
            .expect("original binding remains usable");
    }

    #[test]
    fn random_source_failure_closes_issuance_without_state() {
        let fixture = Fixture::with_random(Arc::new(TestRandom::failing()));
        assert!(matches!(
            fixture
                .registry
                .issue_pair_claim(Id128::new([3; 16]), Id128::new([4; 16])),
            Err(ChallengeError::RandomUnavailable(_))
        ));
        assert_eq!(active_challenges(&fixture.registry), 0);
    }

    #[test]
    fn discarded_connection_challenge_cannot_be_consumed() {
        let fixture = Fixture::new();
        let connection_id = Id128::new([5; 16]);
        let client_key = signing_key([8; 32]);
        let challenge = fixture
            .registry
            .issue_auth(
                connection_id,
                None,
                public_key(&client_key),
                Nonce256::new([7; 32]),
            )
            .expect("issue auth challenge");
        fixture
            .registry
            .discard_scope(ChallengeScope::Auth(connection_id))
            .expect("discard scope");
        let proof = sign_payload(
            JwsType::AuthClient,
            &auth_proof(
                &fixture,
                &challenge,
                connection_id,
                None,
                &client_key,
                Nonce256::new([7; 32]),
            ),
            &client_key,
        );
        assert!(matches!(
            fixture
                .registry
                .verify_and_consume_auth_client(connection_id, &proof),
            Err(ChallengeError::NotFound)
        ));
    }

    #[test]
    fn node_key_is_not_used_as_a_pairing_claim_key() {
        let fixture = Fixture::new();
        let pairing_id = Id128::new([3; 16]);
        let challenge = fixture
            .registry
            .issue_pair_claim(pairing_id, Id128::new([1; 16]))
            .expect("issue challenge");
        let proof = sign_payload(
            JwsType::PairClaim,
            &pair_claim(&fixture, &challenge, pairing_id, &fixture.node_key),
            &signing_key([8; 32]),
        );
        assert!(matches!(
            fixture.registry.verify_and_consume_pair_claim(&proof),
            Err(ChallengeError::Jws(JwsError::InvalidSignature))
        ));
        assert_eq!(active_challenges(&fixture.registry), 1);
    }

    #[test]
    fn pair_status_value_is_fixed_to_protocol_tokens() {
        let value =
            serde_json::to_value(PairStatusValue::PendingConfirmation).expect("serialize status");
        assert_eq!(value, "pending_confirmation");
    }
}
