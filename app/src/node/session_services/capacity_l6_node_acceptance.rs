// Opt-in real-Harness entry points. They use the normal Node/Catalog/WSS path.
// A partial or unavailable branch returns an error after writing "blocked";
// libtest success is never used to disguise missing L6 facts.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapacityL6Config {
    format: u32,
    run_root: std::path::PathBuf,
    launch_profile: std::path::PathBuf,
    dedicated_home: std::path::PathBuf,
    dedicated_codex_home: std::path::PathBuf,
    #[serde(default)]
    allow_model_requests: bool,
    default_limit: u32,
    idle_seconds: u64,
    scenario_files: std::collections::BTreeMap<String, std::path::PathBuf>,
    web_control: Option<CapacityL6Web>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapacityL6Web {
    enabled: bool,
    timeout_seconds: u64,
}

#[derive(Default, serde::Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct CapacityL6Scenario {
    format: Option<u32>,
    // These are explicit normal management commands, never synthetic ACP frames.
    // Prompt/input commands require the separate model-request opt-in.
    management_steps: Vec<CapacityL6Command>,
    real_child_ids: Vec<String>,
    expected_child_text: Vec<String>,
    #[serde(default)]
    expected_replay_text: String,
    #[serde(default)]
    protected_root_steps: Vec<CapacityL6Command>,
    phases: std::collections::BTreeMap<String, Vec<CapacityL6Command>>,
    child_failure_steps: Vec<CapacityL6Command>,
    child_recovery_steps: Vec<CapacityL6Command>,
    candidate_release_steps: Vec<CapacityL6Command>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapacityL6Command {
    action: String,
    payload: serde_json::Value,
}

type CapacityL6Result<T> = Result<T, &'static str>;

fn capacity_l6_sha(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    hex::encode(sha2::Sha256::digest(bytes))
}

fn capacity_l6_write_new(
    path: &std::path::Path,
    value: &serde_json::Value,
) -> CapacityL6Result<String> {
    use std::io::Write as _;
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|_| "execution_evidence_encoding_failed")?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "execution_evidence_unique_file_required")?;
    file.write_all(&bytes)
        .map_err(|_| "execution_evidence_write_failed")?;
    Ok(capacity_l6_sha(&bytes))
}

fn capacity_l6_fingerprint() -> CapacityL6Result<(String, serde_json::Value)> {
    let read = |key: &str| std::env::var(key).map_err(|_| "runner_execution_binding_missing");
    let digest = |key: &str| -> CapacityL6Result<String> {
        let value = read(key)?;
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("runner_execution_digest_invalid");
        }
        Ok(value)
    };
    let run_id = read("ACP_CAPACITY_L6_RUN_ID")?;
    if run_id.len() < 16
        || run_id.len() > 128
        || !run_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("runner_execution_run_id_invalid");
    }
    let executable = std::env::current_exe().map_err(|_| "execution_executable_unavailable")?;
    let exe_sha = digest("ACP_CAPACITY_L6_EXE_SHA256")?;
    if capacity_l6_sha(&std::fs::read(executable).map_err(|_| "execution_executable_unreadable")?)
        != exe_sha
    {
        return Err("runner_execution_executable_changed");
    }
    let config = std::env::var_os("ACP_CAPACITY_L6_CONFIG").ok_or("explicit_config_required")?;
    let config_sha = digest("ACP_CAPACITY_L6_CONFIG_SHA256")?;
    if capacity_l6_sha(&std::fs::read(config).map_err(|_| "execution_config_unreadable")?)
        != config_sha
    {
        return Err("runner_execution_config_changed");
    }
    let source = read("ACP_CAPACITY_L6_SOURCE_MANIFEST")?;
    let source_sha = digest("ACP_CAPACITY_L6_SOURCE_SHA256")?;
    let source = std::fs::read(source).map_err(|_| "execution_source_manifest_unreadable")?;
    if capacity_l6_sha(&source) != source_sha {
        return Err("runner_execution_source_manifest_changed");
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&source).map_err(|_| "execution_source_manifest_invalid")?;
    if manifest["testExecutableSha256"] != exe_sha
        || manifest["files"].as_array().is_none_or(Vec::is_empty)
    {
        return Err("execution_source_manifest_missing_executable_or_sources");
    }
    Ok((
        run_id,
        serde_json::json!({
            "exeSha256":exe_sha,"sourceManifestSha256":source_sha,
            "configSha256":config_sha,
            "branchMatrixSha256":"58fc0ded8e36e33a44e32a37afe8bdb24aa5f2aed601fd2a0094dd2ab878ba02"
        }),
    ))
}

async fn capacity_l6_web_collect_phase(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    phase: &str,
    expected: serde_json::Value,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    let Some(control) = c
        .web_control
        .as_ref()
        .filter(|web| web.enabled && web.timeout_seconds > 0)
    else {
        steps.push(
            serde_json::json!({"step":"same_node_browser_not_configured",
            "phase":phase,"status":"blocked","reason":"explicit_browser_control_required"}),
        );
        return Ok(());
    };
    let root = f
        .area
        .directory
        .parent()
        .ok_or("web_case_root_missing")?
        .to_owned();
    let summary = capacity_l6_web_phase(
        f,
        &root,
        phase,
        expected,
        Duration::from_secs(control.timeout_seconds),
    )
    .await?;
    if summary["status"] != "pending_independent_review"
        || summary["acceptancePassed"] != false
        || summary["browserIdentityRevoked"] != true
    {
        return Err("web_collect_did_not_preserve_pending_review_and_revocation");
    }
    let relative = format!("web-{phase}");
    let summary_sha = capacity_l6_write_new(&root.join(&relative).join("summary.json"), &summary)?;
    let case = root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("web_case_name_missing")?;
    let reference = format!("{case}/{relative}");
    let expected = &summary["expected"];
    steps.push(serde_json::json!({
        "step":"same_node_real_harness_browser_evidence_collected",
        "status":"pending_independent_review",
        "webPhase":{
            "phaseId":summary["phaseId"],"nodeId":summary["nodeId"],"sourceId":summary["sourceId"],
            "nativeIds":expected["nativeIds"],"operationIds":expected["operationIds"],"url":summary["url"],
            "summaryFile":format!("{reference}/summary.json"),"summarySha256":summary_sha,
            "captureFile":format!("{reference}/capture.json"),"captureSha256":summary["captureSha256"],
            "actionsFile":format!("{reference}/actions.json"),"actionsSha256":summary["actionsSha256"],
            "captureSourceFile":format!("{reference}/capture-source.txt"),
            "captureSourceSha256":summary["captureSourceSha256"]
        }
    }));
    Ok(())
}

fn capacity_l6_utf16(value: &serde_json::Value) -> CapacityL6Result<String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.as_str().ok_or("profile_encoded_text_missing")?)
        .map_err(|_| "profile_encoded_text_invalid")?;
    if bytes.len() % 2 != 0 {
        return Err("profile_utf16_invalid");
    }
    String::from_utf16(
        &bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>(),
    )
    .map_err(|_| "profile_utf16_invalid")
}

fn capacity_l6_path_key(path: &std::path::Path) -> CapacityL6Result<String> {
    if !path.is_absolute() {
        return Err("absolute_identity_directory_required");
    }
    // Default locations are input path facts. Do not inspect their contents.
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    return Err("invalid_identity_directory");
                }
            }
            part => normalized.push(part.as_os_str()),
        }
    }
    let text = normalized.to_string_lossy().replace('/', "\\");
    let text = text.strip_prefix(r"\\?\UNC\").map_or_else(
        || text.trim_start_matches(r"\\?\").to_owned(),
        |unc| format!(r"\\{unc}"),
    );
    Ok(text.trim_end_matches('\\').to_ascii_lowercase())
}

fn capacity_l6_within(child: &str, parent: &str) -> bool {
    child == parent
        || child
            .strip_prefix(parent)
            .is_some_and(|tail| tail.starts_with('\\'))
}

fn capacity_l6_checked_directory(path: &std::path::Path) -> CapacityL6Result<std::path::PathBuf> {
    if !path.is_absolute() {
        return Err("absolute_identity_directory_required");
    }
    for ancestor in path.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)
            .map_err(|_| "dedicated_identity_directory_missing")?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt as _;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("identity_directory_reparse_point_forbidden");
            }
        }
        if metadata.file_type().is_symlink() {
            return Err("identity_directory_reparse_point_forbidden");
        }
    }
    let actual = path
        .canonicalize()
        .map_err(|_| "dedicated_identity_directory_missing")?;
    if !actual.is_dir() {
        return Err("dedicated_identity_directory_missing");
    }
    Ok(actual)
}

fn capacity_l6_identity_guard(
    c: &CapacityL6Config,
    defaults_json: &str,
) -> CapacityL6Result<std::path::PathBuf> {
    let defaults: Vec<String> =
        serde_json::from_str(defaults_json).map_err(|_| "runner_default_identity_facts_invalid")?;
    if defaults.is_empty() || defaults.iter().any(String::is_empty) {
        return Err("runner_default_identity_facts_required");
    }
    let defaults = defaults
        .iter()
        .map(|value| capacity_l6_path_key(std::path::Path::new(value)))
        .collect::<CapacityL6Result<Vec<_>>>()?;
    let run_parent =
        capacity_l6_checked_directory(c.run_root.parent().ok_or("run_parent_missing")?)?;
    let run =
        capacity_l6_path_key(&run_parent.join(c.run_root.file_name().ok_or("run_root_missing")?))?;
    let home = capacity_l6_checked_directory(&c.dedicated_home)?;
    for directory in [&c.dedicated_home, &c.dedicated_codex_home] {
        let actual = capacity_l6_path_key(&capacity_l6_checked_directory(directory)?)?;
        if defaults.contains(&actual) {
            return Err("default_identity_directory_forbidden");
        }
        if capacity_l6_within(&actual, &run) || capacity_l6_within(&run, &actual) {
            return Err("identity_and_disposable_run_root_overlap");
        }
    }
    Ok(home)
}

fn capacity_l6_strict_child(
    directory: &std::path::Path,
    home: &std::path::Path,
) -> CapacityL6Result<()> {
    let actual = capacity_l6_path_key(&capacity_l6_checked_directory(directory)?)?;
    let parent = capacity_l6_path_key(home)?;
    if actual == parent || !capacity_l6_within(&actual, &parent) {
        return Err("profile_application_directory_must_be_strict_home_child");
    }
    Ok(())
}

fn capacity_l6_profile_env_guard(
    environment: &[serde_json::Value],
    c: &CapacityL6Config,
    home: &std::path::Path,
) -> CapacityL6Result<()> {
    let mut seen = std::collections::HashSet::new();
    for entry in environment {
        let name = capacity_l6_utf16(&entry["name_utf16le_base64"])?.to_ascii_uppercase();
        let value = capacity_l6_utf16(&entry["value_utf16le_base64"])?;
        if !seen.insert(name.clone()) {
            return Err("duplicate_profile_environment");
        }
        let target = std::path::Path::new(&value);
        match name.as_str() {
            "HOME" | "USERPROFILE" | "CODEX_HOME" => {
                let expected = if name == "CODEX_HOME" {
                    &c.dedicated_codex_home
                } else {
                    home
                };
                if capacity_l6_path_key(&capacity_l6_checked_directory(target)?)?
                    != capacity_l6_path_key(&capacity_l6_checked_directory(expected)?)?
                {
                    return Err("profile_identity_directory_does_not_match_config");
                }
            }
            "APPDATA" | "LOCALAPPDATA" | "TEMP" | "TMP" => {
                capacity_l6_strict_child(target, home)?;
            }
            "SYSTEMROOT" | "WINDIR" | "COMSPEC" | "PATHEXT" => {
                if !std::env::var(&name).is_ok_and(|original| original.eq_ignore_ascii_case(&value))
                {
                    return Err("profile_cannot_replace_operating_system_environment");
                }
            }
            "PATH" | "CODEX_PATH" => {}
            // 仅放行固定只读/工作区写模式与禁浏览器标志，避免沿用自动审查预设或写入未审计变量。
            "INITIAL_AGENT_MODE" => {
                if !matches!(value.as_str(), "read-only" | "workspace-write") {
                    return Err("profile_initial_agent_mode_invalid");
                }
            }
            "NO_BROWSER" => {
                if value != "1" {
                    return Err("profile_no_browser_invalid");
                }
            }
            _ => return Err("profile_environment_outside_explicit_safe_subset"),
        }
    }
    if [
        "HOME",
        "USERPROFILE",
        "CODEX_HOME",
        "APPDATA",
        "LOCALAPPDATA",
        "TEMP",
        "TMP",
    ]
    .iter()
    .any(|name| !seen.contains(*name))
    {
        return Err("profile_requires_all_explicit_dedicated_directories");
    }
    Ok(())
}

fn capacity_l6_profile_guard(c: &CapacityL6Config) -> CapacityL6Result<()> {
    let original_defaults = std::env::var("ACP_CAPACITY_L6_DEFAULT_IDENTITY_DIRECTORIES")
        .map_err(|_| "runner_default_identity_facts_required")?;
    let home = capacity_l6_identity_guard(c, &original_defaults)?;
    let value: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&c.launch_profile).map_err(|_| "profile_not_readable")?,
    )
    .map_err(|_| "profile_invalid_json")?;
    if value["format"] != 1 {
        return Err("profile_format_one_required");
    }
    let executable = capacity_l6_utf16(&value["launch"]["executable_utf16le_base64"])?;
    let executable = std::path::Path::new(&executable);
    if !executable.is_absolute()
        || !executable.is_file()
        || executable
            .file_name()
            .is_none_or(|v| !v.to_string_lossy().eq_ignore_ascii_case("node.exe"))
    {
        return Err("explicit_installed_node_executable_required");
    }
    let args = value["launch"]["arguments_utf16le_base64"]
        .as_array()
        .ok_or("profile_arguments_missing")?;
    if args.len() != 1 {
        return Err("only_installed_adapter_entrypoint_argument_supported");
    }
    let argument = capacity_l6_utf16(&args[0])?;
    let entrypoint = std::path::Path::new(&argument);
    if !entrypoint.is_absolute()
        || !entrypoint.is_file()
        || entrypoint
            .extension()
            .is_none_or(|v| !matches!(v.to_str(), Some("js" | "cjs" | "mjs")))
    {
        return Err("explicit_installed_adapter_entrypoint_required");
    }
    let environment = value["launch"]["environment"]
        .as_array()
        .ok_or("profile_environment_missing")?;
    capacity_l6_profile_env_guard(environment, c, &home)?;
    let recipes = value["recipes"]
        .as_array()
        .ok_or("profile_recipes_missing")?;
    if recipes.is_empty() {
        return Err("profile_recipes_missing");
    }
    for recipe in recipes {
        if !recipe["mcp_servers"].as_array().is_some_and(Vec::is_empty)
            || !recipe["additional_directories"]
                .as_array()
                .is_some_and(Vec::is_empty)
            || !recipe["extra"]
                .as_object()
                .is_some_and(serde_json::Map::is_empty)
        {
            return Err("l6_entry_requires_empty_mcp_and_additional_directories");
        }
    }
    Ok(())
}

fn capacity_l6_required(case: &str) -> &'static [&'static str] {
    match case {
        "cp011" => &[
            "real_fifty_and_three_ingress_last_slot",
            "long_silent_task_protected_1800",
            "only_child_work_protected_1800",
            "approval_and_cancel_work_protection_1800",
            "unprotected_restarts_actual_1800",
            "cache_and_replay_distinct_actual_clocks",
            "pressure_before_send_received_work_recheck",
            "failed_unknown_close_keeps_slot",
            "no_candidate_explicit_refusal",
            "shared_tree_untouched_and_restore_input",
            "same_node_real_harness_web",
            "complete_owned_job_cleanup",
        ],
        "cp023" => &[
            "received_trusted_real_children_unopened_root",
            "direct_children_exact_distinct_bodies",
            "one_original_tree_owner_and_slot",
            "real_fifty_no_candidate_then_confirmed_close",
            "child_failure_cancel_preserves_original_owner",
            "deleted_root_or_invalid_relationship_refusal",
            "tree_final_resource_join_and_shared_tree_isolation",
            "same_node_real_harness_web",
            "complete_owned_job_cleanup",
        ],
        "cp026" => &[
            "cache_only_actual_1800_no_keepalive",
            "actual_replay_received_near_timeout",
            "after_last_receive_1799_then_1800_and_scan",
            "fifty_recent_replay_only_candidate",
            "read_join_close_confirmed_necessary_finish_then_admit",
            "shared_tree_isolation_and_history_limits",
            "same_node_real_harness_web",
            "complete_owned_job_cleanup",
        ],
        _ => &[
            "real_known_work_vs_unprovided_coverage",
            "missing_fields_do_not_clear_known_work",
            "foreground_or_connection_invalid_protects",
            "fifty_candidate_and_refusal",
            "received_before_send_recheck",
            "real_written_closed_and_system_original_nonempty_set",
            "node_zero_partial_unknown_ticket_boundaries",
            "closed_necessary_finish_fault_keeps_fifty",
            "same_original_repair_once_and_restart_owner_isolation",
            "same_node_real_harness_web",
            "complete_owned_job_cleanup",
        ],
    }
}

fn capacity_l6_unimplemented(case: &str) -> &'static [&'static str] {
    match case {
        // These routes now have concrete helpers. Their actual facts are still
        // checked independently in requiredBranches before executionComplete.
        "cp011" | "cp023" | "cp026" | "cp032" => &[],
        _ => &["unknown_real_harness_entry_route"],
    }
}

fn capacity_l6_mark(
    steps: &mut Vec<serde_json::Value>,
    case: &str,
    branch: &str,
) -> CapacityL6Result<()> {
    if !capacity_l6_required(case).contains(&branch) {
        return Err("unknown_required_branch");
    }
    if !steps
        .iter()
        .any(|step| step["branch"] == branch && step["status"] == "passed")
    {
        steps.push(serde_json::json!({"branch":branch,"status":"passed"}));
    }
    Ok(())
}

fn capacity_l6_config() -> CapacityL6Result<CapacityL6Config> {
    let file = std::env::var_os("ACP_CAPACITY_L6_CONFIG").ok_or("explicit_config_required")?;
    let bytes = std::fs::read(file).map_err(|_| "config_not_readable")?;
    let c: CapacityL6Config = serde_json::from_slice(&bytes).map_err(|_| "invalid_config")?;
    if c.format != 1 || c.default_limit != 50 || c.idle_seconds != 1800 {
        return Err("l6_requires_default_fifty_and_real_1800_seconds");
    }
    for path in [
        &c.run_root,
        &c.launch_profile,
        &c.dedicated_home,
        &c.dedicated_codex_home,
    ] {
        if !path.is_absolute() {
            return Err("absolute_explicit_paths_required");
        }
    }
    let allowed = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../capacity-request-tester")
        .canonicalize()
        .or_else(|_| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../.tmp/capacity-request-tester")
                .canonicalize()
        })
        .map_err(|_| "owned_tester_root_missing")?;
    let run_parent = c
        .run_root
        .parent()
        .ok_or("run_parent_missing")?
        .canonicalize()
        .map_err(|_| "run_parent_missing")?;
    if !run_parent.starts_with(&allowed) {
        return Err("run_root_outside_owned_tester_directory");
    }
    capacity_l6_profile_guard(&c)?;
    // The runner also checks the decoded launch environment. This in-process
    // check ensures even a direct ignored invocation cannot select default HOME.
    for name in ["HOME", "USERPROFILE"] {
        if std::env::var_os(name)
            .is_none_or(|value| std::path::Path::new(&value) != c.dedicated_home)
        {
            return Err("explicit_dedicated_home_environment_required");
        }
    }
    if std::env::var_os("CODEX_HOME")
        .is_none_or(|value| std::path::Path::new(&value) != c.dedicated_codex_home)
    {
        return Err("explicit_dedicated_codex_home_environment_required");
    }
    Ok(c)
}

async fn capacity_l6_call(
    f: &mut R3WssCase,
    action: &str,
    payload: serde_json::Value,
) -> CapacityL6Result<serde_json::Value> {
    capacity_l6_call_on(f, 0, action, payload).await
}

async fn capacity_l6_call_on(
    f: &mut R3WssCase,
    peer_index: usize,
    action: &str,
    payload: serde_json::Value,
) -> CapacityL6Result<serde_json::Value> {
    let mut frames = Vec::new();
    capacity_l6_call_collect_on(f, peer_index, action, payload, &mut frames).await
}

fn capacity_l6_next_command_at(
    previous: Option<tokio::time::Instant>,
    now: tokio::time::Instant,
) -> tokio::time::Instant {
    previous.map_or(now, |previous| {
        (previous + Duration::from_millis(520)).max(now)
    })
}

// This is client pacing, not a product rate-limit override. It also covers
// direct Create sends used while another original command remains outstanding.
async fn capacity_l6_pace_command(f: &R3WssCase) -> CapacityL6Result<()> {
    capacity_l6_pace_command_on(f, 0).await
}

async fn capacity_l6_pace_command_on(
    f: &R3WssCase,
    peer_index: usize,
) -> CapacityL6Result<()> {
    type ClientTimes = std::collections::HashMap<String, tokio::time::Instant>;
    static CLIENT_TIMES: std::sync::OnceLock<std::sync::Mutex<ClientTimes>> =
        std::sync::OnceLock::new();
    let client = &f
        .peers
        .get(peer_index)
        .ok_or("command_actor_missing")?
        .client_id;
    let scheduled = {
        let mut times = CLIENT_TIMES
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| "command_pacing_state_unavailable")?;
        let now = tokio::time::Instant::now();
        times.retain(|_, previous| {
            now.saturating_duration_since(*previous) < Duration::from_mins(2)
        });
        let scheduled = capacity_l6_next_command_at(times.get(client).copied(), now);
        times.insert(client.clone(), scheduled);
        scheduled
    };
    tokio::time::sleep_until(scheduled).await;
    Ok(())
}

async fn capacity_l6_call_collect(
    f: &mut R3WssCase,
    action: &str,
    payload: serde_json::Value,
    frames: &mut Vec<(RecordKind, Vec<u8>)>,
) -> CapacityL6Result<serde_json::Value> {
    capacity_l6_call_collect_on(f, 0, action, payload, frames).await
}

async fn capacity_l6_call_collect_on(
    f: &mut R3WssCase,
    peer_index: usize,
    action: &str,
    payload: serde_json::Value,
    frames: &mut Vec<(RecordKind, Vec<u8>)>,
) -> CapacityL6Result<serde_json::Value> {
    capacity_l6_pace_command_on(f, peer_index).await?;
    let request = Id128::new(crate::random::bytes().map_err(|_| "system_random_unavailable")?);
    let peer = f
        .peers
        .get_mut(peer_index)
        .ok_or("management_peer_missing")?;
    send_json(
        &mut peer.socket,
        &serde_json::json!({
            "v":1,"type":"command","requestId":request,"action":action,"payload":payload
        }),
    )
    .await;
    timeout(Duration::from_secs(90), async {
        loop {
            let peer = f
                .peers
                .get_mut(peer_index)
                .ok_or("management_peer_missing")?;
            match peer.socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let response: serde_json::Value = serde_json::from_str(text.as_str())
                        .map_err(|_| "invalid_management_response")?;
                    if response["requestId"] == request.to_base64url() {
                        return Ok(response);
                    }
                    // Normal live status events may precede this command result.
                    if response.get("requestId").is_some() {
                        return Err("management_response_correlation_failed");
                    }
                }
                Some(Ok(Message::Binary(bytes))) => {
                    let (header, body) = TransferHeader::decode(&bytes)
                        .map_err(|_| "invalid_view_frame_during_management")?;
                    frames.push((
                        header
                            .record_kind()
                            .ok_or("unexpected_view_control_frame")?,
                        body.to_vec(),
                    ));
                    let credit = TransferHeader::credit(header.stream(), header.packet())
                        .encode(&[])
                        .map_err(|_| "view_credit_encoding_failed")?;
                    f.peers
                        .get_mut(peer_index)
                        .ok_or("management_peer_missing")?
                        .socket
                        .send(Message::Binary(credit.into()))
                        .await
                        .map_err(|_| "view_credit_failed")?;
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                Some(Ok(Message::Close(Some(close)))) if u16::from(close.code) == 4429 => {
                    return Err("management_command_rate_limit");
                }
                _ => return Err("management_socket_closed"),
            }
        }
    })
    .await
    .map_err(|_| "management_response_timeout")?
}

async fn capacity_l6_operation(
    f: &mut R3WssCase,
    operation: serde_json::Value,
) -> CapacityL6Result<serde_json::Value> {
    let registered = capacity_l6_call(
        f,
        "source.session.register",
        serde_json::json!({"selection":f.selection_json(),"operation":operation}),
    )
    .await?;
    if registered["ok"] != true {
        return Ok(registered);
    }
    let operation = registered["result"]["sourceOperationId"]
        .as_str()
        .ok_or("source_operation_not_registered")?;
    capacity_l6_call(
        f,
        "source.session.execute",
        serde_json::json!({"sourceOperationId":operation}),
    )
    .await
}

async fn capacity_l6_new(f: &mut R3WssCase) -> CapacityL6Result<String> {
    let result = capacity_l6_operation(f, serde_json::json!({"kind":"create"})).await?;
    if result["ok"] != true || result["result"]["outcome"] != "succeeded" {
        // Do not print the upstream message, raw response, auth path or body.
        return Err(if result["ok"] != true {
            match result["error"]["code"].as_str() {
                Some("session_capacity_full") => "real_new_capacity_full",
                Some("rate_limit") => "real_new_rate_limit",
                Some("node_stopping") => "real_new_node_stopping",
                Some("invalid_configuration") => "real_new_invalid_configuration",
                Some("invalid_target") => "real_new_invalid_target",
                Some("revoked") | Some("unauthenticated") => "real_new_revoked",
                _ => "real_new_not_confirmed_auth_or_capability_required",
            }
        } else {
            "real_new_not_confirmed_auth_or_capability_required"
        });
    }
    result["result"]["response"]["nativeSessionId"]
        .as_str()
        .map(str::to_owned)
        .ok_or("real_new_has_no_verified_native_identity")
}

async fn capacity_l6_inspect(f: &mut R3WssCase) -> CapacityL6Result<serde_json::Value> {
    let result = capacity_l6_call(f, "session.capacity.inspect", serde_json::json!({})).await?;
    if result["ok"] != true
        || result["result"]["limit"] != 50
        || result["result"]["automaticRecycling"] != true
    {
        return Err("actual_default_capacity_or_registered_scan_not_available");
    }
    Ok(result["result"].clone())
}

async fn capacity_l6_auth(
    mut socket: ManagementSocket,
    key: &SigningKey,
    client: &str,
    node: PublicKeySec1,
    origin: &str,
) -> CapacityL6Result<ManagementSocket> {
    send_json(
        &mut socket,
        &serde_json::json!({
            "v":1,"type":"hello","clientId":client,"clientPublicKey":client_public_key(key),
            "clientNonce":Nonce256::new(crate::random::bytes().map_err(|_| "system_random_unavailable")?)
        }),
    )
    .await;
    let challenge = next_json(&mut socket, Duration::from_secs(5)).await;
    let proof = verify_auth_host(
        challenge["hostProof"]
            .as_str()
            .ok_or("node_auth_challenge_missing")?,
        &node,
    )
    .map_err(|_| "node_auth_signature_failed")?;
    if proof.origin != origin || proof.client_id.map(Id128::to_base64url).as_deref() != Some(client)
    {
        return Err("node_auth_identity_mismatch");
    }
    let compact = sign(
        JwsType::AuthClient,
        &serde_json::to_vec(&proof).map_err(|_| "auth_serialization_failed")?,
        key,
    )
    .map_err(|_| "auth_signing_failed")?;
    send_json(
        &mut socket,
        &serde_json::json!({"v":1,"type":"proof","proof":compact}),
    )
    .await;
    let result = next_json(&mut socket, Duration::from_secs(5)).await;
    if result["type"] != "authenticated" || result["clientId"] != client {
        return Err("dedicated_actor_not_authenticated");
    }
    Ok(socket)
}

async fn capacity_l6_start(f: &mut R3WssCase, c: &CapacityL6Config) -> CapacityL6Result<String> {
    let listen = free_address();
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
            .ok_or("node_owner_missing")?
            .ready()
            .await
            .map_err(|_| "real_node_start_failed")?,
    );
    let node = f.node.as_ref().ok_or("node_missing")?;
    let launch = f
        .area
        .directory
        .canonicalize()
        .map_err(|_| "owned_workspace_missing")?
        .to_string_lossy()
        .into_owned();
    let workspace = ipc::workspace_register(node.ipc_address(), &launch)
        .await
        .map_err(|_| "actual_workspace_registration_failed")?;
    let key = loop {
        let bytes: [u8; 32] = crate::random::bytes().map_err(|_| "system_random_unavailable")?;
        if let Ok(key) = SigningKey::from_slice(&bytes) {
            break key;
        }
    };
    let client_id = pair_browser(node, &key, "Real Harness dedicated L6 controller").await;
    let observed = node
        .management
        .as_ref()
        .ok_or("management_missing")?
        .observe_next_document_socket_for_test()
        .map_err(|_| "management_observer_failed")?;
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
    .map_err(|_| "real_wss_connect_failed")?;
    if response.status() != 101 {
        return Err("real_wss_upgrade_failed");
    }
    let socket = capacity_l6_auth(socket, &key, &client_id, node_public_key(node), &origin).await?;
    let endpoint = observed
        .await
        .map_err(|_| "authenticated_endpoint_missing")?;
    let client = endpoint.client();
    let profile = crate::adapters::acp::node_configuration_profile::parse_profile(
        std::fs::read(&c.launch_profile).map_err(|_| "profile_not_readable")?,
    )
    .map_err(|_| "actual_profile_invalid")?;
    let draft = CatalogDraft::install_harness(
        "Real installed ACP dedicated L6".into(),
        SourceSelection::New {
            name: "Real Harness dedicated L6 source".into(),
        },
        profile,
    )
    .map_err(|_| "catalog_draft_invalid")?;
    let catalog = &node
        .session_services
        .as_ref()
        .ok_or("services_missing")?
        .catalog;
    let actor = client.context.actor();
    let registered = catalog
        .register(actor, draft, &r3_deadline())
        .await
        .map_err(|_| "catalog_registration_failed")?;
    let applied = catalog
        .apply(actor, registered, &r3_deadline())
        .await
        .map_err(|_| "catalog_apply_failed")?;
    let (source, harness, profile) = match applied.fact().result() {
        Some(CatalogResult::Harness {
            source,
            harness,
            profile,
        }) => (*source, *harness, *profile),
        _ => return Err("catalog_harness_fact_missing"),
    };
    f.selection = Some(SourceLaunchSelection {
        source,
        harness,
        profile,
        workspace: crate::domain::EntityId::decode(&workspace.workspace_id)
            .map_err(|_| "workspace_identity_invalid")?,
        launch_path: launch,
    });
    f.peers.push(R3WssPeer {
        client_id,
        key,
        socket,
    });
    // The post-reservation Create is held on the first authenticated socket.
    // Losing load requests need their own original command/response correlation.
    let key = loop {
        let bytes: [u8; 32] = crate::random::bytes().map_err(|_| "system_random_unavailable")?;
        if let Ok(key) = SigningKey::from_slice(&bytes) {
            break key;
        }
    };
    let client_id = pair_browser(node, &key, "Real Harness independent L6 controller").await;
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
    .map_err(|_| "independent_l6_wss_connect_failed")?;
    if response.status() != 101 {
        return Err("independent_l6_wss_upgrade_failed");
    }
    let socket = capacity_l6_auth(socket, &key, &client_id, node_public_key(node), &origin).await?;
    f.peers.push(R3WssPeer {
        client_id,
        key,
        socket,
    });
    Ok(origin)
}

struct CapacityL6View {
    mode: String,
    records: Vec<(RecordKind, Vec<u8>)>,
    history_incomplete: bool,
    actual_complete: bool,
}

async fn capacity_l6_view_records(
    f: &mut R3WssCase,
    native: &str,
    reason: &str,
) -> CapacityL6Result<CapacityL6View> {
    let opened=capacity_l6_call(f,"view.open_source",serde_json::json!({"selection":f.selection_json(),"nativeSessionId":native,"reason":reason})).await?;
    if opened["ok"] != true {
        return Err(match opened["error"]["code"].as_str() {
            Some("source_released") => "real_view_source_released",
            Some("read_failed") => "real_view_read_failed",
            Some("session_capacity_full") => "real_view_session_capacity_full",
            Some("session_tree_unverified") => "real_view_session_tree_unverified",
            Some("invalid_target") => "real_view_invalid_target",
            Some("invalid_state") => "real_view_invalid_state",
            Some("node_stopping") => "real_view_node_stopping",
            Some("invalid_source") => "real_view_invalid_source",
            Some("view_unavailable") => "real_view_unavailable",
            Some("rate_limit") => "real_view_rate_limit",
            Some("revoked") => "real_view_revoked",
            _ => "real_view_not_available",
        });
    }
    let mut stream = Id128::parse_base64url(
        opened["result"]["streamId"]
            .as_str()
            .ok_or("view_stream_missing")?,
    )
    .map_err(|_| "view_stream_invalid")?;
    let mut original_records =
        capacity_l6_drain_original_complete(&mut f.peers[0].socket, stream).await?;
    // The real management rebind waits for this original read's Complete before
    // publishing its replacement stream. SnapshotEnd alone is not completion.
    let rebound = capacity_l6_call_collect(
        f,
        "view.rebind",
        serde_json::json!({"streamId":stream.to_base64url()}),
        &mut original_records,
    )
    .await?;
    if rebound["ok"] != true {
        return Err("original_read_did_not_complete_and_rebind");
    }
    stream = Id128::parse_base64url(
        rebound["result"]["streamId"]
            .as_str()
            .ok_or("rebound_stream_missing")?,
    )
    .map_err(|_| "rebound_stream_invalid")?;
    let (records, again) = capacity_l6_drain_snapshot(&mut f.peers[0].socket, stream).await?;
    if again {
        return Err("completed_read_changed_again");
    }
    // Detach before command close so no unused transfer chunk holds the writer.
    r3_send_binary(
        &mut f.peers[0].socket,
        TransferHeader::detach(stream)
            .encode(&[])
            .map_err(|_| "detach_encoding_failed")?,
    )
    .await;
    let close = capacity_l6_call(
        f,
        "view.close",
        serde_json::json!({"streamId":stream.to_base64url()}),
    )
    .await?;
    if close["ok"] != true {
        return Err("reader_close_unconfirmed");
    }
    Ok(CapacityL6View {
        mode: rebound["result"]["status"]["mode"]
            .as_str()
            .ok_or("completed_view_mode_missing")?
            .to_owned(),
        records,
        history_incomplete: rebound["result"]["status"]["historyIncomplete"] == true,
        actual_complete: true,
    })
}

fn capacity_l6_expand(
    value: &serde_json::Value,
    native: &str,
    target: &str,
    selection: &serde_json::Value,
    last: &serde_json::Value,
) -> CapacityL6Result<serde_json::Value> {
    match value {
        serde_json::Value::String(text) if text == "$nativeId" => Ok(serde_json::json!(native)),
        serde_json::Value::String(text) if text == "$targetId" => Ok(serde_json::json!(target)),
        serde_json::Value::String(text) if text == "$selection" => Ok(selection.clone()),
        serde_json::Value::String(text) if text.starts_with("$last/") => last
            .pointer(&text[5..])
            .cloned()
            .ok_or("scenario_previous_result_reference_missing"),
        serde_json::Value::Array(values) => values
            .iter()
            .map(|v| capacity_l6_expand(v, native, target, selection, last))
            .collect::<CapacityL6Result<Vec<_>>>()
            .map(serde_json::Value::Array),
        serde_json::Value::Object(values) => values
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.clone(),
                    capacity_l6_expand(v, native, target, selection, last)?,
                ))
            })
            .collect::<CapacityL6Result<serde_json::Map<_, _>>>()
            .map(serde_json::Value::Object),
        _ => Ok(value.clone()),
    }
}

async fn capacity_l6_submit_text_on(
    f: &mut R3WssCase,
    peer_index: usize,
    target: &str,
    text: &str,
) -> CapacityL6Result<serde_json::Value> {
    if text.is_empty() {
        return Err("explicit_prompt_text_required");
    }
    let begin = capacity_l6_call_on(
        f,
        peer_index,
        "input.upload.begin",
        serde_json::json!({"targetId":target,"purpose":"submit"}),
    )
    .await?;
    let upload = Id128::parse_base64url(
        begin["result"]["uploadId"]
            .as_str()
            .ok_or("actual_upload_not_created")?,
    )
    .map_err(|_| "invalid_upload_id")?;
    let body = serde_json::to_vec(&serde_json::json!([{"type":"text","text":text}]))
        .map_err(|_| "prompt_encoding_failed")?;
    let mut wire = vec![0; 32];
    wire[..4].copy_from_slice(b"ACPI");
    wire[4] = 1;
    wire[5] = 1;
    wire[8..24].copy_from_slice(upload.as_bytes());
    wire.extend_from_slice(&body);
    f.peers
        .get_mut(peer_index)
        .ok_or("management_peer_missing")?
        .socket
        .send(Message::Binary(wire.into()))
        .await
        .map_err(|_| "actual_prompt_upload_failed")?;
    let ack = timeout(Duration::from_secs(10), async {
        loop {
            let peer = f
                .peers
                .get_mut(peer_index)
                .ok_or("management_peer_missing")?;
            match peer.socket.next().await {
                Some(Ok(Message::Binary(bytes))) => return Ok(bytes),
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                _ => return Err("actual_prompt_ack_failed"),
            }
        }
    })
    .await
    .map_err(|_| "actual_prompt_ack_timeout")??;
    if ack.len() != 32
        || &ack[..8] != b"ACPI\x01\x02\0\0"
        || &ack[8..24] != upload.as_bytes()
        || ack[24..] != (body.len() as u64).to_be_bytes()
    {
        return Err("actual_prompt_ack_identity_or_offset_mismatch");
    }
    let finished = capacity_l6_call_on(
        f,
        peer_index,
        "input.upload.finish",
        serde_json::json!({"uploadId":upload.to_base64url(),"totalBytes":body.len().to_string()}),
    )
    .await?;
    if finished["ok"] != true {
        return Err("actual_upload_finish_failed");
    }
    let ready = timeout(Duration::from_secs(30), async {
        loop {
            let status = capacity_l6_call_on(
                f,
                peer_index,
                "input.upload.status",
                serde_json::json!({"uploadId":upload.to_base64url()}),
            )
            .await?;
            match status["result"]["phase"].as_str() {
                Some("ready") => return Ok(status),
                Some("preparing") => tokio::time::sleep(Duration::from_millis(20)).await,
                _ => return Err("actual_upload_not_ready"),
            }
        }
    })
    .await
    .map_err(|_| "actual_upload_preparation_timeout")??;
    let body_id = ready["result"]["bodyId"]
        .as_str()
        .ok_or("actual_body_identity_missing")?;
    let registration = capacity_l6_call_on(
        f,
        peer_index,
        "input.register",
        serde_json::json!({"targetId":target,"operation":{"kind":"submit","bodyId":body_id}}),
    )
    .await?;
    let id = registration["result"]["registrationId"]
        .as_str()
        .ok_or("actual_input_registration_failed")?;
    let consumed = capacity_l6_call_on(
        f,
        peer_index,
        "input.consume",
        serde_json::json!({"targetId":target,"registrationId":id,"bodyId":body_id}),
    )
    .await?;
    if consumed["ok"] != true || consumed["result"]["request"]["outcome"]["kind"] != "queued" {
        return Err("actual_input_not_queued");
    }
    Ok(consumed)
}

/// 通过真实管理 RPC `approval.list` -> `approval.register` -> `approval.decide` 决策指定树内的待决审批。
async fn capacity_l6_decide_approval_on(
    f: &mut R3WssCase,
    peer_index: usize,
    native: &str,
    selection: &serde_json::Value,
    payload: &serde_json::Value,
) -> CapacityL6Result<serde_json::Value> {
    let choice_kind = payload
        .get("choiceKind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("allow_once");
    if !matches!(choice_kind, "allow_once" | "reject_once") {
        return Err("l6_approval_choice_kind_invalid");
    }
    let scope = payload
        .get("scope")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("tree");
    if !matches!(scope, "root" | "child" | "tree") {
        return Err("l6_approval_scope_invalid");
    }
    let match_text = payload.get("matchText").and_then(serde_json::Value::as_str);
    let timeout_secs = payload
        .get("timeoutSecs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(60)
        .clamp(1, 300);
    let source_id = selection["sourceId"]
        .as_str()
        .ok_or("approval_selection_source_missing")?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

    loop {
        let listed = capacity_l6_call_on(f, peer_index, "approval.list", serde_json::json!({})).await?;
        if listed["ok"] != true {
            return Err("l6_approval_list_failed");
        }
        let members: std::collections::HashSet<String> = capacity_l6_known_work(f, native)
            .unwrap_or_default()
            .into_iter()
            .map(|fact| fact.native)
            .collect();
        if let Some(items) = listed["result"]["items"].as_array() {
            for item in items {
                if item["source"]["sourceId"].as_str() != Some(source_id)
                    || item["status"]["validity"].as_str() != Some("current_pending")
                    || !item["status"]["decision"].is_null()
                {
                    continue;
                }
                let Some(raw_text) = item["content"]["rawJson"].as_str() else {
                    continue;
                };
                if let Some(needle) = match_text
                    && !needle.is_empty()
                    && !raw_text.contains(needle)
                {
                    continue;
                }
                let Ok(raw_value) = serde_json::from_str::<serde_json::Value>(raw_text) else {
                    continue;
                };
                let Some(session_id) = raw_value["params"]["sessionId"].as_str() else {
                    continue;
                };
                let scope_match = match scope {
                    "root" => session_id == native,
                    "child" => session_id != native && members.contains(session_id),
                    "tree" => members.contains(session_id),
                    _ => false,
                };
                if !scope_match {
                    continue;
                }
                let approval_id = item["approvalId"]
                    .as_str()
                    .ok_or("l6_approval_id_missing")?;
                let choice_id = item["choices"]
                    .as_array()
                    .and_then(|choices| {
                        choices.iter().find(|choice| {
                            choice["selectable"] == true
                                && choice["originalKind"].as_str() == Some(choice_kind)
                        })
                    })
                    .and_then(|choice| choice["choiceId"].as_str())
                    .ok_or("l6_approval_choice_not_available")?;
                let registered = capacity_l6_call_on(
                    f,
                    peer_index,
                    "approval.register",
                    serde_json::json!({"approvalId": approval_id, "choiceId": choice_id}),
                )
                .await?;
                if registered["ok"] != true {
                    return Err("l6_approval_register_failed");
                }
                let reg_id = registered["result"]["approvalRegistrationId"]
                    .as_str()
                    .ok_or("l6_approval_registration_id_missing")?;
                let decided = capacity_l6_call_on(
                    f,
                    peer_index,
                    "approval.decide",
                    serde_json::json!({"approvalRegistrationId": reg_id}),
                )
                .await?;
                if decided["ok"] != true
                    || !matches!(
                        decided["result"]["state"].as_str(),
                        Some("accepted" | "existing")
                    )
                {
                    return Err("l6_approval_decide_failed");
                }
                return Ok(decided);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("l6_expected_pending_approval_not_observed");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// 轮询进程内已知工作与候选时钟，直到目标主执行树完全恢复空闲及候选资格。
async fn capacity_l6_wait_idle(
    f: &R3WssCase,
    native: &str,
    payload: &serde_json::Value,
) -> CapacityL6Result<serde_json::Value> {
    use crate::application::native_dispatch::runtime_child_source_clock_probe::rt_source_candidate_clocks;
    use crate::domain::session_capacity::Coverage;
    let timeout_secs = payload
        .get("timeoutSecs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(60)
        .clamp(1, 300);
    let runtime = f
        .node
        .as_ref()
        .and_then(|node| node.session_services.as_ref())
        .ok_or("wait_idle_runtime_missing")?
        .native
        .clone();
    let source = f.selection.as_ref().ok_or("wait_idle_source_missing")?.source;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let all_idle = capacity_l6_known_work(f, native).is_ok_and(|facts| {
            facts.iter().all(|fact| {
                fact.foreground == Coverage::Idle
                    && fact.background != Coverage::Active
                    && fact.tools == 0
                    && fact.pending_approvals == 0
            })
        });
        let eligible = rt_source_candidate_clocks(&runtime)
            .map_err(|_| "wait_idle_candidate_probe_failed")?
            .into_iter()
            .find(|(s, id, _)| *s == source && id.as_ref() == native)
            .is_some_and(|(_, _, since)| since.is_some());
        if all_idle && eligible {
            return Ok(serde_json::json!({"ok": true, "idleNative": native}));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("l6_wait_idle_timeout");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn capacity_l6_apply_commands(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    native: &str,
    commands: &[CapacityL6Command],
) -> CapacityL6Result<()> {
    capacity_l6_apply_commands_on(f, c, native, commands, 0).await
}

async fn capacity_l6_apply_commands_on(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    native: &str,
    commands: &[CapacityL6Command],
    peer_index: usize,
) -> CapacityL6Result<()> {
    if commands.is_empty() {
        return Err("real_scenario_commands_required");
    }
    if !c.allow_model_requests {
        return Err("explicit_model_request_permission_required_for_scenario_commands");
    }
    let target = capacity_l6_register_target(f, native).await?.encode();
    let selection = f.selection_json();
    let mut last = serde_json::Value::Null;
    for command in commands {
        if command.action == "l6.submit_text" {
            let text = command.payload["text"]
                .as_str()
                .ok_or("explicit_prompt_text_required")?;
            last = capacity_l6_submit_text_on(f, peer_index, &target, text).await?;
            continue;
        }
        if command.action == "l6.decide_approval" {
            last = capacity_l6_decide_approval_on(
                f,
                peer_index,
                native,
                &selection,
                &command.payload,
            )
            .await?;
            continue;
        }
        if command.action == "l6.wait_idle" {
            last = capacity_l6_wait_idle(f, native, &command.payload).await?;
            continue;
        }
        if ![
            "input.",
            "queue.",
            "session.",
            "approval.",
            "source.session.",
        ]
        .iter()
        .any(|prefix| command.action.starts_with(prefix))
        {
            return Err("scenario_command_outside_session_queue_and_approval");
        }
        let payload = capacity_l6_expand(&command.payload, native, &target, &selection, &last)?;
        last = capacity_l6_call_on(f, peer_index, &command.action, payload).await?;
        if last["ok"] != true {
            return Err("real_scenario_command_not_accepted");
        }
    }
    Ok(())
}

fn capacity_l6_record_strings(value: &serde_json::Value, strings: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => strings.push(text.clone()),
        serde_json::Value::Array(values) => {
            for value in values {
                capacity_l6_record_strings(value, strings);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                capacity_l6_record_strings(value, strings);
            }
        }
        _ => {}
    }
}

fn capacity_l6_exact_body(
    view: &CapacityL6View,
    wanted: &str,
    forbidden: &[String],
) -> CapacityL6Result<()> {
    if wanted.is_empty() || !view.actual_complete {
        return Err("expected_exact_body_and_completed_original_read_required");
    }
    let mut strings = Vec::new();
    for (_, bytes) in &view.records {
        if let Ok(value) = serde_json::from_slice(bytes) {
            capacity_l6_record_strings(&value, &mut strings);
        }
    }
    if !strings.iter().any(|s| s.contains(wanted)) {
        return Err("expected_exact_real_body_not_received");
    }
    if forbidden
        .iter()
        .any(|text| !text.is_empty() && strings.iter().any(|s| s.contains(text)))
    {
        return Err("different_child_body_leaked_into_view");
    }
    Ok(())
}

async fn capacity_l6_body(
    f: &mut R3WssCase,
    c: &CapacityL6Config,
    case: &str,
    steps: &mut Vec<serde_json::Value>,
) -> CapacityL6Result<()> {
    let scenario: CapacityL6Scenario = if let Some(file) = c.scenario_files.get(case) {
        serde_json::from_slice(&std::fs::read(file).map_err(|_| "scenario_not_readable")?)
            .map_err(|_| "invalid_scenario")?
    } else {
        CapacityL6Scenario::default()
    };
    if scenario.format.is_some_and(|format| format != 1) {
        return Err("invalid_scenario_format");
    }
    match case {
        "cp011" => capacity_l6_cp011_work_body(f, c, &scenario, steps).await,
        "cp023" => capacity_l6_cp023_body(f, c, &scenario, steps).await,
        "cp026" => capacity_l6_cp026_body(f, c, &scenario, steps).await,
        "cp032" => capacity_l6_cp032_body(f, c, &scenario, steps).await,
        _ => Err("unsupported_real_capacity_case"),
    }
}

async fn capacity_l6_run(case: &str) -> Result<(), String> {
    let c = capacity_l6_config().map_err(str::to_owned)?;
    let (run_id, fingerprint) = capacity_l6_fingerprint().map_err(str::to_owned)?;
    std::fs::create_dir_all(&c.run_root).map_err(|_| "owned_run_root_creation_failed")?;
    let case_root = c.run_root.join(case);
    std::fs::create_dir(&case_root).map_err(|_| "unique_case_root_required_no_overwrite")?;
    let node_root = case_root.join("node");
    std::fs::create_dir(&node_root).map_err(|_| "owned_node_root_creation_failed")?;
    let (certificate, private_key) = write_certificate(&node_root);
    let mut f = R3WssCase {
        area: TestArea {
            directory: node_root,
            certificate,
            private_key,
        },
        owner: None,
        node: None,
        peers: Vec::new(),
        selection: None,
    };
    let mut steps = Vec::new();
    let mut binding = serde_json::Value::Null;
    let run = Box::pin(AssertUnwindSafe(async {
        let origin = capacity_l6_start(&mut f, &c).await?;
        binding=serde_json::json!({
            "nodeId":f.node.as_ref().ok_or("execution_original_node_missing")?.node_id(),
            "sourceId":f.selection.as_ref().ok_or("execution_original_source_missing")?.source.encode(),
            "url":origin
        });
        steps.push(
            serde_json::json!({"step":"real_node_catalog_wss","url":origin,"status":"passed"}),
        );
        Box::pin(capacity_l6_body(&mut f, &c, case, &mut steps)).await
    })
    .catch_unwind())
    .await;
    // Remove only this fixture's own fault, including on an interrupted branch.
    if let Some(pool) = f
        .node
        .as_ref()
        .and_then(|n| n.storage.as_deref())
        .map(crate::storage::Storage::pool_for_test)
    {
        let _ = sqlx::query("DROP TRIGGER IF EXISTS l6_ticket_source_fault")
            .execute(pool)
            .await;
        let _ = sqlx::query("DROP TRIGGER IF EXISTS l6_ticket_finish_fault")
            .execute(pool)
            .await;
        let _ = sqlx::query("DROP TRIGGER IF EXISTS l6_known_finish_fault")
            .execute(pool)
            .await;
        let _ = sqlx::query("DROP TRIGGER IF EXISTS l6_clock_finish_fault")
            .execute(pool)
            .await;
        let _ = sqlx::query("DROP TRIGGER IF EXISTS l6_owned_finish_fault")
            .execute(pool)
            .await;
    }
    let retained = f
        .node
        .as_ref()
        .and_then(|n| n.session_services.as_ref())
        .map(|s| s.native.clone());
    let unclosed_storage = f
        .node
        .as_ref()
        .and_then(|n| n.storage.as_deref())
        .map(|s| (s.pool_for_test().clone(), s.request_registry()));
    let address = f.node.as_ref().map(NodeRuntime::listen_addr);
    let prior_cleanup = steps
        .iter()
        .filter(|step| step.get("priorCleanupSources").is_some())
        .cloned()
        .collect::<Vec<_>>();
    let prior_valid = prior_cleanup.iter().all(|step| {
        step["priorCleanupJoined"] == true
            && step["priorCleanupMetadataConfirmed"] == true
            && step["priorCleanupPortClosed"] == true
            && step["priorCleanupSources"]
                .as_array()
                .is_some_and(|sources| {
                    !sources.is_empty()
                        && sources.iter().all(|source| {
                            source["physicalPresent"] == true
                                && source["activeProcesses"] == 0
                                && source["metadataConfirmed"] == true
                        })
                })
    });
    let node_closed = f
        .close_with_cleanup_budget(Duration::from_secs(300))
        .await;
    if !node_closed {
        if let Some(native) = &retained {
            let _ = native
                .shutdown(tokio::time::Instant::now() + Duration::from_secs(30))
                .await;
        }
        if let Some((pool, registry)) = unclosed_storage {
            let _ = timeout(Duration::from_secs(20), async {
                registry.close_connections().await;
                pool.close().await;
            })
            .await;
        }
    }
    let closed = node_closed && prior_valid;
    let diagnostics = retained
        .as_ref()
        .map(crate::application::native_dispatch::NativeRuntimeHandle::source_close_diagnostics)
        .unwrap_or_default();
    let job_zero = diagnostics.iter().all(|source| {
        matches!(
            source.physical_owner,
            Some(
                crate::application::native_dispatch::OwnerCloseKind::WindowsJob {
                    active_processes: 0
                }
            )
        )
    });
    let metadata_confirmed = !diagnostics.is_empty()
        && diagnostics
            .iter()
            .all(|source| source.metadata_close.as_ref().is_some_and(Result::is_ok));
    let port_closed = if let Some(address) = address {
        matches!(
            timeout(
                Duration::from_secs(10),
                tokio::net::TcpStream::connect(address)
            )
            .await,
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused
        )
    } else {
        true
    };
    if closed && metadata_confirmed && job_zero && port_closed && !diagnostics.is_empty() {
        capacity_l6_mark(&mut steps, case, "complete_owned_job_cleanup").map_err(str::to_owned)?;
    }
    let mut source_evidence=diagnostics.iter().map(|source|{
        let active=match source.physical_owner {
            Some(crate::application::native_dispatch::OwnerCloseKind::WindowsJob{active_processes})=>Some(active_processes),
            _=>None,
        };
        serde_json::json!({"physicalPresent":source.physical_present,
            "activeProcesses":active,"metadataConfirmed":source.metadata_close.as_ref().is_some_and(Result::is_ok),
            "taskSlots":source.task_slots,"jobSlots":source.job_slots})
    }).collect::<Vec<_>>();
    for prior in &prior_cleanup {
        if let Some(sources) = prior["priorCleanupSources"].as_array() {
            source_evidence.extend(sources.iter().cloned());
        }
    }
    let cleanup = serde_json::json!({
        "binding":binding,
        "joined":closed,"metadataConfirmed":metadata_confirmed,
        "jobActiveProcesses":if job_zero && !diagnostics.is_empty(){Some(0)}else{None},
        "portClosed":port_closed,"sources":source_evidence
    });
    let cleanup_sha =
        capacity_l6_write_new(&case_root.join("cleanup.json"), &cleanup).map_err(str::to_owned)?;
    let required = capacity_l6_required(case);
    let completed = steps
        .iter()
        .filter_map(|s| {
            if s["status"] == "passed" {
                s["branch"].as_str().map(str::to_owned)
            } else {
                None
            }
        })
        .collect::<std::collections::HashSet<_>>();
    let web_phases = steps
        .iter()
        .filter_map(|step| step.get("webPhase").cloned())
        .collect::<Vec<_>>();
    let web_pending = !web_phases.is_empty();
    let missing = required
        .iter()
        .filter(|id| {
            !(completed.contains(**id) || web_pending && **id == "same_node_real_harness_web")
        })
        .copied()
        .collect::<Vec<_>>();
    // Implementation coverage is explicit and independent of authentication or
    // runtime facts. Never call a route complete while code branches are absent.
    let missing_implementation = capacity_l6_unimplemented(case);
    let entry_complete = missing_implementation.is_empty();
    let execution_complete = matches!(&run, Ok(Ok(())))
        && missing.is_empty()
        && closed
        && metadata_confirmed
        && job_zero
        && port_closed
        && entry_complete;
    let (status, reason) = match run {
        Ok(Ok(())) if execution_complete && web_pending => {
            ("pending_independent_review", "independent_review")
        }
        Ok(Ok(())) if !entry_complete => ("blocked", "required_entry_branches_not_implemented"),
        Ok(Ok(())) if !missing.is_empty() => ("blocked", "required_actual_branch_not_confirmed"),
        Ok(Ok(())) => ("blocked", "owned_cleanup_or_same_node_browser_unconfirmed"),
        Ok(Err(reason)) => ("blocked", reason),
        Err(_) => ("failed", "test_runtime_or_assertion_failure"),
    };
    // Only assertions over actual Node/Harness facts mark a branch. Configuration
    // and partial commands cannot turn an unavailable required branch into passed.
    let result = serde_json::json!({
        "format":2,"kind":"capacity_l6_execution","runId":run_id,
        "fingerprint":fingerprint,"binding":binding,"case":case,"status":status,"reason":reason,"steps":steps,
        "defaultLimit":50,"idleSeconds":1800,"cleanupJoined":closed,
        "jobActiveProcesses":if job_zero && !diagnostics.is_empty(){Some(0)}else{None},"portClosed":port_closed,
        "requiredBranches":required.iter().map(|id|serde_json::json!({"id":id,"status":if completed.contains(*id){"passed"}else if web_pending && *id=="same_node_real_harness_web"{"pending_review"}else{"blocked"}})).collect::<Vec<_>>(),
        "missingBranches":missing,
        "entryImplementationComplete":entry_complete,"missingImplementationBranches":missing_implementation,
        "executionComplete":execution_complete,
        "pendingReasons":if execution_complete {vec!["independent_review"]} else {vec![reason]},
        "webPhases":web_phases,
        "cleanup":{"joined":closed,"metadataConfirmed":metadata_confirmed,
            "jobActiveProcesses":if job_zero && !diagnostics.is_empty(){Some(0)}else{None},
            "portClosed":port_closed,"sourceDiagnosticsFile":format!("{case}/cleanup.json"),
            "sourceDiagnosticsSha256":cleanup_sha},
        "branchMatrixSha256":"58fc0ded8e36e33a44e32a37afe8bdb24aa5f2aed601fd2a0094dd2ab878ba02",
        "realHarness":true,"fullCpPassed":false,
        "evidenceBoundary":"Only steps actually completed; configured scenario is not proof of capability. No raw bodies, authentication material or private profile environment are included."
    });
    capacity_l6_write_new(&case_root.join(format!("{case}.result.json")), &result)
        .map_err(str::to_owned)?;
    if status == "pending_independent_review" {
        Ok(())
    } else {
        Err(format!(
            "L6_{status}: {reason}; structured result in dedicated run directory"
        ))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit dedicated authenticated real Harness only; no default credentials; actual 50/1800s"]
async fn capacity_l6_cp011_real_fifty_and_thirty() -> Result<(), String> {
    Box::pin(capacity_l6_run("cp011")).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit real child scenario; missing real identities/capabilities is blocked"]
async fn capacity_l6_cp023_real_unopened_root_children() -> Result<(), String> {
    Box::pin(capacity_l6_run("cp023")).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit real standard replay; actual 50/1800s, no clock acceleration"]
async fn capacity_l6_cp026_real_replay_clock_and_pressure() -> Result<(), String> {
    Box::pin(capacity_l6_run("cp026")).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real Harness close plus explicitly scoped Node necessary-commit fault"]
async fn capacity_l6_cp032_real_known_work_and_finish_fault() -> Result<(), String> {
    Box::pin(capacity_l6_run("cp032")).await
}
