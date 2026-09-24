//! 条目格式（`design.md` D6）：明文头 + 平台包裹的秘密值。
//!
//! ```text
//! offset  size  field
//! 0       4     magic  "ACPK"
//! 4       2     format 版本（u16be，当前 1）
//! 6       1     用途 token（1 = node-identity，2 = provider-credential）
//! 7       2     标签长度（u16be，≤ 128）
//! 9       n     标签（UTF-8；节点身份是 label，Provider 凭据是 key）
//! 9+n     32    盐（每条目独立；不参与保密，只用于派生附加熵）
//! 41+n    余下  被平台包裹的秘密值（DPAPI blob；非 Windows 平台不会产生此段）
//! ```
//!
//! 三条约束：
//!
//! 1. **头部是明文但可鉴别**：读取时头部字段必须与请求的用途/标签/版本一致，否则视为损坏——
//!    把条目文件挪到另一个标签下不会「正好还能用」。
//! 2. **秘密值只以被包裹形态落盘**：明文标量只在进程内存在，且写入前必须已经过平台包裹成功。
//! 3. **附加熵把条目绑死**：加密附加熵由「盐 + 版本 + 用途 + 标签 + 域分离标签」派生
//!    （[`EntryHeader::additional_entropy`]），因此复制/剪接条目不能解密。

use identity_auth::{KeyPurpose, SecretPurpose};
use sha2::Digest as _;

use crate::error::StoreError;

/// 条目文件 magic。
pub const MAGIC: &[u8; 4] = b"ACPK";
/// 当前格式版本。
pub const FORMAT_VERSION: u16 = 1;
/// 盐长度。
pub const SALT_LEN: usize = 32;
/// 标签/键的最大长度（字节）。
pub const MAX_LABEL_LEN: usize = 128;
/// Provider 凭据的最大长度（字节）。
pub const MAX_SECRET_LEN: usize = 4096;
/// 节点身份私钥标量的长度（字节）。
pub const PRIVATE_KEY_LEN: usize = 32;
/// 附加熵的域分离标签（长度前缀 transcript 的一部分）。
const ENTROPY_DOMAIN: &[u8] = b"acp-remote/keystore-entry/v1";

/// 标签形状是否合法：非空、≤ 128 字节、不含路径分隔符与控制字符。
///
/// **只此一份**：[\`EntryHeader::new\`]（写入路径）与 [\`EntryHeader::decode\`]（读取路径）共用它，
/// 因为标签会被拼进文件路径——两侧不对称就会给「decode 后自己拼路径」的调用方留下目录穿越。
pub fn is_valid_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= MAX_LABEL_LEN
        && !label.contains(['/', '\\', '\0'])
        && !label.chars().any(char::is_control)
}

/// 条目用途 token。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntryPurpose {
    /// 节点长期身份私钥（32 字节标量）。
    NodeIdentity,
    /// Provider 凭据（任意长度的秘密字节）。
    ProviderCredential,
}

impl EntryPurpose {
    /// wire/文件里的 token。
    pub const fn token(self) -> u8 {
        match self {
            Self::NodeIdentity => 1,
            Self::ProviderCredential => 2,
        }
    }

    /// 目录名（keystore 根下的子目录）。
    pub const fn directory(self) -> &'static str {
        match self {
            // 与 `KeyPurpose::as_str()` / `SecretPurpose::as_str()` 保持一致（同一份词表）。
            Self::NodeIdentity => "node-identity",
            Self::ProviderCredential => "provider-credential",
        }
    }

    /// 由 token 解析（未知 token 即条目损坏）。
    pub fn from_token(token: u8) -> Result<Self, StoreError> {
        match token {
            1 => Ok(Self::NodeIdentity),
            2 => Ok(Self::ProviderCredential),
            _ => Err(StoreError::HeaderInvalid),
        }
    }

    /// 由节点密钥用途映射。
    pub const fn from_key_purpose(purpose: KeyPurpose) -> Self {
        match purpose {
            KeyPurpose::NodeIdentity => Self::NodeIdentity,
        }
    }

    /// 由秘密用途映射。
    pub const fn from_secret_purpose(purpose: SecretPurpose) -> Self {
        match purpose {
            SecretPurpose::ProviderCredential => Self::ProviderCredential,
        }
    }

    /// 该用途允许的秘密值长度是否合法。
    pub const fn accepts_len(self, len: usize) -> bool {
        match self {
            Self::NodeIdentity => len == PRIVATE_KEY_LEN,
            Self::ProviderCredential => len >= 1 && len <= MAX_SECRET_LEN,
        }
    }
}

/// 条目头（明文部分）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryHeader {
    /// 用途。
    pub purpose: EntryPurpose,
    /// 标签（节点身份）或键（Provider 凭据）。
    pub label: String,
    /// 条目格式版本。
    pub version: u16,
    /// 每条目独立的盐。
    pub salt: [u8; SALT_LEN],
}

impl EntryHeader {
    /// 构造（标签长度与形状在此校验一次）。
    pub fn new(
        purpose: EntryPurpose,
        label: &str,
        version: u16,
        salt: [u8; SALT_LEN],
    ) -> Result<Self, StoreError> {
        if !is_valid_label(label) {
            return Err(StoreError::Path(label.to_owned()));
        }
        if version != FORMAT_VERSION {
            return Err(StoreError::HeaderInvalid);
        }
        Ok(Self {
            purpose,
            label: label.to_owned(),
            version,
            salt,
        })
    }

    /// 头部字节（不含被包裹的秘密值）。
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(9 + self.label.len() + SALT_LEN);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.version.to_be_bytes());
        out.push(self.purpose.token());
        out.extend_from_slice(&(self.label.len() as u16).to_be_bytes());
        out.extend_from_slice(self.label.as_bytes());
        out.extend_from_slice(&self.salt);
        out
    }

    /// 解析头部，并返回「头部长度」与「剩余字节（被包裹的秘密值）」。
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize, &[u8]), StoreError> {
        if bytes.len() < 9 || &bytes[..4] != MAGIC {
            return Err(StoreError::HeaderInvalid);
        }
        let version = u16::from_be_bytes([bytes[4], bytes[5]]);
        if version != FORMAT_VERSION {
            return Err(StoreError::HeaderInvalid);
        }
        let purpose = EntryPurpose::from_token(bytes[6])?;
        let label_len = usize::from(u16::from_be_bytes([bytes[7], bytes[8]]));
        if label_len == 0 || label_len > MAX_LABEL_LEN {
            return Err(StoreError::HeaderInvalid);
        }
        let label_end = 9 + label_len;
        let salt_end = label_end + SALT_LEN;
        if bytes.len() < salt_end {
            return Err(StoreError::HeaderInvalid);
        }
        let label = std::str::from_utf8(&bytes[9..label_end])
            .map_err(|_| StoreError::HeaderInvalid)?
            .to_owned();
        if !is_valid_label(&label) {
            return Err(StoreError::HeaderInvalid);
        }
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&bytes[label_end..salt_end]);
        let wrapped = &bytes[salt_end..];
        if wrapped.is_empty() {
            return Err(StoreError::Corrupt);
        }
        Ok((
            Self {
                purpose,
                label,
                version,
                salt,
            },
            salt_end,
            wrapped,
        ))
    }

    /// 平台包裹使用的附加熵：域分离 + 长度前缀，把条目绑到「用途 + 标签 + 版本 + 盐」。
    pub fn additional_entropy(&self) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();
        hasher.update(ENTROPY_DOMAIN);
        hasher.update(self.version.to_be_bytes());
        hasher.update([self.purpose.token()]);
        hasher.update((self.label.len() as u32).to_be_bytes());
        hasher.update(self.label.as_bytes());
        hasher.update(self.salt);
        let digest = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        out
    }

    /// 头部是否与请求的用途/标签/版本一致（不一致即条目被挪动或损坏）。
    pub fn matches(&self, purpose: EntryPurpose, label: &str) -> Result<(), StoreError> {
        if self.purpose != purpose || self.label != label || self.version != FORMAT_VERSION {
            return Err(StoreError::IdentityMismatch);
        }
        Ok(())
    }
}
