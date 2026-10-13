//! 连接注册表与每连接的会话句柄（`design.md` D4/D5）：认证成功的 WSS 连接在此登记，供 `server::sync`
//! 的其他子模块（WP4 的事件扇出、WP6 的撤销钩子）投递消息、按 `(hostId, deviceId)` 替换与关闭连接。
//!
//! 四条边界：
//!
//! - **注册表不是事实来源**：它只承载「当前活着的连接」这一连接级事实；授权与撤销判定仍按当次持久化
//!   记录（`design.md` D5：「撤销判定以持久化记录为准，进程内缓存不得让已撤销设备继续可用」）。因此撤销
//!   路径必须是「先提交持久状态、再扫连接表」，即使关闭送达失败，后续认证与命令仍被拒；
//! - **每连接待发送队列有界**：条数上限由 `tokio::sync::mpsc` 的容量给出，字节上限由 [`ConnectionHandle`]
//!   自行计数（§14 的双上限：8 MiB 或 2,000 条，先到者触发）。达到上限即 [`SendFault::HighWater`]，
//!   投递方（WP4）据此**停读新的重放批次**；本模块不丢消息、不阻塞其他连接。停/恢复用两道阈值
//!   （高水位进入、低水位退出），而慢消费者看门狗的计时按「当前饱和段」累计——两个口径都在
//!   [`ConnectionHandle::saturated_for`] 的文档里说明；
//! - **序号只在投递确定时消耗**：出站 `connectionSequence` 的分配、容量准入与入队在同一临界区
//!   （[`OutboundState`]）：被高水位拒绝的投递不消耗序号（§4.1 要求本方向严格 +1，缺号只能被对端判为
//!   `protocol.sequence_invalid`，而协议没有重同步规则）；
//! - **关闭是请求而非暴力断开**：`request_close` 只登记目标 close code 并唤醒会话；会话先排空已入队的
//!   消息（撤销推送必须能先送达）再发 close 帧（§14.2 口径）。

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use acp_core::model::{DeviceId, NodeId};
use acp_core::ports::IdGenerator;
use serde::Serialize;
use sync_protocol::common::{DecimalString, Uuid};
use sync_protocol::envelope::{ConnectionFields, Envelope, MessageType, encode_body};
use tokio::sync::Notify;
use tokio::sync::mpsc;

use crate::sync::conn::limits::{
    MAX_PENDING_QUEUE_BYTES, MAX_PENDING_QUEUE_MESSAGES, SessionLimits,
};

/// 队列里的一条已编码消息。
pub(crate) struct Outbound {
    pub(crate) text: String,
    pub(crate) bytes: u64,
}

/// 出站账本：待发送队列的占用与出站序号共用**一个**临界区。
///
/// 两者必须同锁：`connectionSequence` 一旦提交就必须真的发出去（§4.1 要求本方向严格 +1，对端按
/// `protocol.sequence_invalid` 校验），因此准入失败时不能消耗序号。
#[derive(Debug, Default)]
struct OutboundState {
    /// 已提交的出站序号；下一条消息用它 +1 作为候选值（认证后从 1 开始）。
    sequence: u64,
    pending_messages: u64,
    pending_bytes: u64,
}

impl OutboundState {
    /// 容量预算：条数与字节双上限（§14）。字节预算要等编码后才准，因此它是准入的第二道。
    fn admits(&self, bytes: u64) -> bool {
        self.pending_messages < MAX_PENDING_QUEUE_MESSAGES
            && self.pending_bytes + bytes <= MAX_PENDING_QUEUE_BYTES
    }
}

/// 投递失败的分类：都是**调用方**（WP4/WP6）要处理的判定，不是本模块的错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendFault {
    /// 队列已达高水位（条数或字节）：调用方应停止生产新消息（例如停读快照批次）而不是重试。
    HighWater,
    /// 连接已结束（会话已退出或 close 已发出）。
    ///
    /// **与 [`SendFault::HighWater`] 的先后**：容量判定先于入队，因此连接结束后若占用恰好停在上限上，
    /// 投递方此后看到的是 `HighWater` 而不是 `Closed`。判断「连接还在不在」应以注册表与
    /// [`ConnectionHandle::close_request`] 为准，不要只看本枚举（本批改动未调整这个先后，见交付报告）。
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
    host_id: NodeId,
    device_id: DeviceId,
    client_ip: IpAddr,
    limits: SessionLimits,
    ids: Arc<dyn IdGenerator>,
    outbound: mpsc::Sender<Outbound>,
    /// 出站账本（待发送占用 + 出站序号；见 [`OutboundState`]）。
    outbound_state: Mutex<OutboundState>,
    close_code: AtomicU16,
    close_reason: Mutex<Option<&'static str>>,
    /// 关闭请求的唤醒信号（会话正阻塞在 `recv()` 时也要能立刻收到）。
    pub(crate) close_signal: Notify,
    /// 当前饱和段的起点（慢消费者看门狗用）：首次投递被容量判定拒绝的时刻，占用回到低水位线之下即清除
    /// （见 [`ConnectionHandle::saturated_for`] 与 [`ConnectionHandle::consumed`]）。
    saturated_since: Mutex<Option<Instant>>,
}

impl std::fmt::Debug for ConnectionHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectionHandle")
            .field("connection_id", &self.connection_id.as_str())
            .field("device_id", &self.device_id.as_str())
            .finish_non_exhaustive()
    }
}

impl ConnectionHandle {
    /// 会话侧构造（注册表登记在认证成功之后，`conn::session`）。
    pub(crate) fn new(
        connection_id: Uuid,
        host_id: NodeId,
        device_id: DeviceId,
        client_ip: IpAddr,
        limits: SessionLimits,
        ids: Arc<dyn IdGenerator>,
    ) -> (Arc<Self>, mpsc::Receiver<Outbound>) {
        let (outbound, receiver) = mpsc::channel(MAX_PENDING_QUEUE_MESSAGES as usize);
        let handle = Arc::new(Self {
            connection_id,
            host_id,
            device_id,
            client_ip,
            limits,
            ids,
            outbound,
            outbound_state: Mutex::new(OutboundState::default()),
            close_code: AtomicU16::new(0),
            close_reason: Mutex::new(None),
            close_signal: Notify::new(),
            saturated_since: Mutex::new(None),
        });
        (handle, receiver)
    }

    /// 本次连接的 `connectionId`（`auth.server_challenge` 下发的那个）。
    pub fn connection_id(&self) -> &Uuid {
        &self.connection_id
    }

    /// 本次连接服务的 Host 身份（`auth.client_hello.hostId`；§8.5 的替换作用域是 `(hostId, deviceId)`）。
    pub fn host_id(&self) -> &NodeId {
        &self.host_id
    }

    /// 对端设备标识。
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    /// 对端地址（限流与日志用；不可信来源的转发头已在接入层被忽略）。
    pub fn client_ip(&self) -> IpAddr {
        self.client_ip
    }

    /// 本连接的生效限额（`auth.authenticated.limits` 的同值）。
    pub fn limits(&self) -> SessionLimits {
        self.limits
    }

    /// 注册表的键（`(hostId, deviceId)`，§8.5）。
    pub(crate) fn key(&self) -> (String, String) {
        (
            self.host_id.as_str().to_owned(),
            self.device_id.as_str().to_owned(),
        )
    }

    /// 发送一条认证后消息：自动分配 `messageId` 与出站 `connectionSequence`，按 `type` 选信封形状。
    ///
    /// 只接受认证后 type（`AuthState::PostAuth`）；`auth.*` 不能经本入口发送（认证前的消息由握手阶段
    /// 直接写帧）。
    ///
    /// 准入判定、序号分配与入队在**同一个临界区**内完成：序号先是候选值，只有容量预算通过（且真的入队）
    /// 才提交。因此被高水位拒绝的投递不消耗序号——否则本方向此后每一帧都带缺口，合规对端必须一直报
    /// `protocol.sequence_invalid`，而协议没有重同步规则。
    pub fn send<T: Serialize>(&self, message_type: MessageType, body: &T) -> Result<(), SendFault> {
        self.send_with_frame(message_type, body).map(|_| ())
    }

    /// 发送一条认证后消息，并返回本次入队的**帧文本**。
    ///
    /// 与 [`ConnectionHandle::send`] 逐条同路径（同一临界区、同一序号与容量判定），只是把实际入队的那份
    /// 文本交回调用方：快照 digest 必须对「将要发出的完整 chunk 帧字节」求摘要（§9.4：接收方按收到的
    /// 原始 UTF-8 字节复算，不得重新序列化 JSON），而重新编码一次会得到不同的 messageId/序号。
    pub fn send_with_frame<T: Serialize>(
        &self,
        message_type: MessageType,
        body: &T,
    ) -> Result<String, SendFault> {
        let mut state = lock(&self.outbound_state);
        // ① 用候选序号装配信封：装配失败不触碰账本（也不消耗序号）。
        let candidate = state.sequence + 1;
        let text = self.encode(message_type, body, candidate)?;
        let bytes = text.len() as u64;
        // ② 容量预算（条数 + 字节双上限）。
        if !state.admits(bytes) {
            drop(state);
            self.mark_saturated();
            return Err(SendFault::HighWater);
        }
        // ③ 入队成功才提交序号与占用；`try_send` 不阻塞，持锁是安全的。
        match self.outbound.try_send(Outbound {
            text: text.clone(),
            bytes,
        }) {
            Ok(()) => {
                state.sequence = candidate;
                state.pending_messages += 1;
                state.pending_bytes += bytes;
                Ok(text)
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                // 预算是上界，而 `pending_messages` 覆盖了管道里的全部消息，因此这条分支实际不可达；
                // `Full` 对调用方与 `HighWater` 是同一个处置，这里同样不提交序号。
                drop(state);
                self.mark_saturated();
                Err(SendFault::HighWater)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(SendFault::Closed),
        }
    }

    /// 用给定序号装配一条认证后消息的原始 JSON 文本（不投队列、不触碰账本）。
    fn encode<T: Serialize>(
        &self,
        message_type: MessageType,
        body: &T,
        sequence: u64,
    ) -> Result<String, SendFault> {
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

    /// 已入队未写出的量。
    pub fn pending(&self) -> PendingQueue {
        let state = lock(&self.outbound_state);
        PendingQueue {
            messages: state.pending_messages,
            bytes: state.pending_bytes,
        }
    }

    /// 是否处于高水位（字节或条数达上限，或刚刚被拒过一条消息）。WP4 在生成快照批次前检查它。
    ///
    /// 「刚被拒过」也计入：队里可能已经没有消息（那条消息根本没入队），但高水位状态仍然成立，调用方
    /// 应先停下来而不是继续尝试。清除要等 [`ConnectionHandle::consumed`] 观察到占用回到**低水位线**
    /// （高水位的一半）之下——迟滞（hysteresis）让投递方不必在阈值上下反复启停。
    pub fn saturated(&self) -> bool {
        let rejected_recently = lock(&self.saturated_since).is_some();
        if rejected_recently {
            return true;
        }
        let pending = self.pending();
        pending.bytes >= MAX_PENDING_QUEUE_BYTES || pending.messages >= MAX_PENDING_QUEUE_MESSAGES
    }

    /// 当前饱和段已持续的时长（从未进入饱和时 `None`）；会话的慢消费者看门狗据此断开（§14）。
    ///
    /// **口径（为什么这样与「持续过慢」一致、又不会判死瞬时突发）**：这里的计时属于**当前饱和段**——
    /// 起点是首次被 [`ConnectionHandle::send`] 的容量判定拒绝的时刻（[`Self::mark_saturated`]；即占用
    /// 真的顶到上限的那一刻），终点只在占用回到低水位线之下时到来。于是：
    ///
    /// - 反复瞬时超过再回落的消费者（在高低水位之间抖动）**不会**把计时清零：它从未真的脱离饱和，
    ///   计时连续累计到宽限即断开——这正是 §14 的「**持续**过慢则发送 `resource.backpressure` 并断开」；
    /// - 瞬时突发会真的把队列排到低水位之下，计时随之清除，因此一次突发不会被判死。
    ///
    /// 只按「占用是否仍 ≥ 高水位」清除会让抖动永远攒不满宽限（永不断开）；只累计不清除又会把长期健康、
    /// 偶尔突发的连接算成慢消费者。两道阈值（高水位进入、低水位退出）是这两端的折中。
    ///
    /// 起点只在**投递被拒**时建立（[`ConnectionHandle::consumed`] 只清除、不建立）：占用恰好停在上限上、
    /// 生产者就此停手的连接，起点会晚到下一次心跳投递撞上同一道容量判定为止（最长一个心跳周期），
    /// 看门狗因此晚一拍断开，不会漏判。
    pub(crate) fn saturated_for(&self) -> Option<std::time::Duration> {
        (*lock(&self.saturated_since)).map(|since| since.elapsed())
    }

    /// 请求以给定 close code 结束连接（撤销传播的 `4410`、单设备替换的 `4411` 等）。
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
    ///
    /// 清除判据不只是「占用回到上限之下」，而是「占用回到**低水位线**之下」（[`RECOVERY_MESSAGES`] /
    /// [`RECOVERY_BYTES`]，各取高水位的一半）。两个阈值分开口径的理由见 [`ConnectionHandle::saturated_for`]：
    /// 只在高水位附近反复起伏的消费者不会掉到低水位之下，饱和计时因此连续累计；只有真的把队列排出可观
    /// 比例才算脱离饱和。
    ///
    /// 计时在这里**只清除、不建立**：起点唯一来自 `admits()` 拒绝一次投递（[`Self::mark_saturated`]）。
    /// 因此「占用恰好停在上限上、生产者就此停手」时起点会晚一步——队列停在限上不产生拒绝，看门狗要等
    /// 下一次 tick 的心跳投递撞上同一道容量判定才拿到起点（[`ConnectionHandle::saturated_for`] 的调用方
    /// 在计时为空时走心跳分支）。这段空窗最长一个心跳周期，不会漏判持续过慢，也不会把突发判死。
    pub(crate) fn consumed(&self, outbound: &Outbound) {
        let recovered = {
            let mut state = lock(&self.outbound_state);
            state.pending_messages = state.pending_messages.saturating_sub(1);
            state.pending_bytes = state.pending_bytes.saturating_sub(outbound.bytes);
            state.pending_messages < RECOVERY_MESSAGES && state.pending_bytes < RECOVERY_BYTES
        };
        if recovered {
            *lock(&self.saturated_since) = None;
        }
    }

    fn mark_saturated(&self) {
        let mut since = lock(&self.saturated_since);
        if since.is_none() {
            *since = Some(Instant::now());
        }
    }
}

/// 「脱离饱和」的低水位线（条数）：占用低于它才算真的从高水位退出（§14 的 2,000 条的一半）。
pub(crate) const RECOVERY_MESSAGES: u64 = MAX_PENDING_QUEUE_MESSAGES / 2;
/// 「脱离饱和」的低水位线（字节）：与 [`RECOVERY_MESSAGES`] 同口径（§14 的 8 MiB 的一半）。
const RECOVERY_BYTES: u64 = MAX_PENDING_QUEUE_BYTES / 2;

/// 活跃连接的注册表（组合根装配一次，进程内唯一）。
///
/// 键是 `(hostId, deviceId)`（§8.5）：同一设备同时最多一条已认证连接。首次登记返回被顶替的旧句柄，
/// 「登记 + 判定 + 请求关闭旧连接」因此落在同一个临界区里（`design.md` D4 的竞态取舍）。
pub struct ConnectionRegistry {
    connections: Mutex<BTreeMap<(String, String), Arc<ConnectionHandle>>>,
}

impl std::fmt::Debug for ConnectionRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectionRegistry")
            .field("active", &self.active())
            .finish()
    }
}

impl Default for ConnectionRegistry {
    fn default() -> Self {
        Self {
            connections: Mutex::new(BTreeMap::new()),
        }
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

    /// 当前全部活跃连接的句柄（顺序按 `(hostId, deviceId)` 稳定，便于用例断言）。
    pub fn handles(&self) -> Vec<Arc<ConnectionHandle>> {
        lock(&self.connections).values().cloned().collect()
    }

    /// 某设备的全部活跃连接（撤销传播的扫描入口；`design.md` D5）。
    pub fn handles_for_device(&self, device: &DeviceId) -> Vec<Arc<ConnectionHandle>> {
        lock(&self.connections)
            .values()
            .filter(|handle| handle.device_id() == device)
            .cloned()
            .collect()
    }

    /// 请求关闭某设备的全部连接（返回被请求的连接数）。WP6 的撤销钩子接在这里。
    ///
    /// 关闭失败与「没有连接」都是 `0`/`usize` 的普通结果：调用方在持久化撤销提交**之后**调用，关闭
    /// 失败只记日志，不回滚已提交的撤销（`design.md` D5：注册表不是事实来源）。
    pub fn close_device(&self, device: &DeviceId, code: u16, reason: &'static str) -> usize {
        let handles = self.handles_for_device(device);
        for handle in &handles {
            handle.request_close(code, reason);
        }
        handles.len()
    }

    /// 登记一条已认证连接，返回被它顶替的旧句柄（没有则 `None`）。
    ///
    /// 这一步与「向旧句柄发 `4411`」必须连续（不跨锁拆分），否则两条并发完成认证的连接可能都判定自己
    /// 是最新的：先登记的一方拿到的旧句柄恰好是另一方，因此最后登记的一条成为 active，重复投递由客户端
    /// 按 `eventId` 去重兜底（§8.5 已声明该窗口存在）。
    pub(crate) fn register(&self, handle: Arc<ConnectionHandle>) -> Option<Arc<ConnectionHandle>> {
        lock(&self.connections).insert(handle.key(), handle)
    }

    /// 摘除一条连接。
    ///
    /// **必须带身份判定**：被替换的旧连接随后也会走到这里，若只按键删除，它会把后来者的登记一起抹掉
    /// （键是 `(hostId, deviceId)`，同一设备的先后两条连接共用同一个键）。
    pub(crate) fn unregister(&self, handle: &ConnectionHandle) {
        let mut connections = lock(&self.connections);
        let key = handle.key();
        if connections
            .get(&key)
            .is_some_and(|current| current.connection_id() == handle.connection_id())
        {
            connections.remove(&key);
        }
    }
}

/// 锁中毒时取回内层数据而不是 panic：注册表在任何 panic 之后仍要能继续关闭连接。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
