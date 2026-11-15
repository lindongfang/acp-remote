use super::*;
use crate::adapters::acp::{EventReceiver, IoOwner, Version, WriteReport, test_support::Allow};
use crate::application::shared_views::{
    budget::*,
    journal::{ContentIdentity, MemoryJournal, TRANSFER_CHUNK_BYTES},
};
use crate::domain::{EntityId, native::NativeSessionKey, shared_view::ReadEpoch};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
    time::{Instant, timeout},
};

type Peer = (BufReader<ReadHalf<DuplexStream>>, WriteHalf<DuplexStream>);
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
async fn ready() -> (
    Connection,
    EventReceiver,
    IoOwner,
    Peer,
    AcpViewCapture,
    SourceConsumer<RetainedAcpRaw>,
    IngressProbe<RetainedAcpRaw>,
) {
    let (a, b) = tokio::io::duplex(128 * 1024);
    let (ar, aw) = tokio::io::split(a);
    let (br, bw) = tokio::io::split(b);
    let (connection, mut rx, owner) = Connection::spawn(ar, aw).expect("real connection");
    let (mut capture, consumer, probe) = AcpViewCapture::for_connection(&connection);
    let reserve = connection
        .reserve_initialize(Version::V1, deadline())
        .expect("reserve initialize");
    let mut id = Vec::new();
    reserve
        .id()
        .expect("typed RPC")
        .write_json(&mut id)
        .expect("id JSON");
    let frame = reserve.encode().expect("frame");
    let ticket = connection
        .enqueue(reserve, frame, Arc::new(Allow), deadline())
        .expect("enqueue");
    let mut peer = (BufReader::new(br), bw);
    let mut sent = Vec::new();
    peer.0
        .read_until(b'\n', &mut sent)
        .await
        .expect("actual peer received");
    assert!(!sent.is_empty());
    let mut reply=br#"{"jsonrpc":"2.0","id":REPLACE,"result":{"protocolVersion":1,"agentInfo":{"name":"fixture","version":"1"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}}}}"#.to_vec();
    let text = String::from_utf8(reply)
        .expect("fixture UTF8")
        .replace("REPLACE", std::str::from_utf8(&id).expect("id UTF8"));
    reply = text.into_bytes();
    reply.push(b'\n');
    peer.1.write_all(&reply).await.expect("reply");
    let received = timeout(Duration::from_secs(5), rx.next())
        .await
        .expect("receipt timeout")
        .expect("receive")
        .expect("real init");
    capture.capture_received(&received).expect("retain init");
    assert!(matches!(ticket.wait().await, WriteReport::Written));
    (connection, rx, owner, peer, capture, consumer, probe)
}
async fn emit(rx: &mut EventReceiver, peer: &mut Peer, capture: &mut AcpViewCapture, raw: &[u8]) {
    peer.1
        .write_all(raw)
        .await
        .expect("peer writes original bytes");
    peer.1.write_all(b"\n").await.expect("LF");
    let event = timeout(Duration::from_secs(30), rx.next())
        .await
        .expect("reader timeout")
        .expect("receive")
        .expect("real Received");
    capture.capture_received(&event).expect("retain raw");
}
fn raw(raw: &'static [u8]) -> RetainedAcpRaw {
    RetainedAcpRaw {
        message: RawMessage::parse(raw.to_vec()).expect("validated fixture"),
        observation:
            crate::application::shared_views::read_observation::ReadObservation::unavailable(),
    }
}

#[tokio::test]
async fn shared_view_real_receipt_windows_reject_same_sid_and_keep_different_sid_distinct() {
    use crate::adapters::acp::{
        AcpError, ResponseDeadline,
        test_support::{parse_session, setup},
        transport::{ViewReadContextKind, ViewReadWindowKind},
    };
    let (connection, mut events, owner, mut peer, mut capture, _consumer, _probe) = ready().await;
    let strategy = connection.strategy().expect("real negotiated capability");
    let update_a=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"a","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"same"}}}}"#;
    peer.1
        .write_all(update_a)
        .await
        .expect("actual early update");
    peer.1.write_all(b"\n").await.expect("LF");
    let live = timeout(deadline() - Instant::now(), events.next())
        .await
        .expect("wait")
        .expect("read")
        .expect("actual live");
    assert_eq!(
        live.view_read_context(),
        ViewReadContextKind::OutsideReadWindow
    );
    assert!(live.view_read_window().is_none());
    capture
        .capture_received(&live)
        .expect("capture original early receipt");
    let a = parse_session(&serde_json::json!("a")).expect("actual protocol session");
    let b = parse_session(&serde_json::json!("b")).expect("actual protocol session");
    let config = setup(Version::V1, "C:\\fixture");
    let reserved = connection
        .reserve_request(
            strategy.replay(a.clone(), &config).expect("request"),
            ResponseDeadline::Unbounded,
        )
        .expect("reserve A");
    let rpc_a = reserved.id().expect("original RPC").clone();
    let encoded = reserved.encode().expect("encode");
    let ticket_a = connection
        .enqueue(reserved, encoded, Arc::new(Allow), deadline())
        .expect("enqueue A");
    let mut request_bytes = Vec::new();
    peer.0
        .read_until(b'\n', &mut request_bytes)
        .await
        .expect("peer A");
    assert_eq!(ticket_a.wait().await, WriteReport::Written);
    for request in [
        strategy.replay(a.clone(), &config).expect("replay"),
        strategy.resume(a, &config).expect("resume"),
    ] {
        assert!(
            matches!(
                connection.reserve_request(request, ResponseDeadline::Unbounded),
                Err(AcpError::SessionReadBusy)
            ),
            "same SID is rejected before another cell exists"
        );
    }
    let reserved = connection
        .reserve_request(
            strategy.replay(b, &config).expect("request"),
            ResponseDeadline::Unbounded,
        )
        .expect("different SID accepted");
    let rpc_b = reserved.id().expect("original RPC B").clone();
    let encoded = reserved.encode().expect("encode");
    let ticket_b = connection
        .enqueue(reserved, encoded, Arc::new(Allow), deadline())
        .expect("enqueue B");
    request_bytes.clear();
    peer.0
        .read_until(b'\n', &mut request_bytes)
        .await
        .expect("peer B");
    assert_eq!(ticket_b.wait().await, WriteReport::Written);
    for (body,rpc,sid) in [(update_a.as_slice(),&rpc_a,"a"),(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"b","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"same"}}}}"#.as_slice(),&rpc_b,"b")] {
        peer.1.write_all(body).await.expect("actual wire update");peer.1.write_all(b"\n").await.expect("LF");
        let received=timeout(deadline()-Instant::now(),events.next()).await.expect("wait").expect("read").expect("actual receipt");
        assert_eq!(received.view_read_context(),ViewReadContextKind::UniqueReceiptWindow);
        let window=received.view_read_window().expect("unique original cut");
        assert_eq!(window.scope(),connection.scope());assert_eq!(window.rpc(),rpc);
        assert_eq!(window.session().as_str(),sid);assert_eq!(window.kind(),ViewReadWindowKind::Replay);
        capture.capture_received(&received).expect("same original retained allocation");
        let retained=RetainedAcpRaw::from_received(&received);
        let observed=retained.observation();let actual=observed.window.expect("equivalent fixed inner context");
        assert_eq!(actual.session.as_ref(),sid);assert!(matches!(actual.kind,crate::application::shared_views::read_observation::ReadWindowKind::Replay));
    }
    assert!(
        owner.shutdown(deadline()).await.joined,
        "original IO joins with original pending tickets retained"
    );
}

fn collect<R: RetainedRaw>(
    journal: &MemoryJournal<R>,
    record: crate::domain::shared_view::RawRecordId,
) -> Vec<u8> {
    let snapshot = journal.snapshot().expect("snapshot");
    let length = journal.raw_metadata(record).expect("metadata").byte_length;
    let mut all = Vec::new();
    while all.len() < length {
        let part = journal
            .copy_raw_chunk(snapshot, record, all.len())
            .expect("chunk");
        assert!(!part.is_empty() && part.len() <= TRANSFER_CHUNK_BYTES);
        all.extend(part);
    }
    all
}

#[tokio::test]
async fn first_open_without_later_output_retains_exact_unknown_bytes() {
    let (_c, mut rx, owner, mut peer, mut capture, mut consumer, probe) = ready().await;
    let original=br#"{"jsonrpc":"2.0","method":"future","params":{"huge":123456789012345678901234567890123456789,"exponent":1e999999,"order":{"z":null,"a":[true,"\u0041"]}}}"#;
    emit(&mut rx, &mut peer, &mut capture, original).await;
    let cut = probe.retained_frontier();
    assert_eq!(cut, 2);
    assert_eq!(probe.processed_frontier(), 0);
    let mut journal = MemoryJournal::new().expect("journal");
    let report = consumer.drain_into(&mut journal).expect("detach");
    assert_eq!(report.processed, 2);
    timeout(Duration::from_secs(1), probe.wait_processed(cut))
        .await
        .expect("no later output needed")
        .expect("processed");
    let id = journal.raw_metadata_at(1).expect("second raw").id;
    assert_eq!(collect(&journal, id), original);
    assert!(owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn foreign_real_connection_is_rejected_without_stopping_control() {
    let (ca, _ra, oa, pa, mut a, _da, probe) = ready().await;
    let (cb, mut rb, ob, mut pb, mut b, _db, _) = ready().await;
    assert_ne!(ca.scope(), cb.scope());
    let body = br#"{"jsonrpc":"2.0","method":"future","params":{}}"#;
    pb.1.write_all(body).await.expect("write");
    pb.1.write_all(b"\n").await.expect("LF");
    let event = rb.next().await.expect("read").expect("actual B event");
    assert_eq!(a.capture_received(&event), Err(ViewError::InvalidScope));
    b.capture_received(&event).expect("B keeps own event");
    assert_eq!(probe.failure(), Some(ViewError::InvalidScope));
    assert!(!ca.is_closed());
    drop(pa);
    drop(pb);
    assert!(oa.shutdown(deadline()).await.joined);
    assert!(ob.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn slow_consumer_exceeds_old_item_count_and_protocol_control_keeps_progressing() {
    let (c, mut rx, owner, mut peer, mut capture, mut consumer, probe) = ready().await;
    let body =
        br#"{"jsonrpc":"2.0","method":"future","params":{"content":"same text is not identity"}}"#;
    for _ in 0..4097 {
        emit(&mut rx, &mut peer, &mut capture, body).await;
    }
    assert_eq!(probe.retained_frontier(), 4098);
    assert_eq!(probe.processed_frontier(), 0);
    let session =
        super::super::test_support::parse_session(&serde_json::json!("session")).expect("session");
    let cancel = c
        .reserve_cancel(
            session,
            super::super::ExtensionFields::empty().expect("extensions"),
        )
        .expect("control");
    let encoded = cancel.encode().expect("encode cancel");
    let ticket = c
        .enqueue(cancel, encoded, Arc::new(Allow), deadline())
        .expect("control enqueue");
    let mut line = String::new();
    timeout(Duration::from_secs(2), peer.0.read_line(&mut line))
        .await
        .expect("control continues")
        .expect("read cancel");
    assert!(line.contains("session/cancel"));
    assert!(matches!(ticket.wait().await, WriteReport::Written));
    let mut journal = MemoryJournal::new().expect("journal");
    consumer.drain_into(&mut journal).expect("drain");
    assert_eq!(journal.raw_count(), 4098);
    assert!(probe.failure().is_none());
    assert!(owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn actual_frame_above_old_32mib_limit_is_retained_and_chunked() {
    let (_c, mut rx, owner, mut peer, mut capture, mut consumer, _) = ready().await;
    let mut body = br#"{"jsonrpc":"2.0","method":"future","params":{"text":""#.to_vec();
    body.resize(body.len() + 32 * 1024 * 1024 + 1, b'x');
    body.extend_from_slice(br#""}}"#);
    emit(&mut rx, &mut peer, &mut capture, &body).await;
    let mut journal = MemoryJournal::new().expect("journal");
    consumer.drain_into(&mut journal).expect("drain");
    let id = journal.raw_metadata_at(1).expect("large original").id;
    let snapshot = journal.snapshot().expect("snapshot");
    let mut offset = 0;
    let mut hash = Sha256::new();
    while offset < body.len() {
        let chunk = journal
            .copy_raw_chunk(snapshot, id, offset)
            .expect("bounded processing chunk");
        assert!(chunk.len() <= TRANSFER_CHUNK_BYTES);
        offset += chunk.len();
        hash.update(chunk);
    }
    assert_eq!(offset, body.len());
    assert_eq!(hash.finalize(), Sha256::digest(&body));
    assert!(owner.shutdown(deadline()).await.joined);
}

#[test]
fn immutable_snapshot_keeps_old_tool_version_and_original_anchor_position() {
    let mut j = MemoryJournal::new().expect("view");
    let first = j.append_raw(raw(br#"{"tool":1}"#), 1, 9).expect("raw");
    let anchor = j
        .project(first, Some(ContentIdentity::Tool("tool".into())))
        .expect("first tool");
    let text = j
        .append_raw(raw(br#"{"text":"after tool"}"#), 2, 9)
        .expect("text");
    j.project(text, None).expect("text chunk");
    let old = j.snapshot().expect("H");
    let last = j
        .append_raw(raw(br#"{"tool":2}"#), 3, 10)
        .expect("late tool");
    assert_eq!(
        j.project(last, Some(ContentIdentity::Tool("tool".into())))
            .expect("same identity"),
        anchor
    );
    assert_eq!(j.item_at(old, 0).expect("old H").raw, first);
    assert_eq!(
        j.item_at(j.snapshot().expect("new H"), 0)
            .expect("new tool")
            .raw,
        last
    );
    assert_eq!(
        j.item_at(j.snapshot().expect("new H"), 1)
            .expect("text position")
            .raw,
        text
    );
    assert_eq!(j.anchor_count(), 2);
    j.seal();
    assert_eq!(j.item_at(old, 0), Err(ViewError::StaleCursor));
}

#[test]
fn identity_namespaces_and_unidentified_chunks_do_not_text_deduplicate() {
    let mut j = MemoryJournal::new().expect("journal");
    for identity in [
        Some(ContentIdentity::Tool("same".into())),
        Some(ContentIdentity::Message("same".into())),
        None,
        None,
    ] {
        let id = j.append_raw(raw(br#"{"same":"text"}"#), 1, 1).expect("raw");
        j.project(id, identity).expect("anchor");
    }
    assert_eq!(j.anchor_count(), 4);
}

#[tokio::test]
async fn provider_samples_new_operations_without_reusing_service_start_instant() {
    let key = NativeSessionKey::from_config(Some(EntityId::from_bytes([7; 16])), "native")
        .expect("configured key");
    let provider =
        MonotonicReadOptionsProvider::new(Duration::from_millis(1), ReadResponseBudget::Unbounded);
    let first = NativeReadStartContext::sample(
        &key,
        ReadReason::FirstOpen,
        ReadEpoch::fresh().expect("read epoch"),
    );
    let selected = provider.for_new_operation(&first).expect("first options");
    tokio::time::sleep(Duration::from_millis(4)).await;
    let second = NativeReadStartContext::sample(
        &key,
        ReadReason::ExplicitRefresh,
        ReadEpoch::fresh().expect("second epoch"),
    );
    let newer = provider.for_new_operation(&second).expect("fresh options");
    assert!(newer.write_deadline > selected.write_deadline);
    assert!(newer.write_deadline > second.now());
    assert_eq!(newer.response_wait, ReadResponseWait::Unbounded);
}
