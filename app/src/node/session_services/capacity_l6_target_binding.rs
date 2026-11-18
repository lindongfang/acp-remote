// Target registration and launch selection are different normal product facts.
// This client prepares the latter through the authenticated Catalog upload path.
async fn capacity_l6_bind_target(
    f: &mut R3WssCase,
    target: crate::domain::EntityId,
) -> CapacityL6Result<()> {
    let selection = f.selection.clone().ok_or("target_selection_missing")?;
    let expected = (
        selection.profile.id().encode(),
        selection.profile.revision().to_be_bytes().to_vec(),
        selection.workspace.encode(),
        selection.launch_path.clone(),
    );
    let pool = capacity_l6_clock_pool(f)?;
    let query = "SELECT profile_id,profile_revision,workspace_id,launch_path \
                 FROM node_target_launch_selection WHERE target_id=?";
    let before: Option<(String, Vec<u8>, String, String)> = sqlx::query_as(query)
        .bind(target.encode())
        .fetch_optional(&pool)
        .await
        .map_err(|_| "target_binding_read_failed")?;
    if let Some(before) = before {
        return if before == expected {
            Ok(())
        } else {
            Err("target_existing_launch_selection_conflicts")
        };
    }
    let body = serde_json::to_vec(&serde_json::json!({
        "kind":"bind_target_initial","targetId":target.encode(),
        "sourceId":selection.source.encode(),"harnessId":selection.harness.encode(),
        "profileId":selection.profile.id().encode(),
        "profileRevision":selection.profile.revision().to_string(),
        "workspaceId":selection.workspace.encode(),"launchPath":selection.launch_path
    }))
    .map_err(|_| "target_catalog_document_encoding_failed")?;
    let upload = capacity_l6_upload_catalog_document(f, &body).await?;
    let registered = capacity_l6_call(
        f,
        "catalog.register",
        serde_json::json!({"documentUploadId":upload.to_base64url()}),
    )
    .await?;
    let request = registered["result"]["catalogRequestId"]
        .as_str()
        .ok_or("target_catalog_binding_not_registered")?;
    let applied = capacity_l6_call(
        f,
        "catalog.apply",
        serde_json::json!({"catalogRequestId":request}),
    )
    .await?;
    if applied["ok"] != true
        || applied["result"]["fact"]["state"] != "committed"
        || applied["result"]["fact"]["result"]["targetId"] != target.encode()
    {
        return Err("target_catalog_binding_not_committed");
    }
    let after: Option<(String, Vec<u8>, String, String)> = sqlx::query_as(query)
        .bind(target.encode())
        .fetch_optional(&pool)
        .await
        .map_err(|_| "target_committed_binding_read_failed")?;
    if after != Some(expected) {
        return Err("target_committed_binding_not_original_selection");
    }
    let released = capacity_l6_call(
        f,
        "catalog.upload.release",
        serde_json::json!({"uploadId":upload.to_base64url()}),
    )
    .await?;
    if released["ok"] != true {
        return Err("target_catalog_upload_cleanup_unconfirmed");
    }
    Ok(())
}

async fn capacity_l6_upload_catalog_document(
    f: &mut R3WssCase,
    body: &[u8],
) -> CapacityL6Result<Id128> {
    let started = capacity_l6_call(f, "catalog.upload.begin", serde_json::json!({})).await?;
    let upload = Id128::parse_base64url(
        started["result"]["uploadId"]
            .as_str()
            .ok_or("target_catalog_upload_not_created")?,
    )
    .map_err(|_| "target_catalog_upload_identity_invalid")?;
    capacity_l6_upload_frame(f, upload, body).await?;
    let finished = capacity_l6_call(
        f,
        "catalog.upload.finish",
        serde_json::json!({"uploadId":upload.to_base64url(),"totalBytes":body.len().to_string()}),
    )
    .await?;
    if finished["ok"] != true {
        return Err("target_catalog_upload_finish_failed");
    }
    timeout(Duration::from_secs(30), async {
        loop {
            let status = capacity_l6_call(
                f,
                "catalog.upload.status",
                serde_json::json!({"uploadId":upload.to_base64url()}),
            )
            .await?;
            match status["result"]["phase"].as_str() {
                Some("ready") => return Ok::<(), &'static str>(()),
                Some("preparing") => {}
                _ => return Err("target_catalog_document_not_ready"),
            }
        }
    })
    .await
    .map_err(|_| "target_catalog_document_preparation_timeout")??;
    Ok(upload)
}

async fn capacity_l6_upload_frame(
    f: &mut R3WssCase,
    upload: Id128,
    body: &[u8],
) -> CapacityL6Result<()> {
    let mut wire = vec![0; 32];
    wire[..4].copy_from_slice(b"ACPI");
    wire[4] = 1;
    wire[5] = 1;
    wire[8..24].copy_from_slice(upload.as_bytes());
    wire.extend_from_slice(body);
    f.peers[0]
        .socket
        .send(Message::Binary(wire.into()))
        .await
        .map_err(|_| "owned_catalog_upload_write_failed")?;
    timeout(Duration::from_secs(10), async {
        loop {
            match f.peers[0].socket.next().await {
                Some(Ok(Message::Binary(ack))) => {
                    if ack.len() != 32
                        || &ack[..8] != b"ACPI\x01\x02\0\0"
                        || &ack[8..24] != upload.as_bytes()
                        || ack[24..] != (body.len() as u64).to_be_bytes()
                    {
                        return Err("owned_catalog_upload_ack_identity_or_offset_mismatch");
                    }
                    return Ok(());
                }
                Some(Ok(Message::Text(text))) => {
                    let event: serde_json::Value = serde_json::from_str(text.as_str())
                        .map_err(|_| "invalid_catalog_upload_live_event")?;
                    if event.get("requestId").is_some() {
                        return Err("unexpected_catalog_upload_command_response");
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                _ => return Err("owned_catalog_upload_ack_socket_closed"),
            }
        }
    })
    .await
    .map_err(|_| "owned_catalog_upload_ack_timeout")?
}
