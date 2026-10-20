//! Node Link 的传输层：最小 [`Transport`] 接口 + 真实 WSS 实现。
//!
//! 设计（`docs/MODULE_ARCHITECTURE.md` §4.6）：协议与 session backend **不感知** socket、TLS 或
//! Tailscale；它们只依赖本模块的 trait。WSS 是默认实现，测试可以换成脚本化的内存实现。
//!
//! ## 连接所有权与关闭顺序
//!
//! [`WssConnection`] 被 [`crate::link`] 拆成读写两半后，写半边由 link 任务独占、读半边也由同一任务
//! 轮询：**没有第二条后台任务**。关闭顺序固定为「停止接受新请求 → 写 close 帧 → 丢弃两半 → join
//! link 任务」；`node-link-client` 不留下 detached task。
//!
//! ## TLS 信任
//!
//! [`TlsTrust::Roots`] 是生产路径：调用方必须显式注入信任锚（证书 DER）。仓库里**没有**「使用系统
//! 根存储」的依赖，也没有任何「跳过校验」的开关；[`TlsTrust::PlaintextDev`] 只供显式注入的
//! 开发/测试传输使用（明文仅限 loopback）。`app` 的 Owner 链路不使用它：Owner 端点只允许
//! `wss://`，明文 scheme 在构造链路前即被拒绝（`docs/CONFIG_REFERENCE.md` §3.1）。
//!
//! ## Host / 子协议
//!
//! 请求携带 `Host: <authority>` 与 Node Link 的 WebSocket 子协议；服务端的 upgrade 白名单按同一
//! 子协议判定（`NODE_LINK_PROTOCOL.md` §2.1）。端点路径按用途分开：业务连接是
//! `/node-link/v1`，独立附件连接是 `/node-link/v1/attachments`（§12.9）。

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt as _, StreamExt as _};
use rustls_pki_types::CertificateDer;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::{Message, WebSocketConfig};

use crate::error::ClientError;

/// 业务连接的 WSS 端点路径（`NODE_LINK_PROTOCOL.md` §2.1）。
pub const BUSINESS_PATH: &str = "/node-link/v1";

/// 独立附件连接的 WSS 端点路径（§12.9）。
pub const ATTACHMENTS_PATH: &str = "/node-link/v1/attachments";

/// WebSocket 子协议（§2.1）。服务端 `server::node_link::conn::WS_SUBPROTOCOL` 是同一字符串；协议
/// crate 不导出它，因此这里保留唯一一份客户端常量并在文档里指回 §2.1。
pub const WS_SUBPROTOCOL: &str = "acp-remote.nodelink.v1.json";

/// 单条消息的默认上限（§2.5；服务端可经 `node.ready.limits` 再下调）。
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// 连接建立的超时（握手本身另有 15 秒合同窗口；这里只覆盖 TCP/TLS/upgrade）。
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// TLS 信任方式。
#[derive(Clone)]
pub enum TlsTrust {
    /// 只信任这些证书（DER）签发的对端证书。
    Roots(Vec<CertificateDer<'static>>),
    /// 明文 `ws://`，**只在** `dev_mode.allow_plaintext` 下允许（loopback）。
    PlaintextDev,
}

impl std::fmt::Debug for TlsTrust {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Roots(roots) => formatter
                .debug_struct("TlsTrust::Roots")
                .field("count", &roots.len())
                .finish(),
            Self::PlaintextDev => formatter.write_str("TlsTrust::PlaintextDev"),
        }
    }
}

/// 端点与用途。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionTarget {
    /// `wss://<authority>`（不含路径）。明文开发模式下是 `ws://<authority>`。
    origin: String,
    /// authority（`host[:port]`），同时是 HTTP `Host` 头与 TLS SNI 的来源。
    authority: String,
    /// `host`（IPv6 去掉方括号）或 IP 文本。
    host: String,
    /// 端口（缺省 443 / 明文 80）。
    port: u16,
    /// 端点路径。
    path: &'static str,
}

impl ConnectionTarget {
    /// 解析业务连接端点（`wss://host[:port]/node-link/v1`；显式开发模式下 `ws://` 同形）。
    pub fn business(endpoint: &str) -> Result<Self, ClientError> {
        Self::parse(endpoint, BUSINESS_PATH)
    }

    /// 解析独立附件连接端点（由业务端点的 origin 派生，路径固定 `/node-link/v1/attachments`）。
    pub fn attachments(endpoint: &str) -> Result<Self, ClientError> {
        Self::parse(endpoint, ATTACHMENTS_PATH)
    }

    fn parse(endpoint: &str, path: &'static str) -> Result<Self, ClientError> {
        let (scheme, rest) = endpoint
            .split_once("://")
            .ok_or_else(|| ClientError::Protocol("endpoint is not a URL".to_owned()))?;
        if scheme != "wss" && scheme != "ws" {
            return Err(ClientError::Protocol(
                "endpoint must use wss (or ws in the explicit plaintext dev mode)".to_owned(),
            ));
        }
        let authority = rest
            .split('/')
            .next()
            .ok_or_else(|| ClientError::Protocol("endpoint has no authority".to_owned()))?;
        if authority.is_empty() {
            return Err(ClientError::Protocol(
                "endpoint has no authority".to_owned(),
            ));
        }
        let (host, port) = split_authority(authority, scheme == "wss")?;
        // 业务端点必须逐字带合同路径；附件端点由 origin 派生，因此只校验 origin 部分。
        // `ws://` 只被 `TlsTrust::PlaintextDev` 接受（`transport.connect` 再校验 scheme），
        // 是否允许明文由调用方的信任注入决定，不由 URL 形状决定。
        if path == BUSINESS_PATH {
            let expected = format!("{scheme}://{authority}{BUSINESS_PATH}");
            if endpoint != expected {
                return Err(ClientError::Protocol(
                    "endpoint must be <scheme>://<authority>/node-link/v1".to_owned(),
                ));
            }
        }
        Ok(Self {
            origin: format!("{scheme}://{authority}"),
            authority: authority.to_owned(),
            host,
            port,
            path,
        })
    }

    /// 本次连接的完整 URI（tungstenite 的请求目标；TLS 由本模块另行建立）。
    fn uri(&self) -> String {
        format!("{}{}", self.origin, self.path)
    }

    /// authority（诊断与 `Host` 头）。
    pub fn authority(&self) -> &str {
        &self.authority
    }

    /// 端点路径。
    pub fn path(&self) -> &str {
        self.path
    }
}

/// 由信任锚构造客户端 TLS 配置；`Roots` 为空即失败关闭（不退回「不校验」）。
///
/// 配对 claim/status（[`crate::pairing`]）与 WSS 连接共用这一份构造，因此两条出站路径的
/// 信任锚、协议版本与 provider 不可能分叉。
pub(crate) fn tls_client_config(
    trust: &TlsTrust,
) -> Result<Arc<rustls::ClientConfig>, ClientError> {
    let TlsTrust::Roots(roots) = trust else {
        return Err(ClientError::Protocol(
            "plaintext dev mode has no TLS configuration".to_owned(),
        ));
    };
    if roots.is_empty() {
        return Err(ClientError::Protocol(
            "no TLS trust anchor was configured for the owner node".to_owned(),
        ));
    }
    let mut store = rustls::RootCertStore::empty();
    for root in roots {
        store
            .add(root.clone())
            .map_err(|_| ClientError::Protocol("a TLS trust anchor is invalid".to_owned()))?;
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| ClientError::Protocol("TLS protocol versions are unavailable".to_owned()))?
        .with_root_certificates(store)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// 拆分 `host[:port]`，支持 `[v6]:port`。
fn split_authority(authority: &str, secure: bool) -> Result<(String, u16), ClientError> {
    let default_port = if secure { 443 } else { 80 };
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, tail) = rest.split_once(']').ok_or_else(|| {
            ClientError::Protocol("endpoint has an unterminated IPv6 host".to_owned())
        })?;
        let port = match tail {
            "" => default_port,
            tail => tail
                .strip_prefix(':')
                .and_then(|port| port.parse::<u16>().ok())
                .ok_or_else(|| ClientError::Protocol("endpoint port is invalid".to_owned()))?,
        };
        return Ok((host.to_owned(), port));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => Ok((
            host.to_owned(),
            port.parse::<u16>()
                .map_err(|_| ClientError::Protocol("endpoint port is invalid".to_owned()))?,
        )),
        _ => Ok((authority.to_owned(), default_port)),
    }
}

/// 连接建立后的读写两半。
#[async_trait]
pub trait MessageSink: Send {
    /// 发送一条文本消息。
    async fn send(&mut self, text: String) -> Result<(), ClientError>;

    /// 发送 close 帧（幂等：已关闭时成功返回）。
    async fn close(&mut self) -> Result<(), ClientError>;
}

/// 入站消息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    /// 文本消息。
    Text(String),
    /// 对端关闭（携带可选 close code）。
    Closed(Option<u16>),
}

#[async_trait]
pub trait MessageSource: Send {
    /// 读取下一条消息；流结束时返回 [`Incoming::Closed`]。
    async fn recv(&mut self) -> Result<Incoming, ClientError>;
}

/// 一条已建立的连接（可拆成读写两半）。
pub trait Connection: Send {
    /// 拆分。拆开后读半边只被轮询、写半边只被写入：两者互不共享 `&mut`。
    fn split(self: Box<Self>) -> (Box<dyn MessageSink>, Box<dyn MessageSource>);
}

/// 传输层工厂。
#[async_trait]
pub trait Transport: Send + Sync {
    /// 建立连接（包含 TLS/WebSocket upgrade，但不做 Node Link 握手）。
    async fn connect(&self, target: &ConnectionTarget) -> Result<Box<dyn Connection>, ClientError>;
}

/// 按每个 Owner 的 `ClientConfig` 构造传输。
///
/// 信任方式与单消息上限是**每条 Owner 链路**的事实（`ClientConfig.trust`/
/// `ClientConfig.max_message_bytes`），不能由进程级共享的一个传输假装满足；因此
/// [`crate::NodeLinkClient`] 只持有工厂，在 `open` 时按配置构造。
pub trait TransportFactory: Send + Sync {
    /// 由信任方式与单消息上限构造一个传输（可被重连与附件连接复用）。
    fn build(
        &self,
        trust: &TlsTrust,
        max_message_bytes: usize,
    ) -> Result<Arc<dyn Transport>, ClientError>;
}

/// 生产实现：真实 WSS（或显式开发模式下的 WS）传输。
#[derive(Debug, Default, Clone, Copy)]
pub struct WssTransportFactory;

impl TransportFactory for WssTransportFactory {
    fn build(
        &self,
        trust: &TlsTrust,
        max_message_bytes: usize,
    ) -> Result<Arc<dyn Transport>, ClientError> {
        // 失败关闭：`Roots(empty)` 不得被延迟到第一次重连才发现（与 `tls_client_config` 同一口径）。
        if let TlsTrust::Roots(roots) = trust {
            if roots.is_empty() {
                return Err(ClientError::Protocol(
                    "no TLS trust anchor was configured for the owner node".to_owned(),
                ));
            }
        }
        Ok(Arc::new(WssTransport::new(
            trust.clone(),
            max_message_bytes,
        )))
    }
}

/// WSS 传输实现。
pub struct WssTransport {
    trust: Arc<TlsTrust>,
    max_message_bytes: usize,
}

impl WssTransport {
    /// 用信任锚与单消息上限构造。`max_message_bytes` 来自配置的
    /// `node_link.max_message_bytes`（只会被服务端继续下调）。
    pub fn new(trust: TlsTrust, max_message_bytes: usize) -> Self {
        Self {
            trust: Arc::new(trust),
            max_message_bytes,
        }
    }

    /// 构造客户端 TLS 配置。`Roots` 为空时失败关闭（不退回「不校验」）。
    fn client_config(&self) -> Result<Arc<rustls::ClientConfig>, ClientError> {
        tls_client_config(self.trust.as_ref())
    }

    fn ws_config(&self) -> WebSocketConfig {
        // `WebSocketConfig` 是 `#[non_exhaustive]`：只能从默认值增量改写，不能整体字面量构造。
        let mut config = WebSocketConfig::default();
        config.max_message_size = Some(self.max_message_bytes);
        config.max_frame_size = Some(self.max_message_bytes);
        config
    }
}

#[async_trait]
impl Transport for WssTransport {
    async fn connect(&self, target: &ConnectionTarget) -> Result<Box<dyn Connection>, ClientError> {
        let request = target.uri().into_client_request().map_err(|error| {
            ClientError::Protocol(format!("endpoint request is invalid: {error}"))
        })?;
        let mut request = request;
        request.headers_mut().insert(
            "sec-websocket-protocol",
            HeaderValue::from_static(WS_SUBPROTOCOL),
        );

        let address = (target.host.as_str(), target.port);
        let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
            .await
            .map_err(|_| ClientError::Timeout)?
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        tcp.set_nodelay(true)
            .map_err(|error| ClientError::Transport(error.to_string()))?;

        match self.trust.as_ref() {
            TlsTrust::Roots(_) => {
                if !matches!(target.origin.split_once("://"), Some(("wss", _))) {
                    return Err(ClientError::Protocol(
                        "a TLS trust anchor requires a wss:// endpoint".to_owned(),
                    ));
                }
                let config = self.client_config()?;
                let connector = tokio_rustls::TlsConnector::from(config);
                let server_name = rustls_pki_types::ServerName::try_from(target.host.clone())
                    .map_err(|_| {
                        ClientError::Protocol("endpoint host is not a valid TLS name".to_owned())
                    })?;
                let tls =
                    tokio::time::timeout(CONNECT_TIMEOUT, connector.connect(server_name, tcp))
                        .await
                        .map_err(|_| ClientError::Timeout)?
                        .map_err(|error| ClientError::Transport(error.to_string()))?;
                let (socket, _) = tokio::time::timeout(
                    CONNECT_TIMEOUT,
                    tokio_tungstenite::client_async_with_config(
                        request,
                        tls,
                        Some(self.ws_config()),
                    ),
                )
                .await
                .map_err(|_| ClientError::Timeout)?
                .map_err(|error| ClientError::Transport(error.to_string()))?;
                Ok(Box::new(WssConnection::new(socket)))
            }
            TlsTrust::PlaintextDev => {
                if !matches!(target.origin.split_once("://"), Some(("ws", _))) {
                    return Err(ClientError::Protocol(
                        "plaintext dev mode requires a ws:// endpoint".to_owned(),
                    ));
                }
                let (socket, _) = tokio::time::timeout(
                    CONNECT_TIMEOUT,
                    tokio_tungstenite::client_async_with_config(
                        request,
                        tcp,
                        Some(self.ws_config()),
                    ),
                )
                .await
                .map_err(|_| ClientError::Timeout)?
                .map_err(|error| ClientError::Transport(error.to_string()))?;
                Ok(Box::new(WssConnection::new(socket)))
            }
        }
    }
}

/// 底层流（明文或 TLS）。
trait Socket: AsyncRead + AsyncWrite + Unpin + Send {}
impl<S: AsyncRead + AsyncWrite + Unpin + Send> Socket for S {}

struct WssConnection<S: Socket> {
    socket: WebSocketStream<S>,
}

impl<S: Socket> WssConnection<S> {
    fn new(socket: WebSocketStream<S>) -> Self {
        Self { socket }
    }
}

impl<S: Socket + 'static> Connection for WssConnection<S> {
    fn split(self: Box<Self>) -> (Box<dyn MessageSink>, Box<dyn MessageSource>) {
        let (sink, stream) = self.socket.split();
        (Box::new(WssSink { sink }), Box::new(WssSource { stream }))
    }
}

struct WssSink<S> {
    sink: futures_util::stream::SplitSink<WebSocketStream<S>, Message>,
}

#[async_trait]
impl<S: Socket + 'static> MessageSink for WssSink<S> {
    async fn send(&mut self, text: String) -> Result<(), ClientError> {
        self.sink
            .send(Message::Text(text.into()))
            .await
            .map_err(|error| ClientError::Transport(error.to_string()))
    }

    async fn close(&mut self) -> Result<(), ClientError> {
        let _ = self.sink.send(Message::Close(None)).await;
        let _ = self.sink.flush().await;
        Ok(())
    }
}

struct WssSource<S> {
    stream: futures_util::stream::SplitStream<WebSocketStream<S>>,
}

#[async_trait]
impl<S: Socket + 'static> MessageSource for WssSource<S> {
    async fn recv(&mut self) -> Result<Incoming, ClientError> {
        loop {
            match self.stream.next().await {
                Some(Ok(Message::Text(text))) => return Ok(Incoming::Text(text.to_string())),
                Some(Ok(Message::Binary(_))) => {
                    // Node Link v1 只使用文本 JSON；二进制帧是协议违规，不静默忽略。
                    return Err(ClientError::Protocol(
                        "binary frames are not part of the node-link v1 wire".to_owned(),
                    ));
                }
                Some(Ok(Message::Close(frame))) => {
                    return Ok(Incoming::Closed(frame.map(|frame| u16::from(frame.code))));
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => continue,
                Some(Err(error)) => return Err(ClientError::Transport(error.to_string())),
                None => return Ok(Incoming::Closed(None)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_parsing_requires_the_contract_path_and_derives_port_and_host() {
        let target = ConnectionTarget::business("wss://work-pc.example.ts.net/node-link/v1")
            .expect("contract endpoint");
        assert_eq!(target.authority(), "work-pc.example.ts.net");
        assert_eq!(target.port, 443);
        assert_eq!(target.path(), BUSINESS_PATH);

        let with_port =
            ConnectionTarget::business("wss://100.101.102.103:8765/node-link/v1").expect("port");
        assert_eq!(with_port.port, 8765);

        let v6 = ConnectionTarget::business("wss://[fd7a::1]:8765/node-link/v1").expect("ipv6");
        assert_eq!(v6.host, "fd7a::1");
        assert_eq!(v6.port, 8765);

        assert!(ConnectionTarget::business("wss://host/node-link/v1/").is_err());
        assert!(ConnectionTarget::business("wss://host/node-link/v2").is_err());
        assert!(ConnectionTarget::business("https://host/node-link/v1").is_err());
    }

    #[test]
    fn plaintext_dev_endpoints_use_the_same_business_shape() {
        // `dev_mode` + loopback 的门控在信任注入处（`app::trust::OutboundTrust::for_scheme`）；
        // 这里只钉住 URL 形状不会把合法的 `ws://` 业务端点判成配置错误。
        let target = ConnectionTarget::business("ws://127.0.0.1:8765/node-link/v1")
            .expect("plaintext dev endpoint");
        assert_eq!(target.uri(), "ws://127.0.0.1:8765/node-link/v1");
        assert!(ConnectionTarget::business("ws://127.0.0.1:8765/node-link/v1/").is_err());
        assert!(ConnectionTarget::business("ws://127.0.0.1:8765/node-link/v2").is_err());
    }

    #[test]
    fn attachment_endpoint_keeps_the_same_origin() {
        let target = ConnectionTarget::attachments("wss://host:8765/node-link/v1")
            .expect("attachments endpoint");
        assert_eq!(target.uri(), "wss://host:8765/node-link/v1/attachments");
    }

    #[test]
    fn empty_tls_roots_fail_closed() {
        let transport = WssTransport::new(TlsTrust::Roots(Vec::new()), DEFAULT_MAX_MESSAGE_BYTES);
        assert!(transport.client_config().is_err());
    }
}
