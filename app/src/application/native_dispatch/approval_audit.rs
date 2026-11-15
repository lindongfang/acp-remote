//! Only exact original necessary facts can construct an audit basis; metadata is readonly data.
use super::{
    ApprovalAdmissionProbe, ApprovalInvalidationProof, ApprovalReplyArmProof,
    ApprovalReplyTerminalProof, Arc, BoxFuture, ConfigurationPort, EntityId, NativeApprovalPort,
    NativeApprovalRepository, NativeControlRepository, NativeCoordinator, NativeDispatchRepository,
    NativeResourceRepository, OwnedNativeSource, RegisteredApprovalProbe, SourceLaunchPort,
};
use crate::{
    application::request_registry::{RegistryDeadline, RegistryOwnerScope},
    domain::{
        approval_audit::{
            ApprovalAuditBasisMetadata, ApprovalAuditEventKind, ApprovalAuditOutcome,
            ApprovalAuditReceipt, ApprovalAuditWarning,
        },
        approval_owner::{
            CommitState, LeaseCloseState, SqlConfirmation, SqlTaskJoinState,
            TerminalReplyObservation,
        },
        approval_records::{
            ApprovalAttemptRecord, ApprovalCommandRecord, ApprovalDecisionRecord,
            ApprovalInvalidationRecord, ApprovalTerminalRecord, StoredApprovalCommand,
            StoredApprovalDecision, StoredApprovalInvalidation, StoredApprovalTerminal,
        },
    },
};
trait OriginalAuditProof: Send + Sync {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    fn instance(&self) -> Option<EntityId>;
    fn verify(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<ApprovalAuditNecessary, ApprovalAuditWarning>;
}
#[derive(Clone)]
pub enum ApprovalAuditNecessary {
    Registered(ApprovalCommandRecord),
    Decided(ApprovalDecisionRecord),
    Delivery {
        arm: ApprovalAttemptRecord,
        terminal: ApprovalTerminalRecord,
    },
    Invalidated(ApprovalInvalidationRecord),
}
struct RegisteredProof<P: NativeApprovalPort> {
    original: Arc<RegisteredApprovalProbe<P>>,
    stored: StoredApprovalCommand,
}
struct DecisionProof<P: NativeApprovalPort> {
    original: Arc<ApprovalAdmissionProbe<P>>,
    stored: StoredApprovalDecision,
}
struct DeliveryProof<P: NativeApprovalPort> {
    arm: Arc<ApprovalReplyArmProof<P>>,
    terminal: Arc<ApprovalReplyTerminalProof<P>>,
    stored: StoredApprovalTerminal,
}
struct InvalidationProof<P: NativeApprovalPort> {
    original: Arc<ApprovalInvalidationProof<P>>,
    stored: StoredApprovalInvalidation,
}
fn confirmed(value: &SqlConfirmation) -> bool {
    value.commit == CommitState::Confirmed
        && value.lease_close == LeaseCloseState::Confirmed
        && value.task_join == SqlTaskJoinState::Joined
}
macro_rules! verify {
    ($self:ident,$owner:ident,$field:ident,$variant:ident) => {{
        $self
            .$field
            .verify_owner($owner)
            .map_err(|_| ApprovalAuditWarning::InvalidBasis)?;
        if $self.stored.record() != $self.$field.metadata()
            || $self.stored.record_fingerprint() != $self.$field.fingerprint()
            || !confirmed($self.stored.confirmation())
            || !confirmed(
                &$self
                    .$field
                    .write_cache()
                    .confirmation()
                    .map_err(|_| ApprovalAuditWarning::InvalidBasis)?,
            )
        {
            return Err(ApprovalAuditWarning::InvalidBasis);
        }
        Ok(ApprovalAuditNecessary::$variant(
            $self.stored.record().clone(),
        ))
    }};
}
impl<P: NativeApprovalPort> OriginalAuditProof for RegisteredProof<P> {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    fn instance(&self) -> Option<EntityId> {
        Some(self.original.instance_for_approval_test())
    }
    fn verify(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<ApprovalAuditNecessary, ApprovalAuditWarning> {
        verify!(self, owner, original, Registered)
    }
}
impl<P: NativeApprovalPort> OriginalAuditProof for DecisionProof<P> {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    fn instance(&self) -> Option<EntityId> {
        Some(self.original.instance_for_approval_test())
    }
    fn verify(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<ApprovalAuditNecessary, ApprovalAuditWarning> {
        verify!(self, owner, original, Decided)
    }
}
impl<P: NativeApprovalPort> OriginalAuditProof for InvalidationProof<P> {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    fn instance(&self) -> Option<EntityId> {
        Some(self.original.instance_for_approval_test())
    }
    fn verify(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<ApprovalAuditNecessary, ApprovalAuditWarning> {
        verify!(self, owner, original, Invalidated)
    }
}
impl<P: NativeApprovalPort> OriginalAuditProof for DeliveryProof<P> {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    fn instance(&self) -> Option<EntityId> {
        Some(self.terminal.instance_for_approval_test())
    }
    fn verify(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<ApprovalAuditNecessary, ApprovalAuditWarning> {
        self.arm
            .verify_owner(owner)
            .map_err(|_| ApprovalAuditWarning::InvalidBasis)?;
        self.terminal
            .verify_owner(owner)
            .map_err(|_| ApprovalAuditWarning::InvalidBasis)?;
        if !Arc::ptr_eq(&self.arm.original, &self.terminal.original)
            || self.arm.metadata().response != self.terminal.metadata().response
            || self.stored.record() != self.terminal.metadata()
            || !confirmed(self.stored.confirmation())
            || !confirmed(
                &self
                    .arm
                    .write_cache()
                    .confirmation()
                    .map_err(|_| ApprovalAuditWarning::InvalidBasis)?,
            )
            || !confirmed(
                &self
                    .terminal
                    .write_cache()
                    .confirmation()
                    .map_err(|_| ApprovalAuditWarning::InvalidBasis)?,
            )
        {
            return Err(ApprovalAuditWarning::InvalidBasis);
        }
        Ok(ApprovalAuditNecessary::Delivery {
            arm: self.arm.metadata().clone(),
            terminal: self.terminal.metadata().clone(),
        })
    }
}
pub struct ApprovalAuditBasis {
    metadata: ApprovalAuditBasisMetadata,
    owner: RegistryOwnerScope,
    original: Arc<dyn OriginalAuditProof>,
}
impl ApprovalAuditBasis {
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    pub(crate) fn original_instance_for_test(&self) -> Option<EntityId> {
        self.original.instance()
    }
    #[must_use]
    pub const fn metadata(&self) -> &ApprovalAuditBasisMetadata {
        &self.metadata
    }
    pub(crate) fn verify_original(
        &self,
        owner: &RegistryOwnerScope,
    ) -> Result<ApprovalAuditNecessary, ApprovalAuditWarning> {
        if !self.owner.matches(owner) {
            return Err(ApprovalAuditWarning::InvalidBasis);
        }
        self.original.verify(owner)
    }
    pub(super) fn registered<P: NativeApprovalPort>(
        probe: Arc<RegisteredApprovalProbe<P>>,
        stored: StoredApprovalCommand,
    ) -> Result<Self, ApprovalAuditWarning> {
        let metadata = ApprovalAuditBasisMetadata {
            approval: probe.metadata().approval,
            event: ApprovalAuditEventKind::Registered,
            basis_fingerprint: *probe.fingerprint(),
            actor: Some(probe.metadata().actor),
            target: Some(probe.metadata().target),
            outcome: ApprovalAuditOutcome::Registered,
            occurred_at_ms: probe.metadata().registered_at_ms,
        };
        let owner = probe.original.owner.clone();
        let original = Arc::new(RegisteredProof {
            original: probe,
            stored,
        });
        original.verify(&owner)?;
        Ok(Self {
            metadata,
            owner,
            original,
        })
    }
    pub(super) fn decided<P: NativeApprovalPort>(
        probe: Arc<ApprovalAdmissionProbe<P>>,
        stored: StoredApprovalDecision,
        target: EntityId,
    ) -> Result<Self, ApprovalAuditWarning> {
        let metadata = ApprovalAuditBasisMetadata {
            approval: probe.metadata().approval,
            event: ApprovalAuditEventKind::Decided,
            basis_fingerprint: *probe.fingerprint(),
            actor: Some(probe.metadata().actor),
            target: Some(target),
            outcome: ApprovalAuditOutcome::Accepted,
            occurred_at_ms: probe.metadata().accepted_at_ms,
        };
        let owner = probe.original.owner.clone();
        let original = Arc::new(DecisionProof {
            original: probe,
            stored,
        });
        original.verify(&owner)?;
        Ok(Self {
            metadata,
            owner,
            original,
        })
    }
    pub(super) fn delivery<P: NativeApprovalPort>(
        arm: Arc<ApprovalReplyArmProof<P>>,
        terminal: Arc<ApprovalReplyTerminalProof<P>>,
        stored: StoredApprovalTerminal,
        target: Option<EntityId>,
    ) -> Result<Self, ApprovalAuditWarning> {
        let outcome = match terminal.metadata().observation {
            TerminalReplyObservation::Zero { .. } => ApprovalAuditOutcome::Zero,
            TerminalReplyObservation::Written => ApprovalAuditOutcome::Written,
            TerminalReplyObservation::Unknown { .. } => ApprovalAuditOutcome::Unknown,
        };
        let metadata = ApprovalAuditBasisMetadata {
            approval: terminal.metadata().approval,
            event: ApprovalAuditEventKind::Delivery,
            basis_fingerprint: *terminal.fingerprint(),
            actor: None,
            target,
            outcome,
            occurred_at_ms: terminal.metadata().observed_at_ms,
        };
        let owner = terminal.original.owner.clone();
        let original = Arc::new(DeliveryProof {
            arm,
            terminal,
            stored,
        });
        original.verify(&owner)?;
        Ok(Self {
            metadata,
            owner,
            original,
        })
    }
    pub(super) fn invalidated<P: NativeApprovalPort>(
        probe: Arc<ApprovalInvalidationProof<P>>,
        stored: StoredApprovalInvalidation,
        target: Option<EntityId>,
    ) -> Result<Self, ApprovalAuditWarning> {
        let metadata = ApprovalAuditBasisMetadata {
            approval: probe.metadata().approval,
            event: ApprovalAuditEventKind::Invalidated,
            basis_fingerprint: *probe.fingerprint(),
            actor: None,
            target,
            outcome: ApprovalAuditOutcome::Invalidated,
            occurred_at_ms: probe.metadata().observed_at_ms,
        };
        let owner = probe.original.owner.clone();
        let original = Arc::new(InvalidationProof {
            original: probe,
            stored,
        });
        original.verify(&owner)?;
        Ok(Self {
            metadata,
            owner,
            original,
        })
    }
}
pub trait ApprovalAuditPort: Send + Sync {
    fn append_approval<'a>(
        &'a self,
        basis: &'a ApprovalAuditBasis,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<ApprovalAuditReceipt, ApprovalAuditWarning>>;
}

pub(super) fn record_warning<P: NativeApprovalPort>(
    slot: &super::approval_inventory::ApprovalSlot<P>,
    warning: ApprovalAuditWarning,
) {
    match slot.state.lock() {
        Ok(mut state) => state.audit_warnings.push(warning),
        Err(poisoned) => poisoned.into_inner().audit_warnings.push(warning),
    }
    slot.publish_change();
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
        + NativeApprovalRepository,
{
    // Every caller is inside its existing Source.jobs/children producer or original exclusive closer.
    // No child right is acquired after seal; that original actor owns this bounded SQL until it returns.
    pub(super) async fn append_owned_approval_audit(
        &self,
        source: &Arc<super::runtime::SourceSlot<P, L>>,
        slot: &Arc<super::approval_inventory::ApprovalSlot<P>>,
        basis: Result<ApprovalAuditBasis, ApprovalAuditWarning>,
    ) {
        if !Arc::ptr_eq(&source.pre, &slot.request.original.source) {
            record_warning(slot, ApprovalAuditWarning::InvalidBasis);
            return;
        }
        self.append_closing_approval_audit(slot, basis).await;
    }
    pub(super) async fn append_closing_approval_audit(
        &self,
        slot: &super::approval_inventory::ApprovalSlot<P>,
        basis: Result<ApprovalAuditBasis, ApprovalAuditWarning>,
    ) {
        let basis = match basis {
            Ok(basis) => basis,
            Err(warning) => {
                record_warning(slot, warning);
                return;
            }
        };
        let Ok(deadline) = self.deadline() else {
            record_warning(slot, ApprovalAuditWarning::Sql);
            return;
        };
        if let Err(warning) = self.repository.append_approval(&basis, &deadline).await {
            record_warning(slot, warning);
        }
    }
}
