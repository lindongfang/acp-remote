//! 升级请求的**来源**（HTTP `Origin`）边界：只如实取得并归一化，不判定是否与设备绑定一致
//! （`docs/SYNC_PROTOCOL.md` §3.2）。
//!
//! **来源与 `Host` 不可互相替代**：`Host` 回答「这个请求打到了哪个虚拟主机」，由
//! [`HostPolicy`](crate::transport::net::HostPolicy) 按 `daemon.allowed_hosts`/`daemon.public_origin`
//! 判定（`docs/CONFIG_REFERENCE.md` §1）；而来源回答「发起请求的页面从哪个来源加载」——它本身就是 Sync
//! 设备身份持久化的绑定对象，即 canonical origin（`docs/adr/0001-pwa-identity-and-secure-transport.md`
//! 的 Canonical Origin：`scheme + hostname + port`，一个 PWA 设备身份只绑定一个来源）。反向代理形态下两者
//! 可以合法地不同（浏览器地址栏是 `https://pc.example.ts.net`，代理转发时可以把 `Host` 改写成内部名），
//! 因此：来源是**协议层**概念、`Host` 是**虚拟主机**概念，两者在接入层各判一次，既不互为替代，也不能
//! 因为其中一个通过就认定另一个合法。反向代理必须原样保留 `Origin`（`docs/SECURITY_DESIGN.md` §7.2）。
//!
//! 归一化口径与 `docs/SYNC_PROTOCOL.md` §3.2 逐条对应：
//!
//! | §3.2 的条款 | 本模块的处理 |
//! |---|---|
//! | 小写 scheme | `https`/`http` 的大小写一律归一为小写；其余 scheme 判非法（不是合法来源，也不是「无来源」） |
//! | 浏览器规范化 hostname | 复用 `Host` 边界的 host 归一化：小写、去尾部点、IPv6 去方括号后再按需补回；非 ASCII 判非法（浏览器发送前已按 IDNA 转成 A-label，本层不引入 UTS-46 转换，避免把与绑定不同的 host 判成一致） |
//! | 显式有效 port | authority 里写出的端口必须是十进制且落在 `0..=65535`；不写端口就是没有端口 |
//! | 默认端口规范化为 `https:443`、`http:80` 后再比较；线上字符串形式省略默认端口 | `https://x:443` → `https://x`；`http://x:80` → `http://x`；非默认端口保留为 `scheme://host:port` |
//! | 不包含 path、query、fragment 或尾部 `/` | 出现任一即判非法（**不剥离**：来源不是 URL，静默改写会掩盖客户端实际发来的取值，而该取值要与设备绑定逐字比较） |
//! | 服务端必须校验 `Origin`，不能信任客户端 JSON 中自报的 Origin | 取值只来自升级请求头；上层只消费 [`RequestOrigin`]，不读客户端 body 里的自报值 |
//! | 非浏览器原生客户端可以没有 HTTP `Origin`，此时 transcript 中使用空字节串 | 头缺席 → [`RequestOrigin::Absent`]（不是一个空串、也不是某个占位 origin；空字节串由握手层按此形态生成） |
//!
//! **三种形态互不折叠**：头缺席（非浏览器对端的合法形态，`docs/SYNC_PROTOCOL.md` §3.2 明确允许）、头
//! 存在且可归一化（[`RequestOrigin::Known`]）、头存在但不可归一化（[`MalformedOrigin`]）。最后一种在
//! 调用点**失败关闭**而不是降级成「无来源」：来源会与设备记录里的绑定逐字比较，把不可校验的取值改写成
//! 另一个协议状态等于让客户端自选授权形态，而 `Absent` 在协议里有它自己的含义（§3.2 的「非浏览器对端」）。
//! `Origin: null`（Fetch 规范里 opaque origin 的合法序列化）属于最后一种：它没有 scheme/host/port，
//! 不可能是 canonical origin，也永远不可能等于任何设备绑定，因此按非法处理，不得落进空字节串分支。

use axum::http::HeaderMap;
use axum::http::header;

use crate::transport::net::host::normalize_host;

/// 升级请求里已取的来源（`docs/SYNC_PROTOCOL.md` §3.2）。
///
/// 只承载「取到了什么」，不承载判定结论：与设备记录绑定的逐字比较由 `identity-auth` 的绑定检查完成
/// （`check_binding`），本层不重复实现比较规则（`design.md` 决策 D1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestOrigin {
    /// 请求带 `Origin` 头，取值是按 §3.2 归一化后的 canonical origin 文本（`scheme://host[:port]`，
    /// 默认端口已省略）。归一化口径见模块文档。
    Known(String),
    /// 请求没有 `Origin` 头（非浏览器原生客户端）：按 §3.2，transcript 的来源字段取**空字节串**，
    /// 授权仍按设备与 `Host` 身份判定。这不是「校验失败」，也不是一个占位 origin。
    Absent,
}

impl RequestOrigin {
    /// 归一化后的 canonical origin；[`RequestOrigin::Absent`] 时为 `None`。
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Known(origin) => Some(origin),
            Self::Absent => None,
        }
    }
}

/// `Origin` 头存在但不可归一化（非法形态）。
///
/// 调用方 MUST 失败关闭，MUST NOT 降级成 [`RequestOrigin::Absent`]：见模块文档「三种形态互不折叠」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MalformedOrigin;

/// 从升级请求头取来源（`docs/SYNC_PROTOCOL.md` §3.2）。
///
/// - 头缺席 → `Ok(`[`RequestOrigin::Absent`]`)`；
/// - 头存在且可归一化 → `Ok(`[`RequestOrigin::Known`]`)`；
/// - 头存在但不可归一化（含重复头、非 ASCII 值、`null`、非 `http(s)` scheme、带 path/query/fragment、
///   非法端口等）→ `Err(`[`MalformedOrigin`]`)`，调用方失败关闭。
pub(crate) fn request_origin(headers: &HeaderMap) -> Result<RequestOrigin, MalformedOrigin> {
    let mut values = headers.get_all(header::ORIGIN).iter();
    let Some(value) = values.next() else {
        return Ok(RequestOrigin::Absent);
    };
    // 同一请求出现多个 `Origin` 头没有合法来源（浏览器只发一个）：不接受任何一个（与 `Host` 边界同处置）。
    if values.next().is_some() {
        return Err(MalformedOrigin);
    }
    let raw = value.to_str().map_err(|_| MalformedOrigin)?;
    normalize_origin(raw)
        .map(RequestOrigin::Known)
        .ok_or(MalformedOrigin)
}

/// 把 `Origin` 头的取值归一化成 canonical origin 文本；非法形态返回 `None`。
///
/// 归一化口径与 `docs/SYNC_PROTOCOL.md` §3.2 的逐条对应见模块文档。
fn normalize_origin(raw: &str) -> Option<String> {
    // 头值两侧的空白按 HTTP 的 OWS 规则裁掉（`http` 解析头值时也会裁）；值内部的空白/控制字符不是来源的
    // 一部分，出现即判非法，不给 `host :port` 这类形态留「先裁再接受」的余地。
    let raw = raw.trim();
    if raw.is_empty() || !raw.is_ascii() {
        return None;
    }
    if raw
        .bytes()
        .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return None;
    }
    let (scheme, authority) = raw.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    // 只接受 http(s)：来源必须是可比较的 web 来源，其余 scheme（含 `wss`）判非法。
    if !matches!(scheme.as_str(), "http" | "https") {
        return None;
    }
    // 来源是 `scheme://authority`，不含 path/query/fragment；带 userinfo（`@`）的取值同样非法。
    if authority.is_empty() || authority.contains(['/', '?', '#', '@']) {
        return None;
    }
    let (host, port) = split_authority(authority)?;
    // 与 `Host` 边界共用同一套 host 归一化（小写、去尾部点、去方括号），避免两套口径漂移。
    let host = normalize_host(host)?;
    let port = match port {
        None => None,
        Some(port) => Some(normalize_port(port)?),
    };
    Some(serialize(&scheme, &host, port))
}

/// 拆分 authority 为（host 文本, 端口文本）；形态不合法返回 `None`。
///
/// `[v6]` 形式保留方括号（由 `host::normalize_host` 去掉），未加方括号的 IPv6 不是合法的来源序列化。
fn split_authority(authority: &str) -> Option<(&str, Option<&str>)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (inner, tail) = rest.split_once(']')?;
        // 含方括号的 host 文本：`[` + inner + `]`。
        let host = &authority[..inner.len() + 2];
        return match tail {
            "" => Some((host, None)),
            _ => Some((host, Some(tail.strip_prefix(':')?))),
        };
    }
    match authority.split_once(':') {
        None => Some((authority, None)),
        // 第二个 `:` 表示未加方括号的 IPv6 字面量：不是合法的来源序列化。
        Some((_, port)) if port.contains(':') => None,
        Some((host, port)) => Some((host, Some(port))),
    }
}

/// 端口必须是十进制且落在 `0..=65535`（`u16`）；不合法返回 `None`。
///
/// 前导零（`https://x:0443`）按十进制解析后参与默认端口判定，因此与 `https://x:443` 同值。
fn normalize_port(raw: &str) -> Option<u16> {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse::<u16>().ok()
}

/// 序列化 canonical origin：省略 scheme 的默认端口（`https:443`/`http:80`），IPv6 补回方括号。
fn serialize(scheme: &str, host: &str, port: Option<u16>) -> String {
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    let default_port = if scheme == "https" { 443 } else { 80 };
    match port {
        Some(port) if port != default_port => format!("{scheme}://{host}:{port}"),
        _ => format!("{scheme}://{host}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn origin_of(value: &str) -> Option<String> {
        normalize_origin(value)
    }

    #[test]
    fn normalizes_the_documented_shape() {
        // 小写 scheme + 浏览器规范化 hostname + 显式有效 port，省略默认端口。
        for (raw, expected) in [
            ("https://pc.example.test", "https://pc.example.test"),
            ("HTTPS://PC.Example.Test", "https://pc.example.test"),
            (
                "https://PC.EXAMPLE.TEST:8443",
                "https://pc.example.test:8443",
            ),
            // 默认端口被省略（§3.2：`https:443`/`http:80` 归一化后再比较）。
            ("https://pc.example.test:443", "https://pc.example.test"),
            ("http://pc.example.test:80", "http://pc.example.test"),
            // 前导零按十进制解析后判定默认端口。
            ("https://pc.example.test:0443", "https://pc.example.test"),
            ("http://pc.example.test:0443", "http://pc.example.test:443"),
            // `:0` 不是任何 scheme 的默认端口，按显式端口保留。
            ("http://127.0.0.1:0", "http://127.0.0.1:0"),
            // 尾部点按 host 归一化去掉（与 `Host` 边界同一口径）。
            ("https://pc.example.test.", "https://pc.example.test"),
            // IPv6 字面量：方括号是序列化的一部分，端口可省略或显式。
            ("https://[::1]", "https://[::1]"),
            ("https://[::1]:443", "https://[::1]"),
            ("https://[::1]:9443", "https://[::1]:9443"),
        ] {
            assert_eq!(origin_of(raw).as_deref(), Some(expected), "{raw}");
        }
    }

    #[test]
    fn malformed_values_are_rejected() {
        for raw in [
            "",
            "   ",
            // Fetch 里 opaque origin 的合法序列化，但不是 canonical origin（没有 scheme/host/port）。
            "null",
            // 不是 `scheme://authority` 形态。
            "pc.example.test",
            "//pc.example.test",
            "https:/pc.example.test",
            "https://",
            // scheme 非 http(s)。
            "wss://pc.example.test",
            "ftp://pc.example.test",
            "chrome-extension://abcdef",
            // path/query/fragment/尾部 `/`（含只有一个 `/` 的尾部形态）。
            "https://pc.example.test/",
            "https://pc.example.test/path",
            "https://pc.example.test?x=1",
            "https://pc.example.test#f",
            // userinfo 不属于来源。
            "https://user@pc.example.test",
            "https://user:secret@pc.example.test:8443",
            // 值内部的空白：`https:// pc.example.test` 与 `host :port` 不得靠「先裁再接受」混进来。
            "https:// pc.example.test",
            "https://pc.example.test :80",
            "https://pc.example\ttest",
            // 端口非法。
            "https://pc.example.test:",
            "https://pc.example.test:0x50",
            "https://pc.example.test:8443a",
            "https://pc.example.test:70000",
            "https://pc.example.test:-1",
            // host 非法（空白、逗号拼接、非 ASCII、方括号不闭合或多余后缀）。
            "https://pc.example.test evil.example.com",
            "https://pc.example.test, evil.example.com",
            "https://全角.example.test",
            "https://[::1",
            "https://[::1]x",
            "https://.",
            "https://pc.example.test:8443:99",
            "https://::1",
        ] {
            assert_eq!(origin_of(raw), None, "{raw} 必须判非法");
        }
    }

    #[test]
    fn the_header_is_read_with_the_three_distinct_shapes() {
        let mut headers = HeaderMap::new();
        // 头缺席：非浏览器对端的合法形态。
        assert_eq!(request_origin(&headers), Ok(RequestOrigin::Absent));
        // 头存在且可归一化。
        headers.insert(
            header::ORIGIN,
            "https://PC.Example.Test:443"
                .parse()
                .expect("合法 header 值"),
        );
        assert_eq!(
            request_origin(&headers),
            Ok(RequestOrigin::Known("https://pc.example.test".to_owned()))
        );
        assert_eq!(
            request_origin(&headers).expect("已取来源").as_str(),
            Some("https://pc.example.test")
        );
        // 头存在但非法：失败关闭，不折叠成「无来源」。
        for value in ["null", "https://pc.example.test/", "::"] {
            let mut headers = HeaderMap::new();
            headers.insert(header::ORIGIN, value.parse().expect("合法 header 值"));
            assert_eq!(request_origin(&headers), Err(MalformedOrigin), "{value}");
        }
        // 重复头同样非法（与 `Host` 边界同处置：不接受任何一个）。
        let mut headers = HeaderMap::new();
        headers.append(
            header::ORIGIN,
            "https://pc.example.test".parse().expect("合法 header 值"),
        );
        headers.append(
            header::ORIGIN,
            "https://evil.example.test".parse().expect("合法 header 值"),
        );
        assert_eq!(request_origin(&headers), Err(MalformedOrigin));
        // 非 ASCII 头值（`HeaderValue` 允许 obs-text）同样非法。
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_bytes("https://全角.example.test".as_bytes()).expect("字节值"),
        );
        assert_eq!(request_origin(&headers), Err(MalformedOrigin));
    }

    #[test]
    fn absent_is_not_the_same_as_an_empty_value() {
        // `Origin:` 空值不是「头缺席」：它存在但不可归一化。
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static(""));
        assert_eq!(request_origin(&headers), Err(MalformedOrigin));
        assert_ne!(
            request_origin(&headers),
            Ok(RequestOrigin::Absent),
            "空值与缺席必须区分"
        );
    }
}
