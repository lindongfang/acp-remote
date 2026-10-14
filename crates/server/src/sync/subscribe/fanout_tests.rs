//! `server::sync::subscribe::fanout` 的用例：实时事件在屏障之后投递，且与重放共用同一个字段装配。
//!
//! 驱动方式：真实的 [`SubscribeRoute`] + 真实的出站队列 + 一个替身 [`EventReader`]（不起 SQLite）。
//! 断言的是**对端真的收到的帧**，因此"实时事件越过屏障"这一类回归在帧序上直接可见。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use acp_core::model::{Actor, CommittedEvent, PortError};
use acp_core::ports::{EventPublisher, SyncEventRecord};
use tokio::sync::mpsc;

use super::EventReader;
use super::fanout::{SyncPublisher, dispatch};
use crate::sync::conn::MessageRoute;
use crate::sync::conn::RouteOutcome;
use crate::sync::conn::registry::{ConnectionRegistry, Outbound};
use crate::sync::subscribe::SubscribeRoute;
use crate::sync::subscribe::fixture::{self, DEVICE, EPOCH, Frame, Reachability, World};

/// 等一帧到达的上限（实时链路的用例不该依赖 sleep；取不到就是失败）。
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);

/// 确定性的事件读面替身：按事件 id 回放预置的素材；`failing` 注入读面失败。
struct FakeReader {
    records: Mutex<BTreeMap<String, SyncEventRecord>>,
    failing: bool,
}

impl FakeReader {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            records: Mutex::new(BTreeMap::new()),
            failing: false,
        })
    }

    /// 让每次读都失败关闭（历史行无法归因的注入点）。
    fn failing() -> Arc<Self> {
        Arc::new(Self {
            records: Mutex::new(BTreeMap::new()),
            failing: true,
        })
    }

    fn push(&self, record: SyncEventRecord) {
        self.records
            .lock()
            .expect("读面锁")
            .insert(record.event.id.as_str().to_owned(), record);
    }
}

#[async_trait::async_trait]
impl EventReader for FakeReader {
    async fn read(
        &self,
        event: &CommittedEvent,
        _actor: &Actor,
    ) -> Result<Option<SyncEventRecord>, PortError> {
        if self.failing {
            // 与真实存储同一条失败关闭口径（`origin_kind = device` 而得不出唯一设备 id）。
            return Err(PortError::Corrupt("owned_event.origin_kind"));
        }
        Ok(self
            .records
            .lock()
            .expect("读面锁")
            .get(event.id.as_str())
            .cloned())
    }
}

/// 一条要发布的 committed 事件（定位字段齐全；正文由读面给出）。
fn committed(id: &str, sequence: u64) -> CommittedEvent {
    CommittedEvent {
        id: acp_core::model::EventId::new(id).expect("event id"),
        event_type: acp_core::model::EventType::new("turn.started").expect("event type"),
        session: Some(acp_core::model::SessionId::new(fixture::SESSION).expect("session id")),
        session_sequence: Some(fixture::sequence(1)),
        global_sequence: fixture::sequence(sequence),
        origin_epoch: Some(
            acp_core::model::OriginEpoch::new(fixture::ORIGIN_EPOCH).expect("origin epoch"),
        ),
        origin_sequence: Some(fixture::sequence(sequence)),
        created_at: fixture::timestamp("2026-09-17T12:10:00.123Z"),
    }
}

/// 一条实时事件的固定素材（序号 8，`agent` 来源）。
const LIVE_ID: &str = "da702ba1-f49f-4e16-a677-a7744dfedb08";

/// 订阅一次使屏障建立（库里没有事件，因此直接收到 `sync.caught_up`）；交回路由与注册表。
async fn bar_connection(
    world: &World,
    features: &[&str],
) -> (
    Arc<SubscribeRoute>,
    Arc<ConnectionRegistry>,
    mpsc::Receiver<Outbound>,
) {
    let route = Arc::new(world.route(Arc::new(Reachability(true))));
    let (handle, receiver) = fixture::connection(DEVICE);
    let registry = ConnectionRegistry::new();
    registry.register(Arc::clone(&handle));
    let selected: Vec<String> = features.iter().map(|f| (*f).to_owned()).collect();
    route.on_authenticated(&handle, &selected);
    let envelope = fixture::envelope(
        "sync.subscribe",
        1,
        serde_json::json!({
            "cursor": { "serverEpoch": EPOCH, "globalSequence": "0" },
            "scope": "machine"
        }),
    );
    assert_eq!(route.route(&handle, &envelope).await, RouteOutcome::Claimed);
    let mut receiver = receiver;
    let frames = fixture::frames(&mut receiver);
    assert_eq!(
        fixture::bodies(&frames, "sync.caught_up").len(),
        1,
        "屏障必须先建立：实时事件只能排在它之后"
    );
    (route, registry, receiver)
}

/// 跑一条实时事件的分发，并把期间到达的出站帧取回。
async fn fan_out_one(
    route: &Arc<SubscribeRoute>,
    registry: &Arc<ConnectionRegistry>,
    reader: &Arc<FakeReader>,
    receiver: &mut mpsc::Receiver<Outbound>,
    event: CommittedEvent,
) -> Vec<Frame> {
    let mut publisher = SyncPublisher::channel();
    let queue = publisher.take_receiver().expect("新建发布端必然还有接收端");
    let (shutdown, signal) = crate::transport::net::Shutdown::channel();
    let task = tokio::spawn(dispatch(
        Arc::clone(registry),
        Arc::clone(route),
        Arc::clone(reader),
        queue,
        signal,
    ));
    publisher.publish(acp_core::model::CommittedDelivery::Owned(event));
    // 丢掉发布端即关闭队列：分发循环 `recv` 到 `None` 后退出。
    drop(publisher);
    tokio::time::timeout(FRAME_TIMEOUT, task)
        .await
        .expect("分发循环必须在时限内退出")
        .expect("分发任务不 panic");
    drop(shutdown);
    fixture::frames(receiver)
}

/// 屏障建立之后，一条实时事件经扇出链路投到该连接（4.8 的"之后继续实时投递"）。
#[tokio::test]
async fn a_live_event_is_delivered_after_the_barrier() {
    let world = World::new();
    world.seed_device(DEVICE, &["session.list"]);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    let (route, registry, mut receiver) = bar_connection(&world, &[]).await;

    let reader = FakeReader::new();
    reader.push(fixture::agent_event(8));
    let frames = fan_out_one(
        &route,
        &registry,
        &reader,
        &mut receiver,
        committed(LIVE_ID, 8),
    )
    .await;

    let events = fixture::bodies(&frames, "event");
    assert_eq!(events.len(), 1, "实时事件必须投到已建立屏障的连接");
    assert_eq!(events[0]["globalSequence"], "8");
    assert_eq!(events[0]["origin"]["kind"], "agent");
    assert!(
        events[0]["origin"].get("nodeId").is_none(),
        "未协商 core.event-origin.v1：不带 nodeId 键"
    );
    assert!(fixture::bodies(&frames, "error").is_empty());
}

/// 未建立屏障的连接（刚认证、还没订阅）绝不进实时目标集。
#[tokio::test]
async fn a_connection_without_a_barrier_receives_no_live_event() {
    let world = World::new();
    world.seed_device(DEVICE, &["session.list"]);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    let route = Arc::new(world.route(Arc::new(Reachability(true))));
    let (handle, mut receiver) = fixture::connection(DEVICE);
    let registry = Arc::new(ConnectionRegistry::new());
    registry.register(Arc::clone(&handle));
    route.on_authenticated(&handle, &[]);
    assert!(
        route.live_targets(&registry).is_empty(),
        "未建立屏障的连接绝不进实时目标集"
    );

    let reader = FakeReader::new();
    reader.push(fixture::agent_event(8));
    let frames = fan_out_one(
        &route,
        &registry,
        &reader,
        &mut receiver,
        committed(LIVE_ID, 8),
    )
    .await;
    assert!(
        fixture::bodies(&frames, "event").is_empty(),
        "没有屏障就不该收到任何实时事件"
    );
    assert!(frames.is_empty(), "一帧都不该有");
}

/// 读面失败时这一条不投递、也不伪造字段（客户端重连后按 cursor 重放时拿到同一条失败关闭）。
#[tokio::test]
async fn a_read_failure_skips_the_event_instead_of_forging_it() {
    let world = World::new();
    world.seed_device(DEVICE, &["session.list"]);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    let (route, registry, mut receiver) = bar_connection(&world, &[]).await;

    let reader = FakeReader::failing();
    let frames = fan_out_one(
        &route,
        &registry,
        &reader,
        &mut receiver,
        committed(LIVE_ID, 8),
    )
    .await;
    assert!(
        fixture::bodies(&frames, "event").is_empty(),
        "读面失败绝不投一条伪造字段的帧"
    );
    assert!(fixture::bodies(&frames, "error").is_empty());
}

/// 队列满（`try_send` 失败）时 `publish` 不阻塞、不 panic：事件已持久化，客户端按 cursor 重放补齐。
#[tokio::test]
async fn a_full_fanout_queue_never_blocks_the_publisher() {
    let mut publisher = SyncPublisher::channel();
    let mut queue = publisher.take_receiver().expect("新建发布端必然还有接收端");
    let event = committed("da702ba1-f49f-4e16-a677-a7744dfedb07", 7);
    // 远超队列容量（1024）：`try_send` 满即丢，调用方必须立即返回（否则 broker 的提交路径被阻塞）。
    for _ in 0..2048 {
        publisher.publish(acp_core::model::CommittedDelivery::Owned(event.clone()));
    }
    assert!(queue.try_recv().is_ok(), "队列仍可取到消息（说明没有阻塞）");
}
