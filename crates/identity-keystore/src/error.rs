//! 失败分类：内部条目/文件错误 → `identity-auth` 端口错误。
//!
//! 端口边界只允许出现 `KeystoreError`（`identity-auth` 的封闭分类）；本模块的错误一律先映射再返回，
//! 因此 `anyhow`、`std::io::Error` 与平台错误**不会**穿过端口。错误文案里不含秘密材料：
//! 路径、用途 token 与失败类别可以出现，密钥字节、盐与秘密值不可以。

use identity_auth::KeystoreError;

/// 条目与文件层的失败。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("所在平台没有可用的安全存储后端")]
    PlatformUnavailable,
    #[error("条目路径不可用：{0}")]
    Path(String),
    #[error("条目头部不合法（magic/版本/长度/token）")]
    HeaderInvalid,
    #[error("条目标识与请求不一致（用途、标签或版本）")]
    IdentityMismatch,
    #[error("条目被篡改或无法解开")]
    Corrupt,
    #[error("条目不存在")]
    Missing,
    #[error("秘密值长度超出上限")]
    TooLong,
    #[error("写入失败：{0}")]
    Io(String),
}

impl StoreError {
    /// 映射到端口错误。`Corrupt` 与 `HeaderInvalid` 分开保留在内部，对端口统一表现为 `EntryCorrupt`。
    pub fn into_port(self) -> KeystoreError {
        match self {
            Self::PlatformUnavailable => KeystoreError::Unavailable,
            Self::Missing => KeystoreError::EntryMissing,
            Self::Corrupt | Self::HeaderInvalid | Self::IdentityMismatch => {
                KeystoreError::EntryCorrupt
            }
            Self::TooLong | Self::Path(_) => KeystoreError::EntryInvalid,
            Self::Io(_) => KeystoreError::Unavailable,
        }
    }
}

impl From<StoreError> for KeystoreError {
    fn from(error: StoreError) -> Self {
        error.into_port()
    }
}
