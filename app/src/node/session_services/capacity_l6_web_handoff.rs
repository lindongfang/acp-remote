// Included only in the Windows/native-dispatch-dev-fixtures R3 test module.
// This collects independent browser evidence; it never signs a DOM verdict.

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapacityL6WebExpected {
    native_ids: Vec<String>,
    operation_ids: Vec<String>,
    occupied: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    history_incomplete: Option<bool>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapacityL6WebReceipt {
    format: u32,
    binding: serde_json::Value,
    browser_client_id: String,
    capture_sha256: String,
    actions_sha256: String,
    capture_source_sha256: String,
}

struct CapacityL6WebFiles {
    info: std::path::PathBuf,
    receipt: std::path::PathBuf,
    control: crate::ipc::IpcControl,
    stop_on_drop: bool,
}

impl Drop for CapacityL6WebFiles {
    fn drop(&mut self) {
        // These are only the fixed files created in this unique owned phase.
        // Never recursively remove an authentication home or captured evidence.
        let _ = std::fs::remove_file(&self.info);
        let _ = std::fs::remove_file(&self.receipt);
        if self.stop_on_drop {
            // Cancellation cannot spawn an unowned async revocation task. The
            // existing Node owner performs shutdown and its caller must join.
            self.control.request_stop();
        }
    }
}

fn capacity_l6_web_sha256(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    hex::encode(sha2::Sha256::digest(bytes))
}

fn capacity_l6_web_owned_path(path: &std::path::Path) -> Result<(), &'static str> {
    use std::os::windows::fs::MetadataExt as _;
    if !path.is_absolute() {
        return Err("web_owned_absolute_path_required");
    }
    for component in path.ancestors() {
        let metadata =
            std::fs::symlink_metadata(component).map_err(|_| "web_owned_path_not_available")?;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("web_owned_path_reparse_rejected");
        }
    }
    Ok(())
}

fn capacity_l6_web_write_new(
    path: &std::path::Path,
    value: &serde_json::Value,
) -> Result<(), &'static str> {
    use std::io::Write as _;
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "web_info_encoding_failed")?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "web_unique_file_required")?;
    file.write_all(&bytes)
        .map_err(|_| "web_owned_file_write_failed")?;
    file.sync_all().map_err(|_| "web_owned_file_sync_failed")
}

fn capacity_l6_web_expected(value: serde_json::Value) -> Result<serde_json::Value, &'static str> {
    let expected: CapacityL6WebExpected =
        serde_json::from_value(value).map_err(|_| "web_expected_identity_shape_invalid")?;
    if expected.occupied > 50
        || expected
            .native_ids
            .iter()
            .chain(&expected.operation_ids)
            .any(String::is_empty)
        || expected
            .mode
            .as_deref()
            .is_some_and(|mode| !["replay_complete", "current_only"].contains(&mode))
    {
        return Err("web_expected_identity_or_policy_invalid");
    }
    serde_json::to_value(expected).map_err(|_| "web_expected_encoding_failed")
}

fn capacity_l6_web_artifact(
    phase_root: &std::path::Path,
    name: &str,
    expected_hash: &str,
) -> Result<Vec<u8>, &'static str> {
    if expected_hash.len() != 64
        || !expected_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("web_artifact_hash_invalid");
    }
    let path = phase_root.join(name);
    capacity_l6_web_owned_path(&path)?;
    let bytes = std::fs::read(path).map_err(|_| "web_artifact_not_readable")?;
    if bytes.is_empty() || capacity_l6_web_sha256(&bytes) != expected_hash {
        return Err("web_artifact_empty_or_digest_mismatch");
    }
    Ok(bytes)
}

fn capacity_l6_web_collect(
    phase_root: &std::path::Path,
    binding: &serde_json::Value,
    client: &str,
) -> Result<serde_json::Value, &'static str> {
    let receipt_path = phase_root.join("receipt.json");
    capacity_l6_web_owned_path(&receipt_path)?;
    let bytes = std::fs::read(receipt_path).map_err(|_| "web_receipt_not_readable")?;
    let receipt: CapacityL6WebReceipt =
        serde_json::from_slice(&bytes).map_err(|_| "web_receipt_shape_invalid")?;
    if receipt.format != 1 || receipt.binding != *binding || receipt.browser_client_id != client {
        return Err("web_receipt_does_not_match_original_phase_and_browser");
    }
    let capture = capacity_l6_web_artifact(phase_root, "capture.json", &receipt.capture_sha256)?;
    let actions = capacity_l6_web_artifact(phase_root, "actions.json", &receipt.actions_sha256)?;
    let _source = capacity_l6_web_artifact(
        phase_root,
        "capture-source.txt",
        &receipt.capture_source_sha256,
    )?;
    for (bytes, kind, field) in [
        (&capture, "independent-browser-dom-capture", "dom"),
        (&actions, "independent-browser-actions", "actions"),
    ] {
        let document: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| "web_capture_json_invalid")?;
        if document["binding"] != *binding
            || document["browserClientId"] != client
            || document["kind"] != kind
            || (field == "dom"
                && document[field]
                    .as_object()
                    .is_none_or(serde_json::Map::is_empty))
            || (field == "actions" && document[field].as_array().is_none_or(Vec::is_empty))
        {
            return Err("web_capture_original_binding_missing");
        }
    }
    capacity_l6_web_write_new(
        &phase_root.join("receipt-consumed.json"),
        &serde_json::json!({
            "phaseId":binding["phaseId"],
            "status":"collected_pending_independent_review"
        }),
    )?;
    // Do not return pairing codes, correlation nonce, raw DOM or action text.
    // A matching file/hash is provenance, not proof that a browser ran.
    Ok(serde_json::json!({
        "format":1,
        "phaseId":binding["phaseId"],
        "phase":binding["phase"],
        "nodeId":binding["nodeId"],
        "sourceId":binding["sourceId"],
        "url":binding["url"],
        "expected":binding["expected"],
        "browserClientId":client,
        "status":"pending_independent_review",
        "acceptancePassed":false,
        "captureSha256":receipt.capture_sha256,
        "actionsSha256":receipt.actions_sha256,
        "captureSourceSha256":receipt.capture_source_sha256,
        "independentReviewRequired":true
    }))
}

async fn capacity_l6_web_browser(
    pairing: &mut crate::ipc::PairingSession,
    expected_pairing: &str,
    expected_name: &str,
    original_clients: &[String],
) -> Result<String, &'static str> {
    let pending = pairing
        .next_event()
        .await
        .map_err(|_| "web_pairing_claim_not_received")?;
    if pending.event != "pending_confirmation"
        || pending.pairing_id != expected_pairing
        || pending.display_name.as_deref() != Some(expected_name)
    {
        let _ = pairing.reject().await;
        return Err("web_pairing_claim_does_not_match_this_phase");
    }
    pairing
        .approve()
        .await
        .map_err(|_| "web_pairing_approval_failed")?;
    let approved = pairing
        .next_event()
        .await
        .map_err(|_| "web_pairing_approval_not_confirmed")?;
    if approved.event != "approved" || approved.pairing_id != expected_pairing {
        return Err("web_pairing_result_does_not_match_original");
    }
    let client = approved
        .client_id
        .ok_or("web_pairing_has_no_original_client")?;
    if original_clients.contains(&client) {
        return Err("web_browser_must_be_new_dedicated_identity");
    }
    Ok(client)
}

async fn capacity_l6_web_phase(
    f: &mut R3WssCase,
    case_root: &std::path::Path,
    phase: &str,
    expected: serde_json::Value,
    budget: Duration,
) -> Result<serde_json::Value, &'static str> {
    if phase.is_empty()
        || !phase
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || budget.is_zero()
        || budget > Duration::from_hours(24)
    {
        return Err("web_phase_or_deadline_invalid");
    }
    let expected = capacity_l6_web_expected(expected)?;
    capacity_l6_web_owned_path(case_root)?;
    let actual_parent = f
        .area
        .directory
        .parent()
        .ok_or("web_original_node_parent_missing")?
        .canonicalize()
        .map_err(|_| "web_original_node_parent_unavailable")?;
    if case_root
        .canonicalize()
        .map_err(|_| "web_case_root_unavailable")?
        != actual_parent
    {
        return Err("web_case_root_not_owned_by_original_node");
    }
    let phase_root = case_root.join(format!("web-{phase}"));
    std::fs::create_dir(&phase_root).map_err(|_| "web_phase_must_be_unique_no_overwrite")?;
    crate::platform::protect_private_directory(&phase_root)
        .map_err(|_| "web_phase_private_directory_required")?;
    let node = f.node.as_ref().ok_or("web_original_node_missing")?;
    let selection = f.selection.as_ref().ok_or("web_original_source_missing")?;
    let address = node.ipc_address().clone();
    let mut files = CapacityL6WebFiles {
        info: phase_root.join("info.json"),
        receipt: phase_root.join("receipt.json"),
        control: node.control.clone(),
        stop_on_drop: true,
    };
    let original_clients: Vec<_> = f.peers.iter().map(|p| p.client_id.clone()).collect();
    let phase_id =
        Id128::new(crate::random::bytes().map_err(|_| "web_phase_randomness_unavailable")?)
            .to_base64url();
    let display_name = format!("L6 browser {phase_id}");
    let mut approved_client = None;
    let outcome = tokio::time::timeout(budget, async {
        let (mut pairing, start) = crate::ipc::begin_pairing(&address)
            .await
            .map_err(|_| "web_original_node_pairing_unavailable")?;
        if start.node_id != node.node_id()
            || start.origin != format!("https://localhost:{}", node.listen_addr().port())
        {
            return Err("web_pairing_node_or_origin_mismatch");
        }
        let binding = serde_json::json!({
            "phaseId":phase_id,"phase":phase,"nodeId":node.node_id(),
            "sourceId":selection.source.encode(),"url":start.origin,
            "nonce":Id128::new(crate::random::bytes()
                .map_err(|_| "web_phase_randomness_unavailable")?).to_base64url(),
            "expected":expected,
        });
        capacity_l6_web_write_new(
            &files.info,
            &serde_json::json!({
                "format":1,"binding":binding,"selection":f.selection_json(),
                "pairingCode":start.code,"expiresAt":start.expires_at,"displayName":display_name,
                "receiptFile":"receipt.json",
                "requiredPrivateArtifacts":["capture.json","actions.json","capture-source.txt"],
                "scope":"owned same-node handoff only; independent browser evidence required"
            }),
        )?;
        let client = capacity_l6_web_browser(
            &mut pairing,
            &start.pairing_id,
            &display_name,
            &original_clients,
        )
        .await?;
        approved_client = Some(client.clone());
        std::fs::remove_file(&files.info).map_err(|_| "web_pairing_secret_cleanup_failed")?;
        capacity_l6_web_write_new(
            &phase_root.join("paired.json"),
            &serde_json::json!({
                "format":1,"phaseId":phase_id,"browserClientId":client,
                "status":"paired_only_not_dom_verdict"
            }),
        )?;
        loop {
            if files.receipt.exists() {
                return capacity_l6_web_collect(&phase_root, &binding, &client);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    let result = outcome.unwrap_or(Err("web_phase_deadline_missing_independent_evidence"));
    if let Some(client) = approved_client {
        let revoke = tokio::time::timeout(
            Duration::from_secs(30),
            crate::ipc::device_revoke(&address, &client),
        )
        .await
        .map_err(|_| "web_browser_revoke_deadline")?
        .map_err(|_| "web_browser_revoke_not_confirmed")?;
        if revoke.client_id != client || revoke.status != crate::trust::ClientStatus::Revoked {
            return Err("web_browser_revocation_identity_mismatch");
        }
        capacity_l6_web_write_new(
            &phase_root.join("revoked.json"),
            &serde_json::json!({
                "format":1,"phaseId":phase_id,"browserClientId":client,
                "status":"original_identity_revoked"
            }),
        )?;
    }
    if files.receipt.exists() {
        std::fs::remove_file(&files.receipt).map_err(|_| "web_receipt_secret_cleanup_failed")?;
    }
    files.stop_on_drop = result.is_err();
    result.map(|mut summary| {
        summary["browserIdentityRevoked"] = serde_json::json!(true);
        summary
    })
}
