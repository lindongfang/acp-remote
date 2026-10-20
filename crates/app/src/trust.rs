//! Access 侧出站 TLS 的信任来源（`SECURITY_DESIGN.md` §9.5、`NODE_LINK_PROTOCOL.md` §13）。
//!
//! 信任模型（用户已裁定的决策）：
//!
//! - **系统根 + 可选自定义 CA**：默认只信任操作系统根存储；`node_link.access.trust_roots` 给出的 PEM
//!   文件是**追加**的固定 CA（自签 Owner、Tailscale 自建 CA 等场景），不会替换系统根；
//! - **失败关闭**：系统根与自定义 CA 都为空、任一路径不可读、任一 PEM 不含证书，都让启动失败——绝不以
//!   「空锚」或「跳过校验」继续（`node-link-client` 的 `TlsTrust::Roots(empty)` 同样失败关闭）；
//! - **Owner 端点一律 WSS**：持久化的 Owner 信任记录只接受 `wss://`（`NodeRecord`），因此出站
//!   Owner 链路也只按 `wss://` 取根集合；`http://`/`ws://` 在这里**显式拒绝**，不会因为
//!   `dev_mode.allow_plaintext` 而把业务链路降级成明文。该开关仍服务入站监听与配对 URL 的明文
//!   开发形态，不属于本模块的出站信任选择；
//! - **不记录敏感面**：错误与日志只带 CA 文件的**数量**与分类，不打印路径内容；证书校验失败时连接
//!   直接失败（由 `node-link-client` 的 rustls 栈判定），不回退、不重试明文。
//!
//! 本模块只做「把配置变成信任锚集合」这一件事：不发起连接、不解析 URL、不缓存。

use std::path::{Path, PathBuf};

use node_link_client::TlsTrust;
use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject as _;

use crate::config::Config;

/// 出站信任锚集合。
#[derive(Debug, Clone)]
pub struct OutboundTrust {
    roots: Vec<CertificateDer<'static>>,
    allow_plaintext: bool,
}

impl OutboundTrust {
    /// 按配置加载：系统根 + `node_link.access.trust_roots` 的 PEM 证书。
    ///
    /// 返回的集合至少有一个锚；否则是 [`TrustError::NoRoots`]。
    pub fn load(config: &Config) -> Result<Self, TrustError> {
        let native = rustls_native_certs::load_native_certs();
        // 标准库/平台的部分失败不阻止启动（某些平台会为个别证书返回错误），但必须可见；真正的失败是
        // 「一个锚都没有」——那由下面的 `roots.is_empty()` 失败关闭。
        if !native.errors.is_empty() {
            tracing::warn!(
                event = "daemon.trust_native_certs_partial",
                errors = native.errors.len(),
                loaded = native.certs.len(),
                "平台根存储有个别条目无法读取：已加载的根仍然生效"
            );
        }
        let mut roots = native.certs;
        let system = roots.len();
        for path in &config.node_link_access.trust_roots {
            let parsed = load_pem_certificates(path)?;
            tracing::info!(
                event = "daemon.trust_custom_ca_loaded",
                file = ?path.file_name(),
                certificates = parsed.len(),
                "已追加一个自定义 CA 文件（路径只记文件名）"
            );
            roots.extend(parsed);
        }
        if roots.is_empty() {
            return Err(TrustError::NoRoots);
        }
        tracing::info!(
            event = "daemon.trust_loaded",
            system_roots = system,
            custom_roots = roots.len() - system,
            "出站 TLS 信任锚已就绪（系统根 + 可选自定义 CA）"
        );
        Ok(Self {
            roots,
            allow_plaintext: config.dev_mode.enabled && config.dev_mode.allow_plaintext,
        })
    }

    /// 只为这些根构造 `TlsTrust`；集合为空时失败关闭（不返回 `Roots(vec![])`）。
    pub fn roots_trust(&self) -> Result<TlsTrust, TrustError> {
        if self.roots.is_empty() {
            return Err(TrustError::NoRoots);
        }
        Ok(TlsTrust::Roots(self.roots.clone()))
    }

    /// 按目标 scheme 选择信任方式。
    ///
    /// - `https`/`wss`：显式根集合；
    /// - `http`/`ws`：显式拒绝（持久化的 Owner 端点只允许 `wss://`；`dev_mode.allow_plaintext`
    ///   不放宽出站 Owner 链路）；
    /// - 其它 scheme：`TrustError::UnsupportedScheme`。
    pub fn for_scheme(&self, scheme: &str, _host: &str) -> Result<TlsTrust, TrustError> {
        match scheme {
            "https" | "wss" => self.roots_trust(),
            "http" | "ws" => Err(TrustError::PlaintextNotSupported),
            _ => Err(TrustError::UnsupportedScheme),
        }
    }

    /// 已加载的根数量（诊断用）。
    pub fn root_count(&self) -> usize {
        self.roots.len()
    }

    /// 是否允许明文（`dev_mode.enabled && dev_mode.allow_plaintext`）。
    pub fn allows_plaintext(&self) -> bool {
        self.allow_plaintext
    }
}

/// 信任加载失败（全部失败关闭）。
#[derive(Debug, thiserror::Error)]
pub enum TrustError {
    /// 系统根与自定义 CA 都为空：没有可用锚。
    #[error(
        "no TLS trust anchor is available (the system store is empty and no custom CA is configured)"
    )]
    NoRoots,
    /// 自定义 CA 文件不可读。
    #[error("a configured trust root file is missing or unreadable")]
    Unreadable {
        path: PathBuf,
        kind: std::io::ErrorKind,
    },
    /// 文件存在但不是 PEM 证书集合。
    #[error("a configured trust root file is not a usable PEM certificate")]
    Pem { path: PathBuf },
    /// 出站 Owner 端点使用了明文 scheme。
    #[error(
        "an owner endpoint must use wss://; plaintext dev mode does not apply to node-link access links"
    )]
    PlaintextNotSupported,
    /// 不支持的 scheme。
    #[error("the node-link target must use https/wss")]
    UnsupportedScheme,
}

impl TrustError {
    /// CLI/启动日志用的简短说明：不含路径（`SECURITY_DESIGN.md` §14.1）。
    pub fn message(&self) -> String {
        match self {
            Self::NoRoots => "no TLS trust anchor is available for node-link access".to_owned(),
            Self::Unreadable { .. } => {
                "a configured node_link.access.trust_roots file is unreadable".to_owned()
            }
            Self::Pem { .. } => {
                "a configured node_link.access.trust_roots file is not a PEM certificate".to_owned()
            }
            Self::PlaintextNotSupported => {
                "an owner endpoint must use wss; plaintext dev mode does not apply to node-link access links"
                    .to_owned()
            }
            Self::UnsupportedScheme => "the node-link target scheme is not supported".to_owned(),
        }
    }
}

/// 从 PEM 文件读取**全部**证书；空文件、无证书、语法的任一错误都失败关闭。
fn load_pem_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>, TrustError> {
    let iterator = CertificateDer::pem_file_iter(path).map_err(|error| match error {
        rustls_pki_types::pem::Error::Io(source) => TrustError::Unreadable {
            path: path.to_path_buf(),
            kind: source.kind(),
        },
        _ => TrustError::Pem {
            path: path.to_path_buf(),
        },
    })?;
    let mut certificates = Vec::new();
    for item in iterator {
        let certificate = item.map_err(|_| TrustError::Pem {
            path: path.to_path_buf(),
        })?;
        certificates.push(certificate);
    }
    if certificates.is_empty() {
        return Err(TrustError::Pem {
            path: path.to_path_buf(),
        });
    }
    Ok(certificates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn config_with(roots: &[&Path], dev: bool, plaintext: bool) -> Config {
        let list = roots
            .iter()
            .map(|path| format!("\"{}\"", path.display().to_string().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!(
            "dev_mode.enabled = {dev}\ndev_mode.allow_plaintext = {plaintext}\n[node_link.access]\ntrust_roots = [{list}]\n"
        );
        Config::from_toml(&text).expect("配置可解析")
    }

    /// 一个合法的自签证书 PEM（`rcgen` 现算，不提交私钥材料）。
    fn write_certificate(path: &Path) {
        let certificate =
            rcgen::generate_simple_self_signed(vec!["owner.test".to_owned()]).expect("自签证书");
        std::fs::write(path, certificate.cert.pem()).expect("写入 PEM");
    }

    fn temp_dir(label: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("acpr-trust-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("创建临时目录");
        path
    }

    #[test]
    fn custom_ca_is_appended_to_the_system_roots() {
        let root = temp_dir("custom");
        let path = root.join("ca.pem");
        write_certificate(&path);
        let config = config_with(&[&path], false, false);
        let trust = OutboundTrust::load(&config).expect("加载信任锚");
        assert!(trust.root_count() >= 1);
        let TlsTrust::Roots(roots) = trust.roots_trust().expect("roots") else {
            panic!("必须返回显式根集合");
        };
        assert!(!roots.is_empty(), "不得返回空锚集合");
    }

    #[test]
    fn a_missing_or_non_pem_file_fails_closed() {
        let root = temp_dir("bad");
        let missing = root.join("missing.pem");
        let error = OutboundTrust::load(&config_with(&[&missing], false, false))
            .expect_err("缺失文件必须失败");
        assert!(matches!(error, TrustError::Unreadable { .. }), "{error}");

        let empty = root.join("empty.pem");
        std::fs::write(&empty, b"not a certificate").expect("写入");
        let error = OutboundTrust::load(&config_with(&[&empty], false, false))
            .expect_err("非 PEM 必须失败");
        assert!(matches!(error, TrustError::Pem { .. }), "{error}");
    }

    #[test]
    fn owner_endpoints_are_wss_only_even_in_dev_mode() {
        let trust = OutboundTrust {
            roots: vec![CertificateDer::from(vec![0u8; 1])],
            allow_plaintext: false,
        };
        assert!(matches!(
            trust.for_scheme("wss", "owner.test"),
            Ok(TlsTrust::Roots(_))
        ));
        assert!(matches!(
            trust.for_scheme("ws", "127.0.0.1"),
            Err(TrustError::PlaintextNotSupported)
        ));
        // `dev_mode.allow_plaintext` 服务入站与配对 URL 的开发形态，不放宽出站 Owner 链路：
        // 持久化的 Owner 端点只允许 `wss://`（`NodeRecord`），这里必须显式拒绝而不是返回 `PlaintextDev`。
        let dev = OutboundTrust {
            roots: vec![CertificateDer::from(vec![0u8; 1])],
            allow_plaintext: true,
        };
        assert!(matches!(
            dev.for_scheme("ws", "127.0.0.1"),
            Err(TrustError::PlaintextNotSupported)
        ));
        assert!(matches!(
            dev.for_scheme("http", "127.0.0.1"),
            Err(TrustError::PlaintextNotSupported)
        ));
        assert!(matches!(
            dev.for_scheme("ftp", "owner.test"),
            Err(TrustError::UnsupportedScheme)
        ));
    }

    #[test]
    fn an_empty_roots_trust_is_refused() {
        let trust = OutboundTrust {
            roots: Vec::new(),
            allow_plaintext: true,
        };
        assert!(matches!(trust.roots_trust(), Err(TrustError::NoRoots)));
    }
}
