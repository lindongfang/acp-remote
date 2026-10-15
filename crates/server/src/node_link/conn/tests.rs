//! `server::node_link::conn` 的行为用例（`[PV3]` 的 WP4 部分）。
//!
//! 驱动方式：真实 loopback listener（只经 `transport::net` 的公开形状注册 WSS 处理器）+ 本模块自带的
//! 极简 WebSocket 客户端。因此「Host/subprotocol 准入 → 帧 → 信封 → 握手 → 业务」整条链路都在真实
//! 连接上被验证，接入层与协议层之间的接线（例如 binary 帧、超限、close code）不是手工调用的结果。
//!
//! 配对侧的事实由 `local_admin::test_support` 的端口替身提供（同一 crate 的测试设施，不是第二个实现）：
//! `node.pair.begin` → `TestWorld::claim` → `node.pair.confirm` 经真实的本地通道推进到 `approved`，
//! 与 WP3 的 HTTPS claim 路径落库的事实同形。
//!
//! 覆盖映射（`specs/node-link-owner-server/spec.md`）：
//!
//! | 需求 | 用例 |
//! |---|---|
//! | 握手准入与版本/feature 协商 | `handshake_completes_and_enters_the_business_phase`、`an_envelope_version_other_than_v1_is_closed_with_4406`、`a_handshake_that_never_gets_a_hello_is_closed_with_4408` |
//! | 正常握手完成 | `handshake_completes_and_enters_the_business_phase` |
//! | 认证前发送业务消息 | `a_business_message_before_hello_is_closed_with_4401` |
//! | 必需 feature 未满足 | `a_missing_required_feature_is_reported_with_details_and_closed` |
//! | 节点双向认证与凭据状态 | `handshake_completes_and_enters_the_business_phase`（双向证明与审计） |
//! | 的验收项（任务 2.29） | `the_challenge_catalog_revision_is_the_proof_transcript_source` |
//! | proof 无效被拒绝 | `an_invalid_proof_is_closed_with_4401_and_audited` |
//! | 已撤销节点连接被拒绝 | `a_revoked_node_is_closed_with_4410` |
//! | 未知节点不泄露存在性之外的能力 | `an_unknown_node_gets_a_challenge_and_fails_as_node_unknown` |
//! | limits 只下调 | `node_ready_echoes_the_negotiated_limits_and_never_raises_them` |
//! | 固定常量不可协商 | `node_ready_echoes_the_negotiated_limits_and_never_raises_them`、`protocol_constants_are_not_configurable`、`json_structure_limits_are_enforced_on_the_wire` |
//! | 信封与 connectionSequence 校验 | `envelope_and_sequence_violations_are_rejected_without_closing`、`a_binary_frame_is_closed_with_4400` |
//! | 每次消息级拒绝都推进入站期望序号 | `envelope_and_sequence_violations_are_rejected_without_closing`（未知 type / post_mvp / 形状错误）、`a_shape_invalid_envelope_that_consumed_the_sequence_does_not_strand_the_connection`、`nesting_beyond_the_parser_recursion_limit_cannot_advance_the_sequence`（已知残留的解析边界：深度 127 推进、128 不推进） |
//! | 出站序号只在投递确认时消耗（`connectionSequence` 不得留缺口） | `a_rejected_delivery_does_not_consume_the_outbound_sequence` |
//! | 认证收尾推进 `owned_node.last_connected_at`（§11.6 第 9 条） | `authentication_advances_the_node_last_connected_time` |
//! | §2.5 准入闸门（10 次/分钟、同源在途握手配额） | `the_eleventh_authentication_attempt_from_one_address_is_rate_limited`、`the_fifth_in_flight_handshake_from_one_address_is_refused`、`authenticated_connections_do_not_hold_the_in_flight_handshake_quota` |
//! | 序号回退被拒绝 | `envelope_and_sequence_violations_are_rejected_without_closing` |
//! | post_mvp 消息显式拒绝 | `envelope_and_sequence_violations_are_rejected_without_closing`（`catalog.changed`/`link.backpressure`） |
//! | 未知字段被拒绝 | `envelope_and_sequence_violations_are_rejected_without_closing` |
//! | 首次认证清除已批准配对 | `handshake_completes_and_enters_the_business_phase` |
//! | 心跳与 90 秒静默 | `heartbeat_round_trip_keeps_the_connection_alive`、`silence_beyond_the_window_is_closed_with_4408`、`a_busy_outbound_queue_does_not_starve_the_heartbeat`、`inbound_silence_still_closes_a_connection_with_a_busy_outbound_queue` |
//! | 饱和可恢复（高水位进入 / 低水位迟滞退出） | `a_saturated_queue_clears_only_below_the_low_water_mark` |
//! | 瞬时突发后回落不得断开、不影响其他连接 | `a_transient_burst_that_drains_does_not_disconnect_the_connection` |
//! | 持续饱和的判定必须成立、且不拖住其他连接 | `a_slow_consumer_that_never_recovers_keeps_its_saturation_segment`、`a_continuously_saturated_connection_does_not_stall_its_peer` |
//! | 持续饱和的连接仍会被断开 | `a_permanently_saturated_queue_still_reaches_the_slow_consumer_watchdog` |
//! | 饱和期间心跳不停（不得因心跳停摆走静默关闭） | `the_tick_keeps_the_heartbeat_while_the_connection_is_saturated`、`a_heartbeat_deferred_by_the_high_water_mark_is_not_fatal` |
//! | 饱和恢复后重新读取快照与事件批次 | `a_recovered_connection_resumes_catalog_batches_and_event_delivery` |
//! | 审计与日志边界 | `handshake_completes_and_enters_the_business_phase`、`an_invalid_proof_is_closed_with_4401_and_audited` |
//! | 验收「schema/fixture 漂移」 | `fixtures_are_consumed_by_the_handshake_and_error_layers` |
//!
//! 四处**刻意的测试手法**（都在用例里就地说明）：
//!
//! - `node-link-proof/v1` 的 `catalogRevision`：用例只从 `node.challenge` 的 body 取该字段（Access 该走的路），
//!   不再带外取值——2026-09-26 的 v1 内合同修订把字段补进了握手消息（`NODE_LINK_PROTOCOL.md` §12.2 的修订记录，
//!   design D13）；`the_challenge_catalog_revision_is_the_proof_transcript_source` 进一步锁定
//!   「取错值必然验签失败」，证明该字段真的进了验签输入；
//! - 90 秒静默与 30 秒慢消费者宽限：用 [`NodeLinkConn::with_test_windows`] 调短窗口（常量取值由
//!   `protocol_constants_are_not_configurable` 与静默边界断言锁定，机制由真实连接覆盖）；
//! - 「出站队列持续非空 / 持续饱和」的压力：用独立 OS 线程当生产者（[`OutboundPressure`]），而不是
//!   tokio 任务——只有与运行时**并行**的生产者才能保证 `select!` 的每一轮都有出站消息就绪，从而真的
//!   复现「tick 被出站分支饿死」这一缺陷条件（理由见该结构的文档注释）；
//! - 饱和的判定分两层锁定：**瞬时突发后回落**、**持续饱和 → 断开**、**持续饱和不拖住其他连接**与
//!   **恢复后重新读取批次**都走真实连接 / 真实路由（分别在宽限之后断言连接仍在、宽限之后断言以
//!   `close::NORMAL` 断开、宽限窗口内断言饱和段连续累计到宽限之上且对照组持续可用、以及断言两批
//!   catalog 快照 + 完整 resource 快照 + 扇出事件都重新到达）；**饱和期心跳不被短路**与**「持续饱和」
//!   判据本身**在句柄口径与两个纯函数（[`tick_action`]、[`heartbeat_outcome`]）上锁定——饱和时队列
//!   本就顶满，`link.ping` 只能被推迟（一级处置），端到端观察不到「饱和期间真的收到 ping」；「队列
//!   持续非空（未饱和）时心跳照发」由 `a_busy_outbound_queue_does_not_starve_the_heartbeat` 在真实
//!   连接上覆盖。

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use acp_core::model::{
    Actor, AgentId, AgentRef, AuditAction, AuditOutcome, CachePolicy, CommandRecord,
    CommittedDelivery, CommittedEvent, EntityRef, EventId, EventPayload, EventType, ExportId,
    ExportRecord, ExportTemplate, GlobalCursor, GrantSet, NodeId, NodeKind, NodeRecord, NodeState,
    OriginCursor, OriginEpoch, OwnedSessionRef, PairingId, PairingPeer, PairingState, PeerIdentity,
    PortError, RequestId, ResourceOrigin, Sequence, ServerEpoch, Session, SessionId,
    SessionSnapshot, SessionState, SessionSummary, TemplateId, Timestamp as TimestampText, Version,
    ViewJson, WorkspaceAlias, WorkspaceAliasEntry,
};
use acp_core::ports::{
    AuditQuery, AuditStore as _, CommitOutcome, EventPublisher as _, HistoryPage, HistoryQuery,
    NodeLinkSlice, OwnedCommit, OwnedEventRecord, PruneReport, ReadView, ReplayBatch, ReplayLimit,
    RetentionPolicy, SessionQuery, SessionStore, StoreHealth,
};
use acp_core::use_cases::NodeLinkHandshakeView;
use base64::Engine as _;
use identity_auth::{ChallengeId, FeatureList, NodeLinkChallenge, NodeLinkProof, P1363Signature};
use node_link_protocol::common::{DecimalString, Uuid};
use node_link_protocol::envelope::{ConnectionFields, Envelope, MessageType};
use node_link_protocol::structure::{MAX_ARRAY_ELEMENTS, MAX_NESTING_DEPTH, MAX_OBJECT_FIELDS};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::local_admin::envelope::{AdminOutcome, AdminRequest, AdminResponse, decode_request};
use crate::local_admin::handler::LocalAdminHandler as _;
use crate::local_admin::method::Method;
use crate::local_admin::test_support::{
    FakeIds, FixedStore, TEST_PUBLIC_ORIGIN, TEST_SERVER_EPOCH, TestWorld, test_nonce,
    test_public_key,
};
use crate::node_link::catalog::CatalogRoute;
use crate::node_link::conn::handshake::SUPPORTED_FEATURES;
use crate::node_link::conn::limits::{
    DEFAULT_CATALOG_SNAPSHOT_BATCH_SIZE, DEFAULT_HEARTBEAT_INTERVAL_MS,
    DEFAULT_MAX_IN_FLIGHT_COMMANDS, DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_MAX_PENDING_QUEUE_BYTES,
    DEFAULT_MAX_PENDING_QUEUE_MESSAGES, DEFAULT_RESOURCE_SNAPSHOT_BATCH_SIZE, NodeLinkConfig,
};
use crate::node_link::conn::registry::Outbound;
use crate::node_link::conn::session::{
    HEARTBEAT_SILENCE_TIMEOUT, HeartbeatOutcome, TickAction, heartbeat_outcome, tick_action,
};
use crate::node_link::conn::{
    AUTH_ATTEMPTS_PER_MINUTE, ConnectionHandle, ConnectionRegistry, HANDSHAKE_TIMEOUT,
    MAX_IN_FLIGHT_HANDSHAKES_PER_IP, MessageRoute, NodeLinkConn, RouteOutcome, SendFault,
    SessionLimits, WS_PATH, WS_SUBPROTOCOL, WsEndpoint, close,
};
use crate::node_link::resource::{NodeLinkPublisher, ResourceRoute};
use crate::transport::net::{NetConfig, NetError, NetListener, Shutdown, ShutdownHandle};

// ---------------------------------------------------------------------------------------------
// 常量与固定素材
// ---------------------------------------------------------------------------------------------

/// 本节点标识（与 `TestWorld` 装配的 `Authority` 同源）。
const OWNER_NODE: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
/// 测试扮演的 Access Node（经真实配对通道批准）。
const ACCESS_NODE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
/// 从未配对的 Access Node。
const UNKNOWN_NODE: &str = "9c8f6b1d-7a35-4f0b-9b6a-2f6d5c4e3b1a";
/// 本机的目录修订号（`catalogRevision`）：`AuditStore::watermark()` 的来源（`FakeAudit` 按本值播种子
/// 条审计行）；`node.challenge`/`node.ready` 必须与它同源。
const CATALOG_REVISION: u64 = 7;
/// `FixedStore` 的 `head()` 值（`serverEpoch` 的来源；`catalogRevision` 已改取审计水位，不再用它）。
const STORE_HEAD_SEQUENCE: u64 = 7;
/// 单次 I/O 的超时（用例失败要快速失败，不挂住测试进程）。
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// [`a_handshake_that_never_gets_a_hello_is_closed_with_4408`] 需要等满 15 秒握手窗口。
const TIMEOUT_WINDOW_IO: Duration = Duration::from_secs(25);
/// `node-link.core.v1` 是唯一必需 feature（`features.json` 的 `required: true`）。
const REQUIRED_FEATURE: &str = "node-link.core.v1";

// ---------------------------------------------------------------------------------------------
// 世界与端点
// ---------------------------------------------------------------------------------------------

/// 端口替身世界 + 真实 loopback listener + 已注册的 WSS 端点。
struct Harness {
    world: TestWorld,
    addr: SocketAddr,
    conn: Arc<NodeLinkConn>,
    shutdown: ShutdownHandle,
    join: JoinHandle<Result<(), NetError>>,
}

impl Harness {
    /// 起一个已注册 `/node-link/v1` 的接入层（`127.0.0.1:0`，端口由内核分配）。
    async fn start(world: TestWorld, config: NodeLinkConfig) -> Self {
        Self::start_with_windows(world, config, None).await
    }

    /// 同 [`Harness::start`]，但把会话看门狗的两个窗口换成本次用例的值。
    ///
    /// §2.5 的两个窗口是固定常量（90 秒静默 + 30 秒慢消费者宽限），真实等待在用例里不可接受；窗口取值
    /// 本身由 `protocol_constants_are_not_configurable` 与静默边界断言锁定，本入口只让机制可被覆盖。
    async fn start_with_test_windows(
        world: TestWorld,
        config: NodeLinkConfig,
        silence: Duration,
        slow_consumer: Duration,
    ) -> Self {
        Self::start_with_windows(world, config, Some((silence, slow_consumer))).await
    }

    async fn start_with_windows(
        world: TestWorld,
        config: NodeLinkConfig,
        windows: Option<(Duration, Duration)>,
    ) -> Self {
        // `catalogRevision` 取审计水位（`AuditStore::watermark`，design D4/G5 的口径）：把本机世界的水位
        // 固定为 [`CATALOG_REVISION`]，断言值因此可判定。
        world.audit.set_watermark(CATALOG_REVISION);
        let registry = ConnectionRegistry::new();
        let conn = NodeLinkConn::new(
            world.core.clone(),
            world.authority.clone(),
            config,
            Arc::clone(&registry),
        );
        let conn = match windows {
            Some((silence, slow_consumer)) => conn.with_test_windows(silence, slow_consumer),
            None => conn,
        };
        let conn = Arc::new(conn);
        let mut listener = NetListener::bind(NetConfig {
            listen: "127.0.0.1:0".to_owned(),
            ..NetConfig::default()
        })
        .await
        .expect("绑定 loopback listener");
        listener
            .register_ws(
                WS_PATH,
                WS_SUBPROTOCOL,
                WsEndpoint::handler(Arc::clone(&conn)),
            )
            .expect("注册 Node Link 端点");
        let addr = listener.local_addrs()[0];
        let (shutdown, signal) = Shutdown::channel();
        let join = tokio::spawn(async move { listener.serve(signal).await });
        Self {
            world,
            addr,
            conn,
            shutdown,
            join,
        }
    }

    /// 默认配置 + 带固定水位的存储替身（`daemon.public_origin` 与 `TestWorld` 的 canonical origin 同源）。
    async fn with_store(store: Arc<dyn SessionStore>) -> Self {
        Self::start(
            TestWorld::with_store(store),
            NodeLinkConfig {
                public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
                ..NodeLinkConfig::default()
            },
        )
        .await
    }

    /// 默认世界与默认配置（`serverEpoch` 固定为 [`TEST_SERVER_EPOCH`]，`catalogRevision` 固定为
    /// [`CATALOG_REVISION`]）。
    async fn new() -> Self {
        Self::with_store(Arc::new(FixedStore::new(
            TEST_SERVER_EPOCH,
            STORE_HEAD_SEQUENCE,
        )))
        .await
    }

    /// 结束接入层（用例末尾调用；端口随 listener 释放）。
    async fn stop(self) {
        self.shutdown.trigger();
        let _ = tokio::time::timeout(IO_TIMEOUT, self.join).await;
    }

    /// 本机用户登记一次节点配对（`node.pair.begin`），返回配对 id。
    async fn begin(&self) -> String {
        let response = self
            .world
            .router()
            .handle(admin_request(
                Method::NodePairBegin,
                json!({
                    "mode": "owner",
                    "pairingUrl": null,
                    "displayName": "Owner Node",
                    "requestedGrants": ["grant.observe"],
                }),
            ))
            .await;
        success(&response)["pairingId"]
            .as_str()
            .expect("pairingId")
            .to_owned()
    }

    /// Access 侧认领（与 HTTPS claim 落库的事实同形：peer 行 + `pending_confirmation`）。
    ///
    /// `host_binding` 取注册值的逐字回显（§11.2 第 1 条：绑定不一致的认领必须被拒）。
    async fn claim(&self, pairing: &str) {
        let binding = self
            .world
            .trust
            .pairing(pairing)
            .expect("登记后的配对必须在库里")
            .host_binding()
            .to_owned();
        let peer = PairingPeer::try_new(
            PeerIdentity::Node(node(ACCESS_NODE)),
            "Office Access",
            test_public_key(),
            &binding,
            test_nonce(),
        )
        .expect("测试用 peer 合法");
        self.world
            .claim(&pairing_id(pairing), peer)
            .await
            .expect("认领必须成功");
    }

    /// 本机用户确认配对（真实本地通道；`approved` + 信任行 + 验签材料）。
    async fn approve(&self, pairing: &str, grants: &[&str]) {
        let response = self
            .world
            .router()
            .handle(admin_request(
                Method::NodePairConfirm,
                json!({"pairingId": pairing, "grants": grants, "exportIds": []}),
            ))
            .await;
        assert_eq!(
            success(&response)["grants"],
            json!(grants),
            "确认后的授权集合必须逐条回显"
        );
    }

    /// 走完整配对链路并返回配对 id（`approved`，授权 `grant.observe`）。
    async fn approved_pairing(&self) -> String {
        let pairing = self.begin().await;
        self.claim(&pairing).await;
        self.approve(&pairing, &["grant.observe"]).await;
        pairing
    }

    /// 本机用户撤销该节点（`node.revoke` 的前置）。
    async fn revoke_node(&self, node_id: &str) {
        let response = self
            .world
            .router()
            .handle(admin_request(
                Method::NodeRevoke,
                json!({"nodeId": node_id}),
            ))
            .await;
        let _ = success(&response);
        assert_eq!(
            self.world.trust.nodes_of(node_id)[0].state(),
            NodeState::Revoked,
            "撤销必须先落库"
        );
    }

    /// 持久化信任的当次快照（用例断言用）。
    async fn view(&self, node_id: &str) -> NodeLinkHandshakeView {
        self.world
            .core
            .node_link_handshake_view(&node(node_id))
            .await
            .expect("握手视图可读")
    }

    /// 该节点最近一次配对的 id（快照里的 `pairing` 行）。
    async fn pairing_of(&self, node_id: &str) -> String {
        self.view(node_id)
            .await
            .pairing
            .expect("该节点有配对行")
            .id()
            .as_str()
            .to_owned()
    }

    /// 审计行（按动作过滤）：`AuditStore` 的行 + 信任写集提交时随事务写入的行。
    async fn audit_rows(&self, actions: Vec<AuditAction>) -> Vec<acp_core::model::AuditRecord> {
        let mut rows = self
            .world
            .audit
            .query(AuditQuery {
                since: None,
                until: None,
                actions: actions.clone(),
                actor: None,
                target: None,
                limit: None,
            })
            .await
            .expect("审计查询");
        rows.extend(
            self.world
                .trust
                .audits()
                .into_iter()
                .filter(|row| actions.is_empty() || actions.contains(&row.action())),
        );
        rows.sort_by(|left, right| left.at().as_str().cmp(right.at().as_str()));
        rows
    }
}

fn node(text: &str) -> NodeId {
    NodeId::new(text).expect("测试用 NodeId 文本必须规范")
}

fn pairing_id(text: &str) -> PairingId {
    PairingId::new(text).expect("测试用 PairingId 文本必须规范")
}

fn admin_request(method: Method, params: Value) -> AdminRequest {
    let bytes = serde_json::to_vec(&json!({
        "v": 1,
        "id": "2ae1c07c-0000-4000-8000-0000000000ff",
        "method": method.as_str(),
        "params": params,
    }))
    .expect("请求信封可序列化");
    decode_request(&bytes).expect("信封合法")
}

fn success(response: &AdminResponse) -> Value {
    match response.outcome() {
        AdminOutcome::Success { result } => Value::Object(result.clone()),
        AdminOutcome::Failure { error } => panic!(
            "期望成功响应，实际 {}：{}",
            error.code().as_str(),
            error.message()
        ),
    }
}

/// 规范的 32 字节 nonce 文本（末字节低 2 位固定为 0）。
fn nonce_text(seed: u8) -> String {
    let mut bytes = [seed; 32];
    bytes[31] &= 0b1111_1100;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 16 字节 nonce 文本（`link.ping`/`link.pong`）。
fn ping_nonce(seed: u8) -> String {
    let bytes = [seed; 16];
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 用固定私钥（标量 1，公钥即 `test_public_key()` 的基点 G）签名一段 transcript。
///
/// 与 `FakeKeystore::sign` 同一素材：Access 侧证明必须能由配对批准写入持久化的公钥验证。
fn sign_with_access_key(transcript: &[u8]) -> String {
    use p256::ecdsa::signature::Signer as _;
    let mut scalar = [0u8; 32];
    scalar[31] = 1;
    let signing = p256::ecdsa::SigningKey::from_slice(&scalar).expect("标量 1 是合法私钥");
    let signature: p256::ecdsa::Signature = signing.sign(transcript);
    P1363Signature::try_from_bytes(signature.to_bytes().as_slice())
        .expect("P-256 签名是 64 字节")
        .to_base64url()
}

// ---------------------------------------------------------------------------------------------
// 极简 WebSocket 客户端
// ---------------------------------------------------------------------------------------------

/// 一个入站帧。
enum Frame {
    Text(String),
    Binary,
    Close(u16, String),
    Ping,
    Pong,
}

/// 极简 WebSocket 客户端（文本/二进制/close 帧 + 客户端掩码；不做扩展与分片）。
struct Client {
    stream: TcpStream,
    buffer: Vec<u8>,
    /// 本方向下一个出站序号（认证后从 `1` 开始）。
    outbound_sequence: u64,
    /// `node.challenge` 下发的连接标识（认证后消息必须携带）。
    connection_id: Option<String>,
    /// 已读到但用例尚未取用的消息。
    pending: Vec<Value>,
    /// 已收到的服务端出站序号（认证后从 `node.ready` 的 1 开始）。
    server_sequence: u64,
    /// 当前用例段落（失败信息用）。
    step: String,
    /// 最近一条本方向消息的 `connectionSequence`（失败信息用）。
    last_sent_sequence: Option<String>,
    /// 读帧的超时（15 秒握手窗口的用例需要更长的预算）。
    read_timeout: Duration,
}

impl Client {
    /// 建立连接并完成 WebSocket 升级（带上 subprotocol）。
    async fn connect(addr: SocketAddr) -> Self {
        Self::connect_with_timeout(addr, IO_TIMEOUT).await
    }

    async fn connect_with_timeout(addr: SocketAddr, read_timeout: Duration) -> Self {
        let mut stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
            .await
            .expect("连接超时")
            .expect("连接 loopback");
        let mut key_bytes = [0x11u8; 16];
        key_bytes[0] = 0x42;
        let key = base64::engine::general_purpose::STANDARD.encode(key_bytes);
        let request = format!(
            "GET {WS_PATH} HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Protocol: {WS_SUBPROTOCOL}\r\n\r\n"
        );
        tokio::time::timeout(IO_TIMEOUT, stream.write_all(request.as_bytes()))
            .await
            .expect("写升级请求超时")
            .expect("写升级请求");
        let mut buffer = Vec::new();
        let head_end = loop {
            if let Some(head_end) = find_subsequence(&buffer, b"\r\n\r\n") {
                break head_end;
            }
            let mut chunk = [0u8; 1024];
            let read = tokio::time::timeout(IO_TIMEOUT, stream.read(&mut chunk))
                .await
                .expect("读升级响应超时")
                .expect("读升级响应");
            assert!(read > 0, "升级响应未读全就被关闭");
            buffer.extend_from_slice(&chunk[..read]);
        };
        let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
        assert!(
            head.starts_with("HTTP/1.1 101 "),
            "WebSocket 升级必须成功，实际：{head}"
        );
        assert!(
            head.to_ascii_lowercase()
                .contains(&format!("sec-websocket-protocol: {WS_SUBPROTOCOL}")),
            "升级响应必须回显协商出的 subprotocol，实际：{head}"
        );
        buffer.drain(..head_end + 4);
        Self {
            stream,
            buffer,
            outbound_sequence: 0,
            connection_id: None,
            pending: Vec::new(),
            server_sequence: 0,
            step: String::new(),
            last_sent_sequence: None,
            read_timeout,
        }
    }

    /// 发送一条文本帧。
    async fn send_text(&mut self, text: &str) {
        self.send_frame(0x1, text.as_bytes()).await;
    }

    /// 发送一条二进制帧。
    async fn send_binary(&mut self, bytes: &[u8]) {
        self.send_frame(0x2, bytes).await;
    }

    /// 发送一个 close 帧（`1000`）。
    async fn send_close(&mut self) {
        self.send_frame(0x8, &1000u16.to_be_bytes()).await;
    }

    async fn send_frame(&mut self, opcode: u8, payload: &[u8]) {
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x80 | opcode);
        // 客户端帧必须掩码；掩码键取固定值（对端不校验取值）。
        let mask = [0x21u8, 0x43, 0x65, 0x87];
        if payload.len() < 126 {
            frame.push(0x80 | payload.len() as u8);
        } else if payload.len() <= u16::MAX as usize {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % 4]),
        );
        tokio::time::timeout(IO_TIMEOUT, self.stream.write_all(&frame))
            .await
            .expect("写帧超时")
            .expect("写帧");
    }

    /// 读取下一个帧（`None` = 连接已结束）。
    async fn next_frame(&mut self) -> Option<Frame> {
        self.fill(2).await?;
        let opcode = self.buffer[0] & 0x0f;
        let masked = self.buffer[1] & 0x80 != 0;
        let mut length = u64::from(self.buffer[1] & 0x7f);
        let mut offset = 2usize;
        if length == 126 {
            self.fill(4).await?;
            length = u64::from(u16::from_be_bytes([self.buffer[2], self.buffer[3]]));
            offset = 4;
        } else if length == 127 {
            self.fill(10).await?;
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&self.buffer[2..10]);
            length = u64::from_be_bytes(bytes);
            offset = 10;
        }
        let length = usize::try_from(length).expect("帧长度可放进 usize");
        if masked {
            offset += 4;
        }
        self.fill(offset + length).await?;
        let mask: Option<[u8; 4]> = masked.then(|| {
            let mut key = [0u8; 4];
            key.copy_from_slice(&self.buffer[offset - 4..offset]);
            key
        });
        let mut payload = self.buffer[offset..offset + length].to_vec();
        if let Some(mask) = mask {
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % 4];
            }
        }
        self.buffer.drain(..offset + length);
        Some(match opcode {
            0x1 => Frame::Text(String::from_utf8(payload).expect("文本帧必须是 UTF-8")),
            0x2 => Frame::Binary,
            0x8 => {
                let code = payload
                    .get(..2)
                    .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
                    .unwrap_or(0);
                let reason = payload
                    .get(2..)
                    .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                    .unwrap_or_default();
                Frame::Close(code, reason)
            }
            0x9 => Frame::Ping,
            0xa => Frame::Pong,
            other => panic!("未预期的 WebSocket opcode：{other}"),
        })
    }

    /// 确保缓冲区里至少有 `needed` 字节；返回 `None` 表示对端已关闭流。
    async fn fill(&mut self, needed: usize) -> Option<()> {
        while self.buffer.len() < needed {
            let mut chunk = [0u8; 16 * 1024];
            let read = tokio::time::timeout(self.read_timeout, self.stream.read(&mut chunk))
                .await
                .expect("读帧超时")
                .expect("读帧");
            if read == 0 {
                return None;
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
        Some(())
    }

    /// 取下一条消息（跳过 WS 控制帧）；`None` = 连接结束。
    async fn next_message(&mut self) -> Option<Value> {
        if !self.pending.is_empty() {
            return Some(self.pending.remove(0));
        }
        loop {
            match self.next_frame().await? {
                Frame::Text(text) => {
                    return Some(
                        serde_json::from_str(&text).unwrap_or_else(|error| {
                            panic!("服务端消息不是 JSON（{error}）：{text}")
                        }),
                    );
                }
                Frame::Binary => panic!("本切片的服务端消息必须是文本帧"),
                Frame::Close(code, reason) => panic!("对端在消息之前关闭了连接：{code} {reason}"),
                Frame::Ping | Frame::Pong => continue,
            }
        }
    }

    /// 取下一条消息，并（若是认证后消息）核对服务端出站序号严格加一。
    async fn take_message(&mut self) -> Value {
        let message = self.next_message().await.expect("对端已关闭连接");
        if message.get("connectionId").is_some() {
            self.server_sequence += 1;
            assert_eq!(
                message["connectionSequence"],
                json!(self.server_sequence.to_string()),
                "服务端出站序号必须严格加一：{message}"
            );
            assert_eq!(
                message["connectionId"],
                json!(
                    self.connection_id
                        .clone()
                        .expect("认证后消息必须已有 connectionId")
                )
            );
        }
        message
    }

    /// 期待下一条消息的 `type` 是 `message_type`，并返回整条消息。
    async fn expect_type(&mut self, message_type: &str) -> Value {
        let message = self.take_message().await;
        assert_eq!(
            message["type"],
            json!(message_type),
            "步骤「{}」的消息类型不符（本方向计数 {}，上一条发出 {}）：{message}",
            self.step,
            self.outbound_sequence,
            self.last_sent_sequence.as_deref().unwrap_or("<无>")
        );
        message
    }

    /// 期待一条 `link.error` 并返回其 body（消息级拒绝）。
    async fn expect_error(&mut self, expected_code: &str) -> Value {
        let message = self.expect_type("link.error").await;
        assert_eq!(
            message["body"]["code"],
            json!(expected_code),
            "错误码不符：{message}"
        );
        message["body"].clone()
    }

    /// 读到 close 帧（中途的消息收进 `pending`），返回 close code。
    async fn expect_close(&mut self) -> u16 {
        loop {
            match self.next_frame().await {
                None => panic!("对端在没有 close 帧的情况下结束了连接"),
                Some(Frame::Text(text)) => {
                    self.pending
                        .push(serde_json::from_str(&text).expect("文本帧是 JSON"));
                }
                Some(Frame::Close(code, _reason)) => return code,
                Some(Frame::Binary | Frame::Ping | Frame::Pong) => {}
            }
        }
    }

    /// 构造并发送一条认证后消息（`connectionId`/`connectionSequence` 由客户端维护）。
    ///
    /// 序号用「本方向下一个应到的值」，但**不**推进本地计数：只有服务端确实受理后才由调用方
    /// [`Client::advance_after_accepted`] 推进（被拒的消息不推进任何一侧的序号，§2.2）。
    async fn send_post_auth(&mut self, message_type: &str, body: Value) {
        let connection_id = self
            .connection_id
            .clone()
            .expect("认证后消息必须已有 connectionId");
        let sequence = (self.outbound_sequence + 1).to_string();
        self.last_sent_sequence = Some(sequence.clone());
        let text = json!({
            "protocolVersion": 1,
            "type": message_type,
            "messageId": uuid_text(),
            "connectionId": connection_id,
            "connectionSequence": sequence,
            "body": body,
        })
        .to_string();
        self.send_text(&text).await;
    }

    /// 服务端受理了一条本方向的消息后推进序号。
    fn advance_after_accepted(&mut self) {
        self.outbound_sequence += 1;
    }

    /// 标记当前段落（失败信息里带上它，便于定位是哪一步）。
    fn step(&mut self, label: &str) {
        self.step = label.to_owned();
    }

    /// 发送一条认证后消息，但完全由调用方给出信封字段（边界用例）。
    async fn send_raw(&mut self, envelope: Value) {
        self.last_sent_sequence = envelope["connectionSequence"].as_str().map(str::to_owned);
        self.send_text(&envelope.to_string()).await;
    }

    /// 认证后消息的信封骨架（可由用例逐字段改写）。
    fn envelope(&self, message_type: &str, body: Value) -> Value {
        json!({
            "protocolVersion": 1,
            "type": message_type,
            "messageId": uuid_text(),
            "connectionId": self.connection_id.clone().expect("认证后消息"),
            "connectionSequence": (self.outbound_sequence + 1).to_string(),
            "body": body,
        })
    }

    /// 用「下一条合法序号」发送一条 `link.ping`，并期待回填同一 nonce 的 `link.pong`。
    ///
    /// 这是本文件里「连接仍然可用」的统一证据：每次拒绝之后都跑一次，证明拒绝没有让连接进入非法状态。
    async fn ping_round_trip(&mut self, seed: u8) -> Value {
        let nonce = ping_nonce(seed);
        self.send_post_auth("link.ping", json!({"nonce": nonce}))
            .await;
        let pong = self.expect_type("link.pong").await;
        assert_eq!(pong["body"]["nonce"], json!(nonce), "pong 必须原样回填");
        self.advance_after_accepted();
        pong
    }

    /// 在 `budget` 内读到一条指定 `type` 的认证后消息：沿途其他类型的消息被逐条消费（并按序号校验）。
    ///
    /// 用于「对端持续推流」的用例——那里不能假定下一条消息就是目标类型。预算用尽即失败；连接提前
    /// 结束由 `take_message` 自己报错。
    async fn expect_type_eventually(&mut self, message_type: &str, budget: Duration) -> Value {
        let deadline = Instant::now() + budget;
        loop {
            assert!(
                Instant::now() < deadline,
                "在 {budget:?} 内没有读到 {message_type}（步骤「{}」）",
                self.step
            );
            let message = self.take_message().await;
            if message["type"] == json!(message_type) {
                return message;
            }
        }
    }

    /// 在 `budget` 内读到 close 帧；中途的文本帧**直接丢弃**（不保留在 `pending` 里）。
    ///
    /// 预算是必需的而不是「反正读超时会兜底」：对端持续推流时 `read_timeout` 永远不会触发（帧一直在
    /// 到达），没有预算的话「对端根本不断开」会表现为用例挂住而不是失败。
    async fn expect_close_within(&mut self, budget: Duration) -> u16 {
        let deadline = Instant::now() + budget;
        loop {
            assert!(
                Instant::now() < deadline,
                "在 {budget:?} 内没有等到 close 帧（步骤「{}」）",
                self.step
            );
            match self.next_frame().await {
                None => panic!("对端在没有 close 帧的情况下结束了连接"),
                Some(Frame::Close(code, _reason)) => return code,
                Some(Frame::Binary | Frame::Ping | Frame::Pong | Frame::Text(_)) => {}
            }
        }
    }
}

/// 在 buffer 里找子切片。
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn uuid_text() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ---------------------------------------------------------------------------------------------
// 握手辅助（Access 侧）
// ---------------------------------------------------------------------------------------------

/// 发送 `node.hello`（认证前的第一条消息）。
async fn send_hello(client: &mut Client, access_node: &str, supported: &[&str], required: &[&str]) {
    let text = json!({
        "protocolVersion": 1,
        "type": "node.hello",
        "messageId": uuid_text(),
        "body": {
            "minProtocolVersion": 1,
            "maxProtocolVersion": 1,
            "accessNodeId": access_node,
            "role": "access",
            "clientNonce": nonce_text(0x31),
            "supportedFeatures": supported,
            "requiredFeatures": required,
        },
    })
    .to_string();
    client.send_text(&text).await;
}

/// 全部 mvp feature 加一条 Owner 不实现的 ID（协商必须只取交集）。
fn declared_features() -> Vec<&'static str> {
    let mut features: Vec<&'static str> = SUPPORTED_FEATURES.to_vec();
    features.push("node-link.node-rotation.v1");
    features
}

/// 从 `node.challenge` 的 body 取 `catalogRevision`（decimal string）并转成 transcript 用的 `u64`。
///
/// 这就是 Access 该走的路：2026-09-26 的修订把该字段放进握手消息，两个连接 transcript domain 的 tag 6
/// 都取它（§9.4）。
fn challenge_catalog_revision(challenge: &Value) -> u64 {
    challenge["body"]["catalogRevision"]
        .as_str()
        .unwrap_or_else(|| panic!("node.challenge 必须带 catalogRevision：{challenge}"))
        .parse()
        .expect("catalogRevision 是无前导零的十进制串")
}

/// 发送 `node.proof`：`mutate` 可破坏签名以覆盖该拒绝路径。
///
/// 两个输入都只从 `node.challenge` 取：`connectionId`/`serverNonce`/`selectedFeatures` 与
/// [`challenge_catalog_revision`]（Access 侧拿不到别的来源）。
async fn send_proof(client: &mut Client, access_node: &str, challenge: &Value, corrupt: bool) {
    send_proof_with_revision(
        client,
        access_node,
        challenge,
        challenge_catalog_revision(challenge),
        corrupt,
    )
    .await;
}

/// 发送 `node.proof`，并显式给出进 transcript 的 `catalogRevision`（供「取错值必然失败」的用例）。
async fn send_proof_with_revision(
    client: &mut Client,
    access_node: &str,
    challenge: &Value,
    catalog_revision: u64,
    corrupt: bool,
) {
    let body = challenge["body"].clone();
    let selected: Vec<String> = body["selectedFeatures"]
        .as_array()
        .expect("selectedFeatures")
        .iter()
        .map(|feature| feature.as_str().expect("feature id").to_owned())
        .collect();
    let input = NodeLinkProof {
        owner_node_id: node(OWNER_NODE),
        access_node_id: node(access_node),
        catalog_revision,
        client_nonce: identity_auth::Nonce::new(&nonce_text(0x31)).expect("规范 nonce"),
        server_nonce: identity_auth::Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
            .expect("规范 nonce"),
        connection_id: ChallengeId::parse(body["connectionId"].as_str().expect("connectionId"))
            .expect("规范 uuid"),
        negotiated_features: FeatureList::new(selected),
    };
    let transcript = input.transcript().expect("transcript 可装配");
    let mut signature = P1363Signature::try_from_base64url(&sign_with_access_key(&transcript))
        .expect("签名是 64 字节 P1363");
    if corrupt {
        let mut bytes = *signature.as_bytes();
        bytes[0] ^= 0xff;
        signature = P1363Signature::try_from_bytes(&bytes).expect("翻转一个字节仍是 64 字节");
    }
    let text = json!({
        "protocolVersion": 1,
        "type": "node.proof",
        "messageId": uuid_text(),
        "body": {
            "connectionId": body["connectionId"],
            "accessNodeId": access_node,
            "nodeProof": signature.to_base64url(),
        },
    })
    .to_string();
    client.send_text(&text).await;
}

/// 完成一次成功握手（hello → challenge → proof → ready），返回 `node.ready` 消息。
async fn authenticated(client: &mut Client, access_node: &str, corrupt_proof: bool) -> Value {
    send_hello(
        client,
        access_node,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = client.expect_type("node.challenge").await;
    client.connection_id = Some(
        challenge["body"]["connectionId"]
            .as_str()
            .expect("connectionId")
            .to_owned(),
    );
    send_proof(client, access_node, &challenge, corrupt_proof).await;
    client.expect_type("node.ready").await
}

/// 认证前必须省略连接字段（§2.2）：`node.challenge` 的信封检查。
fn assert_no_connection_fields(message: &Value) {
    let object = message.as_object().expect("信封是对象");
    assert!(
        !object.contains_key("connectionId") && !object.contains_key("connectionSequence"),
        "认证前消息必须省略连接字段：{message}"
    );
}

// ---------------------------------------------------------------------------------------------
// R36/R37/R40/R32：正常握手
// ---------------------------------------------------------------------------------------------

/// 已配对节点完成整条四步握手后进入业务阶段。
///
/// 逐项断言：挑战信封省略连接字段、Owner 挑战证明可验证、`node.ready` 的 `catalogRevision`/`serverEpoch`
/// 与持久化水位同源、limits 与 §2.5 默认值一致、连接已登记、首次认证在同一写集里把已批准配对推进到
/// `consumed` 并留下 `node.authenticated`，连接结束后注册表清空。
#[tokio::test]
async fn handshake_completes_and_enters_the_business_phase() {
    let harness = Harness::new().await;
    let pairing = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;

    send_hello(
        &mut client,
        ACCESS_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = client.expect_type("node.challenge").await;
    assert_no_connection_fields(&challenge);

    // R40「双向」的 Owner 侧一半：挑战证明可用本节点公钥在 `node-link-challenge/v1` 域验证。
    // `catalogRevision` 也从 wire 取（它进了两个连接 domain 的 tag 6，见文件头说明）。
    let body = challenge["body"].clone();
    let selected = feature_ids(&body);
    let catalog_revision = challenge_catalog_revision(&challenge);
    assert_eq!(
        catalog_revision, CATALOG_REVISION,
        "挑战的修订号必须与本机水位同源"
    );
    let mut expected_selected: Vec<String> = SUPPORTED_FEATURES
        .iter()
        .map(|id| (*id).to_owned())
        .collect();
    expected_selected.sort();
    assert_eq!(
        selected, expected_selected,
        "selectedFeatures 必须是对端声明与 Owner 支持集合的交集（升序）"
    );
    let owner_public_key = harness
        .world
        .authority
        .node_public_key()
        .await
        .expect("本节点公钥可用");
    NodeLinkChallenge {
        owner_node_id: node(OWNER_NODE),
        access_node_id: node(ACCESS_NODE),
        catalog_revision,
        client_nonce: identity_auth::Nonce::new(&nonce_text(0x31)).expect("规范 nonce"),
        server_nonce: identity_auth::Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
            .expect("规范 nonce"),
        connection_id: ChallengeId::parse(body["connectionId"].as_str().expect("connectionId"))
            .expect("规范 uuid"),
        negotiated_features: FeatureList::new(selected.clone()),
    }
    .verify(
        &owner_public_key,
        &P1363Signature::try_from_base64url(body["nodeProof"].as_str().expect("nodeProof"))
            .expect("挑战证明是 64 字节 P1363"),
    )
    .expect("Owner 挑战证明必须可用本节点公钥验证");

    let connection_id = body["connectionId"]
        .as_str()
        .expect("connectionId")
        .to_owned();
    client.connection_id = Some(connection_id.clone());
    send_proof(&mut client, ACCESS_NODE, &challenge, false).await;

    // R37：`node.ready` 携带 catalogRevision/limits/serverEpoch，且信封序号从 1 开始。
    let ready = client.expect_type("node.ready").await;
    assert_eq!(ready["connectionId"], json!(connection_id));
    assert_eq!(ready["body"]["ownerNodeId"], json!(OWNER_NODE));
    assert_eq!(
        ready["body"]["catalogRevision"],
        json!(CATALOG_REVISION.to_string()),
        "catalogRevision 必须与本机水位同源"
    );
    assert_eq!(ready["body"]["serverEpoch"], json!(TEST_SERVER_EPOCH));
    assert_eq!(
        ready["body"]["limits"],
        json!({
            "maxMessageBytes": DEFAULT_MAX_MESSAGE_BYTES,
            "catalogSnapshotBatchSize": DEFAULT_CATALOG_SNAPSHOT_BATCH_SIZE,
            "resourceSnapshotBatchSize": DEFAULT_RESOURCE_SNAPSHOT_BATCH_SIZE,
            "maxInFlightCommands": DEFAULT_MAX_IN_FLIGHT_COMMANDS,
            "maxPendingQueueBytes": DEFAULT_MAX_PENDING_QUEUE_BYTES,
            "maxPendingQueueMessages": DEFAULT_MAX_PENDING_QUEUE_MESSAGES,
            "heartbeatIntervalMs": DEFAULT_HEARTBEAT_INTERVAL_MS,
        }),
        "默认部署下发 §2.5 的默认值"
    );

    // 业务阶段：连接已登记，句柄的事实与握手一致。
    let handles = harness.conn.registry().handles();
    assert_eq!(handles.len(), 1, "认证成功后必须登记连接");
    let handle = &handles[0];
    assert_eq!(handle.connection_id().as_str(), connection_id);
    assert_eq!(handle.node_id(), &node(ACCESS_NODE));
    assert_eq!(handle.client_ip().to_string(), "127.0.0.1");
    assert!(!handle.saturated(), "刚建连的队列不应处于高水位");
    let _ = client.ping_round_trip(0x41).await;

    // R32（衔接侧）：首次认证的写集把已批准配对推进到 consumed，并留下 node.authenticated。
    let consumed = harness.world.trust.pairing(&pairing).expect("配对仍在库里");
    assert_eq!(
        consumed.state(),
        PairingState::Consumed,
        "首条成功认证必须消费已批准配对"
    );
    assert!(
        consumed.terminal_at().is_some(),
        "consumed 必须带终态时间（§11.2）"
    );
    let auth_rows = harness
        .audit_rows(vec![AuditAction::NodeAuthenticated])
        .await;
    assert_eq!(auth_rows.len(), 1, "首条成功认证留一行审计：{auth_rows:?}");
    assert_eq!(auth_rows[0].outcome(), AuditOutcome::Success);
    assert_eq!(auth_rows[0].via_node(), Some(&node(ACCESS_NODE)));
    assert_eq!(
        auth_rows[0].target(),
        &EntityRef::Pairing(pairing_id(&pairing)),
        "首次认证的审计目标是那次配对"
    );
    assert_eq!(
        auth_rows[0].local_principal_ref(),
        None,
        "节点级信任模型下 localPrincipalRef 为空（§8.3）"
    );
    assert_eq!(auth_rows[0].detail_digest(), None, "握手不写细节摘要");
    assert_eq!(
        harness
            .view(ACCESS_NODE)
            .await
            .pairing
            .map(|record| record.state()),
        Some(PairingState::Consumed),
        "重新读快照时不再有待消费的配对"
    );

    // 连接结束后注册表清空（撤销传播与关闭路径依赖这一条）。
    client.send_close().await;
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "会话结束后必须从注册表摘除",
    )
    .await;
    harness.stop().await;
}

/// 该节点 `access` 行上的 `last_connected_at`（管理视图 `lastSeenAt` 的同一列）。
fn last_connected_at(harness: &Harness) -> Option<String> {
    harness
        .world
        .trust
        .nodes_of(ACCESS_NODE)
        .into_iter()
        .find(|record| record.kind() == NodeKind::Access)
        .and_then(|record| record.last_connected_at().map(|at| at.as_str().to_owned()))
}

/// §11.6 第 9 条：认证收尾把 `owned_node.last_connected_at` 推进到本次认证时间。
///
/// 三段落分别对应的三件事：① 首次认证（有已批准配对的消费写集）必须让它非空；② 重复认证（没有
/// 待消费配对）继续推进；③ 更早的认证时间不得倒退已存值（「只前进」）。每次成功认证都留一行
/// `node.authenticated`——首次由消费写集写，重复认证与 `last_connected_at` 同一事务写。
#[tokio::test]
async fn authentication_advances_the_node_last_connected_time() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let first_auth_at = harness.world.clock.text();

    // ① 首次认证。
    let mut first = Client::connect(harness.addr).await;
    let _ = authenticated(&mut first, ACCESS_NODE, false).await;
    assert_eq!(
        last_connected_at(&harness),
        Some(first_auth_at.clone()),
        "首次认证的收尾写集必须写下 last_connected_at"
    );
    first.send_close().await;
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "第一条连接结束后必须从注册表摘除",
    )
    .await;

    // ② 重复认证：没有待消费配对，但时间仍然推进到本次认证时刻。
    const SECOND_AUTH_AT: &str = "2026-09-18T09:12:04.412Z";
    harness.world.clock.set(SECOND_AUTH_AT);
    let mut second = Client::connect(harness.addr).await;
    let _ = authenticated(&mut second, ACCESS_NODE, false).await;
    assert_eq!(
        last_connected_at(&harness),
        Some(SECOND_AUTH_AT.to_owned()),
        "重复认证必须推进 last_connected_at"
    );
    second.send_close().await;
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "第二条连接结束后必须从注册表摘除",
    )
    .await;

    // ③ 时钟回拨：更早的认证时间不得覆盖已存值。
    harness.world.clock.set("2026-09-18T09:12:03.000Z");
    let mut third = Client::connect(harness.addr).await;
    let _ = authenticated(&mut third, ACCESS_NODE, false).await;
    assert_eq!(
        last_connected_at(&harness),
        Some(SECOND_AUTH_AT.to_owned()),
        "`last_connected_at` 只前进不倒退"
    );
    let rows = harness
        .audit_rows(vec![AuditAction::NodeAuthenticated])
        .await;
    assert_eq!(rows.len(), 3, "每次成功认证各留一行：{rows:?}");
    assert!(
        rows.iter().all(|row| row.outcome() == AuditOutcome::Success
            && row.via_node() == Some(&node(ACCESS_NODE))),
        "重复认证的归因与首次一致：{rows:?}"
    );
    harness.stop().await;
}

/// 在途握手配额只在握手阶段内持有——同源第 5 条**已认证**连接不被拒绝。
///
/// 句子里的「同源」是真实条件：所有连接都来自 `127.0.0.1`。五条连接全部保持打开（句柄不因连接
/// 结束而释放），因此本用例锁定的区别只有一处：配额在认证完成时释放，而不是在会话结束时。
#[tokio::test]
async fn authenticated_connections_do_not_hold_the_in_flight_handshake_quota() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut clients = Vec::new();
    for _ in 0..MAX_IN_FLIGHT_HANDSHAKES_PER_IP + 1 {
        let mut client = Client::connect(harness.addr).await;
        let _ = authenticated(&mut client, ACCESS_NODE, false).await;
        clients.push(client);
    }
    assert_eq!(
        harness.conn.registry().active(),
        MAX_IN_FLIGHT_HANDSHAKES_PER_IP + 1,
        "同源已认证连接不得被在途握手配额拒绝"
    );
    assert_eq!(harness.conn.in_flight_handshakes(), 0, "握手完成即释放配额");
    // 最后一条仍然可以往来（拒绝路径的副产物不会残留到业务阶段）。
    clients
        .last_mut()
        .expect("第 5 条连接")
        .ping_round_trip(0x91)
        .await;
    harness.stop().await;
}

/// 在途握手配额的拒绝路径——同源并发 5 条**未认证**连接的第 5 条被拒。
///
/// 前 4 条各自完成 `node.hello`（拿到 `node.challenge` 就证明它的会话已持有配额）并停在等 proof 阶段；
/// 第 5 条在会话第一步（读到任何帧之前）就被拒。两种极端的区别就是 F3 的修复点：未认证的连接仍然
/// 占配额，已认证的不占。
#[tokio::test]
async fn the_fifth_in_flight_handshake_from_one_address_is_refused() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut half_open = Vec::new();
    for _ in 0..MAX_IN_FLIGHT_HANDSHAKES_PER_IP {
        let mut client = Client::connect(harness.addr).await;
        send_hello(
            &mut client,
            ACCESS_NODE,
            &declared_features(),
            &[REQUIRED_FEATURE],
        )
        .await;
        let _ = client.expect_type("node.challenge").await;
        half_open.push(client);
    }
    assert_eq!(
        harness.conn.in_flight_handshakes(),
        MAX_IN_FLIGHT_HANDSHAKES_PER_IP,
        "半开握手必须占配额"
    );

    let mut fifth = Client::connect(harness.addr).await;
    let error = fifth.expect_error("nodelink.resource.rate_limited").await;
    assert_eq!(error["retryable"], json!(true), "限流可重试：{error}");
    assert_eq!(
        error["details"],
        json!({}),
        "在途配额拒绝没有重试等待时间（与认证限流区分）"
    );
    assert_eq!(fifth.expect_close().await, 4429, "配额耗尽以 4429 关闭");
    // 零副作用：没有新增信任行、没有成功认证审计、没有连接登记。
    assert_eq!(
        harness.world.trust.node_count(),
        1,
        "只有配对批准写入的那一行"
    );
    assert!(
        harness
            .audit_rows(vec![AuditAction::NodeAuthenticated])
            .await
            .is_empty()
    );
    assert_eq!(harness.conn.registry().active(), 0);
    drop(half_open);
    harness.stop().await;
}

/// 单 IP 新认证尝试的固定限流（§2.5：10 次/分钟）——同一 IP 的第 11 次建连被拒。
///
/// 前 10 次建连都在会话开始后立即断开：每次都必须先占用配额（`in_flight_handshakes() == 1` 证明
/// `run()` 已经跑过限流判定）再释放，因此两条闸门不会互相干扰（否则第 5 次就会撞上在途配额，
/// 用例会验到另一条拒绝路径上去）。
#[tokio::test]
async fn the_eleventh_authentication_attempt_from_one_address_is_rate_limited() {
    let harness = Harness::new().await;
    for _ in 0..AUTH_ATTEMPTS_PER_MINUTE {
        let client = Client::connect(harness.addr).await;
        assert_eventually(
            || harness.conn.in_flight_handshakes() == 1,
            "会话必须已开始并占用配额",
        )
        .await;
        drop(client);
        assert_eventually(
            || harness.conn.in_flight_handshakes() == 0,
            "断开后配额必须释放",
        )
        .await;
    }

    let mut eleventh = Client::connect(harness.addr).await;
    let error = eleventh
        .expect_error("nodelink.resource.rate_limited")
        .await;
    assert_eq!(error["retryable"], json!(true), "限流可重试：{error}");
    assert!(
        error["details"]["retryAfterMs"]
            .as_u64()
            .is_some_and(|millis| millis > 0),
        "必须给出重试等待毫秒数：{error}"
    );
    assert_eq!(eleventh.expect_close().await, 4429, "限流以 4429 关闭");
    // 零副作用：没有信任行、没有审计、没有连接登记。
    assert_eq!(harness.world.trust.node_count(), 0);
    assert!(harness.audit_rows(vec![]).await.is_empty());
    assert_eq!(harness.conn.registry().active(), 0);
    harness.stop().await;
}

/// 挑战/就绪消息 body 的 `selectedFeatures`。
fn feature_ids(body: &Value) -> Vec<String> {
    body["selectedFeatures"]
        .as_array()
        .expect("selectedFeatures")
        .iter()
        .map(|feature| feature.as_str().expect("feature id").to_owned())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// 2.29 的验收用例：`node.challenge.catalogRevision` 是 proof transcript 的 tag 6 来源
// ---------------------------------------------------------------------------------------------

/// （任务 2.29 的验收项，design D13）：Access 只能从 `node.challenge` 拿到 `catalogRevision`，
/// 用它构造的 `node-link-proof/v1` transcript 必须通过验签；改用别的值必然 `proof_invalid`。
///
/// 反例是这条用例的关键：它证明该字段真的进了验签输入（Owner 用签发挑战时记录的同一个值验证），
/// 而不是被忽略的装饰字段；正例则走完整路径（挑战 → proof → `node.ready`）并核对 `node.ready` 与
/// 挑战同源。
#[tokio::test]
async fn the_challenge_catalog_revision_is_the_proof_transcript_source() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;

    // 反例：拿挑战里的修订号 +1 签名 → proof_invalid + 4401（配对不被消费，留给下面的正例）。
    let mut wrong = Client::connect(harness.addr).await;
    send_hello(
        &mut wrong,
        ACCESS_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = wrong.expect_type("node.challenge").await;
    let revision = challenge_catalog_revision(&challenge);
    send_proof_with_revision(&mut wrong, ACCESS_NODE, &challenge, revision + 1, false).await;
    wrong.expect_error("nodelink.auth.proof_invalid").await;
    assert_eq!(wrong.expect_close().await, 4401, "取错修订号以 4401 关闭");

    // 正例：同一个修订号（从 wire 读的那个）签出来的 proof 必须被接受。
    let mut client = Client::connect(harness.addr).await;
    let ready = authenticated(&mut client, ACCESS_NODE, false).await;
    assert_eq!(
        ready["body"]["catalogRevision"],
        json!(revision.to_string()),
        "node.ready 与 node.challenge 的修订号必须同源"
    );
    let _ = client.ping_round_trip(0x6a).await;
    harness.stop().await;
}

/// 在预算内轮询一个条件（异步状态收敛的断言，失败时给出固定消息）。
async fn assert_eventually(mut condition: impl FnMut() -> bool, message: &str) {
    let deadline = Instant::now() + IO_TIMEOUT;
    while !condition() {
        assert!(Instant::now() < deadline, "等待超时：{message}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ---------------------------------------------------------------------------------------------
// R38/R39：认证前的消息准入
// ---------------------------------------------------------------------------------------------

/// 握手前收到业务消息 → `link.error`（`type_unsupported`）4401 关闭，且不产生任何副作用。
#[tokio::test]
async fn a_business_message_before_hello_is_closed_with_4401() {
    let harness = Harness::new().await;
    let mut client = Client::connect(harness.addr).await;

    // `command.submit` 只允许在认证后出现，因此信封形状按认证后给出（带连接字段）。
    let text = json!({
        "protocolVersion": 1,
        "type": "command.submit",
        "messageId": uuid_text(),
        "connectionId": uuid_text(),
        "connectionSequence": "1",
        "body": {},
    })
    .to_string();
    client.send_text(&text).await;
    let error = client
        .expect_error("nodelink.protocol.type_unsupported")
        .await;
    assert_eq!(error["retryable"], json!(false), "协议类错误不可重试");
    assert_eq!(
        client.expect_close().await,
        4401,
        "认证前的业务消息以 4401 关闭"
    );

    // 没有副作用：没有信任行、没有审计、没有连接登记。
    assert_eq!(harness.world.trust.node_count(), 0);
    assert!(harness.audit_rows(vec![]).await.is_empty());
    assert_eq!(harness.conn.registry().active(), 0);
    harness.stop().await;
}

/// 对端要求 Owner 不实现的 feature → `feature_required` `details.features` 关闭。
#[tokio::test]
async fn a_missing_required_feature_is_reported_with_details_and_closed() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;

    send_hello(
        &mut client,
        ACCESS_NODE,
        &declared_features(),
        &["node-link.node-rotation.v1", REQUIRED_FEATURE],
    )
    .await;
    let error = client
        .expect_error("nodelink.protocol.feature_required")
        .await;
    assert_eq!(
        error["details"]["features"],
        json!(["node-link.node-rotation.v1"]),
        "details.features 必须给出排序去重后未被选择的必需 feature"
    );
    assert_eq!(
        client.expect_close().await,
        4400,
        "未进入业务阶段即以 4400 关闭"
    );
    // 未进入业务阶段，也不消费配对。
    assert_eq!(
        harness
            .world
            .trust
            .pairing(&harness.pairing_of(ACCESS_NODE).await)
            .map(|record| record.state()),
        Some(PairingState::Approved),
        "协商失败不得消费配对"
    );
    assert_eq!(harness.conn.registry().active(), 0);
    harness.stop().await;
}

/// 信封 `protocolVersion` 不是 1 → `version_unsupported` 4406。
#[tokio::test]
async fn an_envelope_version_other_than_v1_is_closed_with_4406() {
    let harness = Harness::new().await;
    let mut client = Client::connect(harness.addr).await;
    let text = json!({
        "protocolVersion": 2,
        "type": "node.hello",
        "messageId": uuid_text(),
        "body": {},
    })
    .to_string();
    client.send_text(&text).await;
    let error = client
        .expect_error("nodelink.protocol.version_unsupported")
        .await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4406, "版本不兼容以 4406 关闭");
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R36：握手超时
// ---------------------------------------------------------------------------------------------

/// 握手窗口内没有 `node.hello` → 15 秒后以 4408 关闭，且不留认证失败审计（对端身份未知）。
///
/// 这里必须等满真实的 15 秒：窗口是 §2.5 的固定常量，缩短它就无法证明生产路径上的窗口是 15 秒。
#[tokio::test]
async fn a_handshake_that_never_gets_a_hello_is_closed_with_4408() {
    let harness = Harness::new().await;
    // 计时起点必须在**连接之前**：服务端的 15 秒窗口从会话任务开始起算（即 WS 升级完成之后由
    // `handshake_phase` 取 `now + HANDSHAKE_TIMEOUT`，见 `node_link/conn/session.rs:348`），
    // 而 `connect_with_timeout` 还要完成 TCP 连接、WS 升级并读到 101 响应才返回。若起点在它之后，
    // 本断言的余量恰好等于这段偏移（负载下可达毫秒级），于是「窗口必须满 15 秒」会随机差几毫秒
    // 而失败（CI 实测 14.998687621s，差 1.3ms）。起点提前后 `elapsed >= HANDSHAKE_TIMEOUT`
    // 才真正表达「服务端等满了窗口」。
    let started = Instant::now();
    let mut client = Client::connect_with_timeout(harness.addr, TIMEOUT_WINDOW_IO).await;
    assert_eq!(client.expect_close().await, 4408, "握手超时以 4408 关闭");
    assert!(
        started.elapsed() >= HANDSHAKE_TIMEOUT,
        "关闭必须发生在 15 秒窗口之后，实际 {:?}",
        started.elapsed()
    );
    assert!(
        harness
            .audit_rows(vec![AuditAction::NodeAuthFailed])
            .await
            .is_empty(),
        "还不知道对端身份的时刻不写认证失败审计（无法归因）"
    );
    assert_eq!(harness.conn.registry().active(), 0);
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R41/R42/R43：认证结果
// ---------------------------------------------------------------------------------------------

/// 签名被破坏的 proof → `proof_invalid` 4401 `node.auth_failed` 审计，信任记录不变。
#[tokio::test]
async fn an_invalid_proof_is_closed_with_4401_and_audited() {
    let harness = Harness::new().await;
    let pairing = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    send_hello(
        &mut client,
        ACCESS_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = client.expect_type("node.challenge").await;
    send_proof(&mut client, ACCESS_NODE, &challenge, true).await;
    let error = client.expect_error("nodelink.auth.proof_invalid").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4401, "证明无效以 4401 关闭");

    let rows = harness.audit_rows(vec![AuditAction::NodeAuthFailed]).await;
    assert_eq!(rows.len(), 1, "认证失败必须留痕：{rows:?}");
    assert_eq!(rows[0].outcome(), AuditOutcome::Failed);
    assert_eq!(
        rows[0].actor(),
        &acp_core::model::Actor::Node {
            node: node(ACCESS_NODE),
            access_node: node(ACCESS_NODE),
        }
    );
    assert_eq!(rows[0].target(), &EntityRef::Node(node(ACCESS_NODE)));
    assert_eq!(rows[0].via_node(), Some(&node(ACCESS_NODE)));
    assert_eq!(
        rows[0].detail_digest(),
        None,
        "失败原因不进审计行，只进日志"
    );

    // 「信任记录不受影响」：配对仍是 approved（未被消费）、节点仍是 paired、没有连接登记。
    assert_eq!(
        harness
            .world
            .trust
            .pairing(&pairing)
            .map(|record| record.state()),
        Some(PairingState::Approved)
    );
    assert_eq!(
        harness.world.trust.nodes_of(ACCESS_NODE)[0].state(),
        NodeState::Paired
    );
    assert_eq!(harness.conn.registry().active(), 0);
    harness.stop().await;
}

/// 的第二个分支：nonce 对不上（签名本身有效，但为另一个挑战签发）同样 `proof_invalid`。
#[tokio::test]
async fn a_proof_for_another_challenge_is_rejected_with_the_same_code() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    send_hello(
        &mut client,
        ACCESS_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = client.expect_type("node.challenge").await;
    // 用另一个 serverNonce 装配 proof（等价于把挑战 A 的证明重放到挑战 B）。
    let mut tampered = challenge.clone();
    tampered["body"]["serverNonce"] = json!(nonce_text(0x77));
    send_proof(&mut client, ACCESS_NODE, &tampered, false).await;
    let error = client.expect_error("nodelink.auth.proof_invalid").await;
    assert_eq!(error["code"], json!("nodelink.auth.proof_invalid"));
    assert_eq!(client.expect_close().await, 4401);
    assert_eq!(
        harness
            .audit_rows(vec![AuditAction::NodeAuthFailed])
            .await
            .len(),
        1
    );
    harness.stop().await;
}

/// 已撤销节点即使交出签名有效的 proof 也被拒 → `node_revoked` 4410。
#[tokio::test]
async fn a_revoked_node_is_closed_with_4410() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    harness.revoke_node(ACCESS_NODE).await;
    let mut client = Client::connect(harness.addr).await;
    send_hello(
        &mut client,
        ACCESS_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = client.expect_type("node.challenge").await;
    // 签名仍然有效：验签材料作为墓碑保留（撤销不删公钥，§11.6 第 5 条）。
    send_proof(&mut client, ACCESS_NODE, &challenge, false).await;
    let error = client.expect_error("nodelink.auth.node_revoked").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4410, "已撤销节点以 4410 关闭");
    let rows = harness.audit_rows(vec![AuditAction::NodeAuthFailed]).await;
    assert_eq!(rows.len(), 1, "被撤销节点的连接尝试要留痕");
    assert_eq!(rows[0].outcome(), AuditOutcome::Denied);
    assert_eq!(harness.conn.registry().active(), 0, "不得进入业务阶段");
    harness.stop().await;
}

/// 从未配对的节点照常拿到挑战（不用错误区分存在性），proof 阶段失败并映射为 `node_unknown`。
#[tokio::test]
async fn an_unknown_node_gets_a_challenge_and_fails_as_node_unknown() {
    let harness = Harness::new().await;
    let mut client = Client::connect(harness.addr).await;
    send_hello(
        &mut client,
        UNKNOWN_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    // 未知节点也照常签发挑战（用 `daemon.public_origin` 推出本机 endpoint）。
    let challenge = client.expect_type("node.challenge").await;
    assert_no_connection_fields(&challenge);
    assert!(
        !feature_ids(&challenge["body"]).is_empty(),
        "未知节点不因此降低 feature 协商结果"
    );
    send_proof(&mut client, UNKNOWN_NODE, &challenge, false).await;
    let error = client.expect_error("nodelink.auth.node_unknown").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4401);
    let rows = harness.audit_rows(vec![AuditAction::NodeAuthFailed]).await;
    assert_eq!(rows.len(), 1, "未知节点的尝试同样留痕");
    assert_eq!(rows[0].outcome(), AuditOutcome::Denied);
    assert_eq!(
        harness.world.trust.node_count(),
        0,
        "未知节点的失败不得创建信任行"
    );
    let view = harness.view(UNKNOWN_NODE).await;
    assert!(view.node.is_none() && view.public_key.is_none() && view.pairing.is_none());
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R44/R45/R46：limits
// ---------------------------------------------------------------------------------------------

/// `node.ready.limits` 只下调、与连接内部判定同一份数字，固定常量不受配置影响。
#[tokio::test]
async fn node_ready_echoes_the_negotiated_limits_and_never_raises_them() {
    let harness = Harness::start(
        TestWorld::with_store(Arc::new(FixedStore::new(
            TEST_SERVER_EPOCH,
            STORE_HEAD_SEQUENCE,
        ))),
        NodeLinkConfig {
            public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
            // 三个下调项 + 两个试图上调的配置。
            max_in_flight_commands: 8,
            heartbeat_interval_ms: 15_000,
            max_pending_queue_messages: 10,
            max_message_bytes: u64::MAX,
            catalog_snapshot_batch_size: 4_096,
            ..NodeLinkConfig::default()
        },
    )
    .await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let ready = authenticated(&mut client, ACCESS_NODE, false).await;

    assert_eq!(
        ready["body"]["limits"],
        json!({
            "maxMessageBytes": DEFAULT_MAX_MESSAGE_BYTES,
            "catalogSnapshotBatchSize": DEFAULT_CATALOG_SNAPSHOT_BATCH_SIZE,
            "resourceSnapshotBatchSize": DEFAULT_RESOURCE_SNAPSHOT_BATCH_SIZE,
            "maxInFlightCommands": 8,
            "maxPendingQueueBytes": DEFAULT_MAX_PENDING_QUEUE_BYTES,
            "maxPendingQueueMessages": 10,
            "heartbeatIntervalMs": 15_000,
        }),
        "下调生效、上调回落默认值（R45），未出现的项取默认值"
    );

    // 下发的数字就是连接内部判定用的数字（避免「下发一套、自己按另一套」）。
    let handle = harness.conn.registry().handles()[0].clone();
    assert_eq!(handle.limits().max_in_flight_commands(), 8);
    assert_eq!(handle.limits().max_pending_queue_messages(), 10);
    assert_eq!(
        handle.limits().heartbeat_interval(),
        Duration::from_millis(15_000)
    );
    assert_eq!(
        handle.limits().max_pending_queue_bytes(),
        DEFAULT_MAX_PENDING_QUEUE_BYTES
    );
    harness.stop().await;
}

/// 握手超时与心跳静默超时是固定常量（15 s 90 s），不是配置项、也不随协商变化。
#[test]
fn protocol_constants_are_not_configurable() {
    assert_eq!(HANDSHAKE_TIMEOUT, Duration::from_secs(15));
    assert_eq!(HEARTBEAT_SILENCE_TIMEOUT, Duration::from_secs(90));
    // 配置里根本没有这两个键：`NodeLinkConfig` 只有 §2.5 表里标为「可下调」的七项 + `public_origin`。
    let config = NodeLinkConfig {
        max_message_bytes: u64::MAX,
        catalog_snapshot_batch_size: u64::MAX,
        resource_snapshot_batch_size: u64::MAX,
        max_in_flight_commands: u64::MAX,
        max_pending_queue_bytes: u64::MAX,
        max_pending_queue_messages: u64::MAX,
        heartbeat_interval_ms: u64::MAX,
        public_origin: None,
    };
    let limits = crate::node_link::conn::SessionLimits::negotiate(&config);
    assert!(
        limits.max_message_bytes() <= DEFAULT_MAX_MESSAGE_BYTES
            && u64::from(limits.max_in_flight_commands()) <= DEFAULT_MAX_IN_FLIGHT_COMMANDS
            && limits.max_pending_queue_messages() <= DEFAULT_MAX_PENDING_QUEUE_MESSAGES as usize
            && limits.heartbeat_interval() <= Duration::from_millis(DEFAULT_HEARTBEAT_INTERVAL_MS),
        "任何配置都只能下调，不可能把任何限额抬到 §2.5 默认值以上"
    );

    // §2.5 的三个 JSON 结构上限同样不可下调、也不进 `node.ready.limits`：取值只来自协议 crate 的常量，
    // `NodeLinkConfig` 里根本没有对应键（上面的结构体字面量就是编译期证据）。
    assert_eq!(node_link_protocol::structure::MAX_NESTING_DEPTH, 64);
    assert_eq!(node_link_protocol::structure::MAX_OBJECT_FIELDS, 1_024);
    assert_eq!(node_link_protocol::structure::MAX_ARRAY_ELEMENTS, 10_000);
}

/// §2.5 的三个固定 JSON 结构上限在真实连接的 wire 解码边界生效。
///
/// 判定在协议层（`node_link_protocol::structure`，§2.4 的「长度限制必须在 wire DTO 边界验证」），
/// 适配器只负责把越界映射到既有的 decode 错误路径（`nodelink.protocol.schema_invalid`，消息级拒绝、
/// 连接保持可用）。用例用**认证前**的 `link.error` 帧：它的 `details` 是 §2.4 的开放扩展点，可以承载边界
/// 大小的结构，同时证明结构判定先于阶段/type 判定。
#[tokio::test]
async fn json_structure_limits_are_enforced_on_the_wire() {
    let harness = Harness::new().await;
    let mut client = Client::connect(harness.addr).await;

    // 反例：三条上限各越界一格 —— 每条都是消息级拒绝（不回 4400/4401，不关连接）。
    let over_limit = [
        (
            "① JSON 嵌套深度 65",
            nested_details(MAX_NESTING_DEPTH - DEPTH_OUTSIDE_DETAILS + 1),
        ),
        ("② 单对象字段数 1025", object_details(MAX_OBJECT_FIELDS + 1)),
        (
            "③ 单数组元素数 10001",
            array_details(MAX_ARRAY_ELEMENTS + 1),
        ),
    ];
    for (label, details) in &over_limit {
        client.step(label);
        client.send_text(&structure_frame(details.clone())).await;
        client
            .expect_error("nodelink.protocol.schema_invalid")
            .await;
    }

    // 正例：三条上限的边界值必须被接受。对端对它们不给任何回应，因此下面的握手是断言：
    // 若其中任何一条被拒（`link.error`），`expect_type("node.challenge")` 读到的第一条消息就会是它。
    client.step("④ 边界正例：64 / 1024 / 10000");
    for details in [
        nested_details(MAX_NESTING_DEPTH - DEPTH_OUTSIDE_DETAILS),
        object_details(MAX_OBJECT_FIELDS),
        array_details(MAX_ARRAY_ELEMENTS),
    ] {
        client.send_text(&structure_frame(details)).await;
    }
    send_hello(
        &mut client,
        ACCESS_NODE,
        &declared_features(),
        &[REQUIRED_FEATURE],
    )
    .await;
    let challenge = client.expect_type("node.challenge").await;
    assert!(
        !challenge["body"]["catalogRevision"].is_null(),
        "边界正例之后握手必须照常完成：{challenge}"
    );
    harness.stop().await;
}

/// `details` 之外的固定嵌套层数（信封 → `body` → `details`）；与协议层测试（`tests/structure_limits.rs`）同口径。
const DEPTH_OUTSIDE_DETAILS: usize = 3;

/// 一条认证前的 `link.error` 帧，`details` 由调用方给出（§2.4 的开放扩展点）。
fn structure_frame(details: Value) -> String {
    json!({
        "protocolVersion": 1,
        "type": "link.error",
        "messageId": uuid_text(),
        "body": {
            "code": "nodelink.protocol.invalid_json",
            "message": "x",
            "retryable": false,
            "correlationId": null,
            "details": details,
        },
    })
    .to_string()
}

/// `{"deep": [[…]]}`：`levels` 层嵌套数组，最内层是含括号与转义引号的字符串
/// （整帧深度 = [`DEPTH_OUTSIDE_DETAILS`] + `levels`）。
fn nested_details(levels: usize) -> Value {
    let mut value = Value::String("[{\"]".to_owned());
    for _ in 0..levels {
        value = Value::Array(vec![value]);
    }
    json!({ "deep": value })
}

/// `{"v": {"f0":0,…}}`：内层对象恰好 `count` 个成员。
fn object_details(count: usize) -> Value {
    let members: serde_json::Map<String, Value> = (0..count)
        .map(|index| (format!("f{index}"), json!(index)))
        .collect();
    json!({ "v": Value::Object(members) })
}

/// `{"items": [0,1,…]}`：恰好 `count` 个元素的数组。
fn array_details(count: usize) -> Value {
    let items: Vec<Value> = (0..count).map(|index| json!(index)).collect();
    json!({ "items": items })
}

// ---------------------------------------------------------------------------------------------
// R47/R48/R49/R50：信封、序号与 type
// ---------------------------------------------------------------------------------------------

/// 已认证连接上的信封/序号/type 违规判定。
///
/// 断言的三件事同等重要：（1）错误码与信封形状符合 §2.2/§14.1；（2）消息级拒绝**不关闭**连接——
/// 每次拒绝后都跑一次 `link.ping`/`link.pong` 往返；（3）入站序号记账只看「帧文本能不能读出本连接的
/// 下一个序号」：读得出（形状错误、未知 type、post_mvp）就推进，读不出（非 JSON、连接字段不成对）就
/// 不推进。因此每个段落之后的那次往返必须与「推不推进」一致——这正是回退修法时会变红的地方。
#[tokio::test]
async fn envelope_and_sequence_violations_are_rejected_without_closing() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;

    client.step("① 序号跳号");
    // ① 序号跳号（期望 1，给 2）→ sequence_invalid。
    let mut envelope = client.envelope("link.ping", json!({"nonce": ping_nonce(0x51)}));
    envelope["connectionSequence"] = json!("2");
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.sequence_invalid")
        .await;
    client.ping_round_trip(0x52).await;

    client.step("② 序号回退");
    // ② 序号回退（期望 2，给 1）→ sequence_invalid（R48）。
    let mut envelope = client.envelope("link.ping", json!({"nonce": ping_nonce(0x53)}));
    envelope["connectionSequence"] = json!("1");
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.sequence_invalid")
        .await;
    client.ping_round_trip(0x54).await;

    client.step("③ connectionId 不匹配");
    // ③ connectionId 不匹配 → sequence_invalid。
    let mut envelope = client.envelope("link.ping", json!({"nonce": ping_nonce(0x55)}));
    envelope["connectionId"] = json!(uuid_text());
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.sequence_invalid")
        .await;
    client.ping_round_trip(0x56).await;

    client.step("④ 只给 connectionId");
    // ④ 只给 connectionId、不给 connectionSequence → schema_invalid（信封形状错）。成对的连接字段读不
    //    出来，因此这条拒绝**不**推进序号：随后那条合法消息仍用同一个序号（对端没有为它消耗下一个）。
    let envelope = json!({
        "protocolVersion": 1,
        "type": "link.ping",
        "messageId": uuid_text(),
        "connectionId": client.connection_id.clone().expect("connectionId"),
        "body": {"nonce": ping_nonce(0x57)},
    });
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.schema_invalid")
        .await;
    client.ping_round_trip(0x58).await;

    client.step("⑤ 未知信封字段");
    // ⑤ 未知信封字段 → schema_invalid（控制信封是 closed object）。信封里其余字段都合法，连接字段与序号
    //    都读得出，因此这条拒绝照常推进入站期望序号（§2.2 的序号属于「对端发出的消息」，形状是否被接受
    //    不影响它已被消耗）。与 ④ 的差别只在「读不读得出序号」：④ 缺 connectionSequence，这一条不缺。
    let mut envelope = client.envelope("link.ping", json!({"nonce": ping_nonce(0x59)}));
    envelope["surprise"] = json!(1);
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.schema_invalid")
        .await;
    client.advance_after_accepted();
    client.ping_round_trip(0x5a).await;

    client.step("⑥ 未知 body 字段");
    // ⑥ 未知 body 字段 → schema_invalid（R50）。信封本身合法，因此该序号已被对端占用
    //    （§2.2 的序号属于「对端发出的消息」，body 是否被接受不影响它）。
    client
        .send_post_auth("link.ping", json!({"nonce": ping_nonce(0x5b), "extra": 1}))
        .await;
    client
        .expect_error("nodelink.protocol.schema_invalid")
        .await;
    client.advance_after_accepted();
    client.ping_round_trip(0x5c).await;

    client.step("⑦ 非法 JSON");
    // ⑦ 不是合法 JSON → invalid_json（消息不生效、连接可用；信封无法解析，因此序号未被占用）。
    client.send_text("{not json").await;
    client.expect_error("nodelink.protocol.invalid_json").await;
    client.ping_round_trip(0x5d).await;

    client.step("⑧ 未知 type");
    // ⑧ 未知 type → type_unsupported（不泄露「存在但未实现」与「完全未知」的差异）。信封结构与 JSON
    //    都合法，因此该序号已被对端占用（§2.2 的序号记账与 type 无关，与 post_mvp 同口径）。
    client.send_post_auth("node.future.thing", json!({})).await;
    client
        .expect_error("nodelink.protocol.type_unsupported")
        .await;
    client.advance_after_accepted();
    client.ping_round_trip(0x5e).await;

    client.step("⑨ post_mvp");
    // ⑨ post_mvp 消息显式拒绝（R49）：用 manifest 里登记为「schema 合法」的两份 fixture。
    for (index, fixture) in ["catalog-changed.json", "link-backpressure.json"]
        .iter()
        .enumerate()
    {
        let case: Value = read_fixture(&format!("valid/{fixture}"));
        let mut envelope = case.clone();
        envelope["messageId"] = json!(uuid_text());
        envelope["connectionId"] = json!(client.connection_id.clone().expect("connectionId"));
        envelope["connectionSequence"] = json!((client.outbound_sequence + 1).to_string());
        client.send_raw(envelope).await;
        client
            .expect_error("nodelink.protocol.type_unsupported")
            .await;
        // post_mvp 消息的信封合法，因此它的序号已被对端占用（§2.2），连接不能因一次显式拒绝而错位。
        client.advance_after_accepted();
        client.ping_round_trip(0x60 + index as u8).await;
    }

    client.step("⑩ 未实现的 v1 type");
    // ⑩ 本切片（WP4）还没有路由的 v1 业务 type：显式 `type_unsupported` 而不是静默丢弃。
    let case: Value = read_fixture("valid/resource-ack.json");
    let mut envelope = case.clone();
    envelope["messageId"] = json!(uuid_text());
    envelope["connectionId"] = json!(client.connection_id.clone().expect("connectionId"));
    envelope["connectionSequence"] = json!((client.outbound_sequence + 1).to_string());
    client.send_raw(envelope).await;
    client
        .expect_error("nodelink.protocol.type_unsupported")
        .await;
    client.advance_after_accepted();
    client.ping_round_trip(0x7f).await;

    harness.stop().await;
}

/// §2.2：**已知 `type` 但帧另有形状类错误**同样是消息级拒绝，那一次拒绝也照常推进入站期望序号——不推进
/// 会让对端此后每条消息都被判 `nodelink.protocol.sequence_invalid`，连接退化成只出不进（收到 90 秒静默
/// 超时为止）。三个段落各覆盖一类：未登记的信封字段（closed object）、缺 `messageId`（必需字段）、§2.5 的
/// JSON 结构超限（判定在解析之前，但同样已经在 wire 上占用了对端的下一个序号）。
///
/// 与词表外 `type` 的段落（上面那个大用例的 ⑧）不同，这几条帧的 `type` 都在词表里，`Envelope::decode`
/// 因 `deny_unknown_fields`/缺必需字段/结构超限失败，落在 `judge` 里 `Malformed` 一族与 `Structure` 的
/// 兜底分支上，因此序号记账必须在那些分支也成立。必要性由每段末尾的合法消息证明：只回退对应分支的序号
/// 推进时，它会被判 `sequence_invalid`（而不是 `link.pong`）而变红。
#[tokio::test]
async fn a_shape_invalid_envelope_that_consumed_the_sequence_does_not_strand_the_connection() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;

    // ① 未登记的信封字段：连接字段与序号都正好是下一个，但信封是 closed object → schema_invalid。
    client.step("信封多一个未登记字段");
    let mut extra = client.envelope("link.ping", json!({"nonce": ping_nonce(0x61)}));
    extra["unregisteredEnvelopeField"] = json!(true);
    client.send_raw(extra).await;
    client
        .expect_error("nodelink.protocol.schema_invalid")
        .await;

    // ② 紧接着一条序号按对端计数器正常递增的合法消息：不得被判序号错误，且必须被正常处理
    //    （`link.pong` 回填同一 nonce，由 `ping_round_trip` 自己断言）。
    client.step("未登记字段之后序号正常递增的合法消息");
    client.advance_after_accepted();
    let _ = client.ping_round_trip(0x62).await;

    // ③ 缺 `messageId`（§2.2 的必需字段）：同样是信封形状错误、同样不关连接。
    client.step("缺 messageId");
    let mut missing = client.envelope("link.ping", json!({"nonce": ping_nonce(0x63)}));
    missing
        .as_object_mut()
        .expect("信封是 object")
        .remove("messageId");
    client.send_raw(missing).await;
    client
        .expect_error("nodelink.protocol.schema_invalid")
        .await;

    // ④ 同样紧接着一条序号正常递增的合法消息。
    client.step("缺 messageId 之后序号正常递增的合法消息");
    client.advance_after_accepted();
    let _ = client.ping_round_trip(0x64).await;

    // ⑤ §2.5 的结构超限也是同一族的消息级拒绝（`schema_invalid`、不关连接）。帧的其余部分全部合法：用
    //    `link.error` 的开放 `details` 承载越界结构（与 `json_structure_limits_are_enforced_on_the_wire`
    //    同口径），因此这条拒绝只可能来自结构上限——深度若不到 65，这帧会被正常受理、根本不会回
    //    `link.error`。判定先于解析并不改变「这帧占用了对端的下一个序号」，因此同样推进。
    client.step("JSON 嵌套深度 65");
    let mut deep = client.envelope("link.error", json!({}));
    deep["body"] = json!({
        "code": "nodelink.protocol.invalid_json",
        "message": "x",
        "retryable": false,
        "correlationId": null,
        "details": nested_details(MAX_NESTING_DEPTH - DEPTH_OUTSIDE_DETAILS + 1),
    });
    client.send_raw(deep).await;
    client
        .expect_error("nodelink.protocol.schema_invalid")
        .await;

    // ⑥ 同样紧接着一条序号正常递增的合法消息。
    client.step("结构超限之后序号正常递增的合法消息");
    client.advance_after_accepted();
    let _ = client.ping_round_trip(0x66).await;

    assert_eq!(harness.conn.registry().active(), 1, "连接必须保持可用");
    harness.stop().await;
}

/// **已知限制**（不是期望行为）：嵌套深度超过 `serde_json` 实际可解析边界的合法 JSON 帧读不出序号，
/// 因而不推进入站期望序号（对应变更任务 1.5）。
///
/// §2.5 的嵌套上限是 64，但 `wire::sequence_after_rejection` 靠 `serde_json` 重新解析帧文本来读序号，
/// 而 `serde_json` 的递归上限是 128、检查又在**进入容器时先自减再判零**，因此实际只能解析嵌套 127 层
/// 的帧——深度 65..=127 的帧可解析（推进），深度 ≥128 的帧即便文本是合法 JSON 也解析不出序号（不推进）。
/// 规格的例外条款把它登记为**已知残留**：
/// `openspec/changes/node-link-malformed-sequence/specs/node-link-owner-server/spec.md` 的「只有读不出
/// 「本连接的下一个序号」的帧才不推进：文本不是合法 JSON、连接字段不成对或缺席、`connectionId` 属于别的
/// 连接、序号非规范十进制 / 超 `2^63-1` / 不等于期望值，**以及嵌套深度超过解析器递归上限**（按文本重解析
/// 读不出序号）。最后一种是**已知残留**」。
///
/// 因此本用例钉的是**解析能力的边界，不是策略**：若将来改用不经完整解析的方式读序号（例如直接扫描文本），
/// 第二段的期望必须随之改成「推进」。第一段（深度 127）本身是正确行为——两条消息级拒绝都应推进，只有
/// 深度 128 这一条因读不出序号而做不到。
#[tokio::test]
async fn nesting_beyond_the_parser_recursion_limit_cannot_advance_the_sequence() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;

    // ① 深度 127（= `DEPTH_OUTSIDE_DETAILS` + 124）：帧文本仍可被 `serde_json` 解析出序号，因此这条
    //    结构类拒绝与其它消息级拒绝同口径，照常推进入站期望序号。
    client.step("JSON 嵌套深度 127 的结构超限");
    let mut deep_127 = client.envelope("link.error", json!({}));
    deep_127["body"] = json!({
        "code": "nodelink.protocol.invalid_json",
        "message": "x",
        "retryable": false,
        "correlationId": null,
        "details": nested_details(127 - DEPTH_OUTSIDE_DETAILS),
    });
    client.send_raw(deep_127).await;
    client
        .expect_error("nodelink.protocol.schema_invalid")
        .await;

    //    序号确已被消耗：推进本地计数后，序号正常递增的合法消息必须被正常处理并回填 nonce。
    client.step("深度 127 之后序号正常递增的合法消息");
    client.advance_after_accepted();
    let _ = client.ping_round_trip(0x67).await;

    // ② 深度 128：超出解析器递归上限 ⇒ 读不出序号 ⇒ 不推进。这条**不**推进本地计数，紧接的往返因此
    //    用与刚才深帧完全相同的那一个序号——它仍被受理，正是「不推进」的证据。
    client.step("JSON 嵌套深度 128 的结构超限");
    let mut deep_128 = client.envelope("link.error", json!({}));
    deep_128["body"] = json!({
        "code": "nodelink.protocol.invalid_json",
        "message": "x",
        "retryable": false,
        "correlationId": null,
        "details": nested_details(128 - DEPTH_OUTSIDE_DETAILS),
    });
    client.send_raw(deep_128).await;
    client
        .expect_error("nodelink.protocol.schema_invalid")
        .await;

    client.step("深度 128 之后沿用同一序号的合法消息");
    let _ = client.ping_round_trip(0x68).await;

    assert_eq!(harness.conn.registry().active(), 1, "连接必须保持可用");
    harness.stop().await;
}

/// binary 帧不是本协议的载荷 → `invalid_json` 4400 关闭。
#[tokio::test]
async fn a_binary_frame_is_closed_with_4400() {
    let harness = Harness::new().await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;
    client.send_binary(b"\x01\x02\x03").await;
    let error = client.expect_error("nodelink.protocol.invalid_json").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4400, "binary 帧以 4400 关闭");
    harness.stop().await;
}

/// 读一份 `fixtures/node-link/v1/` 下的 fixture（路径相对仓库根，与 `CARGO_MANIFEST_DIR` 无关）。
fn read_fixture(relative: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures/node-link/v1")
        .join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("fixture 必须存在（{}）：{error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("fixture 必须是 JSON（{}）：{error}", path.display()))
}

// ---------------------------------------------------------------------------------------------
// R79/R80/R81：心跳、静默与慢连接
// ---------------------------------------------------------------------------------------------

/// 按协商的间隔发送 `link.ping`，回填 nonce 的 `link.pong` 让连接继续存活。
///
/// 间隔用 `node.ready.limits.heartbeatIntervalMs = 1_000`（schema 下限）驱动，无需等待真实 30 秒。
#[tokio::test]
async fn heartbeat_round_trip_keeps_the_connection_alive() {
    let harness = Harness::start(
        TestWorld::with_store(Arc::new(FixedStore::new(
            TEST_SERVER_EPOCH,
            STORE_HEAD_SEQUENCE,
        ))),
        NodeLinkConfig {
            public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
            heartbeat_interval_ms: 1_000,
            ..NodeLinkConfig::default()
        },
    )
    .await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;

    // Owner 必须主动发心跳（认证后出站序号从 1 开始的 link.ping）。
    let ping = client.expect_type("link.ping").await;
    let heartbeat_sequence = client.server_sequence;
    assert_eq!(
        heartbeat_sequence, 2,
        "心跳是 `node.ready`（1）之后的首个出站消息"
    );
    let nonce = ping["body"]["nonce"].as_str().expect("nonce").to_owned();
    client
        .send_post_auth("link.pong", json!({"nonce": nonce}))
        .await;
    client.advance_after_accepted();
    // 回填之后连接仍可用（心跳不是「发一次就断」）。
    client.ping_round_trip(0x71).await;
    // 下一个心跳周期到达后连接仍然活着（静默计时未误触发）。
    let second = client.expect_type("link.ping").await;
    assert!(
        client.server_sequence > heartbeat_sequence,
        "心跳必须按协商的间隔持续发送"
    );
    assert_ne!(
        second["body"]["nonce"],
        json!(nonce),
        "每次心跳的 nonce 必须不同"
    );
    harness.stop().await;
}

/// 静默超过窗口（§2.5 固定 90 秒）→ 4408 关闭，信任记录不受影响。
///
/// 窗口用 [`NodeLinkConn::with_test_windows`] 调成 2 秒：90 秒的真实等待在用例里不可接受，而窗口取值由
/// [`protocol_constants_are_not_configurable`] 锁定，本用例证明的是「静默判定 → 4408」这条机制。
#[tokio::test]
async fn silence_beyond_the_window_is_closed_with_4408() {
    let harness = Harness::start_with_test_windows(
        TestWorld::with_store(Arc::new(FixedStore::new(
            TEST_SERVER_EPOCH,
            STORE_HEAD_SEQUENCE,
        ))),
        NodeLinkConfig {
            public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
            // 心跳间隔取 schema 上限以免 ping 掩盖静默判定（静默窗口仍独立计时）。
            heartbeat_interval_ms: 300_000,
            ..NodeLinkConfig::default()
        },
        Duration::from_secs(2),
        Duration::from_secs(30),
    )
    .await;
    let pairing = harness.approved_pairing().await;
    let mut client = Client::connect_with_timeout(harness.addr, IO_TIMEOUT).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;
    let started = Instant::now();
    assert_eq!(
        client.expect_close().await,
        4408,
        "静默超过窗口以 4408 关闭"
    );
    assert!(
        started.elapsed() >= Duration::from_secs(2),
        "关闭必须发生在窗口之后"
    );
    assert_eq!(
        harness
            .world
            .trust
            .pairing(&pairing)
            .map(|record| record.state()),
        Some(PairingState::Consumed),
        "静默关闭只影响连接，不改写已经落库的持久化状态"
    );
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "会话结束后必须从注册表摘除",
    )
    .await;
    harness.stop().await;
}

/// 瞬时突发后回落：该连接**不得**被断开，同 Owner 上其他连接不受影响。
///
/// 这是「饱和可恢复」在真实连接上的证据（另一侧见句柄级的
/// `a_slow_consumer_that_never_recovers_keeps_its_saturation_segment`）：队列被压到高水位（其中一条投递
/// 真的被拒、饱和段真的建立过），客户端的读随后把队列排到低水位之下——该连接必须继续正常往来，而不是
/// 因为「曾经饱和过」被会话 Tick 的慢消费者看门狗断开。
///
/// 饱和宽限调成 1.5 秒（30 秒的真实等待在用例里不可接受），断言发生在宽限之后：只要饱和状态没被清除，
/// 同一个 Tick 分支早已把这条连接关掉。
#[tokio::test]
async fn a_transient_burst_that_drains_does_not_disconnect_the_connection() {
    let harness = Harness::start_with_test_windows(
        TestWorld::with_store(Arc::new(FixedStore::new(
            TEST_SERVER_EPOCH,
            STORE_HEAD_SEQUENCE,
        ))),
        NodeLinkConfig {
            public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
            max_pending_queue_messages: 4,
            // 心跳取 schema 上限：本用例只看饱和判定，不让 ping 混进队列水位。
            heartbeat_interval_ms: 300_000,
            ..NodeLinkConfig::default()
        },
        Duration::from_secs(90),
        Duration::from_millis(1_500),
    )
    .await;
    let _ = harness.approved_pairing().await;

    // ① 对照组（快连接）：全程正常往来，证明一侧的突发不影响别人（R81）。
    let mut healthy = Client::connect(harness.addr).await;
    let _ = authenticated(&mut healthy, ACCESS_NODE, false).await;

    // ② 突发连接：握手后立刻把队列压到高水位（客户端的读照常进行，写不会阻塞会话）。
    let mut burst = Client::connect(harness.addr).await;
    let _ = authenticated(&mut burst, ACCESS_NODE, false).await;
    let burst_handle = other_handle(&harness, &healthy);
    assert_eq!(
        saturate(&burst_handle),
        1,
        "突发必须真的顶到高水位，否则本用例什么都没证明"
    );
    assert!(burst_handle.saturated(), "高水位判定必须对投递方可见");
    assert!(burst_handle.saturated_for().is_some(), "饱和段必须已经建立");

    // ③ 客户端把突发读完 → 队列排到低水位之下 → 会话侧的 `consumed` 清除饱和状态。
    for _ in 0..burst_handle.limits().max_pending_queue_messages() {
        burst.expect_type("link.error").await;
    }
    assert_eventually(
        || !burst_handle.saturated(),
        "客户端读完突发后，队列必须排到低水位之下并清除饱和状态",
    )
    .await;
    assert!(
        burst_handle.saturated_for().is_none(),
        "清除必须连饱和段计时一起归零"
    );

    // ④ 宽限窗口过去之后：曾被饱和的连接必须还在，且两侧都能继续正常往来。
    tokio::time::sleep(Duration::from_millis(2_000)).await;
    assert_eq!(
        harness.conn.registry().active(),
        2,
        "瞬时突发后回落的连接不得被断开（曾饱和 ≠ 持续饱和）"
    );
    burst.ping_round_trip(0x91).await;
    healthy.ping_round_trip(0x92).await;
    harness.stop().await;
}

/// 持续饱和的连接**不得**拖住同 Owner 上的其他连接；该连接的饱和段必须连续累计到宽限之上。
///
/// **判据取规格实际用的那个量**：慢消费者看门狗的唯一判据是 `saturated_for() >= slow_consumer`
/// （`session.rs` 的 `tick_action`）——「当前饱和段已持续的时长」。因此本用例断言「观测到一个
/// 饱和段连续累计到宽限之上」，同时对照组一直在正常往来（R81 的另一半）。它**不断言**每一轮
/// 占用都在低水位之上：会话的 `send_text` 只把数据交给内核（`transport/net/ws.rs` 没有写期限），
/// 只要内核还吸得下、占用就会往下走，把「内核吸收了多少」当成常量正是下面那条记录的共同病根。
///
/// **四次失败的口径（都记下来，避免再走）**：①断言「占用必须始终在低水位之上」；②把形状换成压力源
/// 持续补货、并把上限抬到 schema 默认的 2,000 条（低水位 1,000 条）；③改成只看「单段连续时长」。
/// 三次都栽在同一个量上：填充载荷只有几个字节，**队列的字节上限（8 MiB）从不参与判定**，于是「有没有
/// 脱离饱和」只取决于内核替对端吸走了多少条——而那是平台相关的量（CI 的 Linux runner 吸得更多；本机
/// loopback 实测 163,800 B ≈ 546 条，已经与整个队列同量级）。③在 CI 上把同一个竞态的**两端**都演了
/// 一遍：内核吸走得多就够把段清零（失败在 `longest`：只到 1.216 s——退出判定在采样之后，所以它没到
/// 宽限只可能是段被清零过）；吸得慢则队列一直顶满、看门狗照常按宽限断开（失败在 `active() == 1`）。
/// 两次的同一提交另一次运行都通过。
///
/// 本版（④）把**字节**变成绑定维度，两个方向一起堵。填充串取
/// [`PRESSURE_FILLER_SATURATION_BYTES`]（32 KiB）：①队列常驻 8 MiB（≈254 条，远不到 1,000 条的条数
/// 低水位），内核吸走几百 KB～几 MB 也压不到 4 MiB 的字节低水位之下，段不会被清零；②填满 socket 只需
/// 约 10～30 次 32 KiB 写（旧形状要约 1,850 次 4 字节写），写路径在毫秒量级就停住、远早于宽限（窗口内
/// 实测占用恒为 254 条 / 8,372,508 B、一次都没往下走，就是「写路径已停」的直接证据）。判定因此只
/// 描述队列状态，不描述内核缓冲大小。
///
/// 「段 ≥ 宽限」与「连接还在」为什么能同时成立：对端不读、socket 缓冲吸满之后，会话停在
/// `send_text` 上，`select!` 里的 tick 同样得不到轮询，宽限判定根本没机会执行（`transport/net/ws.rs`
/// 没有写期限，这个缺口记在 `conn::session` 的模块文档里）。④ 的填充串让「吸满」在毫秒内完成，
/// 所以这个状态在窗口内是稳定的，而不是靠运气。本用例只看「不拖住对端」。
///
/// 这里**不**断言「该连接被断开」：断开判定由「队列持续饱和、写路径没卡住」的
/// `a_permanently_saturated_queue_still_reaches_the_slow_consumer_watchdog` 与句柄级的
/// `a_slow_consumer_that_never_recovers_keeps_its_saturation_segment` 覆盖，本用例只看「不拖住对端」。
#[tokio::test]
async fn a_continuously_saturated_connection_does_not_stall_its_peer() {
    const GRACE: Duration = Duration::from_millis(1_500);
    let harness = Harness::start_with_test_windows(
        TestWorld::with_store(Arc::new(FixedStore::new(
            TEST_SERVER_EPOCH,
            STORE_HEAD_SEQUENCE,
        ))),
        NodeLinkConfig {
            public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
            // 条数上限取 §2.5 的默认值 2,000（低水位 1,000 条），字节上限同样取默认的 8 MiB（低水位
            // 4 MiB）。**让字节成为绑定维度**是本用例成立的前提：填充串取
            // [`PRESSURE_FILLER_SATURATION_BYTES`]，队列因此常驻 8 MiB（≈254 条，远不到 1,000 条的条数
            // 低水位），内核替对端吸走的那几百 KB～几 MB 压不到 4 MiB 的字节低水位之下。
            max_pending_queue_messages: 2_000,
            // 心跳取 schema 上限：本用例只看饱和判定，不让 ping 混进队列水位。
            heartbeat_interval_ms: 300_000,
            ..NodeLinkConfig::default()
        },
        Duration::from_secs(90),
        GRACE,
    )
    .await;
    let _ = harness.approved_pairing().await;

    // ① 对照组（快连接）。
    let mut healthy = Client::connect(harness.addr).await;
    let _ = authenticated(&mut healthy, ACCESS_NODE, false).await;

    // ② 慢连接：握手后一个字节都不再读；压力源持续投递，队列被顶在字节高水位上（填充串给足，
    // 见 [`PRESSURE_FILLER_SATURATION_BYTES`]——这是本用例不再是竞态的关键）。
    let mut slow = Client::connect(harness.addr).await;
    let _ = authenticated(&mut slow, ACCESS_NODE, false).await;
    let slow_handle = other_handle(&harness, &healthy);
    let pressure = OutboundPressure::spawn_with_filler(
        Arc::clone(&slow_handle),
        None,
        json!({ "padding": "x".repeat(PRESSURE_FILLER_SATURATION_BYTES) }),
    );
    // 先等饱和真正建立（压力源是独立 OS 线程，起步时隙在用例之外，不能假定它已开跑）。
    assert_eventually(
        || slow_handle.saturated(),
        "压力源挂上后队列必须达到高水位（否则本用例什么都没证明）",
    )
    .await;
    assert!(
        pressure.sent() >= 1,
        "压力源必须真的投过消息，实际 {} 条",
        pressure.sent()
    );

    // ③ 宽限窗口内观测饱和段。判据取**规格实际用的那个量**：慢消费者看门狗的唯一判据是
    // `saturated_for() >= slow_consumer`（`session.rs` 的 `tick_action`），即「当前饱和段已持续的
    // 时长」。因此这里断言「观测到一个饱和段连续累计到宽限之上」，而**不断言**每一轮占用都在
    // 低水位之上——后者把「内核吸收了多少」当成常量，是平台相关的量（前两版修法在 Linux CI 上
    // 反复失败正是如此）。会话的 `send_text` 只把数据交给内核：只要某一刻压力源没补上、占用掉到
    // 低水位之下，判定就会断一次，随后重新建立；那不影响看门狗，因为断开用的是「单段连续时长」。
    let deadline = Instant::now() + 2 * GRACE;
    let mut longest: std::time::Duration = std::time::Duration::ZERO;
    let mut observed_saturated = false;
    loop {
        if let Some(since) = slow_handle.saturated_for() {
            observed_saturated = true;
            longest = longest.max(since);
        }
        if Instant::now() >= deadline {
            break;
        }
        // 对照组在同一窗口内持续可用（§2.5：不丢消息、不阻塞其他连接）。
        healthy.ping_round_trip(0xa1).await;
    }
    assert!(
        pressure.sent() >= 10,
        "压力源必须真的持续往队列里投递，实际只投了 {} 条",
        pressure.sent()
    );
    assert!(
        observed_saturated,
        "窗口内必须至少一次观测到饱和（否则本用例什么都没证明）"
    );
    assert!(
        longest >= GRACE,
        "必须观测到一个饱和段连续累计到宽限之上（实际 {:?}）——看门狗的判据正是这个量",
        longest
    );
    assert_eq!(
        harness.conn.registry().active(),
        2,
        "两条连接都必须还在（慢连接从未被判定为拖住对端）"
    );
    healthy.ping_round_trip(0xa2).await;

    // 关掉慢连接的对端并停掉压力源：会话从写路径上退出，接入层随之收尾。
    drop(pressure);
    drop(slow);
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// R79/R81：持续非空的出站队列不得饿死 tick（回归）
// ---------------------------------------------------------------------------------------------

/// 起一个只挂 `/node-link/v1` 的接入层，装配件由调用方给出（窗口/限额已在调用方定好）。
///
/// 与 [`Harness::start`] 的唯一区别是 Node Link 装配件的构造方式：需要用
/// [`NodeLinkConn::with_test_windows`] 调短窗口的用例没法走默认配置。
async fn harness_with_conn(world: TestWorld, conn: Arc<NodeLinkConn>) -> Harness {
    let mut listener = NetListener::bind(NetConfig {
        listen: "127.0.0.1:0".to_owned(),
        ..NetConfig::default()
    })
    .await
    .expect("绑定 loopback listener");
    listener
        .register_ws(
            WS_PATH,
            WS_SUBPROTOCOL,
            WsEndpoint::handler(Arc::clone(&conn)),
        )
        .expect("注册 Node Link 端点");
    let addr = listener.local_addrs()[0];
    let (shutdown, signal) = Shutdown::channel();
    let join = tokio::spawn(async move { listener.serve(signal).await });
    Harness {
        world,
        addr,
        conn,
        shutdown,
        join,
    }
}

/// 「持续非空但**不**饱和」压力源的水位（条）。
///
/// 上界：必须低于 2,000 条的默认高水位，否则投递会被拒（`mark_saturated`），tick 就走到慢消费者判定
/// 而不是心跳判定；下界：必须远大于一次 socket 写入突发能排空的量——会话在一次客户端读取（最多
/// 16 KiB）之后能连续写出约百条消息，取 1,800 让这个突发不足以把队列排空。队列排空会让 `select!`
/// 出现「出站分支不就绪」的窗口，缺陷里的 tick 就有机会被轮询到（用例于是漏报）。
const BUSY_QUEUE_TARGET: u64 = 1_800;

/// 压力源默认填充串的字节数（只够让消息成型，内容不参与任何断言）。
///
/// 「持续非空」那组用例压的是**条数**维度，不需要字节维度参与；要压字节维度的用例显式传
/// [`PRESSURE_FILLER_SATURATION_BYTES`] 长的填充串。
const PRESSURE_FILLER_DEFAULT_BYTES: usize = 4;

/// 「持续饱和」用例的填充串字节数：32 KiB。
///
/// **必须给足**，否则判定落到调度竞态上。队列判定是**条数 + 字节**双维度（`conn::registry` 的
/// `consumed`）：进入是「条数 ≥ 2,000 或字节 ≥ 8 MiB」，退出是「条数 < 1,000 **且** 字节 < 4 MiB」。
/// 默认的 4 字节填充让字节维度只有 0.6 MB 量级、永不参与判定，于是「有没有脱离饱和」完全取决于
/// **内核能替对端吸走多少条**：对端一个字节都不读时，内核仍会先把 socket 缓冲吸满——本机 loopback
/// 实测 163,800 B ≈ 546 条（Linux runner 更大，见用例文档里记的两次失败）。吸走量与整个队列同量级
/// ⇒ 用例结果由调度决定：CI 上同一个竞态的两个方向各失败过一次——吸走得多就把饱和段清零（最长
/// 1.216 s < 宽限 1.5 s），吸得慢则队列一直顶满、连接被看门狗按宽限断开（`active() == 1`）；两次的
/// 同一提交另一次运行都通过。
///
/// 32 KiB 之后**字节**成为唯一绑定维度：队列顶到 8 MiB（≈254 条，远不到 1,000 条的低水位），内核
/// 即使把 socket 缓冲吸满也只带走几百 KB～几 MB，占用不会掉到 4 MiB 之下 ⇒ 饱和段连续累计，判定只
/// 描述队列状态、不描述内核缓冲大小。顺带把「顶满反而被看门狗断开」那个方向也堵住：填满 socket 只需
/// 约 10～30 次这种大小的写，写路径在毫秒量级就停在 `send_text` 上、远早于宽限（详见用例文档）。
/// `saturate_with` 的旧注释里那句「需要足够大的消息把 socket 缓冲填满」说的就是这个量，但它当时只用
/// 在句柄口径的 helper 上，没有接到本用例的压力源上。
const PRESSURE_FILLER_SATURATION_BYTES: usize = 32 * 1024;

/// 「对端持续推流」的用例里等 close 帧的预算：慢消费者宽限（用例里 1.5 秒）+ 关闭前排空已入队消息的
/// 预算（2 秒）+ 余量。没有它的话「对端根本不断开」会表现为用例挂住（帧一直在到达，读超时不会触发）。
const CLOSE_WAIT_BUDGET: Duration = Duration::from_secs(8);

/// 「出站队列持续非空」的压力源：一个**独立 OS 线程**不停往会话的待发送队列投递填充消息。
///
/// 为什么是线程而不是 tokio 任务：tokio 任务只在会话让出执行权时才能运行，而本用例要压的恰恰是
/// 「会话因为出站队列每轮都就绪而几乎不让出执行权」这一形态。线程与运行时并行执行，队列在会话排空
/// 一条之后立刻被补上，`receiver.recv()` 因此在每个 `select!` 轮次上都就绪——这正是缺陷里 tick
/// 被饿死的条件。若换成 tokio 任务，队列会出现「已被排空、生产者还没补上」的空窗，而空窗里
/// `select!` 会把 tick 轮询到，用例就会漏报（这正是既有
/// [`a_transient_burst_that_drains_does_not_disconnect_the_connection`] 在缺陷版本下也仍然通过的原因：
/// 它只灌一次队列，之后队列被排空，tick 就有了被轮询的机会）。
///
/// - `target = Some(n)`：只在队列少于 `n` 条时投递，**从不触发高水位**（`saturated_for()` 保持
///   `None`，tick 于是会走到心跳判定）；
/// - `target = None`：一直投递到高水位，队列持续饱和（tick 走到慢消费者判定）。
///
/// 填充消息的载荷长度由调用方给（[`OutboundPressure::spawn_with_filler`]）：「持续非空」那组用例压的是
/// **条数**维度，用默认量即可；「持续饱和」用例必须压**字节**维度，理由见
/// [`PRESSURE_FILLER_SATURATION_BYTES`]。
struct OutboundPressure {
    sent: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl OutboundPressure {
    /// 默认填充载荷的压力源（[`PRESSURE_FILLER_DEFAULT_BYTES`] 字节的填充串）。
    fn spawn(handle: Arc<ConnectionHandle>, target: Option<u64>) -> Self {
        let filler = json!({ "padding": "x".repeat(PRESSURE_FILLER_DEFAULT_BYTES) });
        Self::spawn_with_filler(handle, target, filler)
    }

    /// 同 [`OutboundPressure::spawn`]，但由调用方给出填充消息的 body（内容只编码一次，线程里复用）。
    fn spawn_with_filler(
        handle: Arc<ConnectionHandle>,
        target: Option<u64>,
        filler: Value,
    ) -> Self {
        let sent = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let counter = Arc::clone(&sent);
        let flag = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                let has_room = match target {
                    Some(target) => handle.pending().messages < target,
                    None => true,
                };
                if has_room {
                    match handle.send(MessageType::LinkError, &filler) {
                        Ok(()) => {
                            counter.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        // 会话已结束（接收端被丢弃）：队列再没人消费，压力源收工。
                        Err(SendFault::Closed) => break,
                        // 高水位（`target = None` 时的常态）：让出 CPU 后立刻重试。
                        Err(_) => {}
                    }
                }
                std::thread::yield_now();
            }
        });
        Self {
            sent,
            stop,
            thread: Some(thread),
        }
    }

    /// 压力源已成功入队的填充消息数（用例据此断言队列确实被持续撑住，而不是只灌了一次）。
    fn sent(&self) -> u64 {
        self.sent.load(Ordering::Relaxed)
    }
}

impl Drop for OutboundPressure {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// 出站队列持续非空时，心跳仍按 `heartbeatIntervalMs` 的节奏发出。
///
/// 缺陷版本把 tick 排在出站分支之后、且每轮重建 `sleep(500ms)`：只要出站队列每轮都就绪，tick 永远
/// 不被轮询，`link.ping` 一条都发不出去——恰恰在最需要保活的时候保活停摆。压力源把队列撑在
/// [`BUSY_QUEUE_TARGET`] 条（低于 2,000 条的默认高水位），投递因此从**不**被拒绝，`saturated_for()`
/// 保持 `None`，tick 会走到心跳判定而不是慢消费者判定。
#[tokio::test]
async fn a_busy_outbound_queue_does_not_starve_the_heartbeat() {
    let harness = Harness::start(
        TestWorld::with_store(Arc::new(FixedStore::new(
            TEST_SERVER_EPOCH,
            STORE_HEAD_SEQUENCE,
        ))),
        NodeLinkConfig {
            public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
            heartbeat_interval_ms: 1_000,
            ..NodeLinkConfig::default()
        },
    )
    .await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;
    let handle = harness.conn.registry().handles()[0].clone();

    // 认证刚完成就挂上压力源：从现在起队列一直留着消息。
    let pressure = OutboundPressure::spawn(Arc::clone(&handle), Some(BUSY_QUEUE_TARGET));
    let heartbeat = Duration::from_millis(1_000);

    client.step("出站队列持续非空时的心跳");
    let first = client
        .expect_type_eventually("link.ping", heartbeat * 5)
        .await;
    let first_at = Instant::now();
    let second = client
        .expect_type_eventually("link.ping", heartbeat * 5)
        .await;
    let interval = first_at.elapsed();
    assert_ne!(
        first["body"]["nonce"], second["body"]["nonce"],
        "每次心跳的 nonce 必须不同"
    );
    assert!(
        interval >= heartbeat * 9 / 10 && interval <= heartbeat * 3,
        "心跳必须按 heartbeatIntervalMs（1 秒）的节奏持续发送，实测相邻间隔 {interval:?}"
    );
    assert!(
        pressure.sent() >= 10,
        "压力源必须真的持续往队列里投递，实际只投了 {} 条",
        pressure.sent()
    );
    harness.stop().await;
}

/// 出站队列持续非空时，入站静默判定仍会推进到 4408 关闭。
///
/// 同一缺陷的另一条可见后果：缺陷版本下 tick 不被轮询，「连续 90 秒无入站消息即关闭」这条判据永远
/// 等不到检查它的时刻。窗口用 [`NodeLinkConn::with_test_windows`] 调成 2 秒（真实 90 秒在用例里不可
/// 接受；固定常量的取值由 [`protocol_constants_are_not_configurable`] 锁定，本用例证明的是
/// 「静默判定 → 4408」这条机制）。
#[tokio::test]
async fn inbound_silence_still_closes_a_connection_with_a_busy_outbound_queue() {
    let world = TestWorld::with_store(Arc::new(FixedStore::new(
        TEST_SERVER_EPOCH,
        STORE_HEAD_SEQUENCE,
    )));
    let registry = ConnectionRegistry::new();
    let conn = Arc::new(
        NodeLinkConn::new(
            world.core.clone(),
            world.authority.clone(),
            NodeLinkConfig {
                public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
                // 心跳间隔取 schema 上限：本用例要验的是静默判定，不让 ping 混进观察窗口。
                heartbeat_interval_ms: 300_000,
                ..NodeLinkConfig::default()
            },
            Arc::clone(&registry),
        )
        .with_test_windows(Duration::from_secs(2), Duration::from_secs(30)),
    );
    let harness = harness_with_conn(world, conn).await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;
    let handle = harness.conn.registry().handles()[0].clone();
    let pressure = OutboundPressure::spawn(Arc::clone(&handle), Some(BUSY_QUEUE_TARGET));

    let started = Instant::now();
    client.step("出站队列持续非空时的静默超时");
    assert_eq!(
        client.expect_close_within(CLOSE_WAIT_BUDGET).await,
        4408,
        "入站静默超时必须仍然以 4408 关闭"
    );
    assert!(
        started.elapsed() >= Duration::from_secs(2),
        "关闭必须发生在静默窗口之后"
    );
    assert!(
        pressure.sent() >= 10,
        "压力源必须真的持续往队列里投递，实际只投了 {} 条",
        pressure.sent()
    );
    harness.stop().await;
}

/// 出站队列**持续处于高水位**时，慢消费者看门狗仍会断开连接。
///
/// 与「持续非空」那两条不同，这里压力源一直投递到高水位（队列从不回到上限之下），
/// `saturated_for()` 因此持续为 `Some`：断开与否完全取决于 tick 有没有被轮询到。队列上限保持默认的
/// 2,000 条（只用 4 条这类小上限的话，会话一次突发就能把队列排空，tick 又会获得被轮询的机会），
/// 宽限用 [`NodeLinkConn::with_test_windows`] 调成 1.5 秒（同上的理由）。
#[tokio::test]
async fn a_permanently_saturated_queue_still_reaches_the_slow_consumer_watchdog() {
    let world = TestWorld::with_store(Arc::new(FixedStore::new(
        TEST_SERVER_EPOCH,
        STORE_HEAD_SEQUENCE,
    )));
    let registry = ConnectionRegistry::new();
    let conn = Arc::new(
        NodeLinkConn::new(
            world.core.clone(),
            world.authority.clone(),
            NodeLinkConfig {
                public_origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
                heartbeat_interval_ms: 300_000,
                ..NodeLinkConfig::default()
            },
            Arc::clone(&registry),
        )
        .with_test_windows(Duration::from_secs(90), Duration::from_millis(1_500)),
    );
    let harness = harness_with_conn(world, conn).await;
    let _ = harness.approved_pairing().await;
    let mut client = Client::connect(harness.addr).await;
    let _ = authenticated(&mut client, ACCESS_NODE, false).await;
    let handle = harness.conn.registry().handles()[0].clone();
    let pressure = OutboundPressure::spawn(Arc::clone(&handle), None);

    client.step("出站队列持续饱和时的慢消费者看门狗");
    assert_eq!(
        client.expect_close_within(CLOSE_WAIT_BUDGET).await,
        close::NORMAL,
        "持续高水位必须在慢消费者宽限后断开"
    );
    assert!(
        handle.saturated(),
        "压力源必须让队列一直处于高水位（`saturated_for()` 持续为 Some）"
    );
    assert!(
        pressure.sent() >= 10,
        "压力源必须真的持续往队列里投递，实际只投了 {} 条",
        pressure.sent()
    );
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 连接句柄：序号与队列账本
// ---------------------------------------------------------------------------------------------

/// 除 `known` 之外那条已登记连接的句柄（用例总是同时持有「对照组」与「被测连接」）。
fn other_handle(harness: &Harness, known: &Client) -> Arc<ConnectionHandle> {
    let known_id = known
        .connection_id
        .as_deref()
        .expect("认证后必须有 connectionId");
    harness
        .conn
        .registry()
        .handles()
        .into_iter()
        .find(|handle| handle.connection_id().as_str() != known_id)
        .expect("另一条连接必须已登记")
}

/// 一条已入队消息信封里的 `connectionSequence`（对端看到的那个值）。
fn outbound_sequence(outbound: &Outbound) -> String {
    let message: Value = serde_json::from_str(&outbound.text).expect("出站消息是 JSON");
    message["connectionSequence"]
        .as_str()
        .expect("认证后消息必须带序号")
        .to_owned()
}

/// 出站 `connectionSequence` 只在**投递确定**时消耗。
///
/// 队列压到高水位（条数 1 / 字节 1 KiB）→ 触发一次 `HighWater` → 排空 → 再发一条：两条真正入队的
/// 消息的序号必须是 1、2。对端按 §2.2 要求严格加一，被拒的投递不得烧掉序号——否则本方向此后每一帧
/// 都带缺口，合规对端必须一直报 `sequence_invalid`，而协议没有重同步规则。
#[test]
fn a_rejected_delivery_does_not_consume_the_outbound_sequence() {
    let limits = SessionLimits::negotiate(&NodeLinkConfig {
        max_pending_queue_messages: 1,
        max_pending_queue_bytes: 1_024,
        ..NodeLinkConfig::default()
    });
    let (handle, mut receiver) = ConnectionHandle::new(
        Uuid::parse(&uuid_text()).expect("连接标识是规范 uuid"),
        node(ACCESS_NODE),
        "127.0.0.1".parse().expect("loopback 地址"),
        limits,
        Arc::new(FakeIds::default()),
    );

    // ① 压到高水位：第一条入队，第二条被拒（条数预算只有 1 条的余量）。
    handle
        .send(MessageType::LinkPing, &json!({ "nonce": ping_nonce(0x61) }))
        .expect("第一条必须入队");
    assert_eq!(
        handle.send(MessageType::LinkPing, &json!({ "nonce": ping_nonce(0x62) })),
        Err(SendFault::HighWater),
        "队列满时必须高水位拒绝"
    );
    assert!(handle.saturated(), "高水位判定必须对投递方可见");
    assert_eq!(handle.pending().messages, 1);

    // ② 排空（会话侧取出消息后记账）。
    let first = receiver.try_recv().expect("第一条已入队");
    handle.consumed(&first);
    assert_eq!(handle.pending().messages, 0);

    // ③ 再发一条：序号必须与第一条连续。
    handle
        .send(MessageType::LinkPing, &json!({ "nonce": ping_nonce(0x63) }))
        .expect("排空后必须能继续投递");
    let second = receiver.try_recv().expect("再发的那条已入队");
    assert_eq!(
        [outbound_sequence(&first), outbound_sequence(&second)],
        ["1".to_owned(), "2".to_owned()],
        "被拒的投递不得消耗序号"
    );
}

// ---------------------------------------------------------------------------------------------
// 饱和：高/低水位迟滞、「曾经饱和」与「持续饱和」的区分
// ---------------------------------------------------------------------------------------------

/// 一个只按条数上限判定的连接句柄（字节上限取 §2.5 默认值，本组用例不碰字节维度）。
fn handle_with_queue(messages: u64) -> (Arc<ConnectionHandle>, mpsc::Receiver<Outbound>) {
    ConnectionHandle::new(
        Uuid::parse(&uuid_text()).expect("连接标识是规范 uuid"),
        node(ACCESS_NODE),
        "127.0.0.1".parse().expect("loopback 地址"),
        SessionLimits::negotiate(&NodeLinkConfig {
            max_pending_queue_messages: messages,
            ..NodeLinkConfig::default()
        }),
        Arc::new(FakeIds::default()),
    )
}

/// 把待发送队列压到高水位，返回被容量判定拒绝的次数（`1` = 饱和起点已建立）。
///
/// 投递方（WP5 的 catalog/resource 与这里的用例）都在被拒一次后停手；循环里没有 `await`，因此在
/// 单线程运行时上用例与生产者不会交错，压满的过程是确定的。
fn saturate(handle: &ConnectionHandle) -> u64 {
    saturate_with(handle, 64)
}

/// 同 [`saturate`]，但由调用方给出每条 filler 的字节数。
///
/// 「持续饱和」的**端到端**用例需要足够的水量（理由见 [`PRESSURE_FILLER_SATURATION_BYTES`]），但它走
/// 的是真实连接与 [`OutboundPressure`]；本 helper 只被 [`saturate`] 用在句柄口径上。
fn saturate_with(handle: &ConnectionHandle, payload_len: usize) -> u64 {
    let payload = "x".repeat(payload_len);
    let mut rejected = 0;
    for _ in 0..handle.limits().max_pending_queue_messages() + 1 {
        match handle.send(MessageType::LinkError, &json!({ "padding": payload })) {
            Ok(()) => {}
            Err(SendFault::HighWater) => {
                rejected += 1;
                break;
            }
            Err(fault) => panic!("未预期的投递失败：{fault:?}"),
        }
    }
    rejected
}

/// 饱和是**可恢复的迟滞判定**：高水位进入、低水位（高水位的一半）退出。
///
/// 三处断言对应三件必须同时成立的事：① 低水位不得等于高水位（否则没有迟滞带，消费者在阈值附近抖动时
/// 看门狗永远攒不满宽限）；② 占用仍在迟滞带内（≥ 低水位）时**不得**清除——这正是「持续」的判定面；
/// ③ 占用真的降到低水位之下才清除，且计时一并归零（「曾经饱和过」不等于「此刻饱和」）。
#[test]
fn a_saturated_queue_clears_only_below_the_low_water_mark() {
    // 低水位常量与其口径：§2.5 的默认上限（2,000 条 / 8 MiB）之半。
    let (defaults, _defaults_outbound) = handle_with_queue(DEFAULT_MAX_PENDING_QUEUE_MESSAGES);
    assert_eq!(
        defaults.low_water_messages(),
        DEFAULT_MAX_PENDING_QUEUE_MESSAGES / 2,
        "条数低水位取高水位的一半"
    );
    assert_eq!(
        defaults.low_water_bytes(),
        DEFAULT_MAX_PENDING_QUEUE_BYTES / 2,
        "字节低水位取高水位的一半"
    );
    assert!(
        defaults.low_water_messages() < DEFAULT_MAX_PENDING_QUEUE_MESSAGES
            && defaults.low_water_bytes() < DEFAULT_MAX_PENDING_QUEUE_BYTES,
        "低水位不得等于高水位：否则迟滞带为空"
    );

    let (handle, mut receiver) = handle_with_queue(4);
    assert_eq!(handle.low_water_messages(), 2, "4 条上限 → 低水位 2 条");

    // ① 压到高水位：4 条入队、第 5 条被拒（饱和起点建立）。
    assert_eq!(saturate(&handle), 1, "持续投递必须让队列顶到上限");
    assert!(handle.saturated(), "高水位判定必须对投递方可见");
    assert!(handle.saturated_for().is_some(), "被拒一次即建立饱和段计时");

    // ② 消费一条（占用 3，仍在低水位 2 之上）→ 仍然饱和、计时连续累计。
    let first = receiver.try_recv().expect("第一条已入队");
    handle.consumed(&first);
    assert_eq!(handle.pending().messages, 3);
    assert!(handle.saturated(), "迟滞带内不得复位：低水位不得等于高水位");
    assert!(
        handle.saturated_for().is_some(),
        "迟滞带内饱和段必须连续累计"
    );

    // ③ 再消费两条（占用 1 < 低水位 2）→ 清除：曾饱和的连接恢复为「不饱和」。
    for _ in 0..2 {
        let outbound = receiver.try_recv().expect("已入队");
        handle.consumed(&outbound);
    }
    assert_eq!(handle.pending().messages, 1);
    assert!(!handle.saturated(), "降到低水位以下必须清除饱和状态");
    assert!(handle.saturated_for().is_none(), "清除必须连计时一起归零");

    // ④ 恢复后容量判定重新可用（读入门据此重新开始读取批次）。
    handle
        .send(MessageType::LinkError, &json!({ "padding": "y" }))
        .expect("恢复后必须能继续入队");
}

/// 「持续饱和」必须与「曾经饱和」区分开：只要消费者**从未**把队列排到低水位之下，饱和段就连续累计，
/// 看门狗的判据（`saturated_for >= SLOW_CONSUMER_GRACE`）成立、[`tick_action`] 判为断开。
///
/// 这是 [`a_saturated_queue_clears_only_below_the_low_water_mark`] 的另一侧：那里是瞬时突发后回落（不得
/// 断开），这里是消费者一直在动、却始终没跟上（宽限后必须断开）。用例按 `send`/`consumed` 的真实口径
/// 驱动句柄——会话 Tick 消费的正是这两个入口产生的结论——因此在毫秒级真实时间里覆盖 30 秒宽限所依赖
/// 的状态形状。
#[test]
fn a_slow_consumer_that_never_recovers_keeps_its_saturation_segment() {
    const GRACE: Duration = Duration::from_millis(250);
    let (handle, mut receiver) = handle_with_queue(4);
    assert_eq!(saturate(&handle), 1, "持续投递必须让队列顶到上限");

    let started = Instant::now();
    while started.elapsed() < 3 * GRACE {
        // 消费者在动（每取走一条就有人立刻补上一条），占用因此始终停在低水位 2 之上。
        let outbound = receiver.try_recv().expect("生产者必须已经把队列补满");
        handle.consumed(&outbound);
        assert!(
            handle.saturated(),
            "占用从未降到低水位之下：饱和段不得被清除"
        );
        handle
            .send(MessageType::LinkError, &json!({ "padding": "x" }))
            .expect("腾出的位置必须能立刻补上");
    }
    let saturated = handle.saturated_for().expect("饱和段必须连续存在");
    assert!(
        saturated >= GRACE,
        "持续饱和必须让饱和段累积到宽限之上（实际 {saturated:?}）"
    );
    assert_eq!(
        tick_action(handle.saturated_for(), GRACE, true),
        TickAction::Disconnect,
        "持续饱和必须断开，且断开优先于心跳"
    );
}

/// Tick 的动作判定：饱和**不得**短路心跳（§2.5：饱和不是静默）。
///
/// 第一行是回退成 `if saturated { … } else if due { … }` 时变红的那一条：饱和段尚未过宽限又到了心跳
/// 周期，必须仍然发 `link.ping`。其余三行锁定边界——过宽限时断开优先于心跳、未饱和时心跳照旧、未到期
/// 什么都不做。会话 Tick 只做分派，判定本身在这里被逐条锁定。
#[test]
fn the_tick_keeps_the_heartbeat_while_the_connection_is_saturated() {
    const GRACE: Duration = Duration::from_millis(1_500);
    assert_eq!(
        tick_action(Some(Duration::from_millis(200)), GRACE, true),
        TickAction::Ping,
        "饱和期间心跳必须照发（饱和不是静默）"
    );
    assert_eq!(
        tick_action(Some(GRACE), GRACE, true),
        TickAction::Disconnect,
        "饱和段达到宽限即断开，且优先于心跳"
    );
    assert_eq!(
        tick_action(None, GRACE, true),
        TickAction::Ping,
        "不饱和时心跳照旧"
    );
    assert_eq!(
        tick_action(Some(Duration::from_millis(200)), GRACE, false),
        TickAction::Idle,
        "未到心跳周期且未过宽限时什么都不做"
    );
}

/// 心跳投递结果的分类：高水位是**一级处置（推迟）**，不得当致命错误——那是看门狗的二级处置。
///
/// 这一行是「把推迟改回致命」时变红的那一条：把 [`SendFault::HighWater`] 映射成
/// [`HeartbeatOutcome::Ended`] 会让队列顶满的那一次心跳直接关掉连接，绕过 30 秒宽限。
#[test]
fn a_heartbeat_deferred_by_the_high_water_mark_is_not_fatal() {
    assert_eq!(heartbeat_outcome(Ok(())), HeartbeatOutcome::Queued);
    assert_eq!(
        heartbeat_outcome(Err(SendFault::HighWater)),
        HeartbeatOutcome::Deferred,
        "高水位只推迟这一次心跳，生死由慢消费者看门狗按宽限判定"
    );
    assert_eq!(
        heartbeat_outcome(Err(SendFault::Closed)),
        HeartbeatOutcome::Ended,
        "连接已结束不是背压"
    );
    assert_eq!(
        heartbeat_outcome(Err(SendFault::Encoding)),
        HeartbeatOutcome::Ended,
        "装不出信封是本机缺陷，不是背压"
    );
}

// ---------------------------------------------------------------------------------------------
// manifest fixture 漂移
// ---------------------------------------------------------------------------------------------

/// 消费 `fixtures/node-link/v1/manifest.json` 的每一份用例：本层（信封/握手/错误/心跳）的正反判定。
///
/// 判定口径按 manifest 的 `valid` 与消息族分组——不是「随便挑几份」：
///
/// | 分组 | `valid = true` | `valid = false` |
/// |---|---|---|
/// | WP4 解码的 body 族（`node.hello`/`node.challenge`/`node.proof`/`node.ready`） | 信封判定通过 + body 能解码成类型化 DTO + 往返保真 | 信封/body 至少一层拒绝 |
/// | 握手与心跳的 `link.*` 族 | 同上（`link.error` 认证前无连接字段、认证后有） | 信封判定拒绝（`schema_invalid`） |
/// | WP5/WP6 的业务族 | 信封判定通过（post_mvp 一族按 拒绝） | 信封判定通过、body 级拒绝归 WP5/WP6 |
///
/// 新增 fixture 必须落进这三张表之一，否则本用例失败（漂移门禁的意图：manifest 是唯一来源）。
#[test]
fn fixtures_are_consumed_by_the_handshake_and_error_layers() {
    let manifest: Value = read_fixture("manifest.json");
    let cases = manifest["cases"].as_array().expect("manifest.cases");
    assert!(!cases.is_empty(), "manifest 必须有用例");

    // body 级拒绝由 WP5/WP6 承担的反例（本层只断言「信封本身是合法的」）。
    let deferred_body_cases = [
        "invalid/session-create-with-cwd.json",
        "invalid/resource-event-missing-origin.json",
        "invalid/stale-attachment-generation.json",
        "invalid/command-unknown-field.json",
        "invalid/resource-event-missing-session.json",
        "invalid/resource-ack-missing-session.json",
        "invalid/command-terminal-missing-command.json",
        "invalid/session-create-accepted-with-result.json",
        // `session.resume` 的非空 payload：信封与 `sessionRef`/attachment 合法，body 级由命令路由
        // 回 `command.rejected(nodelink.command.unsupported_field)` + `details.field`（比 schema 的
        // `additionalProperties` 更具体，§12.7）。
        "invalid/session-resume-with-cwd.json",
    ];

    let mut handshake_bodies = 0usize;
    let mut link_bodies = 0usize;
    let mut routed = 0usize;
    let mut pairing_cases = 0usize;
    for case in cases {
        let fixture = case["fixture"].as_str().expect("fixture path");
        let valid = case["valid"].as_bool().expect("valid");
        let schema = case["schema"].as_str().expect("schema path");
        if schema.ends_with("pairing.schema.json") {
            // 配对请求/响应不是 `message.schema.json` 的消息（WP3 的端点用例已逐条消费）。
            pairing_cases += 1;
            continue;
        }
        let message: Value = read_fixture(fixture);
        let message_type = message["type"]
            .as_str()
            .unwrap_or_else(|| panic!("{fixture} 不是消息 fixture"));
        let text = message.to_string();
        let has_connection = message.get("connectionId").is_some();
        let phase = if has_connection {
            node_link_protocol::envelope::Phase::PostAuth
        } else {
            node_link_protocol::envelope::Phase::PreAuth
        };
        let connection_id = message
            .get("connectionId")
            .and_then(|value| Uuid::parse(value.as_str()?).ok());
        let sequence = message
            .get("connectionSequence")
            .and_then(|value| value.as_str().and_then(|text| text.parse::<u64>().ok()));
        let (judged, next) = crate::node_link::conn::wire::judge(
            &text,
            phase,
            Some(sequence.unwrap_or(1)),
            connection_id.as_ref(),
        );

        match (fixture_kind(message_type), valid) {
            (FixtureKind::Handshake | FixtureKind::Link, true) => {
                match fixture_kind(message_type) {
                    FixtureKind::Handshake => handshake_bodies += 1,
                    _ => link_bodies += 1,
                }
                let envelope = match judged {
                    crate::node_link::conn::wire::Inbound::Message(envelope) => envelope,
                    other => panic!("{fixture} 必须是本层接受的消息，实际：{}", describe(&other)),
                };
                if has_connection {
                    assert_eq!(next, sequence.map(|value| value + 1), "{fixture}");
                }
                // body 必须能用类型化 DTO 解码，且往返 JSON 与原文等价（未知字段在 closed object 上必失败）。
                let body = envelope.body().get();
                let round_trip = decode_known_body(message_type, body)
                    .unwrap_or_else(|error| panic!("{fixture}：类型化 body 被拒：{error}"));
                assert_eq!(round_trip, message["body"], "{fixture} 往返必须保真");
            }
            (FixtureKind::Handshake, false) => {
                // 握手 body 由本层类型化解码：信封合法、缺字段在 body 级被拒（不是信封级）。
                let envelope = match judged {
                    crate::node_link::conn::wire::Inbound::Message(envelope) => envelope,
                    other => panic!(
                        "{fixture} 的信封必须合法（缺字段是 body 级问题）：{}",
                        describe(&other)
                    ),
                };
                assert!(
                    decode_known_body(message_type, envelope.body().get()).is_err(),
                    "{fixture} 的 body 必须被类型化解码拒绝"
                );
            }
            (FixtureKind::Link, false) => {
                assert!(
                    matches!(
                        judged,
                        crate::node_link::conn::wire::Inbound::Rejected { .. }
                            | crate::node_link::conn::wire::Inbound::Fatal { .. }
                    ),
                    "{fixture} 必须被本层拒绝"
                );
            }
            (FixtureKind::Routed, valid) => {
                routed += 1;
                let post_mvp = matches!(
                    message_type,
                    "catalog.changed"
                        | "resource.detach"
                        | "node.rotate-key.request"
                        | "node.rotate-key.result"
                        | "link.backpressure"
                );
                if post_mvp {
                    assert!(
                        matches!(
                            judged,
                            crate::node_link::conn::wire::Inbound::Rejected { .. }
                        ),
                        "{fixture} 是 post_mvp：必须被显式拒绝（R49）"
                    );
                } else {
                    assert!(
                        matches!(judged, crate::node_link::conn::wire::Inbound::Message(_)),
                        "{fixture} 的信封必须合法（body 级判定归 WP5/WP6）"
                    );
                    if !valid {
                        assert!(
                            deferred_body_cases.contains(&fixture),
                            "{fixture} 未在用例的「body 级反例」清单里登记：新增 fixture 必须显式归类"
                        );
                    }
                }
            }
        }
    }

    // 三类都真的被走到（避免清单为空导致用例静默通过）。
    assert!(
        handshake_bodies >= 3,
        "握手 body 的用例数：{handshake_bodies}"
    );
    assert!(link_bodies >= 2, "link.* 的用例数：{link_bodies}");
    assert!(routed >= 10, "业务族的用例数：{routed}");
    assert_eq!(pairing_cases, 2, "配对 fixture 恰好两份（claim/status）");
}

/// fixture 的消息族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureKind {
    /// WP4 解码的握手 body（`node.hello`/`node.challenge`/`node.proof`/`node.ready`）。
    Handshake,
    /// 握手阶段与心跳的 `link.*`（`link.error`/`link.ping`/`link.pong`）。
    Link,
    /// WP5/WP6 的业务族（作为信封级输入经本层）。
    Routed,
}

fn fixture_kind(message_type: &str) -> FixtureKind {
    match message_type {
        "node.hello" | "node.challenge" | "node.proof" | "node.ready" => FixtureKind::Handshake,
        "link.error" | "link.ping" | "link.pong" => FixtureKind::Link,
        _ => FixtureKind::Routed,
    }
}

/// 把 body 解码成类型化 DTO 并重新编码成 JSON（未知字段在 `deny_unknown_fields` 上必然失败）。
fn decode_typed_body<T>(body: &str) -> Result<Value, serde_json::Error>
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let decoded: T = serde_json::from_str(body)?;
    serde_json::to_value(&decoded)
}

/// 本层解码的 handshake/`link.*` body 的类型化往返（`Err` = 该 body 在本层的 DTO 上不合法）。
///
/// `match` 穷尽 WP4 的 body 族：新增家族成员时这里会编译失败，不会静默落到 `panic` 分支之外的缺口。
fn decode_known_body(message_type: &str, body: &str) -> Result<Value, serde_json::Error> {
    match message_type {
        "node.hello" => decode_typed_body::<node_link_protocol::handshake::NodeHello>(body),
        "node.challenge" => decode_typed_body::<node_link_protocol::handshake::NodeChallenge>(body),
        "node.proof" => decode_typed_body::<node_link_protocol::handshake::NodeProof>(body),
        "node.ready" => decode_typed_body::<node_link_protocol::handshake::NodeReady>(body),
        "link.ping" => decode_typed_body::<node_link_protocol::error::Ping>(body),
        "link.pong" => decode_typed_body::<node_link_protocol::error::Pong>(body),
        "link.error" => decode_typed_body::<node_link_protocol::error::Body>(body),
        other => panic!("{other} 的类型不在 WP4 的 body 族里"),
    }
}

/// 判定结果的可读描述（用例失败时给出原因）。
fn describe(inbound: &crate::node_link::conn::wire::Inbound) -> String {
    match inbound {
        crate::node_link::conn::wire::Inbound::Message(_) => "接受".to_owned(),
        crate::node_link::conn::wire::Inbound::Rejected { code, .. } => {
            format!("消息级拒绝（{}）", code.as_str())
        }
        crate::node_link::conn::wire::Inbound::Fatal { code, .. } => {
            format!("致命拒绝（{}）", code.as_str())
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 受害面：饱和恢复后读入门必须重新开始读取批次
// ---------------------------------------------------------------------------------------------

/// 本组用例的会话 / Export / 事件素材（与 `resource/tests.rs`、`catalog.rs` 的路由夹具同形：固定 id 与
/// 固定时间戳让断言可判定）。
const RESUME_SESSION: &str = "3ae1c07c-9242-46e9-a9d2-4ec58c130f4a";
const RESUME_EXPORT_A: &str = "11111111-1111-4111-8111-111111111111";
const RESUME_EXPORT_B: &str = "22222222-2222-4222-8222-222222222222";
const RESUME_ORIGIN_EPOCH: &str = "018f6f89-8a23-7a10-a0d3-f92e6a31d952";
const RESUME_EVENT: &str = "4ae1c07c-9242-46e9-a9d2-4ec58c130f4b";
const RESUME_CONNECTION: &str = "5ae1c07c-9242-46e9-a9d2-4ec58c130f4c";

fn resume_timestamp() -> TimestampText {
    TimestampText::new("2026-09-18T09:00:00.000Z").expect("固定时间戳")
}

fn resume_agent() -> AgentRef {
    AgentRef::try_new(AgentId::new("codex").expect("agent id"), "Codex").expect("agent ref")
}

fn resume_session_id() -> SessionId {
    SessionId::new(RESUME_SESSION).expect("session id")
}

/// 会话聚合：归属校验读 `session.agent()`，因此 agent 必须与 Export 的清单一致。
fn resume_session() -> Session {
    Session::try_new(
        resume_session_id(),
        OwnedSessionRef::new(resume_session_id()),
        Some("Session".to_owned()),
        resume_agent(),
        SessionState::Idle,
        ResourceOrigin::Local,
        None,
        Version::new(3),
        resume_timestamp(),
        resume_timestamp(),
        None,
    )
    .expect("会话聚合")
}

fn resume_summary() -> SessionSummary {
    SessionSummary::try_new(
        resume_session_id(),
        Some("Session".to_owned()),
        resume_agent(),
        SessionState::Idle,
        ResourceOrigin::Local,
        None,
        Version::new(3),
        resume_timestamp(),
        resume_timestamp(),
        None,
    )
    .expect("会话摘要")
}

fn resume_origin_head(sequence: u64) -> OriginCursor {
    OriginCursor::new(
        OriginEpoch::new(RESUME_ORIGIN_EPOCH).expect("epoch"),
        Sequence::new(sequence).expect("sequence"),
    )
}

/// 一条已持久化的会话事件（`payload` 正文经会话绑定的读入口取回）。
fn resume_event() -> CommittedEvent {
    CommittedEvent {
        id: EventId::new(RESUME_EVENT).expect("event id"),
        event_type: EventType::new("agent.message").expect("event type"),
        session: Some(resume_session_id()),
        session_sequence: Some(Sequence::new(1).expect("sequence")),
        global_sequence: Sequence::new(1).expect("sequence"),
        origin_epoch: Some(OriginEpoch::new(RESUME_ORIGIN_EPOCH).expect("epoch")),
        origin_sequence: Some(Sequence::new(1).expect("sequence")),
        created_at: resume_timestamp(),
    }
}

/// 一条已配对的 Access 信任行：两个 Export 都在清单里（不在清单里的 Export 一概不授予）。
fn resume_node() -> NodeRecord {
    NodeRecord::try_new(
        node(ACCESS_NODE),
        "Office Access",
        NodeKind::Access,
        test_public_key().fingerprint(),
        GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
        vec![
            ExportId::new(RESUME_EXPORT_A).expect("export id"),
            ExportId::new(RESUME_EXPORT_B).expect("export id"),
        ],
        NodeState::Paired,
        None,
        resume_timestamp(),
        None,
        None,
    )
    .expect("信任行")
}

/// 一条可见 Export：零参数 template（带参数的投影整体失败关闭）、agent 与会话一致。
fn resume_export(id: &str) -> ExportRecord {
    ExportRecord::try_new(
        ExportId::new(id).expect("export id"),
        "Project Export",
        vec![AgentId::new("codex").expect("agent id")],
        vec![
            WorkspaceAliasEntry::try_new(WorkspaceAlias::new("project").expect("alias"), "Project")
                .expect("alias entry"),
        ],
        WorkspaceAlias::new("project").expect("alias"),
        vec![
            ExportTemplate::try_new(
                TemplateId::new("template-1").expect("template id"),
                "Template",
                WorkspaceAlias::new("project").expect("alias"),
                Vec::new(),
            )
            .expect("template"),
        ],
        TemplateId::new("template-1").expect("template id"),
        GrantSet::try_from_iter(["grant.observe"]).expect("scopes"),
        CachePolicy::NoContentCache,
        resume_timestamp(),
        None,
    )
    .expect("Export 记录")
}

/// Node Link 会话读面的最小存储替身：只实现快照/扇出路径触及的方法（其余一律 `unreachable!`，
/// 替身不比真实存储宽容）。
#[derive(Clone)]
struct ResumeStore {
    head: GlobalCursor,
    slice: NodeLinkSlice,
}

impl ResumeStore {
    /// 一个会话 + 一条已持久化事件（未决交互为空：快照因此只有一个 `session_meta` chunk）。
    fn new() -> Self {
        Self {
            head: GlobalCursor::new(
                ServerEpoch::new(TEST_SERVER_EPOCH).expect("server epoch"),
                Sequence::new(1).expect("sequence"),
            ),
            slice: NodeLinkSlice {
                summary: resume_summary(),
                head: resume_origin_head(1),
                pending_interactions: Vec::new(),
                events: vec![OwnedEventRecord {
                    event: resume_event(),
                    payload: EventPayload::new(
                        ViewJson::new(r#"{"kind":"agent.message"}"#).expect("view"),
                        None,
                    ),
                }],
            },
        }
    }
}

#[async_trait::async_trait]
impl SessionStore for ResumeStore {
    async fn commit(&self, _commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        unreachable!("受害面用例不写存储")
    }

    async fn load(&self, session: &SessionId) -> Result<Option<SessionSnapshot>, PortError> {
        if session != &resume_session_id() {
            return Ok(None);
        }
        Ok(Some(SessionSnapshot {
            session: resume_session(),
            origin_epoch: OriginEpoch::new(RESUME_ORIGIN_EPOCH).expect("epoch"),
            head: self.head.clone(),
        }))
    }

    async fn list(&self, _query: SessionQuery) -> Result<Vec<SessionSummary>, PortError> {
        unreachable!("受害面用例不列举会话")
    }

    async fn head(&self) -> Result<GlobalCursor, PortError> {
        Ok(self.head.clone())
    }

    async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError> {
        Ok(Box::new(self.clone()))
    }

    async fn find_request(
        &self,
        _request: &RequestId,
        _actor: &Actor,
    ) -> Result<Option<CommandRecord>, PortError> {
        unreachable!("受害面用例不查命令")
    }

    async fn load_recovery(
        &self,
        _session: &SessionId,
    ) -> Result<Option<acp_core::model::SessionRecoveryRecord>, PortError> {
        unreachable!("受害面用例不读恢复数据")
    }

    async fn unsettled_commands(
        &self,
        _limit: ReplayLimit,
    ) -> Result<Vec<CommandRecord>, PortError> {
        unreachable!("受害面用例不查命令")
    }

    async fn retention_window(
        &self,
        _session: &SessionId,
    ) -> Result<Option<(Sequence, Sequence)>, PortError> {
        unreachable!("受害面用例不查保留窗口")
    }

    async fn prune(
        &self,
        _policy: RetentionPolicy,
        _at: TimestampText,
    ) -> Result<PruneReport, PortError> {
        unreachable!("受害面用例不清扫")
    }

    async fn health(&self) -> Result<StoreHealth, PortError> {
        unreachable!("受害面用例不查健康")
    }
}

#[async_trait::async_trait]
impl ReadView for ResumeStore {
    async fn head(&self) -> Result<GlobalCursor, PortError> {
        Ok(self.head.clone())
    }

    async fn replay(
        &self,
        _after: Option<GlobalCursor>,
        _limit: ReplayLimit,
    ) -> Result<ReplayBatch, PortError> {
        unreachable!("受害面用例不读 Sync 重放面")
    }

    /// Sync 事件读面不在这个替身的范围内：受害面用例只驱动 Node Link 自己的路径。
    ///
    /// `sync_replay` 由 `sync-server-connection` 的 1.6 新增进 `ReadView`；本替身走同款机械桩，
    /// 与 `node_link/command/tests.rs` 的同名实现一致（真被调用即 `unreachable!`，让缺口显式而非静默）。
    async fn sync_replay(
        &self,
        _after: Option<GlobalCursor>,
        _limit: ReplayLimit,
    ) -> Result<acp_core::ports::SyncReplaySlice, PortError> {
        unreachable!("受害面用例不读 Sync 事件重放面")
    }

    async fn read_session(&self, _query: HistoryQuery) -> Result<HistoryPage, PortError> {
        unreachable!("受害面用例不读历史面")
    }

    async fn event_payload(&self, _event: &EventId) -> Result<Option<EventPayload>, PortError> {
        unreachable!("受害面用例只经会话绑定的读入口取正文")
    }

    /// 只按 `origin_sequence > after` 取行；`after` 的 epoch 解释权在用例层（与真实存储同口径）。
    async fn node_link_slice(
        &self,
        session: &SessionId,
        after: Option<OriginCursor>,
        _limit: ReplayLimit,
    ) -> Result<NodeLinkSlice, PortError> {
        if session != &resume_session_id() {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        }
        let mut slice = self.slice.clone();
        if let Some(after) = after {
            slice
                .events
                .retain(|record| record.event.origin_sequence > Some(after.origin_sequence));
        }
        Ok(slice)
    }

    async fn session_event_payload(
        &self,
        session: &SessionId,
        event: &EventId,
    ) -> Result<Option<EventPayload>, PortError> {
        if session != &resume_session_id() {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        }
        Ok(self
            .slice
            .events
            .iter()
            .find(|record| &record.event.id == event)
            .map(|record| record.payload.clone()))
    }
}

/// 认证后消息的信封（连接字段按 §2.2 必需；序号在本组用例里不参与判定）。
fn resume_envelope(handle: &ConnectionHandle, message_type: MessageType, body: Value) -> Envelope {
    Envelope::new(
        message_type,
        Uuid::parse(&uuid_text()).expect("message id"),
        Some(ConnectionFields {
            connection_id: handle.connection_id().clone(),
            connection_sequence: DecimalString::parse("1").expect("sequence"),
        }),
        serde_json::value::RawValue::from_string(body.to_string()).expect("body"),
    )
    .expect("信封")
}

/// 取走出站队列里已有的消息并记账（模拟会话写出），返回解析后的帧。
fn deliver_pending(
    handle: &ConnectionHandle,
    outbound: &mut mpsc::Receiver<Outbound>,
) -> Vec<Value> {
    let mut frames = Vec::new();
    while let Ok(outbound) = outbound.try_recv() {
        handle.consumed(&outbound);
        frames.push(serde_json::from_str(&outbound.text).expect("帧是合法 JSON"));
    }
    frames
}

/// 出站队列里各帧的 `type`（顺序即投递顺序）。
fn frame_types(frames: &[Value]) -> Vec<&str> {
    frames
        .iter()
        .map(|frame| frame["type"].as_str().expect("帧必须有 type"))
        .collect()
}

/// 饱和 → 恢复后，读入门必须重新开始读取批次（`catalog.rs` 的批间门 + `resource.rs` 的 chunk 门）。
///
/// 两个门都以 `handle.saturated()` 判定：饱和状态一旦粘滞（清除点不可达），恢复后的每一次
/// `catalog.subscribe`/`resource.subscribe` 都会在批量中途、第一个 chunk 之前被拦下——连接还在、订阅也
/// 成功，但快照批次与事件再也不会到达。
///
/// 断言的是**全量恢复**：协商批次=1、可见 2 条 → 目录快照必须到两帧；资源快照必须走完
/// `snapshot_begin → chunk → snapshot_end`；扇出的 `resource.event` 也必须送达。把任一受害门改成
/// 「永不再读」时，前半只剩一条批次、后半停在 `snapshot_begin`。
#[tokio::test]
async fn a_recovered_connection_resumes_catalog_batches_and_event_delivery() {
    let world = TestWorld::with_store(Arc::new(ResumeStore::new()));
    world.trust.seed_node(resume_node());
    world.exports.seed_export(resume_export(RESUME_EXPORT_A));
    world.exports.seed_export(resume_export(RESUME_EXPORT_B));

    let registry = ConnectionRegistry::new();
    let (handle, mut outbound) = ConnectionHandle::new(
        Uuid::parse(RESUME_CONNECTION).expect("连接标识是规范 uuid"),
        node(ACCESS_NODE),
        "127.0.0.1".parse().expect("loopback 地址"),
        SessionLimits::negotiate(&NodeLinkConfig {
            // 协商批次=1、可见 2 条：目录快照因此必须切两批，批间门才有机会被命中。
            catalog_snapshot_batch_size: 1,
            resource_snapshot_batch_size: 1,
            max_pending_queue_messages: 4,
            ..NodeLinkConfig::default()
        }),
        Arc::new(FakeIds::default()),
    );
    registry.register(Arc::clone(&handle));
    let catalog_route = CatalogRoute::new(Arc::clone(&world.core));
    let resource_route = Arc::new(ResourceRoute::new(
        Arc::clone(&world.core),
        Arc::clone(&registry),
        world.authority.local_node().clone(),
    ));

    // ① 顶到高水位（第 5 条被拒）→ 这条连接真的进入过饱和。
    assert_eq!(
        saturate(&handle),
        1,
        "必须先真的进入过饱和，否则本用例没有受害面"
    );
    assert!(handle.saturated());
    assert!(handle.saturated_for().is_some());

    // ② 按会话侧的口径排空（`consumed` 是唯一的清除点）→ 恢复。
    let drained = deliver_pending(&handle, &mut outbound);
    assert_eq!(drained.len(), 4, "排空的正是刚才入队的那几条");
    assert_eq!(handle.pending().messages, 0);
    assert!(!handle.saturated(), "排到低水位以下必须清除饱和状态");

    // ③ 目录快照：两批都必须到达（批间门在 index > 0 的那一批上）。
    let subscribe = resume_envelope(
        &handle,
        MessageType::CatalogSubscribe,
        json!({ "knownRevision": null }),
    );
    assert_eq!(
        catalog_route.route(&handle, &subscribe).await,
        RouteOutcome::Claimed
    );
    let frames = deliver_pending(&handle, &mut outbound);
    assert_eq!(
        frame_types(&frames),
        ["catalog.snapshot", "catalog.snapshot"],
        "恢复后必须重新发出**全部**快照批次（两条可见 Export、协商批次 1）"
    );
    let export_ids: Vec<&str> = frames
        .iter()
        .map(|frame| {
            frame["body"]["exports"][0]["exportId"]
                .as_str()
                .expect("exportId")
        })
        .collect();
    assert_eq!(
        export_ids,
        [RESUME_EXPORT_A, RESUME_EXPORT_B],
        "批次顺序按 exportId 升序，且不得丢批"
    );

    // ④ 资源快照：attach → subscribe(cursor = null) 必须走完 begin → chunk → end。
    let attach = resume_envelope(
        &handle,
        MessageType::ResourceAttach,
        json!({
            "remoteSessionRef": {
                "ownerNodeId": world.authority.local_node().as_str(),
                "exportId": RESUME_EXPORT_A,
                "sessionId": RESUME_SESSION,
            }
        }),
    );
    assert_eq!(
        resource_route.route(&handle, &attach).await,
        RouteOutcome::Claimed
    );
    let attached = deliver_pending(&handle, &mut outbound);
    assert_eq!(
        frame_types(&attached),
        ["resource.attached"],
        "attach 必须回一帧"
    );
    let subscribe = resume_envelope(
        &handle,
        MessageType::ResourceSubscribe,
        json!({
            "attachmentId": attached[0]["body"]["attachmentId"],
            "attachmentGeneration": attached[0]["body"]["attachmentGeneration"],
            "cursor": null,
        }),
    );
    assert_eq!(
        resource_route.route(&handle, &subscribe).await,
        RouteOutcome::Claimed
    );
    let snapshot = deliver_pending(&handle, &mut outbound);
    assert_eq!(
        frame_types(&snapshot),
        [
            "resource.snapshot_begin",
            "resource.snapshot_chunk",
            "resource.snapshot_end"
        ],
        "恢复后快照 chunk 必须重新开始发出（chunk 门在第一个 chunk 之前）"
    );

    // ⑤ 扇出：已持久化事件必须投给这条订阅过的连接。
    let (publisher, queue) = NodeLinkPublisher::channel();
    let (shutdown_handle, shutdown) = Shutdown::channel();
    let dispatch = tokio::spawn(Arc::clone(&resource_route).dispatch(queue, shutdown));
    publisher.publish(CommittedDelivery::Owned(resume_event()));
    let frame = tokio::time::timeout(IO_TIMEOUT, outbound.recv())
        .await
        .expect("扇出必须送达")
        .expect("队列未关闭");
    let frame: Value = serde_json::from_str(&frame.text).expect("帧是合法 JSON");
    assert_eq!(frame["type"], "resource.event");
    assert_eq!(frame["body"]["originEventId"], RESUME_EVENT);

    // 关闭序列：分发循环随 Shutdown 结束（不留 detached task）。
    shutdown_handle.trigger();
    tokio::time::timeout(IO_TIMEOUT, dispatch)
        .await
        .expect("分发循环必须随关闭信号结束")
        .expect("分发任务不 panic");
}
