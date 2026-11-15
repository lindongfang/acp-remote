use super::*;
use crate::{
    adapters::acp::{Version, raw_json::RawDocument, test_support::setup},
    application::shared_views::{
        CurrentViewAuthority,
        reader::PrepareNext,
        wire::{HEADER_BYTES, RecordKind, TransferHeader},
    },
    domain::EntityId,
    shared_view_test_support::{FixtureSigningClient, FixtureTaskOwner, OwnedTestArea},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(super) async fn reply(
    peer: &mut crate::shared_view_test_support::FixturePeer,
    result: &[u8],
) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let bytes = peer
        .next_node_frame(deadline)
        .await
        .expect("actual request IO")
        .expect("LF frame");
    let document = RawDocument::parse(bytes.clone()).expect("actual request");
    let id = document
        .root()
        .object()
        .expect("request object")
        .get("id")
        .expect("unique decoded field")
        .expect("actual id");
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    response.extend_from_slice(id.fragment().as_bytes());
    response.extend_from_slice(b",\"result\":");
    response.extend_from_slice(result);
    response.extend_from_slice(b"}\n");
    peer.write_bytes(&response, deadline)
        .await
        .expect("actual peer response");
    bytes
}
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "One original ACP peer and authenticated Node owner span the causal create, replay, rebind, and cleanup scenario"
)]
async fn shared_view_module_real_create_early_sid_backfill_authenticated_fixed_h_and_tail() {
    let area = OwnedTestArea::create(&std::env::temp_dir(), "shared-view-module")
        .expect("owned unique child");
    let cert =
        rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("synthetic TLS");
    area.install_tls(
        cert.cert.pem().as_bytes(),
        cert.signing_key.serialize_pem().as_bytes(),
    )
    .expect("actual create_new");
    let resource_owner = FixtureTaskOwner::new(&area, tokio::runtime::Handle::current())
        .expect("registered resources");
    let auth = Arc::new(
        LocalViewAuthFixture::start(area)
            .await
            .expect("actual Node"),
    );
    let signer = FixtureSigningClient::generate().expect("synthetic signing client");
    let client = auth
        .authenticate(&signer)
        .await
        .expect("real PairClaim and AuthProof");
    let second_signer = FixtureSigningClient::generate().expect("second actual key");
    let second = auth
        .authenticate(&second_signer)
        .await
        .expect("second real client");
    let (wire, mut peer) =
        FixturePeerWire::from_duplex(&resource_owner.handle(), tokio::io::duplex(128 * 1024))
            .unwrap_or_else(|_| panic!("same original tuple"));
    let driver = tokio::spawn(async move {
        let request=reply(&mut peer,br#"{"protocolVersion":2,"info":{"name":"fixture","version":"2"},"capabilities":{"session":{}}}"#).await;
        assert!(
            String::from_utf8(request)
                .expect("request UTF8")
                .contains("initialize")
        );
        peer
    });
    let module = AcpViewModuleFixture::start(
        FixtureSourceConfig {
            source: NativeSourceId(EntityId::from_bytes([82; 16])),
            entry_alias: Arc::from("actual-module"),
        },
        wire,
        Arc::clone(&auth),
    )
    .await
    .expect("real initialized module");
    let mut peer = driver.await.expect("peer driver joined");
    let big = "z".repeat(70_001);
    let original=format!(r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"new-native","update":{{"sessionUpdate":"agent_message_chunk","content":{{"type":"future","value":123456789012345678901234567890,"body":"{big}"}}}}}}}}"#).into_bytes();
    let expected = original.clone();
    let driver = tokio::spawn(async move {
        let deadline = Instant::now() + Duration::from_secs(10);
        let request = peer
            .next_node_frame(deadline)
            .await
            .expect("new request IO")
            .expect("actual new frame");
        assert!(
            String::from_utf8(request.clone())
                .expect("UTF8")
                .contains("session/new")
        );
        let document = RawDocument::parse(request).expect("original request");
        let id = document
            .root()
            .object()
            .expect("object")
            .get("id")
            .expect("unique decoded field")
            .expect("id lookup");
        peer.write_bytes(&original, deadline)
            .await
            .expect("early update before returned native SID");
        peer.write_bytes(b"\n", deadline).await.expect("LF");
        let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
        response.extend_from_slice(id.fragment().as_bytes());
        response.extend_from_slice(b",\"result\":{\"sessionId\":\"new-native\"}}\n");
        peer.write_bytes(&response, deadline)
            .await
            .expect("true NewSession");
        peer
    });
    let target = module
        .create(setup(Version::V2, "C:\\fixture"))
        .await
        .expect("true response bind");
    assert_eq!(target.origin(), FixtureTargetOrigin::ModuleFixture);
    let mut peer = driver.await.expect("peer joined");
    let read = module
        .reads
        .open_current(&client, target.key())
        .await
        .expect("current authenticated cut/backfill");
    assert_eq!(
        read.descriptor().read_mode(),
        crate::domain::shared_view::ReadMode::CurrentOnly
    );
    assert!(
        module.reads.next_snapshot(&second, &read).await.is_err(),
        "same source never permits cross-client stream reuse"
    );
    let stream = read.stream();
    let (node, mut display) = tokio::io::duplex(128);
    let binding = auth
        .authority()
        .client_binding(&client)
        .expect("same opaque binding");
    let (mut writer, mut credits) = stream.bind_duplex(node, &binding).expect("one real peer");
    let deadline = || Instant::now() + Duration::from_secs(10);
    display
        .write_all(
            &TransferHeader::credit(stream.id(), 0)
                .encode(&[])
                .expect("credit codec"),
        )
        .await
        .expect("real initial credit");
    credits
        .read_credit(auth.authority().as_ref(), &client, target.key(), deadline())
        .await
        .expect("current-qualified initial credit");
    let mut reconstructed = Vec::new();
    loop {
        let chunk = match module
            .reads
            .next_snapshot(&client, &read)
            .await
            .expect("qualified chunk")
        {
            PrepareNext::Chunk(chunk) => chunk,
            PrepareNext::SnapshotFinished => break,
            _ => panic!("actual initial credit and matching prior ack"),
        };
        let authority = auth.authority();
        let send = writer.write_chunk(
            chunk.into_local(),
            authority.as_ref(),
            &client,
            target.key(),
            deadline(),
        );
        let receive = async {
            let mut header = [0; HEADER_BYTES];
            display.read_exact(&mut header).await.expect("real header");
            let length =
                u32::from_be_bytes(header[64..68].try_into().expect("fixed payload length"))
                    as usize;
            assert!(length <= 16 * 1024, "per-frame transport bound");
            let mut packet = header.to_vec();
            packet.resize(HEADER_BYTES + length, 0);
            display
                .read_exact(&mut packet[HEADER_BYTES..])
                .await
                .expect("actual opaque payload");
            let (metadata, payload) = TransferHeader::decode(&packet).expect("full real packet");
            (metadata, payload.to_vec())
        };
        let (flushed, (header, payload)) = tokio::join!(send, receive);
        flushed.expect("true local flush");
        if header.record_kind() == Some(RecordKind::OriginalRaw) {
            reconstructed.extend_from_slice(&payload);
        }
        display
            .write_all(
                &TransferHeader::credit(stream.id(), header.packet())
                    .encode(&[])
                    .expect("ack"),
            )
            .await
            .expect("real ack bytes");
        credits
            .read_credit(auth.authority().as_ref(), &client, target.key(), deadline())
            .await
            .expect("true flushed packet commit");
    }
    assert_eq!(
        reconstructed, expected,
        "all early source bytes including unknown number survive H and five chunks"
    );
    peer.write_bytes(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"new-native","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"tail"}}}}"#,deadline()).await.expect("real live delta");
    peer.write_bytes(b"\n", deadline()).await.expect("LF");
    let changed = timeout_at(deadline(), module.reads.wait_change(&client, &read))
        .await
        .expect("worker publication wakes idle authenticated reader")
        .expect("current view");
    assert!(changed.revision().as_u64() > read.descriptor().revision().as_u64());
    assert!(matches!(
        module
            .reads
            .next_delta(&client, &read)
            .await
            .expect("tail excludes initial H"),
        PrepareNext::Chunk(_)
    ));
    drop(read);
    let before_enqueues = module.core.enqueues.load(Ordering::Acquire);
    let read_a = module
        .reads
        .open(
            &client,
            target.key(),
            crate::application::shared_views::budget::ReadReason::FirstOpen,
        )
        .await
        .expect("first genuine read intent");
    let read_b = module
        .reads
        .open(
            &second,
            target.key(),
            crate::application::shared_views::budget::ReadReason::FirstOpen,
        )
        .await
        .expect("second client joins the same actual in-flight read");
    let request = peer
        .next_node_frame(deadline())
        .await
        .expect("real replay wire")
        .expect("one replay request");
    let document = RawDocument::parse(request).expect("actual read request");
    let object = document.root().object().expect("request");
    assert_eq!(
        object
            .get("method")
            .expect("unique decoded field")
            .expect("method")
            .string()
            .expect("text"),
        "session/resume"
    );
    assert_eq!(
        module.policy.calls.load(Ordering::Acquire),
        1,
        "one new operation selects once; second client joins without a provider call"
    );
    let id = object
        .get("id")
        .expect("unique decoded field")
        .expect("original Replay typed RPC");
    let replay_body=format!(r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"new-native","update":{{"sessionUpdate":"agent_message_chunk","content":{{"type":"future","n":99999999999999999999999999999999999999,"body":"{big}"}}}}}}}}"#).into_bytes();
    peer.write_bytes(&replay_body, deadline())
        .await
        .expect("true replay history");
    peer.write_bytes(b"\n", deadline()).await.expect("LF");
    peer.write_bytes(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"new-native","update":{"sessionUpdate":"state_update","state":"idle"}}}"#,deadline()).await.expect("mandatory real state snapshot");
    peer.write_bytes(b"\n", deadline()).await.expect("LF");
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    response.extend_from_slice(id.fragment().as_bytes());
    response.extend_from_slice(b",\"result\":{}}\n");
    peer.write_bytes(&response, deadline())
        .await
        .expect("real Replay completion");
    timeout_at(
        deadline(),
        module.reads.wait_original_completion(&client, &read_a),
    )
    .await
    .expect("original request subscription wakes")
    .expect("true same read completion");
    assert!(
        matches!(
            module.reads.next_snapshot(&client, &read_a).await,
            Err(ViewError::StaleCursor)
        ),
        "old H does not switch generation"
    );
    assert!(
        matches!(
            module.reads.rebind_same_read(&second, &read_a).await,
            Err(ViewError::InvalidClient)
        ),
        "another authenticated client cannot consume this original intent"
    );
    let rebound_a = module
        .reads
        .rebind_same_read(&client, &read_a)
        .await
        .expect("same actual completed operation");
    let (new_a, metadata_a) = rebound_a.into_parts();
    let rebound_b = module
        .reads
        .rebind_same_read(&second, &read_b)
        .await
        .expect("second client has independent same-operation intent");
    let (new_b, metadata_b) = rebound_b.into_parts();
    assert_ne!(new_a.stream_id(), read_a.stream_id());
    assert_ne!(new_a.stream_id(), new_b.stream_id());
    assert_eq!(metadata_a.read, metadata_b.read);
    assert_eq!(metadata_a.epoch, metadata_b.epoch);
    assert!(
        matches!(
            module.reads.rebind_same_read(&client, &read_a).await,
            Err(ViewError::InvalidStage)
        ),
        "original intent is consumed once"
    );
    assert_eq!(
        module.core.enqueues.load(Ordering::Acquire),
        before_enqueues + 1,
        "two rebinds enqueue no additional native request"
    );
    assert_eq!(
        module.policy.calls.load(Ordering::Acquire),
        1,
        "two rebinds do not renew the operation budget"
    );
    drop((read_a, read_b, new_a, new_b));
    let before = module.core.enqueues.load(Ordering::Acquire);
    let mut transfer = module
        .open_transfer(
            &client,
            &target,
            crate::application::shared_views::budget::ReadReason::FirstOpen,
        )
        .await
        .expect("registered actual display sender and credit receiver");
    let request = peer
        .next_node_frame(deadline())
        .await
        .expect("new external read really issues a new request")
        .expect("actual replay");
    let document = RawDocument::parse(request).expect("request");
    let id = document
        .root()
        .object()
        .expect("object")
        .get("id")
        .expect("unique decoded field")
        .expect("typed id");
    peer.write_bytes(&replay_body, deadline())
        .await
        .expect("same real large history");
    peer.write_bytes(b"\n", deadline()).await.expect("LF");
    peer.write_bytes(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"new-native","update":{"sessionUpdate":"state_update","state":"idle"}}}"#,deadline()).await.expect("state snapshot");
    peer.write_bytes(b"\n", deadline()).await.expect("LF");
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    response.extend_from_slice(id.fragment().as_bytes());
    response.extend_from_slice(b",\"result\":{}}\n");
    peer.write_bytes(&response, deadline())
        .await
        .expect("actual completion while prior display has not been drained");
    let views = module.views();
    loop {
        let changed = views.core.changed.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        let generation = views
            .core
            .target(target.key())
            .expect("actual current target");
        if generation.send.epoch() != metadata_a.epoch {
            break;
        }
        changed.await;
    }
    assert!(
        matches!(
            timeout_at(deadline(), transfer.next_record())
                .await
                .expect("stale wake"),
            Err(ViewError::StaleCursor)
        ),
        "old partial record is explicitly discarded"
    );
    let status = timeout_at(deadline(), transfer.next_record())
        .await
        .expect("bootstrap Credit0 precedes Status so no circular wait")
        .expect("true Status bytes");
    assert_eq!(status.address().kind(), RecordKind::Status);
    let status_json: serde_json::Value =
        serde_json::from_slice(status.bytes()).expect("platform-only Status JSON");
    assert_eq!(status_json["type"], "stream_rebound");
    assert!(
        !String::from_utf8_lossy(status.bytes()).contains("new-native"),
        "native SID never enters rebound metadata"
    );
    transfer
        .credit(1)
        .await
        .expect("real matching Status flush and peer credit");
    transfer
        .credit(1)
        .await
        .expect("duplicate actual credit is inert and does not grant a second permit");
    let segment = transfer
        .next_record()
        .await
        .expect("actual segment Status before body");
    assert_eq!(segment.address().kind(), RecordKind::Status);
    let segment_json: serde_json::Value =
        serde_json::from_slice(segment.bytes()).expect("platform segment metadata");
    assert_eq!(segment_json["type"], "view_segment");
    assert_eq!(segment_json["phase"], "start");
    transfer
        .credit(2)
        .await
        .expect("segment Status actual flush and credit");
    let large = timeout_at(deadline(), transfer.next_record())
        .await
        .expect("all chunks of the new H arrive")
        .expect("actual raw bytes");
    assert_eq!(large.address().kind(), RecordKind::OriginalRaw);
    assert_eq!(
        large.bytes(),
        replay_body,
        "new H never concatenates any old partial record"
    );
    let last = 2 + u64::try_from(replay_body.len().div_ceil(16 * 1024)).expect("packet count");
    transfer
        .credit(last)
        .await
        .expect("last actual raw chunk credit");
    let state = transfer
        .next_record()
        .await
        .expect("next actual original state record");
    assert_eq!(state.address().kind(), RecordKind::OriginalRaw);
    transfer
        .credit(last + 1)
        .await
        .expect("state record credit");
    let mut final_packet = last + 1;
    let mut projection_headers = 0;
    let mut evidence_records = 0;
    loop {
        let record = transfer
            .next_record()
            .await
            .expect("actual projection records then SnapshotEnd");
        final_packet += u64::try_from(record.bytes().len().div_ceil(16 * 1024).max(1))
            .expect("actual packet count");
        let end = record.address().kind() == RecordKind::SnapshotEnd;
        if record.address().kind() == RecordKind::Status {
            let metadata: serde_json::Value =
                serde_json::from_slice(record.bytes()).expect("platform-only segment end");
            assert_eq!(metadata["type"], "view_segment");
            assert_eq!(metadata["phase"], "end");
        } else if !end {
            assert_eq!(record.address().kind(), RecordKind::ItemProjection);
            let metadata: serde_json::Value =
                serde_json::from_slice(record.bytes()).expect("platform-only projection metadata");
            match metadata["type"].as_str().expect("platform record type") {
                "item_projection" => {
                    projection_headers += 1;
                    assert!(metadata["revision"].is_string());
                }
                "projection_evidence" => {
                    evidence_records += 1;
                    assert!(metadata["raw"]["rawRecordId"].is_string());
                }
                _ => panic!("declared projection record"),
            }
        }
        transfer
            .credit(final_packet)
            .await
            .expect("projection/End real flush and matching credit");
        if end {
            break;
        }
    }
    assert_eq!(
        projection_headers, 2,
        "message/state keep their actual anchors"
    );
    assert_eq!(
        evidence_records, 1,
        "unknown content remains an exact raw span"
    );
    assert_eq!(
        module.core.enqueues.load(Ordering::Acquire),
        before + 1,
        "Status and full new H continuation issue no extra RPC"
    );
    assert_eq!(
        module.policy.calls.load(Ordering::Acquire),
        2,
        "only the external open selected another operation budget"
    );
    let receipt = timeout_at(deadline(), transfer.detach())
        .await
        .expect("real Detach cleanup wakes")
        .expect("actual close receipt");
    assert_eq!(receipt.last_committed_packet(), final_packet);
    let report = module.shutdown(deadline()).await;
    assert!(
        report.io_joined() && report.pump_joined() && report.views_joined(),
        "actual original IO/pump/view/reader joins"
    );
    assert_eq!(report.failure(), None);
    assert!(
        resource_owner.shutdown(deadline()).await.all_joined(),
        "same resource registry releases before area cleanup"
    );
    auth.close()
        .await
        .expect("actual Node/SQL close and owned first cleanup");
}
