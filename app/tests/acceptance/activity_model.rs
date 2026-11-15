//! Independent consumer tests of the public activity model; no node, storage or process claims.
#[cfg(test)]
mod acceptance {
    use acp_remote::domain::{EntityId, activity::*, native::*};
    use std::{any::TypeId, num::NonZeroUsize};
    fn id(n: u8) -> EntityId {
        EntityId::from_bytes([n; 16])
    }
    fn key(source: u8) -> NativeSessionKey {
        NativeSessionKey::from_config(Some(id(source)), "opaque-same-id").unwrap()
    }
    fn epoch(n: u8) -> ResourceEpoch {
        ResourceEpoch {
            runtime: RuntimeEpoch(id(100)),
            connection: HarnessConnectionEpoch(id(n)),
            load: LoadEpoch(id(n)),
        }
    }
    fn ready() -> SessionActivity {
        let mut s = SessionActivity::new(key(1), epoch(1));
        s.observe_foreground(epoch(1), WorkActivity::Idle).unwrap();
        s
    }
    fn recovery(stop: StopEvidence) -> RecoveryFacts {
        RecoveryFacts {
            target: key(1),
            old_epoch: epoch(1),
            stop,
            identity_verified: true,
            unique_owner_verified: true,
            restore_succeeded: true,
            management_reconciled: true,
        }
    }

    #[test]
    fn cl056_config_namespace_registry_and_epoch_types() {
        assert_ne!(TypeId::of::<RequestEpoch>(), TypeId::of::<RuntimeEpoch>());
        assert_ne!(
            TypeId::of::<RuntimeEpoch>(),
            TypeId::of::<HarnessConnectionEpoch>()
        );
        assert!(matches!(
            NativeSessionKey::from_config(None, "opaque"),
            Err(NativeIdentityError::MissingSource)
        ));
        let mut registry = ActivityRegistry::new(NonZeroUsize::new(2).unwrap());
        for alias in 0..3 {
            let s = registry.claim(key(1), epoch(1)).unwrap();
            if alias == 0 {
                s.observe_foreground(epoch(1), WorkActivity::Idle).unwrap();
                s.reserve_input(id(7), id(8)).unwrap();
            } else {
                assert_eq!(s.reserve_input(id(9), id(10)), Err(ActivityError::Busy));
            }
        }
        assert!(matches!(
            registry.claim(key(1), epoch(2)),
            Err(ActivityError::OwnerConflict)
        ));
        let other = registry.claim(key(2), epoch(2)).unwrap();
        other
            .observe_foreground(epoch(2), WorkActivity::Idle)
            .unwrap();
        assert!(other.reserve_input(id(9), id(10)).is_ok());
        assert!(!format!("{:?}", key(1)).contains("opaque-same-id"));
    }

    #[test]
    fn cl066_unprompted_native_work_has_no_run_and_keeps_background_unknown() {
        for background in [WorkActivity::Unknown, WorkActivity::Active] {
            let mut s = ready();
            s.observe_foreground(epoch(1), WorkActivity::Active)
                .unwrap();
            s.observe_background(epoch(1), background).unwrap();
            assert!(s.input().is_none());
            assert!(!s.can_accept_input());
            s.observe_permission(epoch(1), id(20), None).unwrap();
            let scope = s.cancel_scope().unwrap();
            assert_eq!(scope.permissions(), &[id(20)]);
            let done = s
                .finish_work(scope.work().unwrap(), ForegroundOutcome::Completed)
                .unwrap();
            assert!(done.input.is_none());
            assert!(s.can_accept_input());
            assert_eq!(s.background(), background);
            assert!(!s.locally_quiescent());
        }
    }

    #[test]
    fn cl067_run_termination_is_not_permission_invalidation() {
        let mut s = ready();
        let w = s.reserve_input(id(2), id(3)).unwrap();
        for (p, run) in [(id(10), Some(id(3))), (id(11), None)] {
            s.observe_permission(epoch(1), p, run).unwrap();
        }
        s.confirm_insertion(&w).unwrap();
        s.finish_work(&w, ForegroundOutcome::Completed).unwrap();
        s.observe_background(epoch(1), WorkActivity::Idle).unwrap();
        assert_eq!(s.cancel_scope().unwrap().permissions().len(), 2);
        assert!(!s.locally_quiescent());
        assert_eq!(
            s.observe_permission(epoch(1), id(10), None),
            Err(ActivityError::PermissionConflict)
        );
        s.retire_permission(epoch(1), id(10)).unwrap();
        s.retire_permission(epoch(1), id(11)).unwrap();
        assert!(s.locally_quiescent());
    }

    #[test]
    fn cl072_idle_cancel_keeps_admission_and_cannot_retarget_future_work() {
        let mut s = ready();
        s.observe_permission(epoch(1), id(20), None).unwrap();
        let cancel = s.cancel_scope().unwrap();
        assert!(cancel.work().is_none());
        assert!(s.input().is_none());
        assert!(s.can_accept_input());
        s.retire_permission(epoch(1), id(20)).unwrap();
        let next = s.reserve_input(id(2), id(3)).unwrap();
        assert!(!s.cancellation_is_current(&cancel));
        s.finish_work(&next, ForegroundOutcome::Completed).unwrap();
        assert!(!s.cancellation_is_current(&cancel));
        assert_eq!(s.background(), WorkActivity::Unknown);
    }

    #[test]
    fn cl073_all_idle_observations_preserve_pending_and_confirmed_input() {
        for inserted in [false, true] {
            let mut s = ready();
            let w = s.reserve_input(id(2), id(3)).unwrap();
            if inserted {
                s.confirm_insertion(&w).unwrap();
            }
            for _ in 0..3 {
                s.observe_foreground(epoch(1), WorkActivity::Idle).unwrap();
                let _ = s.cancel_scope().unwrap();
                assert!(!s.can_accept_input());
                assert_eq!(s.input().unwrap().awaiting_insertion, !inserted);
            }
            s.reject_input(&w).unwrap();
            assert!(s.can_accept_input());
        }
    }

    #[test]
    fn cl074_old_terminal_and_cancel_never_settle_new_or_other_work() {
        for cancellation_first in [true, false] {
            let mut s = ready();
            let old = s.reserve_input(id(2), id(3)).unwrap();
            let before = cancellation_first.then(|| s.cancel_scope().unwrap());
            let done = s.finish_work(&old, ForegroundOutcome::Completed).unwrap();
            assert_eq!(done.outcome, ForegroundOutcome::Completed);
            let cancel = before.unwrap_or_else(|| s.cancel_scope().unwrap());
            let new = s.reserve_input(id(4), id(5)).unwrap();
            assert_eq!(
                s.finish_work(&old, ForegroundOutcome::Cancelled),
                Err(ActivityError::WrongWork)
            );
            assert!(!s.cancellation_is_current(&cancel));
            assert_eq!(s.current_work(), Some(&new));
            let mut other = SessionActivity::new(key(2), epoch(1));
            other
                .observe_foreground(epoch(1), WorkActivity::Idle)
                .unwrap();
            let own = other.reserve_input(id(6), id(7)).unwrap();
            assert_eq!(
                other.finish_work(&new, ForegroundOutcome::Completed),
                Err(ActivityError::WrongWork)
            );
            assert_eq!(other.current_work(), Some(&own));
        }
    }

    #[test]
    fn cl043_unknown_no_run_and_interrupted_input_do_not_unlock_on_handshake() {
        let mut empty = SessionActivity::new(key(1), epoch(1));
        assert!(!empty.can_accept_input());
        assert!(empty.input().is_none());
        empty.interrupt(epoch(1)).unwrap();
        assert_eq!(
            empty.observe_foreground(epoch(2), WorkActivity::Idle),
            Err(ActivityError::StaleEpoch)
        );
        let mut s = ready();
        let w = s.reserve_input(id(2), id(3)).unwrap();
        s.interrupt(epoch(1)).unwrap();
        assert_eq!(s.confirm_insertion(&w), Err(ActivityError::Disconnected));
        assert!(!s.can_accept_input());
        assert!(s.cancel_scope().is_err());
        assert_eq!(
            s.recover(epoch(2), &recovery(StopEvidence::Unknown)),
            Err(ActivityError::RecoveryUnverified)
        );
        assert_eq!(s.input().unwrap().request, id(2));
    }

    #[test]
    fn cl044_cl045_each_recovery_prerequisite_is_necessary_for_both_stop_routes() {
        for stop in [
            StopEvidence::OwnedResourcesConverged,
            StopEvidence::NativeStoppedAndResumeCannotRedispatch,
        ] {
            for missing in 0..8 {
                let mut s = ready();
                let old = s.reserve_input(id(2), id(3)).unwrap();
                s.observe_permission(epoch(1), id(20), None).unwrap();
                s.interrupt(epoch(1)).unwrap();
                let mut facts = recovery(stop);
                match missing {
                    0 => facts.identity_verified = false,
                    1 => facts.unique_owner_verified = false,
                    2 => facts.restore_succeeded = false,
                    3 => facts.management_reconciled = false,
                    4 => facts.old_epoch = epoch(4),
                    5 => facts.target = key(2),
                    6 => facts.stop = StopEvidence::Unknown,
                    _ => (),
                }
                if missing < 7 {
                    assert_eq!(
                        s.recover(epoch(2), &facts),
                        Err(ActivityError::RecoveryUnverified)
                    );
                    assert_eq!(s.input().unwrap().request, id(2));
                    assert!(!s.can_accept_input());
                } else {
                    let prior = s.recover(epoch(2), &facts).unwrap().unwrap();
                    assert_eq!(prior.request, id(2));
                    assert!(!s.can_accept_input());
                    s.observe_foreground(epoch(2), WorkActivity::Idle).unwrap();
                    assert!(s.can_accept_input());
                    assert!(s.cancel_scope().unwrap().permissions().is_empty());
                    let new = s.reserve_input(id(4), id(5)).unwrap();
                    assert_eq!(
                        s.finish_work(&old, ForegroundOutcome::Completed),
                        Err(ActivityError::StaleEpoch)
                    );
                    assert_eq!(s.current_work(), Some(&new));
                    assert!(!s.locally_quiescent());
                }
            }
        }
    }

    #[test]
    fn cl074_load_epoch_fences_recreated_target_on_same_connection() {
        let mut prior = ready();
        let old = prior.reserve_input(id(2), id(3)).unwrap();
        let fresh_epoch = ResourceEpoch {
            load: LoadEpoch(id(9)),
            ..epoch(1)
        };
        let mut fresh = SessionActivity::new(key(1), fresh_epoch);
        fresh
            .observe_foreground(fresh_epoch, WorkActivity::Idle)
            .unwrap();
        let own = fresh.reserve_input(id(4), id(5)).unwrap();
        assert_eq!(
            fresh.finish_work(&old, ForegroundOutcome::Completed),
            Err(ActivityError::StaleEpoch)
        );
        assert_eq!(fresh.current_work(), Some(&own));
    }
}
