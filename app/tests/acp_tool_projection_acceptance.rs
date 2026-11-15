//! P2A A-H: exact finite facts and actual wire entry, not a complete current-value reader.
#![allow(clippy::too_many_lines)] // Each table keeps one complete oracle and its negative variants together.
#[path = "acp_runtime_acceptance/support/legacy.rs"]
mod bridge;
#[path = "acceptance/p2a_support.rs"]
mod support;
use acp_remote::{
    adapters::acp::{
        Incoming, SessionId, UpdateOrigin, Version,
        tool_projection::*,
        view_observations::{
            ObservationError, ObservationFailure, ObservationOutcome, parse_header,
        },
    },
    domain::views::*,
};
use bridge::*;
use serde_json::{Value, json};
use support::*;

const FIELDS: [AnchorField; 9] = [
    AnchorField::Content,
    AnchorField::Metadata,
    AnchorField::Name,
    AnchorField::Title,
    AnchorField::Kind,
    AnchorField::Status,
    AnchorField::Locations,
    AnchorField::RawInput,
    AnchorField::RawOutput,
];
fn decoded<'a>(bytes: &'a [u8], version: Version, id: &'a str) -> AnchorView<'a> {
    decode_anchor(
        ViewItem {
            position: 19,
            key: Some(key(id)),
            bytes,
        },
        version,
    )
    .unwrap()
}
fn created(step: ProjectionStep) -> u64 {
    let AnchorOutcome::CreatedPartial { position } = step.anchor else {
        panic!("expected CreatedPartial: {step:?}")
    };
    position
}
fn recorded(step: ProjectionStep, sequence: u64) {
    assert!(
        matches!(step.raw, ObservationOutcome::Recorded { sequence:q, .. } if q == sequence),
        "{step:?}"
    );
}
fn dropped(step: ProjectionStep, sequence: u64) {
    assert_eq!(
        step.raw,
        ObservationOutcome::Dropped {
            sequence,
            reason: ObservationFailure::Capacity
        }
    );
}
fn initial(id: &str) -> Value {
    json!({"sessionUpdate":"tool_call_update","toolCallId":id,"title":"tool","name":"native-name","status":"pending","kind":"read"})
}
fn large_update(id: &str, padding: usize, status: &str) -> Value {
    json!({"sessionUpdate":"tool_call_update","toolCallId":id,"name":"new-name","status":status,"padding":"x".repeat(padding)})
}
fn fact(
    bytes: &[u8],
    v: Version,
    field: AnchorField,
    k: Knowledge,
    seq: u64,
    origin: FieldOrigin,
    loss: u64,
) {
    assert_eq!(
        decoded(bytes, v, "t").field(field),
        FieldFact {
            knowledge: k,
            sequence: seq,
            origin,
            last_raw_loss: loss
        }
    );
}
fn bare(version: Version) -> Vec<u8> {
    let mut b = vec![0; 512];
    b[..4].copy_from_slice(b"ACPA");
    b[4] = 1;
    b[5] = if version == Version::V1 { 1 } else { 2 };
    b[6] = 4;
    b[7] = 1;
    put(&mut b, 8, 1);
    put(&mut b, 16, 1);
    b[24] = 1;
    b[25] = 255;
    b[26] = 255;
    b
}
fn invalid(bytes: &[u8], version: Version) {
    assert!(matches!(
        decode_anchor(
            ViewItem {
                position: 0,
                key: Some(key("t")),
                bytes
            },
            version
        ),
        Err(ProjectionError::InvalidAnchor)
    ));
}

#[test]
fn a_fixed_offsets_lengths_reserved_and_field_ranges() {
    for version in [Version::V1, Version::V2] {
        let b = bare(version);
        assert_eq!(decoded(&b, version, "t").position(), 19);
        for n in 0..512 {
            invalid(&b[..n], version);
        }
        let mut longer = b.clone();
        longer.push(0);
        invalid(&longer, version);
        for offset in (0..7).chain(27..32).chain(320..512) {
            let mut changed = b.clone();
            changed[offset] = 254;
            invalid(&changed, version);
        }
        for offset in [7, 24] {
            let mut changed = b.clone();
            changed[offset] |= 128;
            invalid(&changed, version);
        }
        for (offset, value) in [(8, 0), (8, 2), (48, 1), (192, 1)] {
            let mut changed = b.clone();
            put(&mut changed, offset, value);
            invalid(&changed, version);
        }
        for (offset, value) in [(32, 8), (176, 4), (41, 1)] {
            let mut changed = b.clone();
            changed[offset] = value;
            invalid(&changed, version);
        }
        assert!(
            decode_anchor(
                ViewItem {
                    position: 1,
                    key: None,
                    bytes: &b
                },
                version
            )
            .is_err()
        );
        assert!(
            decode_anchor(
                ViewItem {
                    position: 1,
                    key: Some(SourceKey {
                        namespace: KeyNamespace::AgentMessage,
                        id: "t"
                    }),
                    bytes: &b
                },
                version
            )
            .is_err()
        );
        assert!(
            decode_anchor(
                ViewItem {
                    position: 1,
                    key: Some(key(&"x".repeat(4097))),
                    bytes: &b
                },
                version
            )
            .is_err()
        );
        assert!(
            decode_anchor(
                ViewItem {
                    position: 1,
                    key: Some(key("")),
                    bytes: &b
                },
                version
            )
            .is_ok()
        );
        let wrong = if version == Version::V1 {
            Version::V2
        } else {
            Version::V1
        };
        invalid(&b, wrong);
        let mut assigned = b.clone();
        assigned[35] = 1;
        put(&mut assigned, 72, 1);
        assigned[179] = 1;
        assert!(
            decode_anchor(
                ViewItem {
                    position: 1,
                    key: Some(key("t")),
                    bytes: &assigned
                },
                version
            )
            .is_ok()
        );
        let mut bad = assigned.clone();
        put(&mut bad, 72, 2);
        invalid(&bad, version);
        let mut bad = assigned.clone();
        put(&mut bad, 216, 2);
        invalid(&bad, version);
        let mut bad = assigned;
        bad[7] |= 2;
        invalid(&bad, version);
    }
}

#[test]
fn a_exhaustive_small_knowledge_code_wire_matrix() {
    let mut cases = 0;
    for version in [Version::V1, Version::V2] {
        for (slot, offset, max) in [
            (4, 26, 9),
            (5, 25, if version == Version::V1 { 3 } else { 4 }),
        ] {
            for knowledge in 0..8 {
                for code in 0..=255u8 {
                    let mut b = bare(version);
                    b[32 + slot] = knowledge;
                    b[offset] = code;
                    if knowledge != 0 {
                        put(&mut b, 48 + slot * 8, 1);
                        b[176 + slot] = 1;
                    }
                    if knowledge == 5 {
                        b[7] |= 4;
                    }
                    if knowledge == 6 {
                        put(&mut b, 192 + slot * 8, 1);
                        b[7] |= 2;
                    }
                    let expected = match knowledge {
                        0 | 5 => code == 255,
                        1 => code <= max || (version == Version::V2 && code == 255),
                        6 => version == Version::V2 && code == 255,
                        _ => false,
                    };
                    let result = decode_anchor(
                        ViewItem {
                            position: 7,
                            key: Some(key("t")),
                            bytes: &b,
                        },
                        version,
                    );
                    assert_eq!(
                        result.is_ok(),
                        expected,
                        "{version:?} slot={slot} knowledge={knowledge} code={code}"
                    );
                    if let Ok(view) = result {
                        assert_eq!(
                            view.field(if slot == 4 {
                                AnchorField::Kind
                            } else {
                                AnchorField::Status
                            })
                            .knowledge as u8,
                            knowledge
                        );
                    }
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 8192);
}

#[tokio::test]
async fn b_shape_atomicity_version_errors_and_protocol_id_boundary() {
    let bads = [
        ("title", json!(3)),
        ("name", json!(false)),
        ("status", json!([])),
        ("kind", json!({})),
        ("content", json!({})),
        ("locations", json!("bad")),
        ("_meta", json!([])),
    ];
    for v in [Version::V1, Version::V2] {
        let mut r = Rig::new(v, 100_000, 1000).await;
        created(r.apply(initial("t")).await);
        for (index, (name, value)) in bads.iter().enumerate() {
            let before = r.anchor("t");
            let mut update = json!({"sessionUpdate":"tool_call_update","toolCallId":"t","status":"completed","name":"would-change"});
            update[*name] = value.clone();
            let step = r.apply(update).await;
            recorded(step, u64::try_from(index + 2).unwrap());
            assert_eq!(
                step.anchor,
                AnchorOutcome::Failed(ProjectionError::MalformedUpdate)
            );
            assert_eq!(r.anchor("t"), before);
        }
        let tag = if v == Version::V1 {
            "tool_call_content_chunk"
        } else {
            "tool_call"
        };
        let step=r.apply(json!({"sessionUpdate":tag,"toolCallId":"other","content":{"type":"text"},"title":"t"})).await;
        assert_eq!(
            step.anchor,
            AnchorOutcome::Failed(ProjectionError::UnsupportedAnchorKind)
        );
        assert!(matches!(
            r.windows.item_lease(&r.h, key("other")),
            Err(ViewError::MissingItem)
        ));
        let step = r
            .apply(json!({"sessionUpdate":"future_display","status":"fake"}))
            .await;
        assert_eq!(step.anchor, AnchorOutcome::Ignored);
        assert_eq!(r.read.raw_stats().last_sequence, 10);
        assert!(matches!(
            r.windows.window_status(&r.h).unwrap().state,
            ViewState::Incomplete(_)
        ));
        r.no_wire().await;
    }
    for id in [json!(null), json!(17), json!("x".repeat(4097))] {
        let (mut c, mut p) = ready(Version::V1).await;
        let expected_failure = !id.is_string();
        let result = next_raw(
            &mut c,
            &mut p,
            &frame(json!({"sessionUpdate":"tool_call_update","toolCallId":id})),
        )
        .await;
        assert_eq!(result.is_err(), expected_failure);
        assert_eq!(c.is_closed(), expected_failure);
    }
}

#[tokio::test]
async fn b_initial_title_chunk_shape_empty_and_utf8_ids() {
    let mut r = Rig::new(Version::V1, 100_000, 1000).await;
    for update in [
        json!({"sessionUpdate":"tool_call","toolCallId":"missing"}),
        json!({"sessionUpdate":"tool_call","toolCallId":"null","title":null}),
        json!({"sessionUpdate":"tool_call","toolCallId":"bad-status","title":"t","status":null}),
        json!({"sessionUpdate":"tool_call","toolCallId":"bad-content","title":"t","content":null}),
    ] {
        assert_eq!(
            r.apply(update).await.anchor,
            AnchorOutcome::Failed(ProjectionError::MalformedUpdate)
        );
    }
    for id in [String::new(), "é".repeat(2048)] {
        created(
            r.apply(json!({"sessionUpdate":"tool_call","toolCallId":id,"title":"valid"}))
                .await,
        );
    }
    let (mut c, mut p) = ready(Version::V1).await;
    assert!(
        next_raw(
            &mut c,
            &mut p,
            &frame(initial(&format!("{}a", "é".repeat(2048))))
        )
        .await
        .is_ok()
    );
    let (mut c, mut p) = ready(Version::V1).await;
    let long = "x".repeat(4097);
    let received = next_raw(&mut c, &mut p, &frame(initial(&long)))
        .await
        .unwrap();
    assert!(
        matches!(received.incoming(),Incoming::Update { event:acp_remote::adapters::acp::SessionEvent::Tool {tool,..},.. } if tool.as_str()==long)
    );
    assert!(!c.is_closed());
    let mut r = Rig::new(Version::V2, 100_000, 1000).await;
    for content in [json!(null), json!([]), json!({}), json!({"type":1})] {
        assert_eq!(r.apply(json!({"sessionUpdate":"tool_call_content_chunk","toolCallId":"t","content":content})).await.anchor,AnchorOutcome::Failed(ProjectionError::MalformedUpdate));
    }
    let pos=created(r.apply(json!({"sessionUpdate":"tool_call_content_chunk","toolCallId":"t","content":{"type":"future","x":null},"_meta":{"ignored":"chunk-only"},"status":"completed"})).await);
    let (_, b) = r.anchor("t");
    let view = decoded(&b, Version::V2, "t");
    assert_eq!(
        view.field(AnchorField::Content).knowledge,
        Knowledge::Deferred
    );
    assert_eq!(
        view.field(AnchorField::Metadata).knowledge,
        Knowledge::Unknown
    );
    assert_eq!(view.tool_status(), ToolStatusFact::Unknown);
    assert_eq!(
        r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","status":"completed"}))
            .await
            .anchor,
        AnchorOutcome::Updated { position: pos }
    );
}

#[tokio::test]
async fn c_v1_v2_finite_null_transitions_noop_and_flags() {
    for v in [Version::V1, Version::V2] {
        let mut r = Rig::new(v, 200_000, 1000).await;
        let tag = if v == Version::V1 {
            "tool_call"
        } else {
            "tool_call_update"
        };
        created(r.apply(json!({"sessionUpdate":tag,"toolCallId":"t","title":"T","name":"N","status":"pending","kind":"read","content":[],"locations":[],"_meta":{},"rawInput":1,"rawOutput":null})).await);
        let (_, old) = r.anchor("t");
        for f in [
            AnchorField::Content,
            AnchorField::Metadata,
            AnchorField::Locations,
            AnchorField::RawInput,
        ] {
            fact(&old, v, f, Knowledge::Deferred, 1, FieldOrigin::Live, 0);
        }
        fact(
            &old,
            v,
            AnchorField::RawOutput,
            if v == Version::V1 {
                Knowledge::Deferred
            } else {
                Knowledge::KnownAbsent
            },
            1,
            FieldOrigin::Live,
            0,
        );
        recorded(r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","name":null,"title":null,"status":null,"kind":null,"content":null,"locations":null,"_meta":null,"rawInput":null,"rawOutput":null})).await,2);
        let (_, after) = r.anchor("t");
        for f in FIELDS {
            let k = if v == Version::V1 {
                match f {
                    AnchorField::Name => Knowledge::Known,
                    AnchorField::RawInput | AnchorField::RawOutput => Knowledge::Deferred,
                    _ => Knowledge::Uninterpreted,
                }
            } else {
                match f {
                    AnchorField::Status | AnchorField::Kind => Knowledge::Known,
                    AnchorField::Content | AnchorField::Locations | AnchorField::Metadata => {
                        Knowledge::KnownEmpty
                    }
                    _ => Knowledge::KnownAbsent,
                }
            };
            let keep = v == Version::V1
                && matches!(
                    f,
                    AnchorField::Name | AnchorField::RawInput | AnchorField::RawOutput
                );
            fact(
                &after,
                v,
                f,
                k,
                if keep { 1 } else { 2 },
                FieldOrigin::Live,
                0,
            );
        }
        let view = decoded(&after, v, "t");
        assert_eq!(
            view.tool_status(),
            if v == Version::V1 {
                ToolStatusFact::Uninterpreted
            } else {
                ToolStatusFact::Pending
            }
        );
        assert_eq!(
            view.tool_kind(),
            if v == Version::V1 {
                ToolKindFact::Uninterpreted
            } else {
                ToolKindFact::Other
            }
        );
        assert_eq!(after[7], if v == Version::V1 { 5 } else { 1 });
        r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t"}))
            .await;
        let (_, noop) = r.anchor("t");
        assert_eq!(&noop[25..], &after[25..]);
        assert_eq!(u64_at(&noop, 16), 3);
        r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","title":"resolved","status":"completed","kind":"other","content":[],"locations":[],"_meta":{}})).await;
        assert_eq!(r.anchor("t").1[7], 1);
    }
    let mut r = Rig::new(Version::V1, 100_000, 1000).await;
    created(
        r.apply(json!({"sessionUpdate":"tool_call","toolCallId":"t","title":"t","_meta":null}))
            .await,
    );
    let (_, b) = r.anchor("t");
    fact(
        &b,
        Version::V1,
        AnchorField::Name,
        Knowledge::KnownAbsent,
        1,
        FieldOrigin::Live,
        0,
    );
    fact(
        &b,
        Version::V1,
        AnchorField::Metadata,
        Knowledge::Uninterpreted,
        1,
        FieldOrigin::Live,
        0,
    );
    assert_eq!(
        decoded(&b, Version::V1, "t").tool_status(),
        ToolStatusFact::Unknown
    );
}

#[tokio::test]
async fn c_dropped_null_clear_uninterpreted_and_noop_full_provenance_table() {
    for v in [Version::V1, Version::V2] {
        let mut r = Rig::new(v, 14_000, 100).await;
        created(r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","title":"old","name":"old","status":"pending","kind":"read","content":[],"locations":[],"_meta":{},"rawInput":1,"rawOutput":2})).await);
        let step=r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","title":null,"name":null,"status":null,"kind":null,"content":null,"locations":null,"_meta":null,"rawInput":null,"rawOutput":null,"padding":"x".repeat(40_000)})).await;
        dropped(step, 2);
        assert!(matches!(step.anchor, AnchorOutcome::Updated { .. }));
        let (_, b) = r.anchor("t");
        for f in FIELDS {
            let keep = v == Version::V1
                && matches!(
                    f,
                    AnchorField::Name | AnchorField::RawInput | AnchorField::RawOutput
                );
            let k = if v == Version::V1 {
                if keep {
                    if f == AnchorField::Name {
                        Knowledge::Known
                    } else {
                        Knowledge::Deferred
                    }
                } else {
                    Knowledge::Uninterpreted
                }
            } else {
                match f {
                    AnchorField::Status | AnchorField::Kind => Knowledge::Known,
                    AnchorField::Content | AnchorField::Locations | AnchorField::Metadata => {
                        Knowledge::KnownEmpty
                    }
                    _ => Knowledge::KnownAbsent,
                }
            };
            fact(
                &b,
                v,
                f,
                k,
                if keep { 1 } else { 2 },
                FieldOrigin::Live,
                if keep { 0 } else { 2 },
            );
        }
        let step=r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","padding":"x".repeat(40_000)})).await;
        dropped(step, 3);
        let (_, noop) = r.anchor("t");
        assert_eq!(&noop[25..], &b[25..]);
        assert_eq!(u64_at(&noop, 16), 3);
        assert_eq!(r.read.raw_stats().dropped_count, 2);
        assert_eq!(b[7], if v == Version::V1 { 7 } else { 3 });
    }
    let mut r = Rig::new(Version::V1, 14_000, 100).await;
    let step=r.apply(json!({"sessionUpdate":"tool_call","toolCallId":"t","title":"T","_meta":null,"rawInput":null,"rawOutput":null,"padding":"x".repeat(40_000)})).await;
    dropped(step, 1);
    created(step);
    let (_, b) = r.anchor("t");
    fact(
        &b,
        Version::V1,
        AnchorField::Name,
        Knowledge::KnownAbsent,
        1,
        FieldOrigin::Live,
        1,
    );
    fact(
        &b,
        Version::V1,
        AnchorField::Metadata,
        Knowledge::Uninterpreted,
        1,
        FieldOrigin::Live,
        1,
    );
    fact(
        &b,
        Version::V1,
        AnchorField::RawInput,
        Knowledge::MissingRaw,
        1,
        FieldOrigin::Live,
        1,
    );
    fact(
        &b,
        Version::V1,
        AnchorField::RawOutput,
        Knowledge::MissingRaw,
        1,
        FieldOrigin::Live,
        1,
    );
}

#[tokio::test]
async fn c_dropped_big_fields_and_chunk_touch_only_defined_slots() {
    let mut r = Rig::new(Version::V2, 14_000, 100).await;
    let p = created(r.apply(initial("t")).await);
    let step=r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","content":[],"locations":[],"_meta":{},"rawInput":{},"rawOutput":[],"padding":"x".repeat(40_000)})).await;
    dropped(step, 2);
    assert_eq!(step.anchor, AnchorOutcome::Updated { position: p });
    let (_, b) = r.anchor("t");
    for f in [
        AnchorField::Content,
        AnchorField::Metadata,
        AnchorField::Locations,
        AnchorField::RawInput,
        AnchorField::RawOutput,
    ] {
        fact(
            &b,
            Version::V2,
            f,
            Knowledge::MissingRaw,
            2,
            FieldOrigin::Live,
            2,
        );
    }
    let step=r.apply(json!({"sessionUpdate":"tool_call_content_chunk","toolCallId":"t","content":{"type":"content","content":{"type":"text","text":"x".repeat(40_000)}},"_meta":{"entity":"never"},"status":"failed"})).await;
    dropped(step, 3);
    assert_eq!(step.anchor, AnchorOutcome::Updated { position: p });
    let (_, after) = r.anchor("t");
    fact(
        &after,
        Version::V2,
        AnchorField::Content,
        Knowledge::MissingRaw,
        3,
        FieldOrigin::Live,
        3,
    );
    for f in FIELDS.into_iter().filter(|f| *f != AnchorField::Content) {
        assert_eq!(
            decoded(&after, Version::V2, "t").field(f),
            decoded(&b, Version::V2, "t").field(f)
        );
    }
    assert_eq!(
        decoded(&after, Version::V2, "t").tool_status(),
        ToolStatusFact::Pending
    );
}

#[tokio::test]
async fn c_v1_closed_enums_v2_unknown_and_raw_kept_verbatim() {
    let mut r = Rig::new(Version::V1, 200_000, 1000).await;
    created(r.apply(initial("t")).await);
    for (field, values) in [
        (
            "status",
            vec!["pending", "in_progress", "completed", "failed"],
        ),
        (
            "kind",
            vec![
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
            ],
        ),
    ] {
        for value in values {
            let mut u = json!({"sessionUpdate":"tool_call_update","toolCallId":"t"});
            u[field] = json!(value);
            assert!(matches!(
                r.apply(u).await.anchor,
                AnchorOutcome::Updated { .. }
            ));
        }
    }
    for (field, value) in [
        ("status", "cancelled"),
        ("status", "future-status"),
        ("kind", "future-kind"),
    ] {
        let old = r.anchor("t");
        let mut u =
            json!({"sessionUpdate":"tool_call_update","toolCallId":"t","title":"must-not-apply"});
        u[field] = json!(value);
        assert_eq!(
            r.apply(u).await.anchor,
            AnchorOutcome::Failed(ProjectionError::MalformedUpdate)
        );
        assert_eq!(r.anchor("t"), old);
    }
    let mut r = Rig::new(Version::V2, 200_000, 1000).await;
    let raw=b"{ \"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"A\",\"update\":{\"sessionUpdate\":\"tool_call_update\",\"toolCallId\":\"t\",\"status\":\"_future-status\",\"kind\":\"future-kind\",\"unknown\":1e+02}},\"_meta\":{\"escape\":\"\\u0061\"}}";
    created(r.apply_bytes(raw).await);
    let (_, b) = r.anchor("t");
    let view = decoded(&b, Version::V2, "t");
    assert_eq!(
        view.tool_status(),
        ToolStatusFact::UnrecognizedInProgress {
            sequence: 1,
            raw: RawEvidence::Unverified
        }
    );
    assert_eq!(
        view.tool_kind(),
        ToolKindFact::UnrecognizedOther {
            sequence: 1,
            raw: RawEvidence::Unverified
        }
    );
    let snap = r.windows.snapshot(&r.h).unwrap();
    let record = snap.items().find(|i| i.key.is_none()).unwrap();
    assert_eq!(parse_header(record.bytes).unwrap().raw, raw);
    drop(snap);
    r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","status":"cancelled"}))
        .await;
    assert_eq!(
        decoded(&r.anchor("t").1, Version::V2, "t").tool_status(),
        ToolStatusFact::Cancelled
    );
}

#[tokio::test]
async fn d_257_and_long_id_late_completion_remains_original_position_both_versions() {
    for v in [Version::V1, Version::V2] {
        let mut r = Rig::new(v, 16 * 1024 * 1024, 4096).await;
        let mut first = 0;
        for n in 0..257 {
            let p = created(r.apply(initial(&format!("tool-{n}"))).await);
            if n == 0 {
                first = p;
            }
            r.apply(json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"following output"}})).await;
        }
        let long = "é".repeat(2048);
        let long_pos = created(r.apply(initial(&long)).await);
        let before = r.windows.window_status(&r.h).unwrap();
        assert_eq!(before.evicted_count, 0);
        let step = r.apply(large_update("tool-0", 300_000, "completed")).await;
        recorded(step, 516);
        assert_eq!(step.anchor, AnchorOutcome::Updated { position: first });
        assert_eq!(
            r.apply(
                json!({"sessionUpdate":"tool_call_update","toolCallId":long,"status":"completed"})
            )
            .await
            .anchor,
            AnchorOutcome::Updated { position: long_pos }
        );
        let (p, b) = r.anchor("tool-0");
        assert_eq!(p, first);
        assert_eq!(
            decoded(&b, v, "tool-0").tool_status(),
            ToolStatusFact::Completed
        );
        assert_eq!(u64_at(&b, 8), 1);
        assert_eq!(u64_at(&b, 16), 516);
        assert_eq!(r.read.raw_stats().last_sequence, 517);
        assert_eq!(r.windows.window_status(&r.h).unwrap().evicted_count, 0);
        r.no_wire().await;
    }
}

#[tokio::test]
async fn e_dropped_raw_updates_small_facts_unknown_evidence_and_loss_survives_success() {
    for v in [Version::V1, Version::V2] {
        let mut r = Rig::new(v, 14_000, 100).await;
        let p = created(r.apply(initial("t")).await);
        let step = r.apply(large_update("t", 40_000, "completed")).await;
        dropped(step, 2);
        assert_eq!(step.anchor, AnchorOutcome::Updated { position: p });
        let (_, b) = r.anchor("t");
        fact(
            &b,
            v,
            AnchorField::Status,
            Knowledge::Known,
            2,
            FieldOrigin::Live,
            2,
        );
        fact(
            &b,
            v,
            AnchorField::Name,
            Knowledge::MissingRaw,
            2,
            FieldOrigin::Live,
            2,
        );
        assert_eq!(decoded(&b, v, "t").tool_status(), ToolStatusFact::Completed);
        let step=r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","status":"failed","name":"recovered"})).await;
        recorded(step, 3);
        let (_, b) = r.anchor("t");
        fact(
            &b,
            v,
            AnchorField::Status,
            Knowledge::Known,
            3,
            FieldOrigin::Live,
            2,
        );
        fact(
            &b,
            v,
            AnchorField::Name,
            Knowledge::Known,
            3,
            FieldOrigin::Live,
            2,
        );
        assert_eq!(b[7] & 2, 2);
        assert_eq!(r.read.raw_stats().dropped_count, 1);
        assert_eq!(r.read.raw_stats().last_sequence, 3);
        r.no_wire().await;
    }
    let mut r = Rig::new(Version::V2, 14_000, 100).await;
    let p = created(r.apply(initial("t")).await);
    let mut update = large_update("t", 40_000, "future");
    update["kind"] = json!("_future");
    let step = r.apply(update).await;
    dropped(step, 2);
    assert_eq!(step.anchor, AnchorOutcome::Updated { position: p });
    let (_, b) = r.anchor("t");
    let view = decoded(&b, Version::V2, "t");
    assert_eq!(
        view.tool_status(),
        ToolStatusFact::UnrecognizedInProgress {
            sequence: 2,
            raw: RawEvidence::NotRecorded
        }
    );
    assert_eq!(
        view.tool_kind(),
        ToolKindFact::UnrecognizedOther {
            sequence: 2,
            raw: RawEvidence::NotRecorded
        }
    );
}

#[tokio::test]
async fn e_recorded_raw_can_be_evicted_by_its_own_anchor_or_later_fifo() {
    let mut update = initial("t");
    update["padding"] = json!("x".repeat(5000));
    let raw = frame(update);
    let raw_cost = 512 + 32 + raw.len();
    // identity1025 + window1024 + two read workspaces2560 + retained raw + anchor workspace1024.
    let limit = 1025 + 1024 + 2560 + raw_cost + 1024;
    let mut r = Rig::new(Version::V2, limit, 100).await;
    let step = r.apply_bytes(&raw).await;
    recorded(step, 1);
    let pos = created(step);
    assert_eq!(r.windows.window_status(&r.h).unwrap().evicted_count, 1);
    let snap = r.windows.snapshot(&r.h).unwrap();
    assert_eq!(snap.items().len(), 1);
    let item = snap.items().next().unwrap();
    assert_eq!(item.position, pos);
    let view = decode_anchor(item, Version::V2).unwrap();
    assert_eq!(view.tool_status(), ToolStatusFact::Pending);
    assert_eq!(view.field(AnchorField::Name).knowledge, Knowledge::Known);
    drop(snap);
    let mut r = Rig::new(Version::V2, 14_000, 100).await;
    let mut update = initial("t");
    update["padding"] = json!("x".repeat(5000));
    let p = created(r.apply(update).await);
    r.apply(json!({"sessionUpdate":"future_display","padding":"y".repeat(5000)}))
        .await;
    let snap = r.windows.snapshot(&r.h).unwrap();
    assert!(
        !snap
            .items()
            .filter(|i| i.key.is_none())
            .any(|i| parse_header(i.bytes).unwrap().sequence == 1)
    );
    drop(snap);
    let (position, b) = r.anchor("t");
    assert_eq!(position, p);
    fact(
        &b,
        Version::V2,
        AnchorField::Name,
        Knowledge::Known,
        1,
        FieldOrigin::Live,
        0,
    );
    assert_eq!(
        decoded(&b, Version::V2, "t").tool_status(),
        ToolStatusFact::Pending
    );
}

#[tokio::test]
async fn f_actual_fifo_removal_allows_only_new_partial_anchor() {
    let mut r = Rig::new(Version::V2, 14_000, 100).await;
    let old = created(r.apply(initial("t")).await);
    r.apply(json!({"sessionUpdate":"future_display","padding":"x".repeat(6000)}))
        .await;
    assert_eq!(r.anchor("t").0, old);
    let step = r.apply(large_update("t", 5500, "completed")).await;
    recorded(step, 3);
    let new = created(step);
    assert!(new > old);
    let (_, b) = r.anchor("t");
    assert_eq!(u64_at(&b, 8), 3);
    assert_eq!(u64_at(&b, 16), 3);
    assert_eq!(b[7] & 1, 1);
    assert_eq!(
        decoded(&b, Version::V2, "t").tool_status(),
        ToolStatusFact::Completed
    );
    assert!(
        r.windows
            .window_status(&r.h)
            .unwrap()
            .evicted_through
            .unwrap()
            >= old
    );
}

#[tokio::test]
async fn f_capacity_at_lease_workspace_candidate_never_changes_old_or_falls_back() {
    for remaining in [319, 320 + 1023, 320 + 1024 + 1025 - 1] {
        let mut r = Rig::new(Version::V2, 20_000, 100).await;
        created(r.apply(initial("t")).await);
        let old = r.anchor("t");
        let before = r.usage();
        let reserved_bytes = 20_000 - before.bytes - remaining;
        let blocker = r
            .windows
            .stage_workspace(&r.h, reserved_bytes - 512)
            .unwrap();
        let held = r.usage();
        let step = r.apply(large_update("t", 40_000, "completed")).await;
        dropped(step, 2);
        assert_eq!(
            step.anchor,
            AnchorOutcome::Failed(ProjectionError::View(ViewError::Capacity))
        );
        assert_eq!(r.usage(), held);
        drop(blocker);
        assert_eq!(r.anchor("t"), old);
        assert_eq!(r.usage(), before);
        assert_eq!(r.read.raw_stats().last_sequence, 2);
        assert_eq!(r.read.raw_stats().dropped_count, 1);
        assert_eq!(r.windows.window_status(&r.h).unwrap().evicted_count, 0);
        let step = r
            .apply(
                json!({"sessionUpdate":"tool_call_update","toolCallId":"t","status":"completed"}),
            )
            .await;
        assert_eq!(step.anchor, AnchorOutcome::Updated { position: old.0 });
    }
}

#[tokio::test]
async fn g_exact_replacement_peak_and_layout_charges() {
    let mut r = Rig::new(Version::V2, 20_000, 100).await;
    let p = created(r.apply(initial("t")).await);
    let before = r.usage();
    let peak = 320 + 1024 + (512 + 1 + 512);
    let reserved_bytes = 20_000 - before.bytes - peak;
    let blocker = r
        .windows
        .stage_workspace(&r.h, reserved_bytes - 512)
        .unwrap();
    let held = r.usage();
    let step = r.apply(large_update("t", 40_000, "completed")).await;
    dropped(step, 2);
    assert_eq!(step.anchor, AnchorOutcome::Updated { position: p });
    assert_eq!(r.usage(), held);
    drop(blocker);
    assert_eq!(r.usage(), before);
    println!(
        "P2A_LAYOUT read={} raw_read={} extra={} replacement_peak={} anchor_bytes={} usage_after={} items={}",
        size_of::<ToolProjectionRead>(),
        size_of::<acp_remote::adapters::acp::view_observations::ObservationRead>(),
        size_of::<ToolProjectionRead>()
            - size_of::<acp_remote::adapters::acp::view_observations::ObservationRead>(),
        peak,
        r.anchor("t").1.len(),
        before.bytes,
        before.items
    );
}

#[tokio::test]
async fn f_corrupt_anchor_is_failed_not_missing_and_raw_result_is_preserved() {
    let mut r = Rig::new(Version::V2, 100_000, 100).await;
    let p = r
        .windows
        .insert(&r.h, Some(key("t")), b"not an anchor")
        .unwrap();
    let old = r.anchor("t");
    let step = r.apply(initial("t")).await;
    recorded(step, 1);
    assert_eq!(
        step.anchor,
        AnchorOutcome::Failed(ProjectionError::InvalidAnchor)
    );
    assert_eq!(r.anchor("t"), old);
    assert_eq!(old.0, p);
    assert_eq!(r.read.raw_stats().last_sequence, 1);
    assert_eq!(r.read.raw_stats().dropped_count, 0);
    r.no_wire().await;
}

#[tokio::test]
async fn f_full_reference_ledger_rejects_lookup_without_fallback() {
    let mut r = Rig::new(Version::V2, 20_000, 4).await;
    created(r.apply(initial("t")).await);
    let old = r.anchor("t");
    let snapshot = r.windows.snapshot(&r.h).unwrap();
    assert_eq!(r.usage().items, 4);
    let step = r.apply(large_update("t", 40_000, "completed")).await;
    dropped(step, 2);
    assert_eq!(
        step.anchor,
        AnchorOutcome::Failed(ProjectionError::View(ViewError::Capacity))
    );
    assert_eq!(r.usage().items, 4);
    assert_eq!(r.windows.window_status(&r.h).unwrap().evicted_count, 0);
    drop(snapshot);
    assert_eq!(r.anchor("t"), old);
}

#[tokio::test]
async fn g_begin_second_workspace_failure_refunds_and_drop_retains_only_old_scope() {
    let (client, _peer) = ready(Version::V2).await;
    let mut windows = ViewWindows::new(ViewLimits {
        target_bytes: 1025 + 1024 + 1536 + 1023,
        ..ViewLimits::default()
    })
    .unwrap();
    let h = windows.begin(target(), epoch(1)).unwrap();
    let before = windows.target_usage(&target()).unwrap();
    assert!(matches!(
        ToolProjectionRead::begin(&mut windows, &h, client.strategy().unwrap()),
        Err(ProjectionError::View(ViewError::Capacity))
    ));
    assert_eq!(windows.target_usage(&target()).unwrap(), before);
    let mut r = Rig::new(Version::V2, 100_000, 100).await;
    assert_eq!(
        r.usage(),
        ViewUsage {
            bytes: 1025 + 1024 + 2560,
            items: 0
        }
    );
    assert!(
        size_of::<ToolProjectionRead>()
            - size_of::<acp_remote::adapters::acp::view_observations::ObservationRead>()
            <= 512
    );
    let initial_usage = r.usage();
    r.windows.release(&r.h).unwrap();
    assert_eq!(r.usage(), initial_usage);
    let newer = r.windows.begin(target(), epoch(2)).unwrap();
    let new_usage = r.usage();
    assert_eq!(new_usage.bytes, initial_usage.bytes + 1024);
    let input = r.incoming(&frame(initial("t"))).await;
    let stats = r.read.raw_stats();
    assert_eq!(
        r.read.apply(&mut r.windows, &input),
        Err(ProjectionError::Observation(ObservationError::View(
            ViewError::Stale
        )))
    );
    assert_eq!(r.read.raw_stats(), stats);
    assert_eq!(
        r.windows.window_status(&newer).unwrap().state,
        ViewState::Reading
    );
    drop(r.read);
    assert_eq!(
        r.windows.target_usage(&target()).unwrap().bytes,
        1025 + 1024
    );
    r.windows.release(&newer).unwrap();
    r.windows.collect_released();
    assert_eq!(
        r.windows.usage(),
        ViewUsage {
            bytes: 8192,
            items: 0
        }
    );
}

#[tokio::test]
async fn g_ignored_invalid_and_foreign_scope_do_not_double_observe() {
    let mut r = Rig::new(Version::V2, 100_000, 100).await;
    let raw = frame(initial("t"));
    let mut other_wire: Value = serde_json::from_slice(&raw).unwrap();
    other_wire["params"]["sessionId"] = json!("B");
    let input = r.incoming(&serde_json::to_vec(&other_wire).unwrap()).await;
    let before = r.read.raw_stats();
    let usage = r.usage();
    let step = r.read.apply(&mut r.windows, &input).unwrap();
    assert_eq!(step.raw, ObservationOutcome::Ignored);
    assert_eq!(step.anchor, AnchorOutcome::Ignored);
    assert_eq!(r.read.raw_stats(), before);
    assert_eq!(r.usage(), usage);
    let (mut external, mut external_peer) = ready(Version::V2).await;
    let input = next_raw(&mut external, &mut external_peer, &raw)
        .await
        .unwrap();
    assert_eq!(
        r.read.apply(&mut r.windows, &input),
        Err(ProjectionError::Observation(
            ObservationError::InvalidObservation
        ))
    );
    assert_eq!(r.read.raw_stats(), before);
    let mut other = ViewWindows::new(ViewLimits::default()).unwrap();
    let input = r.incoming(&raw).await;
    assert_eq!(
        r.read.apply(&mut other, &input),
        Err(ProjectionError::Observation(ObservationError::View(
            ViewError::Stale
        )))
    );
    assert_eq!(r.read.raw_stats(), before);
    assert_eq!(created(r.apply(initial("t")).await), 2);
    assert_eq!(r.read.raw_stats().last_sequence, 1);
    r.no_wire().await;
}

#[tokio::test]
async fn d_replay_then_live_preserves_per_field_and_raw_origins_without_new_read() {
    let mut r = Rig::new(Version::V1, 100_000, 100).await;
    let request = r
        .client
        .strategy()
        .unwrap()
        .legacy_replay(
            SessionId::parse(raw_value(&json!("A")).as_ref()).unwrap(),
            "C:/synthetic",
        )
        .unwrap();
    let req = r.client.request(request, deadline()).await.unwrap();
    assert_eq!(wire(&mut r.peer).await["method"], "session/load");
    let a = frame(initial("t"));
    let p = created(r.apply_bytes(&a).await);
    let response =
        serde_json::to_vec(&json!({"jsonrpc":"2.0","id":req.json(),"result":{}})).unwrap();
    let event = r.incoming(&response).await;
    assert_eq!(
        r.read.apply(&mut r.windows, &event).unwrap().raw,
        ObservationOutcome::Ignored
    );
    let b =
        frame(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","status":"completed"}));
    assert_eq!(
        r.apply_bytes(&b).await.anchor,
        AnchorOutcome::Updated { position: p }
    );
    let (_, anchor) = r.anchor("t");
    fact(
        &anchor,
        Version::V1,
        AnchorField::Title,
        Knowledge::Known,
        1,
        FieldOrigin::DuringReplay,
        0,
    );
    fact(
        &anchor,
        Version::V1,
        AnchorField::Status,
        Knowledge::Known,
        2,
        FieldOrigin::Live,
        0,
    );
    assert_eq!(anchor[24], 5);
    let snap = r.windows.snapshot(&r.h).unwrap();
    let raws: Vec<_> = snap
        .items()
        .filter(|i| i.key.is_none())
        .map(|i| {
            let header = parse_header(i.bytes).unwrap();
            (header.sequence, header.origin, header.raw.to_vec())
        })
        .collect();
    assert_eq!(
        raws,
        vec![
            (1, UpdateOrigin::DuringReplay, a),
            (2, UpdateOrigin::Live, b)
        ]
    );
    assert_eq!(r.read.raw_stats().last_sequence, 2);
    drop(snap);
    let request = r
        .client
        .strategy()
        .unwrap()
        .legacy_resume(
            SessionId::parse(raw_value(&json!("A")).as_ref()).unwrap(),
            "C:/synthetic",
        )
        .unwrap();
    let req = r.client.request(request, deadline()).await.unwrap();
    assert_eq!(wire(&mut r.peer).await["method"], "session/resume");
    r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","name":"resume-name"}))
        .await;
    let response =
        serde_json::to_vec(&json!({"jsonrpc":"2.0","id":req.json(),"result":{}})).unwrap();
    let event = r.incoming(&response).await;
    assert_eq!(
        r.read.apply(&mut r.windows, &event).unwrap().raw,
        ObservationOutcome::Ignored
    );
    r.apply(json!({"sessionUpdate":"tool_call_update","toolCallId":"t","status":"failed"}))
        .await;
    let (_, anchor) = r.anchor("t");
    fact(
        &anchor,
        Version::V1,
        AnchorField::Name,
        Knowledge::Known,
        3,
        FieldOrigin::DuringResume,
        0,
    );
    fact(
        &anchor,
        Version::V1,
        AnchorField::Status,
        Knowledge::Known,
        4,
        FieldOrigin::Live,
        0,
    );
    fact(
        &anchor,
        Version::V1,
        AnchorField::Title,
        Knowledge::Known,
        1,
        FieldOrigin::DuringReplay,
        0,
    );
    assert_eq!(anchor[24], 7);
    let snap = r.windows.snapshot(&r.h).unwrap();
    let origins: Vec<_> = snap
        .items()
        .filter(|item| item.key.is_none())
        .map(|item| {
            let header = parse_header(item.bytes).unwrap();
            (header.sequence, header.origin)
        })
        .collect();
    assert_eq!(
        origins,
        vec![
            (1, UpdateOrigin::DuringReplay),
            (2, UpdateOrigin::Live),
            (3, UpdateOrigin::DuringResume),
            (4, UpdateOrigin::Live)
        ]
    );
    assert_eq!(r.read.raw_stats().last_sequence, 4);
}
