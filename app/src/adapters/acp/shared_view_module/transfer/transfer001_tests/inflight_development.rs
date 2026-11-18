use super::{Rig, STATE, deadline, flushed, state_record};
use crate::{
    adapters::acp::{
        raw_json::RawDocument,
        shared_view_module::{ModuleOriginalReadProgress, ModuleViewTransfer},
    },
    application::shared_views::{
        budget::ReadReason, reader::OriginalReadProgress, wire::RecordKind,
    },
    domain::EntityId,
};
use futures_util::FutureExt;
use std::{
    panic::AssertUnwindSafe,
    sync::{Arc, atomic::Ordering},
};
use tokio::time::timeout_at;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_restore_keeps_original_local_observer_and_rebind_closed() {
    let rig = Rig::start().await;
    let outcome = AssertUnwindSafe(async {
        let old = rig
            .module
            .reads
            .open_current(&rig.client, rig.target.key())
            .await
            .unwrap();
        let observer = rig.module.reads.original_observer(&old).unwrap();
        let views = rig.module.views();
        views.core.send.seal_target(rig.target.key()).unwrap();
        views.core.send.restore_target(rig.target.key()).unwrap();
        let restored = views
            .core
            .send
            .view_for_target(
                crate::domain::shared_view::ViewEpoch::fresh().unwrap(),
                rig.target.key(),
            )
            .unwrap();
        assert!(restored.current().unwrap());
        assert_eq!(
            rig.module.reads.original_progress(&rig.client, &old).await,
            Err(crate::domain::shared_view::ViewError::SourceReleased)
        );
        assert_eq!(
            rig.module
                .reads
                .try_observe_original(&rig.client, &observer)
                .await,
            Err(crate::domain::shared_view::ViewError::SourceReleased)
        );
        assert!(matches!(
            rig.module
                .reads
                .rebind_inflight_same_read(&rig.client, &old)
                .await,
            Err(crate::domain::shared_view::ViewError::SourceReleased)
        ));
        let _close = rig.module.reads.release(old);
    })
    .catch_unwind()
    .await;
    rig.close().await;
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

async fn unbusy_progress(transfer: &ModuleViewTransfer) -> ModuleOriginalReadProgress {
    timeout_at(deadline(), async {
        loop {
            let progress = transfer.original_progress().await.unwrap();
            if progress != ModuleOriginalReadProgress::Busy {
                return progress;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("ordinary current-auth gate becomes available; Busy is not IO Pending")
}

async fn wait_idle(transfer: &ModuleViewTransfer) {
    timeout_at(deadline(), async {
        loop {
            if matches!(
                *transfer.shared.lane.lock().unwrap(),
                super::super::ContinuationLane::AwaitData
            ) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual sender AwaitData, not an invented IO fact");
}
async fn drain_original(
    transfer: &mut ModuleViewTransfer,
    rig: &Rig,
    stream: EntityId,
    packet: u64,
) {
    transfer.credit(packet).await.unwrap();
    loop {
        let shared = Arc::clone(&transfer.shared);
        let ready = async move {
            loop {
                if matches!(
                    *shared.lane.lock().unwrap(),
                    super::super::ContinuationLane::AwaitData
                ) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        };
        let record = timeout_at(deadline(), async {
            tokio::select! {
                record=transfer.next_record()=>Some(record.expect("original peer record")),
                ()=ready=>None,
            }
        })
        .await
        .expect("old H reaches actual idle without waiting for Replay Response");
        let Some(_) = record else {
            break;
        };
        let packet = rig.module.probe().stream(stream).unwrap().highest_started();
        flushed(rig, stream, packet).await;
        transfer.credit(packet).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn module_inflight_quiet_peer_two_raw_records_use_original_request_and_real_bootstrap() {
    let mut rig = Rig::start().await;
    let outcome=AssertUnwindSafe(async {
        let (mut transfer,old_stream,old_packet)=state_record(&rig).await;
        let request=rig.peer.next_node_frame(deadline()).await.unwrap().unwrap();
        let document=RawDocument::parse(request).unwrap();
        let root=document.root().object().unwrap();
        assert!(root.get("params").expect("unique decoded field").unwrap().object().unwrap().get("replayFrom").expect("unique decoded field").is_some(),"actual standard Replay RPC");
        // A separate actual consumer joins the same in-flight native job; this also
        // exercises the ordinary authenticated wait helper without a new RPC/budget.
        let witness=rig.module.reads.open(&rig.client,rig.target.key(),ReadReason::FirstOpen).await.unwrap();
        let old_epoch=witness.descriptor().view_epoch();
        let raw_one=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"transfer-window","update":{"sessionUpdate":"_quiet_future","data":{"n":1234567890123456789012345678901234567890,"body":"one"}}}}"#;
        let raw_two=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"transfer-window","update":{"sessionUpdate":"_quiet_future","data":{"missing":null,"body":"two"}}}}"#;
        rig.captured(raw_one).await;
        rig.captured(raw_two).await;
        // This peer now sends no Response and no later notification.
        let progress=timeout_at(deadline(),rig.module.reads.wait_original_progress(&rig.client,&witness)).await.unwrap().unwrap();
        assert!(matches!(progress,OriginalReadProgress::CandidateReady{..}));
        assert_eq!(unbusy_progress(&transfer).await,ModuleOriginalReadProgress::CandidateReady);
        assert_eq!(rig.module.policy.calls.load(Ordering::Acquire),1);
        let calls=rig.module.policy.calls.load(Ordering::Acquire);
        let enqueues=rig.module.core.enqueues.load(Ordering::Acquire);
        assert!(matches!(transfer.follow_original_inflight().await,Err(crate::domain::shared_view::ViewError::InvalidCredit)));
        drain_original(&mut transfer,&rig,old_stream,old_packet).await;
        wait_idle(&transfer).await;
        assert_eq!(unbusy_progress(&transfer).await,ModuleOriginalReadProgress::CandidateReady);
        let follow_deadline=deadline();
        let mut attempt=0u32;
        let followed=loop {
            timeout_at(follow_deadline,wait_idle(&transfer)).await.unwrap();
            attempt=attempt.checked_add(1).unwrap();
            eprintln!("[INFLIGHT-CALL-BEGIN] {attempt}");
            let followed=timeout_at(follow_deadline,transfer.follow_original_inflight()).await.unwrap();
            eprintln!("[INFLIGHT-CALL-RETURN] {attempt} error={:?}",followed.as_ref().err());
            let observed=transfer.shared.current_stream.lock().unwrap().observation().unwrap();
            if matches!(followed,Err(crate::domain::shared_view::ViewError::InvalidStage))
                && transfer.last_follow_rejection==Some(super::super::original_read::FollowRejection::PreflightBusy)
                && observed.phase==crate::application::shared_views::local_transfer::LocalTransferPhase::Ready
                && observed.highest_started==observed.highest_flushed && observed.highest_flushed==observed.highest_committed
            {
                assert_eq!(EntityId::from_bytes(*transfer.stream.as_bytes()),old_stream);
                assert_eq!(rig.module.policy.calls.load(Ordering::Acquire),calls);
                assert_eq!(rig.module.core.enqueues.load(Ordering::Acquire),enqueues);
                eprintln!("[INFLIGHT-CALL-RETRY] {attempt} unchanged_stream=true packet={} provider={calls} enqueues={enqueues}",observed.highest_committed);
                tokio::task::yield_now().await;
                continue;
            }
            break followed;
        };
        if let Err(error)=followed {
            let stream=transfer.shared.current_stream.lock().unwrap().clone();
            let observed=stream.observation().unwrap();
            eprintln!("[INFLIGHT-FOLLOW-ERROR] error={error:?} phase={:?} started={} flushed={} committed={} public_progress={:?}",observed.phase,observed.highest_started,observed.highest_flushed,observed.highest_committed,transfer.original_progress().await);
        }
        let notice=followed.unwrap();
        assert_eq!(notice.address().kind(),RecordKind::Status);
        let stream=EntityId::from_bytes(*transfer.stream.as_bytes());
        assert_ne!(stream,old_stream);
        assert_ne!(transfer.epoch,old_epoch);
        assert_eq!(witness.descriptor().view_epoch(),old_epoch,"original fixed H never jumps");
        assert_eq!(rig.module.policy.calls.load(Ordering::Acquire),calls);
        assert_eq!(rig.module.core.enqueues.load(Ordering::Acquire),enqueues);
        flushed(&rig,stream,1).await;
        let observed=rig.module.probe().stream(stream).unwrap();
        assert_eq!(observed.highest_flushed(),1);
        assert_eq!(observed.highest_committed(),0,"actual notice receipt does not imply credit one");
        transfer.credit(1).await.unwrap();
        let mut raw=Vec::new();
        while raw.len()<2 {
            let record=timeout_at(deadline(),transfer.next_record()).await.unwrap().unwrap();
            let packet=rig.module.probe().stream(stream).unwrap().highest_started();
            flushed(&rig,stream,packet).await;
            if record.address().kind()==RecordKind::OriginalRaw { raw.push(record.bytes().to_vec()); }
            transfer.credit(packet).await.unwrap();
        }
        assert_eq!(raw,vec![raw_one.to_vec(),raw_two.to_vec()]);
        assert!(!raw.iter().any(|bytes|bytes==STATE),"old load raw is not mixed into the Replay candidate");
        assert_eq!(unbusy_progress(&transfer).await,ModuleOriginalReadProgress::CandidateReady,"no fabricated complete without actual Response");
        assert_eq!(rig.module.policy.calls.load(Ordering::Acquire),calls);
        assert_eq!(rig.module.core.enqueues.load(Ordering::Acquire),enqueues);
        let _ = rig.module.reads.release(witness);
        transfer.detach().await.unwrap();
    }).catch_unwind().await;
    rig.close().await;
    if let Err(error) = outcome {
        std::panic::resume_unwind(error);
    }
}
