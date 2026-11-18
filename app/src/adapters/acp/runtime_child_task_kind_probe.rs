// Private cfg(test) overlay under adapters::acp::native_views.
impl super::AcpSourceViews {
    pub(crate) fn rt_spawn_target_kind_for_test(
        &self,
        target: &crate::domain::native::NativeSessionKey,
        kind: crate::application::shared_views::tasks::ViewTaskKind,
        work: crate::application::shared_views::BoxReadFuture<
            'static,
            Result<(), crate::domain::shared_view::ViewError>,
        >,
    ) -> Result<(), crate::domain::shared_view::ViewError> {
        let owner = self
            .owner
            .lock()
            .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?
            .clone()
            .ok_or(crate::domain::shared_view::ViewError::SourceReleased)?;
        let generation = owner.handle().core.target(target)?.send.clone();
        owner
            .supervisor
            .spawn_target(kind, target, generation, move |_| work)
    }
}
