//! 组合根：构造并持有全部具体实现（`docs/MODULE_ARCHITECTURE.md` §6、`design.md` 决策 1/6）。
//!
//! 本模块只做装配，不承载业务规则：
//!
//! | 端口 / 注入口 | 具体实现 |
//! | --- | --- |
//! | `SessionStore` / `RemoteDeliveryStore` / `ExportStore` / `TrustStore` / `LocalConfigStore` / `AttachmentStore` / `AuditStore` | `storage_sqlite::SqliteStore`（一次 `open` 得到同一实例） |
//! | `AgentCatalog` / `SessionBackendFactory` | `agent_host::AgentHost`（本机 Agent 的目录与端点；本切片不接空闲回收任务） |
//! | `CredentialResolver` | [`KeystoreCredentialResolver`]（keystore 条目命名与 `provider.configure` 同规则） |
//! | `Clock` | [`crate::clock::SystemClock`] |
//! | `IdGenerator` | [`UuidIdGenerator`] |
//! | `EventPublisher` | [`ForkedPublisher`]（结构化日志 + Node Link 事件扇出，[`forked_publisher`]） |
//! | `EntropySource` / `IdentityKeystore` | `identity_keystore::{OsEntropy, FileKeystore, EphemeralKeystore}` |
//! | `identity_auth::Authority` | 组合根持有的共享状态机 |
//! | `AuditHook`（连接级拒绝） | [`AuditSink`]（有界通道 + 由组合根持有的写任务） |
//! | `ConnectionCloser` | [`NodeLinkCloser`]（把撤销通知接到 Node Link 的连接注册表与命令管线） |
//! | `DaemonControl` | `crate::daemon::AppDaemonControl` |
//!
//! 关闭期的资源释放由 [`Composition::close`] 负责：它逐一释放共享句柄，再执行 `storage-sqlite` 的
//! `close()`（含 `wal_checkpoint(TRUNCATE)`）。任何仍被其他组件持有的句柄都会让 `Arc::try_unwrap`
//! 失败——那时**不静默跳过检查点**，而是上报为关闭错误。

use std::sync::Arc;

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::{
    Actor, AgentProfile, AuditAction, AuditOutcome, AuditRecord, CommittedDelivery, CommittedEvent,
    DeviceId, EntityRef, ExportId, NodeId, PortError, SecretValue, ServerEpoch, Timestamp,
    UnavailableKind,
};
use acp_core::ports::{
    AgentCatalog, AttachmentStore, AuditStore, Clock, CredentialResolver, EventPublisher,
    IdGenerator, LocalConfigStore, RetentionPolicy, SessionBackendFactory, SessionStore,
};
use acp_core::ports::{ExportStore, RemoteDeliveryStore, TrustStore};
use acp_core::use_cases::{UseCaseDeps, UseCases};
use agent_host::{AgentHost, HostConfig};
use identity_auth::{Authority, EntropySource, IdentityKeystore, PeerPublicKey, SecretPurpose};
use identity_keystore::{EphemeralKeystore, FileKeystore, OsEntropy};
use server::local_admin::ConnectionCloser;
use server::node_link::CommandRoute;
use server::node_link::resource::NodeLinkPublisher;
use server::transport::local::{AuditHook, AuthorizationDenied};
use storage_sqlite::error::StorageError;
use storage_sqlite::session_store::SqliteStore;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::clock::SystemClock;
use crate::config::{Config, ConfigError, KeystoreChoice};
use crate::identity::{IdentityUnavailable, NodeIdentity};

/// keystore 根目录（`<data_dir>/keystore`）。
const KEYSTORE_DIRECTORY: &str = "keystore";

/// 连接级拒绝审计队列的容量（有界，避免被拒绝连接造成无界内存增长）。
const AUDIT_CHANNEL_CAPACITY: usize = 64;

/// 组合根装配失败。全部失败即拒绝启动（失败关闭）。
#[derive(Debug, thiserror::Error)]
pub enum ComposeError {
    /// 打开（必要时迁移）存储失败。
    #[error("打开或迁移存储失败")]
    Store(#[source] StorageError),
    /// 首次种子导入失败。
    #[error("首次种子导入失败")]
    Seed(#[source] PortError),
    /// 种子条目本身非法（键名/序号级说明，不含配置值）。
    #[error("`[[agents.profiles]]` 第 {index} 项非法：{reason}")]
    InvalidSeed {
        /// 条目序号（从 1 开始）。
        index: usize,
        /// 简短原因。
        reason: &'static str,
    },
    /// 本节点身份不可用。
    #[error("本节点身份不可用")]
    Identity(#[source] IdentityUnavailable),
    /// 平台没有可用的安全存储后端，且配置要求失败关闭。
    #[error("平台没有可用的安全存储后端")]
    KeystoreUnavailable,
    /// 关闭时仍有其它组件持有存储句柄，无法完成 `wal_checkpoint(TRUNCATE)`。
    #[error("关闭时存储仍被其他组件持有")]
    StoreStillShared,
}

impl ComposeError {
    /// 不含路径与凭据的简短说明（CLI 的 stderr 与日志都用它）。
    ///
    /// `storage-sqlite` 的 `Display` 可能包含完整路径（`PathUnusable`/`InsecurePermissions`），
    /// `SECURITY_DESIGN.md` §14.1 禁止把它写进日志/CLI 输出，因此这里只给**分类**级描述。
    pub fn message(&self) -> String {
        match self {
            Self::Store(source) => {
                format!(
                    "open or migrate the local store failed: {}",
                    storage_error_token(source)
                )
            }
            Self::Seed(source) => format!(
                "the first-time seed import failed: {}",
                port_error_token(source)
            ),
            Self::InvalidSeed { index, reason } => {
                format!("agent profile seed #{index} is invalid: {reason}")
            }
            Self::Identity(_) => "this node's identity is not usable".to_owned(),
            Self::KeystoreUnavailable => "no platform keystore backend is available".to_owned(),
            Self::StoreStillShared => "the store is still shared at shutdown time".to_owned(),
        }
    }
}

/// `PortError` 的稳定 token（日志用；**不**转述可能含 SQL/路径的内层文本，§14.1）。
pub fn port_error_token(error: &PortError) -> &'static str {
    match error {
        PortError::NotFound(_) => "not_found",
        PortError::Conflict(_) => "conflict",
        PortError::InvalidRequest(_) => "invalid_request",
        PortError::Unavailable(kind) => kind.as_str(),
        PortError::Corrupt(_) => "corrupt",
        PortError::Backend(_) => "backend",
    }
}

/// `storage-sqlite` 失败的分类 token（路径级细节不进日志）。
pub fn storage_error_token(error: &StorageError) -> &'static str {
    match error {
        StorageError::Sql(_) => "sql",
        StorageError::Io(_) => "io",
        StorageError::FileFormatTooNew { .. } | StorageError::SchemaTooNew { .. } => "too_new",
        StorageError::PathUnusable { .. } => "path_unusable",
        StorageError::InsecurePermissions { .. } => "insecure_permissions",
        StorageError::FailClosed(_) => "fail_closed",
        StorageError::Corrupt(_) => "corrupt",
        StorageError::OutOfRange(_) => "out_of_range",
        StorageError::InvalidRequest(_) => "invalid_request",
        StorageError::StorageFull => "storage_full",
        StorageError::ColumnValue { .. } => "column_value",
    }
}

/// 首次种子导入的结果（用于启动日志）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedOutcome {
    /// 首次初始化：导入了 `count` 个 profile 并提交「已初始化」标记（同一事务）。
    Imported {
        /// profile 数量（可以为 0：空种子也写标记）。
        count: usize,
    },
    /// 已经初始化过：不再导入，SQLite 是唯一权威。
    AlreadySeeded,
}

/// 组合根持有的全部具体实现。
pub struct Composition {
    config: Config,
    started_at: Timestamp,
    started_instant: std::time::Instant,
    store: Arc<SqliteStore>,
    keystore: Arc<dyn IdentityKeystore>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    identity: NodeIdentity,
    authority: Arc<Authority>,
    host: Arc<AgentHost>,
    /// broker 句柄：`UseCases` 内部也持有同一实例，但合并窗口的定时任务需要直接调 `Broker::pump`
    /// （core 不读时钟、不设定时器，见 `crates/core/src/broker.rs` 模块头）。
    broker: Arc<Broker>,
    use_cases: Arc<UseCases>,
}

impl Composition {
    /// 打开（必要时迁移）存储并装配其余端口与用例面。
    ///
    /// **不**取单实例锁、**不**创建 endpoint、**不**写管理状态（种子导入由 [`Composition::seed_if_needed`]）：
    /// 启动序列的顺序由 `crate::daemon` 掌握。
    ///
    /// `publisher` 是 broker 提交成功后的发布端口（`CORE_PORTS_AND_STORAGE.md` §6 第 1/3 条）：组合根用它
    /// 注入 [`forked_publisher`] 的分叉（日志 + Node Link 扇出），测试可只注入 [`LoggingPublisher`]。
    pub async fn assemble(
        config: Config,
        publisher: Arc<dyn EventPublisher>,
    ) -> Result<Self, ComposeError> {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
        let started_at = clock.now();
        let store = Arc::new(
            SqliteStore::open(config.storage.clone(), &started_at)
                .await
                .map_err(ComposeError::Store)?,
        );
        // 同一个 `SqliteStore` 实例实现七个端口：每个端口一次显式向上转型（`Arc<SqliteStore>` →
        // `Arc<dyn …>`），不在字段位置依赖隐式强制转换。
        let session_store: Arc<dyn SessionStore> = store.clone();
        let deliveries: Arc<dyn RemoteDeliveryStore> = store.clone();
        let export_store: Arc<dyn ExportStore> = store.clone();
        let trust_store: Arc<dyn TrustStore> = store.clone();
        let audit_store: Arc<dyn AuditStore> = store.clone();
        let local_config: Arc<dyn LocalConfigStore> = store.clone();
        let attachments: Arc<dyn AttachmentStore> = store.clone();

        let entropy: Arc<dyn EntropySource> = Arc::new(OsEntropy::new());
        let keystore = build_keystore(&config, Arc::clone(&entropy))?;
        let identity = NodeIdentity::ensure(keystore.as_ref())
            .await
            .map_err(ComposeError::Identity)?;
        let ids: Arc<dyn IdGenerator> = Arc::new(UuidIdGenerator);
        let credentials: Arc<dyn CredentialResolver> = Arc::new(KeystoreCredentialResolver::new(
            Arc::clone(&keystore),
            Arc::clone(&local_config),
        ));
        let authority = Arc::new(Authority::new(
            identity.node_id().clone(),
            identity.key().clone(),
            Arc::clone(&keystore),
            Arc::clone(&entropy),
            Arc::clone(&clock),
        ));
        let host = Arc::new(AgentHost::new(
            Arc::clone(&local_config),
            credentials,
            // 本切片不接 `sessions.idle_timeout_ms`（「已知但未接线」）与客户端能力宣告
            // （`HostConfig::default()` = `ClientCapabilities::none()`：没有可作答的上游客户端，不虚报能力）。
            HostConfig::default(),
            Arc::clone(&ids),
            Arc::clone(&clock),
        ));
        let backends: Arc<dyn SessionBackendFactory> = host.clone();
        let catalog: Arc<dyn AgentCatalog> = host.clone();
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: Arc::clone(&session_store),
                deliveries: Arc::clone(&deliveries),
                backends,
                exports: Arc::clone(&export_store),
                trust: Arc::clone(&trust_store),
                publisher,
                clock: Arc::clone(&clock),
                ids: Arc::clone(&ids),
                audit: Some(Arc::clone(&audit_store)),
            },
            BrokerConfig {
                persist_deltas: config.storage.persist_deltas,
                // `sessions.queue_policy`/`sessions.max_queued_turns` 属未接线段落：取 v1 默认
                // （`queue` / 16，与 `SYNC_PROTOCOL.md` §11.6 的二选一一致）。
                ..BrokerConfig::default()
            },
        ));
        let use_cases = Arc::new(UseCases::new(UseCaseDeps {
            broker: Arc::clone(&broker),
            store: session_store,
            deliveries,
            exports: export_store,
            trust: trust_store,
            audit: audit_store,
            config: local_config,
            attachments,
            catalog,
            clock: Arc::clone(&clock),
            ids: Arc::clone(&ids),
        }));
        Ok(Self {
            config,
            started_at,
            started_instant: std::time::Instant::now(),
            store,
            keystore,
            clock,
            ids,
            identity,
            authority,
            host,
            broker,
            use_cases,
        })
    }

    /// 启动配置。
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Daemon 启动时间。
    pub fn started_at(&self) -> &Timestamp {
        &self.started_at
    }

    /// 单调计时起点（`daemon.status.uptimeMs` 的来源：不受系统时间跳变影响）。
    pub fn started_instant(&self) -> std::time::Instant {
        self.started_instant
    }

    /// 本节点身份。
    pub fn identity(&self) -> &NodeIdentity {
        &self.identity
    }

    /// 身份状态机（配对方法与 `PairingSessions` 共用同一实例）。
    pub fn authority(&self) -> &Arc<Authority> {
        &self.authority
    }

    /// 本机 Agent 宿主（`AgentCatalog` + `SessionBackendFactory`；关闭序列调用 `shutdown_all`）。
    pub fn host(&self) -> &Arc<AgentHost> {
        &self.host
    }

    /// 用例面。
    pub fn use_cases(&self) -> &Arc<UseCases> {
        &self.use_cases
    }

    /// broker（`UseCases` 内部的同一实例）：合并窗口的定时任务直接调 `Broker::pump`。
    pub fn broker(&self) -> &Arc<Broker> {
        &self.broker
    }

    /// 平台安全存储端口（注入 `LocalAdminDeps`）。
    pub fn keystore(&self) -> &Arc<dyn IdentityKeystore> {
        &self.keystore
    }

    /// 时间来源。
    pub fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    /// id 分配器。
    pub fn ids(&self) -> &Arc<dyn IdGenerator> {
        &self.ids
    }

    /// 审计仓储（`audit.export` 的读面与连接级拒绝的写面）。
    pub fn audit_store(&self) -> Arc<dyn AuditStore> {
        self.store.clone()
    }

    /// 本节点公钥（`daemon.status.nodePublicKey`）。
    pub fn node_public_key(&self) -> &PeerPublicKey {
        self.identity.public_key()
    }

    /// 存储层的库级事实（诊断用；`None` 表示 `meta.server_epoch` 不是规范 UUID）。
    pub fn server_epoch(&self) -> Option<ServerEpoch> {
        ServerEpoch::new(&self.store.metadata().server_epoch).ok()
    }

    /// §7.5 的保留/容量策略（周期清理用）。
    pub fn retention_policy(&self) -> RetentionPolicy {
        RetentionPolicy {
            transcript_retention_days: self.config.storage.transcript_retention_days,
            sync_event_retention_days: self.config.storage.sync_event_retention_days,
            audit_retention_days: self.config.storage.audit_retention_days,
            max_total_size_bytes: self.config.storage.max_total_size_bytes,
            max_session_size_bytes: self.config.storage.max_session_size_bytes,
            persist_deltas: self.config.storage.persist_deltas,
        }
    }

    /// 附件存储（周期孤儿回收用）。
    pub fn attachments(&self) -> Arc<SqliteStore> {
        Arc::clone(&self.store)
    }

    /// 首次种子导入（`CONFIG_REFERENCE.md` 的「配置与管理状态的权威」）。
    ///
    /// - 未初始化：把配置文件里的 profile 与「已初始化」标记**同一事务**提交（空种子也写标记）；
    /// - 已初始化：不再导入，SQLite 是唯一权威；配置文件仍含 profile 时只记一条不含参数值的提示。
    pub async fn seed_if_needed(&self) -> Result<SeedOutcome, ComposeError> {
        let actor = Actor::LocalCli;
        let state = self
            .use_cases
            .seed_state(&actor)
            .await
            .map_err(ComposeError::Seed)?;
        if state.is_seeded() {
            if !self.config.seeds.is_empty() {
                tracing::warn!(
                    event = "daemon.seed_skipped",
                    configured_seeds = self.config.seeds.len(),
                    "管理存储已初始化：配置文件里的 profile 不再导入，请改用 `agent.configure`"
                );
            }
            return Ok(SeedOutcome::AlreadySeeded);
        }
        let profiles = self
            .config
            .seeds
            .iter()
            .enumerate()
            .map(|(index, seed)| {
                seed.to_profile(&self.started_at)
                    .map_err(|error| ComposeError::InvalidSeed {
                        index: index + 1,
                        reason: seed_invalid_reason(&error),
                    })
            })
            .collect::<Result<Vec<AgentProfile>, _>>()?;
        let count = profiles.len();
        self.use_cases
            .mark_seeded(&actor, profiles)
            .await
            .map_err(ComposeError::Seed)?;
        Ok(SeedOutcome::Imported { count })
    }

    /// 关闭存储：释放本对象持有的共享句柄后执行 `SqliteStore::close()`（含 `wal_checkpoint(TRUNCATE)`）。
    ///
    /// **调用方必须先释放其它持有者**（连接处理器、`DaemonControl`、周期任务、审计写任务、Agent 宿主）：
    /// 否则 `Arc::try_unwrap` 失败，本方法上报 [`ComposeError::StoreStillShared`] 并**不**假装已检查点。
    pub async fn close(self) -> Result<(), ComposeError> {
        let Self {
            store,
            use_cases,
            host,
            broker,
            authority,
            keystore,
            clock,
            ids,
            identity,
            config,
            ..
        } = self;
        // 显式释放：`UseCases`/`Broker`/`AgentHost` 各自持有存储句柄（`Arc<dyn …>` 克隆）。
        drop(use_cases);
        drop(broker);
        drop(host);
        drop(authority);
        drop(keystore);
        drop(clock);
        drop(ids);
        drop(identity);
        drop(config);
        match Arc::try_unwrap(store) {
            Ok(store) => {
                store.close().await;
                Ok(())
            }
            Err(_) => Err(ComposeError::StoreStillShared),
        }
    }
}

/// 种子条目非法的**静态原因**（不回显配置值）。
fn seed_invalid_reason(error: &ConfigError) -> &'static str {
    match error {
        ConfigError::Invalid { detail } if detail.contains("agent_id") => "invalid agentId",
        ConfigError::Invalid { detail } if detail.contains("env") => "invalid env binding",
        _ => "not a valid agent profile",
    }
}

/// 选择并构造 keystore（`identity.keystore` + `identity.fail_closed_on_missing_keystore` + `dev_mode.*`）。
///
/// 失败关闭是本函数的默认行为：平台不支持时**不**静默降级成进程内身份，只有配置显式选择
/// `identity.keystore = "ephemeral"`（且 `dev_mode.enabled = true`，由 `crate::config` 校验）或
/// `dev_mode.ephemeral_identity = true` 时才使用 [`EphemeralKeystore`]。
fn build_keystore(
    config: &Config,
    entropy: Arc<dyn EntropySource>,
) -> Result<Arc<dyn IdentityKeystore>, ComposeError> {
    if config.dev_mode.ephemeral_identity || config.keystore == KeystoreChoice::Ephemeral {
        tracing::warn!(
            event = "daemon.ephemeral_identity",
            "开发模式：节点身份只存在于本进程内（重启即换身份）"
        );
        return Ok(Arc::new(EphemeralKeystore::new(entropy)));
    }
    if identity_keystore::platform_supported() {
        return Ok(Arc::new(FileKeystore::new(
            config.data_dir.join(KEYSTORE_DIRECTORY),
            entropy,
        )));
    }
    // 平台没有后端：`fail_closed_on_missing_keystore = false`（只允许出现在开发模式配置里，已由
    // `crate::config` 校验）才允许用进程内身份继续，并记一条警告——不是静默降级。
    if !config.fail_closed_on_missing_keystore {
        tracing::warn!(
            event = "daemon.keystore_missing",
            "平台没有可用的安全存储后端：按 `identity.fail_closed_on_missing_keystore = false` 使用进程内身份"
        );
        return Ok(Arc::new(EphemeralKeystore::new(entropy)));
    }
    Err(ComposeError::KeystoreUnavailable)
}

// ---------------------------------------------------------------------------------------------
// 端口实现
// ---------------------------------------------------------------------------------------------

/// 规范小写 UUID 文本（`uuid` 的 v4 = 系统 CSPRNG，`docs/SECURITY_DESIGN.md` §13.1）。
fn new_uuid_text() -> String {
    Uuid::new_v4().hyphenated().to_string()
}

/// `IdGenerator`：`uuid v4`。各 newtype 只接受规范小写 UUID 文本，而 `Uuid::hyphenated()` 的输出必然
/// 是该形状（36 字符、小写十六进制、四个连字符），因此这里的构造不可失败。
#[derive(Debug, Clone, Copy)]
pub struct UuidIdGenerator;

macro_rules! uuid_id {
    ($($method:ident -> $type:ty),* $(,)?) => {
        impl IdGenerator for UuidIdGenerator {
            $(
                fn $method(&self) -> $type {
                    <$type>::new(&new_uuid_text())
                        .expect("v4 文本必然是规范小写 uuid 文本")
                }
            )*
        }
    };
}

uuid_id! {
    turn_id -> acp_core::model::TurnId,
    interaction_id -> acp_core::model::InteractionId,
    pairing_id -> acp_core::model::PairingId,
    message_id -> acp_core::model::MessageId,
    origin_epoch -> acp_core::model::OriginEpoch,
    request_id -> acp_core::model::RequestId,
}

/// `EventPublisher`：本切片**没有订阅者**（`server::sync`/`server::node_link` 尚未落地），因此只记一条
/// 不含正文的结构化日志，同时把「发布发生在提交之后」这一顺序点保留在 `Broker` 之后（§6 第 1/3 条）。
///
/// 切片 6 装配 WSS/Node Link 连接表时替换本实现；`publish` 的契约（不得回滚已提交事务）不受影响。
#[derive(Debug, Clone, Copy)]
pub struct LoggingPublisher;

impl EventPublisher for LoggingPublisher {
    fn publish(&self, delivery: CommittedDelivery) {
        match delivery {
            CommittedDelivery::Owned(event) => tracing::trace!(
                event = "daemon.published",
                kind = "owned",
                event_id = %event.id.as_str(),
                global_sequence = event.global_sequence.get(),
                "本切片没有事件订阅者（server::sync/node_link 未落地）"
            ),
            CommittedDelivery::Imported {
                event_type,
                local_sequence,
                ..
            } => tracing::trace!(
                event = "daemon.published",
                kind = "imported",
                event_type = event_type.as_str(),
                local_sequence = local_sequence.get(),
                "本切片没有事件订阅者（server::sync/node_link 未落地）"
            ),
        }
    }
}

/// `CredentialResolver`：把 profile 的凭据绑定解析成子进程环境变量。
///
/// 命名与 `provider.configure`（`server::local_admin::router`）**同规则**：
///
/// - 条目标签 = `<ProviderRef.keystore_ref>.<fieldName>`，而 `keystore_ref` 由 `provider.configure` 写为
///   `sha256(providerId)` 前 16 个小写 hex 字符 + `@v<version>`（`reports/wp3b1-handoff.md`）；
/// - 因此本实现**读回** `ProviderRef`（SQLite 只保存非秘密引用），不自行重算摘要，避免两处口径漂移。
///
/// 失败一律 `Unavailable(KeystoreUnavailable)`（失败关闭）：绑定名不在白名单、Provider 引用或字段不存在、
/// keystore 不可用、值不是 UTF-8，都不得静默跳过该变量后继续启动 Agent。
pub struct KeystoreCredentialResolver {
    keystore: Arc<dyn IdentityKeystore>,
    config: Arc<dyn LocalConfigStore>,
}

impl KeystoreCredentialResolver {
    /// 装配。
    pub fn new(keystore: Arc<dyn IdentityKeystore>, config: Arc<dyn LocalConfigStore>) -> Self {
        Self { keystore, config }
    }
}

/// 凭据失败：不区分成因（调用方只应知道「凭据不可用」），且**不回显字段值或标签**。
fn credentials_unavailable() -> PortError {
    PortError::Unavailable(UnavailableKind::KeystoreUnavailable)
}

#[async_trait::async_trait]
impl CredentialResolver for KeystoreCredentialResolver {
    async fn resolve_env(
        &self,
        profile: &AgentProfile,
    ) -> Result<Vec<(String, SecretValue)>, PortError> {
        if profile.env().is_empty() {
            return Ok(Vec::new());
        }
        let references = self.config.provider_refs().await?;
        let mut resolved = Vec::with_capacity(profile.env().len());
        for binding in profile.env() {
            // 白名单是注入上限（`SECURITY_DESIGN.md` §12.2）：绑定名不在白名单内即失败关闭。
            // `AgentProfile::try_new` 已保证不会出现这种状态，这里复查以防外部改写的库。
            if !profile
                .env_allowlist()
                .iter()
                .any(|name| name == binding.name())
            {
                return Err(credentials_unavailable());
            }
            let reference = references
                .iter()
                .find(|reference| reference.id() == binding.provider_id())
                .ok_or_else(credentials_unavailable)?;
            if !reference
                .configured_fields()
                .iter()
                .any(|field| field == binding.field())
            {
                return Err(credentials_unavailable());
            }
            let label = provider_secret_label(reference.keystore_ref(), binding.field());
            let secret = self
                .keystore
                .get_secret(SecretPurpose::ProviderCredential, &label)
                .await
                .map_err(|_| credentials_unavailable())?
                .ok_or_else(credentials_unavailable)?;
            let value =
                String::from_utf8(secret.into_bytes()).map_err(|_| credentials_unavailable())?;
            resolved.push((binding.name().to_owned(), SecretValue::new(value)));
        }
        Ok(resolved)
    }
}

/// 某个字段的 keystore 标签：`<keystore_ref>.<fieldName>`（与 `provider.configure` 的写入规则同形）。
pub fn provider_secret_label(keystore_ref: &str, field: &str) -> String {
    format!("{keystore_ref}.{field}")
}

/// 连接级拒绝的审计落地（`LOCAL_ADMIN_PROTOCOL.md` §2.2、`SECURITY_DESIGN.md` §14.2）。
///
/// `AuditHook` 的实现必须是同步且不阻塞的，而 `AuditStore::append` 是异步的：因此本实现把记录放进
/// **有界**通道，由一个由组合根持有的写任务落库（该任务在关闭序列里被关闭并等待，不是 detached task）。
/// 通道满或已关闭时丢弃并记一条警告——同一事件已经在 `transport::local` 里记过结构化日志，放弃的只是
/// 「写库」这一份，且丢弃是有界的（不无界增长）。
///
/// 审计行的形状（本切片口径）：`actor = LocalCli`、`target = Node(本节点)`、`outcome = denied`、
/// `action = authorization.denied`；`detail_digest` 为空，传输种类/拒绝原因/对端 OS 标识只在结构化日志里
/// （`EntityRef` 没有「连接」变体，`owned_audit` 的 `target_kind` 词表由合同漂移门禁钉死，不能新增取值）。
pub struct AuditSink {
    sender: mpsc::Sender<AuditRecord>,
    clock: Arc<dyn Clock>,
    local_node: NodeId,
}

impl std::fmt::Debug for AuditSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("AuditSink").finish_non_exhaustive()
    }
}

/// 审计写任务的所有者句柄（关闭序列通过它排空并等待任务结束）。
pub struct AuditWriter {
    sender: mpsc::Sender<AuditRecord>,
    handle: tokio::task::JoinHandle<()>,
}

impl AuditWriter {
    /// 启动写任务，返回「注入给 endpoint 的 hook」与「由组合根持有的任务句柄」。
    pub fn start(
        audit: Arc<dyn AuditStore>,
        clock: Arc<dyn Clock>,
        local_node: NodeId,
    ) -> (Arc<AuditSink>, Self) {
        let (sender, mut receiver) = mpsc::channel::<AuditRecord>(AUDIT_CHANNEL_CAPACITY);
        let task_sender = sender.clone();
        let handle = tokio::spawn(async move {
            while let Some(record) = receiver.recv().await {
                if let Err(error) = audit.append(record).await {
                    tracing::warn!(
                        event = "authorization.denied_audit_failed",
                        error = port_error_token(&error),
                        "连接级拒绝的审计记录写入失败"
                    );
                }
            }
        });
        (
            Arc::new(AuditSink {
                sender,
                clock,
                local_node,
            }),
            Self {
                sender: task_sender,
                handle,
            },
        )
    }

    /// 关闭：关闭通道（写任务写完已入队的记录后退出）并等待它结束。
    pub async fn close(self) {
        drop(self.sender);
        if let Err(error) = self.handle.await {
            tracing::warn!(event = "daemon.task_failed", task = "audit_writer", error = %error);
        }
    }
}

impl AuditHook for AuditSink {
    fn authorization_denied(&self, event: AuthorizationDenied<'_>) {
        // `AuditHook` 是同步的、写库是异步的：除了把拒绝事件交给写任务，还必须留下一条结构化日志——
        // `deny_connection` 自己什么都不打印（§2.2 只要求「已审计」），否则一次连接级拒绝在运维上不可见。
        // `peer_identity`（SID/uid 字符串）是诊断所需，不是秘密，但也不写进本行以外的位置。
        tracing::warn!(
            event = "local_admin.connection_rejected",
            transport = event.transport,
            instance_id = event.instance_id,
            reason = event.reason.as_str(),
            "本地通道拒绝了本次连接（对端 OS 用户不匹配或无法确认）"
        );
        let record = AuditRecord::try_new(
            self.clock.now(),
            AuditAction::AuthorizationDenied,
            Actor::LocalCli,
            None,
            None,
            EntityRef::Node(self.local_node.clone()),
            AuditOutcome::Denied,
            None,
        );
        match record {
            Ok(record) => {
                if self.sender.try_send(record).is_err() {
                    tracing::warn!(
                        event = "authorization.denied_audit_dropped",
                        transport = event.transport,
                        reason = event.reason.as_str(),
                        "审计写通道已满或已关闭：本事件只保留结构化日志"
                    );
                }
            }
            Err(_) => tracing::error!(
                event = "authorization.denied_audit_invalid",
                "连接级拒绝的审计记录构造失败"
            ),
        }
    }
}

/// `ConnectionCloser`：撤销提交后关闭该设备/节点的 active connection 并通知 Export 撤销（`design.md` D7）。
///
/// `local_admin` 已经在**持久提交成功之后**才调本实现（`local_admin::pairing`），因此这里只做「通知与关闭」：
/// 推送/关闭失败只记日志，不回滚已提交的撤销。授权判定不依赖推送——`node_link` 在处理 `resource.attach`、
/// `command.submit` 与 catalog 订阅时按当次持久化记录复核（D7）。
///
/// `close_device` 目前没有可关闭的连接：`server::sync` 未落地（设备连接属切片 7），本实现记录一条结构化
/// 事件并明确「本次没有可关闭的连接」，不假装成功。
#[derive(Debug)]
pub struct NodeLinkCloser {
    command: Arc<CommandRoute>,
}

impl NodeLinkCloser {
    /// 装配：`command` 是本机（Owner）的命令管线，它内部持有连接注册表与 resource 路由。
    pub fn new(command: Arc<CommandRoute>) -> Self {
        Self { command }
    }
}

#[async_trait::async_trait]
impl ConnectionCloser for NodeLinkCloser {
    async fn close_device(&self, device: &DeviceId) {
        tracing::info!(
            event = "daemon.revocation_connection_sweep",
            target_kind = "device",
            target_id = %device.as_str(),
            closed_connections = 0u64,
            "撤销已提交：本切片没有按设备持有的连接表（server::sync 未落地，设备连接属切片 7）"
        );
    }

    async fn close_node(&self, node: &NodeId) {
        let closed = self.command.node_revoked(node).await;
        tracing::info!(
            event = "daemon.revocation_connection_sweep",
            target_kind = "node",
            target_id = %node.as_str(),
            closed_connections = u64::try_from(closed).unwrap_or(u64::MAX),
            "撤销已提交：已推送 node.trust.revoked 并以 4410 关闭该节点的连接"
        );
    }

    async fn export_revoked(&self, export: &ExportId) {
        let notified = self.command.export_revoked(export).await;
        tracing::info!(
            event = "daemon.export_revoked_notification",
            target_kind = "export",
            target_id = %export.as_str(),
            notified_connections = u64::try_from(notified).unwrap_or(u64::MAX),
            "撤销已提交：已向持有该 Export 的活跃连接推送 export.revoked"
        );
    }
}

/// `EventPublisher` 分叉（`design.md` D6）：一路保留组合根的结构化日志（[`LoggingPublisher`] 的既有语义），
/// 一路把同一个已提交投递转发给 `server::node_link` 的事件扇出。
///
/// 分叉点在 `app`（组合根）而不在 `server`：`server::node_link` 不需要知道其他消费方。`publish` 仍是同步
/// 且非阻塞的（扇出侧队列满只丢这一条并记日志，已提交的事务不回滚，见 D6 的既定取舍）。
struct ForkedPublisher {
    node_link: NodeLinkPublisher,
}

impl EventPublisher for ForkedPublisher {
    fn publish(&self, delivery: CommittedDelivery) {
        LoggingPublisher.publish(delivery.clone());
        // 只有 `CommittedDelivery::Owned` 会变成 `resource.event`（imported 投递不跨节点再导出，§4）。
        self.node_link.publish(delivery);
    }
}

/// 装配 broker 的发布端口与它的事件队列（组合根在 [`Composition::assemble`] 之前调用）。
///
/// 返回的接收端交给 `server::node_link::ResourceRoute::dispatch`（由 `crate::daemon` spawn 并纳入关闭
/// 序列）；两者必须同时存在，否则事件会在扇出消费者出现之前被丢弃。
pub fn forked_publisher() -> (Arc<dyn EventPublisher>, mpsc::Receiver<CommittedEvent>) {
    let (node_link, queue) = NodeLinkPublisher::channel();
    (Arc::new(ForkedPublisher { node_link }), queue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use acp_core::model::{AgentId, ProviderEnvBinding, ProviderRefKind};
    use server::local_admin::{
        AdminOutcome, DaemonControl, DaemonStatus, LocalAdminDeps, LocalAdminHandler,
        LocalAdminRouter, NoConnections, PairingSessions, decode_request,
    };

    /// 每个用例一个独立临时目录（用例结束即删）。
    struct TempDir {
        path: std::path::PathBuf,
    }

    /// 与 `storage-sqlite` 的目录创建同口径：Unix 上按 `0700` 建立，否则 `strict_permissions`
    /// 会在 Linux/macOS 上对既有目录失败关闭（Windows 的权限判定是 `Unverifiable`，本地不触发）。
    #[cfg(unix)]
    fn create_owner_only_dir(path: &std::path::Path) {
        use std::os::unix::fs::DirBuilderExt as _;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .expect("临时目录");
    }

    #[cfg(not(unix))]
    fn create_owner_only_dir(path: &std::path::Path) {
        std::fs::create_dir_all(path).expect("临时目录");
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "acpr-wp4a-compose-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            create_owner_only_dir(&path);
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// 开发模式配置：显式 `identity.keystore = "ephemeral"`，因此在任何平台上都能装配
    /// （Linux/CI 没有平台 keystore 后端，正式档位会失败关闭）。
    fn dev_config(data_dir: &std::path::Path, extra: &str) -> Config {
        let text = format!(
            "[daemon]\ndata_dir = {dir:?}\npublic_origin = \"https://host.example\"\n\
             [identity]\nkeystore = \"ephemeral\"\n[dev_mode]\nenabled = true\n{extra}",
            dir = data_dir.display()
        );
        Config::from_toml(&text).expect("开发模式配置")
    }

    /// 仅供测试的最小 `DaemonControl` 实现（本模块的用例不经 `daemon.*`）。
    struct TestDaemonControl;

    #[async_trait::async_trait]
    impl DaemonControl for TestDaemonControl {
        async fn status(&self) -> DaemonStatus {
            unreachable!("本模块的用例不经 daemon.status")
        }

        async fn stop(
            &self,
            _grace_ms: Option<u64>,
        ) -> Result<(), server::local_admin::AdminError> {
            unreachable!("本模块的用例不经 daemon.stop")
        }
    }

    /// keystore 选择：平台不支持时默认失败关闭；显式开发模式开关才转向进程内身份。
    #[test]
    fn keystore_selection_fails_closed_outside_dev_mode() {
        let dir = TempDir::new("keystore");
        let entropy: Arc<dyn EntropySource> = Arc::new(OsEntropy::new());
        let ephemeral_config = dev_config(&dir.path, "");
        assert!(
            build_keystore(&ephemeral_config, Arc::clone(&entropy)).is_ok(),
            "显式 dev_mode + ephemeral 必须可用"
        );

        // 平台支持与否由编译期决定：只有「不支持 + 失败关闭」这一支才必须报错。
        let platform_config =
            Config::from_toml(&format!("[daemon]\ndata_dir = {:?}\n", dir.path.display()))
                .expect("默认配置");
        let outcome = build_keystore(&platform_config, Arc::clone(&entropy));
        if identity_keystore::platform_supported() {
            assert!(outcome.is_ok(), "平台支持时 platform keystore 必须可用");
        } else {
            assert!(
                matches!(outcome, Err(ComposeError::KeystoreUnavailable)),
                "平台不支持时必须失败关闭"
            );
        }

        // `dev_mode.ephemeral_identity = true` 也走进程内身份（显式开发模式开关）。
        let forced = Config::from_toml(&format!(
            "[daemon]\ndata_dir = {:?}\n[dev_mode]\nenabled = true\nephemeral_identity = true\n",
            dir.path.display()
        ))
        .expect("开发模式配置");
        assert!(build_keystore(&forced, entropy).is_ok());
    }

    /// 凭据解析：写入用的条目命名（`provider.configure`）与解析侧标签**同规则**，且字段缺失或条目
    /// 被删除时失败关闭。
    #[tokio::test]
    async fn credentials_resolve_through_the_shared_keystore_layout() {
        let dir = TempDir::new("credentials");
        let composition =
            Composition::assemble(dev_config(&dir.path, ""), Arc::new(LoggingPublisher))
                .await
                .expect("装配");
        let keystore = Arc::clone(composition.keystore());
        let clock = Arc::clone(composition.clock());
        let store = composition.audit_store();

        let router = LocalAdminRouter::new(LocalAdminDeps {
            daemon: Arc::new(TestDaemonControl),
            core: Arc::clone(composition.use_cases()),
            keystore: Arc::clone(&keystore),
            audit: Arc::clone(&store),
            clock: Arc::clone(&clock),
            pairing: Arc::new(PairingSessions::new(
                Arc::clone(composition.authority()),
                None,
                Arc::new(NoConnections),
            )),
        });
        let payload = serde_json::to_vec(&serde_json::json!({
            "v": 1,
            "id": "2ae1c07c-0000-4000-8000-000000000001",
            "method": "provider.configure",
            "params": {
                "providerId": "openai.primary",
                "kind": "provider",
                "displayName": "OpenAI",
                "values": { "api_key": "s3cret-value" },
            },
        }))
        .expect("编码请求");
        let response = router
            .handle(decode_request(&payload).expect("信封合法"))
            .await;
        match response.outcome() {
            AdminOutcome::Success { .. } => {}
            other => panic!("provider.configure 必须成功，实际 {other:?}"),
        }

        // 解析侧：读回持久引用，按 `<keystore_ref>.<field>` 取秘密值。
        let resolver = KeystoreCredentialResolver::new(
            Arc::clone(&keystore),
            Arc::clone(&composition.attachments()) as Arc<dyn LocalConfigStore>,
        );
        let stored = composition
            .use_cases()
            .provider_refs(&Actor::LocalCli)
            .await
            .expect("读回引用");
        assert_eq!(stored.len(), 1);
        let reference = &stored[0];
        assert_eq!(reference.kind(), ProviderRefKind::Provider);
        assert_eq!(
            reference.keystore_ref().len(),
            "0123456789abcdef@v1".len(),
            "`sha256(providerId)[..16]@v<version>`"
        );
        assert_eq!(
            provider_secret_label(reference.keystore_ref(), "api_key"),
            format!("{}.api_key", reference.keystore_ref()),
            "字段条目标签 = `<ref>.<field>`"
        );

        let profile = |agent: &str, provider: &str, field: &str| {
            AgentProfile::try_new(
                AgentId::new(agent).expect("agent id"),
                "Codex",
                "codex",
                Vec::new(),
                vec!["OPENAI_API_KEY".to_owned()],
                vec![ProviderEnvBinding::try_new(provider, field, "OPENAI_API_KEY").expect("绑定")],
                true,
                clock.now(),
                clock.now(),
            )
            .expect("profile")
        };
        let resolved = resolver
            .resolve_env(&profile("codex", "openai.primary", "api_key"))
            .await
            .expect("可解析");
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0, "OPENAI_API_KEY");
        assert_eq!(resolved[0].1.expose_secret(), "s3cret-value");

        // 未配置的字段 → 失败关闭（不回退、不跳过）。
        assert!(matches!(
            resolver
                .resolve_env(&profile("codex", "openai.primary", "org"))
                .await,
            Err(PortError::Unavailable(UnavailableKind::KeystoreUnavailable))
        ));
        // 未登记的 Provider → 失败关闭。
        assert!(
            resolver
                .resolve_env(&profile("codex", "unknown.provider", "api_key"))
                .await
                .is_err()
        );

        // 条目被删除后仍然失败关闭（keystore 不可用/引用失效不得静默跳过变量）。
        keystore
            .delete_secret(
                SecretPurpose::ProviderCredential,
                &provider_secret_label(reference.keystore_ref(), "api_key"),
            )
            .await
            .expect("删除条目");
        assert!(
            resolver
                .resolve_env(&profile("codex", "openai.primary", "api_key"))
                .await
                .is_err()
        );

        // 未绑定凭据的 profile 返回空集合（不取值、不失败）。
        let plain = AgentProfile::try_new(
            AgentId::new("omp").expect("agent id"),
            "OMP",
            "omp",
            Vec::new(),
            vec!["PATH".to_owned()],
            Vec::new(),
            false,
            clock.now(),
            clock.now(),
        )
        .expect("profile");
        assert!(
            resolver
                .resolve_env(&plain)
                .await
                .expect("空集合")
                .is_empty()
        );

        // 关闭：先释放本用例持有的句柄（连接处理器与 router 已用完即弃）。
        drop(resolver);
        drop(router);
        drop(store);
        drop(keystore);
        composition.close().await.expect("可关闭");
    }

    /// `Composition::close` 必须真的完成检查点，而不是静默跳过：仍有共享句柄时上报错误。
    #[tokio::test]
    async fn a_shared_store_handle_blocks_the_checkpoint() {
        let dir = TempDir::new("shared");
        let composition =
            Composition::assemble(dev_config(&dir.path, ""), Arc::new(LoggingPublisher))
                .await
                .expect("装配");
        // 先留一份句柄：`close()` 会消费 `composition`，结尾还要靠它显式关池。
        let store = Arc::clone(&composition.store);
        let extra = composition.attachments();
        let error = composition.close().await.expect_err("仍有句柄时必须上报");
        assert!(matches!(error, ComposeError::StoreStillShared));
        assert!(error.message().contains("shared"), "{}", error.message());
        drop(extra);
        // 句柄全部释放后再关池并**等待**完成，临时目录守卫才删得掉：Windows 上未释放的文件句柄
        // 会让 `remove_dir_all` 失败（design D3：持有打开资源的用例必须先显式释放资源）。
        Arc::try_unwrap(store)
            .expect("除本用例外的句柄都已释放")
            .close()
            .await;
    }

    /// 首次种子导入只提交一次；已初始化后不再导入（R5–R7 的组合根部分）。
    #[tokio::test]
    async fn seeding_imports_once_and_never_reimports() {
        let dir = TempDir::new("seed");
        let seeds =
            "[[agents.profiles]]\nagent_id = \"codex\"\ncommand = \"codex-acp\"\ndefault = true\n";
        let composition =
            Composition::assemble(dev_config(&dir.path, seeds), Arc::new(LoggingPublisher))
                .await
                .expect("装配");
        assert_eq!(
            composition.seed_if_needed().await.expect("首次导入"),
            SeedOutcome::Imported { count: 1 }
        );
        let profiles = composition
            .use_cases()
            .profiles(&Actor::LocalCli)
            .await
            .expect("读回 profile");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id().as_str(), "codex");
        assert!(profiles[0].is_default());

        // 第二次调用（等价于重启）不再导入。
        assert_eq!(
            composition.seed_if_needed().await.expect("幂等"),
            SeedOutcome::AlreadySeeded
        );
        assert_eq!(
            composition
                .use_cases()
                .profiles(&Actor::LocalCli)
                .await
                .expect("读回 profile")
                .len(),
            1
        );

        // 运行期改动是权威：重启（新建组合根）不覆盖它。
        let renamed = AgentProfile::try_new(
            AgentId::new("codex").expect("agent id"),
            "Codex Two",
            "codex-acp",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            true,
            composition.clock().now(),
            composition.clock().now(),
        )
        .expect("profile");
        composition
            .use_cases()
            .put_profile(&Actor::LocalCli, renamed)
            .await
            .expect("运行期改动");
        composition.close().await.expect("可关闭");

        let restarted =
            Composition::assemble(dev_config(&dir.path, seeds), Arc::new(LoggingPublisher))
                .await
                .expect("重启装配");
        assert_eq!(
            restarted.seed_if_needed().await.expect("重启"),
            SeedOutcome::AlreadySeeded
        );
        let profiles = restarted
            .use_cases()
            .profiles(&Actor::LocalCli)
            .await
            .expect("读回 profile");
        assert_eq!(profiles.len(), 1);
        assert_eq!(
            profiles[0].display_name(),
            "Codex Two",
            "配置文件里的种子不得覆盖运行期的本地改动"
        );
        restarted.close().await.expect("可关闭");
    }
}
