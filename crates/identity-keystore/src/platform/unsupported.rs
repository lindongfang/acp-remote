//! 非 Windows 平台的失败关闭后端。
//!
//! 本阶段只有 Windows x64 是交付目标（`docs/SECURITY_DESIGN.md` §20）：其它平台**没有**可用的
//! 安全存储实现，因此这里返回明确的「不可用」，并且：
//!
//! - **不写任何字节**：调用方（[`crate::store::FileKeystore`]）先包裹、后落盘，包裹失败就不会有文件；
//! - **不降级**：不退回进程内实现、不用弱随机源、不把秘密值明文落盘；
//! - **可诊断**：`KeystoreError::Unavailable` 是封闭分类里唯一表示「本平台不可用」的取值。

use crate::error::StoreError;

/// 总是失败：本平台没有可用的包裹后端。
pub fn wrap_secret(_plaintext: &[u8], _entropy: &[u8; 32]) -> Result<Vec<u8>, StoreError> {
    Err(StoreError::PlatformUnavailable)
}

/// 总是失败：本平台没有可用的解包后端。
pub fn unwrap_secret(_wrapped: &[u8], _entropy: &[u8; 32]) -> Result<Vec<u8>, StoreError> {
    Err(StoreError::PlatformUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_windows_backend_never_succeeds() {
        assert_eq!(
            wrap_secret(b"secret", &[0u8; 32]),
            Err(StoreError::PlatformUnavailable)
        );
        assert_eq!(
            unwrap_secret(b"blob", &[0u8; 32]),
            Err(StoreError::PlatformUnavailable)
        );
    }
}
