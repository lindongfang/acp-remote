//! 可信代理与转发头（`docs/CONFIG_REFERENCE.md` §1、`docs/SECURITY_DESIGN.md` §7.1/§7.2）。
//!
//! `Forwarded`/`X-Forwarded-*` 只在连接对端地址落在 `daemon.trusted_proxies` 内时被采信；其余来源的
//! 转发头一律忽略（限流与日志因此以对端真实连接地址为准）。链式解析取「从右向左第一个不可信地址」，
//! 与反向代理的追加语义一致：最靠近本进程的那一跳由我们验证过，它右侧的取值可以信，左侧的取值由它保证。
//!
//! `X-Forwarded-Proto`/`X-Forwarded-Host` **不**被消费：TLS 边界与 Host 归属由 `daemon.tls.mode`、
//! `public_origin`/`allowed_hosts` 表达（`CONFIG_REFERENCE.md` §1 的三形态表），反向代理必须原样保留
//! `Host`（§7.2），因此这里不引入第二条 Host 来源。

use std::net::{IpAddr, SocketAddr};

use axum::http::HeaderMap;
use axum::http::header;

use crate::transport::net::listener::NetError;

/// 可信代理集合与转发头解析。
#[derive(Debug, Clone, Default)]
pub struct ProxyPolicy {
    trusted: Vec<IpAddr>,
}

/// 一次请求的真实对端地址解析结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedPeer {
    /// 用于限流与日志的对端地址。
    pub client_ip: IpAddr,
    /// 该地址是否来自被采信的转发头（用于审计归因）。
    pub forwarded_headers: bool,
}

impl ProxyPolicy {
    /// 解析 `daemon.trusted_proxies`。
    ///
    /// 取值可以是裸 IP（`127.0.0.1`）或 `IP:端口`（`127.0.0.1:443`，`CONFIG_REFERENCE.md` §1 的示例形态）；
    /// 端口只是对端代理自身监听地址的书写形式，连进来的连接源端口是临时端口，因此匹配只看 IP。
    /// 其它形态（主机名、CIDR）一律失败关闭，不静默忽略。
    pub fn new(entries: &[String]) -> Result<Self, NetError> {
        let mut trusted = Vec::with_capacity(entries.len());
        for entry in entries {
            let ip = entry
                .trim()
                .parse::<IpAddr>()
                .or_else(|_| entry.trim().parse::<SocketAddr>().map(|addr| addr.ip()))
                .map_err(|_| NetError::InvalidTrustedProxy {
                    value: entry.clone(),
                })?;
            if !trusted.contains(&ip) {
                trusted.push(ip);
            }
        }
        Ok(Self { trusted })
    }

    /// 对端是否可信。
    pub fn is_trusted(&self, peer: IpAddr) -> bool {
        self.trusted.contains(&peer)
    }

    /// 解析真实对端地址：对端不可信时直接返回连接地址，可信时按转发链从右向左取第一个不可信地址。
    pub fn resolve(&self, peer: SocketAddr, headers: &HeaderMap) -> ResolvedPeer {
        let peer_ip = peer.ip();
        if !self.is_trusted(peer_ip) {
            return ResolvedPeer {
                client_ip: peer_ip,
                forwarded_headers: false,
            };
        }
        let Some(chain) = forwarded_chain(headers) else {
            return ResolvedPeer {
                client_ip: peer_ip,
                forwarded_headers: false,
            };
        };
        let client_ip = chain
            .iter()
            .rev()
            .find(|candidate| !self.is_trusted(**candidate))
            .copied()
            .unwrap_or(chain[0]);
        ResolvedPeer {
            client_ip,
            forwarded_headers: true,
        }
    }
}

/// 取转发链：优先 `X-Forwarded-For`（部署里最常见的形态），未提供可用取值时回退 RFC 7239 的 `Forwarded`。
fn forwarded_chain(headers: &HeaderMap) -> Option<Vec<IpAddr>> {
    if let Some(chain) = header_chain(headers, header::HeaderName::from_static("x-forwarded-for")) {
        return Some(chain);
    }
    let forwarded = headers.get_all(header::FORWARDED).iter();
    let mut chain = Vec::new();
    for value in forwarded {
        let Ok(value) = value.to_str() else {
            continue;
        };
        // `Forwarded: for=192.0.2.1;proto=https, for="[2001:db8::1]:1234"`：只取 `for=` 参数。
        for element in value.split(',') {
            for parameter in element.split(';') {
                let Some((name, value)) = parameter.split_once('=') else {
                    continue;
                };
                if name.trim().eq_ignore_ascii_case("for") {
                    if let Some(ip) = parse_forwarded_ip(value) {
                        chain.push(ip);
                    }
                }
            }
        }
    }
    if chain.is_empty() { None } else { Some(chain) }
}

/// 解析一个逗号分隔的转发头，返回其中可解析的 IP 序列（全部不可解析时返回 `None`）。
fn header_chain(headers: &HeaderMap, name: header::HeaderName) -> Option<Vec<IpAddr>> {
    let values = headers.get_all(name).iter();
    let mut chain = Vec::new();
    for value in values {
        let Ok(value) = value.to_str() else {
            continue;
        };
        for element in value.split(',') {
            if let Some(ip) = parse_forwarded_ip(element) {
                chain.push(ip);
            }
        }
    }
    if chain.is_empty() { None } else { Some(chain) }
}

/// 解析一个转发地址（`1.2.3.4`、`1.2.3.4:5678`、`[2001:db8::1]:5678`、`"2001:db8::1"`、`unknown`）。
fn parse_forwarded_ip(raw: &str) -> Option<IpAddr> {
    let token = raw.trim().trim_matches('"').trim();
    if token.is_empty() || token.eq_ignore_ascii_case("unknown") || token.starts_with('_') {
        return None;
    }
    if let Ok(ip) = token.parse::<IpAddr>() {
        return Some(ip);
    }
    token.parse::<SocketAddr>().ok().map(|addr| addr.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(
                header::HeaderName::from_bytes(name.as_bytes()).expect("合法头名"),
                value.parse().expect("合法头值"),
            );
        }
        headers
    }

    fn addr(text: &str) -> SocketAddr {
        text.parse().expect("合法地址")
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().expect("合法地址")
    }

    #[test]
    fn untrusted_peer_never_uses_forwarded_headers() {
        let policy = ProxyPolicy::new(&[]).expect("空白名单合法");
        let resolved = policy.resolve(
            addr("127.0.0.1:5555"),
            &headers(&[("x-forwarded-for", "203.0.113.9")]),
        );
        assert_eq!(resolved.client_ip, addr("127.0.0.1:5555").ip());
        assert!(!resolved.forwarded_headers);
    }

    #[test]
    fn untrusted_peer_ignores_forwarded_header() {
        let policy = ProxyPolicy::new(&["10.0.0.1".to_owned()]).expect("合法");
        let resolved = policy.resolve(
            addr("127.0.0.1:5555"),
            &headers(&[("forwarded", "for=203.0.113.9")]),
        );
        assert_eq!(resolved.client_ip, addr("127.0.0.1:5555").ip());
        assert!(!resolved.forwarded_headers);
    }

    #[test]
    fn trusted_peer_uses_x_forwarded_for() {
        let policy = ProxyPolicy::new(&["127.0.0.1".to_owned()]).expect("合法");
        let resolved = policy.resolve(
            addr("127.0.0.1:5555"),
            &headers(&[("x-forwarded-for", "203.0.113.9")]),
        );
        assert_eq!(resolved.client_ip, ip("203.0.113.9"));
        assert!(resolved.forwarded_headers);
    }

    #[test]
    fn trusted_peer_skips_its_own_appended_addresses() {
        let policy = ProxyPolicy::new(&["127.0.0.1".to_owned()]).expect("合法");
        let resolved = policy.resolve(
            addr("127.0.0.1:5555"),
            &headers(&[("x-forwarded-for", "203.0.113.9, 127.0.0.1")]),
        );
        assert_eq!(resolved.client_ip, ip("203.0.113.9"));
    }

    #[test]
    fn trusted_peer_falls_back_to_the_connection_address_when_headers_are_unusable() {
        let policy = ProxyPolicy::new(&["127.0.0.1".to_owned()]).expect("合法");
        for value in ["unknown", "", "garbage"] {
            let resolved = policy.resolve(
                addr("127.0.0.1:5555"),
                &headers(&[("x-forwarded-for", value)]),
            );
            assert_eq!(resolved.client_ip, addr("127.0.0.1:5555").ip());
            assert!(!resolved.forwarded_headers);
        }
    }

    #[test]
    fn trusted_peer_uses_rfc7239_forwarded_when_x_forwarded_for_is_absent() {
        let policy = ProxyPolicy::new(&["127.0.0.1".to_owned()]).expect("合法");
        let resolved = policy.resolve(
            addr("127.0.0.1:5555"),
            &headers(&[("forwarded", "for=\"[2001:db8::1]:1234\";proto=https")]),
        );
        assert_eq!(resolved.client_ip, ip("2001:db8::1"));
        assert!(resolved.forwarded_headers);
    }

    #[test]
    fn trusted_proxy_entries_accept_ip_and_ip_port_shapes() {
        let policy =
            ProxyPolicy::new(&["127.0.0.1:443".to_owned(), "::1".to_owned()]).expect("合法");
        assert!(policy.is_trusted(addr("127.0.0.1:9999").ip()));
        assert!(policy.is_trusted(addr("[::1]:9999").ip()));
        assert!(!policy.is_trusted(addr("10.0.0.1:1").ip()));
    }

    #[test]
    fn invalid_trusted_proxy_entries_fail_closed() {
        assert!(ProxyPolicy::new(&["proxy.internal".to_owned()]).is_err());
        assert!(ProxyPolicy::new(&["10.0.0.0/8".to_owned()]).is_err());
        assert!(ProxyPolicy::new(&["".to_owned()]).is_err());
    }
}
