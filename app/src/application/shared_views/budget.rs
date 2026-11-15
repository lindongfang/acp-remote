//! A selected absolute IO deadline belongs to one operation, never the lifetime of a service.
use crate::domain::{
    native::NativeSessionKey,
    shared_view::{ReadEpoch, ViewError},
};
use std::time::Duration;
use tokio::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadReason {
    FirstOpen,
    ExplicitRefresh,
    Reopen,
    Reconnect,
    Reacquire,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadResponseWait {
    At(Instant),
    Unbounded,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadOptions {
    pub write_deadline: Instant,
    pub response_wait: ReadResponseWait,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadResponseBudget {
    After(Duration),
    Unbounded,
}

pub struct NativeReadStartContext<'a> {
    target: &'a NativeSessionKey,
    reason: ReadReason,
    epoch: ReadEpoch,
    now: Instant,
}
impl<'a> NativeReadStartContext<'a> {
    /// Pure scheduling metadata; construction does not produce a protocol binding or receipt.
    #[must_use]
    pub(crate) fn sample(
        target: &'a NativeSessionKey,
        reason: ReadReason,
        epoch: ReadEpoch,
    ) -> Self {
        Self {
            target,
            reason,
            epoch,
            now: Instant::now(),
        }
    }
    #[must_use]
    pub const fn target(&self) -> &NativeSessionKey {
        self.target
    }
    #[must_use]
    pub const fn reason(&self) -> ReadReason {
        self.reason
    }
    #[must_use]
    pub const fn read_epoch(&self) -> ReadEpoch {
        self.epoch
    }
    #[must_use]
    pub const fn now(&self) -> Instant {
        self.now
    }
}
pub trait NativeReadOptionsProvider: Send + Sync + 'static {
    /// # Errors
    /// Reports an actual inability to choose representable operation-local IO options.
    fn for_new_operation(
        &self,
        context: &NativeReadStartContext<'_>,
    ) -> Result<NativeReadOptions, ViewError>;
}
pub struct MonotonicReadOptionsProvider {
    write_wait: Duration,
    response: ReadResponseBudget,
}
impl MonotonicReadOptionsProvider {
    #[must_use]
    pub const fn new(write_wait: Duration, response: ReadResponseBudget) -> Self {
        Self {
            write_wait,
            response,
        }
    }
}
impl NativeReadOptionsProvider for MonotonicReadOptionsProvider {
    fn for_new_operation(
        &self,
        context: &NativeReadStartContext<'_>,
    ) -> Result<NativeReadOptions, ViewError> {
        let write_deadline = context
            .now
            .checked_add(self.write_wait)
            .ok_or(ViewError::ReadFailure)?;
        let response_wait = match self.response {
            ReadResponseBudget::After(wait) => ReadResponseWait::At(
                context
                    .now
                    .checked_add(wait)
                    .ok_or(ViewError::ReadFailure)?,
            ),
            ReadResponseBudget::Unbounded => ReadResponseWait::Unbounded,
        };
        Ok(NativeReadOptions {
            write_deadline,
            response_wait,
        })
    }
}
