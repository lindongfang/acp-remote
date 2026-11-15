//! Real local SQLite, R3, ACP and Windows Job through Storage's production composition.
use super::*;
use crate::application::shared_views::RetainedRaw;
use crate::domain::native_dispatch::TransportReason;
use crate::{
    adapters::acp::{
        native_dispatch::{AcpNativeAdapter, AcpStreams},
        raw_json::{OwnedRawValue, RawDocument},
        *,
    },
    application::{
        queue_owner::{ConsumeReply, QueueRead},
        request_registry::*,
    },
    domain::ActorContext,
    platform::native_source::{
        WindowsLaunchPlan, WindowsOwnedSource, WindowsRunningProof, WindowsSourceLauncher,
    },
    session_storage::native_configuration::*,
    storage::{NodeMeta, SCHEMA_VERSION},
};
use sha2::{Digest, Sha256};
use sqlx::Connection;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::{path::PathBuf, time::Duration};
use tokio::time::{Instant, timeout};

#[tokio::test]
async fn a2_native_default_resume_saves_sql_without_a_caller_audit() {
    let (storage, area, scope) = a2_native_empty_storage().await;
    let configuration = Arc::new(storage.native_configuration_provider(Arc::new(Configuration)));
    let (service, native) = storage
        .start_native_service(
            Authority(storage.pool_for_test().clone()),
            Validator,
            SystemNodeClock,
            Arc::new(AcpNativeAdapter::default()),
            Arc::new(WindowsSourceLauncher),
            configuration,
            Arc::new(Retain),
            options(),
        )
        .await
        .unwrap();
    let result: Result<(), String> = async {
        let mutation = ValidatedMutation::Resume;
        let request = service.register(&id(10),id(4),&mutation).await.map_err(|_| "native default register failed")?;
        if !matches!(service.consume(&id(10),id(4),request.id(),mutation).await,
            ConsumeReply::Resolved { fact: RequestFact::Settled { .. }, .. }) {
            return Err("native default empty Resume did not settle".into());
        }
        timeout(Duration::from_secs(10), async {
            loop {
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM main.node_management_audit WHERE request_id=? AND action='input.resume' AND outcome='resumed' AND restored_count=0")
                    .bind(request.id().encode()).fetch_one(storage.pool_for_test()).await.unwrap();
                if count==1 { break; }
                tokio::task::yield_now().await;
            }
        }).await.map_err(|_| "native default SQL audit not observed")?;
        if !service.take_audit_warnings().is_empty() || !native.core.sources.lock().unwrap().is_empty() {
            return Err("empty Resume produced an audit warning or unexpected native source".into());
        }
        eprintln!("a2 native actual default factory/queue/SQL Resume; zero native launches, not a Harness execution test");
        Ok(())
    }.await;
    let close = native
        .shutdown(Instant::now() + Duration::from_secs(5))
        .await;
    let result = if close.joined && close.metadata_confirmed {
        result
    } else {
        Err("native default owner not physically/metadata joined".into())
    };
    drop(service);
    drop(native);
    a2_native_finish(storage, area, scope, result).await;
}

struct NativeArea {
    directory: PathBuf,
}
impl NativeArea {
    fn remove_owned_tree(&self) -> std::io::Result<()> {
        std::fs::remove_dir_all(&self.directory)
    }
}
impl std::ops::Deref for NativeArea {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.directory
    }
}

async fn a2_native_empty_storage() -> (
    crate::sqlite_fixture_scope::FixtureStorage,
    Arc<NativeArea>,
    crate::sqlite_fixture_scope::SqliteFixtureScope,
) {
    let area = std::env::temp_dir().join(format!(
        "a2-native-audit-dev-{}-{}",
        std::process::id(),
        random_id().unwrap().encode()
    ));
    std::fs::create_dir(&area).unwrap();
    let area = Arc::new(NativeArea { directory: area });
    let scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
        &area.join("node.sqlite3"),
        area.clone(),
    );
    let storage = scope.open().await.unwrap();
    storage
        .write_node_meta(&NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "A2-native-development".into(),
            public_key: vec![1, 2, 3],
            key_file_ref: "synthetic-unused".into(),
        })
        .await
        .unwrap();
    sqlx::query("INSERT INTO main.clients VALUES(?,?,'A2 native dev','trusted',1,0,NULL)")
        .bind(id(10).encode())
        .bind(vec![1u8])
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO main.product_sources VALUES(?,'A2 native dev')")
        .bind(id(1).encode())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO main.product_targets VALUES(?,?,?,NULL)")
        .bind(id(4).encode())
        .bind(id(1).encode())
        .bind("A2-native-empty-resume")
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    (storage, area, scope)
}

async fn a2_native_finish(
    storage: crate::sqlite_fixture_scope::FixtureStorage,
    area: Arc<NativeArea>,
    scope: crate::sqlite_fixture_scope::SqliteFixtureScope,
    result: Result<(), String>,
) {
    storage.close().await;
    scope.stop_and_observe().await;
    let mut errors: Vec<String> = result.err().into_iter().collect();
    if !storage.registry_close_status().is_complete() {
        errors.push("native default registry close incomplete".into());
    }
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let path = area.join(format!("node.sqlite3{suffix}"));
        if !suffix.is_empty() && !path.exists() {
            eprintln!("a2-native first-exclusive {suffix}=absent");
            continue;
        }
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(path)
        {
            Ok(handle) => {
                drop(handle);
                eprintln!("a2-native first-exclusive {suffix}=ok");
            }
            Err(error) => errors.push(format!("native first-exclusive {suffix}: {error:?}")),
        }
    }
    drop(storage);
    assert!(
        errors.is_empty(),
        "a2-native preserved={} errors={errors:?}",
        area.display()
    );
    let actual = area.canonicalize().unwrap();
    assert_eq!(
        actual.parent(),
        Some(std::env::temp_dir().canonicalize().unwrap().as_path())
    );
    assert!(
        actual
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&format!("a2-native-audit-dev-{}-", std::process::id()))
    );
    assert_eq!(
        std::fs::symlink_metadata(&actual)
            .unwrap()
            .file_attributes()
            & 0x400,
        0
    );
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let path = actual.join(format!("node.sqlite3{suffix}"));
        if path.exists() {
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            assert!(metadata.is_file());
            assert_eq!(metadata.file_attributes() & 0x400, 0);
            std::fs::remove_file(path).unwrap();
        }
    }
    std::fs::remove_dir(actual).unwrap();
}
type ReleasedResourceRow = (String, String, String, Option<String>, Option<String>);
const fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
struct Authority(sqlx::SqlitePool);
impl RequestAuthority for Authority {
    type Actor = EntityId;
    async fn authorize(
        &self,
        a: &EntityId,
        _: EntityId,
        _: QueueAction,
    ) -> Result<ActorContext, RegistryError> {
        let active:i64=sqlx::query_scalar("SELECT count(*) FROM clients WHERE client_id=? AND status='trusted' AND auth_generation=1").bind(a.encode()).fetch_one(&self.0).await?;
        if active == 1 {
            Ok(ActorContext {
                client_id: *a,
                auth_generation: 1,
            })
        } else {
            Err(RegistryError::InvalidActor)
        }
    }
    async fn authorize_read(
        &self,
        a: &EntityId,
        t: EntityId,
    ) -> Result<ActorContext, RegistryError> {
        self.authorize(a, t, QueueAction::Submit).await
    }
    async fn resolve_target(&self, t: EntityId) -> Result<NativeSessionKey, RegistryError> {
        let (s, n): (String, String) =
            sqlx::query_as("SELECT source_id,native_id FROM product_targets WHERE target_id=?")
                .bind(t.encode())
                .fetch_one(&self.0)
                .await?;
        NativeSessionKey::from_config(
            Some(EntityId::decode(&s).map_err(|_| RegistryError::InvalidTarget)?),
            &n,
        )
        .map_err(|_| RegistryError::InvalidTarget)
    }
}
pub struct Validator;
impl ValidatedBodyPort for Validator {
    type Raw = str;
    type Body = OwnedRawValue;
    fn validate(&self, raw: &str) -> Result<(OwnedRawValue, [u8; 32]), RegistryError> {
        let document = RawDocument::parse(raw.as_bytes().to_vec())
            .map_err(|_| RegistryError::InvalidIntent)?;
        if document.root().array().is_none() {
            return Err(RegistryError::InvalidIntent);
        }
        Ok((document.root_owned(), Sha256::digest(raw.as_bytes()).into()))
    }
}
pub struct Retain;
impl RetainPrompt<OwnedRawValue> for Retain {
    type Retained = OwnedRawValue;
    fn retain(&self, b: &ValidatedBody<OwnedRawValue>) -> OwnedRawValue {
        b.body().clone()
    }
}
struct Audit;
impl AuditPort for Audit {
    fn append<'a>(&'a self, _: &'a AuditMetadata) -> BoxFuture<'a, Result<(), AuditFailure>> {
        Box::pin(async { Ok(()) })
    }
}
pub struct Configuration;
struct BridgeReadOptions {
    calls: std::sync::atomic::AtomicUsize,
    fail_next: AtomicBool,
    inner: crate::application::shared_views::budget::MonotonicReadOptionsProvider,
}
impl BridgeReadOptions {
    const fn new() -> Self {
        Self {
            calls: std::sync::atomic::AtomicUsize::new(0),
            fail_next: AtomicBool::new(false),
            inner: crate::application::shared_views::budget::MonotonicReadOptionsProvider::new(
                Duration::from_secs(5),
                crate::application::shared_views::budget::ReadResponseBudget::Unbounded,
            ),
        }
    }
}
impl crate::application::shared_views::budget::NativeReadOptionsProvider for BridgeReadOptions {
    fn for_new_operation(
        &self,
        context: &crate::application::shared_views::budget::NativeReadStartContext<'_>,
    ) -> Result<
        crate::application::shared_views::budget::NativeReadOptions,
        crate::domain::shared_view::ViewError,
    > {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if self.fail_next.swap(false, Ordering::AcqRel) {
            // A controlled provider error, not a claim of a real system allocation failure.
            return Err(crate::domain::shared_view::ViewError::AllocationFailure);
        }
        crate::application::shared_views::budget::NativeReadOptionsProvider::for_new_operation(
            &self.inner,
            context,
        )
    }
}
async fn bridge_wait_read(
    request: &crate::application::shared_views::read_control::NativeReadRequest,
    expected: crate::domain::shared_view::ReadMode,
) -> crate::application::shared_views::read_control::NativeReadMetadata {
    timeout(Duration::from_secs(8), async {
        let mut changes = request.subscribe();
        loop {
            let status = changes.borrow_and_update().clone();
            if status.read_mode() == expected && (expected == crate::domain::shared_view::ReadMode::ReplayIncomplete
                || status.transport() == crate::application::shared_views::read_control::ReadTransportState::Written)
            { return status; }
            changes.changed().await.unwrap();
        }
    }).await.unwrap()
}
impl TrustedNativeReadConfiguration for Configuration {
    fn selected_read_harness(
        &self,
        read: &crate::application::queue_owner::ReadTargetProof,
    ) -> Result<EntityId, DispatchError> {
        Ok(if read.native().source().0 == id(7) {
            id(9)
        } else {
            id(3)
        })
    }
}
impl TrustedNativeConfiguration for Configuration {
    fn selected_harness(&self, accepted: &AcceptedTargetProof) -> Result<EntityId, DispatchError> {
        Ok(if accepted.native().source().0 == id(7) {
            id(9)
        } else {
            id(3)
        })
    }
    fn effective(
        &self,
        d: &NativeDefinition,
    ) -> Result<EffectiveNativeConfiguration, DispatchError> {
        let mut setups = Vec::new();
        for (wire, version) in [
            (ProtocolVersion::V1, Version::V1),
            (ProtocolVersion::V2, Version::V2),
        ] {
            setups.push((
                wire,
                SetupSnapshot {
                    wire: version,
                    source: ConfigSourceStamp {
                        reference: Arc::from(id(6).encode()),
                        revision: 1,
                    },
                    cwd: Known::Known(Arc::from(
                        d.cwd.to_str().ok_or(DispatchError::InvalidConfiguration)?,
                    )),
                    mcp_servers: Known::Known(vec![]),
                    additional_directories: Known::Known(vec![]),
                    extra: ExtensionFields::empty()
                        .map_err(|_| DispatchError::InvalidConfiguration)?,
                },
            ));
        }
        Ok(EffectiveNativeConfiguration {
            identity: id(6),
            revision: 1,
            environment: vec![],
            setups,
        })
    }
}
pub const fn options() -> NativeRuntimeOptions {
    NativeRuntimeOptions {
        spawn_wait: Duration::from_secs(8),
        initialize_write_wait: Duration::from_secs(5),
        initialize_response_wait: Duration::from_secs(5),
        load_write_wait: Duration::from_secs(5),
        load_response: ResponseBudget::Wait(Duration::from_secs(5)),
        prompt_write_wait: Duration::from_secs(5),
        prompt_response: ResponseBudget::Wait(Duration::from_secs(5)),
        metadata_wait: Duration::from_secs(5),
        close_wait: Duration::from_secs(5),
    }
}
#[tokio::test]
async fn native_source_cleanup_lock_cut_keeps_failed_live_source_closed_and_other_source_healthy() {
    run_factory_scenario("two-sources-fatal").await;
}
#[tokio::test]
async fn native_failed_source_closes_while_independent_source_keeps_real_prompting() {
    run_factory_scenario("two-sources").await;
}
#[tokio::test]
async fn native_unknown_notification_does_not_replace_current_state_or_block_next_prompt() {
    run_factory_scenario("unknown-notification").await;
}
#[tokio::test]
async fn native_factory_real_child_load_prompt_terminal_close() {
    run_factory_scenario("success").await;
}
#[tokio::test]
async fn native_bridge_first_open_without_input_then_dispatch_uses_same_real_resource() {
    run_factory_scenario("read-first").await;
}
#[tokio::test]
async fn native_bridge_resource_release_denies_display_only_for_original_target() {
    run_factory_scenario("bridge-target-release").await;
}
#[tokio::test]
async fn native_factory_real_unknown_permission_shutdown_controls() {
    run_factory_scenario("permission").await;
}
async fn run_factory_scenario(scenario: &str) {
    run_factory_with_launcher(scenario, Arc::new(WindowsSourceLauncher), None).await;
}
#[tokio::test]
async fn native_factory_real_known_zero_retries_same_selected_owner() {
    let gate = Arc::new(PromptGate::default());
    run_factory_with_launcher("zero", Arc::new(GatedLauncher(gate.clone())), Some(gate)).await;
}
#[tokio::test]
async fn native_factory_real_shared_target_close_keeps_other_session_usable() {
    run_factory_scenario("shared").await;
}
#[tokio::test]
async fn native_factory_real_v1_v2_stopped_keeps_three_paused_until_explicit_resume() {
    for version in ["v1", "v2"] {
        for reason in ["max_tokens", "max_turn_requests", "refusal", "cancelled"] {
            let scenario = format!("stop-{version}-{reason}");
            run_factory_scenario(&scenario).await;
            eprintln!("synthetic Stopped {version}/{reason} completed");
        }
    }
}
#[tokio::test]
async fn native_factory_v2_unknown_insertion_never_settles_or_retries() {
    run_factory_scenario("early-stop-v2-never-insertion").await;
}
#[tokio::test]
async fn native_factory_v2_early_idle_waits_for_late_insertion_without_second_idle() {
    run_factory_scenario("early-stop-v2-max_tokens").await;
}
#[tokio::test]
async fn native_factory_real_priority_only_advances_original_selected_item() {
    for version in ["v1", "v2"] {
        for outcome in ["end_turn", "error", "cancelled"] {
            run_factory_scenario(&format!("priority-{version}-{outcome}")).await;
        }
    }
}
#[tokio::test]
async fn native_factory_release_clears_retired_v2_prompt_association() {
    run_factory_scenario("priority-v2-cancelled-never-insert").await;
}
#[tokio::test]
async fn native_repeated_control_release_does_not_wake_unchanged_target() {
    run_factory_scenario("priority-idempotent").await;
}
#[tokio::test]
async fn native_factory_first_shutdown_joins_worker_paused_before_real_sql() {
    run_factory_scenario("worker-stop").await;
}
#[tokio::test]
async fn native_factory_shutdown_caller_drop_keeps_original_cleanup_owner() {
    run_factory_scenario("caller-drop").await;
}
#[tokio::test]
async fn native_factory_edit_retires_old_unselected_body_before_actual_poll() {
    run_factory_scenario("edit-prepared").await;
}
#[tokio::test]
async fn native_factory_shared_live_permission_release_keeps_other_target() {
    run_factory_scenario("shared-live").await;
}
#[tokio::test]
async fn native_factory_replay_snapshots_do_not_end_live_work() {
    run_factory_scenario("replay-live").await;
}
#[tokio::test]
async fn native_factory_safe_control_zero_retries_original_request_once() {
    let gate = Arc::new(PromptGate::default());
    gate.control.store(true, Ordering::SeqCst);
    run_factory_with_launcher(
        "control-zero",
        Arc::new(GatedLauncher(gate.clone())),
        Some(gate),
    )
    .await;
}
#[tokio::test]
async fn native_factory_control_deadline_zero_accepts_fresh_attempt_budget() {
    let gate = Arc::new(PromptGate::default());
    gate.control.store(true, Ordering::SeqCst);
    run_factory_with_launcher(
        "control-deadline",
        Arc::new(GatedLauncher(gate.clone())),
        Some(gate),
    )
    .await;
}
#[tokio::test]
async fn native_factory_owned_before_io_failure_cleans_real_job_without_scope() {
    run_factory_with_launcher("take-io-failure", Arc::new(FailingIoLauncher), None).await;
}
#[tokio::test]
async fn native_storage_close_runs_safe_controls_before_original_io_stop() {
    run_factory_scenario("storage-close-permission").await;
}
#[tokio::test]
async fn native_source_closure_reconciles_original_written_transport_before_closed_sql() {
    for scenario in ["unknown-shutdown-v1", "unknown-shutdown-v2"] {
        run_factory_scenario(scenario).await;
    }
}
#[tokio::test]
async fn native_source_closing_cancels_original_pre_and_unbound_permissions_without_run() {
    for scenario in ["pre-permission", "unbound-permission"] {
        run_factory_scenario(scenario).await;
    }
}
#[tokio::test]
async fn native_factory_actual_create_rejection_closes_original_pre_source_without_scope() {
    run_factory_scenario("create-missing").await;
}
#[tokio::test]
async fn native_factory_tracked_pre_and_post_create_failure_matrix_keeps_original_owners() {
    use crate::platform::process_owner::{TrackedSpawnCut as Cut, TrackedSpawnHooks};
    for (scenario, cut) in [
        ("launch-first-pipe", Cut::AfterFirstPipe),
        ("launch-before-create", Cut::BeforeCreateCall),
        ("launch-after-create", Cut::AfterCreateBeforeAssign),
        ("launch-after-assign", Cut::AfterAssignBeforeResume),
    ] {
        eprintln!("synthetic tracked factory cut: {scenario}");
        run_factory_with_launcher(
            scenario,
            Arc::new(FaultLauncher(TrackedSpawnHooks {
                cut: Some(cut),
                before_create: None,
            })),
            None,
        )
        .await;
    }
}
#[expect(
    clippy::too_many_lines,
    reason = "This test-only fixture shares one real SQLite/R3/ACP/Windows composition across the explicitly named developer scenarios."
)]
async fn run_factory_with_launcher<L>(
    scenario: &str,
    launcher: Arc<L>,
    gate: Option<Arc<PromptGate>>,
) where
    L: SourceLaunchPort<Plan = WindowsLaunchPlan>,
    L::Owned: OwnedNativeSource<Streams = AcpStreams>,
{
    let check_idempotent_release = scenario == "priority-idempotent";
    let scenario = if check_idempotent_release {
        "priority-v1-end_turn"
    } else {
        scenario
    };
    let bridge_target_release = scenario == "bridge-target-release";
    let scenario = if bridge_target_release {
        "shared-live"
    } else {
        scenario
    };
    let child = crate::test_support::native_dispatch_dev_child(&["NATIVE_DISPATCH_DEV_CHILD"]);
    assert!(child.is_file());
    let area = std::env::temp_dir().join(format!(
        "acpr-native-dev-{}-{}",
        std::process::id(),
        random_id().unwrap().encode()
    ));
    std::fs::create_dir_all(&area).unwrap();
    let area = Arc::new(NativeArea { directory: area });
    let database = area.join("node.sqlite3");
    let executable = if matches!(scenario, "create-missing" | "cause-missing-exe") {
        area.join("missing-native.exe")
    } else {
        child.clone()
    };
    assert!(!database.exists());
    let storage_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&database, area.clone());
    let storage = storage_scope.open().await.unwrap();
    storage
        .write_node_meta(&NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "native-dev-synthetic".into(),
            public_key: vec![1, 2, 3],
            key_file_ref: "synthetic-unused".into(),
        })
        .await
        .unwrap();
    sqlx::query("INSERT INTO clients VALUES(?,?,'native dev','trusted',1,0,NULL)")
        .bind(id(10).encode())
        .bind(vec![1u8])
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_sources VALUES(?,'native dev')")
        .bind(id(1).encode())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces VALUES(?,?)")
        .bind(id(2).encode())
        .bind(area.to_str().unwrap())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO harnesses VALUES(?,'native dev',?,?,NULL)")
        .bind(id(3).encode())
        .bind(executable.to_str().unwrap())
        .bind(serde_json::to_string(&[scenario]).unwrap())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_harness_sources VALUES(?,?)")
        .bind(id(3).encode())
        .bind(id(1).encode())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,?)")
        .bind(id(4).encode())
        .bind(id(1).encode())
        .bind("synthetic-native-A")
        .bind(id(2).encode())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    if scenario.starts_with("two-sources") {
        sqlx::query("INSERT INTO product_sources VALUES(?,'synthetic source B')")
            .bind(id(7).encode())
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        sqlx::query("INSERT INTO harnesses VALUES(?,'synthetic B missing exe',?,?,NULL)")
            .bind(id(9).encode())
            .bind(
                if scenario == "two-sources-fatal" {
                    child.clone()
                } else {
                    area.join("missing-source-B.exe")
                }
                .to_str()
                .unwrap(),
            )
            .bind(if scenario == "two-sources-fatal" {
                r#"["fatal"]"#
            } else {
                "[]"
            })
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_harness_sources VALUES(?,?)")
            .bind(id(9).encode())
            .bind(id(7).encode())
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,?)")
            .bind(id(8).encode())
            .bind(id(7).encode())
            .bind("synthetic-native-B")
            .bind(id(2).encode())
            .execute(storage.pool_for_test())
            .await
            .unwrap();
    }
    let config = Arc::new(storage.native_configuration_provider(Arc::new(Configuration)));
    let mut budget = options();
    if scenario == "control-deadline" {
        budget.close_wait = Duration::from_millis(100);
    }
    if scenario == "zero" {
        budget.prompt_write_wait = Duration::from_millis(100);
    }
    let (service, native) = storage
        .start_native_service_with_audit_for_test(
            Authority(storage.pool_for_test().clone()),
            Validator,
            SystemNodeClock,
            Arc::new(Audit),
            Arc::new(AcpNativeAdapter::default()),
            launcher,
            config,
            Arc::new(Retain),
            budget,
        )
        .await
        .unwrap();
    timeout(Duration::from_secs(5), async {
        while native.status() == NativeRuntimeStatus::Starting {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(native.status(), NativeRuntimeStatus::Running);
    if bridge_target_release {
        native
            .install_view_factory(Arc::new(
                crate::adapters::acp::native_views::AcpNativeViewFactory::replay_only_for_test(
                    std::num::NonZeroUsize::new(64).unwrap(),
                    Arc::new(BridgeReadOptions::new()),
                ),
            ))
            .unwrap();
    }
    if scenario.starts_with("cause-") {
        use crate::application::shared_views::budget::ReadReason;
        use crate::domain::shared_view::ViewError;
        if scenario == "cause-ready-metadata" {
            sqlx::query("CREATE TRIGGER cause_loaded_failure BEFORE UPDATE OF lifecycle ON node_native_resource_bindings WHEN NEW.lifecycle='loaded' BEGIN SELECT RAISE(ABORT,'synthetic loaded metadata failure'); END")
                .execute(storage.pool_for_test()).await.unwrap();
        }
        let proof = service
            .prepare_native_read(&id(10), id(4), ReadReason::FirstOpen)
            .await
            .unwrap();
        let acquisition = native
            .acquire_read_source(proof, ReadReason::FirstOpen)
            .unwrap();
        let acquired = timeout(Duration::from_secs(8), acquisition.source_capability())
            .await
            .unwrap();
        let completion = timeout(
            Duration::from_secs(8),
            super::read_source::cause_tests::completed(&acquisition),
        )
        .await
        .unwrap();
        let original =
            completion.expect_err("selected actual failure must finish the managed operation");
        let (acquire_error, capability) = match acquired {
            Ok(capability) => (None, Some(capability)),
            Err(error) => (Some(error), None),
        };
        let activation = acquisition.activation();
        let binding = if let Some(capability) = &capability {
            Some(
                timeout(
                    Duration::from_secs(8),
                    super::read_source::wait_read_binding(capability.clone()),
                )
                .await
                .unwrap()
                .map(|_| ()),
            )
        } else {
            None
        };
        let counts: (i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM node_request_registrations),(SELECT count(*) FROM node_native_dispatches)").fetch_one(storage.pool_for_test()).await.unwrap();
        if scenario == "cause-ready-metadata" {
            sqlx::query("DROP TRIGGER cause_loaded_failure")
                .execute(storage.pool_for_test())
                .await
                .unwrap();
        }
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(8))
            .await;
        let released = capability
            .as_ref()
            .map(|capability| capability.require_current());
        drop(capability);
        drop(acquisition);
        timeout(Duration::from_secs(8), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        assert_eq!(counts, (0, 0), "read failure must not mint D or a Run");
        if scenario == "cause-missing-exe" {
            assert!(matches!(
                original,
                DispatchError::SourcePlatform {
                    operation: "CreateProcessW",
                    code: 2
                }
            ));
            assert_eq!(acquire_error, Some(ViewError::NativeFailure(original)));
            assert!(binding.is_none());
        } else {
            assert!(acquire_error.is_none());
            assert_eq!(released, Some(Err(ViewError::SourceReleased)));
            if scenario == "cause-resume-error" {
                assert_eq!(original, DispatchError::ProtocolRejected);
                assert_eq!(
                    activation,
                    Ok(super::read_source::ReadActivationSnapshot::Failed(original))
                );
                assert_eq!(binding, Some(Err(ViewError::NativeFailure(original))));
            } else {
                assert_eq!(original, DispatchError::StorageUnavailable);
                assert_eq!(
                    activation,
                    Ok(super::read_source::ReadActivationSnapshot::Ready)
                );
                assert_eq!(
                    binding,
                    Some(Ok(())),
                    "real matching Resumed wins over later metadata failure"
                );
            }
        }
        return;
    }
    let mut bridge_attachment_weak = None;
    if scenario == "read-first" {
        use crate::application::shared_views::budget::ReadReason;
        let read_options = Arc::new(BridgeReadOptions::new());
        native
            .install_view_factory(Arc::new(
                crate::adapters::acp::native_views::AcpNativeViewFactory::replay_only_for_test(
                    std::num::NonZeroUsize::new(64).unwrap(),
                    read_options.clone(),
                ),
            ))
            .unwrap();
        let proof = service
            .prepare_native_read(&id(10), id(4), ReadReason::FirstOpen)
            .await
            .unwrap();
        let original_owner = proof.owner();
        let acquisition = native
            .acquire_read_source(proof, ReadReason::FirstOpen)
            .unwrap();
        let capability = timeout(Duration::from_secs(8), acquisition.source_capability())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            capability.pre_resource.as_ref().unwrap().metadata.owner,
            original_owner
        );
        assert_eq!(
            capability.scope,
            native.core.protocol.source_scope(&capability.source)
        );
        let binding = timeout(Duration::from_secs(8), acquisition.replay_binding())
            .await
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(&binding.capability, &capability));
        assert_eq!(
            acquisition.activation().unwrap(),
            super::read_source::ReadActivationSnapshot::Ready
        );
        let attachment = capability.source.views().unwrap();
        bridge_attachment_weak = Some(Arc::downgrade(&attachment));
        let views = attachment.views().unwrap();
        timeout(Duration::from_secs(5), async {
            loop {
                let changed = views.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if views.core.probe.processed_frontier() == 2
                    && views.core.probe.processed_route_version()
                        == views.core.probe.route_version()
                {
                    break;
                }
                changed.await;
            }
        })
        .await
        .unwrap();
        assert_eq!(views.core.probe.retained_frontier(), 2);
        assert!(
            views
                .core
                .target(capability.pre_resource.as_ref().unwrap().native())
                .is_ok()
        );
        let empty = service
            .inspect_queue(&id(10), id(4), |view| {
                let queue = match view {
                    QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                };
                queue.active().is_none() && queue.waiting().next().is_none()
            })
            .await
            .unwrap();
        assert!(empty);
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM node_request_registrations),(SELECT count(*) FROM node_native_dispatches)"
        ).fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(counts, (0, 0));
        let source = {
            let sources = native.core.sources.lock().unwrap();
            assert_eq!(sources.len(), 1);
            assert_eq!(sources[0].all_resources.lock().unwrap().len(), 1);
            assert!(Arc::ptr_eq(
                sources[0].all_resources.lock().unwrap()[0]
                    .read_capability
                    .get()
                    .unwrap(),
                &capability
            ));
            sources[0].clone()
        };
        let before_jobs = source.jobs.registered_for_test().unwrap();
        let control = attachment.read_control().unwrap();
        let first = control
            .request(capability.native(), ReadReason::FirstOpen)
            .unwrap();
        let joined = control
            .request(capability.native(), ReadReason::FirstOpen)
            .unwrap();
        let first_status =
            bridge_wait_read(&first, crate::domain::shared_view::ReadMode::ReplayComplete).await;
        assert_eq!(
            control.status(&joined).operation_id(),
            first_status.operation_id()
        );
        assert_eq!(read_options.calls.load(Ordering::Acquire), 1);
        let second = control
            .request(capability.native(), ReadReason::ExplicitRefresh)
            .unwrap();
        let second_status = bridge_wait_read(
            &second,
            crate::domain::shared_view::ReadMode::ReplayComplete,
        )
        .await;
        assert_ne!(first_status.read_epoch(), second_status.read_epoch());
        assert!(
            second_status.selected_options().unwrap().write_deadline()
                > first_status.selected_options().unwrap().write_deadline()
        );
        assert_eq!(
            std::fs::read_to_string(area.join("read-first-replay-count")).unwrap(),
            "2"
        );
        read_options.fail_next.store(true, Ordering::Release);
        let failed = control
            .request(capability.native(), ReadReason::ExplicitRefresh)
            .unwrap();
        let failed_status = bridge_wait_read(
            &failed,
            crate::domain::shared_view::ReadMode::ReplayIncomplete,
        )
        .await;
        assert_eq!(
            failed_status.failure(),
            Some(crate::domain::shared_view::ViewError::AllocationFailure)
        );
        assert_eq!(
            std::fs::read_to_string(area.join("read-first-replay-count")).unwrap(),
            "2"
        );
        let after_failure = control
            .request(capability.native(), ReadReason::ExplicitRefresh)
            .unwrap();
        let recovered_status = bridge_wait_read(
            &after_failure,
            crate::domain::shared_view::ReadMode::ReplayComplete,
        )
        .await;
        assert_ne!(
            recovered_status.operation_id(),
            failed_status.operation_id()
        );
        assert_eq!(
            std::fs::read_to_string(area.join("read-first-replay-count")).unwrap(),
            "3"
        );
        assert_eq!(read_options.calls.load(Ordering::Acquire), 4);
        assert_eq!(source.jobs.registered_for_test().unwrap(), before_jobs + 3);
        let current = timeout(Duration::from_secs(5), async {
            loop {
                let changed = views.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let generation = views.core.target(capability.native()).unwrap();
                let snapshot = generation.snapshot.read().unwrap().clone();
                if snapshot.descriptor.read_epoch() == recovered_status.read_epoch() {
                    break snapshot;
                }
                changed.await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            current.projection.records.len(),
            2,
            "each real read rebuilds only its own reliable receipt window"
        );
        let unknown = current.projection.records.get(1).unwrap();
        let expected = br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"synthetic-native-A","update":{"sessionUpdate":"_future_bridge_event","data":{"z":null,"a":1234567890123456789012345678901234567890,"nested":[true,{"k":"kept"}]}}}}"#;
        assert_eq!(unknown.raw.bytes(), expected);
        assert!(unknown.sequence > current.projection.records.get(0).unwrap().sequence);
        assert_eq!(native.core.sources.lock().unwrap().len(), 1);
        assert_eq!(source.all_resources.lock().unwrap().len(), 1);
    }
    if scenario.starts_with("unknown-shutdown-") {
        sqlx::query("CREATE TRIGGER dev_fail_native_transport BEFORE UPDATE OF transport_state ON node_native_write_attempts BEGIN SELECT RAISE(ABORT,'synthetic transport record failure'); END")
            .execute(storage.pool_for_test()).await.unwrap();
    }
    let edit_cut = if scenario == "edit-prepared" {
        Some(storage.request_registry().native_metadata_hooks_for_test().pause_next(crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::DispatchReserveBeforeBegin).unwrap())
    } else {
        None
    };
    let mutation = ValidatedMutation::Submit(
        service
            .validate_body(r#"[{"type":"text","text":"synthetic native developer input"}]"#)
            .unwrap(),
    );
    let registered = service.register(&id(10), id(4), &mutation).await.unwrap();
    assert!(matches!(
        service
            .consume(&id(10), id(4), registered.id(), mutation)
            .await,
        ConsumeReply::Resolved {
            fact: RequestFact::Settled { .. },
            ..
        }
    ));
    if scenario.starts_with("two-sources") {
        timeout(Duration::from_secs(6),async {loop {
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE close_reason='native_settled'").fetch_one(storage.pool_for_test()).await.unwrap();
            let idle=service.inspect_queue(&id(10),id(4),|view|match view {QueueRead::Stable{queue}|QueueRead::Processing{queue,..}=>queue.active().is_none()}).await.unwrap();
            if count==1&&idle {break;}tokio::task::yield_now().await;
        }}).await.unwrap();
        let source_a = native
            .core
            .sources
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.pre.metadata.source_id == id(1))
            .cloned()
            .unwrap();
        let parent_cut = if scenario == "two-sources-fatal" {
            Some(storage.request_registry().native_metadata_hooks_for_test().pause_next(crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::DispatchReserveBeforeBegin).unwrap())
        } else {
            None
        };
        let mutation = ValidatedMutation::Submit(
            service
                .validate_body(r#"[{"type":"text","text":"synthetic B retains original body"}]"#)
                .unwrap(),
        );
        let record = service.register(&id(10), id(8), &mutation).await.unwrap();
        let original_b = record.id().entity();
        assert!(matches!(
            service.consume(&id(10), id(8), record.id(), mutation).await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        if let Some(cut) = parent_cut {
            timeout(Duration::from_secs(5), cut.wait_until_entered())
                .await
                .unwrap();
            let source_b = native
                .core
                .sources
                .lock()
                .unwrap()
                .iter()
                .find(|source| source.pre.metadata.source_id == id(7))
                .cloned()
                .unwrap();
            let original_close = source_b.close.lock().await;
            cut.release();
            timeout(Duration::from_secs(5), async {
                loop {
                    if source_b.pre.authority.closed.load(Ordering::Acquire) {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(source_b.physical_closed.lock().unwrap().is_none());
            assert!(source_b.closure.lock().unwrap().is_none());
            let closed:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_source_instances WHERE source_id=? AND lifecycle='closed'").bind(id(7).encode()).fetch_one(storage.pool_for_test()).await.unwrap();
            assert_eq!(
                closed, 0,
                "metadata cannot overtake the original cleanup blocked on its actual lock"
            );
            assert!(source_b.tasks.quiet_flags().is_err());
            drop(original_close);
        }
        let source_b = timeout(Duration::from_secs(6), async {
            loop {
                let source = native
                    .core
                    .sources
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|s| s.pre.metadata.source_id == id(7))
                    .cloned();
                if let Some(source) = source {
                    break source;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        native
            .core
            .wait_source_metadata_closed(&source_b, Instant::now() + Duration::from_secs(6))
            .await
            .unwrap();
        assert!(
            source_b
                .metadata_closer
                .get()
                .unwrap()
                .join(Instant::now() + Duration::from_secs(6))
                .await
        );
        assert!(source_b.physical_closed.lock().unwrap().is_some());
        assert_eq!(
            source_b.memory.lock().unwrap().phase,
            super::runtime::SourcePhase::Closed
        );
        assert!(source_a.physical_closed.lock().unwrap().is_none());
        assert_eq!(native.status(), NativeRuntimeStatus::Running);
        let b_row:(String,String,Option<i64>,Option<String>)=sqlx::query_as("SELECT lifecycle,close_basis,close_job_active,scope_binding_id FROM node_native_source_instances WHERE source_id=?").bind(id(7).encode()).fetch_one(storage.pool_for_test()).await.unwrap();
        if scenario == "two-sources-fatal" {
            assert_eq!(
                (&b_row.0, &b_row.1, b_row.2),
                (&"closed".to_owned(), &"owned_converged".to_owned(), Some(0))
            );
            assert!(b_row.3.is_some());
        } else {
            assert_eq!(
                b_row,
                ("closed".into(), "entered_never_created".into(), None, None)
            );
        }
        assert!(
            service
                .inspect_queue(&id(10), id(8), |view| match view {
                    QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } =>
                        if scenario == "two-sources-fatal" {
                            queue.active().is_none()
                        } else {
                            queue.waiting().any(|entry| entry.id() == original_b)
                                || queue.active().is_some_and(|entry| entry.id() == original_b)
                        },
                })
                .await
                .unwrap()
        );
        assert!(native.ensure_target(id(8)).await.is_err());
        assert_eq!(native.core.sources.lock().unwrap().len(), 2);
        let next = ValidatedMutation::Submit(
            service
                .validate_body(r#"[{"type":"text","text":"synthetic A after B source closed"}]"#)
                .unwrap(),
        );
        let record = service.register(&id(10), id(4), &next).await.unwrap();
        assert!(matches!(
            service.consume(&id(10), id(4), record.id(), next).await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        timeout(Duration::from_secs(6),async {loop {
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE close_reason='native_settled'").fetch_one(storage.pool_for_test()).await.unwrap();
            if count==2 {break;}tokio::task::yield_now().await;
        }}).await.unwrap();
        assert!(source_a.physical_closed.lock().unwrap().is_none());
        assert!(source_b.metadata_closer.get().unwrap().confirmed_joined());
        let b_effects: i64 =
            sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE instance_id=?")
                .bind(source_b.pre.metadata.instance_id.encode())
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
        assert_eq!(b_effects, i64::from(scenario == "two-sources-fatal"));
        if scenario == "two-sources-fatal" {
            let closed: String = sqlx::query_scalar(
                "SELECT close_reason FROM node_native_dispatches WHERE instance_id=?",
            )
            .bind(source_b.pre.metadata.instance_id.encode())
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
            assert_eq!(closed, "resources_converged");
            let fabricated:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatch_facts WHERE target_id=? AND result_kind IS NOT NULL").bind(id(8).encode()).fetch_one(storage.pool_for_test()).await.unwrap();
            assert_eq!(
                fabricated, 0,
                "source cleanup cannot manufacture a native terminal result"
            );
        }
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(8))
            .await;
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        timeout(Duration::from_secs(8), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        return;
    }
    let mut edited_request = None;
    if scenario == "create-missing" || scenario.starts_with("launch-") {
        let source = timeout(Duration::from_secs(8), async {
            loop {
                let source = native.core.sources.lock().unwrap().first().cloned();
                if let Some(source) = source
                    && source.physical_closed.lock().unwrap().is_some()
                {
                    break source;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        if scenario == "create-missing" {
            assert!(matches!(
                source.memory.lock().unwrap().error,
                Some(DispatchError::SourcePlatform {
                    operation: "CreateProcessW",
                    code: 2
                })
            ));
        } else {
            assert!(source.memory.lock().unwrap().error.is_some());
        }
        assert!(matches!(
            source.launch_outcome.lock().unwrap().as_ref(),
            Some(super::source_ports::LaunchOutcome::Failed)
        ));
        assert!(source.spawned_record.lock().unwrap().is_none());
        let original = source.owned.lock().unwrap().clone().unwrap();
        assert!(source.source.get().is_none() && source.io.lock().unwrap().is_none());
        assert!(native.ensure_target(id(4)).await.is_err());
        assert!(native.ensure_target(id(4)).await.is_err());
        assert_eq!(native.core.sources.lock().unwrap().len(), 1);
        assert!(Arc::ptr_eq(
            &original,
            source.owned.lock().unwrap().as_ref().unwrap()
        ));
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(8))
            .await;
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        let row:(String,String,String,String,Option<i64>,Option<String>,Option<String>)=sqlx::query_as("SELECT lifecycle,close_basis,close_stage,close_owner_evidence,close_job_active,scope_binding_id,initialize_rpc_kind FROM node_native_source_instances")
            .fetch_one(storage.pool_for_test()).await.unwrap();
        let (basis, stage, owner, job, resource_stage) = match scenario {
            "create-missing" => (
                "entered_never_created",
                "create_rejected",
                "no_process_created",
                None,
                "create_rejected",
            ),
            "launch-first-pipe" | "launch-before-create" => (
                "entered_never_created",
                "entered_before_create",
                "no_process_created",
                None,
                "entered_before_create",
            ),
            "launch-after-create" => (
                "owned_converged",
                "owned_before_io",
                "suspended_root_exited",
                None,
                "reserved_no_target_io",
            ),
            "launch-after-assign" => (
                "owned_converged",
                "owned_before_io",
                "windows_job",
                Some(0),
                "reserved_no_target_io",
            ),
            _ => unreachable!(),
        };
        assert_eq!(
            row,
            (
                "closed".into(),
                basis.into(),
                stage.into(),
                owner.into(),
                job,
                None,
                None
            )
        );
        let resources:Vec<ReleasedResourceRow>=sqlx::query_as("SELECT lifecycle,release_basis,release_stage,scope_binding_id,load_rpc_kind FROM node_native_resource_bindings")
            .fetch_all(storage.pool_for_test()).await.unwrap();
        assert_eq!(
            resources,
            vec![(
                "released".into(),
                basis.into(),
                resource_stage.into(),
                None,
                None
            )]
        );
        let effects:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM node_native_dispatches)+(SELECT count(*) FROM node_native_observations)+(SELECT count(*) FROM node_native_convergences)+(SELECT count(*) FROM node_native_control_facts)+(SELECT count(*) FROM node_native_source_losses)")
            .fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(
            effects,
            i64::from(scenario == "launch-after-assign"),
            "only the original actually assigned Job may add its real source Convergence; no protocol effects/Scope/fake Job0"
        );
        assert!(source.memory.lock().unwrap().scope.is_none());
        let repeated = native
            .shutdown(Instant::now() + Duration::from_secs(3))
            .await;
        assert!(repeated.joined && repeated.metadata_confirmed);
        timeout(Duration::from_secs(6), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        return;
    }
    if scenario == "pre-permission" || scenario == "unbound-permission" {
        timeout(Duration::from_secs(5), async {
            while !area.join("initialize-received").is_file() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let init_cut=storage.request_registry().native_metadata_hooks_for_test()
            .pause_next(crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::SourceStateBeforeBegin).unwrap();
        std::fs::write(area.join("release-initialize"), b"synthetic").unwrap();
        timeout(Duration::from_secs(4), init_cut.wait_until_entered())
            .await
            .unwrap();
        let source = timeout(Duration::from_secs(8), async {
            loop {
                let source = native.core.sources.lock().unwrap().first().cloned();
                if let Some(source) = source {
                    let retained = source.permissions.lock().unwrap().len();
                    if area.join("early-permission").is_file()
                        && retained == 1
                        && source.pre.authority.required.load(Ordering::SeqCst) == 0
                    {
                        break source;
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let permission = source
            .permissions
            .lock()
            .unwrap()
            .values()
            .next()
            .cloned()
            .unwrap();
        assert_eq!(
            permission.session.len(),
            if scenario == "unbound-permission" {
                "synthetic-unbound-session".len()
            } else {
                "synthetic-native-A".len()
            }
        );
        assert!(permission.snapshot().unwrap().claimed.is_none());
        assert!(!permission.snapshot().unwrap().written);
        assert!(
            !area.join("permission-cancelled").is_file(),
            "unknown kind/session did not automatically deny or cancel"
        );
        let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches")
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
        assert_eq!(runs, 0);
        let loaded:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_resource_bindings WHERE lifecycle='loaded' OR load_rpc_kind IS NOT NULL").fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(loaded, 0);
        let closer = native.clone();
        let closing = tokio::spawn(async move {
            closer
                .shutdown(Instant::now() + Duration::from_secs(8))
                .await
        });
        timeout(Duration::from_secs(4), async {
            while !area.join("permission-cancelled").is_file() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        init_cut.release();
        let report = timeout(Duration::from_secs(8), closing)
            .await
            .unwrap()
            .unwrap();
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        assert!(
            area.join("permission-cancelled").is_file(),
            "same original request was actually Cancelled at the peer"
        );
        let controls:Vec<(String,Option<String>,String)>=sqlx::query_as("SELECT control_kind,target_id,transport_state FROM node_native_control_facts WHERE phase='terminal'")
            .fetch_all(storage.pool_for_test()).await.unwrap();
        assert_eq!(
            controls,
            vec![(
                "permission_cancelled".into(),
                if scenario == "pre-permission" {
                    Some(id(4).encode())
                } else {
                    None
                },
                "written".into()
            )]
        );
        assert!(permission.snapshot().unwrap().retired);
        timeout(Duration::from_secs(6), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        return;
    }
    if scenario.starts_with("unknown-shutdown-") {
        timeout(Duration::from_secs(8), async {
            loop {
                let runs = native
                    .core
                    .runs
                    .lock()
                    .unwrap()
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                if area.join("unknown-prompt").is_file()
                    && runs.first().is_some_and(|run| {
                        run.transport_for_test().unwrap()
                            == Some(TransportProgress::Finished(TransportFact::Written))
                    })
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        // Drain the original worker's failed metadata attempt before removing the real trigger.
        let run = native
            .core
            .runs
            .lock()
            .unwrap()
            .values()
            .next()
            .cloned()
            .unwrap();
        let action = run.action_for_resource_close().await;
        let armed: String =
            sqlx::query_scalar("SELECT transport_state FROM node_native_write_attempts")
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
        assert_eq!(armed, "armed");
        sqlx::query("DROP TRIGGER dev_fail_native_transport")
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        drop(action);
        let cut = storage.request_registry().native_metadata_hooks_for_test()
            .pause_next(crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::SourceClosedBeforeBegin).unwrap();
        let closing = native.clone();
        let task = tokio::spawn(async move {
            closing
                .shutdown(Instant::now() + Duration::from_secs(8))
                .await
        });
        timeout(Duration::from_secs(6), cut.wait_until_entered())
            .await
            .unwrap();
        let current:(String,String)=sqlx::query_as("SELECT s.lifecycle,w.transport_state FROM node_native_source_instances s JOIN node_native_dispatches d ON d.instance_id=s.instance_id JOIN node_native_write_attempts w ON w.dispatch_id=d.dispatch_id")
            .fetch_one(storage.pool_for_test()).await.unwrap();
        assert_ne!(current.0, "closed");
        assert_eq!(
            current.1, "written",
            "original actual Ticket must be reliably reconciled BEFORE SourceClosed BEGIN"
        );
        let results: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM node_native_dispatch_facts WHERE result_kind IS NOT NULL",
        )
        .fetch_one(storage.pool_for_test())
        .await
        .unwrap();
        assert_eq!(
            results, 0,
            "physical retirement cannot manufacture native completion"
        );
        cut.release();
        let report = timeout(Duration::from_secs(8), task)
            .await
            .unwrap()
            .unwrap();
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        let closed: Vec<(String, String)> =
            sqlx::query_as("SELECT lifecycle,close_reason FROM node_native_dispatches")
                .fetch_all(storage.pool_for_test())
                .await
                .unwrap();
        assert_eq!(
            closed,
            vec![("closed".into(), "resources_converged".into())]
        );
        assert!(
            service
                .inspect_queue(&id(10), id(4), |view| {
                    let queue = match view {
                        QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                    };
                    queue.active().is_none()
                })
                .await
                .unwrap()
        );
        timeout(Duration::from_secs(6), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        return;
    }
    if let Some(cut) = edit_cut {
        timeout(Duration::from_secs(8), cut.wait_until_entered())
            .await
            .unwrap();
        let mutation=ValidatedMutation::Edit{item:registered.id().entity(),revision:1,body:service.validate_body(r#"[{"type":"text","text":"synthetic edited body","_future":{"nested":[1,null,{"x":"kept"}]}}]"#).unwrap()};
        let edit = service.register(&id(10), id(4), &mutation).await.unwrap();
        assert!(matches!(
            service.consume(&id(10), id(4), edit.id(), mutation).await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        edited_request = Some(edit.id());
        cut.release();
    }
    if scenario == "take-io-failure" {
        timeout(Duration::from_secs(8), async {
            loop {
                let sources = native.core.sources.lock().unwrap().clone();
                if sources.len() == 1 && sources[0].physical_closed.lock().unwrap().is_some() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let stage:(String,Option<String>,Option<String>)=sqlx::query_as("SELECT lifecycle,scope_binding_id,initialize_rpc_kind FROM node_native_source_instances").fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(stage, ("unknown".into(), None, None));
        let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches")
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
        assert_eq!(runs, 0);
        assert!(native.ensure_target(id(4)).await.is_err());
        let sources: i64 = sqlx::query_scalar("SELECT count(*) FROM node_native_source_instances")
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
        assert_eq!(
            sources, 1,
            "failure cannot silently open a replacement source"
        );
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(8))
            .await;
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        let closed:(String,String,i64,String,Option<String>)=sqlx::query_as("SELECT close_stage,close_io_evidence,close_job_active,lifecycle,scope_binding_id FROM node_native_source_instances").fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(
            closed,
            (
                "owned_before_io".into(),
                "not_prepared".into(),
                0,
                "closed".into(),
                None
            )
        );
        timeout(Duration::from_secs(6), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        return;
    }
    if scenario == "control-zero" || scenario == "control-deadline" {
        let gate = gate.as_ref().unwrap();
        let (source, operation) = timeout(Duration::from_secs(4), async {
            loop {
                let source = native.core.sources.lock().unwrap().first().cloned();
                if let Some(source) = source {
                    let control = source.controls.lock().unwrap().first().cloned();
                    if gate.polled.load(Ordering::SeqCst) > 0
                        && let Some(control) = control
                    {
                        break (source, control);
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(gate.accepted.load(Ordering::SeqCst), 0);
        if scenario == "control-zero" {
            operation.cancel_write_for_test().unwrap();
        }
        timeout(Duration::from_secs(3), async {
            loop {
                let (progress, stored) = operation.transport_for_test().unwrap();
                if stored
                    && matches!(
                        progress,
                        Some(TransportProgress::Finished(TransportFact::Zero { .. }))
                    )
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        if scenario == "control-deadline" {
            assert!(matches!(
                operation.transport_for_test().unwrap().0,
                Some(TransportProgress::Finished(TransportFact::Zero {
                    reason: TransportReason::Deadline
                }))
            ));
        }
        assert_eq!(
            source.pre.authority.required.load(Ordering::SeqCst),
            1,
            "actual Zero does not discharge original unsupported obligation"
        );
        assert!(!area.join("unsupported-written").is_file());
        let cut=storage.request_registry().native_metadata_hooks_for_test().pause_next(crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::ControlTerminalBeforeBegin).unwrap();
        gate.open.store(true, Ordering::SeqCst);
        let next = native
            .retry_zero_control(
                operation.id_for_test(),
                Instant::now() + Duration::from_secs(4),
            )
            .await
            .unwrap();
        assert_ne!(next, operation.id_for_test());
        timeout(Duration::from_secs(3), cut.wait_until_entered())
            .await
            .unwrap();
        timeout(Duration::from_secs(4), async {
            while !area.join("unsupported-written").is_file() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(source.pre.authority.required.load(Ordering::SeqCst), 1);
        assert!(
            !area.join("control-prompt").is_file(),
            "actual reply Written cannot bypass its necessary SQL"
        );
        cut.release();
        timeout(Duration::from_secs(3), async {
            loop {
                let count: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM node_native_control_facts WHERE phase='terminal'",
                )
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
                if count == 2 && source.pre.authority.required.load(Ordering::SeqCst) == 0 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let facts:Vec<(Vec<u8>,String)>=sqlx::query_as("SELECT attempt_serial,transport_state FROM node_native_control_facts WHERE phase='terminal' ORDER BY attempt_serial").fetch_all(storage.pool_for_test()).await.unwrap();
        assert_eq!(
            facts,
            vec![
                (0_u64.to_be_bytes().to_vec(), "zero".into()),
                (1_u64.to_be_bytes().to_vec(), "written".into())
            ]
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT obligation_id) FROM node_native_control_facts",
        )
        .fetch_one(storage.pool_for_test())
        .await
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(source.pre.authority.required.load(Ordering::SeqCst), 0);
    }
    if scenario == "replay-live" {
        timeout(Duration::from_secs(8), async {
            loop {
                let count: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM node_native_dispatch_facts WHERE fact_kind='v2_inserted'",
                )
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
                if count == 1 && area.join("live-prompt-ready").is_file() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let source = native.core.sources.lock().unwrap()[0].clone();
        let resource = source
            .targets
            .lock()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .clone();
        let (interval, attempt) = {
            let facts = resource.target.facts.lock().unwrap();
            assert_eq!(
                facts.foreground,
                crate::domain::activity::WorkActivity::Active
            );
            assert!(facts.interval_live);
            (
                facts.interval,
                facts.pending_input.as_ref().unwrap().attempt,
            )
        };
        let connection = source.source.get().unwrap().clone();
        let setup = resource.setup.get().unwrap().clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        native
            .core
            .tasks
            .spawn(async move {
                let ticket = connection
                    .replay_for_test(
                        "synthetic-native-A",
                        &setup,
                        Instant::now() + Duration::from_secs(5),
                    )
                    .unwrap();
                let result = ticket.wait().await;
                let _ = tx.send(result);
            })
            .unwrap();
        assert_eq!(
            timeout(Duration::from_secs(6), rx).await.unwrap().unwrap(),
            WriteReport::Written
        );
        timeout(Duration::from_secs(8),async{loop{let snapshots:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatch_facts WHERE transition_kind='snapshot' AND fact_kind='foreground_idle'").fetch_one(storage.pool_for_test()).await.unwrap();let clean={let facts=resource.target.facts.lock().unwrap();facts.requirements.is_empty()&&facts.failed_requirements.is_empty()};if snapshots>=2&&clean&&area.join("replay-sent").is_file(){break;}tokio::task::yield_now().await;}}).await.unwrap();
        {
            let facts = resource.target.facts.lock().unwrap();
            assert_eq!(
                facts.foreground,
                crate::domain::activity::WorkActivity::Active
            );
            assert_eq!(facts.interval, interval);
            assert!(facts.interval_live && facts.interval_open);
            assert_eq!(facts.terminal_sequence, 0);
            assert_eq!(facts.pending_input.as_ref().unwrap().attempt, attempt);
        }
        let terminals:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatch_facts WHERE result_kind IS NOT NULL OR fact_kind='foreground_terminal'").fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(terminals, 0);
        let active = service
            .inspect_queue(&id(10), id(4), |view| {
                let q = match view {
                    QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                };
                q.active().is_some()
            })
            .await
            .unwrap();
        assert!(active);
        let new = ValidatedMutation::Submit(
            service
                .validate_body(r#"[{"type":"text","text":"synthetic after old snapshot"}]"#)
                .unwrap(),
        );
        let new_registration = service.register(&id(10), id(4), &new).await.unwrap();
        assert!(matches!(
            service
                .consume(&id(10), id(4), new_registration.id(), new)
                .await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        let before_replay_snapshots: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM node_native_dispatch_facts \
             WHERE transition_kind='snapshot' AND fact_kind='foreground_idle'",
        )
        .fetch_one(storage.pool_for_test())
        .await
        .unwrap();
        let connection = source.source.get().unwrap().clone();
        let setup = resource.setup.get().unwrap().clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        native
            .core
            .tasks
            .spawn(async move {
                let ticket = connection
                    .replay_for_test(
                        "synthetic-native-A",
                        &setup,
                        Instant::now() + Duration::from_secs(5),
                    )
                    .unwrap();
                let result = ticket.wait().await;
                let _ = tx.send(result);
            })
            .unwrap();
        assert_eq!(
            timeout(Duration::from_secs(6), rx).await.unwrap().unwrap(),
            WriteReport::Written
        );
        timeout(Duration::from_secs(8), async {
            loop {
                let snapshots: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM node_native_dispatch_facts \
                     WHERE transition_kind='snapshot' AND fact_kind='foreground_idle'",
                )
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
                let clean = {
                    let facts = resource.target.facts.lock().unwrap();
                    facts.requirements.is_empty() && facts.failed_requirements.is_empty()
                };
                if snapshots > before_replay_snapshots && clean {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let (active, waiting) = service
            .inspect_queue(&id(10), id(4), |view| {
                let q = match view {
                    QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                };
                (
                    q.active().is_some(),
                    q.waiting()
                        .map(|entry| (entry.id(), entry.suspended()))
                        .collect::<Vec<_>>(),
                )
            })
            .await
            .unwrap();
        assert!(active);
        assert_eq!(
            waiting,
            vec![(new_registration.id().entity(), false)],
            "old snapshot/repeated idle must not pause an input queued after it"
        );
        {
            let facts = resource.target.facts.lock().unwrap();
            assert_eq!(facts.pending_input.as_ref().unwrap().attempt, attempt);
            assert_eq!(facts.terminal_sequence, 0);
        }
        let terminals: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM node_native_dispatch_facts \
             WHERE result_kind IS NOT NULL OR fact_kind='foreground_terminal'",
        )
        .fetch_one(storage.pool_for_test())
        .await
        .unwrap();
        assert_eq!(terminals, 0, "stale snapshots cannot synthesize a terminal");
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(8))
            .await;
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        timeout(Duration::from_secs(6), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        return;
    }
    if scenario == "shared-live" {
        timeout(Duration::from_secs(8),async{loop{let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatch_facts WHERE fact_kind='approval_present'").fetch_one(storage.pool_for_test()).await.unwrap();if count==1{break;}tokio::task::yield_now().await;}}).await.unwrap();
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,?)")
            .bind(id(5).encode())
            .bind(id(1).encode())
            .bind("synthetic-native-B")
            .bind(id(2).encode())
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        let mutation = ValidatedMutation::Submit(
            service
                .validate_body(
                    r#"[{"type":"text","text":"synthetic B while A has unknown permission"}]"#,
                )
                .unwrap(),
        );
        let record = service.register(&id(10), id(5), &mutation).await.unwrap();
        assert!(matches!(
            service.consume(&id(10), id(5), record.id(), mutation).await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        timeout(Duration::from_secs(8),async{loop{let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE target_id=? AND lifecycle='closed'").bind(id(5).encode()).fetch_one(storage.pool_for_test()).await.unwrap();if count==1{break;}tokio::task::yield_now().await;}}).await.unwrap();
        let display = if bridge_target_release {
            let source = native.core.sources.lock().unwrap()[0]
                .source
                .get()
                .unwrap()
                .clone();
            let attachment = source.views().unwrap();
            let attachment_weak = Arc::downgrade(&attachment);
            let views = attachment.views().unwrap();
            drop(attachment);
            let a = NativeSessionKey::from_config(Some(id(1)), "synthetic-native-A").unwrap();
            let b = NativeSessionKey::from_config(Some(id(1)), "synthetic-native-B").unwrap();
            let generations = timeout(Duration::from_secs(5), async {
                loop {
                    let changed = views.core.changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    if let (Ok(a), Ok(b)) = (views.core.target(&a), views.core.target(&b)) {
                        break (a, b);
                    }
                    changed.await;
                }
            })
            .await
            .unwrap();
            assert!(generations.0.send.current().unwrap() && generations.1.send.current().unwrap());
            Some((views, generations, attachment_weak))
        } else {
            None
        };
        native.release_target(id(4)).await.unwrap();
        if let Some((views, (a, b), _)) = &display {
            assert!(
                !a.send.current().unwrap(),
                "the original resource release denies its display generations"
            );
            assert!(
                b.send.current().unwrap(),
                "other target display remains current on the same source"
            );
            assert!(
                !views.core.probe.is_sealed(),
                "resource release does not close the source display"
            );
        }
        let controls:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_control_facts WHERE target_id=? AND phase='terminal' AND transport_state='written'").bind(id(4).encode()).fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(
            controls, 2,
            "only explicit release safely cancelled A and its original permission"
        );
        let resources: Vec<(String, String)> = sqlx::query_as(
            "SELECT target_id,lifecycle FROM node_native_resource_bindings ORDER BY target_id",
        )
        .fetch_all(storage.pool_for_test())
        .await
        .unwrap();
        assert_eq!(
            resources,
            vec![
                (id(4).encode(), "released".into()),
                (id(5).encode(), "loaded".into())
            ]
        );
        let closed_sources: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM node_native_source_instances WHERE lifecycle='closed'",
        )
        .fetch_one(storage.pool_for_test())
        .await
        .unwrap();
        assert_eq!(closed_sources, 0);
        let mutation = ValidatedMutation::Submit(
            service
                .validate_body(r#"[{"type":"text","text":"synthetic B after A live release"}]"#)
                .unwrap(),
        );
        let record = service.register(&id(10), id(5), &mutation).await.unwrap();
        assert!(matches!(
            service.consume(&id(10), id(5), record.id(), mutation).await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        timeout(Duration::from_secs(8),async{loop{let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE target_id=? AND lifecycle='closed'").bind(id(5).encode()).fetch_one(storage.pool_for_test()).await.unwrap();if count==2{break;}tokio::task::yield_now().await;}}).await.unwrap();
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(8))
            .await;
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        if let Some((_, _, attachment)) = display {
            assert!(
                attachment.upgrade().is_none(),
                "true original join releases the attachment even while snapshots exist"
            );
        }
        timeout(Duration::from_secs(6), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        return;
    }
    if scenario.starts_with("priority") {
        eprintln!("priority scenario: {scenario}");
        let approval = timeout(Duration::from_secs(10), async {
            loop {
                let count: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM node_native_dispatch_facts \
                     WHERE fact_kind='approval_present'",
                )
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
                if count == 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        if approval.is_err() {
            let dispatches: i64 = sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches")
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
            let facts: Vec<String> = sqlx::query_scalar(
                "SELECT fact_kind FROM node_native_dispatch_facts ORDER BY owner_order",
            )
            .fetch_all(storage.pool_for_test())
            .await
            .unwrap();
            panic!(
                "priority scenario {scenario} did not observe approval_present: \
                 dispatches={dispatches}, facts={facts:?}"
            );
        }
        let cancel = ValidatedMutation::<OwnedRawValue>::Cancel {
            item: registered.id().entity(),
            revision: 1,
        };
        let cancelled = service.register(&id(10), id(4), &cancel).await.unwrap();
        assert!(matches!(
            service
                .consume(&id(10), id(4), cancelled.id(), cancel)
                .await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        let controls: i64 = sqlx::query_scalar("SELECT count(*) FROM node_native_control_facts")
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
        assert_eq!(
            controls, 0,
            "ordinary queue Cancel never sends native cancellation"
        );
        let mut old = Vec::new();
        for _ in 0..3 {
            let mutation = ValidatedMutation::Submit(
                service
                    .validate_body(r#"[{"type":"text","text":"synthetic priority waiting"}]"#)
                    .unwrap(),
            );
            let item = service.register(&id(10), id(4), &mutation).await.unwrap();
            assert!(matches!(
                service.consume(&id(10), id(4), item.id(), mutation).await,
                ConsumeReply::Resolved {
                    fact: RequestFact::Settled { .. },
                    ..
                }
            ));
            old.push(item.id().entity());
        }
        let prioritize = ValidatedMutation::<OwnedRawValue>::Prioritize {
            item: old[1],
            revision: 1,
        };
        let selected = service.register(&id(10), id(4), &prioritize).await.unwrap();
        assert!(matches!(
            service
                .consume(&id(10), id(4), selected.id(), prioritize)
                .await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        let selected_y = old[1];
        let remote_error = scenario == "priority-v1-error";
        let priority_observation = timeout(Duration::from_secs(10), async {
            loop {
                let count: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM node_native_dispatches \
                     WHERE lifecycle='closed' AND close_reason='native_settled'",
                )
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
                let queue = service
                    .inspect_queue(&id(10), id(4), |view| {
                        let q = match view {
                            QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => {
                                queue
                            }
                        };
                        (
                            q.active().map(crate::domain::input_queue::QueueEntry::id),
                            q.waiting()
                                .map(|entry| (entry.id(), entry.suspended()))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .await
                    .unwrap();
                let expected_waiting = vec![(old[0], true), (old[2], true)];
                let active_matches = if remote_error {
                    queue.0 == Some(selected_y)
                } else {
                    queue.0.is_none()
                };
                let rejected: i64 = if remote_error {
                    sqlx::query_scalar(
                        "SELECT count(*) FROM node_native_dispatch_facts \
                         WHERE fact_kind='prompt_rejected' AND attempt_id IS NOT NULL",
                    )
                    .fetch_one(storage.pool_for_test())
                    .await
                    .unwrap()
                } else {
                    0
                };
                if active_matches
                    && queue.1 == expected_waiting
                    && (remote_error && rejected == 1 || !remote_error && count == 2)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        if priority_observation.is_err() {
            let queue = service
                .inspect_queue(&id(10), id(4), |view| {
                    let q = match view {
                        QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                    };
                    (
                        q.active().map(|entry| (entry.id(), entry.suspended())),
                        q.waiting()
                            .map(|entry| (entry.id(), entry.suspended()))
                            .collect::<Vec<_>>(),
                        q.priority_failure(),
                        q.last_stop_reason(),
                    )
                })
                .await
                .unwrap();
            let dispatches: Vec<(String, String, Option<String>)> = sqlx::query_as(
                "SELECT item_id,lifecycle,close_reason FROM node_native_dispatches \
                 ORDER BY selection_owner_order",
            )
            .fetch_all(storage.pool_for_test())
            .await
            .unwrap();
            panic!(
                "priority scenario {scenario} did not settle as expected: \
                 active={:?}, waiting={:?}, priority_failure={:?}, stop={:?}, dispatches={dispatches:?}",
                queue.0, queue.1, queue.2, queue.3
            );
        }
        if remote_error {
            let rejected: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM node_native_dispatch_facts \
                 WHERE fact_kind='prompt_rejected' AND attempt_id IS NOT NULL",
            )
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
            assert_eq!(
                rejected, 1,
                "v1 RemoteError remains an unknown native outcome"
            );
            let settled: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM node_native_dispatches \
                 WHERE close_reason='native_settled'",
            )
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
            assert_eq!(
                settled, 1,
                "v1 RemoteError must not claim the selected input settled"
            );
        }
        let order: Vec<String> = sqlx::query_scalar(
            "SELECT item_id FROM node_native_dispatches ORDER BY selection_owner_order",
        )
        .fetch_all(storage.pool_for_test())
        .await
        .unwrap();
        assert_eq!(order, [registered.id().entity().encode(), old[1].encode()]);
        let remaining = service
            .inspect_queue(&id(10), id(4), |view| {
                let q = match view {
                    QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                };
                q.waiting()
                    .map(crate::domain::input_queue::QueueEntry::id)
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap();
        assert_eq!(remaining, [old[0], old[2]]);
        let remaining_suspended = service
            .inspect_queue(&id(10), id(4), |view| {
                let q = match view {
                    QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                };
                q.waiting()
                    .all(crate::domain::input_queue::QueueEntry::suspended)
            })
            .await
            .unwrap();
        assert!(
            remaining_suspended,
            "Y success, failure or cancellation must not resume X/Z"
        );
        if scenario == "priority-v2-cancelled" {
            let insertions: Vec<(String, String)> = sqlx::query_as(
                "SELECT d.item_id, f.fact_kind \
                 FROM node_native_dispatch_facts f \
                 JOIN node_native_write_attempts a USING(attempt_id) \
                 JOIN node_native_dispatches d USING(dispatch_id) \
                 WHERE f.fact_kind='v2_inserted' \
                 ORDER BY d.selection_owner_order",
            )
            .fetch_all(storage.pool_for_test())
            .await
            .unwrap();
            assert_eq!(
                insertions,
                vec![
                    (registered.id().entity().encode(), "v2_inserted".into()),
                    (old[1].encode(), "v2_inserted".into())
                ],
                "a late insertion response must remain associated with the interrupted X, not Y"
            );
        }
        if scenario == "priority-v2-cancelled-never-insert" {
            let resource = native.core.sources.lock().unwrap()[0]
                .all_resources
                .lock()
                .unwrap()[0]
                .clone();
            {
                let facts = resource.target.facts.lock().unwrap();
                assert_eq!(
                    facts.retired_prompts.len(),
                    1,
                    "the interrupted X must retain its attempt association after Y ends"
                );
            }
            native.release_target(id(4)).await.unwrap();
            assert!(
                resource
                    .target
                    .facts
                    .lock()
                    .unwrap()
                    .retired_prompts
                    .is_empty(),
                "resource release must discard dead v2 prompt associations"
            );
            assert!(
                native
                    .core
                    .environment
                    .target(resource.pre.native())
                    .is_none(),
                "a closed target must no longer remain reachable through the environment"
            );
        }
        let controls:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_control_facts WHERE phase='terminal' AND transport_state='written'").fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(controls, 2);
        let repeated_release = if check_idempotent_release {
            let source = native.core.sources.lock().unwrap()[0].clone();
            let resource = source.all_resources.lock().unwrap()[0].clone();
            let operation = source
                .controls
                .lock()
                .unwrap()
                .iter()
                .find(|control| control.kind_for_test() == ControlKind::NativeCancel)
                .cloned()
                .unwrap();
            Some((source, resource, operation))
        } else {
            None
        };
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(8))
            .await;
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        let repeated_observation = repeated_release.map(|(source, resource, operation)| {
            let before = source.pre.authority.required.load(Ordering::SeqCst);
            let changed = resource.target.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            native
                .core
                .repeat_control_release_for_test(&source, &resource, &operation)
                .unwrap();
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            let woke = std::future::Future::poll(changed.as_mut(), &mut context).is_ready();
            let after = source.pre.authority.required.load(Ordering::SeqCst);
            (woke, before, after)
        });
        timeout(Duration::from_secs(8), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        if let Some((woke, before, after)) = repeated_observation {
            assert!(
                !woke,
                "idempotent control cleanup must not notify an unchanged target"
            );
            assert_eq!(
                (before, after),
                (0, 0),
                "idempotent cleanup must not decrement required twice"
            );
        }
        return;
    }
    if scenario.starts_with("early-stop-v2-") {
        eprintln!("early idle test: waiting for first prompt");
        timeout(Duration::from_secs(10), async {
            while !area.join("first-prompt-ready").is_file() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        eprintln!("early idle test: queuing old inputs");
        let mut old = Vec::new();
        for _ in 0..3 {
            let mutation = ValidatedMutation::Submit(
                service
                    .validate_body(r#"[{"type":"text","text":"synthetic old waiting"}]"#)
                    .unwrap(),
            );
            let item = service.register(&id(10), id(4), &mutation).await.unwrap();
            assert!(matches!(
                service.consume(&id(10), id(4), item.id(), mutation).await,
                ConsumeReply::Resolved {
                    fact: RequestFact::Settled { .. },
                    ..
                }
            ));
            old.push(item.id().entity());
        }
        eprintln!("early idle test: releasing first prompt");
        std::fs::write(area.join("release-first-prompt"), b"synthetic").unwrap();
        let source = native.core.sources.lock().unwrap()[0].clone();
        let resource = source
            .targets
            .lock()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .clone();
        let early_observation = timeout(Duration::from_secs(10), async {
            loop {
                let idle_sent = area.join("early-idle-sent").is_file();
                let terminal_facts: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM node_native_dispatch_facts \
                     WHERE fact_kind='foreground_terminal' AND native_stop_reason='max_tokens'",
                )
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
                let paused = service
                    .inspect_queue(&id(10), id(4), |view| {
                        let q = match view {
                            QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => {
                                queue
                            }
                        };
                        q.active().is_some()
                            && q.failure_suspended()
                            && q.last_stop_reason()
                                == Some(crate::domain::native_dispatch::KnownStopReason::MaxTokens)
                            && q.waiting().len() == 3
                            && q.waiting()
                                .all(crate::domain::input_queue::QueueEntry::suspended)
                    })
                    .await
                    .unwrap();
                let clean = {
                    let facts = resource.target.facts.lock().unwrap();
                    facts.requirements.is_empty() && facts.failed_requirements.is_empty()
                };
                if idle_sent && terminal_facts == 1 && paused && clean {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        if early_observation.is_err() {
            let terminal_facts: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM node_native_dispatch_facts \
                 WHERE fact_kind='foreground_terminal' AND native_stop_reason='max_tokens'",
            )
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
            let queue = service
                .inspect_queue(&id(10), id(4), |view| {
                    let q = match view {
                        QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                    };
                    (
                        q.active().is_some(),
                        q.failure_suspended(),
                        q.last_stop_reason(),
                        q.waiting().len(),
                        q.waiting()
                            .all(crate::domain::input_queue::QueueEntry::suspended),
                    )
                })
                .await
                .unwrap();
            panic!(
                "early idle was not observed: marker={}, terminals={terminal_facts}, queue={queue:?}",
                area.join("early-idle-sent").is_file()
            );
        }
        early_observation.unwrap();
        eprintln!("early idle test: early terminal and pause observed");
        {
            let facts = resource.target.facts.lock().unwrap();
            let pending = facts
                .pending_input
                .as_ref()
                .expect("late insertion response must still be pending");
            assert!(!pending.inserted);
            assert_eq!(
                facts.terminal,
                Some(crate::domain::native_dispatch::KnownStopReason::MaxTokens)
            );
            assert!(!facts.interval_open);
            assert!(!facts.interval_live);
            assert!(facts.requirements.is_empty());
            assert!(facts.failed_requirements.is_empty());
        }
        let settled_before: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM node_native_dispatches \
             WHERE lifecycle='closed' AND close_reason='native_settled'",
        )
        .fetch_one(storage.pool_for_test())
        .await
        .unwrap();
        assert_eq!(
            settled_before, 0,
            "idle must not settle the input before its insertion response"
        );
        if scenario == "early-stop-v2-never-insertion" {
            let terminal_attempt: Option<Vec<u8>> = sqlx::query_scalar(
                "SELECT attempt_id FROM node_native_dispatch_facts \
                 WHERE fact_kind='foreground_terminal' \
                   AND native_stop_reason='max_tokens'",
            )
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
            assert_eq!(
                terminal_attempt, None,
                "a live foreground terminal must not be guessed onto the pending Run"
            );
            tokio::time::sleep(Duration::from_millis(150)).await;
            let dispatches: i64 = sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches")
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
            assert_eq!(dispatches, 1, "unknown insertion must not be re-sent");
            assert_eq!(
                std::fs::read_to_string(area.join("prompt-count")).unwrap(),
                "1",
                "the native peer must receive exactly one original prompt"
            );
            let pending = {
                let facts = resource.target.facts.lock().unwrap();
                facts.pending_input.is_some()
            };
            assert!(
                pending,
                "permanent insertion uncertainty must leave the original input pending"
            );
            let active = service
                .inspect_queue(&id(10), id(4), |view| {
                    let q = match view {
                        QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                    };
                    q.active().is_some()
                })
                .await
                .unwrap();
            assert!(
                active,
                "unknown insertion must not release the original queue slot"
            );
            let report = native
                .shutdown(Instant::now() + Duration::from_secs(8))
                .await;
            assert!(report.joined && report.metadata_confirmed, "{report:?}");
            assert!(
                resource
                    .target
                    .facts
                    .lock()
                    .unwrap()
                    .retired_prompts
                    .is_empty(),
                "source closure must discard dead v2 prompt associations"
            );
            assert!(
                native
                    .core
                    .environment
                    .target(resource.pre.native())
                    .is_none(),
                "source closure must retire the target environment entry"
            );
            timeout(Duration::from_secs(8), storage.close())
                .await
                .unwrap();
            storage_scope.stop_and_observe().await;
            area.remove_owned_tree().unwrap();
            return;
        }

        eprintln!("early idle test: releasing late insertion");
        std::fs::write(area.join("release-late-insertion"), b"synthetic").unwrap();
        timeout(Duration::from_secs(10), async {
            loop {
                let settled: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM node_native_dispatches \
                     WHERE lifecycle='closed' AND close_reason='native_settled'",
                )
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
                let finished = service
                    .inspect_queue(&id(10), id(4), |view| {
                        let q = match view {
                            QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => {
                                queue
                            }
                        };
                        q.active().is_none()
                            && q.failure_suspended()
                            && q.last_stop_reason()
                                == Some(crate::domain::native_dispatch::KnownStopReason::MaxTokens)
                            && q.waiting()
                                .map(crate::domain::input_queue::QueueEntry::id)
                                .eq(old.iter().copied())
                            && q.waiting()
                                .all(crate::domain::input_queue::QueueEntry::suspended)
                    })
                    .await
                    .unwrap();
                if settled == 1 && finished {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        eprintln!("early idle test: late settlement observed");
        let terminal_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM node_native_dispatch_facts \
             WHERE fact_kind='foreground_terminal' AND native_stop_reason='max_tokens'",
        )
        .fetch_one(storage.pool_for_test())
        .await
        .unwrap();
        assert_eq!(
            terminal_count, 1,
            "late insertion must not require or manufacture a second idle"
        );
        eprintln!("early idle test: shutting down native");
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(8))
            .await;
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        timeout(Duration::from_secs(8), storage.close())
            .await
            .unwrap();
        eprintln!("early idle test: native and storage stopped");
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        eprintln!("early idle test: fixture area removed");
        return;
    }
    if let Some(reason) = scenario
        .strip_prefix("stop-v1-")
        .or_else(|| scenario.strip_prefix("stop-v2-"))
    {
        timeout(Duration::from_secs(10), async {
            while !area.join("first-prompt-ready").is_file() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let mut old = Vec::new();
        for _ in 0..3 {
            let mutation = ValidatedMutation::Submit(
                service
                    .validate_body(r#"[{"type":"text","text":"synthetic old waiting"}]"#)
                    .unwrap(),
            );
            let item = service.register(&id(10), id(4), &mutation).await.unwrap();
            assert!(matches!(
                service.consume(&id(10), id(4), item.id(), mutation).await,
                ConsumeReply::Resolved {
                    fact: RequestFact::Settled { .. },
                    ..
                }
            ));
            old.push(item.id().entity());
        }
        std::fs::write(area.join("release-first-prompt"), b"synthetic").unwrap();
        timeout(Duration::from_secs(10), async {
            loop {
                let ready = service
                    .inspect_queue(&id(10), id(4), |view| {
                        let q = match view {
                            QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => {
                                queue
                            }
                        };
                        q.active().is_none()
                            && q.failure_suspended()
                            && q.last_stop_reason()
                                == Some(crate::domain::native_dispatch::KnownStopReason::classify(
                                    Some(reason),
                                ))
                            && q.waiting().len() == 3
                            && q.waiting()
                                .all(crate::domain::input_queue::QueueEntry::suspended)
                    })
                    .await
                    .unwrap();
                if ready {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let reason_count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatch_facts WHERE result_kind='stopped' AND native_stop_reason=?").bind(reason).fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(reason_count, 1);
        if scenario.starts_with("stop-v2-") {
            let unassociated_terminals: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM node_native_dispatch_facts \
                 WHERE fact_kind='foreground_terminal' \
                   AND native_stop_reason=? \
                   AND attempt_id IS NULL",
            )
            .bind(reason)
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
            assert_eq!(
                unassociated_terminals, 1,
                "a live v2 foreground terminal must retain its unknown Run scope"
            );
        }
        if reason == "refusal" {
            let source = native.core.sources.lock().unwrap()[0].clone();
            let resource = source.all_resources.lock().unwrap()[0].clone();
            let raw = resource
                .target
                .facts
                .lock()
                .unwrap()
                .raw
                .iter()
                .map(|message| message.bytes().to_vec())
                .collect::<Vec<_>>();
            assert!(
                raw.iter().any(|message| {
                    let text = String::from_utf8_lossy(message);
                    text.contains("\"followUpContext\"") && text.contains("\"sdk-owned-context\"")
                }),
                "refusal must retain the original SDK follow-up context without normalization"
            );
        }
        let failures:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatch_facts WHERE result_kind IN('completed','failed')").fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(
            failures, 0,
            "non-success was not reported as success or failure"
        );
        let mutation = ValidatedMutation::Submit(
            service
                .validate_body(r#"[{"type":"text","text":"synthetic new priority after stop"}]"#)
                .unwrap(),
        );
        let new = service.register(&id(10), id(4), &mutation).await.unwrap();
        assert!(matches!(
            service.consume(&id(10), id(4), new.id(), mutation).await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        timeout(Duration::from_secs(10),async{loop{let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE lifecycle='closed' AND close_reason='native_settled'").fetch_one(storage.pool_for_test()).await.unwrap();let stable=service.inspect_queue(&id(10),id(4),|view|{let q=match view{QueueRead::Stable{queue}|QueueRead::Processing{queue,..}=>queue};q.active().is_none()&&q.failure_suspended()&&q.last_stop_reason()==Some(crate::domain::native_dispatch::KnownStopReason::classify(Some(reason)))&&q.waiting().len()==3&&q.waiting().all(crate::domain::input_queue::QueueEntry::suspended)}).await.unwrap();if count==2&&stable{break;}tokio::time::sleep(Duration::from_millis(5)).await;}}).await.unwrap();
        let remaining = service
            .inspect_queue(&id(10), id(4), |view| {
                let q = match view {
                    QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                };
                q.waiting()
                    .map(crate::domain::input_queue::QueueEntry::id)
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap();
        assert_eq!(
            remaining, old,
            "new input success did not restore old items"
        );
        let resume = ValidatedMutation::<OwnedRawValue>::Resume;
        let resumed = service.register(&id(10), id(4), &resume).await.unwrap();
        assert!(matches!(
            service.consume(&id(10), id(4), resumed.id(), resume).await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        timeout(Duration::from_secs(10),async{loop{let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE lifecycle='closed' AND close_reason='native_settled'").fetch_one(storage.pool_for_test()).await.unwrap();if count==5{break;}tokio::time::sleep(Duration::from_millis(5)).await;}}).await.unwrap();
        let resumed_state = service
            .inspect_queue(&id(10), id(4), |view| {
                let q = match view {
                    QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                };
                (q.failure_suspended(), q.last_stop_reason())
            })
            .await
            .unwrap();
        assert_eq!(resumed_state, (false, None));
        let order: Vec<String> = sqlx::query_scalar(
            "SELECT item_id FROM node_native_dispatches ORDER BY selection_owner_order",
        )
        .fetch_all(storage.pool_for_test())
        .await
        .unwrap();
        let expected = [
            registered.id().entity(),
            new.id().entity(),
            old[0],
            old[1],
            old[2],
        ]
        .map(EntityId::encode);
        assert_eq!(order, expected);
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(8))
            .await;
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        timeout(Duration::from_secs(8), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        return;
    }
    if scenario == "zero" {
        timeout(Duration::from_secs(10), async {
            loop {
                let n: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM node_native_write_attempts WHERE transport_state='zero'",
                )
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
                if n == 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let gate = gate.unwrap();
        assert_eq!(gate.accepted.load(Ordering::SeqCst), 0);
        assert!(gate.polled.load(Ordering::SeqCst) > 0);
        gate.open.store(true, Ordering::SeqCst);
        native.retry_zero(id(4)).await.unwrap();
    }
    if scenario == "storage-close-permission" {
        timeout(Duration::from_secs(8),async{loop{let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatch_facts WHERE fact_kind='approval_present'").fetch_one(storage.pool_for_test()).await.unwrap();if count==1{break;}tokio::task::yield_now().await;}}).await.unwrap();
        timeout(Duration::from_secs(10), storage.close())
            .await
            .unwrap();
        let raw_scope =
            crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&database, area.clone());
        raw_scope.start_raw().await;
        let mut check = sqlx::SqliteConnection::connect_with(&raw_scope.options().read_only(true))
            .await
            .unwrap();
        let controls:Vec<(String,String)>=sqlx::query_as("SELECT control_kind,transport_state FROM node_native_control_facts WHERE phase='terminal' ORDER BY control_kind").fetch_all(&mut check).await.unwrap();
        assert_eq!(
            controls,
            vec![
                ("native_cancel".into(), "written".into()),
                ("permission_cancelled".into(), "written".into())
            ]
        );
        let source: (String, i64) =
            sqlx::query_as("SELECT lifecycle,close_job_active FROM node_native_source_instances")
                .fetch_one(&mut check)
                .await
                .unwrap();
        assert_eq!(source, ("closed".into(), 0));
        check.close().await.unwrap();
        raw_scope.stop_and_observe().await;
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(2))
            .await;
        assert!(
            report.joined && report.metadata_confirmed,
            "original cached close report: {report:?}"
        );
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        return;
    }
    if scenario == "permission" {
        timeout(Duration::from_secs(15),async{loop{let n:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatch_facts WHERE fact_kind='approval_present'").fetch_one(storage.pool_for_test()).await.unwrap();if n==1{break;}tokio::time::sleep(Duration::from_millis(10)).await;}}).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM node_native_control_facts")
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
        assert_eq!(
            n, 0,
            "unknown permission remains live until an authorized shutdown"
        );
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(8))
            .await;
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        let controls:Vec<(String,Option<String>,String)>=sqlx::query_as("SELECT control_kind,rpc_id_kind,transport_state FROM node_native_control_facts WHERE phase='terminal' ORDER BY control_kind").fetch_all(storage.pool_for_test()).await.unwrap();
        assert_eq!(
            controls,
            vec![
                ("native_cancel".into(), None, "written".into()),
                (
                    "permission_cancelled".into(),
                    Some("string".into()),
                    "written".into()
                )
            ]
        );
        timeout(Duration::from_secs(8), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        return;
    }
    let result=timeout(Duration::from_secs(15),async{loop{let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE lifecycle='closed' AND close_reason='native_settled'").fetch_one(storage.pool_for_test()).await.unwrap();if count==1{break;}tokio::time::sleep(Duration::from_millis(10)).await;}}).await;
    if result.is_err() {
        let sources = native.core.sources.lock().unwrap().clone();
        for source in sources {
            let m = source.memory.lock().unwrap();
            eprintln!(
                "synthetic dev source stage {:?}, error {:?}",
                m.phase, m.error
            );
            drop(m);
        }
        panic!("factory did not settle synthetic native input");
    }
    if scenario == "zero" {
        let counts:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT count(*),count(DISTINCT selection_id),count(DISTINCT dispatch_id),count(DISTINCT rpc_id_bytes),sum(CASE WHEN transport_state='written' THEN 1 ELSE 0 END) FROM node_native_write_attempts").fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(counts, (2, 1, 1, 2, 1));
        let registrations: i64 =
            sqlx::query_scalar("SELECT count(*) FROM node_request_registrations")
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
        assert_eq!(registrations, 1);
    }
    if let Some(edit) = edited_request {
        let retired:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches d WHERE close_reason='known_zero' AND selection_id IS NULL AND NOT EXISTS(SELECT 1 FROM node_native_write_attempts a WHERE a.dispatch_id=d.dispatch_id)").fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(
            retired, 1,
            "old preparation is retired without fabricating a write attempt"
        );
        let origin:(String,Vec<u8>)=sqlx::query_as("SELECT content_request_id,content_revision FROM node_native_dispatches WHERE close_reason='native_settled'").fetch_one(storage.pool_for_test()).await.unwrap();
        assert_eq!(origin, (edit.encode(), 2_u64.to_be_bytes().to_vec()));
        let received: serde_json::Value =
            serde_json::from_slice(&std::fs::read(area.join("prompt-content.json")).unwrap())
                .unwrap();
        assert_eq!(
            received,
            serde_json::json!([{"type":"text","text":"synthetic edited body","_future":{"nested":[1,null,{"x":"kept"}]}}])
        );
    }
    let empty = service
        .inspect_queue(&id(10), id(4), |view| {
            let q = match view {
                QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
            };
            q.active().is_none() && q.waiting().next().is_none()
        })
        .await
        .unwrap();
    assert!(empty);
    if scenario == "caller-drop" {
        let cut=storage.request_registry().native_metadata_hooks_for_test().pause_next(crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::SourceClosedBeforeBegin).unwrap();
        let closing = native.clone();
        let caller = tokio::spawn(async move {
            closing
                .shutdown(Instant::now() + Duration::from_secs(8))
                .await
        });
        timeout(Duration::from_secs(6), cut.wait_until_entered())
            .await
            .unwrap();
        assert_eq!(native.core.shutdown_tasks.registered_for_test().unwrap(), 1);
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        cut.release();
        timeout(Duration::from_secs(6), async {
            loop {
                let count: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM node_native_source_instances WHERE lifecycle='closed'",
                )
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
                if count == 1 && native.status() == NativeRuntimeStatus::Stopped {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(2))
            .await;
        assert!(
            report.joined && report.metadata_confirmed,
            "same original cleanup completed: {report:?}"
        );
        assert_eq!(
            native.core.shutdown_tasks.registered_for_test().unwrap(),
            1,
            "second waiter must reuse the first cleanup operation"
        );
        timeout(Duration::from_secs(6), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        return;
    }
    if scenario == "worker-stop" {
        let cut=storage.request_registry().native_metadata_hooks_for_test().pause_next(crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::DispatchReserveBeforeBegin).unwrap();
        let mutation = ValidatedMutation::Submit(
            service
                .validate_body(r#"[{"type":"text","text":"synthetic second worker iteration"}]"#)
                .unwrap(),
        );
        let second = service.register(&id(10), id(4), &mutation).await.unwrap();
        assert!(matches!(
            service.consume(&id(10), id(4), second.id(), mutation).await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        timeout(Duration::from_secs(8), cut.wait_until_entered())
            .await
            .unwrap();
        let stopped = native.core.stop.notified();
        tokio::pin!(stopped);
        stopped.as_mut().enable();
        let closing = native.clone();
        let task = tokio::spawn(async move {
            closing
                .shutdown(Instant::now() + Duration::from_secs(4))
                .await
        });
        timeout(Duration::from_secs(2), &mut stopped).await.unwrap();
        cut.release();
        let report = timeout(Duration::from_secs(6), task)
            .await
            .unwrap()
            .unwrap();
        let first_joined = report.joined && report.metadata_confirmed;
        timeout(Duration::from_secs(8), storage.close())
            .await
            .unwrap();
        storage_scope.stop_and_observe().await;
        area.remove_owned_tree().unwrap();
        assert!(
            first_joined,
            "first shutdown must join the worker: {report:?}"
        );
        return;
    }
    if scenario == "shared" {
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,?)")
            .bind(id(5).encode())
            .bind(id(1).encode())
            .bind("synthetic-native-B")
            .bind(id(2).encode())
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        let mutation = ValidatedMutation::Submit(
            service
                .validate_body(r#"[{"type":"text","text":"synthetic B"}]"#)
                .unwrap(),
        );
        let record = service.register(&id(10), id(5), &mutation).await.unwrap();
        assert!(matches!(
            service.consume(&id(10), id(5), record.id(), mutation).await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        timeout(Duration::from_secs(10),async{loop{let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE lifecycle='closed' AND close_reason='native_settled'").fetch_one(storage.pool_for_test()).await.unwrap();if count==2{break;}tokio::time::sleep(Duration::from_millis(5)).await;}}).await.unwrap();
        let instances: i64 =
            sqlx::query_scalar("SELECT count(*) FROM node_native_source_instances")
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
        assert_eq!(instances, 1);
        native.release_target(id(4)).await.unwrap();
        native.release_target(id(4)).await.unwrap();
        let statuses: Vec<(String, String)> = sqlx::query_as(
            "SELECT target_id,lifecycle FROM node_native_resource_bindings ORDER BY target_id",
        )
        .fetch_all(storage.pool_for_test())
        .await
        .unwrap();
        assert!(
            statuses
                .iter()
                .any(|(target, state)| *target == id(4).encode() && state == "released")
        );
        assert!(
            statuses
                .iter()
                .any(|(target, state)| *target == id(5).encode() && state == "loaded")
        );
        let closed_sources: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM node_native_source_instances WHERE lifecycle='closed'",
        )
        .fetch_one(storage.pool_for_test())
        .await
        .unwrap();
        assert_eq!(closed_sources, 0);
        let mutation = ValidatedMutation::Submit(
            service
                .validate_body(r#"[{"type":"text","text":"synthetic B after A closed"}]"#)
                .unwrap(),
        );
        let record = service.register(&id(10), id(5), &mutation).await.unwrap();
        assert!(matches!(
            service.consume(&id(10), id(5), record.id(), mutation).await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        timeout(Duration::from_secs(10),async{loop{let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE lifecycle='closed' AND close_reason='native_settled'").fetch_one(storage.pool_for_test()).await.unwrap();if count==3{break;}tokio::time::sleep(Duration::from_millis(5)).await;}}).await.unwrap();
    }
    if scenario == "unknown-notification" {
        let source = native.core.sources.lock().unwrap()[0].clone();
        assert!(
            !source.pre.authority.quarantined.load(Ordering::Acquire),
            "unsupported notification has no authority to overwrite the known session state"
        );
        let current = source.all_resources.lock().unwrap()[0]
            .target
            .facts
            .lock()
            .unwrap()
            .foreground;
        assert_eq!(current, crate::domain::activity::WorkActivity::Idle);
        let next = ValidatedMutation::Submit(
            service
                .validate_body(r#"[{"type":"text","text":"synthetic after unknown notification"}]"#)
                .unwrap(),
        );
        let registration = service.register(&id(10), id(4), &next).await.unwrap();
        assert!(matches!(
            service
                .consume(&id(10), id(4), registration.id(), next)
                .await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        timeout(Duration::from_secs(6),async {loop {
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_native_dispatches WHERE close_reason='native_settled'").fetch_one(storage.pool_for_test()).await.unwrap();
            if count==2 {break;}tokio::task::yield_now().await;
        }}).await.unwrap();
        assert_eq!(native.core.sources.lock().unwrap().len(), 1);
    }
    let report = native
        .shutdown(Instant::now() + Duration::from_secs(8))
        .await;
    assert!(
        report.joined && report.metadata_confirmed,
        "physical/metadata close {report:?}"
    );
    if let Some(attachment) = bridge_attachment_weak {
        assert!(
            attachment.upgrade().is_none(),
            "the original unique view closer released its joined attachment"
        );
    }
    let source_close: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT lifecycle,close_owner_evidence,close_job_active FROM node_native_source_instances",
    )
    .fetch_all(storage.pool_for_test())
    .await
    .unwrap();
    assert_eq!(
        source_close,
        vec![("closed".into(), "windows_job".into(), 0)]
    );
    timeout(Duration::from_secs(8), storage.close())
        .await
        .unwrap();
    storage_scope.stop_and_observe().await;
    area.remove_owned_tree().unwrap();
}

#[derive(Default)]
struct PromptGate {
    open: AtomicBool,
    control: AtomicBool,
    polled: std::sync::atomic::AtomicUsize,
    accepted: std::sync::atomic::AtomicUsize,
}
struct GateWriter {
    actual: Box<dyn tokio::io::AsyncWrite + Unpin + Send>,
    gate: Arc<PromptGate>,
    prompt: bool,
}
impl tokio::io::AsyncWrite for GateWriter {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        if if self.gate.control.load(Ordering::SeqCst) {
            bytes
                .windows(b"synthetic-unsupported".len())
                .any(|part| part == b"synthetic-unsupported")
        } else {
            bytes
                .windows(b"session/prompt".len())
                .any(|part| part == b"session/prompt")
        } {
            self.prompt = true;
            self.gate.polled.fetch_add(1, Ordering::SeqCst);
            if !self.gate.open.load(Ordering::SeqCst) {
                return std::task::Poll::Pending;
            }
        }
        let result = std::pin::Pin::new(&mut self.actual).poll_write(cx, bytes);
        if self.prompt
            && let std::task::Poll::Ready(Ok(n)) = result
        {
            self.gate.accepted.fetch_add(n, Ordering::SeqCst);
        }
        result
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let result = std::pin::Pin::new(&mut self.actual).poll_flush(cx);
        if result.is_ready() {
            self.prompt = false;
        }
        result
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.actual).poll_shutdown(cx)
    }
}
struct GatedLauncher(Arc<PromptGate>);
struct FailingIoLauncher;
struct FailingIoOwned(WindowsOwnedSource);
struct FaultLauncher(crate::platform::process_owner::TrackedSpawnHooks);
impl SourceLaunchPort for FaultLauncher {
    type Plan = WindowsLaunchPlan;
    type Owned = WindowsOwnedSource;
    type Running = WindowsRunningProof;
    fn owner_kind(&self) -> NativeOwnerKind {
        NativeOwnerKind::WindowsJob
    }
    fn same_plan(&self, current: &WindowsLaunchPlan, candidate: &WindowsLaunchPlan) -> bool {
        WindowsSourceLauncher.same_plan(current, candidate)
    }
    fn verify_running(
        &self,
        owner: &WindowsOwnedSource,
        proof: &WindowsRunningProof,
        controller: &Arc<SourceLaunchController>,
    ) -> Result<(), DispatchError> {
        WindowsSourceLauncher.verify_running(owner, proof, controller)
    }
    fn launch(
        &self,
        plan: Arc<WindowsLaunchPlan>,
        permit: LaunchPermit,
        adopter: LaunchAdopter<WindowsOwnedSource>,
        deadline: Instant,
    ) -> BoxFuture<'_, LaunchCompletion<WindowsRunningProof>> {
        WindowsSourceLauncher.launch_with_hooks_for_test(
            plan,
            permit,
            adopter,
            deadline,
            self.0.clone(),
        )
    }
}
impl SourceLaunchPort for FailingIoLauncher {
    type Plan = WindowsLaunchPlan;
    type Owned = FailingIoOwned;
    type Running = WindowsRunningProof;
    fn verify_running(
        &self,
        owner: &FailingIoOwned,
        proof: &WindowsRunningProof,
        controller: &Arc<SourceLaunchController>,
    ) -> Result<(), DispatchError> {
        WindowsSourceLauncher.verify_running(&owner.0, proof, controller)
    }
    fn same_plan(&self, a: &WindowsLaunchPlan, b: &WindowsLaunchPlan) -> bool {
        WindowsSourceLauncher.same_plan(a, b)
    }
    fn owner_kind(&self) -> NativeOwnerKind {
        NativeOwnerKind::WindowsJob
    }
    fn launch(
        &self,
        plan: Arc<WindowsLaunchPlan>,
        permit: LaunchPermit,
        adopter: LaunchAdopter<FailingIoOwned>,
        deadline: Instant,
    ) -> BoxFuture<'_, LaunchCompletion<WindowsRunningProof>> {
        Box::pin(async move {
            WindowsSourceLauncher
                .launch(
                    plan,
                    permit,
                    LaunchAdopter {
                        adopt: Box::new(move |owned| adopter.adopt(FailingIoOwned(owned))),
                    },
                    deadline,
                )
                .await
        })
    }
}
impl OwnedNativeSource for FailingIoOwned {
    type Streams = AcpStreams;
    fn take_streams(&mut self) -> Result<AcpStreams, DispatchError> {
        Err(DispatchError::ProtocolUnavailable)
    }
    fn owner_kind(&self) -> NativeOwnerKind {
        self.0.owner_kind()
    }
    fn request_stop(&self) {
        self.0.request_stop();
    }
    fn shutdown(
        &mut self,
        deadline: Instant,
    ) -> BoxFuture<'_, Result<OwnedSourceShutdown, DispatchError>> {
        self.0.shutdown(deadline)
    }
}
struct GatedOwned {
    actual: WindowsOwnedSource,
    gate: Arc<PromptGate>,
}
impl SourceLaunchPort for GatedLauncher {
    type Plan = WindowsLaunchPlan;
    type Owned = GatedOwned;
    type Running = WindowsRunningProof;
    fn verify_running(
        &self,
        owner: &GatedOwned,
        proof: &WindowsRunningProof,
        controller: &Arc<SourceLaunchController>,
    ) -> Result<(), DispatchError> {
        WindowsSourceLauncher.verify_running(&owner.actual, proof, controller)
    }
    fn same_plan(&self, a: &WindowsLaunchPlan, b: &WindowsLaunchPlan) -> bool {
        WindowsSourceLauncher.same_plan(a, b)
    }
    fn owner_kind(&self) -> NativeOwnerKind {
        NativeOwnerKind::WindowsJob
    }
    fn launch(
        &self,
        plan: Arc<WindowsLaunchPlan>,
        permit: LaunchPermit,
        adopter: LaunchAdopter<GatedOwned>,
        deadline: Instant,
    ) -> BoxFuture<'_, LaunchCompletion<WindowsRunningProof>> {
        let gate = self.0.clone();
        Box::pin(async move {
            WindowsSourceLauncher
                .launch(
                    plan,
                    permit,
                    LaunchAdopter {
                        adopt: Box::new(move |actual| adopter.adopt(GatedOwned { actual, gate })),
                    },
                    deadline,
                )
                .await
        })
    }
}
impl OwnedNativeSource for GatedOwned {
    type Streams = AcpStreams;
    fn take_streams(&mut self) -> Result<AcpStreams, DispatchError> {
        let gate = self.gate.clone();
        Ok(self
            .actual
            .take_streams()?
            .map_writer_for_test(move |actual| {
                Box::new(GateWriter {
                    actual,
                    gate,
                    prompt: false,
                })
            }))
    }
    fn owner_kind(&self) -> NativeOwnerKind {
        self.actual.owner_kind()
    }
    fn request_stop(&self) {
        self.actual.request_stop();
    }
    fn shutdown(
        &mut self,
        deadline: Instant,
    ) -> BoxFuture<'_, Result<OwnedSourceShutdown, DispatchError>> {
        self.actual.shutdown(deadline)
    }
}

#[tokio::test]
async fn native_cause_real_missing_exe_preserves_managed_error_and_original_close() {
    run_factory_scenario("cause-missing-exe").await;
}
#[tokio::test]
async fn native_cause_real_resume_remote_error_keeps_old_protocol_class() {
    run_factory_scenario("cause-resume-error").await;
}
#[tokio::test]
async fn native_cause_real_ready_wins_later_loaded_sql_failure_and_then_release() {
    run_factory_scenario("cause-ready-metadata").await;
}

#[tokio::test]
async fn native_effective_close_deadline_respects_earlier_source_close_deadline() {
    let child = crate::test_support::native_dispatch_dev_child(&["NATIVE_DISPATCH_DEV_CHILD"]);
    assert!(child.is_file());
    let area = std::env::temp_dir().join(format!(
        "acpr-native-close-deadline-{}-{}",
        std::process::id(),
        random_id().unwrap().encode()
    ));
    std::fs::create_dir_all(&area).unwrap();
    let area = Arc::new(NativeArea { directory: area });
    let database = area.join("node.sqlite3");
    let storage_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&database, area.clone());
    let storage = storage_scope.open().await.unwrap();
    storage
        .write_node_meta(&NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "native-dev-close-deadline".into(),
            public_key: vec![1, 2, 3],
            key_file_ref: "synthetic-unused".into(),
        })
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_sources VALUES(?,'native close deadline')")
        .bind(id(1).encode())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces VALUES(?,?)")
        .bind(id(2).encode())
        .bind(area.to_str().unwrap())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO harnesses VALUES(?,'native dev',?,?,NULL)")
        .bind(id(3).encode())
        .bind(child.to_str().unwrap())
        .bind(r#"["success"]"#)
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_harness_sources VALUES(?,?)")
        .bind(id(3).encode())
        .bind(id(1).encode())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,?)")
        .bind(id(4).encode())
        .bind(id(1).encode())
        .bind("synthetic-close-deadline")
        .bind(id(2).encode())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    let config = Arc::new(storage.native_configuration_provider(Arc::new(Configuration)));
    let mut budget = options();
    budget.close_wait = Duration::from_secs(30);
    sqlx::query("INSERT INTO clients VALUES(?,?,'native dev','trusted',1,0,NULL)")
        .bind(id(10).encode())
        .bind(vec![1u8])
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    let (service, native) = storage
        .start_native_service(
            Authority(storage.pool_for_test().clone()),
            Validator,
            SystemNodeClock,
            Arc::new(AcpNativeAdapter::default()),
            Arc::new(WindowsSourceLauncher),
            config,
            Arc::new(Retain),
            budget,
        )
        .await
        .unwrap();
    timeout(Duration::from_secs(5), async {
        while native.status() == NativeRuntimeStatus::Starting {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let next = ValidatedMutation::Submit(
        service
            .validate_body(r#"[{"type":"text","text":"synthetic close deadline"}]"#)
            .unwrap(),
    );
    let record = service.register(&id(10), id(4), &next).await.unwrap();
    assert!(matches!(
        service.consume(&id(10), id(4), record.id(), next).await,
        ConsumeReply::Resolved {
            fact: RequestFact::Settled { .. },
            ..
        }
    ));
    native.ensure_target(id(4)).await.unwrap();
    let source = native.core.sources.lock().unwrap()[0].clone();
    assert!(
        native.core.effective_close_deadline(&source).unwrap()
            > Instant::now() + Duration::from_secs(20)
    );
    let earlier = Instant::now() + Duration::from_millis(120);
    *source.close_deadline.lock().unwrap() = Some(earlier);
    assert_eq!(
        native.core.effective_close_deadline(&source).unwrap(),
        earlier
    );
    *source.close_deadline.lock().unwrap() = Some(Instant::now() + Duration::from_secs(60));
    assert!(
        native.core.effective_close_deadline(&source).unwrap()
            < Instant::now() + Duration::from_secs(45)
    );
    *source.close_deadline.lock().unwrap() = None;
    drop(source);
    let started = Instant::now();
    let report = native.shutdown(started + Duration::from_millis(400)).await;
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "shutdown should honor caller deadline rather than multiplying 30s close_wait: {report:?}"
    );
    timeout(Duration::from_secs(5), storage.close())
        .await
        .unwrap();
    storage_scope.stop_and_observe().await;
    area.remove_owned_tree().unwrap();
}
