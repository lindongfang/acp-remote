//! 受控路径全链路集成测试里的 **Owner Node**：把生产接线（`app::daemon` 的同一批装配点）与**唯一的**
//! 故障注入点（`SessionStore::commit`）组合起来，用真实 SQLite、真实 `identity_auth::Authority`、真实
//! `transport::net` listener 驱动 Node Link 的入站面。
//!
//! 与生产路径的关系：
//!
//! - 路由与 path 注册走 `app::daemon::{net_config, register_node_link_paths}`（组合根自己用的同一函数），
//!   因此「node id 来源」「路由顺序」「配对端点安全响应头」不会在测试里出现第二份口径；
//! - 撤销通知缝用 `app::compose::NodeLinkCloser`（组合根注入 `local_admin` 的真实实现）；
//! - 本地确认走真实的 `LocalAdminRouter`（与本地通道服务的是同一个处理器），因此 `node.pair.confirm`/
//!   `export.revoke`/`node.revoke` 的持久化与通知顺序都被真实执行；
//! - **两处刻意的测试替身**：① Agent 后端用脚本化端点（真实 ACP 子进程不在本测试范围，且会引入
//!   进程/平台依赖）；② `SessionStore` 套一层可控失败的装饰器（R61 的故障注入点）——它只在提交的
//!   事件批次带指定 marker 时失败，其余提交原样落地。
//!   其余端口（trust/export/audit/config/attachments/deliveries）都是真实现。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::{
    Actor, AgentDescriptor, AgentRef, CapabilitySet, CommandRecord, ConfigOption,
    CreateSessionRequest, EndpointEvent, EventKind, EventPayload, EventType, GlobalCursor,
    InteractionId, InteractionResolution, ModeId, ModeState, OwnedSessionRef, PortError,
    PromptRequest, RequestId, Sequence, SessionId, SessionReference, SessionSnapshot,
    SessionSummary, Timestamp, TurnId, UnavailableKind, ViewJson,
};
use acp_core::ports::{
    AgentCatalog, AttachmentStore, AuditStore, Clock, CommitOutcome, EventSink, ExportStore,
    HistoryPage, HistoryQuery, IdGenerator, LocalConfigStore, OwnedCommit, PruneReport, ReadView,
    RemoteDeliveryStore, ReplayLimit, RetentionPolicy, SessionBackendFactory, SessionEndpoint,
    SessionQuery, SessionStore, StoreHealth, TrustStore, TurnAccepted,
};
use acp_core::use_cases::{UseCaseDeps, UseCases};
use identity_auth::{Authority, EntropySource, IdentityKeystore};
use identity_keystore::{EphemeralKeystore, OsEntropy};
use server::local_admin::{
    AdminOutcome, AdminRequest, AdminResponse, DaemonControl, DaemonStatus, LocalAdminDeps,
    LocalAdminHandler, LocalAdminRouter, Method, PairingSessions, decode_request,
};
use server::node_link::{CommandRoute, NodeLinkConfig};
use server::transport::net::{NetListener, Shutdown};
use storage_sqlite::session_store::SqliteStore;
use tokio::task::JoinHandle;

use super::TempRoot;
use super::nodelink;

/// 脚本化后端提供的 Agent selector（用例创建 Export 时必须引用它）。
pub const SCRIPTED_AGENT: &str = "codex";

/// 受控路径的 Owner Node。
pub struct OwnerNode {
    root: TempRoot,
    /// 真实存储（除 `SessionStore` 外的端口共用同一个 `SqliteStore` 实例）。
    store: Arc<SqliteStore>,
    /// 故障注入开关：`commit` 命中该 marker 的事件批次时返回 `StorageFull`（`None` = 不注入）。
    fault_marker: Arc<Mutex<Option<String>>>,
    /// 脚本端点暂存的后端事件（`release_events` 把它们交给 broker）。
    parked: Arc<Parked>,
    /// 被拒绝的 commit 次数（断言「故障确实发生过」）。
    pub rejected_commits: Arc<AtomicU32>,
    /// 真实存储的会话端口（测试用它做持久事实的只读断言）。
    pub sessions: Arc<dyn SessionStore>,
    /// 用例面。
    pub core: Arc<UseCases>,
    /// broker（`UseCases` 内部同一实例；用例按合并窗口的语义直接 `pump`）。
    pub broker: Arc<Broker>,
    /// 身份状态机（本机 node id 的唯一来源）。
    pub authority: Arc<Authority>,
    /// 命令路由（撤销通知缝与终态观察的连接面）。
    pub command: Arc<CommandRoute>,
    /// 本地管理处理器（与本地通道服务的是同一个）。
    pub router: Arc<LocalAdminRouter>,
    /// 网络 listener 的实际地址。
    pub addr: std::net::SocketAddr,
    shutdown: server::transport::net::ShutdownHandle,
    tasks: Vec<(&'static str, JoinHandle<()>)>,
}

impl OwnerNode {
    /// 起一个受控路径的 Owner Node（真实存储 + 真实 listener + 脚本化 Agent 后端）。
    pub async fn start(label: &str) -> Self {
        Self::start_with(label, "").await
    }

    /// 同上，但 `daemon` 段追加 TLS `direct` 配置（自签 PEM 现算的轮次）。
    pub async fn start_tls(
        label: &str,
        cert_path: &std::path::Path,
        key_path: &std::path::Path,
    ) -> Self {
        let tls = format!(
            "[daemon.tls]\nmode = \"direct\"\ncert_path = \"{}\"\nkey_path = \"{}\"\n",
            cert_path.display().to_string().replace('\\', "/"),
            key_path.display().to_string().replace('\\', "/"),
        );
        Self::start_with(label, &tls).await
    }

    /// 装配主体：`daemon_extra` 追加在 `[daemon]` 段之后（可含 `[daemon.tls]` 表）。
    async fn start_with(label: &str, daemon_extra: &str) -> Self {
        install_trace_logging();
        let root = TempRoot::new(label);
        let data_dir = root.join("data");
        super::create_owner_only_dir(&data_dir);
        let config_text = format!(
            "[daemon]\ndata_dir = \"{}\"\npublic_origin = \"https://{}\"\nlisten = \"127.0.0.1:0\"\n{daemon_extra}",
            data_dir.display().to_string().replace('\\', "/"),
            nodelink::PUBLIC_HOST,
        );
        let config = app::Config::from_toml(&config_text).expect("测试配置合法");
        let clock: Arc<dyn Clock> = Arc::new(app::clock::SystemClock::new());
        let ids: Arc<dyn IdGenerator> = Arc::new(app::compose::UuidIdGenerator);
        let store = Arc::new(
            SqliteStore::open(config.storage.clone(), &clock.now())
                .await
                .expect("打开真实存储"),
        );
        let fault_marker = Arc::new(Mutex::new(None));
        let rejected_commits = Arc::new(AtomicU32::new(0));
        let sessions: Arc<dyn SessionStore> = Arc::new(FlakySessionStore {
            inner: Arc::clone(&store),
            marker: Arc::clone(&fault_marker),
            rejected: Arc::clone(&rejected_commits),
        });
        let deliveries: Arc<dyn RemoteDeliveryStore> = store.clone();
        let exports: Arc<dyn ExportStore> = store.clone();
        let trust: Arc<dyn TrustStore> = store.clone();
        let audit: Arc<dyn AuditStore> = store.clone();
        let local_config: Arc<dyn LocalConfigStore> = store.clone();
        let attachments: Arc<dyn AttachmentStore> = store.clone();

        let entropy: Arc<dyn EntropySource> = Arc::new(OsEntropy::new());
        let keystore: Arc<dyn IdentityKeystore> =
            Arc::new(EphemeralKeystore::new(Arc::clone(&entropy)));
        let identity = app::NodeIdentity::ensure(keystore.as_ref())
            .await
            .expect("本机身份就绪");
        let authority = Arc::new(Authority::new(
            identity.node_id().clone(),
            // 条目标签只有一个来源：`app::identity` 给出的 `NodeIdentity::key()`（不再复刻私有常量）。
            identity.key().clone(),
            Arc::clone(&keystore),
            Arc::clone(&entropy),
            Arc::clone(&clock),
        ));

        let parked = Arc::new(Parked::default());
        let backends: Arc<dyn SessionBackendFactory> = Arc::new(ScriptedBackends {
            parked: Arc::clone(&parked),
        });
        let catalog: Arc<dyn AgentCatalog> = Arc::new(ScriptedCatalog);
        let (publisher, queue) = app::compose::forked_publisher();
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: Arc::clone(&sessions),
                deliveries: Arc::clone(&deliveries),
                backends,
                exports: Arc::clone(&exports),
                trust: Arc::clone(&trust),
                publisher,
                clock: Arc::clone(&clock),
                ids: Arc::clone(&ids),
                audit: Some(Arc::clone(&audit)),
            },
            BrokerConfig::default(),
        ));
        let core = Arc::new(UseCases::new(UseCaseDeps {
            broker: Arc::clone(&broker),
            store: Arc::clone(&sessions),
            deliveries,
            exports,
            trust,
            audit: Arc::clone(&audit),
            config: local_config,
            attachments,
            catalog,
            clock: Arc::clone(&clock),
            ids: Arc::clone(&ids),
        }));

        // —— 网络接入面：与组合根同一批装配点 ——
        let mut listener = NetListener::bind(app::daemon::net_config(&config))
            .await
            .expect("绑定 loopback listener");
        let ingress = app::daemon::register_node_link_paths(
            &mut listener,
            &Arc::clone(&core),
            &Arc::clone(&authority),
            &NodeLinkConfig {
                public_origin: config.public_origin.clone(),
                ..NodeLinkConfig::default()
            },
        )
        .expect("注册 Node Link path");
        let addr = listener.local_addrs()[0];
        let (shutdown, signal) = Shutdown::channel();
        let mut tasks: Vec<(&'static str, JoinHandle<()>)> = Vec::new();
        // 三个任务共用同一个关闭信号（与组合根 `NetIngress` 的语义一致）：触发即停止 accept、
        // 排空在途连接并退出两个分发循环。
        tasks.push((
            "node_link_listener",
            tokio::spawn({
                let signal = signal.clone();
                async move {
                    let _ = listener.serve(signal).await;
                }
            }),
        ));
        let fanout_signal = signal.clone();
        tasks.push((
            "node_link_fanout",
            tokio::spawn({
                let resource = Arc::clone(ingress.resource());
                async move { resource.dispatch(queue, fanout_signal).await }
            }),
        ));
        let command_signal = signal.clone();
        tasks.push((
            "node_link_commands",
            tokio::spawn({
                let command = Arc::clone(ingress.command());
                async move { command.dispatch(command_signal).await }
            }),
        ));

        // —— 本地管理面：与本地通道同一个处理器；撤销通知缝用组合根的真实实现 ——
        let router = Arc::new(LocalAdminRouter::new(LocalAdminDeps {
            daemon: Arc::new(UnusedDaemonControl),
            core: Arc::clone(&core),
            keystore: Arc::clone(&keystore),
            audit: Arc::clone(&audit),
            clock: Arc::clone(&clock),
            pairing: Arc::new(PairingSessions::new(
                Arc::clone(&authority),
                config.public_origin.clone(),
                Arc::new(app::compose::NodeLinkCloser::new(Arc::clone(
                    ingress.command(),
                ))),
            )),
        }));

        Self {
            root,
            store,
            fault_marker,
            parked,
            rejected_commits,
            sessions,
            core,
            broker,
            authority,
            command: Arc::clone(ingress.command()),
            router,
            addr,
            shutdown,
            tasks,
        }
    }

    /// 本机（Owner）node id。
    pub fn owner_node_id(&self) -> &acp_core::model::NodeId {
        self.authority.local_node()
    }

    /// 数据目录（注册 workspace alias 时需要一个真实存在的目录）。
    pub fn data_dir(&self) -> PathBuf {
        self.root.join("data")
    }

    /// 注入/取消故障：`commit` 命中该 marker 的事件批次时返回 `StorageFull`。
    ///
    /// 这是 R61（「broker 提交失败时连接上不出现对应 `resource.event`」）的注入点，也是本测试唯一一处
    /// 影响生产行为的替身：失败只发生在带该 marker 的那一批提交上。该批属于**在跑的** turn，因此按
    /// `CORE_PORTS_AND_STORAGE.md` §6 第 9 条这个 turn 随后被放弃——同一个 turn 的终态批与它后续的事件
    /// 都不再提交（命令以 `uncertain` 收尾），其余 turn 的事件照常提交。
    pub fn fail_commits_with(&self, marker: Option<&str>) {
        let mut current = self.fault_marker.lock().expect("故障开关");
        *current = marker.map(str::to_owned);
    }

    /// 释放某个 marker 暂存的后端事件（模拟 Agent 在 accepted 之后产出）；返回释放条数。
    pub fn release_events(&self, marker: &str) -> usize {
        self.parked.release(marker)
    }

    /// 调用一个本地管理方法（与本地通道同一处理器；失败即 panic 出可读消息）。
    pub async fn admin(&self, method: Method, params: serde_json::Value) -> serde_json::Value {
        let response = self.call(method, params).await;
        match response.outcome() {
            AdminOutcome::Success { result } => serde_json::Value::Object(result.clone()),
            AdminOutcome::Failure { error } => panic!(
                "`{}` 期望成功，实际 {}：{}",
                method.as_str(),
                error.code().as_str(),
                error.message()
            ),
        }
    }

    /// 调用一个本地管理方法并返回原始响应（错误路径用例用）。
    pub async fn call(&self, method: Method, params: serde_json::Value) -> AdminResponse {
        let payload = serde_json::to_vec(&serde_json::json!({
            "v": 1,
            "id": nodelink::uuid_text(),
            "method": method.as_str(),
            "params": params,
        }))
        .expect("请求信封可序列化");
        let request: AdminRequest = decode_request(&payload).expect("信封合法");
        self.router.handle(request).await
    }

    /// 关闭接入面并释放存储（用例末尾调用；留下零后台任务与零打开端口）。
    pub async fn stop(self) {
        let Self {
            root,
            store,
            fault_marker,
            parked: _,
            rejected_commits,
            sessions,
            core,
            broker,
            authority,
            command,
            router,
            addr: _,
            shutdown,
            tasks,
        } = self;
        shutdown.trigger();
        for (name, mut handle) in tasks {
            match tokio::time::timeout(Duration::from_secs(10), &mut handle).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => panic!("接入面任务 `{name}` 异常结束：{error}"),
                Err(_) => {
                    handle.abort();
                    let _ = handle.await;
                    panic!("接入面任务 `{name}` 未在期限内退出");
                }
            }
        }
        drop(command);
        drop(router);
        drop(core);
        drop(broker);
        drop(authority);
        drop(sessions);
        drop(fault_marker);
        drop(rejected_commits);
        match Arc::try_unwrap(store) {
            Ok(store) => store.close().await,
            Err(_) => panic!("存储仍有其它持有者：关闭序列会漏掉 checkpoint"),
        }
        drop(root);
    }
}

/// 置 `ACPR_TEST_LOG=1` 时把 daemon 侧的结构化日志（DEBUG）装到 stderr，便于排查全链路卡点。
fn install_trace_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if std::env::var_os("ACPR_TEST_LOG").is_none() {
            return;
        }
        let config = app::config::LoggingConfig {
            level: tracing::Level::DEBUG,
            format: app::config::LogFormat::Text,
            file: None,
        };
        let _ = app::logging::init(&config);
    });
}

/// 只实现本地管理注入点里与 Node Link 无关的 `daemon.*`（用例不调用它们）。
struct UnusedDaemonControl;

#[async_trait::async_trait]
impl DaemonControl for UnusedDaemonControl {
    async fn status(&self) -> DaemonStatus {
        unreachable!("受控路径用例不经 daemon.status")
    }

    async fn stop(&self, _grace_ms: Option<u64>) -> Result<(), server::local_admin::AdminError> {
        unreachable!("受控路径用例不经 daemon.stop")
    }
}

/// `SessionStore` 装饰器：把带 marker 的事件批次挡在存储之外（其余方法原样转发给真实实现）。
struct FlakySessionStore {
    inner: Arc<SqliteStore>,
    marker: Arc<Mutex<Option<String>>>,
    rejected: Arc<AtomicU32>,
}

impl FlakySessionStore {
    /// 本批次是否携带当前注入的 marker。
    fn matches_fault(&self, commit: &OwnedCommit) -> bool {
        let marker = self.marker.lock().expect("故障开关");
        let Some(marker) = marker.as_deref() else {
            return false;
        };
        commit
            .events
            .iter()
            .any(|event| event.payload.view.as_str().contains(marker))
    }
}

#[async_trait::async_trait]
impl SessionStore for FlakySessionStore {
    async fn commit(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        if self.matches_fault(&commit) {
            self.rejected.fetch_add(1, Ordering::SeqCst);
            return Err(PortError::Unavailable(UnavailableKind::StorageFull));
        }
        self.inner.commit(commit).await
    }

    async fn load(&self, session: &SessionId) -> Result<Option<SessionSnapshot>, PortError> {
        self.inner.load(session).await
    }

    async fn list(&self, query: SessionQuery) -> Result<Vec<SessionSummary>, PortError> {
        self.inner.list(query).await
    }

    async fn head(&self) -> Result<GlobalCursor, PortError> {
        self.inner.head().await
    }

    async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError> {
        self.inner.read_view().await
    }

    async fn find_request(
        &self,
        request: &RequestId,
        actor: &Actor,
    ) -> Result<Option<CommandRecord>, PortError> {
        self.inner.find_request(request, actor).await
    }

    async fn unsettled_commands(
        &self,
        limit: ReplayLimit,
    ) -> Result<Vec<CommandRecord>, PortError> {
        self.inner.unsettled_commands(limit).await
    }

    async fn retention_window(
        &self,
        session: &SessionId,
    ) -> Result<Option<(Sequence, Sequence)>, PortError> {
        self.inner.retention_window(session).await
    }

    async fn prune(
        &self,
        policy: RetentionPolicy,
        at: Timestamp,
    ) -> Result<PruneReport, PortError> {
        SessionStore::prune(self.inner.as_ref(), policy, at).await
    }

    async fn health(&self) -> Result<StoreHealth, PortError> {
        self.inner.health().await
    }
}

/// Agent 目录替身：`export.create` 要求被引用的 agent 在本机目录里，脚本端点的 agent 因此必须出现。
struct ScriptedCatalog;

#[async_trait::async_trait]
impl AgentCatalog for ScriptedCatalog {
    async fn agents(&self) -> Result<Vec<AgentDescriptor>, PortError> {
        Ok(vec![AgentDescriptor::new(
            AgentRef::try_new(
                acp_core::model::AgentId::new(SCRIPTED_AGENT).expect("agent id"),
                "Codex",
            )
            .expect("agent ref"),
            true,
            acp_core::model::ResourceOrigin::Local,
        )])
    }

    async fn agent_capabilities(&self, _agent: &AgentRef) -> Result<CapabilitySet, PortError> {
        Ok(CapabilitySet::empty())
    }
}

/// 会话后端替身：`create`/`open` 返回脚本化端点（不启动进程）。
struct ScriptedBackends {
    parked: Arc<Parked>,
}

#[async_trait::async_trait]
impl SessionBackendFactory for ScriptedBackends {
    async fn create(
        &self,
        session: &SessionId,
        _request: CreateSessionRequest,
        sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        Ok(Box::new(ScriptedEndpoint {
            reference: SessionReference::Owned(OwnedSessionRef::new(session.clone())),
            sink,
            parked: Arc::clone(&self.parked),
        }))
    }

    async fn open(
        &self,
        reference: SessionReference,
        sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        Ok(Box::new(ScriptedEndpoint {
            reference,
            sink,
            parked: Arc::clone(&self.parked),
        }))
    }
}

/// 脚本端点的「暂存 → 释放」两段式。
///
/// 真实的 Agent 在 **accepted 回复之后**才陆续产出事件，命令的终态因此由终态观察循环推送；若脚本端点
/// 在 `prompt` 里一次性把 delta 与 turn 终态塞进 sink，broker 会在 `submit_command` 内跑完整轮 turn，
/// 回复就变成「直接回终态」——那条路径同样合法，但**覆盖不到**终态观察循环（RV1-WP6-F9/F4）。
/// 因此脚本端点先按 prompt 正文（marker）暂存，等用例显式 `release` 时才送进 sink。
#[derive(Default)]
struct Parked {
    /// marker → 暂存的后端事件（顺序即提交顺序）。
    events: Mutex<std::collections::BTreeMap<String, Vec<EndpointEvent>>>,
    /// 最近一次由工厂拿到的 sink（本切片只有一个会话，够用）。
    sink: Mutex<Option<EventSink>>,
}

/// `std::sync::Mutex` 的取锁简写（中毒即 panic：用例内不可能有别的持有者）。
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().expect("用例内互斥量不会中毒")
}

impl Parked {
    fn remember_sink(&self, sink: &EventSink) {
        *lock(&self.sink) = Some(sink.clone());
    }

    fn park(&self, marker: &str, events: Vec<EndpointEvent>) {
        lock(&self.events).insert(marker.to_owned(), events);
    }

    /// 把某个 marker 暂存的事件送进 broker 的 sink（模拟 Agent 稍后产出）。
    pub fn release(&self, marker: &str) -> usize {
        let events = lock(&self.events).remove(marker).unwrap_or_default();
        let count = events.len();
        match lock(&self.sink).as_ref() {
            Some(sink) => {
                for event in events {
                    sink.send(event);
                }
            }
            None => panic!("脚本端点还没有拿到 sink"),
        }
        count
    }
}

/// 脚本化会话端点：`prompt` 时把固定脚本（一条 delta + turn 终态）交给 broker 的 sink。
///
/// 脚本里 view 的 `text` 取**本次 prompt 的正文**：用例因此可以用正文当 marker，按 marker 断言
/// 「哪一条事件到了连接上」（也让 R61 的注入能精确命中一条事件批次）。
struct ScriptedEndpoint {
    reference: SessionReference,
    sink: EventSink,
    parked: Arc<Parked>,
}

impl ScriptedEndpoint {
    fn script(&self, marker: &str) -> Vec<EndpointEvent> {
        let at = Timestamp::new("2026-09-26T10:00:00.000Z").expect("规范时间戳");
        vec![
            EndpointEvent::new(
                EventKind::Delta,
                EventType::new("agent.message.delta").expect("合法事件类型"),
                EventPayload::new(
                    ViewJson::new(&format!(
                        r#"{{"state":"running","text":"{marker}","messageId":"2ae1c07c-0000-4000-8000-0000000000aa"}}"#
                    ))
                    .expect("view 是 JSON object"),
                    None,
                ),
                None,
                None,
                at.clone(),
            ),
            EndpointEvent::new(
                EventKind::State,
                EventType::new("turn.completed").expect("合法事件类型"),
                EventPayload::new(
                    ViewJson::new(r#"{"state":"completed"}"#).expect("view 是 JSON object"),
                    None,
                ),
                None,
                None,
                at,
            ),
        ]
    }
}

#[async_trait::async_trait]
impl SessionEndpoint for ScriptedEndpoint {
    fn reference(&self) -> SessionReference {
        self.reference.clone()
    }

    async fn prompt(
        &self,
        request: PromptRequest,
        _at: Timestamp,
    ) -> Result<TurnAccepted, PortError> {
        let marker = prompt_text(&request);
        self.parked.remember_sink(&self.sink);
        self.parked.park(&marker, self.script(&marker));
        Ok(TurnAccepted {
            turn: TurnId::new(&nodelink::uuid_text()).expect("turn id 是规范 uuid 文本"),
        })
    }

    async fn cancel(&self, _turn: Option<TurnId>) -> Result<(), PortError> {
        Ok(())
    }

    async fn set_mode(&self, _mode: &ModeId) -> Result<(), PortError> {
        unreachable!("受控路径用例不切模式")
    }

    async fn list_config(&self) -> Result<Vec<ConfigOption>, PortError> {
        Ok(Vec::new())
    }

    async fn modes(&self) -> Result<ModeState, PortError> {
        Ok(ModeState::new(None, Vec::new()))
    }

    async fn set_config(
        &self,
        _id: &acp_core::model::ConfigOptionId,
        _value: acp_core::model::ConfigValue,
    ) -> Result<(), PortError> {
        unreachable!("受控路径用例不改配置")
    }

    async fn resolve_interaction(
        &self,
        _interaction: &InteractionId,
        _resolution: InteractionResolution,
    ) -> Result<(), PortError> {
        unreachable!("受控路径用例不解析交互")
    }

    async fn read_history(&self, _query: HistoryQuery) -> Result<HistoryPage, PortError> {
        unreachable!("受控路径用例不读历史")
    }

    async fn close(&self) -> Result<(), PortError> {
        Ok(())
    }
}

/// 本次 prompt 的正文（用例用 `{"type":"text","text":"<marker>"}` 作为唯一内容块）。
fn prompt_text(request: &PromptRequest) -> String {
    let block = request.content.first().expect("prompt 至少一个内容块");
    let value: serde_json::Value =
        serde_json::from_str(block.as_str()).expect("prompt 内容块是 JSON object");
    value
        .get("text")
        .and_then(serde_json::Value::as_str)
        .expect("prompt 内容块带 text")
        .to_owned()
}
