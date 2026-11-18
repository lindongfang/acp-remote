// Real CP032 observations. Scenario commands trigger work; only original
// received ACP facts and the original Node owners establish branch evidence.
#[derive(Clone, Debug)]
struct CapacityL6CapabilityPost {
    session: String,
    foreground: crate::domain::session_capacity::Coverage,
    background: crate::domain::session_capacity::Coverage,
    active_tools: Vec<String>,
}

#[derive(Clone, Debug)]
struct CapacityL6CapabilityFact {
    session: String,
    kind: String,
    state: Option<String>,
    background: Option<bool>,
    background_missing: bool,
    tool: Option<String>,
    tool_active: Option<bool>,
    tool_status_missing: bool,
    digest: String,
    current_receipt: Option<(u64, u64)>,
    post: Option<CapacityL6CapabilityPost>,
}

fn capacity_l6_capability_background(value: &serde_json::Value) -> Option<bool> {
    let items = value.as_array()?;
    for item in items {
        let reference = match item["type"].as_str()? {
            "terminal" => Some("terminalId"),
            "tool_call" => Some("toolCallId"),
            "subagent" => Some("sessionId"),
            _ => None,
        };
        if let Some(reference) = reference
            && item[reference].as_str().is_none()
        {
            return None;
        }
    }
    Some(!items.is_empty())
}

fn capacity_l6_capability_fact(bytes: &[u8]) -> Option<CapacityL6CapabilityFact> {
    let message: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    if message["method"] != "session/update" {
        return None;
    }
    let params = message.get("params")?;
    let update = params.get("update")?;
    let kind = update["sessionUpdate"].as_str()?;
    if !["state_update", "tool_call", "tool_call_update"].contains(&kind) {
        return None;
    }
    let status = update.get("status");
    Some(CapacityL6CapabilityFact {
        session: params["sessionId"].as_str()?.into(),
        kind: kind.into(),
        state: update["state"].as_str().map(str::to_owned),
        background: update
            .get("background")
            .and_then(capacity_l6_capability_background),
        background_missing: update
            .get("background")
            .is_none_or(serde_json::Value::is_null),
        tool: update["toolCallId"].as_str().map(str::to_owned),
        tool_active: status.and_then(|value| {
            if value.is_null() {
                Some(true)
            } else {
                value
                    .as_str()
                    .map(|state| !["completed", "failed", "cancelled"].contains(&state))
            }
        }),
        tool_status_missing: status.is_none(),
        digest: capacity_l6_sha(bytes),
        current_receipt: None,
        post: None,
    })
}

fn capacity_l6_capability_raw(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
) -> CapacityL6Result<Vec<(usize, CapacityL6CapabilityFact)>> {
    capacity_l6_capability_raw_for(f, root, false)
}

fn capacity_l6_capability_tree_raw(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
) -> CapacityL6Result<Vec<(usize, CapacityL6CapabilityFact)>> {
    capacity_l6_capability_raw_for(f, root, true)
}

fn capacity_l6_capability_raw_for(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
    include_children: bool,
) -> CapacityL6Result<Vec<(usize, CapacityL6CapabilityFact)>> {
    use crate::application::native_dispatch::runtime_child_source_clock_probe::{
        l6_capacity_receipts, l6_instance_raw_facts,
    };
    let services = f
        .node
        .as_ref()
        .and_then(|node| node.session_services.as_ref())
        .ok_or("capability_original_runtime_missing")?;
    let source = f
        .selection
        .as_ref()
        .ok_or("capability_original_source_missing")?
        .source;
    let allowed_sessions: std::collections::HashSet<String> = if include_children {
        let mut set: std::collections::HashSet<String> = capacity_l6_known_work(f, &root.native)
            .unwrap_or_default()
            .into_iter()
            .map(|fact| fact.native)
            .collect();
        set.insert(root.native.clone());
        set
    } else {
        std::iter::once(root.native.clone()).collect()
    };
    let receipts = l6_capacity_receipts(&services.native, source)
        .map_err(|_| "capability_current_receipt_collector_missing")?;
    Ok(l6_instance_raw_facts(&services.native, |raw| {
        capacity_l6_capability_fact(raw.bytes())
    })
    .map_err(|_| "capability_original_raw_observer_failed")?
    .into_iter()
    .filter_map(|(actual, instance, index, mut fact)| {
        if actual != source
            || instance.encode() != root.instance
            || !allowed_sessions.contains(&fact.session)
        {
            return None;
        }
        let matching = receipts
            .iter()
            .filter(|receipt| {
                receipt.source == source
                    && receipt.instance.encode() == root.instance
                    && receipt.raw_index == index
            })
            .collect::<Vec<_>>();
        if matching.len() == 1
            && matching[0].freshness == "current"
            && matching[0].session.as_ref() == fact.session
        {
            fact.current_receipt = Some((matching[0].sequence, matching[0].receipt));
            fact.post = matching[0]
                .post
                .as_ref()
                .map(|post| CapacityL6CapabilityPost {
                    session: matching[0].session.to_string(),
                    foreground: post.foreground,
                    background: post.background,
                    active_tools: post.active_tools.iter().map(ToString::to_string).collect(),
                });
        }
        Some((index, fact))
    })
    .collect())
}

#[derive(Clone, Copy)]
enum CapacityL6CapabilityExpected {
    Unprovided,
    Background,
    Tool,
    ToolEnded,
    UnknownForeground,
    Idle,
}

fn capacity_l6_capability_matching(
    facts: Vec<(usize, CapacityL6CapabilityFact)>,
    before: Option<usize>,
    expected: CapacityL6CapabilityExpected,
    accepts: impl Fn(&CapacityL6CapabilityFact) -> bool,
) -> Option<(usize, CapacityL6CapabilityFact)> {
    facts.into_iter().rev().find(|(index, fact)| {
        before.is_none_or(|old| *index > old)
            && fact.current_receipt.is_some()
            && capacity_l6_capability_post_matches(fact, expected)
            && accepts(fact)
    })
}

fn capacity_l6_capability_post_matches(
    fact: &CapacityL6CapabilityFact,
    expected: CapacityL6CapabilityExpected,
) -> bool {
    use crate::domain::session_capacity::Coverage;
    let Some(post) = &fact.post else {
        return false;
    };
    if post.session != fact.session {
        return false;
    }
    match expected {
        CapacityL6CapabilityExpected::Background => post.background == Coverage::Active,
        CapacityL6CapabilityExpected::Tool => fact
            .tool
            .as_ref()
            .is_some_and(|tool| post.active_tools.contains(tool)),
        CapacityL6CapabilityExpected::ToolEnded => fact
            .tool
            .as_ref()
            .is_some_and(|tool| !post.active_tools.contains(tool)),
        CapacityL6CapabilityExpected::Unprovided => {
            post.foreground == Coverage::Idle
                && post.background == Coverage::Unknown
                && post.active_tools.is_empty()
        }
        CapacityL6CapabilityExpected::UnknownForeground => post.foreground == Coverage::Unknown,
        CapacityL6CapabilityExpected::Idle => {
            post.foreground == Coverage::Idle
                && post.background != Coverage::Active
                && post.active_tools.is_empty()
        }
    }
}

fn capacity_l6_capability_current(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
    expected: CapacityL6CapabilityExpected,
) -> CapacityL6Result<bool> {
    use crate::domain::session_capacity::Coverage;
    let facts = capacity_l6_known_work(f, &root.native)?;
    let fact = facts
        .iter()
        .find(|fact| fact.native == root.native)
        .ok_or("capability_original_current_state_missing")?;
    let eligible = capacity_l6_clock_sample(f, root)?.1.is_some();
    Ok(match expected {
        CapacityL6CapabilityExpected::Unprovided => {
            fact.foreground == Coverage::Idle
                && fact.background == Coverage::Unknown
                && fact.tools == 0
                && fact.pending_approvals == 0
                && eligible
        }
        CapacityL6CapabilityExpected::Background => {
            fact.background == Coverage::Active && !eligible
        }
        CapacityL6CapabilityExpected::Tool => fact.tools > 0 && !eligible,
        CapacityL6CapabilityExpected::UnknownForeground => {
            fact.foreground == Coverage::Unknown && !eligible
        }
        CapacityL6CapabilityExpected::Idle | CapacityL6CapabilityExpected::ToolEnded => {
            fact.foreground == Coverage::Idle
                && fact.background != Coverage::Active
                && fact.tools == 0
                && fact.pending_approvals == 0
                && eligible
        }
    })
}

async fn capacity_l6_capability_phase(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    root: &CapacityL6ClockRoot,
    phase: &str,
    expected: CapacityL6CapabilityExpected,
    accepts: impl Fn(&CapacityL6CapabilityFact) -> bool,
) -> CapacityL6Result<(usize, CapacityL6CapabilityFact)> {
    let before = capacity_l6_capability_raw(f, root)?
        .last()
        .map(|(index, _)| *index);
    capacity_l6_phase_commands(f, c, scenario, phase, &root.native).await?;
    timeout(Duration::from_secs(60), async {
        loop {
            if capacity_l6_clock_state(f, root).await? != (0, 0, 0) {
                return Err("capability_phase_changed_original_live_reservation");
            }
            if capacity_l6_capability_current(f, root, expected)?
                && let Some(observed) = capacity_l6_capability_matching(
                    capacity_l6_capability_raw(f, root)?,
                    before,
                    expected,
                    &accepts,
                )
            {
                return Ok(observed);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "required_real_capability_transition_not_observed")?
}

fn capacity_l6_capability_step(
    steps: &mut Vec<serde_json::Value>,
    root: &CapacityL6ClockRoot,
    phase: &str,
    observed: &(usize, CapacityL6CapabilityFact),
) {
    steps.push(serde_json::json!({
        "step":phase,"sourceNative":root.native,
        "reservation":root.reservation,"instance":root.instance,"runtimeEpoch":root.epoch,
        "originalRawIndex":observed.0,"originalRawSha256":observed.1.digest,
        "actualCurrentSequenceAndReceipt":observed.1.current_receipt,
        "currentOwnerFactsChecked":true,"sameReceiptProjectionChecked":true,
        "claimsCompleteHiddenCoverage":false
    }));
}

async fn capacity_l6_capability_inventory(f: &R3WssCase) -> CapacityL6Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT json_array(reservation_id,source_id,native_id,instance_id,runtime_epoch,root_verified,physically_closed,released) FROM node_session_capacity ORDER BY reservation_id",
    ).fetch_all(&capacity_l6_clock_pool(f)?).await
        .map_err(|_| "capability_original_inventory_unavailable")
}

fn capacity_l6_capability_tool_retained(
    f: &R3WssCase,
    root: &CapacityL6ClockRoot,
    tool: &str,
) -> CapacityL6Result<bool> {
    use crate::application::native_dispatch::runtime_child_source_clock_probe::l6_active_tools;
    let services = f
        .node
        .as_ref()
        .and_then(|node| node.session_services.as_ref())
        .ok_or("capability_tool_owner_missing")?;
    let source = f
        .selection
        .as_ref()
        .ok_or("capability_tool_source_missing")?
        .source;
    let observed =
        l6_active_tools(&services.native).map_err(|_| "capability_tool_observer_failed")?;
    Ok(observed.iter().any(|(s, instance, n, tools)| {
        *s == source
            && instance.encode() == root.instance
            && n.as_ref() == root.native
            && tools.iter().any(|id| id.as_ref() == tool)
    }))
}

/// 仅查找并终止从属于当前测试进程后代树的 `codex.exe app-server` 子进程。
#[cfg(windows)]
fn capacity_l6_kill_owned_app_server() -> CapacityL6Result<Vec<u32>> {
    let parent_pid = std::process::id();
    let script = format!(
        r#"
$parentPid = {parent_pid}
$procs = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name, CommandLine
$descendants = [Collections.Generic.HashSet[int]]::new()
$queue = [Collections.Generic.Queue[int]]::new()
$queue.Enqueue($parentPid)
while ($queue.Count -gt 0) {{
  $cur = $queue.Dequeue()
  foreach ($p in $procs) {{
    if ([int]$p.ParentProcessId -eq $cur -and $descendants.Add([int]$p.ProcessId)) {{
      $queue.Enqueue([int]$p.ProcessId)
    }}
  }}
}}
$targets = @($procs | Where-Object {{
  $descendants.Contains([int]$_.ProcessId) -and $_.Name -ieq 'codex.exe' -and $_.CommandLine -like '*app-server*'
}})
$killed = @()
foreach ($t in $targets) {{
  Stop-Process -Id ([int]$t.ProcessId) -Force
  $killed += [int]$t.ProcessId
}}
ConvertTo-Json @($killed) -Compress
"#
    );
    let output = std::process::Command::new("pwsh.exe")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|_| "owned_app_server_kill_invocation_failed")?;
    if !output.status.success() {
        return Err("owned_app_server_kill_command_failed");
    }
    let killed: Vec<u32> =
        serde_json::from_slice(&output.stdout).map_err(|_| "owned_app_server_kill_parse_failed")?;
    if killed.is_empty() {
        return Err("owned_codex_app_server_not_found");
    }
    Ok(killed)
}

fn capacity_l6_tree_has_active_child(f: &R3WssCase, root: &CapacityL6ClockRoot) -> CapacityL6Result<bool> {
    use crate::domain::session_capacity::Coverage;
    let facts = capacity_l6_known_work(f, &root.native)?;
    Ok(facts.iter().any(|fact| {
        fact.native != root.native
            && (fact.foreground == Coverage::Active
                || fact.background == Coverage::Active
                || fact.tools > 0)
    }))
}

fn capacity_l6_tree_unknown_protected(f: &R3WssCase, root: &CapacityL6ClockRoot) -> CapacityL6Result<bool> {
    use crate::domain::session_capacity::Coverage;
    let facts = capacity_l6_known_work(f, &root.native)?;
    let eligible = capacity_l6_clock_sample(f, root)?.1.is_some();
    Ok(!eligible
        && facts
            .iter()
            .any(|fact| fact.foreground == Coverage::Unknown))
}

async fn capacity_l6_capability_missing(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    root: &CapacityL6ClockRoot,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    use crate::domain::session_capacity::Coverage;
    use CapacityL6CapabilityExpected as E;
    let before_start = capacity_l6_capability_tree_raw(f, root)?
        .last()
        .map(|(index, _)| *index);
    capacity_l6_phase_commands(f, c, scenario, "capability_background_start", &root.native).await?;
    let (active, child_branch) = timeout(Duration::from_secs(60), async {
        loop {
            if capacity_l6_clock_state(f, root).await? != (0, 0, 0) {
                return Err("capability_phase_changed_original_live_reservation");
            }
            if capacity_l6_capability_current(f, root, E::Background)?
                && let Some(observed) = capacity_l6_capability_matching(
                    capacity_l6_capability_raw(f, root)?,
                    before_start,
                    E::Background,
                    |fact| fact.kind == "state_update" && fact.background == Some(true),
                )
            {
                return Ok((observed, false));
            }
            let eligible = capacity_l6_clock_sample(f, root)?.1.is_some();
            if !eligible
                && capacity_l6_tree_has_active_child(f, root)?
                && let Some(observed) = capacity_l6_capability_tree_raw(f, root)?
                    .into_iter()
                    .rev()
                    .find(|(index, fact)| {
                        before_start.is_none_or(|old| *index > old)
                            && fact.current_receipt.is_some()
                            && fact.post.as_ref().is_some_and(|post| {
                                post.session == fact.session
                                    && (post.foreground == Coverage::Active
                                        || !post.active_tools.is_empty())
                            })
                    })
            {
                return Ok((observed, true));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "required_real_capability_transition_not_observed")??;
    capacity_l6_capability_step(steps, root, "real_known_background_started", &active);

    let before_missing = capacity_l6_capability_tree_raw(f, root)?
        .last()
        .map(|(index, _)| *index);
    capacity_l6_phase_commands(f, c, scenario, "capability_background_missing", &root.native).await?;
    let missing = timeout(Duration::from_secs(60), async {
        loop {
            if capacity_l6_clock_state(f, root).await? != (0, 0, 0) {
                return Err("capability_phase_changed_original_live_reservation");
            }
            if !child_branch {
                if capacity_l6_capability_current(f, root, E::Background)?
                    && let Some(observed) = capacity_l6_capability_matching(
                        capacity_l6_capability_raw(f, root)?,
                        before_missing,
                        E::Background,
                        |fact| fact.kind == "state_update" && fact.background_missing,
                    )
                {
                    return Ok(observed);
                }
            } else {
                // 真实 codex-acp 不发 background 字段：父收到缺 background 字段的 idle state_update 后，已知子代理工作仍保留保护。
                let eligible = capacity_l6_clock_sample(f, root)?.1.is_some();
                if !eligible
                    && capacity_l6_tree_has_active_child(f, root)?
                    && let Some(observed) = capacity_l6_capability_tree_raw(f, root)?
                        .into_iter()
                        .rev()
                        .find(|(index, fact)| {
                            before_missing.is_none_or(|old| *index > old)
                                && fact.current_receipt.is_some()
                                && fact.session == root.native
                                && fact.kind == "state_update"
                                && fact.state.as_deref() == Some("idle")
                                && fact.background_missing
                        })
                {
                    return Ok(observed);
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "required_real_capability_transition_not_observed")??;
    capacity_l6_capability_step(
        steps,
        root,
        "missing_background_kept_original_work",
        &missing,
    );

    let ended = capacity_l6_capability_phase(
        f,
        c,
        scenario,
        root,
        "capability_background_end",
        E::Idle,
        |fact| {
            fact.kind == "state_update"
                && fact.state.as_deref() == Some("idle")
                && (child_branch || fact.background == Some(false))
        },
    )
    .await?;
    capacity_l6_capability_step(steps, root, "reliable_background_end_observed", &ended);

    let tool = capacity_l6_capability_phase(
        f,
        c,
        scenario,
        root,
        "capability_tool_start",
        E::Tool,
        |fact| fact.tool.is_some() && fact.tool_active == Some(true),
    )
    .await?;
    let tool_id = tool
        .1
        .tool
        .as_deref()
        .ok_or("capability_actual_tool_identity_missing")?;
    if !capacity_l6_capability_tool_retained(f, root, tool_id)? {
        return Err("capability_received_tool_not_current_original_work");
    }
    let missing = capacity_l6_capability_phase(
        f,
        c,
        scenario,
        root,
        "capability_tool_missing",
        E::Tool,
        |fact| {
            fact.kind == "tool_call_update"
                && fact.tool.as_deref() == Some(tool_id)
                && fact.tool_status_missing
        },
    )
    .await?;
    if !capacity_l6_capability_tool_retained(f, root, tool_id)? {
        return Err("capability_missing_status_cleared_same_original_tool");
    }
    capacity_l6_capability_step(
        steps,
        root,
        "missing_same_tool_status_kept_original_work",
        &missing,
    );
    let ended = capacity_l6_capability_phase(
        f,
        c,
        scenario,
        root,
        "capability_tool_end",
        E::ToolEnded,
        |fact| fact.tool.as_deref() == Some(tool_id) && fact.tool_active == Some(false),
    )
    .await?;
    capacity_l6_capability_step(steps, root, "reliable_same_tool_end_observed", &ended);
    capacity_l6_mark(steps, "cp032", "missing_fields_do_not_clear_known_work")
}

async fn capacity_l6_cp032_capabilities(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    scenario: &CapacityL6Scenario,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    use crate::domain::session_capacity::Coverage;
    if capacity_l6_inspect(f).await?["occupied"] != 0 || scenario.protected_root_steps.is_empty() {
        return Err("capability_entry_requires_empty_pool_and_real_protection_commands");
    }
    for phase in [
        "capability_background_start",
        "capability_background_missing",
        "capability_background_end",
        "capability_tool_start",
        "capability_tool_missing",
        "capability_tool_end",
        "capability_unprovided_idle",
        "capability_protect_unprovided",
        "capability_foreground_unknown",
        "capability_candidate_idle",
    ] {
        if scenario.phases.get(phase).is_none_or(Vec::is_empty) {
            return Err("capability_required_real_phase_input_missing");
        }
    }
    let services = f
        .node
        .as_ref()
        .and_then(|node| node.session_services.as_ref())
        .ok_or("capability_original_runtime_missing")?;
    let source = f
        .selection
        .as_ref()
        .ok_or("capability_original_source_missing")?
        .source;
    let receipts=crate::application::native_dispatch::runtime_child_source_clock_probe::
        l6_watch_capacity_receipts(&services.native,source)
            .map_err(|_|"capability_current_receipt_registration_failed")?;

    // 树 #1 (invalid_root)：先建立子代理关联并等待保护就位，再精准终止本测试进程所属 codex.exe app-server。
    // codex-acp AppServerRecovery 会发出真实 state_update(unknown) 并自动重启新一代 app-server。
    let invalid_native = capacity_l6_new(f).await?;
    let invalid_root = capacity_l6_clock_root(f, invalid_native).await?;
    capacity_l6_phase_commands(
        f,
        c,
        scenario,
        "capability_foreground_unknown",
        &invalid_root.native,
    )
    .await?;
    timeout(Duration::from_secs(60), async {
        loop {
            if !capacity_l6_clock_sample(f, &invalid_root)?.1.is_none()
                || !capacity_l6_tree_has_active_child(f, &invalid_root)?
            {
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
            return Ok::<_, &'static str>(());
        }
    })
    .await
    .map_err(|_| "invalid_root_subagent_protection_not_observed")??;
    let before_kill = capacity_l6_capability_tree_raw(f, &invalid_root)?
        .last()
        .map(|(index, _)| *index);
    let _killed_pids = capacity_l6_kill_owned_app_server()?;
    let unknown = timeout(Duration::from_secs(60), async {
        loop {
            if capacity_l6_clock_state(f, &invalid_root).await? != (0, 0, 0) {
                return Err("capability_phase_changed_original_live_reservation");
            }
            if capacity_l6_tree_unknown_protected(f, &invalid_root)?
                && let Some(observed) = capacity_l6_capability_tree_raw(f, &invalid_root)?
                    .into_iter()
                    .rev()
                    .find(|(index, fact)| {
                        before_kill.is_none_or(|old| *index > old)
                            && fact.current_receipt.is_some()
                            && fact.kind == "state_update"
                            && fact
                                .state
                                .as_deref()
                                .is_some_and(|state| {
                                    !["idle", "running", "requires_action"].contains(&state)
                                })
                            && fact
                                .post
                                .as_ref()
                                .is_some_and(|post| post.foreground == Coverage::Unknown)
                    })
            {
                return Ok(observed);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "required_real_unknown_foreground_not_observed")??;
    capacity_l6_capability_step(
        steps,
        &invalid_root,
        "actual_unknown_foreground_protected",
        &unknown,
    );
    capacity_l6_mark(steps, "cp032", "foreground_or_connection_invalid_protects")?;

    // 树 #2 (unprovided)：新创建的会话自动验证新一代 app-server 可用，且无 background 字段时不阻断候选判定。
    let unprovided_native = capacity_l6_new(f).await?;
    let unprovided = capacity_l6_clock_root(f, unprovided_native).await?;
    let observed = capacity_l6_capability_phase(
        f,
        c,
        scenario,
        &unprovided,
        "capability_unprovided_idle",
        CapacityL6CapabilityExpected::Unprovided,
        |fact| {
            fact.kind == "state_update"
                && fact.state.as_deref() == Some("idle")
                && fact.background_missing
        },
    )
    .await?;
    // Unknown is not a synonym for a complete empty background snapshot.
    // Every actual state in this new original owner must have omitted coverage.
    if capacity_l6_capability_raw(f, &unprovided)?
        .iter()
        .any(|(_, fact)| {
            fact.current_receipt.is_some()
                && fact.kind == "state_update"
                && !fact.background_missing
        })
    {
        return Err("configured_unprovided_root_had_actual_background_coverage");
    }
    capacity_l6_capability_step(
        steps,
        &unprovided,
        "unprovided_scope_still_eligible",
        &observed,
    );
    capacity_l6_mark(steps, "cp032", "real_known_work_vs_unprovided_coverage")?;
    capacity_l6_phase_commands(
        f,
        c,
        scenario,
        "capability_protect_unprovided",
        &unprovided.native,
    )
    .await?;
    capacity_l6_require_known(f, &unprovided, "any_known").await?;

    // 树 #3 (subject)：完成真实缺失字段测试后立即施加已知保护，随后补齐其余 47 棵受保护树共占 50/50。
    let subject_native = capacity_l6_new(f).await?;
    let subject = capacity_l6_clock_root(f, subject_native).await?;
    capacity_l6_capability_missing(f, c, scenario, &subject, steps).await?;
    capacity_l6_apply_commands(f, c, &subject.native, &scenario.protected_root_steps).await?;
    capacity_l6_require_known(f, &subject, "any_known").await?;

    let mut protected = vec![unprovided];
    for _ in 0..47 {
        let native = capacity_l6_new(f).await?;
        let root = capacity_l6_clock_root(f, native).await?;
        capacity_l6_apply_commands(f, c, &root.native, &scenario.protected_root_steps).await?;
        capacity_l6_require_known(f, &root, "any_known").await?;
        protected.push(root);
    }
    for root in &protected {
        capacity_l6_require_known(f, root, "any_known").await?;
    }
    capacity_l6_require_known(f, &subject, "any_known").await?;
    if !capacity_l6_tree_unknown_protected(f, &invalid_root)?
        || capacity_l6_inspect(f).await?["occupied"] != 50
    {
        return Err("capability_fifty_original_protections_not_current");
    }
    let before = capacity_l6_capability_inventory(f).await?;
    capacity_l6_full_refusal(f).await?;
    if capacity_l6_capability_inventory(f).await? != before {
        return Err("capability_full_refusal_changed_original_responsibility");
    }
    let idle = capacity_l6_capability_phase(
        f,
        c,
        scenario,
        &subject,
        "capability_candidate_idle",
        CapacityL6CapabilityExpected::Idle,
        |fact| fact.kind == "state_update" && fact.state.as_deref() == Some("idle"),
    )
    .await?;
    for root in &protected {
        capacity_l6_require_known(f, root, "any_known").await?;
    }
    if !capacity_l6_tree_unknown_protected(f, &invalid_root)?
        || capacity_l6_inspect(f).await?["occupied"] != 50
    {
        return Err("capability_candidate_phase_changed_fifty_before_request");
    }
    capacity_l6_capability_step(
        steps,
        &subject,
        "fifty_with_one_real_candidate_after_refusal",
        &idle,
    );
    capacity_l6_mark(steps, "cp032", "fifty_candidate_and_refusal")?;
    let collected = capacity_l6_web_collect_phase(
        f,
        c,
        "capability-fifty-known-and-unprovided",
        serde_json::json!({"nativeIds":[subject.native],"operationIds":[],"occupied":50}),
        steps,
    )
    .await;
    let _ = receipts
        .records()
        .map_err(|_| "capability_receipt_collection_failed")?;
    collected?;
    for root in &protected {
        capacity_l6_require_known(f, root, "any_known").await?;
    }
    if !capacity_l6_tree_unknown_protected(f, &invalid_root)?
        || !capacity_l6_capability_current(f, &subject, CapacityL6CapabilityExpected::Idle)?
        || capacity_l6_clock_state(f, &subject).await? != (0, 0, 0)
        || capacity_l6_inspect(f).await?["occupied"] != 50
    {
        return Err("capability_web_handoff_changed_final_candidate_or_capacity");
    }
    Ok(())
}

async fn capacity_l6_capability_operation(
    f: &R3WssCase,
    operation: &str,
) -> CapacityL6Result<(String, Vec<String>)> {
    let pool = capacity_l6_clock_pool(f)?;
    let record = sqlx::query_scalar(
        "SELECT json_array(operation_id,anchor_reservation,reason,phase,outcome) FROM node_session_automatic_releases WHERE operation_id=?",
    ).bind(operation).fetch_one(&pool).await.map_err(|_|"restart_original_system_operation_missing")?;
    let members:Vec<String> = sqlx::query_scalar(
        "SELECT json_array(m.reservation_id,c.source_id,c.native_id,c.instance_id,c.runtime_epoch,c.physically_closed,c.released) FROM node_session_automatic_release_members m JOIN node_session_capacity c ON c.reservation_id=m.reservation_id WHERE m.operation_id=? ORDER BY m.reservation_id",
    ).bind(operation).fetch_all(&pool).await.map_err(|_|"restart_original_system_members_missing")?;
    if members.is_empty() {
        return Err("restart_original_system_member_set_empty");
    }
    Ok((record, members))
}

type CapacityL6RestartOriginal = (String, String, String, String, String, String, i64, i64);

async fn capacity_l6_cp032_restart(
    f: &mut R3WssCase,
    _c: &CapacityL6Config,
    operation: &str,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    use crate::application::native_dispatch::runtime_child_source_clock_probe::{
        l6_automatic_waits, rt_source_activity_clocks,
    };
    if !steps.iter().any(|step| {
        step["step"] == "same_original_repair"
            && step["operationId"] == operation
            && step["releasedExactlyOnce"] == true
    }) {
        return Err("restart_requires_caller_original_necessary_repair_evidence");
    }
    let source = f
        .selection
        .as_ref()
        .ok_or("restart_original_source_missing")?
        .source
        .encode();
    let pool = capacity_l6_clock_pool(f)?;
    let original:Option<CapacityL6RestartOriginal> = sqlx::query_as(
        "SELECT c.native_id,c.reservation_id,c.instance_id,c.runtime_epoch,o.phase,o.outcome,c.physically_closed,c.released FROM node_session_automatic_releases o JOIN node_session_capacity c ON c.reservation_id=o.anchor_reservation WHERE o.operation_id=? AND c.source_id=?",
    ).bind(operation).bind(source).fetch_optional(&pool).await.map_err(|_|"restart_original_repair_query_failed")?;
    let (native, reservation, instance, epoch, phase, outcome, closed, released) =
        original.ok_or("restart_original_repair_missing")?;
    if phase != "finished" || outcome != "requested" || closed != 1 || released != 1 {
        return Err("restart_requires_confirmed_original_written_close_and_finish");
    }
    let fixed = capacity_l6_capability_operation(f, operation).await?;
    drop(pool);
    let node = f.node.as_ref().ok_or("restart_original_node_missing")?;
    let node_id = node.node_id().to_owned();
    let old_address = node.listen_addr();
    let retained = node
        .session_services
        .as_ref()
        .ok_or("restart_original_services_missing")?
        .native
        .clone();
    let key = f
        .peers
        .first()
        .ok_or("restart_original_authenticated_actor_missing")?
        .key
        .clone();
    let client = f.peers[0].client_id.clone();
    // R3 close joins the Node owner and clears sockets; TestArea is a separate
    // field and remains alive. Never recreate the Catalog or copy credentials.
    if !f.close().await {
        return Err("restart_original_node_join_unconfirmed");
    }
    let shutdown = retained
        .last_shutdown_report()
        .filter(|report| report.joined && report.metadata_confirmed)
        .ok_or("restart_original_runtime_join_or_metadata_unconfirmed")?;
    let diagnostics = retained.source_close_diagnostics();
    if diagnostics.is_empty()
        || diagnostics.iter().any(|source| {
            !source.physical_present
                || !source.metadata_close.as_ref().is_some_and(Result::is_ok)
                || !matches!(
                    source.physical_owner,
                    Some(
                        crate::application::native_dispatch::OwnerCloseKind::WindowsJob {
                            active_processes: 0
                        }
                    )
                )
        })
    {
        return Err("restart_original_job_or_necessary_closure_unconfirmed");
    }
    let port_check_started = std::time::Instant::now();
    match timeout(
        Duration::from_secs(10),
        tokio::net::TcpStream::connect(old_address),
    )
    .await
    {
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {}
        Ok(Err(_)) => return Err("restart_original_port_other_connection_error"),
        Ok(Ok(_)) => return Err("restart_original_port_still_accepting"),
        Err(_) => return Err("restart_original_port_refusal_not_observed_before_deadline"),
    }
    let prior_sources = diagnostics
        .iter()
        .map(|source| {
            let active = match source.physical_owner {
                Some(crate::application::native_dispatch::OwnerCloseKind::WindowsJob {
                    active_processes,
                }) => Some(active_processes),
                _ => None,
            };
            serde_json::json!({
                "physicalPresent":source.physical_present,"activeProcesses":active,
                "metadataConfirmed":source.metadata_close.as_ref().is_some_and(Result::is_ok),
                "taskSlots":source.task_slots,"jobSlots":source.job_slots,
            })
        })
        .collect::<Vec<_>>();
    // Retain actual prior cleanup even if the next startup or read fails.
    steps.push(serde_json::json!({
        "step":"prior_owned_node_closed_before_restart",
        "priorCleanupSources":prior_sources,"priorCleanupJoined":shutdown.joined,
        "priorCleanupMetadataConfirmed":shutdown.metadata_confirmed,"priorCleanupPortClosed":true,
        "priorListenAddress":old_address.to_string(),
        "portRefusalObservedAfterMillis":port_check_started.elapsed().as_millis()
    }));
    let listen = old_address;
    let origin = format!("https://localhost:{}", listen.port());
    let mut config = f.area.config(listen);
    config.canonical_origin = origin.clone();
    f.owner = Some(NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        config,
        Arc::new(FakeProtector::default()),
    ));
    f.node = Some(
        f.owner
            .as_ref()
            .ok_or("restart_new_node_owner_missing")?
            .ready()
            .await
            .map_err(|_| "restart_same_owned_node_start_failed")?,
    );
    let node = f.node.as_ref().ok_or("restart_new_node_missing")?;
    if node.node_id() != node_id {
        return Err("restart_changed_original_node_identity");
    }
    let services = node
        .session_services
        .as_ref()
        .ok_or("restart_new_services_missing")?;
    if !rt_source_activity_clocks(&services.native)
        .map_err(|_| "restart_clock_observer_failed")?
        .is_empty()
        || !l6_automatic_waits(&services.native)
            .map_err(|_| "restart_wait_observer_failed")?
            .is_empty()
    {
        return Err("restart_reused_old_monotonic_activity_or_original_ticket");
    }
    let (socket, response) = websocket_exchange(
        &f.area,
        listen,
        &format!("localhost:{}", listen.port()),
        &[
            ("Origin", &origin),
            ("Sec-WebSocket-Protocol", CONTROL_PROTOCOL),
        ],
    )
    .await
    .map_err(|_| "restart_authenticated_socket_unavailable")?;
    if response.status() != 101 {
        return Err("restart_wss_upgrade_failed");
    }
    let socket = capacity_l6_auth(socket, &key, &client, node_public_key(node), &origin).await?;
    f.peers.push(R3WssPeer {
        client_id: client,
        key,
        socket,
    });
    if capacity_l6_capability_operation(f, operation).await? != fixed
        || capacity_l6_inspect(f).await?["occupied"] != 0
    {
        return Err("restart_changed_finished_original_or_unconfirmed_source_close");
    }
    let started = std::time::Instant::now();
    let view = capacity_l6_view_records(f, &native, "reacquire").await?;
    let current = capacity_l6_clock_root(f, native.clone()).await?;
    if !view.actual_complete
        || current.reservation == reservation
        || current.instance == instance
        || current.epoch == epoch
        || capacity_l6_capability_operation(f, operation).await? != fixed
        || capacity_l6_inspect(f).await?["occupied"] != 1
    {
        return Err("restart_reloaded_root_adopted_or_released_original_owner");
    }
    let (received, candidate) = capacity_l6_clock_sample(f, &current)?;
    if received < started || candidate.is_some_and(|since| since < started) {
        return Err("restart_reused_original_monotonic_eligibility");
    }
    steps.push(serde_json::json!({
        "step":"same_owned_node_restart_original_operation_isolated",
        "nodeId":node_id,"sourceId":f.selection.as_ref().ok_or("restart_source_missing")?.source.encode(),
        "operationId":operation,"native":native,
        "oldReservation":reservation,"newReservation":current.reservation,
        "oldInstance":instance,"newInstance":current.instance,
        "oldRuntimeEpoch":epoch,"newRuntimeEpoch":current.epoch,
        "originalOperationAndMembersUnchanged":true,"oldJobActiveProcesses":0,
        "oldRegisteredTaskSlots":diagnostics.iter().map(|source|source.task_slots).sum::<usize>(),
        "oldRegisteredJobSlots":diagnostics.iter().map(|source|source.job_slots).sum::<usize>(),
        "oldPortClosed":true,
        "oldOwnerJoined":shutdown.joined,"oldNecessaryMetadataConfirmed":shutdown.metadata_confirmed,
        "newReceivedClockObserved":true,"newOccupied":1,
        "priorListenAddress":old_address.to_string(),"newListenAddress":listen.to_string(),
        "executionUrlUnchanged":true
    }));
    capacity_l6_mark(
        steps,
        "cp032",
        "same_original_repair_once_and_restart_owner_isolation",
    )
}

#[test]
fn capacity_l6_capability_parser_keeps_missing_distinct_from_empty_and_active() {
    let parse = |extra: &str| {
        capacity_l6_capability_fact(format!(
        r#"{{"method":"session/update","params":{{"sessionId":"owned","update":{{"sessionUpdate":"state_update","state":"idle"{extra}}}}}}}"#
    ).as_bytes()).unwrap()
    };
    let missing = parse("");
    assert!(missing.background_missing);
    assert_eq!(missing.background, None);
    let empty = parse(r#","background":[]"#);
    assert!(!empty.background_missing);
    assert_eq!(empty.background, Some(false));
    let active = parse(r#","background":[{"type":"terminal","terminalId":"actual"}]"#);
    assert_eq!(active.background, Some(true));
    let malformed = parse(r#","background":{}"#);
    assert!(!malformed.background_missing);
    assert_eq!(malformed.background, None);
    assert_eq!(parse(r#","background":[{}]"#).background, None);
    assert_eq!(
        parse(r#","background":[{"type":"terminal"}]"#).background,
        None
    );
    assert!(missing.current_receipt.is_none());
}

#[test]
fn capacity_l6_capability_parser_binds_session_and_same_tool_status_presence() {
    let parse = |extra: &str| {
        capacity_l6_capability_fact(format!(
        r#"{{"method":"session/update","params":{{"sessionId":"owned","update":{{"sessionUpdate":"tool_call_update","toolCallId":"tool"{extra}}}}}}}"#
    ).as_bytes()).unwrap()
    };
    let missing = parse("");
    assert_eq!(missing.session, "owned");
    assert_eq!(missing.tool.as_deref(), Some("tool"));
    assert!(missing.tool_status_missing);
    assert_eq!(missing.tool_active, None);
    let pending = parse(r#","status":null"#);
    assert!(!pending.tool_status_missing);
    assert_eq!(pending.tool_active, Some(true));
    assert_eq!(parse(r#","status":"completed""#).tool_active, Some(false));
    assert_ne!(missing.digest, pending.digest);
    assert!(capacity_l6_capability_fact(br#"{"method":"session/new","result":{}}"#).is_none());
}

#[test]
fn capacity_l6_capability_same_receipt_rejects_later_work_repair() {
    use crate::domain::session_capacity::Coverage;
    let mut missing = capacity_l6_capability_fact(
        br#"{"method":"session/update","params":{"sessionId":"owned","update":{"sessionUpdate":"state_update","state":"idle"}}}"#,
    )
    .unwrap();
    missing.current_receipt = Some((4, 9));
    missing.post = Some(CapacityL6CapabilityPost {
        session: "owned".into(),
        foreground: Coverage::Idle,
        background: Coverage::Idle,
        active_tools: Vec::new(),
    });
    let mut restarted = missing.clone();
    restarted.background_missing = false;
    restarted.background = Some(true);
    restarted.current_receipt = Some((5, 10));
    restarted.post.as_mut().unwrap().background = Coverage::Active;
    // The latest owner is active again, exactly as the phase's separate current
    // check permits. It cannot prove that the earlier missing receipt kept it.
    assert_eq!(
        restarted.post.as_ref().unwrap().background,
        Coverage::Active
    );
    assert!(
        capacity_l6_capability_matching(
            vec![(7, missing), (8, restarted)],
            Some(6),
            CapacityL6CapabilityExpected::Background,
            |fact| fact.background_missing,
        )
        .is_none(),
        "L6-CAP001: later work must not repair the earlier missing receipt proof"
    );
}

#[test]
fn capacity_l6_capability_same_receipt_requires_original_tool_and_post() {
    use crate::domain::session_capacity::Coverage;
    let mut missing = capacity_l6_capability_fact(
        br#"{"method":"session/update","params":{"sessionId":"owned","update":{"sessionUpdate":"tool_call_update","toolCallId":"original"}}}"#,
    )
    .unwrap();
    missing.current_receipt = Some((4, 9));
    missing.post = Some(CapacityL6CapabilityPost {
        session: "owned".into(),
        foreground: Coverage::Idle,
        background: Coverage::Idle,
        active_tools: vec!["different".into()],
    });
    let matches = |fact: CapacityL6CapabilityFact| {
        capacity_l6_capability_matching(
            vec![(7, fact)],
            Some(6),
            CapacityL6CapabilityExpected::Tool,
            |fact| fact.tool_status_missing,
        )
    };
    assert!(matches(missing.clone()).is_none());
    missing
        .post
        .as_mut()
        .unwrap()
        .active_tools
        .push("original".into());
    assert!(matches(missing.clone()).is_some());
    missing.post.as_mut().unwrap().session = "other".into();
    assert!(matches(missing.clone()).is_none());
    missing.post = None;
    assert!(matches(missing.clone()).is_none());
    missing.current_receipt = None;
    assert!(matches(missing).is_none());
}

#[test]
fn capacity_l6_capability_same_receipt_positive_is_not_replaced_by_later_state() {
    use crate::domain::session_capacity::Coverage;
    let mut missing = capacity_l6_capability_fact(
        br#"{"method":"session/update","params":{"sessionId":"owned","update":{"sessionUpdate":"state_update","state":"idle"}}}"#,
    )
    .unwrap();
    missing.current_receipt = Some((4, 9));
    missing.post = Some(CapacityL6CapabilityPost {
        session: "owned".into(),
        foreground: Coverage::Idle,
        background: Coverage::Active,
        active_tools: Vec::new(),
    });
    let mut ended = missing.clone();
    ended.background_missing = false;
    ended.background = Some(false);
    ended.current_receipt = Some((5, 10));
    ended.post.as_mut().unwrap().background = Coverage::Idle;
    let observed = capacity_l6_capability_matching(
        vec![(7, missing), (8, ended)],
        Some(6),
        CapacityL6CapabilityExpected::Background,
        |fact| fact.background_missing,
    )
    .unwrap();
    assert_eq!(observed.0, 7);
    assert_eq!(observed.1.current_receipt, Some((4, 9)));
}

#[test]
fn capacity_l6_capability_same_receipt_tool_end_can_precede_foreground_idle() {
    use crate::domain::session_capacity::Coverage;
    let mut ended = capacity_l6_capability_fact(
        br#"{"method":"session/update","params":{"sessionId":"owned","update":{"sessionUpdate":"tool_call_update","toolCallId":"original","status":"completed"}}}"#,
    )
    .unwrap();
    ended.current_receipt = Some((4, 9));
    ended.post = Some(CapacityL6CapabilityPost {
        session: "owned".into(),
        foreground: Coverage::Active,
        background: Coverage::Unknown,
        active_tools: Vec::new(),
    });
    assert!(capacity_l6_capability_post_matches(
        &ended,
        CapacityL6CapabilityExpected::ToolEnded
    ));
    assert!(!capacity_l6_capability_post_matches(
        &ended,
        CapacityL6CapabilityExpected::Idle
    ));
    ended
        .post
        .as_mut()
        .unwrap()
        .active_tools
        .push("original".into());
    assert!(!capacity_l6_capability_post_matches(
        &ended,
        CapacityL6CapabilityExpected::ToolEnded
    ));
}
