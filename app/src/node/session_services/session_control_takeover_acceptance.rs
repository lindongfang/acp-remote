// Session control takeover & read-only isolation L5 tests (R3 ACP source/approval runtime).
// Included under `crate::node::tests::r3_source_wss_development`.

async fn sct_install_capacity_peer(f: &mut R3WssCase, scenario: &str) {
    let child = crate::test_support::session_capacity_acceptance_peer();
    let child_str = child.to_string_lossy();
    assert!(std::path::Path::new(child_str.as_ref()).is_file());
    let raw = format!(
        r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}"],"environment":[]}},"recipes":[{{"wire":2,"mcp_servers":[],"additional_directories":[],"extra":{{}}}}]}}"#,
        r3_utf16(&child_str),
        r3_utf16(scenario)
    );
    let profile =
        crate::adapters::acp::node_configuration_profile::parse_profile(raw.into_bytes()).unwrap();
    let draft = CatalogDraft::install_harness(
        "SCT capacity peer".into(),
        SourceSelection::Existing(r3_id(1)),
        profile,
    )
    .unwrap();
    let node = f.node.as_ref().unwrap();
    let policy = node.application_policy().unwrap();
    let actor = policy_actor(&policy, &f.peers[0].key, &f.peers[0].client_id, Id128::new([250; 16])).await;
    let catalog = &node.session_services.as_ref().unwrap().catalog;
    let registered = catalog.register(&actor, draft, &r3_deadline()).await.unwrap();
    let result = catalog.apply(&actor, registered, &r3_deadline()).await.unwrap();
    let (harness, profile_id) = match result.fact().result().unwrap() {
        CatalogResult::Harness {
            harness, profile, ..
        } => (*harness, *profile),
        _ => panic!("expected installed SCT capacity peer profile"),
    };
    let launch = f
        .area
        .directory
        .canonicalize()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    f.selection = Some(crate::domain::source_sessions::SourceLaunchSelection {
        source: r3_id(1),
        harness,
        profile: profile_id,
        workspace: r3_id(9),
        launch_path: launch,
    });
}

async fn sct_r3_call_peer(
    f: &mut R3WssCase,
    peer_idx: usize,
    marker: u8,
    action: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    let request_id = Id128::new([marker; 16]);
    send_json(
        &mut f.peers[peer_idx].socket,
        &serde_json::json!({
            "v": 1,
            "type": "command",
            "requestId": request_id,
            "action": action,
            "payload": payload,
        }),
    )
    .await;
    let expected = request_id.to_base64url();
    loop {
        let frame = next_json(&mut f.peers[peer_idx].socket, Duration::from_secs(15)).await;
        if frame["type"] != "result" {
            continue;
        }
        assert_eq!(frame["requestId"], expected, "{frame}");
        return frame;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sct_red_check_approval_uncontrolled_and_cross_epoch_two_step_rejected() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            sct_install_capacity_peer(f, "source-create-replay-state").await;
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let root = capacity_create(f, 10).await;
            let root_target = r3_id(40);
            let child_target = r3_id(41);
            capacity_route_target(f, &root, root_target).await;
            capacity_route_target(f, "runtime-child-leaf", child_target).await;
            std::fs::write(
                f.area.directory.join("runtime-emit-approval-tree"),
                root.as_bytes(),
            )
            .unwrap();
            let discovered = capacity_source_operation(
                f,
                20,
                serde_json::json!({ "kind": "discover", "cursor": null }),
            )
            .await;
            assert_eq!(discovered["ok"], true, "{discovered}");
            assert_eq!(discovered["result"]["outcome"], "succeeded", "{discovered}");

            // Wait until approval is confirmed & associated so failure can only come from control gate.
            let (approval_id, choice_id) = tokio::time::timeout(Duration::from_secs(10), async {
                let node = f.node.as_ref().unwrap();
                let approvals = node.session_services.as_ref().unwrap().approval.clone();
                let policy = node.application_policy().unwrap();
                let actor =
                    policy_actor(&policy, &f.peers[1].key, &f.peers[1].client_id, Id128::new([29; 16]))
                        .await;
                loop {
                    let mut all = approvals.list(&actor).await.unwrap();
                    if let Some(value) = all.pop()
                        && value.status().request_save.commit
                            == crate::domain::approval_owner::CommitState::Confirmed
                    {
                        break (
                            value.id().encode(),
                            value.choices()[0].id().encode(),
                        );
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("confirmed associated approval");

            // Wait until discover's parent-child link (`node_session_parents`) is also persisted so
            // `resolve_control_root` maps `child_target` to Peer 0's controlled `root`.
            tokio::time::timeout(Duration::from_secs(10), async {
                let registry = f
                    .node
                    .as_ref()
                    .unwrap()
                    .storage
                    .as_ref()
                    .unwrap()
                    .request_registry();
                loop {
                    if let Ok(resolved) = registry.resolve_control_root(child_target).await
                        && resolved.root.native_id() == root.as_str()
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("child_target parent link persisted");

            // 1. Peer 0 holds initial control of `root` (and its child tree); Peer 1 is uncontrolled
            // and must receive session_not_controlled on approval.register!
            let denied = sct_r3_call_peer(
                f,
                1,
                31,
                "approval.register",
                serde_json::json!({
                    "approvalId": approval_id,
                    "choiceId": choice_id,
                }),
            )
            .await;
            assert_eq!(denied["ok"], false, "{denied}");
            assert_eq!(denied["error"]["code"], "session_not_controlled", "{denied}");

            // 2. Peer 0 registers approval decision at epoch 1 -> Peer 1 preempts (epoch 2) -> Peer 0 retakes (epoch 3).
            let reg_a = sct_r3_call_peer(
                f,
                0,
                32,
                "approval.register",
                serde_json::json!({
                    "approvalId": approval_id,
                    "choiceId": choice_id,
                    "expectedControlEpoch": "1",
                }),
            )
            .await;
            assert_eq!(reg_a["ok"], true, "{reg_a}");
            let reg_a_id = reg_a["result"]["approvalRegistrationId"]
                .as_str()
                .expect("approvalRegistrationId")
                .to_owned();

            let take_b = sct_r3_call_peer(
                f,
                1,
                33,
                "session.control.takeover",
                serde_json::json!({ "targetId": child_target.encode() }),
            )
            .await;
            assert_eq!(take_b["ok"], true, "{take_b}");
            assert_eq!(take_b["result"]["controlEpoch"], "2", "{take_b}");

            let retake_a = sct_r3_call_peer(
                f,
                0,
                34,
                "session.control.takeover",
                serde_json::json!({ "targetId": root_target.encode() }),
            )
            .await;
            assert_eq!(retake_a["ok"], true, "{retake_a}");
            assert_eq!(retake_a["result"]["controlEpoch"], "3", "{retake_a}");

            // Peer 0 submits epoch-1 reg_a_id after A(1)->B(2)->A(3) wrap-around: must be rejected!
            let stale_decide = sct_r3_call_peer(
                f,
                0,
                35,
                "approval.decide",
                serde_json::json!({
                    "approvalRegistrationId": reg_a_id,
                }),
            )
            .await;
            assert_eq!(stale_decide["ok"], false, "{stale_decide}");
            assert_eq!(
                stale_decide["error"]["code"], "session_not_controlled",
                "{stale_decide}"
            );
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sct_rv_int_001_approval_decide_rechecks_control_before_commit_and_releases_admission() {
    use crate::application::approval_owner::{
        ApprovalAuthority, CurrentApprovalActor, CurrentApprovalReader,
    };
    use crate::application::native_dispatch::NativeApprovalService;
    use crate::domain::EntityId;
    use crate::domain::approval_owner::{
        ApprovalRegistrationId, ApprovalRejection, DecisionAcceptance, NecessaryWriteError,
        RegisterDecision, SubmitDecision,
    };
    use crate::domain::native::NativeSessionKey;
    use crate::node::request_authority::NodeRequestAuthority;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct PreemptOnThirdVerify {
        inner: NodeRequestAuthority,
        control: Arc<crate::application::session_control::SessionControlCoordinator>,
        preemptor: EntityId,
        target: EntityId,
        root: NativeSessionKey,
        verify_calls: AtomicUsize,
    }

    impl ApprovalAuthority for PreemptOnThirdVerify {
        type Actor = crate::access_policy::AuthenticatedActor;
        fn verify_owner_scope(
            &self,
            owner: &crate::application::request_registry::RegistryOwnerScope,
        ) -> Result<(), ApprovalRejection> {
            self.inner.verify_owner_scope(owner)
        }
        fn authorize_decision<'a>(
            &'a self,
            actor: &'a Self::Actor,
            target: EntityId,
        ) -> crate::application::native_dispatch::BoxFuture<
            'a,
            Result<CurrentApprovalActor, ApprovalRejection>,
        > {
            self.inner.authorize_decision(actor, target)
        }
        fn authorize_decision_with_epoch<'a>(
            &'a self,
            actor: &'a Self::Actor,
            target: EntityId,
            expected_control_epoch: Option<u64>,
        ) -> crate::application::native_dispatch::BoxFuture<
            'a,
            Result<CurrentApprovalActor, ApprovalRejection>,
        > {
            self.inner
                .authorize_decision_with_epoch(actor, target, expected_control_epoch)
        }
        fn verify_approval_decide(
            &self,
            actor: &CurrentApprovalActor,
            registration: ApprovalRegistrationId,
            expected_control_epoch: Option<u64>,
        ) -> Result<(), ApprovalRejection> {
            let call = self.verify_calls.fetch_add(1, Ordering::SeqCst) + 1;
            if call == 3 {
                self.control
                    .takeover(
                        self.preemptor,
                        "Peer 1",
                        r3_id(240),
                        self.target,
                        &self.root,
                        Some(self.target),
                    )
                    .unwrap();
            }
            self.inner
                .verify_approval_decide(actor, registration, expected_control_epoch)
        }
        fn authorize_read<'a>(
            &'a self,
            actor: &'a Self::Actor,
        ) -> crate::application::native_dispatch::BoxFuture<
            'a,
            Result<CurrentApprovalReader, ApprovalRejection>,
        > {
            self.inner.authorize_read(actor)
        }
    }

    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            sct_install_capacity_peer(f, "source-create-replay-state").await;
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let root = capacity_create(f, 10).await;
            let root_target = r3_id(40);
            let child_target = r3_id(41);
            capacity_route_target(f, &root, root_target).await;
            capacity_route_target(f, "runtime-child-leaf", child_target).await;
            std::fs::write(
                f.area.directory.join("runtime-emit-approval-tree"),
                root.as_bytes(),
            )
            .unwrap();
            let discovered = capacity_source_operation(
                f,
                20,
                serde_json::json!({ "kind": "discover", "cursor": null }),
            )
            .await;
            assert_eq!(discovered["ok"], true, "{discovered}");
            assert_eq!(discovered["result"]["outcome"], "succeeded", "{discovered}");

            let node = f.node.as_ref().unwrap();
            let storage = node.storage.as_ref().unwrap();
            let policy = node.application_policy().unwrap();
            let actor_0 =
                policy_actor(&policy, &f.peers[0].key, &f.peers[0].client_id, Id128::new([31; 16]))
                    .await;
            let actor_1 =
                policy_actor(&policy, &f.peers[1].key, &f.peers[1].client_id, Id128::new([32; 16]))
                    .await;
            let approvals = node.session_services.as_ref().unwrap().approval.clone();

            let display = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let mut all = approvals.list(&actor_0).await.unwrap();
                    if let Some(value) = all.pop()
                        && value.status().request_save.commit
                            == crate::domain::approval_owner::CommitState::Confirmed
                    {
                        break value;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("confirmed approval");

            let resolved = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if let Ok(r) = storage.request_registry().resolve_control_root(child_target).await
                        && r.root.native_id() == root.as_str()
                    {
                        break r;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("child_target parent link persisted");

            let reg_0 = approvals
                .register_decision(
                    &actor_0,
                    RegisterDecision::new(display.id(), display.choices()[0].id()),
                )
                .await
                .unwrap();

            let wrapped = NativeApprovalService::create(
                node.session_services.as_ref().unwrap().native.clone(),
                Arc::new(PreemptOnThirdVerify {
                    inner: node.request_authority().unwrap(),
                    control: storage.session_control(),
                    preemptor: EntityId::decode(&f.peers[1].client_id).unwrap(),
                    target: child_target,
                    root: resolved.root,
                    verify_calls: AtomicUsize::new(0),
                }),
            )
            .unwrap();

            let rejected = wrapped
                .decide(&actor_0, SubmitDecision::new(reg_0))
                .await
                .unwrap_err();
            assert!(
                matches!(
                    rejected,
                    NecessaryWriteError::Rejected(ApprovalRejection::SessionNotControlled)
                ),
                "expected SessionNotControlled from pre-commit verify, got {rejected:?}"
            );

            let count_before: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM main.node_approval_decisions WHERE approval_id = ?",
            )
            .bind(display.id().encode())
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
            assert_eq!(count_before, 0);

            let reg_1 = approvals
                .register_decision(
                    &actor_1,
                    RegisterDecision::new(display.id(), display.choices()[0].id()),
                )
                .await
                .unwrap();
            let accepted = approvals
                .decide(&actor_1, SubmitDecision::new(reg_1))
                .await
                .unwrap();
            assert!(matches!(accepted, DecisionAcceptance::Accepted(_)));

            let count_after: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM main.node_approval_decisions WHERE approval_id = ?",
            )
            .bind(display.id().encode())
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
            assert_eq!(count_after, 1);
        })
    })
    .await;
}
