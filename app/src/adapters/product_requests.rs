//! Strict product-request JWS adapter. Verification is not permission to dispatch a native action.
use crate::{
    domain::{
        EntityId,
        native::RequestEpoch,
        requests::{
            MAX_REQUEST_TIME, ProductAction, ProductRequestIdentity, ProductTarget,
            ProductTargetKind, REQUEST_VALIDITY_MS, RequestIdentityError, RequestIntent,
            RequestTime,
        },
    },
    identity::NodeIdentity,
    jws::{JwsError, JwsType, ParsedJws},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// An internal signing port. Production uses the node identity; this is never a remote key API.
pub trait ProductRequestSigner {
    fn node_id(&self) -> &str;
    /// # Errors
    /// Reports signature encoding failure without exposing the signing material.
    fn sign_product_request(&self, payload: &[u8]) -> Result<String, JwsError>;
}
impl ProductRequestSigner for NodeIdentity {
    fn node_id(&self) -> &str {
        self.node_id()
    }
    fn sign_product_request(&self, payload: &[u8]) -> Result<String, JwsError> {
        self.sign_jws(JwsType::ProductRequest, payload)
    }
}

pub struct SignedProductRequest(String);
impl SignedProductRequest {
    #[must_use]
    pub fn compact(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for SignedProductRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignedProductRequest")
            .field("encoded_bytes", &self.0.len())
            .finish()
    }
}

pub use crate::domain::requests::VerifiedProductRequest;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Claims {
    node_id: String,
    request_epoch: String,
    actor_id: String,
    request_id: String,
    action: String,
    target_kind: String,
    target_id: String,
    payload_digest: String,
    issued_at: u64,
    expires_at: u64,
}

/// Issues the default fixed-duration identity from an already sampled clock and random request ID.
/// # Errors
/// Rejects unsafe times, invalid node IDs and signer failures. No SQL or native action occurs.
pub fn issue(
    signer: &impl ProductRequestSigner,
    epoch: RequestEpoch,
    request: EntityId,
    intent: &RequestIntent,
    time: RequestTime,
) -> Result<SignedProductRequest, RequestIdentityError> {
    if !is_digest(signer.node_id()) {
        return Err(RequestIdentityError::InvalidClaims);
    }
    let expires_at = time
        .millis()
        .checked_add(REQUEST_VALIDITY_MS)
        .filter(|value| *value <= MAX_REQUEST_TIME)
        .ok_or(RequestIdentityError::InvalidClaims)?;
    let claims = Claims {
        node_id: signer.node_id().into(),
        request_epoch: epoch.0.encode(),
        actor_id: intent.actor.encode(),
        request_id: request.encode(),
        action: intent.action.label().into(),
        target_kind: intent.target.kind.label().into(),
        target_id: intent.target.id.encode(),
        payload_digest: hex::encode(intent.payload_digest),
        issued_at: time.millis(),
        expires_at,
    };
    let payload = serde_json::to_vec(&claims).map_err(|_| RequestIdentityError::InvalidClaims)?;
    signer
        .sign_product_request(&payload)
        .map(SignedProductRequest)
        .map_err(|_| RequestIdentityError::InvalidSignature)
}

/// Validates the exact node, epoch, actor, action, target and precomputed business digest.
/// # Errors
/// Wrong purposes/signatures, malformed claims, future issue times, expired or mismatched identities fail closed.
pub fn verify(
    compact: &str,
    node_public_key: &[u8],
    epoch: RequestEpoch,
    expected: &RequestIntent,
    time: RequestTime,
) -> Result<VerifiedProductRequest, RequestIdentityError> {
    let signed = ParsedJws::parse(compact, &[JwsType::ProductRequest])
        .map_err(|_| RequestIdentityError::InvalidSignature)?;
    signed
        .verify(node_public_key)
        .map_err(|_| RequestIdentityError::InvalidSignature)?;
    let claims: Claims = signed
        .decode_payload()
        .map_err(|_| RequestIdentityError::InvalidClaims)?;
    if !is_digest(&claims.node_id)
        || claims.node_id != hex::encode(Sha256::digest(node_public_key))
        || !is_digest(&claims.payload_digest)
        || claims.issued_at > MAX_REQUEST_TIME
        || claims.expires_at > MAX_REQUEST_TIME
        || claims.expires_at <= claims.issued_at
        || claims.expires_at - claims.issued_at > REQUEST_VALIDITY_MS
        || claims.issued_at > time.millis()
    {
        return Err(RequestIdentityError::InvalidClaims);
    }
    let decode_id =
        |value: &str| EntityId::decode(value).map_err(|_| RequestIdentityError::InvalidClaims);
    let request_epoch = RequestEpoch(decode_id(&claims.request_epoch)?);
    let mut digest = [0; 32];
    hex::decode_to_slice(&claims.payload_digest, &mut digest)
        .map_err(|_| RequestIdentityError::InvalidClaims)?;
    let intent = RequestIntent {
        actor: decode_id(&claims.actor_id)?,
        action: ProductAction::from_label(&claims.action)
            .ok_or(RequestIdentityError::InvalidClaims)?,
        target: ProductTarget {
            kind: ProductTargetKind::from_label(&claims.target_kind)
                .ok_or(RequestIdentityError::InvalidClaims)?,
            id: decode_id(&claims.target_id)?,
        },
        payload_digest: digest,
    };
    let identity = ProductRequestIdentity {
        epoch: request_epoch,
        request: decode_id(&claims.request_id)?,
        intent,
        issued_at: claims.issued_at,
        expires_at: claims.expires_at,
    };
    if request_epoch != epoch {
        return Err(RequestIdentityError::EpochMismatch);
    }
    if identity.intent != *expected {
        return Err(RequestIdentityError::IntentMismatch);
    }
    if time.millis() >= identity.expires_at {
        return Err(RequestIdentityError::Expired);
    }
    Ok(VerifiedProductRequest(identity))
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "../../tests/acceptance/request_identity.rs"]
mod historical_identity_acceptance;
