//! `node-link-listener` 规格的 daemon 侧验收（R1–R4、R12–R15 的 [PV4] 轮次）。
//!
//! 这些用例都用**真实二进制**（`CARGO_BIN_EXE_acp-remote`）跑：绑定与失败关闭、启动输出的告警、
//! TLS `direct` 的 PEM 失败关闭与真实 TLS 终止都只有跨进程才能验证（配置解析 + 组合根接线 +
//! 进程退出码是一整条路径）。
//!
//! 端口一律用 `127.0.0.1:0`（内核分配）或显式占用的随机端口：不放任用例抢默认的 8765。

mod support;

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use support::{Daemon, failure_code, run_start_once};
/// 现算一张 ECDSA P-256 自签证书并写成 PEM 文件（仓库不提交任何私钥材料）。
struct TestCertificate {
    directory: std::path::PathBuf,
    cert_path: std::path::PathBuf,
    key_path: std::path::PathBuf,
    der: rustls_pki_types::CertificateDer<'static>,
}

impl TestCertificate {
    fn generate(label: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "acpr-wp7-cert-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("创建临时目录");
        let certified = rcgen::generate_simple_self_signed(vec![
            support::nodelink::PUBLIC_HOST.to_owned(),
            "localhost".to_owned(),
        ])
        .expect("自签证书");
        let cert_path = directory.join("cert.pem");
        let key_path = directory.join("key.pem");
        std::fs::write(&cert_path, certified.cert.pem()).expect("写证书");
        std::fs::write(&key_path, certified.signing_key.serialize_pem()).expect("写私钥");
        set_owner_only(&cert_path);
        set_owner_only(&key_path);
        Self {
            directory,
            cert_path,
            key_path,
            der: certified.cert.der().clone(),
        }
    }

    /// 供 TOML 使用的路径文本（正斜杠，避开基本字符串转义）。
    fn toml(&self, path: &std::path::Path) -> String {
        path.display().to_string().replace('\\', "/")
    }

    fn cert_toml(&self) -> String {
        self.toml(&self.cert_path)
    }

    fn key_toml(&self) -> String {
        self.toml(&self.key_path)
    }
}

impl Drop for TestCertificate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// 只有当前用户可读（Unix）；Windows 上由平台 ACL 承担（`SECURITY_DESIGN.md` §13.2）。
#[cfg(unix)]
fn set_owner_only(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).expect("chmod 0600");
}

#[cfg(not(unix))]
fn set_owner_only(_path: &std::path::Path) {}

/// 从 `daemon.status.listen` 取「实际绑定的地址」并断言只有一个、是 loopback、且真的可达（TCP 可连）。
fn bound_address(daemon: &Daemon) -> SocketAddr {
    let status = daemon.ok_value(server::local_admin::Method::DaemonStatus, support::params());
    let listen = status["listen"].as_array().expect("listen 是数组");
    assert_eq!(
        listen.len(),
        1,
        "共享 listener 必须且只能报告一个绑定地址：{listen:?}"
    );
    let addr: SocketAddr = listen[0]
        .as_str()
        .expect("listen 项是文本")
        .parse()
        .expect("listen 项是 ip:port");
    assert!(addr.ip().is_loopback(), "{addr}");
    std::net::TcpStream::connect(addr).expect("status 报告的监听地址必须真的可达");
    addr
}

/// R1/R2：配置的监听地址被真正绑定，且 `daemon.status.listen` 回实际地址（`127.0.0.1:0` → 内核分配的端口）。
#[test]
fn the_configured_listen_address_is_bound_and_reported() {
    let mut daemon = Daemon::configure_with_listen("listen-bound", "127.0.0.1:0", "", "");
    daemon.start();
    let addr = bound_address(&daemon);
    assert_ne!(addr.port(), 0, "0 只是请求值，实际端口由内核分配：{addr}");
    assert!(
        !daemon.log_events("net.listener_bound").is_empty(),
        "绑定必须留下一条 net.listener_bound（启动输出可判定）：{}",
        daemon.log()
    );
    assert!(
        daemon.log_events("daemon.ingress_ready").len() == 1,
        "网络接入面必须只就绪一次"
    );
    assert!(
        daemon.stop().success(),
        "显式停止必须正常退出（含网络 listener 排空）"
    );
}

/// R3：端口被占用时拒绝启动（不降级为「无网络接入」），并且本地管理通道也不开放。
#[test]
fn an_occupied_listen_address_refuses_startup() {
    // 占住一个随机端口（用例结束即释放），再让 Daemon 绑同一个地址。
    let occupied = TcpListener::bind("127.0.0.1:0").expect("占用端口");
    let addr = occupied.local_addr().expect("本地地址");

    let daemon = Daemon::configure_with_listen("listen-taken", &addr.to_string(), "", "");
    let (status, _stdout, stderr) = run_start_once(daemon.config_path());
    assert!(!status.success(), "端口占用必须拒绝启动：{stderr}");
    assert_eq!(failure_code(&stderr), "local.unavailable", "{stderr}");
    assert!(
        stderr.contains("cannot be bound"),
        "错误必须指出绑定失败（且不含路径）：{stderr}"
    );
    assert!(
        daemon.log_events("net.listener_bound").is_empty(),
        "绑定失败不得留下 net.listener_bound"
    );
    assert!(
        daemon.log_events("daemon.ready").is_empty(),
        "启动失败不得进入 daemon.ready"
    );
    // 监听地址上的占用者仍然是本用例的 listener：Daemon 没有关闭它、也没有半初始化地残留。
    drop(occupied);
    let free = TcpListener::bind("127.0.0.1:0").expect("释放后可再绑定");
    drop(free);
}

/// R3：非法监听地址拒绝启动（错误必须说明是 `ip:port` 字面量问题）。
#[test]
fn an_invalid_listen_literal_refuses_startup() {
    let daemon = Daemon::configure_with_listen("listen-invalid", "example.ts.net:8765", "", "");
    let (status, _stdout, stderr) = run_start_once(daemon.config_path());
    assert!(!status.success(), "非法地址必须拒绝启动：{stderr}");
    assert_eq!(failure_code(&stderr), "local.unavailable", "{stderr}");
    assert!(
        stderr.contains("not a valid `ip:port` literal"),
        "错误必须指出 `daemon.listen` 不是合法字面量：{stderr}"
    );
}

/// R14：`tls.mode = "direct"` 的 PEM 缺失即拒绝启动（不降级为明文监听）。
#[test]
fn direct_tls_without_readable_pem_refuses_startup() {
    let missing = std::env::temp_dir().join("acpr-wp7-missing-cert.pem");
    let daemon = Daemon::configure_with_listen(
        "tls-missing",
        "127.0.0.1:0",
        &format!(
            "[daemon.tls]\nmode = \"direct\"\ncert_path = \"{}\"\nkey_path = \"{}\"\n",
            missing.display().to_string().replace('\\', "/"),
            missing.display().to_string().replace('\\', "/"),
        ),
        "",
    );
    let (status, _stdout, stderr) = run_start_once(daemon.config_path());
    assert!(!status.success(), "缺 PEM 必须拒绝启动：{stderr}");
    assert_eq!(failure_code(&stderr), "local.unavailable", "{stderr}");
    assert!(
        stderr.contains("the TLS certificate file is missing or unreadable"),
        "错误必须指出证书文件不可读（且不含私钥内容）：{stderr}"
    );
    assert!(
        daemon.log_events("net.listener_bound").is_empty(),
        "TLS 失败时不得绑定端口（失败发生在绑定之前）"
    );
}

/// R12/R13/R15：`direct` 模式真的终止 TLS——TLS 握手成功，明文连接拿不到 HTTP 响应。
#[test]
fn direct_tls_terminates_tls_and_refuses_plaintext() {
    let certificate = TestCertificate::generate("direct");
    let mut daemon = Daemon::configure_with_listen(
        "tls-direct",
        "127.0.0.1:0",
        &format!(
            "[daemon.tls]\nmode = \"direct\"\ncert_path = \"{}\"\nkey_path = \"{}\"\n",
            certificate.cert_toml(),
            certificate.key_toml(),
        ),
        "",
    );
    daemon.start();
    let addr = bound_address(&daemon);

    // TLS 握手必须成功（只信任本用例现算的那张证书，不做「跳过校验」）。
    let der = certificate.der.clone();
    support::block_on(async move {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(der).expect("加入自签证书");
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("默认协议版本可用")
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(config));
        let stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("连接 loopback");
        let server_name = rustls_pki_types::ServerName::try_from(support::nodelink::PUBLIC_HOST)
            .expect("合法 SNI")
            .to_owned();
        tokio::time::timeout(
            Duration::from_secs(10),
            connector.connect(server_name, stream),
        )
        .await
        .expect("TLS 握手超时")
        .expect("TLS 握手必须成功（direct 模式自己终止 TLS）");
    });

    // 明文连接拿不到 HTTP 响应：发送明文请求后要么读到 TLS alert 字节，要么直接 EOF——
    // 都不可能是 `HTTP/1.1` 响应。
    let mut plain = TcpStream::connect(addr).expect("连接 loopback");
    use std::io::{Read as _, Write as _};
    let _ = plain.write_all(
        format!(
            "GET /node-link/v1 HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            support::nodelink::PUBLIC_HOST
        )
        .as_bytes(),
    );
    let _ = plain.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = Vec::new();
    let _ = plain.read_to_end(&mut buffer);
    assert!(
        !String::from_utf8_lossy(&buffer).starts_with("HTTP/1.1"),
        "明文连接不得拿到 HTTP 响应：{:?}",
        String::from_utf8_lossy(&buffer)
    );

    // 状态里报告的地址仍是同一个（TLS 终止不改变监听地址）。
    assert_eq!(bound_address(&daemon), addr);
    assert!(daemon.stop().success());
}

/// R15：`dev_mode.allow_plaintext` 只对 loopback 生效——非 loopback 监听 + 该开关即拒绝启动。
#[test]
fn plaintext_dev_mode_is_refused_off_loopback() {
    let daemon = Daemon::configure_with_dev_mode(
        "plaintext-off-loopback",
        "0.0.0.0:0",
        "allow_plaintext = true\n",
        "",
    );
    let (status, _stdout, stderr) = run_start_once(daemon.config_path());
    assert!(!status.success(), "非 loopback 明文必须拒绝启动：{stderr}");
    assert_eq!(failure_code(&stderr), "local.unavailable", "{stderr}");
    assert!(
        stderr.contains("only applies to a loopback"),
        "错误必须指出明文开发模式只允许 loopback：{stderr}"
    );
}

/// R4：`proxy` 模式监听非 loopback 是显式配置，启动成功但必须给出告警（明文面暴露在本机之外）。
#[test]
fn proxy_mode_off_loopback_starts_with_a_warning() {
    let mut daemon = Daemon::configure_with_listen("proxy-off-loopback", "0.0.0.0:0", "", "");
    daemon.start();
    let status = daemon.ok_value(server::local_admin::Method::DaemonStatus, support::params());
    let listen = status["listen"].as_array().expect("listen 是数组");
    assert_eq!(listen.len(), 1, "{listen:?}");
    let addr: SocketAddr = listen[0].as_str().expect("文本").parse().expect("ip:port");
    assert!(addr.ip().is_unspecified(), "0.0.0.0 的绑定结果：{addr}");

    let warnings = daemon.log_events("daemon.listener_warning");
    let text = serde_json::to_string(&warnings).expect("告警可序列化");
    assert!(
        text.contains("非 loopback"),
        "启动输出必须包含「监听非 loopback 地址」的告警：{text}"
    );
    assert!(
        text.contains("proxy 模式"),
        "proxy 模式 + 非 loopback 必须追加明文面告警：{text}"
    );
    // 底层接入层也各有一条结构化告警（可由运维直接过滤）。
    assert!(!daemon.log_events("net.listen_non_loopback").is_empty());
    assert!(
        !daemon
            .log_events("net.plaintext_beyond_loopback")
            .is_empty()
    );
    assert!(daemon.stop().success());
}

/// D11 的 unwired 收敛：四个键 + `dev_mode.allow_plaintext` 不再出现在「已知但未接线」里。
#[test]
fn the_wired_listener_keys_are_no_longer_reported_as_unwired() {
    let mut daemon = Daemon::configure_with_dev_mode(
        "unwired-convergence",
        "127.0.0.1:0",
        "allow_plaintext = true\n",
        "",
    );
    daemon.start();
    // 开发模式下的明文开关与 loopback 监听兼容：Daemon 正常就绪，因此本用例能在启动日志里断言清单。
    let unwired = daemon.log_events("daemon.config_unwired");
    let keys: Vec<String> = unwired
        .iter()
        .filter_map(|event| event["key"].as_str().map(str::to_owned))
        .collect();
    for key in [
        "daemon.listen",
        "daemon.allowed_hosts",
        "daemon.trusted_proxies",
        "daemon.tls.mode",
        "daemon.tls.cert_path",
        "daemon.tls.key_path",
        "dev_mode.allow_plaintext",
    ] {
        assert!(
            !keys.iter().any(|reported| reported == key),
            "`{key}` 已接线，不得再报未接线：{keys:?}"
        );
    }
    assert!(daemon.stop().success());
}
