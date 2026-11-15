//! Original setup, driver and partial process owners survive every tracked launch result.
#[cfg_attr(
    not(test),
    expect(
        clippy::wildcard_imports,
        reason = "This private implementation child shares its parent Windows owner namespace; no names are re-exported."
    )
)]
use super::*;
use std::{
    os::windows::io::IntoRawHandle,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
};
use tokio::sync::Notify;
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, CompareObjectHandles, DUPLICATE_SAME_ACCESS, DuplicateHandle,
        ERROR_INVALID_HANDLE, ERROR_NOT_SAME_OBJECT, ERROR_SUCCESS, GetHandleInformation,
        SetLastError,
    },
    System::Threading::GetCurrentProcess,
};

struct Identity;
#[cfg(test)]
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum TrackedSpawnCut {
    AfterFirstPipe,
    BeforeCreateCall,
    AfterCreateBeforeAssign,
    AfterAssignBeforeResume,
}
#[cfg(test)]
#[derive(Default)]
pub struct TrackedSpawnBarrier {
    entered: Notify,
    released: Mutex<bool>,
    changed: std::sync::Condvar,
}
#[cfg(test)]
impl TrackedSpawnBarrier {
    fn pause(&self) {
        self.entered.notify_one();
        let mut released = self.released.lock().expect("synthetic barrier lock");
        while !*released {
            released = self.changed.wait(released).expect("synthetic barrier wait");
        }
        drop(released);
    }
    pub async fn wait_until_entered(&self) {
        self.entered.notified().await;
    }
    /// # Panics
    /// This test-only barrier fails if another synthetic test poisons its lock.
    pub fn release(&self) {
        *self.released.lock().expect("synthetic barrier release") = true;
        self.changed.notify_all();
    }
}
#[cfg(test)]
#[derive(Clone, Default)]
pub struct TrackedSpawnHooks {
    pub cut: Option<TrackedSpawnCut>,
    pub before_create: Option<Arc<TrackedSpawnBarrier>>,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ProcessCreationCut {
    BeforeCreateCall,
    CreateCallRejected,
}
struct DriverTerminalProof {
    identity: Arc<Identity>,
}
struct OriginalSetupClosedProof {
    identity: Arc<Identity>,
    driver: Arc<DriverTerminalProof>,
}
struct OriginalRootExitedProof {
    identity: Arc<Identity>,
    code: u32,
}
struct OriginalJobZeroProof {
    identity: Arc<Identity>,
}
pub struct NoProcessCreatedProof {
    identity: Arc<Identity>,
    cut: ProcessCreationCut,
    setup: Arc<OriginalSetupClosedProof>,
}
pub struct SuspendedUnassignedClosedProof {
    identity: Arc<Identity>,
    setup: Arc<OriginalSetupClosedProof>,
    root: Arc<OriginalRootExitedProof>,
}
pub struct RunningProcessProof {
    identity: Arc<Identity>,
}
pub enum ProcessSpawnCompletion {
    Running(RunningProcessProof),
    Failed(ProcessOwnerError),
}
pub enum TrackedShutdownReport {
    NeverCreated(Arc<NoProcessCreatedProof>),
    SuspendedUnassigned(Arc<SuspendedUnassignedClosedProof>),
    AssignedJob(ShutdownReport),
    Unconfirmed,
}
impl NoProcessCreatedProof {
    #[must_use]
    pub const fn cut(&self) -> ProcessCreationCut {
        self.cut
    }
}
impl SuspendedUnassignedClosedProof {
    #[must_use]
    pub fn root_exit_code(&self) -> u32 {
        self.root.code
    }
}
struct PreparedSpec {
    executable: Vec<u16>,
    cwd: Vec<u16>,
    command: Vec<u16>,
    environment: Vec<u16>,
}
pub struct PreparedProcessSpawn {
    shared: Arc<Shared>,
    spec: PreparedSpec,
}
pub struct TrackedProcessOwner {
    shared: Arc<Shared>,
}
pub struct ProcessSpawnDriver {
    shared: Arc<Shared>,
    spec: Option<PreparedSpec>,
    armed: bool,
}
struct Shared {
    identity: Arc<Identity>,
    state: Mutex<State>,
    stopped: AtomicBool,
    driver_future_terminal: AtomicBool,
    changed: Notify,
    #[cfg(test)]
    pipe_fault: Mutex<Option<PipeFault>>,
    #[cfg(test)]
    before_child: Mutex<Option<Arc<crate::test_support::AsyncBarrier>>>,
    #[cfg(test)]
    after_child_create: Mutex<Option<Arc<TrackedSpawnBarrier>>>,
    #[cfg(test)]
    after_pipe_create: Mutex<Option<Arc<TrackedSpawnBarrier>>>,
    #[cfg(test)]
    after_pipe_record: Mutex<Option<Arc<TrackedSpawnBarrier>>>,
    #[cfg(test)]
    acquired_pipes: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    acquired_children: std::sync::atomic::AtomicUsize,
}
#[expect(
    clippy::struct_excessive_bools,
    reason = "These independent original acquisition and irreversible OS phase witnesses must remain separately inspectable on partial failure."
)]
struct State {
    #[cfg(test)]
    hooks: TrackedSpawnHooks,
    create_entered: bool,
    create_rejected: bool,
    created: bool,
    assigned: bool,
    resumed: bool,
    untracked_output: bool,
    pipes: Vec<OriginalPipe>,
    children: Vec<OriginalHandle>,
    job: Option<OriginalHandle>,
    process: Option<OriginalHandle>,
    thread: Option<OriginalHandle>,
    pid: u32,
    running: Option<Arc<tokio::sync::Mutex<OwnedProcess>>>,
    blocking: Option<Arc<BlockingSlot>>,
    never_created: Option<Arc<NoProcessCreatedProof>>,
    suspended: Option<Arc<SuspendedUnassignedClosedProof>>,
    assigned_closed: Option<ShutdownReport>,
    root_exited: Option<Arc<OriginalRootExitedProof>>,
    job_zero: Option<Arc<OriginalJobZeroProof>>,
}
struct OriginalHandle {
    raw: usize,
    owned: bool,
    lost: bool,
    closed: bool,
}
impl OriginalHandle {
    fn new(handle: OwnedHandle) -> Self {
        Self {
            raw: handle.into_raw_handle() as usize,
            owned: true,
            lost: false,
            closed: false,
        }
    }
    const fn handle(&self) -> HANDLE {
        self.raw as HANDLE
    }
    fn close(&mut self) -> bool {
        if self.closed {
            return true;
        }
        if !self.owned || self.lost {
            return false;
        }
        // SAFETY: This is the same original unique handle, still owned here (not a disposed Tokio number).
        if unsafe { CloseHandle(self.handle()) } != 0 {
            self.closed = true;
            self.owned = false;
            return true;
        }
        if std::io::Error::last_os_error().raw_os_error()
            == i32::try_from(ERROR_INVALID_HANDLE).ok()
        {
            self.lost = true;
            self.owned = false;
        }
        false
    }
    fn take_owned(&mut self) -> Result<OwnedHandle, ProcessOwnerError> {
        if !self.owned || self.lost || self.closed {
            return Err(ProcessOwnerError::InvalidSpec(
                "original handle unavailable",
            ));
        }
        self.owned = false;
        // SAFETY: Transfers this still-valid original unique owner, never a disposed numeric observation.
        Ok(unsafe { OwnedHandle::from_raw_handle(self.handle()) })
    }
}
impl Drop for OriginalHandle {
    fn drop(&mut self) {
        if self.owned && !self.lost {
            let _ = self.close();
        }
    }
}
struct OriginalPipe {
    raw: usize,
    original_before_registration: Option<OriginalHandle>,
    identity_reference: Option<OriginalHandle>,
    original_retired: Option<OriginalPipeRetirement>,
    server: Option<Arc<NamedPipeServer>>,
    disposed: bool,
    transferred: bool,
    closed: bool,
    #[cfg(test)]
    fault: Option<PipeFault>,
}
#[derive(Clone, Copy)]
enum OriginalPipeRetirement {
    ExplicitOriginalClose,
    InvalidOriginalTableEntry,
    DifferentKernelObject,
}
enum OriginalPipeObservation {
    StillOriginal,
    Retired(OriginalPipeRetirement),
    Unknown,
}
#[cfg(test)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum PipeFault {
    DuplicateCall,
    ObjectObservation,
    IdentityClose,
}
impl OriginalPipe {
    fn acquired(original: OwnedHandle) -> Self {
        Self {
            raw: original.as_raw_handle() as usize,
            original_before_registration: Some(OriginalHandle::new(original)),
            identity_reference: None,
            original_retired: None,
            server: None,
            disposed: false,
            transferred: false,
            closed: false,
            #[cfg(test)]
            fault: None,
        }
    }
    fn acquire_identity_reference(&mut self) -> Result<(), ProcessOwnerError> {
        #[cfg(test)]
        if self.fault == Some(PipeFault::DuplicateCall) {
            return Err(ProcessOwnerError::InvalidSpec(
                "synthetic before pipe duplicate",
            ));
        }
        if self.identity_reference.is_some() {
            return Err(ProcessOwnerError::InvalidSpec(
                "pipe identity already acquired",
            ));
        }
        let original = self
            .original_before_registration
            .as_ref()
            .filter(|original| original.owned && !original.lost && !original.closed)
            .ok_or(ProcessOwnerError::InvalidSpec("original pipe not owned"))?;
        let mut duplicate = null_mut();
        // SAFETY: Duplicate only this STILL-OWNED original handle. The new reference is
        // noninheritable and is immediately installed in this same original pipe inventory.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                original.handle(),
                GetCurrentProcess(),
                &raw mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(last_error("duplicate pipe identity"));
        }
        self.identity_reference = Some(OriginalHandle::new(own(
            duplicate,
            "duplicate pipe identity",
        )?));
        Ok(())
    }
    fn take_original_for_registration(&mut self) -> Result<OwnedHandle, ProcessOwnerError> {
        if self.identity_reference.is_none() {
            return Err(ProcessOwnerError::InvalidSpec("pipe identity missing"));
        }
        self.original_before_registration
            .as_mut()
            .ok_or(ProcessOwnerError::InvalidSpec("original pipe missing"))?
            .take_owned()
    }
    fn dispose_original_server(&mut self) -> bool {
        if self.transferred {
            return false;
        }
        if let Some(server) = self.server.take() {
            match Arc::try_unwrap(server) {
                Ok(server) => {
                    drop(server);
                    self.disposed = true;
                }
                Err(server) => {
                    self.server = Some(server);
                    return false;
                }
            }
        }
        self.disposed
    }
    fn close_and_observe(&mut self) -> bool {
        if self.closed {
            return true;
        }
        if self.original_retired.is_none() {
            if let Some(original) = &mut self.original_before_registration
                && original.owned
            {
                if !original.close() {
                    return false;
                }
                self.original_retired = Some(OriginalPipeRetirement::ExplicitOriginalClose);
            } else {
                if !self.dispose_original_server() {
                    return false;
                }
                match self.observe_original_identity() {
                    OriginalPipeObservation::Retired(retirement) => {
                        self.original_retired = Some(retirement);
                    }
                    OriginalPipeObservation::StillOriginal | OriginalPipeObservation::Unknown => {
                        return false;
                    }
                }
            }
        }
        if self.close_identity_reference() {
            self.closed = true;
        }
        self.closed
    }
    fn observe_original_identity(&self) -> OriginalPipeObservation {
        #[cfg(test)]
        if self.fault == Some(PipeFault::ObjectObservation) {
            return OriginalPipeObservation::Unknown;
        }
        let Some(reference) = self
            .identity_reference
            .as_ref()
            .filter(|reference| reference.owned && !reference.lost && !reference.closed)
        else {
            return OriginalPipeObservation::Unknown;
        };
        let mut flags = 0;
        // SAFETY: The reference remains uniquely owned under the original State lock.
        if unsafe { GetHandleInformation(reference.handle(), &raw mut flags) } == 0 {
            return OriginalPipeObservation::Unknown;
        }
        // SAFETY: Only readonly queries use the potentially disposed original number.
        // It is never duplicated, cancelled, terminated or closed.
        let result = unsafe { GetHandleInformation(self.raw as HANDLE, &raw mut flags) };
        if result == 0 {
            return if std::io::Error::last_os_error().raw_os_error()
                == i32::try_from(ERROR_INVALID_HANDLE).ok()
            {
                OriginalPipeObservation::Retired(OriginalPipeRetirement::InvalidOriginalTableEntry)
            } else {
                OriginalPipeObservation::Unknown
            };
        }
        // SAFETY: Compare kernel object identity with the original still-owned reference.
        // Last error is read immediately and all unclassified outcomes remain Unknown.
        unsafe { SetLastError(ERROR_SUCCESS) };
        if unsafe { CompareObjectHandles(self.raw as HANDLE, reference.handle()) } != 0 {
            OriginalPipeObservation::StillOriginal
        } else if std::io::Error::last_os_error().raw_os_error()
            == i32::try_from(ERROR_NOT_SAME_OBJECT).ok()
        {
            OriginalPipeObservation::Retired(OriginalPipeRetirement::DifferentKernelObject)
        } else {
            OriginalPipeObservation::Unknown
        }
    }
    fn close_identity_reference(&mut self) -> bool {
        #[cfg(test)]
        if self.fault == Some(PipeFault::IdentityClose) {
            return false;
        }
        if let Some(reference) = &mut self.identity_reference {
            reference.close()
        } else {
            // No duplicate was acquired: only an actual still-owned original Close
            // can finish the pre-registration failure path without a reference.
            matches!(
                self.original_retired,
                Some(OriginalPipeRetirement::ExplicitOriginalClose)
            )
        }
    }
    fn identity_closed_after_handoff(&self) -> bool {
        self.transferred
            && self
                .identity_reference
                .as_ref()
                .is_some_and(|reference| reference.closed)
    }
}
struct BlockingSlot {
    task: Mutex<Option<tokio::task::JoinHandle<Result<(), ProcessOwnerError>>>>,
    result: Mutex<Option<Result<(), ProcessOwnerError>>>,
    join_owner: tokio::sync::Mutex<()>,
    installed: AtomicBool,
    changed: Notify,
}
impl BlockingSlot {
    fn new() -> Self {
        Self {
            task: Mutex::new(None),
            result: Mutex::new(None),
            join_owner: tokio::sync::Mutex::new(()),
            installed: AtomicBool::new(false),
            changed: Notify::new(),
        }
    }
    async fn join(&self, deadline: Instant) -> Result<(), ProcessOwnerError> {
        tokio::time::timeout_at(deadline, async {
            // One waiter owns polling. Cancellation releases this async lock so the next
            // waiter re-polls the same retained handle with its own live waker.
            let _join_owner = self.join_owner.lock().await;
            loop {
                let changed = self.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let completed = match self.result.lock() {
                    Ok(result) => result.clone(),
                    Err(error) => error.into_inner().clone(),
                };
                if let Some(result) = completed {
                    return result;
                }
                if !self.installed.load(Ordering::Acquire) {
                    changed.await;
                    continue;
                }
                let completion = std::future::poll_fn(|cx| {
                    let mut task = match self.task.lock() {
                        Ok(task) => task,
                        Err(error) => error.into_inner(),
                    };
                    let Some(handle) = task.as_mut() else {
                        return Poll::Ready(());
                    };
                    match Pin::new(handle).poll(cx) {
                        Poll::Pending => Poll::Pending,
                        Poll::Ready(result) => {
                            let result = result.unwrap_or(Err(ProcessOwnerError::InvalidSpec(
                                "tracked native call task failed",
                            )));
                            *match self.result.lock() {
                                Ok(value) => value,
                                Err(error) => error.into_inner(),
                            } = Some(result);
                            task.take();
                            drop(task);
                            self.changed.notify_waiters();
                            Poll::Ready(())
                        }
                    }
                });
                tokio::select! {()=&mut changed=>{},()=completion=>{}}
            }
        })
        .await
        .map_err(|_| ProcessOwnerError::SpawnTimeout)?
    }
}
impl OwnedProcess {
    /// Preallocates the original tracked state without OS handles, process creation or await.
    /// # Errors
    /// Rejects unrepresentable specifications and recoverable initial inventory allocation failure.
    pub fn prepare_tracked(spec: &ProcessSpec) -> Result<PreparedProcessSpawn, ProcessOwnerError> {
        if !spec.executable.is_absolute() || !spec.cwd.is_absolute() {
            return Err(ProcessOwnerError::InvalidSpec(
                "absolute executable and cwd required",
            ));
        }
        let spec = PreparedSpec {
            executable: wide_null(spec.executable.as_os_str())?,
            cwd: wide_null(spec.cwd.as_os_str())?,
            command: command_line(spec.executable.as_os_str(), &spec.args)?,
            environment: environment(&spec.env)?,
        };
        let mut pipes = Vec::new();
        pipes
            .try_reserve(3)
            .map_err(|_| ProcessOwnerError::InvalidSpec("pipe inventory allocation"))?;
        let mut children = Vec::new();
        children
            .try_reserve(3)
            .map_err(|_| ProcessOwnerError::InvalidSpec("handle inventory allocation"))?;
        let shared = Arc::new(Shared {
            identity: Arc::new(Identity),
            state: Mutex::new(State {
                #[cfg(test)]
                hooks: TrackedSpawnHooks::default(),
                create_entered: false,
                create_rejected: false,
                created: false,
                assigned: false,
                resumed: false,
                untracked_output: false,
                pipes,
                children,
                job: None,
                process: None,
                thread: None,
                pid: 0,
                running: None,
                blocking: None,
                never_created: None,
                suspended: None,
                assigned_closed: None,
                root_exited: None,
                job_zero: None,
            }),
            stopped: AtomicBool::new(false),
            driver_future_terminal: AtomicBool::new(false),
            changed: Notify::new(),
            #[cfg(test)]
            pipe_fault: Mutex::new(None),
            #[cfg(test)]
            before_child: Mutex::new(None),
            #[cfg(test)]
            after_child_create: Mutex::new(None),
            #[cfg(test)]
            after_pipe_create: Mutex::new(None),
            #[cfg(test)]
            after_pipe_record: Mutex::new(None),
            #[cfg(test)]
            acquired_pipes: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            acquired_children: std::sync::atomic::AtomicUsize::new(0),
        });
        Ok(PreparedProcessSpawn { shared, spec })
    }
}
impl PreparedProcessSpawn {
    #[cfg(test)]
    #[must_use]
    /// # Panics
    /// This test-only setter requires the fresh, unpoisoned original prepared owner.
    pub fn with_hooks_for_test(self, hooks: TrackedSpawnHooks) -> Self {
        self.shared
            .state
            .lock()
            .expect("synthetic fresh prepared owner")
            .hooks = hooks;
        self
    }
    #[must_use]
    pub fn into_owner_driver(self) -> (TrackedProcessOwner, ProcessSpawnDriver) {
        (
            TrackedProcessOwner {
                shared: self.shared.clone(),
            },
            ProcessSpawnDriver {
                shared: self.shared,
                spec: Some(self.spec),
                armed: true,
            },
        )
    }
}
struct DriverActivation {
    shared: Arc<Shared>,
}
impl Drop for DriverActivation {
    fn drop(&mut self) {
        self.shared
            .driver_future_terminal
            .store(true, Ordering::Release);
        self.shared.changed.notify_waiters();
    }
}
impl Drop for ProcessSpawnDriver {
    fn drop(&mut self) {
        if self.armed {
            self.shared
                .driver_future_terminal
                .store(true, Ordering::Release);
            self.shared.stopped.store(true, Ordering::Release);
            self.shared.changed.notify_waiters();
        }
    }
}
impl ProcessSpawnDriver {
    pub async fn run(mut self, deadline: Instant) -> ProcessSpawnCompletion {
        let activation = DriverActivation {
            shared: self.shared.clone(),
        };
        self.armed = false;
        let result = {
            let shared = self.shared.clone();
            let stopped = shared.changed.notified();
            tokio::pin!(stopped);
            stopped.as_mut().enable();
            if shared.stopped.load(Ordering::Acquire) {
                Err(ProcessOwnerError::SpawnTimeout)
            } else {
                tokio::select! {biased;
                    ()=&mut stopped=>Err(ProcessOwnerError::SpawnTimeout),
                    ()=tokio::time::sleep_until(deadline)=>Err(ProcessOwnerError::SpawnTimeout),
                    result=self.run_steps(deadline)=>result,
                }
            }
        };
        drop(activation);
        match result {
            Ok(()) => ProcessSpawnCompletion::Running(RunningProcessProof {
                identity: self.shared.identity.clone(),
            }),
            Err(error) => ProcessSpawnCompletion::Failed(error),
        }
    }
    async fn run_steps(&mut self, deadline: Instant) -> Result<(), ProcessOwnerError> {
        for index in 0..3 {
            self.pipe().await?;
            #[cfg(test)]
            if index == 0
                && self
                    .shared
                    .state
                    .lock()
                    .map_err(|_| ProcessOwnerError::InvalidSpec("driver state lock"))?
                    .hooks
                    .cut
                    == Some(TrackedSpawnCut::AfterFirstPipe)
            {
                return Err(ProcessOwnerError::InvalidSpec(
                    "synthetic after first original pipe",
                ));
            }
            #[cfg(not(test))]
            let _ = index;
        }
        let spec = self
            .spec
            .take()
            .ok_or(ProcessOwnerError::InvalidSpec("driver already consumed"))?;
        let slot = Arc::new(BlockingSlot::new());
        self.shared
            .state
            .lock()
            .map_err(|_| ProcessOwnerError::InvalidSpec("driver state lock"))?
            .blocking = Some(slot.clone());
        let shared = self.shared.clone();
        let task = tokio::task::spawn_blocking(move || native_create(&shared, spec, deadline));
        match slot.task.lock() {
            Ok(mut target) => *target = Some(task),
            Err(error) => *error.into_inner() = Some(task),
        }
        slot.installed.store(true, Ordering::Release);
        slot.changed.notify_waiters();
        slot.join(deadline).await
    }
    fn take_original_pipe_for_registration(
        &self,
        index: usize,
    ) -> Result<OwnedHandle, ProcessOwnerError> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| ProcessOwnerError::InvalidSpec("pipe state lock"))?;
        let pipe = state
            .pipes
            .get_mut(index)
            .ok_or(ProcessOwnerError::InvalidSpec(
                "original pipe index missing",
            ))?;
        pipe.acquire_identity_reference()?;
        let original = pipe.take_original_for_registration()?;
        drop(state);
        Ok(original)
    }
    #[cfg(test)]
    async fn pause_before_original_child(&self) -> Result<(), ProcessOwnerError> {
        let before_child = self
            .shared
            .before_child
            .lock()
            .map_err(|_| ProcessOwnerError::InvalidSpec("synthetic before child lock"))?
            .clone();
        if let Some(before_child) = before_child {
            before_child.pause().await;
        }
        Ok(())
    }
    fn capture_acquired_pipe(&self, original: OwnedHandle) -> Result<usize, ProcessOwnerError> {
        // Capture first even on poison; only cleanup may proceed with poisoned state.
        let (mut state, poisoned) = match self.shared.state.lock() {
            Ok(state) => (state, false),
            Err(error) => (error.into_inner(), true),
        };
        let index = state.pipes.len();
        state.pipes.push(OriginalPipe::acquired(original));
        if poisoned {
            return Err(ProcessOwnerError::InvalidSpec("pipe state lock"));
        }
        #[cfg(test)]
        if index == 0 {
            state.pipes[index].fault = *self
                .shared
                .pipe_fault
                .lock()
                .map_err(|_| ProcessOwnerError::InvalidSpec("synthetic pipe fault lock"))?;
        }
        drop(state);
        Ok(index)
    }
    fn capture_registered_server(
        &self,
        index: usize,
        server: &Arc<NamedPipeServer>,
    ) -> Result<(), ProcessOwnerError> {
        let (mut state, poisoned) = match self.shared.state.lock() {
            Ok(state) => (state, false),
            Err(error) => (error.into_inner(), true),
        };
        // Indices come from this unique driver's append-only inventory; no path
        // removes/reorders entries while the driver can still own a local server.
        state.pipes[index].server = Some(server.clone());
        drop(state);
        if poisoned {
            Err(ProcessOwnerError::InvalidSpec("pipe state lock"))
        } else {
            Ok(())
        }
    }
    fn note_failed_registration(&self, index: usize) {
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(error) => error.into_inner(),
        };
        state.pipes[index].disposed = true;
    }
    fn capture_acquired_child(&self, child: OwnedHandle) -> Result<(), ProcessOwnerError> {
        let (mut state, poisoned) = match self.shared.state.lock() {
            Ok(state) => (state, false),
            Err(error) => (error.into_inner(), true),
        };
        state.children.push(OriginalHandle::new(child));
        drop(state);
        if poisoned {
            Err(ProcessOwnerError::InvalidSpec("pipe state lock"))
        } else {
            Ok(())
        }
    }
    async fn pipe(&self) -> Result<(), ProcessOwnerError> {
        if self.shared.stopped.load(Ordering::Acquire) {
            return Err(ProcessOwnerError::SpawnTimeout);
        }
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).map_err(|_| ProcessOwnerError::Windows {
            operation: "pipe random",
            code: 0,
        })?;
        let name = OsString::from(format!(r"\\.\pipe\acp-process-{}", hex::encode(nonce)));
        let sid = crate::platform::current_user_sid().map_err(|_| ProcessOwnerError::Windows {
            operation: "pipe owner",
            code: 0,
        })?;
        #[cfg(test)]
        let after_create = self.shared.after_pipe_create.lock().unwrap().clone();
        #[cfg(test)]
        let after_record = self.shared.after_pipe_record.lock().unwrap().clone();
        #[cfg(test)]
        let after_child_create = self.shared.after_child_create.lock().unwrap().clone();
        let original = crate::platform::windows::create_private_pipe_owned(&name, &sid)
            .map_err(|error| io_error("create stdio pipe", &error))?;
        #[cfg(test)]
        {
            self.shared.acquired_pipes.fetch_add(1, Ordering::SeqCst);
            if let Some(after_create) = after_create {
                after_create.pause();
            }
        }
        let index = self.capture_acquired_pipe(original)?;
        #[cfg(test)]
        if let Some(after_record) = after_record {
            after_record.pause();
        }
        let transferred = self
            .take_original_pipe_for_registration(index)?
            .into_raw_handle();
        // SAFETY: The original acquired handle was recorded before this single ownership transfer.
        let server = unsafe { NamedPipeServer::from_raw_handle(transferred) };
        let server = match server {
            Ok(server) => Arc::new(server),
            Err(error) => {
                // Retain actual failure stage even if coordination was poisoned.
                self.note_failed_registration(index);
                return Err(io_error("register stdio pipe", &error));
            }
        };
        self.capture_registered_server(index, &server)?;
        #[cfg(test)]
        self.pause_before_original_child().await?;
        let path = wide_null(&name)?;
        let child = {
            let attributes = SECURITY_ATTRIBUTES {
                nLength: win_size::<SECURITY_ATTRIBUTES>()?,
                lpSecurityDescriptor: null_mut(),
                bInheritHandle: 1,
            };
            // SAFETY: Open this same random owner-only pipe; child end is synchronous and deliberately inheritable.
            own(
                unsafe {
                    CreateFileW(
                        path.as_ptr(),
                        FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                        0,
                        &raw const attributes,
                        OPEN_EXISTING,
                        0,
                        null_mut(),
                    )
                },
                "open stdio pipe",
            )?
        };
        #[cfg(test)]
        self.shared.acquired_children.fetch_add(1, Ordering::SeqCst);
        #[cfg(test)]
        if let Some(after_child_create) = after_child_create {
            after_child_create.pause();
        }
        self.capture_acquired_child(child)?;
        server
            .connect()
            .await
            .map_err(|error| io_error("connect stdio pipe", &error))?;
        Ok(())
    }
}

impl TrackedProcessOwner {
    fn state_for_cleanup(&self) -> Option<std::sync::MutexGuard<'_, State>> {
        // Poison never makes the source Running. Cleanup still owns these original typed
        // handles and must verify every actual phase/OS condition before creating evidence.
        match self.shared.state.try_lock() {
            Ok(state) => Some(state),
            Err(std::sync::TryLockError::Poisoned(error)) => Some(error.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }
    /// Requests cancellation of future driver work; this is not a closure proof.
    pub fn request_stop(&self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.shared.changed.notify_waiters();
    }
    #[must_use]
    pub fn confirms_running(&self, proof: &RunningProcessProof) -> bool {
        Arc::ptr_eq(&self.shared.identity, &proof.identity)
            && self.shared.driver_future_terminal.load(Ordering::Acquire)
            && !self.shared.stopped.load(Ordering::Acquire)
            && self.shared.state.try_lock().is_ok_and(|state| {
                state.resumed
                    && state.running.is_some()
                    && state.pipes.len() == 3
                    && state
                        .pipes
                        .iter()
                        .all(OriginalPipe::identity_closed_after_handoff)
            })
    }
    /// # Errors
    /// Refuses stopping, nonrunning, borrowed or already-consumed original IO.
    pub fn take_io(&mut self) -> Result<ProcessIo, ProcessOwnerError> {
        if self.shared.stopped.load(Ordering::Acquire) {
            return Err(ProcessOwnerError::AlreadyTaken);
        }
        let state = self
            .shared
            .state
            .try_lock()
            .map_err(|_| ProcessOwnerError::InvalidSpec("tracked process state busy"))?;
        if state.pipes.len() != 3
            || !state
                .pipes
                .iter()
                .all(OriginalPipe::identity_closed_after_handoff)
        {
            return Err(ProcessOwnerError::InvalidSpec(
                "pipe identity closure unconfirmed",
            ));
        }
        let process = state.running.clone().ok_or(ProcessOwnerError::InvalidSpec(
            "tracked process not running",
        ))?;
        drop(state);
        let mut process = process
            .try_lock()
            .map_err(|_| ProcessOwnerError::InvalidSpec("tracked process owner busy"))?;
        process.take_io()
    }
    /// Observes only this original owner, driver and setup inventory. Incomplete results retain them.
    #[expect(
        clippy::too_many_lines,
        reason = "The original driver, root, Job, setup handles and cached proof must be reconciled together without substituting owners."
    )]
    pub async fn shutdown(&mut self, deadline: Instant) -> TrackedShutdownReport {
        self.request_stop();
        loop {
            let changed = self.shared.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let blocking = match self.state_for_cleanup() {
                Some(state) => state.blocking.clone(),
                None => return TrackedShutdownReport::Unconfirmed,
            };
            if let Some(blocking) = blocking {
                let _ = blocking.join(deadline).await;
                let complete = match blocking.result.lock() {
                    Ok(result) => result.is_some(),
                    Err(error) => error.into_inner().is_some(),
                };
                if !complete {
                    return TrackedShutdownReport::Unconfirmed;
                }
            }
            if !self.shared.driver_future_terminal.load(Ordering::Acquire) {
                if tokio::time::timeout_at(deadline, &mut changed)
                    .await
                    .is_err()
                {
                    return TrackedShutdownReport::Unconfirmed;
                }
                continue;
            }
            let running = match self.state_for_cleanup() {
                Some(state) => state.running.clone(),
                None => return TrackedShutdownReport::Unconfirmed,
            };
            if let Some(running) = running {
                let Ok(mut process) = tokio::time::timeout_at(deadline, running.lock()).await
                else {
                    return TrackedShutdownReport::Unconfirmed;
                };
                let result = process.shutdown(deadline).await;
                drop(process);
                let Ok(report) = result else {
                    return TrackedShutdownReport::Unconfirmed;
                };
                let Some(mut state) = self.state_for_cleanup() else {
                    return TrackedShutdownReport::Unconfirmed;
                };
                let mut references_closed = true;
                for pipe in &mut state.pipes {
                    references_closed &= pipe.close_identity_reference();
                }
                return if references_closed {
                    TrackedShutdownReport::AssignedJob(report)
                } else {
                    TrackedShutdownReport::Unconfirmed
                };
            }
            let ready = {
                let Some(mut state) = self.state_for_cleanup() else {
                    return TrackedShutdownReport::Unconfirmed;
                };
                if let Some(proof) = &state.never_created {
                    return TrackedShutdownReport::NeverCreated(proof.clone());
                }
                if let Some(proof) = &state.suspended {
                    return TrackedShutdownReport::SuspendedUnassigned(proof.clone());
                }
                if let Some(report) = state.assigned_closed {
                    return TrackedShutdownReport::AssignedJob(report);
                }
                if state.untracked_output {
                    return TrackedShutdownReport::Unconfirmed;
                }
                let root_exit = if let Some(proof) = &state.root_exited {
                    Some(proof.code)
                } else if state.created {
                    let Some(process) = state.process.as_ref() else {
                        return TrackedShutdownReport::Unconfirmed;
                    };
                    if !process.owned {
                        return TrackedShutdownReport::Unconfirmed;
                    }
                    if state.assigned {
                        let Some(job) = state.job.as_ref() else {
                            return TrackedShutdownReport::Unconfirmed;
                        };
                        // SAFETY: Stop only this original assigned Job. Never a substituted empty Job.
                        if unsafe { TerminateJobObject(job.handle(), 1) } == 0 {
                            return TrackedShutdownReport::Unconfirmed;
                        }
                    } else {
                        if state.resumed {
                            return TrackedShutdownReport::Unconfirmed;
                        }
                        // SAFETY: Original still-owned, never-resumed suspended root.
                        if unsafe { TerminateProcess(process.handle(), 1) } == 0 {
                            // An already exited root can legitimately reject termination; verify below.
                        }
                    }
                    let mut code = 0;
                    // SAFETY: Zero-time wait and exit query use the same still-owned original root.
                    if unsafe { WaitForSingleObject(process.handle(), 0) } != WAIT_OBJECT_0
                        || unsafe { GetExitCodeProcess(process.handle(), &raw mut code) } == 0
                    {
                        None
                    } else {
                        state.root_exited = Some(Arc::new(OriginalRootExitedProof {
                            identity: self.shared.identity.clone(),
                            code,
                        }));
                        Some(code)
                    }
                } else {
                    None
                };
                if state.created && root_exit.is_none() {
                    false
                } else {
                    // Assigned-but-not-running cleanup still needs the complete original Job, including descendants.
                    if state.assigned && state.job_zero.is_none() {
                        let Some(job) = state.job.as_ref() else {
                            return TrackedShutdownReport::Unconfirmed;
                        };
                        let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION =
                            unsafe { zeroed() };
                        let Ok(size) = win_size::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() else {
                            return TrackedShutdownReport::Unconfirmed;
                        };
                        if unsafe {
                            QueryInformationJobObject(
                                job.handle(),
                                JobObjectBasicAccountingInformation,
                                (&raw mut accounting).cast(),
                                size,
                                null_mut(),
                            )
                        } == 0
                            || accounting.ActiveProcesses != 0
                        {
                            return TrackedShutdownReport::Unconfirmed;
                        }
                        state.job_zero = Some(Arc::new(OriginalJobZeroProof {
                            identity: self.shared.identity.clone(),
                        }));
                    }
                    let mut closed = true;
                    for pipe in &mut state.pipes {
                        closed &= pipe.close_and_observe();
                    }
                    for child in &mut state.children {
                        closed &= child.close();
                    }
                    if let Some(thread) = &mut state.thread {
                        closed &= thread.close();
                    }
                    if let Some(process) = &mut state.process {
                        closed &= process.close();
                    }
                    // SuspendedUnassigned requires this SAME original Job handle closed, not Job active0.
                    if let Some(job) = &mut state.job {
                        closed &= job.close();
                    }
                    if closed {
                        let driver = Arc::new(DriverTerminalProof {
                            identity: self.shared.identity.clone(),
                        });
                        let setup = Arc::new(OriginalSetupClosedProof {
                            identity: self.shared.identity.clone(),
                            driver,
                        });
                        if !state.created && (!state.create_entered || state.create_rejected) {
                            let cut = if state.create_rejected {
                                ProcessCreationCut::CreateCallRejected
                            } else {
                                ProcessCreationCut::BeforeCreateCall
                            };
                            state.never_created = Some(Arc::new(NoProcessCreatedProof {
                                identity: self.shared.identity.clone(),
                                cut,
                                setup,
                            }));
                            true
                        } else if state.created && !state.assigned && !state.resumed {
                            if let Some(root) = state.root_exited.clone() {
                                state.suspended = Some(Arc::new(SuspendedUnassignedClosedProof {
                                    identity: self.shared.identity.clone(),
                                    setup,
                                    root,
                                }));
                                true
                            } else {
                                false
                            }
                        } else if state.created && state.assigned {
                            if !state.job_zero.as_ref().is_some_and(|proof| {
                                Arc::ptr_eq(&proof.identity, &self.shared.identity)
                            }) {
                                return TrackedShutdownReport::Unconfirmed;
                            }
                            state.assigned_closed = Some(ShutdownReport {
                                disposition: ShutdownDisposition::Forced,
                                active_processes: 0,
                                root_exit_code: root_exit,
                            });
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            };
            if ready {
                continue;
            }
            if Instant::now() >= deadline {
                return TrackedShutdownReport::Unconfirmed;
            }
            tokio::select! {()=&mut changed=>{},()=tokio::time::sleep_until(deadline.min(Instant::now()+Duration::from_millis(10)))=>{}}
        }
    }
    #[must_use]
    pub fn confirms_never_created(&self, proof: &NoProcessCreatedProof) -> bool {
        Arc::ptr_eq(&self.shared.identity, &proof.identity)
            && Arc::ptr_eq(&proof.identity, &proof.setup.identity)
            && Arc::ptr_eq(&proof.identity, &proof.setup.driver.identity)
    }
    #[must_use]
    pub fn confirms_suspended(&self, proof: &SuspendedUnassignedClosedProof) -> bool {
        Arc::ptr_eq(&self.shared.identity, &proof.identity)
            && Arc::ptr_eq(&proof.identity, &proof.root.identity)
            && Arc::ptr_eq(&proof.identity, &proof.setup.identity)
            && Arc::ptr_eq(&proof.identity, &proof.setup.driver.identity)
    }
}
impl Drop for TrackedProcessOwner {
    fn drop(&mut self) {
        self.request_stop();
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Every real suspended-create, assign and resume step records its original handle under the same tracked owner lock."
)]
fn native_create(
    shared: &Arc<Shared>,
    mut spec: PreparedSpec,
    deadline: Instant,
) -> Result<(), ProcessOwnerError> {
    let mut state = shared
        .state
        .lock()
        .map_err(|_| ProcessOwnerError::InvalidSpec("driver state lock"))?;
    if shared.stopped.load(Ordering::Acquire) || Instant::now() >= deadline {
        return Err(ProcessOwnerError::SpawnTimeout);
    }
    let job = own(
        unsafe { CreateJobObjectW(null(), null()) },
        "CreateJobObjectW",
    )?;
    state.job = Some(OriginalHandle::new(job));
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let job = state
        .job
        .as_ref()
        .ok_or(ProcessOwnerError::InvalidSpec("missing tracked Job"))?
        .handle();
    if unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            win_size::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()?,
        )
    } == 0
    {
        return Err(last_error("SetInformationJobObject"));
    }
    if state.children.len() != 3 {
        return Err(ProcessOwnerError::InvalidSpec("incomplete tracked stdio"));
    }
    let mut handles = [
        state.children[0].handle(),
        state.children[1].handle(),
        state.children[2].handle(),
    ];
    let mut attributes = Attributes::new(&mut handles)?;
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = win_size::<STARTUPINFOEXW>()?;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.pointer();
    let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
    #[cfg(test)]
    if let Some(barrier) = &state.hooks.before_create {
        barrier.pause();
    }
    #[cfg(test)]
    if state.hooks.cut == Some(TrackedSpawnCut::BeforeCreateCall) {
        return Err(ProcessOwnerError::InvalidSpec(
            "synthetic before CreateProcess",
        ));
    }
    if shared.stopped.load(Ordering::Acquire) || Instant::now() >= deadline {
        return Err(ProcessOwnerError::SpawnTimeout);
    }
    state.create_entered = true;
    // SAFETY: Same original stdio handles, complete private startup list and validated strings remain live.
    let created = unsafe {
        CreateProcessW(
            spec.executable.as_ptr(),
            spec.command.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            spec.environment.as_ptr().cast(),
            spec.cwd.as_ptr(),
            &raw const startup.StartupInfo,
            &raw mut info,
        )
    };
    if created == 0 {
        state.create_rejected = true;
        return Err(last_error("CreateProcessW"));
    }
    state.created = true;
    state.pid = info.dwProcessId;
    // Successful CreateProcess guarantees these new handles. Record each immediately before any later fallible operation.
    if !info.hProcess.is_null()
        && info.hProcess != windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE
    {
        state.process = Some(OriginalHandle::new(unsafe {
            OwnedHandle::from_raw_handle(info.hProcess)
        }));
    } else {
        state.untracked_output = true;
    }
    if !info.hThread.is_null()
        && info.hThread != windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE
    {
        state.thread = Some(OriginalHandle::new(unsafe {
            OwnedHandle::from_raw_handle(info.hThread)
        }));
    } else {
        state.untracked_output = true;
    }
    if state.untracked_output {
        return Err(ProcessOwnerError::InvalidSpec(
            "invalid CreateProcess output",
        ));
    }
    #[cfg(test)]
    if state.hooks.cut == Some(TrackedSpawnCut::AfterCreateBeforeAssign) {
        return Err(ProcessOwnerError::InvalidSpec(
            "synthetic after original root acquisition",
        ));
    }
    if shared.stopped.load(Ordering::Acquire) || Instant::now() >= deadline {
        return Err(ProcessOwnerError::SpawnTimeout);
    }
    let process = state
        .process
        .as_ref()
        .ok_or(ProcessOwnerError::InvalidSpec("missing tracked root"))?
        .handle();
    if unsafe { AssignProcessToJobObject(job, process) } == 0 {
        return Err(last_error("AssignProcessToJobObject"));
    }
    state.assigned = true;
    #[cfg(test)]
    if state.hooks.cut == Some(TrackedSpawnCut::AfterAssignBeforeResume) {
        return Err(ProcessOwnerError::InvalidSpec(
            "synthetic after original Job assignment",
        ));
    }
    if shared.stopped.load(Ordering::Acquire) || Instant::now() >= deadline {
        return Err(ProcessOwnerError::SpawnTimeout);
    }
    let thread = state
        .thread
        .as_ref()
        .ok_or(ProcessOwnerError::InvalidSpec("missing tracked thread"))?
        .handle();
    if unsafe { ResumeThread(thread) } == u32::MAX {
        return Err(last_error("ResumeThread"));
    }
    state.resumed = true;
    for child in &mut state.children {
        if !child.close() {
            return Err(ProcessOwnerError::InvalidSpec(
                "original child handle close unconfirmed",
            ));
        }
    }
    if !state
        .thread
        .as_mut()
        .ok_or(ProcessOwnerError::InvalidSpec("missing tracked thread"))?
        .close()
    {
        return Err(ProcessOwnerError::InvalidSpec(
            "original thread handle close unconfirmed",
        ));
    }
    if state.pipes.len() != 3 || state.pipes.iter().any(|pipe| pipe.server.is_none()) {
        return Err(ProcessOwnerError::InvalidSpec("incomplete original pipes"));
    }
    let mut detached = Vec::with_capacity(3);
    for pipe in &mut state.pipes {
        let Some(server) = pipe.server.take() else {
            return Err(ProcessOwnerError::InvalidSpec("missing original pipe"));
        };
        detached.push(Arc::try_unwrap(server));
    }
    if detached.iter().any(Result::is_err) {
        for (pipe, server) in state.pipes.iter_mut().zip(detached) {
            pipe.server = Some(match server {
                Ok(server) => Arc::new(server),
                Err(server) => server,
            });
        }
        return Err(ProcessOwnerError::InvalidSpec(
            "original pipe still borrowed",
        ));
    }
    let mut pipes = Vec::with_capacity(3);
    for server in detached {
        match server {
            Ok(server) => pipes.push(server),
            Err(_) => {
                return Err(ProcessOwnerError::InvalidSpec(
                    "original pipe handoff failed",
                ));
            }
        }
    }
    let pipes: [NamedPipeServer; 3] = match pipes.try_into() {
        Ok(pipes) => pipes,
        Err(pipes) => {
            for (record, server) in state.pipes.iter_mut().zip(pipes) {
                record.server = Some(Arc::new(server));
            }
            return Err(ProcessOwnerError::InvalidSpec(
                "incomplete original pipe handoff",
            ));
        }
    };
    let process = match state
        .process
        .as_mut()
        .ok_or(ProcessOwnerError::InvalidSpec("missing tracked root"))
        .and_then(OriginalHandle::take_owned)
    {
        Ok(process) => process,
        Err(error) => {
            for (record, server) in state.pipes.iter_mut().zip(pipes) {
                record.server = Some(Arc::new(server));
            }
            return Err(error);
        }
    };
    let job = match state
        .job
        .as_mut()
        .ok_or(ProcessOwnerError::InvalidSpec("missing tracked Job"))
        .and_then(OriginalHandle::take_owned)
    {
        Ok(job) => job,
        Err(error) => {
            state.process = Some(OriginalHandle::new(process));
            for (record, server) in state.pipes.iter_mut().zip(pipes) {
                record.server = Some(Arc::new(server));
            }
            return Err(error);
        }
    };
    let [stdin, stdout, stderr] = pipes;
    for pipe in &mut state.pipes {
        pipe.transferred = true;
    }
    state.running = Some(Arc::new(tokio::sync::Mutex::new(OwnedProcess {
        job,
        process,
        pid: state.pid,
        io: Some(ProcessIo {
            stdin,
            stdout,
            stderr,
        }),
        forced: false,
    })));
    let mut references_closed = true;
    for pipe in &mut state.pipes {
        // Only the additional identity references close here. Original IO is now
        // installed in the SAME running owner and is not marked physically closed.
        references_closed &= pipe.close_identity_reference();
    }
    if !references_closed {
        return Err(ProcessOwnerError::InvalidSpec(
            "pipe identity handoff closure unconfirmed",
        ));
    }
    drop(state);
    Ok(())
}

#[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
mod tests {
    use super::*;
    fn missing_spec() -> ProcessSpec {
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let executable =
            std::env::temp_dir().join(format!("native-tracked-missing-{}.exe", hex::encode(nonce)));
        assert!(!executable.exists());
        ProcessSpec {
            executable,
            args: vec![],
            cwd: std::env::temp_dir(),
            env: vec![],
        }
    }
    #[tokio::test]
    async fn tracked_unused_driver_consumes_actual_future_create_right_without_job_zero() {
        let prepared = OwnedProcess::prepare_tracked(&missing_spec()).unwrap();
        let (mut owner, driver) = prepared.into_owner_driver();
        drop(driver);
        let TrackedShutdownReport::NeverCreated(proof) = owner
            .shutdown(Instant::now() + Duration::from_secs(3))
            .await
        else {
            panic!("original no-create proof missing");
        };
        assert!(owner.confirms_never_created(&proof));
        assert!(matches!(proof.cut(), ProcessCreationCut::BeforeCreateCall));
        assert!(owner.shared.state.lock().unwrap().pipes.is_empty());
        assert!(owner.shared.state.lock().unwrap().job.is_none());
    }
    #[tokio::test]
    async fn tracked_actual_create_rejection_closes_original_setup_and_retains_same_proof() {
        let prepared = OwnedProcess::prepare_tracked(&missing_spec()).unwrap();
        let (mut owner, driver) = prepared.into_owner_driver();
        let outcome = driver.run(Instant::now() + Duration::from_secs(5)).await;
        let error = match outcome {
            ProcessSpawnCompletion::Failed(error) => error,
            ProcessSpawnCompletion::Running(_) => {
                panic!("missing executable unexpectedly produced a running owner")
            }
        };
        assert!(
            matches!(
                error,
                ProcessOwnerError::Windows {
                    operation: "CreateProcessW",
                    code: 2
                }
            ),
            "unexpected spawn error: {error:?}"
        );
        let TrackedShutdownReport::NeverCreated(proof) = owner
            .shutdown(Instant::now() + Duration::from_secs(5))
            .await
        else {
            panic!("actual rejected-call/setup proof missing");
        };
        assert!(matches!(
            proof.cut(),
            ProcessCreationCut::CreateCallRejected
        ));
        assert!(owner.confirms_never_created(&proof));
        {
            let state = owner.shared.state.lock().unwrap();
            assert!(state.create_entered && state.create_rejected && !state.created);
            assert!(state.pipes.len() == 3 && state.pipes.iter().all(|pipe| pipe.closed));
            assert!(state.children.len() == 3 && state.children.iter().all(|handle| handle.closed));
            assert!(state.job.as_ref().unwrap().closed);
            drop(state);
        }
        let TrackedShutdownReport::NeverCreated(repeated) = owner
            .shutdown(Instant::now() + Duration::from_secs(3))
            .await
        else {
            panic!("original proof lost");
        };
        assert!(Arc::ptr_eq(&proof, &repeated));
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tracked_cancelled_driver_keeps_original_blocking_call_until_actual_join() {
        for abort_waiter in [false, true] {
            let barrier = Arc::new(TrackedSpawnBarrier::default());
            let prepared = OwnedProcess::prepare_tracked(&missing_spec())
                .unwrap()
                .with_hooks_for_test(TrackedSpawnHooks {
                    cut: None,
                    before_create: Some(barrier.clone()),
                });
            let (mut owner, driver) = prepared.into_owner_driver();
            let driver_task = tokio::spawn(driver.run(Instant::now() + Duration::from_secs(3)));
            tokio::time::timeout(Duration::from_secs(2), barrier.wait_until_entered())
                .await
                .unwrap();
            owner.request_stop();
            if abort_waiter {
                driver_task.abort();
                assert!(matches!(driver_task.await,Err(error) if error.is_cancelled()));
            } else {
                assert!(matches!(
                    driver_task.await.unwrap(),
                    ProcessSpawnCompletion::Failed(_)
                ));
            }
            assert!(owner.shared.driver_future_terminal.load(Ordering::Acquire));
            assert!(matches!(
                owner
                    .shutdown(Instant::now() + Duration::from_millis(20))
                    .await,
                TrackedShutdownReport::Unconfirmed
            ));
            barrier.release();
            let end = Instant::now() + Duration::from_secs(3);
            let proof = loop {
                match owner.shutdown(end).await {
                    TrackedShutdownReport::NeverCreated(proof) => break proof,
                    TrackedShutdownReport::Unconfirmed if Instant::now() < end => {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    _ => panic!(
                        "same original setup/driver did not converge after real blocking completion"
                    ),
                }
            };
            assert!(matches!(proof.cut(), ProcessCreationCut::BeforeCreateCall));
            assert!(owner.confirms_never_created(&proof));
            {
                let state = owner.shared.state.lock().unwrap();
                assert!(
                    !state.create_entered && !state.created && !state.assigned && !state.resumed
                );
                assert!(state.pipes.len() == 3 && state.pipes.iter().all(|pipe| pipe.closed));
                assert!(
                    state.children.len() == 3 && state.children.iter().all(|handle| handle.closed)
                );
                assert!(state.job.as_ref().unwrap().closed);
                assert!(
                    state
                        .blocking
                        .as_ref()
                        .unwrap()
                        .result
                        .lock()
                        .unwrap()
                        .is_some()
                );
                drop(state);
            }
            let TrackedShutdownReport::NeverCreated(repeated) = owner.shutdown(end).await else {
                panic!("original closed proof lost after cancellation");
            };
            assert!(Arc::ptr_eq(&proof, &repeated));
        }
    }
}

#[cfg(test)]
mod acquisition_poison_tests;
#[cfg(test)]
mod disposal_reuse_probe;
