// Test-only private overlay nested directly under domain::session_capacity.
// Read-only: does not establish safety, touch a clock or synthesize work.
impl super::CapacityIdlePolicy {
    pub(crate) const fn rt_last_activity_for_test(&self) -> std::time::Instant {
        self.last_activity
    }
}
