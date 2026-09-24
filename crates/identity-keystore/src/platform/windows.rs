//! Windows 后端：DPAPI（`Scope::User`）。
//!
//! 语义（`docs/SECURITY_DESIGN.md` §20）：`CryptProtectData`/`CryptUnprotectData` 只做
//! 「用当前用户的凭据包裹字节 / 解开字节」；私钥在进程内解开后立即用于 `p256` 签名，
//! 明文不出本进程、不进日志。
//!
//! 已知代价（实证见 `reports/wp3-dpapi-verification.log`）：
//!
//! - wrapper `windows-dpapi 0.2.0` 传递依赖已停止维护的 `winapi 0.3`；
//! - wrapper 不暴露 `CRYPTPROTECT_UI_FORBIDDEN`，因此本实现**总是**传入附加熵（由条目头派生），
//!   把「无熵 + 缺主密钥时可能弹出交互提示」这条路径收敛为「有熵的静默路径」；
//! - 换成自写 wrapper 或替换 crate 需要新 ADR（`docs/adr/0006-identity-keystore-split.md` 的边界不变）。

use identity_auth::KeystoreError;

use crate::error::StoreError;

/// 用当前用户凭据包裹秘密值。
pub fn wrap_secret(plaintext: &[u8], entropy: &[u8; 32]) -> Result<Vec<u8>, StoreError> {
    windows_dpapi::encrypt_data(plaintext, windows_dpapi::Scope::User, Some(entropy))
        // `anyhow::Error` **不**穿过端口：只在这里转成具名分类（文案里没有秘密材料）。
        .map_err(|_| StoreError::PlatformUnavailable)
}

/// 解开秘密值。失败（被篡改、跨用户、跨机器、熵不符）一律表现为「损坏」，不尝试修复或覆盖。
pub fn unwrap_secret(wrapped: &[u8], entropy: &[u8; 32]) -> Result<Vec<u8>, StoreError> {
    let plaintext = windows_dpapi::decrypt_data(wrapped, windows_dpapi::Scope::User, Some(entropy))
        .map_err(|_| StoreError::Corrupt)?;
    Ok(plaintext)
}

/// 端口错误映射的显式锚点（避免 `KeystoreError` 在本模块被间接引入时出现未使用导入）。
const _: fn(StoreError) -> KeystoreError = StoreError::into_port;
