use super::*;
use crate::{
    adapters::acp::{
        Incoming, Response, ResponseDeadline, Version, WriteReport,
        raw_json::RawDocument,
        test_support::{Allow, setup},
    },
    application::shared_views::{RetainedRaw, SharedViewOwner},
    domain::{
        EntityId,
        native::{NativeSessionKey, NativeSourceId},
        shared_projection::{FieldKnowledge, StructuredItemKind, ToolField},
    },
};
use std::{num::NonZeroUsize, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    time::{Instant, timeout_at},
};

#[allow(
    clippy::too_many_lines,
    reason = "One causal actual source flow checks routing/backfill and version-specific original-byte field evidence"
)]
async fn exercise(version: Version) {
    let deadline = || Instant::now() + Duration::from_secs(10);
    let (node, peer) = tokio::io::duplex(128 * 1024);
    let (nr, nw) = tokio::io::split(node);
    let (pr, mut pw) = tokio::io::split(peer);
    let mut pr = BufReader::new(pr);
    let (connection, mut received, io) = Connection::spawn(nr, nw).expect("actual one connection");
    let (mut capture, consumer, probe) =
        super::super::shared_view_capture::AcpViewCapture::for_connection(&connection);
    let source = NativeSourceId(EntityId::from_bytes([71; 16]));
    let owner = SharedViewOwner::start_source(
        connection.scope(),
        source,
        consumer,
        probe.clone(),
        Arc::new(AcpViewProjector::new(connection.clone())),
        NonZeroUsize::new(64).expect("fair work batch"),
        tokio::runtime::Handle::current(),
    )
    .expect("registered source worker");
    let views = owner.handle();
    assert_eq!(views.core.scope, connection.scope());
    let init = connection
        .reserve_initialize(version, deadline())
        .expect("actual reserve");
    let id = init.id().expect("rpc").clone();
    let frame = init.encode().expect("frame");
    let ticket = connection
        .enqueue(init, frame, Arc::new(Allow), deadline())
        .expect("actual init enqueue");
    let mut sent = Vec::new();
    pr.read_until(b'\n', &mut sent)
        .await
        .expect("actual peer receives");
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    id.write_json(&mut response).expect("typed rpc");
    response.extend_from_slice(match version {Version::V1=>br#","result":{"protocolVersion":1,"agentInfo":{"name":"fixture","version":"1"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}}}}"#,Version::V2=>br#","result":{"protocolVersion":2,"info":{"name":"fixture","version":"2"},"capabilities":{"session":{}}}}"#});
    response.push(b'\n');
    pw.write_all(&response)
        .await
        .expect("real negotiated response");
    let event = timeout_at(deadline(), received.next())
        .await
        .expect("wait")
        .expect("parse")
        .expect("real init");
    capture
        .capture_received(&event)
        .expect("same source retain");
    assert!(matches!(ticket.wait().await, WriteReport::Written));
    let long_id = "x".repeat(32769);
    let first = format!(
        r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"s","update":{{"sessionUpdate":"tool_call_update","toolCallId":"{long_id}","name":"before","title":"original","rawInput":123456789012345678901234567890,"content":[{{"type":"text","text":"first"}}],"_meta":{{"future":1e999999}}}}}}}}"#
    );
    pw.write_all(first.as_bytes())
        .await
        .expect("long identity wire");
    pw.write_all(b"\n").await.expect("LF");
    let event = timeout_at(deadline(), received.next())
        .await
        .expect("wait")
        .expect("parse")
        .expect("real update");
    capture
        .capture_received(&event)
        .expect("capture before target bind");
    let foreign=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"other","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"foreign"}}}}"#;
    pw.write_all(foreign)
        .await
        .expect("other target actual wire");
    pw.write_all(b"\n").await.expect("LF");
    let event = received
        .next()
        .await
        .expect("parse")
        .expect("actual foreign target");
    capture
        .capture_received(&event)
        .expect("source retains unassigned");
    let session = SessionId::parse(
        RawDocument::parse(br#""s""#.to_vec())
            .expect("session")
            .root(),
    )
    .expect("id");
    let reservation = connection
        .reserve_request(
            connection
                .strategy()
                .expect("negotiated")
                .resume(session, &setup(version, "C:\\fixture"))
                .expect("actual Resume"),
            ResponseDeadline::Unbounded,
        )
        .expect("actual scope/rpc");
    let key = NativeSessionKey::from_config(Some(source.0), "s").expect("configured namespace");
    let (version_route, cut) = capture
        .bind_module_reservation(&reservation, key.clone(), None)
        .expect("real reservation bind, no Loaded");
    assert_eq!(cut, 3);
    let rpc = reservation.id().expect("actual Resume RPC").clone();
    let frame = reservation.encode().expect("encode");
    let ticket = connection
        .enqueue(reservation, frame, Arc::new(Allow), deadline())
        .expect("enqueue");
    sent.clear();
    pr.read_until(b'\n', &mut sent)
        .await
        .expect("real resume bytes");
    if version == Version::V2 {
        pw.write_all(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"state_update","state":"idle"}}}"#).await.expect("mandatory state");
        pw.write_all(b"\n").await.expect("LF");
        let event = received
            .next()
            .await
            .expect("parse")
            .expect("actual snapshot");
        capture.capture_received(&event).expect("retain");
    }
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    rpc.write_json(&mut response).expect("original rpc");
    response.extend_from_slice(b",\"result\":{}}\n");
    pw.write_all(&response).await.expect("real response");
    let event = received
        .next()
        .await
        .expect("parse")
        .expect("actual resume response");
    assert!(matches!(
        event.incoming(),
        Incoming::Response {
            response: Response::Resumed { .. },
            ..
        }
    ));
    capture.capture_received(&event).expect("capture");
    assert!(matches!(ticket.wait().await, WriteReport::Written));
    timeout_at(
        deadline(),
        probe.wait_barrier(probe.retained_frontier(), version_route),
    )
    .await
    .expect("no later output needed")
    .expect("actual processed cut and route");
    let generation = views.core.target(&key).expect("bound actual target");
    let old = Arc::clone(&*generation.snapshot.read().expect("snapshot"));
    assert!(
        old.descriptor.history_incomplete(),
        "Resume is not standard history completion"
    );
    let item = old.projection.item(0).expect("first tool anchor");
    assert_eq!(item.kind(), StructuredItemKind::Tool);
    assert_eq!(
        item.field(ToolField::Name).knowledge(),
        FieldKnowledge::Known
    );
    let field = item
        .field(ToolField::RawInput)
        .raw_reference()
        .expect("original numeric fragment");
    let original = old
        .raw
        .get(usize::try_from(field.record().as_u64() - 1).expect("index"))
        .expect("same original record");
    assert_eq!(
        original.raw.bytes()[field.byte_start()..field.byte_end()],
        *b"123456789012345678901234567890"
    );
    assert!(
        original.raw.bytes() == first.as_bytes(),
        "raw lexical preservation"
    );
    let replacement = format!(
        r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"s","update":{{"sessionUpdate":"tool_call_update","toolCallId":"{long_id}","name":null,"_meta":null,"rawInput":null,"status":"completed"}}}}}}"#
    );
    for body in [
        br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"same"}}}}"#.as_slice(),
        br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"same"}}}}"#.as_slice(),
        replacement.as_bytes(),
    ] {pw.write_all(body).await.expect("actual update bytes");pw.write_all(b"\n").await.expect("LF");let event=received.next().await.expect("parse").expect("real received");capture.capture_received(&event).expect("retain");}
    probe
        .wait_processed(probe.retained_frontier())
        .await
        .expect("worker actual raw cut");
    let current = Arc::clone(&*generation.snapshot.read().expect("current"));
    assert_eq!(
        current.projection.anchors.len(),
        if version == Version::V1 { 3 } else { 4 },
        "same tool remains at first anchor, unidentified equal messages stay distinct"
    );
    let latest = current
        .projection
        .item(0)
        .expect("first original tool position");
    assert_eq!(
        old.projection
            .item(0)
            .expect("immutable H")
            .field(ToolField::Name)
            .knowledge(),
        FieldKnowledge::Known
    );
    assert_eq!(
        latest.field(ToolField::Name).knowledge(),
        if version == Version::V1 {
            FieldKnowledge::Known
        } else {
            FieldKnowledge::KnownAbsent
        }
    );
    assert_eq!(
        latest.field(ToolField::Metadata).knowledge(),
        if version == Version::V1 {
            FieldKnowledge::Uninterpreted
        } else {
            FieldKnowledge::KnownEmpty
        }
    );
    assert_eq!(
        latest.field(ToolField::RawInput).knowledge(),
        if version == Version::V1 {
            FieldKnowledge::Deferred
        } else {
            FieldKnowledge::KnownAbsent
        }
    );
    assert_eq!(
        latest.field(ToolField::Title).raw_reference(),
        item.field(ToolField::Title).raw_reference(),
        "omitted title keeps original evidence"
    );
    for index in 0..current.raw.len() {
        let record = current.raw.get(index).expect("source raw");
        if record.envelope.session.as_deref() == Some("other") {
            assert_eq!(
                *record.assignment.lock().expect("assignment"),
                None,
                "unmanaged target not broadcast into s"
            );
        }
    }
    let session = SessionId::parse(
        RawDocument::parse(br#""s""#.to_vec())
            .expect("session")
            .root(),
    )
    .expect("id");
    let reservation = connection
        .reserve_request(
            connection
                .strategy()
                .expect("same actual strategy")
                .replay(session, &setup(version, "C:\\fixture"))
                .expect("actual Replay"),
            ResponseDeadline::Unbounded,
        )
        .expect("actual original Replay");
    let read =
        crate::domain::shared_view::ReadEpoch::fresh().expect("new actual read operation epoch");
    let mut facts = capture
        .track_replay(&reservation, read)
        .expect("same reservation brand");
    let _ = capture
        .bind_module_reservation(&reservation, key.clone(), Some(read))
        .expect("candidate FIFO marker before enqueue");
    let rpc = reservation.id().expect("actual Replay rpc").clone();
    let frame = reservation.encode().expect("real replay frame");
    let ticket = connection
        .enqueue(reservation, frame, Arc::new(Allow), deadline())
        .expect("owned actual enqueue");
    sent.clear();
    pr.read_until(b'\n', &mut sent)
        .await
        .expect("real peer receives replay");
    let replay_update = format!(
        r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"s","update":{{"sessionUpdate":"tool_call_update","toolCallId":"{long_id}","status":"completed"}}}}}}"#
    );
    pw.write_all(replay_update.as_bytes())
        .await
        .expect("same literal ID in new read");
    pw.write_all(b"\n").await.expect("LF");
    let event = received
        .next()
        .await
        .expect("actual parse")
        .expect("actual read update");
    capture
        .capture_received(&event)
        .expect("capture candidate original");
    if version == Version::V2 {
        pw.write_all(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"replay_complete"}}}"#).await.expect("replay boundary before mandatory current state");
        pw.write_all(b"\n").await.expect("LF");
        let marker = received
            .next()
            .await
            .expect("actual marker parse")
            .expect("actual marker");
        capture.capture_received(&marker).expect("capture marker");
        assert!(
            !facts
                .observe_received(&marker)
                .expect("marker cannot complete RPC")
        );
        pw.write_all(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"state_update","state":"idle"}}}"#).await.expect("mandatory native state");
        pw.write_all(b"\n").await.expect("LF");
        let event = received
            .next()
            .await
            .expect("actual state parse")
            .expect("actual state");
        capture.capture_received(&event).expect("capture state");
        assert!(
            !facts
                .observe_received(&event)
                .expect("state cannot complete read")
        );
    }
    probe
        .wait_barrier(probe.retained_frontier(), probe.route_version())
        .await
        .expect("candidate data processed");
    assert_eq!(
        views
            .core
            .target(&key)
            .expect("still shared old view")
            .send
            .epoch(),
        generation.send.epoch(),
        "refresh does not erase visible prior output while revalidating"
    );
    assert_eq!(
        generation
            .snapshot
            .read()
            .expect("old visible")
            .descriptor
            .freshness(),
        crate::domain::shared_view::Freshness::Revalidating
    );
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    rpc.write_json(&mut response).expect("original replay rpc");
    response.extend_from_slice(b",\"result\":{}}\n");
    pw.write_all(&response)
        .await
        .expect("actual complete response");
    let event = received
        .next()
        .await
        .expect("actual response parse")
        .expect("actual response");
    capture
        .capture_received(&event)
        .expect("retain original response");
    assert!(facts.observe_received(&event).expect("true Replay receipt"));
    let version_complete = capture
        .complete_module_read(key.clone(), &facts)
        .expect("true completion marker, no fake flags");
    probe
        .wait_barrier(event.sequence(), version_complete)
        .await
        .expect("all prior raw and completion marker processed");
    let replacement = views
        .core
        .target(&key)
        .expect("promoted same canonical target");
    assert_ne!(replacement.send.epoch(), generation.send.epoch());
    assert!(
        !generation
            .send
            .current()
            .expect("old cursor send epoch sealed")
    );
    let replacement_snapshot = Arc::clone(&*replacement.snapshot.read().expect("new current"));
    assert_eq!(
        replacement_snapshot.descriptor.read_mode(),
        crate::domain::shared_view::ReadMode::ReplayComplete
    );
    assert_eq!(
        replacement_snapshot.descriptor.join(),
        crate::domain::shared_view::JoinState::RebuiltNoStableIds
    );
    assert_eq!(
        replacement_snapshot
            .projection
            .item(0)
            .expect("new read anchor")
            .field(ToolField::Title)
            .knowledge(),
        FieldKnowledge::Unknown,
        "equal IDs do not silently import fields from a previous unproven load epoch"
    );
    assert!(matches!(ticket.wait().await, WriteReport::Written));
    let report = owner.shutdown(deadline()).await;
    assert!(report.worker_joined() && report.cleanup_joined());
    assert_eq!(report.failure(), None);
    assert!(
        !connection.is_closed(),
        "view closure never closes native IO"
    );
    assert!(
        io.shutdown(deadline()).await.joined,
        "original native IO separately joined"
    );
}
#[tokio::test]
async fn shared_view_source_v1_real_routing_long_identity_nulls_and_fixed_h() {
    exercise(Version::V1).await;
}
#[tokio::test]
async fn shared_view_source_v2_real_routing_long_identity_nulls_and_fixed_h() {
    exercise(Version::V2).await;
}

#[test]
fn opaque_duplicates_remain_projectable_but_queried_ambiguity_fails() {
    for version in [Version::V1, Version::V2] {
        let original = br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"t","rawInput":{"x":1,"\u0078":2},"unknown":{"y":1,"y":2}}}}"#;
        let raw = RawMessage::parse(original.to_vec()).unwrap();
        let projected = envelope(&raw, version).unwrap();
        assert!(projected.failure.is_none());
        assert_eq!(projected.projection, ProjectionState::Available);
        let FieldChange::Set {
            span: Some((start, end)),
            ..
        } = projected.fields[ToolField::RawInput as usize]
        else {
            panic!("raw input retains its source span")
        };
        assert_eq!(&raw.bytes()[start..end], br#"{"x":1,"\u0078":2}"#);
        assert_eq!(raw.bytes(), original);
        let duplicate = String::from_utf8(original.to_vec()).unwrap().replace(
            r#""toolCallId":"t""#,
            r#""toolCallId":"t","\u0074oolCallId":"other""#,
        );
        let Err((session, error)) =
            envelope(&RawMessage::parse(duplicate.into_bytes()).unwrap(), version)
        else {
            panic!("queried tool identity ambiguity must fail")
        };
        assert_eq!(session.as_deref(), Some("s"));
        assert_eq!(error, ViewError::ProjectionFailure);
    }
    let duplicate_type = RawMessage::parse(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call_content_chunk","toolCallId":"t","content":{"type":"text","\u0074ype":"image","text":"x"}}}}"#.to_vec()).unwrap();
    let projected = envelope(&duplicate_type, Version::V2).unwrap();
    assert_eq!(projected.failure, Some(ViewError::ProjectionFailure));
    assert_eq!(projected.projection, ProjectionState::ProjectionUnavailable);
}
