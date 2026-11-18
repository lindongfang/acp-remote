// CP026 uses real monotonic receipt/candidate clocks, original reservations and
// real management reads. No clock, activity, relationship or ACP response is seeded.
#[derive(Clone)]
struct CapacityL6ClockRoot {
    native: String,
    reservation: String,
    instance: String,
    epoch: String,
}

fn capacity_l6_clock_pool(f: &R3WssCase) -> CapacityL6Result<sqlx::SqlitePool> {
    f.node
        .as_ref()
        .and_then(|n| n.storage.as_ref())
        .map(|s| s.pool_for_test().clone())
        .ok_or("clock_storage_missing")
}

async fn capacity_l6_clock_root(
    f: &R3WssCase,
    native: String,
) -> CapacityL6Result<CapacityL6ClockRoot> {
    let source = f
        .selection
        .as_ref()
        .ok_or("clock_source_missing")?
        .source
        .encode();
    let rows: Vec<(String,String,String)> = sqlx::query_as(
        "SELECT reservation_id,instance_id,runtime_epoch FROM node_session_capacity WHERE source_id=? AND native_id=? AND released=0",
    ).bind(source).bind(&native).fetch_all(&capacity_l6_clock_pool(f)?).await
        .map_err(|_| "clock_original_reservation_query_failed")?;
    if rows.len() != 1 {
        return Err("clock_requires_one_original_live_reservation");
    }
    let (reservation, instance, epoch) = rows.into_iter().next().ok_or("clock_original_missing")?;
    Ok(CapacityL6ClockRoot {
        native,
        reservation,
        instance,
        epoch,
    })
}

fn capacity_l6_clock_sample(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
) -> CapacityL6Result<(std::time::Instant, Option<std::time::Instant>)> {
    use crate::application::native_dispatch::runtime_child_source_clock_probe::{
        rt_source_activity_clocks, rt_source_candidate_clocks,
    };
    let native = &f
        .node
        .as_ref()
        .and_then(|n| n.session_services.as_ref())
        .ok_or("clock_runtime_missing")?
        .native;
    let source = f.selection.as_ref().ok_or("clock_source_missing")?.source;
    let received: Vec<_> = rt_source_activity_clocks(native)
        .map_err(|_| "clock_receipt_unavailable")?
        .into_iter()
        .filter(|(s, n, _)| *s == source && n.as_ref() == root.native)
        .collect();
    let eligible: Vec<_> = rt_source_candidate_clocks(native)
        .map_err(|_| "clock_candidate_unavailable")?
        .into_iter()
        .filter(|(s, n, _)| *s == source && n.as_ref() == root.native)
        .collect();
    if received.len() != 1 || eligible.len() != 1 {
        return Err("clock_original_runtime_ambiguous");
    }
    Ok((received[0].2, eligible[0].2))
}

async fn capacity_l6_clock_state(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
) -> CapacityL6Result<(i64, i64, i64)> {
    let row:Option<(i64,i64,i64)>=sqlx::query_as(
        "SELECT physically_closed,released,(SELECT count(*) FROM node_session_automatic_releases WHERE anchor_reservation=c.reservation_id) FROM node_session_capacity c WHERE reservation_id=? AND native_id=? AND instance_id=? AND runtime_epoch=?",
    ).bind(&root.reservation).bind(&root.native).bind(&root.instance).bind(&root.epoch)
        .fetch_optional(&capacity_l6_clock_pool(f)?).await
        .map_err(|_|"clock_original_state_query_failed")?;
    row.ok_or("clock_original_identity_changed")
}

async fn capacity_l6_clock_eligible(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
) -> CapacityL6Result<(std::time::Instant, std::time::Instant)> {
    let until = tokio::time::Instant::now() + Duration::from_secs(45);
    loop {
        if capacity_l6_clock_state(f, root).await? != (0, 0, 0) {
            return Err("clock_original_was_already_closing");
        }
        let (received, candidate) = capacity_l6_clock_sample(f, root)?;
        if let Some(candidate) = candidate {
            return Ok((received, candidate));
        }
        if tokio::time::Instant::now() >= until {
            return Err("clock_current_idle_eligibility_not_observed");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn capacity_l6_clock_text(value: &serde_json::Value, expected: &str) -> bool {
    match value {
        serde_json::Value::Object(fields) => fields.iter().any(|(key, value)| {
            (key == "text" && value.as_str().is_some_and(|text| text.contains(expected)))
                || capacity_l6_clock_text(value, expected)
        }),
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| capacity_l6_clock_text(value, expected)),
        _ => false,
    }
}

fn capacity_l6_clock_has_body(records: &[(RecordKind, Vec<u8>)], expected: &str) -> bool {
    !expected.is_empty()
        && records.iter().any(|(kind, bytes)| {
            *kind == RecordKind::OriginalRaw
                && serde_json::from_slice::<serde_json::Value>(bytes)
                    .is_ok_and(|value| capacity_l6_clock_text(&value, expected))
        })
}

async fn capacity_l6_clock_commands(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    native: &str,
    commands: &[CapacityL6Command],
) -> CapacityL6Result<()> {
    if commands.is_empty() {
        return Err("clock_real_history_or_protection_scenario_required");
    }
    if !c.allow_model_requests {
        return Err("clock_explicit_model_permission_required");
    }
    capacity_l6_apply_commands(f, c, native, commands).await
}

// Complete the original read and consume its replacement once. The returned
// current reader has no original intent and must never be rebound again.
async fn capacity_l6_clock_rebind(
    f: &mut R3WssCase,
    stream: Id128,
    expected: &str,
) -> CapacityL6Result<(Id128, Vec<(RecordKind, Vec<u8>)>)> {
    let mut prior = Vec::new();
    let rebound = capacity_l6_call_collect(
        f,
        "view.rebind",
        serde_json::json!({
            "streamId":stream.to_base64url()
        }),
        &mut prior,
    )
    .await?;
    if rebound["ok"] != true {
        return Err(match rebound["error"]["code"].as_str() {
            Some("invalid_stage") => "clock_original_rebind_invalid_stage",
            Some("stale_cursor") => "clock_original_rebind_stale_cursor",
            Some("stream_sealed") => "clock_original_rebind_stream_sealed",
            Some("read_failed") => "clock_original_rebind_read_failed",
            Some("unsupported_capability") => "clock_original_rebind_unsupported",
            Some("source_released") => "clock_original_rebind_source_released",
            _ => "clock_original_read_rebind_rejected",
        });
    }
    if rebound["result"]["status"]["historyIncomplete"] == true {
        return Err("clock_completed_cache_history_still_incomplete");
    }
    let next = Id128::parse_base64url(
        rebound["result"]["streamId"]
            .as_str()
            .ok_or("clock_cache_stream_missing")?,
    )
    .map_err(|_| "clock_cache_stream_invalid")?;
    let (records, again) = capacity_l6_drain_snapshot(&mut f.peers[0].socket, next).await.map_err(|e| match e { "view_snapshot_deadline_elapsed_zero_packets" => "clock_cache_rebind_zero_packets", "view_snapshot_deadline_elapsed_partial_packets" => "clock_cache_rebind_partial_packets", _ => e })?;
    if again || !capacity_l6_clock_has_body(&records, expected) {
        return Err("clock_cached_body_or_stable_generation_missing");
    }
    Ok((next, records))
}

async fn capacity_l6_clock_cache_open(
    f: &mut R3WssCase,
    root: &CapacityL6ClockRoot,
    expected: &str,
) -> CapacityL6Result<(Id128, Vec<(RecordKind, Vec<u8>)>)> {
    let opened = capacity_l6_call(
        f,
        "view.open_source",
        serde_json::json!({
            "selection":f.selection_json(),"nativeSessionId":root.native,"reason":"explicit_refresh"
        }),
    )
    .await?;
    if opened["ok"] != true {
        return Err("clock_cache_initial_read_unavailable");
    }
    let stream = Id128::parse_base64url(
        opened["result"]["streamId"]
            .as_str()
            .ok_or("clock_cache_initial_stream_missing")?,
    )
    .map_err(|_| "clock_cache_initial_stream_invalid")?;
    // Drain with continuous credit until the original read actually completes;
    // a snapshot boundary alone does not permit replacing the active pump.
    let _original = capacity_l6_drain_original_complete(&mut f.peers[0].socket, stream).await.map_err(|e| match e { "view_snapshot_deadline_elapsed_zero_packets" => "clock_cache_original_zero_packets", "view_snapshot_deadline_elapsed_partial_packets" => "clock_cache_original_partial_packets", _ => e })?;
    capacity_l6_clock_rebind(f, stream, expected).await
}

async fn capacity_l6_clock_cache_visit(
    f: &mut R3WssCase,
    records: &[(RecordKind, Vec<u8>)],
    expected: &str,
) -> CapacityL6Result<()> {
    // Re-read only the client's already received cache. The accompanying
    // heartbeat and management roundtrip issue no native replay request.
    if !capacity_l6_clock_has_body(records, expected) {
        return Err("clock_client_cached_body_missing");
    }
    f.peers[0]
        .socket
        .send(Message::Ping(b"capacity-cache".to_vec().into()))
        .await
        .map_err(|_| "clock_cache_heartbeat_failed")?;
    let _ = capacity_l6_inspect(f).await?;
    Ok(())
}

async fn capacity_l6_clock_cache_close(f: &mut R3WssCase, stream: Id128) -> CapacityL6Result<()> {
    r3_send_binary(
        &mut f.peers[0].socket,
        TransferHeader::detach(stream)
            .encode(&[])
            .map_err(|_| "clock_detach_encoding_failed")?,
    )
    .await;
    let mut prior = Vec::new();
    let closed = capacity_l6_call_collect(
        f,
        "view.close",
        serde_json::json!({
            "streamId":stream.to_base64url()
        }),
        &mut prior,
    )
    .await?;
    if closed["ok"] != true {
        return Err("clock_cache_reader_close_unconfirmed");
    }
    Ok(())
}

async fn capacity_l6_clock_wait_before(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
    received: std::time::Instant,
    since: std::time::Instant,
    seconds: u64,
) -> CapacityL6Result<()> {
    loop {
        let state = capacity_l6_clock_state(f, root).await?;
        let sample = capacity_l6_clock_sample(f, root)?;
        if state != (0, 0, 0) {
            return Err("clock_original_closed_before_required_boundary");
        }
        if sample != (received, Some(since)) {
            return Err("clock_unplanned_activity_or_eligibility_change");
        }
        let elapsed = since.elapsed();
        if elapsed >= Duration::from_secs(seconds) {
            if seconds == 1799 && elapsed >= Duration::from_mins(30) {
                return Err("clock_actual_1799_observation_window_missed");
            }
            return Ok(());
        }
        tokio::time::sleep(
            Duration::from_secs(seconds)
                .saturating_sub(elapsed)
                .min(Duration::from_secs(1)),
        )
        .await;
    }
}

async fn capacity_l6_clock_wait_finished(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
    since: std::time::Instant,
    reason: &str,
) -> CapacityL6Result<String> {
    let until = tokio::time::Instant::now() + Duration::from_secs(45);
    loop {
        let rows:Vec<(String,String,Option<String>,i64,i64)>=sqlx::query_as(
            "SELECT o.operation_id,o.phase,o.outcome,c.physically_closed,c.released FROM node_session_automatic_releases o JOIN node_session_capacity c ON c.reservation_id=o.anchor_reservation WHERE c.reservation_id=? AND c.instance_id=? AND c.runtime_epoch=? AND o.reason=?",
        ).bind(&root.reservation).bind(&root.instance).bind(&root.epoch).bind(reason)
            .fetch_all(&capacity_l6_clock_pool(f)?).await.map_err(|_|"clock_original_finish_query_failed")?;
        if !rows.is_empty() && reason == "idle_timeout" && since.elapsed() < Duration::from_mins(30)
        {
            return Err("clock_idle_request_before_actual_1800");
        }
        if rows.len() > 1 {
            return Err("clock_multiple_operations_for_original_root");
        }
        if let Some((operation, phase, outcome, physical, released)) = rows.first() {
            if phase == "finished"
                && outcome.as_deref() == Some("requested")
                && *physical == 1
                && *released == 1
            {
                return Ok(operation.clone());
            }
            if outcome.as_deref().is_some_and(|o| o != "requested") {
                return Err("clock_real_close_not_confirmed");
            }
        }
        if tokio::time::Instant::now() >= until {
            return Err("clock_original_close_not_finished_within_scan_window");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn capacity_l6_clock_release_restored(
    f: &mut R3WssCase,
    root: &CapacityL6ClockRoot,
) -> CapacityL6Result<()> {
    let node = f.node.as_ref().ok_or("clock_node_missing")?;
    let selection = f.selection.as_ref().ok_or("clock_source_missing")?;
    let target = ipc::target_register(
        node.ipc_address(),
        &selection.source.encode(),
        &root.native,
        &selection.workspace.encode(),
    )
    .await
    .map_err(|_| "clock_restored_target_registration_failed")?
    .target_id;
    let registered = capacity_l6_call(
        f,
        "native.release.register",
        serde_json::json!({"targetId":target}),
    )
    .await?;
    if registered["ok"] != true {
        return Err("clock_restored_release_not_registered");
    }
    let operation = registered["result"]["releaseOperationId"]
        .as_str()
        .ok_or("clock_restored_release_identity_missing")?
        .to_owned();
    let released = capacity_l6_call(
        f,
        "native.release",
        serde_json::json!({"releaseOperationId":operation}),
    )
    .await?;
    if released["ok"] != true || released["result"]["state"] != "released" {
        return Err("clock_restored_real_release_not_confirmed");
    }
    if capacity_l6_clock_state(f, root).await? != (1, 1, 0)
        || capacity_l6_inspect(f).await?["occupied"] != 0
    {
        return Err("clock_restored_original_resource_not_reconciled");
    }
    Ok(())
}

async fn capacity_l6_cp026_body(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    if c.default_limit != 50 || c.idle_seconds != 1800 {
        return Err("clock_requires_real_default_limits");
    }
    if scenario.expected_replay_text.is_empty() || scenario.protected_root_steps.is_empty() {
        return Err("clock_expected_real_replay_text_and_protection_commands_required");
    }
    let cache_native = capacity_l6_new(f).await?;
    let cache = capacity_l6_clock_root(f, cache_native).await?;
    capacity_l6_clock_commands(f, c, &cache.native, &scenario.management_steps).await?;
    let replay_native = capacity_l6_new(f).await?;
    let replay = capacity_l6_clock_root(f, replay_native).await?;
    capacity_l6_clock_commands(f, c, &replay.native, &scenario.management_steps).await?;
    // Prepare both histories before starting the two real idle intervals.
    let (cache_stream, cache_records) =
        capacity_l6_clock_cache_open(f, &cache, &scenario.expected_replay_text).await?;
    let (cache_received, cache_since) = capacity_l6_clock_eligible(f, &cache).await?;
    let initial = capacity_l6_view_records(f, &replay.native, "explicit_refresh").await?;
    if !initial.actual_complete
        || initial.history_incomplete
        || !capacity_l6_clock_has_body(&initial.records, &scenario.expected_replay_text)
    {
        return Err("clock_real_initial_complete_replay_body_missing");
    }
    let (initial_received, initial_since) = capacity_l6_clock_eligible(f, &replay).await?;
    if initial_since
        .checked_duration_since(cache_since)
        .is_none_or(|gap| gap > Duration::from_secs(30))
    {
        return Err("clock_two_real_idle_starts_too_far_apart_for_cache_control");
    }
    let mut cache_ticks = 0_u64;
    while initial_since.elapsed() < Duration::from_mins(29) {
        if capacity_l6_clock_state(f, &cache).await? != (0, 0, 0)
            || capacity_l6_clock_state(f, &replay).await? != (0, 0, 0)
        {
            return Err("clock_control_root_closed_before_near_timeout_replay");
        }
        capacity_l6_clock_cache_visit(f, &cache_records, &scenario.expected_replay_text).await?;
        cache_ticks += 1;
        if capacity_l6_clock_sample(f, &cache)? != (cache_received, Some(cache_since))
            || capacity_l6_clock_sample(f, &replay)? != (initial_received, Some(initial_since))
        {
            return Err("clock_cache_or_unplanned_work_refreshed_idle_time");
        }
        let remaining = Duration::from_mins(29).saturating_sub(initial_since.elapsed());
        tokio::time::sleep(remaining.min(Duration::from_secs(20))).await;
    }
    let refresh_started = initial_since.elapsed().as_millis();
    let refreshed = capacity_l6_view_records(f, &replay.native, "explicit_refresh").await?;
    if !refreshed.actual_complete
        || refreshed.history_incomplete
        || !capacity_l6_clock_has_body(&refreshed.records, &scenario.expected_replay_text)
    {
        return Err("clock_near_timeout_real_replay_body_missing");
    }
    let (replay_received, replay_since) = capacity_l6_clock_eligible(f, &replay).await?;
    if replay_received <= initial_received
        || replay_since < replay_received
        || initial_since.elapsed() >= Duration::from_mins(30)
    {
        return Err("clock_near_timeout_replay_did_not_refresh_original_clock_in_time");
    }
    steps.push(serde_json::json!({"step":"specific_root_near_timeout_replay",
        "root":replay.native,"reservation":replay.reservation,
        "refreshStartedAfterMs":refresh_started,"receivedAfterMs":replay_received.duration_since(initial_since).as_millis()}));
    capacity_l6_mark(steps, "cp026", "actual_replay_received_near_timeout")?;

    // Leave the cache reader until just before its timeout. Client cache visits and
    // the authenticated connection persisted throughout the real interval.
    while cache_since.elapsed() < Duration::from_secs(1798) {
        capacity_l6_clock_cache_visit(f, &cache_records, &scenario.expected_replay_text).await?;
        cache_ticks += 1;
        if capacity_l6_clock_state(f, &cache).await? != (0, 0, 0)
            || capacity_l6_clock_sample(f, &cache)? != (cache_received, Some(cache_since))
        {
            return Err("clock_cached_read_refreshed_activity");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    capacity_l6_clock_cache_close(f, cache_stream).await?;
    capacity_l6_clock_wait_before(f, &cache, cache_received, cache_since, 1799).await?;
    let cache_operation =
        capacity_l6_clock_wait_finished(f, &cache, cache_since, "idle_timeout").await?;
    if cache_ticks == 0 || cache_since.elapsed() < Duration::from_mins(30) {
        return Err("clock_cache_control_did_not_observe_real_thirty_minutes");
    }
    if capacity_l6_clock_state(f, &replay).await? != (0, 0, 0) {
        return Err("clock_cache_timeout_mistakenly_closed_replayed_tree");
    }
    steps.push(
        serde_json::json!({"step":"cache_control_original_timeout","root":cache.native,
        "reservation":cache.reservation,"operation":cache_operation,"clientCacheVisits":cache_ticks,
        "elapsedMs":cache_since.elapsed().as_millis()}),
    );
    capacity_l6_mark(steps, "cp026", "cache_only_actual_1800_no_keepalive")?;
    capacity_l6_clock_wait_before(f, &replay, replay_received, replay_since, 1799).await?;
    steps.push(serde_json::json!({"step":"replayed_original_alive_at_1799","reservation":replay.reservation,
        "elapsedMs":replay_since.elapsed().as_millis()}));
    let replay_operation =
        capacity_l6_clock_wait_finished(f, &replay, replay_since, "idle_timeout").await?;
    steps.push(serde_json::json!({"step":"replayed_original_timeout_after_1800",
        "reservation":replay.reservation,"operation":replay_operation,"elapsedMs":replay_since.elapsed().as_millis()}));
    capacity_l6_mark(steps, "cp026", "after_last_receive_1799_then_1800_and_scan")?;

    // Re-open the same real native history after automatic release. A fresh
    // reservation and a completed native read rule out old client cache.
    let restored_view = capacity_l6_view_records(f, &replay.native, "reopen").await?;
    if !restored_view.actual_complete
        || restored_view.history_incomplete
        || !capacity_l6_clock_has_body(&restored_view.records, &scenario.expected_replay_text)
    {
        return Err("clock_reopened_native_history_or_complete_read_missing");
    }
    let restored = capacity_l6_clock_root(f, replay.native.clone()).await?;
    if restored.reservation == replay.reservation
        || capacity_l6_clock_state(f, &replay).await? != (1, 1, 1)
        || capacity_l6_clock_state(f, &restored).await? != (0, 0, 0)
        || capacity_l6_inspect(f).await?["occupied"] != 1
    {
        return Err("clock_reopened_original_identity_or_new_reservation_invalid");
    }
    capacity_l6_web_collect_phase(
        f,
        c,
        "reopened-root-history",
        serde_json::json!({
            "nativeIds":[replay.native],"operationIds":[replay_operation],
            "occupied":1,"mode":"replay_complete","historyIncomplete":false
        }),
        steps,
    )
    .await?;
    capacity_l6_clock_release_restored(f, &restored).await?;
    steps.push(serde_json::json!({
        "step":"automatically_released_native_history_reopened_and_closed",
        "native":replay.native,"releasedReservation":replay.reservation,
        "restoredReservation":restored.reservation,
        "historyMode":"actual_complete_replay","occupied":0
    }));

    // A separate fresh fifty-tree pressure phase. Protection is accepted only
    // when the real runtime reports every other original tree ineligible.
    let candidate_native = capacity_l6_new(f).await?;
    let candidate = capacity_l6_clock_root(f, candidate_native).await?;
    capacity_l6_clock_commands(f, c, &candidate.native, &scenario.management_steps).await?;
    let mut protected = Vec::new();
    for _ in 0..49 {
        let native = capacity_l6_new(f).await?;
        let root = capacity_l6_clock_root(f, native).await?;
        capacity_l6_clock_commands(f, c, &root.native, &scenario.protected_root_steps).await?;
        protected.push(root);
    }
    let before = capacity_l6_clock_sample(f, &candidate)?.0;
    let recent = capacity_l6_view_records(f, &candidate.native, "explicit_refresh").await?;
    if !recent.actual_complete
        || recent.history_incomplete
        || !capacity_l6_clock_has_body(&recent.records, &scenario.expected_replay_text)
    {
        return Err("clock_pressure_actual_replay_body_missing");
    }
    let (received, since) = capacity_l6_clock_eligible(f, &candidate).await?;
    if received <= before {
        return Err("clock_pressure_replay_was_only_cache");
    }
    // Observe a full normal scan, not stale safe_since snapshots.
    tokio::time::sleep(Duration::from_secs(31)).await;
    for root in &protected {
        if capacity_l6_clock_state(f, root).await? != (0, 0, 0)
            || capacity_l6_clock_sample(f, root)?.1.is_some()
        {
            return Err("clock_fifty_real_roots_not_uniquely_protected");
        }
    }
    if capacity_l6_clock_sample(f, &candidate)?.1.is_none()
        || capacity_l6_inspect(f).await?["occupied"] != 50
    {
        return Err("clock_recent_replay_not_sole_fifty_tree_candidate");
    }
    capacity_l6_mark(steps, "cp026", "fifty_recent_replay_only_candidate")?;
    let pool = capacity_l6_clock_pool(f)?;
    sqlx::query("CREATE TRIGGER l6_clock_finish_fault BEFORE UPDATE OF phase ON node_session_automatic_releases WHEN NEW.phase='finished' BEGIN SELECT RAISE(ABORT,'owned L6 clock finish gate'); END")
        .execute(&pool).await.map_err(|_|"clock_finish_gate_install_failed")?;
    let pressured = capacity_l6_operation(f, serde_json::json!({"kind":"create"})).await;
    let held=async {
        let response=pressured?;
        let rows:Vec<(String,String,Option<String>,i64,i64)>=sqlx::query_as(
            "SELECT o.operation_id,o.phase,o.outcome,c.physically_closed,c.released FROM node_session_automatic_releases o JOIN node_session_capacity c ON c.reservation_id=o.anchor_reservation WHERE c.reservation_id=? AND o.reason='capacity_pressure'",
        ).bind(&candidate.reservation).fetch_all(&pool).await.map_err(|_|"clock_pressure_original_query_failed")?;
        if response["result"]["outcome"]=="succeeded" || rows.len()!=1
            || rows[0].1!="pending" || rows[0].2.as_deref()!=Some("requested")
            || rows[0].3!=1 || rows[0].4!=0 || capacity_l6_inspect(f).await?["occupied"]!=50 {
            return Err("clock_real_closed_but_unfinished_refunded_or_did_not_close_original");
        }
        for root in &protected {
            if capacity_l6_clock_state(f,root).await?!=(0,0,0) {
                return Err("clock_pressure_changed_shared_other_tree");
            }
        }
        Ok(rows[0].0.clone())
    }.await;
    sqlx::query("DROP TRIGGER l6_clock_finish_fault")
        .execute(&pool)
        .await
        .map_err(|_| "clock_finish_gate_remove_failed")?;
    let original_operation = held?;
    let finished =
        capacity_l6_clock_wait_finished(f, &candidate, since, "capacity_pressure").await?;
    if finished != original_operation || capacity_l6_inspect(f).await?["occupied"] != 49 {
        return Err("clock_original_ticket_did_not_coordinate_exactly_once");
    }
    let admitted = capacity_l6_new(f).await?;
    if admitted == candidate.native
        || protected.iter().any(|r| r.native == admitted)
        || capacity_l6_inspect(f).await?["occupied"] != 50
    {
        return Err("clock_new_tree_not_admitted_after_original_finish");
    }
    for root in &protected {
        if capacity_l6_clock_state(f, root).await? != (0, 0, 0) {
            return Err("clock_admission_changed_other_original_tree");
        }
    }
    steps.push(
        serde_json::json!({"step":"recent_replay_pressure_original_finish_then_admission",
        "reservation":candidate.reservation,"operation":finished,"newNative":admitted,
        "necessaryFinishFault":"owned Node SQLite gate; real Harness close","occupied":50}),
    );
    capacity_l6_mark(
        steps,
        "cp026",
        "read_join_close_confirmed_necessary_finish_then_admit",
    )?;
    capacity_l6_mark(steps, "cp026", "shared_tree_isolation_and_history_limits")?;
    // Same-Node DOM and final Job/port cleanup are separately owned and signed.
    Ok(())
}

// This controlled peer regression checks only the cache/heartbeat entry path.
// It never substitutes for the real Harness, fifty roots or 1800-second test.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_cache_control_keeps_one_stream_and_original_clock() {
    r3_wss_case("source-create-replay-state-verified", true, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 211).await;
            let root = capacity_l6_clock_root(f, native)
                .await
                .expect("controlled original root");
            let (stream, records) =
                capacity_l6_clock_cache_open(f, &root, "capacity fixture replay")
                    .await
                    .expect("one completed controlled replay");
            let sample = capacity_l6_clock_sample(f, &root).expect("original clock");
            for _ in 0..2 {
                capacity_l6_clock_cache_visit(f, &records, "capacity fixture replay")
                    .await
                    .expect("cache visit and heartbeat without another rebind");
                assert_eq!(
                    capacity_l6_clock_sample(f, &root).expect("unchanged clock"),
                    sample
                );
                assert_eq!(
                    capacity_l6_clock_state(f, &root)
                        .await
                        .expect("original state"),
                    (0, 0, 0)
                );
            }
            capacity_l6_clock_cache_close(f, stream)
                .await
                .expect("same reader closed");
        })
    })
    .await;
}
