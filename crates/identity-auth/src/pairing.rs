//! 配对状态机（合同 §4）：创建、认领校验、落定、过期与重启终结、SAS 与状态视图。
//!
//! 入口只做纯计算与内存态变更：持久化事实由调用方以快照入参提供、以领域值返回，由调用方组装
//! `core::ports` 的写集（`PairingWrite`/`PairingClaimWrite`/`PairingSettlementWrite`/`ExpiryWrite`）
//! 并单事务提交。这样「状态机不访问存储」与「写集一事务提交」同时成立，且每个失败路径都能直接单测。

use acp_core::model::{
    PairingClaim, PairingId, PairingPeer, PairingRecord, PairingSettlement, PairingState,
    PairingTarget, PeerIdentity, Timestamp,
};

use crate::authority::Authority;
use crate::error::PairingError;
use crate::state::PairingMaterial;
use crate::transcript::{
    NodeLinkPairingProof, NodeLinkPairingSas, SyncPairingProof, SyncPairingSas,
};
use crate::types::{
    CanonicalOrigin, ClaimFields, ClaimKindFields, ClaimOutcome, ClaimRejection, ClaimedPairing,
    Completion, IdentityFact, PAIRING_MAX_FAILURES, PAIRING_MAX_SECONDS, PairingDecision,
    PairingDraft, PairingRequestId, PairingSpec, PairingStatusView, RequestedCapabilities, Sas,
    at_or_after, is_unconfirmed,
};

impl Authority {
    /// 创建一个配对：登记请求集合、绑定、有效期与内存 secret；返回待落库的记录草稿。
    ///
    /// `expires_at` 由调用方按「不超过 5 分钟」算出后传入，本方法只复核上界（收窄可以、延长不行）。
    pub fn begin_pairing(
        &self,
        pairing_id: &PairingId,
        spec: &PairingSpec,
        requested: &RequestedCapabilities,
        display_name: Option<&str>,
        created_at: &Timestamp,
        expires_at: &Timestamp,
    ) -> Result<PairingDraft, PairingError> {
        let target = spec.target();
        if !requested.matches_target(target) {
            return Err(PairingError::CapabilityKindMismatch);
        }
        let created = crate::transcript::unix_seconds(created_at)?;
        let expires = crate::transcript::unix_seconds(expires_at)?;
        if expires <= created || expires - created > PAIRING_MAX_SECONDS {
            return Err(PairingError::InvalidWindow);
        }
        let secret = crate::types::PairingSecret::generate(self.entropy())
            .map_err(|_| PairingError::EntropyUnavailable)?;
        let server_nonce = crate::handshake::random_nonce_for(self.entropy())
            .map_err(|_| PairingError::EntropyUnavailable)?;
        let pairing_request_id = PairingRequestId::generate(self.entropy())
            .map_err(|_| PairingError::EntropyUnavailable)?;
        let record = PairingRecord::try_new(
            pairing_id.clone(),
            target,
            PairingState::Created,
            display_name.map(str::to_owned),
            requested.scopes.clone(),
            requested.grants.clone(),
            secret.digest(),
            spec.host_binding(),
            created_at.clone(),
            expires_at.clone(),
            None,
            None,
            None,
        )?;
        self.state().put_secret(
            pairing_id,
            PairingMaterial {
                secret,
                server_nonce: server_nonce.clone(),
                pairing_request_id: pairing_request_id.clone(),
                // 新建的配对本就未批准；批准由 `Authority::mark_pairing_approved` 在持久化提交
                // 成功之后置位（见 `settle` 的说明），`settle` 本身不置位。
                approved: false,
            },
        );
        Ok(PairingDraft {
            record,
            secret,
            server_nonce,
            pairing_request_id,
        })
    }

    /// SAS 派生（合同 §4.4）：本机在批准界面上展示的 6 位短验证码。
    ///
    /// 只在「已认领且尚未落定」的窗口内有意义（secret 还在内存、请求方身份已固定）；
    /// 过期、已落定或 secret 已被清除时返回错误，**不**以占位值代替。本机结果只用于本地展示，
    /// 绝不当作对端结果下发（合同 §4.4）。
    pub async fn pairing_sas(
        &self,
        pairing: &PairingRecord,
        peer: &ClaimedPairing,
    ) -> Result<Sas, PairingError> {
        if pairing.state() != PairingState::PendingConfirmation {
            return Err(PairingError::WrongState);
        }
        if at_or_after(&self.now(), pairing.expires_at()) {
            return Err(PairingError::Expired);
        }
        let material = self
            .state()
            .material(pairing.id())
            .cloned()
            .ok_or(PairingError::SecretUnavailable)?;
        let host_public_key = self
            .node_public_key()
            .await
            .map_err(|_| PairingError::SecretUnavailable)?;
        let secret = material.secret;
        let transcript = match (pairing.target(), &peer.peer) {
            (PairingTarget::Device, PeerIdentity::Device(device)) => {
                let canonical_origin = CanonicalOrigin::parse(&peer.host_binding)?;
                SyncPairingSas {
                    host_id: self.local_node().clone(),
                    device_id: device.clone(),
                    pairing_id: pairing.id().clone(),
                    canonical_origin,
                    host_public_key,
                    device_public_key: peer.public_key.clone(),
                    client_nonce: peer.client_nonce.clone(),
                    server_nonce: material.server_nonce,
                    pairing_request_id: material.pairing_request_id,
                }
                .transcript()?
            }
            (PairingTarget::Node, PeerIdentity::Node(node)) => NodeLinkPairingSas {
                owner_node_id: self.local_node().clone(),
                access_node_id: node.clone(),
                pairing_id: pairing.id().clone(),
                owner_public_key: host_public_key,
                access_public_key: peer.public_key.clone(),
                client_nonce: peer.client_nonce.clone(),
                server_nonce: material.server_nonce,
                pairing_request_id: material.pairing_request_id,
            }
            .transcript()?,
            _ => return Err(PairingError::ClaimMismatch),
        };
        crate::transcript::derive_sas(&transcript, &secret).map_err(PairingError::from)
    }

    /// 认领校验：结构 → 状态/过期 → 绑定 → 集合 → HMAC 的唯一入口。
    ///
    /// `existing` 是调用方从存储里读到的「该配对已固定的对端」（没有则为 `None`）：相同载荷重发
    /// 返回 [`ClaimOutcome::Repeat`]（幂等，不产生第二次写入），不同载荷返回拒绝。
    pub fn verify_claim(
        &self,
        pairing: &PairingRecord,
        existing: Option<&ClaimedPairing>,
        fields: &ClaimFields,
    ) -> Result<ClaimOutcome, PairingError> {
        // 结构：标识与目标族必须一致（不一致属结构性错误，先于任何状态判定）。
        if pairing.id() != &fields.pairing || !fields.kind.matches_target(pairing.target()) {
            return Ok(ClaimOutcome::Rejected(ClaimRejection::Malformed));
        }
        // 幂等：已固定对端的重复认领。
        if let Some(existing) = existing {
            return Ok(if existing.matches(fields) {
                ClaimOutcome::Repeat(Box::new(existing.clone()))
            } else {
                ClaimOutcome::Rejected(ClaimRejection::NotClaimable)
            });
        }
        if pairing.state() != PairingState::Created {
            return Ok(ClaimOutcome::Rejected(ClaimRejection::NotClaimable));
        }
        if self.state().is_invalidated(&fields.pairing) {
            return Ok(ClaimOutcome::Rejected(ClaimRejection::NotClaimable));
        }
        if at_or_after(&self.now(), pairing.expires_at()) {
            return Ok(ClaimOutcome::Rejected(ClaimRejection::Expired));
        }
        // §11.2 第 1 条：对端必须逐字回显登记绑定。
        if fields.host_binding != pairing.host_binding() {
            return Ok(ClaimOutcome::Rejected(ClaimRejection::BindingMismatch));
        }
        let registered = RequestedCapabilities {
            scopes: pairing.requested_scopes().clone(),
            grants: pairing.requested_grants().clone(),
        };
        if !fields.requested.within(&registered) {
            return Ok(ClaimOutcome::Rejected(
                ClaimRejection::CapabilitiesExceedRegistered,
            ));
        }
        // 证明：内存 secret（重启后必然缺失 → 该配对已不可认领）。
        let Some(secret) = self.state().secret(&fields.pairing) else {
            return Ok(ClaimOutcome::Rejected(ClaimRejection::NotClaimable));
        };
        if self.verify_claim_proof(pairing, fields, &secret).is_err() {
            let failures = self.state().record_failure(&fields.pairing);
            if failures >= PAIRING_MAX_FAILURES {
                self.state().invalidate(&fields.pairing);
                return Ok(ClaimOutcome::Rejected(ClaimRejection::TooManyFailures));
            }
            return Ok(ClaimOutcome::Rejected(ClaimRejection::ProofInvalid));
        }
        Ok(ClaimOutcome::Claimed(Box::new(ClaimedPairing {
            pairing: fields.pairing.clone(),
            peer: fields.peer.clone(),
            display_name: fields.display_name.clone(),
            public_key: fields.public_key.clone(),
            host_binding: fields.host_binding.clone(),
            client_nonce: fields.client_nonce.clone(),
            requested: fields.requested.clone(),
        })))
    }

    /// 按目标族选择 transcript domain 并校验认领证明。
    fn verify_claim_proof(
        &self,
        pairing: &PairingRecord,
        fields: &ClaimFields,
        secret: &crate::types::PairingSecret,
    ) -> Result<(), PairingError> {
        match pairing.target() {
            PairingTarget::Device => {
                let PeerIdentity::Device(device) = &fields.peer else {
                    return Err(PairingError::ClaimMismatch);
                };
                let crate::types::ClaimKindFields::Device { client_kind } = fields.kind else {
                    return Err(PairingError::ClaimMismatch);
                };
                let canonical_origin = CanonicalOrigin::parse(&fields.host_binding)?;
                let input = SyncPairingProof {
                    host_id: self.local_node().clone(),
                    device_id: device.clone(),
                    pairing_id: fields.pairing.clone(),
                    pairing_expires_at: pairing.expires_at().clone(),
                    canonical_origin,
                    device_public_key: fields.public_key.clone(),
                    client_nonce: fields.client_nonce.clone(),
                    device_name: fields.display_name.clone(),
                    client_kind,
                };
                input.verify(secret, &fields.proof)?;
            }
            PairingTarget::Node => {
                let PeerIdentity::Node(node) = &fields.peer else {
                    return Err(PairingError::ClaimMismatch);
                };
                let ClaimKindFields::Node { node_kind } = fields.kind else {
                    return Err(PairingError::ClaimMismatch);
                };
                let input = NodeLinkPairingProof {
                    owner_node_id: self.local_node().clone(),
                    access_node_id: node.clone(),
                    pairing_id: fields.pairing.clone(),
                    pairing_expires_at: pairing.expires_at().clone(),
                    access_public_key: fields.public_key.clone(),
                    client_nonce: fields.client_nonce.clone(),
                    node_name: fields.display_name.clone(),
                    node_kind,
                };
                input.verify(secret, &fields.proof)?;
            }
        }
        Ok(())
    }

    /// 落定：批准或拒绝。批准要求状态为 `pending_confirmation`、未过期，且最终集合不超出请求值。
    ///
    /// **不在落定时清除 secret**（合同 §4.3）：批准后的配对仍要被 `pairing-status` 之类的
    /// HMAC 证明使用，直到「首次认证成功」才提前清除（见 [`Authority::complete`]）；
    /// 被拒绝的配对也可以为可靠轮询保留到原过期时间——两条路径的上界都是 `expires_at`，
    /// 由 [`Authority::due_pairings`] 在过期时统一清除（**包含已终结的 `rejected` 记录**）。
    ///
    /// **不**在落定时置位「已批准」：[`Authority::mark_pairing_approved`] 由调用方在持久化提交
    /// **成功之后**调用（design D3：内存态绝不超前于已提交状态），使 [`Authority::complete`]
    /// 只能把**已批准**的配对作为消费目标。
    pub fn settle(
        &self,
        pairing: &PairingRecord,
        decision: &PairingDecision,
        at: &Timestamp,
    ) -> Result<PairingSettlement, PairingError> {
        match decision {
            PairingDecision::Approve {
                granted_scopes,
                granted_grants,
            } => {
                if pairing.state() != PairingState::PendingConfirmation {
                    return Err(PairingError::WrongState);
                }
                if at_or_after(at, pairing.expires_at()) {
                    return Err(PairingError::Expired);
                }
                let granted = RequestedCapabilities {
                    scopes: granted_scopes.clone(),
                    grants: granted_grants.clone(),
                };
                if !granted.matches_target(pairing.target()) {
                    return Err(PairingError::CapabilityKindMismatch);
                }
                let requested = RequestedCapabilities {
                    scopes: pairing.requested_scopes().clone(),
                    grants: pairing.requested_grants().clone(),
                };
                if !granted.within(&requested) {
                    return Err(PairingError::CapabilitiesExceedRequested);
                }
                // **不**在这里置位「已批准」（design D3：内存态绝不超前于已提交状态）：置位由
                // [`Authority::mark_pairing_approved`] 在调用方**提交成功之后**完成。
                Ok(PairingSettlement::approved(
                    granted_scopes.clone(),
                    granted_grants.clone(),
                ))
            }
            PairingDecision::Reject { reason } => {
                if !matches!(
                    pairing.state(),
                    PairingState::Created | PairingState::PendingConfirmation
                ) {
                    return Err(PairingError::WrongState);
                }
                PairingSettlement::rejected(reason.as_deref()).map_err(PairingError::from)
            }
        }
    }

    /// 过期扫描。**任何**到达 `expires_at` 的记录都在这里被清除内存 secret——这是 secret 的硬上界，
    /// 与记录是否已终结无关（`rejected`/`expired` 记录也在此清除，否则它们会一直驻留到进程退出）。
    ///
    /// 返回值只包含**未终结**的配对：它们需要调用方按 `core::ports` 的写集（§11.6）提交终态写集
    /// （`expire_pairings`）。已批准但已过期的记录也在此列：它的落库终态（`approved_at` 是否保留）
    /// 属存储侧语义，本层只保证 secret 不再可用。
    pub fn due_pairings(&self, pairings: &[PairingRecord], at: &Timestamp) -> Vec<PairingId> {
        let mut due = Vec::new();
        for record in pairings {
            if !at_or_after(at, record.expires_at()) {
                continue;
            }
            // 清除先于状态判断：终态记录同样必须释放 secret。
            self.state().clear_secret(record.id());
            if !record.state().is_terminal() {
                due.push(record.id().clone());
            }
        }
        due
    }

    /// 进程重启后已无法继续验密的配对（`created`/`pending_confirmation`）：**全部**必须终结。
    ///
    /// 依据合同 §4.3：secret 只在内存，重启不能凭 digest 恢复；已批准的信任记录不受影响
    /// （它们不在未确认集合里）。
    pub fn unrecoverable_after_restart(&self, pairings: &[PairingRecord]) -> Vec<PairingId> {
        pairings
            .iter()
            .filter(|record| is_unconfirmed(record.state()))
            .map(|record| {
                self.state().clear_secret(record.id());
                record.id().clone()
            })
            .collect()
    }

    /// 状态视图：`created` 阶段只暴露状态与过期时间；已认领后按调用方传入的 SAS 展示。
    ///
    /// 非批准态一旦过期，状态字段按 `Expired` 返回（`rejected`/`created` 不再表现为「有效状态」）。
    pub fn pairing_status(
        &self,
        pairing: &PairingRecord,
        peer: Option<&ClaimedPairing>,
        sas: Option<crate::types::Sas>,
    ) -> PairingStatusView {
        let expired = !matches!(
            pairing.state(),
            PairingState::Approved | PairingState::Consumed
        ) && at_or_after(&self.now(), pairing.expires_at());
        let state = if expired {
            PairingState::Expired
        } else {
            pairing.state()
        };
        let claimed = !matches!(state, PairingState::Created | PairingState::Expired);
        PairingStatusView {
            state,
            display_name: claimed
                .then(|| peer.map(|peer| peer.display_name.clone()))
                .flatten(),
            public_key_fingerprint: claimed
                .then(|| peer.map(|peer| peer.public_key.fingerprint()))
                .flatten(),
            sas: if claimed { sas } else { None },
            requested: claimed.then(|| RequestedCapabilities {
                scopes: pairing.requested_scopes().clone(),
                grants: pairing.requested_grants().clone(),
            }),
            peer: claimed
                .then(|| peer.map(|peer| peer.peer.clone()))
                .flatten(),
            expires_at: pairing.expires_at().clone(),
        }
    }

    /// 确认「该配对的批准已**持久化提交成功**」（design D3：内存态只允许比已提交状态更严格，
    /// 因此置位发生在提交之后，而不是 `settle` 内）。
    ///
    /// 返回 `false` 表示内存里已经没有该配对的材料（例如已过期被清理、或进程重启过）——
    /// 调用方应把这个信号当作「本次批准的内存效果已不可用」处理（例如在日志/审计里记录），
    /// 而不是当作成功。合同 §4.1 与 [`Authority::complete_auth`] 的语义依赖它被正确调用。
    pub fn mark_pairing_approved(&self, pairing: &PairingId) -> bool {
        self.state().mark_approved(pairing)
    }

    /// 单个配对的累计 proof 失败次数。
    pub fn failure_count(&self, pairing: &PairingId) -> u32 {
        self.state().failures(pairing)
    }

    /// 内存中是否仍持有该配对的 secret（测试与诊断用；不暴露内容）。
    pub fn has_secret(&self, pairing: &PairingId) -> bool {
        self.state().secret(pairing).is_some()
    }
}

impl Authority {
    /// 认证收尾（合同 §5.1 的第 3 个入口，也是唯一的副作用入口）。
    ///
    /// **公开名是 [`Authority::complete_auth`](crate::handshake) 的同一实现**：本方法是它的实现体，
    /// 保持 `pub(crate)` 以免出现两个对外名字（合同只登记 `complete_auth`）。
    ///
    /// 状态机内只做一件事：清除**已批准**配对的内存 secret（合同 §4.3 的「首次认证成功时提前清除」；
    /// 「已批准」只能由 [`Authority::mark_pairing_approved`] 在**持久化提交成功之后**置位，
    /// 因此未批准、已拒绝或「提交失败」的配对都不会被误当成消费目标）；返回的 [`Completion`]
    /// 告诉调用方需要落库的消费目标、与之一致的时间与本次事实。
    pub(crate) fn complete(
        &self,
        fact: IdentityFact,
        pairing: Option<&PairingId>,
        at: &Timestamp,
    ) -> Completion {
        let consume_pairing = pairing.filter(|pairing| {
            // 只有「已批准且仍持有 secret」的配对才需要被推进为 consumed：已经清除说明本次不是首次
            // 认证；尚未批准说明调用方传错了目标（可能是同对端的旧配对）。
            self.state().take_approved_secret(pairing)
        });
        Completion {
            fact,
            at: at.clone(),
            consume_pairing: consume_pairing.cloned(),
        }
    }
}

/// `PairingClaim` 与 `ClaimedPairing` 之间的桥接：调用方把 [`ClaimOutcome`] 变成存储层写集。
impl ClaimedPairing {
    /// 构造 core 的 [`PairingClaim`]（可直接放进 `PairingClaimWrite`）。
    pub fn to_claim(&self) -> Result<PairingClaim, PairingError> {
        let peer = PairingPeer::try_new(
            self.peer.clone(),
            &self.display_name,
            self.public_key.clone(),
            &self.host_binding,
            self.client_nonce.clone(),
        )?;
        PairingClaim::try_new(
            self.pairing.clone(),
            peer,
            self.requested.scopes.clone(),
            self.requested.grants.clone(),
        )
        .map_err(PairingError::from)
    }
}
