//! Thread-local observation around the real parser and full-byte SHA.
use std::{cell::RefCell, sync::Arc};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Point {
    RawParsedBeforeBlockChecks,
    DigestComputed,
}
type Observer = Arc<dyn Fn(Point) + Send + Sync>;
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
pub(super) fn point(p: Point) {
    let observer = OBSERVER.with(|s| s.borrow().clone());
    if let Some(o) = observer {
        o(p);
    }
}
