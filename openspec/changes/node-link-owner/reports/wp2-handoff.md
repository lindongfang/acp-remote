# WP2 Handoff — `server::transport::net`（`node-link-owner` / DU1）

## Shared Report

- **task_id**: 2.4 / 2.5 / 2.6（WP2；阶段 `implement`；交付单元 DU1）；行 1.4 的依赖接入口径在本报告「依赖包含关系」一节核对。
- **role**: coder（WP2 实现 Agent）。
- **phase**: implement。
- **agent_context**: 任务级 coder 子 Agent（worker），**不继承**其他 WP 的实现对话；本 WP 是 W1 波次的唯一写入者，未与其它 Agent 并发。工作目录为独立 worktree `D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`），`CARGO_TARGET_DIR` 未设置但 worktree 自有 `target/`，与主仓库构建目录物理隔离。
- **target_revision**: `4e8a1075a48e004808f498447b54e45dbff824ce`（本 WP 交付提交；起点 `af9064ed6dcd3e88ab76b825253fb2dc95b74967` = WP1 之后、本 WP 之前）。
- **scope**: 只做 WP2：新增 `crates/server/src/transport/net/**`、`crates/server/src/transport/mod.rs`（新增 `pub mod net;`）、`crates/server/src/lib.rs`（crate 文档现状注记）、`crates/server/Cargo.toml`（无改动：WP1 已预登记 axum/rustls/tokio-rustls/rustls-pki-types 与 dev 的 rcgen）。**未改**任何权威规划工件（`plan.md`/`tasks.md`/`verification.md`）、`docs/**`、`schemas/**`/`compatibility/**`、`crates/server/tests/**`（WP7）、`crates/app/**`、`crates/server/src/node_link/**`（WP3–WP6）。
- **changes**（16 个文件，4318 insertions / 7 deletions，`git diff --stat HEAD~1`）：
  - `transport/net/mod.rs`：模块文档、公开面再导出、四个默认常量（默认监听 `127.0.0.1:8765`、单消息 1 MiB、请求体 64 KiB、排空宽限 10000 ms）。
  - `transport/net/config.rs`：`NetConfig`/`TlsMode`（`TlsMode` 手写 `Debug` 隐去两个 PEM 路径）。
  - `transport/net/listener.rs`：`NetListener::bind/local_addrs/warnings/register_ws/register_post/serve`、`NetError`、`ListenerWarning`、`NetAcceptor`+`NetStream`（自定义 `axum::serve::Listener`）、WS 会话 supervisor。
  - `transport/net/route.rs`：`RouteTable`、`HttpHandler`、`RouteError`、边界中间件、WS 升级判定、413、`NetState`。
  - `transport/net/host.rs`、`proxy.rs`、`ratelimit.rs`、`ws.rs`、`http.rs`、`shutdown.rs`、`tls.rs`、`permissions.rs`：Host 边界、可信代理与真实对端地址、滑动窗口限流器、WS 连接值对象与 `WsHandler`、请求/响应值对象与 `PeerInfo`、关闭信号、TLS 加载、平台权限判定。
  - `transport/net/tests.rs` + `transport/net/test_client.rs`（`cfg(test)`）：真实 loopback listener 的端到端用例与手写 HTTP/WS 客户端。
- **checks**（本轮 [PV3]，命令与原始输出见 `reports/wp2-transport-net.log`）：
  - `cargo fmt --all -- --check` → exit 0；
  - `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` → exit 0；
  - `cargo test --locked -p server --all-features` → exit 0（lib 161 通过，其中 `transport::net` 72；集成目标 14/6/4/4 全通过；无失败、无 ignored；`local_endpoint_unix`（`cfg(unix)`）与 doc-tests 在本平台为 0 用例，属既有平台分支而非「全跳过」）；
  - `rg -n "unsafe" crates/server/src` → 零命中（exit 1）；
  - `[PV5]` 的 `cfg(windows)` 轮次：`direct_mode_permissions_are_unverifiable_on_this_platform` 本机执行通过，原始输出写入 `reports/pv5-windows-nodelink.log`；
  - 额外执行（**不作为 [PV1]/[PV2] 的证据行**，仅说明本变更未让既有门禁变红）：`npm run check` exit 0（10 道子检查）、`node scripts/check-crate-boundaries.mjs` exit 0（12 个 crate 与 §5 矩阵一致）。**`npm run verify` 的全 workspace `cargo test` 未在本轮执行**（属 3.3/2.24）。
- **issues**: 无阻断。6 条需要主 Agent / reviewer 知晓的判断与偏离见「冻结形状说明」「判断与偏离」「未执行项」。
- **result**: **PASS**（WP2 计划内检查全部满足；不代表独立 review、E2E 或合并已完成）。
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp2-handoff.md`
  - 原始日志：[PV3] 的 fmt/clippy/test、`rg unsafe` 自检、net 用例逐个执行、提交后复跑、合同门禁与边界门禁（额外执行，非本轮门禁）、提交信息：`openspec/changes/node-link-owner/reports/wp2-transport-net.log`
  - [PV5] 本机 Windows 轮次：`openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log`
- **resource_cleanup**: 用例只绑定 `127.0.0.1:0` 随机端口（进程结束即释放）；TLS 用例用 `rcgen` 现算自签证书并写入 `<temp>/acpr-net-cert-<pid>-<thread>`，由 `TestCertificate` 的 `Drop` 删除；权限用例的临时文件同样在使用后删除；未使用数据库、容器、固定端口或外部账号；`target/` 作为缓存保留（被 git 忽略）。

## 依赖包含关系（1.4 / Dependency Handoffs 的 WP2 行）

- 上游：WP1 的依赖口径（`Cargo.toml` 的 `[workspace.dependencies]` 五项 + `crates/server/Cargo.toml` 预登记 + §5 矩阵 `server→acpr-wire` 格），本 WP 只经这些公开项消费：`axum`（`ws` feature）、`rustls`/`tokio-rustls`（provider = `ring`，未再打开默认 feature）、`rustls-pki-types`（`pem::PemObject`）、`rcgen`（dev，TLS 用例）。
- 包含关系核对：`cargo build -p server`/`cargo test -p server` 只经 `server` 公开项编译，无 `unsafe`、无新增依赖、无 `Cargo.toml`/`Cargo.lock` 改动（`git diff HEAD~1 --stat` 只有 16 个 `crates/server/src/...` 文件）。`cargo check --locked -p server --all-targets --all-features` 通过即证明依赖可解析可编译。
- 失效条件：若 WP1 的依赖口径或 §5 矩阵被改动，本 WP 的 [PV3] 证据按 plan 的 Dependency Handoffs 失效列重开。

## 覆盖映射：R1–R18 → 用例（`specs/node-link-listener/spec.md`）

| R | 场景 / 需求 | 用例（`crates/server/src/transport/net/`） |
|---|---|---|
| R1 | 共享 listener 绑定与失败关闭 | `listener.rs` 的 `invalid_listen_value_fails_closed`、`zero_limits_fail_closed`；`tests.rs` 的 `local_addrs_reports_the_real_bound_address`、`bind_fails_closed_when_the_address_is_taken` |
| R2 | 默认 loopback 启动 | `config.rs` 的 `default_matches_config_reference`（默认值 `127.0.0.1:8765`）、`tests.rs` 的 `default_config_binds_only_loopback` + `local_addrs_reports_the_real_bound_address`；**真实 8765 端口的端到端绑定属 WP7 的 [PV4]**（Coverage Index 的 R2 行同时列 2.4 与 2.21） |
| R3 | 绑定失败即拒绝启动 | `bind_fails_closed_when_the_address_is_taken`（占用端口后 `NetListener::bind` 返回 `NetError::Bind`，无半初始化监听） |
| R4 | 非 loopback 监听告警 | `non_loopback_listen_warns_without_failing`（绑定 `0.0.0.0:0` 成功 + `NonLoopbackListen` + `PlaintextBeyondLoopback`）；默认值不落在非 loopback（`default_config_binds_only_loopback`） |
| R5 | 按 path 的路由 | `unregistered_paths_return_404`（注册 WS + 两个 pairing 端点后，`/sync/v1`、`/sync/v1/pairing/claim`、`/`、`/unknown` 均 404）、`route.rs` 的 `path_validation_rejects_ambiguous_shapes` |
| R6 | Node Link 端点正常升级 | `ws_upgrade_with_the_required_subprotocol_succeeds`（101 + 回填 `Sec-WebSocket-Protocol` + 帧往返） |
| R7 | Sync 路径明确不可用 | `unregistered_paths_return_404`（`/sync/v1` 与 `/sync/v1/pairing/claim` 任意方法 404） |
| R8 | 压缩或错误 subprotocol 被拒绝 | `ws_upgrade_without_the_subprotocol_is_rejected`、`ws_upgrade_with_compression_is_rejected`、`plain_get_on_the_ws_path_is_rejected`；`route.rs` 的 `subprotocol_detection_is_exact_and_token_wise`、`compression_detection_covers_parameterised_offers` |
| R9 | Host 与代理头边界（需求级） | `host.rs` 的 `allowlist_accepts_only_listed_hosts`、`public_origin_accepts_only_its_own_host`、`empty_configuration_accepts_loopback_hosts_only`、`malformed_hosts_are_rejected`、`repeated_host_headers_are_rejected`、`invalid_configuration_values_fail_closed` |
| R10 | Host 不匹配被拒绝 | `wrong_host_is_rejected_before_routing`（已注册与未注册 path 都是 400 且处理器零调用）、`allowed_hosts_whitelist_is_used_when_configured`、`default_configuration_accepts_loopback_host_only` |
| R11 | 不可信来源的转发头被忽略 | `forwarded_headers_are_ignored_from_untrusted_peers`、`forwarded_headers_are_honored_from_trusted_proxies`；`proxy.rs` 的 6 个单测（含 RFC 7239 `Forwarded` 回退与「全部不可解析时回落连接地址」） |
| R12 | TLS 两种终止模式（需求级） | `direct_mode_terminates_tls_and_rejects_plaintext`、`direct_mode_does_not_warn_about_plaintext`、`plaintext_proxy_non_loopback_warns`、`tls.rs` 的 `proxy_mode_has_no_tls_configuration` |
| R13 | direct 模式正常终止 TLS | `direct_mode_terminates_tls_and_rejects_plaintext`（TLS 上完成 HTTP 请求；明文连接拿不到可用会话） |
| R14 | direct 模式证书缺失即拒绝启动 | `direct_mode_missing_certificate_fails_closed`、`direct_mode_invalid_pem_fails_closed`、`tls.rs` 的 `missing_files_fail_closed`/`invalid_pem_fails_closed_without_leaking_contents`、`direct_mode_relaxed_permissions_fail_closed`（`cfg(unix)`，Linux CI）、`direct_mode_permissions_are_unverifiable_on_this_platform`（Windows，[PV5] 本机） |
| R15 | 非 loopback 明文边界 | `plaintext_proxy_non_loopback_warns`、`listener.rs` 的 `plaintext_dev_flag_rejects_non_loopback_listen` |
| R16 | 请求体与单消息上限（需求级） | `pairing_body_over_the_limit_is_rejected_with_413`、`oversize_ws_message_closes_with_1009`、`binary_frames_are_passed_to_the_handler`、`ws.rs` 的 `recognizes_the_capacity_error_for_this_connection`/`other_errors_are_not_message_too_large` |
| R17 | 配对请求体超限 | `pairing_body_over_the_limit_is_rejected_with_413`（413 且处理器零调用；上限内请求仍正常进入） |
| R18 | 超大 WebSocket 消息被拒绝 | `oversize_ws_message_closes_with_1009`（客户端收到 close code 1009，处理器未收到该消息） |

补充（D11 的接入层部分）：`shutdown_drains_and_releases_the_listener`（关闭后会话被取消、端口已释放、新连接不再被接受）、`shutdown.rs` 的三个取消路径单测。

## 冻结的公开形状（WP3 / WP4 按此消费；`crates/server/src/transport/net/` 的公开面）

```text
// config.rs
pub enum TlsMode { Proxy, Direct { cert_path: PathBuf, key_path: PathBuf } }   // Debug 隐去两个路径
pub struct NetConfig {
    pub listen: String,                 // IP:端口 字面量（默认 DEFAULT_LISTEN = "127.0.0.1:8765"）
    pub public_origin: Option<String>,
    pub allowed_hosts: Vec<String>,
    pub trusted_proxies: Vec<String>,
    pub tls: TlsMode,
    pub allow_plaintext_dev: bool,      // dev_mode.allow_plaintext；只对 loopback 生效
    pub max_message_bytes: usize,       // 默认 1 MiB（NODE_LINK_PROTOCOL §2.5）
    pub max_body_bytes: usize,          // 默认 64 KiB（§13.4 的 413）
    pub drain_grace: Duration,          // 对应 daemon.shutdown_grace_ms（默认 10 s）
}
impl Default for NetConfig  // 与 CONFIG_REFERENCE §1/§10 的默认值一致

// listener.rs
pub struct NetListener;
impl NetListener {
    pub async fn bind(config: NetConfig) -> Result<Self, NetError>;
    pub fn local_addrs(&self) -> Vec<SocketAddr>;       // daemon.status.listen 的来源
    pub fn warnings(&self) -> &[ListenerWarning];       // 启动输出（WP7 打印）
    pub fn register_ws(&mut self, path: &str, required_subprotocol: &str,
                       handler: Arc<dyn WsHandler>) -> Result<(), RouteError>;
    pub fn register_post(&mut self, path: &str,
                         handler: Arc<dyn HttpHandler>) -> Result<(), RouteError>;
    pub async fn serve(self, shutdown: Shutdown) -> Result<(), NetError>;
}
pub enum NetError { InvalidListen, PlaintextDevNonLoopback, InvalidPublicOrigin, InvalidAllowedHost,
                    InvalidTrustedProxy, InvalidLimit, TlsFileUnreadable, TlsPemInvalid,
                    TlsInsecurePermissions, TlsKeyCertificateMismatch, TlsConfigBuild, Bind, Serve }
pub enum ListenerWarning { NonLoopbackListen { addr }, PlaintextBeyondLoopback { addr },
                           PermissionsUnverifiable { role, path } }   // 均实现 Display
pub enum TlsFile { Certificate, PrivateKey }   // 实现 Display（「证书」/「私钥」）

// shutdown.rs
pub struct Shutdown;  impl Shutdown { pub fn channel() -> (ShutdownHandle, Shutdown);
                                      pub fn is_shutdown(&self) -> bool; pub async fn wait(self); }
pub struct ShutdownHandle;  impl ShutdownHandle { pub fn trigger(&self); }

// route.rs
#[async_trait] pub trait HttpHandler: Send + Sync + 'static {
    async fn handle(&self, request: HttpRequest) -> HttpResponse; }
pub enum RouteError { InvalidPath, InvalidSubprotocol, DuplicatePath }

// ws.rs
#[async_trait] pub trait WsHandler: Send + Sync + 'static {
    async fn handle(self: Arc<Self>, connection: WsConnection, peer: PeerInfo); }
pub struct WsConnection;
impl WsConnection {
    pub fn negotiated_subprotocol(&self) -> Option<&str>;
    pub async fn recv(&mut self) -> Option<Result<WsMessage, WsError>>;  // None = 连接结束
    pub async fn send_text(&mut self, text: impl Into<String>) -> Result<(), WsError>;
    pub async fn send_binary(&mut self, bytes: Bytes) -> Result<(), WsError>;
    pub async fn close(self, code: u16, reason: &str);
}
pub enum WsMessage { Text(String), Binary(Bytes) }
pub enum WsError { MessageTooLarge, Transport(String) }

// http.rs
pub struct PeerInfo { pub peer_addr: SocketAddr, pub client_ip: IpAddr, pub forwarded_headers: bool,
                      pub tls_terminated: bool, pub shutdown: Shutdown }
pub struct HttpRequest { pub method: Method, pub path: String, pub headers: HeaderMap,
                         pub body: Bytes, pub peer: PeerInfo }
pub struct HttpResponse { pub status: StatusCode, pub headers: HeaderMap, pub body: Bytes }
impl HttpResponse { pub fn new(StatusCode) -> Self; pub fn json(StatusCode, Bytes) -> Self;
                    pub fn with_header(HeaderName, HeaderValue) -> Self;
                    pub fn with_body(impl Into<Bytes>) -> Self; }

// ratelimit.rs
pub struct SlidingWindowLimiter;  impl SlidingWindowLimiter {
    pub fn new(limit: u32, window: Duration) -> Self;
    pub fn check(&self, key: IpAddr) -> RateLimit;              // 键 = PeerInfo::client_ip
    pub fn check_at(&self, key: IpAddr, now: Instant) -> RateLimit;
    pub fn tracked_keys(&self) -> usize; }
pub enum RateLimit { Allowed { remaining: u32 }, Denied { retry_after: Duration } }
pub const MAX_TRACKED_KEYS: usize = 4096;

// host.rs / proxy.rs（配置解析与判定，WP3/WP4 一般不必直接用）
pub struct HostPolicy;  pub struct ProxyPolicy;  pub struct ResolvedPeer { client_ip, forwarded_headers }
```

**WP3/WP4 的接线要点**：注册 `/node-link/v1`（`WS_SUBPROTOCOL = "acp-remote.nodelink.v1.json"`）与两个 pairing path；`node_link` 不 import `axum` 类型，只实现 `WsHandler`/`HttpHandler` 并消费 `WsConnection`/`HttpRequest`/`HttpResponse`/`PeerInfo`；认证限流用 `SlidingWindowLimiter::new(10, Duration::from_secs(60))`，键取 `peer.client_ip`；会话里用 `peer.shutdown` 作为取消路径。

## 判断与偏离（需 reviewer 与主 Agent 知晓）

1. **Host 口径的空档补全（已获裁定）**：`allowed_hosts` 非空 → 白名单；否则 `public_origin` 非空 → 只接受其 host；**两者都空 → 只接受 loopback 形态的 Host**（`127.0.0.0/8`、`[::1]`、`localhost`，端口忽略）。依据：`SPECS` R2 的「默认 loopback 启动后端点可达」与 `CONFIG_REFERENCE.md` §1「只接受与 `public_origin` 一致的 Host」在 `public_origin = null` 时互相冲突，本轮由 supervisor 裁定取 A（`SECURITY_DESIGN.md` §7.2 禁止任意 Host / DNS rebinding，排除「放行任意 Host」；排除「拒绝全部」因为它让默认配置不可达）。该口径写在 `host.rs` 的模块文档与 `HostPolicy::new` 注释里，并有三个分支各自的单测。`CONFIG_REFERENCE.md` §1 的条文补写归 WP8（主 Agent 已记录，本 WP 不动该文件）。
2. **`HttpResponse.status` 用 `StatusCode` 而非任务书建议的 `u16`**：性质不变（同一状态码域），但更早失败（构造期即拒绝非法状态码）。WP3 直接用 `StatusCode::{OK, CREATED, BAD_REQUEST, UNAUTHORIZED, FORBIDDEN, NOT_FOUND, CONFLICT, GONE, PAYLOAD_TOO_LARGE, TOO_MANY_REQUESTS}`。
3. **`WsHandler::handle` 的接收者是 `self: Arc<Self>` 而非 `&self`**：WS 会话必须能被接入选层放进 `'static` 的任务集（`axum` 的升级回调自身是 `tokio::spawn` 出来的），借用式接收者会让会话 future 绑在栈上、无法被接入选层持有与取消（也就无法满足「无 detached task」）。`HttpHandler` 仍在请求内就地 await，保持 `&self`。
4. **`NetConfig.listen` 是字符串而非 `SocketAddr`**：与 `daemon.listen` 的配置类型一致，且只接受 `IP:端口` 字面量（不做主机名解析——一个名字解析出多个地址会让「绑定一个 listener」的语义不确定）。非法取值 → `NetError::InvalidListen`。
5. **`max_body_bytes` 的默认值 64 KiB**：`NODE_LINK_PROTOCOL.md` §13.4 只要求「有上限」，`CONFIG_REFERENCE.md` §3 没有对应配置键，因此由 `transport::net` 取一个远大于配对请求体（小型 JSON）的保守默认值，并由 `NetConfig` 字段决定；WP7 若将来新增配置键只需传值。
6. **1009 的判定依赖 tungstenite 的错误文本**：`tungstenite 0.29` 的容量错误文本是 `Space limit exceeded: Message too long: {大小} > {上限}`（本轮实测），`ws.rs` 在**一处**（`message_too_large`）解析它并核对上限数值等于本连接配置值；`recognizes_the_capacity_error_for_this_connection` 固定该文本形态、`oversize_ws_message_closes_with_1009` 端到端断言客户端收到 1009。**若上游措辞变化，用例会失败而不是静默降级**（代价：升级 tungstenite 时需要看一眼这个用例）。
7. **TLS 握手的接入层超时 10 秒**（`listener.rs` 的常量，不出现在配置键里）：只连接不握手的对端不得无限占用 accept 循环；协议层限额仍以 `NODE_LINK_PROTOCOL.md` §2.5 为准。
8. **`inspect_file_permissions` 与 `storage-sqlite` 的判定同构但各自实现**：两个 crate 是平级适配器（`AGENTS.md` §4 禁止横向依赖），因此这里复制了纯函数形态的权限视图，并在模块文档指向 `CORE_PORTS_AND_STORAGE.md` §7.1/§9 判据 12 的既有裁定。
9. **`X-Forwarded-Proto`/`X-Forwarded-Host` 不被消费**：TLS 边界与 Host 归属由 `daemon.tls.mode` + `public_origin`/`allowed_hosts` 表达（`CONFIG_REFERENCE.md` §1 三形态表），`SECURITY_DESIGN.md` §7.2 要求反代原样保留 `Host`；引入第二条 Host 来源会削弱「拒绝任意 Host」的可判定性。

## 未执行项（不粉饰）

1. **`npm run verify`（[PV1] 的全 workspace `cargo test`）不在本轮**：本轮只跑 [PV3] 的 fmt/clippy/`-p server` 测试 + `rg unsafe` 自检 + [PV5] 的 Windows 轮次；此外额外跑了 `npm run check` 与 `check:boundaries`（均为 exit 0）。[PV1]/[PV2] 的正式轮次属 3.3。
2. **Linux 专属分支未在本机执行**：`direct_mode_relaxed_permissions_fail_closed`（`cfg(unix)`）与 `permissions.rs` 的 `unix_inspection_reports_relaxed_for_world_readable_file` 在本机 Windows 不编译，由 Linux CI 覆盖；本轮如实记为「本机未执行」。
3. **真实第二 OS 账号的跨用户 ACL 端到端检查未执行**：需要第二个 OS 账号，属平台限制（与 `storage-sqlite` 的既有口径一致）。Windows 上的判定本身是「不可核验 + 告警」，已在 [PV5] 留证。
4. **非 loopback 主机的真实外部可达性未验证**：用例只绑定 `0.0.0.0:0` 验证告警与绑定成功，未从另一台机器发起连接（无外部主机资源）。
5. **`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）本地不可执行**：其判定只在 CI 生效（`reports/wp1-deps.log` 已记录同一口径）；本 WP 不新增依赖、不提交私钥材料。
6. **WP7 未完**：`daemon.listen`/`daemon.tls.*`/`daemon.allowed_hosts`/`daemon.trusted_proxies` 仍在 `app` 的 unwired 清单里（本 WP 只提供接入层与公开形状，接线与启动输出由 2.21 落地）。

## 待澄清问题

无阻塞项。第 1 条（Host 口径）已由 supervisor 裁定并落入代码与本文档；若 reviewer 认为 `CONFIG_REFERENCE.md` §1 需要更早补写条文（而不是等 WP8），请由主 Agent 决定（本 WP 的写入范围不含 `docs/**`）。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.4"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/wp2-transport-net.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo fmt --all -- --check（exit 0）、cargo clippy --locked -p server --all-targets --all-features -- -D warnings（exit 0）、cargo test --locked -p server --all-features（exit 0；lib 161 通过，其中 transport::net 72；无失败/ignored）在 target_revision 上执行；R5–R11 的场景由 tests.rs/host.rs/proxy.rs/route.rs 的用例覆盖（见本报告映射表）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/wp2-transport-net.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一轮 [PV3] 命令集；R12–R18 由 direct_mode_*（TLS 终止/失败关闭/权限）、pairing_body_over_the_limit_is_rejected_with_413、oversize_ws_message_closes_with_1009、ws.rs 的错误文本用例覆盖；rg -n \"unsafe\" crates/server/src 零命中（exit 1）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本机 Windows 轮次：direct_mode_permissions_are_unverifiable_on_this_platform 执行通过（1 passed），net 用例整轮 72 passed；Linux 专属的 cfg(unix) 权限用例本机不编译，由 CI 覆盖（如实记录，未写成已通过）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.6"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/wp2-transport-net.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "交付前局部验证：fmt/clippy/-p server 测试全绿、无零用例/全跳过（net 72 个用例逐个出现在 --list 与执行输出里）；transport::net 的公开形状已冻结（见本报告「冻结的公开形状」一节）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.6"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: VALIDATION
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/node-link-owner/reports/wp2-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "额外执行的既有合同门禁（npm run check 与 check-crate-boundaries，均 exit 0）只证明本变更未让门禁变红；**不作为 [PV1]/[PV2] 的证据行**（未跑全 workspace cargo test，正式轮次属 3.3/2.24），日志见 reports/wp2-transport-net.log 第 6 节。"
    source_evidence: NOT_APPLICABLE
```
