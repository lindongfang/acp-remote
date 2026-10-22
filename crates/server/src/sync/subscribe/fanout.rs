//! Sync 的事件**实时**扇出（重放屏障之后接续的那一段）与它的分发循环。
//!
//! ## 为什么单独一个模块
//!
//! 实时投递与重放共用同一副"持久化行 → `event` 帧"的装配（[`crate::sync::event::frame`]），但两者的**触发点**
//! 不同：重放由客户端消息驱动（`sync.subscribe`/`sync.snapshot_request` 在路由线程里同步推进），实时
//! 投递由 broker 的提交后发布驱动（`EventPublisher::publish` 在提交路径上被**同步**调用）。把后者塞进
//! 重放路径会让"一条连接一份订阅状态"与"broker 的提交路径"耦合，且 `publish` 不得阻塞 broker——
//! 因此这里是一道有界 channel 加一个分发循环（与 `server::node_link::resource` 的 `NodeLinkPublisher`
//! 同形同口径，类型各自定义，`AGENTS.md` §6）。
//!
//! ## 三条硬规则
//!
//! - **实时事件 MUST 排在屏障之后**：只投"已建立屏障"的连接（`sync.caught_up` 已发，见
//!   [`SubscribeRoute::live_targets`]）。重放中的连接由路由线程自己推进，本循环**不**碰它——否则同一
//!   连接会收到两条投递链路交错的帧，且实时事件可能越过序号更早的已持久化事件（4.3/4.8 的判据）。
//! - **慢客户端不得阻塞 broker 或 Agent**：`publish` 只做 `try_send`，队列满即丢这一条并记日志
//!   （事件已持久化，客户端重连后凭 cursor 重放补齐）；分发侧每连接仍看出站账本的高水位停读信号，
//!   达到即跳过该连接。**停读信号真的传到投递层**：这里是它唯一的消费点。
//! - **不调用 `server::node_link`**：本模块只经 `core` 的读面取事件正文、按 `(hostId, deviceId)` 从
//!   Sync 自己的连接注册表找目标（`AGENTS.md` §5）。

use std::sync::Arc;

use acp_core::model::CommittedEvent;
use acp_core::ports::EventPublisher;
use sync_protocol::envelope::MessageType;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::sync::conn::registry::ConnectionRegistry;
use crate::sync::subscribe::{EventReader, SubscribeRoute};
use crate::transport::net::Shutdown;

/// 扇出链路自己的有界队列容量（每连接队列之外的第二道；满了只丢这一条并记日志，broker 绝不被阻塞）。
const EVENT_QUEUE_CAPACITY: usize = 1024;

/// 实时扇出的发布端（broker 提交成功后由组合根的 `EventPublisher` 分叉调用）。
///
/// 通道的接收端由本结构持有（`take_receiver`）：发布端必须先装进组合根的 `EventPublisher` 分叉，而
/// 那时分发循环还没有启动——两侧的生命周期因此必然错开，接收端在这里等 `NetIngress::start` 来取。
pub struct SyncPublisher {
    queue: mpsc::Sender<CommittedEvent>,
    receiver: Option<mpsc::Receiver<CommittedEvent>>,
}

impl SyncPublisher {
    /// 建立「发布端 ↔ 分发循环」的通道。
    pub fn channel() -> Self {
        let (queue, receiver) = mpsc::channel(EVENT_QUEUE_CAPACITY);
        Self {
            queue,
            receiver: Some(receiver),
        }
    }

    /// 取回通道的接收端（只应取一次；第二次起得 `None`，调用方据此失败关闭而不是静默丢事件）。
    pub fn take_receiver(&mut self) -> Option<mpsc::Receiver<CommittedEvent>> {
        self.receiver.take()
    }
}

impl EventPublisher for SyncPublisher {
    fn publish(&self, event: CommittedEvent) {
        let event_id = event.id.as_str().to_owned();
        let global_sequence = event.global_sequence.get();
        // `try_send` 失败即丢：事件已持久化，客户端重连后凭 cursor 重放补齐（§9.5）。
        if let Err(error) = self.queue.try_send(event) {
            match error {
                mpsc::error::TrySendError::Full(_) => warn!(
                    event = "sync.fanout_queue_full",
                    event_id = event_id.as_str(),
                    global_sequence,
                    "the sync fan-out queue is full; the device replays owned events by cursor"
                ),
                mpsc::error::TrySendError::Closed(_) => debug!(
                    event = "sync.fanout_closed",
                    event_id = event_id.as_str(),
                    "the sync fan-out is shutting down"
                ),
            }
        }
    }
}

/// 实时投递的分发循环（组合根 spawn，关闭序列参与）。
///
/// `reader` 是事件行 → `event` 帧素材的读面（与重放共用同一个 [`EventReader`] 实现）；`shutdown`
/// 触发即结束循环，队列里剩余的事件不再投递——它们已经在事件库里持久化，客户端重连后凭 cursor 重放。
pub async fn dispatch<E: EventReader + 'static>(
    registry: Arc<ConnectionRegistry>,
    subscribe: Arc<SubscribeRoute>,
    reader: Arc<E>,
    mut queue: mpsc::Receiver<CommittedEvent>,
    shutdown: Shutdown,
) {
    loop {
        let event = tokio::select! {
            biased;
            () = shutdown.clone().wait() => {
                debug!(event = "sync.fanout_shutdown", "the sync event fan-out stops");
                return;
            }
            received = queue.recv() => match received {
                Some(event) => event,
                None => return,
            },
        };
        deliver_owned(&registry, &subscribe, reader.as_ref(), &event).await;
    }
}

async fn deliver_owned<E: EventReader>(
    registry: &ConnectionRegistry,
    subscribe: &SubscribeRoute,
    reader: &E,
    event: &CommittedEvent,
) {
    let targets = subscribe.live_targets(registry);
    if targets.is_empty() {
        return;
    }
    for (target, actor) in &targets {
        let negotiated = subscribe.origin_feature_of(target);
        // 素材按连接读：`sync_replay` 自带 `session.list` 授权判定，无权设备在读面就失败
        // （与重放路径同一个 actor、同一道判定，因此两条路径的可见性不可能不同）。
        let record = match reader.read(event, actor).await {
            Ok(Some(record)) => record,
            Ok(None) => {
                debug!(
                    event = "sync.fanout_record_missing",
                    connection_id = target.connection_id().as_str(),
                    event_id = event.id.as_str(),
                    "the event row cannot be read back; the device replays it by cursor"
                );
                continue;
            }
            Err(error) => {
                // 读面失败（含历史行无法归因的 `Corrupt` 或授权拒绝）：**这一条不投递**。客户端重连后
                // 按 cursor 重放时会在重放路径上拿到同一条失败关闭（那里回 `internal.unavailable` +
                // 可见诊断），因此这里静默跳过不会把失败藏起来。
                warn!(
                    event = "sync.fanout_read_failed",
                    connection_id = target.connection_id().as_str(),
                    event_id = event.id.as_str(),
                    error = ?error,
                    "the event row cannot be read for fan-out; the device replays it by cursor"
                );
                continue;
            }
        };
        if record.origin_kind == acp_core::model::EventOrigin::Node && !negotiated {
            // §10.1：未协商 `core.event-origin.v1` 的连接 MUST NOT 收到 `kind = node` 的事件，
            // 且服务端 MUST NOT 因客户端版本伪造或降级 `kind`。
            continue;
        }
        match subscribe.frame_event(target, actor, &record).await {
            Ok(body) => {
                if target.send(MessageType::Event, &body).is_err() {
                    // 高水位或连接已结束：事件已持久化，客户端重连后凭 cursor 重放补齐（§14）。
                    debug!(
                        event = "sync.fanout_skipped",
                        connection_id = target.connection_id().as_str(),
                        event_id = event.id.as_str(),
                        "the connection cannot take the event right now"
                    );
                }
            }
            Err(fault) => {
                // 组装不出合法帧：显式跳过这一条并记**可见**诊断（绝不伪造字段、绝不静默丢弃）。
                warn!(
                    event = "sync.fanout_event_unassemblable",
                    connection_id = target.connection_id().as_str(),
                    event_id = event.id.as_str(),
                    fault = %fault,
                    "the persisted event cannot be mapped to a v1 event frame"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! 4.4 的扇出回归：`publish` 在提交路径上被同步调用，队列满时只能丢这一条，绝不得阻塞 broker。

    use super::*;

    fn event(number: u64) -> CommittedEvent {
        let uuid = || uuid::Uuid::new_v4().to_string();
        CommittedEvent {
            id: acp_core::model::EventId::new(&uuid()).expect("event id"),
            event_type: acp_core::model::EventType::new("agent.message.delta").expect("type"),
            session: None,
            session_sequence: None,
            global_sequence: acp_core::model::Sequence::new(number + 1).expect("sequence"),
            origin_epoch: None,
            origin_sequence: None,
            created_at: acp_core::model::Timestamp::new("2026-01-01T00:00:00.000Z")
                .expect("timestamp"),
        }
    }

    #[test]
    fn a_full_queue_drops_instead_of_blocking_the_publisher() {
        let mut publisher = SyncPublisher::channel();
        let mut receiver = publisher.take_receiver().expect("receiver");
        for number in 0..EVENT_QUEUE_CAPACITY as u64 {
            publisher.publish(event(number));
        }
        // 队列已满：这次发布仍立即返回（丢弃），broker 的提交路径不会被慢连接拖住。
        publisher.publish(event(10_000));
        let mut delivered = 0;
        while receiver.try_recv().is_ok() {
            delivered += 1;
        }
        assert_eq!(delivered, EVENT_QUEUE_CAPACITY);
    }
}
