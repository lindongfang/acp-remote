//! `server::sync::conn` 的行为用例（`sync-device-auth` 的 WP3 部分）。
//!
//! 驱动方式：真实 loopback listener（只经 `transport::net` 的公开形状注册 WSS 处理器）+ 本模块自带的
//! 极简 WebSocket 客户端。因此「subprotocol 准入 → 帧 → 信封 → 四消息握手 → 业务阶段 → close code」
//! 整条链路都在真实连接上被验证，接入层与协议层之间的接线（binary 帧、15 秒窗口、请求来源）不是手工
//! 调用的结果。
//!
//! 设备侧的持久事实由**真实的本地通道**推进（`device.pair.begin` → `claim` → `device.pair.confirm`），
//! 与 WP5 的 HTTPS claim 路径落库的事实同形。区别只有一处，见 [`SyncTrust`]：`FakeTrust` 的
//! `put_device` 是 `unreachable!`（WP3 之前没有调用方），而设备认证收尾必须经它推进 `last_seen_at`。
//!
//! 覆盖映射（`specs/sync-device-auth/spec.md` 的八条要求）：
//!
//! | 需求 | 用例 |
//! |---|---|
//! | WSS 端点与 subprotocol | `handshake_completes_and_enters_the_business_phase`、`the_endpoint_rejects_another_subprotocol` |
//! | 非文本帧被拒绝 | `a_binary_frame_is_closed_with_4400`（认证前后各一次） |
//! | 认证消息序列与窗口 | `handshake_completes_and_enters_the_business_phase`、`a_business_message_before_hello_is_closed_with_4401`、`a_duplicate_hello_is_closed_with_4401`、`the_first_message_window_is_five_seconds_not_the_whole_fifteen`、`the_handshake_still_has_fifteen_seconds_after_the_hello` |
//! | 来源绑定来自已验证的 HTTP Origin | `an_origin_that_differs_from_the_binding_is_refused`、`a_missing_origin_is_refused`、`a_non_https_origin_is_refused` |
//! | 未知与已撤销设备的可见行为 | `an_unknown_device_gets_a_challenge_and_fails_as_proof_invalid`、`a_revoked_device_is_closed_with_4410` |
//! | 单设备只保留一条活动连接 | `a_new_connection_replaces_the_previous_one` |
//! | 撤销立即传播 | `revocation_requests_the_close_of_the_active_connection` |
//! | 心跳与静默超时 | `heartbeat_round_trip_keeps_the_connection_alive`、`silence_beyond_the_window_is_closed_with_4408` |
//! | 限额下发只下调 | `authenticated_echoes_the_negotiated_limits_and_never_raises_them` |
//! | 首次认证的写集（配对消费 + `last_seen_at` + 审计） | `the_first_authentication_consumes_the_pairing_and_advances_last_seen` |
//! | 信封/版本/schema、序号与 type 判定 | `envelope_sequence_and_type_violations_are_rejected_without_closing`、`an_envelope_version_other_than_v1_is_closed_with_4406`、`a_hello_without_a_common_version_is_closed_with_4406`、`a_missing_required_feature_is_reported_with_details_and_closed` |
//! | 协议错误码与入站序号推进 | `an_unknown_message_type_is_rejected_as_type_unsupported_and_keeps_the_sequence`、`a_schema_invalid_envelope_that_consumed_the_sequence_does_not_strand_the_connection` |
//! | feature 交集（排序去重） | `selected_features_are_the_sorted_intersection` |
//! | 出站队列双上限与序号账本 | `the_message_high_water_is_reported_without_consuming_a_sequence`、`the_byte_budget_triggers_before_the_message_count` |
//! | 持续过慢的判定口径（§14 的两级背压） | `the_saturation_clock_survives_jitter_and_clears_only_below_the_low_water_mark`、`a_connection_that_stays_at_high_water_is_closed_after_the_grace`、`a_transient_burst_that_drains_does_not_trip_the_slow_consumer_watchdog`、`a_heartbeat_send_at_high_water_defers_instead_of_closing` |
//! | §14 的认证前固定上限 | `the_sixth_in_flight_handshake_from_one_address_is_refused`（单 IP 在途握手 5 条）、`the_eleventh_authentication_attempt_from_one_address_is_rate_limited`（单 IP 新认证尝试 10 次/分钟）、`the_first_message_window_is_five_seconds_not_the_whole_fifteen`（首条消息 5 秒）、`an_oversized_client_hello_is_rejected_before_the_handshake`（hello 32 KiB）、`the_node_concurrent_connection_cap_refuses_the_extra_connection`（单节点 256 条） |
//!
//! 一处**刻意的测试手法**：心跳节奏 / 90 秒静默 / 30 秒慢消费者宽限 / 单节点并发 256 条用
//! [`SyncConn::with_test_windows`] 与 [`SyncConn::with_test_connection_cap`] 调小（四个常量的取值由
//! `conn::limits`、`conn::session` 与 `conn::registry` 的常量断言锁定，机制由真实连接覆盖）。wire 上的
//! `heartbeatIntervalMs` 仍是固定常量 30 秒，与用例里的节奏刻意不同——对端被告知的始终是协议值。

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::{
    AuditAction, AuditOutcome, ConflictKind, DeviceId, DeviceRecord, DeviceState, EntityRef,
    NodeId, PairingId, PairingPeer, PairingState, PeerIdentity, PortError, Timestamp,
};
use acp_core::ports::{
    AgentCatalog, AuditQuery, AuditStore, Clock, DeviceWrite, ExportStore, PairingClaimOutcome,
    PendingAudit, RemoteDeliveryStore, SessionStore, TrustRecordRef, TrustStore,
};
use acp_core::use_cases::{UseCaseDeps, UseCases};
use base64::Engine as _;
use identity_auth::{
    CanonicalOrigin, ChallengeId, FeatureList, P1363Signature, SyncDeviceProof, SyncHostChallenge,
};
use serde_json::{Value, json};
use sync_protocol::envelope::MessageType;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;

use crate::local_admin::envelope::{AdminOutcome, AdminRequest, AdminResponse, decode_request};
use crate::local_admin::handler::LocalAdminHandler as _;
use crate::local_admin::method::Method;
use crate::local_admin::test_support::{
    FakeAudit, FakeConfig, FakeExports, FakeIds, FakeTrust, FixedStore, NotTouched,
    TEST_PUBLIC_ORIGIN, TEST_SERVER_EPOCH, TestWorld, test_nonce, test_public_key,
};
use crate::sync::conn::limits::{
    DEFAULT_HEARTBEAT_INTERVAL_MS, DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_MAX_PROMPT_BYTES,
    DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH, FIRST_MESSAGE_TIMEOUT, HELLO_MAX_BYTES,
    MAX_CONCURRENT_CONNECTIONS, MAX_PENDING_QUEUE_BYTES, MAX_PENDING_QUEUE_MESSAGES, SyncConfig,
};
use crate::sync::conn::registry::{ConnectionHandle, Outbound, RECOVERY_MESSAGES, SendFault};
use crate::sync::conn::session::{HEARTBEAT_SILENCE_TIMEOUT, HeartbeatOutcome, heartbeat_outcome};
use crate::sync::conn::{
    AUTH_ATTEMPTS_PER_MINUTE, AUTH_WINDOW, ConnectionRegistry, HANDSHAKE_TIMEOUT,
    MAX_IN_FLIGHT_HANDSHAKES_PER_IP, SUPPORTED_FEATURES, SessionLimits, SyncConn, WS_PATH,
    WS_SUBPROTOCOL, WsEndpoint, close,
};
use crate::transport::net::{NetConfig, NetError, NetListener, Shutdown, ShutdownHandle};

// ---------------------------------------------------------------------------------------------
// 常量与固定素材
// ---------------------------------------------------------------------------------------------

/// 本机 host id（与 `TestWorld` 装配的 `Authority` 同源）。
const OWNER_NODE: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
/// 测试扮演的浏览器设备（经真实配对通道批准）。
const DEVICE_ID: &str = "2ae1c07c-0000-4000-8000-0000000000a1";
/// 从未配对的设备。
const UNKNOWN_DEVICE: &str = "9c8f6b1d-7a35-4f0b-9b6a-2f6d5c4e3b1a";
/// 设备记录绑定的 canonical origin（`device.pair.begin` 登记的 `daemon.public_origin`）。
const DEVICE_ORIGIN: &str = TEST_PUBLIC_ORIGIN;
/// `FixedStore` 的 `head()` 值（`auth.authenticated.serverEpoch`/`headGlobalSequence` 的来源）。
const STORE_HEAD_SEQUENCE: u64 = 7;
/// 客户端 `clientNonce` 的固定素材（每次连接重新生成；用例只需要「两边对同一个值」）。
const CLIENT_NONCE_SEED: u8 = 0x31;
/// 单次 I/O 的超时（用例失败要快速失败，不挂住测试进程）。
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// 两条时限用例（首条消息 5 秒 / 整条认证 15 秒）需要等满 15 秒窗口，故读取预算放宽到 25 秒。
const TIMEOUT_WINDOW_IO: Duration = Duration::from_secs(25);

// ---------------------------------------------------------------------------------------------
// 信任替身：只补 `put_device`
// ---------------------------------------------------------------------------------------------

/// 设备认证收尾需要的写面。
///
/// `FakeTrust` 的 `put_device` 是 `unreachable!`（WP3 之前没有调用方），而 `UseCases::record_device_connected`
/// 是 `owned_device.last_seen_at` 的唯一生产者、必须经它落库。因此这里只补**那一个方法**，其余全部委托
/// 给同一个 `FakeTrust` 实例（读取面、`consume_pairing`、`revoke_device` 用的都是它已实现、已照抄存储层
/// 语义的版本）。`put_device` 本身照抄 `storage-sqlite::admin::trust` 的 `put_device`/`upsert_device`
/// 的可观察语义：拒绝 `revoked` 写入、同 ID 不得换绑指纹、身份材料必须已绑定、`last_seen_at` 只前进。
struct SyncTrust {
    inner: FakeTrust,
    /// 写集里的审计行落到 `AuditStore` 替身（`FakeTrust` 只暴露审计读面，没有写入面）。
    audits: FakeAudit,
    /// 设备已绑定的验签公钥（`TrustStore::peer_key` 对设备侧的来源）。
    ///
    /// `FakeTrust` 只在 `approve_node` 里把对端公钥从 `owned_pairing_peer` 转入 `keys`（照抄
    /// §11.6 第 4 条），**设备**分支的 `approve_device` 漏了同一步——此前没有设备走完整 WSS 握手，
    /// 因此从不被察觉。真实存储的两个分支都会写 `owned_peer_key`，所以这里补上等价事实：配对确认之后
    /// 由用例经 [`SyncTrust::bind_device_key`] 显式绑定（不是「随便给一把钥匙」，而是把配对批准已经
    /// 落库的那把公钥按存储层的方式暴露给验签读取面）。
    device_keys: Mutex<BTreeMap<String, acp_core::model::PeerPublicKey>>,
}

impl SyncTrust {
    /// 绑定设备侧验签公钥（见 [`SyncTrust::device_keys`]）。
    fn bind_device_key(&self, device: &DeviceId, key: acp_core::model::PeerPublicKey) {
        self.device_keys
            .lock()
            .expect("设备公钥锁")
            .insert(device.as_str().to_owned(), key);
    }

    /// `upsert_device` 的三分支 `CASE`：新值为空保留旧值、旧值为空取新值、否则取较大者。
    fn advance_last_seen(
        stored: Option<Timestamp>,
        incoming: Option<Timestamp>,
    ) -> Option<Timestamp> {
        match (stored, incoming) {
            (stored, None) => stored,
            (None, incoming) => incoming,
            (Some(stored), Some(incoming)) => Some(if incoming.as_str() > stored.as_str() {
                incoming
            } else {
                stored
            }),
        }
    }
}

#[async_trait::async_trait]
impl TrustStore for SyncTrust {
    // 两个方法必须用 UFCS 调用：`FakeTrust` 有同名的**固有**方法（`device(&str)`/`pairing(&str)`），
    // 它们会遮蔽 trait 方法，直接 `self.inner.device(id)` 会解析到固有方法。
    async fn device(&self, id: &DeviceId) -> Result<Option<DeviceRecord>, PortError> {
        TrustStore::device(&self.inner, id).await
    }

    async fn devices(&self) -> Result<Vec<DeviceRecord>, PortError> {
        self.inner.devices().await
    }

    async fn node(
        &self,
        id: &NodeId,
        kind: acp_core::model::NodeKind,
    ) -> Result<Option<acp_core::model::NodeRecord>, PortError> {
        self.inner.node(id, kind).await
    }

    async fn nodes(&self) -> Result<Vec<acp_core::model::NodeRecord>, PortError> {
        self.inner.nodes().await
    }

    async fn nodes_for(&self, id: &NodeId) -> Result<Vec<acp_core::model::NodeRecord>, PortError> {
        self.inner.nodes_for(id).await
    }

    async fn peer_key(
        &self,
        peer: &PeerIdentity,
    ) -> Result<Option<acp_core::model::PeerPublicKey>, PortError> {
        if let PeerIdentity::Device(device) = peer {
            // 锁只在求值这条语句期间持有：`MutexGuard` 不是 `Send`，绝不能跨 `await` 活着。
            let bound = self
                .device_keys
                .lock()
                .expect("设备公钥锁")
                .get(device.as_str())
                .cloned();
            if let Some(key) = bound {
                return Ok(Some(key));
            }
        }
        self.inner.peer_key(peer).await
    }

    async fn pairing(
        &self,
        id: &PairingId,
    ) -> Result<Option<acp_core::model::PairingRecord>, PortError> {
        TrustStore::pairing(&self.inner, id).await
    }

    async fn pairing_peer(&self, id: &PairingId) -> Result<Option<PairingPeer>, PortError> {
        self.inner.pairing_peer(id).await
    }

    async fn pairing_for(
        &self,
        peer: &PeerIdentity,
    ) -> Result<Option<acp_core::model::PairingRecord>, PortError> {
        self.inner.pairing_for(peer).await
    }

    /// **本替身唯一自己实现的方法**（见 [`SyncTrust`] 的文档）。
    async fn put_device(&self, write: DeviceWrite) -> Result<(), PortError> {
        let record = write.record;
        if record.state() == DeviceState::Revoked {
            return Err(PortError::InvalidRequest(
                "use device.revoke to revoke a device",
            ));
        }
        let device = record.device_id().clone();
        let fingerprint = record.public_key_fingerprint().clone();
        let bound = self.peer_key(&PeerIdentity::Device(device.clone())).await?;
        if bound
            .as_ref()
            .is_some_and(|key| key.fingerprint() != fingerprint)
        {
            return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
        }
        let existing = self.inner.device(device.as_str());
        let conflicting = existing.as_ref().is_some_and(|row| {
            row.public_key_fingerprint() != &fingerprint || row.state() == DeviceState::Revoked
        });
        if conflicting {
            return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
        }
        if bound.is_none() && existing.is_none() {
            return Err(PortError::InvalidRequest(
                "device identity material is not bound",
            ));
        }
        let advanced = DeviceRecord::try_new(
            device,
            record.display_name(),
            fingerprint,
            record.scopes().clone(),
            record.state(),
            record.created_at().clone(),
            Self::advance_last_seen(
                existing
                    .as_ref()
                    .and_then(|row| row.last_seen_at().cloned()),
                record.last_seen_at().cloned(),
            ),
            record.revoked_at().cloned(),
        )
        .map_err(PortError::from)?;
        self.inner.seed_device(advanced);
        for pending in &write.context.audit {
            self.audits
                .append(audit_row(&write.context.at, pending)?)
                .await?;
        }
        Ok(())
    }

    async fn put_node(&self, write: acp_core::ports::NodeWrite) -> Result<(), PortError> {
        self.inner.put_node(write).await
    }

    async fn revoke_device(
        &self,
        write: acp_core::ports::DeviceRevocation,
    ) -> Result<(), PortError> {
        self.inner.revoke_device(write).await
    }

    async fn revoke_node(&self, write: acp_core::ports::NodeRevocation) -> Result<(), PortError> {
        self.inner.revoke_node(write).await
    }

    async fn create_pairing(&self, write: acp_core::ports::PairingWrite) -> Result<(), PortError> {
        self.inner.create_pairing(write).await
    }

    async fn claim_pairing(
        &self,
        write: acp_core::ports::PairingClaimWrite,
    ) -> Result<PairingClaimOutcome, PortError> {
        self.inner.claim_pairing(write).await
    }

    async fn settle_pairing(
        &self,
        write: acp_core::ports::PairingSettlementWrite,
    ) -> Result<TrustRecordRef, PortError> {
        self.inner.settle_pairing(write).await
    }

    async fn expire_pairings(&self, write: acp_core::ports::ExpiryWrite) -> Result<u64, PortError> {
        self.inner.expire_pairings(write).await
    }

    async fn consume_pairing(
        &self,
        write: acp_core::ports::PairingConsumption,
    ) -> Result<acp_core::model::PairingRecord, PortError> {
        self.inner.consume_pairing(write).await
    }

    async fn record_node_connected(
        &self,
        write: acp_core::ports::NodeConnectedWrite,
    ) -> Result<(), PortError> {
        self.inner.record_node_connected(write).await
    }
}

/// `PendingAudit` → `AuditRecord`（与 `FakeTrust::append_audits` 同一转换）。
fn audit_row(
    at: &Timestamp,
    pending: &PendingAudit,
) -> Result<acp_core::model::AuditRecord, PortError> {
    acp_core::model::AuditRecord::try_new(
        at.clone(),
        pending.action,
        pending.actor.clone(),
        pending.via_node.clone(),
        pending.local_principal_ref.clone(),
        pending.target.clone(),
        pending.outcome,
        pending.detail_digest.clone(),
    )
    .map_err(PortError::from)
}

/// Sync 握手不读 Agent 目录：给组合提供一个空目录（其余方法不可触及）。
struct EmptyCatalog;

#[async_trait::async_trait]
impl AgentCatalog for EmptyCatalog {
    async fn agents(&self) -> Result<Vec<acp_core::model::AgentDescriptor>, PortError> {
        Ok(Vec::new())
    }

    async fn agent_capabilities(
        &self,
        _agent: &acp_core::model::AgentRef,
    ) -> Result<acp_core::model::CapabilitySet, PortError> {
        unreachable!("Sync 握手不读 catalog")
    }
}

// ---------------------------------------------------------------------------------------------
// 世界与端点
// ---------------------------------------------------------------------------------------------

/// 用例对生产常量的调小（§14 的固定值由 `conn::limits`/`conn::session`/`conn::registry` 的常量断言锁定，
/// 机制由真实连接覆盖；30 秒/90 秒的等待与 257 条连接在用例里不可接受）。
#[derive(Clone, Copy, Default)]
struct Tuning {
    /// 会话窗口（心跳节奏、静默、慢消费者宽限）。
    windows: Option<(Duration, Duration, Duration)>,
    /// 单节点并发 WSS 连接上限（§14 的 256）。
    connection_cap: Option<usize>,
}

/// 端口替身世界 + 真实 loopback listener + 已注册的 WSS 端点。
struct Harness {
    world: TestWorld,
    audit: FakeAudit,
    trust: Arc<SyncTrust>,
    conn: Arc<SyncConn>,
    addr: SocketAddr,
    shutdown: ShutdownHandle,
    join: JoinHandle<Result<(), NetError>>,
}

impl Harness {
    /// 起一个已注册 `/sync/v1` 的接入层（`127.0.0.1:0`，端口由内核分配）。
    async fn start(world: TestWorld, config: SyncConfig, tuning: Tuning) -> Self {
        let trust = world.trust.clone();
        let audit = FakeAudit::default();
        let clock = world.clock.clone();
        let sync_trust = Arc::new(SyncTrust {
            inner: trust,
            audits: audit.clone(),
            device_keys: Mutex::new(BTreeMap::new()),
        });
        let store: Arc<dyn SessionStore> =
            Arc::new(FixedStore::new(TEST_SERVER_EPOCH, STORE_HEAD_SEQUENCE));
        let deliveries: Arc<dyn RemoteDeliveryStore> = Arc::new(NotTouched);
        let exports: Arc<dyn ExportStore> = Arc::new(FakeExports::default());
        let audit_port: Arc<dyn AuditStore> = Arc::new(audit.clone());
        let clock_port: Arc<dyn Clock> = Arc::new(clock);
        let trust_port: Arc<dyn TrustStore> = Arc::clone(&sync_trust) as Arc<dyn TrustStore>;
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: Arc::clone(&store),
                deliveries: Arc::clone(&deliveries),
                backends: Arc::new(NotTouched),
                exports: Arc::clone(&exports),
                trust: Arc::clone(&trust_port),
                publisher: Arc::new(NotTouched),
                clock: Arc::clone(&clock_port),
                ids: Arc::new(FakeIds::default()),
                audit: Some(Arc::clone(&audit_port)),
            },
            BrokerConfig::default(),
        ));
        let core = Arc::new(UseCases::new(UseCaseDeps {
            broker,
            store,
            deliveries,
            exports,
            trust: trust_port,
            audit: audit_port,
            config: Arc::new(FakeConfig::default()),
            attachments: Arc::new(NotTouched),
            catalog: Arc::new(EmptyCatalog),
            clock: clock_port,
            ids: Arc::new(FakeIds::default()),
        }));
        let registry = ConnectionRegistry::new();
        let mut conn = SyncConn::new(
            Arc::clone(&core),
            Arc::clone(&world.authority),
            config,
            Arc::clone(&registry),
        );
        if let Some((heartbeat, silence, slow_consumer)) = tuning.windows {
            conn = conn.with_test_windows(heartbeat, silence, slow_consumer);
        }
        if let Some(cap) = tuning.connection_cap {
            conn = conn.with_test_connection_cap(cap);
        }
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
            .expect("注册 Sync 端点");
        let addr = listener.local_addrs()[0];
        let (shutdown, signal) = Shutdown::channel();
        let join = tokio::spawn(async move { listener.serve(signal).await });
        Self {
            world,
            audit,
            trust: sync_trust,
            conn,
            addr,
            shutdown,
            join,
        }
    }

    /// 默认世界与默认配置（`serverEpoch` 固定为 [`TEST_SERVER_EPOCH`]，水位固定为
    /// [`STORE_HEAD_SEQUENCE`]）。
    async fn new() -> Self {
        Self::with_config(SyncConfig::default()).await
    }

    /// 默认世界 + 给定配置。
    async fn with_config(config: SyncConfig) -> Self {
        Self::start(
            TestWorld::with_store(Arc::new(FixedStore::new(
                TEST_SERVER_EPOCH,
                STORE_HEAD_SEQUENCE,
            ))),
            config,
            Tuning::default(),
        )
        .await
    }

    /// 默认世界 + 调短的会话窗口（心跳、静默、慢消费者宽限）。
    async fn with_windows(heartbeat: Duration, silence: Duration, slow_consumer: Duration) -> Self {
        Self::start(
            TestWorld::with_store(Arc::new(FixedStore::new(
                TEST_SERVER_EPOCH,
                STORE_HEAD_SEQUENCE,
            ))),
            SyncConfig::default(),
            Tuning {
                windows: Some((heartbeat, silence, slow_consumer)),
                connection_cap: None,
            },
        )
        .await
    }

    /// 默认世界 + 显式调小的生产常量（会话窗口与并发连接上限）。
    async fn with_tuning(config: SyncConfig, tuning: Tuning) -> Self {
        Self::start(
            TestWorld::with_store(Arc::new(FixedStore::new(
                TEST_SERVER_EPOCH,
                STORE_HEAD_SEQUENCE,
            ))),
            config,
            tuning,
        )
        .await
    }

    /// 结束接入层（用例末尾调用；端口随 listener 释放）。
    async fn stop(self) {
        self.shutdown.trigger();
        let _ = tokio::time::timeout(IO_TIMEOUT, self.join).await;
    }

    /// 本机用户登记一次设备配对并走到 `approved`（§7：begin → claim → confirm）。
    async fn approved_device_pairing(&self, scopes: &[&str]) -> String {
        let begin = success(
            &self
                .world
                .router()
                .handle(admin_request(
                    Method::DevicePairBegin,
                    json!({
                        "requestedPacks": [],
                        "requestedScopes": scopes,
                        "expiresInMs": null,
                    }),
                ))
                .await,
        );
        let pairing = begin["pairingId"].as_str().expect("pairingId").to_owned();
        // 认领：绑定必须逐字回显登记值（§11.2 第 1 条）。
        let peer = PairingPeer::try_new(
            PeerIdentity::Device(device_id(DEVICE_ID)),
            "Test Phone",
            test_public_key(),
            DEVICE_ORIGIN,
            test_nonce(),
        )
        .expect("测试用 peer 合法");
        self.world
            .claim(&pairing_id(&pairing), peer)
            .await
            .expect("认领必须成功");
        let confirmed = success(
            &self
                .world
                .router()
                .handle(admin_request(
                    Method::DevicePairConfirm,
                    json!({"pairingId": pairing, "scopes": scopes}),
                ))
                .await,
        );
        assert_eq!(
            confirmed["scopes"],
            json!(scopes),
            "确认后的 scope 集合必须逐条回显"
        );
        // 批准把对端公钥转入 `owned_peer_key`（§11.6 第 4 条）：存储层对设备分支也这么做，
        // `FakeTrust::approve_device` 漏了这一步（见 [`SyncTrust::device_keys`]），这里补上等价事实。
        self.trust
            .bind_device_key(&device_id(DEVICE_ID), test_public_key());
        pairing
    }

    /// 本机用户撤销该设备（`device.revoke` 的前置）。
    async fn revoke_device(&self, device: &str) {
        let response = self
            .world
            .router()
            .handle(admin_request(
                Method::DeviceRevoke,
                json!({"deviceId": device}),
            ))
            .await;
        let _ = success(&response);
        assert_eq!(
            self.world
                .trust
                .device(device)
                .expect("设备行必须已落库")
                .state(),
            DeviceState::Revoked,
            "撤销必须先落库"
        );
    }

    /// 该设备的持久化行（用例断言用）。
    fn device_row(&self, device: &str) -> DeviceRecord {
        self.world.trust.device(device).expect("设备行必须已落库")
    }

    /// 审计行（按动作过滤）：`AuditStore` 替身的行 + 信任写集提交时随事务写入的行。
    async fn audit_rows(&self, actions: Vec<AuditAction>) -> Vec<acp_core::model::AuditRecord> {
        let mut rows = self
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

fn device_id(text: &str) -> DeviceId {
    DeviceId::new(text).expect("测试用 DeviceId 文本必须规范")
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

/// 16 字节 nonce 文本（`control.ping`/`control.pong`）。
fn ping_nonce(seed: u8) -> String {
    let bytes = [seed; 16];
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn uuid_text() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 用固定私钥（标量 1，公钥即 `test_public_key()` 的基点 G）签名一段 transcript。
///
/// 与 `FakeKeystore::sign` 同一素材：设备侧证明必须能由配对批准写入持久化的公钥验证。
fn sign_with_device_key(transcript: &[u8]) -> String {
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
    /// `auth.server_challenge` 下发的连接标识（认证后消息必须携带）。
    connection_id: Option<String>,
    /// 已读到但用例尚未取用的消息。
    pending: Vec<Value>,
    /// 已收到的服务端出站序号（认证后从 `auth.authenticated` 的 1 开始）。
    server_sequence: u64,
    /// 当前用例段落（失败信息用）。
    step: String,
    /// 读帧的超时（15 秒握手窗口的用例需要更长的预算）。
    read_timeout: Duration,
}

impl Client {
    /// 建立连接并完成 WebSocket 升级（带上协议约定的 subprotocol 与给定来源）。
    async fn connect(addr: SocketAddr, origin: Option<&str>) -> Self {
        Self::try_connect(addr, origin, WS_SUBPROTOCOL, IO_TIMEOUT)
            .await
            .expect("WebSocket 升级必须成功")
    }

    /// 尝试升级；升级被接入层拒绝时返回 `None`（用例据此断言「不建立连接」）。
    async fn try_connect(
        addr: SocketAddr,
        origin: Option<&str>,
        subprotocol: &str,
        read_timeout: Duration,
    ) -> Option<Self> {
        let mut stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
            .await
            .expect("连接超时")
            .expect("连接 loopback");
        let mut key_bytes = [0x11u8; 16];
        key_bytes[0] = 0x42;
        let key = base64::engine::general_purpose::STANDARD.encode(key_bytes);
        let origin_line = origin
            .map(|origin| format!("Origin: {origin}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "GET {WS_PATH} HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Protocol: {subprotocol}\r\n{origin_line}\r\n"
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
            if read == 0 {
                // 对端在响应未读全时关闭：接入层对未注册/不匹配的升级请求就是这样处置的。
                return None;
            }
            buffer.extend_from_slice(&chunk[..read]);
        };
        let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
        if !head.starts_with("HTTP/1.1 101 ") {
            return None;
        }
        assert!(
            head.to_ascii_lowercase()
                .contains(&format!("sec-websocket-protocol: {subprotocol}")),
            "升级响应必须回显协商出的 subprotocol，实际：{head}"
        );
        buffer.drain(..head_end + 4);
        Some(Self {
            stream,
            buffer,
            outbound_sequence: 0,
            connection_id: None,
            pending: Vec::new(),
            server_sequence: 0,
            step: String::new(),
            read_timeout,
        })
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

    /// 取下一条消息，并核对「认证前省略连接字段、认证后携带且严格加一」。
    async fn take_message(&mut self) -> Value {
        let message = self.next_message().await.expect("对端已关闭连接");
        match message.get("connectionId") {
            Some(_) => {
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
            None => assert!(
                message.get("connectionSequence").is_none(),
                "认证前消息必须省略连接字段：{message}"
            ),
        }
        message
    }

    /// 期待下一条消息的 `type` 是 `message_type`，并返回整条消息。
    async fn expect_type(&mut self, message_type: &str) -> Value {
        let message = self.take_message().await;
        assert_eq!(
            message["type"],
            json!(message_type),
            "步骤「{}」的消息类型不符：{message}",
            self.step
        );
        message
    }

    /// 期待一条 `error` 并返回其 body。
    async fn expect_error(&mut self, expected_code: &str) -> Value {
        let message = self.expect_type("error").await;
        assert_eq!(
            message["body"]["code"],
            json!(expected_code),
            "错误码不符：{message}"
        );
        message["body"].clone()
    }

    /// 读到 close 帧（中途的文本消息收进 `pending`），返回 close code。
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
    /// [`Client::advance_after_accepted`] 推进（信封/序号级拒绝不推进任何一侧的序号）。
    async fn send_post_auth(&mut self, message_type: &str, body: Value) {
        let connection_id = self
            .connection_id
            .clone()
            .expect("认证后消息必须已有 connectionId");
        let sequence = (self.outbound_sequence + 1).to_string();
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

    /// 发送一条完全由调用方给出信封字段的消息（边界用例）。
    async fn send_raw(&mut self, envelope: Value) {
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

    /// 用「下一条合法序号」发送一条 `control.ping`，并期待回填同一 nonce 的 `control.pong`。
    async fn ping_round_trip(&mut self, seed: u8) -> Value {
        let nonce = ping_nonce(seed);
        self.send_post_auth("control.ping", json!({"nonce": nonce}))
            .await;
        let pong = self.expect_type("control.pong").await;
        assert_eq!(pong["body"]["nonce"], json!(nonce), "pong 必须原样回填");
        self.advance_after_accepted();
        pong
    }

    /// 回填服务端 `control.ping` 的 nonce（§13）。
    async fn reply_pong(&mut self, nonce: &str) {
        self.send_post_auth("control.pong", json!({"nonce": nonce}))
            .await;
        self.advance_after_accepted();
    }
}

/// 在 buffer 里找子切片。
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ---------------------------------------------------------------------------------------------
// 握手辅助（设备侧）
// ---------------------------------------------------------------------------------------------

/// 发送 `auth.client_hello`（认证前的第一条消息）。
async fn send_hello(client: &mut Client, device: &str, supported: &[&str], required: &[&str]) {
    let text = json!({
        "protocolVersion": 1,
        "type": "auth.client_hello",
        "messageId": uuid_text(),
        "body": {
            "minProtocolVersion": 1,
            "maxProtocolVersion": 1,
            "hostId": OWNER_NODE,
            "deviceId": device,
            "clientKind": "pwa",
            "clientNonce": nonce_text(CLIENT_NONCE_SEED),
            "supportedFeatures": supported,
            "requiredFeatures": required,
        },
    })
    .to_string();
    client.send_text(&text).await;
}

/// 全部 mvp feature 加一条 Host 不实现的 ID（协商必须只取交集）。
fn declared_features() -> Vec<&'static str> {
    let mut features: Vec<&'static str> = SUPPORTED_FEATURES.to_vec();
    features.push("core.future-thing.v1");
    features
}

/// 发送 `auth.client_proof`：`corrupt` 可破坏签名以覆盖该拒绝路径。
///
/// 两个输入都只从 `auth.server_challenge` 取：`connectionId`/`serverNonce`/`selectedFeatures`
/// （设备侧拿不到别的来源）。
async fn send_proof(
    client: &mut Client,
    device: &str,
    origin: &str,
    challenge: &Value,
    corrupt: bool,
) {
    let body = challenge["body"].clone();
    let selected: Vec<String> = body["selectedFeatures"]
        .as_array()
        .expect("selectedFeatures")
        .iter()
        .map(|feature| feature.as_str().expect("feature id").to_owned())
        .collect();
    let input = SyncDeviceProof {
        host_id: node(OWNER_NODE),
        device_id: device_id(device),
        canonical_origin: CanonicalOrigin::parse(origin).expect("规范 canonical origin"),
        client_nonce: identity_auth::Nonce::new(&nonce_text(CLIENT_NONCE_SEED))
            .expect("规范 nonce"),
        server_nonce: identity_auth::Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
            .expect("规范 nonce"),
        connection_id: ChallengeId::parse(body["connectionId"].as_str().expect("connectionId"))
            .expect("规范 uuid"),
        negotiated_features: FeatureList::new(selected),
    };
    let transcript = input.transcript().expect("transcript 可装配");
    let mut signature = P1363Signature::try_from_base64url(&sign_with_device_key(&transcript))
        .expect("签名是 64 字节 P1363");
    if corrupt {
        let mut bytes = *signature.as_bytes();
        bytes[0] ^= 0xff;
        signature = P1363Signature::try_from_bytes(&bytes).expect("翻转一个字节仍是 64 字节");
    }
    let text = json!({
        "protocolVersion": 1,
        "type": "auth.client_proof",
        "messageId": uuid_text(),
        "body": {
            "connectionId": body["connectionId"],
            "deviceId": device,
            "deviceProof": signature.to_base64url(),
        },
    })
    .to_string();
    client.send_text(&text).await;
}

/// 完成一次成功认证（hello → challenge → proof → authenticated），返回 `auth.authenticated`。
async fn authenticated(
    client: &mut Client,
    device: &str,
    origin: &str,
    corrupt_proof: bool,
) -> Value {
    send_hello(client, device, &declared_features(), &[]).await;
    let challenge = client.expect_type("auth.server_challenge").await;
    client.connection_id = Some(
        challenge["body"]["connectionId"]
            .as_str()
            .expect("connectionId")
            .to_owned(),
    );
    send_proof(client, device, origin, &challenge, corrupt_proof).await;
    client.expect_type("auth.authenticated").await
}

/// 发送 hello 并取下 `auth.server_challenge`（客户端此时还不知道 `connectionId` 的用途：
/// 认证前消息必须省略连接字段）。
async fn prepare_challenge(client: &mut Client, device: &str) -> Value {
    send_hello(client, device, &declared_features(), &[]).await;
    client.expect_type("auth.server_challenge").await
}

async fn assert_eventually(mut condition: impl FnMut() -> bool, message: &str) {
    let deadline = Instant::now() + IO_TIMEOUT;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(condition(), "{message}");
}

// ---------------------------------------------------------------------------------------------
// 正常四消息握手
// ---------------------------------------------------------------------------------------------

/// 已配对设备完成整条四步握手后进入业务阶段。
///
/// 逐项断言：挑战信封省略连接字段、Host 挑战证明可用本节点公钥在 `sync-host-challenge/v1` 域验证、
/// `auth.authenticated` 携带连接字段且序号从 1 开始、feature 是交集（排序去重）、`limits` 取 §14 默认值、
/// 连接已登记且句柄事实与握手一致、连接结束后注册表清空。
#[tokio::test]
async fn handshake_completes_and_enters_the_business_phase() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;

    let challenge = prepare_challenge(&mut client, DEVICE_ID).await;
    assert!(
        challenge.get("connectionId").is_none() && challenge.get("connectionSequence").is_none(),
        "认证前消息必须省略连接字段：{challenge}"
    );
    let body = challenge["body"].clone();
    assert_eq!(body["hostId"], json!(OWNER_NODE));
    assert_eq!(body["selectedProtocolVersion"], json!(1), "v1 只有版本 1");
    let selected = feature_ids(&body);
    let mut expected: Vec<String> = SUPPORTED_FEATURES
        .iter()
        .map(|id| (*id).to_owned())
        .collect();
    expected.sort();
    assert_eq!(
        selected, expected,
        "selectedFeatures 必须是对端声明与 Host 支持集合的交集（升序）"
    );

    let connection_id = body["connectionId"]
        .as_str()
        .expect("connectionId")
        .to_owned();
    client.connection_id = Some(connection_id.clone());

    // 双向认证的 Host 侧一半：挑战证明可用本节点公钥在 `sync-host-challenge/v1` 域验证。
    let host_public_key = harness
        .world
        .authority
        .node_public_key()
        .await
        .expect("本节点公钥可用");
    SyncHostChallenge {
        host_id: node(OWNER_NODE),
        device_id: device_id(DEVICE_ID),
        canonical_origin: CanonicalOrigin::parse(DEVICE_ORIGIN).expect("规范 origin"),
        client_nonce: identity_auth::Nonce::new(&nonce_text(CLIENT_NONCE_SEED)).expect("nonce"),
        server_nonce: identity_auth::Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
            .expect("nonce"),
        connection_id: ChallengeId::parse(&connection_id).expect("规范 uuid"),
        negotiated_features: FeatureList::new(selected.clone()),
    }
    .verify(
        &host_public_key,
        &P1363Signature::try_from_base64url(body["hostProof"].as_str().expect("hostProof"))
            .expect("挑战证明是 64 字节 P1363"),
    )
    .expect("Host 挑战证明必须可用本节点公钥验证");

    send_proof(&mut client, DEVICE_ID, DEVICE_ORIGIN, &challenge, false).await;
    let authenticated = client.expect_type("auth.authenticated").await;
    assert_eq!(authenticated["connectionId"], json!(connection_id));
    assert_eq!(authenticated["connectionSequence"], json!("1"));
    assert_eq!(authenticated["body"]["deviceId"], json!(DEVICE_ID));
    assert_eq!(authenticated["body"]["scopes"], json!(["session.read"]));
    assert_eq!(
        authenticated["body"]["serverEpoch"],
        json!(TEST_SERVER_EPOCH)
    );
    assert_eq!(
        authenticated["body"]["headGlobalSequence"],
        json!(STORE_HEAD_SEQUENCE.to_string())
    );
    assert_eq!(
        authenticated["body"]["heartbeatIntervalMs"],
        json!(DEFAULT_HEARTBEAT_INTERVAL_MS)
    );

    // 业务阶段：连接已登记，句柄的事实与握手一致。
    let handles = harness.conn.registry().handles();
    assert_eq!(handles.len(), 1, "认证成功后必须登记连接");
    let handle = &handles[0];
    assert_eq!(handle.connection_id().as_str(), connection_id);
    assert_eq!(handle.device_id(), &device_id(DEVICE_ID));
    assert_eq!(handle.host_id(), &node(OWNER_NODE));
    assert_eq!(handle.client_ip().to_string(), "127.0.0.1");
    assert!(!handle.saturated(), "刚建连的队列不应处于高水位");
    client.step("握手后的心跳往返");
    let _ = client.ping_round_trip(0x41).await;

    // 连接结束后注册表清空（撤销传播与关闭路径依赖这一条）。
    client.send_close().await;
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "会话结束后必须从注册表摘除",
    )
    .await;
    harness.stop().await;
}

/// 首次认证的写集：配对推进到 `consumed`、`last_seen_at` 推进、`device.authenticated` 留痕。
#[tokio::test]
async fn the_first_authentication_consumes_the_pairing_and_advances_last_seen() {
    let harness = Harness::new().await;
    let pairing = harness.approved_device_pairing(&["session.read"]).await;
    assert!(
        harness.device_row(DEVICE_ID).last_seen_at().is_none(),
        "批准只创建信任行，不写 last_seen_at"
    );

    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;
    let first_auth_at = harness.world.clock_text();
    assert_eq!(
        harness
            .device_row(DEVICE_ID)
            .last_seen_at()
            .map(|at| at.as_str().to_owned()),
        Some(first_auth_at.clone()),
        "首次认证必须经 record_device_connected 推进 last_seen_at"
    );
    let consumed = harness.world.trust.pairing(&pairing).expect("配对仍在库里");
    assert_eq!(consumed.state(), PairingState::Consumed, "首次认证消费配对");
    assert!(consumed.terminal_at().is_some(), "consumed 必须带终态时间");

    let rows = harness
        .audit_rows(vec![AuditAction::DeviceAuthenticated])
        .await;
    assert_eq!(rows.len(), 2, "消费写集与收尾写集各留一行：{rows:?}");
    assert!(
        rows.iter().all(|row| row.outcome() == AuditOutcome::Success
            && row.actor().device_id() == Some(&device_id(DEVICE_ID))),
        "两条审计都必须归因到该设备：{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.target() == &EntityRef::Pairing(pairing_id(&pairing))),
        "首次认证的审计目标是那次配对：{rows:?}"
    );

    // 重复认证（无待消费配对）：时间继续推进，且设备仍然可用。
    const SECOND_AUTH_AT: &str = "2026-09-18T09:12:04.412Z";
    harness.world.clock.set(SECOND_AUTH_AT);
    client.send_close().await;
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "第一条连接结束后必须从注册表摘除",
    )
    .await;
    let mut second = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut second, DEVICE_ID, DEVICE_ORIGIN, false).await;
    assert_eq!(
        harness
            .device_row(DEVICE_ID)
            .last_seen_at()
            .map(|at| at.as_str().to_owned()),
        Some(SECOND_AUTH_AT.to_owned()),
        "重复认证必须继续推进 last_seen_at（该列的唯一生产者就是收尾写集）"
    );
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 认证窗口与阶段白名单
// ---------------------------------------------------------------------------------------------

/// 认证完成前的业务消息以 `4401` 关闭，且不产生任何业务副作用。
#[tokio::test]
async fn a_business_message_before_hello_is_closed_with_4401() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    client.step("认证前发 sync.subscribe");
    let text = json!({
        "protocolVersion": 1,
        "type": "sync.subscribe",
        "messageId": uuid_text(),
        "body": {},
    })
    .to_string();
    client.send_text(&text).await;
    let error = client.expect_error("auth.required").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(
        client.expect_close().await,
        4401,
        "认证完成前收到业务消息必须以 4401 关闭"
    );
    assert_eq!(
        harness.conn.registry().active(),
        0,
        "被拒绝的连接不得占用设备槽位"
    );
    harness.stop().await;
}

/// 第二条 `auth.client_hello` 以 `4401` 关闭（§14：认证前最多只接收 hello 与 proof 各一次）。
#[tokio::test]
async fn a_duplicate_hello_is_closed_with_4401() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = prepare_challenge(&mut client, DEVICE_ID).await;
    client.step("第二条 hello");
    send_hello(&mut client, DEVICE_ID, &declared_features(), &[]).await;
    let _ = client.expect_error("auth.required").await;
    assert_eq!(client.expect_close().await, 4401);
    harness.stop().await;
}

/// §14 的认证前固定上限：连接建立后第一条消息（必须是 `auth.client_hello`）必须在 **5 秒**内完整到达
/// ——超时以 `4408` 关闭，MUST NOT 一直等到 15 秒整窗结束。
#[tokio::test]
async fn the_first_message_window_is_five_seconds_not_the_whole_fifteen() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    // 计时起点在连接之前：服务端窗口从会话任务开始（WS 升级完成后）起算，客户端起点晚于它就会吃掉余量。
    let started = Instant::now();
    let mut client = Client::try_connect(
        harness.addr,
        Some(DEVICE_ORIGIN),
        WS_SUBPROTOCOL,
        TIMEOUT_WINDOW_IO,
    )
    .await
    .expect("升级必须成功");
    assert_eq!(client.expect_close().await, 4408, "首条消息超时必须 4408");
    assert!(
        started.elapsed() >= FIRST_MESSAGE_TIMEOUT,
        "关闭必须发生在 5 秒窗口之后"
    );
    assert!(
        started.elapsed() < HANDSHAKE_TIMEOUT,
        "首条消息超时不得等到 15 秒整窗（§14 的固定 5 秒）"
    );
    assert_eq!(
        harness.conn.registry().active(),
        0,
        "未完成认证的连接不占设备槽位"
    );
    harness.stop().await;
}

/// 首条消息窗口结束后，整条认证仍受 **15 秒**整窗兜住（§14 的两条时限并存、不互相替代）。
#[tokio::test]
async fn the_handshake_still_has_fifteen_seconds_after_the_hello() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    // 计时起点在连接之前（与 `the_first_message_window...` 同一理由：窗口从服务端会话任务开始起算）。
    let started = Instant::now();
    let mut client = Client::try_connect(
        harness.addr,
        Some(DEVICE_ORIGIN),
        WS_SUBPROTOCOL,
        TIMEOUT_WINDOW_IO,
    )
    .await
    .expect("升级必须成功");
    // hello 在 5 秒窗口内到达：挑战已签发，5 秒窗口到此结束。
    let _ = prepare_challenge(&mut client, DEVICE_ID).await;
    assert!(
        started.elapsed() < FIRST_MESSAGE_TIMEOUT,
        "用例素材：hello 必须落在 5 秒窗口内"
    );
    assert_eq!(client.expect_close().await, 4408, "认证超时必须 4408");
    assert!(
        started.elapsed() >= HANDSHAKE_TIMEOUT,
        "hello 之后剩余的时间必须由 15 秒整窗兜住，而不是 5 秒窗口"
    );
    assert_eq!(
        harness.conn.registry().active(),
        0,
        "不因超时而建立认证状态"
    );
    harness.stop().await;
}

/// §14 的认证前固定上限：首条 `auth.client_hello` 的消息体最多 **32 KiB**。超限被拒绝，且不进入握手状态机。
#[tokio::test]
async fn an_oversized_client_hello_is_rejected_before_the_handshake() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;

    // 判据量的是信封 `body` 的原始 UTF-8 字节数，因此 padding 放进 body。上限在**任何类型化解码之前**
    // 生效（§14「超限应在分配完整 body 前尽早拒绝」），所以 body 本身是否还能通过 schema 不影响这条判定。
    let body = json!({
        "minProtocolVersion": 1,
        "maxProtocolVersion": 1,
        "hostId": OWNER_NODE,
        "deviceId": DEVICE_ID,
        "clientKind": "pwa",
        "clientNonce": nonce_text(CLIENT_NONCE_SEED),
        "supportedFeatures": declared_features(),
        "requiredFeatures": [],
        "padding": "x".repeat(HELLO_MAX_BYTES as usize),
    });
    assert!(
        body.to_string().len() as u64 > HELLO_MAX_BYTES,
        "用例素材：body 必须越过 32 KiB 的固定上限"
    );
    client.step("超限的 client_hello");
    let text = json!({
        "protocolVersion": 1,
        "type": "auth.client_hello",
        "messageId": uuid_text(),
        "body": body,
    })
    .to_string();
    client.send_text(&text).await;

    let error = client.expect_error("protocol.message_too_large").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(
        error["details"],
        json!({}),
        "§12.2：未登记 details 的 code 必须送 {{}}"
    );
    assert_eq!(
        client.expect_close().await,
        4400,
        "超限的 hello 以 4400 关闭"
    );
    assert_eq!(
        harness.conn.registry().active(),
        0,
        "超限的 hello 不得建立任何认证状态"
    );
    assert_eventually(
        || harness.conn.in_flight_handshakes() == 0,
        "被拒绝的连接不得留下在途握手凭证",
    )
    .await;
    harness.stop().await;
}

/// §14 的认证前固定上限：单节点**并发 WSS 连接** 256。超限的新连接被显式拒绝——不排队、不静默丢弃，
/// 也不挤掉既有连接（这里把上限调成 1，让用例只起两条连接；256 由常量断言锁定）。
#[tokio::test]
async fn the_node_concurrent_connection_cap_refuses_the_extra_connection() {
    let harness = Harness::with_tuning(
        SyncConfig::default(),
        Tuning {
            windows: None,
            connection_cap: Some(1),
        },
    )
    .await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;

    // ① 第一条连接占住并发配额（它甚至还没发 hello：配额覆盖「已建立的连接」）。
    let mut first = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    assert_eventually(
        || harness.conn.concurrent_connections() == 1,
        "第一条连接必须占住并发配额",
    )
    .await;

    // ② 第二条连接的升级成功（接入层不管这条判定），但会话立即显式拒绝：错误可见、不排队、不静默丢弃。
    let mut second = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let error = second.expect_error("resource.rate_limited").await;
    assert_eq!(error["retryable"], json!(true));
    assert_eq!(
        second.expect_close().await,
        4429,
        "超限的连接按限流语义显式关闭"
    );

    // ③ 既有连接没有被挤掉：它仍能走完握手（挑战照常签发）。
    let _ = prepare_challenge(&mut first, DEVICE_ID).await;
    assert_eq!(
        harness.conn.concurrent_connections(),
        1,
        "被拒绝的新连接不得动既有连接的配额"
    );

    // ④ 会话结束即归还配额：新连接因此可以再进来。
    drop(first);
    assert_eventually(
        || harness.conn.concurrent_connections() == 0,
        "连接结束后并发配额必须归还",
    )
    .await;
    let mut third = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = prepare_challenge(&mut third, DEVICE_ID).await;
    harness.stop().await;
}

/// binary 帧在认证前与认证后都被拒绝并以 `4400` 关闭，内容不进协议逻辑。
#[tokio::test]
async fn a_binary_frame_is_closed_with_4400() {
    // ① 认证前。
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    client.step("认证前 binary 帧");
    client.send_binary(b"not-json").await;
    let _ = client.expect_error("protocol.invalid_json").await;
    assert_eq!(client.expect_close().await, 4400, "binary 帧必须 4400 关闭");
    harness.stop().await;

    // ② 认证后。
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;
    client.step("认证后 binary 帧");
    client.send_binary(&[0x00, 0x01]).await;
    assert_eq!(client.expect_close().await, 4400, "binary 帧必须 4400 关闭");
    assert_eq!(
        client
            .pending
            .first()
            .map(|message| &message["body"]["code"]),
        Some(&json!("protocol.invalid_json")),
        "关闭前必须尽力送出错误体：{:?}",
        client.pending
    );
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "binary 帧必须结束会话（内容不得进入协议逻辑）",
    )
    .await;
    harness.stop().await;
}

/// 未声明协议 subprotocol 的升级被拒绝，不建立连接、不产生会话状态。
#[tokio::test]
async fn the_endpoint_rejects_another_subprotocol() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let client = Client::try_connect(
        harness.addr,
        Some(DEVICE_ORIGIN),
        "other.protocol",
        IO_TIMEOUT,
    )
    .await;
    assert!(client.is_none(), "缺少约定 subprotocol 的升级必须被拒绝");
    assert_eq!(harness.conn.registry().active(), 0);
    harness.stop().await;
}

/// §14 的单 IP 在途握手配额：第 6 条未认证连接被 `4429` 拒绝，连接结束后配额释放。
#[tokio::test]
async fn the_sixth_in_flight_handshake_from_one_address_is_refused() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    // 五条只升级、不发任何帧的连接占满配额。
    let mut clients = Vec::new();
    for _ in 0..MAX_IN_FLIGHT_HANDSHAKES_PER_IP {
        clients.push(Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await);
    }
    assert_eventually(
        || harness.conn.in_flight_handshakes() == MAX_IN_FLIGHT_HANDSHAKES_PER_IP,
        "五条连接必须占满在途握手配额",
    )
    .await;

    // 第 6 条：任何帧都不再被处理（包括 hello），直接以 4429 关闭。
    let mut refused = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    send_hello(&mut refused, DEVICE_ID, &declared_features(), &[]).await;
    let error = refused.expect_error("resource.rate_limited").await;
    assert_eq!(error["retryable"], json!(true), "限流必须可重试");
    assert!(
        error["details"]["retryAfterMs"].is_number(),
        "必须给出退避建议：{error}"
    );
    assert_eq!(refused.expect_close().await, 4429);
    assert_eq!(
        harness.conn.registry().active(),
        0,
        "被限流的连接不占设备槽位"
    );

    // 连接结束后配额释放（凭证只覆盖握手阶段）。
    clients.pop();
    assert_eventually(
        || harness.conn.in_flight_handshakes() < MAX_IN_FLIGHT_HANDSHAKES_PER_IP,
        "未认证连接结束后必须释放配额",
    )
    .await;
    harness.stop().await;
}

/// §14 的单 IP **新认证尝试**上限：10 次/分钟。同源第 11 条连接在进入握手之前就被 `4429` 拒绝。
///
/// 每条连接都必须真的走到「认证尝试」这一步（只完成 WSS 升级不算），因此前 10 条都取回一次
/// `auth.server_challenge` 再断开。这样同一时刻至多一条在途握手，单 IP 在途握手配额（5 条）不会抢先
/// 拒绝——第 11 条命中的只能是 10 次/分钟的窗口，而不是另一种同样回 `4429` 的拒绝。
///
/// 循环次数取**协议口径的字面量**（10），不取被测常量 [`AUTH_ATTEMPTS_PER_MINUTE`]：否则把常量改坏时
/// 循环次数跟着变，第「11」条永远是被拒的那一条，用例对该常量失明（常量本身由
/// [`protocol_constants_are_not_configurable`] 锁定）。
#[tokio::test]
async fn the_eleventh_authentication_attempt_from_one_address_is_rate_limited() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    for _ in 0..10 {
        let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
        let _ = prepare_challenge(&mut client, DEVICE_ID).await;
        drop(client);
        assert_eventually(
            || harness.conn.in_flight_handshakes() == 0,
            "未认证连接结束后必须在途握手配额归零（否则下一条会撞上配额而不是本窗口）",
        )
        .await;
    }

    // 第 11 次尝试：在握手阶段之前被拒绝（连接不建立，也不占在途握手配额）。
    let mut refused = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    send_hello(&mut refused, DEVICE_ID, &declared_features(), &[]).await;
    let error = refused.expect_error("resource.rate_limited").await;
    assert_eq!(error["retryable"], json!(true), "限流必须可重试");
    // 退避建议必须来自 60 秒的认证窗口，而不是 15 秒的握手窗口——两种拒绝（本窗口 / 在途握手配额）
    // 回同一个错误码与 close code，只有退避时长能把「第 11 条确实撞上 10 次/分钟」与「撞上配额」分开。
    let retry_after_ms = error["details"]["retryAfterMs"]
        .as_u64()
        .expect("retryAfterMs 必须是数字");
    assert!(
        retry_after_ms > HANDSHAKE_TIMEOUT.as_millis() as u64,
        "退避建议必须来自 60 秒的认证窗口：{error}"
    );
    assert_eq!(refused.expect_close().await, 4429);
    assert_eq!(
        harness.conn.registry().active(),
        0,
        "被限流的连接不占设备槽位"
    );
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 来源绑定
// ---------------------------------------------------------------------------------------------

/// 请求头 `Origin` 与设备绑定的 canonical origin 不一致 → `auth.origin_mismatch`。
#[tokio::test]
async fn an_origin_that_differs_from_the_binding_is_refused() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some("https://other.example.test")).await;
    client.step("来源与绑定不一致");
    send_hello(&mut client, DEVICE_ID, &declared_features(), &[]).await;
    let error = client.expect_error("auth.origin_mismatch").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4401);
    assert_eq!(harness.conn.registry().active(), 0, "不建立已认证连接");
    harness.stop().await;
}

/// 没有 `Origin` 头的连接被拒绝（§8.5 的已知偏差：v1 的设备信任绑定在 canonical origin 上）。
#[tokio::test]
async fn a_missing_origin_is_refused() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, None).await;
    client.step("无来源");
    send_hello(&mut client, DEVICE_ID, &declared_features(), &[]).await;
    let _ = client.expect_error("auth.origin_mismatch").await;
    assert_eq!(client.expect_close().await, 4401);
    harness.stop().await;
}

/// 非 `https://` 的来源（dev 形态的如实取值）由本层拒绝，不以空字节串或 `http` 绑定放行。
#[tokio::test]
async fn a_non_https_origin_is_refused() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some("http://127.0.0.1:8765")).await;
    client.step("http 来源");
    send_hello(&mut client, DEVICE_ID, &declared_features(), &[]).await;
    let _ = client.expect_error("auth.origin_mismatch").await;
    assert_eq!(client.expect_close().await, 4401);
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 未知与已撤销设备
// ---------------------------------------------------------------------------------------------

/// 未知设备照常拿到结构完整的挑战，失败推迟到 proof 阶段并统一为 `auth.proof_invalid`。
#[tokio::test]
async fn an_unknown_device_gets_a_challenge_and_fails_as_proof_invalid() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    client.step("未知设备握手");
    let challenge = prepare_challenge(&mut client, UNKNOWN_DEVICE).await;
    // 挑战必须结构完整（不得用错误类别区分设备是否存在）。
    let body = challenge["body"].clone();
    assert_eq!(body["hostId"], json!(OWNER_NODE));
    assert_eq!(body["selectedProtocolVersion"], json!(1));
    assert!(
        body["connectionId"].is_string()
            && body["serverNonce"].is_string()
            && body["hostProof"].is_string(),
        "未知设备也必须拿到结构完整的挑战：{challenge}"
    );
    client.connection_id = Some(
        body["connectionId"]
            .as_str()
            .expect("connectionId")
            .to_owned(),
    );
    send_proof(
        &mut client,
        UNKNOWN_DEVICE,
        DEVICE_ORIGIN,
        &challenge,
        false,
    )
    .await;
    let error = client.expect_error("auth.proof_invalid").await;
    assert_eq!(
        error["message"],
        json!("the proof is not valid"),
        "未知设备与验签失败必须不可区分"
    );
    assert_eq!(client.expect_close().await, 4401);
    assert_eq!(harness.conn.registry().active(), 0);
    harness.stop().await;
}

/// 已撤销设备提交有效签名的完整握手 → `auth.device_revoked` + `4410`，且不把状态改回可用。
#[tokio::test]
async fn a_revoked_device_is_closed_with_4410() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    harness.revoke_device(DEVICE_ID).await;

    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    client.step("已撤销设备握手");
    let challenge = prepare_challenge(&mut client, DEVICE_ID).await;
    client.connection_id = Some(
        challenge["body"]["connectionId"]
            .as_str()
            .expect("connectionId")
            .to_owned(),
    );
    send_proof(&mut client, DEVICE_ID, DEVICE_ORIGIN, &challenge, false).await;
    let error = client.expect_error("auth.device_revoked").await;
    assert_eq!(error["retryable"], json!(false));
    assert_eq!(client.expect_close().await, 4410);
    assert_eq!(harness.conn.registry().active(), 0, "不建立已认证连接");
    assert_eq!(
        harness.device_row(DEVICE_ID).state(),
        DeviceState::Revoked,
        "失败的认证尝试不得把设备改回可用"
    );
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 单设备替换与撤销传播
// ---------------------------------------------------------------------------------------------

/// 新连接先独立完成认证，随后旧连接收到 `4411`；设备不被撤销、新连接保持可用。
#[tokio::test]
async fn a_new_connection_replaces_the_previous_one() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let before = harness.device_row(DEVICE_ID);

    let mut first = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut first, DEVICE_ID, DEVICE_ORIGIN, false).await;
    assert_eq!(harness.conn.registry().active(), 1);

    // 第一条连接必须自己完成完整认证之后才轮到它被顶替。
    let mut second = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut second, DEVICE_ID, DEVICE_ORIGIN, false).await;
    assert_eventually(
        || harness.conn.registry().active() == 1,
        "同一设备最多保留一条已认证连接",
    )
    .await;
    assert_eq!(first.expect_close().await, 4411, "旧连接必须以 4411 被关闭");

    // 新连接保持已认证，设备的信任与授权不变。
    second.step("顶替后仍可用");
    let _ = second.ping_round_trip(0x51).await;
    assert_eq!(
        harness.conn.registry().handles()[0]
            .connection_id()
            .as_str(),
        second.connection_id.as_deref().expect("connectionId"),
        "留下的必须是新连接"
    );
    let after = harness.device_row(DEVICE_ID);
    assert_eq!(after.state(), DeviceState::Active, "4411 不撤销设备");
    assert_eq!(
        after.scopes().iter().collect::<Vec<_>>(),
        vec!["session.read"]
    );
    assert_eq!(
        after.created_at().as_str(),
        before.created_at().as_str(),
        "顶替不改写信任行"
    );
    assert!(
        harness
            .audit_rows(vec![AuditAction::DeviceRevoked])
            .await
            .is_empty(),
        "顶替不得在审计中记为撤销"
    );

    // 旧连接的退出不得摘掉新连接的登记（注册表的键相同，必须按 connectionId 判定）。
    assert_eq!(
        harness.conn.registry().active(),
        1,
        "旧连接的退出不得摘掉新连接"
    );
    assert_eq!(
        harness.conn.registry().handles()[0]
            .connection_id()
            .as_str(),
        second.connection_id.as_deref().expect("connectionId")
    );
    harness.stop().await;
}

/// 撤销传播：按 deviceId 扫连接并以 `4410` 请求关闭（组合根在持久化撤销提交之后调用）。
#[tokio::test]
async fn revocation_requests_the_close_of_the_active_connection() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;

    // 撤销以持久记录为准：先提交持久状态，再扫连接表。
    harness.revoke_device(DEVICE_ID).await;
    let closed = harness.conn.registry().close_device(
        &device_id(DEVICE_ID),
        close::REVOKED,
        "the device was revoked",
    );
    assert_eq!(closed, 1, "该设备的活动连接必须被扫到");
    assert_eq!(client.expect_close().await, 4410, "撤销必须以 4410 关闭");
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "撤销后连接必须从注册表摘除",
    )
    .await;

    // 后续认证被拒（§8.5：撤销后不再接受新连接）。
    let mut again = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let challenge = prepare_challenge(&mut again, DEVICE_ID).await;
    again.connection_id = Some(
        challenge["body"]["connectionId"]
            .as_str()
            .expect("connectionId")
            .to_owned(),
    );
    send_proof(&mut again, DEVICE_ID, DEVICE_ORIGIN, &challenge, false).await;
    let _ = again.expect_error("auth.device_revoked").await;
    assert_eq!(again.expect_close().await, 4410);
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 心跳与静默
// ---------------------------------------------------------------------------------------------

/// 心跳往返：服务端按窗口节奏发 `control.ping`，客户端回填同值 nonce 后连接保持；nonce 绝不复用。
#[tokio::test]
async fn heartbeat_round_trip_keeps_the_connection_alive() {
    let harness = Harness::with_windows(
        Duration::from_millis(300),
        Duration::from_secs(5),
        Duration::from_secs(30),
    )
    .await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let auth_message = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;
    assert_eq!(
        auth_message["body"]["heartbeatIntervalMs"],
        json!(DEFAULT_HEARTBEAT_INTERVAL_MS),
        "下发的仍然是 §13 的固定常量（用例只把实际节奏调短）"
    );

    client.step("第一条 control.ping");
    let first = client.expect_type("control.ping").await;
    let first_nonce = first["body"]["nonce"].as_str().expect("nonce").to_owned();
    client.reply_pong(&first_nonce).await;

    client.step("第二条 control.ping");
    let second = client.expect_type("control.ping").await;
    let second_nonce = second["body"]["nonce"].as_str().expect("nonce").to_owned();
    assert_ne!(first_nonce, second_nonce, "ping nonce 绝不复用");
    client.reply_pong(&second_nonce).await;

    // 正常往来仍在：客户端主动 ping，服务端回填同值 nonce。
    client.step("客户端主动 ping");
    let _ = client.ping_round_trip(0x61).await;
    assert_eq!(harness.conn.registry().active(), 1, "连接必须保持");
    harness.stop().await;
}

/// 静默超过窗口 → `4408`；持久化状态不被改写。
#[tokio::test]
async fn silence_beyond_the_window_is_closed_with_4408() {
    let harness = Harness::with_windows(
        // 心跳节奏长于静默窗口，避免 ping 掩盖静默判定（静默窗口独立计时）。
        Duration::from_secs(30),
        Duration::from_secs(1),
        Duration::from_secs(30),
    )
    .await;
    let pairing = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;
    let started = Instant::now();
    assert_eq!(
        client.expect_close().await,
        4408,
        "静默超过窗口以 4408 关闭"
    );
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "关闭必须发生在窗口之后"
    );
    assert_eq!(
        harness.world.trust.pairing(&pairing).map(|r| r.state()),
        Some(PairingState::Consumed),
        "静默关闭只影响连接，不改写已落库的持久化状态"
    );
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "会话结束后必须从注册表摘除",
    )
    .await;
    harness.stop().await;
}

/// §14 的二级背压：**持续**处于高水位的连接在宽限之后收到 `resource.backpressure` 并以 `1000` 断开。
///
/// 用例扮演 WP4 的角色：一直把这条连接的出站队列按在低水位之上（这就是「持续过慢」的定义），客户端照常
/// 读帧——因此判定只由真实队列账本驱动，与接入层的缓冲无关。
#[tokio::test]
async fn a_connection_that_stays_at_high_water_is_closed_after_the_grace() {
    let grace = Duration::from_millis(300);
    let harness = Harness::with_tuning(
        SyncConfig::default(),
        Tuning {
            windows: Some((Duration::from_millis(100), Duration::from_secs(10), grace)),
            connection_cap: None,
        },
    )
    .await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;

    let handle = harness
        .conn
        .registry()
        .handles()
        .pop()
        .expect("已认证连接必须已登记");
    let filler = tokio::spawn({
        let handle = Arc::clone(&handle);
        async move {
            let body = json!({"nonce": ping_nonce(0x56)});
            // ① 先顶到高水位：第一次被拒即开始饱和计时（「持续过慢」的起点）。
            while handle.send(MessageType::ControlPing, &body).is_ok() {}
            // ② 之后一直把占用保持在低水位之上（一个停不下来的生产者），每轮都让出一次执行权。
            loop {
                while handle.pending().messages < RECOVERY_MESSAGES + 200 {
                    if handle.send(MessageType::ControlPing, &body).is_err() {
                        break;
                    }
                }
                tokio::task::yield_now().await;
            }
        }
    });

    // 二级处置：`resource.backpressure` 先到，随后是 `1000`——中途那些已入队的消息先被读出（它们是
    // 用例自己投递的那一批，不是协议判定的一部分）。
    let code = client.expect_close().await;
    assert_eq!(code, 1000, "持续过慢必须在宽限之后断开");
    assert!(
        client.pending.iter().any(|message| {
            message["type"] == json!("error")
                && message["body"]["code"] == json!("resource.backpressure")
        }),
        "二级处置必须先发 resource.backpressure（§14），实际收到：{:?}",
        client.pending.last()
    );
    // 用例结束：停掉这个持续投递的任务。**不**靠它自己观察 `Closed` 退出——连接结束后占用会停在
    // 上限上，而 [`ConnectionHandle::send`] 的容量判定先于入队，因此它此后一直看到 `HighWater`（这不
    // 影响本用例的判定：慢消费者断开由会话看门狗做出，与投递方怎么收尾无关）。
    filler.abort();
    let _ = filler.await;
    assert_eventually(
        || harness.conn.registry().active() == 0,
        "断开后必须从注册表摘除",
    )
    .await;
    harness.stop().await;
}

/// §14 的一级背压与二级断开的分界：一次**瞬时突发**把队列顶到高水位、随后被读完（占用回到低水位之下）
/// 的连接不触发慢消费者断开。
#[tokio::test]
async fn a_transient_burst_that_drains_does_not_trip_the_slow_consumer_watchdog() {
    let grace = Duration::from_millis(300);
    let harness = Harness::with_tuning(
        SyncConfig::default(),
        Tuning {
            windows: Some((Duration::from_millis(200), Duration::from_secs(10), grace)),
            connection_cap: None,
        },
    )
    .await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;

    // ① 突发：一次性投递到高水位（模拟一整批快照 chunk 一口气生成）。
    let handle = harness
        .conn
        .registry()
        .handles()
        .pop()
        .expect("已认证连接必须已登记");
    let body = json!({"nonce": ping_nonce(0x57)});
    let mut queued = 0u64;
    while handle.send(MessageType::ControlPing, &body).is_ok() {
        queued += 1;
    }
    assert_eq!(queued, MAX_PENDING_QUEUE_MESSAGES, "§14 的条数上限先到");
    assert!(handle.saturated_for().is_some(), "突发即开始计时");

    // ② 回落：客户端把出站消息读完，占用回到低水位之下 → 计时清零。
    let draining = Instant::now() + IO_TIMEOUT;
    while handle.saturated_for().is_some() && Instant::now() < draining {
        let _ = client.take_message().await;
    }
    assert!(
        handle.saturated_for().is_none(),
        "读完突发后排空状态必须解除"
    );

    // ③ 宽限之后仍然活着（期间按 §13 回应服务端心跳，否则静默判定会先生效）。
    let watch = Instant::now() + grace * 4;
    while Instant::now() < watch {
        let Ok(message) =
            tokio::time::timeout(Duration::from_millis(100), client.take_message()).await
        else {
            continue;
        };
        if message["type"] == json!("control.ping") {
            let nonce = message["body"]["nonce"]
                .as_str()
                .expect("ping 必须带 nonce")
                .to_owned();
            client.reply_pong(&nonce).await;
        }
    }
    assert_eq!(
        harness.conn.registry().active(),
        1,
        "瞬时突发不得触发慢消费者断开"
    );
    let _ = client.ping_round_trip(0x58).await;
    assert_eq!(harness.conn.registry().active(), 1, "连接必须保持可用");
    harness.stop().await;
}

/// 协议常量不可配置：三个固定窗口与两个队列上限、以及 §14 认证前的固定上限就是协议值。
#[test]
fn protocol_constants_are_not_configurable() {
    assert_eq!(HEARTBEAT_SILENCE_TIMEOUT, Duration::from_secs(90));
    assert_eq!(DEFAULT_HEARTBEAT_INTERVAL_MS, 30_000);
    assert_eq!(MAX_PENDING_QUEUE_BYTES, 8_388_608);
    assert_eq!(MAX_PENDING_QUEUE_MESSAGES, 2_000);
    assert_eq!(DEFAULT_MAX_MESSAGE_BYTES, 1_048_576);
    assert_eq!(DEFAULT_MAX_PROMPT_BYTES, 262_144);
    assert_eq!(DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH, 500);
    // §14 的「认证前固定上限」表：首条消息 5 秒、首条 hello 32 KiB、单节点并发 256、单 IP 在途 5、
    // 单 IP 新认证尝试 10 次/分钟（窗口 60 秒）。
    assert_eq!(HANDSHAKE_TIMEOUT, Duration::from_secs(15));
    assert_eq!(FIRST_MESSAGE_TIMEOUT, Duration::from_secs(5));
    assert_eq!(HELLO_MAX_BYTES, 32 * 1024);
    assert_eq!(MAX_CONCURRENT_CONNECTIONS, 256);
    assert_eq!(MAX_IN_FLIGHT_HANDSHAKES_PER_IP, 5);
    assert_eq!(AUTH_ATTEMPTS_PER_MINUTE, 10);
    assert_eq!(AUTH_WINDOW, Duration::from_secs(60));
}

// ---------------------------------------------------------------------------------------------
// 协商与信封判定
// ---------------------------------------------------------------------------------------------

/// v1 的版本交集为空 → `protocol.version_unsupported` + `4406`。
#[tokio::test]
async fn a_hello_without_a_common_version_is_closed_with_4406() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    client.step("无公共版本");
    let text = json!({
        "protocolVersion": 1,
        "type": "auth.client_hello",
        "messageId": uuid_text(),
        "body": {
            "minProtocolVersion": 2,
            "maxProtocolVersion": 3,
            "hostId": OWNER_NODE,
            "deviceId": DEVICE_ID,
            "clientKind": "pwa",
            "clientNonce": nonce_text(CLIENT_NONCE_SEED),
            "supportedFeatures": declared_features(),
            "requiredFeatures": [],
        },
    })
    .to_string();
    client.send_text(&text).await;
    let _ = client.expect_error("protocol.version_unsupported").await;
    assert_eq!(client.expect_close().await, 4406);
    harness.stop().await;
}

/// 信封 `protocolVersion != 1` → `protocol.version_unsupported` + `4406`（§4.1 的 `const 1`）。
#[tokio::test]
async fn an_envelope_version_other_than_v1_is_closed_with_4406() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    client.step("信封版本 2");
    let text = json!({
        "protocolVersion": 2,
        "type": "auth.client_hello",
        "messageId": uuid_text(),
        "body": {},
    })
    .to_string();
    client.send_text(&text).await;
    let _ = client.expect_error("protocol.version_unsupported").await;
    assert_eq!(client.expect_close().await, 4406);
    harness.stop().await;
}

/// 客户端必需 feature 未被选中 → `protocol.feature_required` + `details.features` + `4406`。
#[tokio::test]
async fn a_missing_required_feature_is_reported_with_details_and_closed() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    client.step("必需 feature 未满足");
    send_hello(
        &mut client,
        DEVICE_ID,
        &["core.snapshot.v1"],
        &["core.session-create.v1", "core.event-ack.v1"],
    )
    .await;
    let error = client.expect_error("protocol.feature_required").await;
    assert_eq!(
        error["details"]["features"],
        json!(["core.event-ack.v1", "core.session-create.v1"]),
        "未满足项必须按 UTF-8 字节升序、去重"
    );
    assert_eq!(client.expect_close().await, 4406);
    harness.stop().await;
}

/// `selectedFeatures` 是对端声明与服务端支持集合的交集（排序去重）。
#[tokio::test]
async fn selected_features_are_the_sorted_intersection() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    send_hello(
        &mut client,
        DEVICE_ID,
        &[
            "core.snapshot.v1",
            "core.event-ack.v1",
            "core.future-thing.v1",
        ],
        &[],
    )
    .await;
    let challenge = client.expect_type("auth.server_challenge").await;
    assert_eq!(
        feature_ids(&challenge["body"]),
        vec![
            "core.event-ack.v1".to_owned(),
            "core.snapshot.v1".to_owned()
        ],
        "交集必须排序去重，且不含服务端不实现的 ID"
    );
    harness.stop().await;
}

/// `auth.authenticated.limits` 只下调：配置值高于 §14 默认值时取默认值。
#[tokio::test]
async fn authenticated_echoes_the_negotiated_limits_and_never_raises_them() {
    // ① 上调被忽略。
    let harness = Harness::with_config(SyncConfig {
        max_message_bytes: u64::MAX,
        max_prompt_bytes: u64::MAX,
        max_replay_events_per_batch: u64::MAX,
    })
    .await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let auth_message = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;
    assert_eq!(
        auth_message["body"]["limits"],
        json!({
            "maxMessageBytes": DEFAULT_MAX_MESSAGE_BYTES,
            "maxPromptBytes": DEFAULT_MAX_PROMPT_BYTES,
            "maxReplayEventsPerBatch": DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH,
        }),
        "高于默认值的配置必须被忽略"
    );
    harness.stop().await;

    // ② 下调生效。
    let harness = Harness::with_config(SyncConfig {
        max_message_bytes: 4_096,
        max_prompt_bytes: 1_024,
        max_replay_events_per_batch: 10,
    })
    .await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let auth_message = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;
    assert_eq!(
        auth_message["body"]["limits"],
        json!({
            "maxMessageBytes": 4096,
            "maxPromptBytes": 1024,
            "maxReplayEventsPerBatch": 10,
        }),
        "下调后的取值必须下发"
    );
    harness.stop().await;
}

/// 信封/序号/type 违规都是**消息级**拒绝：连接保持可用，且序号记账与 type 无关。
#[tokio::test]
async fn envelope_sequence_and_type_violations_are_rejected_without_closing() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;

    // ① 序号跳号：不推进任何一侧的序号。
    client.step("序号跳号");
    let mut skipped = client.envelope("control.ping", json!({"nonce": ping_nonce(0x71)}));
    skipped["connectionSequence"] = json!("5");
    client.send_raw(skipped).await;
    let _ = client.expect_error("protocol.sequence_invalid").await;

    // ② connectionId 不匹配：同样不推进。
    client.step("connectionId 不匹配");
    let mut other_id = client.envelope("control.ping", json!({"nonce": ping_nonce(0x72)}));
    other_id["connectionId"] = json!(uuid_text());
    client.send_raw(other_id).await;
    let _ = client.expect_error("protocol.sequence_invalid").await;

    // ③ 信封合法、序号正好是下一个、但 type 无实现：显式拒绝（绝不静默丢弃），序号照常消耗。
    client.step("未实现的业务 type");
    let message_id = uuid_text();
    let mut business = client.envelope("sync.subscribe", json!({}));
    business["messageId"] = json!(message_id);
    client.send_raw(business).await;
    let error = client.expect_error("protocol.type_unsupported").await;
    assert_eq!(
        error["correlationId"],
        json!(message_id),
        "有可关联的消息时必须回填 correlationId"
    );
    client.advance_after_accepted();

    // ④ 认证前才合法的 type 在认证后出现：仍是消息级拒绝（信封形状不对），但这一帧的连接字段可读、序号
    //    正好是下一个，因此序号照常推进（§4.1：序号描述对端已发出的消息，本机接不接受它的 type 不影响
    //    这一事实；不推进会留下「此后每条入站消息都被判序号错误」的哑管道）。
    client.step("认证前 type 出现在认证后");
    let late_hello = client.envelope("auth.client_hello", json!({}));
    client.send_raw(late_hello).await;
    let _ = client.expect_error("protocol.schema_invalid").await;
    client.advance_after_accepted();

    // 连接仍然可用（每个拒绝之后都跑一次心跳往返）。
    client.step("违规后仍可用");
    let _ = client.ping_round_trip(0x73).await;
    assert_eq!(harness.conn.registry().active(), 1);
    harness.stop().await;
}

/// §4.2/§4.1：词表**之外**的 `type` 回 `protocol.type_unsupported`（不是 `schema_invalid`），
/// 且这一次拒绝照常推进入站期望序号——不推进会让对端此后每条消息都被判序号错误，连接退化成只出不进。
///
/// 判定口径：`Envelope::decode` 走 serde，`type` 的词表外取值被折进 `Malformed`，因此这条区分只在失败
/// 路径上重读帧才能得出（`wire::malformed_inbound`）。序号推进的必要性由 ② 证明：它在只回退「序号推进」
/// 这一半修法时必然变红。
#[tokio::test]
async fn an_unknown_message_type_is_rejected_as_type_unsupported_and_keeps_the_sequence() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;

    // ① 词表外的 type（信封其余字段都合法、序号正好是下一个）：错误码必须是 type_unsupported。
    client.step("词表外的 type");
    let mut unknown = client.envelope("sync.does_not_exist", json!({}));
    unknown["messageId"] = json!(uuid_text());
    client.send_raw(unknown).await;
    let error = client.expect_error("protocol.type_unsupported").await;
    assert_eq!(error["retryable"], json!(false));

    // ② 紧接着一条类型合法、序号按客户端计数器正常递增（2）的消息：不得被判序号错误。
    //    「未知 type 也消耗了客户端发出的序号 1」是 §4.1 的口径（序号描述对端已发出的消息）。
    client.step("未知 type 之后序号正常递增的合法消息");
    client.advance_after_accepted();
    let _ = client.ping_round_trip(0x74).await;
    assert_eq!(harness.conn.registry().active(), 1, "连接必须保持可用");
    harness.stop().await;
}

/// §4.1/§4.2：**已知 `type` 但信封另有形状错误**（这里是多一个未登记的信封字段）同样是消息级拒绝，
/// 那一次拒绝也照常推进入站期望序号——不推进会让对端此后每条消息都被判序号错误，连接退化成只出不进。
///
/// 与词表外 `type` 的用例不同，这条帧的 `type` 在词表里：`Envelope::decode` 因 `deny_unknown_fields`
/// 失败，落在 `schema_invalid` 的兜底分支（`wire::malformed_inbound`）上，因此序号记账必须在那一支也
/// 成立。序号推进的必要性由 ② 证明：只回退「兜底分支的序号推进」这一半修法时它必然变红。
#[tokio::test]
async fn a_schema_invalid_envelope_that_consumed_the_sequence_does_not_strand_the_connection() {
    let harness = Harness::new().await;
    let _ = harness.approved_device_pairing(&["session.read"]).await;
    let mut client = Client::connect(harness.addr, Some(DEVICE_ORIGIN)).await;
    let _ = authenticated(&mut client, DEVICE_ID, DEVICE_ORIGIN, false).await;

    // ① 类型合法、连接字段与序号都正好是下一个，但信封多了一个未登记字段：schema 错误，消息不生效。
    client.step("信封多一个未登记字段");
    let mut extra = client.envelope("control.ping", json!({"nonce": ping_nonce(0x75)}));
    extra["unregisteredEnvelopeField"] = json!(true);
    client.send_raw(extra).await;
    let error = client.expect_error("protocol.schema_invalid").await;
    assert_eq!(error["retryable"], json!(false));

    // ② 紧接着一条类型合法、序号按客户端计数器正常递增（2）的消息：不得被判序号错误。
    client.step("信封形状错误之后序号正常递增的合法消息");
    client.advance_after_accepted();
    let _ = client.ping_round_trip(0x76).await;
    assert_eq!(harness.conn.registry().active(), 1, "连接必须保持可用");
    harness.stop().await;
}

// ---------------------------------------------------------------------------------------------
// 出站队列：双上限与序号账本
// ---------------------------------------------------------------------------------------------

/// 一条已入队消息信封里的 `connectionSequence`。
fn outbound_sequence(outbound: &Outbound) -> String {
    let message: Value = serde_json::from_str(&outbound.text).expect("出站消息是 JSON");
    message["connectionSequence"]
        .as_str()
        .expect("认证后消息必须带序号")
        .to_owned()
}

/// 一条用于队列用例的连接句柄（默认限额，出站队列即 §14 的固定上限）。
fn handle_for_queue_tests() -> (Arc<ConnectionHandle>, tokio::sync::mpsc::Receiver<Outbound>) {
    ConnectionHandle::new(
        sync_protocol::common::Uuid::parse(&uuid_text()).expect("连接标识是规范 uuid"),
        node(OWNER_NODE),
        device_id(DEVICE_ID),
        "127.0.0.1".parse().expect("loopback 地址"),
        SessionLimits::negotiate(&SyncConfig::default()),
        Arc::new(FakeIds::default()),
    )
}

/// 队列达到**条数**上限即高水位；被拒绝的投递不消耗序号，排空后状态自动清除。
#[test]
fn the_message_high_water_is_reported_without_consuming_a_sequence() {
    let (handle, mut receiver) = handle_for_queue_tests();
    assert!(!handle.saturated());
    assert!(handle.saturated_for().is_none());

    let body = json!({"nonce": ping_nonce(0x81)});
    let mut accepted = 0u64;
    loop {
        match handle.send(MessageType::ControlPing, &body) {
            Ok(()) => accepted += 1,
            Err(SendFault::HighWater) => break,
            Err(fault) => panic!("未预期的投递失败：{fault:?}"),
        }
        assert!(
            accepted <= MAX_PENDING_QUEUE_MESSAGES,
            "条数上限必须在 {MAX_PENDING_QUEUE_MESSAGES} 处触发"
        );
    }
    assert_eq!(accepted, MAX_PENDING_QUEUE_MESSAGES, "条数上限先到");
    assert!(handle.saturated(), "高水位必须对投递方可见（停读信号）");
    assert!(handle.saturated_for().is_some());
    assert_eq!(handle.pending().messages, MAX_PENDING_QUEUE_MESSAGES);

    // 排空：已入队的最大序号必须等于接受条数（被拒的投递没有烧掉序号）。
    let mut last = String::new();
    while let Ok(outbound) = receiver.try_recv() {
        last = outbound_sequence(&outbound);
        handle.consumed(&outbound);
    }
    assert_eq!(last, accepted.to_string(), "被拒的投递不得消耗序号");
    assert!(!handle.saturated(), "排空到上限之下后状态自动清除");
    assert!(handle.saturated_for().is_none());

    // 排空后可以继续投递，序号与前面连续。
    handle
        .send(MessageType::ControlPing, &body)
        .expect("排空后必须能继续投递");
    let next = receiver.try_recv().expect("新一条已入队");
    assert_eq!(
        outbound_sequence(&next),
        (accepted + 1).to_string(),
        "序号必须严格加一"
    );
}

/// 队列达到**字节**上限即高水位（先到者触发，与条数无关）。
#[test]
fn the_byte_budget_triggers_before_the_message_count() {
    let (handle, receiver) = handle_for_queue_tests();
    // 单条 ~5 MiB：第一条入队，第二条越过 8 MiB 的字节预算。
    let padding = "x".repeat(5 * 1024 * 1024);
    let body = json!({"padding": padding});
    handle
        .send(MessageType::ControlPing, &body)
        .expect("第一条必须入队");
    assert_eq!(
        handle.send(MessageType::ControlPing, &body),
        Err(SendFault::HighWater),
        "字节预算必须先于条数触发"
    );
    assert_eq!(handle.pending().messages, 1, "条数远未到上限");
    assert!(
        handle.pending().bytes <= MAX_PENDING_QUEUE_BYTES,
        "已入队的字节数不得越过上限"
    );
    drop(receiver);
}

/// §14 的「**持续**过慢」口径：饱和计时只在占用回到**低水位线**（高水位的一半）之下才清零。
///
/// 迟滞（两个阈值）是这条判定的全部：只同高水位一个阈值会在两种错法之间二选一——「一抖动就清零」会让
/// 反复瞬时超过再回落的消费者永远攒不满宽限（永不断开，与「持续过慢则断开」相悖）；「清了才算」若没有
/// 低水位线，又会把「占用在高水位附近反复起伏」当健康。用例覆盖两侧：抖动不清零、真的排空才清零。
#[test]
fn the_saturation_clock_survives_jitter_and_clears_only_below_the_low_water_mark() {
    let (handle, mut receiver) = handle_for_queue_tests();
    let body = json!({"nonce": ping_nonce(0x85)});

    // ① 突发：一直投递到高水位（第一次被拒即开始计时）。
    let mut queued = 0u64;
    while handle.send(MessageType::ControlPing, &body).is_ok() {
        queued += 1;
    }
    assert_eq!(queued, MAX_PENDING_QUEUE_MESSAGES, "§14 的条数上限先到");
    let burst = handle.saturated_for().expect("第一次被拒即开始计时");

    // ② 抖动：排空到「低水位之上、高水位之下」——计时**不**清零。下界取**协议口径**（§14 高水位的一半）
    //    而不是被测常量 [`RECOVERY_MESSAGES`] 自身：否则把恢复阈值改成高水位（这正是本用例要证伪的那个
    //    回归）时，`pending == MAX_PENDING_QUEUE_MESSAGES` 不满足循环条件，循环体一次都不执行、本步只剩
    //    恒真断言，用例对「阈值常量」这类变异失明。用协议值当边界时，阈值被改坏会让排空真的掉进
    //    「已低于恢复线」，下面的断言因此变红。
    while handle.pending().messages > MAX_PENDING_QUEUE_MESSAGES / 2 {
        let outbound = receiver.try_recv().expect("队里必须有消息");
        handle.consumed(&outbound);
    }
    let jittered = handle
        .saturated_for()
        .expect("占用仍在低水位之上：抖动不得清零饱和计时");
    assert!(
        jittered >= burst,
        "计时量的是当前饱和段的持续时长，只增不减"
    );
    assert!(
        handle.saturated(),
        "占用仍在高水位之上时投递方仍应停读新的批次"
    );

    // ③ 确实脱离饱和：占用落到低水位线之下，计时清零、投递方解除停读。
    let outbound = receiver.try_recv().expect("队里必须有消息");
    handle.consumed(&outbound);
    assert!(
        handle.saturated_for().is_none(),
        "占用低于低水位线才是「确实脱离饱和」"
    );
    assert!(!handle.saturated(), "脱离饱和后停读信号必须解除");
}

/// §14 的两级背压：心跳类消息（`control.ping` 与回填的 `control.pong`）撞上队列高水位时**不得**当致命
/// 错误——一级是推迟/丢弃这一次（连接保持），二级（持续过慢）才由慢消费者看门狗断开。
#[test]
fn a_heartbeat_send_at_high_water_defers_instead_of_closing() {
    assert_eq!(heartbeat_outcome(Ok(())), HeartbeatOutcome::Queued);
    assert_eq!(
        heartbeat_outcome(Err(SendFault::HighWater)),
        HeartbeatOutcome::Deferred,
        "高水位是一级背压（停读新批次），不是致命错误"
    );
    assert_eq!(
        heartbeat_outcome(Err(SendFault::Closed)),
        HeartbeatOutcome::Ended,
        "连接已结束才结束会话"
    );
    assert_eq!(
        heartbeat_outcome(Err(SendFault::Encoding)),
        HeartbeatOutcome::Ended,
        "本机装不出信封同样是会话级失败"
    );
}

/// `feature_ids`：`auth.server_challenge.selectedFeatures` 的 ID 列表。
fn feature_ids(body: &Value) -> Vec<String> {
    body["selectedFeatures"]
        .as_array()
        .expect("selectedFeatures")
        .iter()
        .map(|feature| feature.as_str().expect("feature id").to_owned())
        .collect()
}
