//! Independent test calls use the actual public runtime and reader-created events.
#![allow(dead_code)] // This fixture is shared by two separately selectable modules.
#[path = "../acp_runtime_acceptance/support/mod.rs"]
mod fresh;
use acp_remote::adapters::acp::*;
pub use fresh::{raw_value, rpc_value, setup};
use serde_json::Value;
use std::{
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Notify,
    time::{Duration, Instant, timeout},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyId(pub RpcId);
impl LegacyId {
    pub fn json(&self) -> Value {
        rpc_value(&self.0)
    }
}
pub trait RpcJson {
    fn json(&self) -> Value;
}
impl RpcJson for RpcId {
    fn json(&self) -> Value {
        rpc_value(self)
    }
}
pub fn raw_field(raw: &RawMessage, name: &str) -> Option<Value> {
    raw.field(name)
        .unwrap()
        .map(|v| serde_json::from_str(v.fragment()).unwrap())
}

#[derive(Clone, Default)]
pub struct WriterControl {
    mode: Arc<AtomicU8>,
    accepted: Arc<AtomicUsize>,
    pending: Arc<Notify>,
}
impl WriterControl {
    pub fn mode(&self, value: u8) {
        self.mode.store(value, Ordering::SeqCst);
    }
    pub async fn wait_pending(&self) {
        timeout(Duration::from_secs(10), self.pending.notified())
            .await
            .unwrap();
    }
    pub fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }
}
struct ObservedWrite<W> {
    inner: W,
    control: WriterControl,
}
impl<W: AsyncWrite + Unpin> AsyncWrite for ObservedWrite<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match this.control.mode.load(Ordering::SeqCst) {
            1 => Poll::Ready(Err(io::Error::other("synthetic writer failure"))),
            2 => {
                this.control.pending.notify_one();
                Poll::Pending
            }
            3 => {
                let accepted = this.control.accepted();
                if accepted >= 8 {
                    this.control.pending.notify_one();
                    return Poll::Pending;
                }
                let n = bytes.len().min(8 - accepted);
                let result = Pin::new(&mut this.inner).poll_write(cx, &bytes[..n]);
                if let Poll::Ready(Ok(n)) = result {
                    this.control.accepted.fetch_add(n, Ordering::SeqCst);
                }
                result
            }
            _ => Pin::new(&mut this.inner).poll_write(cx, bytes),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
pub struct TestClient {
    pub connection: Connection,
    pub receiver: EventReceiver,
    pub owner: IoOwner,
    pub control: WriterControl,
}
impl TestClient {
    pub fn new<R, W>(read: R, write: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let control = WriterControl::default();
        let (connection, receiver, owner) = Connection::spawn(
            read,
            ObservedWrite {
                inner: write,
                control: control.clone(),
            },
        )
        .unwrap();
        Self {
            connection,
            receiver,
            owner,
            control,
        }
    }
    pub fn strategy(&self) -> Option<Strategy> {
        self.connection.strategy()
    }
    pub fn is_closed(&self) -> bool {
        self.connection.is_closed()
    }
    pub fn pending_count(&self) -> usize {
        self.connection.pending_count()
    }
    pub fn enqueue(
        &self,
        reservation: Reservation,
        deadline: Instant,
    ) -> Result<WriteTicket, AcpError> {
        let frame = reservation.encode()?;
        self.connection
            .enqueue(reservation, frame, Arc::new(fresh::Fence), deadline)
            .map_err(|e| e.error())
    }
    async fn submit(
        &self,
        reservation: Reservation,
        deadline: Instant,
    ) -> Result<LegacyId, AcpError> {
        let id = reservation.id().cloned().ok_or(AcpError::InvalidMessage)?;
        written(self.enqueue(reservation, deadline)?.wait().await)?;
        Ok(LegacyId(id))
    }
    pub async fn initialize(
        &self,
        version: Version,
        deadline: Instant,
    ) -> Result<LegacyId, AcpError> {
        self.submit(
            self.connection.reserve_initialize(version, deadline)?,
            deadline,
        )
        .await
    }
    pub async fn initialize_preferred(&self, deadline: Instant) -> Result<LegacyId, AcpError> {
        self.submit(
            self.connection.reserve_preferred_initialize(deadline)?,
            deadline,
        )
        .await
    }
    pub async fn request(&self, request: Request, deadline: Instant) -> Result<LegacyId, AcpError> {
        self.submit(
            self.connection
                .reserve_request(request, ResponseDeadline::At(deadline))?,
            deadline,
        )
        .await
    }
    pub async fn cancel(&self, session: &SessionId, deadline: Instant) -> Result<(), AcpError> {
        let reservation = self
            .connection
            .reserve_cancel(session.clone(), ExtensionFields::empty()?)?;
        written(self.enqueue(reservation, deadline)?.wait().await)
    }
    pub async fn next(&mut self, _: Instant) -> Result<Incoming, AcpError> {
        Ok(self
            .receiver
            .next()
            .await?
            .ok_or(AcpError::Closed)?
            .incoming()
            .clone())
    }
    pub async fn permission_response(
        &self,
        request: &PermissionRequest,
        choice: Option<&OptionId>,
        deadline: Instant,
    ) -> Result<(), AcpError> {
        let choice = choice.map_or(PermissionSelection::Cancelled, |id| {
            PermissionSelection::Selected(id.clone())
        });
        let reservation = self
            .connection
            .reserve_permission_response(request, choice)?;
        written(self.enqueue(reservation, deadline)?.wait().await)
    }
    pub async fn shutdown(&self, deadline: Instant) -> Result<(), AcpError> {
        let report = self.owner.shutdown(deadline).await;
        if report.joined {
            report.error.map_or(Ok(()), Err)
        } else {
            Err(AcpError::Deadline)
        }
    }
}
const fn written(report: WriteReport) -> Result<(), AcpError> {
    match report {
        WriteReport::Written => Ok(()),
        WriteReport::Zero {
            reason: ZeroReason::Deadline,
        } => Err(AcpError::Deadline),
        WriteReport::Unknown { .. } => Err(AcpError::DeliveryUnknown),
        WriteReport::Zero { .. } => Err(AcpError::Closed),
    }
}
pub trait LegacySetup {
    fn legacy_replay(&self, session: SessionId, cwd: &str) -> Result<Request, AcpError>;
    fn legacy_resume(&self, session: SessionId, cwd: &str) -> Result<Request, AcpError>;
}
impl LegacySetup for Strategy {
    fn legacy_replay(&self, session: SessionId, cwd: &str) -> Result<Request, AcpError> {
        self.replay(session, &setup(self.version(), cwd))
    }
    fn legacy_resume(&self, session: SessionId, cwd: &str) -> Result<Request, AcpError> {
        self.resume(session, &setup(self.version(), cwd))
    }
}

pub async fn drop_pending_waiter(ticket: &WriteTicket) {
    let mut waiting = Box::pin(ticket.wait());
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(waiting.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(waiting);
}
