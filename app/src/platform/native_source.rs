//! The production Windows launcher keeps the original Job owner through all IO handoffs.
use super::process_owner::{
    NoProcessCreatedProof, OwnedProcess, ProcessCreationCut, ProcessOwnerError,
    ProcessSpawnCompletion, ProcessSpec, RunningProcessProof, SuspendedUnassignedClosedProof,
    TrackedProcessOwner, TrackedShutdownReport,
};
use crate::{
    adapters::acp::native_dispatch::AcpStreams,
    application::native_dispatch::{
        BoxFuture, EnteredLaunch, LaunchAdopter, LaunchCompletion, LaunchPermit,
        NeverCreatedWitness, OwnedNativeSource, OwnedNeverCreatedClosure, OwnedSourceShutdown,
        OwnedSuspendedUnassignedClosure, SourceCreationCut, SourceLaunchController,
        SourceLaunchPort, SuspendedUnassignedWitness, owner_witness_sealed,
    },
    domain::native_dispatch::{DispatchError, NativeOwnerKind},
};
use std::{
    ffi::OsString,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
};
use tokio::{io::AsyncReadExt, task::JoinHandle, time::Instant};

pub struct WindowsLaunchPlan {
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
    pub cwd: PathBuf,
    pub environment: Vec<(OsString, OsString)>,
}
#[derive(Default)]
pub struct WindowsSourceLauncher;
pub struct WindowsOwnedSource {
    process: TrackedProcessOwner,
    entered: Arc<EnteredLaunch>,
    stderr: tokio::sync::Mutex<Option<Arc<StderrTask>>>,
    stopping: AtomicBool,
}
pub struct WindowsRunningProof {
    entered: Arc<EnteredLaunch>,
    proof: RunningProcessProof,
}
struct StderrTask {
    handle: Mutex<Option<JoinHandle<()>>>,
    installed: AtomicBool,
    changed: tokio::sync::Notify,
    join_owner: tokio::sync::Mutex<()>,
}
impl StderrTask {
    fn install<R: tokio::io::AsyncRead + Unpin + Send + 'static>(
        slot: &mut Option<Arc<Self>>,
        mut stderr: R,
    ) {
        let record = Arc::new(Self {
            handle: Mutex::new(None),
            installed: AtomicBool::new(false),
            changed: tokio::sync::Notify::new(),
            join_owner: tokio::sync::Mutex::new(()),
        });
        // The original owner contains the stable task slot before its future can first poll.
        *slot = Some(record.clone());
        let task = tokio::spawn(async move {
            let mut bytes = [0u8; 8192];
            loop {
                match stderr.read(&mut bytes).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });
        match record.handle.lock() {
            Ok(mut handle) => *handle = Some(task),
            Err(error) => *error.into_inner() = Some(task),
        }
        record.installed.store(true, Ordering::Release);
        record.changed.notify_waiters();
    }
    async fn join(&self, deadline: Instant) -> bool {
        tokio::time::timeout_at(deadline, async {
            let _join_owner = self.join_owner.lock().await;
            loop {
                let changed = self.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if self.installed.load(Ordering::Acquire) {
                    break;
                }
                changed.await;
            }
            std::future::poll_fn(|cx| {
                let mut slot = match self.handle.lock() {
                    Ok(slot) => slot,
                    Err(error) => error.into_inner(),
                };
                let Some(handle) = slot.as_mut() else {
                    return Poll::Ready(());
                };
                if Pin::new(handle).poll(cx).is_pending() {
                    return Poll::Pending;
                }
                slot.take();
                drop(slot);
                Poll::Ready(())
            })
            .await;
        })
        .await
        .is_ok()
    }
}
impl owner_witness_sealed::Sealed for NoProcessCreatedProof {}
impl NeverCreatedWitness for NoProcessCreatedProof {
    fn cut(&self) -> SourceCreationCut {
        match self.cut() {
            ProcessCreationCut::BeforeCreateCall => SourceCreationCut::BeforeCreateCall,
            ProcessCreationCut::CreateCallRejected => SourceCreationCut::CreateCallRejected,
        }
    }
}
impl owner_witness_sealed::Sealed for SuspendedUnassignedClosedProof {}
impl SuspendedUnassignedWitness for SuspendedUnassignedClosedProof {
    fn root_exit_code(&self) -> u32 {
        self.root_exit_code()
    }
}
const fn process_error(error: &ProcessOwnerError) -> DispatchError {
    match error {
        ProcessOwnerError::Windows { operation, code } => DispatchError::SourcePlatform {
            operation,
            code: *code,
        },
        ProcessOwnerError::InvalidSpec(_) => DispatchError::InvalidConfiguration,
        ProcessOwnerError::AlreadyTaken | ProcessOwnerError::SpawnTimeout => {
            DispatchError::ResourceUnconfirmed
        }
    }
}
impl SourceLaunchPort for WindowsSourceLauncher {
    type Plan = WindowsLaunchPlan;
    type Owned = WindowsOwnedSource;
    type Running = WindowsRunningProof;
    fn verify_running(
        &self,
        owner: &WindowsOwnedSource,
        proof: &WindowsRunningProof,
        controller: &Arc<SourceLaunchController>,
    ) -> Result<(), DispatchError> {
        owner.entered.require_original(controller)?;
        proof.entered.require_original(controller)?;
        if !Arc::ptr_eq(&owner.entered, &proof.entered)
            || !owner.process.confirms_running(&proof.proof)
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        Ok(())
    }
    fn owner_kind(&self) -> NativeOwnerKind {
        NativeOwnerKind::WindowsJob
    }
    fn same_plan(&self, a: &WindowsLaunchPlan, b: &WindowsLaunchPlan) -> bool {
        a.executable == b.executable
            && a.arguments == b.arguments
            && a.cwd == b.cwd
            && a.environment == b.environment
    }
    fn launch(
        &self,
        p: Arc<WindowsLaunchPlan>,
        permit: LaunchPermit,
        adopter: LaunchAdopter<WindowsOwnedSource>,
        deadline: Instant,
    ) -> BoxFuture<'_, LaunchCompletion<WindowsRunningProof>> {
        launch_windows(
            p,
            permit,
            adopter,
            deadline,
            #[cfg(test)]
            None,
        )
    }
}
#[cfg(test)]
impl WindowsSourceLauncher {
    #[must_use]
    pub fn launch_with_hooks_for_test(
        &self,
        plan: Arc<WindowsLaunchPlan>,
        permit: LaunchPermit,
        adopter: LaunchAdopter<WindowsOwnedSource>,
        deadline: Instant,
        hooks: super::process_owner::TrackedSpawnHooks,
    ) -> BoxFuture<'static, LaunchCompletion<WindowsRunningProof>> {
        launch_windows(plan, permit, adopter, deadline, Some(hooks))
    }
}
fn launch_windows(
    p: Arc<WindowsLaunchPlan>,
    permit: LaunchPermit,
    adopter: LaunchAdopter<WindowsOwnedSource>,
    deadline: Instant,
    #[cfg(test)] hooks: Option<super::process_owner::TrackedSpawnHooks>,
) -> BoxFuture<'static, LaunchCompletion<WindowsRunningProof>> {
    Box::pin(async move {
        let prepared = match OwnedProcess::prepare_tracked(&ProcessSpec {
            executable: p.executable.clone(),
            args: p.arguments.clone(),
            cwd: p.cwd.clone(),
            env: p.environment.clone(),
        }) {
            Ok(prepared) => prepared,
            Err(error) => {
                return LaunchCompletion::BeforeEntryFailed {
                    error: process_error(&error),
                    permit,
                };
            }
        };
        #[cfg(test)]
        let prepared = if let Some(hooks) = hooks {
            prepared.with_hooks_for_test(hooks)
        } else {
            prepared
        };
        let Ok(entered) = permit.enter() else {
            return LaunchCompletion::Revoked;
        };
        if let Err(error) = entered.require_entered() {
            return LaunchCompletion::EnteredUnconfirmed(error);
        }
        let entered = Arc::new(entered);
        let (process, driver) = prepared.into_owner_driver();
        adopter.adopt(WindowsOwnedSource {
            process,
            entered: entered.clone(),
            stderr: tokio::sync::Mutex::new(None),
            stopping: AtomicBool::new(false),
        });
        match driver.run(deadline).await {
            ProcessSpawnCompletion::Running(proof) => {
                LaunchCompletion::OwnedRunning(WindowsRunningProof { entered, proof })
            }
            ProcessSpawnCompletion::Failed(error) => {
                LaunchCompletion::OwnedFailed(process_error(&error))
            }
        }
    })
}
impl OwnedNativeSource for WindowsOwnedSource {
    type Streams = AcpStreams;
    fn take_streams(&mut self) -> Result<AcpStreams, DispatchError> {
        if self.stopping.load(Ordering::SeqCst) {
            return Err(DispatchError::Stopped);
        }
        let io = self
            .process
            .take_io()
            .map_err(|_| DispatchError::ProtocolUnavailable)?;
        let mut slot = self
            .stderr
            .try_lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        if slot.is_some() {
            return Err(DispatchError::Conflict);
        }
        StderrTask::install(&mut slot, io.stderr);
        drop(slot);
        Ok(AcpStreams::from_owned(io.stdout, io.stdin))
    }
    fn owner_kind(&self) -> NativeOwnerKind {
        NativeOwnerKind::WindowsJob
    }
    fn request_stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.process.request_stop();
    }
    fn shutdown(
        &mut self,
        deadline: Instant,
    ) -> BoxFuture<'_, Result<OwnedSourceShutdown, DispatchError>> {
        Box::pin(async move {
            self.request_stop();
            // Job termination must precede a potentially stuck pipe/stderr join.
            let report = self.process.shutdown(deadline).await;
            match report {
                TrackedShutdownReport::NeverCreated(proof) => {
                    if !self.process.confirms_never_created(&proof) {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    return Ok(OwnedSourceShutdown::NeverCreated(Arc::new(
                        OwnedNeverCreatedClosure::from_actual(self.entered.clone(), proof),
                    )));
                }
                TrackedShutdownReport::SuspendedUnassigned(proof) => {
                    if !self.process.confirms_suspended(&proof) {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    return Ok(OwnedSourceShutdown::SuspendedUnassigned(Arc::new(
                        OwnedSuspendedUnassignedClosure::from_actual(self.entered.clone(), proof),
                    )));
                }
                TrackedShutdownReport::Unconfirmed => {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                TrackedShutdownReport::AssignedJob(_) => {}
            }
            let TrackedShutdownReport::AssignedJob(report) = report else {
                return Err(DispatchError::ResourceUnconfirmed);
            };
            let mut slot = self.stderr.lock().await;
            let stderr_joined = if let Some(task) = slot.as_ref() {
                task.join(deadline).await
            } else {
                true
            };
            if stderr_joined {
                slot.take();
            }
            drop(slot);
            Ok(OwnedSourceShutdown::WindowsJob {
                active_processes: report.active_processes,
                root_exit_code: report.root_exit_code,
                stderr_joined,
            })
        })
    }
}
