//! 接入面的配置形状：`daemon.listen`/`daemon.public_origin`/`daemon.allowed_hosts`/
//! `daemon.trusted_proxies`/`daemon.tls.*`/`dev_mode.allow_plaintext` 在传输层的表示
//! （`docs/CONFIG_REFERENCE.md` §1/§10）。
//!
//! 本模块只承载数据与内置默认值；校验与失败关闭发生在 [`crate::transport::net::NetListener::bind`]
//! （key 名与默认值的权威是 `CONFIG_REFERENCE.md`，这里不复制一份配置文档）。

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::transport::net::{
    DEFAULT_DRAIN_GRACE_MS, DEFAULT_LISTEN, DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_MESSAGE_BYTES,
};

/// `daemon.tls` 的两种模式（`CONFIG_REFERENCE.md` §1）。
#[derive(Clone)]
pub enum TlsMode {
    /// TLS 由同机可信反向代理终止；Daemon 在监听地址上提供明文 HTTP/WSS。
    Proxy,
    /// Daemon 自己终止 TLS；两个 PEM 路径必需，加载失败即拒绝启动。
    Direct {
        /// PEM 证书链路径（`daemon.tls.cert_path`）。
        cert_path: PathBuf,
        /// PEM 私钥路径（`daemon.tls.key_path`）；内容不得进入日志或错误消息。
        key_path: PathBuf,
    },
}

/// 手写 `Debug`：`direct` 模式的两个路径不打印（`SECURITY_DESIGN.md` §14.1 的日志白名单禁止完整敏感路径，
/// 私钥内容更是不得进入 `Debug`/日志/崩溃报告），因此任何 `{:?}` 都不会泄漏密钥材料或路径。
impl fmt::Debug for TlsMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Proxy => f.write_str("TlsMode::Proxy"),
            Self::Direct { .. } => {
                f.write_str("TlsMode::Direct { cert_path: <redacted>, key_path: <redacted> }")
            }
        }
    }
}

/// 共享 listener 的配置（`design.md` D2/D9/D10）。
#[derive(Debug, Clone)]
pub struct NetConfig {
    /// `daemon.listen`：`IP:端口` 字面量（默认 [`DEFAULT_LISTEN`]）。
    ///
    /// 只接受 `std::net::SocketAddr` 可解析的字面量：不做主机名解析，避免「一个名字解析出多个地址」
    /// 让「绑定一个 listener」的语义变得不确定。
    pub listen: String,
    /// `daemon.public_origin`（`scheme://host[:port]`）；`None` = 未配置。
    pub public_origin: Option<String>,
    /// `daemon.allowed_hosts`：反向代理场景下的 Host 白名单；空表示按 `public_origin` 推导。
    pub allowed_hosts: Vec<String>,
    /// `daemon.trusted_proxies`：允许采信 `Forwarded`/`X-Forwarded-*` 的对端地址。
    pub trusted_proxies: Vec<String>,
    /// `daemon.tls.mode` 与其在 `direct` 模式下的 PEM 路径。
    pub tls: TlsMode,
    /// `dev_mode.allow_plaintext`：明文开发模式**只允许 loopback**（`SECURITY_DESIGN.md` §7.3）。
    pub allow_plaintext_dev: bool,
    /// 单条 WebSocket message 上限（默认 [`DEFAULT_MAX_MESSAGE_BYTES`]，`NODE_LINK_PROTOCOL.md` §2.5）。
    pub max_message_bytes: usize,
    /// 配对 HTTP 请求体上限（默认 [`DEFAULT_MAX_BODY_BYTES`]，§13.4 的 413）。
    pub max_body_bytes: usize,
    /// 关闭时排空在途连接的上限（对应 `daemon.shutdown_grace_ms`）。
    pub drain_grace: Duration,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            listen: DEFAULT_LISTEN.to_owned(),
            public_origin: None,
            allowed_hosts: Vec::new(),
            trusted_proxies: Vec::new(),
            tls: TlsMode::Proxy,
            allow_plaintext_dev: false,
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            drain_grace: Duration::from_millis(DEFAULT_DRAIN_GRACE_MS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_config_reference() {
        let config = NetConfig::default();
        // `CONFIG_REFERENCE.md` §1 的默认值：loopback:8765、proxy 模式、无 public_origin/allowed_hosts。
        assert_eq!(config.listen, "127.0.0.1:8765");
        assert_eq!(config.public_origin, None);
        assert!(config.allowed_hosts.is_empty());
        assert!(config.trusted_proxies.is_empty());
        assert!(matches!(config.tls, TlsMode::Proxy));
        assert!(!config.allow_plaintext_dev);
        // `NODE_LINK_PROTOCOL.md` §2.5 的单条消息上限。
        assert_eq!(config.max_message_bytes, 1024 * 1024);
        assert_eq!(config.drain_grace.as_millis(), 10_000);
    }

    #[test]
    fn debug_never_prints_tls_paths() {
        let mode = TlsMode::Direct {
            cert_path: PathBuf::from("C:/secret/owner-cert.pem"),
            key_path: PathBuf::from("C:/secret/owner-key.pem"),
        };
        let printed = format!("{mode:?}");
        assert!(!printed.contains("owner-cert.pem"));
        assert!(!printed.contains("owner-key.pem"));
        assert!(printed.contains("<redacted>"));
    }
}
