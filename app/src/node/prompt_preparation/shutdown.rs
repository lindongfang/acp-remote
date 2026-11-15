use super::{
    Arc, Future, Pin, PreparationFailure, PreparationShutdownObservation,
    PreparationShutdownReport, PreparationTaskObservation, PreparationTaskPhase,
    PromptPreparationShutdownPort, State, TaskExit, TaskJoinState, TaskSlot, lifecycle_lock,
};
pub(super) struct PreparationShutdown {
    pub(super) state: Arc<State>,
    pub(super) supervisor: Arc<TaskSlot>,
}
impl PromptPreparationShutdownPort for PreparationShutdown {
    fn seal(&self) {
        self.state.seal();
    }
    fn observe(&self) -> PreparationShutdownObservation {
        let i = lifecycle_lock(&self.state.inner);
        let mut cpu = Vec::new();
        let mut disposal = Vec::new();
        let mut active_use = 0;
        let mut failures = Vec::new();
        for j in &i.jobs {
            let observation = j
                .signal
                .observation
                .lock()
                .map_or_else(|p| p.into_inner().clone(), |g| g.clone());
            cpu.push(PreparationTaskObservation::from_owner_state(
                observation.identity().clone(),
                observation.phase(),
                j.slot.state(),
            ));
            let d = lifecycle_lock(&j.data);
            active_use += d.uses;
            if let Some(dd) = &d.disposal {
                disposal.push(dd.slot.observation(PreparationTaskPhase::Disposing));
            }
        }
        let supervisor = self.supervisor.state();
        for join in cpu
            .iter()
            .chain(&disposal)
            .map(PreparationTaskObservation::join)
            .chain(std::iter::once(supervisor))
        {
            match join {
                TaskJoinState::Joined(TaskExit::Failed(f)) => failures.push(f),
                TaskJoinState::StartUnconfirmed => {
                    failures.push(PreparationFailure::StartUnconfirmed);
                }
                _ => {}
            }
        }
        PreparationShutdownObservation::from_owner_state(
            i.sealed, cpu, disposal, active_use, supervisor, failures,
        )
    }
    fn join(&self) -> Pin<Box<dyn Future<Output = PreparationShutdownReport> + Send + '_>> {
        Box::pin(async move {
            self.seal();
            let _ = self.supervisor.join().await;
            let observation = self.observe();
            if let Some(report) = PreparationShutdownReport::from_owner_observation(observation) {
                report
            } else {
                std::future::pending().await
            }
        })
    }
}
