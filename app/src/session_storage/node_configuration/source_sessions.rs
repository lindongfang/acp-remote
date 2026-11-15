//! Source-only projection of the same committed, protected profile and registered workspaces.
use super::{
    Arc, BoxFuture, CatalogError, CatalogOperation, CatalogOperationKind, CatalogSqlRole,
    DispatchError, EntityId, RegistryDeadline, Row, SelectedProfile, SetupSnapshot,
    SqliteNodeConfiguration, WindowsLaunchPlan,
};
use crate::application::request_registry::RegistryOwnerScope;
use crate::{
    application::native_dispatch::{
        SourceInitializationPlan, SourceReadSelectionProof,
        source_sessions::{SourceAccessProof, SourceConfigurationPort},
    },
    domain::source_sessions::{SourceLaunchSelection, SourceSessionError, SourceSessionFailure},
};

pub(in crate::session_storage) async fn verify_directory_on_disk(
    path: &str,
) -> Result<String, CatalogError> {
    if path.is_empty() || path.contains('\0') || path.len() > 4096 {
        return Err(CatalogError::InvalidConfiguration);
    }
    let owned = path.to_owned();
    tokio::task::spawn_blocking(move || {
        let candidate = std::path::Path::new(&owned);
        if !candidate.is_absolute() {
            return Err(CatalogError::InvalidConfiguration);
        }
        let canon =
            std::fs::canonicalize(candidate).map_err(|_| CatalogError::InvalidConfiguration)?;
        if !canon.is_dir() {
            return Err(CatalogError::InvalidConfiguration);
        }
        canon
            .to_str()
            .map(str::to_owned)
            .ok_or(CatalogError::InvalidConfiguration)
    })
    .await
    .map_err(|_| CatalogError::StorageUnavailable)?
}

pub(in crate::session_storage) async fn launch_path_within_workspace(
    launch_path: &str,
    canonical_path: &str,
) -> bool {
    if launch_path.is_empty()
        || launch_path.contains('\0')
        || canonical_path.is_empty()
        || canonical_path.contains('\0')
    {
        return false;
    }
    let launch = launch_path.to_owned();
    let workspace = canonical_path.to_owned();
    tokio::task::spawn_blocking(move || {
        let launch_path = std::path::Path::new(&launch);
        let workspace_path = std::path::Path::new(&workspace);
        if !launch_path.is_absolute() || !workspace_path.is_absolute() {
            return false;
        }
        let Ok(canon_launch) = std::fs::canonicalize(launch_path) else {
            return false;
        };
        let Ok(canon_workspace) = std::fs::canonicalize(workspace_path) else {
            return false;
        };
        canon_launch.is_dir()
            && canon_workspace.is_dir()
            && canon_launch.starts_with(&canon_workspace)
    })
    .await
    .unwrap_or(false)
}

impl SourceConfigurationPort for SqliteNodeConfiguration {
    fn source_configuration_scope(&self) -> RegistryOwnerScope {
        self.registry.owner_scope()
    }
    fn resolve_source<'a>(
        &'a self,
        proof: &'a SourceAccessProof,
    ) -> BoxFuture<
        'a,
        Result<SourceInitializationPlan<WindowsLaunchPlan, SetupSnapshot>, SourceSessionFailure>,
    > {
        Box::pin(self.selected_source(proof))
    }
}
impl SqliteNodeConfiguration {
    pub(super) async fn selected_source_read(
        &self,
        proof: &SourceReadSelectionProof,
    ) -> Result<SourceInitializationPlan<WindowsLaunchPlan, SetupSnapshot>, DispatchError> {
        proof.require_current()?;
        let scope = self.registry.owner_scope();
        proof.verify_registry_owner(&scope)?;
        scope.require_open()?;
        let deadline = tokio::time::Instant::now()
            .checked_add(self.metadata_wait)
            .ok_or(DispatchError::StorageUnavailable)?;
        let d = RegistryDeadline::new(deadline);
        let _lease = self.registry.catalog_operation_lease()?;
        let guard = self
            .fence
            .enter(CatalogOperationKind::ResolveRead, &scope, &d)
            .await
            .map_err(|e| DispatchError::from(e.kind()))?;
        let operation = guard.operation.clone();
        let result = d
            .run(scope.while_open(async {
                Ok(self
                    .read_source_selection(proof.selection(), &operation, &d)
                    .await)
            }))
            .await;
        let result = match result {
            Ok(v) => v,
            Err(e) => Err(CatalogError::from(e)),
        };
        let final_result = match result {
            Ok(plan) => {
                proof.require_current()?;
                scope.require_open()?;
                if d.is_cancelled() || tokio::time::Instant::now() >= d.deadline() {
                    return Err(CatalogError::Cancelled.into());
                }
                Ok(plan)
            }
            Err(e) => Err(operation.failure(e).kind().into()),
        };
        drop(guard);
        final_result
    }
    async fn selected_source(
        &self,
        proof: &SourceAccessProof,
    ) -> Result<SourceInitializationPlan<WindowsLaunchPlan, SetupSnapshot>, SourceSessionFailure>
    {
        let scope = self.registry.owner_scope();
        proof.verify_owner_scope(&scope)?;
        let deadline = tokio::time::Instant::now()
            .checked_add(self.metadata_wait)
            .ok_or(SourceSessionError::StorageUnavailable)?;
        let d = RegistryDeadline::new(deadline);
        let _lease = self
            .registry
            .catalog_operation_lease()
            .map_err(|_| SourceSessionError::NodeStopping)?;
        let guard = self
            .fence
            .enter(CatalogOperationKind::Resolve, &scope, &d)
            .await?;
        let operation = guard.operation.clone();
        let result = d
            .run(
                scope.while_open(async {
                    Ok(self.read_source_profile(proof, &operation, &d).await)
                }),
            )
            .await;
        let result = match result {
            Ok(value) => value,
            Err(error) => Err(CatalogError::from(error)),
        };
        let result =
            result.map_err(|error| SourceSessionFailure::Configuration(operation.failure(error)));
        if result.is_ok() {
            proof.verify_owner_scope(&scope)?;
        }
        drop(guard);
        result
    }
    async fn read_source_profile(
        &self,
        proof: &SourceAccessProof,
        operation: &Arc<CatalogOperation>,
        d: &RegistryDeadline,
    ) -> Result<SourceInitializationPlan<WindowsLaunchPlan, SetupSnapshot>, CatalogError> {
        let plan = self
            .read_source_selection(proof.selection(), operation, d)
            .await?;
        proof
            .verify_owner_scope(&self.registry.owner_scope())
            .map_err(|_| CatalogError::InvalidActor)?;
        Ok(plan)
    }
    async fn read_source_selection(
        &self,
        selection: &SourceLaunchSelection,
        operation: &Arc<CatalogOperation>,
        d: &RegistryDeadline,
    ) -> Result<SourceInitializationPlan<WindowsLaunchPlan, SetupSnapshot>, CatalogError> {
        if selection.launch_path.is_empty() || selection.launch_path.contains('\0') {
            return Err(CatalogError::InvalidConfiguration);
        }
        let mut c = self
            .registry
            .acquire_catalog_connection(operation.clone(), CatalogSqlRole::ConfigurationRead)
            .await?;
        c.begin_read().await?;
        let selected = async {
            let row = sqlx::query(SELECT_SOURCE)
                .bind(&selection.launch_path)
                .bind(selection.workspace.encode())
                .bind(selection.profile.id().encode())
                .bind(selection.profile.revision().to_be_bytes().to_vec())
                .bind(selection.source.encode())
                .bind(selection.harness.encode())
                .fetch_optional(c.sql()?)
                .await
                .map_err(|_| CatalogError::StorageUnavailable)?
                .ok_or(CatalogError::InvalidConfiguration)?;
            let mut snapshot = SelectedProfile::from_row(&row)?;
            if snapshot.node_id != self.node_id
                || snapshot.public_key != self.public_key
                || snapshot.profile != selection.profile
                || snapshot.source != selection.source
                || snapshot.harness != selection.harness
                || snapshot.workspace != selection.workspace
                || snapshot.launch_path != selection.launch_path
                || snapshot.native.is_some()
            {
                return Err(CatalogError::CorruptState);
            }
            let roots = sqlx::query("SELECT workspace_id,canonical_path FROM main.workspaces")
                .fetch_all(c.sql()?)
                .await
                .map_err(|_| CatalogError::StorageUnavailable)?;
            snapshot
                .workspaces
                .try_reserve_exact(roots.len())
                .map_err(|_| CatalogError::Allocation)?;
            for row in roots {
                let id: String = row
                    .try_get("workspace_id")
                    .map_err(|_| CatalogError::CorruptState)?;
                let id = EntityId::decode(&id).map_err(|_| CatalogError::CorruptState)?;
                let path: String = row
                    .try_get("canonical_path")
                    .map_err(|_| CatalogError::CorruptState)?;
                snapshot.workspaces.push((id, path));
            }
            Ok(snapshot)
        }
        .await;
        let snapshot = c.finish_catalog(selected, d).await?;
        self.source_plan(snapshot).await
    }
}
const SELECT_SOURCE: &str = "SELECT p.profile_id,p.revision,p.harness_id,p.source_id,p.payload_format,p.protected_payload,p.payload_digest,p.created_by_request_id,c.actor_id,c.action,c.register_auth_generation,c.state,c.intent_digest,w.workspace_id,? AS launch_path,NULL AS native_id,m.node_id,m.public_key FROM main.node_launch_profiles p JOIN main.node_catalog_commands c ON c.request_id=p.created_by_request_id AND c.profile_id=p.profile_id AND c.profile_revision=p.revision AND c.state='committed' AND ((c.action='harness.install' AND c.result_kind='harness' AND c.result_id=p.harness_id) OR (c.action='profile.publish' AND c.result_kind='profile' AND c.result_id=p.profile_id)) JOIN main.product_harness_sources hs ON hs.harness_id=p.harness_id AND hs.source_id=p.source_id JOIN main.harnesses h ON h.harness_id=p.harness_id JOIN main.product_sources ps ON ps.source_id=p.source_id JOIN main.workspaces w ON w.workspace_id=? JOIN main.node_meta m ON m.singleton=1 WHERE p.profile_id=? AND p.revision=? AND p.source_id=? AND p.harness_id=?";
