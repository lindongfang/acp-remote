//! 组合根：构造并持有全部具体实现（`docs/MODULE_ARCHITECTURE.md` §6、`design.md` 决策 1/6）。
//!
//! 本模块只做装配，不承载业务规则：
//!
//! | 端口 / 注入口 | 具体实现 |
//! | --- | --- |
//! | `SessionStore` / `RemoteDeliveryStore` / `TrustStore` / `LocalConfigStore` / `AttachmentStore` / `AuditStore` | `storage_sqlite::SqliteStore`（一次 `open` 得到同一实例） |
//! | `AgentCatalog` / `SessionBackendFactory` | `agent_host::AgentHost`（本机 Agent 的目录与端点；本切片不接空闲回收任务） |
//! | `CredentialResolver` | [`KeystoreCredentialResolver`]（keystore 条目命名与 `provider.configure` 同规则） |
//! | `Clock` | [`crate::clock::SystemClock`] |
//! | `IdGenerator` | [`UuidIdGenerator`] |
//! | `EventPublisher` | [`ForkedPublisher`]（结构化日志 + Node Link 事件扇出，[`forked_publisher`]） |
//! | `EntropySource` / `IdentityKeystore` | `identity_keystore::{OsEntropy, FileKeystore, EphemeralKeystore}` |
//! | `identity_auth::Authority` | 组合根持有的共享状态机 |
//! | `AuditHook`（连接级拒绝） | [`AuditSink`]（有界通道 + 由组合根持有的写任务） |
//! | `ConnectionCloser` | [`NodeLinkCloser`]（把撤销通知与重新配对的连接作废接到 Node Link 的连接注册表与命令管线，把设备撤销接到 Sync 的连接注册表） |
//! | `sync::subscribe::OwnerReachability` | [`NodeLinkReachability`]（Owner 可达 = 存在来自该 node 的活跃 Node Link 连接） |
//! | `core::ports::DirectoryBrowser`（`workspace.browse` 的唯一端口） | core 自带的默认实现（[`UseCases::new`] 的默认值，即 `crates/core/src/use_cases.rs` 的 `SystemDirectories`：只列直接子目录、只读、不回退）。组合根**不替换**它——本机文件系统就是生产实现；[`UseCases::with_directory_browser`] 留给测试替身与另一份实现 |
//! | `core::ports::SessionFiles`（会话文件/Git 只读查询） | [`NativeSessionFiles`]：组合根构造**一个**实例，同时交给 [`UseCases::with_session_files`] 与 Sync 命令面（连接级版本释放），并在 [`Composition::close`] 调 `release_all` |
//! | `DaemonControl` | `crate::daemon::AppDaemonControl` |
//!
//! 关闭期的资源释放由 [`Composition::close`] 负责：它逐一释放共享句柄，再执行 `storage-sqlite` 的
//! `close()`（含 `wal_checkpoint(TRUNCATE)`）。任何仍被其他组件持有的句柄都会让 `Arc::try_unwrap`
//! 失败——那时**不静默跳过检查点**，而是上报为关闭错误。

use std::sync::Arc;
use std::time::Duration;

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::{
    Actor, AttachmentGeneration, AttachmentId, AuditAction, AuditOutcome, AuditRecord,
    CommittedEvent, DeviceId, EndpointEvent, EntityRef, NodeId, PortError, ResourceGeneration,
    ServerEpoch, SessionId, Timestamp, UnavailableKind,
};
use acp_core::ports::TrustStore;
use acp_core::ports::{
    AgentCatalog, AttachmentRef, AttachmentStore, AttachmentTransferStore, AuditStore, Clock,
    EventPublisher, IdGenerator, InteractionAuthority, LocalConfigStore, NodeEventSink,
    PruneReport, ReleaseOutcome, RetentionPolicy, SessionBackendFactory, SessionContentFacts,
    SessionDeletion, SessionFiles, SessionRelease, SessionStore,
};
use acp_core::use_cases::{LocalSessionLifecycle, UseCaseDeps, UseCases};
use agent_host::{AgentHost, HostConfig, NodeEvents};
use identity_auth::{Authority, EntropySource, IdentityKeystore, PeerPublicKey};
use identity_keystore::{EphemeralKeystore, FileKeystore, OsEntropy};
use server::local_admin::ConnectionCloser;
use server::node_link::CommandRoute;
use server::node_link::conn::ConnectionRegistry as NodeLinkConnectionRegistry;
use server::node_link::resource::NodeLinkPublisher;
use server::sync::conn::{ConnectionRegistry as SyncConnectionRegistry, close as sync_close};
use server::sync::subscribe::OwnerReachability;
use server::sync::subscribe::fanout::SyncPublisher;
use server::transport::local::{AuditHook, AuthorizationDenied};
use storage_sqlite::error::StorageError;
use storage_sqlite::session_store::SqliteStore;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::clock::{FileClock, SystemClock};
use crate::config::{Config, KeystoreChoice};
use crate::identity::{IdentityUnavailable, NodeIdentity};
use crate::session_files::NativeSessionFiles;

/// keystore 根目录（`<data_dir>/keystore`）。
const KEYSTORE_DIRECTORY: &str = "keystore";

/// 连接级拒绝审计队列的容量（有界，避免被拒绝连接造成无界内存增长）。
const AUDIT_CHANNEL_CAPACITY: usize = 64;

/// 关闭序列等待节点级事件提交排空的上限（`Composition::close` 的屏障）。
///
/// 节点级事件稀少（每个 profile 进程各一次）且提交是单条事务；正常情形下毫秒级完成。**必须有**上限：
/// `close()` 不得无限期挂住，超过上限如实上报为关闭错误（见 [`ComposeError::NodeEventsPending`]）。
const NODE_EVENT_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// 验收文件时钟路径。只在显式开发模式中生效，不是正式配置键或网络能力。
pub const TEST_CLOCK_FILE_ENV: &str = "ACPR_TEST_CLOCK_FILE";
/// 验收扫描间隔（毫秒）。只在文件时钟已启用时生效，范围 `1..=60000`。
pub const TEST_RECLAIM_INTERVAL_MS_ENV: &str = "ACPR_TEST_RECLAIM_INTERVAL_MS";

const PRODUCTION_RECLAIM_INTERVAL: Duration = Duration::from_secs(60);

#[doc(hidden)]
#[derive(Clone)]
struct SessionDeletePort {
    store: Arc<SqliteStore>,
}

#[async_trait::async_trait]
impl SessionDeletion for SessionDeletePort {
    async fn preflight(
        &self,
        write: &acp_core::model::SessionDeletionWrite,
    ) -> Result<acp_core::model::SessionDeletionEligibility, PortError> {
        self.store.preflight(write).await
    }

    async fn delete_session(
        &self,
        write: acp_core::model::SessionDeletionWrite,
    ) -> Result<acp_core::model::SessionDeletionOutcome, PortError> {
        self.store.delete_session(write).await
    }
}

/// Test composition keeps the same storage-side generation check as production.
#[doc(hidden)]
pub fn session_deletion_port(store: Arc<SqliteStore>) -> Arc<dyn SessionDeletion> {
    Arc::new(SessionDeletePort { store })
}

/// 组合根装配失败。全部失败即拒绝启动（失败关闭）。
#[derive(Debug, thiserror::Error)]
pub enum ComposeError {
    /// 打开（必要时迁移）存储失败。
    #[error("打开或迁移存储失败")]
    Store(#[source] StorageError),
    /// 组合根调用用例失败。
    #[error("本地用例失败")]
    Port(#[source] PortError),
    /// 本节点身份不可用。
    #[error("本节点身份不可用")]
    Identity(#[source] IdentityUnavailable),
    /// 平台没有可用的安全存储后端，且配置要求失败关闭。
    #[error("平台没有可用的安全存储后端")]
    KeystoreUnavailable,
    /// 关闭时仍有其它组件持有存储句柄，无法完成 `wal_checkpoint(TRUNCATE)`。
    #[error("关闭时存储仍被其他组件持有")]
    StoreStillShared,
    /// 关闭时仍有节点级事件提交未完成，存储不得在此之前关闭（否则最后一条
    /// `agent.disconnected` 会因写入撞上已关闭的存储而丢失）。
    #[error("关闭时仍有节点级事件提交未完成")]
    NodeEventsPending,
    /// 未启用开发模式却出现了验收时钟环境变量。
    #[error("the test clock is only available in explicit dev mode")]
    TestClockOutsideDevMode,
    /// 验收时钟文件不存在、不可读或内容非法。
    #[error("the test clock file is unusable")]
    TestClockInvalid,
    /// 没有启用验收时钟却设置了短扫描间隔。
    #[error("the test reclaim interval requires the test clock")]
    TestReclaimIntervalWithoutClock,
    /// 验收扫描间隔不是 `1..=60000` 毫秒。
    #[error("the test reclaim interval must be 1..=60000 milliseconds")]
    TestReclaimIntervalInvalid,
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
            Self::Port(source) => format!("a local use case failed: {}", port_error_token(source)),
            Self::Identity(_) => "this node's identity is not usable".to_owned(),
            Self::KeystoreUnavailable => "no platform keystore backend is available".to_owned(),
            Self::StoreStillShared => "the store is still shared at shutdown time".to_owned(),
            Self::NodeEventsPending => {
                "node-level event submissions did not drain before the shutdown deadline".to_owned()
            }
            Self::TestClockOutsideDevMode => {
                "the test clock is only available in explicit dev mode".to_owned()
            }
            Self::TestClockInvalid => "the test clock file is unusable".to_owned(),
            Self::TestReclaimIntervalWithoutClock => {
                "the test reclaim interval requires the test clock".to_owned()
            }
            Self::TestReclaimIntervalInvalid => {
                "the test reclaim interval must be 1..=60000 milliseconds".to_owned()
            }
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
    /// 本机会话生命周期（`core.local-session-lifecycle.v1`）：启动收敛、容量/保留回收与查看租约。
    lifecycle: Arc<LocalSessionLifecycle>,
    /// 会话文件/Git 的原生只读端口（与 `UseCases` 内部注入的是同一个实例）。
    ///
    /// 用例面不暴露端口，而连接级版本释放（`CommandRoute::on_closed`）与 daemon 关闭都要拿到同一实例，
    /// 因此组合根显式持有它：`Composition::close` 调 [`NativeSessionFiles::release_all`]，确保关闭后不残留
    /// 任何内容快照。
    session_files: Arc<NativeSessionFiles>,
    /// 节点级事件提交任务的句柄（组合根持有，与 sink 闭包共享）。
    ///
    /// `NodeEvents` 的出口是**同步**闭包，而 `Broker::commit_node_event` 是异步的：每次投递都在组合根
    /// 自己的 runtime 上 `spawn` 一个提交任务，句柄登记在这里。存在的唯一理由是**关闭顺序**——若不等待，
    /// 最后一条 `agent.disconnected`（在 `AgentHost::shutdown_all` 内投递）会与紧随其后的
    /// `Composition::close()` 竞争，存储可能在提交被 poll 到之前就关闭（见 [`Composition::close`]）。
    node_event_tasks: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    /// 生产固定 60 秒；只有显式验收文件时钟可以缩短。
    reclaim_interval: Duration,
    /// Access 侧出站运行时（按持久化 Owner 信任建立连接、claim/status 与远程路由）。
    ///
    /// 由 `crate::daemon::run` 在组合根之外装配后经 [`Composition::with_access`] 注入：本结构不发起
    /// 网络连接（`assemble` 只做本地装配），因此「启动序列里何时开始连 Owner」仍然只有一个位置。
    access: Option<Arc<crate::access::AccessManager>>,
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
        let (clock, reclaim_interval) = build_clock(&config)?;
        let started_at = clock.now();
        let store = Arc::new(
            SqliteStore::open(config.storage.clone(), &started_at)
                .await
                .map_err(ComposeError::Store)?,
        );
        if config.agents.is_empty() {
            match store.discarded_agent_profiles().await {
                Ok(count) if count > 0 => tracing::warn!(
                    event = "daemon.agents_discarded",
                    count,
                    "配置未声明任何 Agent；旧库中的 profile 已随 v10 迁移删除，请在配置的 [agent] 中重新声明"
                ),
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(event = "daemon.agents_discarded_unreadable", error = ?error)
                }
            }
        }
        // 同一个 `SqliteStore` 实例实现七个端口：每个端口一次显式向上转型（`Arc<SqliteStore>` →
        // `Arc<dyn …>`），不在字段位置依赖隐式强制转换。
        let session_store: Arc<dyn SessionStore> = store.clone();
        let trust_store: Arc<dyn TrustStore> = store.clone();
        let audit_store: Arc<dyn AuditStore> = store.clone();
        let local_config: Arc<dyn LocalConfigStore> = store.clone();
        let attachments: Arc<dyn AttachmentStore> = store.clone();
        // 分块附件与清单端口：Sync/Node Link 的独立附件连接（`server::attachment_transfer`）只经它落盘，
        // 适配器不另建临时文件（`CORE_PORTS_AND_STORAGE.md` §5.8）。
        let attachment_transfers: Arc<dyn AttachmentTransferStore> = store.clone();
        // 有效待处理交互的权威读面/仲裁端口：`session.read` 的 `pending_interactions` 与
        // `permission.resolve`/`elicitation.respond` 的版本校验都经它；不注入时 Broker 只能退回
        // 旧 peer 的按 id 路径，协商了新 feature 的客户端会收到 `internal.unavailable`。
        let interaction_authority: Arc<dyn InteractionAuthority> = store.clone();

        let entropy: Arc<dyn EntropySource> = Arc::new(OsEntropy::new());
        let keystore = build_keystore(&config, Arc::clone(&entropy))?;
        let identity = NodeIdentity::ensure(keystore.as_ref())
            .await
            .map_err(ComposeError::Identity)?;
        let ids: Arc<dyn IdGenerator> = Arc::new(UuidIdGenerator);
        let authority = Arc::new(Authority::new(
            identity.node_id().clone(),
            identity.key().clone(),
            Arc::clone(&keystore),
            Arc::clone(&entropy),
            Arc::clone(&clock),
        ));
        let host = Arc::new(
            AgentHost::new(
                Arc::new(config.agents.clone()),
                host_config(&config),
                Arc::clone(&ids),
                Arc::clone(&clock),
            )
            // 附件字节读取端口：`session.prompt` 的附件引用由 core 校验后，本层再次核对长度与摘要并从
            // 同一份存储读取（`prompt.rs`）。不注入时带附件的 prompt 会被显式拒绝。
            //
            // **必须是弱句柄**（[`WeakAttachmentStore`]）：该端口会随会话存活，而会话与 core 的会话槽位
            // 之间存在一条引用环（会话持有 core 的 `EventSink` → sink 闭包持有槽位 → 槽位缓存端点 →
            // 端点持有会话）。环在 `AgentHost::shutdown_all` 之后仍然存在，直到进程结束；注入强引用就会
            // 让关闭序列报 `StoreStillShared` 并跳过 `wal_checkpoint(TRUNCATE)`。
            .with_attachments(Arc::new(WeakAttachmentStore::new(&store))),
        );
        let backends: Arc<dyn SessionBackendFactory> = host.clone();
        let catalog: Arc<dyn AgentCatalog> = host.clone();
        // 生命周期与 broker 共用同一个发布端口/存储/时钟：会话活动事件必须先落库再广播。
        let lifecycle_store = Arc::clone(&session_store);
        let lifecycle_content = Arc::clone(&store) as Arc<dyn SessionContentFacts>;
        let lifecycle_publisher = Arc::clone(&publisher);
        let broker = Arc::new(
            Broker::new(
                BrokerDeps {
                    store: Arc::clone(&session_store),
                    backends,
                    trust: Arc::clone(&trust_store),
                    publisher,
                    clock: Arc::clone(&clock),
                    ids: Arc::clone(&ids),
                    audit: Some(Arc::clone(&audit_store)),
                },
                BrokerConfig {
                    persist_deltas: config.storage.persist_deltas,
                    // `sessions.*`（`CONFIG_REFERENCE.md` §4）：排队策略与上限由配置决定，默认同 v1
                    // （`queue` / 16，与 `SYNC_PROTOCOL.md` §11.6 的二选一一致）。
                    queue_policy: config.sessions.queue_policy,
                    max_queued_turns: config.sessions.max_queued_turns,
                },
            )
            .with_interaction_authority(interaction_authority),
        );
        let session_files = Arc::new(NativeSessionFiles::new());
        let lifecycle = Arc::new(LocalSessionLifecycle::new(
            Arc::clone(&clock),
            lifecycle_store,
            lifecycle_content,
            lifecycle_publisher,
            config.sessions.lifecycle,
        ));
        lifecycle.set_session_release(Arc::new(HostSessionRelease {
            host: Arc::clone(&host),
        }));
        let use_cases = Arc::new(
            UseCases::new(UseCaseDeps {
                broker: Arc::clone(&broker),
                store: session_store,
                trust: trust_store,
                audit: audit_store,
                config: local_config,
                attachments,
                catalog,
                clock: Arc::clone(&clock),
                ids: Arc::clone(&ids),
            })
            .with_session_files(Arc::clone(&session_files) as Arc<dyn SessionFiles>)
            .with_attachment_transfers(attachment_transfers)
            .with_session_content(
                Arc::clone(&store) as Arc<dyn acp_core::ports::SessionContentFacts>
            )
            .with_session_deletion(session_deletion_port(Arc::clone(&store))),
        );
        // `session.create` 与 `session.resume` 共用同一本容量账：broker 在分配会话标识之后、启动后端
        // 之前预占，失败交还、落账后 settle 为 ready。
        assert!(
            broker.set_lifecycle(Arc::clone(&lifecycle)),
            "本机生命周期只接线一次"
        );

        // 节点级事件缝（`design.md` D6、`core-derived-events` R8）：把 WP4 的 `NodeEvents` 接到 WP3 的
        // `Broker::commit_node_event`。这是本文件承接的**规划缺口收尾**——该缝在 `plan.md` 没有所有者行。
        //
        // 为什么用 `set_node_events` 而不是 `with_node_events`：`with_node_events` 消耗 `self` 且必须在
        // `AgentHost::new` **当时**接，而这里的依赖方向相反——`Broker` 需要 `host`（作为
        // `SessionBackendFactory`），`node_sink` 闭包又需要 `Broker`，三者构成
        // `host → sink → broker → backends → host` 的环。唯一的解法是先建 `host`、再建 `broker`，
        // 然后**原地**接线。`set_node_events` 是 `&self`，经 `Arc<AgentHost>` 解引用即可调用。
        //
        // 为什么在这里（`assemble` 末尾）是安全的：本函数**不**打开 endpoint、**不** spawn Agent——
        // `AgentHost::ensure_runtime` 只在 `SessionBackendFactory::create`/`resume` 或
        // `AgentCatalog::agent_capabilities` 被调用时才由 `Broker::endpoint_for` 惰性触发，而那些调用点
        // 全部在 `assemble` 返回之后（`daemon` 的接入层与管理面）。因此本进程的第一次 `agent.connected`
        // 必然发生在接线完成之后，不会被漏掉（`NodeEvents` 不回填已发生的事件）。
        //
        // 桥接（同步 → 异步）：`NodeEventSink` 的闭包是同步的，`commit_node_event` 是异步的，而 core 不依赖
        // runtime。组合根本就持有 runtime，因此闭包用 `tokio::runtime::Handle::current()`（`assemble` 是
        // `async fn`，调用点必然在 runtime 上下文内）`spawn` 一个提交任务。**不用 `Handle::block_on`**：
        // 闭包会在 async worker 上下文里被调用（进程读循环的同步钩子路径），在那里 `block_on` 会 panic。
        //
        // 提交任务**不是**游离的：句柄登记进 `node_event_tasks`，由 [`Composition::close`] 在关闭存储
        // **之前**等待（带 5 s 上限）——否则最后一条 `agent.disconnected` 会撞上已关闭的存储。
        let node_event_tasks = Arc::new(std::sync::Mutex::new(Vec::new()));
        let node_handle = tokio::runtime::Handle::current();
        host.set_node_events(NodeEvents::new(node_event_sink(
            Arc::downgrade(&broker),
            node_handle,
            Arc::clone(&node_event_tasks),
        )));

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
            lifecycle,
            session_files,
            node_event_tasks,
            reclaim_interval,
            access: None,
        })
    }

    /// 启动配置。
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// 注入 Access 侧出站运行时（装配顺序见 [`crate::daemon::run`]）。
    ///
    /// 同时把同一实例接成 core 的 imported 读取端口：`session.read` 回源的唯一实现就在
    /// `AccessManager`（它持有活的 Owner link），组合根只做接线，不参与正文处理。
    pub fn with_access(mut self, access: Arc<crate::access::AccessManager>) -> Self {
        // 四个运行期远程面共用同一个 `RemoteRuntime` 实例：目录、回源读取、命令查询与标题转发必须
        // 看到同一份代际/连接事实。Access 只转发 `session.title.set`，不写标题/命令/收据（D3）。
        self.use_cases
            .set_remote_runtime_directory(access.runtime());
        self.use_cases.set_remote_source_reads(access.runtime());
        self.use_cases.set_remote_command_status(access.runtime());
        self.use_cases.set_remote_title_write(access.runtime());
        self.access = Some(access);
        self
    }

    /// Access 侧出站运行时（`None` = 本进程没有装配出站适配器，例如只服务入站面的测试组合根）。
    pub fn access(&self) -> Option<&Arc<crate::access::AccessManager>> {
        self.access.as_ref()
    }

    /// 启动清理门禁（§7.2）：迁移成功后、开始监听前执行一次；失败即拒绝启动，重开可安全重试。
    pub async fn run_startup_gate(&self) -> Result<bool, ComposeError> {
        self.store
            .run_startup_gate()
            .await
            .map_err(ComposeError::Store)
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

    /// 本机会话生命周期（组合根唯一实例；`server::sync` 与后台回收任务共用）。
    pub fn lifecycle(&self) -> &Arc<LocalSessionLifecycle> {
        &self.lifecycle
    }

    /// 会话文件/Git 的原生只读端口（与 [`Composition::use_cases`] 注入的是同一个实例）。
    ///
    /// 组合根把它同时交给 `UseCases::with_session_files` 与 Sync 命令面；daemon 关闭时
    /// [`Composition::close`] 用它释放全部连接期内容快照。
    pub fn session_files(&self) -> &Arc<NativeSessionFiles> {
        &self.session_files
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

    /// 生命周期扫描间隔。生产恒为 60 秒，验收文件时钟可显式缩短。
    pub fn reclaim_interval(&self) -> Duration {
        self.reclaim_interval
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

    /// 关闭存储：释放本对象持有的共享句柄后执行 `SqliteStore::close()`（含 `wal_checkpoint(TRUNCATE)`）。
    ///
    /// **调用方必须先释放其它持有者**（连接处理器、`DaemonControl`、周期任务、审计写任务、Agent 宿主）：
    /// 否则 `Arc::try_unwrap` 失败，本方法上报 [`ComposeError::StoreStillShared`] 并**不**假装已检查点。
    ///
    /// **先排空节点级事件提交**：`agent.disconnected` 的最后一次投递发生在 `AgentHost::shutdown_all`
    /// 内部，其提交任务由 [`node_event_sink`] spawn。若不等这些任务完成就关池，最后一条断开事件会因写入
    /// 撞上已关闭的存储而丢失（R8 退化为一条日志）。等待带 5 s 上限；超时**如实上报**
    /// [`ComposeError::NodeEventsPending`] 而不是静默继续。
    pub async fn close(self) -> Result<(), ComposeError> {
        let Self {
            store,
            use_cases,
            lifecycle,
            session_files,
            host,
            broker,
            authority,
            keystore,
            clock,
            ids,
            identity,
            config,
            node_event_tasks,
            access,
            ..
        } = self;
        // Access 出站连接：先停 supervisor（等待其结束，无 detached task），再释放它对存储的句柄。
        // 这一步必须在 `Arc::try_unwrap(store)` 之前——`access` 持有 `UseCases` 与 `RemoteDeliveryStore`
        // 的强引用。
        if let Some(access) = access {
            access.shutdown_all().await;
            drop(access);
        }
        // 内容快照只存在于内存；连接断开的逐版本释放由 Sync 命令面完成，这里兜底释放全部（daemon 关闭）。
        // 必须在关闭存储之前：`release_all` 不触碰存储，但关闭顺序上它属于「先释放资源」的一段。
        session_files.release_all();
        // 显式释放：`UseCases`/`Broker`/`AgentHost` 各自持有存储句柄（`Arc<dyn …>` 克隆）。
        drop(use_cases);
        drop(lifecycle);
        drop(broker);
        drop(host);
        drop(authority);
        drop(keystore);
        drop(clock);
        drop(ids);
        drop(identity);
        drop(config);
        // 关闭前的屏障：等已登记的节点级提交任务结束（sink 闭包仍可能登记新任务，因此循环取用，
        // 直到「一轮下来没有新增且未结束的任务」或超时）。
        let deadline = std::time::Instant::now() + NODE_EVENT_DRAIN_TIMEOUT;
        loop {
            let pending: Vec<tokio::task::JoinHandle<()>> = {
                let mut tasks = node_event_tasks
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                tasks.retain(|task| !task.is_finished());
                // 取出再等待：不必持锁跨越 `await`（否则 sink 闭包会阻塞）。
                std::mem::take(&mut *tasks)
            };
            if pending.is_empty() {
                break;
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero()
                || tokio::time::timeout(remaining, futures_join(pending))
                    .await
                    .is_err()
            {
                return Err(ComposeError::NodeEventsPending);
            }
        }
        match Arc::try_unwrap(store) {
            Ok(store) => {
                store.close().await;
                Ok(())
            }
            Err(_) => Err(ComposeError::StoreStillShared),
        }
    }
}

/// 等待全部句柄结束（`JoinHandle` 没有 `futures` 依赖，这里就地串起来）。
async fn futures_join(handles: Vec<tokio::task::JoinHandle<()>>) {
    for handle in handles {
        let _ = handle.await;
    }
}

/// 注入给 `agent-host` 的附件读取端口：**非拥有**句柄（`Weak`）。
///
/// 由 [`Composition::assemble`] 构造并交给 [`AgentHost::with_attachments`]。它只把五个
/// [`AttachmentStore`] 方法转发给同一份 SQLite 存储，自身不缓存字节、也不保留存储的强引用。
///
/// **为什么必须是 `Weak`**：`AgentHost` 把该端口交给每个会话，而会话与 core 的会话槽位之间存在一条
/// 引用环——会话持有 core 给的 `EventSink`，sink 闭包持有 `Arc<Slot>`，槽位缓存
/// `Arc<dyn SessionEndpoint>`，端点又持有会话。`AgentHost::shutdown_all` 只清运行目录与任务，环本身留到
/// 进程结束；若注入强引用，[`Composition::close`] 的 `Arc::try_unwrap` 就会失败并上报
/// [`ComposeError::StoreStillShared`]，跳过 `wal_checkpoint(TRUNCATE)`。弱句柄让泄漏的环不再钉住存储：
/// 存储仍活着时端口行为完全相同；存储已关闭时如实返回 `Unavailable(IoError)`，不静默返回空字节。
struct WeakAttachmentStore {
    store: std::sync::Weak<SqliteStore>,
}

impl WeakAttachmentStore {
    fn new(store: &Arc<SqliteStore>) -> Self {
        Self {
            store: Arc::downgrade(store),
        }
    }

    /// 升级到存储句柄。存储已关闭（组合根关闭序列）时返回明确的不可用，而不是 panic。
    fn upgrade(&self) -> Result<Arc<SqliteStore>, PortError> {
        self.store
            .upgrade()
            .ok_or(PortError::Unavailable(UnavailableKind::IoError))
    }
}

#[async_trait::async_trait]
impl AttachmentStore for WeakAttachmentStore {
    async fn put(
        &self,
        bytes: &[u8],
        media_type: &str,
        at: Timestamp,
    ) -> Result<AttachmentRef, PortError> {
        self.upgrade()?.put(bytes, media_type, at).await
    }

    async fn get(&self, id: &AttachmentId) -> Result<Option<Vec<u8>>, PortError> {
        self.upgrade()?.get(id).await
    }

    async fn link(
        &self,
        session: &SessionId,
        attachment: &AttachmentId,
        generation: AttachmentGeneration,
        at: Timestamp,
    ) -> Result<(), PortError> {
        self.upgrade()?
            .link(session, attachment, generation, at)
            .await
    }

    async fn prune_lru(&self, budget_bytes: u64, at: Timestamp) -> Result<PruneReport, PortError> {
        self.upgrade()?.prune_lru(budget_bytes, at).await
    }

    async fn sweep_orphans(&self, at: Timestamp, limit: u32) -> Result<u32, PortError> {
        self.upgrade()?.sweep_orphans(at, limit).await
    }
}

/// 选择进程时钟与扫描间隔。
///
/// 文件时钟和短间隔都只在 `dev_mode.enabled = true` 且显式设置
/// [`TEST_CLOCK_FILE_ENV`] 时启用。生产配置即使继承了同名环境变量，也会在未启用
/// `dev_mode` 时失败关闭；普通生产路径始终使用 [`SystemClock`] 和固定 60 秒扫描。
fn build_clock(config: &Config) -> Result<(Arc<dyn Clock>, Duration), ComposeError> {
    build_clock_with(
        config.dev_mode.enabled,
        std::env::var_os(TEST_CLOCK_FILE_ENV),
        std::env::var_os(TEST_RECLAIM_INTERVAL_MS_ENV),
    )
}

fn build_clock_with(
    dev_mode: bool,
    test_clock_file: Option<std::ffi::OsString>,
    test_interval: Option<std::ffi::OsString>,
) -> Result<(Arc<dyn Clock>, Duration), ComposeError> {
    let dev_test_clock = dev_mode && test_clock_file.is_some();

    if test_clock_file.is_some() && !dev_mode {
        return Err(ComposeError::TestClockOutsideDevMode);
    }
    if test_interval.is_some() && !dev_test_clock {
        return Err(ComposeError::TestReclaimIntervalWithoutClock);
    }

    let Some(path) = test_clock_file else {
        return Ok((
            Arc::new(SystemClock::new()) as Arc<dyn Clock>,
            PRODUCTION_RECLAIM_INTERVAL,
        ));
    };
    if path.is_empty() {
        return Err(ComposeError::TestClockInvalid);
    }
    let clock = FileClock::open(path).map_err(|_| ComposeError::TestClockInvalid)?;
    let interval = match test_interval {
        None => PRODUCTION_RECLAIM_INTERVAL,
        Some(value) => value
            .to_str()
            .and_then(|text| text.parse::<u64>().ok())
            .filter(|millis| (1..=60_000).contains(millis))
            .map(Duration::from_millis)
            .ok_or(ComposeError::TestReclaimIntervalInvalid)?,
    };
    Ok((Arc::new(clock), interval))
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
    session_id -> acp_core::model::SessionId,
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
    fn publish(&self, event: CommittedEvent) {
        tracing::trace!(
            event = "daemon.published",
            event_id = %event.id.as_str(),
            global_sequence = event.global_sequence.get(),
            "本切片没有事件订阅者（server::sync/node_link 未落地）"
        );
    }
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

/// `ConnectionCloser`：授权变化提交后关闭该设备/节点的 active connection 并通知 Export 撤销
/// （`design.md` D7）。
///
/// `local_admin` 已经在**持久提交成功之后**才调本实现（`local_admin::pairing`），因此这里只做「通知与关闭」：
/// 推送/关闭失败只记日志，不回滚已提交的授权变化。授权判定不依赖推送——`node_link` 在处理 `resource.attach`、
/// `command.submit` 与 catalog 订阅时按当次持久化记录复核（D7）。
///
/// `close_device` 关的是 **Sync 的**连接表（`server::sync::conn::ConnectionRegistry`，键是
/// `(hostId, deviceId)`）：设备只经 Sync 面连接本机，因此撤销设备只需要那一个注册表。关闭是「请求关闭」
/// 语义（先排空已入队消息再发 close 帧，`4410`），返回被请求关闭的连接数——注册表不是事实来源，
/// 因此关闭失败与「没有连接」都不回滚已提交的撤销。
///
/// `close_node` 与 `close_node_after_reauth` 走**两条不同**的 node_link 路径：前者是撤销（推
/// `node.trust.revoked` + 4410），后者是重新配对收窄授权（不推消息 + 1000），不能互换。
#[derive(Debug)]
pub struct NodeLinkCloser {
    command: Arc<CommandRoute>,
    /// Sync 的活跃连接表（登记者是 `server::sync` 的会话；组合根在 [`crate::daemon`] 里交给本实现）。
    sync: Arc<SyncConnectionRegistry>,
    /// Access 侧出站运行时：`node.revoke` 提交后停止重连并取消远程路由。
    access: Option<Arc<crate::access::AccessManager>>,
}

impl NodeLinkCloser {
    /// 装配：`command` 是本机（Owner）的命令管线，它内部持有连接注册表与 resource 路由；`sync` 是
    /// Sync 入站面的连接注册表（设备连接只登记在那里）。
    pub fn new(command: Arc<CommandRoute>, sync: Arc<SyncConnectionRegistry>) -> Self {
        Self {
            command,
            sync,
            access: None,
        }
    }

    /// 注入 Access 侧出站运行时（同一节点的两种角色可以同时存在，撤销必须两条腿都停）。
    pub fn with_access(mut self, access: Arc<crate::access::AccessManager>) -> Self {
        self.access = Some(access);
        self
    }
}

#[async_trait::async_trait]
impl ConnectionCloser for NodeLinkCloser {
    async fn close_device(&self, device: &DeviceId) {
        // `4410` 是 §12.3 的「设备已撤销」；reason 是固定短文本（进 close 帧，不插值请求内容）。
        let closed = self
            .sync
            .close_device(device, sync_close::REVOKED, "the device was revoked");
        tracing::info!(
            event = "daemon.revocation_connection_sweep",
            target_kind = "device",
            target_id = %device.as_str(),
            closed_connections = u64::try_from(closed).unwrap_or(u64::MAX),
            "撤销已提交：已请求以 4410 关闭该设备的全部 Sync 连接"
        );
    }

    async fn close_node(&self, node: &NodeId) {
        let closed = self.command.node_revoked(node).await;
        // 出站腿：撤销后不再重连，并让远程路由（惰性构造自 link 表）随之失效。
        if let Some(access) = self.access.as_ref() {
            access.revoke(node).await;
        }
        tracing::info!(
            event = "daemon.revocation_connection_sweep",
            target_kind = "node",
            target_id = %node.as_str(),
            closed_connections = u64::try_from(closed).unwrap_or(u64::MAX),
            "撤销已提交：已推送 node.trust.revoked 并以 4410 关闭该节点的连接"
        );
    }

    async fn close_node_after_reauth(&self, node: &NodeId) {
        let closed = self.command.node_reauth(node).await;
        tracing::info!(
            event = "daemon.reauth_connection_sweep",
            target_kind = "node",
            target_id = %node.as_str(),
            closed_connections = u64::try_from(closed).unwrap_or(u64::MAX),
            "重新配对：已关闭既有连接，等待对端重连取新 catalog"
        );
    }
}

/// `OwnerReachability`：Sync 面的 Owner 可达性（`sync_visible_sessions` 与快照里 `origin.online` 的输入）。
///
/// 组合根是唯一同时看得见两边连接表的地方，因此这条运行时事实在这里注入：某 node 可达 ⇔ 本机有一条来自
/// 该 node id 的**活跃** Node Link 连接（那份注册表就是该事实的拥有者，见 `server::node_link::conn`）。
/// 两侧互不调用（`AGENTS.md` §5 的平级约束），本实现只做一次只读查询、不建第二份在线缓存：Node Link
/// 连接结束时它的句柄从注册表移除，下一次查询即为不可达。
pub struct NodeLinkReachability {
    connections: Arc<NodeLinkConnectionRegistry>,
}

impl NodeLinkReachability {
    /// 装配：`connections` 是 Node Link 入站面的连接注册表（组合根从 `NodeLinkIngress` 取得）。
    pub fn new(connections: Arc<NodeLinkConnectionRegistry>) -> Self {
        Self { connections }
    }
}

impl OwnerReachability for NodeLinkReachability {
    fn reachable(&self, owner: &NodeId) -> bool {
        !self.connections.handles_for_node(owner).is_empty()
    }
}

/// `EventPublisher` 分叉（`design.md` D6）：一路保留组合根的结构化日志（[`LoggingPublisher`] 的既有语义），
/// 一路把同一个已提交投递转发给 `server::node_link` 的事件扇出，一路转发给 `server::sync` 的实时扇出
/// （`sync.subscribe` 的 `event` 帧实时腿；重放腿在订阅面的 `advance` 里，由客户端消息驱动）。
///
/// 分叉点在 `app`（组合根）而不在 `server`：两个入站适配器都不需要知道其他消费方。`publish` 仍是同步
/// 且非阻塞的（各扇出侧队列满只丢这一条并记日志，已提交的事务不回滚，见 D6 的既定取舍）。
struct ForkedPublisher {
    node_link: NodeLinkPublisher,
    sync: Option<SyncPublisher>,
}

impl EventPublisher for ForkedPublisher {
    fn publish(&self, event: CommittedEvent) {
        LoggingPublisher.publish(event.clone());
        self.node_link.publish(event.clone());
        if let Some(sync) = self.sync.as_ref() {
            sync.publish(event);
        }
    }
}

/// 节点级事件的同步出口 → 异步提交的桥（`Composition::assemble` 的接线用）。
///
/// `NodeEventSink::new` 的闭包是同步的（core 的 `NodeEvents::send` 在进程读循环的同步钩子路径上调用它），
/// 而 `Broker::commit_node_event` 是异步的。本函数把每条事件 `spawn` 到组合根自己的 runtime 上提交，
/// 并把任务句柄登记进 `tasks`，供 [`Composition::close`] 在关闭存储前等待。
///
/// **持 `Weak<Broker>` 而非 `Arc`**：`AgentHost` 持有 `NodeEvents`（⇒ 本闭包），而 `Broker` 持有
/// `AgentHost`（作为 `SessionBackendFactory`）——若本闭包再持 `Arc<Broker>`，就构成
/// `host → NodeEvents → Broker → host` 的强引用环。环会让 `AgentHost` 与 `Broker` 永不析构，
/// 进而让它们各自持有的存储句柄永不释放，`Composition::close` 会（**正确地**）以
/// [`ComposeError::StoreStillShared`] 拒绝检查点。`Weak` 断开该环。
///
/// 失败**不静默**：`commit_node_event` 的 `Err` 一律记一条 `error` 级日志（`agent.connected`/
/// `agent.disconnected` 未落库，R8 不成立），绝不用 `let _ =` 吞掉——那会让「事件没有落库」重新变成
/// 不可观测。任务句柄被 `JoinHandle::is_finished` 回收，不随进程生命周期无界增长（节点级事件每个
/// profile 进程各一次，量极小）。
fn node_event_sink(
    broker: std::sync::Weak<Broker>,
    runtime: tokio::runtime::Handle,
    tasks: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
) -> NodeEventSink {
    NodeEventSink::new(move |event: EndpointEvent| {
        let Some(broker) = broker.upgrade() else {
            // 组合根正在关闭/已释放：此时没有需要落库的提交通道，如实告警（不静默吞掉）。
            // 正常关闭序列里 Agent 先于 `Broker` 释放，断开事件在那之前已经提交（见 `close` 的屏障）。
            tracing::warn!(
                event = "daemon.node_event_commit_skipped",
                event_type = event.event_type.as_str(),
                "组合根已释放，节点级事件无法提交"
            );
            return;
        };
        let handle = runtime.spawn(async move {
            if let Err(error) = broker.commit_node_event(event).await {
                // 含内层原因的稳定 token 交 `port_error_token`，不回显可能含 SQL/路径的内层文本（§14.1）。
                tracing::error!(
                    event = "daemon.node_event_commit_failed",
                    reason = port_error_token(&error),
                    "节点级事件提交失败：agent.connected/agent.disconnected 未落库（R8）"
                );
            }
        });
        // 先回收已结束的句柄（避免长跑进程里句柄列表无界增长），再登记本次的。
        let mut tasks = tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        tasks.retain(|task| !task.is_finished());
        tasks.push(handle);
    })
}

/// 装配 broker 的发布端口与它的事件队列（组合根在 [`Composition::assemble`] 之前调用）。
///
/// 返回的接收端交给 `server::node_link::ResourceRoute::dispatch`（由 `crate::daemon` spawn 并纳入关闭
/// 序列）；Sync 的发布端经 [`forked_publisher_with_sync`] 安装。两者必须同时存在，否则事件会在扇出
/// 消费者出现之前被丢弃。
pub fn forked_publisher() -> (Arc<dyn EventPublisher>, mpsc::Receiver<CommittedEvent>) {
    forked_publisher_with_sync(None)
}

/// 同 [`forked_publisher`]，但把 Sync 的发布端一并装进分叉。
///
/// 组合根（[`crate::daemon::NetIngress::start`] 的调用方）必须先建好 Sync 的通道，再把发布端交进来：
/// `server::sync` 不得调用平级的 `server::node_link`，因此这条缝合只能发生在 app 侧。
///
/// 返回的仍是 [`EventPublisher`] 端口而不是分叉结构本身：调用方只需要「发布 + 队列」这一对口，
/// 分叉的内部形状留在本模块（结构体因此保持私有）。
pub fn forked_publisher_with_sync(
    sync: Option<SyncPublisher>,
) -> (Arc<dyn EventPublisher>, mpsc::Receiver<CommittedEvent>) {
    let (node_link, queue) = NodeLinkPublisher::channel();
    (Arc::new(ForkedPublisher { node_link, sync }), queue)
}

/// `agent-host` 的运行配置。
///
/// `idle_timeout` **恒为 `None`**：旧的进程级空闲回收按 ACP 活动判定、看不到查看者租约，会把正在被
/// 别的设备查看/恢复的受保护会话当空闲杀掉。保留期与逐会话释放由 `LocalSessionLifecycle` 负责
/// （租约/任务/交互保护都在 core）；停整个共享进程只允许在 lifecycle 证明全部会话已释放后执行，
/// 本切片不提供该路径。`sessions.idle_timeout_ms` 仍由 lifecycle 读取（显式 `0` 只关闭按时间回收）。
fn host_config(config: &Config) -> HostConfig {
    HostConfig {
        idle_timeout: None,
        client_capabilities: agent_host::config::ClientCapabilities {
            terminal: Some(true),
            ..agent_host::config::ClientCapabilities::none()
        },
        clean_shutdown_fence: Some(config.data_dir.join("agent-host.clean-shutdown.fence")),
        ..HostConfig::default()
    }
}

/// `SessionRelease` 的组合根适配器：把 `LocalSessionLifecycle` 的逐会话释放接到
/// [`AgentHost::release_session_at_generation`]（真实 ACP `session/close`）。
struct HostSessionRelease {
    host: Arc<AgentHost>,
}

#[async_trait::async_trait]
impl SessionRelease for HostSessionRelease {
    async fn release_session(
        &self,
        session: SessionId,
        generation: ResourceGeneration,
        _now: Timestamp,
    ) -> Result<ReleaseOutcome, PortError> {
        let outcome = self
            .host
            .release_session_at_generation(&session, generation.get())
            .await;
        Ok(match outcome {
            agent_host::release::ReleaseOutcome::Released => ReleaseOutcome::Released,
            agent_host::release::ReleaseOutcome::Failed => ReleaseOutcome::Failed,
            agent_host::release::ReleaseOutcome::Uncertain => ReleaseOutcome::Uncertain,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acp_core::model::{EventId, EventType, Sequence, SessionId};

    /// 固定的 canonical 时间戳（扇出分叉不读它，只为满足记录形状）。
    fn timestamp(text: &str) -> Timestamp {
        Timestamp::new(text).expect("canonical timestamp")
    }

    /// 已提交投递的最小构造（只填扇出分叉会读到的字段）。
    fn owned_delivery(sequence: u64) -> CommittedEvent {
        CommittedEvent {
            id: EventId::new("04dbad03-8869-4c5a-94fa-2f3af2fb622b").expect("event id"),
            event_type: EventType::new("turn.queued").expect("event type"),
            session: Some(
                SessionId::new("04dbad03-8869-4c5a-94fa-2f3af2fb622c").expect("session id"),
            ),
            session_sequence: Some(Sequence::new(1).expect("session sequence")),
            global_sequence: Sequence::new(sequence).expect("global sequence"),
            origin_epoch: None,
            origin_sequence: None,
            created_at: timestamp("2026-09-17T12:10:00.123Z"),
        }
    }

    /// 生命周期接线后，`sessions.idle_timeout_ms` **不再**交给旧的进程级 idle sweep：该 sweep 只看 ACP
    /// 活动，会把正在被查看/恢复的受保护会话杀掉。配置值仍然进入 lifecycle（按时间回收仍可用）。
    #[test]
    fn the_legacy_idle_sweep_no_longer_receives_the_session_idle_timeout() {
        let config =
            Config::from_toml("[sessions]\nidle_timeout_ms = 1500\nmax_active_sessions = 7\n")
                .expect("测试配置合法");
        assert_eq!(
            config.sessions.idle_timeout,
            Some(std::time::Duration::from_millis(1500)),
            "配置解析保持旧口径（值仍然被接受）"
        );
        let host = host_config(&config);
        assert!(
            host.idle_timeout.is_none(),
            "旧的进程级 sweep 必须被关闭，否则会杀受保护会话"
        );
        assert_eq!(
            config.sessions.lifecycle.idle_timeout_ms(),
            1500,
            "保留期 / 逐会话回收仍读取该值"
        );
        assert_eq!(config.sessions.lifecycle.max_active_sessions(), 7);
    }

    #[test]
    fn test_clock_environment_fails_closed_outside_dev_mode_and_without_a_clock() {
        assert!(matches!(
            build_clock_with(false, Some("clock".into()), None),
            Err(ComposeError::TestClockOutsideDevMode)
        ));
        assert!(matches!(
            build_clock_with(true, None, Some("5".into())),
            Err(ComposeError::TestReclaimIntervalWithoutClock)
        ));

        let dir = TempDir::new("clock-selection");
        let path = dir.path.join("clock");
        std::fs::write(&path, "2026-10-26T00:30:00.000Z").expect("clock file");
        assert!(matches!(
            build_clock_with(true, Some(path.as_os_str().to_owned()), Some("0".into())),
            Err(ComposeError::TestReclaimIntervalInvalid)
        ));
    }

    #[test]
    fn test_clock_environment_rejects_missing_and_invalid_files() {
        let dir = TempDir::new("clock-files");
        let missing = dir.path.join("missing");
        assert!(matches!(
            build_clock_with(true, Some(missing.into_os_string()), None),
            Err(ComposeError::TestClockInvalid)
        ));

        let invalid = dir.path.join("invalid");
        std::fs::write(&invalid, "not-a-timestamp").expect("invalid clock file");
        assert!(matches!(
            build_clock_with(true, Some(invalid.into_os_string()), None),
            Err(ComposeError::TestClockInvalid)
        ));
    }

    #[test]
    fn test_clock_interval_accepts_only_the_documented_bounds() {
        let dir = TempDir::new("clock-interval");
        let path = dir.path.join("clock");
        std::fs::write(&path, "2026-10-26T00:30:00.000Z").expect("clock file");

        for (raw, expected) in [
            (Some("1".into()), Duration::from_millis(1)),
            (None, PRODUCTION_RECLAIM_INTERVAL),
            (Some("60000".into()), Duration::from_secs(60)),
        ] {
            let (clock, interval) = build_clock_with(true, Some(path.as_os_str().to_owned()), raw)
                .expect("bounded interval");
            assert_eq!(clock.now().as_str(), "2026-10-26T00:30:00.000Z");
            assert_eq!(interval, expected);
        }

        for raw in ["0", "60001", "not-a-number"] {
            assert!(matches!(
                build_clock_with(true, Some(path.as_os_str().to_owned()), Some(raw.into())),
                Err(ComposeError::TestReclaimIntervalInvalid)
            ));
        }
    }

    #[test]
    fn production_clock_selection_ignores_test_clock_inputs() {
        let (clock, interval) =
            build_clock_with(false, None, None).expect("production clock selection");
        assert_eq!(interval, PRODUCTION_RECLAIM_INTERVAL);
        assert!(
            clock.now().as_str() >= "2024-01-01T00:00:00.000Z",
            "the production path must use the system clock"
        );

        let (dev_clock, dev_interval) =
            build_clock_with(true, None, None).expect("dev mode without a test clock");
        assert_eq!(dev_interval, PRODUCTION_RECLAIM_INTERVAL);
        assert!(
            dev_clock.now().as_str() >= "2024-01-01T00:00:00.000Z",
            "dev mode without ACPR_TEST_CLOCK_FILE must keep the system clock"
        );
    }

    /// `ForkedPublisher` 必须把同一条已提交投递**同时**分叉给 Node Link 与 Sync 两路。
    ///
    /// 分叉点是组合根而不是 `server`（D6）：两路都拿到事件才可能各自投递。判据来自任务 6.2
    /// 「`EventPublisher` 增加 Sync 扇出分支」——只接一路会让另一路的实时腿静默失效。
    #[test]
    fn forked_publisher_delivers_the_same_event_to_both_ingress_adapters() {
        let mut sync_publisher = SyncPublisher::channel();
        let mut sync_queue = sync_publisher
            .take_receiver()
            .expect("新建发布端必然还有接收端");
        let (node_link, mut node_link_queue) = NodeLinkPublisher::channel();
        let forked = ForkedPublisher {
            node_link,
            sync: Some(sync_publisher),
        };

        let delivery = owned_delivery(7);
        forked.publish(delivery.clone());

        assert!(
            node_link_queue.try_recv().is_ok(),
            "Node Link 的实时腿必须收到同一条已提交投递"
        );
        assert!(
            sync_queue.try_recv().is_ok(),
            "Sync 的实时腿必须收到同一条已提交投递（6.2 的新增分支）"
        );
    }

    /// 两路**都没有**接入时（`sync: None`），分叉不得 panic 也不得阻塞调用方。
    ///
    /// 这是组合根装配尚未提供 Sync 通道时的中间态：Node Link 侧必须照常工作，而 `publish` 仍同步返回。
    #[test]
    fn forked_publisher_without_a_sync_branch_keeps_the_node_link_leg() {
        let (node_link, mut node_link_queue) = NodeLinkPublisher::channel();
        let forked = ForkedPublisher {
            node_link,
            sync: None,
        };

        forked.publish(owned_delivery(8));

        assert!(
            node_link_queue.try_recv().is_ok(),
            "没有 Sync 分支时 Node Link 的实时腿必须不受影响"
        );
    }

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

    /// 组合根把同一个 `NativeSessionFiles` 交给用例面与 Sync 命令面，并在 daemon 关闭路径释放全部
    /// 连接期内容快照（WP3 3.2）。
    #[tokio::test]
    async fn daemon_close_releases_all_content_snapshots() {
        let dir = TempDir::new("session-files");
        let workspace = dir.path.join("workspace");
        std::fs::create_dir_all(&workspace).expect("workspace");
        std::fs::write(workspace.join("note.txt"), b"hello").expect("file");
        let composition =
            Composition::assemble(dev_config(&dir.path, ""), Arc::new(LoggingPublisher))
                .await
                .expect("装配");
        // 通过组合根持有的那个实例建立快照：`release_all` 若作用在另一份实例上，计数不会归零。
        let files = Arc::clone(composition.session_files());
        let path = acp_core::model::SessionFilePath::parse("note.txt").expect("path");
        SessionFiles::open_content(
            files.as_ref(),
            workspace.to_str().expect("utf-8 workspace"),
            &path,
            acp_core::model::FileReadPurpose::Preview,
        )
        .await
        .expect("open snapshot");
        assert_eq!(files.snapshot_count(), 1);
        composition.close().await.expect("关闭组合根");
        assert_eq!(
            files.snapshot_count(),
            0,
            "daemon 关闭路径必须调用 release_all 释放内容快照"
        );
    }

    /// 组合根把同一个 SQLite 存储装配成附件传输端口：没有它，`/sync/v1/attachments` 与
    /// `/node-link/v1/attachments` 不会注册、对应 feature 也不会宣告。
    #[tokio::test]
    async fn composition_injects_the_attachment_transfer_store() {
        let dir = TempDir::new("attachments");
        let composition =
            Composition::assemble(dev_config(&dir.path, ""), Arc::new(LoggingPublisher))
                .await
                .expect("装配");
        assert!(
            composition.use_cases().attachment_transfers().is_some(),
            "组合根必须注入 AttachmentTransferStore"
        );
        composition.close().await.expect("关闭组合根");
    }

    /// 回归（WP5c）：注入给 `AgentHost` 的附件端口必须**不拥有**存储。
    ///
    /// 会话与 core 的会话槽位之间的引用环保留到进程结束（见 [`WeakAttachmentStore`]），因此端口若持有
    /// 强引用，`close()` 就会以 [`ComposeError::StoreStillShared`] 失败并跳过检查点——`node_link_e2e`
    /// 的 AC1 用例正是如此暴露它。本用例直接断言：构造端口不增加强计数、仍能读写同一份存储，存储关闭后
    /// 如实返回不可用（不 panic、不返回空字节）。
    #[tokio::test]
    async fn the_injected_attachment_port_does_not_own_the_store() {
        let dir = TempDir::new("attachment-port");
        let composition =
            Composition::assemble(dev_config(&dir.path, ""), Arc::new(LoggingPublisher))
                .await
                .expect("装配");
        let store = composition.attachments();
        let before = Arc::strong_count(&store);
        let port = WeakAttachmentStore::new(&store);
        assert_eq!(
            Arc::strong_count(&store),
            before,
            "弱端口不得增加存储的强引用（否则泄漏的会话环会阻止检查点）"
        );

        let at = composition.clock().now();
        let reference = port
            .put(b"attachment-bytes", "text/plain", at.clone())
            .await
            .expect("经弱端口写入");
        assert_eq!(
            port.get(&reference.id).await.expect("经弱端口读取"),
            Some(b"attachment-bytes".to_vec()),
            "弱端口必须转发到同一份存储"
        );

        drop(store);
        composition.close().await.expect("关闭组合根");
        assert!(
            matches!(
                port.get(&reference.id).await,
                Err(PortError::Unavailable(UnavailableKind::IoError))
            ),
            "存储关闭后必须如实不可用，而不是 panic 或返回空字节"
        );
    }

    /// 关闭顺序：仍有其它句柄时 `close()` 必须失败关闭，全部释放后才关闭连接池。
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
}
