use super::model::{field, object};
use super::{
    AcpError, PermissionRequest, PromptResult, RawMessage, Request, RequestKind, RpcId,
    SessionEvent, SessionId, Strategy, Version,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    time::{Instant, timeout_at},
};
const MAX_FRAME: usize = 32 * 1024 * 1024;
const MAX_PENDING: usize = 64;
const MAX_SERVER_IDS: usize = 4096;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Fresh,
    Initializing,
    Ready,
    Closed,
}
struct Pending {
    kind: RequestKind,
    deadline: Instant,
    snapshot: bool,
}
/// One transport owner, one writer, no detached tasks or unbounded output queues.
/// Dropping it closes its I/O halves; the separate process owner must still reclaim its Job.
pub struct Connection<R, W> {
    owner_tag: Arc<()>,
    reader: BufReader<R>,
    writer: W,
    input: Vec<u8>,
    pending: HashMap<RpcId, Pending>,
    server_pending: HashMap<RpcId, ([u8; 32], bool)>,
    server_seen: HashSet<RpcId>,
    next_id: i64,
    phase: Phase,
    strategy: Option<Strategy>,
    rejected: Option<RawMessage>,
}
#[derive(Debug)]
pub enum Response {
    Initialized(Strategy),
    NewSession(SessionId),
    Prompt(PromptResult),
    Resumed { session: SessionId },
    Closed { session: SessionId },
    RemoteError { code: i64 },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateOrigin {
    Live,
    DuringResume,
}
#[derive(Debug)]
pub enum Incoming {
    Response {
        id: RpcId,
        response: Response,
        raw: RawMessage,
    },
    Permission(PermissionRequest),
    Update {
        event: SessionEvent,
        origin: UpdateOrigin,
        raw: RawMessage,
    },
    UnsupportedRequest {
        id: RpcId,
        raw: RawMessage,
    },
    UnknownNotification {
        raw: RawMessage,
    },
}
impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> Connection<R, W> {
    #[must_use]
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            owner_tag: Arc::new(()),
            reader: BufReader::new(reader),
            writer,
            input: Vec::new(),
            pending: HashMap::new(),
            server_pending: HashMap::new(),
            server_seen: HashSet::new(),
            next_id: 0,
            phase: Phase::Fresh,
            strategy: None,
            rejected: None,
        }
    }
    #[must_use]
    pub const fn strategy(&self) -> Option<&Strategy> {
        self.strategy.as_ref()
    }
    /// Takes the single parsed frame rejected for control/correlation semantics.
    /// It remains original bytes; callers must not put sensitive contents into logs or errors.
    #[must_use]
    pub const fn take_rejected_message(&mut self) -> Option<RawMessage> {
        self.rejected.take()
    }
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        matches!(self.phase, Phase::Closed)
    }
    /// Attempts exactly one initialize. A failed/unknown attempt never permits retry on this connection.
    /// # Errors
    /// Rejects a repeated attempt, failed write or expired deadline.
    pub async fn initialize(
        &mut self,
        offer: Version,
        deadline: Instant,
    ) -> Result<RpcId, AcpError> {
        if self.phase != Phase::Fresh {
            return Err(AcpError::AlreadyInitialized);
        }
        self.phase = Phase::Initializing;
        self.submit(super::strategy::initialize(offer), deadline)
            .await
    }
    /// Registers before writing. Cancellation during any write permanently poisons this connection.
    /// # Errors
    /// Rejects uninitialized/closed connections, capacity exhaustion and unknown writes.
    pub async fn request(
        &mut self,
        request: Request,
        deadline: Instant,
    ) -> Result<RpcId, AcpError> {
        if self.phase != Phase::Ready {
            return Err(if self.is_closed() {
                AcpError::Closed
            } else {
                AcpError::NotInitialized
            });
        }
        if !self
            .strategy
            .as_ref()
            .is_some_and(|strategy| strategy.owns_request(&request))
        {
            return Err(AcpError::InvalidRequestScope);
        }
        self.submit(request, deadline).await
    }
    async fn submit(&mut self, request: Request, deadline: Instant) -> Result<RpcId, AcpError> {
        if self.pending.len() >= MAX_PENDING {
            return Err(AcpError::Capacity);
        }
        let id = self
            .next_id
            .checked_add(1)
            .filter(|n| *n <= 9_007_199_254_740_991)
            .ok_or(AcpError::Capacity)?;
        self.next_id = id;
        let id = RpcId::Integer(id);
        let frame =
            json!({"jsonrpc":"2.0","id":id.json(),"method":request.method,"params":request.params});
        self.pending.insert(
            id.clone(),
            Pending {
                kind: request.kind,
                deadline,
                snapshot: false,
            },
        );
        self.write(frame, deadline).await?;
        Ok(id)
    }
    /// Notification only; cancellation never frees the foreground Run by itself.
    /// # Errors
    /// Rejects uninitialized state and unknown writes. Application ownership checks precede this call.
    pub async fn cancel(&mut self, session: &SessionId, deadline: Instant) -> Result<(), AcpError> {
        self.require_ready()?;
        self.write(json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session.as_str()}}),deadline).await
    }
    /// Delivers one selected result directly to the sole writer. This is not peer execution ACK.
    /// # Errors
    /// Rejects spent/unknown request IDs and unsupported/unadvertised options.
    pub async fn permission_response(
        &mut self,
        request: &PermissionRequest,
        selection: Option<&super::OptionId>,
        deadline: Instant,
    ) -> Result<(), AcpError> {
        self.require_ready()?;
        if request
            .connection
            .as_ref()
            .is_none_or(|source| !Arc::ptr_eq(source, &self.owner_tag))
        {
            return Err(AcpError::InvalidMessage);
        }
        let expected = self
            .server_pending
            .get(&request.id)
            .ok_or(AcpError::UnmatchedResponse)?;
        if !expected.1 || expected.0.as_slice() != Sha256::digest(request.raw.bytes()).as_slice() {
            return Err(AcpError::InvalidMessage);
        }
        let strategy = self.strategy.as_ref().ok_or(AcpError::NotInitialized)?;
        let id = RpcId::parse(
            request
                .raw
                .fields()
                .get("id")
                .ok_or(AcpError::InvalidMessage)?,
        )?;
        if id != request.id {
            return Err(AcpError::InvalidMessage);
        }
        let original = strategy.permission(
            id,
            request.raw.clone(),
            request
                .raw
                .fields()
                .get("params")
                .ok_or(AcpError::InvalidMessage)?,
        )?;
        let frame = strategy.permission_response(&original, selection)?;
        self.server_pending.remove(&request.id);
        self.write(frame, deadline).await
    }

    /// Standard method-not-found; unsupported client surfaces are never silently accepted.
    /// # Errors
    /// Rejects unknown/spent request IDs or failed writes.
    pub async fn reject_unsupported(
        &mut self,
        id: &RpcId,
        deadline: Instant,
    ) -> Result<(), AcpError> {
        self.require_ready()?;
        if self.server_pending.get(id).is_none_or(|entry| entry.1) {
            return Err(AcpError::UnmatchedResponse);
        }
        self.server_pending.remove(id);
        self.write(json!({"jsonrpc":"2.0","id":id.json(),"error":{"code":-32601,"message":"Client method is not supported"}}),deadline).await
    }
    async fn write(&mut self, frame: Value, deadline: Instant) -> Result<(), AcpError> {
        if self.phase == Phase::Closed {
            return Err(AcpError::Closed);
        }
        if Instant::now() >= deadline {
            self.poison();
            return Err(AcpError::Deadline);
        }
        let mut bounded = FrameBuffer(Vec::new());
        if serde_json::to_writer(&mut bounded, &frame).is_err() {
            self.poison();
            return Err(AcpError::Capacity);
        }
        let mut bytes = bounded.0;
        bytes.push(b'\n');
        // A dropped future leaves phase Closed. Restore it only after write_all AND flush complete.
        let prior = self.phase;
        self.phase = Phase::Closed;
        let result = timeout_at(deadline, async {
            self.writer.write_all(&bytes).await?;
            self.writer.flush().await
        })
        .await;
        if matches!(result, Ok(Ok(()))) {
            self.phase = prior;
            Ok(())
        } else {
            self.poison();
            Err(AcpError::DeliveryUnknown)
        }
    }

    /// Pumps one frame; caller owns polling and must not place permission handling behind a prompt wait.
    /// # Errors
    /// Deadlines, malformed/unmatched messages and EOF invalidate this connection without retransmission.
    pub async fn next(&mut self, deadline: Instant) -> Result<Incoming, AcpError> {
        if self.phase == Phase::Fresh {
            return Err(AcpError::NotInitialized);
        }
        if self.phase == Phase::Closed {
            return Err(AcpError::Closed);
        }
        let deadline = self
            .pending
            .values()
            .map(|p| p.deadline)
            .min()
            .map_or(deadline, |d| d.min(deadline));
        if Instant::now() >= deadline {
            self.poison();
            return Err(AcpError::Deadline);
        }
        let raw = match timeout_at(deadline, self.read_frame()).await {
            Ok(Ok(raw)) => raw,
            Ok(Err(e)) => {
                self.poison();
                return Err(e);
            }
            Err(_) => {
                self.poison();
                return Err(AcpError::Deadline);
            }
        };
        match self.route(raw.clone()) {
            Ok(event) => Ok(event),
            Err(e) => {
                self.rejected = Some(raw);
                self.poison();
                Err(e)
            }
        }
    }
    async fn read_frame(&mut self) -> Result<RawMessage, AcpError> {
        loop {
            let bytes = self.reader.fill_buf().await.map_err(|_| AcpError::Io)?;
            if bytes.is_empty() {
                return Err(if self.input.is_empty() {
                    AcpError::DeliveryUnknown
                } else {
                    AcpError::InvalidMessage
                });
            }
            let newline = bytes.iter().position(|b| *b == b'\n');
            let count = newline.map_or(bytes.len(), |i| i + 1);
            if self.input.len().saturating_add(count) > MAX_FRAME + 2 {
                return Err(AcpError::Capacity);
            }
            self.input.extend_from_slice(&bytes[..count]);
            self.reader.consume(count);
            if newline.is_some() {
                self.input.pop();
                if self.input.last() == Some(&b'\r') {
                    self.input.pop();
                }
                if self.input.len() > MAX_FRAME {
                    return Err(AcpError::Capacity);
                }
                return RawMessage::parse(std::mem::take(&mut self.input));
            }
        }
    }
    fn route(&mut self, raw: RawMessage) -> Result<Incoming, AcpError> {
        let fields = raw.fields();
        if fields.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Err(AcpError::InvalidMessage);
        }
        if let Some(method) = fields.get("method") {
            if fields.contains_key("result") || fields.contains_key("error") {
                return Err(AcpError::InvalidMessage);
            }
            let method = method.as_str().ok_or(AcpError::InvalidMessage)?;
            let absent = Value::Null;
            let params = fields.get("params").unwrap_or(&absent);
            if !params.is_null() && !params.is_object() && !params.is_array() {
                return Err(AcpError::InvalidMessage);
            }
            let strategy = self.strategy.as_ref().ok_or(AcpError::NotInitialized)?;
            if let Some(id) = fields.get("id") {
                let id = RpcId::parse(id)?;
                if self.server_seen.len() >= MAX_SERVER_IDS
                    || self.server_pending.len() >= MAX_PENDING
                {
                    return Err(AcpError::Capacity);
                }
                if !self.server_seen.insert(id.clone()) {
                    return Err(AcpError::InvalidMessage);
                }
                self.server_pending.insert(
                    id.clone(),
                    (
                        Sha256::digest(raw.bytes()).into(),
                        method == "session/request_permission",
                    ),
                );
                if method == "session/request_permission" {
                    let mut request = strategy.permission(id, raw.clone(), params)?;
                    request.connection = Some(Arc::clone(&self.owner_tag));
                    return Ok(Incoming::Permission(request));
                }
                return Ok(Incoming::UnsupportedRequest { id, raw });
            }
            if method == "session/update" {
                let event = strategy.update(params)?;
                let session = SessionId::parse(field(params, "sessionId")?)?;
                let origin = if self
                    .pending
                    .values()
                    .any(|p| matches!(&p.kind,RequestKind::Resume(s) if *s==session))
                {
                    UpdateOrigin::DuringResume
                } else {
                    UpdateOrigin::Live
                };
                if let SessionEvent::State { session, .. } = &event {
                    for pending in self.pending.values_mut() {
                        if matches!(&pending.kind,RequestKind::Resume(s) if s==session) {
                            pending.snapshot = true;
                        }
                    }
                }
                return Ok(Incoming::Update { event, origin, raw });
            }
            return Ok(Incoming::UnknownNotification { raw });
        }
        self.route_response(raw)
    }
    fn route_response(&mut self, raw: RawMessage) -> Result<Incoming, AcpError> {
        let fields = raw.fields();
        let id = RpcId::parse(fields.get("id").ok_or(AcpError::InvalidMessage)?)?;
        let pending = self
            .pending
            .remove(&id)
            .ok_or(AcpError::UnmatchedResponse)?;
        if fields.contains_key("result") == fields.contains_key("error") {
            return Err(AcpError::InvalidMessage);
        }
        if let Some(error) = fields.get("error") {
            object(error)?;
            let code = field(error, "code")?
                .as_i64()
                .ok_or(AcpError::InvalidMessage)?;
            field(error, "message")?
                .as_str()
                .ok_or(AcpError::InvalidMessage)?;
            if matches!(pending.kind, RequestKind::Initialize(_)) {
                self.poison();
            }
            return Ok(Incoming::Response {
                id,
                response: Response::RemoteError { code },
                raw,
            });
        }
        let result = fields.get("result").ok_or(AcpError::InvalidMessage)?;
        object(result)?;
        let response = match pending.kind {
            RequestKind::Initialize(offer) => {
                let strategy = Strategy::select(offer, raw.clone(), result)?;
                self.strategy = Some(strategy.clone());
                self.phase = Phase::Ready;
                Response::Initialized(strategy)
            }
            RequestKind::NewSession => {
                Response::NewSession(SessionId::parse(field(result, "sessionId")?)?)
            }
            RequestKind::Prompt(_) => Response::Prompt(
                self.strategy
                    .as_ref()
                    .ok_or(AcpError::NotInitialized)?
                    .prompt_result(result)?,
            ),
            RequestKind::Resume(session) => {
                if self
                    .strategy
                    .as_ref()
                    .is_some_and(|s| s.version() == Version::V2)
                    && !pending.snapshot
                {
                    return Err(AcpError::MissingSnapshot);
                }
                Response::Resumed { session }
            }
            RequestKind::Close(session) => Response::Closed { session },
        };
        Ok(Incoming::Response { id, response, raw })
    }
    /// Stops new work immediately, clears correlation and closes stdin within one absolute budget.
    /// # Errors
    /// A timed-out close remains closed and requires the process owner to reclaim its resources.
    pub async fn shutdown(&mut self, deadline: Instant) -> Result<(), AcpError> {
        self.poison();
        match timeout_at(deadline, self.writer.shutdown()).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(AcpError::Io),
            Err(_) => Err(AcpError::Deadline),
        }
    }
    fn require_ready(&self) -> Result<(), AcpError> {
        if self.phase == Phase::Ready {
            Ok(())
        } else {
            Err(if self.is_closed() {
                AcpError::Closed
            } else {
                AcpError::NotInitialized
            })
        }
    }
    fn poison(&mut self) {
        self.phase = Phase::Closed;
        self.pending.clear();
        self.server_pending.clear();
        self.input.clear();
    }
}

/// The serialization allocation itself obeys the byte limit, before any peer write.
struct FrameBuffer(Vec<u8>);
impl std::io::Write for FrameBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_FRAME {
            return Err(std::io::Error::other("ACP frame byte budget exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
