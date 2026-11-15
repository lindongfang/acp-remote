use super::*;
use crate::{
    application::shared_views::{
        budget::ReadReason,
        read_control::{NativeReadControl, NativeReadMetadata, ReadTransportState},
        wire::RecordKind,
    },
    domain::shared_view::Freshness,
    shared_view_test_support::{FixtureError, FixtureIoController, FixtureIoFault, FixtureIoPlan},
};
use std::sync::atomic::AtomicBool;

async fn start_controlled(
    version: Version,
    provider: Option<Arc<dyn NativeReadOptionsProvider>>,
) -> (Harness, FixtureIoController) {
    let area = OwnedTestArea::create(&std::env::temp_dir(), "shared-view-reachability")
        .expect("unique child");
    let cert =
        rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("synthetic TLS");
    area.install_tls(
        cert.cert.pem().as_bytes(),
        cert.signing_key.serialize_pem().as_bytes(),
    )
    .expect("create_new");
    let owner =
        FixtureTaskOwner::new(&area, tokio::runtime::Handle::current()).expect("owner before IO");
    let auth = Arc::new(
        LocalViewAuthFixture::start(area)
            .await
            .expect("actual Node"),
    );
    let client = auth
        .authenticate(&FixtureSigningClient::generate().expect("own key"))
        .await
        .expect("actual auth");
    let (wire, mut peer, control) =
        FixturePeerWire::from_controlled_duplex(&owner.handle(), tokio::io::duplex(128 * 1024))
            .unwrap_or_else(|_| panic!("original tuple"));
    let driver = tokio::spawn(async move {
        let result: &[u8] = match version {
            Version::V1 => br#"{"protocolVersion":1,"agentInfo":{"name":"fixture","version":"1"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}}}"#,
            Version::V2 => br#"{"protocolVersion":2,"info":{"name":"fixture","version":"2"},"capabilities":{"session":{}}}"#,
        };
        super::super::tests::reply(&mut peer, result).await;
        peer
    });
    let config = FixtureSourceConfig {
        source: NativeSourceId(EntityId::from_bytes([94; 16])),
        entry_alias: Arc::from("controlled"),
    };
    let module = match provider {
        Some(provider) => {
            AcpViewModuleFixture::start_with_read_options_provider(
                config,
                wire,
                Arc::clone(&auth),
                provider,
            )
            .await
        }
        None => AcpViewModuleFixture::start(config, wire, Arc::clone(&auth)).await,
    }
    .expect("same real initialization");
    let mut peer = driver.await.expect("init peer joined");
    let driver = tokio::spawn(async move {
        super::super::tests::reply(&mut peer, br#"{"sessionId":"control-session"}"#).await;
        peer
    });
    let target = module
        .create(setup(version, "C:\\fixture"))
        .await
        .expect("true NewSession SID");
    let harness = Harness {
        auth,
        owner,
        module,
        target,
        peer: driver.await.expect("create peer joined"),
        client,
    };
    wait_flushes(&control, 2).await;
    (harness, control)
}
async fn wait_flushes(control: &FixtureIoController, count: u64) {
    timeout_at(deadline(), async {
        loop {
            let snapshot = control.snapshot();
            if snapshot.delegated_flushes() >= count {
                return;
            }
            control
                .changed_after(&snapshot)
                .await
                .expect("actual underlying flush change");
        }
    })
    .await
    .expect("previous real frames have actually flushed before arm");
}
async fn wait_metadata(
    request: &NativeReadRequest,
    predicate: impl Fn(&NativeReadMetadata) -> bool + Send + Sync,
) -> NativeReadMetadata {
    let mut changed = request.subscribe();
    timeout_at(deadline(), async {
        loop {
            let state = changed.borrow().clone();
            if predicate(&state) {
                return state;
            }
            changed.changed().await.expect("original request watch");
        }
    })
    .await
    .unwrap_or_else(|_| {
        let state = changed.borrow();
        panic!(
            "actual original state did not converge: mode={:?} transport={:?} failure={:?}",
            state.read_mode(),
            state.transport(),
            state.failure()
        );
    })
}
async fn explicit_next(h: &Harness, version: Version) -> NativeReadRequest {
    timeout_at(deadline(), async {
        loop {
            match h
                .module
                .replay(
                    &h.target,
                    setup(version, "C:\\fixture"),
                    NativeReadOptions {
                        write_deadline: deadline(),
                        response_wait: ReadResponseWait::Unbounded,
                    },
                )
                .await
            {
                Ok(request) => return request,
                Err(ViewError::InvalidStage) => tokio::task::yield_now().await,
                Err(error) => panic!("actual reservation failed: {error}"),
            }
        }
    })
    .await
    .expect("original terminal operation retires before one successor")
}
const NEW_RAW: &[u8] = br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"controlled-new"},"unknown":123456789012345678901234567890}}}"#;
async fn reply_read(h: &mut Harness, failed: bool) {
    let frame = h
        .peer
        .next_node_frame(deadline())
        .await
        .expect("real IO")
        .expect("real LF frame");
    let document = super::super::super::raw_json::RawDocument::parse(frame).expect("real request");
    let rpc = document
        .root()
        .object()
        .expect("object")
        .get("id")
        .expect("unique decoded field")
        .expect("typed RPC");
    h.send(NEW_RAW).await;
    if !failed {
        h.send(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"state_update","state":"idle"}}}"#).await;
    }
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    response.extend_from_slice(rpc.fragment().as_bytes());
    response.extend_from_slice(if failed {
        br#", "error":{"code":-32000,"message":"owned controlled failure"}}"#
    } else {
        b",\"result\":{}}"
    });
    h.send(&response).await;
}
async fn close_fault(h: Harness, control: &FixtureIoController) {
    let weak = Arc::downgrade(&h.module.core);
    let report = h.module.shutdown(deadline()).await;
    assert!(report.io_joined() && report.pump_joined() && report.views_joined());
    let owned = h.owner.shutdown(deadline()).await;
    assert_eq!(owned.pending_tasks(), 0);
    assert_eq!(owned.pending_resources(), 0);
    assert!(
        matches!(owned.failure(), Some(FixtureError::Io)),
        "original pump IO error stays visible"
    );
    assert!(control.snapshot().is_closed());
    drop(h.module);
    drop(h.peer);
    drop(h.client);
    drop(h.target);
    assert!(
        weak.upgrade().is_none(),
        "retaining IO controller does not retain Module/pump/read owner"
    );
    h.auth
        .close()
        .await
        .expect("actual Node/Storage first cleanup");
}
#[allow(
    clippy::too_many_lines,
    reason = "One original IO gate, native response, authenticated bundle and later IO error remain in one causal scenario"
)]
async fn response_before_flush_error(version: Version, failed: bool) {
    let (mut h, control) = start_controlled(version, None).await;
    let initial = explicit_next(&h, version).await;
    h.complete_replay().await;
    wait_metadata(&initial, |s| {
        s.read_mode() == ReadMode::ReplayComplete && s.transport() == ReadTransportState::Written
    })
    .await;
    h.wait_complete().await;
    wait_flushes(&control, 3).await;
    let old = h
        .module
        .reads
        .open_current(&h.client, h.target.key())
        .await
        .expect("authenticated original complete H");
    let old_descriptor = old.descriptor();
    let mut worker = h
        .module
        .test_controller()
        .arm_pause(
            ViewHookPoint::BeforeWorkerDetach,
            HookSelector::Source(h.module.scope()),
        )
        .expect("worker only");
    let mut flush = control
        .arm(FixtureIoPlan::NextFlush)
        .expect("next actual target flush");
    let request = explicit_next(&h, version).await;
    timeout_at(deadline(), worker.arrived())
        .await
        .expect("real worker paused")
        .expect("actual route cut");
    reply_read(&mut h, failed).await;
    let observed = timeout_at(deadline(), flush.arrived())
        .await
        .expect("actual flush Pending")
        .expect("actual IO call");
    assert_eq!(observed.delegated_flushes(), 3);
    timeout_at(deadline(), async {
        loop {
            let wake = h.module.core.changed.notified();
            tokio::pin!(wake);
            wake.as_mut().enable();
            let received = {
                let operations = h.module.core.operations.lock().expect("original registry");
                operations.iter().any(|op| {
                    op.replay
                        .as_ref()
                        .is_some_and(|replay| Some(replay.original_epoch()) == request.read_epoch())
                        && op.completed.borrow().as_ref().is_some_and(|r| {
                            if failed {
                                r.is_err()
                            } else {
                                matches!(r, Ok(OperationResult::Replayed))
                            }
                        })
                })
            };
            if received {
                break;
            }
            wake.await;
        }
    })
    .await
    .expect("real response processed by original pump independently of paused worker/flush");
    assert_ne!(
        h.module.native_reads.status(&request).read_mode(),
        ReadMode::ReplayComplete,
        "unprocessed prefix cannot become complete"
    );
    worker.release();
    let expected = if failed {
        ReadMode::ReplayIncomplete
    } else {
        ReadMode::ReplayComplete
    };
    wait_metadata(&request, |s| s.read_mode() == expected).await;
    if failed {
        timeout_at(deadline(), async {
            loop {
                let views = h.module.views();
                let signal = views.core.changed.clone();
                let wake = signal.notified();
                tokio::pin!(wake);
                wake.as_mut().enable();
                let current = views.core.target(h.target.key()).expect("target");
                let state = current.snapshot.read().expect("snapshot").clone();
                if state.segments.publications.len() >= 2 {
                    break;
                }
                wake.await;
            }
        })
        .await
        .expect("actual failed candidate segment published");
        assert_eq!(old.descriptor().view_epoch(), old_descriptor.view_epoch());
    } else {
        h.wait_complete().await;
    }
    assert_eq!(
        h.module.native_reads.status(&request).transport(),
        ReadTransportState::Writing
    );
    let mut started = h
        .module
        .test_controller()
        .observe_once(ViewHookPoint::AfterStartSendBeforeFlush, HookSelector::Any)
        .expect("real display start");
    let mut transfer = h
        .module
        .open_transfer(&h.client, &h.target, ReadReason::FirstOpen)
        .await
        .expect("same pending native operation; real auth/display");
    let stream = timeout_at(deadline(), started.arrived())
        .await
        .expect("real display IO")
        .expect("actual Started")
        .stream_id()
        .expect("actual stream");
    loop {
        let record = timeout_at(deadline(), transfer.next_record())
            .await
            .expect("real Module peer body")
            .expect("actual bytes");
        let found = record.address().kind() == RecordKind::OriginalRaw && record.bytes() == NEW_RAW;
        let packet = h
            .module
            .probe()
            .stream(stream)
            .expect("actual live stream")
            .highest_started();
        transfer
            .credit(packet)
            .await
            .expect("actual matching peer credit");
        if found {
            break;
        }
        assert_ne!(
            record.address().kind(),
            RecordKind::SnapshotEnd,
            "original new raw must be delivered"
        );
    }
    transfer.detach().await.expect("real consumer close");
    flush
        .fail(FixtureIoFault::ConnectionReset)
        .expect("only actual next flush returns Err");
    let terminal = wait_metadata(&request, |s| s.transport() == ReadTransportState::Unknown).await;
    assert_eq!(
        terminal.read_mode(),
        expected,
        "later flush error never fabricates or erases completion"
    );
    assert_eq!(old.descriptor().freshness(), Freshness::Stale);
    drop((old, request, initial, transfer));
    close_fault(h, &control).await;
}
#[tokio::test]
async fn shared_view_controlled_module_v1_complete_before_flush_error_and_real_raw_bundle() {
    response_before_flush_error(Version::V1, false).await;
}
#[tokio::test]
async fn shared_view_controlled_module_v2_complete_before_flush_error_and_real_raw_bundle() {
    response_before_flush_error(Version::V2, false).await;
}
#[tokio::test]
async fn shared_view_controlled_module_remote_error_before_flush_keeps_readable_failed_segments() {
    for version in [Version::V1, Version::V2] {
        response_before_flush_error(version, true).await;
    }
}

struct CountingPolicy {
    calls: AtomicU64,
    fail: AtomicBool,
}
#[tokio::test]
async fn shared_view_controlled_module_partial_original_write_produces_actual_unknown_not_completion()
 {
    let (h, control) = start_controlled(Version::V2, None).await;
    let before = h.module.views().core.probe.retained_frontier();
    let mut gate = control
        .arm(FixtureIoPlan::WritePrefix {
            forwarded: std::num::NonZeroUsize::new(1).expect("one actual byte"),
        })
        .expect("next real write");
    let request = explicit_next(&h, Version::V2).await;
    let observation = timeout_at(deadline(), gate.arrived())
        .await
        .expect("actual next write poll")
        .expect("partial original bytes");
    assert_eq!(observation.forwarded_since_arm(), 1);
    gate.fail(FixtureIoFault::BrokenPipe)
        .expect("actual AsyncWrite error");
    let state = wait_metadata(&request, |s| s.transport() == ReadTransportState::Unknown).await;
    assert_eq!(state.read_mode(), ReadMode::ReplayIncomplete);
    {
        let operations = h.module.core.operations.lock().expect("original operation");
        let operation = operations
            .iter()
            .find(|op| matches!(op.kind, RequestKind::Replay(_)))
            .expect("actual reserved RPC");
        assert!(matches!(
            operation
                .ticket
                .as_ref()
                .expect("original Ticket")
                .progress(),
            WriteProgress::Finished(WriteReport::Unknown { accepted: 1, .. })
        ));
    }
    assert_eq!(
        h.module.views().core.probe.retained_frontier(),
        before,
        "no fabricated Response or receipt sequence"
    );
    drop(request);
    close_fault(h, &control).await;
}
impl NativeReadOptionsProvider for CountingPolicy {
    fn for_new_operation(
        &self,
        context: &NativeReadStartContext<'_>,
    ) -> Result<NativeReadOptions, ViewError> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if self.fail.load(Ordering::Acquire) {
            return Err(ViewError::AllocationFailure);
        }
        Ok(NativeReadOptions {
            write_deadline: context.now() + Duration::from_secs(5),
            response_wait: ReadResponseWait::Unbounded,
        })
    }
}
async fn next_policy_request(h: &Harness, previous: EntityId) -> NativeReadRequest {
    timeout_at(deadline(), async {
        loop {
            let request = h
                .module
                .native_reads
                .request(h.target.key(), ReadReason::FirstOpen)
                .expect("real external read");
            if request.pinned_operation() != previous {
                return request;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("one actual successor after previous job retires")
}
#[tokio::test]
async fn shared_view_controlled_module_provider_join_failure_and_fresh_successor_use_original_watch()
 {
    let policy = Arc::new(CountingPolicy {
        calls: AtomicU64::new(0),
        fail: AtomicBool::new(false),
    });
    let weak = Arc::downgrade(&policy);
    let (mut h, control) = start_controlled(Version::V2, Some(policy.clone())).await;
    assert_eq!(
        policy.calls.load(Ordering::Acquire),
        0,
        "initialize/create are not provider calls"
    );
    let first = h
        .module
        .native_reads
        .request(h.target.key(), ReadReason::FirstOpen)
        .expect("original op");
    let read = h
        .module
        .reads
        .open(&h.client, h.target.key(), ReadReason::Reopen)
        .await
        .expect("join original");
    h.complete_replay().await;
    let initial = wait_metadata(&first, |s| {
        s.read_mode() == ReadMode::ReplayComplete && s.transport() == ReadTransportState::Written
    })
    .await;
    let selected = initial.selected_options().expect("real provider output");
    assert_eq!(policy.calls.load(Ordering::Acquire), 1);
    assert_eq!(
        h.module.policy.calls.load(Ordering::Acquire),
        0,
        "not the unused default policy"
    );
    let before = h.module.core.enqueues.load(Ordering::Acquire);
    policy.fail.store(true, Ordering::Release);
    let failed = next_policy_request(&h, first.pinned_operation()).await;
    let failure = wait_metadata(&failed, |s| s.read_mode() == ReadMode::ReplayIncomplete).await;
    assert_eq!(failure.failure(), Some(ViewError::AllocationFailure));
    assert!(failure.selected_options().is_none() && failure.read_epoch().is_none());
    assert_eq!(
        failure.transport(),
        ReadTransportState::Reserved,
        "provider failure is not a fabricated Zero"
    );
    assert_eq!(policy.calls.load(Ordering::Acquire), 2);
    assert_eq!(
        h.module.core.enqueues.load(Ordering::Acquire),
        before,
        "no Replay RPC reserved/enqueued"
    );
    policy.fail.store(false, Ordering::Release);
    let next = next_policy_request(&h, failed.pinned_operation()).await;
    h.complete_replay().await;
    let successor = wait_metadata(&next, |s| {
        s.read_mode() == ReadMode::ReplayComplete && s.transport() == ReadTransportState::Written
    })
    .await;
    assert_eq!(policy.calls.load(Ordering::Acquire), 3);
    assert!(
        successor
            .selected_options()
            .expect("new selected budget")
            .selected_at()
            > selected.selected_at()
    );
    assert_eq!(
        h.module.native_reads.status(&first).selected_options(),
        Some(selected),
        "old request never retargets new job"
    );
    let _ = h.module.reads.release(read);
    drop((first, failed, next));
    let report = h.module.shutdown(deadline()).await;
    assert!(report.io_joined() && report.pump_joined() && report.views_joined());
    assert!(h.owner.shutdown(deadline()).await.all_joined());
    drop(h.module);
    drop(policy);
    assert!(
        weak.upgrade().is_none(),
        "original coordinator released injected provider"
    );
    assert!(control.snapshot().is_closed());
    h.auth
        .close()
        .await
        .expect("actual Node and owned area cleanup");
}
