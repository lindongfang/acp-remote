//! `Host` 边界（`docs/CONFIG_REFERENCE.md` §1、`docs/SECURITY_DESIGN.md` §7.1/§7.2）。
//!
//! 判定分三个分支，与 `CONFIG_REFERENCE.md` §1「`allowed_hosts` 为空时只接受与 `public_origin` 一致的
//! Host」一致：
//!
//! 1. `daemon.allowed_hosts` 非空 → 白名单内的 Host 才被接受；
//! 2. 白名单为空且 `daemon.public_origin` 已配置 → 只接受与它的 host 一致的 Host；
//! 3. 两者都为空（默认配置：只监听 loopback 的形态）→ 只接受 loopback 形态的 Host
//!    （`127.0.0.0/8`、`[::1]`、`localhost`）。
//!
//! 第 3 条是 `CONFIG_REFERENCE.md` §1 在 `public_origin = null` 默认配置下的空档补全，不是语义变更：
//! 严格照字面「只接受与 `public_origin` 一致的 Host」会让默认 loopback 配置的配对/Node Link 端点不可达
//! （与 `specs/node-link-listener/spec.md` 的「默认 loopback 启动」场景冲突），而放行任意 Host 违反
//! `SECURITY_DESIGN.md` §7.2「限制 Host，拒绝任意 Host 转发和 DNS rebinding」。因此两者之间取
//! 「只接受 loopback 形态」，由本变更的 supervisor 裁定（见 `reports/wp2-handoff.md` 的冻结形状说明）。
//!
//! 端口一律忽略：比较的是 host 名（HTTP 语义里端口不参与主机归属判定，反向代理形态下对外端口与
//! Daemon 实际端口本来就不一致）。Host 解析失败、缺失或同一请求出现多个 `Host` 都算不接受。

use axum::http::HeaderMap;
use axum::http::header;

use crate::transport::net::listener::NetError;

/// 接受的 Host 集合。
#[derive(Debug, Clone)]
pub struct HostPolicy {
    accepted: AcceptedHosts,
}

/// [`HostPolicy`] 的三个分支（见模块文档）。
#[derive(Debug, Clone)]
enum AcceptedHosts {
    /// `allowed_hosts` 非空：白名单（已归一化）。
    Allowlist(Vec<String>),
    /// `allowed_hosts` 为空、`public_origin` 已配置：只接受它的 host（已归一化）。
    PublicOrigin(String),
    /// 两者都空：只接受 loopback 形态的 Host。
    LoopbackOnly,
}

impl HostPolicy {
    /// 由 `daemon.public_origin` 与 `daemon.allowed_hosts` 构造；非法取值即失败关闭。
    pub fn new(public_origin: Option<&str>, allowed_hosts: &[String]) -> Result<Self, NetError> {
        if !allowed_hosts.is_empty() {
            let mut normalized = Vec::with_capacity(allowed_hosts.len());
            for entry in allowed_hosts {
                let host = normalize_host(entry).ok_or_else(|| NetError::InvalidAllowedHost {
                    value: entry.clone(),
                })?;
                normalized.push(host);
            }
            return Ok(Self {
                accepted: AcceptedHosts::Allowlist(normalized),
            });
        }
        match public_origin {
            Some(origin) => {
                let host = origin_host(origin).ok_or_else(|| NetError::InvalidPublicOrigin {
                    value: origin.to_owned(),
                })?;
                Ok(Self {
                    accepted: AcceptedHosts::PublicOrigin(host),
                })
            }
            None => Ok(Self {
                accepted: AcceptedHosts::LoopbackOnly,
            }),
        }
    }

    /// 请求是否可进入路由。`None` 表示缺失或非法的 `Host`。
    pub fn accepts(&self, host: Option<&str>) -> bool {
        let Some(host) = host else {
            return false;
        };
        let Some(name) = normalize_host(host) else {
            return false;
        };
        match &self.accepted {
            AcceptedHosts::Allowlist(list) => list.iter().any(|entry| entry == &name),
            AcceptedHosts::PublicOrigin(expected) => expected == &name,
            AcceptedHosts::LoopbackOnly => is_loopback_host(&name),
        }
    }

    /// 从请求头取 Host：缺失、多个值或非 ASCII（按 `to_str` 判定）都返回 `None`。
    pub fn request_host(headers: &HeaderMap) -> Option<&str> {
        let mut values = headers.get_all(header::HOST).iter();
        let first = values.next()?.to_str().ok()?;
        if values.next().is_some() {
            // 重复的 Host 头是请求走私/缓存投毒的常见载体：不接受任何一个。
            return None;
        }
        Some(first)
    }
}

/// 归一化 host 名：小写、去尾点、去掉端口与 `[]`（IPv6 字面量写成 `::1`）。
///
/// 空白、逗号（多个 host 的拼接形态）与非 ASCII 都判为非法。
fn normalize_host(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || !raw.is_ascii() {
        return None;
    }
    if raw.contains(|character: char| character.is_ascii_whitespace() || character == ',') {
        return None;
    }
    let name = if let Some(rest) = raw.strip_prefix('[') {
        // `[::1]` 或 `[::1]:8765`。
        let (inner, tail) = rest.split_once(']')?;
        if !tail.is_empty() && !tail.starts_with(':') {
            return None;
        }
        inner
    } else {
        raw.split(':').next()?
    };
    let name = name.trim_end_matches('.');
    if name.is_empty() {
        return None;
    }
    Some(name.to_ascii_lowercase())
}

/// loopback 形态的 host 名（`127.0.0.0/8`、`::1`、`localhost`）。
fn is_loopback_host(name: &str) -> bool {
    if name == "localhost" {
        return true;
    }
    name.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// `scheme://host[:port][/…]` → 归一化的 host。
fn origin_host(origin: &str) -> Option<String> {
    let (scheme, rest) = origin.split_once("://")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let authority = rest.split(['/', '#', '?']).next()?;
    if authority.contains('@') {
        // origin 不带 userinfo；带 `@` 的取值按非法处理。
        return None;
    }
    normalize_host(authority)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_map(host: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(host) = host {
            headers.insert(header::HOST, host.parse().expect("合法 header 值"));
        }
        headers
    }

    #[test]
    fn allowlist_accepts_only_listed_hosts() {
        let policy = HostPolicy::new(
            Some("https://owner.example.com"),
            &[
                "owner.example.com".to_owned(),
                "proxy.internal:8443".to_owned(),
            ],
        )
        .expect("构造成功");
        assert!(policy.accepts(Some("owner.example.com")));
        assert!(policy.accepts(Some("OWNER.example.com:8765")));
        assert!(policy.accepts(Some("proxy.internal")));
        assert!(!policy.accepts(Some("evil.example.com")));
        assert!(!policy.accepts(None));
    }

    #[test]
    fn public_origin_accepts_only_its_own_host() {
        let policy =
            HostPolicy::new(Some("https://owner.example.com:9443"), &[]).expect("构造成功");
        assert!(policy.accepts(Some("owner.example.com")));
        assert!(policy.accepts(Some("owner.example.com:8765")));
        assert!(!policy.accepts(Some("evil.example.com")));
        // `public_origin` 的端口不参与判定（host 名一致即可）。
        assert!(policy.accepts(Some("owner.example.com:1")));
    }

    #[test]
    fn empty_configuration_accepts_loopback_hosts_only() {
        let policy = HostPolicy::new(None, &[]).expect("构造成功");
        // 默认 loopback 形态：本机地址与任意端口都可达。
        assert!(policy.accepts(Some("127.0.0.1:8765")));
        assert!(policy.accepts(Some("127.0.0.1")));
        assert!(policy.accepts(Some("127.9.9.9:1")));
        assert!(policy.accepts(Some("[::1]:8765")));
        assert!(policy.accepts(Some("localhost:8765")));
        assert!(policy.accepts(Some("LOCALHOST")));
        // 但 DNS rebinding 形态的 Host 一律拒绝（§7.2）。
        assert!(!policy.accepts(Some("evil.example.com")));
        assert!(!policy.accepts(Some("owner.example.com")));
        assert!(!policy.accepts(None));
    }

    #[test]
    fn malformed_hosts_are_rejected() {
        let policy = HostPolicy::new(None, &[]).expect("构造成功");
        assert!(!policy.accepts(Some("")));
        assert!(!policy.accepts(Some("127.0.0.1 evil.example.com")));
        assert!(!policy.accepts(Some("127.0.0.1, evil.example.com")));
        assert!(!policy.accepts(Some("127.0.0.1,127.0.0.1")));
        assert!(!policy.accepts(Some("全角.example.com")));
        assert!(!policy.accepts(Some("[::1")));
        assert!(!policy.accepts(Some("[::1]x")));
        assert!(!policy.accepts(Some(".")));
    }

    #[test]
    fn repeated_host_headers_are_rejected() {
        let mut headers = header_map(Some("127.0.0.1:8765"));
        headers.append(header::HOST, "evil.example.com".parse().expect("值"));
        assert_eq!(HostPolicy::request_host(&headers), None);
    }

    #[test]
    fn request_host_reads_the_single_header() {
        let headers = header_map(Some("127.0.0.1:8765"));
        assert_eq!(HostPolicy::request_host(&headers), Some("127.0.0.1:8765"));
        assert_eq!(HostPolicy::request_host(&HeaderMap::new()), None);
    }

    #[test]
    fn invalid_configuration_values_fail_closed() {
        assert!(HostPolicy::new(Some("owner.example.com"), &[]).is_err());
        assert!(HostPolicy::new(Some("https://"), &[]).is_err());
        assert!(HostPolicy::new(Some("ftp://owner.example.com"), &[]).is_err());
        assert!(HostPolicy::new(Some("https://user@owner.example.com"), &[]).is_err());
        assert!(HostPolicy::new(None, &["bad host".to_owned()]).is_err());
        assert!(HostPolicy::new(None, &["".to_owned()]).is_err());
    }

    #[test]
    fn origin_host_parses_the_documented_shape() {
        assert_eq!(
            origin_host("https://owner.example.com"),
            Some("owner.example.com".to_owned())
        );
        assert_eq!(
            origin_host("https://owner.example.com:9443"),
            Some("owner.example.com".to_owned())
        );
        assert_eq!(
            origin_host("http://127.0.0.1:8765/"),
            Some("127.0.0.1".to_owned())
        );
        assert_eq!(origin_host("https://[::1]:9443"), Some("::1".to_owned()));
        assert_eq!(origin_host("wss://owner.example.com"), None);
    }
}
