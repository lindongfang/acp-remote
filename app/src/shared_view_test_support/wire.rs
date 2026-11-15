//! Owned real duplex endpoints. Scope exists only after the actual ACP Connection is spawned.
use super::{FixtureError, FixtureOwnerHandle, FixtureOwnerReport};
use crate::adapters::acp::IoOwner;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::{
    sync::watch,
    time::{Instant, timeout_at},
};
mod control;
pub use control::{
    FixtureIoBoundary, FixtureIoController, FixtureIoFault, FixtureIoGate, FixtureIoObservation,
    FixtureIoPlan, FixtureIoSnapshot,
};

pub struct AdoptRejected<T> {
    error: FixtureError,
    original: T,
}
impl<T> AdoptRejected<T> {
    #[must_use]
    pub const fn error(&self) -> FixtureError {
        self.error
    }
    #[must_use]
    pub fn into_original(self) -> T {
        self.original
    }
}
pub struct FixturePeerWire {
    resource: Arc<dyn WireResource>,
    owner: FixtureOwnerHandle,
    handed_off: bool,
}
pub struct FixturePeer {
    resource: Arc<DuplexResource>,
}
#[derive(Clone)]
pub struct FixtureResourceLease {
    resource: Arc<dyn WireResource>,
}
pub(super) trait WireResource: Send + Sync {
    #[cfg(all(test, windows))]
    fn process_report(&self) -> Option<crate::platform::process_owner::ShutdownReport> {
        None
    }
    fn closed(&self) -> bool;
    fn request_stop(&self);
    fn take_node(&self) -> Result<FixtureNodeIo, FixtureError>;
    fn install_io(&self, owner: IoOwner) -> Result<(), AdoptRejected<IoOwner>>;
    fn shutdown(
        &self,
        deadline: Instant,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>>;
}
pub enum FixtureNodeIo {
    Duplex(DuplexStream),
    ControlledDuplex(control::ControlledDuplex),
    #[cfg(windows)]
    Child {
        reader: tokio::net::windows::named_pipe::NamedPipeServer,
        writer: tokio::net::windows::named_pipe::NamedPipeServer,
    },
}
impl tokio::io::AsyncRead for FixtureNodeIo {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Duplex(stream) => std::pin::Pin::new(stream).poll_read(cx, buffer),
            Self::ControlledDuplex(stream) => std::pin::Pin::new(stream).poll_read(cx, buffer),
            #[cfg(windows)]
            Self::Child { reader, .. } => std::pin::Pin::new(reader).poll_read(cx, buffer),
        }
    }
}
impl tokio::io::AsyncWrite for FixtureNodeIo {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match &mut *self {
            Self::Duplex(stream) => std::pin::Pin::new(stream).poll_write(cx, bytes),
            Self::ControlledDuplex(stream) => std::pin::Pin::new(stream).poll_write(cx, bytes),
            #[cfg(windows)]
            Self::Child { writer, .. } => std::pin::Pin::new(writer).poll_write(cx, bytes),
        }
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Duplex(stream) => std::pin::Pin::new(stream).poll_flush(cx),
            Self::ControlledDuplex(stream) => std::pin::Pin::new(stream).poll_flush(cx),
            #[cfg(windows)]
            Self::Child { writer, .. } => std::pin::Pin::new(writer).poll_flush(cx),
        }
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Duplex(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
            Self::ControlledDuplex(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
            #[cfg(windows)]
            Self::Child { writer, .. } => std::pin::Pin::new(writer).poll_shutdown(cx),
        }
    }
}
pub(super) struct DuplexResource {
    node: Mutex<Option<DuplexStream>>,
    peer: tokio::sync::Mutex<PeerState>,
    io: Mutex<Option<Arc<IoOwner>>>,
    node_taken: AtomicBool,
    closed: AtomicBool,
    stop: watch::Sender<bool>,
    control: Option<FixtureIoController>,
}
struct PeerState {
    stream: Option<DuplexStream>,
    partial: Vec<u8>,
}
impl FixturePeerWire {
    /// # Errors
    /// Pre-adoption failure returns the same actual tuple. Both endpoints remain in the owner
    /// after successful adoption; no Source/Scope/Received is constructed here.
    pub fn from_duplex(
        owner: &FixtureOwnerHandle,
        pair: (DuplexStream, DuplexStream),
    ) -> Result<(Self, FixturePeer), AdoptRejected<(DuplexStream, DuplexStream)>> {
        Self::adopt_duplex(owner, pair, None)
    }
    /// # Errors
    /// Adopts the same original endpoints; the controller changes only actual node-side IO.
    pub fn from_controlled_duplex(
        owner: &FixtureOwnerHandle,
        pair: (DuplexStream, DuplexStream),
    ) -> Result<(Self, FixturePeer, FixtureIoController), AdoptRejected<(DuplexStream, DuplexStream)>>
    {
        let control = FixtureIoController::new();
        let (wire, peer) = Self::adopt_duplex(owner, pair, Some(control.clone()))?;
        Ok((wire, peer, control))
    }
    fn adopt_duplex(
        owner: &FixtureOwnerHandle,
        pair: (DuplexStream, DuplexStream),
        control: Option<FixtureIoController>,
    ) -> Result<(Self, FixturePeer), AdoptRejected<(DuplexStream, DuplexStream)>> {
        let Ok(mut slots) = owner.inner.resources.lock() else {
            return Err(AdoptRejected {
                error: FixtureError::JoinUnconfirmed,
                original: pair,
            });
        };
        if owner.inner.released.load(Ordering::Acquire) || *owner.inner.stop.borrow() {
            return Err(AdoptRejected {
                error: FixtureError::Cancelled,
                original: pair,
            });
        }
        if slots.try_reserve(1).is_err() {
            return Err(AdoptRejected {
                error: FixtureError::Allocation,
                original: pair,
            });
        }
        let (stop, _) = watch::channel(false);
        let resource = Arc::new(DuplexResource {
            node: Mutex::new(Some(pair.0)),
            peer: tokio::sync::Mutex::new(PeerState {
                stream: Some(pair.1),
                partial: Vec::new(),
            }),
            io: Mutex::new(None),
            node_taken: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            stop,
            control,
        });
        let adopted: Arc<dyn WireResource> = resource.clone();
        slots.push(Arc::clone(&adopted));
        drop(slots);
        Ok((
            Self {
                resource: adopted,
                owner: owner.clone(),
                handed_off: false,
            },
            FixturePeer { resource },
        ))
    }
    pub(crate) fn resource_owner(&self) -> FixtureOwnerHandle {
        self.owner.clone()
    }
    pub(crate) fn take_node(
        &mut self,
    ) -> Result<(FixtureNodeIo, FixtureResourceLease), FixtureError> {
        let node = self.resource.take_node()?;
        self.handed_off = true;
        Ok((
            node,
            FixtureResourceLease {
                resource: Arc::clone(&self.resource),
            },
        ))
    }
}
impl Drop for FixturePeerWire {
    fn drop(&mut self) {
        if !self.handed_off {
            self.resource.request_stop();
        }
    }
}
impl Drop for FixturePeer {
    fn drop(&mut self) {
        self.resource.request_stop();
    }
}
impl FixtureResourceLease {
    pub(crate) fn install_io(&self, owner: IoOwner) -> Result<(), AdoptRejected<IoOwner>> {
        self.resource.install_io(owner)
    }
    pub async fn shutdown(&self, deadline: Instant) -> FixtureOwnerReport {
        let done = self.resource.shutdown(deadline).await;
        FixtureOwnerReport {
            tasks: 0,
            resources: usize::from(!done),
            failure: (!done).then_some(FixtureError::JoinUnconfirmed),
        }
    }
}
impl DuplexResource {
    pub(super) fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    pub(super) async fn shutdown(&self, deadline: Instant) -> bool {
        self.request_stop();
        if self.closed() {
            return true;
        }
        let done = timeout_at(deadline, async {
            let mut peer = self.peer.lock().await;
            peer.stream.take();
            drop(peer);
            let io = self
                .io
                .lock()
                .map_err(|_| FixtureError::JoinUnconfirmed)?
                .clone();
            if let Some(io) = io {
                if !io.shutdown(deadline).await.joined {
                    return Err(FixtureError::JoinUnconfirmed);
                }
            } else {
                let mut node = self
                    .node
                    .lock()
                    .map_err(|_| FixtureError::JoinUnconfirmed)?;
                if self.node_taken.load(Ordering::Acquire) {
                    return Err(FixtureError::JoinUnconfirmed);
                }
                node.take();
            }
            Ok(())
        })
        .await;
        if matches!(done, Ok(Ok(()))) {
            self.closed.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }
}
impl WireResource for DuplexResource {
    fn closed(&self) -> bool {
        Self::closed(self)
    }
    fn request_stop(&self) {
        if let Some(control) = &self.control {
            control.close();
        }
        self.stop.send_replace(true);
    }
    fn take_node(&self) -> Result<FixtureNodeIo, FixtureError> {
        if *self.stop.borrow() {
            return Err(FixtureError::Cancelled);
        }
        let mut slot = self
            .node
            .lock()
            .map_err(|_| FixtureError::JoinUnconfirmed)?;
        if *self.stop.borrow() {
            return Err(FixtureError::Cancelled);
        }
        let node = slot.take().ok_or(FixtureError::AlreadyTaken)?;
        self.node_taken.store(true, Ordering::Release);
        drop(slot);
        Ok(match &self.control {
            Some(control) => FixtureNodeIo::ControlledDuplex(control::ControlledDuplex::new(
                node,
                control.clone(),
            )),
            None => FixtureNodeIo::Duplex(node),
        })
    }
    fn install_io(&self, owner: IoOwner) -> Result<(), AdoptRejected<IoOwner>> {
        let Ok(mut slot) = self.io.lock() else {
            return Err(AdoptRejected {
                error: FixtureError::JoinUnconfirmed,
                original: owner,
            });
        };
        if slot.is_some() {
            return Err(AdoptRejected {
                error: FixtureError::AlreadyTaken,
                original: owner,
            });
        }
        *slot = Some(Arc::new(owner));
        Ok(())
    }
    fn shutdown(
        &self,
        deadline: Instant,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> {
        Box::pin(Self::shutdown(self, deadline))
    }
}
#[cfg(windows)]
mod child;
#[cfg(windows)]
pub use child::ChildWireAdoptError;
impl FixturePeer {
    /// # Errors
    /// The actual IO/deadline result is returned. A partial failed write is never retried here.
    pub async fn write_bytes(
        &mut self,
        bytes: &[u8],
        deadline: Instant,
    ) -> Result<(), FixtureError> {
        let mut stop = self.resource.stop.subscribe();
        let operation = async {
            let mut peer = self.resource.peer.lock().await;
            peer.stream
                .as_mut()
                .ok_or(FixtureError::Cancelled)?
                .write_all(bytes)
                .await
                .map_err(|_| FixtureError::Io)
        };
        tokio::select! {result=timeout_at(deadline,operation)=>result.map_err(|_|FixtureError::Timeout)?,() = wait_stopped(&mut stop)=>Err(FixtureError::Cancelled)}
    }
    /// # Errors
    /// Reads an actual LF frame without a total/frame quota. Partial bytes remain owned if
    /// this waiter is cancelled, so the next call cannot invent a new frame boundary.
    pub async fn next_node_frame(
        &mut self,
        deadline: Instant,
    ) -> Result<Option<Vec<u8>>, FixtureError> {
        let mut stop = self.resource.stop.subscribe();
        let operation = async {
            let mut peer = self.resource.peer.lock().await;
            loop {
                let mut byte = [0];
                let count = peer
                    .stream
                    .as_mut()
                    .ok_or(FixtureError::Cancelled)?
                    .read(&mut byte)
                    .await
                    .map_err(|_| FixtureError::Io)?;
                if count == 0 {
                    return if peer.partial.is_empty() {
                        Ok(None)
                    } else {
                        Err(FixtureError::Io)
                    };
                }
                if byte[0] == b'\n' {
                    return Ok(Some(std::mem::take(&mut peer.partial)));
                }
                peer.partial
                    .try_reserve(1)
                    .map_err(|_| FixtureError::Allocation)?;
                peer.partial.push(byte[0]);
            }
        };
        tokio::select! {result=timeout_at(deadline,operation)=>result.map_err(|_|FixtureError::Timeout)?,() = wait_stopped(&mut stop)=>Err(FixtureError::Cancelled)}
    }
    pub async fn close(&mut self, deadline: Instant) -> FixtureOwnerReport {
        FixtureResourceLease {
            resource: self.resource.clone(),
        }
        .shutdown(deadline)
        .await
    }
}
async fn wait_stopped(stop: &mut watch::Receiver<bool>) {
    if *stop.borrow() {
        return;
    }
    while stop.changed().await.is_ok() {
        if *stop.borrow() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FixtureTaskOwner, OwnedTestArea};
    use super::*;
    use crate::adapters::acp::*;
    use tokio::runtime::Handle;
    struct InitializeFence {
        scope: ConnectionScope,
        rpc: RpcId,
    }
    impl WriteFence for InitializeFence {
        fn check_and_poll(
            &self,
            identity: &WriteIdentity,
            _step: WriteStep,
            cx: &mut std::task::Context<'_>,
            io: &mut dyn PollIo,
        ) -> Result<(), FenceRejection> {
            if identity.scope() != self.scope
                || identity.id() != Some(&self.rpc)
                || !matches!(
                    identity.kind(),
                    OutboundKind::Request(RequestKind::Initialize(Version::V1))
                )
            {
                return Err(FenceRejection::StaleTarget);
            }
            let _ = io.poll(cx);
            Ok(())
        }
    }
    #[tokio::test]
    async fn shared_view_fixture_duplex_adoption_drives_actual_acp_and_joins_original_io() {
        let mut area =
            OwnedTestArea::create(&std::env::temp_dir(), "shared-view-peer").expect("owned area");
        let owner = FixtureTaskOwner::new(&area, Handle::current()).expect("registered owner");
        let (mut wire, mut peer) =
            FixturePeerWire::from_duplex(&owner.handle(), tokio::io::duplex(1024))
                .unwrap_or_else(|_| panic!("actual original pair adoption"));
        let (node, lease) = wire.take_node().expect("same original node endpoint");
        let (reader, writer) = tokio::io::split(node);
        let (connection, mut received, io) =
            Connection::spawn(reader, writer).expect("actual ACP connection creates Scope");
        lease
            .install_io(io)
            .unwrap_or_else(|_| panic!("same original IoOwner installed"));
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let reserved = connection
            .reserve_initialize(Version::V1, deadline)
            .expect("actual initialize reservation");
        let rpc = reserved.id().expect("actual typed RPC").clone();
        let frame = reserved.encode().expect("real wire");
        let ticket = connection
            .enqueue(
                reserved,
                frame,
                Arc::new(InitializeFence {
                    scope: connection.scope(),
                    rpc: rpc.clone(),
                }),
                deadline,
            )
            .expect("actual initialize enqueue");
        let request = peer
            .next_node_frame(deadline)
            .await
            .expect("real peer read")
            .expect("one LF frame");
        assert!(request.starts_with(b"{\"jsonrpc\""));
        let mut id = Vec::new();
        rpc.write_json(&mut id).expect("typed RPC encoding");
        let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
        response.extend(id);
        response.extend_from_slice(br#", "result":{"protocolVersion":1,"agentInfo":{"name":"fixture","version":"1"},"agentCapabilities":{}}}"#);
        response.push(b'\n');
        peer.write_bytes(&response, deadline)
            .await
            .expect("real peer writes response");
        let event = timeout_at(deadline, received.next())
            .await
            .expect("reader wait")
            .expect("actual parse")
            .expect("actual Received");
        assert_eq!(event.scope(), connection.scope());
        assert_eq!(event.sequence(), 1);
        assert!(matches!(
            event.incoming(),
            Incoming::Response {
                response: Response::Initialized(_),
                ..
            }
        ));
        assert!(matches!(ticket.wait().await, WriteReport::Written));
        assert!(
            owner.shutdown(deadline).await.all_joined(),
            "same owner closes original ACP IO and actual peer"
        );
        assert!(
            area.cleanup()
                .await
                .expect("same canonical owned child")
                .removed()
        );
    }
}
