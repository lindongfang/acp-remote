#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use std::{
    io,
    pin::Pin,
    sync::{Arc, Condvar, Mutex, mpsc},
    task::{Context, Poll},
    time::Duration,
};
use support::*;
use tokio::io::AsyncWrite;

struct Gate {
    released: Mutex<bool>,
    changed: Condvar,
    arrived: mpsc::Sender<()>,
}
impl Gate {
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
struct ReleaseOnExit(Arc<Gate>);
impl Drop for ReleaseOnExit {
    fn drop(&mut self) {
        self.0.release();
    }
}
struct DropHeld<W> {
    inner: W,
    gate: Arc<Gate>,
}
impl<W: AsyncWrite + Unpin> AsyncWrite for DropHeld<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, bytes)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
impl<W> Drop for DropHeld<W> {
    fn drop(&mut self) {
        let _ = self.gate.arrived.send(());
        let mut released = self.gate.released.lock().unwrap();
        while !*released {
            released = self.gate.changed.wait(released).unwrap();
        }
        drop(released);
    }
}

#[test]
fn lifecycle001_runtime_a_cancelled_collector_cannot_claim_join_before_writer_exits() {
    let (arrived_tx, arrived_rx) = mpsc::channel();
    let gate = Arc::new(Gate {
        released: Mutex::new(false),
        changed: Condvar::new(),
        arrived: arrived_tx,
    });
    let _release = ReleaseOnExit(gate.clone());
    let (bundle_tx, bundle_rx) = mpsc::channel();
    let (stopped_tx, stopped_rx) = mpsc::channel();
    let child_gate = gate.clone();
    let thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let bundle = runtime.block_on(async move {
            let (read, r, rweak) = ReadControl::new();
            let (_write, w, wweak) = WriteControl::new();
            let (client, mut receiver, owner) = Connection::spawn(
                r,
                DropHeld {
                    inner: w,
                    gate: child_gate,
                },
            )
            .unwrap();
            let res = client.reserve_initialize(Version::V2, deadline()).unwrap();
            let id = res.id().unwrap().clone();
            let ticket = send(&client, res);
            finish(&ticket).await;
            read.frame(
                serde_json::to_vec(
                    &json!({"jsonrpc":"2.0","id":rpc_value(&id),"result":handshake(Version::V2)}),
                )
                .unwrap(),
            );
            assert!(receiver.next().await.unwrap().is_some());
            (client, receiver, owner, rweak, wweak)
        });
        bundle_tx.send(bundle).unwrap();
        runtime.shutdown_background();
        stopped_tx.send(()).unwrap();
    });
    let (client, receiver, owner, rweak, wweak) =
        bundle_rx.recv_timeout(Duration::from_secs(20)).unwrap();
    stopped_rx.recv_timeout(Duration::from_secs(20)).unwrap();
    arrived_rx.recv_timeout(Duration::from_secs(20)).unwrap();
    assert!(wweak.upgrade().is_some());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut first = Box::pin(owner.shutdown(deadline()));
        let first_poll = std::future::poll_fn(|cx| Poll::Ready(first.as_mut().poll(cx))).await;
        assert!(
            first_poll.is_pending(),
            "writer Drop is still held; join cannot be complete"
        );
        drop(first);
        let before = owner
            .shutdown(tokio::time::Instant::now() + Duration::from_millis(150))
            .await;
        assert!(
            !before.joined,
            "runtime A cancellation is not an acknowledgement of writer exit"
        );
        assert!(wweak.upgrade().is_some());
        gate.release();
        let final_report = owner.shutdown(deadline()).await;
        assert!(final_report.joined);
        assert!(rweak.upgrade().is_none());
        assert!(wweak.upgrade().is_none());
        assert!(client.is_closed());
    });
    drop(receiver);
    drop(client);
    drop(owner);
    thread.join().unwrap();
}
