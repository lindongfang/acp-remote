//! Node Link 的 attachment / 订阅 / 快照 / origin 重放 / 事件扇出 / ACK
//! （`design.md` D5/D6/D9，`NODE_LINK_PROTOCOL.md` §12.4）。
//!
//! 一条连接的状态在本模块内维护（`connectionId` → attachment/订阅/ACK 水位），因为 `conn` 只承载
//! 「连接级事实」（注册表与有界出站队列），协议状态属资源层：
//!
//! - **attachment 是临时路由凭据**：同一个 `remoteSessionRef` 每次 `resource.attach` 都得到新的
//!   `attachmentId`/`attachmentGeneration`；新 generation 生效即覆盖旧的那个（旧 frame 因此找不到
//!   当前 attachment → `attach_generation_stale`），重新 attach 也**清掉**该会话的旧订阅（Access 必须
//!   重新 `resource.subscribe` 才恢复事件流）；
//! - **快照只承载元数据**：`session_meta` 与 `pending_interactions` 两类 item，正文一律不进快照；
//!   `snapshotDigest` 按「对**将要发出的**完整 chunk 帧文本逐条 SHA-256、按 index 连接再 SHA-256」计算
//!   （§12.4：接收方按收到的原始字节复算，不得重新序列化）；
//! - **正文只在事件里**：`resource.event` 的 payload 由 core 的会话绑定只读入口按 `eventId` 取回，
//!   `payloadDigest = base64url(SHA-256(ACPR-CJ1(payload)))`（ACPR-CJ1 来自叶子 crate
//!   `acpr_wire::cj1`，D5）；内嵌 ACP 原文超过 256 KiB 时降级为 `rawAcp.rawUnavailable(size_limit)`，
//!   不截断后伪装完整；
//! - **先持久化后发布**：本模块只消费 broker 在 `SessionStore::commit` 成功**之后**发布的
//!   `CommittedDelivery::Owned`（§6.1），自己不产生事件；
//! - **快照并发为 1**：一条连接 = 一个会话任务（`conn::session`），`route` 因此对该连接串行调用，
//!   一次 `resource.subscribe` 内部同步发完整条快照；
//! - **慢连接**：投递一律经 WP4 的有界队列；高水位时跳过这一条并记录，持续过慢由会话看门狗断开，
//!   Access 重连后凭 origin cursor 重放补齐（§2.5；v1 不发 `link.backpressure`）。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use acp_core::model::{
    AcpRaw, CommittedDelivery, CommittedEvent, EntityRef, EventPayload as CoreEventPayload,
    ExportId, InteractionKind, NodeId, OriginCursor as CoreOriginCursor, PortError,
    RawUnavailableReason, SessionId, SessionState as CoreSessionState,
};
use acp_core::ports::{EventPublisher, ReplayLimit};
use acp_core::use_cases::{NodeLinkEvent, NodeLinkReplay, UseCases};
use acpr_wire::{RawAcp, RawAcpUnavailableReason, RawObject};
use node_link_protocol::common::{
    Base64Url, DecimalString, InteractionKind as WireInteractionKind, Nullable, OriginCursor,
    PendingInteraction, ProtocolVersionV1, RemoteSessionRef, SessionMeta,
    SessionState as WireSessionState, Timestamp, Uuid,
};
use node_link_protocol::envelope::{Envelope, MessageType};
use node_link_protocol::error::ErrorCode;
use node_link_protocol::resource::{
    Ack, Attach, Attached, Event, EventPayload as WireEventPayload, EventType as WireEventType,
    SnapshotBegin, SnapshotChunk, SnapshotEnd, SnapshotItemSessionMeta, SnapshotItems, Subscribe,
};
use serde::de::DeserializeOwned;
use sha2::{Digest as _, Sha256};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::node_link::conn::{ConnectionHandle, ConnectionRegistry, MessageRoute, RouteOutcome};
use crate::node_link::{catalog, link_error};
use crate::transport::net::Shutdown;

/// 扇出链路自己的有界队列容量（每连接队列之外的第二道；满了只丢这一条并记日志，broker 绝不被阻塞）。
pub const EVENT_QUEUE_CAPACITY: usize = 1024;

/// 内嵌 ACP 原文的单条上限（§12.4：超过即 `rawAcp.rawUnavailable(size_limit)`）。
pub const MAX_INLINE_ACP_BYTES: usize = 256 * 1024;

/// 一条连接上的资源状态。
#[derive(Debug, Default)]
struct ConnectionState {
    /// 下一个可用的 generation（同一连接内单调递增）。
    next_generation: u64,
    /// 当前 attachment（按 `sessionId`）：新 attach 覆盖旧的那个。
    attachments: BTreeMap<String, Attachment>,
    /// 已订阅的会话（按 `sessionId`）：事件扇出只看它。
    subscriptions: BTreeMap<String, Subscription>,
    /// 每个会话最近一次 ACK 的 origin cursor（单调不减）。
    acks: BTreeMap<String, CoreOriginCursor>,
}

/// 一个会话的 attachment（`resource.attached` 下发的临时路由凭据）。
#[derive(Debug, Clone)]
struct Attachment {
    id: Uuid,
    generation: u64,
    export: ExportId,
    session: SessionId,
    /// wire 上的 `remoteSessionRef`（回包与事件映射共用同一份）。
    remote: RemoteSessionRef,
}

/// 一条连接上的订阅（会话 + 生效的 attachment 代际）。
#[derive(Debug, Clone)]
struct Subscription {
    attachment_id: Uuid,
    generation: u64,
}

/// WP5 的 resource 路由：attachment、快照/重放、ACK，以及事件扇出用的连接状态表。
pub struct ResourceRoute {
    core: Arc<UseCases>,
    registry: Arc<ConnectionRegistry>,
    /// 本机（Owner）node id：`remoteSessionRef.ownerNodeId` 的判定基准（见
    /// [`ResourceRoute::owner_matches`]）。
    node_id: NodeId,
    states: Mutex<BTreeMap<String, ConnectionState>>,
}

impl std::fmt::Debug for ResourceRoute {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResourceRoute")
            .field("connections", &lock(&self.states).len())
            .finish_non_exhaustive()
    }
}

impl ResourceRoute {
    /// 装配。`registry` 是连接注册表（扇出按 `connectionId` 找当前句柄）；`node_id` 是本机 node id，
    /// 组合根传 `Authority::local_node`（它也是握手 `node.ready.ownerNodeId` 的来源）。
    pub fn new(core: Arc<UseCases>, registry: Arc<ConnectionRegistry>, node_id: NodeId) -> Self {
        Self {
            core,
            registry,
            node_id,
            states: Mutex::new(BTreeMap::new()),
        }
    }

    /// 扇出分发循环（组合根 spawn，关闭序列参与）。
    ///
    /// 队列里是事件**定位**（`CommittedEvent`：id + 会话归属 + origin 三元组，**不含正文**）：正文在
    /// 分发期经会话绑定只读入口按 `eventId` 取回，因此投递链路不复制也不缓存正文。`shutdown` 触发即
    /// 结束循环；队列里剩余的事件不再投递——它们已经在 Owner 的事件日志里持久化，Access 重连后凭
    /// origin cursor 重放补齐。
    pub async fn dispatch(
        self: Arc<Self>,
        mut queue: mpsc::Receiver<CommittedEvent>,
        shutdown: Shutdown,
    ) {
        loop {
            let event = tokio::select! {
                biased;
                () = shutdown.clone().wait() => {
                    debug!(event = "node_link.fanout_shutdown", "the node-link event fan-out stops");
                    return;
                }
                received = queue.recv() => match received {
                    Some(event) => event,
                    None => return,
                },
            };
            self.fan_out(&event).await;
        }
    }

    /// 把一条已持久化的 owned 事件投递给所有订阅了该会话的连接。
    async fn fan_out(&self, event: &CommittedEvent) {
        let Some(session) = event.session.clone() else {
            return;
        };
        let targets = self.subscribers(&session);
        if targets.is_empty() {
            return;
        }
        // 正文按 (accessNodeId, exportId) 取回一次，供同一事件的所有目标复用（同一会话可能被不同的
        // Export 导出，各自的可见性独立判定）。
        let mut payloads: BTreeMap<(String, String), Option<WireEventPayload>> = BTreeMap::new();
        for (key, handle, attachment) in targets {
            let cache_key = (
                handle.node_id().as_str().to_owned(),
                attachment.export.as_str().to_owned(),
            );
            let payload = match payloads.get(&cache_key) {
                Some(cached) => cached.clone(),
                None => {
                    let fetched = match self
                        .core
                        .node_link_event_payload(
                            handle.node_id(),
                            &attachment.export,
                            &attachment.session,
                            &event.id,
                        )
                        .await
                    {
                        Ok(Some(payload)) => wire_payload(&payload),
                        Ok(None) => None,
                        Err(error) => {
                            warn!(
                                event = "node_link.fanout_payload_failed",
                                access_node_id = handle.node_id().as_str(),
                                event_id = event.id.as_str(),
                                error = ?error,
                                "the event payload could not be read; the access node replays it by cursor"
                            );
                            None
                        }
                    };
                    payloads.insert(cache_key, fetched.clone());
                    fetched
                }
            };
            let Some(payload) = payload else {
                warn!(
                    event = "node_link.fanout_payload_invalid",
                    access_node_id = handle.node_id().as_str(),
                    event_id = event.id.as_str(),
                    "the stored event payload is missing or cannot be mapped to a resource.event payload"
                );
                continue;
            };
            let Some(body) = event_body(event, &attachment.remote, payload) else {
                warn!(
                    event = "node_link.fanout_event_invalid",
                    event_id = event.id.as_str(),
                    "the event cannot be mapped to a resource.event"
                );
                continue;
            };
            if handle.send(MessageType::ResourceEvent, &body).is_err() {
                // 高水位或连接已结束：事件已持久化，Access 重连后凭 cursor 重放补齐（§2.5/D9）。
                debug!(
                    event = "node_link.fanout_skipped",
                    connection_id = handle.connection_id().as_str(),
                    event_id = event.id.as_str(),
                    "the connection cannot take the event right now"
                );
                self.forget_if_closed(&key);
            }
        }
    }

    /// 订阅了该会话的连接（顺序按 `connectionId` 稳定）。
    ///
    /// 连接结束时 `conn` 会从注册表移除句柄，但不会通知路由；这里以「注册表里还在」为活跃判据，因此
    /// 已结束连接的状态表条目不会被再次投递。同时**顺带回收**句柄已不在注册表里的条目
    /// （[`ResourceRoute::forget_if_closed`] 只在投递失败且句柄已摘除时清理，正常结束的连接永远走不到
    /// 那条路）：回收判据在订阅判定**之前**，因此「attach 后从未 subscribe」与「re-attach 清掉订阅」
    /// 这两类连接也一并回收（它们不再持有任何本会话订阅，只看订阅的话永远扫不到）；不回收的话状态表
    /// 会按历史连接数单调增长，每次扇出的扫描成本随之线性上升。回收只发生在扇出时（没有事件要投递的
    /// 连接不产生任何成本），因此它是惰性的、不需要额外的生命周期回调。
    fn subscribers(&self, session: &SessionId) -> Vec<(String, Arc<ConnectionHandle>, Attachment)> {
        let handles = self.registry.handles();
        let mut states = lock(&self.states);
        let mut targets = Vec::new();
        let mut ended = Vec::new();
        for (key, state) in states.iter() {
            let Some(handle) = handles
                .iter()
                .find(|handle| handle.connection_id().as_str() == key)
            else {
                // 连接已结束（见本方法的说明）：登记回收，不在遍历中改动表。
                ended.push(key.clone());
                continue;
            };
            let Some(subscription) = state.subscriptions.get(session.as_str()) else {
                continue;
            };
            let Some(attachment) = state.attachments.get(session.as_str()) else {
                continue;
            };
            if attachment.id != subscription.attachment_id
                || attachment.generation != subscription.generation
            {
                continue;
            }
            targets.push((key.clone(), Arc::clone(handle), attachment.clone()));
        }
        for key in ended {
            states.remove(&key);
        }
        targets
    }

    /// 投递失败后清理由该连接持有的状态（惰性清理，见 [`ResourceRoute::subscribers`] 的说明）。
    fn forget_if_closed(&self, key: &str) {
        let alive = self
            .registry
            .handles()
            .iter()
            .any(|handle| handle.connection_id().as_str() == key);
        if !alive {
            lock(&self.states).remove(key);
        }
    }

    /// 取该连接的资源状态（不存在则建），并在持锁期间执行 `action`。
    ///
    /// 状态表是纯内存的协议状态：临界区里只有映射操作，因此不会把锁带过 `await`。
    fn with_state<T>(
        &self,
        handle: &ConnectionHandle,
        action: impl FnOnce(&mut ConnectionState) -> T,
    ) -> T {
        let mut states = lock(&self.states);
        let state = states
            .entry(handle.connection_id().as_str().to_owned())
            .or_default();
        action(state)
    }
}

#[async_trait::async_trait]
impl MessageRoute for ResourceRoute {
    async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        match message.message_type() {
            MessageType::ResourceAttach => match decode::<Attach>(message) {
                Some(request) => self.on_attach(session, &request, message).await,
                None => self.reject(session, ErrorCode::ProtocolSchemaInvalid, message),
            },
            MessageType::ResourceSubscribe => match decode::<Subscribe>(message) {
                Some(request) => self.on_subscribe(session, &request, message).await,
                None => self.reject(session, ErrorCode::ProtocolSchemaInvalid, message),
            },
            MessageType::ResourceAck => match decode::<Ack>(message) {
                Some(request) => self.on_ack(session, &request, message).await,
                None => self.reject(session, ErrorCode::ProtocolSchemaInvalid, message),
            },
            _ => RouteOutcome::Unclaimed,
        }
    }
}

impl ResourceRoute {
    // -----------------------------------------------------------------------------------------
    // resource.attach
    // -----------------------------------------------------------------------------------------

    /// `resource.attach`：可见性/撤销复核 → 签发新的 attachment → `resource.attached`。
    async fn on_attach(
        &self,
        handle: &ConnectionHandle,
        request: &Attach,
        message: &Envelope,
    ) -> RouteOutcome {
        let remote = &request.remote_session_ref;
        // ① 复合身份的本机分量复核：`ownerNodeId` 必须是本机（Owner）——否则这个 `remoteSessionRef`
        //    指向的是另一个 Owner 的会话，本机不签发任何 attachment；与「会话不存在」同码，不泄露差别。
        if !self.owner_matches(remote) {
            return self.protocol_error(handle, ErrorCode::ExportNotFound, message);
        }
        let (Some(export), Some(session)) = (
            export_id(remote.export_id.as_str()),
            session_id(remote.session_id.as_str()),
        ) else {
            return self.protocol_error(handle, ErrorCode::ExportNotFound, message);
        };
        // ② 可见性复核（D14 的唯一判定点）：未知/未配对的节点、未导出的 Export、已撤销的 Export，
        //    以及与该节点 grants 不相交的 Export 都在这里被挡住。
        match catalog::export_is_visible(&self.core, handle.node_id(), &export).await {
            Ok(true) => {}
            Ok(false) => {
                return self.protocol_error(handle, ErrorCode::ExportNotGranted, message);
            }
            Err(error) => return self.fault(handle, &error, message),
        }
        // ③ 会话归属复核（core 的会话绑定只读入口同时校验对端是已配对的 access 行、Export 未撤销、
        //    会话的 Agent 属于该 Export）。
        let view = match self
            .core
            .node_link_session_view(handle.node_id(), &export, &session)
            .await
        {
            Ok(view) => view,
            Err(error) => return self.attach_fault(handle, &error, message),
        };
        let Some(session_meta) = session_meta(&view.session) else {
            return self.fault(handle, &PortError::Corrupt("session state"), message);
        };
        // ③ 签发新的 attachment/generation：同一连接内 generation 单调递增，旧的那个被覆盖
        //    （旧 frame 随后找不到当前 attachment → `attach_generation_stale`）；重新 attach 同时清掉
        //    该会话的旧订阅——Access 必须重新 `resource.subscribe` 才恢复事件流（§12.4）。
        let Some(attachment_id) = message_id(&self.core) else {
            return self.fault(
                handle,
                &PortError::Corrupt("id generator returned a non-uuid"),
                message,
            );
        };
        let generation = self.with_state(handle, |state| {
            let generation = state.next_generation + 1;
            state.next_generation = generation;
            state.attachments.insert(
                session.as_str().to_owned(),
                Attachment {
                    id: attachment_id.clone(),
                    generation,
                    export: export.clone(),
                    session: session.clone(),
                    remote: remote.clone(),
                },
            );
            state.subscriptions.remove(session.as_str());
            generation
        });
        let Some(generation) = DecimalString::parse(&generation.to_string()).ok() else {
            return self.fault(handle, &PortError::Corrupt("generation"), message);
        };
        debug!(
            event = "node_link.resource_attached",
            access_node_id = handle.node_id().as_str(),
            session_id = session.as_str(),
            attachment_id = attachment_id.as_str(),
            attachment_generation = generation.as_str(),
            "a new attachment was issued"
        );
        let body = Attached {
            attachment_id,
            attachment_generation: generation,
            session_meta,
        };
        if handle.send(MessageType::ResourceAttached, &body).is_err() {
            debug!(
                event = "node_link.resource_attached_not_sent",
                connection_id = handle.connection_id().as_str(),
                "the attachment could not be queued"
            );
        }
        RouteOutcome::Claimed
    }

    // -----------------------------------------------------------------------------------------
    // resource.subscribe
    // -----------------------------------------------------------------------------------------

    /// `resource.subscribe`：`cursor = null` 走快照，非空走 origin 增量重放；两者都登记订阅。
    async fn on_subscribe(
        &self,
        handle: &ConnectionHandle,
        request: &Subscribe,
        message: &Envelope,
    ) -> RouteOutcome {
        let Some(attachment) = self.current_attachment(handle, request) else {
            // 未知的 attachmentId、generation 不符、或已被新 attach 覆盖：一律按「代际已过期」拒绝。
            return self.protocol_error(handle, ErrorCode::ResourceAttachGenerationStale, message);
        };
        self.subscribe(handle, &attachment);
        match request.cursor.as_ref() {
            None => self.snapshot(handle, &attachment, message).await,
            Some(cursor) => {
                let Some(cursor) = core_cursor(cursor) else {
                    return self.protocol_error(handle, ErrorCode::ProtocolSchemaInvalid, message);
                };
                self.replay(handle, &attachment, cursor, message).await
            }
        }
    }

    /// 登记订阅（扇出据此投递）。
    fn subscribe(&self, handle: &ConnectionHandle, attachment: &Attachment) {
        let session = attachment.session.as_str().to_owned();
        let subscription = Subscription {
            attachment_id: attachment.id.clone(),
            generation: attachment.generation,
        };
        self.with_state(handle, |state| {
            state.subscriptions.insert(session, subscription);
        });
    }

    /// 快照流程（`snapshot_begin` → `snapshot_chunk*` → `snapshot_end`）。
    async fn snapshot(
        &self,
        handle: &ConnectionHandle,
        attachment: &Attachment,
        message: &Envelope,
    ) -> RouteOutcome {
        let view = match self
            .core
            .node_link_session_view(handle.node_id(), &attachment.export, &attachment.session)
            .await
        {
            Ok(view) => view,
            Err(error) => return self.attach_fault(handle, &error, message),
        };
        let Some(session_meta) = session_meta(&view.session) else {
            return self.fault(handle, &PortError::Corrupt("session state"), message);
        };
        let Some(cursor) = wire_cursor(&view) else {
            return self.fault(handle, &PortError::Corrupt("origin cursor"), message);
        };
        let Some(snapshot_id) = message_id(&self.core) else {
            return self.fault(
                handle,
                &PortError::Corrupt("id generator returned a non-uuid"),
                message,
            );
        };
        // ① items：session_meta 一条（schema 上限也是 1）+ 未决交互（含创建事件的 payloadDigest）。
        let session_meta_item = SnapshotItemSessionMeta {
            session_ref: attachment.remote.clone(),
            session_meta,
        };
        let mut interaction_items = Vec::with_capacity(view.pending_interactions.len());
        let mut omitted = 0_usize;
        for pending in &view.pending_interactions {
            match self.pending_interaction(handle, attachment, pending).await {
                Some(body) => interaction_items.push(body),
                // 单条的失败原因由 `pending_interaction` 记日志；这里累计，循环后汇总一条。
                None => omitted += 1,
            }
        }
        if omitted > 0 {
            warn!(
                event = "node_link.snapshot_items_omitted",
                access_node_id = handle.node_id().as_str(),
                resource = "pending_interactions",
                omitted = omitted,
                total = view.pending_interactions.len(),
                "the snapshot omits pending interactions that cannot be mapped; the access node sees a shorter snapshot"
            );
        }
        // ② 切分：`resourceSnapshotBatchSize`（schema 的 maxItems 是 500，限额只会更小）。
        let batch = handle.limits().resource_snapshot_batch_size() as usize;
        let batch = batch.max(1);
        let interaction_batches: Vec<&[PendingInteraction]> =
            interaction_items.chunks(batch).collect();
        let chunk_count = 1 + interaction_batches.len() as u64;
        let Some(begin) = snapshot_begin(snapshot_id.clone(), cursor.clone(), chunk_count) else {
            return self.fault(handle, &PortError::Corrupt("snapshot begin"), message);
        };
        if handle
            .send(MessageType::ResourceSnapshotBegin, &begin)
            .is_err()
        {
            debug!(
                event = "node_link.snapshot_paused",
                connection_id = handle.connection_id().as_str(),
                "the snapshot begin could not be queued"
            );
            return RouteOutcome::Claimed;
        }
        // ③ chunks：digest 必须对**将要发出的完整帧文本**逐条求摘要，因此用 `send_with_frame`。
        let mut digests = Vec::with_capacity(chunk_count as usize);
        let mut chunk_index = 0_u64;
        let mut chunk = |items: SnapshotItems| {
            let Some(index) = DecimalString::parse(&chunk_index.to_string()).ok() else {
                return Err(());
            };
            chunk_index += 1;
            Ok(SnapshotChunk::new(snapshot_id.clone(), index, items))
        };
        let session_meta_chunk = match chunk(SnapshotItems::SessionMeta(vec![session_meta_item])) {
            Ok(chunk) => chunk,
            Err(()) => return self.fault(handle, &PortError::Corrupt("chunk index"), message),
        };
        match self.send_chunk(handle, &session_meta_chunk) {
            Ok(frame) => digests.push(Sha256::digest(frame.as_bytes())),
            Err(()) => return RouteOutcome::Claimed,
        }
        for items in interaction_batches {
            let chunk = match chunk(SnapshotItems::PendingInteractions(items.to_vec())) {
                Ok(chunk) => chunk,
                Err(()) => return self.fault(handle, &PortError::Corrupt("chunk index"), message),
            };
            match self.send_chunk(handle, &chunk) {
                Ok(frame) => digests.push(Sha256::digest(frame.as_bytes())),
                Err(()) => return RouteOutcome::Claimed,
            }
        }
        if digests.len() != chunk_count as usize {
            warn!(
                event = "node_link.snapshot_incomplete",
                connection_id = handle.connection_id().as_str(),
                expected = chunk_count,
                sent = digests.len(),
                "the snapshot could not be completed; the access node must resubscribe"
            );
            return RouteOutcome::Claimed;
        }
        let Some(snapshot_digest) = snapshot_digest(&digests) else {
            return self.fault(handle, &PortError::Corrupt("snapshot digest"), message);
        };
        let Some(end) = snapshot_end(snapshot_id, cursor, chunk_count, snapshot_digest) else {
            return self.fault(handle, &PortError::Corrupt("snapshot end"), message);
        };
        if handle.send(MessageType::ResourceSnapshotEnd, &end).is_err() {
            debug!(
                event = "node_link.snapshot_paused",
                connection_id = handle.connection_id().as_str(),
                "the snapshot end could not be queued"
            );
        }
        debug!(
            event = "node_link.snapshot_sent",
            access_node_id = handle.node_id().as_str(),
            chunks = chunk_count,
            "the resource snapshot was queued"
        );
        RouteOutcome::Claimed
    }

    /// 发送一个 chunk 并回传它实际入队的帧文本（digest 的前像）。
    fn send_chunk(&self, handle: &ConnectionHandle, chunk: &SnapshotChunk) -> Result<String, ()> {
        if handle.saturated() {
            debug!(
                event = "node_link.snapshot_paused",
                connection_id = handle.connection_id().as_str(),
                pending_messages = handle.pending().messages,
                "the connection is above the queue high water mark; the snapshot stops early"
            );
            return Err(());
        }
        handle
            .send_with_frame(MessageType::ResourceSnapshotChunk, chunk)
            .map_err(|fault| {
                debug!(
                    event = "node_link.snapshot_chunk_not_sent",
                    connection_id = handle.connection_id().as_str(),
                    fault = ?fault,
                    "the snapshot chunk could not be queued"
                );
            })
    }

    /// 未决交互 item：`payloadDigest` 是**创建该交互的 origin 事件的 payload** 的 ACPR-CJ1 摘要。
    ///
    /// 映射任一步失败都会让该 item **不进快照**（对端因此少看到一条未决交互），所以这条省略必须留有
    /// 可观测的记录：只带标识、不含任何正文（与 `node_link.fanout_event_invalid` 同一口径）。
    async fn pending_interaction(
        &self,
        handle: &ConnectionHandle,
        attachment: &Attachment,
        pending: &acp_core::use_cases::NodeLinkPendingInteraction,
    ) -> Option<PendingInteraction> {
        let mapped = self
            .map_pending_interaction(handle, attachment, pending)
            .await;
        if mapped.is_none() {
            warn!(
                event = "node_link.snapshot_pending_interaction_unmappable",
                access_node_id = handle.node_id().as_str(),
                session_id = attachment.session.as_str(),
                interaction_id = pending.interaction.id().as_str(),
                origin_event_id = pending.origin_event.as_str(),
                "the pending interaction cannot be mapped to a snapshot item; it is left out of the snapshot"
            );
        }
        mapped
    }

    /// [`ResourceRoute::pending_interaction`] 的映射体（任一步失败都返回 `None`，调用方负责记日志）。
    async fn map_pending_interaction(
        &self,
        handle: &ConnectionHandle,
        attachment: &Attachment,
        pending: &acp_core::use_cases::NodeLinkPendingInteraction,
    ) -> Option<PendingInteraction> {
        let payload = self
            .core
            .node_link_event_payload(
                handle.node_id(),
                &attachment.export,
                &attachment.session,
                &pending.origin_event,
            )
            .await
            .ok()
            .flatten()?;
        let payload = wire_payload(&payload)?;
        Some(PendingInteraction {
            interaction_id: Uuid::parse(pending.interaction.id().as_str()).ok()?,
            kind: match pending.interaction.kind() {
                InteractionKind::Permission => WireInteractionKind::Permission,
                InteractionKind::Elicitation => WireInteractionKind::Elicitation,
            },
            created_at: Timestamp::parse(pending.interaction.created_at().as_str()).ok()?,
            payload_digest: payload_digest(&payload)?,
        })
    }

    /// 增量重放：从该 origin cursor 之后的事件继续（epoch 不一致一律拒绝）。
    async fn replay(
        &self,
        handle: &ConnectionHandle,
        attachment: &Attachment,
        cursor: CoreOriginCursor,
        message: &Envelope,
    ) -> RouteOutcome {
        let limit = ReplayLimit::new(handle.limits().resource_snapshot_batch_size());
        let replay = match self
            .core
            .node_link_replay(
                handle.node_id(),
                &attachment.export,
                &attachment.session,
                Some(cursor),
                limit,
            )
            .await
        {
            Ok(replay) => replay,
            Err(error) => return self.attach_fault(handle, &error, message),
        };
        if self.send_events(handle, attachment, &replay).is_err() {
            return RouteOutcome::Claimed;
        }
        RouteOutcome::Claimed
    }

    /// 发送一批 origin 事件（快照之后的续读与增量重放共用）。
    fn send_events(
        &self,
        handle: &ConnectionHandle,
        attachment: &Attachment,
        replay: &NodeLinkReplay,
    ) -> Result<(), ()> {
        for event in &replay.events {
            let Some(payload) = wire_payload(&event.payload) else {
                warn!(
                    event = "node_link.replay_event_payload_invalid",
                    access_node_id = handle.node_id().as_str(),
                    event_id = event.event_id.as_str(),
                    "the replayed event payload cannot be mapped to a resource.event payload"
                );
                continue;
            };
            let Some(body) = replay_event_body(event, &attachment.remote, payload) else {
                warn!(
                    event = "node_link.replay_event_invalid",
                    access_node_id = handle.node_id().as_str(),
                    event_id = event.event_id.as_str(),
                    "the replayed event cannot be mapped to a resource.event"
                );
                continue;
            };
            if handle.send(MessageType::ResourceEvent, &body).is_err() {
                debug!(
                    event = "node_link.replay_paused",
                    connection_id = handle.connection_id().as_str(),
                    "the replay stopped early; the access node resumes from its last cursor"
                );
                return Err(());
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // resource.ack
    // -----------------------------------------------------------------------------------------

    /// `resource.ack`：归属 + epoch + 单调性全部成立才记账；任何一条不成立都是 `sequence_invalid`。
    async fn on_ack(
        &self,
        handle: &ConnectionHandle,
        request: &Ack,
        message: &Envelope,
    ) -> RouteOutcome {
        let (Some(export), Some(session)) = (
            export_id(request.session_ref.export_id.as_str()),
            session_id(request.session_ref.session_id.as_str()),
        ) else {
            return self.protocol_error(handle, ErrorCode::ProtocolSequenceInvalid, message);
        };
        let Some(cursor) = core_cursor(&request.cursor) else {
            return self.protocol_error(handle, ErrorCode::ProtocolSchemaInvalid, message);
        };
        // ① 归属：`sessionRef` 必须指向本连接当前 attachment 所属的会话（含 export 与 owner 一致）。
        //    owner 分量先判：`ownerNodeId` 不等于本机时，这个 `sessionRef` 就不等于本连接 attachment 的
        //    sessionRef（§12.4：「`sessionRef` 不属于本连接」→ `sequence_invalid`），因此与「没有
        //    attachment」「epoch 不符」同码，一律不记账。
        if !self.owner_matches(&request.session_ref) {
            return self.protocol_error(handle, ErrorCode::ProtocolSequenceInvalid, message);
        }
        //    存在性：本连接当前必须真的持有该会话的 attachment（没有 attachment 与 epoch 不符同码，
        //    一律不记账）。
        if self.attachment_for(handle, &export, &session).is_none() {
            return self.protocol_error(handle, ErrorCode::ProtocolSequenceInvalid, message);
        }
        // ② epoch 必须与该会话当前 origin epoch 一致。
        let head = match self
            .core
            .node_link_session_view(handle.node_id(), &export, &session)
            .await
        {
            Ok(view) => view.head,
            Err(error) => return self.attach_fault(handle, &error, message),
        };
        if head.origin_epoch != cursor.origin_epoch {
            return self.protocol_error(handle, ErrorCode::ProtocolSequenceInvalid, message);
        }
        // ③ 单调不减：回退不改写已有水位。
        let accepted = self.with_state(handle, |state| match state.acks.get(session.as_str()) {
            Some(existing) if existing.origin_sequence >= cursor.origin_sequence => false,
            _ => {
                state
                    .acks
                    .insert(session.as_str().to_owned(), cursor.clone());
                true
            }
        });
        if !accepted {
            return self.protocol_error(handle, ErrorCode::ProtocolSequenceInvalid, message);
        }
        debug!(
            event = "node_link.resource_acked",
            access_node_id = handle.node_id().as_str(),
            session_id = session.as_str(),
            origin_sequence = cursor.origin_sequence.get(),
            "the access node acknowledged the origin cursor"
        );
        RouteOutcome::Claimed
    }

    // -----------------------------------------------------------------------------------------
    // 状态查询与错误映射
    // -----------------------------------------------------------------------------------------

    /// `remoteSessionRef` 的本机分量判定：复合身份的 `ownerNodeId` 必须指向本机（Owner）。
    ///
    /// 不成立时该 ref 指的不是本机的会话：「一个状态只能有一个权威写入者」要求本机只认自己的资源，
    /// 而且回显一个对端自报的 owner 分量会把伪造身份变成事实（因此两处入口都在签发/记账**之前**比对）。
    fn owner_matches(&self, remote: &RemoteSessionRef) -> bool {
        remote.owner_node_id.as_str() == self.node_id.as_str()
    }

    /// 当前生效的 attachment（`resource.subscribe` 的前置）：必须存在同 `(id, generation)` 的条目。
    fn current_attachment(
        &self,
        handle: &ConnectionHandle,
        request: &Subscribe,
    ) -> Option<Attachment> {
        let generation = request.attachment_generation.as_str().parse::<u64>().ok()?;
        let state = lock(&self.states);
        let state = state.get(handle.connection_id().as_str())?;
        state
            .attachments
            .values()
            .find(|attachment| {
                attachment.id == request.attachment_id && attachment.generation == generation
            })
            .cloned()
    }

    /// 本连接上属于 `(export, session)` 的当前 attachment。
    fn attachment_for(
        &self,
        handle: &ConnectionHandle,
        export: &ExportId,
        session: &SessionId,
    ) -> Option<Attachment> {
        let state = lock(&self.states);
        let state = state.get(handle.connection_id().as_str())?;
        state
            .attachments
            .get(session.as_str())
            .filter(|attachment| &attachment.export == export)
            .cloned()
    }

    /// WP6（`design.md` D7/D8）的 additive seam：session-scoped 命令复核「当前 attachment」。
    ///
    /// 返回该 `(attachmentId, attachmentGeneration)` 在当前连接上生效的 `remoteSessionRef`；任何不匹配
    /// （未知 id、过期代际、已被新 `resource.attach` 覆盖、属于别会话）都返回 `None`——调用方据此回
    /// `attach_generation_stale`。attachment 表只有这一份（WP5 持有），命令层不得另建镜像。
    pub fn current_attachment_session_ref(
        &self,
        handle: &ConnectionHandle,
        attachment_id: &Uuid,
        generation: u64,
    ) -> Option<RemoteSessionRef> {
        let state = lock(&self.states);
        let state = state.get(handle.connection_id().as_str())?;
        state
            .attachments
            .values()
            .find(|attachment| {
                attachment.id == *attachment_id && attachment.generation == generation
            })
            .map(|attachment| attachment.remote.clone())
    }

    /// WP6（`design.md` D7、R77）的 additive seam：撤销 Export 后清除内存中的 attachment 与订阅，并
    /// 返回受影响的 `connectionId`。
    ///
    /// 清除是「旧连接不得继续取资源」的内存面（授权面由逐消息的持久化复核保证）；调用方（撤销传播）用
    /// 返回的 id 从连接注册表解析句柄并推送 `export.revoked`。已结束连接的条目顺带回收，与扇出路径的
    /// 惰性回收同一标准。
    pub fn revoke_export(&self, export: &ExportId) -> Vec<String> {
        let mut states = lock(&self.states);
        let mut affected = Vec::new();
        for (key, state) in states.iter_mut() {
            let sessions: Vec<String> = state
                .attachments
                .iter()
                .filter(|(_, attachment)| &attachment.export == export)
                .map(|(session, _)| session.clone())
                .collect();
            if sessions.is_empty() {
                continue;
            }
            affected.push(key.clone());
            for session in sessions {
                state.attachments.remove(&session);
                state.subscriptions.remove(&session);
            }
        }
        affected.sort();
        affected
    }

    /// 可恢复的协议拒绝（连接保持可用）。
    fn protocol_error(
        &self,
        handle: &ConnectionHandle,
        code: ErrorCode,
        message: &Envelope,
    ) -> RouteOutcome {
        let text = match code {
            ErrorCode::ExportNotFound => "the requested session is not exported to this node",
            ErrorCode::ExportNotGranted => "the requested session is not granted to this node",
            ErrorCode::ResourceAttachGenerationStale => {
                "the attachment is not the current one; attach again before subscribing"
            }
            ErrorCode::ProtocolSequenceInvalid => {
                "the cursor is not usable for this session on this connection"
            }
            _ => "the resource request does not match the v1 schema",
        };
        self.send_error(handle, code, text, message);
        RouteOutcome::Claimed
    }

    /// 会话读失败的映射：未导出/未授权是协议级拒绝，其余是内部故障。
    fn attach_fault(
        &self,
        handle: &ConnectionHandle,
        error: &PortError,
        message: &Envelope,
    ) -> RouteOutcome {
        match error {
            PortError::NotFound(EntityRef::Export(_) | EntityRef::Session(_)) => {
                self.protocol_error(handle, ErrorCode::ExportNotFound, message)
            }
            PortError::InvalidRequest("export.not_granted") => {
                self.protocol_error(handle, ErrorCode::ExportNotGranted, message)
            }
            PortError::InvalidRequest("authorization.scope_denied") => {
                self.protocol_error(handle, ErrorCode::ExportNotGranted, message)
            }
            PortError::InvalidRequest("nodelink.origin_epoch_mismatch") => {
                self.protocol_error(handle, ErrorCode::ProtocolSequenceInvalid, message)
            }
            other => self.fault(handle, other, message),
        }
    }

    /// 内部故障：显式回 `link.error`（不静默），连接保持可用。
    fn fault(
        &self,
        handle: &ConnectionHandle,
        error: &PortError,
        message: &Envelope,
    ) -> RouteOutcome {
        warn!(
            event = "node_link.resource_failed",
            access_node_id = handle.node_id().as_str(),
            error = ?error,
            "the resource request could not be served"
        );
        self.send_error(
            handle,
            ErrorCode::InternalUnavailable,
            "the owner node cannot serve the resource request",
            message,
        );
        RouteOutcome::Claimed
    }

    fn reject(
        &self,
        handle: &ConnectionHandle,
        code: ErrorCode,
        message: &Envelope,
    ) -> RouteOutcome {
        self.send_error(
            handle,
            code,
            "the resource request does not match the v1 schema",
            message,
        );
        RouteOutcome::Claimed
    }

    fn send_error(
        &self,
        handle: &ConnectionHandle,
        code: ErrorCode,
        text: &'static str,
        envelope: &Envelope,
    ) {
        let Some(body) = link_error(code, text, Some(envelope.message_id())) else {
            warn!(
                event = "node_link.error_body_failed",
                code = code.as_str(),
                "the link.error body cannot be assembled"
            );
            return;
        };
        debug!(
            event = "node_link.resource_rejected",
            code = code.as_str(),
            "the resource request was rejected without closing the connection"
        );
        let _ = handle.send(MessageType::LinkError, &body);
    }
}

/// 事件扇出的同步入口：把 broker 的 owned 投递投进有界 channel（只带定位，不带正文）。
///
/// 组合根（WP7）用分叉的 `EventPublisher` 装配它（D6）：一路保持既有语义，一路转发到这里。
/// `publish` 是同步方法且不得阻塞 broker，因此队列满时只记日志并丢弃这一条——事件已在 Owner 的
/// 持久化日志里，Access 重连后凭 origin cursor 重放补齐（与慢连接的处置同口径，§2.5）。
pub struct NodeLinkPublisher {
    queue: mpsc::Sender<CommittedEvent>,
}

impl NodeLinkPublisher {
    /// 建立「发布端 ↔ 分发循环」的通道。
    pub fn channel() -> (Self, mpsc::Receiver<CommittedEvent>) {
        let (queue, receiver) = mpsc::channel(EVENT_QUEUE_CAPACITY);
        (Self { queue }, receiver)
    }
}

impl EventPublisher for NodeLinkPublisher {
    fn publish(&self, delivery: CommittedDelivery) {
        // 只映射 owned：imported 投递不跨节点再导出（§4 拓扑规则）。
        let CommittedDelivery::Owned(event) = delivery else {
            return;
        };
        if event.session.is_none() {
            // 非会话级事件（node 作用域）没有会话级 origin cursor，也不属于任何 attachment。
            return;
        }
        if let Err(error) = self.queue.try_send(event) {
            match error {
                mpsc::error::TrySendError::Full(event) => warn!(
                    event = "node_link.fanout_queue_full",
                    event_id = event.id.as_str(),
                    "the node-link fan-out queue is full; the access node replays the event by cursor"
                ),
                // 分发循环已随关闭序列结束：此时不投递是正确行为（事件已持久化，重连后按 cursor 重放）。
                mpsc::error::TrySendError::Closed(event) => debug!(
                    event = "node_link.fanout_closed",
                    event_id = event.id.as_str(),
                    "the node-link fan-out is shutting down"
                ),
            }
        }
    }
}

/// 把 core 的事件 payload 投影成 wire payload（`payload.view` 逐字节、`payload.acp` 按上限降级）。
fn wire_payload(payload: &CoreEventPayload) -> Option<WireEventPayload> {
    let view = RawObject::parse(payload.view.as_str()).ok()?;
    let acp = match payload.acp.as_ref() {
        Some(AcpRaw::Available {
            media_type,
            raw_json,
            byte_length,
            sha256,
        }) => {
            let digest = Base64Url::<32>::parse(sha256.as_str()).ok()?;
            if media_type != "application/json" || raw_json.len() > MAX_INLINE_ACP_BYTES {
                // ① wire 的 `rawAcp.mediaType` 是固定值 `application/json`：其它 media type 无法诚实
                //    表达；② 超过 256 KiB 的内嵌内容必须显式降级而不是截断（§12.4）。
                let reason = if media_type != "application/json" {
                    RawAcpUnavailableReason::StorageFailure
                } else {
                    RawAcpUnavailableReason::SizeLimit
                };
                Some(RawAcp::Unavailable {
                    reason,
                    byte_length: DecimalString::parse(&byte_length.to_string()).ok()?,
                    sha256: Nullable::from_option(Some(digest)),
                })
            } else {
                // `rawJson` 是 JSON **字符串**（内容是 ACP 原文档文本），因此这里编码一次字符串字面量。
                let quoted = serde_json::to_string(raw_json).ok()?;
                Some(RawAcp::Complete {
                    raw_json: serde_json::value::RawValue::from_string(quoted).ok()?,
                    byte_length: DecimalString::parse(&byte_length.to_string()).ok()?,
                    sha256: digest,
                })
            }
        }
        Some(AcpRaw::Unavailable {
            reason,
            byte_length,
            sha256,
        }) => Some(RawAcp::Unavailable {
            reason: match reason {
                RawUnavailableReason::SizeLimit => RawAcpUnavailableReason::SizeLimit,
                RawUnavailableReason::RetentionExpired => RawAcpUnavailableReason::RetentionExpired,
                RawUnavailableReason::StorageFailure => RawAcpUnavailableReason::StorageFailure,
            },
            byte_length: DecimalString::parse(&byte_length.to_string()).ok()?,
            sha256: match sha256 {
                Some(digest) => {
                    Nullable::from_option(Some(Base64Url::<32>::parse(digest.as_str()).ok()?))
                }
                None => Nullable::null(),
            },
        }),
        None => None,
    };
    WireEventPayload::new(Some(view), acp).ok()
}

/// `payloadDigest = base64url(SHA-256(ACPR-CJ1(payload)))`（`SYNC_PROTOCOL.md` §3.3/§9.4）。
///
/// 前像必须是**要发出去的那份 payload 对象**（ACPR-CJ1 会规范化成员顺序与转义，因此与 JSON 渲染顺序
/// 无关）；计算失败（含超出 ACPR-CJ1 取值域）返回 `None`，调用方按「无法形成 payload」处置。
fn payload_digest(payload: &WireEventPayload) -> Option<Base64Url<32>> {
    let text = serde_json::to_string(payload).ok()?;
    let canonical = acpr_wire::cj1::canonicalize(&text).ok()?;
    let digest = Sha256::digest(canonical.as_bytes());
    Base64Url::<32>::parse(&base64url(&digest)).ok()
}

/// 扇出路径的事件映射（已持久化的 `CommittedEvent` + 取回的正文）。
fn event_body(
    event: &CommittedEvent,
    session_ref: &RemoteSessionRef,
    payload: WireEventPayload,
) -> Option<Event> {
    let payload = ensure_digestable(payload)?;
    Some(Event {
        origin_event_id: Uuid::parse(event.id.as_str()).ok()?,
        origin_epoch: Uuid::parse(event.origin_epoch.as_ref()?.as_str()).ok()?,
        origin_sequence: DecimalString::parse(&event.origin_sequence?.get().to_string()).ok()?,
        session_ref: session_ref.clone(),
        event_type: WireEventType::parse(event.event_type.as_str()).ok()?,
        payload_digest: payload_digest(&payload)?,
        created_at: Timestamp::parse(event.created_at.as_str()).ok()?,
        payload,
    })
}

/// 重放路径的事件映射（core 的 `NodeLinkEvent`，origin 三元组已就位）。
fn replay_event_body(
    event: &NodeLinkEvent,
    session_ref: &RemoteSessionRef,
    payload: WireEventPayload,
) -> Option<Event> {
    let payload = ensure_digestable(payload)?;
    Some(Event {
        origin_event_id: Uuid::parse(event.event_id.as_str()).ok()?,
        origin_epoch: Uuid::parse(event.origin_epoch.as_str()).ok()?,
        origin_sequence: DecimalString::parse(&event.origin_sequence.get().to_string()).ok()?,
        session_ref: session_ref.clone(),
        event_type: WireEventType::parse(event.event_type.as_str()).ok()?,
        payload_digest: payload_digest(&payload)?,
        created_at: Timestamp::parse(event.created_at.as_str()).ok()?,
        payload,
    })
}

/// 保证 payload 能算出 ACPR-CJ1 摘要。
///
/// `view` 在落库时已经过一次 ACPR-CJ1（`storage-sqlite` 的 `payload_digest`），因此这条路径实际不可达；
/// 一旦不可达被打破，就把 payload 降级为「不带 view、原文不可用」——摘要仍与实际发出的字节一致，
/// 既不发一个算不出摘要的事件，也不假装内容完整。
fn ensure_digestable(payload: WireEventPayload) -> Option<WireEventPayload> {
    if payload_digest(&payload).is_some() {
        return Some(payload);
    }
    warn!(
        event = "node_link.payload_not_canonicalizable",
        "the event payload cannot be canonicalized; the raw document is reported as unavailable"
    );
    WireEventPayload::new(
        None,
        Some(RawAcp::Unavailable {
            reason: RawAcpUnavailableReason::StorageFailure,
            byte_length: DecimalString::parse("0").ok()?,
            sha256: Nullable::null(),
        }),
    )
    .ok()
}

/// `snapshotDigest`：逐 chunk 对**完整帧文本**求 SHA-256，按 index 连接后再求一次 SHA-256（§12.4）。
fn snapshot_digest(chunks: &[sha2::digest::Output<Sha256>]) -> Option<Base64Url<32>> {
    let mut hasher = Sha256::new();
    for digest in chunks {
        hasher.update(digest);
    }
    Base64Url::<32>::parse(&base64url(&hasher.finalize())).ok()
}

fn snapshot_begin(
    snapshot_id: Uuid,
    cursor: OriginCursor,
    chunk_count: u64,
) -> Option<SnapshotBegin> {
    Some(SnapshotBegin {
        snapshot_id,
        cursor,
        schema_version: ProtocolVersionV1::new(1).ok()?,
        chunk_count: node_link_protocol::common::BoundedU64::<0, 100_000>::new(chunk_count).ok()?,
    })
}

fn snapshot_end(
    snapshot_id: Uuid,
    cursor: OriginCursor,
    chunk_count: u64,
    snapshot_digest: Base64Url<32>,
) -> Option<SnapshotEnd> {
    Some(SnapshotEnd {
        snapshot_id,
        cursor,
        chunk_count: node_link_protocol::common::BoundedU64::<0, 100_000>::new(chunk_count).ok()?,
        snapshot_digest,
    })
}

/// 快照的 `sessionMeta`（只承载 `state`/`version`）。
///
/// `pub(crate)` 是因为 WP6 的 `session.create` 终态结果要用同一个映射（`SessionCreateResult.sessionMeta`）——
/// 两处必须同形，不复制第二份。
pub(crate) fn session_meta(summary: &acp_core::model::SessionSummary) -> Option<SessionMeta> {
    let state = match summary.state() {
        CoreSessionState::Idle => WireSessionState::Idle,
        CoreSessionState::Queued => WireSessionState::Queued,
        CoreSessionState::Running => WireSessionState::Running,
        CoreSessionState::WaitingInput => WireSessionState::WaitingInput,
        CoreSessionState::WaitingPermission => WireSessionState::WaitingPermission,
        CoreSessionState::Failed => WireSessionState::Failed,
        CoreSessionState::Closed => WireSessionState::Closed,
    };
    Some(SessionMeta {
        state,
        version: DecimalString::parse(&summary.version().get().to_string()).ok()?,
    })
}

/// core 的 origin cursor → wire。
fn wire_cursor(view: &acp_core::use_cases::NodeLinkSessionView) -> Option<OriginCursor> {
    Some(OriginCursor {
        origin_epoch: Uuid::parse(view.head.origin_epoch.as_str()).ok()?,
        origin_sequence: DecimalString::parse(&view.head.origin_sequence.get().to_string()).ok()?,
    })
}

/// wire 的 origin cursor → core。
fn core_cursor(cursor: &OriginCursor) -> Option<CoreOriginCursor> {
    Some(CoreOriginCursor {
        origin_epoch: acp_core::model::OriginEpoch::new(cursor.origin_epoch.as_str()).ok()?,
        origin_sequence: acp_core::model::Sequence::new(
            cursor.origin_sequence.as_str().parse::<u64>().ok()?,
        )
        .ok()?,
    })
}

fn export_id(text: &str) -> Option<ExportId> {
    ExportId::new(text).ok()
}

fn session_id(text: &str) -> Option<SessionId> {
    SessionId::new(text).ok()
}

/// id 分配收敛到 core 的 `IdGenerator`（§3.1）：Node Link 的非命令标识（attachmentId/snapshotId）
/// 走它，本层不自造随机源。
fn message_id(core: &UseCases) -> Option<Uuid> {
    Uuid::parse(core.ids().message_id().as_str()).ok()
}

/// 无填充 base64url 编码（wire 的二进制字段形式，`SYNC_PROTOCOL.md` §3.2）。
fn base64url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 解码消息 body（`Envelope` 的 body 是原始字节；失败按 schema 错误处理）。
fn decode<T: DeserializeOwned>(message: &Envelope) -> Option<T> {
    serde_json::from_str(message.body().get()).ok()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests;
