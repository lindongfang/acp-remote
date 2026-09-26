//! 连接注册表与每连接的会话句柄（`design.md` D7/D9）：认证成功的 WSS 连接在此登记，供
//! `server::node_link` 的其他子模块（WP5 的事件扇出、WP6 的撤销传播）投递消息与关闭连接。
//!
//! 三条边界：
//!
//! - **注册表不是事实来源**：它只承载「当前活着的连接」这一连接级事实；授权判定仍按当次持久化记录
//!   （`design.md` D7：「撤销判定以持久化记录为准，进程内缓存不得让已撤销的 Export/节点继续可用」）；
//! - **每连接待发送队列有界**：条数上限由 `tokio::sync::mpsc` 的容量给出，字节上限由 [`ConnectionHandle`]
//!   自行计数（`maxPendingQueueBytes`/`maxPendingQueueMessages` 双上限，§2.5）。达到上限即
//!   [`SendFault::HighWater`]，调用方（WP5）据此停读新的快照批次；本模块不丢消息、不阻塞其他连接；
//! - **关闭是请求而非暴力断开**：`request_close` 只登记目标 close code 并唤醒会话；会话先排空已入队的
//!   消息（撤销推送必须能先送达）再发 close 帧（§14.2）。

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use acp_core::model::NodeId;
use acp_core::ports::IdGenerator;
use node_link_protocol::common::{DecimalString, Uuid};
use node_link_protocol::envelope::{ConnectionFields, Envelope, MessageType, encode_body};
use serde::Serialize;
use tokio::sync::Notify;
use tokio::sync::mpsc;

use crate::node_link::conn::limits::SessionLimits;

/// 队列里的一条已编码消息。
pub(crate) struct Outbound {
    pub(crate) text: String,
    pub(crate) bytes: u64,
}

/// 投递失败的分类：都是**调用方**（WP5/WP6）要处理的判定，不是本模块的错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendFault {
    /// 队列已达高水位（条数或字节）：调用方应停止生产新消息（例如停读快照批次）而不是重试。
    HighWater,
    /// 连接已结束（会话已退出或 close 已发出）。
    Closed,
    /// 消息无法装配成信封（本机缺陷：id/序号/body 编码）。调用方记结构化日志。
    Encoding,
}

/// 已入队未写出的量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingQueue {
    pub messages: u64,
    pub bytes: u64,
}

/// 一条已认证的 WSS 连接的句柄。
///
/// 方法都是同步的（`try_send` 语义）：投递者不得因为某个慢连接而阻塞自己的任务；队列满由
/// [`SendFault::HighWater`] 表达。
pub struct ConnectionHandle {
    connection_id: Uuid,
    node_id: NodeId,
    client_ip: IpAddr,
    limits: SessionLimits,
    ids: Arc<dyn IdGenerator>,
    outbound: mpsc::Sender<Outbound>,
    pending_bytes: AtomicU64,
    pending_messages: AtomicU64,
    outbound_sequence: Mutex<u64>,
    close_code: AtomicU16,
    close_reason: Mutex<Option<&'static str>>,
    /// 关闭请求的唤醒信号（会话正阻塞在 `recv()` 时也要能立刻收到）。
    pub(crate) close_signal: Notify,
    /// 首次观察到高水位的时刻（慢消费者看门狗用）。
    saturated_since: Mutex<Option<Instant>>,
}

impl std::fmt::Debug for ConnectionHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectionHandle")
            .field("connection_id", &self.connection_id.as_str())
            .field("node_id", &self.node_id.as_str())
            .finish_non_exhaustive()
    }
}

impl ConnectionHandle {
    /// 会话侧构造（注册表登记在认证成功之后，`conn::session`）。
    pub(crate) fn new(
        connection_id: Uuid,
        node_id: NodeId,
        client_ip: IpAddr,
        limits: SessionLimits,
        ids: Arc<dyn IdGenerator>,
    ) -> (Arc<Self>, mpsc::Receiver<Outbound>) {
        let (outbound, receiver) = mpsc::channel(limits.max_pending_queue_messages());
        let handle = Arc::new(Self {
            connection_id,
            node_id,
            client_ip,
            limits,
            ids,
            outbound,
            pending_bytes: AtomicU64::new(0),
            pending_messages: AtomicU64::new(0),
            outbound_sequence: Mutex::new(0),
            close_code: AtomicU16::new(0),
            close_reason: Mutex::new(None),
            close_signal: Notify::new(),
            saturated_since: Mutex::new(None),
        });
        (handle, receiver)
    }

    /// 本次连接的 `connectionId`（`node.challenge` 下发的那个）。
    pub fn connection_id(&self) -> &Uuid {
        &self.connection_id
    }

    /// 对端节点标识。
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// 对端地址（限流与日志用；不可信来源的转发头已在接入层被忽略）。
    pub fn client_ip(&self) -> IpAddr {
        self.client_ip
    }

    /// 本连接的生效限额（`node.ready.limits` 的同值）。
    pub fn limits(&self) -> SessionLimits {
        self.limits
    }

    /// 发送一条认证后消息：自动分配 `messageId` 与出站 `connectionSequence`，按 `type` 选信封形状。
    ///
    /// 只接受认证后 type（`AuthState::PostAuth`）；`node.hello`/`node.challenge`/`node.proof` 不能经
    /// 本入口发送（认证前消息由握手阶段直接写帧）。
    pub fn send<T: Serialize>(&self, message_type: MessageType, body: &T) -> Result<(), SendFault> {
        let text = self.encode(message_type, body)?;
        self.enqueue(text)
    }

    /// 装配一条认证后消息的原始 JSON 文本（不投队列；会话测试与 `link.error` 共用同一路径）。
    pub(crate) fn encode<T: Serialize>(
        &self,
        message_type: MessageType,
        body: &T,
    ) -> Result<String, SendFault> {
        let sequence = self.next_sequence();
        let message_id =
            Uuid::parse(self.ids.message_id().as_str()).map_err(|_| SendFault::Encoding)?;
        let sequence =
            DecimalString::parse(&sequence.to_string()).map_err(|_| SendFault::Encoding)?;
        let body = encode_body(body).map_err(|_| SendFault::Encoding)?;
        let envelope = Envelope::new(
            message_type,
            message_id,
            Some(ConnectionFields {
                connection_id: self.connection_id.clone(),
                connection_sequence: sequence,
            }),
            body,
        )
        .map_err(|_| SendFault::Encoding)?;
        envelope.encode().map_err(|_| SendFault::Encoding)
    }

    /// 把一条已装配的原始 JSON 文本投进队列。
    pub(crate) fn enqueue(&self, text: String) -> Result<(), SendFault> {
        let bytes = text.len() as u64;
        if self.pending_bytes.load(Ordering::SeqCst) + bytes > self.limits.max_pending_queue_bytes()
        {
            self.mark_saturated();
            return Err(SendFault::HighWater);
        }
        match self.outbound.try_send(Outbound { text, bytes }) {
            Ok(()) => {
                self.pending_bytes.fetch_add(bytes, Ordering::SeqCst);
                self.pending_messages.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.mark_saturated();
                Err(SendFault::HighWater)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(SendFault::Closed),
        }
    }

    /// 已入队未写出的量。
    pub fn pending(&self) -> PendingQueue {
        PendingQueue {
            messages: self.pending_messages.load(Ordering::SeqCst),
            bytes: self.pending_bytes.load(Ordering::SeqCst),
        }
    }

    /// 是否处于高水位（字节或条数达上限，或刚刚被拒过一条消息）。WP5 在生成快照批次前检查它。
    ///
    /// 「刚被拒过」也计入：队里可能已经没有消息（那条消息根本没入队），但高水位状态仍然成立，
    /// 调用方应先停下来而不是继续尝试。一旦 `consumed` 观察到队列回到上限之下，该状态自动清除。
    pub fn saturated(&self) -> bool {
        if lock(&self.saturated_since).is_some() {
            return true;
        }
        let pending = self.pending();
        pending.bytes >= self.limits.max_pending_queue_bytes()
            || pending.messages as usize >= self.limits.max_pending_queue_messages()
    }

    /// 进入过饱和状态的持续时长（没有进入过时 `None`）；会话的慢消费者看门狗据此断开。
    pub(crate) fn saturated_for(&self) -> Option<std::time::Duration> {
        (*lock(&self.saturated_since)).map(|since| since.elapsed())
    }

    /// 请求以给定 close code 结束连接（`node.trust.revoked` + 4410、撤销传播等）。
    ///
    /// 幂等：首次请求生效，后续请求不改写目标 code；会话先排空已入队消息再发 close 帧。
    pub fn request_close(&self, code: u16, reason: &'static str) {
        let _ = self
            .close_code
            .compare_exchange(0, code, Ordering::SeqCst, Ordering::SeqCst);
        {
            let mut slot = lock(&self.close_reason);
            if slot.is_none() {
                *slot = Some(reason);
            }
        }
        self.close_signal.notify_waiters();
    }

    /// 已登记的关闭目标（`None` = 未请求关闭）；会话每轮循环检查它，防止丢失唤醒。
    pub(crate) fn close_request(&self) -> Option<(u16, &'static str)> {
        let code = self.close_code.load(Ordering::SeqCst);
        if code == 0 {
            return None;
        }
        Some((
            code,
            (*lock(&self.close_reason)).unwrap_or("connection closed"),
        ))
    }

    /// 队列消费记账（会话写出后调用）。
    pub(crate) fn consumed(&self, outbound: &Outbound) {
        self.pending_bytes
            .fetch_sub(outbound.bytes, Ordering::SeqCst);
        self.pending_messages.fetch_sub(1, Ordering::SeqCst);
        if !self.saturated() {
            *lock(&self.saturated_since) = None;
        }
    }

    fn next_sequence(&self) -> u64 {
        let mut sequence = lock(&self.outbound_sequence);
        *sequence += 1;
        *sequence
    }

    fn mark_saturated(&self) {
        let mut since = lock(&self.saturated_since);
        if since.is_none() {
            *since = Some(Instant::now());
        }
    }
}

/// 活跃连接的注册表（组合根装配一次，进程内唯一）。
///
/// WP6 的撤销传播用 [`ConnectionRegistry::close_node`] 关闭某节点的全部连接（`design.md` D7）；
/// WP5 用 [`ConnectionRegistry::handles`] 找到自己的订阅者（订阅状态归 WP5）。
#[derive(Default)]
pub struct ConnectionRegistry {
    connections: Mutex<BTreeMap<String, Arc<ConnectionHandle>>>,
}

impl std::fmt::Debug for ConnectionRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectionRegistry")
            .field("active", &self.active())
            .finish()
    }
}

impl ConnectionRegistry {
    /// 空注册表的共享句柄。
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 当前活跃连接数。
    pub fn active(&self) -> usize {
        lock(&self.connections).len()
    }

    /// 当前全部活跃连接的句柄（顺序按 `connectionId` 稳定，便于用例断言）。
    pub fn handles(&self) -> Vec<Arc<ConnectionHandle>> {
        lock(&self.connections).values().cloned().collect()
    }

    /// 某节点的全部活跃连接。
    pub fn handles_for_node(&self, node: &NodeId) -> Vec<Arc<ConnectionHandle>> {
        lock(&self.connections)
            .values()
            .filter(|handle| handle.node_id() == node)
            .cloned()
            .collect()
    }

    /// 请求关闭某节点的全部连接（返回被请求的连接数）。
    ///
    /// 关闭失败与「没有连接」都是 `Ok`：调用方（撤销传播）在持久化提交**之后**调用，关闭失败只记日志，
    /// 不回滚已提交的撤销（§11.6 第 5 条）。
    pub fn close_node(&self, node: &NodeId, code: u16, reason: &'static str) -> usize {
        let handles = self.handles_for_node(node);
        for handle in &handles {
            handle.request_close(code, reason);
        }
        handles.len()
    }

    pub(crate) fn register(&self, handle: Arc<ConnectionHandle>) {
        lock(&self.connections).insert(handle.connection_id().as_str().to_owned(), handle);
    }

    pub(crate) fn unregister(&self, connection_id: &str) {
        lock(&self.connections).remove(connection_id);
    }
}

/// 锁中毒时取回内层数据而不是 panic：注册表在任何 panic 之后仍要能继续关闭连接。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
