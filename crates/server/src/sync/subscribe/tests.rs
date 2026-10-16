//! `server::sync::subscribe` 的行为用例（变更 `sync-server-connection` 的任务 4.1/4.2/4.7 与 4.3/4.8
//! 的投递腿）。
//!
//! 驱动方式是**真实出站队列**：`MessageRoute::route` 处理真实信封，用例只看对端真的收到的帧。
//! 投递腿的观测口是 `fixture::World::store` 的 `sync_replay_calls`（每批的 `after`）与出站帧序列。

use std::sync::Arc;

use acp_core::ports::SessionStore as _;

use super::event::EVENT_ORIGIN_FEATURE;
use super::*;
use crate::sync::conn::MessageRoute;
use crate::sync::conn::SyncConfig;
use crate::sync::conn::limits::DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH;
use crate::sync::subscribe::fixture::{
    self, DEVICE, EPOCH, Frame, OTHER_EPOCH, OWNER, Reachability, SESSION, World,
};

/// 路由 + 连接 + 出站队列的固定装配。
struct Harness {
    route: Arc<SubscribeRoute>,
    handle: Arc<ConnectionHandle>,
    receiver: tokio::sync::mpsc::Receiver<crate::sync::conn::registry::Outbound>,
    sequence: u64,
}

impl Harness {
    fn new(world: &World, features: &[&str]) -> Self {
        Self::with_limit(world, features, None)
    }

    /// 带单批事件数上限的装配（`None` = 用 §14 的默认上限）。
    fn with_limit(world: &World, features: &[&str], events: Option<u32>) -> Self {
        let route = Arc::new(world.route(Arc::new(Reachability(true))));
        let config = SyncConfig {
            max_replay_events_per_batch: u64::from(events.unwrap_or(
                u32::try_from(DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH).expect("默认值在 u32 内"),
            )),
            ..SyncConfig::default()
        };
        let (handle, receiver) =
            fixture::connection_with_limits(DEVICE, fixture::CONNECTION, &config);
        let selected: Vec<String> = features.iter().map(|f| (*f).to_owned()).collect();
        route.on_authenticated(&handle, &selected);
        Self {
            route,
            handle,
            receiver,
            sequence: 0,
        }
    }
    /// 发一条消息并把新产生的帧取回。
    async fn send(&mut self, kind: &str, body: serde_json::Value) -> Vec<Frame> {
        self.sequence += 1;
        let envelope = fixture::envelope(kind, self.sequence, body);
        let outcome = self.route.route(&self.handle, &envelope).await;
        assert_eq!(outcome, RouteOutcome::Claimed, "{kind} 必须被本路由认领");
        fixture::frames(&mut self.receiver)
    }

    /// 发一条**原始 body**（形状非法的用例需要绕开 `serde_json::json!` 的合法形状）。
    async fn send_raw(&mut self, kind: &str, body: &str) -> Vec<Frame> {
        self.sequence += 1;
        let frame = format!(
            r#"{{"protocolVersion":1,"type":"{kind}","messageId":"04dbad03-8869-4c5a-94fa-2f3af2fb622b","connectionId":"{}","connectionSequence":"{}","body":{body}}}"#,
            fixture::CONNECTION,
            self.sequence
        );
        let envelope = Envelope::decode(&frame).expect("信封合法");
        let outcome = self.route.route(&self.handle, &envelope).await;
        assert_eq!(outcome, RouteOutcome::Claimed);
        fixture::frames(&mut self.receiver)
    }
}

fn subscribe_body(cursor: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "cursor": cursor, "scope": "machine" })
}

fn at(text_epoch: &str, sequence: u64) -> serde_json::Value {
    serde_json::json!({ "serverEpoch": text_epoch, "globalSequence": sequence.to_string() })
}

/// 与 [`at`] 同形，但序号直接给文本：超出 `u64` 的取值只能这样构造。
fn at_text(text_epoch: &str, sequence: &str) -> serde_json::Value {
    serde_json::json!({ "serverEpoch": text_epoch, "globalSequence": sequence })
}

/// 四种 cursor 失败原因可区分、且越界游标绝不按 head 续投（任务 4.1）。
#[tokio::test]
async fn cursor_faults_are_distinguishable_and_never_clamped() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    // 保留窗口下界 6：序号 3 已早于窗口。
    world.store.set_window_low(6);
    let mut harness = Harness::new(&world, &[]);

    // ① malformed：形状非法的游标回 `sync.cursor_invalid{malformed}`，且不要求重建快照。
    let frames = harness
        .send_raw(
            "sync.subscribe",
            &format!(r#"{{"cursor":{{"serverEpoch":"{EPOCH}","globalSequence":"abc"}},"scope":"machine"}}"#),
        )
        .await;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].code(), Some("sync.cursor_invalid"));
    assert_eq!(frames[0].reason(), Some("malformed"));

    // ② epoch_mismatch：既回错误也主动下发 `sync.reset_required{epoch_mismatch}`。
    let frames = harness
        .send("sync.subscribe", subscribe_body(at(OTHER_EPOCH, 3)))
        .await;
    assert_eq!(frames[0].reason(), Some("epoch_mismatch"));
    assert_eq!(
        fixture::bodies(&frames, "sync.reset_required")[0]["reason"],
        "epoch_mismatch"
    );

    // ③ beyond_head：回错误、不要求重建，且**不按 head 续投**（没有 caught_up、没有事件）。
    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 99)))
        .await;
    assert_eq!(frames[0].reason(), Some("beyond_head"));
    assert!(fixture::bodies(&frames, "sync.reset_required").is_empty());
    assert!(fixture::bodies(&frames, "sync.caught_up").is_empty());

    // ④ cursor_expired：形状与 epoch 都合法、但早于保留窗口（保留窗口下界只在存储层可见）。
    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 3)))
        .await;
    assert_eq!(frames[0].reason(), Some("cursor_expired"));
    assert_eq!(
        fixture::bodies(&frames, "sync.reset_required")[0]["reason"],
        "cursor_expired"
    );
    assert!(
        fixture::bodies(&frames, "sync.caught_up").is_empty(),
        "越界游标不得按 head 续投"
    );

    // 四种取值两两不同。
    let reasons: Vec<&str> = CursorFault::ALL
        .iter()
        .map(|fault| fault.as_str())
        .collect();
    assert_eq!(
        reasons,
        [
            "malformed",
            "epoch_mismatch",
            "beyond_head",
            "cursor_expired"
        ]
    );
}

/// 序号域的分层（§3.3/§9.2）：**可表示**但大于 head 才是 `beyond_head`，超出 v1 上界是协议错误。
///
/// 这是 finding「把"无法表示"折叠进 `beyond_head`」的回归闸门：把 `decide` 的域校验退回
/// `Rejected{BeyondHead}`（或让 `parse_sequence` 的 `None` 仍按 `beyond_head` 处置）时，这里的断言变红。
#[test]
fn the_sequence_domain_is_judged_before_the_head_comparison() {
    /// v1 上界本身（`2^63-1`）：可表示。
    const MAX: &str = "9223372036854775807";
    /// 上界 + 1：超出 v1 域，`u64` 仍可承载。
    const ABOVE_MAX: &str = "9223372036854775808";
    /// `u64` 的上界：仍可承载，但不是合法 v1 序号。
    const U64_MAX: &str = "18446744073709551615";
    /// 超出 `u64`：连承载都做不到（schema 的 `decimalString` 没有上界，因此它能通过解码）。
    const ABOVE_U64: &str = "18446744073709551616";

    let head = fixture::cursor(EPOCH, 7);
    let wire = |text_epoch: &str, text: &str| {
        SubmittedCursor::Value(
            serde_json::from_value::<Cursor>(at_text(text_epoch, text)).expect("游标形状合法"),
        )
    };

    // 上界本身可表示：大于 head 只是 `beyond_head`（不是 schema 错误）。
    assert_eq!(
        cursor::decide(&wire(EPOCH, MAX), &head),
        CursorDecision::Rejected {
            fault: CursorFault::BeyondHead,
            reset: None,
        }
    );

    // 上界之上（含超出 `u64`）一律是协议错误，且不映射到任何 `sync.cursor_invalid` / 重置。
    for text in [ABOVE_MAX, U64_MAX, ABOVE_U64, "99999999999999999999999"] {
        let submitted = wire(EPOCH, text);
        let SubmittedCursor::Value(submitted_cursor) = &submitted else {
            unreachable!("形状合法的游标")
        };
        let decision = cursor::decide(&submitted, &head);
        assert_eq!(decision, CursorDecision::SequenceOutOfRange, "序号 {text}");
        assert_eq!(
            cursor::fault_for(&decision),
            None,
            "序号 {text} 不是 cursor 判定"
        );
        assert_eq!(cursor::reset_for(&decision), None, "序号 {text} 不涉及重建");
        assert!(
            cursor::sequence_out_of_range(submitted_cursor),
            "序号 {text} 超出 v1 域（`sync.ack` 用的是同一判定）"
        );
    }

    // 域校验先于 epoch 比较：跨 epoch 但无法表示的取值同样是协议错误（不是 `epoch_mismatch`）。
    assert_eq!(
        cursor::decide(&wire(OTHER_EPOCH, "99999999999999999999999"), &head),
        CursorDecision::SequenceOutOfRange
    );
    // 反过来：可表示且同 epoch 的越界值仍是 `beyond_head`，不会被误判成 schema 错误。
    assert_eq!(
        cursor::decide(&wire(EPOCH, "8"), &head),
        CursorDecision::Rejected {
            fault: CursorFault::BeyondHead,
            reset: None,
        }
    );
}

/// 序号超出 v1 上界是**协议错误**（`protocol.schema_invalid`，§3.3），不是 `sync.cursor_invalid`。
///
/// 两条路径（`sync.subscribe` 的 cursor 判定与 `sync.ack`）必须同口径；同时保留"可表示但大于水位 →
/// `beyond_head`"的既有口径，把两者分开。
#[tokio::test]
async fn an_unrepresentable_sequence_is_a_protocol_error_on_both_paths() {
    /// v1 上界本身（`2^63-1`）：可表示。
    const MAX: &str = "9223372036854775807";
    /// 上界 + 1：超出 v1 域。
    const ABOVE_MAX: &str = "9223372036854775808";
    /// 超出 `u64`：连承载都做不到（`decimalString` 没有上界，因此能通过解码走到这里）。
    const ABOVE_U64: &str = "18446744073709551616";

    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    let mut harness = Harness::new(&world, &[]);

    // ① `sync.subscribe`：上界本身可表示，比 head 大只是 `beyond_head`（既有口径不变）。
    let frames = harness
        .send("sync.subscribe", subscribe_body(at_text(EPOCH, MAX)))
        .await;
    assert_eq!(frames[0].code(), Some("sync.cursor_invalid"));
    assert_eq!(frames[0].reason(), Some("beyond_head"));

    // ② `sync.subscribe`：超出上界（含超出 `u64`）→ `protocol.schema_invalid`。
    for text in [ABOVE_MAX, ABOVE_U64, "99999999999999999999999"] {
        let frames = harness
            .send("sync.subscribe", subscribe_body(at_text(EPOCH, text)))
            .await;
        assert_eq!(frames.len(), 1, "序号 {text} 只回一条 error");
        assert_eq!(
            frames[0].code(),
            Some("protocol.schema_invalid"),
            "序号 {text} MUST NOT 回 sync.cursor_invalid{{beyond_head}}"
        );
        assert_eq!(
            frames[0].reason(),
            None,
            "序号 {text} 没有 cursor_invalid 的 reason"
        );
        assert!(
            fixture::bodies(&frames, "sync.reset_required").is_empty(),
            "序号 {text} 不要求重建快照"
        );
    }

    // ③ `sync.ack`：先以有效 cursor 订阅，拿到屏障 7。
    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 5)))
        .await;
    assert_eq!(
        fixture::bodies(&frames, "sync.caught_up")[0]["cursor"]["globalSequence"],
        "7"
    );

    // 可表示且超过屏障 → `beyond_head`（既有口径不变）。
    let frames = harness
        .send("sync.ack", serde_json::json!({ "cursor": at(EPOCH, 8) }))
        .await;
    assert_eq!(frames[0].code(), Some("sync.cursor_invalid"));
    assert_eq!(frames[0].reason(), Some("beyond_head"));

    // 超出上界 → `protocol.schema_invalid`（与订阅路径同一结论）。
    for text in [ABOVE_MAX, ABOVE_U64] {
        let frames = harness
            .send(
                "sync.ack",
                serde_json::json!({ "cursor": at_text(EPOCH, text) }),
            )
            .await;
        assert_eq!(frames.len(), 1, "序号 {text} 只回一条 error");
        assert_eq!(
            frames[0].code(),
            Some("protocol.schema_invalid"),
            "序号 {text} MUST NOT 回 sync.cursor_invalid{{beyond_head}}"
        );
        assert_eq!(frames[0].reason(), None, "序号 {text}");
    }

    // 域校验先于 epoch 比较（两条路径同口径）：跨 epoch 且无法表示的取值仍是协议错误，
    // 不会被 `epoch_mismatch` 抢先定案。
    let frames = harness
        .send(
            "sync.ack",
            serde_json::json!({ "cursor": at_text(OTHER_EPOCH, ABOVE_MAX) }),
        )
        .await;
    assert_eq!(frames[0].code(), Some("protocol.schema_invalid"));
    assert_eq!(frames[0].reason(), None);
}

/// 首次同步：`reset_required{initial_sync}` → 快照 → 屏障 → ACK（§9.2/§9.4/§9.5）。
#[tokio::test]
async fn first_sync_walks_reset_then_snapshot_then_caught_up() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    world.seed_workspace("project", "Project", &fixture::workspace_path());
    world
        .store
        .push_session(fixture::core_session(SESSION, "本地会话", None));
    let mut harness = Harness::new(
        &world,
        &[
            crate::sync::snapshot::LOCAL_CATALOG_FEATURE,
            crate::sync::snapshot::REMOTE_ORIGIN_FEATURE,
        ],
    );

    // `cursor: null` → 主动下发 `initial_sync` 重置（客户端据此发 `sync.snapshot_request`）。
    let frames = harness
        .send("sync.subscribe", subscribe_body(serde_json::Value::Null))
        .await;
    let resets = fixture::bodies(&frames, "sync.reset_required");
    assert_eq!(resets.len(), 1);
    assert_eq!(resets[0]["reason"], "initial_sync");
    assert_eq!(resets[0]["snapshotAvailable"], true);
    assert!(
        fixture::bodies(&frames, "error").is_empty(),
        "首次同步不回 cursor_invalid"
    );

    // 客户端请求快照：begin → 3 chunks → end → caught_up（屏障之后没有事件）。
    let frames = harness
        .send(
            "sync.snapshot_request",
            serde_json::json!({ "reason": "initial_sync" }),
        )
        .await;
    assert_eq!(
        fixture::bodies(&frames, "sync.snapshot_begin")[0]["chunkCount"],
        3
    );
    assert_eq!(fixture::bodies(&frames, "sync.snapshot_chunk").len(), 3);
    assert_eq!(fixture::bodies(&frames, "sync.snapshot_end").len(), 1);
    let barriers = fixture::bodies(&frames, "sync.caught_up");
    assert_eq!(barriers.len(), 1, "重放结束后必须发屏障");
    assert_eq!(barriers[0]["cursor"]["globalSequence"], "7");

    // 屏障可被 ACK：即使该段没有对它可见的事件（§9.5 的场景）。
    let frames = harness
        .send("sync.ack", serde_json::json!({ "cursor": at(EPOCH, 7) }))
        .await;
    assert!(frames.is_empty(), "合法 ACK 不产生任何对端可见消息");
}

/// `cache_incompatible` 只由客户端自报，服务端**每连接确权一次**（防回声循环）。
#[tokio::test]
async fn cache_incompatible_reset_is_confirmed_once_per_connection() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    let mut harness = Harness::new(&world, &[]);

    let frames = harness
        .send(
            "sync.snapshot_request",
            serde_json::json!({ "reason": "cache_incompatible" }),
        )
        .await;
    let resets = fixture::bodies(&frames, "sync.reset_required");
    assert_eq!(resets.len(), 1);
    assert_eq!(resets[0]["reason"], "cache_incompatible");
    assert_eq!(
        fixture::bodies(&frames, "sync.snapshot_end").len(),
        1,
        "确权之后照常下发快照"
    );

    // 客户端收到 `reset_required` 会回声一条 `snapshot_request`：第二次不得再确权（否则无限往返）。
    let frames = harness
        .send(
            "sync.snapshot_request",
            serde_json::json!({ "reason": "cache_incompatible" }),
        )
        .await;
    assert!(fixture::bodies(&frames, "sync.reset_required").is_empty());
    assert_eq!(fixture::bodies(&frames, "sync.snapshot_end").len(), 1);
}

/// ACK 单调不减、不得越过本连接已发送的最高游标（§9.5；任务 4.7）。
#[tokio::test]
async fn ack_is_monotonic_and_bounded_by_the_sent_barrier() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    let mut harness = Harness::new(&world, &[]);

    // 以有效 cursor 订阅：屏障之后没有事件，因此屏障就是当前水位 7。
    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 5)))
        .await;
    assert_eq!(
        fixture::bodies(&frames, "sync.caught_up")[0]["cursor"]["globalSequence"],
        "7"
    );

    // 屏障 ACK 被接受。
    let frames = harness
        .send("sync.ack", serde_json::json!({ "cursor": at(EPOCH, 7) }))
        .await;
    assert!(frames.is_empty());

    // 回退 ACK 被忽略（无对端可见后果），服务端已知进度不变。
    let frames = harness
        .send("sync.ack", serde_json::json!({ "cursor": at(EPOCH, 6) }))
        .await;
    assert!(frames.is_empty(), "回退被忽略并只记诊断");

    // 超过本连接已发送的最高游标 → `sync.cursor_invalid`。
    let frames = harness
        .send("sync.ack", serde_json::json!({ "cursor": at(EPOCH, 8) }))
        .await;
    assert_eq!(frames[0].code(), Some("sync.cursor_invalid"));
    assert_eq!(frames[0].reason(), Some("beyond_head"));

    // 跨 epoch 的 ACK 同样不可采纳（序号不可比）。
    let frames = harness
        .send(
            "sync.ack",
            serde_json::json!({ "cursor": at(OTHER_EPOCH, 1) }),
        )
        .await;
    assert_eq!(frames[0].code(), Some("sync.cursor_invalid"));
    assert_eq!(frames[0].reason(), Some("epoch_mismatch"));

    // ACK 不推进事件库 head：水位仍是 7，且 ACK 不触发任何重放读。
    assert_eq!(
        world
            .store
            .head()
            .await
            .expect("水位")
            .global_sequence
            .get(),
        7
    );
    assert_eq!(world.store.replay_calls().len(), 1, "ACK 不触发任何重放读");
}

/// 恢复位置一律取客户端提交的 cursor，而不是服务端保存的最大 ACK（§9.5；任务 4.7）。
///
/// 投递腿落地后这条用例**同时**是重放路径的观测口：Sync 事件读面每次收到的 `after` 都能断言。
#[tokio::test]
async fn resubscribe_starts_from_the_submitted_cursor_not_the_stored_ack() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    world.store.push_sync_event(fixture::sync_event(
        "da702ba1-f49f-4e16-a677-a7744dfedba6",
        6,
        acp_core::model::EventOrigin::Agent,
        None,
        None,
        None,
    ));
    let mut harness = Harness::new(&world, &[]);

    // 第一次：从 6 之后续传（空），屏障 7，客户端 ACK 7。
    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 6)))
        .await;
    assert_eq!(
        fixture::bodies(&frames, "sync.caught_up")[0]["cursor"]["globalSequence"],
        "7"
    );
    harness
        .send("sync.ack", serde_json::json!({ "cursor": at(EPOCH, 7) }))
        .await;

    // 以更旧的 cursor 重订阅：服务端必须从 5 之后读（而不是跳过到已 ACK 的 7）。
    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 5)))
        .await;
    let calls = world.store.sync_replay_calls();
    assert_eq!(
        calls.get(1).cloned(),
        Some(fixture::cursor(EPOCH, 5)),
        "第二次订阅的第一批必须从客户端提交的 5 之后读（calls = {calls:?}）"
    );
    // 6 那条事件因此真的被投出来（而不是被最大 ACK 跳过），其后照常收到屏障 7。
    let events = fixture::bodies(&frames, "event");
    assert_eq!(events.len(), 1, "5 之后只有序号 6 一条可见事件");
    assert_eq!(events[0]["globalSequence"], "6");
    assert_eq!(
        fixture::bodies(&frames, "sync.caught_up")[0]["cursor"]["globalSequence"],
        "7"
    );
    assert!(
        fixture::bodies(&frames, "error").is_empty(),
        "正常投递不回错误（失败关闭路径不再出现）"
    );
}

/// 有效 cursor + 若干可见事件 → 按序 `event` 帧 + `caught_up`，且**失败关闭路径不再出现**
/// （任务 4.3/4.8 的主判据）。
#[tokio::test]
async fn a_valid_cursor_replays_events_then_announces_the_barrier() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    // 序号 6、7 两条在游标 5 之后可见的事件（`agent` 来源：不带身份字段）。
    for (id, sequence) in [
        ("da702ba1-f49f-4e16-a677-a7744dfedba6", 6),
        ("da702ba1-f49f-4e16-a677-a7744dfedba7", 7),
    ] {
        world.store.push_sync_event(fixture::sync_event(
            id,
            sequence,
            acp_core::model::EventOrigin::Agent,
            None,
            None,
            None,
        ));
    }
    let mut harness = Harness::new(&world, &[]);

    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 5)))
        .await;
    let events = fixture::bodies(&frames, "event");
    assert_eq!(events.len(), 2, "5 之后两条可见事件都投出来");
    assert_eq!(events[0]["globalSequence"], "6");
    assert_eq!(events[1]["globalSequence"], "7", "按 globalSequence 升序");
    // §10.1 的必填字段逐项到场（这一条同时钉住 `event` 帧的形状）。
    assert_eq!(events[0]["origin"]["kind"], "agent");
    assert_eq!(events[0]["origin"]["deviceId"], serde_json::Value::Null);
    assert_eq!(events[0]["causationRequestId"], serde_json::Value::Null);
    assert_eq!(events[0]["remoteOrigin"], serde_json::Value::Null);
    assert_eq!(
        events[0]["payload"]["view"],
        serde_json::json!({"global": 6})
    );
    assert!(
        !events[0].get("origin").unwrap().get("nodeId").is_some(),
        "未协商 core.event-origin.v1：origin 不带 nodeId 键"
    );

    let barriers = fixture::bodies(&frames, "sync.caught_up");
    assert_eq!(barriers.len(), 1, "投完必须发屏障");
    assert_eq!(barriers[0]["cursor"]["globalSequence"], "7");

    // 失败关闭路径在正常有事件时**不再出现**（4.8 的显式判据）。
    let errors = fixture::bodies(&frames, "error");
    assert!(errors.is_empty(), "正常投递不回 error，得到 {errors:?}");

    // 屏障可被 ACK（§9.5：ACK 上限就是本连接已发送的最高游标）。
    let frames = harness
        .send("sync.ack", serde_json::json!({ "cursor": at(EPOCH, 7) }))
        .await;
    assert!(frames.is_empty(), "合法 ACK 不产生任何对端可见消息");
}

/// 快照链路与重放共用同一道推进：快照 `end` 之后若有 cursor 之后的事件，按序补投并补齐屏障。
///
/// 快照游标取**开始组装时**的水位（7），而水位之后（序号 8）又提交了一条事件（模拟与 head 读竞争的
/// 那次提交）——这条事件绝不能被漏掉（§9.4 的 `event where globalSequence > 快照 cursor`）。
#[tokio::test]
async fn an_event_committed_after_the_snapshot_head_is_still_delivered() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    world
        .store
        .push_session(fixture::core_session(SESSION, "本地会话", None));
    let mut harness = Harness::new(&world, &[]);

    // 首次同步走快照：序号 7 那条在快照 cursor 上，不在"之后"，因此快照后不重投它。
    let frames = harness
        .send("sync.subscribe", subscribe_body(serde_json::Value::Null))
        .await;
    assert_eq!(
        fixture::bodies(&frames, "sync.reset_required")[0]["reason"],
        "initial_sync"
    );
    let frames = harness
        .send(
            "sync.snapshot_request",
            serde_json::json!({ "reason": "initial_sync" }),
        )
        .await;
    assert_eq!(fixture::bodies(&frames, "sync.snapshot_end").len(), 1);
    assert_eq!(
        fixture::bodies(&frames, "sync.snapshot_end")[0]["cursor"]["globalSequence"],
        "7",
        "快照游标是开始组装时的水位"
    );
    assert_eq!(
        fixture::bodies(&frames, "sync.caught_up")[0]["cursor"]["globalSequence"],
        "7",
        "快照 cursor 之后没有事件：屏障就是它"
    );

    // 水位之后又提交一条事件（序号 8）：重新订阅时它必须按序补投，然后补齐新屏障 8。
    world.store.push_sync_event(fixture::sync_event(
        "da702ba1-f49f-4e16-a677-a7744dfedba6",
        7,
        acp_core::model::EventOrigin::Agent,
        None,
        None,
        None,
    ));
    world.store.push_sync_event(fixture::sync_event(
        "da702ba1-f49f-4e16-a677-a7744dfedba7",
        8,
        acp_core::model::EventOrigin::Agent,
        None,
        None,
        None,
    ));
    world.store.set_head(fixture::cursor(EPOCH, 8));
    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 6)))
        .await;
    let sequences: Vec<&str> = fixture::bodies(&frames, "event")
        .iter()
        .map(|body| body["globalSequence"].as_str().expect("序号是字符串"))
        .collect();
    assert_eq!(
        sequences,
        ["7", "8"],
        "cursor 之后的事件按 globalSequence 升序补投"
    );
    assert_eq!(
        fixture::bodies(&frames, "sync.caught_up")[0]["cursor"]["globalSequence"],
        "8",
        "屏障取本批读视图的 head（含序号 8）"
    );
    assert!(
        fixture::bodies(&frames, "error").is_empty(),
        "正常投递不回错误"
    );
}

/// 分批投递：页长小于可见事件数时，跨批仍按 `globalSequence` 升序，屏障取最后一批的 head。
///
/// 观测口是 `sync_replay_calls`：每批的 `after` 必须停在上一批最后一条（推进屏障游标），而不是原地打转。
#[tokio::test]
async fn a_large_window_is_delivered_in_batches_without_repeating_or_reordering() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 4));
    for sequence in 1..=4u64 {
        world.store.push_sync_event(fixture::agent_event(sequence));
    }
    let mut harness = Harness::with_limit(&world, &[], Some(2));
    // 单批两条（`maxReplayEventsPerBatch` 的下限）：四 events 因此分两批。

    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 0)))
        .await;
    let sequences: Vec<&str> = fixture::bodies(&frames, "event")
        .iter()
        .map(|body| body["globalSequence"].as_str().expect("序号是字符串"))
        .collect();
    assert_eq!(
        sequences,
        ["1", "2", "3", "4"],
        "跨批仍按全局序号升序、无重复"
    );
    let calls = world.store.sync_replay_calls();
    assert_eq!(calls.len(), 3, "两批满页 + 一页空页（屏障）");
    assert_eq!(
        calls,
        vec![
            fixture::cursor(EPOCH, 0),
            fixture::cursor(EPOCH, 2),
            fixture::cursor(EPOCH, 4)
        ],
        "屏障游标在批与批之间必须前进"
    );
    assert_eq!(
        fixture::bodies(&frames, "sync.caught_up")[0]["cursor"]["globalSequence"],
        "4"
    );
    assert!(fixture::bodies(&frames, "error").is_empty());
}

/// 重放期间到达的实时事件**不出现在屏障之前**（任务 4.3/4.8 的 barrier 判据）。
///
/// `sync.caught_up` 就是那条屏障：实时事件由 [`crate::sync::subscribe::fanout`] 投递，而它只投
/// **已建立屏障**的连接（[`SubscribeRoute::live_targets`]）——重放中的连接（还没有屏障）绝不进实时
/// 目标集，因此帧序上不可能出现"实时事件越过序号更早的已持久化事件"。本用例把这条闸门做成可断言的
/// 事实：订阅前不在实时目标集里、屏障建立之后才在。
#[tokio::test]
async fn a_live_event_arriving_mid_replay_is_not_delivered_before_the_barrier() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 4));
    for sequence in 1..=4u64 {
        world.store.push_sync_event(fixture::agent_event(sequence));
    }
    let mut harness = Harness::new(&world, &[]);
    world.registry.register(Arc::clone(&harness.handle));
    // 连接刚认证、尚未订阅：不在实时目标集里（barrier 未建立）。
    assert!(
        harness.route.live_targets(&world.registry).is_empty(),
        "未建立屏障的连接绝不进实时目标集"
    );

    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 0)))
        .await;
    let sequences: Vec<&str> = fixture::bodies(&frames, "event")
        .iter()
        .map(|body| body["globalSequence"].as_str().expect("序号是字符串"))
        .collect();
    assert_eq!(sequences, ["1", "2", "3", "4"], "重放按全局序号升序");
    assert_eq!(
        fixture::bodies(&frames, "sync.caught_up")[0]["cursor"]["globalSequence"],
        "4",
        "屏障游标到达 head"
    );
    // 屏障建立之后该连接才进入实时目标集：实时事件因此只能排在屏障之后。
    assert_eq!(
        harness.route.live_targets(&world.registry).len(),
        1,
        "屏障建立后实时扇出才看得见这条连接"
    );
}

/// 重放**尚未走到 head** 时，闸门绝不打开。
///
/// 上面的用例只在订阅前后各取一次样，看不到重放进行中的闸门状态 —— 那正是「闸门何时打开」这一维度
/// 的盲区：若把 `live_targets` 的 barrier 判据去掉、或在订阅到达时提前闭合杠杆点，那条用例仍然全绿。
///
/// 本用例的做法：页长 1（重放因此分多批推进），并在**数据页**上注入读失败 —— 失败路径会
/// `clear_barrier` 并拒绝，因此断言「此刻仍不在实时目标集」就直接证明了「闸门不在读到 head 之前打开」。
/// 断言用的 `live_targets` 与生产实时扇出是同一道闸（同一个方法、同一份 `states`），不是替身自证。
#[tokio::test]
async fn a_connection_is_not_a_live_target_while_the_replay_is_still_running() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 4));
    for sequence in 1..=4u64 {
        world.store.push_sync_event(fixture::agent_event(sequence));
    }
    // 页长 1：重放必须分批推进，「已投若干帧、尚未到 head」这个中间态因此真实存在。
    let mut harness = Harness::with_limit(&world, &[], Some(1));
    world.registry.register(Arc::clone(&harness.handle));
    // 数据页读到一半即失败：失败路径清杠杆点并拒绝，绝不谎报屏障。
    world
        .store
        .fail_sync_replay(acp_core::model::PortError::Corrupt("探针"));

    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 0)))
        .await;
    // 闸门未开 ⇒ 这条连接不是实时目标（重放尚未走到 head）。
    assert!(
        harness.route.live_targets(&world.registry).is_empty(),
        "重放未走到 head 时，连接绝不进实时目标集"
    );
    // 失败必须以显式拒绝收场，且不得发出 `caught_up`（那是"已到 head"的断言）。
    assert!(
        !fixture::bodies(&frames, "sync.caught_up").is_empty()
            || !fixture::bodies(&frames, "error").is_empty(),
        "失败必须以拒绝收场（而非静默）"
    );
    assert!(
        fixture::bodies(&frames, "sync.caught_up").is_empty(),
        "没有读到 head 就不得发 catch_up"
    );
}

/// 权限过滤造成的序号空洞合法且不报错，屏障游标高于该段（任务 4.3）。
///
/// 构造方式：设备**没有** `session.list` 之外的可见性问题时，这里的"过滤"由 core 读面给出——注入一条
/// `origin_kind` 词表外的行会让**整批**读失败关闭（那是历史行处置，另有用例）；本用例的过滤语义按
/// §9.5 的场景表达：被跳过的序号不出现在 `event` 帧里、不回错误、且屏障高于它。
#[tokio::test]
async fn filtered_gaps_are_legal_and_the_barrier_advances_past_them() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 9));
    // 序号 2、5、9 三条可见；3/4、6/7/8 对该设备不可见（模拟权限过滤造成的空洞）。
    for sequence in [2u64, 5, 9] {
        world.store.push_sync_event(fixture::agent_event(sequence));
    }
    let mut harness = Harness::new(&world, &[]);

    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 1)))
        .await;
    let sequences: Vec<&str> = fixture::bodies(&frames, "event")
        .iter()
        .map(|body| body["globalSequence"].as_str().expect("序号是字符串"))
        .collect();
    assert_eq!(sequences, ["2", "5", "9"], "不可见序号不出现（空洞）");
    assert!(
        fixture::bodies(&frames, "error").is_empty(),
        "空洞不报错（§9.5）"
    );
    assert_eq!(
        fixture::bodies(&frames, "sync.caught_up")[0]["cursor"]["globalSequence"],
        "9",
        "屏障游标高于被过滤的整段"
    );
    // 客户端据此 ACK 而不被判为缺口（§9.5）。
    let frames = harness
        .send("sync.ack", serde_json::json!({ "cursor": at(EPOCH, 9) }))
        .await;
    assert!(frames.is_empty(), "屏障 ACK 被接受");
}

/// `origin` 的身份字段按 feature `core.event-origin.v1` 协商决定：协商带 `nodeId` 键（无节点产生者
/// 取 `null`），未协商整键缺席；`kind`/`deviceId` 两种情况逐字节相同（绝不因客户端版本降级 `kind`）。
#[tokio::test]
async fn the_origin_shape_follows_the_negotiated_event_origin_feature() {
    /// 协商与未协商各跑一次订阅，取回全部 `event` 帧的 `origin`。
    async fn origins(world: &World, features: &[&str]) -> Vec<serde_json::Value> {
        let mut harness = Harness::new(world, features);
        let frames = harness
            .send("sync.subscribe", subscribe_body(at(EPOCH, 0)))
            .await;
        fixture::bodies(&frames, "event")
            .into_iter()
            .map(|body| body["origin"].clone())
            .collect()
    }

    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 2));
    // 序号 1：`device` 来源（带非空 `deviceId`、`nodeId` 恒为 `null`）。
    world.store.push_sync_event(fixture::sync_event(
        "da702ba1-f49f-4e16-a677-a7744dfedb01",
        1,
        acp_core::model::EventOrigin::Device,
        Some(DEVICE),
        None,
        Some("4c4dafda-dd98-442e-8d55-252b75bac72d"),
    ));
    // 序号 2：`node` 来源（另一个节点经 Node Link 命令产生）。
    world.store.push_sync_event(fixture::sync_event(
        "da702ba1-f49f-4e16-a677-a7744dfedb02",
        2,
        acp_core::model::EventOrigin::Node,
        None,
        Some(OWNER),
        Some("4c4dafda-dd98-442e-8d55-252b75bac73e"),
    ));

    // 协商了 `core.event-origin.v1`：`origin` 恒带 `nodeId` 键。
    let negotiated = origins(&world, &[EVENT_ORIGIN_FEATURE]).await;
    assert_eq!(negotiated.len(), 2);
    assert_eq!(negotiated[0]["kind"], "device");
    assert_eq!(negotiated[0]["deviceId"], DEVICE);
    assert_eq!(
        negotiated[0]["nodeId"],
        serde_json::Value::Null,
        "device 来源没有节点产生者：nodeId 键在场且取 null"
    );
    assert_eq!(negotiated[1]["kind"], "node");
    assert_eq!(
        negotiated[1]["nodeId"], OWNER,
        "node 来源必须带非空 nodeId（§10.1 的配对规则）"
    );
    assert_eq!(negotiated[1]["deviceId"], serde_json::Value::Null);

    // 未协商：`origin` 只有 `kind`/`deviceId` 两个键，且 `kind = node` 的事件**整条不投递**
    // （服务端 MUST NOT 因客户端版本伪造或降级 kind）。
    let legacy = origins(&world, &[]).await;
    assert_eq!(legacy.len(), 1, "node 来源事件对未协商连接不投递");
    assert_eq!(legacy[0]["kind"], "device");
    assert_eq!(legacy[0]["deviceId"], DEVICE);
    assert!(
        legacy[0].get("nodeId").is_none(),
        "未协商时 origin 不得出现 nodeId 键（旧客户端会按 schema 拒绝整条事件）"
    );
}

/// 无法归因的历史事件（读面 `Corrupt`）：投递层显式失败关闭，且**不伪造** `kind`/`deviceId`。
///
/// 完成判据（任务 4.8 的历史行处置）：协商了该 feature 的连接 MUST NOT 收到 `deviceId` 为 `null` 的
/// `kind = device` 事件；这里注入一条 `Corrupt`，断言回 `internal.unavailable`、不发 `caught_up`、
/// 不发任何 `event` 帧。
#[tokio::test]
async fn an_unattributable_history_row_fails_closed_instead_of_forging_an_identity() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    world
        .store
        .fail_sync_replay(acp_core::model::PortError::Corrupt("origin"));
    let mut harness = Harness::new(&world, &[EVENT_ORIGIN_FEATURE]);

    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 5)))
        .await;
    assert!(
        fixture::bodies(&frames, "event").is_empty(),
        "MUST NOT 伪造 origin 字段发一条非法帧"
    );
    assert!(
        fixture::bodies(&frames, "sync.caught_up").is_empty(),
        "MUST NOT 谎报屏障"
    );
    let errors = fixture::bodies(&frames, "error");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["code"], "internal.unavailable");
    assert_eq!(errors[0]["retryable"], true);
    assert_eq!(
        errors[0]["correlationId"], "04dbad03-8869-4c5a-94fa-2f3af2fb622b",
        "可关联到触发它的入站消息"
    );
}

/// 本机没有该设备的信任行：整个订阅面认证类失败关闭，不回任何资源。
///
/// 设备方向不再有授权过滤（`pairing-model` 删除设备权限层），因此订阅面的唯一准入判定就是
/// 「持久信任行存在」；这条用例把旧的「无 scope 被拒」换成它——旧实现在这里会给出
/// `authorization.scope_denied`，而该错误码已不由设备方向产生。
#[tokio::test]
async fn a_device_without_a_trust_row_is_denied() {
    let world = World::new();
    // 故意不 seed_device：连接声称的设备在本机没有信任行。
    world.store.set_head(fixture::cursor(EPOCH, 7));
    let mut harness = Harness::new(&world, &[crate::sync::snapshot::LOCAL_CATALOG_FEATURE]);

    let frames = harness
        .send("sync.subscribe", subscribe_body(serde_json::Value::Null))
        .await;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].code(), Some("auth.device_unknown"));
    assert!(
        fixture::bodies(&frames, "sync.reset_required").is_empty(),
        "失败关闭的连接不该拿到重置"
    );

    let frames = harness
        .send(
            "sync.snapshot_request",
            serde_json::json!({ "reason": "initial_sync" }),
        )
        .await;
    assert_eq!(frames[0].code(), Some("auth.device_unknown"));
    assert!(fixture::bodies(&frames, "sync.snapshot_chunk").is_empty());
}

/// 连接结束即回收状态（状态表不随连接数单调增长）。
#[tokio::test]
async fn connection_state_is_reclaimed_when_the_connection_ends() {
    let world = World::new();
    world.seed_device(DEVICE);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    let route = Arc::new(world.route(Arc::new(Reachability(true))));

    let (first, _first_receiver) =
        fixture::connection_with(DEVICE, "9de54db7-74ae-4c21-88e3-05df0dc2e631");
    let (second, mut second_receiver) =
        fixture::connection_with(DEVICE, "9de54db7-74ae-4c21-88e3-05df0dc2e632");
    world.registry.register(Arc::clone(&first));
    world.registry.register(Arc::clone(&second));
    for handle in [&first, &second] {
        route.on_authenticated(handle, &[]);
    }
    assert_eq!(route.tracked(), 2);

    // 第一条连接结束（摘除注册表）之后，第二条连接的任何一条入站消息都会顺带回收它的状态。
    world.registry.unregister(&first);
    let envelope = fixture::envelope("sync.subscribe", 1, subscribe_body(serde_json::Value::Null));
    assert_eq!(route.route(&second, &envelope).await, RouteOutcome::Claimed);
    let _ = fixture::frames(&mut second_receiver);
    assert_eq!(route.tracked(), 1);
}

/// 「未协商 `acp.raw-payload.v1` 时不下发原文容器」必须在**真实重放路径**上成立。
///
/// WP1 新增的门控在 `sync::event::frame` 里；若它没有被 [`SubscribeRoute::advance`] 接进来，
/// 本条会红 —— 这是「模块被孤立测试、从未接线」的回归保护（`advance` 曾直接调
/// `subscribe::event::event_body`，那时门控在任何真实连接上都不生效）。
#[tokio::test]
async fn the_raw_payload_gate_applies_on_the_replay_path() {
    /// 一条带 ACP 原文容器（不可用分支，免去摘要计算）的事件。
    fn event_with_raw_payload() -> acp_core::ports::SyncEventRecord {
        let mut record = fixture::sync_event(
            "da702ba1-f49f-4e16-a677-a7744dfedbc1",
            6,
            acp_core::model::EventOrigin::Agent,
            None,
            None,
            None,
        );
        record.payload = acp_core::model::EventPayload::new(
            acp_core::model::ViewJson::new(
                r#"{"turnId":"7f3e9a10-5c2b-4d8e-9f01-3a6b5c7d8e9f","state":"running"}"#,
            )
            .expect("view"),
            Some(acp_core::model::AcpRaw::Unavailable {
                reason: acp_core::model::RawUnavailableReason::RetentionExpired,
                byte_length: 7,
                sha256: None,
            }),
        );
        record
    }

    for (features, expect_container) in [(&[][..], false), (&["acp.raw-payload.v1"][..], true)] {
        let world = World::new();
        world.seed_device(DEVICE);
        world.store.set_head(fixture::cursor(EPOCH, 6));
        world.store.push_sync_event(event_with_raw_payload());
        let mut harness = Harness::new(&world, features);

        let frames = harness
            .send("sync.subscribe", subscribe_body(at(EPOCH, 5)))
            .await;
        let events = fixture::bodies(&frames, "event");
        assert_eq!(events.len(), 1, "事件必须照常投出（门控不改这一条）");
        let payload = &events[0]["payload"];
        assert_eq!(
            payload.get("acp").is_some(),
            expect_container,
            "协商集合 {features:?} 下 acp 容器是否携带不符；实际 payload={payload}"
        );
        assert!(
            payload.get("view").is_some(),
            "无论是否协商，view 都必须下发"
        );
    }
}
