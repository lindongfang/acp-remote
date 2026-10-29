use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::{Mutex, watch};

use crate::{
    challenge::{ChallengeError, ChallengeRegistry, ChallengeScope, IssuedChallenge},
    claims::{
        ClaimResult, ClaimResultStatus, Id128, NodeId, PairClaim, PairStatusValue, PublicKeySec1,
        StatusResult, UnixSeconds, verify_pair_claim, verify_pair_status,
    },
    identity::NodeIdentity,
    jws::{JwsError, JwsType},
    trust::{TrustError, TrustStore, TrustedClientRecord},
};

pub const PAIRING_LIFETIME_SECONDS: u64 = 300;
const APPROVAL_RESULT_LINGER_SECONDS: u64 = 60;
const MAX_ACTIVE_PAIRINGS: usize = 8;
const CODE_BYTES: usize = 8;
const CODE_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionState {
    PendingClaim,
    PendingConfirmation,
    Approved,
    Rejected,
    Cancelled,
    Expired,
    Consumed,
}

impl SessionState {
    const fn is_pending(self) -> bool {
        matches!(self, Self::PendingClaim | Self::PendingConfirmation)
    }

    const fn is_approved(self) -> bool {
        matches!(self, Self::Approved | Self::Consumed)
    }

    const fn protocol_status(self) -> Option<PairStatusValue> {
        match self {
            Self::PendingConfirmation => Some(PairStatusValue::PendingConfirmation),
            Self::Approved => Some(PairStatusValue::Approved),
            Self::Rejected => Some(PairStatusValue::Rejected),
            Self::Cancelled => Some(PairStatusValue::Cancelled),
            Self::Expired => Some(PairStatusValue::Expired),
            Self::Consumed => Some(PairStatusValue::Consumed),
            Self::PendingClaim => None,
        }
    }
}

#[derive(Clone, Debug)]
struct BoundClient {
    public_key: PublicKeySec1,
    display_name: String,
    claim_request_id: Id128,
    claim_compact: String,
    claim_host_proof: String,
    client_id: Option<Id128>,
}

#[derive(Debug)]
struct Session {
    pairing_id: Id128,
    code: String,
    state: SessionState,
    expires_at: UnixSeconds,
    deadline: Instant,
    retention_deadline: Instant,
    result_expires_at: UnixSeconds,
    bound: Option<BoundClient>,
    notify: watch::Sender<PairingEvent>,
}

#[derive(Default)]
struct Registry {
    sessions: HashMap<Id128, Session>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingEvent {
    pub v: u8,
    pub event: String,
    pub pairing_id: String,
    pub expires_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

impl PairingEvent {
    fn from_session(session: &Session, event: &str) -> Self {
        let bound = session.bound.as_ref();
        Self {
            v: 1,
            event: event.to_owned(),
            pairing_id: session.pairing_id.to_base64url(),
            expires_at: session.result_expires_at.get(),
            code: None,
            display_name: bound.map(|bound| bound.display_name.clone()),
            fingerprint: bound
                .map(|bound| hex::encode(Sha256::digest(bound.public_key.as_bytes()))),
            client_id: bound
                .and_then(|bound| bound.client_id)
                .map(Id128::to_base64url),
        }
    }

    fn started(pairing_id: Id128, code: &str, expires_at: u64) -> Self {
        Self {
            v: 1,
            event: "started".to_owned(),
            pairing_id: pairing_id.to_base64url(),
            expires_at,
            code: Some(format_pairing_code(code)),
            display_name: None,
            fingerprint: None,
            client_id: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingTicket {
    pub pairing_id: Id128,
    pub code: String,
    pub expires_at: u64,
    pub deadline: Instant,
}

#[derive(Clone, Debug)]
pub struct PairingLookup {
    pub pairing_id: Id128,
    pub expires_at: u64,
    pub challenge: IssuedChallenge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimResponse {
    pub host_proof: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusResponse {
    pub host_proof: String,
}

#[derive(Debug, Error)]
pub enum PairingError {
    #[error("pairing is invalid or no longer available")]
    PairingInvalid,
    #[error("pairing request identifier was reused with different content")]
    RequestConflict,
    #[error("pairing capacity is exhausted")]
    CapacityExhausted,
    #[error("pairing randomness is unavailable")]
    RandomUnavailable,
    #[error("pairing clock is unavailable")]
    ClockUnavailable,
    #[error("pairing storage is unavailable")]
    StorageUnavailable,
    #[error("pairing was already approved; revoke the client instead")]
    AlreadyApproved,
    #[error(transparent)]
    Trust(#[from] TrustError),
    #[error(transparent)]
    Jws(#[from] JwsError),
    #[error(transparent)]
    Challenge(#[from] ChallengeError),
}

pub trait PairingClock: Send + Sync {
    /// Returns the current UTC Unix timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error when the system clock is unavailable.
    fn unix_seconds(&self) -> Result<u64, String>;

    /// Returns the monotonic instant used for local expiry.
    ///
    /// # Errors
    ///
    /// Returns an error when the monotonic clock is unavailable.
    fn monotonic_now(&self) -> Result<Instant, String>;
}

#[derive(Debug)]
struct SystemClock;

impl PairingClock for SystemClock {
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

pub trait PairingRandom: Send + Sync {
    /// Fills the complete output buffer or returns an error.
    ///
    /// # Errors
    ///
    /// Returns an error when cryptographic randomness is unavailable.
    fn fill(&self, output: &mut [u8]) -> Result<(), String>;
}

#[derive(Debug)]
struct SystemRandom;

impl PairingRandom for SystemRandom {
    fn fill(&self, output: &mut [u8]) -> Result<(), String> {
        getrandom::fill(output).map_err(|error| error.to_string())
    }
}

pub struct PairingService {
    identity: Arc<NodeIdentity>,
    challenges: Arc<ChallengeRegistry>,
    trust: TrustStore,
    clock: Arc<dyn PairingClock>,
    random: Arc<dyn PairingRandom>,
    registry: Mutex<Registry>,
}

impl PairingService {
    /// Creates the in-memory pairing state machine for one node.
    ///
    /// # Errors
    ///
    /// Returns an error when the node identity or canonical origin is invalid.
    pub fn new(
        identity: Arc<NodeIdentity>,
        origin: String,
        trust: TrustStore,
    ) -> Result<Self, PairingError> {
        Self::with_sources(
            identity,
            origin,
            trust,
            Arc::new(SystemClock),
            Arc::new(SystemRandom),
        )
    }

    /// Creates a pairing state machine with injectable clock and randomness sources.
    ///
    /// # Errors
    ///
    /// Returns an error when the node identity or canonical origin is invalid.
    pub fn with_sources(
        identity: Arc<NodeIdentity>,
        origin: String,
        trust: TrustStore,
        clock: Arc<dyn PairingClock>,
        random: Arc<dyn PairingRandom>,
    ) -> Result<Self, PairingError> {
        let public_key_bytes: [u8; 65] = identity
            .public_key()
            .try_into()
            .map_err(|_| PairingError::PairingInvalid)?;
        let public_key = PublicKeySec1::new(public_key_bytes)?;
        let node_id = NodeId::from_public_key(&public_key)?;
        if node_id.to_hex() != identity.node_id() {
            return Err(PairingError::PairingInvalid);
        }
        let challenges = ChallengeRegistry::new(node_id, public_key, origin)?;
        Ok(Self {
            identity,
            challenges: Arc::new(challenges),
            trust,
            clock,
            random,
            registry: Mutex::new(Registry::default()),
        })
    }

    /// Creates one bounded local pairing request.
    ///
    /// # Errors
    ///
    /// Returns an error when randomness or time is unavailable or eight pairings are active.
    pub async fn create_pairing(
        &self,
    ) -> Result<(PairingTicket, watch::Receiver<PairingEvent>), PairingError> {
        let now_unix = self.unix_seconds()?;
        let now = self.monotonic_now()?;
        let deadline = now
            .checked_add(Duration::from_secs(PAIRING_LIFETIME_SECONDS))
            .ok_or(PairingError::ClockUnavailable)?;
        let expires_at =
            UnixSeconds::from_unix_seconds(now_unix)?.checked_add(PAIRING_LIFETIME_SECONDS)?;
        let mut registry = self.registry.lock().await;
        prune_registry(&mut registry, now, now_unix)?;
        if registry.sessions.len() >= MAX_ACTIVE_PAIRINGS {
            return Err(PairingError::CapacityExhausted);
        }

        let pairing_id = self.new_pairing_id(&registry)?;
        let code = self.new_pairing_code(&registry)?;
        let initial_event = PairingEvent::started(pairing_id, &code, expires_at.get());
        let (notify, receiver) = watch::channel(initial_event);
        registry.sessions.insert(
            pairing_id,
            Session {
                pairing_id,
                code: code.clone(),
                state: SessionState::PendingClaim,
                expires_at,
                deadline,
                retention_deadline: deadline,
                result_expires_at: expires_at,
                bound: None,
                notify,
            },
        );
        drop(registry);
        Ok((
            PairingTicket {
                pairing_id,
                code,
                expires_at: expires_at.get(),
                deadline,
            },
            receiver,
        ))
    }

    /// Looks up a pairing code and issues the single current claim challenge.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, unknown, expired, or already claimed pairings.
    pub async fn lookup(
        &self,
        code: &str,
        request_id: Id128,
    ) -> Result<PairingLookup, PairingError> {
        let code = normalize_pairing_code(code)?;
        let now_unix = self.unix_seconds()?;
        let now = self.monotonic_now()?;
        let mut registry = self.registry.lock().await;
        prune_registry(&mut registry, now, now_unix)?;
        let session = registry
            .sessions
            .values()
            .find(|session| session.state == SessionState::PendingClaim && session.code == code)
            .ok_or(PairingError::PairingInvalid)?;
        let pairing_id = session.pairing_id;
        let expires_at = session.result_expires_at.get();
        let challenge = self.challenges.issue_pair_claim(pairing_id, request_id)?;
        drop(registry);
        Ok(PairingLookup {
            pairing_id,
            expires_at,
            challenge,
        })
    }

    /// Verifies a claim proof and atomically binds its client key to the pairing.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid proof or challenge, a conflicting retry, an already
    /// registered public key, or unavailable storage.
    pub async fn claim(&self, compact: &str) -> Result<ClaimResponse, PairingError> {
        let claim = verify_pair_claim(compact)?;
        let now_unix = self.unix_seconds()?;
        let now = self.monotonic_now()?;
        let mut registry = self.registry.lock().await;
        prune_registry(&mut registry, now, now_unix)?;
        let session = registry
            .sessions
            .get(&claim.pairing_id)
            .ok_or(PairingError::PairingInvalid)?;
        if let Some(bound) = &session.bound {
            if session.state != SessionState::PendingConfirmation {
                return Err(PairingError::PairingInvalid);
            }
            if bound.claim_request_id == claim.request_id {
                if bound.claim_compact == compact {
                    return Ok(ClaimResponse {
                        host_proof: bound.claim_host_proof.clone(),
                    });
                }
                return Err(PairingError::RequestConflict);
            }
            return Err(PairingError::PairingInvalid);
        }
        if session.state != SessionState::PendingClaim {
            return Err(PairingError::PairingInvalid);
        }
        if self
            .trust
            .client_by_public_key(claim.client_public_key.as_bytes())
            .await?
            .is_some()
        {
            return Err(PairingError::PairingInvalid);
        }
        self.challenges.verify_and_consume_pair_claim(compact)?;
        let host_proof = self.sign_claim_result(&claim)?;
        let event = {
            let session = registry
                .sessions
                .get_mut(&claim.pairing_id)
                .ok_or(PairingError::PairingInvalid)?;
            session.state = SessionState::PendingConfirmation;
            session.bound = Some(BoundClient {
                public_key: claim.client_public_key,
                display_name: claim.display_name.clone(),
                claim_request_id: claim.request_id,
                claim_compact: compact.to_owned(),
                claim_host_proof: host_proof.clone(),
                client_id: None,
            });
            PairingEvent::from_session(session, "pending_confirmation")
        };
        let _ = registry
            .sessions
            .get(&claim.pairing_id)
            .ok_or(PairingError::PairingInvalid)?
            .notify
            .send(event);
        drop(registry);
        Ok(ClaimResponse { host_proof })
    }

    /// Issues the single current status challenge for the claimed key.
    ///
    /// # Errors
    ///
    /// Returns an error when the pairing is unknown, has no bound key, or the key does not match.
    pub async fn status_challenge(
        &self,
        pairing_id: Id128,
        client_public_key: PublicKeySec1,
        request_id: Id128,
    ) -> Result<IssuedChallenge, PairingError> {
        let now_unix = self.unix_seconds()?;
        let now = self.monotonic_now()?;
        let mut registry = self.registry.lock().await;
        prune_registry(&mut registry, now, now_unix)?;
        let session = registry
            .sessions
            .get(&pairing_id)
            .ok_or(PairingError::PairingInvalid)?;
        let bound = session.bound.as_ref().ok_or(PairingError::PairingInvalid)?;
        if bound.public_key != client_public_key {
            return Err(PairingError::PairingInvalid);
        }
        let challenge =
            self.challenges
                .issue_pair_status(pairing_id, client_public_key, request_id)?;
        drop(registry);
        Ok(challenge)
    }

    /// Verifies a status proof and returns the signed current result.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid proof or binding, or an unknown/expired pairing.
    pub async fn status(&self, compact: &str) -> Result<StatusResponse, PairingError> {
        let proof = verify_pair_status(compact)?;
        let now_unix = self.unix_seconds()?;
        let now = self.monotonic_now()?;
        let mut registry = self.registry.lock().await;
        prune_registry(&mut registry, now, now_unix)?;
        let session = registry
            .sessions
            .get(&proof.pairing_id)
            .ok_or(PairingError::PairingInvalid)?;
        let bound = session.bound.as_ref().ok_or(PairingError::PairingInvalid)?;
        if bound.public_key != proof.client_public_key {
            return Err(PairingError::PairingInvalid);
        }
        let challenge = self.challenges.verify_and_consume_pair_status(compact)?;
        let status = session
            .state
            .protocol_status()
            .ok_or(PairingError::PairingInvalid)?;
        let client_id = bound.client_id;
        let result_expires_at = session.result_expires_at;
        drop(registry);
        let result = StatusResult {
            v: 1,
            purpose: crate::claims::StatusPurpose::StatusResult,
            node_id: challenge.node_id,
            origin: challenge.origin,
            pairing_id: proof.pairing_id,
            request_id: proof.request_id,
            client_public_key: proof.client_public_key,
            status,
            client_id,
            expires_at: result_expires_at,
        };
        let payload = serde_json::to_vec(&result).map_err(|_| PairingError::StorageUnavailable)?;
        let host_proof = self.identity.sign_jws(JwsType::HostStatus, &payload)?;
        Ok(StatusResponse { host_proof })
    }

    /// Persists trust for the pending client and only then reports approval success.
    ///
    /// # Errors
    ///
    /// Returns an error when the pairing is not pending, the public key was concurrently
    /// registered, randomness is unavailable, or the trust transaction fails.
    pub async fn approve(&self, pairing_id: Id128) -> Result<TrustedClientRecord, PairingError> {
        let now_unix = self.unix_seconds()?;
        let now = self.monotonic_now()?;
        let mut registry = self.registry.lock().await;
        prune_registry(&mut registry, now, now_unix)?;
        let session = registry
            .sessions
            .get(&pairing_id)
            .ok_or(PairingError::PairingInvalid)?;
        if session.state == SessionState::Approved || session.state == SessionState::Consumed {
            return Err(PairingError::AlreadyApproved);
        }
        if session.state != SessionState::PendingConfirmation {
            return Err(PairingError::PairingInvalid);
        }
        let bound = session.bound.as_ref().ok_or(PairingError::PairingInvalid)?;
        let approval_expiry = UnixSeconds::from_unix_seconds(now_unix)?
            .checked_add(APPROVAL_RESULT_LINGER_SECONDS)?;
        let result_expires_at = std::cmp::max(session.expires_at, approval_expiry);
        let retention_deadline = std::cmp::max(
            session.deadline,
            now.checked_add(Duration::from_secs(APPROVAL_RESULT_LINGER_SECONDS))
                .ok_or(PairingError::ClockUnavailable)?,
        );
        let (client, client_id) = match self
            .trust
            .client_by_public_key(bound.public_key.as_bytes())
            .await?
        {
            Some(_) => return Err(PairingError::PairingInvalid),
            None => self.register_new_client(bound).await?,
        };
        let event = {
            let session = registry
                .sessions
                .get_mut(&pairing_id)
                .ok_or(PairingError::PairingInvalid)?;
            session.state = SessionState::Approved;
            session.retention_deadline = retention_deadline;
            session.result_expires_at = result_expires_at;
            if let Some(bound) = &mut session.bound {
                bound.client_id = Some(client_id);
            }
            PairingEvent::from_session(session, "approved")
        };
        let _ = registry
            .sessions
            .get(&pairing_id)
            .ok_or(PairingError::PairingInvalid)?
            .notify
            .send(event);
        drop(registry);
        let _ = self.challenges.discard_pairing(pairing_id);
        Ok(client)
    }

    /// Rejects one pending pairing.
    ///
    /// # Errors
    ///
    /// Returns an error when the pairing is no longer pending.
    pub async fn reject(&self, pairing_id: Id128) -> Result<(), PairingError> {
        self.finish_pending(pairing_id, SessionState::Rejected, "rejected")
            .await
    }

    /// Cancels one pending pairing or treats an approved pairing as already committed.
    ///
    /// # Errors
    ///
    /// Returns an error when cancellation loses the approval linearization point.
    pub async fn cancel(&self, pairing_id: Id128) -> Result<(), PairingError> {
        let now_unix = self.unix_seconds()?;
        let now = self.monotonic_now()?;
        let mut registry = self.registry.lock().await;
        prune_registry(&mut registry, now, now_unix)?;
        let session = registry
            .sessions
            .get_mut(&pairing_id)
            .ok_or(PairingError::PairingInvalid)?;
        if session.state.is_approved() {
            return Err(PairingError::AlreadyApproved);
        }
        if !session.state.is_pending() {
            return Err(PairingError::PairingInvalid);
        }
        session.state = SessionState::Cancelled;
        let event = PairingEvent::from_session(session, "cancelled");
        let _ = session.notify.send(event);
        drop(registry);
        let _ = self.challenges.discard_pairing(pairing_id);
        Ok(())
    }

    /// Cancels all pending pairings when the node starts shutting down.
    pub async fn clear(&self) {
        let mut registry = self.registry.lock().await;
        for session in registry.sessions.values_mut() {
            if session.state.is_pending() {
                session.state = SessionState::Cancelled;
                session.retention_deadline = Instant::now();
                let event = PairingEvent::from_session(session, "cancelled");
                let _ = session.notify.send(event);
            }
        }
        registry.sessions.clear();
        drop(registry);
        let _ = self.challenges.clear();
    }

    /// Expires a pairing whose local deadline has elapsed.
    ///
    /// # Errors
    ///
    /// Returns an error when the clock is unavailable.
    pub async fn expire_if_due(&self, pairing_id: Id128) -> Result<bool, PairingError> {
        let now_unix = self.unix_seconds()?;
        let now = self.monotonic_now()?;
        let mut registry = self.registry.lock().await;
        let Some(session) = registry.sessions.get_mut(&pairing_id) else {
            return Ok(false);
        };
        if !session.state.is_pending() || now < session.deadline {
            return Ok(false);
        }
        session.state = SessionState::Expired;
        session.result_expires_at = UnixSeconds::from_unix_seconds(now_unix)?;
        session.retention_deadline = now;
        let event = PairingEvent::from_session(session, "expired");
        let _ = session.notify.send(event);
        drop(registry);
        let _ = self.challenges.discard_pairing(pairing_id);
        Ok(true)
    }

    /// Marks the matching approved result consumed after the first successful management auth.
    pub async fn mark_consumed_for_client(&self, client_id: &str) {
        let Ok(client_id) = Id128::parse_base64url(client_id) else {
            return;
        };
        let mut registry = self.registry.lock().await;
        for session in registry.sessions.values_mut() {
            if session.state == SessionState::Approved
                && session.bound.as_ref().and_then(|bound| bound.client_id) == Some(client_id)
            {
                session.state = SessionState::Consumed;
            }
        }
    }

    pub(crate) fn issue_auth_challenge(
        &self,
        connection_id: Id128,
        client_id: Option<Id128>,
        client_public_key: PublicKeySec1,
        client_nonce: crate::claims::Nonce256,
    ) -> Result<IssuedChallenge, PairingError> {
        Ok(self
            .challenges
            .issue_auth(connection_id, client_id, client_public_key, client_nonce)?)
    }

    pub(crate) fn verify_auth_proof(
        &self,
        connection_id: Id128,
        compact: &str,
    ) -> Result<crate::claims::AuthProof, PairingError> {
        Ok(self
            .challenges
            .verify_and_consume_auth_client(connection_id, compact)?)
    }

    pub(crate) fn discard_auth_challenge(&self, connection_id: Id128) {
        let _ = self
            .challenges
            .discard_scope(ChallengeScope::Auth(connection_id));
    }

    #[cfg(test)]
    pub(crate) fn active_auth_challenge_count(&self) -> usize {
        self.challenges.active_auth_count()
    }

    fn sign_claim_result(&self, claim: &PairClaim) -> Result<String, PairingError> {
        let result = ClaimResult {
            v: 1,
            purpose: crate::claims::ClaimPurpose::ClaimResult,
            node_id: claim.node_id,
            origin: claim.origin.clone(),
            pairing_id: claim.pairing_id,
            request_id: claim.request_id,
            client_public_key: claim.client_public_key,
            status: ClaimResultStatus::PendingConfirmation,
            expires_at: claim.exp,
        };
        let payload = serde_json::to_vec(&result).map_err(|_| PairingError::StorageUnavailable)?;
        Ok(self.identity.sign_jws(JwsType::HostClaim, &payload)?)
    }

    async fn register_new_client(
        &self,
        bound: &BoundClient,
    ) -> Result<(TrustedClientRecord, Id128), PairingError> {
        for _ in 0..4 {
            let client_id = self.random_id()?;
            let encoded_client_id = client_id.to_base64url();
            match self
                .trust
                .register_client(
                    &encoded_client_id,
                    bound.public_key.as_bytes(),
                    &bound.display_name,
                )
                .await
            {
                Ok(client) => return Ok((client, client_id)),
                Err(TrustError::ClientIdCollision) => {}
                Err(TrustError::PublicKeyAlreadyRegistered) => {
                    return Err(PairingError::PairingInvalid);
                }
                Err(error) => return Err(PairingError::Trust(error)),
            }
        }
        Err(PairingError::StorageUnavailable)
    }

    async fn finish_pending(
        &self,
        pairing_id: Id128,
        state: SessionState,
        event: &str,
    ) -> Result<(), PairingError> {
        let now_unix = self.unix_seconds()?;
        let now = self.monotonic_now()?;
        let mut registry = self.registry.lock().await;
        prune_registry(&mut registry, now, now_unix)?;
        let session = registry
            .sessions
            .get_mut(&pairing_id)
            .ok_or(PairingError::PairingInvalid)?;
        if session.state.is_approved() {
            return Err(PairingError::AlreadyApproved);
        }
        if !session.state.is_pending() {
            return Err(PairingError::PairingInvalid);
        }
        session.state = state;
        let notification = PairingEvent::from_session(session, event);
        let _ = session.notify.send(notification);
        drop(registry);
        let _ = self.challenges.discard_pairing(pairing_id);
        Ok(())
    }

    fn new_pairing_id(&self, registry: &Registry) -> Result<Id128, PairingError> {
        for _ in 0..16 {
            let pairing_id = self.random_id()?;
            if !registry.sessions.contains_key(&pairing_id) {
                return Ok(pairing_id);
            }
        }
        Err(PairingError::RandomUnavailable)
    }

    fn random_id(&self) -> Result<Id128, PairingError> {
        let mut bytes = [0_u8; 16];
        self.random
            .fill(&mut bytes)
            .map_err(|_| PairingError::RandomUnavailable)?;
        Ok(Id128::new(bytes))
    }

    fn new_pairing_code(&self, registry: &Registry) -> Result<String, PairingError> {
        for _ in 0..32 {
            let mut bytes = [0_u8; CODE_BYTES];
            self.random
                .fill(&mut bytes)
                .map_err(|_| PairingError::RandomUnavailable)?;
            let code: String = bytes
                .iter()
                .map(|byte| char::from(CODE_ALPHABET[usize::from(byte & 0x1f)]))
                .collect();
            if !registry
                .sessions
                .values()
                .any(|session| session.code == code)
            {
                return Ok(code);
            }
        }
        Err(PairingError::RandomUnavailable)
    }

    fn unix_seconds(&self) -> Result<u64, PairingError> {
        self.clock
            .unix_seconds()
            .map_err(|_| PairingError::ClockUnavailable)
    }

    fn monotonic_now(&self) -> Result<Instant, PairingError> {
        self.clock
            .monotonic_now()
            .map_err(|_| PairingError::ClockUnavailable)
    }
}

fn prune_registry(
    registry: &mut Registry,
    now: Instant,
    now_unix: u64,
) -> Result<(), PairingError> {
    for session in registry.sessions.values_mut() {
        if session.state.is_pending() && now >= session.deadline {
            session.state = SessionState::Expired;
            session.result_expires_at = UnixSeconds::from_unix_seconds(now_unix)?;
            session.retention_deadline = now;
            let event = PairingEvent::from_session(session, "expired");
            let _ = session.notify.send(event);
        }
    }
    registry
        .sessions
        .retain(|_, session| now < session.retention_deadline);
    Ok(())
}

/// Normalizes the exact Crockford alphabet, case folding, and one middle hyphen.
///
/// # Errors
///
/// Returns an error for any other length, separator, or alphabet character.
pub fn normalize_pairing_code(input: &str) -> Result<String, PairingError> {
    let uppercase = input.to_ascii_uppercase();
    let bytes = uppercase.as_bytes();
    let (left, right) = match bytes.len() {
        8 => bytes.split_at(4),
        9 if bytes.get(4) == Some(&b'-') => (&bytes[..4], &bytes[5..]),
        _ => return Err(PairingError::PairingInvalid),
    };
    let mut compact = [0_u8; 8];
    compact[..4].copy_from_slice(left);
    compact[4..].copy_from_slice(right);
    if !compact.iter().all(|byte| CODE_ALPHABET.contains(byte)) {
        return Err(PairingError::PairingInvalid);
    }
    String::from_utf8(compact.to_vec()).map_err(|_| PairingError::PairingInvalid)
}

#[must_use]
pub fn format_pairing_code(code: &str) -> String {
    code.get(..4)
        .zip(code.get(4..))
        .filter(|(left, right)| code.len() == 8 && left.len() == 4 && right.len() == 4)
        .map_or_else(
            || code.to_owned(),
            |(left, right)| format!("{left}-{right}"),
        )
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{
            Mutex,
            atomic::{AtomicU8, Ordering},
        },
    };

    use p256::ecdsa::SigningKey;

    use super::*;
    use crate::{
        claims::{
            PairStatus, PublicKeySec1, StatusPurpose, verify_claim_result, verify_status_result,
        },
        jws::{JwsType, sign},
        storage::Storage,
    };

    struct ClockState {
        unix_seconds: u64,
        monotonic: Instant,
    }

    struct TestClock {
        state: Mutex<ClockState>,
    }

    impl TestClock {
        fn new() -> Self {
            Self {
                state: Mutex::new(ClockState {
                    unix_seconds: 1_700_000_000,
                    monotonic: Instant::now(),
                }),
            }
        }

        fn advance(&self, seconds: u64) {
            let mut state = self.state.lock().expect("test clock lock");
            state.unix_seconds += seconds;
            state.monotonic += Duration::from_secs(seconds);
        }
    }

    impl PairingClock for TestClock {
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

    struct TestRandom {
        next: AtomicU8,
    }

    impl TestRandom {
        fn new() -> Self {
            Self {
                next: AtomicU8::new(1),
            }
        }
    }

    impl PairingRandom for TestRandom {
        fn fill(&self, output: &mut [u8]) -> Result<(), String> {
            output.fill(self.next.fetch_add(1, Ordering::Relaxed).max(1));
            Ok(())
        }
    }

    struct Fixture {
        service: PairingService,
        identity: Arc<NodeIdentity>,
        clock: Arc<TestClock>,
        storage: Storage,
        directory: PathBuf,
    }

    impl Fixture {
        async fn new(label: &str) -> Self {
            Self::with_random(label, Arc::new(TestRandom::new())).await
        }

        async fn with_random(label: &str, random: Arc<dyn PairingRandom>) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock")
                .as_nanos();
            let directory = std::env::temp_dir().join(format!(
                "acp-remote-pairing-{label}-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir(&directory).expect("test directory");
            let storage = Storage::open(&directory.join("node.db"))
                .await
                .expect("test storage");
            let identity = Arc::new(NodeIdentity::for_test([7_u8; 32]));
            let clock = Arc::new(TestClock::new());
            let service = PairingService::with_sources(
                identity.clone(),
                "https://node.example.test".to_owned(),
                storage.trust_store(),
                clock.clone(),
                random,
            )
            .expect("pairing service");
            Self {
                service,
                identity,
                clock,
                storage,
                directory,
            }
        }

        fn node_public_key(&self) -> PublicKeySec1 {
            PublicKeySec1::new(
                self.identity
                    .public_key()
                    .try_into()
                    .expect("node SEC1 public key"),
            )
            .expect("valid node public key")
        }

        async fn close(self) {
            let Self {
                service,
                storage,
                directory,
                ..
            } = self;
            drop(service);
            storage.close().await;
            std::fs::remove_dir_all(directory).expect("test cleanup");
        }
    }

    fn signing_key(bytes: [u8; 32]) -> SigningKey {
        SigningKey::from_slice(&bytes).expect("test scalar")
    }

    fn public_key(key: &SigningKey) -> PublicKeySec1 {
        PublicKeySec1::new(
            key.verifying_key()
                .to_sec1_point(false)
                .as_bytes()
                .try_into()
                .expect("65-byte SEC1 key"),
        )
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

    fn claim_proof(
        challenge: &IssuedChallenge,
        pairing_id: Id128,
        client_key: &SigningKey,
        display_name: &str,
    ) -> String {
        let claim = PairClaim {
            v: 1,
            node_id: challenge.node_id,
            origin: challenge.origin.clone(),
            request_id: challenge.request_id.expect("claim request id"),
            challenge_id: challenge.challenge_id,
            nonce: challenge.nonce,
            iat: challenge.iat,
            exp: challenge.exp,
            pairing_id,
            client_public_key: public_key(client_key),
            display_name: display_name.to_owned(),
        };
        sign_payload(JwsType::PairClaim, &claim, client_key)
    }

    fn status_proof(
        challenge: &IssuedChallenge,
        pairing_id: Id128,
        client_key: &SigningKey,
    ) -> String {
        let status = PairStatus {
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
        };
        sign_payload(JwsType::PairStatus, &status, client_key)
    }

    #[test]
    fn normalizes_only_exact_crockford_alphabet_with_optional_middle_hyphen() {
        assert_eq!(
            normalize_pairing_code("abcd-efgh").expect("hyphenated code"),
            "ABCDEFGH"
        );
        assert_eq!(
            normalize_pairing_code("abcdefgh").expect("compact code"),
            "ABCDEFGH"
        );
        for invalid in [
            "ABCD-EFG",
            "ABCD--EFG",
            "-BCD-EFGH",
            "ABCD-EFGI",
            "ABCD-EFGO",
            "ABCD EFGH",
        ] {
            assert!(
                matches!(
                    normalize_pairing_code(invalid),
                    Err(PairingError::PairingInvalid)
                ),
                "{invalid}"
            );
        }
        assert_eq!(format_pairing_code("ABCDEFGH"), "ABCD-EFGH");
    }

    #[tokio::test]
    async fn pairing_lifetime_is_three_hundred_seconds_and_cannot_be_renewed() {
        let fixture = Fixture::new("expiry").await;
        let (ticket, _events) = fixture.service.create_pairing().await.expect("pairing");
        assert_eq!(ticket.expires_at, 1_700_000_300);

        fixture.clock.advance(299);
        let lookup = fixture
            .service
            .lookup(&ticket.code, Id128::new([1; 16]))
            .await
            .expect("lookup just before expiry");
        assert_eq!(lookup.expires_at, ticket.expires_at);

        fixture.clock.advance(1);
        assert!(matches!(
            fixture
                .service
                .lookup(&ticket.code, Id128::new([2; 16]))
                .await,
            Err(PairingError::PairingInvalid)
        ));
        fixture.close().await;
    }

    struct CollisionRandom {
        ids: TestRandom,
        codes: AtomicU8,
        recover_after_collision: bool,
    }

    impl PairingRandom for CollisionRandom {
        fn fill(&self, output: &mut [u8]) -> Result<(), String> {
            if output.len() == CODE_BYTES {
                let call = self.codes.fetch_add(1, Ordering::Relaxed);
                output.fill(if self.recover_after_collision && call >= 2 {
                    2
                } else {
                    1
                });
                Ok(())
            } else {
                self.ids.fill(output)
            }
        }
    }

    #[tokio::test]
    async fn code_collision_retries_without_replacing_the_first_pairing() {
        let random = Arc::new(CollisionRandom {
            ids: TestRandom::new(),
            codes: AtomicU8::new(0),
            recover_after_collision: true,
        });
        let fixture = Fixture::with_random("collision-retry", random.clone()).await;
        let (first, _) = fixture
            .service
            .create_pairing()
            .await
            .expect("first pairing");
        let (second, _) = fixture
            .service
            .create_pairing()
            .await
            .expect("collision retry");
        assert_ne!(first.code, second.code);
        assert_ne!(first.pairing_id, second.pairing_id);
        assert_eq!(random.codes.load(Ordering::Relaxed), 3);
        let found = fixture
            .service
            .lookup(&first.code, Id128::new([70; 16]))
            .await
            .expect("first code remains usable");
        assert_eq!(found.pairing_id, first.pairing_id);
        fixture.close().await;
    }

    #[tokio::test]
    async fn persistent_code_collision_fails_closed_with_bounded_retries() {
        let random = Arc::new(CollisionRandom {
            ids: TestRandom::new(),
            codes: AtomicU8::new(0),
            recover_after_collision: false,
        });
        let fixture = Fixture::with_random("collision-limit", random.clone()).await;
        let (first, _) = fixture
            .service
            .create_pairing()
            .await
            .expect("first pairing");
        assert!(matches!(
            fixture.service.create_pairing().await,
            Err(PairingError::RandomUnavailable)
        ));
        assert_eq!(random.codes.load(Ordering::Relaxed), 33);
        assert_eq!(fixture.service.registry.lock().await.sessions.len(), 1);
        let found = fixture
            .service
            .lookup(&first.code, Id128::new([71; 16]))
            .await
            .expect("first code remains usable");
        assert_eq!(found.pairing_id, first.pairing_id);
        fixture.close().await;
    }

    #[tokio::test]
    async fn rejects_the_ninth_active_pairing() {
        let fixture = Fixture::new("capacity").await;
        for _ in 0..MAX_ACTIVE_PAIRINGS {
            fixture
                .service
                .create_pairing()
                .await
                .expect("pairing within capacity");
        }
        assert!(matches!(
            fixture.service.create_pairing().await,
            Err(PairingError::CapacityExhausted)
        ));
        fixture.close().await;
    }

    #[tokio::test]
    async fn claim_retries_are_idempotent_and_conflicting_content_is_rejected() {
        let fixture = Fixture::new("claim").await;
        let (ticket, _events) = fixture.service.create_pairing().await.expect("pairing");
        let request_id = Id128::new([3; 16]);
        let lookup = fixture
            .service
            .lookup(&ticket.code, request_id)
            .await
            .expect("lookup");
        let client_key = signing_key([8; 32]);
        let proof = claim_proof(&lookup.challenge, ticket.pairing_id, &client_key, "Alice");

        let first = fixture.service.claim(&proof).await.expect("first claim");
        let repeated = fixture
            .service
            .claim(&proof)
            .await
            .expect("idempotent claim");
        assert_eq!(first.host_proof, repeated.host_proof);
        let result = verify_claim_result(&first.host_proof, &fixture.node_public_key())
            .expect("host claim proof");
        assert_eq!(result.pairing_id, ticket.pairing_id);
        assert_eq!(result.status, ClaimResultStatus::PendingConfirmation);

        let conflicting = claim_proof(&lookup.challenge, ticket.pairing_id, &client_key, "Mallory");
        assert!(matches!(
            fixture.service.claim(&conflicting).await,
            Err(PairingError::RequestConflict)
        ));
        fixture.close().await;
    }

    #[tokio::test]
    async fn claim_retries_stop_at_every_terminal_state() {
        for terminal in ["cancelled", "rejected", "expired", "approved", "consumed"] {
            let fixture = Fixture::new(terminal).await;
            let (ticket, _) = fixture.service.create_pairing().await.expect("pair");
            let key = signing_key([0x61; 32]);
            let lookup = fixture
                .service
                .lookup(&ticket.code, Id128::new([0x62; 16]))
                .await
                .expect("lookup");
            let proof = claim_proof(&lookup.challenge, ticket.pairing_id, &key, "Retry fixture");
            let first = fixture.service.claim(&proof).await.expect("claim");
            assert_eq!(
                fixture.service.claim(&proof).await.expect("pending retry"),
                first
            );
            match terminal {
                "cancelled" => fixture
                    .service
                    .cancel(ticket.pairing_id)
                    .await
                    .expect("cancel"),
                "rejected" => fixture
                    .service
                    .reject(ticket.pairing_id)
                    .await
                    .expect("reject"),
                "expired" => fixture.clock.advance(300),
                _ => {
                    let client = fixture
                        .service
                        .approve(ticket.pairing_id)
                        .await
                        .expect("approve");
                    if terminal == "consumed" {
                        fixture
                            .service
                            .mark_consumed_for_client(&client.client_id)
                            .await;
                    }
                }
            }
            assert!(
                matches!(
                    fixture.service.claim(&proof).await,
                    Err(PairingError::PairingInvalid)
                ),
                "{terminal}"
            );
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn natural_expiry_rejects_old_material_and_fresh_status_without_trust() {
        let fixture = Fixture::new("natural-expiry").await;
        let (ticket, _) = fixture.service.create_pairing().await.expect("pair");
        let key = signing_key([0x63; 32]);
        let lookup = fixture
            .service
            .lookup(&ticket.code, Id128::new([0x64; 16]))
            .await
            .expect("lookup");
        let claim = claim_proof(&lookup.challenge, ticket.pairing_id, &key, "Expiry fixture");
        fixture.service.claim(&claim).await.expect("claim");
        fixture.clock.advance(299);
        let challenge = fixture
            .service
            .status_challenge(ticket.pairing_id, public_key(&key), Id128::new([0x65; 16]))
            .await
            .expect("fresh status before boundary");
        let proof = status_proof(&challenge, ticket.pairing_id, &key);
        fixture.clock.advance(1);
        assert!(matches!(
            fixture
                .service
                .lookup(&ticket.code, Id128::new([0x66; 16]))
                .await,
            Err(PairingError::PairingInvalid)
        ));
        assert!(matches!(
            fixture.service.claim(&claim).await,
            Err(PairingError::PairingInvalid)
        ));
        assert!(matches!(
            fixture.service.status(&proof).await,
            Err(PairingError::PairingInvalid)
        ));
        assert!(matches!(
            fixture
                .service
                .status_challenge(ticket.pairing_id, public_key(&key), Id128::new([0x67; 16]))
                .await,
            Err(PairingError::PairingInvalid)
        ));
        assert!(
            fixture
                .service
                .trust
                .client_by_public_key(public_key(&key).as_bytes())
                .await
                .expect("DB")
                .is_none()
        );
        fixture.close().await;
    }

    #[tokio::test]
    #[allow(
        clippy::too_many_lines,
        reason = "Keep all three retention boundary branches and their shared invariants together"
    )]
    async fn approved_and_consumed_results_follow_the_exact_maximum_retention_deadline() {
        for (approval_time, retention) in [(0, 300), (240, 300), (299, 359)] {
            let fixture = Fixture::new("retention-boundary").await;
            let (ticket, _) = fixture.service.create_pairing().await.expect("pair");
            let key = signing_key([0x68; 32]);
            let lookup = fixture
                .service
                .lookup(&ticket.code, Id128::new([0x69; 16]))
                .await
                .expect("lookup");
            fixture
                .service
                .claim(&claim_proof(
                    &lookup.challenge,
                    ticket.pairing_id,
                    &key,
                    "Retention fixture",
                ))
                .await
                .expect("claim");
            fixture.clock.advance(approval_time);
            let client = fixture
                .service
                .approve(ticket.pairing_id)
                .await
                .expect("approve");
            let challenge = fixture
                .service
                .status_challenge(ticket.pairing_id, public_key(&key), Id128::new([0x6A; 16]))
                .await
                .expect("approved status challenge");
            let response = fixture
                .service
                .status(&status_proof(&challenge, ticket.pairing_id, &key))
                .await
                .expect("approved result");
            let result = verify_status_result(&response.host_proof, &fixture.node_public_key())
                .expect("host signature");
            assert_eq!(result.status, PairStatusValue::Approved);
            assert_eq!(result.expires_at.get(), ticket.expires_at + retention - 300);
            assert!(matches!(
                fixture
                    .service
                    .status_challenge(
                        ticket.pairing_id,
                        public_key(&signing_key([0x6B; 32])),
                        Id128::new([0x6C; 16])
                    )
                    .await,
                Err(PairingError::PairingInvalid)
            ));
            fixture
                .service
                .mark_consumed_for_client(&client.client_id)
                .await;
            fixture.clock.advance(retention - approval_time - 1);
            let challenge = fixture
                .service
                .status_challenge(ticket.pairing_id, public_key(&key), Id128::new([0x6D; 16]))
                .await
                .expect("retention minus one");
            let response = fixture
                .service
                .status(&status_proof(&challenge, ticket.pairing_id, &key))
                .await
                .expect("consumed result");
            let result = verify_status_result(&response.host_proof, &fixture.node_public_key())
                .expect("host signature");
            assert_eq!(result.status, PairStatusValue::Consumed);
            assert_eq!(
                result.client_id.map(Id128::to_base64url),
                Some(client.client_id.clone())
            );
            let challenge = fixture
                .service
                .status_challenge(ticket.pairing_id, public_key(&key), Id128::new([0x6E; 16]))
                .await
                .expect("fresh challenge one second before expiry");
            let fresh = status_proof(&challenge, ticket.pairing_id, &key);
            fixture.clock.advance(1);
            assert!(matches!(
                fixture.service.status(&fresh).await,
                Err(PairingError::PairingInvalid)
            ));
            for step in [0, 1] {
                fixture.clock.advance(step);
                assert!(matches!(
                    fixture
                        .service
                        .status_challenge(
                            ticket.pairing_id,
                            public_key(&key),
                            Id128::new([0x6F; 16])
                        )
                        .await,
                    Err(PairingError::PairingInvalid)
                ));
                assert_eq!(
                    fixture
                        .service
                        .trust
                        .client_by_public_key(public_key(&key).as_bytes())
                        .await
                        .expect("trust")
                        .expect("persistent identity")
                        .client_id,
                    client.client_id
                );
            }
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn approval_persists_trust_and_cannot_be_rolled_back_by_cancel() {
        let fixture = Fixture::new("approval").await;
        let (ticket, _events) = fixture.service.create_pairing().await.expect("pairing");
        let client_key = signing_key([9; 32]);
        let lookup = fixture
            .service
            .lookup(&ticket.code, Id128::new([4; 16]))
            .await
            .expect("lookup");
        let proof = claim_proof(&lookup.challenge, ticket.pairing_id, &client_key, "Alice");
        fixture.service.claim(&proof).await.expect("claim");

        let client = fixture
            .service
            .approve(ticket.pairing_id)
            .await
            .expect("approval");
        assert_eq!(client.status, crate::trust::ClientStatus::Trusted);
        assert!(matches!(
            fixture.service.cancel(ticket.pairing_id).await,
            Err(PairingError::AlreadyApproved)
        ));
        assert!(
            fixture
                .service
                .trust
                .client_by_id(&client.client_id)
                .await
                .expect("trust lookup")
                .is_some()
        );

        let status_challenge = fixture
            .service
            .status_challenge(
                ticket.pairing_id,
                public_key(&client_key),
                Id128::new([5; 16]),
            )
            .await
            .expect("status challenge");
        let status = fixture
            .service
            .status(&status_proof(
                &status_challenge,
                ticket.pairing_id,
                &client_key,
            ))
            .await
            .expect("status result");
        let result = verify_status_result(&status.host_proof, &fixture.node_public_key())
            .expect("host status proof");
        assert_eq!(result.purpose, StatusPurpose::StatusResult);
        assert_eq!(result.status, PairStatusValue::Approved);
        assert_eq!(
            result.client_id.map(Id128::to_base64url),
            Some(client.client_id)
        );
        fixture.close().await;
    }

    #[tokio::test]
    #[allow(
        clippy::too_many_lines,
        reason = "Keep the transition table and its shared security invariants together"
    )]
    async fn terminal_states_cannot_transition_back_or_change_committed_trust() {
        for claimed in [false, true] {
            for terminal in [
                SessionState::Rejected,
                SessionState::Cancelled,
                SessionState::Expired,
                SessionState::Approved,
                SessionState::Consumed,
            ] {
                if !claimed && terminal.is_approved() {
                    continue;
                }
                let fixture = Fixture::new("terminal-matrix").await;
                let client = signing_key([15; 32]);
                let (ticket, _events) = fixture.service.create_pairing().await.expect("pairing");
                if claimed {
                    let lookup = fixture
                        .service
                        .lookup(&ticket.code, Id128::new([80; 16]))
                        .await
                        .expect("lookup");
                    fixture
                        .service
                        .claim(&claim_proof(
                            &lookup.challenge,
                            ticket.pairing_id,
                            &client,
                            "Terminal fixture",
                        ))
                        .await
                        .expect("claim");
                } else {
                    assert!(
                        fixture.service.approve(ticket.pairing_id).await.is_err(),
                        "unclaimed requests cannot be approved"
                    );
                }
                match terminal {
                    SessionState::Rejected => fixture
                        .service
                        .reject(ticket.pairing_id)
                        .await
                        .expect("reject"),
                    SessionState::Cancelled => fixture
                        .service
                        .cancel(ticket.pairing_id)
                        .await
                        .expect("cancel"),
                    SessionState::Expired => {
                        fixture.clock.advance(300);
                        assert!(
                            fixture
                                .service
                                .expire_if_due(ticket.pairing_id)
                                .await
                                .expect("expire")
                        );
                    }
                    _ => {
                        let stored = fixture
                            .service
                            .approve(ticket.pairing_id)
                            .await
                            .expect("approve");
                        if terminal == SessionState::Consumed {
                            fixture
                                .service
                                .mark_consumed_for_client(&stored.client_id)
                                .await;
                        }
                    }
                }
                let before = fixture
                    .service
                    .trust
                    .client_by_public_key(public_key(&client).as_bytes())
                    .await
                    .expect("trust before");
                assert_eq!(before.is_some(), terminal.is_approved());
                let _ = fixture.service.approve(ticket.pairing_id).await;
                let _ = fixture.service.reject(ticket.pairing_id).await;
                let _ = fixture.service.cancel(ticket.pairing_id).await;
                let _ = fixture.service.expire_if_due(ticket.pairing_id).await;
                let state = fixture
                    .service
                    .registry
                    .lock()
                    .await
                    .sessions
                    .get(&ticket.pairing_id)
                    .map(|session| session.state);
                assert!(
                    state.is_none() || state == Some(terminal),
                    "terminal state changed: {terminal:?} -> {state:?}"
                );
                let after = fixture
                    .service
                    .trust
                    .client_by_public_key(public_key(&client).as_bytes())
                    .await
                    .expect("trust after");
                assert_eq!(
                    before, after,
                    "terminal retries must not resurrect or roll back trust"
                );
                fixture.close().await;
            }
        }
    }

    #[tokio::test]
    async fn concurrent_approval_and_cancellation_follow_the_commit_winner() {
        for approve_first in [true, false] {
            let fixture = Fixture::new("approve-cancel-race").await;
            let client = signing_key([16; 32]);
            let (ticket, _events) = fixture.service.create_pairing().await.expect("pairing");
            let lookup = fixture
                .service
                .lookup(&ticket.code, Id128::new([81; 16]))
                .await
                .expect("lookup");
            fixture
                .service
                .claim(&claim_proof(
                    &lookup.challenge,
                    ticket.pairing_id,
                    &client,
                    "Race fixture",
                ))
                .await
                .expect("claim");
            let (approval, cancellation) = if approve_first {
                tokio::join!(
                    fixture.service.approve(ticket.pairing_id),
                    fixture.service.cancel(ticket.pairing_id)
                )
            } else {
                let (cancellation, approval) = tokio::join!(
                    fixture.service.cancel(ticket.pairing_id),
                    fixture.service.approve(ticket.pairing_id)
                );
                (approval, cancellation)
            };
            let stored = fixture
                .service
                .trust
                .client_by_public_key(public_key(&client).as_bytes())
                .await
                .expect("authoritative trust");
            if let Ok(approved) = approval {
                assert!(matches!(cancellation, Err(PairingError::AlreadyApproved)));
                assert_eq!(
                    stored.expect("committed trust remains").client_id,
                    approved.client_id
                );
            } else {
                assert!(cancellation.is_ok());
                assert!(stored.is_none(), "cancellation won before commit");
            }
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn reject_and_cancel_never_create_trust() {
        let fixture = Fixture::new("reject").await;
        let clients = [
            (signing_key([10; 32]), PairStatusValue::Rejected),
            (signing_key([11; 32]), PairStatusValue::Cancelled),
        ];

        for (index, (client_key, expected_status)) in clients.iter().enumerate() {
            let (ticket, _events) = fixture.service.create_pairing().await.expect("pairing");
            let request_id = Id128::new([20 + u8::try_from(index).expect("index"); 16]);
            let lookup = fixture
                .service
                .lookup(&ticket.code, request_id)
                .await
                .expect("lookup");
            let proof = claim_proof(&lookup.challenge, ticket.pairing_id, client_key, "Alice");
            fixture.service.claim(&proof).await.expect("claim");
            if expected_status == &PairStatusValue::Rejected {
                fixture
                    .service
                    .reject(ticket.pairing_id)
                    .await
                    .expect("reject");
            } else {
                fixture
                    .service
                    .cancel(ticket.pairing_id)
                    .await
                    .expect("cancel");
            }

            let status_challenge = fixture
                .service
                .status_challenge(
                    ticket.pairing_id,
                    public_key(client_key),
                    Id128::new([30 + u8::try_from(index).expect("index"); 16]),
                )
                .await
                .expect("status challenge");
            let status = fixture
                .service
                .status(&status_proof(
                    &status_challenge,
                    ticket.pairing_id,
                    client_key,
                ))
                .await
                .expect("status");
            let result = verify_status_result(&status.host_proof, &fixture.node_public_key())
                .expect("verified status");
            assert_eq!(&result.status, expected_status);
            assert_eq!(result.client_id, None);
            assert!(
                fixture
                    .service
                    .trust
                    .client_by_public_key(public_key(client_key).as_bytes())
                    .await
                    .expect("trust lookup")
                    .is_none()
            );
        }
        fixture.close().await;
    }

    #[tokio::test]
    async fn revoked_public_key_cannot_establish_a_new_identity() {
        let fixture = Fixture::new("revoked").await;
        let client_key = signing_key([12; 32]);
        let (first_ticket, _events) = fixture
            .service
            .create_pairing()
            .await
            .expect("first pairing");
        let first_lookup = fixture
            .service
            .lookup(&first_ticket.code, Id128::new([40; 16]))
            .await
            .expect("first lookup");
        let first_proof = claim_proof(
            &first_lookup.challenge,
            first_ticket.pairing_id,
            &client_key,
            "Alice",
        );
        fixture
            .service
            .claim(&first_proof)
            .await
            .expect("first claim");
        let client = fixture
            .service
            .approve(first_ticket.pairing_id)
            .await
            .expect("approval");
        fixture
            .service
            .trust
            .revoke(
                "local-owner",
                &Id128::new([41; 16]).to_base64url(),
                &client.client_id,
            )
            .await
            .expect("revoke");

        let (second_ticket, _events) = fixture
            .service
            .create_pairing()
            .await
            .expect("second pairing");
        let second_lookup = fixture
            .service
            .lookup(&second_ticket.code, Id128::new([42; 16]))
            .await
            .expect("second lookup");
        let second_proof = claim_proof(
            &second_lookup.challenge,
            second_ticket.pairing_id,
            &client_key,
            "Alice",
        );
        assert!(matches!(
            fixture.service.claim(&second_proof).await,
            Err(PairingError::PairingInvalid)
        ));
        fixture.close().await;
    }

    #[tokio::test]
    async fn restart_and_clear_discard_temporary_pairing_material() {
        let fixture = Fixture::new("restart").await;
        let (ticket, _events) = fixture.service.create_pairing().await.expect("pairing");
        let restarted = PairingService::with_sources(
            fixture.identity.clone(),
            "https://node.example.test".to_owned(),
            fixture.service.trust.clone(),
            fixture.clock.clone(),
            Arc::new(TestRandom::new()),
        )
        .expect("restarted pairing service");
        assert!(matches!(
            restarted.lookup(&ticket.code, Id128::new([50; 16])).await,
            Err(PairingError::PairingInvalid)
        ));

        fixture.service.clear().await;
        assert!(matches!(
            fixture
                .service
                .lookup(&ticket.code, Id128::new([51; 16]))
                .await,
            Err(PairingError::PairingInvalid)
        ));
        fixture.close().await;
    }
}
