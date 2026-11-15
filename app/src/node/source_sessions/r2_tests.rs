async fn source_add_paired_client(f: &mut SourceCase, key_byte: u8, name: &str) -> (String, usize) {
    let node = f.node.as_ref().unwrap();
    let key = SigningKey::from_slice(&[key_byte; 32]).unwrap();
    let id = pair_browser(node, &key, name).await;
    let observed = node
        .management
        .as_ref()
        .unwrap()
        .observe_next_document_socket_for_test()
        .unwrap();
    let peer = valid_management_socket(&f.area, node.listen_addr()).await;
    f.peers
        .push(authenticate_socket(peer, &key, &id, node_public_key(node)).await);
    let endpoint = observed.await.unwrap();
    f.clients.push(endpoint.client());
    f.endpoints.push(endpoint);
    (id, f.clients.len() - 1)
}

async fn source_read_one_chunk(
    f: &mut SourceCase,
    idx: usize,
    transfer: &DocumentTransfer,
) -> (Vec<u8>, u64) {
    f.endpoints[idx].attach(transfer).unwrap();
    f.peers[idx]
        .send(Message::Binary(
            TransferHeader::credit(transfer.descriptor().stream_id, 0)
                .encode(&[])
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
    let state = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let value = transfer.prepare_next(&f.clients[idx]).await.unwrap();
            if value != DocumentPrepare::AwaitCredit {
                break value;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(state, DocumentPrepare::Prepared);
    assert_eq!(
        f.endpoints[idx].send(transfer).unwrap(),
        DocumentEnqueue::Queued
    );
    let bytes = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match f.peers[idx].next().await.unwrap().unwrap() {
                Message::Binary(bytes) => break bytes,
                Message::Ping(_) | Message::Pong(_) => {}
                other => panic!("unexpected frame kind {:?}", std::mem::discriminant(&other)),
            }
        }
    })
    .await
    .unwrap();
    let (header, payload) = TransferHeader::decode(&bytes).unwrap();
    assert_eq!(header.record_kind(), Some(RecordKind::OriginalRaw));
    f.peers[idx]
        .send(Message::Binary(
            TransferHeader::credit(header.stream(), header.packet())
                .encode(&[])
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
    (payload.to_vec(), header.packet())
}

async fn source_drain_at(
    f: &mut SourceCase,
    idx: usize,
    transfer: &DocumentTransfer,
    send_initial_credit: bool,
) -> Vec<u8> {
    if send_initial_credit {
        f.endpoints[idx].attach(transfer).unwrap();
        f.peers[idx]
            .send(Message::Binary(
                TransferHeader::credit(transfer.descriptor().stream_id, 0)
                    .encode(&[])
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
    }
    let mut raw = Vec::new();
    loop {
        let state = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let value = transfer.prepare_next(&f.clients[idx]).await.unwrap();
                if value != DocumentPrepare::AwaitCredit {
                    break value;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        if state == DocumentPrepare::Complete {
            return raw;
        }
        assert_eq!(
            f.endpoints[idx].send(transfer).unwrap(),
            DocumentEnqueue::Queued
        );
        let bytes = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match f.peers[idx].next().await.unwrap().unwrap() {
                    Message::Binary(bytes) => break bytes,
                    Message::Ping(_) | Message::Pong(_) => {}
                    other => panic!("unexpected frame kind {:?}", std::mem::discriminant(&other)),
                }
            }
        })
        .await
        .unwrap();
        let (header, payload) = TransferHeader::decode(&bytes).unwrap();
        if header.record_kind() == Some(RecordKind::OriginalRaw) {
            raw.extend_from_slice(payload);
        } else {
            assert_eq!(header.record_kind(), Some(RecordKind::SnapshotEnd));
            assert!(payload.is_empty());
        }
        f.peers[idx]
            .send(Message::Binary(
                TransferHeader::credit(header.stream(), header.packet())
                    .encode(&[])
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_v02_delayed_create_only_mints_native_id_from_matched_response() {
    source_case("delayed-create", |f| {
        Box::pin(async move {
            let service = f.service();
            let id = f.register(SourceSessionAction::Create).await;
            assert!(
                !f.area.directory.join("source-create-count").exists(),
                "registration alone must not start child or send session/new"
            );
            let receipt = service.execute(&f.clients[0], id).await.unwrap();
            source_wait_file(f, "source-create-waiting").await;

            // Before response returns, transport is Written and no native_session_id exists.
            let pending = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let fact = service.query(&f.clients[0], id).await.unwrap();
                    if fact.transport
                        == Some(crate::domain::native_dispatch::TransportFact::Written)
                    {
                        break fact;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(pending.state, SourceSessionState::Owned);
            assert_eq!(pending.outcome, SourceSessionOutcome::Pending);
            assert!(pending.response.is_none());
            assert!(pending.rpc.is_some());
            assert!(pending.source_instance.is_some());
            assert!(pending.connection_epoch.is_some());
            assert_eq!(
                pending.wire,
                Some(crate::domain::native_dispatch::ProtocolVersion::V1)
            );
            assert!(receipt.try_result().is_none());

            let new_requests =
                std::fs::read_to_string(f.area.directory.join("source-new-request.jsonl")).unwrap();
            let req: serde_json::Value =
                serde_json::from_str(new_requests.lines().next().unwrap()).unwrap();
            assert_eq!(
                req["cwd"].as_str(),
                Some(f.selection.as_ref().unwrap().launch_path.as_str())
            );
            assert_eq!(req["mcpServers"], serde_json::json!([]));

            std::fs::write(
                f.area.directory.join("source-release-create"),
                b"release v02",
            )
            .unwrap();
            let fact = tokio::time::timeout(Duration::from_secs(15), receipt.wait())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(fact.outcome, SourceSessionOutcome::Succeeded);
            let response = fact.response.as_ref().unwrap();
            assert_eq!(response.kind, SourceResponseKind::Created);
            assert_eq!(
                response.native_session_id.as_deref(),
                Some("source-native-created")
            );
            assert_eq!(response.ack, SourceAckState::Confirmed);

            let transfer = service.open_result(&f.clients[0], id).await.unwrap();
            let raw = source_drain(f, &transfer).await;
            let raw_text = std::str::from_utf8(&raw).unwrap();
            assert!(raw_text.contains("source-native-created") && raw_text.contains("1.2300e+09"));
            assert!(service.open_result(&f.clients[0], id).await.is_err());

            let same = service.execute(&f.clients[1], id).await.unwrap();
            assert_eq!(same.identity(), receipt.identity());
            assert_eq!(
                same.wait().await.unwrap().outcome,
                SourceSessionOutcome::Succeeded
            );
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("source-create-count")).unwrap(),
                "1"
            );
            assert!(!f.area.directory.join("source-list-count").exists());
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_m01_admission_scope_actor_source_and_seal_boundaries() {
    source_case("delayed-create", |f| {
        Box::pin(async move {
            let service = f.service();

            // Foreign Node / foreign socket client must be rejected with 0 child/RPC.
            let mut foreign = SourceCase {
                area: TestArea::new(),
                owner: None,
                node: None,
                peers: Vec::new(),
                clients: Vec::new(),
                endpoints: Vec::new(),
                selection: None,
            };
            foreign.start("normal").await;
            assert!(matches!(
                service
                    .register(
                        &foreign.clients[0],
                        f.selection.as_ref().unwrap().clone(),
                        SourceSessionAction::Create
                    )
                    .await,
                Err(SourceSessionFailure::Rejected(
                    SourceSessionError::InvalidScope
                ))
            ));
            let _ = foreign.close().await;

            // Mismatched selections rejected before child launch (SQL rolls back cleanly).
            let base = f.selection.as_ref().unwrap().clone();
            for bad in [
                {
                    let mut s = base.clone();
                    s.profile = crate::domain::native_catalog::ProfileKey::try_new(
                        base.profile.id(),
                        999,
                    )
                    .unwrap();
                    s
                },
                {
                    let mut s = base.clone();
                    s.workspace = source_id(88);
                    s
                },
                {
                    let mut s = base.clone();
                    s.source = source_id(89);
                    s
                },
            ] {
                let err = service
                    .register(&f.clients[0], bad, SourceSessionAction::Create)
                    .await
                    .unwrap_err();
                assert!(matches!(
                    err,
                    SourceSessionFailure::NecessarySql(ref sql_err)
                        if sql_err.kind == SourceSessionError::InvalidSelection
                            && sql_err.observation.as_ref().is_some_and(|obs| {
                                obs.transaction
                                    == crate::domain::native_catalog::TransactionKnowledge::RollbackConfirmed
                                    && obs.cleanup
                                        == crate::domain::native_catalog::CleanupKnowledge::ConfirmedOriginal
                            })
                ));
            }

            // Actor B is paired on same Node: cannot execute Actor A's registration.
            let (_actor_b_id, idx_b) = source_add_paired_client(f, 72, "Actor B").await;
            let op_a = f.register(SourceSessionAction::Create).await;
            assert!(matches!(
                service.execute(&f.clients[idx_b], op_a).await,
                Err(SourceSessionFailure::Rejected(
                    SourceSessionError::InvalidActor
                ))
            ));
            assert!(!f.area.directory.join("source-create-count").exists());

            // Pre-execute socket seal prevents admission.
            f.peers[1].close(None).await.unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                while f.clients[1].context.lifetime.with_live(|| Ok(())).is_ok() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(matches!(
                service.execute(&f.clients[1], op_a).await,
                Err(SourceSessionFailure::Rejected(
                    SourceSessionError::InvalidActor
                ))
            ));
            assert!(!f.area.directory.join("source-create-count").exists());

            // Post-execute revocation of Actor A does NOT cancel already-admitted Owned operation,
            // while new query/open_result from Actor A fails and Actor B can still query/read.
            let receipt = service.execute(&f.clients[0], op_a).await.unwrap();
            source_wait_file(f, "source-create-waiting").await;
            {
                let node = f.node.as_ref().unwrap();
                let pool = node.storage.as_ref().unwrap().pool_for_test();
                let actor_a_id: String = sqlx::query_scalar(
                    "SELECT client_id FROM clients WHERE display_name='Source session actor'",
                )
                .fetch_one(pool)
                .await
                .unwrap();
                crate::ipc::device_revoke(node.ipc_address(), &actor_a_id)
                    .await
                    .unwrap();
            }

            assert!(matches!(
                service.query(&f.clients[0], op_a).await,
                Err(SourceSessionFailure::Rejected(
                    SourceSessionError::InvalidActor
                ))
            ));
            std::fs::write(
                f.area.directory.join("source-release-create"),
                b"after revoke",
            )
            .unwrap();
            let fact = tokio::time::timeout(Duration::from_secs(15), receipt.wait())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(fact.outcome, SourceSessionOutcome::Succeeded);
            assert!(service.open_result(&f.clients[0], op_a).await.is_err());

            let fact_b = service.query(&f.clients[idx_b], op_a).await.unwrap();
            assert_eq!(fact_b.outcome, SourceSessionOutcome::Succeeded);
            assert_eq!(
                fact_b
                    .response
                    .as_ref()
                    .and_then(|resp| resp.native_session_id.as_deref()),
                Some("source-native-created")
            );
            assert!(matches!(
                service.open_result(&f.clients[idx_b], op_a).await,
                Err(SourceSessionFailure::Rejected(
                    SourceSessionError::ResultUnavailable
                ))
            ));

            // While another actor (Actor C) stays live, Actor B cannot steal execute
            // yet CAN query and open/read Actor C's retained raw result.
            let (_actor_c_id, idx_c) = source_add_paired_client(f, 73, "Actor C").await;
            let op_c = service
                .register(
                    &f.clients[idx_c],
                    base.clone(),
                    SourceSessionAction::Create,
                )
                .await
                .unwrap()
                .id;
            assert!(matches!(
                service.execute(&f.clients[idx_b], op_c).await,
                Err(SourceSessionFailure::Rejected(
                    SourceSessionError::InvalidActor
                ))
            ));
            let receipt_c = service.execute(&f.clients[idx_c], op_c).await.unwrap();
            let fact_c = tokio::time::timeout(Duration::from_secs(15), receipt_c.wait())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(fact_c.outcome, SourceSessionOutcome::Succeeded);
            let query_c_by_b = service.query(&f.clients[idx_b], op_c).await.unwrap();
            assert_eq!(query_c_by_b.outcome, SourceSessionOutcome::Succeeded);
            assert!(query_c_by_b.result_available);
            let transfer_b = service.open_result(&f.clients[idx_b], op_c).await.unwrap();
            let raw_b = source_drain_at(f, idx_b, &transfer_b, true).await;
            assert!(
                std::str::from_utf8(&raw_b)
                    .unwrap()
                    .contains("source-native-created")
            );

            // Different explicit source_id with identical short native_session_id ("source-native-created")
            // does not confuse physical equivalence / claim conflict.
            let node = f.node.as_ref().unwrap();
            let pool = node.storage.as_ref().unwrap().pool_for_test();
            sqlx::query("INSERT INTO product_sources VALUES(?,'second explicit source')")
                .bind(source_id(11).encode())
                .execute(pool)
                .await
                .unwrap();
            let child = crate::test_support::native_dispatch_dev_child(&["SOURCE_SESSION_CHILD"]);
            let child = child.to_string_lossy();
            let raw_prof = format!(
                r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}"],"environment":[]}},"recipes":[{{"wire":1,"mcp_servers":[],"additional_directories":[],"extra":{{}}}}]}}"#,
                source_utf16(&child),
                source_utf16("normal")
            );
            let profile2 =
                crate::adapters::acp::node_configuration_profile::parse_profile(raw_prof.into_bytes())
                    .unwrap();
            let draft2 = CatalogDraft::install_harness(
                "Second source harness".into(),
                SourceSelection::Existing(source_id(11)),
                profile2,
            )
            .unwrap();
            let catalog = &node.session_services.as_ref().unwrap().catalog;
            let reg2 = catalog
                .register(f.clients[idx_b].context.actor(), draft2, &source_deadline())
                .await
                .unwrap();
            let applied2 = catalog
                .apply(f.clients[idx_b].context.actor(), reg2, &source_deadline())
                .await
                .unwrap();
            let (h2, p2) = match applied2.fact().result().unwrap() {
                CatalogResult::Harness {
                    harness, profile, ..
                } => (*harness, *profile),
                _ => panic!("expected harness"),
            };
            let sel2 = SourceLaunchSelection {
                source: source_id(11),
                harness: h2,
                profile: p2,
                workspace: base.workspace,
                launch_path: base.launch_path.clone(),
            };
            let op_b = service
                .register(&f.clients[idx_b], sel2, SourceSessionAction::Create)
                .await
                .unwrap()
                .id;
            let fact2 = tokio::time::timeout(
                Duration::from_secs(15),
                service.execute(&f.clients[idx_b], op_b).await.unwrap().wait(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(fact2.outcome, SourceSessionOutcome::Succeeded);
            assert!(fact2.failure.is_none());
            service.release_result(&f.clients[idx_b], op_b).await.unwrap();
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_m01_admit_and_arm_sql_faults_keep_zero_new() {
    use crate::application::native_dispatch::source_sessions::SourceSqlTestFault;
    use crate::domain::native_catalog::{CleanupKnowledge, TransactionKnowledge};
    source_case("normal", |f| {
        Box::pin(async move {
            let service = f.service();

            // 1. Admit INSERT failure -> 0 child / 0 session/new.
            let id_admit = f.register(SourceSessionAction::Create).await;
            service
                .arm_next_sql_fault_for_test(
                    SourceSqlRole::Admit,
                    SourceSqlTestFault::StatementError,
                )
                .unwrap();
            let err_admit = tokio::time::timeout(
                Duration::from_secs(15),
                service
                    .execute(&f.clients[0], id_admit)
                    .await
                    .unwrap()
                    .wait(),
            )
            .await
            .unwrap()
            .unwrap_err();
            match err_admit {
                SourceSessionFailure::NecessarySql(sql_err) => {
                    let obs = sql_err.observation.unwrap();
                    assert_eq!(obs.role, SourceSqlRole::Admit);
                    assert_eq!(obs.transaction, TransactionKnowledge::RollbackConfirmed);
                    assert_eq!(obs.cleanup, CleanupKnowledge::ConfirmedOriginal);
                }
                other => panic!("expected NecessarySql on Admit failure, got {other:?}"),
            }
            assert!(!f.area.directory.join("source-create-count").exists());
            let q1 = service.query(&f.clients[1], id_admit).await.unwrap();
            assert_eq!(q1.state, SourceSessionState::Registered);

            // 2. Arm UPDATE failure -> source initialized/attached, but 0 session/new written.
            let id_arm = f.register(SourceSessionAction::Create).await;
            service
                .arm_next_sql_fault_for_test(SourceSqlRole::Arm, SourceSqlTestFault::StatementError)
                .unwrap();
            let err_arm = tokio::time::timeout(
                Duration::from_secs(15),
                service.execute(&f.clients[0], id_arm).await.unwrap().wait(),
            )
            .await
            .unwrap()
            .unwrap_err();
            match err_arm {
                SourceSessionFailure::NecessarySql(sql_err) => {
                    let obs = sql_err.observation.unwrap();
                    assert_eq!(obs.role, SourceSqlRole::Arm);
                    assert_eq!(obs.transaction, TransactionKnowledge::RollbackConfirmed);
                    assert_eq!(obs.cleanup, CleanupKnowledge::ConfirmedOriginal);
                }
                other => panic!("expected NecessarySql on Arm failure, got {other:?}"),
            }
            assert!(!f.area.directory.join("source-create-count").exists());
            let q2 = service.query(&f.clients[1], id_arm).await.unwrap();
            assert_eq!(q2.outcome, SourceSessionOutcome::Refused);
            assert!(q2.transport.is_none());
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_m02_register_query_caller_cancel_windows_and_never_scheduled() {
    use crate::application::native_dispatch::source_sessions::SourceSqlScheduleState;
    use crate::domain::native_catalog::{CleanupKnowledge, TransactionKnowledge};
    use crate::session_storage::request_registry::connection_close_test_hooks::{
        CloseBarrier, ClosePoint,
    };
    source_case("normal", |f| {
        Box::pin(async move {
            let service = f.service();
            let registry = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .request_registry();
            let closer = registry.connection_closer_for_test();

            // Window 1: Register after task/receipt installed, BEFORE SQL enters.
            let phase_reg1 = service.pause_next_sql_for_test(SourceSqlRole::Register).unwrap();
            let inv_reg1_rx = service
                .observe_next_invocation_for_test(SourceSqlRole::Register)
                .unwrap();
            let mut fut_reg1 = Box::pin(service.register(
                &f.clients[0],
                f.selection.as_ref().unwrap().clone(),
                SourceSessionAction::Create,
            ));
            tokio::select! {
                res = &mut fut_reg1 => panic!("register finished before pause: {res:?}"),
                wait = tokio::time::timeout(Duration::from_secs(10), phase_reg1.wait_until_entered()) => wait.unwrap(),
            }
            drop(fut_reg1);
            let inv_reg1 = inv_reg1_rx.await.unwrap();
            let op_id1 = inv_reg1.operation_id.unwrap();
            assert_eq!(inv_reg1.schedule_state(), SourceSqlScheduleState::Scheduled);
            assert!(inv_reg1.task_observation().installed);
            assert_eq!(inv_reg1.sql_started_for_test(), 0);
            phase_reg1.release();
            assert!(
                inv_reg1
                    .join_original_task(tokio::time::Instant::now() + Duration::from_secs(10))
                    .await
            );
            assert_eq!(inv_reg1.sql_started_for_test(), 1);
            let obs1 = inv_reg1.sql_observation().unwrap();
            assert_eq!(obs1.transaction, TransactionKnowledge::CommitConfirmed);
            assert_eq!(obs1.cleanup, CleanupKnowledge::ConfirmedOriginal);
            assert_eq!(
                service.query(&f.clients[1], op_id1).await.unwrap().state,
                SourceSessionState::Registered
            );

            // Window 2: Register AFTER SQL entered, BEFORE close report.
            let close_reg2 = CloseBarrier::new();
            closer
                .pause_next(ClosePoint::AfterCloseAck, close_reg2.clone())
                .unwrap();
            let inv_reg2_rx = service
                .observe_next_invocation_for_test(SourceSqlRole::Register)
                .unwrap();
            let mut fut_reg2 = Box::pin(service.register(
                &f.clients[0],
                f.selection.as_ref().unwrap().clone(),
                SourceSessionAction::Create,
            ));
            tokio::select! {
                res = &mut fut_reg2 => panic!("register finished before close barrier: {res:?}"),
                wait = tokio::time::timeout(Duration::from_secs(10), close_reg2.arrived()) => wait.unwrap(),
            }
            drop(fut_reg2);
            let inv_reg2 = inv_reg2_rx.await.unwrap();
            let op_id2 = inv_reg2.operation_id.unwrap();
            assert_eq!(inv_reg2.sql_started_for_test(), 1);
            assert_eq!(
                inv_reg2.sql_observation().unwrap().transaction,
                TransactionKnowledge::CommitConfirmed
            );
            close_reg2.release();
            assert!(
                inv_reg2
                    .join_original_task(tokio::time::Instant::now() + Duration::from_secs(10))
                    .await
            );
            assert_eq!(
                inv_reg2.sql_observation().unwrap().cleanup,
                CleanupKnowledge::ConfirmedOriginal
            );
            assert_eq!(
                service.query(&f.clients[1], op_id2).await.unwrap().state,
                SourceSessionState::Registered
            );

            // Window 3: Query after task/receipt installed, BEFORE SQL enters.
            let phase_q1 = service.pause_next_sql_for_test(SourceSqlRole::Query).unwrap();
            let inv_q1_rx = service
                .observe_next_invocation_for_test(SourceSqlRole::Query)
                .unwrap();
            let mut fut_q1 = Box::pin(service.query(&f.clients[1], op_id1));
            tokio::select! {
                res = &mut fut_q1 => panic!("query finished before pause: {res:?}"),
                wait = tokio::time::timeout(Duration::from_secs(10), phase_q1.wait_until_entered()) => wait.unwrap(),
            }
            drop(fut_q1);
            let inv_q1 = inv_q1_rx.await.unwrap();
            assert_eq!(inv_q1.schedule_state(), SourceSqlScheduleState::Scheduled);
            assert_eq!(inv_q1.sql_started_for_test(), 0);
            phase_q1.release();
            assert!(
                inv_q1
                    .join_original_task(tokio::time::Instant::now() + Duration::from_secs(10))
                    .await
            );
            assert_eq!(inv_q1.sql_started_for_test(), 1);
            assert!(inv_q1.cleanup_for_test().is_some());

            // Window 4: Query AFTER SQL entered, BEFORE close report.
            let close_q2 = CloseBarrier::new();
            closer
                .pause_next(ClosePoint::AfterCloseAck, close_q2.clone())
                .unwrap();
            let inv_q2_rx = service
                .observe_next_invocation_for_test(SourceSqlRole::Query)
                .unwrap();
            let mut fut_q2 = Box::pin(service.query(&f.clients[1], op_id1));
            tokio::select! {
                res = &mut fut_q2 => panic!("query finished before close barrier: {res:?}"),
                wait = tokio::time::timeout(Duration::from_secs(10), close_q2.arrived()) => wait.unwrap(),
            }
            drop(fut_q2);
            let inv_q2 = inv_q2_rx.await.unwrap();
            assert_eq!(inv_q2.sql_started_for_test(), 1);
            close_q2.release();
            assert!(
                inv_q2
                    .join_original_task(tokio::time::Instant::now() + Duration::from_secs(10))
                    .await
            );

            // NeverScheduled proof for Register.
            service
                .reject_next_dispatch_for_test(
                    SourceSqlRole::Register,
                    SourceSessionError::NodeStopping,
                )
                .unwrap();
            let inv_rej_reg_rx = service
                .observe_next_invocation_for_test(SourceSqlRole::Register)
                .unwrap();
            let rej_reg = service
                .register(
                    &f.clients[0],
                    f.selection.as_ref().unwrap().clone(),
                    SourceSessionAction::Create,
                )
                .await;
            assert!(matches!(
                rej_reg,
                Err(SourceSessionFailure::Rejected(
                    SourceSessionError::NodeStopping
                ))
            ));
            let inv_rej_reg = inv_rej_reg_rx.await.unwrap();
            assert_eq!(
                inv_rej_reg.schedule_state(),
                SourceSqlScheduleState::NeverScheduled
            );
            assert_eq!(
                inv_rej_reg.rejection(),
                Some(SourceSessionError::NodeStopping)
            );
            assert!(!inv_rej_reg.task_observation().installed);
            assert_eq!(inv_rej_reg.sql_started_for_test(), 0);
            assert!(inv_rej_reg.cleanup_for_test().is_none());

            // NeverScheduled proof for Query.
            service
                .reject_next_dispatch_for_test(
                    SourceSqlRole::Query,
                    SourceSessionError::NodeStopping,
                )
                .unwrap();
            let inv_rej_q_rx = service
                .observe_next_invocation_for_test(SourceSqlRole::Query)
                .unwrap();
            let rej_q = service.query(&f.clients[1], op_id1).await;
            assert!(matches!(
                rej_q,
                Err(SourceSessionFailure::Rejected(
                    SourceSessionError::NodeStopping
                ))
            ));
            let inv_rej_q = inv_rej_q_rx.await.unwrap();
            assert_eq!(
                inv_rej_q.schedule_state(),
                SourceSqlScheduleState::NeverScheduled
            );
            assert_eq!(inv_rej_q.rejection(), Some(SourceSessionError::NodeStopping));
            assert!(!inv_rej_q.task_observation().installed);
            assert_eq!(inv_rej_q.sql_started_for_test(), 0);

            assert!(!f.area.directory.join("source-create-count").exists());
            assert!(!f.area.directory.join("source-list-count").exists());
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_m03a_real_response_commit_failure_rolls_back_and_reconcile_repairs_without_resend()
 {
    use crate::application::native_dispatch::source_sessions::SourceSqlTestFault;
    use crate::domain::native_catalog::{CleanupKnowledge, TransactionKnowledge};
    source_case("normal", |f| {
        Box::pin(async move {
            let service = f.service();
            let id = f.register(SourceSessionAction::Create).await;
            service
                .arm_next_sql_fault_for_test(
                    SourceSqlRole::Response,
                    SourceSqlTestFault::DeferredCommit,
                )
                .unwrap();
            let receipt = service.execute(&f.clients[0], id).await.unwrap();
            let err = tokio::time::timeout(Duration::from_secs(15), receipt.wait())
                .await
                .unwrap()
                .unwrap_err();
            let index = source_sql_role_index(&service, id, SourceSqlRole::Response);
            let sql_obs = service.original_sql_for_test(id).unwrap()[index].0.unwrap();
            assert_eq!(sql_obs.transaction, TransactionKnowledge::RollbackConfirmed);
            assert_eq!(sql_obs.cleanup, CleanupKnowledge::ConfirmedOriginal);
            assert!(matches!(
                err,
                SourceSessionFailure::NecessarySql(ref s)
                    if s.observation == Some(sql_obs)
            ));

            // query twice: purely read-only, DB still has response_kind IS NULL & outcome Unknown.
            for _ in 0..2 {
                let q = service.query(&f.clients[1], id).await.unwrap();
                assert_eq!(q.outcome, SourceSessionOutcome::Unknown);
                assert!(q.response.is_none());
            }

            // reconcile repairs using saved SourceWriteProof without resending RPC.
            let rec = service.reconcile(&f.clients[1], id).await.unwrap();
            assert_eq!(
                rec.response
                    .as_ref()
                    .and_then(|r| r.native_session_id.as_deref()),
                Some("source-native-created")
            );
            assert_eq!(rec.outcome, SourceSessionOutcome::Unknown);
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("source-create-count")).unwrap(),
                "1"
            );
            service.release_result(&f.clients[0], id).await.unwrap();
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_m04_unsupported_list_allows_new_and_wrong_reply_stays_unknown() {
    use crate::domain::native_dispatch::{DispatchError, NativeProtocolFailure};

    source_case("no-list-with-new", |f| {
        Box::pin(async move {
            let service = f.service();
            let disc = f
                .register(SourceSessionAction::Discover { cursor: None })
                .await;
            let err = tokio::time::timeout(
                Duration::from_secs(15),
                service.execute(&f.clients[0], disc).await.unwrap().wait(),
            )
            .await
            .unwrap()
            .unwrap_err();
            assert_eq!(
                err,
                SourceSessionFailure::Native(DispatchError::ProtocolFault(
                    NativeProtocolFailure::UnsupportedCapability
                ))
            );
            assert!(!f.area.directory.join("source-list-count").exists());

            let create = f.register(SourceSessionAction::Create).await;
            let created = tokio::time::timeout(
                Duration::from_secs(15),
                service.execute(&f.clients[0], create).await.unwrap().wait(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(created.outcome, SourceSessionOutcome::Succeeded);
            assert_eq!(
                created
                    .response
                    .as_ref()
                    .and_then(|r| r.native_session_id.as_deref()),
                Some("source-native-created")
            );
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("source-create-count")).unwrap(),
                "1"
            );
            assert!(!f.area.directory.join("source-list-count").exists());
            service.release_result(&f.clients[0], create).await.unwrap();
        })
    })
    .await;

    source_case("wrong-reply-stays-unknown", |f| {
        Box::pin(async move {
            let service = f.service();
            let create = f.register(SourceSessionAction::Create).await;
            let fact = tokio::time::timeout(
                Duration::from_secs(15),
                service.execute(&f.clients[0], create).await.unwrap().wait(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(fact.outcome, SourceSessionOutcome::Unknown);
            assert!(fact.response.is_none());
            assert_eq!(
                fact.transport,
                Some(crate::domain::native_dispatch::TransportFact::Written)
            );
            let q = service.query(&f.clients[1], create).await.unwrap();
            assert_eq!(q.outcome, SourceSessionOutcome::Unknown);
            assert!(q.response.is_none());
            assert!(!format!("{q:?}").contains("forged-sid"));
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_m05_per_chunk_auth_nonblocking_doc_foreign_owner_and_disposal() {
    use crate::application::document_disposal::{DisposalPoint, DocumentTaskIdentity};
    source_case("r2-v01-discover", |f| {
        Box::pin(async move {
            let service = f.service();
            let node = f.node.as_ref().unwrap();
            let events: Arc<Mutex<Vec<(DisposalPoint, DocumentTaskIdentity)>>> =
                Arc::new(Mutex::new(Vec::new()));
            let events_obs = events.clone();
            node.document_transfer_service()
                .unwrap()
                .install_disposal_observer_for_test(Arc::new(move |point, identity| {
                    events_obs.lock().unwrap().push((point, identity));
                }))
                .unwrap();

            let (_actor_b_id, idx_b) = source_add_paired_client(f, 73, "Reader B").await;
            let id = f
                .register(SourceSessionAction::Discover { cursor: None })
                .await;
            let fact = tokio::time::timeout(
                Duration::from_secs(15),
                service.execute(&f.clients[0], id).await.unwrap().wait(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(fact.outcome, SourceSessionOutcome::Succeeded);

            // Open reader A and reader B concurrently before A consumes final End.
            let transfer_a = service.open_result(&f.clients[0], id).await.unwrap();
            let transfer_b = service.open_result(&f.clients[idx_b], id).await.unwrap();

            // Foreign client cannot open_result.
            let mut foreign = SourceCase {
                area: TestArea::new(),
                owner: None,
                node: None,
                peers: Vec::new(),
                clients: Vec::new(),
                endpoints: Vec::new(),
                selection: None,
            };
            foreign.start("normal").await;
            assert!(service.open_result(&foreign.clients[0], id).await.is_err());
            let _ = foreign.close().await;

            // Actor A receives first chunk, then Actor A is revoked.
            let (chunk1, _) = source_read_one_chunk(f, 0, &transfer_a).await;
            assert!(!chunk1.is_empty());
            let pool = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .pool_for_test();
            let actor_a_id: String = sqlx::query_scalar(
                "SELECT client_id FROM clients WHERE display_name='Source session actor'",
            )
            .fetch_one(pool)
            .await
            .unwrap();
            crate::ipc::device_revoke(f.node.as_ref().unwrap().ipc_address(), &actor_a_id)
                .await
                .unwrap();

            // Actor A cannot fetch any further chunk.
            assert!(transfer_a.prepare_next(&f.clients[0]).await.is_err());

            // Actor B is still live with NO credit sent (AwaitCredit); this does NOT stall ACP Create.
            f.endpoints[idx_b].attach(&transfer_b).unwrap();
            assert_eq!(
                transfer_b.prepare_next(&f.clients[idx_b]).await.unwrap(),
                DocumentPrepare::AwaitCredit
            );
            let create_id = service
                .register(
                    &f.clients[idx_b],
                    f.selection.as_ref().unwrap().clone(),
                    SourceSessionAction::Create,
                )
                .await
                .unwrap()
                .id;
            let create_fact = tokio::time::timeout(
                Duration::from_secs(15),
                service
                    .execute(&f.clients[idx_b], create_id)
                    .await
                    .unwrap()
                    .wait(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(create_fact.outcome, SourceSessionOutcome::Succeeded);
            service
                .release_result(&f.clients[idx_b], create_id)
                .await
                .unwrap();

            // Actor B now grants credit and drains full original payload.
            let raw_b = source_drain_at(f, idx_b, &transfer_b, true).await;
            assert!(
                std::str::from_utf8(&raw_b)
                    .unwrap()
                    .contains("external-session-A")
            );
            drop(transfer_a);
            drop(transfer_b);

            // Final raw/typed owner disposal enters & joins on Node-owned blocking worker.
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let guard = events.lock().unwrap();
                    let entered = guard
                        .iter()
                        .filter(|(p, _)| *p == DisposalPoint::BlockingEntered)
                        .map(|(_, id)| id)
                        .collect::<Vec<_>>();
                    let joined = guard
                        .iter()
                        .filter(|(p, _)| *p == DisposalPoint::BlockingJoined)
                        .map(|(_, id)| id)
                        .collect::<Vec<_>>();
                    if !entered.is_empty() && joined.len() >= entered.len() {
                        break;
                    }
                    drop(guard);
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_m06_long_list_4097_and_partial_fail_off_receiver_loop() {
    use crate::domain::native_dispatch::DispatchError;

    source_case("list-4097", |f| {
        Box::pin(async move {
            let service = f.service();
            let decode_barrier = service.pause_next_cpu_decode_for_test().unwrap();
            let list_id = f
                .register(SourceSessionAction::Discover { cursor: None })
                .await;
            let list_receipt = service.execute(&f.clients[0], list_id).await.unwrap();
            tokio::time::timeout(Duration::from_secs(15), decode_barrier.wait_until_entered())
                .await
                .unwrap();

            // While long list decode is paused in CPU slot, ACP receiver still handles concurrent Create!
            let cpu = service.cpu_tasks_for_test(list_id).unwrap();
            assert!(cpu.iter().any(|(_, obs)| obs.installed && !obs.joined));
            assert!(service.registered_native_tasks_for_test().unwrap() > 0);
            let create_id = f.register(SourceSessionAction::Create).await;
            let create_fact = tokio::time::timeout(
                Duration::from_secs(15),
                service
                    .execute(&f.clients[0], create_id)
                    .await
                    .unwrap()
                    .wait(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(create_fact.outcome, SourceSessionOutcome::Succeeded);
            service
                .release_result(&f.clients[0], create_id)
                .await
                .unwrap();

            decode_barrier.release();
            let list_fact = tokio::time::timeout(Duration::from_secs(15), list_receipt.wait())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(list_fact.outcome, SourceSessionOutcome::Succeeded);
            let transfer = service.open_result(&f.clients[0], list_id).await.unwrap();
            let raw = source_drain(f, &transfer).await;
            let raw_text = std::str::from_utf8(&raw).unwrap();
            assert!(raw_text.contains("session-0000") && raw_text.contains("session-4096"));
            let cpu_after = service.cpu_tasks_for_test(list_id).unwrap();
            assert!(cpu_after.iter().all(|(_, obs)| obs.joined));
        })
    })
    .await;

    source_case("list-4098-partial-fail", |f| {
        Box::pin(async move {
            let service = f.service();
            let retire_barrier = service.pause_next_cpu_retire_for_test().unwrap();
            let id = f
                .register(SourceSessionAction::Discover { cursor: None })
                .await;
            let receipt = service.execute(&f.clients[0], id).await.unwrap();
            tokio::time::timeout(Duration::from_secs(15), retire_barrier.wait_until_entered())
                .await
                .unwrap();
            retire_barrier.release();
            let err = tokio::time::timeout(Duration::from_secs(15), receipt.wait())
                .await
                .unwrap()
                .unwrap_err();
            assert_eq!(
                err,
                SourceSessionFailure::Native(DispatchError::ProtocolRejected)
            );
            let q = service.query(&f.clients[1], id).await.unwrap();
            assert_eq!(q.outcome, SourceSessionOutcome::Unknown);
            assert_eq!(q.failure, Some(SourceFailureClass::Protocol));
            assert!(service.open_result(&f.clients[0], id).await.is_err());
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_m07_source_close_joins_paused_cpu_retire() {
    source_case("list-4098-partial-fail", |f| {
        Box::pin(async move {
            let service = f.service();
            let retire_barrier = service.pause_next_cpu_retire_for_test().unwrap();
            let id = f
                .register(SourceSessionAction::Discover { cursor: None })
                .await;
            let receipt = service.execute(&f.clients[0], id).await.unwrap();
            tokio::time::timeout(Duration::from_secs(15), retire_barrier.wait_until_entered())
                .await
                .unwrap();

            let native = f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .clone();
            let report_short = native
                .shutdown(tokio::time::Instant::now() + Duration::from_millis(50))
                .await;
            assert!(
                !report_short.joined,
                "source close cannot report joined while CPU retire worker is still blocked"
            );

            retire_barrier.release();
            let report_full = native
                .shutdown(tokio::time::Instant::now() + Duration::from_secs(15))
                .await;
            assert!(report_full.joined && report_full.metadata_confirmed);
            let close_status = service.source_close_for_test(id).unwrap().unwrap();
            assert!(close_status.0);
            assert_eq!(close_status.1, Some(Ok(())));
            assert_eq!(close_status.2, 0);
            assert!(receipt.wait().await.is_err());
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_m07_node_restart_preserves_written_unknown_without_rpc_resend() {
    let area = TestArea::new();
    let key = SigningKey::from_slice(&[71; 32]).unwrap();
    let launch = area
        .directory
        .canonicalize()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let child = crate::test_support::native_dispatch_dev_child(&["SOURCE_SESSION_CHILD"]);
    let child = child.to_string_lossy().into_owned();

    // Boot 1: execute Create against "exit-create" child -> WrittenUnknown persisted.
    let (client_id, op_id) = {
        let owner1 = NodeProcessOwner::begin(
            &tokio::runtime::Handle::current(),
            area.config(free_address()),
            Arc::new(FakeProtector::default()),
        );
        let node1 = owner1.ready().await.unwrap();
        let pool = node1.storage.as_ref().unwrap().pool_for_test();
        sqlx::query("INSERT INTO product_sources VALUES(?,'restart source fixture')")
            .bind(source_id(1).encode())
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces VALUES(?,?)")
            .bind(source_id(9).encode())
            .bind(&launch)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,?)")
            .bind(source_id(2).encode())
            .bind(source_id(1).encode())
            .bind("source-native-existing")
            .bind(source_id(9).encode())
            .execute(pool)
            .await
            .unwrap();
        let client_id = pair_browser(&node1, &key, "Restart actor").await;
        let observed = node1
            .management
            .as_ref()
            .unwrap()
            .observe_next_document_socket_for_test()
            .unwrap();
        let peer1 = valid_management_socket(&area, node1.listen_addr()).await;
        let mut peer1 = authenticate_socket(peer1, &key, &client_id, node_public_key(&node1)).await;
        let endpoint1 = observed.await.unwrap();
        let client1 = endpoint1.client();

        let raw = format!(
            r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}"],"environment":[]}},"recipes":[{{"wire":1,"mcp_servers":[],"additional_directories":[],"extra":{{}}}}]}}"#,
            source_utf16(&child),
            source_utf16("exit-create")
        );
        let profile =
            crate::adapters::acp::node_configuration_profile::parse_profile(raw.into_bytes())
                .unwrap();
        let draft = CatalogDraft::install_harness(
            "Restart child".into(),
            SourceSelection::Existing(source_id(1)),
            profile,
        )
        .unwrap();
        let catalog = &node1.session_services.as_ref().unwrap().catalog;
        let reg = catalog
            .register(client1.context.actor(), draft, &source_deadline())
            .await
            .unwrap();
        let res = catalog
            .apply(client1.context.actor(), reg, &source_deadline())
            .await
            .unwrap();
        let (harness, profile) = match res.fact().result().unwrap() {
            CatalogResult::Harness {
                harness, profile, ..
            } => (*harness, *profile),
            _ => panic!("expected harness"),
        };
        let selection = SourceLaunchSelection {
            source: source_id(1),
            harness,
            profile,
            workspace: source_id(9),
            launch_path: launch.clone(),
        };
        let service1 = node1.source_session_management().unwrap();
        let op_id = service1
            .register(&client1, selection, SourceSessionAction::Create)
            .await
            .unwrap()
            .id;
        let receipt = service1.execute(&client1, op_id).await.unwrap();
        let fact = tokio::time::timeout(Duration::from_secs(15), receipt.wait())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fact.outcome, SourceSessionOutcome::Unknown);
        assert_eq!(
            fact.transport,
            Some(crate::domain::native_dispatch::TransportFact::Written)
        );
        assert_eq!(
            std::fs::read_to_string(area.directory.join("source-create-count")).unwrap(),
            "1"
        );

        let _ = peer1.close(None).await;
        drop(endpoint1);
        drop(client1);
        node1.shutdown(None).await.unwrap();
        owner1.join_actual().await.unwrap();
        (client_id, op_id)
    };

    // Boot 2 on SAME TestArea: query / reconcile / execute / open_result preserve Unknown with 0 RPCs.
    {
        let owner2 = NodeProcessOwner::begin(
            &tokio::runtime::Handle::current(),
            area.config(free_address()),
            Arc::new(FakeProtector::default()),
        );
        let node2 = owner2.ready().await.unwrap();
        let observed = node2
            .management
            .as_ref()
            .unwrap()
            .observe_next_document_socket_for_test()
            .unwrap();
        let peer2 = valid_management_socket(&area, node2.listen_addr()).await;
        let mut peer2 = authenticate_socket(peer2, &key, &client_id, node_public_key(&node2)).await;
        let endpoint2 = observed.await.unwrap();
        let client2 = endpoint2.client();

        let service2 = node2.source_session_management().unwrap();
        let q = service2.query(&client2, op_id).await.unwrap();
        assert_eq!(q.outcome, SourceSessionOutcome::Unknown);
        assert_eq!(
            q.transport,
            Some(crate::domain::native_dispatch::TransportFact::Written)
        );
        assert!(q.response.is_none());
        assert!(!q.result_available);

        let r = service2.reconcile(&client2, op_id).await.unwrap();
        assert_eq!(r.outcome, SourceSessionOutcome::Unknown);
        assert!(r.response.is_none());
        assert!(service2.open_result(&client2, op_id).await.is_err());

        let ex = service2.execute(&client2, op_id).await.unwrap();
        let ex_fact = tokio::time::timeout(Duration::from_secs(10), ex.wait())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ex_fact.outcome, SourceSessionOutcome::Unknown);

        assert_eq!(
            std::fs::read_to_string(area.directory.join("source-create-count")).unwrap(),
            "1",
            "restart query/reconcile/execute must never resend session/new"
        );
        assert!(!area.directory.join("source-list-count").exists());

        let _ = peer2.close(None).await;
        drop(endpoint2);
        drop(client2);
        node2.shutdown(None).await.unwrap();
        owner2.join_actual().await.unwrap();
    }
}
