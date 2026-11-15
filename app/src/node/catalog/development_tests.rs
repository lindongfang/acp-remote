use super::{
    FakeProtector, Id128, NodeRuntime, SigningKey, TestArea, free_address, pair_browser,
    policy_actor,
};
use crate::{
    application::{
        native_catalog::CatalogRepository,
        native_dispatch::{ReadConfigurationPort, SourceReadSelectionProof},
        request_registry::RegistryDeadline,
    },
    domain::{
        EntityId, native_catalog::*, native_dispatch::DispatchError,
        source_sessions::SourceLaunchSelection,
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{sync::Arc, time::Duration};
fn d() -> RegistryDeadline {
    RegistryDeadline::new(tokio::time::Instant::now() + Duration::from_secs(5))
}
fn dual_port<T: crate::application::native_dispatch::ReadConfigurationPort>(_: &T) {}
fn utf16le_b64(value: &str) -> String {
    STANDARD.encode(
        value
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

#[tokio::test]
async fn node_configuration_core_real_paired_actor_and_revoke_before_catalog_sql() {
    let area = TestArea::new();
    let runtime = NodeRuntime::start(
        area.config(free_address()),
        std::sync::Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    let key = SigningKey::from_slice(&[38; 32]).unwrap();
    let client = pair_browser(&runtime, &key, "catalog development").await;
    let policy = runtime.application_policy().unwrap();
    let actor = policy_actor(&policy, &key, &client, Id128::new([21; 16])).await;
    let authority = runtime.request_authority().unwrap();
    let context = authority.current(&actor).await.unwrap();
    assert_eq!(context.client_id.encode(), client);
    let core = runtime
        .catalog_core(Arc::new(FakeProtector::default()), Duration::from_secs(5))
        .unwrap();
    let configuration: Arc<crate::session_storage::SqliteNodeConfiguration> = core.configuration();
    let same_configuration = core.configuration();
    assert!(Arc::ptr_eq(&configuration, &same_configuration));
    dual_port(configuration.as_ref());
    dual_port(same_configuration.as_ref());
    // Production schema9 is present; an empty catalog must preserve unknown-request semantics.
    assert_eq!(
        core.query(
            &actor,
            CatalogRequestId::new(EntityId::from_bytes([7; 16])),
            &d()
        )
        .await
        .unwrap_err()
        .kind(),
        CatalogError::UnknownRequest
    );
    core.reconcile(&d()).await.unwrap();
    crate::ipc::device_revoke(runtime.ipc_address(), &client)
        .await
        .unwrap();
    assert_eq!(
        core.query(
            &actor,
            CatalogRequestId::new(EntityId::from_bytes([7; 16])),
            &d()
        )
        .await
        .unwrap_err()
        .kind(),
        CatalogError::InvalidActor
    );
    assert_eq!(
        core.apply(
            &actor,
            CatalogRequestId::new(EntityId::from_bytes([7; 16])),
            &d()
        )
        .await
        .unwrap_err()
        .kind(),
        CatalogError::InvalidActor
    );
    let draft = CatalogDraft::bind_target_initial(
        EntityId::from_bytes([1; 16]),
        EntityId::from_bytes([2; 16]),
        EntityId::from_bytes([3; 16]),
        ProfileKey::try_new(EntityId::from_bytes([4; 16]), 1).unwrap(),
        EntityId::from_bytes([5; 16]),
        "C:\\launch".into(),
        "C:\\canonical".into(),
    )
    .unwrap();
    assert_eq!(
        core.register(&actor, draft, &d()).await.unwrap_err().kind(),
        CatalogError::InvalidActor
    );
    runtime.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn catalog_workspace_target_inventory_ipc_and_cl028_disk_verification() {
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::windows::named_pipe::ClientOptions,
    };

    let area = TestArea::new();
    let runtime = NodeRuntime::start(
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    let key = SigningKey::from_slice(&[51; 32]).unwrap();
    let client = pair_browser(&runtime, &key, "catalog registration").await;
    let policy = runtime.application_policy().unwrap();
    let actor = policy_actor(&policy, &key, &client, Id128::new([31; 16])).await;
    let services = runtime
        .session_services
        .as_ref()
        .unwrap()
        .management_services();
    let core = runtime
        .catalog_core(Arc::new(FakeProtector::default()), Duration::from_secs(5))
        .unwrap();
    let configuration = core.configuration();

    let ws_root = area.directory.join("ws-main");
    let ws_nested = ws_root.join("sub");
    let ws_extra = area.directory.join("ws-extra");
    let ws_outside = area.directory.join("ws-outside");
    let not_a_dir = area.directory.join("plain-file.txt");
    std::fs::create_dir(&ws_root).unwrap();
    std::fs::create_dir(&ws_nested).unwrap();
    std::fs::create_dir(&ws_extra).unwrap();
    std::fs::create_dir(&ws_outside).unwrap();
    std::fs::write(&not_a_dir, b"not-a-directory").unwrap();

    // Workspace registration: valid, idempotent, and invalid path rejections.
    let missing_path = area.directory.join("does-not-exist");
    for bad in [
        missing_path.to_str().unwrap(),
        not_a_dir.to_str().unwrap(),
        "relative/workspace",
    ] {
        assert_eq!(
            services
                .register_workspace(&actor, bad)
                .await
                .unwrap_err()
                .kind(),
            CatalogError::InvalidConfiguration
        );
        assert!(
            crate::ipc::workspace_register(runtime.ipc_address(), bad)
                .await
                .is_err()
        );
    }

    let ws_record = services
        .register_workspace(&actor, ws_root.to_str().unwrap())
        .await
        .unwrap();
    let ws_ipc = crate::ipc::workspace_register(runtime.ipc_address(), ws_root.to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(ws_ipc.workspace_id, ws_record.workspace_id.encode());
    assert_eq!(ws_ipc.canonical_path, ws_record.canonical_path);

    let extra_record =
        crate::ipc::workspace_register(runtime.ipc_address(), ws_extra.to_str().unwrap())
            .await
            .unwrap();
    let nested_canon = std::fs::canonicalize(&ws_nested)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let outside_canon = std::fs::canonicalize(&ws_outside)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    // Install harness + profile with sensitive env value and additional_directories.
    let profile_json = format!(
        r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}"],"environment":[{{"name_utf16le_base64":"{}","value_utf16le_base64":"{}"}}]}},"recipes":[{{"wire":2,"mcp_servers":[],"additional_directories":[{{"workspace_id":"{}","path":"{}"}}],"extra":{{}}}}]}}"#,
        utf16le_b64("C:\\Windows\\System32\\cmd.exe"),
        utf16le_b64("/c"),
        utf16le_b64("OPENAI_API_KEY"),
        utf16le_b64("SECRET_ENV_VALUE_CANARY"),
        extra_record.workspace_id,
        extra_record.canonical_path.replace('\\', "\\\\"),
    );
    let profile_draft =
        crate::adapters::acp::node_configuration_profile::parse_profile(profile_json.into_bytes())
            .unwrap();
    let install_draft = CatalogDraft::install_harness(
        "Codex CLI".into(),
        SourceSelection::New {
            name: "Codex Source".into(),
        },
        profile_draft,
    )
    .unwrap();
    let install_req = core.register(&actor, install_draft, &d()).await.unwrap();
    let install_receipt = core.apply(&actor, install_req, &d()).await.unwrap();
    let (harness_id, source_id, profile_key) = match install_receipt.fact().result().unwrap() {
        CatalogResult::Harness {
            harness,
            source,
            profile,
        } => (*harness, *source, *profile),
        other => panic!("expected harness install result, got {other:?}"),
    };

    // Target registration: validation, idempotency, conflict on different workspace.
    assert_eq!(
        services
            .register_target(
                &actor,
                EntityId::from_bytes([99; 16]),
                "codex-thread-1",
                ws_record.workspace_id,
            )
            .await
            .unwrap_err()
            .kind(),
        CatalogError::InvalidSource
    );
    assert_eq!(
        services
            .register_target(
                &actor,
                source_id,
                "codex-thread-1",
                EntityId::from_bytes([98; 16]),
            )
            .await
            .unwrap_err()
            .kind(),
        CatalogError::InvalidConfiguration
    );
    for bad_native in ["", "bad\0id"] {
        assert_eq!(
            services
                .register_target(&actor, source_id, bad_native, ws_record.workspace_id)
                .await
                .unwrap_err()
                .kind(),
            CatalogError::InvalidTarget
        );
    }

    let target_record = services
        .register_target(&actor, source_id, "codex-thread-1", ws_record.workspace_id)
        .await
        .unwrap();
    assert!(target_record.binding.is_none());
    let target_ipc = crate::ipc::target_register(
        runtime.ipc_address(),
        &source_id.encode(),
        "codex-thread-1",
        &ws_record.workspace_id.encode(),
    )
    .await
    .unwrap();
    assert_eq!(target_ipc.target_id, target_record.target_id.encode());

    assert_eq!(
        services
            .register_target(
                &actor,
                source_id,
                "codex-thread-1",
                EntityId::decode(&extra_record.workspace_id).unwrap(),
            )
            .await
            .unwrap_err()
            .kind(),
        CatalogError::Conflict
    );

    // Observe workspace & bind target.
    let observation = core
        .observe_workspace(
            &actor,
            target_record.target_id,
            source_id,
            ws_record.workspace_id,
            &d(),
        )
        .await
        .unwrap();
    let bind_draft = observation
        .bind(harness_id, profile_key, nested_canon.clone())
        .unwrap();
    let bind_req = core.register(&actor, bind_draft, &d()).await.unwrap();
    let bind_receipt = core.apply(&actor, bind_req, &d()).await.unwrap();
    assert!(matches!(
        bind_receipt.fact().result(),
        Some(CatalogResult::Target { .. })
    ));

    // Catalog inventory via WS service and Owner IPC never leaks protected secrets.
    let inventory = services.catalog_inventory(&actor).await.unwrap();
    assert_eq!(inventory.workspaces.len(), 2);
    assert_eq!(inventory.sources.len(), 1);
    assert_eq!(inventory.harnesses.len(), 1);
    assert_eq!(inventory.profiles.len(), 1);
    assert_eq!(inventory.targets.len(), 1);
    let bound = inventory.targets[0].binding.as_ref().unwrap();
    assert_eq!(bound.profile_id, profile_key.id());
    assert_eq!(bound.profile_revision, profile_key.revision());
    assert_eq!(bound.launch_path, nested_canon);

    let ipc_inv = crate::ipc::catalog_inventory(runtime.ipc_address())
        .await
        .unwrap();
    assert_eq!(ipc_inv.workspaces.len(), 2);
    assert_eq!(ipc_inv.targets.len(), 1);
    let ipc_json = serde_json::to_string(&ipc_inv).unwrap();
    assert!(!ipc_json.contains("SECRET_ENV_VALUE_CANARY"));
    assert!(!ipc_json.contains("OPENAI_API_KEY"));

    // Non-empty payload on IPC catalog.inventory must be rejected as invalid_request.
    {
        let mut pipe = ClientOptions::new()
            .open(&runtime.ipc_address().pipe_name)
            .unwrap();
        let req_id = Id128::new([44; 16]).to_base64url();
        let frame = serde_json::to_vec(&serde_json::json!({
            "v": 1,
            "requestId": req_id,
            "command": "catalog.inventory",
            "payload": { "extra": 1 }
        }))
        .unwrap();
        pipe.write_all(&frame).await.unwrap();
        pipe.write_all(b"\n").await.unwrap();
        pipe.flush().await.unwrap();
        let mut line = String::new();
        BufReader::new(pipe).read_line(&mut line).await.unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(resp["ok"], false);
        assert_eq!(resp["error"], "invalid_request");
    }

    // CL-028: resolve_source_read passes while dirs exist, fails if launch_path escapes workspace
    // or if workspace/additional_directories are removed or replaced by files.
    let valid_proof = SourceReadSelectionProof::mint(
        configuration.owner_scope(),
        SourceLaunchSelection {
            source: source_id,
            harness: harness_id,
            profile: profile_key,
            workspace: ws_record.workspace_id,
            launch_path: nested_canon.clone(),
        },
        "codex-thread-1",
    )
    .unwrap();
    let resolved = configuration
        .resolve_source_read(&valid_proof)
        .await
        .ok()
        .unwrap();
    assert_eq!(resolved.launch.cwd, std::path::PathBuf::from(&nested_canon));

    let escaped_proof = SourceReadSelectionProof::mint(
        configuration.owner_scope(),
        SourceLaunchSelection {
            source: source_id,
            harness: harness_id,
            profile: profile_key,
            workspace: ws_record.workspace_id,
            launch_path: outside_canon,
        },
        "codex-thread-1",
    )
    .unwrap();
    assert_eq!(
        configuration
            .resolve_source_read(&escaped_proof)
            .await
            .err()
            .unwrap(),
        DispatchError::InvalidConfiguration
    );

    // Delete additional directory -> resolve_source_read refuses with InvalidConfiguration.
    std::fs::remove_dir(&ws_extra).unwrap();
    assert_eq!(
        configuration
            .resolve_source_read(&valid_proof)
            .await
            .err()
            .unwrap(),
        DispatchError::InvalidConfiguration
    );
    std::fs::create_dir(&ws_extra).unwrap();
    assert!(
        configuration
            .resolve_source_read(&valid_proof)
            .await
            .is_ok()
    );

    // Replace nested launch directory with a regular file -> resolve_source_read refuses.
    std::fs::remove_dir(&ws_nested).unwrap();
    std::fs::write(&ws_nested, b"file-not-dir").unwrap();
    assert_eq!(
        configuration
            .resolve_source_read(&valid_proof)
            .await
            .err()
            .unwrap(),
        DispatchError::InvalidConfiguration
    );
    std::fs::remove_file(&ws_nested).unwrap();
    std::fs::create_dir(&ws_nested).unwrap();

    // Delete registered main workspace -> observe_workspace and register_target refuse.
    std::fs::remove_dir_all(&ws_root).unwrap();
    assert_eq!(
        core.observe_workspace(
            &actor,
            target_record.target_id,
            source_id,
            ws_record.workspace_id,
            &d(),
        )
        .await
        .err()
        .unwrap()
        .kind(),
        CatalogError::InvalidConfiguration
    );
    assert_eq!(
        services
            .register_target(&actor, source_id, "codex-thread-2", ws_record.workspace_id)
            .await
            .unwrap_err()
            .kind(),
        CatalogError::InvalidConfiguration
    );
    assert_eq!(
        configuration
            .resolve_source_read(&valid_proof)
            .await
            .err()
            .unwrap(),
        DispatchError::InvalidConfiguration
    );

    runtime.shutdown(None).await.unwrap();
}
