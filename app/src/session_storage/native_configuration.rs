//! Actual management definitions plus an explicitly selected trusted configuration provider.
use super::{native_dispatch::SqliteNativeDispatch, request_registry::SqliteRequestRegistry};
use crate::{
    adapters::acp::SetupSnapshot,
    application::{
        native_dispatch::{
            AcceptedTargetProof, BoxFuture, ConfigurationKey, ConfigurationPort,
            ConfiguredSourcePlan, ReadConfigurationPort, SourceInitializationPlan,
            SourceReadSelectionProof,
        },
        queue_owner::ReadTargetProof,
        request_registry::RegistryDeadline,
    },
    domain::{EntityId, native::NativeSessionKey, native_dispatch::DispatchError},
    platform::native_source::WindowsLaunchPlan,
};
use sqlx::Row;
use std::{ffi::OsString, path::PathBuf, sync::Arc};
use tokio::time::{Duration, Instant};

/// No Debug: these trusted inputs may contain private paths and configuration references.
pub struct NativeDefinition {
    pub target: EntityId,
    pub source: EntityId,
    pub harness: EntityId,
    pub workspace: EntityId,
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
    pub cwd: PathBuf,
    pub configuration_reference: Option<String>,
}
pub struct EffectiveNativeConfiguration {
    pub identity: EntityId,
    pub revision: u64,
    pub setups: Vec<(
        crate::domain::native_dispatch::ProtocolVersion,
        SetupSnapshot,
    )>,
    pub environment: Vec<(OsString, OsString)>,
}
/// Trusted composition port, not a network request or boolean ready adapter.
/// Selection is explicit even when the source has more than one registered harness entry.
pub trait TrustedNativeConfiguration: Send + Sync + 'static {
    /// # Errors
    /// Rejects an invalid accepted target or an unavailable trusted harness selection.
    fn selected_harness(&self, accepted: &AcceptedTargetProof) -> Result<EntityId, DispatchError>;
    /// # Errors
    /// Rejects incomplete or unsupported effective configuration instead of dropping unknown settings.
    fn effective(
        &self,
        definition: &NativeDefinition,
    ) -> Result<EffectiveNativeConfiguration, DispatchError>;
}
/// Read selection uses the same complete definition and effective-configuration query.
/// There is no default harness selection for viewing.
pub trait TrustedNativeReadConfiguration: TrustedNativeConfiguration {
    /// # Errors
    /// Rejects an unavailable trusted selection for this original read owner.
    fn selected_read_harness(&self, read: &ReadTargetProof) -> Result<EntityId, DispatchError>;
}
trait ConfigurationWitness: Sync {
    fn target(&self) -> EntityId;
    fn native(&self) -> &NativeSessionKey;
    fn require_current(&self) -> Result<(), DispatchError>;
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
}
pub struct SqliteNativeConfigurationProvider<C> {
    registry: SqliteRequestRegistry,
    configuration: Arc<C>,
}
impl<C: TrustedNativeConfiguration> SqliteNativeConfigurationProvider<C> {
    pub(crate) const fn new(registry: SqliteRequestRegistry, configuration: Arc<C>) -> Self {
        Self {
            registry,
            configuration,
        }
    }
}
impl<C: TrustedNativeConfiguration> ConfigurationPort for SqliteNativeConfigurationProvider<C> {
    type LaunchPlan = WindowsLaunchPlan;
    type Setup = SetupSnapshot;
    fn resolve<'a>(
        &'a self,
        proof: &'a AcceptedTargetProof,
    ) -> BoxFuture<'a, Result<ConfiguredSourcePlan<WindowsLaunchPlan, SetupSnapshot>, DispatchError>>
    {
        Box::pin(async move {
            proof.require_current()?;
            let harness = self.configuration.selected_harness(proof)?;
            self.resolve_selected(proof, harness).await
        })
    }
}
impl<C: TrustedNativeReadConfiguration> ReadConfigurationPort
    for SqliteNativeConfigurationProvider<C>
{
    fn resolve_read<'a>(
        &'a self,
        proof: &'a ReadTargetProof,
    ) -> BoxFuture<'a, Result<ConfiguredSourcePlan<WindowsLaunchPlan, SetupSnapshot>, DispatchError>>
    {
        Box::pin(async move {
            proof.require_current()?;
            let harness = self.configuration.selected_read_harness(proof)?;
            self.resolve_selected(proof, harness).await
        })
    }
    fn resolve_source_read<'a>(
        &'a self,
        _proof: &'a SourceReadSelectionProof,
    ) -> BoxFuture<
        'a,
        Result<SourceInitializationPlan<WindowsLaunchPlan, SetupSnapshot>, DispatchError>,
    > {
        Box::pin(async { Err(DispatchError::InvalidConfiguration) })
    }
}
impl<C: TrustedNativeConfiguration> SqliteNativeConfigurationProvider<C> {
    fn resolve_selected<'a>(
        &'a self,
        proof: &'a impl ConfigurationWitness,
        harness: EntityId,
    ) -> BoxFuture<'a, Result<ConfiguredSourcePlan<WindowsLaunchPlan, SetupSnapshot>, DispatchError>>
    {
        Box::pin(async move {
            proof.require_current()?;
            let repository = SqliteNativeDispatch::new(self.registry.clone());
            let deadline = RegistryDeadline::new(Instant::now() + Duration::from_secs(30));
            let mut connection = repository.begin(&deadline).await?;
            let result=async{
            let row=sqlx::query("SELECT t.source_id,t.native_id,t.workspace_id,w.canonical_path,h.executable,h.arguments_json,h.config_ref,hs.source_id AS harness_source FROM product_targets t JOIN workspaces w ON w.workspace_id=t.workspace_id JOIN harnesses h ON h.harness_id=? JOIN product_harness_sources hs ON hs.harness_id=h.harness_id WHERE t.target_id=?").bind(harness.encode()).bind(proof.target().encode()).fetch_optional(connection.connection()?).await?.ok_or(DispatchError::InvalidConfiguration)?;
            let source:String=row.try_get("source_id")?;let harness_source:String=row.try_get("harness_source")?;
            let native:String=row.try_get("native_id")?;
            if source!=proof.native().source().0.encode()||harness_source!=source||native!=proof.native().native_id(){return Err(DispatchError::InvalidSource);}
            let workspace:String=row.try_get("workspace_id")?;
            let workspace=EntityId::decode(&workspace).map_err(|_|DispatchError::CorruptState)?;
            let executable:PathBuf=PathBuf::from(row.try_get::<String,_>("executable")?);
            let cwd:PathBuf=PathBuf::from(row.try_get::<String,_>("canonical_path")?);
            if !executable.is_absolute()||!cwd.is_absolute(){return Err(DispatchError::InvalidConfiguration);}
            let arguments:String=row.try_get("arguments_json")?;
            let arguments:Vec<String>=serde_json::from_str(&arguments).map_err(|_|DispatchError::InvalidConfiguration)?;
            Ok(NativeDefinition{target:proof.target(),source:proof.native().source().0,harness,workspace,executable,arguments:arguments.into_iter().map(OsString::from).collect(),cwd,configuration_reference:row.try_get("config_ref")?})
        }.await;
            let definition = repository.finish(connection, result, &deadline).await?;
            proof.require_current()?;
            let effective = self.configuration.effective(&definition)?;
            // Unknown setup fields remain explicit. The actual negotiated bind step validates them.
            for (wire, setup) in &effective.setups {
                if setup.wire.number()
                    != u64::try_from(wire.number())
                        .map_err(|_| DispatchError::InvalidConfiguration)?
                    || setup.source.revision != effective.revision
                    || setup.source.reference.as_ref() != effective.identity.encode()
                {
                    return Err(DispatchError::InvalidConfiguration);
                }
            }
            let launch = Arc::new(WindowsLaunchPlan {
                executable: definition.executable,
                arguments: definition.arguments,
                cwd: definition.cwd,
                environment: effective.environment,
            });
            Ok(ConfiguredSourcePlan {
                source_id: definition.source,
                target: proof.target(),
                native: proof.native().clone(),
                workspace: definition.workspace,
                harness,
                configuration: ConfigurationKey {
                    identity: effective.identity,
                    revision: effective.revision,
                },
                launch,
                setups: effective.setups,
            })
        })
    }
}
