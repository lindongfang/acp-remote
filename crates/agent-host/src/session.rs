//! 会话端点：一个 ACP 会话到 `SessionEndpoint` 的完整映射。
//!
//! 生命周期与不变量：
//!
//! - 一个会话同时最多一个 active turn；第二个 `prompt` 直接拒绝（总线忙由 core 判，这里只防重复派发）。
//! - turn 终态（完成/失败/取消）**只产生一次事件**；进程中途退出与响应到达这两条路径都汇入
//!   [`AcpSession::finish_turn`]，由状态里的 `turn_running` 保证唯一。
//! - 交互（权限/elicitation）不代答、不超时伪造结论：登记原始 ACP `id` 后一直等待
//!   `resolve_interaction`，回传时逐字使用原来的 id 类型与字面量。
//! - 能力门控：Agent 未宣告的能力不发消息、直接返回显式不支持。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use acp_core::model::{
    AgentSessionId, ConfigOption, ConfigValue, DispatchContext, ElicitationAction, EndpointEvent,
    EventKind, EventType, InteractionId, InteractionKind, InteractionResolution, ModeId, ModeRef,
    ModeState, OwnedSessionRef, PortError, PromptRequest, PublicError, SessionId, SessionReference,
    Timestamp, TurnId, ViewJson,
};
use acp_core::ports::{
    AttachmentStore, Clock, EventSink, HistoryPage, HistoryQuery, IdGenerator, SessionEndpoint,
    TurnAccepted,
};
use acp_protocol::capability::AgentCapabilities;
use acp_protocol::message::{self, PermissionOutcome, RequestPermissionResponse};
use acp_protocol::terminal::{CreateTerminalRequest, CreateTerminalResponse, TerminalRefRequest};
use acp_protocol::{Envelope, IdLiteral};
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::error::HostError;
use crate::limits;
use crate::mapper::{self, UpdateState};
use crate::process::Supervisor;
use crate::release::{ReleaseAction, ReleaseRefusal};
use crate::terminal::{TerminalConfig, TerminalService};

/// 已登记但尚未解析的交互。
#[derive(Debug, Clone)]
struct PendingInteraction {
    /// Agent 发来的原始 `id`（回传时必须原样使用）。
    id: IdLiteral,
    /// 类别（决定回传形状与校验）。
    kind: InteractionKind,
}

/// 准备（临时）端点的选项复核状态（`design.md` D4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparationGate {
    /// 不是准备端点（`session.create`/`resume`/`open` 的普通绑定）：行为与之前完全一致。
    NotPreparation,
    /// 临时端点，尚未读取并复核当前选项。
    Unverified,
    /// 选项已由 Agent 确认（未修改时即 Agent 的默认值）：允许转正与首次派发。
    Ready,
    /// 请求的模式/配置已不可用：保留在报告里可见，但**阻止**派发（不静默回退默认值）。
    Blocked,
}

/// 会话的内部状态（一把锁保护，锁内不做 I/O）。
#[derive(Debug)]
struct Inner {
    /// 本地绑定不再接受新工作。
    ///
    /// 它**不是**「Agent 侧已释放」的证据：重绑/进程级回收也会置位（见 [`AcpSession::close_session`]）。
    closed: bool,
    /// Agent 已确认释放（`session/close` 的 ack），或进程已经退出使资源随之消失。
    ///
    /// 只有这个位能支撑「资源与容量名额可以交还」。已 closed 但没有它的绑定只说明本层让出了这一代。
    released: bool,
    /// 本绑定是否是**准备（临时）端点**：真时不得派发 prompt，直到 [`AcpSession::mark_owned`] 转正。
    temporary: bool,
    /// 准备选项复核状态（普通绑定恒为 [`PreparationGate::NotPreparation`]）。
    preparation: PreparationGate,
    /// 当前 turn 的派发上下文（`design.md` D7）：只在该次 prompt 期间存在，随 turn 终态清空。
    dispatch: Option<DispatchContext>,
    turn_running: bool,
    /// `session/prompt` 的字节已经确认写入 Agent stdin（写确认前不算已派发）。
    turn_dispatched: bool,
    cancel_requested: bool,
    agent_capabilities: AgentCapabilities,
    modes: Option<acp_protocol::message::SessionModeState>,
    config_options: Vec<Value>,
    interactions: HashMap<String, PendingInteraction>,
    updates: UpdateState,
    last_activity: Instant,
}

/// 一个 owned 会话的适配器端点。
pub struct AcpSession {
    reference: SessionReference,
    acp_session_id: String,
    agent_session_id: Option<AgentSessionId>,
    /// 本绑定的资源代（同一 core 会话每次建立/恢复/重绑递增）。
    ///
    /// 迟到的旧代结果（尤其是关闭结果）只能作用于它自己那一代：目录摘除必须核对它，否则一次超时后
    /// 才到达的旧 `session/close` ack 会把新恢复的绑定误删。
    generation: u64,
    supervisor: Arc<Supervisor>,
    sink: EventSink,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    /// 会话工作目录的规范绝对路径（终端默认 cwd 与 prompt 路径引用的边界）。
    workspace: PathBuf,
    /// 附件读取端口；未接线时含 `attachment_ref` 的 prompt 会被整条拒绝。
    attachments: Option<Arc<dyn AttachmentStore>>,
    /// 终端服务（重绑时把同一实例交给新绑定，旧绑定得到空替身）。
    terminal: Mutex<Arc<TerminalService>>,
    inner: Arc<Mutex<Inner>>,
}

// Mapping and delivery share this lock with terminal output. A concurrent terminal
// entry must close the text fragment before the next ACP notification is mapped.
fn ordered_terminal_sink(inner: Arc<Mutex<Inner>>, sink: EventSink) -> EventSink {
    EventSink::new(move |event| {
        let mut inner = lock(&inner);
        inner.updates.interrupt();
        sink.send(event);
    })
}

impl std::fmt::Debug for AcpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpSession")
            .field("acp_session_id", &self.acp_session_id)
            .field("closed", &self.is_closed())
            .field("turn_running", &self.turn_running())
            .finish_non_exhaustive()
    }
}

/// 建立一个 endpoint 所需的全部输入（由组合层一次性给出，避免长参数表）。
pub(crate) struct SessionInit {
    /// core 已分配的会话标识。
    pub session: SessionId,
    /// 本端点绑定的 ACP 会话标识（创建时来自 `session/new`，恢复时来自持久化记录）。
    pub acp_session_id: String,
    /// 同上标识的 core 形状（`None` = Agent 给的标识取不出合法值，core 因此两列都不写）。
    pub agent_session_id: Option<AgentSessionId>,
    /// 本绑定的资源代（由组合层按 core 会话递增；重绑/恢复得到新代）。
    pub generation: u64,
    /// 该 Agent 的进程监督者。
    pub supervisor: Arc<Supervisor>,
    /// core 的事件出口。
    pub sink: EventSink,
    /// 标识分配器（messageId/interactionId 由适配器提供）。
    pub ids: Arc<dyn IdGenerator>,
    /// 时钟。
    pub clock: Arc<dyn Clock>,
    /// 本绑定是否是准备（临时）端点：真时不得派发 prompt，转正后仍沿用同一 Agent 端点。
    pub temporary: bool,
    /// `initialize` 协商出的 Agent 能力（能力门控判据）。
    pub capabilities: AgentCapabilities,
    /// `session/new` 给出的模式状态（`None` = 未宣告）。
    pub modes: Option<acp_protocol::message::SessionModeState>,
    /// `session/new` 给出的配置项原始对象。
    pub config_options: Vec<Value>,
    /// 会话工作目录的规范绝对路径（创建来自 `request.workspace`，恢复来自持久化的 cwd）。
    pub workspace: PathBuf,
    /// 附件读取端口（组合根注入；未接线时显式拒绝附件输入）。
    pub attachments: Option<Arc<dyn AttachmentStore>>,
    /// 是否已宣告 `clientCapabilities.terminal`（未宣告时不接受 `terminal/*`）。
    pub terminal_enabled: bool,
}

impl AcpSession {
    /// 构造（由 [`crate::host::AgentHost`] 使用；`acp_session_id` 已由 `session/new` 协商完成）。
    pub(crate) fn new(init: SessionInit) -> Self {
        let SessionInit {
            session,
            acp_session_id,
            agent_session_id,
            generation,
            supervisor,
            sink,
            ids,
            clock,
            temporary,
            capabilities,
            modes,
            config_options,
            workspace,
            attachments,
            terminal_enabled,
        } = init;
        let inner = Arc::new(Mutex::new(Inner {
            closed: false,
            released: false,
            temporary,
            preparation: if temporary {
                PreparationGate::Unverified
            } else {
                PreparationGate::NotPreparation
            },
            dispatch: None,
            turn_running: false,
            turn_dispatched: false,
            cancel_requested: false,
            agent_capabilities: capabilities,
            modes,
            config_options,
            interactions: HashMap::new(),
            updates: UpdateState::default(),
            last_activity: Instant::now(),
        }));
        let terminal = Mutex::new(Arc::new(TerminalService::new(TerminalConfig {
            session: session.clone(),
            workspace: workspace.clone(),
            clock: Arc::clone(&clock),
            supervisor: Arc::clone(&supervisor),
            sink: ordered_terminal_sink(Arc::clone(&inner), sink.clone()),
            enabled: terminal_enabled,
        })));
        Self {
            reference: SessionReference::Owned(OwnedSessionRef::new(session)),
            acp_session_id,
            agent_session_id,
            generation,
            supervisor,
            sink,
            ids,
            clock,
            workspace,
            attachments,
            terminal,
            inner,
        }
    }

    /// ACP 会话标识（路由用）。
    #[must_use]
    pub fn acp_session_id(&self) -> &str {
        &self.acp_session_id
    }

    /// ACP 会话标识的 core 形状（未取得时 `None`）：由 core 落盘，本层**不**编造占位值。
    #[must_use]
    pub fn agent_session_id(&self) -> Option<&AgentSessionId> {
        self.agent_session_id.as_ref()
    }

    /// core 会话标识。
    #[must_use]
    pub fn session_id(&self) -> &SessionId {
        self.reference.session_id()
    }

    /// 本绑定的资源代（同一 core 会话每次建立/恢复/重绑递增）。
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// 距最后一次活动的时长（资源事实，只读；判定由外部驱动）。
    #[must_use]
    pub fn last_activity_elapsed(&self) -> Duration {
        lock(&self.inner).last_activity.elapsed()
    }

    /// 是否已经关闭。
    #[must_use]
    pub fn is_closed(&self) -> bool {
        lock(&self.inner).closed
    }

    /// 是否已**确认真实释放**（Agent ack 了 `session/close`，或进程已退出使资源消失）。
    ///
    /// 与 [`AcpSession::is_closed`] 的区别是语义，不是程度：`closed` 也可能是「本层把这一代让给了
    /// 新绑定」。只有本方法为真时，释放端口才可以交还容量名额。
    #[must_use]
    pub fn is_released(&self) -> bool {
        lock(&self.inner).released
    }

    /// 空闲回收判据：**没有进行中的 turn** 且空闲时间超过 `timeout`。
    ///
    /// 已关闭的会话恒为假：调用方必须先把它们排除在判定之外（`AgentRuntime::all_sessions_reclaimable`
    /// 只统计仍打开的会话），否则 `Endpoint::close()` 这类已关闭绑定会让 runtime 永不空闲。
    #[must_use]
    pub fn is_idle_for(&self, timeout: Duration) -> bool {
        let inner = lock(&self.inner);
        !inner.closed && !inner.turn_running && inner.last_activity.elapsed() >= timeout
    }

    fn touch(&self) {
        lock(&self.inner).last_activity = Instant::now();
    }

    fn emit(&self, event: EndpointEvent) {
        let mut inner = lock(&self.inner);
        if !matches!(
            event.event_type.as_str(),
            "agent.message.delta" | "agent.thought.delta" | "user.message.delta"
        ) {
            inner.updates.interrupt();
        }
        self.sink.send(event);
    }

    /// 发起一次 turn。
    ///
    /// 返回 [`TurnAccepted`] 之前先等待 `session/prompt` **真的写进** Agent 的 stdin：入队成功不算
    /// 接受。写入失败时回滚 turn 状态、不派发 `finish_turn`，并返回明确错误。
    ///
    /// turn 本身不设超时：它的结束由响应到达或进程退出决定。
    ///
    /// 内容映射（媒体、路径引用、ACP 帧预算）在**派发前**完成：任何一块失败（能力未宣告、超出
    /// 1 MiB 帧预算、路径逃逸、附件读取/摘要不符）都让整条 prompt 失败，**一个字节也不发**，
    /// 也不会留下残缺的 prompt 或半截附件。
    pub async fn prompt(
        self: &Arc<Self>,
        request: PromptRequest,
        at: Timestamp,
    ) -> Result<TurnAccepted, HostError> {
        self.prompt_with_context(request, at, None).await
    }

    /// 带本次派发上下文的 prompt（`design.md` D7）：上下文只在该次 turn 期间存在，用户输入回显据此
    /// 关联到已保存的首条消息；普通 prompt 传 `None`，事件上不会出现任何关联字段。
    pub async fn prompt_with_context(
        self: &Arc<Self>,
        request: PromptRequest,
        _at: Timestamp,
        context: Option<DispatchContext>,
    ) -> Result<TurnAccepted, HostError> {
        let capabilities = {
            let mut inner = lock(&self.inner);
            if inner.closed {
                return Err(HostError::SessionClosed);
            }
            if inner.temporary {
                // 准备端点只用于读取真实选项与转正；准备期间**绝不**发送 prompt（不靠空消息促使
                // Agent 写盘，也不伪造一个 turn）。
                return Err(HostError::PreparationNotReady {
                    detail: "temporary endpoint is not owned yet",
                });
            }
            match inner.preparation {
                PreparationGate::NotPreparation | PreparationGate::Ready => {}
                PreparationGate::Unverified => {
                    return Err(HostError::PreparationNotReady {
                        detail: "options are not verified",
                    });
                }
                PreparationGate::Blocked => {
                    return Err(HostError::PreparationNotReady {
                        detail: "requested options are unavailable",
                    });
                }
            }
            if inner.turn_running {
                // 同一会话不能有两个 active turn（`AGENTS.md` §4）。
                return Err(HostError::AgentRejected {
                    code: -32000,
                    message: "会话已有进行中的 turn".to_owned(),
                });
            }
            inner.turn_running = true;
            inner.cancel_requested = false;
            inner.updates.begin_turn();
            inner.last_activity = Instant::now();
            inner.dispatch = context;
            inner.agent_capabilities.clone()
        };

        let encoded = crate::prompt::encode_prompt(
            &request,
            &capabilities,
            self.attachments.as_deref(),
            &self.workspace,
        )
        .await;
        let prompt = match encoded {
            Ok(prompt) => prompt,
            Err(error) => {
                // 整条拒绝：turn 状态回滚，调用方看到明确失败，Agent 侧什么也没收到。
                let mut inner = lock(&self.inner);
                inner.turn_running = false;
                inner.dispatch = None;
                return Err(error);
            }
        };

        let params = json!({
            "sessionId": self.acp_session_id,
            "prompt": prompt,
        });
        let receiver = match self
            .supervisor
            .begin_request_confirmed("session/prompt", &params)
            .await
        {
            Ok(receiver) => receiver,
            Err(error) => {
                // 字节没有真正写入 stdin：回滚 turn 状态，不派发 `finish_turn`。
                let mut inner = lock(&self.inner);
                inner.turn_running = false;
                inner.dispatch = None;
                return Err(error);
            }
        };
        lock(&self.inner).turn_dispatched = true;

        let session = Arc::clone(self);
        self.supervisor.spawn_owned(async move {
            session.finish_turn(receiver.await).await;
        });

        Ok(TurnAccepted {
            // 占位：`SessionEndpoint::prompt` 的签名不接收 core 的 `TurnId`，因此这里只保证格式合法，
            // **不具权威性**（core 用自己分配的值，见 `crates/core/src/broker.rs`）。
            turn: self.ids.turn_id(),
        })
    }

    /// 派发前预检：与 [`Self::prompt`] 使用同一套内容映射/能力/帧预算校验，但不改变 turn 状态。
    pub async fn validate_prompt(&self, request: &PromptRequest) -> Result<(), HostError> {
        if lock(&self.inner).closed {
            return Err(HostError::SessionClosed);
        }
        let capabilities = lock(&self.inner).agent_capabilities.clone();
        crate::prompt::encode_prompt(
            request,
            &capabilities,
            self.attachments.as_deref(),
            &self.workspace,
        )
        .await
        .map(|_| ())
    }

    /// 结束 turn：把响应/失败收敛成**唯一**一个终态事件。
    async fn finish_turn(
        self: Arc<Self>,
        outcome: Result<Result<Value, HostError>, oneshot::error::RecvError>,
    ) {
        let (event_type, error) = match outcome {
            Ok(Ok(value)) => {
                match serde_json::from_value::<acp_protocol::message::PromptResponse>(value) {
                    Ok(response) if response.stop_reason == "cancelled" => ("turn.cancelled", None),
                    Ok(_) => ("turn.completed", None),
                    Err(failure) => (
                        "turn.failed",
                        mapper::public_error(
                            "internal.unavailable",
                            &format!("无法解码 session/prompt 响应：{failure}"),
                            false,
                        ),
                    ),
                }
            }
            Ok(Err(error)) => ("turn.failed", error_error(&error)),
            Err(_) => (
                "turn.failed",
                mapper::public_error("internal.unavailable", "请求通道已关闭", true),
            ),
        };

        {
            let mut inner = lock(&self.inner);
            if !inner.turn_running {
                // 进程退出路径已经产生过终态（或本就无 turn）：终态唯一，这里不再发事件。
                return;
            }
            inner.turn_running = false;
            inner.turn_dispatched = false;
            inner.cancel_requested = false;
            inner.dispatch = None;
            inner.last_activity = Instant::now();
        }

        let at = self.clock.now();
        if let Ok(event) = mapper::turn_event(event_type, error.as_ref(), at) {
            self.emit(event);
        }
    }

    /// 进程退出时的收敛：进行中的 turn 必须以明确失败结束，并且只结束一次。
    ///
    /// 终端任务也在这里回收：它们虽然由 Daemon spawn，但归属于这个会话；Agent 崩溃不留下孤儿进程树。
    pub fn on_agent_exit(&self, status: &str) {
        self.terminal().shutdown_sync();
        let is_turn = {
            let inner = lock(&self.inner);
            // 写确认前进程退出：prompt 从未真正到达 Agent，不得伪造一个 turn 终态。
            inner.turn_running && inner.turn_dispatched
        };
        if !is_turn {
            // 没有可收敛的 turn 时也必须丢掉该次派发上下文：旧代的关联不得留给后续事件。
            lock(&self.inner).dispatch = None;
            return;
        }
        let error = mapper::public_error(
            "internal.unavailable",
            &format!("Agent 进程退出（{status}）"),
            true,
        );
        let at = self.clock.now();
        {
            let mut inner = lock(&self.inner);
            if !inner.turn_running {
                return;
            }
            inner.turn_running = false;
            inner.turn_dispatched = false;
            inner.dispatch = None;
        }
        if let Ok(event) = mapper::turn_event("turn.failed", error.as_ref(), at) {
            self.emit(event);
        }
    }

    /// 处理一条来自 Agent 的信封（通知或 agent→client 请求）。
    ///
    /// 需要等待的终端请求（`wait_for_exit`/`release`）由监督者收养的任务异步处理：路由循环不能
    /// 被一个长任务阻塞——否则同一 Agent 的 `session/update` 与其它请求都会停在这里。
    pub fn handle_envelope(self: &Arc<Self>, envelope: &Envelope) {
        self.touch();
        match envelope.method() {
            Some("session/update") => self.handle_update(envelope),
            Some("session/request_permission") => self.handle_permission(envelope),
            Some("elicitation/create") => self.handle_elicitation(envelope),
            Some(
                method @ ("terminal/create"
                | "terminal/output"
                | "terminal/wait_for_exit"
                | "terminal/kill"
                | "terminal/release"),
            ) => self.handle_terminal(envelope, method),
            Some(method) => {
                // 未实现/未登记的 agent→client 请求：显式不支持，**不静默丢弃**。
                if let Some(id) = envelope.id() {
                    let _ = self.supervisor.send_error_response(
                        id,
                        message::CODE_METHOD_NOT_FOUND,
                        &format!("{method} 不受支持"),
                    );
                }
                tracing::warn!(method, "收到不支持的方法（已显式拒绝）");
            }
            None => {}
        }
    }

    /// 终端服务方法的分发。
    ///
    /// 形状不合法的请求得到 `-32602`；未宣告 `clientCapabilities.terminal` 时得到 `-32601`
    /// （等价于方法未找到）；未知/跨会话的终端标识得到 `-32602`。错误响应只携带协议元数据，
    /// 不含命令行、参数、环境变量值或路径。
    fn handle_terminal(self: &Arc<Self>, envelope: &Envelope, method: &str) {
        let Some(id) = envelope.id().cloned() else {
            tracing::warn!(method, "terminal 请求没有 id（无法回包）");
            return;
        };
        let params = match envelope.params() {
            Ok(params) => params,
            Err(error) => {
                tracing::warn!(method, error = %error, "terminal 请求缺少 params");
                let _ = self.supervisor.send_error_response(
                    &id,
                    message::CODE_INVALID_PARAMS,
                    "terminal 请求缺少参数",
                );
                return;
            }
        };
        let service = self.terminal();
        if !service.is_enabled() {
            // 未宣告的能力：等价于方法未找到（不暴露「有服务但没开」之外的任何细节）。
            let _ = self.supervisor.send_error_response(
                &id,
                message::CODE_METHOD_NOT_FOUND,
                "terminal 能力未宣告",
            );
            return;
        }

        match method {
            "terminal/create" => {
                let request = match CreateTerminalRequest::from_params(&params) {
                    Ok(request) => request,
                    Err(error) => {
                        tracing::warn!(method, error = %error, "terminal/create 参数不合法");
                        let _ = self.supervisor.send_error_response(
                            &id,
                            message::CODE_INVALID_PARAMS,
                            "terminal/create 参数不合法",
                        );
                        return;
                    }
                };
                if request.session_id != self.acp_session_id {
                    let _ = self.supervisor.send_error_response(
                        &id,
                        message::CODE_INVALID_PARAMS,
                        "请求指向未知会话",
                    );
                    return;
                }
                match service.create(&request) {
                    Ok(terminal_id) => {
                        let response = CreateTerminalResponse {
                            terminal_id,
                            meta: None,
                        };
                        match serde_json::to_value(&response) {
                            Ok(value) => send_terminal_response(&self.supervisor, &id, &value),
                            Err(_) => {
                                let _ = self.supervisor.send_error_response(
                                    &id,
                                    message::CODE_INTERNAL_ERROR,
                                    "terminal/create 响应无法编码",
                                );
                            }
                        }
                    }
                    Err(error) => send_terminal_error(&self.supervisor, &id, &error),
                }
            }
            "terminal/output" => match TerminalRefRequest::from_params(&params) {
                Ok(reference) => {
                    // 预算按实际 id 计算：保留正文与 1 MiB 服务上限相等时，JSON 转义后的响应会超帧。
                    let budget = crate::terminal::result_frame_budget(&id);
                    match service.output(&reference.terminal_id, budget) {
                        Ok(response) => match serde_json::to_value(&response) {
                            Ok(value) => send_terminal_response(&self.supervisor, &id, &value),
                            Err(_) => {
                                let _ = self.supervisor.send_error_response(
                                    &id,
                                    message::CODE_INTERNAL_ERROR,
                                    "terminal/output 响应无法编码",
                                );
                            }
                        },
                        Err(error) => send_terminal_error(&self.supervisor, &id, &error),
                    }
                }
                Err(error) => {
                    tracing::warn!(method, error = %error, "terminal/output 参数不合法");
                    let _ = self.supervisor.send_error_response(
                        &id,
                        message::CODE_INVALID_PARAMS,
                        "terminal/output 参数不合法",
                    );
                }
            },
            "terminal/kill" => match TerminalRefRequest::from_params(&params) {
                Ok(reference) => {
                    if let Err(error) = service.kill(&reference.terminal_id) {
                        send_terminal_error(&self.supervisor, &id, &error);
                    } else {
                        send_terminal_response(&self.supervisor, &id, &json!({}));
                    }
                }
                Err(error) => {
                    tracing::warn!(method, error = %error, "terminal/kill 参数不合法");
                    let _ = self.supervisor.send_error_response(
                        &id,
                        message::CODE_INVALID_PARAMS,
                        "terminal/kill 参数不合法",
                    );
                }
            },
            "terminal/wait_for_exit" | "terminal/release" => {
                let reference = match TerminalRefRequest::from_params(&params) {
                    Ok(reference) => reference,
                    Err(error) => {
                        tracing::warn!(method, error = %error, "terminal 请求参数不合法");
                        let _ = self.supervisor.send_error_response(
                            &id,
                            message::CODE_INVALID_PARAMS,
                            "terminal 请求参数不合法",
                        );
                        return;
                    }
                };
                let session = Arc::clone(self);
                let method = method.to_owned();
                self.supervisor.spawn_owned(async move {
                    let outcome = if method == "terminal/wait_for_exit" {
                        session
                            .terminal()
                            .wait_for_exit(&reference.terminal_id)
                            .await
                            .and_then(|response| {
                                serde_json::to_value(&response).map_err(|_| {
                                    HostError::TerminalRejected {
                                        detail: "响应无法编码".to_owned(),
                                    }
                                })
                            })
                    } else {
                        session
                            .terminal()
                            .release(&reference.terminal_id)
                            .await
                            .map(|()| json!({}))
                    };
                    match outcome {
                        Ok(value) => {
                            send_terminal_response(&session.supervisor, &id, &value);
                        }
                        Err(error) => send_terminal_error(&session.supervisor, &id, &error),
                    }
                });
            }
            _ => {
                // 上面的 match 已经穷尽了会进入本函数的五个方法；不可达分支保持显式拒绝。
                let _ = self.supervisor.send_error_response(
                    &id,
                    message::CODE_METHOD_NOT_FOUND,
                    "terminal 方法不受支持",
                );
            }
        }
    }

    /// 当前终端服务实例（`open` 重绑会把同一实例交给新绑定）。
    fn terminal(&self) -> Arc<TerminalService> {
        Arc::clone(&lock(&self.terminal))
    }

    fn handle_update(&self, envelope: &Envelope) {
        let notification = match message::session_notification(envelope) {
            Ok(notification) => notification,
            Err(error) => {
                tracing::warn!(error = %error, "无法解码 session/update（记为协议错误）");
                return;
            }
        };
        if notification.session_id != self.acp_session_id {
            tracing::warn!("session/update 属于其它会话，忽略");
            return;
        }
        let mut inner = lock(&self.inner);
        let events = {
            let mut updates = std::mem::take(&mut inner.updates);
            // 派发上下文只在本次 turn 内存在，且**只**在适配器里用于回显关联；它不写进任何事件视图
            // （普通 prompt 的上下文是 `None`，行为与本字段出现之前完全相同）。
            let result = mapper::update_events(
                &notification.update,
                envelope.document(),
                &mut updates,
                self.ids.as_ref(),
                self.clock.as_ref(),
                inner.dispatch.as_ref(),
            );
            inner.updates = updates;
            result
        };
        match events {
            Ok(events) => {
                for event in events {
                    self.sink.send(event);
                }
            }
            Err(error) => tracing::warn!(error = %error, "无法映射 session/update"),
        }
    }

    fn handle_permission(&self, envelope: &Envelope) {
        let params = match envelope.params() {
            Ok(params) => params,
            Err(error) => {
                tracing::warn!(error = %error, "权限请求缺少 params");
                return;
            }
        };
        let Ok(request) = serde_json::from_value::<message::RequestPermissionRequest>(params)
        else {
            tracing::warn!("权限请求无法解码");
            return;
        };
        let Some(id) = envelope.id().cloned() else {
            tracing::warn!("权限请求没有 id");
            return;
        };
        let interaction_id = self.ids.interaction_id();
        let raw = mapper::acp_raw(envelope.document()).ok();
        let options = mapper::interaction_options(&request.options);
        if options.is_empty() {
            // 没有可选项的权限请求无法被解析；显式拒绝而不是让它悬挂。
            let _ = self.supervisor.send_error_response(
                &id,
                message::CODE_INVALID_PARAMS,
                "权限请求没有候选选项",
            );
            return;
        }
        let at = self.clock.now();
        let event = match mapper::permission_event(
            &interaction_id,
            &request.tool_call,
            &request.options,
            raw,
            at,
        ) {
            Ok(event) => event,
            Err(error) => {
                // 交付不出去就绝不登记：登记了却没人知道 interactionId 会让 Agent 永久等待。
                tracing::warn!(error = %error, "权限请求无法映射为事件");
                let _ = self.supervisor.send_error_response(
                    &id,
                    message::CODE_INTERNAL_ERROR,
                    "权限请求无法映射为事件",
                );
                return;
            }
        };
        {
            let mut inner = lock(&self.inner);
            inner.interactions.insert(
                interaction_id.as_str().to_owned(),
                PendingInteraction {
                    id,
                    kind: InteractionKind::Permission,
                },
            );
        }
        self.emit(event);
    }

    fn handle_elicitation(&self, envelope: &Envelope) {
        let params = match envelope.params() {
            Ok(params) => params,
            Err(error) => {
                tracing::warn!(error = %error, "elicitation 请求缺少 params");
                return;
            }
        };
        let Ok(request) = message::ElicitationRequest::from_params(&params) else {
            tracing::warn!("elicitation 请求无法解码");
            return;
        };
        let Some(id) = envelope.id().cloned() else {
            tracing::warn!("elicitation 请求没有 id");
            return;
        };
        // elicitation 一律交给人/上层作答：不代答、不用超时伪造结论。
        let interaction_id = self.ids.interaction_id();
        let raw = mapper::acp_raw(envelope.document()).ok();
        let at = self.clock.now();
        let event = match mapper::elicitation_event(&interaction_id, &request, raw, at) {
            Ok(event) => event,
            Err(error) => {
                tracing::warn!(error = %error, "elicitation 请求无法映射为事件");
                let _ = self.supervisor.send_error_response(
                    &id,
                    message::CODE_INTERNAL_ERROR,
                    "elicitation 请求无法映射为事件",
                );
                return;
            }
        };
        {
            let mut inner = lock(&self.inner);
            inner.interactions.insert(
                interaction_id.as_str().to_owned(),
                PendingInteraction {
                    id,
                    kind: InteractionKind::Elicitation,
                },
            );
        }
        self.emit(event);
    }

    /// 取消当前 turn（通知，不需要响应）。
    pub fn cancel_turn(&self, turn: Option<TurnId>) -> Result<(), HostError> {
        let _ = turn;
        {
            // 用户/客户端活动：空闲时钟必须覆盖出站活动，否则「只改模式/只取消」的会话会被回收。
            self.touch();
            let mut inner = lock(&self.inner);
            if inner.closed {
                return Err(HostError::SessionClosed);
            }
            if !inner.turn_running {
                // 没有进行中的 turn：取消是幂等空操作（不伪造终态）。
                return Ok(());
            }
            inner.cancel_requested = true;
        }
        self.supervisor.send_notification(
            "session/cancel",
            &json!({ "sessionId": self.acp_session_id }),
        )
    }

    /// 解析一个交互。
    ///
    /// 应答必须先**真的写进** Agent 的 stdin 才产生 `*.resolved`：入队成功不等于送达。写入失败时
    /// 登记被放回（交互没有凭空消失），调用方拿到明确错误，core 会让它保持仲裁未确认。
    pub async fn resolve(
        &self,
        interaction: &InteractionId,
        resolution: InteractionResolution,
    ) -> Result<(), HostError> {
        // 交互应答也是活动：未解析的交互是 Agent 在等人类，这一时刻显然不是「空闲」。
        self.touch();
        let pending = lock(&self.inner)
            .interactions
            .remove(interaction.as_str())
            .ok_or(HostError::UnknownInteraction)?;
        let result = match (pending.kind, resolution.clone()) {
            (InteractionKind::Permission, InteractionResolution::Permission(decision)) => {
                let response = RequestPermissionResponse {
                    outcome: PermissionOutcome::Selected {
                        option_id: decision.option_id().to_owned(),
                    },
                    meta: None,
                };
                match serde_json::to_value(&response) {
                    Ok(value) => {
                        self.supervisor
                            .send_response_confirmed(&pending.id, &value)
                            .await
                    }
                    Err(_) => Err(HostError::IdUnavailable),
                }
            }
            (InteractionKind::Permission, InteractionResolution::Elicitation { .. }) => {
                Err(HostError::InvalidResolution)
            }
            (
                InteractionKind::Elicitation,
                InteractionResolution::Elicitation { action, values },
            ) => {
                let action = match action {
                    ElicitationAction::Submit => acp_protocol::message::ElicitationAction::Accept,
                    ElicitationAction::Decline => acp_protocol::message::ElicitationAction::Decline,
                    ElicitationAction::Cancel => acp_protocol::message::ElicitationAction::Cancel,
                };
                let content = if values.is_null() {
                    None
                } else {
                    serde_json::from_str::<Value>(&values.to_json_text()).ok()
                };
                let response = acp_protocol::message::ElicitationResponse { action, content };
                self.supervisor
                    .send_response_confirmed(&pending.id, &response.to_result())
                    .await
            }
            (InteractionKind::Elicitation, InteractionResolution::Permission(_)) => {
                Err(HostError::InvalidResolution)
            }
        };
        if result.is_err() {
            // 回传失败时把登记放回，避免「交互凭空消失」；`*.resolved` 也因此不会发出。
            let mut inner = lock(&self.inner);
            inner
                .interactions
                .insert(interaction.as_str().to_owned(), pending);
            return result;
        }
        // ACP 没有「权限/elicitation 已解析」的通知：这是 Daemon 自己的事实，必须在成功回传后
        // 生成事件，否则其他设备与刷新后的历史永远看不到 handled_elsewhere。
        let (event_type, view) = match &resolution {
            InteractionResolution::Permission(decision) => (
                "permission.resolved",
                json!({
                    "interactionId": interaction.as_str(),
                    "resolution": decision.option_id(),
                    "resolvedByDeviceId": null,
                }),
            ),
            InteractionResolution::Elicitation { action, .. } => (
                "elicitation.resolved",
                json!({
                    "interactionId": interaction.as_str(),
                    "action": action.as_str(),
                    "resolvedByDeviceId": null,
                }),
            ),
        };
        if let (Ok(event_type), Ok(view)) =
            (EventType::new(event_type), ViewJson::new(&view.to_string()))
        {
            self.emit(EndpointEvent::view_only(
                EventKind::Structured,
                event_type,
                view,
                None,
                None,
                self.clock.now(),
            ));
        }
        result
    }

    /// 用同一进程上的既有协商结果建立新的 endpoint 绑定（重开或替换 sink）。
    ///
    /// 交互与 turn 状态**不继承**：旧绑定已经让出，未解析的交互随旧绑定作废（不代答）。资源代由
    /// 调用方给出并严格递增：旧绑定随后的迟到结果（例如关闭 ack）只能作用于旧代。
    pub(crate) fn rebind(&self, sink: EventSink, generation: u64) -> Self {
        let inner = lock(&self.inner);
        // 终端服务随 ACP 会话一起存活：新绑定接管同一实例并把事件出口指向新 sink；旧绑定拿到
        // 空替身，因此旧端点即使被关闭也不会误杀新绑定上的终端。
        let new_inner = Arc::new(Mutex::new(Inner {
            closed: false,
            released: false,
            temporary: false,
            preparation: PreparationGate::NotPreparation,
            dispatch: None,
            turn_running: false,
            turn_dispatched: false,
            cancel_requested: false,
            agent_capabilities: inner.agent_capabilities.clone(),
            modes: inner.modes.clone(),
            config_options: inner.config_options.clone(),
            interactions: HashMap::new(),
            updates: UpdateState::default(),
            last_activity: Instant::now(),
        }));
        drop(inner);
        let enabled = self.terminal().is_enabled();
        let replacement = Arc::new(TerminalService::new(TerminalConfig {
            session: self.session_id().clone(),
            workspace: self.workspace.clone(),
            clock: Arc::clone(&self.clock),
            supervisor: Arc::clone(&self.supervisor),
            sink: ordered_terminal_sink(Arc::clone(&new_inner), sink.clone()),
            enabled,
        }));
        let terminal = std::mem::replace(&mut *lock(&self.terminal), replacement);
        terminal.set_sink(ordered_terminal_sink(Arc::clone(&new_inner), sink.clone()));
        Self {
            reference: self.reference.clone(),
            acp_session_id: self.acp_session_id.clone(),
            agent_session_id: self.agent_session_id.clone(),
            generation,
            supervisor: Arc::clone(&self.supervisor),
            sink,
            ids: Arc::clone(&self.ids),
            clock: Arc::clone(&self.clock),
            workspace: self.workspace.clone(),
            attachments: self.attachments.clone(),
            terminal: Mutex::new(terminal),
            inner: new_inner,
        }
    }

    /// 标记会话为已确认释放并作废未解析交互（**不**结束终端、不派发 `session/cancel`）。
    ///
    /// 单会话释放用它：只有 Agent ack 之后才允许把本地绑定标记为关闭并摘除映射；`session/close`
    /// 本身已经要求 Agent 取消该会话的进行中工作，因此不再额外发 `session/cancel`。
    /// 同时置位「已确认释放」——这是释放端口可以交还名额的唯一依据。
    pub fn mark_closed(&self) {
        self.mark_closed_inner(true);
    }

    /// 本地让出这一代（重绑）：置 `closed` 并作废进行中的 turn 与未解析交互，但**不**置
    /// `released`——此时没有任何 Agent 侧的释放证据，释放端口必须继续按「占用未确认」处理
    /// （不交还容量名额），直到真的 `session/close` ack 或进程退出。
    pub fn mark_yielded(&self) {
        self.mark_closed_inner(false);
    }

    fn mark_closed_inner(&self, released: bool) {
        let mut inner = lock(&self.inner);
        inner.closed = true;
        // 只有 Agent ack（或进程退出）才是释放证据；本地让出不得冒充。
        inner.released |= released;
        // 无论是否已确认释放，迟到的旧代响应都不得再产出属于本绑定的 turn 终态事件。
        inner.turn_running = false;
        inner.turn_dispatched = false;
        inner.cancel_requested = false;
        inner.dispatch = None;
        // 未解析的交互随会话一起作废：不代答、不伪造结论，登记直接丢弃。
        inner.interactions.clear();
    }

    /// 关闭会话：先取消进行中的 turn，再让出 endpoint（进程由目录层统一回收）。
    ///
    /// 这是**进程级**关闭（空闲回收、Daemon 停止、运行时丢弃）与**重绑让出**的入口：它不调用 ACP
    /// `session/close`，也不声称 Agent 侧已释放——它只置本地 `closed`，**不**置 `released`。已有确认
    /// 的释放事实不会被它抹掉。终端进程树在这里**同步**结束（Job 句柄关闭/进程组结束是内核操作），
    /// 等待者随即被唤醒；需要排空已读输出与 join 驱动任务的路径调用
    /// [`AcpSession::shutdown_terminals`]。
    pub fn close_session(&self) {
        let was_running = {
            let mut inner = lock(&self.inner);
            inner.closed = true;
            // 未解析的交互随会话一起作废：不代答、不伪造结论，登记直接丢弃。
            inner.interactions.clear();
            inner.dispatch = None;
            inner.turn_running
        };
        self.terminal().shutdown_sync();
        if was_running {
            let _ = self.supervisor.send_notification(
                "session/cancel",
                &json!({ "sessionId": self.acp_session_id }),
            );
        }
    }

    /// 关闭会话并等待终端驱动任务把已读输出交付完（有界；会话/Agent/Daemon 关闭路径使用）。
    ///
    /// [`TerminalService`] 的关闭标志是**永久**的：一旦调用，这个实例不再接受任何 `terminal/*`。
    /// 因此「请求释放但 Agent 没有确认、绑定必须保留」的路径不能只靠它收口——失败后调用
    /// [`AcpSession::restore_terminals`] 换入新实例。
    pub async fn shutdown_terminals(&self) {
        self.terminal().shutdown().await;
    }

    /// 释放未完成时恢复终端服务，让保留下来的绑定继续可用。
    ///
    /// 与 [`AcpSession::rebind`] 同口径：换入一个全新的 [`TerminalService`]（同一会话、同一工作目录、
    /// 同一事件出口），已关闭的旧实例随之丢弃——它的终端进程已经在关闭顺序里结束，不会泄漏。
    ///
    /// 绑定若已被别的路径让出（进程级关闭/回收），这里**不再**复活终端：那一刻本层已经放弃了这一代，
    /// 恢复只会给一个正在收敛的运行时装上新服务。
    pub(crate) fn restore_terminals(&self) {
        if self.is_closed() {
            return;
        }
        let previous = self.terminal();
        if !previous.is_closed() {
            return;
        }
        let replacement = Arc::new(TerminalService::new(TerminalConfig {
            session: self.session_id().clone(),
            workspace: self.workspace.clone(),
            clock: Arc::clone(&self.clock),
            supervisor: Arc::clone(&self.supervisor),
            sink: ordered_terminal_sink(Arc::clone(&self.inner), self.sink.clone()),
            enabled: previous.is_enabled(),
        }));
        *lock(&self.terminal) = replacement;
    }

    /// 模式状态（未宣告时为空结果，不编造候选）。
    #[must_use]
    pub fn mode_state(&self) -> ModeState {
        let inner = lock(&self.inner);
        let Some(modes) = &inner.modes else {
            return ModeState::new(None, Vec::new());
        };
        // 当前模式的展示名同样来自 Agent（`availableModes` 里同 id 的 `name`）；Agent 没有列出当前
        // 模式时退回 id，**不**编造一个展示名。
        let current_name = modes
            .available_modes
            .iter()
            .find(|mode| mode.id == modes.current_mode_id)
            .map_or_else(|| modes.current_mode_id.clone(), |mode| mode.name.clone());
        let current = ModeRef::try_new(
            match ModeId::new(&modes.current_mode_id) {
                Ok(id) => id,
                Err(_) => return ModeState::new(None, Vec::new()),
            },
            &current_name,
        )
        .ok();
        let available = modes
            .available_modes
            .iter()
            .filter_map(|mode| ModeRef::try_new(ModeId::new(&mode.id).ok()?, &mode.name).ok())
            .collect();
        ModeState::new(current, available)
    }

    /// 更新模式状态（`set_mode` 成功后由 host 调用）。
    pub(crate) fn note_mode(&self, mode_id: &str) {
        let mut inner = lock(&self.inner);
        if let Some(modes) = inner.modes.as_mut() {
            modes.current_mode_id = mode_id.to_owned();
        }
    }

    /// 更新配置项的当前值（`set_config` 成功后由 host 调用）。
    ///
    /// 只改 Agent 已经确认的那一项的 `currentValue`；值形状来自本次请求（布尔或选择值），
    /// **不**伪造 Agent 没有返回的新选项。
    pub(crate) fn note_config(&self, id: &acp_core::model::ConfigOptionId, value: &ConfigValue) {
        let raw = match value {
            ConfigValue::Boolean(flag) => json!(flag),
            ConfigValue::Select(selected) => json!(selected.as_str()),
            // 线上没有文本取值的形状，`set_config` 不会发出这类请求。
            ConfigValue::Text(_) => return,
        };
        let mut inner = lock(&self.inner);
        for option in &mut inner.config_options {
            if option.get("id").and_then(Value::as_str) != Some(id.as_str()) {
                continue;
            }
            if let Some(object) = option.as_object_mut() {
                object.insert("currentValue".to_owned(), raw);
            }
            return;
        }
    }

    /// 是否是准备（临时）端点（未转正时不得派发 prompt）。
    #[must_use]
    pub fn is_temporary(&self) -> bool {
        lock(&self.inner).temporary
    }

    /// 记录选项复核结果：没有不可用项 ⇒ `Ready`，否则 `Blocked`（保留报告可见，但阻止派发）。
    pub(crate) fn note_preparation(&self, blocked: bool) {
        let mut inner = lock(&self.inner);
        if inner.preparation == PreparationGate::NotPreparation {
            return;
        }
        inner.preparation = if blocked {
            PreparationGate::Blocked
        } else {
            PreparationGate::Ready
        };
    }

    /// 把准备端点转正（首次提交所有权转移）：之后同一端点即可用于首次派发，**不**新建 Agent 会话。
    ///
    /// 只允许在选项已由 Agent 确认（[`PreparationGate::Ready`]）且未关闭/未释放时转正；不可用的选择
    /// 会阻止转正，而不是静默回退默认值。
    pub(crate) fn mark_owned(&self) -> Result<(), HostError> {
        let mut inner = lock(&self.inner);
        if inner.closed || inner.released {
            return Err(HostError::SessionClosed);
        }
        if !inner.temporary {
            return Err(HostError::PreparationNotReady {
                detail: "endpoint is already owned",
            });
        }
        match inner.preparation {
            PreparationGate::Ready => {
                inner.temporary = false;
                Ok(())
            }
            PreparationGate::Blocked => Err(HostError::PreparationNotReady {
                detail: "requested options are unavailable",
            }),
            PreparationGate::Unverified | PreparationGate::NotPreparation => {
                Err(HostError::PreparationNotReady {
                    detail: "options are not verified",
                })
            }
        }
    }

    /// 配置项（保留 Agent 给的原始对象；core 的 `ConfigOption` 形状只在能如实映射时生成）。
    #[must_use]
    pub fn raw_config_options(&self) -> Vec<Value> {
        lock(&self.inner).config_options.clone()
    }

    /// 配置项（core 形状）。
    #[must_use]
    pub fn config_options(&self) -> Vec<ConfigOption> {
        self.raw_config_options()
            .iter()
            .filter_map(config_option)
            .collect()
    }

    /// 能力门控：Agent 是否宣告了某个能力路径。
    #[must_use]
    pub fn declares(&self, path: &str) -> bool {
        lock(&self.inner)
            .agent_capabilities
            .declared_capability_paths()
            .contains(&path)
    }

    /// 是否有进行中的 turn。
    #[must_use]
    pub fn turn_running(&self) -> bool {
        lock(&self.inner).turn_running
    }

    /// 尚未解析的交互个数。
    #[must_use]
    pub fn pending_interactions(&self) -> usize {
        lock(&self.inner).interactions.len()
    }
}

/// 发送终端成功响应；发送失败时必须显式回一条错误响应，不能静默丢弃让请求悬挂。
///
/// 典型失败是编码后的帧超过 ACP 上限（`HostError::Protocol(Oversize)`）：调用方已按帧预算裁剪
/// `terminal/output`，这里的兜底保证「即使预算计算与编码器有出入」也不会让 Agent 等到超时。
/// 错误响应本身很小；若连它都发不出去（Agent 已退出、stdout 已关闭），已经没有可回话的通道。
fn send_terminal_response(supervisor: &Supervisor, id: &IdLiteral, result: &Value) {
    if let Err(error) = supervisor.send_response(id, result) {
        tracing::warn!(error = %error, "terminal 响应发送失败（改回显式错误）");
        let _ = supervisor.send_error_response(
            id,
            message::CODE_INTERNAL_ERROR,
            "terminal 响应发送失败",
        );
    }
}

/// 终端请求失败的 JSON-RPC 错误响应。
///
/// 响应只带协议元数据性的固定文案：命令行、参数、环境变量值与路径都不回传给 Agent，
/// 也不进日志（它本来就知道自己发了什么；这些字段可能被当作敏感内容记录）。
fn send_terminal_error(supervisor: &Supervisor, id: &IdLiteral, error: &HostError) {
    let (code, message) = match error {
        HostError::CapabilityNotDeclared { .. } => {
            (message::CODE_METHOD_NOT_FOUND, "terminal 能力未宣告")
        }
        HostError::UnknownTerminal => (message::CODE_INVALID_PARAMS, "未知终端"),
        HostError::SessionClosed => (message::CODE_INVALID_PARAMS, "会话已关闭"),
        HostError::InvalidEnvName | HostError::EnvNotAllowed { .. } => {
            (message::CODE_INVALID_PARAMS, "环境变量不合法")
        }
        HostError::TerminalRejected { .. } => (message::CODE_INTERNAL_ERROR, "终端任务不可用"),
        _ => (message::CODE_INTERNAL_ERROR, "终端请求失败"),
    };
    tracing::warn!(code, "terminal 请求失败（已显式拒绝）");
    let _ = supervisor.send_error_response(id, code, message);
}

/// Agent 错误 → 公开错误（只使用已登记的错误码）。
fn error_error(error: &HostError) -> Option<PublicError> {
    let code = match error {
        HostError::AgentRejected { code, .. } if *code == message::CODE_METHOD_NOT_FOUND => {
            "capability.unsupported_by_agent"
        }
        HostError::CapabilityNotDeclared { .. } => "capability.unsupported_by_agent",
        _ => "internal.unavailable",
    };
    let retryable = matches!(
        error,
        HostError::AgentExited { .. } | HostError::NotRunning | HostError::Timeout { .. }
    );
    mapper::public_error(code, &error.to_string(), retryable)
}

/// Agent 给的配置项原始对象 → core `ConfigOption`（形状不匹配时返回 `None`，绝不编造）。
fn config_option(value: &Value) -> Option<ConfigOption> {
    use acp_core::model::{ConfigOptionEntry, ConfigOptionId, ConfigOptionKind, SelectValue};
    let id = ConfigOptionId::new(value.get("id")?.as_str()?).ok()?;
    let name = value.get("name")?.as_str()?;
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let category = value
        .get("category")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let kind = value.get("type").and_then(Value::as_str)?;
    let current = value.get("currentValue")?;
    let (kind, current) = match kind {
        "boolean" => (
            ConfigOptionKind::Boolean,
            ConfigValue::Boolean(current.as_bool()?),
        ),
        "select" => (
            ConfigOptionKind::Select,
            ConfigValue::select(current.as_str()?).ok()?,
        ),
        _ => return None,
    };
    let entries: Vec<ConfigOptionEntry> = value
        .get("options")
        .and_then(Value::as_array)
        .map(|options| {
            options
                .iter()
                .filter_map(|option| {
                    let entry_value = SelectValue::new(option.get("value")?.as_str()?).ok()?;
                    ConfigOptionEntry::try_new(
                        entry_value,
                        option.get("name")?.as_str()?,
                        option
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    )
                    .ok()
                })
                .collect()
        })
        .unwrap_or_default();
    ConfigOption::try_new(id, name, description, category, kind, current, entries).ok()
}

/// `SessionEndpoint` 的包装：turn 的等待任务需要 `Arc<AcpSession>`，而 trait 方法只有 `&self`。
pub struct Endpoint {
    session: Arc<AcpSession>,
    /// 组合层提供的真实释放操作（停终端 → 能力门控的 `session/close` → 成功后摘除映射）。
    release: Arc<dyn BindingControl>,
}

/// 组合层注入给端点的会话级释放操作。
///
/// 端点不认识进程目录，也不允许自行猜测「Agent 是否已释放」；它把关闭事实委托给唯一持有
/// `Supervisor`、协商能力与映射的实现（`AgentHost` 的运行时）。
#[async_trait::async_trait]
pub(crate) trait BindingControl: Send + Sync {
    /// 执行真实清理顺序；只有 Agent 确认后才摘除映射并标记关闭。
    ///
    /// 返回值区分「Agent 已确认释放」与「只是本地让出」：core 的回收判定据
    /// [`ReleaseAction::outcome`] 决定是否交还容量名额，不能用 `()` 把两者抹平。
    async fn release(&self, session: &AcpSession) -> Result<ReleaseAction, ReleaseRefusal>;
}

impl Endpoint {
    /// 构造。
    pub(crate) fn new(session: Arc<AcpSession>, release: Arc<dyn BindingControl>) -> Self {
        Self { session, release }
    }

    /// 底层会话（目录层用于回收与路由）。
    pub fn session(&self) -> &Arc<AcpSession> {
        &self.session
    }

    /// 本绑定的资源代（诊断与跨层事实报告用）。
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.session.generation()
    }

    /// 执行一次真实释放并返回 core 口径的结论（与 [`SessionEndpoint::close`] 同一顺序）。
    ///
    /// 与 `close` 的差别只在结果形状：`close` 把未完成映射成 `PortError`，这里保留
    /// [`ReleaseOutcome`] 的三态，供 core 的逐会话回收判定是否交还容量名额。
    pub(crate) async fn release_outcome(&self) -> crate::release::ReleaseOutcome {
        match self.release.release(&self.session).await {
            Ok(action) => action.outcome(),
            Err(refusal) => refusal.outcome,
        }
    }
}

#[async_trait::async_trait]
impl SessionEndpoint for Endpoint {
    fn reference(&self) -> SessionReference {
        self.session.reference.clone()
    }

    fn agent_session_id(&self) -> Option<&AgentSessionId> {
        self.session.agent_session_id()
    }

    async fn prompt(
        &self,
        request: PromptRequest,
        at: Timestamp,
    ) -> Result<TurnAccepted, PortError> {
        self.session
            .prompt(request, at)
            .await
            .map_err(|error| error.to_port_error())
    }

    async fn prompt_with_context(
        &self,
        request: PromptRequest,
        at: Timestamp,
        context: Option<DispatchContext>,
    ) -> Result<TurnAccepted, PortError> {
        self.session
            .prompt_with_context(request, at, context)
            .await
            .map_err(|error| error.to_port_error())
    }

    async fn validate_prompt(&self, request: &PromptRequest) -> Result<(), PortError> {
        self.session
            .validate_prompt(request)
            .await
            .map_err(|error| error.to_port_error())
    }

    async fn cancel(&self, turn: Option<TurnId>) -> Result<(), PortError> {
        self.session
            .cancel_turn(turn)
            .map_err(|error| error.to_port_error())
    }

    async fn set_mode(&self, mode: &ModeId) -> Result<(), PortError> {
        // 已让出/旧代的端点不得再改 Agent 状态（新代可能已经占用同一个 ACP 会话标识）。
        if self.session.is_closed() {
            return Err(HostError::SessionClosed.to_port_error());
        }
        // 出站活动：刷新空闲时钟（只改模式、不 prompt 的会话不得在超时前被回收）。
        self.session.touch();
        if self.session.mode_state().available.is_empty() {
            // Agent 没有给过任何可用模式：显式拒绝，**不发消息**（不虚报支持）。
            return Err(HostError::CapabilityNotDeclared {
                capability: "session.modes".to_owned(),
            }
            .to_port_error());
        }
        let params = json!({ "sessionId": self.session.acp_session_id, "modeId": mode.as_str() });
        self.session
            .supervisor
            .request("session/set_mode", &params, limits::SHORT_REQUEST_TIMEOUT)
            .await
            .map_err(|error| error.to_port_error())?;
        self.session.note_mode(mode.as_str());
        let at = self.session.clock.now();
        if let Ok(event) = mapper::view_event_public(
            acp_core::model::EventKind::State,
            "session.mode.changed",
            json!({ "currentModeId": mode.as_str() }),
            at,
        ) {
            self.session.emit(event);
        }
        Ok(())
    }

    async fn list_config(&self) -> Result<Vec<ConfigOption>, PortError> {
        self.session.touch();
        Ok(self.session.config_options())
    }

    async fn modes(&self) -> Result<ModeState, PortError> {
        self.session.touch();
        Ok(self.session.mode_state())
    }

    async fn set_config(
        &self,
        id: &acp_core::model::ConfigOptionId,
        value: ConfigValue,
    ) -> Result<(), PortError> {
        // 同 `set_mode`：旧代/已让出的端点不得改写 Agent 状态。
        if self.session.is_closed() {
            return Err(HostError::SessionClosed.to_port_error());
        }
        // 出站活动：同 `set_mode`，写入配置也算活动。
        self.session.touch();
        let known = self
            .session
            .config_options()
            .iter()
            .any(|option| option.id() == id);
        if !known {
            return Err(HostError::CapabilityNotDeclared {
                capability: "configOptions".to_owned(),
            }
            .to_port_error());
        }
        // `session/set_config_option` 的 `value` 是一个 anyOf：布尔必须写成
        // `{type:"boolean",value:bool}`，其余只能是 `{value:"<id>"}`。裸值两种形状都不匹配。
        let raw = match &value {
            ConfigValue::Boolean(flag) => message::config_value_boolean(*flag),
            ConfigValue::Select(selected) => message::config_value_id(selected.as_str()),
            // 线上没有文本取值的形状（只有 boolean 与 value-id），因此显式拒绝而不是发一个非法请求。
            ConfigValue::Text(_) => {
                return Err(HostError::CapabilityNotDeclared {
                    capability: "configOptions.text".to_owned(),
                }
                .to_port_error());
            }
        };
        let params = json!({
            "sessionId": self.session.acp_session_id,
            "configId": id.as_str(),
            "value": raw,
        });
        self.session
            .supervisor
            .request(
                "session/set_config_option",
                &params,
                limits::SHORT_REQUEST_TIMEOUT,
            )
            .await
            .map_err(|error| error.to_port_error())?;
        // Agent 已确认：当前值以这次请求为准（不重新读 Agent，也不改其它选项）。
        self.session.note_config(id, &value);
        Ok(())
    }

    async fn resolve_interaction(
        &self,
        interaction: &InteractionId,
        resolution: InteractionResolution,
    ) -> Result<(), PortError> {
        self.session
            .resolve(interaction, resolution)
            .await
            .map_err(|error| error.to_port_error())
    }

    async fn read_history(&self, query: HistoryQuery) -> Result<HistoryPage, PortError> {
        // 本 crate 不持有会话历史（历史是 Owner Node 的持久化事实，由 core 的 `SessionStore` 提供）。
        // 返回显式不支持，而不是空页——空页会被误读成「没有历史」。
        let _ = query;
        Err(PortError::InvalidRequest(
            "agent-host 不提供历史查询（历史由 core 的 SessionStore 提供）",
        ))
    }

    async fn close(&self) -> Result<(), PortError> {
        // 真实清理顺序由组合层执行（`BindingControl::release`）：在绑定门内停本地终端 → 能力门控下发
        // `session/close`（有界超时）→ Agent 确认后才摘除映射并标记关闭。失败/超时保留真实资源占用
        // （终端服务换入新实例，绑定仍可继续服务）：本端点不得只置本地 closed 位来伪装成功。
        // 这一代若早已让出（重绑/进程级回收），关闭是幂等的：不发请求、不触碰新代绑定。
        self.release
            .release(&self.session)
            .await
            .map(|_action| ())
            .map_err(|refusal| {
                tracing::warn!(
                    session = %self.session.session_id(),
                    generation = self.session.generation(),
                    outcome = ?refusal.outcome,
                    error = %refusal.error,
                    "会话释放未完成（资源仍占用）"
                );
                refusal.error.to_port_error()
            })
    }
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("session", &self.session.session_id())
            .field("generation", &self.session.generation())
            .finish_non_exhaustive()
    }
}

/// 取锁（中毒时让出内部值：状态没有被 I/O 污染，继续用比 panic 更安全）。
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
