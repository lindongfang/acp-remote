//! A weak, readonly status subscription. Each next requires the current real actor again.
#![allow(
    clippy::needless_pass_by_value,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, ConfigurationPort, Instant, NativeApprovalPort, NativeApprovalRepository,
    NativeControlRepository, NativeCoordinator, NativeDispatchRepository, NativeResourceRepository,
    NativeRuntimeHandle, OwnedNativeSource, SourceLaunchPort, approval_inventory::ApprovalSlot,
    environment::SourcePermissionSlot, runtime::SourceSlot,
};
use crate::{
    application::approval_owner::ApprovalAuthority,
    domain::approval_owner::{ApprovalId, ApprovalRejection, ApprovalStatus},
};
use std::sync::Weak;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalWatchError {
    Rejected(ApprovalRejection),
    TimedOut,
    OriginalUnavailable,
}
pub struct ApprovalStatusWatch<A, P, L, G, N>
where
    A: ApprovalAuthority,
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    core: Weak<NativeCoordinator<P, L, G, N>>,
    authority: Arc<A>,
    source: Weak<SourceSlot<P, L>>,
    permission: Weak<SourcePermissionSlot<P>>,
    slot: Weak<ApprovalSlot<P>>,
    id: ApprovalId,
}
impl<A, P, L, G, N> ApprovalStatusWatch<A, P, L, G, N>
where
    A: ApprovalAuthority,
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    pub(super) fn from_original(
        native: &NativeRuntimeHandle<P, L, G, N>,
        authority: Arc<A>,
        source: Arc<SourceSlot<P, L>>,
        permission: Arc<SourcePermissionSlot<P>>,
        slot: Arc<ApprovalSlot<P>>,
        id: ApprovalId,
    ) -> Self {
        Self {
            core: Arc::downgrade(&native.core),
            authority,
            source: Arc::downgrade(&source),
            permission: Arc::downgrade(&permission),
            slot: Arc::downgrade(&slot),
            id,
        }
    }
    /// # Errors
    /// Rejects revoked/foreign actors, an unavailable original, exhausted sequence or elapsed wait.
    pub async fn next(
        &mut self,
        actor: &A::Actor,
        after_sequence: u64,
        wait_until: Instant,
    ) -> Result<ApprovalStatus, ApprovalWatchError> {
        loop {
            let core = self
                .core
                .upgrade()
                .ok_or(ApprovalWatchError::OriginalUnavailable)?;
            let source = self
                .source
                .upgrade()
                .ok_or(ApprovalWatchError::OriginalUnavailable)?;
            let permission = self
                .permission
                .upgrade()
                .ok_or(ApprovalWatchError::OriginalUnavailable)?;
            let slot = self
                .slot
                .upgrade()
                .ok_or(ApprovalWatchError::OriginalUnavailable)?;
            let changed = slot.changed.notified();
            let source_changed = source.changed.notified();
            tokio::pin!(changed);
            tokio::pin!(source_changed);
            changed.as_mut().enable();
            source_changed.as_mut().enable();
            let generation = source.progress.current();
            let reader = tokio::time::timeout_at(wait_until, self.authority.authorize_read(actor))
                .await
                .map_err(|_| ApprovalWatchError::TimedOut)?
                .map_err(ApprovalWatchError::Rejected)?;
            if !reader
                .owner
                .matches(&core.repository.approval_owner_scope())
                || slot.request.metadata().approval != self.id
            {
                return Err(ApprovalWatchError::Rejected(
                    ApprovalRejection::InvalidActor,
                ));
            }
            let service = super::approval_service::NativeApprovalService::create(
                NativeRuntimeHandle { core: core.clone() },
                self.authority.clone(),
            )
            .map_err(ApprovalWatchError::Rejected)?;
            let status = service
                .status(&source, &permission, &slot)
                .map_err(ApprovalWatchError::Rejected)?;
            if status.change_sequence > after_sequence {
                return Ok(status);
            }
            if status.change_sequence == u64::MAX {
                return Err(ApprovalWatchError::Rejected(
                    ApprovalRejection::LengthUnrepresentable,
                ));
            }
            tokio::time::timeout_at(wait_until,async{tokio::select!{()=changed=>{},()=source_changed=>{},()=source.progress.wait_after(generation)=>{}}}).await.map_err(|_|ApprovalWatchError::TimedOut)?;
        }
    }
}
