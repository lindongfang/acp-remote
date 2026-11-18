use super::{NativeQueueDriver, OwnerCell, RuntimeCore, lock};
use crate::{
    application::native_dispatch::{DispatchIoError, DispatchIoProgress},
    domain::{EntityId, native::NativeSessionKey, native_dispatch::DispatchError},
};
use std::{sync::Arc, task::Poll};

type IoPoll = Result<Poll<Result<DispatchIoProgress, DispatchIoError>>, DispatchError>;

pub trait AutomaticCloseGuard: Send + Sync {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Retain the original non-revision callback API for internal callers"
        )
    )]
    fn poll_idle(&self, operation: EntityId, poll: &mut dyn FnMut() -> IoPoll) -> IoPoll {
        self.poll_idle_revision(operation, &mut |_| poll())
    }
    fn poll_idle_revision(
        &self,
        operation: EntityId,
        poll: &mut dyn FnMut(u64) -> IoPoll,
    ) -> IoPoll;
    fn poll_frozen(&self, operation: EntityId, poll: &mut dyn FnMut() -> IoPoll) -> IoPoll;
    fn finish(&self, operation: EntityId) -> Result<bool, DispatchError>;
}

struct AutomaticCloseOwner<T> {
    owners: Vec<Arc<OwnerCell<T>>>,
    core: Arc<RuntimeCore<T>>,
}

impl<T: Send + 'static> NativeQueueDriver<T> {
    #[cfg(test)]
    pub(crate) fn automatic_close_guard(
        &self,
        native: &NativeSessionKey,
    ) -> Result<Arc<dyn AutomaticCloseGuard>, DispatchError> {
        self.automatic_close_tree_guard(std::slice::from_ref(native))
    }

    pub(crate) fn automatic_close_tree_guard(
        &self,
        natives: &[NativeSessionKey],
    ) -> Result<Arc<dyn AutomaticCloseGuard>, DispatchError> {
        self.core.running()?;
        if natives.is_empty() {
            return Err(DispatchError::InvalidTarget);
        }
        let mut natives = natives.to_vec();
        natives.sort_by(|a, b| {
            a.source()
                .0
                .encode()
                .cmp(&b.source().0.encode())
                .then_with(|| a.native_id().cmp(b.native_id()))
        });
        natives.dedup();
        let owners = natives
            .into_iter()
            .map(|native| self.core.owner_for(native))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Arc::new(AutomaticCloseOwner {
            owners,
            core: self.core.clone(),
        }))
    }
}

impl<T: Send + 'static> AutomaticCloseGuard for AutomaticCloseOwner<T> {
    fn poll_idle_revision(
        &self,
        operation: EntityId,
        poll: &mut dyn FnMut(u64) -> IoPoll,
    ) -> IoPoll {
        let mut states = self
            .owners
            .iter()
            .map(|owner| lock(&owner.state))
            .collect::<Result<Vec<_>, _>>()?;
        self.core.running()?;
        if states.iter().any(|state| {
            state.automatic_close.is_some()
                || state.queue.waiting().next().is_some()
                || state.queue.active().is_some()
                || state.queue.priority().is_some()
                || state.pending.is_some()
                || state.active.is_some()
                || state.external.is_some()
                || state.priority_handoff.is_some()
                || state.selected_priority.is_some()
        }) {
            return Err(DispatchError::NotReady);
        }
        let revision = states.iter().try_fold(0_u64, |sum, state| {
            sum.checked_add(state.native_order)
                .ok_or(DispatchError::SequenceExhausted)
        })?;
        let result = poll(revision);
        if matches!(result, Ok(Poll::Ready(Ok(DispatchIoProgress::Accepted(count)))) if count > 0) {
            for state in &mut states {
                state.automatic_close = Some(operation);
            }
        }
        drop(states);
        result
    }

    fn poll_frozen(&self, operation: EntityId, poll: &mut dyn FnMut() -> IoPoll) -> IoPoll {
        let states = self
            .owners
            .iter()
            .map(|owner| lock(&owner.state))
            .collect::<Result<Vec<_>, _>>()?;
        self.core.running()?;
        if states
            .iter()
            .any(|state| state.automatic_close != Some(operation))
        {
            return Err(DispatchError::StaleSelection);
        }
        let result = poll();
        drop(states);
        result
    }

    fn finish(&self, operation: EntityId) -> Result<bool, DispatchError> {
        let mut states = self
            .owners
            .iter()
            .map(|owner| lock(&owner.state))
            .collect::<Result<Vec<_>, _>>()?;
        let mut finished = false;
        for state in &mut states {
            if state.automatic_close == Some(operation) {
                state.automatic_close = None;
                finished = true;
            }
        }
        drop(states);
        if finished {
            for owner in &self.owners {
                owner.changed.notify_waiters();
            }
        }
        Ok(finished)
    }
}
