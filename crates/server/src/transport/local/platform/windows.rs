//! `cfg(windows)`：Named Pipe endpoint 与对端 SID 校验（`docs/LOCAL_ADMIN_PROTOCOL.md` §2.1/§2.2）。
//!
//! - 首个 pipe 实例经 `windows_local_ipc::create_pipe_server` 创建：它带「仅当前用户 SID」的 SDDL 与
//!   `FILE_FLAG_FIRST_PIPE_INSTANCE`（同名第二个首实例会失败，因此「同一 endpoint 起两次」是明确错误）。
//! - 后续实例用 tokio 的 safe `ServerOptions`：同一 pipe 名的实例参数必须一致，tokio 的默认值
//!   （`PIPE_UNLIMITED_INSTANCES`、65536/65536 缓冲、字节模式、拒绝远端客户端）与 wrapper 的
//!   `CreateNamedPipeW` 参数刻意同值（`vendor/windows-local-ipc/src/windows.rs` 的常量注释、WP2 用例 ⑪）。
//! - 连接建立后必须校验对端凭据（§2.2）：`client_user_sid` 与本进程 SID 逐字比较；拿不到对端 SID
//!   （未连接、对端已退出、无权查询）一律按「凭据不一致」处理，绝不放行。
//! - **无论本次连接是否被接受，endpoint 都必须继续可用**：每次 `accept` 都要先把同名的下一个 pipe
//!   实例补上；`connect` 失败让当前实例不可再用时也一样，否则 endpoint 此后会永久只回
//!   「没有可用的 pipe 实例」（正确性约束，也是 `settle_accept` 单元用例的断言点）。

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
    ///
    /// # 本 future **不是 cancel-safe**
    ///
    /// `accept` 的第一步是 `take_pending()`：它把当前待连接的 pipe 实例从 `self` **移出**。因此一旦这个
    /// future 在 `connect()` 完成之前被丢掉（`select!` 另一个分支胜出、`timeout` 到期、任务被 abort），
    /// 那个实例会连同**已经连上它但尚未被处理**的客户端一起被关闭：客户端侧 `open()` 已经成功，
    /// 却会立刻看到 EOF，而服务端收不到任何待处理连接（WP4a 实测可复现）。
    ///
    /// 调用方 MUST 在**专用任务**里串行调用本方法，并把结果投递给其余逻辑（`crates/app/src/daemon.rs`
    /// 的接受循环就是如此：自有的接受任务 + 容量 1 的 channel + 循环只 `recv`）；不得把 `accept()` 直接
    /// 放进 `select!`/`timeout` 里。取消安全需要幂等接缝（把实例放回 `pending`），本模块暂不提供。
    pub async fn accept(&mut self) -> Result<LocalStream, EndpointError> {
        let server = self.take_pending()?;
        let connected = server.connect().await;
        // connect 失败时该实例已不可再用，因此收尾步骤先补建下一个实例再决定返回（见 `settle_accept`）：
        // 否则 `pending` 会一直为空，endpoint 此后每次 `accept` 都只能回「没有可用的 pipe 实例」。
        let connected = self.settle_accept(connected, server)?;

        let own_sid = windows_local_ipc::current_user_sid();
        let peer_sid = windows_local_ipc::client_user_sid(&connected);
        match (own_sid, peer_sid) {
            (Ok(own), Ok(peer)) if own == peer => Ok(connected),
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

    /// 取出一个待连接的实例；没有待连接实例时现建一个。
    ///
    /// 第二个分支是「重试」语义：上一次 `accept` 在补建下一个实例时失败（系统调用错误）会留下空的
    /// `pending`，这里再试一次，而不是让 endpoint 永久只回「没有可用的 pipe 实例」。
    fn take_pending(&mut self) -> Result<LocalStream, EndpointError> {
        match self.pending.take() {
            Some(server) => Ok(server),
            None => create_pipe_instance(&self.pipe_name),
        }
    }

    /// `accept` 的收尾一步：**先把下一个实例补上**，再决定本次返回。
    ///
    /// 模块头注释里的不变量在这里落地：无论本次连接是否被接受，endpoint 都必须继续可用。补建发生在
    /// 凭据判定之前，因此对端被拒绝（`PeerRejected`）时也成立；`connect` 失败时同样成立——该实例虽已
    /// 不可再用，`pending` 已经被下一个实例填上。
    ///
    /// 补建自身失败时不静默：`pending` 保持为空并上报该错误，下一次 `accept` 的 `take_pending` 会再试；
    /// 两个失败同时发生时上报 `connect` 的成因，因为它解释的是本次连接。
    fn settle_accept(
        &mut self,
        connected: io::Result<()>,
        server: LocalStream,
    ) -> Result<LocalStream, EndpointError> {
        let refilled = self.refill_pending();
        match (connected, refilled) {
            (Ok(()), Ok(())) => Ok(server),
            (Ok(()), Err(error)) => Err(error),
            (Err(source), _) => Err(EndpointError::Os { source }),
        }
    }

    /// 补建同名的下一个实例（参数必须与首个实例一致，见模块头注释）。
    fn refill_pending(&mut self) -> Result<(), EndpointError> {
        self.pending = Some(create_pipe_instance(&self.pipe_name)?);
        Ok(())
    }
}

/// 创建该 pipe 名的后续实例（tokio 的 safe `ServerOptions`，参数与首实例同值）。
fn create_pipe_instance(pipe_name: &str) -> Result<LocalStream, EndpointError> {
    ServerOptions::new()
        .create(pipe_name)
        .map_err(|source| EndpointError::Os { source })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::transport::local::LoggingAuditHook;

    fn config() -> LocalEndpointConfig {
        LocalEndpointConfig {
            data_dir: std::env::temp_dir(),
            instance_id: InstanceId::generate(),
            runtime_dir: None,
        }
    }

    /// `connect` 失败后 endpoint 必须仍然可用：`pending` 被补建，下一次真实连接能被接受。
    ///
    /// 真实 Named Pipe 上无法稳定注入 `connect` 失败：`ConnectNamedPipe` 对「客户端已连接」与
    /// 「客户端连上后立刻断开」都返回成功（mio 把 `ERROR_PIPE_CONNECTED`/`ERROR_NO_DATA` 都当成功），
    /// 只有真正的系统调用错误才 `Err`。因此这里从可单测的接缝 `settle_accept` 注入失败——它是
    /// `accept` 里唯一以 `io::Result` 形式接收 connect 结果的一步。把它改回「connect 失败就直接 `?`
    /// 返回」会让本用例的 `pending` 断言与随后的真实连接都失败。
    #[tokio::test]
    async fn a_failed_connect_leaves_the_endpoint_usable() {
        let mut endpoint = LocalEndpoint::bind(config(), Arc::new(LoggingAuditHook))
            .await
            .expect("本机创建 Named Pipe endpoint 应成功");
        let pipe_name = endpoint.describe();
        let server = endpoint.take_pending().expect("首实例在 pending 里");

        let failed = endpoint.settle_accept(
            Err(io::Error::other("模拟 ConnectNamedPipe 立即失败")),
            server,
        );
        assert!(
            matches!(failed, Err(EndpointError::Os { .. })),
            "connect 失败必须作为错误上报"
        );
        assert!(
            endpoint.pending.is_some(),
            "connect 失败后必须已补建下一个 pipe 实例"
        );

        let _client = tokio::net::windows::named_pipe::ClientOptions::new()
            .open(&pipe_name)
            .expect("补建后的实例必须可连接");
        assert!(
            endpoint.accept().await.is_ok(),
            "connect 失败之后 endpoint 必须仍能接受连接"
        );
    }

    /// `accept` 不是 cancel-safe（模块内的硬约束，调用方必须自己保证）：被取消的 `accept` 会把
    /// `take_pending` 移出的实例一起丢掉，**已经连上它的客户端**那个连接也就随之消失。
    ///
    /// 断言尽量定在机制层：手动把 future 轮询一次（此时还没客户端，`connect()` 挂起）→ 客户端连上这个
    /// 已从 `pending` 移出的实例（`open()` 成功）→ 丢掉 future（等价于 `select!`/`timeout` 取消）→
    /// 客户端读到的不是服务端回复，而是连接结束（EOF 或错误）。把 `accept` 改回「先 `connect()` 再
    /// `take_pending()`」或引入幂等接缝（把实例放回 `pending`）都会让本用例失败。
    #[tokio::test]
    async fn a_cancelled_accept_drops_the_pending_instance_and_kills_the_connected_client() {
        use tokio::io::AsyncReadExt as _;

        let mut endpoint = LocalEndpoint::bind(config(), Arc::new(LoggingAuditHook))
            .await
            .expect("本机创建 Named Pipe endpoint 应成功");
        let pipe_name = endpoint.describe();
        assert!(endpoint.pending.is_some(), "bind 之后应有一个待连接实例");

        // 轮询一次：`take_pending` 已把实例移出 `self`，`connect()` 停在等待客户端。
        let mut accept = Box::pin(endpoint.accept());
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(
            std::future::Future::poll(accept.as_mut(), &mut context).is_pending(),
            "没有客户端时 accept 必须挂起"
        );

        let mut client = tokio::net::windows::named_pipe::ClientOptions::new()
            .open(&pipe_name)
            .expect("客户端连上的是已被移出的那个实例");
        // 让本进程的 reactor 处理「客户端已连接」的完成包（`accept` 此后不再被轮询，就像被取消了一样）。
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 取消：`select!` 的另一分支胜出 / `timeout` 到期时发生的就是这件事。
        drop(accept);
        assert!(
            endpoint.pending.is_none(),
            "取消之后实例没有被放回：它已被丢掉（正是非 cancel-safe 的成因）"
        );

        let mut buf = [0u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(5), client.read(&mut buf))
            .await
            .expect("取消之后客户端必须立刻看到连接结束，而不是无限等待");
        assert!(
            matches!(read, Ok(0) | Err(_)),
            "孤儿客户端必须读到 EOF/错误（实际 {read:?}）"
        );

        // endpoint 本身仍可用（模块头注释的不变量）：下一次 accept 现建实例并正常接收。
        let connect = tokio::spawn(async move {
            tokio::net::windows::named_pipe::ClientOptions::new().open(&pipe_name)
        });
        let accepted = tokio::time::timeout(Duration::from_secs(5), endpoint.accept())
            .await
            .expect("accept 不应挂死");
        assert!(
            accepted.is_ok(),
            "取消之后 endpoint 必须仍能接受连接（{accepted:?}）"
        );
        let _ = connect.await.expect("等待连接任务");
    }
}
