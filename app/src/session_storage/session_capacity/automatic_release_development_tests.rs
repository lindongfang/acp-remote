//! Real SQLite development fixtures; no production close or original transport is simulated as proof.
use super::*;
use crate::application::native_dispatch::AutomaticReleasePhase;

type CapacitySqlRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    i64,
    i64,
    i64,
);

async fn root(f: &Fixture, n: u16, native: &str) -> CapacityRecord {
    let reservation = f.operation(n).await;
    let repository = f.repository();
    repository
        .reserve_capacity(&reservation, &deadline())
        .await
        .expect("original reservation");
    f.created(n, native, true).await;
    repository
        .bind_created_capacity(
            id(n),
            &NativeSessionKey::from_config(Some(id(2)), native).expect("native"),
            &f.source,
            &deadline(),
        )
        .await
        .expect("verified root")
}
async fn begin(f: &Fixture, record: &CapacityRecord, n: u16) -> AutomaticReleaseRecord {
    f.repository()
        .begin_automatic_release(
            record,
            id(n),
            AutomaticReleaseReason::IdleTimeout,
            &deadline(),
        )
        .await
        .expect("system intent")
}
async fn occupied(f: &Fixture) -> CapacitySnapshot {
    f.repository()
        .inspect_capacity(&deadline())
        .await
        .expect("capacity")
}

#[tokio::test]
async fn automatic_release_pending_requested_physical_and_finish_are_separate() {
    let f = Fixture::new().await;
    let original = root(&f, 100, "created-only").await;
    let pending = begin(&f, &original, 200).await;
    assert_eq!(pending.phase, AutomaticReleasePhase::Pending);
    assert_eq!(pending.outcome, None);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT (SELECT COUNT(*) FROM product_targets)+(SELECT COUNT(*) FROM node_native_release_operations)")
            .fetch_one(f.storage.pool_for_test()).await.expect("no invented user target"), 0
    );
    let repo = f.repository();
    let requested = repo
        .record_automatic_release_outcome(
            &original,
            id(200),
            AutomaticReleaseOutcome::Requested,
            &deadline(),
        )
        .await
        .expect("development caller asserts original Written");
    assert_eq!(requested.outcome, Some(AutomaticReleaseOutcome::Requested));
    assert!(!requested.original.physically_closed);
    assert_eq!(occupied(&f).await.occupied, 1);
    assert_eq!(
        repo.finish_automatic_release(&original, id(200), &deadline())
            .await,
        Err(DispatchError::ResourceUnconfirmed)
    );
    repo.confirm_capacity_closed(&original, &deadline())
        .await
        .expect("original physical close");
    assert_eq!(occupied(&f).await.closed_pending, 1);
    let finished = repo
        .finish_automatic_release(&original, id(200), &deadline())
        .await
        .expect("atomic finish");
    assert_eq!(finished.phase, AutomaticReleasePhase::Finished);
    assert!(finished.original.physically_closed);
    assert_eq!(finished.outcome, Some(AutomaticReleaseOutcome::Requested));
    assert_eq!(occupied(&f).await.occupied, 0);
    assert_eq!(begin(&f, &original, 200).await, finished);
    assert_eq!(
        repo.finish_automatic_release(&original, id(200), &deadline())
            .await
            .expect("retry"),
        finished
    );
    f.close().await;
}

#[tokio::test]
async fn automatic_release_outcome_transition_matrix_preserves_failed_unknown_and_withdrawn() {
    use AutomaticReleaseOutcome::{Failed, Requested, Unknown, Withdrawn};
    for (index, observations, allowed) in [
        (0, vec![Requested, Requested, Unknown, Unknown], true),
        (1, vec![Requested, Failed, Failed], true),
        (2, vec![Withdrawn, Withdrawn], true),
        (3, vec![Unknown, Requested], false),
        (4, vec![Unknown, Withdrawn], false),
        (5, vec![Requested, Withdrawn], false),
        (6, vec![Failed, Requested], false),
        (7, vec![Failed, Unknown], false),
        (8, vec![Withdrawn, Requested], false),
    ] {
        let f = Fixture::new().await;
        let original = root(&f, 100, "matrix").await;
        begin(&f, &original, 200).await;
        let repo = f.repository();
        for (step, observation) in observations.iter().enumerate() {
            let result = repo
                .record_automatic_release_outcome(&original, id(200), *observation, &deadline())
                .await;
            if step + 1 == observations.len() && !allowed {
                assert_eq!(result, Err(DispatchError::Conflict), "case {index}");
            } else {
                assert!(result.is_ok(), "case {index} step {step}: {result:?}");
            }
        }
        assert_eq!(occupied(&f).await.occupied, 1);
        if observations[0] == Withdrawn {
            assert_eq!(
                repo.finish_automatic_release(&original, id(200), &deadline())
                    .await,
                Err(DispatchError::Conflict)
            );
        } else {
            repo.confirm_capacity_closed(&original, &deadline())
                .await
                .expect("original close");
            assert_eq!(
                repo.record_automatic_release_outcome(&original, id(201), Withdrawn, &deadline())
                    .await,
                Err(DispatchError::Conflict)
            );
            repo.finish_automatic_release(&original, id(200), &deadline())
                .await
                .expect("original physical fact can reconcile failed/unknown");
            assert_eq!(occupied(&f).await.occupied, 0);
        }
        f.close().await;
    }
}

#[tokio::test]
async fn automatic_release_rejects_changed_identity_and_reason() {
    let f = Fixture::new().await;
    let record = root(&f, 100, "anchor").await;
    begin(&f, &record, 200).await;
    let repo = f.repository();
    assert_eq!(
        repo.begin_automatic_release(
            &record,
            id(200),
            AutomaticReleaseReason::CapacityPressure,
            &deadline()
        )
        .await,
        Err(DispatchError::Conflict)
    );
    for changed in [
        CapacityRecord {
            id: id(101),
            ..record.clone()
        },
        CapacityRecord {
            source: id(12),
            ..record.clone()
        },
        CapacityRecord {
            instance: id(17),
            ..record.clone()
        },
        CapacityRecord {
            runtime: RuntimeEpoch(id(18)),
            ..record.clone()
        },
        CapacityRecord {
            native: None,
            ..record.clone()
        },
    ] {
        assert_eq!(
            repo.begin_automatic_release(
                &changed,
                id(200),
                AutomaticReleaseReason::IdleTimeout,
                &deadline()
            )
            .await,
            Err(DispatchError::Conflict)
        );
        assert_eq!(
            repo.record_automatic_release_outcome(
                &changed,
                id(200),
                AutomaticReleaseOutcome::Unknown,
                &deadline()
            )
            .await,
            Err(DispatchError::Conflict)
        );
        assert_eq!(
            repo.finish_automatic_release(&changed, id(200), &deadline())
                .await,
            Err(DispatchError::Conflict)
        );
    }
    f.close().await;
}

#[tokio::test]
async fn automatic_release_rejects_unverified_or_closed_anchor() {
    let f = Fixture::new().await;
    let record = root(&f, 100, "anchor").await;
    begin(&f, &record, 200).await;
    let repo = f.repository();
    sqlx::query("UPDATE node_session_capacity SET root_verified=0 WHERE reservation_id=?")
        .bind(record.id.encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("unverified fixture");
    assert_eq!(
        repo.begin_automatic_release(
            &record,
            id(201),
            AutomaticReleaseReason::IdleTimeout,
            &deadline()
        )
        .await,
        Err(DispatchError::SessionTreeUnverified)
    );
    sqlx::query("UPDATE node_session_capacity SET root_verified=1 WHERE reservation_id=?")
        .bind(record.id.encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("restore root fixture");
    repo.confirm_capacity_closed(&record, &deadline())
        .await
        .expect("physical");
    assert_eq!(
        repo.begin_automatic_release(
            &record,
            id(201),
            AutomaticReleaseReason::IdleTimeout,
            &deadline()
        )
        .await,
        Err(DispatchError::Conflict)
    );
    assert_eq!(
        repo.record_automatic_release_outcome(
            &record,
            id(200),
            AutomaticReleaseOutcome::Withdrawn,
            &deadline()
        )
        .await,
        Err(DispatchError::Conflict)
    );
    f.close().await;
}

#[tokio::test]
async fn automatic_release_captures_anchor_and_all_original_same_owner_members() {
    let f = Fixture::new().await;
    let record = root(&f, 100, "set").await;
    sqlx::query("INSERT INTO node_session_capacity SELECT ?,source_id,native_id,instance_id,runtime_epoch,NULL,0,0,0 FROM node_session_capacity WHERE reservation_id=?")
        .bind(id(101).encode()).bind(record.id.encode()).execute(f.storage.pool_for_test()).await.expect("second original responsibility fixture");
    begin(&f, &record, 200).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM node_session_automatic_release_members WHERE operation_id=?"
        )
        .bind(id(200).encode())
        .fetch_one(f.storage.pool_for_test())
        .await
        .expect("fixed members"),
        2
    );
    let repo = f.repository();
    repo.confirm_capacity_closed(&record, &deadline())
        .await
        .expect("anchor closed");
    assert_eq!(
        repo.finish_automatic_release(&record, id(200), &deadline())
            .await,
        Err(DispatchError::ResourceUnconfirmed)
    );
    assert_eq!(occupied(&f).await.occupied, 1);
    let member = CapacityRecord {
        id: id(101),
        ..record.clone()
    };
    repo.confirm_capacity_closed(&member, &deadline())
        .await
        .expect("last original member closed");
    repo.finish_automatic_release(&record, id(200), &deadline())
        .await
        .expect("all original members");
    assert_eq!(occupied(&f).await.occupied, 0);
    f.close().await;
}

#[tokio::test]
async fn automatic_release_outer_finish_failure_rolls_back_both_updates() {
    for trigger in [
        "CREATE TRIGGER auto_fail BEFORE UPDATE OF phase ON node_session_automatic_releases WHEN NEW.phase='finished' BEGIN SELECT RAISE(ABORT,'owned fixture'); END",
        "CREATE TRIGGER auto_fail BEFORE UPDATE OF released ON node_session_capacity WHEN NEW.released=1 BEGIN SELECT RAISE(ABORT,'owned fixture'); END",
    ] {
        let f = Fixture::new().await;
        let record = root(&f, 100, "failure").await;
        begin(&f, &record, 200).await;
        let repo = f.repository();
        repo.confirm_capacity_closed(&record, &deadline())
            .await
            .expect("physical");
        sqlx::query(trigger)
            .execute(f.storage.pool_for_test())
            .await
            .expect("owned fault");
        assert!(
            repo.finish_automatic_release(&record, id(200), &deadline())
                .await
                .is_err()
        );
        let pending = begin(&f, &record, 200).await;
        assert_eq!(pending.phase, AutomaticReleasePhase::Pending);
        assert_eq!(occupied(&f).await.closed_pending, 1);
        sqlx::query("DROP TRIGGER auto_fail")
            .execute(f.storage.pool_for_test())
            .await
            .expect("remove owned fault");
        repo.finish_automatic_release(&record, id(200), &deadline())
            .await
            .expect("original retry");
        assert_eq!(occupied(&f).await.occupied, 0);
        f.close().await;
    }
}

#[tokio::test]
async fn automatic_release_finished_a_never_releases_resumed_b_closed_pending() {
    let f = Fixture::new().await;
    let a = root(&f, 100, "generation").await;
    let repo = f.repository();
    begin(&f, &a, 200).await;
    repo.confirm_capacity_closed(&a, &deadline())
        .await
        .expect("A physical");
    repo.finish_automatic_release(&a, id(200), &deadline())
        .await
        .expect("A finished");
    let b = repo
        .reserve_capacity(
            &CapacityReservation {
                id: id(101),
                source: f.source.clone(),
                native: a.native.clone(),
                create_operation: None,
            },
            &deadline(),
        )
        .await
        .expect("explicit resume B");
    begin(&f, &b, 201).await;
    repo.confirm_capacity_closed(&b, &deadline())
        .await
        .expect("B physical");
    sqlx::query("CREATE TRIGGER auto_fail BEFORE UPDATE OF phase ON node_session_automatic_releases WHEN NEW.phase='finished' BEGIN SELECT RAISE(ABORT,'owned fault'); END")
        .execute(f.storage.pool_for_test()).await.expect("B outer finish failure");
    assert!(
        repo.finish_automatic_release(&b, id(201), &deadline())
            .await
            .is_err()
    );
    assert_eq!(
        begin(&f, &a, 200).await.phase,
        AutomaticReleasePhase::Finished
    );
    assert_eq!(
        repo.record_automatic_release_outcome(
            &a,
            id(200),
            AutomaticReleaseOutcome::Requested,
            &deadline()
        )
        .await,
        Err(DispatchError::Conflict)
    );
    assert_eq!(
        repo.finish_automatic_release(&a, id(200), &deadline())
            .await
            .expect("A immutable retry")
            .phase,
        AutomaticReleasePhase::Finished
    );
    assert_eq!(
        occupied(&f).await.closed_pending,
        1,
        "A original proof must not return B"
    );
    sqlx::query("DROP TRIGGER auto_fail")
        .execute(f.storage.pool_for_test())
        .await
        .expect("remove fault");
    repo.finish_automatic_release(&b, id(201), &deadline())
        .await
        .expect("B original operation");
    assert_eq!(occupied(&f).await.occupied, 0);
    f.close().await;
}

#[tokio::test]
async fn automatic_release_unfinished_create_and_original_resource_block_finish() {
    let f = Fixture::new().await;
    let record = root(&f, 100, "management").await;
    let repo = f.repository();
    sqlx::query("UPDATE node_source_session_operations SET operation_state='owned',outcome='pending',terminal_at_ms=NULL WHERE operation_id=?")
        .bind(record.id.encode()).execute(f.storage.pool_for_test()).await.expect("unresolved Create fixture");
    assert_eq!(
        repo.begin_automatic_release(
            &record,
            id(200),
            AutomaticReleaseReason::IdleTimeout,
            &deadline()
        )
        .await,
        Err(DispatchError::ResourceUnconfirmed)
    );
    f.created(100, "management", true).await;
    begin(&f, &record, 200).await;
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
        .bind(id(20).encode())
        .bind(id(2).encode())
        .bind("management")
        .execute(f.storage.pool_for_test())
        .await
        .expect("execution target fixture");
    sqlx::query("INSERT INTO node_native_resource_bindings(load_epoch,target_id,owner_id,instance_id,runtime_epoch,connection_epoch,config_identity,config_revision,lifecycle,reserved_at_ms,updated_at_ms) VALUES(?,?,?,?,?,?,?,X'0000000000000001','unknown',0,0)")
        .bind(id(21).encode()).bind(id(20).encode()).bind(id(22).encode()).bind(id(7).encode()).bind(id(8).encode()).bind(id(9).encode()).bind(id(6).encode())
        .execute(f.storage.pool_for_test()).await.expect("original loaded resource");
    repo.confirm_capacity_closed(&record, &deadline())
        .await
        .expect("physical only");
    assert_eq!(
        repo.finish_automatic_release(&record, id(200), &deadline())
            .await,
        Err(DispatchError::ResourceUnconfirmed)
    );
    sqlx::query("UPDATE node_native_resource_bindings SET lifecycle='released',release_basis='owned_converged',release_stage='unknown',release_evidence_fingerprint=zeroblob(32) WHERE load_epoch=?")
        .bind(id(21).encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("resource management fixture converges");
    sqlx::query("UPDATE node_source_session_operations SET operation_state='owned',outcome='pending',terminal_at_ms=NULL WHERE operation_id=?")
        .bind(record.id.encode()).execute(f.storage.pool_for_test()).await.expect("necessary state unavailable fixture");
    assert_eq!(
        repo.finish_automatic_release(&record, id(200), &deadline())
            .await,
        Err(DispatchError::ResourceUnconfirmed)
    );
    assert_eq!(occupied(&f).await.closed_pending, 1);
    f.created(100, "management", true).await;
    repo.finish_automatic_release(&record, id(200), &deadline())
        .await
        .expect("both management responsibilities converge");
    f.close().await;
}

#[tokio::test]
async fn automatic_release_unknown_reopens_without_changing_owner_or_observation() {
    let f = Fixture::new().await;
    let record = root(&f, 100, "reopen").await;
    begin(&f, &record, 200).await;
    f.repository()
        .record_automatic_release_outcome(
            &record,
            id(200),
            AutomaticReleaseOutcome::Unknown,
            &deadline(),
        )
        .await
        .expect("unknown original ticket");
    f.storage.close().await;
    let reopened = Storage::open(&f.path)
        .await
        .expect("reopen pending operation");
    let repo = reopened.request_registry().native_dispatch_repository();
    let pending = repo
        .begin_automatic_release(
            &record,
            id(200),
            AutomaticReleaseReason::IdleTimeout,
            &deadline(),
        )
        .await
        .expect("same original operation");
    assert_eq!(pending.outcome, Some(AutomaticReleaseOutcome::Unknown));
    assert_eq!(pending.phase, AutomaticReleasePhase::Pending);
    assert_eq!(
        repo.record_automatic_release_outcome(
            &record,
            id(200),
            AutomaticReleaseOutcome::Withdrawn,
            &deadline()
        )
        .await,
        Err(DispatchError::Conflict)
    );
    repo.confirm_capacity_closed(&record, &deadline())
        .await
        .expect("original physical close fixture");
    reopened.close().await;
    let reopened = Storage::open(&f.path)
        .await
        .expect("closed pending reopens");
    let repo = reopened.request_registry().native_dispatch_repository();
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("ledger")
            .closed_pending,
        1
    );
    repo.finish_automatic_release(&record, id(200), &deadline())
        .await
        .expect("management closure");
    reopened.close().await;
    let reopened = Storage::open(&f.path).await.expect("finished reopens");
    let repo = reopened.request_registry().native_dispatch_repository();
    assert_eq!(
        repo.begin_automatic_release(
            &record,
            id(200),
            AutomaticReleaseReason::IdleTimeout,
            &deadline()
        )
        .await
        .expect("finished history")
        .phase,
        AutomaticReleasePhase::Finished
    );
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("returned")
            .occupied,
        0
    );
    reopened.close().await;
    f.close().await;
}

#[tokio::test]
async fn automatic_release_real_deferred_commit_failure_retains_pending_and_capacity() {
    let f = Fixture::new().await;
    let record = root(&f, 100, "commit-failure").await;
    begin(&f, &record, 200).await;
    let repo = f.repository();
    repo.confirm_capacity_closed(&record, &deadline())
        .await
        .expect("physical");
    sqlx::raw_sql("CREATE TABLE auto_test_parent(id INTEGER PRIMARY KEY); CREATE TABLE auto_test_child(parent INTEGER REFERENCES auto_test_parent(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER auto_test_commit AFTER UPDATE OF phase ON node_session_automatic_releases WHEN NEW.phase='finished' BEGIN INSERT INTO auto_test_child VALUES(1); END;")
        .execute(f.storage.pool_for_test()).await.expect("owned deferred FK failure");
    assert_eq!(
        repo.finish_automatic_release(&record, id(200), &deadline())
            .await,
        Err(DispatchError::CommitUnconfirmed)
    );
    assert_eq!(
        begin(&f, &record, 200).await.phase,
        AutomaticReleasePhase::Pending
    );
    assert_eq!(occupied(&f).await.closed_pending, 1);
    sqlx::raw_sql(
        "DROP TRIGGER auto_test_commit; DROP TABLE auto_test_child; DROP TABLE auto_test_parent;",
    )
    .execute(f.storage.pool_for_test())
    .await
    .expect("remove own failure fixture");
    repo.finish_automatic_release(&record, id(200), &deadline())
        .await
        .expect("retry commits original");
    assert_eq!(occupied(&f).await.occupied, 0);
    f.close().await;
}

#[tokio::test]
async fn automatic_release_committed_report_loss_returns_unknown_and_retries_original() {
    let f = Fixture::new().await;
    let record = root(&f, 100, "report-loss").await;
    begin(&f, &record, 200).await;
    let repo = f.repository();
    repo.confirm_capacity_closed(&record, &deadline())
        .await
        .expect("physical");
    let barrier=repo.registry.native_metadata_hooks_for_test().pause_next(
        crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::AutomaticReleaseFinishedAfterCommit,
    ).expect("owned report boundary");
    let d = deadline();
    let lose_report = async {
        barrier.wait_until_entered().await;
        assert_eq!(
            occupied(&f).await.occupied,
            0,
            "actual necessary commit already happened"
        );
        d.cancel();
        barrier.release();
    };
    let (result, ()) = tokio::join!(
        repo.finish_automatic_release(&record, id(200), &d),
        lose_report
    );
    assert_eq!(result, Err(DispatchError::CommitUnconfirmed));
    assert_eq!(
        repo.finish_automatic_release(&record, id(200), &deadline())
            .await
            .expect("same operation coordinates actual commit")
            .phase,
        AutomaticReleasePhase::Finished
    );
    f.close().await;
}

#[tokio::test]
async fn automatic_release_sql_close_report_loss_does_not_claim_success() {
    let f = Fixture::new().await;
    let record = root(&f, 100, "sql-close").await;
    begin(&f, &record, 200).await;
    let repo = f.repository();
    repo.confirm_capacity_closed(&record, &deadline())
        .await
        .expect("physical");
    repo.registry
        .connection_closer_for_test()
        .fail_next_result_after_ack()
        .expect("lose physical SQL close report");
    assert_eq!(
        repo.finish_automatic_release(&record, id(200), &deadline())
            .await,
        Err(DispatchError::SqlCloseUnconfirmed {
            commit_confirmed: true
        })
    );
    assert_eq!(
        occupied(&f).await.occupied,
        0,
        "actual transaction committed, caller received uncertainty"
    );
    assert_eq!(
        repo.finish_automatic_release(&record, id(200), &deadline())
            .await
            .expect("same operation retry")
            .phase,
        AutomaticReleasePhase::Finished
    );
    f.close().await;
}

#[tokio::test]
async fn automatic_release_explicit_winner_only_coordinates_original_set_after_resume() {
    let f = Fixture::new().await;
    let a = root(&f, 100, "explicit").await;
    begin(&f, &a, 200).await;
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
        .bind(id(20).encode())
        .bind(id(2).encode())
        .bind("explicit")
        .execute(f.storage.pool_for_test())
        .await
        .expect("real target fixture");
    let releases = f.storage.native_release_operations();
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 1,
    };
    releases
        .register(actor, id(20), id(21), id(22), 1, &deadline())
        .await
        .expect("real explicit registration");
    releases
        .begin(actor, id(22), 2, &deadline())
        .await
        .expect("real explicit fixed collection");
    let repo = f.repository();
    repo.confirm_capacity_closed(&a, &deadline())
        .await
        .expect("A physical");
    releases
        .finish(actor, id(22), None, 3, &deadline())
        .await
        .expect("explicit winner");
    let b = repo
        .reserve_capacity(
            &CapacityReservation {
                id: id(101),
                source: f.source.clone(),
                native: a.native.clone(),
                create_operation: None,
            },
            &deadline(),
        )
        .await
        .expect("resumed generation B");
    repo.confirm_capacity_closed(&b, &deadline())
        .await
        .expect("B pending, never A");
    repo.finish_automatic_release(&a, id(200), &deadline())
        .await
        .expect("system original-only coordination");
    assert_eq!(occupied(&f).await.closed_pending, 1);
    begin(&f, &a, 200).await;
    assert_eq!(occupied(&f).await.closed_pending, 1);
    f.close().await;
}

#[tokio::test]
async fn automatic_release_capture_rejects_another_owner_of_same_native() {
    let f = Fixture::new().await;
    let record = root(&f, 100, "owner-conflict").await;
    sqlx::query("INSERT INTO node_native_source_instances(instance_id,source_id,runtime_epoch,connection_epoch,config_identity,config_revision,owner_kind,lifecycle,created_at_ms,updated_at_ms) SELECT ?,source_id,?,?,config_identity,config_revision,owner_kind,lifecycle,created_at_ms,updated_at_ms FROM node_native_source_instances WHERE instance_id=?")
        .bind(id(17).encode()).bind(id(18).encode()).bind(id(19).encode()).bind(id(7).encode())
        .execute(f.storage.pool_for_test()).await.expect("different original owner fixture");
    sqlx::query("INSERT INTO node_session_capacity SELECT ?,source_id,native_id,?,?,NULL,0,0,0 FROM node_session_capacity WHERE reservation_id=?")
        .bind(id(101).encode()).bind(id(17).encode()).bind(id(18).encode()).bind(record.id.encode())
        .execute(f.storage.pool_for_test()).await.expect("same native, conflicting owner responsibility");
    assert_eq!(
        f.repository()
            .begin_automatic_release(
                &record,
                id(200),
                AutomaticReleaseReason::IdleTimeout,
                &deadline()
            )
            .await,
        Err(DispatchError::Conflict)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM node_session_automatic_releases")
            .fetch_one(f.storage.pool_for_test())
            .await
            .expect("no partial intent"),
        0
    );
    assert_eq!(
        occupied(&f).await.occupied,
        1,
        "conflicting original rows still share the physical slot"
    );
    f.close().await;
}

#[tokio::test]
async fn automatic_single_native_capture_excludes_other_native_without_claiming_tree() {
    let f = Fixture::new().await;
    let original = root(&f, 100, "single-root").await;
    root(&f, 101, "different-native").await;
    begin(&f, &original, 200).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM node_session_automatic_release_members")
            .fetch_one(f.storage.pool_for_test())
            .await
            .expect("single-native subset"),
        1
    );
    let repo = f.repository();
    repo.confirm_capacity_closed(&original, &deadline())
        .await
        .expect("original target physical closure");
    repo.finish_automatic_release(&original, id(200), &deadline())
        .await
        .expect("only captured single target");
    assert_eq!(
        occupied(&f).await.occupied,
        1,
        "unrelated native never adopted into this operation"
    );
    f.close().await;
}

#[tokio::test]
async fn automatic_schema13_rejects_missing_anchor_cross_native_and_false_finished_history() {
    let f = Fixture::new().await;
    let a = root(&f, 100, "a").await;
    let b = root(&f, 101, "b").await;
    begin(&f, &a, 200).await;
    sqlx::query("DELETE FROM node_session_automatic_release_members WHERE operation_id=?")
        .bind(id(200).encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("owned missing-anchor corruption");
    assert!(schema::initialize(f.storage.pool_for_test()).await.is_err());
    sqlx::query("INSERT INTO node_session_automatic_release_members VALUES(?,?)")
        .bind(id(200).encode())
        .bind(a.id.encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("restore original anchor");
    sqlx::query("INSERT INTO node_session_automatic_release_members VALUES(?,?)")
        .bind(id(200).encode())
        .bind(b.id.encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("owned unrelated-native corruption");
    assert!(schema::initialize(f.storage.pool_for_test()).await.is_err());
    sqlx::query("DELETE FROM node_session_automatic_release_members WHERE reservation_id=?")
        .bind(b.id.encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("restore exact original collection");
    sqlx::query("UPDATE node_session_automatic_releases SET phase='finished' WHERE operation_id=?")
        .bind(id(200).encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("owned false-finish corruption");
    assert!(schema::initialize(f.storage.pool_for_test()).await.is_err());
    sqlx::query("UPDATE node_session_automatic_releases SET phase='pending' WHERE operation_id=?")
        .bind(id(200).encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("restore owned intent");
    schema::initialize(f.storage.pool_for_test())
        .await
        .expect("valid original data still accepted");
    f.close().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "The pinned historical DDL, exact original row snapshots, migration and cleanup form one upgrade proof."
)]
async fn automatic_schema13_real_historical12_preserves_capacity_and_explicit_associations() {
    let f = Fixture::new().await;
    root(&f, 100, "historical-native").await;
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
        .bind(id(20).encode())
        .bind(id(2).encode())
        .bind("historical-native")
        .execute(f.storage.pool_for_test())
        .await
        .expect("historical target data");
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 1,
    };
    let releases = f.storage.native_release_operations();
    releases
        .register(actor, id(20), id(21), id(22), 1, &deadline())
        .await
        .expect("explicit history registration");
    releases
        .begin(actor, id(22), 2, &deadline())
        .await
        .expect("original explicit responsibility set");
    let path = f.path.with_extension("historical12.sqlite3");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .expect("separate empty historical file");
    sqlx::raw_sql(include_str!("schema12_fixture.sql"))
        .execute(&pool)
        .await
        .expect("exact DDL before any schema13 migration");
    let mut connection = pool.acquire().await.expect("owned historical connection");
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&mut *connection)
        .await
        .expect("fixture seed only");
    sqlx::query("ATTACH DATABASE ? AS fixture_seed")
        .bind(f.path.to_str().expect("owned fixture path"))
        .execute(&mut *connection)
        .await
        .expect("read fixture rows");
    let names:Vec<String>=sqlx::query_scalar("SELECT name FROM main.sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .fetch_all(&mut *connection).await.expect("pinned historical table names");
    for name in &names {
        if crate::session_storage::schema::is_retired_legacy_object(name) {
            continue;
        }
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        sqlx::query(&format!(
            "INSERT INTO main.{name} SELECT * FROM fixture_seed.{name}"
        ))
        .execute(&mut *connection)
        .await
        .expect("seed unchanged historical columns");
    }
    sqlx::query("DETACH DATABASE fixture_seed")
        .execute(&mut *connection)
        .await
        .expect("end read snapshot");
    sqlx::query("UPDATE node_meta SET schema_version=12")
        .execute(&mut *connection)
        .await
        .expect("historical metadata");
    sqlx::query("PRAGMA user_version=12")
        .execute(&mut *connection)
        .await
        .expect("historical head");
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&mut *connection)
        .await
        .expect("normal FK enforcement");
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&mut *connection)
            .await
            .expect("old relational integrity")
            .is_empty()
    );
    drop(connection);
    let capacities: Vec<CapacitySqlRow> =
        sqlx::query_as("SELECT * FROM node_session_capacity ORDER BY reservation_id")
            .fetch_all(&pool)
            .await
            .expect("original capacity rows");
    let associations: Vec<(String, String)> = sqlx::query_as(
        "SELECT * FROM node_session_capacity_releases ORDER BY operation_id,reservation_id",
    )
    .fetch_all(&pool)
    .await
    .expect("original explicit links");
    let definitions: Vec<(String, String)> =
        sqlx::query_as("SELECT name,sql FROM sqlite_schema WHERE sql IS NOT NULL ORDER BY name")
            .fetch_all(&pool)
            .await
            .expect("original layout");
    assert_eq!(capacities.len(), 1);
    assert_eq!(associations.len(), 1);
    schema::initialize(&pool)
        .await
        .expect("schema12 to13 with actual occupied capacity and explicit operation");
    assert_eq!(
        sqlx::query_as::<_, CapacitySqlRow>(
            "SELECT * FROM node_session_capacity ORDER BY reservation_id"
        )
        .fetch_all(&pool)
        .await
        .expect("preserved capacity"),
        capacities
    );
    assert_eq!(
        sqlx::query_as::<_, (String, String)>(
            "SELECT * FROM node_session_capacity_releases ORDER BY operation_id,reservation_id"
        )
        .fetch_all(&pool)
        .await
        .expect("preserved original links"),
        associations
    );
    for (name, definition) in definitions {
        if crate::session_storage::schema::is_retired_legacy_object(&name) {
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_schema WHERE name=?")
                    .bind(&name)
                    .fetch_one(&pool)
                    .await
                    .expect("retired absent"),
                0
            );
            continue;
        }
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT sql FROM sqlite_schema WHERE name=?")
                .bind(name)
                .fetch_one(&pool)
                .await
                .expect("preserved DDL"),
            definition
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM node_session_automatic_releases")
            .fetch_one(&pool)
            .await
            .expect("no inferred sysops"),
        0
    );
    pool.close().await;
    let migrated = Storage::open(&path)
        .await
        .expect("reopen exact migrated identity");
    assert_eq!(
        migrated
            .request_registry()
            .native_dispatch_repository()
            .inspect_capacity(&deadline())
            .await
            .expect("restored capacity")
            .occupied,
        1
    );
    migrated.close().await;
    for suffix in ["", "-wal", "-shm"] {
        let file = PathBuf::from(format!("{}{suffix}", path.display()));
        if file.exists() {
            std::fs::remove_file(file).expect("owned historical file cleanup");
        }
    }
    f.close().await;
}

#[tokio::test]
async fn automatic_release_source_closed_winner_finishes_only_original_history() {
    let f = Fixture::new().await;
    let original = root(&f, 100, "source-close").await;
    begin(&f, &original, 200).await;
    sqlx::query("UPDATE node_native_source_instances SET lifecycle='closed',close_basis='owned_converged',close_stage='started',close_io_evidence='started_joined',close_owner_evidence='test_owned_stream',close_evidence_fingerprint=zeroblob(32) WHERE instance_id=?")
        .bind(id(7).encode()).execute(f.storage.pool_for_test()).await.expect("full original source closure fixture");
    let repo = f.repository();
    repo.abandon_capacity(original.id, &f.source, &deadline())
        .await
        .expect("original source converged");
    assert_eq!(occupied(&f).await.occupied, 0);
    assert_eq!(
        repo.finish_automatic_release(&original, id(200), &deadline())
            .await
            .expect("system coordinates original history")
            .phase,
        AutomaticReleasePhase::Finished
    );
    assert_eq!(occupied(&f).await.occupied, 0);
    f.close().await;
}
