//! 共享 listener 的绑定、失败关闭、告警与关闭排空（`design.md` D2/D10/D11，`specs/node-link-listener` §1）。
//!
//! [`NetListener::bind`] 的固定顺序：**校验配置 → 加载 TLS（`direct` 时失败即拒）→ 绑定 TCP → 记录告警**。
//! 任何一步失败都返回错误且不留下半初始化的监听（绑定失败即拒绝启动，不降级为无网络接入运行）。
//!
//! 非 loopback 监听是显式配置的后果，因此只告警不拒绝（spec「非 loopback 监听告警」）；`proxy` 模式下的
//! 非 loopback 监听额外告警一次：那是明文对外暴露的形态，必须由同机可信反代终止 TLS（D10）。
//!
//! 异步任务的所有权：HTTP 侧由 `axum::serve` 持有（它对在途请求做宽限排空），WebSocket 侧由本模块的
//! supervisor 任务持有——`axum` 的升级回调是 `tokio::spawn` 出来的，若不接管就会变成无人持有的任务。
//! 会话 future 通过有界 channel 交给 supervisor 放进 `JoinSet`，关闭时停止接收、等宽限、再强制取消。

use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;

use axum::serve::Listener;
use axum::serve::ListenerExt as _;
use rustls::ServerConfig;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::io::ReadBuf;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;

use crate::transport::net::HostPolicy;
use crate::transport::net::NetConfig;
use crate::transport::net::ProxyPolicy;
use crate::transport::net::Shutdown;
use crate::transport::net::TlsFile;
use crate::transport::net::route::NetState;
use crate::transport::net::route::RouteError;
use crate::transport::net::route::RouteTable;
use crate::transport::net::route::SessionJob;
use crate::transport::net::tls::load_server_config;
use crate::transport::net::ws::WsHandler;

/// WS 会话交接队列的容量：升级回调按此对「会话过多」施加背压，超出即等待 supervisor 取走。
const SESSION_QUEUE_CAPACITY: usize = 64;

/// TLS 握手的接入层超时：只连接不握手的对端不得无限占用 accept 循环。
///
/// 这是接入层实现细节（不是协议限额，协议限额见 `NODE_LINK_PROTOCOL.md` §2.5），因此不出现在配置键里。
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// listener 绑定/服务失败。所有变体都要求调用方**拒绝启动或拒绝继续**，不得降级。
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    /// `daemon.listen` 不是合法的 `IP:端口` 字面量。
    #[error("`daemon.listen` 不是合法的 IP:端口 字面量：`{value}`")]
    InvalidListen {
        /// 配置值。
        value: String,
        /// 解析错误。
        #[source]
        source: std::net::AddrParseError,
    },
    /// `dev_mode.allow_plaintext` 与监听地址冲突（明文开发模式只允许 loopback）。
    #[error("`dev_mode.allow_plaintext` 只对 loopback 生效，不能与监听地址 {addr} 组合")]
    PlaintextDevNonLoopback {
        /// 监听地址。
        addr: SocketAddr,
    },
    /// `daemon.public_origin` 非法。
    #[error("`daemon.public_origin` 必须是 `http(s)://host[:port]`：`{value}`")]
    InvalidPublicOrigin {
        /// 配置值。
        value: String,
    },
    /// `daemon.allowed_hosts` 中有一项不是合法 host。
    #[error("`daemon.allowed_hosts` 含非法项：`{value}`")]
    InvalidAllowedHost {
        /// 配置值。
        value: String,
    },
    /// `daemon.trusted_proxies` 中有一项不是 IP 或 `IP:端口`。
    #[error("`daemon.trusted_proxies` 含非法项：`{value}`")]
    InvalidTrustedProxy {
        /// 配置值。
        value: String,
    },
    /// 上限被配成 0（会让端点不可用，属配置错误）。
    #[error("上限 `{name}` 不能为 0")]
    InvalidLimit {
        /// 配置键名。
        name: &'static str,
    },
    /// `direct` 模式的证书/私钥文件不存在或不可读。
    #[error("TLS {role}文件不可读：{path}")]
    TlsFileUnreadable {
        /// 文件角色。
        role: TlsFile,
        /// 文件路径。
        path: PathBuf,
        /// 底层错误。
        #[source]
        source: io::Error,
    },
    /// PEM 解析失败（消息不含文件内容）。
    #[error("TLS {role}文件不是可用的 PEM")]
    TlsPemInvalid {
        /// 文件角色。
        role: TlsFile,
    },
    /// 文件权限明显宽松（`SECURITY_DESIGN.md` §13.2）：失败关闭。
    #[error("TLS {role}文件权限过于宽松（要求仅当前用户可访问）：{path}")]
    TlsInsecurePermissions {
        /// 文件角色。
        role: TlsFile,
        /// 文件路径。
        path: PathBuf,
    },
    /// 证书与私钥不匹配（消息不含密钥材料）。
    #[error("TLS 证书与私钥不匹配")]
    TlsKeyCertificateMismatch,
    /// rustls 配置构建失败。
    #[error("TLS 配置构建失败")]
    TlsConfigBuild,
    /// TCP 绑定失败（端口占用、地址非法、权限不足）。
    #[error("监听 {addr} 失败：{source}")]
    Bind {
        /// 监听地址。
        addr: SocketAddr,
        /// 底层错误。
        #[source]
        source: io::Error,
    },
    /// `serve` 期间接入层自身失败。
    #[error("接入层运行失败：{source}")]
    Serve {
        /// 底层错误。
        #[source]
        source: io::Error,
    },
}

/// 启动期告警：由组合根（`app`）打进启动输出，`daemon.start` 的返回里也已包含这些事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerWarning {
    /// 监听地址不是 loopback：必须是显式配置。
    NonLoopbackListen {
        /// 实际监听地址。
        addr: SocketAddr,
    },
    /// `proxy` 模式下监听非 loopback：明文面暴露在本机之外的风险（D10）。
    PlaintextBeyondLoopback {
        /// 实际监听地址。
        addr: SocketAddr,
    },
    /// 平台无法核验证书/私钥文件权限（不失败关闭，但也**不是**已核验）。
    PermissionsUnverifiable {
        /// 文件角色。
        role: TlsFile,
        /// 文件路径。
        path: PathBuf,
    },
}

impl fmt::Display for ListenerWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonLoopbackListen { addr } => write!(
                f,
                "监听非 loopback 地址 {addr}：仅在你显式配置并自行保证网络边界时使用"
            ),
            Self::PlaintextBeyondLoopback { addr } => write!(
                f,
                "proxy 模式下监听非 loopback 地址 {addr}：接入面是明文，必须由同机可信反代终止 TLS"
            ),
            Self::PermissionsUnverifiable { role, path } => write!(
                f,
                "本平台无法核验 TLS {}文件权限（{}）：未核验，而不是已通过",
                role.describe(),
                path.display()
            ),
        }
    }
}

/// 共享 HTTP/WSS listener（`daemon.listen`）。
pub struct NetListener {
    config: NetConfig,
    local_addr: SocketAddr,
    host_policy: HostPolicy,
    proxy_policy: ProxyPolicy,
    tls: Option<Arc<ServerConfig>>,
    tcp: TcpListener,
    routes: RouteTable,
    warnings: Vec<ListenerWarning>,
}

/// 手写 `Debug`：只展示运维需要的事实（监听地址、是否终止 TLS、告警），不展开注册表，也不打印证书路径
/// （`NetConfig` 的 `Debug` 已把两个 TLS 路径隐去，这里不再把它拉进来）。
impl fmt::Debug for NetListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NetListener")
            .field("local_addr", &self.local_addr)
            .field("tls_terminated", &self.tls.is_some())
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

impl NetListener {
    /// 校验配置、加载 TLS、绑定 `daemon.listen`。
    pub async fn bind(config: NetConfig) -> Result<Self, NetError> {
        let addr: SocketAddr =
            config
                .listen
                .trim()
                .parse()
                .map_err(|source| NetError::InvalidListen {
                    value: config.listen.clone(),
                    source,
                })?;
        // `SECURITY_DESIGN.md` §7.3/§10：明文开发模式只允许 loopback；非 loopback 时失败关闭，
        // 而不是「按 loopback 语义静默生效」。
        if config.allow_plaintext_dev && !addr.ip().is_loopback() {
            return Err(NetError::PlaintextDevNonLoopback { addr });
        }
        for (name, value) in [
            ("max_message_bytes", config.max_message_bytes),
            ("max_body_bytes", config.max_body_bytes),
        ] {
            if value == 0 {
                return Err(NetError::InvalidLimit { name });
            }
        }
        let host_policy = HostPolicy::new(config.public_origin.as_deref(), &config.allowed_hosts)?;
        let proxy_policy = ProxyPolicy::new(&config.trusted_proxies)?;
        let mut warnings = Vec::new();
        let tls = load_server_config(&config.tls, &mut warnings)?;
        // TLS 配置就绪后才绑定：TLS 失败时端口不会被短暂占用。
        let tcp = TcpListener::bind(addr)
            .await
            .map_err(|source| NetError::Bind { addr, source })?;
        let local_addr = tcp
            .local_addr()
            .map_err(|source| NetError::Bind { addr, source })?;
        if !local_addr.ip().is_loopback() {
            warnings.push(ListenerWarning::NonLoopbackListen { addr: local_addr });
            tracing::warn!(
                event = "net.listen_non_loopback",
                address = %local_addr,
                "监听非 loopback 地址"
            );
            if tls.is_none() {
                warnings.push(ListenerWarning::PlaintextBeyondLoopback { addr: local_addr });
                tracing::warn!(
                    event = "net.plaintext_beyond_loopback",
                    address = %local_addr,
                    "proxy 模式监听非 loopback 地址：明文面必须由同机可信反代终止 TLS"
                );
            }
        }
        tracing::info!(
            event = "net.listener_bound",
            address = %local_addr,
            tls = tls.is_some(),
            "接入层已绑定"
        );
        Ok(Self {
            config,
            local_addr,
            host_policy,
            proxy_policy,
            tls,
            tcp,
            routes: RouteTable::default(),
            warnings,
        })
    }

    /// 实际绑定的监听地址清单（`daemon.status.listen` 的来源）。
    pub fn local_addrs(&self) -> Vec<SocketAddr> {
        vec![self.local_addr]
    }

    /// 启动期告警（非 loopback 监听、proxy 明文面、权限不可核验）。
    pub fn warnings(&self) -> &[ListenerWarning] {
        &self.warnings
    }

    /// 注册一个 WebSocket 端点：只有声明 `required_subprotocol` 的升级请求才成功。
    ///
    /// `server::node_link` 传入协议冻结的 path 与 subprotocol（`NODE_LINK_PROTOCOL.md` §2.1）；
    /// 本模块不写死它们，也不解释它们的语义。
    pub fn register_ws(
        &mut self,
        path: &str,
        required_subprotocol: &str,
        handler: Arc<dyn WsHandler>,
    ) -> Result<(), RouteError> {
        self.routes.insert_ws(path, required_subprotocol, handler)
    }

    /// 注册一个只接受 `POST` 的 HTTP 端点（配对 claim/status）。
    pub fn register_post(
        &mut self,
        path: &str,
        handler: Arc<dyn crate::transport::net::route::HttpHandler>,
    ) -> Result<(), RouteError> {
        self.routes.insert_post(path, handler)
    }

    /// 服务接入面直到收到关闭信号，并按 [`NetConfig::drain_grace`] 排空在途连接。
    ///
    /// 返回即表示接入层已停止：TCP listener 已关闭、在途 HTTP 请求与会话都已结束或已被强制取消。
    pub async fn serve(mut self, shutdown: Shutdown) -> Result<(), NetError> {
        let (sessions_tx, sessions_rx) = mpsc::channel::<SessionJob>(SESSION_QUEUE_CAPACITY);
        let mut supervisor = tokio::spawn(supervise_sessions(sessions_rx));
        let state = Arc::new(NetState {
            host_policy: self.host_policy,
            proxy_policy: self.proxy_policy,
            max_message_bytes: self.config.max_message_bytes,
            max_body_bytes: self.config.max_body_bytes,
            tls_terminated: self.tls.is_some(),
            shutdown: shutdown.clone(),
            sessions: sessions_tx.clone(),
        });
        let router = std::mem::take(&mut self.routes).into_router(Arc::clone(&state));
        let app = router.into_make_service_with_connect_info::<SocketAddr>();
        let acceptor = NetAcceptor {
            tcp: self.tcp,
            tls: self.tls.map(TlsAcceptor::from),
        };
        let serve_signal = shutdown.clone();
        let outer_signal = shutdown.clone();
        let mut server = tokio::spawn(async move {
            axum::serve(acceptor.tap_io(|_connection: &mut NetStream| {}), app)
                .with_graceful_shutdown(serve_signal.wait())
                .await
        });
        // 一个共享的宽限截止时刻：HTTP 排空与会话排空共用 `daemon.shutdown_grace_ms` 的预算。
        let deadline = tokio::time::Instant::now() + self.config.drain_grace;
        let served = tokio::select! {
            joined = &mut server => join_result(joined),
            _ = outer_signal.wait() => match tokio::time::timeout_at(deadline, &mut server).await {
                Ok(joined) => join_result(joined),
                Err(_) => {
                    // 宽限内没排空：强制取消在途 HTTP 连接（连接随之关闭，不遗留任务）。
                    server.abort();
                    let _ = server.await;
                    tracing::warn!(
                        event = "net.http_drain_timeout",
                        grace_ms = self.config.drain_grace.as_millis(),
                        "宽限内未排空在途 HTTP 连接：已强制关闭"
                    );
                    Ok(())
                }
            },
        };
        // 关闭会话交接队列（state 与 serve 手里的发送端），再排空在途 WebSocket 会话。
        drop(state);
        drop(sessions_tx);
        match tokio::time::timeout_at(deadline, &mut supervisor).await {
            Ok(_) => {}
            Err(_) => {
                supervisor.abort();
                let _ = supervisor.await;
                tracing::warn!(
                    event = "net.session_drain_timeout",
                    grace_ms = self.config.drain_grace.as_millis(),
                    "宽限内未排空在途 WebSocket 会话：已强制关闭"
                );
            }
        }
        served
    }
}

/// `axum::serve` 的 `JoinHandle` 结果 → 本模块的错误类型。
fn join_result(joined: Result<io::Result<()>, tokio::task::JoinError>) -> Result<(), NetError> {
    match joined {
        Ok(Ok(())) => Ok(()),
        Ok(Err(source)) => Err(NetError::Serve { source }),
        // 只在强制取消路径出现；不 panic 的 task 不会走到这里。
        Err(error) => Err(NetError::Serve {
            source: io::Error::other(error.to_string()),
        }),
    }
}

/// WebSocket 会话的所有者：把接入层的升级回调交来的会话放进自己持有的 `JoinSet`。
///
/// 队列关闭后停止接收新会话，并等待在途会话结束（上限由 [`NetListener::serve`] 的宽限定时器给出）；
/// supervisor 被取消时 `JoinSet` 随之丢弃，其持有的全部会话任务一并中止，因此不存在 detached task。
async fn supervise_sessions(mut sessions: mpsc::Receiver<SessionJob>) {
    let mut running: JoinSet<()> = JoinSet::new();
    loop {
        tokio::select! {
            job = sessions.recv() => match job {
                Some(job) => {
                    running.spawn(job);
                }
                None => break,
            },
            // 及时收割已完成会话，避免 `JoinSet` 无界增长。
            Some(_) = running.join_next(), if !running.is_empty() => {}
        }
    }
    while running.join_next().await.is_some() {}
}

/// 已接受的接入连接（明文或本进程终止的 TLS）。
enum NetStream {
    /// `proxy` 模式的明文连接。
    Plaintext(Box<TcpStream>),
    /// `direct` 模式已完成 TLS 握手的连接。
    Tls(Box<TlsStream<TcpStream>>),
}

impl AsyncRead for NetStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plaintext(stream) => std::pin::Pin::new(stream.as_mut()).poll_read(cx, buffer),
            Self::Tls(stream) => std::pin::Pin::new(stream.as_mut()).poll_read(cx, buffer),
        }
    }
}

impl AsyncWrite for NetStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Plaintext(stream) => std::pin::Pin::new(stream.as_mut()).poll_write(cx, buffer),
            Self::Tls(stream) => std::pin::Pin::new(stream.as_mut()).poll_write(cx, buffer),
        }
    }

    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plaintext(stream) => std::pin::Pin::new(stream.as_mut()).poll_flush(cx),
            Self::Tls(stream) => std::pin::Pin::new(stream.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plaintext(stream) => std::pin::Pin::new(stream.as_mut()).poll_shutdown(cx),
            Self::Tls(stream) => std::pin::Pin::new(stream.as_mut()).poll_shutdown(cx),
        }
    }
}

/// `axum::serve` 的监听器：明文模式下直接交出 TCP 流，`direct` 模式下在 accept 循环内完成 TLS 握手
/// （握手超时/失败只放弃本次连接，不终止 listener）。
struct NetAcceptor {
    tcp: TcpListener,
    tls: Option<TlsAcceptor>,
}

impl Listener for NetAcceptor {
    type Io = NetStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.tcp.accept().await {
                Ok((stream, addr)) => {
                    let Some(acceptor) = self.tls.clone() else {
                        return (NetStream::Plaintext(Box::new(stream)), addr);
                    };
                    match tokio::time::timeout(TLS_HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await
                    {
                        Ok(Ok(stream)) => return (NetStream::Tls(Box::new(stream)), addr),
                        Ok(Err(error)) => tracing::debug!(
                            event = "net.tls_handshake_failed",
                            peer = %addr,
                            error = %error,
                            "TLS 握手失败：放弃本次连接"
                        ),
                        Err(_) => tracing::debug!(
                            event = "net.tls_handshake_timeout",
                            peer = %addr,
                            "TLS 握手超时：放弃本次连接"
                        ),
                    }
                }
                Err(error) => handle_accept_error(&error).await,
            }
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.tcp.local_addr()
    }
}

/// accept 失败的处置（与 `axum::serve` 对 `TcpListener` 的处置一致）：连接级错误直接重试，
/// 其余（例如文件描述符耗尽）记一次错误并退避 1 秒后重试，而不是终止接入层。
async fn handle_accept_error(error: &io::Error) {
    if matches!(
        error.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
    ) {
        return;
    }
    tracing::error!(event = "net.accept_error", error = %error, "accept 失败：退避后重试");
    tokio::time::sleep(Duration::from_secs(1)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_listen_value_fails_closed() {
        let error = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(NetListener::bind(NetConfig {
                listen: "not-an-address".to_owned(),
                ..NetConfig::default()
            }))
            .expect_err("非法监听地址必须失败关闭");
        assert!(matches!(error, NetError::InvalidListen { .. }));
        assert!(error.to_string().contains("not-an-address"));
    }

    #[test]
    fn plaintext_dev_flag_rejects_non_loopback_listen() {
        let error = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(NetListener::bind(NetConfig {
                listen: "0.0.0.0:0".to_owned(),
                allow_plaintext_dev: true,
                ..NetConfig::default()
            }))
            .expect_err("明文开发模式不得用于非 loopback");
        assert!(matches!(error, NetError::PlaintextDevNonLoopback { .. }));
    }

    #[test]
    fn zero_limits_fail_closed() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        for config in [
            NetConfig {
                max_message_bytes: 0,
                ..NetConfig::default()
            },
            NetConfig {
                max_body_bytes: 0,
                ..NetConfig::default()
            },
        ] {
            let error = runtime
                .block_on(NetListener::bind(config))
                .expect_err("0 上限必须失败关闭");
            assert!(matches!(error, NetError::InvalidLimit { .. }));
        }
    }

    #[test]
    fn warning_messages_state_the_risk_without_secrets() {
        let warning = ListenerWarning::PlaintextBeyondLoopback {
            addr: "0.0.0.0:8765".parse().expect("合法地址"),
        };
        assert!(warning.to_string().contains("明文"));
        let warning = ListenerWarning::PermissionsUnverifiable {
            role: TlsFile::PrivateKey,
            path: PathBuf::from("key.pem"),
        };
        assert!(warning.to_string().contains("未核验"));
    }
}
