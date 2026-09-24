//! `identity-auth` 定义、组合根注入的端口（`docs/IDENTITY_AND_AUTH_CONTRACT.md` §7）。
//!
//! 平台实现（DPAPI/Keychain/Secret Service）在 `identity-keystore`；本文件只定义形状与不变量：
//! 私钥不出端口、`sign` 是使用私钥的唯一操作、`SecretBytes` 不实现 `Debug`/`Serialize`、
//! 熵源失败即失败关闭。

use std::fmt;

use acp_core::model::PeerPublicKey;
use async_trait::async_trait;

use crate::types::P1363Signature;

/// 长期密钥用途。v1 只有 Node Identity（`docs/IDENTITY_AND_AUTH_CONTRACT.md` §7：`DeviceIdentity`
/// 在第一阶段没有调用方，已从合同里删除；将来需要时按新用例重新登记）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyPurpose {
    NodeIdentity,
}

impl KeyPurpose {
    /// 全部取值（供存储层与测试穷举）。
    pub const ALL: &'static [Self] = &[Self::NodeIdentity];

    /// 稳定 token（keystore 目录名与错误信息共用；不含秘密）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NodeIdentity => "node-identity",
        }
    }
}

/// 需要平台保护的**秘密值**用途。v1 只有 Provider 凭据（配对 secret 不进 keystore）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretPurpose {
    ProviderCredential,
}

impl SecretPurpose {
    /// 全部取值。
    pub const ALL: &'static [Self] = &[Self::ProviderCredential];

    /// 稳定 token。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderCredential => "provider-credential",
        }
    }
}

/// 不透明引用：**不是**密钥材料，可以落进 SQLite 的引用列，但不得进日志（`SECURITY_DESIGN.md` §14.1）。
///
/// `Debug` 只表明类型、不打印内容（`AGENTS.md` §7「日志不得记录密钥」）：引用文本本身是
/// 可落库的定位符，但把它写进日志会把「哪把密钥存在」暴露给日志读者，因此一律不打印。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyHandle(String);

impl KeyHandle {
    /// 构造。只做形状校验（非空、≤256、无控制字符）；内容由实现自己解释。
    pub fn new(text: &str) -> Result<Self, KeystoreError> {
        let well_formed = !text.is_empty()
            && text.len() <= 256
            && !text.chars().any(|character| character.is_control());
        if !well_formed {
            return Err(KeystoreError::EntryInvalid);
        }
        Ok(Self(text.to_owned()))
    }

    /// 底层文本（写引用列时使用；**不要**写日志）。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for KeyHandle {
    /// 引用不进日志：只表明类型，不打印内容。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KeyHandle(<opaque>)")
    }
}

/// 秘密字节：只经端口进出。**不实现** `Debug`/`Display`/`Serialize`/`Clone`，避免被顺手打日志、
/// 写库或复制到别处；离开作用域时清零缓冲区。
///
/// 两个「不实现」是编译期约束，用 `compile_fail` 文档测试固定：
///
/// ```compile_fail
/// let secret = identity_auth::SecretBytes::new(b"value");
/// let _ = format!("{secret:?}");
/// ```
///
/// ```compile_fail
/// let secret = identity_auth::SecretBytes::new(b"value");
/// let copy = secret.clone();
/// let _ = copy;
/// ```
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    /// 从字节构造（复制一份）。
    pub fn new(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }

    /// 借用字节。
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// 取出字节（所有权转移）。
    pub fn into_bytes(mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }

    /// 字节数。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// 平台 keystore 失败分类。所有失败都**失败关闭**：调用方不得据此继续启动、继续认证或静默重建身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KeystoreError {
    #[error("平台安全存储不可用")]
    Unavailable,
    #[error("条目不存在")]
    EntryMissing,
    #[error("条目损坏或无法解封")]
    EntryCorrupt,
    #[error("条目标识非法")]
    EntryInvalid,
    #[error("条目用途与请求的用途不匹配")]
    PurposeMismatch,
    #[error("secret 不存在")]
    SecretMissing,
}

/// 平台安全存储端口。
///
/// 不变量（合同 §7）：私钥不出端口——`sign` 是唯一使用私钥的操作；`public_key` 必须与 `sign`
/// 使用同一把私钥（65 字节 SEC1 未压缩点）；条目缺失或损坏时返回明确错误，绝不静默生成新身份。
#[async_trait]
pub trait IdentityKeystore: Send + Sync {
    /// 生成一个新条目。实现决定条目落点；返回不透明引用。
    async fn generate(&self, purpose: KeyPurpose, label: &str) -> Result<KeyHandle, KeystoreError>;

    /// 返回 65 字节 SEC1 未压缩公钥。
    async fn public_key(&self, handle: &KeyHandle) -> Result<PeerPublicKey, KeystoreError>;

    /// 对已编码 transcript 签名，返回 64 字节 P1363（`r || s`）。
    async fn sign(
        &self,
        handle: &KeyHandle,
        transcript: &[u8],
    ) -> Result<P1363Signature, KeystoreError>;

    /// 删除条目。条目本身不可用时返回明确错误，不静默成功。
    async fn delete(&self, handle: &KeyHandle) -> Result<(), KeystoreError>;

    /// 读取秘密值（Provider 凭据）。
    async fn get_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
    ) -> Result<Option<SecretBytes>, KeystoreError>;

    /// 写入秘密值。
    async fn put_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
        value: &SecretBytes,
    ) -> Result<(), KeystoreError>;

    /// 删除秘密值。
    async fn delete_secret(&self, purpose: SecretPurpose, key: &str) -> Result<(), KeystoreError>;
}

/// 熵源失败。只有「不可用」一种结果：没有可用的系统熵源时**失败关闭**，不得退回弱随机源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EntropyError {
    #[error("系统熵源不可用")]
    Unavailable,
}

/// 熵源端口（同步）：挑战 nonce、配对 secret 与待生成密钥材料的**唯一**随机性来源。
///
/// 本 crate 不自研 PRNG，也不直接读系统随机数；实现由组合根注入（平台侧）。
pub trait EntropySource: Send + Sync {
    /// 用随机字节填满 `out`；失败即返回错误。
    fn fill(&self, out: &mut [u8]) -> Result<(), EntropyError>;
}
