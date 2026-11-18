// The native sessions and IO are real. Only the requested original automatic
// Close and owned Node metadata transactions receive test faults.
async fn capacity_l6_add_real_source(
    f: &R3WssCase,
    c: &CapacityL6Config,
) -> CapacityL6Result<SourceLaunchSelection> {
    let node = f.node.as_ref().ok_or("fault_original_node_missing")?;
    let selected = f
        .selection
        .as_ref()
        .ok_or("fault_original_selection_missing")?;
    let peer = f.peers.first().ok_or("fault_original_actor_missing")?;
    let address = node.listen_addr();
    let origin = format!("https://localhost:{}", address.port());
    let observed = node
        .management
        .as_ref()
        .ok_or("fault_management_missing")?
        .observe_next_document_socket_for_test()
        .map_err(|_| "fault_actor_observer_failed")?;
    let (socket, response) = websocket_exchange(
        &f.area,
        address,
        &format!("localhost:{}", address.port()),
        &[
            ("Origin", &origin),
            ("Sec-WebSocket-Protocol", CONTROL_PROTOCOL),
        ],
    )
    .await
    .map_err(|_| "fault_catalog_socket_failed")?;
    if response.status() != 101 {
        return Err("fault_catalog_upgrade_failed");
    }
    let mut socket = capacity_l6_auth(
        socket,
        &peer.key,
        &peer.client_id,
        node_public_key(node),
        &origin,
    )
    .await?;
    let endpoint = observed
        .await
        .map_err(|_| "fault_catalog_actor_not_observed")?;
    let client = endpoint.client();
    let profile = crate::adapters::acp::node_configuration_profile::parse_profile(
        std::fs::read(&c.launch_profile).map_err(|_| "fault_profile_not_readable")?,
    )
    .map_err(|_| "fault_profile_invalid")?;
    let draft = CatalogDraft::install_harness(
        "Real Harness dedicated fault source".into(),
        SourceSelection::New {
            name: "Real Harness isolated ticket fault".into(),
        },
        profile,
    )
    .map_err(|_| "fault_catalog_draft_invalid")?;
    let catalog = &node
        .session_services
        .as_ref()
        .ok_or("fault_services_missing")?
        .catalog;
    let registered = catalog
        .register(client.context.actor(), draft, &r3_deadline())
        .await
        .map_err(|_| "fault_catalog_registration_failed")?;
    let applied = catalog
        .apply(client.context.actor(), registered, &r3_deadline())
        .await
        .map_err(|_| "fault_catalog_apply_failed")?;
    let (source, harness, profile) = match applied.fact().result() {
        Some(CatalogResult::Harness {
            source,
            harness,
            profile,
        }) => (*source, *harness, *profile),
        _ => return Err("fault_catalog_original_fact_missing"),
    };
    socket
        .close(None)
        .await
        .map_err(|_| "fault_catalog_socket_close_failed")?;
    Ok(SourceLaunchSelection {
        source,
        harness,
        profile,
        workspace: selected.workspace,
        launch_path: selected.launch_path.clone(),
    })
}

async fn capacity_l6_original_ticket_faults(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    case: &str,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    use crate::adapters::acp::capacity_close_write_fault::CloseWriteFaultMode;
    use crate::application::native_dispatch::capacity_close_write_fault_probe;
    use crate::application::native_dispatch::runtime_child_source_clock_probe::rt_source_candidate_clocks;
    if scenario.protected_root_steps.is_empty() || !c.allow_model_requests {
        return Err("ticket_fault_real_healthy_source_protection_required");
    }
    // An independent healthy source remains live while each fault source fails.
    // Sharing a connection would make the healthy-tree assertion meaningless.
    capacity_l6_release_all(f).await?;
    let healthy_selection = f
        .selection
        .clone()
        .ok_or("fault_healthy_selection_missing")?;
    let mut healthy = Vec::new();
    for _ in 0..49 {
        let id = capacity_l6_new(f).await?;
        let root = capacity_l6_clock_root(f, id).await?;
        capacity_l6_apply_commands(f, c, &root.native, &scenario.protected_root_steps).await?;
        capacity_l6_require_known(f, &root, "any_known").await?;
        healthy.push(root);
    }
    for (label, mode) in [
        ("zero", CloseWriteFaultMode::BeforeWrite),
        ("partial", CloseWriteFaultMode::AfterOneByte),
        ("after_frame_unknown", CloseWriteFaultMode::BeforeFlush),
    ] {
        f.selection = Some(capacity_l6_add_real_source(f, c).await?);
        let fault_result = async {
            let id = capacity_l6_new(f).await?;
            let root = capacity_l6_clock_root(f, id).await?;
            capacity_l6_clock_eligible(f, &root).await?;
            let runtime = f.node.as_ref().and_then(|node|node.session_services.as_ref())
                .ok_or("fault_native_runtime_missing")?.native.clone();
            let healthy_clocks=rt_source_candidate_clocks(&runtime)
                .map_err(|_|"fault_healthy_candidate_read_failed")?;
            for original in &healthy {
                if capacity_l6_clock_state(f, original).await? != (0, 0, 0)
                    || !healthy_clocks.iter().any(|(source,native,since)|
                        *source==healthy_selection.source && native.as_ref()==original.native && since.is_none()) {
                    return Err("fault_healthy_source_not_original_protected_owner");
                }
            }
            let source = f.selection.as_ref().ok_or("fault_source_missing")?.source;
            let pool = capacity_l6_clock_pool(f)?;
            let connection: String = sqlx::query_scalar(
                "SELECT connection_epoch FROM node_native_source_instances WHERE instance_id=? AND source_id=?")
                .bind(&root.instance).bind(source.encode()).fetch_one(&pool).await
                .map_err(|_| "fault_original_connection_query_failed")?;
            let instance = crate::domain::EntityId::decode(&root.instance)
                .map_err(|_| "fault_original_instance_invalid")?;
            let connection = crate::domain::EntityId::decode(&connection)
                .map_err(|_| "fault_original_connection_invalid")?;
            let guard = capacity_close_write_fault_probe::install(
                &runtime, source, instance, connection, &root.native, mode,
            ).map_err(|_| "fault_original_close_registration_failed")?;
            // The only interpolated values are decoded, fixed-length Node IDs.
            // No profile, credential or content is included in SQL or evidence.
            sqlx::query(&format!(
                "CREATE TRIGGER l6_ticket_source_fault BEFORE UPDATE OF lifecycle ON node_native_source_instances WHEN NEW.instance_id='{}' AND NEW.lifecycle='closed' BEGIN SELECT RAISE(ABORT,'owned original source close fault'); END",instance.encode()))
                .execute(&pool).await.map_err(|_|"fault_source_commit_gate_failed")?;
            sqlx::query(&format!(
                "CREATE TRIGGER l6_ticket_finish_fault BEFORE UPDATE OF phase ON node_session_automatic_releases WHEN NEW.anchor_reservation='{}' AND NEW.phase='finished' BEGIN SELECT RAISE(ABORT,'owned original finish fault'); END",root.reservation))
                .execute(&pool).await.map_err(|_|"fault_finish_commit_gate_failed")?;
            let attempt = capacity_l6_operation(f, serde_json::json!({"kind":"create"})).await?;
            let observed = guard.observation();
            let rows:Vec<(String,String,Option<String>)>=sqlx::query_as(
                "SELECT operation_id,phase,outcome FROM node_session_automatic_releases WHERE anchor_reservation=? AND reason='capacity_pressure'")
                .bind(&root.reservation).fetch_all(&pool).await.map_err(|_|"fault_original_operation_query_failed")?;
            if !observed.claimed || !observed.fired || observed.total==0 || rows.len()!=1
                || attempt["result"]["outcome"]=="succeeded"
                || capacity_l6_inspect(f).await?["occupied"]!=50 {
                return Err("fault_original_ticket_or_occupied_window_not_confirmed");
            }
            let (operation,phase,outcome)=&rows[0];
            let expected_zero=label=="zero";
            if (expected_zero && (observed.accepted!=0 || phase!="withdrawn" || outcome.as_deref()!=Some("withdrawn")))
                || (!expected_zero && (outcome.as_deref()!=Some("unknown") || phase!="pending"))
                || (label=="partial" && (observed.accepted!=1 || observed.accepted>=observed.total))
                || (label=="after_frame_unknown" && observed.accepted!=observed.total) {
                return Err("fault_original_ticket_outcome_does_not_match_actual_acceptance");
            }
            let original: (i64,i64)=sqlx::query_as(
                "SELECT physically_closed,released FROM node_session_capacity WHERE reservation_id=? AND instance_id=?")
                .bind(&root.reservation).bind(&root.instance).fetch_one(&pool).await
                .map_err(|_|"fault_original_reservation_query_failed")?;
            if original.1!=0 || (expected_zero && original.0!=0) {
                return Err("unconfirmed_original_ticket_was_refunded");
            }
            for other in &healthy {
                if capacity_l6_clock_state(f, other).await? != (0,0,0) {
                    return Err("original_close_fault_changed_healthy_source_tree");
                }
            }
            steps.push(serde_json::json!({"step":"real_harness_original_close_node_ticket_fault",
                "case":case,"fault":label,"sourceId":source.encode(),"nativeId":root.native,
                "instanceId":root.instance,"connectionEpoch":connection.encode(),
                "reservation":root.reservation,"operationId":operation,
                "accepted":observed.accepted,"total":observed.total,"phase":phase,
                "outcome":outcome,"physicallyClosed":original.0,"released":false,
                "occupied":50,"healthyOriginalTrees":49,
                "evidenceBasis":"Node-owned actual transport fault; no synthetic Harness response"}));
            drop(guard);
            sqlx::query("DROP TRIGGER l6_ticket_source_fault").execute(&pool).await
                .map_err(|_|"fault_source_gate_remove_failed")?;
            sqlx::query("DROP TRIGGER l6_ticket_finish_fault").execute(&pool).await
                .map_err(|_|"fault_finish_gate_remove_failed")?;
            if expected_zero {
                capacity_l6_release_one(f,&root.native).await?;
            } else {
                capacity_close_write_fault_probe::request_metadata_retry(
                    &runtime,source,instance,connection,
                ).map_err(|_|"fault_original_metadata_retry_not_requested")?;
            }
            timeout(Duration::from_secs(90),async {
                loop {
                    let released:i64=sqlx::query_scalar(
                        "SELECT released FROM node_session_capacity WHERE reservation_id=?")
                        .bind(&root.reservation).fetch_one(&pool).await
                        .map_err(|_|"fault_original_reconciliation_query_failed")?;
                    if released==1 {break Ok::<(),&'static str>(());}
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }).await.map_err(|_|"fault_original_reliable_closure_not_reconciled")??;
            if capacity_l6_inspect(f).await?["occupied"]!=49 {
                return Err("fault_original_closure_did_not_refund_exactly_one");
            }
            Ok(())
        }.await;
        // Restore the selection even if an actual Harness/IO fact is missing.
        f.selection = Some(healthy_selection.clone());
        fault_result?;
    }
    capacity_l6_mark(
        steps,
        case,
        if case == "cp011" {
            "failed_unknown_close_keeps_slot"
        } else {
            "node_zero_partial_unknown_ticket_boundaries"
        },
    )?;
    capacity_l6_release_all(f).await
}
