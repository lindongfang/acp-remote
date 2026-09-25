//! `cfg(windows)`：Named Pipe endpoint 与对端 SID 校验（`docs/LOCAL_ADMIN_PROTOCOL.md` §2.1/§2.2）。
//!
//! - 首个 pipe 实例经 `windows_local_ipc::create_pipe_server` 创建：它带「仅当前用户 SID」的 SDDL 与
//!   `FILE_FLAG_FIRST_PIPE_INSTANCE`（同名第二个首实例会失败，因此「同一 endpoint 起两次」是明确错误）。
//! - 后续实例用 tokio 的 safe `ServerOptions`：同一 pipe 名的实例参数必须一致，tokio 的默认值
//!   （`PIPE_UNLIMITED_INSTANCES`、65536/65536 缓冲、字节模式、拒绝远端客户端）与 wrapper 的
//!   `CreateNamedPipeW` 参数刻意同值（`vendor/windows-local-ipc/src/windows.rs` 的常量注释、WP2 用例 ⑪）。
//! - 连接建立后必须校验对端凭据（§2.2）：`client_user_sid` 与本进程 SID 逐字比较；拿不到对端 SID
//!   （未连接、对端已退出、无权查询）一律按「凭据不一致」处理，绝不放行。

use std::io;
use std::sync::Arc;

use tokio::net::windows::named_pipe::ServerOptions;

use crate::transport::local::audit::{AuditHook, DeniedReason, deny_connection};
use crate::transport::local::endpoint::{
    EndpointError, InstanceId, LocalEndpointConfig, named_pipe_path,
};

/// 已通过 endpoint 权限与对端凭据校验的连接（双向字节流；framing 由上层负责）。
pub type LocalStream = tokio::net::windows::named_pipe::NamedPipeServer;

/// Windows 退出时该名字已有首个 pipe 实例的错误码（`ERROR_ACCESS_DENIED`，WP2 用例 ⑥ 实测）。
const ERROR_ACCESS_DENIED: i32 = 5;

/// Named Pipe endpoint：当前待连接的 pipe 实例 + 定位用的 pipe 名。
pub struct LocalEndpoint {
    pipe_name: String,
    instance_id: InstanceId,
    audit: Arc<dyn AuditHook>,
    /// 等待下一条连接的 pipe 实例；每次 `accept` 后立刻补一个新实例。
    pending: Option<LocalStream>,
}

impl LocalEndpoint {
    /// 按 §2.1 创建 endpoint。
    ///
    /// 失败即失败关闭：调用方（`app::daemon`）必须拒绝启动，不得降级为无管理通道。
    /// 必须在已开启 I/O 的 Tokio runtime 内调用（`create_pipe_server` 与 tokio 自己的 `ServerOptions::create`
    /// 是同一条约束）。
    pub async fn bind(
        config: LocalEndpointConfig,
        audit: Arc<dyn AuditHook>,
    ) -> Result<Self, EndpointError> {
        let user_sid =
            windows_local_ipc::current_user_sid().map_err(|source| EndpointError::Os { source })?;
        let pipe_name = named_pipe_path(&user_sid, &config.instance_id);
        let first = create_first_instance(&pipe_name)?;
        Ok(Self {
            pipe_name,
            instance_id: config.instance_id,
            audit,
            pending: Some(first),
        })
    }

    /// endpoint 的定位串（§2.1：pipe 名只用于定位，不视为秘密）。
    pub fn describe(&self) -> String {
        self.pipe_name.clone()
    }

    /// 接受下一条连接并完成 §2.2 的对端凭据校验。
    ///
    /// 返回 `Err(EndpointError::PeerRejected)` 表示本次连接已被拒绝（不发送任何 frame，审计事件已记录）——
    /// 调用方继续接受下一条连接即可，这不是致命错误；其他错误表示 endpoint 本身不可用。
    pub async fn accept(&mut self) -> Result<LocalStream, EndpointError> {
        let server = self.pending.take().ok_or_else(|| EndpointError::Os {
            source: io::Error::other("本地通道 endpoint 没有可用的 pipe 实例"),
        })?;
        server
            .connect()
            .await
            .map_err(|source| EndpointError::Os { source })?;
        // 先把同名的下一个实例准备好：无论本次连接是否被接受，endpoint 都必须继续可用。
        let next = ServerOptions::new()
            .create(&self.pipe_name)
            .map_err(|source| EndpointError::Os { source })?;
        self.pending = Some(next);

        let own_sid = windows_local_ipc::current_user_sid();
        let peer_sid = windows_local_ipc::client_user_sid(&server);
        match (own_sid, peer_sid) {
            (Ok(own), Ok(peer)) if own == peer => Ok(server),
            (Ok(_), Ok(peer)) => {
                deny_connection(
                    self.audit.as_ref(),
                    "named_pipe",
                    self.instance_id.as_str(),
                    DeniedReason::DifferentOsUser,
                    Some(peer.as_str()),
                );
                Err(EndpointError::PeerRejected)
            }
            (_, Ok(peer)) => {
                deny_connection(
                    self.audit.as_ref(),
                    "named_pipe",
                    self.instance_id.as_str(),
                    DeniedReason::PeerIdentityUnavailable,
                    Some(peer.as_str()),
                );
                Err(EndpointError::PeerRejected)
            }
            (_, Err(_)) => {
                // 拿不到对端 SID（未连接、对端已退出、无权查询）＝凭据无法确认，失败关闭。
                deny_connection(
                    self.audit.as_ref(),
                    "named_pipe",
                    self.instance_id.as_str(),
                    DeniedReason::PeerIdentityUnavailable,
                    None,
                );
                Err(EndpointError::PeerRejected)
            }
        }
    }
}

/// 创建该 pipe 名的首个实例（带仅当前用户 SID 的 SDDL）。
fn create_first_instance(pipe_name: &str) -> Result<LocalStream, EndpointError> {
    windows_local_ipc::create_pipe_server(pipe_name).map_err(|source| {
        if source.raw_os_error() == Some(ERROR_ACCESS_DENIED) {
            // 该名字已有首个实例：另一个实例正在运行（或上一次运行未清理）→ 失败关闭（§7）。
            EndpointError::AlreadyInUse
        } else {
            EndpointError::Os { source }
        }
    })
}
