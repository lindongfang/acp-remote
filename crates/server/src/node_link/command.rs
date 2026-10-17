//! Node Link 的命令管线与撤销传播（`design.md` D7/D8、`NODE_LINK_PROTOCOL.md` §12.5/§12.7）。
//!
//! 处理链（D8 的五步，顺序即实现顺序）：
//!
//! 1. **wire 边界校验**：`command.submit`/`command.status` 的 body 由 `node-link-protocol` 的类型校验；
//!    唯一的例外是 `session.create` 的禁带字段——schema 会把 `cwd`/`mcpServers`/凭据键判成
//!    `schema_invalid`，而 §12.7 要求更具体的 `nodelink.command.unsupported_field` + `details.field`，
//!    因此本模块在严格解码**之前**先看原始 payload 的键与取值（绝对路径）；
//! 2. **授权交集**：Export grant ∩ 该节点信任记录 grant。core 的 `Broker::authorize` 对 Node actor 只判
//!    「信任记录 grants 含所需 grant 且存在覆盖它的未撤销 Export」，本层按 grant 与命令的会话归属再收敛
//!    到**具体 Export**（`catalog::visible_exports` 是可见性的唯一判定点，见该模块的 D1 三条件）；越权一律
//!    `command.rejected(nodelink.export.not_granted)` 且**无副作用**，并按 R69/R82 写一条
//!    `authorization.denied` 审计——core 自己的拒绝路径会留痕，但本层拒的命令不会到达 core，因此必须
//!    自己写（`UseCases::record_node_link_auth` 接受该动作）；
//! 3. **幂等**：mutation 的幂等键由 core 的 `(actor, requestId)` 承载（Node Link 的 actor 是
//!    `Actor::Node { node: access_node, .. }`，即 §12.5 的 `(ownerNodeId, accessNodeId, requestId)`，
//!    其中 ownerNodeId 是本进程恒定的本机 id），冲突由 core 报 `command.idempotency_conflict`；
//!    `session.create` 走 core 的 `create_session`/`settle_session_create`（§6 第 20 条），幂等行与
//!    终态都持久在 `owned_command` 里——本模块**不再**有进程内幂等表，因此重启后同一 requestId 既不重复
//!    创建、也不丢首次结果。本层在提交前多一道语义比对（§12.5）：已落盘的记录命令名或
//!    ACPR-CJ1 指纹不同即回 `nodelink.command.idempotency_conflict`，**不**回首次结果。
//! 4. **派发**：查询命令同步完成（结果直接放进 `command.terminal`），mutation 同步接受；`session.create`
//!    先回 `accepted(result = null)`，创建后把适配层投影的结果写进终态并**以持久记录为唯一权威**回
//!    `command.terminal`（`terminalEventId` 因此非 null）。后端失败的终态由 **core** 写（按错误类别
//!    `failed`/`uncertain`，零会话行），本层只回读回包；只有「受理提交之前就失败」（授权、参数/别名、
//!    本机 workspace 解析、写盘）没有记录可回读，那时才本地合成同形 `failed` 终态。
//!
//! **命令级日志**（`sync-command-handling` 的「命令级结构化日志」）：每条被接受的业务命令与其终态各记一条
//! 结构化日志（主体标识、命令名、资源标识、请求标识；终态再加类别与封闭词表里的成因）。字段与词表的唯一
//! 产出点是 [`crate::command_log`]。
//! 5. **终态映射**：`completed` 必带非空 `result`（core 的 `CommandResult` 可空，**非空由本层保证**；
//!    `session.create` 的持久结果就是 `SessionCreateResult` 原文，回读时还原成具名变体）、其余 status
//!    必带 `error`、`terminalEventId` 取该记录的终态事件、`uncertain` 原样透传。
//!
//! **终态推送**（§12.5）：每条被接受的 mutation 在本模块的待观察表里挂一条 `(connectionId, requestId)`，
//! 由组合根 spawn 的 [`CommandRoute::dispatch`] 有界轮询 core 的 `command_status` 并推 `command.terminal`。
//! 观察项的取消路径有三条：连接离开注册表、命令到达终态、超过 [`WATCH_MAX_AGE`]；`Shutdown` 触发即结束
//! 整个循环。轮询是 v1 唯一可行的手段——broker 的扇出信号（`CommittedEvent`）不携带 `causation`，无法把
//! 终态定位回 requestId 与连接（WP6 的实现期结论）。
//!
//! **未终结的 mutation 回 `command.accepted`**（不是 `command.terminal`）：wire 的
//! `command.terminal.status` 词表只有 `completed|failed|rejected|uncertain`（Sync 的 `CommandStatus`
//! 多一个 `accepted`），回 `uncertain` 会把对端钉在一个假终态上（且 `uncertain` 是终态、Access 不得重试）。
//! 两种 `command.status` 形式因此在所有情况下都产生同形回复。
//!
//! **撤销传播**（D7）：[`CommandRoute::node_revoked`]/[`CommandRoute::export_revoked`] 把 `local_admin`
//! 的撤销事实变成 `node.trust.revoked`（+ 4410 关闭）与 `export.revoked`（+ 清除内存订阅）。推送失败只
//! 记日志：权威判定始终按当次持久化记录（`catalog::visible_exports` 与 `node_link_session_view`），
//! 因此即使推送丢失，已撤销的 Export/节点也无法继续取资源或发命令。
//!
//! **重新配对的连接作废**：[`CommandRoute::node_reauth`] 与撤销同址同序（提交后关闭），但不推消息、
//! 以正常关闭（1000）让对端重连取新 catalog——授权收窄不是撤销，对端不得据此停止重连。
//!
//! 本切片**显式不支持**的三个查询（`session.read`/`session.mode.list`/`session.config.list`）：它们的
//! wire 结果形状（`sessionReadResult`/`modeListResult`/`configListResult`）需要把正文/活体元数据投影成
//! Sync 登记的视图，其中 `session.read` 的 `messages` 还要事件正文的聚合投影——那属 Access facade 的正文
//! 路径。这里回 `command.rejected(nodelink.command.unsupported)`，**不**返回被裁剪的结果（不得静默丢弃
//! 正文），也不把结构化事件退化成文本。
//!
//! 依赖纪律：只调用 `core::use_cases` 与 `identity-auth` 的公开入口，不查 SQLite、不做平台分支。

use std::collections::BTreeMap;
use std::str::FromStr as _;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use acp_core::model::{
    Actor, AgentId, AgentRef, AuditAction, AuditOutcome, ClientCommand, CommandKind,
    CommandPayload as CorePayload, CommandReceipt, CommandRecord, CommandStatus as CoreStatus,
    ConfigOptionId, ConfigValue as CoreConfigValue, CreateSessionRequest,
    ElicitationAction as CoreElicitationAction, ElicitationValues, EntityRef,
    InteractionId, ModeId, NodeId, NodeState, PortError,
    PromptContentBlock, ReadInclude, RequestId as CoreRequestId, ResourceOrigin, SessionId,
    TemplateSelection, Timestamp as CoreTimestamp, TurnId, Version, DirectoryId,
    WorkspaceSource,
};
use acp_core::ports::{PreparedSessionCreate, SessionCreateAdmission, SessionQuery};
use acp_core::use_cases::{NodeLinkCatalogView, UseCases};
use identity_auth::Authority;
use node_link_protocol::catalog::NodeTrustRevoked;
use node_link_protocol::command::{
    CommandAccepted, CommandName, CommandPayload as WirePayload, CommandRejected,
    CommandResultPayload, CommandStatus as WireStatusBody, CommandSubmit, CommandTerminal,
    ConfigValue as WireConfigValue, ElicitationAction as WireElicitationAction,
    IncludeResource as WireInclude, SessionCreate, Terminal, TerminalStatus,
};
use node_link_protocol::common::{
    Nullable, PublicError, RawObject, RemoteSessionRef,
    SessionCreateResult, SessionMeta, SessionResumeResult, Text, Timestamp as WireTimestamp, Uuid,
};
use node_link_protocol::envelope::{Envelope, MessageType};
use node_link_protocol::error::ErrorCode;
use serde::de::DeserializeOwned;
use serde::de::IgnoredAny;
use sha2::{Digest as _, Sha256};
use tracing::{debug, info, warn};

use crate::command_log::{self, Cause, Subject, TerminalClass};
use crate::node_link::conn::{
    ConnectionHandle, ConnectionRegistry, MessageRoute, RouteOutcome, close,
};
use crate::node_link::resource::ResourceRoute;
use crate::node_link::{catalog, link_error};
use crate::transport::net::{RateLimit, Shutdown, SlidingWindowLimiter};

/// 单连接命令速率（§2.5：120 次/分钟，固定常量、不可配置）。
pub const COMMANDS_PER_MINUTE: u32 = 120;

/// 命令限流的窗口（§2.5 的固定 1 分钟）。
pub const COMMAND_WINDOW: Duration = Duration::from_secs(60);

/// 连续被限流多少次即按 §2.5 关闭连接（4429）：连续超限说明对端没有遵守 `retryAfterMs`。
pub const MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS: u32 = 3;

/// 终态观察的轮询间隔（唯一的终态推送手段，见模块文档）。
pub const TERMINAL_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// 单条终态观察的最长寿命：超过即放弃（只记日志），对端仍可凭 `command.status` 重查。
pub const WATCH_MAX_AGE: Duration = Duration::from_secs(600);

/// in-flight 上限拒绝时的建议退避（`details.retryAfterMs` 是 `nodelink.resource.rate_limited`
/// 的登记字段）。
const IN_FLIGHT_RETRY_AFTER: Duration = Duration::from_millis(1_000);

/// `details.field`（`nodelink.command.unsupported_field` 的必需登记字段）的 schema 上限。
const MAX_DETAILS_FIELD: usize = 128;

/// 超长键的替代字段名：`details.field` 只表达「哪个字段被拒」，不承载任意长度的输入。
const UNKNOWN_FIELD: &str = "<unknown>";

/// 一条连接的命令状态：限流窗口、连续超限计数与尚未观察到终态的命令。
#[derive(Debug)]
struct ConnectionState {
    limiter: SlidingWindowLimiter,
    consecutive_rate_limited: u32,
    pending: Vec<PendingCommand>,
}

/// 待观察终态的 mutation。
#[derive(Debug, Clone)]
struct PendingCommand {
    request: CoreRequestId,
    since: Instant,
}

/// `session.create` / `session.resume` 的首次结果（终态回读与「没有持久记录」时的本地合成都用它）。
///
/// `Completed` 承载 wire 的具名结果变体（[`CommandResultPayload::SessionCreate`] /
/// [`CommandResultPayload::SessionResume`]）而不是 core 的类型：终态**以持久记录为唯一权威**，
/// 而记录里存的就是该形状的原文。
#[derive(Debug, Clone)]
enum LocalOutcome {
    Completed(CommandResultPayload),
    Failed(PublicError),
    Uncertain(PublicError),
}

/// session-scoped 命令引用的会话定位（grant 判定与 attachment 复核共用）。
#[derive(Debug, Clone)]
struct SessionTarget {
    session: SessionId,
    remote: RemoteSessionRef,
}

/// WP6 的命令管线与撤销传播（组合根持有一个 `Arc<CommandRoute>`）。
pub struct CommandRoute {
    core: Arc<UseCases>,
    registry: Arc<ConnectionRegistry>,
    resource: Arc<ResourceRoute>,
    authority: Arc<Authority>,
    /// 本机（Owner）node id：`remoteSessionRef.ownerNodeId` 的来源。
    node_id: NodeId,
    states: Mutex<BTreeMap<String, ConnectionState>>,
    /// 新登记一条终态观察时的唤醒信号（分发循环因此不必等满一个 tick）。
    wake: tokio::sync::Notify,
}

impl std::fmt::Debug for CommandRoute {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandRoute")
            .field("connections", &lock(&self.states).len())
            .finish_non_exhaustive()
    }
}

impl CommandRoute {
    /// 装配。`resource` 是 attachment 的唯一事实来源（session-scoped 命令的代际复核与 Export 撤销后的
    /// 订阅清理都经它），`authority` 提供时钟与 node id（本层不读系统时间、不自造标识）。
    pub fn new(
        core: Arc<UseCases>,
        registry: Arc<ConnectionRegistry>,
        resource: Arc<ResourceRoute>,
        authority: Arc<Authority>,
    ) -> Self {
        Self {
            node_id: authority.local_node().clone(),
            core,
            registry,
            resource,
            authority,
            states: Mutex::new(BTreeMap::new()),
            wake: tokio::sync::Notify::new(),
        }
    }

    /// 本机（Owner）node id（组合根与用例断言共用）。
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// 终态分发循环（组合根 spawn，`Shutdown` 参与关闭序列）。
    pub async fn dispatch(self: Arc<Self>, shutdown: Shutdown) {
        loop {
            let tick = tokio::time::sleep(TERMINAL_POLL_INTERVAL);
            tokio::select! {
                biased;
                () = shutdown.clone().wait() => {
                    debug!(
                        event = "node_link.command_dispatch_shutdown",
                        "the command terminal watcher stops"
                    );
                    return;
                }
                () = self.wake.notified() => {}
                () = tick => {}
            }
            self.poll_pending().await;
        }
    }

    // -----------------------------------------------------------------------------------------
    // 消息入口
    // -----------------------------------------------------------------------------------------

    async fn on_submit(&self, handle: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        // ① `session.create` 的禁带字段：schema 会判成 `schema_invalid`，而 §12.7 要求更具体的
        //    `unsupported_field` + `details.field`，因此先看原始 payload（不创建会话、不部分应用参数）。
        let forbidden = forbidden_payload_field(message.body().get());
        if let Some((request, command, field)) = forbidden {
            return self.reject_field(handle, command, &request, &field);
        }
        let Some(submit) = decode::<CommandSubmit>(message) else {
            return self.reject_schema(handle, message, "the command does not match the v1 schema");
        };
        if !self.admit_rate(handle) {
            return RouteOutcome::Claimed;
        }
        match submit.command {
            CommandName::CommandStatus => {
                let WirePayload::CommandStatus(status) = &submit.payload else {
                    // 由 wire 的判别式保证不可达；保留分支只为不 panic。
                    return self.reject_schema(
                        handle,
                        message,
                        "the command.status payload is not the status payload",
                    );
                };
                let target = status.target_request_id.clone();
                self.on_status(handle, message, &target).await
            }
            CommandName::SessionCreate => self.on_session_create(handle, message, &submit).await,
            CommandName::SessionResume => self.on_session_resume(handle, message, &submit).await,
            command => self.on_dispatched(handle, message, &submit, command).await,
        }
    }

    /// `command.status` 的独立消息形式（等价于 `command.submit{command:"command.status"}`）。
    async fn on_standalone_status(
        &self,
        handle: &ConnectionHandle,
        message: &Envelope,
    ) -> RouteOutcome {
        let Some(status) = decode::<WireStatusBody>(message) else {
            return self.reject_schema(handle, message, "the command.status body is invalid");
        };
        if !self.admit_rate(handle) {
            return RouteOutcome::Claimed;
        }
        self.on_status(handle, message, &status.target_request_id)
            .await
    }

    // -----------------------------------------------------------------------------------------
    // command.status（两种形式归一到本处理器）
    // -----------------------------------------------------------------------------------------

    /// 重查同一 `requestId` 的终态；两种形式在此归一（§12.5：回复使用 `command.terminal` 形状）。
    async fn on_status(
        &self,
        handle: &ConnectionHandle,
        message: &Envelope,
        target: &Uuid,
    ) -> RouteOutcome {
        let Some(request) = core_request(target) else {
            return self.reject_schema(handle, message, "the targetRequestId is not a uuid");
        };
        let record = match self
            .core
            .command_status(&node_actor(handle.node_id()), request)
            .await
        {
            Ok(record) => record,
            Err(error) => return self.port_fault(handle, message, &error),
        };
        let Some(record) = record else {
            // 未知 requestId：`command.rejected`/`command.terminal` 都必须给出该 request 的命令名，
            // 而未知请求没有名字，因此这是连接级错误（不猜、也不伪造命令名）。
            self.send_error(
                handle,
                ErrorCode::CommandNotFound,
                "the request is not known to this owner",
                RawObject::empty(),
                Some(message.message_id()),
            );
            return RouteOutcome::Claimed;
        };
        if record.status().is_terminal() {
            self.send_terminal(handle, &record);
            return RouteOutcome::Claimed;
        }
        // 未终结（core 的 `accepted`，只有 mutation 会留下这种记录）：回 `command.accepted`
        // （`acceptedAt` 取首次接受时间），与首次提交时的回复同形。
        let (Some(command), Some(accepted_at)) =
            (command_name(record.command()), record.accepted_at())
        else {
            warn!(
                event = "node_link.command_status_mapping_failed",
                access_node_id = handle.node_id().as_str(),
                request_id = target.as_str(),
                "the persisted command record cannot be expressed on the wire"
            );
            self.send_error(
                handle,
                ErrorCode::InternalUnavailable,
                "the command record cannot be expressed on the wire",
                RawObject::empty(),
                Some(message.message_id()),
            );
            return RouteOutcome::Claimed;
        };
        self.send_accepted(handle, target, command, accepted_at, None, record.session());
        RouteOutcome::Claimed
    }

    // -----------------------------------------------------------------------------------------
    // 查询与 mutation 的派发
    // -----------------------------------------------------------------------------------------

    async fn on_dispatched(
        &self,
        handle: &ConnectionHandle,
        message: &Envelope,
        submit: &CommandSubmit,
        command: CommandName,
    ) -> RouteOutcome {
        let target = match command.requires_session_attachment() {
            true => match self.session_target(handle, message, submit) {
                Ok(target) => Some(target),
                Err(outcome) => return outcome,
            },
            false => None,
        };
        if let Some(target) = target.as_ref() {
            if let Err(outcome) = self.attachment_is_current(handle, message, submit, target) {
                return outcome;
            }
        }
        let payload = match core_payload(command, submit) {
            Ok(payload) => payload,
            Err(reason) => return self.reject_schema(handle, message, reason),
        };
        if payload.family() == CommandKind::Query {
            return self.on_query(handle, message, submit, command).await;
        }
        if !self.admit_in_flight(handle).await {
            return RouteOutcome::Claimed;
        }
        // 幂等键与持久化记录都以 core 的 `RequestId` 为准（wire 的 `requestId` 是同一 uuid 文本）。
        let Some(request) = core_request(&submit.request_id) else {
            return self.reject_schema(handle, message, "the requestId is not a uuid");
        };
        let Some(fingerprint) = fingerprint_of(&submit.payload) else {
            return self.reject_schema(handle, message, "the payload cannot be fingerprinted");
        };
        let expected_version = match submit.expected_version.as_ref() {
            Some(version) => match version.as_str().parse::<u64>() {
                Ok(value) => Some(Version::new(value)),
                Err(_) => {
                    return self.reject_schema(
                        handle,
                        message,
                        "the expectedVersion is not a decimal string",
                    );
                }
            },
            None => None,
        };
        let actor = node_actor(handle.node_id());
        let client_command = ClientCommand {
            actor: actor.clone(),
            request: request.clone(),
            command: command.as_str().to_owned(),
            kind: CommandKind::Mutation,
            session: target.as_ref().map(|target| target.session.clone()),
            expected_version,
            request_fingerprint: fingerprint,
            payload,
        };
        let receipt = match self.core.submit_command(&actor, client_command).await {
            Ok(receipt) => receipt,
            Err(error) => return self.port_fault(handle, message, &error),
        };
        // 首次结果 = 该 requestId 的持久化记录：已终结即回终态（R67 的「返回首次的 accepted/terminal
        // 结果」），否则回 accepted 并挂一条终态观察。`command.accepted.result` 取 core 收据里的 turn
        // （同键重试走 broker 的幂等路径，同样带上它，两次回复因此同形）。
        let receipt_turn = match receipt {
            CommandReceipt::Rejected { error } => {
                return self.send_rejected(
                    handle,
                    command,
                    &submit.request_id,
                    &error_info(error.code()),
                    RawObject::empty(),
                );
            }
            CommandReceipt::Accepted { turn, .. } => turn,
        };
        let record = match self.core.command_status(&actor, request.clone()).await {
            Ok(record) => record,
            Err(error) => return self.port_fault(handle, message, &error),
        };
        let Some(record) = record else {
            warn!(
                event = "node_link.command_record_missing",
                access_node_id = handle.node_id().as_str(),
                request_id = submit.request_id.as_str(),
                "the accepted mutation has no persisted record"
            );
            self.send_error(
                handle,
                ErrorCode::InternalUnavailable,
                "the accepted command has no persisted record",
                RawObject::empty(),
                Some(message.message_id()),
            );
            return RouteOutcome::Claimed;
        };
        if record.status().is_terminal() {
            self.send_terminal(handle, &record);
            return RouteOutcome::Claimed;
        }
        self.send_accepted(
            handle,
            &submit.request_id,
            command,
            &record
                .accepted_at()
                .cloned()
                .unwrap_or_else(|| self.clock()),
            receipt_turn.as_ref(),
            record.session(),
        );
        self.watch(handle, &request);
        RouteOutcome::Claimed
    }

    /// 查询命令：同步完成，结果直接放进 `command.terminal`（`command.terminal` 始终是权威终态）。
    async fn on_query(
        &self,
        handle: &ConnectionHandle,
        message: &Envelope,
        submit: &CommandSubmit,
        command: CommandName,
    ) -> RouteOutcome {
        let result = match command {
            CommandName::SessionList => {
                let actor = node_actor(handle.node_id());
                let summaries = match self
                    .core
                    .list_sessions(
                        &actor,
                        SessionQuery {
                            only: None,
                            states: Vec::new(),
                            limit: None,
                        },
                    )
                    .await
                {
                    Ok(summaries) => summaries,
                    Err(error) => return self.port_fault(handle, message, &error),
                };
                match self.visible_sessions(handle.node_id(), &summaries).await {
                    Ok(result) => result,
                    Err(error) => return self.port_fault(handle, message, &error),
                }
            }
            // 其余三个查询的 wire 结果形状属 Access facade 的正文路径（见模块文档）：显式回不支持，
            // 不返回被裁剪的结果。
            _ => {
                debug!(
                    access_node_id = handle.node_id().as_str(),
                    command = command.as_str(),
                    request_id = submit.request_id.as_str(),
                    "the query command has no v1 projection in this slice"
                );
                return self.reject_code(
                    handle,
                    command,
                    &submit.request_id,
                    "nodelink.command.unsupported",
                );
            }
        };
        let body = CommandTerminal {
            request_id: submit.request_id.clone(),
            command,
            terminal: Terminal {
                status: TerminalStatus::Completed,
                terminal_at: match wire_timestamp(&self.clock()) {
                    Some(at) => at,
                    None => return self.port_fault(handle, message, &clock_fault()),
                },
                terminal_event_id: Nullable::null(),
                result: Nullable::from_option(Some(CommandResultPayload::Object(result))),
                error: Nullable::null(),
            },
        };
        let _ = handle.send(MessageType::CommandTerminal, &body);
        RouteOutcome::Claimed
    }

    /// `session.list`：已配对节点看到本机全部会话，不按 Export/授权集合过滤。
    async fn visible_sessions(
        &self,
        _node: &NodeId,
        summaries: &[acp_core::model::SessionSummary],
    ) -> Result<RawObject, PortError> {
        let mut sessions = Vec::new();
        for summary in summaries {
            match session_summary(summary) {
                Some(projected) => sessions.push(projected),
                None => warn!(
                    event = "node_link.session_summary_invalid",
                    session_id = summary.session_id().as_str(),
                    "the session summary cannot be expressed on the wire"
                ),
            }
        }
        sessions.sort_by(|left, right| left.session_id.as_str().cmp(right.session_id.as_str()));
        let payload = sync_protocol::command::SessionListResult { sessions };
        let text = serde_json::to_string(&payload)
            .map_err(|_| PortError::Corrupt("session.list result"))?;
        RawObject::parse(&text).map_err(|_| PortError::Corrupt("session.list result"))
    }

    // -----------------------------------------------------------------------------------------
    // session.create
    // -----------------------------------------------------------------------------------------

    async fn on_session_create(
        &self,
        handle: &ConnectionHandle,
        message: &Envelope,
        submit: &CommandSubmit,
    ) -> RouteOutcome {
        let WirePayload::SessionCreate(payload) = &submit.payload else {
            return self.reject_schema(handle, message, "the session.create payload is invalid");
        };
        // ② 参数校验：已经配对即完全信任；只校验 Agent 与目录确实属于本机。
        let view = match self.core.node_link_catalog_view(handle.node_id()).await {
            Ok(view) => view,
            Err(error) => return self.port_fault(handle, message, &error),
        };
        if !view
            .directories
            .iter()
            .any(|record| record.directory_id().as_str() == payload.directory_id.as_str())
        {
            return self.reject_field(
                handle,
                CommandName::SessionCreate,
                &submit.request_id,
                "directoryId",
            );
        }
        if !view
            .agents
            .iter()
            .any(|descriptor| descriptor.agent.agent_id().as_str() == payload.agent_id.as_str())
        {
            return self.reject_field(
                handle,
                CommandName::SessionCreate,
                &submit.request_id,
                "agentId",
            );
        }
        // ③ 幂等：幂等键 `(ownerNodeId, accessNodeId, requestId)` 的**持久事实**在 core 的
        //    `owned_command` 里（§6 第 20 条）。已经有记录就不再创建第二个会话：语义相同则已终结的回
        //    首次结果、未终结的（创建进行中、或崩溃窗口）回同形 `accepted` 并挂终态观察；语义不同回
        //    `nodelink.command.idempotency_conflict`（§12.5：不同语义不得得到首次结果）。
        let Some(fingerprint) = fingerprint_of(&submit.payload) else {
            return self.reject_schema(handle, message, "the payload cannot be fingerprinted");
        };
        let actor = node_actor(handle.node_id());
        let Some(request) = core_request(&submit.request_id) else {
            return self.reject_schema(handle, message, "the requestId is not a uuid");
        };
        match self.core.command_status(&actor, request.clone()).await {
            Ok(Some(record)) => {
                // 指纹是 ACPR-CJ1 之后的解码 payload 摘要（由本层计算，core 不依赖 `acpr-wire`）。
                let same = record.command() == CommandName::SessionCreate.as_str()
                    && record.kind() == CommandKind::Mutation
                    && record.request_fingerprint() == &fingerprint;
                if !same {
                    return self.reject_code(
                        handle,
                        CommandName::SessionCreate,
                        &submit.request_id,
                        "nodelink.command.idempotency_conflict",
                    );
                }
                if record.status().is_terminal() {
                    self.send_terminal(handle, &record);
                } else {
                    let accepted_at = record
                        .accepted_at()
                        .cloned()
                        .unwrap_or_else(|| self.clock());
                    self.send_accepted(
                        handle,
                        &submit.request_id,
                        CommandName::SessionCreate,
                        &accepted_at,
                        None,
                        record.session(),
                    );
                    self.watch(handle, &request);
                }
                return RouteOutcome::Claimed;
            }
            Ok(None) => {}
            Err(error) => return self.port_fault(handle, message, &error),
        }
        if !self.admit_in_flight(handle).await {
            return RouteOutcome::Claimed;
        }
        // ④ 先回 `accepted(result = null)`，创建完成后发 `terminal`（两者连续入队）。
        self.send_accepted(
            handle,
            &submit.request_id,
            CommandName::SessionCreate,
            &self.clock(),
            None,
            None,
        );
        let Ok(agent_id) = AgentId::new(payload.agent_id.as_str()) else {
            return self.reject_schema(handle, message, "the agentId is out of range");
        };
        let agent_name = agent_display_name(&view, payload.agent_id.as_str());
        let Ok(agent) = AgentRef::try_new(agent_id, &agent_name) else {
            return self.reject_schema(handle, message, "the agent name is out of range");
        };
        let Ok(alias) = DirectoryId::new(payload.directory_id.as_str()) else {
            return self.reject_schema(handle, message, "the workspaceAlias is not a symbol name");
        };
        let create = CreateSessionRequest {
            agent,
            workspace: None,
            template: None,
            origin: ResourceOrigin::Local,
        };
        let outcome = match self
            .core
            .create_session(
                &actor,
                request.clone(),
                fingerprint,
                create,
                Some(WorkspaceSource::Directory(alias)),
            )
            .await
        {
            Ok(SessionCreateAdmission::Prepared(prepared)) => {
                // 会话行此刻**尚未落账**（§6 第 20 条）：结果由 core 给出的准备值投影（不回读存储），
                // 随后由下面的终态结算把会话行、幂等行回填与终态一次落盘。
                match self.session_create_result(&prepared) {
                    Ok(result) => {
                        LocalOutcome::Completed(CommandResultPayload::SessionCreate(result))
                    }
                    Err(error) => {
                        // 会话确实已创建：不能用 `failed` 撒谎（那会让 Access 以为没有会话），按 §12.7 的
                        // `uncertain` 语义终结本次命令（Access 不得自动重试）。
                        warn!(
                            event = "node_link.session_create_result_failed",
                            access_node_id = handle.node_id().as_str(),
                            command = CommandName::SessionCreate.as_str(),
                            request_id = submit.request_id.as_str(),
                            cause = Cause::of_port_error(&error).as_str(),
                            "the created session cannot be projected into SessionCreateResult"
                        );
                        LocalOutcome::Uncertain(error_info("nodelink.command.uncertain"))
                    }
                }
            }
            Ok(SessionCreateAdmission::InFlight) => {
                // 键相同、首次创建仍在窗口期（并发重发）：**不**派发第二次创建（§12.7 的「同一 requestId
                // 不产生第二个会话」）。上一步的持久记录检查覆盖不到它——另一条任务尚未来得及回填终态
                // ——而本层已回过一条同形的 `accepted(result = null)`，因此这里只挂终态观察：终态由
                // 首次受理的那一次结算写入，本连接收到它即收尾（与未终结记录那条分支同形）。
                self.watch(handle, &request);
                return RouteOutcome::Claimed;
            }
            Ok(SessionCreateAdmission::Settled(record)) => {
                // 键相同且首次已经终结：回首次结果，不覆盖、不重建（与上一步的已终结分支同形）。
                self.send_terminal(handle, &record);
                return RouteOutcome::Claimed;
            }
            Err(PortError::Conflict(acp_core::model::ConflictKind::IdempotencyConflict)) => {
                // 同键不同语义（§12.5）：回 `command.rejected` 且零副作用。
                return self.reject_code(
                    handle,
                    CommandName::SessionCreate,
                    &submit.request_id,
                    "nodelink.command.idempotency_conflict",
                );
            }
            Err(error) => {
                // 后端的终态由 **core** 写：`create_session` 按错误类别落 `failed`（可证明 Agent 侧未产生
                // 会话）或 `uncertain`（说不清），两者都是**零会话行**的形状，然后把原错误上抛。因此本层
                // 不再自己分类，更不再以「有没有持久记录」当副作用证据——受理记录在受理阶段就已存在
                // （`SessionCreateAdmission` 的三态），拿它当替身会把可证明的失败误报成 `uncertain`
                // （`node-link-owner-server`「受理记录的存在不构成副作用证据」）。成因只写封闭类别：
                // `PortError` 的 `Debug` 可能带出后端文本（含路径）。
                warn!(
                    event = "node_link.session_create_failed",
                    access_node_id = handle.node_id().as_str(),
                    command = CommandName::SessionCreate.as_str(),
                    request_id = submit.request_id.as_str(),
                    cause = Cause::of_port_error(&error).as_str(),
                    "session.create could not be completed"
                );
                return self
                    .reply_after_create_failure(handle, &submit.request_id, &request, &error)
                    .await;
            }
        };
        // 终态落盘（§6 第 20 条）：只有**本层投影出的**终态需要结算——会话行与终态在同一次提交里落盘
        // （投影失败写 `uncertain`：会话已创建，不能报 `failed`）。后端失败那条路径的终态已经由 core
        // 写好，本层不结算也不分类（见上面的 `Err` 臂）。
        let settled = match &outcome {
            LocalOutcome::Completed(result) => match serde_json::to_string(result)
                .ok()
                .and_then(|text| acp_core::model::CommandResult::from_json_text(&text).ok())
            {
                Some(result) => self
                    .core
                    .settle_session_create(
                        &actor,
                        &request,
                        CoreStatus::Completed,
                        Some(result),
                        None,
                    )
                    .await
                    .map(|_| true),
                None => {
                    warn!(
                        event = "node_link.session_create_result_unencodable",
                        request_id = submit.request_id.as_str(),
                        "the created session result cannot be persisted"
                    );
                    Ok(false)
                }
            },
            LocalOutcome::Uncertain(error) => self
                .core
                .settle_session_create(
                    &actor,
                    &request,
                    CoreStatus::Uncertain,
                    None,
                    core_error(error),
                )
                .await
                .map(|_| true),
            // `session.create` 不产出 `Failed` 终态：可证明未产生会话的那一档由 core 按错误类别写好，
            // 本层不再分类。保留臂只为让匹配穷尽（`LocalOutcome` 与 `session.resume` 共用）。
            LocalOutcome::Failed(_) => Ok(false),
        };
        if let Err(error) = settled {
            warn!(
                event = "node_link.session_create_terminal_failed",
                access_node_id = handle.node_id().as_str(),
                request_id = submit.request_id.as_str(),
                cause = Cause::of_port_error(&error).as_str(),
                "the session.create terminal could not be persisted"
            );
        }
        // 回包以**持久记录**为唯一权威：创建与终态同源（`terminalEventId` 可非空），重试与
        // `command.status` 重查因此与首次同形。
        match self.core.command_status(&actor, request.clone()).await {
            Ok(Some(record)) if record.status().is_terminal() => {
                self.send_terminal(handle, &record)
            }
            Ok(Some(record)) => {
                // 已受理但尚未终结：core 的失败终态没落盘（存储瞬断）。**不伪造终态**——回受理态并挂终态
                // 观察；记录由启动恢复按 §6 第 16 条收敛到 `uncertain`。
                self.accept_and_watch_create(
                    handle,
                    &submit.request_id,
                    &request,
                    record.session(),
                    record.accepted_at(),
                );
            }
            Ok(None) => {
                // 受理提交成功却查无记录：存储与用例不一致（接线缺陷）。不宣告任何结论，回受理态并挂
                // 观察，对端据此等待或凭 `command.status` 重查。
                warn!(
                    event = "node_link.session_create_record_missing",
                    access_node_id = handle.node_id().as_str(),
                    command = CommandName::SessionCreate.as_str(),
                    request_id = submit.request_id.as_str(),
                    cause = Cause::NotFound.as_str(),
                    "the accepted session.create has no persisted record"
                );
                self.accept_and_watch_create(handle, &submit.request_id, &request, None, None);
            }
            Err(read_error) => {
                // 记录读不回来（存储不可用）：此刻既不能宣告「没有副作用」，也不能凭空造终态。
                warn!(
                    event = "node_link.session_create_record_unreadable",
                    access_node_id = handle.node_id().as_str(),
                    command = CommandName::SessionCreate.as_str(),
                    request_id = submit.request_id.as_str(),
                    cause = Cause::of_port_error(&read_error).as_str(),
                    "the session.create record could not be read"
                );
                self.accept_and_watch_create(handle, &submit.request_id, &request, None, None);
            }
        }
        RouteOutcome::Claimed
    }

    /// `session.create` 失败后的回包：**以持久记录为唯一权威**（与 Sync 面 `reply_after_create_failure`
    /// 同一判据，只是本面的「没有记录」那一档有自己的本地合成形状）。
    ///
    /// core 已按错误类别写好终态（可证明 Agent 侧未产生会话 ⇒ `failed`，说不清 ⇒ `uncertain`，零会话行），
    /// 因此：
    ///
    /// - **有记录且已终结** ⇒ `command.terminal`（`terminalEventId` 非空），同键重发因此回首次结果、不会
    ///   被回成「没有副作用」；
    /// - **有记录但仍是 `accepted`**（core 的失败终态没落盘）⇒ 回受理态并挂终态观察，不伪造终态；
    /// - **没有记录** ⇒ 失败发生在受理提交之前（授权拒绝、参数/别名未登记、本机解析失败，或受理提交本身
    ///   写失败）：这一档 core 还没调用后端，**可证明**本机没有副作用，因此本地合成 `failed` 终态是如实的
    ///   （§12.7 的「幂等行落盘前失败」：同 `requestId` 重查回 `nodelink.command.not_found`）；
    /// - **记录读不回来**（存储不可用）⇒ 不宣告任何结论：回受理态并挂观察，对端可凭 `command.status` 重查。
    async fn reply_after_create_failure(
        &self,
        handle: &ConnectionHandle,
        submit_request: &Uuid,
        request: &CoreRequestId,
        error: &PortError,
    ) -> RouteOutcome {
        match self
            .core
            .command_status(&node_actor(handle.node_id()), request.clone())
            .await
        {
            Ok(Some(record)) if record.status().is_terminal() => {
                self.send_terminal(handle, &record);
            }
            Ok(Some(record)) => {
                self.accept_and_watch_create(
                    handle,
                    submit_request,
                    request,
                    record.session(),
                    record.accepted_at(),
                );
            }
            Ok(None) => self.send_local_terminal(
                handle,
                CommandName::SessionCreate,
                submit_request,
                None,
                &LocalOutcome::Failed(error_info(&port_error_code(error))),
                Cause::of_port_error(error),
            ),
            Err(read_error) => {
                warn!(
                    event = "node_link.session_create_record_unreadable",
                    access_node_id = handle.node_id().as_str(),
                    command = CommandName::SessionCreate.as_str(),
                    request_id = submit_request.as_str(),
                    cause = Cause::of_port_error(&read_error).as_str(),
                    "the session.create record could not be read after a backend failure"
                );
                self.accept_and_watch_create(handle, submit_request, request, None, None);
            }
        }
        RouteOutcome::Claimed
    }

    /// 创建成功后组装 `SessionCreateResult`：`sessionId` 永远来自 Owner（core 的会话身份），
    /// `sessionMeta` 取自 core 随 [`PreparedSessionCreate`] 给出的会话摘要。
    ///
    /// 这里**不回读存储**：会话行要到 `settle_session_create` 的落账提交才存在（§6 第 20 条），此前任何
    /// 读面（含 `node_link_session_view` 的可见性前置）都必然报「会话不存在」；而 core 的摘要与落账提交
    /// 将写入的取值同源（同一套投影口径），因此结果与之后的读路径逐字段一致。
    /// `node_link_session_view` 的可见性前置（对端是已配对的 access 行、Export 未撤销且覆盖
    /// `grant.remote-work`、会话的 Agent 属于该 Export）在本处理器内由 `session_create_parameter` 在
    /// **同一个请求**上先行判定；会话又是本请求刚建的，因此不存在「借用任意 `(exportId, sessionId)`
    /// 组合读取」这条路径。
    fn session_create_result(
        &self,
        prepared: &PreparedSessionCreate,
    ) -> Result<SessionCreateResult, PortError> {
        let Some(session_meta) = crate::node_link::resource::session_meta(&prepared.summary) else {
            return Err(PortError::Corrupt("session meta"));
        };
        Ok(SessionCreateResult {
            remote_session_ref: self.remote_session_ref(&prepared.session)?,
            session_meta,
        })
    }

    /// `remoteSessionRef` 的唯一投影：三个分量都来自本层与 core 的会话身份（不读存储）。
    fn remote_session_ref(&self, session: &SessionId) -> Result<RemoteSessionRef, PortError> {
        let Some(owner_node_id) = uuid_of(self.node_id.as_str()) else {
            return Err(PortError::Corrupt("the owner node id is not a uuid"));
        };
        let Some(session_id) = uuid_of(session.as_str()) else {
            return Err(PortError::Corrupt("the session id is not a uuid"));
        };
        Ok(RemoteSessionRef {
            owner_node_id,
            session_id,
        })
    }

    /// 恢复成功后组装 `SessionResumeResult`：与 [`Self::session_create_result`] **同源**的映射
    /// 因此两条路径的回包形状必然一致；`remoteSessionRef.exportId` 只有适配层有，
    /// core 的 `resume_session` 只返回 `SessionId`。
    async fn session_resume_result(
        &self,
        access_node: &NodeId,
        session: &SessionId,
    ) -> Result<SessionResumeResult, PortError> {
        let (remote_session_ref, session_meta) = self
            .remote_session_ref_and_meta(access_node, session)
            .await?;
        Ok(SessionResumeResult {
            remote_session_ref,
            session_meta,
        })
    }

    /// `remoteSessionRef` + `sessionMeta` 的**唯一**读面投影：`sessionMeta` 取自该会话的持久化摘要
    /// （读面按**该 Access 节点**的可见性复核）。只服务**已落账**的会话（`session.resume`）。
    async fn remote_session_ref_and_meta(
        &self,
        access_node: &NodeId,
        session: &SessionId,
    ) -> Result<(RemoteSessionRef, SessionMeta), PortError> {
        let view = self
            .core
            .node_link_session_view(access_node, session)
            .await?;
        let Some(session_meta) = crate::node_link::resource::session_meta(&view.session) else {
            return Err(PortError::Corrupt("session meta"));
        };
        Ok((self.remote_session_ref(session)?, session_meta))
    }

    // -----------------------------------------------------------------------------------------
    // session.resume
    // -----------------------------------------------------------------------------------------

    /// `session.resume`（§12.7、§5.1）：恢复**进程不在**的 owned 会话。
    ///
    /// 与 `session.create` 同形的管线：授权 → 幂等 → `accepted(result = null)` → 工作 → 终态落盘，
    /// 只是输入全部取自 Owner 自身的持久化记录（客户端不得提供），因此没有参数校验分支。
    async fn on_session_resume(
        &self,
        handle: &ConnectionHandle,
        message: &Envelope,
        submit: &CommandSubmit,
    ) -> RouteOutcome {
        if !matches!(&submit.payload, WirePayload::SessionResume(_)) {
            return self.reject_schema(handle, message, "the session.resume payload is invalid");
        }
        // ① 定位会话（只解析 `sessionRef`，不做任何本机读取）。
        let target = match self.session_target(handle, message, submit) {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        // ③ attachment 代际复核（§12.5）：命令必须携带当前 attachment。
        if let Err(outcome) = self.attachment_is_current(handle, message, submit, &target) {
            return outcome;
        }
        // ④ 幂等：幂等键 `(ownerNodeId, accessNodeId, requestId)`，持久事实在 core 的 `owned_command`
        //    里（core 的 `resume_session` 自建 `accepted` 行）。同键不同语义回
        //    `nodelink.command.idempotency_conflict`，**不**触发第二次 spawn。
        //    本命令的 `payload` 恒为 `{}`，语义全部落在 `sessionRef` 上，因此指纹之外**必须**再比
        //    会话身份（§12.5「`sessionRef` 语义不同即冲突」）：core 的 `resume_session` 落幂等行时
        //    已把目标会话写进 `session`（`broker.rs` 的 `IdempotencyRecord.session`），只比指纹会把
        //    「同 requestId 改指另一个同样已授权的会话」误判成同语义，从而回上一个会话的终态结果。
        let Some(fingerprint) = fingerprint_of(&submit.payload) else {
            return self.reject_schema(handle, message, "the payload cannot be fingerprinted");
        };
        let actor = node_actor(handle.node_id());
        let Some(request) = core_request(&submit.request_id) else {
            return self.reject_schema(handle, message, "the requestId is not a uuid");
        };
        match self.core.command_status(&actor, request.clone()).await {
            Ok(Some(record)) => {
                let same = record.command() == CommandName::SessionResume.as_str()
                    && record.kind() == CommandKind::Mutation
                    && record.request_fingerprint() == &fingerprint
                    && record.session() == Some(&target.session);
                if !same {
                    return self.reject_code(
                        handle,
                        CommandName::SessionResume,
                        &submit.request_id,
                        "nodelink.command.idempotency_conflict",
                    );
                }
                if record.status().is_terminal() {
                    self.send_terminal(handle, &record);
                } else {
                    let accepted_at = record
                        .accepted_at()
                        .cloned()
                        .unwrap_or_else(|| self.clock());
                    self.send_accepted(
                        handle,
                        &submit.request_id,
                        CommandName::SessionResume,
                        &accepted_at,
                        None,
                        record.session(),
                    );
                    self.watch(handle, &request);
                }
                return RouteOutcome::Claimed;
            }
            Ok(None) => {}
            Err(error) => return self.port_fault(handle, message, &error),
        }
        if !self.admit_in_flight(handle).await {
            return RouteOutcome::Claimed;
        }
        // ⑤ 先回 `accepted(result = null)`，恢复完成后发 `terminal`（两者连续入队）。
        self.send_accepted(
            handle,
            &submit.request_id,
            CommandName::SessionResume,
            &self.clock(),
            None,
            Some(&target.session),
        );
        // ⑥ 恢复（core 用例内部：授权 → 窄读取 → cwd 复校验 → `factory.resume`；**授权先于本机读取**）。
        let outcome = match self
            .core
            .resume_session(&actor, request.clone(), fingerprint, target.session.clone())
            .await
        {
            Ok(session) => match self
                .session_resume_result(handle.node_id(), &session)
                .await
            {
                Ok(result) => LocalOutcome::Completed(CommandResultPayload::SessionResume(result)),
                Err(error) => {
                    // 会话确实已恢复：不能用 `failed` 撒谎（那会让 Access 以为没有恢复，而 Owner 侧
                    // 有一个已可交互却拿不到 `remoteSessionRef` 的会话），与 `session.create` 投影失败
                    // 分支同判据。
                    warn!(
                        event = "node_link.session_resume_result_failed",
                        access_node_id = handle.node_id().as_str(),
                        request_id = submit.request_id.as_str(),
                        error = ?error,
                        "the resumed session cannot be projected into SessionResumeResult"
                    );
                    LocalOutcome::Uncertain(error_info("nodelink.command.uncertain"))
                }
            },
            Err(PortError::Conflict(acp_core::model::ConflictKind::IdempotencyConflict)) => {
                // 同键不同语义（§12.5）：回 `command.rejected` 且零副作用。
                return self.reject_code(
                    handle,
                    CommandName::SessionResume,
                    &submit.request_id,
                    "nodelink.command.idempotency_conflict",
                );
            }
            Err(error) => {
                // 恢复失败即**确定类**失败：能力未宣告、该会话无持久化恢复数据（两列 NULL）、
                // cwd 复校验失败、越权 —— 这几类 core 都已给出明确结论（`BackendUnsupported` 走
                // `nodelink.command.unsupported`，复校验失败走服务端不可用类），一律结 `failed`。
                // `uncertain` 只属于崩溃窗口（accepted 已落盘但本次没能结算），由启动恢复按
                // `CORE_PORTS_AND_STORAGE.md` §6 第 16 条终结，不在本层制造。
                warn!(
                    event = "node_link.session_resume_failed",
                    access_node_id = handle.node_id().as_str(),
                    request_id = submit.request_id.as_str(),
                    error = ?error,
                    "session.resume could not be completed"
                );
                LocalOutcome::Failed(error_info(&port_error_code(&error)))
            }
        };
        // ⑦ 终态落盘：core 的 `settle_session_resume` 只终结 `session.resume` 的持久记录（§5.1）。
        let settled = match &outcome {
            LocalOutcome::Completed(result) => match serde_json::to_string(result)
                .ok()
                .and_then(|text| acp_core::model::CommandResult::from_json_text(&text).ok())
            {
                Some(result) => self
                    .core
                    .settle_session_resume(
                        &actor,
                        &request,
                        CoreStatus::Completed,
                        Some(result),
                        None,
                    )
                    .await
                    .map(|_| true),
                None => {
                    warn!(
                        event = "node_link.session_resume_result_unencodable",
                        request_id = submit.request_id.as_str(),
                        "the resumed session result cannot be persisted"
                    );
                    Ok(false)
                }
            },
            LocalOutcome::Failed(error) => self
                .core
                .settle_session_resume(
                    &actor,
                    &request,
                    CoreStatus::Failed,
                    None,
                    core_error(error),
                )
                .await
                .map(|_| true),
            LocalOutcome::Uncertain(error) => self
                .core
                .settle_session_resume(
                    &actor,
                    &request,
                    CoreStatus::Uncertain,
                    None,
                    core_error(error),
                )
                .await
                .map(|_| true),
        };
        if let Err(error) = settled {
            warn!(
                event = "node_link.session_resume_terminal_failed",
                access_node_id = handle.node_id().as_str(),
                request_id = submit.request_id.as_str(),
                error = ?error,
                "the session.resume terminal could not be persisted"
            );
        }
        // 回包以**持久记录**为唯一权威（重试与 `command.status` 重查因此与首次同形）。
        match self.core.command_status(&actor, request.clone()).await {
            Ok(Some(record)) if record.status().is_terminal() => {
                self.send_terminal(handle, &record)
            }
            _ => {
                // 没有持久记录：恢复在幂等行落盘前就失败（授权、窄读取、cwd 复校验）。本层能确认的
                // 只有**确定类失败**，因此本地回 `failed` 终态；存储写失败落在同一窗口却不属这一类，
                // 它没有留下持久首次结果，同 requestId 重查回 `nodelink.command.not_found`。
                self.send_local_terminal(
                    handle,
                    CommandName::SessionResume,
                    &submit.request_id,
                    Some(&target.session),
                    &outcome,
                    local_outcome_cause(&outcome),
                );
            }
        }
        RouteOutcome::Claimed
    }

    // -----------------------------------------------------------------------------------------
    // 授权与边界判定
    // -----------------------------------------------------------------------------------------

    /// session-scoped 命令的会话定位：仅解析 `sessionRef`（不做 attachment 查找）。
    ///
    /// 解析失败一律 `export.not_found`：引用指向别的 Owner、或不是合法的复合引用时，该会话对本节点
    /// 不可用——不区分「不存在」与「不属于本机」，避免把 ref 变成存在性预言机。
    fn session_target(
        &self,
        handle: &ConnectionHandle,
        message: &Envelope,
        submit: &CommandSubmit,
    ) -> Result<SessionTarget, RouteOutcome> {
        let Some(remote) = submit.session_ref.as_ref() else {
            // wire 的 null/非 null 规则由 `CommandSubmit::validate` 保证；不可达。
            return Err(self.reject_schema(
                handle,
                message,
                "the session-scoped command does not carry a sessionRef",
            ));
        };
        if remote.owner_node_id.as_str() != self.node_id.as_str() {
            return Err(self.send_and_claim(
                handle,
                ErrorCode::ProtocolSequenceInvalid,
                "the sessionRef does not point at this owner node",
                Some(message.message_id()),
            ));
        }
        let Some(session) = core_session(remote.session_id.as_str()) else {
            return Err(self.send_and_claim(
                handle,
                ErrorCode::ProtocolSequenceInvalid,
                "the sessionRef is not a valid compound reference",
                Some(message.message_id()),
            ));
        };
        Ok(SessionTarget {
            session,
            remote: remote.clone(),
        })
    }

    /// 代际复核（§12.5）：命令必须携带当前 attachment；过期回 `attach_generation_stale`。
    fn attachment_is_current(
        &self,
        handle: &ConnectionHandle,
        message: &Envelope,
        submit: &CommandSubmit,
        target: &SessionTarget,
    ) -> Result<(), RouteOutcome> {
        let attachment = submit.attachment_id.as_ref();
        let generation = submit
            .attachment_generation
            .as_ref()
            .and_then(|generation| generation.as_str().parse::<u64>().ok());
        let stale = || {
            self.send_and_claim(
                handle,
                ErrorCode::ResourceAttachGenerationStale,
                "the attachment is not the current one; attach again before retrying",
                Some(message.message_id()),
            )
        };
        let (Some(attachment), Some(generation)) = (attachment, generation) else {
            return Err(stale());
        };
        match self
            .resource
            .current_attachment_session_ref(handle, attachment, generation)
        {
            Some(current) if current == target.remote => Ok(()),
            _ => Err(stale()),
        }
    }

    // -----------------------------------------------------------------------------------------
    // 响应装配
    // -----------------------------------------------------------------------------------------

    /// 越权留痕（R69/R82/R83）：适配层的交集比 core 的 Owner 侧判定更严，被它拒的命令不会到达 broker，
    /// 因此这条 `authorization.denied` 只能在这里写；审计写失败不改判定（也不改响应）。
    async fn audit_denied(&self, node: &NodeId) {
        if let Err(error) = self
            .core
            .record_node_link_auth(node, AuditAction::AuthorizationDenied, AuditOutcome::Denied)
            .await
        {
            warn!(
                event = "node_link.denial_audit_failed",
                access_node_id = node.as_str(),
                error = ?error,
                "the denied command could not be audited"
            );
        }
    }

    /// schema 级拒绝（连接保持可用）。
    fn reject_schema(
        &self,
        handle: &ConnectionHandle,
        message: &Envelope,
        text: &'static str,
    ) -> RouteOutcome {
        self.send_and_claim(
            handle,
            ErrorCode::ProtocolSchemaInvalid,
            text,
            Some(message.message_id()),
        )
    }

    /// 字段类拒绝（`nodelink.command.unsupported_field` + `details.field`，§12.7）。
    fn reject_field(
        &self,
        handle: &ConnectionHandle,
        command: CommandName,
        request: &Uuid,
        field: &str,
    ) -> RouteOutcome {
        self.send_rejected(
            handle,
            command,
            request,
            &error_info("nodelink.command.unsupported_field"),
            field_details(field),
        )
    }

    /// 命令级拒绝（按 `nodelink.*` 代码，`details` 为空）。
    fn reject_code(
        &self,
        handle: &ConnectionHandle,
        command: CommandName,
        request: &Uuid,
        code: &str,
    ) -> RouteOutcome {
        self.send_rejected(
            handle,
            command,
            request,
            &error_info(code),
            RawObject::empty(),
        )
    }

    /// 端口错误 → 协议响应：越权回 `export.not_granted`，未知会话/Export 回 `export.not_found`，
    /// 其余按 wire 错误码表映射（映射不到的只进结构化日志，不冒充 wire 码）。
    fn port_fault(
        &self,
        handle: &ConnectionHandle,
        message: &Envelope,
        error: &PortError,
    ) -> RouteOutcome {
        let correlation = Some(message.message_id());
        let text = match error {
            PortError::InvalidRequest("authorization.scope_denied" | "export.not_granted") => {
                return self.send_and_claim(
                    handle,
                    ErrorCode::ProtocolSequenceInvalid,
                    "the command is not granted to this node",
                    correlation,
                );
            }
            PortError::NotFound(EntityRef::Export(_) | EntityRef::Session(_)) => {
                return self.send_and_claim(
                    handle,
                    ErrorCode::ProtocolSequenceInvalid,
                    "the referenced session is not available to this node",
                    correlation,
                );
            }
            PortError::InvalidRequest(reason) if reason.starts_with("nodelink.") => {
                return self.send_and_claim(
                    handle,
                    error_code_of(reason).unwrap_or(ErrorCode::InternalUnavailable),
                    "the command was rejected by the owner",
                    correlation,
                );
            }
            other => {
                warn!(
                    event = "node_link.command_failed",
                    access_node_id = handle.node_id().as_str(),
                    cause = Cause::of_port_error(other).as_str(),
                    "the command could not be served"
                );
                "the owner node cannot serve the command right now"
            }
        };
        self.send_and_claim(handle, ErrorCode::InternalUnavailable, text, correlation)
    }

    /// 发送 `link.error` 并认领该消息。
    fn send_and_claim(
        &self,
        handle: &ConnectionHandle,
        code: ErrorCode,
        text: &'static str,
        correlation: Option<&Uuid>,
    ) -> RouteOutcome {
        self.send_error(handle, code, text, RawObject::empty(), correlation);
        RouteOutcome::Claimed
    }

    fn send_error(
        &self,
        handle: &ConnectionHandle,
        code: ErrorCode,
        text: &'static str,
        details: RawObject,
        correlation: Option<&Uuid>,
    ) {
        let Some(mut body) = link_error(code, text, correlation) else {
            warn!(
                event = "node_link.error_body_failed",
                code = code.as_str(),
                "the link.error body cannot be assembled"
            );
            return;
        };
        body.details = details;
        let _ = handle.send(MessageType::LinkError, &body);
    }

    /// `command.accepted`：`result` 取 core 收据里唯一有意义的字段（turn，§12.5 的 `object | null`）。
    ///
    /// `session.create` 的 accepted 必须为 `null`（schema 的 `if/then`：结果由 `command.terminal` 给出）；
    /// 没有 turn 的 mutation（取消/模式/配置/交互解析）也回 `null`，不编造字段。`command.status` 重查
    /// 未终结 mutation 时同样回 `null`——turn 归属不在持久命令行里（它在 `owned_turn.causation`），
    /// 重查的单点读入口只有命令行本身；首次提交与同键重试都带 `turnId`。
    ///
    /// `session` 只服务命令级日志（资源标识；创建类命令受理阶段还没有会话，传 `None`），不参与 wire 形状。
    fn send_accepted(
        &self,
        handle: &ConnectionHandle,
        request: &Uuid,
        command: CommandName,
        accepted_at: &CoreTimestamp,
        turn: Option<&TurnId>,
        session: Option<&SessionId>,
    ) {
        command_log::accepted(
            Subject::Node(handle.node_id().as_str()),
            command.as_str(),
            session,
            request.as_str(),
            turn,
        );
        let Some(accepted_at) = wire_timestamp(accepted_at) else {
            warn!(
                event = "node_link.command_accepted_at_invalid",
                request_id = request.as_str(),
                "the acceptance timestamp is not a canonical wire timestamp"
            );
            return;
        };
        let result = match (command, turn) {
            // `session.create` 与 `session.resume` 的 accepted 必须为 `null`（schema 的 `if/then` 与
            // §12.7）：两者都是异步的，结果由 `command.terminal` 的具名结果变体给出。
            (CommandName::SessionCreate | CommandName::SessionResume, _) => Nullable::null(),
            (_, Some(turn)) => match uuid_of(turn.as_str()) {
                Some(turn) => Nullable::from_option(Some(CommandResultPayload::Object(
                    details_object(&serde_json::json!({ "turnId": turn.as_str() })),
                ))),
                None => Nullable::null(),
            },
            (_, None) => Nullable::null(),
        };
        let body = CommandAccepted {
            request_id: request.clone(),
            command,
            accepted_at,
            result,
        };
        if handle.send(MessageType::CommandAccepted, &body).is_err() {
            debug!(
                event = "node_link.command_accepted_not_queued",
                connection_id = handle.connection_id().as_str(),
                "the command.accepted message could not be queued"
            );
        }
    }

    /// 终态推送（`command.terminal`）。
    ///
    /// 这是把**持久终态**交付给对端的唯一落点（终态观察者、`command.status` 重查、幂等重放、落账后的
    /// 首次回包都经它），命令级终态日志因此也在这里：类别与成因都取自持久记录本身。
    fn send_terminal(&self, handle: &ConnectionHandle, record: &CommandRecord) {
        if let Some(class) = record_terminal_class(record.status()) {
            command_log::terminal(
                Subject::Node(handle.node_id().as_str()),
                record.command(),
                record.session(),
                record.request().as_str(),
                class,
                record
                    .error()
                    .map_or(Cause::None, |error| Cause::of_code(error.code())),
            );
        }
        match terminal_body(record, &self.clock()) {
            Some(body) => {
                if handle.send(MessageType::CommandTerminal, &body).is_err() {
                    debug!(
                        event = "node_link.command_terminal_not_queued",
                        connection_id = handle.connection_id().as_str(),
                        "the command.terminal message could not be queued"
                    );
                }
            }
            None => warn!(
                event = "node_link.command_terminal_mapping_failed",
                request_id = record.request().as_str(),
                "the persisted command record cannot be mapped to command.terminal"
            ),
        }
    }

    /// 建会话的「回受理态并挂终态观察」：记录仍是 `accepted`（core 的终态没落盘）与记录读不回来两档
    /// 共用——两者都**不伪造终态**，记录才是唯一事实（有记录时由启动恢复收敛到 `uncertain`）。
    fn accept_and_watch_create(
        &self,
        handle: &ConnectionHandle,
        submit_request: &Uuid,
        request: &CoreRequestId,
        session: Option<&SessionId>,
        accepted_at: Option<&CoreTimestamp>,
    ) {
        let at = accepted_at.cloned().unwrap_or_else(|| self.clock());
        self.send_accepted(
            handle,
            submit_request,
            CommandName::SessionCreate,
            &at,
            None,
            session,
        );
        self.watch(handle, request);
    }

    /// **本地合成**终态的发送：没有持久记录可读时用它（`session.create`/`session.resume` 在幂等行落盘
    /// 之前就失败的那一档）。
    ///
    /// 与 [`Self::send_terminal`] 的差别是记录不存在：`terminalEventId` 因此为 `null`——这是这一档的既有
    /// 口径（§12.7 的「幂等行落盘前失败」：同 `requestId` 重查回 `nodelink.command.not_found`，重试可以
    /// 得到不同结果），日志照记（对端确实收到了一条终态）。
    fn send_local_terminal(
        &self,
        handle: &ConnectionHandle,
        command: CommandName,
        request: &Uuid,
        session: Option<&SessionId>,
        outcome: &LocalOutcome,
        cause: Cause,
    ) {
        let Some(body) = local_terminal(command, request, outcome, &self.clock()) else {
            warn!(
                event = "node_link.command_terminal_not_assembled",
                command = command.as_str(),
                request_id = request.as_str(),
                "the locally synthesized terminal could not be assembled"
            );
            return;
        };
        command_log::terminal(
            Subject::Node(handle.node_id().as_str()),
            command.as_str(),
            session,
            request.as_str(),
            local_terminal_class(outcome),
            cause,
        );
        if handle.send(MessageType::CommandTerminal, &body).is_err() {
            debug!(
                event = "node_link.command_terminal_not_queued",
                connection_id = handle.connection_id().as_str(),
                "the locally synthesized terminal could not be queued"
            );
        }
    }

    /// 命令级拒绝（`command.rejected`）：未接受、无副作用。
    fn send_rejected(
        &self,
        handle: &ConnectionHandle,
        command: CommandName,
        request: &Uuid,
        error: &PublicError,
        details: RawObject,
    ) -> RouteOutcome {
        let mut error = error.clone();
        error.details = details;
        let body = CommandRejected {
            request_id: request.clone(),
            command,
            error,
        };
        let _ = handle.send(MessageType::CommandRejected, &body);
        debug!(
            event = "node_link.command_rejected",
            access_node_id = handle.node_id().as_str(),
            request_id = request.as_str(),
            code = body.error.code.as_str(),
            "the command was rejected without a side effect"
        );
        RouteOutcome::Claimed
    }

    // -----------------------------------------------------------------------------------------
    // 限流与 in-flight
    // -----------------------------------------------------------------------------------------

    /// 命令限流（§2.5：单连接 120/分钟 + 连续超限 4429）。返回 `false` 表示本次命令已被拒绝。
    fn admit_rate(&self, handle: &ConnectionHandle) -> bool {
        let mut states = lock(&self.states);
        let state = states
            .entry(handle.connection_id().as_str().to_owned())
            .or_insert_with(Self::new_state);
        match state.limiter.check(handle.client_ip()) {
            RateLimit::Allowed { .. } => {
                state.consecutive_rate_limited = 0;
                true
            }
            RateLimit::Denied { retry_after } => {
                state.consecutive_rate_limited += 1;
                let close_now =
                    state.consecutive_rate_limited >= MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS;
                drop(states);
                warn!(
                    event = "node_link.command_rate_limited",
                    access_node_id = handle.node_id().as_str(),
                    retry_after_ms = retry_after.as_millis() as u64,
                    close = close_now,
                    "the access node exceeded the per-connection command rate"
                );
                self.send_error(
                    handle,
                    ErrorCode::ResourceRateLimited,
                    "the command rate limit was exceeded",
                    rate_limit_details(retry_after),
                    None,
                );
                if close_now {
                    handle.request_close(close::RATE_LIMITED, "command rate limit exceeded");
                }
                false
            }
        }
    }

    /// 单连接 in-flight 上限（§2.5：默认 32，只算已接受但未终结的 mutation）。
    ///
    /// 先摘掉已经终结的观察项（每条最多一次 `command_status` 读），再看是否已满；因此该表恒不超过上限。
    async fn admit_in_flight(&self, handle: &ConnectionHandle) -> bool {
        let limit = handle.limits().max_in_flight_commands() as usize;
        let pending = self.pending_of(handle);
        if pending.len() < limit {
            return true;
        }
        let actor = node_actor(handle.node_id());
        for entry in &pending {
            match self
                .core
                .command_status(&actor, entry.request.clone())
                .await
            {
                Ok(Some(record)) if record.status().is_terminal() => {
                    self.forget_pending(handle.connection_id().as_str(), &entry.request);
                }
                Ok(_) => {}
                Err(error) => {
                    // 读不回来时保持占用（失败关闭）：宁可拒绝新命令，也不把在途数算错。
                    warn!(
                        event = "node_link.command_status_failed",
                        access_node_id = handle.node_id().as_str(),
                        request_id = entry.request.as_str(),
                        error = ?error,
                        "the in-flight bookkeeping could not be refreshed"
                    );
                }
            }
        }
        if self.pending_of(handle).len() < limit {
            return true;
        }
        warn!(
            event = "node_link.command_in_flight_limit",
            access_node_id = handle.node_id().as_str(),
            limit,
            "the connection reached the negotiated in-flight command limit"
        );
        self.send_error(
            handle,
            ErrorCode::ResourceRateLimited,
            "the in-flight command limit was reached",
            rate_limit_details(IN_FLIGHT_RETRY_AFTER),
            None,
        );
        false
    }

    fn pending_of(&self, handle: &ConnectionHandle) -> Vec<PendingCommand> {
        lock(&self.states)
            .get(handle.connection_id().as_str())
            .map(|state| state.pending.clone())
            .unwrap_or_default()
    }

    fn new_state() -> ConnectionState {
        ConnectionState {
            // 每条连接一个窗口；限流器的键是地址类型，本连接只用它自己的对端地址（对端地址只用于
            // `check`，构造器不需要它）。
            limiter: SlidingWindowLimiter::new(COMMANDS_PER_MINUTE, COMMAND_WINDOW),
            consecutive_rate_limited: 0,
            pending: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------------------------
    // 终态观察（命令终态推送）
    // -----------------------------------------------------------------------------------------

    /// 轮询所有待观察命令的持久化记录，把已经终结的那些推成 `command.terminal`。
    async fn poll_pending(&self) {
        // 先回收离开注册表的连接：整条状态（限流窗口 + 观察表）随连接消失，**不**依赖它是否还有
        // pending——没有 in-flight 命令的连接否则会永远留下一条空状态。
        self.forget_gone_connections();
        let mut done: Vec<(String, CoreRequestId)> = Vec::new();
        for (connection_id, node, pending) in self.pending_snapshot() {
            let Some(handle) = self.handle_of(&connection_id) else {
                // 连接已结束：整条状态（含观察表）随连接消失。
                self.forget_connection(&connection_id);
                continue;
            };
            if pending.since.elapsed() >= WATCH_MAX_AGE {
                // 有界观察：超龄即放弃，避免一条永不终结的记录把表撑住；对端仍可凭 `command.status` 重查。
                done.push((connection_id.clone(), pending.request.clone()));
                warn!(
                    event = "node_link.command_terminal_abandoned",
                    connection_id = connection_id.as_str(),
                    request_id = pending.request.as_str(),
                    "the command terminal was not observed within the watch budget"
                );
                continue;
            }
            match self
                .core
                .command_status(&node_actor(&node), pending.request.clone())
                .await
            {
                Ok(Some(record)) if record.status().is_terminal() => {
                    done.push((connection_id.clone(), pending.request.clone()));
                    self.send_terminal(&handle, &record);
                }
                Ok(_) => {}
                Err(error) => {
                    warn!(
                        event = "node_link.command_status_failed",
                        access_node_id = node.as_str(),
                        request_id = pending.request.as_str(),
                        error = ?error,
                        "the command terminal could not be read; the watcher retries on the next tick"
                    );
                }
            }
        }
        for (connection_id, request) in done {
            self.forget_pending(&connection_id, &request);
        }
    }

    /// 观察表的快照（含所属节点），避免在持锁期间 `await`。
    fn pending_snapshot(&self) -> Vec<(String, NodeId, PendingCommand)> {
        let handles = self.registry.handles();
        let states = lock(&self.states);
        let mut snapshot = Vec::new();
        for (connection_id, state) in states.iter() {
            let Some(handle) = handles
                .iter()
                .find(|handle| handle.connection_id().as_str() == connection_id)
            else {
                continue;
            };
            for pending in &state.pending {
                snapshot.push((
                    connection_id.clone(),
                    handle.node_id().clone(),
                    pending.clone(),
                ));
            }
        }
        snapshot
    }

    fn handle_of(&self, connection_id: &str) -> Option<Arc<ConnectionHandle>> {
        self.registry
            .handles()
            .into_iter()
            .find(|handle| handle.connection_id().as_str() == connection_id)
    }

    /// 摘掉一条待观察项。
    fn forget_pending(&self, connection_id: &str, request: &CoreRequestId) {
        let mut states = lock(&self.states);
        let Some(state) = states.get_mut(connection_id) else {
            return;
        };
        state
            .pending
            .retain(|pending| pending.request.as_str() != request.as_str());
    }

    fn forget_connection(&self, connection_id: &str) {
        lock(&self.states).remove(connection_id);
    }

    /// 回收「不在存活连接集合里」的全部连接状态（限流窗口与观察表一起消失）。
    ///
    /// 判据是**注册表**的存活集合，而不是「还有没有 pending」：一条只被限流窗口引用的状态同样是垃圾。
    fn forget_gone_connections(&self) {
        let live: Vec<String> = self
            .registry
            .handles()
            .iter()
            .map(|handle| handle.connection_id().as_str().to_owned())
            .collect();
        lock(&self.states).retain(|connection_id, _| live.contains(connection_id));
    }

    /// 登记一条终态观察。
    fn watch(&self, handle: &ConnectionHandle, request_id: &CoreRequestId) {
        {
            let mut states = lock(&self.states);
            let state = states
                .entry(handle.connection_id().as_str().to_owned())
                .or_insert_with(Self::new_state);
            state.pending.push(PendingCommand {
                request: request_id.clone(),
                since: Instant::now(),
            });
        }
        self.wake.notify_one();
    }

    // -----------------------------------------------------------------------------------------
    // 授权变化的连接传播（D7：撤销；重新配对）
    // -----------------------------------------------------------------------------------------

    /// `node.revoke` 提交后：推送 `node.trust.revoked` 并以 4410 关闭该节点的全部连接（返回连接数）。
    ///
    /// `revokedAt` 取该 `access` 信任行的持久化撤销时间（读不到就不推消息，只关闭）；关闭是**请求**，
    /// 会话会先排空已入队消息（撤销推送因此先送达）再发 close 帧。
    pub async fn node_revoked(&self, node: &NodeId) -> usize {
        let handles = self.registry.handles_for_node(node);
        if handles.is_empty() {
            return 0;
        }
        let revoked_at = match self.core.node_link_handshake_view(node).await {
            Ok(view) => view.node.as_ref().and_then(|row| row.revoked_at().cloned()),
            Err(error) => {
                warn!(
                    event = "node_link.revocation_view_failed",
                    access_node_id = node.as_str(),
                    error = ?error,
                    "the revoked node trust row could not be read"
                );
                None
            }
        };
        let revoked_node_id = uuid_of(node.as_str());
        for handle in &handles {
            match (revoked_at.as_ref(), revoked_node_id.clone()) {
                (Some(at), Some(revoked_node_id)) => match wire_timestamp(at) {
                    Some(revoked_at) => {
                        let body = NodeTrustRevoked {
                            revoked_node_id,
                            revoked_at,
                            reason: node_revocation_reason(),
                        };
                        if handle.send(MessageType::NodeTrustRevoked, &body).is_err() {
                            debug!(
                                event = "node_link.revocation_not_queued",
                                connection_id = handle.connection_id().as_str(),
                                "the node.trust.revoked message could not be queued"
                            );
                        }
                    }
                    None => warn!(
                        event = "node_link.revocation_timestamp_invalid",
                        access_node_id = node.as_str(),
                        "the persisted revocation timestamp is not a canonical wire timestamp"
                    ),
                },
                _ => debug!(
                    event = "node_link.revocation_notification_skipped",
                    access_node_id = node.as_str(),
                    "no persisted revokedAt is available; the connection is closed without the notification"
                ),
            }
            handle.request_close(close::REVOKED, "the node trust was revoked");
        }
        for handle in &handles {
            self.forget_connection(handle.connection_id().as_str());
        }
        info!(
            event = "node_link.node_revocation_propagated",
            access_node_id = node.as_str(),
            connections = handles.len(),
            "the revoked node trust was propagated to its active connections"
        );
        handles.len()
    }

    /// `node.pair.confirm` 提交后：以**正常关闭**（1000）作废该节点的全部连接（返回连接数）。
    ///
    /// 重新配对不是撤销：这里**不**推 `node.trust.revoked`，也不以 4410（「节点已撤销」）关闭
    /// （`NODE_LINK_PROTOCOL.md` §15 规定这两者都表示「停止重连」），而是让对端按已提交的新清单重连
    /// 并重取 catalog。关闭同样是**请求**，会话会先排空已入队消息再发 close 帧。
    pub async fn node_reauth(&self, node: &NodeId) -> usize {
        let handles = self.registry.handles_for_node(node);
        if handles.is_empty() {
            return 0;
        }
        for handle in &handles {
            handle.request_close(
                close::NORMAL,
                "the node trust was re-confirmed; reconnect to fetch the updated catalog",
            );
        }
        for handle in &handles {
            self.forget_connection(handle.connection_id().as_str());
        }
        info!(
            event = "node_link.node_reauth_invalidated_connections",
            access_node_id = node.as_str(),
            connections = handles.len(),
            "the re-confirmed node trust invalidated its existing connections; the peer is expected to reconnect"
        );
        handles.len()
    }

    /// 当前时间（唯一来源是 `Authority` 注入的 `Clock`，本层不读系统时间）。
    fn clock(&self) -> CoreTimestamp {
        self.authority.now()
    }
}

#[async_trait::async_trait]
impl MessageRoute for CommandRoute {
    async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        match message.message_type() {
            MessageType::CommandSubmit => self.on_submit(session, message).await,
            MessageType::CommandStatus => self.on_standalone_status(session, message).await,
            _ => RouteOutcome::Unclaimed,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 纯函数：判定、映射与 wire 装配
// ---------------------------------------------------------------------------------------------

/// 两个「本切片不接受客户端参数」的命令的禁带字段判定（§12.7）。
///
/// `session.create`：返回 `(requestId, command, 字段名)`。
/// - 不在 `agentId`/`exportId`/`workspaceAlias`/`templateParams` 白名单里的键一律拒
///   （`cwd`/`mcpServers`/`apiKey`/`token`/`env`/`credential` 等）；
/// - 白名单键上的**绝对路径**取值也拒（`workspaceAlias` 的 pattern 已挡住大部分，这里覆盖驱动号与
///   UNC 形态）；
/// - `templateParams` 出现且不是空对象即拒（首切片 template 零参数）。
///
/// `session.resume`：**任何**键都拒（`payload` 必须是空对象 `{}`；恢复所需的 Agent 标识、ACP 会话
/// 标识与创建时目录一律取自 Owner 自身的持久化记录）。
///
/// 这是一个**判定**（只看键与取值形态），不是解析路径：真正落地的参数一律由严格解码后的类型给出。
/// body 不是这两个命令、或取不到 `requestId`/`command` 时返回 `None`，交给严格解码处理；
/// 键超过 `details.field` 的上限时回 [`UNKNOWN_FIELD`]，不把超长输入塞进错误体。
fn forbidden_payload_field(body: &str) -> Option<(Uuid, CommandName, String)> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let command = value.get("command")?.as_str()?;
    let request = Uuid::parse(value.get("requestId")?.as_str()?).ok()?;
    let command = CommandName::from_str(command).ok()?;
    let payload = value.get("payload")?.as_object()?;
    let rejected = |key: &str, value: &serde_json::Value| match command {
        CommandName::SessionResume => true,
        CommandName::SessionCreate => {
            !matches!(
                key,
                "agentId" | "exportId" | "directoryId" | "templateParams"
            ) || match key {
                "templateParams" => !is_empty_object(value),
                _ => value.as_str().is_some_and(is_absolute_path),
            }
        }
        _ => false,
    };
    for (key, value) in payload {
        if rejected(key.as_str(), value) {
            let field = if key.chars().count() <= MAX_DETAILS_FIELD {
                key.clone()
            } else {
                UNKNOWN_FIELD.to_owned()
            };
            return Some((request, command, field));
        }
    }
    None
}

fn is_empty_object(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|object| object.is_empty())
}

/// 取值是否为绝对路径（POSIX 前缀、Windows 驱动号、UNC）。
fn is_absolute_path(text: &str) -> bool {
    if text.starts_with('/') || text.starts_with("\\\\") {
        return true;
    }
    let mut chars = text.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(drive), Some(':'), Some('/' | '\\')) if drive.is_ascii_alphabetic()
    )
}

/// 持久化记录 → `command.terminal`（未终结的 `accepted` 返回 `None`）。
fn terminal_body(record: &CommandRecord, at: &CoreTimestamp) -> Option<CommandTerminal> {
    let command = command_name(record.command())?;
    let status = match record.status() {
        CoreStatus::Completed => TerminalStatus::Completed,
        CoreStatus::Failed => TerminalStatus::Failed,
        CoreStatus::Uncertain => TerminalStatus::Uncertain,
        CoreStatus::Rejected => TerminalStatus::Rejected,
        CoreStatus::Accepted => return None,
    };
    // 终态时间：三种终结的 mutation 必有持久化值；`rejected` 行没有（core 的 `owned_command` 只写
    // accepted/completed/failed/uncertain，`Rejected` 记录 `terminal_at` 恒为 `None`），回退到本次应答
    // 时间。这里不发明持久化事实之外的语义：status 与 result 全部取自记录本身。
    let terminal_at = match record.terminal_at() {
        Some(value) => wire_timestamp(value)?,
        None => wire_timestamp(at)?,
    };
    // `completed` 必带非空 `result`：`session.create` 的持久结果就是 `SessionCreateResult` 的原文
    // （§12.7 强制该形状），还原成具名变体；其余命令的开放对象按 `RawObject` 字节保真承载，
    // core 记录里 `result` 为 NULL 时补一个空 object（schema 只要求 object，不编造字段）。
    let result = match status {
        TerminalStatus::Completed => completed_result(command, record),
        _ => Nullable::null(),
    };
    let error = match status {
        TerminalStatus::Completed => Nullable::null(),
        _ => {
            let mapped = match record.error() {
                Some(error) => error_info(error.code()),
                // `uncertain` 允许没有 `error`：wire 要求非 `completed` 必须给出 error，因此补一个
                // 本协议自己的 `nodelink.command.uncertain`（不猜成功或失败）。
                None if status == TerminalStatus::Uncertain => {
                    error_info("nodelink.command.uncertain")
                }
                None => error_info("nodelink.internal.unavailable"),
            };
            Nullable::from_option(Some(mapped))
        }
    };
    Some(CommandTerminal {
        request_id: uuid_of(record.request().as_str())?,
        command,
        terminal: Terminal {
            status,
            terminal_at,
            terminal_event_id: match record.terminal_event() {
                Some(event) => Nullable::from_option(Uuid::parse(event.as_str()).ok()),
                None => Nullable::null(),
            },
            result,
            error,
        },
    })
}

/// `completed` 终态的结果对象。
///
/// `session.create` 必须回 `SessionCreateResult`、`session.resume` 必须回 `SessionResumeResult`
/// （schema 的 `if/then`）：持久记录里存的正是该形状的原文，这里还原成具名变体；形态不符（不是本切片
/// 写的行）→ 退回开放对象并记一条警告（不伪造 `{}`）。
fn completed_result(
    command: CommandName,
    record: &CommandRecord,
) -> Nullable<CommandResultPayload> {
    let Some(object) = parsed_result(record) else {
        return Nullable::null();
    };
    if command == CommandName::SessionCreate {
        match serde_json::from_str::<SessionCreateResult>(object.get()) {
            Ok(created) => {
                return Nullable::from_option(Some(CommandResultPayload::SessionCreate(created)));
            }
            Err(_) => warn!(
                event = "node_link.session_create_result_unreadable",
                request_id = record.request().as_str(),
                "the persisted session.create result is not a SessionCreateResult"
            ),
        }
    }
    if command == CommandName::SessionResume {
        match serde_json::from_str::<SessionResumeResult>(object.get()) {
            Ok(resumed) => {
                return Nullable::from_option(Some(CommandResultPayload::SessionResume(resumed)));
            }
            Err(_) => warn!(
                event = "node_link.session_resume_result_unreadable",
                request_id = record.request().as_str(),
                "the persisted session.resume result is not a SessionResumeResult"
            ),
        }
    }
    Nullable::from_option(Some(CommandResultPayload::Object(object)))
}

/// core 记录里的 `result` 文本（为 NULL 时补空 object；文本不是 object 时返回 `None`）。
fn parsed_result(record: &CommandRecord) -> Option<RawObject> {
    match record.result() {
        Some(result) => RawObject::parse(result.as_str()).ok(),
        None => Some(RawObject::empty()),
    }
}

/// wire 的 `PublicError` → core 的 `PublicError`（终态落盘用；`code`/`message`/`retryable`/`details`
/// 原样承载，回读时逐字一致）。
fn core_error(error: &PublicError) -> Option<acp_core::model::PublicError> {
    let details = acp_core::model::ViewJson::new(error.details.get()).ok()?;
    acp_core::model::PublicError::try_new(
        error.code.as_str(),
        error.message.as_str(),
        error.retryable,
        details,
    )
    .ok()
}

/// core 记录的状态 → 命令级日志的终态类别。
///
/// `accepted`/`rejected` 返回 `None`：前者不是终态，后者不是**已接受**命令的终态（那条命令从未被接受，
/// 它的结局由 `node_link.command_rejected` 一类日志记录）。
fn record_terminal_class(status: CoreStatus) -> Option<TerminalClass> {
    match status {
        CoreStatus::Completed => Some(TerminalClass::Completed),
        CoreStatus::Failed => Some(TerminalClass::Failed),
        CoreStatus::Uncertain => Some(TerminalClass::Uncertain),
        CoreStatus::Accepted | CoreStatus::Rejected => None,
    }
}

/// 本地合成终态的类别（与 [`local_terminal`] 的同一个 `match`，不另立第二套判定）。
fn local_terminal_class(outcome: &LocalOutcome) -> TerminalClass {
    match outcome {
        LocalOutcome::Completed(_) => TerminalClass::Completed,
        LocalOutcome::Failed(_) => TerminalClass::Failed,
        LocalOutcome::Uncertain(_) => TerminalClass::Uncertain,
    }
}

/// 本地合成终态的成因：本层投影出的 wire 错误码 → 封闭词表（这一档手里没有 core 的错误对象；
/// [`Cause::of_code`] 同样认识 `nodelink.*` 的细分码）。
fn local_outcome_cause(outcome: &LocalOutcome) -> Cause {
    match outcome {
        LocalOutcome::Completed(_) => Cause::None,
        LocalOutcome::Failed(error) | LocalOutcome::Uncertain(error) => {
            Cause::of_code(error.code.as_str())
        }
    }
}

/// `session.create` / `session.resume` 的本地合成终态（`completed` 带具名结果变体，其余带原错误）。
///
/// 只在**没有**持久记录时使用（工作在幂等行落盘前就失败）：有记录的场景一律以记录为唯一权威。
fn local_terminal(
    command: CommandName,
    request: &Uuid,
    outcome: &LocalOutcome,
    at: &CoreTimestamp,
) -> Option<CommandTerminal> {
    let terminal_at = wire_timestamp(at)?;
    Some(CommandTerminal {
        request_id: request.clone(),
        command,
        terminal: match outcome {
            LocalOutcome::Completed(result) => Terminal {
                status: TerminalStatus::Completed,
                terminal_at,
                terminal_event_id: Nullable::null(),
                result: Nullable::from_option(Some(result.clone())),
                error: Nullable::null(),
            },
            LocalOutcome::Failed(error) => Terminal {
                status: TerminalStatus::Failed,
                terminal_at,
                terminal_event_id: Nullable::null(),
                result: Nullable::null(),
                error: Nullable::from_option(Some(error.clone())),
            },
            LocalOutcome::Uncertain(error) => Terminal {
                status: TerminalStatus::Uncertain,
                terminal_at,
                terminal_event_id: Nullable::null(),
                result: Nullable::null(),
                error: Nullable::from_option(Some(error.clone())),
            },
        },
    })
}

/// `node.trust.revoked.reason` 的固定文本（schema 上限 512，close reason 不含敏感信息）。
fn node_revocation_reason() -> Text<512> {
    Text::<512>::parse("the owner revoked this node trust")
        .unwrap_or_else(|_| Text::<512>::parse("revoked").expect("固定短文本"))
}

/// core 的错误码 → wire 的封闭错误码（词表之外的细分只进结构化日志，不冒充 wire 码）。
///
/// 已知**永久**失败必须映射成语义最近的登记码，**不能**落到 `internal.unavailable`：
/// 那个码在 registry 里是 `retryable = true`，会把「重试也不会变」的失败标成可重试，Access 于是按
/// `retryable` 自动重试一条永远不会成功的命令。本表的两支都是永久失败：
/// - `capability.unsupported_by_{client,broker,agent}`：能力协商失败，端到端不可用（Node Link 没有
///   自己的 capability 错误码，`nodelink.command.unsupported` 是登记过的最接近语义）；
/// - `state.version_conflict`：未修改的重提交必然再次冲突（Access 必须先重新 attach/重读版本），
///   同样不作为可重试信号。
///
/// 仍然落在 `internal.unavailable` 的只有：真正未知的 core 码（保守：宁可按可重试处理，也不谎报
/// 「永久」）与 registry 里本来就登记为可重试的码（如 `session.busy`）。
fn wire_error(core_code: &str) -> Option<(ErrorCode, &'static str)> {
    Some(match core_code {
        // 能力类永久失败（见函数文档）。
        "capability.unsupported_by_client"
        | "capability.unsupported_by_broker"
        | "capability.unsupported_by_agent" => (
            ErrorCode::CommandUnsupported,
            "the command is not supported by this owner",
        ),
        // 状态版本冲突：未修改的重提交是永久失败（见函数文档）。
        "state.version_conflict" => (
            ErrorCode::CommandUnsupported,
            "the command conflicts with the owner's current session state",
        ),
        // core 自己的「无法确认副作用」码（启动恢复写进终态记录的那一个）。
        "command.uncertain" => (
            ErrorCode::CommandUncertain,
            "the owner cannot confirm the outcome of this command",
        ),
        "authorization.scope_denied" | "export.not_granted" | "nodelink.export.not_granted" => (
            ErrorCode::ProtocolSequenceInvalid,
            "the command is not granted to this node",
        ),
        "command.idempotency_conflict" | "nodelink.command.idempotency_conflict" => (
            ErrorCode::CommandIdempotencyConflict,
            "the requestId was already used for a different command or payload",
        ),
        "command.unsupported" | "nodelink.command.unsupported" => (
            ErrorCode::CommandUnsupported,
            "the command is not supported by this owner",
        ),
        "command.not_found" | "nodelink.command.not_found" => (
            ErrorCode::CommandNotFound,
            "the request is not known to this owner",
        ),
        "nodelink.command.unsupported_field" => (
            ErrorCode::CommandUnsupportedField,
            "the command carries a field this owner does not accept",
        ),
        "nodelink.command.uncertain" => (
            ErrorCode::CommandUncertain,
            "the owner cannot confirm the outcome of this command",
        ),
        "protocol.schema_invalid" | "nodelink.protocol.schema_invalid" => (
            ErrorCode::ProtocolSchemaInvalid,
            "the command does not match the v1 schema",
        ),
        "session.not_found" | "export.not_found" => (
            ErrorCode::ProtocolSequenceInvalid,
            "the referenced session is not available to this node",
        ),
        _ => (
            ErrorCode::InternalUnavailable,
            "the owner node cannot serve the command",
        ),
    })
}

/// 错误码 → wire 的 `PublicError`（`retryable` 取 registry 登记的语义，不自行发明）。
fn error_info(code: &str) -> PublicError {
    let (code, message) = wire_error(code).unwrap_or((
        ErrorCode::InternalUnavailable,
        "the owner node cannot serve the command",
    ));
    PublicError {
        code,
        message: Text::parse(message).unwrap_or_else(|_| {
            Text::parse("the owner node cannot serve the command").expect("固定文本")
        }),
        retryable: code.default_retryable(),
        details: RawObject::empty(),
    }
}

/// `nodelink.*` 错误码文本 → wire 枚举（core 以 `PortError::InvalidRequest("nodelink.…")` 传回的场合）。
fn error_code_of(text: &str) -> Option<ErrorCode> {
    ErrorCode::from_str(text).ok()
}

/// 端口错误 → core 错误码（只为映射到 wire 码）。
fn port_error_code(error: &PortError) -> String {
    match error {
        PortError::InvalidRequest(reason) => (*reason).to_owned(),
        PortError::NotFound(_) => "command.not_found".to_owned(),
        // 「后端不支持该操作」（§2）：目标 Agent 未宣告能力，或该会话没有持久化恢复数据。它**不是**
        // 临时故障——重试不会变，因此映射到既有的永久失败码 `nodelink.command.unsupported`
        // （registry 里 `retryable = false`），而不是按可重试处理的 `internal.unavailable`。
        // 本函数只产出 core 侧的错误**文本**，wire 码由 `wire_error` 解析（不新增任何错误码）。
        PortError::Unavailable(acp_core::model::UnavailableKind::BackendUnsupported) => {
            "nodelink.command.unsupported".to_owned()
        }
        _ => "internal.unavailable".to_owned(),
    }
}

/// `details.field`（`nodelink.command.unsupported_field` 的必需登记字段）。
fn field_details(field: &str) -> RawObject {
    details_object(&serde_json::json!({ "field": field }))
}

/// `details.parameter`（`nodelink.export.not_granted` 的可选登记字段）。
fn parameter_details(parameter: &str) -> RawObject {
    details_object(&serde_json::json!({ "parameter": parameter }))
}

/// `details.retryAfterMs`（`nodelink.resource.rate_limited` 的登记字段）。
fn rate_limit_details(retry_after: Duration) -> RawObject {
    details_object(&serde_json::json!({ "retryAfterMs": retry_after.as_millis() as u64 }))
}

fn details_object(value: &serde_json::Value) -> RawObject {
    RawObject::parse(&value.to_string()).unwrap_or_else(|_| RawObject::empty())
}

/// wire 命令名文本 → 枚举。
fn command_name(text: &str) -> Option<CommandName> {
    CommandName::from_str(text).ok()
}

fn core_request(value: &Uuid) -> Option<CoreRequestId> {
    CoreRequestId::new(value.as_str()).ok()
}

fn core_session(text: &str) -> Option<SessionId> {
    SessionId::new(text).ok()
}

fn uuid_of(text: &str) -> Option<Uuid> {
    Uuid::parse(text).ok()
}

/// Node Link 的 actor：`node` 与 `access_node` 都是该对端（§12.5 幂等键的节点维度）。
fn node_actor(node: &NodeId) -> Actor {
    Actor::Node {
        node: node.clone(),
        access_node: node.clone(),
    }
}

/// core 的 `Timestamp`（固定 24 字符文本）→ wire 的 `Timestamp`（同形同校验）。
fn wire_timestamp(value: &CoreTimestamp) -> Option<WireTimestamp> {
    WireTimestamp::parse(value.as_str()).ok()
}

/// 时钟给出的值不是规范 wire 时间戳（本机缺陷：`Clock` 实现不满足合同）。
fn clock_fault() -> PortError {
    PortError::Corrupt("the clock produced a non-canonical timestamp")
}

/// core 的 `SessionSummary` → Sync 的 `sessionSummary`（§11.4 的共享合同，Node Link 不另立形状）。
fn session_summary(
    summary: &acp_core::model::SessionSummary,
) -> Option<sync_protocol::common::SessionSummary> {
    use sync_protocol::common as sync;
    let agent = summary.agent();
    let state = match summary.state() {
        acp_core::model::SessionState::Idle => sync::SessionState::Idle,
        acp_core::model::SessionState::Queued => sync::SessionState::Queued,
        acp_core::model::SessionState::Running => sync::SessionState::Running,
        acp_core::model::SessionState::WaitingInput => sync::SessionState::WaitingInput,
        acp_core::model::SessionState::WaitingPermission => sync::SessionState::WaitingPermission,
        acp_core::model::SessionState::Failed => sync::SessionState::Failed,
        acp_core::model::SessionState::Closed => sync::SessionState::Closed,
    };
    let origin = match summary.origin() {
        ResourceOrigin::Local => sync::OriginBlock::Local,
        // Owner 侧只投影本机 owned 会话；imported 摘要不该出现在这里（本切片没有 imported 路径）。
        ResourceOrigin::Remote { .. } => return None,
    };
    let current_mode = match summary.current_mode() {
        Some(mode) => Nullable::from_option(Some(sync::ModeRef {
            mode_id: sync::NonEmptyText::<256>::parse(mode.mode_id().as_str()).ok()?,
            display_name: sync::NonEmptyText::<256>::parse(mode.display_name()).ok()?,
        })),
        None => Nullable::null(),
    };
    Some(sync::SessionSummary {
        session_id: Uuid::parse(summary.session_id().as_str()).ok()?,
        title: match summary.title() {
            Some(title) => Nullable::from_option(Some(sync::Text::<512>::parse(title).ok()?)),
            None => Nullable::null(),
        },
        agent: sync::SessionAgentRef {
            agent_id: sync::NonEmptyText::<128>::parse(agent.agent_id().as_str()).ok()?,
            name: sync::NonEmptyText::<128>::parse(agent.name()).ok()?,
        },
        state,
        origin,
        current_mode,
        version: node_link_protocol::common::DecimalString::parse(
            &summary.version().get().to_string(),
        )
        .ok()?,
        created_at: wire_timestamp(summary.created_at())?,
        updated_at: wire_timestamp(summary.updated_at())?,
        // Node Link 投影不扩展目录引用（本次变更的 Non-Goal）：键保持**缺席**而非 `null`，
        // 因此未协商 `core.local-catalog.v1` 的客户端不会因未知字段拒绝整条摘要。
        workspace: None,
    })
}

/// Export 的 agent 展示名（目录里查不到就退回 agentId，不编造名字）。
fn agent_display_name(view: &NodeLinkCatalogView, agent: &str) -> String {
    view.agents
        .iter()
        .find(|descriptor| descriptor.agent.agent_id().as_str() == agent)
        .map(|descriptor| descriptor.agent.name().to_owned())
        .unwrap_or_else(|| agent.to_owned())
}

/// wire payload → core payload（判别式由 `command` 给出，`node-link-protocol` 已校验形状）。
fn core_payload(command: CommandName, submit: &CommandSubmit) -> Result<CorePayload, &'static str> {
    let payload = match &submit.payload {
        WirePayload::SessionList(_) => CorePayload::SessionList {},
        WirePayload::SessionRead(read) => CorePayload::SessionRead {
            include: read
                .include
                .as_slice()
                .iter()
                .map(|item| match item {
                    WireInclude::Messages => ReadInclude::Messages,
                    WireInclude::Turns => ReadInclude::Turns,
                    WireInclude::PendingInteractions => ReadInclude::PendingInteractions,
                    WireInclude::ConfigOptions => ReadInclude::ConfigOptions,
                    WireInclude::Capabilities => ReadInclude::Capabilities,
                })
                .collect(),
        },
        WirePayload::CommandStatus(status) => CorePayload::CommandStatus {
            target_request: core_request(&status.target_request_id)
                .ok_or("the targetRequestId is not a uuid")?,
        },
        WirePayload::SessionModeList(_) => CorePayload::ModeList {},
        WirePayload::SessionConfigList(_) => CorePayload::ConfigList {},
        WirePayload::SessionPrompt(prompt) => {
            let mut content = Vec::with_capacity(prompt.content.as_slice().len());
            for block in prompt.content.as_slice() {
                // 内容块按原文交给 core（core 只做不透明载体），不重新构造字段。
                let text = serde_json::to_string(block)
                    .map_err(|_| "the prompt block cannot be re-encoded")?;
                content.push(
                    PromptContentBlock::from_json_text(&text)
                        .map_err(|_| "the prompt block is not a valid content block")?,
                );
            }
            CorePayload::Prompt { content }
        }
        WirePayload::SessionCancel(cancel) => CorePayload::Cancel {
            turn: Some(
                TurnId::new(cancel.turn_id.as_str()).map_err(|_| "the turnId is not a uuid")?,
            ),
        },
        WirePayload::ElicitationRespond(respond) => CorePayload::ElicitationRespond {
            interaction: InteractionId::new(respond.interaction_id.as_str())
                .map_err(|_| "the interactionId is not a uuid")?,
            action: match respond.action {
                WireElicitationAction::Submit => CoreElicitationAction::Submit,
                WireElicitationAction::Decline => CoreElicitationAction::Decline,
                WireElicitationAction::Cancel => CoreElicitationAction::Cancel,
            },
            values: match respond.values.as_ref() {
                Some(values) => ElicitationValues::parse_json(values.get())
                    .map_err(|_| "the elicitation values are not acceptable")?,
                None => ElicitationValues::null(),
            },
        },
        WirePayload::SessionModeSet(set) => CorePayload::ModeSet {
            mode: ModeId::new(set.mode_id.as_str()).map_err(|_| "the modeId is out of range")?,
        },
        WirePayload::SessionConfigSet(set) => CorePayload::ConfigSet {
            id: ConfigOptionId::new(set.config_id.as_str())
                .map_err(|_| "the configId is out of range")?,
            value: match &set.value {
                WireConfigValue::String(text) => CoreConfigValue::text(text.as_str())
                    .map_err(|_| "the config value is out of range")?,
                WireConfigValue::Boolean(flag) => CoreConfigValue::Boolean(*flag),
            },
        },
        WirePayload::PermissionResolve(resolve) => CorePayload::PermissionResolve {
            interaction: InteractionId::new(resolve.interaction_id.as_str())
                .map_err(|_| "the interactionId is not a uuid")?,
            option_id: resolve.option_id.as_str().to_owned(),
        },
        WirePayload::SessionCreate(_) => {
            return Err("session.create is dispatched by its own handler");
        }
        // 与 `session.create` 同形（Path A）：`session.resume` 没有 core payload 变体，
        // 它的 `accepted` 行与幂等行由 core 的 `resume_session` 自建，因此本函数不映射它。
        WirePayload::SessionResume(_) => {
            return Err("session.resume is dispatched by its own handler");
        }
    };
    if payload.family()
        != match command {
            CommandName::SessionList
            | CommandName::SessionRead
            | CommandName::CommandStatus
            | CommandName::SessionModeList
            | CommandName::SessionConfigList => CommandKind::Query,
            _ => CommandKind::Mutation,
        }
    {
        return Err("the payload does not match the command name");
    }
    Ok(payload)
}

/// 解码后 payload 的 ACPR-CJ1 摘要（core 的幂等比对依据，§6 第 6 条）。
fn fingerprint_of<T: serde::Serialize>(payload: &T) -> Option<acp_core::model::Digest> {
    let text = serde_json::to_string(payload).ok()?;
    let canonical = acpr_wire::cj1::canonicalize(&text).ok()?;
    let bytes = Sha256::digest(canonical.as_bytes());
    use base64::Engine as _;
    acp_core::model::Digest::new(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
        .ok()
}

/// 解码消息 body（失败按 schema 错误处理）。
fn decode<T: DeserializeOwned>(message: &Envelope) -> Option<T> {
    serde_json::from_str(message.body().get()).ok()
}

/// 锁中毒时取回内层数据而不是 panic：命令管线在任何 panic 之后仍要能回答与关闭连接。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

