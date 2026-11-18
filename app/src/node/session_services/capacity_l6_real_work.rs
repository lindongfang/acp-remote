// Real model/activity triggers are explicit scenario commands. A prompt being
// accepted never establishes that the expected work or permission exists.
#[derive(Clone)]
struct CapacityL6KnownWork {
    native: String,
    foreground: crate::domain::session_capacity::Coverage,
    background: crate::domain::session_capacity::Coverage,
    tools: usize,
    pending_approvals: usize,
}

fn capacity_l6_known_work(f: &R3WssCase, root: &str) -> CapacityL6Result<Vec<CapacityL6KnownWork>> {
    use crate::application::native_dispatch::runtime_child_source_clock_probe::rt_source_known_work;
    let services = f
        .node
        .as_ref()
        .and_then(|node| node.session_services.as_ref())
        .ok_or("known_work_runtime_missing")?;
    let source = f
        .selection
        .as_ref()
        .ok_or("known_work_source_missing")?
        .source;
    let facts = rt_source_known_work(&services.native).map_err(|_| "known_work_read_failed")?;
    let values = facts
        .into_iter()
        .filter_map(
            |(s, id, foreground, background, tools, parent, permissions)| {
                (s == source && (id.as_ref() == root || parent.as_deref() == Some(root))).then(
                    || CapacityL6KnownWork {
                        native: id.to_string(),
                        foreground,
                        background,
                        tools,
                        pending_approvals: permissions,
                    },
                )
            },
        )
        .collect::<Vec<_>>();
    if !values.iter().any(|value| value.native == root) {
        return Err("real_root_current_activity_not_observed");
    }
    Ok(values)
}

fn capacity_l6_known_matches(facts: &[CapacityL6KnownWork], root: &str, kind: &str) -> bool {
    use crate::domain::session_capacity::Coverage;
    let Some(parent) = facts.iter().find(|fact| fact.native == root) else {
        return false;
    };
    let active = |fact: &CapacityL6KnownWork| {
        fact.foreground == Coverage::Active || fact.background == Coverage::Active || fact.tools > 0
    };
    match kind {
        "foreground" => parent.foreground == Coverage::Active,
        "child_only" => {
            parent.foreground == Coverage::Idle
                && facts.iter().any(|fact| fact.native != root && active(fact))
        }
        "approvals" => {
            facts
                .iter()
                .map(|fact| fact.pending_approvals)
                .sum::<usize>()
                >= 2
        }
        "one_approval" => {
            facts
                .iter()
                .map(|fact| fact.pending_approvals)
                .sum::<usize>()
                == 1
        }
        "known_after_approvals" => {
            facts
                .iter()
                .map(|fact| fact.pending_approvals)
                .sum::<usize>()
                == 0
                && facts.iter().any(active)
        }
        "background" => parent.background == Coverage::Active,
        "unknown_foreground" => parent.foreground == Coverage::Unknown,
        "any_known" => facts
            .iter()
            .any(|fact| active(fact) || fact.pending_approvals > 0),
        _ => false,
    }
}

async fn capacity_l6_require_known(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
    kind: &str,
) -> CapacityL6Result<()> {
    let until = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if capacity_l6_clock_state(f, root).await? != (0, 0, 0) {
            return Err("known_work_original_resource_already_closing");
        }
        if capacity_l6_known_matches(
            &capacity_l6_known_work(f, &root.native)?,
            &root.native,
            kind,
        ) && capacity_l6_clock_sample(f, root)?.1.is_none()
        {
            return Ok(());
        }
        if tokio::time::Instant::now() >= until {
            return Err("required_real_current_work_or_approval_not_observed");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn capacity_l6_phase_commands(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    phase: &str,
    native: &str,
) -> CapacityL6Result<()> {
    capacity_l6_phase_commands_on(f, c, scenario, phase, native, 0).await
}

async fn capacity_l6_phase_commands_on(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    phase: &str,
    native: &str,
    peer_index: usize,
) -> CapacityL6Result<()> {
    let commands = scenario
        .phases
        .get(phase)
        .filter(|commands| !commands.is_empty())
        .ok_or("explicit_real_scenario_phase_commands_missing")?;
    capacity_l6_apply_commands_on(f, c, native, commands, peer_index).await
}

async fn capacity_l6_full_refusal(f: &mut R3WssCase) -> CapacityL6Result<()> {
    let attempted = capacity_l6_operation(f, serde_json::json!({"kind":"create"})).await?;
    if attempted["ok"] != false
        || attempted["error"]["code"] != "session_capacity_full"
        || capacity_l6_inspect(f).await?["occupied"] != 50
    {
        return Err("all_known_work_full_pool_did_not_refuse_new_tree");
    }
    Ok(())
}

async fn capacity_l6_cp011_work_body(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    // All phases have executable normal Node command paths. A vendor may not
    // provide these facts: that is blocked, never invented by a fixture.
    for phase in [
        "long_silent_start",
        "child_only_start",
        "approvals_start",
        "cancel_one_approval",
        "cancel_last_approval",
        "end_long_work",
        "end_child_work",
        "end_remaining_work",
        "restored_input",
        "received_before_send_arm",
        "received_before_send_finish",
        "last_slot_execution",
    ] {
        if scenario.phases.get(phase).is_none_or(Vec::is_empty) {
            return Err("cp011_required_real_work_phase_input_missing");
        }
    }
    if scenario.protected_root_steps.is_empty()
        || scenario.expected_replay_text.is_empty()
        || !c.allow_model_requests
    {
        return Err("cp011_explicit_model_opt_in_and_real_protection_commands_required");
    }
    capacity_l6_last_slot(f, c, scenario, steps).await?;
    let mut roots = Vec::new();
    for index in 0..50 {
        let native = capacity_l6_new(f).await?;
        let root = capacity_l6_clock_root(f, native).await?;
        let phase = match index {
            0 => Some("long_silent_start"),
            1 => Some("child_only_start"),
            2 => Some("approvals_start"),
            _ => None,
        };
        if let Some(phase) = phase {
            capacity_l6_phase_commands(f, c, scenario, phase, &root.native).await?;
        } else {
            capacity_l6_apply_commands(f, c, &root.native, &scenario.protected_root_steps).await?;
        }
        roots.push(root);
    }
    capacity_l6_require_known(f, &roots[0], "foreground").await?;
    capacity_l6_require_known(f, &roots[1], "child_only").await?;
    capacity_l6_require_known(f, &roots[2], "approvals").await?;
    for root in &roots[3..] {
        capacity_l6_require_known(f, root, "any_known").await?;
    }
    if capacity_l6_inspect(f).await?["occupied"] != 50 {
        return Err("cp011_fifty_real_work_trees_not_retained");
    }
    capacity_l6_web_collect_phase(f,c,"known-work-protected",
        serde_json::json!({"nativeIds":roots[..3].iter().map(|root|root.native.clone()).collect::<Vec<_>>(),
            "operationIds":[],"occupied":50}),steps).await?;
    let owner = rt_direct_owner_snapshot(f).await;
    let quiet = capacity_l6_clock_sample(f, &roots[0])?.0;
    let started = std::time::Instant::now();
    capacity_l6_full_refusal(f).await?;
    while started.elapsed() < Duration::from_mins(30) {
        // A truly silent task must not be replaced with a stream of activity
        // that continually refreshes its receipt clock.
        if capacity_l6_clock_sample(f, &roots[0])?.0 != quiet {
            return Err("configured_long_task_was_not_silent_for_actual_1800_seconds");
        }
        for (index, kind) in [(0, "foreground"), (1, "child_only"), (2, "approvals")] {
            if !capacity_l6_known_matches(
                &capacity_l6_known_work(f, &roots[index].native)?,
                &roots[index].native,
                kind,
            ) || capacity_l6_clock_state(f, &roots[index]).await? != (0, 0, 0)
                || capacity_l6_clock_sample(f, &roots[index])?.1.is_some()
            {
                return Err("known_work_or_approval_not_preserved_during_real_1800_seconds");
            }
        }
        if capacity_l6_inspect(f).await?["occupied"] != 50
            || rt_direct_owner_snapshot(f).await != owner
        {
            return Err("protected_real_fifty_owner_or_capacity_changed");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let protected_seconds = started.elapsed().as_secs();
    capacity_l6_mark(steps, "cp011", "long_silent_task_protected_1800")?;
    capacity_l6_mark(steps, "cp011", "only_child_work_protected_1800")?;
    capacity_l6_mark(steps, "cp011", "no_candidate_explicit_refusal")?;
    capacity_l6_phase_commands(f, c, scenario, "cancel_one_approval", &roots[2].native).await?;
    capacity_l6_require_known(f, &roots[2], "one_approval").await?;
    capacity_l6_full_refusal(f).await?;
    capacity_l6_phase_commands(f, c, scenario, "cancel_last_approval", &roots[2].native).await?;
    capacity_l6_require_known(f, &roots[2], "known_after_approvals").await?;
    capacity_l6_full_refusal(f).await?;
    capacity_l6_mark(steps, "cp011", "approval_and_cancel_work_protection_1800")?;
    // All three independent protections end through real control/model work,
    // then a fresh interval begins. Protected duration is never subtracted.
    for (index, phase) in [
        (0, "end_long_work"),
        (1, "end_child_work"),
        (2, "end_remaining_work"),
    ] {
        capacity_l6_phase_commands(f, c, scenario, phase, &roots[index].native).await?;
    }
    let mut intervals = Vec::new();
    for root in &roots[..3] {
        intervals.push(capacity_l6_clock_eligible(f, root).await?);
    }
    for (_, since) in &intervals {
        if *since < started + Duration::from_mins(30) {
            return Err("protection_release_reused_the_old_protected_interval");
        }
    }
    // Observe all original roots on one real timeline. Sequential 1799-second
    // waits can miss another root's boundary; they cannot prove this interval.
    let mut before_seen = [false; 3];
    loop {
        for (index, (root, (received, since))) in roots[..3].iter().zip(&intervals).enumerate() {
            if before_seen[index] {
                continue;
            }
            let elapsed = since.elapsed();
            if elapsed >= Duration::from_mins(30) {
                return Err("known_work_fresh_interval_1799_observation_missed");
            }
            if capacity_l6_clock_state(f, root).await? != (0, 0, 0)
                || capacity_l6_clock_sample(f, root)? != (*received, Some(*since))
            {
                return Err("known_work_fresh_interval_original_clock_or_owner_changed");
            }
            if elapsed >= Duration::from_secs(1799) {
                before_seen[index] = true;
            }
        }
        if before_seen.iter().all(|seen| *seen) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    for (root, (_, since)) in roots[..3].iter().zip(&intervals) {
        capacity_l6_clock_wait_finished(f, root, *since, "idle_timeout").await?;
    }
    for root in &roots[3..] {
        if capacity_l6_clock_state(f, root).await? != (0, 0, 0) {
            return Err("ending_one_tree_work_closed_shared_other_real_tree");
        }
    }
    capacity_l6_mark(steps, "cp011", "unprotected_restarts_actual_1800")?;
    steps.push(serde_json::json!({"step":"real_known_work_and_approval_intervals","protectedSeconds":protected_seconds,"fiftyRealRoots":50,"freshIdleIntervals":3}));
    let prior = roots[0].clone();
    let restored = capacity_l6_view_records(f, &prior.native, "explicit_refresh").await?;
    let original = capacity_l6_clock_root(f, prior.native.clone()).await?;
    if !restored.actual_complete
        || original.reservation == prior.reservation
        || !capacity_l6_clock_has_body(&restored.records, &scenario.expected_replay_text)
    {
        return Err("closed_original_real_root_history_not_restored_in_new_reservation");
    }
    let target = capacity_l6_register_target(f, &prior.native).await?;
    let pool = capacity_l6_clock_pool(f)?;
    let before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE target_id=?")
            .bind(target.encode())
            .fetch_one(&pool)
            .await
            .map_err(|_| "restored_input_before_query_failed")?;
    capacity_l6_phase_commands(f, c, scenario, "restored_input", &prior.native).await?;
    timeout(Duration::from_secs(90),async{
        loop {
            let written:i64=sqlx::query_scalar("SELECT count(DISTINCT d.dispatch_id) FROM node_native_dispatches d JOIN node_native_write_attempts t ON t.dispatch_id=d.dispatch_id WHERE d.target_id=? AND t.transport_state='written'")
                .bind(target.encode()).fetch_one(&pool).await.map_err(|_|"restored_input_written_query_failed")?;
            if written>before{break Ok::<(),&'static str>(());}
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.map_err(|_|"restored_real_root_input_not_actually_written")??;
    for root in &roots[3..] {
        if capacity_l6_clock_state(f, root).await? != (0, 0, 0) {
            return Err("restored_root_input_changed_shared_other_tree");
        }
    }
    let reopened_occupied = capacity_l6_inspect(f).await?["occupied"].clone();
    capacity_l6_web_collect_phase(
        f,
        c,
        "reopened-root-input",
        serde_json::json!({"nativeIds":[prior.native],"operationIds":[],
            "occupied":reopened_occupied,
            "mode":restored.mode,"historyIncomplete":restored.history_incomplete}),
        steps,
    )
    .await?;
    capacity_l6_mark(steps, "cp011", "shared_tree_untouched_and_restore_input")?;
    capacity_l6_release_all(f).await?;
    // The same real cache/replay program is reused as an explicit subphase,
    // retaining its own per-root/1799/1800 assertions rather than shortening it.
    capacity_l6_cp026_body(f, c, scenario, steps).await?;
    capacity_l6_mark(steps, "cp011", "cache_and_replay_distinct_actual_clocks")?;
    capacity_l6_pre_send_recheck(f, c, scenario, "cp011", steps).await?;
    capacity_l6_original_ticket_faults(f, c, scenario, "cp011", steps).await
}
