//! Independent RB-001 internal acceptance. Controlled duplex ACP is not a real Harness.
use super::{
    CapacityFreshness, CapacityObservation, CapacityState, CapacityUpdate, NativeLifecyclePort,
    ObservationOrigin, ProtocolVersion, capacity_activity::CapacityActivity,
};
use crate::{
    adapters::acp::{
        ConfigSourceStamp, Connection, ExtensionFields, FenceRejection, Incoming, IoOwner, Known,
        ObservationAcknowledger, ObservationLease, ObservationReceiver, PollIo, ResponseDeadline,
        RpcId, SessionId, SessionSetup, SetupSnapshot, UpdateOrigin, Version, ViewReadContextKind,
        WriteFence, WriteIdentity, WriteReport, WriteStep, native_dispatch::AcpNativeAdapter,
        raw_json::RawDocument,
    },
    domain::session_capacity::{Coverage, RecycleEligibility},
};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream},
    time::timeout,
};

struct Permit;
impl WriteFence for Permit {
    fn check_and_poll(
        &self,
        _: &WriteIdentity,
        _: WriteStep,
        cx: &mut std::task::Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let _ = io.poll(cx);
        Ok(())
    }
}

struct Peer {
    connection: Connection,
    receiver: ObservationReceiver,
    ack: ObservationAcknowledger,
    owner: IoOwner,
    stream: DuplexStream,
}
impl Peer {
    async fn start() -> Self {
        let (local, peer) = tokio::io::duplex(65536);
        let (read, write) = tokio::io::split(local);
        let mut owner = None;
        let (start, receiver, ack, _) = Connection::prepare_observed(read, write, |io| {
            owner = Some(io);
            Ok(())
        })
        .expect("owned actual transport");
        let mut result = Self {
            connection: start.start(),
            receiver,
            ack,
            stream: peer,
            owner: owner.expect("adopt original IO"),
        };
        let reservation = result
            .connection
            .reserve_initialize(Version::V2, deadline())
            .unwrap();
        let id = result.write(reservation).await;
        result
            .feed(&[json!({"jsonrpc":"2.0","id":rpc_json(&id),"result":{
                "protocolVersion":2,"info":{"name":"independent-controlled-peer","version":"fixed"},
                "capabilities":{"session":{}}
            }})])
            .await;
        let lease = result.next().await;
        result.ack.acknowledge(lease).unwrap();
        result
    }
    async fn write(&mut self, reservation: crate::adapters::acp::Reservation) -> RpcId {
        let id = reservation.id().unwrap().clone();
        let frame = reservation.encode().unwrap();
        let ticket = self
            .connection
            .enqueue(reservation, frame, Arc::new(Permit), deadline())
            .unwrap();
        assert_eq!(
            timeout(Duration::from_secs(5), ticket.wait())
                .await
                .unwrap(),
            WriteReport::Written
        );
        let mut line = String::new();
        timeout(
            Duration::from_secs(5),
            BufReader::new(&mut self.stream).read_line(&mut line),
        )
        .await
        .unwrap()
        .unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["id"], rpc_json(&id));
        id
    }
    async fn replay(&mut self, native: &str) -> RpcId {
        let strategy = self.connection.strategy().unwrap();
        let native = SessionId::parse(
            RawDocument::parse(serde_json::to_vec(native).unwrap())
                .unwrap()
                .root_owned()
                .as_ref(),
        )
        .unwrap();
        let setup = SessionSetup::try_from_snapshot(SetupSnapshot {
            wire: Version::V2,
            source: ConfigSourceStamp {
                reference: Arc::from("independent"),
                revision: 1,
            },
            cwd: Known::Known(Arc::from("C:/independent")),
            mcp_servers: Known::Known(vec![]),
            additional_directories: Known::Known(vec![]),
            extra: ExtensionFields::empty().unwrap(),
        })
        .unwrap();
        let request = strategy.replay(native, &setup).unwrap();
        let reservation = self
            .connection
            .reserve_request(request, ResponseDeadline::Unbounded)
            .unwrap();
        self.write(reservation).await
    }
    async fn feed(&mut self, frames: &[Value]) {
        let mut bytes = Vec::new();
        for frame in frames {
            serde_json::to_writer(&mut bytes, frame).unwrap();
            bytes.push(b'\n');
        }
        timeout(Duration::from_secs(5), self.stream.write_all(&bytes))
            .await
            .unwrap()
            .unwrap();
    }
    async fn next(&mut self) -> ObservationLease {
        timeout(Duration::from_secs(5), self.receiver.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    }
    async fn apply(
        &mut self,
        original: &Value,
        activity: &mut CapacityActivity,
        now: Instant,
        origin: UpdateOrigin,
        freshness: CapacityFreshness,
        rpc: Option<&RpcId>,
    ) -> CapacityObservation {
        let lease = self.next().await;
        let received = lease.received();
        assert_eq!(
            received.raw().bytes(),
            serde_json::to_vec(original).unwrap(),
            "raw bytes preserved"
        );
        assert!(
            matches!(received.incoming(), Incoming::Update { origin: actual, .. } if *actual == origin)
        );
        if let Some(rpc) = rpc {
            assert_eq!(
                received.view_read_context(),
                ViewReadContextKind::UniqueReceiptWindow
            );
            assert_eq!(received.view_read_window().unwrap().rpc(), rpc);
        } else {
            assert_eq!(
                received.view_read_context(),
                ViewReadContextKind::OutsideReadWindow
            );
            assert!(received.view_read_window().is_none());
        }
        let projected = AcpNativeAdapter::default()
            .capacity_observation(&lease, ProtocolVersion::V2)
            .unwrap()
            .unwrap();
        assert_eq!(projected.freshness, freshness);
        assert_eq!(
            projected.origin,
            match origin {
                UpdateOrigin::Live => ObservationOrigin::Live,
                UpdateOrigin::DuringReplay => ObservationOrigin::DuringReplay,
                UpdateOrigin::DuringResume => ObservationOrigin::DuringResume,
            }
        );
        activity.observe(&projected, now).unwrap();
        self.ack.acknowledge(lease).unwrap();
        projected
    }
    async fn stop(self) {
        assert!(
            self.owner.shutdown(deadline()).await.joined,
            "original IO joined"
        );
    }
}
fn rpc_json(id: &RpcId) -> Value {
    let mut bytes = Vec::new();
    id.write_json(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn deadline() -> tokio::time::Instant {
    tokio::time::Instant::now() + Duration::from_secs(5)
}
fn facts() -> RecycleEligibility {
    RecycleEligibility {
        foreground: Coverage::Unknown,
        background: Coverage::Unknown,
        children: Coverage::Unknown,
        pending_approvals: Some(0),
        root_verified: true,
        management_reconciled: true,
        close_supported: true,
    }
}
fn update(session: &str, patch: Value) -> Value {
    let mut params = serde_json::Map::new();
    params.insert("sessionId".into(), Value::String(session.into()));
    params.insert("update".into(), patch);
    json!({"jsonrpc":"2.0","method":"session/update","params":params})
}
fn state(session: &str, foreground: &str, background: Value) -> Value {
    let mut patch = json!({"sessionUpdate":"state_update","state":foreground});
    patch["background"] = background;
    update(session, patch)
}
fn observe(activity: &mut CapacityActivity, session: &str, change: CapacityUpdate, now: Instant) {
    activity
        .observe(
            &CapacityObservation {
                session: Arc::from(session),
                origin: ObservationOrigin::Live,
                freshness: CapacityFreshness::Current,
                update: change,
            },
            now,
        )
        .unwrap();
}
fn idle(activity: &mut CapacityActivity, session: &str, now: Instant) {
    observe(
        activity,
        session,
        CapacityUpdate::State(CapacityState {
            foreground: Coverage::Idle,
            background: Some(Coverage::Idle),
        }),
        now,
    );
}

/// OT-001: three independently protected facts survive actual history; only Current ends each.
#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keeps the complete foreground/background/tool history and current-state assertions together"
)]
async fn ot001_real_reader_history_cannot_clear_foreground_background_or_tool() {
    for protection in ["foreground", "background", "tool"] {
        let mut peer = Peer::start().await;
        let start = Instant::now();
        let mut activity = CapacityActivity::default();
        activity.verify_root("r").unwrap();
        activity.created("r", ProtocolVersion::V2, start);
        let live = match protection {
            "foreground" => state("r", "running", json!([])),
            "background" => state("r", "idle", json!([{"type":"terminal","terminalId":"bg"}])),
            _ => update(
                "r",
                json!({"sessionUpdate":"tool_call_update","toolCallId":"t","status":"in_progress"}),
            ),
        };
        peer.feed(std::slice::from_ref(&live)).await;
        peer.apply(
            &live,
            &mut activity,
            start,
            UpdateOrigin::Live,
            CapacityFreshness::Current,
            None,
        )
        .await;
        assert_eq!(activity.evaluate_tree("r", start, facts()).unwrap(), None);
        let rpc = peer.replay("r").await;
        let end = if protection == "tool" {
            update(
                "r",
                json!({"sessionUpdate":"tool_call_update","toolCallId":"t","status":"completed"}),
            )
        } else {
            state("r", "idle", json!([]))
        };
        let marker = update("r", json!({"sessionUpdate":"replay_complete"}));
        let frames = [end.clone(), marker.clone(), end.clone()];
        peer.feed(&frames).await;
        peer.apply(
            &frames[0],
            &mut activity,
            start + Duration::from_secs(1),
            UpdateOrigin::DuringReplay,
            CapacityFreshness::History,
            Some(&rpc),
        )
        .await;
        assert_eq!(
            activity
                .evaluate_tree("r", start + Duration::from_secs(1), facts())
                .unwrap(),
            None,
            "{protection} must remain active after history idle/completed"
        );
        peer.apply(
            &frames[1],
            &mut activity,
            start + Duration::from_secs(2),
            UpdateOrigin::DuringReplay,
            CapacityFreshness::History,
            Some(&rpc),
        )
        .await;
        assert_eq!(
            activity
                .evaluate_tree("r", start + Duration::from_secs(2), facts())
                .unwrap(),
            None
        );
        peer.apply(
            &frames[2],
            &mut activity,
            start + Duration::from_secs(3),
            UpdateOrigin::DuringReplay,
            CapacityFreshness::Current,
            Some(&rpc),
        )
        .await;
        assert_eq!(
            activity
                .evaluate_tree("r", start + Duration::from_secs(3), facts())
                .unwrap(),
            Some(start + Duration::from_secs(3)),
            "{protection} explicit current ending is effective"
        );
        // A proper current snapshot completes the real read; it does not change original DuringReplay.
        let snapshot = state("r", "idle", json!([]));
        peer.feed(&[
            snapshot.clone(),
            json!({"jsonrpc":"2.0","id":rpc_json(&rpc),"result":{}}),
        ])
        .await;
        peer.apply(
            &snapshot,
            &mut activity,
            start + Duration::from_secs(4),
            UpdateOrigin::DuringReplay,
            CapacityFreshness::Current,
            Some(&rpc),
        )
        .await;
        let response = peer.next().await;
        assert!(matches!(
            response.received().incoming(),
            Incoming::Response { .. }
        ));
        peer.ack.acknowledge(response).unwrap();
        peer.stop().await;
    }
}

/// OT-002: current child state on a real replay stream, not a synthetic origin enum alone.
#[tokio::test]
async fn ot002_real_reader_child_history_preserves_busy_until_current_child_snapshot() {
    let mut peer = Peer::start().await;
    let now = Instant::now();
    let mut activity = CapacityActivity::default();
    activity.verify_root("r").unwrap();
    activity.associate("r", "c").unwrap();
    activity.created("r", ProtocolVersion::V2, now);
    let child = |s: &str| {
        update(
            "r",
            json!({"sessionUpdate":"subagent_update","sessionId":"c",
        "state":{"state":s,"background":[]}}),
        )
    };
    let live = child("running");
    peer.feed(std::slice::from_ref(&live)).await;
    peer.apply(
        &live,
        &mut activity,
        now,
        UpdateOrigin::Live,
        CapacityFreshness::Current,
        None,
    )
    .await;
    assert_eq!(activity.evaluate_tree("r", now, facts()).unwrap(), None);
    let rpc = peer.replay("r").await;
    let frames = [
        child("idle"),
        update("r", json!({"sessionUpdate":"replay_complete"})),
        child("idle"),
        state("r", "idle", json!([])),
    ];
    peer.feed(&frames).await;
    for (i, frame) in frames.iter().enumerate() {
        let at = now + Duration::from_secs(1 + i as u64);
        peer.apply(
            frame,
            &mut activity,
            at,
            UpdateOrigin::DuringReplay,
            if i < 2 {
                CapacityFreshness::History
            } else {
                CapacityFreshness::Current
            },
            Some(&rpc),
        )
        .await;
        assert_eq!(
            activity.evaluate_tree("r", at, facts()).unwrap(),
            if i < 2 { None } else { Some(at) }
        );
    }
    peer.feed(&[json!({"jsonrpc":"2.0","id":rpc_json(&rpc),"result":{}})])
        .await;
    let response = peer.next().await;
    peer.ack.acknowledge(response).unwrap();
    peer.stop().await;
}

/// OT-003: multi-level messages and approval lifecycle touches refresh the original root clock.
#[tokio::test]
async fn ot003_descendant_activity_and_approval_touches_reach_all_ancestors() {
    let mut peer = Peer::start().await;
    let now = Instant::now();
    let mut activity = CapacityActivity::default();
    activity.verify_root("r").unwrap();
    activity.associate("r", "c").unwrap();
    activity.associate("c", "g").unwrap();
    for session in ["r", "c", "g"] {
        idle(&mut activity, session, now);
    }
    assert_eq!(
        activity.evaluate_tree("r", now, facts()).unwrap(),
        Some(now)
    );
    let later = now + Duration::from_secs(11);
    let message = update(
        "g",
        json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"independent descendant"}}),
    );
    peer.feed(std::slice::from_ref(&message)).await;
    let received = peer
        .apply(
            &message,
            &mut activity,
            later,
            UpdateOrigin::Live,
            CapacityFreshness::Current,
            None,
        )
        .await;
    assert!(matches!(received.update, CapacityUpdate::Activity));
    assert_eq!(
        activity.evaluate_tree("r", later, facts()).unwrap(),
        Some(later)
    );
    let replay_received = now + Duration::from_secs(13);
    activity
        .observe(
            &CapacityObservation {
                session: Arc::from("g"),
                origin: ObservationOrigin::DuringReplay,
                freshness: CapacityFreshness::History,
                update: CapacityUpdate::Activity,
            },
            replay_received,
        )
        .unwrap();
    assert_eq!(
        activity
            .evaluate_tree("r", replay_received, facts())
            .unwrap(),
        Some(replay_received)
    );
    let approval_end = now + Duration::from_secs(19);
    activity.touch("g", approval_end).unwrap();
    assert_eq!(
        activity.evaluate_tree("r", approval_end, facts()).unwrap(),
        Some(approval_end)
    );
    let mut pending = facts();
    pending.pending_approvals = Some(1);
    assert_eq!(
        activity.evaluate_tree("r", approval_end, pending).unwrap(),
        None
    );
    let final_end = now + Duration::from_secs(29);
    activity.touch("g", final_end).unwrap();
    assert_eq!(
        activity.evaluate_tree("r", final_end, facts()).unwrap(),
        Some(final_end)
    );
    assert_eq!(activity.root_for("g").as_deref(), Some("r"));
    peer.stop().await;
}

/// OT-004: current state beats delayed new response; omitted/null-like background keeps work.
#[test]
fn ot004_delayed_created_and_missing_background_never_clear_known_activity() {
    let now = Instant::now();
    let mut activity = CapacityActivity::default();
    activity.verify_root("r").unwrap();
    observe(
        &mut activity,
        "r",
        CapacityUpdate::State(CapacityState {
            foreground: Coverage::Active,
            background: Some(Coverage::Active),
        }),
        now,
    );
    activity.created("r", ProtocolVersion::V2, now);
    assert_eq!(activity.evaluate_tree("r", now, facts()).unwrap(), None);
    for background in [None, Some(Coverage::Unknown)] {
        observe(
            &mut activity,
            "r",
            CapacityUpdate::State(CapacityState {
                foreground: Coverage::Idle,
                background,
            }),
            now,
        );
        assert_eq!(activity.evaluate_tree("r", now, facts()).unwrap(), None);
    }
    idle(&mut activity, "r", now);
    assert_eq!(
        activity.evaluate_tree("r", now, facts()).unwrap(),
        Some(now)
    );
    let mut unconfirmed = facts();
    unconfirmed.management_reconciled = false;
    assert_eq!(activity.evaluate_tree("r", now, unconfirmed).unwrap(), None);
}

/// OT-005: persisted graph is identity, neither idle nor an old busy-state replay.
#[test]
fn ot005_restored_identity_without_current_child_state_is_protected() {
    let now = Instant::now();
    let mut old = CapacityActivity::default();
    old.verify_root("r").unwrap();
    old.associate("r", "c").unwrap();
    old.associate("c", "g").unwrap();
    for s in ["r", "c", "g"] {
        idle(&mut old, s, now);
    }
    assert!(old.evaluate_tree("r", now, facts()).unwrap().is_some());
    old.forget_closed("r");
    assert_eq!(old.members("r").unwrap().len(), 3);
    assert_eq!(old.evaluate_tree("r", now, facts()).unwrap(), None);
    old.created("r", ProtocolVersion::V2, now);
    idle(&mut old, "c", now);
    assert_eq!(
        old.evaluate_tree("r", now, facts()).unwrap(),
        None,
        "grandchild has no current state"
    );
    idle(&mut old, "g", now);
    assert_eq!(old.evaluate_tree("r", now, facts()).unwrap(), Some(now));
    let mut restarted = CapacityActivity::default();
    restarted.verify_root("r").unwrap();
    restarted.associate("r", "c").unwrap();
    idle(&mut restarted, "r", now);
    assert_eq!(restarted.evaluate_tree("r", now, facts()).unwrap(), None);
}

/// OT-006: uncertainty blocks the foreground without erasing an earlier active tool.
#[test]
fn ot006_unconfirmed_snapshot_needs_current_foreground_and_tool_end() {
    let now = Instant::now();
    let mut activity = CapacityActivity::default();
    activity.verify_root("r").unwrap();
    idle(&mut activity, "r", now);
    observe(
        &mut activity,
        "r",
        CapacityUpdate::Tool {
            id: Arc::from("t"),
            active: Some(true),
        },
        now,
    );
    activity
        .observe(
            &CapacityObservation {
                session: Arc::from("r"),
                origin: ObservationOrigin::DuringReplay,
                freshness: CapacityFreshness::Unconfirmed,
                update: CapacityUpdate::Tool {
                    id: Arc::from("t"),
                    active: Some(false),
                },
            },
            now,
        )
        .unwrap();
    observe(
        &mut activity,
        "r",
        CapacityUpdate::Tool {
            id: Arc::from("t"),
            active: Some(false),
        },
        now,
    );
    assert_eq!(
        activity.evaluate_tree("r", now, facts()).unwrap(),
        None,
        "tool end cannot make unknown foreground idle"
    );
    idle(&mut activity, "r", now);
    assert_eq!(
        activity.evaluate_tree("r", now, facts()).unwrap(),
        Some(now)
    );
}
