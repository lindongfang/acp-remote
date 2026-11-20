//! Signed, scope-bound page coordinates. Reading authorization is checked by the caller.
use super::{Direction, PageError, PageSize};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"acp-remote/history-page-cursor/v1\0";
const PAYLOAD: usize = 44;
const WIRE: usize = PAYLOAD + 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Generation {
    Cache([u8; 16]),
    Live([u8; 16]),
}

impl Generation {
    const fn parts(self) -> (u8, [u8; 16]) {
        match self {
            Self::Cache(token) => (1, token),
            Self::Live(token) => (2, token),
        }
    }

    #[must_use]
    pub fn encode(self) -> String {
        let (kind, token) = self.parts();
        let mut bytes = [0_u8; 17];
        bytes[0] = kind;
        bytes[1..].copy_from_slice(&token);
        URL_SAFE_NO_PAD.encode(bytes)
    }
}

/// Each identity field is prefixed by its unsigned, big-endian 64-bit byte length.
#[must_use]
pub fn fingerprint(kind: &str, fields: &[&[u8]], child: Option<&str>) -> [u8; 16] {
    fn field(hash: &mut Sha256, bytes: &[u8]) {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    let mut hash = Sha256::new();
    field(&mut hash, kind.as_bytes());
    for bytes in fields {
        field(&mut hash, bytes);
    }
    hash.update([u8::from(child.is_some())]);
    if let Some(child) = child {
        field(&mut hash, child.as_bytes());
    }
    let mut short = [0; 16];
    short.copy_from_slice(&hash.finalize()[..16]);
    short
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CursorScope {
    pub target: [u8; 16],
    pub direction: Direction,
    pub size: PageSize,
}

impl CursorScope {
    /// Signs the full v1 payload using the node's persistent signing operation.
    #[must_use]
    pub fn issue(
        self,
        generation: Generation,
        anchor: u64,
        sign: impl FnOnce(&[u8]) -> [u8; 64],
    ) -> String {
        let mut buf = Vec::with_capacity(WIRE);
        let (kind, token) = generation.parts();
        buf.extend([
            1,
            match self.direction {
                Direction::Older => 0,
                Direction::Newer => 1,
            },
        ]);
        buf.extend(self.target);
        buf.push(kind);
        buf.extend(token);
        buf.extend(anchor.to_be_bytes());
        buf.push(self.size.get());
        buf.extend(sign(&[DOMAIN, &buf].concat()));
        URL_SAFE_NO_PAD.encode(buf)
    }

    /// # Errors
    /// Malformed or foreign cursors are invalid. Only an authentic matching scope can be stale.
    pub fn verify(
        self,
        generation: Generation,
        wire: &str,
        public_key: &[u8],
    ) -> Result<u64, PageError> {
        let (stored, anchor) = self.authenticate(wire, public_key)?;
        if stored != generation {
            return Err(PageError::StaleCursor);
        }
        Ok(anchor)
    }
    /// Checks the signature and the complete request scope before looking up a generation.
    pub(crate) fn authenticate(
        self,
        wire: &str,
        public_key: &[u8],
    ) -> Result<(Generation, u64), PageError> {
        if wire.len() != WIRE.div_ceil(3) * 4 {
            return Err(PageError::InvalidCursor);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(wire)
            .map_err(|_| PageError::InvalidCursor)?;
        let bytes: &[u8; WIRE] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| PageError::InvalidCursor)?;
        let (payload, sig) = bytes.split_at(PAYLOAD);
        let key =
            VerifyingKey::from_sec1_bytes(public_key).map_err(|_| PageError::InvalidCursor)?;
        let sig = Signature::from_slice(sig).map_err(|_| PageError::InvalidCursor)?;
        key.verify(&[DOMAIN, payload].concat(), &sig)
            .map_err(|_| PageError::InvalidCursor)?;
        let direction = match self.direction {
            Direction::Older => 0,
            Direction::Newer => 1,
        };
        let anchor = u64::from_be_bytes(
            payload[35..43]
                .try_into()
                .map_err(|_| PageError::InvalidCursor)?,
        );
        if payload[0] != 1
            || payload[1] != direction
            || payload[2..18] != self.target
            || !matches!(payload[18], 1 | 2)
            || payload[43] != self.size.get()
            || anchor == 0
        {
            return Err(PageError::InvalidCursor);
        }
        let token = payload[19..35]
            .try_into()
            .map_err(|_| PageError::InvalidCursor)?;
        let stored = if payload[18] == 1 {
            Generation::Cache(token)
        } else {
            Generation::Live(token)
        };
        Ok((stored, anchor))
    }
}
