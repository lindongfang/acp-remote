//! Passive points: parsing, hashing and original task operations remain real.
use std::sync::Arc;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Point {
    BeforeAdmissionLock,
    RegisteredBeforeStart,
    CpuEntered,
    RawParsedBeforeBlockChecks,
    DigestComputed,
    CpuJoined,
    BeforeResultPublish,
    ResultPublishAttempted,
    DisposalEntered,
    DisposalJoined,
}
pub type Observer = Arc<dyn Fn(Point) + Send + Sync>;
#[derive(Clone, Default)]
pub struct Hooks {
    pub(crate) observer: Option<Observer>,
    pub(crate) queue: Option<
        Arc<
            dyn Fn(
                    crate::application::queue_owner::validation_test_hooks::Point,
                    crate::application::queue_owner::QueueRuntimeIdentity,
                ) + Send
                + Sync,
        >,
    >,
}
impl Hooks {
    pub(crate) fn point(&self, p: Point) {
        if let Some(h) = &self.observer {
            h(p);
        }
    }
}
