//! 会话、turn、命令与交互（`docs/CORE_PORTS_AND_STORAGE.md` §3.3）。
//!
//! 状态机规则（本合同在 §3.3/§6 的允许范围内给出的唯一转换表；`Session`/`Turn` 的状态字段私有，只能经
//! `transition(..)` 前进，适配器无法直接改状态）：
//!
//! ```text
//! SessionState: idle → queued|running|closed|failed
//!               queued → running|idle|closed|failed
//!               running → waiting_input|waiting_permission|idle|failed|closed
//!               waiting_input|waiting_permission → running|failed|closed
//!               failed|closed → (终态，无出边)
//! TurnState:    queued → running|cancelled|failed
//!               running → waiting_input|waiting_permission|completed|failed|cancelled
//!               waiting_input|waiting_permission → running|failed|cancelled
//!               completed|failed|cancelled → (终态，无出边)
//! CommandStatus: accepted → completed|failed|rejected|uncertain；(其余为终态)
//! ```

use std::fmt;
use std::str::FromStr;

use super::ViewJson;
use super::elicitation::{ElicitationAction, ElicitationValues};
use super::error::InvalidValue;
use super::identity::Actor;
use super::ids::{
    AgentRef, ConfigOptionId, EventId, ExportId, InteractionId, ModeId, ModeRef, NodeId,
    OriginEpoch, OwnedSessionRef, RemoteSessionRef, RequestId, SessionId, TurnId, is_error_code,
    is_scope_name, require_bounded,
};
use super::scalars::{Digest, GlobalCursor, Timestamp, Version};

/// 会话来源：owned 与 imported 在类型上可区分（§3.3）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceOrigin {
    Local,
    Remote {
        owner_node_id: NodeId,
        export_id: ExportId,
        origin_epoch: OriginEpoch,
        online: bool,
    },
}

impl ResourceOrigin {
    /// 是否为本节点 owned。
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    /// 远端在线状态；本地会话返回 `None`（`online` 只对 imported 有意义）。
    pub fn online(&self) -> Option<bool> {
        match self {
            Self::Local => None,
            Self::Remote { online, .. } => Some(*online),
        }
    }

    /// 该 origin 下的 imported 会话引用。
    pub fn remote_session(&self, session_id: SessionId) -> Option<RemoteSessionRef> {
        match self {
            Self::Local => None,
            Self::Remote {
                owner_node_id,
                export_id,
                ..
            } => Some(RemoteSessionRef::new(
                owner_node_id.clone(),
                export_id.clone(),
                session_id,
            )),
        }
    }
}

token_enum!(
    /// 会话状态（`common.schema.json#/$defs/sessionSummary`）。
    SessionState {
        Idle => "idle",
        Queued => "queued",
        Running => "running",
        WaitingInput => "waiting_input",
        WaitingPermission => "waiting_permission",
        Failed => "failed",
        Closed => "closed",
    }
);

impl SessionState {
    /// 终态：`failed` 与 `closed` 之后不再有合法转换。
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Failed | Self::Closed)
    }

    /// 是否允许一次状态转换。
    pub const fn can_transition_to(self, next: Self) -> bool {
        match self {
            Self::Idle => matches!(
                next,
                Self::Queued | Self::Running | Self::Closed | Self::Failed
            ),
            Self::Queued => matches!(
                next,
                Self::Running | Self::Idle | Self::Closed | Self::Failed
            ),
            Self::Running => matches!(
                next,
                Self::WaitingInput
                    | Self::WaitingPermission
                    | Self::Idle
                    | Self::Failed
                    | Self::Closed
            ),
            Self::WaitingInput | Self::WaitingPermission => {
                matches!(next, Self::Running | Self::Failed | Self::Closed)
            }
            Self::Failed | Self::Closed => false,
        }
    }
}

token_enum!(
    /// turn 状态（`SYNC_PROTOCOL.md` §11.6）。
    TurnState {
        Queued => "queued",
        Running => "running",
        WaitingInput => "waiting_input",
        WaitingPermission => "waiting_permission",
        Completed => "completed",
        Failed => "failed",
        Cancelled => "cancelled",
    }
);

impl TurnState {
    /// 终态：终态之后不再有合法转换。
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// 是否允许一次状态转换。
    pub const fn can_transition_to(self, next: Self) -> bool {
        match self {
            Self::Queued => matches!(next, Self::Running | Self::Cancelled | Self::Failed),
            Self::Running => matches!(
                next,
                Self::WaitingInput
                    | Self::WaitingPermission
                    | Self::Completed
                    | Self::Failed
                    | Self::Cancelled
            ),
            Self::WaitingInput | Self::WaitingPermission => {
                matches!(next, Self::Running | Self::Failed | Self::Cancelled)
            }
            Self::Completed | Self::Failed | Self::Cancelled => false,
        }
    }
}

/// 会话聚合。字段私有：状态只能经 [`Session::transition`] 前进。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    id: SessionId,
    reference: OwnedSessionRef,
    title: Option<String>,
    agent: AgentRef,
    state: SessionState,
    origin: ResourceOrigin,
    current_mode: Option<ModeRef>,
    version: Version,
    created_at: Timestamp,
    updated_at: Timestamp,
    closed_at: Option<Timestamp>,
}

impl Session {
    /// 构造。`title` 上限 512 字符；`closed_at` 必须与 `state = closed` 一致；`reference` 必须指向本 id。
    #[allow(clippy::too_many_arguments)] // §3.3 冻结的 11 字段记录，不为此引入 builder
    pub fn try_new(
        id: SessionId,
        reference: OwnedSessionRef,
        title: Option<String>,
        agent: AgentRef,
        state: SessionState,
        origin: ResourceOrigin,
        current_mode: Option<ModeRef>,
        version: Version,
        created_at: Timestamp,
        updated_at: Timestamp,
        closed_at: Option<Timestamp>,
    ) -> Result<Self, InvalidValue> {
        if reference.session_id != id {
            return Err(InvalidValue::Field);
        }
        if let Some(title) = &title {
            require_bounded(title, 0, 512)?;
        }
        if closed_at.is_some() != (state == SessionState::Closed) {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            id,
            reference,
            title,
            agent,
            state,
            origin,
            current_mode,
            version,
            created_at,
            updated_at,
            closed_at,
        })
    }

    /// 会话标识。
    pub fn id(&self) -> &SessionId {
        &self.id
    }

    /// owned 引用。
    pub fn reference(&self) -> &OwnedSessionRef {
        &self.reference
    }

    /// 标题。
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Agent 引用。
    pub fn agent(&self) -> &AgentRef {
        &self.agent
    }

    /// 当前状态。
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// 来源。
    pub fn origin(&self) -> &ResourceOrigin {
        &self.origin
    }

    /// 当前模式。
    pub fn current_mode(&self) -> Option<&ModeRef> {
        self.current_mode.as_ref()
    }

    /// 乐观并发版本。
    pub fn version(&self) -> Version {
        self.version
    }

    /// 创建时间。
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }

    /// 最近更新时间。
    pub fn updated_at(&self) -> &Timestamp {
        &self.updated_at
    }

    /// 关闭时间。
    pub fn closed_at(&self) -> Option<&Timestamp> {
        self.closed_at.as_ref()
    }

    /// 状态转换；非法转换（含离开终态）返回 `InvalidValue::StateTransition`。
    ///
    /// 进入 `closed` 时写入 `closed_at`，任何转换都刷新 `updated_at`。version 由存储层在提交事务内递增，
    /// 不在这里改（§5.2 的 `CommitOutcome.version` 是唯一来源）。
    pub fn transition(&mut self, next: SessionState, at: &Timestamp) -> Result<(), InvalidValue> {
        if !self.state.can_transition_to(next) {
            return Err(InvalidValue::StateTransition);
        }
        self.state = next;
        self.updated_at = at.clone();
        if next == SessionState::Closed {
            self.closed_at = Some(at.clone());
        }
        Ok(())
    }

    /// 可投影摘要（`SYNC_PROTOCOL.md` §9.6 的 `sessionSummary`）。
    pub fn summary(&self) -> SessionSummary {
        SessionSummary {
            session_id: self.id.clone(),
            title: self.title.clone(),
            agent: self.agent.clone(),
            state: self.state,
            origin: self.origin.clone(),
            current_mode: self.current_mode.clone(),
            version: self.version,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

/// `Session` 的可投影子集：owned 与 imported 用同一形状，且**不含任何正文**
/// （`SYNC_PROTOCOL.md` §9.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    session_id: SessionId,
    title: Option<String>,
    agent: AgentRef,
    state: SessionState,
    origin: ResourceOrigin,
    current_mode: Option<ModeRef>,
    version: Version,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl SessionSummary {
    /// 构造。`title` 上限 512 字符。
    #[allow(clippy::too_many_arguments)] // 与 §9.6 的投影字段一一对应
    pub fn try_new(
        session_id: SessionId,
        title: Option<String>,
        agent: AgentRef,
        state: SessionState,
        origin: ResourceOrigin,
        current_mode: Option<ModeRef>,
        version: Version,
        created_at: Timestamp,
        updated_at: Timestamp,
    ) -> Result<Self, InvalidValue> {
        if let Some(title) = &title {
            require_bounded(title, 0, 512)?;
        }
        Ok(Self {
            session_id,
            title,
            agent,
            state,
            origin,
            current_mode,
            version,
            created_at,
            updated_at,
        })
    }

    /// 会话标识。
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// 标题。
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Agent 引用。
    pub fn agent(&self) -> &AgentRef {
        &self.agent
    }

    /// 会话状态。
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// 来源。
    pub fn origin(&self) -> &ResourceOrigin {
        &self.origin
    }

    /// 当前模式。
    pub fn current_mode(&self) -> Option<&ModeRef> {
        self.current_mode.as_ref()
    }

    /// 乐观并发版本。
    pub fn version(&self) -> Version {
        self.version
    }

    /// 创建时间。
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }

    /// 最近更新时间。
    pub fn updated_at(&self) -> &Timestamp {
        &self.updated_at
    }
}

/// `Session` + 会话 epoch + 全局 head（§3.3，供 `commit`/`load` 返回）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub session: Session,
    pub origin_epoch: OriginEpoch,
    pub head: GlobalCursor,
}

/// turn 聚合。字段私有：状态只能经 [`Turn::transition`] 前进。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    id: TurnId,
    session: SessionId,
    state: TurnState,
    queue_index: u32,
    causation: Option<RequestId>,
    started_at: Option<Timestamp>,
    ended_at: Option<Timestamp>,
}

impl Turn {
    /// 构造。`ended_at` 必须与终态一致。
    pub fn try_new(
        id: TurnId,
        session: SessionId,
        state: TurnState,
        queue_index: u32,
        causation: Option<RequestId>,
        started_at: Option<Timestamp>,
        ended_at: Option<Timestamp>,
    ) -> Result<Self, InvalidValue> {
        if ended_at.is_some() != state.is_terminal() {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            id,
            session,
            state,
            queue_index,
            causation,
            started_at,
            ended_at,
        })
    }

    /// turn 标识。
    pub fn id(&self) -> &TurnId {
        &self.id
    }

    /// 所属会话。
    pub fn session(&self) -> &SessionId {
        &self.session
    }

    /// 当前状态。
    pub fn state(&self) -> TurnState {
        self.state
    }

    /// 持久化接受顺序（`UNIQUE (session_id, queue_index)`）。
    pub fn queue_index(&self) -> u32 {
        self.queue_index
    }

    /// 触发它的命令。
    pub fn causation(&self) -> Option<&RequestId> {
        self.causation.as_ref()
    }

    /// 首次进入 `running` 的时间。
    pub fn started_at(&self) -> Option<&Timestamp> {
        self.started_at.as_ref()
    }

    /// 终态时间。
    pub fn ended_at(&self) -> Option<&Timestamp> {
        self.ended_at.as_ref()
    }

    /// 状态转换；非法转换（含离开终态）返回 `InvalidValue::StateTransition`。
    ///
    /// 首次进入 `running` 时写入 `started_at`，进入终态时写入 `ended_at`。
    pub fn transition(&mut self, next: TurnState, at: &Timestamp) -> Result<(), InvalidValue> {
        if !self.state.can_transition_to(next) {
            return Err(InvalidValue::StateTransition);
        }
        if next == TurnState::Running && self.started_at.is_none() {
            self.started_at = Some(at.clone());
        }
        self.state = next;
        if next.is_terminal() {
            self.ended_at = Some(at.clone());
        }
        Ok(())
    }
}

token_enum!(
    /// 配置项类别（`common.schema.json#/$defs/configOptionView`）。
    ConfigOptionKind {
        Select => "select",
        Boolean => "boolean",
    }
);

newtype!(
    /// 文本型配置值（≤4096 字符）。
    ConfigText,
    check_config_text
);

newtype!(
    /// 选择型配置值（1..=256 字符）。
    SelectValue,
    check_select_value
);

fn check_config_text(text: &str) -> Result<(), InvalidValue> {
    require_bounded(text, 0, 4096)
}

fn check_select_value(text: &str) -> Result<(), InvalidValue> {
    require_bounded(text, 1, 256)
}

/// 配置值（§3.3）。`Text` 与 `Select` 都是字符串，`Boolean` 是布尔。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigValue {
    Text(ConfigText),
    Boolean(bool),
    Select(SelectValue),
}

impl ConfigValue {
    /// 文本值。
    pub fn text(value: &str) -> Result<Self, InvalidValue> {
        Ok(Self::Text(ConfigText::new(value)?))
    }

    /// 布尔值。
    pub fn boolean(value: bool) -> Self {
        Self::Boolean(value)
    }

    /// 选择值。
    pub fn select(value: &str) -> Result<Self, InvalidValue> {
        Ok(Self::Select(SelectValue::new(value)?))
    }

    /// 是否匹配给定配置项类别（`SYNC_PROTOCOL.md` §11.5 要求类型一致）。
    pub fn matches_kind(&self, kind: ConfigOptionKind) -> bool {
        matches!(
            (self, kind),
            (Self::Boolean(_), ConfigOptionKind::Boolean)
                | (Self::Text(_) | Self::Select(_), ConfigOptionKind::Select)
        )
    }
}

/// 配置项的候选取值（`configOptionView.options[]`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigOptionEntry {
    value: SelectValue,
    name: String,
    description: Option<String>,
}

impl ConfigOptionEntry {
    /// 构造。`name` 1..=256，`description` ≤1024。
    pub fn try_new(
        value: SelectValue,
        name: &str,
        description: Option<String>,
    ) -> Result<Self, InvalidValue> {
        require_bounded(name, 1, 256)?;
        if let Some(description) = &description {
            require_bounded(description, 0, 1024)?;
        }
        Ok(Self {
            value,
            name: name.to_owned(),
            description,
        })
    }

    /// 候选值。
    pub fn value(&self) -> &SelectValue {
        &self.value
    }

    /// 展示名。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 说明。
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}

/// 会话配置项（`configOptionView`）。`current` 必须匹配 `kind`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigOption {
    id: ConfigOptionId,
    name: String,
    description: Option<String>,
    category: Option<String>,
    kind: ConfigOptionKind,
    current: ConfigValue,
    options: Vec<ConfigOptionEntry>,
}

impl ConfigOption {
    /// 构造。`name` 1..=256，`description` ≤1024，`category` ≤128；`current` 必须匹配 `kind`。
    pub fn try_new(
        id: ConfigOptionId,
        name: &str,
        description: Option<String>,
        category: Option<String>,
        kind: ConfigOptionKind,
        current: ConfigValue,
        options: Vec<ConfigOptionEntry>,
    ) -> Result<Self, InvalidValue> {
        require_bounded(name, 1, 256)?;
        if let Some(description) = &description {
            require_bounded(description, 0, 1024)?;
        }
        if let Some(category) = &category {
            require_bounded(category, 0, 128)?;
        }
        if !current.matches_kind(kind) {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            id,
            name: name.to_owned(),
            description,
            category,
            kind,
            current,
            options,
        })
    }

    /// 配置项标识。
    pub fn id(&self) -> &ConfigOptionId {
        &self.id
    }

    /// 展示名。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 说明。
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// 分组。
    pub fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }

    /// 类别。
    pub fn kind(&self) -> ConfigOptionKind {
        self.kind
    }

    /// 当前值。
    pub fn current(&self) -> &ConfigValue {
        &self.current
    }

    /// 候选取值。
    pub fn options(&self) -> &[ConfigOptionEntry] {
        &self.options
    }
}

token_enum!(
    /// 命令类别（取自 `compatibility/commands/v1/commands.json` 的分类）。
    CommandKind {
        Query => "query",
        Mutation => "mutation",
    }
);

token_enum!(
    /// `session.read` 的 `include` 取值（`SYNC_PROTOCOL.md` §11.5）。
    ReadInclude {
        Messages => "messages",
        Turns => "turns",
        PendingInteractions => "pending_interactions",
        ConfigOptions => "config_options",
        Capabilities => "capabilities",
    }
);

/// `session.prompt` 的内容块。
///
/// v1 baseline 只有 `{ "type": "text", "text": "..." }`；块的具体形状属协议 crate，core 只做不透明载体
/// （必须是 object，字节原样保留），因此这里包一层 [`ViewJson`] 而不是解释字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptContentBlock(ViewJson);

impl PromptContentBlock {
    /// 从已验证的 object 构造。
    pub fn new(json: ViewJson) -> Self {
        Self(json)
    }

    /// 从 JSON 文本构造。
    pub fn from_json_text(text: &str) -> Result<Self, InvalidValue> {
        Ok(Self(ViewJson::new(text)?))
    }

    /// 底层视图。
    pub fn as_json(&self) -> &ViewJson {
        &self.0
    }

    /// JSON 文本。
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// `session.prompt` 的 payload。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptRequest {
    pub content: Vec<PromptContentBlock>,
}

impl PromptRequest {
    /// 构造。
    pub fn new(content: Vec<PromptContentBlock>) -> Self {
        Self { content }
    }

    /// `content` 必须 1..=64 项（`SYNC_PROTOCOL.md` §11.5）。
    pub fn validate(&self) -> Result<(), InvalidValue> {
        check_prompt_len(self.content.len())
    }
}

/// `session.prompt` 的内容块数量约束（1..=64）。
fn check_prompt_len(len: usize) -> Result<(), InvalidValue> {
    if len == 0 {
        return Err(InvalidValue::Empty);
    }
    if len > 64 {
        return Err(InvalidValue::TooLong { max: 64 });
    }
    Ok(())
}

/// 命令 payload：按命令名一对一（§3.3）。`session.create` 不在其中——它经
/// [`super::CreateSessionRequest`] 与 Node Link 的 `session.create` 路径进入（§12.7）。
#[derive(Debug, Clone, PartialEq)]
pub enum CommandPayload {
    SessionList {},
    SessionRead {
        include: Vec<ReadInclude>,
    },
    CommandStatus {
        target_request: RequestId,
    },
    ModeList {},
    ConfigList {},
    Prompt {
        content: Vec<PromptContentBlock>,
    },
    Cancel {
        turn: Option<TurnId>,
    },
    ModeSet {
        mode: ModeId,
    },
    ConfigSet {
        id: ConfigOptionId,
        value: ConfigValue,
    },
    PermissionResolve {
        interaction: InteractionId,
        option_id: String,
    },
    ElicitationRespond {
        interaction: InteractionId,
        action: ElicitationAction,
        values: ElicitationValues,
    },
}

impl CommandPayload {
    /// 该 payload 对应的命令类别。
    pub fn family(&self) -> CommandKind {
        match self {
            Self::SessionList {}
            | Self::SessionRead { .. }
            | Self::CommandStatus { .. }
            | Self::ModeList {}
            | Self::ConfigList {} => CommandKind::Query,
            Self::Prompt { .. }
            | Self::Cancel { .. }
            | Self::ModeSet { .. }
            | Self::ConfigSet { .. }
            | Self::PermissionResolve { .. }
            | Self::ElicitationRespond { .. } => CommandKind::Mutation,
        }
    }

    /// 该 payload 是否要求顶层 `sessionId`（`SYNC_PROTOCOL.md` §11.5 的 `sessionId` 列）。
    pub fn requires_session(&self) -> bool {
        !matches!(self, Self::SessionList {} | Self::CommandStatus { .. })
    }

    /// 该 payload 是否要求 `expectedVersion`。
    pub fn requires_expected_version(&self) -> bool {
        matches!(self, Self::ModeSet { .. } | Self::ConfigSet { .. })
    }

    /// payload 自身的结构校验（`include` 去重与非空、prompt 项数、optionId 长度、elicitation 一致性）。
    pub fn validate(&self) -> Result<(), InvalidValue> {
        match self {
            Self::SessionRead { include } => {
                if include.is_empty() {
                    return Err(InvalidValue::Empty);
                }
                for (index, item) in include.iter().enumerate() {
                    if include[..index].contains(item) {
                        return Err(InvalidValue::Field);
                    }
                }
                Ok(())
            }
            Self::Prompt { content } => check_prompt_len(content.len()),
            Self::PermissionResolve { option_id, .. } => require_bounded(option_id, 1, 64),
            Self::ElicitationRespond { action, values, .. } => check_elicitation(*action, values),
            Self::SessionList {}
            | Self::CommandStatus { .. }
            | Self::ModeList {}
            | Self::ConfigList {}
            | Self::Cancel { .. }
            | Self::ModeSet { .. }
            | Self::ConfigSet { .. } => Ok(()),
        }
    }
}

/// 客户端命令（§3.3）。字段公有；跨字段规则由 [`ClientCommand::validate`] 表达。
///
/// 比 §3.3 的字段表多一个 [`ClientCommand::request_fingerprint`]：§6 第 6 条与 §7.3 要求幂等比对
/// 「解码后 payload 按 ACPR-CJ1 取的摘要」，而 core 既无 crypto 依赖也无规范 JSON 实现（§2），只能由
/// 适配器在解码时算好并原样传入。
#[derive(Debug, Clone, PartialEq)]
pub struct ClientCommand {
    pub actor: Actor,
    pub request: RequestId,
    pub command: String,
    pub kind: CommandKind,
    pub session: Option<SessionId>,
    pub expected_version: Option<Version>,
    /// 适配器对**解码后** payload 按 ACPR-CJ1 取的 SHA-256；幂等冲突判定（§6 第 6 条）的唯一依据。
    pub request_fingerprint: Digest,
    pub payload: CommandPayload,
}

impl ClientCommand {
    /// 结构校验：命令名形状、`kind` 与 payload 家族一致、`sessionId` 必需性、
    /// `expectedVersion` 必需性（`SYNC_PROTOCOL.md` §11.5）。
    pub fn validate(&self) -> Result<(), InvalidValue> {
        if !is_scope_name(&self.command) {
            return Err(InvalidValue::Field);
        }
        self.payload.validate()?;
        if self.payload.family() != self.kind {
            return Err(InvalidValue::Field);
        }
        if self.payload.requires_session() != self.session.is_some() {
            return Err(InvalidValue::Field);
        }
        if self.kind == CommandKind::Query && self.expected_version.is_some() {
            return Err(InvalidValue::Field);
        }
        if self.payload.requires_expected_version() != self.expected_version.is_some() {
            return Err(InvalidValue::Field);
        }
        Ok(())
    }
}

/// 同步接受结果（§3.3）：`accepted` 不是业务完成，终态只经事件与 `command.status` 表达。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandReceipt {
    Accepted {
        request: RequestId,
        turn: Option<TurnId>,
    },
    Rejected {
        error: PublicError,
    },
}

token_enum!(
    /// 命令状态（`SYNC_PROTOCOL.md` §11.2，token 即 `owned_command.status` 的 CHECK 取值）。
    CommandStatus {
        Accepted => "accepted",
        Completed => "completed",
        Failed => "failed",
        Rejected => "rejected",
        Uncertain => "uncertain",
    }
);

impl CommandStatus {
    /// 终态：`completed`/`failed`/`rejected`/`uncertain` 之后不再变化
    /// （`uncertain` 也是终态，`SYNC_PROTOCOL.md` §11.2）。
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Accepted)
    }

    /// 是否允许一次状态转换。
    pub const fn can_transition_to(self, next: Self) -> bool {
        match self {
            Self::Accepted => !matches!(next, Self::Accepted),
            Self::Completed | Self::Failed | Self::Rejected | Self::Uncertain => false,
        }
    }
}

/// 命令的最终结果：一个 JSON object（§11.2 的 `result`；`uncertain`/`rejected` 时为 `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult(ViewJson);

impl CommandResult {
    /// 从已验证的 object 构造。
    pub fn new(json: ViewJson) -> Self {
        Self(json)
    }

    /// 从 JSON 文本构造。
    pub fn from_json_text(text: &str) -> Result<Self, InvalidValue> {
        Ok(Self(ViewJson::new(text)?))
    }

    /// 底层视图。
    pub fn as_json(&self) -> &ViewJson {
        &self.0
    }

    /// JSON 文本。
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<ViewJson> for CommandResult {
    fn from(value: ViewJson) -> Self {
        Self(value)
    }
}

/// 结构化公开错误（`common.schema.json#/$defs/publicError`）。**不含** SQL、prompt、密钥或 ACP raw。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicError {
    code: String,
    message: String,
    retryable: bool,
    details: ViewJson,
}

impl PublicError {
    /// 构造：`code` 必须匹配 `^[a-z][a-z0-9._-]{0,127}$`，`message` ≤1024 字符。
    pub fn try_new(
        code: &str,
        message: &str,
        retryable: bool,
        details: ViewJson,
    ) -> Result<Self, InvalidValue> {
        if !is_error_code(code) {
            return Err(InvalidValue::ErrorCode);
        }
        require_bounded(message, 0, 1024)?;
        Ok(Self {
            code: code.to_owned(),
            message: message.to_owned(),
            retryable,
            details,
        })
    }

    /// 无 details 的构造（`details = {}`）。
    pub fn coded(code: &str, message: &str, retryable: bool) -> Result<Self, InvalidValue> {
        Self::try_new(code, message, retryable, ViewJson::empty_object())
    }

    /// 错误码。
    pub fn code(&self) -> &str {
        &self.code
    }

    /// 面向用户的消息。
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 是否可重试。
    pub fn retryable(&self) -> bool {
        self.retryable
    }

    /// 已登记的 details（§12.2 的 details 登记表）。
    pub fn details(&self) -> &ViewJson {
        &self.details
    }
}

/// `CommandRecord` 的终态子集（§3.3）：`{ status, terminal_at, terminal_event, result, error }`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandTerminalRecord {
    status: CommandStatus,
    terminal_at: Option<Timestamp>,
    terminal_event: Option<EventId>,
    result: Option<CommandResult>,
    error: Option<PublicError>,
}

impl CommandTerminalRecord {
    /// 构造。`status` 必须是终态；`rejected` 不带 `terminal_at`/`result` 且必须带 `error`；
    /// `completed` 必须不带 `error`；`failed` 必须带 `error`。
    pub fn try_new(
        status: CommandStatus,
        terminal_at: Option<Timestamp>,
        terminal_event: Option<EventId>,
        result: Option<CommandResult>,
        error: Option<PublicError>,
    ) -> Result<Self, InvalidValue> {
        if !status.is_terminal() {
            return Err(InvalidValue::Field);
        }
        check_terminal_shape(
            status,
            terminal_at.as_ref(),
            result.as_ref(),
            error.as_ref(),
        )?;
        Ok(Self {
            status,
            terminal_at,
            terminal_event,
            result,
            error,
        })
    }

    /// 终态状态。
    pub fn status(&self) -> CommandStatus {
        self.status
    }

    /// 终态时间。
    pub fn terminal_at(&self) -> Option<&Timestamp> {
        self.terminal_at.as_ref()
    }

    /// 终态事件（`command.completed`/`command.failed`/`command.uncertain`）。
    pub fn terminal_event(&self) -> Option<&EventId> {
        self.terminal_event.as_ref()
    }

    /// 结果。
    pub fn result(&self) -> Option<&CommandResult> {
        self.result.as_ref()
    }

    /// 错误。
    pub fn error(&self) -> Option<&PublicError> {
        self.error.as_ref()
    }
}

/// 幂等记录与命令状态（`SYNC_PROTOCOL.md` §11.2/§11.4）。字段私有；跨字段规则镜像
/// `owned_command` 的 CHECK 约束（§7.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRecord {
    session: Option<SessionId>,
    request: RequestId,
    command: String,
    kind: CommandKind,
    actor: Actor,
    accepted_at: Option<Timestamp>,
    status: CommandStatus,
    terminal_at: Option<Timestamp>,
    terminal_event: Option<EventId>,
    result: Option<CommandResult>,
    error: Option<PublicError>,
    expected_version: Option<Version>,
    request_fingerprint: super::scalars::Digest,
}

impl CommandRecord {
    /// 构造。除 §3.3 的字段形状外，额外强制 §11.2/§7.3 的相容性：
    ///
    /// - `rejected` 必须无 `accepted_at`/`result` 且有 `error`；其余状态必须有 `accepted_at`；
    /// - `accepted` 不得携带 `terminal_at`/`terminal_event`/`result`/`error`；
    /// - `completed` 不得有 `error`，查询命令不得有 `terminal_event`，mutation 的 `completed` 必须有；
    /// - `failed`/`uncertain` 必须有 `error` 与 `terminal_event`。
    #[allow(clippy::too_many_arguments)] // §3.3 冻结的 13 字段记录
    pub fn try_new(
        session: Option<SessionId>,
        request: RequestId,
        command: &str,
        kind: CommandKind,
        actor: Actor,
        accepted_at: Option<Timestamp>,
        status: CommandStatus,
        terminal_at: Option<Timestamp>,
        terminal_event: Option<EventId>,
        result: Option<CommandResult>,
        error: Option<PublicError>,
        expected_version: Option<Version>,
        request_fingerprint: super::scalars::Digest,
    ) -> Result<Self, InvalidValue> {
        if !is_scope_name(command) {
            return Err(InvalidValue::Field);
        }
        match status {
            CommandStatus::Accepted => {
                if accepted_at.is_none()
                    || terminal_at.is_some()
                    || terminal_event.is_some()
                    || result.is_some()
                    || error.is_some()
                {
                    return Err(InvalidValue::Field);
                }
            }
            CommandStatus::Rejected => {
                if accepted_at.is_some() {
                    return Err(InvalidValue::Field);
                }
                check_terminal_shape(
                    status,
                    terminal_at.as_ref(),
                    result.as_ref(),
                    error.as_ref(),
                )?;
                if terminal_event.is_some() {
                    return Err(InvalidValue::Field);
                }
            }
            CommandStatus::Completed | CommandStatus::Failed | CommandStatus::Uncertain => {
                if accepted_at.is_none() {
                    return Err(InvalidValue::Field);
                }
                check_terminal_shape(
                    status,
                    terminal_at.as_ref(),
                    result.as_ref(),
                    error.as_ref(),
                )?;
                match (kind, status) {
                    (CommandKind::Query, _) | (_, CommandStatus::Rejected) => {
                        if terminal_event.is_some() {
                            return Err(InvalidValue::Field);
                        }
                    }
                    (CommandKind::Mutation, _) => {
                        if terminal_event.is_none() {
                            return Err(InvalidValue::Field);
                        }
                    }
                }
            }
        }
        Ok(Self {
            session,
            request,
            command: command.to_owned(),
            kind,
            actor,
            accepted_at,
            status,
            terminal_at,
            terminal_event,
            result,
            error,
            expected_version,
            request_fingerprint,
        })
    }

    /// 目标会话；非会话命令为 `None`。
    pub fn session(&self) -> Option<&SessionId> {
        self.session.as_ref()
    }

    /// 请求标识（幂等键的一半）。
    pub fn request(&self) -> &RequestId {
        &self.request
    }

    /// 命令名。
    pub fn command(&self) -> &str {
        &self.command
    }

    /// 命令类别。
    pub fn kind(&self) -> CommandKind {
        self.kind
    }

    /// 提交者。
    pub fn actor(&self) -> &Actor {
        &self.actor
    }

    /// 首次持久化接受时间；`rejected` 为 `None`。
    pub fn accepted_at(&self) -> Option<&Timestamp> {
        self.accepted_at.as_ref()
    }

    /// 当前状态。
    pub fn status(&self) -> CommandStatus {
        self.status
    }

    /// 终态时间。
    pub fn terminal_at(&self) -> Option<&Timestamp> {
        self.terminal_at.as_ref()
    }

    /// 唯一的 command terminal event。
    pub fn terminal_event(&self) -> Option<&EventId> {
        self.terminal_event.as_ref()
    }

    /// 最终结果。
    pub fn result(&self) -> Option<&CommandResult> {
        self.result.as_ref()
    }

    /// 失败原因。
    pub fn error(&self) -> Option<&PublicError> {
        self.error.as_ref()
    }

    /// 乐观并发期望值（原样保存，用于跨重启的幂等冲突判定）。
    pub fn expected_version(&self) -> Option<Version> {
        self.expected_version
    }

    /// 解码后 payload 的 ACPR-CJ1 摘要（幂等冲突判定的唯一依据之一）。
    pub fn request_fingerprint(&self) -> &super::scalars::Digest {
        &self.request_fingerprint
    }

    /// 取终态子集。
    pub fn terminal(&self) -> Option<CommandTerminalRecord> {
        if !self.status.is_terminal() {
            return None;
        }
        Some(CommandTerminalRecord {
            status: self.status,
            terminal_at: self.terminal_at.clone(),
            terminal_event: self.terminal_event.clone(),
            result: self.result.clone(),
            error: self.error.clone(),
        })
    }
}

/// §11.2 的终态相容性：`rejected` 无时间与结果、必须有错误；`completed` 无错误；`failed` 必须有错误。
fn check_terminal_shape(
    status: CommandStatus,
    terminal_at: Option<&Timestamp>,
    result: Option<&CommandResult>,
    error: Option<&PublicError>,
) -> Result<(), InvalidValue> {
    match status {
        CommandStatus::Rejected => {
            if terminal_at.is_some() || result.is_some() || error.is_none() {
                return Err(InvalidValue::Field);
            }
        }
        CommandStatus::Completed => {
            if terminal_at.is_none() || error.is_some() {
                return Err(InvalidValue::Field);
            }
        }
        CommandStatus::Failed => {
            if terminal_at.is_none() || error.is_none() {
                return Err(InvalidValue::Field);
            }
        }
        CommandStatus::Uncertain => {
            if terminal_at.is_none() {
                return Err(InvalidValue::Field);
            }
        }
        CommandStatus::Accepted => return Err(InvalidValue::Field),
    }
    Ok(())
}

token_enum!(
    /// 交互类别（`SYNC_PROTOCOL.md` §10.3）。
    InteractionKind {
        Permission => "permission",
        Elicitation => "elicitation",
    }
);

/// 交互候选项（`interactionOption`）。`option_id`/`label` 是 Agent 提供的原文，原样保留。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionOption {
    option_id: String,
    label: String,
    kind: String,
}

impl InteractionOption {
    /// 构造。`option_id` 1..=128，`label` 1..=512，`kind` 1..=64。
    pub fn try_new(option_id: &str, label: &str, kind: &str) -> Result<Self, InvalidValue> {
        require_bounded(option_id, 1, 128)?;
        require_bounded(label, 1, 512)?;
        require_bounded(kind, 1, 64)?;
        Ok(Self {
            option_id: option_id.to_owned(),
            label: label.to_owned(),
            kind: kind.to_owned(),
        })
    }

    /// Agent 给的原始 optionId。
    pub fn option_id(&self) -> &str {
        &self.option_id
    }

    /// 展示文本。
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Agent 给的类别标记（仅用于展示）。
    pub fn kind(&self) -> &str {
        &self.kind
    }
}

token_enum!(
    /// 权限决定的分类（仅用于 UI/审计；`option_id` 才是权威）。
    PermissionDecisionKind {
        AllowOnce => "allow_once",
        AllowAlways => "allow_always",
        RejectOnce => "reject_once",
        RejectAlways => "reject_always",
    }
);

/// 权限决定（§3.3）。`option_id` 必须是 Agent 在未解决请求里给出的原始值并原样回传。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionDecision {
    option_id: String,
    kind: PermissionDecisionKind,
}

impl PermissionDecision {
    /// 构造。`option_id` 1..=64。
    pub fn try_new(option_id: &str, kind: PermissionDecisionKind) -> Result<Self, InvalidValue> {
        require_bounded(option_id, 1, 64)?;
        Ok(Self {
            option_id: option_id.to_owned(),
            kind,
        })
    }

    /// 原始 optionId。
    pub fn option_id(&self) -> &str {
        &self.option_id
    }

    /// 分类。
    pub fn kind(&self) -> PermissionDecisionKind {
        self.kind
    }
}

/// 权限与 elicitation 共用的唯一解析入口（§3.3）。
#[derive(Debug, Clone, PartialEq)]
pub enum InteractionResolution {
    Permission(PermissionDecision),
    Elicitation {
        action: ElicitationAction,
        values: ElicitationValues,
    },
}

impl InteractionResolution {
    /// 权限解析。
    pub fn permission(decision: PermissionDecision) -> Self {
        Self::Permission(decision)
    }

    /// elicitation 提交：`values` 可以是 object，也可以是 `null`（ACP `content: null`）。
    pub fn elicitation_submit(values: ElicitationValues) -> Result<Self, InvalidValue> {
        Self::elicitation(ElicitationAction::Submit, values)
    }

    /// elicitation 取消：`values` 必须为 `null`。
    pub fn elicitation_cancel() -> Self {
        Self::Elicitation {
            action: ElicitationAction::Cancel,
            values: ElicitationValues::null(),
        }
    }

    /// elicitation 拒绝（A1 定案新增的动作）：`values` 必须为 `null`。
    pub fn elicitation_decline() -> Self {
        Self::Elicitation {
            action: ElicitationAction::Decline,
            values: ElicitationValues::null(),
        }
    }

    /// 按定案口径校验：`submit` ⇒ `null` 或 object 皆可；`cancel`/`decline` ⇒ 必须 `null`。
    pub fn elicitation(
        action: ElicitationAction,
        values: ElicitationValues,
    ) -> Result<Self, InvalidValue> {
        let resolution = Self::Elicitation { action, values };
        resolution.validate()?;
        Ok(resolution)
    }

    /// 校验 `action` 与 `values` 的一致性。
    pub fn validate(&self) -> Result<(), InvalidValue> {
        match self {
            Self::Permission(_) => Ok(()),
            Self::Elicitation { action, values } => check_elicitation(*action, values),
        }
    }
}

/// 定案口径（A1）：`submit` ⇒ `null` 或 object 皆可；`cancel`/`decline` ⇒ 必须 `null`。
fn check_elicitation(
    action: ElicitationAction,
    values: &ElicitationValues,
) -> Result<(), InvalidValue> {
    match action {
        ElicitationAction::Submit => values.validate(),
        ElicitationAction::Cancel | ElicitationAction::Decline => {
            if values.is_null() {
                Ok(())
            } else {
                Err(InvalidValue::ElicitationValues)
            }
        }
    }
}

token_enum!(
    /// 交互解析结果（`SYNC_PROTOCOL.md` §11.5）。
    Resolution {
        Resolved => "resolved",
        AlreadyResolved => "already_resolved",
    }
);

/// 未解决交互（`NODE_LINK_PROTOCOL.md` §12.4 的快照元数据 + §3.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingInteraction {
    id: InteractionId,
    kind: InteractionKind,
    session: SessionId,
    created_at: Timestamp,
    options: Vec<InteractionOption>,
}

impl PendingInteraction {
    /// 构造。
    pub fn try_new(
        id: InteractionId,
        kind: InteractionKind,
        session: SessionId,
        created_at: Timestamp,
        options: Vec<InteractionOption>,
    ) -> Result<Self, InvalidValue> {
        Ok(Self {
            id,
            kind,
            session,
            created_at,
            options,
        })
    }

    /// 交互标识。
    pub fn id(&self) -> &InteractionId {
        &self.id
    }

    /// 类别。
    pub fn kind(&self) -> InteractionKind {
        self.kind
    }

    /// 所属会话。
    pub fn session(&self) -> &SessionId {
        &self.session
    }

    /// 创建时间。
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }

    /// 候选项（permission 请求的原文；elicitation 为空）。
    pub fn options(&self) -> &[InteractionOption] {
        &self.options
    }
}
