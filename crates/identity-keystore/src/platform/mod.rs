//! 平台包裹：把秘密值交给平台安全存储（Windows DPAPI）或失败关闭。
//!
//! 平台**后端选择**只在本模块（`identity-auth` 保持纯状态机、零 `cfg`）；crate 内另有
//! `store.rs` 的 `#[cfg(unix)]` 权限位路径，两处合起来就是本 crate 的全部 `cfg`。
//! 两个后端提供**完全相同**的函数签名：
//!
//! - `windows`：DPAPI，`Scope::User`（当前用户），附加熵来自条目头；
//! - 其它平台：一律 `Err(StoreError::PlatformUnavailable)`，**不写任何字节**。

#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
mod unsupported;

#[cfg(windows)]
pub use windows::{unwrap_secret, wrap_secret};

#[cfg(not(windows))]
pub use unsupported::{unwrap_secret, wrap_secret};
