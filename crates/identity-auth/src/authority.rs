//! `Authority`：持有注入端口与进程内状态的组合点。
//!
//! 它是本 crate 的唯一边界对象：adapter 与组合根按连接/配对调用它的入口；它自己**不**访问存储、
//! 不读系统时间、不做 IO。所有需要密钥的操作都经 [`port::IdentityKeystore`]，所有随机性都经
//! [`port::EntropySource`]，所有时间都经 [`acp_core::ports::Clock`]。

use std::sync::{Arc, Mutex, MutexGuard};

use acp_core::model::{NodeId, PeerPublicKey, Timestamp};
use acp_core::ports::Clock;

use crate::error::IdentityError;
use crate::port::{EntropySource, IdentityKeystore, KeyHandle};
use crate::state::State;
use crate::transcript::{
    NodeLinkChallenge, NodeLinkPairingOwnerProof, SyncHostChallenge, SyncPairingHostProof,
};
use crate::types::P1363Signature;

/// 身份状态机的组合点（见模块文档）。
pub struct Authority {
    local_node: NodeId,
    node_key: KeyHandle,
    keystore: Arc<dyn IdentityKeystore>,
    entropy: Arc<dyn EntropySource>,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
}

impl Authority {
    /// 装配。`node_key` 是本节点长期身份密钥的 keystore 引用（可由组合根经
    /// `IdentityKeystore::generate` 创建或从已有引用读回）。
    pub fn new(
        local_node: NodeId,
        node_key: KeyHandle,
        keystore: Arc<dyn IdentityKeystore>,
        entropy: Arc<dyn EntropySource>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            local_node,
            node_key,
            keystore,
            entropy,
            clock,
            state: Mutex::new(State::default()),
        }
    }

    /// 本节点标识（transcript 的 `hostId` / `ownerNodeId`）。
    pub fn local_node(&self) -> &NodeId {
        &self.local_node
    }

    /// 当前时间（唯一来源是注入的 `Clock`；状态机不读系统时间）。
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    /// 本节点长期身份公钥（组装 transcript 与展示指纹时使用）。
    pub async fn node_public_key(&self) -> Result<PeerPublicKey, IdentityError> {
        let key = self
            .keystore
            .public_key(&self.node_key)
            .await
            .map_err(IdentityError::from)?;
        Ok(key)
    }

    /// 熵源端口。
    pub(crate) fn entropy(&self) -> &dyn EntropySource {
        self.entropy.as_ref()
    }

    /// 进程内状态。
    ///
    /// 锁中毒时取回内层数据而不是 panic：状态机在任何 panic 之后仍要能继续失败关闭地运行。
    pub(crate) fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 用本节点身份签名一个已装配的 transcript。
    ///
    /// 只服务**本节点**必须发出的四个 domain（Sync 的 `pairing-host-proof`/`host-challenge`、
    /// Node Link 的 `node-link-pairing-owner-proof`/`node-link-challenge`）；device/access 侧的
    /// 证明由客户端平台自己签（`docs/SECURITY_DESIGN.md` §9.3），本 crate 只验签。
    pub(crate) async fn sign_transcript(
        &self,
        transcript: &[u8],
    ) -> Result<P1363Signature, crate::port::KeystoreError> {
        self.keystore.sign(&self.node_key, transcript).await
    }

    /// 签 Sync 配对宿主证明（claim 响应）。
    pub async fn sign_sync_pairing_host_proof(
        &self,
        input: &SyncPairingHostProof,
    ) -> Result<P1363Signature, IdentityError> {
        let transcript = input.transcript()?;
        self.sign_transcript(&transcript)
            .await
            .map_err(IdentityError::from)
    }

    /// 签 Sync 连接挑战（`hello` 内部使用，也可供测试与运维诊断直接调用）。
    pub async fn sign_sync_host_challenge(
        &self,
        input: &SyncHostChallenge,
    ) -> Result<P1363Signature, IdentityError> {
        let transcript = input.transcript()?;
        self.sign_transcript(&transcript)
            .await
            .map_err(IdentityError::from)
    }

    /// 签 Node Link 配对 Owner 证明。
    pub async fn sign_node_link_pairing_owner_proof(
        &self,
        input: &NodeLinkPairingOwnerProof,
    ) -> Result<P1363Signature, IdentityError> {
        let transcript = input.transcript()?;
        self.sign_transcript(&transcript)
            .await
            .map_err(IdentityError::from)
    }

    /// 签 Node Link 连接挑战。
    pub async fn sign_node_link_challenge(
        &self,
        input: &NodeLinkChallenge,
    ) -> Result<P1363Signature, IdentityError> {
        let transcript = input.transcript()?;
        self.sign_transcript(&transcript)
            .await
            .map_err(IdentityError::from)
    }

    /// 进程重启语义：丢弃全部内存 secret 与挑战缓存（返回 `(secrets, challenges)` 清理数）。
    ///
    /// 组合根在启动时调用一次；随后应按 [`crate::pairing::AuthorityPairingExt::unrecoverable_after_restart`]
    /// 终结存储里已无法继续验密的配对。
    pub fn reset_memory(&self) -> (usize, usize) {
        let mut state = self.state();
        let secrets = state.clear_all_secrets();
        let challenges = state.clear_challenges();
        (secrets, challenges)
    }

    /// 当前缓存的挑战数（上限见 [`crate::MAX_CHALLENGES`]；用于资源边界的回归测试）。
    pub fn challenge_cache_len(&self) -> usize {
        self.state().challenge_count()
    }
}

impl std::fmt::Debug for Authority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Authority")
            .field("local_node", &self.local_node)
            .finish_non_exhaustive()
    }
}
