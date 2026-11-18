//! Current observations stay with the source, including sessions without a platform target.
use super::{
    CapacityFreshness, CapacityObservation, CapacityState, CapacityUpdate, ProtocolVersion,
};
use crate::domain::session_capacity::{
    CapacityClockError, CapacityIdlePolicy, Coverage, ObservedWork, RecycleEligibility,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

pub(super) struct SessionActivity {
    work: ObservedWork,
    tools: HashMap<Arc<str>, bool>,
    /// A known association needs the complete tree owner before automatic selection.
    children_reported: bool,
    policy: CapacityIdlePolicy,
}

impl SessionActivity {
    fn new(now: Instant) -> Self {
        Self {
            work: ObservedWork::default(),
            tools: HashMap::new(),
            children_reported: false,
            policy: CapacityIdlePolicy::new(now),
        }
    }

    fn state(&mut self, state: CapacityState) {
        self.work.observe(state.foreground, state.background);
    }

    fn update(&mut self, observation: &CapacityObservation) {
        match observation.freshness {
            CapacityFreshness::History => return,
            CapacityFreshness::Unconfirmed => {
                self.work.observe(Coverage::Unknown, None);
                return;
            }
            CapacityFreshness::Current => {}
        }
        match &observation.update {
            CapacityUpdate::State(state) => self.state(*state),
            CapacityUpdate::Tool { id, active } => {
                if let Some(active) = active {
                    self.tools.insert(id.clone(), *active);
                } else {
                    self.tools.entry(id.clone()).or_insert(true);
                }
            }
            CapacityUpdate::Child { .. } => {
                self.children_reported = true;
            }
            CapacityUpdate::Activity => {}
        }
    }

    pub(super) fn touch(&mut self, now: Instant) -> Result<(), CapacityClockError> {
        self.policy.received_activity(now)
    }

    #[cfg(test)]
    pub(super) fn evaluate(
        &mut self,
        now: Instant,
        mut eligibility: RecycleEligibility,
    ) -> Result<Option<Instant>, CapacityClockError> {
        eligibility.foreground = self.work.foreground();
        eligibility.background = self.work.background();
        if self.children_reported || self.tools.values().any(|active| *active) {
            eligibility.children = Coverage::Active;
        }
        self.policy.observe_safety(now, eligibility)?;
        Ok(self.policy.candidate_since())
    }
}

#[derive(Default)]
pub(super) struct CapacityActivity {
    sessions: HashMap<Arc<str>, SessionActivity>,
    unowned: HashSet<Arc<str>>,
    tree: super::capacity_tree::CapacityTree,
}

impl CapacityActivity {
    pub(super) fn created(&mut self, session: &str, wire: ProtocolVersion, now: Instant) {
        self.recognize(session);
        let activity = self
            .sessions
            .entry(Arc::from(session))
            .or_insert_with(|| SessionActivity::new(now));
        if wire == ProtocolVersion::V2 {
            activity.work.created();
        }
    }

    pub(super) fn observe(
        &mut self,
        observation: &CapacityObservation,
        now: Instant,
    ) -> Result<(), CapacityClockError> {
        let activity = self
            .sessions
            .entry(observation.session.clone())
            .or_insert_with(|| SessionActivity::new(now));
        activity.policy.received_activity(now)?;
        activity.update(observation);
        if let CapacityUpdate::Child { session, state } = &observation.update {
            let child = self
                .sessions
                .entry(session.clone())
                .or_insert_with(|| SessionActivity::new(now));
            child.touch(now)?;
            if observation.freshness == CapacityFreshness::Current {
                match state {
                    Some(Some(state)) => child.state(*state),
                    Some(None) => child.work.observe(Coverage::Unknown, None),
                    None => {}
                }
            }
        }
        for ancestor in self.tree.ancestors(&observation.session) {
            self.sessions
                .entry(ancestor)
                .or_insert_with(|| SessionActivity::new(now))
                .touch(now)?;
        }
        Ok(())
    }

    pub(super) fn verify_root(&mut self, session: &str) -> Result<(), super::DispatchError> {
        self.tree.root(session)?;
        for member in self.tree.members(session)? {
            self.recognize(&member);
        }
        Ok(())
    }

    pub(super) fn associate(
        &mut self,
        parent: &str,
        child: &str,
    ) -> Result<(), super::DispatchError> {
        self.tree.associate(parent, child)
    }

    pub(super) fn members(&self, root: &str) -> Result<Vec<Arc<str>>, super::DispatchError> {
        self.tree.members(root)
    }

    pub(super) fn recognizes_member(&self, session: &str) -> bool {
        self.tree.contains(session)
    }

    pub(super) fn root_for(&self, session: &str) -> Option<Arc<str>> {
        self.tree.root_for(session)
    }

    pub(super) fn invalidate_candidate(
        &mut self,
        session: &str,
        now: Instant,
    ) -> Result<(), super::DispatchError> {
        if let Some(session) = self.sessions.get_mut(session) {
            session
                .policy
                .observe_safety(
                    now,
                    RecycleEligibility {
                        foreground: Coverage::Unknown,
                        background: Coverage::Unknown,
                        children: Coverage::Unknown,
                        pending_approvals: None,
                        root_verified: false,
                        management_reconciled: false,
                        close_supported: false,
                    },
                )
                .map_err(|_| super::DispatchError::ObservationOrder)?;
        }
        Ok(())
    }

    pub(super) fn evaluate_tree(
        &mut self,
        root: &str,
        now: Instant,
        mut eligibility: RecycleEligibility,
    ) -> Result<Option<Instant>, super::DispatchError> {
        let members = self.tree.members(root)?;
        let protected = members
            .iter()
            .filter(|member| member.as_ref() != root)
            .any(|member| {
                self.sessions.get(member.as_ref()).is_none_or(|session| {
                    session.work.foreground() != Coverage::Idle
                        || session.work.background() == Coverage::Active
                        || session.tools.values().any(|active| *active)
                })
            });
        let Some(session) = self.sessions.get_mut(root) else {
            return Ok(None);
        };
        eligibility.foreground = session.work.foreground();
        eligibility.background = session.work.background();
        if protected || session.tools.values().any(|active| *active) {
            eligibility.children = Coverage::Active;
        }
        session
            .policy
            .observe_safety(now, eligibility)
            .map_err(|_| super::DispatchError::ObservationOrder)?;
        Ok(session.policy.candidate_since())
    }

    pub(super) fn touch(&mut self, session: &str, now: Instant) -> Result<(), CapacityClockError> {
        self.sessions
            .entry(Arc::from(session))
            .or_insert_with(|| SessionActivity::new(now))
            .policy
            .received_activity(now)?;
        for ancestor in self.tree.ancestors(session) {
            self.sessions
                .entry(ancestor)
                .or_insert_with(|| SessionActivity::new(now))
                .touch(now)?;
        }
        Ok(())
    }

    pub(super) fn forget_closed(&mut self, session: &str) {
        if let Ok(members) = self.tree.members(session) {
            for member in members {
                self.sessions.remove(member.as_ref());
            }
        } else {
            self.sessions.remove(session);
        }
    }

    #[cfg(test)]
    pub(super) fn session(&mut self, session: &str) -> Option<&mut SessionActivity> {
        self.sessions.get_mut(session)
    }

    pub(super) fn unrecognized(&mut self, session: &str) {
        self.unowned.insert(Arc::from(session));
    }

    pub(super) fn recognize(&mut self, session: &str) {
        self.unowned.remove(session);
    }

    pub(super) fn has_unowned(&self) -> bool {
        !self.unowned.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::super::ObservationOrigin;
    use super::*;

    const fn freshness(origin: ObservationOrigin) -> CapacityFreshness {
        match origin {
            ObservationOrigin::Live | ObservationOrigin::DuringResume => CapacityFreshness::Current,
            ObservationOrigin::DuringReplay => CapacityFreshness::History,
        }
    }

    fn eligibility() -> RecycleEligibility {
        RecycleEligibility {
            foreground: Coverage::Unknown,
            background: Coverage::Unknown,
            children: Coverage::Unknown,
            pending_approvals: Some(0),
            root_verified: true,
            management_reconciled: true,
            close_supported: true,
        }
    }

    fn tool(active: Option<bool>, origin: ObservationOrigin) -> CapacityObservation {
        CapacityObservation {
            session: Arc::from("root"),
            origin,
            freshness: freshness(origin),
            update: CapacityUpdate::Tool {
                id: Arc::from("tool"),
                active,
            },
        }
    }

    #[test]
    fn newly_observed_tool_defaults_pending_and_missing_does_not_clear_it() {
        let now = Instant::now();
        let mut activity = CapacityActivity::default();
        activity.created("root", ProtocolVersion::V2, now);
        activity
            .observe(&tool(None, ObservationOrigin::Live), now)
            .unwrap();
        assert_eq!(
            activity
                .session("root")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap(),
            None
        );
        activity
            .observe(&tool(Some(false), ObservationOrigin::Live), now)
            .unwrap();
        assert_eq!(
            activity
                .session("root")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap(),
            Some(now)
        );
        activity
            .observe(&tool(None, ObservationOrigin::Live), now)
            .unwrap();
        assert_eq!(
            activity
                .session("root")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap(),
            Some(now)
        );
    }

    #[test]
    fn replay_refreshes_time_without_resurrecting_tool_work() {
        let now = Instant::now();
        let later = now + std::time::Duration::from_secs(10);
        let mut activity = CapacityActivity::default();
        activity.created("root", ProtocolVersion::V2, now);
        assert_eq!(
            activity
                .session("root")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap(),
            Some(now)
        );
        activity
            .observe(&tool(Some(true), ObservationOrigin::DuringReplay), later)
            .unwrap();
        assert_eq!(
            activity
                .session("root")
                .unwrap()
                .evaluate(later, eligibility())
                .unwrap(),
            Some(later)
        );
    }

    #[test]
    fn current_resume_snapshot_wins_over_new_default_and_sessions_do_not_share_facts() {
        let now = Instant::now();
        let mut activity = CapacityActivity::default();
        activity
            .observe(
                &CapacityObservation {
                    session: Arc::from("root"),
                    origin: ObservationOrigin::DuringResume,
                    freshness: CapacityFreshness::Current,
                    update: CapacityUpdate::State(CapacityState {
                        foreground: Coverage::Active,
                        background: None,
                    }),
                },
                now,
            )
            .unwrap();
        activity.created("root", ProtocolVersion::V2, now);
        activity.created("other", ProtocolVersion::V2, now);
        assert_eq!(
            activity
                .session("root")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap(),
            None
        );
        assert_eq!(
            activity
                .session("other")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap(),
            Some(now)
        );
        assert!(activity.session("unknown").is_none());
    }

    #[test]
    fn known_child_requires_tree_integration_even_if_parent_is_idle() {
        let now = Instant::now();
        let mut activity = CapacityActivity::default();
        activity.created("root", ProtocolVersion::V2, now);
        activity
            .observe(
                &CapacityObservation {
                    session: Arc::from("root"),
                    origin: ObservationOrigin::Live,
                    freshness: CapacityFreshness::Current,
                    update: CapacityUpdate::Child {
                        session: Arc::from("child"),
                        state: None,
                    },
                },
                now,
            )
            .unwrap();
        assert_eq!(
            activity
                .session("root")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap(),
            None
        );
    }

    #[test]
    fn historical_idle_does_not_clear_current_busy_work() {
        let now = Instant::now();
        let mut activity = CapacityActivity::default();
        for (origin, foreground, background) in [
            (ObservationOrigin::Live, Coverage::Active, Coverage::Active),
            (
                ObservationOrigin::DuringReplay,
                Coverage::Idle,
                Coverage::Idle,
            ),
        ] {
            activity
                .observe(
                    &CapacityObservation {
                        session: Arc::from("root"),
                        origin,
                        freshness: freshness(origin),
                        update: CapacityUpdate::State(CapacityState {
                            foreground,
                            background: Some(background),
                        }),
                    },
                    now,
                )
                .unwrap();
        }
        assert_eq!(
            activity
                .session("root")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap(),
            None
        );
    }

    #[test]
    fn recognized_created_identity_only_clears_its_own_unknown_scope() {
        let now = Instant::now();
        let mut activity = CapacityActivity::default();
        activity.unrecognized("created-before-decoding");
        activity.unrecognized("stranger");
        activity.created("created-before-decoding", ProtocolVersion::V2, now);
        assert!(activity.has_unowned());
        activity.recognize("stranger");
        assert!(!activity.has_unowned());
    }

    #[test]
    fn confirmed_current_snapshot_after_replay_can_clear_work() {
        let now = Instant::now();
        let mut activity = CapacityActivity::default();
        for (freshness, foreground) in [
            (CapacityFreshness::Current, Coverage::Active),
            (CapacityFreshness::Unconfirmed, Coverage::Idle),
            (CapacityFreshness::Current, Coverage::Idle),
        ] {
            activity
                .observe(
                    &CapacityObservation {
                        session: Arc::from("root"),
                        origin: ObservationOrigin::DuringReplay,
                        freshness,
                        update: CapacityUpdate::State(CapacityState {
                            foreground,
                            background: Some(foreground),
                        }),
                    },
                    now,
                )
                .unwrap();
            let candidate = activity
                .session("root")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap();
            assert_eq!(
                candidate.is_some(),
                freshness == CapacityFreshness::Current && foreground == Coverage::Idle
            );
        }
    }

    #[test]
    fn current_tool_after_replay_is_not_misclassified_as_history() {
        let now = Instant::now();
        let mut activity = CapacityActivity::default();
        activity.created("root", ProtocolVersion::V2, now);
        let mut observation = tool(Some(true), ObservationOrigin::DuringReplay);
        observation.freshness = CapacityFreshness::Current;
        activity.observe(&observation, now).unwrap();
        assert_eq!(
            activity
                .session("root")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap(),
            None
        );
    }

    #[test]
    fn replacing_a_closed_generation_does_not_reuse_current_state_or_timer() {
        let now = Instant::now();
        let mut activity = CapacityActivity::default();
        activity.created("root", ProtocolVersion::V2, now);
        assert!(
            activity
                .session("root")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap()
                .is_some()
        );
        activity.forget_closed("root");
        assert!(activity.session("root").is_none());
        activity.touch("root", now).unwrap();
        assert_eq!(
            activity
                .session("root")
                .unwrap()
                .evaluate(now, eligibility())
                .unwrap(),
            None
        );
    }

    #[test]
    fn child_state_and_missing_fields_protect_the_entire_tree() {
        let now = Instant::now();
        let mut activity = CapacityActivity::default();
        activity.created("root", ProtocolVersion::V2, now);
        activity.verify_root("root").unwrap();
        activity.associate("root", "child").unwrap();
        for (state, protected) in [
            (None, true),
            (
                Some(Some(CapacityState {
                    foreground: Coverage::Idle,
                    background: Some(Coverage::Active),
                })),
                true,
            ),
            (
                Some(Some(CapacityState {
                    foreground: Coverage::Idle,
                    background: None,
                })),
                true,
            ),
            (
                Some(Some(CapacityState {
                    foreground: Coverage::Idle,
                    background: Some(Coverage::Idle),
                })),
                false,
            ),
            (Some(None), true),
        ] {
            activity
                .observe(
                    &CapacityObservation {
                        session: Arc::from("root"),
                        origin: ObservationOrigin::Live,
                        freshness: CapacityFreshness::Current,
                        update: CapacityUpdate::Child {
                            session: Arc::from("child"),
                            state,
                        },
                    },
                    now,
                )
                .unwrap();
            assert_eq!(
                activity
                    .evaluate_tree("root", now, eligibility())
                    .unwrap()
                    .is_none(),
                protected
            );
        }
    }

    #[test]
    fn actual_descendant_replay_refreshes_root_without_resurrecting_work() {
        let now = Instant::now();
        let later = now + std::time::Duration::from_mins(29);
        let mut activity = CapacityActivity::default();
        activity.created("root", ProtocolVersion::V2, now);
        activity.verify_root("root").unwrap();
        activity.associate("root", "child").unwrap();
        activity
            .observe(
                &CapacityObservation {
                    session: Arc::from("root"),
                    origin: ObservationOrigin::Live,
                    freshness: CapacityFreshness::Current,
                    update: CapacityUpdate::Child {
                        session: Arc::from("child"),
                        state: Some(Some(CapacityState {
                            foreground: Coverage::Idle,
                            background: None,
                        })),
                    },
                },
                now,
            )
            .unwrap();
        assert_eq!(
            activity.evaluate_tree("root", now, eligibility()).unwrap(),
            Some(now)
        );
        let mut replay = tool(Some(true), ObservationOrigin::DuringReplay);
        replay.session = Arc::from("child");
        activity.observe(&replay, later).unwrap();
        assert_eq!(
            activity
                .evaluate_tree("root", later, eligibility())
                .unwrap(),
            Some(later)
        );
    }

    #[test]
    fn descendant_approval_completion_restarts_the_root_idle_interval() {
        let now = Instant::now();
        let later = now + std::time::Duration::from_mins(29);
        let mut activity = CapacityActivity::default();
        activity.created("root", ProtocolVersion::V2, now);
        activity.created("child", ProtocolVersion::V2, now);
        activity.verify_root("root").unwrap();
        activity.associate("root", "child").unwrap();
        assert_eq!(
            activity.evaluate_tree("root", now, eligibility()).unwrap(),
            Some(now)
        );
        activity.touch("child", later).unwrap();
        assert_eq!(
            activity
                .evaluate_tree("root", later, eligibility())
                .unwrap(),
            Some(later)
        );
    }

    #[test]
    fn restored_identity_needs_current_descendant_state_in_every_generation() {
        let now = Instant::now();
        let mut activity = CapacityActivity::default();
        activity.verify_root("root").unwrap();
        activity.associate("root", "child").unwrap();
        activity.created("root", ProtocolVersion::V2, now);
        assert!(
            activity
                .evaluate_tree("root", now, eligibility())
                .unwrap()
                .is_none()
        );
        activity.created("child", ProtocolVersion::V2, now);
        assert!(
            activity
                .evaluate_tree("root", now, eligibility())
                .unwrap()
                .is_some()
        );
        activity.forget_closed("root");
        assert_eq!(activity.members("root").unwrap().len(), 2);
        activity.created("root", ProtocolVersion::V2, now);
        assert!(
            activity
                .evaluate_tree("root", now, eligibility())
                .unwrap()
                .is_none()
        );
    }
}

#[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
#[path = "runtime_child_activity_clock_probe.rs"]
mod runtime_child_activity_clock_probe;
