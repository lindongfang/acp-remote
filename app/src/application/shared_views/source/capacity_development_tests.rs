use super::*;

#[derive(Clone)]
struct Raw;
impl RetainedRaw for Raw {
    fn len(&self) -> usize {
        0
    }
    fn bytes(&self) -> &[u8] {
        &[]
    }
}
struct Projector;
impl ViewProjector<Raw> for Projector {
    fn inspect(&self, _: &Raw) -> ProjectionEnvelope {
        ProjectionEnvelope::unknown(None)
    }
}
struct Fixture {
    owner: SharedViewOwner<(), Raw>,
    pending: PendingViewWorker<(), Raw>,
    producer: super::super::ingress::SourceProducer<(), Raw>,
    key: NativeSessionKey,
}
fn fixture() -> Fixture {
    let source = NativeSourceId(EntityId::from_bytes([7; 16]));
    let key = NativeSessionKey::from_config(Some(source.0), "original").unwrap();
    let (producer, consumer, probe) = super::super::ingress::source_ingress(());
    let (owner, worker) = SharedViewOwner::prepare_source(
        (),
        source,
        consumer,
        probe,
        Arc::new(Projector),
        NonZeroUsize::new(1).unwrap(),
        Handle::current(),
    )
    .unwrap();
    Fixture {
        owner,
        pending: worker,
        producer,
        key,
    }
}

#[tokio::test]
async fn delayed_restore_markers_keep_their_original_closed_generation() {
    let Fixture {
        owner,
        mut pending,
        mut producer,
        key,
    } = fixture();
    let views = owner.handle();
    // The owned worker has not run: both admitted markers are queued before detach.
    views.core.send.seal_target(&key).unwrap();
    let first = views.core.send.restore_target(&key).unwrap().unwrap();
    producer.restore_route(key.clone(), first).unwrap();
    let read = ReadEpoch::fresh().unwrap();
    let rpc = super::super::read_control::RpcScalar::Integer(1);
    let prefix = views
        .core
        .probe
        .track_prefix(super::super::read_prefix::ReadPrefixIdentity {
            rpc: rpc.clone(),
            session: key.native_id().into(),
            cell: 1,
        })
        .unwrap();
    producer
        .bind_read_route(key.clone(), Some(read), Some(rpc))
        .unwrap();
    let context = super::super::read_observation::ReadObservation::unavailable();
    producer.retain(&(), 1, 1, Raw, context.clone()).unwrap();
    prefix.response(1);
    producer
        .complete_read(key.clone(), read, 1, 1, prefix)
        .unwrap();
    views.core.send.seal_target(&key).unwrap();
    let second = views.core.send.restore_target(&key).unwrap().unwrap();
    producer.restore_route(key.clone(), second).unwrap();
    let mut entries = pending.worker.consumer.detach().unwrap().into_iter();
    let SourceEntry::Bind(first) = entries.next().unwrap() else {
        panic!("first marker")
    };
    pending.worker.bind(first).unwrap();
    let stale = views.core.target(&key).unwrap();
    assert!(!stale.send.current().unwrap());
    let SourceEntry::Bind(old_read) = entries.next().unwrap() else {
        panic!("old replay marker")
    };
    pending.worker.bind(old_read).unwrap();
    assert!(
        !pending
            .worker
            .targets
            .get(&key)
            .unwrap()
            .candidate
            .as_ref()
            .unwrap()
            .published
            .send
            .current()
            .unwrap()
    );
    let SourceEntry::Raw(frame) = entries.next().unwrap() else {
        panic!("old response fixture")
    };
    pending.worker.record(frame).unwrap();
    pending.worker.consumer.processed(1, Ok(()), &context);
    let SourceEntry::ReadComplete(completion) = entries.next().unwrap() else {
        panic!("old completion")
    };
    pending.worker.complete(completion).unwrap();
    assert!(!views.core.target(&key).unwrap().send.current().unwrap());
    let SourceEntry::Bind(second) = entries.next().unwrap() else {
        panic!("second marker")
    };
    pending.worker.bind(second).unwrap();
    assert!(views.core.target(&key).unwrap().send.current().unwrap());
    assert!(!stale.send.current().unwrap());
    assert!(entries.next().is_none());
}

#[cfg(windows)]
#[tokio::test]
async fn restore_worker_failure_seals_its_admitted_flag_and_exposes_route_failure() {
    use super::super::controller::{HookSelector, ViewFalliblePoint};
    let Fixture {
        owner,
        mut pending,
        mut producer,
        key,
    } = fixture();
    let views = owner.handle();
    views.core.send.seal_target(&key).unwrap();
    let restored = views.core.send.restore_target(&key).unwrap().unwrap();
    let send = restored
        .view(&views.core.send, ViewEpoch::fresh().unwrap())
        .unwrap();
    producer.restore_route(key.clone(), restored).unwrap();
    let _fault = owner
        .test_controller()
        .fail_next(
            ViewFalliblePoint::NewViewEpochRandom,
            HookSelector::Target(key),
        )
        .unwrap();
    let SourceEntry::Bind(marker) = pending
        .worker
        .consumer
        .detach()
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
    else {
        panic!("restore marker")
    };
    let error = pending.worker.bind(marker).unwrap_err();
    // This is the actual owned worker's failure publication path.
    pending.worker.consumer.route_failed(error);
    assert_eq!(views.core.probe.route_failure(), Some(error));
    assert!(!send.current().unwrap());
}

#[tokio::test]
async fn hn_030_publication_takes_send_fence_before_candidate_write() {
    use std::sync::TryLockError;
    use std::time::{Duration, Instant};

    let Fixture {
        owner: _owner,
        producer: _producer,
        mut pending,
        key,
    } = fixture();
    let worker = &mut pending.worker;
    let current = worker.generation(&key, None, None, None).unwrap();
    worker.install_target(&key, current).unwrap();
    let next = worker
        .generation(&key, Some(ReadEpoch::fresh().unwrap()), None, None)
        .unwrap();
    let target = worker.targets.get(&key).unwrap();
    let old = &target.current.published;
    let replaced = old.replaced.subscribe();
    let candidate_reader = target.published.candidate.read().unwrap();
    std::thread::scope(|scope| {
        let publisher = scope.spawn(|| Worker::<(), Raw>::replace_generation(target, &next));
        // Only the publisher can own the send fence. Release the candidate lock
        // even on timeout, so a lock-order regression cannot strand the worker.
        let deadline = Instant::now() + Duration::from_secs(5);
        let fenced = loop {
            if matches!(
                old.send.group.inner.try_lock(),
                Err(TryLockError::WouldBlock)
            ) {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        drop(candidate_reader);
        publisher.join().unwrap().unwrap();
        assert!(
            fenced,
            "publisher must own send fence while waiting for candidate"
        );
    });
    assert!(*replaced.borrow());
    assert!(!old.send.current().unwrap());
    assert!(Arc::ptr_eq(
        &target.published.current.read().unwrap(),
        &next.published
    ));
}
