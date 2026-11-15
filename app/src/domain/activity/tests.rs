use super::*;
use crate::domain::native::{HarnessConnectionEpoch, LoadEpoch, NativeIdentityError, RuntimeEpoch};

fn id(value: u8) -> EntityId {
    EntityId::from_bytes([value; 16])
}
fn target(source: u8) -> NativeSessionKey {
    NativeSessionKey::from_config(Some(id(source)), "same-native-session").unwrap()
}
fn epoch(value: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(id(100)),
        connection: HarnessConnectionEpoch(id(value)),
        load: LoadEpoch(id(value)),
    }
}
fn loaded() -> SessionActivity {
    let mut activity = SessionActivity::new(target(1), epoch(1));
    activity
        .observe_foreground(epoch(1), WorkActivity::Idle)
        .unwrap();
    activity
}
fn recovery() -> RecoveryFacts {
    RecoveryFacts {
        target: target(1),
        old_epoch: epoch(1),
        stop: StopEvidence::OwnedResourcesConverged,
        identity_verified: true,
        unique_owner_verified: true,
        restore_succeeded: true,
        management_reconciled: true,
    }
}

#[test]
fn explicit_source_is_required_and_native_identifier_is_not_logged() {
    assert_eq!(
        NativeSessionKey::from_config(None, "native"),
        Err(NativeIdentityError::MissingSource)
    );
    assert_eq!(
        NativeSessionKey::from_config(Some(id(1)), ""),
        Err(NativeIdentityError::InvalidNativeId)
    );
    assert!(NativeSessionKey::from_config(Some(id(1)), &"x".repeat(4097)).is_err());
    assert!(!format!("{:?}", target(1)).contains("same-native-session"));
}

#[test]
fn configuration_aliases_share_one_slot_and_different_sources_remain_separate() {
    let mut registry = ActivityRegistry::new(NonZeroUsize::new(2).unwrap());
    let first_entry = NativeSessionKey::from_config(Some(id(1)), "same-native-session").unwrap();
    let alias_entry = NativeSessionKey::from_config(Some(id(1)), "same-native-session").unwrap();
    let activity = registry.claim(first_entry, epoch(1)).unwrap();
    activity
        .observe_foreground(epoch(1), WorkActivity::Idle)
        .unwrap();
    let work = activity.reserve_input(id(5), id(6)).unwrap();
    assert_eq!(
        registry
            .claim(alias_entry, epoch(1))
            .unwrap()
            .reserve_input(id(7), id(8)),
        Err(ActivityError::Busy)
    );
    assert!(matches!(
        registry.claim(target(1), epoch(2)),
        Err(ActivityError::OwnerConflict)
    ));
    assert!(registry.claim(target(2), epoch(2)).is_ok());
    assert!(matches!(
        registry.claim(target(3), epoch(3)),
        Err(ActivityError::Capacity)
    ));
    let activity = registry.claim(target(1), epoch(1)).unwrap();
    assert_eq!(activity.current_work(), Some(&work));
}

#[test]
fn native_active_without_run_blocks_input_but_can_cancel_and_observe_permissions() {
    let mut activity = loaded();
    activity
        .observe_foreground(epoch(1), WorkActivity::Active)
        .unwrap();
    assert!(activity.input().is_none());
    assert_eq!(
        activity.reserve_input(id(3), id(4)),
        Err(ActivityError::Busy)
    );
    activity.observe_permission(epoch(1), id(9), None).unwrap();
    let cancel = activity.cancel_scope().unwrap();
    assert!(cancel.work().is_some());
    assert_eq!(cancel.permissions(), &[id(9)]);
    activity
        .observe_background(epoch(1), WorkActivity::Active)
        .unwrap();
    activity
        .finish_work(cancel.work().unwrap(), ForegroundOutcome::Completed)
        .unwrap();
    assert!(activity.can_accept_input());
    assert_eq!(activity.background(), WorkActivity::Active);
    assert!(!activity.locally_quiescent());
    assert_eq!(activity.cancel_scope().unwrap().permissions(), &[id(9)]);
}

#[test]
fn idle_cancel_does_not_manufacture_run_completion_or_foreground_wait() {
    let mut activity = loaded();
    activity.observe_permission(epoch(1), id(9), None).unwrap();
    let cancel = activity.cancel_scope().unwrap();
    assert!(cancel.work().is_none());
    assert!(activity.input().is_none());
    assert!(activity.can_accept_input());
    activity.retire_permission(epoch(1), id(9)).unwrap();
    let next = activity.reserve_input(id(3), id(4)).unwrap();
    assert!(!activity.cancellation_is_current(&cancel));
    activity
        .finish_work(&next, ForegroundOutcome::Completed)
        .unwrap();
    // Returning to idle must not make an earlier idle cancellation current again.
    assert!(!activity.cancellation_is_current(&cancel));
    assert_eq!(activity.background(), WorkActivity::Unknown);
}

#[test]
fn idle_snapshot_and_cancel_write_do_not_settle_unconfirmed_input() {
    let mut activity = loaded();
    let work = activity.reserve_input(id(3), id(4)).unwrap();
    let cancel = activity.cancel_scope().unwrap();
    activity
        .observe_foreground(epoch(1), WorkActivity::Idle)
        .unwrap();
    assert!(activity.cancellation_is_current(&cancel));
    assert!(activity.input().unwrap().awaiting_insertion);
    assert!(!activity.can_accept_input());
    activity.confirm_insertion(&work).unwrap();
    assert!(!activity.input().unwrap().awaiting_insertion);
    assert!(!activity.can_accept_input());
    let ended = activity
        .finish_work(&work, ForegroundOutcome::Completed)
        .unwrap();
    assert_eq!(ended.input.unwrap().request, id(3));
    assert_eq!(ended.outcome, ForegroundOutcome::Completed);
    assert!(activity.can_accept_input());
}

#[test]
fn reliable_rejection_releases_only_its_input_and_preserves_native_activity() {
    let mut activity = loaded();
    let work = activity.reserve_input(id(3), id(4)).unwrap();
    activity
        .observe_foreground(epoch(1), WorkActivity::Active)
        .unwrap();
    assert_eq!(activity.reject_input(&work).unwrap().run, id(4));
    assert!(activity.input().is_none());
    assert!(!activity.can_accept_input());
    activity
        .finish_work(&work, ForegroundOutcome::Completed)
        .unwrap();
    let pending = activity.reserve_input(id(5), id(6)).unwrap();
    assert_eq!(activity.reject_input(&work), Err(ActivityError::WrongWork));
    activity.reject_input(&pending).unwrap();
    assert!(activity.can_accept_input());
}

#[test]
fn natural_completion_before_or_after_cancel_never_requires_second_terminal_event() {
    for cancel_before_completion in [true, false] {
        let mut activity = loaded();
        let work = activity.reserve_input(id(3), id(4)).unwrap();
        let cancel = cancel_before_completion.then(|| activity.cancel_scope().unwrap());
        let completed = activity
            .finish_work(&work, ForegroundOutcome::Completed)
            .unwrap();
        assert_eq!(completed.outcome, ForegroundOutcome::Completed);
        if !cancel_before_completion {
            assert!(activity.cancel_scope().unwrap().work().is_none());
        }
        let next = activity.reserve_input(id(5), id(6)).unwrap();
        if let Some(cancel) = cancel {
            assert!(!activity.cancellation_is_current(&cancel));
        }
        assert_eq!(
            activity.finish_work(&work, ForegroundOutcome::Cancelled),
            Err(ActivityError::WrongWork)
        );
        assert_eq!(activity.current_work(), Some(&next));
        assert_eq!(activity.input().unwrap().run, id(6));
    }
}

#[test]
fn work_results_cannot_cross_targets_even_with_same_epoch_and_local_sequence() {
    let mut first = loaded();
    let mut second = SessionActivity::new(target(2), epoch(1));
    second
        .observe_foreground(epoch(1), WorkActivity::Idle)
        .unwrap();
    let first_work = first.reserve_input(id(3), id(4)).unwrap();
    let second_work = second.reserve_input(id(5), id(6)).unwrap();
    assert_eq!(
        second.finish_work(&first_work, ForegroundOutcome::Completed),
        Err(ActivityError::WrongWork)
    );
    assert_eq!(second.current_work(), Some(&second_work));
}

#[test]
fn frontend_idle_and_platform_completion_do_not_retire_background_permissions() {
    let mut activity = loaded();
    let work = activity.reserve_input(id(3), id(4)).unwrap();
    activity
        .observe_permission(epoch(1), id(9), Some(id(4)))
        .unwrap();
    activity.observe_permission(epoch(1), id(10), None).unwrap();
    activity
        .finish_work(&work, ForegroundOutcome::Completed)
        .unwrap();
    activity
        .observe_foreground(epoch(1), WorkActivity::Idle)
        .unwrap();
    assert_eq!(activity.cancel_scope().unwrap().permissions().len(), 2);
    activity
        .observe_background(epoch(1), WorkActivity::Idle)
        .unwrap();
    assert!(!activity.locally_quiescent());
    assert!(activity.retire_permission(epoch(1), id(9)).unwrap());
    assert!(!activity.retire_permission(epoch(1), id(9)).unwrap());
    activity.retire_permission(epoch(1), id(10)).unwrap();
    assert!(activity.locally_quiescent());
}

#[test]
fn pending_permission_references_are_bounded_and_conflicting_observations_do_not_replace_them() {
    let mut activity = loaded();
    for value in 0..64 {
        activity
            .observe_permission(epoch(1), id(value), None)
            .unwrap();
    }
    activity.observe_permission(epoch(1), id(1), None).unwrap();
    assert_eq!(
        activity.observe_permission(epoch(1), id(1), Some(id(80))),
        Err(ActivityError::PermissionConflict)
    );
    assert_eq!(
        activity.observe_permission(epoch(1), id(80), None),
        Err(ActivityError::Capacity)
    );
    activity.retire_permission(epoch(1), id(1)).unwrap();
    activity.observe_permission(epoch(1), id(80), None).unwrap();
}

#[test]
fn interrupt_handshake_timeout_and_partial_recovery_do_not_unlock_old_input() {
    let mut activity = loaded();
    let work = activity.reserve_input(id(3), id(4)).unwrap();
    activity.observe_permission(epoch(1), id(9), None).unwrap();
    let unknown = activity.interrupt(epoch(1)).unwrap().unwrap();
    assert_eq!(unknown.request, id(3));
    assert_eq!(
        activity.observe_foreground(epoch(1), WorkActivity::Idle),
        Err(ActivityError::Disconnected)
    );
    assert_eq!(
        activity.observe_foreground(epoch(2), WorkActivity::Idle),
        Err(ActivityError::StaleEpoch)
    );
    assert!(activity.cancel_scope().is_err());
    assert_eq!(
        activity.confirm_insertion(&work),
        Err(ActivityError::Disconnected)
    );
    for missing in 0..7 {
        let mut facts = recovery();
        match missing {
            0 => facts.stop = StopEvidence::Unknown,
            1 => facts.identity_verified = false,
            2 => facts.unique_owner_verified = false,
            3 => facts.restore_succeeded = false,
            4 => facts.management_reconciled = false,
            5 => facts.target = target(2),
            _ => facts.old_epoch = epoch(2),
        }
        assert_eq!(
            activity.recover(epoch(2), &facts),
            Err(ActivityError::RecoveryUnverified)
        );
        assert!(!activity.can_accept_input());
        assert_eq!(activity.input(), Some(&unknown));
    }
    let prior = activity.recover(epoch(2), &recovery()).unwrap().unwrap();
    assert_eq!(prior, unknown);
    assert!(!activity.can_accept_input()); // Handshake/recovery is not a reliable idle observation.
    activity
        .observe_foreground(epoch(2), WorkActivity::Idle)
        .unwrap();
    assert!(activity.cancel_scope().unwrap().permissions().is_empty());
    let next = activity.reserve_input(id(5), id(6)).unwrap();
    assert_eq!(
        activity.finish_work(&work, ForegroundOutcome::Completed),
        Err(ActivityError::StaleEpoch)
    );
    assert_eq!(activity.current_work(), Some(&next));
}

#[test]
fn verified_native_stop_alternative_still_requires_all_recovery_facts() {
    let mut activity = loaded();
    activity
        .observe_foreground(epoch(1), WorkActivity::Active)
        .unwrap();
    activity.interrupt(epoch(1)).unwrap();
    let mut facts = recovery();
    facts.stop = StopEvidence::NativeStoppedAndResumeCannotRedispatch;
    facts.restore_succeeded = false;
    assert_eq!(
        activity.recover(epoch(2), &facts),
        Err(ActivityError::RecoveryUnverified)
    );
    facts.restore_succeeded = true;
    assert_eq!(activity.recover(epoch(2), &facts), Ok(None));
    activity
        .observe_foreground(epoch(2), WorkActivity::Idle)
        .unwrap();
    assert!(activity.can_accept_input());
    assert_eq!(activity.background(), WorkActivity::Unknown);
    assert!(!activity.locally_quiescent());
}

#[test]
fn unknown_foreground_and_exhausted_fences_do_not_accept_work() {
    let mut activity = SessionActivity::new(target(1), epoch(1));
    assert_eq!(
        activity.reserve_input(id(3), id(4)),
        Err(ActivityError::Busy)
    );
    activity
        .observe_foreground(epoch(1), WorkActivity::Idle)
        .unwrap();
    activity.serial = u64::MAX;
    assert_eq!(
        activity.reserve_input(id(3), id(4)),
        Err(ActivityError::Capacity)
    );
    assert!(activity.input().is_none());
    assert!(activity.current_work().is_none());
}

#[test]
fn recreated_target_on_the_same_connection_cannot_accept_old_load_work_tokens() {
    let mut previous = loaded();
    let old_work = previous.reserve_input(id(3), id(4)).unwrap();
    let new_epoch = ResourceEpoch {
        load: LoadEpoch(id(99)),
        ..epoch(1)
    };
    let mut reloaded = SessionActivity::new(target(1), new_epoch);
    reloaded
        .observe_foreground(new_epoch, WorkActivity::Idle)
        .unwrap();
    let new_work = reloaded.reserve_input(id(5), id(6)).unwrap();
    assert_eq!(
        reloaded.finish_work(&old_work, ForegroundOutcome::Completed),
        Err(ActivityError::StaleEpoch)
    );
    assert_eq!(reloaded.current_work(), Some(&new_work));
}
