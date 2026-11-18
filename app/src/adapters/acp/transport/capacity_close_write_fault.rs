//! One-shot test faults on an original automatic-close command.
//! The command owns a claimed fault even if the installing guard is dropped.
use super::{
    AcpError, Connection, OutboundKind, RequestKind, Reservation, WriteIdentity, WriteStep,
    state::Core,
};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseWriteFaultMode {
    BeforeWrite,
    AfterOneByte,
    BeforeFlush,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CloseWriteFaultObservation {
    pub claimed: bool,
    pub fired: bool,
    pub accepted: usize,
    pub total: usize,
}

pub(super) struct CloseWriteFault {
    native: String,
    mode: CloseWriteFaultMode,
    claimed: AtomicBool,
    fired: AtomicBool,
    accepted: AtomicUsize,
    total: AtomicUsize,
}

pub struct CloseWriteFaultGuard {
    core: Weak<Core>,
    fault: Arc<CloseWriteFault>,
}

impl CloseWriteFaultGuard {
    pub(crate) fn observation(&self) -> CloseWriteFaultObservation {
        CloseWriteFaultObservation {
            claimed: self.fault.claimed.load(Ordering::SeqCst),
            fired: self.fault.fired.load(Ordering::SeqCst),
            accepted: self.fault.accepted.load(Ordering::SeqCst),
            total: self.fault.total.load(Ordering::SeqCst),
        }
    }
}

impl Drop for CloseWriteFaultGuard {
    fn drop(&mut self) {
        if let Some(core) = self.core.upgrade() {
            let mut pending = match core.capacity_close_fault.lock() {
                Ok(pending) => pending,
                Err(poisoned) => poisoned.into_inner(),
            };
            if pending
                .as_ref()
                .is_some_and(|fault| Arc::ptr_eq(fault, &self.fault))
            {
                *pending = None;
            }
        }
    }
}

impl Connection {
    pub(crate) fn capacity_install_close_write_fault(
        &self,
        native: &str,
        mode: CloseWriteFaultMode,
    ) -> Result<CloseWriteFaultGuard, AcpError> {
        if native.is_empty() {
            return Err(AcpError::InvalidMessage);
        }
        let fault = Arc::new(CloseWriteFault {
            native: native.to_owned(),
            mode,
            claimed: AtomicBool::new(false),
            fired: AtomicBool::new(false),
            accepted: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
        });
        let mut pending = self
            .core
            .capacity_close_fault
            .lock()
            .map_err(|_| AcpError::Io)?;
        if pending.is_some() {
            return Err(AcpError::InvalidMessage);
        }
        *pending = Some(fault.clone());
        Ok(CloseWriteFaultGuard {
            core: Arc::downgrade(&self.core),
            fault,
        })
    }
}

impl super::super::native_dispatch::AcpSource {
    #[allow(
        dead_code,
        reason = "Node L6 entrypoints are optional test-only consumers of this installation port"
    )]
    pub(crate) fn capacity_install_close_write_fault(
        &self,
        native: &str,
        mode: CloseWriteFaultMode,
    ) -> Result<CloseWriteFaultGuard, crate::domain::native_dispatch::DispatchError> {
        self.connection()?
            .capacity_install_close_write_fault(native, mode)
            .map_err(|_| crate::domain::native_dispatch::DispatchError::ProtocolUnavailable)
    }
}

pub(super) fn claim(
    core: &Core,
    identity: &WriteIdentity,
    automatic: bool,
    total: usize,
) -> Option<Arc<CloseWriteFault>> {
    if !automatic {
        return None;
    }
    let OutboundKind::Request(RequestKind::Close(native)) = identity.kind() else {
        return None;
    };
    let mut pending = match core.capacity_close_fault.lock() {
        Ok(pending) => pending,
        Err(poisoned) => poisoned.into_inner(),
    };
    if pending
        .as_ref()
        .is_none_or(|fault| fault.native != native.as_str())
    {
        return None;
    }
    let fault = pending.take()?;
    fault.total.store(total, Ordering::SeqCst);
    fault.claimed.store(true, Ordering::SeqCst);
    Some(fault)
}

impl CloseWriteFault {
    pub(super) fn reject_before_poll(&self, step: WriteStep) -> bool {
        if self.mode == CloseWriteFaultMode::BeforeWrite
            && matches!(step, WriteStep::Chunk { offset: 0, .. })
        {
            self.fired.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }

    pub(super) fn write_end(&self, offset: usize, total: usize) -> Result<usize, AcpError> {
        if self.mode == CloseWriteFaultMode::AfterOneByte {
            if offset > 0 {
                self.fired.store(true, Ordering::SeqCst);
                return Err(AcpError::Io);
            }
            return Ok(total.min(1));
        }
        Ok(total)
    }

    pub(super) fn fail_flush(&self) -> bool {
        if self.mode == CloseWriteFaultMode::BeforeFlush {
            self.fired.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }

    pub(super) fn accepted(&self, count: usize) {
        self.accepted.store(count, Ordering::SeqCst);
    }
}

/// 单次 Resume 入队故障观测状态，仅绑定目标 `native` 会话。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResumeEnqueueFaultObservation {
    pub claimed: bool,
    pub fired: bool,
}

pub(super) struct ResumeEnqueueFault {
    native: String,
    claimed: AtomicBool,
    fired: AtomicBool,
}

/// 绑定单源连接与单一目标会话的 Resume 入队拒绝守卫；未消费 Drop 时自动清除。
pub struct ResumeEnqueueFaultGuard {
    core: Weak<Core>,
    fault: Arc<ResumeEnqueueFault>,
}

impl ResumeEnqueueFaultGuard {
    /// 读取故障守卫是否已被目标会话入队消费并触发。
    #[must_use]
    pub fn observation(&self) -> ResumeEnqueueFaultObservation {
        ResumeEnqueueFaultObservation {
            claimed: self.fault.claimed.load(Ordering::SeqCst),
            fired: self.fault.fired.load(Ordering::SeqCst),
        }
    }
}

impl Drop for ResumeEnqueueFaultGuard {
    fn drop(&mut self) {
        if let Some(core) = self.core.upgrade() {
            let mut pending = match core.capacity_resume_fault.lock() {
                Ok(pending) => pending,
                Err(poisoned) => poisoned.into_inner(),
            };
            if pending
                .as_ref()
                .is_some_and(|fault| Arc::ptr_eq(fault, &self.fault))
            {
                *pending = None;
            }
        }
    }
}

impl Connection {
    /// 在连接上挂载仅针对指定 `native` 会话的单次 `session/resume` 入队拒绝守卫。
    pub(crate) fn capacity_install_resume_enqueue_fault(
        &self,
        native: &str,
    ) -> Result<ResumeEnqueueFaultGuard, AcpError> {
        if native.is_empty() {
            return Err(AcpError::InvalidMessage);
        }
        let fault = Arc::new(ResumeEnqueueFault {
            native: native.to_owned(),
            claimed: AtomicBool::new(false),
            fired: AtomicBool::new(false),
        });
        let mut pending = self
            .core
            .capacity_resume_fault
            .lock()
            .map_err(|_| AcpError::Io)?;
        if pending.is_some() {
            return Err(AcpError::InvalidMessage);
        }
        *pending = Some(fault.clone());
        Ok(ResumeEnqueueFaultGuard {
            core: Arc::downgrade(&self.core),
            fault,
        })
    }
}

impl super::super::native_dispatch::AcpSource {
    /// 从原生源入口挂载单次 `session/resume` 入队拒绝守卫。
    #[allow(
        dead_code,
        reason = "Node L6 entrypoints are optional test-only consumers of this installation port"
    )]
    pub(crate) fn capacity_install_resume_enqueue_fault(
        &self,
        native: &str,
    ) -> Result<ResumeEnqueueFaultGuard, crate::domain::native_dispatch::DispatchError> {
        self.connection()?
            .capacity_install_resume_enqueue_fault(native)
            .map_err(|_| crate::domain::native_dispatch::DispatchError::ProtocolUnavailable)
    }
}

/// 若连接挂载了匹配当前 `Resume` 预留目标会话的测试故障，则原子消费一次并返回 `true`。
#[must_use]
pub(crate) fn claim_resume_enqueue_fault(
    connection: &Connection,
    reservation: &Reservation,
) -> bool {
    let OutboundKind::Request(RequestKind::Resume(native)) = reservation.kind() else {
        return false;
    };
    let mut pending = match connection.core.capacity_resume_fault.lock() {
        Ok(pending) => pending,
        Err(poisoned) => poisoned.into_inner(),
    };
    if pending
        .as_ref()
        .is_none_or(|fault| fault.native != native.as_str())
    {
        return false;
    }
    let Some(fault) = pending.take() else {
        return false;
    };
    fault.claimed.store(true, Ordering::SeqCst);
    fault.fired.store(true, Ordering::SeqCst);
    true
}
