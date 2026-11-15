//! Original process adoption: original Job, ACP IO and stderr have separate actual joins.
use super::{
    AdoptRejected, FixtureError, FixtureNodeIo, FixtureOwnerHandle, FixturePeerWire,
    FixtureResourceLease, IoOwner, WireResource,
};
use crate::platform::process_owner::{OwnedProcess, ShutdownReport};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Mutex as AsyncMutex;
use tokio::{
    io::AsyncReadExt,
    sync::watch,
    time::{Instant, timeout_at},
};
pub enum ChildWireAdoptError {
    BeforeAdopt(AdoptRejected<OwnedProcess>),
    Adopted {
        error: FixtureError,
        cleanup: FixtureResourceLease,
    },
}
struct ChildResource {
    child: AsyncMutex<Option<OwnedProcess>>,
    node: Mutex<Option<FixtureNodeIo>>,
    io: Mutex<Option<Arc<IoOwner>>>,
    node_taken: AtomicBool,
    unavailable: AtomicBool,
    stop: watch::Sender<bool>,
    closed: AtomicBool,
    stderr: Mutex<Option<Arc<super::super::TaskSlot>>>,
    area: Arc<super::super::AreaRegistry>,
    stderr_closed_without_task: AtomicBool,
    report: Mutex<Option<ShutdownReport>>,
}
impl FixturePeerWire {
    /// # Errors
    /// Before adoption returns the original process; later failures retain it in the same
    /// registered resource and return its cleanup lease, without claiming any join.
    #[allow(
        clippy::result_large_err,
        reason = "The frozen adoption error returns the original process without allocating merely to report an error"
    )]
    pub fn from_child_owned(
        owner: &FixtureOwnerHandle,
        child: OwnedProcess,
    ) -> Result<Self, ChildWireAdoptError> {
        let Ok(mut resources) = owner.inner.resources.lock() else {
            return Err(ChildWireAdoptError::BeforeAdopt(AdoptRejected {
                error: FixtureError::JoinUnconfirmed,
                original: child,
            }));
        };
        if owner.inner.released.load(Ordering::Acquire) || *owner.inner.stop.borrow() {
            return Err(ChildWireAdoptError::BeforeAdopt(AdoptRejected {
                error: FixtureError::Cancelled,
                original: child,
            }));
        }
        if resources.try_reserve(1).is_err() {
            return Err(ChildWireAdoptError::BeforeAdopt(AdoptRejected {
                error: FixtureError::Allocation,
                original: child,
            }));
        }
        let (stop, _) = watch::channel(false);
        let resource = Arc::new(ChildResource {
            child: AsyncMutex::new(Some(child)),
            node: Mutex::new(None),
            io: Mutex::new(None),
            node_taken: AtomicBool::new(false),
            unavailable: AtomicBool::new(false),
            stop,
            closed: AtomicBool::new(false),
            stderr: Mutex::new(None),
            area: Arc::clone(&owner.inner.area),
            report: Mutex::new(None),
            stderr_closed_without_task: AtomicBool::new(false),
        });
        let adopted: Arc<dyn WireResource> = resource.clone();
        resources.push(Arc::clone(&adopted));
        let cleanup = FixtureResourceLease {
            resource: Arc::clone(&adopted),
        };
        // The resource registry remains locked until all synchronous adoption steps finish;
        // a concurrent owner shutdown cannot mistake this in-progress transfer for closed IO.
        let parts = resource.child.try_lock().map_or_else(
            |_| Err(FixtureError::JoinUnconfirmed),
            |mut process| {
                process
                    .as_mut()
                    .ok_or(FixtureError::JoinUnconfirmed)
                    .and_then(|process| process.take_io().map_err(|_| FixtureError::AlreadyTaken))
            },
        );
        let parts = match parts {
            Ok(parts) => parts,
            Err(error) => {
                resource.unavailable.store(true, Ordering::Release);
                return Err(ChildWireAdoptError::Adopted { error, cleanup });
            }
        };
        let mut node = match resource.node.lock() {
            Ok(node) => node,
            Err(poison) => poison.into_inner(),
        };
        *node = Some(FixtureNodeIo::Child {
            reader: parts.stdout,
            writer: parts.stdin,
        });
        drop(node);
        let mut stderr = parts.stderr;
        let mut stop = resource.stop.subscribe();
        let task=owner.spawn_registered(async move {
            let mut bytes=[0;8192];
            loop {
                if *stop.borrow() {return Ok(());}
                tokio::select! {
                    read=stderr.read(&mut bytes)=>match read {Ok(0)=>return Ok(()),Ok(_)=>{},Err(_)=>return Err(FixtureError::Io)},
                    _ = stop.changed()=>{},
                }
            }
        });
        let task = match task {
            Ok(task) => task,
            Err(error) => {
                resource
                    .stderr_closed_without_task
                    .store(true, Ordering::Release);
                return Err(ChildWireAdoptError::Adopted { error, cleanup });
            }
        };
        match resource.stderr.lock() {
            Ok(mut slot) => *slot = Some(task),
            Err(poison) => *poison.into_inner() = Some(task),
        }
        drop(resources);
        Ok(Self {
            resource: adopted,
            owner: owner.clone(),
            handed_off: false,
        })
    }
}
impl ChildResource {
    async fn join_stderr(&self, deadline: Instant) -> bool {
        let slot = match self.stderr.lock() {
            Ok(slot) => slot.clone(),
            Err(poison) => poison.into_inner().clone(),
        };
        let Some(slot) = slot else {
            return self.stderr_closed_without_task.load(Ordering::Acquire);
        };
        matches!(
            super::super::join_registered_slot(&slot, &self.area, deadline).await,
            Ok(Ok(()))
        )
    }
    async fn close_actual(&self, deadline: Instant) -> bool {
        self.request_stop();
        if self.closed() {
            return true;
        }
        if !self.node_taken.load(Ordering::Acquire) {
            match self.node.lock() {
                Ok(mut node) => {
                    node.take();
                }
                Err(poison) => {
                    poison.into_inner().take();
                }
            }
        }
        let io = match self.io.lock() {
            Ok(io) => io.clone(),
            Err(poison) => poison.into_inner().clone(),
        };
        // All three futures remain owned here. In particular, a blocked ACP join is not
        // awaited before the original process owner can terminate its actual Job.
        let process = async {
            let mut owner = self.child.lock().await;
            let Some(child) = owner.as_mut() else {
                return false;
            };
            let closed = child.shutdown(deadline).await.is_ok_and(|report| {
                let zero = report.active_processes == 0;
                match self.report.lock() {
                    Ok(mut slot) => *slot = Some(report),
                    Err(poison) => *poison.into_inner() = Some(report),
                }
                zero
            });
            drop(owner);
            closed
        };
        let protocol = async {
            match io {
                Some(io) => io.shutdown(deadline).await.joined,
                None => !self.node_taken.load(Ordering::Acquire),
            }
        };
        let completed = timeout_at(deadline, async {
            tokio::join!(process, protocol, self.join_stderr(deadline))
        })
        .await;
        if matches!(completed, Ok((true, true, true))) && !self.unavailable.load(Ordering::Acquire)
        {
            self.closed.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }
}
impl WireResource for ChildResource {
    #[cfg(test)]
    fn process_report(&self) -> Option<ShutdownReport> {
        match self.report.lock() {
            Ok(report) => *report,
            Err(poison) => *poison.into_inner(),
        }
    }
    fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    fn request_stop(&self) {
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
        Ok(node)
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
        Box::pin(self.close_actual(deadline))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapters::acp::*,
        platform::process_owner::ProcessSpec,
        shared_view_test_support::{FixtureTaskOwner, OwnedTestArea},
    };
    use std::{future::Future, task::Poll, time::Duration};
    struct OriginalInit {
        scope: ConnectionScope,
        rpc: RpcId,
    }
    impl WriteFence for OriginalInit {
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
                    OutboundKind::Request(RequestKind::Initialize(_))
                )
            {
                return Err(FenceRejection::StaleTarget);
            }
            let _ = io.poll(cx);
            Ok(())
        }
    }
    #[tokio::test]
    #[allow(
        clippy::too_many_lines,
        reason = "The original process tree, ACP IO, stderr drain and cancelled close waiter must be verified under the same owner"
    )]
    async fn shared_view_child_original_job_stdio_stderr_and_cancelled_close_wait_remain_owned() {
        let mut area = OwnedTestArea::create(&std::env::temp_dir(), "shared-view-child")
            .expect("unique owned child area");
        let owner = FixtureTaskOwner::new(&area, tokio::runtime::Handle::current())
            .expect("registered before child adoption");
        let executable = std::path::PathBuf::from(std::env::var_os("SystemRoot").expect("Windows"))
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let script = r#"
$p=Start-Process -FilePath "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -WindowStyle Hidden -ArgumentList @('-NoProfile','-NonInteractive','-Command','Start-Sleep -Seconds 60') -PassThru
[Console]::Error.WriteLine(('e'*131073))
while($null -ne ($line=[Console]::ReadLine())) {
 $r=$line|ConvertFrom-Json
 if($r.method -ne 'initialize') {exit 41}
 [Console]::WriteLine((@{jsonrpc='2.0';id=$r.id;result=@{protocolVersion=2;info=@{name='owned';version='2'};capabilities=@{session=@{}}}}|ConvertTo-Json -Compress -Depth 12))
}
"#;
        let deadline = || Instant::now() + Duration::from_secs(10);
        let child = OwnedProcess::spawn(
            ProcessSpec {
                executable,
                args: vec![
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-WindowStyle".into(),
                    "Hidden".into(),
                    "-Command".into(),
                    script.into(),
                ],
                cwd: area.path().to_path_buf(),
                env: Vec::new(),
            },
            deadline(),
        )
        .await
        .expect("actual owned Windows Job spawn");
        let mut wire = FixturePeerWire::from_child_owned(&owner.handle(), child)
            .unwrap_or_else(|_| panic!("original child adopted"));
        let (node, lease) = wire.take_node().expect("original named pipe tuple once");
        let (reader, writer) = tokio::io::split(node);
        let (connection, mut events, io) =
            Connection::spawn(reader, writer).expect("actual Scope from original pipes");
        lease
            .install_io(io)
            .unwrap_or_else(|_| panic!("original IO retained"));
        let reservation = connection
            .reserve_preferred_initialize(deadline())
            .expect("original initialize");
        let rpc = reservation.id().expect("typed RPC").clone();
        let frame = reservation.encode().expect("wire");
        let ticket = connection
            .enqueue(
                reservation,
                frame,
                Arc::new(OriginalInit {
                    scope: connection.scope(),
                    rpc,
                }),
                deadline(),
            )
            .expect("real enqueue");
        let event = timeout_at(deadline(), events.next())
            .await
            .expect("real child response wait")
            .expect("parse")
            .expect("true Received");
        assert!(matches!(
            event.incoming(),
            Incoming::Response {
                response: Response::Initialized(_),
                ..
            }
        ));
        assert_eq!(ticket.wait().await, WriteReport::Written);
        let mut first = Box::pin(lease.shutdown(deadline()));
        let pending = std::future::poll_fn(|cx| Poll::Ready(first.as_mut().poll(cx))).await;
        assert!(
            pending.is_pending(),
            "actual descendant keeps the first close unfinished"
        );
        assert!(
            connection.is_closed(),
            "first actual close poll requested ACP IO stop"
        );
        drop(first);
        let report = lease
            .shutdown(Instant::now() + Duration::from_secs(2))
            .await;
        assert!(
            report.all_joined(),
            "same original close continues after waiter cancellation"
        );
        assert_eq!(
            lease
                .resource
                .process_report()
                .expect("actual original process report")
                .active_processes,
            0,
            "root exit alone was not accepted as tree zero"
        );
        assert!(
            owner.shutdown(deadline()).await.all_joined(),
            "original stderr task and ACP IO are actually joined"
        );
        assert!(
            area.cleanup()
                .await
                .expect("no live owned task/pipe/Job")
                .removed()
        );
    }
}
