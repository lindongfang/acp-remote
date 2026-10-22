//! 组合层：把 profile 目录、进程监督与会话端点装配成 core 需要的两个出站端口。
//!
//! 所有权与生命周期（`docs/MODULE_ARCHITECTURE.md` §4.5）：
//!
//! - **每个 Agent 一个进程与一个 Job/进程组**：结束某个 Agent 不影响其它 Agent。监督者由本结构持有。
//! - 每个监督者拥有自己的任务集（读 stdout、读 stderr、退出监视、写出、turn 等待、进站路由），
//!   关闭时统一 join；本模块不创建 detached task。
//! - 空闲回收**破坏运行时与映射**：命中的运行时在同一临界区被移出目录、其会话映射被清空，
//!   `open()` 随后必须显式失败（不得把已结束的进程当作活跃端点）；可用性语义不随运行状态变化。
//! - 能力协商按**进程代**缓存：进程换了，缓存即失效。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use acp_core::model::{
    AgentDescriptor, AgentId, AgentProfile, AgentRef, AgentSessionId, Capability, CapabilitySet,
    CreateSessionRequest, PortError, ResourceOrigin, ResumeSessionRequest, SessionId,
    SessionReference, UnavailableKind,
};
use acp_core::ports::{
    AgentCatalog, AttachmentStore, Clock, EventSink, IdGenerator, SessionBackendFactory,
    SessionEndpoint,
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
use crate::node::{self, AgentLifecycle, ExitCause, ExitMark, NodeEvents};
use crate::process::{Inbound, Supervisor};
use crate::release::{ReleaseAction, ReleaseOutcome, ReleaseRefusal};
use crate::session::{AcpSession, BindingControl, Endpoint, SessionInit};

/// 一个 Agent 的运行时（一个进程 + 该进程上的全部会话）。
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
    /// 该 profile 的 Agent 标识（节点级事件里的 `agentId`）。
    agent: AgentId,
    /// 节点级事件的交付口（组合根注入；未接线时交付返回 [`crate::NodeEventError::Unbound`] 并记日志）。
    ///
    /// 与 `AgentHost` 共享同一个 `Arc<RwLock<..>>`：组合根可以在任何时刻接线，已建立的进程实例
    /// 的下一次上报因此也能走到新通道（进程**已经发生过**的连接不会被补报——补报会违反「恰好一次」）。
    node_events: Arc<std::sync::RwLock<NodeEvents>>,
    /// 该进程实例的连接生命周期（`Stopped → Connected → Disconnected` 各一次）。
    lifecycle: std::sync::Mutex<AgentLifecycle>,
    /// 该进程实例是否已被判定退出（断开上报的前置；与 `Supervisor` 的退出标记同源）。
    exit_mark: ExitMark,
    /// 时钟（节点级事件的时间戳）。
    clock: Arc<dyn Clock>,
    /// 绑定变更的串行门（与 [`AgentHost::bindings`] 是同一个实例）。
    ///
    /// `open`/`resume` 用它串行化「读目录 → 让出旧绑定 → 登记新绑定」，逐会话释放也在**整个**关闭
    /// 过程（判定 → 停终端 → `session/close` → 摘除映射）里持有它：否则关闭在途时 `open` 可以把同一个
    /// core 会话重绑到新代，而旧代 ack 到达时目录里已经指向新代，留下一个指向「已被 Agent 释放的会话」
    /// 的新绑定。
    bindings: Arc<Mutex<()>>,
}

impl std::fmt::Debug for AgentRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentRuntime")
            .field("agent", &self.agent)
            .field("generation", &self.generation)
            .field("running", &self.supervisor.is_running())
            .finish_non_exhaustive()
    }
}

impl AgentRuntime {
    fn new(
        supervisor: Arc<Supervisor>,
        generation: u64,
        agent: AgentId,
        node_events: Arc<std::sync::RwLock<NodeEvents>>,
        clock: Arc<dyn Clock>,
        bindings: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            supervisor,
            generation,
            capabilities: std::sync::Mutex::new(None),
            by_acp: std::sync::Mutex::new(HashMap::new()),
            by_core: std::sync::Mutex::new(HashMap::new()),
            last_activity: std::sync::Mutex::new(std::time::Instant::now()),
            agent,
            node_events,
            lifecycle: std::sync::Mutex::new(AgentLifecycle::default()),
            exit_mark: ExitMark::new(),
            clock,
            bindings,
        }
    }

    /// 上报本进程实例的连接（**恰好一次**）。
    ///
    /// `AgentLifecycle::on_spawn` 在 `Stopped` 之外恒为假，因此复用既有进程的路径（`create`/`resume`/
    /// `agent_capabilities` 三条入口都经 `ensure_runtime_tracked` 复用 runtime）不会产生第二条连接。
    fn report_connected(&self) {
        let first = lock(&self.lifecycle).on_spawn();
        if !first {
            return;
        }
        let events = read(&self.node_events).clone();
        if node::report_connected(&events, &self.agent, self.clock.as_ref()).is_err() {
            // `NodeEvents::send` 已经记了「未接线」的 `error` 日志；这里补上「哪个进程实例」的上下文，
            // 让漏接在日志里可定位（事件本身**没有**被交付，也就不会落库）。
            crate::node::log_undelivered("agent.connected", &self.agent);
        }
    }

    /// 上报本进程实例的断开（**恰好一次**，且只在「已判定退出」之后）。
    fn report_disconnected(&self) {
        let oversize = self.supervisor.exit_was_oversize();
        // 先取「应否上报」再判前置：`on_exit` 保证 `Connected → Disconnected` 只成立一次。
        let due = lock(&self.lifecycle).on_exit();
        if !due {
            return;
        }
        let exited = self.exit_mark.take_disconnect() || self.supervisor.has_exited();
        let events = read(&self.node_events).clone();
        if node::report_disconnected(
            &events,
            &self.agent,
            ExitCause::classify(oversize),
            exited,
            self.clock.as_ref(),
        )
        .is_err()
        {
            crate::node::log_undelivered("agent.disconnected", &self.agent);
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

    /// 该运行时下的**全部会话**是否都可回收（唯一允许停止共享进程的判据）。
    ///
    /// 逐会话回收由外部（core 的生命周期目录）驱动；本层只提供事实。只有「所有仍打开的会话都空闲、
    /// 没有进行中的 turn」且进程级时钟也超时，才允许结束这个共享进程——任一会话被查看/有任务/有交互
    /// 时都不能以 A 的回收连带终止 B（`specs/local-agent-host/spec.md` 的「共享进程存在被查看的会话」）。
    ///
    /// 没有活动会话（例如只协商过能力的进程）看进程级时钟；有会话时要求**全部**仍打开的会话都空闲。
    ///
    /// **已关闭的会话不参与判定**：已确认释放（或进程已退出）的绑定不再占用资源，已让出给新代的绑定
    /// 也不再是这一代的事；若把它算进「必须空闲」，只要 `by_acp` 里留着一个已关闭会话，这个 runtime
    /// 就永远不会被空闲回收。
    fn all_sessions_reclaimable(&self, timeout: Duration) -> bool {
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

    /// 只在目录仍指向**本代**绑定时摘除映射（迟到旧代的关闭结果不得移除新绑定）。
    ///
    /// 返回是否真的摘除。判定同时核 `acp_session_id` 与资源代：恢复通常会带回同一个 ACP 会话标识，
    /// 因此标识本身不足以区分两代。
    fn remove_session_if_current(&self, session: &AcpSession) -> bool {
        let current = lock(&self.by_acp)
            .get(session.acp_session_id())
            .map(|current| current.generation());
        if current != Some(session.generation()) {
            return false;
        }
        lock(&self.by_core).remove(session.session_id().as_str());
        lock(&self.by_acp).remove(session.acp_session_id());
        self.touch();
        true
    }

    /// 本进程协商到的能力是否宣告了 `sessionCapabilities.close`。
    fn supports_session_close(&self) -> bool {
        lock(&self.capabilities)
            .as_ref()
            .is_some_and(|capabilities| capabilities.supports_session_close())
    }

    /// 目录当前是否仍指向这条**具体代**的绑定。
    fn is_current_binding(&self, session: &AcpSession) -> bool {
        let acp_id = lock(&self.by_core)
            .get(session.session_id().as_str())
            .cloned();
        acp_id.as_deref() == Some(session.acp_session_id())
            && self
                .session_for_acp(session.acp_session_id())
                .is_some_and(|current| current.generation() == session.generation())
    }

    /// 逐会话的真实释放顺序（本 crate 内唯一实现）。
    ///
    /// **整个顺序在绑定门内完成**（与 `open`/`resume` 是同一把 `bindings` 门）：判定 → 停本地终端 →
    /// `session/close` → 标记与摘除之间不允许任何人把同一个 core 会话重绑到新代。否则关闭在途时的
    /// `open` 会建立一条新绑定，而旧代随后的 ack 因「代不同」不会摘除它，目录里就留下一个指向已被
    /// Agent 释放的会话的新绑定。门的持有时间以 [`limits::SESSION_CLOSE_TIMEOUT`] 为界：一次释放会短暂
    /// 挡住其它会话的绑定变更，这是刻意的取舍（正确性优先于那一次并发度）。
    ///
    /// 1. **Agent 已确认释放**（或进程已退出）⇒ 幂等成功，**不发任何请求**。
    /// 2. 本地已 closed 但**没有**确认 ⇒ [`ReleaseAction::Yielded`]：这一代只在本地让出，Agent 侧是否
    ///    仍占用说不清；不重发请求，也不摘除可能已指向新代的目录条目。
    /// 3. 仍有活动映射但不是当前代 ⇒ **不发任何请求**：旧端点不得关闭新代。
    /// 4. 进程已退出 ⇒ 资源随进程树消失：摘除本代映射并标记关闭（没有任何东西可 ack）。
    /// 5. 未宣告 `sessionCapabilities.close` ⇒ **不发请求**，返回明确失败并保留映射与资源。
    /// 6. 停本地终端（不再接受新的终端工作，驱动任务有界排空）。
    /// 7. 发送有界超时的 `session/close { sessionId }`。只有 ack 成功后才标记关闭并摘除映射；
    ///    超时/拒绝/无法解码都返回错误，映射与真实资源占用保持不变，并**换入新的终端服务**：第 6 步
    ///    已经让旧实例永久关闭，保留绑定就必须还能继续服务 `terminal/*`。
    ///
    /// 本方法**不**终止共享进程：同 Agent 的其它会话可能仍在服务。
    async fn release_bound_session(
        &self,
        session: &AcpSession,
    ) -> Result<ReleaseAction, ReleaseRefusal> {
        let _bindings = self.bindings.lock().await;
        if session.is_released() {
            // Agent 已确认释放（或进程已退出）：重复释放幂等，且不再发任何请求。
            self.remove_session_if_current(session);
            return Ok(ReleaseAction::Released);
        }
        if session.is_closed() {
            if !self.supervisor.is_running() {
                // 进程已经退出：这条已让出的绑定没有任何可释放的外部资源。
                session.mark_closed();
                return Ok(ReleaseAction::Released);
            }
            // 本地已让出（重绑/进程级回收）但没有 Agent 的释放证据：不重发请求，也不摘除目录
            // （其中的条目可能已经指向新代）。
            return Ok(ReleaseAction::Yielded);
        }
        if !self.is_current_binding(session) {
            if !self.supervisor.is_running() {
                // 进程已经退出：这条旧代绑定没有任何可释放的外部资源。
                session.mark_closed();
                return Ok(ReleaseAction::Released);
            }
            // 旧代端点的关闭不得作用到新代绑定（更不得发 session/close 把共享会话真关掉）。
            return Err(ReleaseRefusal::from_error(HostError::SessionClosed));
        }
        if !self.supervisor.is_running() {
            session.mark_closed();
            self.remove_session_if_current(session);
            return Ok(ReleaseAction::Released);
        }
        if !self.supports_session_close() {
            return Err(ReleaseRefusal::from_error(
                HostError::CapabilityNotDeclared {
                    capability: "session/close".to_owned(),
                },
            ));
        }
        session.shutdown_terminals().await;
        let params = match session_close_params(session.acp_session_id()) {
            Ok(params) => params,
            Err(error) => {
                // 请求没有发出去：绑定保留，终端收口必须撤销（否则绑定带着一个永久关闭的服务）。
                session.restore_terminals();
                return Err(ReleaseRefusal::from_error(error));
            }
        };
        match self
            .supervisor
            .request("session/close", &params, limits::SESSION_CLOSE_TIMEOUT)
            .await
        {
            Ok(_) => {
                // Agent 已确认释放：现在才允许让出本地绑定。
                session.mark_closed();
                self.remove_session_if_current(session);
                Ok(ReleaseAction::Released)
            }
            Err(error @ (HostError::AgentExited { .. } | HostError::NotRunning)) => {
                // 进程在关闭请求期间退出：资源随进程树消失是可观察事实，不是猜测的「已释放」。
                tracing::warn!(error = %error, "session/close 期间 Agent 退出（资源随进程消失）");
                session.mark_closed();
                self.remove_session_if_current(session);
                Ok(ReleaseAction::Released)
            }
            Err(error) => {
                // 释放没发生：绑定与真实资源保持占用。终端服务已经在请求前收口，这里换入新实例，
                // 让保留下来的绑定继续接受 `terminal/*`（与 rebind 的口径一致）。
                session.restore_terminals();
                Err(ReleaseRefusal::from_error(error))
            }
        }
    }

    fn sessions(&self) -> Vec<Arc<AcpSession>> {
        lock(&self.by_acp).values().cloned().collect()
    }

    /// 进程退出时收敛：进行中的 turn 必须以明确失败结束（**一次**），能力缓存随之失效，
    /// 并上报本进程实例的节点级断开（**一次**，且不早于退出判定）。
    fn on_exit(&self) {
        // 先在 `Supervisor` 侧标记退出（幂等），再上报：即便退出监视任务被 abort 而没有走到
        // `mark_exit`，这里也让「已退出」成立，断开上报因此永远不会早于它（spec 的顺序要求）。
        let status = self
            .supervisor
            .exit_status()
            .unwrap_or_else(|| "未知".to_owned());
        self.supervisor.mark_exit_if_running(status.clone());
        self.exit_mark.mark();
        for session in self.sessions() {
            session.on_agent_exit(&status);
        }
        lock(&self.capabilities).take();
        self.report_disconnected();
    }
}

#[async_trait::async_trait]
impl BindingControl for AgentRuntime {
    async fn release(&self, session: &AcpSession) -> Result<(), ReleaseRefusal> {
        // 旧代端点的关闭是幂等的：无论是「已确认释放」还是「已在本地让出」，都不再发请求、不破坏新代。
        self.release_bound_session(session).await.map(|_action| ())
    }
}

/// 一个运行期会话的可观察事实（`AgentHost::session_resources` 的条目）。
///
/// 这些字段只描述**本层真的知道**的东西：不包含查看者/租约（它们在 core），也不做「应该回收」的
/// 判断——逐会话回收的决策属于 core 的生命周期目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionResourceFacts {
    /// 所属 Agent。
    pub agent: AgentId,
    /// core 会话标识。
    pub session: SessionId,
    /// ACP 会话标识。
    pub acp_session_id: String,
    /// 绑定资源代。
    pub generation: u64,
    /// 本地绑定是否已关闭（**不**代表 Agent 已确认释放：重绑/回收也会置位）。
    pub closed: bool,
    /// 是否有进行中的 turn。
    pub turn_running: bool,
    /// 距最后一次活动的时长。
    pub last_activity_ago: Duration,
    /// 所属 Agent 进程是否仍在运行。
    pub process_running: bool,
}

/// 实现 [`AgentCatalog`] 与 [`SessionBackendFactory`] 的本地 Agent 组合层。
pub struct AgentHost {
    /// 启动配置声明的 Agent（唯一权威；运行期没有写入路径）。
    profiles: Arc<Vec<AgentProfile>>,
    host_config: HostConfig,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    runtimes: Mutex<HashMap<AgentId, Arc<AgentRuntime>>>,
    generations: std::sync::Mutex<HashMap<AgentId, u64>>,
    /// 每个 core 会话的绑定资源代（建立/恢复/重绑递增；跨进程代续增）。
    binding_generations: std::sync::Mutex<HashMap<SessionId, u64>>,
    /// 绑定变更与逐会话释放的串行门。
    ///
    /// `open` 的重绑与 `resume` 的换绑只覆盖「读目录 → 让出旧绑定 → 登记新绑定」这一小段，**不**覆盖
    /// Agent I/O；逐会话释放则在整个关闭过程（判定 → 停终端 → `session/close` → 摘除映射）里持有它，
    /// 因此关闭在途时不会有并发的重绑基于同一个旧绑定构造出第二个可派发端点，也不会出现「旧代 ack
    /// 到达时目录已指向新代」的窗口。`AgentRuntime` 持有同一个实例（`Arc`），释放由此参与这把门。
    bindings: Arc<Mutex<()>>,
    /// 关闭标志：`shutdown_all()` 先置位，此后 `ensure_runtime` 一律拒绝启动新进程。
    shutting_down: AtomicBool,
    /// 节点级 `agent.connected`/`agent.disconnected` 的交付口（组合根注入）。
    ///
    /// 默认未接线。未接线**不是**静默成功：交付会返回 [`crate::NodeEventError::Unbound`] 并记 `error`
    /// 级日志。本 crate 在没有同步入站面时不发明投递通道，也**不**把节点级事件塞进某个会话的
    /// `EventSink`（那会把它们错误地归属到一个会话上）。
    node_events: Arc<std::sync::RwLock<NodeEvents>>,
    /// 附件读取端口（组合根注入）。未接线时 `attachment_ref` 输入被**整条**拒绝——不把附件降级成
    /// 文本、也不凭附件标识去猜内容。
    attachments: std::sync::Mutex<Option<Arc<dyn AttachmentStore>>>,
}

impl std::fmt::Debug for AgentHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentHost")
            .field("idle_timeout", &self.host_config.idle_timeout())
            .field("node_events", &self.node_events_bound())
            .finish_non_exhaustive()
    }
}

impl AgentHost {
    /// 构造（组合根装配本机实现）。
    #[must_use]
    pub fn new(
        profiles: Arc<Vec<AgentProfile>>,
        host_config: HostConfig,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            profiles,
            host_config,
            ids,
            clock,
            runtimes: Mutex::new(HashMap::new()),
            generations: std::sync::Mutex::new(HashMap::new()),
            binding_generations: std::sync::Mutex::new(HashMap::new()),
            bindings: Arc::new(Mutex::new(())),
            shutting_down: AtomicBool::new(false),
            node_events: Arc::new(std::sync::RwLock::new(NodeEvents::unbound())),
            attachments: std::sync::Mutex::new(None),
        }
    }

    /// 注入节点级事件的交付口（组合根把 core 的节点级提交入口包成 [`NodeEvents`] 后调用）。
    ///
    /// 可在任何时刻调用：已建立的进程实例共享同一个交付口句柄，它们**此后**的断开上报会走到新通道。
    /// （此前已经发生过的连接不会被补报——补报会违反「恰好一次」。）
    #[must_use]
    pub fn with_node_events(self, node_events: NodeEvents) -> Self {
        self.set_node_events(node_events);
        self
    }

    /// 同 [`AgentHost::with_node_events`]，但作用于已构造的实例。
    pub fn set_node_events(&self, node_events: NodeEvents) {
        *write(&self.node_events) = node_events;
    }

    /// 节点级事件的交付口是否已接线（诊断用）。
    #[must_use]
    pub fn node_events_bound(&self) -> bool {
        read(&self.node_events).is_bound()
    }

    /// 注入附件读取端口（组合根用 `AttachmentStore` 实现；测试可注入 fake）。
    ///
    /// 未注入时，含 `attachment_ref` 的 prompt 会被**整条**拒绝（`HostError::PromptRejected`）：
    /// 没有可读的字节就绝不把附件当成文本或凭空编造内容发给 Agent。
    #[must_use]
    pub fn with_attachments(self, attachments: Arc<dyn AttachmentStore>) -> Self {
        self.set_attachments(attachments);
        self
    }

    /// 同 [`AgentHost::with_attachments`]，但作用于已构造的实例。
    pub fn set_attachments(&self, attachments: Arc<dyn AttachmentStore>) {
        *lock(&self.attachments) = Some(attachments);
    }

    fn attachments(&self) -> Option<Arc<dyn AttachmentStore>> {
        lock(&self.attachments).clone()
    }

    /// 分配下一个绑定资源代（同一 core 会话严格递增；跨进程代仍单调）。
    fn next_binding_generation(&self, session: &SessionId) -> u64 {
        let mut generations = lock(&self.binding_generations);
        let next = generations.entry(session.clone()).or_insert(0);
        *next += 1;
        *next
    }

    /// 逐会话释放端口（core 的 `SessionRelease` 适配入口）。
    ///
    /// 语义：`Released` 只来自 Agent 对 `session/close` 的确认（或进程已退出、资源随进程树消失）；
    /// `Failed`/`Uncertain` 表示资源仍占用，调用方**不得**交还容量名额。本方法只释放这一条会话绑定，
    /// 绝不终止共享进程——同 Agent 的其它会话继续可用。
    ///
    /// **本地已 closed 不等于已释放**：重绑与进程级回收都会把绑定标记为 closed，但那一刻没有任何
    /// Agent 侧的确认（见 [`crate::session::AcpSession::is_released`]）。对这样的绑定本方法返回
    /// `Failed`——真实资源可能已由新代占用，交还名额会让同一个会话被重复计入。
    ///
    /// 没有活动绑定时返回 `Released`：本层已经没有该会话的资源可释放（重复调用幂等）。若调用期间
    /// 该会话被重绑/恢复成新代，返回 `Failed`——旧代端点不得对新代绑定执行破坏性关闭，调用方应针对
    /// 新代重新判定。
    #[must_use]
    pub async fn release_session(&self, session: &SessionId) -> ReleaseOutcome {
        match self.find_binding(session).await {
            None => ReleaseOutcome::Released,
            Some((runtime, binding)) => Self::release_binding(&runtime, &binding).await,
        }
    }

    /// 带资源代的逐会话释放（core `SessionRelease::release_session(session, generation, now)` 的适配）。
    ///
    /// 只有目录里的当前绑定仍是该代时才允许发送 `session/close`；若期间已重绑/恢复成新代，返回
    /// `Failed` 且**不触碰**新代绑定——迟到的释放请求不得关闭一个刚建立的新资源。
    #[must_use]
    pub async fn release_session_at_generation(
        &self,
        session: &SessionId,
        generation: u64,
    ) -> ReleaseOutcome {
        match self.find_binding(session).await {
            None => ReleaseOutcome::Released,
            Some((runtime, binding)) if binding.generation() == generation => {
                Self::release_binding(&runtime, &binding).await
            }
            Some((_, binding)) => {
                tracing::warn!(
                    session = %session,
                    expected = generation,
                    current = binding.generation(),
                    "释放请求属于旧资源代（已重绑），不作用于新绑定"
                );
                ReleaseOutcome::Failed
            }
        }
    }

    /// 对一条已解析到的绑定执行释放并收敛为端口语义。
    ///
    /// 「已在本地让出、但没有 Agent 确认」的绑定映射成 [`ReleaseOutcome::Failed`]：资源仍可能被新代
    /// 占用，交还名额会让同一个会话被重复计入（`SessionEndpoint::close` 对同一状态仍是幂等成功）。
    async fn release_binding(runtime: &AgentRuntime, binding: &AcpSession) -> ReleaseOutcome {
        match runtime.release_bound_session(binding).await {
            Ok(action) => {
                let outcome = action.outcome();
                if outcome != ReleaseOutcome::Released {
                    tracing::warn!(
                        session = %binding.session_id(),
                        generation = binding.generation(),
                        "绑定已在本地让出，但没有 Agent 确认的释放（资源与名额保持占用）"
                    );
                }
                outcome
            }
            Err(refusal) => {
                tracing::warn!(
                    session = %binding.session_id(),
                    generation = binding.generation(),
                    outcome = ?refusal.outcome,
                    error = %refusal.error,
                    "单会话释放未完成（资源与名额保持占用）"
                );
                refusal.outcome
            }
        }
    }

    /// 当前运行期会话的事实快照（只报告，不做回收决定）。
    ///
    /// 逐会话回收由 core 的生命周期目录驱动；本层只如实给出「是否仍打开、是否有进行中的 turn、
    /// 距最后一次活动多久、所属进程是否仍在运行、绑定资源代」这些可观察事实。
    pub async fn session_resources(&self) -> Vec<SessionResourceFacts> {
        let runtimes: Vec<(AgentId, Arc<AgentRuntime>)> = self
            .runtimes
            .lock()
            .await
            .iter()
            .map(|(agent, runtime)| (agent.clone(), Arc::clone(runtime)))
            .collect();
        let mut facts = Vec::new();
        for (agent, runtime) in runtimes {
            let process_running = runtime.supervisor.is_running();
            for session in runtime.sessions() {
                facts.push(SessionResourceFacts {
                    agent: agent.clone(),
                    session: session.session_id().clone(),
                    acp_session_id: session.acp_session_id().to_owned(),
                    generation: session.generation(),
                    closed: session.is_closed(),
                    turn_running: session.turn_running(),
                    last_activity_ago: session.last_activity_elapsed(),
                    process_running,
                });
            }
        }
        // 稳定输出（并列的回收选择由 core 用稳定 ID 决定；这里只保证报告顺序可复现）。
        facts.sort_by(|left, right| left.session.as_str().cmp(right.session.as_str()));
        facts
    }

    /// 在全部运行时里查一个 core 会话当前的活动绑定。
    async fn find_binding(
        &self,
        session: &SessionId,
    ) -> Option<(Arc<AgentRuntime>, Arc<AcpSession>)> {
        let runtimes: Vec<Arc<AgentRuntime>> =
            self.runtimes.lock().await.values().cloned().collect();
        for runtime in runtimes {
            let acp_id = lock(&runtime.by_core).get(session.as_str()).cloned();
            if let Some(acp_id) = acp_id {
                if let Some(binding) = runtime.session_for_acp(&acp_id) {
                    return Some((runtime, binding));
                }
            }
        }
        None
    }

    /// 空闲回收判据（`None` = 不因空闲关闭）。
    #[must_use]
    pub fn idle_timeout(&self) -> Option<Duration> {
        self.host_config.idle_timeout()
    }

    /// 终端服务是否已按切真实的端到端能力宣告启用。
    ///
    /// 声明与执行用**同一个**判据：`initialize` 里宣告的 `clientCapabilities.terminal` 就是它。
    /// 未宣告时 Agent 发来的 `terminal/*` 一律按「方法未找到」拒绝，绝不偷偷执行命令。
    fn terminal_enabled(&self) -> bool {
        self.host_config.client_capabilities.terminal == Some(true)
    }

    /// 取（必要时启动）某个 Agent 的运行时。
    ///
    /// 一把异步锁串行化「启动 + initialize」：并发调用不会造出两个进程、两个 Job 或两条协商。
    async fn ensure_runtime(&self, agent: &AgentId) -> Result<Arc<AgentRuntime>, HostError> {
        Ok(self.ensure_runtime_tracked(agent).await?.0)
    }

    /// 同 [`AgentHost::ensure_runtime`]，并回报「本次调用是否真的拉起了一个新进程」。
    ///
    /// 恢复的能力门控需要这个区别：能力未宣告时必须回收**本次为恢复而拉起**的子进程，而复用到的
    /// 既有进程可能正在服务其它会话，不得被一次失败的恢复连带结束（`design.md` D4）。
    async fn ensure_runtime_tracked(
        &self,
        agent: &AgentId,
    ) -> Result<(Arc<AgentRuntime>, bool), HostError> {
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
                return Ok((runtime, false));
            }
            // 进程已退出：不跨代复用这个 runtime（也不留残留映射）——先作废它的会话端点与映射，
            // 再按关闭顺序回收整棵树与任务，最后按「不存在」重启（新一代）。
            runtimes.remove(agent);
            for session in runtime.sessions() {
                session.close_session();
                session.shutdown_terminals().await;
            }
            runtime.clear_sessions();
            runtime.supervisor.shutdown().await;
        }

        let profile = self
            .profiles
            .iter()
            .find(|profile| profile.id() == agent)
            .cloned()
            .ok_or(HostError::UnknownProfile)?;
        let spec = launch::resolve_launch(&profile)?;
        let (supervisor, incoming) = Supervisor::start(spec).await?;
        let generation = {
            let mut generations = lock(&self.generations);
            let next = generations.entry(agent.clone()).or_insert(0);
            *next += 1;
            *next
        };
        let runtime = Arc::new(AgentRuntime::new(
            Arc::clone(&supervisor),
            generation,
            agent.clone(),
            Arc::clone(&self.node_events),
            Arc::clone(&self.clock),
            Arc::clone(&self.bindings),
        ));
        // 退出上报钩子：断开事件的唯一触发点（无论哪条路径先观察到退出，钩子只被取走一次）。
        // 它在 `converge_exit` 的顺序下被调用，因此钩子里 `supervisor.is_running()` 必然已为假。
        //
        // 用 `Weak` 而非 `Arc`：钩子由 `Supervisor` 持有，而 `AgentRuntime` 又持有 `Supervisor`，
        // 强引用会构成环（`Supervisor::drop` 的进程树清理因此永不执行）。进程退出时目录与路由任务
        // 都还持有强引用，`upgrade()` 成功；运行时已被让出时钩子是无副作用的空操作（那一刻断开已由
        // 让出路径显式上报过）。
        {
            let reporter = Arc::downgrade(&runtime);
            supervisor.set_exit_reporter(move |_oversize| {
                if let Some(runtime) = reporter.upgrade() {
                    runtime.report_disconnected();
                }
            });
        }

        // 响应与通知共用同一条 FIFO，因此 router 必须在发出 `initialize` **之前**运行；
        // 否则 initialize 的响应会排在一个没有消费者的队列里，启动必然超时。
        // `report_connected` 仍等到 initialize 成功之后（见下）。
        spawn_router(Arc::clone(&runtime), incoming);

        match self.initialize(&runtime, limits::STARTUP_TIMEOUT).await {
            Ok(capabilities) => {
                supervisor.set_negotiated_capabilities(capabilities.clone());
                *lock(&runtime.capabilities) = Some(capabilities);
            }
            Err(error) => {
                // 协商失败：不留下半开的进程（进程树与任务一并回收）。**不上报连接**——该进程从未
                // 可服务会话，因此也就没有连接可上报（`AgentLifecycle::on_spawn` 从未被调用）。
                supervisor.shutdown().await;
                return Err(error);
            }
        }

        runtime.touch();
        // 连接上报在进程**确实可服务会话**之后（`initialize` 成功），且每个进程实例一次。
        // router 已在 initialize 之前启动（响应也走 FIFO）；若进程恰在 initialize 成功与本次
        // 上报之间退出，路由的 `on_exit` 会先看到「尚未上报连接」并把断开当作从未连接——
        // 这个窗口只有一次同步调用，且不会留下可用的半开进程。上报之后再插入运行目录表。
        runtime.report_connected();
        runtimes.insert(agent.clone(), Arc::clone(&runtime));
        Ok((runtime, true))
    }

    /// 让出同一 core 会话在任何运行时里的既有绑定。
    ///
    /// 恢复成功后必须保证「同一 core 会话只有一条活跃绑定」（`specs/local-agent-host/spec.md` 的
    /// 「反复恢复不产生第二个端点」）：旧绑定先停止接受 turn，再摘出映射，随后本次恢复才登记新绑定。
    /// 旧绑定可能落在另一个运行时（同一会话在两个 Agent 进程上都留有映射），因此按目录全扫而不是
    /// 只看目标运行时。
    async fn evict_binding(&self, session: &SessionId) {
        let runtimes: Vec<Arc<AgentRuntime>> =
            self.runtimes.lock().await.values().cloned().collect();
        for runtime in runtimes {
            let existing = lock(&runtime.by_core).get(session.as_str()).cloned();
            let Some(acp_id) = existing else { continue };
            match runtime.session_for_acp(&acp_id) {
                Some(previous) => {
                    previous.close_session();
                    previous.shutdown_terminals().await;
                    runtime.remove_session(&previous);
                }
                // 映射不完整（只有 `by_core` 条目）：只摘掉它，不猜一个端点出来。
                None => {
                    lock(&runtime.by_core).remove(session.as_str());
                }
            }
        }
    }

    /// 回收**本次恢复**拉起的运行时（进程树、映射与会话绑定一并让出）。
    ///
    /// 只在 `spawned` 为真时动手：复用到的既有进程可能正在服务其它会话，不能因为一次恢复失败被
    /// 连带结束；而能力未宣告时规格要求的可观察保证正是「回收本次为恢复而拉起的子进程」。
    async fn discard_spawned_runtime(
        &self,
        agent: &AgentId,
        runtime: &Arc<AgentRuntime>,
        spawned: bool,
    ) {
        if !spawned {
            return;
        }
        {
            let mut runtimes = self.runtimes.lock().await;
            if runtimes
                .get(agent)
                .is_some_and(|current| Arc::ptr_eq(current, runtime))
            {
                runtimes.remove(agent);
            }
        }
        for session in runtime.sessions() {
            session.close_session();
            session.shutdown_terminals().await;
        }
        runtime.clear_sessions();
        runtime.supervisor.shutdown().await;
    }

    /// 结束某个 core 会话所属的 Agent 运行时（窗口期事件队列超限的失败关闭口径）。
    ///
    /// 顺序与 `local-agent-host` 的「超限结束的失败关闭顺序」同源：**先**把该运行时从运行目录里摘掉
    /// （此后按 `AgentId` 复用取不到它，`Supervisor::is_running()` 在 `shutdown()` 置位后即为假），**再**
    /// 让未完成请求以明确结果结束、回收进程树与子任务。与 [`Self::discard_spawned_runtime`] 的唯一差别
    /// 是触发者：那一条服务「本次恢复失败」，本一条服务「这个运行时的会话已不可信」。
    ///
    /// 按**运行时**（= 一个 Agent 进程）回收而不是只按会话：进程是按 `AgentId` 复用与关停的最小单位，
    /// 而「一条超限消息结束一个 Agent」的既有口径同样是整进程处置——复用同一进程的其它会话因此与那份
    /// 口径一致，不会出现「同一个进程一半可用一半不可用」的分裂状态。
    async fn terminate_runtime_of(&self, session: &SessionId) {
        let candidates: Vec<(AgentId, Arc<AgentRuntime>)> = {
            let runtimes = self.runtimes.lock().await;
            runtimes
                .iter()
                .filter(|(_, runtime)| lock(&runtime.by_core).contains_key(session.as_str()))
                .map(|(agent, runtime)| (agent.clone(), Arc::clone(runtime)))
                .collect()
        };
        for (agent, runtime) in candidates {
            {
                let mut runtimes = self.runtimes.lock().await;
                if runtimes
                    .get(&agent)
                    .is_some_and(|current| Arc::ptr_eq(current, &runtime))
                {
                    runtimes.remove(&agent);
                }
            }
            for session in runtime.sessions() {
                session.close_session();
                session.shutdown_terminals().await;
            }
            runtime.clear_sessions();
            runtime.supervisor.shutdown().await;
        }
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
            serde_json::from_value(value).map_err(|error| HostError::MalformedResponse {
                method: "initialize",
                detail: error.to_string(),
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
    /// 只在「该 Agent 下**全部**会话都可回收」时才结束共享进程（逐会话回收由外部驱动，本层只报告
    /// 事实：见 [`AgentHost::session_resources`]）。
    ///
    /// 判定与「移出目录」在**同一个临界区**里完成：不能留下「死 supervisor 还挂在目录里、映射仍可被
    /// `open` 复用」的窗口（Q4-1）。实际的关闭流程（最长 5 s grace）在锁外执行，不阻塞其它入口。
    pub async fn sweep_idle(&self, idle_timeout: Duration) {
        if idle_timeout.is_zero() {
            return;
        }
        let reclaimed: Vec<(Arc<AgentRuntime>, Vec<Arc<AcpSession>>)> = {
            let mut runtimes = self.runtimes.lock().await;
            let idle: Vec<AgentId> = runtimes
                .iter()
                .filter(|(_, runtime)| runtime.all_sessions_reclaimable(idle_timeout))
                .map(|(agent, _)| agent.clone())
                .collect();
            let mut reclaimed = Vec::with_capacity(idle.len());
            for agent in idle {
                if let Some(runtime) = runtimes.remove(&agent) {
                    // 先让端点作废（`close_session` 只改内存状态、不等待），再清空映射：
                    // 回收后的既有会话不得再被 `open` 复用。终端驱动任务在锁外排空，避免长时间持锁。
                    let sessions = runtime.sessions();
                    for session in &sessions {
                        session.close_session();
                    }
                    runtime.clear_sessions();
                    reclaimed.push((runtime, sessions));
                }
            }
            reclaimed
        };
        for (runtime, sessions) in reclaimed {
            for session in sessions {
                session.shutdown_terminals().await;
            }
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
                session.shutdown_terminals().await;
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
                session.shutdown_terminals().await;
            }
            runtime.clear_sessions();
            runtime.supervisor.shutdown().await;
        }
    }
}

/// 进站消息路由：把 Agent 的通知与请求分发给对应会话。
fn spawn_router(runtime: Arc<AgentRuntime>, mut incoming: mpsc::UnboundedReceiver<Inbound>) {
    let supervisor = Arc::clone(&runtime.supervisor);
    supervisor.spawn_owned(async move {
        while let Some(inbound) = incoming.recv().await {
            match inbound {
                Inbound::Notification(envelope) => route(&runtime, &envelope),
                Inbound::Response { id, envelope } => {
                    runtime.supervisor.settle_response(id, &envelope);
                }
            }
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
        let mut descriptors = Vec::with_capacity(self.profiles.len());
        for profile in self.profiles.iter() {
            // 可用性只做只读探测：命令能解析。**不做 spawn**。可用性不随运行状态变化。
            let available = command_resolves(profile.command());
            descriptors.push(AgentDescriptor::new(
                agent_ref(profile)?,
                available,
                ResourceOrigin::Local,
            ));
        }
        Ok(descriptors)
    }

    async fn profile(&self, id: &AgentId) -> Result<Option<AgentProfile>, PortError> {
        Ok(self
            .profiles
            .iter()
            .find(|profile| profile.id() == id)
            .cloned())
    }

    async fn agent_capabilities(&self, agent: &AgentRef) -> Result<CapabilitySet, PortError> {
        let capabilities = self.negotiated(agent.agent_id()).await?;
        Ok(capability_set(&capabilities))
    }
}

#[async_trait::async_trait]
impl SessionBackendFactory for AgentHost {
    /// 结束该会话所属的 Agent 运行时（进程树与映射一并回收，此后不再被复用）。幂等：该会话没有任何
    /// 运行时（已结束、从未建立）时是成功的空操作。
    async fn terminate_agent(&self, session: &SessionId) -> Result<(), PortError> {
        self.terminate_runtime_of(session).await;
        Ok(())
    }

    async fn create(
        &self,
        session: &SessionId,
        request: CreateSessionRequest,
        sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        let agent = request.agent.agent_id().clone();
        // 建立路径的错误映射走 `to_establishment_error`：可证明「Agent 侧没有产生会话」的那一类收敛成
        // `Unavailable(SessionNotEstablished)`，core 据此把终态判为 `failed`（其余一律 `uncertain`）。
        let runtime = self
            .ensure_runtime(&agent)
            .await
            .map_err(|error| error.to_establishment_error())?;
        let capabilities = lock(&runtime.capabilities).clone().unwrap_or_default();

        let params = session_new_params(&request)?;
        // 工作目录的规范绝对路径：终端的默认 cwd 与 prompt 路径引用的边界都用它。
        // `session_new_params` 已经拒绝没有 workspace 的请求，这里只是把取值取出来。
        let workspace = request
            .workspace
            .as_ref()
            .map(|workspace| PathBuf::from(workspace.canonical_path()))
            .ok_or(PortError::InvalidRequest(
                "session/new 需要已解析的 workspace（ACP 要求 cwd）",
            ))?;
        let value = runtime
            .supervisor
            .request("session/new", &params, limits::SHORT_REQUEST_TIMEOUT)
            .await
            .map_err(|error| error.to_establishment_error())?;
        let response: acp_protocol::message::NewSessionResponse = serde_json::from_value(value)
            .map_err(|error| {
                // 请求已发出、Agent 已答复，只是答复形状不合约定：会话**可能**已经建立，
                // 因此这不是「可证明未产生会话」那一类。
                HostError::MalformedResponse {
                    method: "session/new",
                    detail: error.to_string(),
                }
                .to_establishment_error()
            })?;

        // `session/new` 成功返回的 ACP 会话标识：core 用它落盘（§3.6）。取不出合法 `AgentSessionId`
        // （空、超过 512 字符或含 NUL）时给 `None`，由 core 决定两列都不写——**不**编造占位值。
        let agent_session_id = AgentSessionId::new(&response.session_id).ok();
        let generation = self.next_binding_generation(session);
        let session = Arc::new(AcpSession::new(SessionInit {
            session: session.clone(),
            acp_session_id: response.session_id,
            agent_session_id,
            generation,
            supervisor: Arc::clone(&runtime.supervisor),
            sink,
            ids: Arc::clone(&self.ids),
            clock: Arc::clone(&self.clock),
            capabilities,
            modes: response.modes,
            config_options: response.config_options.unwrap_or_default(),
            workspace,
            attachments: self.attachments(),
            terminal_enabled: self.terminal_enabled(),
        }));
        runtime
            .insert_session(&session)
            .map_err(|error| error.to_port_error())?;
        let release: Arc<dyn BindingControl> = runtime;
        Ok(Box::new(Endpoint::new(session, release)))
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
        // 绑定变更串行化：并发 `open` 不得各自基于同一个旧绑定造出两个可派发端点；逐会话释放也在整段
        // 过程里持有这把门，因此「关闭在途」时这里会等释放结束，不会把同一会话重绑到新代。
        let _bindings = self.bindings.lock().await;
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
            // 存在两个会派发 turn 的 endpoint（`AGENTS.md` §4「同一会话最多一个 active turn」）。
            // 新绑定拿到严格递增的资源代：旧端点随后的迟到关闭结果只能作用于旧代。
            let generation = self.next_binding_generation(&owned.session_id);
            let rebound = Arc::new(previous.rebind(sink, generation));
            // 终端服务已转交新绑定；旧绑定只保留「关闭」状态，不能再派发任何请求。这是**本地让出**
            // 而不是确认释放：没有 Agent 的 `session/close` ack，旧代端点随后的释放必须仍报告
            // 「占用未确认」（不得交还容量名额）。
            previous.mark_yielded();
            runtime.remove_session(&previous);
            runtime
                .insert_session(&rebound)
                .map_err(|error| error.to_port_error())?;
            let release: Arc<dyn BindingControl> = runtime;
            return Ok(Box::new(Endpoint::new(rebound, release)));
        }
        Err(PortError::InvalidRequest("没有该会话的活动 endpoint"))
    }

    /// 进程不在时的会话恢复（`session/resume`，`design.md` D4）。
    ///
    /// 顺序就是能力门控的落点：`initialize`（`ensure_runtime` 复用既有启动路径）→ 读**协商到的**
    /// 能力 → 未宣告就到此为止（不发 `session/resume`、不建绑定，并回收本次拉起的子进程）→ 宣告了
    /// 才发送 `session/resume { sessionId, cwd }`。
    ///
    /// 发送的取值**全部来自持久化记录**（`request`）：本层不读存储、不解析路径、也不做任何补齐，
    /// `cwd` 取值逐字来自 `request.workspace_cwd`（不含 workspace 别名）；只有出站参数的**形式**经
    /// `session_resume_params` 转换（剥掉 Windows verbatim 前缀），持久化取值本身不变。
    async fn resume(
        &self,
        session: &SessionId,
        request: ResumeSessionRequest,
        sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        let ResumeSessionRequest {
            agent,
            agent_session_id,
            workspace_cwd,
        } = request;
        let agent_id = agent.agent_id().clone();
        // 与 `create` 同一口径（见那里的注释）：可证明未建立会话的一类收敛成
        // `Unavailable(SessionNotEstablished)`，让 `settle_session_resume` 能把终态判为 `failed`。
        let (runtime, spawned) = self
            .ensure_runtime_tracked(&agent_id)
            .await
            .map_err(|error| error.to_establishment_error())?;
        // 读的是该 runtime 协商并缓存下来的能力：本次拉起进程时 `initialize` 刚协商过；复用既有进程
        // （`spawned == false`）时读到的是**那个进程此前**的协商结果——不重新协商，也不虚报能力。
        let capabilities = lock(&runtime.capabilities).clone().unwrap_or_default();

        if !capabilities.supports_session_resume() {
            // 协商能力只能经 `initialize` 得知，而 `initialize` 必然已经拉起进程；因此这里的可观察
            // 保证是「一个字节都不发」+「回收本次为恢复而拉起的子进程」，不是「不启动进程」。
            self.discard_spawned_runtime(&agent_id, &runtime, spawned)
                .await;
            return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
        }

        let params = session_resume_params(&agent_session_id, &workspace_cwd)?;
        let value = match runtime
            .supervisor
            .request("session/resume", &params, limits::SHORT_REQUEST_TIMEOUT)
            .await
        {
            Ok(value) => value,
            Err(error) => {
                // Agent 拒绝（或进程消失/超时）：明确失败，且不留下本次拉起的进程。分类与 `create`
                // 同口径——Agent 明确拒绝 ⇒ 可证明；超时/掉线 ⇒ 说不清。
                self.discard_spawned_runtime(&agent_id, &runtime, spawned)
                    .await;
                return Err(error.to_establishment_error());
            }
        };
        let response: message::SessionResumeResponse = match serde_json::from_value(value) {
            Ok(response) => response,
            Err(error) => {
                self.discard_spawned_runtime(&agent_id, &runtime, spawned)
                    .await;
                return Err(HostError::MalformedResponse {
                    method: "session/resume",
                    detail: error.to_string(),
                }
                .to_establishment_error());
            }
        };

        // 旧绑定先让出：成功恢复后同一 core 会话只能有一条能派发 turn 的绑定。
        // 这一段与 `open` 共用绑定门（不覆盖上面的 Agent I/O），避免交错产生两个可派发端点；逐会话
        // 释放同样在这把门内完成，因此释放进行中时的换绑会等它结束，而不是在旧代 ack 到达前抢先登记。
        let bindings = self.bindings.lock().await;
        self.evict_binding(session).await;
        let generation = self.next_binding_generation(session);
        let endpoint_session = Arc::new(AcpSession::new(SessionInit {
            session: session.clone(),
            acp_session_id: agent_session_id.as_str().to_owned(),
            // 恢复得到的端点读回的必须是**持久化**的标识（不是新会话标识，也不是占位值）。
            agent_session_id: Some(agent_session_id),
            generation,
            supervisor: Arc::clone(&runtime.supervisor),
            sink,
            ids: Arc::clone(&self.ids),
            clock: Arc::clone(&self.clock),
            capabilities,
            modes: response.modes,
            config_options: response.config_options.unwrap_or_default(),
            workspace: PathBuf::from(&workspace_cwd),
            attachments: self.attachments(),
            terminal_enabled: self.terminal_enabled(),
        }));
        if let Err(error) = runtime.insert_session(&endpoint_session) {
            drop(bindings);
            self.discard_spawned_runtime(&agent_id, &runtime, spawned)
                .await;
            return Err(error.to_port_error());
        }
        drop(bindings);
        let release: Arc<dyn BindingControl> = runtime;
        Ok(Box::new(Endpoint::new(endpoint_session, release)))
    }
}

/// `session/new` 的参数。
///
/// 两个约束都来自 pinned schema（`schemas/acp/v1/upstream/schema.json` 的 `NewSessionRequest`）：
/// `cwd` 与 `mcpServers` 都是**必填**。因此：
///
/// - `cwd` 只能来自 core 已解析的 workspace 路径（本 crate 不读存储、不拼路径、也不拿 Daemon 的当前目录
///   冒充用户选定的 workspace）；core 未解析时显式拒绝，而不是发一个缺必填字段的请求；
/// - 出站的 `cwd` 是**剥掉 Windows verbatim 前缀（`\\?\`）后的形式**：Agent 可能把 `cwd` 当路径片段用
///   （例如派生会话目录名），而 `?` 是 Windows 非法文件名字符。转换只作用于这个出站参数，持久化取值与
///   core 的解析结果都不动；前缀规则的唯一实现是 [`acp_core::strip_verbatim`]（本层不复制判断）；
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
    let cwd = outbound_cwd(workspace.canonical_path());
    Ok(json!({
        "cwd": cwd,
        "mcpServers": [],
    }))
}

/// `session/resume` 的参数。
///
/// 两个字段都是 pinned schema（`schemas/acp/v1/upstream/schema.json` 的 `ResumeSessionRequest`）的
/// **必填**字段，取值逐字来自持久化记录：`sessionId` 是该会话持久化的 ACP 会话标识（一字不改），`cwd`
/// 是持久化的「创建时 canonical path」原文、**其出站字节剥掉 Windows verbatim 前缀（`\\?\`）**（理由同
/// [`session_new_params`]：Agent 可能把它当路径片段用）。持久化取值本身不变，转换只作用于这个出站参数。
/// 字段名与形状取自 `acp-protocol` 的类型化 DTO，本层不另抄一份；路径的解析、存在性与 `canonicalize`
/// 一致性都由 core 在调用本端口**之前**复校验（§5.1）。
fn session_resume_params(
    agent_session_id: &AgentSessionId,
    workspace_cwd: &str,
) -> Result<Value, PortError> {
    let cwd = outbound_cwd(workspace_cwd);
    let request = message::SessionResumeRequest::new(agent_session_id.as_str(), cwd.as_str());
    // 两个字符串字段的编码不会失败；沿用 `initialize` 对「自编码失败」的既有映射。
    serde_json::to_value(&request).map_err(|_| HostError::IdUnavailable.to_port_error())
}

/// `session/close` 的参数。
///
/// 唯一的必填字段 `sessionId` 逐字是该绑定协商到的 ACP 会话标识（来自 `session/new` 或持久化恢复
/// 记录）；字段形状取自 `acp-protocol` 的类型化 DTO，本层不另抄一份。`_meta` 缺省，不会凭空出现。
fn session_close_params(acp_session_id: &str) -> Result<Value, HostError> {
    serde_json::to_value(message::SessionCloseRequest::new(acp_session_id))
        .map_err(|_| HostError::IdUnavailable)
}

/// 出站路径参数的形式转换：剥掉 Windows verbatim 前缀（`acp_core::strip_verbatim`，规则只在 core 一处
/// 实现）。**只**用于发往 Agent 的参数，不回写持久化取值或 core 的解析结果。
fn outbound_cwd(cwd: &str) -> String {
    acp_core::strip_verbatim(PathBuf::from(cwd))
        .to_string_lossy()
        .into_owned()
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

/// 读锁（中毒时让出内部值，口径同 [`lock`]）。
pub(crate) fn read<T>(lock: &std::sync::RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// 写锁（中毒时让出内部值，口径同 [`lock`]）。
fn write<T>(lock: &std::sync::RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 恢复请求的参数逐字来自持久化取值：恰好是 `sessionId` 与 `cwd` 两个必填字段。
    ///
    /// 本用例的取值为无前缀的普通绝对路径，因此 `outbound_cwd` 的形式转换是逐字节直通；带 verbatim
    /// 前缀时的剥离由 `outbound_cwd` 负责（其断言属集成层，见 `tests/`）。
    ///
    /// 键集合也是断言对象：**不得**把 workspace 别名、默认值或任何补齐字段发出去
    /// （`design.md` D3 的 Round 13 订正：恢复路径以持久化取值为权威、不按别名重解析）。
    #[test]
    fn resume_params_carry_the_persisted_values_verbatim() {
        let id = AgentSessionId::new("acp-session-restored").expect("agent session id");
        let params = session_resume_params(&id, r"C:\work\demo").expect("params");
        assert_eq!(
            params,
            json!({ "sessionId": "acp-session-restored", "cwd": "C:\\work\\demo" })
        );
        let mut keys: Vec<&str> = params
            .as_object()
            .expect("对象")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["cwd", "sessionId"],
            "参数只允许这两个键：{params}"
        );
    }

    /// 关闭参数只带 `sessionId`（逐字是该绑定的 ACP 标识），不夹带 `cwd` 或任何补齐字段。
    #[test]
    fn close_params_carry_only_the_agent_session_id() {
        let params = session_close_params("acp-session-1").expect("params");
        assert_eq!(params, json!({ "sessionId": "acp-session-1" }));
        let mut keys: Vec<&str> = params
            .as_object()
            .expect("对象")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["sessionId"], "参数只允许这一个键：{params}");
    }

    // -----------------------------------------------------------------------------------------
    // 释放语义：这些用例需要一条**真的在运行**的 Agent 进程（`is_current_binding` 与
    // `supervisor.is_running()` 的判定对象），因此用同一个 fake ACP child。
    // -----------------------------------------------------------------------------------------

    use acp_core::model::Timestamp;

    struct TestClock;

    impl Clock for TestClock {
        fn now(&self) -> Timestamp {
            Timestamp::new("2026-10-21T00:00:00.000Z").expect("timestamp")
        }
    }

    struct TestIds;

    impl IdGenerator for TestIds {
        fn session_id(&self) -> SessionId {
            SessionId::new("00000000-0000-4000-8000-000000000001").expect("session id")
        }

        fn turn_id(&self) -> acp_core::model::TurnId {
            acp_core::model::TurnId::new("00000000-0000-4000-8000-000000000002").expect("turn id")
        }

        fn interaction_id(&self) -> acp_core::model::InteractionId {
            acp_core::model::InteractionId::new("00000000-0000-4000-8000-000000000003")
                .expect("interaction id")
        }

        fn pairing_id(&self) -> acp_core::model::PairingId {
            acp_core::model::PairingId::new("00000000-0000-4000-8000-000000000004")
                .expect("pairing id")
        }

        fn message_id(&self) -> acp_core::model::MessageId {
            acp_core::model::MessageId::new("00000000-0000-4000-8000-000000000005")
                .expect("message id")
        }

        fn origin_epoch(&self) -> acp_core::model::OriginEpoch {
            acp_core::model::OriginEpoch::new("00000000-0000-4000-8000-000000000006")
                .expect("origin epoch")
        }

        fn request_id(&self) -> acp_core::model::RequestId {
            acp_core::model::RequestId::new("00000000-0000-4000-8000-000000000007")
                .expect("request id")
        }
    }

    /// fake ACP child 的可执行文件（与 `terminal.rs` 的单测同一判据：`cargo test` 必须构建 bin 目标）。
    fn fake_agent_binary() -> PathBuf {
        let mut path = std::env::current_exe().expect("current exe");
        path.pop(); // deps
        path.pop(); // debug|release
        let base = path.join("acpr-fake-acp-agent");
        if base.exists() {
            return base;
        }
        let exe = base.with_extension("exe");
        assert!(
            exe.exists(),
            "找不到 fake ACP Agent 可执行文件（cargo test 必须构建 bin 目标）：{}",
            exe.display()
        );
        exe
    }

    fn launch_spec(program: &std::path::Path) -> crate::launch::LaunchSpec {
        let env = launch::NECESSARY_ENV
            .iter()
            .filter_map(|name| {
                std::env::var(name)
                    .ok()
                    .map(|value| ((*name).to_owned(), value))
            })
            .collect();
        crate::launch::LaunchSpec {
            program: program.to_string_lossy().into_owned(),
            args: Vec::new(),
            env,
        }
    }

    /// 起一个只应答（什么都不做）的 fake Agent，返回运行时与一条已登记的本代绑定。
    async fn runtime_with_binding() -> (Arc<AgentRuntime>, Arc<Supervisor>, Arc<AcpSession>) {
        runtime_with_binding_on(Arc::new(Mutex::new(()))).await
    }

    /// 同上，但运行时与宿主共享同一把绑定门（用于覆盖 `open` 的重绑顺序）。
    async fn runtime_with_binding_on(
        bindings: Arc<Mutex<()>>,
    ) -> (Arc<AgentRuntime>, Arc<Supervisor>, Arc<AcpSession>) {
        let program = fake_agent_binary();
        let (supervisor, _incoming) = Supervisor::start(launch_spec(&program))
            .await
            .expect("启动监督者");
        let runtime = Arc::new(AgentRuntime::new(
            Arc::clone(&supervisor),
            1,
            AgentId::new("agent-1").expect("id"),
            Arc::new(std::sync::RwLock::new(NodeEvents::unbound())),
            Arc::new(TestClock),
            bindings,
        ));
        let session = Arc::new(AcpSession::new(SessionInit {
            session: SessionId::new("11111111-1111-4111-8111-111111111111").expect("session"),
            acp_session_id: "acp-session-1".to_owned(),
            agent_session_id: None,
            generation: 1,
            supervisor: Arc::clone(&supervisor),
            sink: EventSink::new(|_| {}),
            ids: Arc::new(TestIds),
            clock: Arc::new(TestClock),
            capabilities: AgentCapabilities::default(),
            modes: None,
            config_options: Vec::new(),
            workspace: std::env::temp_dir(),
            attachments: None,
            terminal_enabled: false,
        }));
        runtime.insert_session(&session).expect("登记绑定");
        (runtime, supervisor, session)
    }

    /// 本地 closed（重绑与进程级回收走的就是这条）**不是**释放证据：释放端口必须报告未释放。
    ///
    /// 旧实现把「绑定已 closed」直接当成成功，于是 `release_session*` 在 closed→摘除的窗口里会把
    /// 真实仍被占用的资源算作已释放，交还名额后同一个会话可能被重复计入。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_locally_closed_binding_reports_not_released() {
        let (runtime, supervisor, session) = runtime_with_binding().await;

        // ① 本地让出：没有 Agent 的 ack，也没有进程退出。
        session.close_session();
        assert!(session.is_closed() && !session.is_released());
        assert_eq!(
            runtime
                .release_bound_session(&session)
                .await
                .expect("旧端点让出是幂等的"),
            ReleaseAction::Yielded,
            "本地 closed 不能变成「已确认释放」"
        );
        assert_eq!(
            AgentHost::release_binding(&runtime, &session).await,
            ReleaseOutcome::Failed,
            "已 closed 但未确认的绑定不得交还名额"
        );

        // ② Agent 确认（或进程已退出）之后：同样的本地状态，但带确认事实 ⇒ 幂等成功。
        session.mark_closed();
        assert!(session.is_released());
        assert_eq!(
            runtime
                .release_bound_session(&session)
                .await
                .expect("确认后的重复释放必须幂等"),
            ReleaseAction::Released
        );
        assert_eq!(
            AgentHost::release_binding(&runtime, &session).await,
            ReleaseOutcome::Released
        );

        supervisor.shutdown().await;
    }

    /// `open` 先赢得绑定门：重绑把旧代标成「本地让出」，但**没有** Agent 的 `session/close` ack。
    /// 旧代随后的释放必须仍报告「占用未确认」——不得因为本地让出被误记成 released 而交还容量名额；
    /// 旧端点自身的关闭仍是幂等成功（组合层把 `Yielded` 映射成 `Ok`）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_binding_yielded_by_open_is_not_released() {
        let host = AgentHost::new(
            Arc::new(Vec::new()),
            HostConfig::default(),
            Arc::new(TestIds),
            Arc::new(TestClock),
        );
        // 运行时与宿主共享同一把绑定门，重绑顺序与真实 `open` 完全一致。
        let (runtime, supervisor, previous) =
            runtime_with_binding_on(Arc::clone(&host.bindings)).await;
        host.runtimes
            .lock()
            .await
            .insert(AgentId::new("agent-1").expect("id"), Arc::clone(&runtime));
        let session = previous.session_id().clone();
        let old_generation = previous.generation();
        // 真实路径里 `create` 先分配了第 1 代；这里补上同样的记账，`open` 的重绑才会拿到第 2 代。
        assert_eq!(
            host.next_binding_generation(&session),
            old_generation,
            "种子代必须与手工登记的首代一致"
        );

        let reopened = host
            .open(
                SessionReference::Owned(acp_core::model::OwnedSessionRef::new(session.clone())),
                EventSink::new(|_| {}),
            )
            .await
            .expect("open 已有会话");
        assert!(previous.is_closed(), "旧代必须停止派发");
        assert!(
            !previous.is_released(),
            "重绑只是本地让出：没有任何 Agent 的释放证据"
        );

        // 竞态路径：释放请求在重绑前已解析到这条旧代绑定，随后才拿到绑定门。
        assert_eq!(
            runtime
                .release_bound_session(&previous)
                .await
                .expect("旧代让出是幂等的"),
            ReleaseAction::Yielded,
            "本地让出不能变成「已确认释放」"
        );
        assert_eq!(
            AgentHost::release_binding(&runtime, &previous).await,
            ReleaseOutcome::Failed,
            "没有 session/close ack，不得交还容量名额"
        );
        // 对外端口同样不得报告已释放（定向到旧资源代的释放）。
        assert_eq!(
            host.release_session_at_generation(&session, old_generation)
                .await,
            ReleaseOutcome::Failed,
            "旧资源代的释放不得报告已释放"
        );

        // 旧端点自身的关闭仍是幂等成功，且不触碰新代绑定。
        let release = Arc::clone(&runtime) as Arc<dyn BindingControl>;
        let old_endpoint = Endpoint::new(Arc::clone(&previous), release);
        old_endpoint
            .close()
            .await
            .expect("旧代端点的关闭是幂等成功");
        old_endpoint.close().await.expect("重复关闭仍然幂等");
        let current = runtime
            .session_for_acp(previous.acp_session_id())
            .expect("新代绑定仍须在目录里");
        assert_eq!(current.generation(), old_generation + 1);
        assert!(!current.is_closed() && !current.is_released());

        drop(reopened);
        supervisor.shutdown().await;
    }
}
