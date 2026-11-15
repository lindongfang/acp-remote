#![allow(
    clippy::cast_possible_truncation,
    clippy::items_after_statements,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::needless_pass_by_ref_mut,
    clippy::option_if_let_else,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    clippy::used_underscore_binding,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use axum::{
    Router,
    extract::{
        Extension, State,
        ws::{CloseFrame, Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
    },
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{RETRY_AFTER, SEC_WEBSOCKET_PROTOCOL},
    },
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::{SinkExt, StreamExt, stream::SplitStream};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::{
    sync::{Notify, OwnedMutexGuard, OwnedSemaphorePermit, Semaphore, mpsc, watch},
    time::timeout,
};

use crate::application::shared_views::{
    socket_authority::{SocketLifetime, SocketViewAuthority, SocketViewClient},
    socket_transfer::{PreparedSocketChunk, QueuedSocketChunk, SocketViewStream},
};
#[cfg(windows)]
use crate::node::{
    NativeSocketViewRead, NodeCatalogManagement, NodeManagementSessionServices, UploadOwner,
    UploadRoutingError,
};
use crate::{
    access_policy::{
        AccessPolicyError, ApplicationPolicy, AuthenticatedActor as AuthenticatedClient,
    },
    challenge::{ChallengeError, IssuedChallenge},
    claims::{AuthProof, Id128, NodeId, Nonce256, PublicKeySec1},
    config::NodeConfig,
    identity::NodeIdentity,
    ipc::IpcControl,
    jws::{JwsError, JwsType},
    pairing::{PairingError, PairingService},
    pairing_http::{PeerAddress, security_headers, validate_origin_and_host},
    strict_json::{MAX_PROTOCOL_JSON_DEPTH, parse_object},
    trust::{TrustError, TrustStore},
};

mod connection_owner;
use connection_owner::{ConnectionContext, ConnectionLease, ConnectionRegistry, ConnectionStop};

#[cfg(test)]
use connection_owner::{AcquireError, ConnectionObservation};

pub(crate) mod input_upload;
mod session_commands;
pub mod session_protocol;

pub(crate) const CONTROL_PATH: &str = "/api/access/v1/control";
pub(crate) const CONTROL_PROTOCOL: &str = "acpr.access.v1";
pub(crate) const MAX_MANAGEMENT_FRAME_BYTES: usize = 32 * 1024;

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const PROOF_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_WINDOW: Duration = Duration::from_secs(60);
const MAX_COMMANDS_PER_WINDOW: u16 = 120;
const MAX_MANAGEMENT_CONNECTIONS: usize = 128;
const MAX_UNAUTHENTICATED_CONNECTIONS: usize = 32;
const MAX_CONNECTIONS_PER_CLIENT: usize = 8;
const MAX_OUTBOUND_BYTES: usize = 1024 * 1024;
const MAX_OUTBOUND_FRAMES: usize = 256;
const WRITER_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const WRITER_SEND_TIMEOUT: Duration = Duration::from_secs(15);
const RATE_WINDOW: Duration = Duration::from_secs(60);
const SOURCE_IDLE: Duration = Duration::from_mins(5);
const MAX_SOURCE_BUCKETS: usize = 1_024;
const HANDSHAKE_LIMIT_PER_SOURCE: u16 = 30;
const HANDSHAKE_LIMIT_GLOBAL: u16 = 120;

const CLOSE_FORMAT: u16 = 4400;
const CLOSE_AUTHENTICATION: u16 = 4401;
const CLOSE_TIMEOUT: u16 = 4408;
const CLOSE_REVOKED: u16 = 4410;
const CLOSE_RATE_LIMITED: u16 = 4429;
const CLOSE_INTERNAL: u16 = 4500;
const CLOSE_GOING_AWAY: u16 = 1001;

pub(crate) struct ManagementService {
    state: Arc<ManagementState>,
}

impl ManagementService {
    /// Creates the authenticated management transport for one node.
    ///
    /// # Errors
    ///
    /// Returns an error when the node identity cannot be represented by the protocol key types.
    pub fn new(
        identity: Arc<NodeIdentity>,
        origin: String,
        trust: TrustStore,
        pairing: Arc<PairingService>,
        control: IpcControl,
        application_policy: Arc<ApplicationPolicy>,
    ) -> Result<Self, JwsError> {
        let public_key: [u8; 65] = identity
            .public_key()
            .try_into()
            .map_err(|_| JwsError::InvalidPublicKey)?;
        let node_public_key = PublicKeySec1::new(public_key)?;
        let node_id = NodeId::from_public_key(&node_public_key)?;
        let canonical =
            NodeConfig::normalize_origin(&origin).map_err(|_| JwsError::InvalidPayload)?;
        if canonical != origin {
            return Err(JwsError::InvalidPayload);
        }
        let socket_authority = SocketViewAuthority::new(application_policy.clone(), trust.clone());
        Ok(Self {
            state: Arc::new(ManagementState {
                socket_authority,
                application_policy,
                identity,
                origin: Arc::from(origin),
                trust,
                pairing,
                control,
                node_id,
                node_public_key,
                limiter: HandshakeLimiter::default(),
                connections: ConnectionRegistry::new_shared(),
                #[cfg(windows)]
                session_services: std::sync::OnceLock::new(),
                #[cfg(test)]
                write_barrier: Mutex::new(None),
                #[cfg(test)]
                auth_resolve_barrier: Mutex::new(None),
                #[cfg(test)]
                socket_test_accept: Mutex::new(None),
                #[cfg(test)]
                test_hooks: ManagementTestHooks::default(),
            }),
        })
    }

    #[cfg(windows)]
    pub(crate) fn install_session_services(
        &self,
        services: NodeManagementSessionServices,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        if !services.belongs_to(&self.state.socket_authority) {
            return Err(crate::domain::shared_view::ViewError::InvalidScope);
        }
        self.state
            .session_services
            .set(services)
            .map_err(|_| crate::domain::shared_view::ViewError::AlreadyAttached)
    }
    #[cfg(test)]
    pub(crate) fn observe_next_socket_client_for_upload_test(
        &self,
    ) -> Result<
        impl std::future::Future<
            Output = Result<SocketViewClient, crate::domain::shared_view::ViewError>,
        > + Send,
        crate::domain::shared_view::ViewError,
    > {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut slot = self
            .state
            .socket_test_accept
            .lock()
            .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
        if slot.is_some() {
            return Err(crate::domain::shared_view::ViewError::AlreadyAttached);
        }
        *slot = Some(tx);
        Ok(async move {
            let (attachment, _) = rx
                .await
                .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
            Ok(attachment.client.context.client())
        })
    }
    #[cfg(test)]
    pub(crate) fn observe_next_document_socket_for_test(
        &self,
    ) -> Result<
        impl std::future::Future<
            Output = Result<DocumentSocketEndpoint, crate::domain::shared_view::ViewError>,
        > + Send,
        crate::domain::shared_view::ViewError,
    > {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut slot = self
            .state
            .socket_test_accept
            .lock()
            .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
        if slot.is_some() {
            return Err(crate::domain::shared_view::ViewError::AlreadyAttached);
        }
        *slot = Some(tx);
        Ok(async move {
            let (attachment, outbound) = rx
                .await
                .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
            Ok(DocumentSocketEndpoint {
                attachment,
                outbound,
            })
        })
    }
    pub(crate) fn socket_authority(&self) -> Arc<SocketViewAuthority> {
        self.state.socket_authority.clone()
    }

    pub(crate) fn router(&self) -> Router {
        Router::new()
            .route(CONTROL_PATH, get(upgrade))
            .layer(middleware::from_fn(security_headers))
            .with_state(Arc::clone(&self.state))
    }

    pub(crate) fn begin_shutdown(&self) {
        self.state.connections.begin_shutdown();
    }

    pub(crate) async fn wait_closed(&self) {
        self.state.connections.wait_empty().await;
    }

    #[cfg(test)]
    pub(crate) fn pause_next_write(&self) -> crate::test_support::SharedAsyncBarrier {
        let barrier = std::sync::Arc::new(crate::test_support::AsyncBarrier::default());
        let mut slot = self
            .state
            .write_barrier
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert!(slot.is_none(), "only one write barrier can be installed");
        *slot = Some(std::sync::Arc::clone(&barrier));
        barrier
    }

    #[cfg(test)]
    pub(crate) fn pause_next_after_client_resolve(
        &self,
    ) -> crate::test_support::SharedAsyncBarrier {
        let barrier = std::sync::Arc::new(crate::test_support::AsyncBarrier::default());
        let mut slot = self
            .state
            .auth_resolve_barrier
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert!(
            slot.is_none(),
            "only one authentication resolve barrier can be installed"
        );
        *slot = Some(std::sync::Arc::clone(&barrier));
        barrier
    }

    #[cfg(test)]
    pub(crate) fn pause_next_boundary(
        &self,
        boundary: ManagementBoundary,
    ) -> crate::test_support::SharedAsyncBarrier {
        let barrier = Arc::new(crate::test_support::AsyncBarrier::default());
        let slot = match boundary {
            ManagementBoundary::Admission => &self.state.test_hooks.admission,
            ManagementBoundary::UpgradeRegistered => &self.state.test_hooks.upgrade_registered,
            ManagementBoundary::RejectedClose => &self.state.test_hooks.rejected_close,
            ManagementBoundary::Enqueue => &self.state.test_hooks.enqueue,
            ManagementBoundary::WriteLocked => &self.state.test_hooks.write_locked,
            ManagementBoundary::SubscriptionRevision => {
                &self.state.test_hooks.subscription_revision
            }
            ManagementBoundary::BeforeInputDispatch => &self.state.test_hooks.before_input_dispatch,
            ManagementBoundary::BeforeViewControlDispatch => {
                &self.state.test_hooks.before_view_control_dispatch
            }
            ManagementBoundary::InputAcceptedBeforeAck => {
                &self.state.test_hooks.input_accepted_before_ack
            }
        };
        let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
        assert!(slot.is_none(), "one boundary barrier at a time");
        *slot = Some(Arc::clone(&barrier));
        barrier
    }

    #[cfg(test)]
    pub(crate) fn boundary_counts(&self) -> (usize, usize) {
        use std::sync::atomic::Ordering;
        (
            self.state.test_hooks.admitted.load(Ordering::SeqCst),
            self.state.test_hooks.enqueued.load(Ordering::SeqCst),
        )
    }

    #[cfg(test)]
    pub(crate) fn slow_writer_fixture(&self, fresh_command_window: bool) {
        self.state
            .test_hooks
            .fresh_command_window
            .store(fresh_command_window, std::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn protected_write_count(&self) -> usize {
        self.state
            .test_hooks
            .protected_writes
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn queue_failure_snapshot(&self) -> (usize, usize, usize, usize) {
        use std::sync::atomic::Ordering;
        let hooks = &self.state.test_hooks;
        (
            hooks.overflow_bytes.load(Ordering::SeqCst),
            hooks.overflow_frames.load(Ordering::SeqCst),
            hooks.rejected_bytes.load(Ordering::SeqCst),
            hooks.last_close_code.load(Ordering::SeqCst),
        )
    }
    #[cfg(test)]
    pub(crate) fn connection_counts_for_test(&self) -> (usize, usize, usize, usize) {
        let snapshot = self.state.connections.snapshot_for_test();
        (
            snapshot.total,
            snapshot.slots,
            snapshot.protocol_handles,
            snapshot.writer_handles,
        )
    }
    #[cfg(test)]
    pub(crate) fn writer_close_count_for_test(&self) -> usize {
        self.state
            .test_hooks
            .writer_closes
            .load(std::sync::atomic::Ordering::SeqCst)
    }
    #[cfg(test)]
    pub(crate) fn pause_next_upload_command(
        &self,
        action: UploadCommandAction,
        boundary: UploadCommandBoundary,
    ) -> crate::test_support::SharedAsyncBarrier {
        let barrier = Arc::new(crate::test_support::AsyncBarrier::default());
        let mut slot = self
            .state
            .test_hooks
            .upload_command
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert!(
            slot.is_none(),
            "only one upload command barrier can be installed"
        );
        *slot = Some((action, boundary, Arc::clone(&barrier)));
        barrier
    }
    #[cfg(test)]
    pub(crate) fn pause_next_input_dispatch(&self) -> crate::test_support::SharedAsyncBarrier {
        self.pause_next_boundary(ManagementBoundary::BeforeInputDispatch)
    }
    #[cfg(test)]
    pub(crate) fn pause_next_view_control_dispatch(
        &self,
    ) -> crate::test_support::SharedAsyncBarrier {
        self.pause_next_boundary(ManagementBoundary::BeforeViewControlDispatch)
    }
    #[cfg(test)]
    pub(crate) fn pause_next_input_accepted_before_ack(
        &self,
    ) -> crate::test_support::SharedAsyncBarrier {
        self.pause_next_boundary(ManagementBoundary::InputAcceptedBeforeAck)
    }
}

impl Drop for ManagementService {
    fn drop(&mut self) {
        self.begin_shutdown();
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) enum ManagementBoundary {
    Admission,
    UpgradeRegistered,
    RejectedClose,
    Enqueue,
    SubscriptionRevision,
    WriteLocked,
    BeforeInputDispatch,
    BeforeViewControlDispatch,
    InputAcceptedBeforeAck,
}
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum UploadCommandAction {
    Begin,
    Finish,
    Status,
    Release,
}
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UploadCommandBoundary {
    BeforeCall,
    AfterCall,
}

#[cfg(test)]
#[derive(Default)]
struct ManagementTestHooks {
    protocol_failure: std::sync::atomic::AtomicBool,
    upgrade_registered: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    rejected_close: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    admission: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    enqueue: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    subscription_revision: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    write_locked: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    before_input_dispatch: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    before_view_control_dispatch: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    input_accepted_before_ack: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    upload_command: Mutex<
        Option<(
            UploadCommandAction,
            UploadCommandBoundary,
            crate::test_support::SharedAsyncBarrier,
        )>,
    >,
    protected_writes: std::sync::atomic::AtomicUsize,
    admitted: std::sync::atomic::AtomicUsize,
    enqueued: std::sync::atomic::AtomicUsize,
    fresh_command_window: std::sync::atomic::AtomicBool,
    overflow_bytes: std::sync::atomic::AtomicUsize,
    overflow_frames: std::sync::atomic::AtomicUsize,
    rejected_bytes: std::sync::atomic::AtomicUsize,
    last_close_code: std::sync::atomic::AtomicUsize,
    writer_closes: std::sync::atomic::AtomicUsize,
    writer_settlement: Mutex<Option<WriterSettlement>>,
}

#[cfg(test)]
async fn pause_boundary(slot: &Mutex<Option<crate::test_support::SharedAsyncBarrier>>) {
    let barrier = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(barrier) = barrier {
        barrier.pause().await;
    }
}
#[cfg(test)]
async fn pause_upload_command(
    state: &ManagementState,
    action: UploadCommandAction,
    boundary: UploadCommandBoundary,
) {
    let barrier = {
        let mut slot = state
            .test_hooks
            .upload_command
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if slot
            .as_ref()
            .is_some_and(|(armed, current, _)| *armed == action && *current == boundary)
        {
            slot.take().map(|(_, _, barrier)| barrier)
        } else {
            None
        }
    };
    if let Some(barrier) = barrier {
        barrier.pause().await;
    }
}

struct ManagementState {
    socket_authority: Arc<SocketViewAuthority>,
    application_policy: Arc<ApplicationPolicy>,
    identity: Arc<NodeIdentity>,
    origin: Arc<str>,
    trust: TrustStore,
    pairing: Arc<PairingService>,
    control: IpcControl,
    node_id: NodeId,
    node_public_key: PublicKeySec1,
    limiter: HandshakeLimiter,
    connections: Arc<ConnectionRegistry>,
    #[cfg(windows)]
    session_services: std::sync::OnceLock<NodeManagementSessionServices>,
    #[cfg(test)]
    write_barrier: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    #[cfg(test)]
    auth_resolve_barrier: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    #[cfg(test)]
    socket_test_accept: Mutex<Option<tokio::sync::oneshot::Sender<SocketTestAccepted>>>,
    #[cfg(test)]
    test_hooks: ManagementTestHooks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RateLimitRejection {
    retry_after_seconds: u64,
}

#[derive(Clone)]
struct HandshakeLimiter {
    state: Arc<Mutex<HandshakeRateState>>,
}

struct HandshakeRateState {
    sources: HashMap<IpAddr, SourceWindow>,
    global_started: Instant,
    global_count: u16,
}

struct SourceWindow {
    started: Instant,
    count: u16,
    last_seen: Instant,
}

impl Default for HandshakeLimiter {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(HandshakeRateState {
                sources: HashMap::new(),
                global_started: Instant::now(),
                global_count: 0,
            })),
        }
    }
}

impl HandshakeLimiter {
    fn check(&self, source: SocketAddr) -> Result<(), RateLimitRejection> {
        self.check_at(source, Instant::now())
    }

    fn check_at(&self, source: SocketAddr, now: Instant) -> Result<(), RateLimitRejection> {
        let source = source.ip();
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state
            .sources
            .retain(|_, window| now.saturating_duration_since(window.last_seen) < SOURCE_IDLE);
        if now.saturating_duration_since(state.global_started) >= RATE_WINDOW {
            state.global_started = now;
            state.global_count = 0;
        }
        state.global_count = state.global_count.saturating_add(1);

        let source_started = if let Some(window) = state.sources.get_mut(&source) {
            if now.saturating_duration_since(window.started) >= RATE_WINDOW {
                window.started = now;
                window.count = 0;
            }
            window.count = window.count.saturating_add(1);
            window.last_seen = now;
            Some(window.started)
        } else if state.sources.len() < MAX_SOURCE_BUCKETS {
            state.sources.insert(
                source,
                SourceWindow {
                    started: now,
                    count: 1,
                    last_seen: now,
                },
            );
            Some(now)
        } else {
            None
        };

        let source_count = source_started
            .and_then(|_| state.sources.get(&source))
            .map_or(u16::MAX, |window| window.count);
        let source_over = source_count > HANDSHAKE_LIMIT_PER_SOURCE;
        let global_over = state.global_count > HANDSHAKE_LIMIT_GLOBAL;
        if !source_over && !global_over {
            return Ok(());
        }

        let source_deadline = source_started
            .and_then(|started| started.checked_add(RATE_WINDOW))
            .unwrap_or(now);
        let global_deadline = state.global_started.checked_add(RATE_WINDOW).unwrap_or(now);
        drop(state);
        let deadline = if source_over && global_over {
            std::cmp::max(source_deadline, global_deadline)
        } else if source_over {
            source_deadline
        } else {
            global_deadline
        };
        Err(RateLimitRejection {
            retry_after_seconds: seconds_until(deadline, now),
        })
    }
}

async fn upgrade(
    State(state): State<Arc<ManagementState>>,
    peer: Option<Extension<PeerAddress>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let Some(Extension(PeerAddress(peer))) = peer else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if let Err(rejection) = state.limiter.check(peer) {
        return rate_limit_response(rejection.retry_after_seconds);
    }
    if !state.control.is_ready() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if !validate_origin_and_host(&headers, &state.origin) || !has_control_protocol(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let Ok(ticket) = state.connections.prepare_upgrade() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    #[cfg(test)]
    pause_boundary(&state.test_hooks.upgrade_registered).await;
    ws.protocols([CONTROL_PROTOCOL])
        .max_message_size(MAX_MANAGEMENT_FRAME_BYTES + 1)
        .max_frame_size(MAX_MANAGEMENT_FRAME_BYTES + 1)
        .on_upgrade(move |socket| ticket.adopt_socket(socket, state))
        .into_response()
}

fn has_control_protocol(headers: &HeaderMap) -> bool {
    let mut values = headers.get_all(SEC_WEBSOCKET_PROTOCOL).iter();
    let Some(value) = values.next() else {
        return false;
    };
    if values.next().is_some() {
        return false;
    }
    let Ok(value) = value.to_str() else {
        return false;
    };
    let mut protocols = value.split(',').map(str::trim);
    protocols.next() == Some(CONTROL_PROTOCOL) && protocols.next().is_none()
}

fn rate_limit_response(retry_after_seconds: u64) -> Response {
    let mut response = StatusCode::TOO_MANY_REQUESTS.into_response();
    if let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string()) {
        response.headers_mut().insert(RETRY_AFTER, value);
    }
    response
}

async fn close_immediately(mut socket: WebSocket, close: CloseFrame) {
    let _ = timeout(
        WRITER_CLOSE_TIMEOUT,
        socket.send(Message::Close(Some(close))),
    )
    .await;
}

#[allow(
    clippy::significant_drop_tightening,
    reason = "Lifetime guards must stay owned through the original protocol, unique writer and actual writer join"
)]
#[cfg(test)]
fn serve_connection(
    socket: WebSocket,
    state: Arc<ManagementState>,
    lease: ConnectionLease,
    shutdown: &mut watch::Receiver<bool>,
) -> ConnectionObservation {
    state
        .connections
        .clone()
        .adopt_existing(socket, state, lease, shutdown.clone())
}

async fn run_owned_connection(
    socket: WebSocket,
    state: Arc<ManagementState>,
    mut lease: ConnectionLease,
    shutdown: &mut ConnectionStop,
    context: ConnectionContext,
) {
    let (sink, stream) = socket.split();
    let mut reader = ProtocolReader {
        stream,
        peer_close_received: false,
    };
    let (outbound, outbound_rx) = Outbound::new();
    let _lifetime_owner = SocketLifetimeOwner(outbound.lifetime.clone());
    let (close_signal, close_rx) = watch::channel(None);
    let (write_authority, write_authority_rx) = watch::channel(None);
    let writer_lifetime = outbound.lifetime.clone();
    let writer_capacity = outbound.capacity_freed.clone();
    // Keep the queue alive until the owner supplies Close, including protocol unwind.
    // Otherwise recv(None) can end the writer before the supervisor observes that unwind.
    let writer_queue_guard = outbound.sender.clone();
    let Ok(writer) = context.start_writer(
        async move {
            let _writer_lifetime_owner = SocketLifetimeOwner(writer_lifetime);
            let result = write_loop(
                sink,
                outbound_rx,
                close_rx,
                write_authority_rx,
                writer_capacity,
            )
            .await;
            drop(writer_queue_guard);
            result
        },
        close_signal,
        outbound.lifetime.clone(),
    ) else {
        return;
    };
    #[cfg(test)]
    assert!(
        !state
            .test_hooks
            .protocol_failure
            .swap(false, std::sync::atomic::Ordering::SeqCst),
        "injected failure in the original protocol task"
    );
    let result = {
        let mut attempt = SocketAuthAttempt {
            reader: &mut reader,
            lease: &mut lease,
            outbound: &outbound,
        };
        run_protocol(&mut attempt, &state, shutdown, &write_authority).await
    };
    #[cfg(test)]
    state.test_hooks.last_close_code.store(
        usize::from(result.code),
        std::sync::atomic::Ordering::SeqCst,
    );
    outbound.lifetime.seal();
    let directive = if reader.peer_close_received {
        CloseDirective::ReplyToPeer
    } else {
        CloseDirective::Initiate(result)
    };
    context.close_writer(directive);
    #[cfg(test)]
    state
        .test_hooks
        .writer_closes
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let settlement = writer.wait().await;
    if settlement != WriterSettlement::Joined(Ok(())) {
        tracing::warn!(
            ?settlement,
            "management socket writer did not close cleanly"
        );
    }
    #[cfg(test)]
    state
        .test_hooks
        .writer_settlement
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .replace(settlement);
    drop(outbound);
}

#[allow(clippy::too_many_lines)]
async fn run_protocol(
    attempt: &mut SocketAuthAttempt<'_>,
    state: &Arc<ManagementState>,
    shutdown: &mut ConnectionStop,
    write_authority: &watch::Sender<Option<WriteAuthority>>,
) -> CloseFrame {
    let SocketAuthAttempt {
        reader,
        lease,
        outbound,
    } = attempt;
    if shutdown.stopped() {
        return close(CLOSE_GOING_AWAY, "node stopping");
    }

    let hello = tokio::select! {
        biased;
        () = shutdown.wait() => return close(CLOSE_GOING_AWAY, "node stopping"),
        received = tokio::time::timeout(HELLO_TIMEOUT, receive_frame(reader)) => {
            match received {
                Ok(Ok(ClientFrame::Hello(hello))) => hello,
                Ok(Ok(_)) => return close(CLOSE_FORMAT, "invalid frame"),
                Ok(Err(close)) => return close,
                Err(_) => return close(CLOSE_TIMEOUT, "authentication timeout"),
            }
        }
    };

    let Ok(connection_id) = random_id() else {
        return close(CLOSE_INTERNAL, "internal error");
    };
    let Ok(challenge) = state.pairing.issue_auth_challenge(
        connection_id,
        hello.client_id,
        hello.client_public_key,
        hello.client_nonce,
    ) else {
        return close(CLOSE_INTERNAL, "internal error");
    };
    let _challenge_cleanup = AuthChallengeCleanup {
        pairing: &state.pairing,
        connection_id,
    };
    let Ok(host_proof) = host_auth_proof(state, &hello, &challenge) else {
        return close(CLOSE_INTERNAL, "internal error");
    };
    let challenge_message = ChallengeMessage {
        v: 1,
        kind: "challenge",
        node_id: challenge.node_id,
        node_public_key: challenge.node_public_key,
        origin: challenge.origin.clone(),
        connection_id,
        challenge_id: challenge.challenge_id,
        nonce: challenge.nonce,
        iat: challenge.iat,
        exp: challenge.exp,
        host_proof,
    };
    if outbound.send_json(&challenge_message).is_err() {
        return close(CLOSE_RATE_LIMITED, "limit exceeded");
    }

    let proof = tokio::select! {
        biased;
        () = shutdown.wait() => return close(CLOSE_GOING_AWAY, "node stopping"),
        received = tokio::time::timeout(PROOF_TIMEOUT, receive_frame(reader)) => {
            match received {
                Ok(Ok(ClientFrame::Proof(proof))) => proof.proof,
                Ok(Ok(_)) => return close(CLOSE_FORMAT, "invalid frame"),
                Ok(Err(close)) => return close,
                Err(_) => return close(CLOSE_TIMEOUT, "authentication timeout"),
            }
        }
    };

    let revisions = state.trust.revision_receiver();
    let authenticated = match state
        .application_policy
        .authenticate(connection_id, &proof)
        .await
    {
        Ok(actor) => actor,
        Err(AccessPolicyError::Proof(error)) => return authentication_error_close(&error),
        Err(AccessPolicyError::InvalidActor) => {
            return close(CLOSE_AUTHENTICATION, "authentication failed");
        }
        Err(_) => {
            return close(CLOSE_INTERNAL, "storage unavailable");
        }
    };
    let Ok(client_id) = Id128::parse_base64url(authenticated.client_id()) else {
        return close(CLOSE_INTERNAL, "internal error");
    };
    #[cfg(test)]
    let resolve_barrier = {
        state
            .auth_resolve_barrier
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    };
    #[cfg(test)]
    if let Some(barrier) = resolve_barrier {
        barrier.pause().await;
    }
    match still_valid(state, &authenticated).await {
        Ok(true) => {}
        Ok(false) => return close(CLOSE_REVOKED, "revoked"),
        Err(()) => return close(CLOSE_INTERNAL, "storage unavailable"),
    }
    if !lease.authenticate(client_id) {
        return close(CLOSE_RATE_LIMITED, "limit exceeded");
    }
    let receipt = AuthenticatedSocketReceipt {
        policy: state.application_policy.clone(),
        actor: authenticated.clone(),
        connection_id,
        lifetime: outbound.lifetime.clone(),
    };
    let Ok(client) = state.socket_authority.accept(receipt) else {
        return close(CLOSE_INTERNAL, "socket unavailable");
    };
    #[cfg(windows)]
    let (upload, catalog_management) = match state.session_services.get() {
        None => (None, None),
        Some(services) => {
            let upload = match services.open_upload(&client) {
                Ok(owner) => owner,
                Err(crate::management_ws::input_upload::UploadError::NodeStopping) => {
                    return close(CLOSE_GOING_AWAY, "node stopping");
                }
                Err(_) => return close(CLOSE_INTERNAL, "upload unavailable"),
            };
            (Some(upload), Some(services.catalog_management()))
        }
    };
    let attachment = Arc::new(SocketAttachment {
        client,
        #[cfg(windows)]
        upload,
        #[cfg(windows)]
        catalog_management,
        streams: Mutex::new(HashMap::new()),
        #[cfg(windows)]
        document_pumps: Mutex::new(HashMap::new()),
        #[cfg(windows)]
        view_pumps: Mutex::new(HashMap::new()),
        #[cfg(windows)]
        view_close_receipts: Mutex::new(HashMap::new()),
    });
    #[cfg(test)]
    {
        let sender = state
            .socket_test_accept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(sender) = sender {
            let _ = sender.send((attachment.clone(), (*outbound).clone()));
        }
    }

    write_authority.send_replace(Some(WriteAuthority {
        attachment: attachment.clone(),
        state: Arc::clone(state),
        authenticated: authenticated.clone(),
    }));
    state
        .pairing
        .mark_consumed_for_client(&authenticated.client_id_string)
        .await;
    let authenticated_message = AuthenticatedMessage {
        v: 1,
        kind: "authenticated",
        client_id,
        connection_id,
    };
    if let Err(error) =
        enqueue_authenticated(state, &authenticated, outbound, &authenticated_message).await
    {
        return error;
    }

    run_authenticated(
        reader,
        state,
        authenticated,
        outbound,
        shutdown,
        revisions,
        attachment,
    )
    .await
}

async fn run_authenticated(
    reader: &mut ProtocolReader,
    state: &Arc<ManagementState>,
    authenticated: AuthenticatedClient,
    outbound: &Outbound,
    shutdown: &mut ConnectionStop,
    revisions: watch::Receiver<u64>,
    attachment: Arc<SocketAttachment>,
) -> CloseFrame {
    let close_frame = run_authenticated_loop(
        reader,
        state,
        authenticated,
        outbound,
        shutdown,
        revisions,
        &attachment,
    )
    .await;
    #[cfg(windows)]
    attachment.drain().await;
    close_frame
}

async fn run_authenticated_loop(
    reader: &mut ProtocolReader,
    state: &Arc<ManagementState>,
    authenticated: AuthenticatedClient,
    outbound: &Outbound,
    shutdown: &mut ConnectionStop,
    mut revisions: watch::Receiver<u64>,
    attachment: &Arc<SocketAttachment>,
) -> CloseFrame {
    let mut subscribed = false;
    let mut last_sent_revision = None;
    let mut command_window = CommandWindow::new();
    let mut pending_reads = session_commands::PendingReads::new();

    let close_frame = loop {
        tokio::select! {
            biased;
            () = shutdown.wait() => break close(CLOSE_GOING_AWAY, "node stopping"),
            completion = pending_reads.next(), if !pending_reads.is_empty() => {
                match completion {
                    Some(CommandOutcome::Result(result)) => {
                        let enqueued = tokio::select! {
                            biased;
                            () = shutdown.wait() => break close(CLOSE_GOING_AWAY, "node stopping"),
                            result = enqueue_authenticated(state, &authenticated, outbound, &result) => result,
                        };
                        if let Err(error) = enqueued {
                            break error;
                        }
                    }
                    Some(CommandOutcome::Close(error)) => break error,
                    None => {}
                }
            }
            changed = revisions.changed() => {
                if changed.is_err() {
                    break close(CLOSE_INTERNAL, "storage unavailable");
                }
                let revision = *revisions.borrow_and_update();
                match tokio::select! {
                    biased;
                    () = shutdown.wait() => break close(CLOSE_GOING_AWAY, "node stopping"),
                    result = lock_current_actor(state, &authenticated) => result,
                } {
                    Ok(_gate) => {
                        if subscribed && last_sent_revision != Some(revision) {
                            if outbound.send_json(&DevicesChangedMessage {
                                v: 1,
                                kind: "devices.changed",
                                revision,
                            }).is_err() {
                                break close(CLOSE_RATE_LIMITED, "limit exceeded");
                            }
                            drop(_gate);
                            last_sent_revision = Some(revision);
                        }
                    }
                    Err(error) => break error,
                }
            }
            received = receive_frame(reader) => {
                let frame = match received {
                    Ok(frame) => frame,
                    Err(close) => break close,
                };
                #[cfg(test)]
                if state.test_hooks.fresh_command_window.load(std::sync::atomic::Ordering::SeqCst) {
                    command_window = CommandWindow::new();
                }
                if let ClientFrame::Shared(bytes)=frame {
                    #[cfg(windows)]
                    if bytes.starts_with(b"ACPI") {
                        #[cfg(test)]
                        {
                            let barrier = {
                                let slot = &state.test_hooks.before_input_dispatch;
                                slot.lock()
                                    .unwrap_or_else(PoisonError::into_inner)
                                    .take()
                            };
                            if let Some(barrier) = barrier {
                                tokio::select! {
                                    biased;
                                    () = shutdown.wait() => break close(CLOSE_GOING_AWAY, "node stopping"),
                                    () = barrier.pause() => {}
                                }
                            }
                        }
                        if shutdown.stopped() {
                            break close(CLOSE_GOING_AWAY, "node stopping");
                        }
                        let Some(owner) = attachment.upload.as_ref() else {
                            break close(CLOSE_FORMAT, "upload unavailable");
                        };
                        let data = match input_upload::InputData::decode_client(&bytes) {
                            Ok(data) => data,
                            Err(_) => break close(CLOSE_FORMAT, "invalid input data"),
                        };
                        let accepted = tokio::select! {
                            biased;
                            () = shutdown.wait() => break close(CLOSE_GOING_AWAY, "node stopping"),
                            accepted = owner.accept_data_routed_typed(&attachment.client, data) => accepted,
                        };
                        let ack = match accepted {
                            Ok(ack) => ack,
                            Err(error) => break upload_routing_close(error),
                        };
                        #[cfg(test)]
                        {
                            let barrier = {
                                let slot = &state.test_hooks.input_accepted_before_ack;
                                slot.lock()
                                    .unwrap_or_else(PoisonError::into_inner)
                                    .take()
                            };
                            if let Some(barrier) = barrier {
                                tokio::select! {
                                    biased;
                                    () = shutdown.wait() => break close(CLOSE_GOING_AWAY, "node stopping"),
                                    () = barrier.pause() => {}
                                }
                            }
                        }
                        if shutdown.stopped() {
                            break close(CLOSE_GOING_AWAY, "node stopping");
                        }
                        if outbound.send_binary(ack.encode()).is_err() {
                            break close(CLOSE_RATE_LIMITED, "limit exceeded");
                        }
                        continue;
                    }
                    #[cfg(test)]
                    {
                        let barrier = {
                            let slot = &state.test_hooks.before_view_control_dispatch;
                            slot.lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .take()
                        };
                        if let Some(barrier) = barrier {
                            tokio::select! {
                                biased;
                                () = shutdown.wait() => break close(CLOSE_GOING_AWAY, "node stopping"),
                                () = barrier.pause() => {}
                            }
                        }
                    }
                    let control_result = tokio::select! {
                        biased;
                        () = shutdown.wait() => break close(CLOSE_GOING_AWAY, "node stopping"),
                        result = async {
                            if let Err(error) = attachment.control(&bytes, outbound).await {
                                return Err(binary_control_failure(state, &authenticated, error).await);
                            }
                            Ok(())
                        } => result,
                    };
                    if let Err(error) = control_result {
                        break error;
                    }
                    continue;
                }
                if !command_window.accept() {
                    break close(CLOSE_RATE_LIMITED, "limit exceeded");
                }
                match frame {
                    ClientFrame::Command(command) => {
                        if !matches!(command.action.as_str(), "node.info" | "device.list" | "device.revoke") {
                            if let Some(outcome) = session_commands::start(state, &authenticated, attachment, outbound, &command, &mut pending_reads) {
                                match outcome {
                                    CommandOutcome::Result(result) => {
                                        let enqueued = tokio::select! {
                                            biased;
                                            () = shutdown.wait() => break close(CLOSE_GOING_AWAY, "node stopping"),
                                            result = enqueue_authenticated(state, &authenticated, outbound, &result) => result,
                                        };
                                        if let Err(error) = enqueued {
                                            break error;
                                        }
                                    }
                                    CommandOutcome::Close(error) => break error,
                                }
                            }
                            continue;
                        }
                        match execute_command(
                            state,
                            &authenticated,
                            command.request_id,
                            &command.action,
                            command.payload,
                        )
                        .await
                        {
                            CommandOutcome::Result(result) => {
                                if let Err(error) =
                                    enqueue_authenticated(state, &authenticated, outbound, &result)
                                        .await
                                {
                                    break error;
                                }
                            }
                            CommandOutcome::Close(close) => break close,
                        }
                    }
                    ClientFrame::Subscribe(subscribe) => {
                        let subscribed_res = tokio::select! {
                            biased;
                            () = shutdown.wait() => break close(CLOSE_GOING_AWAY, "node stopping"),
                            result = subscribe_devices(state, &authenticated, outbound, subscribe) => result,
                        };
                        match subscribed_res {
                            Ok(Some(revision)) => {
                                subscribed = true;
                                last_sent_revision = Some(revision);
                            }
                            Ok(None) => {}
                            Err(error) => break error,
                        }
                    }
                    ClientFrame::Shared(_) | ClientFrame::Hello(_) | ClientFrame::Proof(_) => {
                        break close(CLOSE_FORMAT, "invalid frame");
                    }
                }
            }
        }
    };
    pending_reads.shutdown().await;
    close_frame
}

async fn binary_control_failure(
    state: &ManagementState,
    authenticated: &AuthenticatedClient,
    error: crate::domain::shared_view::ViewError,
) -> CloseFrame {
    if matches!(
        error,
        crate::domain::shared_view::ViewError::Unauthenticated
            | crate::domain::shared_view::ViewError::Revoked
    ) {
        return match still_valid(state, authenticated).await {
            Ok(false) => close(CLOSE_REVOKED, "revoked"),
            Ok(true) => close(CLOSE_AUTHENTICATION, "authentication failed"),
            Err(()) => close(CLOSE_INTERNAL, "storage unavailable"),
        };
    }
    close(CLOSE_FORMAT, "invalid view control")
}

async fn subscribe_devices(
    state: &ManagementState,
    authenticated: &AuthenticatedClient,
    outbound: &Outbound,
    subscribe: SubscribeFrame,
) -> Result<Option<u64>, CloseFrame> {
    let _gate = lock_current_actor(state, authenticated).await?;
    if subscribe.resource != "devices" {
        outbound
            .send_json(&management_error(subscribe.request_id, "invalid_request"))
            .map_err(|()| close(CLOSE_RATE_LIMITED, "limit exceeded"))?;
        return Ok(None);
    }
    let revision = state
        .trust
        .current_revision()
        .await
        .map_err(|_| close(CLOSE_INTERNAL, "storage unavailable"))?;
    #[cfg(test)]
    pause_boundary(&state.test_hooks.subscription_revision).await;
    outbound
        .send_json(&management_success(
            subscribe.request_id,
            &serde_json::json!({"resource":"devices"}),
        ))
        .map_err(|()| close(CLOSE_RATE_LIMITED, "limit exceeded"))?;
    outbound
        .send_json(&DevicesChangedMessage {
            v: 1,
            kind: "devices.changed",
            revision,
        })
        .map_err(|()| close(CLOSE_RATE_LIMITED, "limit exceeded"))?;
    // Do not mark the watch receiver read after this SQL snapshot: a later
    // commit must remain observable even if it occurs before these enqueues.
    Ok(Some(revision))
}

async fn execute_command(
    state: &Arc<ManagementState>,
    authenticated: &AuthenticatedClient,
    request_id: Id128,
    action: &str,
    payload: Value,
) -> CommandOutcome {
    if !state.control.is_ready() {
        return CommandOutcome::Result(management_error(request_id, "node_stopping"));
    }
    // Admission linearizes while holding the actor gate. Release it before taking a
    // revoke target's gate, including self-revocation and reciprocal revocations.
    #[cfg(test)]
    pause_boundary(&state.test_hooks.admission).await;
    if let Err(error) = admit_command(state, authenticated).await {
        return CommandOutcome::Close(error);
    }

    let result = match action {
        "node.info" => {
            if payload.as_object().is_some_and(Map::is_empty) {
                Ok(serde_json::json!({
                    "nodeId": state.node_id,
                    "publicKey": state.node_public_key,
                    "origin": state.origin.as_ref(),
                    "status": "ready"
                }))
            } else {
                Err(CommandResultError::InvalidRequest)
            }
        }
        "device.list" => {
            let Some(object) = payload.as_object() else {
                return CommandOutcome::Result(management_error(request_id, "invalid_request"));
            };
            if object.len() != 1 || !object.contains_key("cursor") {
                return CommandOutcome::Result(management_error(request_id, "invalid_request"));
            }
            let cursor = match object.get("cursor") {
                Some(Value::Null) => None,
                Some(Value::String(value)) => Some(value.as_str()),
                _ => {
                    return CommandOutcome::Result(management_error(request_id, "invalid_request"));
                }
            };
            state
                .trust
                .list_page(&authenticated.client_id_string, cursor)
                .await
                .map_err(CommandResultError::Trust)
                .and_then(|page| {
                    serde_json::to_value(page).map_err(|_| CommandResultError::Internal)
                })
        }
        "device.revoke" => {
            let Some(object) = payload.as_object() else {
                return CommandOutcome::Result(management_error(request_id, "invalid_request"));
            };
            if object.len() != 1 || !object.contains_key("clientId") {
                return CommandOutcome::Result(management_error(request_id, "invalid_request"));
            }
            let Some(Value::String(client_id)) = object.get("clientId") else {
                return CommandOutcome::Result(management_error(request_id, "invalid_request"));
            };
            if Id128::parse_base64url(client_id).is_err() {
                return CommandOutcome::Result(management_error(request_id, "invalid_request"));
            }
            let result = state
                .trust
                .revoke(
                    &authenticated.client_id_string,
                    &request_id.to_base64url(),
                    client_id,
                )
                .await
                .map_err(CommandResultError::Trust);
            if result.is_ok() && client_id == &authenticated.client_id_string {
                return CommandOutcome::Close(close(CLOSE_REVOKED, "revoked"));
            }
            result.and_then(|value| {
                serde_json::to_value(value).map_err(|_| CommandResultError::Internal)
            })
        }
        _ => Err(CommandResultError::InvalidRequest),
    };

    match still_valid(state, authenticated).await {
        Ok(true) => {}
        Ok(false) => return CommandOutcome::Close(close(CLOSE_REVOKED, "revoked")),
        Err(()) => {
            return CommandOutcome::Close(close(CLOSE_INTERNAL, "storage unavailable"));
        }
    }
    match result {
        Ok(value) => CommandOutcome::Result(management_success(request_id, &value)),
        Err(CommandResultError::InvalidRequest) => {
            CommandOutcome::Result(management_error(request_id, "invalid_request"))
        }
        Err(CommandResultError::Trust(error)) => {
            CommandOutcome::Result(management_trust_error(request_id, &error))
        }
        Err(CommandResultError::Internal) => {
            CommandOutcome::Close(close(CLOSE_INTERNAL, "internal error"))
        }
    }
}

enum CommandResultError {
    InvalidRequest,
    Trust(TrustError),
    Internal,
}

async fn lock_current_actor(
    state: &ManagementState,
    authenticated: &AuthenticatedClient,
) -> Result<OwnedMutexGuard<()>, CloseFrame> {
    let gate = state
        .trust
        .client_gate(authenticated.client_id())
        .lock_owned()
        .await;
    match still_valid(state, authenticated).await {
        Ok(true) => Ok(gate),
        Ok(false) => Err(close(CLOSE_REVOKED, "revoked")),
        Err(()) => Err(close(CLOSE_INTERNAL, "storage unavailable")),
    }
}

async fn admit_command(
    state: &ManagementState,
    authenticated: &AuthenticatedClient,
) -> Result<(), CloseFrame> {
    let _gate = lock_current_actor(state, authenticated).await?;
    #[cfg(test)]
    state
        .test_hooks
        .admitted
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

async fn enqueue_authenticated(
    state: &ManagementState,
    authenticated: &AuthenticatedClient,
    outbound: &Outbound,
    value: &(impl Serialize + Sync),
) -> Result<(), CloseFrame> {
    #[cfg(test)]
    pause_boundary(&state.test_hooks.enqueue).await;
    let _gate = lock_current_actor(state, authenticated).await?;
    if outbound.send_json(value).is_err() {
        #[cfg(test)]
        {
            use std::sync::atomic::Ordering;
            state.test_hooks.overflow_bytes.store(
                MAX_OUTBOUND_BYTES - outbound.bytes.available_permits(),
                Ordering::SeqCst,
            );
            state.test_hooks.overflow_frames.store(
                MAX_OUTBOUND_FRAMES - outbound.sender.capacity(),
                Ordering::SeqCst,
            );
            state.test_hooks.rejected_bytes.store(
                serde_json::to_vec(value).map_or(0, |bytes| bytes.len()),
                Ordering::SeqCst,
            );
        }
        return Err(close(CLOSE_RATE_LIMITED, "limit exceeded"));
    }
    #[cfg(test)]
    state
        .test_hooks
        .enqueued
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

enum CommandOutcome {
    Result(Value),
    Close(CloseFrame),
}

struct AuthChallengeCleanup<'a> {
    pairing: &'a PairingService,
    connection_id: Id128,
}

impl Drop for AuthChallengeCleanup<'_> {
    fn drop(&mut self) {
        self.pairing.discard_auth_challenge(self.connection_id);
    }
}

async fn still_valid(
    state: &ManagementState,
    authenticated: &AuthenticatedClient,
) -> Result<bool, ()> {
    match state
        .application_policy
        .require_current_actor(authenticated)
        .await
    {
        Ok(()) => Ok(true),
        Err(AccessPolicyError::InvalidActor) => Ok(false),
        Err(_) => Err(()),
    }
}

fn host_auth_proof(
    state: &ManagementState,
    hello: &HelloFrame,
    challenge: &IssuedChallenge,
) -> Result<String, ()> {
    let connection_id = challenge.connection_id.ok_or(())?;
    let binding = auth_binding(challenge, connection_id, hello);
    let payload = serde_json::to_vec(&binding).map_err(|_| ())?;
    state
        .identity
        .sign_jws(JwsType::AuthHost, &payload)
        .map_err(|_| ())
}

fn auth_binding(
    challenge: &IssuedChallenge,
    connection_id: Id128,
    hello: &HelloFrame,
) -> AuthProof {
    AuthProof {
        v: 1,
        node_id: challenge.node_id,
        node_public_key: challenge.node_public_key,
        origin: challenge.origin.clone(),
        connection_id,
        challenge_id: challenge.challenge_id,
        nonce: challenge.nonce,
        iat: challenge.iat,
        exp: challenge.exp,
        client_id: hello.client_id,
        client_public_key: hello.client_public_key,
        client_nonce: hello.client_nonce,
    }
}

const fn authentication_error_close(error: &PairingError) -> CloseFrame {
    match error {
        PairingError::Challenge(ChallengeError::Expired) => {
            close(CLOSE_TIMEOUT, "authentication timeout")
        }
        PairingError::Challenge(
            ChallengeError::StoreUnavailable | ChallengeError::ClockUnavailable(_),
        )
        | PairingError::StorageUnavailable
        | PairingError::ClockUnavailable => close(CLOSE_INTERNAL, "storage unavailable"),
        PairingError::Challenge(_)
        | PairingError::Jws(_)
        | PairingError::PairingInvalid
        | PairingError::RequestConflict
        | PairingError::CapacityExhausted
        | PairingError::RandomUnavailable
        | PairingError::AlreadyApproved
        | PairingError::Trust(_) => close(CLOSE_AUTHENTICATION, "authentication failed"),
    }
}

fn management_trust_error(request_id: Id128, error: &TrustError) -> Value {
    match error {
        TrustError::InvalidRequest => management_error(request_id, "invalid_request"),
        TrustError::NotFound => management_error(request_id, "not_found"),
        TrustError::RequestConflict => management_error(request_id, "request_conflict"),
        TrustError::StaleCursor => management_error(request_id, "stale_cursor"),
        TrustError::CapacityExhausted | TrustError::StorageUnavailable => {
            management_error(request_id, "storage_unavailable")
        }
        TrustError::PublicKeyAlreadyRegistered
        | TrustError::ClientIdCollision
        | TrustError::Database(_)
        | TrustError::Storage(_) => management_error(request_id, "storage_unavailable"),
    }
}

fn management_success(request_id: Id128, result: &Value) -> Value {
    serde_json::json!({
        "v": 1,
        "type": "result",
        "requestId": request_id,
        "ok": true,
        "result": result,
        "error": null
    })
}

fn management_error(request_id: Id128, code: &'static str) -> Value {
    serde_json::json!({
        "v": 1,
        "type": "result",
        "requestId": request_id,
        "ok": false,
        "result": null,
        "error": {"code": code}
    })
}

#[derive(Debug)]
enum ClientFrame {
    Shared(bytes::Bytes),
    Hello(HelloFrame),
    Proof(ProofFrame),
    Command(CommandFrame),
    Subscribe(SubscribeFrame),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HelloFrame {
    #[serde(rename = "v")]
    _v: u8,
    #[serde(rename = "type")]
    _kind: String,
    client_id: Option<Id128>,
    client_public_key: PublicKeySec1,
    client_nonce: Nonce256,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProofFrame {
    #[serde(rename = "v")]
    _v: u8,
    #[serde(rename = "type")]
    _kind: String,
    proof: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CommandFrame {
    #[serde(rename = "v")]
    _v: u8,
    #[serde(rename = "type")]
    _kind: String,
    request_id: Id128,
    action: String,
    payload: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SubscribeFrame {
    #[serde(rename = "v")]
    _v: u8,
    #[serde(rename = "type")]
    _kind: String,
    request_id: Id128,
    resource: String,
}

async fn receive_frame(reader: &mut ProtocolReader) -> Result<ClientFrame, CloseFrame> {
    loop {
        let message = reader
            .stream
            .next()
            .await
            .ok_or_else(|| close(CLOSE_GOING_AWAY, "peer closed"))?
            .map_err(transport_read_close)?;
        match message {
            Message::Text(text) => {
                if text.as_str().len() > MAX_MANAGEMENT_FRAME_BYTES {
                    return Err(close(CLOSE_RATE_LIMITED, "limit exceeded"));
                }
                return parse_frame(text.as_str());
            }
            Message::Binary(bytes) => {
                if bytes.len() > MAX_MANAGEMENT_FRAME_BYTES {
                    return Err(close(CLOSE_RATE_LIMITED, "limit exceeded"));
                }
                return Ok(ClientFrame::Shared(bytes));
            }
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Close(_) => {
                reader.peer_close_received = true;
                return Err(close(1000, "normal closure"));
            }
        }
    }
}

fn transport_read_close(error: axum::Error) -> CloseFrame {
    let inner = error.into_inner();
    match inner.downcast_ref::<tungstenite::Error>() {
        Some(tungstenite::Error::Capacity(_)) => close(CLOSE_RATE_LIMITED, "limit exceeded"),
        Some(tungstenite::Error::Protocol(_) | tungstenite::Error::Utf8(_)) => {
            close(CLOSE_FORMAT, "invalid frame")
        }
        _ => close(CLOSE_GOING_AWAY, "peer closed"),
    }
}

fn parse_frame(input: &str) -> Result<ClientFrame, CloseFrame> {
    let object = parse_object(input.as_bytes(), MAX_PROTOCOL_JSON_DEPTH)
        .map_err(|_| close(CLOSE_FORMAT, "invalid frame"))?;
    if object.get("v").and_then(Value::as_u64) != Some(1) {
        return Err(close(CLOSE_FORMAT, "invalid frame"));
    }
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| close(CLOSE_FORMAT, "invalid frame"))?
        .to_owned();
    let invalid = || close(CLOSE_FORMAT, "invalid frame");
    let value = Value::Object(object);
    match kind.as_str() {
        "hello" => serde_json::from_value::<HelloFrame>(value)
            .map(ClientFrame::Hello)
            .map_err(|_| invalid()),
        "proof" => serde_json::from_value::<ProofFrame>(value)
            .map(ClientFrame::Proof)
            .map_err(|_| invalid()),
        "command" => serde_json::from_value::<CommandFrame>(value)
            .map(ClientFrame::Command)
            .map_err(|_| invalid()),
        "subscribe" => serde_json::from_value::<SubscribeFrame>(value)
            .map(ClientFrame::Subscribe)
            .map_err(|_| invalid()),
        _ => Err(invalid()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChallengeMessage {
    v: u8,
    #[serde(rename = "type")]
    kind: &'static str,
    node_id: NodeId,
    node_public_key: PublicKeySec1,
    origin: String,
    connection_id: Id128,
    challenge_id: Id128,
    nonce: Nonce256,
    iat: crate::claims::UnixSeconds,
    exp: crate::claims::UnixSeconds,
    host_proof: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthenticatedMessage {
    v: u8,
    #[serde(rename = "type")]
    kind: &'static str,
    client_id: Id128,
    connection_id: Id128,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DevicesChangedMessage {
    v: u8,
    #[serde(rename = "type")]
    kind: &'static str,
    revision: u64,
}

struct CommandWindow {
    started: Instant,
    count: u16,
}

impl CommandWindow {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            count: 0,
        }
    }

    fn accept(&mut self) -> bool {
        self.accept_at(Instant::now())
    }

    fn accept_at(&mut self, now: Instant) -> bool {
        if now.saturating_duration_since(self.started) >= COMMAND_WINDOW {
            self.started = now;
            self.count = 0;
        }
        self.count = self.count.saturating_add(1);
        self.count <= MAX_COMMANDS_PER_WINDOW
    }
}

#[derive(Clone)]
struct Outbound {
    lifetime: Arc<SocketLifetime>,
    sender: mpsc::Sender<QueuedFrame>,
    bytes: Arc<Semaphore>,
    capacity_freed: Arc<Notify>,
}

#[derive(Clone)]
struct WriteAuthority {
    attachment: Arc<SocketAttachment>,
    state: Arc<ManagementState>,
    authenticated: AuthenticatedClient,
}

enum QueuedMessage {
    Text(String),
    Binary(Vec<u8>),
    Shared(QueuedSocketChunk),
}
enum SocketEnqueueError {
    Rejected(PreparedSocketChunk),
    StateLost,
}
#[cfg(windows)]
enum BinarySendError {
    Full,
    Closed,
}
struct QueuedFrame {
    message: QueuedMessage,
    _permit: OwnedSemaphorePermit,
}

impl Outbound {
    fn send_shared(
        &self,
        chunk: PreparedSocketChunk,
        client: &SocketViewClient,
    ) -> Result<(), SocketEnqueueError> {
        if !chunk.belongs_to(client) || !Arc::ptr_eq(&self.lifetime, &client.context.lifetime) {
            return Err(SocketEnqueueError::Rejected(chunk));
        }
        let Ok(bytes) = u32::try_from(chunk.byte_len()) else {
            return Err(SocketEnqueueError::Rejected(chunk));
        };
        let Ok(permit) = self.bytes.clone().try_acquire_many_owned(bytes) else {
            return Err(SocketEnqueueError::Rejected(chunk));
        };
        let chunk = chunk.queue().map_err(SocketEnqueueError::Rejected)?;
        match self.sender.try_send(QueuedFrame {
            message: QueuedMessage::Shared(chunk),
            _permit: permit,
        }) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(frame)) => match frame.message {
                QueuedMessage::Shared(chunk) => Err(SocketEnqueueError::Rejected(chunk.rejected())),
                QueuedMessage::Text(_) | QueuedMessage::Binary(_) => {
                    Err(SocketEnqueueError::StateLost)
                }
            },
            Err(mpsc::error::TrySendError::Closed(_)) => Err(SocketEnqueueError::StateLost),
        }
    }
    fn send_document(
        &self,
        transfer: &crate::application::document_transfer::DocumentTransfer,
        client: &SocketViewClient,
    ) -> Result<
        crate::application::document_transfer::DocumentEnqueue,
        crate::application::document_source::DocumentError,
    > {
        use crate::application::document_transfer::{
            DocumentEnqueueFailure, PreparedDocumentChunk,
        };
        transfer.enqueue_with(client, |chunk| {
            match self.send_shared(chunk.into_socket(), client) {
                Ok(()) => Ok(()),
                Err(SocketEnqueueError::Rejected(chunk)) => {
                    match PreparedDocumentChunk::rejected(chunk) {
                        Ok(chunk) => Err(DocumentEnqueueFailure::Rejected(chunk)),
                        Err(_) => Err(DocumentEnqueueFailure::StateLost),
                    }
                }
                Err(SocketEnqueueError::StateLost) => Err(DocumentEnqueueFailure::StateLost),
            }
        })
    }
    fn new() -> (Self, mpsc::Receiver<QueuedFrame>) {
        let (sender, receiver) = mpsc::channel(MAX_OUTBOUND_FRAMES);
        (
            Self {
                lifetime: SocketLifetime::new(),
                sender,
                bytes: Arc::new(Semaphore::new(MAX_OUTBOUND_BYTES)),
                capacity_freed: Arc::new(Notify::new()),
            },
            receiver,
        )
    }

    fn send_json(&self, value: &impl Serialize) -> Result<(), ()> {
        let json = serde_json::to_string(value).map_err(|_| ())?;
        if json.len() > MAX_MANAGEMENT_FRAME_BYTES {
            return Err(());
        }
        let bytes = u32::try_from(json.len()).map_err(|_| ())?;
        let permit = Arc::clone(&self.bytes)
            .try_acquire_many_owned(bytes)
            .map_err(|_| ())?;
        self.sender
            .try_send(QueuedFrame {
                message: QueuedMessage::Text(json),
                _permit: permit,
            })
            .map_err(|_| ())
    }
    fn send_binary(&self, bytes: [u8; input_upload::HEADER_BYTES]) -> Result<(), ()> {
        let permit = Arc::clone(&self.bytes)
            .try_acquire_many_owned(input_upload::HEADER_BYTES as u32)
            .map_err(|_| ())?;
        self.sender
            .try_send(QueuedFrame {
                message: QueuedMessage::Binary(bytes.to_vec()),
                _permit: permit,
            })
            .map_err(|_| ())
    }
    #[cfg(windows)]
    fn send_binary_frame(&self, bytes: Vec<u8>) -> Result<(), BinarySendError> {
        let limit = crate::application::shared_views::wire::HEADER_BYTES
            + crate::application::shared_views::wire::MAX_PAYLOAD_BYTES;
        if bytes.len() > limit || self.sender.is_closed() {
            return Err(BinarySendError::Closed);
        }
        let Ok(len) = u32::try_from(bytes.len()) else {
            return Err(BinarySendError::Closed);
        };
        let Ok(permit) = Arc::clone(&self.bytes).try_acquire_many_owned(len) else {
            return Err(BinarySendError::Full);
        };
        match self.sender.try_send(QueuedFrame {
            message: QueuedMessage::Binary(bytes),
            _permit: permit,
        }) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(BinarySendError::Full),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(BinarySendError::Closed),
        }
    }
}

struct ProtocolReader {
    stream: SplitStream<WebSocket>,
    peer_close_received: bool,
}

#[derive(Clone)]
enum CloseDirective {
    Initiate(CloseFrame),
    ReplyToPeer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriterCloseError {
    TimedOut,
    Transport,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriterSettlement {
    Joined(Result<(), WriterCloseError>),
    Aborted,
    TaskFailed,
}

async fn finish_writer_close(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    directive: CloseDirective,
) -> Result<(), WriterCloseError> {
    timeout(WRITER_CLOSE_TIMEOUT, async {
        match directive {
            CloseDirective::Initiate(frame) => sink.send(Message::Close(Some(frame))).await,
            // The locked transport queues its reply when the reader receives Close.
            // Only the original writer drives that queued reply; another send is invalid.
            CloseDirective::ReplyToPeer => sink.flush().await,
        }
    })
    .await
    .map_err(|_| WriterCloseError::TimedOut)?
    .map_err(|_| WriterCloseError::Transport)
}

struct NotifyCapacityOnDrop(Arc<Notify>);
impl Drop for NotifyCapacityOnDrop {
    fn drop(&mut self) {
        self.0.notify_waiters();
    }
}

async fn write_loop(
    mut sink: futures_util::stream::SplitSink<WebSocket, Message>,
    mut outbound: mpsc::Receiver<QueuedFrame>,
    mut close_rx: watch::Receiver<Option<CloseDirective>>,
    authority: watch::Receiver<Option<WriteAuthority>>,
    capacity_freed: Arc<Notify>,
) -> Result<(), WriterCloseError> {
    let _notify_on_exit = NotifyCapacityOnDrop(Arc::clone(&capacity_freed));
    loop {
        tokio::select! {
            biased;
            changed = close_rx.changed() => {
                let frame = if changed.is_ok() {
                    close_rx.borrow_and_update().clone()
                } else {
                    None
                };
                if let Some(frame) = frame {
                    return finish_writer_close(&mut sink, frame).await;
                }
                if changed.is_err() {
                    return Err(WriterCloseError::Stopped);
                }
            }
            queued = outbound.recv() => {
                let Some(queued) = queued else {
                    return Err(WriterCloseError::Stopped);
                };
                let QueuedFrame {
                    message,
                    _permit: permit,
                } = queued;
                let write_authority = authority.borrow().clone();
                #[cfg(test)]
                if let Some(authority) = &write_authority {
                    let barrier = {
                        authority
                            .state
                            .write_barrier
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .take()
                    };
                    if let Some(barrier) = barrier {
                        barrier.pause().await;
                    }
                }
                let message = match message {
                    QueuedMessage::Shared(chunk) => {
                        let Some(authority) = &write_authority else {
                            return Err(WriterCloseError::Stopped);
                        };
                        if !chunk.belongs_to(&authority.attachment.client) {
                            return Err(WriterCloseError::Stopped);
                        }
                        match chunk.send(&mut sink, WRITER_SEND_TIMEOUT).await {
                            Ok(()) => {}
                            Err(
                                crate::domain::shared_view::ViewError::StaleCursor
                                | crate::domain::shared_view::ViewError::StreamSealed
                                | crate::domain::shared_view::ViewError::SourceReleased,
                            ) => {}
                            Err(_) => return Err(WriterCloseError::Stopped),
                        }
                        drop(permit);
                        capacity_freed.notify_waiters();
                        continue;
                    }
                    QueuedMessage::Text(json) => Message::Text(Utf8Bytes::from(json)),
                    QueuedMessage::Binary(bytes) => Message::Binary(bytes::Bytes::from(bytes)),
                };
                let gate = if let Some(authority) = &write_authority {
                    let gate = authority
                        .state
                        .trust
                        .client_gate(&authority.authenticated.client_id_string);
                    let gate = gate.lock_owned().await;
                    match still_valid(&authority.state, &authority.authenticated).await {
                        Ok(true) => {}
                        Ok(false) => {
                            return finish_writer_close(&mut sink, CloseDirective::Initiate(close(CLOSE_REVOKED, "revoked"))).await;
                        }
                        Err(()) => {
                            return finish_writer_close(&mut sink, CloseDirective::Initiate(close(CLOSE_INTERNAL, "storage unavailable"))).await;
                        }
                    }
                    Some(gate)
                } else {
                    None
                };
                #[cfg(test)]
                if let Some(authority) = &write_authority {
                    pause_boundary(&authority.state.test_hooks.write_locked).await;
                }
                match timeout(WRITER_SEND_TIMEOUT, sink.send(message)).await {
                    Ok(Ok(())) => {
                        #[cfg(test)]
                        if let Some(authority) = &write_authority {
                            authority.state.test_hooks.protected_writes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                    Ok(Err(_)) | Err(_) => return Err(WriterCloseError::Transport),
                }
                drop(gate);
                drop(permit);
                capacity_freed.notify_waiters();
            }
        }
    }
}

const fn close(code: u16, reason: &'static str) -> CloseFrame {
    CloseFrame {
        code,
        reason: Utf8Bytes::from_static(reason),
    }
}
#[cfg(windows)]
const fn upload_routing_close(error: UploadRoutingError) -> CloseFrame {
    match error {
        UploadRoutingError::Business(_) => close(CLOSE_FORMAT, "invalid input data"),
        UploadRoutingError::Revoked => close(CLOSE_REVOKED, "revoked"),
        UploadRoutingError::StorageUnavailable => close(CLOSE_INTERNAL, "storage unavailable"),
    }
}

fn random_id() -> Result<Id128, ()> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| ())?;
    Ok(Id128::new(bytes))
}

fn seconds_until(deadline: Instant, now: Instant) -> u64 {
    let remaining = deadline.saturating_duration_since(now).as_millis();
    let rounded_up = remaining.saturating_add(999) / 1_000;
    u64::try_from(rounded_up).unwrap_or(u64::MAX).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    #[test]
    fn handshake_limiter_enforces_source_and_global_windows() {
        let limiter = HandshakeLimiter::default();
        let now = Instant::now();
        let first_source = source(40_001);
        for _ in 0..HANDSHAKE_LIMIT_PER_SOURCE {
            limiter
                .check_at(first_source, now)
                .expect("source request within limit");
        }
        let rejection = limiter
            .check_at(first_source, now)
            .expect_err("source request over limit");
        assert!((59..=60).contains(&rejection.retry_after_seconds));
        assert!(
            limiter.check_at(source(40_002), now).is_err(),
            "ephemeral source port does not create a new source budget"
        );
        assert!(
            limiter
                .check_at(SocketAddr::from(([127, 0, 0, 2], 40_002)), now)
                .is_ok()
        );

        let global = HandshakeLimiter::default();
        let mut accepted = 0;
        for index in 0..HANDSHAKE_LIMIT_GLOBAL {
            let source =
                SocketAddr::from(([127, 1, 0, u8::try_from(index).unwrap_or(u8::MAX)], 41_000));
            if global.check_at(source, now).is_ok() {
                accepted += 1;
            }
        }
        assert_eq!(accepted, HANDSHAKE_LIMIT_GLOBAL);
        assert!(global.check_at(source(42_000), now).is_err());
    }

    #[test]
    fn handshake_buckets_reject_eviction_and_expire_at_five_idle_minutes() {
        let limiter = HandshakeLimiter::default();
        let start = Instant::now();
        for source in 0_u16..1024 {
            let [high, low] = source.to_be_bytes();
            let _ = limiter.check_at(SocketAddr::from(([10, 0, high, low], 51000)), start);
        }
        assert_eq!(
            limiter.state.lock().expect("state").sources.len(),
            MAX_SOURCE_BUCKETS
        );
        let new = SocketAddr::from(([10, 1, 0, 0], 51000));
        let minute = start + Duration::from_secs(60);
        limiter
            .check_at(new, minute)
            .expect_err("1025th bucket refused");
        {
            let state = limiter.state.lock().expect("state");
            assert_eq!(
                state.global_count, 1,
                "global quota reset, not hiding capacity"
            );
            assert!(!state.sources.contains_key(&new.ip()));
            drop(state);
        }
        let old = SocketAddr::from(([10, 0, 0, 0], 51000));
        for _ in 0..HANDSHAKE_LIMIT_PER_SOURCE {
            limiter.check_at(old, minute).expect("retained source");
        }
        limiter
            .check_at(old, minute)
            .expect_err("source quota enforced");
        limiter
            .check_at(new, minute)
            .expect_err("new source still refused");
        limiter
            .check_at(old, minute)
            .expect_err("old source not reset by new bucket");
        limiter
            .check_at(new, start + Duration::from_secs(299))
            .expect_err("not idle five minutes yet");
        assert_eq!(
            limiter.state.lock().expect("state").sources.len(),
            MAX_SOURCE_BUCKETS
        );
        limiter
            .check_at(new, start + Duration::from_secs(300))
            .expect("idle sources expire at deadline");
        assert_eq!(limiter.state.lock().expect("state").sources.len(), 2);
    }

    #[test]
    fn handshake_and_command_rejections_do_not_slide_fixed_windows() {
        let limiter = HandshakeLimiter::default();
        let start = Instant::now();
        for _ in 0..HANDSHAKE_LIMIT_PER_SOURCE {
            limiter.check_at(source(51000), start).expect("quota");
        }
        assert_eq!(
            limiter
                .check_at(source(51000), start)
                .expect_err("over")
                .retry_after_seconds,
            60
        );
        assert_eq!(
            limiter
                .check_at(source(51000), start + Duration::from_millis(59999))
                .expect_err("before exact deadline")
                .retry_after_seconds,
            1
        );
        limiter
            .check_at(source(51000), start + Duration::from_secs(60))
            .expect("exact deadline");
        let mut commands = CommandWindow {
            started: start,
            count: 0,
        };
        for _ in 0..MAX_COMMANDS_PER_WINDOW {
            assert!(commands.accept_at(start));
        }
        assert!(!commands.accept_at(start));
        assert!(!commands.accept_at(start + Duration::from_millis(59999)));
        assert!(commands.accept_at(start + Duration::from_secs(60)));
        assert_eq!(commands.count, 1);
    }

    #[test]
    fn outbound_byte_and_frame_limits_are_independent_and_release_permits() {
        let (outbound, mut receiver) = Outbound::new();
        let full_frame = "a".repeat(MAX_MANAGEMENT_FRAME_BYTES - 2);
        for _ in 0..(MAX_OUTBOUND_BYTES / MAX_MANAGEMENT_FRAME_BYTES) {
            outbound.send_json(&full_frame).expect("exact byte budget");
        }
        assert_eq!(receiver.len(), 32, "byte limit before frame limit");
        assert_eq!(outbound.bytes.available_permits(), 0);
        assert!(outbound.send_json(&"x").is_err());
        drop(receiver.try_recv().expect("one frame"));
        assert_eq!(
            outbound.bytes.available_permits(),
            MAX_MANAGEMENT_FRAME_BYTES
        );
        outbound
            .send_json(&"x")
            .expect("released byte permit reusable");
        drop(receiver);
        assert_eq!(outbound.bytes.available_permits(), MAX_OUTBOUND_BYTES);

        let (outbound, mut receiver) = Outbound::new();
        for _ in 0..MAX_OUTBOUND_FRAMES {
            outbound.send_json(&"x").expect("frame budget");
        }
        let remaining = outbound.bytes.available_permits();
        assert!(remaining > 0, "frame limit before byte limit");
        assert!(outbound.send_json(&"x").is_err());
        assert_eq!(
            outbound.bytes.available_permits(),
            remaining,
            "failed enqueue releases its permit"
        );
        drop(receiver.try_recv().expect("one frame"));
        outbound
            .send_json(&"x")
            .expect("released frame slot reusable");
        drop(receiver);
        assert_eq!(outbound.bytes.available_permits(), MAX_OUTBOUND_BYTES);
    }

    #[test]
    fn connection_registry_bounds_total_unauthenticated_and_per_client_connections() {
        let unauthenticated = ConnectionRegistry::new_shared();
        let mut leases = Vec::new();
        for _ in 0..MAX_UNAUTHENTICATED_CONNECTIONS {
            leases.push(
                unauthenticated
                    .acquire()
                    .expect("unauthenticated connection within limit"),
            );
        }
        assert_eq!(unauthenticated.acquire().err(), Some(AcquireError::Limit));
        drop(leases);

        let per_client = ConnectionRegistry::new_shared();
        let client = Id128::new([31; 16]);
        let mut client_leases = Vec::new();
        for _ in 0..MAX_CONNECTIONS_PER_CLIENT {
            let mut lease = per_client.acquire().expect("connection within total limit");
            assert!(lease.authenticate(client));
            client_leases.push(lease);
        }
        let mut over_limit = per_client.acquire().expect("unauthenticated slot");
        assert!(!over_limit.authenticate(client));
        assert_eq!(client_leases.len(), MAX_CONNECTIONS_PER_CLIENT);

        let total = ConnectionRegistry::new_shared();
        let mut total_leases = Vec::new();
        for client_index in 0..(MAX_MANAGEMENT_CONNECTIONS / MAX_CONNECTIONS_PER_CLIENT) {
            let client = Id128::new([u8::try_from(client_index).unwrap_or(0); 16]);
            for _ in 0..MAX_CONNECTIONS_PER_CLIENT {
                let mut lease = total.acquire().expect("connection within total limit");
                assert!(lease.authenticate(client));
                total_leases.push(lease);
            }
        }
        assert_eq!(total.acquire().err(), Some(AcquireError::Limit));

        total.begin_shutdown();
        drop(total_leases);
        assert_eq!(total.acquire().err(), Some(AcquireError::Closing));
    }

    #[test]
    fn command_window_and_outbound_queue_are_bounded() {
        let mut commands = CommandWindow::new();
        for _ in 0..MAX_COMMANDS_PER_WINDOW {
            assert!(commands.accept());
        }
        assert!(!commands.accept());

        let (outbound, receiver) = Outbound::new();
        for _ in 0..MAX_OUTBOUND_FRAMES {
            outbound
                .send_json(&serde_json::json!({"v": 1}))
                .expect("outbound queue within limit");
        }
        assert!(outbound.send_json(&serde_json::json!({"v": 1})).is_err());
        drop(receiver);
    }

    #[test]
    fn management_parser_rejects_invalid_kind_version_and_depth() {
        let cases = [
            r#"{"v":2,"type":"hello"}"#,
            r#"{"v":1,"type":"unknown"}"#,
            r#"{"v":1,"type":"command","requestId":"AAAAAAAAAAAAAAAAAAAAAA","action":"node.info","payload":{"a":{"b":{"c":{"d":{"e":{"f":{"g":1}}}}}}}}}"#,
        ];
        for input in cases {
            assert!(parse_frame(input).is_err());
        }
    }
}

// Only this module can construct the receipt after the original proof/current/lease sequence.
pub(crate) struct AuthenticatedSocketReceipt {
    policy: Arc<ApplicationPolicy>,
    actor: AuthenticatedClient,
    connection_id: Id128,
    lifetime: Arc<SocketLifetime>,
}
impl AuthenticatedSocketReceipt {
    pub(crate) const fn policy(&self) -> &Arc<ApplicationPolicy> {
        &self.policy
    }
    pub(crate) const fn actor(&self) -> &AuthenticatedClient {
        &self.actor
    }
    pub(crate) const fn connection_id(&self) -> Id128 {
        self.connection_id
    }
    pub(crate) const fn lifetime(&self) -> &Arc<SocketLifetime> {
        &self.lifetime
    }
}
struct SocketAuthAttempt<'a> {
    reader: &'a mut ProtocolReader,
    lease: &'a mut ConnectionLease,
    outbound: &'a Outbound,
}
struct SocketLifetimeOwner(Arc<SocketLifetime>);
impl Drop for SocketLifetimeOwner {
    fn drop(&mut self) {
        self.0.seal();
    }
}
#[cfg(windows)]
type ViewPumpDone = (
    NativeSocketViewRead,
    Option<PreparedSocketChunk>,
    Result<(), crate::domain::shared_view::ViewError>,
);
#[cfg(windows)]
type ViewCloseOutcome = Result<
    crate::application::shared_views::reader::ReadCloseReceipt,
    crate::domain::shared_view::ViewError,
>;
#[cfg(windows)]
enum ViewPumpState {
    Active {
        stop: watch::Sender<bool>,
        join: tokio::task::JoinHandle<ViewPumpDone>,
    },
    Closing(watch::Receiver<Option<ViewCloseOutcome>>),
}
#[cfg(windows)]
async fn emit_stream_rebound_and_park(
    read: &NativeSocketViewRead,
    stream: &SocketViewStream,
    client: &SocketViewClient,
    outbound: &Outbound,
    stop: &mut watch::Receiver<bool>,
) -> Result<(), crate::domain::shared_view::ViewError> {
    use crate::application::shared_views::{
        reader::OriginalReadProgress,
        wire::{RecordKind, TransferHeader},
    };
    use crate::domain::shared_view::ViewError;
    let completed = loop {
        if *stop.borrow_and_update() {
            return Ok(());
        }
        tokio::select! {
            biased;
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow_and_update() {
                    return Ok(());
                }
            }
            () = stream.wait_terminally_sealed() => return Ok(()),
            result = read.wait_original_progress(client) => break result,
        }
    };
    match completed {
        Ok(OriginalReadProgress::Complete { .. }) => {}
        Ok(OriginalReadProgress::Failed(error)) => return Err(error),
        Ok(_) => return Err(ViewError::StaleCursor),
        Err(ViewError::StreamSealed) => return Ok(()),
        Err(error) => return Err(error),
    }
    let packet_seq = loop {
        let ready = stream.ready_notified();
        tokio::pin!(ready);
        ready.as_mut().enable();
        match stream.next_stale_rebound_packet() {
            Ok(Some(packet)) => break packet,
            Ok(None) => {}
            Err(ViewError::StreamSealed) => return Ok(()),
            Err(error) => return Err(error),
        }
        tokio::select! {
            biased;
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow_and_update() {
                    return Ok(());
                }
            }
            () = stream.wait_terminally_sealed() => return Ok(()),
            () = ready => {}
        }
    };
    let payload = serde_json::to_vec(&serde_json::json!({
        "type": "stream_rebound",
        "previousStreamId": stream.id().to_base64url(),
    }))
    .map_err(|_| ViewError::AllocationFailure)?;
    let total = u64::try_from(payload.len()).map_err(|_| ViewError::LengthUnrepresentable)?;
    let header = TransferHeader::data(
        stream.id(),
        packet_seq,
        1,
        0,
        total,
        RecordKind::Status,
        payload.len(),
    )
    .map_err(|_| ViewError::InvalidCursor)?;
    let frame = header
        .encode(&payload)
        .map_err(|_| ViewError::AllocationFailure)?;
    match stream.mark_stale_rebound_emitted() {
        Ok(()) => {}
        Err(ViewError::StreamSealed) => return Ok(()),
        Err(error) => return Err(error),
    }
    loop {
        let capacity = outbound.capacity_freed.notified();
        tokio::pin!(capacity);
        capacity.as_mut().enable();
        match outbound.send_binary_frame(frame.clone()) {
            Ok(()) => break,
            Err(BinarySendError::Closed) => return Err(ViewError::TransportFailure),
            Err(BinarySendError::Full) => {}
        }
        tokio::select! {
            biased;
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow_and_update() {
                    return Ok(());
                }
            }
            () = stream.wait_terminally_sealed() => return Ok(()),
            () = capacity => {}
        }
    }
    loop {
        tokio::select! {
            biased;
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow_and_update() {
                    return Ok(());
                }
            }
            () = stream.wait_terminally_sealed() => return Ok(()),
        }
    }
}
#[cfg(windows)]
async fn run_view_pump(
    read: NativeSocketViewRead,
    client: SocketViewClient,
    outbound: Outbound,
    mut stop: watch::Receiver<bool>,
) -> ViewPumpDone {
    use crate::application::shared_views::reader::PrepareNext;
    let Ok(stream) = read.stream() else {
        return (
            read,
            None,
            Err(crate::domain::shared_view::ViewError::ReadFailure),
        );
    };
    let mut snapshot_done = false;
    let mut pending_chunk: Option<PreparedSocketChunk> = None;
    let result = 'pump: loop {
        if *stop.borrow_and_update() {
            break Ok(());
        }
        if let Some(chunk) = pending_chunk.take() {
            let capacity = outbound.capacity_freed.notified();
            tokio::pin!(capacity);
            capacity.as_mut().enable();
            match outbound.send_shared(chunk, &client) {
                Ok(()) => continue 'pump,
                Err(SocketEnqueueError::StateLost) => {
                    break Err(crate::domain::shared_view::ViewError::TransportFailure);
                }
                Err(SocketEnqueueError::Rejected(chunk)) => {
                    pending_chunk = Some(chunk);
                    tokio::select! {
                        biased;
                        _ = stop.changed() => {
                            if *stop.borrow_and_update() {
                                break Ok(());
                            }
                        }
                        () = capacity => {}
                    }
                    continue 'pump;
                }
            }
        }
        let next = tokio::select! {
            biased;
            _ = stop.changed() => {
                if *stop.borrow_and_update() {
                    break 'pump Ok(());
                }
                continue 'pump;
            }
            next = async {
                if snapshot_done {
                    read.next_delta(&client).await
                } else {
                    read.next_snapshot(&client).await
                }
            } => next,
        };
        match next {
            Ok(PrepareNext::Chunk(chunk)) => match chunk.into_socket() {
                Ok(chunk) => pending_chunk = Some(chunk),
                Err(error) => break Err(error),
            },
            Ok(PrepareNext::SnapshotFinished) => {
                snapshot_done = true;
            }
            Ok(PrepareNext::AwaitCredit) => {
                let ready = stream.ready_notified();
                tokio::pin!(ready);
                ready.as_mut().enable();
                match stream.ready() {
                    Ok(true) => continue 'pump,
                    Ok(false) => {}
                    Err(crate::domain::shared_view::ViewError::StaleCursor) => {
                        if stream.terminally_sealed().unwrap_or(true) {
                            break Ok(());
                        }
                        break emit_stream_rebound_and_park(
                            &read, &stream, &client, &outbound, &mut stop,
                        )
                        .await;
                    }
                    Err(error) => break Err(error),
                }
                if !stream.can_emit_stale_rebound().unwrap_or(false) {
                    break Ok(());
                }
                if stream.explicitly_sealed().unwrap_or(false)
                    && !stream.view_current().unwrap_or(false)
                {
                    break emit_stream_rebound_and_park(
                        &read, &stream, &client, &outbound, &mut stop,
                    )
                    .await;
                }
                tokio::select! {
                    biased;
                    _ = stop.changed() => {
                        if *stop.borrow_and_update() {
                            break Ok(());
                        }
                    }
                    () = stream.wait_closed() => {
                        if !stream.can_emit_stale_rebound().unwrap_or(false)
                            || stream.view_current().unwrap_or(false)
                        {
                            break Ok(());
                        }
                        break emit_stream_rebound_and_park(
                            &read,
                            &stream,
                            &client,
                            &outbound,
                            &mut stop,
                        )
                        .await;
                    }
                    () = ready => {}
                }
            }
            Ok(PrepareNext::AwaitData) => {
                tokio::select! {
                    biased;
                    _ = stop.changed() => {
                        if *stop.borrow_and_update() {
                            break Ok(());
                        }
                    }
                    () = stream.wait_closed() => {
                        if !stream.can_emit_stale_rebound().unwrap_or(false)
                            || stream.view_current().unwrap_or(false)
                        {
                            break Ok(());
                        }
                        break emit_stream_rebound_and_park(
                            &read,
                            &stream,
                            &client,
                            &outbound,
                            &mut stop,
                        )
                        .await;
                    }
                    changed = read.wait_change(&client) => {
                        match changed {
                            Ok(_) => {}
                            Err(crate::domain::shared_view::ViewError::StaleCursor) => {
                                break emit_stream_rebound_and_park(
                                    &read,
                                    &stream,
                                    &client,
                                    &outbound,
                                    &mut stop,
                                )
                                .await;
                            }
                            Err(error) => break Err(error),
                        }
                    }
                }
            }
            Ok(PrepareNext::Closed) => break Ok(()),
            Err(crate::domain::shared_view::ViewError::StaleCursor) => {
                break emit_stream_rebound_and_park(&read, &stream, &client, &outbound, &mut stop)
                    .await;
            }
            Err(error) => break Err(error),
        }
    };
    (read, pending_chunk, result)
}

#[cfg(windows)]
async fn run_document_pump(
    transfer: crate::application::document_transfer::DocumentTransfer,
    client: SocketViewClient,
    outbound: Outbound,
    mut stop: watch::Receiver<bool>,
    attachment: std::sync::Weak<SocketAttachment>,
) {
    use crate::application::document_transfer::{DocumentEnqueue, DocumentPrepare};
    use crate::domain::shared_view::ViewError;
    let stream = transfer.socket_stream();
    let stream_id = stream.id();
    loop {
        if *stop.borrow_and_update() || stream.explicitly_sealed().unwrap_or(true) {
            break;
        }
        let ready = stream.ready_notified();
        tokio::pin!(ready);
        ready.as_mut().enable();
        let capacity = outbound.capacity_freed.notified();
        tokio::pin!(capacity);
        capacity.as_mut().enable();
        match transfer.prepare_next(&client).await {
            Ok(DocumentPrepare::Prepared) => match outbound.send_document(&transfer, &client) {
                Ok(DocumentEnqueue::Queued) => {}
                Ok(DocumentEnqueue::Backpressured) => {
                    tokio::select! {
                        biased;
                        changed = stop.changed() => {
                            if changed.is_err() || *stop.borrow_and_update() {
                                break;
                            }
                        }
                        () = transfer.wait_closed() => break,
                        () = capacity => {}
                    }
                }
                Err(_) => break,
            },
            Ok(DocumentPrepare::AwaitCredit) => {
                tokio::select! {
                    biased;
                    changed = stop.changed() => {
                        if changed.is_err() || *stop.borrow_and_update() {
                            break;
                        }
                    }
                    () = transfer.wait_closed() => break,
                    () = ready => {}
                }
            }
            Ok(DocumentPrepare::Complete) | Err(_) => break,
        }
    }
    drop(transfer);
    if let Some(attachment) = attachment.upgrade() {
        attachment
            .view_close_receipts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(stream_id, Err(ViewError::StreamSealed));
        attachment
            .streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&stream_id);
        attachment
            .document_pumps
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&stream_id);
    }
}

#[cfg(windows)]
type DocumentPumpHandle = (watch::Sender<bool>, tokio::task::JoinHandle<()>);

struct SocketAttachment {
    client: SocketViewClient,
    #[cfg(windows)]
    upload: Option<UploadOwner>,
    #[cfg(windows)]
    catalog_management: Option<NodeCatalogManagement>,
    streams: Mutex<HashMap<Id128, SocketViewStream>>,
    #[cfg(windows)]
    document_pumps: Mutex<HashMap<Id128, DocumentPumpHandle>>,
    #[cfg(windows)]
    view_pumps: Mutex<HashMap<Id128, ViewPumpState>>,
    #[cfg(windows)]
    view_close_receipts: Mutex<HashMap<Id128, ViewCloseOutcome>>,
}
impl SocketAttachment {
    #[cfg(windows)]
    #[expect(
        clippy::significant_drop_tightening,
        reason = "Hold document_pumps mutex across duplicate check, reservation, spawn, and insertion"
    )]
    fn attach_document(
        self: &Arc<Self>,
        transfer: crate::application::document_transfer::DocumentTransfer,
        outbound: &Outbound,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        let stream = transfer.socket_stream();
        if !stream.is_document() || !stream.belongs_to(&self.client) {
            return Err(crate::domain::shared_view::ViewError::InvalidClient);
        }
        let stream_id = stream.id();
        self.view_close_receipts
            .lock()
            .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?
            .try_reserve(1)
            .map_err(|_| crate::domain::shared_view::ViewError::AllocationFailure)?;
        {
            let mut streams = self
                .streams
                .lock()
                .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
            if streams.contains_key(&stream_id) {
                return Err(crate::domain::shared_view::ViewError::AlreadyAttached);
            }
            streams
                .try_reserve(1)
                .map_err(|_| crate::domain::shared_view::ViewError::AllocationFailure)?;
            streams.insert(stream_id, stream);
        }
        let mut pumps = self
            .document_pumps
            .lock()
            .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
        if pumps.contains_key(&stream_id) {
            drop(pumps);
            self.streams
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&stream_id);
            return Err(crate::domain::shared_view::ViewError::AlreadyAttached);
        }
        if pumps.try_reserve(1).is_err() {
            drop(pumps);
            self.streams
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&stream_id);
            return Err(crate::domain::shared_view::ViewError::AllocationFailure);
        }
        let (stop, stop_rx) = watch::channel(false);
        let client = self.client.context.client();
        let outbound = outbound.clone();
        let weak = Arc::downgrade(self);
        let join = tokio::spawn(run_document_pump(transfer, client, outbound, stop_rx, weak));
        pumps.insert(stream_id, (stop, join));
        drop(pumps);
        Ok(())
    }
    #[cfg(test)]
    fn attach_read(
        &self,
        read: &crate::application::shared_views::reader::OpenedView,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        let stream = read.socket_stream()?;
        if !stream.belongs_to(&self.client) {
            return Err(crate::domain::shared_view::ViewError::InvalidClient);
        }
        let mut streams = self
            .streams
            .lock()
            .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
        streams
            .try_reserve(1)
            .map_err(|_| crate::domain::shared_view::ViewError::AllocationFailure)?;
        streams.insert(stream.id(), stream);
        drop(streams);
        Ok(())
    }
    #[cfg(all(test, windows))]
    fn attach_shared_read(
        &self,
        read: &crate::application::shared_views::reader::OpenedView,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        self.attach_read(read)
    }
    #[cfg(windows)]
    fn attach_view_read(
        &self,
        read: NativeSocketViewRead,
        outbound: &Outbound,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        let stream = read.stream()?;
        if !stream.belongs_to(&self.client) {
            return Err(crate::domain::shared_view::ViewError::InvalidClient);
        }
        let stream_id = Id128::new(*stream.id().as_bytes());
        {
            let mut streams = self
                .streams
                .lock()
                .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
            if streams.contains_key(&stream_id) {
                return Err(crate::domain::shared_view::ViewError::AlreadyAttached);
            }
            streams
                .try_reserve(1)
                .map_err(|_| crate::domain::shared_view::ViewError::AllocationFailure)?;
        }
        {
            let mut pumps = self
                .view_pumps
                .lock()
                .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
            if pumps.contains_key(&stream_id) {
                return Err(crate::domain::shared_view::ViewError::AlreadyAttached);
            }
            pumps
                .try_reserve(1)
                .map_err(|_| crate::domain::shared_view::ViewError::AllocationFailure)?;
        }
        let (stop, stop_rx) = watch::channel(false);
        {
            let mut streams = self
                .streams
                .lock()
                .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
            if streams.contains_key(&stream_id) {
                return Err(crate::domain::shared_view::ViewError::AlreadyAttached);
            }
            streams.insert(stream_id, stream);
        }
        let client = self.client.context.client();
        let outbound = outbound.clone();
        let join = tokio::spawn(run_view_pump(read, client, outbound, stop_rx));
        let mut pumps = self
            .view_pumps
            .lock()
            .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
        pumps.insert(stream_id, ViewPumpState::Active { stop, join });
        Ok(())
    }
    #[cfg(windows)]
    async fn rebind_view_read(
        self: &Arc<Self>,
        stream_id: Id128,
    ) -> Result<NativeSocketViewRead, crate::domain::shared_view::ViewError> {
        use crate::domain::shared_view::ViewError;
        let () = self
            .view_close_receipts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        let res_rx = {
            let mut pumps = self
                .view_pumps
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            match pumps.remove(&stream_id) {
                None => {
                    let closed = self
                        .view_close_receipts
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .contains_key(&stream_id);
                    return Err(if closed {
                        ViewError::StreamSealed
                    } else {
                        ViewError::InvalidClient
                    });
                }
                Some(ViewPumpState::Closing(rx)) => {
                    pumps.insert(stream_id, ViewPumpState::Closing(rx));
                    return Err(ViewError::InvalidStage);
                }
                Some(ViewPumpState::Active { stop, join }) => {
                    let (tx, rx) = watch::channel(None);
                    pumps.insert(stream_id, ViewPumpState::Closing(rx));
                    let (res_tx, res_rx) = tokio::sync::oneshot::channel();
                    let this = Arc::clone(self);
                    tokio::spawn(async move {
                        let _ = stop.send(true);
                        let (outcome, rebound) = match join.await {
                            Ok((read, pending_chunk, _)) => {
                                if pending_chunk.is_some() {
                                    drop(pending_chunk);
                                    let receipt = read.release();
                                    (Ok(receipt), Err(ViewError::StreamSealed))
                                } else {
                                    let client = this.client.context.client();
                                    let rebound =
                                        match read.wait_original_progress(&client).await {
                                            Ok(crate::application::shared_views::reader::OriginalReadProgress::Complete { .. }) => {
                                                read.rebind_same_read(&client).await
                                            }
                                            Ok(crate::application::shared_views::reader::OriginalReadProgress::Failed(error)) => {
                                                Err(error)
                                            }
                                            Ok(_) => Err(ViewError::StaleCursor),
                                            Err(error) => Err(error),
                                        };
                                    let receipt = read.release();
                                    (Ok(receipt), rebound)
                                }
                            }
                            Err(_) => (Err(ViewError::ReadFailure), Err(ViewError::ReadFailure)),
                        };
                        this.streams
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .remove(&stream_id);
                        this.view_close_receipts
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .insert(stream_id, outcome.clone());
                        this.view_pumps
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .remove(&stream_id);
                        let _ = tx.send(Some(outcome));
                        let _ = res_tx.send(rebound);
                    });
                    res_rx
                }
            }
        };
        res_rx.await.map_err(|_| ViewError::ReadFailure)?
    }
    #[cfg(windows)]
    async fn close_view_read(self: &Arc<Self>, stream_id: Id128) -> ViewCloseOutcome {
        {
            let mut receipts = self
                .view_close_receipts
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if let Some(receipt) = receipts.get(&stream_id) {
                return receipt.clone();
            }
            if receipts.try_reserve(1).is_err() {
                return Err(crate::domain::shared_view::ViewError::AllocationFailure);
            }
        }
        enum Action {
            Return(ViewCloseOutcome),
            Wait(watch::Receiver<Option<ViewCloseOutcome>>),
        }
        let action = {
            let mut pumps = self
                .view_pumps
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            match pumps.remove(&stream_id) {
                None => {
                    let receipts = self
                        .view_close_receipts
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    Action::Return(
                        receipts
                            .get(&stream_id)
                            .cloned()
                            .unwrap_or(Err(crate::domain::shared_view::ViewError::InvalidClient)),
                    )
                }
                Some(ViewPumpState::Closing(rx)) => {
                    pumps.insert(stream_id, ViewPumpState::Closing(rx.clone()));
                    Action::Wait(rx)
                }
                Some(ViewPumpState::Active { stop, join }) => {
                    let (tx, rx) = watch::channel(None);
                    pumps.insert(stream_id, ViewPumpState::Closing(rx.clone()));
                    let this = Arc::clone(self);
                    tokio::spawn(async move {
                        let _ = stop.send(true);
                        let outcome = match join.await {
                            Ok((read, pending_chunk, _)) => {
                                let receipt = read.release();
                                drop(pending_chunk);
                                Ok(receipt)
                            }
                            Err(_) => Err(crate::domain::shared_view::ViewError::ReadFailure),
                        };
                        this.streams
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .remove(&stream_id);
                        this.view_close_receipts
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .insert(stream_id, outcome.clone());
                        this.view_pumps
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .remove(&stream_id);
                        let _ = tx.send(Some(outcome));
                    });
                    Action::Wait(rx)
                }
            }
        };
        let mut rx = match action {
            Action::Return(outcome) => return outcome,
            Action::Wait(rx) => rx,
        };
        loop {
            if let Some(outcome) = rx.borrow_and_update().clone() {
                return outcome;
            }
            if rx.changed().await.is_err() {
                return self
                    .view_close_receipts
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(&stream_id)
                    .cloned()
                    .unwrap_or(Err(crate::domain::shared_view::ViewError::ReadFailure));
            }
        }
    }
    #[cfg(windows)]
    async fn drain(&self) {
        loop {
            let mut active = Vec::new();
            let mut closing = Vec::new();
            {
                let mut pumps = self
                    .view_pumps
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                if pumps.is_empty() {
                    break;
                }
                let keys: Vec<Id128> = pumps.keys().copied().collect();
                for stream_id in keys {
                    match pumps.remove(&stream_id) {
                        Some(ViewPumpState::Active { stop, join }) => {
                            let (tx, rx) = watch::channel(None);
                            pumps.insert(stream_id, ViewPumpState::Closing(rx));
                            active.push((stream_id, stop, join, tx));
                        }
                        Some(ViewPumpState::Closing(rx)) => {
                            pumps.insert(stream_id, ViewPumpState::Closing(rx.clone()));
                            closing.push(rx);
                        }
                        None => {}
                    }
                }
            }
            for (stream_id, stop, join, tx) in active {
                let _ = stop.send(true);
                let outcome = match join.await {
                    Ok((read, pending_chunk, _)) => {
                        let receipt = read.release();
                        drop(pending_chunk);
                        Ok(receipt)
                    }
                    Err(_) => Err(crate::domain::shared_view::ViewError::ReadFailure),
                };
                self.streams
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&stream_id);
                self.view_close_receipts
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(stream_id, outcome.clone());
                self.view_pumps
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&stream_id);
                let _ = tx.send(Some(outcome));
            }
            for mut rx in closing {
                while rx.borrow_and_update().is_none() {
                    if rx.changed().await.is_err() {
                        break;
                    }
                }
            }
        }
        let document_pumps: Vec<_> = self
            .document_pumps
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .collect();
        for (stream_id, (stop, join)) in document_pumps {
            let _ = stop.send(true);
            let _ = join.await;
            self.view_close_receipts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(
                    stream_id,
                    Err(crate::domain::shared_view::ViewError::StreamSealed),
                );
            self.streams
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&stream_id);
        }
    }
    async fn control(
        &self,
        bytes: &[u8],
        _outbound: &Outbound,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        use crate::application::shared_views::wire::{PacketKind, TransferHeader};
        let (h, payload) = TransferHeader::decode(bytes)
            .map_err(|_| crate::domain::shared_view::ViewError::InvalidCursor)?;
        let stream = self
            .streams
            .lock()
            .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?
            .get(&h.stream())
            .cloned();
        let Some(stream) = stream else {
            let owned_closed = self
                .view_close_receipts
                .lock()
                .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?
                .contains_key(&h.stream());
            if owned_closed
                && payload.is_empty()
                && matches!(h.kind(), PacketKind::Credit | PacketKind::Detach)
            {
                return Ok(());
            }
            return Err(crate::domain::shared_view::ViewError::InvalidClient);
        };
        let _ = stream.accept_control(&self.client, bytes).await?;
        Ok(())
    }
}
impl Drop for SocketAttachment {
    fn drop(&mut self) {
        self.client.context.lifetime.seal();
    }
}

#[cfg(test)]
#[path = "management_ws/socket_development.rs"]
mod socket_development;

#[cfg(test)]
type SocketTestAccepted = (Arc<SocketAttachment>, Outbound);

#[cfg(all(test, windows))]
#[path = "management_ws/socket_independent_acceptance.rs"]
mod socket_independent_acceptance;

#[cfg(test)]
mod management_independent;

#[cfg(test)]
pub(crate) struct DocumentSocketEndpoint {
    attachment: Arc<SocketAttachment>,
    outbound: Outbound,
}
#[cfg(test)]
impl DocumentSocketEndpoint {
    #[cfg(windows)]
    pub(crate) fn attach_document(
        &self,
        transfer: crate::application::document_transfer::DocumentTransfer,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        self.attachment.attach_document(transfer, &self.outbound)
    }
    #[cfg(windows)]
    pub(crate) fn attach_shared_read(
        &self,
        read: &crate::application::shared_views::reader::OpenedView,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        self.attachment.attach_shared_read(read)
    }
    #[cfg(windows)]
    pub(crate) fn outbound_observation_for_test(&self) -> (usize, usize) {
        (
            MAX_OUTBOUND_BYTES - self.outbound.bytes.available_permits(),
            MAX_OUTBOUND_FRAMES - self.outbound.sender.capacity(),
        )
    }
    #[cfg(windows)]
    pub(crate) fn hold_outbound_bytes_below_ack_for_test(
        &self,
    ) -> tokio::sync::OwnedSemaphorePermit {
        self.outbound
            .bytes
            .clone()
            .try_acquire_many_owned(
                u32::try_from(MAX_OUTBOUND_BYTES - (input_upload::HEADER_BYTES - 1))
                    .expect("outbound byte budget fits u32"),
            )
            .expect("fixture reserves all but one ACK's byte budget")
    }
    pub(crate) fn token_observation(
        &self,
        transfer: &crate::application::document_transfer::DocumentTransfer,
    ) -> Result<
        Option<crate::application::shared_views::socket_transfer::SocketTokenObservation>,
        crate::application::document_source::DocumentError,
    > {
        let stream = transfer.socket_stream();
        if !stream.belongs_to(&self.attachment.client) {
            return Err(crate::domain::shared_view::ViewError::InvalidClient.into());
        }
        Ok(stream.token_observation()?)
    }

    pub(crate) fn client(&self) -> SocketViewClient {
        self.attachment.client.context.client()
    }
    pub(crate) fn attach(
        &self,
        transfer: &crate::application::document_transfer::DocumentTransfer,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        let stream = transfer.socket_stream();
        if !stream.is_document() || !stream.belongs_to(&self.attachment.client) {
            return Err(crate::domain::shared_view::ViewError::InvalidClient);
        }
        let mut streams = self
            .attachment
            .streams
            .lock()
            .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
        streams
            .try_reserve(1)
            .map_err(|_| crate::domain::shared_view::ViewError::AllocationFailure)?;
        streams.insert(stream.id(), stream);
        Ok(())
    }
    pub(crate) fn send(
        &self,
        transfer: &crate::application::document_transfer::DocumentTransfer,
    ) -> Result<
        crate::application::document_transfer::DocumentEnqueue,
        crate::application::document_source::DocumentError,
    > {
        self.outbound
            .send_document(transfer, &self.attachment.client)
    }
    pub(crate) fn observation(
        &self,
        transfer: &crate::application::document_transfer::DocumentTransfer,
    ) -> Result<
        crate::application::shared_views::local_transfer::LocalTransferObservation,
        crate::application::document_source::DocumentError,
    > {
        if !transfer.socket_stream().belongs_to(&self.attachment.client) {
            return Err(crate::domain::shared_view::ViewError::InvalidClient.into());
        }
        transfer.observation()
    }
    pub(crate) fn flush_observation(
        &self,
        transfer: &crate::application::document_transfer::DocumentTransfer,
    ) -> Result<
        crate::application::shared_views::socket_transfer::FlushPollObservation,
        crate::application::document_source::DocumentError,
    > {
        let stream = transfer.socket_stream();
        if !stream.belongs_to(&self.attachment.client) {
            return Err(crate::domain::shared_view::ViewError::InvalidClient.into());
        }
        Ok(stream.flush_poll_observation())
    }
}

#[cfg(test)]
#[path = "management_ws/connection_owner/development_tests.rs"]
mod connection_owner_development_tests;
