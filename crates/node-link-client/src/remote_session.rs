//! 远程会话：把 Owner 导出的会话实现为会话级 [`SessionEndpoint`](acp_core::ports::SessionEndpoint)。
//!
//! 事实边界（`NODE_LINK_PROTOCOL.md` §12.7、`CORE_PORTS_AND_STORAGE.md` §5.1）：
//!
//! - **创建**：`RemoteSession::create` 发送 `session.create`，只携带 Owner catalog 里的 `agentId` 与
//!   `directoryId`。它**绝不**把 Access 本机的 `canonical_path` 或别名字符串放进 payload（Owner 面在
//!   schema 层就用 `additionalProperties: false` 拒绝 `cwd`）。
//! - **命令**：`prompt`/`cancel`/`set_mode`/`set_config` 都走 `command.submit`；`requestId` 由调用方
//!   给出并在重试时复用（本模块不从连接状态里另造）。断线后的等待以 [`ClientError::Uncertain`] 结束，
//!   通过 [`RemoteSession::outcome`] 暴露给上层，**不**自动重发。
//! - **读取**：`session.read` 是唯一的历史入口，返回 Owner 的结果**原样**（[`RemoteReadResult`]）。
//!   把该结果投影成 Sync/Core 的类型属于 Access facade（本仓库尚未落地），本 crate 不做第二套解释；
//!   因此 `SessionEndpoint::read_history` 显式返回 [`UnavailableKind::BackendUnsupported`]，而不是返回
//!   空页或伪造的结构化历史。
//! - **有效交互**：`resolve_interaction` 映射 `permission.resolve` / `elicitation.respond`；版本由调用
//!   方给出（协商 `node-link.pending-interactions.v1` 时 Owner 要求它）。没有版本时传 `None`——Owner
//!   按合同拒绝时会给出结构化错误，本层不猜测版本。
//! - **attachment**：每个远程会话在使用前申请 `resource.attach`；重连由 supervisor 重新申请，ticket
//!   不跨连接复用。
//!
//! `SessionEndpoint::modes`/`list_config` 的**类型化**投影同样属于 facade：本 crate 只提供
//! [`RemoteSession::query_mode_list`]/[`RemoteSession::query_config_list`] 两个原样返回路径。

use std::sync::Arc;

use acp_core::model::RemoteSessionRef;
use acp_core::model::{
    AgentId, AgentRef, AgentSessionId, ConfigOption, ConfigOptionId, ConfigValue, DirectoryId,
    InteractionId, InteractionResolution, ModeId, ModeState, PortError, PromptRequest, RequestId,
    SessionId, SessionReference, SessionTitle, Timestamp, TurnId, UnavailableKind,
};
use acp_core::ports::{
    EventSink, HistoryInclude, HistoryPage, HistoryQuery, RemoteReadQuery, SessionBackendFactory,
    SessionEndpoint, TurnAccepted,
};
use node_link_protocol::command::{
    CommandName, CommandPayload, CommandSubmit, ElicitationAction as WireAction,
    ElicitationRespond, IncludeResource, PermissionResolve, PromptContentBlock, PromptContentList,
    SessionCancel, SessionConfigList, SessionConfigSet, SessionCreate, SessionModeList,
    SessionModeSet, SessionPrompt, SessionRead, SessionReadBefore,
    SessionTitle as WireSessionTitle, SessionTitleSet,
};
use node_link_protocol::common::{
    AgentId as WireAgentId, DecimalString, DirectoryId as WireDirectoryId, Nullable, RawObject,
    SessionMeta, Uuid,
};
use node_link_protocol::resource::EventPayload as WireEventPayload;
use uuid::Uuid as RandomUuid;

use crate::error::ClientError;
use crate::handshake::{PENDING_INTERACTIONS_FEATURE, REMOTE_HISTORY_FEATURE};
use crate::link::{AttachmentTicket, CommandOutcome, Completion, SharedLink};

/// `session.read` 的原样结果（Owner 的 `command.terminal.result` 对象）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteReadResult {
    /// Owner 返回的结果对象（字节保真）。
    pub result: RawObject,
}

/// 创建结果：Owner 分配的会话引用与元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedRemoteSession {
    /// `remoteSessionRef`（Owner 的权威标识）。
    pub reference: RemoteSessionRef,
    /// 会话状态与版本。
    pub meta: SessionMeta,
}

/// 一次远程会话。
pub struct RemoteSession {
    shared: Arc<SharedLink>,
    remote: RemoteSessionRef,
    /// 最近一次被接受（`command.accepted`）的 turn；只用于 `cancel` 的默认值，**不是** Owner 的 turn id。
    last_turn: std::sync::Mutex<Option<TurnId>>,
}

impl std::fmt::Debug for RemoteSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteSession")
            .field("owner", &self.remote.owner_node_id)
            .field("session", &self.remote.session_id)
            .finish_non_exhaustive()
    }
}

impl RemoteSession {
    /// 打开一个已存在的远程会话（不触发网络；attachment 在第一次使用时申请）。
    pub fn open(shared: Arc<SharedLink>, remote: RemoteSessionRef) -> Self {
        Self {
            shared,
            remote,
            last_turn: std::sync::Mutex::new(None),
        }
    }

    /// 创建远程会话：`session.create` 只携带 Owner catalog 的 `agentId`/`directoryId`。
    pub async fn create(
        shared: Arc<SharedLink>,
        agent: &AgentId,
        directory: &DirectoryId,
        request: RequestId,
    ) -> Result<Self, PortError> {
        let payload = SessionCreate {
            agent_id: WireAgentId::parse(agent.as_str()).map_err(|_| {
                PortError::InvalidRequest("the agent id is not encodable for node link")
            })?,
            directory_id: WireDirectoryId::parse(directory.as_str()).map_err(|_| {
                PortError::InvalidRequest("the directory id is not encodable for node link")
            })?,
        };
        let submit = CommandSubmit {
            request_id: wire_request(&request)?,
            command: CommandName::SessionCreate,
            session_ref: Nullable::null(),
            attachment_id: Nullable::null(),
            attachment_generation: Nullable::null(),
            expected_version: Nullable::null(),
            payload: CommandPayload::SessionCreate(payload),
        };
        let outcome = shared
            .submit(submit, Completion::Terminal)
            .await
            .map_err(|error| error.to_port_error())?;
        let (reference, meta) = match outcome {
            CommandOutcome::Terminal(terminal)
                if terminal.status == node_link_protocol::command::TerminalStatus::Completed =>
            {
                match terminal.result.as_ref() {
                    Some(node_link_protocol::command::CommandResultPayload::SessionCreate(
                        result,
                    )) => (
                        result.remote_session_ref.clone(),
                        result.session_meta.clone(),
                    ),
                    _ => {
                        return Err(PortError::Backend(Box::new(std::io::Error::other(
                            "session.create completed without a session result",
                        ))));
                    }
                }
            }
            other => return Err(command_failure(other)),
        };
        let session_id = SessionId::new(reference.session_id.as_str())
            .map_err(|_| PortError::InvalidRequest("the owner returned an invalid session id"))?;
        let remote = RemoteSessionRef::new(
            {
                let owner = shared.owner().clone();
                if owner.as_str() != reference.owner_node_id.as_str() {
                    return Err(PortError::InvalidRequest(
                        "session.create returned a session of another owner",
                    ));
                }
                owner
            },
            session_id,
        );
        let _ = meta;
        Ok(Self::open(shared, remote))
    }

    /// 远程会话引用（core 类型）。
    pub fn remote(&self) -> &RemoteSessionRef {
        &self.remote
    }

    /// 当前 attachment ticket（未 attach 时申请）。
    async fn ticket(&self) -> Result<AttachmentTicket, ClientError> {
        self.shared.attach(wire_remote(&self.remote)?).await
    }

    /// 读取当前会话的版本（`set_mode`/`set_config` 的 `expectedVersion` 来源）。
    async fn version(&self) -> Result<DecimalString, PortError> {
        // 版本来自当前连接的 attachment 元数据（运行期事实，不读任何本地索引）。
        self.ticket()
            .await
            .map(|ticket| ticket.meta.version)
            .map_err(|error| error.to_port_error())
    }

    /// 提交一条会话范围命令。
    async fn submit_session(
        &self,
        request: RequestId,
        command: CommandName,
        payload: CommandPayload,
        completion: Completion,
        expected_version: Nullable<DecimalString>,
    ) -> Result<CommandOutcome, ClientError> {
        let ticket = self.ticket().await?;
        let submit = CommandSubmit {
            request_id: wire_request(&request)
                .map_err(|_| ClientError::Local("the request id is not a canonical uuid"))?,
            command,
            session_ref: Nullable::value(wire_remote(&self.remote)?),
            attachment_id: Nullable::value(ticket.id.clone()),
            attachment_generation: Nullable::value(ticket.generation.clone()),
            expected_version,
            payload,
        };
        self.shared.submit(submit, completion).await
    }

    /// `session.read`：返回 Owner 的结果对象原样。
    ///
    /// `include` 是 wire 的 `IncludeResource` 列表（调用方按实际投影需要给出）。`beforeHistory`、
    /// `before` 与 `limit` 只在协商 `node-link.remote-history.v1` 后由 Owner 接受；未协商的连接由调用方
    /// 省略它们（本方法只按入参编码，不自行猜测）。
    pub async fn read_remote(
        &self,
        request: RequestId,
        include: Vec<IncludeResource>,
        before_history: Option<DecimalString>,
        before: Option<SessionReadBefore>,
        limit: Option<u64>,
    ) -> Result<RawObject, PortError> {
        let include_text = serde_json::to_string(&include)
            .map_err(|_| PortError::InvalidRequest("the include list cannot be encoded"))?;
        let payload = SessionRead {
            include: serde_json::from_str(&include_text)
                .map_err(|_| PortError::InvalidRequest("the include list is not encodable"))?,
            before_history,
            before,
            limit,
        };
        let outcome = self
            .submit_session(
                request,
                CommandName::SessionRead,
                CommandPayload::SessionRead(payload),
                Completion::Terminal,
                Nullable::null(),
            )
            .await
            .map_err(|error| error.to_port_error())?;
        match outcome {
            CommandOutcome::Terminal(terminal)
                if terminal.status == node_link_protocol::command::TerminalStatus::Completed =>
            {
                match terminal.result.as_ref() {
                    Some(node_link_protocol::command::CommandResultPayload::Object(object)) => {
                        Ok(object.clone())
                    }
                    _ => Err(PortError::Unavailable(UnavailableKind::BackendUnsupported)),
                }
            }
            other => Err(command_failure(other)),
        }
    }

    /// `session.read` 的运行期入口：把 core 的分页参数翻译成 Node Link payload，返回 Owner 的完成
    /// 结果对象原样（类型化投影与覆盖校验在 `runtime` 内完成）。
    ///
    /// 正文不落盘：返回的对象只在调用栈与有界运行期内存中存在。
    pub async fn read_document(&self, query: RemoteReadQuery) -> Result<RawObject, PortError> {
        if !self.shared.is_online() {
            return Err(PortError::Unavailable(UnavailableKind::OwnerOffline));
        }
        if query.include.turns || query.include.capabilities {
            return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
        }
        let features = self.shared.features();
        let has = |feature: &str| features.iter().any(|entry| entry == feature);
        if query.include.history && !has(REMOTE_HISTORY_FEATURE) {
            return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
        }
        if query.include.pending_interactions && !has(PENDING_INTERACTIONS_FEATURE) {
            return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
        }
        if (query.before_history.is_some() || query.before_messages.is_some())
            && !has(REMOTE_HISTORY_FEATURE)
        {
            return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
        }
        let remote_history = has(REMOTE_HISTORY_FEATURE);
        let include = wire_include(&query.include)?;
        let before_history = match query.before_history {
            Some(sequence) if remote_history => Some(
                DecimalString::parse(&sequence.get().to_string()).map_err(|_| {
                    PortError::InvalidRequest("the history cursor is not encodable")
                })?,
            ),
            _ => None,
        };
        let before = match query.before_messages.as_ref() {
            Some(cursor) if remote_history => Some(SessionReadBefore {
                created_at: node_link_protocol::common::Timestamp::parse(
                    cursor.created_at.as_str(),
                )
                .map_err(|_| PortError::InvalidRequest("the message cursor is not a timestamp"))?,
                message_id: Uuid::parse(cursor.message_id.as_str()).map_err(|_| {
                    PortError::InvalidRequest("the message cursor is not a canonical uuid")
                })?,
            }),
            _ => None,
        };
        let limit = match (remote_history, query.limit.events()) {
            (true, value @ 1..) => Some(u64::from(value)),
            _ => None,
        };
        self.read_remote(new_request_id()?, include, before_history, before, limit)
            .await
    }

    /// `session.mode.list`：返回 Owner 的结果对象原样（类型化投影属 facade）。
    pub async fn query_mode_list(&self, request: RequestId) -> Result<RawObject, PortError> {
        let outcome = self
            .submit_session(
                request,
                CommandName::SessionModeList,
                CommandPayload::SessionModeList(SessionModeList {}),
                Completion::Terminal,
                Nullable::null(),
            )
            .await
            .map_err(|error| error.to_port_error())?;
        open_object(outcome)
    }

    /// `session.config.list`：返回 Owner 的结果对象原样（类型化投影属 facade）。
    pub async fn query_config_list(&self, request: RequestId) -> Result<RawObject, PortError> {
        let outcome = self
            .submit_session(
                request,
                CommandName::SessionConfigList,
                CommandPayload::SessionConfigList(SessionConfigList {}),
                Completion::Terminal,
                Nullable::null(),
            )
            .await
            .map_err(|error| error.to_port_error())?;
        open_object(outcome)
    }

    /// 最近一次被接受的 turn（仅用于 `cancel` 的默认值；Owner 的 turn id 不在此暴露）。
    pub fn last_turn(&self) -> Option<TurnId> {
        self.last_turn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// 稳定 requestId 的 prompt 提交（重试必须复用同一个 `request`）。
    pub async fn prompt_with_request(
        &self,
        request: RequestId,
        prompt: PromptRequest,
        submission: Option<Uuid>,
    ) -> Result<TurnAccepted, PortError> {
        let content = wire_prompt_content(&prompt)?;
        let payload = SessionPrompt {
            content,
            submission_id: submission,
        };
        let outcome = self
            .submit_session(
                request,
                CommandName::SessionPrompt,
                CommandPayload::SessionPrompt(payload),
                Completion::Accepted,
                Nullable::null(),
            )
            .await
            .map_err(|error| error.to_port_error())?;
        match outcome {
            CommandOutcome::Accepted { .. } => {
                let turn = TurnId::new(&RandomUuid::new_v4().to_string())
                    .map_err(|_| PortError::InvalidRequest("turn id cannot be generated"))?;
                *self
                    .last_turn
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(turn.clone());
                Ok(TurnAccepted { turn })
            }
            other => Err(command_failure(other)),
        }
    }

    /// 一次 `command.status` 查询（只读；恢复路径不使用副作用命令）。
    pub async fn status(
        &self,
        request: RequestId,
        target: RequestId,
    ) -> Result<CommandOutcome, PortError> {
        let submit = crate::link::status_submit(wire_request(&request)?, wire_request(&target)?)
            .map_err(|error| error.to_port_error())?;
        self.shared
            .submit(submit, Completion::Terminal)
            .await
            .map_err(|error| error.to_port_error())
    }

    /// `session.title.set`：Owner 元数据命令（`design.md` D2/D3）。
    ///
    /// 三条约束都在这里落实：`request` 由调用方给出并**原样使用**（重试复用同一 id，不在此换号）；
    /// `expectedVersion` 恒为 `null`（合同不接受版本确认，按 Owner 提交顺序后写生效）；attachment 与代际
    /// 取当前连接的路由票证（与 `set_mode`/`set_config` 同一 `submit_session` 模式）。等待到 Owner 的
    /// 终态：该命令在 Owner 侧与终态同事务提交，因此正常情况下不会多等一轮。
    ///
    /// 本方法只转发，不保存标题、不缓存结果；`uncertain` 等结果仍按 [`CommandOutcome`] 表达，调用方
    /// （[`crate::runtime::RemoteRuntime`]）据此决定是否显式回查。
    pub async fn set_title(
        &self,
        request: RequestId,
        title: &SessionTitle,
    ) -> Result<CommandOutcome, ClientError> {
        let title = WireSessionTitle::parse(title.as_str())
            .map_err(|_| ClientError::Local("the title is not encodable for node link"))?;
        let payload = SessionTitleSet { title };
        self.submit_session(
            request,
            CommandName::SessionTitleSet,
            CommandPayload::SessionTitleSet(payload),
            Completion::Terminal,
            Nullable::null(),
        )
        .await
    }
}

#[async_trait::async_trait]
impl SessionEndpoint for RemoteSession {
    fn reference(&self) -> SessionReference {
        SessionReference::Remote(self.remote.clone())
    }

    fn agent_session_id(&self) -> Option<&AgentSessionId> {
        // Node Link v1 的 catalog/ready 不下发 ACP 会话标识；不编造占位值（§3.6）。
        None
    }

    async fn prompt(
        &self,
        request: PromptRequest,
        _at: Timestamp,
    ) -> Result<TurnAccepted, PortError> {
        let request_id = new_request_id()?;
        self.prompt_with_request(request_id, request, None).await
    }

    async fn validate_prompt(&self, _request: &PromptRequest) -> Result<(), PortError> {
        Ok(())
    }

    async fn cancel(&self, turn: Option<TurnId>) -> Result<(), PortError> {
        let Some(turn) = turn.or_else(|| self.last_turn()) else {
            return Err(PortError::InvalidRequest(
                "node link session.cancel requires a turn id",
            ));
        };
        let payload = SessionCancel {
            turn_id: Uuid::parse(turn.as_str())
                .map_err(|_| PortError::InvalidRequest("the turn id is not a canonical uuid"))?,
        };
        let outcome = self
            .submit_session(
                new_request_id()?,
                CommandName::SessionCancel,
                CommandPayload::SessionCancel(payload),
                Completion::Accepted,
                Nullable::null(),
            )
            .await
            .map_err(|error| error.to_port_error())?;
        match outcome {
            CommandOutcome::Accepted { .. } => Ok(()),
            other => Err(command_failure(other)),
        }
    }

    async fn set_mode(&self, mode: &ModeId) -> Result<(), PortError> {
        let payload = SessionModeSet {
            mode_id: node_link_protocol::common::NonEmptyText::<256>::parse(mode.as_str())
                .map_err(|_| PortError::InvalidRequest("the mode id is not encodable"))?,
        };
        let version = self.version().await?;
        let outcome = self
            .submit_session(
                new_request_id()?,
                CommandName::SessionModeSet,
                CommandPayload::SessionModeSet(payload),
                Completion::Accepted,
                Nullable::value(version),
            )
            .await
            .map_err(|error| error.to_port_error())?;
        match outcome {
            CommandOutcome::Accepted { .. } => Ok(()),
            other => Err(command_failure(other)),
        }
    }

    async fn list_config(&self) -> Result<Vec<ConfigOption>, PortError> {
        // 类型化投影属 Access facade；不返回空列表冒充「没有配置项」。
        Err(PortError::Unavailable(UnavailableKind::BackendUnsupported))
    }

    async fn modes(&self) -> Result<ModeState, PortError> {
        // 同上：空候选会被误读成「Owner 没有模式」，因此显式不支持。
        Err(PortError::Unavailable(UnavailableKind::BackendUnsupported))
    }

    async fn set_config(&self, id: &ConfigOptionId, value: ConfigValue) -> Result<(), PortError> {
        let wire_value = match &value {
            ConfigValue::Text(text) => node_link_protocol::command::ConfigValue::String(
                node_link_protocol::common::NonEmptyText::<256>::parse(text.as_str())
                    .map_err(|_| PortError::InvalidRequest("the config value is not encodable"))?,
            ),
            ConfigValue::Select(select) => node_link_protocol::command::ConfigValue::String(
                node_link_protocol::common::NonEmptyText::<256>::parse(select.as_str())
                    .map_err(|_| PortError::InvalidRequest("the config value is not encodable"))?,
            ),
            ConfigValue::Boolean(flag) => node_link_protocol::command::ConfigValue::Boolean(*flag),
        };
        let payload = SessionConfigSet {
            config_id: node_link_protocol::common::NonEmptyText::<256>::parse(id.as_str())
                .map_err(|_| PortError::InvalidRequest("the config id is not encodable"))?,
            value: wire_value,
        };
        let version = self.version().await?;
        let outcome = self
            .submit_session(
                new_request_id()?,
                CommandName::SessionConfigSet,
                CommandPayload::SessionConfigSet(payload),
                Completion::Accepted,
                Nullable::value(version),
            )
            .await
            .map_err(|error| error.to_port_error())?;
        match outcome {
            CommandOutcome::Accepted { .. } => Ok(()),
            other => Err(command_failure(other)),
        }
    }

    async fn resolve_interaction(
        &self,
        interaction: &InteractionId,
        resolution: InteractionResolution,
    ) -> Result<(), PortError> {
        self.resolve_interaction_versioned(interaction, None, resolution)
            .await
    }

    async fn read_history(&self, _query: HistoryQuery) -> Result<HistoryPage, PortError> {
        // core 的 `HistoryPage` 以 owned 行（事件 + `owned_message`）为原料，imported 会话没有这些行；
        // 把 Owner 的完成结果硬折成它只会丢掉 user/agent 角色或正文块。Access 的 `session.read` 因此经
        // [`RemoteSessionReads::read_imported`] 原样转发 Owner 的结果，本方法保持显式不可用。
        Err(PortError::Unavailable(UnavailableKind::BackendUnsupported))
    }

    async fn close(&self) -> Result<(), PortError> {
        // 远程会话没有本地运行时；关闭只释放本端引用。attachment 随连接关闭失效。
        Ok(())
    }
}

/// core 的 `HistoryInclude` → Node Link 的 `include` 词表（顺序与 schema 的枚举一致）。
fn wire_include(include: &HistoryInclude) -> Result<Vec<IncludeResource>, PortError> {
    let mut items = Vec::new();
    if include.messages {
        items.push(IncludeResource::Messages);
    }
    if include.pending_interactions {
        items.push(IncludeResource::PendingInteractions);
    }
    if include.config_options {
        items.push(IncludeResource::ConfigOptions);
    }
    if include.history {
        items.push(IncludeResource::History);
    }
    if include.turns || include.capabilities {
        return Err(PortError::Unavailable(UnavailableKind::BackendUnsupported));
    }
    if items.is_empty() {
        return Err(PortError::InvalidRequest(
            "session.read requires at least one include",
        ));
    }
    Ok(items)
}

impl RemoteSession {
    /// 带 `requestVersion` 的交互解析（协商 `node-link.pending-interactions.v1` 时必填）。
    pub async fn resolve_interaction_versioned(
        &self,
        interaction: &InteractionId,
        request_version: Option<acp_core::model::Version>,
        resolution: InteractionResolution,
    ) -> Result<(), PortError> {
        let interaction_id = Uuid::parse(interaction.as_str())
            .map_err(|_| PortError::InvalidRequest("the interaction id is not a canonical uuid"))?;
        let request_version = request_version
            .map(|version| {
                DecimalString::parse(&version.get().to_string())
                    .map_err(|_| PortError::InvalidRequest("the request version is not encodable"))
            })
            .transpose()?;
        let (command, payload) = match resolution {
            InteractionResolution::Permission(decision) => (
                CommandName::PermissionResolve,
                CommandPayload::PermissionResolve(PermissionResolve {
                    interaction_id,
                    request_version,
                    option_id: node_link_protocol::common::NonEmptyText::<128>::parse(
                        decision.option_id(),
                    )
                    .map_err(|_| {
                        PortError::InvalidRequest("the permission option id is not encodable")
                    })?,
                }),
            ),
            InteractionResolution::Elicitation { action, values } => {
                let (wire_action, wire_values) = match action {
                    acp_core::model::ElicitationAction::Submit => {
                        let text = values.to_json_text();
                        if text == "null" {
                            (WireAction::Submit, Nullable::null())
                        } else {
                            (
                                WireAction::Submit,
                                Nullable::value(RawObject::parse(&text).map_err(|_| {
                                    PortError::InvalidRequest(
                                        "the elicitation values are not an object",
                                    )
                                })?),
                            )
                        }
                    }
                    acp_core::model::ElicitationAction::Decline => {
                        (WireAction::Decline, Nullable::null())
                    }
                    acp_core::model::ElicitationAction::Cancel => {
                        (WireAction::Cancel, Nullable::null())
                    }
                };
                (
                    CommandName::ElicitationRespond,
                    CommandPayload::ElicitationRespond(ElicitationRespond {
                        interaction_id,
                        request_version,
                        action: wire_action,
                        values: wire_values,
                    }),
                )
            }
        };
        let outcome = self
            .submit_session(
                new_request_id()?,
                command,
                payload,
                Completion::Accepted,
                Nullable::null(),
            )
            .await
            .map_err(|error| error.to_port_error())?;
        match outcome {
            CommandOutcome::Accepted { .. } => Ok(()),
            other => Err(command_failure(other)),
        }
    }
}

/// `SessionBackendFactory` 的远程实现。
///
/// 为什么 `create` 显式不支持：core 的 [`CreateSessionRequest`](acp_core::model::CreateSessionRequest)
/// 携带的是 **Access 本机**的 `ResolvedWorkspace`（canonical path + 由路径派生的目录标识），而 Node Link
/// 的 `session.create` 只接受 Owner catalog 的 `directoryId`。把 Access 的目录标识当成 Owner 的目录标识
/// 发出去就是「把 Access 本地路径当成 Owner 路径」——这正是需求禁止的错误。远程创建因此走
/// [`RemoteSession::create`]（显式接收 Owner 的 `agentId`/`directoryId`），由 Access facade 在拿到
/// catalog 后调用。
pub struct RemoteSessionFactory {
    shared: Arc<SharedLink>,
}

impl RemoteSessionFactory {
    /// 装配。
    pub fn new(shared: Arc<SharedLink>) -> Self {
        Self { shared }
    }

    /// 打开一个已登记的 imported 会话。
    pub fn session(&self, remote: RemoteSessionRef) -> RemoteSession {
        RemoteSession::open(Arc::clone(&self.shared), remote)
    }
}

#[async_trait::async_trait]
impl SessionBackendFactory for RemoteSessionFactory {
    async fn create(
        &self,
        _session: &SessionId,
        _request: acp_core::model::CreateSessionRequest,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        Err(PortError::Unavailable(UnavailableKind::BackendUnsupported))
    }

    async fn open(
        &self,
        reference: SessionReference,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        match reference {
            SessionReference::Remote(remote) => Ok(Box::new(self.session(remote))),
            SessionReference::Owned(_) => Err(PortError::InvalidRequest(
                "node-link-client only serves imported sessions",
            )),
        }
    }

    async fn resume(
        &self,
        _session: &SessionId,
        _request: acp_core::model::ResumeSessionRequest,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        Err(PortError::Unavailable(UnavailableKind::BackendUnsupported))
    }

    async fn terminate_agent(&self, _session: &SessionId) -> Result<(), PortError> {
        // 远程会话没有本机进程可结束；空操作是唯一正确语义。
        Ok(())
    }
}

fn wire_prompt_content(prompt: &PromptRequest) -> Result<PromptContentList, PortError> {
    let blocks = prompt
        .content
        .iter()
        .map(|block| {
            serde_json::from_str::<PromptContentBlock>(block.as_str()).map_err(|_| {
                PortError::InvalidRequest(
                    "a prompt content block is not representable on node link v1",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let text = serde_json::to_string(&blocks)
        .map_err(|_| PortError::InvalidRequest("the prompt content cannot be encoded"))?;
    serde_json::from_str::<PromptContentList>(&text)
        .map_err(|_| PortError::InvalidRequest("the prompt content is not encodable"))
}

pub(crate) fn wire_remote(
    remote: &RemoteSessionRef,
) -> Result<node_link_protocol::common::RemoteSessionRef, ClientError> {
    Ok(node_link_protocol::common::RemoteSessionRef {
        owner_node_id: Uuid::parse(remote.owner_node_id.as_str())
            .map_err(|_| ClientError::Local("the owner node id is not a canonical uuid"))?,
        session_id: Uuid::parse(remote.session_id.as_str())
            .map_err(|_| ClientError::Local("the session id is not a canonical uuid"))?,
    })
}

fn wire_request(request: &RequestId) -> Result<Uuid, PortError> {
    Uuid::parse(request.as_str())
        .map_err(|_| PortError::InvalidRequest("the request id is not a canonical uuid"))
}

fn new_request_id() -> Result<RequestId, PortError> {
    RequestId::new(&RandomUuid::new_v4().to_string())
        .map_err(|_| PortError::InvalidRequest("request id cannot be generated"))
}

/// 把命令结果映射成端口错误；`uncertain` 保持为「现在无法确认」，不伪造成功。
fn command_failure(outcome: CommandOutcome) -> PortError {
    match outcome {
        CommandOutcome::Rejected(error) => {
            let code = error.code.as_str().to_owned();
            let retryable = error.retryable;
            ClientError::Rejected {
                code,
                retryable,
                error: core_public_error(&error),
            }
            .to_port_error()
        }
        CommandOutcome::Terminal(terminal) => match terminal.status {
            node_link_protocol::command::TerminalStatus::Uncertain => {
                ClientError::Uncertain.to_port_error()
            }
            node_link_protocol::command::TerminalStatus::Rejected => {
                match terminal.error.as_ref() {
                    Some(error) => ClientError::Rejected {
                        code: error.code.as_str().to_owned(),
                        retryable: error.retryable,
                        error: core_public_error(error),
                    }
                    .to_port_error(),
                    None => PortError::Unavailable(UnavailableKind::BackendUnsupported),
                }
            }
            _ => PortError::Unavailable(UnavailableKind::RemoteUnavailable),
        },
        CommandOutcome::Accepted { .. } => {
            PortError::Unavailable(UnavailableKind::RemoteUnavailable)
        }
    }
}

fn core_public_error(
    error: &node_link_protocol::common::PublicError,
) -> acp_core::model::PublicError {
    let details = acp_core::model::ViewJson::new(error.details.get())
        .unwrap_or_else(|_| acp_core::model::ViewJson::empty_object());
    acp_core::model::PublicError::try_new(
        error.code.as_str(),
        error.message.as_str(),
        error.retryable,
        details,
    )
    .unwrap_or_else(|_| {
        // wire 已按 schema 校验过 code/message；这里的回退只在库被外部改写时触发。
        acp_core::model::PublicError::coded(
            "internal.unavailable",
            "the owner reported an invalid error",
            false,
        )
        .expect("the fallback error code is valid")
    })
}

fn open_object(outcome: CommandOutcome) -> Result<RawObject, PortError> {
    match outcome {
        CommandOutcome::Terminal(terminal)
            if terminal.status == node_link_protocol::command::TerminalStatus::Completed =>
        {
            match terminal.result.as_ref() {
                Some(node_link_protocol::command::CommandResultPayload::Object(object)) => {
                    Ok(object.clone())
                }
                _ => Err(PortError::Unavailable(UnavailableKind::BackendUnsupported)),
            }
        }
        other => Err(command_failure(other)),
    }
}

/// 供上层读取事件 payload 的窄入口（facade 的类型化投影会用到；本 crate 不重新解释 view）。
pub fn wire_payload(event: &node_link_protocol::resource::Event) -> &WireEventPayload {
    &event.payload
}

/// core 的 `AgentRef` 与 Owner catalog 的 `agentId` 只做标识映射（不携带本地路径）。
pub fn agent_reference(agent: &AgentRef) -> Option<WireAgentId> {
    WireAgentId::parse(agent.agent_id().as_str()).ok()
}

/// 命令名到 core 的 `RequestId` 形状校验（保持 newtype 边界）。
pub fn request_from_wire(uuid: &Uuid) -> Option<RequestId> {
    RequestId::new(uuid.as_str()).ok()
}

/// `CreatedRemoteSession` 的元数据访问（调用方通常只需要引用）。
pub fn created_meta(created: &CreatedRemoteSession) -> &SessionMeta {
    &created.meta
}
