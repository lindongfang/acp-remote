//! Safety handoff consumes only the same original Selected actor's real terminal state.
#![allow(
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, ConfigurationPort, DispatchError, NativeApprovalPort, NativeApprovalRepository,
    NativeControlRepository, NativeCoordinator, NativeDispatchRepository, NativeResourceRepository,
    NativeRuntimeOptions, OwnedNativeSource, PermissionPendingSnapshot, PromptTicket,
    SourceLaunchPort, TransportFact, TransportProgress,
    environment::{PermissionResponseOwner, SourcePermissionSlot},
    runtime::SourceSlot,
};
impl<P, L, G, N> NativeCoordinator<P, L, G, N>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository,
{
    pub(super) async fn settle_selected_for_safety(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        permission: &Arc<SourcePermissionSlot<P>>,
    ) -> Result<(), DispatchError> {
        let selected = {
            let owner = permission
                .response_owner
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            match owner.as_ref() {
                Some(PermissionResponseOwner::Selected(selected)) => Some(selected.clone()),
                _ => None,
            }
        };
        let Some(selected) = selected else {
            return Ok(());
        };
        // A local cancel is requested outside the protocol's lock hierarchy. It grants no native result.
        let ticket = selected
            .state
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .ticket
            .clone();
        if let Some(ticket) = ticket {
            ticket.request_cancel();
        }
        let admission = permission
            .approval
            .get()
            .ok_or(DispatchError::ResourceUnconfirmed)?
            .state
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .admission
            .clone()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        let job = admission
            .job
            .get()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        if !job
            .join(NativeRuntimeOptions::at(self.options.prompt_write_wait)?)
            .await
            || job.failed()
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let (progress, stored) = {
            let state = selected
                .state
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            (
                state
                    .probe
                    .as_ref()
                    .map(|probe| self.protocol.probe_progress(probe)),
                state.terminal_stored.clone(),
            )
        };
        match progress {
            Some(TransportProgress::Finished(TransportFact::Written)) => {
                if stored.is_none() {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                return Ok(());
            }
            Some(TransportProgress::Finished(TransportFact::Zero { .. })) => {
                if stored.is_none() {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
            }
            // No reservation was acquired by this completed actor. This is a stage, never a Zero proof.
            None => {
                let state = selected
                    .state
                    .lock()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?;
                if state.reservation.is_some() || state.frame.is_some() || state.ticket.is_some() {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
            }
            _ => return Err(DispatchError::ResourceUnconfirmed),
        }
        let connection = source.source.get().ok_or(DispatchError::InvalidScope)?;
        if self
            .protocol
            .inspect_permission(connection, &permission.request)?
            != PermissionPendingSnapshot::Waiting
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let _action = permission
            .action
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        let mut state = permission
            .state
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        let mut owner = permission
            .response_owner
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        if !matches!(owner.as_ref(),Some(PermissionResponseOwner::Selected(current)) if Arc::ptr_eq(current,&selected))
            || state.claimed != Some(selected.id.entity())
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        state.claimed = None;
        owner.take();
        Ok(())
    }
}
