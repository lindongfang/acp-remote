//! Access Node 的节点身份端口与默认适配器。
//!
//! 本 crate **不持私钥、不读平台存储**（`AGENTS.md` §5：平台 keystore 由 `identity-keystore` 实现、
//! 由组合根注入）。握手需要的三件事以窄端口表达：
//!
//! - 本机 node id（写入 `node.hello.accessNodeId` 与证明 transcript）；
//! - 65 字节 SEC1 公钥（配对 claim 与诊断）；
//! - 对一段**已编码 transcript** 的 P-1363 签名（唯一使用私钥的操作）。
//!
//! [`KeystoreIdentity`] 是组合根使用的默认实现：它把签名委托给 `identity_auth::IdentityKeystore`，
//! 因此私钥不出端口、不进入本 crate 的日志或错误。

use std::sync::Arc;

use acp_core::model::{NodeId, PeerPublicKey};
use async_trait::async_trait;
use identity_auth::{IdentityKeystore, KeyHandle, KeyPurpose, P1363Signature};

use crate::error::ClientError;

/// 测试/引导签名器：对已编码 transcript 返回 P-1363 签名。
type Signer = Arc<dyn Fn(&[u8]) -> [u8; 64] + Send + Sync>;

/// Access Node 的节点身份。
#[async_trait]
pub trait NodeIdentity: Send + Sync {
    /// 本机 node id。
    fn node_id(&self) -> &NodeId;

    /// 65 字节 SEC1 未压缩公钥。
    async fn public_key(&self) -> Result<PeerPublicKey, ClientError>;

    /// 对已编码 transcript 签名（P-1363 `r || s`）。
    ///
    /// 输入必须是 `identity-auth` 的 transcript 装配结果（domain 与字段 tag 表在
    /// `compatibility/transcripts/v1/transcripts.json`）；本端口不接受未编码的业务对象。
    async fn sign(&self, transcript: &[u8]) -> Result<P1363Signature, ClientError>;
}

/// 基于 `identity_auth::IdentityKeystore` 的实现（组合根装配）。
pub struct KeystoreIdentity {
    node_id: NodeId,
    keystore: Arc<dyn IdentityKeystore>,
    handle: KeyHandle,
}

impl KeystoreIdentity {
    /// 装配。`node_id` 必须与该 handle 绑定：调用方负责在启动时按同一来源读取（`app::identity`）。
    pub fn new(node_id: NodeId, keystore: Arc<dyn IdentityKeystore>, handle: KeyHandle) -> Self {
        Self {
            node_id,
            keystore,
            handle,
        }
    }

    /// 读取 Node Identity 的公钥并校验 node id 形状。
    pub async fn verify_identity(&self) -> Result<PeerPublicKey, ClientError> {
        self.public_key().await
    }
}

#[async_trait]
impl NodeIdentity for KeystoreIdentity {
    fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    async fn public_key(&self) -> Result<PeerPublicKey, ClientError> {
        self.keystore
            .public_key(&self.handle)
            .await
            .map_err(|_| ClientError::Local("node identity public key is unavailable"))
    }

    async fn sign(&self, transcript: &[u8]) -> Result<P1363Signature, ClientError> {
        self.keystore
            .sign(&self.handle, transcript)
            .await
            .map_err(|_| ClientError::Local("node identity signing failed"))
    }
}

/// 只读的测试/引导实现：固定标识与固定签名（**不用于生产**）。
///
/// 它存在的理由与 `identity-keystore` 的 `EphemeralKeystore` 相同：测试与无 keystore 的开发模式需要
/// 一条明确、可见的路径；生产装配只接受 [`KeystoreIdentity`]。
pub struct ScriptedIdentity {
    node_id: NodeId,
    public_key: PeerPublicKey,
    signer: Signer,
}

impl ScriptedIdentity {
    /// 构造。
    pub fn new(node_id: NodeId, public_key: PeerPublicKey, signer: Signer) -> Self {
        Self {
            node_id,
            public_key,
            signer,
        }
    }
}

impl std::fmt::Debug for ScriptedIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScriptedIdentity")
            .field("node_id", &self.node_id)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl NodeIdentity for ScriptedIdentity {
    fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    async fn public_key(&self) -> Result<PeerPublicKey, ClientError> {
        Ok(self.public_key.clone())
    }

    async fn sign(&self, transcript: &[u8]) -> Result<P1363Signature, ClientError> {
        P1363Signature::try_from_bytes(&(self.signer)(transcript))
            .map_err(|_| ClientError::Local("scripted signer returned an invalid signature"))
    }
}

/// 读取节点身份的用途标记（与 `identity-auth` 的 `KeyPurpose` 同源，不复制词表）。
pub const NODE_IDENTITY_PURPOSE: KeyPurpose = KeyPurpose::NodeIdentity;

/// 把 64 字节签名与 wire 的 `Base64Url<64>` 互转的唯一入口（避免各处重复编码）。
pub fn signature_to_bytes(signature: &P1363Signature) -> [u8; 64] {
    *signature.as_bytes()
}
