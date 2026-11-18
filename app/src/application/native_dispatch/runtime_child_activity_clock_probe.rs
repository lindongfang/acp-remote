#[cfg(feature = "approval-owner-acceptance-fixtures")]
type L6KnownSessionWork = (
    std::sync::Arc<str>,
    crate::domain::session_capacity::Coverage,
    crate::domain::session_capacity::Coverage,
    usize,
    Option<std::sync::Arc<str>>,
);
// Test-only private overlay nested under native_dispatch::capacity_activity.
impl super::CapacityActivity {
    pub(crate) fn rt_current_foreground_for_test(
        &self,
        session: &str,
    ) -> Option<(
        crate::domain::session_capacity::Coverage,
        std::time::Instant,
    )> {
        self.sessions.get(session).map(|activity| {
            (
                activity.work.foreground(),
                activity.policy.rt_last_activity_for_test(),
            )
        })
    }
    #[cfg(feature = "approval-owner-acceptance-fixtures")]
    pub(crate) fn rt_activity_clocks_for_test(
        &self,
    ) -> Vec<(std::sync::Arc<str>, std::time::Instant)> {
        self.sessions
            .iter()
            .map(|(session, activity)| {
                (session.clone(), activity.policy.rt_last_activity_for_test())
            })
            .collect()
    }
}

#[cfg(feature = "approval-owner-acceptance-fixtures")]
#[test]
fn l6_capacity_post_snapshot_keeps_each_session_projection() {
    use super::super::{
        CapacityFreshness, CapacityObservation, CapacityState, CapacityUpdate, ObservationOrigin,
    };
    use crate::domain::session_capacity::Coverage;
    use std::sync::Arc;
    let now = std::time::Instant::now();
    let mut activity = super::CapacityActivity::default();
    let event = |session: &str, update| CapacityObservation {
        session: Arc::from(session),
        origin: ObservationOrigin::Live,
        freshness: CapacityFreshness::Current,
        update,
    };
    activity
        .observe(
            &event(
                "owned",
                CapacityUpdate::State(CapacityState {
                    foreground: Coverage::Idle,
                    background: Some(Coverage::Active),
                }),
            ),
            now,
        )
        .unwrap();
    activity
        .observe(
            &event(
                "owned",
                CapacityUpdate::Tool {
                    id: Arc::from("original"),
                    active: Some(true),
                },
            ),
            now,
        )
        .unwrap();
    activity
        .observe(
            &event(
                "owned",
                CapacityUpdate::State(CapacityState {
                    foreground: Coverage::Idle,
                    background: None,
                }),
            ),
            now,
        )
        .unwrap();
    let missing = activity.l6_session_post_for_test("owned").unwrap();
    activity
        .observe(
            &event(
                "other",
                CapacityUpdate::Tool {
                    id: Arc::from("unrelated"),
                    active: Some(true),
                },
            ),
            now,
        )
        .unwrap();
    activity
        .observe(
            &event(
                "owned",
                CapacityUpdate::Tool {
                    id: Arc::from("original"),
                    active: Some(false),
                },
            ),
            now,
        )
        .unwrap();
    activity
        .observe(
            &event(
                "owned",
                CapacityUpdate::State(CapacityState {
                    foreground: Coverage::Idle,
                    background: Some(Coverage::Idle),
                }),
            ),
            now,
        )
        .unwrap();
    let ended = activity.l6_session_post_for_test("owned").unwrap();
    assert_eq!(missing.background, Coverage::Active);
    assert_eq!(missing.active_tools, vec![Arc::from("original")]);
    assert_eq!(ended.background, Coverage::Idle);
    assert!(ended.active_tools.is_empty());
    assert!(activity.l6_session_post_for_test("absent").is_none());
}

impl super::CapacityActivity {
    #[cfg(feature = "approval-owner-acceptance-fixtures")]
    pub(crate) fn l6_candidate_clocks_for_test(
        &self,
    ) -> Vec<(std::sync::Arc<str>, Option<std::time::Instant>)> {
        self.sessions
            .iter()
            .map(|(id, a)| (id.clone(), a.policy.candidate_since()))
            .collect()
    }
}

impl super::CapacityActivity {
    #[cfg(feature = "approval-owner-acceptance-fixtures")]
    pub(crate) fn l6_known_work_for_test(&self) -> Vec<L6KnownSessionWork> {
        self.sessions
            .iter()
            .map(|(id, s)| {
                (
                    id.clone(),
                    s.work.foreground(),
                    s.work.background(),
                    s.tools.values().filter(|v| **v).count(),
                    self.tree.root_for(id),
                )
            })
            .collect()
    }
}

impl super::CapacityActivity {
    #[cfg(feature = "approval-owner-acceptance-fixtures")]
    pub fn l6_session_post_for_test(
        &self,
        session: &str,
    ) -> Option<super::super::runtime_child_source_clock_probe::L6CapacityPostState> {
        self.sessions.get(session).map(|activity| {
            let mut active_tools = activity
                .tools
                .iter()
                .filter(|(_, active)| **active)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            active_tools.sort();
            super::super::runtime_child_source_clock_probe::L6CapacityPostState {
                foreground: activity.work.foreground(),
                background: activity.work.background(),
                active_tools,
            }
        })
    }

    #[cfg(feature = "approval-owner-acceptance-fixtures")]
    pub fn l6_active_tool_ids_for_test(
        &self,
    ) -> Vec<(std::sync::Arc<str>, Vec<std::sync::Arc<str>>)> {
        self.sessions
            .iter()
            .map(|(session, activity)| {
                let mut tools = activity
                    .tools
                    .iter()
                    .filter(|(_, active)| **active)
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>();
                tools.sort();
                (session.clone(), tools)
            })
            .collect()
    }
}
