use super::*;
use std::future::{Future, poll_fn};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::{Duration, Instant, timeout_at},
};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
async fn poll_once<F: Future>(mut future: Pin<&mut F>) -> Poll<F::Output> {
    poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx))).await
}

#[tokio::test]
async fn shared_view_controlled_io_real_prefix_cancelled_observation_drop_and_next_nonce() {
    let control = FixtureIoController::new();
    let (node, mut peer) = tokio::io::duplex(64);
    let mut node = ControlledDuplex::new(node, control.clone());
    let mut gate = control
        .arm(FixtureIoPlan::WritePrefix {
            forwarded: NonZeroUsize::new(2).expect("nonzero"),
        })
        .expect("arm");
    {
        let waiting = gate.arrived();
        tokio::pin!(waiting);
        assert!(poll_once(waiting.as_mut()).await.is_pending());
    }
    let prior = control.snapshot();
    let changed = control.changed_after(&prior);
    tokio::pin!(changed);
    assert!(poll_once(changed.as_mut()).await.is_pending());
    let writing = tokio::spawn(async move {
        node.write_all(b"abcdef").await?;
        node.flush().await
    });
    let mut prefix = [0; 2];
    peer.read_exact(&mut prefix).await.expect("real bytes");
    assert_eq!(&prefix, b"ab");
    timeout_at(deadline(), changed)
        .await
        .expect("no lost wake")
        .expect("actual change");
    let observed = timeout_at(deadline(), gate.arrived())
        .await
        .expect("actual next write")
        .expect("gate remains owned");
    assert_eq!(observed.boundary(), FixtureIoBoundary::WritePrefixPending);
    assert_eq!(observed.forwarded_since_arm(), 2);
    assert_eq!(control.snapshot().forwarded_bytes(), 2);
    assert!(matches!(
        control.arm(FixtureIoPlan::NextFlush),
        Err(FixtureError::ControlBusy)
    ));
    drop(gate);
    let mut next = control
        .arm(FixtureIoPlan::NextFlush)
        .expect("new independent nonce");
    let mut tail = [0; 4];
    peer.read_exact(&mut tail)
        .await
        .expect("same original remainder");
    assert_eq!(&tail, b"cdef");
    timeout_at(deadline(), next.arrived())
        .await
        .expect("flush real poll")
        .expect("old Drop did not unarm new gate");
    assert_eq!(control.snapshot().delegated_flushes(), 0);
    next.release().expect("delegate actual flush");
    timeout_at(deadline(), writing)
        .await
        .expect("writer joins")
        .expect("task")
        .expect("actual flush result");
    assert_eq!(control.snapshot().delegated_flushes(), 1);
    assert_eq!(control.snapshot().forwarded_bytes(), 6);
}

#[tokio::test]
async fn shared_view_controlled_io_read_passthrough_foreign_snapshot_and_stop_wake() {
    let control = FixtureIoController::new();
    let other = FixtureIoController::new();
    assert!(matches!(
        control.changed_after(&other.snapshot()).await,
        Err(FixtureError::Unsupported)
    ));
    let (node, mut peer) = tokio::io::duplex(64);
    let (mut reader, mut writer) = tokio::io::split(ControlledDuplex::new(node, control.clone()));
    let mut gate = control.arm(FixtureIoPlan::NextFlush).expect("arm");
    let writing = tokio::spawn(async move {
        writer.write_all(b"out").await?;
        writer.flush().await
    });
    let mut bytes = [0; 3];
    peer.read_exact(&mut bytes)
        .await
        .expect("actual full write");
    timeout_at(deadline(), gate.arrived())
        .await
        .expect("Pending flush")
        .expect("actual boundary");
    peer.write_all(b"in").await.expect("read side stays real");
    let mut incoming = [0; 2];
    reader
        .read_exact(&mut incoming)
        .await
        .expect("not blocked by flush gate");
    assert_eq!(&incoming, b"in");
    let before = control.snapshot();
    let observation = control.changed_after(&before);
    tokio::pin!(observation);
    assert!(poll_once(observation.as_mut()).await.is_pending());
    control.close();
    assert!(matches!(
        timeout_at(deadline(), observation)
            .await
            .expect("stop wakes observer"),
        Err(FixtureError::Cancelled)
    ));
    assert!(matches!(gate.arrived().await, Err(FixtureError::Cancelled)));
    let error = timeout_at(deadline(), writing)
        .await
        .expect("stop wakes actual writer")
        .expect("writer joins")
        .expect_err("no fake flush success");
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(control.snapshot().delegated_flushes(), 0);
    assert!(control.snapshot().is_closed());
}

#[tokio::test]
async fn shared_view_controlled_io_failed_prefix_is_only_actual_forwarded_bytes() {
    let control = FixtureIoController::new();
    let (node, mut peer) = tokio::io::duplex(64);
    let mut node = ControlledDuplex::new(node, control.clone());
    let mut gate = control
        .arm(FixtureIoPlan::WritePrefix {
            forwarded: NonZeroUsize::new(1).expect("nonzero"),
        })
        .expect("arm");
    let writing = tokio::spawn(async move { node.write_all(b"whole frame").await });
    timeout_at(deadline(), gate.arrived())
        .await
        .expect("partial write actual next poll")
        .expect("observed");
    gate.fail(FixtureIoFault::BrokenPipe)
        .expect("single controlled IO error");
    let error = timeout_at(deadline(), writing)
        .await
        .expect("actual IO resolves")
        .expect("task join")
        .expect_err("real write path error");
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    let mut received = Vec::new();
    peer.read_to_end(&mut received)
        .await
        .expect("original endpoint EOF");
    assert_eq!(&received, b"w");
    assert_eq!(control.snapshot().forwarded_bytes(), 1);
    assert_eq!(control.snapshot().delegated_flushes(), 0);
}
struct OriginalInitializeFence(
    crate::adapters::acp::ConnectionScope,
    crate::adapters::acp::RpcId,
);
impl crate::adapters::acp::WriteFence for OriginalInitializeFence {
    fn check_and_poll(
        &self,
        identity: &crate::adapters::acp::WriteIdentity,
        _step: crate::adapters::acp::WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn crate::adapters::acp::PollIo,
    ) -> Result<(), crate::adapters::acp::FenceRejection> {
        if identity.scope() != self.0 || identity.id() != Some(&self.1) {
            return Err(crate::adapters::acp::FenceRejection::StaleTarget);
        }
        let _ = io.poll(cx);
        Ok(())
    }
}
#[tokio::test]
async fn shared_view_controlled_io_owner_cancelled_shutdown_keeps_real_io_and_weak_release() {
    use crate::{
        adapters::acp::{Connection, Version},
        shared_view_test_support::{FixturePeerWire, FixtureTaskOwner, OwnedTestArea},
    };
    let mut area =
        OwnedTestArea::create(&std::env::temp_dir(), "shared-control-owner").expect("owned child");
    let owner =
        FixtureTaskOwner::new(&area, tokio::runtime::Handle::current()).expect("registered owner");
    let (mut wire, mut peer, control) =
        FixturePeerWire::from_controlled_duplex(&owner.handle(), tokio::io::duplex(4096))
            .unwrap_or_else(|_| panic!("original tuple"));
    let (node, lease) = wire.take_node().expect("one original endpoint");
    let (reader, writer) = tokio::io::split(node);
    let (connection, events, io) = Connection::spawn(reader, writer).expect("actual IO and scope");
    lease
        .install_io(io)
        .unwrap_or_else(|_| panic!("original IoOwner"));
    let weak = {
        let io = peer.resource.io.lock().expect("resource slot");
        Arc::downgrade(io.as_ref().expect("same actual IoOwner"))
    };
    let mut gate = control.arm(FixtureIoPlan::NextFlush).expect("real flush");
    let reservation = connection
        .reserve_initialize(Version::V1, deadline())
        .expect("real reservation");
    let rpc = reservation.id().expect("real RPC").clone();
    let frame = reservation.encode().expect("actual frame");
    let ticket = connection
        .enqueue(
            reservation,
            frame,
            Arc::new(OriginalInitializeFence(connection.scope(), rpc)),
            deadline(),
        )
        .expect("original enqueue");
    peer.next_node_frame(deadline())
        .await
        .expect("actual peer bytes")
        .expect("actual LF frame");
    timeout_at(deadline(), gate.arrived())
        .await
        .expect("real pending flush")
        .expect("observed");
    {
        let closing = owner.shutdown(deadline());
        tokio::pin!(closing);
        assert!(
            poll_once(closing.as_mut()).await.is_pending(),
            "actual original IO tasks still need to join after stop"
        );
    }
    assert!(control.snapshot().is_closed());
    assert!(matches!(gate.arrived().await, Err(FixtureError::Cancelled)));
    assert!(
        timeout_at(deadline(), owner.shutdown(deadline()))
            .await
            .expect("same owner resumes close")
            .all_joined()
    );
    drop((connection, events, ticket, peer, wire, lease, owner));
    timeout_at(deadline(), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("retained controller holds no IO owner/task");
    assert!(
        area.cleanup()
            .await
            .expect("first physical owned cleanup")
            .removed()
    );
}
