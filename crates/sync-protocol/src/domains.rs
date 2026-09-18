//! Sync 的 transcript domain/tag 表（`docs/SYNC_PROTOCOL.md` §6.3）。
//!
//! 表必须与 `compatibility/transcripts/v1/transcripts.json` 中 `protocol == "sync"` 的条目
//! 逐项相等，由 `tests/tables_match_registry.rs` 断言；编解码与宽度校验在叶子 crate
//! `acpr-transcript` 的 `table` 模块，本 crate 不重复实现。

use acpr_transcript::table::{DomainSpec, FieldSpec, FieldType, Proof};

/// docs/SYNC_PROTOCOL.md §6.3 的登记表，顺序与 registry 一致。
pub const DOMAINS: [DomainSpec; 6] = [
    DomainSpec {
        domain: "acp-remote/pairing-proof/v1",
        proof: Proof::Hmac,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "hostId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "deviceId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 4,
                name: "pairingId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 5,
                name: "pairingExpiresAt",
                ty: FieldType::U64be,
            },
            FieldSpec {
                tag: 6,
                name: "canonicalOrigin",
                ty: FieldType::Utf8,
            },
            FieldSpec {
                tag: 8,
                name: "devicePublicKey",
                ty: FieldType::Sec1_65,
            },
            FieldSpec {
                tag: 9,
                name: "clientNonce",
                ty: FieldType::Bytes32,
            },
            FieldSpec {
                tag: 14,
                name: "deviceName",
                ty: FieldType::Utf8,
            },
            FieldSpec {
                tag: 15,
                name: "clientKind",
                ty: FieldType::Utf8,
            },
        ],
    },
    DomainSpec {
        domain: "acp-remote/pairing-host-proof/v1",
        proof: Proof::Signature,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "hostId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "deviceId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 4,
                name: "pairingId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 6,
                name: "canonicalOrigin",
                ty: FieldType::Utf8,
            },
            FieldSpec {
                tag: 7,
                name: "hostPublicKey",
                ty: FieldType::Sec1_65,
            },
            FieldSpec {
                tag: 8,
                name: "devicePublicKey",
                ty: FieldType::Sec1_65,
            },
            FieldSpec {
                tag: 9,
                name: "clientNonce",
                ty: FieldType::Bytes32,
            },
            FieldSpec {
                tag: 10,
                name: "serverNonce",
                ty: FieldType::Bytes32,
            },
            FieldSpec {
                tag: 13,
                name: "pairingRequestId",
                ty: FieldType::Uuid16,
            },
        ],
    },
    DomainSpec {
        domain: "acp-remote/pairing-sas/v1",
        proof: Proof::Hmac,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "hostId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "deviceId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 4,
                name: "pairingId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 6,
                name: "canonicalOrigin",
                ty: FieldType::Utf8,
            },
            FieldSpec {
                tag: 7,
                name: "hostPublicKey",
                ty: FieldType::Sec1_65,
            },
            FieldSpec {
                tag: 8,
                name: "devicePublicKey",
                ty: FieldType::Sec1_65,
            },
            FieldSpec {
                tag: 9,
                name: "clientNonce",
                ty: FieldType::Bytes32,
            },
            FieldSpec {
                tag: 10,
                name: "serverNonce",
                ty: FieldType::Bytes32,
            },
            FieldSpec {
                tag: 13,
                name: "pairingRequestId",
                ty: FieldType::Uuid16,
            },
        ],
    },
    DomainSpec {
        domain: "acp-remote/pairing-status/v1",
        proof: Proof::Hmac,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "hostId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "deviceId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 4,
                name: "pairingId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 13,
                name: "pairingRequestId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 16,
                name: "requestNonce",
                ty: FieldType::Bytes32,
            },
        ],
    },
    DomainSpec {
        domain: "acp-remote/host-challenge/v1",
        proof: Proof::Signature,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "hostId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "deviceId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 6,
                name: "canonicalOrigin",
                ty: FieldType::Utf8,
            },
            FieldSpec {
                tag: 9,
                name: "clientNonce",
                ty: FieldType::Bytes32,
            },
            FieldSpec {
                tag: 10,
                name: "serverNonce",
                ty: FieldType::Bytes32,
            },
            FieldSpec {
                tag: 11,
                name: "connectionId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 12,
                name: "negotiatedFeatures",
                ty: FieldType::NulJoined,
            },
        ],
    },
    DomainSpec {
        domain: "acp-remote/device-proof/v1",
        proof: Proof::Signature,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "hostId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "deviceId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 6,
                name: "canonicalOrigin",
                ty: FieldType::Utf8,
            },
            FieldSpec {
                tag: 9,
                name: "clientNonce",
                ty: FieldType::Bytes32,
            },
            FieldSpec {
                tag: 10,
                name: "serverNonce",
                ty: FieldType::Bytes32,
            },
            FieldSpec {
                tag: 11,
                name: "connectionId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 12,
                name: "negotiatedFeatures",
                ty: FieldType::NulJoined,
            },
        ],
    },
];
