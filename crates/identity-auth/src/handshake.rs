//! 逐连接 challenge-response（合同 §5）：挑战签发、证明校验与认证收尾。
//!
//! 每个新连接完整执行一次：`hello` 生成挑战与服务端 nonce 并用本节点身份签 `host-challenge`；
//! `verify_proof` **消费**挑战（未知/已消费/已过期一律同一类失败）、重新核对绑定、用**持久化信任里
//! 的公钥**验签，然后返回已验证事实与凭据状态。验签公钥绝不取握手消息自带的公钥（合同 §5.1）。

use acp_core::model::{Nonce, PairingId, PeerIdentity, Timestamp};

use crate::authority::Authority;
use crate::error::HandshakeError;
use crate::port::KeystoreError;
use crate::transcript::{
    FeatureList, NodeLinkChallenge, NodeLinkProof, SyncDeviceProof, SyncHostChallenge, add_seconds,
};
use crate::types::{
    Authenticated, CHALLENGE_TTL_SECONDS, ChallengeId, ConnectionBinding, ConnectionKind,
    HandshakeFailure, IdentityFact, P1363Signature, PeerTrust, at_or_after,
};

/// hello 校验与挑战签发的输入（**已解码**的结构化字段，调用方不拼字节）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeRequest {
    /// 连接类型。
    pub kind: ConnectionKind,
    /// 对端身份（未知 id 也必须能签发挑战）。
    pub peer: PeerIdentity,
    /// 连接维度的事实（设备侧是 canonical origin + Host，节点侧是 endpoint + Host）。
    pub binding: ConnectionBinding,
    /// 对端在 hello 里提交的 client nonce（进入证明 transcript，必须原样记住）。
    pub client_nonce: Nonce,
    /// 协商结果（**由 adapter 决定**：它拥有 feature 词表与 required 语义；本层只写进 transcript）。
    pub negotiated_features: FeatureList,
    /// Node Link 的目录修订号（`node-link-challenge` 的 `catalogRevision`；Sync 侧必须为 `None`）。
    pub catalog_revision: Option<u64>,
}

/// 挑战签发结果：adapter 把它编码成 `auth.server_challenge` / `node.challenge`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeIssue {
    /// 连接标识（wire 的 `connectionId`，也是本层挑战缓存的一次性键）。
    pub challenge_id: ChallengeId,
    /// 服务端 nonce。
    pub server_nonce: Nonce,
    /// 挑战过期时间（固定 15 秒窗口）。
    pub expires_at: Timestamp,
    /// 本节点身份对宿主/owner 证明 transcript 的签名。
    pub host_proof: P1363Signature,
}

/// 证明校验的输入（**已解码**的结构化字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofSubmission {
    /// 连接类型。
    pub kind: ConnectionKind,
    /// 挑战标识（wire 的 `connectionId`）。
    pub challenge_id: ChallengeId,
    /// 服务端 nonce 回显。
    pub server_nonce: Nonce,
    /// 对端身份。
    pub peer: PeerIdentity,
    /// 对端 nonce 回显。
    pub client_nonce: Nonce,
    /// 对端对 proof transcript 的签名（64 字节 P1363）。
    pub signature: P1363Signature,
}

impl Authority {
    /// 校验 hello 并签发挑战。
    ///
    /// 未知对端（`trust.public_key` 为 `None`）**也会**拿到结构完整的挑战：不得用错误区分对端是否存在。
    /// 挑战缓存只在内存，进程重启即丢弃；签名先于缓存写入，失败不留半成品。
    pub async fn hello(
        &self,
        request: &ChallengeRequest,
        trust: &PeerTrust,
    ) -> Result<ChallengeIssue, HandshakeError> {
        if !peer_kind_matches(request.kind, &request.peer) {
            return Err(HandshakeError::TrustKind);
        }
        self.check_binding(request.kind, &request.binding, trust)?;
        match (request.kind, request.catalog_revision) {
            (ConnectionKind::SyncDevice, None) | (ConnectionKind::NodeLink, Some(_)) => {}
            _ => return Err(HandshakeError::MissingCatalogRevision),
        }

        let now = self.now();
        let expires_at = add_seconds(&now, CHALLENGE_TTL_SECONDS)?;
        let server_nonce = random_nonce_for(self.entropy())?;
        let challenge_id = ChallengeId::generate(self.entropy())
            .map_err(|_| HandshakeError::EntropyUnavailable)?;

        // 先签名（唯一可能失败的 `await`），成功后才登记挑战。
        let host_proof = match (request.kind, &request.peer) {
            (ConnectionKind::SyncDevice, PeerIdentity::Device(device)) => {
                let Some(canonical_origin) = request.binding.canonical_origin.clone() else {
                    return Err(HandshakeError::BindingKind);
                };
                let input = SyncHostChallenge {
                    host_id: self.local_node().clone(),
                    device_id: device.clone(),
                    canonical_origin,
                    client_nonce: request.client_nonce.clone(),
                    server_nonce: server_nonce.clone(),
                    connection_id: challenge_id.clone(),
                    negotiated_features: request.negotiated_features.clone(),
                };
                let transcript = input.transcript().map_err(HandshakeError::Transcript)?;
                self.sign_transcript(&transcript)
                    .await
                    .map_err(map_keystore)?
            }
            (ConnectionKind::NodeLink, PeerIdentity::Node(access)) => {
                let Some(catalog_revision) = request.catalog_revision else {
                    return Err(HandshakeError::MissingCatalogRevision);
                };
                let input = NodeLinkChallenge {
                    owner_node_id: self.local_node().clone(),
                    access_node_id: access.clone(),
                    catalog_revision,
                    client_nonce: request.client_nonce.clone(),
                    server_nonce: server_nonce.clone(),
                    connection_id: challenge_id.clone(),
                    negotiated_features: request.negotiated_features.clone(),
                };
                let transcript = input.transcript().map_err(HandshakeError::Transcript)?;
                self.sign_transcript(&transcript)
                    .await
                    .map_err(map_keystore)?
            }
            _ => return Err(HandshakeError::TrustKind),
        };

        let mut state = self.state();
        state.put_challenge(crate::state::ChallengeRecord {
            kind: request.kind,
            peer: request.peer.clone(),
            client_nonce: request.client_nonce.clone(),
            server_nonce: server_nonce.clone(),
            connection_id: challenge_id.clone(),
            negotiated_features: request.negotiated_features.clone(),
            catalog_revision: request.catalog_revision,
            binding: request.binding.clone(),
            expires_at: expires_at.clone(),
        });
        drop(state);

        Ok(ChallengeIssue {
            challenge_id,
            server_nonce,
            expires_at,
            host_proof,
        })
    }

    /// 校验证明：消费挑战 → 复核绑定 → 用信任中的公钥验签 → 返回事实与凭据状态。
    ///
    /// 失败一律返回 [`HandshakeFailure`]：对端可见分类统一，审计动作按连接类型给出（§14.2）。
    /// 已撤销/未知的凭据状态**不**在这里表现为失败——签名有效就返回 `Authenticated`，由 adapter
    /// 按 `credential` 决定关闭连接并映射到 `auth.device_revoked`/`auth.device_unknown`。
    pub fn verify_proof(
        &self,
        submission: &ProofSubmission,
        trust: &PeerTrust,
    ) -> Result<Authenticated, HandshakeFailure> {
        self.verify_proof_inner(submission, trust)
            .map_err(|reason| HandshakeFailure::new(reason, submission.kind))
    }

    fn verify_proof_inner(
        &self,
        submission: &ProofSubmission,
        trust: &PeerTrust,
    ) -> Result<Authenticated, HandshakeError> {
        // 挑战必须存在且未被消费（一次性）；消费先于验签，重放因此必然失败。
        let Some(record) = self.state().take_challenge(&submission.challenge_id) else {
            return Err(HandshakeError::UnknownChallenge);
        };
        if record.kind != submission.kind || record.peer != submission.peer {
            return Err(HandshakeError::UnknownChallenge);
        }
        if record.server_nonce != submission.server_nonce
            || record.client_nonce != submission.client_nonce
        {
            return Err(HandshakeError::NonceMismatch);
        }
        if at_or_after(&self.now(), &record.expires_at) {
            return Err(HandshakeError::UnknownChallenge);
        }
        self.check_binding(submission.kind, &record.binding, trust)?;

        // 验签公钥**只**来自持久化信任。
        let Some(public_key) = trust.public_key.as_ref() else {
            return Err(HandshakeError::UntrustedPeer);
        };
        match (submission.kind, &submission.peer) {
            (ConnectionKind::SyncDevice, PeerIdentity::Device(device)) => {
                let Some(canonical_origin) = record.binding.canonical_origin.clone() else {
                    return Err(HandshakeError::BindingKind);
                };
                let input = SyncDeviceProof {
                    host_id: self.local_node().clone(),
                    device_id: device.clone(),
                    canonical_origin,
                    client_nonce: record.client_nonce.clone(),
                    server_nonce: record.server_nonce.clone(),
                    connection_id: record.connection_id.clone(),
                    negotiated_features: record.negotiated_features.clone(),
                };
                input
                    .verify(public_key, &submission.signature)
                    .map_err(HandshakeError::from)?;
            }
            (ConnectionKind::NodeLink, PeerIdentity::Node(access)) => {
                let Some(catalog_revision) = record.catalog_revision else {
                    return Err(HandshakeError::MissingCatalogRevision);
                };
                let input = NodeLinkProof {
                    owner_node_id: self.local_node().clone(),
                    access_node_id: access.clone(),
                    catalog_revision,
                    client_nonce: record.client_nonce.clone(),
                    server_nonce: record.server_nonce.clone(),
                    connection_id: record.connection_id.clone(),
                    negotiated_features: record.negotiated_features.clone(),
                };
                input
                    .verify(public_key, &submission.signature)
                    .map_err(HandshakeError::from)?;
            }
            _ => return Err(HandshakeError::TrustKind),
        }

        let fact = match &submission.peer {
            PeerIdentity::Device(device) => IdentityFact::Device {
                device: device.clone(),
                scopes: trust.scopes.clone(),
            },
            PeerIdentity::Node(node) => IdentityFact::Node {
                node: node.clone(),
                kind: trust.node_kind.ok_or(HandshakeError::TrustKind)?,
                grants: trust.grants.clone(),
            },
        };
        Ok(Authenticated {
            fact,
            credential: trust.credential,
            connection: record.binding,
        })
    }

    /// 校验绑定：`Host` 必须与声明的 origin/endpoint 自洽，且在持久化信任里有绑定时必须逐字相等。
    ///
    /// 未知对端没有登记绑定，因此只做自洽校验——它同样会拿到挑战，但随后必因没有公钥而验签失败。
    fn check_binding(
        &self,
        kind: ConnectionKind,
        binding: &ConnectionBinding,
        trust: &PeerTrust,
    ) -> Result<(), HandshakeError> {
        match kind {
            ConnectionKind::SyncDevice => {
                let Some(origin) = &binding.canonical_origin else {
                    return Err(HandshakeError::BindingKind);
                };
                if binding.endpoint.is_some() {
                    return Err(HandshakeError::BindingKind);
                }
                if !binding.host.eq_ignore_ascii_case(origin.authority()) {
                    return Err(HandshakeError::BindingMismatch);
                }
                if let Some(expected) = &trust.host_binding {
                    if expected != origin.as_str() {
                        return Err(HandshakeError::BindingMismatch);
                    }
                }
            }
            ConnectionKind::NodeLink => {
                let Some(endpoint) = &binding.endpoint else {
                    return Err(HandshakeError::BindingKind);
                };
                if binding.canonical_origin.is_some() {
                    return Err(HandshakeError::BindingKind);
                }
                if !binding.host.eq_ignore_ascii_case(endpoint.authority()) {
                    return Err(HandshakeError::BindingMismatch);
                }
                if let Some(expected) = &trust.host_binding {
                    if expected != endpoint.as_str() {
                        return Err(HandshakeError::BindingMismatch);
                    }
                }
            }
        }
        Ok(())
    }

    /// 认证收尾（唯一副作用入口，见 [`Authority::complete`]）。
    pub fn complete_auth(
        &self,
        fact: IdentityFact,
        pairing: Option<&PairingId>,
        at: &Timestamp,
    ) -> crate::types::Completion {
        self.complete(fact, pairing, at)
    }
}

/// 连接类型与对端身份类别必须一致。
fn peer_kind_matches(kind: ConnectionKind, peer: &PeerIdentity) -> bool {
    matches!(
        (kind, peer),
        (ConnectionKind::SyncDevice, PeerIdentity::Device(_))
            | (ConnectionKind::NodeLink, PeerIdentity::Node(_))
    )
}

/// 生成 32 字节服务端 nonce 的规范 base64url 文本。
pub(crate) fn random_nonce_for(
    entropy: &dyn crate::port::EntropySource,
) -> Result<Nonce, HandshakeError> {
    let mut bytes = [0u8; 32];
    entropy
        .fill(&mut bytes)
        .map_err(|_| HandshakeError::EntropyUnavailable)?;
    Nonce::new(&acpr_transcript::encode_base64url(&bytes)).map_err(HandshakeError::from)
}

/// `identity-auth` 内部 signing 失败到握手错误的映射。
fn map_keystore(error: KeystoreError) -> HandshakeError {
    HandshakeError::Keystore(error)
}
