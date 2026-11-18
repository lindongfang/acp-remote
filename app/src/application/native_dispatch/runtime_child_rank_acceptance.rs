// Private overlay: child module of capacity_automatic_rank.
// These are policy/rank composition assertions, not transport/SQLite proof.
use super::*;
use crate::domain::session_capacity::{CapacityIdlePolicy, Coverage, RecycleEligibility};
use std::time::Duration;

fn current_idle() -> RecycleEligibility {
    RecycleEligibility {
        foreground: Coverage::Idle,
        background: Coverage::Unknown,
        children: Coverage::Unknown,
        pending_approvals: Some(0),
        root_verified: true,
        management_reconciled: true,
        close_supported: true,
    }
}

fn selection(original: Rank) -> PressureSelection {
    PressureSelection {
        original,
        refresh: Arc::new(|| Err(DispatchError::Stopped)),
    }
}

#[test]
fn rt_006a_new_candidate_starts_at_observation_and_cannot_inherit_old_age() {
    let old = Instant::now();
    let selected_at = old + Duration::from_secs(1);
    let appeared = old + Duration::from_secs(2);
    let mut b = CapacityIdlePolicy::new(old);
    assert_eq!(b.candidate_since(), None);
    b.observe_safety(appeared, current_idle()).unwrap();
    assert_eq!(b.candidate_since(), Some(appeared));
    let original = selection((selected_at, "source".into(), Arc::from("A"), "epoch".into()));
    assert!(original.permits(
        selected_at,
        &PressureCut {
            started: appeared,
            first_other: Some((
                b.candidate_since().unwrap(),
                "source".into(),
                Arc::from("B"),
                "b".into()
            )),
        }
    ));
    // Activity received during the rank scan is conservative Zero, even if no
    // competing tree existed. Equality is a genuine coarse-clock boundary.
    assert!(!original.permits(
        appeared,
        &PressureCut {
            started: appeared,
            first_other: None,
        }
    ));
}

#[test]
fn rt_006b_recovered_candidate_loses_old_safe_age_until_current_requalification() {
    let old = Instant::now();
    let a_since = old + Duration::from_secs(1);
    let blocked = old + Duration::from_secs(2);
    let recovered = old + Duration::from_secs(3);
    let mut b = CapacityIdlePolicy::new(old);
    b.observe_safety(old, current_idle()).unwrap();
    assert_eq!(b.candidate_since(), Some(old));
    let mut busy = current_idle();
    busy.foreground = Coverage::Active;
    b.observe_safety(blocked, busy).unwrap();
    assert_eq!(b.candidate_since(), None);
    b.observe_safety(recovered, current_idle()).unwrap();
    assert_eq!(b.candidate_since(), Some(recovered));
    let a = selection((a_since, "source".into(), Arc::from("A"), "a".into()));
    assert!(a.permits(
        a_since,
        &PressureCut {
            started: recovered + Duration::from_secs(1),
            first_other: Some((
                b.candidate_since().unwrap(),
                "source".into(),
                Arc::from("B"),
                "b".into()
            )),
        }
    ));
    // A current eligible B really older than A must defeat A. This guards
    // against turning the preceding requalification rule into blanket immunity.
    assert!(!a.permits(
        a_since,
        &PressureCut {
            started: recovered + Duration::from_secs(1),
            first_other: Some((old, "source".into(), Arc::from("B"), "b".into())),
        }
    ));
}

#[test]
fn rt_006c_equal_time_uses_source_then_native_then_load_epoch_in_both_directions() {
    let since = Instant::now();
    let selected = (
        since,
        "source-b".into(),
        Arc::from("native-b"),
        "epoch-b".into(),
    );
    let a = selection(selected.clone());
    for (other, expected) in [
        (
            (
                since,
                "source-a".into(),
                Arc::from("native-z"),
                "epoch-z".into(),
            ),
            false,
        ),
        (
            (
                since,
                "source-c".into(),
                Arc::from("native-a"),
                "epoch-a".into(),
            ),
            true,
        ),
        (
            (
                since,
                "source-b".into(),
                Arc::from("native-a"),
                "epoch-z".into(),
            ),
            false,
        ),
        (
            (
                since,
                "source-b".into(),
                Arc::from("native-c"),
                "epoch-a".into(),
            ),
            true,
        ),
        (
            (
                since,
                "source-b".into(),
                Arc::from("native-b"),
                "epoch-a".into(),
            ),
            false,
        ),
        (
            (
                since,
                "source-b".into(),
                Arc::from("native-b"),
                "epoch-c".into(),
            ),
            true,
        ),
        (selected, true),
    ] {
        assert_eq!(
            a.permits(
                since,
                &PressureCut {
                    started: since + Duration::from_secs(1),
                    first_other: Some(other.clone()),
                }
            ),
            expected,
            "full equal-time rank: {other:?}"
        );
    }
}
