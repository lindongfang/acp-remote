//! 平台相关的 endpoint 实现（`docs/LOCAL_ADMIN_PROTOCOL.md` §2）。
//!
//! 平台分支刻意只出现在本目录：Windows 用 Named Pipe + SDDL（经 `windows-local-ipc` 的 safe API），Unix 用
//! Unix socket + `0700`/`0600` 权限 + `SO_PEERCRED` 对端 uid 比对。两边的公开形状一致——
//! `LocalEndpoint::bind` / `accept` 与 [`LocalStream`]——因此上层（`serve_connection`、`app::daemon`）
//! 不需要任何 `cfg`。

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::{LocalEndpoint, LocalStream};
#[cfg(windows)]
pub use windows::{LocalEndpoint, LocalStream};
