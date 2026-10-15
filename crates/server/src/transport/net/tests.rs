//! `transport::net` 的行为用例（`[PV3]` 的 WP2 部分）。
//!
//! 覆盖映射（Coverage Index / `specs/node-link-listener/spec.md`）：
//!
//! | 场景 | 用例 |
//! |---|---|
//! | 共享 listener 绑定与失败关闭 | `local_addrs_reports_the_real_bound_address`、`bind_fails_closed_when_the_address_is_taken` |
//! | 默认 loopback 启动 | `default_config_binds_only_loopback`（默认值的权威断言在 `config` 模块；真实 8765 端口的端到端绑定由 WP7 的 `[PV4]` 覆盖） |
//! | 绑定失败即拒绝启动 | `bind_fails_closed_when_the_address_is_taken` |
//! | 非 loopback 监听告警 | `non_loopback_listen_warns_without_failing` |
//! | 按 path 路由：协议 path 与协议族子路径明确拒绝 | `production_shaped_listener_keeps_protocol_rejections_and_serves_static_paths` |
//! | 静态面：协议端点优先于静态回退（`ui-asset-hosting`「协议端点优先于静态回退」） | `protocol_endpoints_win_over_the_static_fallback`、`unregistered_protocol_subpaths_stay_rejected_under_the_static_fallback` |
//! | 静态面：根路径入口与单页回退 | `root_path_serves_the_entry_document`、`frontend_deep_links_fall_back_to_the_entry_document` |
//! | 静态面：构建产物的内容类型（`ui-asset-hosting`「内容类型与安全响应头」） | `built_assets_are_served_with_matching_content_types` |
//! | 静态面：CSP 与接入层安全响应头 | `static_responses_carry_the_csp_baseline_and_the_access_layer_security_headers` |
//! | 静态面：非读取方法明确拒绝（`ui-asset-hosting`「请求方法不匹配」） | `non_read_methods_on_static_paths_are_explicitly_rejected` |
//! | Node Link 端点正常升级 | `ws_upgrade_with_the_required_subprotocol_succeeds` |
//! | 升级来源（`Origin`）的取值与归一化（`SYNC_PROTOCOL.md` §3.2） | `ws_upgrade_reports_the_normalized_origin_to_the_handler`、`ws_upgrade_without_origin_reports_absent`、`ws_upgrade_with_a_malformed_origin_is_rejected` |
//! | 压缩/错误 subprotocol 被拒绝 | `ws_upgrade_without_the_subprotocol_is_rejected`、`ws_upgrade_with_compression_is_rejected`、`plain_get_on_the_ws_path_is_rejected`、`repeated_subprotocol_headers_follow_the_token_rule` |
//! | Host 与代理头边界 | `host_policy_*`（`host` 模块）、`wrong_host_is_rejected_before_routing`、`wrong_host_is_rejected_on_the_ws_upgrade_path`、`allowed_hosts_whitelist_is_used_when_configured`、`default_configuration_accepts_loopback_host_only` |
//! | 不可信来源的转发头被忽略 | `forwarded_headers_are_ignored_from_untrusted_peers`、`forwarded_headers_are_honored_from_trusted_proxies` |
//! | TLS 两种模式 | `direct_mode_terminates_tls_and_rejects_plaintext`、`direct_mode_does_not_warn_about_plaintext`、`idle_tcp_connections_do_not_block_new_connections`（回归）、`saturated_handshake_pool_waits_for_a_slot_instead_of_dropping_new_connections`（回归） |
//! | direct 模式证书缺失即拒绝启动 | `direct_mode_missing_certificate_fails_closed`、`direct_mode_invalid_pem_fails_closed`、`direct_mode_relaxed_permissions_fail_closed`（Unix）、`direct_mode_permissions_are_unverifiable_on_this_platform`（Windows，`[PV5]`） |
//! | 非 loopback 明文边界 | `plaintext_proxy_non_loopback_warns`、`plaintext_dev_flag_rejects_non_loopback`（`listener` 模块） |
//! | 请求体上限 | `pairing_body_over_the_limit_is_rejected_with_413` |
//! | 每路径默认响应头（`register_post` 的第三个参数） | `path_default_headers_cover_access_layer_rejections_and_keep_route_isolation`、`path_default_headers_do_not_override_or_duplicate_handler_headers`、`invalid_path_default_headers_are_rejected_at_registration` |
//! | 超大 WebSocket 消息被拒绝 | `oversize_ws_message_closes_with_1009` |
//! | 关闭排空（D11） | `shutdown_drains_and_releases_the_listener`、`binary_frames_are_passed_to_the_handler` |

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::body::Bytes;
use axum::http::HeaderName;
use axum::http::HeaderValue;
use axum::http::StatusCode;

use super::test_client::RunningServer;
use super::test_client::TestCertificate;
use super::test_client::TestResponse;
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
/// Sync 协议族冻结的 WSS path 与 subprotocol（`SYNC_PROTOCOL.md` §3）；同上，本模块只作为参数接收它们。
const SYNC_WS_PATH: &str = "/sync/v1";
/// 同上。
const SYNC_WS_SUBPROTOCOL: &str = "acp-remote.sync.v1.json";
/// Sync 配对 claim 端点（`SYNC_PROTOCOL.md` §7）。
const SYNC_CLAIM_PATH: &str = "/sync/v1/pairing/claim";
/// Sync 配对 status 端点（`SYNC_PROTOCOL.md` §7）。
const SYNC_STATUS_PATH: &str = "/sync/v1/pairing/status";

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
    async fn handle(
        self: Arc<Self>,
        mut connection: WsConnection,
        _peer: PeerInfo,
        _origin: RequestOrigin,
    ) {
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
    async fn handle(
        self: Arc<Self>,
        _connection: WsConnection,
        _peer: PeerInfo,
        _origin: RequestOrigin,
    ) {
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
    async fn handle(
        self: Arc<Self>,
        mut connection: WsConnection,
        _peer: PeerInfo,
        _origin: RequestOrigin,
    ) {
        let _guard = SessionGuard(Arc::clone(&self.finished));
        while let Some(message) = connection.recv().await {
            if message.is_err() {
                return;
            }
        }
    }
}

/// 把接入选层交给处理器的**来源**逐条存档的 WS 处理器（`docs/SYNC_PROTOCOL.md` §3.2）。
///
/// 记录后保持连接直到对端关闭：处理器立刻返回会让「101 已发出」与「连接被关闭」竞争，用例就无法稳定地
/// 断言升级结果。
struct OriginWsHandler {
    origins: Arc<std::sync::Mutex<Vec<RequestOrigin>>>,
}

#[async_trait::async_trait]
impl WsHandler for OriginWsHandler {
    async fn handle(
        self: Arc<Self>,
        mut connection: WsConnection,
        _peer: PeerInfo,
        origin: RequestOrigin,
    ) {
        self.origins.lock().expect("锁可用").push(origin);
        while let Some(message) = connection.recv().await {
            if message.is_err() {
                return;
            }
        }
    }
}

/// 等待处理器至少记录了 `expected` 条来源，并返回当次快照。
async fn recorded_origins(
    origins: &Arc<std::sync::Mutex<Vec<RequestOrigin>>>,
    expected: usize,
) -> Vec<RequestOrigin> {
    for _ in 0..300 {
        let ready = {
            let recorded = origins.lock().expect("锁可用");
            if recorded.len() >= expected {
                Some(recorded.clone())
            } else {
                None
            }
        };
        if let Some(recorded) = ready {
            return recorded;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("等待 {expected} 条来源记录超时");
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

/// 前端入口文档：静态面用例的替身产物（真实构建由前端工程产出）。
const ENTRY_DOCUMENT: &str = "<!doctype html><html><body>acp-remote</body></html>";

/// 静态面用例的内存资源源：WP1 的替身（WP2 用嵌入资源表实现同一个 trait）。
///
/// 键是构建产物里的**根相对绝对路径**（与 [`UiAssetSource`] 的口径一致），只放用例要断言的几类文件。
struct MemoryAssets {
    assets: BTreeMap<String, &'static str>,
}

impl MemoryAssets {
    /// 与生产产物同形的清单：脚本（含 source map）、样式、图片、字体、PWA 清单、未知扩展名各一份。
    fn fixture() -> Self {
        let mut assets = BTreeMap::new();
        for (path, body) in [
            ("/_expo/static/js/web/entry-abc.js", "console.log('entry');"),
            ("/_expo/static/js/web/entry-abc.js.map", "{\"version\":3}"),
            ("/assets/app.css", "body{margin:0}"),
            ("/assets/logo.svg", "<svg/>"),
            ("/assets/icon.png", "PNGDATA"),
            ("/assets/font.woff2", "WOFF2"),
            ("/manifest.webmanifest", "{\"name\":\"acp-remote\"}"),
            ("/assets/blob.dat", "RAW"),
        ] {
            assets.insert(path.to_owned(), body);
        }
        Self { assets }
    }
}

impl UiAssetSource for MemoryAssets {
    fn entry_document(&self) -> Bytes {
        Bytes::from_static(ENTRY_DOCUMENT.as_bytes())
    }

    fn asset(&self, path: &str) -> Option<Bytes> {
        self.assets
            .get(path)
            .map(|body| Bytes::from_static(body.as_bytes()))
    }
}

/// 回一个可识别正文的 HTTP 处理器：用来证明请求真的交给了协议处理器，而不是被静态回退接管。
struct MarkerHttpHandler {
    marker: &'static str,
}

#[async_trait::async_trait]
impl HttpHandler for MarkerHttpHandler {
    async fn handle(&self, _request: HttpRequest) -> HttpResponse {
        HttpResponse::new(StatusCode::OK).with_body(Bytes::from_static(self.marker.as_bytes()))
    }
}

/// 与生产组合根同形的注册：两个协议族的 WS 端点与配对 POST，再加上根路径的静态面。
///
/// 静态面是**显式注册**的（`register_static`），因此「协议端点优先」与「未注册协议子路径仍被拒绝」
/// 这两条都在真实形态下被验证，而不是在一个「没有静态面」的旧形态下被验证。
fn register_production_shape(listener: &mut NetListener) {
    for (path, subprotocol) in [
        (WS_PATH, WS_SUBPROTOCOL),
        (SYNC_WS_PATH, SYNC_WS_SUBPROTOCOL),
    ] {
        listener
            .register_ws(
                path,
                subprotocol,
                Arc::new(IdleWsHandler {
                    finished: Arc::new(AtomicUsize::new(0)),
                }),
            )
            .expect("注册 WS");
    }
    for (path, marker) in [
        (CLAIM_PATH, "claim"),
        (STATUS_PATH, "status"),
        (SYNC_CLAIM_PATH, "sync-claim"),
        (SYNC_STATUS_PATH, "sync-status"),
    ] {
        listener
            .register_post(path, Arc::new(MarkerHttpHandler { marker }), &[])
            .expect("注册配对 POST");
    }
    listener
        .register_static("/", Arc::new(MemoryAssets::fixture()))
        .expect("注册静态面");
}

/// 只注册静态面（协议端点一律不注册）：验证「协议面缺席时静态面自己也不会越界」。
fn register_static_only(listener: &mut NetListener) {
    listener
        .register_static("/", Arc::new(MemoryAssets::fixture()))
        .expect("注册静态面");
}

#[tokio::test]
async fn local_addrs_reports_the_real_bound_address() {
    // 绑定随机 loopback 端口后，`daemon.status.listen` 的来源就是实际地址（不是配置里的 0）。
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_post(
                CLAIM_PATH,
                Arc::new(RecordingHttpHandler {
                    calls: Arc::new(AtomicUsize::new(0)),
                    last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                    status: StatusCode::OK,
                }),
                &[],
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
    // 默认配置只落在 loopback（`CONFIG_REFERENCE.md` §1 的默认值断言在 `config` 模块）。
    let config = NetConfig::default();
    assert_eq!(config.listen, "127.0.0.1:8765");
    let bound: SocketAddr = config.listen.parse().expect("默认值可解析");
    assert!(bound.ip().is_loopback());
}

#[tokio::test]
async fn bind_fails_closed_when_the_address_is_taken() {
    // 地址被占用即拒绝启动，且不留下半初始化的监听。
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
    // 非 loopback 是显式配置的后果，只告警不拒绝；`proxy` 模式再加一条明文面告警（D10）。
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
    // 非 loopback 明文只可能在显式 proxy 形态下成立，且必须告警。
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
async fn production_shaped_listener_keeps_protocol_rejections_and_serves_static_paths() {
    // **本用例是 `unregistered_paths_return_404` 的改写。** 旧断言（`/` 与 `/unknown` 必须 404）描述的是
    // **静态面缺席期**的行为：当时 `transport::net` 只有 ws/post 两类端点（`server::sync` 也没有在这个
    // listener 上注册），任何未注册 path 都只有 404 一条出路。`sync-ui-hosting` 落地第三类端点
    // （`register_static`）后 `/` 由入口文档接管、`/unknown` 由单页回退接管，因此这里让 listener 与生产
    // 同形（两个协议族的端点 + 根路径静态面）并把每个 path 的期望**显式重新判定**，而不是沿用旧结论：
    //
    // - 协议 path 与协议族下未注册的子路径 → 仍 404（接入层既有语义，未被静态回退接管）；
    // - `/`、前端深链、`/unknown`（不属于协议族）→ 入口文档（单页回退）；
    // - 构建产物中存在的资源 → 按路径返回该资源本身；
    // - 静态语义下的非读取方法 → 明确 405（不是 200 入口文档）。
    let server = start_server(loopback_config(), register_production_shape).await;
    let host = host_of(server.addr);

    // ① 协议 path 与协议族下未注册的子路径：仍 404，且不返回入口文档。这里同时覆盖非读取方法——
    //    保留前缀下的 POST 仍是**协议面**的 404，而不是静态面的 405。
    for path in [
        "/sync/v1/pairing/other",
        "/node-link/v1/pairing/other",
        "/node-link/v1/unknown",
    ] {
        for method in ["GET", "POST"] {
            let response = request(&server, method, path, &host, b"{}").await;
            assert_eq!(response.status, 404, "{method} {path}");
            assert_ne!(response.body_text(), ENTRY_DOCUMENT, "{method} {path}");
        }
    }

    // ② 非协议路径：`/`、前端深链与 `/unknown` 都回退到同一入口文档（旧断言里的 404 已不成立）。
    for path in ["/", "/pair", "/hosts/local/sessions/abc", "/unknown"] {
        let response = request(&server, "GET", path, &host, b"").await;
        assert_eq!(response.status, 200, "GET {path}");
        assert_eq!(response.body_text(), ENTRY_DOCUMENT, "GET {path}");
        assert!(
            response
                .header("content-type")
                .is_some_and(|value| value.starts_with("text/html")),
            "GET {path} 的内容类型必须是 HTML"
        );
    }

    // ③ 构建产物中存在的资源：按路径原样返回，不返回入口文档。
    let asset = request(&server, "GET", "/assets/app.css", &host, b"").await;
    assert_eq!(asset.status, 200);
    assert_eq!(asset.body_text(), "body{margin:0}");
    assert_eq!(
        asset.header("content-type"),
        Some("text/css; charset=utf-8")
    );

    // ④ 静态语义下的非读取方法：405 + `Allow`，不是 200 入口文档。
    for path in ["/", "/pair", "/unknown"] {
        let response = request(&server, "POST", path, &host, b"{}").await;
        assert_eq!(response.status, 405, "POST {path}");
        assert_eq!(response.header("allow"), Some("GET, HEAD"), "POST {path}");
        assert_ne!(response.body_text(), ENTRY_DOCUMENT, "POST {path}");
    }

    // ⑤ 已注册的协议端点未被回退吞掉：非升级请求打 WS path 仍是既有 4xx，配对 POST 仍由各自处理器接管。
    let plain_upgrade = request(&server, "GET", WS_PATH, &host, b"").await;
    assert!(
        (400..500).contains(&plain_upgrade.status),
        "非升级请求必须 4xx，实际 {}",
        plain_upgrade.status
    );
    assert_ne!(plain_upgrade.body_text(), ENTRY_DOCUMENT);
    let claim = request(&server, "POST", CLAIM_PATH, &host, b"{}").await;
    assert_eq!(claim.status, 200);
    assert_eq!(claim.body_text(), "claim");
    let wrong_method = request(&server, "GET", CLAIM_PATH, &host, b"").await;
    assert_eq!(wrong_method.status, 405);
    assert_ne!(wrong_method.body_text(), ENTRY_DOCUMENT);
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn protocol_endpoints_win_over_the_static_fallback() {
    // spec「协议端点不被回退吞掉」：静态面注册在根路径后，两个协议族的 WS 端点与四条配对 POST 仍由各自
    // 的协议逻辑接管，返回的结果 MUST NOT 是入口文档。WS 端点用**真实升级**（101）证明这一点，比断言
    // 「不是 200 HTML」更强。
    let server = start_server(loopback_config(), register_production_shape).await;
    let host = host_of(server.addr);

    for (path, subprotocol) in [
        (WS_PATH, WS_SUBPROTOCOL),
        (SYNC_WS_PATH, SYNC_WS_SUBPROTOCOL),
    ] {
        let mut connection = connect(server.addr).await.expect("连接");
        connection
            .write_all(ws_request(path, &host, Some(subprotocol), None).as_bytes())
            .await
            .expect("写升级请求");
        let upgrade = connection.read_response().await.expect("读升级响应");
        assert_eq!(upgrade.status, 101, "{path} 必须由 WS 端点接管");
        assert_eq!(upgrade.header("sec-websocket-protocol"), Some(subprotocol));
    }

    for (path, marker) in [
        (CLAIM_PATH, "claim"),
        (STATUS_PATH, "status"),
        (SYNC_CLAIM_PATH, "sync-claim"),
        (SYNC_STATUS_PATH, "sync-status"),
    ] {
        let response = request(&server, "POST", path, &host, b"{}").await;
        assert_eq!(response.status, 200, "POST {path}");
        assert_eq!(response.body_text(), marker, "POST {path}");
        assert_ne!(response.body_text(), ENTRY_DOCUMENT, "POST {path}");
    }

    // 非 POST 打配对 path：`axum` 的方法路由给 405，不会被回退改成入口文档。
    let wrong_method = request(&server, "GET", SYNC_CLAIM_PATH, &host, b"").await;
    assert_eq!(wrong_method.status, 405);
    assert_ne!(wrong_method.body_text(), ENTRY_DOCUMENT);
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn unregistered_protocol_subpaths_stay_rejected_under_the_static_fallback() {
    // spec「未注册的协议子路径不被回退接管」：保留前缀由**已注册 path 的前两段**派生
    // （两个协议族都在第 2 段带版本号），因此协议族下未注册的子路径仍按接入层既有语义 404，
    // 不会被静态回退接管成入口文档或构建产物。
    let server = start_server(loopback_config(), register_production_shape).await;
    let host = host_of(server.addr);
    for path in [
        "/sync/v1/pairing/other",
        "/sync/v1/sessions",
        "/node-link/v1/pairing/other",
        "/node-link/v1/unknown",
        "/node-link/v1/",
    ] {
        let response = request(&server, "GET", path, &host, b"").await;
        assert_eq!(response.status, 404, "GET {path}");
        assert_ne!(response.body_text(), ENTRY_DOCUMENT, "GET {path}");
    }
    // 与保留前缀共享字符串但不在其「之下」的 path 不属于协议族：它按静态语义回退。
    let sibling = request(&server, "GET", "/sync/v1extra", &host, b"").await;
    assert_eq!(sibling.status, 200);
    assert_eq!(sibling.body_text(), ENTRY_DOCUMENT);
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn root_path_serves_the_entry_document() {
    // spec「根路径返回入口」：`GET /` 200 + HTML 且正文是入口文档；`HEAD` 属于读取方法（不 405）。
    let server = start_server(loopback_config(), register_static_only).await;
    let host = host_of(server.addr);
    let root = request(&server, "GET", "/", &host, b"").await;
    assert_eq!(root.status, 200);
    assert_eq!(root.body_text(), ENTRY_DOCUMENT);
    let content_type = root.header("content-type").expect("根路径必须带内容类型");
    assert!(content_type.starts_with("text/html"), "实际 {content_type}");

    let head = request(&server, "HEAD", "/", &host, b"").await;
    assert_eq!(head.status, 200);
    assert_eq!(head.header("content-type"), root.header("content-type"));
    assert!(head.body.is_empty(), "HEAD 只回头部");
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn frontend_deep_links_fall_back_to_the_entry_document() {
    // spec「前端路由深链可刷新」：构建产物里没有同名文件 → 返回同一入口文档而不是 404。
    let server = start_server(loopback_config(), register_static_only).await;
    let host = host_of(server.addr);
    for path in [
        "/pair",
        "/hosts",
        "/hosts/local/sessions/abc",
        "/assets/missing.png",
    ] {
        let response = request(&server, "GET", path, &host, b"").await;
        assert_eq!(response.status, 200, "GET {path}");
        assert_eq!(response.body_text(), ENTRY_DOCUMENT, "GET {path}");
        assert!(
            response
                .header("content-type")
                .is_some_and(|value| value.starts_with("text/html")),
            "GET {path} 的内容类型必须是 HTML"
        );
    }
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn built_assets_are_served_with_matching_content_types() {
    // spec「构建产物按路径返回」+「资源带正确类型」：逐类型断言，且脚本/样式不得被标成通用二进制
    // （`nosniff` 下那会让浏览器拒绝执行），未知扩展名不得误导浏览器按 HTML 执行。
    let server = start_server(loopback_config(), register_static_only).await;
    let host = host_of(server.addr);
    for (path, content_type, body) in [
        (
            "/_expo/static/js/web/entry-abc.js",
            "text/javascript; charset=utf-8",
            "console.log('entry');",
        ),
        (
            "/_expo/static/js/web/entry-abc.js.map",
            "application/json; charset=utf-8",
            "{\"version\":3}",
        ),
        (
            "/assets/app.css",
            "text/css; charset=utf-8",
            "body{margin:0}",
        ),
        ("/assets/logo.svg", "image/svg+xml", "<svg/>"),
        ("/assets/icon.png", "image/png", "PNGDATA"),
        ("/assets/font.woff2", "font/woff2", "WOFF2"),
        (
            "/manifest.webmanifest",
            "application/manifest+json; charset=utf-8",
            "{\"name\":\"acp-remote\"}",
        ),
        ("/assets/blob.dat", "application/octet-stream", "RAW"),
    ] {
        let response = request(&server, "GET", path, &host, b"").await;
        assert_eq!(response.status, 200, "{path}");
        assert_eq!(response.body_text(), body, "{path}");
        assert_eq!(
            response.header("content-type"),
            Some(content_type),
            "{path}"
        );
        assert_ne!(response.header("content-type"), Some("text/html"), "{path}");
    }
    for path in [
        "/_expo/static/js/web/entry-abc.js",
        "/assets/app.css",
        "/assets/icon.png",
    ] {
        let response = request(&server, "GET", path, &host, b"").await;
        assert_ne!(
            response.header("content-type"),
            Some(static_ui::FALLBACK_CONTENT_TYPE),
            "{path} 不得被标成通用二进制"
        );
    }
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn static_responses_carry_the_csp_baseline_and_the_access_layer_security_headers() {
    // spec「入口文档带 CSP 响应头」：CSP 由**响应头**下发（不是文档内的 `<meta>`），取值逐字等于
    // `docs/FRONTEND_DESIGN.md` §4.5 的基线；四个既有安全头取值与配对端点一致。
    const CSP_BASELINE: &str = "default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";
    let server = start_server(loopback_config(), register_static_only).await;
    let host = host_of(server.addr);
    for path in [
        "/",
        "/pair",
        "/_expo/static/js/web/entry-abc.js",
        "/assets/icon.png",
    ] {
        let response = request(&server, "GET", path, &host, b"").await;
        assert_eq!(response.status, 200, "{path}");
        assert_eq!(
            response.header("content-security-policy"),
            Some(CSP_BASELINE),
            "{path}"
        );
        assert_eq!(response.header("cache-control"), Some("no-store"), "{path}");
        assert_eq!(response.header("pragma"), Some("no-cache"), "{path}");
        assert_eq!(
            response.header("referrer-policy"),
            Some("no-referrer"),
            "{path}"
        );
        assert_eq!(
            response.header("x-content-type-options"),
            Some("nosniff"),
            "{path}"
        );
    }
    // 接入层在调用处理器前产生的响应同样带安全头：Host 边界在静态 path 上的 400。
    let wrong_host = request(&server, "GET", "/", "evil.example.com", b"").await;
    assert_eq!(wrong_host.status, 400);
    assert_eq!(
        wrong_host.header("content-security-policy"),
        Some(CSP_BASELINE)
    );
    assert_eq!(wrong_host.header("x-content-type-options"), Some("nosniff"));
    // 用例里的字面量与实现常量必须是同一份（改动常量会让用例失败，而不是悄悄放行）。
    assert_eq!(static_ui::CONTENT_SECURITY_POLICY, CSP_BASELINE);
    // 基线不含放宽项（不是「包含某个子串就过」：六个指令的逐条比对在 `static_ui` 的单测里）。
    for forbidden in [
        "unsafe-inline",
        "unsafe-eval",
        "*",
        "http:",
        "data:",
        "https:",
    ] {
        assert!(!CSP_BASELINE.contains(forbidden), "基线不得含 {forbidden}");
    }
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn non_read_methods_on_static_paths_are_explicitly_rejected() {
    // spec「请求方法不匹配」：静态语义下的非 GET/HEAD 返回 405 + `Allow: GET, HEAD`，
    // 不得返回 200 入口文档冒充成功。
    let server = start_server(loopback_config(), register_static_only).await;
    let host = host_of(server.addr);
    for path in ["/", "/pair", "/assets/app.css", "/unknown"] {
        for method in ["POST", "PUT", "DELETE", "OPTIONS"] {
            let response = request(&server, method, path, &host, b"{}").await;
            assert_eq!(response.status, 405, "{method} {path}");
            assert_eq!(
                response.header("allow"),
                Some("GET, HEAD"),
                "{method} {path}"
            );
            assert_ne!(response.body_text(), ENTRY_DOCUMENT, "{method} {path}");
        }
    }
    // 保留前缀下的非读取请求仍是**协议面**语义的 404（不是静态面的 405）——该断言在
    // `production_shaped_listener_keeps_protocol_rejections_and_serves_static_paths` 里，因为只有注册了
    // 协议端点的 listener 才派生出保留前缀（这里是静态面单独在跑的形态）。
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn ws_upgrade_with_the_required_subprotocol_succeeds() {
    // 带约定 subprotocol、不请求压缩的升级成功，响应回填 subprotocol，处理器收到会话。
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
async fn ws_upgrade_reports_the_normalized_origin_to_the_handler() {
    // 带 `Origin` 头的升级照常成功，处理器收到的是按 `docs/SYNC_PROTOCOL.md` §3.2 归一化后的
    // canonical origin（小写 scheme、规范化 hostname、省略默认端口、不含 path/query/fragment）。
    let origins = Arc::new(std::sync::Mutex::new(Vec::new()));
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_ws(
                WS_PATH,
                WS_SUBPROTOCOL,
                Arc::new(OriginWsHandler {
                    origins: Arc::clone(&origins),
                }),
            )
            .expect("注册 WS");
    })
    .await;
    let host = host_of(server.addr);
    for (index, (sent, expected)) in [
        ("https://PC.Example.Test", "https://pc.example.test"),
        ("HTTPS://pc.example.test:0443", "https://pc.example.test"),
        ("http://127.0.0.1:80", "http://127.0.0.1"),
        (
            "https://pc.example.test:8443",
            "https://pc.example.test:8443",
        ),
        ("https://[::1]:9443", "https://[::1]:9443"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut connection = connect(server.addr).await.expect("连接");
        connection
            .write_all(
                ws_request_with_headers(
                    WS_PATH,
                    &host,
                    &[("Sec-WebSocket-Protocol", WS_SUBPROTOCOL), ("Origin", sent)],
                )
                .as_bytes(),
            )
            .await
            .expect("写升级请求");
        assert_eq!(
            connection.read_response().await.expect("读升级响应").status,
            101,
            "{sent} 必须升级成功"
        );
        // 逐条断言：等够 `index + 1` 条记录后，第 `index` 条必定是本次连接交给处理器的取值。
        let recorded = recorded_origins(&origins, index + 1).await;
        assert_eq!(
            recorded[index],
            RequestOrigin::Known(expected.to_owned()),
            "{sent} 的归一化取值"
        );
        assert_eq!(recorded[index].as_str(), Some(expected));
        drop(connection);
    }
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn ws_upgrade_without_origin_reports_absent() {
    // 非浏览器对端可以没有 `Origin` 头（§3.2）：升级照常成功，处理器收到「无来源」——这不是校验失败，
    // 也不是某个占位 origin；transcript 的空字节串由上层按此形态生成。
    let origins = Arc::new(std::sync::Mutex::new(Vec::new()));
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_ws(
                WS_PATH,
                WS_SUBPROTOCOL,
                Arc::new(OriginWsHandler {
                    origins: Arc::clone(&origins),
                }),
            )
            .expect("注册 WS");
    })
    .await;
    let host = host_of(server.addr);
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(ws_request(WS_PATH, &host, Some(WS_SUBPROTOCOL), None).as_bytes())
        .await
        .expect("写升级请求");
    assert_eq!(
        connection.read_response().await.expect("读升级响应").status,
        101
    );
    let recorded = recorded_origins(&origins, 1).await;
    assert_eq!(recorded, vec![RequestOrigin::Absent]);
    assert_eq!(recorded[0].as_str(), None);
    drop(connection);
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn ws_upgrade_with_a_malformed_origin_is_rejected() {
    // `Origin` 头存在但不可归一化（含 `null`、非 http(s) scheme、带 path/userinfo、非法端口）→ 结构化
    // 拒绝（400）：既不升级、也不调用处理器，更不降级成「无来源」（后者在协议里另有含义）。
    let origins = Arc::new(std::sync::Mutex::new(Vec::new()));
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_ws(
                WS_PATH,
                WS_SUBPROTOCOL,
                Arc::new(OriginWsHandler {
                    origins: Arc::clone(&origins),
                }),
            )
            .expect("注册 WS");
    })
    .await;
    let host = host_of(server.addr);
    for sent in [
        "null",
        "https://pc.example.test/",
        "https://pc.example.test/path",
        "https://user@pc.example.test",
        "ftp://pc.example.test",
        "wss://pc.example.test",
        "pc.example.test",
        "https://pc.example.test:8443:99",
        "https://pc.example.test:70000",
    ] {
        let mut connection = connect(server.addr).await.expect("连接");
        connection
            .write_all(
                ws_request_with_headers(
                    WS_PATH,
                    &host,
                    &[("Sec-WebSocket-Protocol", WS_SUBPROTOCOL), ("Origin", sent)],
                )
                .as_bytes(),
            )
            .await
            .expect("写升级请求");
        assert_eq!(
            connection.read_response().await.expect("读响应").status,
            400,
            "{sent} 必须被拒绝"
        );
    }
    // 重复的 `Origin` 头同样不合格（与 `Host` 边界同处置：不接受任何一个）。
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(
            ws_request_with_headers(
                WS_PATH,
                &host,
                &[
                    ("Sec-WebSocket-Protocol", WS_SUBPROTOCOL),
                    ("Origin", "https://pc.example.test"),
                    ("Origin", "https://evil.example.test"),
                ],
            )
            .as_bytes(),
        )
        .await
        .expect("写升级请求");
    assert_eq!(
        connection.read_response().await.expect("读响应").status,
        400
    );
    assert!(
        origins.lock().expect("锁可用").is_empty(),
        "非法来源不得进入处理器"
    );
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn ws_upgrade_without_the_subprotocol_is_rejected() {
    // 未声明约定 subprotocol 的升级被拒绝（不建立连接、不返回业务级错误消息）。
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
    // 协商到 permessage-deflate 的升级必须被拒绝（`NODE_LINK_PROTOCOL.md` §2.1）。
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
    // （N3）：同一 token 拆成两个 `Sec-WebSocket-Protocol` 头时锁定本层行为。
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
    // 非升级请求对 WS path 返回明确 4xx（axum 的升级提取器给出 400/405/426）。
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
    // Host 不匹配返回明确错误，且不进入任何 path 的处理逻辑（含未注册 path：先 400 而不是 404）。
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
                    &[],
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
    // （N2）：Host 边界同样覆盖 `/node-link/v1` 的升级路径——先 400，且 WS 处理器零调用。
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
    // `allowed_hosts` 非空时以白名单为准（此时 `public_origin` 不参与判定）。
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
                    &[],
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
    // 默认配置（无 public_origin、无 allowed_hosts）只接受 loopback 形态的 Host。
    let server = start_server(loopback_config(), |listener| {
        listener
            .register_post(
                CLAIM_PATH,
                Arc::new(RecordingHttpHandler {
                    calls: Arc::new(AtomicUsize::new(0)),
                    last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                    status: StatusCode::OK,
                }),
                &[],
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
    // 直连客户端（不在 trusted_proxies 内）的 X-Forwarded-For 被忽略，限流/日志用真实对端地址。
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
                &[],
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
    // 对端在 trusted_proxies 内时才采信转发头（同机反代形态）。
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
                    &[],
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
    // 请求体超限返回 413，且不解析内容、不调用处理器（§13.4）。
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
                    &[],
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

/// 发一条请求并读回响应（默认头用例的缩写）。
async fn request(
    server: &RunningServer,
    method: &str,
    path: &str,
    host: &str,
    body: &[u8],
) -> TestResponse {
    let mut connection = connect(server.addr).await.expect("连接");
    connection
        .write_all(&http_request(method, path, host, &[], body))
        .await
        .expect("写请求");
    connection.read_response().await.expect("读响应")
}

#[tokio::test]
async fn path_default_headers_cover_access_layer_rejections_and_keep_route_isolation() {
    // `register_post` 的每路径默认头：该 path 上的**所有**响应（含接入层在调用处理器前产生的 413/400）
    // 都补齐它们；未声明的 path 与未注册 path 的既有行为不变。
    const PAIRING_HEADERS: [(&str, &str); 2] = [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
    ];
    let calls = Arc::new(AtomicUsize::new(0));
    let handler = || {
        Arc::new(RecordingHttpHandler {
            calls: Arc::clone(&calls),
            last_client_ip: Arc::new(std::sync::Mutex::new(None)),
            status: StatusCode::OK,
        })
    };
    let server = start_server(
        NetConfig {
            public_origin: Some("https://owner.example.com".to_owned()),
            max_body_bytes: 1024,
            ..loopback_config()
        },
        |listener| {
            listener
                .register_post(CLAIM_PATH, handler(), &PAIRING_HEADERS)
                .expect("注册 claim");
            listener
                .register_post(STATUS_PATH, handler(), &[])
                .expect("注册 status");
        },
    )
    .await;
    let host = "owner.example.com";
    // ① 请求体超限：413 由接入层产生。
    let oversized = request(&server, "POST", CLAIM_PATH, host, &[b'a'; 4096]).await;
    assert_eq!(oversized.status, 413);
    assert_eq!(oversized.header("cache-control"), Some("no-store"));
    assert_eq!(oversized.header("x-content-type-options"), Some("nosniff"));
    // ② Host 边界在路由之前拒绝：400 也带默认头。
    let bad_host = request(&server, "POST", CLAIM_PATH, "evil.example.com", b"{}").await;
    assert_eq!(bad_host.status, 400);
    assert_eq!(bad_host.header("cache-control"), Some("no-store"));
    assert_eq!(bad_host.header("x-content-type-options"), Some("nosniff"));
    // ③ 处理器产生的响应同样补齐。
    let handled = request(&server, "POST", CLAIM_PATH, host, b"{}").await;
    assert_eq!(handled.status, 200);
    assert_eq!(handled.header("cache-control"), Some("no-store"));
    assert_eq!(handled.header("x-content-type-options"), Some("nosniff"));
    // ③' 方法不匹配（GET）：405 也由接入层产生（`axum` 的方法路由），同样补齐默认头。
    let wrong_method = request(&server, "GET", CLAIM_PATH, host, b"").await;
    assert_eq!(wrong_method.status, 405);
    assert_eq!(wrong_method.header("cache-control"), Some("no-store"));
    // ④ 未声明默认头的 path（空切片）与未注册 path 都不受影响。
    // 注意 `/unknown` 的 404 仍然成立：本用例的 listener **没有**注册静态端点（`register_static`），
    // fallback 因此还是接入层既有的 `not_found`——静态面的单页回退不在这里生效（语义已随
    // `sync-ui-hosting` 改变的同名断言在 `production_shaped_listener_keeps_protocol_rejections_and_serves_static_paths`）。
    for (path, expected) in [(STATUS_PATH, 200), ("/unknown", 404)] {
        let response = request(&server, "POST", path, host, b"{}").await;
        assert_eq!(response.status, expected, "{path}");
        assert_eq!(response.header("cache-control"), None, "{path}");
    }
    let wrong_method_other = request(&server, "GET", STATUS_PATH, host, b"").await;
    assert_eq!(wrong_method_other.status, 405);
    assert_eq!(wrong_method_other.header("cache-control"), None);
    // ⑤ 413 与 Host 400 都没有调用处理器。
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn path_default_headers_do_not_override_or_duplicate_handler_headers() {
    // 默认头是**补齐**语义：处理器已经设置同名头时保留处理器自己的值，也不产生重复头。
    struct HandlerWithHeader;

    #[async_trait::async_trait]
    impl HttpHandler for HandlerWithHeader {
        async fn handle(&self, _request: HttpRequest) -> HttpResponse {
            HttpResponse::new(StatusCode::OK).with_header(
                HeaderName::from_static("cache-control"),
                HeaderValue::from_static("private"),
            )
        }
    }

    let server = start_server(loopback_config(), |listener| {
        listener
            .register_post(
                CLAIM_PATH,
                Arc::new(HandlerWithHeader),
                &[("cache-control", "no-store"), ("pragma", "no-cache")],
            )
            .expect("注册 claim");
    })
    .await;
    let response = request(&server, "POST", CLAIM_PATH, &host_of(server.addr), b"{}").await;
    assert_eq!(response.status, 200);
    let values = response
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("cache-control"))
        .map(|(_, value)| value.as_str())
        .collect::<Vec<_>>();
    assert_eq!(values, vec!["private"], "处理器自己的值保留，且不重复");
    assert_eq!(response.header("pragma"), Some("no-cache"));
    server.stop().await.expect("关闭序列成功");
}

#[tokio::test]
async fn invalid_path_default_headers_are_rejected_at_registration() {
    // 头名/值非法即拒绝注册，而不是留到响应期才失败；被拒的声明不留下半注册的路由。
    let mut listener = NetListener::bind(loopback_config())
        .await
        .expect("绑定 loopback listener");
    let handler = || {
        Arc::new(RecordingHttpHandler {
            calls: Arc::new(AtomicUsize::new(0)),
            last_client_ip: Arc::new(std::sync::Mutex::new(None)),
            status: StatusCode::OK,
        })
    };
    for invalid in [
        &[("bad name", "value")][..],
        &[("cache-control", "bad\nvalue")][..],
    ] {
        let error = listener
            .register_post(CLAIM_PATH, handler(), invalid)
            .expect_err("非法默认头必须拒绝注册");
        assert!(
            matches!(error, RouteError::InvalidDefaultHeader { .. }),
            "{error}"
        );
    }
    listener
        .register_post(CLAIM_PATH, handler(), &[("cache-control", "no-store")])
        .expect("被拒的声明不得占用该 path");
}

#[tokio::test]
async fn oversize_ws_message_closes_with_1009() {
    // 超过 maxMessageBytes 的消息按 1009 拒绝，且处理器拿不到这条消息。
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
    // binary 帧原样上送（是否按 `link.error` 拒绝由 `server::node_link` 决定，§2.1）。
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
    // direct 模式只接受 TLS 握手成功的连接。
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
                    &[],
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
    // （回归）：`direct` 模式的 TLS 握手不占用 accept 关键路径——若干只建立 TCP、
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
                    &[],
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
async fn saturated_handshake_pool_waits_for_a_slot_instead_of_dropping_new_connections() {
    // （回归）：握手池被「只连接不握手」的连接**占满**时，新连接既不被丢弃、
    // 也不被拒绝——它等一个槽位，并在既有握手因超时归还槽位后照常完成 TLS 握手与请求。
    //
    // 用例按 `TLS_HANDSHAKE_POOL_SIZE` 精确填满池（不写死槽位数，容量调整时不会静默失去覆盖）。
    // 区分度：背压实现只等**一个**既有握手超时（时间与排队连接数无关）；在 accept 里顺序握手的
    // 老实现要等「池容量 × 单次握手超时」，因此 5 s 的上限能分开两者（注入的 500 ms × 64 = 32 s）。
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
            // 测试专用注入：把单次握手超时从生产的 10 s 缩到 500 ms，让「池满 → 等槽位 → 超时
            // 归还槽位」在用例时限内真实经过；公开语义与生产默认值不变（池容量仍是生产常量）。
            listener.override_handshake_timeout(Duration::from_millis(500));
            listener
                .register_post(
                    CLAIM_PATH,
                    Arc::new(RecordingHttpHandler {
                        calls: Arc::new(AtomicUsize::new(0)),
                        last_client_ip: Arc::new(std::sync::Mutex::new(None)),
                        status: StatusCode::OK,
                    }),
                    &[],
                )
                .expect("注册 claim");
        },
    )
    .await;
    // 占满全部槽位：只建立 TCP、不发 ClientHello 的连接（无需任何凭据），每个槽位各自占用到超时。
    let _idle: Vec<_> = {
        let mut idle = Vec::new();
        for _ in 0..listener::TLS_HANDSHAKE_POOL_SIZE {
            idle.push(connect(server.addr).await.expect("TCP 连接"));
        }
        idle
    };
    // 新连接在池满时只能等槽位：它排在既有连接之后（TCP 接收队列 FIFO），因此被 accept 时池已耗尽。
    let mut connection = tokio::time::timeout(
        Duration::from_secs(5),
        connect_tls(server.addr, client_tls_config(certificate.der.clone())),
    )
    .await
    .expect("池满时新连接必须等槽位，不得被丢弃或无限推迟")
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
    // direct 模式不产生「明文面」告警（只有 loopback 监听本身也不告警）。
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
    // 证书/私钥缺失即拒绝启动，不降级为明文监听。
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
    // PEM 解析失败即拒绝启动，错误消息不含文件内容。
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
    // Unix 上宽松权限（组/其他可读）即拒绝启动（`SECURITY_DESIGN.md` §13.2）。
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
    // （`[PV5]` 的本机轮次）：Windows 上 ACL 读取需要不安全代码，因此判定是「未核验 警告」，
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
