use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::VerifyingKey;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest, Sha256};

use crate::{
    config::NodeConfig,
    jws::{JwsError, JwsType, ParsedJws, decode_fixed_base64url},
};

pub const MAX_SAFE_UNIX_SECONDS: u64 = 9_007_199_254_740_991;
pub const MAX_CHALLENGE_SECONDS: u64 = 30;

const ID_BYTES: usize = 16;
const NONCE_BYTES: usize = 32;
const NODE_ID_BYTES: usize = 32;
const PUBLIC_KEY_BYTES: usize = 65;
const MAX_DISPLAY_NAME_SCALARS: usize = 80;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Id128([u8; ID_BYTES]);

impl Id128 {
    #[must_use]
    pub const fn new(bytes: [u8; ID_BYTES]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; ID_BYTES] {
        &self.0
    }

    #[must_use]
    pub fn to_base64url(self) -> String {
        URL_SAFE_NO_PAD.encode(self.0)
    }

    /// Parses one canonical unpadded base64url 128-bit identifier.
    ///
    /// # Errors
    ///
    /// Returns an error for padding, non-canonical encoding, or the wrong decoded length.
    pub fn parse_base64url(value: &str) -> Result<Self, JwsError> {
        let bytes = decode_fixed_base64url(value, ID_BYTES)?;
        let bytes: [u8; ID_BYTES] = bytes.try_into().map_err(|_| JwsError::InvalidBase64)?;
        Ok(Self(bytes))
    }
}

impl Serialize for Id128 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_base64url())
    }
}

impl<'de> Deserialize<'de> for Id128 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let bytes = decode_fixed_base64url(&value, ID_BYTES)
            .map_err(|_| de::Error::custom("invalid 128-bit identifier"))?;
        let bytes = bytes
            .try_into()
            .map_err(|_| de::Error::custom("invalid 128-bit identifier"))?;
        Ok(Self(bytes))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Nonce256([u8; NONCE_BYTES]);

impl Nonce256 {
    #[must_use]
    pub const fn new(bytes: [u8; NONCE_BYTES]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; NONCE_BYTES] {
        &self.0
    }

    #[must_use]
    pub fn to_base64url(self) -> String {
        URL_SAFE_NO_PAD.encode(self.0)
    }
}

impl Serialize for Nonce256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_base64url())
    }
}

impl<'de> Deserialize<'de> for Nonce256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let bytes = decode_fixed_base64url(&value, NONCE_BYTES)
            .map_err(|_| de::Error::custom("invalid 256-bit nonce"))?;
        let bytes = bytes
            .try_into()
            .map_err(|_| de::Error::custom("invalid 256-bit nonce"))?;
        Ok(Self(bytes))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NodeId([u8; NODE_ID_BYTES]);

impl NodeId {
    /// Derives the lowercase hexadecimal node identifier from a SEC1 public key.
    ///
    /// # Errors
    ///
    /// Returns an error when the supplied bytes are not an uncompressed P-256 point.
    pub fn from_public_key(public_key: &PublicKeySec1) -> Result<Self, JwsError> {
        let digest = Sha256::digest(public_key.as_bytes());
        let bytes: [u8; NODE_ID_BYTES] = digest
            .as_slice()
            .try_into()
            .map_err(|_| JwsError::InvalidPublicKey)?;
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; NODE_ID_BYTES] {
        &self.0
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

impl Serialize for NodeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.len() != NODE_ID_BYTES * 2
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(de::Error::custom("invalid node identifier"));
        }
        let mut bytes = [0_u8; NODE_ID_BYTES];
        hex::decode_to_slice(value, &mut bytes)
            .map_err(|_| de::Error::custom("invalid node identifier"))?;
        Ok(Self(bytes))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicKeySec1([u8; PUBLIC_KEY_BYTES]);

impl PublicKeySec1 {
    /// Validates an uncompressed SEC1 P-256 public key and wraps it.
    ///
    /// # Errors
    ///
    /// Returns an error when the bytes are not a valid 65-byte uncompressed SEC1 point.
    pub fn new(bytes: [u8; PUBLIC_KEY_BYTES]) -> Result<Self, JwsError> {
        if bytes[0] != 0x04 || VerifyingKey::from_sec1_bytes(&bytes).is_err() {
            return Err(JwsError::InvalidPublicKey);
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; PUBLIC_KEY_BYTES] {
        &self.0
    }
}

impl Serialize for PublicKeySec1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&URL_SAFE_NO_PAD.encode(self.0))
    }
}

impl<'de> Deserialize<'de> for PublicKeySec1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let bytes = decode_fixed_base64url(&value, PUBLIC_KEY_BYTES)
            .map_err(|_| de::Error::custom("invalid public key"))?;
        let bytes = bytes
            .try_into()
            .map_err(|_| de::Error::custom("invalid public key"))?;
        Self::new(bytes).map_err(|_| de::Error::custom("invalid public key"))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct UnixSeconds(u64);

impl UnixSeconds {
    /// Creates a timestamp only when it fits the protocol's JSON safe-integer range.
    ///
    /// # Errors
    ///
    /// Returns an error for values above the JSON safe-integer limit.
    pub const fn from_unix_seconds(value: u64) -> Result<Self, JwsError> {
        if value > MAX_SAFE_UNIX_SECONDS {
            return Err(JwsError::InvalidPayload);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Adds a bounded number of seconds.
    ///
    /// # Errors
    ///
    /// Returns an error when the result exceeds the JSON safe-integer range.
    pub fn checked_add(self, seconds: u64) -> Result<Self, JwsError> {
        let value = self
            .0
            .checked_add(seconds)
            .filter(|value| *value <= MAX_SAFE_UNIX_SECONDS)
            .ok_or(JwsError::InvalidPayload)?;
        Ok(Self(value))
    }
}

impl Serialize for UnixSeconds {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for UnixSeconds {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        if value > MAX_SAFE_UNIX_SECONDS {
            return Err(de::Error::custom(
                "unix timestamp exceeds safe integer range",
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PairClaim {
    pub v: u8,
    pub node_id: NodeId,
    pub origin: String,
    pub request_id: Id128,
    pub challenge_id: Id128,
    pub nonce: Nonce256,
    pub iat: UnixSeconds,
    pub exp: UnixSeconds,
    pub pairing_id: Id128,
    pub client_public_key: PublicKeySec1,
    pub display_name: String,
}

impl PairClaim {
    fn validate(&self) -> Result<(), JwsError> {
        validate_common(
            self.v,
            &self.origin,
            self.iat,
            self.exp,
            Some(&self.display_name),
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PairStatus {
    pub v: u8,
    pub node_id: NodeId,
    pub origin: String,
    pub request_id: Id128,
    pub challenge_id: Id128,
    pub nonce: Nonce256,
    pub iat: UnixSeconds,
    pub exp: UnixSeconds,
    pub pairing_id: Id128,
    pub client_public_key: PublicKeySec1,
}

impl PairStatus {
    fn validate(&self) -> Result<(), JwsError> {
        validate_common(self.v, &self.origin, self.iat, self.exp, None)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClaimPurpose {
    #[serde(rename = "claim-result")]
    ClaimResult,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StatusPurpose {
    #[serde(rename = "status-result")]
    StatusResult,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClaimResultStatus {
    #[serde(rename = "pending_confirmation")]
    PendingConfirmation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairStatusValue {
    PendingConfirmation,
    Approved,
    Rejected,
    Cancelled,
    Expired,
    Consumed,
}

impl PairStatusValue {
    const fn is_approved(self) -> bool {
        matches!(self, Self::Approved | Self::Consumed)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ClaimResult {
    pub v: u8,
    pub purpose: ClaimPurpose,
    pub node_id: NodeId,
    pub origin: String,
    pub pairing_id: Id128,
    pub request_id: Id128,
    pub client_public_key: PublicKeySec1,
    pub status: ClaimResultStatus,
    pub expires_at: UnixSeconds,
}

impl ClaimResult {
    fn validate(&self) -> Result<(), JwsError> {
        validate_result_base(self.v, &self.origin)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StatusResult {
    pub v: u8,
    pub purpose: StatusPurpose,
    pub node_id: NodeId,
    pub origin: String,
    pub pairing_id: Id128,
    pub request_id: Id128,
    pub client_public_key: PublicKeySec1,
    pub status: PairStatusValue,
    pub client_id: Option<Id128>,
    pub expires_at: UnixSeconds,
}

impl StatusResult {
    fn validate(&self) -> Result<(), JwsError> {
        validate_result_base(self.v, &self.origin)?;
        if self.status.is_approved() != self.client_id.is_some() {
            return Err(JwsError::InvalidPayload);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AuthProof {
    pub v: u8,
    pub node_id: NodeId,
    pub node_public_key: PublicKeySec1,
    pub origin: String,
    pub connection_id: Id128,
    pub challenge_id: Id128,
    pub nonce: Nonce256,
    pub iat: UnixSeconds,
    pub exp: UnixSeconds,
    pub client_id: Option<Id128>,
    pub client_public_key: PublicKeySec1,
    pub client_nonce: Nonce256,
}

impl AuthProof {
    fn validate(&self) -> Result<(), JwsError> {
        validate_common(self.v, &self.origin, self.iat, self.exp, None)?;
        if NodeId::from_public_key(&self.node_public_key)? != self.node_id {
            return Err(JwsError::InvalidPayload);
        }
        Ok(())
    }
}

/// Parses, schema-validates, and verifies a pairing claim using its own public key.
///
/// # Errors
///
/// Returns an error for an invalid envelope, schema, public key, or signature.
pub fn verify_pair_claim(compact: &str) -> Result<PairClaim, JwsError> {
    let parsed = ParsedJws::parse(compact, &[JwsType::PairClaim])?;
    let claim: PairClaim = parsed.decode_payload()?;
    parsed.verify(claim.client_public_key.as_bytes())?;
    claim.validate()?;
    Ok(claim)
}

/// Parses, schema-validates, and verifies a pairing status proof with its claimed key.
///
/// # Errors
///
/// Returns an error for an invalid envelope, schema, public key, or signature.
pub fn verify_pair_status(compact: &str) -> Result<PairStatus, JwsError> {
    let parsed = ParsedJws::parse(compact, &[JwsType::PairStatus])?;
    let status: PairStatus = parsed.decode_payload()?;
    parsed.verify(status.client_public_key.as_bytes())?;
    status.validate()?;
    Ok(status)
}

/// Verifies a pairing claim result signed by the expected node public key.
///
/// # Errors
///
/// Returns an error when the envelope, schema, signature, or node key is invalid.
pub fn verify_claim_result(
    compact: &str,
    node_public_key: &PublicKeySec1,
) -> Result<ClaimResult, JwsError> {
    let parsed = ParsedJws::parse(compact, &[JwsType::HostClaim])?;
    parsed.verify(node_public_key.as_bytes())?;
    let result: ClaimResult = parsed.decode_payload()?;
    result.validate()?;
    validate_node_identity(result.node_id, node_public_key)?;
    Ok(result)
}

/// Verifies a pairing status result signed by the expected node public key.
///
/// # Errors
///
/// Returns an error when the envelope, schema, signature, or node key is invalid.
pub fn verify_status_result(
    compact: &str,
    node_public_key: &PublicKeySec1,
) -> Result<StatusResult, JwsError> {
    let parsed = ParsedJws::parse(compact, &[JwsType::HostStatus])?;
    parsed.verify(node_public_key.as_bytes())?;
    let result: StatusResult = parsed.decode_payload()?;
    result.validate()?;
    validate_node_identity(result.node_id, node_public_key)?;
    Ok(result)
}

/// Verifies a client management proof using the public key carried in the proof.
///
/// # Errors
///
/// Returns an error for an invalid envelope, schema, public key, signature, or node identity.
pub fn verify_auth_client(compact: &str) -> Result<AuthProof, JwsError> {
    let parsed = ParsedJws::parse(compact, &[JwsType::AuthClient])?;
    let proof: AuthProof = parsed.decode_payload()?;
    parsed.verify(proof.client_public_key.as_bytes())?;
    proof.validate()?;
    Ok(proof)
}

/// Verifies a server authentication proof using the pinned node public key.
///
/// # Errors
///
/// Returns an error for an invalid envelope, schema, signature, public key, or node identity.
pub fn verify_auth_host(
    compact: &str,
    node_public_key: &PublicKeySec1,
) -> Result<AuthProof, JwsError> {
    let parsed = ParsedJws::parse(compact, &[JwsType::AuthHost])?;
    parsed.verify(node_public_key.as_bytes())?;
    let proof: AuthProof = parsed.decode_payload()?;
    proof.validate()?;
    if proof.node_public_key != *node_public_key {
        return Err(JwsError::InvalidPayload);
    }
    Ok(proof)
}

fn validate_result_base(version: u8, origin: &str) -> Result<(), JwsError> {
    if version != 1 {
        return Err(JwsError::InvalidPayload);
    }
    validate_origin(origin)
}

fn validate_node_identity(
    node_id: NodeId,
    node_public_key: &PublicKeySec1,
) -> Result<(), JwsError> {
    if NodeId::from_public_key(node_public_key)? != node_id {
        return Err(JwsError::InvalidPayload);
    }
    Ok(())
}

fn validate_common(
    version: u8,
    origin: &str,
    iat: UnixSeconds,
    exp: UnixSeconds,
    display_name: Option<&str>,
) -> Result<(), JwsError> {
    if version != 1 || exp <= iat || exp.get() - iat.get() > MAX_CHALLENGE_SECONDS {
        return Err(JwsError::InvalidPayload);
    }
    validate_origin(origin)?;
    if let Some(name) = display_name {
        validate_display_name(name)?;
    }
    Ok(())
}

fn validate_origin(origin: &str) -> Result<(), JwsError> {
    let canonical = NodeConfig::normalize_origin(origin).map_err(|_| JwsError::InvalidPayload)?;
    if canonical != origin {
        return Err(JwsError::InvalidPayload);
    }
    Ok(())
}

fn validate_display_name(name: &str) -> Result<(), JwsError> {
    let mut count = 0_usize;
    for character in name.chars() {
        if character.is_control() {
            return Err(JwsError::InvalidPayload);
        }
        count += 1;
        if count > MAX_DISPLAY_NAME_SCALARS {
            return Err(JwsError::InvalidPayload);
        }
    }
    if count == 0 {
        return Err(JwsError::InvalidPayload);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use p256::ecdsa::{SigningKey, signature::Signer};
    use serde_json::{Value, json};

    use super::*;
    use crate::jws::{JwsType, sign};

    fn signing_key() -> SigningKey {
        SigningKey::from_slice(&[7_u8; 32]).expect("test scalar")
    }

    fn public_key(signing_key: &SigningKey) -> PublicKeySec1 {
        let point = signing_key.verifying_key().to_sec1_point(false);
        let bytes: [u8; PUBLIC_KEY_BYTES] = point.as_bytes().try_into().expect("65-byte SEC1 key");
        PublicKeySec1::new(bytes).expect("valid SEC1 key")
    }

    fn node() -> (SigningKey, PublicKeySec1, NodeId) {
        let key = signing_key();
        let public_key = public_key(&key);
        let node_id = NodeId::from_public_key(&public_key).expect("node id");
        (key, public_key, node_id)
    }

    fn pair_claim_value(node_id: NodeId, client_key: PublicKeySec1) -> Value {
        json!({
            "v": 1,
            "nodeId": node_id,
            "origin": "https://node.example.test",
            "requestId": "AAAAAAAAAAAAAAAAAAAAAA",
            "challengeId": "AQEBAQEBAQEBAQEBAQEBAQ",
            "nonce": "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI",
            "iat": 1_700_000_000_u64,
            "exp": 1_700_000_030_u64,
            "pairingId": "AwMDAwMDAwMDAwMDAwMDAw",
            "clientPublicKey": client_key,
            "displayName": "Alice"
        })
    }

    fn compact_for(typ: JwsType, value: &Value, key: &SigningKey) -> String {
        sign(
            typ,
            &serde_json::to_vec(value).expect("serialize payload"),
            key,
        )
        .expect("sign")
    }

    #[test]
    fn verifies_pair_claim_with_the_claimed_key_only() {
        let (_, _, node_id) = node();
        let client_key = signing_key();
        let claimed_public_key = public_key(&client_key);
        let compact = compact_for(
            JwsType::PairClaim,
            &pair_claim_value(node_id, claimed_public_key),
            &client_key,
        );
        let claim = verify_pair_claim(&compact).expect("verified claim");
        assert_eq!(claim.display_name, "Alice");

        let wrong_key = signing_key_bytes([9_u8; 32]);
        let wrong = compact_for(
            JwsType::PairClaim,
            &pair_claim_value(node_id, claimed_public_key),
            &wrong_key,
        );
        assert_eq!(verify_pair_claim(&wrong), Err(JwsError::InvalidSignature));
    }

    #[test]
    fn rejects_unknown_fields_bad_versions_and_noncanonical_origins() {
        let (_, _, node_id) = node();
        let client_key = signing_key();
        let base = pair_claim_value(node_id, public_key(&client_key));

        let mut unknown = base.clone();
        unknown["extra"] = json!(true);
        assert_eq!(
            verify_pair_claim(&compact_for(JwsType::PairClaim, &unknown, &client_key)),
            Err(JwsError::InvalidPayload)
        );

        let mut version = base.clone();
        version["v"] = json!(2);
        assert_eq!(
            verify_pair_claim(&compact_for(JwsType::PairClaim, &version, &client_key)),
            Err(JwsError::InvalidPayload)
        );

        let mut origin = base;
        origin["origin"] = json!("https://NODE.example.test:443/");
        assert_eq!(
            verify_pair_claim(&compact_for(JwsType::PairClaim, &origin, &client_key)),
            Err(JwsError::InvalidPayload)
        );
    }

    #[test]
    fn validates_display_name_by_unicode_scalars_and_controls() {
        let (_, _, node_id) = node();
        let client_key = signing_key();
        let base = pair_claim_value(node_id, public_key(&client_key));

        for invalid in [
            String::new(),
            "\n".to_owned(),
            "x".repeat(81),
            "a\u{7f}b".to_owned(),
        ] {
            let mut payload = base.clone();
            payload["displayName"] = json!(invalid);
            assert_eq!(
                verify_pair_claim(&compact_for(JwsType::PairClaim, &payload, &client_key)),
                Err(JwsError::InvalidPayload)
            );
        }

        let mut valid = base;
        valid["displayName"] = json!("设备🙂");
        assert!(verify_pair_claim(&compact_for(JwsType::PairClaim, &valid, &client_key)).is_ok());
    }

    #[test]
    fn rejects_unsafe_integer_ids_and_wrong_challenge_window() {
        let (_, _, node_id) = node();
        let client_key = signing_key();
        let base = pair_claim_value(node_id, public_key(&client_key));

        let mut big = base.clone();
        big["iat"] = json!(MAX_SAFE_UNIX_SECONDS + 1);
        assert_eq!(
            verify_pair_claim(&compact_for(JwsType::PairClaim, &big, &client_key)),
            Err(JwsError::InvalidPayload)
        );

        let mut short_challenge = base.clone();
        short_challenge["challengeId"] = json!("AQEBAQEBAQEBAQEBAQEB");
        assert_eq!(
            verify_pair_claim(&compact_for(
                JwsType::PairClaim,
                &short_challenge,
                &client_key
            )),
            Err(JwsError::InvalidPayload)
        );

        let mut long_window = base;
        long_window["exp"] = json!(1_700_000_031_u64);
        assert_eq!(
            verify_pair_claim(&compact_for(JwsType::PairClaim, &long_window, &client_key)),
            Err(JwsError::InvalidPayload)
        );
    }

    #[test]
    fn requires_approved_status_to_carry_a_client_id() {
        let (node_key, _, node_id) = node();
        let client_key = signing_key();
        let base = json!({
            "v": 1,
            "purpose": "status-result",
            "nodeId": node_id,
            "origin": "https://node.example.test",
            "pairingId": "AwMDAwMDAwMDAwMDAwMDAw",
            "requestId": "AAAAAAAAAAAAAAAAAAAAAA",
            "clientPublicKey": public_key(&client_key),
            "status": "approved",
            "clientId": null,
            "expiresAt": 1_700_000_300_u64
        });
        assert_eq!(
            verify_status_result(
                &compact_for(JwsType::HostStatus, &base, &node_key),
                &public_key(&node_key)
            ),
            Err(JwsError::InvalidPayload)
        );
    }

    #[test]
    fn host_results_bind_node_id_to_the_pinned_public_key() {
        let (node_key, node_public_key, _) = node();
        let client_key = signing_key_bytes([9_u8; 32]);
        let client_public_key = public_key(&client_key);
        let other_node_id = NodeId::from_public_key(&client_public_key).expect("other node id");

        let claim = json!({
            "v": 1,
            "purpose": "claim-result",
            "nodeId": other_node_id,
            "origin": "https://node.example.test",
            "pairingId": "AwMDAwMDAwMDAwMDAwMDAw",
            "requestId": "AAAAAAAAAAAAAAAAAAAAAA",
            "clientPublicKey": client_public_key,
            "status": "pending_confirmation",
            "expiresAt": 1_700_000_300_u64
        });
        assert_eq!(
            verify_claim_result(
                &compact_for(JwsType::HostClaim, &claim, &node_key),
                &node_public_key
            ),
            Err(JwsError::InvalidPayload)
        );

        let status = json!({
            "v": 1,
            "purpose": "status-result",
            "nodeId": other_node_id,
            "origin": "https://node.example.test",
            "pairingId": "AwMDAwMDAwMDAwMDAwMDAw",
            "requestId": "AAAAAAAAAAAAAAAAAAAAAA",
            "clientPublicKey": client_public_key,
            "status": "pending_confirmation",
            "clientId": null,
            "expiresAt": 1_700_000_300_u64
        });
        assert_eq!(
            verify_status_result(
                &compact_for(JwsType::HostStatus, &status, &node_key),
                &node_public_key
            ),
            Err(JwsError::InvalidPayload)
        );
    }

    #[test]
    fn auth_payload_binds_node_id_to_the_node_public_key() {
        let (node_key, node_public_key, node_id) = node();
        let client_key = signing_key_bytes([8_u8; 32]);
        let payload = json!({
            "v": 1,
            "nodeId": node_id,
            "nodePublicKey": node_public_key,
            "origin": "https://node.example.test",
            "connectionId": "AAAAAAAAAAAAAAAAAAAAAA",
            "challengeId": "AQEBAQEBAQEBAQEBAQEBAQ",
            "nonce": "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI",
            "iat": 1_700_000_000_u64,
            "exp": 1_700_000_015_u64,
            "clientId": null,
            "clientPublicKey": public_key(&client_key),
            "clientNonce": Nonce256::new([3_u8; NONCE_BYTES])
        });
        let compact = compact_for(JwsType::AuthHost, &payload, &node_key);
        let proof = verify_auth_host(&compact, &node_public_key).expect("verified auth proof");
        assert_eq!(proof.node_id, node_id);

        let client_compact = compact_for(JwsType::AuthClient, &payload, &client_key);
        assert_eq!(
            verify_auth_host(&client_compact, &node_public_key),
            Err(JwsError::UnsupportedType)
        );

        let mut wrong_node = payload;
        wrong_node["nodeId"] =
            json!(NodeId::from_public_key(&public_key(&client_key)).expect("id"));
        let compact = compact_for(JwsType::AuthHost, &wrong_node, &node_key);
        assert_eq!(
            verify_auth_host(&compact, &node_public_key),
            Err(JwsError::InvalidPayload)
        );
    }

    #[test]
    fn verification_rejects_der_signature_value() {
        let key = signing_key();
        let payload = br#"{"v":1}"#;
        let compact = compact_for(
            JwsType::PairStatus,
            &serde_json::from_slice(payload).expect("json"),
            &key,
        );
        let mut parts: Vec<String> = compact.split('.').map(str::to_owned).collect();
        let signature: p256::ecdsa::Signature =
            key.sign(format!("{}.{}", parts[0], parts[1]).as_bytes());
        let der = signature.to_der();
        parts[2] = URL_SAFE_NO_PAD.encode(der.as_bytes());
        assert!(matches!(
            ParsedJws::parse(&parts.join("."), &[JwsType::PairStatus]),
            Err(JwsError::InvalidSignatureEncoding)
        ));
    }

    #[test]
    fn rejects_invalid_sec1_public_keys() {
        assert_eq!(
            PublicKeySec1::new([0_u8; PUBLIC_KEY_BYTES]),
            Err(JwsError::InvalidPublicKey)
        );
        let mut off_curve = [0_u8; PUBLIC_KEY_BYTES];
        off_curve[0] = 4;
        off_curve[PUBLIC_KEY_BYTES - 1] = 1;
        assert_eq!(
            PublicKeySec1::new(off_curve),
            Err(JwsError::InvalidPublicKey)
        );

        let short_key = URL_SAFE_NO_PAD.encode([4_u8; PUBLIC_KEY_BYTES - 1]);
        assert!(serde_json::from_value::<PublicKeySec1>(json!(short_key)).is_err());
    }

    fn signing_key_bytes(bytes: [u8; 32]) -> SigningKey {
        SigningKey::from_slice(&bytes).expect("test scalar")
    }
}
