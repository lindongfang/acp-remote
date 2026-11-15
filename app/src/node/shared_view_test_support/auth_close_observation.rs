//! Default-off fixture facts at the original explicit close return boundaries.
// Deliberately retain the crate ceiling even inside the current private parent.
#![allow(clippy::redundant_pub_crate)]
use super::LocalViewAuthFixture;
use crate::shared_view_test_support::{AreaCleanupReport, FixtureError, FixtureOwnerReport};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClosePhase {
    NotObserved,
    AuthorityReturned,
    OwnerReturned,
    AreaReturned,
    FirstWaitCancelled,
    ObservationUnavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErasureSite {
    OwnerNotJoined,
    AreaCleanupError,
    AreaNotRemoved,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OwnerObservation {
    tasks: usize,
    resources: usize,
    failure: Option<FixtureError>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AreaObservation {
    Returned { removed: bool, pending: usize },
    Failed(FixtureError),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FirstCloseObservation {
    phase: ClosePhase,
    owner: Option<OwnerObservation>,
    area: Option<AreaObservation>,
    erasure: Option<ErasureSite>,
}
impl FirstCloseObservation {
    const fn empty(phase: ClosePhase) -> Self {
        Self {
            phase,
            owner: None,
            area: None,
            erasure: None,
        }
    }
}
#[cfg(feature = "shared-view-acceptance-fixtures")]
impl FirstCloseObservation {
    #[must_use]
    pub const fn phase(self) -> ClosePhase {
        self.phase
    }
    #[must_use]
    pub const fn owner(self) -> Option<OwnerObservation> {
        self.owner
    }
    #[must_use]
    pub const fn area(self) -> Option<AreaObservation> {
        self.area
    }
    #[must_use]
    pub const fn erasure(self) -> Option<ErasureSite> {
        self.erasure
    }
}
#[cfg(feature = "shared-view-acceptance-fixtures")]
impl OwnerObservation {
    #[must_use]
    pub const fn pending_tasks(self) -> usize {
        self.tasks
    }
    #[must_use]
    pub const fn pending_resources(self) -> usize {
        self.resources
    }
    #[must_use]
    pub const fn failure(self) -> Option<FixtureError> {
        self.failure
    }
}
pub(super) struct FirstCloseObserver {
    claimed: AtomicBool,
    snapshot: Mutex<FirstCloseObservation>,
}
impl FirstCloseObserver {
    pub(super) const fn new() -> Self {
        Self {
            claimed: AtomicBool::new(false),
            snapshot: Mutex::new(FirstCloseObservation::empty(ClosePhase::NotObserved)),
        }
    }
    pub(super) fn begin_first(&self) -> FirstCloseGuard<'_> {
        FirstCloseGuard {
            observer: self,
            selected: self
                .claimed
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok(),
            finished: false,
        }
    }
    fn update(&self, update: impl FnOnce(&mut FirstCloseObservation)) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            update(&mut snapshot);
        }
    }
    fn read(&self) -> FirstCloseObservation {
        self.snapshot.lock().map_or_else(
            |_| FirstCloseObservation::empty(ClosePhase::ObservationUnavailable),
            |value| *value,
        )
    }
}
pub(super) struct FirstCloseGuard<'a> {
    observer: &'a FirstCloseObserver,
    selected: bool,
    finished: bool,
}
impl FirstCloseGuard<'_> {
    pub(super) fn authority_returned(&self) {
        if self.selected {
            self.observer
                .update(|value| value.phase = ClosePhase::AuthorityReturned);
        }
    }
    pub(super) fn owner_returned(&self, report: &FixtureOwnerReport) {
        if self.selected {
            self.observer.update(|value| {
                value.phase = ClosePhase::OwnerReturned;
                value.owner = Some(OwnerObservation {
                    tasks: report.pending_tasks(),
                    resources: report.pending_resources(),
                    failure: report.failure(),
                });
            });
        }
    }
    pub(super) fn area_returned(&self, report: &Result<AreaCleanupReport, FixtureError>) {
        if self.selected {
            self.observer.update(|value| {
                value.phase = ClosePhase::AreaReturned;
                value.area = Some(match report {
                    Ok(report) => AreaObservation::Returned {
                        removed: report.removed(),
                        pending: report.pending_resources(),
                    },
                    Err(error) => AreaObservation::Failed(*error),
                });
            });
        }
    }
    pub(super) fn returning(&mut self, site: ErasureSite) {
        if self.selected {
            self.observer.update(|value| value.erasure = Some(site));
        }
        self.finished = true;
    }
    pub(super) const fn normal_return(&mut self) {
        self.finished = true;
    }
}
impl Drop for FirstCloseGuard<'_> {
    fn drop(&mut self) {
        if self.selected && !self.finished {
            self.observer
                .update(|value| value.phase = ClosePhase::FirstWaitCancelled);
        }
    }
}
pub(crate) fn first_close_snapshot(fixture: &LocalViewAuthFixture) -> FirstCloseObservation {
    fixture.close_observer.read()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_view_test_support::{FixtureTaskOwner, OwnedTestArea};
    use std::{
        future::{Future, poll_fn},
        task::Poll,
    };
    use tokio::time::Instant as TokioInstant;

    fn area() -> OwnedTestArea {
        let area = OwnedTestArea::create(&std::env::temp_dir(), "shared-auth-close-observation")
            .expect("self-owned child");
        let tls = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
            .expect("synthetic cert");
        area.install_tls(
            tls.cert.pem().as_bytes(),
            tls.signing_key.serialize_pem().as_bytes(),
        )
        .expect("create_new TLS");
        area
    }
    #[tokio::test]
    async fn shared_view_close_observation_real_normal_return_is_not_cancelled() {
        let fixture = LocalViewAuthFixture::start(area())
            .await
            .expect("actual Node");
        fixture.close().await.expect("original explicit close");
        let first = first_close_snapshot(&fixture);
        eprintln!("[AUTH-CLOSE-FIRST] {first:?}");
        assert_eq!(first.phase, ClosePhase::AreaReturned);
        assert_eq!(first.erasure, None);
        assert_eq!(
            first.owner,
            Some(OwnerObservation {
                tasks: 0,
                resources: 0,
                failure: None
            })
        );
        assert_eq!(
            first.area,
            Some(AreaObservation::Returned {
                removed: true,
                pending: 0
            })
        );
    }
    #[tokio::test]
    async fn shared_view_close_observation_real_normal_error_is_not_cancelled_or_overwritten() {
        let area = area();
        let blocker = FixtureTaskOwner::new(&area, tokio::runtime::Handle::current())
            .expect("actual additional area owner");
        let fixture = LocalViewAuthFixture::start(area)
            .await
            .expect("actual Node");
        let original = fixture.close().await;
        let first = first_close_snapshot(&fixture);
        let released = blocker
            .shutdown(TokioInstant::now() + super::super::SHUTDOWN_TIMEOUT)
            .await;
        let cleanup = fixture.close().await;
        eprintln!("[AUTH-CLOSE-FIRST] {first:?}");
        assert!(released.all_joined());
        cleanup.expect("explicit new caller after actual other owner release");
        assert_eq!(
            original,
            Err(crate::domain::shared_view::ViewError::ReadFailure)
        );
        assert_eq!(first.phase, ClosePhase::AreaReturned);
        assert_eq!(first.erasure, Some(ErasureSite::AreaNotRemoved));
        assert!(matches!(
            first.area,
            Some(AreaObservation::Returned {
                removed: false,
                pending: 1..
            })
        ));
        assert_eq!(first_close_snapshot(&fixture), first);
    }
    #[tokio::test]
    async fn shared_view_close_observation_real_first_pending_cancellation_is_preserved() {
        let fixture = LocalViewAuthFixture::start(area())
            .await
            .expect("actual Node");
        let mut first = Box::pin(fixture.close());
        assert_eq!(
            first_close_snapshot(&fixture).phase,
            ClosePhase::NotObserved
        );
        let returned = poll_fn(|cx| Poll::Ready(first.as_mut().poll(cx))).await;
        drop(first);
        let cancelled = first_close_snapshot(&fixture);
        let cleanup = fixture.close().await;
        eprintln!("[AUTH-CLOSE-FIRST] {cancelled:?}");
        cleanup.expect("actual remaining owner wait and first removal");
        assert!(
            returned.is_pending(),
            "only actual Pending is a cancellation observation"
        );
        assert_eq!(cancelled.phase, ClosePhase::FirstWaitCancelled);
        assert_eq!(cancelled.erasure, None);
        assert_eq!(first_close_snapshot(&fixture), cancelled);
    }
    #[test]
    fn shared_view_close_observation_concurrent_claim_and_poison_do_not_change_close() {
        let observer = FirstCloseObserver::new();
        let mut first = observer.begin_first();
        first.authority_returned();
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _later = observer.begin_first();
                })
                .join()
                .expect("observer contender");
        });
        first.normal_return();
        drop(first);
        assert_eq!(observer.read().phase, ClosePhase::AuthorityReturned);
        let _ = std::panic::catch_unwind(|| {
            let _guard = observer.snapshot.lock().expect("own observer");
            panic!("synthetic observer poison");
        });
        assert_eq!(observer.read().phase, ClosePhase::ObservationUnavailable);
        let mut later = observer.begin_first();
        later.normal_return();
    }
}
