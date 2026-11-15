#![cfg(test)]
//! Independent ordinary-dependency P1 cases. A synthetic owner is not node authorization.
#[path = "acp_runtime_acceptance/support/legacy.rs"]
mod bridge;
use acp_remote::{
    adapters::acp::{
        AcpError, Incoming, Response, SessionEvent, SessionId, Strategy, UpdateOrigin, Version,
        view_observations::{
            ObservationError, ObservationFailure, ObservationOutcome, ObservationRead, parse_header,
        },
    },
    domain::{
        EntityId,
        native::{
            HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch,
        },
        views::{
            ROOT_BYTES, ViewError, ViewFailure, ViewHandle, ViewLimits, ViewState, ViewWindows,
        },
    },
};
use bridge::*;
use serde_json::{Value, json};
use std::{
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{
        AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf,
    },
    time::{Instant, timeout},
};

struct Meter<W> {
    writer: W,
    bytes: Arc<AtomicUsize>,
}
impl<W: AsyncWrite + Unpin> AsyncWrite for Meter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match Pin::new(&mut this.writer).poll_write(cx, bytes) {
            Poll::Ready(Ok(n)) => {
                this.bytes.fetch_add(n, Ordering::SeqCst);
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().writer).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().writer).poll_shutdown(cx)
    }
}
type Client = TestClient;
struct Peer {
    read: BufReader<ReadHalf<DuplexStream>>,
    write: WriteHalf<DuplexStream>,
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn session(value: &str) -> SessionId {
    SessionId::parse(raw_value(&json!(value)).as_ref()).unwrap()
}
fn epoch(n: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([1; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([2; 16])),
        load: LoadEpoch(EntityId::from_bytes([n; 16])),
    }
}
fn native(name: &str) -> NativeSessionKey {
    NativeSessionKey::from_config(Some(EntityId::from_bytes([44; 16])), name).unwrap()
}
fn window(limits: ViewLimits, name: &str) -> (ViewWindows, ViewHandle) {
    let mut windows = ViewWindows::new(limits).unwrap();
    let h = windows.begin(native(name), epoch(3)).unwrap();
    (windows, h)
}
async fn emitted(peer: &mut Peer) -> Value {
    let mut bytes = Vec::new();
    assert!(
        timeout(
            Duration::from_secs(10),
            peer.read.read_until(b'\n', &mut bytes)
        )
        .await
        .unwrap()
        .unwrap()
            > 0
    );
    serde_json::from_slice(&bytes).unwrap()
}
async fn receive(
    client: &mut Client,
    peer: &mut Peer,
    bytes: &[u8],
) -> Result<acp_remote::adapters::acp::Received, AcpError> {
    let write = async {
        peer.write.write_all(bytes).await.unwrap();
        peer.write.write_all(b"\r\n").await.unwrap();
    };
    let ((), incoming) = tokio::join!(write, client.next(deadline()));
    incoming
}
async fn value(
    client: &mut Client,
    peer: &mut Peer,
    value: &Value,
) -> acp_remote::adapters::acp::Received {
    receive(client, peer, &serde_json::to_vec(value).unwrap())
        .await
        .unwrap()
}
async fn ready(version: Version) -> (Client, Peer, Strategy, Arc<AtomicUsize>) {
    let (local, remote) = tokio::io::duplex(4096);
    let (reader, writer) = tokio::io::split(local);
    let (pr, pw) = tokio::io::split(remote);
    let bytes = Arc::new(AtomicUsize::new(0));
    let mut client = TestClient::new(
        reader,
        Meter {
            writer,
            bytes: bytes.clone(),
        },
    );
    let mut peer = Peer {
        read: BufReader::new(pr),
        write: pw,
    };
    let id = client.initialize(version, deadline()).await.unwrap();
    assert_eq!(
        emitted(&mut peer).await["params"]["protocolVersion"],
        version.number()
    );
    let result = match version {
        Version::V1 => {
            json!({"protocolVersion":1,"agentInfo":{"name":"independent","version":"fixed"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}}})
        }
        Version::V2 => {
            json!({"protocolVersion":2,"info":{"name":"independent","version":"fixed"},"capabilities":{"session":{}}})
        }
    };
    assert!(matches!(
        value(
            &mut client,
            &mut peer,
            &json!({"jsonrpc":"2.0","id":id.json(),"result":result})
        )
        .await
        .incoming(),
        Incoming::Response {
            response: Response::Initialized(_),
            ..
        }
    ));
    let strategy = client.strategy().unwrap().clone();
    (client, peer, strategy, bytes)
}
fn update(name: &str, marker: usize) -> Value {
    json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":name,"update":{"sessionUpdate":"future_observation","marker":marker}}})
}
fn recorded(outcome: ObservationOutcome) -> (u64, u64) {
    match outcome {
        ObservationOutcome::Recorded {
            sequence,
            publication,
        } => {
            assert!(!publication.replaced);
            (sequence, publication.position)
        }
        other => panic!("expected record, got {other:?}"),
    }
}
fn assert_record(bytes: &[u8], raw: &[u8], version: Version, origin: UpdateOrigin, seq: u64) {
    assert_eq!(bytes.len(), 32 + raw.len());
    assert_eq!(&bytes[..4], b"ACPO");
    assert_eq!(bytes[4], 1);
    assert_eq!(bytes[5], u8::try_from(version.number()).unwrap());
    let tag = match origin {
        UpdateOrigin::Live => 1,
        UpdateOrigin::DuringResume => 2,
        UpdateOrigin::DuringReplay => 3,
    };
    assert_eq!(bytes[6], tag);
    assert_eq!(bytes[7], 1);
    assert_eq!(&bytes[8..16], &seq.to_le_bytes());
    assert_eq!(&bytes[16..24], &(raw.len() as u64).to_le_bytes());
    assert_eq!(&bytes[24..32], &[0; 8]);
    assert_eq!(&bytes[32..], raw);
    let parsed = parse_header(bytes).unwrap();
    assert_eq!(parsed.version, version);
    assert_eq!(parsed.origin, origin);
    assert_eq!(parsed.sequence, seq);
    assert_eq!(parsed.raw.as_ptr(), bytes[32..].as_ptr());
    assert_eq!(parsed.raw, raw);
}

#[tokio::test]
async fn r1_real_wire_exact_full_notification_both_versions() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer, strategy, meter) = ready(version).await;
        let (mut windows, h) = window(ViewLimits::default(), "session-A");
        let mut read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
        let raws: Vec<Vec<u8>>=vec![
            br#" { "params" : {"update":{"sessionUpdate":"future_observation","number":1e+02,"negative":-0,"text":"\u0000\u0061","future":{"list":[null,true,3]}},"sessionId":"session-A","outerFuture":[]}, "jsonrpc":"2.0", "method":"session/update", "_meta":{"full-envelope":true} } "#.to_vec(),
            "{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session-A\",\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"messageId\":\"m\",\"content\":{\"type\":\"future\",\"opaque\":[\"é中\",1.00]},\"_meta\":{\"chunk\":true}}},\"_meta\":{\"notify\":null}}".as_bytes().to_vec(),
            br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-A","update":{"sessionUpdate":"tool_call_update","toolCallId":"tool","status":"completed","_meta":{"opaque":{"nested":null}}}}}"#.to_vec(),
        ];
        let initial = meter.load(Ordering::SeqCst);
        for (i, raw) in raws.iter().enumerate() {
            let incoming = receive(&mut client, &mut peer, raw).await.unwrap();
            assert_eq!(
                recorded(read.observe(&mut windows, &incoming).unwrap()),
                ((i + 1) as u64, (i + 1) as u64)
            );
        }
        // P0's real v1-null entry, without inventing a synthetic Incoming.
        if version == Version::V1 {
            let raw=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-A","update":{"sessionUpdate":"agent_message_chunk","messageId":null,"content":{"type":"text","text":"null-id"}}}}"#;
            let incoming = receive(&mut client, &mut peer, raw).await.unwrap();
            assert!(matches!(
                incoming.incoming(),
                Incoming::Update {
                    event: SessionEvent::Message { message: None, .. },
                    ..
                }
            ));
            assert_eq!(
                recorded(read.observe(&mut windows, &incoming).unwrap()),
                (4, 4)
            );
        }
        let snapshot = windows.snapshot(&h).unwrap();
        for (i, item) in snapshot.items().enumerate().take(raws.len()) {
            assert!(item.key.is_none());
            assert_record(
                item.bytes,
                &raws[i],
                version,
                UpdateOrigin::Live,
                (i + 1) as u64,
            );
        }
        assert_eq!(meter.load(Ordering::SeqCst), initial);
        drop(snapshot);
        drop(read);
        windows.release(&h).unwrap();
        windows.collect_released();
        assert_eq!(windows.usage().bytes, ROOT_BYTES);
        assert_eq!(windows.usage().items, 0);
    }
}

#[tokio::test]
async fn r2_origin_sequence_ignored_and_response_does_not_reset() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer, strategy, _) = ready(version).await;
        let (mut windows, h) = window(ViewLimits::default(), "session-A");
        let mut read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
        let request = strategy
            .legacy_replay(session("session-A"), "C:/synthetic")
            .unwrap();
        let id = client.request(request, deadline()).await.unwrap();
        emitted(&mut peer).await;
        let raw = serde_json::to_vec(&update("session-A", 1)).unwrap();
        let incoming = receive(&mut client, &mut peer, &raw).await.unwrap();
        assert_eq!(
            recorded(read.observe(&mut windows, &incoming).unwrap()),
            (1, 1)
        );
        let before = windows.usage();
        let baseline_metrics = read.stats();
        let other = value(&mut client, &mut peer, &update("session-B", 99)).await;
        assert_eq!(
            read.observe(&mut windows, &other).unwrap(),
            ObservationOutcome::Ignored
        );
        if version == Version::V2 {
            let state=value(&mut client,&mut peer,&json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-A","update":{"sessionUpdate":"state_update","state":"idle"}}})).await;
            assert_eq!(
                read.observe(&mut windows, &state).unwrap(),
                ObservationOutcome::Ignored
            );
        }
        let response = value(
            &mut client,
            &mut peer,
            &json!({"jsonrpc":"2.0","id":id.json(),"result":{}}),
        )
        .await;
        assert_eq!(
            read.observe(&mut windows, &response).unwrap(),
            ObservationOutcome::Ignored
        );
        assert_eq!(read.stats(), baseline_metrics);
        assert_eq!(windows.usage(), before);
        let live = serde_json::to_vec(&update("session-A", 2)).unwrap();
        let incoming = receive(&mut client, &mut peer, &live).await.unwrap();
        assert_eq!(
            recorded(read.observe(&mut windows, &incoming).unwrap()),
            (2, 2)
        );
        let snapshot = windows.snapshot(&h).unwrap();
        let items: Vec<_> = snapshot.items().collect();
        assert_record(items[0].bytes, &raw, version, UpdateOrigin::DuringReplay, 1);
        assert_record(items[1].bytes, &live, version, UpdateOrigin::Live, 2);
        drop(items);
        drop(snapshot);
        if version == Version::V2 {
            let id = client
                .request(
                    strategy
                        .legacy_resume(session("session-A"), "C:/synthetic")
                        .unwrap(),
                    deadline(),
                )
                .await
                .unwrap();
            emitted(&mut peer).await;
            let resumed = value(&mut client, &mut peer, &update("session-A", 3)).await;
            assert_eq!(
                recorded(read.observe(&mut windows, &resumed).unwrap()),
                (3, 3)
            );
            let s = windows.snapshot(&h).unwrap();
            assert_eq!(
                parse_header(s.items().last().unwrap().bytes)
                    .unwrap()
                    .origin,
                UpdateOrigin::DuringResume
            );
            drop(s);
            // No snapshot: true protocol failure, not an observation drop.
            assert!(matches!(
                receive(
                    &mut client,
                    &mut peer,
                    &serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id.json(),"result":{}}))
                        .unwrap()
                )
                .await,
                Err(AcpError::MissingSnapshot)
            ));
            assert!(client.is_closed());
        }
        drop(read);
        windows.finish(&h).unwrap();
        let next = windows.begin(native("session-A"), epoch(4)).unwrap();
        let new_read = ObservationRead::begin(&mut windows, &next, &strategy).unwrap();
        assert_eq!(new_read.stats().last_sequence, 0);
    }
}

#[tokio::test]
async fn r3_production_long_stream_rolls_and_large_frame_does_not_drain() {
    let (mut client, mut peer, strategy, meter) = ready(Version::V2).await;
    let (mut windows, h) = window(ViewLimits::default(), "session-A");
    let mut read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
    let before = meter.load(Ordering::SeqCst);
    for i in 1..=13000 {
        let event = value(&mut client, &mut peer, &update("session-A", i)).await;
        assert_eq!(
            recorded(read.observe(&mut windows, &event).unwrap()),
            (i as u64, i as u64)
        );
    }
    let status = windows.window_status(&h).unwrap();
    assert!(status.evicted_count > 8192);
    assert_eq!(status.last_position, Some(13000));
    assert_eq!(status.retained_items, 4096);
    // At the production item ceiling a full snapshot needs another 4096 charged references.
    // This proves storage progress only; first-subscriber delivery remains a separate limitation.
    assert!(matches!(windows.snapshot(&h), Err(ViewError::Capacity)));
    println!(
        "R3 count ceiling: retained4096, tail-position13000; full snapshot Capacity; NOT subscriber readback"
    );
    drop(read);
    windows.release(&h).unwrap();
    let (mut windows, h) = window(ViewLimits::default(), "session-A");
    let mut read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
    let payload = "x".repeat(256 * 1024);
    for i in 1..=205 {
        let event=value(&mut client,&mut peer,&json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-A","update":{"sessionUpdate":"future_observation","marker":i,"payload":payload}}})).await;
        recorded(read.observe(&mut windows, &event).unwrap());
    }
    let s = windows.snapshot(&h).unwrap();
    assert!(s.evicted_count() > 100);
    assert_eq!(
        parse_header(s.items().last().unwrap().bytes)
            .unwrap()
            .sequence,
        205
    );
    drop(s);
    drop(read);
    windows.release(&h).unwrap();
    let limits = ViewLimits {
        target_bytes: 6000,
        node_bytes: 20000,
        target_items: 32,
        node_items: 64,
    };
    let (mut windows, h) = window(limits, "session-A");
    let mut read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
    let huge=value(&mut client,&mut peer,&json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-A","update":{"sessionUpdate":"future_observation","payload":"x".repeat(12000)}}})).await;
    assert_eq!(
        read.observe(&mut windows, &huge).unwrap(),
        ObservationOutcome::Dropped {
            sequence: 1,
            reason: ObservationFailure::Capacity
        }
    );
    let small = value(&mut client, &mut peer, &update("session-A", 2)).await;
    assert_eq!(
        recorded(read.observe(&mut windows, &small).unwrap()),
        (2, 1)
    );
    let loss_metrics = read.stats();
    assert_eq!(loss_metrics.dropped_count, 1);
    assert_eq!(loss_metrics.first_dropped, Some(1));
    assert_eq!(loss_metrics.last_dropped, Some(1));
    assert_eq!(windows.finish(&h), Err(ViewError::Incomplete));
    assert_eq!(
        windows.window_status(&h).unwrap().state,
        ViewState::Incomplete(ViewFailure::Capacity)
    );
    assert_eq!(meter.load(Ordering::SeqCst), before);
    println!("R3 production: 13000 records; 205 x >=256KiB >48MiB; recovery seq=2 position=1");
}

#[path = "acceptance/p1_observation_ordering.rs"]
mod ordering;

#[tokio::test]
async fn r4_capacity_failure_preserves_permission_state_other_session_pump() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer, strategy, meter) = ready(version).await;
        let (mut windows, h) = window(
            ViewLimits {
                target_bytes: 6000,
                node_bytes: 30000,
                target_items: 32,
                node_items: 64,
            },
            "session-A",
        );
        let hb = windows.begin(native("session-B"), epoch(3)).unwrap();
        let mut a = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
        let mut b = ObservationRead::begin(&mut windows, &hb, &strategy).unwrap();
        let id = client
            .request(
                strategy
                    .legacy_replay(session("session-A"), "C:/synthetic")
                    .unwrap(),
                deadline(),
            )
            .await
            .unwrap();
        emitted(&mut peer).await;
        let baseline = meter.load(Ordering::SeqCst);
        let huge=value(&mut client,&mut peer,&json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-A","update":{"sessionUpdate":"future","data":"x".repeat(9000)}}})).await;
        assert!(matches!(
            a.observe(&mut windows, &huge).unwrap(),
            ObservationOutcome::Dropped { .. }
        ));
        let mut permission = json!({"jsonrpc":"2.0","id":"pending-only","method":"session/request_permission","params":{"sessionId":"session-A","title":"synthetic permission","options":[{"optionId":"deny","kind":"reject_once","name":"synthetic"}]}});
        if version == Version::V1 {
            permission["params"]["toolCall"] = json!({"toolCallId":"t"});
        }
        let incoming = value(&mut client, &mut peer, &permission).await;
        assert!(matches!(incoming.incoming(), Incoming::Permission(_)));
        assert_eq!(
            a.observe(&mut windows, &incoming).unwrap(),
            ObservationOutcome::Ignored
        );
        let state=value(&mut client,&mut peer,&json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-A","update":{"sessionUpdate":"state_update","state":"idle"}}})).await;
        if version == Version::V2 {
            assert!(matches!(
                state.incoming(),
                Incoming::Update {
                    event: SessionEvent::State { .. },
                    ..
                }
            ));
            assert_eq!(
                a.observe(&mut windows, &state).unwrap(),
                ObservationOutcome::Ignored
            );
        } else {
            recorded(a.observe(&mut windows, &state).unwrap());
        }
        let other = value(&mut client, &mut peer, &update("session-B", 1)).await;
        assert_eq!(
            a.observe(&mut windows, &other).unwrap(),
            ObservationOutcome::Ignored
        );
        assert_eq!(recorded(b.observe(&mut windows, &other).unwrap()), (1, 1));
        let response = value(
            &mut client,
            &mut peer,
            &json!({"jsonrpc":"2.0","id":id.json(),"result":{}}),
        )
        .await;
        assert_eq!(
            a.observe(&mut windows, &response).unwrap(),
            ObservationOutcome::Ignored
        );
        assert!(!client.is_closed());
        assert_eq!(client.pending_count(), 0);
        assert_eq!(
            meter.load(Ordering::SeqCst),
            baseline,
            "observation emitted bytes"
        );
        assert_eq!(
            windows.window_status(&h).unwrap().state,
            ViewState::Incomplete(ViewFailure::Capacity)
        );
        // A separately-owned permission handler remains usable; P1 itself emitted nothing.
        if let Incoming::Permission(request) = incoming.incoming() {
            client
                .permission_response(request, None, deadline())
                .await
                .unwrap();
        }
        let answer = emitted(&mut peer).await;
        assert_eq!(answer["id"], "pending-only");
        assert_eq!(answer["result"]["outcome"]["outcome"], "cancelled");
        assert!(answer.get("method").is_none());
    }
}

#[tokio::test]
async fn r5_exact_fees_peak_boundary_and_old_read_last_drop() {
    let (mut client, mut peer, strategy, _) = ready(Version::V2).await;
    let (mut windows, h) = window(ViewLimits::default(), "session-A");
    let baseline = windows.target_usage(h.target()).unwrap();
    assert_eq!(baseline.bytes, 1024 + 9 + 1024);
    let mut read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
    assert_eq!(
        windows.target_usage(h.target()).unwrap().bytes,
        baseline.bytes + 1536
    );
    let raw = serde_json::to_vec(&update("session-A", 7)).unwrap();
    let incoming = receive(&mut client, &mut peer, &raw).await.unwrap();
    recorded(read.observe(&mut windows, &incoming).unwrap());
    let item_cost = 512 + 32 + raw.len();
    assert_eq!(
        windows.target_usage(h.target()).unwrap().bytes,
        baseline.bytes + 1536 + item_cost
    );
    let old = windows.snapshot(&h).unwrap();
    assert_eq!(
        windows.target_usage(h.target()).unwrap().bytes,
        baseline.bytes + 1536 + item_cost + 320
    );
    windows.release(&h).unwrap();
    assert_eq!(
        windows.target_usage(h.target()).unwrap().bytes,
        baseline.bytes + 1536 + item_cost + 320
    );
    let next = windows.begin(native("session-A"), epoch(4)).unwrap();
    assert_eq!(
        windows.target_usage(h.target()).unwrap().bytes,
        baseline.bytes + 1536 + item_cost + 320 + 1024
    );
    assert!(matches!(
        read.observe(&mut windows, &incoming),
        Err(ObservationError::View(ViewError::Stale))
    ));
    assert_eq!(
        windows.window_status(&next).unwrap().state,
        ViewState::Reading
    );
    drop(read);
    assert_eq!(
        windows.target_usage(h.target()).unwrap().bytes,
        baseline.bytes + item_cost + 320
    );
    drop(old);
    assert_eq!(
        windows.target_usage(h.target()).unwrap().bytes,
        baseline.bytes
    );
    windows.release(&next).unwrap();
    windows.collect_released();
    assert_eq!(windows.usage().bytes, ROOT_BYTES);
    // Real per-frame scratch peak: each below/at boundary fixture uses ordinary API.
    let peak = baseline.bytes + 1536 + 768 + item_cost;
    for (budget, expected) in [(peak - 1, false), (peak, true)] {
        let (mut windows, h) = window(
            ViewLimits {
                target_bytes: budget,
                node_bytes: ROOT_BYTES + budget,
                target_items: 8,
                node_items: 8,
            },
            "session-A",
        );
        let mut read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
        assert_eq!(
            matches!(
                read.observe(&mut windows, &incoming).unwrap(),
                ObservationOutcome::Recorded { .. }
            ),
            expected
        );
        assert!(windows.target_usage(h.target()).unwrap().bytes <= budget);
    }
    println!(
        "R5 independent costs: read=1536 frame=768 raw={} item={} exactPeak={peak}",
        raw.len(),
        item_cost
    );
}

#[test]
fn r6_header_offsets_all_invalid_tags_lengths_and_borrowed_format_only() {
    let mut valid = vec![0_u8; 35];
    valid[..4].copy_from_slice(b"ACPO");
    valid[4] = 1;
    valid[5] = 2;
    valid[6] = 3;
    valid[7] = 1;
    valid[8..16].copy_from_slice(&1_u64.to_le_bytes());
    valid[16..24].copy_from_slice(&3_u64.to_le_bytes());
    valid[32..].copy_from_slice(b"bad");
    let record = parse_header(&valid).unwrap();
    assert_eq!(record.raw, b"bad");
    assert_eq!(record.raw.as_ptr(), valid[32..].as_ptr());
    for length in 0..35 {
        assert!(matches!(
            parse_header(&valid[..length]),
            Err(ObservationError::InvalidHeader)
        ));
    }
    for index in [0, 1, 2, 3, 4, 5, 6, 7, 24, 25, 26, 27, 28, 29, 30, 31] {
        let mut bad = valid.clone();
        bad[index] = 255;
        assert!(
            matches!(parse_header(&bad), Err(ObservationError::InvalidHeader)),
            "index {index}"
        );
    }
    for index in [4, 5, 6, 7] {
        let mut bad = valid.clone();
        bad[index] = 0;
        assert!(parse_header(&bad).is_err());
    }
    for length in [0_u64, 1, 2, 4, u64::MAX] {
        let mut bad = valid.clone();
        bad[16..24].copy_from_slice(&length.to_le_bytes());
        assert!(parse_header(&bad).is_err());
    }
    let mut bad = valid.clone();
    bad[8..16].fill(0);
    assert!(parse_header(&bad).is_err());
    let mut extra = valid.clone();
    extra.push(0);
    assert!(parse_header(&extra).is_err());
    let twice = [valid.as_slice(), valid.as_slice()].concat();
    assert!(parse_header(&twice).is_err());
    for version in [1, 2] {
        for origin in [1, 2, 3] {
            let mut good = valid.clone();
            good[5] = version;
            good[6] = origin;
            assert!(parse_header(&good).is_ok());
        }
    }
    let debug = format!("{record:?}");
    assert!(!debug.contains("bad"));
    println!(
        "R6 all truncations 0..34; 16 corrupt tag/reserved offsets; zero/overflow/inexact lengths; borrowed non-JSON accepted as format only"
    );
}

#[tokio::test]
async fn r7_inconsistent_public_fields_foreign_windows_and_read_lifetime() {
    let (mut client, mut peer, strategy, _) = ready(Version::V2).await;
    let (mut windows, h) = window(ViewLimits::default(), "session-A");
    let mut read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
    let (mut external, mut external_peer, _, _) = ready(Version::V2).await;
    let incoming = value(&mut external, &mut external_peer, &update("session-A", 1)).await;
    let before = read.stats();
    let usage = windows.usage();
    assert!(matches!(
        read.observe(&mut windows, &incoming),
        Err(ObservationError::InvalidObservation)
    ));
    assert_eq!(read.stats(), before);
    assert_eq!(windows.usage(), usage);
    let incoming = value(&mut client, &mut peer, &update("session-A", 2)).await;
    let (mut foreign, fh) = window(ViewLimits::default(), "session-A");
    assert!(matches!(
        read.observe(&mut foreign, &incoming),
        Err(ObservationError::View(ViewError::Stale))
    ));
    assert_eq!(
        foreign.window_status(&fh).unwrap().state,
        ViewState::Reading
    );
    assert_eq!(read.stats(), before);
    let ignored = value(
        &mut client,
        &mut peer,
        &json!({"jsonrpc":"2.0","method":"future/notification","params":{}}),
    )
    .await;
    assert_eq!(
        read.observe(&mut foreign, &ignored).unwrap(),
        ObservationOutcome::Ignored
    );
    recorded(read.observe(&mut windows, &incoming).unwrap());
    drop(incoming);
    drop(strategy);
    drop(client);
    drop(peer);
    let snapshot = windows.snapshot(&h).unwrap();
    assert_eq!(snapshot.items().len(), 1);
    assert!(
        parse_header(snapshot.items().next().unwrap().bytes)
            .unwrap()
            .raw
            .starts_with(b"{")
    );
    drop(snapshot);
    drop(read);
    windows.release(&h).unwrap();
    windows.collect_released();
    assert_eq!(windows.usage().bytes, ROOT_BYTES);
}

#[tokio::test]
async fn r8_true_transport_error_is_not_a_display_drop_and_read_begin_capacity() {
    let (mut client, mut peer, strategy, _) = ready(Version::V2).await;
    let (mut windows, h) = window(ViewLimits::default(), "session-A");
    let read = ObservationRead::begin(&mut windows, &h, &strategy).unwrap();
    let malformed=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-A","update":{"sessionUpdate":"agent_message_chunk","messageId":null,"content":{"type":"text","text":"invalid-v2"}}}}"#;
    assert!(receive(&mut client, &mut peer, malformed).await.is_err());
    assert!(client.is_closed());
    assert_eq!(read.stats().last_sequence, 0);
    assert_eq!(read.stats().dropped_count, 0);
    let initial = 1024 + 9 + 1024;
    let (mut windows, h) = window(
        ViewLimits {
            target_bytes: initial + 1535,
            node_bytes: ROOT_BYTES + initial + 1535,
            target_items: 1,
            node_items: 1,
        },
        "session-A",
    );
    let before = windows.usage();
    assert!(matches!(
        ObservationRead::begin(&mut windows, &h, &strategy),
        Err(ObservationError::View(ViewError::Capacity))
    ));
    assert_eq!(windows.usage(), before);
    assert_eq!(
        windows.window_status(&h).unwrap().state,
        ViewState::Incomplete(ViewFailure::Capacity)
    );
}
