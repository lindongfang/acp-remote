//! Node-owned input orchestration. Trusted ports are composition boundaries, never client DTOs.
use crate::domain::{
    ActorContext, EntityId,
    activity::{ForegroundOutcome, SessionActivity, WorkActivity, WorkToken},
    native::RequestEpoch,
    product_commands::{
        InputReceipt, InputResult, InputTarget, ProductCommandError as Error, ProductCommandFact,
        input_intent,
    },
    requests::{ProductRequestIdentity, RequestIntent, RequestTime, VerifiedProductRequest},
};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    sync::{Mutex as AsyncMutex, Notify, OwnedMutexGuard},
    time::{Duration, Instant, timeout_at},
};

/// Shared node-runtime fault latch. It can only be closed, never reset by a request or query.
#[derive(Clone)]
pub struct FaultSignal(pub(crate) Arc<AtomicBool>);
impl FaultSignal {
    pub fn fault(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    #[must_use]
    pub fn faulted(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
/// Minted only by this service after its trusted authority has rechecked the opaque actor.
pub struct CurrentActor(ActorContext);
impl std::ops::Deref for CurrentActor {
    type Target = ActorContext;
    fn deref(&self) -> &ActorContext {
        &self.0
    }
}
/// Only node-owned tickets/permits can establish a zero-send command association.
pub struct CommandOwnership(ProductCommandFact, Option<EntityId>);

/// Persistence port. Public callers cannot manufacture `CurrentActor` or `CommandOwnership`.
/// The SQLite adapter implements this inward-facing contract without being imported here.
pub trait ProductRepository: Clone + Send + Sync {
    fn fault_signal(&self) -> FaultSignal;
    fn fault(&self) {
        self.fault_signal().fault();
    }
    fn faulted(&self) -> bool {
        self.fault_signal().faulted()
    }
    fn control(&self) -> impl Future<Output = Result<(RequestEpoch, u64), Error>> + Send;
    fn mark_interrupted(
        &self,
        time: &impl TimeSource,
    ) -> impl Future<Output = Result<u64, Error>> + Send;
    fn checkpoint(
        &self,
        actor: &CurrentActor,
        epoch: RequestEpoch,
        time: &impl TimeSource,
        deadline: Instant,
    ) -> impl Future<Output = Result<RequestTime, Error>> + Send;
    #[allow(
        clippy::too_many_arguments,
        reason = "Independent admission coordinates remain explicit at the port"
    )]
    fn admit(
        &self,
        actor: &CurrentActor,
        verified: &VerifiedProductRequest,
        target: &InputTarget,
        available: bool,
        time: &impl TimeSource,
        epoch: RequestEpoch,
        deadline: Instant,
    ) -> impl Future<Output = Result<(ProductCommandFact, bool), Error>> + Send;
    #[allow(
        clippy::too_many_arguments,
        reason = "Single-claim coordinates remain explicit at the port"
    )]
    fn claim(
        &self,
        actor: &CurrentActor,
        verified: &VerifiedProductRequest,
        target: &InputTarget,
        fact: &ProductCommandFact,
        claim: EntityId,
        time: &impl TimeSource,
        epoch: RequestEpoch,
        deadline: Instant,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
    fn final_fence(
        &self,
        actor: &CurrentActor,
        verified: &VerifiedProductRequest,
        target: &InputTarget,
        time: &impl TimeSource,
        epoch: RequestEpoch,
        deadline: Instant,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn fail_unsent(
        &self,
        ownership: &CommandOwnership,
        result: InputResult,
        time: &impl TimeSource,
        deadline: Instant,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn report(
        &self,
        receipt: &InputReceipt,
        result: InputResult,
        time: &impl TimeSource,
        deadline: Instant,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn insertion(
        &self,
        receipt: &InputReceipt,
        deadline: Instant,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn get(
        &self,
        actor: &CurrentActor,
        command: EntityId,
        deadline: Instant,
    ) -> impl Future<Output = Result<ProductCommandFact, Error>> + Send;
}
impl CurrentActor {
    #[must_use]
    pub const fn context(&self) -> ActorContext {
        self.0
    }
}
impl CommandOwnership {
    #[must_use]
    pub const fn fact(&self) -> &ProductCommandFact {
        &self.0
    }
    #[must_use]
    pub const fn claim(&self) -> Option<EntityId> {
        self.1
    }
}
fn new_id() -> Result<EntityId, Error> {
    crate::random::bytes()
        .map(EntityId::from_bytes)
        .map_err(|_| Error::RandomUnavailable)
}

/// Implementations own a `RequestClock` and sample server wall/monotonic time afresh per call.
/// Returning client-supplied timestamps is not a valid implementation.
pub trait TimeSource: Send + Sync {
    /// # Errors
    /// Rejects missing, regressing or unsafe server time.
    fn sample(&self) -> Result<RequestTime, Error>;
}
/// Trusted cryptographic adapter; implementations fix the node key, purpose and strict decoder.
pub trait RequestIdentityPort: Send + Sync {
    /// # Errors
    /// Rejects invalid signatures, purposes, claims, intents and request deadlines.
    fn verify(
        &self,
        compact: &str,
        epoch: RequestEpoch,
        intent: &RequestIntent,
        time: RequestTime,
    ) -> Result<VerifiedProductRequest, Error>;
    /// # Errors
    /// Rejects unsafe signing times and signer failures; the service commits the checkpoint separately.
    fn issue(
        &self,
        epoch: RequestEpoch,
        request: EntityId,
        intent: &RequestIntent,
        time: RequestTime,
    ) -> Result<String, Error>;
}
/// Implement only at a verified authentication/resource boundary. Actor MUST be opaque there.
/// These callbacks are not RPCs and must never turn caller `ActorContext`/boolean fields into grants.
pub trait InputAuthority: Send + Sync {
    type Actor: Send + Sync;
    /// # Errors
    /// Rejects an actor that cannot be resolved to the authentication gate.
    fn actor_gate(&self, actor: &Self::Actor) -> Result<Arc<AsyncMutex<()>>, Error>;
    fn require_current(
        &self,
        actor: &Self::Actor,
        target: EntityId,
    ) -> impl Future<Output = Result<ActorContext, Error>> + Send;
    fn owner(
        &self,
        target: EntityId,
    ) -> impl Future<Output = Result<Arc<AsyncMutex<InputOwner>>, Error>> + Send;
    /// Drains already-observed native events while the target owner is held.
    /// # Errors
    /// Rejects a source whose connection, observation stream or replay admission is no longer valid.
    fn refresh_owner(
        &self,
        owner: &mut InputOwner,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

pub struct InputOwner {
    target: InputTarget,
    activity: SessionActivity,
    replay_admitted: bool,
    owner_nonce: EntityId,
}
impl InputOwner {
    /// Called by a trusted native-source adapter after resource and replay validation, not a client.
    /// # Errors
    /// Rejects unavailable randomness for the target owner identity.
    pub fn from_verified_source(target: InputTarget, replay_admitted: bool) -> Result<Self, Error> {
        let activity = SessionActivity::new(target.native.clone(), target.epoch);
        Ok(Self {
            target,
            activity,
            replay_admitted,
            owner_nonce: new_id()?,
        })
    }
    /// Native-source observation hook. This does not grant management authorization.
    /// # Errors
    /// Rejects observations from an invalid or replaced resource owner.
    pub fn observe_foreground(&mut self, state: WorkActivity) -> Result<(), Error> {
        self.activity
            .observe_foreground(self.target.epoch, state)
            .map_err(|_| Error::InvalidTarget)
    }
    pub fn invalidate(&mut self) {
        let _ = self.activity.interrupt(self.target.epoch);
        self.replay_admitted = false;
    }
    #[must_use]
    pub const fn target(&self) -> &InputTarget {
        &self.target
    }
    fn available(&self) -> bool {
        self.replay_admitted && self.activity.can_accept_input()
    }
    fn owns(&self, p: &PendingInput) -> bool {
        self.owner_nonce == p.owner_nonce
            && self.target == p.target
            && self.replay_admitted
            && self.activity.foreground() == WorkActivity::Idle
            && self.activity.current_work() == Some(&p.work)
            && self.activity.input().is_some_and(|i| {
                i.request == p.proof.identity().request
                    && i.run == p.fact.run
                    && i.awaiting_insertion
            })
    }
}

#[derive(Clone, Default)]
/// Node-work cancellation. Browser disconnect must not cancel an already node-owned input/task.
pub struct Cancellation {
    inner: Arc<CancelState>,
}
#[derive(Default)]
struct CancelState {
    cancelled: AtomicBool,
    notify: Notify,
}
impl Cancellation {
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        self.inner.notify.notify_waiters();
    }
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }
    async fn cancelled(&self) {
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}
async fn wait<T>(
    deadline: Instant,
    cancel: &Cancellation,
    future: impl Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    tokio::select! { biased; ()=cancel.cancelled()=>Err(Error::Cancelled), result=timeout_at(deadline,future)=>result.map_err(|_|Error::Cancelled)? }
}
fn operation_deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn phase(overall: Instant, seconds: u64) -> Instant {
    overall.min(Instant::now() + Duration::from_secs(seconds))
}
fn request_expiry(
    i: &ProductRequestIdentity,
    t: RequestTime,
    sampled_at: Instant,
) -> Result<Instant, Error> {
    let remaining = i
        .expires_at
        .checked_sub(t.millis())
        .filter(|n| *n > 0)
        .ok_or(Error::Expired)?;
    sampled_at
        .checked_add(Duration::from_millis(remaining))
        .ok_or(Error::ClockUnreliable)
}
struct OperationGuard {
    store: FaultSignal,
    armed: bool,
}
impl OperationGuard {
    const fn new(store: FaultSignal) -> Self {
        Self { store, armed: true }
    }
    const fn finish(&mut self) {
        self.armed = false;
    }
}
impl Drop for OperationGuard {
    fn drop(&mut self) {
        if self.armed {
            self.store.fault();
        }
    }
}

const OBJECT_BYTES: usize = 16 * 1024 * 1024;
const TOTAL_BYTES: usize = 128 * 1024 * 1024;
const OBJECTS: usize = 64;
#[derive(Default)]
struct BudgetState {
    bytes: usize,
    objects: usize,
    cells: Vec<std::sync::Weak<InputCell>>,
}
#[derive(Clone, Default)]
pub struct MemoryInputs {
    state: Arc<Mutex<BudgetState>>,
}
struct CellState {
    body: Option<Vec<u8>>,
    borrowed: bool,
    retired: bool,
    accounted: bool,
    scope: Option<(EntityId, i64, EntityId)>,
    deadline: Instant,
}
struct InputCell {
    state: Mutex<CellState>,
    budget: Arc<Mutex<BudgetState>>,
    bytes: usize,
    length: usize,
    cancel: Cancellation,
}
impl InputCell {
    fn deadline(&self) -> Result<Instant, Error> {
        self.state
            .lock()
            .map(|state| state.deadline)
            .map_err(|_| Error::InvalidInput)
    }
    fn tighten_deadline(&self, deadline: Instant) -> Result<Instant, Error> {
        let mut state = self.state.lock().map_err(|_| Error::InvalidInput)?;
        if state.retired {
            return Err(if Instant::now() >= state.deadline {
                Error::InputExpired
            } else {
                Error::InvalidInput
            });
        }
        state.deadline = state.deadline.min(deadline);
        let deadline = state.deadline;
        drop(state);
        if Instant::now() >= deadline {
            self.retire();
            return Err(Error::InputExpired);
        }
        Ok(deadline)
    }
    fn retire(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.retired = true;
        self.cancel.cancel();
        if state.borrowed {
            return;
        }
        state.body = None;
        if state.accounted {
            if let Ok(mut budget) = self.budget.lock() {
                budget.bytes = budget.bytes.saturating_sub(self.bytes);
                budget.objects = budget.objects.saturating_sub(1);
            }
            state.accounted = false;
        }
    }
    fn bind(&self, actor: ActorContext, target: EntityId) -> Result<(), Error> {
        let mut state = self.state.lock().map_err(|_| Error::InvalidInput)?;
        let scope = (actor.client_id, actor.auth_generation, target);
        if state.retired || state.scope.is_some_and(|s| s != scope) {
            return Err(Error::InvalidInput);
        }
        state.scope = Some(scope);
        drop(state);
        Ok(())
    }
}
impl Drop for InputCell {
    fn drop(&mut self) {
        self.retire();
    }
}
/// Structure checking is trusted adapter work. This stage does not implement an ACP content schema.
pub trait InputStructureValidator {
    /// # Errors
    /// Rejects incomplete or unsupported structural input without retaining its content.
    fn validate(&self, body: &[u8]) -> Result<(), Error>;
}
pub struct InputUpload {
    cell: Arc<InputCell>,
    live: bool,
}
pub struct TemporaryInput {
    cell: Arc<InputCell>,
    live: bool,
}
struct BorrowedInput {
    body: Vec<u8>,
    cell: Arc<InputCell>,
}
impl Drop for BorrowedInput {
    fn drop(&mut self) {
        self.body.clear();
        self.body.shrink_to_fit();
        if let Ok(mut state) = self.cell.state.lock() {
            state.borrowed = false;
        }
        self.cell.retire();
    }
}
impl Drop for InputUpload {
    fn drop(&mut self) {
        if self.live {
            self.cell.retire();
        }
    }
}
impl Drop for TemporaryInput {
    fn drop(&mut self) {
        if self.live {
            self.cell.retire();
        }
    }
}
impl fmt::Debug for TemporaryInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TemporaryInput")
            .field("bytes", &self.cell.length)
            .finish_non_exhaustive()
    }
}
impl MemoryInputs {
    /// Reserves bytes before upload. Gateway assembly should use the scoped upload entry.
    /// Extra adapter structural/reference storage must be included; no unaccounted caches are allowed.
    /// # Errors
    /// Rejects expired reservations, arithmetic overflow and body/object/node budget exhaustion.
    pub fn reserve(
        &self,
        length: usize,
        extra_retained_bytes: usize,
        deadline: Instant,
    ) -> Result<InputUpload, Error> {
        self.expire();
        let bytes = length
            .checked_add(extra_retained_bytes)
            .ok_or(Error::InvalidInput)?;
        let mut budget = self.state.lock().map_err(|_| Error::InvalidInput)?;
        if bytes > OBJECT_BYTES
            || budget.objects >= OBJECTS
            || budget
                .bytes
                .checked_add(bytes)
                .is_none_or(|v| v > TOTAL_BYTES)
            || deadline <= Instant::now()
        {
            return Err(Error::InvalidInput);
        }
        budget
            .cells
            .retain(|c| c.upgrade().is_some_and(|cell| !cell.cancel.is_cancelled()));
        budget.bytes += bytes;
        budget.objects += 1;
        let mut body = Vec::new();
        if body.try_reserve_exact(length).is_err() {
            budget.bytes -= bytes;
            budget.objects -= 1;
            return Err(Error::InvalidInput);
        }
        let cell = Arc::new(InputCell {
            state: Mutex::new(CellState {
                body: Some(body),
                borrowed: false,
                retired: false,
                accounted: true,
                scope: None,
                deadline: deadline.min(Instant::now() + Duration::from_secs(3600)),
            }),
            budget: self.state.clone(),
            bytes,
            length,
            cancel: Cancellation::default(),
        });
        budget.cells.push(Arc::downgrade(&cell));
        drop(budget);
        Ok(InputUpload { cell, live: true })
    }
    fn cells(&self) -> Vec<Arc<InputCell>> {
        self.state.lock().map_or_else(
            |_| Vec::new(),
            |s| {
                s.cells
                    .iter()
                    .filter_map(std::sync::Weak::upgrade)
                    .collect()
            },
        )
    }
    pub fn expire(&self) {
        for cell in self.cells() {
            if cell
                .deadline()
                .map_or(true, |deadline| Instant::now() >= deadline)
            {
                cell.retire();
            }
        }
    }
    pub fn revoke(&self, actor: EntityId) {
        for cell in self.cells() {
            let matches = cell
                .state
                .lock()
                .is_ok_and(|s| s.scope.is_some_and(|scope| scope.0 == actor));
            if matches {
                cell.retire();
            }
        }
    }
    pub fn stop(&self) {
        for cell in self.cells() {
            cell.retire();
        }
    }
    #[must_use]
    pub fn retained(&self) -> (usize, usize) {
        self.expire();
        self.state
            .lock()
            .map_or((TOTAL_BYTES, OBJECTS), |s| (s.bytes, s.objects))
    }
    /// The returned owner must be retained and shut down with the node. It owns its one sweeper task.
    #[must_use]
    pub fn supervise(&self) -> MemoryInputSupervisor {
        let stop = Cancellation::default();
        let task_stop = stop.clone();
        let inputs = self.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {()=task_stop.cancelled()=>{inputs.stop();break;},()=tokio::time::sleep(Duration::from_millis(100))=>inputs.expire()}
            }
        });
        MemoryInputSupervisor {
            stop,
            task: Some(task),
            inputs: self.clone(),
        }
    }
}
pub struct MemoryInputSupervisor {
    stop: Cancellation,
    task: Option<tokio::task::JoinHandle<()>>,
    inputs: MemoryInputs,
}
impl MemoryInputSupervisor {
    pub async fn shutdown(mut self) {
        self.stop.cancel();
        self.inputs.stop();
        if let Some(mut task) = self.task.take()
            && timeout_at(Instant::now() + Duration::from_secs(5), &mut task)
                .await
                .is_err()
        {
            task.abort();
        }
    }
}
impl Drop for MemoryInputSupervisor {
    fn drop(&mut self) {
        self.stop.cancel();
        self.inputs.stop();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
impl InputUpload {
    /// # Errors
    /// Rejects expired, revoked or oversized upload data.
    pub fn append(&mut self, chunk: &[u8]) -> Result<(), Error> {
        if Instant::now() >= self.cell.deadline()? {
            self.cell.retire();
            return Err(Error::InputExpired);
        }
        let mut state = self.cell.state.lock().map_err(|_| Error::InvalidInput)?;
        if state.retired {
            return Err(Error::InvalidInput);
        }
        let body = state.body.as_mut().ok_or(Error::InvalidInput)?;
        if body
            .len()
            .checked_add(chunk.len())
            .is_none_or(|n| n > self.cell.length)
        {
            drop(state);
            self.cell.retire();
            return Err(Error::InvalidInput);
        }
        body.extend_from_slice(chunk);
        drop(state);
        Ok(())
    }
    /// # Errors
    /// Rejects incomplete, invalid or expired structural input.
    pub fn finish(
        mut self,
        validator: &impl InputStructureValidator,
    ) -> Result<TemporaryInput, Error> {
        if Instant::now() >= self.cell.deadline()? {
            return Err(Error::InputExpired);
        }
        {
            let state = self.cell.state.lock().map_err(|_| Error::InvalidInput)?;
            let body = state.body.as_ref().ok_or(Error::InvalidInput)?;
            if state.retired || body.len() != self.cell.length {
                return Err(Error::InvalidInput);
            }
            validator.validate(body)?;
            drop(state);
        }
        self.live = false;
        Ok(TemporaryInput {
            cell: self.cell.clone(),
            live: true,
        })
    }
}
impl TemporaryInput {
    /// # Errors
    /// Rejects expired, revoked or already consumed input.
    pub fn digest(&self) -> Result<[u8; 32], Error> {
        let state = self.cell.state.lock().map_err(|_| Error::InvalidInput)?;
        if Instant::now() >= state.deadline {
            return Err(Error::InputExpired);
        }
        if state.retired {
            return Err(Error::InvalidInput);
        }
        Ok(Sha256::digest(state.body.as_ref().ok_or(Error::InvalidInput)?).into())
    }
    fn borrow_once(mut self) -> Result<BorrowedInput, Error> {
        let body = {
            let mut state = self.cell.state.lock().map_err(|_| Error::InvalidInput)?;
            if Instant::now() >= state.deadline {
                return Err(Error::InputExpired);
            }
            if state.retired {
                return Err(Error::InvalidInput);
            }
            let body = state.body.take().ok_or(Error::InvalidInput)?;
            state.borrowed = true;
            body
        };
        self.live = false;
        Ok(BorrowedInput {
            body,
            cell: self.cell.clone(),
        })
    }
}
struct PendingInput {
    proof: VerifiedProductRequest,
    actor: ActorContext,
    target: InputTarget,
    fact: ProductCommandFact,
    work: WorkToken,
    owner_nonce: EntityId,
    body: TemporaryInput,
}
pub struct InputTicket {
    pending: Option<Box<PendingInput>>,
    store: FaultSignal,
}
impl Drop for InputTicket {
    fn drop(&mut self) {
        if self.pending.is_some() {
            self.store.fault();
        }
    }
}
pub struct DispatchPermit {
    pending: Option<Box<PendingInput>>,
    claim: EntityId,
    store: FaultSignal,
}
impl Drop for DispatchPermit {
    fn drop(&mut self) {
        if self.pending.is_some() {
            self.store.fault();
        }
    }
}
pub enum InputAdmission {
    New {
        fact: ProductCommandFact,
        ticket: InputTicket,
    },
    Replay(ProductCommandFact),
}
impl InputAdmission {
    #[must_use]
    pub const fn fact(&self) -> &ProductCommandFact {
        match self {
            Self::New { fact, .. } | Self::Replay(fact) => fact,
        }
    }
}
pub struct ExecutionReceipt {
    inner: InputReceipt,
    work: WorkToken,
    settled: AtomicBool,
}
impl ExecutionReceipt {
    #[must_use]
    pub const fn command(&self) -> EntityId {
        self.inner.command
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeInputReport {
    TransportWritten,
    Inserted,
    Completed,
    Cancelled,
    Rejected,
    Unknown,
}

/// The supplied future must do its first write within this call. On Drop it must cancel every
/// deferred queue item; no detached task may perform a first write after this future is dropped.
///
/// Real adapters are NOT wired by this stage. Passing client callbacks is not supported.
pub trait InputTransport: Send + Sync {
    fn write_once(&self, body: &[u8]) -> impl Future<Output = Result<(), Error>> + Send;
}

pub struct ProductInputService<A, C, T, R> {
    store: R,
    authority: A,
    codec: C,
    time: T,
    epoch: RequestEpoch,
}
impl<A: InputAuthority, C: RequestIdentityPort, T: TimeSource, R: ProductRepository>
    ProductInputService<A, C, T, R>
{
    /// Startup only. All services/pools must be constructed before opening input admission.
    /// Assembly must share one store health handle across the node; this is not a fault-reset API.
    /// # Errors
    /// Rejects invalid startup control, unreliable time and failed conservative marking.
    pub async fn start(store: R, authority: A, codec: C, time: T) -> Result<Self, Error> {
        let (epoch, floor) = store.control().await?;
        if time.sample()?.millis() < floor {
            store.fault();
            return Err(Error::ClockUnreliable);
        }
        store.mark_interrupted(&time).await?;
        Ok(Self {
            store,
            authority,
            codec,
            time,
            epoch,
        })
    }
    #[must_use]
    pub fn faulted(&self) -> bool {
        self.store.faulted()
    }
    /// Authenticates the upload scope before reserving retained memory.
    /// # Errors
    /// Rejects stale actors, inaccessible targets, cancellation and memory budget exhaustion.
    pub async fn prepare_input(
        &self,
        actor: &A::Actor,
        target: EntityId,
        inputs: &MemoryInputs,
        length: usize,
        extra_retained_bytes: usize,
        cancel: &Cancellation,
    ) -> Result<InputUpload, Error> {
        let deadline = operation_deadline();
        let (current, _gate, _owner) = self.gates(actor, target, deadline, cancel).await?;
        let upload = inputs.reserve(
            length,
            extra_retained_bytes,
            Instant::now() + Duration::from_secs(3600),
        )?;
        upload.cell.bind(*current, target)?;
        Ok(upload)
    }
    async fn gates(
        &self,
        actor: &A::Actor,
        target: EntityId,
        deadline: Instant,
        cancel: &Cancellation,
    ) -> Result<
        (
            CurrentActor,
            OwnedMutexGuard<()>,
            OwnedMutexGuard<InputOwner>,
        ),
        Error,
    > {
        let gate = self.authority.actor_gate(actor)?;
        let gate = wait(phase(deadline, 5), cancel, async {
            Ok(gate.lock_owned().await)
        })
        .await?;
        let current = wait(
            deadline,
            cancel,
            self.authority.require_current(actor, target),
        )
        .await?;
        let owner = wait(phase(deadline, 5), cancel, self.authority.owner(target)).await?;
        let owner = wait(phase(deadline, 5), cancel, async {
            Ok(owner.lock_owned().await)
        })
        .await?;
        if owner.target.target != target {
            return Err(Error::InvalidTarget);
        }
        Ok((CurrentActor(current), gate, owner))
    }
    async fn fail_orphan(
        &self,
        pending: &PendingInput,
        claim: Option<EntityId>,
    ) -> Result<(), Error> {
        let deadline = Instant::now() + Duration::from_secs(5);
        self.store
            .fail_unsent(
                &CommandOwnership(pending.fact.clone(), claim),
                InputResult::NotSent,
                &self.time,
                deadline,
            )
            .await?;
        let owner = timeout_at(deadline, self.authority.owner(pending.target.target))
            .await
            .map_err(|_| Error::Cancelled)??;
        let mut owner = timeout_at(deadline, owner.lock_owned())
            .await
            .map_err(|_| Error::Cancelled)?;
        if owner.owner_nonce == pending.owner_nonce
            && owner.activity.current_work() == Some(&pending.work)
        {
            let _ = owner.activity.reject_input(&pending.work);
        }
        drop(owner);
        Ok(())
    }
    /// # Errors
    /// Rejects unavailable signing/checkpoint state, invalid intents and unsafe deadlines.
    pub async fn issue(
        &self,
        actor: &A::Actor,
        target: EntityId,
        content_digest: [u8; 32],
        cancel: &Cancellation,
    ) -> Result<String, Error> {
        let deadline = operation_deadline();
        let mut guard = OperationGuard::new(self.store.fault_signal());
        let result = async {
            let (current, _gate, owner) = self.gates(actor, target, deadline, cancel).await?;
            let time = wait(
                deadline,
                cancel,
                self.store
                    .checkpoint(&current, self.epoch, &self.time, deadline),
            )
            .await?;
            let intent = input_intent(current.client_id, &owner.target, content_digest)?;
            let compact = self.codec.issue(self.epoch, new_id()?, &intent, time)?;
            let final_time = self.time.sample()?;
            self.codec
                .verify(&compact, self.epoch, &intent, final_time)?;
            wait(
                deadline,
                cancel,
                self.authority.require_current(actor, target),
            )
            .await?;
            Ok(compact)
        }
        .await;
        guard.finish();
        result
    }
    /// # Errors
    /// Rejects stale authorization, conflicting or expired identities, busy targets and unconfirmed storage.
    pub async fn submit(
        &self,
        actor: &A::Actor,
        target: EntityId,
        compact: &str,
        body: TemporaryInput,
        cancel: &Cancellation,
    ) -> Result<InputAdmission, Error> {
        let deadline = operation_deadline();
        let mut guard = OperationGuard::new(self.store.fault_signal());
        let result = async {
            let (current, _gate, mut owner) = self.gates(actor, target, deadline, cancel).await?;
            body.cell.bind(*current, target)?;
            if Instant::now() >= body.cell.deadline()? {
                return Err(Error::InputExpired);
            }
            let time = wait(
                deadline,
                cancel,
                self.store
                    .checkpoint(&current, self.epoch, &self.time, deadline),
            )
            .await?;
            let intent = input_intent(current.client_id, &owner.target, body.digest()?)?;
            let proof = self.codec.verify(compact, self.epoch, &intent, time)?;
            // The anchor precedes sampling: a slow sample/checkpoint cannot extend body lifetime.
            let sampled_at = Instant::now();
            let fresh_time = self.time.sample()?;
            if let Err(error) =
                self.codec
                    .verify_identity_time(proof.identity(), self.epoch, fresh_time)
            {
                if error == Error::Expired {
                    // Preserve a newly observed expiry floor as well as rejecting this body.
                    wait(
                        deadline,
                        cancel,
                        self.store
                            .checkpoint(&current, self.epoch, &self.time, deadline),
                    )
                    .await?;
                }
                return Err(error);
            }
            let signed_expiry = request_expiry(proof.identity(), fresh_time, sampled_at)?;
            let deadline = deadline.min(body.cell.tighten_deadline(signed_expiry)?);
            if wait(deadline, cancel, self.authority.refresh_owner(&mut owner))
                .await
                .is_err()
            {
                owner.invalidate();
            }
            let (fact, replay) = wait(
                deadline,
                cancel,
                self.store.admit(
                    &current,
                    &proof,
                    &owner.target,
                    owner.available(),
                    &self.time,
                    self.epoch,
                    deadline,
                ),
            )
            .await?;
            if replay {
                return Ok(InputAdmission::Replay(fact));
            }
            let work = owner
                .activity
                .reserve_input(proof.identity().request, fact.run)
                .map_err(|_| {
                    self.store.fault();
                    Error::StorageUnavailable
                })?;
            let pending = PendingInput {
                proof,
                actor: *current,
                target: owner.target.clone(),
                fact: fact.clone(),
                work,
                owner_nonce: owner.owner_nonce,
                body,
            };
            Ok(InputAdmission::New {
                fact,
                ticket: InputTicket {
                    pending: Some(Box::new(pending)),
                    store: self.store.fault_signal(),
                },
            })
        }
        .await;
        guard.finish();
        result
    }
    /// # Errors
    /// Rejects stale tickets, changed native activity, expiry and unconfirmed claim commits.
    pub async fn claim(
        &self,
        actor: &A::Actor,
        mut ticket: InputTicket,
        cancel: &Cancellation,
    ) -> Result<DispatchPermit, Error> {
        let deadline = operation_deadline();
        let mut guard = OperationGuard::new(self.store.fault_signal());
        let pending = ticket.pending.take().ok_or(Error::InvalidTicket)?;
        let mut resolved = false;
        let result = async {
            let (current, _gate, mut owner) = match self
                .gates(actor, pending.target.target, deadline, cancel)
                .await
            {
                Ok(guards) => guards,
                Err(error) => {
                    self.fail_orphan(&pending, None).await?;
                    resolved = true;
                    return Err(error);
                }
            };
            wait(deadline, cancel, self.authority.refresh_owner(&mut owner)).await?;
            if current.client_id != pending.actor.client_id
                || current.auth_generation != pending.actor.auth_generation
            {
                return Err(Error::InvalidActor);
            }
            if !owner.owns(&pending) {
                wait(
                    deadline,
                    cancel,
                    self.store.fail_unsent(
                        &CommandOwnership(pending.fact.clone(), None),
                        InputResult::NotSent,
                        &self.time,
                        deadline,
                    ),
                )
                .await?;
                if owner.activity.current_work() == Some(&pending.work) {
                    let _ = owner.activity.reject_input(&pending.work);
                }
                resolved = true;
                return Err(Error::Busy);
            }
            let claim = new_id()?;
            let claimed = wait(
                deadline,
                cancel,
                self.store.claim(
                    &current,
                    &pending.proof,
                    &pending.target,
                    &pending.fact,
                    claim,
                    &self.time,
                    self.epoch,
                    deadline,
                ),
            )
            .await;
            match claimed {
                Ok(true) => Ok(DispatchPermit {
                    pending: Some(pending),
                    claim,
                    store: self.store.fault_signal(),
                }),
                Ok(false) => Err(Error::InvalidTicket),
                Err(e) => {
                    if e == Error::Expired {
                        let _ = owner.activity.reject_input(&pending.work);
                        resolved = true;
                    }
                    Err(e)
                }
            }
        }
        .await;
        if result.is_err() && !resolved {
            self.store.fault();
        }
        guard.finish();
        result
    }
    /// # Errors
    /// Rejects stale final fences; possible writes remain unknown on transport failure or cancellation.
    #[allow(
        clippy::too_many_lines,
        reason = "The bounded first-write sequence retains both fences through zero-send or unknown cleanup"
    )]
    pub async fn dispatch(
        &self,
        actor: &A::Actor,
        mut permit: DispatchPermit,
        transport: &impl InputTransport,
        cancel: &Cancellation,
    ) -> Result<ExecutionReceipt, Error> {
        let deadline = operation_deadline();
        let mut guard = OperationGuard::new(self.store.fault_signal());
        let pending = permit.pending.take().ok_or(Error::InvalidTicket)?;
        let claim = permit.claim;
        let mut resolved = false;
        let result = async {
            let(current,gate,mut owner)=match self.gates(actor,pending.target.target,deadline,cancel).await {
                Ok(guards)=>guards,
                Err(error)=>{self.fail_orphan(&pending,Some(claim)).await?;resolved=true;return Err(error);}
            };
            let qualification = async {
                if current.client_id != pending.actor.client_id
                    || current.auth_generation != pending.actor.auth_generation
                {
                    return Err(Error::InvalidActor);
                }
                if !owner.owns(&pending) {
                    return Err(Error::InvalidTarget);
                }
                wait(
                    deadline,
                    cancel,
                    self.store.final_fence(
                        &current,
                        &pending.proof,
                        &pending.target,
                        &self.time,
                        self.epoch,
                        deadline,
                    ),
                )
                .await?;
                let final_actor=wait(
                    deadline,
                    cancel,
                    self.authority.require_current(actor, pending.target.target),
                )
                .await?;
                if final_actor.client_id!=pending.actor.client_id || final_actor.auth_generation!=pending.actor.auth_generation {return Err(Error::InvalidActor);}
                wait(deadline,cancel,self.authority.refresh_owner(&mut owner)).await?;
                if !owner.owns(&pending) {return Err(Error::InvalidTarget);}
                let sampled_at=Instant::now();
                let now=self.time.sample()?;
                self.codec.verify_identity_time(pending.proof.identity(),self.epoch,now)?;
                let signature_deadline=request_expiry(pending.proof.identity(),now,sampled_at)?;
                let input_deadline=pending.body.cell.deadline()?;
                Ok((signature_deadline,input_deadline))
            }
            .await;
            let (signature_deadline,input_deadline) = match qualification {
                Ok(deadlines) => deadlines,
                Err(e) => {
                    let status = if e == Error::Expired {
                        InputResult::Expired
                    } else {
                        InputResult::NotSent
                    };
                    // Cleanup is node-owned even when the original request cancellation fired.
                    self.store
                         .fail_unsent(&CommandOwnership(pending.fact.clone(),Some(claim)),
                            status,
                            &self.time,
                            Instant::now() + Duration::from_secs(5),
                        )
                        .await?;
                    if owner.activity.current_work() == Some(&pending.work) {
                        let _ = owner.activity.reject_input(&pending.work);
                    }
                    resolved=true;
                    return Err(e);
                }
            };
            let transport_budget=phase(deadline,2);
            let write_deadline=transport_budget.min(signature_deadline).min(input_deadline);
            let receipt = ExecutionReceipt {
                inner: InputReceipt {
                    command: pending.fact.command,
                    run: pending.fact.run,
                    claim,
                    target: pending.target.clone(),
                    owner_nonce: pending.owner_nonce,
                },
                work: pending.work.clone(),
                settled: AtomicBool::new(false),
            };
            // The body lease lives until this one future is dropped/completed; never retained for replay.
            let borrowed=match pending.body.borrow_once() {
                Ok(body)=>body,
                Err(error)=>{
                    let error=self.unsent_error(pending.proof.identity(),error);
                    self.store.fail_unsent(&CommandOwnership(pending.fact.clone(),Some(claim)),if error==Error::Expired {InputResult::Expired}else {InputResult::NotSent},&self.time,Instant::now()+Duration::from_secs(5)).await?;
                    if owner.activity.current_work()==Some(&pending.work){let _=owner.activity.reject_input(&pending.work);}
                    resolved=true;return Err(error);
                }
            };
            // Last consumption fence, after every await/sample/borrow and before write_once can run.
            let checked_at=Instant::now();
            let signature_due=checked_at>=signature_deadline;
            let input_due=checked_at>=input_deadline;
            let budget_due=checked_at>=deadline || checked_at>=transport_budget;
            let cancelled=cancel.is_cancelled() || borrowed.cell.cancel.is_cancelled();
            let refusal=if input_due {Some(Error::InputExpired)}else if budget_due || cancelled || signature_due {Some(Error::Cancelled)}else {None};
            // A conservative monotonic cut-off does not, by itself, prove signed Unix expiry.
            let refusal=if signature_due {Some(self.unsent_error(pending.proof.identity(),refusal.unwrap_or(Error::Cancelled)))}else {refusal};
            let faulted=self.store.faulted();
            let refusal=if faulted {Some(Error::StorageUnavailable)}else {refusal};
            if let Some(error)=refusal {
                drop(borrowed);
                self.store.fail_unsent(&CommandOwnership(pending.fact.clone(),Some(claim)),if error==Error::Expired {InputResult::Expired}else {InputResult::NotSent},&self.time,Instant::now()+Duration::from_secs(5)).await?;
                if owner.activity.current_work()==Some(&pending.work) {let _=owner.activity.reject_input(&pending.work);}
                resolved=true;
                return Err(error);
            }
            let sent=tokio::select! {biased; ()=borrowed.cell.cancel.cancelled()=>Err(Error::Cancelled), result=wait(write_deadline,cancel,transport.write_once(&borrowed.body))=>result};
            drop(borrowed);
            if let Err(error)=sent {
                let _ = owner.activity.interrupt(pending.target.epoch);
                drop(owner);
                drop(gate);
                let saved = self
                    .store
                    .report(
                        &receipt.inner,
                        InputResult::Unknown,
                        &self.time,
                        Instant::now() + Duration::from_secs(5),
                    )
                    .await;
                if saved.is_err() {
                    self.store.fault();
                }
                receipt.settled.store(true, Ordering::SeqCst);
                resolved=true;
                return Err(error);
            }
            Ok(receipt)
        }
        .await;
        if result.is_err() && !resolved {
            self.store.fault();
        }
        guard.finish();
        result
    }
    fn unsent_error(&self, identity: &ProductRequestIdentity, fallback: Error) -> Error {
        match self
            .time
            .sample()
            .and_then(|time| self.codec.verify_identity_time(identity, self.epoch, time))
        {
            Ok(()) => fallback,
            Err(error) => error,
        }
    }
    /// Native owner evidence only. A browser cannot obtain an `ExecutionReceipt` by command lookup.
    /// # Errors
    /// Rejects stale native evidence, conflicting prior results and unconfirmed result commits.
    pub async fn report(
        &self,
        receipt: &ExecutionReceipt,
        report: NativeInputReport,
        cancel: &Cancellation,
    ) -> Result<(), Error> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut guard = OperationGuard::new(self.store.fault_signal());
        let result = async {
            let owner = wait(
                deadline,
                cancel,
                self.authority.owner(receipt.inner.target.target),
            )
            .await?;
            let mut owner = wait(deadline, cancel, async { Ok(owner.lock_owned().await) }).await?;
            if owner.owner_nonce != receipt.inner.owner_nonce
                || owner.target != receipt.inner.target
            {
                return Err(Error::InvalidTicket);
            }
            if !receipt.settled.load(Ordering::SeqCst)
                && owner.activity.current_work() != Some(&receipt.work)
            {
                return Err(Error::InvalidTicket);
            }
            match report {
                NativeInputReport::TransportWritten => Ok(()),
                NativeInputReport::Inserted => {
                    wait(
                        deadline,
                        cancel,
                        self.store.insertion(&receipt.inner, deadline),
                    )
                    .await?;
                    owner
                        .activity
                        .confirm_insertion(&receipt.work)
                        .map_err(|_| Error::InvalidTicket)
                }
                other => {
                    let outcome = match other {
                        NativeInputReport::Completed => InputResult::Completed,
                        NativeInputReport::Cancelled => InputResult::Cancelled,
                        NativeInputReport::Rejected => InputResult::Rejected,
                        _ => InputResult::Unknown,
                    };
                    // Preserve native facts even when SQL fails, while retaining the separate fault marker.
                    let saved = wait(
                        deadline,
                        cancel,
                        self.store
                            .report(&receipt.inner, outcome, &self.time, deadline),
                    )
                    .await;
                    if !receipt.settled.swap(true, Ordering::SeqCst) {
                        match other {
                            NativeInputReport::Completed | NativeInputReport::Cancelled => {
                                let _ = owner.activity.finish_work(
                                    &receipt.work,
                                    if other == NativeInputReport::Cancelled {
                                        ForegroundOutcome::Cancelled
                                    } else {
                                        ForegroundOutcome::Completed
                                    },
                                );
                            }
                            NativeInputReport::Rejected => {
                                let _ = owner.activity.reject_input(&receipt.work);
                            }
                            _ => {
                                let _ = owner.activity.interrupt(receipt.inner.target.epoch);
                            }
                        }
                    }
                    drop(owner);
                    if saved.is_err() {
                        self.store.fault();
                    }
                    saved
                }
            }
        }
        .await;
        guard.finish();
        result
    }
    /// # Errors
    /// Rejects stale actors, inaccessible targets and unavailable command metadata.
    pub async fn command(
        &self,
        actor: &A::Actor,
        target: EntityId,
        command: EntityId,
        cancel: &Cancellation,
    ) -> Result<ProductCommandFact, Error> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let gate = self.authority.actor_gate(actor)?;
        let _gate = wait(deadline, cancel, async { Ok(gate.lock_owned().await) }).await?;
        let current = wait(
            deadline,
            cancel,
            self.authority.require_current(actor, target),
        )
        .await?;
        let fact = wait(
            deadline,
            cancel,
            self.store.get(&CurrentActor(current), command, deadline),
        )
        .await?;
        if fact.target != target {
            return Err(Error::InvalidTarget);
        }
        Ok(fact)
    }
}
trait IdentityTime {
    fn verify_identity_time(
        &self,
        i: &ProductRequestIdentity,
        epoch: RequestEpoch,
        now: RequestTime,
    ) -> Result<(), Error>;
}
impl<C: RequestIdentityPort> IdentityTime for C {
    fn verify_identity_time(
        &self,
        i: &ProductRequestIdentity,
        epoch: RequestEpoch,
        now: RequestTime,
    ) -> Result<(), Error> {
        if i.epoch != epoch {
            return Err(Error::EpochMismatch);
        }
        if i.issued_at > now.millis() {
            return Err(Error::ClockUnreliable);
        }
        if now.millis() >= i.expires_at {
            return Err(Error::Expired);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
