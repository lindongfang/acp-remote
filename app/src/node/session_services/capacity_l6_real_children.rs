// Real-Harness CP023 orchestration. No ACP notification, relationship, owner,
// capacity fact or task completion is manufactured by this entry.
#[derive(Clone)]
struct CapacityL6ChildRoot {
    native: String,
    reservation: String,
    instance: String,
    epoch: String,
}

type CapacityL6ChildSchemaRow = (String, String, String, i64, Option<String>);

struct CapacityL6RelationFault {
    database: std::path::PathBuf,
    schema: Vec<CapacityL6ChildSchemaRow>,
    control: crate::ipc::IpcControl,
    failure_marker: std::path::PathBuf,
    armed: bool,
}

// This synchronous, joined thread is only the cancellation fallback for a
// test-owned SQL schema fault. It cannot outlive the guard or become a detached
// cleanup task. SQLite busy waits and the complete future both have deadlines.
fn capacity_l6_child_schema_action(
    database: std::path::PathBuf,
    original: Vec<CapacityL6ChildSchemaRow>,
    restore: bool,
) -> CapacityL6Result<()> {
    std::thread::Builder::new()
        .name("capacity-l6-relation-schema".into())
        .spawn(move || {
        use sqlx::{ConnectOptions as _, Connection as _};
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "child_relation_restore_runtime_unavailable")?;
        runtime.block_on(async { timeout(Duration::from_secs(12), async {
            let mut connection = sqlx::sqlite::SqliteConnectOptions::new()
                .filename(database)
                .create_if_missing(false)
                .busy_timeout(Duration::from_secs(2))
                .connect()
                .await
                .map_err(|_| "child_relation_owned_database_unavailable")?;
            if restore {
                let tables: Vec<String> = sqlx::query_scalar(
                    "SELECT name FROM main.sqlite_schema WHERE type='table' AND name IN ('node_session_parents','l6_owned_unavailable_parents') ORDER BY name",
                )
                .fetch_all(&mut connection)
                .await
                .map_err(|_| "child_relation_restore_table_inventory_failed")?;
                if tables == ["l6_owned_unavailable_parents"] {
                    sqlx::query(
                        "ALTER TABLE main.l6_owned_unavailable_parents RENAME TO node_session_parents",
                    )
                    .execute(&mut connection)
                    .await
                    .map_err(|_| "child_relation_restore_rename_failed")?;
                } else if tables != ["node_session_parents"] {
                    return Err("child_relation_restore_ambiguous_schema_stop");
                }
                let now: Vec<CapacityL6ChildSchemaRow> = sqlx::query_as(
                    "SELECT type,name,tbl_name,rootpage,sql FROM main.sqlite_schema ORDER BY type,name",
                )
                .fetch_all(&mut connection)
                .await
                .map_err(|_| "child_relation_restore_schema_unavailable")?;
                // ALTER may quote names in the stored SQL, including dependent
                // views/FKs. Keep every object/rootpage and restore only the
                // exact original SQL text, never relation rows or owner facts.
                if now.len() != original.len()
                    || now.iter().zip(&original).any(|(actual, expected)| {
                        actual.0 != expected.0
                            || actual.1 != expected.1
                            || actual.2 != expected.2
                            || actual.3 != expected.3
                    })
                {
                    return Err("child_relation_restore_schema_objects_changed_stop");
                }
                sqlx::query("PRAGMA writable_schema=ON")
                    .execute(&mut connection)
                    .await
                    .map_err(|_| "child_relation_exact_schema_restore_unavailable")?;
                let mut transaction = connection
                    .begin()
                    .await
                    .map_err(|_| "child_relation_schema_restore_transaction_failed")?;
                for (actual, expected) in now.iter().zip(&original) {
                    if actual.4 != expected.4 {
                        let affected = sqlx::query(
                            "UPDATE main.sqlite_schema SET sql=? WHERE type=? AND name=? AND tbl_name=? AND rootpage=?",
                        )
                        .bind(&expected.4)
                        .bind(&expected.0)
                        .bind(&expected.1)
                        .bind(&expected.2)
                        .bind(expected.3)
                        .execute(&mut *transaction)
                        .await
                        .map_err(|_| "child_relation_exact_schema_restore_failed")?
                        .rows_affected();
                        if affected != 1 {
                            return Err("child_relation_schema_restore_not_unique");
                        }
                    }
                }
                transaction
                    .commit()
                    .await
                    .map_err(|_| "child_relation_schema_restore_commit_unconfirmed")?;
                sqlx::query("PRAGMA writable_schema=OFF")
                    .execute(&mut connection)
                    .await
                    .map_err(|_| "child_relation_schema_write_mode_not_closed")?;
                let version: i64 = sqlx::query_scalar("PRAGMA schema_version")
                    .fetch_one(&mut connection)
                    .await
                    .map_err(|_| "child_relation_schema_version_unavailable")?;
                let next = version
                    .checked_add(1)
                    .filter(|v| *v <= i64::from(i32::MAX))
                    .ok_or("child_relation_schema_version_exhausted")?;
                sqlx::query(&format!("PRAGMA schema_version={next}"))
                    .execute(&mut connection)
                    .await
                    .map_err(|_| "child_relation_schema_invalidation_failed")?;
                let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
                    .fetch_one(&mut connection)
                    .await
                    .map_err(|_| "child_relation_integrity_unavailable")?;
                if integrity != "ok" {
                    return Err("child_relation_restored_integrity_failed");
                }
                if !sqlx::query("PRAGMA foreign_key_check")
                    .fetch_all(&mut connection)
                    .await
                    .map_err(|_| "child_relation_foreign_key_check_unavailable")?
                    .is_empty()
                {
                    return Err("child_relation_restored_foreign_key_check_failed");
                }
                let checked: Vec<CapacityL6ChildSchemaRow> = sqlx::query_as(
                    "SELECT type,name,tbl_name,rootpage,sql FROM main.sqlite_schema ORDER BY type,name",
                )
                .fetch_all(&mut connection)
                .await
                .map_err(|_| "child_relation_restored_schema_unavailable")?;
                if checked != original {
                    return Err("child_relation_restored_schema_not_exact");
                }
            } else {
                sqlx::query(
                    "ALTER TABLE main.node_session_parents RENAME TO l6_owned_unavailable_parents",
                )
                .execute(&mut connection)
                .await
                .map_err(|_| "child_relation_fault_install_failed")?;
            }
            connection
                .close()
                .await
                .map_err(|_| "child_relation_owned_connection_close_failed")
        }).await.map_err(|_| "child_relation_owned_sql_deadline")? })
    })
    .map_err(|_| "child_relation_owned_thread_start_failed")?
    .join()
    .map_err(|_| "child_relation_owned_thread_join_failed")?
}

impl CapacityL6RelationFault {
    async fn install(f: &R3WssCase) -> CapacityL6Result<Self> {
        let pool = capacity_l6_child_pool(f)?;
        let database = pool.connect_options().get_filename().to_path_buf();
        capacity_l6_web_owned_path(&database)?;
        let owned = f
            .area
            .directory
            .canonicalize()
            .map_err(|_| "child_relation_owned_node_directory_missing")?;
        let resolved = database
            .canonicalize()
            .map_err(|_| "child_relation_owned_database_missing")?;
        if !resolved.starts_with(owned) {
            return Err("child_relation_fault_outside_this_owned_node_rejected");
        }
        let schema: Vec<CapacityL6ChildSchemaRow> = sqlx::query_as(
            "SELECT type,name,tbl_name,rootpage,sql FROM main.sqlite_schema ORDER BY type,name",
        )
        .fetch_all(&pool)
        .await
        .map_err(|_| "child_relation_original_schema_unavailable")?;
        if schema
            .iter()
            .filter(|row| row.0 == "table" && row.1 == "node_session_parents")
            .count()
            != 1
            || schema
                .iter()
                .any(|row| row.1 == "l6_owned_unavailable_parents")
        {
            return Err("child_relation_original_table_not_unique");
        }
        let mut guard = Self {
            database: resolved,
            schema,
            control: f
                .node
                .as_ref()
                .ok_or("child_relation_original_node_missing")?
                .control
                .clone(),
            failure_marker: f.area.directory.join("l6-relation-restore-failed.json"),
            armed: true,
        };
        let installed =
            capacity_l6_child_schema_action(guard.database.clone(), guard.schema.clone(), false);
        if let Err(error) = installed {
            guard.restore()?;
            return Err(error);
        }
        Ok(guard)
    }

    fn restore(&mut self) -> CapacityL6Result<()> {
        if self.armed {
            capacity_l6_child_schema_action(self.database.clone(), self.schema.clone(), true)?;
            self.armed = false;
        }
        Ok(())
    }
}

impl Drop for CapacityL6RelationFault {
    fn drop(&mut self) {
        if self.restore().is_err() {
            self.control.request_stop();
            let _ = std::fs::write(
                &self.failure_marker,
                br#"{"restorationConfirmed":false,"originalNodeStopRequested":true}"#,
            );
        }
    }
}

async fn capacity_l6_child_relation_rows(f: &R3WssCase) -> CapacityL6Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT json_array(source_id,child_native_id,parent_native_id,observed_instance,runtime_epoch,connection_epoch,hex(sequence),hex(receipt),hex(fingerprint),observed_at_ms) FROM node_session_parents ORDER BY source_id,child_native_id",
    )
    .fetch_all(&capacity_l6_child_pool(f)?)
    .await
    .map_err(|_| "child_original_relationship_proof_unavailable")
}

fn capacity_l6_child_pool(f: &R3WssCase) -> CapacityL6Result<sqlx::SqlitePool> {
    f.node
        .as_ref()
        .and_then(|node| node.storage.as_ref())
        .map(|storage| storage.pool_for_test().clone())
        .ok_or("child_original_storage_missing")
}

fn capacity_l6_child_source(f: &R3WssCase) -> CapacityL6Result<String> {
    f.selection
        .as_ref()
        .map(|selection| selection.source.encode())
        .ok_or("child_original_source_missing")
}

async fn capacity_l6_child_root(
    f: &R3WssCase,
    native: &str,
) -> CapacityL6Result<CapacityL6ChildRoot> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT reservation_id,instance_id,runtime_epoch FROM node_session_capacity WHERE source_id=? AND native_id=? AND root_verified=1 AND released=0",
    )
    .bind(capacity_l6_child_source(f)?)
    .bind(native)
    .fetch_all(&capacity_l6_child_pool(f)?)
    .await
    .map_err(|_| "child_original_reservation_query_failed")?;
    if rows.len() != 1 {
        return Err("child_requires_one_actual_verified_root_reservation");
    }
    let (reservation, instance, epoch) = rows
        .into_iter()
        .next()
        .ok_or("child_original_reservation_missing")?;
    Ok(CapacityL6ChildRoot {
        native: native.into(),
        reservation,
        instance,
        epoch,
    })
}

async fn capacity_l6_child_state(
    f: &R3WssCase,
    root: &CapacityL6ChildRoot,
) -> CapacityL6Result<(i64, i64)> {
    sqlx::query_as(
        "SELECT physically_closed,released FROM node_session_capacity WHERE reservation_id=? AND source_id=? AND native_id=? AND instance_id=? AND runtime_epoch=?",
    )
    .bind(&root.reservation)
    .bind(capacity_l6_child_source(f)?)
    .bind(&root.native)
    .bind(&root.instance)
    .bind(&root.epoch)
    .fetch_one(&capacity_l6_child_pool(f)?)
    .await
    .map_err(|_| "child_original_generation_state_unavailable")
}

async fn capacity_l6_child_inventory(f: &R3WssCase) -> CapacityL6Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT json_array(reservation_id,source_id,native_id,instance_id,runtime_epoch,root_verified,physically_closed,released) FROM node_session_capacity ORDER BY reservation_id",
    )
    .fetch_all(&capacity_l6_child_pool(f)?)
    .await
    .map_err(|_| "child_capacity_inventory_unavailable")
}

fn capacity_l6_child_candidate(f: &R3WssCase, native: &str) -> CapacityL6Result<bool> {
    use crate::application::native_dispatch::runtime_child_source_clock_probe::rt_source_candidate_clocks;
    let services = f
        .node
        .as_ref()
        .and_then(|node| node.session_services.as_ref())
        .ok_or("child_original_runtime_missing")?;
    let source = f.selection.as_ref().ok_or("child_source_missing")?.source;
    let samples: Vec<_> = rt_source_candidate_clocks(&services.native)
        .map_err(|_| "child_candidate_probe_unavailable")?
        .into_iter()
        .filter(|(actual_source, actual_native, _)| {
            *actual_source == source && actual_native.as_ref() == native
        })
        .collect();
    if samples.len() != 1 {
        return Err("child_original_candidate_generation_ambiguous");
    }
    Ok(samples[0].2.is_some())
}

async fn capacity_l6_child_live_unchanged(
    f: &R3WssCase,
    roots: &[CapacityL6ChildRoot],
) -> CapacityL6Result<()> {
    for root in roots {
        if capacity_l6_child_state(f, root).await? != (0, 0) {
            return Err("child_operation_changed_other_original_tree");
        }
    }
    Ok(())
}

async fn capacity_l6_child_release(
    f: &mut R3WssCase,
    root: &CapacityL6ChildRoot,
) -> CapacityL6Result<String> {
    let node = f.node.as_ref().ok_or("child_node_missing")?;
    let selection = f.selection.as_ref().ok_or("child_selection_missing")?;
    let target = ipc::target_register(
        node.ipc_address(),
        &selection.source.encode(),
        &root.native,
        &selection.workspace.encode(),
    )
    .await
    .map_err(|_| "child_root_target_registration_failed")?
    .target_id;
    let registered = capacity_l6_call(
        f,
        "native.release.register",
        serde_json::json!({"targetId":target}),
    )
    .await?;
    if registered["ok"] != true {
        return Err("child_root_release_registration_refused");
    }
    let operation = registered["result"]["releaseOperationId"]
        .as_str()
        .ok_or("child_root_release_operation_missing")?
        .to_owned();
    let result = capacity_l6_call(
        f,
        "native.release",
        serde_json::json!({"releaseOperationId":operation}),
    )
    .await?;
    if result["ok"] != true || result["result"]["state"] != "released" {
        return Err("child_root_close_and_necessary_join_not_confirmed");
    }
    if capacity_l6_child_state(f, root).await? != (1, 1) {
        return Err("child_root_original_reservation_not_reconciled");
    }
    Ok(operation)
}

async fn capacity_l6_child_relations(
    f: &mut R3WssCase,
    root: &str,
    count: usize,
) -> CapacityL6Result<Vec<String>> {
    let discovery =
        capacity_l6_operation(f, serde_json::json!({"kind":"discover","cursor":null})).await?;
    if discovery["ok"] != true || discovery["result"]["outcome"] != "succeeded" {
        return Err("child_real_relationship_discovery_not_confirmed");
    }
    let source = capacity_l6_child_source(f)?;
    let pool = capacity_l6_child_pool(f)?;
    timeout(Duration::from_secs(30), async {
        loop {
            let ids: Vec<String> = sqlx::query_scalar(
                "SELECT child_native_id FROM node_session_parents WHERE source_id=? AND parent_native_id=? ORDER BY child_native_id",
            )
            .bind(&source)
            .bind(root)
            .fetch_all(&pool)
            .await
            .map_err(|_| "child_received_relationship_query_failed")?;
            if ids.len() >= count {
                return Ok(ids);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "child_actual_received_persisted_relationship_missing")?
}

async fn capacity_l6_child_view(
    f: &mut R3WssCase,
    child: &str,
    reason: &str,
) -> CapacityL6Result<CapacityL6View> {
    let mut opened = capacity_l6_call(
        f,
        "view.open_source",
        serde_json::json!({"selection":f.selection_json(),"nativeSessionId":child,"reason":reason}),
    )
    .await?;
    if opened["ok"] != true {
        return Err("child_actual_view_not_available");
    }
    let mut stream = Id128::parse_base64url(
        opened["result"]["streamId"]
            .as_str()
            .ok_or("child_stream_missing")?,
    )
    .map_err(|_| "child_stream_invalid")?;
    let (mut records, rebound) = capacity_l6_drain_snapshot(&mut f.peers[0].socket, stream).await?;
    // ChildAutoDelivered has no independent replay operation to complete.
    // Only an actual stream_rebound authorizes waiting for/rebinding a new
    // generation. A root marker must never make the child history complete.
    if rebound {
        opened = capacity_l6_call_collect(
            f,
            "view.rebind",
            serde_json::json!({"streamId":stream.to_base64url()}),
            &mut records,
        )
        .await?;
        if opened["ok"] != true {
            return Err("child_observed_rebound_not_confirmed");
        }
        stream = Id128::parse_base64url(
            opened["result"]["streamId"]
                .as_str()
                .ok_or("child_rebound_stream_missing")?,
        )
        .map_err(|_| "child_rebound_stream_invalid")?;
        let (next, again) = capacity_l6_drain_snapshot(&mut f.peers[0].socket, stream).await?;
        if again {
            return Err("child_snapshot_changed_again");
        }
        records = next;
    }
    r3_send_binary(
        &mut f.peers[0].socket,
        TransferHeader::detach(stream)
            .encode(&[])
            .map_err(|_| "child_snapshot_detach_invalid")?,
    )
    .await;
    let closed = capacity_l6_call(
        f,
        "view.close",
        serde_json::json!({"streamId":stream.to_base64url()}),
    )
    .await?;
    if closed["ok"] != true {
        return Err("child_snapshot_reader_close_unconfirmed");
    }
    Ok(CapacityL6View {
        mode: opened["result"]["status"]["mode"]
            .as_str()
            .ok_or("child_actual_mode_missing")?
            .to_owned(),
        records,
        history_incomplete: opened["result"]["status"]["historyIncomplete"] == true,
        // For this child helper the complete unit is the actual delivered
        // snapshot, not a claimed native history completion.
        actual_complete: true,
    })
}

async fn capacity_l6_child_bodies(
    f: &mut R3WssCase,
    scenario: &CapacityL6Scenario,
    children: &[String],
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    let mut matched = std::collections::HashSet::new();
    for (index, child) in children.iter().enumerate() {
        let view = capacity_l6_child_view(f, child, "first_open").await?;
        if view.mode == "current_only" && !view.history_incomplete {
            return Err("child_current_only_falsely_claimed_complete_history");
        }
        let candidates = if scenario.real_child_ids.is_empty() {
            (0..scenario.expected_child_text.len()).collect::<Vec<_>>()
        } else {
            vec![index]
        };
        let matched_index = candidates
            .into_iter()
            .find(|candidate| {
                let forbidden = scenario
                    .expected_child_text
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| i != candidate)
                    .map(|(_, text)| text.clone())
                    .collect::<Vec<_>>();
                capacity_l6_exact_body(&view, &scenario.expected_child_text[*candidate], &forbidden)
                    .is_ok()
            })
            .ok_or("child_exact_distinct_content_not_observed")?;
        if !matched.insert(matched_index) {
            return Err("child_same_body_repeated_for_different_subjects");
        }
        steps.push(serde_json::json!({
            "step":"direct_real_child","native":child,"expectationIndex":matched_index,
            "exactBodyMatched":true,"mode":view.mode,
            "historyIncomplete":view.history_incomplete,"currentChildSnapshotComplete":view.actual_complete
        }));
    }
    Ok(())
}

async fn capacity_l6_child_cancel(
    f: &mut R3WssCase,
    child: &str,
    root: &CapacityL6ChildRoot,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    let before = capacity_l6_child_inventory(f).await?;
    let opened = capacity_l6_call(
        f,
        "view.open_source",
        serde_json::json!({"selection":f.selection_json(),"nativeSessionId":child,"reason":"explicit_refresh"}),
    )
    .await?;
    if opened["ok"] != true {
        return Err("child_cancellation_requires_actual_open_reader");
    }
    let stream = Id128::parse_base64url(
        opened["result"]["streamId"]
            .as_str()
            .ok_or("child_cancel_stream_missing")?,
    )
    .map_err(|_| "child_cancel_stream_invalid")?;
    // The detach and close concern this actual authenticated reader. No
    // artificial native cancellation/Close or task completion is installed.
    r3_send_binary(
        &mut f.peers[0].socket,
        TransferHeader::detach(stream)
            .encode(&[])
            .map_err(|_| "child_cancel_detach_encoding_failed")?,
    )
    .await;
    let closed = capacity_l6_call(
        f,
        "view.close",
        serde_json::json!({"streamId":stream.to_base64url()}),
    )
    .await?;
    if closed["ok"] != true
        || capacity_l6_child_state(f, root).await? != (0, 0)
        || capacity_l6_child_inventory(f).await? != before
    {
        return Err("child_cancel_refunded_or_changed_original_owner");
    }
    steps.push(serde_json::json!({
        "step":"actual_child_reader_detach_close_retains_root",
        "native":child,"root":root.native,"reservation":root.reservation,
        "streamId":stream.to_base64url(),"nativeCloseRequested":false,
        "boundary":"cancel actual authenticated consumer after real tree resource acquisition",
        "nativeReadPendingClaim":false
    }));
    Ok(())
}

async fn capacity_l6_child_failure(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    root_native: &str,
    child: &str,
    others: &[CapacityL6ChildRoot],
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<CapacityL6ChildRoot> {
    use crate::application::native_dispatch::capacity_close_write_fault_probe;
    if scenario.child_failure_steps.is_empty() || scenario.child_recovery_steps.is_empty() {
        return Err("child_real_failure_and_recovery_scenario_required");
    }
    if capacity_l6_inspect(f).await?["occupied"] != 49 {
        return Err("child_failure_requires_released_root_at_forty_nine");
    }
    let anchor = others
        .first()
        .ok_or("child_failure_shared_tree_anchor_missing")?;
    let runtime = f
        .node
        .as_ref()
        .and_then(|node| node.session_services.as_ref())
        .ok_or("child_failure_runtime_missing")?
        .native
        .clone();
    let source = f
        .selection
        .as_ref()
        .ok_or("child_failure_source_missing")?
        .source;
    let pool = capacity_l6_child_pool(f)?;
    let connection_str: String = sqlx::query_scalar(
        "SELECT connection_epoch FROM node_native_source_instances WHERE instance_id=? AND source_id=?",
    )
    .bind(&anchor.instance)
    .bind(source.encode())
    .fetch_one(&pool)
    .await
    .map_err(|_| "child_failure_connection_query_failed")?;
    let instance = crate::domain::EntityId::decode(&anchor.instance)
        .map_err(|_| "child_failure_instance_invalid")?;
    let connection = crate::domain::EntityId::decode(&connection_str)
        .map_err(|_| "child_failure_connection_invalid")?;

    capacity_l6_apply_commands(f, c, root_native, &scenario.child_failure_steps).await?;
    let guard = capacity_close_write_fault_probe::install_resume_enqueue_fault(
        &runtime, source, instance, connection, root_native,
    )
    .map_err(|_| "child_resume_fault_install_failed")?;
    let failed = capacity_l6_call(
        f,
        "view.open_source",
        serde_json::json!({"selection":f.selection_json(),"nativeSessionId":child,"reason":"reacquire"}),
    )
    .await?;
    let observed = guard.observation();
    drop(guard);
    if !observed.claimed
        || !observed.fired
        || failed["ok"] != false
        || failed["error"]["code"] != "read_failed"
    {
        return Err("child_actual_native_read_failure_not_observed");
    }
    let failed_owner = capacity_l6_child_root(f, root_native).await?;
    if capacity_l6_child_state(f, &failed_owner).await? != (0, 0)
        || capacity_l6_inspect(f).await?["occupied"] != 50
    {
        return Err("child_failed_read_lost_original_tree_responsibility");
    }
    capacity_l6_child_live_unchanged(f, others).await?;

    capacity_l6_apply_commands(f, c, root_native, &scenario.child_recovery_steps).await?;
    let recovered = capacity_l6_child_view(f, child, "explicit_refresh").await?;
    let recovered_owner = capacity_l6_child_root(f, root_native).await?;
    if !recovered.actual_complete
        || recovered_owner.reservation != failed_owner.reservation
        || capacity_l6_child_state(f, &recovered_owner).await? != (0, 0)
        || capacity_l6_inspect(f).await?["occupied"] != 50
    {
        return Err("child_original_tree_failure_recovery_not_confirmed");
    }
    capacity_l6_child_live_unchanged(f, others).await?;
    steps.push(serde_json::json!({
        "step":"real_child_read_failed_then_original_owner_recovered",
        "root":recovered_owner.native,"child":child,"reservation":recovered_owner.reservation,
        "nodeFaultInjected":true,"faultScope":"owned_single_root_resume_enqueue_reject"
    }));
    Ok(recovered_owner)
}

async fn capacity_l6_child_web(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    phase: &str,
    natives: &[String],
    operation_ids: &[String],
    occupied: u32,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    // Shared format-2 collector persists only a pending review reference.
    // It cannot mark the independent browser branch as passed.
    capacity_l6_web_collect_phase(
        f,
        c,
        phase,
        serde_json::json!({
            "nativeIds":natives,"operationIds":operation_ids,"occupied":occupied
        }),
        steps,
    )
    .await
}

async fn capacity_l6_child_fifty(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    root: &CapacityL6ChildRoot,
    children: &[String],
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<(CapacityL6ChildRoot, Vec<CapacityL6ChildRoot>)> {
    if scenario.protected_root_steps.is_empty() || scenario.candidate_release_steps.is_empty() {
        return Err("child_fifty_real_protection_and_release_commands_required");
    }
    capacity_l6_apply_commands(f, c, &root.native, &scenario.protected_root_steps).await?;
    let mut protected = Vec::new();
    for _ in 0..49 {
        let native = capacity_l6_new(f).await?;
        let other = capacity_l6_child_root(f, &native).await?;
        capacity_l6_apply_commands(f, c, &native, &scenario.protected_root_steps).await?;
        protected.push(other);
    }
    if capacity_l6_inspect(f).await?["occupied"] != 50 {
        return Err("child_real_fifty_unique_roots_not_occupied");
    }
    let before_reuse = capacity_l6_child_inventory(f).await?;
    let reused = capacity_l6_child_view(f, &children[0], "reopen").await?;
    if !reused.actual_complete || capacity_l6_child_inventory(f).await? != before_reuse {
        return Err("child_same_tree_at_fifty_minted_new_owner_or_slot");
    }
    // Retire only this test-created root through its real original Close. Its
    // received child identity remains known, but its next read needs a slot.
    let retired = capacity_l6_child_release(f, root).await?;
    let replacement_native = capacity_l6_new(f).await?;
    let replacement = capacity_l6_child_root(f, &replacement_native).await?;
    capacity_l6_apply_commands(f, c, &replacement.native, &scenario.protected_root_steps).await?;
    protected.push(replacement);
    // One real normal scan observes all current protections. Never alter the
    // policy clock or assert that a submitted prompt by itself is protection.
    tokio::time::sleep(Duration::from_secs(31)).await;
    capacity_l6_child_live_unchanged(f, &protected).await?;
    for other in &protected {
        if capacity_l6_child_candidate(f, &other.native)? {
            return Err("child_fifty_has_candidate_in_no_candidate_phase");
        }
    }
    let before_denied = capacity_l6_child_inventory(f).await?;
    let pool = capacity_l6_child_pool(f)?;
    let before_operations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM node_session_automatic_releases")
            .fetch_one(&pool)
            .await
            .map_err(|_| "child_auto_operation_count_unavailable")?;
    let denied = capacity_l6_call(
        f,
        "view.open_source",
        serde_json::json!({"selection":f.selection_json(),"nativeSessionId":children[0],"reason":"reacquire"}),
    )
    .await?;
    if denied["ok"] != false || denied["result"].is_object() {
        return Err("child_new_tree_at_fifty_not_explicitly_refused");
    }
    tokio::time::sleep(Duration::from_secs(31)).await;
    let after_operations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM node_session_automatic_releases")
            .fetch_one(&pool)
            .await
            .map_err(|_| "child_auto_operation_recheck_unavailable")?;
    if capacity_l6_inspect(f).await?["occupied"] != 50
        || capacity_l6_child_inventory(f).await? != before_denied
        || before_operations != after_operations
    {
        return Err("child_no_candidate_refusal_queued_or_changed_owned_resources");
    }
    let candidate = protected.remove(0);
    capacity_l6_apply_commands(f, c, &candidate.native, &scenario.candidate_release_steps).await?;
    timeout(Duration::from_secs(60), async {
        loop {
            if capacity_l6_child_candidate(f, &candidate.native)? {
                return Ok::<(), &'static str>(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| "child_real_candidate_release_not_observed")??;
    for other in &protected {
        if capacity_l6_child_candidate(f, &other.native)? {
            return Err("child_close_selection_not_sole_confirmed_candidate");
        }
    }
    capacity_l6_child_web(
        f,
        c,
        "child-request-pressure",
        children,
        std::slice::from_ref(&retired),
        50,
        steps,
    )
    .await?;
    let loaded = capacity_l6_child_view(f, &children[0], "reacquire").await?;
    let restored = capacity_l6_child_root(f, &root.native).await?;
    let operations: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT operation_id,phase,outcome FROM node_session_automatic_releases WHERE anchor_reservation=? AND reason='capacity_pressure'",
    )
    .bind(&candidate.reservation)
    .fetch_all(&pool)
    .await
    .map_err(|_| "child_pressure_original_operation_unavailable")?;
    if !loaded.actual_complete
        || operations.len() != 1
        || operations[0].1 != "finished"
        || operations[0].2.as_deref() != Some("requested")
        || capacity_l6_child_state(f, &candidate).await? != (1, 1)
        || restored.reservation == root.reservation
        || capacity_l6_inspect(f).await?["occupied"] != 50
    {
        return Err("child_admission_without_original_close_join_finish");
    }
    capacity_l6_child_live_unchanged(f, &protected).await?;
    steps.push(serde_json::json!({
        "step":"real_fifty_child_refusal_then_original_close_and_admission",
        "root":root.native,"retiredReleaseOperation":retired,
        "originalCandidate":candidate.native,"systemOperation":operations[0].0,
        "originalReservation":candidate.reservation,"restoredReservation":restored.reservation,
        "noCandidateObservedForSeconds":31,"occupied":50
    }));
    capacity_l6_mark(
        steps,
        "cp023",
        "real_fifty_no_candidate_then_confirmed_close",
    )?;
    capacity_l6_child_web(
        f,
        c,
        "child-after-pressure",
        children,
        &[operations[0].0.clone()],
        50,
        steps,
    )
    .await?;
    Ok((restored, protected))
}

async fn capacity_l6_child_invalidated(
    f: &mut R3WssCase,
    root: &CapacityL6ChildRoot,
    child: &str,
    others: &[CapacityL6ChildRoot],
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    if capacity_l6_child_state(f, root).await? != (1, 1) {
        return Err("child_relationship_fault_requires_closed_joined_original_root");
    }
    let capacity = capacity_l6_child_inventory(f).await?;
    let relations = capacity_l6_child_relation_rows(f).await?;
    if relations.is_empty() {
        return Err("child_relationship_fault_requires_actual_received_proof");
    }
    let mut fault = CapacityL6RelationFault::install(f).await?;
    // The old reader/root has been closed, so a cached live owner cannot
    // turn this into a no-load query. This is not a closed Node/database.
    let pool = capacity_l6_child_pool(f)?;
    let healthy: i64 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&pool)
        .await
        .map_err(|_| "child_relationship_fault_stopped_database")?;
    let unavailable = sqlx::query_scalar::<_, String>(
        "SELECT parent_native_id FROM main.node_session_parents WHERE source_id=? AND child_native_id=?",
    )
    .bind(capacity_l6_child_source(f)?)
    .bind(child)
    .fetch_optional(&pool)
    .await;
    let refused = capacity_l6_call(
        f,
        "view.open_source",
        serde_json::json!({"selection":f.selection_json(),"nativeSessionId":child,"reason":"reacquire"}),
    )
    .await;
    fault.restore()?;
    if capacity_l6_child_relation_rows(f).await? != relations
        || capacity_l6_child_inventory(f).await? != capacity
    {
        return Err("child_relationship_fault_changed_original_proof_or_owner");
    }
    capacity_l6_child_live_unchanged(f, others).await?;
    let refused = refused?;
    if healthy != 1
        || unavailable.is_ok()
        || refused["ok"] != false
        || refused["result"].is_object()
        || ![
            "read_failed",
            "necessary_storage_unavailable",
            "invalid_target",
        ]
        .contains(&refused["error"]["code"].as_str().unwrap_or(""))
    {
        return Err("child_unreadable_relationship_not_explicitly_refused");
    }
    steps.push(serde_json::json!({
        "step":"owned_node_relationship_lookup_fault_then_child_refused",
        "root":root.native,"child":child,"source":capacity_l6_child_source(f)?,
        "faultScope":"Node SQL table unavailable; not vendor root deletion",
        "schemaAndRelationshipProofRestoredExactly":true,"newReservations":0,
        "otherOriginalTreesUnchanged":true,
        "nativeIoEvidence":"resolve_session_tree fails before admit/write by source path; no measured native IO counter"
    }));
    capacity_l6_mark(
        steps,
        "cp023",
        "deleted_root_or_invalid_relationship_refusal",
    )
}

async fn capacity_l6_cp023_body(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    if scenario.expected_child_text.len() < 2
        || scenario.expected_child_text.iter().any(String::is_empty)
        || scenario
            .expected_child_text
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != scenario.expected_child_text.len()
    {
        return Err("child_two_distinct_actual_body_expectations_required");
    }
    let native = capacity_l6_new(f).await?;
    let root = capacity_l6_child_root(f, &native).await?;
    capacity_l6_apply_commands(f, c, &native, &scenario.management_steps).await?;
    let discovered =
        capacity_l6_child_relations(f, &native, scenario.expected_child_text.len()).await?;
    let children = if scenario.real_child_ids.is_empty() {
        discovered
    } else if scenario
        .real_child_ids
        .iter()
        .all(|child| discovered.contains(child))
    {
        scenario.real_child_ids.clone()
    } else {
        return Err("child_configured_identity_not_in_actual_received_tree");
    };
    if children.len() != scenario.expected_child_text.len() {
        return Err("child_exact_identity_body_mapping_required");
    }
    capacity_l6_mark(
        steps,
        "cp023",
        "received_trusted_real_children_unopened_root",
    )?;
    let owner = capacity_l6_child_inventory(f).await?;
    // The independent browser gets the first opportunity to load real child
    // views. No root or child body was opened by this entry before this phase.
    capacity_l6_child_web(f, c, "children-unopened-root", &children, &[], 1, steps).await?;
    capacity_l6_child_bodies(f, scenario, &children, steps).await?;
    capacity_l6_mark(steps, "cp023", "direct_children_exact_distinct_bodies")?;
    let parent = capacity_l6_view_records(f, &native, "first_open").await?;
    if !parent.actual_complete
        || capacity_l6_child_inventory(f).await? != owner
        || capacity_l6_inspect(f).await?["occupied"] != 1
    {
        return Err("child_parent_open_changed_original_tree_owner");
    }
    capacity_l6_mark(steps, "cp023", "one_original_tree_owner_and_slot")?;
    let (restored, others) =
        capacity_l6_child_fifty(f, c, scenario, &root, &children, steps).await?;
    // 先在 50/50 状态下验证活跃子流取消不退费，再显式释放整树至 49/50 以检验重开期间的受控 Resume 入队失败与同预留恢复。
    let cancel = capacity_l6_child_cancel(f, &children[1], &restored, steps).await;
    let _first_release = capacity_l6_child_release(f, &restored).await?;
    let failure = capacity_l6_child_failure(
        f,
        c,
        scenario,
        &restored.native,
        &children[0],
        &others,
        steps,
    )
    .await;
    if failure.is_ok() && cancel.is_ok() {
        capacity_l6_mark(
            steps,
            "cp023",
            "child_failure_cancel_preserves_original_owner",
        )?;
    }
    let recovered_owner = match failure {
        Ok(owner) => owner,
        Err(err) => {
            cancel?;
            return Err(err);
        }
    };
    let release = capacity_l6_child_release(f, &recovered_owner).await?;
    capacity_l6_child_live_unchanged(f, &others).await?;
    if capacity_l6_inspect(f).await?["occupied"] != 49 {
        return Err("child_final_join_did_not_release_exact_original_tree");
    }
    steps.push(serde_json::json!({
        "step":"tree_original_close_reads_join_and_necessary_finish",
        "root":recovered_owner.native,"reservation":recovered_owner.reservation,
        "releaseOperationId":release,"otherOriginalTrees":others.len(),"occupied":49
    }));
    capacity_l6_mark(
        steps,
        "cp023",
        "tree_final_resource_join_and_shared_tree_isolation",
    )?;
    let invalidated =
        capacity_l6_child_invalidated(f, &recovered_owner, &children[0], &others, steps).await;
    cancel?;
    invalidated
}

// Controlled-peer checks validate only this entry's observer/owner bookkeeping.
// They do not execute, claim or replace a real-Harness CP023 run.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_children_observer_preserves_distinct_bodies_and_original_slot() {
    r3_wss_case("source-create-replay-state-verified", false, |f| {
        Box::pin(async move {
            let native = rt_direct_tree(f).await;
            let root = capacity_l6_child_root(f, &native).await.unwrap();
            let scenario = CapacityL6Scenario {
                expected_child_text: vec![
                    "RT actual alpha child A".into(),
                    "RT actual beta child B".into(),
                ],
                ..CapacityL6Scenario::default()
            };
            let children = capacity_l6_child_relations(f, &native, 2).await.unwrap();
            let before = capacity_l6_child_inventory(f).await.unwrap();
            let mut steps = Vec::new();
            capacity_l6_child_bodies(f, &scenario, &children, &mut steps)
                .await
                .unwrap();
            assert_eq!(steps.len(), 2);
            assert!(steps.iter().all(|step| {
                step["mode"] == "current_only"
                    && step["historyIncomplete"] == true
                    && step["exactBodyMatched"] == true
            }));
            assert_eq!(capacity_l6_child_inventory(f).await.unwrap(), before);
            assert_eq!(capacity_l6_child_state(f, &root).await.unwrap(), (0, 0));
            let operation = capacity_l6_child_release(f, &root).await.unwrap();
            assert!(!operation.is_empty());
            assert_eq!(capacity_l6_child_state(f, &root).await.unwrap(), (1, 1));
            assert_eq!(capacity_l6_inspect(f).await.unwrap()["occupied"], 0);
            capacity_l6_child_invalidated(f, &root, &children[0], &[], &mut steps)
                .await
                .unwrap();
            assert!(steps.iter().any(|step| {
                step["branch"] == "deleted_root_or_invalid_relationship_refusal"
                    && step["status"] == "passed"
            }));
            assert_eq!(capacity_method_count(f, "session/resume"), 1);
            assert_eq!(capacity_method_count(f, "session/load"), 0);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_children_wrong_body_mapping_cannot_mark_a_branch() {
    r3_wss_case("source-create-replay-state-verified", false, |f| {
        Box::pin(async move {
            let native = rt_direct_tree(f).await;
            let children = capacity_l6_child_relations(f, &native, 2).await.unwrap();
            let scenario = CapacityL6Scenario {
                real_child_ids: children.clone(),
                expected_child_text: vec![
                    "RT actual beta child B".into(),
                    "RT actual alpha child A".into(),
                ],
                ..CapacityL6Scenario::default()
            };
            let mut steps = Vec::new();
            assert_eq!(
                capacity_l6_child_bodies(f, &scenario, &children, &mut steps).await,
                Err("child_exact_distinct_content_not_observed")
            );
            assert!(steps.is_empty());
            assert_eq!(capacity_l6_inspect(f).await.unwrap()["occupied"], 1);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_children_relation_fault_restores_after_actual_future_cancel_and_panic() {
    r3_wss_case("source-create-replay-state-verified", false, |f| {
        Box::pin(async move {
            let _native = rt_direct_tree(f).await;
            let proof = capacity_l6_child_relation_rows(f).await.unwrap();
            let owners = capacity_l6_child_inventory(f).await.unwrap();
            let (entered, mut observed) = tokio::sync::oneshot::channel();
            let mut work = Box::pin(async {
                let _guard = CapacityL6RelationFault::install(f).await.unwrap();
                let _ = entered.send(());
                std::future::pending::<()>().await;
            });
            tokio::select! {
                () = &mut work => panic!("controlled SQL fault must await cancellation"),
                result = &mut observed => result.unwrap(),
            }
            drop(work);
            assert_eq!(capacity_l6_child_relation_rows(f).await.unwrap(), proof);
            assert_eq!(capacity_l6_child_inventory(f).await.unwrap(), owners);
            let guard = CapacityL6RelationFault::install(f).await.unwrap();
            let panicked = std::panic::catch_unwind(AssertUnwindSafe(move || {
                let _guard = guard;
                panic!("controlled owned SQL fault unwind");
            }));
            assert!(panicked.is_err());
            assert_eq!(capacity_l6_child_relation_rows(f).await.unwrap(), proof);
            assert_eq!(capacity_l6_child_inventory(f).await.unwrap(), owners);
            assert!(
                !f.area
                    .directory
                    .join("l6-relation-restore-failed.json")
                    .exists()
            );
        })
    })
    .await;
}
