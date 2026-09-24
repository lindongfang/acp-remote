//! 进程内状态：配对 secret、每配对失败计数、挑战缓存。
//!
//! 三条不变量（`design.md` D3）：
//!
//! - **只在内存**：这里的一切都不得持久化；重启后 `Authority` 从空状态开始（合同 §4.3）。
//! - **内存态只允许比已提交状态更严格**：例如第 5 次失败后配对在内存里已失效，但绝不会出现
//!   「内存里批准、库里没有信任」。
//! - **不跨 `await` 持锁**：`Authority` 只在同步临界区里锁本结构；需要调用 keystore 的路径先算出
//!   输入、在锁外 `await`，再短锁写回。

use std::collections::{BTreeMap, BTreeSet};

use acp_core::model::{Nonce, PairingId, PeerIdentity, Timestamp};

use crate::transcript::FeatureList;
use crate::types::at_or_after;
use crate::types::{
    ChallengeId, ConnectionBinding, ConnectionKind, PairingRequestId, PairingSecret,
};

/// 挑战缓存的硬上限（远超真实并发连接数；超出时淘汰最早过期的条目）。
pub const MAX_CHALLENGES: usize = 1024;

/// 一次挑战的内存记录（一次性消费）。
#[derive(Debug, Clone)]
pub(crate) struct ChallengeRecord {
    pub kind: ConnectionKind,
    pub peer: PeerIdentity,
    pub client_nonce: Nonce,
    pub server_nonce: Nonce,
    pub connection_id: ChallengeId,
    pub negotiated_features: FeatureList,
    pub catalog_revision: Option<u64>,
    pub binding: ConnectionBinding,
    pub expires_at: Timestamp,
}

/// 一次配对的内存材料：secret 与其派生 SAS 需要的本机 nonce / 请求标识。
///
/// 三者同生同灭（同一把锁、同一张表）：重启后一起丢失，这就是「重启后未确认配对全部终结」的
/// 物理依据，不需要额外的持久化字段。
#[derive(Debug, Clone)]
pub(crate) struct PairingMaterial {
    pub secret: PairingSecret,
    pub server_nonce: Nonce,
    pub pairing_request_id: PairingRequestId,
    /// 是否已被批准（「内存态只允许比已提交状态更严格」，见 design D3）。
    ///
    /// 为什么需要它：`complete` 只拿到配对标识，若用「内存里还有 secret」代理
    /// 「已批准且未被消费」，那么仍在保留期内的 `created`/`pending_confirmation`/`rejected`
    /// 记录也会被当成消费目标（合同 §5.1 要求 `consume_pairing` 只在已批准配对的首次认证时非空）。
    pub approved: bool,
}

/// 进程内状态。
#[derive(Debug, Default)]
pub(crate) struct State {
    secrets: BTreeMap<String, PairingMaterial>,
    failures: BTreeMap<String, u32>,
    invalidated: BTreeSet<String>,
    challenges: BTreeMap<String, ChallengeRecord>,
}

impl State {
    /// 登记一个配对的内存材料（创建配对时）。
    pub fn put_secret(&mut self, pairing: &PairingId, material: PairingMaterial) {
        self.secrets.insert(pairing.as_str().to_owned(), material);
    }

    /// 取配对 secret。
    pub fn secret(&self, pairing: &PairingId) -> Option<PairingSecret> {
        self.secrets.get(pairing.as_str()).map(|m| m.secret)
    }

    /// 取配对内存材料（SAS 派生需要本机 nonce 与请求标识）。
    pub fn material(&self, pairing: &PairingId) -> Option<&PairingMaterial> {
        self.secrets.get(pairing.as_str())
    }

    /// 标记该配对已批准（`settle(Approve)` 时置位，不改变 secret 的保留期）。
    pub fn mark_approved(&mut self, pairing: &PairingId) -> bool {
        match self.secrets.get_mut(pairing.as_str()) {
            Some(material) => {
                material.approved = true;
                true
            }
            None => false,
        }
    }

    /// 取出「已批准且仍持有 secret」的配对（首次认证成功消费一次）：返回 `true` 时才清除。
    pub fn take_approved_secret(&mut self, pairing: &PairingId) -> bool {
        let approved = self
            .secrets
            .get(pairing.as_str())
            .is_some_and(|material| material.approved);
        if approved {
            self.secrets.remove(pairing.as_str());
        }
        approved
    }

    /// 清除配对 secret（批准后首次认证成功、拒绝、过期与重启终结都走这里）。
    pub fn clear_secret(&mut self, pairing: &PairingId) -> bool {
        self.secrets.remove(pairing.as_str()).is_some()
    }

    /// 清空全部 secret（启动恢复用；返回被清理的条数）。
    pub fn clear_all_secrets(&mut self) -> usize {
        let count = self.secrets.len();
        self.secrets.clear();
        count
    }

    /// 记录一次 proof 失败并返回累计次数。
    pub fn record_failure(&mut self, pairing: &PairingId) -> u32 {
        let entry = self
            .failures
            .entry(pairing.as_str().to_owned())
            .or_insert(0);
        *entry = entry.saturating_add(1);
        *entry
    }

    /// 当前失败次数（测试与错误信息使用）。
    pub fn failures(&self, pairing: &PairingId) -> u32 {
        self.failures
            .get(pairing.as_str())
            .copied()
            .unwrap_or_default()
    }

    /// 标记配对在内存中失效（第 5 次失败后），并返回是否刚刚完成失效转换。
    pub fn invalidate(&mut self, pairing: &PairingId) -> bool {
        self.invalidated.insert(pairing.as_str().to_owned())
    }

    /// 是否已被内存标记失效。
    pub fn is_invalidated(&self, pairing: &PairingId) -> bool {
        self.invalidated.contains(pairing.as_str())
    }

    /// 登记挑战：先清扫已过期条目，再在上限内加入（最坏情况淘汰**最早过期**的那条）。
    ///
    /// 为什么要清：`AGENTS.md` §5 要求「所有输入都有长度、数量、频率和资源限制」。挑战只会被
    /// `take_challenge` 消费，而「完成 hello 但不发 proof」的连接会留下永不消费的条目；
    /// 因此这里用注入时钟做清扫 + 硬上限，使内存占用与真实连接数解耦。
    /// 淘汰是安全的：被淘汰的客户端只需重新握手（未知挑战本来就是统一的证明失败）。
    pub fn put_challenge(&mut self, challenge: ChallengeRecord, now: &Timestamp) {
        self.challenges
            .retain(|_, record| !at_or_after(now, &record.expires_at));
        if self.challenges.len() >= MAX_CHALLENGES {
            if let Some(oldest) = self
                .challenges
                .iter()
                .min_by(|left, right| left.1.expires_at.as_str().cmp(right.1.expires_at.as_str()))
                .map(|(key, _)| key.clone())
            {
                self.challenges.remove(&oldest);
            }
        }
        self.challenges
            .insert(challenge.connection_id.as_str().to_owned(), challenge);
    }

    /// 当前缓存的挑战数（测试用）。
    pub fn challenge_count(&self) -> usize {
        self.challenges.len()
    }

    /// 取出并**消费**挑战（一次性；缺失即未知或已消费）。
    pub fn take_challenge(&mut self, challenge: &ChallengeId) -> Option<ChallengeRecord> {
        self.challenges.remove(challenge.as_str())
    }

    /// 丢弃全部挑战（进程重启语义；客户端必须重新握手）。
    pub fn clear_challenges(&mut self) -> usize {
        let count = self.challenges.len();
        self.challenges.clear();
        count
    }
}
