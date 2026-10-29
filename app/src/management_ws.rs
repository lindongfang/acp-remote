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
        Ok(Self {
            state: Arc::new(ManagementState {
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
                #[cfg(test)]
                write_barrier: Mutex::new(None),
                #[cfg(test)]
                auth_resolve_barrier: Mutex::new(None),
                #[cfg(test)]
                test_hooks: ManagementTestHooks::default(),
            }),
        })
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
            ManagementBoundary::Enqueue => &self.state.test_hooks.enqueue,
            ManagementBoundary::WriteLocked => &self.state.test_hooks.write_locked,
            ManagementBoundary::SubscriptionRevision => {
                &self.state.test_hooks.subscription_revision
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
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) enum ManagementBoundary {
    Admission,
    Enqueue,
    SubscriptionRevision,
    WriteLocked,
}

#[cfg(test)]
#[derive(Default)]
struct ManagementTestHooks {
    admission: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    enqueue: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    subscription_revision: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    write_locked: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    protected_writes: std::sync::atomic::AtomicUsize,
    admitted: std::sync::atomic::AtomicUsize,
    enqueued: std::sync::atomic::AtomicUsize,
    fresh_command_window: std::sync::atomic::AtomicBool,
    overflow_bytes: std::sync::atomic::AtomicUsize,
    overflow_frames: std::sync::atomic::AtomicUsize,
    rejected_bytes: std::sync::atomic::AtomicUsize,
    last_close_code: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
async fn pause_boundary(slot: &Mutex<Option<crate::test_support::SharedAsyncBarrier>>) {
    let barrier = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(barrier) = barrier {
        barrier.pause().await;
    }
}

struct ManagementState {
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
    #[cfg(test)]
    write_barrier: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
    #[cfg(test)]
    auth_resolve_barrier: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
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

#[derive(Default)]
struct RegistryState {
    total: usize,
    unauthenticated: usize,
    authenticated: HashMap<Id128, usize>,
    closing: bool,
}

struct ConnectionRegistry {
    state: Mutex<RegistryState>,
    changed: Notify,
    shutdown: watch::Sender<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AcquireError {
    Closing,
    Limit,
}

struct ConnectionLease {
    registry: Arc<ConnectionRegistry>,
    authenticated: Option<Id128>,
}

impl ConnectionRegistry {
    fn new() -> Self {
        let (shutdown, _) = watch::channel(false);
        Self {
            state: Mutex::new(RegistryState::default()),
            changed: Notify::new(),
            shutdown,
        }
    }

    fn new_shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    fn acquire(self: &Arc<Self>) -> Result<ConnectionLease, AcquireError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.closing {
            return Err(AcquireError::Closing);
        }
        if state.total >= MAX_MANAGEMENT_CONNECTIONS
            || state.unauthenticated >= MAX_UNAUTHENTICATED_CONNECTIONS
        {
            return Err(AcquireError::Limit);
        }
        state.total += 1;
        state.unauthenticated += 1;
        drop(state);
        Ok(ConnectionLease {
            registry: Arc::clone(self),
            authenticated: None,
        })
    }

    fn begin_shutdown(&self) {
        let changed = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if state.closing {
                false
            } else {
                state.closing = true;
                true
            }
        };
        if changed {
            self.shutdown.send_replace(true);
            self.changed.notify_waiters();
        }
    }

    async fn wait_empty(&self) {
        loop {
            let notified = self.changed.notified();
            if self
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .total
                == 0
            {
                return;
            }
            notified.await;
        }
    }

    fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }
}

impl ConnectionLease {
    fn authenticate(&mut self, client_id: Id128) -> bool {
        let mut state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.closing {
            return false;
        }
        let count = state.authenticated.get(&client_id).copied().unwrap_or(0);
        if count >= MAX_CONNECTIONS_PER_CLIENT {
            return false;
        }
        state.unauthenticated = state.unauthenticated.saturating_sub(1);
        *state.authenticated.entry(client_id).or_default() += 1;
        drop(state);
        self.authenticated = Some(client_id);
        true
    }
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        let empty = {
            let mut state = self
                .registry
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            state.total = state.total.saturating_sub(1);
            if let Some(client_id) = self.authenticated {
                if let Some(count) = state.authenticated.get_mut(&client_id) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        state.authenticated.remove(&client_id);
                    }
                }
            } else {
                state.unauthenticated = state.unauthenticated.saturating_sub(1);
            }
            state.total == 0
        };
        if empty {
            self.registry.changed.notify_one();
        }
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

    let connections = Arc::clone(&state.connections);
    let mut shutdown = connections.shutdown_receiver();
    ws.protocols([CONTROL_PROTOCOL])
        .max_message_size(MAX_MANAGEMENT_FRAME_BYTES + 1)
        .max_frame_size(MAX_MANAGEMENT_FRAME_BYTES + 1)
        .on_upgrade(move |socket| async move {
            let lease = match connections.acquire() {
                Ok(lease) => lease,
                Err(AcquireError::Closing) => {
                    close_immediately(socket, close(CLOSE_GOING_AWAY, "node stopping")).await;
                    return;
                }
                Err(AcquireError::Limit) => {
                    close_immediately(socket, close(CLOSE_RATE_LIMITED, "limit exceeded")).await;
                    return;
                }
            };
            serve_connection(socket, state, lease, &mut shutdown).await;
        })
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

async fn serve_connection(
    socket: WebSocket,
    state: Arc<ManagementState>,
    mut lease: ConnectionLease,
    shutdown: &mut watch::Receiver<bool>,
) {
    let (sink, mut reader) = socket.split();
    let (outbound, outbound_rx) = Outbound::new();
    let (close_signal, close_rx) = watch::channel(None);
    let (write_authority, write_authority_rx) = watch::channel(None);
    let mut writer = tokio::spawn(write_loop(sink, outbound_rx, close_rx, write_authority_rx));
    let result = run_protocol(
        &mut reader,
        &state,
        &mut lease,
        &outbound,
        shutdown,
        &write_authority,
    )
    .await;
    #[cfg(test)]
    state.test_hooks.last_close_code.store(
        usize::from(result.code),
        std::sync::atomic::Ordering::SeqCst,
    );
    close_signal.send_replace(Some(result));
    if timeout(WRITER_CLOSE_TIMEOUT, &mut writer).await.is_err() {
        writer.abort();
        let _ = writer.await;
    }
    drop(outbound);
}

#[allow(clippy::too_many_lines)]
async fn run_protocol(
    reader: &mut SplitStream<WebSocket>,
    state: &Arc<ManagementState>,
    lease: &mut ConnectionLease,
    outbound: &Outbound,
    shutdown: &mut watch::Receiver<bool>,
    write_authority: &watch::Sender<Option<WriteAuthority>>,
) -> CloseFrame {
    if *shutdown.borrow() {
        return close(CLOSE_GOING_AWAY, "node stopping");
    }

    let hello = tokio::select! {
        biased;
        changed = shutdown.changed() => {
            if changed.is_err() || *shutdown.borrow() {
                return close(CLOSE_GOING_AWAY, "node stopping");
            }
            return close(CLOSE_INTERNAL, "internal error");
        }
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
        changed = shutdown.changed() => {
            if changed.is_err() || *shutdown.borrow() {
                return close(CLOSE_GOING_AWAY, "node stopping");
            }
            return close(CLOSE_INTERNAL, "internal error");
        }
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
    write_authority.send_replace(Some(WriteAuthority {
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

    run_authenticated(reader, state, authenticated, outbound, shutdown, revisions).await
}

async fn run_authenticated(
    reader: &mut SplitStream<WebSocket>,
    state: &Arc<ManagementState>,
    authenticated: AuthenticatedClient,
    outbound: &Outbound,
    shutdown: &mut watch::Receiver<bool>,
    mut revisions: watch::Receiver<u64>,
) -> CloseFrame {
    let mut subscribed = false;
    let mut last_sent_revision = None;
    let mut command_window = CommandWindow::new();

    loop {
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return close(CLOSE_GOING_AWAY, "node stopping");
                }
            }
            changed = revisions.changed() => {
                if changed.is_err() {
                    return close(CLOSE_INTERNAL, "storage unavailable");
                }
                let revision = *revisions.borrow_and_update();
                let _gate = match lock_current_actor(state, &authenticated).await {
                    Ok(gate) => gate,
                    Err(error) => return error,
                };
                if subscribed && last_sent_revision != Some(revision) {
                    if outbound.send_json(&DevicesChangedMessage {
                        v: 1,
                        kind: "devices.changed",
                        revision,
                    }).is_err() {
                        return close(CLOSE_RATE_LIMITED, "limit exceeded");
                    }
                    last_sent_revision = Some(revision);
                }
            }
            received = receive_frame(reader) => {
                let frame = match received {
                    Ok(frame) => frame,
                    Err(close) => return close,
                };
                #[cfg(test)]
                if state.test_hooks.fresh_command_window.load(std::sync::atomic::Ordering::SeqCst) {
                    command_window = CommandWindow::new();
                }
                if !command_window.accept() {
                    return close(CLOSE_RATE_LIMITED, "limit exceeded");
                }
                match frame {
                    ClientFrame::Command(command) => {
                        match execute_command(
                            state,
                            &authenticated,
                            command.request_id,
                            &command.action,
                            command.payload,
                        ).await {
                            CommandOutcome::Result(result) => {
                                if let Err(error) = enqueue_authenticated(state, &authenticated, outbound, &result).await {
                                    return error;
                                }
                            }
                            CommandOutcome::Close(close) => return close,
                        }
                    }
                    ClientFrame::Subscribe(subscribe) => {
                        match subscribe_devices(state, &authenticated, outbound, subscribe).await {
                            Ok(Some(revision)) => {
                                subscribed = true;
                                last_sent_revision = Some(revision);
                            }
                            Ok(None) => {}
                            Err(error) => return error,
                        }
                    }
                    ClientFrame::Hello(_) | ClientFrame::Proof(_) => {
                        return close(CLOSE_FORMAT, "invalid frame");
                    }
                }
            }
        }
    }
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

async fn receive_frame(reader: &mut SplitStream<WebSocket>) -> Result<ClientFrame, CloseFrame> {
    loop {
        let message = reader
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
            Message::Binary(_) => return Err(close(CLOSE_FORMAT, "invalid frame")),
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Close(_) => return Err(close(1000, "normal closure")),
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

struct Outbound {
    sender: mpsc::Sender<QueuedFrame>,
    bytes: Arc<Semaphore>,
}

#[derive(Clone)]
struct WriteAuthority {
    state: Arc<ManagementState>,
    authenticated: AuthenticatedClient,
}

struct QueuedFrame {
    json: String,
    _permit: OwnedSemaphorePermit,
}

impl Outbound {
    fn new() -> (Self, mpsc::Receiver<QueuedFrame>) {
        let (sender, receiver) = mpsc::channel(MAX_OUTBOUND_FRAMES);
        (
            Self {
                sender,
                bytes: Arc::new(Semaphore::new(MAX_OUTBOUND_BYTES)),
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
                json,
                _permit: permit,
            })
            .map_err(|_| ())
    }
}

async fn write_loop(
    mut sink: futures_util::stream::SplitSink<WebSocket, Message>,
    mut outbound: mpsc::Receiver<QueuedFrame>,
    mut close_rx: watch::Receiver<Option<CloseFrame>>,
    authority: watch::Receiver<Option<WriteAuthority>>,
) {
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
                    let _ = timeout(
                        WRITER_CLOSE_TIMEOUT,
                        sink.send(Message::Close(Some(frame))),
                    ).await;
                    return;
                }
                if changed.is_err() {
                    return;
                }
            }
            queued = outbound.recv() => {
                let Some(queued) = queued else {
                    return;
                };
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
                let _gate = if let Some(authority) = &write_authority {
                    let gate = authority
                        .state
                        .trust
                        .client_gate(&authority.authenticated.client_id_string);
                    let gate = gate.lock_owned().await;
                    match still_valid(&authority.state, &authority.authenticated).await {
                        Ok(true) => {}
                        Ok(false) => {
                            let _ = timeout(
                                WRITER_CLOSE_TIMEOUT,
                                sink.send(Message::Close(Some(close(CLOSE_REVOKED, "revoked")))),
                            ).await;
                            return;
                        }
                        Err(()) => {
                            let _ = timeout(
                                WRITER_CLOSE_TIMEOUT,
                                sink.send(Message::Close(Some(close(
                                    CLOSE_INTERNAL,
                                    "storage unavailable",
                                )))),
                            ).await;
                            return;
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
                let message = Message::Text(Utf8Bytes::from(queued.json));
                match timeout(WRITER_SEND_TIMEOUT, sink.send(message)).await {
                    Ok(Ok(())) => {
                        #[cfg(test)]
                        if let Some(authority) = &write_authority {
                            authority.state.test_hooks.protected_writes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                    Ok(Err(_)) | Err(_) => return,
                }
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
