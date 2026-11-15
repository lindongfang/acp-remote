//! Independent L3 consumer cases bound to the pre-reviewed Q3 contract.
//! No authentication, durable deduplication, protocol I/O or native result claims.
use acp_remote::domain::{
    EntityId,
    input_queue::{
        DispatchReadiness, DispatchToken, InputQueue, MAX_WAITING_INPUTS, PriorityFailure,
        QueueDisposition, QueueEntry, QueueError, QueueInput, Rejected, Submission,
    },
    native::{HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch},
};

const X: u8 = 21;
const Y: u8 = 22;
const Z: u8 = 23;
const D: u8 = 24;
const E: u8 = 25;
const W: u8 = 26;
const fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
fn key(source: u8, native: &str) -> NativeSessionKey {
    NativeSessionKey::from_config(Some(id(source)), native).unwrap()
}
fn q() -> InputQueue<String> {
    InputQueue::new(key(1, "q3-synthetic-session"))
}
const fn epoch(load: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(id(101)),
        connection: HarnessConnectionEpoch(id(102)),
        load: LoadEpoch(id(load)),
    }
}
const fn ready() -> DispatchReadiness {
    DispatchReadiness::Ready(epoch(103))
}
fn body(n: u8) -> String {
    format!("input-{n}/v1")
}
fn input(n: u8) -> QueueInput<String> {
    QueueInput {
        id: id(n),
        submitter: id(10),
        body: body(n),
    }
}
fn queued(queue: &mut InputQueue<String>, n: u8) {
    match queue.submit(input(n), DispatchReadiness::Blocked).unwrap() {
        Submission::Queued {
            id: actual,
            revision,
        } => {
            assert_eq!(actual, id(n));
            assert_eq!(revision, 1);
        }
        Submission::Dispatching(_) => panic!("ordinary admission unexpectedly dispatches"),
    }
}
fn direct(queue: &mut InputQueue<String>, n: u8) -> DispatchToken {
    match queue.submit(input(n), ready()).unwrap() {
        Submission::Dispatching(token) => token,
        Submission::Queued { .. } => panic!("eligible failure-mode input did not dispatch"),
    }
}
fn start(queue: &mut InputQueue<String>) -> DispatchToken {
    let token = queue.prepare(ready()).unwrap().expect("runnable item");
    queue.begin_send(&token, ready()).unwrap();
    token
}
fn suspend(items: &[u8]) -> InputQueue<String> {
    let mut queue = q();
    queued(&mut queue, W);
    let token = start(&mut queue);
    for &n in items {
        queued(&mut queue, n);
    }
    assert_eq!(
        queue
            .settle(&token, QueueDisposition::Suspend)
            .unwrap()
            .id(),
        id(W)
    );
    assert!(queue.failure_suspended());
    assert!(queue.waiting().all(QueueEntry::suspended));
    queue
}
fn error<T>(result: Result<T, QueueError>) -> QueueError {
    result.err().expect("expected rejection")
}
type Entry = (EntityId, u64, EntityId, EntityId, String, bool);
fn entry(e: &QueueEntry<String>) -> Entry {
    (
        e.id(),
        e.revision(),
        e.input().submitter,
        e.editor(),
        e.input().body.clone(),
        e.suspended(),
    )
}
fn snapshot(queue: &InputQueue<String>) -> (Vec<Entry>, Option<Entry>, bool) {
    (
        queue.waiting().map(entry).collect(),
        queue.active().map(entry),
        queue.failure_suspended(),
    )
}
fn ids(queue: &InputQueue<String>) -> Vec<EntityId> {
    queue.waiting().map(QueueEntry::id).collect()
}
fn reject_submit(
    queue: &mut InputQueue<String>,
    n: u8,
    r: DispatchReadiness,
    expected: QueueError,
) {
    let before = snapshot(queue);
    let rejected = queue.submit(input(n), r).unwrap_err();
    assert_eq!(rejected.error, expected);
    assert_eq!(rejected.value.id, id(n));
    assert_eq!(rejected.value.body, body(n));
    assert_eq!(snapshot(queue), before);
}

#[test]
fn cl087_api01_capacity_last_slot_orders_and_normal_ready_queues() {
    assert_eq!(MAX_WAITING_INPUTS, 3);
    for (third, loser) in [(Z, E), (E, Z)] {
        let mut queue = q();
        queued(&mut queue, X);
        queued(&mut queue, Y);
        queued(&mut queue, third);
        reject_submit(
            &mut queue,
            loser,
            DispatchReadiness::Blocked,
            QueueError::Full,
        );
        assert_eq!(ids(&queue), vec![id(X), id(Y), id(third)]);
    }
    let mut queue = q();
    assert!(
        matches!(queue.submit(input(X), ready()).unwrap(), Submission::Queued { id: i, revision: 1 } if i == id(X))
    );
    assert!(queue.active().is_none());
}

#[test]
fn cl087_api02_preparation_stays_in_three_waiting_slots() {
    let mut queue = q();
    queued(&mut queue, X);
    queued(&mut queue, Y);
    let token = queue.prepare(ready()).unwrap().unwrap();
    queued(&mut queue, Z);
    reject_submit(&mut queue, E, ready(), QueueError::Full);
    assert_eq!(ids(&queue), vec![id(X), id(Y), id(Z)]);
    assert!(queue.active().is_none());
    queue.begin_send(&token, ready()).unwrap();
    assert_eq!(queue.active().unwrap().id(), id(X));
}

#[test]
fn cl087_api03_targets_and_same_key_instance_token_fence() {
    let target = key(1, "q3-synthetic-session");
    let mut queues = [
        InputQueue::new(target.clone()),
        InputQueue::new(key(2, "q3-synthetic-session")),
        InputQueue::new(key(1, "q3-other")),
    ];
    assert_eq!(queues[0].target(), &target);
    assert_ne!(queues[0].target(), queues[1].target());
    assert_ne!(queues[0].target(), queues[2].target());
    for queue in &mut queues {
        for n in [X, Y, Z] {
            queued(queue, n);
        }
    }
    let token = queues[0].prepare(ready()).unwrap().unwrap();
    let mut same_key = q();
    queued(&mut same_key, X);
    let before = snapshot(&same_key);
    assert_eq!(
        error(same_key.begin_send(&token, ready())),
        QueueError::StaleToken
    );
    assert_eq!(snapshot(&same_key), before);
    for queue in &mut queues[1..] {
        let before = snapshot(queue);
        assert_eq!(
            error(queue.release_preparation(&token)),
            QueueError::StaleToken
        );
        assert_eq!(snapshot(queue), before);
    }
    queues[0].begin_send(&token, ready()).unwrap();
}

#[test]
fn cl088_api01_edit_identity_version_and_all_stale_mutations_unchanged() {
    let mut queue = q();
    for n in [X, Y, Z] {
        queued(&mut queue, n);
    }
    assert_eq!(
        queue.edit(id(Y), 1, id(11), "Y/v2".into()).unwrap(),
        body(Y)
    );
    let edited = queue.waiting().nth(1).unwrap();
    assert_eq!(edited.id(), id(Y));
    assert_eq!(edited.revision(), 2);
    assert_eq!(edited.editor(), id(11));
    assert_eq!(edited.input().submitter, id(10));
    assert_eq!(edited.input().body, "Y/v2");
    let before = snapshot(&queue);
    let rejected = queue
        .edit(id(Y), 1, id(12), "stale replacement".into())
        .unwrap_err();
    assert_eq!(rejected.error, QueueError::RevisionChanged);
    assert_eq!(rejected.value, "stale replacement");
    assert_eq!(snapshot(&queue), before);
    assert_eq!(error(queue.cancel(id(Y), 1)), QueueError::RevisionChanged);
    assert_eq!(snapshot(&queue), before);
    assert_eq!(
        error(queue.prioritize(id(Y), 1)),
        QueueError::RevisionChanged
    );
    assert_eq!(snapshot(&queue), before);
}

#[test]
fn cl088_api02_cancel_releases_slot_without_second_cancel_effect() {
    let mut queue = q();
    for n in [X, Y, Z] {
        queued(&mut queue, n);
    }
    assert_eq!(queue.cancel(id(X), 1).unwrap().into_input().body, body(X));
    let before = snapshot(&queue);
    assert_eq!(error(queue.cancel(id(X), 1)), QueueError::NotFound);
    assert_eq!(snapshot(&queue), before);
    queued(&mut queue, E);
    assert_eq!(ids(&queue), vec![id(Y), id(Z), id(E)]);
}

#[test]
fn cl089_api01_fifo_and_active_blocking_with_independent_waiting_slots() {
    let mut queue = q();
    for n in [X, Y, Z] {
        queued(&mut queue, n);
    }
    for n in [X, Y, Z] {
        let token = start(&mut queue);
        assert_eq!(queue.dispatch_input(&token).unwrap().id(), id(n));
        let before = snapshot(&queue);
        assert_eq!(error(queue.prepare(ready())), QueueError::Busy);
        assert_eq!(snapshot(&queue), before);
        if n == X {
            queued(&mut queue, D);
            assert_eq!(queue.waiting().len(), 3);
        }
        assert_eq!(
            queue
                .settle(&token, QueueDisposition::Advance)
                .unwrap()
                .id(),
            id(n)
        );
    }
    let token = start(&mut queue);
    assert_eq!(queue.active().unwrap().id(), id(D));
    queue.settle(&token, QueueDisposition::Advance).unwrap();
    assert!(queue.prepare(ready()).unwrap().is_none());
}

#[test]
fn cl090_api01_priority_waits_for_ready_and_preserves_other_relative_order() {
    let mut queue = q();
    for n in [X, Y, Z] {
        queued(&mut queue, n);
    }
    queue.prioritize(id(Y), 1).unwrap();
    let before = snapshot(&queue);
    assert!(queue.prepare(DispatchReadiness::Blocked).unwrap().is_none());
    assert_eq!(snapshot(&queue), before);
    let token = start(&mut queue);
    assert_eq!(queue.active().unwrap().id(), id(Y));
    assert_eq!(ids(&queue), vec![id(X), id(Z)]);
    queue.settle(&token, QueueDisposition::Advance).unwrap();
    for n in [X, Z] {
        let t = start(&mut queue);
        assert_eq!(queue.active().unwrap().id(), id(n));
        queue.settle(&t, QueueDisposition::Advance).unwrap();
    }
}

#[test]
fn cl090_api02_priority_change_and_edit_invalidate_selection_without_fallback() {
    let mut queue = q();
    for n in [X, Y, Z] {
        queued(&mut queue, n);
    }
    let tx = queue.prepare(ready()).unwrap().unwrap();
    queue.prioritize(id(Y), 1).unwrap();
    let before = snapshot(&queue);
    assert_eq!(
        error(queue.begin_send(&tx, ready())),
        QueueError::StaleToken
    );
    assert_eq!(snapshot(&queue), before);
    let ty = queue.prepare(ready()).unwrap().unwrap();
    queue.edit(id(Y), 1, id(11), "Y/v2".into()).unwrap();
    assert_eq!(
        queue.priority_failure(),
        Some(PriorityFailure::SelectionChanged)
    );
    assert_eq!(queue.priority(), None);
    assert_eq!(ids(&queue), vec![id(X), id(Y), id(Z)]);
    assert!(queue.waiting().all(QueueEntry::suspended));
    let before = snapshot(&queue);
    assert_eq!(
        error(queue.begin_send(&ty, ready())),
        QueueError::StaleToken
    );
    assert_eq!(snapshot(&queue), before);
    assert!(queue.prepare(ready()).unwrap().is_none());
    assert_eq!(queue.resume(), 3);
    assert_eq!(queue.priority_failure(), None);
    let current = queue.prepare(ready()).unwrap().unwrap();
    assert_eq!(queue.begin_send(&current, ready()), Ok(()));
    let e = queue.dispatch_input(&current).unwrap();
    assert_eq!(e.id(), id(X));
    assert_eq!(e.revision(), 1);
}

#[test]
fn cl090_api03_last_priority_then_cancel_pauses_without_fallback() {
    let mut queue = q();
    for n in [X, Y, Z] {
        queued(&mut queue, n);
    }
    queue.prioritize(id(Y), 1).unwrap();
    queue.prioritize(id(Z), 1).unwrap();
    queue.cancel(id(Z), 1).unwrap();
    assert_eq!(
        queue.priority_failure(),
        Some(PriorityFailure::SelectionChanged)
    );
    assert_eq!(ids(&queue), vec![id(X), id(Y)]);
    assert!(queue.waiting().all(QueueEntry::suspended));
    assert!(queue.prepare(ready()).unwrap().is_none());
    assert_eq!(queue.resume(), 2);
    let token = start(&mut queue);
    assert_eq!(queue.dispatch_input(&token).unwrap().id(), id(X));
    queue.settle(&token, QueueDisposition::Advance).unwrap();
    let token = start(&mut queue);
    assert_eq!(queue.dispatch_input(&token).unwrap().id(), id(Y));
}

#[test]
fn cl090_api04_normal_full_draft_and_priority_cannot_bypass_active() {
    let mut queue = q();
    for n in [X, Y, Z] {
        queued(&mut queue, n);
    }
    reject_submit(&mut queue, D, ready(), QueueError::Full);
    let token = start(&mut queue);
    queue.prioritize(id(Y), 1).unwrap();
    let before = snapshot(&queue);
    assert_eq!(error(queue.prepare(ready())), QueueError::Busy);
    assert_eq!(snapshot(&queue), before);
    assert_eq!(queue.dispatch_input(&token).unwrap().id(), id(X));
}

#[test]
fn cl092_api01_dispatch_has_no_original_actor_or_gate_argument() {
    let mut queue = q();
    queued(&mut queue, X);
    let token = start(&mut queue);
    assert_eq!(
        queue.dispatch_input(&token).unwrap().input().submitter,
        id(10)
    );
    let original = queue
        .settle(&token, QueueDisposition::Advance)
        .unwrap()
        .into_input();
    assert_eq!(original.submitter, id(10));
    assert_eq!(original.body, body(X));
}

struct Payload {
    marker: String,
    structure: Vec<(String, Vec<u8>)>,
}
fn payload(s: &str) -> Payload {
    Payload {
        marker: s.into(),
        structure: vec![
            ("opaque/未知".into(), vec![0, 255, 7]),
            ("😀\nline".into(), vec![9, 0]),
        ],
    }
}
fn check_payload(p: &Payload, marker: &str) {
    assert_eq!(p.marker, marker);
    assert_eq!(
        p.structure,
        vec![
            ("opaque/未知".into(), vec![0, 255, 7]),
            ("😀\nline".into(), vec![9, 0])
        ]
    );
}

#[test]
fn cl095_api01_nonclone_nondebug_structured_body_moves_without_loss() {
    let mut queue = InputQueue::new(key(1, "q3-structured"));
    assert!(matches!(
        queue
            .submit(
                QueueInput {
                    id: id(X),
                    submitter: id(10),
                    body: payload("original")
                },
                ready()
            )
            .unwrap(),
        Submission::Queued { .. }
    ));
    let old = queue.edit(id(X), 1, id(11), payload("edited")).unwrap();
    check_payload(&old, "original");
    let token = queue.prepare(ready()).unwrap().unwrap();
    queue.begin_send(&token, ready()).unwrap();
    check_payload(
        &queue.dispatch_input(&token).unwrap().input().body,
        "edited",
    );
    let moved = queue
        .settle(&token, QueueDisposition::Advance)
        .unwrap()
        .into_input();
    check_payload(&moved.body, "edited");
    assert_eq!(moved.id, id(X));
    assert_eq!(moved.submitter, id(10));
}

#[test]
fn cl095_api02_over_32mib_body_and_edit_still_count_as_one_slot() {
    let mut queue = q();
    let large = "x".repeat(32 * 1024 * 1024 + 1);
    queue
        .submit(
            QueueInput {
                id: id(X),
                submitter: id(10),
                body: large,
            },
            ready(),
        )
        .unwrap();
    assert_eq!(queue.waiting().len(), 1);
    assert_eq!(
        queue.waiting().next().unwrap().input().body.len(),
        32 * 1024 * 1024 + 1
    );
    assert!(
        queue
            .waiting()
            .next()
            .unwrap()
            .input()
            .body
            .bytes()
            .all(|b| b == b'x')
    );
    let replacement = "y".repeat(32 * 1024 * 1024 + 1);
    let old = queue.edit(id(X), 1, id(11), replacement).unwrap();
    assert_eq!(old.len(), 32 * 1024 * 1024 + 1);
    assert!(old.bytes().all(|b| b == b'x'));
    drop(old);
    assert!(
        queue
            .waiting()
            .next()
            .unwrap()
            .input()
            .body
            .bytes()
            .all(|b| b == b'y')
    );
    queued(&mut queue, Y);
    queued(&mut queue, Z);
    reject_submit(&mut queue, D, ready(), QueueError::Full);
    let moved = queue.cancel(id(X), 2).unwrap().into_input();
    assert_eq!(moved.body.len(), 32 * 1024 * 1024 + 1);
    assert!(moved.body.bytes().all(|b| b == b'y'));
}

#[test]
fn cl096_api01_node_processing_order_has_no_clock_input() {
    let mut queue = q();
    for n in [Z, X, Y] {
        queued(&mut queue, n);
    }
    assert_eq!(ids(&queue), vec![id(Z), id(X), id(Y)]);
    queue.edit(id(X), 1, id(11), "new".into()).unwrap();
    assert_eq!(ids(&queue), vec![id(Z), id(X), id(Y)]);
}

#[test]
fn cl097_api01_observation_and_blocked_prepare_do_not_expire_items() {
    let mut queue = q();
    queued(&mut queue, X);
    let before = snapshot(&queue);
    for _ in 0..1000 {
        assert_eq!(snapshot(&queue), before);
        assert!(queue.prepare(DispatchReadiness::Blocked).unwrap().is_none());
    }
    assert_eq!(
        queue
            .edit(id(X), 1, id(11), "still-editable".into())
            .unwrap(),
        body(X)
    );
    assert_eq!(
        queue.cancel(id(X), 2).unwrap().input().body,
        "still-editable"
    );
    queued(&mut queue, Y);
    let token = start(&mut queue);
    assert_eq!(queue.dispatch_input(&token).unwrap().id(), id(Y));
}

#[test]
fn cl098_api01_fresh_instance_is_empty_after_old_instance_drop() {
    {
        let mut queue = q();
        queued(&mut queue, X);
        queued(&mut queue, Y);
        queue.edit(id(X), 1, id(11), "edited".into()).unwrap();
    }
    let queue = q();
    assert_eq!(queue.waiting().len(), 0);
    assert!(queue.active().is_none());
    assert!(!queue.failure_suspended());
}

#[test]
fn cl099_api01_cancel_before_begin_and_begin_before_cancel_or_edit() {
    let mut queue = q();
    queued(&mut queue, X);
    let t = queue.prepare(ready()).unwrap().unwrap();
    assert_eq!(queue.cancel(id(X), 1).unwrap().into_input().body, body(X));
    let before = snapshot(&queue);
    assert_eq!(error(queue.begin_send(&t, ready())), QueueError::StaleToken);
    assert_eq!(snapshot(&queue), before);
    queued(&mut queue, Y);
    let t = start(&mut queue);
    let before = snapshot(&queue);
    assert_eq!(error(queue.cancel(id(Y), 1)), QueueError::DispatchStarted);
    assert_eq!(snapshot(&queue), before);
    let rejected = queue
        .edit(id(Y), 1, id(11), "attempted".into())
        .unwrap_err();
    assert_eq!(rejected.error, QueueError::DispatchStarted);
    assert_eq!(rejected.value, "attempted");
    assert_eq!(snapshot(&queue), before);
    assert_eq!(queue.dispatch_input(&t).unwrap().input().body, body(Y));
}

#[test]
fn cl099_api02_edit_before_begin_requires_current_revision() {
    let mut queue = q();
    queued(&mut queue, X);
    let old = queue.prepare(ready()).unwrap().unwrap();
    queue.edit(id(X), 1, id(11), "current-v2".into()).unwrap();
    let before = snapshot(&queue);
    assert_eq!(
        error(queue.begin_send(&old, ready())),
        QueueError::StaleToken
    );
    assert_eq!(snapshot(&queue), before);
    let current = queue.prepare(ready()).unwrap().unwrap();
    assert_eq!(
        error(queue.begin_send(&old, ready())),
        QueueError::StaleToken
    );
    queue.begin_send(&current, ready()).unwrap();
    assert_eq!(queue.active().unwrap().revision(), 2);
    assert_eq!(queue.active().unwrap().input().body, "current-v2");
}

#[test]
fn cl099_api03_begin_rechecks_blocked_and_every_epoch_component() {
    let mut queue = q();
    queued(&mut queue, X);
    let t = queue.prepare(ready()).unwrap().unwrap();
    let before = snapshot(&queue);
    assert_eq!(
        error(queue.begin_send(&t, DispatchReadiness::Blocked)),
        QueueError::Blocked
    );
    assert_eq!(snapshot(&queue), before);
    let mut changed = epoch(103);
    changed.runtime = RuntimeEpoch(id(106));
    let mut connection = epoch(103);
    connection.connection = HarnessConnectionEpoch(id(105));
    for e in [epoch(104), connection, changed] {
        assert_eq!(
            error(queue.begin_send(&t, DispatchReadiness::Ready(e))),
            QueueError::ResourceChanged
        );
        assert_eq!(snapshot(&queue), before);
    }
    queue.begin_send(&t, ready()).unwrap();
    assert_eq!(queue.waiting().len(), 0);
    assert_eq!(queue.active().unwrap().id(), id(X));
}

#[test]
fn cl099_api04_release_keeps_position_and_old_tokens_are_not_reused() {
    let mut queue = q();
    queued(&mut queue, X);
    queued(&mut queue, Y);
    let t = queue.prepare(ready()).unwrap().unwrap();
    let before = snapshot(&queue);
    assert_eq!(error(queue.prepare(ready())), QueueError::Busy);
    queue.release_preparation(&t).unwrap();
    assert_eq!(snapshot(&queue), before);
    assert_eq!(error(queue.release_preparation(&t)), QueueError::StaleToken);
    assert_eq!(error(queue.begin_send(&t, ready())), QueueError::StaleToken);
    assert_eq!(snapshot(&queue), before);
    let next = queue.prepare(ready()).unwrap().unwrap();
    assert_eq!(error(queue.release_preparation(&t)), QueueError::StaleToken);
    queue.begin_send(&next, ready()).unwrap();
    assert_eq!(queue.active().unwrap().id(), id(X));
}

#[test]
fn cl099_api05_clone_settles_once_and_foreign_tokens_cannot_read_or_settle() {
    let mut queue = q();
    queued(&mut queue, X);
    queued(&mut queue, Y);
    let t = start(&mut queue);
    let clone = t.clone();
    let before = snapshot(&queue);
    assert_eq!(
        error(queue.begin_send(&clone, ready())),
        QueueError::StaleToken
    );
    assert_eq!(snapshot(&queue), before);
    let mut foreign = q();
    queued(&mut foreign, X);
    let ft = start(&mut foreign);
    let foreign_before = snapshot(&foreign);
    assert_eq!(error(foreign.dispatch_input(&t)), QueueError::StaleToken);
    assert_eq!(
        error(foreign.settle(&t, QueueDisposition::Suspend)),
        QueueError::StaleToken
    );
    assert_eq!(snapshot(&foreign), foreign_before);
    assert_eq!(error(queue.dispatch_input(&ft)), QueueError::StaleToken);
    assert_eq!(snapshot(&queue), before);
    assert_eq!(
        queue.settle(&t, QueueDisposition::Advance).unwrap().id(),
        id(X)
    );
    let new = start(&mut queue);
    let before = snapshot(&queue);
    assert_eq!(error(queue.dispatch_input(&clone)), QueueError::StaleToken);
    assert_eq!(
        error(queue.settle(&clone, QueueDisposition::Suspend)),
        QueueError::StaleToken
    );
    assert_eq!(snapshot(&queue), before);
    assert_eq!(queue.dispatch_input(&new).unwrap().id(), id(Y));
}

#[test]
fn cl110_api01_full_suspension_direct_new_work_and_advance_keeps_old_suspended() {
    let mut queue = suspend(&[X, Y, Z]);
    let old = ids(&queue);
    let d = direct(&mut queue, D);
    assert_eq!(ids(&queue), old);
    assert!(queue.waiting().all(QueueEntry::suspended));
    assert_eq!(queue.active().unwrap().id(), id(D));
    reject_submit(&mut queue, E, ready(), QueueError::Full);
    queue.settle(&d, QueueDisposition::Advance).unwrap();
    assert!(queue.failure_suspended());
    assert!(queue.prepare(ready()).unwrap().is_none());
    let e = direct(&mut queue, E);
    assert_eq!(queue.dispatch_input(&e).unwrap().id(), id(E));
    assert_eq!(ids(&queue), old);
    assert!(queue.waiting().all(QueueEntry::suspended));
}

#[test]
fn cl110_api02_edit_observe_and_prioritize_do_not_resume_old_items() {
    let mut queue = suspend(&[X, Y, Z]);
    queue.edit(id(X), 1, id(11), "X edited".into()).unwrap();
    let before = snapshot(&queue);
    assert_eq!(error(queue.prioritize(id(X), 2)), QueueError::Suspended);
    assert_eq!(snapshot(&queue), before);
    assert!(queue.prepare(ready()).unwrap().is_none());
    assert_eq!(snapshot(&queue), before);
    assert_eq!(queue.waiting().next().unwrap().revision(), 2);
    assert!(queue.failure_suspended());
}

#[test]
fn cl111_api01_waiting_new_item_gets_last_slot_and_dispatches_before_suspended() {
    let mut queue = suspend(&[X, Y]);
    queued(&mut queue, D);
    assert_eq!(ids(&queue), vec![id(X), id(Y), id(D)]);
    assert!(!queue.waiting().last().unwrap().suspended());
    reject_submit(&mut queue, E, ready(), QueueError::Full);
    let d = start(&mut queue);
    assert_eq!(queue.active().unwrap().id(), id(D));
    queue.settle(&d, QueueDisposition::Advance).unwrap();
    assert_eq!(ids(&queue), vec![id(X), id(Y)]);
    assert!(queue.waiting().all(QueueEntry::suspended));
    assert!(queue.prepare(ready()).unwrap().is_none());
}

#[test]
fn cl111_api02_new_failure_suspends_all_remaining_items_without_requeueing_failed_input() {
    let mut queue = suspend(&[X]);
    queued(&mut queue, D);
    queued(&mut queue, E);
    let d = start(&mut queue);
    assert_eq!(queue.active().unwrap().id(), id(D));
    queue.prioritize(id(E), 1).unwrap();
    assert_eq!(
        queue.settle(&d, QueueDisposition::Suspend).unwrap().id(),
        id(D)
    );
    assert_eq!(ids(&queue), vec![id(X), id(E)]);
    assert!(queue.waiting().all(QueueEntry::suspended));
    assert!(queue.prepare(ready()).unwrap().is_none());
    assert_eq!(queue.resume(), 2);
    let next = start(&mut queue);
    assert_eq!(queue.dispatch_input(&next).unwrap().id(), id(X));
}

#[test]
fn cl112_api01_resume_retries_do_not_duplicate_or_preempt_active_new_input() {
    let mut queue = suspend(&[X, Y]);
    let d = direct(&mut queue, D);
    assert_eq!(queue.resume(), 2);
    assert_eq!(queue.resume(), 0);
    assert!(!queue.failure_suspended());
    assert_eq!(ids(&queue), vec![id(X), id(Y)]);
    assert_eq!(error(queue.prepare(ready())), QueueError::Busy);
    assert_eq!(queue.dispatch_input(&d).unwrap().id(), id(D));
    queue.settle(&d, QueueDisposition::Advance).unwrap();
    for n in [X, Y] {
        let t = start(&mut queue);
        assert_eq!(queue.active().unwrap().id(), id(n));
        queue.settle(&t, QueueDisposition::Advance).unwrap();
    }
    assert_eq!(queue.waiting().len(), 0);
    assert!(queue.prepare(ready()).unwrap().is_none());
}

#[test]
fn cl112_api02_resume_places_existing_runnable_d_before_old_xy() {
    let mut queue = suspend(&[X, Y]);
    queued(&mut queue, D);
    assert_eq!(queue.resume(), 2);
    assert_eq!(ids(&queue), vec![id(D), id(X), id(Y)]);
    assert!(queue.waiting().all(|e| !e.suspended()));
    for n in [D, X, Y] {
        let t = start(&mut queue);
        assert_eq!(queue.active().unwrap().id(), id(n));
        queue.settle(&t, QueueDisposition::Advance).unwrap();
    }
}

#[test]
fn cl112_api03_resume_invalidates_prepared_d_without_changing_new_first_order() {
    let mut queue = suspend(&[X, Y]);
    queued(&mut queue, D);
    let old = queue.prepare(ready()).unwrap().unwrap();
    assert_eq!(queue.resume(), 2);
    let before = snapshot(&queue);
    assert_eq!(
        error(queue.begin_send(&old, ready())),
        QueueError::StaleToken
    );
    assert_eq!(snapshot(&queue), before);
    assert_eq!(ids(&queue), vec![id(D), id(X), id(Y)]);
    let t = start(&mut queue);
    assert_eq!(queue.dispatch_input(&t).unwrap().id(), id(D));
}

#[test]
fn cl113_api01_two_linear_orders_only_one_direct_owner_and_no_fourth_waiting_slot() {
    for (first, second) in [(D, E), (E, D)] {
        let mut queue = suspend(&[X, Y, Z]);
        let token = direct(&mut queue, first);
        reject_submit(&mut queue, second, ready(), QueueError::Full);
        assert_eq!(ids(&queue), vec![id(X), id(Y), id(Z)]);
        assert_eq!(queue.waiting().len(), 3);
        assert!(queue.waiting().all(QueueEntry::suspended));
        assert_eq!(queue.dispatch_input(&token).unwrap().id(), id(first));
    }
}

#[test]
fn cl113_api02_blocked_full_and_existing_runnable_or_prepared_item_forbid_direct_bypass() {
    let mut queue = suspend(&[X, Y, Z]);
    reject_submit(&mut queue, D, DispatchReadiness::Blocked, QueueError::Full);
    let mut queue = suspend(&[X, Y]);
    queued(&mut queue, D);
    let token = queue.prepare(ready()).unwrap().unwrap();
    reject_submit(&mut queue, E, ready(), QueueError::Full);
    queue.begin_send(&token, ready()).unwrap();
    assert_eq!(queue.active().unwrap().id(), id(D));
}

#[test]
fn cl113_api03_direct_active_is_retained_when_last_transport_conditions_change() {
    let mut queue = suspend(&[X, Y, Z]);
    let d = direct(&mut queue, D);
    let before = snapshot(&queue);
    assert_eq!(
        error(queue.prepare(DispatchReadiness::Blocked)),
        QueueError::Busy
    );
    reject_submit(
        &mut queue,
        E,
        DispatchReadiness::Ready(epoch(104)),
        QueueError::Full,
    );
    assert_eq!(snapshot(&queue), before);
    assert_eq!(queue.dispatch_input(&d).unwrap().id(), id(D));
}

#[test]
fn cl115_api01_other_editor_cancel_and_resume_during_active_new_input() {
    let mut queue = suspend(&[X, Y]);
    queue.edit(id(X), 1, id(11), "X by B".into()).unwrap();
    assert_eq!(queue.cancel(id(Y), 1).unwrap().id(), id(Y));
    let d = direct(&mut queue, D);
    let x = queue.waiting().next().unwrap();
    assert_eq!(x.id(), id(X));
    assert_eq!(x.input().submitter, id(10));
    assert_eq!(x.editor(), id(11));
    assert!(x.suspended());
    assert_eq!(queue.resume(), 1);
    assert_eq!(queue.dispatch_input(&d).unwrap().id(), id(D));
    assert_eq!(error(queue.prepare(ready())), QueueError::Busy);
    queue.settle(&d, QueueDisposition::Advance).unwrap();
    let x = start(&mut queue);
    let out = queue.settle(&x, QueueDisposition::Advance).unwrap();
    assert_eq!(out.id(), id(X));
    assert_eq!(out.revision(), 2);
    assert_eq!(out.input().body, "X by B");
    assert_eq!(out.editor(), id(11));
    assert_eq!(queue.waiting().len(), 0);
}

#[test]
fn cl115_api02_resumed_cancel_send_order_and_stale_revision_keep_state() {
    for cancel_first in [true, false] {
        let mut queue = suspend(&[X, Y]);
        queue.edit(id(X), 1, id(11), "X v2".into()).unwrap();
        queue.resume();
        let t = queue.prepare(ready()).unwrap().unwrap();
        let before = snapshot(&queue);
        assert_eq!(error(queue.cancel(id(X), 1)), QueueError::RevisionChanged);
        assert_eq!(
            error(queue.prioritize(id(X), 1)),
            QueueError::RevisionChanged
        );
        assert_eq!(snapshot(&queue), before);
        if cancel_first {
            queue.cancel(id(X), 2).unwrap();
            let before = snapshot(&queue);
            assert_eq!(error(queue.begin_send(&t, ready())), QueueError::StaleToken);
            assert_eq!(snapshot(&queue), before);
        } else {
            queue.begin_send(&t, ready()).unwrap();
            let before = snapshot(&queue);
            assert_eq!(error(queue.cancel(id(X), 2)), QueueError::DispatchStarted);
            assert_eq!(snapshot(&queue), before);
        }
    }
}

#[test]
fn cl087_api04_duplicate_waiting_or_active_id_precedes_full_and_returns_new_input() {
    let mut queue = q();
    for n in [X, Y, Z] {
        queued(&mut queue, n);
    }
    let before = snapshot(&queue);
    let rejected = queue
        .submit(
            QueueInput {
                id: id(X),
                submitter: id(11),
                body: "different body".into(),
            },
            ready(),
        )
        .unwrap_err();
    assert_eq!(rejected.error, QueueError::DuplicateId);
    assert_eq!(rejected.value.body, "different body");
    assert_eq!(rejected.value.submitter, id(11));
    assert_eq!(snapshot(&queue), before);
    let t = start(&mut queue);
    reject_submit(&mut queue, X, ready(), QueueError::DuplicateId);
    assert_eq!(queue.dispatch_input(&t).unwrap().input().body, body(X));
}

#[test]
fn cl088_api03_equal_body_edit_advances_version_and_mutation_errors_are_ordered() {
    let mut queue = q();
    queued(&mut queue, X);
    assert_eq!(queue.waiting().next().unwrap().editor(), id(10));
    assert_eq!(queue.edit(id(X), 1, id(11), body(X)).unwrap(), body(X));
    assert_eq!(queue.waiting().next().unwrap().revision(), 2);
    assert_eq!(queue.waiting().next().unwrap().editor(), id(11));
    let before = snapshot(&queue);
    let missing = queue.edit(id(99), 0, id(11), "missing".into()).unwrap_err();
    assert_eq!(missing.error, QueueError::NotFound);
    assert_eq!(missing.value, "missing");
    assert_eq!(snapshot(&queue), before);
    assert_eq!(error(queue.cancel(id(99), 0)), QueueError::NotFound);
    assert_eq!(error(queue.prioritize(id(99), 0)), QueueError::NotFound);
    let token = start(&mut queue);
    let before = snapshot(&queue);
    assert_eq!(
        queue
            .edit(id(X), 0, id(11), "active".into())
            .unwrap_err()
            .error,
        QueueError::DispatchStarted
    );
    assert_eq!(error(queue.cancel(id(X), 0)), QueueError::DispatchStarted);
    assert_eq!(
        error(queue.prioritize(id(X), 0)),
        QueueError::DispatchStarted
    );
    assert_eq!(snapshot(&queue), before);
    assert_eq!(queue.dispatch_input(&token).unwrap().revision(), 2);
}

#[test]
fn cl099_api06_busy_and_stale_precede_blocked_then_epoch_error() {
    let mut queue = q();
    queued(&mut queue, X);
    let stale = queue.prepare(ready()).unwrap().unwrap();
    queue.release_preparation(&stale).unwrap();
    let current = queue.prepare(ready()).unwrap().unwrap();
    let before = snapshot(&queue);
    assert_eq!(
        error(queue.prepare(DispatchReadiness::Blocked)),
        QueueError::Busy
    );
    assert_eq!(
        error(queue.begin_send(&stale, DispatchReadiness::Blocked)),
        QueueError::StaleToken
    );
    assert_eq!(
        error(queue.begin_send(&current, DispatchReadiness::Blocked)),
        QueueError::Blocked
    );
    assert_eq!(
        error(queue.begin_send(&current, DispatchReadiness::Ready(epoch(104)))),
        QueueError::ResourceChanged
    );
    assert_eq!(snapshot(&queue), before);
    queue.begin_send(&current, ready()).unwrap();
    let before = snapshot(&queue);
    assert_eq!(
        error(queue.prepare(DispatchReadiness::Blocked)),
        QueueError::Busy
    );
    assert_eq!(snapshot(&queue), before);
}

#[test]
fn cl112_api04_resume_complete_noop_preserves_prepared_token() {
    let mut normal = q();
    queued(&mut normal, X);
    let t = normal.prepare(ready()).unwrap().unwrap();
    let before = snapshot(&normal);
    assert_eq!(normal.resume(), 0);
    assert_eq!(snapshot(&normal), before);
    normal.begin_send(&t, ready()).unwrap();
    let mut failed = suspend(&[X, Y]);
    queued(&mut failed, D);
    let old = failed.prepare(ready()).unwrap().unwrap();
    assert_eq!(failed.resume(), 2);
    assert_eq!(
        error(failed.begin_send(&old, ready())),
        QueueError::StaleToken
    );
    let current = failed.prepare(ready()).unwrap().unwrap();
    let before = snapshot(&failed);
    assert_eq!(failed.resume(), 0);
    assert_eq!(snapshot(&failed), before);
    failed.begin_send(&current, ready()).unwrap();
    assert_eq!(failed.active().unwrap().id(), id(D));
    let mut empty = suspend(&[]);
    assert!(empty.failure_suspended());
    assert_eq!(empty.resume(), 0);
    assert!(!empty.failure_suspended());
}

#[test]
fn cl099_api07_explicit_zero_send_settle_returns_content_without_requeue() {
    let mut queue = suspend(&[X, Y, Z]);
    let token = direct(&mut queue, D);
    reject_submit(&mut queue, E, ready(), QueueError::Full);
    let result = queue
        .settle(&token, QueueDisposition::Advance)
        .unwrap()
        .into_input();
    assert_eq!(result.id, id(D));
    assert_eq!(result.body, body(D));
    assert_eq!(ids(&queue), vec![id(X), id(Y), id(Z)]);
    assert!(queue.waiting().all(QueueEntry::suspended));
    assert!(queue.active().is_none());
    assert!(queue.prepare(ready()).unwrap().is_none());
    let new = direct(&mut queue, E);
    let before = snapshot(&queue);
    assert_eq!(error(queue.dispatch_input(&token)), QueueError::StaleToken);
    assert_eq!(
        error(queue.settle(&token, QueueDisposition::Advance)),
        QueueError::StaleToken
    );
    assert_eq!(snapshot(&queue), before);
    assert_eq!(queue.dispatch_input(&new).unwrap().id(), id(E));
    let mut normal = q();
    queued(&mut normal, X);
    queued(&mut normal, Y);
    let token = start(&mut normal);
    let result = normal
        .settle(&token, QueueDisposition::Advance)
        .unwrap()
        .into_input();
    assert_eq!(result.id, id(X));
    assert_eq!(ids(&normal), vec![id(Y)]);
    assert!(normal.active().is_none());
}

#[test]
fn cl095_api03_debug_redacts_body_without_requiring_debug_generic_parameter() {
    const CANARY: &str = "Q3_SYNTHETIC_SECRET_BODY_CANARY";
    let original = QueueInput {
        id: id(X),
        submitter: id(10),
        body: payload(CANARY),
    };
    assert!(!format!("{original:?}").contains(CANARY));
    let rejected = Rejected {
        error: QueueError::Full,
        value: payload(CANARY),
    };
    assert!(!format!("{rejected:?}").contains(CANARY));
    let mut queue = q();
    queued(&mut queue, X);
    let token = queue.prepare(ready()).unwrap().unwrap();
    assert!(!format!("{token:?}").contains(CANARY));
    assert!(!format!("{:?}", QueueError::Full).contains(CANARY));
}
