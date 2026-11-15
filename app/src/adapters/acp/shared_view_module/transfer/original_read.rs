//! A short control-flow lane coordinates actual writers with one original-read handoff.
use super::{
    Bootstrap, CollectedModuleRecord, Continue, ModuleViewTransfer, Sender, Shared, ViewError,
    oneshot,
};
use crate::{
    application::shared_views::{
        CurrentViewAuthority, local_transfer::LocalTransferPhase, reader::OriginalReadProgress,
        wire::RecordKind,
    },
    domain::EntityId,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FollowRejection {
    PreflightBusy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleOriginalReadProgress {
    Busy,
    Pending,
    CandidateReady,
    Complete,
    Failed(ViewError),
}
#[derive(Clone, Copy)]
pub(super) enum ContinuationLane {
    AwaitData,
    AwaitCredit,
    Writing,
    Switching(EntityId),
    Stopped,
}
pub(super) enum ContinueKind {
    Completed,
    InFlight {
        nonce: EntityId,
        adopted: Arc<AtomicBool>,
    },
}
struct LaneClaim {
    shared: Arc<Shared>,
    nonce: EntityId,
    adopted: Arc<AtomicBool>,
}
impl Drop for LaneClaim {
    fn drop(&mut self) {
        let mut lane = match self.shared.lane.lock() {
            Ok(lane) => lane,
            Err(poison) => poison.into_inner(),
        };
        if !self.adopted.load(Ordering::Acquire)
            && matches!(*lane,ContinuationLane::Switching(nonce) if nonce==self.nonce)
        {
            *lane = ContinuationLane::AwaitData;
        }
        drop(lane);
        // Reuse the existing signal solely to wake a sender whose command was cancelled.
        // Its value is unchanged; this is no flush, completion or authorization event.
        self.shared.signal.send_modify(|_| {});
    }
}
impl ModuleViewTransfer {
    /// # Errors
    /// Performs current authentication without waiting for the display actor gate.
    pub async fn original_progress(&self) -> Result<ModuleOriginalReadProgress, ViewError> {
        self.authority.client_binding(&self.client)?;
        let observer = self
            .shared
            .original
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .clone();
        Ok(
            match self
                .reads
                .try_observe_original(&self.client, &observer)
                .await?
            {
                None => ModuleOriginalReadProgress::Busy,
                Some(OriginalReadProgress::Pending) => ModuleOriginalReadProgress::Pending,
                Some(OriginalReadProgress::CandidateReady { .. }) => {
                    ModuleOriginalReadProgress::CandidateReady
                }
                Some(OriginalReadProgress::Complete { .. }) => ModuleOriginalReadProgress::Complete,
                Some(OriginalReadProgress::Failed(error)) => {
                    ModuleOriginalReadProgress::Failed(error)
                }
            },
        )
    }
    /// # Errors
    /// Returns only the new stream's actually received packet-one Status. Credit one is explicit.
    #[expect(
        clippy::too_many_lines,
        reason = "Keep current-auth preflight, linear lane claim, adopted reply and real Credit0/Status receipt together; test-only classification adds no control transition"
    )]
    pub async fn follow_original_inflight(&mut self) -> Result<CollectedModuleRecord, ViewError> {
        #[cfg(test)]
        {
            self.last_follow_rejection = None;
        }
        if !self.frame.is_empty() || !self.record.is_empty() || self.record_header.is_some() {
            return Err(ViewError::InvalidStage);
        }
        match self.original_progress().await? {
            ModuleOriginalReadProgress::CandidateReady => {}
            ModuleOriginalReadProgress::Busy => {
                #[cfg(test)]
                {
                    self.last_follow_rejection = Some(FollowRejection::PreflightBusy);
                    eprintln!("[INFLIGHT-FOLLOW-PREFLIGHT] Busy");
                }
                let lane = self
                    .shared
                    .lane
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?;
                return Err(if matches!(*lane, ContinuationLane::AwaitCredit) {
                    ViewError::InvalidCredit
                } else {
                    ViewError::InvalidStage
                });
            }
            ModuleOriginalReadProgress::Failed(error) => return Err(error),
            _ => return Err(ViewError::InvalidStage),
        }
        let nonce =
            EntityId::from_bytes(crate::random::bytes().map_err(|_| ViewError::RandomUnavailable)?);
        let adopted = Arc::new(AtomicBool::new(false));
        {
            let mut lane = self
                .shared
                .lane
                .lock()
                .map_err(|_| ViewError::ReadFailure)?;
            match *lane {
                ContinuationLane::AwaitCredit => return Err(ViewError::InvalidCredit),
                ContinuationLane::Writing | ContinuationLane::Switching(_) => {
                    return Err(ViewError::InvalidStage);
                }
                ContinuationLane::Stopped => return Err(ViewError::StreamSealed),
                ContinuationLane::AwaitData => {}
            }
            let stream = self
                .shared
                .current_stream
                .lock()
                .map_err(|_| ViewError::ReadFailure)?;
            if stream.id() != self.stream {
                return Err(ViewError::InvalidStage);
            }
            let observed = stream.observation()?;
            if observed.phase != LocalTransferPhase::Ready
                || observed.highest_flushed != observed.highest_committed
                || observed.highest_started != observed.highest_flushed
            {
                return Err(ViewError::InvalidCredit);
            }
            if !stream.ready()? {
                return Err(ViewError::InvalidCredit);
            }
            *lane = ContinuationLane::Switching(nonce);
        }
        let claim = LaneClaim {
            shared: Arc::clone(&self.shared),
            nonce,
            adopted: Arc::clone(&adopted),
        };
        let (reply, ready) = oneshot::channel();
        self.continue_read
            .try_send(Continue {
                reply,
                kind: ContinueKind::InFlight { nonce, adopted },
            })
            .map_err(|error| match error {
                tokio::sync::mpsc::error::TrySendError::Closed(_) => ViewError::StreamSealed,
                tokio::sync::mpsc::error::TrySendError::Full(_) => ViewError::InvalidStage,
            })?;
        let Bootstrap {
            peer,
            stream,
            epoch,
        } = ready.await.map_err(|_| ViewError::StreamSealed)??;
        drop(claim);
        self.peer = Some(peer);
        self.stream = stream;
        self.epoch = epoch;
        self.frame.clear();
        self.record.clear();
        self.record_header = None;
        self.stale_reported = false;
        self.credit(0).await?;
        let notice = self.next_record().await?;
        if notice.address().kind() != RecordKind::Status || notice.address().record_id() != 1 {
            return Err(ViewError::ProtocolFailure);
        }
        Ok(notice)
    }
}
impl Sender {
    pub(super) fn begin_prepare(&self) -> Result<bool, ViewError> {
        let mut lane = self
            .shared
            .lane
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        match *lane {
            ContinuationLane::Switching(_) => Ok(false),
            ContinuationLane::Stopped => Err(ViewError::StreamSealed),
            _ => {
                *lane = ContinuationLane::Writing;
                Ok(true)
            }
        }
    }
    pub(super) fn set_idle_lane(&self, next: ContinuationLane) -> Result<(), ViewError> {
        *self
            .shared
            .lane
            .lock()
            .map_err(|_| ViewError::ReadFailure)? = next;
        Ok(())
    }
    pub(super) async fn take_inflight_command(&mut self) -> Result<Option<Continue>, ViewError> {
        let mut wake = self.shared.signal.subscribe();
        let mut stop = self.supervisor.subscribe_stop();
        loop {
            if !matches!(
                *self
                    .shared
                    .lane
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?,
                ContinuationLane::Switching(_)
            ) {
                return Ok(None);
            }
            if *stop.borrow() {
                return Err(ViewError::NodeStopping);
            }
            tokio::select! {
                command=self.commands.recv()=>return command.map(Some).ok_or(ViewError::StreamSealed),
                changed=wake.changed()=>changed.map_err(|_|ViewError::StreamSealed)?,
                _=stop.changed()=>{},
            }
        }
    }
    pub(super) async fn accept_inflight(&mut self, command: Continue) -> Result<bool, ViewError> {
        let ContinueKind::InFlight { nonce, adopted } = &command.kind else {
            let _ = command.reply.send(Err(ViewError::InvalidStage));
            return Ok(false);
        };
        {
            let mut lane = self
                .shared
                .lane
                .lock()
                .map_err(|_| ViewError::ReadFailure)?;
            if command.reply.is_closed()
                || !matches!(*lane,ContinuationLane::Switching(value) if value==*nonce)
            {
                if matches!(*lane,ContinuationLane::Switching(value) if value==*nonce) {
                    *lane = ContinuationLane::AwaitData;
                }
                return Ok(false);
            }
            adopted.store(true, Ordering::Release);
        }
        let old = self.read.as_ref().ok_or(ViewError::StreamSealed)?;
        let result = match self.reads.original_progress(&self.client, old).await {
            Ok(OriginalReadProgress::CandidateReady { .. }) => {
                self.reads
                    .rebind_inflight_same_read(&self.client, old)
                    .await
            }
            Ok(OriginalReadProgress::Failed(error)) | Err(error) => Err(error),
            _ => Err(ViewError::InvalidStage),
        };
        match result {
            Ok(rebound) => {
                self.install_rebound(command, rebound).await?;
                Ok(true)
            }
            Err(error) => {
                self.set_idle_lane(ContinuationLane::AwaitData)?;
                let _ = command.reply.send(Err(error));
                Ok(false)
            }
        }
    }
}
