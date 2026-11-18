//! Developer regression using the real authenticated TLS/socket fixture.
use super::*;
use crate::domain::shared_view::ViewEpoch;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_restore_keeps_old_socket_packet_observer_and_rebind_closed() {
    let mut h = Harness::new(false).await;
    let s = Source::new(&h, raw_bytes(100)).await;
    let outcome = AssertUnwindSafe(async {
        let stream = s.stream(0);
        h.sessions[0].initial(&stream).await;
        let mut gate = s
            .owner
            .test_controller()
            .arm_pause(
                ViewHookPoint::BeforeSinkReady,
                HookSelector::Stream(s.opened[0].stream_id()),
            )
            .unwrap();
        s.send(&h, 0, 0).await;
        bound(gate.arrived()).await.unwrap();
        let views = s.owner.handle();
        views.core.send.seal_target(&s.target).unwrap();
        views.core.send.restore_target(&s.target).unwrap();
        let restored = views
            .core
            .send
            .view_for_target(ViewEpoch::fresh().unwrap(), &s.target)
            .unwrap();
        assert!(restored.current().unwrap());
        assert!(!stream.view_current().unwrap());
        // The writer owns the authentication gate while paused before sink ready.
        // Let it reject the sealed target before the observer acquires that gate.
        gate.release();
        until(|| stream.explicitly_sealed().unwrap()).await;
        let client = &h.sessions[0].attachment.client;
        assert!(matches!(
            s.reads.original_progress(client, &s.opened[0]).await,
            Err(ViewError::SourceReleased | ViewError::StreamSealed)
        ));
        assert!(matches!(
            s.reads
                .rebind_inflight_same_read(client, &s.opened[0])
                .await,
            Err(ViewError::SourceReleased | ViewError::StreamSealed)
        ));
        let observed = stream.observation().unwrap();
        assert_eq!(observed.highest_started, 0);
        assert_eq!(observed.highest_flushed, 0);
        assert_eq!(observed.highest_committed, 0);
        info(&mut h.sessions[0], 119).await;
    })
    .catch_unwind()
    .await;
    finish(h, s, outcome).await;
}
