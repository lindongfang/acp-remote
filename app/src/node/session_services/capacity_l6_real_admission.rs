// Real ingress checks share only normal Node commands and a post-commit test
// pause. The pause never supplies a native response, relationship or activity.
async fn capacity_l6_register_target(
    f: &mut R3WssCase,
    native: &str,
) -> CapacityL6Result<crate::domain::EntityId> {
    let node = f.node.as_ref().ok_or("real_target_node_missing")?;
    let selection = f.selection.as_ref().ok_or("real_target_source_missing")?;
    let target = ipc::target_register(
        node.ipc_address(),
        &selection.source.encode(),
        native,
        &selection.workspace.encode(),
    )
    .await
    .map_err(|_| "real_target_registration_failed")?;
    let target = crate::domain::EntityId::decode(&target.target_id)
        .map_err(|_| "real_target_identity_invalid")?;
    capacity_l6_bind_target(f, target).await?;
    Ok(target)
}

async fn capacity_l6_release_one(f: &mut R3WssCase, native: &str) -> CapacityL6Result<()> {
    let target = capacity_l6_register_target(f, native).await?;
    let registered = capacity_l6_call(
        f,
        "native.release.register",
        serde_json::json!({"targetId":target.encode()}),
    )
    .await?;
    let operation = registered["result"]["releaseOperationId"]
        .as_str()
        .ok_or("real_release_operation_not_registered")?;
    let released = capacity_l6_call(
        f,
        "native.release",
        serde_json::json!({"releaseOperationId":operation}),
    )
    .await?;
    if released["ok"] != true || released["result"]["state"] != "released" {
        return Err("real_original_tree_release_not_confirmed");
    }
    Ok(())
}

async fn capacity_l6_release_all(f: &mut R3WssCase) -> CapacityL6Result<()> {
    let roots:Vec<String>=sqlx::query_scalar("SELECT native_id FROM node_session_capacity WHERE released=0 AND native_id IS NOT NULL ORDER BY reservation_id")
        .fetch_all(&capacity_l6_clock_pool(f)?).await.map_err(|_|"real_remaining_roots_query_failed")?;
    for root in roots {
        capacity_l6_release_one(f, &root).await?;
    }
    if capacity_l6_inspect(f).await?["occupied"] != 0 {
        return Err("real_original_tree_cleanup_did_not_coordinate");
    }
    Ok(())
}

fn capacity_l6_last_slot_view_refusal(read: &serde_json::Value) -> CapacityL6Result<()> {
    if read["ok"] == true {
        return Err("real_replay_accepted_last_slot_owned_by_create");
    }
    match read["error"]["code"].as_str() {
        Some("session_capacity_full") => Ok(()),
        Some("source_released") => Err("real_replay_released_before_last_slot_refusal"),
        Some("invalid_target") => Err("real_replay_invalid_target_at_last_slot"),
        Some("invalid_configuration") => Err("real_replay_invalid_configuration_at_last_slot"),
        Some("read_failed") => Err("real_replay_read_failed_at_last_slot"),
        Some("session_tree_unverified") => Err("real_replay_tree_unverified_at_last_slot"),
        Some("node_stopping") => Err("real_replay_node_stopping_at_last_slot"),
        Some("invalid_source") => Err("real_replay_invalid_source_at_last_slot"),
        Some("view_unavailable") => Err("real_replay_view_unavailable_at_last_slot"),
        Some("queue_rejected") => Err("real_replay_queue_rejected_at_last_slot"),
        Some("revoked") => Err("real_replay_revoked_at_last_slot"),
        Some("invalid_client") => Err("real_replay_invalid_client_at_last_slot"),
        _ => Err("real_replay_did_not_refuse_last_slot_owned_by_create"),
    }
}

async fn capacity_l6_require_protected_roots(
    f: &R3WssCase,
    roots: &[CapacityL6ClockRoot],
) -> CapacityL6Result<()> {
    for root in roots {
        if capacity_l6_clock_state(f, root).await? != (0, 0, 0) {
            return Err("real_protected_root_changed_before_last_slot");
        }
        if !capacity_l6_known_matches(
            &capacity_l6_known_work(f, &root.native)
                .map_err(|_| "real_protected_root_activity_unavailable")?,
            &root.native,
            "any_known",
        ) {
            return Err("real_protected_root_became_auto_release_candidate");
        }
    }
    Ok(())
}

async fn capacity_l6_last_slot(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    use crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint;
    if scenario.protected_root_steps.is_empty() || !c.allow_model_requests {
        return Err("real_admission_protected_work_commands_required");
    }
    let execution_commands = scenario
        .phases
        .get("last_slot_execution")
        .filter(|commands| !commands.is_empty())
        .ok_or("real_last_slot_execution_input_commands_required")?;
    if capacity_l6_inspect(f).await?["occupied"] != 0 {
        return Err("real_admission_requires_empty_owned_node_pool");
    }
    let execution = capacity_l6_new(f).await?;
    let execution_target = capacity_l6_register_target(f, &execution).await?;
    capacity_l6_release_one(f, &execution).await?;
    let replay = capacity_l6_new(f).await?;
    capacity_l6_release_one(f, &replay).await?;
    let mut originals = Vec::new();
    for _ in 0..49 {
        let native = capacity_l6_new(f).await?;
        let root = capacity_l6_clock_root(f, native).await?;
        capacity_l6_apply_commands(f, c, &root.native, &scenario.protected_root_steps).await?;
        capacity_l6_require_known(f, &root, "any_known").await?;
        originals.push(root);
    }
    capacity_l6_require_protected_roots(f, &originals).await?;
    let registered = capacity_l6_call(
        f,
        "source.session.register",
        serde_json::json!({"selection":f.selection_json(),"operation":{"kind":"create"}}),
    )
    .await?;
    let operation = match registered["result"]["sourceOperationId"].as_str() {
        Some(op) => op.to_owned(),
        None => {
            if registered["ok"] != true {
                return Err(match registered["error"]["code"].as_str() {
                    Some("session_capacity_full") => "last_slot_create_capacity_full",
                    Some("node_stopping") => "last_slot_create_node_stopping",
                    Some("invalid_configuration") => "last_slot_create_invalid_configuration",
                    Some("invalid_target") => "last_slot_create_invalid_target",
                    Some("rate_limit") => "last_slot_create_rate_limit",
                    Some("revoked") => "last_slot_create_revoked",
                    _ => "last_slot_create_not_registered",
                });
            }
            return Err("last_slot_create_not_registered");
        }
    };
    let node = f.node.as_ref().ok_or("last_slot_node_missing")?;
    let native = node
        .session_services
        .as_ref()
        .ok_or("last_slot_runtime_missing")?
        .native
        .clone();
    let barrier = node
        .storage
        .as_ref()
        .ok_or("last_slot_storage_missing")?
        .request_registry()
        .native_metadata_hooks_for_test()
        .pause_next(NativeMetadataPoint::CapacityReservedAfterCommit)
        .map_err(|_| "last_slot_cut_not_armed")?;
    let auto_release_cut = node
        .storage
        .as_ref()
        .ok_or("last_slot_storage_missing")?
        .request_registry()
        .native_metadata_hooks_for_test()
        .pause_next(NativeMetadataPoint::AutomaticReleaseBeforeBegin)
        .map_err(|_| "last_slot_auto_release_cut_not_armed")?;
    let gate = CapacityL6ReserveGate(barrier.clone());
    let request = Id128::new(crate::random::bytes().map_err(|_| "system_random_unavailable")?);
    capacity_l6_pace_command(f).await?;
    send_json(
        &mut f.peers[0].socket,
        &serde_json::json!({"v":1,"type":"command","requestId":request,
        "action":"source.session.execute","payload":{"sourceOperationId":operation}}),
    )
    .await;
    let reserve_wait = timeout(Duration::from_secs(120), barrier.wait_until_entered());
    tokio::pin!(reserve_wait);
    let execute_response = timeout(Duration::from_secs(120), async {
        loop {
            match f.peers[0].socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let response: serde_json::Value = serde_json::from_str(text.as_str())
                        .map_err(|_| "invalid_management_response")?;
                    if response["requestId"] == request.to_base64url() {
                        return Ok(response);
                    }
                    if response.get("requestId").is_some() {
                        return Err("management_response_correlation_failed");
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                Some(Ok(Message::Binary(_) | Message::Frame(_) | Message::Close(_)))
                | Some(Err(_))
                | None => return Err("last_slot_execute_socket_ended"),
            }
        }
    });
    tokio::pin!(execute_response);
    let early_response = tokio::select! {
        entered = &mut reserve_wait => {
            entered.map_err(|_| "real_committed_last_slot_not_reached")?;
            None
        }
        response = &mut execute_response => {
            Some(response.map_err(|_| "last_slot_execute_response_timeout")??)
        }
    };
    if let Some(response) = early_response {
        return Err(if response["ok"] != true {
            match response["error"]["code"].as_str() {
                Some("source_released") => "last_slot_execute_source_released",
                Some("invalid_target") => "last_slot_execute_invalid_target",
                Some("invalid_configuration") => "last_slot_execute_invalid_configuration",
                Some("read_failed") => "last_slot_execute_read_failed",
                Some("session_capacity_full") => "last_slot_execute_capacity_full",
                Some("session_tree_unverified") => "last_slot_execute_tree_unverified",
                Some("node_stopping") => "last_slot_execute_node_stopping",
                _ => "last_slot_execute_rejected",
            }
        } else {
            "last_slot_execute_responded_before_reservation"
        });
    }
    let reserved:Vec<String>=sqlx::query_scalar("SELECT reservation_id FROM node_session_capacity WHERE native_id IS NULL AND released=0 AND physically_closed=0")
        .fetch_all(&capacity_l6_clock_pool(f)?).await.map_err(|_|"last_slot_reserved_identity_query_failed")?;
    if reserved.len() != 1
        || native
            .inspect_capacity()
            .await
            .map_err(|_| "last_slot_capacity_unavailable")?
            .occupied
            != 50
    {
        return Err("last_slot_not_one_real_native_none_reservation");
    }
    // The first socket awaits its own Create. Use an independent authenticated
    // socket for the two losing load attempts; always restore the local index.
    f.peers.swap(0, 1);
    let losing = async {
        capacity_l6_require_protected_roots(f, &originals).await?;
        let read = {
            let read = capacity_l6_call(
                f,
                "view.open_source",
                serde_json::json!({
                    "selection": f.selection_json(),
                    "nativeSessionId": replay,
                    "reason": "explicit_refresh"
                }),
            );
            tokio::pin!(read);
            let entered = auto_release_cut.wait_until_entered();
            tokio::pin!(entered);
            tokio::select! {
                () = &mut entered => {
                    auto_release_cut.release();
                    let _ = timeout(Duration::from_secs(120), &mut read)
                        .await
                        .map_err(|_| "real_replay_last_slot_auto_release_view_timeout")?;
                    return Err("real_protected_root_became_auto_release_candidate");
                }
                read = &mut read => read?,
            }
        };
        auto_release_cut.release();
        capacity_l6_last_slot_view_refusal(&read)?;
        capacity_l6_apply_commands(f, c, &execution, execution_commands).await?;
        let queued = capacity_l6_call(
            f,
            "input.queue",
            serde_json::json!({"targetId":execution_target.encode()}),
        )
        .await?;
        let state = &queued["result"];
        if queued["ok"] != true
            || !state["active"].is_null()
            || !state["priority"].is_null()
            || !state["waiting"]
                .as_array()
                .is_some_and(|waiting| waiting.len() == 1)
        {
            return Err("real_execution_last_slot_attempt_not_one_accepted_input");
        }
        match native.ensure_target(execution_target).await {
            Err(crate::domain::native_dispatch::DispatchError::SessionCapacityFull) => {}
            Err(crate::domain::native_dispatch::DispatchError::InvalidTarget) => {
                return Err("real_execution_target_not_bound");
            }
            Err(crate::domain::native_dispatch::DispatchError::InvalidConfiguration) => {
                return Err("real_execution_configuration_not_bound");
            }
            _ => return Err("real_execution_did_not_refuse_last_slot_owned_by_create"),
        }
        Ok::<(), &'static str>(())
    }
    .await;
    f.peers.swap(0, 1);
    losing?;
    drop(gate);
    // Correlate the outstanding command instead of issuing another execute.
    let actual = timeout(Duration::from_secs(90), async {
        loop {
            match f.peers[0].socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let response: serde_json::Value = serde_json::from_str(text.as_str())
                        .map_err(|_| "last_slot_invalid_response")?;
                    if response["requestId"] == request.to_base64url() {
                        break Ok(response);
                    }
                    if response.get("requestId").is_some() {
                        break Err("last_slot_response_wrong_request");
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                _ => break Err("last_slot_original_socket_ended"),
            }
        }
    })
    .await
    .map_err(|_| "last_slot_original_response_timeout")??;
    let created = actual["result"]["response"]["nativeSessionId"]
        .as_str()
        .ok_or("last_slot_actual_new_identity_missing")?;
    if actual["result"]["outcome"] != "succeeded" {
        return Err("last_slot_original_create_not_confirmed");
    }
    let bound: Option<(String, i64, i64)> = sqlx::query_as(
        "SELECT native_id,root_verified,released FROM node_session_capacity WHERE reservation_id=?",
    )
    .bind(&reserved[0])
    .fetch_optional(&capacity_l6_clock_pool(f)?)
    .await
    .map_err(|_| "last_slot_original_binding_query_failed")?;
    if bound != Some((created.to_owned(), 1, 0)) {
        return Err("last_slot_response_not_bound_to_original_reservation");
    }
    for root in &originals {
        if capacity_l6_clock_state(f, root).await? != (0, 0, 0) {
            return Err("last_slot_attempt_changed_other_original_tree");
        }
    }
    // The subsequent successful routes are sequential recovery checks, not
    // additional winners of the already decided concurrent last-slot request.
    capacity_l6_release_one(f, created).await?;
    // The execution input was already accepted above. Re-submitting it here
    // creates a second queue item even though only one load attempt was refused.
    // Trigger the accepted item's real load through the same normal admission
    // path instead.
    native
        .ensure_target(execution_target)
        .await
        .map_err(|error| match error {
            crate::domain::native_dispatch::DispatchError::SessionCapacityFull => {
                "real_execution_accepted_work_still_full_after_release"
            }
            crate::domain::native_dispatch::DispatchError::InvalidTarget => {
                "real_execution_accepted_target_lost_after_release"
            }
            crate::domain::native_dispatch::DispatchError::InvalidConfiguration => {
                "real_execution_configuration_invalid_after_release"
            }
            _ => "real_execution_accepted_work_load_failed_after_release",
        })?;
    timeout(Duration::from_secs(90),async{
        loop {
            let loaded:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_resource_bindings WHERE target_id=? AND lifecycle='loaded' AND load_transport_state='written'")
                .bind(execution_target.encode()).fetch_one(&capacity_l6_clock_pool(f)?).await.map_err(|_|"real_execution_loaded_query_failed")?;
            if loaded==1{break Ok::<(),&'static str>(());}
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.map_err(|_|"real_execution_positive_not_durably_loaded")??;
    let executed = capacity_l6_clock_root(f, execution.clone()).await?;
    if capacity_l6_inspect(f).await?["occupied"] != 50 {
        return Err("real_execution_positive_did_not_reserve_fiftieth_tree");
    }
    // The original accepted executions must settle before this observer starts
    // an explicit close and a different original tree's replay. A durably loaded
    // slot alone does not establish that those queued jobs have completed.
    match timeout(Duration::from_secs(90), async {
        loop {
            let queue = capacity_l6_call(
                f,
                "input.queue",
                serde_json::json!({"targetId":execution_target.encode()}),
            )
            .await?;
            if queue["ok"] != true {
                return Err("real_execution_queue_not_readable");
            }
            let state = &queue["result"];
            if state["processing"].is_null()
                && state["active"].is_null()
                && state["priority"].is_null()
                && state["waiting"].as_array().is_some_and(Vec::is_empty)
            {
                return Ok::<(), &'static str>(());
            }
        }
    })
    .await
    {
        Ok(result) => result?,
        Err(_) => {
            if let Ok(queue) = capacity_l6_call(
                f,
                "input.queue",
                serde_json::json!({"targetId":execution_target.encode()}),
            )
            .await
                && queue["ok"] == true
            {
                let state = &queue["result"];
                steps.push(serde_json::json!({
                    "step": "cp011_last_slot_queue_diagnostic",
                    "processing": state["processing"].is_null().then_some("none").or_else(
                        || state["processing"]["phase"].as_str()).unwrap_or("unknown"),
                    "active": !state["active"].is_null(),
                    "priority": !state["priority"].is_null(),
                    "waiting": state["waiting"].as_array().map_or(0, Vec::len),
                    "failureSuspended": state["failureSuspended"].clone(),
                    "stopReason": state["stopReason"].clone(),
                    "priorityFailure": state["priorityFailure"].clone(),
                    "activeItem": capacity_l6_queue_entry_metadata(&state["active"]),
                    "waitingItems": state["waiting"].as_array().map(|waiting| {
                        waiting.iter().map(capacity_l6_queue_entry_metadata).collect::<Vec<_>>()
                    }).unwrap_or_default(),
                }));
            }
            return Err("real_execution_accepted_work_not_settled_before_release");
        }
    }
    // Observe the accepted queue's original durable load. ensure_target is an
    // execution preparation API requiring a currently accepted queue item; a
    // completed input no longer supplies that prerequisite for a second call.
    let inserted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM node_native_dispatch_facts f JOIN node_native_resource_bindings b \
         ON b.load_epoch=f.load_epoch AND b.target_id=f.target_id WHERE b.target_id=? \
         AND b.lifecycle='loaded' AND b.load_transport_state='written' \
         AND f.fact_kind IN ('v2_inserted','v1_prompt_result')",
    )
    .bind(execution_target.encode())
    .fetch_one(&capacity_l6_clock_pool(f)?)
    .await
    .map_err(|_| "real_execution_original_dispatch_fact_query_failed")?;
    if inserted == 0 {
        return Err("real_execution_original_input_not_confirmed");
    }
    capacity_l6_release_one(f, &execution).await?;
    let view = capacity_l6_view_records(f, &replay, "explicit_refresh").await?;
    let replayed = capacity_l6_clock_root(f, replay.clone()).await?;
    if !view.actual_complete
        || capacity_l6_inspect(f).await?["occupied"] != 50
        || executed.reservation == replayed.reservation
    {
        return Err("real_replay_positive_did_not_complete_its_own_fiftieth_reservation");
    }
    steps.push(serde_json::json!({"step":"real_last_slot_original_create_then_sequential_execution_replay",
        "createReservation":reserved[0],"executionReservation":executed.reservation,
        "replayReservation":replayed.reservation,"actualRoots":50,
        "boundary":"owned Node post-commit pause, real Harness new/resume/read; success routes sequential"}));
    capacity_l6_mark(steps, "cp011", "real_fifty_and_three_ingress_last_slot")?;
    capacity_l6_release_all(f).await
}

fn capacity_l6_queue_entry_metadata(entry: &serde_json::Value) -> serde_json::Value {
    if entry.is_null() {
        return serde_json::Value::Null;
    }
    serde_json::json!({
        "itemId": entry["itemId"].clone(),
        "revision": entry["revision"].clone(),
        "suspended": entry["suspended"].clone(),
        "submitterId": entry["submitterId"].clone(),
        "editorId": entry["editorId"].clone(),
    })
}
