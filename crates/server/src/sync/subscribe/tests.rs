//! `server::sync::subscribe` 的行为用例（变更 `sync-server-connection` 的任务 4.1/4.2/4.7 与 4.3 的
//! 可判定部分）。
//!
//! 驱动方式是**真实出站队列**：`MessageRoute::route` 处理真实信封，用例只看对端真的收到的帧。
//!
//! 本切片**未落地事件投递腿**（见模块文档：`event` 帧必需的 `origin`/`causationRequestId` 没有任何 core
//! 读面能给出）。因此用例对投递腿的断言是**否定的**：绝不谎报 `sync.caught_up`、恢复位置一律取客户端提交
//! 的 cursor（`fixture::World::replay_calls` 是它的观测口）。

use std::sync::Arc;

use acp_core::ports::SessionStore as _;

use super::*;
use crate::sync::conn::MessageRoute;
use crate::sync::subscribe::fixture::{
    self, DEVICE, EPOCH, Frame, OTHER_EPOCH, Reachability, SESSION, World,
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
        let route = Arc::new(world.route(Arc::new(Reachability(true))));
        let (handle, receiver) = fixture::connection(DEVICE);
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
    world.seed_device(DEVICE, &["session.list"]);
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

/// 序号域的分层（§3.2/§9.2）：**可表示**但大于 head 才是 `beyond_head`，超出 v1 上界是协议错误。
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

/// 序号超出 v1 上界是**协议错误**（`protocol.schema_invalid`，§3.2），不是 `sync.cursor_invalid`。
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
    world.seed_device(DEVICE, &["session.list"]);
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
    world.seed_device(DEVICE, &["session.list"]);
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
    world.seed_device(DEVICE, &["session.list"]);
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
    world.seed_device(DEVICE, &["session.list"]);
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
#[tokio::test]
async fn resubscribe_starts_from_the_submitted_cursor_not_the_stored_ack() {
    let world = World::new();
    world.seed_device(DEVICE, &["session.list"]);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    world.store.push_event(fixture::committed_event(
        "da702ba1-f49f-4e16-a677-a7744dfedba6",
        "turn.started",
        6,
        1,
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
    assert_eq!(
        world.store.replay_calls().last().cloned(),
        Some(Some(fixture::cursor(EPOCH, 5))),
        "恢复位置必须取客户端提交的 cursor"
    );
    assert!(
        fixture::bodies(&frames, "sync.caught_up").is_empty(),
        "5 之后仍有事件：不得谎报屏障（本切片不投递事件，见模块文档）"
    );
    assert_eq!(frames[0].code(), Some("internal.unavailable"));
}

/// 有待投递事件时**不谎报** `caught_up`（本切片未落地事件投递腿的失败关闭口径）。
#[tokio::test]
async fn pending_events_are_refused_instead_of_a_false_barrier() {
    let world = World::new();
    world.seed_device(DEVICE, &["session.list"]);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    world.store.push_event(fixture::committed_event(
        "da702ba1-f49f-4e16-a677-a7744dfedba6",
        "turn.started",
        6,
        1,
    ));
    let mut harness = Harness::new(&world, &[]);

    let frames = harness
        .send("sync.subscribe", subscribe_body(at(EPOCH, 5)))
        .await;
    assert!(
        fixture::bodies(&frames, "sync.caught_up").is_empty(),
        "屏障之后还有事件时不得声明已追平"
    );
    let errors = fixture::bodies(&frames, "error");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["code"], "internal.unavailable");
    assert_eq!(errors[0]["retryable"], true);
    assert_eq!(
        errors[0]["correlationId"], "04dbad03-8869-4c5a-94fa-2f3af2fb622b",
        "可关联到触发它的入站消息"
    );

    // 快照链路同样不得越过这一条：快照游标是快照开始时的水位（7），而**水位之后**又提交了一条事件
    // （序号 8，模拟与快照的 head 读竞争的那次提交）——快照 `end` 已发，但 `caught_up` 不发。
    world.store.push_event(fixture::committed_event(
        "da702ba1-f49f-4e16-a677-a7744dfedba7",
        "turn.completed",
        8,
        2,
    ));
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
    assert!(fixture::bodies(&frames, "sync.caught_up").is_empty());
    assert_eq!(
        fixture::bodies(&frames, "error")[0]["code"],
        "internal.unavailable"
    );
}

/// 设备 scope 不含 `session.list`：整个订阅面失败关闭，不回任何资源（任务 4.6 的授权前置）。
#[tokio::test]
async fn a_device_without_the_observe_scope_is_denied() {
    let world = World::new();
    world.seed_device(DEVICE, &["session.prompt"]);
    world.store.set_head(fixture::cursor(EPOCH, 7));
    let mut harness = Harness::new(&world, &[crate::sync::snapshot::LOCAL_CATALOG_FEATURE]);

    let frames = harness
        .send("sync.subscribe", subscribe_body(serde_json::Value::Null))
        .await;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].code(), Some("authorization.scope_denied"));
    assert!(
        fixture::bodies(&frames, "sync.reset_required").is_empty(),
        "无权设备连重置都不该拿到"
    );

    let frames = harness
        .send(
            "sync.snapshot_request",
            serde_json::json!({ "reason": "initial_sync" }),
        )
        .await;
    assert_eq!(frames[0].code(), Some("authorization.scope_denied"));
    assert!(fixture::bodies(&frames, "sync.snapshot_chunk").is_empty());
}

/// 连接结束即回收状态（状态表不随连接数单调增长）。
#[tokio::test]
async fn connection_state_is_reclaimed_when_the_connection_ends() {
    let world = World::new();
    world.seed_device(DEVICE, &["session.list"]);
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
