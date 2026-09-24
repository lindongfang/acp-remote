//! 入口：`identity-auth` 身份端口的**平台安全存储**适配器（`docs/MODULE_ARCHITECTURE.md` §3.1/§4.12）。
//!
//! 职责与边界：
//!
//! - **只实现端口**：本 crate 实现 `identity_auth::IdentityKeystore` 与 `identity_auth::EntropySource`，
//!   不承载业务规则、不认识会话/命令/协议；除组合根外没有 crate 依赖它。
//! - **私钥不出端口**：私钥/秘密值只在进程内解开，签名在进程内完成（`p256` + 64 字节 P1363），
//!   公开 API 里没有任何「导出私钥」的路径。
//! - **平台失败即失败关闭**：非 Windows 平台在本阶段**不提供**可用后端（`SECURITY_DESIGN.md` §20），
//!   任何写入/读取都返回 `KeystoreError::Unavailable`，**不**回退到进程内实现、**不**退化到弱随机源。
//! - **平台差异只在本 crate 的 `platform` 模块内**：`identity-auth` 保持纯状态机，`cfg` 只出现在这里。
//!
//! 条目格式（见 [`entry`]）：明文头（magic/版本/用途/标签/盐）+ **平台包裹的秘密值**；
//! 秘密值只在被包裹的状态下落盘，附加熵由「盐 + 用途 + 标签 + 版本 + 域分离标签」派生。

pub mod entropy;
pub mod entry;
pub mod ephemeral;
pub mod error;
pub mod platform;
pub mod store;

pub use entropy::OsEntropy;
pub use ephemeral::EphemeralKeystore;
pub use error::StoreError;
pub use store::FileKeystore;

/// 本平台是否提供可用的安全存储后端。
///
/// 组合根必须据此决定启动策略（`identity.fail_closed_on_missing_keystore`），不得把
/// 「后端不可用」静默降级成「在进程内生成一把新身份」。
pub const fn platform_supported() -> bool {
    cfg!(windows)
}
