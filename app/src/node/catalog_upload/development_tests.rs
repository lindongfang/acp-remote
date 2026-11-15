use super::*;
use crate::application::queue_owner::QueueRead;
use crate::node::queue_management::NodeQueueMutation as Mutation;
use crate::{
    application::shared_views::socket_authority::SocketViewClient,
    management_ws::input_upload::{InputData, UploadError},
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
    catalog_owners: Vec<crate::node::catalog_upload::CatalogUploadOwner>,
    catalog_receipt:
        Option<Arc<dyn crate::application::catalog_preparation::CatalogUploadShutdownPort>>,
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
        self.catalog_receipt = storage.catalog_upload_shutdown_receipt();
        assert!(self.catalog_receipt.is_some());
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
        self.catalog_owners = self
            .clients
            .iter()
            .map(|c| node.catalog_upload_owner(c).unwrap())
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
        eprintln!("[CATALOG-C1-CLOSE] peers");
        self.close_peers().await;
        eprintln!("[CATALOG-C1-CLOSE] owners");
        self.upload_owners.clear();
        self.catalog_owners.clear();
        self.endpoints.clear();
        let shutdown = if let Some(node) = self.node.take() {
            eprintln!("[CATALOG-C1-CLOSE] shutdown");
            let probe = node.shutdown_observation_probe();
            let native = node.session_services.as_ref().unwrap().native.clone();
            let queue = node.session_services.as_ref().unwrap().queue.clone();
            let result = node.shutdown(None).await;
            let native_status = native.core.status();
            let native_task_failures = queue.task_failures();
            let native_audit_cancelled = queue.take_audit_warnings().len();
            eprintln!(
                "[CATALOG-C1-NATIVE] status={:?} task_failures={native_task_failures} audit_cancelled={native_audit_cancelled}",
                native_status
            );
            for (index, source) in native.source_close_diagnostics().iter().enumerate() {
                eprintln!("[CATALOG-C1-NATIVE] source={index} {source:?}");
            }
            eprintln!("[CATALOG-C1-CLOSE] shutdown_ok={} observation={:?} native={:?}",
                result.is_ok(), probe.snapshot(), native.last_shutdown_report());
            result.is_ok()
        } else {
            true
        };
        let joined = if let Some(owner) = &self.owner {
            eprintln!("[CATALOG-C1-CLOSE] join");
            owner.join_actual().await.is_ok()
        } else {
            true
        };
        let registry = self
            .registry
            .as_ref()
            .is_none_or(|r| r.close_status().is_complete());
        let pool = self.pool.as_ref().is_none_or(|p| p.size() == 0);
        eprintln!("[CATALOG-C1-CLOSE] complete {shutdown} {joined} {registry} {pool}");
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
        catalog_owners: Vec::new(),
        catalog_receipt: None,
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
    let catalog = fixture.catalog_receipt.as_ref().unwrap().observe();
    assert!(
        catalog.sealed
            && catalog.payload_roots == 0
            && catalog.active_use == 0
            && catalog.registered_jobs == 0,
        "original catalog roots/workers drained"
    );
    assert_eq!(
        catalog.supervisor,
        crate::application::catalog_preparation::CatalogTaskJoinState::Joined(
            crate::application::catalog_preparation::CatalogTaskExit::Completed
        )
    );
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

use crate::domain::native_catalog::{CatalogResult, CatalogState, ProfileKey};
use crate::node::catalog_upload::CatalogUploadStatus;
fn utf16(value: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(
        value
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}
fn profile(scenario: &str, large: bool) -> Vec<u8> {
    let child = crate::test_support::native_dispatch_dev_child(&["C1_NATIVE_CHILD"]);
    let child = child.to_string_lossy();
    assert!(std::path::Path::new(child.as_ref()).is_file());
    format!(r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}"],"environment":[{{"name_utf16le_base64":"{}","value_utf16le_base64":"{}"}}]}},"recipes":[{{"wire":2,"mcp_servers":[],"additional_directories":[],"extra":{{"future":{{"decimal":1.2300e+09,"ordered":{{"b":2,"a":1}},"payload":"{}"}}}}}}]}}"#,utf16(&child),utf16(scenario),utf16("C1_SECRET"),utf16("CATALOG_SECRET_CANARY"),if large{"x".repeat(40*1024)}else{String::new()}).into_bytes()
}
async fn upload(f: &Case, socket: usize, raw: &[u8]) -> (Id128, CatalogUploadStatus) {
    let owner = &f.catalog_owners[socket];
    let c = &f.clients[socket];
    let u = owner.begin(c).await.unwrap().upload();
    for (n, part) in raw.chunks(16384).enumerate() {
        let wire = frame(u, (n * 16384) as u64, part);
        let ack = owner
            .accept_data(c, InputData::decode_client(&wire).unwrap())
            .await
            .unwrap();
        assert_eq!(ack.next_offset(), ((n * 16384) + part.len()) as u64);
    }
    owner.finish(c, u, raw.len() as u64).await.unwrap();
    let status = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let s = owner.status(c, u).await.unwrap();
            if matches!(s.phase(), UploadPhase::Ready | UploadPhase::Failed) {
                break s;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    (u, status)
}
async fn install(f: &Case, large: bool) -> (crate::domain::EntityId, ProfileKey, Vec<u8>) {
    let raw = profile("stop-v2-max_tokens", large);
    let text = format!(
        r#"{{"kind":"install_harness","name":"C1 owned child","source":{{"kind":"existing","sourceId":"{}"}},"profile":{}}}"#,
        target(1).encode(),
        std::str::from_utf8(&raw).unwrap()
    );
    let (u, status) = upload(f, 0, text.as_bytes()).await;
    assert!(
        matches!(status.phase(), UploadPhase::Ready),
        "original profile must prepare"
    );
    let management = f.node.as_ref().unwrap().catalog_management().unwrap();
    let registered = management.register(&f.clients[0], u).await.unwrap();
    let same = management.register(&f.clients[0], u).await.unwrap();
    assert!(
        registered.identity().same(&same.identity()),
        "same upload retains same register slot"
    );
    let id = registered.wait().await.unwrap();
    assert_eq!(same.wait().await.unwrap(), id);
    let applied = management.apply(&f.clients[0], id).await.unwrap();
    let same = management.apply(&f.clients[0], id).await.unwrap();
    assert!(applied.identity().same(&same.identity()));
    let receipt = applied.wait().await.unwrap();
    assert_eq!(receipt.fact().state(), CatalogState::Committed);
    let metadata = crate::node::catalog_upload::catalog_receipt_metadata(&receipt).to_string();
    assert!(
        !metadata.contains("CATALOG_SECRET_CANARY")
            && !metadata.contains("launch")
            && !metadata.contains("payload"),
        "safe result must exclude all configuration"
    );
    let result = receipt.fact().result().unwrap();
    match result {
        CatalogResult::Harness {
            harness,
            source,
            profile,
        } => {
            assert_eq!(*source, target(1));
            (*harness, *profile, raw)
        }
        _ => panic!("original catalog returned wrong result kind"),
    }
}
async fn registered_apply(f: &Case, raw: &[u8]) -> crate::domain::native_catalog::CatalogReceipt {
    let (u, status) = upload(f, 0, raw).await;
    assert!(
        matches!(status.phase(), UploadPhase::Ready),
        "valid draft must be ready"
    );
    let m = f.node.as_ref().unwrap().catalog_management().unwrap();
    let id = m
        .register(&f.clients[0], u)
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    let result = m
        .apply(&f.clients[0], id)
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    let observed = m
        .query(&f.clients[0], id)
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert_eq!(observed.state(), result.fact().state());
    result
}
async fn bind(
    f: &Case,
    harness: crate::domain::EntityId,
    profile: ProfileKey,
    launch: &str,
) -> crate::domain::native_catalog::CatalogReceipt {
    let pool = f.pool.as_ref().unwrap();
    let canonical = f
        .area
        .directory
        .canonicalize()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    sqlx::query("INSERT INTO main.workspaces VALUES(?,?)")
        .bind(target(9).encode())
        .bind(&canonical)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE main.product_targets SET workspace_id=? WHERE target_id=?")
        .bind(target(9).encode())
        .bind(target(2).encode())
        .execute(pool)
        .await
        .unwrap();
    let text=serde_json::json!({"kind":"bind_target_initial","targetId":target(2).encode(),"sourceId":target(1).encode(),"harnessId":harness.encode(),"profileId":profile.id().encode(),"profileRevision":profile.revision().to_string(),"workspaceId":target(9).encode(),"launchPath":launch}).to_string();
    let receipt = registered_apply(f, text.as_bytes()).await;
    let row:(String,String)=sqlx::query_as("SELECT launch_path,observed_canonical_path FROM main.node_target_launch_selection WHERE target_id=?").bind(target(2).encode()).fetch_one(pool).await.unwrap();
    assert!(
        row.0 == launch && row.1 == canonical,
        "explicit launch path and original registered canonical remain separate"
    );
    receipt
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_c1_three_original_branches_preserve_profile_and_trusted_workspace() {
    case(|f|Box::pin(async move{
 let(harness,key,original)=install(f,true).await;
 let protected:Vec<u8>=sqlx::query_scalar("SELECT protected_payload FROM main.node_launch_profiles WHERE profile_id=?").bind(key.id().encode()).fetch_one(f.pool.as_ref().unwrap()).await.unwrap();assert!(protected.windows(original.len()).any(|p|p==original),"original protected intent must preserve exact profile fragment with lexical number/order");
 let next=profile("stop-v2-max_tokens",true);let text=format!(r#"{{"kind":"publish_profile","harnessId":"{}","sourceId":"{}","publication":{{"kind":"next","profileId":"{}","expectedRevision":"{}"}},"profile":{}}}"#,harness.encode(),target(1).encode(),key.id().encode(),key.revision(),std::str::from_utf8(&next).unwrap());let published=registered_apply(f,text.as_bytes()).await;let next=match published.fact().result().unwrap(){CatalogResult::Profile{profile,..}=>*profile,_=>panic!("expected profile result")};assert_eq!(next.revision(),2);
 let launch=format!("CATALOG_PATH_CANARY_{}","z".repeat(40*1024));bind(f,harness,next,&launch).await;
 let new_source_text=format!(r#"{{"kind":"install_harness","name":"C1 new source","source":{{"kind":"new","name":"C1 source"}},"profile":{}}}"#,std::str::from_utf8(&original).unwrap());
 let new_source=registered_apply(f,new_source_text.as_bytes()).await;
 assert!(matches!(new_source.fact().result(),Some(CatalogResult::Harness{source,..})if *source!=target(1)),"new source comes from original catalog transaction");
 let new_profile_text=format!(r#"{{"kind":"publish_profile","harnessId":"{}","sourceId":"{}","publication":{{"kind":"new"}},"profile":{}}}"#,harness.encode(),target(1).encode(),std::str::from_utf8(&original).unwrap());
 let new_profile=registered_apply(f,new_profile_text.as_bytes()).await;
 assert!(matches!(new_profile.fact().result(),Some(CatalogResult::Profile{profile,..})if profile.id()!=key.id()&&profile.revision()==1),"new profile and next revision retain original distinct semantics");
 let services=f.node.as_ref().unwrap().session_services.as_ref().unwrap();assert!(Arc::ptr_eq(&services.catalog.configuration(),&services.configuration),"same original catalog configuration Arc");
})).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_c1_closed_drafts_and_original_socket_purpose_are_isolated() {
    case(|f|Box::pin(async move{
 let owner=&f.catalog_owners[0];let c=&f.clients[0];let u=owner.begin(c).await.unwrap().upload();
 let wire=frame(u,0,b"[]");assert!(f.upload_owners[0].accept_data(c,InputData::decode_client(&wire).unwrap()).await.is_err());assert!(owner.status(&f.clients[1],u).await.is_err());
 assert!(owner.accept_data(c,InputData::decode_client(&wire).unwrap()).await.is_ok());assert!(owner.accept_data(c,InputData::decode_client(&wire).unwrap()).await.is_ok());let conflict=frame(u,0,b"{}");assert!(matches!(owner.accept_data(c,InputData::decode_client(&conflict).unwrap()).await,Err(UploadError::Conflict)));owner.release(c,u).await.unwrap();
 for raw in [br#"{"kind":"install_harness","kind":"install_harness","name":"CATALOG_SECRET_CANARY"}"#.as_slice(),br#"{"kind":"bind_target_initial","observedCanonicalPath":"CATALOG_PATH_CANARY"}"#.as_slice(),br#"{"kind":"future"}"#.as_slice()]{let(u,s)=upload(f,0,raw).await;assert!(matches!(s.phase(),UploadPhase::Failed));let safe=format!("{:?}",s.error());assert!(!safe.contains("CATALOG_SECRET_CANARY")&&!safe.contains("CATALOG_PATH_CANARY"));assert!(f.node.as_ref().unwrap().catalog_management().unwrap().register(c,u).await.is_err());owner.release(c,u).await.unwrap();}
 let count:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_catalog_commands").fetch_one(f.pool.as_ref().unwrap()).await.unwrap();assert_eq!(count,0);
})).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_c1_ordinary_native_child_real_stop_suspends_original_waiting() {
    case(|f| {
        Box::pin(async move {
            eprintln!("[CATALOG-C1-STAGE] begin");
            let _release = CatalogFileRelease(f.area.directory.join("release-first-prompt"));
            let (harness, profile, _) = install(f, false).await;
            eprintln!("[CATALOG-C1-STAGE] installed");
            let launch = f
                .area
                .directory
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            bind(f, harness, profile, &launch).await;
            eprintln!("[CATALOG-C1-STAGE] bound");
            let services = f.node.as_ref().unwrap().session_services.as_ref().unwrap();
            let first = input_submit(
                f,
                b"[{\"type\":\"text\",\"text\":\"first real controlled prompt\"}]",
            )
            .await;
            eprintln!("[CATALOG-C1-STAGE] first submitted");
            // ensure_target acquires the original accepted selection; it is not an actor/ID source constructor.
            services.native.ensure_target(target(2)).await.unwrap();
            eprintln!("[CATALOG-C1-STAGE] target ensured");
            tokio::time::timeout(Duration::from_secs(10), async {
                while !f.area.directory.join("first-prompt-ready").is_file() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            eprintln!("[CATALOG-C1-STAGE] first prompt ready");
            let old = input_submit(f, b"[{\"type\":\"text\",\"text\":\"old real waiting\"}]").await;
            eprintln!("[CATALOG-C1-STAGE] old submitted");
            std::fs::write(
                f.area.directory.join("release-first-prompt"),
                b"owned synthetic child gate",
            )
            .unwrap();
            eprintln!("[CATALOG-C1-STAGE] first prompt released");
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let suspended = services
                        .queue
                        .inspect_queue(f.clients[0].context.actor(), target(2), |q| {
                            let q = match q {
                                QueueRead::Stable { queue }
                                | QueueRead::Processing { queue, .. } => queue,
                            };
                            q.active().is_none()
                                && q.failure_suspended()
                                && q.waiting().any(|e| e.id() == old && e.suspended())
                        })
                        .await
                        .unwrap();
                    if suspended {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            eprintln!("[CATALOG-C1-STAGE] queue suspended");
            assert_ne!(first, old);
            let doc = f
                .node
                .as_ref()
                .unwrap()
                .document_transfer_service()
                .unwrap()
                .open_queued_body(&f.clients[0], target(2), old, 1)
                .await
                .unwrap();
            doc.detach().unwrap();
            eprintln!("[CATALOG-C1-STAGE] body detached");
        })
    })
    .await;
}
async fn input_submit(f: &Case, raw: &[u8]) -> crate::domain::EntityId {
    let owner = &f.upload_owners[0];
    let c = &f.clients[0];
    let upload = owner
        .begin(c, target(2), UploadPurpose::Submit)
        .await
        .unwrap()
        .upload();
    for (n, part) in raw.chunks(16384).enumerate() {
        let wire = frame(upload, (n * 16384) as u64, part);
        owner
            .accept_data(c, InputData::decode_client(&wire).unwrap())
            .await
            .unwrap();
    }
    owner.finish(c, upload, raw.len() as u64).await.unwrap();
    let body = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let s = owner.status(c, upload).await.unwrap();
            if let Some(body) = s.body() {
                break body;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let m = f.node.as_ref().unwrap().queue_management().unwrap();
    let r = m
        .register(c, target(2), Mutation::Submit { body })
        .await
        .unwrap();
    m.begin_consume(c, target(2), r.id(), Some(body))
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    r.id().entity()
}

struct CatalogBlock {
    entered: tokio::sync::watch::Sender<bool>,
    released: std::sync::Mutex<bool>,
    changed: std::sync::Condvar,
}
impl CatalogBlock {
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
struct CatalogRelease(Arc<CatalogBlock>);
impl Drop for CatalogRelease {
    fn drop(&mut self) {
        self.0.release();
    }
}
#[tokio::test(flavor = "current_thread")]
async fn catalog_c1_actual_append_wait_cancel_no_runtime_and_original_shutdown_join() {
    case(|f|Box::pin(async move{
 use crate::application::catalog_preparation::{CatalogWorkerPoint,CatalogTaskPhase,CatalogTaskJoinState};
 let gate=Arc::new(CatalogBlock{entered:tokio::sync::watch::channel(false).0,released:std::sync::Mutex::new(false),changed:std::sync::Condvar::new()});let release=CatalogRelease(gate.clone());let actual=Arc::new(std::sync::Mutex::new(None));let observed=actual.clone();let block=gate.clone();
 f.node.as_ref().unwrap().catalog_management().unwrap().install_worker_observer_for_test(Arc::new(move|point,phase,identity|{if point==CatalogWorkerPoint::Entered&&phase==CatalogTaskPhase::Append{*observed.lock().unwrap()=Some(identity);let mut released=block.released.lock().unwrap();block.entered.send_replace(true);while !*released{released=block.changed.wait(released).unwrap();}}})).unwrap();
 let owner=f.catalog_owners[0].clone();let client=f.endpoints[0].client();let u=owner.begin(&client).await.unwrap().upload();let append_owner=owner.clone();let append_client=client.context.client();let mut call=Box::pin(async move{let wire=frame(u,0,b"[");append_owner.accept_data(&append_client,InputData::decode_client(&wire).unwrap()).await});let mut entered=gate.entered.subscribe();
 tokio::time::timeout(Duration::from_secs(5),async{tokio::select!{_=&mut call=>panic!("append cannot return before original worker release"),()=async{while !*entered.borrow_and_update(){entered.changed().await.unwrap();}}=>{}}}).await.unwrap();
 let receipt=f.catalog_receipt.as_ref().unwrap().clone();let before=receipt.observe();let identity=actual.lock().unwrap().clone().unwrap();let exact=before.workers.iter().any(|w|w.identity.same(&identity)&&w.join==CatalogTaskJoinState::Pending);let status=owner.status(&client,u).await.unwrap();let old_offset=status.next_offset();let finish_refused=owner.finish(&client,u,1).await.is_err();let foreign_refused=owner.release(&f.clients[1],u).await.is_err();
 std::thread::spawn(move||drop(call)).join().unwrap();owner.release(&client,u).await.unwrap();
 send_json(&mut f.peers[0],&serde_json::json!({"v":1,"type":"command","requestId":Id128::new([88;16]),"action":"node.info","payload":{}})).await;let info=next_json(&mut f.peers[0],Duration::from_secs(2)).await;let responsive=info["ok"]==true;
 let strong=receipt.clone();let mut wait=Box::pin(async move{strong.join().await});let pending=std::future::poll_fn(|cx|std::task::Poll::Ready(wait.as_mut().poll(cx).is_pending())).await;std::thread::spawn(move||drop(wait)).join().unwrap();
 let node=f.node.take().unwrap();let mut shutdown=Box::pin(node.shutdown(None));let shutdown_pending=std::future::poll_fn(|cx|std::task::Poll::Ready(shutdown.as_mut().poll(cx).is_pending())).await;std::thread::spawn(move||drop(shutdown)).join().unwrap();
 drop(release);
 // Cleanup/join occurs in the outer original Case before its physical assertions.
 assert!(exact&&before.active_use>0&&before.payload_roots==1&&old_offset==0,"original append must stay owned before copy/ACK");assert!(finish_refused&&foreign_refused&&responsive&&pending&&shutdown_pending,"real pending work must not block reader or lose its original join when waits are cancelled");
})).await;
}

struct CatalogFileRelease(std::path::PathBuf);
impl Drop for CatalogFileRelease {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, b"owned fixture release on exit");
    }
}
