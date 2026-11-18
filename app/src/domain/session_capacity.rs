//! Monotonic request policy; native close execution and durable release are separate.
use std::time::{Duration, Instant};

mod observed_work;
pub use observed_work::ObservedWork;

pub const DEFAULT_SESSION_LIMIT: u32 = 50;
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_mins(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Coverage {
    Unknown,
    Idle,
    Active,
}

/// Current node observations authorize a request, not a claim about hidden native work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecycleEligibility {
    pub foreground: Coverage,
    pub background: Coverage,
    pub children: Coverage,
    pub pending_approvals: Option<u32>,
    pub root_verified: bool,
    pub management_reconciled: bool,
    pub close_supported: bool,
}
impl RecycleEligibility {
    #[must_use]
    pub fn eligible(self) -> bool {
        self.foreground == Coverage::Idle
            && self.background != Coverage::Active
            && self.children != Coverage::Active
            && self.pending_approvals == Some(0)
            && self.root_verified
            && self.management_reconciled
            && self.close_supported
    }
}

pub struct CapacityIdlePolicy {
    last_observed: Instant,
    last_activity: Instant,
    safe_since: Option<Instant>,
}
impl CapacityIdlePolicy {
    #[must_use]
    pub const fn new(now: Instant) -> Self {
        Self {
            last_observed: now,
            last_activity: now,
            safe_since: None,
        }
    }

    /// # Errors
    /// Rejects a backwards clock rather than extending or shortening an idle window.
    pub fn observe_safety(
        &mut self,
        now: Instant,
        facts: RecycleEligibility,
    ) -> Result<(), CapacityClockError> {
        self.advance(now)?;
        if !facts.eligible() {
            self.safe_since = None;
        } else if self.safe_since.is_none() {
            self.safe_since = Some(now);
        }
        Ok(())
    }

    /// Actual received replay or real work refreshes time; replay supplies no work protection.
    /// Page reads, cache access, heartbeats and merely sending replay never call this method.
    /// # Errors
    /// Rejects backwards observations.
    pub fn received_activity(&mut self, now: Instant) -> Result<(), CapacityClockError> {
        self.advance(now)?;
        self.last_activity = now;
        Ok(())
    }

    /// Most recent activity/safe transition is used for current capacity-pressure ordering.
    /// A recent replay changes rank but does not prevent selecting the sole safe candidate.
    #[must_use]
    pub fn candidate_since(&self) -> Option<Instant> {
        self.safe_since.map(|safe| safe.max(self.last_activity))
    }

    #[must_use]
    pub fn idle_timeout_reached(&self, now: Instant) -> bool {
        self.candidate_since()
            .and_then(|since| now.checked_duration_since(since))
            .is_some_and(|elapsed| elapsed >= DEFAULT_IDLE_TIMEOUT)
    }

    fn advance(&mut self, now: Instant) -> Result<(), CapacityClockError> {
        if now < self.last_observed {
            return Err(CapacityClockError);
        }
        self.last_observed = now;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("capacity observation moved backwards on its monotonic clock")]
pub struct CapacityClockError;

#[cfg(test)]
mod development_tests {
    use super::*;
    fn safe() -> RecycleEligibility {
        RecycleEligibility {
            foreground: Coverage::Idle,
            background: Coverage::Idle,
            children: Coverage::Idle,
            pending_approvals: Some(0),
            root_verified: true,
            management_reconciled: true,
            close_supported: true,
        }
    }
    #[test]
    fn development_fixture_replay_refreshes_timeout_without_work_protection() {
        let start = Instant::now();
        let mut policy = CapacityIdlePolicy::new(start);
        policy
            .observe_safety(start, safe())
            .expect("verified development facts");
        assert!(!policy.idle_timeout_reached(start + Duration::from_secs(1799)));
        assert!(policy.idle_timeout_reached(start + DEFAULT_IDLE_TIMEOUT));
        policy
            .received_activity(start + Duration::from_mins(29))
            .expect("actual replay receive");
        assert_eq!(
            policy.candidate_since(),
            Some(start + Duration::from_mins(29))
        );
        assert!(!policy.idle_timeout_reached(start + DEFAULT_IDLE_TIMEOUT));
        assert!(policy.idle_timeout_reached(start + Duration::from_mins(59)));
        assert_eq!(DEFAULT_SESSION_LIMIT, 50);
    }
    #[test]
    fn unreported_internal_coverage_allows_request_without_claiming_idle() {
        let start = Instant::now();
        let mut facts = safe();
        facts.background = Coverage::Unknown;
        facts.children = Coverage::Unknown;
        let mut policy = CapacityIdlePolicy::new(start);
        policy.observe_safety(start, facts).expect("current facts");
        assert_eq!(policy.candidate_since(), Some(start));
        assert!(policy.idle_timeout_reached(start + DEFAULT_IDLE_TIMEOUT));
        assert_eq!(facts.background, Coverage::Unknown);
        assert_eq!(facts.children, Coverage::Unknown);
    }
    #[test]
    fn necessary_unknowns_and_known_active_work_protect() {
        let now = Instant::now();
        let mut cases = Vec::new();
        for coverage in [Coverage::Unknown, Coverage::Active] {
            let mut facts = safe();
            facts.foreground = coverage;
            cases.push(facts);
        }
        let mut facts = safe();
        facts.background = Coverage::Active;
        cases.push(facts);
        let mut facts = safe();
        facts.children = Coverage::Active;
        cases.push(facts);
        for approvals in [None, Some(1)] {
            let mut facts = safe();
            facts.pending_approvals = approvals;
            cases.push(facts);
        }
        let mut facts = safe();
        facts.close_supported = false;
        cases.push(facts);
        let mut facts = safe();
        facts.management_reconciled = false;
        cases.push(facts);
        let mut facts = safe();
        facts.root_verified = false;
        cases.push(facts);
        for facts in cases {
            let mut policy = CapacityIdlePolicy::new(now);
            policy
                .observe_safety(now, facts)
                .expect("development observation");
            assert_eq!(policy.candidate_since(), None);
            assert!(!policy.idle_timeout_reached(now + DEFAULT_IDLE_TIMEOUT));
        }
    }
    #[test]
    fn development_fixture_last_approval_resets_safety_window_and_latest_replay_ranks() {
        let now = Instant::now();
        let mut a = CapacityIdlePolicy::new(now);
        let mut b = CapacityIdlePolicy::new(now);
        a.observe_safety(now, safe()).expect("safe A");
        b.observe_safety(now, safe()).expect("safe B");
        a.received_activity(now + Duration::from_secs(5))
            .expect("recent replay");
        assert!(b.candidate_since() < a.candidate_since());
        let mut protected = safe();
        protected.pending_approvals = Some(1);
        b.observe_safety(now + Duration::from_secs(2000), protected)
            .expect("approval");
        assert_eq!(b.candidate_since(), None);
        b.observe_safety(now + Duration::from_secs(4000), safe())
            .expect("last approval gone");
        assert!(!b.idle_timeout_reached(now + Duration::from_secs(5799)));
        assert!(b.idle_timeout_reached(now + Duration::from_secs(5800)));
        assert!(b.received_activity(now).is_err());
    }
}

#[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
#[path = "runtime_child_policy_clock_probe.rs"]
mod runtime_child_policy_clock_probe;
