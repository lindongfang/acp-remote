//! 模块内用例用的极简 HTTP/1.1 与 WebSocket 客户端，以及自签证书与临时目录工具。
//!
//! 这些用例走**真实 loopback listener**（`127.0.0.1:0`，`plan.md` 的 Runtime Resources），因此绑定、路由、
//! Host 边界、升级规则、上限、TLS 与关闭排空都在真实连接上被验证。不引入新依赖：HTTP 与 WebSocket 的
//! 最小帧编解码都在这里手写（`tokio-tungstenite` 不是本 crate 的直接依赖，不能使用）。
//!
//! 每个 I/O 都有超时：用例失败时给出明确错误，而不是挂住 `cargo test`。

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rustls_pki_types::CertificateDer;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt as _;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt as _;
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio_rustls::TlsConnector;

use crate::transport::net::NetConfig;
use crate::transport::net::NetError;
use crate::transport::net::NetListener;
use crate::transport::net::Shutdown;
use crate::transport::net::ShutdownHandle;

/// 单次 I/O 的超时：用例失败要快速失败，不挂住测试进程。
const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// 测试用的接入层流（明文或 TLS）。
pub trait TestStream: AsyncRead + AsyncWrite + Unpin {}
impl<S: AsyncRead + AsyncWrite + Unpin> TestStream for S {}

/// 一条测试连接（带未消费字节缓冲）。
pub struct TestConnection<S: TestStream> {
    stream: S,
    pending: Vec<u8>,
}

/// 解析出的 HTTP 响应。
#[derive(Debug, Clone)]
pub struct TestResponse {
    /// 状态码。
    pub status: u16,
    /// 响应头（原样保留小写比较）。
    pub headers: Vec<(String, String)>,
    /// 响应体（按 `Content-Length` 读取）。
    pub body: Vec<u8>,
}

impl TestResponse {
    /// 按名字取响应头（大小写不敏感）。
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// 响应体的 UTF-8 文本。
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// 一条解析出的 WebSocket 帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsFrame {
    /// opcode（`0x1` text、`0x2` binary、`0x8` close）。
    pub opcode: u8,
    /// payload。
    pub payload: Vec<u8>,
}

impl WsFrame {
    /// close 帧携带的 code。
    pub fn close_code(&self) -> Option<u16> {
        if self.opcode != 0x8 || self.payload.len() < 2 {
            return None;
        }
        Some(u16::from_be_bytes([self.payload[0], self.payload[1]]))
    }

    /// text 帧的文本。
    pub fn text(&self) -> Option<String> {
        if self.opcode != 0x1 {
            return None;
        }
        String::from_utf8(self.payload.clone()).ok()
    }
}

impl<S: TestStream> TestConnection<S> {
    /// 包装一条已建立的流。
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            pending: Vec::new(),
        }
    }

    /// 写入原始字节。
    pub async fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        tokio::time::timeout(IO_TIMEOUT, self.stream.write_all(bytes))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "写超时"))?
    }

    /// 读到 header 结束（`\r\n\r\n` 之后的内容留在缓冲里）并返回 header 块。
    async fn read_head(&mut self) -> io::Result<Vec<u8>> {
        loop {
            if let Some(index) = find_subsequence(&self.pending, b"\r\n\r\n") {
                let head = self.pending[..index].to_vec();
                self.pending.drain(..index + 4);
                return Ok(head);
            }
            self.fill().await?;
        }
    }

    /// 解析一个完整 HTTP 响应（按 `Content-Length` 收 body；无该头时视为无体）。
    pub async fn read_response(&mut self) -> io::Result<TestResponse> {
        let head = self.read_head().await?;
        let text = String::from_utf8_lossy(&head).into_owned();
        let mut lines = text.split("\r\n");
        let status_line = lines.next().unwrap_or_default();
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "缺少状态行"))?;
        let mut headers = Vec::new();
        for line in lines {
            if let Some((name, value)) = line.split_once(':') {
                headers.push((name.trim().to_owned(), value.trim().to_owned()));
            }
        }
        let length = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.parse::<usize>().ok())
            .unwrap_or(0);
        while self.pending.len() < length {
            self.fill().await?;
        }
        let body = self.pending.drain(..length).collect();
        Ok(TestResponse {
            status,
            headers,
            body,
        })
    }

    /// 发送一个 text 帧（客户端必须加掩码）。
    pub async fn send_text(&mut self, text: &str) -> io::Result<()> {
        self.send_frame(0x1, text.as_bytes()).await
    }

    /// 发送一个 binary 帧。
    pub async fn send_binary(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.send_frame(0x2, bytes).await
    }

    /// 发送一个带掩码的帧。
    pub async fn send_frame(&mut self, opcode: u8, payload: &[u8]) -> io::Result<()> {
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x80 | opcode);
        let length = payload.len();
        if length < 126 {
            frame.push(0x80 | u8::try_from(length).expect("小于 126"));
        } else if let Ok(short) = u16::try_from(length) {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&short.to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(
                &u64::try_from(length)
                    .expect("usize 可放进 u64")
                    .to_be_bytes(),
            );
        }
        let mask = [0x11u8, 0x22, 0x33, 0x44];
        frame.extend_from_slice(&mask);
        let mut masked = Vec::with_capacity(payload.len());
        for (index, byte) in payload.iter().enumerate() {
            masked.push(byte ^ mask[index % 4]);
        }
        frame.extend_from_slice(&masked);
        self.write_all(&frame).await
    }

    /// 读取一个服务端帧（服务端不掩码）。
    pub async fn read_frame(&mut self) -> io::Result<WsFrame> {
        self.fill_to(2).await?;
        let first = self.pending[0];
        let second = self.pending[1];
        let opcode = first & 0x0f;
        let masked = second & 0x80 != 0;
        let mut cursor = 2usize;
        let length = match second & 0x7f {
            126 => {
                self.fill_to(4).await?;
                cursor = 4;
                usize::from(u16::from_be_bytes([self.pending[2], self.pending[3]]))
            }
            127 => {
                self.fill_to(10).await?;
                cursor = 10;
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&self.pending[2..10]);
                usize::try_from(u64::from_be_bytes(bytes))
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "帧过长"))?
            }
            other => usize::from(other),
        };
        if masked {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "服务端帧不得加掩码",
            ));
        }
        self.fill_to(cursor + length).await?;
        let payload = self.pending[cursor..cursor + length].to_vec();
        self.pending.drain(..cursor + length);
        Ok(WsFrame { opcode, payload })
    }

    /// 读到至少 `needed` 字节。
    async fn fill_to(&mut self, needed: usize) -> io::Result<()> {
        while self.pending.len() < needed {
            self.fill().await?;
        }
        Ok(())
    }

    /// 读一块数据进缓冲。
    async fn fill(&mut self) -> io::Result<()> {
        let mut chunk = [0u8; 16 * 1024];
        let read = tokio::time::timeout(IO_TIMEOUT, self.stream.read(&mut chunk))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "读超时"))??;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "对端在响应完成前关闭连接",
            ));
        }
        self.pending.extend_from_slice(&chunk[..read]);
        Ok(())
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// 构造一个 HTTP/1.1 请求（`extra` 为额外请求头）；请求体按 `Content-Length` 附在头之后。
pub fn http_request(
    method: &str,
    path: &str,
    host: &str,
    extra: &[(&str, &str)],
    body: &[u8],
) -> Vec<u8> {
    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n").into_bytes();
    for (name, value) in extra {
        request.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    request.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    request.extend_from_slice(b"Connection: close\r\n\r\n");
    request.extend_from_slice(body);
    request
}

/// 构造一个 WebSocket 升级请求（`subprotocols` 与 `extensions` 为空表示不带对应头）。
pub fn ws_request(
    path: &str,
    host: &str,
    subprotocols: Option<&str>,
    extensions: Option<&str>,
) -> String {
    let mut headers = Vec::new();
    if let Some(subprotocols) = subprotocols {
        headers.push(("Sec-WebSocket-Protocol", subprotocols));
    }
    if let Some(extensions) = extensions {
        headers.push(("Sec-WebSocket-Extensions", extensions));
    }
    ws_request_with_headers(path, host, &headers)
}

/// 构造一个 WebSocket 升级请求，并按 `headers` 顺序（可重复同名）追加原始请求头。
///
/// 重复同名头用于构造「同一 token 拆成两个头」这类边界形态（用例里的 `http`/`hyper` 侧不会自动合并）。
pub fn ws_request_with_headers(path: &str, host: &str, headers: &[(&str, &str)]) -> String {
    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n"
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request
}

/// 明文连接。
pub async fn connect(addr: SocketAddr) -> io::Result<TestConnection<TcpStream>> {
    let stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "连接超时"))??;
    Ok(TestConnection::new(stream))
}

/// 客户端 TLS 配置：只信任用例自签的那张证书（因此不需要危险的「跳过校验」路径）。
pub fn client_tls_config(certificate: CertificateDer<'static>) -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certificate).expect("加入自签证书");
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("默认协议版本可用")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
}

/// TLS 连接（证书的自签 SAN 是 `localhost`，因此用该名字做 SNI/校验）。
pub async fn connect_tls(
    addr: SocketAddr,
    config: Arc<rustls::ClientConfig>,
) -> io::Result<TestConnection<tokio_rustls::client::TlsStream<TcpStream>>> {
    let stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "连接超时"))??;
    let connector = TlsConnector::from(config);
    let server_name = rustls_pki_types::ServerName::try_from("localhost")
        .expect("合法 SNI")
        .to_owned();
    let stream = tokio::time::timeout(IO_TIMEOUT, connector.connect(server_name, stream))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "TLS 握手超时"))??;
    Ok(TestConnection::new(stream))
}

/// 正在运行的测试接入层。
pub struct RunningServer {
    /// 实际监听地址。
    pub addr: SocketAddr,
    /// 启动期告警。
    pub warnings: Vec<crate::transport::net::ListenerWarning>,
    handle: ShutdownHandle,
    join: JoinHandle<Result<(), NetError>>,
}

impl RunningServer {
    /// 触发关闭并等待接入层停止（同时验证关闭序列能正常收尾）。
    pub async fn stop(self) -> Result<(), NetError> {
        self.handle.trigger();
        tokio::time::timeout(IO_TIMEOUT, self.join)
            .await
            .expect("关闭序列必须在上限内结束")
            .expect("接入层任务不得 panic")
    }
}

/// 绑定 + 注册 + 启动接入层。
pub async fn start_server(
    config: NetConfig,
    register: impl FnOnce(&mut NetListener),
) -> RunningServer {
    let mut listener = NetListener::bind(config).await.expect("绑定接入层");
    let warnings = listener.warnings().to_vec();
    register(&mut listener);
    let addr = listener.local_addrs()[0];
    let (handle, shutdown) = Shutdown::channel();
    let join = tokio::spawn(async move { listener.serve(shutdown).await });
    RunningServer {
        addr,
        warnings,
        handle,
        join,
    }
}

/// 用例自建的自签证书与私钥（结束即删除临时目录；仓库不提交任何私钥材料）。
pub struct TestCertificate {
    /// PEM 证书链路径。
    pub cert_path: PathBuf,
    /// PEM 私钥路径。
    pub key_path: PathBuf,
    /// 证书 DER（供客户端信任）。
    pub der: CertificateDer<'static>,
    directory: PathBuf,
}

impl TestCertificate {
    /// 现算一张 ECDSA P-256 自签证书并写成权限合规的 PEM 文件。
    pub fn generate() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "acpr-net-cert-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).expect("创建临时目录");
        let certified =
            rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("自签证书");
        let cert_path = directory.join("cert.pem");
        let key_path = directory.join("key.pem");
        std::fs::write(&cert_path, certified.cert.pem()).expect("写证书");
        std::fs::write(&key_path, certified.signing_key.serialize_pem()).expect("写私钥");
        set_owner_only(&cert_path);
        set_owner_only(&key_path);
        Self {
            cert_path,
            key_path,
            der: certified.cert.der().clone(),
            directory,
        }
    }
}

impl Drop for TestCertificate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// 把文件收成「只有当前用户可访问」（`SECURITY_DESIGN.md` §13.2 的目标形态）。
#[cfg(unix)]
pub fn set_owner_only(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).expect("chmod 0600");
}

/// Windows：文件权限由平台 ACL 承担，本函数不做任何事（见 `permissions` 模块文档）。
#[cfg(not(unix))]
pub fn set_owner_only(_path: &std::path::Path) {}
