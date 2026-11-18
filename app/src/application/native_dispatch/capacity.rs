//! Durable admission and per-native ownership for Create, execution and replay.
use super::{
    Arc, AtomicBool, BoxFuture, CapacityRecord, CapacityReservation, ConfigurationPort,
    DispatchError, NativeControlRepository, NativeCoordinator, NativeDispatchRepository,
    NativeResourceRepository, NativeSessionKey, Ordering, OwnedNativeSource, PreResourceProbe,
    PreSourceProbe, ResourceAuthority, ResourceMetadata, SourceLaunchPort,
    runtime::{LifecycleOperation, SourceSlot},
    tasks::TaskSlot,
};
use crate::domain::shared_view::ViewError;
use std::sync::{Mutex, Weak};

pub(super) fn restore_display(
    source: &PreSourceProbe,
    resource: &PreResourceProbe,
    restore: impl FnOnce() -> Result<(), ViewError>,
) -> Result<(), ViewError> {
    let _gate = source
        .authority
        .gate
        .lock()
        .map_err(|_| ViewError::ReadFailure)?;
    if !Arc::ptr_eq(&source.authority, &resource.authority.source)
        || resource.metadata.source != source.metadata
        || source.authority.closed.load(Ordering::Acquire)
        || source.authority.closing.load(Ordering::Acquire)
        || resource.authority.closed.load(Ordering::Acquire)
        || resource.authority.releasing.load(Ordering::Acquire)
    {
        return Err(ViewError::SourceReleased);
    }
    restore()
}

pub(super) struct CapacityResource<P: super::NativeApprovalPort> {
    pub(super) pre: Arc<PreResourceProbe>,
    pub(super) record: Mutex<Option<CapacityRecord>>,
    pub(super) close: tokio::sync::Mutex<()>,
    pub(super) operation: Mutex<Option<Arc<LifecycleOperation<P>>>>,
    pub(super) automatic: Mutex<Option<Arc<super::capacity_automatic::AutomaticOperation<P>>>>,
    pub(super) queue_revision: Mutex<Option<(Vec<NativeSessionKey>, u64)>>,
    pub(super) reads: Mutex<Vec<Weak<TaskSlot>>>,
}

pub(super) trait ReadCapacityAdmission: Send + Sync {
    fn reserve(&self) -> BoxFuture<'_, Result<(), ViewError>>;
}
struct OriginalAdmission<P, L, G, N>
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
    core: Weak<NativeCoordinator<P, L, G, N>>,
    source: Weak<SourceSlot<P, L>>,
    resource: Arc<CapacityResource<P>>,
}
impl<P, L, G, N> ReadCapacityAdmission for OriginalAdmission<P, L, G, N>
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
    fn reserve(&self) -> BoxFuture<'_, Result<(), ViewError>> {
        Box::pin(async move {
            let core = self.core.upgrade().ok_or(ViewError::NodeStopping)?;
            let source = self.source.upgrade().ok_or(ViewError::SourceReleased)?;
            if self.resource.pre.authority.closed.load(Ordering::Acquire)
                || self
                    .resource
                    .pre
                    .authority
                    .releasing
                    .load(Ordering::Acquire)
                || source.pre.authority.closing.load(Ordering::Acquire)
                || source.pre.authority.closed.load(Ordering::Acquire)
            {
                return Err(ViewError::SourceReleased);
            }
            track_reservation(
                &source.capacity_reservations,
                self.resource.pre.metadata.load_epoch,
            )
            .map_err(ViewError::NativeFailure)?;
            let record = core
                .admit_capacity(&CapacityReservation {
                    id: self.resource.pre.metadata.load_epoch,
                    source: source.pre.clone(),
                    native: Some(self.resource.pre.native.clone()),
                    create_operation: None,
                })
                .await
                .map_err(ViewError::NativeFailure)?;
            install_capacity_record(&source.pre, &self.resource, record).map_err(|error| {
                if matches!(error, DispatchError::Stopped) {
                    ViewError::SourceReleased
                } else {
                    ViewError::NativeFailure(error)
                }
            })?;
            core.restore_tree_members(&source, &self.resource.pre.native)
                .await
                .map_err(ViewError::NativeFailure)?;
            Ok(())
        })
    }
}

fn track_reservation(
    reservations: &Mutex<Vec<super::EntityId>>,
    id: super::EntityId,
) -> Result<(), DispatchError> {
    let mut reservations = reservations.lock().map_err(|_| DispatchError::Stopped)?;
    if !reservations.contains(&id) {
        reservations
            .try_reserve(1)
            .map_err(|_| DispatchError::StorageUnavailable)?;
        reservations.push(id);
    }
    drop(reservations);
    Ok(())
}

fn install_capacity_record<P: super::NativeApprovalPort>(
    source: &PreSourceProbe,
    resource: &CapacityResource<P>,
    record: CapacityRecord,
) -> Result<(), DispatchError> {
    let _gate = source
        .authority
        .gate
        .lock()
        .map_err(|_| DispatchError::Stopped)?;
    if source.authority.closed.load(Ordering::Acquire)
        || source.authority.closing.load(Ordering::Acquire)
        || resource.pre.authority.closed.load(Ordering::Acquire)
        || resource.pre.authority.releasing.load(Ordering::Acquire)
    {
        return Err(DispatchError::Stopped);
    }
    if !Arc::ptr_eq(&source.authority, &resource.pre.authority.source)
        || resource.pre.metadata.source != source.metadata
        || record.source != source.metadata.source_id
        || record.instance != source.metadata.instance_id
        || record.runtime != source.metadata.runtime
        || record.native.as_ref() != Some(&resource.pre.native)
    {
        return Err(DispatchError::ResourceUnconfirmed);
    }
    let mut current = resource.record.lock().map_err(|_| DispatchError::Stopped)?;
    if current.as_ref().map_or_else(
        || record.id != resource.pre.metadata.load_epoch,
        |current| !current.same_reservation(&record),
    ) {
        return Err(DispatchError::ResourceUnconfirmed);
    }
    *current = Some(record);
    drop(current);
    Ok(())
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
    pub(super) async fn capacity_source(
        self: &Arc<Self>,
        native: &NativeSessionKey,
        plan: Arc<super::SourceInitializationPlan<L::Plan, P::Setup>>,
    ) -> Result<Arc<SourceSlot<P, L>>, DispatchError> {
        let mut routes = self.capacity_routes.lock().await;
        if let Some(owner) = self
            .repository
            .capacity_owner(native, &self.deadline()?)
            .await?
        {
            if owner.physically_closed {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            let sources = self.sources.lock().map_err(|_| DispatchError::Stopped)?;
            let source = sources
                .iter()
                .find(|source| {
                    source.pre.metadata.instance_id == owner.instance
                        && source.pre.metadata.runtime == owner.runtime
                        && !source.pre.authority.closed.load(Ordering::Acquire)
                        && !source.pre.authority.closing.load(Ordering::Acquire)
                })
                .cloned()
                .ok_or(DispatchError::ResourceUnconfirmed)?;
            drop(sources);
            let stopped = source
                .capacity
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .get(native)
                .is_some_and(|resource| {
                    resource.pre.authority.closed.load(Ordering::Acquire)
                        || resource.pre.authority.releasing.load(Ordering::Acquire)
                });
            if stopped {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            return Ok(source);
        }
        if let Some(source) = routes.get(native).and_then(Weak::upgrade)
            && !source.pre.authority.closed.load(Ordering::Acquire)
            && !source.pre.authority.closing.load(Ordering::Acquire)
        {
            return Ok(source);
        }
        let source = self.ensure_source_slot(plan)?;
        routes.insert(native.clone(), Arc::downgrade(&source));
        drop(routes);
        Ok(source)
    }

    pub(super) fn capacity_resource(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        native: &NativeSessionKey,
    ) -> Result<Arc<CapacityResource<P>>, DispatchError> {
        let mut resources = source.capacity.lock().map_err(|_| DispatchError::Stopped)?;
        if let Some(resource) = resources.get(native)
            && !resource.pre.authority.closed.load(Ordering::Acquire)
        {
            return Ok(resource.clone());
        }
        if resources.contains_key(native) {
            source
                .capacity_activity
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .forget_closed(native.native_id());
        }
        let resource = Arc::new(CapacityResource {
            pre: Arc::new(PreResourceProbe {
                metadata: ResourceMetadata {
                    source: source.pre.metadata.clone(),
                    target: Self::new_id()?,
                    owner: Self::new_id()?,
                    load_epoch: Self::new_id()?,
                    reserved_at_ms: self.clock.unix_millis(),
                },
                native: native.clone(),
                authority: Arc::new(ResourceAuthority {
                    source: source.pre.authority.clone(),
                    closed: AtomicBool::new(false),
                    releasing: AtomicBool::new(false),
                }),
            }),
            record: Mutex::new(None),
            close: tokio::sync::Mutex::new(()),
            operation: Mutex::new(None),
            automatic: Mutex::new(None),
            queue_revision: Mutex::new(None),
            reads: Mutex::new(Vec::new()),
        });
        resources.insert(native.clone(), resource.clone());
        drop(resources);
        Ok(resource)
    }

    pub(super) fn read_capacity(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<CapacityResource<P>>,
    ) -> Arc<dyn ReadCapacityAdmission> {
        Arc::new(OriginalAdmission {
            core: Arc::downgrade(self),
            source: Arc::downgrade(source),
            resource: resource.clone(),
        })
    }

    pub(super) async fn reserve_native_capacity(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<CapacityResource<P>>,
    ) -> Result<(), DispatchError> {
        track_reservation(
            &source.capacity_reservations,
            resource.pre.metadata.load_epoch,
        )?;
        let record = self
            .admit_capacity(&CapacityReservation {
                id: resource.pre.metadata.load_epoch,
                source: source.pre.clone(),
                native: Some(resource.pre.native.clone()),
                create_operation: None,
            })
            .await?;
        install_capacity_record(&source.pre, resource, record)?;
        self.restore_tree_members(source, &resource.pre.native)
            .await?;
        restore_display(&source.pre, &resource.pre, || {
            source.view_attachment.restore_target(resource.pre.native())
        })
        .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        Ok(())
    }

    pub(super) async fn capacity_closed(
        &self,
        resource: &Arc<CapacityResource<P>>,
        expected: &CapacityRecord,
    ) -> Result<(), DispatchError> {
        if !resource.pre.authority.closed.load(Ordering::Acquire) {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let record = resource
            .record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let record = record.ok_or(DispatchError::ResourceUnconfirmed)?;
        if !record.same_reservation(expected) {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        self.repository
            .confirm_capacity_closed(expected, &self.deadline()?)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        EntityId,
        native::RuntimeEpoch,
        native_dispatch::{NativeOwnerKind, SourceMetadata},
    };
    #[tokio::test]
    async fn a_close_between_reservation_and_display_restore_keeps_the_deny() {
        let id = |n| EntityId::from_bytes([n; 16]);
        let source = super::super::capacity_source_development_fixture(SourceMetadata {
            source_id: id(1),
            instance_id: id(2),
            runtime: RuntimeEpoch(id(3)),
            connection_epoch: id(4),
            config_identity: id(5),
            config_revision: 1,
            owner_kind: NativeOwnerKind::TestOwnedStream,
            created_at_ms: 0,
        });
        let resource = Arc::new(PreResourceProbe {
            metadata: ResourceMetadata {
                source: source.metadata.clone(),
                target: id(6),
                owner: id(7),
                load_epoch: id(8),
                reserved_at_ms: 0,
            },
            native: NativeSessionKey::from_config(Some(id(1)), "synthetic-root").unwrap(),
            authority: Arc::new(ResourceAuthority {
                source: source.authority.clone(),
                closed: AtomicBool::new(false),
                releasing: AtomicBool::new(false),
            }),
        });
        let restored = Arc::new(AtomicBool::new(false));
        let (reserved, observed) = tokio::sync::oneshot::channel();
        let (resume, waiting) = tokio::sync::oneshot::channel();
        let original_source = source.clone();
        let original_resource = resource.clone();
        let calls = restored.clone();
        let late_restore = tokio::spawn(async move {
            reserved.send(()).unwrap();
            waiting.await.unwrap();
            restore_display(&original_source, &original_resource, || {
                calls.store(true, Ordering::Release);
                Ok(())
            })
        });
        observed.await.unwrap();
        {
            let _gate = source.authority.gate.lock().unwrap();
            resource.authority.releasing.store(true, Ordering::Release);
        }
        resume.send(()).unwrap();
        assert_eq!(late_restore.await.unwrap(), Err(ViewError::SourceReleased));
        assert!(!restored.load(Ordering::Acquire));
    }
}
