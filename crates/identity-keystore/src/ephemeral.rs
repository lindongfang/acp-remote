//! 进程内 keystore：**显式构造**，供测试与本地开发使用，不是默认档位。
//!
//! 为什么存在：Linux/CI 容器没有可用的平台 keystore（`ADR-0006`），而适配器与组合根仍需在
//! 这些环境里跑通「端口形状 + 签名/验签一致」的路径。因此本实现：
//!
//! - **没有 `Default`**：必须显式 `EphemeralKeystore::new()`，避免任何「拿不到就默默用它」；
//! - **不落盘、不伪装平台能力**：所有条目只在内存；`platform_supported()` 仍如实反映平台；
//! - **可以注入固定标量**（[`EphemeralKeystore::with_seed`]），让测试可重跑而不用弱随机源。

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use identity_auth::PeerPublicKey;
use identity_auth::{
    EntropySource, IdentityKeystore, KeyHandle, KeyPurpose, KeystoreError, P1363Signature,
    SecretBytes, SecretPurpose,
};
use p256::elliptic_curve::sec1::ToEncodedPoint as _;

/// 进程内条目。
///
/// 秘密统一放在 [`SecretBytes`] 里：它是「析构即清零」的容器，因此进程内条目不再留一份
/// 未清零的明文 `Vec<u8>`（副本同样以 `SecretBytes` 形式返回给调用方）。
struct Entry {
    secret: SecretBytes,
}

/// 进程内 keystore。
pub struct EphemeralKeystore {
    entropy: std::sync::Arc<dyn EntropySource>,
    entries: Mutex<BTreeMap<String, Entry>>,
}

impl EphemeralKeystore {
    /// 取条目表；锁中毒映射为端口错误而不是 panic（`AGENTS.md` §7：正常路径不用 `unwrap`/`expect`）。
    fn entries(&self) -> Result<std::sync::MutexGuard<'_, BTreeMap<String, Entry>>, KeystoreError> {
        self.entries.lock().map_err(|_| KeystoreError::Unavailable)
    }

    /// 用给定熵源构造（生成节点标量时用它取随机数）。
    pub fn new(entropy: std::sync::Arc<dyn EntropySource>) -> Self {
        Self {
            entropy,
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    /// 用固定标量预置一个节点身份条目（**仅供测试与本地开发**：生产路径必须走平台后端，
    /// 或显式选择本类型并自行承担「内存明文」这一取舍；调用方只能来自组合根/测试）。
    pub fn with_seed(seed: [u8; 32]) -> Self {
        let keystore = Self::new(std::sync::Arc::new(crate::OsEntropy::new()));
        let mut seed = seed;
        keystore.entries().expect("新建实例的锁不可能中毒").insert(
            format!(
                "{}/primary",
                crate::entry::EntryPurpose::NodeIdentity.directory()
            ),
            Entry {
                secret: SecretBytes::new(&seed),
            },
        );
        // 形参是栈上数组：复制进 `SecretBytes` 后立刻清零，避免在调用者栈帧里留下副本。
        seed.fill(0);
        keystore
    }

    fn generate_scalar(&self) -> Result<SecretBytes, KeystoreError> {
        for _ in 0..8 {
            let mut candidate = [0u8; 32];
            self.entropy.fill(&mut candidate).map_err(|_| {
                candidate.fill(0);
                KeystoreError::Unavailable
            })?;
            if p256::SecretKey::from_slice(&candidate).is_ok() {
                let scalar = SecretBytes::new(&candidate);
                candidate.fill(0);
                return Ok(scalar);
            }
            candidate.fill(0);
        }
        Err(KeystoreError::Unavailable)
    }

    /// 取一份秘密副本（清零容器）；条目里那份保留到条目被删除/进程退出。
    fn get(&self, handle: &KeyHandle) -> Result<SecretBytes, KeystoreError> {
        self.entries()?
            .get(handle.as_str())
            .map(|entry| SecretBytes::new(entry.secret.as_bytes()))
            .ok_or(KeystoreError::EntryMissing)
    }
}

#[async_trait]
impl IdentityKeystore for EphemeralKeystore {
    async fn generate(&self, purpose: KeyPurpose, label: &str) -> Result<KeyHandle, KeystoreError> {
        let scalar = self.generate_scalar()?;
        let handle = KeyHandle::new(&format!("{}/{label}", purpose.as_str()))
            .map_err(|_| KeystoreError::EntryInvalid)?;
        self.entries()?
            .insert(handle.as_str().to_owned(), Entry { secret: scalar });
        Ok(handle)
    }

    async fn public_key(&self, handle: &KeyHandle) -> Result<PeerPublicKey, KeystoreError> {
        let secret = self.get(handle)?;
        let secret_key = p256::SecretKey::from_slice(secret.as_bytes())
            .map_err(|_| KeystoreError::EntryCorrupt)?;
        let point = secret_key.public_key().to_encoded_point(false);
        PeerPublicKey::try_from_bytes(point.as_bytes()).map_err(|_| KeystoreError::EntryCorrupt)
    }

    async fn sign(
        &self,
        handle: &KeyHandle,
        transcript: &[u8],
    ) -> Result<P1363Signature, KeystoreError> {
        use p256::ecdsa::signature::Signer as _;

        let secret = self.get(handle)?;
        let secret_key = p256::SecretKey::from_slice(secret.as_bytes())
            .map_err(|_| KeystoreError::EntryCorrupt)?;
        let signature: p256::ecdsa::Signature =
            p256::ecdsa::SigningKey::from(secret_key).sign(transcript);
        P1363Signature::try_from_bytes(signature.to_bytes().as_slice())
            .map_err(|_| KeystoreError::EntryCorrupt)
    }

    async fn delete(&self, handle: &KeyHandle) -> Result<(), KeystoreError> {
        let removed = self.entries()?.remove(handle.as_str());
        match removed {
            Some(_) => Ok(()),
            None => Err(KeystoreError::EntryMissing),
        }
    }

    async fn get_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
    ) -> Result<Option<SecretBytes>, KeystoreError> {
        let handle = KeyHandle::new(&format!("{}/{key}", purpose.as_str()))
            .map_err(|_| KeystoreError::EntryInvalid)?;
        match self.get(&handle) {
            Ok(secret) => Ok(Some(secret)),
            Err(KeystoreError::EntryMissing) => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn put_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
        value: &SecretBytes,
    ) -> Result<(), KeystoreError> {
        let handle = KeyHandle::new(&format!("{}/{key}", purpose.as_str()))
            .map_err(|_| KeystoreError::EntryInvalid)?;
        self.entries()?.insert(
            handle.as_str().to_owned(),
            Entry {
                secret: SecretBytes::new(value.as_bytes()),
            },
        );
        Ok(())
    }

    async fn delete_secret(&self, purpose: SecretPurpose, key: &str) -> Result<(), KeystoreError> {
        let handle = KeyHandle::new(&format!("{}/{key}", purpose.as_str()))
            .map_err(|_| KeystoreError::EntryInvalid)?;
        match self.delete(&handle).await {
            Ok(()) => Ok(()),
            Err(KeystoreError::EntryMissing) => Err(KeystoreError::SecretMissing),
            Err(error) => Err(error),
        }
    }
}
