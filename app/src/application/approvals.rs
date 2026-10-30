//! Approval persistence port. Runtime issuer and source fences belong to the connection owner.
use crate::domain::{
    ActorContext,
    approvals::{
        ApprovalDeliveryClaim, ApprovalError, ApprovalId, ApprovalObservation, ApprovalView,
        ConnectionEpoch, DecisionInput, DecisionProbe, DecisionRecord, RunLifecycle,
        TransportRecord, TransportReport,
    },
    history::OutputScope,
};
use std::future::Future;
pub trait ApprovalRepository: Send + Sync {
    fn probe(
        &self,
        actor: ActorContext,
        input: DecisionInput,
    ) -> impl Future<Output = Result<DecisionProbe, ApprovalError>> + Send;
    fn observe(
        &self,
        observation: &ApprovalObservation,
    ) -> impl Future<Output = Result<ApprovalView, ApprovalError>> + Send;
    fn decide(
        &self,
        actor: ActorContext,
        input: DecisionInput,
    ) -> impl Future<Output = Result<DecisionRecord, ApprovalError>> + Send;
    fn view(
        &self,
        actor: ActorContext,
        id: ApprovalId,
    ) -> impl Future<Output = Result<ApprovalView, ApprovalError>> + Send;
    fn claim(
        &self,
        actor: ActorContext,
        id: ApprovalId,
    ) -> impl Future<Output = Result<ApprovalDeliveryClaim, ApprovalError>> + Send;
    fn current(
        &self,
        claim: &ApprovalDeliveryClaim,
    ) -> impl Future<Output = Result<bool, ApprovalError>> + Send;
    fn report(
        &self,
        claim: &ApprovalDeliveryClaim,
        report: TransportReport,
    ) -> impl Future<Output = Result<TransportRecord, ApprovalError>> + Send;
    fn close_connection(
        &self,
        epoch: ConnectionEpoch,
    ) -> impl Future<Output = Result<(), ApprovalError>> + Send;
    fn invalidate_run(
        &self,
        scope: OutputScope,
        epoch: ConnectionEpoch,
        event: RunLifecycle,
    ) -> impl Future<Output = Result<(), ApprovalError>> + Send;
}

use super::history::{HistoryClock, HistoryCoordinator, HistoryRepository, OutputControl};
use crate::domain::history::FlushReason;
use std::sync::Arc;
/// A clean, owner-specific history version; never an authenticated issuer.
pub struct HistoryCheckpoint {
    pub(crate) scope: OutputScope,
    pub(crate) revision: u64,
    pub(crate) owner: Arc<()>,
}
pub trait ApprovalHistory: Send + Sync {
    fn prepare(
        &self,
        scope: OutputScope,
    ) -> impl Future<Output = Result<HistoryCheckpoint, ApprovalError>> + Send;
    fn current(&self, checkpoint: &HistoryCheckpoint) -> bool;
}
impl<R: HistoryRepository, C: HistoryClock, O: OutputControl> ApprovalHistory
    for HistoryCoordinator<R, C, O>
{
    async fn prepare(&self, scope: OutputScope) -> Result<HistoryCheckpoint, ApprovalError> {
        self.flush(scope, FlushReason::BeforeApproval)
            .await
            .map_err(|_| ApprovalError::HistoryUnavailable)?;
        self.approval_checkpoint(scope)
            .map_err(|_| ApprovalError::HistoryUnavailable)
    }
    fn current(&self, checkpoint: &HistoryCheckpoint) -> bool {
        self.approval_checkpoint_current(checkpoint)
    }
}
pub struct PreparedObservation {
    observation: ApprovalObservation,
    checkpoint: HistoryCheckpoint,
}
pub struct PreparedDecision {
    actor: ActorContext,
    input: DecisionInput,
    checkpoint: Option<HistoryCheckpoint>,
}
pub struct PreparedDelivery {
    actor: ActorContext,
    id: ApprovalId,
    checkpoint: HistoryCheckpoint,
}
pub struct ApprovalService<R, H> {
    repository: R,
    history: H,
}
impl<R: ApprovalRepository, H: ApprovalHistory> ApprovalService<R, H> {
    #[must_use]
    pub const fn new(repository: R, history: H) -> Self {
        Self {
            repository,
            history,
        }
    }
    /// Flushes without holding actor/source fences or a database transaction.
    /// # Errors
    /// Failed history prevents observation acceptance and display.
    pub async fn prepare_observation(
        &self,
        observation: ApprovalObservation,
    ) -> Result<PreparedObservation, ApprovalError> {
        let checkpoint = self.history.prepare(observation.binding.scope).await?;
        Ok(PreparedObservation {
            observation,
            checkpoint,
        })
    }
    /// Caller must hold the live issuer's owner fence; a mismatch requires release and reflush.
    /// # Errors
    /// Rejects changed history and invalid immutable sources.
    pub async fn commit_observation(
        &self,
        prepared: PreparedObservation,
    ) -> Result<ApprovalView, ApprovalError> {
        self.check(&prepared.checkpoint)?;
        self.repository.observe(&prepared.observation).await
    }
    /// Probes idempotency before requiring the original Run's history to remain live.
    /// # Errors
    /// Rejects conflicts and failed forced saves, without accepting a Command.
    pub async fn prepare_decision(
        &self,
        actor: ActorContext,
        input: DecisionInput,
    ) -> Result<PreparedDecision, ApprovalError> {
        let checkpoint = match self.repository.probe(actor, input).await? {
            DecisionProbe::Replay(_) => None,
            DecisionProbe::Fresh(view) => Some(self.history.prepare(view.binding.scope).await?),
        };
        Ok(PreparedDecision {
            actor,
            input,
            checkpoint,
        })
    }
    /// Caller holds `client_gate` then source owner; this short operation performs no flush or native transport I/O.
    /// # Errors
    /// Rejects changed history and failed atomic decision persistence.
    pub async fn commit_decision(
        &self,
        prepared: PreparedDecision,
    ) -> Result<DecisionRecord, ApprovalError> {
        if let Some(checkpoint) = &prepared.checkpoint {
            self.check(checkpoint)?;
        }
        self.repository.decide(prepared.actor, prepared.input).await
    }
    /// Flushes before entering the final source-owner delivery critical section.
    /// # Errors
    /// Rejects history failure and invalid actor/request state.
    pub async fn prepare_delivery(
        &self,
        actor: ActorContext,
        id: ApprovalId,
    ) -> Result<PreparedDelivery, ApprovalError> {
        let view = self.repository.view(actor, id).await?;
        let checkpoint = self.history.prepare(view.binding.scope).await?;
        Ok(PreparedDelivery {
            actor,
            id,
            checkpoint,
        })
    }
    /// Caller must retain actor/source fences through actual writer submission in WP3.
    /// # Errors
    /// Changed history or a stale source cannot claim delivery.
    pub async fn claim_delivery(
        &self,
        prepared: PreparedDelivery,
    ) -> Result<ApprovalDeliveryClaim, ApprovalError> {
        self.check(&prepared.checkpoint)?;
        self.repository.claim(prepared.actor, prepared.id).await
    }
    fn check(&self, checkpoint: &HistoryCheckpoint) -> Result<(), ApprovalError> {
        if self.history.current(checkpoint) {
            Ok(())
        } else {
            Err(ApprovalError::HistoryUnavailable)
        }
    }
}
