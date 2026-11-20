use super::*;
use crate::application::queue_owner::QueueRead;
use crate::domain::request_registry::{QueueOutcome, QueueRejection, RegistryError, RequestFact};
use crate::node::queue_management::{
    NodeConsumeResult as Consumed, NodeQueueError, NodeQueueMutation as Mutation,
};
use crate::{
    application::shared_views::socket_authority::SocketViewClient,
    management_ws::input_upload::InputData,
    node::input_upload::{UploadOwner, UploadPhase, UploadPurpose},
};
use futures_util::FutureExt;
use sha2::{Digest, Sha256};
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
    client_id: String,
    registry: Option<crate::session_storage::request_registry::SqliteRequestRegistry>,
    pool: Option<sqlx::SqlitePool>,
    shutdown_failure: Option<String>,
}
impl Case {
    async fn start(&mut self) {
        self.start_with_factory(
            crate::node::session_services::FactoryTestHooks::default(),
        )
        .await;
    }
    async fn start_with_factory(
        &mut self,
        factory: crate::node::session_services::FactoryTestHooks,
    ) {
        self.owner = Some(NodeProcessOwner::begin_for_factory_test(
            &tokio::runtime::Handle::current(),
            self.area.config(free_address()),
            Arc::new(FakeProtector::default()),
            factory,
        ));
        self.node = Some(self.owner.as_ref().unwrap().ready().await.unwrap());
        let node = self.node.as_ref().unwrap();
        let storage = node.storage.as_ref().unwrap();
        self.registry = Some(storage.request_registry());
        self.pool = Some(storage.pool_for_test().clone());
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
        let client_entity = crate::domain::EntityId::decode(&self.client_id).unwrap();
        let session_control = storage.session_control();
        for n in [2u8, 3] {
            let root_key = crate::domain::native::NativeSessionKey::from_config(
                Some(target(1)),
                &format!("upload-native-{n}"),
            )
            .unwrap();
            session_control.claim_initial(&root_key, client_entity, "Upload author fixture", true);
            session_control.update_root_target_id(&root_key, target(n));
        }
        for _ in 0..2 {
            let observed = node
                .management
                .as_ref()
                .unwrap()
                .observe_next_socket_client_for_upload_test()
                .unwrap();
            let peer = valid_management_socket(&self.area, node.listen_addr()).await;
            let peer =
                authenticate_socket(peer, &key, &self.client_id, node_public_key(node)).await;
            self.peers.push(peer);
            self.clients.push(observed.await.unwrap());
        }
        let other = SigningKey::from_slice(&[70; 32]).unwrap();
        let other_id = pair_browser(node, &other, "Queue other actor").await;
        let observed = node
            .management
            .as_ref()
            .unwrap()
            .observe_next_socket_client_for_upload_test()
            .unwrap();
        let peer = valid_management_socket(&self.area, node.listen_addr()).await;
        self.peers
            .push(authenticate_socket(peer, &other, &other_id, node_public_key(node)).await);
        self.clients.push(observed.await.unwrap());
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
        let shutdown = if let Some(node) = self.node.take() {
            let observation = node.shutdown_observation_probe();
            match node.shutdown(None).await {
                Ok(()) => true,
                Err(error) => {
                    self.shutdown_failure =
                        Some(format!("{error}; shutdown observation {:?}", observation.snapshot()));
                    false
                }
            }
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
    case_with_factory(
        crate::node::session_services::FactoryTestHooks::default(),
        run,
    )
    .await;
}
async fn case_with_factory(
    factory: crate::node::session_services::FactoryTestHooks,
    run: impl for<'a> FnOnce(&'a mut Case) -> Pin<Box<dyn Future<Output = ()> + 'a>>,
) {
    let mut fixture = Case {
        area: TestArea::new(),
        owner: None,
        node: None,
        peers: Vec::new(),
        clients: Vec::new(),
        upload_owners: Vec::new(),
        client_id: String::new(),
        registry: None,
        pool: None,
        shutdown_failure: None,
    };
    let result = AssertUnwindSafe(async {
        fixture.start_with_factory(factory).await;
        run(&mut fixture).await;
    })
    .catch_unwind()
    .await;
    let closed = fixture.close().await;
    let directory_free = crate::data_dir::DataDirectory::acquire(&fixture.area.data_dir()).is_ok();
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
        "original closure receipts: {}",
        fixture.shutdown_failure.as_deref().unwrap_or("no shutdown error")
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

#[tokio::test]
async fn queue_u2_register_repeat_and_same_body_new_identity_use_original_queue() {
    case(|f| Box::pin(async move {
        let facade = f.node.as_ref().unwrap().queue_management().unwrap();
        let raw = b" \n[{\"type\":\"future-block\",\"opaque\":{\"x\":1,\"y\":2,\"n\":9007199254740993}}] \t";
        let body_id = body(f, UploadPurpose::Submit, raw).await;
        let uploads = f.uploads(0);
        let lease = uploads.lease_body(&f.clients[0], body_id, target(2), UploadPurpose::Submit).await.unwrap();
        let original_ptr = lease.validated().body().uploaded_bytes().as_ptr() as usize;
        let registration = facade.register(&f.clients[0], target(2), Mutation::Submit { body: body_id }).await.unwrap();
        assert_eq!(registration.intent().body_digest(), Some(&<[u8;32]>::from(Sha256::digest(raw))));
        assert!(queued(f, 2).await.is_empty(), "register does not occupy the queue");
        let (a,b) = tokio::join!(facade.begin_consume(&f.clients[0], target(2), registration.id(), Some(body_id)), facade.begin_consume(&f.clients[0], target(2), registration.id(), Some(body_id)));
        let (a,b) = (a.unwrap(),b.unwrap());
        assert!(a.same_operation(&b));
        assert!(!a.transferred(), "unpolled original future has not transferred custody");
        let (one,two) = tokio::join!(a.wait(),b.wait());
        assert_eq!(one,two);
        assert!(matches!(one.unwrap(), Consumed::Resolved { fact: RequestFact::Settled { outcome: QueueOutcome::Queued { .. }, .. }, returned: false }));
        assert!(a.transferred());
        assert_eq!(queued(f, 2).await, vec![(registration.id().entity(), original_ptr)]);
        let next = facade.register(&f.clients[0], target(2), Mutation::Submit { body: body_id }).await.unwrap();
        assert_ne!(next.id(), registration.id());
        let next = facade.begin_consume(&f.clients[0], target(2), next.id(), Some(body_id)).await.unwrap();
        assert!(matches!(next.wait().await.unwrap(), Consumed::Resolved { returned: false, .. }));
        assert_eq!(queued(f,2).await.len(), 2);
        assert!(queued(f,2).await.iter().all(|(_,pointer)|*pointer==original_ptr));
        drop(lease);
    })).await;
}

#[tokio::test]
async fn queue_u2_binding_conflicts_and_real_full_return_preserve_draft() {
    case(|f| {
        Box::pin(async move {
            let facade = f.node.as_ref().unwrap().queue_management().unwrap();
            let first = body(
                f,
                UploadPurpose::Submit,
                b"[{\"type\":\"text\",\"text\":\"queue draft\"}]",
            )
            .await;
            let changed = body(
                f,
                UploadPurpose::Submit,
                b"[{\"type\":\"text\",\"text\":\"changed draft\"}]",
            )
            .await;
            let edit = body(
                f,
                UploadPurpose::Edit,
                b"[{\"type\":\"text\",\"text\":\"edit draft\"}]",
            )
            .await;
            let r = facade
                .register(&f.clients[0], target(2), Mutation::Submit { body: first })
                .await
                .unwrap();
            assert!(matches!(
                facade
                    .begin_consume(&f.clients[0], target(2), r.id(), Some(changed))
                    .await,
                Err(NodeQueueError::Registry(RegistryError::Conflict))
            ));
            assert!(
                facade
                    .begin_consume(&f.clients[0], target(2), r.id(), Some(edit))
                    .await
                    .is_err()
            );
            assert!(
                facade
                    .begin_consume(&f.clients[0], target(3), r.id(), Some(first))
                    .await
                    .is_err()
            );
            assert!(
                facade
                    .begin_consume(&f.clients[1], target(2), r.id(), Some(first))
                    .await
                    .is_err()
            );
            assert!(
                facade
                    .begin_consume(&f.clients[0], target(2), r.id(), None)
                    .await
                    .is_err()
            );
            let one = facade
                .begin_consume(&f.clients[0], target(2), r.id(), Some(first))
                .await
                .unwrap();
            one.wait().await.unwrap();
            for _ in 0..2 {
                let r = facade
                    .register(&f.clients[0], target(2), Mutation::Submit { body: first })
                    .await
                    .unwrap();
                facade
                    .begin_consume(&f.clients[0], target(2), r.id(), Some(first))
                    .await
                    .unwrap()
                    .wait()
                    .await
                    .unwrap();
            }
            assert_eq!(queued(f, 2).await.len(), 3);
            let full = facade
                .register(&f.clients[0], target(2), Mutation::Submit { body: first })
                .await
                .unwrap();
            let full = facade
                .begin_consume(&f.clients[0], target(2), full.id(), Some(first))
                .await
                .unwrap();
            assert!(matches!(
                full.wait().await.unwrap(),
                Consumed::Resolved {
                    fact: RequestFact::Settled {
                        outcome: QueueOutcome::Rejected(QueueRejection::Full),
                        ..
                    },
                    returned: true
                }
            ));
            let draft = f
                .uploads(0)
                .lease_body(&f.clients[0], first, target(2), UploadPurpose::Submit)
                .await
                .unwrap();
            assert_eq!(
                draft.validated().body().uploaded_bytes(),
                b"[{\"type\":\"text\",\"text\":\"queue draft\"}]"
            );
            drop(draft);
        })
    })
    .await;
}

#[tokio::test]
async fn queue_u2_cancel_wait_keeps_slot_and_socket_seal_wins_before_transfer() {
    case(|f| {
        Box::pin(async move {
            let facade = f.node.as_ref().unwrap().queue_management().unwrap();
            let body = body(f, UploadPurpose::Submit, b"[]").await;
            let r = facade
                .register(&f.clients[0], target(2), Mutation::Submit { body })
                .await
                .unwrap();
            let receipt = facade
                .begin_consume(&f.clients[0], target(2), r.id(), Some(body))
                .await
                .unwrap();
            let trust = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .trust_store();
            let held = trust.client_gate(&f.client_id).lock_owned().await;
            assert!(
                receipt.wait().now_or_never().is_none(),
                "guard wait is genuinely pending"
            );
            assert!(!receipt.transferred());
            drop(held);
            let same = facade
                .begin_consume(&f.clients[0], target(2), r.id(), Some(body))
                .await
                .unwrap();
            assert!(same.same_operation(&receipt));
            peer_seal(f, 0).await;
            assert!(matches!(
                same.wait().await,
                Err(NodeQueueError::SocketClosed)
            ));
            assert!(!receipt.transferred());
            assert!(matches!(
                facade
                    .query(&f.clients[2], target(2), r.id())
                    .await
                    .unwrap(),
                RequestFact::Registered(_)
            ));
            assert!(queued(f, 2).await.is_empty());
        })
    })
    .await;
}

#[tokio::test]
async fn queue_u2_real_queue_transfer_survives_wait_cancel_socket_seal_and_revoke() {
    case(|f| Box::pin(async move {
        use crate::session_storage::request_registry::test_hooks::{Action,Barrier,Operation,Point};
        let facade=f.node.as_ref().unwrap().queue_management().unwrap();
        let body=body(f,UploadPurpose::Submit,b"[]").await;
        let r=facade.register(&f.clients[0],target(2),Mutation::Submit{body}).await.unwrap();
        let receipt=facade.begin_consume(&f.clients[0],target(2),r.id(),Some(body)).await.unwrap();
        let gate=Barrier::new(); let cleanup=ReleaseSqlGate(gate.clone());
        f.registry.as_ref().unwrap().faults_for_test().arm(Point::BeforeCommit(Operation::Begin),Action::Pause(gate.clone())).unwrap();
        {
            let waiting=receipt.wait();tokio::pin!(waiting);
            tokio::time::timeout(Duration::from_secs(10),async{tokio::select!{()=gate.arrived()=>{},result=&mut waiting=>panic!("original Queue returned before real SQL gate: {result:?}")}}).await.unwrap();
        }
        assert!(receipt.transferred(),"real original install_pending has already moved the mutation");
        peer_seal(f,0).await;
        let ipc=f.node.as_ref().unwrap().ipc_address();
        let revoke=ipc::device_revoke(ipc,&f.client_id);
        gate.release();
        let (result,revoked)=tokio::join!(receipt.wait(),revoke);
        revoked.unwrap();
        assert!(matches!(result.unwrap(),Consumed::Resolved{fact:RequestFact::Settled{outcome:QueueOutcome::Queued{..},..},..}));
        assert!(matches!(facade.query(&f.clients[2],target(2),r.id()).await.unwrap(),RequestFact::Settled{outcome:QueueOutcome::Queued{..},..}));
        assert_eq!(queued(f,2).await.len(),1);
        drop(cleanup);
    })).await;
}

#[tokio::test]
async fn queue_u2_real_commit_unconfirmed_returns_pending_then_original_reconcile() {
    case(|f| {
        Box::pin(async move {
            use crate::session_storage::request_registry::test_hooks::{Action, Operation, Point};
            let facade = f.node.as_ref().unwrap().queue_management().unwrap();
            let body = body(f, UploadPurpose::Submit, b"[]").await;
            let r = facade
                .register(&f.clients[0], target(2), Mutation::Submit { body })
                .await
                .unwrap();
            let receipt = facade
                .begin_consume(&f.clients[0], target(2), r.id(), Some(body))
                .await
                .unwrap();
            f.registry
                .as_ref()
                .unwrap()
                .faults_for_test()
                .arm(Point::AfterCommit(Operation::Begin), Action::Unconfirmed)
                .unwrap();
            assert_eq!(
                receipt.wait().await.unwrap(),
                Consumed::Pending { request: r.id() }
            );
            assert!(receipt.transferred());
            assert!(matches!(
                facade
                    .query(&f.clients[2], target(2), r.id())
                    .await
                    .unwrap(),
                RequestFact::Applying(_)
            ));
            let original = &f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .queue;
            let reconciled = original.reconcile(target(2), r.id()).await.unwrap();
            assert!(matches!(
                reconciled,
                RequestFact::Settled {
                    outcome: QueueOutcome::Queued { .. },
                    ..
                }
            ));
            assert_eq!(
                facade
                    .query(&f.clients[2], target(2), r.id())
                    .await
                    .unwrap(),
                reconciled
            );
            assert_eq!(queued(f, 2).await.len(), 1);
        })
    })
    .await;
}

#[tokio::test]
async fn queue_u2_original_refused_returns_the_mutation_when_runtime_is_stopped() {
    case(|f| {
        Box::pin(async move {
            let facade = f.node.as_ref().unwrap().queue_management().unwrap();
            let body = body(f, UploadPurpose::Submit, b"[]").await;
            let r = facade
                .register(&f.clients[0], target(2), Mutation::Submit { body })
                .await
                .unwrap();
            let receipt = facade
                .begin_consume(&f.clients[0], target(2), r.id(), Some(body))
                .await
                .unwrap();
            let original = &f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .queue;
            let report = original
                .shutdown(crate::application::request_registry::RegistryDeadline::new(
                    tokio::time::Instant::now() + Duration::from_secs(10),
                ))
                .await;
            assert!(report.joined);
            assert_eq!(
                receipt.wait().await.unwrap(),
                Consumed::Refused {
                    error: RegistryError::NodeStopping,
                    returned: true
                }
            );
            assert!(!receipt.transferred());
            let draft = f
                .uploads(0)
                .lease_body(&f.clients[0], body, target(2), UploadPurpose::Submit)
                .await
                .unwrap();
            assert_eq!(draft.validated().body().uploaded_bytes(), b"[]");
            drop(draft);
        })
    })
    .await;
}

fn latest_fact(reply: &Consumed) -> Option<&RequestFact> {
    match reply {
        Consumed::Resolved { fact, .. } | Consumed::Observed { fact } => Some(fact),
        _ => None,
    }
}
#[tokio::test]
async fn queue_u2_pending_old_receipt_and_new_same_id_command_keep_distinct_facts() {
    case(|f| {
        Box::pin(async move {
            use crate::session_storage::request_registry::test_hooks::{Action, Operation, Point};
            let facade = f.node.as_ref().unwrap().queue_management().unwrap();
            let body = body(f, UploadPurpose::Submit, b"[]").await;
            let r = facade
                .register(&f.clients[0], target(2), Mutation::Submit { body })
                .await
                .unwrap();
            let old = facade
                .begin_consume(&f.clients[0], target(2), r.id(), Some(body))
                .await
                .unwrap();
            f.registry
                .as_ref()
                .unwrap()
                .faults_for_test()
                .arm(Point::AfterCommit(Operation::Begin), Action::Unconfirmed)
                .unwrap();
            assert_eq!(
                old.wait().await.unwrap(),
                Consumed::Pending { request: r.id() }
            );
            let applying_fact = facade
                .query(&f.clients[2], target(2), r.id())
                .await
                .unwrap();
            let applying_retry = facade
                .begin_consume(&f.clients[0], target(2), r.id(), Some(body))
                .await
                .unwrap();
            let applying_reply = applying_retry.wait().await.unwrap();
            let applying_same = old.same_operation(&applying_retry);
            let reconciled = f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .queue
                .reconcile(target(2), r.id())
                .await
                .unwrap();
            assert!(matches!(
                reconciled,
                RequestFact::Settled {
                    outcome: QueueOutcome::Queued { .. },
                    ..
                }
            ));
            let new = facade
                .begin_consume(&f.clients[0], target(2), r.id(), Some(body))
                .await
                .unwrap();
            let same = old.same_operation(&new);
            let old_reply = old.wait().await.unwrap();
            let new_reply = new.wait().await.unwrap();
            let count = queued(f, 2).await.len();
            assert!(applying_same);
            assert!(matches!(applying_fact, RequestFact::Applying(_)));
            assert_eq!(
                latest_fact(&applying_reply),
                Some(&applying_fact),
                "new command reads the actual applying fact without replacing its owner"
            );
            assert!(same);
            assert_eq!(
                old_reply,
                Consumed::Pending { request: r.id() },
                "new SELECT must not rewrite the original receipt"
            );
            assert_eq!(count, 1, "a retry must not dispatch another mutation");
            assert_eq!(
                latest_fact(&new_reply),
                Some(&reconciled),
                "new command must return the latest original necessary fact"
            );
        })
    })
    .await;
}

#[tokio::test]
async fn queue_u2_other_actor_cannot_reserve_the_registration_owner_slot() {
    case(|f| {
        Box::pin(async move {
            let facade = f.node.as_ref().unwrap().queue_management().unwrap();
            let a = body_for(f, 0, UploadPurpose::Submit, b"[]").await;
            let b = body_for(f, 2, UploadPurpose::Submit, b"[]").await;
            let registered = facade
                .register(&f.clients[0], target(2), Mutation::Submit { body: a })
                .await
                .unwrap();
            assert!(
                matches!(
                    facade
                        .query(&f.clients[2], target(2), registered.id())
                        .await
                        .unwrap(),
                    RequestFact::Registered(_)
                ),
                "B may read the shared target's original fact"
            );
            let attempt = facade
                .begin_consume(&f.clients[2], target(2), registered.id(), Some(b))
                .await;
            let rejected_before_slot = matches!(
                attempt,
                Err(NodeQueueError::Registry(RegistryError::InvalidActor))
            );
            if let Ok(unauthorized) = attempt {
                let _ = unauthorized.wait().await;
            }
            let allowed = match facade
                .begin_consume(&f.clients[0], target(2), registered.id(), Some(a))
                .await
            {
                Ok(original) => matches!(
                    original.wait().await.unwrap(),
                    Consumed::Resolved {
                        fact: RequestFact::Settled {
                            outcome: QueueOutcome::Queued { .. },
                            ..
                        },
                        ..
                    }
                ),
                Err(_) => false,
            };
            let count = queued(f, 2).await.len();
            assert!(
                rejected_before_slot,
                "the different actor must be rejected before reserving A's slot"
            );
            assert!(
                allowed,
                "the original registering actor must remain able to consume"
            );
            assert_eq!(count, 1);
        })
    })
    .await;
}
