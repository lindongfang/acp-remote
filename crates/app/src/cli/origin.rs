//! `acp-remote config origin`：只读地检测本机候选地址，打印 `daemon.public_origin` 建议值。
//!
//! 这个命令**不写配置文件**：`public_origin` 绑定设备身份，写错的代价比多复制一行高。
//! 它也不参与 Daemon 启动路径：`tailscale` 命令只在用户显式调用本命令时执行，且有超时。
//!
//! 候选来源（`CONFIG_REFERENCE.md` §1）：
//! 1. `tailscale status --json` 的 `Self.DNSName`（MagicDNS 名字，HTTPS 场景唯一能签证书的形态）；
//! 2. `tailscale status --json` 的 `Self.TailscaleIPs`；
//! 3. `if-addrs` 枚举的本机非 loopback 地址（Tailscale 网段、局域网、IPv6）。

use std::io::{self, IsTerminal as _, Write as _};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::process::Command as ProcessCommand;
use std::sync::mpsc;
use std::time::Duration;

use server::transport::net::TlsMode;

use super::Context;

/// `tailscale status --json` 的超时：命令只读本地状态，正常情况下远小于这个值。
const TAILSCALE_TIMEOUT: Duration = Duration::from_secs(3);

/// 一个可用作 `public_origin` 的候选。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    /// 主机名或 IP 字面量（不带端口）。
    host: String,
    /// 人类可读的来源标签。
    label: &'static str,
    /// 证书/可达性提示。
    note: &'static str,
    /// 排序优先级：越小越靠前（MagicDNS < Tailscale IP < 局域网 < 其它）。
    priority: u8,
}

/// 执行命令：只打印建议值，不落盘。
pub(crate) fn run(context: &Context) -> Result<(), String> {
    let config = &context.loaded.config;
    let mut candidates = tailscale_candidates();
    candidates.extend(interface_candidates());
    dedupe(&mut candidates);
    candidates.sort_by_key(|candidate| candidate.priority);

    if candidates.is_empty() {
        return Err(
            "未检测到可用的非 loopback 地址；请手动把 daemon.public_origin 配成对端可访问的域名或 IP"
                .to_owned(),
        );
    }

    if !candidates.iter().any(|candidate| candidate.priority == 0) {
        eprintln!(
            "未检测到 Tailscale MagicDNS 名字（命令不可用或未登录）；以下候选来自本机网卡。\
             HTTPS 场景优先使用 `tailscale serve status` 显示的名字。"
        );
    }

    let direct = matches!(&config.tls, TlsMode::Direct { .. });
    let origin = |candidate: &Candidate| format_origin_value(&candidate.host, direct, config.port);

    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        println!("检测到以下候选：");
        for (index, candidate) in candidates.iter().enumerate() {
            println!(
                "  {}. {}（{}）— {}",
                index + 1,
                origin(candidate),
                candidate.label,
                candidate.note
            );
        }
        println!();
        print!("选择 [1-{}]（回车 = 推荐 1）：", candidates.len());
        io::stdout().flush().map_err(|error| error.to_string())?;
        let mut line = String::new();
        io::stdin()
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        let choice = line.trim();
        let selected = if choice.is_empty() {
            &candidates[0]
        } else {
            let index = choice
                .parse::<usize>()
                .ok()
                .filter(|index| (1..=candidates.len()).contains(index))
                .ok_or_else(|| format!("无效选择 `{choice}`：请输入 1..={}", candidates.len()))?;
            &candidates[index - 1]
        };
        println!();
        println!("# 写入配置文件（[daemon] 段）：");
        println!("public_origin = \"{}\"", origin(selected));
        return Ok(());
    }

    // 非交互：直接给出推荐值与全部候选，便于脚本/粘贴。
    println!("推荐：");
    println!("public_origin = \"{}\"", origin(&candidates[0]));
    println!();
    println!("其它候选：");
    for candidate in candidates.iter().skip(1) {
        println!(
            "  {}（{}）— {}",
            origin(candidate),
            candidate.label,
            candidate.note
        );
    }
    Ok(())
}

/// 把候选主机格式化为 `public_origin` 的取值（`host[:port]`；IPv6 加方括号）。
///
/// `direct` 模式且端口不是 443 时才带端口；`proxy` 模式的外部端口由反代决定，通常就是 443。
fn format_origin_value(host: &str, direct: bool, port: u16) -> String {
    let host = if host.parse::<Ipv6Addr>().is_ok() && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    if direct && port != 443 {
        format!("{host}:{port}")
    } else {
        host
    }
}

/// `tailscale status --json` 的候选。命令不可用、超时或输出非法时返回空列表。
fn tailscale_candidates() -> Vec<Candidate> {
    let Some(output) = run_tailscale() else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    if let Some(name) = value
        .pointer("/Self/DNSName")
        .and_then(|value| value.as_str())
    {
        let name = name.trim_end_matches('.').to_owned();
        if !name.is_empty() {
            candidates.push(Candidate {
                host: name,
                label: "Tailscale MagicDNS",
                note: "推荐：tailscale cert / tailscale serve 可签",
                priority: 0,
            });
        }
    }
    if let Some(ips) = value
        .pointer("/Self/TailscaleIPs")
        .and_then(|value| value.as_array())
    {
        for ip in ips.iter().filter_map(|value| value.as_str()) {
            candidates.push(Candidate {
                host: ip.to_owned(),
                label: "Tailscale IP",
                note: "仅当 TLS 证书包含该 IP 时可用",
                priority: 1,
            });
        }
    }
    candidates
}

/// 执行 `tailscale status --json`，带超时。
///
/// 用独立线程 + `recv_timeout`：`std::process::Command` 没有超时 API。命令是 CLI 的叶子操作，
/// 超时后线程随进程退出，不影响 Daemon。
fn run_tailscale() -> Option<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let output = ProcessCommand::new("tailscale")
            .args(["status", "--json"])
            .output();
        let _ = sender.send(output);
    });
    match receiver.recv_timeout(TAILSCALE_TIMEOUT) {
        Ok(Ok(output)) if output.status.success() => Some(output.stdout),
        _ => None,
    }
}

/// 本机网卡候选（不含 loopback、link-local 与未指定地址）。
fn interface_candidates() -> Vec<Candidate> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for interface in interfaces {
        match interface.addr {
            if_addrs::IfAddr::V4(address) => {
                let ip = address.ip;
                if ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() {
                    continue;
                }
                let (label, priority) = if is_tailscale_v4(ip) {
                    ("Tailscale IP", 1)
                } else if ip.is_private() {
                    ("局域网", 2)
                } else {
                    ("其它 IPv4", 3)
                };
                candidates.push(Candidate {
                    host: ip.to_string(),
                    label,
                    note: "仅当 TLS 证书包含该地址时可用",
                    priority,
                });
            }
            if_addrs::IfAddr::V6(address) => {
                let ip = address.ip;
                if ip.is_loopback() || ip.is_unspecified() || is_ipv6_link_local(ip) {
                    continue;
                }
                let (label, priority) = if is_tailscale_v6(ip) {
                    ("Tailscale IPv6", 1)
                } else {
                    ("IPv6", 3)
                };
                candidates.push(Candidate {
                    host: ip.to_string(),
                    label,
                    note: "仅当 TLS 证书包含该地址时可用",
                    priority,
                });
            }
        }
    }
    candidates
}

/// `100.64.0.0/10`（Tailscale 使用的 CGNAT 网段）。
fn is_tailscale_v4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

/// Tailscale IPv6 ULA：`fd7a:115c:a1e0::/48`。
fn is_tailscale_v6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    segments[0] == 0xfd7a && segments[1] == 0x115c && segments[2] == 0xa1e0
}

fn is_ipv6_link_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

/// 去重：同一个主机只保留优先级最高的那条。
fn dedupe(candidates: &mut Vec<Candidate>) {
    let mut seen = std::collections::BTreeSet::new();
    candidates.retain(|candidate| seen.insert(candidate.host.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailscale_ranges_are_classified() {
        assert!(is_tailscale_v4("100.64.0.1".parse().expect("ip")));
        assert!(is_tailscale_v4("100.127.255.254".parse().expect("ip")));
        assert!(!is_tailscale_v4("100.63.255.255".parse().expect("ip")));
        assert!(!is_tailscale_v4("100.128.0.0".parse().expect("ip")));
        assert!(is_tailscale_v6("fd7a:115c:a1e0::1".parse().expect("ip")));
        assert!(!is_tailscale_v6("fd00::1".parse().expect("ip")));
    }

    #[test]
    fn origin_value_only_carries_the_port_in_direct_mode() {
        assert_eq!(
            format_origin_value("host.ts.net", false, 8765),
            "host.ts.net"
        );
        assert_eq!(
            format_origin_value("host.ts.net", true, 8765),
            "host.ts.net:8765"
        );
        assert_eq!(format_origin_value("host.ts.net", true, 443), "host.ts.net");
    }

    #[test]
    fn ipv6_hosts_are_bracketed() {
        assert_eq!(format_origin_value("::1", true, 8765), "[::1]:8765");
        assert_eq!(
            format_origin_value("fd7a:115c:a1e0::1", false, 8765),
            "[fd7a:115c:a1e0::1]"
        );
        assert_eq!(format_origin_value("127.0.0.1", false, 8765), "127.0.0.1");
    }
}
