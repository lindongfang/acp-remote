//! 配对状态机（合同 §4）：创建、认领校验、落定、过期与重启终结、状态视图。
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
use crate::pairing_code::{PairingCode, PairingCodeLookup};
use crate::state::PairingMaterial;
use crate::transcript::{
    NodeLinkPairingProof, NodeLinkPairingStatus, SyncPairingProof, SyncPairingStatus,
};
use crate::types::{
    CanonicalOrigin, ClaimFields, ClaimKindFields, ClaimOutcome, ClaimRejection, ClaimedPairing,
    Completion, IdentityFact, PAIRING_MAX_FAILURES, PAIRING_MAX_SECONDS, PairingDecision,
    PairingDraft, PairingProof, PairingQrLookup, PairingRequestId, PairingRequestMaterial,
    PairingSecret, PairingSpec, PairingStatusView, QrToken, at_or_after, is_unconfirmed,
};

impl Authority {
    /// 创建一个配对：登记绑定、有效期、内存 secret 与网页入口配对码；返回待落库的记录草稿。
    ///
    /// `expires_at` 由调用方按「不超过 5 分钟」算出后传入，本方法只复核上界（收窄可以、延长不行）。
    /// 设备与节点配对都不携带任何授权集合。
    ///
    /// 派生出的配对码必须在**仍持有 secret 的活跃配对**中唯一（design D2）：碰撞时重新生成
    /// secret（从而得到新码），用户侧不报错。唯一性检查与登记在同一把锁内完成，避免两个并发
    /// 创建同时通过检查；40 bit 空间下重试的期望次数是 1，只有熵源退化才会反复碰撞。
    ///
    /// 设备配对还会独立生成 32 字节扫码取回凭据（design D3）：与 secret 各抽取一次，绝不复用；
    /// 同样在锁内做唯一性检查，碰撞时重新生成。节点配对不生成、不登记该凭据。
    pub fn begin_pairing(
        &self,
        pairing_id: &PairingId,
        spec: &PairingSpec,
        display_name: Option<&str>,
        created_at: &Timestamp,
        expires_at: &Timestamp,
    ) -> Result<PairingDraft, PairingError> {
        let target = spec.target();
        let created = crate::transcript::unix_seconds(created_at)?;
        let expires = crate::transcript::unix_seconds(expires_at)?;
        if expires <= created || expires - created > PAIRING_MAX_SECONDS {
            return Err(PairingError::InvalidWindow);
        }
        let server_nonce = crate::handshake::random_nonce_for(self.entropy())
            .map_err(|_| PairingError::EntropyUnavailable)?;
        let pairing_request_id = PairingRequestId::generate(self.entropy())
            .map_err(|_| PairingError::EntropyUnavailable)?;
        let (secret, code, qr_token, record) = loop {
            let secret = PairingSecret::generate(self.entropy())
                .map_err(|_| PairingError::EntropyUnavailable)?;
            let code = crate::pairing_code::derive_pairing_code(&secret)?;
            let record = PairingRecord::try_new(
                pairing_id.clone(),
                target,
                PairingState::Created,
                display_name.map(str::to_owned),
                secret.digest()?,
                spec.host_binding(),
                created_at.clone(),
                expires_at.clone(),
                None,
                None,
                None,
            )?;
            let mut state = self.state();
            if state.code_in_use(&code) {
                // 碰撞：丢弃本次 secret 与草稿，重新生成；不向用户报错。
                continue;
            }
            let qr_token = match target {
                PairingTarget::Device => {
                    let mut token = QrToken::generate(self.entropy())
                        .map_err(|_| PairingError::EntropyUnavailable)?;
                    while state.qr_token_in_use(&token) {
                        // 凭据必须绑定唯一设备配对：碰撞时重新生成，绝不复用。
                        token = QrToken::generate(self.entropy())
                            .map_err(|_| PairingError::EntropyUnavailable)?;
                    }
                    Some(token)
                }
                PairingTarget::Node => None,
            };
            state.put_secret(
                pairing_id,
                PairingMaterial {
                    secret,
                    code: code.clone(),
                    server_nonce: server_nonce.clone(),
                    pairing_request_id: pairing_request_id.clone(),
                    qr_token,
                    // 新建的配对本就未批准；批准由 `Authority::mark_pairing_approved` 在持久化提交
                    // 成功之后置位（见 `settle` 的说明），`settle` 本身不置位。
                    approved: false,
                },
            );
            break (secret, code, qr_token, record);
        };
        Ok(PairingDraft {
            record,
            secret,
            code,
            server_nonce,
            pairing_request_id,
            qr_token,
        })
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

    /// 落定：批准或拒绝。批准要求状态为 `pending_confirmation` 且未过期。
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
            PairingDecision::Approve => {
                if pairing.state() != PairingState::PendingConfirmation {
                    return Err(PairingError::WrongState);
                }
                if at_or_after(at, pairing.expires_at()) {
                    return Err(PairingError::Expired);
                }
                // **不**在这里置位「已批准」（design D3：内存态绝不超前于已提交状态）：置位由
                // [`Authority::mark_pairing_approved`] 在调用方**提交成功之后**完成。
                Ok(PairingSettlement::approved())
            }
            PairingDecision::Reject { reason } => {
                if !matches!(
                    pairing.state(),
                    PairingState::Created | PairingState::PendingConfirmation
                ) {
                    return Err(PairingError::WrongState);
                }
                let settlement =
                    PairingSettlement::rejected(reason.as_deref()).map_err(PairingError::from)?;
                // 拒绝即关闭扫码取回资格（design D3）；secret 仍按合同 §4.3 保留到原过期时间，
                // 供状态轮询使用，但「保留 secret」绝不重新开放取回。
                self.state().close_qr_retrieval(pairing.id());
                Ok(settlement)
            }
        }
    }

    /// 过期扫描。**任何**到达 `expires_at` 的记录都在这里被清除内存 secret——这是 secret 的硬上界，
    /// 与记录是否已终结无关（`rejected`/`expired` 记录也在此清除，否则它们会一直驻留到进程退出）。
    /// 配对码反查表与 secret 同步清理（`clear_secret`）：过期后该码与不存在的码一样查不到任何配对。
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
    /// （它们不在未确认集合里）。配对码反查表与 secret 同步清理，不能从持久化摘要重建。
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

    /// 状态视图：`created` 阶段只暴露状态与过期时间；已认领后展示对端元数据与请求集合。
    ///
    /// 非批准态一旦过期，状态字段按 `Expired` 返回（`rejected`/`created` 不再表现为「有效状态」）。
    pub fn pairing_status(
        &self,
        pairing: &PairingRecord,
        peer: Option<&ClaimedPairing>,
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

    /// 配对通道可读的**非秘密**请求材料（claim 响应与 Owner 证明需要；design D12 的 seam 补全）。
    ///
    /// 与 secret 同生同灭（同一个内存条目）：secret 已被清除（过期扫描、重启、首次认证成功后）
    /// 或该配对未曾创建时返回 `None`。返回的两个值都不是凭据——它们会原样出现在 claim 响应里，
    /// 因此可以经公开入口读取；secret 本身**不**经任何入口返回。
    pub fn pairing_request_material(&self, pairing: &PairingId) -> Option<PairingRequestMaterial> {
        self.state()
            .material(pairing)
            .map(|material| PairingRequestMaterial {
                server_nonce: material.server_nonce.clone(),
                pairing_request_id: material.pairing_request_id.clone(),
            })
    }

    /// Node Link 配对状态查询证明（`node-link-pairing-status/v1`，`NODE_LINK_PROTOCOL.md` §9.4/§13.3）。
    ///
    /// 密钥是本机内存里该配对的 pairing secret（§4.3）：secret 缺失（未创建、已被清除）与 HMAC 不匹配
    /// 都是具名失败，adapter 必须把两者收敛为同一个对端可见的失败（§4.5：不泄露校验差异）。
    /// `input.owner_node_id` 必须等于本节点：本入口只服务本节点登记的配对，不替别的 owner 装配 transcript。
    pub fn verify_node_link_pairing_status(
        &self,
        input: &NodeLinkPairingStatus,
        proof: &PairingProof,
    ) -> Result<(), PairingError> {
        if &input.owner_node_id != self.local_node() {
            return Err(PairingError::ClaimMismatch);
        }
        let Some(secret) = self.state().secret(&input.pairing_id) else {
            return Err(PairingError::SecretUnavailable);
        };
        input.verify(&secret, proof).map_err(PairingError::from)
    }

    /// Sync 配对状态查询证明（`acp-remote/pairing-status/v1`，`SYNC_PROTOCOL.md` §7.3）。
    ///
    /// 与 [`Authority::verify_node_link_pairing_status`] **同形**（同一套三条守卫，domain 与字段集合
    /// 不同）：密钥是本机内存里该配对的 pairing secret（§4.3）；`input.host_id` 必须等于本节点：本入口只
    /// 服务本节点登记的配对，不替别的 host 装配 transcript；secret 缺失（未创建、已被清除——过期扫描、
    /// 重启、首次认证成功）与 HMAC 不匹配都是具名失败（`SecretUnavailable`/`Proof(Hmac)`），**adapter
    /// 必须把两者收敛为同一个对端可见的失败**（§4.5：不泄露校验差异）。
    ///
    /// `SyncPairingStatus` 的 `hmac`/`verify` 是本入口的唯一密码学依赖（§7.3 的
    /// `pairing-status` 域，`transcript.rs`）；本入口不读存储、不读系统时间。
    pub fn verify_sync_pairing_status(
        &self,
        input: &SyncPairingStatus,
        proof: &PairingProof,
    ) -> Result<(), PairingError> {
        if &input.host_id != self.local_node() {
            return Err(PairingError::ClaimMismatch);
        }
        let Some(secret) = self.state().secret(&input.pairing_id) else {
            return Err(PairingError::SecretUnavailable);
        };
        input.verify(&secret, proof).map_err(PairingError::from)
    }

    /// 按扫码取回凭据查找配对材料（**不消耗**凭据、不推进状态）。
    ///
    /// 这是 `docs/IDENTITY_AND_AUTH_CONTRACT.md` §5.1 登记的入口：返回值只覆盖本进程内存里仍持有
    /// secret 且凭据仍然绑定的配对。adapter 仍必须确认该配对为 `created`、未过期、未被拒绝/失效，
    /// 且来源/Host 与登记绑定一致；任何一项不满足都收敛为同一个 `401 pairing.qr_invalid`。
    /// 需要把权威状态检查也放进身份层的调用方可用
    /// [`Authority::retrieve_qr_material`]（它要求调用方传入已读到的持久记录）。
    pub fn lookup_pairing_by_qr_token(&self, token: &QrToken) -> Option<PairingQrLookup> {
        let (pairing, secret) = self.state().qr_material(token)?;
        Some(PairingQrLookup { pairing, secret })
    }

    /// 扫码取回：只有 `created`、未过期、未被内存标记失效且持久记录仍对应这份材料的设备配对
    /// 才返回同一份资料（design D3/D5）。
    ///
    /// 依赖方向：本方法**不读存储**，`record` 由调用方（服务端）从持久化读取后传入；调用方必须
    /// 先完成来源/Host/身份绑定校验，再调用这里。判定顺序为：凭据反查 → 记录标识一致 →
    /// `created` → 未失效 → 未过期 → 记录摘要与内存 secret 一致。
    ///
    /// 副作用：**无**。不消耗凭据、不消耗网页码、不推进状态、不改写 `expires_at`；重复调用返回
    /// 相同配对标识与 secret。认领成功后由调用方调用 [`Authority::close_qr_retrieval`] 关闭入口；
    /// 即使尚未关闭，已认领的记录状态也不再满足 `created`，取回同样失败。所有失败统一返回 `None`
    /// （adapter 必须收敛为同一个对端可见失败）。
    pub fn retrieve_qr_material(
        &self,
        record: &PairingRecord,
        token: &QrToken,
    ) -> Option<PairingQrLookup> {
        if record.target() != PairingTarget::Device {
            return None;
        }
        let now = self.now();
        let state = self.state();
        let (pairing, secret) = state.qr_material(token)?;
        if &pairing != record.id() {
            return None;
        }
        if record.state() != PairingState::Created {
            return None;
        }
        if state.is_invalidated(&pairing) {
            return None;
        }
        if at_or_after(&now, record.expires_at()) {
            return None;
        }
        // 材料替换后旧凭据不能返回新 secret：记录摘要必须仍与内存 secret 一致。
        if record.secret_digest() != &secret.digest().ok()? {
            return None;
        }
        Some(PairingQrLookup { pairing, secret })
    }

    /// 关闭扫码取回资格（认领提交成功后由调用方调用）：移除反查条目，**保留** pairing secret。
    ///
    /// 返回 `false` 表示该配对已没有可取回的凭据（从未创建过设备凭据、已被清理或已经关闭）；
    /// 调用方不应把它当作失败——关闭是幂等的。为幂等 claim/status 保留的 secret 不会因此重新
    /// 开放取回。
    pub fn close_qr_retrieval(&self, pairing: &PairingId) -> bool {
        self.state().close_qr_retrieval(pairing)
    }

    /// 用网页入口的配对码**一次性**取回绑定与 secret（design D3/D4）。
    ///
    /// 只覆盖本进程内存里仍持有 secret 的配对：重启、过期扫描、拒绝后的原过期时间、已批准配对
    /// 的首次认证成功都会让码与 secret 一起消失。查找使用常数时间比较；**成功即消耗**——同一码的
    /// 第二次查找返回 `None`，与不存在的码不可区分（`None` 是所有失败的唯一形状，adapter 必须把它
    /// 收敛为同一个 `401 pairing.code_invalid`）。查找不推进配对状态，返回的 secret 之后仍按原有
    /// claim/status transcript 使用。
    ///
    /// 这是本 crate **唯一**按码返回 secret 的入口，只服务面向前端的 lookup 端点；不需要 secret 的
    /// 调用方不要走这里。
    pub fn consume_pairing_code(&self, code: &PairingCode) -> Option<PairingCodeLookup> {
        let (pairing, secret) = self.state().take_code(code)?;
        Some(PairingCodeLookup { pairing, secret })
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
        PairingClaim::try_new(self.pairing.clone(), peer).map_err(PairingError::from)
    }
}
