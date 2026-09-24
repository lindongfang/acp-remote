//! 条目仓库：目录布局、原子写、读/删与端口实现。
//!
//! 布局（`design.md` D6）：`<root>/<purpose>/<label>.<version>`；`<root>` 的父子目录由调用方给出
//! （Daemon 用配置里的数据目录），本 crate 只在**包裹成功之后**才创建目录与文件。
//!
//! 原子写：同目录临时文件 → `fsync` → `rename`。写失败的临时文件会被删除，不会留下半成品条目
//! （对照合同 §7 的「条目损坏时失败而不覆盖」）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use identity_auth::{
    EntropySource, IdentityKeystore, KeyHandle, KeyPurpose, KeystoreError, P1363Signature,
    PeerPublicKey, SecretBytes, SecretPurpose,
};
use p256::elliptic_curve::sec1::ToEncodedPoint as _;

use crate::entry::{
    EntryHeader, EntryPurpose, FORMAT_VERSION, MAX_LABEL_LEN, PRIVATE_KEY_LEN, SALT_LEN,
};
use crate::error::StoreError;
use crate::platform::{unwrap_secret, wrap_secret};

/// 生成私钥标量时的最大重试次数（熵源给出零值/超范围的标量时必须换一个，而不是接受它）。
const MAX_SCALAR_ATTEMPTS: usize = 8;

/// 平台安全存储的目录实现。
pub struct FileKeystore {
    root: PathBuf,
    entropy: Arc<dyn EntropySource>,
}

impl FileKeystore {
    /// 构造。`root` 由组合根决定；本方法**不**创建任何目录（避免「不可用平台也留下目录」）。
    pub fn new(root: impl Into<PathBuf>, entropy: Arc<dyn EntropySource>) -> Self {
        Self {
            root: root.into(),
            entropy,
        }
    }

    /// 根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 条目路径：`<root>/<purpose>/<label>.<version>`。
    pub fn entry_path(&self, purpose: EntryPurpose, label: &str) -> PathBuf {
        self.root
            .join(purpose.directory())
            .join(format!("{label}.{FORMAT_VERSION}"))
    }

    /// 列出某用途下已存在的标签（用于孤儿回收与诊断；不返回秘密材料）。
    pub fn list(&self, purpose: EntryPurpose) -> Result<Vec<String>, StoreError> {
        let directory = self.root.join(purpose.directory());
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(StoreError::Io(error.kind().to_string())),
        };
        let suffix = format!(".{FORMAT_VERSION}");
        let mut labels = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| StoreError::Io(error.kind().to_string()))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some(label) = name.strip_suffix(&suffix) {
                labels.push(label.to_owned());
            }
        }
        labels.sort();
        Ok(labels)
    }

    /// 读取条目：校验头部与请求一致，返回头部与被包裹的秘密值。
    fn read_entry(
        &self,
        purpose: EntryPurpose,
        label: &str,
    ) -> Result<(EntryHeader, Vec<u8>), StoreError> {
        let path = self.entry_path(purpose, label);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(StoreError::Missing);
            }
            Err(error) => return Err(StoreError::Io(error.kind().to_string())),
        };
        let (header, _, wrapped) = EntryHeader::decode(&bytes)?;
        header.matches(purpose, label)?;
        Ok((header, wrapped.to_vec()))
    }

    /// 原子写入条目：先包裹、再建目录、再临时文件 + rename。任一步失败都不留半成品。
    fn write_entry(
        &self,
        purpose: EntryPurpose,
        label: &str,
        secret: &[u8],
    ) -> Result<(), StoreError> {
        if !purpose.accepts_len(secret.len()) {
            return Err(StoreError::TooLong);
        }
        let mut salt = [0u8; SALT_LEN];
        self.entropy
            .fill(&mut salt)
            .map_err(|_| StoreError::PlatformUnavailable)?;
        let header = EntryHeader::new(purpose, label, FORMAT_VERSION, salt)?;
        let entropy = header.additional_entropy();
        // 先包裹：非 Windows 平台在这里失败，因此**不会**创建目录或文件。
        let wrapped = wrap_secret(secret, &entropy)?;

        let path = self.entry_path(purpose, label);
        let directory = path
            .parent()
            .ok_or(StoreError::HeaderInvalid)?
            .to_path_buf();
        create_private_directory(&directory)?;

        let mut bytes = header.encode();
        bytes.extend_from_slice(&wrapped);
        write_atomic(&path, &bytes)
    }

    /// 解开条目并返回明文秘密值。
    fn read_secret(&self, purpose: EntryPurpose, label: &str) -> Result<Vec<u8>, StoreError> {
        let (header, wrapped) = self.read_entry(purpose, label)?;
        let entropy = header.additional_entropy();
        let secret = unwrap_secret(&wrapped, &entropy)?;
        if !purpose.accepts_len(secret.len()) {
            return Err(StoreError::Corrupt);
        }
        Ok(secret)
    }

    /// 删除条目；不存在时返回明确错误（不静默成功）。
    fn remove_entry(&self, purpose: EntryPurpose, label: &str) -> Result<(), StoreError> {
        let path = self.entry_path(purpose, label);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(StoreError::Missing),
            Err(error) => Err(StoreError::Io(error.kind().to_string())),
        }
    }

    /// 生成一个新的节点身份私钥标量（拒绝零值与非曲线标量）。
    fn generate_scalar(&self) -> Result<[u8; PRIVATE_KEY_LEN], StoreError> {
        for _ in 0..MAX_SCALAR_ATTEMPTS {
            let mut candidate = [0u8; PRIVATE_KEY_LEN];
            self.entropy
                .fill(&mut candidate)
                .map_err(|_| StoreError::PlatformUnavailable)?;
            if p256::SecretKey::from_slice(&candidate).is_ok() {
                return Ok(candidate);
            }
        }
        Err(StoreError::PlatformUnavailable)
    }
}

#[async_trait]
impl IdentityKeystore for FileKeystore {
    async fn generate(&self, purpose: KeyPurpose, label: &str) -> Result<KeyHandle, KeystoreError> {
        let entry_purpose = EntryPurpose::from_key_purpose(purpose);
        let scalar = self.generate_scalar()?;
        self.write_entry(entry_purpose, label, &scalar)?;
        handle_of(entry_purpose, label)
    }

    async fn public_key(&self, handle: &KeyHandle) -> Result<PeerPublicKey, KeystoreError> {
        let (purpose, label) = parse_handle(handle)?;
        let secret = self.read_secret(purpose, &label)?;
        let secret_key =
            p256::SecretKey::from_slice(&secret).map_err(|_| KeystoreError::EntryCorrupt)?;
        let point = secret_key.public_key().to_encoded_point(false);
        PeerPublicKey::try_from_bytes(point.as_bytes()).map_err(|_| KeystoreError::EntryCorrupt)
    }

    async fn sign(
        &self,
        handle: &KeyHandle,
        transcript: &[u8],
    ) -> Result<P1363Signature, KeystoreError> {
        use p256::ecdsa::signature::Signer as _;

        let (purpose, label) = parse_handle(handle)?;
        if purpose != EntryPurpose::NodeIdentity {
            return Err(KeystoreError::PurposeMismatch);
        }
        let secret = self.read_secret(purpose, &label)?;
        let secret_key =
            p256::SecretKey::from_slice(&secret).map_err(|_| KeystoreError::EntryCorrupt)?;
        let signing_key = p256::ecdsa::SigningKey::from(secret_key);
        let signature: p256::ecdsa::Signature = signing_key.sign(transcript);
        P1363Signature::try_from_bytes(signature.to_bytes().as_slice())
            .map_err(|_| KeystoreError::EntryCorrupt)
    }

    async fn delete(&self, handle: &KeyHandle) -> Result<(), KeystoreError> {
        let (purpose, label) = parse_handle(handle)?;
        self.remove_entry(purpose, &label)?;
        Ok(())
    }

    async fn get_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
    ) -> Result<Option<SecretBytes>, KeystoreError> {
        let entry_purpose = EntryPurpose::from_secret_purpose(purpose);
        match self.read_secret(entry_purpose, key) {
            Ok(secret) => Ok(Some(SecretBytes::new(&secret))),
            Err(StoreError::Missing) => Ok(None),
            Err(error) => Err(error.into_port()),
        }
    }

    async fn put_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
        value: &SecretBytes,
    ) -> Result<(), KeystoreError> {
        let entry_purpose = EntryPurpose::from_secret_purpose(purpose);
        self.write_entry(entry_purpose, key, value.as_bytes())?;
        Ok(())
    }

    async fn delete_secret(&self, purpose: SecretPurpose, key: &str) -> Result<(), KeystoreError> {
        let entry_purpose = EntryPurpose::from_secret_purpose(purpose);
        match self.remove_entry(entry_purpose, key) {
            Ok(()) => Ok(()),
            Err(StoreError::Missing) => Err(KeystoreError::SecretMissing),
            Err(error) => Err(error.into_port()),
        }
    }
}

/// 由用途与标签构造端口引用（文本形状 `<purpose>/<label>`）。
fn handle_of(purpose: EntryPurpose, label: &str) -> Result<KeyHandle, KeystoreError> {
    KeyHandle::new(&format!("{}/{label}", purpose.directory()))
        .map_err(|_| KeystoreError::EntryInvalid)
}

/// 解析端口引用；用途不匹配返回 `PurposeMismatch`。
fn parse_handle(handle: &KeyHandle) -> Result<(EntryPurpose, String), KeystoreError> {
    let text = handle.as_str();
    let (purpose_text, label) = text.split_once('/').ok_or(KeystoreError::EntryInvalid)?;
    let purpose = match purpose_text {
        "node-identity" => EntryPurpose::NodeIdentity,
        "provider-credential" => EntryPurpose::ProviderCredential,
        _ => return Err(KeystoreError::PurposeMismatch),
    };
    if label.is_empty()
        || label.len() > MAX_LABEL_LEN
        || label.contains(['/', '\\', '\0'])
        || label.chars().any(char::is_control)
    {
        return Err(KeystoreError::EntryInvalid);
    }
    Ok((purpose, label.to_owned()))
}

/// 创建目录（Unix 上限制为 `0700`）。
fn create_private_directory(directory: &Path) -> Result<(), StoreError> {
    if directory.exists() {
        return Ok(());
    }
    fs::create_dir_all(directory).map_err(|error| StoreError::Io(error.kind().to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| StoreError::Io(error.kind().to_string()))?;
    }
    Ok(())
}

/// 原子写：临时文件 → （Unix `0600`）→ `rename`；失败时清理临时文件。
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let parent = path.parent().ok_or(StoreError::HeaderInvalid)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(StoreError::HeaderInvalid)?;
    let temporary = parent.join(format!(".{name}.tmp-{}", std::process::id()));

    {
        use std::io::Write as _;
        let mut file = fs::File::create(&temporary)
            .map_err(|error| StoreError::Io(error.kind().to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|error| StoreError::Io(error.kind().to_string()))?;
        }
        file.write_all(bytes)
            .map_err(|error| StoreError::Io(error.kind().to_string()))?;
        file.sync_all()
            .map_err(|error| StoreError::Io(error.kind().to_string()))?;
    }

    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        StoreError::Io(error.kind().to_string())
    })
}
