//! Opaque chunk framing. Header parsing never parses, rewrites, or executes a native payload.
use crate::claims::Id128;

pub const HEADER_BYTES: usize = 72;
pub const MAX_PAYLOAD_BYTES: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PacketKind {
    Data,
    Credit,
    Detach,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordKind {
    OriginalRaw,
    ItemProjection,
    Status,
    SnapshotEnd,
    TurnStart,
    FieldBytes,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum WireError {
    #[error("view wire header is invalid")]
    Header,
    #[error("view wire length is invalid")]
    Length,
    #[error("view wire position is invalid")]
    Position,
    #[error("view wire allocation failed")]
    Allocation,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferHeader {
    kind: PacketKind,
    stream: Id128,
    packet: u64,
    record: u64,
    offset: u64,
    total: u64,
    record_kind: Option<RecordKind>,
    last: bool,
    payload_length: u32,
}
impl TransferHeader {
    /// Creates format data only, never a sending or reading authorization.
    /// # Errors
    /// Rejects unrepresentable or inconsistent chunk metadata.
    pub fn data(
        stream: Id128,
        packet: u64,
        record: u64,
        offset: u64,
        total: u64,
        kind: RecordKind,
        length: usize,
    ) -> Result<Self, WireError> {
        let payload_length = u32::try_from(length).map_err(|_| WireError::Length)?;
        let end = offset
            .checked_add(u64::from(payload_length))
            .ok_or(WireError::Position)?;
        let header = Self {
            kind: PacketKind::Data,
            stream,
            packet,
            record,
            offset,
            total,
            record_kind: Some(kind),
            last: end == total,
            payload_length,
        };
        header.validate()?;
        Ok(header)
    }
    #[must_use]
    pub const fn credit(stream: Id128, acknowledged: u64) -> Self {
        Self {
            kind: PacketKind::Credit,
            stream,
            packet: acknowledged,
            record: 0,
            offset: 0,
            total: 0,
            record_kind: None,
            last: false,
            payload_length: 0,
        }
    }
    #[must_use]
    pub const fn detach(stream: Id128) -> Self {
        Self {
            kind: PacketKind::Detach,
            stream,
            packet: 0,
            record: 0,
            offset: 0,
            total: 0,
            record_kind: None,
            last: false,
            payload_length: 0,
        }
    }
    #[must_use]
    pub const fn kind(self) -> PacketKind {
        self.kind
    }
    #[must_use]
    pub const fn stream(self) -> Id128 {
        self.stream
    }
    #[must_use]
    pub const fn packet(self) -> u64 {
        self.packet
    }
    #[must_use]
    pub const fn record(self) -> u64 {
        self.record
    }
    #[must_use]
    pub const fn offset(self) -> u64 {
        self.offset
    }
    #[must_use]
    pub const fn total(self) -> u64 {
        self.total
    }
    #[must_use]
    pub const fn record_kind(self) -> Option<RecordKind> {
        self.record_kind
    }
    #[must_use]
    pub const fn is_last(self) -> bool {
        self.last
    }
    #[must_use]
    pub const fn payload_length(self) -> u32 {
        self.payload_length
    }
    /// # Errors
    /// Rejects payload mismatch and actual allocation failure.
    pub fn encode(self, payload: &[u8]) -> Result<Vec<u8>, WireError> {
        self.validate()?;
        if payload.len() != self.payload_length as usize {
            return Err(WireError::Length);
        }
        let mut out = Vec::new();
        out.try_reserve_exact(HEADER_BYTES + payload.len())
            .map_err(|_| WireError::Allocation)?;
        out.resize(HEADER_BYTES, 0);
        out[..4].copy_from_slice(b"ACPV");
        out[4] = 1;
        out[5] = match self.kind {
            PacketKind::Data => 1,
            PacketKind::Credit => 2,
            PacketKind::Detach => 3,
        };
        out[7] = u8::from(self.last);
        out[8..24].copy_from_slice(self.stream.as_bytes());
        for (offset, value) in [
            (24, self.packet),
            (32, self.record),
            (40, self.offset),
            (48, self.total),
        ] {
            out[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
        }
        out[56] = match self.record_kind {
            None => 0,
            Some(RecordKind::OriginalRaw) => 1,
            Some(RecordKind::ItemProjection) => 2,
            Some(RecordKind::Status) => 3,
            Some(RecordKind::SnapshotEnd) => 4,
            Some(RecordKind::TurnStart) => 5,
            Some(RecordKind::FieldBytes) => 6,
        };
        out[64..68].copy_from_slice(&self.payload_length.to_be_bytes());
        out.extend_from_slice(payload);
        Ok(out)
    }
    /// # Errors
    /// Rejects malformed headers before returning the original opaque payload slice.
    pub fn decode(bytes: &[u8]) -> Result<(Self, &[u8]), WireError> {
        if bytes.len() < HEADER_BYTES
            || &bytes[..4] != b"ACPV"
            || bytes[4] != 1
            || bytes[6] != 0
            || bytes[7] & !1 != 0
            || bytes[57..64].iter().any(|b| *b != 0)
            || bytes[68..72].iter().any(|b| *b != 0)
        {
            return Err(WireError::Header);
        }
        let kind = match bytes[5] {
            1 => PacketKind::Data,
            2 => PacketKind::Credit,
            3 => PacketKind::Detach,
            _ => return Err(WireError::Header),
        };
        let stream = Id128::new(bytes[8..24].try_into().map_err(|_| WireError::Header)?);
        let read = |offset| {
            bytes
                .get(offset..offset + 8)
                .and_then(|b| b.try_into().ok())
                .map(u64::from_be_bytes)
                .ok_or(WireError::Header)
        };
        let record_kind = match bytes[56] {
            0 => None,
            1 => Some(RecordKind::OriginalRaw),
            2 => Some(RecordKind::ItemProjection),
            3 => Some(RecordKind::Status),
            4 => Some(RecordKind::SnapshotEnd),
            5 => Some(RecordKind::TurnStart),
            6 => Some(RecordKind::FieldBytes),
            _ => return Err(WireError::Header),
        };
        let header = Self {
            kind,
            stream,
            packet: read(24)?,
            record: read(32)?,
            offset: read(40)?,
            total: read(48)?,
            record_kind,
            last: bytes[7] == 1,
            payload_length: u32::from_be_bytes(
                bytes[64..68].try_into().map_err(|_| WireError::Header)?,
            ),
        };
        header.validate()?;
        if bytes.len() != HEADER_BYTES + header.payload_length as usize {
            return Err(WireError::Length);
        }
        Ok((header, &bytes[HEADER_BYTES..]))
    }
    fn validate(self) -> Result<(), WireError> {
        if self.payload_length as usize > MAX_PAYLOAD_BYTES {
            return Err(WireError::Length);
        }
        if self.kind != PacketKind::Data {
            if self.record != 0
                || self.offset != 0
                || self.total != 0
                || self.record_kind.is_some()
                || self.last
                || self.payload_length != 0
                || (self.kind == PacketKind::Detach && self.packet != 0)
            {
                return Err(WireError::Header);
            }
            return Ok(());
        }
        if self.packet == 0 || self.record == 0 || self.record_kind.is_none() {
            return Err(WireError::Header);
        }
        let end = self
            .offset
            .checked_add(u64::from(self.payload_length))
            .ok_or(WireError::Position)?;
        if end > self.total || self.last != (end == self.total) {
            return Err(WireError::Position);
        }
        if self.record_kind == Some(RecordKind::FieldBytes) && self.total <= 8 {
            return Err(WireError::Length);
        }
        if self.record_kind == Some(RecordKind::SnapshotEnd) {
            if self.total != 0 || self.offset != 0 || self.payload_length != 0 {
                return Err(WireError::Length);
            }
        } else if self.total == 0 || self.payload_length == 0 {
            return Err(WireError::Length);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_view_wire_roundtrip_preserves_large_u64_and_opaque_utf8_fragments() {
        let id = Id128::new([4; 16]);
        let payload = [0x80, 0xf0, 0x9f, 0x92];
        let h = TransferHeader::data(
            id,
            9_007_199_254_740_993,
            5,
            u64::MAX - 4,
            u64::MAX,
            RecordKind::OriginalRaw,
            4,
        )
        .expect("checked values");
        let wire = h.encode(&payload).expect("encode");
        let (parsed, bytes) = TransferHeader::decode(&wire).expect("decode");
        assert_eq!(parsed, h);
        assert_eq!(bytes, payload);
        assert_eq!(wire.len(), 76);
    }
    #[test]
    fn shared_view_wire_credit_detach_and_empty_snapshot_have_exact_distinct_shapes() {
        let id = Id128::new([5; 16]);
        for h in [
            TransferHeader::credit(id, 0),
            TransferHeader::credit(id, 999),
            TransferHeader::detach(id),
            TransferHeader::data(id, 1, 1, 0, 0, RecordKind::SnapshotEnd, 0)
                .expect("empty snapshot"),
        ] {
            let bytes = h.encode(&[]).expect("encode");
            assert_eq!(bytes.len(), 72);
            assert_eq!(TransferHeader::decode(&bytes).expect("decode").0, h);
        }
        assert_eq!(
            TransferHeader::data(id, 1, 1, 0, 0, RecordKind::OriginalRaw, 0),
            Err(WireError::Length)
        );
        assert_eq!(
            TransferHeader::data(id, 1, 1, u64::MAX, 0, RecordKind::OriginalRaw, 1),
            Err(WireError::Position)
        );
    }
    #[test]
    fn shared_view_wire_rejects_every_reserved_field_and_inconsistent_shape() {
        let h = TransferHeader::data(Id128::new([6; 16]), 1, 1, 0, 1, RecordKind::OriginalRaw, 1)
            .expect("header");
        let good = h.encode(b"x").expect("encode");
        for index in [
            0, 4, 5, 6, 7, 56, 57, 58, 59, 60, 61, 62, 63, 68, 69, 70, 71,
        ] {
            let mut bad = good.clone();
            bad[index] = 255;
            assert!(TransferHeader::decode(&bad).is_err(), "field {index}");
        }
        for length in 0..72 {
            assert!(TransferHeader::decode(&good[..length]).is_err());
        }
        let mut bad = good;
        bad.push(0);
        assert_eq!(TransferHeader::decode(&bad), Err(WireError::Length));
    }
}
