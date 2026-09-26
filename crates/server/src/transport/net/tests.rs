//! `transport::net` 的行为用例（`[PV3]` 的 WP2 部分）。
//!
//! 覆盖映射（Coverage Index / `specs/node-link-listener/spec.md`）：
//!
//! | 场景 | 用例 |
//! |---|---|
//! | [R1] 共享 listener 绑定与失败关闭 | `local_addrs_reports_the_real_bound_address`、`bind_fails_closed_when_the_address_is_taken` |
//! | [R2] 默认 loopback 启动 | `default_config_binds_only_loopback`（默认值的权威断言在 `config` 模块；真实 8765 端口的端到端绑定由 WP7 的 `[PV4]` 覆盖） |
//! | [R3] 绑定失败即拒绝启动 | `bind_fails_closed_when_the_address_is_taken` |
//! | [R4] 非 loopback 监听告警 | `non_loopback_listen_warns_without_failing` |
//! | [R5]/[R7] 按 path 路由、Sync 明确不可用 | `unregistered_paths_return_404` |
//! | [R6] Node Link 端点正常升级 | `ws_upgrade_with_the_required_subprotocol_succeeds` |
//! | [R8] 压缩/错误 subprotocol 被拒绝 | `ws_upgrade_without_the_subprotocol_is_rejected`、`ws_upgrade_with_compression_is_rejected`、`plain_get_on_the_ws_path_is_rejected`、`repeated_subprotocol_headers_follow_the_token_rule` |
//! | [R9]/[R10] Host 与代理头边界 | `host_policy_*`（`host` 模块）、`wrong_host_is_rejected_before_routing`、`wrong_host_is_rejected_on_the_ws_upgrade_path`、`allowed_hosts_whitelist_is_used_when_configured`、`default_configuration_accepts_loopback_host_only` |
//! | [R11] 不可信来源的转发头被忽略 | `forwarded_headers_are_ignored_from_untrusted_peers`、`forwarded_headers_are_honored_from_trusted_proxies` |
//! | [R12]/[R13] TLS 两种模式 | `direct_mode_terminates_tls_and_rejects_plaintext`、`direct_mode_does_not_warn_about_plaintext`、`idle_tcp_connections_do_not_block_new_connections`（RV1-WP2-F3 回归） |
//! | [R14] direct 模式证书缺失即拒绝启动 | `direct_mode_missing_certificate_fails_closed`、`direct_mode_invalid_pem_fails_closed`、`direct_mode_relaxed_permissions_fail_closed`（Unix）、`direct_mode_permissions_are_unverifiable_on_this_platform`（Windows，`[PV5]`） |
//! | [R15] 非 loopback 明文边界 | `plaintext_proxy_non_loopback_warns`、`plaintext_dev_flag_rejects_non_loopback`（`listener` 模块） |
//! | [R16]/[R17] 请求体上限 | `pairing_body_over_the_limit_is_rejected_with_413` |
//! | [R18] 超大 WebSocket 消息被拒绝 | `oversize_ws_message_closes_with_1009` |
//! | 关闭排空（D11） | `shutdown_drains_and_releases_the_listener`、`binary_frames_are_passed_to_the_handler` |

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::body::Bytes;
use axum::http::StatusCode;

use super::test_client::TestCertificate;
use super::test_client::client_tls_config;
use super::test_client::connect;
use super::test_client::connect_tls;
use super::test_client::http_request;
use super::test_client::set_owner_only;
use super::test_client::start_server;
use super::test_client::ws_request;
use super::test_client::ws_request_with_headers;
use super::*;

/// 协议冻结的 WSS path 与 subprotocol（`NODE_LINK_PROTOCOL.md` §2.1）；本模块只作为参数接收它们。
const WS_PATH: &str = "/node-link/v1";
/// 同上。
const WS_SUBPROTOCOL: &str = "acp-remote.nodelink.v1.json";
/// 配对 claim 端点（`NODE_LINK_PROTOCOL.md` §13.2）。
const CLAIM_PATH: &str = "/node-link/v1/pairing/claim";
/// 配对 status 端点（`NODE_LINK_PROTOCOL.md` §13.3）。
const STATUS_PATH: &str = "/node-link/v1/pairing/status";

/// 记录调用次数与最后一次请求的处理器（用例断言「没有调用处理器」/「看到的是真实对端地址」）。
struct RecordingHttpHandler {
    calls: Arc<AtomicUsize>,
    last_client_ip: Arc<std::sync::Mutex<Option<std::net::IpAddr>>>,
    status: StatusCode,
}

#[async_trait::async_trait]
impl HttpHandler for RecordingHttpHandler {
    async fn handle(&self, request: HttpRequest) -> HttpResponse {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.last_client_ip.lock().expect("锁可用") = Some(request.peer.client_ip);
        HttpResponse::new(self.status).with_body(Bytes::from(request.peer.client_ip.to_string()))
    }
}

/// 收到帧后计数并回显的 WS 处理器；返回即会话结束。
struct EchoWsHandler {
    messages: Arc<AtomicUsize>,
    last_binary: Arc<std::sync::Mutex<Option<Vec<u8>>>>,
    subprotocol: Arc<std::sync::Mutex<Option<String>>>,
}

#[async_trait::async_trait]
impl WsHandler for EchoWsHandler {
    async fn handle(self: Arc<Self>, mut connection: WsConnection, _peer: PeerInfo) {
        *self.subprotocol.lock().expect("锁可用") =
            connection.negotiated_subprotocol().map(str::to_owned);
        while let Some(message) = connection.recv().await {
            match message {
                Ok(WsMessage::Text(text)) => {
                    self.messages.fetch_add(1, Ordering::SeqCst);
                    if connection.send_text(text).await.is_err() {
                        return;
                    }
                }
                Ok(WsMessage::Binary(bytes)) => {
                    self.messages.fetch_add(1, Ordering::SeqCst);
                    *self.last_binary.lock().expect("锁可用") = Some(bytes.to_vec());
                    if connection.send_binary(bytes).await.is_err() {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    }
}

/// 一直读直到连接结束的 WS 处理器；被取消时由 [`SessionGuard`] 记录。
struct IdleWsHandler {
    finished: Arc<AtomicUsize>,
}

/// 只记录「被调用过几次」的 WS 处理器；`handle` 返回即会话结束。
struct CountingWsHandler {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl WsHandler for CountingWsHandler {
    async fn handle(self: Arc<Self>, _connection: WsConnection, _peer: PeerInfo) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

struct SessionGuard(Arc<AtomicUsize>);

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl WsHandler for IdleWsHandler {
    async fn handle(self: Arc<Self>, mut connection: WsConnection, _peer: PeerInfo) {
        let _guard = SessionGuard(Arc::clone(&self.finished));
        while let Some(message) = connection.recv().await {
            if message.is_err() {
                return;
            }
        }
    }
}

/// 只注册 WS 端点的配置（默认 loopback 随机端口）。
///
/// 排空宽限取 200 ms：用例里的 WS 会话在关闭时由接入选层取消，短宽限让「关闭序列」用例快速结束，
/// 同时仍然真实走过「宽限内排空/超时强制取消」两条路径。
fn loopback_config() -> NetConfig {
    NetConfig {
        listen: "127.0.0.1:0".to_owned(),
        drain_grace: Duration::from_millis(200),
        ..NetConfig::default()
    }
}

fn host_of(addr: SocketAddr) -> String {
    format!("127.0.0.1:{}", addr.port())
}

async fn wait_for_counter(counter: &Arc<AtomicUsize>, expected: usize) {
    for _ in 0..300 {
        if counter.load(Ordering::SeqCst) >= expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        counter.load(Ordering::SeqCst),
        expected,
        "等待计数达到 {expected} 超时"
    );
}

#[tokio::test]
async fn local_addrs_reports_the_real_bound_address() {
    // [R1]/[R2]：绑定随机 loopback 端口后，`daemon.status.listen` 的来源就是实际地址（不是配置里的 0）。
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_post(
                CLAIM_PATH,
                Arc::new(RecordingHttpHandler {
                    calls: Arc::new(AtomicUsize::new(0)),
                    last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                    status: StatusCode::OK,
                }),
            )
            .expect("注册成功");
    })
    .await;
    let addrs = [server.addr];
    assert_eq!(addrs.len(), 1);
    assert!(server.addr.ip().is_loopback());
    assert_ne!(server.addr.port(), 0, "必须是实际绑定的端口");
    assert!(server.warnings.is_empty(), "loopback 不应产生告警");
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn default_config_binds_only_loopback() {
    // [R2]/[R4]：默认配置只落在 loopback（`CONFIG_REFERENCE.md` §1 的默认值断言在 `config` 模块）。
    let config = NetConfig::default();
    assert_eq!(config.listen, "127.0.0.1:8765");
    let bound: SocketAddr = config.listen.parse().expect("默认值可解析");
    assert!(bound.ip().is_loopback());
}

#[tokio::test]
async fn bind_fails_closed_when_the_address_is_taken() {
    // [R1]/[R3]：地址被占用即拒绝启动，且不留下半初始化的监听。
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("占位 listener");
    let addr = occupied.local_addr().expect("地址");
    let error = NetListener::bind(NetConfig {
        listen: addr.to_string(),
        ..NetConfig::default()
    })
    .await
    .expect_err("端口被占用必须失败关闭");
    assert!(matches!(error, NetError::Bind { .. }));
    assert!(error.to_string().contains(&addr.to_string()));
    // 占位 listener 未被本模块影响。
    drop(occupied);
}

#[tokio::test]
async fn non_loopback_listen_warns_without_failing() {
    // [R4]：非 loopback 是显式配置的后果，只告警不拒绝；`proxy` 模式再加一条明文面告警（D10）。
    let listener = NetListener::bind(NetConfig {
        listen: "0.0.0.0:0".to_owned(),
        ..NetConfig::default()
    })
    .await
    .expect("非 loopback 可以绑定");
    assert!(!listener.local_addrs()[0].ip().is_loopback());
    assert!(
        listener
            .warnings()
            .iter()
            .any(|warning| matches!(warning, ListenerWarning::NonLoopbackListen { .. }))
    );
    assert!(
        listener
            .warnings()
            .iter()
            .any(|warning| matches!(warning, ListenerWarning::PlaintextBeyondLoopback { .. }))
    );
}

#[tokio::test]
async fn plaintext_proxy_non_loopback_warns() {
    // [R15]：非 loopback + 明文只可能在显式 proxy 形态下成立，且必须告警。
    let listener = NetListener::bind(NetConfig {
        listen: "0.0.0.0:0".to_owned(),
        tls: TlsMode::Proxy,
        ..NetConfig::default()
    })
    .await
    .expect("proxy 模式下非 loopback 可以启动");
    assert_eq!(listener.warnings().len(), 2);
}

#[tokio::test]
async fn unregistered_paths_return_404() {
    // [R5]/[R7]：只有注册过的 path 有服务；`/sync/v1*` 与未知 path 一律 404，不进任何处理器。
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_ws(
                WS_PATH,
                WS_SUBPROTOCOL,
                Arc::new(IdleWsHandler {
                    finished: Arc::new(AtomicUsize::new(0)),
                }),
            )
            .expect("注册 WS");
        listener
            .register_post(
                CLAIM_PATH,
                Arc::new(RecordingHttpHandler {
                    calls: Arc::new(AtomicUsize::new(0)),
                    last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                    status: StatusCode::OK,
                }),
            )
            .expect("注册 claim");
        listener
            .register_post(
                STATUS_PATH,
                Arc::new(RecordingHttpHandler {
                    calls: Arc::new(AtomicUsize::new(0)),
                    last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                    status: StatusCode::OK,
                }),
            )
            .expect("注册 status");
    })
    .await;
    let host = host_of(server.addr);
    for path in ["/sync/v1", "/sync/v1/pairing/claim", "/", "/unknown"] {
        let mut connection = connect(server.addr).await.expect("连接");
        connection
            .write_all(&http_request("POST", path, &host, &[], b"{}"))
            .await
            .expect("写请求");
        let response = connection.read_response().await.expect("读响应");
        assert_eq!(response.status, 404, "{path} 必须 404");
    }
    // 未注册 path 上的其他方法同样 404（不是 405）。
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(&http_request("GET", "/sync/v1", &host, &[], b""))
        .await
        .expect("写请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        404
    );
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn ws_upgrade_with_the_required_subprotocol_succeeds() {
    // [R6]：带约定 subprotocol、不请求压缩的升级成功，响应回填 subprotocol，处理器收到会话。
    let messages = Arc::new(AtomicUsize::new(0));
    let subprotocol = Arc::new(std::sync::Mutex::new(None));
    let handler = Arc::new(EchoWsHandler {
        messages: Arc::clone(&messages),
        last_binary: Arc::new(std::sync::Mutex::new(None)),
        subprotocol: Arc::clone(&subprotocol),
    });
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_ws(WS_PATH, WS_SUBPROTOCOL, handler)
            .expect("注册 WS");
    })
    .await;
    let host = host_of(server.addr);
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(ws_request(WS_PATH, &host, Some(WS_SUBPROTOCOL), None).as_bytes())
        .await
        .expect("写升级请求");
    let response = connection.read_response().await.expect("读升级响应");
    assert_eq!(response.status, 101);
    assert_eq!(
        response.header("sec-websocket-protocol"),
        Some(WS_SUBPROTOCOL)
    );
    assert_eq!(response.header("sec-websocket-extensions"), None);
    connection.send_text("hello").await.expect("发 text");
    let frame = connection.read_frame().await.expect("收帧");
    assert_eq!(frame.text().as_deref(), Some("hello"));
    wait_for_counter(&messages, 1).await;
    assert_eq!(
        subprotocol.lock().expect("锁可用").as_deref(),
        Some(WS_SUBPROTOCOL)
    );
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn ws_upgrade_without_the_subprotocol_is_rejected() {
    // [R8]：未声明约定 subprotocol 的升级被拒绝（不建立连接、不返回业务级错误消息）。
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_ws(
                WS_PATH,
                WS_SUBPROTOCOL,
                Arc::new(IdleWsHandler {
                    finished: Arc::new(AtomicUsize::new(0)),
                }),
            )
            .expect("注册 WS");
    })
    .await;
    let host = host_of(server.addr);
    for subprotocols in [
        None,
        Some("other.protocol"),
        Some("ACP-REMOTE.NODELINK.V1.JSON"),
    ] {
        let mut connection = connect(server.addr).await.expect("连接");
        connection
            .write_all(ws_request(WS_PATH, &host, subprotocols, None).as_bytes())
            .await
            .expect("写升级请求");
        let response = connection.read_response().await.expect("读响应");
        assert_eq!(response.status, 400, "{subprotocols:?} 必须被拒绝");
    }
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn ws_upgrade_with_compression_is_rejected() {
    // [R8]：协商到 permessage-deflate 的升级必须被拒绝（`NODE_LINK_PROTOCOL.md` §2.1）。
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_ws(
                WS_PATH,
                WS_SUBPROTOCOL,
                Arc::new(IdleWsHandler {
                    finished: Arc::new(AtomicUsize::new(0)),
                }),
            )
            .expect("注册 WS");
    })
    .await;
    let host = host_of(server.addr);
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(
            ws_request(
                WS_PATH,
                &host,
                Some(WS_SUBPROTOCOL),
                Some("permessage-deflate; client_max_window_bits"),
            )
            .as_bytes(),
        )
        .await
        .expect("写升级请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        400
    );
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn repeated_subprotocol_headers_follow_the_token_rule() {
    // [R8]（N3）：同一 token 拆成两个 `Sec-WebSocket-Protocol` 头时锁定本层行为。
    //
    // 本层与 `axum::extract::ws::WebSocketUpgrade` 用同一口径（都是 `get_all(...)` → 按逗号切分 →
    // trim → 精确匹配），因此结论是**接受**而不是拒绝：任意一个（或两个）头值里以 token 形式出现约定
    // subprotocol 即可升级，且 101 仍然回填该 token。若哪天上游变成「只读第一个头值」，第二个头携带
    // token 的情形就不会回填，本用例会失败——那时必须改成「本层也拒绝双头」的失败关闭口径，而不是
    // 让「校验通过但响应没回填」的不一致静默存在。
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_ws(
                WS_PATH,
                WS_SUBPROTOCOL,
                Arc::new(IdleWsHandler {
                    finished: Arc::new(AtomicUsize::new(0)),
                }),
            )
            .expect("注册 WS");
    })
    .await;
    let host = host_of(server.addr);
    let two_headers = |first: &str, second: &str| {
        ws_request_with_headers(
            WS_PATH,
            &host,
            &[
                ("Sec-WebSocket-Protocol", first),
                ("Sec-WebSocket-Protocol", second),
            ],
        )
    };
    for (first, second) in [
        (WS_SUBPROTOCOL, "other"),
        ("other", WS_SUBPROTOCOL),
        ("other, other2", WS_SUBPROTOCOL),
    ] {
        let mut connection = connect(server.addr).await.expect("连接");
        connection
            .write_all(two_headers(first, second).as_bytes())
            .await
            .expect("写升级请求");
        let response = connection.read_response().await.expect("读升级响应");
        assert_eq!(response.status, 101, "{first:?} + {second:?} 必须升级成功");
        assert_eq!(
            response.header("sec-websocket-protocol"),
            Some(WS_SUBPROTOCOL),
            "{first:?} + {second:?} 的 101 必须回填约定的 subprotocol"
        );
    }
    // 两个头都不含约定 token（即使其中一个头里带逗号列表）一律 400。
    for (first, second) in [
        ("other", "acp-remote.nodelink.v2.json"),
        ("other, other2", "acp-remote.nodelink.v2.json"),
    ] {
        let mut connection = connect(server.addr).await.expect("连接");
        connection
            .write_all(two_headers(first, second).as_bytes())
            .await
            .expect("写升级请求");
        assert_eq!(
            connection.read_response().await.expect("读响应").status,
            400,
            "{first:?} + {second:?} 必须被拒绝"
        );
    }
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn plain_get_on_the_ws_path_is_rejected() {
    // [R8]：非升级请求对 WS path 返回明确 4xx（axum 的升级提取器给出 400/405/426）。
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_ws(
                WS_PATH,
                WS_SUBPROTOCOL,
                Arc::new(IdleWsHandler {
                    finished: Arc::new(AtomicUsize::new(0)),
                }),
            )
            .expect("注册 WS");
    })
    .await;
    let host = host_of(server.addr);
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(&http_request("GET", WS_PATH, &host, &[], b""))
        .await
        .expect("写请求");
    let response = connection.read_response().await.expect("读响应");
    assert!(
        (400..500).contains(&response.status),
        "非升级请求必须 4xx，实际 {}",
        response.status
    );
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn wrong_host_is_rejected_before_routing() {
    // [R9]/[R10]：Host 不匹配返回明确错误，且不进入任何 path 的处理逻辑（含未注册 path：先 400 而不是 404）。
    let calls = Arc::new(AtomicUsize::new(0));
    let server = start_server(
        NetConfig {
            public_origin: Some("https://owner.example.com".to_owned()),
            ..loopback_config()
        },
        |listener| {
            listener
                .register_post(
                    CLAIM_PATH,
                    Arc::new(RecordingHttpHandler {
                        calls: Arc::clone(&calls),
                        last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                        status: StatusCode::OK,
                    }),
                )
                .expect("注册 claim");
        },
    )
    .await;
    for path in [CLAIM_PATH, "/unknown"] {
        let mut connection = connect(server.addr).await.expect("连接");
        connection
            .write_all(&http_request("POST", path, "evil.example.com", &[], b"{}"))
            .await
            .expect("写请求");
        assert_eq!(
            connection.read_response().await.expect("读响应").status,
            400,
            "{path} 上的错误 Host 必须 400"
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0, "处理器不得被调用");
    // 与 `public_origin` 一致的 Host（忽略端口）被接受。
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(&http_request(
            "POST",
            CLAIM_PATH,
            "owner.example.com",
            &[],
            b"{}",
        ))
        .await
        .expect("写请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        200
    );
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn wrong_host_is_rejected_on_the_ws_upgrade_path() {
    // [R9]/[R10]（N2）：Host 边界同样覆盖 `/node-link/v1` 的升级路径——先 400，且 WS 处理器零调用。
    let calls = Arc::new(AtomicUsize::new(0));
    let server = start_server(
        NetConfig {
            public_origin: Some("https://owner.example.com".to_owned()),
            ..loopback_config()
        },
        |listener| {
            listener
                .register_ws(
                    WS_PATH,
                    WS_SUBPROTOCOL,
                    Arc::new(CountingWsHandler {
                        calls: Arc::clone(&calls),
                    }),
                )
                .expect("注册 WS");
        },
    )
    .await;
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(ws_request(WS_PATH, "evil.example.com", Some(WS_SUBPROTOCOL), None).as_bytes())
        .await
        .expect("写升级请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        400,
        "升级路径上的错误 Host 必须 400"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "WS 处理器不得被调用");
    // 与 `public_origin` 一致的 Host 才能升级成功，因此上一条确实是 Host 判定而非升级路径本身失败。
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(ws_request(WS_PATH, "owner.example.com", Some(WS_SUBPROTOCOL), None).as_bytes())
        .await
        .expect("写升级请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        101
    );
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn allowed_hosts_whitelist_is_used_when_configured() {
    // [R9]：`allowed_hosts` 非空时以白名单为准（此时 `public_origin` 不参与判定）。
    let server = start_server(
        NetConfig {
            public_origin: Some("https://owner.example.com".to_owned()),
            allowed_hosts: vec!["proxy.internal".to_owned()],
            ..loopback_config()
        },
        |listener| {
            listener
                .register_post(
                    CLAIM_PATH,
                    Arc::new(RecordingHttpHandler {
                        calls: Arc::new(AtomicUsize::new(0)),
                        last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                        status: StatusCode::OK,
                    }),
                )
                .expect("注册 claim");
        },
    )
    .await;
    for (host, expected) in [
        ("proxy.internal", 200),
        ("proxy.internal:8443", 200),
        ("owner.example.com", 400),
        ("evil.example.com", 400),
    ] {
        let mut connection = connect(server.addr).await.expect("连接");
        connection
            .write_all(&http_request("POST", CLAIM_PATH, host, &[], b"{}"))
            .await
            .expect("写请求");
        assert_eq!(
            connection.read_response().await.expect("读响应").status,
            expected,
            "Host: {host}"
        );
    }
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn default_configuration_accepts_loopback_host_only() {
    // [R9]/[R10]：默认配置（无 public_origin、无 allowed_hosts）只接受 loopback 形态的 Host。
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_post(
                CLAIM_PATH,
                Arc::new(RecordingHttpHandler {
                    calls: Arc::new(AtomicUsize::new(0)),
                    last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                    status: StatusCode::OK,
                }),
            )
            .expect("注册 claim");
    })
    .await;
    let default_host = host_of(server.addr);
    for (host, expected) in [
        (default_host.as_str(), 200),
        ("127.0.0.1:1", 200),
        ("localhost:8765", 200),
        ("evil.example.com", 400),
        ("owner.example.com", 400),
    ] {
        let mut connection = connect(server.addr).await.expect("连接");
        connection
            .write_all(&http_request("POST", CLAIM_PATH, host, &[], b"{}"))
            .await
            .expect("写请求");
        assert_eq!(
            connection.read_response().await.expect("读响应").status,
            expected,
            "Host: {host}"
        );
    }
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn forwarded_headers_are_ignored_from_untrusted_peers() {
    // [R11]：直连客户端（不在 trusted_proxies 内）的 X-Forwarded-For 被忽略，限流/日志用真实对端地址。
    let last_client_ip = Arc::new(std::sync::Mutex::new(None));
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_post(
                CLAIM_PATH,
                Arc::new(RecordingHttpHandler {
                    calls: Arc::new(AtomicUsize::new(0)),
                    last_client_ip: Arc::clone(&last_client_ip),
                    status: StatusCode::OK,
                }),
            )
            .expect("注册 claim");
    })
    .await;
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(&http_request(
            "POST",
            CLAIM_PATH,
            &host_of(server.addr),
            &[("x-forwarded-for", "203.0.113.9")],
            b"{}",
        ))
        .await
        .expect("写请求");
    let response = connection.read_response().await.expect("读响应");
    assert_eq!(response.status, 200);
    assert_eq!(response.body_text(), "127.0.0.1");
    assert_eq!(
        *last_client_ip.lock().expect("锁可用"),
        Some("127.0.0.1".parse().expect("合法地址"))
    );
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn forwarded_headers_are_honored_from_trusted_proxies() {
    // [R11]：对端在 trusted_proxies 内时才采信转发头（同机反代形态）。
    let server = start_server(
        NetConfig {
            trusted_proxies: vec!["127.0.0.1".to_owned()],
            ..loopback_config()
        },
        |listener| {
            listener
                .register_post(
                    CLAIM_PATH,
                    Arc::new(RecordingHttpHandler {
                        calls: Arc::new(AtomicUsize::new(0)),
                        last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                        status: StatusCode::OK,
                    }),
                )
                .expect("注册 claim");
        },
    )
    .await;
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(&http_request(
            "POST",
            CLAIM_PATH,
            &host_of(server.addr),
            &[("x-forwarded-for", "203.0.113.9, 127.0.0.1")],
            b"{}",
        ))
        .await
        .expect("写请求");
    let response = connection.read_response().await.expect("读响应");
    assert_eq!(response.status, 200);
    assert_eq!(response.body_text(), "203.0.113.9");
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn pairing_body_over_the_limit_is_rejected_with_413() {
    // [R16]/[R17]：请求体超限返回 413，且不解析内容、不调用处理器（§13.4）。
    let calls = Arc::new(AtomicUsize::new(0));
    let server = start_server(
        NetConfig {
            max_body_bytes: 1024,
            ..loopback_config()
        },
        |listener| {
            listener
                .register_post(
                    CLAIM_PATH,
                    Arc::new(RecordingHttpHandler {
                        calls: Arc::clone(&calls),
                        last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                        status: StatusCode::CREATED,
                    }),
                )
                .expect("注册 claim");
        },
    )
    .await;
    let body = vec![b'a'; 4096];
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(&http_request(
            "POST",
            CLAIM_PATH,
            &host_of(server.addr),
            &[],
            &body,
        ))
        .await
        .expect("写请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        413
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "处理器不得被调用");
    // 上限以内的请求正常进入处理器。
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(&http_request(
            "POST",
            CLAIM_PATH,
            &host_of(server.addr),
            &[],
            b"{}",
        ))
        .await
        .expect("写请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        201
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn oversize_ws_message_closes_with_1009() {
    // [R18]：超过 maxMessageBytes 的消息按 1009 拒绝，且处理器拿不到这条消息。
    let messages = Arc::new(AtomicUsize::new(0));
    let server = start_server(
        NetConfig {
            max_message_bytes: 4096,
            ..loopback_config()
        },
        |listener| {
            listener
                .register_ws(
                    WS_PATH,
                    WS_SUBPROTOCOL,
                    Arc::new(EchoWsHandler {
                        messages: Arc::clone(&messages),
                        last_binary: Arc::new(std::sync::Mutex::new(None)),
                        subprotocol: Arc::new(std::sync::Mutex::new(None)),
                    }),
                )
                .expect("注册 WS");
        },
    )
    .await;
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(
            ws_request(WS_PATH, &host_of(server.addr), Some(WS_SUBPROTOCOL), None).as_bytes(),
        )
        .await
        .expect("写升级请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        101
    );
    let oversize = vec![b'x'; 8192];
    connection
        .send_text(&String::from_utf8(oversize).expect("ASCII"))
        .await
        .expect("发超限消息");
    let frame = connection.read_frame().await.expect("读 close 帧");
    assert_eq!(frame.opcode, 0x8, "必须是 close 帧");
    assert_eq!(frame.close_code(), Some(1009));
    assert_eq!(messages.load(Ordering::SeqCst), 0, "超限消息不得上送处理器");
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn binary_frames_are_passed_to_the_handler() {
    // [R16]：binary 帧原样上送（是否按 `link.error` 拒绝由 `server::node_link` 决定，§2.1）。
    let handler = Arc::new(EchoWsHandler {
        messages: Arc::new(AtomicUsize::new(0)),
        last_binary: Arc::new(std::sync::Mutex::new(None)),
        subprotocol: Arc::new(std::sync::Mutex::new(None)),
    });
    let last_binary = Arc::clone(&handler.last_binary);
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_ws(WS_PATH, WS_SUBPROTOCOL, handler)
            .expect("注册 WS");
    })
    .await;
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(
            ws_request(WS_PATH, &host_of(server.addr), Some(WS_SUBPROTOCOL), None).as_bytes(),
        )
        .await
        .expect("写升级请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        101
    );
    connection
        .send_binary(&[0x00, 0xff, 0x7f])
        .await
        .expect("发 binary");
    let frame = connection.read_frame().await.expect("收帧");
    assert_eq!(frame.opcode, 0x2);
    assert_eq!(frame.payload, vec![0x00, 0xff, 0x7f]);
    assert_eq!(
        last_binary.lock().expect("锁可用").as_deref(),
        Some([0x00, 0xff, 0x7f].as_slice())
    );
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn shutdown_drains_and_releases_the_listener() {
    // D11 的接入层部分：关闭后不再 accept，在途会话被排空/取消（无 detached task），端口已释放。
    let finished = Arc::new(AtomicUsize::new(0));
    let server = start_server(
        NetConfig {
            drain_grace: Duration::from_millis(200),
            ..loopback_config()
        },
        |listener| {
            listener
                .register_ws(
                    WS_PATH,
                    WS_SUBPROTOCOL,
                    Arc::new(IdleWsHandler {
                        finished: Arc::clone(&finished),
                    }),
                )
                .expect("注册 WS");
        },
    )
    .await;
    let addr = server.addr;
    let mut connection = connect(addr).await.expect("连接");
    connection
        .write_all(ws_request(WS_PATH, &host_of(addr), Some(WS_SUBPROTOCOL), None).as_bytes())
        .await
        .expect("写升级请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        101
    );
    server.stop().await.expect("关闭序列成功");
    // 会话已被取消（Drop 守卫计数），因此不存在无人持有的会话任务。
    wait_for_counter(&finished, 1).await;
    // 客户端可读到连接关闭（或读到错误），且新连接不再被接受。
    let _ = connection.read_frame().await;
    assert!(
        tokio::time::timeout(Duration::from_secs(5), tokio::net::TcpStream::connect(addr))
            .await
            .expect("连接尝试必须在上限内结束")
            .is_err(),
        "关闭后不得继续接受连接"
    );
}

#[tokio::test]
async fn direct_mode_terminates_tls_and_rejects_plaintext() {
    // [R12]/[R13]：direct 模式只接受 TLS 握手成功的连接。
    let certificate = TestCertificate::generate();
    let server = start_server(
        NetConfig {
            tls: TlsMode::Direct {
                cert_path: certificate.cert_path.clone(),
                key_path: certificate.key_path.clone(),
            },
            ..loopback_config()
        },
        |listener| {
            listener
                .register_post(
                    CLAIM_PATH,
                    Arc::new(RecordingHttpHandler {
                        calls: Arc::new(AtomicUsize::new(0)),
                        last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                        status: StatusCode::OK,
                    }),
                )
                .expect("注册 claim");
        },
    )
    .await;
    let mut connection = connect_tls(server.addr, client_tls_config(certificate.der.clone()))
        .await
        .expect("TLS 连接");
    connection
        .write_all(&http_request(
            "POST",
            CLAIM_PATH,
            &host_of(server.addr),
            &[],
            b"{}",
        ))
        .await
        .expect("写请求");
    let response = connection.read_response().await.expect("读响应");
    assert_eq!(response.status, 200);
    // 明文连接拿不到 HTTP 响应（TLS 握手失败即被放弃）。
    let mut plain = connect(server.addr).await.expect("TCP 连接");
    plain
        .write_all(&http_request(
            "POST",
            CLAIM_PATH,
            &host_of(server.addr),
            &[],
            b"{}",
        ))
        .await
        .expect("写明文请求");
    assert!(
        plain.read_response().await.is_err(),
        "明文连接不得建立可用会话"
    );
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn idle_tcp_connections_do_not_block_new_connections() {
    // [R12]（RV1-WP2-F3 回归）：`direct` 模式的 TLS 握手不占用 accept 关键路径——若干只建立 TCP、
    // 不发 ClientHello 的连接（无需任何凭据）不得把新连接的接入推迟一个握手超时（10 s）。
    let certificate = TestCertificate::generate();
    let server = start_server(
        NetConfig {
            tls: TlsMode::Direct {
                cert_path: certificate.cert_path.clone(),
                key_path: certificate.key_path.clone(),
            },
            ..loopback_config()
        },
        |listener| {
            listener
                .register_post(
                    CLAIM_PATH,
                    Arc::new(RecordingHttpHandler {
                        calls: Arc::new(AtomicUsize::new(0)),
                        last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                        status: StatusCode::OK,
                    }),
                )
                .expect("注册 claim");
        },
    )
    .await;
    // 4 条只连接不握手的连接：内联握手的实现会把 accept 循环先后卡在它们各一个握手超时上。
    let _idle: Vec<_> = {
        let mut idle = Vec::new();
        for _ in 0..4 {
            idle.push(connect(server.addr).await.expect("TCP 连接"));
        }
        idle
    };
    // 新连接必须在远小于握手超时的时限内完成 TLS 握手并发出一条 HTTP 请求。
    let mut connection = tokio::time::timeout(
        Duration::from_secs(5),
        connect_tls(server.addr, client_tls_config(certificate.der.clone())),
    )
    .await
    .expect("只连接不握手的对端不得阻塞新连接的接入")
    .expect("TLS 连接");
    connection
        .write_all(&http_request(
            "POST",
            CLAIM_PATH,
            &host_of(server.addr),
            &[],
            b"{}",
        ))
        .await
        .expect("写请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        200
    );
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn direct_mode_does_not_warn_about_plaintext() {
    // [R12]：direct 模式不产生「明文面」告警（只有 loopback 监听本身也不告警）。
    let certificate = TestCertificate::generate();
    let listener = NetListener::bind(NetConfig {
        tls: TlsMode::Direct {
            cert_path: certificate.cert_path.clone(),
            key_path: certificate.key_path.clone(),
        },
        ..loopback_config()
    })
    .await
    .expect("direct 模式绑定成功");
    if cfg!(unix) {
        assert!(listener.warnings().is_empty());
    } else {
        // Windows：ACL 不可核验 → 只应出现「权限未核验」告警，不该出现明文面告警。
        assert!(
            listener
                .warnings()
                .iter()
                .all(|warning| matches!(warning, ListenerWarning::PermissionsUnverifiable { .. }))
        );
    }
}

#[tokio::test]
async fn direct_mode_missing_certificate_fails_closed() {
    // [R14]：证书/私钥缺失即拒绝启动，不降级为明文监听。
    let missing = std::env::temp_dir().join(format!(
        "acpr-net-missing-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let error = NetListener::bind(NetConfig {
        tls: TlsMode::Direct {
            cert_path: missing.clone(),
            key_path: missing,
        },
        ..loopback_config()
    })
    .await
    .expect_err("证书缺失必须失败关闭");
    assert!(matches!(error, NetError::TlsFileUnreadable { .. }));
}

#[tokio::test]
async fn direct_mode_invalid_pem_fails_closed() {
    // [R14]：PEM 解析失败即拒绝启动，错误消息不含文件内容。
    let certificate = TestCertificate::generate();
    std::fs::write(
        &certificate.key_path,
        b"-----BEGIN PRIVATE KEY-----\nsecret\n",
    )
    .expect("写入非法 PEM");
    set_owner_only(&certificate.key_path);
    let error = NetListener::bind(NetConfig {
        tls: TlsMode::Direct {
            cert_path: certificate.cert_path.clone(),
            key_path: certificate.key_path.clone(),
        },
        ..loopback_config()
    })
    .await
    .expect_err("非法 PEM 必须失败关闭");
    assert!(matches!(error, NetError::TlsPemInvalid { .. }));
    assert!(!error.to_string().contains("secret"));
}

#[cfg(unix)]
#[tokio::test]
async fn direct_mode_relaxed_permissions_fail_closed() {
    // [R14]：Unix 上宽松权限（组/其他可读）即拒绝启动（`SECURITY_DESIGN.md` §13.2）。
    use std::os::unix::fs::PermissionsExt as _;
    let certificate = TestCertificate::generate();
    std::fs::set_permissions(
        &certificate.key_path,
        std::fs::Permissions::from_mode(0o644),
    )
    .expect("chmod 0644");
    let error = NetListener::bind(NetConfig {
        tls: TlsMode::Direct {
            cert_path: certificate.cert_path.clone(),
            key_path: certificate.key_path.clone(),
        },
        ..loopback_config()
    })
    .await
    .expect_err("宽松权限必须失败关闭");
    assert!(matches!(
        error,
        NetError::TlsInsecurePermissions {
            role: TlsFile::PrivateKey,
            ..
        }
    ));
    // 证书文件同样判定。
    std::fs::set_permissions(
        &certificate.key_path,
        std::fs::Permissions::from_mode(0o600),
    )
    .expect("chmod 0600");
    std::fs::set_permissions(
        &certificate.cert_path,
        std::fs::Permissions::from_mode(0o640),
    )
    .expect("chmod 0640");
    let error = NetListener::bind(NetConfig {
        tls: TlsMode::Direct {
            cert_path: certificate.cert_path.clone(),
            key_path: certificate.key_path.clone(),
        },
        ..loopback_config()
    })
    .await
    .expect_err("证书文件权限宽松也必须失败关闭");
    assert!(matches!(
        error,
        NetError::TlsInsecurePermissions {
            role: TlsFile::Certificate,
            ..
        }
    ));
}

#[cfg(not(unix))]
#[tokio::test]
async fn direct_mode_permissions_are_unverifiable_on_this_platform() {
    // [R14]（`[PV5]` 的本机轮次）：Windows 上 ACL 读取需要不安全代码，因此判定是「未核验 + 警告」，
    // 既不失败关闭也不声称已核验（`CORE_PORTS_AND_STORAGE.md` §7.1 的已裁定口径）。
    let certificate = TestCertificate::generate();
    let listener = NetListener::bind(NetConfig {
        tls: TlsMode::Direct {
            cert_path: certificate.cert_path.clone(),
            key_path: certificate.key_path.clone(),
        },
        ..loopback_config()
    })
    .await
    .expect("权限不可核验时仍可启动（只告警）");
    assert_eq!(listener.warnings().len(), 2, "证书与私钥各一条未核验告警");
    assert!(
        listener
            .warnings()
            .iter()
            .all(|warning| matches!(warning, ListenerWarning::PermissionsUnverifiable { .. }))
    );
    // 明确记录：本平台**没有**核验过权限，报告里不能写成已通过。
    assert!(
        listener
            .warnings()
            .iter()
            .all(|warning| warning.to_string().contains("未核验"))
    );
}
