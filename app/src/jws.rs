use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{
    Signature, SigningKey, VerifyingKey,
    signature::{Signer, Verifier},
};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use thiserror::Error;

use crate::strict_json::{MAX_PROTOCOL_JSON_DEPTH, StrictJsonError, parse_object};

pub const MAX_COMPACT_JWS_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JwsType {
    PairClaim,
    PairStatus,
    HostClaim,
    HostStatus,
    AuthClient,
    AuthHost,
}

impl JwsType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PairClaim => "acpr-pair-claim+jwt",
            Self::PairStatus => "acpr-pair-status+jwt",
            Self::HostClaim => "acpr-host-claim+jwt",
            Self::HostStatus => "acpr-host-status+jwt",
            Self::AuthClient => "acpr-auth-client+jwt",
            Self::AuthHost => "acpr-auth-host+jwt",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "acpr-pair-claim+jwt" => Some(Self::PairClaim),
            "acpr-pair-status+jwt" => Some(Self::PairStatus),
            "acpr-host-claim+jwt" => Some(Self::HostClaim),
            "acpr-host-status+jwt" => Some(Self::HostStatus),
            "acpr-auth-client+jwt" => Some(Self::AuthClient),
            "acpr-auth-host+jwt" => Some(Self::AuthHost),
            _ => None,
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum JwsError {
    #[error("JWS exceeds the 16 KiB protocol budget")]
    TooLarge,
    #[error("JWS compact serialization is malformed")]
    InvalidCompact,
    #[error("JWS contains non-canonical base64url")]
    InvalidBase64,
    #[error("JWS JSON is invalid")]
    InvalidJson,
    #[error("JWS JSON contains a duplicate field")]
    DuplicateField,
    #[error("JWS payload must be a JSON object")]
    NonObjectPayload,
    #[error("JWS payload does not satisfy the required schema")]
    InvalidPayload,
    #[error("JWS protected header is invalid")]
    InvalidHeader,
    #[error("JWS algorithm must be ES256")]
    UnsupportedAlgorithm,
    #[error("JWS type is not accepted for this operation")]
    UnsupportedType,
    #[error("JWS public key is not a valid P-256 SEC1 point")]
    InvalidPublicKey,
    #[error("JWS signature is not a 64-byte P1363 value")]
    InvalidSignatureEncoding,
    #[error("JWS signature verification failed")]
    InvalidSignature,
}

#[derive(Debug)]
pub struct ParsedJws {
    typ: JwsType,
    payload: Value,
    signing_input: String,
    signature: [u8; 64],
}

impl ParsedJws {
    /// Parses and validates the compact envelope without trusting its payload.
    ///
    /// # Errors
    ///
    /// Returns an error for non-canonical encoding, malformed JSON, an unexpected header, or an
    /// unaccepted application type.
    pub fn parse(compact: &str, accepted_types: &[JwsType]) -> Result<Self, JwsError> {
        if compact.len() > MAX_COMPACT_JWS_BYTES {
            return Err(JwsError::TooLarge);
        }
        let mut segments = compact.split('.');
        let Some(header_segment) = segments.next() else {
            return Err(JwsError::InvalidCompact);
        };
        let Some(payload_segment) = segments.next() else {
            return Err(JwsError::InvalidCompact);
        };
        let Some(signature_segment) = segments.next() else {
            return Err(JwsError::InvalidCompact);
        };
        if segments.next().is_some()
            || header_segment.is_empty()
            || payload_segment.is_empty()
            || signature_segment.is_empty()
        {
            return Err(JwsError::InvalidCompact);
        }

        let header = decode_base64url(header_segment)?;
        let payload_bytes = decode_base64url(payload_segment)?;
        let signature_bytes = decode_base64url(signature_segment)?;
        let signature: [u8; 64] = signature_bytes
            .try_into()
            .map_err(|_| JwsError::InvalidSignatureEncoding)?;
        Signature::from_slice(&signature).map_err(|_| JwsError::InvalidSignatureEncoding)?;
        let header = parse_strict_object(&header)?;
        let typ = parse_header(&header)?;
        if !accepted_types.contains(&typ) {
            return Err(JwsError::UnsupportedType);
        }
        let payload = Value::Object(parse_strict_object(&payload_bytes)?);

        Ok(Self {
            typ,
            payload,
            signing_input: format!("{header_segment}.{payload_segment}"),
            signature,
        })
    }

    #[must_use]
    pub const fn typ(&self) -> JwsType {
        self.typ
    }

    #[must_use]
    pub const fn payload(&self) -> &Value {
        &self.payload
    }

    /// Decodes the already validated payload into an exact protocol schema.
    ///
    /// # Errors
    ///
    /// Returns an error when the payload contains unknown fields, incorrect types, or values that
    /// cannot be represented by the requested schema.
    pub fn decode_payload<T: DeserializeOwned>(&self) -> Result<T, JwsError> {
        serde_json::from_value(self.payload.clone()).map_err(|_| JwsError::InvalidPayload)
    }

    /// Verifies the signature over the original compact JWS signing input.
    ///
    /// # Errors
    ///
    /// Returns an error when the SEC1 public key, P1363 signature, or ECDSA proof is invalid.
    pub fn verify(&self, public_key_sec1: &[u8]) -> Result<(), JwsError> {
        let public_key = VerifyingKey::from_sec1_bytes(public_key_sec1)
            .map_err(|_| JwsError::InvalidPublicKey)?;
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| JwsError::InvalidSignature)?;
        public_key
            .verify(self.signing_input.as_bytes(), &signature)
            .map_err(|_| JwsError::InvalidSignature)
    }
}

/// Creates a strict ES256 compact JWS over the supplied JSON object bytes.
///
/// # Errors
///
/// Returns an error when the payload is not a bounded JSON object or signing fails.
pub fn sign(
    typ: JwsType,
    payload_bytes: &[u8],
    signing_key: &SigningKey,
) -> Result<String, JwsError> {
    if payload_bytes.len() > MAX_COMPACT_JWS_BYTES {
        return Err(JwsError::TooLarge);
    }
    parse_strict_object(payload_bytes)?;
    let header = format!(r#"{{"alg":"ES256","typ":"{}"}}"#, typ.as_str());
    let header_segment = URL_SAFE_NO_PAD.encode(header.as_bytes());
    let payload_segment = URL_SAFE_NO_PAD.encode(payload_bytes);
    let signing_input = format!("{header_segment}.{payload_segment}");
    let signature: Signature = signing_key.sign(signing_input.as_bytes());
    let signature_segment = URL_SAFE_NO_PAD.encode(signature.to_bytes());
    let compact = format!("{signing_input}.{signature_segment}");
    if compact.len() > MAX_COMPACT_JWS_BYTES {
        return Err(JwsError::TooLarge);
    }
    Ok(compact)
}

/// Decodes a strict, unpadded base64url field of an exact byte length.
///
/// # Errors
///
/// Returns an error for non-canonical encoding or an unexpected decoded length.
pub(crate) fn decode_fixed_base64url(
    input: &str,
    expected_len: usize,
) -> Result<Vec<u8>, JwsError> {
    let decoded = decode_base64url(input)?;
    if decoded.len() != expected_len {
        return Err(JwsError::InvalidBase64);
    }
    Ok(decoded)
}

/// Decodes a strict, unpadded base64url field.
///
/// # Errors
///
/// Returns an error for padding, foreign alphabet characters, or non-canonical trailing bits.
pub(crate) fn decode_base64url(input: &str) -> Result<Vec<u8>, JwsError> {
    let decoded = URL_SAFE_NO_PAD
        .decode(input)
        .map_err(|_| JwsError::InvalidBase64)?;
    if URL_SAFE_NO_PAD.encode(&decoded) != input {
        return Err(JwsError::InvalidBase64);
    }
    Ok(decoded)
}

fn parse_header(header: &Map<String, Value>) -> Result<JwsType, JwsError> {
    if header.len() != 2 {
        return Err(JwsError::InvalidHeader);
    }
    let algorithm = header
        .get("alg")
        .and_then(Value::as_str)
        .ok_or(JwsError::InvalidHeader)?;
    if algorithm != "ES256" {
        return Err(JwsError::UnsupportedAlgorithm);
    }
    let typ = header
        .get("typ")
        .and_then(Value::as_str)
        .ok_or(JwsError::InvalidHeader)?;
    JwsType::parse(typ).ok_or(JwsError::UnsupportedType)
}

fn parse_strict_object(input: &[u8]) -> Result<Map<String, Value>, JwsError> {
    parse_object(input, MAX_PROTOCOL_JSON_DEPTH).map_err(|error| match error {
        StrictJsonError::DuplicateField => JwsError::DuplicateField,
        StrictJsonError::NonObject => JwsError::NonObjectPayload,
        StrictJsonError::Invalid | StrictJsonError::TooDeep => JwsError::InvalidJson,
    })
}

#[cfg(test)]
mod strict_json_tests {
    use super::*;

    #[test]
    fn depth_limit_is_applied_to_jws_json() {
        let nested = br#"{"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":{"i":1}}}}}}}}}"#;
        assert!(matches!(
            sign(
                JwsType::PairClaim,
                nested,
                &SigningKey::from_slice(&[7; 32]).expect("key")
            ),
            Err(JwsError::InvalidJson)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signing_key() -> SigningKey {
        let scalar = [7_u8; 32];
        SigningKey::from_slice(&scalar).expect("test scalar")
    }

    #[test]
    fn signs_and_verifies_the_original_signing_input() {
        let key = signing_key();
        let public_key = key.verifying_key().to_sec1_point(false);
        let payload = br#"{"v":1,"value":"test"}"#;
        let compact = sign(JwsType::PairClaim, payload, &key).expect("sign");
        let parsed = ParsedJws::parse(&compact, &[JwsType::PairClaim]).expect("parse");
        parsed
            .verify(public_key.as_bytes())
            .expect("verify original input");
        assert_eq!(parsed.typ(), JwsType::PairClaim);
        assert_eq!(parsed.payload()["value"], "test");

        let mut parts: Vec<String> = compact.split('.').map(str::to_owned).collect();
        parts[1] = URL_SAFE_NO_PAD.encode(br#"{"v":1,"value":"tesu"}"#);
        let tampered = parts.join(".");
        let parsed = ParsedJws::parse(&tampered, &[JwsType::PairClaim]).expect("parse tampered");
        assert_eq!(
            parsed.verify(public_key.as_bytes()),
            Err(JwsError::InvalidSignature)
        );
    }

    #[test]
    fn rejects_non_canonical_base64_and_wrong_parts() {
        let key = signing_key();
        let payload = br#"{"v":1}"#;
        let compact = sign(JwsType::PairStatus, payload, &key).expect("sign");
        let signature = compact.rsplit('.').next().expect("signature");
        assert_eq!(
            decode_fixed_base64url(&format!("{signature}="), 64),
            Err(JwsError::InvalidBase64)
        );
        assert!(matches!(
            ParsedJws::parse(&format!("{compact}.extra"), &[JwsType::PairStatus]),
            Err(JwsError::InvalidCompact)
        ));
    }

    #[test]
    fn rejects_duplicate_fields_and_unexpected_types() {
        let key = signing_key();
        let duplicate = br#"{"v":1,"v":1}"#;
        assert_eq!(
            sign(JwsType::PairClaim, duplicate, &key),
            Err(JwsError::DuplicateField)
        );
        let nested_duplicate = br#"{"v":1,"nested":{"field":1,"field":2}}"#;
        assert_eq!(
            sign(JwsType::PairClaim, nested_duplicate, &key),
            Err(JwsError::DuplicateField)
        );
        let valid = sign(JwsType::PairClaim, br#"{"v":1}"#, &key).expect("sign");
        assert!(matches!(
            ParsedJws::parse(&valid, &[JwsType::AuthClient]),
            Err(JwsError::UnsupportedType)
        ));
    }

    #[test]
    fn rejects_zero_and_out_of_range_p1363_scalars() {
        let key = signing_key();
        let compact = sign(JwsType::PairStatus, br#"{"v":1}"#, &key).expect("sign");
        let mut parts: Vec<String> = compact.split('.').map(str::to_owned).collect();

        parts[2] = URL_SAFE_NO_PAD.encode([0_u8; 64]);
        assert!(matches!(
            ParsedJws::parse(&parts.join("."), &[JwsType::PairStatus]),
            Err(JwsError::InvalidSignatureEncoding)
        ));

        let order = hex::decode("ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551")
            .expect("P-256 order");
        let mut signature = [0_u8; 64];
        signature[..32].copy_from_slice(&order);
        signature[63] = 1;
        parts[2] = URL_SAFE_NO_PAD.encode(signature);
        assert!(matches!(
            ParsedJws::parse(&parts.join("."), &[JwsType::PairStatus]),
            Err(JwsError::InvalidSignatureEncoding)
        ));
    }
}
