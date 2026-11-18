//! One pressure attempt and one owned periodic scan share the same original release path.
use super::{
    Arc, AutomaticReleaseReason, CapacityRecord, CapacityReservation, ConfigurationPort,
    DispatchError, NativeControlRepository, NativeCoordinator, NativeDispatchRepository,
    NativeResourceRepository, OwnedNativeSource, SourceLaunchPort,
};
use crate::domain::session_capacity::DEFAULT_IDLE_TIMEOUT;
use std::time::{Duration, Instant};

impl<P, L, G, N> super::NativeRuntimeHandle<P, L, G, N>
where
    P: super::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + super::NativeApprovalRepository,
{
    pub(crate) fn automatic_recycling_enabled(&self) -> bool {
        self.core.capacity_scan.get().is_some() && self.core.running().is_ok()
    }
}

impl<P, L, G, N> NativeCoordinator<P, L, G, N>
where
    P: super::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + super::NativeApprovalRepository,
{
    pub(super) async fn admit_capacity(
        self: &Arc<Self>,
        reservation: &CapacityReservation,
    ) -> Result<CapacityRecord, DispatchError> {
        self.running()?;
        match self
            .repository
            .reserve_capacity(reservation, &self.deadline()?)
            .await
        {
            Err(DispatchError::SessionCapacityFull) => {}
            other => return other,
        }
        if self.capacity_scan.get().is_none() {
            return Err(DispatchError::SessionCapacityFull);
        }
        if !self
            .reclaim_candidates(AutomaticReleaseReason::CapacityPressure)
            .await?
        {
            return Err(DispatchError::SessionCapacityFull);
        }
        self.running()?;
        // Another admission can take the returned slot. This request never closes a
        // second tree or waits in an automatic admission queue.
        self.repository
            .reserve_capacity(reservation, &self.deadline()?)
            .await
    }

    pub(super) async fn reclaim_candidates(
        self: &Arc<Self>,
        reason: AutomaticReleaseReason,
    ) -> Result<bool, DispatchError> {
        let Ok(_selection) = self.capacity_recycling.try_lock() else {
            return Ok(false);
        };
        self.running()?;
        let sources = self
            .sources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let mut released = false;
        // Revisit the original ticket before selecting a new operation. A late
        // Closed or a recovered necessary write can complete that same release.
        for source in &sources {
            let resources: Vec<_> = source
                .capacity
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .values()
                .cloned()
                .collect();
            for resource in resources {
                if self.running().is_err() {
                    return Ok(released);
                }
                if let Some(original_reason) =
                    super::capacity_automatic::retained_reason(&resource)?
                {
                    released |= self
                        .automatic_release(source, &resource, original_reason)
                        .await
                        .unwrap_or(false);
                    if released && reason == AutomaticReleaseReason::CapacityPressure {
                        return Ok(true);
                    }
                }
            }
        }
        let now = Instant::now();
        let mut candidates = Vec::new();
        for source in sources {
            let resources: Vec<_> = source
                .capacity
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .values()
                .cloned()
                .collect();
            for resource in resources {
                let Ok(Some(since)) = self.automatic_candidate(&source, &resource, now) else {
                    continue;
                };
                if reason == AutomaticReleaseReason::IdleTimeout
                    && now
                        .checked_duration_since(since)
                        .is_none_or(|age| age < DEFAULT_IDLE_TIMEOUT)
                {
                    continue;
                }
                candidates.push((since, source.clone(), resource));
            }
        }
        candidates.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| {
                    a.1.pre
                        .metadata
                        .source_id
                        .encode()
                        .cmp(&b.1.pre.metadata.source_id.encode())
                })
                .then_with(|| a.2.pre.native.native_id().cmp(b.2.pre.native.native_id()))
                .then_with(|| {
                    a.2.pre
                        .metadata
                        .load_epoch
                        .encode()
                        .cmp(&b.2.pre.metadata.load_epoch.encode())
                })
        });
        if reason == AutomaticReleaseReason::CapacityPressure {
            let Some((since, source, capacity)) = candidates.into_iter().next() else {
                return Ok(false);
            };
            let selection = self.automatic_pressure_selection(&source, &capacity, since);
            return self
                .automatic_release_selected(&source, &capacity, reason, Some(selection))
                .await;
        }
        for (_, source, capacity) in candidates {
            if self.running().is_err() {
                break;
            }
            released |= self
                .automatic_release(&source, &capacity, reason)
                .await
                .unwrap_or(false);
        }
        Ok(released)
    }

    pub(super) fn start_capacity_scan(self: &Arc<Self>) -> Result<(), DispatchError> {
        self.capacity_scan_started
            .compare_exchange(
                false,
                true,
                super::Ordering::AcqRel,
                super::Ordering::Acquire,
            )
            .map_err(|_| DispatchError::Conflict)?;
        let core = self.clone();
        let failed = self.clone();
        let scan = self.tasks.spawn_guarded(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                let stopped = core.stop.notified();
                tokio::pin!(stopped);
                stopped.as_mut().enable();
                if core.running().is_err() { return; }
                tokio::select! {
                    biased;
                    () = &mut stopped => return,
                    _ = interval.tick() => {
                        // Each original operation retains its evidence on any error.
                        let _ = core.reclaim_candidates(AutomaticReleaseReason::IdleTimeout).await;
                    }
                }
            }
        }, move || failed.fail_all(DispatchError::ResourceUnconfirmed))?;
        self.capacity_scan
            .set(scan)
            .map_err(|_| DispatchError::Conflict)
    }
}
