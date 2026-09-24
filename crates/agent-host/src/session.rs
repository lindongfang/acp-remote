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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use acp_core::model::{
    ConfigOption, ConfigValue, ElicitationAction, EndpointEvent, InteractionId, InteractionKind,
    InteractionResolution, ModeId, ModeRef, ModeState, OwnedSessionRef, PortError,
    PromptContentBlock, PromptRequest, PublicError, SessionId, SessionReference, Timestamp, TurnId,
};
use acp_core::ports::{
    Clock, EventSink, HistoryPage, HistoryQuery, IdGenerator, SessionEndpoint, TurnAccepted,
};
use acp_protocol::capability::AgentCapabilities;
use acp_protocol::message::{self, PermissionOutcome, RequestPermissionResponse};
use acp_protocol::{Envelope, IdLiteral};
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::error::HostError;
use crate::limits;
use crate::mapper::{self, UpdateState};
use crate::process::Supervisor;

/// 已登记但尚未解析的交互。
#[derive(Debug, Clone)]
struct PendingInteraction {
    /// Agent 发来的原始 `id`（回传时必须原样使用）。
    id: IdLiteral,
    /// 类别（决定回传形状与校验）。
    kind: InteractionKind,
}

/// 会话的内部状态（一把锁保护，锁内不做 I/O）。
#[derive(Debug)]
struct Inner {
    closed: bool,
    turn_running: bool,
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
    supervisor: Arc<Supervisor>,
    sink: EventSink,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    inner: Mutex<Inner>,
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
    /// `session/new` 协商出的 ACP 会话标识。
    pub acp_session_id: String,
    /// 该 Agent 的进程监督者。
    pub supervisor: Arc<Supervisor>,
    /// core 的事件出口。
    pub sink: EventSink,
    /// 标识分配器（messageId/interactionId 由适配器提供）。
    pub ids: Arc<dyn IdGenerator>,
    /// 时钟。
    pub clock: Arc<dyn Clock>,
    /// `initialize` 协商出的 Agent 能力（能力门控判据）。
    pub capabilities: AgentCapabilities,
    /// `session/new` 给出的模式状态（`None` = 未宣告）。
    pub modes: Option<acp_protocol::message::SessionModeState>,
    /// `session/new` 给出的配置项原始对象。
    pub config_options: Vec<Value>,
}

impl AcpSession {
    /// 构造（由 [`crate::host::AgentHost`] 使用；`acp_session_id` 已由 `session/new` 协商完成）。
    pub(crate) fn new(init: SessionInit) -> Self {
        let SessionInit {
            session,
            acp_session_id,
            supervisor,
            sink,
            ids,
            clock,
            capabilities,
            modes,
            config_options,
        } = init;
        Self {
            reference: SessionReference::Owned(OwnedSessionRef::new(session)),
            acp_session_id,
            supervisor,
            sink,
            ids,
            clock,
            inner: Mutex::new(Inner {
                closed: false,
                turn_running: false,
                cancel_requested: false,
                agent_capabilities: capabilities,
                modes,
                config_options,
                interactions: HashMap::new(),
                updates: UpdateState::default(),
                last_activity: Instant::now(),
            }),
        }
    }

    /// ACP 会话标识（路由用）。
    #[must_use]
    pub fn acp_session_id(&self) -> &str {
        &self.acp_session_id
    }

    /// core 会话标识。
    #[must_use]
    pub fn session_id(&self) -> &SessionId {
        self.reference.session_id()
    }

    /// 是否已经关闭。
    #[must_use]
    pub fn is_closed(&self) -> bool {
        lock(&self.inner).closed
    }

    /// 空闲回收判据：**没有进行中的 turn** 且空闲时间超过 `timeout`。
    #[must_use]
    pub fn is_idle_for(&self, timeout: Duration) -> bool {
        let inner = lock(&self.inner);
        !inner.closed && !inner.turn_running && inner.last_activity.elapsed() >= timeout
    }

    fn touch(&self) {
        lock(&self.inner).last_activity = Instant::now();
    }

    fn emit(&self, event: EndpointEvent) {
        self.sink.send(event);
    }

    /// 发起一次 turn。
    ///
    /// 立即返回 [`TurnAccepted`]：`session/prompt` 不设超时，turn 的结束由响应到达或进程退出决定。
    pub async fn prompt(
        self: &Arc<Self>,
        request: PromptRequest,
        _at: Timestamp,
    ) -> Result<TurnAccepted, HostError> {
        {
            let mut inner = lock(&self.inner);
            if inner.closed {
                return Err(HostError::SessionClosed);
            }
            if inner.turn_running {
                // 同一会话不能有两个 active turn（`AGENTS.md` §3）。
                return Err(HostError::AgentRejected {
                    code: -32000,
                    message: "会话已有进行中的 turn".to_owned(),
                });
            }
            inner.turn_running = true;
            inner.cancel_requested = false;
            inner.updates.begin_turn();
            inner.last_activity = Instant::now();
        }

        let params = json!({
            "sessionId": self.acp_session_id,
            "prompt": prompt_content(request)?,
        });
        let receiver = match self.supervisor.begin_request("session/prompt", &params) {
            Ok(receiver) => receiver,
            Err(error) => {
                lock(&self.inner).turn_running = false;
                return Err(error);
            }
        };

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
            inner.cancel_requested = false;
            inner.last_activity = Instant::now();
        }

        let at = self.clock.now();
        if let Ok(event) = mapper::turn_event(event_type, error.as_ref(), at) {
            self.emit(event);
        }
    }

    /// 进程退出时的收敛：进行中的 turn 必须以明确失败结束，并且只结束一次。
    pub fn on_agent_exit(&self, status: &str) {
        let is_turn = {
            let inner = lock(&self.inner);
            inner.turn_running
        };
        if !is_turn {
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
        }
        if let Ok(event) = mapper::turn_event("turn.failed", error.as_ref(), at) {
            self.emit(event);
        }
    }

    /// 处理一条来自 Agent 的信封（通知或 agent→client 请求）。
    pub fn handle_envelope(&self, envelope: &Envelope) {
        self.touch();
        match envelope.method() {
            Some("session/update") => self.handle_update(envelope),
            Some("session/request_permission") => self.handle_permission(envelope),
            Some("elicitation/create") => self.handle_elicitation(envelope),
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
        let events = {
            let mut inner = lock(&self.inner);
            let mut updates = std::mem::take(&mut inner.updates);
            let result = mapper::update_events(
                &notification.update,
                envelope.document(),
                &mut updates,
                self.ids.as_ref(),
                self.clock.as_ref(),
            );
            inner.updates = updates;
            result
        };
        match events {
            Ok(events) => {
                for event in events {
                    self.emit(event);
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
    pub fn resolve(
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
        let result = match (pending.kind, resolution) {
            (InteractionKind::Permission, InteractionResolution::Permission(decision)) => {
                let response = RequestPermissionResponse {
                    outcome: PermissionOutcome::Selected {
                        option_id: decision.option_id().to_owned(),
                    },
                    meta: None,
                };
                serde_json::to_value(&response)
                    .map_err(|_| HostError::IdUnavailable)
                    .and_then(|value| self.supervisor.send_response(&pending.id, &value))
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
                    .send_response(&pending.id, &response.to_result())
            }
            (InteractionKind::Elicitation, InteractionResolution::Permission(_)) => {
                Err(HostError::InvalidResolution)
            }
        };
        if result.is_err() {
            // 回传失败时把登记放回，避免「交互凭空消失」。
            let mut inner = lock(&self.inner);
            inner
                .interactions
                .insert(interaction.as_str().to_owned(), pending);
        }
        result
    }

    /// 用同一进程上的既有协商结果建立新的 endpoint 绑定（重开或替换 sink）。
    ///
    /// 交互与 turn 状态**不继承**：旧绑定已经让出，未解析的交互随旧绑定作废（不代答）。
    pub(crate) fn rebind(&self, sink: EventSink) -> Self {
        let inner = lock(&self.inner);
        Self {
            reference: self.reference.clone(),
            acp_session_id: self.acp_session_id.clone(),
            supervisor: Arc::clone(&self.supervisor),
            sink,
            ids: Arc::clone(&self.ids),
            clock: Arc::clone(&self.clock),
            inner: Mutex::new(Inner {
                closed: false,
                turn_running: false,
                cancel_requested: false,
                agent_capabilities: inner.agent_capabilities.clone(),
                modes: inner.modes.clone(),
                config_options: inner.config_options.clone(),
                interactions: HashMap::new(),
                updates: UpdateState::default(),
                last_activity: Instant::now(),
            }),
        }
    }

    /// 关闭会话：先取消进行中的 turn，再让出 endpoint（进程由目录层统一回收）。
    pub fn close_session(&self) {
        let was_running = {
            let mut inner = lock(&self.inner);
            inner.closed = true;
            // 未解析的交互随会话一起作废：不代答、不伪造结论，登记直接丢弃。
            inner.interactions.clear();
            inner.turn_running
        };
        if was_running {
            let _ = self.supervisor.send_notification(
                "session/cancel",
                &json!({ "sessionId": self.acp_session_id }),
            );
        }
    }

    /// 模式状态（未宣告时为空结果，不编造候选）。
    #[must_use]
    pub fn mode_state(&self) -> ModeState {
        let inner = lock(&self.inner);
        let Some(modes) = &inner.modes else {
            return ModeState::new(None, Vec::new());
        };
        let current = ModeRef::try_new(
            match ModeId::new(&modes.current_mode_id) {
                Ok(id) => id,
                Err(_) => return ModeState::new(None, Vec::new()),
            },
            &modes.current_mode_id,
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

/// 把 core 的 prompt 内容转成 ACP `prompt` 数组（逐字保留原始块，不改写结构）。
fn prompt_content(request: PromptRequest) -> Result<Vec<Value>, HostError> {
    request.validate().map_err(|_| HostError::InvalidPrompt)?;
    request
        .content
        .iter()
        .map(|block: &PromptContentBlock| {
            serde_json::from_str::<Value>(block.as_str()).map_err(|_| HostError::InvalidPrompt)
        })
        .collect()
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
#[derive(Debug)]
pub struct Endpoint {
    session: Arc<AcpSession>,
}

impl Endpoint {
    /// 构造。
    pub(crate) fn new(session: Arc<AcpSession>) -> Self {
        Self { session }
    }

    /// 底层会话（目录层用于回收与路由）。
    pub fn session(&self) -> &Arc<AcpSession> {
        &self.session
    }
}

#[async_trait::async_trait]
impl SessionEndpoint for Endpoint {
    fn reference(&self) -> SessionReference {
        self.session.reference.clone()
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

    async fn cancel(&self, turn: Option<TurnId>) -> Result<(), PortError> {
        self.session
            .cancel_turn(turn)
            .map_err(|error| error.to_port_error())
    }

    async fn set_mode(&self, mode: &ModeId) -> Result<(), PortError> {
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
        let raw = match value {
            ConfigValue::Boolean(flag) => message::config_value_boolean(flag),
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
        Ok(())
    }

    async fn resolve_interaction(
        &self,
        interaction: &InteractionId,
        resolution: InteractionResolution,
    ) -> Result<(), PortError> {
        self.session
            .resolve(interaction, resolution)
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
        self.session.close_session();
        Ok(())
    }
}

/// 取锁（中毒时让出内部值：状态没有被 I/O 污染，继续用比 panic 更安全）。
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
