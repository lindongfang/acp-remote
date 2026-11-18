// This fault changes only the owned Node necessary-finish transaction.
// Native Written and Closed must still come from the original real Harness.
async fn capacity_l6_cp032_finish(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<String> {
    let roots: std::collections::HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT native_id FROM node_session_capacity WHERE released=0 AND native_id IS NOT NULL",
    )
    .fetch_all(&capacity_l6_clock_pool(f)?)
    .await
    .map_err(|_| "real_finish_original_roots_query_failed")?
    .into_iter()
    .collect();
    if roots.len() != 50 || capacity_l6_inspect(f).await?["occupied"] != 50 {
        return Err("real_finish_requires_original_fifty_trees");
    }

    let pool = f
        .node
        .as_ref()
        .and_then(|n| n.storage.as_ref())
        .ok_or("storage_missing")?
        .pool_for_test()
        .clone();
    sqlx::query("CREATE TRIGGER l6_owned_finish_fault BEFORE UPDATE OF phase ON node_session_automatic_releases WHEN NEW.phase='finished' BEGIN SELECT RAISE(ABORT,'owned L6 necessary finish fault'); END")
            .execute(&pool).await.map_err(|_|"owned_finish_fault_install_failed")?;
    let pressured = capacity_l6_operation(f, serde_json::json!({"kind":"create"})).await?;
    let operations:Vec<(String,String,String)>=sqlx::query_as(
            "SELECT o.operation_id,o.anchor_reservation,c.native_id FROM node_session_automatic_releases o JOIN node_session_capacity c ON c.reservation_id=o.anchor_reservation WHERE o.phase='pending' AND o.outcome='requested' AND c.physically_closed=1 AND c.released=0",
        ).fetch_all(&pool).await.map_err(|_|"owned_pending_query_failed")?;
    let occupied = capacity_l6_inspect(f).await?["occupied"].clone();
    if operations.len() != 1 {
        return Err("one_real_closed_original_operation_required");
    }
    let (operation, anchor, closed_native) = &operations[0];
    let fixed_members:Vec<String>=sqlx::query_scalar("SELECT reservation_id FROM node_session_automatic_release_members WHERE operation_id=? ORDER BY reservation_id").bind(operation).fetch_all(&pool).await.map_err(|_|"original_member_set_query_failed")?;
    if fixed_members.is_empty() || !fixed_members.contains(anchor) {
        return Err("system_original_nonempty_member_set_missing");
    }
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('node_session_automatic_releases')")
            .fetch_all(&pool)
            .await
            .map_err(|_| "system_identity_schema_query_failed")?;
    if columns
        .iter()
        .any(|name| name == "actor_id" || name == "target_id")
    {
        return Err("automatic_operation_depends_on_user_actor_or_target");
    }
    if occupied != 50 || pressured["result"]["outcome"] == "succeeded" {
        return Err("necessary_finish_failure_refunded_or_admitted");
    }
    let unchanged:Vec<String>=sqlx::query_scalar("SELECT json_array(reservation_id,native_id,instance_id,runtime_epoch,physically_closed,released) FROM node_session_capacity WHERE reservation_id!=? ORDER BY reservation_id").bind(anchor).fetch_all(&pool).await.map_err(|_|"shared_other_tree_snapshot_failed")?;
    // All other trees are still original occupied owners. An owned SQLite
    // failure is not represented as an upstream natural failure.
    let others:i64=sqlx::query_scalar("SELECT count(*) FROM node_session_capacity WHERE reservation_id!=? AND physically_closed=0 AND released=0").bind(anchor).fetch_one(&pool).await.map_err(|_|"shared_other_tree_count_failed")?;
    if others != 49 {
        return Err("necessary_finish_fault_changed_other_tree");
    }
    capacity_l6_mark(
        steps,
        "cp032",
        "real_written_closed_and_system_original_nonempty_set",
    )?;
    capacity_l6_mark(steps, "cp032", "closed_necessary_finish_fault_keeps_fifty")?;
    steps.push(serde_json::json!({"step":"real_close_node_finish_fault","scope":"Node SQLite injection, real Harness Written/Closed","operationId":operation,"anchorReservation":anchor,"native":closed_native,"originalMemberCount":fixed_members.len(),"occupied":occupied}));
    sqlx::query("DROP TRIGGER l6_owned_finish_fault")
        .execute(&pool)
        .await
        .map_err(|_| "owned_finish_fault_remove_failed")?;
    timeout(Duration::from_secs(90), async {
        loop {
            let phase: Option<String> = sqlx::query_scalar(
                "SELECT phase FROM node_session_automatic_releases WHERE operation_id=?",
            )
            .bind(operation)
            .fetch_optional(&pool)
            .await
            .map_err(|_| "original_operation_repair_query_failed")?;
            if phase.as_deref() == Some("finished") {
                return Ok::<(), &'static str>(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "original_operation_necessary_repair_timeout")??;
    let repaired: (i64, i64) = sqlx::query_as(
        "SELECT physically_closed,released FROM node_session_capacity WHERE reservation_id=?",
    )
    .bind(anchor)
    .fetch_one(&pool)
    .await
    .map_err(|_| "original_reservation_repair_query_failed")?;
    if repaired != (1, 1) || capacity_l6_inspect(f).await?["occupied"] != 49 {
        return Err("original_necessary_repair_did_not_refund_exactly_once");
    }
    let after_members:Vec<String>=sqlx::query_scalar("SELECT reservation_id FROM node_session_automatic_release_members WHERE operation_id=? ORDER BY reservation_id").bind(operation).fetch_all(&pool).await.map_err(|_|"original_member_set_recheck_failed")?;
    let after_other:Vec<String>=sqlx::query_scalar("SELECT json_array(reservation_id,native_id,instance_id,runtime_epoch,physically_closed,released) FROM node_session_capacity WHERE reservation_id!=? ORDER BY reservation_id").bind(anchor).fetch_all(&pool).await.map_err(|_|"shared_other_tree_recheck_failed")?;
    if after_members != fixed_members || after_other != unchanged {
        return Err("original_repair_changed_set_or_shared_tree_owner");
    }
    let next = capacity_l6_new(f).await?;
    if roots.contains(&next) || capacity_l6_inspect(f).await?["occupied"] != 50 {
        return Err("confirmed_original_repair_did_not_allow_exactly_one_new_tree");
    }
    steps.push(serde_json::json!({"step":"same_original_repair","operationId":operation,"phase":"finished","releasedExactlyOnce":true,"sharedOtherTreesUnchanged":true,"newNative":next}));
    capacity_l6_web_collect_phase(f,c,"necessary-finish-repaired",serde_json::json!({"nativeIds":[closed_native,next],"operationIds":[operation],"occupied":50}),steps).await?;
    Ok(operation.clone())
}
