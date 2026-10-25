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
use crate::preparation::{
    PreparationAdmission, PreparationBinding, PreparationError, PreparationExpectation,
    PreparationReceipt,
};
use crate::state::State;
use crate::transcript::{
    NodeLinkChallenge, NodeLinkPairingOwnerProof, PreparationClaims, SyncHostChallenge,
    SyncPairingHostProof,
};
use crate::types::{Authenticated, P1363Signature};

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

    /// 签准备凭据（`acp-remote/session-preparation/v1`，design D2）。
    ///
    /// 只接受结构化 claims：domain 与字段顺序由 [`PreparationClaims::transcript`] 按登记表固定，
    /// 本入口**不**签名任意字节，也不接受调用方给出的 domain。claims 的 `ownerNodeId` 必须是本节点，
    /// 否则拒绝签名（同一把密钥不得为其他节点出证）。`storeEpoch` 由调用方按当前持久
    /// `meta.server_epoch` 提供；本 crate 不读存储，因此不能替调用方校验该事实。
    pub async fn sign_preparation_receipt(
        &self,
        claims: &PreparationClaims,
    ) -> Result<PreparationReceipt, PreparationError> {
        if claims.owner_node_id != self.local_node {
            return Err(PreparationError::Binding(PreparationBinding::OwnerNode));
        }
        let transcript = claims.transcript()?;
        let signature = self.sign_transcript(&transcript).await?;
        Ok(PreparationReceipt::new(claims.clone(), signature))
    }

    /// 用**本节点**身份公钥校验准备凭据（组合根/服务接线入口）。
    ///
    /// 公钥只从本节点 keystore 条目读取，调用方无从传入对端公钥。检查顺序与
    /// [`verify_preparation_receipt`](crate::preparation::verify_preparation_receipt) 相同；
    /// `authenticated.credential` MUST 由调用方在调用前刷新为最新持久设备状态。每帧调用会读一次
    /// keystore；连接级缓存 `node_public_key()` 后直接调用纯函数可以避免重复解包。
    pub async fn verify_preparation_receipt(
        &self,
        authenticated: &Authenticated,
        receipt: &PreparationReceipt,
        expected: &PreparationExpectation,
    ) -> Result<PreparationAdmission, PreparationError> {
        if expected.owner_node_id != self.local_node {
            return Err(PreparationError::Binding(PreparationBinding::OwnerNode));
        }
        let public_key = self.keystore.public_key(&self.node_key).await?;
        crate::preparation::verify_preparation_receipt(
            authenticated,
            &public_key,
            receipt,
            expected,
        )
    }

    /// 进程重启语义：丢弃全部内存 secret、配对码反查与挑战缓存（返回 `(secrets, challenges)`
    /// 清理数）。
    ///
    /// 组合根在启动时调用一次；随后应按
    /// [`Authority::unrecoverable_after_restart`](crate::pairing::Authority::unrecoverable_after_restart)
    /// 终结存储里已无法继续验密的配对。启动后配对码不能从持久化摘要重建（design D3）。
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
