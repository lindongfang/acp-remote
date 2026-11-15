use super::super::test_support::*;
use super::super::{RawMessage, Response, view_observations::parse_header};
use super::*;
use crate::domain::{
    EntityId,
    native::{HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch},
    views::{ViewLimits, ViewState},
};
use serde_json::{Value, json};
use std::{mem::size_of, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
    time::{Instant, timeout},
};

fn target() -> NativeSessionKey {
    NativeSessionKey::from_config(Some(EntityId::from_bytes([1; 16])), "s").unwrap()
}
fn epoch(value: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([1; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([2; 16])),
        load: LoadEpoch(EntityId::from_bytes([value; 16])),
    }
}
fn strategy(wire: Version) -> Strategy {
    unit_strategy(wire)
}
fn setup(
    wire: Version,
    bytes: usize,
    items: usize,
) -> (Strategy, ViewWindows, ViewHandle, ToolProjectionRead) {
    let selected = strategy(wire);
    let mut windows = ViewWindows::new(ViewLimits {
        target_bytes: bytes,
        target_items: items,
        ..ViewLimits::default()
    })
    .unwrap();
    let handle = windows.begin(target(), epoch(1)).unwrap();
    let read = ToolProjectionRead::begin(&mut windows, &handle, &selected).unwrap();
    (selected, windows, handle, read)
}
fn incoming(selected: &Strategy, update: Value, origin: UpdateOrigin) -> Incoming {
    let mut params = json!({"sessionId":"s"});
    params["update"] = update;
    let raw = RawMessage::parse(
        serde_json::to_vec(&json!({
            "jsonrpc":"2.0","method":"session/update","params":params
        }))
        .unwrap(),
    )
    .unwrap();
    let event = selected
        .update(raw.field("params").expect("unique decoded field").unwrap())
        .unwrap();
    Incoming::Update { event, origin, raw }
}
fn update(id: &str, status: &str) -> Value {
    json!({"sessionUpdate":"tool_call_update","toolCallId":id,"status":status})
}
fn run(
    read: &mut ToolProjectionRead,
    windows: &mut ViewWindows,
    selected: &Strategy,
    value: Value,
) -> ProjectionStep {
    read.apply_inner(windows, &incoming(selected, value, UpdateOrigin::Live))
        .unwrap()
}
fn key(id: &str) -> SourceKey<'_> {
    SourceKey {
        namespace: KeyNamespace::Tool,
        id,
    }
}
fn stored(windows: &mut ViewWindows, handle: &ViewHandle, id: &str) -> (u64, [u8; 512]) {
    let lease = windows.item_lease(handle, key(id)).unwrap();
    (
        lease.item().position,
        lease.item().bytes.try_into().unwrap(),
    )
}
fn decode(bytes: &[u8], position: u64, wire: Version) -> Result<AnchorView<'_>, ProjectionError> {
    decode_anchor(
        ViewItem {
            position,
            key: Some(key("t")),
            bytes,
        },
        wire,
    )
}
fn code(bytes: &[u8], slot: usize) -> u8 {
    bytes[32 + slot]
}

#[test]
fn layout_offsets_enumerations_and_wrapper_charge_are_exact() {
    eprintln!(
        "P2A fixed layout: wrapper={} raw={} extra={} patch={} anchor_view={} workspace={} staged={}",
        size_of::<ToolProjectionRead>(),
        size_of::<ObservationRead>(),
        size_of::<ToolProjectionRead>() - size_of::<ObservationRead>(),
        size_of::<ToolPatch>(),
        size_of::<AnchorView<'_>>(),
        size_of::<StageWorkspace>(),
        size_of::<crate::domain::views::StagedItem>()
    );
    assert!(size_of::<ToolProjectionRead>() - size_of::<ObservationRead>() <= 512);
    for wire in [Version::V1, Version::V2] {
        let (s, mut w, h, mut r) = setup(wire, 65536, 128);
        assert_eq!(
            w.target_usage(&target()).unwrap().bytes,
            1025 + 1024 + 1536 + 1024
        );
        let statuses = if wire == Version::V1 {
            vec!["pending", "in_progress", "completed", "failed"]
        } else {
            vec!["pending", "in_progress", "completed", "failed", "cancelled"]
        };
        for (index, status) in statuses.iter().enumerate() {
            let step = run(&mut r, &mut w, &s, update("t", status));
            assert!(matches!(
                step.anchor,
                AnchorOutcome::CreatedPartial { .. } | AnchorOutcome::Updated { .. }
            ));
            let (position, bytes) = stored(&mut w, &h, "t");
            assert_eq!(
                &bytes[..8],
                &[
                    b'A',
                    b'C',
                    b'P',
                    b'A',
                    1,
                    u8::try_from(wire.number()).unwrap(),
                    4,
                    1
                ]
            );
            assert_eq!(bytes[25], u8::try_from(index).unwrap());
            assert_eq!(bytes[26], 255);
            assert_eq!(read_u64(&bytes, 8), 1);
            assert_eq!(read_u64(&bytes, 16), u64::try_from(index).unwrap() + 1);
            assert_eq!(code(&bytes, 5), 1);
            assert_eq!(bytes[181], 1);
            assert!(bytes[27..32].iter().all(|v| *v == 0));
            assert!(bytes[320..].iter().all(|v| *v == 0));
            let anchor = decode(&bytes, position, wire).unwrap();
            assert_eq!(anchor.position(), 2);
            assert_eq!(anchor.tool_kind(), ToolKindFact::Unknown);
        }
        for (index, kind) in [
            "other",
            "read",
            "edit",
            "delete",
            "move",
            "search",
            "execute",
            "think",
            "fetch",
            "switch_mode",
        ]
        .iter()
        .enumerate()
        {
            run(
                &mut r,
                &mut w,
                &s,
                json!({"sessionUpdate":"tool_call_update","toolCallId":"t","kind":kind}),
            );
            let (pos, bytes) = stored(&mut w, &h, "t");
            assert_eq!(bytes[26], u8::try_from(index).unwrap());
            assert_eq!(
                decode(&bytes, pos, wire)
                    .unwrap()
                    .field(AnchorField::Kind)
                    .knowledge,
                Knowledge::Known
            );
        }
    }
}

#[test]
fn strict_decoder_rejects_corruption_and_wire_specific_control_combinations() {
    let (s, mut w, h, mut r) = setup(Version::V2, 65536, 128);
    run(&mut r, &mut w, &s, update("t", "pending"));
    let (position, valid) = stored(&mut w, &h, "t");
    for (offset, value) in [
        (0, 0),
        (4, 2),
        (5, 1),
        (6, 3),
        (7, 9),
        (24, 0),
        (25, 10),
        (27, 1),
        (319, 1),
        (320, 1),
        (32, 8),
        (181, 5),
        (41, 1),
    ] {
        let mut corrupt = valid;
        corrupt[offset] = value;
        assert!(
            matches!(
                decode(&corrupt, position, Version::V2),
                Err(ProjectionError::InvalidAnchor)
            ),
            "offset {offset}"
        );
    }
    for length in [0, 511, 513] {
        let corrupt = vec![0; length];
        assert!(matches!(
            decode(&corrupt, position, Version::V2),
            Err(ProjectionError::InvalidAnchor)
        ));
    }
    for knowledge in [2, 3, 4, 7] {
        let mut corrupt = valid;
        corrupt[37] = knowledge;
        assert!(decode(&corrupt, position, Version::V2).is_err());
    }
    for offset in [8, 16, 88, 232] {
        let mut corrupt = valid;
        write_u64(&mut corrupt, offset, if offset == 8 { 0 } else { 99 });
        if offset == 16 {
            write_u64(&mut corrupt, offset, 0);
        }
        assert!(
            decode(&corrupt, position, Version::V2).is_err(),
            "range {offset}"
        );
    }
    let mut v1 = valid;
    v1[5] = 1;
    for status in [4, 255] {
        v1[25] = status;
        assert!(decode(&v1, position, Version::V1).is_err());
    }
    assert!(
        decode_anchor(
            ViewItem {
                position,
                key: None,
                bytes: &valid
            },
            Version::V2
        )
        .is_err()
    );
    assert!(
        decode_anchor(
            ViewItem {
                position,
                key: Some(SourceKey {
                    namespace: KeyNamespace::AgentMessage,
                    id: "t"
                }),
                bytes: &valid
            },
            Version::V2
        )
        .is_err()
    );
}

#[test]
fn minimal_shapes_validate_whole_frame_before_any_assignment() {
    for wire in [Version::V1, Version::V2] {
        let (s, mut w, h, mut r) = setup(wire, 65536, 256);
        run(&mut r, &mut w, &s, update("t", "pending"));
        let original = stored(&mut w, &h, "t");
        for (field, value) in [
            ("title", json!(false)),
            ("name", json!([])),
            ("status", json!(4)),
            ("kind", json!({})),
            ("content", json!({})),
            ("locations", json!("path")),
            ("_meta", json!([])),
        ] {
            let mut invalid = update("t", "completed");
            invalid[field] = value;
            let step = run(&mut r, &mut w, &s, invalid);
            assert!(matches!(step.raw, ObservationOutcome::Recorded { .. }));
            assert_eq!(
                step.anchor,
                AnchorOutcome::Failed(ProjectionError::MalformedUpdate)
            );
            assert_eq!(stored(&mut w, &h, "t"), original);
        }
        if wire == Version::V1 {
            for value in ["cancelled", "future_status"] {
                let step = run(&mut r, &mut w, &s, update("t", value));
                assert_eq!(
                    step.anchor,
                    AnchorOutcome::Failed(ProjectionError::MalformedUpdate)
                );
            }
            let no_title = json!({"sessionUpdate":"tool_call","toolCallId":"new"});
            assert_eq!(
                run(&mut r, &mut w, &s, no_title).anchor,
                AnchorOutcome::Failed(ProjectionError::MalformedUpdate)
            );
            assert!(matches!(
                w.item_lease(&h, key("new")),
                Err(ViewError::MissingItem)
            ));
            assert_eq!(
                run(
                    &mut r,
                    &mut w,
                    &s,
                    json!({"sessionUpdate":"tool_call_update","toolCallId":"t","kind":"future"})
                )
                .anchor,
                AnchorOutcome::Failed(ProjectionError::MalformedUpdate)
            );
        } else {
            assert_eq!(
                run(
                    &mut r,
                    &mut w,
                    &s,
                    json!({"sessionUpdate":"tool_call","toolCallId":"x","title":"v1"})
                )
                .anchor,
                AnchorOutcome::Failed(ProjectionError::UnsupportedAnchorKind)
            );
            assert_eq!(run(&mut r,&mut w,&s,json!({"sessionUpdate":"tool_call_content_chunk","toolCallId":"t","content":{"type":false}})).anchor, AnchorOutcome::Failed(ProjectionError::MalformedUpdate));
        }
        assert_eq!(stored(&mut w, &h, "t"), original);
        assert_eq!(
            w.window_status(&h).unwrap().state,
            ViewState::Incomplete(ViewFailure::Structure)
        );
    }
}

#[test]
fn v2_exact_clear_deferred_chunk_and_unknown_fallback_transitions() {
    let (s, mut w, h, mut r) = setup(Version::V2, 65536, 256);
    run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call_update","toolCallId":"t",
        "status":"future_status","kind":"future_kind","name":"name","title":"title",
        "content":[],"locations":[],"_meta":{},"rawInput":false,"rawOutput":0}),
    );
    let (pos, bytes) = stored(&mut w, &h, "t");
    assert_eq!(&bytes[32..41], &[7, 7, 1, 1, 1, 1, 7, 7, 7]);
    let anchor = decode(&bytes, pos, Version::V2).unwrap();
    assert_eq!(
        anchor.tool_status(),
        ToolStatusFact::UnrecognizedInProgress {
            sequence: 1,
            raw: RawEvidence::Unverified
        }
    );
    assert_eq!(
        anchor.tool_kind(),
        ToolKindFact::UnrecognizedOther {
            sequence: 1,
            raw: RawEvidence::Unverified
        }
    );
    run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call_update","toolCallId":"t",
        "status":null,"kind":null,"name":null,"title":null,"content":null,
        "locations":null,"_meta":null,"rawInput":null,"rawOutput":null}),
    );
    let (pos, bytes) = stored(&mut w, &h, "t");
    assert_eq!(&bytes[32..41], &[3, 3, 2, 2, 1, 1, 3, 2, 2]);
    assert_eq!(
        decode(&bytes, pos, Version::V2).unwrap().tool_status(),
        ToolStatusFact::Pending
    );
    assert_eq!(
        decode(&bytes, pos, Version::V2).unwrap().tool_kind(),
        ToolKindFact::Other
    );
    run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call_content_chunk","toolCallId":"t",
        "content":{"type":"content","content":{"type":"future","opaque":[1,true]}},
        "_meta":{"must_not_patch_entity":true},"status":"completed","name":"not_entity_patch"}),
    );
    let (pos, bytes) = stored(&mut w, &h, "t");
    assert_eq!(code(&bytes, 0), 7);
    assert_eq!(read_u64(&bytes, 48), 3);
    assert_eq!(read_u64(&bytes, 56), 2);
    assert_eq!(code(&bytes, 1), 3);
    assert_eq!(
        decode(&bytes, pos, Version::V2).unwrap().tool_status(),
        ToolStatusFact::Pending
    );
}

#[test]
fn v1_initial_and_update_null_are_distinct_and_uninterpreted_can_recover() {
    let (s, mut w, h, mut r) = setup(Version::V1, 65536, 256);
    run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call","toolCallId":"t","title":"initial",
        "_meta":null,"rawInput":null,"rawOutput":null}),
    );
    let (_, bytes) = stored(&mut w, &h, "t");
    assert_eq!(&bytes[32..41], &[0, 5, 2, 1, 0, 0, 0, 7, 7]);
    run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call_update","toolCallId":"t",
        "name":null,"title":null,"status":null,"kind":null,"content":null,
        "locations":null,"_meta":null,"rawInput":null,"rawOutput":null}),
    );
    let (pos, bytes) = stored(&mut w, &h, "t");
    assert_eq!(&bytes[32..41], &[5, 5, 2, 5, 5, 5, 5, 7, 7]);
    assert_eq!(read_u64(&bytes, 64), 1); // name no-op
    assert_eq!(read_u64(&bytes, 104), 1); // rawInput no-op
    assert_eq!(
        decode(&bytes, pos, Version::V1).unwrap().tool_status(),
        ToolStatusFact::Uninterpreted
    );
    assert_eq!(bytes[7], 5);
    run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call_update","toolCallId":"t",
        "status":"completed","kind":"edit","title":"new","content":[],"locations":[],"_meta":{}}),
    );
    let (pos, bytes) = stored(&mut w, &h, "t");
    assert_eq!(bytes[7], 1);
    assert_eq!(
        decode(&bytes, pos, Version::V1).unwrap().tool_status(),
        ToolStatusFact::Completed
    );
    assert_eq!(
        decode(&bytes, pos, Version::V1).unwrap().tool_kind(),
        ToolKindFact::Edit
    );
}

#[test]
fn raw_dropped_preserves_inline_control_and_records_assignment_losses_only() {
    let (s, mut w, h, mut r) = setup(Version::V2, 16000, 128);
    run(&mut r, &mut w, &s, update("t", "pending"));
    let original_position = stored(&mut w, &h, "t").0;
    let step = run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call_update","toolCallId":"t",
        "status":"completed","kind":"future_kind","name":"long","content":[],
        "opaque":"x".repeat(32000)}),
    );
    assert!(matches!(
        step.raw,
        ObservationOutcome::Dropped { sequence: 2, .. }
    ));
    assert_eq!(
        step.anchor,
        AnchorOutcome::Updated {
            position: original_position
        }
    );
    let (pos, bytes) = stored(&mut w, &h, "t");
    assert_eq!(
        decode(&bytes, pos, Version::V2).unwrap().tool_status(),
        ToolStatusFact::Completed
    );
    assert_eq!(
        decode(&bytes, pos, Version::V2).unwrap().tool_kind(),
        ToolKindFact::UnrecognizedOther {
            sequence: 2,
            raw: RawEvidence::NotRecorded
        }
    );
    assert_eq!(code(&bytes, 2), 6);
    assert_eq!(read_u64(&bytes, 192 + 2 * 8), 2);
    assert_eq!(read_u64(&bytes, 192 + 3 * 8), 0); // title was not assigned
    run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call_update","toolCallId":"t","name":"small","kind":"read"}),
    );
    let (_, bytes) = stored(&mut w, &h, "t");
    assert_eq!(code(&bytes, 2), 1);
    assert_eq!(read_u64(&bytes, 192 + 2 * 8), 2);
    assert_eq!(bytes[7], 3);
    assert_eq!(r.raw_stats().last_sequence, 3);
    assert_eq!(r.raw_stats().dropped_count, 1);
}

#[test]
fn raw_can_be_recorded_then_evicted_by_its_same_frame_anchor_insert() {
    let (s, mut w, h, mut r) = setup(Version::V2, 7000, 32);
    let step = run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call_update","toolCallId":"t",
        "status":"completed","name":"original-variable-value"}),
    );
    assert!(matches!(
        step.raw,
        ObservationOutcome::Recorded { sequence: 1, .. }
    ));
    assert!(matches!(step.anchor, AnchorOutcome::CreatedPartial { .. }));
    let status = w.window_status(&h).unwrap();
    assert_eq!(status.evicted_count, 1);
    assert_eq!(status.retained_items, 1);
    let (pos, bytes) = stored(&mut w, &h, "t");
    assert_eq!(
        decode(&bytes, pos, Version::V2).unwrap().tool_status(),
        ToolStatusFact::Completed
    );
    assert_eq!(code(&bytes, 2), 1); // observation knowledge, not retained raw proof
    assert_eq!(r.raw_stats().dropped_count, 0);
}

#[test]
fn lookup_capacity_and_replace_peak_failure_never_create_a_new_position() {
    let (s, mut w, h, mut r) = setup(Version::V2, 65536, 4);
    run(&mut r, &mut w, &s, update("t", "pending"));
    let original = stored(&mut w, &h, "t");
    let held = w.item_lease(&h, key("t")).unwrap();
    let step = run(&mut r, &mut w, &s, update("t", "completed"));
    assert_eq!(
        step.anchor,
        AnchorOutcome::Failed(ProjectionError::View(ViewError::Capacity))
    );
    drop(held);
    assert_eq!(stored(&mut w, &h, "t"), original);
    assert_eq!(w.window_status(&h).unwrap().last_position, Some(3));

    let (s, mut w, h, mut r) = setup(Version::V2, 9000, 128);
    run(&mut r, &mut w, &s, update("t", "pending"));
    let original = stored(&mut w, &h, "t");
    let step = run(&mut r, &mut w, &s, update("t", "completed"));
    assert_eq!(
        step.anchor,
        AnchorOutcome::Failed(ProjectionError::View(ViewError::Capacity))
    );
    assert_eq!(stored(&mut w, &h, "t"), original);
}

#[test]
fn real_fifo_anchor_loss_creates_an_explicit_new_partial_anchor() {
    let (s, mut w, h, mut r) = setup(Version::V2, 22000, 128);
    run(&mut r, &mut w, &s, update("t", "pending"));
    let original = stored(&mut w, &h, "t").0;
    run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"future_update","opaque":"x".repeat(16000)}),
    );
    assert!(matches!(
        w.item_lease(&h, key("t")),
        Err(ViewError::MissingItem)
    ));
    let step = run(&mut r, &mut w, &s, update("t", "completed"));
    let AnchorOutcome::CreatedPartial { position } = step.anchor else {
        panic!("actual missing must be partial")
    };
    assert!(position > original);
    let (_, bytes) = stored(&mut w, &h, "t");
    assert_eq!(read_u64(&bytes, 8), 3);
    assert_eq!(read_u64(&bytes, 16), 3);
    assert_eq!(bytes[7] & 1, 1);
}

#[test]
fn corrupt_anchor_and_stale_read_do_not_replace_or_pollute_new_scope() {
    let (s, mut w, h, mut r) = setup(Version::V2, 65536, 128);
    let original = w.insert(&h, Some(key("t")), b"bad anchor").unwrap();
    let step = run(&mut r, &mut w, &s, update("t", "completed"));
    assert_eq!(
        step.anchor,
        AnchorOutcome::Failed(ProjectionError::InvalidAnchor)
    );
    let lease = w.item_lease(&h, key("t")).unwrap();
    assert_eq!(lease.item().position, original);
    assert_eq!(lease.item().bytes, b"bad anchor");
    drop(lease);
    w.release(&h).unwrap();
    let next = w.begin(target(), epoch(2)).unwrap();
    assert!(matches!(
        r.apply_inner(
            &mut w,
            &incoming(&s, update("t", "completed"), UpdateOrigin::Live)
        ),
        Err(ProjectionError::Observation(ObservationError::View(
            ViewError::Stale
        )))
    ));
    assert_eq!(w.window_status(&next).unwrap().retained_items, 0);
    assert_eq!(w.window_status(&next).unwrap().state, ViewState::Reading);
}

type Client = TestClient;
type Peer = (BufReader<ReadHalf<DuplexStream>>, WriteHalf<DuplexStream>);
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(3)
}
async fn feed(peer: &mut Peer, value: Value) {
    let mut bytes = serde_json::to_vec(&value).unwrap();
    bytes.push(b'\n');
    timeout(Duration::from_secs(3), peer.1.write_all(&bytes))
        .await
        .unwrap()
        .unwrap();
}
async fn sent(peer: &mut Peer) -> Value {
    let mut line = String::new();
    timeout(Duration::from_secs(3), peer.0.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(&line).unwrap()
}
fn init_result(wire: Version) -> Value {
    match wire {
        Version::V1 => json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true}}),
        Version::V2 => {
            json!({"protocolVersion":2,"info":{"name":"fixture","version":"fixed"},"capabilities":{"session":{}}})
        }
    }
}
async fn ready(wire: Version) -> (Client, Peer) {
    let (a, b) = tokio::io::duplex(128 * 1024);
    let (ar, aw) = tokio::io::split(a);
    let (br, bw) = tokio::io::split(b);
    let mut client = TestClient::new(ar, aw);
    let mut peer = (BufReader::new(br), bw);
    let id = client.initialize(Version::V2, deadline()).await.unwrap();
    sent(&mut peer).await;
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":id.json(),"result":init_result(wire)}),
    )
    .await;
    assert!(matches!(
        client.next(deadline()).await.unwrap(),
        Incoming::Response {
            response: Response::Initialized(_),
            ..
        }
    ));
    (client, peer)
}
async fn wire_update(client: &mut Client, peer: &mut Peer, value: Value) -> Incoming {
    feed(peer,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":value}})).await;
    client.next(deadline()).await.unwrap()
}

#[tokio::test]
async fn real_connection_257_tools_keep_first_position_after_long_observation() {
    for wire in [Version::V1, Version::V2] {
        let (mut client, mut peer) = ready(wire).await;
        let mut w = ViewWindows::new(ViewLimits::default()).unwrap();
        let h = w.begin(target(), epoch(1)).unwrap();
        let mut r = ToolProjectionRead::begin(&mut w, &h, client.strategy().unwrap()).unwrap();
        let mut first = 0;
        for index in 0..257 {
            let incoming = wire_update(
                &mut client,
                &mut peer,
                update(&format!("t{index}"), "pending"),
            )
            .await;
            let step = r.apply_inner(&mut w, &incoming).unwrap();
            if index == 0 {
                let AnchorOutcome::CreatedPartial { position } = step.anchor else {
                    panic!("first")
                };
                first = position;
            }
        }
        let selected = client.strategy().unwrap();
        let large = incoming(
            selected,
            json!({"sessionUpdate":"tool_call_update","toolCallId":"t0",
            "opaque":"x".repeat(300_000)}),
            UpdateOrigin::Live,
        );
        assert_eq!(
            r.apply_inner(&mut w, &large).unwrap().anchor,
            AnchorOutcome::Updated { position: first }
        );
        let incoming = wire_update(&mut client, &mut peer, update("t0", "completed")).await;
        assert_eq!(
            r.apply_inner(&mut w, &incoming).unwrap().anchor,
            AnchorOutcome::Updated { position: first }
        );
        let (pos, bytes) = stored(&mut w, &h, "t0");
        assert_eq!(
            decode(&bytes, pos, wire).unwrap().tool_status(),
            ToolStatusFact::Completed
        );
        assert_eq!(r.raw_stats().last_sequence, 259);
        let mut line = String::new();
        assert!(
            timeout(Duration::from_millis(10), peer.0.read_line(&mut line))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn real_replay_response_and_live_update_preserve_each_origin_without_reset() {
    let (mut client, mut peer) = ready(Version::V2).await;
    let mut w = ViewWindows::new(ViewLimits::default()).unwrap();
    let h = w.begin(target(), epoch(1)).unwrap();
    let mut r = ToolProjectionRead::begin(&mut w, &h, client.strategy().unwrap()).unwrap();
    let request = client
        .strategy()
        .unwrap()
        .replay(parse_session(&json!("s")).unwrap(), "C:/fixture")
        .unwrap();
    let id = client.request(request, deadline()).await.unwrap();
    sent(&mut peer).await;
    let incoming = wire_update(&mut client, &mut peer, update("t", "pending")).await;
    let first = r.apply_inner(&mut w, &incoming).unwrap();
    let state = wire_update(
        &mut client,
        &mut peer,
        json!({"sessionUpdate":"state_update","state":"idle"}),
    )
    .await;
    assert_eq!(
        r.apply_inner(&mut w, &state).unwrap().raw,
        ObservationOutcome::Ignored
    );
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":id.json(),"result":{}}),
    )
    .await;
    let response = client.next(deadline()).await.unwrap();
    assert_eq!(
        r.apply_inner(&mut w, &response).unwrap().raw,
        ObservationOutcome::Ignored
    );
    let incoming = wire_update(&mut client, &mut peer, update("t", "completed")).await;
    let last = r.apply_inner(&mut w, &incoming).unwrap();
    let AnchorOutcome::CreatedPartial { position } = first.anchor else {
        panic!("first")
    };
    assert_eq!(last.anchor, AnchorOutcome::Updated { position });
    let (pos, bytes) = stored(&mut w, &h, "t");
    assert_eq!(bytes[24], 5);
    assert_eq!(
        decode(&bytes, pos, Version::V2)
            .unwrap()
            .field(AnchorField::Status)
            .origin,
        FieldOrigin::Live
    );
    let snap = w.snapshot(&h).unwrap();
    let origins: Vec<_> = snap
        .items()
        .filter_map(|item| parse_header(item.bytes).ok())
        .map(|raw| (raw.sequence, raw.origin))
        .collect();
    assert_eq!(
        origins,
        vec![(1, UpdateOrigin::DuringReplay), (2, UpdateOrigin::Live)]
    );
}
#[test]
fn exact_unicode_id_boundary_and_metadata_begin_failure_do_not_leak() {
    let selected = strategy(Version::V2);
    let mut failed = ViewWindows::new(ViewLimits {
        target_bytes: 4608,
        ..ViewLimits::default()
    })
    .unwrap();
    let handle = failed.begin(target(), epoch(1)).unwrap();
    let before = failed.target_usage(&target()).unwrap();
    assert!(matches!(
        ToolProjectionRead::begin(&mut failed, &handle, &selected),
        Err(ProjectionError::View(ViewError::Capacity))
    ));
    assert_eq!(failed.target_usage(&target()).unwrap(), before);
    let (s, mut w, h, mut r) = setup(Version::V2, 65536, 128);
    for id in [String::new(), "é".repeat(2048)] {
        let step = run(&mut r, &mut w, &s, update(&id, "completed"));
        assert!(matches!(step.anchor, AnchorOutcome::CreatedPartial { .. }));
        assert_eq!(
            decode(&stored(&mut w, &h, &id).1, 0, Version::V2)
                .unwrap()
                .tool_status(),
            ToolStatusFact::Completed
        );
    }
    let oversized = update(&"x".repeat(4097), "completed");
    let raw = RawMessage::parse(
        serde_json::to_vec(&json!({"params":{"sessionId":"s","update":oversized}})).unwrap(),
    )
    .unwrap();
    assert!(
        s.update(raw.field("params").expect("unique decoded field").unwrap())
            .is_ok()
    );
}

#[test]
fn dropped_v1_noops_preserve_all_four_field_facts() {
    let (s, mut w, h, mut r) = setup(Version::V1, 16000, 128);
    run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call_update","toolCallId":"t",
        "name":"original","rawInput":{"x":1},"rawOutput":false}),
    );
    let (_, before) = stored(&mut w, &h, "t");
    let step = run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call_update","toolCallId":"t",
        "name":null,"rawInput":null,"rawOutput":null,"status":"completed","opaque":"x".repeat(32000)}),
    );
    assert!(matches!(step.raw, ObservationOutcome::Dropped { .. }));
    let (pos, after) = stored(&mut w, &h, "t");
    let old = decode(&before, pos, Version::V1).unwrap();
    let new = decode(&after, pos, Version::V1).unwrap();
    for field in [
        AnchorField::Name,
        AnchorField::RawInput,
        AnchorField::RawOutput,
    ] {
        assert_eq!(new.field(field), old.field(field));
    }
    assert_eq!(new.tool_status(), ToolStatusFact::Completed);
    assert_eq!(new.field(AnchorField::Status).last_raw_loss, 2);
}

#[test]
fn dropped_v2_unknown_status_keeps_required_in_progress_fallback_and_scope_isolation() {
    let (s, mut w, h, mut r) = setup(Version::V2, 16000, 128);
    run(&mut r, &mut w, &s, update("t", "pending"));
    let step = run(
        &mut r,
        &mut w,
        &s,
        json!({"sessionUpdate":"tool_call_update","toolCallId":"t",
        "status":"future","opaque":"x".repeat(32000)}),
    );
    assert!(matches!(step.raw, ObservationOutcome::Dropped { .. }));
    let (pos, bytes) = stored(&mut w, &h, "t");
    assert_eq!(
        decode(&bytes, pos, Version::V2).unwrap().tool_status(),
        ToolStatusFact::UnrecognizedInProgress {
            sequence: 2,
            raw: RawEvidence::NotRecorded
        }
    );
    let mut other = ViewWindows::new(ViewLimits::default()).unwrap();
    let other_handle = other.begin(target(), epoch(1)).unwrap();
    assert!(matches!(
        r.apply_inner(
            &mut other,
            &incoming(&s, update("t", "completed"), UpdateOrigin::Live)
        ),
        Err(ProjectionError::Observation(ObservationError::View(
            ViewError::Stale
        )))
    ));
    assert_eq!(
        other.window_status(&other_handle).unwrap().retained_items,
        0
    );
    assert_eq!(
        other.window_status(&other_handle).unwrap().state,
        ViewState::Reading
    );
}

#[test]
fn metadata_patch_checks_only_interpreted_keys_and_replaces_whole_subtrees() {
    use super::super::{RawMetadata, raw_json::RawDocument};
    let raw = |text: &str| {
        RawDocument::parse(text.as_bytes().to_vec())
            .unwrap()
            .root_owned()
    };
    let selected = strategy(Version::V2);
    let current = RawMetadata::from_raw(raw(
        r#"{"opaque":1,"opaque":2,"replace":{"old":1},"delete":3}"#,
    ))
    .unwrap();
    let patch = raw(r#"{"_meta":{"replace":{"n":1,"\u006e":2},"delete":null,"new":4}}"#);
    let updated = selected
        .tool_metadata_patch(&current, patch.as_ref())
        .unwrap();
    let object = updated.as_object().unwrap();
    assert_eq!(object.len(), 4);
    assert!(matches!(
        object.get("opaque"),
        Err(super::super::AcpError::InvalidMessage)
    ));
    assert_eq!(
        object.get("replace").unwrap().unwrap().fragment(),
        r#"{"n":1,"\u006e":2}"#
    );
    assert!(object.get("delete").unwrap().is_none());
    assert_eq!(object.get("new").unwrap().unwrap().integer(), Some(4));
    for text in [
        r#"{"_meta":{"new":1,"\u006eew":2}}"#,
        r#"{"_meta":{"opaque":null}}"#,
    ] {
        assert!(matches!(
            selected.tool_metadata_patch(&current, raw(text).as_ref()),
            Err(super::super::AcpError::InvalidMessage)
        ));
    }
    let unchanged = selected
        .tool_metadata_patch(&current, raw("{}").as_ref())
        .unwrap();
    assert_eq!(
        unchanged.to_raw().unwrap().as_ref().fragment(),
        current.to_raw().unwrap().as_ref().fragment()
    );
    assert!(
        selected
            .tool_metadata_patch(&current, raw(r#"{"_meta":null}"#).as_ref())
            .unwrap()
            .is_null()
    );
    assert_eq!(current.as_object().unwrap().len(), 4);
}
