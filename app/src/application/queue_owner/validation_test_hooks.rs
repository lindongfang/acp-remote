//! Real validator boundary; opaque identity carries no body data.
use super::QueueRuntimeIdentity;
use std::{cell::RefCell, sync::Arc};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Point {
    Entered,
    Returned,
    PreparationInstalledUnderTaskFence,
    ShutdownBeforeTaskFence,
    StoppingUnderTaskFence,
    StoppedBeforeClaimRelease,
}
type Observer = Arc<dyn Fn(Point, QueueRuntimeIdentity) + Send + Sync>;
thread_local! {static OBSERVER:RefCell<Option<Observer>>=RefCell::new(None);}
pub struct Guard(Option<Observer>);
impl Drop for Guard {
    fn drop(&mut self) {
        OBSERVER.with(|s| {
            s.replace(self.0.take());
        });
    }
}
pub fn install(observer: Observer) -> Guard {
    Guard(OBSERVER.with(|s| s.replace(Some(observer))))
}
pub(super) fn point(p: Point, id: QueueRuntimeIdentity) {
    let o = OBSERVER.with(|s| s.borrow().clone());
    if let Some(o) = o {
        o(p, id);
    }
}
