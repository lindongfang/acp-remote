use super::*;

#[derive(Clone)]
struct SlowFinalClock {
    inner: Clock,
    stall: Arc<AtomicBool>,
}
impl TimeSource for SlowFinalClock {
    fn sample(&self) -> Result<RequestTime, Error> {
        if self.stall.swap(false, Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10_300));
            self.inner.set(NOW + 10_300);
        }
        self.inner.sample()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m_operation_timeout_does_not_expire_still_valid_signed_request() {
    let f = Fixture::new().await;
    let stall = Arc::new(AtomicBool::new(false));
    let slow = SlowFinalClock {
        inner: f.clock.clone(),
        stall: stall.clone(),
    };
    // Construct before resources exist; start() is not a fault-reset or recovery shortcut.
    let service = Arc::new(
        ProductInputService::start(f.store.clone(), f.authority.clone(), Codec::new(), slow)
            .await
            .unwrap(),
    );
    let target = f.target(10).await;
    let token = f.token(&target).await;
    let inputs = MemoryInputs::default();
    let (fact, ticket) = f.admitted(&target, &token, &inputs).await;
    let permit = service
        .claim(&f.actor, ticket, &Cancellation::default())
        .await
        .unwrap();
    f.authority.pause.store(true, Ordering::SeqCst);
    let writer = Arc::new(Writer::default());
    let task_writer = writer.clone();
    let actor = f.actor.clone();
    let task = tokio::spawn(async move {
        service
            .dispatch(
                &actor,
                permit,
                task_writer.as_ref(),
                &Cancellation::default(),
            )
            .await
    });
    timeout(Duration::from_secs(2), f.authority.arrived.notified())
        .await
        .unwrap();
    stall.store(true, Ordering::SeqCst);
    f.authority.release.notify_one();
    let result = timeout(Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap();
    let status: String =
        sqlx::query_scalar("SELECT status FROM product_commands WHERE command_id=?")
            .bind(fact.command.encode())
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let epoch = f.store.control().await.unwrap().0;
    let intent = input_intent(f.actor.0.client_id, &target, Sha256::digest(BODY).into()).unwrap();
    assert!(
        Codec::new()
            .verify(&token, epoch, &intent, f.clock.sample().unwrap())
            .is_ok(),
        "signature has nearly 24 hours left"
    );
    assert_eq!(writer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(inputs.retained(), (0, 0));
    f.close().await;
    assert_eq!(
        status, "failed",
        "operation deadline is not the signed request expiry"
    );
    assert!(
        !matches!(result, Err(Error::Expired)),
        "only signed expiry may be reported Expired"
    );
}

#[tokio::test]
async fn m_body_ttl_does_not_expire_still_valid_signed_request() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let token = f.token(&target).await;
    let inputs = MemoryInputs::default();
    let body_ttl = Instant::now() + Duration::from_secs(3);
    let mut upload = inputs.reserve(BODY.len(), 0, body_ttl).unwrap();
    upload.append(BODY).unwrap();
    let body = upload.finish(&Structure).unwrap();
    let admission = f
        .service
        .submit(
            &f.actor,
            target.target,
            &token,
            body,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    let InputAdmission::New { fact, ticket } = admission else {
        panic!("new admission")
    };
    let permit = f
        .service
        .claim(&f.actor, ticket, &Cancellation::default())
        .await
        .unwrap();
    f.authority.pause.store(true, Ordering::SeqCst);
    let writer = Arc::new(Writer::default());
    let task_writer = writer.clone();
    let service = f.service.clone();
    let actor = f.actor.clone();
    let task = tokio::spawn(async move {
        service
            .dispatch(
                &actor,
                permit,
                task_writer.as_ref(),
                &Cancellation::default(),
            )
            .await
    });
    timeout(Duration::from_secs(5), f.authority.arrived.notified())
        .await
        .unwrap();
    tokio::time::sleep_until(body_ttl + Duration::from_millis(200)).await;
    f.authority.release.notify_one();
    let result = task.await.unwrap();
    let status: String =
        sqlx::query_scalar("SELECT status FROM product_commands WHERE command_id=?")
            .bind(fact.command.encode())
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let epoch = f.store.control().await.unwrap().0;
    let intent = input_intent(f.actor.0.client_id, &target, Sha256::digest(BODY).into()).unwrap();
    assert!(
        Codec::new()
            .verify(&token, epoch, &intent, f.clock.sample().unwrap())
            .is_ok()
    );
    assert_eq!(writer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(inputs.retained(), (0, 0));
    f.close().await;
    assert_eq!(status, "failed");
    assert!(
        !matches!(result, Err(Error::Expired)),
        "body TTL cannot be reported as product request expiry"
    );
}
