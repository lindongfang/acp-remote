//! 组合层：把 profile 目录、进程监督与会话端点装配成 core 需要的两个出站端口。
//!
//! 所有权与生命周期（`docs/MODULE_ARCHITECTURE.md` §4.5）：
//!
//! - **每个 Agent 一个进程与一个 Job/进程组**：结束某个 Agent 不影响其它 Agent。监督者由本结构持有。
//! - 每个监督者拥有自己的任务集（读 stdout、读 stderr、退出监视、写出、turn 等待、进站路由），
//!   关闭时统一 join；本模块不创建 detached task。
//! - 目录查询（[`AgentCatalog::agents`]）**不做 spawn、不消费凭据值**：它只探测「凭据能否解析」，
//!   而探测本身会从 keystore 取值（端口没有 `is_resolvable()` 之类的只读接口）。
//! - 空闲回收**破坏运行时与映射**：命中的运行时在同一临界区被移出目录、其会话映射被清空，
//!   `open()` 随后必须显式失败（不得把已结束的进程当作活跃端点）；可用性语义不随运行状态变化。
//! - 能力协商按**进程代**缓存：进程换了，缓存即失效。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use acp_core::model::{
    AgentDescriptor, AgentId, AgentProfile, AgentRef, Capability, CapabilitySet,
    CreateSessionRequest, PortError, ResourceOrigin, SessionId, SessionReference,
};
use acp_core::ports::{
    AgentCatalog, Clock, CredentialResolver, EventSink, IdGenerator, LocalConfigStore,
    SessionBackendFactory, SessionEndpoint,
};
use acp_protocol::Envelope;
use acp_protocol::capability::AgentCapabilities;
use acp_protocol::message::{self, InitializeRequest, InitializeResponse};
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc};

use crate::config::HostConfig;
use crate::error::HostError;
use crate::launch;
use crate::limits;
use crate::process::Supervisor;
use crate::session::{AcpSession, Endpoint, SessionInit};

/// 一个 Agent 的运行时（一个进程 + 该进程上的全部会话）。
#[derive(Debug)]
struct AgentRuntime {
    supervisor: Arc<Supervisor>,
    /// 协商缓存与**进程代**绑定（`generation` 变化即失效——这里每次启动都是新实例，因此天然成立）。
    generation: u64,
    capabilities: std::sync::Mutex<Option<AgentCapabilities>>,
    /// ACP 会话标识 → 会话端点（路由热路径）。
    by_acp: std::sync::Mutex<HashMap<String, Arc<AcpSession>>>,
    /// core 会话标识 → ACP 会话标识（重复 create 与 open 的判据）。
    by_core: std::sync::Mutex<HashMap<String, String>>,
    /// 进程级最近活动时间（没有活动会话时的空闲回收判据）。
    last_activity: std::sync::Mutex<std::time::Instant>,
}

impl AgentRuntime {
    fn new(supervisor: Arc<Supervisor>, generation: u64) -> Self {
        Self {
            supervisor,
            generation,
            capabilities: std::sync::Mutex::new(None),
            by_acp: std::sync::Mutex::new(HashMap::new()),
            by_core: std::sync::Mutex::new(HashMap::new()),
            last_activity: std::sync::Mutex::new(std::time::Instant::now()),
        }
    }

    /// 进程代（诊断与测试用）。
    #[must_use]
    fn generation(&self) -> u64 {
        self.generation
    }

    /// 进程级空闲判据（没有活动会话时的回收依据）。
    ///
    /// 刷新点：进程启动、会话建立与关闭，以及会话上的出站/入站活动（`AcpSession::touch`）。
    /// **复用已运行 runtime 的目录/能力查询不刷新它**（`agent_capabilities` 只读能力缓存，不产生新的
    /// 进程内工作），因此只做目录查询不会给进程延寿。
    fn idle_for(&self, timeout: Duration) -> bool {
        lock(&self.last_activity).elapsed() >= timeout
    }

    /// 该运行时是否可以按空闲超时回收。
    ///
    /// 没有活动会话（例如只协商过能力的进程）看进程级时钟；有会话时要求**全部**仍打开的会话都空闲
    /// （任一会话有进行中的 turn 就不回收）。
    ///
    /// **已关闭的会话不参与判定**：`Endpoint::close()` 只置关闭位、不摘映射，而 `AcpSession::is_idle_for`
    /// 对已关闭会话恒为假；若把它算进来，只要 `by_acp` 里留着一个已关闭会话，这个 runtime 就永远不会
    /// 被空闲回收（进程泄漏）。会话全部关闭后同样退回进程级时钟。
    fn is_idle(&self, timeout: Duration) -> bool {
        let live: Vec<Arc<AcpSession>> = self
            .sessions()
            .into_iter()
            .filter(|session| !session.is_closed())
            .collect();
        if live.is_empty() {
            return self.idle_for(timeout);
        }
        live.iter().all(|session| session.is_idle_for(timeout))
    }

    /// 让出全部会话映射。
    ///
    /// 回收与关闭路径必须先调它：映射一旦随 runtime 一起离开目录就不再可达，`open()` 因此不会再
    /// 拿一个已关闭的 supervisor 去重建端点（Q4-1）。
    fn clear_sessions(&self) {
        lock(&self.by_core).clear();
        lock(&self.by_acp).clear();
    }

    fn touch(&self) {
        *lock(&self.last_activity) = std::time::Instant::now();
    }

    fn session_for_acp(&self, acp_session_id: &str) -> Option<Arc<AcpSession>> {
        lock(&self.by_acp).get(acp_session_id).cloned()
    }

    fn insert_session(&self, session: &Arc<AcpSession>) -> Result<(), HostError> {
        let acp = session.acp_session_id().to_owned();
        let core_id = session.session_id().as_str().to_owned();
        if lock(&self.by_core).contains_key(&core_id) {
            return Err(HostError::DuplicateSession { session: core_id });
        }
        lock(&self.by_core).insert(core_id, acp.clone());
        lock(&self.by_acp).insert(acp, Arc::clone(session));
        self.touch();
        Ok(())
    }

    fn remove_session(&self, session: &AcpSession) {
        lock(&self.by_core).remove(session.session_id().as_str());
        lock(&self.by_acp).remove(session.acp_session_id());
        self.touch();
    }

    fn sessions(&self) -> Vec<Arc<AcpSession>> {
        lock(&self.by_acp).values().cloned().collect()
    }

    /// 进程退出时收敛：进行中的 turn 必须以明确失败结束（**一次**），能力缓存随之失效。
    fn on_exit(&self) {
        let status = self
            .supervisor
            .exit_status()
            .unwrap_or_else(|| "未知".to_owned());
        for session in self.sessions() {
            session.on_agent_exit(&status);
        }
        lock(&self.capabilities).take();
    }
}

/// 实现 [`AgentCatalog`] 与 [`SessionBackendFactory`] 的本地 Agent 组合层。
pub struct AgentHost {
    config: Arc<dyn LocalConfigStore>,
    credentials: Arc<dyn CredentialResolver>,
    host_config: HostConfig,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    runtimes: Mutex<HashMap<AgentId, Arc<AgentRuntime>>>,
    generations: std::sync::Mutex<HashMap<AgentId, u64>>,
    /// 关闭标志：`shutdown_all()` 先置位，此后 `ensure_runtime` 一律拒绝启动新进程。
    shutting_down: AtomicBool,
}

impl std::fmt::Debug for AgentHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentHost")
            .field("idle_timeout", &self.host_config.idle_timeout())
            .finish_non_exhaustive()
    }
}

impl AgentHost {
    /// 构造（组合根装配本机实现）。
    #[must_use]
    pub fn new(
        config: Arc<dyn LocalConfigStore>,
        credentials: Arc<dyn CredentialResolver>,
        host_config: HostConfig,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            config,
            credentials,
            host_config,
            ids,
            clock,
            runtimes: Mutex::new(HashMap::new()),
            generations: std::sync::Mutex::new(HashMap::new()),
            shutting_down: AtomicBool::new(false),
        }
    }

    /// 空闲回收判据（`None` = 不因空闲关闭）。
    #[must_use]
    pub fn idle_timeout(&self) -> Option<Duration> {
        self.host_config.idle_timeout()
    }

    /// 取（必要时启动）某个 Agent 的运行时。
    ///
    /// 一把异步锁串行化「启动 + initialize」：并发调用不会造出两个进程、两个 Job 或两条协商。
    async fn ensure_runtime(&self, agent: &AgentId) -> Result<Arc<AgentRuntime>, HostError> {
        // 关闭中不得再启动新进程（`shutdown_all` 已置位）：显式失败，不静默超时、不偷偷拉起来。
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(HostError::NotRunning);
        }
        let mut runtimes = self.runtimes.lock().await;
        // 取到锁之后**再查一次**：`shutdown_all` 先置位再取锁，因此在「读标志」与「拿到锁」之间被插入的
        // 调用会在这里被拦下——否则它会在 `shutdown_all` 清表之后往目录里塞一个新进程，"此后一律拒绝"
        // 就只对「取锁之前就读到标志」的调用成立。
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(HostError::NotRunning);
        }
        let stale = runtimes.get(agent).cloned();
        if let Some(runtime) = stale {
            if runtime.supervisor.is_running() {
                return Ok(runtime);
            }
            // 进程已退出：不跨代复用这个 runtime（也不留残留映射）——先作废它的会话端点与映射，
            // 再按关闭顺序回收整棵树与任务，最后按「不存在」重启（新一代）。
            runtimes.remove(agent);
            for session in runtime.sessions() {
                session.close_session();
            }
            runtime.clear_sessions();
            runtime.supervisor.shutdown().await;
        }

        let (_profile, spec) =
            launch::resolve_launch(self.config.as_ref(), self.credentials.as_ref(), agent).await?;
        let (supervisor, incoming) = Supervisor::start(spec).await?;
        let generation = {
            let mut generations = lock(&self.generations);
            let next = generations.entry(agent.clone()).or_insert(0);
            *next += 1;
            *next
        };
        let runtime = Arc::new(AgentRuntime::new(Arc::clone(&supervisor), generation));

        match self.initialize(&runtime, limits::STARTUP_TIMEOUT).await {
            Ok(capabilities) => {
                supervisor.set_negotiated_capabilities(capabilities.clone());
                *lock(&runtime.capabilities) = Some(capabilities);
            }
            Err(error) => {
                // 协商失败：不留下半开的进程（进程树与任务一并回收）。
                supervisor.shutdown().await;
                return Err(error);
            }
        }

        runtime.touch();
        spawn_router(Arc::clone(&runtime), incoming);
        runtimes.insert(agent.clone(), Arc::clone(&runtime));
        Ok(runtime)
    }

    /// 执行 `initialize` 并解出能力声明（只记录 Agent 真宣告的内容）。
    async fn initialize(
        &self,
        runtime: &AgentRuntime,
        timeout: Duration,
    ) -> Result<AgentCapabilities, HostError> {
        let request = InitializeRequest {
            protocol_version: 1,
            // 默认 `none()`：没有可作答的上游客户端就绝不宣告 fs/terminal/elicitation。
            client_capabilities: self.host_config.client_capabilities.clone(),
            client_info: self.host_config.client_info.clone(),
            meta: None,
        };
        let params = serde_json::to_value(&request).map_err(|_| HostError::IdUnavailable)?;
        let value = runtime
            .supervisor
            .request("initialize", &params, timeout)
            .await?;
        let response: InitializeResponse =
            serde_json::from_value(value).map_err(|error| HostError::SpawnFailed {
                detail: format!("无法解码 initialize 响应：{error}"),
            })?;
        if response.protocol_version != 1 {
            return Err(HostError::Protocol(acp_protocol::AcpError::InvalidField {
                field: "protocolVersion".to_owned(),
                detail: "ACP 版本不受支持".to_owned(),
            }));
        }
        Ok(response.capabilities())
    }

    /// 当前协商到的能力（未启动的 Agent 返回空集合：不虚报）。
    async fn negotiated(&self, agent: &AgentId) -> Result<AgentCapabilities, PortError> {
        let runtime = self
            .ensure_runtime(agent)
            .await
            .map_err(|error| error.to_port_error())?;
        Ok(lock(&runtime.capabilities).clone().unwrap_or_default())
    }

    /// 关闭空闲超时的会话所属进程（组合根的定时任务调用）。
    ///
    /// `idle_timeout` 为零表示**不因空闲关闭**（`sessions.idle_timeout_ms = 0`），此时直接返回；
    /// 只在「没有进行中的 turn 且空闲足够久」时才结束进程。
    ///
    /// 判定与「移出目录」在**同一个临界区**里完成：不能留下「死 supervisor 还挂在目录里、映射仍可被
    /// `open` 复用」的窗口（Q4-1）。实际的关闭流程（最长 5 s grace）在锁外执行，不阻塞其它入口。
    pub async fn sweep_idle(&self, idle_timeout: Duration) {
        if idle_timeout.is_zero() {
            return;
        }
        let reclaimed: Vec<Arc<AgentRuntime>> = {
            let mut runtimes = self.runtimes.lock().await;
            let idle: Vec<AgentId> = runtimes
                .iter()
                .filter(|(_, runtime)| runtime.is_idle(idle_timeout))
                .map(|(agent, _)| agent.clone())
                .collect();
            let mut reclaimed = Vec::with_capacity(idle.len());
            for agent in idle {
                if let Some(runtime) = runtimes.remove(&agent) {
                    // 先让端点作废（`close_session` 只改内存状态、不等待），再清空映射：
                    // 回收后的既有会话不得再被 `open` 复用。
                    for session in runtime.sessions() {
                        session.close_session();
                    }
                    runtime.clear_sessions();
                    reclaimed.push(runtime);
                }
            }
            reclaimed
        };
        for runtime in reclaimed {
            runtime.supervisor.shutdown().await;
        }
    }

    /// 显式关闭某个 Agent 的进程树（会话级语义由 core 决定）。
    pub async fn shutdown_agent(&self, agent: &AgentId) {
        // 先从目录移除：之后 `ensure_runtime` 看到的是「不存在」，不会复用正在关闭的 supervisor。
        let runtime = self.runtimes.lock().await.remove(agent);
        if let Some(runtime) = runtime {
            for session in runtime.sessions() {
                session.close_session();
            }
            runtime.clear_sessions();
            runtime.supervisor.shutdown().await;
        }
    }

    /// 全部 Agent 的关闭（Daemon 停止时使用）。
    ///
    /// 先置关闭标志（此后 `ensure_runtime` 拒绝启动任何新进程），再清目录并逐个按关闭顺序回收。
    pub async fn shutdown_all(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let runtimes: Vec<Arc<AgentRuntime>> = {
            let mut runtimes = self.runtimes.lock().await;
            let values = runtimes.values().cloned().collect();
            runtimes.clear();
            values
        };
        for runtime in runtimes {
            for session in runtime.sessions() {
                session.close_session();
            }
            runtime.clear_sessions();
            runtime.supervisor.shutdown().await;
        }
    }
}

/// 进站消息路由：把 Agent 的通知与请求分发给对应会话。
fn spawn_router(runtime: Arc<AgentRuntime>, mut incoming: mpsc::UnboundedReceiver<Envelope>) {
    let supervisor = Arc::clone(&runtime.supervisor);
    supervisor.spawn_owned(async move {
        while let Some(envelope) = incoming.recv().await {
            route(&runtime, &envelope);
        }
        // 通道关闭 = 进程退出（读任务结束）：收敛进行中的 turn。
        runtime.on_exit();
    });
}

fn route(runtime: &AgentRuntime, envelope: &Envelope) {
    let session_id = envelope.params().ok().and_then(|params| {
        params
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let session = match session_id {
        Some(id) => runtime.session_for_acp(&id),
        // 权限/elicitation 请求在 ACP 里不总是带 `sessionId`。只有本进程恰好只有一个会话时才能
        // 无歧义地归因；有多个会话时**显式拒绝**，绝不猜一个会话去交付（猜错会把交互记到别的会话上）。
        None => {
            let sessions = runtime.sessions();
            if sessions.len() == 1 {
                sessions.first().cloned()
            } else {
                None
            }
        }
    };
    match session {
        Some(session) => session.handle_envelope(envelope),
        None => {
            if let Some(id) = envelope.id() {
                let _ = runtime.supervisor.send_error_response(
                    id,
                    message::CODE_INVALID_PARAMS,
                    "请求指向未知会话",
                );
            }
            tracing::warn!(
                method = envelope.method(),
                "收到指向未知会话的消息（已显式拒绝）"
            );
        }
    }
}

#[async_trait::async_trait]
impl AgentCatalog for AgentHost {
    async fn agents(&self) -> Result<Vec<AgentDescriptor>, PortError> {
        let profiles = self.config.profiles().await?;
        let mut descriptors = Vec::with_capacity(profiles.len());
        for profile in profiles {
            // 可用性只做只读探测：命令能解析 + 凭据能解析。**不做 spawn、不消费凭据值**
            // （解析本身会从 keystore 取值）。可用性**不**随运行状态变化：回收或进程崩溃后仍由这两项决定。
            let available = command_resolves(profile.command())
                && self.credentials.resolve_env(&profile).await.is_ok();
            descriptors.push(AgentDescriptor::new(
                agent_ref(&profile)?,
                available,
                ResourceOrigin::Local,
            ));
        }
        Ok(descriptors)
    }

    async fn agent_capabilities(&self, agent: &AgentRef) -> Result<CapabilitySet, PortError> {
        let capabilities = self.negotiated(agent.agent_id()).await?;
        Ok(capability_set(&capabilities))
    }
}

#[async_trait::async_trait]
impl SessionBackendFactory for AgentHost {
    async fn create(
        &self,
        session: &SessionId,
        request: CreateSessionRequest,
        sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        let agent = request.agent.agent_id().clone();
        let runtime = self
            .ensure_runtime(&agent)
            .await
            .map_err(|error| error.to_port_error())?;
        let capabilities = lock(&runtime.capabilities).clone().unwrap_or_default();

        let params = session_new_params(&request)?;
        let value = runtime
            .supervisor
            .request("session/new", &params, limits::SHORT_REQUEST_TIMEOUT)
            .await
            .map_err(|error| error.to_port_error())?;
        let response: acp_protocol::message::NewSessionResponse = serde_json::from_value(value)
            .map_err(|error| {
                HostError::SpawnFailed {
                    detail: format!("无法解码 session/new 响应：{error}"),
                }
                .to_port_error()
            })?;

        let session = Arc::new(AcpSession::new(SessionInit {
            session: session.clone(),
            acp_session_id: response.session_id,
            supervisor: Arc::clone(&runtime.supervisor),
            sink,
            ids: Arc::clone(&self.ids),
            clock: Arc::clone(&self.clock),
            capabilities,
            modes: response.modes,
            config_options: response.config_options.unwrap_or_default(),
        }));
        runtime
            .insert_session(&session)
            .map_err(|error| error.to_port_error())?;
        Ok(Box::new(Endpoint::new(session)))
    }

    async fn open(
        &self,
        reference: SessionReference,
        sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        let SessionReference::Owned(owned) = reference else {
            // imported 会话不归本 crate（那是 Node Link 的 Access 侧语义）。
            return Err(PortError::InvalidRequest("agent-host 只处理 owned 会话"));
        };
        let runtimes: Vec<Arc<AgentRuntime>> =
            self.runtimes.lock().await.values().cloned().collect();
        for runtime in runtimes {
            let existing = lock(&runtime.by_core)
                .get(owned.session_id.as_str())
                .cloned();
            let Some(acp_id) = existing else { continue };
            // 已经结束（进程退出或已被回收）的运行时不得复活映射：显式失败，绝不把死 supervisor
            // 重新标记成活跃端点（`spec.md` 的空闲回收场景）。
            if !runtime.supervisor.is_running() {
                return Err(HostError::SessionClosed.to_port_error());
            }
            let Some(previous) = runtime.session_for_acp(&acp_id) else {
                continue;
            };
            // 重开：同一个 core 会话换一个新的 endpoint 绑定。旧绑定必须先让出，避免同一会话同时
            // 存在两个会派发 turn 的 endpoint（`AGENTS.md` §3「同一会话最多一个 active turn」）。
            runtime.remove_session(&previous);
            let rebound = Arc::new(previous.rebind(sink));
            runtime
                .insert_session(&rebound)
                .map_err(|error| error.to_port_error())?;
            return Ok(Box::new(Endpoint::new(rebound)));
        }
        Err(PortError::InvalidRequest("没有该会话的活动 endpoint"))
    }
}

/// `session/new` 的参数。
///
/// 两个约束都来自 pinned schema（`schemas/acp/v1/upstream/schema.json` 的 `NewSessionRequest`）：
/// `cwd` 与 `mcpServers` 都是**必填**。因此：
///
/// - `cwd` 只能来自 core 已解析的 workspace 路径（本 crate 不读存储、不拼路径、也不拿 Daemon 的当前目录
///   冒充用户选定的 workspace）；core 未解析时显式拒绝，而不是发一个缺必填字段的请求；
/// - `mcpServers` 本阶段固定为空数组（没有 MCP 配置来源），空数组合法。
fn session_new_params(request: &CreateSessionRequest) -> Result<Value, PortError> {
    if request.template.is_some() {
        // template 展开属于 Export/Node Link 语义，本层没有可如实表达的位置：显式拒绝。
        return Err(HostError::CapabilityNotDeclared {
            capability: "session.template".to_owned(),
        }
        .to_port_error());
    }
    let Some(workspace) = &request.workspace else {
        return Err(PortError::InvalidRequest(
            "session/new 需要已解析的 workspace（ACP 要求 cwd）",
        ));
    };
    Ok(json!({
        "cwd": workspace.canonical_path(),
        "mcpServers": [],
    }))
}

/// 从 profile 造 `AgentRef`。
fn agent_ref(profile: &AgentProfile) -> Result<AgentRef, PortError> {
    AgentRef::try_new(profile.id().clone(), profile.display_name())
        .map_err(|_| PortError::Corrupt("profile 的展示名不合法"))
}

/// 能力集合：只放进 Agent **真的**宣告了的路径（词表来自
/// `acp-protocol` 的 `CAPABILITY_PATHS`，与矩阵 `capabilities[]` 逐条比对）。
fn capability_set(capabilities: &AgentCapabilities) -> CapabilitySet {
    capabilities
        .declared_capability_paths()
        .into_iter()
        .filter_map(|path| Capability::try_new(path, None).ok())
        .collect()
}

/// 命令是否可解析（带路径分隔符看文件是否存在，否则在 `PATH` 里找）。
fn command_resolves(command: &str) -> bool {
    if command.is_empty() {
        return false;
    }
    let path = std::path::Path::new(command);
    if path.components().count() > 1 {
        return path.is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let candidates = crate::platform::command_candidates(command);
    std::env::split_paths(&paths).any(|dir| candidates.iter().any(|name| dir.join(name).is_file()))
}

/// 以固定周期做空闲回收（组合根持有返回的句柄，随 Daemon 一起关闭）。
pub fn spawn_idle_sweep(
    host: Arc<AgentHost>,
    idle_timeout: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(limits::IDLE_SWEEP_INTERVAL);
        loop {
            ticker.tick().await;
            host.sweep_idle(idle_timeout).await;
        }
    })
}

/// 有界重试地读 `runtimes` 目录（`try_lock` 与并发入口竞争时不会立刻拿到锁）。
///
/// 局限（如实说明）：`runtimes` 是异步锁，同步函数只能用 `try_lock`；重试仍拿不到锁时返回 `None`，
/// 与「目录里没有这个条目」不可区分。因此这两个读函数是**诊断**手段，不是「进程真的结束了」的证据：
/// 进程外证据请用 fake child 的心跳文件（`tests/catalog.rs` 的回收用例断言回收后心跳不再增长）。
fn try_read_runtimes<T>(
    host: &AgentHost,
    read: impl Fn(&HashMap<AgentId, Arc<AgentRuntime>>) -> Option<T>,
) -> Option<T> {
    for _ in 0..8 {
        if let Ok(runtimes) = host.runtimes.try_lock() {
            return read(&runtimes);
        }
        std::thread::yield_now();
    }
    None
}

/// 某个 Agent 的进程是否在运行（诊断用；不改变状态）。
#[must_use]
pub fn runtime_running(host: &AgentHost, agent: &AgentId) -> bool {
    try_read_runtimes(host, |runtimes| {
        runtimes
            .get(agent)
            .map(|runtime| runtime.supervisor.is_running())
    })
    .unwrap_or(false)
}

/// 当前进程代（诊断用）。
#[must_use]
pub fn runtime_generation(host: &AgentHost, agent: &AgentId) -> Option<u64> {
    try_read_runtimes(host, |runtimes| {
        runtimes.get(agent).map(|runtime| runtime.generation())
    })
}

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
