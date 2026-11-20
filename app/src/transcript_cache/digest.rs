//! Canonical 32-byte SHA-256 content digests for persisted ACP transcript snapshots.

use sha2::{Digest, Sha256};

use crate::adapters::acp::{RawMessage, Version, raw_json::RawDocument};

use super::TranscriptCacheError;

const DIGEST_DOMAIN_TAG: &[u8] = b"acp-transcript-cache-v1\0";

const fn protocol_version_byte(version: Version) -> u8 {
    match version {
        Version::V1 => 1,
        Version::V2 => 2,
    }
}

pub fn normalize_frame_json(raw: &RawMessage) -> Result<String, TranscriptCacheError> {
    let document = RawDocument::parse(raw.bytes().to_vec()).map_err(|error| {
        TranscriptCacheError::Protocol(format!("invalid raw JSON frame: {error}"))
    })?;
    document.root().compact().map_err(|error| {
        TranscriptCacheError::Protocol(format!("failed to compact JSON frame: {error}"))
    })
}

#[derive(Clone)]
pub struct IncrementalDigest {
    version: Version,
    hasher: Sha256,
    count: u64,
}

impl IncrementalDigest {
    #[must_use]
    pub(crate) fn new(version: Version) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(DIGEST_DOMAIN_TAG);
        hasher.update([protocol_version_byte(version)]);
        Self {
            version,
            hasher,
            count: 0,
        }
    }

    #[must_use]
    pub(crate) const fn version(&self) -> Version {
        self.version
    }

    pub(crate) fn push_frame(&mut self, raw: &RawMessage) -> Result<(), TranscriptCacheError> {
        let canonical = normalize_frame_json(raw)?;
        let canonical_len = u64::try_from(canonical.len())
            .map_err(|_| TranscriptCacheError::Protocol("frame exceeds u64 length".to_owned()))?;
        self.hasher.update(canonical_len.to_le_bytes());
        self.hasher.update(canonical.as_bytes());
        self.count = self
            .count
            .checked_add(1)
            .ok_or_else(|| TranscriptCacheError::Protocol("frame count overflow".to_owned()))?;
        Ok(())
    }

    #[must_use]
    pub(crate) fn finalize_snapshot(&self) -> [u8; 32] {
        let mut finalized = self.hasher.clone();
        finalized.update(self.count.to_le_bytes());
        finalized.finalize().into()
    }
}

/// Computes a deterministic 32-byte SHA-256 digest over protocol version and semantically
/// normalized frames.
///
/// Insignificant whitespace variations outside JSON strings do not alter the digest, while any
/// field, value, ordering, or `protocol_version` change alters it.
///
/// # Errors
///
/// Returns `TranscriptCacheError::Protocol` if any frame cannot be normalized as valid JSON.
pub fn compute_content_digest(
    version: Version,
    frames: &[RawMessage],
) -> Result<[u8; 32], TranscriptCacheError> {
    let mut digest = IncrementalDigest::new(version);
    for frame in frames {
        digest.push_frame(frame)?;
    }
    Ok(digest.finalize_snapshot())
}
