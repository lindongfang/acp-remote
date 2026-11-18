// Included beside the existing queue fixture, using its real Node/TLS ownership and cleanup.
use crate::application::shared_views::{
    RetainedRaw, SharedViewOwner,
    indexed_journal::{ProjectionEnvelope, ViewProjector},
    ingress::{SourceProducer, source_ingress},
    read_observation::ReadObservation,
    reader::{OpenedView, ViewReadService},
    wire::TransferHeader,
};
use crate::domain::native::{NativeSessionKey, NativeSourceId};
use crate::node::input_upload::UploadRouteFault;
use std::num::NonZeroUsize;

struct BarrierRelease(Arc<crate::test_support::AsyncBarrier>);
impl Drop for BarrierRelease {
    fn drop(&mut self) {
        self.0.release();
    }
}

struct R06CpuPause {
    entered: std::sync::atomic::AtomicBool,
    changed: tokio::sync::Notify,
    released: std::sync::Mutex<bool>,
    cv: std::sync::Condvar,
}
impl R06CpuPause {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: std::sync::atomic::AtomicBool::new(false),
            changed: tokio::sync::Notify::new(),
            released: std::sync::Mutex::new(false),
            cv: std::sync::Condvar::new(),
        })
    }
    fn pause(&self) {
        self.entered
            .store(true, std::sync::atomic::Ordering::Release);
        self.changed.notify_one();
        let mut released = self
            .released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*released {
            released = self
                .cv
                .wait(released)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
    fn release(&self) {
        *self
            .released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        self.cv.notify_all();
    }
    async fn reached(&self) {
        timeout(Duration::from_secs(10), async {
            loop {
                let notified = self.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.entered.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                notified.await;
            }
        })
        .await
        .expect("prompt CPU reached the original preparation worker");
    }
}
struct ReleaseCpu(Arc<R06CpuPause>);
impl Drop for ReleaseCpu {
    fn drop(&mut self) {
        self.0.release();
    }
}

fn r06_cpu_factory() -> (
    crate::node::session_services::FactoryTestHooks,
    Arc<R06CpuPause>,
    ReleaseCpu,
) {
    let pause = R06CpuPause::new();
    let observed = pause.clone();
    let fired = std::sync::atomic::AtomicBool::new(false);
    let mut hooks = crate::node::session_services::FactoryTestHooks::default();
    hooks.body.observer = Some(Arc::new(move |point| {
        if point == crate::node::prompt_preparation::test_hooks::Point::CpuEntered
            && !fired.swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            observed.pause();
        }
    }));
    let release = ReleaseCpu(pause.clone());
    (hooks, pause, release)
}

async fn r06_stop_connection(
    f: &mut Case,
    barrier: crate::test_support::SharedAsyncBarrier,
) -> tokio::task::JoinHandle<Result<(), AppError>> {
    let _release = BarrierRelease(barrier.clone());
    let node = f.node.take().expect("live Node fixture");
    let control = node.control.clone();
    let shutdown = tokio::spawn(node.shutdown(None));
    timeout(Duration::from_secs(5), async {
        while control.is_ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Node publishes stopping before connection cleanup");
    let (code, reason) = next_close(&mut f.peers[0], Duration::from_secs(5)).await;
    assert_eq!((code, reason.as_str()), (1001, "node stopping"));
    barrier.release();
    shutdown
}

async fn r06_wait_owner_clean(f: &Case) {
    timeout(Duration::from_secs(5), async {
        loop {
            let observation = f.upload_owners[0].observation();
            if observation.sealed && observation.entries == 0 && observation.receiving == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("original upload owner sealed");
}

async fn r06_prepare_for_action(
    f: &mut Case,
    action: crate::management_ws::UploadCommandAction,
) -> Id128 {
    let begin = queue_read_call(
        f,
        0,
        180,
        "input.upload.begin",
        serde_json::json!({"targetId":target(2).encode(),"purpose":"submit"}),
    )
    .await;
    assert_eq!(begin["ok"], true, "{begin}");
    let upload =
        Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap()).expect("upload id");
    if action == crate::management_ws::UploadCommandAction::Finish {
        let payload = br#"[{"type":"text","text":"R06 finish"}]"#;
        send_binary(&mut f.peers[0], frame(upload, 0, payload)).await;
        assert_eq!(
            next_binary(&mut f.peers[0], Duration::from_secs(5))
                .await
                .len(),
            32
        );
    }
    upload
}

fn r06_action_payload(
    action: crate::management_ws::UploadCommandAction,
    upload: Id128,
) -> (&'static str, serde_json::Value) {
    match action {
        crate::management_ws::UploadCommandAction::Begin => (
            "input.upload.begin",
            serde_json::json!({"targetId":target(3).encode(),"purpose":"submit"}),
        ),
        crate::management_ws::UploadCommandAction::Finish => (
            "input.upload.finish",
            serde_json::json!({
                "uploadId":upload.to_base64url(),
                "totalBytes":br#"[{"type":"text","text":"R06 finish"}]"#.len().to_string()
            }),
        ),
        crate::management_ws::UploadCommandAction::Status => (
            "input.upload.status",
            serde_json::json!({"uploadId":upload.to_base64url()}),
        ),
        crate::management_ws::UploadCommandAction::Release => (
            "input.upload.release",
            serde_json::json!({"uploadId":upload.to_base64url()}),
        ),
    }
}

struct QueueReadRelease(Arc<crate::test_support::AsyncBarrier>);
impl Drop for QueueReadRelease {
    fn drop(&mut self) {
        self.0.release();
    }
}
async fn send_binary(peer: &mut ManagementSocket, bytes: Vec<u8>) {
    peer.send(Message::Binary(bytes.into()))
        .await
        .expect("WebSocket binary send");
}
async fn next_binary(peer: &mut ManagementSocket, wait: Duration) -> Vec<u8> {
    timeout(wait, async {
        loop {
            match peer
                .next()
                .await
                .expect("WebSocket frame")
                .expect("WebSocket")
            {
                Message::Binary(bytes) => return bytes.to_vec(),
                Message::Ping(_) | Message::Pong(_) => {}
                Message::Close(frame) => panic!("unexpected WebSocket close: {frame:?}"),
                Message::Text(text) => panic!("unexpected protocol frame: {text}"),
                Message::Frame(_) => panic!("unexpected raw WebSocket frame"),
            }
        }
    })
    .await
    .expect("timed out waiting for binary frame")
}
async fn queue_read_call(
    f: &mut Case,
    socket: usize,
    marker: u8,
    action: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    send_json(&mut f.peers[socket], &serde_json::json!({"v":1,"type":"command","requestId":Id128::new([marker;16]),"action":action,"payload":payload})).await;
    let value = next_json(&mut f.peers[socket], Duration::from_secs(5)).await;
    assert_eq!(value["requestId"], Id128::new([marker; 16]).to_base64url());
    value
}

async fn acpi_begin(f: &mut Case, socket: usize, marker: u8) -> Id128 {
    let begin = queue_read_call(
        f,
        socket,
        marker,
        "input.upload.begin",
        serde_json::json!({
            "targetId":target(2).encode(),
            "purpose":"submit"
        }),
    )
    .await;
    assert_eq!(begin["ok"], true, "{begin}");
    Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap()).expect("upload id")
}

async fn acpi_status(f: &mut Case, socket: usize, marker: u8, upload: Id128) -> serde_json::Value {
    queue_read_call(
        f,
        socket,
        marker,
        "input.upload.status",
        serde_json::json!({"uploadId":upload.to_base64url()}),
    )
    .await
}

async fn acpi_finish(
    f: &mut Case,
    socket: usize,
    marker: u8,
    upload: Id128,
    total: u64,
) -> serde_json::Value {
    queue_read_call(
        f,
        socket,
        marker,
        "input.upload.finish",
        serde_json::json!({
            "uploadId":upload.to_base64url(),
            "totalBytes":total.to_string()
        }),
    )
    .await
}

async fn acpi_wait_terminal(
    f: &mut Case,
    socket: usize,
    mut marker: u8,
    upload: Id128,
    expected: &str,
) -> serde_json::Value {
    timeout(Duration::from_secs(10), async {
        loop {
            let status = acpi_status(f, socket, marker, upload).await;
            marker = marker.checked_add(1).expect("bounded status polling");
            match status["result"]["phase"].as_str() {
                Some(phase) if phase == expected => return status,
                Some("preparing") => tokio::task::yield_now().await,
                other => panic!("unexpected upload phase: {other:?}: {status}"),
            }
        }
    })
    .await
    .expect("upload did not reach terminal phase")
}

async fn r04_new_peer(f: &mut Case) -> (ManagementSocket, UploadOwner) {
    let observed = f
        .node
        .as_ref()
        .unwrap()
        .management
        .as_ref()
        .unwrap()
        .observe_next_socket_client_for_upload_test()
        .unwrap();
    let key = SigningKey::from_slice(&[69; 32]).unwrap();
    let peer = valid_management_socket(&f.area, f.node.as_ref().unwrap().listen_addr()).await;
    let peer = authenticate_socket(
        peer,
        &key,
        &f.client_id,
        node_public_key(f.node.as_ref().unwrap()),
    )
    .await;
    let client = observed.await.unwrap();
    let owner = f
        .node
        .as_ref()
        .unwrap()
        .input_upload_owner(&client)
        .unwrap();
    (peer, owner)
}

async fn r04_call_on(
    peer: &mut ManagementSocket,
    marker: u8,
    action: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    send_json(
        peer,
        &serde_json::json!({
            "v":1,
            "type":"command",
            "requestId":Id128::new([marker;16]),
            "action":action,
            "payload":payload
        }),
    )
    .await;
    let value = next_json(peer, Duration::from_secs(5)).await;
    assert_eq!(value["requestId"], Id128::new([marker; 16]).to_base64url());
    value
}

#[derive(Clone)]
struct R05Raw(Arc<[u8]>);
impl RetainedRaw for R05Raw {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn bytes(&self) -> &[u8] {
        &self.0
    }
}

struct R05Projector;
impl ViewProjector<R05Raw> for R05Projector {
    fn inspect(&self, _: &R05Raw) -> ProjectionEnvelope {
        ProjectionEnvelope::unknown(Some(Arc::from("acpi-r05")))
    }
}

type R05Reads = ViewReadService<
    u64,
    R05Raw,
    crate::application::shared_views::socket_authority::SocketViewAuthority,
    crate::application::shared_views::socket_authority::SocketViewClient,
>;
type R05Source = (
    SharedViewOwner<u64, R05Raw>,
    R05Reads,
    OpenedView,
    SourceProducer<u64, R05Raw>,
);
async fn r05_source_for_endpoint(
    f: &Case,
    endpoint: &crate::management_ws::DocumentSocketEndpoint,
    raw: Arc<[u8]>,
) -> R05Source {
    let scope = 1u64;
    let source = NativeSourceId(crate::domain::EntityId::from_bytes([98; 16]));
    let target = NativeSessionKey::from_config(Some(source.0), "acpi-r05").unwrap();
    let (mut producer, consumer, probe) = source_ingress(scope);
    let owner = SharedViewOwner::start_source(
        scope,
        source,
        consumer,
        probe,
        Arc::new(R05Projector),
        NonZeroUsize::new(8).unwrap(),
        tokio::runtime::Handle::current(),
    )
    .unwrap();
    producer.bind_route(target.clone(), None).unwrap();
    producer
        .retain(&scope, 1, 1, R05Raw(raw), ReadObservation::unavailable())
        .unwrap();
    owner.handle().core.probe.wait_processed(1).await.unwrap();
    let authority = f
        .node
        .as_ref()
        .unwrap()
        .management
        .as_ref()
        .unwrap()
        .socket_authority();
    authority.register_module_source(&owner.handle()).unwrap();
    let reads = ViewReadService::new(owner.handle(), authority, owner.supervisor.clone());
    let client = endpoint.client();
    let read = reads.open_current(&client, &target).await.unwrap();
    endpoint.attach_shared_read(&read).unwrap();
    (owner, reads, read, producer)
}
async fn r05_new_peer(f: &mut Case) -> (ManagementSocket, UploadOwner) {
    r04_new_peer(f).await
}
fn r05_credit(stream: Id128) -> Vec<u8> {
    TransferHeader::credit(stream, 0)
        .encode(&[])
        .expect("ACPV credit")
}

async fn r07_new_peer(
    f: &mut Case,
) -> (
    ManagementSocket,
    UploadOwner,
    crate::management_ws::DocumentSocketEndpoint,
) {
    let observed = f
        .node
        .as_ref()
        .unwrap()
        .management
        .as_ref()
        .unwrap()
        .observe_next_document_socket_for_test()
        .unwrap();
    let key = SigningKey::from_slice(&[69; 32]).unwrap();
    let peer = authenticate_socket(
        valid_management_socket(&f.area, f.node.as_ref().unwrap().listen_addr()).await,
        &key,
        &f.client_id,
        node_public_key(f.node.as_ref().unwrap()),
    )
    .await;
    let endpoint = observed.await.unwrap();
    let owner = f
        .node
        .as_ref()
        .unwrap()
        .input_upload_owner(&endpoint.client())
        .unwrap();
    (peer, owner, endpoint)
}

async fn r07_ack_count_until_close(peer: &mut ManagementSocket) -> usize {
    timeout(Duration::from_secs(5), async {
        let mut acknowledgements = 0;
        loop {
            match peer
                .next()
                .await
                .expect("WebSocket frame")
                .expect("WebSocket")
            {
                Message::Binary(bytes) => {
                    assert_eq!(bytes.len(), 32, "only original ACPI ACKs may precede close");
                    acknowledgements += 1;
                }
                Message::Close(Some(frame)) => {
                    assert_eq!(u16::from(frame.code), 4429);
                    return acknowledgements;
                }
                Message::Ping(_) | Message::Pong(_) => {}
                Message::Text(text) => panic!("unexpected JSON before close: {text}"),
                Message::Close(None) | Message::Frame(_) => panic!("unexpected close/Fraw frame"),
            }
        }
    })
    .await
    .expect("timed out waiting for close-priority output")
}

async fn r09_fault_cell(fault: UploadRouteFault, binary: bool) {
    case(|f| {
        Box::pin(async move {
            let begin = queue_read_call(
                f,
                0,
                220,
                "input.upload.begin",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "purpose": "submit"
                }),
            )
            .await;
            assert_eq!(begin["ok"], true, "{begin}");
            let upload = Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap())
                .expect("upload id");
            let services = f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .management_services();
            let receipt = services
                .arm_route_fault_for_test(&f.clients[0], fault)
                .expect("arm production typed route fault");
            let management = f
                .node
                .as_ref()
                .unwrap()
                .management
                .as_ref()
                .unwrap()
                .clone();
            let before = management.connection_counts_for_test();

            if binary {
                send_binary(&mut f.peers[0], frame(upload, 0, b"[]")).await;
                let (code, _) = next_close(&mut f.peers[0], Duration::from_secs(5)).await;
                assert_eq!(
                    code,
                    match fault {
                        UploadRouteFault::ActorRevoked => 4410,
                        UploadRouteFault::ActorStorageUnavailable => 4500,
                        UploadRouteFault::UploadStorageUnavailable => 4400,
                    },
                    "fault source {fault:?}"
                );
            } else if matches!(fault, UploadRouteFault::UploadStorageUnavailable) {
                let result = queue_read_call(
                    f,
                    0,
                    221,
                    "input.upload.status",
                    serde_json::json!({"uploadId": upload.to_base64url()}),
                )
                .await;
                assert_eq!(result["ok"], false, "{result}");
                assert_eq!(
                    result["error"]["code"], "necessary_storage_unavailable",
                    "fault source {fault:?}"
                );
                assert!(result["result"].is_null());
                let _ = f.peers[0].close(None).await;
            } else {
                send_json(
                    &mut f.peers[0],
                    &serde_json::json!({
                        "v":1,
                        "type":"command",
                        "requestId":Id128::new([221;16]),
                        "action":"input.upload.status",
                        "payload":{"uploadId":upload.to_base64url()}
                    }),
                )
                .await;
                let (code, _) = next_close(&mut f.peers[0], Duration::from_secs(5)).await;
                assert_eq!(
                    code,
                    match fault {
                        UploadRouteFault::ActorRevoked => 4410,
                        UploadRouteFault::ActorStorageUnavailable => 4500,
                        UploadRouteFault::UploadStorageUnavailable => unreachable!(),
                    },
                    "fault source {fault:?}"
                );
                let _ = f.peers[0].close(None).await;
            }

            assert_eq!(receipt.hits(), 1, "fault source {fault:?}");
            let other = queue_read_call(
                f,
                1,
                222,
                "input.upload.begin",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "purpose": "submit"
                }),
            )
            .await;
            assert_eq!(
                other["ok"], true,
                "fault source {fault:?} must not affect another connection: {other}"
            );

            timeout(Duration::from_secs(5), async {
                loop {
                    let observation = f.upload_owners[0].observation();
                    let after = management.connection_counts_for_test();
                    if observation.sealed
                        && observation.entries == 0
                        && observation.receiving == 0
                        && after.0 + 1 == before.0
                        && after.1 + 1 == before.1
                        && after.2 + 1 == before.2
                        && after.3 + 1 == before.3
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap_or_else(|_| panic!("fault source {fault:?} did not seal original owner"));
        })
    })
    .await;
}

#[tokio::test]
async fn acpi_r09_typed_fault_sources_map_once_and_preserve_connection_cleanup() {
    for (fault, binary) in [
        (UploadRouteFault::ActorRevoked, false),
        (UploadRouteFault::ActorRevoked, true),
        (UploadRouteFault::ActorStorageUnavailable, false),
        (UploadRouteFault::ActorStorageUnavailable, true),
        (UploadRouteFault::UploadStorageUnavailable, false),
        (UploadRouteFault::UploadStorageUnavailable, true),
    ] {
        r09_fault_cell(fault, binary).await;
    }
}

async fn r06_command_branch(
    action: crate::management_ws::UploadCommandAction,
    boundary: crate::management_ws::UploadCommandBoundary,
) {
    case(|f| {
        Box::pin(async move {
            let upload = if action == crate::management_ws::UploadCommandAction::Begin {
                Id128::new([181; 16])
            } else {
                r06_prepare_for_action(f, action).await
            };
            let before = f.upload_owners[0].observation();
            let barrier = f
                .node
                .as_ref()
                .unwrap()
                .management
                .as_ref()
                .unwrap()
                .pause_next_upload_command(action, boundary);
            let (name, payload) = r06_action_payload(action, upload);
            send_json(
                &mut f.peers[0],
                &serde_json::json!({
                    "v":1,
                    "type":"command",
                    "requestId":Id128::new([190;16]),
                    "action":name,
                    "payload":payload
                }),
            )
            .await;
            timeout(Duration::from_secs(5), barrier.wait_until_entered())
                .await
                .unwrap_or_else(|_| panic!("{action:?} {boundary:?} did not pause"));
            let at_boundary = f.upload_owners[0].observation();
            if boundary == crate::management_ws::UploadCommandBoundary::BeforeCall {
                assert_eq!(at_boundary.entries, before.entries, "{action:?}");
                assert_eq!(at_boundary.receiving, before.receiving, "{action:?}");
            } else if action == crate::management_ws::UploadCommandAction::Begin {
                assert_eq!(at_boundary.entries, 1, "AfterCall Begin committed normally");
                assert_eq!(at_boundary.receiving, 1);
            }
            let shutdown = r06_stop_connection(f, barrier).await;
            timeout(Duration::from_secs(10), shutdown)
                .await
                .expect("Node stop deadline")
                .expect("Node shutdown task")
                .expect("Node stops cleanly");
            r06_wait_owner_clean(f).await;
        })
    })
    .await;
}

#[tokio::test]
async fn acpi_r06_pending_upload_commands_are_cancelled_without_late_execution() {
    use crate::management_ws::{UploadCommandAction, UploadCommandBoundary};

    for action in [
        UploadCommandAction::Begin,
        UploadCommandAction::Finish,
        UploadCommandAction::Status,
        UploadCommandAction::Release,
    ] {
        for boundary in [
            UploadCommandBoundary::BeforeCall,
            UploadCommandBoundary::AfterCall,
        ] {
            if action == UploadCommandAction::Finish && boundary == UploadCommandBoundary::AfterCall
            {
                continue;
            }
            r06_command_branch(action, boundary).await;
        }
    }
}

#[tokio::test]
async fn acpi_r06_finish_after_call_joins_the_one_original_preparation_task() {
    use crate::management_ws::{UploadCommandAction, UploadCommandBoundary};

    let (factory, cpu, _cpu_release) = r06_cpu_factory();
    case_with_factory(factory, |f| {
        Box::pin(async move {
            let upload = r06_prepare_for_action(f, UploadCommandAction::Finish).await;
            let receipt = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .prompt_preparation_receipt()
                .expect("original preparation receipt");
            let barrier = f
                .node
                .as_ref()
                .unwrap()
                .management
                .as_ref()
                .unwrap()
                .pause_next_upload_command(
                    UploadCommandAction::Finish,
                    UploadCommandBoundary::AfterCall,
                );
            let (name, payload) = r06_action_payload(UploadCommandAction::Finish, upload);
            send_json(
                &mut f.peers[0],
                &serde_json::json!({
                    "v":1,
                    "type":"command",
                    "requestId":Id128::new([191;16]),
                    "action":name,
                    "payload":payload
                }),
            )
            .await;
            timeout(Duration::from_secs(5), barrier.wait_until_entered())
                .await
                .expect("finish AfterCall pause");
            cpu.reached().await;
            let before = receipt.observe();
            assert_eq!(before.cpu().len(), 1, "one original preparation receipt");
            assert_eq!(
                before.cpu()[0].phase(),
                crate::application::prompt_preparation::PreparationTaskPhase::Registered
            );

            let shutdown = r06_stop_connection(f, barrier).await;
            cpu.release();
            timeout(Duration::from_secs(10), shutdown)
                .await
                .expect("Node stop deadline")
                .expect("Node shutdown task")
                .expect("Node stops cleanly");
            let report = timeout(Duration::from_secs(10), receipt.join())
                .await
                .expect("preparation join deadline");
            assert!(
                report.observation().all_joined(),
                "finish worker did not actually join: {report:?}"
            );
            assert_eq!(report.observation().cpu().len(), 1);
            r06_wait_owner_clean(f).await;
        })
    })
    .await;
}

#[tokio::test]
async fn acpi_r06_stop_wins_before_bad_acpi_dispatch_and_accepted_ack() {
    case(|f| {
        Box::pin(async move {
            let barrier = f
                .node
                .as_ref()
                .unwrap()
                .management
                .as_ref()
                .unwrap()
                .pause_next_input_dispatch();
            send_binary(&mut f.peers[0], b"ACPImalformed-before-dispatch".to_vec()).await;
            timeout(Duration::from_secs(5), barrier.wait_until_entered())
                .await
                .expect("input dispatch pause");
            let shutdown = r06_stop_connection(f, barrier).await;
            timeout(Duration::from_secs(10), shutdown)
                .await
                .expect("Node stop deadline")
                .expect("Node shutdown task")
                .expect("Node stops cleanly");
            r06_wait_owner_clean(f).await;
        })
    })
    .await;

    case(|f| {
        Box::pin(async move {
            let begin = queue_read_call(
                f,
                0,
                192,
                "input.upload.begin",
                serde_json::json!({"targetId":target(2).encode(),"purpose":"submit"}),
            )
            .await;
            let upload = Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap())
                .expect("upload id");
            let barrier = f
                .node
                .as_ref()
                .unwrap()
                .management
                .as_ref()
                .unwrap()
                .pause_next_input_accepted_before_ack();
            send_binary(&mut f.peers[0], frame(upload, 0, b"[]")).await;
            timeout(Duration::from_secs(5), barrier.wait_until_entered())
                .await
                .expect("accepted-before-ACK pause");
            assert_eq!(f.upload_owners[0].observation().receiving, 1);
            let shutdown = r06_stop_connection(f, barrier).await;
            timeout(Duration::from_secs(10), shutdown)
                .await
                .expect("Node stop deadline")
                .expect("Node shutdown task")
                .expect("Node stops cleanly");
            r06_wait_owner_clean(f).await;
        })
    })
    .await;
}

#[tokio::test]
async fn acpi_r06_format_error_close_precedes_later_stop_and_owner_seal_is_text_then_binary() {
    case(|f| {
        Box::pin(async move {
            let barrier = f
                .node
                .as_ref()
                .unwrap()
                .management
                .as_ref()
                .unwrap()
                .pause_next_input_dispatch();
            send_binary(&mut f.peers[0], b"ACPIbad-format".to_vec()).await;
            timeout(Duration::from_secs(5), barrier.wait_until_entered())
                .await
                .expect("input dispatch pause");
            barrier.release();
            let (code, reason) = next_close(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!((code, reason.as_str()), (4400, "invalid input data"));
            let node = f.node.take().unwrap();
            timeout(Duration::from_secs(10), node.shutdown(None))
                .await
                .expect("Node stop deadline")
                .expect("Node closes after the prior format close");
            r06_wait_owner_clean(f).await;
        })
    })
    .await;

    case(|f| {
        Box::pin(async move {
            let begin = queue_read_call(
                f,
                0,
                193,
                "input.upload.begin",
                serde_json::json!({"targetId":target(2).encode(),"purpose":"submit"}),
            )
            .await;
            let upload = Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap())
                .expect("upload id");
            f.upload_owners[0].seal();
            assert!(f.node.as_ref().unwrap().control.is_ready());
            let status = queue_read_call(
                f,
                0,
                194,
                "input.upload.status",
                serde_json::json!({"uploadId":upload.to_base64url()}),
            )
            .await;
            assert_eq!(status["error"]["code"], "product_not_ready", "{status}");
            assert!(status["result"].is_null());
            send_binary(&mut f.peers[0], frame(upload, 0, b"[]")).await;
            let (code, reason) = next_close(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!((code, reason.as_str()), (4400, "invalid input data"));
            assert!(f.node.as_ref().unwrap().control.is_ready());
            let node = f.node.take().unwrap();
            timeout(Duration::from_secs(10), node.shutdown(None))
                .await
                .expect("Node stop deadline")
                .expect("Node closes after owner-local seal");
            r06_wait_owner_clean(f).await;
        })
    })
    .await;
}

#[tokio::test]
async fn acpi_r02_last_block_retry_offset_conflicts_and_finished_data_close_once() {
    case(|f| {
        Box::pin(async move {
            let upload = acpi_begin(f, 0, 201).await;
            let first = b"first-last-block";
            send_binary(&mut f.peers[0], frame(upload, 0, first)).await;
            let first_ack = next_binary(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!(
                first_ack.as_slice(),
                crate::management_ws::input_upload::InputAck::received(upload, first.len() as u64)
                    .encode()
                    .as_slice()
            );

            send_binary(&mut f.peers[0], frame(upload, 0, first)).await;
            let retried = next_binary(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!(retried, first_ack, "retry returns the original ACK");
            let status = acpi_status(f, 0, 202, upload).await;
            assert_eq!(status["ok"], true, "{status}");
            assert_eq!(status["result"]["phase"], "receiving");
            assert_eq!(
                status["result"]["nextOffset"],
                first.len().to_string(),
                "retry does not append the body"
            );
            assert_eq!(
                f.upload_owners[0].observation().receiving,
                1,
                "exact previous-block retry stays on the original owner entry"
            );
        })
    })
    .await;

    for (offset, payload, label) in [
        (0u64, b"conflicting".as_slice(), "same-offset conflict"),
        (
            18,
            b"skipped-offset".as_slice(),
            "skipped cumulative offset",
        ),
    ] {
        case(|f| {
            Box::pin(async move {
                let upload = acpi_begin(f, 0, 210).await;
                let first = b"owner-offset-canary";
                send_binary(&mut f.peers[0], frame(upload, 0, first)).await;
                assert_eq!(
                    next_binary(&mut f.peers[0], Duration::from_secs(5))
                        .await
                        .len(),
                    32
                );
                send_binary(&mut f.peers[0], frame(upload, offset, payload)).await;
                let (code, reason) = next_close(&mut f.peers[0], Duration::from_secs(5)).await;
                assert_eq!((code, reason.as_str()), (4400, "invalid input data"));
                timeout(Duration::from_secs(5), async {
                    loop {
                        let observation = f.upload_owners[0].observation();
                        if observation.sealed
                            && observation.entries == 0
                            && observation.receiving == 0
                        {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap_or_else(|_| panic!("{label}: owner did not seal"));
            })
        })
        .await;
    }

    case(|f| {
        Box::pin(async move {
            let upload = acpi_begin(f, 0, 220).await;
            let payload = b"[]";
            send_binary(&mut f.peers[0], frame(upload, 0, payload)).await;
            assert_eq!(
                next_binary(&mut f.peers[0], Duration::from_secs(5))
                    .await
                    .len(),
                32
            );
            let finish = acpi_finish(f, 0, 221, upload, payload.len() as u64).await;
            assert_eq!(finish["ok"], true, "{finish}");
            send_binary(
                &mut f.peers[0],
                frame(upload, payload.len() as u64, b"late"),
            )
            .await;
            let (code, reason) = next_close(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!((code, reason.as_str()), (4400, "invalid input data"));
            timeout(Duration::from_secs(5), async {
                while !f.upload_owners[0].observation().sealed {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("finished upload owner seals after late Data close");
        })
    })
    .await;
}

#[tokio::test]
async fn acpi_r03_finish_preparing_real_ready_failed_and_length_mismatch_join_original_worker() {
    let (factory, cpu, _cpu_release) = r06_cpu_factory();
    case_with_factory(factory, |f| {
        Box::pin(async move {
            let upload = acpi_begin(f, 0, 230).await;
            let body = br#"[{"type":"text","text":"ACPI R03 real worker"}]"#;
            send_binary(&mut f.peers[0], frame(upload, 0, body)).await;
            assert_eq!(
                next_binary(&mut f.peers[0], Duration::from_secs(5))
                    .await
                    .len(),
                32
            );

            let finish = acpi_finish(f, 0, 231, upload, body.len() as u64).await;
            assert_eq!(finish["ok"], true, "{finish}");
            assert_eq!(
                finish["result"],
                serde_json::json!({"uploadId":upload.to_base64url()}),
                "finish must not claim ready"
            );
            cpu.reached().await;

            let preparing = acpi_status(f, 0, 232, upload).await;
            assert_eq!(preparing["ok"], true, "{preparing}");
            assert_eq!(preparing["result"]["phase"], "preparing");
            assert_eq!(preparing["result"]["nextOffset"], body.len().to_string());
            assert!(preparing["result"].get("bodyId").is_none());
            assert!(preparing["result"].get("error").is_none());

            let receipt = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .prompt_preparation_receipt()
                .expect("original preparation receipt");
            let cpu_identity = receipt.observe().cpu()[0].identity().clone();
            assert_eq!(receipt.observe().cpu().len(), 1);
            cpu.release();

            let ready = acpi_wait_terminal(f, 0, 233, upload, "ready").await;
            assert!(ready["result"]["bodyId"].as_str().is_some(), "{ready}");
            assert!(ready["result"].get("error").is_none(), "{ready}");
            let report = timeout(Duration::from_secs(10), receipt.join())
                .await
                .expect("preparation join deadline");
            assert!(
                report.observation().all_joined(),
                "original preparation worker did not join: {report:?}"
            );
            assert!(
                cpu_identity.same(report.observation().cpu()[0].identity()),
                "join did not report the CPU task that entered the barrier"
            );
        })
    })
    .await;

    case(|f| {
        Box::pin(async move {
            let malformed = acpi_begin(f, 0, 240).await;
            send_binary(&mut f.peers[0], frame(malformed, 0, b"[")).await;
            assert_eq!(
                next_binary(&mut f.peers[0], Duration::from_secs(5))
                    .await
                    .len(),
                32
            );
            let finish = acpi_finish(f, 0, 241, malformed, 1).await;
            assert_eq!(finish["ok"], true, "{finish}");
            let failed = acpi_wait_terminal(f, 0, 242, malformed, "failed").await;
            assert_eq!(failed["result"]["error"], "invalid_request", "{failed}");
            assert!(failed["result"].get("bodyId").is_none(), "{failed}");
        })
    })
    .await;

    case(|f| {
        Box::pin(async move {
            let upload = acpi_begin(f, 0, 250).await;
            let body = b"[]";
            send_binary(&mut f.peers[0], frame(upload, 0, body)).await;
            assert_eq!(
                next_binary(&mut f.peers[0], Duration::from_secs(5))
                    .await
                    .len(),
                32
            );
            let mismatch = acpi_finish(f, 0, 251, upload, body.len() as u64 + 1).await;
            assert_eq!(mismatch["ok"], false, "{mismatch}");
            assert_eq!(
                mismatch["error"]["code"], "invalid_upload_state",
                "{mismatch}"
            );
            let status = acpi_status(f, 0, 252, upload).await;
            assert_eq!(status["result"]["phase"], "receiving", "{status}");
            assert_eq!(status["result"]["nextOffset"], body.len().to_string());
            assert_eq!(
                f.upload_owners[0].observation().receiving,
                1,
                "length mismatch does not take the original receiving buffer"
            );
            let receipt = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .prompt_preparation_receipt()
                .unwrap();
            assert!(
                receipt.observe().cpu().is_empty(),
                "length mismatch must not create a preparation receipt"
            );
        })
    })
    .await;
}

#[tokio::test]
async fn acpi_r04_release_foreign_binary_and_disconnected_owner_are_isolated() {
    case(|f| {
        Box::pin(async move {
            let a = acpi_begin(f, 0, 170).await;
            let b = acpi_begin(f, 1, 171).await;
            assert_ne!(a, b);

            let released = queue_read_call(
                f,
                0,
                172,
                "input.upload.release",
                serde_json::json!({"uploadId":a.to_base64url()}),
            )
            .await;
            assert_eq!(released["ok"], true, "{released}");
            assert_eq!(released["result"], serde_json::json!({"released":true}));
            let after_release = acpi_status(f, 0, 173, a).await;
            assert_eq!(after_release["error"]["code"], "body_unavailable");
            let repeated = queue_read_call(
                f,
                0,
                174,
                "input.upload.release",
                serde_json::json!({"uploadId":a.to_base64url()}),
            )
            .await;
            assert_eq!(repeated["ok"], true, "{repeated}");
            assert_eq!(repeated["result"], serde_json::json!({"released":true}));
            let a_live = acpi_begin(f, 0, 175).await;

            let unknown = Id128::new([0xee; 16]);
            for (socket, marker, id) in [(0, 176, unknown), (1, 177, a), (1, 178, unknown)] {
                let status = acpi_status(f, socket, marker, id).await;
                assert_eq!(status["error"]["code"], "body_unavailable", "{status}");
            }
            for (socket, marker, id) in [(0, 179, unknown), (1, 180, a)] {
                let response = queue_read_call(
                    f,
                    socket,
                    marker,
                    "input.upload.release",
                    serde_json::json!({"uploadId":id.to_base64url()}),
                )
                .await;
                assert_eq!(response["error"]["code"], "body_unavailable", "{response}");
            }

            send_binary(&mut f.peers[1], frame(a_live, 0, b"[]")).await;
            let (code, reason) = next_close(&mut f.peers[1], Duration::from_secs(5)).await;
            assert_eq!((code, reason.as_str()), (4400, "invalid input data"));
            let b_owner = f.upload_owners[1].clone();
            timeout(Duration::from_secs(5), async {
                while !b_owner.observation().sealed {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("foreign Data closes only socket B owner");
            assert!(!f.upload_owners[0].observation().sealed);

            send_binary(&mut f.peers[0], frame(a_live, 0, b"[]")).await;
            assert_eq!(
                next_binary(&mut f.peers[0], Duration::from_secs(5))
                    .await
                    .len(),
                32,
                "A remains usable after B's foreign Data"
            );

            let (mut third, third_owner) = r04_new_peer(f).await;
            let third_begin = r04_call_on(
                &mut third,
                181,
                "input.upload.begin",
                serde_json::json!({"targetId":target(2).encode(),"purpose":"submit"}),
            )
            .await;
            assert_eq!(third_begin["ok"], true, "{third_begin}");
            let third_id =
                Id128::parse_base64url(third_begin["result"]["uploadId"].as_str().unwrap())
                    .unwrap();
            let third_status = r04_call_on(
                &mut third,
                182,
                "input.upload.status",
                serde_json::json!({"uploadId":third_id.to_base64url()}),
            )
            .await;
            assert_eq!(third_status["result"]["phase"], "receiving");

            let (mut a2, a2_owner) = r04_new_peer(f).await;
            let a2_begin = r04_call_on(
                &mut a2,
                183,
                "input.upload.begin",
                serde_json::json!({"targetId":target(2).encode(),"purpose":"submit"}),
            )
            .await;
            let a2_id =
                Id128::parse_base64url(a2_begin["result"]["uploadId"].as_str().unwrap()).unwrap();
            a2.send(Message::Binary(frame(a2_id, 0, b"[]").into()))
                .await
                .unwrap();
            assert_eq!(next_binary(&mut a2, Duration::from_secs(5)).await.len(), 32);
            let _ = a2.close(None).await;
            drop(a2);
            timeout(Duration::from_secs(5), async {
                loop {
                    let observation = a2_owner.observation();
                    if observation.sealed && observation.entries == 0 {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("A2 owner seals on disconnect");

            for (marker, action, payload) in [
                (
                    184,
                    "input.upload.status",
                    serde_json::json!({"uploadId":a2_id.to_base64url()}),
                ),
                (
                    185,
                    "input.upload.release",
                    serde_json::json!({"uploadId":a2_id.to_base64url()}),
                ),
            ] {
                let response = r04_call_on(&mut third, marker, action, payload).await;
                assert_eq!(
                    response["error"]["code"], "body_unavailable",
                    "disconnected upload ownership must not transfer: {response}"
                );
            }
            third
                .send(Message::Binary(frame(a2_id, 2, b"[]").into()))
                .await
                .unwrap();
            let (code, reason) = next_close(&mut third, Duration::from_secs(5)).await;
            assert_eq!((code, reason.as_str()), (4400, "invalid input data"));
            let _ = third.close(None).await;
            drop(third);
            timeout(Duration::from_secs(5), async {
                loop {
                    let observation = third_owner.observation();
                    if observation.sealed && observation.entries == 0 {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("third connection owner sealed after its own Format Close");
        })
    })
    .await;
}

#[tokio::test]
async fn acpi_r05_acpv_dispatch_bad_frame_matrix_and_exact_chunk_boundary() {
    case(|f| {
        Box::pin(async move {
            let management = f.node.as_ref().unwrap().management.as_ref().unwrap();
            let observed = management.observe_next_document_socket_for_test().unwrap();
            let key = SigningKey::from_slice(&[69; 32]).unwrap();
            let mut peer = authenticate_socket(
                valid_management_socket(&f.area, f.node.as_ref().unwrap().listen_addr()).await,
                &key,
                &f.client_id,
                node_public_key(f.node.as_ref().unwrap()),
            )
            .await;
            let endpoint = observed.await.unwrap();
            let client = endpoint.client();
            let owner = f
                .node
                .as_ref()
                .unwrap()
                .input_upload_owner(&client)
                .unwrap();
            let (_source_owner, _reads, read, _producer) = r05_source_for_endpoint(
                f,
                &endpoint,
                Arc::from(&b"{\"acpv\":\"credit-only\"}"[..]),
            )
            .await;
            let stream = read.socket_stream().unwrap();
            let dispatch_gate = management.pause_next_input_dispatch();
            send_binary(&mut peer, r05_credit(stream.id())).await;
            timeout(Duration::from_secs(5), async {
                while !stream.ready().unwrap_or(false) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("ACPV credit makes the original view stream ready");
            assert!(
                timeout(
                    Duration::from_millis(200),
                    dispatch_gate.wait_until_entered()
                )
                .await
                .is_err(),
                "a legal ACPV credit must not enter the ACPI upload dispatcher"
            );
            let observation = owner.observation();
            assert_eq!(
                (observation.entries, observation.receiving),
                (0, 0),
                "ACPV credit creates no input upload"
            );
            let _ = peer.close(None).await;
            drop(peer);
        })
    })
    .await;

    let mut version = frame(Id128::new([31; 16]), 0, b"x");
    version[4] = 2;
    let mut reserved = frame(Id128::new([32; 16]), 0, b"x");
    reserved[6] = 1;
    let mut ack_direction = frame(Id128::new([33; 16]), 0, b"x");
    ack_direction[5] = 2;
    let overflow = frame(Id128::new([34; 16]), u64::MAX, b"x");
    let bad_frames = [
        ("four-byte ACPI", b"ACPI".to_vec(), 4400u16),
        (
            "31-byte ACPI header",
            frame(Id128::new([30; 16]), 0, b"x")[..31].to_vec(),
            4400,
        ),
        ("wrong version", version, 4400),
        ("nonzero reserved", reserved, 4400),
        ("ACK direction sent as Data", ack_direction, 4400),
        ("zero payload", frame(Id128::new([35; 16]), 0, b""), 4400),
        (
            "16385-byte chunk",
            frame(Id128::new([36; 16]), 0, &vec![b'x'; 16_385]),
            4400,
        ),
        ("offset overflow", overflow, 4400),
        ("non-ACPI invalid control", b"not-acpi".to_vec(), 4400),
        ("32769-byte binary", vec![b'x'; 32_769], 4429),
    ];
    for (label, bytes, expected_close) in bad_frames {
        case(|f| {
            Box::pin(async move {
                let (mut peer, owner) = r05_new_peer(f).await;
                send_binary(&mut peer, bytes).await;
                let (code, reason) = next_close(&mut peer, Duration::from_secs(5)).await;
                assert_eq!(code, expected_close, "{label}: {reason}");
                timeout(Duration::from_secs(5), async {
                    loop {
                        let observation = owner.observation();
                        if observation.sealed && observation.entries == 0 {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap_or_else(|_| panic!("{label}: original upload owner did not seal"));
            })
        })
        .await;
    }

    case(|f| {
        Box::pin(async move {
            let (mut peer, owner) = r05_new_peer(f).await;
            let begin = r04_call_on(
                &mut peer,
                190,
                "input.upload.begin",
                serde_json::json!({"targetId":target(2).encode(),"purpose":"submit"}),
            )
            .await;
            assert_eq!(begin["ok"], true, "{begin}");
            let upload =
                Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap()).unwrap();
            let empty =
                serde_json::to_vec(&serde_json::json!([{"type":"text","text":""}])).unwrap();
            let payload = serde_json::to_vec(&serde_json::json!([{
                "type":"text",
                "text":"x".repeat(16_384usize.saturating_sub(empty.len()))
            }]))
            .unwrap();
            assert_eq!(payload.len(), 16_384);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&payload).unwrap()[0]["type"],
                "text"
            );
            peer.send(Message::Binary(frame(upload, 0, &payload).into()))
                .await
                .unwrap();
            let ack = next_binary(&mut peer, Duration::from_secs(5)).await;
            assert_eq!(ack.len(), 32);
            assert_eq!(&ack[8..24], upload.as_bytes());
            assert_eq!(&ack[24..], &16_384u64.to_be_bytes());
            let finish = r04_call_on(
                &mut peer,
                191,
                "input.upload.finish",
                serde_json::json!({
                    "uploadId":upload.to_base64url(),
                    "totalBytes":"16384"
                }),
            )
            .await;
            assert_eq!(finish["ok"], true, "{finish}");
            let ready = timeout(Duration::from_secs(10), async {
                let mut marker = 192u8;
                loop {
                    let status = r04_call_on(
                        &mut peer,
                        marker,
                        "input.upload.status",
                        serde_json::json!({"uploadId":upload.to_base64url()}),
                    )
                    .await;
                    marker = marker.checked_add(1).expect("bounded status polling");
                    match status["result"]["phase"].as_str() {
                        Some("ready") => return status,
                        Some("preparing") => tokio::task::yield_now().await,
                        other => panic!("unexpected 16 KiB upload phase: {other:?}: {status}"),
                    }
                }
            })
            .await
            .expect("16 KiB upload reaches ready");
            assert!(ready["result"]["bodyId"].as_str().is_some(), "{ready}");
            assert_eq!(ready["result"]["nextOffset"], "16384", "{ready}");
            assert_eq!(owner.observation().ready, 1);
            let _ = peer.close(None).await;
            drop(peer);
        })
    })
    .await;
}

#[tokio::test]
async fn acpi_r07_writer_fifo_frame_and_byte_saturation_return_exact_budgets() {
    case(|f| {
        Box::pin(async move {
            let (mut peer, _owner, endpoint) = r07_new_peer(f).await;
            let begin = r04_call_on(
                &mut peer,
                196,
                "input.upload.begin",
                serde_json::json!({"targetId":target(2).encode(),"purpose":"submit"}),
            )
            .await;
            let upload =
                Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap()).unwrap();
            let management = f.node.as_ref().unwrap().management.as_ref().unwrap();
            let writer = management.pause_next_write();
            peer.send(Message::Binary(frame(upload, 0, b"[]").into()))
                .await
                .unwrap();
            timeout(Duration::from_secs(5), writer.wait_until_entered())
                .await
                .expect("ACK was dequeued by the original writer");
            assert_eq!(
                endpoint.outbound_observation_for_test(),
                (32, 0),
                "in-flight ACK retains exactly its byte permit after the queue slot returns"
            );
            send_json(
                &mut peer,
                &serde_json::json!({
                    "v":1,
                    "type":"command",
                    "requestId":Id128::new([197;16]),
                    "action":"input.upload.status",
                    "payload":{"uploadId":upload.to_base64url()}
                }),
            )
            .await;
            timeout(Duration::from_secs(5), async {
                while endpoint.outbound_observation_for_test().1 != 1 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("status result queued behind in-flight ACK");
            writer.release();
            let ack = next_binary(&mut peer, Duration::from_secs(5)).await;
            assert_eq!(ack.len(), 32);
            let status = next_json(&mut peer, Duration::from_secs(5)).await;
            assert_eq!(status["result"]["phase"], "receiving", "{status}");
            timeout(Duration::from_secs(5), async {
                while endpoint.outbound_observation_for_test() != (0, 0) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("writer send releases ACK and status permits");
            let _ = peer.close(None).await;
            drop(peer);
        })
    })
    .await;

    case(|f| {
        Box::pin(async move {
            let (mut peer, owner, endpoint) = r07_new_peer(f).await;
            let begin = r04_call_on(
                &mut peer,
                198,
                "input.upload.begin",
                serde_json::json!({"targetId":target(2).encode(),"purpose":"submit"}),
            )
            .await;
            let upload =
                Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap()).unwrap();
            let management = f.node.as_ref().unwrap().management.as_ref().unwrap();
            let writer = management.pause_next_write();
            let retry = frame(upload, 0, b"[]");
            peer.send(Message::Binary(retry.clone().into()))
                .await
                .unwrap();
            timeout(Duration::from_secs(5), writer.wait_until_entered())
                .await
                .expect("first ACK dequeued and paused before actor gate");
            for _ in 0..256 {
                peer.send(Message::Binary(retry.clone().into()))
                    .await
                    .unwrap();
            }
            timeout(Duration::from_secs(180), async {
                while endpoint.outbound_observation_for_test().1 != 256 {
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            })
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "256 retry ACKs fill the original frame queue: {:?}",
                    endpoint.outbound_observation_for_test()
                )
            });
            assert_eq!(endpoint.outbound_observation_for_test(), (8224, 256));
            let writer_closes = management.writer_close_count_for_test();
            peer.send(Message::Binary(retry.into())).await.unwrap();
            timeout(Duration::from_secs(15), async {
                while management.writer_close_count_for_test() == writer_closes {
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            })
            .await
            .expect("frame overflow enters the original writer close path");
            writer.release();
            assert_eq!(
                r07_ack_count_until_close(&mut peer).await,
                1,
                "only the already dequeued ACK may win over close; queued duplicates drop"
            );
            timeout(Duration::from_secs(15), async {
                loop {
                    let observation = owner.observation();
                    if endpoint.outbound_observation_for_test() == (0, 0)
                        && observation.sealed
                        && observation.entries == 0
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("frame saturation close releases writer permits and seals owner");
        })
    })
    .await;

    case(|f| {
        Box::pin(async move {
            let (mut peer, owner, endpoint) = r07_new_peer(f).await;
            let begin = r04_call_on(
                &mut peer,
                199,
                "input.upload.begin",
                serde_json::json!({"targetId":target(2).encode(),"purpose":"submit"}),
            )
            .await;
            let upload =
                Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap()).unwrap();
            let held = endpoint.hold_outbound_bytes_below_ack_for_test();
            peer.send(Message::Binary(frame(upload, 0, b"[]").into()))
                .await
                .unwrap();
            let (code, reason) = next_close(&mut peer, Duration::from_secs(5)).await;
            assert_eq!((code, reason.as_str()), (4429, "limit exceeded"));
            drop(held);
            timeout(Duration::from_secs(5), async {
                loop {
                    let observation = owner.observation();
                    if endpoint.outbound_observation_for_test() == (0, 0)
                        && observation.sealed
                        && observation.entries == 0
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("byte saturation releases held permits and seals the owner");
        })
    })
    .await;
}

#[tokio::test]
async fn acpi_r08_revoke_before_or_after_writer_gate_keeps_linear_order_and_targets_isolated() {
    case(|f| {
        Box::pin(async move {
            let unknown = queue_read_call(
                f,
                0,
                189,
                "input.upload.begin",
                serde_json::json!({"targetId":target(99).encode(),"purpose":"submit"}),
            )
            .await;
            assert_eq!(unknown["error"]["code"], "invalid_target", "{unknown}");
            let malformed = queue_read_call(
                f,
                0,
                188,
                "input.upload.begin",
                serde_json::json!({"targetId":"not-an-id","purpose":"submit"}),
            )
            .await;
            assert_eq!(malformed["error"]["code"], "invalid_request", "{malformed}");
        })
    })
    .await;

    case(|f| {
        Box::pin(async move {
            let management = f.node.as_ref().unwrap().management.as_ref().unwrap();
            let _ = f.peers[1].close(None).await;
            timeout(Duration::from_secs(5), async {
                while management.connection_counts_for_test() != (2, 2, 2, 2) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("extra fixture socket joins before the R08 linear-order case");
            let a = acpi_begin(f, 0, 186).await;
            let management = f.node.as_ref().unwrap().management.as_ref().unwrap();
            let before = management.connection_counts_for_test();
            let writer = management.pause_next_write();
            send_binary(&mut f.peers[0], frame(a, 0, b"[]")).await;
            timeout(Duration::from_secs(5), writer.wait_until_entered())
                .await
                .expect("A ACK dequeued before actor gate");
            let revoked = ipc::device_revoke(
                f.node.as_ref().unwrap().ipc_address(),
                &f.client_id,
            )
            .await
            .expect("revoke transaction commits before writer gate");
            assert_eq!(revoked.client_id, f.client_id);
            writer.release();
            let (code, reason) = next_close(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!((code, reason.as_str()), (4410, "revoked"));
            let mut last_counts = before;
            let mut last_observation = f.upload_owners[0].observation();
            let joined = timeout(Duration::from_secs(5), async {
                loop {
                    let observation = f.upload_owners[0].observation();
                    let counts = f
                        .node
                        .as_ref()
                        .unwrap()
                        .management
                        .as_ref()
                        .unwrap()
                        .connection_counts_for_test();
                    last_counts = counts;
                    last_observation = observation;
                    if observation.sealed
                        && observation.entries == 0
                        && observation.receiving == 0
                        && counts.0 + 1 == before.0
                        && counts.1 + 1 == before.1
                        && counts.2 + 1 == before.2
                        && counts.3 + 1 == before.3
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await;
            assert!(
                joined.is_ok(),
                "A protocol and writer joined before owner cleanup evidence: before={before:?}, after={last_counts:?}, owner={last_observation:?}"
            );

            let b = acpi_begin(f, 2, 185).await;
            send_binary(&mut f.peers[2], frame(b, 0, b"[]")).await;
            assert_eq!(
                next_binary(&mut f.peers[2], Duration::from_secs(5)).await.len(),
                32
            );
            let status = acpi_status(f, 2, 184, b).await;
            assert_eq!(status["result"]["phase"], "receiving", "{status}");
        })
    })
    .await;

    case(|f| {
        Box::pin(async move {
            let a = acpi_begin(f, 0, 183).await;
            let barrier = f
                .node
                .as_ref()
                .unwrap()
                .management
                .as_ref()
                .unwrap()
                .pause_next_boundary(crate::management_ws::ManagementBoundary::WriteLocked);
            send_binary(&mut f.peers[0], frame(a, 0, b"[]")).await;
            timeout(Duration::from_secs(5), barrier.wait_until_entered())
                .await
                .expect("ACK holds actor gate");
            let mut revoke = Box::pin(ipc::device_revoke(
                f.node.as_ref().unwrap().ipc_address(),
                &f.client_id,
            ));
            assert!(
                timeout(Duration::from_millis(100), &mut revoke)
                    .await
                    .is_err(),
                "revoke cannot commit across a sender that already owns the actor gate"
            );
            barrier.release();
            let ack = next_binary(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!(ack.len(), 32);
            timeout(Duration::from_secs(5), &mut revoke)
                .await
                .expect("revoke commits after original send")
                .expect("revoke transaction");
            drop(revoke);
            let (code, reason) = next_close(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!((code, reason.as_str()), (4410, "revoked"));

            let b = acpi_begin(f, 2, 182).await;
            send_binary(&mut f.peers[2], frame(b, 0, b"[]")).await;
            assert_eq!(
                next_binary(&mut f.peers[2], Duration::from_secs(5))
                    .await
                    .len(),
                32
            );
        })
    })
    .await;
}

#[tokio::test]
async fn acpi_r01_real_wss_upload_ack_ready_body_and_queue_share_original_bundle() {
    case(|f| {
        Box::pin(async move {
            let begin = queue_read_call(
                f,
                0,
                200,
                "input.upload.begin",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "purpose": "submit"
                }),
            )
            .await;
            assert_eq!(begin["ok"], true, "{begin}");
            assert_eq!(begin["result"].as_object().unwrap().len(), 1);
            let upload = Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap())
                .expect("upload id");

            let status = queue_read_call(
                f,
                0,
                201,
                "input.upload.status",
                serde_json::json!({"uploadId": upload.to_base64url()}),
            )
            .await;
            assert_eq!(status["ok"], true, "{status}");
            assert_eq!(status["result"]["phase"], "receiving");
            assert_eq!(status["result"]["nextOffset"], "0");
            assert!(status["result"].get("bodyId").is_none());
            assert!(status["result"].get("error").is_none());

            for (marker, action, payload) in [
                (
                    202,
                    "input.upload.begin",
                    serde_json::json!({
                        "targetId": target(2).encode(),
                        "purpose": "other"
                    }),
                ),
                (
                    203,
                    "input.upload.begin",
                    serde_json::json!({
                        "targetId": target(2).encode(),
                        "purpose": "submit",
                        "extra": true
                    }),
                ),
                (
                    204,
                    "input.upload.finish",
                    serde_json::json!({
                        "uploadId": upload.to_base64url(),
                        "totalBytes": 0
                    }),
                ),
            ] {
                let invalid = queue_read_call(f, 0, marker, action, payload).await;
                assert_eq!(invalid["ok"], false, "{invalid}");
                assert_eq!(invalid["error"]["code"], "invalid_request");
                assert!(invalid["result"].is_null());
            }

            let payload = br#"[{"type":"text","text":"ACPI real WSS prompt"}]"#;
            send_binary(&mut f.peers[0], frame(upload, 0, payload)).await;
            let ack = next_binary(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!(ack.len(), 32);
            assert_eq!(&ack[..8], b"ACPI\x01\x02\0\0");
            assert_eq!(&ack[8..24], upload.as_bytes());
            assert_eq!(&ack[24..], &(payload.len() as u64).to_be_bytes());

            let status = queue_read_call(
                f,
                0,
                205,
                "input.upload.status",
                serde_json::json!({"uploadId": upload.to_base64url()}),
            )
            .await;
            assert_eq!(status["result"]["phase"], "receiving");
            assert_eq!(status["result"]["nextOffset"], payload.len().to_string());

            let finish = queue_read_call(
                f,
                0,
                206,
                "input.upload.finish",
                serde_json::json!({
                    "uploadId": upload.to_base64url(),
                    "totalBytes": payload.len().to_string()
                }),
            )
            .await;
            assert_eq!(finish["ok"], true, "{finish}");
            assert_eq!(finish["result"]["uploadId"], upload.to_base64url());
            assert_eq!(finish["result"].as_object().unwrap().len(), 1);

            let mut marker = 207;
            let ready = loop {
                let status = queue_read_call(
                    f,
                    0,
                    marker,
                    "input.upload.status",
                    serde_json::json!({"uploadId": upload.to_base64url()}),
                )
                .await;
                marker = marker.checked_add(1).expect("bounded status polling");
                match status["result"]["phase"].as_str() {
                    Some("ready") => break status,
                    Some("preparing") => tokio::task::yield_now().await,
                    other => panic!("unexpected upload phase: {other:?}: {status}"),
                }
            };
            let body = ready["result"]["bodyId"]
                .as_str()
                .expect("ready body id")
                .to_owned();
            assert_eq!(ready["result"]["nextOffset"], payload.len().to_string());
            assert!(ready["result"].get("error").is_none());

            let registration = queue_read_call(
                f,
                0,
                240,
                "input.register",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "operation": {"kind": "submit", "bodyId": body}
                }),
            )
            .await;
            assert_eq!(registration["ok"], true, "{registration}");
            let registration_id = registration["result"]["registrationId"]
                .as_str()
                .unwrap()
                .to_owned();
            let consumed = queue_read_call(
                f,
                0,
                241,
                "input.consume",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "registrationId": registration_id,
                    "bodyId": body
                }),
            )
            .await;
            assert_eq!(consumed["ok"], true, "{consumed}");
            assert_eq!(consumed["result"]["state"], "settled");
            assert_eq!(consumed["result"]["request"]["outcome"]["kind"], "queued");
            let queue = queue_read_call(
                f,
                0,
                242,
                "input.queue",
                serde_json::json!({"targetId": target(2).encode()}),
            )
            .await;
            assert_eq!(queue["ok"], true, "{queue}");
            assert_eq!(queue["result"]["waiting"].as_array().unwrap().len(), 1);
            assert_eq!(queue["result"]["waiting"][0]["itemId"], registration_id);
        })
    })
    .await;
}

#[tokio::test]
async fn catalog_wss_actions_use_the_authenticated_original_owner_end_to_end() {
    case(|f| {
        Box::pin(async move {
            let released_upload = {
                let begin = queue_read_call(
                    f,
                    0,
                    220,
                    "catalog.upload.begin",
                    serde_json::json!({}),
                )
                .await;
                assert_eq!(begin["ok"], true, "{begin}");
                assert_eq!(begin["result"].as_object().unwrap().len(), 1);
                Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap())
                    .expect("catalog upload id")
            };
            let released = queue_read_call(
                f,
                0,
                221,
                "catalog.upload.release",
                serde_json::json!({"uploadId":released_upload.to_base64url()}),
            )
            .await;
            assert_eq!(released["ok"], true, "{released}");
            assert_eq!(released["result"], serde_json::json!({"released":true}));
            let unavailable = queue_read_call(
                f,
                0,
                222,
                "catalog.upload.status",
                serde_json::json!({"uploadId":released_upload.to_base64url()}),
            )
            .await;
            assert_eq!(unavailable["ok"], false, "{unavailable}");
            assert_eq!(unavailable["error"]["code"], "body_unavailable");

            let executable = catalog_utf16(r"C:\Windows\System32\cmd.exe");
            let document = format!(
                r#"{{"kind":"install_harness","name":"socket catalog route","source":{{"kind":"new","name":"socket catalog source"}},"profile":{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":[],"environment":[]}},"recipes":[{{"wire":2,"mcp_servers":[],"additional_directories":[],"extra":{{}}}}]}}}}"#,
                executable
            );
            let document = document.as_bytes();
            let upload = {
                let begin = queue_read_call(
                    f,
                    0,
                    223,
                    "catalog.upload.begin",
                    serde_json::json!({}),
                )
                .await;
                assert_eq!(begin["ok"], true, "{begin}");
                Id128::parse_base64url(begin["result"]["uploadId"].as_str().unwrap())
                    .expect("catalog upload id")
            };
            let status = queue_read_call(
                f,
                0,
                224,
                "catalog.upload.status",
                serde_json::json!({"uploadId":upload.to_base64url()}),
            )
            .await;
            assert_eq!(status["ok"], true, "{status}");
            assert_eq!(status["result"]["phase"], "receiving");
            assert_eq!(status["result"]["nextOffset"], "0");

            send_binary(&mut f.peers[0], frame(upload, 0, document)).await;
            let ack = next_binary(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!(ack.len(), 32);
            assert_eq!(&ack[..8], b"ACPI\x01\x02\0\0");
            assert_eq!(&ack[8..24], upload.as_bytes());
            assert_eq!(&ack[24..], &(document.len() as u64).to_be_bytes());

            let finish = queue_read_call(
                f,
                0,
                225,
                "catalog.upload.finish",
                serde_json::json!({
                    "uploadId":upload.to_base64url(),
                    "totalBytes":document.len().to_string()
                }),
            )
            .await;
            assert_eq!(finish["ok"], true, "{finish}");
            assert_eq!(finish["result"], serde_json::json!({"uploadId":upload.to_base64url()}));

            let ready = {
                let mut marker = 226;
                loop {
                    let status = queue_read_call(
                        f,
                        0,
                        marker,
                        "catalog.upload.status",
                        serde_json::json!({"uploadId":upload.to_base64url()}),
                    )
                    .await;
                    marker = marker.checked_add(1).expect("bounded status polling");
                    match status["result"]["phase"].as_str() {
                        Some("ready") => break status,
                        Some("preparing") => tokio::task::yield_now().await,
                        other => panic!("unexpected catalog upload phase: {other:?}: {status}"),
                    }
                }
            };
            assert_eq!(ready["result"]["nextOffset"], document.len().to_string());
            assert!(ready["result"].get("error").is_none());

            let registered = queue_read_call(
                f,
                0,
                230,
                "catalog.register",
                serde_json::json!({"documentUploadId":upload.to_base64url()}),
            )
            .await;
            assert_eq!(registered["ok"], true, "{registered}");
            assert_eq!(registered["result"]["state"], "registered");
            let request = registered["result"]["catalogRequestId"]
                .as_str()
                .expect("catalog request id")
                .to_owned();

            let applied = queue_read_call(
                f,
                0,
                231,
                "catalog.apply",
                serde_json::json!({"catalogRequestId":request}),
            )
            .await;
            assert_eq!(applied["ok"], true, "{applied}");
            assert_eq!(applied["result"]["fact"]["catalogRequestId"], request);
            assert_eq!(applied["result"]["fact"]["action"], "install_harness");
            assert_eq!(applied["result"]["fact"]["state"], "committed");
            assert_eq!(
                applied["result"]["fact"]["result"]["kind"],
                "harness"
            );
            let receipt = applied["result"].to_string();
            assert!(!receipt.contains("socket catalog source"));
            assert!(!receipt.contains("cmd.exe"));

            let queried = queue_read_call(
                f,
                0,
                232,
                "catalog.request",
                serde_json::json!({"catalogRequestId":request}),
            )
            .await;
            assert_eq!(queried["ok"], true, "{queried}");
            assert_eq!(queried["result"]["catalogRequestId"], request);
            assert_eq!(queried["result"]["state"], "committed");
            assert_eq!(queried["result"]["result"]["kind"], "harness");
        })
    })
    .await;
}

#[tokio::test]
async fn input_body_open_streams_original_queue_bytes_only_with_read_credit() {
    case(|f| {
        Box::pin(async move {
            let raw = br#"[{"type":"text","text":"open the original queued body"}]"#;
            let body_id = body(f, UploadPurpose::Submit, raw).await;
            let registration = queue_read_call(
                f,
                0,
                245,
                "input.register",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "operation": {"kind": "submit", "bodyId": body_id.to_base64url()}
                }),
            )
            .await;
            assert_eq!(registration["ok"], true, "{registration}");
            let registration_id = registration["result"]["registrationId"]
                .as_str()
                .expect("registration id")
                .to_owned();
            let consumed = queue_read_call(
                f,
                0,
                246,
                "input.consume",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "registrationId": registration_id,
                    "bodyId": body_id.to_base64url()
                }),
            )
            .await;
            assert_eq!(consumed["result"]["state"], "settled", "{consumed}");

            let queue = queue_read_call(
                f,
                0,
                247,
                "input.queue",
                serde_json::json!({"targetId": target(2).encode()}),
            )
            .await;
            let item = &queue["result"]["waiting"][0];
            let opened = queue_read_call(
                f,
                0,
                248,
                "input.body.open",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "itemId": item["itemId"],
                    "expectedRevision": item["revision"]
                }),
            )
            .await;
            assert_eq!(opened["ok"], true, "{opened}");
            assert_eq!(opened["result"]["streamKind"], "queued_input_body");
            assert_eq!(opened["result"]["targetId"], target(2).encode());
            assert_eq!(
                opened["result"]["revision"], item["revision"],
                "the stream is pinned to the observed queue revision"
            );
            assert_eq!(opened["result"]["totalBytes"], raw.len().to_string());
            assert_eq!(opened["result"]["encoding"], "utf8_json");
            let stream =
                Id128::parse_base64url(opened["result"]["streamId"].as_str().expect("stream id"))
                    .expect("typed stream id");

            send_binary(
                &mut f.peers[0],
                TransferHeader::credit(stream, 0)
                    .encode(&[])
                    .expect("credit"),
            )
            .await;
            let frame = next_binary(&mut f.peers[0], Duration::from_secs(5)).await;
            let (header, payload) = TransferHeader::decode(&frame).expect("document chunk");
            assert_eq!(header.stream(), stream);
            assert_eq!(header.packet(), 1);
            assert_eq!(header.offset(), 0);
            assert_eq!(header.total(), raw.len() as u64);
            assert_eq!(
                header.record_kind(),
                Some(crate::application::shared_views::wire::RecordKind::OriginalRaw)
            );
            assert_eq!(payload, raw);

            send_binary(
                &mut f.peers[0],
                TransferHeader::detach(stream).encode(&[]).expect("detach"),
            )
            .await;
        })
    })
    .await;
}

#[tokio::test]
async fn input_body_open_rejects_stale_or_unknown_items_without_body() {
    case(|f| {
        Box::pin(async move {
            let raw = br#"[{"type":"text","text":"private queued canary"}]"#;
            let body_id = body(f, UploadPurpose::Submit, raw).await;
            let registration = queue_read_call(
                f,
                0,
                253,
                "input.register",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "operation": {"kind": "submit", "bodyId": body_id.to_base64url()}
                }),
            )
            .await;
            let registration_id = registration["result"]["registrationId"]
                .as_str()
                .expect("registration id")
                .to_owned();
            let consumed = queue_read_call(
                f,
                0,
                254,
                "input.consume",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "registrationId": registration_id,
                    "bodyId": body_id.to_base64url()
                }),
            )
            .await;
            assert_eq!(consumed["result"]["state"], "settled", "{consumed}");
            let queue = queue_read_call(
                f,
                0,
                255,
                "input.queue",
                serde_json::json!({"targetId": target(2).encode()}),
            )
            .await;
            let item = &queue["result"]["waiting"][0];
            let item_id = item["itemId"].as_str().expect("queue item id").to_owned();
            let revision = item["revision"]
                .as_str()
                .expect("queue revision string")
                .parse::<u64>()
                .expect("queue revision");

            let stale = queue_read_call(
                f,
                0,
                41,
                "input.body.open",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "itemId": item_id,
                    "expectedRevision": revision.saturating_add(1).to_string()
                }),
            )
            .await;
            assert_eq!(stale["error"]["code"], "invalid_revision", "{stale}");

            let missing_item = Id128::new([222; 16]).to_base64url();
            let missing = queue_read_call(
                f,
                0,
                42,
                "input.body.open",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "itemId": missing_item,
                    "expectedRevision": revision.to_string()
                }),
            )
            .await;
            assert_eq!(missing["error"]["code"], "invalid_item", "{missing}");

            let wrong_target = queue_read_call(
                f,
                0,
                43,
                "input.body.open",
                serde_json::json!({
                    "targetId": target(3).encode(),
                    "itemId": item_id,
                    "expectedRevision": revision.to_string()
                }),
            )
            .await;
            assert_eq!(
                wrong_target["error"]["code"], "invalid_item",
                "{wrong_target}"
            );
            let exposed = format!("{stale}{missing}{wrong_target}");
            assert!(
                !exposed.contains("private queued canary"),
                "failed opens must not expose queued content: {exposed}"
            );

            let opened = queue_read_call(
                f,
                0,
                44,
                "input.body.open",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "itemId": item_id,
                    "expectedRevision": revision.to_string()
                }),
            )
            .await;
            assert_eq!(opened["ok"], true, "{opened}");
            assert_eq!(opened["result"]["totalBytes"], raw.len().to_string());
        })
    })
    .await;
}

#[tokio::test]
async fn input_body_open_binds_reads_to_original_connection_and_credit() {
    case(|f| {
        Box::pin(async move {
            let raw = br#"[{"type":"text","text":"owner connection only"}]"#;
            let body_id = body(f, UploadPurpose::Submit, raw).await;
            let registration = queue_read_call(
                f,
                0,
                61,
                "input.register",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "operation": {"kind": "submit", "bodyId": body_id.to_base64url()}
                }),
            )
            .await;
            let registration_id = registration["result"]["registrationId"]
                .as_str()
                .expect("registration id")
                .to_owned();
            let consumed = queue_read_call(
                f,
                0,
                62,
                "input.consume",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "registrationId": registration_id,
                    "bodyId": body_id.to_base64url()
                }),
            )
            .await;
            assert_eq!(consumed["result"]["state"], "settled", "{consumed}");
            let queue = queue_read_call(
                f,
                0,
                63,
                "input.queue",
                serde_json::json!({"targetId": target(2).encode()}),
            )
            .await;
            let item = &queue["result"]["waiting"][0];
            let opened = queue_read_call(
                f,
                0,
                64,
                "input.body.open",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "itemId": item["itemId"],
                    "expectedRevision": item["revision"]
                }),
            )
            .await;
            assert_eq!(opened["ok"], true, "{opened}");
            let stream =
                Id128::parse_base64url(opened["result"]["streamId"].as_str().expect("stream id"))
                    .expect("typed stream id");

            let quiet = timeout(Duration::from_millis(200), async {
                loop {
                    match f.peers[0]
                        .next()
                        .await
                        .expect("WebSocket frame")
                        .expect("WebSocket")
                    {
                        Message::Ping(_) | Message::Pong(_) => {}
                        Message::Binary(bytes) => {
                            panic!("body sent without credit: {}", bytes.len())
                        }
                        Message::Text(text) => {
                            panic!("unexpected protocol frame without credit: {text}")
                        }
                        Message::Close(frame) => {
                            panic!("unexpected close without credit: {frame:?}")
                        }
                        Message::Frame(_) => panic!("unexpected raw frame without credit"),
                    }
                }
            })
            .await;
            assert!(quiet.is_err(), "the original reader must wait for credit");

            for socket in [1, 2] {
                send_binary(
                    &mut f.peers[socket],
                    TransferHeader::credit(stream, 0)
                        .encode(&[])
                        .expect("foreign credit"),
                )
                .await;
                let close = next_close(&mut f.peers[socket], Duration::from_secs(5)).await;
                assert_eq!(close, (4400, "invalid view control".to_owned()));
            }

            send_binary(
                &mut f.peers[0],
                TransferHeader::credit(stream, 0)
                    .encode(&[])
                    .expect("owner credit"),
            )
            .await;
            let frame = next_binary(&mut f.peers[0], Duration::from_secs(5)).await;
            let (header, payload) = TransferHeader::decode(&frame).expect("owner body chunk");
            assert_eq!(header.stream(), stream);
            assert_eq!(header.packet(), 1);
            assert_eq!(payload, raw);
            send_binary(
                &mut f.peers[0],
                TransferHeader::detach(stream).encode(&[]).expect("detach"),
            )
            .await;
        })
    })
    .await;
}

#[tokio::test]
async fn input_body_open_stops_next_chunk_after_actor_revoke() {
    case(|f| {
        Box::pin(async move {
            let body_id = body(f, UploadPurpose::Submit, b"[]").await;
            let registration = queue_read_call(
                f,
                0,
                249,
                "input.register",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "operation": {"kind": "submit", "bodyId": body_id.to_base64url()}
                }),
            )
            .await;
            let registration_id = registration["result"]["registrationId"]
                .as_str()
                .expect("registration id")
                .to_owned();
            let consumed = queue_read_call(
                f,
                0,
                250,
                "input.consume",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "registrationId": registration_id,
                    "bodyId": body_id.to_base64url()
                }),
            )
            .await;
            assert_eq!(consumed["result"]["state"], "settled", "{consumed}");
            let queue = queue_read_call(
                f,
                0,
                251,
                "input.queue",
                serde_json::json!({"targetId": target(2).encode()}),
            )
            .await;
            let item = &queue["result"]["waiting"][0];
            let opened = queue_read_call(
                f,
                0,
                252,
                "input.body.open",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "itemId": item["itemId"],
                    "expectedRevision": item["revision"]
                }),
            )
            .await;
            assert_eq!(opened["ok"], true, "{opened}");
            let stream =
                Id128::parse_base64url(opened["result"]["streamId"].as_str().expect("stream id"))
                    .expect("typed stream id");

            send_binary(
                &mut f.peers[0],
                TransferHeader::credit(stream, 0)
                    .encode(&[])
                    .expect("initial credit"),
            )
            .await;
            let first = next_binary(&mut f.peers[0], Duration::from_secs(5)).await;
            let (header, payload) = TransferHeader::decode(&first).expect("first body chunk");
            assert_eq!(header.stream(), stream);
            assert_eq!(header.packet(), 1);
            assert_eq!(payload, b"[]");

            let management = f.node.as_ref().unwrap().management.as_ref().unwrap();
            let dispatch = management.pause_next_view_control_dispatch();
            send_binary(
                &mut f.peers[0],
                TransferHeader::credit(stream, 1)
                    .encode(&[])
                    .expect("next credit"),
            )
            .await;
            timeout(Duration::from_secs(5), dispatch.wait_until_entered())
                .await
                .expect("next credit reached original input dispatch");
            let revoked = ipc::device_revoke(f.node.as_ref().unwrap().ipc_address(), &f.client_id)
                .await
                .expect("owner revocation transaction");
            assert_eq!(revoked.client_id, f.client_id);
            dispatch.release();
            let close = next_close(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!(close, (4410, "revoked".to_owned()));
        })
    })
    .await;
}

#[tokio::test]
async fn queue_command_register_consume_is_stable_and_single_dispatch() {
    case(|f| {
        Box::pin(async move {
            let body_id = body(f, UploadPurpose::Submit, b"[]").await;
            let request = Id128::new([120; 16]);
            let payload = serde_json::json!({
                "targetId": target(2).encode(),
                "operation": {"kind": "submit", "bodyId": body_id}
            });

            let first = queue_read_call(f, 0, 120, "input.register", payload.clone()).await;
            assert_eq!(first["ok"], true);
            assert_eq!(first["result"]["registrationId"], request.to_base64url());
            assert_eq!(first["result"]["state"], "registered");

            let retry = queue_read_call(f, 0, 120, "input.register", payload.clone()).await;
            assert_eq!(
                retry, first,
                "same request identity returns the original registration fact"
            );

            let conflict = queue_read_call(
                f,
                0,
                120,
                "input.register",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "operation": {"kind": "resume"}
                }),
            )
            .await;
            assert_eq!(conflict["ok"], false);
            assert_eq!(conflict["error"]["code"], "request_conflict");

            let consume = queue_read_call(
                f,
                0,
                122,
                "input.consume",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "registrationId": request.to_base64url(),
                    "bodyId": body_id
                }),
            )
            .await;
            assert_eq!(consume["ok"], true);
            assert_eq!(consume["result"]["state"], "settled");
            assert_eq!(consume["result"]["request"]["outcome"]["kind"], "queued");
            assert_eq!(consume["result"]["returned"], false);

            let repeat_consume = queue_read_call(
                f,
                0,
                123,
                "input.consume",
                serde_json::json!({
                    "targetId": target(2).encode(),
                    "registrationId": request.to_base64url(),
                    "bodyId": body_id
                }),
            )
            .await;
            assert_eq!(repeat_consume["ok"], true);
            assert_eq!(repeat_consume["result"]["state"], "settled");
            assert_eq!(
                repeat_consume["result"]["request"]["outcome"]["kind"],
                "queued"
            );
            assert_eq!(
                queued(f, 0).await.len(),
                1,
                "retry does not dispatch a second queue item"
            );
        })
    })
    .await;
}

#[tokio::test]
async fn queue_command_wss_applies_edit_cancel_prioritize_and_resume() {
    case(|f| {
        Box::pin(async move {
            let target_id = target(2).encode();
            let mut registrations = Vec::new();
            for marker in [130, 131, 132] {
                let body_id = body(f, UploadPurpose::Submit, b"[]").await;
                let registered = queue_read_call(
                    f,
                    0,
                    marker,
                    "input.register",
                    serde_json::json!({
                        "targetId": target_id,
                        "operation": {"kind": "submit", "bodyId": body_id}
                    }),
                )
                .await;
                assert_eq!(
                    registered["ok"], true,
                    "management error code: {}",
                    registered["error"]["code"]
                );
                let registration = Id128::new([marker; 16]);
                let consumed = queue_read_call(
                    f,
                    0,
                    marker + 10,
                    "input.consume",
                    serde_json::json!({
                        "targetId": target_id,
                        "registrationId": registration.to_base64url(),
                        "bodyId": body_id
                    }),
                )
                .await;
                assert_eq!(
                    consumed["ok"], true,
                    "management error code: {}",
                    consumed["error"]["code"]
                );
                assert_eq!(consumed["result"]["request"]["outcome"]["kind"], "queued");
                registrations.push(registration);
            }

            let edited_body = body_for(
                f,
                2,
                UploadPurpose::Edit,
                b"[{\"type\":\"text\",\"text\":\"edited\"}]",
            )
            .await;
            let edited = queue_read_call(
                f,
                2,
                150,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": {
                        "kind": "edit",
                        "itemId": registrations[0].to_base64url(),
                        "expectedRevision": "1",
                        "bodyId": edited_body
                    }
                }),
            )
            .await;
            assert_eq!(
                edited["ok"], true,
                "edit registration error code: {}",
                edited["error"]["code"]
            );
            let edit_consume = queue_read_call(
                f,
                2,
                160,
                "input.consume",
                serde_json::json!({
                    "targetId": target_id,
                    "registrationId": Id128::new([150; 16]).to_base64url(),
                    "bodyId": edited_body
                }),
            )
            .await;
            assert_eq!(
                edit_consume["ok"], true,
                "management error code: {}",
                edit_consume["error"]["code"]
            );
            assert_eq!(
                edit_consume["result"]["request"]["outcome"]["kind"],
                "edited"
            );

            let cancelled = queue_read_call(
                f,
                2,
                151,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": {
                        "kind": "cancel",
                        "itemId": registrations[1].to_base64url(),
                        "expectedRevision": "1"
                    }
                }),
            )
            .await;
            assert_eq!(
                cancelled["ok"], true,
                "management error code: {}",
                cancelled["error"]["code"]
            );
            let cancel_consume = queue_read_call(
                f,
                2,
                161,
                "input.consume",
                serde_json::json!({
                    "targetId": target_id,
                    "registrationId": Id128::new([151; 16]).to_base64url(),
                    "bodyId": null
                }),
            )
            .await;
            assert_eq!(
                cancel_consume["ok"], true,
                "management error code: {}",
                cancel_consume["error"]["code"]
            );
            assert_eq!(
                cancel_consume["result"]["request"]["outcome"]["kind"],
                "cancelled"
            );

            let prioritized = queue_read_call(
                f,
                2,
                152,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": {
                        "kind": "prioritize",
                        "itemId": registrations[2].to_base64url(),
                        "expectedRevision": "1"
                    }
                }),
            )
            .await;
            assert_eq!(
                prioritized["ok"], true,
                "management error code: {}",
                prioritized["error"]["code"]
            );
            let prioritize_consume = queue_read_call(
                f,
                2,
                162,
                "input.consume",
                serde_json::json!({
                    "targetId": target_id,
                    "registrationId": Id128::new([152; 16]).to_base64url(),
                    "bodyId": null
                }),
            )
            .await;
            assert_eq!(
                prioritize_consume["ok"], true,
                "management error code: {}",
                prioritize_consume["error"]["code"]
            );
            assert_eq!(
                prioritize_consume["result"]["request"]["outcome"]["kind"],
                "prioritized"
            );

            let resumed = queue_read_call(
                f,
                2,
                153,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": {"kind": "resume"}
                }),
            )
            .await;
            assert_eq!(
                resumed["ok"], true,
                "management error code: {}",
                resumed["error"]["code"]
            );
            let resume_consume = queue_read_call(
                f,
                2,
                163,
                "input.consume",
                serde_json::json!({
                    "targetId": target_id,
                    "registrationId": Id128::new([153; 16]).to_base64url(),
                    "bodyId": null
                }),
            )
            .await;
            assert_eq!(
                resume_consume["ok"], true,
                "management error code: {}",
                resume_consume["error"]["code"]
            );
            assert_eq!(
                resume_consume["result"]["request"]["outcome"]["kind"],
                "resumed"
            );

            let snapshot = queue_read_call(
                f,
                0,
                164,
                "input.queue",
                serde_json::json!({"targetId": target_id}),
            )
            .await;
            assert_eq!(
                snapshot["ok"], true,
                "management error code: {}",
                snapshot["error"]["code"]
            );
            assert_eq!(snapshot["result"]["waiting"].as_array().unwrap().len(), 2);
            assert_eq!(
                snapshot["result"]["waiting"][0]["itemId"],
                registrations[0].to_base64url()
            );
            assert_eq!(
                snapshot["result"]["waiting"][1]["itemId"],
                registrations[2].to_base64url()
            );
            assert_eq!(snapshot["result"]["waiting"][0]["revision"], "2");
            assert_eq!(snapshot["result"]["failureSuspended"], false);
            assert_eq!(snapshot["result"]["stopReason"], serde_json::Value::Null);
            assert_eq!(
                snapshot["result"]["priorityFailure"],
                serde_json::Value::Null
            );
            assert!(
                snapshot["result"]["waiting"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|entry| entry["suspended"] == false)
            );
        })
    })
    .await;
}

#[tokio::test]
async fn queue_command_resume_preserves_three_suspended_items_and_same_identity() {
    case(|f| {
        Box::pin(async move {
            let target_id = target(2).encode();
            let mut registrations = Vec::new();
            for marker in [170, 172, 174] {
                let body_id = body(f, UploadPurpose::Submit, b"[]").await;
                let registered = queue_read_call(
                    f,
                    0,
                    marker,
                    "input.register",
                    serde_json::json!({
                        "targetId": target_id,
                        "operation": {"kind": "submit", "bodyId": body_id}
                    }),
                )
                .await;
                assert_eq!(registered["ok"], true);
                let registration = Id128::new([marker; 16]);
                let consumed = queue_read_call(
                    f,
                    0,
                    marker + 1,
                    "input.consume",
                    serde_json::json!({
                        "targetId": target_id,
                        "registrationId": registration.to_base64url(),
                        "bodyId": body_id
                    }),
                )
                .await;
                assert_eq!(consumed["result"]["request"]["outcome"]["kind"], "queued");
                registrations.push(registration);
            }

            let queue = &f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .queue;
            assert_eq!(
                queue
                    .suspend_waiting_for_test(
                        f.clients[0].context.actor(),
                        target(2),
                        crate::domain::native_dispatch::KnownStopReason::MaxTokens,
                    )
                    .await
                    .unwrap(),
                3
            );

            let stopped = queue_read_call(
                f,
                2,
                176,
                "input.queue",
                serde_json::json!({"targetId": target_id}),
            )
            .await;
            assert_eq!(stopped["result"]["stopReason"], "max_tokens");
            assert_eq!(
                stopped["result"]["priorityFailure"],
                serde_json::Value::Null
            );
            assert_eq!(stopped["result"]["failureSuspended"], true);
            let paused = stopped["result"]["waiting"].as_array().unwrap();
            assert_eq!(paused.len(), 3);
            assert!(paused.iter().all(|entry| entry["suspended"] == true));

            let resume_payload = serde_json::json!({
                "targetId": target_id,
                "operation": {"kind": "resume"}
            });
            let registered =
                queue_read_call(f, 2, 177, "input.register", resume_payload.clone()).await;
            assert_eq!(registered["ok"], true);
            let registration_retry =
                queue_read_call(f, 2, 177, "input.register", resume_payload).await;
            assert_eq!(
                registration_retry, registered,
                "reconnect retry keeps the original registration fact"
            );

            let consume_payload = serde_json::json!({
                "targetId": target_id,
                "registrationId": Id128::new([177; 16]).to_base64url(),
                "bodyId": null
            });
            let consumed =
                queue_read_call(f, 2, 178, "input.consume", consume_payload.clone()).await;
            assert_eq!(consumed["result"]["request"]["outcome"]["kind"], "resumed");
            assert_eq!(consumed["result"]["request"]["outcome"]["restored"], "3");
            let consume_retry = queue_read_call(f, 2, 178, "input.consume", consume_payload).await;
            assert_eq!(
                consume_retry, consumed,
                "reconnect retry returns the original consume fact"
            );

            let resumed = queue_read_call(
                f,
                0,
                179,
                "input.queue",
                serde_json::json!({"targetId": target_id}),
            )
            .await;
            assert_eq!(resumed["result"]["stopReason"], serde_json::Value::Null);
            assert_eq!(
                resumed["result"]["priorityFailure"],
                serde_json::Value::Null
            );
            assert_eq!(resumed["result"]["failureSuspended"], false);
            let waiting = resumed["result"]["waiting"].as_array().unwrap();
            assert_eq!(waiting.len(), 3);
            for (entry, registration) in waiting.iter().zip(&registrations) {
                assert_eq!(entry["itemId"], registration.to_base64url());
                assert_eq!(entry["revision"], "1");
                assert_eq!(entry["suspended"], false);
            }
        })
    })
    .await;
}

#[tokio::test]
async fn queue_command_revoked_actor_cannot_resume_paused_items() {
    case(|f| {
        Box::pin(async move {
            let target_id = target(2).encode();
            let mut registrations = Vec::new();
            for marker in [180, 182, 184] {
                let body_id = body(f, UploadPurpose::Submit, b"[]").await;
                let registered = queue_read_call(
                    f,
                    0,
                    marker,
                    "input.register",
                    serde_json::json!({
                        "targetId": target_id,
                        "operation": {"kind": "submit", "bodyId": body_id}
                    }),
                )
                .await;
                assert_eq!(registered["ok"], true);
                let registration = Id128::new([marker; 16]);
                let consumed = queue_read_call(
                    f,
                    0,
                    marker + 1,
                    "input.consume",
                    serde_json::json!({
                        "targetId": target_id,
                        "registrationId": registration.to_base64url(),
                        "bodyId": body_id
                    }),
                )
                .await;
                assert_eq!(consumed["result"]["request"]["outcome"]["kind"], "queued");
                registrations.push(registration);
            }

            let queue = &f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .queue;
            assert_eq!(
                queue
                    .suspend_waiting_for_test(
                        f.clients[0].context.actor(),
                        target(2),
                        crate::domain::native_dispatch::KnownStopReason::Refusal,
                    )
                    .await
                    .unwrap(),
                3
            );

            let revoked_client = f.clients[2].context.actor().client_id().to_owned();
            let trust = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .trust_store();
            trust
                .revoke(
                    "local-owner",
                    &Id128::new([186; 16]).to_base64url(),
                    &revoked_client,
                )
                .await
                .unwrap();
            let attempted = f
                .node
                .as_ref()
                .unwrap()
                .queue_management()
                .unwrap()
                .register(&f.clients[2], target(2), Mutation::Resume)
                .await;
            assert!(
                matches!(
                    attempted,
                    Err(NodeQueueError::Registry(RegistryError::InvalidActor))
                ),
                "revoked actor resume result: {attempted:?}"
            );

            let stopped = queue_read_call(
                f,
                0,
                187,
                "input.queue",
                serde_json::json!({"targetId": target_id}),
            )
            .await;
            assert_eq!(stopped["result"]["stopReason"], "refusal");
            assert_eq!(
                stopped["result"]["priorityFailure"],
                serde_json::Value::Null
            );
            assert_eq!(stopped["result"]["failureSuspended"], true);
            let waiting = stopped["result"]["waiting"].as_array().unwrap();
            assert_eq!(waiting.len(), 3);
            for (entry, registration) in waiting.iter().zip(&registrations) {
                assert_eq!(entry["itemId"], registration.to_base64url());
                assert_eq!(entry["suspended"], true);
            }
        })
    })
    .await;
}

#[tokio::test]
async fn queue_read_wss_shared_three_and_original_registration_facts() {
    case(|f| {
        Box::pin(async move {
            let facade = f.node.as_ref().unwrap().queue_management().unwrap();
            let mut ids = Vec::new();
            for _ in 0..3 {
                let body_id = body(f, UploadPurpose::Submit, b"[]").await;
                let r = facade
                    .register(&f.clients[0], target(2), Mutation::Submit { body: body_id })
                    .await
                    .unwrap();
                let receipt = facade
                    .begin_consume(&f.clients[0], target(2), r.id(), Some(body_id))
                    .await
                    .unwrap();
                assert!(matches!(
                    receipt.wait().await.unwrap(),
                    Consumed::Resolved {
                        fact: RequestFact::Settled {
                            outcome: QueueOutcome::Queued { .. },
                            ..
                        },
                        ..
                    }
                ));
                ids.push(r.id());
            }
            let a = queue_read_call(
                f,
                0,
                101,
                "input.queue",
                serde_json::json!({"targetId":target(2).encode()}),
            )
            .await;
            let b = queue_read_call(
                f,
                2,
                102,
                "input.queue",
                serde_json::json!({"targetId":target(2).encode()}),
            )
            .await;
            assert_eq!(a["ok"], true);
            assert_eq!(a["result"], b["result"]);
            assert_eq!(a["result"].as_object().unwrap().len(), 8);
            assert_eq!(a["result"]["stopReason"], serde_json::Value::Null);
            assert_eq!(a["result"]["priorityFailure"], serde_json::Value::Null);
            let entries = a["result"]["waiting"].as_array().unwrap();
            assert_eq!(entries.len(), 3);
            for (entry, id) in entries.iter().zip(&ids) {
                assert_eq!(entry.as_object().unwrap().len(), 5);
                assert_eq!(entry["itemId"], id.encode());
                assert_eq!(entry["revision"], "1");
                assert!(entry.get("body").is_none());
                assert!(entry.get("digest").is_none());
            }
            let value = queue_read_call(
                f,
                2,
                103,
                "input.request",
                serde_json::json!({"targetId":target(2).encode(),"registrationId":ids[0].encode()}),
            )
            .await;
            assert_eq!(value["result"]["state"], "settled");
            assert_eq!(value["result"]["outcome"]["kind"], "queued");
            let r = facade
                .register(&f.clients[0], target(2), Mutation::Resume)
                .await
                .unwrap();
            let value = queue_read_call(
                f,
                2,
                104,
                "input.request",
                serde_json::json!({"targetId":target(2).encode(),"registrationId":r.id().encode()}),
            )
            .await;
            assert_eq!(value["result"]["state"], "registered");
            assert_eq!(queued(f, 0).await.len(), 3);
        })
    })
    .await;
}

#[tokio::test]
async fn queue_read_sql_wait_keeps_same_socket_device_list_responsive() {
    case(|f| Box::pin(async move {
        use crate::session_storage::request_registry::test_hooks::{Action,Operation,Point};
        let facade=f.node.as_ref().unwrap().queue_management().unwrap();
        let r=facade.register(&f.clients[0],target(2),Mutation::Resume).await.unwrap();
        let gate=crate::session_storage::request_registry::test_hooks::Barrier::new();
        let release=ReleaseSqlGate(gate.clone());
        f.registry.as_ref().unwrap().faults_for_test().arm(Point::AfterCommit(Operation::Read),Action::Pause(gate.clone())).unwrap();
        send_json(&mut f.peers[0],&serde_json::json!({"v":1,"type":"command","requestId":Id128::new([105;16]),"action":"input.request","payload":{"targetId":target(2).encode(),"registrationId":r.id().encode()}})).await;
        timeout(Duration::from_secs(5),gate.arrived()).await.unwrap();
        let devices=queue_read_call(f,0,106,"device.list",serde_json::json!({"cursor":null})).await;
        assert_eq!(devices["ok"],true,"same reader progressed before releasing original SQL");
        drop(release);
        let value=next_json(&mut f.peers[0],Duration::from_secs(5)).await;
        assert_eq!(value["requestId"],Id128::new([105;16]).to_base64url()); assert_eq!(value["result"]["state"],"registered");
    })).await;
}

#[tokio::test]
async fn queue_read_revoke_at_prepared_result_sends_no_result() {
    case(|f| Box::pin(async move {
        let management=f.node.as_ref().unwrap().management.as_ref().unwrap();
        let gate=management.pause_next_boundary(crate::management_ws::ManagementBoundary::Enqueue);
        let release=QueueReadRelease(gate.clone());
        send_json(&mut f.peers[0],&serde_json::json!({"v":1,"type":"command","requestId":Id128::new([107;16]),"action":"input.queue","payload":{"targetId":target(2).encode()}})).await;
        timeout(Duration::from_secs(5),gate.wait_until_entered()).await.unwrap();
        let trust=f.node.as_ref().unwrap().storage.as_ref().unwrap().trust_store();
        let revoked=trust.revoke(f.clients[2].context.actor().client_id(),&Id128::new([108;16]).to_base64url(),&f.client_id).await;
        drop(release); revoked.unwrap();
        let (code,_)=next_close(&mut f.peers[0],Duration::from_secs(5)).await;
        assert_eq!(code,4410);
    })).await;
}

#[tokio::test]
async fn queue_read_device_revoked_actor_keeps_accepted_items_and_other_actors_live() {
    case(|f| {
        Box::pin(async move {
            let target_id = target(2).encode();
            let body_id = body(f, UploadPurpose::Submit, b"[]").await;
            let accepted = queue_read_call(
                f,
                0,
                108,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": {"kind": "submit", "bodyId": body_id.to_base64url()}
                }),
            )
            .await;
            assert_eq!(accepted["ok"], true, "{accepted}");
            let registration = Id128::new([108; 16]);
            let consumed = queue_read_call(
                f,
                0,
                109,
                "input.consume",
                serde_json::json!({
                    "targetId": target_id,
                    "registrationId": registration.to_base64url(),
                    "bodyId": body_id.to_base64url()
                }),
            )
            .await;
            assert_eq!(consumed["ok"], true, "{consumed}");
            assert_eq!(consumed["result"]["request"]["outcome"]["kind"], "queued");

            let revoked = ipc::device_revoke(f.node.as_ref().unwrap().ipc_address(), &f.client_id)
                .await
                .expect("owner revocation transaction");
            assert_eq!(revoked.client_id, f.client_id);

            tokio::time::sleep(Duration::from_millis(200)).await;

            let queue = queue_read_call(
                f,
                2,
                111,
                "input.queue",
                serde_json::json!({"targetId": target_id}),
            )
            .await;
            assert_eq!(queue["ok"], true, "{queue}");
            assert_eq!(queue["result"]["stopReason"], serde_json::Value::Null);
            assert_eq!(queue["result"]["failureSuspended"], false);
            let waiting = queue["result"]["waiting"].as_array().unwrap();
            assert_eq!(waiting.len(), 1);
            assert_eq!(waiting[0]["itemId"], registration.to_base64url());
            assert_eq!(waiting[0]["revision"], "1");
            assert_eq!(waiting[0]["suspended"], false);
            assert_eq!(waiting[0].as_object().unwrap().len(), 5);
            assert!(waiting[0].get("body").is_none());
            assert!(waiting[0].get("digest").is_none());

            let other_body = body_for(f, 2, UploadPurpose::Submit, b"[]").await;
            let other_accepted = queue_read_call(
                f,
                2,
                112,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": {"kind": "submit", "bodyId": other_body.to_base64url()}
                }),
            )
            .await;
            assert_eq!(other_accepted["ok"], true, "{other_accepted}");
            let other_registration = Id128::new([112; 16]);
            let other_consumed = queue_read_call(
                f,
                2,
                113,
                "input.consume",
                serde_json::json!({
                    "targetId": target_id,
                    "registrationId": other_registration.to_base64url(),
                    "bodyId": other_body.to_base64url()
                }),
            )
            .await;
            assert_eq!(other_consumed["ok"], true, "{other_consumed}");
            assert_eq!(
                other_consumed["result"]["request"]["outcome"]["kind"],
                "queued"
            );
        })
    })
    .await;
}

#[tokio::test]
async fn queue_read_abort_before_first_poll_wakes_but_join_stays_original() {
    case(|f| {
        Box::pin(async move {
            use crate::application::queue_owner::{
                QueueReadFailure, ReadTaskSettlement, RuntimeSupervisor,
            };
            let service = &f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .queue;
            let receipt = service
                .begin_query_request(
                    f.clients[0].context.actor().clone(),
                    target(2),
                    crate::domain::request_registry::RegistrationId::new(target(99)),
                )
                .unwrap();
            assert_eq!(receipt.task_observation(), ReadTaskSettlement::Pending);
            RuntimeSupervisor::cancel(service);
            assert_eq!(
                receipt.task_observation(),
                ReadTaskSettlement::Pending,
                "abort request is not actual join"
            );
            assert_eq!(
                timeout(Duration::from_secs(5), receipt.wait())
                    .await
                    .unwrap(),
                Err(QueueReadFailure::WorkerStoppedUnconfirmed)
            );
            let _ = service.task_failures();
            assert_eq!(
                receipt.task_observation(),
                ReadTaskSettlement::Cancelled,
                "original reaper observed actual JoinError"
            );
        })
    })
    .await;
}

#[tokio::test]
async fn queue_read_factory_install_is_single_and_rejects_foreign_node() {
    case(|f| {
        Box::pin(async move {
            let services = f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .management_services();
            assert!(matches!(
                f.node
                    .as_ref()
                    .unwrap()
                    .management
                    .as_ref()
                    .unwrap()
                    .install_session_services(services.clone()),
                Err(crate::domain::shared_view::ViewError::AlreadyAttached)
            ));
            let other_area = TestArea::new();
            let other = NodeRuntime::start(
                other_area.config(free_address()),
                Arc::new(FakeProtector::default()),
            )
            .await
            .unwrap();
            let result = other
                .management
                .as_ref()
                .unwrap()
                .install_session_services(services);
            let closed = other.shutdown(None).await;
            assert!(closed.is_ok());
            assert!(matches!(
                result,
                Err(crate::domain::shared_view::ViewError::InvalidScope)
            ));
            assert_eq!(
                queue_read_call(
                    f,
                    0,
                    109,
                    "input.queue",
                    serde_json::json!({"targetId":target(2).encode()})
                )
                .await["ok"],
                true
            );
        })
    })
    .await;
}

fn catalog_utf16(value: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(
        value
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

fn native_view_profile(argument: &str) -> Vec<u8> {
    let child = crate::test_support::native_dispatch_dev_child(&["NATIVE_DISPATCH_DEV_CHILD"]);
    let child = child.to_string_lossy();
    assert!(std::path::Path::new(child.as_ref()).is_file());
    format!(
        r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}"],"environment":[]}},"recipes":[{{"wire":1,"mcp_servers":[],"additional_directories":[],"extra":{{}}}},{{"wire":2,"mcp_servers":[],"additional_directories":[],"extra":{{}}}}]}}"#,
        catalog_utf16(&child),
        catalog_utf16(argument)
    )
    .into_bytes()
}

async fn apply_catalog_payload(
    f: &Case,
    owner: &crate::node::catalog_upload::CatalogUploadOwner,
    raw: &[u8],
) -> crate::domain::native_catalog::CatalogReceipt {
    let client = &f.clients[0];
    let upload = owner.begin(client).await.unwrap().upload();
    for (index, part) in raw.chunks(16384).enumerate() {
        let wire = frame(upload, (index * 16384) as u64, part);
        owner
            .accept_data(client, InputData::decode_client(&wire).unwrap())
            .await
            .unwrap();
    }
    owner
        .finish(client, upload, raw.len() as u64)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status = owner.status(client, upload).await.unwrap();
            if matches!(status.phase(), UploadPhase::Ready | UploadPhase::Failed) {
                break status;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let management = f.node.as_ref().unwrap().catalog_management().unwrap();
    let registration = management
        .register(client, upload)
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    management
        .apply(client, registration)
        .await
        .unwrap()
        .wait()
        .await
        .unwrap()
}

async fn configure_native_view_target(f: &Case) {
    use crate::domain::native_catalog::CatalogResult;

    let node = f.node.as_ref().unwrap();
    let owner = node.catalog_upload_owner(&f.clients[0]).unwrap();
    let profile = native_view_profile("read-first-held");
    let install = format!(
        r#"{{"kind":"install_harness","name":"socket view fixture","source":{{"kind":"existing","sourceId":"{}"}},"profile":{}}}"#,
        target(1).encode(),
        std::str::from_utf8(&profile).unwrap()
    );
    let installed = apply_catalog_payload(f, &owner, install.as_bytes()).await;
    let (harness, source, profile) = match installed.fact().result().unwrap() {
        CatalogResult::Harness {
            harness,
            source,
            profile,
        } => (*harness, *source, *profile),
        other => panic!("expected installed harness, got {other:?}"),
    };
    assert_eq!(source, target(1));

    let canonical = f
        .area
        .directory
        .canonicalize()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let pool = f.pool.as_ref().unwrap();
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

    let bind = format!(
        r#"{{"kind":"bind_target_initial","targetId":"{}","sourceId":"{}","harnessId":"{}","profileId":"{}","profileRevision":"{}","workspaceId":"{}","launchPath":"{}"}}"#,
        target(2).encode(),
        source.encode(),
        harness.encode(),
        profile.id().encode(),
        profile.revision(),
        target(9).encode(),
        canonical.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let bound = apply_catalog_payload(f, &owner, bind.as_bytes()).await;
    assert!(matches!(
        bound.fact().result(),
        Some(CatalogResult::Target {
            target: bound_target,
            workspace,
            harness: bound_harness,
            source: bound_source,
            profile: bound_profile,
        }) if *bound_target == target(2)
            && *workspace == target(9)
            && *bound_harness == harness
            && *bound_source == source
            && *bound_profile == profile
    ));
    let sources = node
        .session_services
        .as_ref()
        .unwrap()
        .source_sessions
        .clone();
    let operation = sources
        .register(
            &f.clients[0],
            crate::domain::source_sessions::SourceLaunchSelection {
                source,
                harness,
                profile,
                workspace: target(9),
                launch_path: canonical,
            },
            crate::domain::source_sessions::SourceSessionAction::Create,
        )
        .await
        .unwrap();
    let created = sources
        .execute(&f.clients[0], operation.id)
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert_eq!(
        created.outcome,
        crate::domain::source_sessions::SourceSessionOutcome::Succeeded,
        "{created:?}"
    );
    let native = created.response.unwrap().native_session_id.unwrap();
    sqlx::query("UPDATE product_targets SET native_id=? WHERE target_id=?")
        .bind(native.as_ref())
        .bind(target(2).encode())
        .execute(pool)
        .await
        .unwrap();
}

struct NativeViewFixtureRelease(std::path::PathBuf);

impl Drop for NativeViewFixtureRelease {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, b"release");
    }
}

fn release_native_view_fixture(f: &Case) -> NativeViewFixtureRelease {
    NativeViewFixtureRelease(f.area.directory.join("release-read-first-held"))
}

#[tokio::test]
async fn queue_read_upload_and_view_success_are_distinct_from_invalid_payloads() {
    case(|f| {
        Box::pin(async move {
            configure_native_view_target(f).await;
            let _release = release_native_view_fixture(f);
            let result = queue_read_call(
                f,
                0,
                110,
                "input.upload.begin",
                serde_json::json!({"targetId":target(2).encode(),"purpose":"submit"}),
            )
            .await;
            assert_eq!(result["ok"], true, "{result}");
            assert!(result["result"]["uploadId"].as_str().is_some());
            let result = queue_read_call(
                f,
                0,
                111,
                "view.open",
                serde_json::json!({"targetId":target(2).encode(),"reason":"first_open"}),
            )
            .await;
            assert_eq!(result["ok"], true, "{result}");
            let result = &result["result"];
            assert_eq!(result["type"], "view.opened", "{result}");
            assert!(result["streamId"].as_str().is_some());
            assert!(result["viewEpoch"].as_str().is_some());
            assert!(result["snapshotRevision"].as_str().is_some());
            assert_eq!(result["capabilities"]["opaqueChunks"], 1);
            assert_eq!(result["status"].as_object().expect("view status").len(), 6);
            let result =
                queue_read_call(f, 0, 112, "input.queue", serde_json::json!({"targetId":27})).await;
            assert_eq!(result["error"]["code"], "invalid_request");
            let result = queue_read_call(
                f,
                0,
                113,
                "input.queue",
                serde_json::json!({"targetId":target(99).encode()}),
            )
            .await;
            assert_eq!(result["error"]["code"], "invalid_target");
        })
    })
    .await;
}

#[tokio::test]
async fn queue_read_view_close_seals_only_its_stream_and_retries_idempotently() {
    case(|f| {
        Box::pin(async move {
            configure_native_view_target(f).await;
            let _release = release_native_view_fixture(f);
            let opened = queue_read_call(
                f,
                0,
                120,
                "view.open",
                serde_json::json!({"targetId":target(2).encode(),"reason":"first_open"}),
            )
            .await;
            assert_eq!(opened["ok"], true, "{opened}");
            assert_eq!(opened["result"]["type"], "view.opened", "{opened}");
            let stream_id = opened["result"]["streamId"].as_str().unwrap().to_owned();

            let first = queue_read_call(
                f,
                0,
                121,
                "view.close",
                serde_json::json!({"streamId":stream_id}),
            )
            .await;
            assert_eq!(first["ok"], true, "{first}");
            let result = first["result"].as_object().expect("closed receipt");
            assert_eq!(result.len(), 5);
            assert_eq!(result["streamId"], stream_id);
            assert_eq!(result["alreadyClosed"], false);
            assert_eq!(result["lastCommittedPacket"], "0");
            assert_eq!(result["startedPacket"], serde_json::Value::Null);
            assert_eq!(result["state"], "closed");

            let retry = queue_read_call(
                f,
                0,
                122,
                "view.close",
                serde_json::json!({"streamId":stream_id}),
            )
            .await;
            assert_eq!(retry["ok"], true, "{retry}");
            assert_eq!(retry["result"], first["result"]);

            let foreign = queue_read_call(
                f,
                1,
                123,
                "view.close",
                serde_json::json!({"streamId":stream_id}),
            )
            .await;
            assert_eq!(foreign["ok"], false, "{foreign}");
            assert_eq!(foreign["error"]["code"], "invalid_client");

            let reopened = queue_read_call(
                f,
                0,
                124,
                "view.open",
                serde_json::json!({"targetId":target(2).encode(),"reason":"reopen"}),
            )
            .await;
            assert_eq!(reopened["ok"], true, "{reopened}");
            assert_eq!(reopened["result"]["type"], "view.opened", "{reopened}");
            assert_ne!(reopened["result"]["streamId"], stream_id);
        })
    })
    .await;
}

#[tokio::test]
async fn queue_read_control_only_does_not_report_an_empty_product() {
    let area = TestArea::new();
    let node = NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    let key = SigningKey::from_slice(&[78; 32]).unwrap();
    let client = pair_browser(&node, &key, "readonly control-only fixture").await;
    let mut peer = authenticate_socket(
        valid_management_socket(&area, node.listen_addr()).await,
        &key,
        &client,
        node_public_key(&node),
    )
    .await;
    let management = node.management.as_ref().unwrap().clone();
    assert_eq!(management.connection_counts_for_test(), (1, 1, 1, 1));
    let upload = Id128::new([114; 16]);
    let result = AssertUnwindSafe(async {
        for (marker, action, payload) in [
            (
                113,
                "input.queue",
                serde_json::json!({"targetId":target(2).encode()}),
            ),
            (
                114,
                "input.upload.begin",
                serde_json::json!({"targetId":target(2).encode(),"purpose":"submit"}),
            ),
            (
                115,
                "input.upload.finish",
                serde_json::json!({"uploadId":upload.to_base64url(),"totalBytes":"0"}),
            ),
            (
                116,
                "input.upload.status",
                serde_json::json!({"uploadId":upload.to_base64url()}),
            ),
            (
                117,
                "input.upload.release",
                serde_json::json!({"uploadId":upload.to_base64url()}),
            ),
        ] {
            send_json(
                &mut peer,
                &serde_json::json!({
                    "v":1,
                    "type":"command",
                    "requestId":Id128::new([marker;16]),
                    "action":action,
                    "payload":payload
                }),
            )
            .await;
            let response = next_json(&mut peer, Duration::from_secs(5)).await;
            assert_eq!(
                response["requestId"],
                Id128::new([marker; 16]).to_base64url()
            );
            assert_eq!(response["ok"], false, "{action}: {response}");
            assert_eq!(
                response["error"]["code"], "product_not_ready",
                "{action}: {response}"
            );
            assert!(response["result"].is_null(), "{action}: {response}");
            assert!(
                response.get("uploadId").is_none()
                    && response.get("ack").is_none()
                    && !response.to_string().contains("uploadId"),
                "{action}: {response}"
            );
        }
    })
    .catch_unwind()
    .await;
    let _ = peer.close(None).await;
    drop(peer);
    timeout(Duration::from_secs(5), async {
        while management.connection_counts_for_test() != (0, 0, 0, 0) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("control-only first connection owner joined");
    let mut peer = authenticate_socket(
        valid_management_socket(&area, node.listen_addr()).await,
        &key,
        &client,
        node_public_key(&node),
    )
    .await;
    send_json(
        &mut peer,
        &serde_json::json!({
            "v":1,
            "type":"command",
            "requestId":Id128::new([118;16]),
            "action":"input.upload.status",
            "payload":{"uploadId":upload.to_base64url()}
        }),
    )
    .await;
    let reconnected = next_json(&mut peer, Duration::from_secs(5)).await;
    assert_eq!(reconnected["error"]["code"], "product_not_ready");
    assert!(reconnected["result"].is_null());
    let _ = peer.close(None).await;
    drop(peer);
    timeout(Duration::from_secs(5), async {
        while management.connection_counts_for_test() != (0, 0, 0, 0) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("control-only reconnect owner joined");
    let closed = node.shutdown(None).await;
    assert!(closed.is_ok(), "control-only shutdown: {closed:?}");
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

#[tokio::test]
async fn native_release_registration_has_a_durable_actor_bound_fact() {
    case(|f| {
        Box::pin(async move {
            let registered = queue_read_call(
                f,
                0,
                212,
                "native.release.register",
                serde_json::json!({"targetId":target(2).encode()}),
            )
            .await;
            assert_eq!(registered["ok"], true, "{registered}");

            let operation = registered["result"]["releaseOperationId"]
                .as_str()
                .expect("release operation id");
            let row: (String, i64, String, String, String, Option<String>) = sqlx::query_as(
                "SELECT actor_id,actor_auth_generation,request_id,target_id,phase,error_code \
                 FROM node_native_release_operations WHERE operation_id=?",
            )
            .bind(operation)
            .fetch_one(f.pool.as_ref().expect("original SQLite pool"))
            .await
            .expect("native release registration must be durable before execution");
            assert_eq!(row.0, f.client_id);
            assert!(row.1 >= 0);
            assert_eq!(row.2, Id128::new([212; 16]).to_base64url());
            assert_eq!(row.3, target(2).encode());
            assert_eq!(row.4, "registered");
            assert_eq!(row.5, None);
        })
    })
    .await;
}

#[tokio::test]
async fn session_management_routes_approval_and_native_release_services() {
    case(|f| {
        Box::pin(async move {
            let approvals =
                queue_read_call(f, 0, 201, "approval.list", serde_json::json!({})).await;
            assert_eq!(
                approvals,
                serde_json::json!({
                    "v": 1,
                    "type": "result",
                    "requestId": Id128::new([201; 16]).to_base64url(),
                    "ok": true,
                    "result": {"items": []},
                    "error": null
                }),
                "approval list must reach the authenticated native approval service"
            );

            let release = queue_read_call(
                f,
                0,
                202,
                "native.release.register",
                serde_json::json!({"targetId": target(99).encode()}),
            )
            .await;
            assert_eq!(release["ok"], false, "{release}");
            assert_eq!(
                release["error"]["code"], "invalid_target",
                "native release registration must validate the real target"
            );
            assert!(release["result"].is_null(), "{release}");

            configure_native_view_target(f).await;
            let release_fixture = release_native_view_fixture(f);
            let opened = queue_read_call(
                f,
                0,
                203,
                "view.open",
                serde_json::json!({
                    "targetId":target(2).encode(),
                    "reason":"first_open"
                }),
            )
            .await;
            assert_eq!(opened["ok"], true, "{opened}");
            assert_eq!(opened["result"]["type"], "view.opened", "{opened}");

            let registered = queue_read_call(
                f,
                0,
                204,
                "native.release.register",
                serde_json::json!({"targetId":target(2).encode()}),
            )
            .await;
            assert_eq!(registered["ok"], true, "{registered}");
            assert_eq!(registered["result"]["targetId"], target(2).encode());
            assert_eq!(registered["result"]["state"], "registered");
            let operation = Id128::parse_base64url(
                registered["result"]["releaseOperationId"]
                    .as_str()
                    .expect("release operation id"),
            )
            .expect("canonical release operation id");
            let retried_registration = queue_read_call(
                f,
                0,
                204,
                "native.release.register",
                serde_json::json!({"targetId":target(2).encode()}),
            )
            .await;
            assert_eq!(retried_registration, registered);
            let conflicting_registration = queue_read_call(
                f,
                0,
                204,
                "native.release.register",
                serde_json::json!({"targetId":target(3).encode()}),
            )
            .await;
            assert_eq!(conflicting_registration["ok"], false);
            assert_eq!(
                conflicting_registration["error"]["code"], "request_conflict",
                "one requestId cannot register a different native release target"
            );

            let foreign_status = queue_read_call(
                f,
                2,
                210,
                "native.release.status",
                serde_json::json!({
                    "releaseOperationId":operation.to_base64url()
                }),
            )
            .await;
            assert_eq!(foreign_status["ok"], false, "{foreign_status}");
            assert_eq!(
                foreign_status["error"]["code"], "invalid_release",
                "release status must be bound to its registering actor"
            );
            assert!(foreign_status["result"].is_null(), "{foreign_status}");

            let foreign_release = queue_read_call(
                f,
                2,
                211,
                "native.release",
                serde_json::json!({
                    "releaseOperationId":operation.to_base64url()
                }),
            )
            .await;
            assert_eq!(foreign_release["ok"], false, "{foreign_release}");
            assert_eq!(
                foreign_release["error"]["code"], "invalid_release",
                "release execution must be bound to its registering actor"
            );
            assert!(foreign_release["result"].is_null(), "{foreign_release}");

            let registered_status = queue_read_call(
                f,
                0,
                205,
                "native.release.status",
                serde_json::json!({
                    "releaseOperationId":operation.to_base64url()
                }),
            )
            .await;
            assert_eq!(registered_status["ok"], true, "{registered_status}");
            assert_eq!(
                registered_status["result"],
                serde_json::json!({
                    "releaseOperationId":operation.to_base64url(),
                    "targetId":target(2).encode(),
                    "state":"registered",
                    "error":null
                })
            );

            std::fs::write(f.area.directory.join("release-read-first-held"), b"release")
                .expect("release the held native fixture");
            let pool = f.pool.as_ref().expect("original SQLite pool").clone();
            timeout(Duration::from_secs(10), async {
                loop {
                    let loaded: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM node_native_resource_bindings \
                         WHERE target_id=? AND lifecycle='loaded'",
                    )
                    .bind(target(2).encode())
                    .fetch_one(&pool)
                    .await
                    .expect("query native resource load state");
                    if loaded == 1 {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("native resource reached loaded state");
            sqlx::query(
                "CREATE TRIGGER release_result_fault \
                 BEFORE UPDATE OF phase,error_code ON node_native_release_operations \
                 WHEN OLD.phase='releasing' \
                 BEGIN SELECT RAISE(ABORT,'native result fixture'); END",
            )
            .execute(&pool)
            .await
            .expect("release result storage fault");
            let unrecorded = queue_read_call(
                f,
                0,
                206,
                "native.release",
                serde_json::json!({
                    "releaseOperationId":operation.to_base64url()
                }),
            )
            .await;
            assert_eq!(unrecorded["ok"], false, "{unrecorded}");
            assert_eq!(
                unrecorded["error"]["code"], "necessary_storage_unavailable",
                "native success must not be reported while its terminal fact is unconfirmed"
            );
            assert!(unrecorded["result"].is_null(), "{unrecorded}");
            let releasing = queue_read_call(
                f,
                0,
                207,
                "native.release.status",
                serde_json::json!({
                    "releaseOperationId":operation.to_base64url()
                }),
            )
            .await;
            assert_eq!(releasing["ok"], true, "{releasing}");
            assert_eq!(releasing["result"]["state"], "releasing", "{releasing}");

            sqlx::query("DROP TRIGGER release_result_fault")
                .execute(&pool)
                .await
                .expect("remove release result storage fault");
            let released = queue_read_call(
                f,
                0,
                208,
                "native.release",
                serde_json::json!({
                    "releaseOperationId":operation.to_base64url()
                }),
            )
            .await;
            assert_eq!(released["ok"], true, "{released}");
            assert_eq!(
                released["result"],
                serde_json::json!({
                    "releaseOperationId":operation.to_base64url(),
                    "targetId":target(2).encode(),
                    "state":"released",
                    "error":null
                })
            );

            let status_after_release = queue_read_call(
                f,
                0,
                209,
                "native.release.status",
                serde_json::json!({
                    "releaseOperationId":operation.to_base64url()
                }),
            )
            .await;
            assert_eq!(
                status_after_release["result"], released["result"],
                "{status_after_release}"
            );
            let repeated = queue_read_call(
                f,
                0,
                210,
                "native.release",
                serde_json::json!({
                    "releaseOperationId":operation.to_base64url()
                }),
            )
            .await;
            assert_eq!(repeated["ok"], true, "{repeated}");
            assert_eq!(repeated["result"], released["result"]);

            let unknown = queue_read_call(
                f,
                0,
                211,
                "native.release.status",
                serde_json::json!({
                    "releaseOperationId":Id128::new([210;16]).to_base64url()
                }),
            )
            .await;
            assert_eq!(unknown["ok"], false, "{unknown}");
            assert_eq!(unknown["error"]["code"], "invalid_release");
            drop(release_fixture);
        })
    })
    .await;
}

#[tokio::test]
async fn pending_reads_run_concurrently_and_shutdown_preempts_inline_enqueue() {
    use crate::management_ws::{ManagementBoundary, UploadCommandAction, UploadCommandBoundary};

    case(|f| {
        Box::pin(async move {
            let upload = acpi_begin(f, 0, 221).await;
            let management = f.node.as_ref().unwrap().management.as_ref().unwrap().clone();
            let barrier = management.pause_next_upload_command(
                UploadCommandAction::Status,
                UploadCommandBoundary::BeforeCall,
            );
            let _release = BarrierRelease(barrier.clone());

            send_json(
                &mut f.peers[0],
                &serde_json::json!({
                    "v": 1,
                    "type": "command",
                    "requestId": Id128::new([222; 16]),
                    "action": "input.upload.status",
                    "payload": {"uploadId": upload.to_base64url()}
                }),
            )
            .await;
            timeout(Duration::from_secs(5), barrier.wait_until_entered())
                .await
                .expect("first PendingReads command paused inside JoinSet task");

            send_json(
                &mut f.peers[0],
                &serde_json::json!({
                    "v": 1,
                    "type": "command",
                    "requestId": Id128::new([223; 16]),
                    "action": "node.info",
                    "payload": {}
                }),
            )
            .await;
            send_json(
                &mut f.peers[0],
                &serde_json::json!({
                    "v": 1,
                    "type": "command",
                    "requestId": Id128::new([224; 16]),
                    "action": "input.upload.begin",
                    "payload": {"targetId": target(2).encode(), "purpose": "submit"}
                }),
            )
            .await;

            let r_info = next_json(&mut f.peers[0], Duration::from_secs(5)).await;
            let r_begin = next_json(&mut f.peers[0], Duration::from_secs(5)).await;
            let ids = [
                r_info["requestId"].as_str().unwrap().to_owned(),
                r_begin["requestId"].as_str().unwrap().to_owned(),
            ];
            assert!(
                ids.contains(&Id128::new([223; 16]).to_base64url())
                    && ids.contains(&Id128::new([224; 16]).to_base64url()),
                "concurrent commands completed while first PendingReads remained paused: {r_info} {r_begin}"
            );
            assert_eq!(r_info["ok"], true);
            assert_eq!(r_begin["ok"], true);

            barrier.release();
            let r_status = next_json(&mut f.peers[0], Duration::from_secs(5)).await;
            assert_eq!(r_status["requestId"], Id128::new([222; 16]).to_base64url());
            assert_eq!(r_status["ok"], true);
        })
    })
    .await;

    case(|f| {
        Box::pin(async move {
            let barrier = f
                .node
                .as_ref()
                .unwrap()
                .management
                .as_ref()
                .unwrap()
                .pause_next_boundary(ManagementBoundary::Enqueue);
            send_json(
                &mut f.peers[0],
                &serde_json::json!({
                    "v": 1,
                    "type": "command",
                    "requestId": Id128::new([225; 16]),
                    "action": "input.upload.begin",
                    "payload": {"targetId": target(2).encode(), "purpose": "submit"}
                }),
            )
            .await;
            timeout(Duration::from_secs(5), barrier.wait_until_entered())
                .await
                .expect("PendingReads completion reached enqueue_authenticated");
            let shutdown = r06_stop_connection(f, barrier).await;
            timeout(Duration::from_secs(10), shutdown)
                .await
                .expect("Node stop deadline")
                .expect("Node shutdown task")
                .expect("Node stops cleanly");
            r06_wait_owner_clean(f).await;
        })
    })
    .await;
}
