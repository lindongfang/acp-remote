//! Sync 远程来源流路由：`sync.remote_subscribe` / `sync.remote_ack` / `sync.remote_unsubscribe`
//! （`docs/SYNC_PROTOCOL.md` §9.7，`design.md` D3–D6）。
//!
//! ## 它替谁做事
//!
//! §9.1–§9.5 的 `sync.subscribe`/`event`/`sync.ack` 只描述本节点 **owned** 事件日志；远程来源（Owner
//! 节点上的会话）走这 9 个独立消息类型，游标是 Owner 的 `originCursor`。本模块把
//! [`acp_core::ports::RemoteEventStream`]（由 `node-link-client` 的 `RemoteRuntime` 提供、组合根注入）
//! 的事件流投影成 `sync.remote_*` 帧，并按连接维护**只存在于内存**的订阅状态。
//!
//! ## 四条不变量
//!
//! 1. **顺序**：`sync.remote_caught_up` 之前的 `sync.remote_event` 都是 Owner 的重放；端口保证屏障之前
//!    不出现实时事件。本模块**不**用定时器推测追平，也不在屏障之前投递实时事件（端口不提供）。
//! 2. **不落盘**：订阅、ACK 与已发水位都是本连接的内存状态；连接关闭即整表丢弃。Access 不写数据库、
//!    临时文件或日志（`AGENTS.md` §7、`design.md` D5）。
//! 3. **有界**：单连接最多 [`MAX_REMOTE_SUBSCRIPTIONS`] 个远程订阅，超限回
//!    `resource.rate_limited{retryAfterMs}`（§9.7）。每订阅只有一个消费者任务，出站队列仍由连接句柄的
//!    有界账本约束；慢客户端由端口（`Lagged`）与出站高水位共同取消**该订阅**，不影响其它订阅或来源。
//! 4. **门控**：未协商 `resource.stateless-origin.v1` 的连接收到任一远程帧都不被本路由认领——连接面对
//!    未知类型照旧回 `protocol.type_unsupported`（§5.3、§9.7 的「未协商降级」），绝不回退到任何本地
//!    落盘中转。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use acp_core::model::{
    AcpRaw as CoreAcpRaw, EventPayload as CoreEventPayload, NodeId, OriginCursor, OriginEpoch,
    PortError, PublicError as CorePublicError, RawUnavailableReason, RemoteSessionRef, SessionId,
    UnavailableKind,
};
use acp_core::ports::{
    RemoteCatalogRevision, RemoteEventStream, RemoteRuntimeDirectory, RemoteSessionEntry,
    RemoteSourceBarrier, RemoteSourceNotice, RemoteSourceState, RemoteStreamEvent,
    RemoteStreamItem, RemoteStreamResetReason,
};
use sync_protocol::common::{
    Base64Url, DecimalString, Nullable, OriginCursor as WireOriginCursor,
    PublicError as WirePublicError, RawAcp, RawObject, SessionRef, Text, Timestamp, Uuid,
};
use sync_protocol::envelope::{Envelope, MessageType};
use sync_protocol::error::{Body, ErrorCode};
use sync_protocol::event::EventType;
use sync_protocol::sync::{
    RemoteAck, RemoteCatalogChanged, RemoteCaughtUp, RemoteEvent, RemoteEventItem,
    RemoteEventPayload, RemoteResetReason, RemoteResetRequired,
    RemoteSourceState as WireSourceState, RemoteSourceStatus, RemoteSubscribe, RemoteUnavailable,
    RemoteUnsubscribe,
};
use tokio::sync::watch;
use tracing::{debug, warn};

use crate::sync::conn::{ConnectionHandle, ConnectionRegistry, MessageRoute, RouteOutcome};

/// `resource.stateless-origin.v1`：运行期来源流/运行期 ACK 的协商 feature（§9.7）。
pub const STATELESS_ORIGIN_FEATURE: &str = "resource.stateless-origin.v1";

/// 单连接的远程订阅上限（§9.7；超限回 `resource.rate_limited`）。
pub const MAX_REMOTE_SUBSCRIPTIONS: usize = 32;

/// 超限时的建议重试间隔（毫秒；`resource.rate_limited.retryAfterMs` 的登记取值域 `0..=3_600_000`）。
const RETRY_AFTER_MS: u64 = 1_000;

/// 组合根注入的运行期端口（与 `UseCases` 里的同一批实例）。
pub struct RemotePorts {
    pub directory: Arc<dyn RemoteRuntimeDirectory>,
    pub stream: Arc<dyn RemoteEventStream>,
}

impl std::fmt::Debug for RemotePorts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemotePorts")
            .finish_non_exhaustive()
    }
}

/// 一条订阅的运行期进度（只在本连接内存里）。
#[derive(Debug, Default)]
struct Progress {
    /// 本订阅已发出的最高来源位置（`remote_ack` 的上限）。
    sent: Option<OriginCursor>,
    /// 本订阅已确认的最高来源位置（单调不减；只服务诊断，不参与投递）。
    acked: Option<OriginCursor>,
}

/// 一条远程订阅。
struct Subscription {
    /// 该会话的来源 epoch（目录条目给出）：ACK 的 epoch 校验基准。
    origin_epoch: OriginEpoch,
    progress: Arc<Mutex<Progress>>,
    /// 取消信号（退订、断线、被同 id 的新订阅替换）。
    cancel: watch::Sender<bool>,
}

/// 一个来源的连接事实（通知去重的记忆）。
#[derive(Debug, Clone, Copy)]
struct SourceFacts {
    state: RemoteSourceState,
    revision: Option<RemoteCatalogRevision>,
}

/// 一条连接的远程流状态。
#[derive(Default)]
struct ConnectionState {
    features: Vec<String>,
    subscriptions: BTreeMap<String, Subscription>,
    sources: BTreeMap<String, SourceFacts>,
    /// 来源通知任务的取消信号（每条连接最多一个）。
    notice_cancel: Option<watch::Sender<bool>>,
}

impl ConnectionState {
    fn negotiated(&self) -> bool {
        self.features
            .iter()
            .any(|feature| feature == STATELESS_ORIGIN_FEATURE)
    }
}

/// 订阅表（任务与路由线程共用：终止的订阅必须能被任务自己摘掉）。
type StateTable = Arc<Mutex<BTreeMap<String, ConnectionState>>>;

/// 远程来源流路由（组合根把它加进 [`crate::sync::router::SyncRoutes`]）。
pub struct RemoteRoute {
    registry: Arc<ConnectionRegistry>,
    directory: Arc<dyn RemoteRuntimeDirectory>,
    stream: Arc<dyn RemoteEventStream>,
    states: StateTable,
}

impl std::fmt::Debug for RemoteRoute {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteRoute")
            .field("connections", &lock(&self.states).len())
            .finish_non_exhaustive()
    }
}

impl RemoteRoute {
    /// 装配。三个端口必须与 `UseCases` 注入的是**同一批实例**（同一份代际/连接事实）。
    pub fn new(registry: Arc<ConnectionRegistry>, ports: RemotePorts) -> Self {
        Self {
            registry,
            directory: ports.directory,
            stream: ports.stream,
            states: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// 本连接是否协商了 `resource.stateless-origin.v1`。
    fn negotiated(&self, session: &ConnectionHandle) -> bool {
        lock(&self.states)
            .get(session.connection_id().as_str())
            .is_some_and(ConnectionState::negotiated)
    }

    /// 取（必要时建立）本连接的状态。
    fn with_state<T>(
        &self,
        session: &ConnectionHandle,
        action: impl FnOnce(&mut ConnectionState) -> T,
    ) -> T {
        let mut states = lock(&self.states);
        let state = states
            .entry(session.connection_id().as_str().to_owned())
            .or_default();
        action(state)
    }

    // -----------------------------------------------------------------------------------------
    // sync.remote_subscribe
    // -----------------------------------------------------------------------------------------

    async fn on_subscribe(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        let Some(request) = decode::<RemoteSubscribe>(message) else {
            return self.schema_invalid(session, message);
        };
        let Some(remote) = core_remote(&request.session_ref) else {
            return self.schema_invalid(session, message);
        };
        let subscription_id = request.subscription_id.clone();
        // 上限判定先于任何来源读取：它是本连接自己的预算，与 Owner 是否可达无关。
        let replacing = {
            let states = lock(&self.states);
            states
                .get(session.connection_id().as_str())
                .is_some_and(|state| state.subscriptions.contains_key(subscription_id.as_str()))
        };
        if !replacing {
            let count = lock(&self.states)
                .get(session.connection_id().as_str())
                .map_or(0, |state| state.subscriptions.len());
            if count >= MAX_REMOTE_SUBSCRIPTIONS {
                debug!(
                    event = "sync.remote_subscription_limit",
                    device_id = session.device_id().as_str(),
                    limit = MAX_REMOTE_SUBSCRIPTIONS,
                    "the connection exceeded the remote subscription budget"
                );
                return self.unavailable(
                    session,
                    &subscription_id,
                    rate_limited_error(),
                    "remote_subscription_limit",
                );
            }
        }
        // 来源身份/在线状态/代际：运行期目录是唯一来源（不从 owned 表或本机文件推断）。
        let entry = match self.directory.lookup(&remote).await {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                let error = owner_state_error(&self.directory, &remote).await;
                return self.unavailable(session, &subscription_id, error, "remote_source_missing");
            }
            Err(error) => {
                let error = source_error(&error);
                return self.unavailable(
                    session,
                    &subscription_id,
                    error,
                    "remote_directory_failed",
                );
            }
        };
        let Some(origin_epoch) = entry_epoch(&entry) else {
            return self.unavailable(
                session,
                &subscription_id,
                internal_error(),
                "remote_source_corrupt",
            );
        };
        let cursor = match request.cursor.as_ref() {
            Some(cursor) => match core_cursor(cursor) {
                Some(cursor) => Some(cursor),
                None => return self.schema_invalid(session, message),
            },
            None => None,
        };
        // `null` 订阅 = 首次重建：Owner 的元数据屏障作为 R 交给客户端，客户端先 `session.read` 重建，
        // 同时本订阅继续缓冲/投递屏障之后的事件（design D4）。
        let request_rebuild = cursor.is_none();
        let receiver = match self
            .stream
            .subscribe(&remote, entry.generation, cursor)
            .await
        {
            Ok(receiver) => receiver,
            Err(error) => {
                let error = source_error(&error);
                return self.unavailable(
                    session,
                    &subscription_id,
                    error,
                    "remote_subscribe_failed",
                );
            }
        };
        // 同一个 subscriptionId 的旧订阅先取消（替换语义：id 是本次连接内的随机标识，不得复用两条）。
        self.cancel_subscription(session, subscription_id.as_str());
        let progress = Arc::new(Mutex::new(Progress::default()));
        let (cancel, cancel_rx) = watch::channel(false);
        self.with_state(session, |state| {
            state.subscriptions.insert(
                subscription_id.as_str().to_owned(),
                Subscription {
                    origin_epoch,
                    progress: Arc::clone(&progress),
                    cancel,
                },
            );
        });
        let Some(handle) = self.handle_of(session) else {
            self.cancel_subscription(session, subscription_id.as_str());
            self.stream.unsubscribe(&remote, entry.generation).await;
            return RouteOutcome::Claimed;
        };
        let states = Arc::clone(&self.states);
        let connection = session.connection_id().as_str().to_owned();
        let id = subscription_id.as_str().to_owned();
        let session_ref = remote.clone();
        let generation = entry.generation;
        let stream = Arc::clone(&self.stream);
        let retire_progress = Arc::clone(&progress);
        tokio::spawn(async move {
            run_subscription(
                handle,
                subscription_id,
                progress,
                cancel_rx,
                receiver,
                request_rebuild,
                SubscriptionCleanup {
                    session: session_ref,
                    generation,
                    stream,
                },
            )
            .await;
            retire(&states, &connection, &id, &retire_progress);
        });
        debug!(
            event = "sync.remote_subscribed",
            device_id = session.device_id().as_str(),
            owner_node_id = remote.owner_node_id.as_str(),
            "the device subscribed to a remote source"
        );
        RouteOutcome::Claimed
    }

    // -----------------------------------------------------------------------------------------
    // sync.remote_ack
    // -----------------------------------------------------------------------------------------

    /// 运行期 ACK：只更新本连接该订阅的内存进度，不落盘、不推 Owner head（§9.7、design D5）。
    ///
    /// 拒绝条件与对端可见错误：
    /// - 未知 `subscriptionId`（含已终止/已替换）→ `protocol.sequence_invalid`；
    /// - `originEpoch` 与订阅来源不符 → `protocol.sequence_invalid`（序号域不同，不可比）；
    /// - 越过本订阅已发 head（含尚未发出任何事件）→ `protocol.sequence_invalid`；
    /// - 回退/重复 → **忽略**并记诊断（不改动已知进度，也不报错）。
    async fn on_ack(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        let Some(request) = decode::<RemoteAck>(message) else {
            return self.schema_invalid(session, message);
        };
        let Some(cursor) = core_cursor(&request.cursor) else {
            return self.schema_invalid(session, message);
        };
        let key = request.subscription_id.as_str().to_owned();
        let verdict = {
            let states = lock(&self.states);
            let Some(subscription) = states
                .get(session.connection_id().as_str())
                .and_then(|state| state.subscriptions.get(&key))
            else {
                return self.sequence_invalid(session, message, "remote_ack_unknown_subscription");
            };
            if subscription.origin_epoch != cursor.origin_epoch {
                return self.sequence_invalid(session, message, "remote_ack_epoch_mismatch");
            }
            let mut progress = lock(&subscription.progress);
            match progress.sent.as_ref() {
                Some(sent) if cursor.origin_sequence <= sent.origin_sequence => {
                    match progress.acked.as_ref() {
                        Some(acked) if acked.origin_sequence >= cursor.origin_sequence => {
                            AckVerdict::Regressed
                        }
                        _ => {
                            progress.acked = Some(cursor.clone());
                            AckVerdict::Advanced
                        }
                    }
                }
                _ => AckVerdict::BeyondSent,
            }
        };
        match verdict {
            AckVerdict::Advanced => {
                debug!(
                    event = "sync.remote_ack_accepted",
                    device_id = session.device_id().as_str(),
                    origin_sequence = cursor.origin_sequence.get(),
                    "the device acknowledged remote progress on this connection"
                );
                RouteOutcome::Claimed
            }
            AckVerdict::Regressed => {
                debug!(
                    event = "sync.remote_ack_ignored",
                    device_id = session.device_id().as_str(),
                    origin_sequence = cursor.origin_sequence.get(),
                    "the acknowledgement does not advance the subscription progress"
                );
                RouteOutcome::Claimed
            }
            AckVerdict::BeyondSent => {
                self.sequence_invalid(session, message, "remote_ack_beyond_sent")
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // sync.remote_unsubscribe
    // -----------------------------------------------------------------------------------------

    async fn on_unsubscribe(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        let Some(request) = decode::<RemoteUnsubscribe>(message) else {
            return self.schema_invalid(session, message);
        };
        let _ = self.cancel_subscription(session, request.subscription_id.as_str());
        debug!(
            event = "sync.remote_unsubscribed",
            device_id = session.device_id().as_str(),
            "the device cancelled a remote subscription"
        );
        RouteOutcome::Claimed
    }

    /// 取消一个订阅（存在才取消；返回是否命中）。
    fn cancel_subscription(&self, session: &ConnectionHandle, key: &str) -> bool {
        let removed = self.with_state(session, |state| state.subscriptions.remove(key));
        match removed {
            Some(subscription) => {
                let _ = subscription.cancel.send(true);
                true
            }
            None => false,
        }
    }

    /// 取消本连接的全部订阅与通知任务（断线/撤销/关闭）。
    fn cancel_connection(&self, session: &ConnectionHandle) {
        let state = lock(&self.states).remove(session.connection_id().as_str());
        let Some(state) = state else {
            return;
        };
        for subscription in state.subscriptions.values() {
            let _ = subscription.cancel.send(true);
        }
        if let Some(cancel) = state.notice_cancel {
            let _ = cancel.send(true);
        }
    }

    /// 连接句柄（任务需要 `'static` 的 `Arc`；注册表是连接存活性的唯一判据）。
    fn handle_of(&self, session: &ConnectionHandle) -> Option<Arc<ConnectionHandle>> {
        self.registry
            .handles()
            .into_iter()
            .find(|handle| handle.connection_id() == session.connection_id())
    }

    // -----------------------------------------------------------------------------------------
    // 来源通知（`sync.remote_catalog_changed`/`remote_source_state`）
    // -----------------------------------------------------------------------------------------

    /// 确保本连接有一个来源通知任务（幂等）。
    fn ensure_watcher(&self, session: &ConnectionHandle) {
        let receiver = self.with_state(session, |state| {
            if state.notice_cancel.is_some() {
                return None;
            }
            let (cancel, receiver) = watch::channel(false);
            state.notice_cancel = Some(cancel);
            Some(receiver)
        });
        let Some(receiver) = receiver else {
            return;
        };
        let Some(handle) = self.handle_of(session) else {
            return;
        };
        let stream = Arc::clone(&self.stream);
        let states = Arc::clone(&self.states);
        let connection = session.connection_id().as_str().to_owned();
        tokio::spawn(async move {
            run_notices(handle, stream, receiver, &states, &connection).await;
        });
    }

    // -----------------------------------------------------------------------------------------
    // 对端可见的装配
    // -----------------------------------------------------------------------------------------

    /// `sync.remote_unavailable`：订阅级的显式不可用（断线/撤销/不支持/限流）。
    fn unavailable(
        &self,
        session: &ConnectionHandle,
        subscription_id: &Uuid,
        error: WirePublicError,
        event: &'static str,
    ) -> RouteOutcome {
        let body = RemoteUnavailable {
            subscription_id: subscription_id.clone(),
            error,
        };
        if let Err(fault) = session.send(MessageType::SyncRemoteUnavailable, &body) {
            debug!(
                event = "sync.remote_unavailable_not_sent",
                detail = event,
                fault = ?fault,
                "the remote unavailable frame could not be queued"
            );
        }
        RouteOutcome::Claimed
    }

    fn schema_invalid(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        self.send_error(
            session,
            ErrorCode::ProtocolSchemaInvalid,
            "the message does not match the v1 schema",
            RawObject::empty(),
            Some(message.message_id().clone()),
            "schema_invalid",
        )
    }

    /// `protocol.sequence_invalid`（订阅/ACK 的序号域或归属不符）。
    fn sequence_invalid(
        &self,
        session: &ConnectionHandle,
        message: &Envelope,
        detail: &'static str,
    ) -> RouteOutcome {
        self.send_error(
            session,
            ErrorCode::ProtocolSequenceInvalid,
            "the cursor is not usable for this subscription",
            RawObject::empty(),
            Some(message.message_id().clone()),
            detail,
        )
    }

    fn send_error(
        &self,
        session: &ConnectionHandle,
        code: ErrorCode,
        text: &'static str,
        details: RawObject,
        correlation: Option<Uuid>,
        event: &'static str,
    ) -> RouteOutcome {
        let Some(body) = error_body(code, text, details, correlation) else {
            warn!(
                event = "sync.remote_error_body_failed",
                detail = event,
                "the error body cannot be assembled"
            );
            return RouteOutcome::Claimed;
        };
        if let Err(fault) = session.send(MessageType::Error, &body) {
            debug!(
                event = "sync.remote_error_not_sent",
                detail = event,
                fault = ?fault,
                "the error message could not be queued"
            );
        }
        RouteOutcome::Claimed
    }
}

#[async_trait::async_trait]
impl MessageRoute for RemoteRoute {
    async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        match message.message_type() {
            MessageType::SyncRemoteSubscribe
            | MessageType::SyncRemoteAck
            | MessageType::SyncRemoteUnsubscribe => {
                if !self.negotiated(session) {
                    // 未协商：不认领，由连接面回 `protocol.type_unsupported`（§5.3/§9.7），
                    // 绝不用任何本地落盘中转「兼容」。
                    debug!(
                        event = "sync.remote_frame_unnegotiated",
                        device_id = session.device_id().as_str(),
                        "the connection did not negotiate resource.stateless-origin.v1"
                    );
                    return RouteOutcome::Unclaimed;
                }
                self.ensure_watcher(session);
                match message.message_type() {
                    MessageType::SyncRemoteSubscribe => self.on_subscribe(session, message).await,
                    MessageType::SyncRemoteAck => self.on_ack(session, message).await,
                    _ => self.on_unsubscribe(session, message).await,
                }
            }
            _ => RouteOutcome::Unclaimed,
        }
    }

    fn on_authenticated(&self, session: &ConnectionHandle, features: &[String]) {
        self.with_state(session, |state| {
            state.features = features.to_vec();
        });
        if features
            .iter()
            .any(|feature| feature == STATELESS_ORIGIN_FEATURE)
        {
            // 只看清单、不打开会话的客户端也必须收到来源离线/撤销与目录 revision 变化。
            self.ensure_watcher(session);
        }
    }

    async fn on_closed(&self, session: &ConnectionHandle) {
        self.cancel_connection(session);
    }
}

/// 只前进不回退的「已发最高来源位置」（同一 epoch 内比较；跨 epoch 视为来源重置）。
fn advance_sent(progress: &Mutex<Progress>, cursor: OriginCursor) {
    let mut progress = lock(progress);
    let replace = match &progress.sent {
        None => true,
        Some(current) => {
            current.origin_epoch != cursor.origin_epoch
                || cursor.origin_sequence > current.origin_sequence
        }
    };
    if replace {
        progress.sent = Some(cursor);
    }
}

/// 一条订阅的消费循环：端口项 → `sync.remote_*` 帧；`sent` 水位随实际入队推进。
/// 订阅结束时释放运行期观察者（）所需的上下文。
struct SubscriptionCleanup {
    session: RemoteSessionRef,
    generation: acp_core::ports::RemoteGeneration,
    stream: Arc<dyn RemoteEventStream>,
}

async fn run_subscription(
    handle: Arc<ConnectionHandle>,
    subscription_id: Uuid,
    progress: Arc<Mutex<Progress>>,
    cancel: watch::Receiver<bool>,
    receiver: Box<dyn acp_core::ports::RemoteStreamReceiver>,
    request_rebuild: bool,
    cleanup: SubscriptionCleanup,
) {
    run_subscription_inner(
        handle,
        subscription_id,
        progress,
        cancel,
        receiver,
        request_rebuild,
    )
    .await;
    cleanup
        .stream
        .unsubscribe(&cleanup.session, cleanup.generation)
        .await;
}

async fn run_subscription_inner(
    handle: Arc<ConnectionHandle>,
    subscription_id: Uuid,
    progress: Arc<Mutex<Progress>>,
    mut cancel: watch::Receiver<bool>,
    mut receiver: Box<dyn acp_core::ports::RemoteStreamReceiver>,
    request_rebuild: bool,
) {
    let mut rebuilt = false;
    loop {
        let item = tokio::select! {
            biased;
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    debug!(
                        event = "sync.remote_subscription_cancelled",
                        connection_id = handle.connection_id().as_str(),
                        "the remote subscription was cancelled"
                    );
                    return;
                }
                continue;
            }
            item = receiver.next() => item,
        };
        let Some(item) = item else {
            debug!(
                event = "sync.remote_subscription_ended",
                connection_id = handle.connection_id().as_str(),
                "the remote stream ended"
            );
            return;
        };
        match item {
            RemoteStreamItem::Event(event) => {
                let Some(body) = remote_event_body(&subscription_id, event.clone()) else {
                    warn!(
                        event = "sync.remote_event_unmappable",
                        connection_id = handle.connection_id().as_str(),
                        "the source event cannot be mapped to sync.remote_event"
                    );
                    continue;
                };
                if let Err(fault) = handle.send(MessageType::SyncRemoteEvent, &body) {
                    debug!(
                        event = "sync.remote_event_not_sent",
                        connection_id = handle.connection_id().as_str(),
                        fault = ?fault,
                        "the outbound ledger refused the remote event; the subscription stops"
                    );
                    // 高水位/连接结束：取消该订阅（不阻塞其它订阅与 Owner），客户端显式重建。
                    let _ = handle.send(
                        MessageType::SyncRemoteUnavailable,
                        &RemoteUnavailable {
                            subscription_id: subscription_id.clone(),
                            error: backpressure_error(),
                        },
                    );
                    return;
                }
                advance_sent(
                    &progress,
                    OriginCursor {
                        origin_epoch: event.origin_epoch.clone(),
                        origin_sequence: event.origin_sequence,
                    },
                );
            }
            RemoteStreamItem::CaughtUp(RemoteSourceBarrier { cursor, .. }) => {
                let Some(wire) = wire_cursor(&cursor) else {
                    warn!(
                        event = "sync.remote_caught_up_unmappable",
                        connection_id = handle.connection_id().as_str(),
                        "the barrier cursor cannot be projected"
                    );
                    continue;
                };
                if request_rebuild && !rebuilt {
                    rebuilt = true;
                    let body = RemoteResetRequired {
                        subscription_id: subscription_id.clone(),
                        reason: RemoteResetReason::CacheIncompatible,
                        cursor: Nullable::value(wire),
                    };
                    if handle
                        .send(MessageType::SyncRemoteResetRequired, &body)
                        .is_err()
                    {
                        return;
                    }
                } else {
                    let body = RemoteCaughtUp {
                        subscription_id: subscription_id.clone(),
                        cursor: wire,
                    };
                    if handle.send(MessageType::SyncRemoteCaughtUp, &body).is_err() {
                        return;
                    }
                }
                advance_sent(&progress, cursor);
            }
            RemoteStreamItem::Reset { reason, cursor } => {
                let body = RemoteResetRequired {
                    subscription_id: subscription_id.clone(),
                    reason: wire_reset_reason(reason),
                    cursor: match cursor.as_ref().and_then(wire_cursor) {
                        Some(cursor) => Nullable::value(cursor),
                        None => Nullable::null(),
                    },
                };
                let _ = handle.send(MessageType::SyncRemoteResetRequired, &body);
                return;
            }
            RemoteStreamItem::Unavailable(error) => {
                let body = RemoteUnavailable {
                    subscription_id: subscription_id.clone(),
                    error: wire_public_error(&error),
                };
                let _ = handle.send(MessageType::SyncRemoteUnavailable, &body);
                return;
            }
        }
    }
}

/// 来源通知循环：把端口的清单/连接事实通知投影成 `sync.remote_catalog_changed`/`remote_source_state`。
///
/// 通知通道溢出或关闭时重新建立接收端（不丢「之后」的事实）；连接被取消即退出。
async fn run_notices(
    handle: Arc<ConnectionHandle>,
    stream: Arc<dyn RemoteEventStream>,
    mut cancel: watch::Receiver<bool>,
    states: &StateTable,
    connection: &str,
) {
    loop {
        let receiver = tokio::select! {
            biased;
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    return;
                }
                continue;
            }
            receiver = stream.notices() => match receiver {
                Ok(receiver) => receiver,
                Err(error) => {
                    debug!(
                        event = "sync.remote_notices_unavailable",
                        connection_id = connection,
                        error = ?error,
                        "the source notice stream is not available; catalog change frames stop"
                    );
                    return;
                }
            },
        };
        let mut receiver = receiver;
        loop {
            let notice = tokio::select! {
                biased;
                changed = cancel.changed() => {
                    if changed.is_err() || *cancel.borrow() {
                        return;
                    }
                    continue;
                }
                notice = receiver.next() => notice,
            };
            let Some(notice) = notice else {
                // 溢出/关闭：重建接收端并从目录事实重新开始（下一轮 `notices()`）。
                break;
            };
            let Some(body) = notice_body(&notice) else {
                continue;
            };
            match body {
                NoticeBody::Catalog(owner, revision) => {
                    let changed = {
                        let mut table = lock(states);
                        let Some(state) = table.get_mut(connection) else {
                            return;
                        };
                        let previous = state.sources.get(&owner).copied();
                        let changed = previous.and_then(|facts| facts.revision) != Some(revision);
                        state.sources.insert(
                            owner.clone(),
                            SourceFacts {
                                state: previous
                                    .map_or(RemoteSourceState::Online, |facts| facts.state),
                                revision: Some(revision),
                            },
                        );
                        changed
                    };
                    if changed {
                        let Some(revision) = DecimalString::parse(&revision.get().to_string()).ok()
                        else {
                            continue;
                        };
                        let Some(owner_node_id) = Uuid::parse(&owner).ok() else {
                            continue;
                        };
                        let body = RemoteCatalogChanged {
                            owner_node_id,
                            revision,
                        };
                        if handle
                            .send(MessageType::SyncRemoteCatalogChanged, &body)
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                NoticeBody::State(owner, source) => {
                    let changed = {
                        let mut table = lock(states);
                        let Some(state) = table.get_mut(connection) else {
                            return;
                        };
                        let previous = state.sources.get(&owner).copied();
                        let changed = previous.is_none_or(|facts| facts.state != source);
                        state.sources.insert(
                            owner.clone(),
                            SourceFacts {
                                state: source,
                                revision: previous.and_then(|facts| facts.revision),
                            },
                        );
                        changed
                    };
                    if changed {
                        let Some(owner_node_id) = Uuid::parse(&owner).ok() else {
                            continue;
                        };
                        let body = WireSourceState {
                            owner_node_id,
                            state: wire_source_state(source),
                        };
                        if handle
                            .send(MessageType::SyncRemoteSourceState, &body)
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
        }
    }
}

/// 通知的中性形状（避免把协议 DTO 带进判定臂）。
enum NoticeBody {
    Catalog(String, RemoteCatalogRevision),
    State(String, RemoteSourceState),
}

fn notice_body(notice: &RemoteSourceNotice) -> Option<NoticeBody> {
    Some(match notice {
        RemoteSourceNotice::CatalogChanged {
            owner_node_id,
            revision,
        } => NoticeBody::Catalog(owner_node_id.as_str().to_owned(), *revision),
        RemoteSourceNotice::State {
            owner_node_id,
            state,
        } => NoticeBody::State(owner_node_id.as_str().to_owned(), *state),
    })
}

/// 任务结束时摘掉自己的订阅（只摘**同一个** `Progress` 实例，避免误删同 id 的新订阅）。
fn retire(
    states: &StateTable,
    connection: &str,
    subscription: &str,
    progress: &Arc<Mutex<Progress>>,
) {
    let mut table = lock(states);
    let Some(state) = table.get_mut(connection) else {
        return;
    };
    let same = state
        .subscriptions
        .get(subscription)
        .is_some_and(|current| Arc::ptr_eq(&current.progress, progress));
    if same {
        state.subscriptions.remove(subscription);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AckVerdict {
    Advanced,
    Regressed,
    BeyondSent,
}

// ---------------------------------------------------------------------------------------------
// 映射
// ---------------------------------------------------------------------------------------------

fn core_remote(reference: &SessionRef) -> Option<RemoteSessionRef> {
    Some(RemoteSessionRef::new(
        NodeId::new(reference.owner_node_id.as_str()).ok()?,
        SessionId::new(reference.session_id.as_str()).ok()?,
    ))
}

fn core_cursor(cursor: &WireOriginCursor) -> Option<OriginCursor> {
    let epoch = OriginEpoch::new(cursor.origin_epoch.as_str()).ok()?;
    let sequence = cursor.origin_sequence.as_str().parse::<u64>().ok()?;
    Some(OriginCursor {
        origin_epoch: epoch,
        origin_sequence: acp_core::model::Sequence::new(sequence).ok()?,
    })
}

fn wire_cursor(cursor: &OriginCursor) -> Option<WireOriginCursor> {
    Some(WireOriginCursor {
        origin_epoch: Uuid::parse(cursor.origin_epoch.as_str()).ok()?,
        origin_sequence: DecimalString::parse(&cursor.origin_sequence.get().to_string()).ok()?,
    })
}

fn entry_epoch(entry: &RemoteSessionEntry) -> Option<OriginEpoch> {
    match entry.summary.origin() {
        acp_core::model::ResourceOrigin::Remote { origin_epoch, .. } => Some(origin_epoch.clone()),
        acp_core::model::ResourceOrigin::Local => None,
    }
}

/// 订阅来源不存在时，按来源状态给出显式错误（离线/撤销/未知不折叠成同一个 `not_found`）。
async fn owner_state_error(
    directory: &Arc<dyn RemoteRuntimeDirectory>,
    remote: &RemoteSessionRef,
) -> WirePublicError {
    match directory.owner_state(&remote.owner_node_id).await {
        Ok(RemoteSourceState::Online) => not_found_error(),
        Ok(RemoteSourceState::Offline) => owner_offline_error(),
        Ok(RemoteSourceState::Revoked) => revoked_error(),
        Err(error) => source_error(&error),
    }
}

fn wire_public_error(error: &CorePublicError) -> WirePublicError {
    WirePublicError {
        code: error_code(error.code()),
        message: Text::<1024>::parse(error.message()).unwrap_or_else(|_| {
            Text::<1024>::parse("the source reported an error").expect("the fallback text is valid")
        }),
        retryable: error.retryable(),
        details: RawObject::parse(error.details().as_str()).unwrap_or_else(|_| RawObject::empty()),
    }
}

/// core 的错误码文本 → sync 错误码。未登记/不属于本协议的文本按 `internal.unavailable` 收敛。
fn error_code(code: &str) -> ErrorCode {
    match code {
        "resource.remote_unavailable" => ErrorCode::ResourceRemoteUnavailable,
        "resource.backpressure" => ErrorCode::ResourceBackpressure,
        "capability.unsupported_by_broker" => ErrorCode::CapabilityUnsupportedByBroker,
        "auth.device_revoked" => ErrorCode::AuthDeviceRevoked,
        "session.not_found" => ErrorCode::SessionNotFound,
        "protocol.sequence_invalid" => ErrorCode::ProtocolSequenceInvalid,
        _ => ErrorCode::InternalUnavailable,
    }
}

/// `PortError` → 订阅级公开错误（每一类都有独立语义，不折叠成同一条）。
fn source_error(error: &PortError) -> WirePublicError {
    match error {
        PortError::Unavailable(UnavailableKind::OwnerOffline)
        | PortError::Unavailable(UnavailableKind::RemoteUnavailable) => owner_offline_error(),
        PortError::Unavailable(UnavailableKind::BackendUnsupported) => WirePublicError {
            code: ErrorCode::CapabilityUnsupportedByBroker,
            message: Text::<1024>::parse("the source stream is not supported")
                .expect("the message is bounded"),
            retryable: false,
            details: RawObject::empty(),
        },
        PortError::NotFound(_) => not_found_error(),
        _ => internal_error(),
    }
}

fn owner_offline_error() -> WirePublicError {
    WirePublicError {
        code: ErrorCode::ResourceRemoteUnavailable,
        message: Text::<1024>::parse("the owner node is not reachable")
            .expect("the message is bounded"),
        retryable: true,
        details: RawObject::parse("{\"reason\":\"owner_offline\"}")
            .expect("the registered details shape is valid"),
    }
}

fn not_found_error() -> WirePublicError {
    WirePublicError {
        code: ErrorCode::SessionNotFound,
        message: Text::<1024>::parse("the session is not visible on this connection")
            .expect("the message is bounded"),
        retryable: false,
        details: RawObject::empty(),
    }
}

fn revoked_error() -> WirePublicError {
    WirePublicError {
        code: ErrorCode::AuthDeviceRevoked,
        message: Text::<1024>::parse("the source was revoked").expect("the message is bounded"),
        retryable: false,
        details: RawObject::empty(),
    }
}

fn rate_limited_error() -> WirePublicError {
    WirePublicError {
        code: ErrorCode::ResourceRateLimited,
        message: Text::<1024>::parse(
            "the connection holds the maximum number of remote subscriptions",
        )
        .expect("the message is bounded"),
        retryable: true,
        details: RawObject::parse(&format!("{{\"retryAfterMs\":{RETRY_AFTER_MS}}}"))
            .expect("the registered details shape is valid"),
    }
}

fn backpressure_error() -> WirePublicError {
    WirePublicError {
        code: ErrorCode::ResourceBackpressure,
        message: Text::<1024>::parse("the connection cannot take more source events")
            .expect("the message is bounded"),
        retryable: true,
        details: RawObject::empty(),
    }
}

fn internal_error() -> WirePublicError {
    WirePublicError {
        code: ErrorCode::InternalUnavailable,
        message: Text::<1024>::parse("the source stream cannot be served")
            .expect("the message is bounded"),
        retryable: true,
        details: RawObject::empty(),
    }
}

fn wire_reset_reason(reason: RemoteStreamResetReason) -> RemoteResetReason {
    match reason {
        RemoteStreamResetReason::EpochMismatch => RemoteResetReason::EpochMismatch,
        RemoteStreamResetReason::CursorExpired => RemoteResetReason::CursorExpired,
        RemoteStreamResetReason::CacheIncompatible => RemoteResetReason::CacheIncompatible,
    }
}

fn wire_source_state(state: RemoteSourceState) -> RemoteSourceStatus {
    match state {
        RemoteSourceState::Online => RemoteSourceStatus::Online,
        RemoteSourceState::Offline => RemoteSourceStatus::Offline,
        RemoteSourceState::Revoked => RemoteSourceStatus::Revoked,
    }
}

/// 一条来源事件 → `sync.remote_event` 的 body。**不**重算 `payloadDigest`：Owner 的摘要原样转发。
fn remote_event_body(subscription_id: &Uuid, event: RemoteStreamEvent) -> Option<RemoteEvent> {
    let payload = wire_payload(&event.payload)?;
    Some(RemoteEvent {
        subscription_id: subscription_id.clone(),
        event: RemoteEventItem {
            origin_event_id: Uuid::parse(event.origin_event_id.as_str()).ok()?,
            origin_epoch: Uuid::parse(event.origin_epoch.as_str()).ok()?,
            origin_sequence: DecimalString::parse(&event.origin_sequence.get().to_string()).ok()?,
            session_ref: SessionRef {
                owner_node_id: Uuid::parse(event.owner_node_id.as_str()).ok()?,
                session_id: Uuid::parse(event.session_id.as_str()).ok()?,
            },
            event_type: EventType::parse(event.event_type.as_str()).ok()?,
            payload_digest: Base64Url::<32>::parse(event.payload_digest.as_str()).ok()?,
            created_at: Timestamp::parse(event.created_at.as_str()).ok()?,
            payload,
        },
    })
}

/// core payload → `sync.remote_event.payload`：`view` 逐字节、`acp` 原文逐字（超过内嵌上限按
/// `size_limit` 显式降级，不截断）。
fn wire_payload(payload: &CoreEventPayload) -> Option<RemoteEventPayload> {
    let view = RawObject::parse(payload.view.as_str()).ok()?;
    let acp = match payload.acp.as_ref() {
        Some(CoreAcpRaw::Available {
            media_type,
            raw_json,
            byte_length,
            sha256,
        }) => {
            let digest = Base64Url::<32>::parse(sha256.as_str()).ok()?;
            if media_type != "application/json" || raw_json.len() > MAX_INLINE_ACP_BYTES {
                let reason = if media_type != "application/json" {
                    acpr_wire::RawAcpUnavailableReason::StorageFailure
                } else {
                    acpr_wire::RawAcpUnavailableReason::SizeLimit
                };
                Some(RawAcp::Unavailable {
                    reason,
                    byte_length: DecimalString::parse(&byte_length.to_string()).ok()?,
                    sha256: Nullable::from_option(Some(digest)),
                })
            } else {
                let quoted = serde_json::to_string(raw_json).ok()?;
                Some(RawAcp::Complete {
                    raw_json: serde_json::value::RawValue::from_string(quoted).ok()?,
                    byte_length: DecimalString::parse(&byte_length.to_string()).ok()?,
                    sha256: digest,
                })
            }
        }
        Some(CoreAcpRaw::Unavailable {
            reason,
            byte_length,
            sha256,
        }) => Some(RawAcp::Unavailable {
            reason: match reason {
                RawUnavailableReason::SizeLimit => acpr_wire::RawAcpUnavailableReason::SizeLimit,
                RawUnavailableReason::RetentionExpired => {
                    acpr_wire::RawAcpUnavailableReason::RetentionExpired
                }
                RawUnavailableReason::StorageFailure => {
                    acpr_wire::RawAcpUnavailableReason::StorageFailure
                }
            },
            byte_length: DecimalString::parse(&byte_length.to_string()).ok()?,
            sha256: match sha256 {
                Some(digest) => Nullable::from_option(Base64Url::<32>::parse(digest.as_str()).ok()),
                None => Nullable::null(),
            },
        }),
        None => None,
    };
    RemoteEventPayload::new(Some(view), acp).ok()
}

/// `session.read` 结果里 ACP 原文的内嵌上限（与 `session_read`/事件帧同一条规则）。
const MAX_INLINE_ACP_BYTES: usize = 256 * 1024;

fn decode<T: serde::de::DeserializeOwned>(message: &Envelope) -> Option<T> {
    serde_json::from_str(message.body().get()).ok()
}

/// `error` 的 body（形状与 `subscribe` 模块同口径：只允许登记的 `details`、retryable 取错误码登记值）。
fn error_body(
    code: ErrorCode,
    message: &str,
    details: RawObject,
    correlation_id: Option<Uuid>,
) -> Option<Body> {
    Some(Body {
        code,
        message: Text::<1024>::parse(message).ok()?,
        retryable: retryable_for(code),
        correlation_id: match correlation_id {
            Some(id) => Nullable::value(id),
            None => Nullable::null(),
        },
        details,
    })
}

/// 错误码的登记 `retryable`（只在本模块产生的四个码上使用；其它码按不可重试处理）。
fn retryable_for(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::ResourceRateLimited
            | ErrorCode::ResourceBackpressure
            | ErrorCode::ResourceRemoteUnavailable
            | ErrorCode::InternalUnavailable
    )
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
