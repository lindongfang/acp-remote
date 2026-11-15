use super::*;
use crate::application::queue_owner::QueueRead;
use crate::domain::request_registry::{QueueOutcome, RequestFact};
use crate::node::queue_management::{NodeConsumeResult as Consumed, NodeQueueMutation as Mutation};
use crate::{
    application::shared_views::socket_authority::SocketViewClient,
    management_ws::input_upload::InputData,
    node::input_upload::{UploadOwner, UploadPhase, UploadPurpose},
};
use futures_util::FutureExt;
use std::{future::Future, panic::AssertUnwindSafe, pin::Pin};

const fn target(n: u8) -> crate::domain::EntityId {
    crate::domain::EntityId::from_bytes([n; 16])
}
fn frame(id: Id128, offset: u64, bytes: &[u8]) -> Vec<u8> {
    let mut wire = vec![0; 32];
    wire[..4].copy_from_slice(b"ACPI");
    wire[4] = 1;
    wire[5] = 1;
    wire[8..24].copy_from_slice(id.as_bytes());
    wire[24..32].copy_from_slice(&offset.to_be_bytes());
    wire.extend_from_slice(bytes);
    wire
}
struct Case {
    area: TestArea,
    owner: Option<NodeProcessOwner>,
    node: Option<NodeRuntime>,
    peers: Vec<ManagementSocket>,
    clients: Vec<SocketViewClient>,
    upload_owners: Vec<UploadOwner>,
    endpoints: Vec<crate::management_ws::DocumentSocketEndpoint>,
    document_receipt: Option<Arc<dyn crate::application::document_disposal::DocumentShutdownPort>>,
    client_id: String,
    registry: Option<crate::session_storage::request_registry::SqliteRequestRegistry>,
    pool: Option<sqlx::SqlitePool>,
}
impl Case {
    async fn start(&mut self) {
        self.owner = Some(NodeProcessOwner::begin(
            &tokio::runtime::Handle::current(),
            self.area.config(free_address()),
            Arc::new(FakeProtector::default()),
        ));
        self.node = Some(self.owner.as_ref().unwrap().ready().await.unwrap());
        let node = self.node.as_ref().unwrap();
        let storage = node.storage.as_ref().unwrap();
        self.registry = Some(storage.request_registry());
        self.pool = Some(storage.pool_for_test().clone());
        self.document_receipt = storage.document_shutdown_receipt();
        assert!(self.document_receipt.is_some());
        sqlx::query("INSERT INTO product_sources VALUES(?,'upload author fixture')")
            .bind(target(1).encode())
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        for n in [2, 3] {
            sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
                .bind(target(n).encode())
                .bind(target(1).encode())
                .bind(format!("upload-native-{n}"))
                .execute(storage.pool_for_test())
                .await
                .unwrap();
        }
        let key = SigningKey::from_slice(&[69; 32]).unwrap();
        self.client_id = pair_browser(node, &key, "Upload author fixture").await;
        for _ in 0..2 {
            let observed = node
                .management
                .as_ref()
                .unwrap()
                .observe_next_document_socket_for_test()
                .unwrap();
            let peer = valid_management_socket(&self.area, node.listen_addr()).await;
            let peer =
                authenticate_socket(peer, &key, &self.client_id, node_public_key(node)).await;
            self.peers.push(peer);
            let endpoint = observed.await.unwrap();
            self.clients.push(endpoint.client());
            self.endpoints.push(endpoint);
        }
        let other = SigningKey::from_slice(&[70; 32]).unwrap();
        let other_id = pair_browser(node, &other, "Queue other actor").await;
        let observed = node
            .management
            .as_ref()
            .unwrap()
            .observe_next_document_socket_for_test()
            .unwrap();
        let peer = valid_management_socket(&self.area, node.listen_addr()).await;
        self.peers
            .push(authenticate_socket(peer, &other, &other_id, node_public_key(node)).await);
        let endpoint = observed.await.unwrap();
        self.clients.push(endpoint.client());
        self.endpoints.push(endpoint);
        self.upload_owners = self
            .clients
            .iter()
            .map(|client| node.input_upload_owner(client).unwrap())
            .collect();
    }
    fn uploads(&self, socket: usize) -> UploadOwner {
        self.upload_owners[socket].clone()
    }
    async fn close_peers(&mut self) {
        for mut peer in self.peers.drain(..) {
            let _ = peer.close(None).await;
        }
    }
    async fn close(&mut self) -> (bool, bool, bool, bool) {
        self.close_peers().await;
        self.upload_owners.clear();
        self.endpoints.clear();
        let shutdown = if let Some(node) = self.node.take() {
            node.shutdown(None).await.is_ok()
        } else {
            true
        };
        let joined = if let Some(owner) = &self.owner {
            owner.join_actual().await.is_ok()
        } else {
            true
        };
        let registry = self
            .registry
            .as_ref()
            .is_none_or(|r| r.close_status().is_complete());
        let pool = self.pool.as_ref().is_none_or(|p| p.size() == 0);
        (shutdown, joined, registry, pool)
    }
}
async fn case(run: impl for<'a> FnOnce(&'a mut Case) -> Pin<Box<dyn Future<Output = ()> + 'a>>) {
    let mut fixture = Case {
        area: TestArea::new(),
        owner: None,
        node: None,
        peers: Vec::new(),
        clients: Vec::new(),
        upload_owners: Vec::new(),
        endpoints: Vec::new(),
        document_receipt: None,
        client_id: String::new(),
        registry: None,
        pool: None,
    };
    let result = AssertUnwindSafe(async {
        fixture.start().await;
        run(&mut fixture).await;
    })
    .catch_unwind()
    .await;
    let closed = fixture.close().await;
    let directory_free = crate::data_dir::DataDirectory::acquire(&fixture.area.data_dir()).is_ok();
    let disposal = fixture.document_receipt.as_ref().unwrap().observe();
    assert!(disposal.sealed && disposal.active_use == 0 && disposal.payload_roots == 0);
    assert_eq!(
        disposal.supervisor,
        crate::application::document_disposal::DocumentTaskJoinState::Joined(
            crate::application::document_disposal::DocumentTaskExit::Completed
        )
    );

    let mut database_free = true;
    {
        use std::os::windows::fs::OpenOptionsExt;
        for name in ["node.sqlite3", "node.sqlite3-wal", "node.sqlite3-shm"] {
            let path = fixture.area.data_dir().join(name);
            match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(path)
            {
                Ok(probe) => drop(probe),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && name != "node.sqlite3" => {}
                Err(_) => database_free = false,
            }
        }
    }
    assert!(
        database_free,
        "actual database exclusive probe after original join"
    );
    assert_eq!(
        closed,
        (true, true, true, true),
        "original closure receipts"
    );
    assert!(
        directory_free,
        "original directory lock released after join"
    );
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}
async fn ready(
    owner: &UploadOwner,
    client: &SocketViewClient,
    upload: Id128,
) -> crate::node::input_upload::UploadStatus {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status = owner.status(client, upload).await.unwrap();
            if matches!(status.phase(), UploadPhase::Ready | UploadPhase::Failed) {
                return status;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

async fn body(f: &Case, purpose: UploadPurpose, raw: &[u8]) -> Id128 {
    body_for(f, 0, purpose, raw).await
}
async fn body_for(f: &Case, socket: usize, purpose: UploadPurpose, raw: &[u8]) -> Id128 {
    let uploads = f.uploads(socket);
    let client = &f.clients[socket];
    let id = uploads
        .begin(client, target(2), purpose)
        .await
        .unwrap()
        .upload();
    let wire = frame(id, 0, raw);
    uploads
        .accept_data(client, InputData::decode_client(&wire).unwrap())
        .await
        .unwrap();
    uploads.finish(client, id, raw.len() as u64).await.unwrap();
    ready(&uploads, client, id).await.body().unwrap()
}
async fn queued(f: &Case, socket: usize) -> Vec<(crate::domain::EntityId, usize)> {
    f.node
        .as_ref()
        .unwrap()
        .session_services
        .as_ref()
        .unwrap()
        .queue
        .inspect_queue(f.clients[socket].context.actor(), target(2), |read| {
            let queue = match read {
                QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
            };
            queue
                .waiting()
                .map(|e| {
                    (
                        e.id(),
                        e.input().body.body().uploaded_bytes().as_ptr() as usize,
                    )
                })
                .collect()
        })
        .await
        .unwrap()
}
async fn peer_seal(f: &mut Case, socket: usize) {
    f.peers[socket].close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while f.clients[socket]
            .context
            .lifetime
            .with_live(|| Ok(()))
            .is_ok()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
struct ReleaseSqlGate(Arc<crate::session_storage::request_registry::test_hooks::Barrier>);
impl Drop for ReleaseSqlGate {
    fn drop(&mut self) {
        self.0.release();
    }
}

use crate::application::{
    document_transfer::{DocumentEnqueue, DocumentPrepare, DocumentTransfer},
    shared_views::wire::{RecordKind, TransferHeader},
};
async fn uploaded(f: &Case, socket: usize, purpose: UploadPurpose, raw: &[u8]) -> Id128 {
    let uploads = f.uploads(socket);
    let client = &f.clients[socket];
    let id = uploads
        .begin(client, target(2), purpose)
        .await
        .unwrap()
        .upload();
    for (n, part) in raw.chunks(16384).enumerate() {
        let wire = frame(id, (n * 16384) as u64, part);
        uploads
            .accept_data(client, InputData::decode_client(&wire).unwrap())
            .await
            .unwrap();
    }
    uploads.finish(client, id, raw.len() as u64).await.unwrap();
    ready(&uploads, client, id).await.body().unwrap()
}
async fn submit(f: &Case, raw: &[u8]) -> crate::domain::EntityId {
    let id = uploaded(f, 0, UploadPurpose::Submit, raw).await;
    let facade = f.node.as_ref().unwrap().queue_management().unwrap();
    let registered = facade
        .register(&f.clients[0], target(2), Mutation::Submit { body: id })
        .await
        .unwrap();
    let reply = facade
        .begin_consume(&f.clients[0], target(2), registered.id(), Some(id))
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert!(matches!(
        reply,
        Consumed::Resolved {
            fact: RequestFact::Settled {
                outcome: QueueOutcome::Queued { .. },
                ..
            },
            ..
        }
    ));
    registered.id().entity()
}
async fn open(
    f: &Case,
    socket: usize,
    item: crate::domain::EntityId,
    revision: u64,
) -> DocumentTransfer {
    let transfer = f
        .node
        .as_ref()
        .unwrap()
        .document_transfer_service()
        .unwrap()
        .open_queued_body(&f.clients[socket], target(2), item, revision)
        .await
        .unwrap();
    f.endpoints[socket].attach(&transfer).unwrap();
    transfer
}
async fn credit(f: &mut Case, socket: usize, stream: Id128, packet: u64) {
    f.peers[socket]
        .send(Message::Binary(
            TransferHeader::credit(stream, packet)
                .encode(&[])
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
}
async fn next_prepare(f: &Case, socket: usize, transfer: &DocumentTransfer) -> DocumentPrepare {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let state = transfer.prepare_next(&f.clients[socket]).await.unwrap();
            if state != DocumentPrepare::AwaitCredit {
                return state;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}
async fn binary(f: &mut Case, socket: usize) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match f.peers[socket].next().await.unwrap().unwrap() {
                Message::Binary(bytes) => return bytes.to_vec(),
                Message::Ping(_) | Message::Pong(_) => {}
                other => panic!(
                    "unexpected document frame kind: {:?}",
                    std::mem::discriminant(&other)
                ),
            }
        }
    })
    .await
    .unwrap()
}
async fn drain(f: &mut Case, socket: usize, transfer: &DocumentTransfer) -> Vec<u8> {
    let mut raw = Vec::new();
    credit(f, socket, transfer.descriptor().stream_id, 0).await;
    loop {
        match next_prepare(f, socket, transfer).await {
            DocumentPrepare::Complete => return raw,
            DocumentPrepare::Prepared => {}
            DocumentPrepare::AwaitCredit => unreachable!(),
        }
        assert_eq!(
            f.endpoints[socket].send(transfer).unwrap(),
            DocumentEnqueue::Queued
        );
        let bytes = binary(f, socket).await;
        let (header, payload) = TransferHeader::decode(&bytes).unwrap();
        assert_eq!(header.stream(), transfer.descriptor().stream_id);
        if header.record_kind() == Some(RecordKind::OriginalRaw) {
            assert_eq!(header.record(), 1);
            assert_eq!(header.offset(), raw.len() as u64);
            raw.extend_from_slice(payload);
        } else {
            assert_eq!(header.record_kind(), Some(RecordKind::SnapshotEnd));
            assert_eq!(header.record(), 2);
            assert!(payload.is_empty());
            assert_eq!(header.total(), 0);
        }
        credit(f, socket, header.stream(), header.packet()).await;
    }
}
#[tokio::test]
async fn document_d1_original_queue_raw_to_real_socket_and_final_credit() {
    case(|f|Box::pin(async move{
  let raw=format!(" \n[{{\"type\":\"future-block\",\"opaque\":{{\"x\":1,\"y\":2,\"n\":9007199254740993,\"p\":\"{}\"}}}}] \t","x".repeat(40*1024)).replace("\\\"","\"").into_bytes();
  let item=submit(f,&raw).await;let transfer=open(f,2,item,1).await;
  assert_eq!(transfer.descriptor().total_bytes,raw.len() as u64);
  assert_eq!(transfer.prepare_next(&f.clients[2]).await.unwrap(),DocumentPrepare::AwaitCredit);
  assert_eq!(f.endpoints[2].observation(&transfer).unwrap().highest_started,0);
  let received=drain(f,2,&transfer).await;assert_eq!(received,raw);
  let observation=f.endpoints[2].observation(&transfer).unwrap();assert!(observation.highest_started>=4);assert_eq!(observation.highest_started,observation.highest_committed);assert_eq!(observation.highest_flushed,observation.highest_committed);
  let descriptor=serde_json::to_value(transfer.descriptor()).unwrap();assert_eq!(descriptor["streamKind"],"queued_input_body");assert_eq!(descriptor["encoding"],"utf8_json");assert_eq!(descriptor.as_object().unwrap().len(),7);assert!(descriptor.get("viewEpoch").is_none());
 })).await;
}
#[tokio::test]
async fn document_d1_real_edit_cancel_seal_without_waiting_for_credit() {
    case(|f| {
        Box::pin(async move {
            let raw = b" [{\"type\":\"text\",\"text\":\"old draft\"}] ";
            let item = submit(f, raw).await;
            let old = open(f, 2, item, 1).await;
            let body = uploaded(f, 0, UploadPurpose::Edit, b"[]").await;
            let queue = f.node.as_ref().unwrap().queue_management().unwrap();
            let r = queue
                .register(
                    &f.clients[0],
                    target(2),
                    Mutation::Edit {
                        item,
                        revision: 1,
                        body,
                    },
                )
                .await
                .unwrap();
            queue
                .begin_consume(&f.clients[0], target(2), r.id(), Some(body))
                .await
                .unwrap()
                .wait()
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(2), old.wait_closed())
                .await
                .unwrap();
            assert_eq!(f.endpoints[2].observation(&old).unwrap().highest_started, 0);
            assert!(
                f.node
                    .as_ref()
                    .unwrap()
                    .document_transfer_service()
                    .unwrap()
                    .open_queued_body(&f.clients[2], target(2), item, 1)
                    .await
                    .is_err()
            );
            let current = open(f, 2, item, 2).await;
            let r = queue
                .register(
                    &f.clients[0],
                    target(2),
                    Mutation::Cancel { item, revision: 2 },
                )
                .await
                .unwrap();
            queue
                .begin_consume(&f.clients[0], target(2), r.id(), None)
                .await
                .unwrap()
                .wait()
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(2), current.wait_closed())
                .await
                .unwrap();
            assert_eq!(
                f.endpoints[2]
                    .observation(&current)
                    .unwrap()
                    .highest_started,
                0
            );
        })
    })
    .await;
}
#[tokio::test]
async fn document_d1_same_actor_other_socket_cannot_take_the_consumer() {
    case(|f| {
        Box::pin(async move {
            let item = submit(f, b"[]").await;
            let transfer = open(f, 0, item, 1).await;
            assert!(transfer.prepare_next(&f.clients[1]).await.is_err());
            assert!(f.endpoints[1].attach(&transfer).is_err());
            assert!(
                transfer
                    .accept_control(
                        &f.clients[1],
                        &TransferHeader::credit(transfer.descriptor().stream_id, 0)
                            .encode(&[])
                            .unwrap()
                    )
                    .await
                    .is_err()
            );
            assert_eq!(
                f.endpoints[0]
                    .observation(&transfer)
                    .unwrap()
                    .highest_started,
                0
            );
            transfer.detach().unwrap();
        })
    })
    .await;
}
#[test]
fn document_d1_descriptor_canonical_decimal_strings() {
    use crate::application::document_source::{DocumentStreamDescriptor, DocumentStreamKind};
    let descriptor = DocumentStreamDescriptor {
        stream_kind: DocumentStreamKind::QueuedInputBody,
        stream_id: Id128::new([1; 16]),
        document_id: Id128::new([2; 16]),
        target_id: Some(target(2)),
        revision: Some(u64::MAX),
        total_bytes: u64::MAX,
        encoding: "utf8_json",
    };
    let wire = serde_json::to_value(&descriptor).unwrap();
    let revision_ok = wire["revision"].as_str() == Some("18446744073709551615");
    let bytes_ok = wire["totalBytes"].as_str() == Some("18446744073709551615");
    assert!(
        revision_ok && bytes_ok,
        "document descriptor must use canonical decimal strings"
    );
    let absent = DocumentStreamDescriptor {
        revision: None,
        ..descriptor
    };
    let wire = serde_json::to_value(absent).unwrap();
    assert!(
        wire["revision"].is_null(),
        "absent revision must remain null"
    );
}

async fn disposal_idle(f: &Case) -> bool {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let state = f.document_receipt.as_ref().unwrap().observe();
            if state.registered_jobs == 0 {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn document_d1_completed_and_empty_jobs_are_reaped() {
    case(|f| {
        Box::pin(async move {
            let item = submit(f, b"[]").await;
            let service = f
                .node
                .as_ref()
                .unwrap()
                .document_transfer_service()
                .unwrap();
            let failed = service
                .open_queued_body(&f.clients[0], target(2), item, 99)
                .await
                .is_err();
            let empty_reaped = disposal_idle(f).await;
            let transfer = open(f, 0, item, 1).await;
            transfer.detach().unwrap();
            let completed_reaped = disposal_idle(f).await;
            drop(transfer);
            assert!(
                failed && empty_reaped && completed_reaped,
                "retired empty and completed disposal jobs must be reaped"
            );
        })
    })
    .await;
}
struct DocumentFlushPause {
    arrived: tokio::sync::watch::Sender<bool>,
    released: tokio::sync::watch::Sender<bool>,
}
struct ReleaseDocumentFlush(Arc<DocumentFlushPause>);
impl Drop for ReleaseDocumentFlush {
    fn drop(&mut self) {
        self.0.released.send_replace(true);
    }
}
impl crate::application::shared_views::local_transfer::LocalIoHooks for DocumentFlushPause {
    fn observe(
        &self,
        _: crate::application::shared_views::controller::ViewHookPoint,
        _: u64,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        Ok(())
    }
    fn pause(
        &self,
        point: crate::application::shared_views::controller::ViewHookPoint,
        _: u64,
    ) -> crate::application::shared_views::BoxReadFuture<
        '_,
        Result<(), crate::domain::shared_view::ViewError>,
    > {
        Box::pin(async move {
            if point == crate::application::shared_views::controller::ViewHookPoint::AfterFlushFactBeforePublish {
                let mut release = self.released.subscribe();
                self.arrived.send_replace(true);
                while !*release.borrow_and_update() { if release.changed().await.is_err() { break; } }
            }
            Ok(())
        })
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn document_d1_publication_failure_retires_without_later_command() {
    case(|f| {
        Box::pin(async move {
            let item = submit(f, b"[]").await;
            let transfer = open(f, 0, item, 1).await;
            let pause = Arc::new(DocumentFlushPause {
                arrived: tokio::sync::watch::channel(false).0,
                released: tokio::sync::watch::channel(false).0,
            });
            let release = ReleaseDocumentFlush(pause.clone());
            transfer
                .socket_stream()
                .install_hooks(pause.clone())
                .unwrap();
            credit(f, 0, transfer.descriptor().stream_id, 0).await;
            next_prepare(f, 0, &transfer).await;
            f.endpoints[0].send(&transfer).unwrap();
            let mut arrived = pause.arrived.subscribe();
            tokio::time::timeout(Duration::from_secs(5), async {
                while !*arrived.borrow_and_update() {
                    arrived.changed().await.unwrap();
                }
            })
            .await
            .unwrap();
            let flushed = transfer.observation().unwrap();
            // This is the original registry state, without sealing the live socket or source epoch.
            f.registry.as_ref().unwrap().seal_connections();
            let live = f.clients[0].context.lifetime.with_live(|| Ok(())).is_ok();
            drop(release);
            let closed = tokio::time::timeout(Duration::from_secs(2), transfer.wait_closed())
                .await
                .is_ok();
            let reaped = disposal_idle(f).await;
            let final_state = transfer.observation().unwrap();
            // Keep the original transfer and endpoint alive until after these physical facts were sampled.
            drop(transfer);
            assert!(
                live && flushed.highest_flushed == 1 && flushed.highest_committed == 0,
                "real flush must precede registry seal while socket remains live"
            );
            assert!(
                closed && reaped,
                "publication failure must itself wake and retire the original source"
            );
            assert!(
                final_state.highest_flushed == 1
                    && final_state.highest_committed == 0
                    && final_state.cursor == 0,
                "sealed publication must retain flush but never commit cursor"
            );
        })
    })
    .await;
}

#[tokio::test]
async fn document_d2_wrapped_snapshot_preserves_original_allocation_and_scope() {
    use crate::application::{document_source::QueuedDocumentSource, shared_views::RetainedRaw};
    use crate::node::document_payload::NodeDocumentPayload;
    case(|f| Box::pin(async move {
        let raw = b" \n[{\"type\":\"future-block\",\"opaque\":9007199254740993}] \t";
        let item = submit(f, raw).await;
        let node = f.node.as_ref().unwrap();
        let queue = &node.session_services.as_ref().unwrap().queue;
        let (document, original_pointer) = queue.inspect_queue(
            f.clients[0].context.actor(), target(2), |read| {
                let queue = match read {
                    QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                };
                let entry = queue.waiting().find(|entry| entry.id() == item).unwrap();
                let document = entry.input().body.body();
                (document.clone(), document.uploaded_bytes().as_ptr() as usize)
            }
        ).await.unwrap();
        let payload = NodeDocumentPayload::from(document);
        assert_eq!(payload.bytes().as_ptr() as usize, original_pointer);
        assert_eq!(payload.bytes(), raw);
        assert_eq!(payload.len(), raw.len());
        drop(payload);
        let service = node.document_transfer_service().unwrap();
        let destination = service.reserve_payload_for_test().unwrap();
        let snapshot = queue.snapshot_queued_body(
            f.clients[0].context.actor(), target(2), item, 1, &destination,
        ).await.unwrap();
        snapshot.verify_scope(&f.registry.as_ref().unwrap().owner_scope()).unwrap();
        snapshot.require_current().await.unwrap();
        assert_eq!(snapshot.total_bytes(), raw.len() as u64);
        assert_eq!(snapshot.copy_range(0, raw.len()).unwrap(), raw);
        snapshot.retire();
        assert!(snapshot.require_current().await.is_err());
        drop(snapshot);
        drop(destination);
        assert!(disposal_idle(f).await);
    })).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn document_d1_pump_delivers_snapshot_end_after_early_credit_during_flush() {
    case(|f| {
        Box::pin(async move {
            let item = submit(f, br#"[{"type":"text","text":"held early pump regression"}]"#).await;
            let transfer = f
                .node
                .as_ref()
                .unwrap()
                .document_transfer_service()
                .unwrap()
                .open_queued_body(&f.clients[0], target(2), item, 1)
                .await
                .unwrap();
            let pause = Arc::new(DocumentFlushPause {
                arrived: tokio::sync::watch::channel(false).0,
                released: tokio::sync::watch::channel(false).0,
            });
            let release = ReleaseDocumentFlush(pause.clone());
            transfer
                .socket_stream()
                .install_hooks(pause.clone())
                .unwrap();
            f.endpoints[0].attach_document(transfer.clone()).unwrap();

            credit(f, 0, transfer.descriptor().stream_id, 0).await;
            let mut arrived = pause.arrived.subscribe();
            tokio::time::timeout(Duration::from_secs(5), async {
                while !*arrived.borrow_and_update() {
                    arrived.changed().await.unwrap();
                }
            })
            .await
            .expect("packet 1 entered post-flush publication pause");

            let packet1 = binary(f, 0).await;
            let (h1, _) = TransferHeader::decode(&packet1).unwrap();
            assert_eq!(h1.packet(), 1);
            assert_eq!(h1.record_kind(), Some(RecordKind::OriginalRaw));

            credit(f, 0, transfer.descriptor().stream_id, 1).await;
            tokio::time::timeout(Duration::from_secs(5), async {
                while !transfer.observation().unwrap().pending_early_credit {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("credit 1 recorded as HeldEarly before publish completed");

            drop(release);
            let packet2 = binary(f, 0).await;
            let (h2, p2) = TransferHeader::decode(&packet2).unwrap();
            assert_eq!(h2.packet(), 2);
            assert_eq!(h2.record_kind(), Some(RecordKind::SnapshotEnd));
            assert!(p2.is_empty());

            credit(f, 0, transfer.descriptor().stream_id, 2).await;
            tokio::time::timeout(Duration::from_secs(5), transfer.wait_closed())
                .await
                .expect("document transfer closed after SnapshotEnd credit");
            assert!(disposal_idle(f).await);
        })
    })
    .await;
}
