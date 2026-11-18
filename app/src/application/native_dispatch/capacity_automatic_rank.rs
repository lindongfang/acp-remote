//! Pressure keeps its selected tree, but rechecks its rank before actual first-byte IO.
use super::{
    Arc, CapacityResource, ConfigurationPort, DispatchError, Instant, NativeApprovalPort,
    NativeControlRepository, NativeCoordinator, NativeDispatchRepository, NativeResourceRepository,
    OwnedNativeSource, SourceLaunchPort, SourceSlot,
};

type Rank = (Instant, String, Arc<str>, String);

pub(super) fn key<P, L>(
    source: &SourceSlot<P, L>,
    capacity: &CapacityResource<P>,
    since: Instant,
) -> Rank
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
{
    (
        since,
        source.pre.metadata.source_id.encode(),
        Arc::from(capacity.pre.native.native_id()),
        capacity.pre.metadata.load_epoch.encode(),
    )
}

pub(super) struct PressureCut {
    started: Instant,
    first_other: Option<Rank>,
}

#[derive(Clone)]
pub(in crate::application::native_dispatch) struct PressureSelection {
    original: Rank,
    refresh: Arc<dyn Fn() -> Result<PressureCut, DispatchError> + Send + Sync>,
}

impl PressureSelection {
    pub(super) fn refresh(&self) -> Result<PressureCut, DispatchError> {
        (self.refresh)()
    }

    pub(super) fn permits(&self, since: Instant, cut: &PressureCut) -> bool {
        // A newly eligible competitor starts at/after the cut. Strict inequality
        // also covers clocks whose resolution gives it exactly the cut timestamp.
        if since < self.original.0 || since >= cut.started {
            return false;
        }
        let mut current = self.original.clone();
        current.0 = since;
        cut.first_other
            .as_ref()
            .is_none_or(|other| current <= *other)
    }
}

impl<P, L, G, N> NativeCoordinator<P, L, G, N>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + super::super::NativeApprovalRepository,
{
    pub(in crate::application::native_dispatch) fn automatic_pressure_selection(
        self: &Arc<Self>,
        source: &SourceSlot<P, L>,
        capacity: &CapacityResource<P>,
        selected_since: Instant,
    ) -> PressureSelection {
        let original = key(source, capacity, selected_since);
        let selected = original.clone();
        let core = Arc::downgrade(self);
        PressureSelection {
            original,
            refresh: Arc::new(move || {
                let core = core.upgrade().ok_or(DispatchError::Stopped)?;
                core.running()?;
                let started = Instant::now();
                let sources = core
                    .sources
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .clone();
                let mut first_other: Option<Rank> = None;
                for source in sources {
                    let resources: Vec<_> = source
                        .capacity
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?
                        .values()
                        .cloned()
                        .collect();
                    for capacity in resources {
                        let identity = key(&source, &capacity, started);
                        if identity.1 == selected.1
                            && identity.2 == selected.2
                            && identity.3 == selected.3
                        {
                            continue;
                        }
                        let Ok(Some(since)) = core.automatic_candidate(&source, &capacity, started)
                        else {
                            continue;
                        };
                        let other = key(&source, &capacity, since);
                        if first_other.as_ref().is_none_or(|first| other < *first) {
                            first_other = Some(other);
                        }
                    }
                }
                Ok(PressureCut {
                    started,
                    first_other,
                })
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pressure_cut_keeps_full_tie_order_and_rejects_equal_clock_or_newer_activity() {
        let initial = Instant::now();
        let later = initial + std::time::Duration::from_secs(1);
        let selected = PressureSelection {
            original: (initial, "source".into(), Arc::from("b"), "load".into()),
            refresh: Arc::new(|| Err(DispatchError::Stopped)),
        };
        assert!(selected.permits(
            initial,
            &PressureCut {
                started: later,
                first_other: None
            }
        ));
        assert!(!selected.permits(
            later,
            &PressureCut {
                started: later,
                first_other: None
            }
        ));
        assert!(!selected.permits(
            initial,
            &PressureCut {
                started: later,
                first_other: Some((initial, "source".into(), Arc::from("a"), "load".into())),
            }
        ));
        assert!(selected.permits(
            initial,
            &PressureCut {
                started: later,
                first_other: Some((initial, "source".into(), Arc::from("c"), "load".into())),
            }
        ));
    }
}

#[cfg(test)]
#[path = "runtime_child_rank_acceptance.rs"]
mod runtime_child_rank_acceptance;
