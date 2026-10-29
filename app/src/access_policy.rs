//! Application access separates authenticated management actors from runtime approval.
use std::{
    sync::{Arc, Weak},
    time::{SystemTime, UNIX_EPOCH},
};

use thiserror::Error;

use crate::{
    challenge::IssuedChallenge,
    claims::{Id128, Nonce256, PublicKeySec1},
    ipc::IpcControl,
    pairing::{PairingError, PairingService},
    trust::{ClientStatus, TrustStore},
};

/// Opaque actor minted only after the application's one-time client proof verification.
#[derive(Clone)]
pub struct AuthenticatedActor {
    node_id: String,
    pub(crate) client_id_string: String,
    pub(crate) public_key: Vec<u8>,
    pub(crate) auth_generation: i64,
}

/// Owns a private challenge scope; dropping it cancels its remaining challenge.
pub struct AuthenticationChallenge {
    challenge: IssuedChallenge,
    connection_id: Id128,
    pairing: Weak<PairingService>,
}

impl AuthenticationChallenge {
    #[must_use]
    pub const fn challenge(&self) -> &IssuedChallenge {
        &self.challenge
    }
    #[must_use]
    pub const fn connection_id(&self) -> Id128 {
        self.connection_id
    }
}

impl Drop for AuthenticationChallenge {
    fn drop(&mut self) {
        if let Some(pairing) = self.pairing.upgrade() {
            pairing.discard_auth_challenge(self.connection_id);
        }
    }
}

impl AuthenticatedActor {
    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id_string
    }
}

/// Presented metadata is not a grant and cannot authorize an operation by itself.
pub struct ApprovalPresentation {
    pub expires_at: u64,
}

/// All requests through this entry require independent runtime approval.
pub struct RuntimeCapabilityRequest {
    pub name: String,
    pub approval: Option<ApprovalPresentation>,
}

pub trait PolicyClock: Send + Sync {
    /// # Errors
    /// Returns an error if the server cannot determine a valid Unix time.
    fn unix_seconds(&self) -> Result<u64, AccessPolicyError>;
}

struct SystemPolicyClock;
impl PolicyClock for SystemPolicyClock {
    fn unix_seconds(&self) -> Result<u64, AccessPolicyError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .map_err(|_| AccessPolicyError::ClockUnavailable)
    }
}

#[derive(Debug, Error)]
pub enum AccessPolicyError {
    #[error("client proof is invalid")]
    Proof(#[source] PairingError),
    #[error("management actor is invalid")]
    InvalidActor,
    #[error("access policy storage is unavailable")]
    StorageUnavailable,
    #[error("access policy clock is unavailable")]
    ClockUnavailable,
    #[error("access policy node is unavailable")]
    NodeUnavailable,
    #[error("access policy random source is unavailable")]
    RandomUnavailable,
    #[error("runtime approval is required")]
    ApprovalRequired,
    #[error("runtime approval has expired")]
    ApprovalExpired,
    #[error("runtime approval has no verifiable issuer")]
    ApprovalUnverified,
}

pub struct ApplicationPolicy {
    node_id: String,
    trust: TrustStore,
    pairing: Weak<PairingService>,
    clock: Arc<dyn PolicyClock>,
    control: IpcControl,
}

impl ApplicationPolicy {
    pub(crate) fn new(
        node_id: String,
        trust: TrustStore,
        pairing: &Arc<PairingService>,
        control: IpcControl,
    ) -> Self {
        Self {
            node_id,
            trust,
            pairing: Arc::downgrade(pairing),
            clock: Arc::new(SystemPolicyClock),
            control,
        }
    }

    /// Uses the same trust coordinator and authentication registry with a controlled clock.
    /// Intended for deterministic policy verification, not an alternate approval source.
    #[must_use]
    pub fn with_clock(&self, clock: Arc<dyn PolicyClock>) -> Self {
        Self {
            node_id: self.node_id.clone(),
            trust: self.trust.clone(),
            pairing: self.pairing.clone(),
            clock,
            control: self.control.clone(),
        }
    }

    /// Issues an ordinary one-time management challenge for an in-process adapter.
    /// # Errors
    /// Returns an error when challenge generation fails or its budget is exhausted.
    pub fn issue_authentication_challenge(
        &self,
        client_id: Option<Id128>,
        key: PublicKeySec1,
        nonce: Nonce256,
    ) -> Result<AuthenticationChallenge, AccessPolicyError> {
        self.require_ready()?;
        let connection_id =
            Id128::new(crate::random::bytes().map_err(|_| AccessPolicyError::RandomUnavailable)?);
        let challenge = self
            .pairing
            .upgrade()
            .ok_or(AccessPolicyError::NodeUnavailable)?
            .issue_auth_challenge(connection_id, client_id, key, nonce)
            .map_err(AccessPolicyError::Proof)?;
        Ok(AuthenticationChallenge {
            challenge,
            connection_id,
            pairing: self.pairing.clone(),
        })
    }

    /// Verifies the real single-use proof before resolving the current trusted client.
    /// # Errors
    /// Rejects invalid proofs, unknown/revoked clients and unavailable storage.
    pub async fn authenticate(
        &self,
        connection_id: Id128,
        compact: &str,
    ) -> Result<AuthenticatedActor, AccessPolicyError> {
        self.require_ready()?;
        let proof = self
            .pairing
            .upgrade()
            .ok_or(AccessPolicyError::NodeUnavailable)?
            .verify_auth_proof(connection_id, compact)
            .map_err(AccessPolicyError::Proof)?;
        if proof.node_id.to_hex() != self.node_id {
            return Err(AccessPolicyError::InvalidActor);
        }
        let record = if let Some(id) = proof.client_id {
            self.trust.client_by_id(&id.to_base64url()).await
        } else {
            self.trust
                .client_by_public_key(proof.client_public_key.as_bytes())
                .await
        }
        .map_err(|_| AccessPolicyError::StorageUnavailable)?
        .ok_or(AccessPolicyError::InvalidActor)?;
        if record.status != ClientStatus::Trusted
            || record.public_key.as_slice() != proof.client_public_key.as_bytes()
        {
            return Err(AccessPolicyError::InvalidActor);
        }
        Ok(AuthenticatedActor {
            node_id: self.node_id.clone(),
            client_id_string: record.client_id,
            public_key: record.public_key,
            auth_generation: record.auth_generation,
        })
    }

    /// Rechecks qualification; it never constitutes runtime approval.
    /// # Errors
    /// Rejects a stale, revoked or differently scoped actor, or unavailable storage.
    pub async fn require_current_actor(
        &self,
        actor: &AuthenticatedActor,
    ) -> Result<(), AccessPolicyError> {
        self.require_ready()?;
        if actor.node_id != self.node_id {
            return Err(AccessPolicyError::InvalidActor);
        }
        let record = self
            .trust
            .client_by_id(actor.client_id())
            .await
            .map_err(|_| AccessPolicyError::StorageUnavailable)?;
        if record.is_some_and(|record| {
            record.status == ClientStatus::Trusted
                && record.public_key == actor.public_key
                && record.auth_generation == actor.auth_generation
        }) {
            Ok(())
        } else {
            Err(AccessPolicyError::InvalidActor)
        }
    }

    /// Application side-effect boundary for future runtime adapters.
    /// No trusted approval issuer exists in this change, so no request can dispatch.
    /// # Errors
    /// Returns an actor, clock or approval error before any dispatcher invocation.
    pub async fn request_runtime_capability(
        &self,
        actor: &AuthenticatedActor,
        request: &RuntimeCapabilityRequest,
        dispatcher: impl FnOnce(&RuntimeCapabilityRequest),
    ) -> Result<(), AccessPolicyError> {
        let gate = self.trust.client_gate(actor.client_id());
        let _guard = gate.lock().await;
        self.require_current_actor(actor).await?;
        self.require_runtime_approval(request.approval.as_ref())?;
        dispatcher(request);
        Ok(())
    }

    fn require_ready(&self) -> Result<(), AccessPolicyError> {
        if self.control.is_ready() {
            Ok(())
        } else {
            Err(AccessPolicyError::NodeUnavailable)
        }
    }

    fn require_runtime_approval(
        &self,
        presentation: Option<&ApprovalPresentation>,
    ) -> Result<(), AccessPolicyError> {
        let presentation = presentation.ok_or(AccessPolicyError::ApprovalRequired)?;
        let now = self.clock.unix_seconds()?;
        if presentation.expires_at <= now {
            return Err(AccessPolicyError::ApprovalExpired);
        }
        // A claimed future timestamp cannot replace a verified independent approval.
        Err(AccessPolicyError::ApprovalUnverified)
    }
}
