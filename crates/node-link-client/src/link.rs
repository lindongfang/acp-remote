//! 已认证的业务连接：请求复用、catalog/事件处理、序号、心跳与「结果不确定」。
//!
//! ## 所有权
//!
//! 一条连接 = 一个 [`Link::run`] 任务。它独占读写两半（[`MessageSink`]/[`MessageSource`]）与全部连接期
//! 状态（下一个出站序号、未完成命令、attach 等待队列）。其它模块只持有 [`SharedLink`]（一个发件箱与
//! 观察快照），**不能**直接写 socket，因此不存在第二条写路径。关闭顺序：shutdown 信号 → 写 close 帧 →
//! 任务返回 → supervisor join（[`crate::manager`]）。
//!
//! ## 稳定 requestId
//!
//! `command.submit.requestId` 由调用方给出并在重发时复用：本模块**不**在重试路径上生成新 id。
//! 断线发生在命令发出之后、终态之前时，未完成命令以 [`ClientError::Uncertain`] 结束，并登记到
//! [`SharedLink::uncertain`]；重连后 supervisor 只发 `command.status` 查询，**绝不**重放副作用命令。
//!
//! ## 事件投递
//!
//! `resource.event` 交给 [`ImportedSink::event`]：它先提交无正文交付收据、再发布，然后返回可确认的
//! origin cursor；本模块在该调用**成功之后**才发 `resource.ack`。因此「已 ACK」永远意味着「已进入
//! 可恢复的无正文索引」。
//!
//! ## 订阅拒绝
//!
//! `resource.subscribe` 没有成功回复，因此「已订阅」只能由该会话的第一条入站帧（事件或快照 chunk）
//! 证明。Owner 的请求级拒绝（`link.error` + `correlationId`）按 `retryable` 分流：不可重试的拒绝
//! 进入 [`SharedLink::subscription_rejected`]（恢复计划不再重试）；可重试的拒绝进入
//! [`SharedLink::subscription_fault`]，并在本连接内按有界指数退避用**同一 cursor** 重发（`stale`
//! 代际先重新 attach），预算耗尽后保留可观测错误、交给下一次重连的恢复计划。重发从不跳过未被补齐的
//! 位置，因此缺口不会变成永久空洞，也不会形成请求风暴。

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use acp_core::model::NodeId;
use acp_core::model::Timestamp as CoreTimestamp;
use node_link_protocol::catalog::{CatalogSnapshot, NodeTrustRevoked};
use node_link_protocol::command::{
    CommandAccepted, CommandName, CommandPayload, CommandRejected, CommandResultPayload,
    CommandStatus as StatusBody, CommandSubmit, CommandTerminal, Terminal, TerminalStatus,
};
use node_link_protocol::common::{
    DecimalString, NodeLinkLimits, Nullable, OriginCursor, RemoteSessionRef, SessionMeta, Uuid,
};
use node_link_protocol::envelope::{Envelope, MessageType, Phase, encode_body};
use node_link_protocol::error::{Body as LinkErrorBody, ErrorCode, Ping, Pong};
use node_link_protocol::resource::{
    Ack, Attach, Attached as AttachedBody, Event, SnapshotItems, SnapshotResource, Subscribe,
};
use tokio::sync::{mpsc, oneshot, watch};
use uuid::Uuid as RandomUuid;

use crate::error::ClientError;
use crate::handshake::{
    Established, HandshakeConfig, handshake, random_bytes_16, wire_connection_fields,
};
use crate::runtime::{EventDisposition, RemoteRuntime};
use crate::transport::{Incoming, MessageSink, MessageSource};

/// 心跳的默认间隔（与 `node.ready.limits.heartbeatIntervalMs` 的默认 30 秒一致）。
pub const DEFAULT_HEARTBEAT: Duration = Duration::from_secs(30);

/// 静默超时（§2.5：连续 90 秒无入站消息或 pong 时对端可关闭；本端也按同一口径放弃）。
pub const DEFAULT_SILENCE_TIMEOUT: Duration = Duration::from_secs(90);

/// 未完成命令的等待窗口：超过后本端放弃等待（对端仍可凭 `command.status` 重查）。
pub const COMMAND_WAIT_TIMEOUT: Duration = Duration::from_secs(120);

/// 本端允许的未完成命令上限（服务端 `maxInFlightCommands` 默认 32）。
pub const MAX_IN_FLIGHT_COMMANDS: usize = 32;

/// 命令的完成条件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completion {
    /// 收到 `command.accepted` 即返回（mutation 的接受语义；终态由事件流表达）。
    Accepted,
    /// 等待 `command.terminal`（`session.create`/`session.resume` 与 `command.status`）。
    Terminal,
}

/// 命令结果（wire 原样，不做第二次解释）。
#[derive(Debug, Clone)]
pub enum CommandOutcome {
    /// 已接受（`command.accepted`），可能带同步结果。
    Accepted {
        accepted_at: node_link_protocol::common::Timestamp,
        result: Option<CommandResultPayload>,
    },
    /// 终态（`command.terminal`）。
    Terminal(Box<Terminal>),
    /// `command.rejected`（未接受、无副作用）。
    Rejected(Box<node_link_protocol::common::PublicError>),
}

impl CommandOutcome {
    /// 终态状态（非终态返回 `None`）。
    pub fn terminal_status(&self) -> Option<TerminalStatus> {
        match self {
            Self::Terminal(terminal) => Some(terminal.status),
            _ => None,
        }
    }
}

/// 连接期观察快照（供远程会话与组合根读）。
#[derive(Debug, Clone, Default)]
pub struct LinkSnapshot {
    pub online: bool,
    pub connection_id: Option<Uuid>,
    pub selected_features: Vec<String>,
    pub limits: Option<NodeLinkLimits>,
    pub server_epoch: Option<Uuid>,
    pub catalog_revision: Option<DecimalString>,
}

/// 一条会话的临时路由凭据（`resource.attached` 的结果）。
#[derive(Debug, Clone)]
pub struct AttachmentTicket {
    pub id: Uuid,
    pub generation: DecimalString,
    pub remote: RemoteSessionRef,
    pub meta: SessionMeta,
}

/// 出站请求（只有 [`Link::run`] 能把它变成帧）。
pub(crate) enum Outbound {
    Command {
        submit: CommandSubmit,
        completion: Completion,
        reply: oneshot::Sender<Result<CommandOutcome, ClientError>>,
    },
    Attach {
        remote: RemoteSessionRef,
        reply: oneshot::Sender<Result<AttachmentTicket, ClientError>>,
    },
    Subscribe {
        remote: RemoteSessionRef,
        attachment: Uuid,
        generation: DecimalString,
        cursor: Option<OriginCursor>,
        /// 帧写入传输层后回 `Ok(())`；本地编码/发送失败回 `Err`。
        ///
        /// 订阅**成功**在协议上没有回复，因此 `Ok` 只表示「已交给传输层」；Owner 的请求级拒绝
        /// 由 [`SharedLink::subscription_error`] 观测。
        reply: oneshot::Sender<Result<(), ClientError>>,
    },
    Ack {
        session: RemoteSessionRef,
        cursor: OriginCursor,
    },
    Close,
}

struct Pending {
    command: CommandName,
    completion: Completion,
    reply: Option<oneshot::Sender<Result<CommandOutcome, ClientError>>>,
    /// 该命令所属的远程会话（不确定登记与 `command.status` 恢复用）。
    session: Option<RemoteSessionRef>,
    /// 这条 `command.status` 查询要清除的不确定命令（只读恢复路径）。
    uncertain_target: Option<Uuid>,
    /// Owner 在 `command.accepted`/`command.terminal`/`command.rejected` 里回填的关联键。
    ///
    /// 普通命令就是 `command.submit.requestId`；`command.status` 则是被查询的目标 `requestId`
    /// （服务端以目标命令的记录回复，而不是查询自己的 id）。用独立字段而不是 map 键，使同一条
    /// 查询与被查询命令的在途提交可以同时存在而不互相覆盖。
    wire_request: Uuid,
    /// 本端发送该命令帧时使用的 `messageId`（`link.error.correlationId` 用它回指请求）。
    message_id: Option<Uuid>,
    /// 是否是 `command.status` 查询：`command.accepted` 也要结清它（不确定项保留到终态）。
    status_query: bool,
}

/// 重连后需要恢复的会话订阅（`cursor = None` 表示先取快照）。
#[derive(Debug, Clone)]
pub struct ResumeSession {
    pub remote: RemoteSessionRef,
    pub cursor: Option<OriginCursor>,
}

/// 一条 attach 的等待项。
struct AttachRequest {
    remote: RemoteSessionRef,
    /// 本端发送该帧时使用的 `messageId`：Owner 的请求级拒绝以 `link.error.correlationId` 回指它，
    /// 因此 attach 拒绝能立即结清，而不是让调用方等满 [`COMMAND_WAIT_TIMEOUT`]。
    message_id: Uuid,
    subscribe: Option<Option<OriginCursor>>,
    /// 随 attach 成功的这次订阅已经重发过几次（重试路径经重新 attach 重发时递增）。
    subscribe_attempts: u32,
    reply: Option<oneshot::Sender<Result<AttachmentTicket, ClientError>>>,
}

/// 非命令资源请求的类别（用于拒绝日志与过期 ticket 处理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceKind {
    Subscribe,
    Ack,
}

impl ResourceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Subscribe => "resource.subscribe",
            Self::Ack => "resource.ack",
        }
    }
}

/// 已发出、尚未被 Owner 拒绝的非命令资源请求。
///
/// 保留 `messageId` 只为匹配 `link.error.correlationId`：`resource.subscribe`/`resource.ack` 没有
/// 成功回复，所以条目在窗口内保留、由后续拒绝或连接结束丢弃。窗口有界（[`TRACKED_RESOURCE_REQUESTS`]）：
/// ACK 是累计水位，新的覆盖旧语义，因此丢弃更早的条目不会丢失「当前水位」的可观测性。
struct ResourceRequest {
    message_id: Uuid,
    remote: RemoteSessionRef,
    kind: ResourceKind,
    /// `resource.subscribe` 的重试上下文：被可重试拒绝后按**同一 cursor** 重新订阅。
    ///
    /// `None` 用于 `resource.ack`（累计水位，重发没有意义：本地 ACK 已推进，重连/重放自然收敛）。
    resubscribe: Option<Resubscribe>,
}

/// 一条已发出订阅的重试上下文。
#[derive(Debug, Clone)]
struct Resubscribe {
    /// 首次订阅请求的 cursor：重试必须从同一位置续读，不能跳到未经补齐的位置。
    cursor: Option<OriginCursor>,
    /// 本连接上已经重发过的次数（首次请求为 0）。
    attempts: u32,
}

/// 一条等待重发的订阅/恢复 attach。
struct SubscriptionRetry {
    remote: RemoteSessionRef,
    cursor: Option<OriginCursor>,
    /// 即将发出的这次重发是第几次（1 起）。
    attempts: u32,
    at: tokio::time::Instant,
}

/// 单条连接上订阅重试的退避起点。
const SUBSCRIPTION_RETRY_BASE: Duration = Duration::from_millis(100);

/// 单条连接上同一会话的重试次数上限。
///
/// 预算耗尽后停止本连接内重试并保留可观测错误；下一次重连的恢复计划仍会尝试（`retryable` 不是
/// 永久结论），因此 Owner 持续拒绝时既不会留下事件缺口，也不会形成请求风暴。
const MAX_SUBSCRIPTION_RETRIES: u32 = 4;

/// 第 `attempts` 次重发的退避（100/200/400/800 ms，上限 2 s）。
fn subscription_retry_backoff(attempts: u32) -> Duration {
    let shift = attempts.saturating_sub(1).min(4);
    (SUBSCRIPTION_RETRY_BASE * 2u32.pow(shift)).min(Duration::from_secs(2))
}

/// 登记（或替换）一条会话的重发计划：同一会话最多一个待发项，重复拒绝不会叠加。
fn schedule_subscription_retry(
    retries: &mut Vec<SubscriptionRetry>,
    remote: &RemoteSessionRef,
    cursor: Option<OriginCursor>,
    attempts: u32,
) {
    retries.retain(|retry| retry.remote.session_id.as_str() != remote.session_id.as_str());
    retries.push(SubscriptionRetry {
        remote: remote.clone(),
        cursor,
        attempts,
        at: tokio::time::Instant::now() + subscription_retry_backoff(attempts),
    });
}

/// 去掉某个会话待发的重发计划（调用方显式订阅或重新 attach 时由它接管）。
fn cancel_subscription_retry(retries: &mut Vec<SubscriptionRetry>, remote: &RemoteSessionRef) {
    retries.retain(|retry| retry.remote.session_id.as_str() != remote.session_id.as_str());
}

/// 发送失败的两种性质：编码失败是本机问题（连接仍可用），写失败必须结束连接。
enum SendFailure {
    Encode(ClientError),
    Transport(ClientError),
}

/// 保留的未决 `resource.subscribe`/`resource.ack` 关联键上限。
const TRACKED_RESOURCE_REQUESTS: usize = 64;

/// 已断开连接上「无法确认结果」的命令（重连后只查询，不重发）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncertainCommand {
    pub request_id: Uuid,
    pub command: CommandName,
    pub session: Option<RemoteSessionRef>,
}

/// 可重试拒绝之后、尚未被入站帧证明已恢复的订阅。
#[derive(Debug, Clone)]
pub struct SubscriptionFault {
    /// Owner 给出的拒绝原因（最近一次）。
    pub error: ClientError,
    /// 本连接上已经重发的次数。
    pub attempts: u32,
    /// 是否还会在本连接上继续重试；预算耗尽后为 `false`，交给下一次重连的恢复计划。
    pub retrying: bool,
}

struct SharedState {
    snapshot: LinkSnapshot,
    attachments: BTreeMap<String, AttachmentTicket>,
    uncertain: BTreeMap<String, UncertainCommand>,
    /// 被 Owner 以**不可重试**错误明确拒绝、本进程内不再自动恢复的订阅（wire session id → 原因）。
    ///
    /// 只影响 supervisor 的重连恢复计划：连接保持可用，显式 `attach` 仍可重试（用户可能重新导出了
    /// 会话），成功 attach 会清掉这里的记录。没有它，一个已撤销导出/旧 epoch cursor 会在每次重连时
    /// 重复被拒，形成重连循环。
    rejected: BTreeMap<String, ClientError>,
    /// 被 Owner 以**可重试**错误拒绝、已安排/正在退避重发的订阅（wire session id → 事实）。
    ///
    /// 与 [`Self::rejected`] 的区别是它不是永久结论：supervisor 的重连计划仍会包含该会话。条目在
    /// 该会话的第一条入站帧（事件或快照 chunk）到达时清除——那是协议层唯一能证明订阅已工作的信号。
    faults: BTreeMap<String, SubscriptionFault>,
}

/// 连接句柄：发件箱 + 观察快照。
pub struct SharedLink {
    owner: NodeId,
    outgoing: mpsc::Sender<Outbound>,
    state: Mutex<SharedState>,
    shutdown: watch::Sender<bool>,
}

impl std::fmt::Debug for SharedLink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedLink")
            .field("owner", &self.owner)
            .field("online", &self.is_online())
            .finish_non_exhaustive()
    }
}

impl SharedLink {
    fn new(owner: NodeId, outgoing: mpsc::Sender<Outbound>, shutdown: watch::Sender<bool>) -> Self {
        Self {
            owner,
            outgoing,
            state: Mutex::new(SharedState {
                snapshot: LinkSnapshot::default(),
                attachments: BTreeMap::new(),
                uncertain: BTreeMap::new(),
                rejected: BTreeMap::new(),
                faults: BTreeMap::new(),
            }),
            shutdown,
        }
    }

    /// Owner node id。
    pub fn owner(&self) -> &NodeId {
        &self.owner
    }

    /// 当前是否已认证可用。
    pub fn is_online(&self) -> bool {
        lock(&self.state).snapshot.online
    }

    /// 观察快照。
    pub fn snapshot(&self) -> LinkSnapshot {
        lock(&self.state).snapshot.clone()
    }

    /// 已协商的 feature。
    pub fn features(&self) -> Vec<String> {
        lock(&self.state).snapshot.selected_features.clone()
    }

    /// 是否协商了某个 feature。
    pub fn has_feature(&self, feature: &str) -> bool {
        lock(&self.state)
            .snapshot
            .selected_features
            .iter()
            .any(|selected| selected == feature)
    }

    /// 当前 attachment ticket（断线后为空；重连由 supervisor 重新 attach）。
    pub fn attachment(&self, session: &RemoteSessionRef) -> Option<AttachmentTicket> {
        lock(&self.state)
            .attachments
            .get(session.session_id.as_str())
            .cloned()
    }

    /// 按 attachment id 反查会话（新资源帧只带 attachmentId）。
    pub fn session_by_attachment(&self, id: &Uuid) -> Option<RemoteSessionRef> {
        lock(&self.state)
            .attachments
            .values()
            .find(|ticket| &ticket.id == id)
            .map(|ticket| ticket.remote.clone())
    }

    /// 上次断线时留下、尚未确认结果的命令。
    pub fn uncertain(&self) -> Vec<UncertainCommand> {
        lock(&self.state).uncertain.values().cloned().collect()
    }

    /// 记录一次「已通过 `command.status` 查询确认」的结果，清除不确定标记。
    pub fn clear_uncertain(&self, request_id: &Uuid) {
        lock(&self.state).uncertain.remove(request_id.as_str());
    }

    /// 该会话的自动恢复订阅是否已被 Owner 以不可重试错误拒绝（supervisor 据此跳过恢复计划）。
    pub fn subscription_rejected(
        &self,
        remote: &node_link_protocol::common::RemoteSessionRef,
    ) -> bool {
        lock(&self.state)
            .rejected
            .contains_key(remote.session_id.as_str())
    }

    /// 该会话的订阅拒绝原因（无则 `None`）。
    ///
    /// `resource.subscribe`/`resource.ack` 没有成功回复，因此拒绝是**异步**到达的：调用方在
    /// [`Self::subscribe`] 之后用本方法确认订阅是否已被 Owner 明确拒绝，避免「以为已订阅、事件却
    /// 永不投递」。不可重试（永久）拒绝与已安排退避重发的可重试拒绝都会在这里出现。
    pub fn subscription_error(
        &self,
        remote: &node_link_protocol::common::RemoteSessionRef,
    ) -> Option<ClientError> {
        let state = lock(&self.state);
        state
            .rejected
            .get(remote.session_id.as_str())
            .cloned()
            .or_else(|| {
                state
                    .faults
                    .get(remote.session_id.as_str())
                    .map(|fault| fault.error.clone())
            })
    }

    /// 可重试拒绝的完整事实（最近原因、已重发次数、是否仍在重试）；无则 `None`。
    ///
    /// 条目在该会话的第一条入站帧到达时清除：之前不能把订阅当作已建立。
    pub fn subscription_fault(
        &self,
        remote: &node_link_protocol::common::RemoteSessionRef,
    ) -> Option<SubscriptionFault> {
        lock(&self.state)
            .faults
            .get(remote.session_id.as_str())
            .cloned()
    }

    /// 标记一个会话的恢复订阅为不可用（连接保持；不再由恢复计划重试）。
    fn reject_subscription(
        &self,
        remote: &node_link_protocol::common::RemoteSessionRef,
        error: ClientError,
    ) {
        lock(&self.state)
            .rejected
            .insert(remote.session_id.as_str().to_owned(), error);
        lock(&self.state).faults.remove(remote.session_id.as_str());
    }

    /// 记录一次可重试拒绝（可观测，且不是永久结论）。
    fn note_subscription_fault(
        &self,
        remote: &node_link_protocol::common::RemoteSessionRef,
        error: ClientError,
        attempts: u32,
        retrying: bool,
    ) {
        lock(&self.state).faults.insert(
            remote.session_id.as_str().to_owned(),
            SubscriptionFault {
                error,
                attempts,
                retrying,
            },
        );
    }

    /// 该会话的订阅已由入站帧证明工作：清除可重试拒绝记录。
    fn clear_subscription_fault(&self, remote: &node_link_protocol::common::RemoteSessionRef) {
        lock(&self.state).faults.remove(remote.session_id.as_str());
    }

    /// 成功的 `resource.attached` 清除永久拒绝记录：Owner 重新导出了会话时恢复计划应再次尝试。
    ///
    /// [`Self::subscription_fault`] 不在这里清除：attach 成功只证明会话可见，订阅仍可能被拒；
    /// 它由该会话的第一条入站帧清除。
    fn clear_subscription_rejection(&self, remote: &node_link_protocol::common::RemoteSessionRef) {
        lock(&self.state)
            .rejected
            .remove(remote.session_id.as_str());
    }

    /// 丢弃过期代际的 ticket：后续 `subscribe`/`attach` 必须重新申请，而不是复用必然被拒的票证。
    fn remove_attachment(&self, remote: &node_link_protocol::common::RemoteSessionRef) {
        lock(&self.state)
            .attachments
            .remove(remote.session_id.as_str());
    }

    /// 停止本连接（幂等）。
    pub fn stop(&self) {
        let _ = self.shutdown.send(true);
        let _ = self.outgoing.try_send(Outbound::Close);
    }

    /// 本链接关闭信号的订阅端（附件连接用它中止在途传输）。
    pub fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    /// 发送一条命令并等待其完成条件。
    pub async fn submit(
        &self,
        submit: CommandSubmit,
        completion: Completion,
    ) -> Result<CommandOutcome, ClientError> {
        if !self.is_online() {
            return Err(ClientError::OwnerOffline);
        }
        let request = submit.request_id.clone();
        let command = submit.command;
        let session = submit.session_ref.as_ref().cloned();
        // 只读查询（`command.status` 等）不会产生副作用：连接丢失时让调用方自行重试，
        // 不把它们登记成「不确定命令」，否则重连恢复会拿查询 id 去查自己。
        let may_have_side_effects = command.accepts_side_effects();
        let (reply, received) = oneshot::channel();
        self.outgoing
            .send(Outbound::Command {
                submit,
                completion,
                reply,
            })
            .await
            .map_err(|_| ClientError::OwnerOffline)?;
        match tokio::time::timeout(COMMAND_WAIT_TIMEOUT, received).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                if may_have_side_effects {
                    self.note_uncertain(&command, session.as_ref(), &request);
                }
                Err(ClientError::Uncertain)
            }
            Err(_) => {
                if may_have_side_effects {
                    self.note_uncertain(&command, session.as_ref(), &request);
                }
                Err(ClientError::Timeout)
            }
        }
    }

    /// 申请一条会话的 attachment（每个 remoteSessionRef 每次重连重新申请）。
    pub async fn attach(&self, remote: RemoteSessionRef) -> Result<AttachmentTicket, ClientError> {
        if !self.is_online() {
            return Err(ClientError::OwnerOffline);
        }
        if let Some(ticket) = self.attachment(&remote) {
            return Ok(ticket);
        }
        let (reply, received) = oneshot::channel();
        self.outgoing
            .send(Outbound::Attach {
                remote: remote.clone(),
                reply,
            })
            .await
            .map_err(|_| ClientError::OwnerOffline)?;
        match tokio::time::timeout(COMMAND_WAIT_TIMEOUT, received).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(ClientError::Closed),
            Err(_) => Err(ClientError::Timeout),
        }
    }

    /// 订阅会话事件（`cursor = None` 表示先取快照，再由快照结束点续读）。
    ///
    /// `Ok(())` 表示订阅帧已交给传输层；订阅**成功**在协议上没有回复。Owner 的请求级拒绝会让
    /// 该会话进入 [`Self::subscription_error`]：不可重试的拒绝使本调用立即失败（不再排队等一个
    /// 必然被拒的请求）；可重试的拒绝会在链路内按退避自动重发，调用方仍可从
    /// [`Self::subscription_fault`] 观测并显式重试。
    ///
    /// 链路离线时立即返回 [`ClientError::OwnerOffline`]：离线期间发件箱无人消费，无超时地等待
    /// 回复会让调用方永久挂起。帧已入队但链路在回复前消失时以 [`ClientError::Timeout`] 结束
    /// （与 [`Self::attach`] 同一口径）。
    pub async fn subscribe(
        &self,
        ticket: &AttachmentTicket,
        cursor: Option<OriginCursor>,
    ) -> Result<(), ClientError> {
        if !self.is_online() {
            return Err(ClientError::OwnerOffline);
        }
        if self.subscription_rejected(&ticket.remote) {
            if let Some(error) = self.subscription_error(&ticket.remote) {
                return Err(error);
            }
        }
        let (reply, received) = oneshot::channel();
        self.outgoing
            .send(Outbound::Subscribe {
                remote: ticket.remote.clone(),
                attachment: ticket.id.clone(),
                generation: ticket.generation.clone(),
                cursor,
                reply,
            })
            .await
            .map_err(|_| ClientError::OwnerOffline)?;
        match tokio::time::timeout(COMMAND_WAIT_TIMEOUT, received).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(ClientError::Closed),
            Err(_) => Err(ClientError::Timeout),
        }
    }

    /// 发送累计 ACK（调用方必须**先**提交收据）。
    pub async fn ack(
        &self,
        session: RemoteSessionRef,
        cursor: OriginCursor,
    ) -> Result<(), ClientError> {
        self.outgoing
            .send(Outbound::Ack { session, cursor })
            .await
            .map_err(|_| ClientError::OwnerOffline)
    }

    fn set_online(&self, established: Option<&Established>) {
        let mut state = lock(&self.state);
        state.snapshot = match established {
            Some(established) => LinkSnapshot {
                online: true,
                connection_id: Some(established.connection_id.clone()),
                selected_features: established.selected_features.clone(),
                limits: Some(established.limits),
                server_epoch: Some(established.server_epoch.clone()),
                catalog_revision: Some(established.catalog_revision.clone()),
            },
            None => LinkSnapshot::default(),
        };
        if established.is_none() {
            state.attachments.clear();
        }
    }

    fn put_attachment(&self, ticket: AttachmentTicket) {
        lock(&self.state)
            .attachments
            .insert(ticket.remote.session_id.as_str().to_owned(), ticket);
    }

    /// 登记一个不确定命令（供 supervisor 在断链时统一记录）。
    fn note_uncertain(
        &self,
        command: &CommandName,
        session: Option<&RemoteSessionRef>,
        request: &Uuid,
    ) {
        lock(&self.state).uncertain.insert(
            request.as_str().to_owned(),
            UncertainCommand {
                request_id: request.clone(),
                command: *command,
                session: session.cloned(),
            },
        );
    }
}

/// 连接结束原因。
#[derive(Debug)]
pub enum LinkEnd {
    /// 本机要求关闭。
    Shutdown,
    /// 对端正常关闭或连接断开。
    Closed,
    /// 对端撤回信任（`node.trust.revoked`）。
    Revoked,
    /// 传输/协议失败。
    Failed(ClientError),
}

/// 一条连接的运行器。
pub struct Link {
    shared: Arc<SharedLink>,
    config: HandshakeConfig,
    runtime: Arc<RemoteRuntime>,
    generation: acp_core::ports::RemoteGeneration,
    heartbeat: Duration,
    silence_timeout: Duration,
}

impl Link {
    /// 装配。`shared` 的 `outgoing` 必须由本运行器消费。
    pub fn new(
        shared: Arc<SharedLink>,
        config: HandshakeConfig,
        runtime: Arc<RemoteRuntime>,
        generation: acp_core::ports::RemoteGeneration,
        heartbeat: Duration,
        silence_timeout: Duration,
    ) -> Self {
        Self {
            shared,
            config,
            runtime,
            generation,
            heartbeat,
            silence_timeout,
        }
    }

    /// 编码并发送 `resource.attach`；成功返回该帧的 `messageId`（调用方登记等待项）。
    async fn send_attach(
        sink: &mut dyn MessageSink,
        established: &Established,
        next_sequence: &mut u64,
        remote: &RemoteSessionRef,
    ) -> Result<Uuid, SendFailure> {
        let body = encode_body(&Attach {
            remote_session_ref: remote.clone(),
        })
        .map_err(|_| SendFailure::Encode(ClientError::Local("attach body cannot be encoded")))?;
        let (text, message_id) = encode_post_auth_with_message_id(
            MessageType::ResourceAttach,
            established,
            *next_sequence,
            body,
        )
        .map_err(SendFailure::Encode)?;
        sink.send(text).await.map_err(SendFailure::Transport)?;
        *next_sequence += 1;
        Ok(message_id)
    }

    /// 编码并发送 `resource.subscribe`，并登记拒绝关联键与重试上下文。
    #[allow(clippy::too_many_arguments)] // 每个参数都是帧或等待队列的一部分
    async fn send_subscribe(
        sink: &mut dyn MessageSink,
        established: &Established,
        next_sequence: &mut u64,
        tracked: &mut VecDeque<ResourceRequest>,
        remote: RemoteSessionRef,
        attachment: Uuid,
        generation: DecimalString,
        cursor: Option<OriginCursor>,
        attempts: u32,
        snapshot_queue: &mut VecDeque<RemoteSessionRef>,
    ) -> Result<(), SendFailure> {
        snapshot_queue.push_back(remote.clone());
        let body = Subscribe {
            attachment_id: attachment,
            attachment_generation: generation,
            cursor: Nullable::from_option(cursor.clone()),
        };
        let body = encode_body(&body).map_err(|_| {
            SendFailure::Encode(ClientError::Local(
                "the resource.subscribe body cannot be encoded",
            ))
        })?;
        let (text, message_id) = encode_post_auth_with_message_id(
            MessageType::ResourceSubscribe,
            established,
            *next_sequence,
            body,
        )
        .map_err(SendFailure::Encode)?;
        sink.send(text).await.map_err(SendFailure::Transport)?;
        *next_sequence += 1;
        track_resource_request(
            tracked,
            message_id,
            remote,
            ResourceKind::Subscribe,
            Some(Resubscribe { cursor, attempts }),
        );
        Ok(())
    }

    /// 驱动连接直到结束。返回后 `shared` 标记为离线，未完成命令登记为不确定。
    pub(crate) async fn run(
        self,
        mut sink: Box<dyn MessageSink>,
        mut source: Box<dyn MessageSource>,
        outgoing: &mut mpsc::Receiver<Outbound>,
        shutdown: &mut watch::Receiver<bool>,
        startup: Vec<ResumeSession>,
    ) -> LinkEnd {
        let established = match tokio::time::timeout(
            crate::handshake::HANDSHAKE_TIMEOUT,
            handshake(&mut *sink, &mut *source, &self.config),
        )
        .await
        {
            Ok(Ok(established)) => established,
            Ok(Err(error)) => {
                self.shared.set_online(None);
                return LinkEnd::Failed(error);
            }
            Err(_) => {
                self.shared.set_online(None);
                return LinkEnd::Failed(ClientError::Timeout);
            }
        };
        self.shared.set_online(Some(&established));
        self.runtime.online(
            &self.shared.owner().clone(),
            self.generation,
            Arc::clone(&self.shared),
        );

        let owner = self.shared.owner().clone();
        let mut next_sequence = established.next_sequence;
        let mut pending: Vec<Pending> = Vec::new();
        let mut attach_queue: VecDeque<AttachRequest> = VecDeque::new();
        let mut resource_requests: VecDeque<ResourceRequest> = VecDeque::new();
        // 每个 resource.subscribe 对应一个快照；快照消息不带 sessionRef，因此按发送顺序关联。
        let mut snapshot_queue: VecDeque<RemoteSessionRef> = VecDeque::new();
        let mut snapshot_current: Option<RemoteSessionRef> = None;

        // 重连后的安全恢复（只读）：catalog 订阅 + 每会话 attach/subscribe + 未确定命令的
        // `command.status` 查询。任何副作用命令都**不**在这里重发。
        let catalog = node_link_protocol::catalog::CatalogSubscribe {
            known_revision: Nullable::null(),
        };
        match encode_body(&catalog) {
            Ok(body) => match encode_post_auth(
                MessageType::CatalogSubscribe,
                &established,
                next_sequence,
                body,
            ) {
                Ok(text) => {
                    if sink.send(text).await.is_err() {
                        self.shared.set_online(None);
                        return LinkEnd::Closed;
                    }
                    next_sequence += 1;
                }
                Err(error) => {
                    self.shared.set_online(None);
                    return LinkEnd::Failed(error);
                }
            },
            Err(_) => {
                self.shared.set_online(None);
                return LinkEnd::Failed(ClientError::Local("catalog body cannot be encoded"));
            }
        }
        for resume in startup {
            // 已被明确拒绝（不可重试）的会话不再进入恢复计划：每次重连重复 attach 同一个不可能成功的
            // 请求会把一次请求级拒绝放大成重连循环。
            if self.shared.subscription_rejected(&resume.remote) {
                tracing::debug!(
                    event = "node_link_client.subscription_skipped",
                    owner = %owner,
                    session_id = resume.remote.session_id.as_str(),
                    "the owner rejected this subscription permanently; the reconnect plan skips it"
                );
                continue;
            }
            match Self::send_attach(&mut *sink, &established, &mut next_sequence, &resume.remote)
                .await
            {
                Ok(message_id) => attach_queue.push_back(AttachRequest {
                    remote: resume.remote,
                    message_id,
                    subscribe: Some(resume.cursor),
                    subscribe_attempts: 0,
                    reply: None,
                }),
                Err(SendFailure::Encode(error)) => {
                    self.shared.set_online(None);
                    return LinkEnd::Failed(error);
                }
                Err(SendFailure::Transport(_)) => {
                    self.shared.set_online(None);
                    return LinkEnd::Closed;
                }
            }
        }
        let uncertain = self.shared.uncertain();
        for entry in uncertain {
            let query = match status_submit(
                Uuid::parse(&RandomUuid::new_v4().to_string()).unwrap_or_else(|_| {
                    Uuid::parse("00000000-0000-0000-0000-000000000000")
                        .expect("nil uuid is canonical")
                }),
                entry.request_id.clone(),
            ) {
                Ok(query) => query,
                Err(_) => continue,
            };
            let body = match encode_body(&query) {
                Ok(body) => body,
                Err(_) => continue,
            };
            let (text, message_id) = match encode_post_auth_with_message_id(
                MessageType::CommandSubmit,
                &established,
                next_sequence,
                body,
            ) {
                Ok(encoded) => encoded,
                Err(error) => {
                    self.shared.set_online(None);
                    return LinkEnd::Failed(error);
                }
            };
            if sink.send(text).await.is_err() {
                self.shared.set_online(None);
                return LinkEnd::Closed;
            }
            next_sequence += 1;
            // 关联键是**目标** requestId：Owner 的 accepted/terminal 回复使用目标命令的记录，
            // 而不是这条查询自己的（一次性）requestId。
            pending.push(Pending {
                command: CommandName::CommandStatus,
                completion: Completion::Terminal,
                reply: None,
                session: entry.session,
                uncertain_target: Some(entry.request_id.clone()),
                wire_request: entry.request_id,
                message_id: Some(message_id),
                status_query: true,
            });
        }
        let mut last_received = Instant::now();
        let mut pending_ping: Option<(Instant, [u8; 16])> = None;
        // 可重试拒绝后的订阅重发计划（每个会话最多一条，见 [`schedule_subscription_retry`]）。
        let mut retries: Vec<SubscriptionRetry> = Vec::new();
        let mut ticker =
            tokio::time::interval_at(tokio::time::Instant::now() + self.heartbeat, self.heartbeat);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let end = 'link: loop {
            let next_retry = retries.iter().map(|retry| retry.at).min();
            tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    if changed.is_ok() && !*shutdown.borrow() {
                        continue;
                    }
                    let _ = sink.close().await;
                    break LinkEnd::Shutdown;
                }
                request = outgoing.recv() => {
                    let Some(request) = request else {
                        let _ = sink.close().await;
                        break LinkEnd::Shutdown;
                    };
                    match request {
                        Outbound::Close => {
                            let _ = sink.close().await;
                            break LinkEnd::Shutdown;
                        }
                        Outbound::Command { submit, completion, reply } => {
                            if pending.len() >= MAX_IN_FLIGHT_COMMANDS {
                                // 连接期上限：显式拒绝，不排队到无界（服务端也会拒绝超过 limits 的并发）。
                                let _ = reply.send(Err(ClientError::Protocol(
                                    "too many in-flight commands on this connection".to_owned(),
                                )));
                                continue;
                            }
                            let command = submit.command;
                            let session = submit.session_ref.as_ref().cloned();
                            let wire_request = correlation_request(&submit);
                            let status_query = matches!(command, CommandName::CommandStatus);
                            let body = match encode_body(&submit) {
                                Ok(body) => body,
                                Err(_) => {
                                    let _ = reply.send(Err(ClientError::Local(
                                        "command body cannot be encoded",
                                    )));
                                    continue;
                                }
                            };
                            let (text, message_id) = match encode_post_auth_with_message_id(
                                MessageType::CommandSubmit,
                                &established,
                                next_sequence,
                                body,
                            ) {
                                Ok(encoded) => encoded,
                                Err(error) => {
                                    let _ = reply.send(Err(error));
                                    continue;
                                }
                            };
                            if let Err(error) = sink.send(text).await {
                                let _ = reply.send(Err(error));
                                break LinkEnd::Closed;
                            }
                            next_sequence += 1;
                            pending.push(Pending {
                                command,
                                completion,
                                reply: Some(reply),
                                session,
                                uncertain_target: None,
                                wire_request,
                                message_id: Some(message_id),
                                status_query,
                            });
                        }
                        Outbound::Attach { remote, reply } => {
                            // 调用方显式 attach 接管该会话：取消待发的自动重发，避免重复订阅帧。
                            cancel_subscription_retry(&mut retries, &remote);
                            match Self::send_attach(
                                &mut *sink,
                                &established,
                                &mut next_sequence,
                                &remote,
                            )
                            .await
                            {
                                Ok(message_id) => attach_queue.push_back(AttachRequest {
                                    remote,
                                    message_id,
                                    subscribe: None,
                                    subscribe_attempts: 0,
                                    reply: Some(reply),
                                }),
                                Err(SendFailure::Encode(error)) => {
                                    let _ = reply.send(Err(error));
                                }
                                Err(SendFailure::Transport(error)) => {
                                    let _ = reply.send(Err(error));
                                    break LinkEnd::Closed;
                                }
                            }
                        }
                        Outbound::Subscribe { remote, attachment, generation, cursor, reply } => {
                            // 显式订阅是调用方接管：取消退避中的自动重发（本次请求重置重试预算）。
                            cancel_subscription_retry(&mut retries, &remote);
                            match Self::send_subscribe(
                                &mut *sink,
                                &established,
                                &mut next_sequence,
                                &mut resource_requests,
                                remote,
                                attachment,
                                generation,
                                cursor,
                                0,
                                &mut snapshot_queue,
                            )
                            .await
                            {
                                Ok(()) => {
                                    let _ = reply.send(Ok(()));
                                }
                                Err(SendFailure::Encode(error)) => {
                                    let _ = reply.send(Err(error));
                                }
                                Err(SendFailure::Transport(error)) => {
                                    let _ = reply.send(Err(error));
                                    break LinkEnd::Closed;
                                }
                            }
                        }
                        Outbound::Ack { session, cursor } => {
                            let body = Ack { session_ref: session.clone(), cursor };
                            match encode_body(&body) {
                                Ok(body) => match encode_post_auth_with_message_id(
                                    MessageType::ResourceAck,
                                    &established,
                                    next_sequence,
                                    body,
                                ) {
                                    Ok((text, message_id)) => {
                                        if sink.send(text).await.is_err() {
                                            break LinkEnd::Closed;
                                        }
                                        next_sequence += 1;
                                        track_resource_request(
                                            &mut resource_requests,
                                            message_id,
                                            session,
                                            ResourceKind::Ack,
                                            None,
                                        );
                                    }
                                    Err(_) => tracing::warn!(
                                        event = "node_link_client.ack_encode_failed",
                                        owner = %owner,
                                        "the resource.ack body cannot be encoded"
                                    ),
                                },
                                Err(_) => tracing::warn!(
                                    event = "node_link_client.ack_encode_failed",
                                    owner = %owner,
                                    "the resource.ack body cannot be encoded"
                                ),
                            }
                        }
                    }
                }
                received = source.recv() => {
                    last_received = Instant::now();
                    let incoming = match received {
                        Ok(incoming) => incoming,
                        Err(error) => break LinkEnd::Failed(error),
                    };
                    let text = match incoming {
                        Incoming::Text(text) => text,
                        Incoming::Closed(code) => {
                            tracing::debug!(
                                event = "node_link_client.connection_closed",
                                owner = %owner,
                                close_code = code,
                                "the owner closed the node-link connection"
                            );
                            break LinkEnd::Closed;
                        }
                    };
                    let envelope = match Envelope::decode_in_phase(&text, Phase::PostAuth) {
                        Ok(envelope) => envelope,
                        Err(error) => {
                            break LinkEnd::Failed(ClientError::Protocol(error.to_string()));
                        }
                    };
                    // 认证后每一条入站消息都必须带同一 connectionId；序号由服务端维护，本端只校验存在性。
                    let Some(fields) = envelope.connection() else {
                        break LinkEnd::Failed(ClientError::Protocol(
                            "an authenticated message is missing connection fields".to_owned(),
                        ));
                    };
                    if fields.connection_id != established.connection_id {
                        break LinkEnd::Failed(ClientError::Protocol(
                            "the owner used a different connection id".to_owned(),
                        ));
                    }
                    match envelope.message_type() {
                        MessageType::CatalogSnapshot => {
                            match decode_body::<CatalogSnapshot>(&envelope) {
                                Ok(snapshot) => {
                                    if let Err(error) = self.runtime.apply_catalog(
                                        &owner,
                                        self.generation,
                                        &snapshot,
                                    ) {
                                        tracing::warn!(
                                            event = "node_link_client.catalog_apply_failed",
                                            owner = %owner,
                                            error = ?error,
                                            "the catalog snapshot cannot be applied to the runtime directory"
                                        );
                                    }
                                }
                                Err(error) => break LinkEnd::Failed(error),
                            }
                        }
                        MessageType::ResourceAttached => {
                            let body: AttachedBody = match decode_body(&envelope) {
                                Ok(body) => body,
                                Err(error) => break LinkEnd::Failed(error),
                            };
                            match attach_queue.pop_front() {
                                Some(request) => {
                                    let ticket = AttachmentTicket {
                                        id: body.attachment_id,
                                        generation: body.attachment_generation,
                                        remote: request.remote.clone(),
                                        meta: body.session_meta,
                                    };
                                    // 成功 attach 说明 Owner 现在允许该会话：清除先前的不可用标记，
                                    // 让后续重连的恢复计划重新包含它。
                                    self.shared.clear_subscription_rejection(&ticket.remote);
                                    self.shared.put_attachment(ticket.clone());
                                    if let Some(cursor) = request.subscribe {
                                        match Self::send_subscribe(
                                            &mut *sink,
                                            &established,
                                            &mut next_sequence,
                                            &mut resource_requests,
                                            ticket.remote.clone(),
                                            ticket.id.clone(),
                                            ticket.generation.clone(),
                                            cursor,
                                            request.subscribe_attempts,
                                            &mut snapshot_queue,
                                        )
                                        .await
                                        {
                                            Ok(()) => {}
                                            Err(SendFailure::Encode(error)) => tracing::warn!(
                                                event = "node_link_client.subscribe_encode_failed",
                                                owner = %owner,
                                                error = %error,
                                                "the resource.subscribe body cannot be encoded"
                                            ),
                                            Err(SendFailure::Transport(_)) => {
                                                break LinkEnd::Closed;
                                            }
                                        }
                                    }
                                    if let Some(reply) = request.reply {
                                        let _ = reply.send(Ok(ticket));
                                    }
                                }
                                None => {
                                    tracing::warn!(
                                        event = "node_link_client.unexpected_attached",
                                        owner = %owner,
                                        "resource.attached arrived without a matching attach request"
                                    );
                                }
                            }
                        }
                        MessageType::ResourceSubscribe => {
                            // `resource.subscribe` 是单向的（服务端直接发快照/重放）。
                        }
                        MessageType::ResourceSnapshotBegin => {
                            snapshot_current = snapshot_queue.pop_front();
                        }
                        MessageType::ResourceSnapshotChunk => {
                            let body: node_link_protocol::resource::SnapshotChunk = match decode_body(&envelope) {
                                Ok(body) => body,
                                Err(error) => break LinkEnd::Failed(error),
                            };
                            if body.resource() == SnapshotResource::SessionMeta {
                                if let SnapshotItems::SessionMeta(items) = body.items() {
                                    for item in items {
                                        // 快照 chunk 到达同样证明该会话的订阅已工作。
                                        self.shared
                                            .clear_subscription_fault(&item.session_ref);
                                        cancel_subscription_retry(&mut retries, &item.session_ref);
                                        if let Err(error) = self.runtime.apply_session_meta(
                                            &owner,
                                            self.generation,
                                            &item.session_ref,
                                            &item.session_meta,
                                        ) {
                                            tracing::warn!(
                                                event = "node_link_client.session_meta_failed",
                                                owner = %owner,
                                                error = ?error,
                                                "a snapshot session_meta item cannot be applied"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        MessageType::ResourceSnapshotEnd => {
                            let body: node_link_protocol::resource::SnapshotEnd =
                                match decode_body(&envelope) {
                                    Ok(body) => body,
                                    Err(error) => break LinkEnd::Failed(error),
                                };
                            if let Some(remote) = snapshot_current.take() {
                                self.runtime.apply_snapshot_end(&owner, &remote, &body.cursor);
                            }
                        }
                        MessageType::ResourceCaughtUp => {
                            let body: node_link_protocol::resource::CaughtUp =
                                match decode_body(&envelope) {
                                    Ok(body) => body,
                                    Err(error) => break LinkEnd::Failed(error),
                                };
                            if let Some(remote) = self.shared.session_by_attachment(&body.attachment_id) {
                                self.runtime.apply_caught_up(&owner, &remote, &body);
                            }
                        }
                        MessageType::ResourceResetRequired => {
                            let body: node_link_protocol::resource::ResetRequired =
                                match decode_body(&envelope) {
                                    Ok(body) => body,
                                    Err(error) => break LinkEnd::Failed(error),
                                };
                            if let Some(remote) = self.shared.session_by_attachment(&body.attachment_id) {
                                self.runtime.apply_reset(&owner, &remote, &body);
                            }
                        }
                        MessageType::ResourceUnavailable => {
                            let body: node_link_protocol::resource::Unavailable =
                                match decode_body(&envelope) {
                                    Ok(body) => body,
                                    Err(error) => break LinkEnd::Failed(error),
                                };
                            let remote = body
                                .attachment_id
                                .as_ref()
                                .and_then(|id| self.shared.session_by_attachment(id));
                            self.runtime
                                .apply_unavailable(&owner, remote.as_ref(), &body);
                        }
                        MessageType::ResourceEvent => {
                            let body: Event = match decode_body(&envelope) {
                                Ok(body) => body,
                                Err(error) => break LinkEnd::Failed(error),
                            };
                            // 实时事件只证明扇出在工作，**不**证明被拒绝的快照/重放已补齐：
                            // 保留同一 cursor 的重发计划，让 Owner 补缺口（重复事件按 originEventId 去重）。
                            // 故障记录由快照项或显式重新订阅清除，不能因一条实时事件消失。
                            match self.runtime.apply_event(&owner, self.generation, &body) {
                                Ok(EventDisposition::Forwarded(cursor)) => {
                                    // 运行期 ACK：只在 `resource.stateless-origin.v1` 协商后发送；
                                    // 它只表达「当前连接已交给有界下游」，不落盘、不是可恢复索引。
                                    if !self
                                        .shared
                                        .has_feature(crate::runtime::STATELESS_ORIGIN_FEATURE)
                                    {
                                        continue;
                                    }
                                    let Some(ack) = crate::runtime::wire_ack(&body.session_ref, &cursor)
                                    else {
                                        tracing::warn!(
                                            event = "node_link_client.ack_encode_failed",
                                            owner = %owner,
                                            "the resource.ack cursor cannot be encoded"
                                        );
                                        continue;
                                    };
                                    match encode_body(&ack) {
                                        Ok(ack_body) => {
                                            match encode_post_auth_with_message_id(
                                                MessageType::ResourceAck,
                                                &established,
                                                next_sequence,
                                                ack_body,
                                            ) {
                                                Ok((text, message_id)) => {
                                                    if sink.send(text).await.is_err() {
                                                        break LinkEnd::Closed;
                                                    }
                                                    next_sequence += 1;
                                                    track_resource_request(
                                                        &mut resource_requests,
                                                        message_id,
                                                        body.session_ref.clone(),
                                                        ResourceKind::Ack,
                                                        None,
                                                    );
                                                }
                                                Err(error) => break LinkEnd::Failed(error),
                                            }
                                        }
                                        Err(_) => tracing::warn!(
                                            event = "node_link_client.ack_encode_failed",
                                            owner = %owner,
                                            "the resource.ack body cannot be encoded"
                                        ),
                                    }
                                }
                                Ok(EventDisposition::Ignored) => {
                                    // 重复/过期事件：不转发、不 ACK。
                                }
                                Err(error) => {
                                    // 协议违规（跨 Owner/摘要不符/epoch 冲突）：不 ACK、关闭连接。
                                    tracing::warn!(
                                        event = "node_link_client.event_rejected",
                                        owner = %owner,
                                        event_type = body.event_type.as_str(),
                                        error = ?error,
                                        "a source event failed runtime validation"
                                    );
                                    break LinkEnd::Failed(ClientError::Protocol(
                                        "a source event failed runtime validation".to_owned(),
                                    ));
                                }
                            }
                        }
                        MessageType::CommandAccepted => {
                            let body: CommandAccepted = match decode_body(&envelope) {
                                Ok(body) => body,
                                Err(error) => break LinkEnd::Failed(error),
                            };
                            // `Completion::Accepted` 的普通命令与状态查询都在 `command.accepted`
                            // 结清；状态查询的 accepted 不清除不确定项（终态才是可证明的事实）。
                            let taken = take_pending(&mut pending, &body.request_id, |entry| {
                                entry.status_query || entry.completion == Completion::Accepted
                            });
                            for mut entry in taken {
                                if let Some(reply) = entry.reply.take() {
                                    let _ = reply.send(Ok(CommandOutcome::Accepted {
                                        accepted_at: body.accepted_at.clone(),
                                        result: body.result.as_ref().cloned(),
                                    }));
                                }
                            }
                        }
                        MessageType::CommandRejected => {
                            let body: CommandRejected = match decode_body(&envelope) {
                                Ok(body) => body,
                                Err(error) => break LinkEnd::Failed(error),
                            };
                            let taken = take_pending(&mut pending, &body.request_id, |_| true);
                            // 明确拒绝 = 可证明没有副作用（对目标 requestId 的既有登记同样收敛）。
                            self.shared.clear_uncertain(&body.request_id);
                            for mut entry in taken {
                                if let Some(target) = entry.uncertain_target.take() {
                                    self.shared.clear_uncertain(&target);
                                }
                                if let Some(reply) = entry.reply.take() {
                                    let _ = reply.send(Ok(CommandOutcome::Rejected(Box::new(
                                        body.error.clone(),
                                    ))));
                                }
                            }
                        }
                        MessageType::CommandTerminal => {
                            let body: CommandTerminal = match decode_body(&envelope) {
                                Ok(body) => body,
                                Err(error) => break LinkEnd::Failed(error),
                            };
                            let taken = take_pending(&mut pending, &body.request_id, |_| true);
                            // 终态是权威结论：即使查询 pending 已被 `command.accepted` 结清，
                            // 也要按目标 requestId 解除不确定登记。
                            self.shared.clear_uncertain(&body.request_id);
                            for mut entry in taken {
                                if let Some(target) = entry.uncertain_target.take() {
                                    // `terminal` 是权威终态（含 `uncertain`）：查询已经收敛，
                                    // 不再在每次重连后重复回查；`uncertain` 的语义已由结果传达。
                                    self.shared.clear_uncertain(&target);
                                }
                                if let Some(reply) = entry.reply.take() {
                                    let _ = reply.send(Ok(CommandOutcome::Terminal(Box::new(
                                        body.terminal.clone(),
                                    ))));
                                }
                            }
                        }
                        MessageType::LinkPing => {
                            if let Ok(ping) = decode_body::<Ping>(&envelope) {
                                let pong = Pong { nonce: ping.nonce };
                                match encode_body(&pong) {
                                    Ok(body) => {
                                        if let Ok(text) = encode_post_auth(
                                            MessageType::LinkPong,
                                            &established,
                                            next_sequence,
                                            body,
                                        ) {
                                            if sink.send(text).await.is_err() {
                                                break LinkEnd::Closed;
                                            }
                                            next_sequence += 1;
                                        }
                                    }
                                    Err(_) => tracing::warn!(
                                        event = "node_link_client.pong_failed",
                                        owner = %owner,
                                        "the link.pong body cannot be encoded"
                                    ),
                                }
                            }
                        }
                        MessageType::LinkPong => {
                            pending_ping = None;
                        }
                        MessageType::NodeTrustRevoked => {
                            if let Ok(body) = decode_body::<NodeTrustRevoked>(&envelope) {
                                tracing::info!(
                                    event = "node_link_client.trust_revoked",
                                    owner = %owner,
                                    revoked_node_id = body.revoked_node_id.as_str(),
                                    "the owner revoked this access node"
                                );
                                // 本地信任行必须落成撤销终态：只停 supervisor 会让重启后的
                                // `start_persisted` 把一个已被撤销的 Owner 当成 `paired` 重连。
                                let revoked_at =
                                    CoreTimestamp::new(body.revoked_at.as_str()).ok();
                                let _ = revoked_at;
                                if let Err(error) = self.runtime.revoked(&owner).await {
                                    tracing::warn!(
                                        event = "node_link_client.revoked_persist_failed",
                                        owner = %owner,
                                        error = ?error,
                                        "the local trust record could not be marked revoked"
                                    );
                                }
                            }
                            let _ = sink.close().await;
                            self.fail_pending(&mut pending, &self.shared, ClientError::Revoked);
                            self.shared.set_online(None);
                            return LinkEnd::Revoked;
                        }
                        MessageType::LinkError => {
                            let body: LinkErrorBody = match decode_body(&envelope) {
                                Ok(body) => body,
                                Err(_) => {
                                    let error = ClientError::Protocol(
                                        "the owner sent an unreadable link.error".to_owned(),
                                    );
                                    self.fail_pending(&mut pending, &self.shared, error.clone());
                                    self.shared.set_online(None);
                                    let _ = sink.close().await;
                                    return LinkEnd::Failed(error);
                                }
                            };
                            let error = link_error(&body);
                            if matches!(body.code, ErrorCode::AuthNodeRevoked) {
                                tracing::warn!(
                                    event = "node_link_client.link_error",
                                    owner = %owner,
                                    error = %error,
                                    "the owner revoked this access node"
                                );
                                if let Err(error) = self.runtime.revoked(&owner).await {
                                    tracing::warn!(
                                        event = "node_link_client.revoked_persist_failed",
                                        owner = %owner,
                                        error = ?error,
                                        "the local trust record could not be marked revoked"
                                    );
                                }
                                self.fail_pending(&mut pending, &self.shared, error.clone());
                                self.shared.set_online(None);
                                let _ = sink.close().await;
                                return LinkEnd::Failed(error);
                            }
                            // 有 `correlationId` 的 `link.error` 一律是**请求级**拒绝：Owner 的资源路由
                            // 用 `protocol.schema_invalid`/`protocol.sequence_invalid` 表达
                            // 「会话不可见/未导出/旧 epoch/旧 cursor」，并明确保持连接可用。只有无关联
                            // 键、且码本身属于连接/认证层的错误才拆除连接。
                            if body.correlation_id.as_ref().is_none()
                                && fatal_link_error(body.code)
                            {
                                tracing::warn!(
                                    event = "node_link_client.link_error",
                                    owner = %owner,
                                    error = %error,
                                    code = body.code.as_str(),
                                    "the owner rejected a message; the connection is no longer usable"
                                );
                                self.fail_pending(&mut pending, &self.shared, error.clone());
                                self.shared.set_online(None);
                                let _ = sink.close().await;
                                return LinkEnd::Failed(error);
                            }
                            // 可恢复的 `link.error`：结清对应请求（有 `correlationId` 时按 messageId，
                            // 没有时按 Owner 语义决定「唯一可归属」或显式失败），保留连接。
                            //
                            // 只有 `command.not_found`（Owner 不认识这条请求）与
                            // `resource.rate_limited`（admission 拒绝，命令从未执行）是「确定没有副作用」
                            // 的结论；它们清除普通命令的不确定登记。被限流的**查询**没有执行，
                            // 因此保留 `uncertain_target`，等下一次重连再回查。
                            let commands = self.settle_request_error(
                                &mut pending,
                                &error,
                                body.code,
                                body.correlation_id.as_ref(),
                            );
                            let resources = self.settle_resource_error(
                                &mut attach_queue,
                                &mut resource_requests,
                                &error,
                                body.code,
                                body.correlation_id.as_ref(),
                                body.retryable,
                                &mut retries,
                            );
                            if body.correlation_id.as_ref().is_some()
                                && commands + resources == 0
                            {
                                // 关联键指向的请求已不在窗口内（例如更早已被累计 ACK 取代）——记录而
                                // 不是静默丢弃；连接仍可用。
                                tracing::warn!(
                                    event = "node_link_client.link_error_unmatched",
                                    owner = %owner,
                                    error = %error,
                                    code = body.code.as_str(),
                                    "a correlated link.error did not match a tracked request"
                                );
                            }
                            tracing::warn!(
                                event = "node_link_client.link_error_recovered",
                                owner = %owner,
                                error = %error,
                                code = body.code.as_str(),
                                retryable = body.retryable,
                                commands,
                                resources,
                                "the owner reported a recoverable error; the connection stays open"
                            );
                        }
                        MessageType::NodeReady => {
                            break LinkEnd::Failed(ClientError::Protocol(
                                "the owner repeated node.ready on an authenticated connection".to_owned(),
                            ));
                        }
                        other => {
                            break LinkEnd::Failed(ClientError::Protocol(format!(
                                "unexpected node-link message type {other}"
                            )));
                        }
                    }
                }
                _ = tokio::time::sleep_until(next_retry.unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(86_400))), if next_retry.is_some() => {
                    // 到期项按顺序取走：每个会话同时最多一条（`schedule_subscription_retry` 保证）。
                    let now = tokio::time::Instant::now();
                    let mut due = Vec::new();
                    let mut index = 0;
                    while index < retries.len() {
                        if retries[index].at <= now {
                            due.push(retries.remove(index));
                        } else {
                            index += 1;
                        }
                    }
                    for retry in due {
                        // 票证可能已随 `attach_generation_stale` 被丢弃（或随断线清空）：先重新 attach，
                        // `resource.attached` 到达后再按**同一 cursor** 订阅。
                        let Some(ticket) = self.shared.attachment(&retry.remote) else {
                            match Self::send_attach(
                                &mut *sink,
                                &established,
                                &mut next_sequence,
                                &retry.remote,
                            )
                            .await
                            {
                                Ok(message_id) => attach_queue.push_back(AttachRequest {
                                    remote: retry.remote.clone(),
                                    message_id,
                                    subscribe: Some(retry.cursor.clone()),
                                    subscribe_attempts: retry.attempts,
                                    reply: None,
                                }),
                                Err(SendFailure::Encode(error)) => tracing::warn!(
                                    event = "node_link_client.subscription_retry_encode_failed",
                                    owner = %owner,
                                    error = %error,
                                    session_id = retry.remote.session_id.as_str(),
                                    "the subscription retry could not be encoded; it is dropped"
                                ),
                                Err(SendFailure::Transport(_)) => break 'link LinkEnd::Closed,
                            }
                            continue;
                        };
                        match Self::send_subscribe(
                            &mut *sink,
                            &established,
                            &mut next_sequence,
                            &mut resource_requests,
                            retry.remote.clone(),
                            ticket.id.clone(),
                            ticket.generation.clone(),
                            retry.cursor.clone(),
                            retry.attempts,
                            &mut snapshot_queue,
                        )
                        .await
                        {
                            Ok(()) => tracing::debug!(
                                event = "node_link_client.subscription_retry",
                                owner = %owner,
                                session_id = retry.remote.session_id.as_str(),
                                attempt = retry.attempts,
                                "re-subscribing after a retryable rejection"
                            ),
                            Err(SendFailure::Encode(error)) => tracing::warn!(
                                event = "node_link_client.subscription_retry_encode_failed",
                                owner = %owner,
                                error = %error,
                                session_id = retry.remote.session_id.as_str(),
                                "the subscription retry could not be encoded; it is dropped"
                            ),
                            Err(SendFailure::Transport(_)) => break 'link LinkEnd::Closed,
                        }
                    }
                }
                _ = ticker.tick() => {
                    if last_received.elapsed() > self.silence_timeout {
                        break LinkEnd::Failed(ClientError::Timeout);
                    }
                    if let Some((sent_at, _)) = pending_ping {
                        if sent_at.elapsed() > self.silence_timeout {
                            break LinkEnd::Failed(ClientError::Timeout);
                        }
                        continue;
                    }
                    let Ok(nonce) = random_bytes_16() else {
                        break LinkEnd::Failed(ClientError::Local("heartbeat nonce is unavailable"));
                    };
                    let ping = Ping {
                        nonce: match node_link_protocol::common::Base64Url::<16>::parse(
                            &base64url(&nonce),
                        ) {
                            Ok(value) => value,
                            Err(_) => break LinkEnd::Failed(ClientError::Local("heartbeat nonce cannot be encoded")),
                        },
                    };
                    match encode_body(&ping) {
                        Ok(body) => match encode_post_auth(
                            MessageType::LinkPing,
                            &established,
                            next_sequence,
                            body,
                        ) {
                            Ok(text) => {
                                if sink.send(text).await.is_err() {
                                    break LinkEnd::Closed;
                                }
                                next_sequence += 1;
                                pending_ping = Some((Instant::now(), nonce));
                            }
                            Err(error) => break LinkEnd::Failed(error),
                        },
                        Err(_) => break LinkEnd::Failed(ClientError::Local(
                            "heartbeat body cannot be encoded",
                        )),
                    }
                }
            }
        };

        // 连接结束：未完成命令一律不确定（即使是对端明确关闭，也无法知道副作用是否发生）。
        self.fail_pending(&mut pending, &self.shared, ClientError::Uncertain);
        while let Some(request) = attach_queue.pop_front() {
            if let Some(reply) = request.reply {
                let _ = reply.send(Err(ClientError::Closed));
            }
        }
        self.shared.set_online(None);
        end
    }

    /// 一条可恢复 `link.error` 的结清规则。
    ///
    /// - 有 `correlationId`：只结清 `messageId` 命中的请求（可能不止一条：同一请求的重试与状态
    ///   查询共享关联键），返回结清条数；
    /// - 无 `correlationId` 且只剩一条待处理请求：Owner 的结论只可能指向它，直接结清并返回 1；
    /// - 无 `correlationId` 且有多条待处理请求：无法唯一归属。**不**让调用方等 120 s 超时，而是
    ///   显式失败全部待处理请求（副作用命令同时登记 uncertain——无法证明它们没有执行）；返回 0。
    ///
    /// `CommandNotFound` 与 `ResourceRateLimited` 是「确定没有执行」的拒绝：普通命令的不确定登记
    /// （若存在同 `requestId` 的旧记录）随之清除；查询被限流只说明查询没执行，不能推断目标命令的
    /// 结果，因此保留 `uncertain_target`。
    fn settle_request_error(
        &self,
        pending: &mut Vec<Pending>,
        error: &ClientError,
        code: ErrorCode,
        correlation: Option<&Uuid>,
    ) -> usize {
        let definitely_not_executed = matches!(
            code,
            ErrorCode::CommandNotFound | ErrorCode::ResourceRateLimited
        );
        // 没有 `correlationId` 时，只有「Owner 语义上只可能指向一条命令」的错误码才可归属到
        // pending；资源/附件/连接级错误可能针对 catalog/attach 等非命令帧，不得拿去失败命令。
        if correlation.is_none() && !command_level_error(code) {
            return 0;
        }
        if let Some(correlation) = correlation {
            let mut settled = 0usize;
            let mut index = 0;
            while index < pending.len() {
                let matches = pending[index]
                    .message_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == correlation.as_str());
                if matches {
                    let mut entry = pending.remove(index);
                    if definitely_not_executed && !entry.status_query {
                        self.shared.clear_uncertain(&entry.wire_request);
                    }
                    if let Some(target) = entry.uncertain_target.take() {
                        // 查询被 `command.not_found` 拒绝 = Owner 没有该目标命令的记录；
                        // 查询被限流 = 查询本身没执行，目标仍不确定。
                        if matches!(code, ErrorCode::CommandNotFound) {
                            self.shared.clear_uncertain(&target);
                        }
                    }
                    if let Some(reply) = entry.reply.take() {
                        let _ = reply.send(Err(error.clone()));
                    }
                    settled += 1;
                } else {
                    index += 1;
                }
            }
            return settled;
        }
        match pending.len() {
            0 => 0,
            1 => {
                let mut entry = pending.remove(0);
                if definitely_not_executed && !entry.status_query {
                    self.shared.clear_uncertain(&entry.wire_request);
                }
                if entry.status_query
                    && matches!(code, ErrorCode::CommandNotFound)
                    && let Some(target) = entry.uncertain_target.take()
                {
                    self.shared.clear_uncertain(&target);
                }
                if let Some(reply) = entry.reply.take() {
                    let _ = reply.send(Err(error.clone()));
                }
                1
            }
            _ => {
                // 无法唯一归属：显式失败，绝不留下等满 `COMMAND_WAIT_TIMEOUT` 的孤立 pending。
                tracing::warn!(
                    event = "node_link_client.link_error_unattributed",
                    error = %error,
                    code = code.as_str(),
                    pending = pending.len(),
                    "the owner reported a request-level error without correlationId; all pending requests fail"
                );
                self.fail_pending(pending, &self.shared, error.clone());
                0
            }
        }
    }

    /// 结清与 `correlationId` 匹配的 attach/subscribe/ack 请求。
    ///
    /// Owner 的资源路由对 `resource.attach`/`resource.subscribe`/`resource.ack` 的协议拒绝都带
    /// `correlationId`（`resource.rs::send_error` 取请求信封的 `message_id`），而这些请求没有进
    /// `pending`（它不是命令）。不在这里匹配，attach 调用方要等满 [`COMMAND_WAIT_TIMEOUT`]，subscribe
    /// 的拒绝则完全静默。
    ///
    /// - 不可重试（`retryable = false`）的拒绝把该会话标为「恢复订阅不可用」，supervisor 的重连
    ///   计划不再重试同一个持久化订阅（已撤销导出/旧 epoch cursor 因此不会形成重连循环）；
    /// - 可重试（`retryable = true`）的拒绝进入 [`SharedLink::subscription_fault`]，并把该会话的
    ///   **同一 cursor** 重发计划追加到 `retries`（有界退避；同一会话不叠加）。订阅在收到该会话的
    ///   入站帧之前都不算已建立，因此缺口可见且能被补齐；
    /// - `attach_generation_stale` 丢弃过期 ticket，重发计划到期时改为重新 attach；
    /// - 有调用方的 attach 立即收到错误；恢复 attach（无调用方）同样按可重试/不可重试分流。
    #[allow(clippy::too_many_arguments)] // 每个参数都是被结清的请求表或重发计划
    fn settle_resource_error(
        &self,
        attach_queue: &mut VecDeque<AttachRequest>,
        tracked: &mut VecDeque<ResourceRequest>,
        error: &ClientError,
        code: ErrorCode,
        correlation: Option<&Uuid>,
        retryable: bool,
        retries: &mut Vec<SubscriptionRetry>,
    ) -> usize {
        let Some(correlation) = correlation else {
            return 0;
        };
        let mut settled = 0usize;
        let mut index = 0;
        while index < attach_queue.len() {
            if attach_queue[index].message_id.as_str() != correlation.as_str() {
                index += 1;
                continue;
            }
            let request = attach_queue.remove(index).expect("index is in range");
            tracing::warn!(
                event = "node_link_client.attach_rejected",
                session_id = request.remote.session_id.as_str(),
                code = code.as_str(),
                retryable,
                "the owner rejected resource.attach; the request is settled"
            );
            if !retryable {
                // 永久拒绝：取消该会话本轮连接内已安排的重发（重复请求只会再被拒一次）。
                cancel_subscription_retry(retries, &request.remote);
                self.shared
                    .reject_subscription(&request.remote, error.clone());
            } else if request.reply.is_none() {
                // 恢复计划里的 attach 没有调用方：可重试拒绝必须自己重发，否则该会话在本连接上
                // 静默缺席。代际过期时票证已被丢弃，到期项会重新走 attach → subscribe。
                if let Some(cursor) = request.subscribe.clone() {
                    let attempts = request.subscribe_attempts.saturating_add(1);
                    let retrying = attempts <= MAX_SUBSCRIPTION_RETRIES;
                    self.shared.note_subscription_fault(
                        &request.remote,
                        error.clone(),
                        attempts,
                        retrying,
                    );
                    if retrying {
                        schedule_subscription_retry(retries, &request.remote, cursor, attempts);
                    }
                }
            }
            if let Some(reply) = request.reply {
                let _ = reply.send(Err(error.clone()));
            }
            settled += 1;
        }
        let mut index = 0;
        while index < tracked.len() {
            if tracked[index].message_id.as_str() != correlation.as_str() {
                index += 1;
                continue;
            }
            let request = tracked.remove(index).expect("index is in range");
            tracing::warn!(
                event = "node_link_client.resource_rejected",
                resource = request.kind.as_str(),
                session_id = request.remote.session_id.as_str(),
                code = code.as_str(),
                retryable,
                "the owner rejected a resource request; the connection stays open"
            );
            if matches!(code, ErrorCode::ResourceAttachGenerationStale) {
                // 代际已过期：缓存票证不可再用，否则后续 subscribe/attach 会重复同一拒绝。
                self.shared.remove_attachment(&request.remote);
            }
            match request.resubscribe {
                Some(resubscribe) if retryable => {
                    let attempts = resubscribe.attempts.saturating_add(1);
                    let retrying = attempts <= MAX_SUBSCRIPTION_RETRIES;
                    self.shared.note_subscription_fault(
                        &request.remote,
                        error.clone(),
                        attempts,
                        retrying,
                    );
                    if retrying {
                        schedule_subscription_retry(
                            retries,
                            &request.remote,
                            resubscribe.cursor,
                            attempts,
                        );
                    } else {
                        // 预算耗尽：本连接不再重发；错误保持可观测，下一次重连的恢复计划会重试。
                        tracing::warn!(
                            event = "node_link_client.subscription_retry_exhausted",
                            session_id = request.remote.session_id.as_str(),
                            code = code.as_str(),
                            attempts,
                            "the subscription keeps being rejected; in-connection retries stop"
                        );
                    }
                }
                Some(_) => {
                    cancel_subscription_retry(retries, &request.remote);
                    self.shared
                        .reject_subscription(&request.remote, error.clone());
                }
                None => {
                    // `resource.ack` 的不可重试拒绝（旧 epoch/回退 cursor）：该会话的订阅需要
                    // 重新建立。可重试的 ACK 拒绝只影响 Owner 侧水位，事件会重复而不是缺失。
                    if !retryable {
                        cancel_subscription_retry(retries, &request.remote);
                        self.shared
                            .reject_subscription(&request.remote, error.clone());
                    }
                }
            }
            settled += 1;
        }
        settled
    }

    fn fail_pending(&self, pending: &mut Vec<Pending>, shared: &SharedLink, error: ClientError) {
        for entry in pending.iter_mut() {
            if let Some(reply) = entry.reply.take() {
                let _ = reply.send(Err(error.clone()));
            }
        }
        // 只有 mutation 需要登记「不确定」；查询命令可以安全重发，但本层仍不给它们自动重试。
        // 恢复查询本身（`uncertain_target`）不重复登记，它已经指向一个既有不确定项。
        for entry in pending.iter() {
            if entry.uncertain_target.is_some() {
                continue;
            }
            if entry.command.accepts_side_effects() {
                shared.note_uncertain(&entry.command, entry.session.as_ref(), &entry.wire_request);
            }
        }
        pending.clear();
    }
}

/// `CommandName` 是否可能产生副作用（`session.list`/`session.read`/`command.status` 等查询不会）。
trait SideEffects {
    fn accepts_side_effects(self) -> bool;
}

impl SideEffects for CommandName {
    fn accepts_side_effects(self) -> bool {
        !matches!(
            self,
            CommandName::SessionList
                | CommandName::SessionRead
                | CommandName::CommandStatus
                | CommandName::SessionModeList
                | CommandName::SessionConfigList
        )
    }
}

/// 登记一条非命令资源请求的关联键，并把窗口限制在 [`TRACKED_RESOURCE_REQUESTS`]。
///
/// 丢弃最旧条目是安全的：ACK 是累计水位，订阅在每次 attach 后重发；Owner 的拒绝紧随对应帧到达，
/// 超出窗口的旧请求已被更新的水位取代。
fn track_resource_request(
    tracked: &mut VecDeque<ResourceRequest>,
    message_id: Uuid,
    remote: RemoteSessionRef,
    kind: ResourceKind,
    resubscribe: Option<Resubscribe>,
) {
    if tracked.len() >= TRACKED_RESOURCE_REQUESTS {
        tracked.pop_front();
    }
    tracked.push_back(ResourceRequest {
        message_id,
        remote,
        kind,
        resubscribe,
    });
}

/// 移除所有与 `wire_request` 关联且满足 `take` 的 pending 项（保持其余项的相对顺序）。
///
/// 同一个关联键下可能同时有一条状态查询与一条在途命令（调用方用同一 `requestId` 重试），
/// 因此这里不是「一个键一条记录」的 map 语义。
fn take_pending(
    pending: &mut Vec<Pending>,
    wire_request: &Uuid,
    mut take: impl FnMut(&Pending) -> bool,
) -> Vec<Pending> {
    let mut taken = Vec::new();
    let mut index = 0;
    while index < pending.len() {
        if pending[index].wire_request.as_str() == wire_request.as_str() && take(&pending[index]) {
            taken.push(pending.remove(index));
        } else {
            index += 1;
        }
    }
    taken
}

/// Owner 在回复里使用的关联键：`command.status` 用目标 `requestId`，其余命令用自身的 `requestId`。
fn correlation_request(submit: &CommandSubmit) -> Uuid {
    match &submit.payload {
        CommandPayload::CommandStatus(body) => body.target_request_id.clone(),
        _ => submit.request_id.clone(),
    }
}

/// 无 `correlationId` 时，`link.error.code` 是否只可能指向一条命令请求。
///
/// 资源/附件/连接级错误可能针对 `catalog.subscribe`、`resource.attach` 等没有 pending 记录的帧；
/// 只有下列命令族错误能在缺失关联键时仍按「Owner 对某条命令的结论」处理。
fn command_level_error(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::CommandUnsupported
            | ErrorCode::CommandNotFound
            | ErrorCode::CommandIdempotencyConflict
            | ErrorCode::CommandUnsupportedField
            | ErrorCode::CommandUncertain
            | ErrorCode::ResourceRateLimited
            | ErrorCode::InteractionUnavailable
            | ErrorCode::InteractionVersionConflict
    )
}

/// 无 `correlationId` 的 `link.error` 是否会破坏整条连接（协议/认证层）。
///
/// 命令、资源与附件族错误是请求级的：Owner 在回复里带上 `correlationId`（请求信封的 `messageId`），
/// 调用方据此结清对应请求，连接继续可用。因此调用方**只在缺少关联键时**使用本判定；带有
/// `correlationId` 的 `protocol.schema_invalid`/`protocol.sequence_invalid` 是资源路由的常规拒绝
/// （会话不可见、未导出、旧 epoch/cursor），绝不能升级成拆连接。
fn fatal_link_error(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::ProtocolInvalidJson
            | ErrorCode::ProtocolSchemaInvalid
            | ErrorCode::ProtocolMessageTooLarge
            | ErrorCode::ProtocolVersionUnsupported
            | ErrorCode::ProtocolFeatureRequired
            | ErrorCode::ProtocolTypeUnsupported
            | ErrorCode::ProtocolSequenceInvalid
            | ErrorCode::AuthProofInvalid
            | ErrorCode::AuthNodeUnknown
            | ErrorCode::AuthCatalogRevisionInvalid
    )
}

fn encode_post_auth(
    message_type: MessageType,
    established: &Established,
    sequence: u64,
    body: Box<serde_json::value::RawValue>,
) -> Result<String, ClientError> {
    encode_post_auth_with_message_id(message_type, established, sequence, body)
        .map(|(text, _)| text)
}

/// 与 [`encode_post_auth`] 相同，但把 envelope 的 `messageId` 一并返回：
/// `link.error.correlationId` 回指的就是它（服务端的 `send_error` 取请求信封的 `messageId`）。
fn encode_post_auth_with_message_id(
    message_type: MessageType,
    established: &Established,
    sequence: u64,
    body: Box<serde_json::value::RawValue>,
) -> Result<(String, Uuid), ClientError> {
    let connection = wire_connection_fields(&established.connection_id, sequence)?;
    let message_id = Uuid::parse(&RandomUuid::new_v4().to_string())
        .map_err(|_| ClientError::Local("message id cannot be encoded"))?;
    let envelope = Envelope::new(message_type, message_id.clone(), Some(connection), body)
        .map_err(|_| ClientError::Local("message envelope cannot be assembled"))?;
    let text = envelope
        .encode()
        .map_err(|_| ClientError::Local("message envelope cannot be encoded"))?;
    Ok((text, message_id))
}

fn decode_body<T: serde::de::DeserializeOwned>(envelope: &Envelope) -> Result<T, ClientError> {
    serde_json::from_str(envelope.body().get()).map_err(|_| {
        ClientError::Protocol("the message body does not match the v1 schema".to_owned())
    })
}

fn link_error(body: &LinkErrorBody) -> ClientError {
    match body.code {
        ErrorCode::AuthNodeRevoked => ClientError::Revoked,
        ErrorCode::ProtocolTypeUnsupported | ErrorCode::CommandUnsupported => {
            ClientError::Unsupported
        }
        _ => ClientError::Protocol(format!("{}: {}", body.code.as_str(), body.message.as_str())),
    }
}

fn base64url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 由 supervisor 构造共享句柄与出站队列。
pub(crate) fn shared_channel(
    owner: NodeId,
) -> (
    Arc<SharedLink>,
    mpsc::Receiver<Outbound>,
    watch::Receiver<bool>,
) {
    let (outgoing, receiver) = mpsc::channel(64);
    let (shutdown, shutdown_rx) = watch::channel(false);
    (
        Arc::new(SharedLink::new(owner, outgoing, shutdown)),
        receiver,
        shutdown_rx,
    )
}

/// 出站队列的构造（供 `manager` 重新建链接时使用同一形状）。
pub(crate) fn channel(
    owner: NodeId,
) -> (
    Arc<SharedLink>,
    mpsc::Receiver<Outbound>,
    watch::Receiver<bool>,
) {
    shared_channel(owner)
}

/// 供 manager 构造 `command.status` 查询（**不**重放原命令）。
pub fn status_submit(request_id: Uuid, target: Uuid) -> Result<CommandSubmit, ClientError> {
    Ok(CommandSubmit {
        request_id,
        command: CommandName::CommandStatus,
        session_ref: Nullable::null(),
        attachment_id: Nullable::null(),
        attachment_generation: Nullable::null(),
        expected_version: Nullable::null(),
        payload: CommandPayload::CommandStatus(StatusBody {
            target_request_id: target,
            // 这里只按 requestId 向**本连接的对端**查询；远程来源定位由调用方显式给出，不在重连
            // 恢复路径上凭本地记录猜测（`resource.stateless-origin.v1`，NODE_LINK_PROTOCOL §12.5）。
            remote_target: None,
        }),
    })
}
