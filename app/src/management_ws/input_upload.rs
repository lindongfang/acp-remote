//! ACPI framing only. A decoded identifier is never an upload qualification.
#![allow(
    clippy::match_same_arms,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use crate::claims::Id128;

pub const HEADER_BYTES: usize = 32;
pub const CHUNK_BYTES: usize = 16_384;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadError {
    InvalidFrame,
    InvalidDirection,
    InvalidOwner,
    InvalidTarget,
    InvalidState,
    Conflict,
    BodyUnavailable,
    InvalidBody,
    Allocation,
    NodeStopping,
    StorageUnavailable,
}
impl std::fmt::Display for UploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for UploadError {}
impl UploadError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::InvalidFrame | Self::InvalidDirection | Self::InvalidOwner => "invalid_request",
            Self::InvalidTarget => "invalid_target",
            Self::InvalidState => "invalid_upload_state",
            Self::Conflict => "upload_conflict",
            Self::BodyUnavailable => "body_unavailable",
            Self::InvalidBody => "invalid_request",
            Self::Allocation => "allocation_failed",
            Self::NodeStopping => "product_not_ready",
            Self::StorageUnavailable => "necessary_storage_unavailable",
        }
    }
}

pub struct InputData<'a> {
    upload: Id128,
    offset: u64,
    payload: &'a [u8],
}
impl<'a> InputData<'a> {
    pub(crate) fn decode_client(wire: &'a [u8]) -> Result<Self, UploadError> {
        if wire.len() < HEADER_BYTES
            || &wire[..4] != b"ACPI"
            || wire[4] != 1
            || wire[6] != 0
            || wire[7] != 0
            || !matches!(wire[5], 1 | 2)
        {
            return Err(UploadError::InvalidFrame);
        }
        if wire[5] != 1 {
            return Err(UploadError::InvalidDirection);
        }
        let payload = &wire[HEADER_BYTES..];
        if payload.is_empty() || payload.len() > CHUNK_BYTES {
            return Err(UploadError::InvalidFrame);
        }
        let mut id = [0; 16];
        id.copy_from_slice(&wire[8..24]);
        let mut offset = [0; 8];
        offset.copy_from_slice(&wire[24..32]);
        let offset = u64::from_be_bytes(offset);
        offset
            .checked_add(payload.len() as u64)
            .ok_or(UploadError::InvalidFrame)?;
        Ok(Self {
            upload: Id128::new(id),
            offset,
            payload,
        })
    }
    pub(crate) const fn upload(&self) -> Id128 {
        self.upload
    }
    pub(crate) const fn offset(&self) -> u64 {
        self.offset
    }
    pub(crate) const fn payload(&self) -> &'a [u8] {
        self.payload
    }
}
impl std::fmt::Debug for InputData<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputData")
            .field("offset", &self.offset)
            .field("payload_bytes", &self.payload.len())
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputAck {
    upload: Id128,
    next_offset: u64,
}
impl InputAck {
    pub(crate) const fn received(upload: Id128, next_offset: u64) -> Self {
        Self {
            upload,
            next_offset,
        }
    }
    pub(crate) const fn next_offset(&self) -> u64 {
        self.next_offset
    }
    pub(crate) fn encode(self) -> [u8; HEADER_BYTES] {
        let mut wire = [0; HEADER_BYTES];
        wire[..4].copy_from_slice(b"ACPI");
        wire[4] = 1;
        wire[5] = 2;
        wire[8..24].copy_from_slice(self.upload.as_bytes());
        wire[24..32].copy_from_slice(&self.next_offset.to_be_bytes());
        wire
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acpi_codec_is_exact_and_rejects_direction_lengths_and_overflow() {
        let id = Id128::new([7; 16]);
        let ack = InputAck::received(id, u64::MAX).encode();
        assert_eq!(&ack[..8], b"ACPI\x01\x02\0\0");
        assert_eq!(&ack[8..24], id.as_bytes());
        assert_eq!(&ack[24..], &u64::MAX.to_be_bytes());
        assert!(matches!(
            InputData::decode_client(&ack),
            Err(UploadError::InvalidDirection)
        ));
        let mut data = ack.to_vec();
        data[5] = 1;
        data[24..32].copy_from_slice(&0u64.to_be_bytes());
        data.push(17);
        let decoded = InputData::decode_client(&data).unwrap();
        assert_eq!(decoded.upload(), id);
        assert_eq!(decoded.offset(), 0);
        assert_eq!(decoded.payload(), &[17]);
        data[24..32].copy_from_slice(&u64::MAX.to_be_bytes());
        assert!(InputData::decode_client(&data).is_err());
        data[24..32].copy_from_slice(&0u64.to_be_bytes());
        data.resize(HEADER_BYTES + CHUNK_BYTES, 8);
        assert_eq!(
            InputData::decode_client(&data).unwrap().payload().len(),
            CHUNK_BYTES
        );
        data.push(8);
        assert!(InputData::decode_client(&data).is_err());
        for bad in [vec![0; 31], ack[..31].to_vec()] {
            assert!(InputData::decode_client(&bad).is_err());
        }
        data.truncate(33);
        data[6] = 1;
        assert!(InputData::decode_client(&data).is_err());
    }
}
