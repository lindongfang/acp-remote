use super::*;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_before_and_during_wait_never_loses_wakeup() {
    for _ in 0..1000 {
        let cancel = Cancellation::default();
        let sender = cancel.clone();
        let task = tokio::spawn(async move {
            tokio::task::yield_now().await;
            sender.cancel();
        });
        timeout_at(
            Instant::now() + Duration::from_millis(200),
            cancel.cancelled(),
        )
        .await
        .unwrap();
        task.await.unwrap();
    }
    let cancel = Cancellation::default();
    cancel.cancel();
    timeout_at(
        Instant::now() + Duration::from_millis(20),
        cancel.cancelled(),
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn cancellation_wakes_all_registered_waiters() {
    let cancel = Cancellation::default();
    let mut waiters = Vec::new();
    for _ in 0..32 {
        let next = cancel.clone();
        waiters.push(tokio::spawn(async move {
            next.cancelled().await;
        }));
    }
    tokio::task::yield_now().await;
    cancel.cancel();
    for waiter in waiters {
        tokio::time::timeout(Duration::from_millis(200), waiter)
            .await
            .unwrap()
            .unwrap();
    }
}
struct Structure;
impl InputStructureValidator for Structure {
    fn validate(&self, _body: &[u8]) -> Result<(), Error> {
        Ok(())
    }
}
#[test]
fn borrowed_bytes_stay_accounted_until_the_writer_releases_them() {
    let inputs = MemoryInputs::default();
    let mut upload = inputs
        .reserve(3, 0, Instant::now() + Duration::from_secs(2))
        .unwrap();
    upload.append(b"abc").unwrap();
    let input = upload.finish(&Structure).unwrap();
    let borrowed = input.borrow_once().unwrap();
    inputs.stop();
    assert!(borrowed.cell.cancel.is_cancelled());
    assert_eq!(inputs.retained(), (3, 1));
    drop(borrowed);
    assert_eq!(inputs.retained(), (0, 0));
}
#[test]
fn invalid_append_retires_owned_body_without_waiting_for_handle_drop() {
    let inputs = MemoryInputs::default();
    let mut upload = inputs
        .reserve(1, 0, Instant::now() + Duration::from_secs(2))
        .unwrap();
    assert_eq!(upload.append(b"abc"), Err(Error::InvalidInput));
    assert_eq!(inputs.retained(), (0, 0));
}
#[test]
fn closed_digest_matches_literal_unicode_and_escape_vector() {
    use crate::domain::native::{
        HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch,
    };
    let id = |n| EntityId::from_bytes([n; 16]);
    let target = InputTarget {
        target: id(10),
        native: NativeSessionKey::from_config(Some(id(2)), "native\"\\\n\u{e9}\u{301}").unwrap(),
        epoch: ResourceEpoch {
            runtime: RuntimeEpoch(id(3)),
            connection: HarnessConnectionEpoch(id(4)),
            load: LoadEpoch(id(5)),
        },
        instance: id(6),
        revision: 0,
    };
    let digest = Sha256::digest(b"synthetic input").into();
    assert_eq!(
        hex::encode(input_intent(id(1), &target, digest).unwrap().payload_digest),
        "a7a26da8aa1aae6717770d95c8ea0dba911a1dfaf33e9c39cb77b8c4154b0a0b"
    );
    let mut changed = target.clone();
    changed.native =
        NativeSessionKey::from_config(Some(id(2)), "native\"\\\ne\u{301}\u{301}").unwrap();
    assert_ne!(
        input_intent(id(1), &target, digest).unwrap(),
        input_intent(id(1), &changed, digest).unwrap()
    );
}
#[tokio::test]
async fn mgt_code_001_deadline_only_tightens_and_supervisor_retires_without_a_reader_call() {
    let inputs = MemoryInputs::default();
    let mut upload = inputs
        .reserve(3, 0, Instant::now() + Duration::from_secs(2))
        .unwrap();
    upload.append(b"abc").unwrap();
    let input = upload.finish(&Structure).unwrap();
    let first = Instant::now() + Duration::from_millis(30);
    assert_eq!(input.cell.tighten_deadline(first).unwrap(), first);
    assert_eq!(
        input
            .cell
            .tighten_deadline(Instant::now() + Duration::from_secs(2))
            .unwrap(),
        first
    );
    let supervisor = inputs.supervise();
    tokio::time::sleep(Duration::from_millis(180)).await;
    {
        let state = input.cell.state.lock().unwrap();
        assert!(state.retired);
        assert!(state.body.is_none());
        assert!(!state.accounted);
        drop(state);
    }
    supervisor.shutdown().await;
}
#[tokio::test]
async fn mgt_code_003_object_lifecycle_errors_do_not_claim_signed_request_expiry() {
    let inputs = MemoryInputs::default();
    let mut upload = inputs
        .reserve(1, 0, Instant::now() + Duration::from_millis(20))
        .unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(upload.append(b"x"), Err(Error::InputExpired));
    assert!(matches!(
        upload.finish(&Structure),
        Err(Error::InputExpired)
    ));
    let mut upload = inputs
        .reserve(1, 0, Instant::now() + Duration::from_millis(20))
        .unwrap();
    upload.append(b"x").unwrap();
    let payload = upload.finish(&Structure).unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(payload.digest(), Err(Error::InputExpired));
    assert_eq!(
        payload
            .cell
            .tighten_deadline(Instant::now() + Duration::from_secs(1)),
        Err(Error::InputExpired)
    );
    assert!(matches!(payload.borrow_once(), Err(Error::InputExpired)));
    assert!(!Error::InputExpired.to_string().contains("product request"));
    assert_eq!(inputs.retained(), (0, 0));
}
