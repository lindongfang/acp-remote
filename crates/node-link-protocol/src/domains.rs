//! Node Link 的 transcript domain/tag 表（`docs/NODE_LINK_PROTOCOL.md` §9.3/§9.4）。
//!
//! 表必须与 `compatibility/transcripts/v1/transcripts.json` 中 `protocol == "node_link"` 的条目
//! 逐项相等，由 `tests/tables_match_registry.rs` 断言；编解码与宽度校验在叶子 crate
//! `acpr-transcript` 的 `table` 模块，本 crate 不重复实现。

use acpr_transcript::table::{DomainSpec, FieldSpec, FieldType, Proof};

/// docs/NODE_LINK_PROTOCOL.md §9.3/§9.4 的登记表，顺序与 registry 一致。
pub const DOMAINS: [DomainSpec; 6] = [
    DomainSpec {
        domain: "acp-remote/node-link-pairing-proof/v1",
        proof: Proof::Hmac,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "ownerNodeId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "accessNodeId",
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
                tag: 8,
                name: "accessPublicKey",
                ty: FieldType::Sec1_65,
            },
            FieldSpec {
                tag: 9,
                name: "clientNonce",
                ty: FieldType::Bytes32,
            },
            FieldSpec {
                tag: 14,
                name: "nodeName",
                ty: FieldType::Utf8,
            },
            FieldSpec {
                tag: 15,
                name: "nodeKind",
                ty: FieldType::Utf8,
            },
        ],
    },
    DomainSpec {
        domain: "acp-remote/node-link-pairing-owner-proof/v1",
        proof: Proof::Signature,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "ownerNodeId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "accessNodeId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 4,
                name: "pairingId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 7,
                name: "ownerPublicKey",
                ty: FieldType::Sec1_65,
            },
            FieldSpec {
                tag: 8,
                name: "accessPublicKey",
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
        domain: "acp-remote/node-link-pairing-sas/v1",
        proof: Proof::Hmac,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "ownerNodeId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "accessNodeId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 4,
                name: "pairingId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 7,
                name: "ownerPublicKey",
                ty: FieldType::Sec1_65,
            },
            FieldSpec {
                tag: 8,
                name: "accessPublicKey",
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
        domain: "acp-remote/node-link-pairing-status/v1",
        proof: Proof::Hmac,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "ownerNodeId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "accessNodeId",
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
        domain: "acp-remote/node-link-challenge/v1",
        proof: Proof::Signature,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "ownerNodeId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "accessNodeId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 6,
                name: "catalogRevision",
                ty: FieldType::U64be,
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
        domain: "acp-remote/node-link-proof/v1",
        proof: Proof::Signature,
        fields: &[
            FieldSpec {
                tag: 1,
                name: "protocolVersion",
                ty: FieldType::U16be,
            },
            FieldSpec {
                tag: 2,
                name: "ownerNodeId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 3,
                name: "accessNodeId",
                ty: FieldType::Uuid16,
            },
            FieldSpec {
                tag: 6,
                name: "catalogRevision",
                ty: FieldType::U64be,
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
