//! [PV3] / 任务 2.4：12 个已登记 domain 的固定向量逐字节重算、验签、HMAC、SAS 与畸形输入负例。
//!
//! 这些用例是合同 §3 第 5 条要求的**常驻回归测试**：向量来自 `fixtures/{sync,node-link}/v1/transcripts/`，
//! 装配只经协议 crate 的表（`design.md` D2），因此「表写错」「字段顺序写错」「宽度写错」都会在这里红。

mod support;

use acp_core::model::{NodeKind, Nonce, PeerPublicKey, Timestamp};
use identity_auth::transcript::{
    NodeLinkChallenge, NodeLinkPairingOwnerProof, NodeLinkPairingProof, NodeLinkPairingSas,
    NodeLinkPairingStatus, NodeLinkProof, SyncDeviceProof, SyncHostChallenge, SyncPairingHostProof,
    SyncPairingProof, SyncPairingSas, SyncPairingStatus,
};
use identity_auth::{
    CanonicalOrigin, ChallengeId, ClientKind, FeatureList, P1363Signature, PairingProof,
    PairingRequestId, PairingSecret, Sas,
};
use serde_json::Value;

/// 读取固定向量：路径相对仓库根（`include_str!` 相对本源文件）。
macro_rules! vector {
    ($rel:literal) => {
        include_str!(concat!("../../../fixtures/", $rel))
    };
}

const SYNC_VECTORS: &[&str] = &[
    vector!("sync/v1/transcripts/pairing-proof.json"),
    vector!("sync/v1/transcripts/pairing-host-proof.json"),
    vector!("sync/v1/transcripts/pairing-sas.json"),
    vector!("sync/v1/transcripts/pairing-status.json"),
    vector!("sync/v1/transcripts/host-challenge.json"),
    vector!("sync/v1/transcripts/device-proof.json"),
];

const NODE_LINK_VECTORS: &[&str] = &[
    vector!("node-link/v1/transcripts/pairing-proof.json"),
    vector!("node-link/v1/transcripts/pairing-owner-proof.json"),
    vector!("node-link/v1/transcripts/pairing-sas.json"),
    vector!("node-link/v1/transcripts/pairing-status.json"),
    vector!("node-link/v1/transcripts/challenge.json"),
    vector!("node-link/v1/transcripts/node-proof.json"),
];

const SYNC_INVALID: &[&str] = &[
    vector!("sync/v1/transcripts/invalid/bad-codec-version.json"),
    vector!("sync/v1/transcripts/invalid/bad-magic.json"),
    vector!("sync/v1/transcripts/invalid/duplicate-tag.json"),
    vector!("sync/v1/transcripts/invalid/field-order.json"),
    vector!("sync/v1/transcripts/invalid/length-mismatch.json"),
    vector!("sync/v1/transcripts/invalid/public-key-bad-base64url.json"),
    vector!("sync/v1/transcripts/invalid/public-key-bad-length.json"),
    vector!("sync/v1/transcripts/invalid/public-key-bad-point.json"),
    vector!("sync/v1/transcripts/invalid/public-key-not-on-curve.json"),
    vector!("sync/v1/transcripts/invalid/trailing-bytes.json"),
    vector!("sync/v1/transcripts/invalid/truncated-domain.json"),
    vector!("sync/v1/transcripts/invalid/truncated-field.json"),
    vector!("sync/v1/transcripts/invalid/unknown-tag.json"),
];

const NODE_LINK_INVALID: &[&str] = &[
    vector!("node-link/v1/transcripts/invalid/bad-codec-version.json"),
    vector!("node-link/v1/transcripts/invalid/bad-magic.json"),
    vector!("node-link/v1/transcripts/invalid/duplicate-tag.json"),
    vector!("node-link/v1/transcripts/invalid/field-order.json"),
    vector!("node-link/v1/transcripts/invalid/public-key-bad-length.json"),
    vector!("node-link/v1/transcripts/invalid/public-key-not-on-curve.json"),
    vector!("node-link/v1/transcripts/invalid/unknown-tag.json"),
];

fn value(text: &str) -> Value {
    serde_json::from_str(text).expect("固定向量必须是合法 JSON")
}

fn text_of(input: &Value, key: &str) -> String {
    input
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("固定向量缺少字段 {key}"))
        .to_owned()
}

fn uuid_of(input: &Value, key: &str) -> String {
    let text = text_of(input, key);
    assert_eq!(text.len(), 36, "固定向量 {key} 必须是 canonical UUID");
    text
}

/// `*Hex` 形态的 32 字节 nonce（签名域与状态域使用）。
fn nonce_of(input: &Value, key: &str) -> Nonce {
    let bytes = support::hex(&text_of(input, key));
    assert_eq!(bytes.len(), 32, "固定向量的 nonce 必须是 32 字节");
    Nonce::new(&acpr_transcript::encode_base64url(&bytes)).expect("编码后的 nonce 必须规范")
}

fn features_of(input: &Value, key: &str) -> FeatureList {
    let list = input
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("固定向量缺少字段 {key}"));
    FeatureList::new(
        list.iter()
            .map(|item| item.as_str().unwrap_or_default().to_owned()),
    )
}

fn key_of(text: &str) -> PeerPublicKey {
    let bytes = acpr_transcript::decode_base64url(text).expect("固定向量公钥必须是规范 base64url");
    PeerPublicKey::try_from_bytes(&bytes).expect("固定向量公钥必须是 65 字节 SEC1 点")
}

fn unix_of(input: &Value, key: &str) -> Timestamp {
    let seconds = input
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("固定向量缺少字段 {key}"));
    identity_auth::timestamp_from_unix_seconds(seconds).expect("固定向量时间戳必须可换算")
}

fn origin_of(input: &Value) -> CanonicalOrigin {
    CanonicalOrigin::parse(&text_of(input, "canonicalOrigin")).expect("固定向量的 origin 必须规范")
}

fn request_id_of(input: &Value) -> PairingRequestId {
    PairingRequestId::parse(&uuid_of(input, "pairingRequestId")).expect("规范 pairingRequestId")
}

fn connection_id_of(input: &Value) -> ChallengeId {
    ChallengeId::parse(&uuid_of(input, "connectionId")).expect("规范 connectionId")
}

/// 12 个 domain 的输入结构（每个分支只出现一次，装配与校验都在这里分发）。
enum Vector {
    SyncPairingProof(SyncPairingProof),
    SyncPairingHostProof(SyncPairingHostProof),
    SyncPairingSas(SyncPairingSas),
    SyncPairingStatus(SyncPairingStatus),
    SyncHostChallenge(SyncHostChallenge),
    SyncDeviceProof(SyncDeviceProof),
    NodeLinkPairingProof(NodeLinkPairingProof),
    NodeLinkPairingOwnerProof(NodeLinkPairingOwnerProof),
    NodeLinkPairingSas(NodeLinkPairingSas),
    NodeLinkPairingStatus(NodeLinkPairingStatus),
    NodeLinkChallenge(NodeLinkChallenge),
    NodeLinkProof(NodeLinkProof),
}

impl Vector {
    /// 按 domain 装配（domain 字符串来自向量，不写死在本 crate）。
    fn build(domain: &str, input: &Value) -> Self {
        match domain {
            "acp-remote/pairing-proof/v1" => Self::SyncPairingProof(SyncPairingProof {
                host_id: support::node(&uuid_of(input, "hostId")),
                device_id: support::device(&uuid_of(input, "deviceId")),
                pairing_id: support::pairing(&uuid_of(input, "pairingId")),
                pairing_expires_at: unix_of(input, "pairingExpiresAtUnixSeconds"),
                canonical_origin: origin_of(input),
                device_public_key: key_of(&text_of(input, "devicePublicKey")),
                client_nonce: nonce_of(input, "clientNonceHex"),
                device_name: text_of(input, "deviceName"),
                client_kind: text_of(input, "clientKind")
                    .parse::<ClientKind>()
                    .expect("固定向量的 clientKind 必须是词表成员"),
            }),
            "acp-remote/pairing-host-proof/v1" => {
                Self::SyncPairingHostProof(SyncPairingHostProof {
                    host_id: support::node(&uuid_of(input, "hostId")),
                    device_id: support::device(&uuid_of(input, "deviceId")),
                    pairing_id: support::pairing(&uuid_of(input, "pairingId")),
                    canonical_origin: origin_of(input),
                    host_public_key: key_of(&text_of(input, "hostPublicKey")),
                    device_public_key: key_of(&text_of(input, "devicePublicKey")),
                    client_nonce: nonce_of(input, "clientNonceHex"),
                    server_nonce: nonce_of(input, "serverNonceHex"),
                    pairing_request_id: request_id_of(input),
                })
            }
            "acp-remote/pairing-sas/v1" => Self::SyncPairingSas(SyncPairingSas {
                host_id: support::node(&uuid_of(input, "hostId")),
                device_id: support::device(&uuid_of(input, "deviceId")),
                pairing_id: support::pairing(&uuid_of(input, "pairingId")),
                canonical_origin: origin_of(input),
                host_public_key: key_of(&text_of(input, "hostPublicKey")),
                device_public_key: key_of(&text_of(input, "devicePublicKey")),
                client_nonce: nonce_of(input, "clientNonceHex"),
                server_nonce: nonce_of(input, "serverNonceHex"),
                pairing_request_id: request_id_of(input),
            }),
            "acp-remote/pairing-status/v1" => Self::SyncPairingStatus(SyncPairingStatus {
                host_id: support::node(&uuid_of(input, "hostId")),
                device_id: support::device(&uuid_of(input, "deviceId")),
                pairing_id: support::pairing(&uuid_of(input, "pairingId")),
                pairing_request_id: request_id_of(input),
                request_nonce: nonce_of(input, "requestNonceHex"),
            }),
            "acp-remote/host-challenge/v1" => Self::SyncHostChallenge(SyncHostChallenge {
                host_id: support::node(&uuid_of(input, "hostId")),
                device_id: support::device(&uuid_of(input, "deviceId")),
                canonical_origin: origin_of(input),
                client_nonce: nonce_of(input, "clientNonceHex"),
                server_nonce: nonce_of(input, "serverNonceHex"),
                connection_id: connection_id_of(input),
                negotiated_features: features_of(input, "negotiatedFeatures"),
            }),
            "acp-remote/device-proof/v1" => Self::SyncDeviceProof(SyncDeviceProof {
                host_id: support::node(&uuid_of(input, "hostId")),
                device_id: support::device(&uuid_of(input, "deviceId")),
                canonical_origin: origin_of(input),
                client_nonce: nonce_of(input, "clientNonceHex"),
                server_nonce: nonce_of(input, "serverNonceHex"),
                connection_id: connection_id_of(input),
                negotiated_features: features_of(input, "negotiatedFeatures"),
            }),
            "acp-remote/node-link-pairing-proof/v1" => {
                Self::NodeLinkPairingProof(NodeLinkPairingProof {
                    owner_node_id: support::node(&uuid_of(input, "ownerNodeId")),
                    access_node_id: support::node(&uuid_of(input, "accessNodeId")),
                    pairing_id: support::pairing(&uuid_of(input, "pairingId")),
                    pairing_expires_at: unix_of(input, "pairingExpiresAt"),
                    access_public_key: support::key_from_hex(&text_of(input, "accessPublicKeyHex")),
                    client_nonce: nonce_of(input, "clientNonceHex"),
                    node_name: text_of(input, "nodeName"),
                    node_kind: text_of(input, "nodeKind")
                        .parse::<NodeKind>()
                        .expect("固定向量的 nodeKind 必须是词表成员"),
                })
            }
            "acp-remote/node-link-pairing-owner-proof/v1" => {
                Self::NodeLinkPairingOwnerProof(NodeLinkPairingOwnerProof {
                    owner_node_id: support::node(&uuid_of(input, "ownerNodeId")),
                    access_node_id: support::node(&uuid_of(input, "accessNodeId")),
                    pairing_id: support::pairing(&uuid_of(input, "pairingId")),
                    owner_public_key: support::key_from_hex(&text_of(input, "ownerPublicKeyHex")),
                    access_public_key: support::key_from_hex(&text_of(input, "accessPublicKeyHex")),
                    client_nonce: nonce_of(input, "clientNonceHex"),
                    server_nonce: nonce_of(input, "serverNonceHex"),
                    pairing_request_id: request_id_of(input),
                })
            }
            "acp-remote/node-link-pairing-sas/v1" => Self::NodeLinkPairingSas(NodeLinkPairingSas {
                owner_node_id: support::node(&uuid_of(input, "ownerNodeId")),
                access_node_id: support::node(&uuid_of(input, "accessNodeId")),
                pairing_id: support::pairing(&uuid_of(input, "pairingId")),
                owner_public_key: support::key_from_hex(&text_of(input, "ownerPublicKeyHex")),
                access_public_key: support::key_from_hex(&text_of(input, "accessPublicKeyHex")),
                client_nonce: nonce_of(input, "clientNonceHex"),
                server_nonce: nonce_of(input, "serverNonceHex"),
                pairing_request_id: request_id_of(input),
            }),
            "acp-remote/node-link-pairing-status/v1" => {
                Self::NodeLinkPairingStatus(NodeLinkPairingStatus {
                    owner_node_id: support::node(&uuid_of(input, "ownerNodeId")),
                    access_node_id: support::node(&uuid_of(input, "accessNodeId")),
                    pairing_id: support::pairing(&uuid_of(input, "pairingId")),
                    pairing_request_id: request_id_of(input),
                    request_nonce: nonce_of(input, "requestNonceHex"),
                })
            }
            "acp-remote/node-link-challenge/v1" => Self::NodeLinkChallenge(NodeLinkChallenge {
                owner_node_id: support::node(&uuid_of(input, "ownerNodeId")),
                access_node_id: support::node(&uuid_of(input, "accessNodeId")),
                catalog_revision: input["catalogRevision"].as_u64().expect("catalogRevision"),
                client_nonce: nonce_of(input, "clientNonceHex"),
                server_nonce: nonce_of(input, "serverNonceHex"),
                connection_id: connection_id_of(input),
                negotiated_features: features_of(input, "negotiatedFeatures"),
            }),
            "acp-remote/node-link-proof/v1" => Self::NodeLinkProof(NodeLinkProof {
                owner_node_id: support::node(&uuid_of(input, "ownerNodeId")),
                access_node_id: support::node(&uuid_of(input, "accessNodeId")),
                catalog_revision: input["catalogRevision"].as_u64().expect("catalogRevision"),
                client_nonce: nonce_of(input, "clientNonceHex"),
                server_nonce: nonce_of(input, "serverNonceHex"),
                connection_id: connection_id_of(input),
                negotiated_features: features_of(input, "negotiatedFeatures"),
            }),
            other => panic!("固定向量里的 domain 未在装配分支中登记：{other}"),
        }
    }

    /// 装配后的 transcript 字节（比对固定向量的唯一输入）。
    fn transcript(&self) -> Vec<u8> {
        match self {
            Self::SyncPairingProof(input) => input.transcript(),
            Self::SyncPairingHostProof(input) => input.transcript(),
            Self::SyncPairingSas(input) => input.transcript(),
            Self::SyncPairingStatus(input) => input.transcript(),
            Self::SyncHostChallenge(input) => input.transcript(),
            Self::SyncDeviceProof(input) => input.transcript(),
            Self::NodeLinkPairingProof(input) => input.transcript(),
            Self::NodeLinkPairingOwnerProof(input) => input.transcript(),
            Self::NodeLinkPairingSas(input) => input.transcript(),
            Self::NodeLinkPairingStatus(input) => input.transcript(),
            Self::NodeLinkChallenge(input) => input.transcript(),
            Self::NodeLinkProof(input) => input.transcript(),
        }
        .expect("装配必须成功")
    }

    /// HMAC 域的校验（签名域返回 `None`）。
    fn verify_hmac(&self, secret: &PairingSecret, proof: &PairingProof) -> Option<bool> {
        let result = match self {
            Self::SyncPairingProof(input) => input.verify(secret, proof),
            Self::SyncPairingStatus(input) => input.verify(secret, proof),
            Self::NodeLinkPairingProof(input) => input.verify(secret, proof),
            Self::NodeLinkPairingStatus(input) => input.verify(secret, proof),
            _ => return None,
        };
        Some(result.is_ok())
    }

    /// 签名域的验签（HMAC 域返回 `None`）。
    fn verify_signature(&self, key: &PeerPublicKey, signature: &P1363Signature) -> Option<bool> {
        let result = match self {
            Self::SyncPairingHostProof(input) => input.verify(key, signature),
            Self::SyncHostChallenge(input) => input.verify(key, signature),
            Self::SyncDeviceProof(input) => input.verify(key, signature),
            Self::NodeLinkPairingOwnerProof(input) => input.verify(key, signature),
            Self::NodeLinkChallenge(input) => input.verify(key, signature),
            Self::NodeLinkProof(input) => input.verify(key, signature),
            _ => return None,
        };
        Some(result.is_ok())
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(bytes);
    let mut text = String::with_capacity(64);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// 逐字节重算 + 摘要 + HMAC/验签 + SAS：12 个向量共用一条断言路径。
fn assert_vector(document: &Value) {
    let domain = document["domain"].as_str().expect("向量必须有 domain");
    let input = &document["input"];
    let expected = &document["expected"];
    let vector = Vector::build(domain, input);
    let transcript = vector.transcript();
    assert_eq!(
        acpr_transcript::encode_base64url(&transcript),
        expected["transcriptBase64url"]
            .as_str()
            .expect("向量必须有 transcript"),
        "{domain}: transcript 必须逐字节等于固定向量"
    );
    assert_eq!(
        sha256_hex(&transcript),
        expected["transcriptSha256Hex"]
            .as_str()
            .expect("向量必须有摘要"),
        "{domain}: transcript 摘要必须等于固定向量"
    );

    if let Some(hmac) = expected.get("hmacSha256").and_then(Value::as_str) {
        let key = acpr_transcript::decode_base64url(
            expected["hmacKeyBase64url"]
                .as_str()
                .expect("HMAC 向量必须有 key"),
        )
        .expect("HMAC key 必须是规范 base64url");
        let secret = PairingSecret::try_from_bytes(&key).expect("HMAC key 必须是 32 字节");
        let proof = PairingProof::try_from_base64url(hmac).expect("HMAC 必须规范");
        if let Some(verified) = vector.verify_hmac(&secret, &proof) {
            assert!(verified, "{domain}: 固定向量的 HMAC 必须验证通过");
        }
        // 不同的 secret 必须不通过（避免「只比较长度」这类退化实现）。
        let other = PairingSecret::try_from_bytes(&[0u8; 32]).expect("32 字节");
        if vector.verify_hmac(&secret, &proof).is_some() {
            assert_eq!(
                vector.verify_hmac(&other, &proof),
                Some(false),
                "{domain}: 换一把 secret 必须校验失败"
            );
        }
        if let Some(sas) = expected.get("sas").and_then(Value::as_str) {
            let derived =
                identity_auth::derive_sas(&transcript, &secret).expect("SAS 派生必须成功");
            assert_eq!(derived.as_str(), sas, "{domain}: SAS 必须等于固定向量");
            assert_eq!(
                derived,
                Sas::from_hmac_output(&key_to_output(hmac)).expect("SAS 形状"),
                "{domain}: SAS 只由 HMAC 输出前 4 字节派生"
            );
        }
    }
    if let Some(signature) = expected.get("p1363Signature").and_then(Value::as_str) {
        let public_key = key_of(
            expected["publicKey"]
                .as_str()
                .expect("签名向量必须有 publicKey"),
        );
        let signature = P1363Signature::try_from_base64url(signature).expect("64 字节 P1363");
        assert_eq!(
            vector.verify_signature(&public_key, &signature),
            Some(true),
            "{domain}: 固定向量的签名必须验证通过"
        );
        // 换一把公钥（同曲线、合法点）必须不通过。
        let other = FakePublicKey::other();
        assert_eq!(
            vector.verify_signature(&other, &signature),
            Some(false),
            "{domain}: 换一把公钥必须验证失败"
        );
    }
}

/// 由向量里的 HMAC 文本还原 32 字节输出（SAS 派生的一致性检查用）。
fn key_to_output(hmac: &str) -> [u8; 32] {
    acpr_transcript::decode_base64url(hmac)
        .expect("HMAC 必须是规范 base64url")
        .try_into()
        .expect("HMAC-SHA256 输出必须是 32 字节")
}

/// 另一把合法公钥（负例用）。
enum FakePublicKey {}

impl FakePublicKey {
    fn other() -> PeerPublicKey {
        // 由固定标量 0x0a.. 导出的公钥；与向量里的任何签名都不匹配。
        let bytes =
            support::hex("0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a");
        let signing = p256::ecdsa::SigningKey::from_slice(&bytes).expect("合法标量");
        let point = p256::ecdsa::VerifyingKey::from(&signing).to_encoded_point(false);
        PeerPublicKey::try_from_bytes(point.as_bytes()).expect("导出的公钥必须合法")
    }
}

#[test]
fn sync_fixed_vectors_round_trip() {
    for document in SYNC_VECTORS.iter().map(|text| value(text)) {
        assert_vector(&document);
    }
}

#[test]
fn node_link_fixed_vectors_round_trip() {
    for document in NODE_LINK_VECTORS.iter().map(|text| value(text)) {
        assert_vector(&document);
    }
}

#[test]
fn registry_and_fixture_domains_agree() {
    // 12 个向量必须覆盖 12 个已登记 domain（少一个就说明向量或表有一侧缺项）。
    let mut domains: Vec<String> = SYNC_VECTORS
        .iter()
        .chain(NODE_LINK_VECTORS.iter())
        .map(|text| value(text)["domain"].as_str().unwrap().to_owned())
        .collect();
    domains.sort();
    domains.dedup();
    assert_eq!(domains.len(), 12, "固定向量必须覆盖全部 12 个 domain");
}

#[test]
fn malformed_transcripts_are_rejected() {
    for document in SYNC_INVALID
        .iter()
        .chain(NODE_LINK_INVALID.iter())
        .map(|text| value(text))
    {
        let description = document["description"].as_str().unwrap_or("invalid 向量");
        if let Some(text) = document
            .get("malformedTranscriptBase64url")
            .and_then(Value::as_str)
        {
            let bytes = acpr_transcript::decode_base64url(text).expect("文本本身是规范 base64url");
            // 宽度与 tag 成员只有对照登记表才能判定，因此这里用表驱动解码（与合同资材校验器同口径）。
            let domain = document["domain"]
                .as_str()
                .expect("结构类负例必须带 domain");
            let spec =
                acpr_transcript::table::domain_spec(&sync_protocol::domains::DOMAINS, domain)
                    .or_else(|| {
                        acpr_transcript::table::domain_spec(
                            &node_link_protocol::domains::DOMAINS,
                            domain,
                        )
                    })
                    .unwrap_or_else(|| panic!("负例 domain 必须在协议表中：{domain}"));
            assert!(
                acpr_transcript::table::decode_transcript(&bytes, spec).is_err(),
                "结构错误必须被表驱动解码器拒绝：{description}"
            );
            // 通用解码器也必须至少拒绝其中的结构错误（不得默默接受）。
            let _ = acpr_transcript::decode(&bytes);
        }
        if let Some(text) = document
            .get("malformedPublicKeyBase64url")
            .and_then(Value::as_str)
        {
            // 非规范 base64url 在解码这一步就被拒（本身就是「结构先于密码学」）；
            // 能解码时必须被 65 字节未压缩点断言或曲线校验拦住。
            let rejected = match acpr_transcript::decode_base64url(text) {
                Ok(bytes) => PeerPublicKey::try_from_bytes(&bytes).is_err(),
                Err(_) => true,
            };
            assert!(rejected, "非法公钥必须在结构层被拒绝：{description}");
        }
    }
}

#[test]
fn non_canonical_base64url_and_zero_components_are_rejected() {
    // 带填充的 base64url：长度断言先于密码学（合同 §5.2）。
    assert!(P1363Signature::try_from_base64url("AA==").is_err());
    // 非字母表字符。
    assert!(P1363Signature::try_from_base64url("+////w").is_err());
    // DER 形态的 71 字节签名不是 64 字节 → 结构错误。
    let der = vec![0u8; 71];
    assert!(P1363Signature::try_from_bytes(&der).is_err());
    // 零值分量：形状合法但不允许（由 `components_nonzero` 判定，见握手用例）。
    let zero = P1363Signature::try_from_bytes(&[0u8; 64]).expect("64 字节形状合法");
    assert!(!zero.components_nonzero(), "全零签名必须被判定为零值分量");
    let mut half_zero = [0u8; 64];
    half_zero[63] = 1;
    let half_zero = P1363Signature::try_from_bytes(&half_zero).expect("64 字节形状合法");
    assert!(
        !half_zero.components_nonzero(),
        "`r` 为零时必须被判定为零值分量"
    );
}
