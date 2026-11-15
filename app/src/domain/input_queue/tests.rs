use super::*;
use crate::domain::native::{HarnessConnectionEpoch, LoadEpoch, RuntimeEpoch};

fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
fn key() -> NativeSessionKey {
    NativeSessionKey::from_config(Some(id(1)), "queue-unit").unwrap()
}
fn ready() -> DispatchReadiness {
    DispatchReadiness::Ready(ResourceEpoch {
        runtime: RuntimeEpoch(id(2)),
        connection: HarnessConnectionEpoch(id(3)),
        load: LoadEpoch(id(4)),
    })
}
fn input(n: u8) -> QueueInput<String> {
    QueueInput {
        id: id(n),
        submitter: id(10),
        body: format!("body-{n}"),
    }
}
fn queued(q: &mut InputQueue<String>, n: u8) {
    assert!(matches!(
        q.submit(input(n), DispatchReadiness::Blocked),
        Ok(Submission::Queued { revision: 1, .. })
    ));
}
fn start(q: &mut InputQueue<String>) -> DispatchToken {
    let token = q.prepare(ready()).unwrap().unwrap();
    q.begin_send(&token, ready()).unwrap();
    token
}

#[test]
fn suspend_waiting_preserves_active_and_invalidates_prepared_selection() {
    let mut q = InputQueue::new(key());
    queued(&mut q, 20);
    queued(&mut q, 21);
    queued(&mut q, 22);
    q.prioritize(id(21), 1).unwrap();
    let old = q.prepare(ready()).unwrap().unwrap();
    assert_eq!(q.suspend_waiting(), 3);
    assert_eq!(q.suspend_waiting(), 0);
    assert_eq!(q.begin_send(&old, ready()), Err(QueueError::StaleToken));
    assert_eq!(q.resume(), 3);
    let active = start(&mut q);
    assert_eq!(q.dispatch_input(&active).unwrap().id(), id(20));
    assert_eq!(q.suspend_waiting(), 2);
    assert_eq!(q.dispatch_input(&active).unwrap().input().body, "body-20");
    assert_eq!(q.prepare(ready()).unwrap_err(), QueueError::Busy);
    q.settle(&active, QueueDisposition::Advance).unwrap();
    assert!(q.waiting().all(QueueEntry::suspended));
}
fn suspended(items: &[u8]) -> InputQueue<String> {
    let mut q = InputQueue::new(key());
    queued(&mut q, 20);
    let old = start(&mut q);
    for n in items {
        queued(&mut q, *n);
    }
    q.settle(&old, QueueDisposition::Suspend).unwrap();
    q
}
fn ids(q: &InputQueue<String>) -> Vec<EntityId> {
    q.waiting().map(QueueEntry::id).collect()
}

#[test]
fn prepare_keeps_capacity_and_cancel_edit_invalidate_old_selection() {
    let mut q = InputQueue::new(key());
    queued(&mut q, 21);
    queued(&mut q, 22);
    let first = q.prepare(ready()).unwrap().unwrap();
    queued(&mut q, 23);
    assert_eq!(
        q.submit(input(24), ready()).unwrap_err().error,
        QueueError::Full
    );
    assert_eq!(q.waiting().len(), 3);
    assert!(q.active().is_none());
    assert_eq!(
        q.edit(id(21), 1, id(11), "edited".into()).unwrap(),
        "body-21"
    );
    assert_eq!(q.begin_send(&first, ready()), Err(QueueError::StaleToken));
    let second = q.prepare(ready()).unwrap().unwrap();
    assert_eq!(q.cancel(id(21), 2).unwrap().into_input().body, "edited");
    assert_eq!(q.begin_send(&second, ready()), Err(QueueError::StaleToken));
    assert_eq!(ids(&q), vec![id(22), id(23)]);
}

#[test]
fn begin_wins_race_and_stale_token_cannot_settle_new_input() {
    let mut q = InputQueue::new(key());
    queued(&mut q, 21);
    queued(&mut q, 22);
    let first = start(&mut q);
    assert_eq!(q.cancel(id(21), 0).err(), Some(QueueError::DispatchStarted));
    assert_eq!(
        q.edit(id(21), 0, id(11), "replacement".into())
            .unwrap_err()
            .error,
        QueueError::DispatchStarted
    );
    assert_eq!(q.dispatch_input(&first).unwrap().input().body, "body-21");
    q.settle(&first, QueueDisposition::Advance).unwrap();
    let second = start(&mut q);
    assert_eq!(
        q.settle(&first, QueueDisposition::Suspend).err(),
        Some(QueueError::StaleToken)
    );
    assert_eq!(q.dispatch_input(&second).unwrap().id(), id(22));
    assert!(!q.failure_suspended());
}

#[test]
fn three_suspended_allow_exactly_one_direct_and_success_does_not_resume() {
    let mut q = suspended(&[21, 22, 23]);
    let Submission::Dispatching(d) = q.submit(input(24), ready()).unwrap() else {
        panic!("direct");
    };
    assert_eq!(ids(&q), vec![id(21), id(22), id(23)]);
    assert!(q.waiting().all(QueueEntry::suspended));
    assert_eq!(
        q.submit(input(25), ready()).unwrap_err().error,
        QueueError::Full
    );
    q.settle(&d, QueueDisposition::Advance).unwrap();
    assert!(q.prepare(ready()).unwrap().is_none());
    assert!(matches!(
        q.submit(input(25), ready()),
        Ok(Submission::Dispatching(_))
    ));
}

#[test]
fn resume_joins_old_after_waiting_new_without_preempting_active() {
    let mut q = suspended(&[21, 22]);
    queued(&mut q, 24);
    let old_preparation = q.prepare(ready()).unwrap().unwrap();
    assert_eq!(q.resume(), 2);
    assert_eq!(ids(&q), vec![id(24), id(21), id(22)]);
    assert_eq!(
        q.begin_send(&old_preparation, ready()),
        Err(QueueError::StaleToken)
    );
    let direct = start(&mut q);
    assert_eq!(q.dispatch_input(&direct).unwrap().id(), id(24));
    assert_eq!(q.resume(), 0);
    assert_eq!(q.prepare(ready()).err(), Some(QueueError::Busy));
    q.settle(&direct, QueueDisposition::Advance).unwrap();
    let next = q.prepare(ready()).unwrap().unwrap();
    assert_eq!(q.resume(), 0);
    q.begin_send(&next, ready()).unwrap();
    assert_eq!(q.dispatch_input(&next).unwrap().id(), id(21));
}

#[test]
fn priority_preserves_others_and_never_resumes_suspended_items() {
    let mut q = suspended(&[21, 22]);
    assert_eq!(q.prioritize(id(22), 1), Err(QueueError::Suspended));
    q.resume();
    queued(&mut q, 23);
    let old = q.prepare(ready()).unwrap().unwrap();
    q.prioritize(id(22), 1).unwrap();
    assert_eq!(q.begin_send(&old, ready()), Err(QueueError::StaleToken));
    q.edit(id(21), 1, id(11), "other-edited".into()).unwrap();
    let selected = start(&mut q);
    assert_eq!(q.dispatch_input(&selected).unwrap().id(), id(22));
    assert_eq!(ids(&q), vec![id(21), id(23)]);
}

#[test]
fn selected_priority_terminal_never_restores_other_paused_items() {
    for (disposition, terminal_reason) in [
        (QueueDisposition::Advance, None),
        (
            QueueDisposition::Suspend,
            Some(crate::domain::native_dispatch::KnownStopReason::MaxTokens),
        ),
        (
            QueueDisposition::Advance,
            Some(crate::domain::native_dispatch::KnownStopReason::Cancelled),
        ),
    ] {
        let mut q = InputQueue::new(key());
        queued(&mut q, 21);
        queued(&mut q, 22);
        queued(&mut q, 23);
        q.prioritize(id(22), 1).unwrap();
        q.suspend_except_priority_with_reason(
            id(22),
            1,
            crate::domain::native_dispatch::KnownStopReason::Cancelled,
        );
        let selected = start(&mut q);
        assert_eq!(q.dispatch_input(&selected).unwrap().id(), id(22));
        q.settle(&selected, disposition).unwrap();
        if let Some(reason) = terminal_reason {
            q.suspend_waiting_with_reason(reason);
        }

        assert!(q.failure_suspended());
        assert_eq!(ids(&q), vec![id(21), id(23)]);
        assert!(q.waiting().all(QueueEntry::suspended));
        assert!(q.prepare(ready()).unwrap().is_none());
    }
}

#[test]
fn selected_priority_loss_pauses_remaining_items_without_fallback() {
    let mut q = InputQueue::new(key());
    queued(&mut q, 21);
    queued(&mut q, 22);
    queued(&mut q, 23);
    q.prioritize(id(22), 1).unwrap();
    q.edit(id(22), 1, id(11), "edited-after-priority".into())
        .unwrap();

    assert_eq!(
        q.priority_failure(),
        Some(PriorityFailure::SelectionChanged)
    );
    assert_eq!(q.last_stop_reason(), None);
    assert!(q.failure_suspended());
    assert_eq!(q.priority(), None);
    assert!(q.waiting().all(QueueEntry::suspended));
    assert!(q.prepare(ready()).unwrap().is_none());
    assert_eq!(q.resume(), 3);
    assert_eq!(ids(&q), vec![id(21), id(22), id(23)]);

    q.prioritize(id(22), 2).unwrap();
    q.cancel(id(22), 2).unwrap();
    assert_eq!(
        q.priority_failure(),
        Some(PriorityFailure::SelectionChanged)
    );
    assert!(q.failure_suspended());
    assert!(q.prepare(ready()).unwrap().is_none());
    assert_eq!(q.resume(), 2);
    assert_eq!(ids(&q), vec![id(21), id(23)]);
}

#[test]
fn resource_change_and_foreign_instance_cannot_consume_preparation() {
    let mut q = InputQueue::new(key());
    queued(&mut q, 21);
    let token = q.prepare(ready()).unwrap().unwrap();
    let mut other = InputQueue::<String>::new(key());
    assert_eq!(
        other.begin_send(&token, ready()),
        Err(QueueError::StaleToken)
    );
    assert_eq!(
        q.begin_send(&token, DispatchReadiness::Blocked),
        Err(QueueError::Blocked)
    );
    let DispatchReadiness::Ready(mut epoch) = ready() else {
        unreachable!();
    };
    epoch.load = LoadEpoch(id(5));
    assert_eq!(
        q.begin_send(&token, DispatchReadiness::Ready(epoch)),
        Err(QueueError::ResourceChanged)
    );
    q.begin_send(&token, ready()).unwrap();
}

#[test]
fn revision_exhaustion_preserves_body_editor_and_existing_preparation() {
    let mut q = InputQueue::new(key());
    queued(&mut q, 21);
    q.waiting[0].revision = u64::MAX;
    let token = q.prepare(ready()).unwrap().unwrap();
    let rejected = q
        .edit(id(21), u64::MAX, id(11), "new-body".into())
        .unwrap_err();
    assert_eq!(rejected.error, QueueError::RevisionExhausted);
    assert_eq!(rejected.value, "new-body");
    assert_eq!(q.waiting[0].input.body, "body-21");
    assert_eq!(q.waiting[0].editor, id(10));
    q.begin_send(&token, ready()).unwrap();
    assert_eq!(q.dispatch_input(&token).unwrap().revision(), u64::MAX);
}

#[test]
fn sequence_exhaustion_preserves_waiting_and_never_wraps_or_reuses_token() {
    let mut q = InputQueue::new(key());
    queued(&mut q, 21);
    q.serial = u64::MAX - 1;
    let last = q.prepare(ready()).unwrap().unwrap();
    assert_eq!(last.serial, u64::MAX);
    q.release_preparation(&last).unwrap();
    assert_eq!(
        q.prepare(ready()).err(),
        Some(QueueError::SequenceExhausted)
    );
    assert_eq!(ids(&q), vec![id(21)]);
    assert_eq!(q.serial, u64::MAX);
    assert_eq!(q.begin_send(&last, ready()), Err(QueueError::StaleToken));

    let mut q = suspended(&[21, 22, 23]);
    q.serial = u64::MAX;
    let rejected = q.submit(input(24), ready()).unwrap_err();
    assert_eq!(rejected.error, QueueError::SequenceExhausted);
    assert_eq!(rejected.value.body, "body-24");
    assert!(q.active().is_none());
    assert!(q.waiting().all(QueueEntry::suspended));
    assert_eq!(ids(&q), vec![id(21), id(22), id(23)]);
}

#[test]
fn editing_other_item_keeps_preparation_and_empty_failure_resume_invalidates_it() {
    let mut q = InputQueue::new(key());
    queued(&mut q, 21);
    queued(&mut q, 22);
    let token = q.prepare(ready()).unwrap().unwrap();
    q.edit(id(22), 1, id(11), "other".into()).unwrap();
    q.begin_send(&token, ready()).unwrap();
    assert_eq!(q.dispatch_input(&token).unwrap().id(), id(21));

    let mut q = suspended(&[]);
    queued(&mut q, 24);
    let token = q.prepare(ready()).unwrap().unwrap();
    assert!(q.failure_suspended());
    assert_eq!(q.resume(), 0);
    assert!(!q.failure_suspended());
    assert_eq!(q.begin_send(&token, ready()), Err(QueueError::StaleToken));
    assert_eq!(ids(&q), vec![id(24)]);
    let token = q.prepare(ready()).unwrap().unwrap();
    assert_eq!(q.resume(), 0);
    q.begin_send(&token, ready()).unwrap();
}
