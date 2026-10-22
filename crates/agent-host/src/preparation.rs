//! 准备（临时）端点：没有 `owned_session` 行、真实模式/配置、资源代与首次派发所有权转移。
//!
//! `design.md` D4/D9 的边界在这里落成三个可观察事实：
//!
//! 1. **真实设置**：模式与配置只来自 `session/new` 的响应（由 `AgentHost::prepare` 建立）；本层不缓存
//!    占位值、不编造默认值，也**不**为了取得选项发送任何 prompt。
//! 2. **选项复核**：[`TemporaryEndpoint::apply_selection`] 先读当前选项，再验证/应用草稿选择。请求的
//!    模式/配置已不可用时**保留在报告里可见**并把端点标为 blocked：转正与派发都被拒绝，不静默回退。
//! 3. **代隔离**：重建（`AgentHost::rebuild_preparation`）分配新的资源代；旧代句柄的关闭结果与
//!    `set_mode`/`set_config` 只能作用于旧代（已让出的绑定直接拒绝），不能释放或改写新代绑定。
//!
//! 首次提交通过 [`crate::host::AgentHost::transfer_preparation`] 把**同一个端点**转正：不存在第二次
//! `session/new`，`agent_session_id` 与已确认的模式/配置原样沿用，随后即可用
//! [`SessionEndpoint::prompt_with_context`] 派发首条。

use std::sync::Arc;

use acp_core::model::{
    ConfigOption, ConfigOptionId, ConfigOptionKind, ConfigValue, DispatchContext, InteractionId,
    InteractionResolution, ModeId, ModeState, PortError, PromptRequest, SessionId,
    SessionReference, Timestamp, TurnId,
};
use acp_core::ports::{HistoryPage, HistoryQuery, SessionEndpoint, TurnAccepted};
use async_trait::async_trait;

use crate::error::HostError;
use crate::release::ReleaseOutcome;
use crate::session::{AcpSession, Endpoint};

/// 一条准备阶段的配置选择（精确值；不把未确认的值写回 Agent）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationConfigChoice {
    /// 配置项标识。
    pub id: ConfigOptionId,
    /// 要应用的值。
    pub value: ConfigValue,
}

impl PreparationConfigChoice {
    /// 构造。
    #[must_use]
    pub fn new(id: ConfigOptionId, value: ConfigValue) -> Self {
        Self { id, value }
    }
}

/// 准备阶段要应用到临时端点的草稿选择。
///
/// 空选择是合法的：表示用户没有改设置，复核会确认并沿用 Agent 的默认值。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreparationSelection {
    /// 目标模式；`None` = 不改模式。
    pub mode: Option<ModeId>,
    /// 目标配置项（每个条目独立验证）。
    pub config: Vec<PreparationConfigChoice>,
}

impl PreparationSelection {
    /// 构造。
    #[must_use]
    pub fn new(mode: Option<ModeId>, config: Vec<PreparationConfigChoice>) -> Self {
        Self { mode, config }
    }
}

/// 选项复核/应用的结果。
///
/// `modes`/`config` 是 Agent 当前**真实**的选项（应用后重新读取），因此不可用的选择仍能在页面上
/// 与当前可用值一起展示；`unavailable_*` 是本次请求里被拒绝的条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationOptions {
    /// Agent 当前的模式状态（`current_mode` + `available`）。
    pub modes: ModeState,
    /// Agent 当前的配置项。
    pub config: Vec<ConfigOption>,
    /// 已确认应用的模式（`None` = 未请求或未应用）。
    pub applied_mode: Option<ModeId>,
    /// 请求但不再可用的模式（保留可见，阻止派发）。
    pub unavailable_mode: Option<ModeId>,
    /// 已确认应用的配置项标识。
    pub applied_config: Vec<ConfigOptionId>,
    /// 请求但不再可用的配置项标识（保留可见，阻止派发）。
    pub unavailable_config: Vec<ConfigOptionId>,
}

impl PreparationOptions {
    /// 是否存在不可用的选择：为真时不得转正/派发（`design.md` D4）。
    #[must_use]
    pub fn blocks_dispatch(&self) -> bool {
        self.unavailable_mode.is_some() || !self.unavailable_config.is_empty()
    }
}

/// 一条准备端点的句柄。
///
/// 它实现 [`SessionEndpoint`]，因此转正后可以用同一对象派发首条；转正前 prompt 会被显式拒绝
/// （`HostError::PreparationNotReady`），不会为了拿选项而发送空消息。
pub struct TemporaryEndpoint {
    endpoint: Endpoint,
    session: Arc<AcpSession>,
}

impl TemporaryEndpoint {
    pub(crate) fn new(endpoint: Endpoint) -> Self {
        let session = Arc::clone(endpoint.session());
        Self { endpoint, session }
    }

    /// core 会话标识。
    #[must_use]
    pub fn session_id(&self) -> &SessionId {
        self.session.session_id()
    }

    /// 本句柄的资源代（每次重建递增；释放/转正都只认这一代）。
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.session.generation()
    }

    /// 底层会话（组合层核对“目录仍指向本代”用）。
    pub(crate) fn session_ref(&self) -> &Arc<AcpSession> {
        &self.session
    }

    /// 是否已确认释放（幂等释放的判据）。
    #[must_use]
    pub fn is_released(&self) -> bool {
        self.session.is_released()
    }

    /// 是否仍是准备端点（转正后为假）。
    #[must_use]
    pub fn is_temporary(&self) -> bool {
        self.session.is_temporary()
    }

    /// 读取并复核/应用草稿选择（`design.md` D4）。
    ///
    /// 顺序固定为「读当前真实选项 → 逐项验证 → 应用可用项 → 重读确认」。不可用的选择留在报告里并
    /// 把端点标成 blocked：之后 [`crate::host::AgentHost::transfer_preparation`] 与任何 prompt 都会被
    /// 拒绝，**不**用默认值冒充成功。
    pub async fn apply_selection(
        &self,
        selection: &PreparationSelection,
    ) -> Result<PreparationOptions, PortError> {
        if self.session.is_closed() {
            return Err(HostError::SessionClosed.to_port_error());
        }
        if !self.session.is_temporary() {
            return Err(HostError::PreparationNotReady {
                detail: "endpoint is already owned",
            }
            .to_port_error());
        }

        // 1. 读当前选项（Agent 的真实值，不是缓存）。
        let modes = self.endpoint.modes().await?;
        let config = self.endpoint.list_config().await?;

        let mut applied_mode = None;
        let mut unavailable_mode = None;
        let mut applied_config = Vec::new();
        let mut unavailable_config = Vec::new();

        // 2. 验证并应用模式（可用才发送；不可用**不**回退到当前模式冒充成功）。
        if let Some(mode) = &selection.mode {
            if modes
                .available
                .iter()
                .any(|candidate| candidate.mode_id() == mode)
            {
                self.endpoint.set_mode(mode).await?;
                applied_mode = Some(mode.clone());
            } else {
                unavailable_mode = Some(mode.clone());
            }
        }

        // 3. 验证并应用配置（每项独立；Text 在 ACP 线上没有取值形状，视为不可用）。
        for choice in &selection.config {
            let Some(option) = config.iter().find(|option| option.id() == &choice.id) else {
                unavailable_config.push(choice.id.clone());
                continue;
            };
            if !value_applicable(option, &choice.value) {
                unavailable_config.push(choice.id.clone());
                continue;
            }
            self.endpoint
                .set_config(&choice.id, choice.value.clone())
                .await?;
            applied_config.push(choice.id.clone());
        }

        // 4. 以 Agent 确认后的状态为准重读（`set_mode`/`set_config` 已更新当前值）。
        let modes = self.endpoint.modes().await?;
        let config = self.endpoint.list_config().await?;
        let blocked = unavailable_mode.is_some() || !unavailable_config.is_empty();
        self.session.note_preparation(blocked);

        Ok(PreparationOptions {
            modes,
            config,
            applied_mode,
            unavailable_mode,
            applied_config,
            unavailable_config,
        })
    }
}

/// 一个配置值是否可用于给定选项。
///
/// 除了类别匹配，选择值必须出现在 Agent 给出的候选中；`Text` 在 ACP 的 `set_config_option` 上没有
/// 可编码形状，因此**不可用**（不接受后再由 Agent 拒绝）。
fn value_applicable(option: &ConfigOption, value: &ConfigValue) -> bool {
    match value {
        ConfigValue::Boolean(_) => option.kind() == ConfigOptionKind::Boolean,
        ConfigValue::Select(selected) => {
            option.kind() == ConfigOptionKind::Select
                && option
                    .options()
                    .iter()
                    .any(|entry| entry.value().as_str() == selected.as_str())
        }
        ConfigValue::Text(_) => false,
    }
}

impl std::fmt::Debug for TemporaryEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemporaryEndpoint")
            .field("session", &self.session.session_id())
            .field("generation", &self.session.generation())
            .field("temporary", &self.session.is_temporary())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl SessionEndpoint for TemporaryEndpoint {
    fn reference(&self) -> SessionReference {
        self.endpoint.reference()
    }

    fn agent_session_id(&self) -> Option<&acp_core::model::AgentSessionId> {
        self.endpoint.agent_session_id()
    }

    async fn prompt(
        &self,
        request: PromptRequest,
        at: Timestamp,
    ) -> Result<TurnAccepted, PortError> {
        self.endpoint.prompt(request, at).await
    }

    async fn prompt_with_context(
        &self,
        request: PromptRequest,
        at: Timestamp,
        context: Option<DispatchContext>,
    ) -> Result<TurnAccepted, PortError> {
        self.endpoint
            .prompt_with_context(request, at, context)
            .await
    }

    async fn validate_prompt(&self, request: &PromptRequest) -> Result<(), PortError> {
        self.endpoint.validate_prompt(request).await
    }

    async fn cancel(&self, turn: Option<TurnId>) -> Result<(), PortError> {
        self.endpoint.cancel(turn).await
    }

    async fn set_mode(&self, mode: &ModeId) -> Result<(), PortError> {
        self.endpoint.set_mode(mode).await
    }

    async fn list_config(&self) -> Result<Vec<ConfigOption>, PortError> {
        self.endpoint.list_config().await
    }

    async fn modes(&self) -> Result<ModeState, PortError> {
        self.endpoint.modes().await
    }

    async fn set_config(&self, id: &ConfigOptionId, value: ConfigValue) -> Result<(), PortError> {
        self.endpoint.set_config(id, value).await
    }

    async fn resolve_interaction(
        &self,
        interaction: &InteractionId,
        resolution: InteractionResolution,
    ) -> Result<(), PortError> {
        self.endpoint
            .resolve_interaction(interaction, resolution)
            .await
    }

    async fn read_history(&self, query: HistoryQuery) -> Result<HistoryPage, PortError> {
        self.endpoint.read_history(query).await
    }

    async fn close(&self) -> Result<(), PortError> {
        self.endpoint.close().await
    }
}

impl TemporaryEndpoint {
    /// 真实释放并给出 core 口径的三态结论（端口 `close` 只返回错误；回收判定需要三态）。
    #[must_use]
    pub async fn release_outcome(&self) -> ReleaseOutcome {
        self.endpoint.release_outcome().await
    }
}
