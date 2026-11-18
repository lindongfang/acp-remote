//! A fixed sorted member set retains each native queue owner through the original close.
use super::{
    Arc, AutomaticCloseGuard, CapacityResource, ConfigurationPort, DispatchError,
    NativeApprovalPort, NativeControlRepository, NativeCoordinator, NativeDispatchRepository,
    NativeResourceRepository, Ordering, OwnedNativeSource, ResourceSlot, SourceLaunchPort,
    SourceSlot,
};
use crate::domain::native::NativeSessionKey;

pub(super) struct AutomaticTree<P: NativeApprovalPort> {
    pub(super) members: Vec<NativeSessionKey>,
    pub(super) resources: Vec<Arc<ResourceSlot<P>>>,
    pub(super) queue: Arc<dyn AutomaticCloseGuard>,
}

impl<P: NativeApprovalPort> AutomaticTree<P> {
    pub(super) fn contains(&self, native: &str) -> bool {
        self.members
            .iter()
            .any(|member| member.native_id() == native)
    }

    pub(super) fn matches(
        &self,
        activity: &super::super::capacity_activity::CapacityActivity,
        root: &str,
    ) -> Result<bool, DispatchError> {
        let current = activity.members(root)?;
        Ok(current.len() == self.members.len()
            && current
                .iter()
                .zip(&self.members)
                .all(|(current, original)| current.as_ref() == original.native_id()))
    }

    pub(super) fn seal<L>(&self, source: &SourceSlot<P, L>)
    where
        L: SourceLaunchPort,
        L::Owned: OwnedNativeSource<Streams = P::Streams>,
    {
        for member in &self.members {
            source.view_attachment.seal_target(member);
        }
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
    pub(super) fn automatic_tree(
        &self,
        source: &SourceSlot<P, L>,
        capacity: &CapacityResource<P>,
    ) -> Result<Arc<AutomaticTree<P>>, DispatchError> {
        let mut members = source
            .capacity_activity
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .members(capacity.pre.native.native_id())?
            .into_iter()
            .map(|native| {
                NativeSessionKey::from_config(Some(source.pre.metadata.source_id), &native)
                    .map_err(|_| DispatchError::InvalidTarget)
            })
            .collect::<Result<Vec<_>, _>>()?;
        members.sort_by(|left, right| left.native_id().cmp(right.native_id()));
        members.dedup();
        if members.is_empty() {
            return Err(DispatchError::SessionTreeUnverified);
        }
        let resources = {
            let targets = source.targets.lock().map_err(|_| DispatchError::Stopped)?;
            members
                .iter()
                .filter_map(|member| targets.get(member).cloned())
                .filter(|resource| {
                    !resource.pre.authority.closed.load(Ordering::Acquire)
                        || Arc::ptr_eq(&resource.pre.authority, &capacity.pre.authority)
                })
                .collect()
        };
        let queue = self.queue.automatic_close_tree_guard(&members)?;
        Ok(Arc::new(AutomaticTree {
            members,
            resources,
            queue,
        }))
    }
}
