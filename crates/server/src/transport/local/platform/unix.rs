//! `cfg(unix)`：Unix socket endpoint、权限位与对端 uid 校验（`docs/LOCAL_ADMIN_PROTOCOL.md` §2.1/§2.2）。
//!
//! - 位置：`$XDG_RUNTIME_DIR/acp-remote/<instanceId>.sock`，`XDG_RUNTIME_DIR` 未设置时回落
//!   `<daemon.data_dir>/run/<instanceId>.sock`；目录 `0700`、socket 文件 `0600`。
//! - 权限「设置后核对」：设置失败或核对不符即拒绝启动（§2.1、§7）。
//! - 旧 socket 处理：持锁后（调用方负责持锁）确认没有活跃 listener 才允许删除重建，否则拒绝启动。
//! - 对端凭据：`SO_PEERCRED` 取 uid 与本进程 uid 比对；取不到或不一致一律拒绝本次连接。
//!
//! 目标平台差异：nix 的 `sockopt::PeerCredentials`（`SO_PEERCRED`）只在 Linux/Android 上存在。
//! 其他 Unix 目标（如 macOS：等价物是 `LOCAL_PEERCRED`/`XuCred`）上本实现**不静默放行**，而是让每次
//! `accept` 以 `PeerIdentityUnavailable` 拒绝连接并记审计——失败关闭，等对应平台的交付补齐（`INITIAL_DESIGN.md`
//! §14：macOS 不在本轮验收范围）。

use std::io;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::net::{UnixListener, UnixStream};

use crate::transport::local::audit::{AuditHook, DeniedReason, deny_connection};
use crate::transport::local::endpoint::{
    EndpointError, InstanceId, LocalEndpointConfig, UnsafePathKind, unix_endpoint_directory,
    unix_socket_path,
};

/// 已通过 endpoint 权限与对端凭据校验的连接。
pub type LocalStream = UnixStream;

/// 目录要求的模式位（§2.1）。
const DIRECTORY_MODE: u32 = 0o700;

/// socket 文件要求的模式位（§2.1）。
const SOCKET_MODE: u32 = 0o600;

/// Unix socket endpoint。
pub struct LocalEndpoint {
    socket_path: PathBuf,
    instance_id: InstanceId,
    audit: Arc<dyn AuditHook>,
    listener: UnixListener,
}

impl LocalEndpoint {
    /// 按 §2.1 创建 endpoint。
    ///
    /// **前置条件**：调用方已持有单实例锁（`app::daemon` 的职责）。本函数无法自行验证锁，但它据此把
    /// 「删除并重建旧 socket」当作安全操作；没有锁时两个实例可能互相抢同一路径。
    ///
    /// 失败即失败关闭：调用方必须拒绝启动。
    pub async fn bind(
        config: LocalEndpointConfig,
        audit: Arc<dyn AuditHook>,
    ) -> Result<Self, EndpointError> {
        let xdg_runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let directory = unix_endpoint_directory(&config, xdg_runtime_dir.as_deref());
        ensure_private_directory(&directory)?;
        let socket_path = unix_socket_path(&directory, &config.instance_id);
        remove_stale_socket(&socket_path)?;
        let listener =
            UnixListener::bind(&socket_path).map_err(|source| EndpointError::Os { source })?;
        set_socket_permissions(&socket_path)?;
        Ok(Self {
            socket_path,
            instance_id: config.instance_id,
            audit,
            listener,
        })
    }

    /// endpoint 的定位串（socket 路径；用于日志与诊断，非 UTF-8 时按 lossy 展示）。
    pub fn describe(&self) -> String {
        self.socket_path.display().to_string()
    }

    /// 接受下一条连接并完成 §2.2 的对端 uid 校验。
    ///
    /// 返回 `Err(EndpointError::PeerRejected)` 表示本次连接已被拒绝（不发送任何 frame，审计事件已记录）——
    /// 调用方继续接受下一条连接即可，这不是致命错误；其他错误表示 endpoint 本身不可用。
    pub async fn accept(&mut self) -> Result<LocalStream, EndpointError> {
        let (stream, _address) = self
            .listener
            .accept()
            .await
            .map_err(|source| EndpointError::Os { source })?;
        match peer_user_id(&stream) {
            Some(uid) if uid == current_user_id() => Ok(stream),
            Some(uid) => {
                let peer = uid.to_string();
                deny_connection(
                    self.audit.as_ref(),
                    "unix_socket",
                    self.instance_id.as_str(),
                    DeniedReason::DifferentOsUser,
                    Some(peer.as_str()),
                );
                Err(EndpointError::PeerRejected)
            }
            None => {
                tracing::warn!(
                    event = "local_admin.peer_identity_unavailable",
                    "无法确认对端 OS 用户（本目标没有 SO_PEERCRED 或系统调用失败）：拒绝本次连接"
                );
                deny_connection(
                    self.audit.as_ref(),
                    "unix_socket",
                    self.instance_id.as_str(),
                    DeniedReason::PeerIdentityUnavailable,
                    None,
                );
                Err(EndpointError::PeerRejected)
            }
        }
    }
}

/// 取对端进程的 uid；`None` = 本目标无法确认（没有 `SO_PEERCRED`，或系统调用失败）。
#[cfg(any(target_os = "linux", target_os = "android"))]
fn peer_user_id(stream: &UnixStream) -> Option<u32> {
    nix::sys::socket::getsockopt(stream, nix::sys::socket::sockopt::PeerCredentials)
        .ok()
        .map(|credentials| credentials.uid())
}

/// 非 Linux/Android 的 Unix 目标：没有 `SO_PEERCRED`，一律按「无法确认」处理（失败关闭）。
#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn peer_user_id(_stream: &UnixStream) -> Option<u32> {
    None
}

/// 本进程的有效 uid。
fn current_user_id() -> u32 {
    nix::unistd::getuid().as_raw()
}

/// 运行时目录：不存在则创建，符号链接或非目录一律拒绝，权限必须能保证为 `0700`。
fn ensure_private_directory(directory: &Path) -> Result<(), EndpointError> {
    match std::fs::symlink_metadata(directory) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(EndpointError::UnsafePath {
                    kind: UnsafePathKind::Symlink,
                });
            }
            if !metadata.file_type().is_dir() {
                return Err(EndpointError::UnsafePath {
                    kind: UnsafePathKind::NotADirectory,
                });
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            std::fs::create_dir_all(directory).map_err(|source| EndpointError::Os { source })?;
        }
        Err(source) => return Err(EndpointError::Os { source }),
    }
    // 「设置后核对」：别人拥有的目录上 chmod 会失败，权限位不符也拒绝启动（§2.1、§7）。
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(DIRECTORY_MODE))
        .map_err(|_| insecure_permissions("directory", DIRECTORY_MODE))?;
    let mode = directory_mode(directory)?;
    if mode != DIRECTORY_MODE {
        return Err(insecure_permissions("directory", DIRECTORY_MODE));
    }
    Ok(())
}

/// socket 文件的权限位：同样「设置后核对」。
fn set_socket_permissions(socket_path: &Path) -> Result<(), EndpointError> {
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(SOCKET_MODE))
        .map_err(|_| insecure_permissions("socket", SOCKET_MODE))?;
    let metadata =
        std::fs::metadata(socket_path).map_err(|_| insecure_permissions("socket", SOCKET_MODE))?;
    if metadata.permissions().mode() & 0o777 != SOCKET_MODE {
        return Err(insecure_permissions("socket", SOCKET_MODE));
    }
    Ok(())
}

/// 目录当前的模式位（低 9 位）。
fn directory_mode(directory: &Path) -> Result<u32, EndpointError> {
    let metadata = std::fs::metadata(directory)
        .map_err(|_| insecure_permissions("directory", DIRECTORY_MODE))?;
    Ok(metadata.permissions().mode() & 0o777)
}

fn insecure_permissions(kind: &'static str, expected: u32) -> EndpointError {
    EndpointError::InsecurePermissions { kind, expected }
}

/// 处理已存在的 socket 路径：符号链接与非 socket 一律拒绝；能连上说明仍有活跃 listener → 拒绝启动；
/// 否则视为上一次运行的残留，删除后重建（§7）。
fn remove_stale_socket(socket_path: &Path) -> Result<(), EndpointError> {
    let metadata = match std::fs::symlink_metadata(socket_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(EndpointError::Os { source }),
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Err(EndpointError::UnsafePath {
            kind: UnsafePathKind::Symlink,
        });
    }
    if !file_type.is_socket() {
        return Err(EndpointError::UnsafePath {
            kind: UnsafePathKind::NotASocket,
        });
    }
    // 存活探测：连得上说明另一个实例仍在监听（此时本实例不该启动）；连不上说明是残留 socket 文件。
    if std::os::unix::net::UnixStream::connect(socket_path).is_ok() {
        return Err(EndpointError::AlreadyInUse);
    }
    std::fs::remove_file(socket_path).map_err(|source| EndpointError::Os { source })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 对端凭据：`SO_PEERCRED` 取到的 uid 必须等于本进程 uid（同进程的一对 socket 是唯一可自证的输入）。
    /// 跨用户分支（uid 不等即拒绝）在本机单账号下无法真实构造，见交付报告的已知限制。
    #[tokio::test]
    async fn peer_user_id_matches_current_user_for_a_local_pair() {
        let (server, client) = UnixStream::pair().expect("socket pair");
        if cfg!(any(target_os = "linux", target_os = "android")) {
            assert_eq!(peer_user_id(&server), Some(current_user_id()));
            assert_eq!(peer_user_id(&client), Some(current_user_id()));
        } else {
            // 本目标没有 SO_PEERCRED：不静默放行，而是「无法确认」→ 拒绝连接。
            assert_eq!(peer_user_id(&server), None);
        }
    }
}
