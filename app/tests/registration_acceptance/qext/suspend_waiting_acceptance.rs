//! Independent public L3 QEXT cases, frozen before product implementation.
//! Calling readiness/settlement here is a synthetic domain fact, not native evidence.
use acp_remote::domain::{
    EntityId,
    input_queue::{
        DispatchReadiness, DispatchToken, InputQueue, QueueDisposition, QueueEntry, QueueError,
        QueueInput, Submission,
    },
    native::{HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch},
};

const fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
fn key() -> NativeSessionKey {
    NativeSessionKey::from_config(Some(id(1)), "qext-synthetic").unwrap()
}
const fn ready() -> DispatchReadiness {
    DispatchReadiness::Ready(ResourceEpoch {
        runtime: RuntimeEpoch(id(101)),
        connection: HarnessConnectionEpoch(id(102)),
        load: LoadEpoch(id(103)),
    })
}
fn input(n: u8) -> QueueInput<String> {
    QueueInput {
        id: id(n),
        submitter: id(10),
        body: format!("body-{n}"),
    }
}
fn queued(queue: &mut InputQueue<String>, n: u8) {
    assert!(matches!(
        queue.submit(input(n), DispatchReadiness::Blocked).unwrap(),
        Submission::Queued { id: item, revision: 1 } if item == id(n)
    ));
}
fn start(queue: &mut InputQueue<String>) -> DispatchToken {
    let token = queue.prepare(ready()).unwrap().unwrap();
    queue.begin_send(&token, ready()).unwrap();
    token
}
fn error<T>(result: Result<T, QueueError>) -> QueueError {
    result.err().expect("expected rejected operation")
}
type EntrySnapshot = (EntityId, u64, EntityId, EntityId, String, bool);
fn entry(e: &QueueEntry<String>) -> EntrySnapshot {
    (
        e.id(),
        e.revision(),
        e.input().submitter,
        e.editor(),
        e.input().body.clone(),
        e.suspended(),
    )
}
fn waiting(queue: &InputQueue<String>) -> Vec<EntrySnapshot> {
    queue.waiting().map(entry).collect()
}

#[test]
fn reg_qext_01_suspend_waiting_preserves_every_non_suspension_field() {
    let mut queue = InputQueue::new(key());
    for n in [21, 22, 23] {
        queued(&mut queue, n);
    }
    queue.edit(id(22), 1, id(11), "edited-Y".into()).unwrap();
    let mut expected = waiting(&queue);
    for item in &mut expected {
        item.5 = true;
    }
    assert_eq!(queue.suspend_waiting(), 3);
    assert_eq!(waiting(&queue), expected);
    assert!(queue.failure_suspended());
    assert!(queue.active().is_none());
    assert!(queue.prepare(ready()).unwrap().is_none());
}

#[test]
fn reg_qext_02_counts_only_new_suspensions_and_empty_still_enters_failure_mode() {
    let mut queue = InputQueue::new(key());
    for n in [21, 22, 23] {
        queued(&mut queue, n);
    }
    assert_eq!(queue.suspend_waiting(), 3);
    let before = waiting(&queue);
    assert_eq!(queue.suspend_waiting(), 0);
    assert_eq!(waiting(&queue), before);
    assert_eq!(queue.resume(), 3);
    assert_eq!(queue.suspend_waiting(), 3);
    assert_eq!(waiting(&queue), before);
    let mut empty: InputQueue<String> = InputQueue::new(key());
    assert_eq!(empty.suspend_waiting(), 0);
    assert!(empty.failure_suspended());
    assert!(empty.active().is_none());
    assert_eq!(empty.waiting().len(), 0);
    assert_eq!(empty.resume(), 0);
    assert!(!empty.failure_suspended());
}

#[test]
fn reg_qext_03_suspension_invalidates_preparation_and_clears_priority() {
    let mut queue = InputQueue::new(key());
    for n in [21, 22, 23] {
        queued(&mut queue, n);
    }
    queue.prioritize(id(22), 1).unwrap();
    let old = queue.prepare(ready()).unwrap().unwrap();
    assert_eq!(queue.suspend_waiting(), 3);
    let before = waiting(&queue);
    assert_eq!(
        error(queue.begin_send(&old, ready())),
        QueueError::StaleToken
    );
    assert_eq!(
        error(queue.release_preparation(&old)),
        QueueError::StaleToken
    );
    assert_eq!(waiting(&queue), before);
    assert_eq!(queue.resume(), 3);
    let current = queue.prepare(ready()).unwrap().unwrap();
    assert_eq!(
        error(queue.release_preparation(&old)),
        QueueError::StaleToken
    );
    queue.begin_send(&current, ready()).unwrap();
    assert_eq!(queue.dispatch_input(&current).unwrap().id(), id(21));
    assert_eq!(
        queue.waiting().map(QueueEntry::id).collect::<Vec<_>>(),
        vec![id(22), id(23)]
    );
}

#[test]
fn reg_qext_04_suspending_waiting_does_not_settle_or_replace_active_owner() {
    let mut queue = InputQueue::new(key());
    queued(&mut queue, 24);
    let active = start(&mut queue);
    for n in [21, 22] {
        queued(&mut queue, n);
    }
    let original = entry(queue.dispatch_input(&active).unwrap());
    assert_eq!(queue.suspend_waiting(), 2);
    assert_eq!(entry(queue.dispatch_input(&active).unwrap()), original);
    assert_eq!(entry(queue.active().unwrap()), original);
    assert_eq!(error(queue.prepare(ready())), QueueError::Busy);
    assert!(matches!(
        queue.submit(input(25), ready()).unwrap(),
        Submission::Queued { .. }
    ));
    assert_eq!(entry(queue.dispatch_input(&active).unwrap()), original);
    assert!(!queue.waiting().last().unwrap().suspended());
    assert_eq!(queue.waiting().len(), 3);
}

#[test]
fn reg_qext_05_later_new_input_runs_without_implicitly_resuming_old_items() {
    let mut queue = InputQueue::new(key());
    for n in [21, 22] {
        queued(&mut queue, n);
    }
    assert_eq!(queue.suspend_waiting(), 2);
    queued(&mut queue, 24);
    queue.edit(id(21), 1, id(11), "X-by-B".into()).unwrap();
    let before = waiting(&queue);
    assert_eq!(error(queue.prioritize(id(21), 2)), QueueError::Suspended);
    assert_eq!(waiting(&queue), before);
    let fresh = start(&mut queue);
    assert_eq!(queue.dispatch_input(&fresh).unwrap().id(), id(24));
    queue.settle(&fresh, QueueDisposition::Advance).unwrap();
    assert_eq!(
        queue.waiting().map(QueueEntry::id).collect::<Vec<_>>(),
        vec![id(21), id(22)]
    );
    assert!(queue.waiting().all(QueueEntry::suspended));
    assert!(queue.failure_suspended());
    assert!(queue.prepare(ready()).unwrap().is_none());
    queued(&mut queue, 25);
    assert_eq!(queue.suspend_waiting(), 1);
    assert_eq!(queue.suspend_waiting(), 0);
}

#[test]
fn reg_qext_06_settle_suspend_keeps_original_token_single_use_semantics() {
    let mut queue = InputQueue::new(key());
    queued(&mut queue, 24);
    let prior = start(&mut queue);
    for n in [21, 22, 23] {
        queued(&mut queue, n);
    }
    let moved = queue
        .settle(&prior, QueueDisposition::Suspend)
        .unwrap()
        .into_input();
    assert_eq!(moved.id, id(24));
    assert_eq!(moved.body, "body-24");
    assert!(queue.active().is_none());
    assert!(queue.waiting().all(QueueEntry::suspended));
    assert_eq!(queue.suspend_waiting(), 0);
    assert_eq!(
        error(queue.settle(&prior, QueueDisposition::Advance)),
        QueueError::StaleToken
    );
    assert_eq!(queue.resume(), 3);
    let current = start(&mut queue);
    let before = entry(queue.dispatch_input(&current).unwrap());
    assert_eq!(
        error(queue.settle(&prior, QueueDisposition::Suspend)),
        QueueError::StaleToken
    );
    assert_eq!(entry(queue.dispatch_input(&current).unwrap()), before);
}

struct StructuredBody {
    marker: String,
    structure: Vec<(String, Vec<u8>)>,
}
fn check_body(body: &StructuredBody) {
    assert_eq!(body.marker, "synthetic-非BMP-😀\nmarker");
    assert_eq!(
        body.structure,
        vec![("未知-extension".into(), vec![0, 255, 13, 10])]
    );
}

#[test]
fn reg_qext_07_nonclone_nondebug_structured_body_survives_suspend_resume_move() {
    let mut queue = InputQueue::new(key());
    let body = StructuredBody {
        marker: "synthetic-非BMP-😀\nmarker".into(),
        structure: vec![("未知-extension".into(), vec![0, 255, 13, 10])],
    };
    queue
        .submit(
            QueueInput {
                id: id(21),
                submitter: id(10),
                body,
            },
            DispatchReadiness::Blocked,
        )
        .unwrap();
    let preparation = queue.prepare(ready()).unwrap().unwrap();
    assert_eq!(queue.suspend_waiting(), 1);
    assert_eq!(queue.waiting().len(), 1);
    check_body(&queue.waiting().next().unwrap().input().body);
    assert_eq!(
        error(queue.begin_send(&preparation, ready())),
        QueueError::StaleToken
    );
    assert_eq!(queue.resume(), 1);
    let entry = queue.cancel(id(21), 1).unwrap();
    assert_eq!(entry.input().submitter, id(10));
    assert_eq!(entry.editor(), id(10));
    assert_eq!(entry.revision(), 1);
    check_body(&entry.into_input().body);
    assert_eq!(queue.waiting().len(), 0);
}
