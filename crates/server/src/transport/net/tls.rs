//! TLS `direct` 模式的启动期一次性加载（`design.md` D10、`docs/SECURITY_DESIGN.md` §7.1/§13.2）。
//!
//! 顺序：先按 §13.2 核验两个文件的平台权限（宽松即失败关闭），再读 PEM、构建 rustls 配置。
//! 任何一步失败都返回错误，由 [`crate::transport::net::NetListener::bind`] 转成「拒绝启动」；不存在
//! 「降级为明文监听」的路径。私钥字节只在构建 rustls 配置时存在于内存，不进入日志、错误消息或 `Debug`
//! （[`crate::transport::net::TlsMode`] 的 `Debug` 也把两个路径隐去）。
//!
//! provider 固定为 rustls 自带的 `ring`（`docs/MODULE_ARCHITECTURE.md` §3.1 的选型结论）；ALPN 只允许
//! `http/1.1`，避免 TLS 层选出本进程未编译的 `h2`。证书变更经重启生效，不改变 Node identity
//! （`SECURITY_DESIGN.md` §9.5）。

use std::sync::Arc;

use rustls::ServerConfig;
use rustls_pki_types::CertificateDer;
use rustls_pki_types::PrivateKeyDer;
use rustls_pki_types::pem::PemObject as _;

use crate::transport::net::config::TlsMode;
use crate::transport::net::listener::ListenerWarning;
use crate::transport::net::listener::NetError;
use crate::transport::net::permissions::PermissionCheck;
use crate::transport::net::permissions::inspect_file_permissions;

/// ALPN 只协商 HTTP/1.1：`axum` 的 `http2` feature 未启用，协商出 `h2` 只会让连接不可用。
const ALPN_PROTOCOLS: &[&[u8]] = &[b"http/1.1"];

/// 证书/私钥文件角色；错误与警告只提角色与路径，绝不包含文件内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsFile {
    /// `daemon.tls.cert_path`。
    Certificate,
    /// `daemon.tls.key_path`。
    PrivateKey,
}

impl TlsFile {
    /// 面向运维的中文名（错误消息用）。
    pub fn describe(self) -> &'static str {
        match self {
            Self::Certificate => "证书",
            Self::PrivateKey => "私钥",
        }
    }
}

/// 错误与警告里只出现角色名，绝不出现文件内容。
impl std::fmt::Display for TlsFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.describe())
    }
}

/// 按 `daemon.tls.mode` 构建 TLS 配置：`proxy` 返回 `None`（明文 listener），`direct` 返回终止配置。
pub(crate) fn load_server_config(
    tls: &TlsMode,
    warnings: &mut Vec<ListenerWarning>,
) -> Result<Option<Arc<ServerConfig>>, NetError> {
    let TlsMode::Direct {
        cert_path,
        key_path,
    } = tls
    else {
        return Ok(None);
    };

    for (role, path) in [
        (TlsFile::Certificate, cert_path),
        (TlsFile::PrivateKey, key_path),
    ] {
        match inspect_file_permissions(path) {
            Ok(PermissionCheck::OwnerOnly) => {}
            Ok(PermissionCheck::Relaxed(_)) => {
                return Err(NetError::TlsInsecurePermissions {
                    role,
                    path: path.clone(),
                });
            }
            Ok(PermissionCheck::Unverifiable) => {
                // Windows：不写不安全代码就读不到 ACL（`CORE_PORTS_AND_STORAGE.md` §7.1 的已裁定口径），
                // 因此只记录一次结构化警告，不失败关闭，也不假装已核验。
                warnings.push(ListenerWarning::PermissionsUnverifiable {
                    role,
                    path: path.clone(),
                });
                tracing::warn!(
                    event = "net.tls_permissions_unverifiable",
                    role = role.describe(),
                    "本平台无法核验文件权限（ACL 读取需要不安全代码）：只记录警告"
                );
            }
            Err(source) => {
                return Err(NetError::TlsFileUnreadable {
                    role,
                    path: path.clone(),
                    source,
                });
            }
        }
    }

    let cert_bytes = std::fs::read(cert_path).map_err(|source| NetError::TlsFileUnreadable {
        role: TlsFile::Certificate,
        path: cert_path.clone(),
        source,
    })?;
    let certificates = CertificateDer::pem_slice_iter(&cert_bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| NetError::TlsPemInvalid {
            role: TlsFile::Certificate,
        })?;
    if certificates.is_empty() {
        return Err(NetError::TlsPemInvalid {
            role: TlsFile::Certificate,
        });
    }

    let key_bytes = std::fs::read(key_path).map_err(|source| NetError::TlsFileUnreadable {
        role: TlsFile::PrivateKey,
        path: key_path.clone(),
        source,
    })?;
    let key = PrivateKeyDer::from_pem_slice(&key_bytes).map_err(|_| NetError::TlsPemInvalid {
        role: TlsFile::PrivateKey,
    })?;
    // 解析后立即丢弃 PEM 明文缓冲：私钥字节此后只存在于 rustls 的配置与签名上下文中。
    drop(key_bytes);

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| NetError::TlsConfigBuild)?
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .map_err(|_| NetError::TlsKeyCertificateMismatch)?;
    config.alpn_protocols = ALPN_PROTOCOLS
        .iter()
        .map(|protocol| protocol.to_vec())
        .collect();
    Ok(Some(Arc::new(config)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write_temp_file(label: &str, contents: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "acpr-net-tls-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, contents).expect("写临时文件");
        path
    }

    #[test]
    fn proxy_mode_has_no_tls_configuration() {
        let mut warnings = Vec::new();
        assert!(
            load_server_config(&TlsMode::Proxy, &mut warnings)
                .expect("proxy 模式不加载证书")
                .is_none()
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn invalid_pem_fails_closed_without_leaking_contents() {
        let cert = write_temp_file("bad-cert", b"-----BEGIN CERTIFICATE-----\nnope\n");
        let key = write_temp_file("bad-key", b"-----BEGIN PRIVATE KEY-----\nsecret-bytes\n");
        let mut warnings = Vec::new();
        let error = load_server_config(
            &TlsMode::Direct {
                cert_path: cert.clone(),
                key_path: key.clone(),
            },
            &mut warnings,
        )
        .expect_err("非法 PEM 必须失败关闭");
        let message = error.to_string();
        assert!(!message.contains("secret-bytes"));
        assert!(!message.contains("nope"));
        let _ = std::fs::remove_file(cert);
        let _ = std::fs::remove_file(key);
    }

    #[test]
    fn missing_files_fail_closed() {
        let missing = std::env::temp_dir().join(format!(
            "acpr-net-tls-missing-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let mut warnings = Vec::new();
        let error = load_server_config(
            &TlsMode::Direct {
                cert_path: missing.clone(),
                key_path: missing,
            },
            &mut warnings,
        )
        .expect_err("文件缺失必须失败关闭");
        assert!(matches!(error, NetError::TlsFileUnreadable { .. }));
    }
}
