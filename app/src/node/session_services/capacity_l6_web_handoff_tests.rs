// Development checks only: inert DOM files never establish a browser verdict.

fn capacity_web_test_binding() -> serde_json::Value {
    serde_json::json!({
        "phaseId":"development-phase","phase":"inspect","nodeId":"development-node",
        "sourceId":"development-source","url":"https://localhost:1","nonce":"private-correlation",
        "expected":{"nativeIds":["development-native"],"operationIds":["development-operation"],"occupied":1}
    })
}

fn capacity_web_test_artifacts(
    directory: &std::path::Path,
    binding: &serde_json::Value,
    client: &str,
) {
    let capture = serde_json::to_vec(&serde_json::json!({
        "kind":"independent-browser-dom-capture","binding":binding,"browserClientId":client,
        "dom":{"inertDevelopmentMarker":"not-a-real-browser"}
    }))
    .unwrap();
    let actions = serde_json::to_vec(&serde_json::json!({
        "kind":"independent-browser-actions","binding":binding,"browserClientId":client,
        "actions":["inert-development-only"]
    }))
    .unwrap();
    let source = b"inert development source; never executed";
    std::fs::write(directory.join("capture.json"), &capture).unwrap();
    std::fs::write(directory.join("actions.json"), &actions).unwrap();
    std::fs::write(directory.join("capture-source.txt"), source).unwrap();
    capacity_l6_web_write_new(
        &directory.join("receipt.json"),
        &serde_json::json!({
            "format":1,"binding":binding,"browserClientId":client,
            "captureSha256":capacity_l6_web_sha256(&capture),
            "actionsSha256":capacity_l6_web_sha256(&actions),
            "captureSourceSha256":capacity_l6_web_sha256(source)
        }),
    )
    .unwrap();
}

#[test]
fn capacity_l6_web_expected_rejects_body_and_nonpolicy_fields() {
    let valid = capacity_web_test_binding()["expected"].clone();
    assert!(capacity_l6_web_expected(valid.clone()).is_ok());
    let mut body = valid.clone();
    body["rawBody"] = serde_json::json!("not permitted in stage expectations");
    assert!(capacity_l6_web_expected(body).is_err());
    let mut too_many = valid;
    too_many["occupied"] = serde_json::json!(51);
    assert!(capacity_l6_web_expected(too_many).is_err());
}

#[test]
fn capacity_l6_web_matching_receipt_stays_pending_and_is_consumed_once() {
    let area = TestArea::new();
    let binding = capacity_web_test_binding();
    capacity_web_test_artifacts(&area.directory, &binding, "development-browser");
    let result = capacity_l6_web_collect(&area.directory, &binding, "development-browser").unwrap();
    assert_eq!(result["status"], "pending_independent_review");
    assert_eq!(result["acceptancePassed"], false);
    assert_eq!(result["independentReviewRequired"], true);
    let output = serde_json::to_string(&result).unwrap();
    assert!(!output.contains("private-correlation"));
    assert!(!output.contains("not-a-real-browser"));
    assert!(capacity_l6_web_collect(&area.directory, &binding, "development-browser").is_err());
}

#[test]
fn capacity_l6_web_original_nonce_source_client_and_hash_are_required() {
    let area = TestArea::new();
    let binding = capacity_web_test_binding();
    capacity_web_test_artifacts(&area.directory, &binding, "development-browser");
    for key in ["nonce", "sourceId", "nodeId", "phaseId", "url"] {
        let mut changed = binding.clone();
        changed[key] = serde_json::json!("foreign");
        assert!(capacity_l6_web_collect(&area.directory, &changed, "development-browser").is_err());
    }
    assert!(capacity_l6_web_collect(&area.directory, &binding, "foreign-browser").is_err());
    std::fs::write(
        area.directory.join("capture.json"),
        b"changed after receipt",
    )
    .unwrap();
    assert!(capacity_l6_web_collect(&area.directory, &binding, "development-browser").is_err());
}

#[test]
fn capacity_l6_web_dropped_guard_cleans_secrets_and_requests_original_stop() {
    let area = TestArea::new();
    let info = area.directory.join("info.json");
    let receipt = area.directory.join("receipt.json");
    std::fs::write(&info, b"inert-secret").unwrap();
    std::fs::write(&receipt, b"inert-receipt").unwrap();
    let (control, stopped) =
        crate::ipc::IpcControl::new("development-node".into(), "https://localhost:1".into());
    let guard = CapacityL6WebFiles {
        info: info.clone(),
        receipt: receipt.clone(),
        control,
        stop_on_drop: true,
    };
    assert!(!*stopped.borrow());
    drop(guard);
    assert!(*stopped.borrow());
    assert!(!info.exists());
    assert!(!receipt.exists());
}

async fn capacity_web_test_node(outer: &TestArea) -> R3WssCase {
    let directory = outer.directory.join("node");
    std::fs::create_dir(&directory).unwrap();
    let area = TestArea {
        directory,
        certificate: outer.certificate.clone(),
        private_key: outer.private_key.clone(),
    };
    let listen = free_address();
    let mut config = area.config(listen);
    config.canonical_origin = format!("https://localhost:{}", listen.port());
    let owner = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        config,
        Arc::new(FakeProtector::default()),
    );
    let node = owner.ready().await.unwrap();
    R3WssCase {
        selection: Some(SourceLaunchSelection {
            source: r3_id(1),
            harness: r3_id(2),
            profile: crate::domain::native_catalog::ProfileKey::try_new(r3_id(3), 1).unwrap(),
            workspace: r3_id(4),
            launch_path: area.directory.to_string_lossy().into_owned(),
        }),
        area,
        owner: Some(owner),
        node: Some(node),
        peers: Vec::new(),
    }
}

async fn capacity_web_test_wait_json(path: &std::path::Path) -> serde_json::Value {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(bytes) = std::fs::read(path)
                && let Ok(value) = serde_json::from_slice(&bytes)
            {
                break value;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_web_real_pairing_revokes_and_never_signs_inert_dom() {
    let outer = TestArea::new();
    let mut fixture = capacity_web_test_node(&outer).await;
    let node = fixture.node.as_ref().unwrap();
    let pairing = node.pairing.as_ref().unwrap().clone();
    let address = node.ipc_address().clone();
    let phase_root = outer.directory.join("web-inspect");
    let actor = async {
        let info = capacity_web_test_wait_json(&phase_root.join("info.json")).await;
        let request_id = Id128::new([81; 16]);
        let lookup = pairing
            .lookup(info["pairingCode"].as_str().unwrap(), request_id)
            .await
            .unwrap();
        let key = SigningKey::from_slice(&[23; 32]).unwrap();
        let claim = crate::claims::PairClaim {
            v: 1,
            node_id: lookup.challenge.node_id,
            origin: lookup.challenge.origin,
            request_id,
            challenge_id: lookup.challenge.challenge_id,
            nonce: lookup.challenge.nonce,
            iat: lookup.challenge.iat,
            exp: lookup.challenge.exp,
            pairing_id: lookup.pairing_id,
            client_public_key: client_public_key(&key),
            display_name: info["displayName"].as_str().unwrap().to_owned(),
        };
        let proof = sign(
            JwsType::PairClaim,
            &serde_json::to_vec(&claim).unwrap(),
            &key,
        )
        .unwrap();
        pairing.claim(&proof).await.unwrap();
        let paired = capacity_web_test_wait_json(&phase_root.join("paired.json")).await;
        let client = paired["browserClientId"].as_str().unwrap();
        capacity_web_test_artifacts(&phase_root, &info["binding"], client);
    };
    let run = AssertUnwindSafe(async {
        let (outcome, ()) = tokio::join!(
            capacity_l6_web_phase(
                &mut fixture,
                &outer.directory,
                "inspect",
                capacity_web_test_binding()["expected"].clone(),
                Duration::from_secs(10)
            ),
            actor
        );
        let summary = outcome.unwrap();
        assert_eq!(summary["status"], "pending_independent_review");
        assert_eq!(summary["acceptancePassed"], false);
        assert_eq!(summary["browserIdentityRevoked"], true);
        let page = crate::ipc::device_list(&address, None).await.unwrap();
        let original = page
            .items
            .iter()
            .find(|item| Some(item.client_id.as_str()) == summary["browserClientId"].as_str())
            .unwrap();
        assert_eq!(original.status, crate::trust::ClientStatus::Revoked);
        assert!(!phase_root.join("info.json").exists());
        assert!(!phase_root.join("receipt.json").exists());
    })
    .catch_unwind()
    .await;
    let closed = fixture.close().await;
    assert!(closed, "original development Node owner must join");
    if let Err(panic) = run {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_web_deadline_keeps_missing_browser_unconfirmed_and_stops_node() {
    let outer = TestArea::new();
    let mut fixture = capacity_web_test_node(&outer).await;
    let result = capacity_l6_web_phase(
        &mut fixture,
        &outer.directory,
        "deadline",
        capacity_web_test_binding()["expected"].clone(),
        Duration::from_millis(50),
    )
    .await;
    assert_eq!(
        result.unwrap_err(),
        "web_phase_deadline_missing_independent_evidence"
    );
    assert!(!outer.directory.join("web-deadline/info.json").exists());
    assert!(fixture.close().await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_web_cancel_drops_secret_and_original_node_owner_joins() {
    let outer = TestArea::new();
    let mut fixture = capacity_web_test_node(&outer).await;
    let phase_root = outer.directory.join("web-cancel");
    let info_path = phase_root.join("info.json");
    let stopped = fixture.node.as_ref().unwrap().control.stop_receiver();
    {
        let phase = capacity_l6_web_phase(
            &mut fixture,
            &outer.directory,
            "cancel",
            capacity_web_test_binding()["expected"].clone(),
            Duration::from_secs(10),
        );
        tokio::pin!(phase);
        tokio::select! {
            result = &mut phase => panic!("phase unexpectedly ended before cancellation: {result:?}"),
            _ = capacity_web_test_wait_json(&info_path) => {}
        }
    }
    assert!(*stopped.borrow());
    assert!(!phase_root.join("info.json").exists());
    assert!(fixture.close().await);
}
