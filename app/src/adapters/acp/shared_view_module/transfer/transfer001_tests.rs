use super::*;
mod inflight_development;
use crate::{
    adapters::acp::{Version, raw_json::RawDocument, test_support::setup},
    application::shared_views::controller::HookSelector,
    domain::{EntityId, native::NativeSourceId},
    shared_view_test_support::{
        FixturePeer, FixturePeerWire, FixtureSigningClient, FixtureSourceConfig, FixtureTaskOwner,
        LocalViewAuthFixture, OwnedTestArea,
    },
};
use futures_util::FutureExt;
use sha2::{Digest, Sha256};
use std::{panic::AssertUnwindSafe, time::Duration};
use tokio::time::{Instant, timeout_at};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
const STATE: &[u8] = br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"transfer-window","update":{"sessionUpdate":"state_update","state":"idle"}}}"#;
async fn send(peer: &mut FixturePeer, bytes: &[u8]) {
    peer.write_bytes(bytes, deadline())
        .await
        .expect("actual peer bytes");
    peer.write_bytes(b"\n", deadline())
        .await
        .expect("actual LF");
}
async fn respond(peer: &mut FixturePeer, result: &[u8], state: bool) {
    let frame = peer
        .next_node_frame(deadline())
        .await
        .expect("actual RPC IO")
        .expect("actual RPC frame");
    let doc = RawDocument::parse(frame).expect("request document");
    let id = doc
        .root()
        .object()
        .expect("object")
        .get("id")
        .expect("unique decoded field")
        .expect("actual RPC");
    if state {
        send(peer, STATE).await;
    }
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    response.extend_from_slice(id.fragment().as_bytes());
    response.extend_from_slice(b",\"result\":");
    response.extend_from_slice(result);
    response.push(b'}');
    send(peer, &response).await;
}
struct Rig {
    module: AcpViewModuleFixture,
    auth: Arc<LocalViewAuthFixture>,
    owner: FixtureTaskOwner,
    peer: FixturePeer,
    client: AuthenticatedLocalViewClient,
    target: FixtureTarget,
}
impl Rig {
    async fn start() -> Self {
        let area = OwnedTestArea::create(&std::env::temp_dir(), "shared-transfer001")
            .expect("unique owned area");
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
            .expect("synthetic TLS");
        area.install_tls(
            cert.cert.pem().as_bytes(),
            cert.signing_key.serialize_pem().as_bytes(),
        )
        .expect("new TLS files");
        let owner = FixtureTaskOwner::new(&area, tokio::runtime::Handle::current())
            .expect("owner before IO");
        let auth = Arc::new(
            LocalViewAuthFixture::start(area)
                .await
                .expect("actual temporary Node"),
        );
        let client = auth
            .authenticate(&FixtureSigningClient::generate().expect("synthetic signing key"))
            .await
            .expect("actual pairing/auth");
        let (wire, mut peer) =
            FixturePeerWire::from_duplex(&owner.handle(), tokio::io::duplex(64 * 1024))
                .unwrap_or_else(|_| panic!("original owned tuple"));
        let driver = tokio::spawn(async move {
            respond(&mut peer,br#"{"protocolVersion":2,"info":{"name":"fixture","version":"2"},"capabilities":{"session":{}}}"#,false).await;
            peer
        });
        let module = AcpViewModuleFixture::start(
            FixtureSourceConfig {
                source: NativeSourceId(EntityId::from_bytes([95; 16])),
                entry_alias: Arc::from("transfer-reproduction"),
            },
            wire,
            Arc::clone(&auth),
        )
        .await
        .expect("actual one Connection/pump");
        let mut peer = driver.await.expect("init driver joined");
        let driver = tokio::spawn(async move {
            respond(&mut peer, b"{}", true).await;
            peer
        });
        let sid = RawDocument::parse(br#""transfer-window""#.to_vec()).expect("SID raw");
        let sid = crate::adapters::acp::SessionId::parse(sid.root()).expect("SID data");
        let target = module
            .resume_existing(sid, setup(Version::V2, "C:\\fixture"))
            .await
            .expect("actual V2 snapshot and Resumed binding");
        Self {
            module,
            auth,
            owner,
            peer: driver.await.expect("resume driver joined"),
            client,
            target,
        }
    }
    async fn captured(&mut self, raw: &[u8]) {
        let views = self.module.views();
        let before = views.core.probe.retained_frontier();
        views
            .core
            .probe
            .wait_processed(before)
            .await
            .expect("earlier real prefix drained");
        let mut gate = self
            .module
            .test_controller()
            .observe_once(
                ViewHookPoint::AfterCaptureEnqueue,
                HookSelector::Source(self.module.scope()),
            )
            .expect("actual next captured raw");
        send(&mut self.peer, raw).await;
        let observed = timeout_at(deadline(), gate.arrived())
            .await
            .expect("capture cut")
            .expect("real original reader/worker");
        assert_eq!(observed.sequence(), Some(before + 1));
        views
            .core
            .probe
            .wait_processed(before + 1)
            .await
            .expect("actual full captured raw prefix");
    }
    async fn close(self) {
        let weak = Arc::downgrade(&self.module.core);
        let module = self.module.shutdown(deadline()).await;
        let owner = self.owner.shutdown(deadline()).await;
        drop(self.module);
        drop(self.peer);
        drop(self.client);
        drop(self.target);
        let released = weak.upgrade().is_none();
        let auth = self.auth.close().await;
        eprintln!(
            "[AUTH-CLOSE-FIRST] {:?}",
            crate::node::shared_view_test_support::auth_close_observation::first_close_snapshot(
                &self.auth
            )
        );
        eprintln!(
            "[TRANSFER001-CLOSE-REPORT] io={} pump={} views={} module_failure={:?} fixture_tasks={} fixture_resources={} fixture_failure={:?} weak={} auth={:?}",
            module.io_joined(),
            module.pump_joined(),
            module.views_joined(),
            module.failure(),
            owner.pending_tasks(),
            owner.pending_resources(),
            owner.failure(),
            released,
            auth
        );
        assert!(module.io_joined() && module.pump_joined() && module.views_joined());
        assert!(owner.all_joined());
        assert_eq!(owner.pending_tasks(), 0);
        assert_eq!(owner.pending_resources(), 0);
        assert!(released, "source/pump/read owners really released");
        auth.expect("actual Node/Storage join and first physical area cleanup");
        eprintln!(
            "[TRANSFER001-CLEANUP] module/io/pump/views=true fixture=true tasks=0 resources=0 auth/first-remove=Ok weak=true"
        );
    }
}
async fn flushed(rig: &Rig, stream: EntityId, packet: u64) {
    timeout_at(deadline(), async {
        loop {
            let observed = rig.module.probe().stream(stream).expect("real stream");
            if observed.highest_flushed() >= packet {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual published flush; no prelude early-credit race");
}
async fn state_record(rig: &Rig) -> (ModuleViewTransfer, EntityId, u64) {
    let mut started = rig
        .module
        .test_controller()
        .observe_once(
            ViewHookPoint::AfterStartSendBeforeFlush,
            HookSelector::Target(rig.target.key().clone()),
        )
        .expect("actual stream witness");
    let mut transfer = rig
        .module
        .open_transfer(&rig.client, &rig.target, ReadReason::FirstOpen)
        .await
        .expect("real open");
    let stream = timeout_at(deadline(), started.arrived())
        .await
        .expect("actual send")
        .expect("actual observation")
        .stream_id()
        .expect("stream");
    loop {
        let record = timeout_at(deadline(), transfer.next_record())
            .await
            .expect("real peer bytes")
            .expect("record");
        let packet = rig
            .module
            .probe()
            .stream(stream)
            .expect("stream")
            .highest_started();
        flushed(rig, stream, packet).await;
        if record.address().kind() == RecordKind::OriginalRaw && record.bytes() == STATE {
            return (transfer, stream, packet);
        }
        assert_ne!(
            record.address().kind(),
            RecordKind::SnapshotEnd,
            "actual Resume state must be in H"
        );
        transfer
            .credit(packet)
            .await
            .expect("ordinary actual prelude ack");
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_view_transfer001_detach_cancels_full_result_gate_without_release() {
    let rig = Rig::start().await;
    let (mut transfer, stream, p) = state_record(&rig).await;
    let mut gate = rig
        .module
        .test_controller()
        .arm_pause(
            ViewHookPoint::AfterModuleCreditReadBeforePublish,
            HookSelector::Stream(stream),
        )
        .expect("full-result gate");
    {
        let ack = transfer.credit(p);
        tokio::pin!(ack);
        tokio::select! {observation=gate.arrived()=>{assert_eq!(observation.expect("actual complete credit").packet_sequence(),Some(p));}, result=&mut ack=>panic!("ack unexpectedly completed before held actual result: {}",result.is_ok())}
    }
    let detached = tokio::time::timeout(Duration::from_millis(200), transfer.detach()).await;
    let completed = matches!(detached, Ok(Ok(_)));
    if !completed {
        gate.release();
        timeout_at(deadline(), transfer.detach())
            .await
            .expect("rescue only after failed cleanup assertion")
            .expect("actual release");
    }
    drop(transfer);
    rig.close().await;
    assert!(
        completed,
        "consumer close must cancel the original full-result hook without manual release"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_view_transfer001_real_late_early_credit() {
    late_early_scenario(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_view_transfer001_detach_after_next_flush_cancels_unreleased_result() {
    late_early_scenario(true).await;
}
async fn late_early_scenario(detaching: bool) {
    let mut rig = Rig::start().await;
    let text = "x".repeat(MAX_PAYLOAD_BYTES * 3 + 257);
    let raw=format!(r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"transfer-window","update":{{"sessionUpdate":"independent_unknown","n":123456789012345678901234567890,"body":"{text}"}}}}}}"#).into_bytes();
    let expected = Sha256::digest(&raw);
    let mut transfer = None;
    let mut held = None;
    let outcome=AssertUnwindSafe(async {
        rig.captured(&raw).await;
        let mut started=rig.module.test_controller().observe_once(ViewHookPoint::AfterStartSendBeforeFlush,HookSelector::Target(rig.target.key().clone())).expect("actual stream");
        transfer=Some(rig.module.open_transfer(&rig.client,&rig.target,ReadReason::FirstOpen).await.expect("original authenticated transfer"));
        let stream=timeout_at(deadline(),started.arrived()).await.expect("real data started").expect("observation").stream_id().expect("actual stream identity");
        let transfer=transfer.as_mut().expect("actual consumer");
        let p=loop {
            let record=timeout_at(deadline(),transfer.next_record()).await.expect("prelude peer record").expect("actual record");
            let packet=rig.module.probe().stream(stream).expect("stream").highest_started();
            flushed(&rig,stream,packet).await;
            if record.address().kind()==RecordKind::OriginalRaw&&record.bytes()==STATE {break packet;}
            transfer.credit(packet).await.expect("actual ordinary prelude credit");
        };
        let k=p.checked_add(1).expect("next packet");
        let mut writer=rig.module.test_controller().arm_pause(ViewHookPoint::AfterFlushFactBeforePublish,HookSelector::Stream(stream)).expect("next actual writer flush");
        transfer.credit(p).await.expect("actual final prelude credit");
        let point=timeout_at(deadline(),writer.arrived()).await.expect("actual raw flush").expect("writer gate");
        assert_eq!(point.packet_sequence(),Some(k));
        let mut delayed=rig.module.test_controller().arm_pause(ViewHookPoint::AfterModuleCreditReadBeforePublish,HookSelector::Stream(stream)).expect("only actual credit result publication");
        let mut credit=transfer.shared.credit.subscribe();
        let collecting=transfer.next_record();tokio::pin!(collecting);
        let received=tokio::select! {
            observation=delayed.arrived()=>observation.expect("actual complete credit read"),
            result=&mut collecting=>panic!("record completed before controlled early credit: {}",result.is_ok()),
        };
        assert_eq!(received.packet_sequence(),Some(k));
        assert!(rig.module.probe().stream(stream).expect("stream").pending_early_credit());
        let before_commit=credit.borrow().revision;
        writer.release();
        timeout_at(deadline(),async {loop {let current=*credit.borrow_and_update();let stream_state=rig.module.probe().stream(stream).expect("actual stream");if current.revision>before_commit&&stream_state.highest_committed()==k{break;}credit.changed().await.expect("actual publisher");}}).await.expect("real committed notification before late early notification");
        flushed(&rig,stream,k+1).await;
        let state=rig.module.probe().stream(stream).expect("actual state");
        assert_eq!(state.highest_committed(),k);assert!(!state.pending_early_credit());
        if detaching {held=Some(delayed);return None;}
        let committed_revision=credit.borrow().revision;
        delayed.release();
        timeout_at(deadline(),async {loop {let current=*credit.borrow_and_update();if current.revision>committed_revision&&matches!(current.result,Ok(LocalCreditResult::HeldEarly)){break;}credit.changed().await.expect("actual delayed publication");}}).await.expect("actual delayed HeldEarly publication after committed publication");
        eprintln!("[TRANSFER001-ORDER] wire/late={k} started={} flushed={} committed={} watch_packet={:?} watch=HeldEarly",state.highest_started(),state.highest_flushed(),state.highest_committed(),credit.borrow().packet);
        match timeout_at(deadline(),&mut collecting).await {
            Ok(Ok(record))=>Some(Sha256::digest(record.bytes())),
            Ok(Err(_))|Err(_)=>None,
        }
    }).catch_unwind().await;
    let mut detached_without_release = true;
    if let Some(transfer) = transfer.as_mut() {
        if detaching {
            detached_without_release = matches!(
                tokio::time::timeout(Duration::from_millis(200), transfer.detach()).await,
                Ok(Ok(_))
            );
            if !detached_without_release {
                if let Some(gate) = held.take() {
                    gate.release();
                }
                timeout_at(deadline(), transfer.detach())
                    .await
                    .expect("rescue original close")
                    .expect("actual receipt");
            }
        } else {
            timeout_at(deadline(), transfer.detach())
                .await
                .expect("real peer detach deadline")
                .expect("actual read release");
        }
    }
    drop(held);
    drop(transfer);
    rig.close().await;
    let actual = outcome.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    if detaching {
        assert!(
            detached_without_release,
            "Detach must cancel an unreleased actual result after the next real packet flushed"
        );
    } else {
        assert_eq!(
            actual,
            Some(expected),
            "original complete raw SHA must arrive after normal owned cleanup"
        );
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_view_transfer001_actual_old_duplicate_and_future_ack_are_not_watermarks() {
    let rig = Rig::start().await;
    let (mut transfer, stream, p) = state_record(&rig).await;
    let result = AssertUnwindSafe(async {
        transfer.credit(p).await.expect("actual current ack");
        flushed(&rig, stream, p + 1).await;
        transfer
            .credit(p - 1)
            .await
            .expect("actual older duplicate ack");
        let fact = *transfer.shared.credit.borrow();
        assert_eq!(fact.packet, Some(p - 1));
        assert!(matches!(fact.result, Ok(LocalCreditResult::Duplicate)));
        assert_eq!(fact.committed_through, p);
        assert_eq!(
            rig.module
                .probe()
                .stream(stream)
                .expect("stream")
                .highest_committed(),
            p,
            "duplicate does not grant next permit"
        );
        transfer
            .credit(0)
            .await
            .expect("actual duplicate initial ack");
        assert_eq!(transfer.shared.credit.borrow().packet, Some(0));
        let invalid = transfer.credit(p + 2).await;
        assert!(matches!(invalid, Err(ViewError::InvalidCursor)));
        assert_eq!(
            transfer.shared.credit.borrow().packet,
            None,
            "invalid actual input has no validated parsed credit carrier"
        );
        let read = rig
            .module
            .reads
            .open_current(&rig.client, rig.target.key())
            .await
            .expect("invalid consumer did not seal shared source");
        let _ = rig.module.reads.release(read);
    })
    .catch_unwind()
    .await;
    let closed = timeout_at(deadline(), transfer.detach())
        .await
        .expect("actual consumer close")
        .expect("real receipt");
    assert_eq!(
        closed.last_committed_packet(),
        p,
        "rejected future credit cannot commit a later packet"
    );
    drop(transfer);
    rig.close().await;
    result.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_view_transfer001_cancelled_complete_read_then_same_ack_is_only_idempotent_observation()
 {
    let rig = Rig::start().await;
    let (mut transfer, stream, p) = state_record(&rig).await;
    let mut gate = rig
        .module
        .test_controller()
        .arm_pause(
            ViewHookPoint::AfterModuleCreditReadBeforePublish,
            HookSelector::Stream(stream),
        )
        .expect("original parsed result only");
    let result=AssertUnwindSafe(async {
        {
            let ack=transfer.credit(p);tokio::pin!(ack);
            tokio::select! {observation=gate.arrived()=>{assert_eq!(observation.expect("complete original parsed ack").packet_sequence(),Some(p));}, result=&mut ack=>panic!("unexpected pre-publication completion: {}",result.is_ok())}
        }
        let before=transfer.shared.credit.borrow().received_revision;
        {let again=transfer.credit(p);tokio::pin!(again);
        let poll=std::future::poll_fn(|cx| std::task::Poll::Ready(std::future::Future::poll(again.as_mut(),cx))).await;
        assert!(poll.is_pending());
        gate.release();
        timeout_at(deadline(),again).await.expect("actual same ack observed").expect("idempotent ack success");}
        let fact=*transfer.shared.credit.borrow();
        assert!(fact.received_revision>before);
        assert_eq!(fact.packet,Some(p));
        assert_eq!(rig.module.probe().stream(stream).expect("stream").highest_committed(),p);
        // The late original result may satisfy this same-ack observation. No assertion
        // assigns it to the second physical frame or manufactures a per-call receipt.
    }).catch_unwind().await;
    timeout_at(deadline(), transfer.detach())
        .await
        .expect("actual detach")
        .expect("real receipt");
    drop(transfer);
    rig.close().await;
    result.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_view_transfer001_partial_header_never_publishes_complete_result_and_owner_stops_it()
{
    let rig = Rig::start().await;
    let (mut transfer, _, p) = state_record(&rig).await;
    let mut observer = transfer.shared.credit.subscribe();
    let before = observer.borrow().received_revision;
    let bytes = TransferHeader::credit(transfer.stream, p)
        .encode(&[])
        .expect("actual same-stream header");
    transfer
        .peer
        .as_mut()
        .expect("original peer")
        .write_all(&bytes[..17])
        .await
        .expect("real partial header bytes");
    let complete = tokio::time::timeout(Duration::from_millis(100), observer.changed()).await;
    let unchanged = complete.is_err() && observer.borrow().received_revision == before;
    // The prefix is terminal: close the actual endpoint and owner. Never restart
    // read_exact on it, append a new whole header, or transfer it to a new stream.
    transfer.peer.take();
    drop(transfer);
    rig.close().await;
    assert!(
        unchanged,
        "partial wire input cannot publish a complete credit outcome"
    );
}
