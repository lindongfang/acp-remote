//! Read-side catalog consumer. Schema installation and management mutation are separate work.
use super::{
    catalog_sql::{CatalogOperation, CatalogSqlFence},
    request_registry::SqliteRequestRegistry,
};
use crate::{
    adapters::acp::{
        SetupSnapshot,
        node_configuration_profile::{self as codec, decode_profile},
    },
    application::{
        native_dispatch::{
            AcceptedTargetProof, BoxFuture, ConfigurationKey, ConfigurationPort,
            ConfiguredSourcePlan, ReadConfigurationPort, SourceInitializationPlan,
            SourceReadSelectionProof,
        },
        queue_owner::ReadTargetProof,
        request_registry::RegistryDeadline,
    },
    domain::{
        EntityId,
        native::NativeSessionKey,
        native_catalog::{
            CatalogError, CatalogOperationKind, CatalogRequestId, CatalogSqlRole, ProfileKey,
        },
        native_dispatch::DispatchError,
    },
    platform::{SecretProtector, native_source::WindowsLaunchPlan},
};
use sha2::{Digest, Sha256};
use sqlx::{Row, sqlite::SqliteRow};
use std::{path::PathBuf, sync::Arc, time::Duration};
use zeroize::Zeroizing;

mod intent;
mod management;
pub(super) mod source_sessions;

pub struct SqliteNodeConfiguration {
    registry: SqliteRequestRegistry,
    fence: Arc<CatalogSqlFence>,
    protector: Arc<dyn SecretProtector>,
    node_id: String,
    public_key: Vec<u8>,
    metadata_wait: Duration,
}
impl SqliteNodeConfiguration {
    fn from_node(
        registry: SqliteRequestRegistry,
        protector: Arc<dyn SecretProtector>,
        node_id: String,
        public_key: Vec<u8>,
        metadata_wait: Duration,
    ) -> Result<Self, CatalogError> {
        registry.owner_scope().require_open()?;
        Ok(Self {
            fence: registry.catalog_fence(),
            registry,
            protector,
            node_id,
            public_key,
            metadata_wait,
        })
    }
    async fn selected(
        &self,
        proof: &impl ConfigurationWitness,
        kind: CatalogOperationKind,
    ) -> Result<ConfiguredSourcePlan<WindowsLaunchPlan, SetupSnapshot>, DispatchError> {
        proof.require_current()?;
        let d = RegistryDeadline::new(
            tokio::time::Instant::now()
                .checked_add(self.metadata_wait)
                .ok_or(DispatchError::StorageUnavailable)?,
        );
        let scope = self.registry.owner_scope();
        proof.verify_registry_owner(&scope)?;
        scope.require_open()?;
        let _lease = self.registry.catalog_operation_lease()?;
        let guard = self
            .fence
            .enter(kind, &scope, &d)
            .await
            .map_err(|e| DispatchError::from(e.kind()))?;
        let operation = guard.operation.clone();
        let result = d
            .run(scope.while_open(async { Ok(self.read_and_decode(proof, &operation, &d).await) }))
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
    async fn read_and_decode(
        &self,
        proof: &impl ConfigurationWitness,
        operation: &Arc<CatalogOperation>,
        d: &RegistryDeadline,
    ) -> Result<ConfiguredSourcePlan<WindowsLaunchPlan, SetupSnapshot>, CatalogError> {
        let mut c = self
            .registry
            .acquire_catalog_connection(operation.clone(), CatalogSqlRole::ConfigurationRead)
            .await?;
        c.begin_read().await?;
        let selected = async {
            let row = sqlx::query(SELECT_CONFIGURATION)
                .bind(proof.target().encode())
                .fetch_optional(c.sql()?)
                .await
                .map_err(|_| CatalogError::StorageUnavailable)?
                .ok_or(CatalogError::InvalidTarget)?;
            let mut snapshot = SelectedProfile::from_row(&row)?;
            let roots = sqlx::query("SELECT workspace_id,canonical_path FROM main.workspaces")
                .fetch_all(c.sql()?)
                .await
                .map_err(|_| CatalogError::StorageUnavailable)?;
            snapshot
                .workspaces
                .try_reserve_exact(roots.len())
                .map_err(|_| CatalogError::Allocation)?;
            for row in roots {
                let raw: String = row
                    .try_get("workspace_id")
                    .map_err(|_| CatalogError::CorruptState)?;
                let workspace = EntityId::decode(&raw).map_err(|_| CatalogError::CorruptState)?;
                let path: String = row
                    .try_get("canonical_path")
                    .map_err(|_| CatalogError::CorruptState)?;
                snapshot.workspaces.push((workspace, path));
            }
            if snapshot.node_id != self.node_id || snapshot.public_key != self.public_key {
                return Err(CatalogError::CorruptState);
            }
            if snapshot.source != proof.native().source().0
                || snapshot.native.as_deref() != Some(proof.native().native_id())
            {
                return Err(CatalogError::InvalidSource);
            }
            Ok(snapshot)
        }
        .await;
        let snapshot = c.finish_catalog(selected, d).await?;
        proof
            .require_current()
            .map_err(|_| CatalogError::InvalidTarget)?;
        self.plan(proof, snapshot).await
    }
    fn decode_snapshot(
        &self,
        snapshot: &SelectedProfile,
    ) -> Result<codec::DecodedProfile, CatalogError> {
        let context =
            protection_context(&snapshot.node_id, &snapshot.public_key, snapshot.request)?;
        let plaintext = Zeroizing::new(
            self.protector
                .unprotect(&snapshot.cipher, &context)
                .map_err(|_| CatalogError::ProtectionUnavailable)?,
        );
        let intent = intent::Intent::decode(
            plaintext.to_vec(),
            snapshot.request,
            snapshot.actor,
            snapshot.generation,
            intent::action(&snapshot.action)?,
        )?;
        if intent.profile != snapshot.profile
            || intent.harness != snapshot.harness
            || intent.source != snapshot.source
        {
            return Err(CatalogError::CorruptState);
        }
        decode_profile(intent.document.ok_or(CatalogError::CorruptState)?.to_vec())
    }
    async fn plan(
        &self,
        proof: &impl ConfigurationWitness,
        snapshot: SelectedProfile,
    ) -> Result<ConfiguredSourcePlan<WindowsLaunchPlan, SetupSnapshot>, CatalogError> {
        let plan = self.source_plan(snapshot).await?;
        proof
            .require_current()
            .map_err(|_| CatalogError::InvalidTarget)?;
        Ok(ConfiguredSourcePlan {
            source_id: plan.source_id,
            target: proof.target(),
            native: proof.native().clone(),
            workspace: plan.workspace,
            harness: plan.harness,
            configuration: plan.configuration,
            launch: plan.launch,
            setups: plan.setups,
        })
    }
    async fn source_plan(
        &self,
        snapshot: SelectedProfile,
    ) -> Result<SourceInitializationPlan<WindowsLaunchPlan, SetupSnapshot>, CatalogError> {
        let registered_canonical = snapshot
            .workspaces
            .iter()
            .find(|(id, _)| *id == snapshot.workspace)
            .map(|(_, path)| path.as_str())
            .ok_or(CatalogError::InvalidConfiguration)?;
        if !source_sessions::launch_path_within_workspace(
            &snapshot.launch_path,
            registered_canonical,
        )
        .await
        {
            return Err(CatalogError::InvalidConfiguration);
        }
        let mut decoded = self.decode_snapshot(&snapshot)?;
        for recipe in &decoded.recipes {
            for directory in &recipe.directories {
                let registered = snapshot
                    .workspaces
                    .iter()
                    .find(|(id, path)| *id == directory.workspace && *path == directory.path)
                    .map(|(_, path)| path.as_str())
                    .ok_or(CatalogError::InvalidConfiguration)?;
                if !source_sessions::launch_path_within_workspace(&directory.path, registered).await
                {
                    return Err(CatalogError::InvalidConfiguration);
                }
            }
        }
        let mut setups = Vec::new();
        setups
            .try_reserve_exact(decoded.recipes.len())
            .map_err(|_| CatalogError::Allocation)?;
        for recipe in decoded.recipes.drain(..) {
            let protocol = codec::protocol(recipe.wire);
            setups.push((
                protocol,
                codec::setup(snapshot.profile, &snapshot.launch_path, recipe),
            ));
        }
        self.registry.owner_scope().require_open()?;
        Ok(SourceInitializationPlan {
            source_id: snapshot.source,
            workspace: snapshot.workspace,
            harness: snapshot.harness,
            configuration: ConfigurationKey {
                identity: snapshot.profile.id(),
                revision: snapshot.profile.revision(),
            },
            launch: Arc::new(WindowsLaunchPlan {
                executable: PathBuf::from(decoded.executable),
                arguments: decoded.arguments,
                cwd: PathBuf::from(snapshot.launch_path),
                environment: decoded.environment,
            }),
            setups,
        })
    }
}
trait ConfigurationWitness: Sync {
    fn target(&self) -> EntityId;
    fn native(&self) -> &NativeSessionKey;
    fn require_current(&self) -> Result<(), DispatchError>;
    fn verify_registry_owner(
        &self,
        scope: &crate::application::request_registry::RegistryOwnerScope,
    ) -> Result<(), DispatchError>;
}
impl ConfigurationWitness for AcceptedTargetProof {
    fn target(&self) -> EntityId {
        self.target()
    }
    fn native(&self) -> &NativeSessionKey {
        self.native()
    }
    fn require_current(&self) -> Result<(), DispatchError> {
        self.require_current()
    }
    fn verify_registry_owner(
        &self,
        scope: &crate::application::request_registry::RegistryOwnerScope,
    ) -> Result<(), DispatchError> {
        self.verify_registry_owner(scope)
    }
}
impl ConfigurationWitness for ReadTargetProof {
    fn target(&self) -> EntityId {
        self.target()
    }
    fn native(&self) -> &NativeSessionKey {
        self.native()
    }
    fn require_current(&self) -> Result<(), DispatchError> {
        self.require_current()
    }
    fn verify_registry_owner(
        &self,
        scope: &crate::application::request_registry::RegistryOwnerScope,
    ) -> Result<(), DispatchError> {
        self.verify_registry_owner(scope)
    }
}
impl ConfigurationPort for SqliteNodeConfiguration {
    type LaunchPlan = WindowsLaunchPlan;
    type Setup = SetupSnapshot;
    fn resolve<'a>(
        &'a self,
        proof: &'a AcceptedTargetProof,
    ) -> BoxFuture<'a, Result<ConfiguredSourcePlan<WindowsLaunchPlan, SetupSnapshot>, DispatchError>>
    {
        Box::pin(self.selected(proof, CatalogOperationKind::Resolve))
    }
}
impl ReadConfigurationPort for SqliteNodeConfiguration {
    fn resolve_read<'a>(
        &'a self,
        proof: &'a ReadTargetProof,
    ) -> BoxFuture<'a, Result<ConfiguredSourcePlan<WindowsLaunchPlan, SetupSnapshot>, DispatchError>>
    {
        Box::pin(self.selected(proof, CatalogOperationKind::ResolveRead))
    }
    fn resolve_source_read<'a>(
        &'a self,
        proof: &'a SourceReadSelectionProof,
    ) -> BoxFuture<
        'a,
        Result<SourceInitializationPlan<WindowsLaunchPlan, SetupSnapshot>, DispatchError>,
    > {
        Box::pin(self.selected_source_read(proof))
    }
}

pub(super) fn protection_context(
    node_id: &str,
    public_key: &[u8],
    request: CatalogRequestId,
) -> Result<Vec<u8>, CatalogError> {
    let request = request.encode();
    let mut result = Vec::new();
    let size = b"acp-remote/node-catalog/protected-intent\0"
        .len()
        .checked_add(4)
        .and_then(|n| n.checked_add(24))
        .and_then(|n| n.checked_add(node_id.len()))
        .and_then(|n| n.checked_add(public_key.len()))
        .and_then(|n| n.checked_add(request.len()))
        .ok_or(CatalogError::Allocation)?;
    result
        .try_reserve_exact(size)
        .map_err(|_| CatalogError::Allocation)?;
    result.extend_from_slice(b"acp-remote/node-catalog/protected-intent\0");
    result.extend_from_slice(&1_u32.to_be_bytes());
    for bytes in [node_id.as_bytes(), public_key, request.as_bytes()] {
        let len = u64::try_from(bytes.len()).map_err(|_| CatalogError::Allocation)?;
        result.extend_from_slice(&len.to_be_bytes());
        result.extend_from_slice(bytes);
    }
    Ok(result)
}
struct SelectedProfile {
    source: EntityId,
    native: Option<String>,
    workspace: EntityId,
    harness: EntityId,
    profile: ProfileKey,
    launch_path: String,
    node_id: String,
    public_key: Vec<u8>,
    request: CatalogRequestId,
    actor: EntityId,
    action: String,
    generation: i64,
    cipher: Vec<u8>,
    workspaces: Vec<(EntityId, String)>,
}
impl SelectedProfile {
    fn from_row(row: &SqliteRow) -> Result<Self, CatalogError> {
        let get = |name: &str| {
            row.try_get::<String, _>(name)
                .map_err(|_| CatalogError::CorruptState)
        };
        let entity =
            |name: &str| EntityId::decode(&get(name)?).map_err(|_| CatalogError::CorruptState);
        let revision: Vec<u8> = row
            .try_get("revision")
            .map_err(|_| CatalogError::CorruptState)?;
        let revision = u64::from_be_bytes(
            revision
                .as_slice()
                .try_into()
                .map_err(|_| CatalogError::CorruptState)?,
        );
        let cipher: Vec<u8> = row
            .try_get("protected_payload")
            .map_err(|_| CatalogError::CorruptState)?;
        let digest: Vec<u8> = row
            .try_get("payload_digest")
            .map_err(|_| CatalogError::CorruptState)?;
        let command_digest: Vec<u8> = row
            .try_get("intent_digest")
            .map_err(|_| CatalogError::CorruptState)?;
        let generation: i64 = row
            .try_get("register_auth_generation")
            .map_err(|_| CatalogError::CorruptState)?;
        if generation < 0
            || digest.len() != 32
            || digest != command_digest
            || Sha256::digest(&cipher).as_slice() != digest
            || get("state")? != "committed"
            || row
                .try_get::<i64, _>("payload_format")
                .map_err(|_| CatalogError::CorruptState)?
                != 1
        {
            return Err(CatalogError::CorruptState);
        }
        Ok(Self {
            source: entity("source_id")?,
            native: row
                .try_get("native_id")
                .map_err(|_| CatalogError::CorruptState)?,
            workspace: entity("workspace_id")?,
            harness: entity("harness_id")?,
            profile: ProfileKey::try_new(entity("profile_id")?, revision)
                .map_err(|_| CatalogError::CorruptState)?,
            launch_path: get("launch_path")?,
            node_id: get("node_id")?,
            public_key: row
                .try_get("public_key")
                .map_err(|_| CatalogError::CorruptState)?,
            request: CatalogRequestId::new(entity("created_by_request_id")?),
            actor: entity("actor_id")?,
            action: get("action")?,
            generation,
            cipher,
            workspaces: Vec::new(),
        })
    }
}
const SELECT_CONFIGURATION: &str = "SELECT p.profile_id,p.revision,p.harness_id,p.source_id,p.payload_format,p.protected_payload,p.payload_digest,p.created_by_request_id,c.actor_id,c.action,c.register_auth_generation,c.state,c.intent_digest,s.workspace_id,s.launch_path,t.native_id,m.node_id,m.public_key FROM main.node_target_launch_selection s JOIN main.node_launch_profiles p ON p.profile_id=s.profile_id AND p.revision=s.profile_revision JOIN main.node_catalog_commands c ON c.request_id=p.created_by_request_id AND c.profile_id=p.profile_id AND c.profile_revision=p.revision AND ((c.action='harness.install' AND c.result_kind='harness' AND c.result_id=p.harness_id) OR (c.action='profile.publish' AND c.result_kind='profile' AND c.result_id=p.profile_id)) JOIN main.node_catalog_commands b ON b.request_id=s.selected_by_request_id AND b.action='target.bind' AND b.state='committed' AND b.result_kind='target' AND b.result_id=s.target_id AND b.profile_id=p.profile_id AND b.profile_revision=p.revision JOIN main.product_targets t ON t.target_id=s.target_id AND t.source_id=p.source_id AND t.workspace_id=s.workspace_id JOIN main.workspaces w ON w.workspace_id=s.workspace_id AND w.canonical_path=s.observed_canonical_path JOIN main.product_harness_sources hs ON hs.harness_id=p.harness_id AND hs.source_id=p.source_id JOIN main.harnesses h ON h.harness_id=p.harness_id JOIN main.product_sources ps ON ps.source_id=p.source_id JOIN main.node_meta m ON m.singleton=1 WHERE s.target_id=?";

#[cfg(test)]
mod tests;

pub(super) fn from_node(
    registry: SqliteRequestRegistry,
    protector: Arc<dyn SecretProtector>,
    node_id: String,
    public_key: Vec<u8>,
    metadata_wait: Duration,
) -> Result<SqliteNodeConfiguration, CatalogError> {
    SqliteNodeConfiguration::from_node(registry, protector, node_id, public_key, metadata_wait)
}
