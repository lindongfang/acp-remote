use super::{
    CloseOutcome, ConnectionCloseStatus, ConnectionOwner, OriginalCleanupObservation,
    test_hooks::{CloseBarrier, ClosePoint},
};
use sqlx::{Connection, SqliteConnection};
use std::{sync::Arc, time::Duration};
use tokio::time::{Instant, timeout};

async fn actual_connection() -> SqliteConnection {
    SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("fresh test database")
}

#[tokio::test]
async fn actual_close_ack_and_join_survive_report_loss_as_global_uncertainty() {
    let owner = ConnectionOwner::new();
    let foreign = ConnectionOwner::new();
    let barrier = CloseBarrier::new();
    owner
        .hooks()
        .pause_next(ClosePoint::AfterCloseAck, barrier.clone())
        .expect("arm original owner");
    owner
        .hooks()
        .fail_next_result_after_ack()
        .expect("lose only reported outcome");
    let connection = owner
        .lease()
        .expect("lease")
        .attach(actual_connection().await);
    let waiter = connection.begin_owned_close().expect("original handoff");
    let receipt = waiter.receipt();
    assert!(receipt.belongs_to(&owner));
    assert!(!receipt.belongs_to(&foreign));
    assert!(receipt.same(&waiter.receipt()));
    timeout(Duration::from_secs(2), barrier.arrived())
        .await
        .expect("actual SQLite close acknowledged");
    assert_eq!(
        receipt
            .confirm_actual_cleanup(Instant::now() + Duration::from_millis(10))
            .await,
        OriginalCleanupObservation::Pending,
        "actual acknowledgement alone does not mean close task joined"
    );
    barrier.release();
    assert_eq!(waiter.wait_report().await, CloseOutcome::Unconfirmed);
    assert_eq!(
        receipt
            .confirm_actual_cleanup(Instant::now() + Duration::from_secs(2))
            .await,
        OriginalCleanupObservation::ConfirmedJoined
    );
    owner.shutdown().await;
    foreign.shutdown().await;
    assert_eq!(owner.status(), ConnectionCloseStatus::Unconfirmed);
    assert_eq!(foreign.status(), ConnectionCloseStatus::Complete);
}

#[tokio::test]
async fn before_close_timeout_keeps_original_slot_and_cannot_confirm_from_no_leases() {
    let owner = ConnectionOwner::new();
    let barrier = CloseBarrier::new();
    owner
        .hooks()
        .pause_next(ClosePoint::BeforeClose, barrier.clone())
        .expect("arm original close");
    let connection = owner
        .lease()
        .expect("lease")
        .attach(actual_connection().await);
    let waiter = connection.begin_owned_close().expect("handoff");
    let receipt = waiter.receipt();
    timeout(Duration::from_secs(2), barrier.arrived())
        .await
        .expect("reached before close");
    {
        let state = owner.state.lock().expect("original owner state");
        assert_eq!(state.leases, 0);
        assert_eq!(state.slots.len(), 1);
    }
    assert_eq!(
        receipt
            .confirm_actual_cleanup(Instant::now() + Duration::from_millis(10))
            .await,
        OriginalCleanupObservation::Pending
    );
    barrier.release();
    assert_eq!(waiter.wait_report().await, CloseOutcome::Confirmed);
    assert_eq!(
        receipt
            .confirm_actual_cleanup(Instant::now() + Duration::from_secs(2))
            .await,
        OriginalCleanupObservation::ConfirmedJoined
    );
    owner.shutdown().await;
    assert_eq!(owner.status(), ConnectionCloseStatus::Complete);
}

#[tokio::test]
async fn cancelling_join_waiter_leaves_independent_close_observer() {
    let owner = ConnectionOwner::new();
    let barrier = CloseBarrier::new();
    owner
        .hooks()
        .pause_next(ClosePoint::BeforeClose, barrier.clone())
        .expect("arm");
    let connection = owner
        .lease()
        .expect("lease")
        .attach(actual_connection().await);
    let waiter = connection.begin_owned_close().expect("handoff");
    let receipt = waiter.receipt();
    drop(waiter);
    timeout(Duration::from_secs(2), barrier.arrived())
        .await
        .expect("actual owner paused");
    let first_receipt = receipt.clone();
    let first = tokio::spawn(async move {
        first_receipt
            .confirm_actual_cleanup(Instant::now() + Duration::from_secs(2))
            .await
    });
    tokio::task::yield_now().await;
    let second_receipt = receipt.clone();
    let second = tokio::spawn(async move {
        second_receipt
            .confirm_actual_cleanup(Instant::now() + Duration::from_secs(2))
            .await
    });
    first.abort();
    assert!(first.await.expect_err("cancelled waiter").is_cancelled());
    barrier.release();
    assert_eq!(
        second.await.expect("remaining waiter joined"),
        OriginalCleanupObservation::ConfirmedJoined
    );
    assert_eq!(
        receipt
            .confirm_actual_cleanup(Instant::now() + Duration::from_secs(2))
            .await,
        OriginalCleanupObservation::ConfirmedJoined
    );
    owner.shutdown().await;
    assert_eq!(owner.status(), ConnectionCloseStatus::Complete);
}

#[tokio::test]
async fn aborted_actual_close_task_never_becomes_confirmed_cleanup() {
    let owner = ConnectionOwner::new();
    let barrier = CloseBarrier::new();
    owner
        .hooks()
        .pause_next(ClosePoint::BeforeClose, Arc::clone(&barrier))
        .expect("arm");
    let connection = owner
        .lease()
        .expect("lease")
        .attach(actual_connection().await);
    let waiter = connection.begin_owned_close().expect("handoff");
    let receipt = waiter.receipt();
    drop(waiter);
    timeout(Duration::from_secs(2), barrier.arrived())
        .await
        .expect("actual close has not run");
    receipt.slot.abort.abort();
    assert_eq!(
        receipt
            .confirm_actual_cleanup(Instant::now() + Duration::from_secs(2))
            .await,
        OriginalCleanupObservation::UnconfirmedJoined
    );
    owner.shutdown().await;
    assert_eq!(owner.status(), ConnectionCloseStatus::Unconfirmed);
}
