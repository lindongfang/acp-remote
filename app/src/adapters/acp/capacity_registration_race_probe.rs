//! Only installs a cut on the actual ACP attachment's original supervisor.
impl super::AcpSourceViews {
    pub(crate) fn capacity_install_registration_gate(
        &self,
        gate: std::sync::Arc<crate::application::shared_views::tasks::capacity_registration_race_hook::RegistrationRaceGate>,
    ) {
        let owner = self.owner.lock().unwrap().clone().unwrap();
        owner.supervisor.capacity_install_registration_gate(gate);
    }
}
