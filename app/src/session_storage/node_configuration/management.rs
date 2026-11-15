#![allow(
    clippy::needless_pass_by_value,
    clippy::option_if_let_else,
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::super::catalog_sql::{CatalogOperation, ReconcileEvidence};
use super::{
    SqliteNodeConfiguration,
    intent::{self, Body, Intent},
    protection_context,
};
use crate::{
    application::{
        native_catalog::{CatalogRepository, CurrentCatalogActor},
        native_dispatch::BoxFuture,
        request_registry::{RegistryDeadline, RegistryOwnerScope},
    },
    domain::{
        EntityId,
        native_catalog::{
            AuditWarning, CatalogAction, CatalogDraft, CatalogError, CatalogFact, CatalogFailure,
            CatalogInventory, CatalogOperationKind, CatalogReceipt, CatalogRequestId,
            CatalogResult, CatalogSqlRole, CatalogState, HarnessSummary, ProductSourceSummary,
            ProductTargetRecord, ProfileKey, ProfileSummary, TargetBindingSummary, WorkspaceRecord,
        },
    },
};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};
use std::sync::Arc;
use zeroize::Zeroizing;
#[derive(PartialEq, Eq)]
struct Command {
    request: CatalogRequestId,
    actor: EntityId,
    generation: i64,
    action: CatalogAction,
    state: CatalogState,
    at: i64,
    cipher: Option<Vec<u8>>,
    digest: Vec<u8>,
    admission: Option<i64>,
    result_kind: Option<String>,
    result_id: Option<EntityId>,
    profile: Option<ProfileKey>,
    result_at: Option<i64>,
}
fn storage(_: sqlx::Error) -> CatalogError {
    CatalogError::StorageUnavailable
}
fn corrupt(_: sqlx::Error) -> CatalogError {
    CatalogError::CorruptState
}
fn decode_id(s: impl AsRef<str>) -> Result<EntityId, CatalogError> {
    EntityId::decode(s.as_ref()).map_err(|_| CatalogError::CorruptState)
}
fn decode_u64_be(r: Vec<u8>) -> Result<u64, CatalogError> {
    Ok(u64::from_be_bytes(
        r.as_slice()
            .try_into()
            .map_err(|_| CatalogError::CorruptState)?,
    ))
}
fn key(id: Option<String>, revision: Option<Vec<u8>>) -> Result<Option<ProfileKey>, CatalogError> {
    match (id, revision) {
        (None, None) => Ok(None),
        (Some(id), Some(r)) => Ok(Some(
            ProfileKey::try_new(decode_id(id)?, decode_u64_be(r)?)
                .map_err(|_| CatalogError::CorruptState)?,
        )),
        _ => Err(CatalogError::CorruptState),
    }
}
impl Command {
    fn from_row(r: &SqliteRow) -> Result<Self, CatalogError> {
        let state: String = r.try_get("state").map_err(corrupt)?;
        let result_id: Option<String> = r.try_get("result_id").map_err(corrupt)?;
        let c = Self {
            request: CatalogRequestId::new(decode_id(
                r.try_get::<String, _>("request_id").map_err(corrupt)?,
            )?),
            actor: decode_id(r.try_get::<String, _>("actor_id").map_err(corrupt)?)?,
            generation: r.try_get("register_auth_generation").map_err(corrupt)?,
            action: intent::action(&r.try_get::<String, _>("action").map_err(corrupt)?)?,
            state: match state.as_str() {
                "registered" => CatalogState::Registered,
                "committed" => CatalogState::Committed,
                "rejected" => CatalogState::Rejected,
                _ => return Err(CatalogError::CorruptState),
            },
            at: r.try_get("registered_at_ms").map_err(corrupt)?,
            cipher: r.try_get("intent_cipher").map_err(corrupt)?,
            digest: r.try_get("intent_digest").map_err(corrupt)?,
            admission: r.try_get("admission_auth_generation").map_err(corrupt)?,
            result_kind: r.try_get("result_kind").map_err(corrupt)?,
            result_id: result_id.map(decode_id).transpose()?,
            profile: key(
                r.try_get("profile_id").map_err(corrupt)?,
                r.try_get("profile_revision").map_err(corrupt)?,
            )?,
            result_at: r.try_get("result_at_ms").map_err(corrupt)?,
        };
        if c.generation < 0 || c.admission.is_some_and(|n| n < 0) || c.digest.len() != 32 {
            return Err(CatalogError::CorruptState);
        }
        let valid = match c.state {
            CatalogState::Registered => {
                c.cipher
                    .as_ref()
                    .is_some_and(|b| Sha256::digest(b).as_slice() == c.digest)
                    && c.admission.is_none()
                    && c.result_kind.is_none()
                    && c.result_id.is_none()
                    && c.profile.is_none()
                    && c.result_at.is_none()
            }
            CatalogState::Committed => {
                c.cipher.is_none()
                    && c.admission.is_some()
                    && c.result_at.is_some()
                    && c.profile.is_some()
                    && c.result_id.is_some()
                    && c.result_kind.as_deref()
                        == Some(match c.action {
                            CatalogAction::InstallHarness => "harness",
                            CatalogAction::PublishProfile => "profile",
                            CatalogAction::BindTargetInitial => "target",
                        })
            }
            CatalogState::Rejected => {
                c.cipher.is_none()
                    && c.admission.is_some()
                    && c.result_at.is_some()
                    && c.result_kind.as_deref() == Some("rejected")
                    && c.result_id.is_none()
                    && c.profile.is_none()
            }
        };
        if !valid {
            return Err(CatalogError::CorruptState);
        }
        Ok(c)
    }
    async fn load(
        sql: &mut SqliteConnection,
        id: CatalogRequestId,
    ) -> Result<Option<Self>, CatalogError> {
        sqlx::query("SELECT * FROM main.node_catalog_commands WHERE request_id=?")
            .bind(id.encode())
            .fetch_optional(sql)
            .await
            .map_err(storage)?
            .as_ref()
            .map(Self::from_row)
            .transpose()
    }
    fn same_registration(&self, original: &Self) -> bool {
        self.request == original.request
            && self.actor == original.actor
            && self.generation == original.generation
            && self.action == original.action
            && self.at == original.at
            && self.digest == original.digest
    }
    async fn fact(&self, sql: &mut SqliteConnection) -> Result<CatalogFact, CatalogError> {
        let result = if self.state == CatalogState::Committed {
            let k = self.profile.ok_or(CatalogError::CorruptState)?;
            let p = Profile::load(sql, k).await.map_err(|e| {
                if e == CatalogError::InvalidConfiguration {
                    CatalogError::CorruptState
                } else {
                    e
                }
            })?;
            match self.action {
                CatalogAction::InstallHarness | CatalogAction::PublishProfile => {
                    if p.creator != self.request {
                        return Err(CatalogError::CorruptState);
                    }
                    if p.digest != self.digest
                        || self.result_id
                            != Some(if self.action == CatalogAction::InstallHarness {
                                p.harness
                            } else {
                                k.id()
                            })
                    {
                        return Err(CatalogError::CorruptState);
                    }
                    Some(if self.action == CatalogAction::InstallHarness {
                        CatalogResult::Harness {
                            harness: p.harness,
                            source: p.source,
                            profile: k,
                        }
                    } else {
                        CatalogResult::Profile {
                            harness: p.harness,
                            source: p.source,
                            profile: k,
                        }
                    })
                }
                CatalogAction::BindTargetInitial => {
                    let target = self.result_id.ok_or(CatalogError::CorruptState)?;
                    let binding = Binding::load(sql, target)
                        .await?
                        .ok_or(CatalogError::CorruptState)?;
                    if binding.profile != k
                        || binding.source != p.source
                        || binding.canonical != binding.registered_path
                        || binding.workspace != binding.target_workspace
                    {
                        return Err(CatalogError::CorruptState);
                    }
                    let selecting = if binding.creator == self.request {
                        None
                    } else {
                        Some(
                            Self::load(sql, binding.creator)
                                .await?
                                .ok_or(CatalogError::CorruptState)?,
                        )
                    };
                    if let Some(c) = selecting
                        && (c.state != CatalogState::Committed
                            || c.action != CatalogAction::BindTargetInitial
                            || c.result_id != Some(target)
                            || c.profile != Some(k))
                    {
                        return Err(CatalogError::CorruptState);
                    }
                    Some(CatalogResult::Target {
                        target,
                        workspace: binding.workspace,
                        harness: p.harness,
                        source: p.source,
                        profile: k,
                    })
                }
            }
        } else {
            None
        };
        Ok(CatalogFact::from_record(
            self.request,
            self.actor,
            self.action,
            self.state,
            (self.at, self.generation),
            self.admission.zip(self.result_at),
            result,
        ))
    }
}
struct Profile {
    harness: EntityId,
    source: EntityId,
    creator: CatalogRequestId,
    digest: Vec<u8>,
    cipher: Vec<u8>,
}
impl Profile {
    async fn load(sql: &mut SqliteConnection, k: ProfileKey) -> Result<Self, CatalogError> {
        let r=sqlx::query("SELECT p.harness_id,p.source_id,p.created_by_request_id,p.payload_format,p.protected_payload,p.payload_digest FROM main.node_launch_profiles p JOIN main.product_harness_sources hs ON hs.harness_id=p.harness_id AND hs.source_id=p.source_id JOIN main.harnesses h ON h.harness_id=p.harness_id JOIN main.product_sources s ON s.source_id=p.source_id WHERE p.profile_id=? AND p.revision=?").bind(k.id().encode()).bind(k.revision().to_be_bytes().to_vec()).fetch_optional(&mut *sql).await.map_err(storage)?.ok_or(CatalogError::InvalidConfiguration)?;
        let p = Self {
            harness: decode_id(r.try_get::<String, _>("harness_id").map_err(corrupt)?)?,
            source: decode_id(r.try_get::<String, _>("source_id").map_err(corrupt)?)?,
            creator: CatalogRequestId::new(decode_id(
                r.try_get::<String, _>("created_by_request_id")
                    .map_err(corrupt)?,
            )?),
            digest: r.try_get("payload_digest").map_err(corrupt)?,
            cipher: r.try_get("protected_payload").map_err(corrupt)?,
        };
        let cipher: Vec<u8> = r.try_get("protected_payload").map_err(corrupt)?;
        let creator = Command::load(sql, p.creator)
            .await?
            .ok_or(CatalogError::CorruptState)?;
        if r.try_get::<i64, _>("payload_format").map_err(corrupt)? != 1
            || p.digest.len() != 32
            || Sha256::digest(&cipher).as_slice() != p.digest
            || creator.state != CatalogState::Committed
            || creator.profile != Some(k)
            || creator.digest != p.digest
            || !matches!(
                creator.action,
                CatalogAction::InstallHarness | CatalogAction::PublishProfile
            )
            || creator.result_id
                != Some(if creator.action == CatalogAction::InstallHarness {
                    p.harness
                } else {
                    k.id()
                })
        {
            return Err(CatalogError::CorruptState);
        }
        let mixed:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_launch_profiles WHERE profile_id=? AND (harness_id<>? OR source_id<>?)").bind(k.id().encode()).bind(p.harness.encode()).bind(p.source.encode()).fetch_one(sql).await.map_err(storage)?;
        if mixed != 0 {
            return Err(CatalogError::CorruptState);
        }
        Ok(p)
    }
}
struct Binding {
    profile: ProfileKey,
    workspace: EntityId,
    target_workspace: EntityId,
    source: EntityId,
    launch_path: String,
    canonical: String,
    registered_path: String,
    creator: CatalogRequestId,
}
impl Binding {
    async fn load(
        sql: &mut SqliteConnection,
        target: EntityId,
    ) -> Result<Option<Self>, CatalogError> {
        let row=sqlx::query("SELECT s.profile_id,s.profile_revision,s.workspace_id,s.launch_path,s.observed_canonical_path,s.selected_by_request_id,t.source_id,t.workspace_id AS target_workspace,w.canonical_path FROM main.node_target_launch_selection s JOIN main.product_targets t ON t.target_id=s.target_id JOIN main.workspaces w ON w.workspace_id=s.workspace_id WHERE s.target_id=?").bind(target.encode()).fetch_optional(sql).await.map_err(storage)?;
        row.map(|r| {
            Ok(Self {
                profile: key(
                    Some(r.try_get("profile_id").map_err(corrupt)?),
                    Some(r.try_get("profile_revision").map_err(corrupt)?),
                )?
                .ok_or(CatalogError::CorruptState)?,
                workspace: decode_id(r.try_get::<String, _>("workspace_id").map_err(corrupt)?)?,
                target_workspace: decode_id(
                    r.try_get::<String, _>("target_workspace")
                        .map_err(corrupt)?,
                )?,
                source: decode_id(r.try_get::<String, _>("source_id").map_err(corrupt)?)?,
                launch_path: r.try_get("launch_path").map_err(corrupt)?,
                canonical: r.try_get("observed_canonical_path").map_err(corrupt)?,
                registered_path: r.try_get("canonical_path").map_err(corrupt)?,
                creator: CatalogRequestId::new(decode_id(
                    r.try_get::<String, _>("selected_by_request_id")
                        .map_err(corrupt)?,
                )?),
            })
        })
        .transpose()
    }
}
#[derive(PartialEq, Eq)]
struct Identity {
    node: String,
    key: Vec<u8>,
}
async fn identity(sql: &mut SqliteConnection) -> Result<Identity, CatalogError> {
    let r = sqlx::query("SELECT node_id,public_key FROM main.node_meta WHERE singleton=1")
        .fetch_optional(sql)
        .await
        .map_err(storage)?
        .ok_or(CatalogError::CorruptState)?;
    Ok(Identity {
        node: r.try_get("node_id").map_err(corrupt)?,
        key: r.try_get("public_key").map_err(corrupt)?,
    })
}
struct Keys {
    harness: EntityId,
    source: EntityId,
    profile: ProfileKey,
    binding: Option<(EntityId, EntityId, String, String)>,
}
impl Keys {
    fn from_intent(i: &Intent) -> Self {
        Self {
            harness: i.harness,
            source: i.source,
            profile: i.profile,
            binding: match &i.body {
                Body::Bind {
                    target,
                    workspace,
                    launch_path,
                    canonical,
                } => Some((*target, *workspace, launch_path.clone(), canonical.clone())),
                _ => None,
            },
        }
    }
    fn matches_fact(&self, fact: &CatalogFact) -> bool {
        match fact.result() {
            Some(
                CatalogResult::Harness {
                    harness,
                    source,
                    profile,
                }
                | CatalogResult::Profile {
                    harness,
                    source,
                    profile,
                },
            ) => {
                self.binding.is_none()
                    && *harness == self.harness
                    && *source == self.source
                    && *profile == self.profile
            }
            Some(CatalogResult::Target {
                target,
                workspace,
                harness,
                source,
                profile,
            }) => {
                self.binding
                    .as_ref()
                    .is_some_and(|(t, w, _, _)| t == target && w == workspace)
                    && *harness == self.harness
                    && *source == self.source
                    && *profile == self.profile
            }
            _ => false,
        }
    }
}

struct Evidence {
    before: Command,
    identity: Identity,
    apply: Option<(Keys, i64, i64)>,
}
impl Evidence {
    async fn settled_fact(
        &self,
        sql: &mut SqliteConnection,
        row: &Command,
    ) -> Result<CatalogFact, CatalogError> {
        if !row.same_registration(&self.before) || row.state == CatalogState::Registered {
            return Err(CatalogError::CorruptState);
        }
        let Some((keys, _, _)) = &self.apply else {
            return Err(CatalogError::CorruptState);
        };
        let fact = row.fact(sql).await?;
        if row.state == CatalogState::Rejected {
            return Ok(fact);
        }
        if !keys.matches_fact(&fact) {
            return Err(CatalogError::CorruptState);
        }
        if keys.binding.is_none() {
            let profile = Profile::load(sql, keys.profile).await?;
            if Some(profile.cipher.as_slice()) != self.before.cipher.as_deref() {
                return Err(CatalogError::CorruptState);
            }
        }
        if let Some((target, _, launch, canonical)) = &keys.binding {
            let binding = Binding::load(sql, *target)
                .await?
                .ok_or(CatalogError::CorruptState)?;
            if binding.launch_path != *launch || binding.canonical != *canonical {
                return Err(CatalogError::CorruptState);
            }
        }
        Ok(fact)
    }
}
impl ReconcileEvidence for Evidence {
    fn request_id(&self) -> CatalogRequestId {
        self.before.request
    }
    fn verify<'a>(
        &'a self,
        sql: &'a mut SqliteConnection,
    ) -> BoxFuture<'a, Result<(), CatalogError>> {
        Box::pin(async move {
            if identity(sql).await? != self.identity {
                return Err(CatalogError::CorruptState);
            }
            let row = Command::load(sql, self.before.request).await?;
            let Some(c) = row else {
                return if self.apply.is_none() {
                    Ok(())
                } else {
                    Err(CatalogError::CorruptState)
                };
            };
            if c == self.before {
                return Ok(());
            }
            let Some((_, generation, at)) = &self.apply else {
                return Err(CatalogError::CorruptState);
            };
            if c.admission != Some(*generation) || c.result_at != Some(*at) {
                return Err(CatalogError::CorruptState);
            }
            self.settled_fact(sql, &c).await?;
            Ok(())
        })
    }
}

struct WorkspaceEvidence {
    request: CatalogRequestId,
    identity: Identity,
    workspace: EntityId,
    canonical_path: String,
}
impl ReconcileEvidence for WorkspaceEvidence {
    fn request_id(&self) -> CatalogRequestId {
        self.request
    }
    fn verify<'a>(
        &'a self,
        sql: &'a mut SqliteConnection,
    ) -> BoxFuture<'a, Result<(), CatalogError>> {
        Box::pin(async move {
            if identity(sql).await? != self.identity {
                return Err(CatalogError::CorruptState);
            }
            let row =
                sqlx::query("SELECT canonical_path FROM main.workspaces WHERE workspace_id=?")
                    .bind(self.workspace.encode())
                    .fetch_optional(sql)
                    .await
                    .map_err(storage)?;
            if let Some(row) = row {
                let path: String = row.try_get("canonical_path").map_err(corrupt)?;
                if path != self.canonical_path {
                    return Err(CatalogError::CorruptState);
                }
            }
            Ok(())
        })
    }
}

struct TargetEvidence {
    request: CatalogRequestId,
    identity: Identity,
    target: EntityId,
    source: EntityId,
    native_id: String,
    workspace: EntityId,
}
impl ReconcileEvidence for TargetEvidence {
    fn request_id(&self) -> CatalogRequestId {
        self.request
    }
    fn verify<'a>(
        &'a self,
        sql: &'a mut SqliteConnection,
    ) -> BoxFuture<'a, Result<(), CatalogError>> {
        Box::pin(async move {
            if identity(sql).await? != self.identity {
                return Err(CatalogError::CorruptState);
            }
            let row = sqlx::query(
                "SELECT source_id,native_id,workspace_id FROM main.product_targets WHERE target_id=?",
            )
            .bind(self.target.encode())
            .fetch_optional(sql)
            .await
            .map_err(storage)?;
            if let Some(row) = row {
                let source = decode_id(row.try_get::<String, _>("source_id").map_err(corrupt)?)?;
                let native_id: String = row.try_get("native_id").map_err(corrupt)?;
                let workspace = row
                    .try_get::<Option<String>, _>("workspace_id")
                    .map_err(corrupt)?
                    .map(decode_id)
                    .transpose()?;
                if source != self.source
                    || native_id != self.native_id
                    || workspace != Some(self.workspace)
                {
                    return Err(CatalogError::CorruptState);
                }
            }
            Ok(())
        })
    }
}

impl SqliteNodeConfiguration {
    fn check_identity(&self, v: &Identity) -> Result<(), CatalogError> {
        if v.node == self.node_id && v.key == self.public_key {
            Ok(())
        } else {
            Err(CatalogError::CorruptState)
        }
    }
    fn check_owner(&self, actor: &CurrentCatalogActor) -> Result<(), CatalogError> {
        let own = self.registry.owner_scope();
        if !own.matches(actor.owner_scope()) {
            return Err(CatalogError::InvalidActor);
        }
        own.require_open()?;
        actor.owner_scope().require_open()?;
        Ok(())
    }
    fn check_owner_opt(&self, actor: Option<&CurrentCatalogActor>) -> Result<(), CatalogError> {
        match actor {
            Some(actor) => self.check_owner(actor),
            None => self
                .registry
                .owner_scope()
                .require_open()
                .map_err(Into::into),
        }
    }
    async fn current(
        &self,
        sql: &mut SqliteConnection,
        actor: &CurrentCatalogActor,
    ) -> Result<(), CatalogError> {
        self.check_owner(actor)?;
        let row = sqlx::query("SELECT status,auth_generation FROM main.clients WHERE client_id=?")
            .bind(actor.context().client_id.encode())
            .fetch_optional(sql)
            .await
            .map_err(storage)?
            .ok_or(CatalogError::InvalidActor)?;
        if row.try_get::<String, _>("status").map_err(corrupt)? != "trusted"
            || row.try_get::<i64, _>("auth_generation").map_err(corrupt)?
                != actor.context().auth_generation
            || actor.context().auth_generation < 0
        {
            return Err(CatalogError::InvalidActor);
        }
        Ok(())
    }
    async fn ensure_actor_if_present(
        &self,
        sql: &mut SqliteConnection,
        actor: Option<&CurrentCatalogActor>,
    ) -> Result<(), CatalogError> {
        match actor {
            Some(actor) => self.current(sql, actor).await,
            None => self
                .registry
                .owner_scope()
                .require_open()
                .map_err(Into::into),
        }
    }
    async fn start(
        &self,
        actor: &CurrentCatalogActor,
        kind: CatalogOperationKind,
        d: &RegistryDeadline,
    ) -> Result<
        (
            super::super::catalog_sql::CatalogGuard,
            super::super::registry_connection_owner::ConnectionLease,
        ),
        CatalogFailure,
    > {
        self.start_opt(Some(actor), kind, d).await
    }
    async fn start_opt(
        &self,
        actor: Option<&CurrentCatalogActor>,
        kind: CatalogOperationKind,
        d: &RegistryDeadline,
    ) -> Result<
        (
            super::super::catalog_sql::CatalogGuard,
            super::super::registry_connection_owner::ConnectionLease,
        ),
        CatalogFailure,
    > {
        self.check_owner_opt(actor)
            .map_err(|e| crate::application::native_catalog::failure(kind, e))?;
        let lease = self
            .registry
            .catalog_operation_lease()
            .map_err(|e| crate::application::native_catalog::failure(kind, e.into()))?;
        let scope = match actor {
            Some(actor) => actor.owner_scope(),
            None => &self.registry.owner_scope(),
        };
        let guard = self.fence.enter(kind, scope, d).await?;
        Ok((guard, lease))
    }
    fn final_fence(
        &self,
        actor: &CurrentCatalogActor,
        d: &RegistryDeadline,
    ) -> Result<(), CatalogError> {
        self.final_fence_opt(Some(actor), d)
    }
    fn final_fence_opt(
        &self,
        actor: Option<&CurrentCatalogActor>,
        d: &RegistryDeadline,
    ) -> Result<(), CatalogError> {
        self.check_owner_opt(actor)?;
        if d.is_cancelled() || tokio::time::Instant::now() >= d.deadline() {
            return Err(CatalogError::Cancelled);
        }
        Ok(())
    }
    async fn read_identity(
        &self,
        actor: &CurrentCatalogActor,
        op: &Arc<CatalogOperation>,
        d: &RegistryDeadline,
    ) -> Result<Identity, CatalogError> {
        let mut c = self
            .registry
            .acquire_catalog_connection(op.clone(), CatalogSqlRole::IdentityRead)
            .await?;
        c.begin_read().await?;
        let result = async {
            self.current(c.sql()?, actor).await?;
            let v = identity(c.sql()?).await?;
            self.check_identity(&v)?;
            Ok(v)
        }
        .await;
        c.finish_catalog(result, d).await
    }
    async fn register_inner(
        &self,
        actor: &CurrentCatalogActor,
        draft: CatalogDraft,
        at: i64,
        d: &RegistryDeadline,
        op: &Arc<CatalogOperation>,
    ) -> Result<CatalogRequestId, CatalogError> {
        let request = CatalogRequestId::new(intent::random_id()?);
        op.set_request(request);
        let identity = self.read_identity(actor, op, d).await?;
        let intent = Intent::normalize(draft)?;
        let plaintext = intent.encode(
            request,
            actor.context().client_id,
            actor.context().auth_generation,
        )?;
        let context = protection_context(&identity.node, &identity.key, request)?;
        let cipher = self
            .protector
            .protect(&plaintext, &context)
            .map_err(|_| CatalogError::ProtectionUnavailable)?;
        self.final_fence(actor, d)?;
        if cipher.is_empty() {
            return Err(CatalogError::ProtectionUnavailable);
        }
        let digest = Sha256::digest(&cipher).to_vec();
        let evidence = Arc::new(Evidence {
            before: Command {
                request,
                actor: actor.context().client_id,
                generation: actor.context().auth_generation,
                action: intent.action,
                state: CatalogState::Registered,
                at,
                cipher: Some(cipher),
                digest,
                admission: None,
                result_kind: None,
                result_id: None,
                profile: None,
                result_at: None,
            },
            identity,
            apply: None,
        });
        op.set_effect(evidence.clone());
        let mut c = self
            .registry
            .acquire_catalog_connection(op.clone(), CatalogSqlRole::RegisterWrite)
            .await?;
        c.begin_write().await?;
        let result = async {
            self.current(c.sql()?, actor).await?;
            let v = identity_value(c.sql()?).await?;
            self.check_identity(&v)?;
            if v != evidence.identity {
                return Err(CatalogError::Conflict);
            }
            validate(c.sql()?, &intent).await?;
            let row = &evidence.before;
            sqlx::query("INSERT INTO main.node_catalog_commands(request_id,actor_id,register_auth_generation,action,state,registered_at_ms,intent_cipher,intent_digest) VALUES(?,?,?,?,'registered',?,?,?)")
                .bind(request.encode())
                .bind(row.actor.encode())
                .bind(row.generation)
                .bind(intent::label(row.action))
                .bind(at)
                .bind(row.cipher.as_deref())
                .bind(&row.digest)
                .execute(c.sql()?)
                .await
                .map_err(storage)?;
            Ok(request)
        }
        .await;
        c.finish_catalog(result, d).await
    }
    async fn apply_inner(
        &self,
        actor: &CurrentCatalogActor,
        id: CatalogRequestId,
        at: i64,
        d: &RegistryDeadline,
        op: &Arc<CatalogOperation>,
    ) -> Result<CatalogFact, CatalogError> {
        op.set_request(id);
        let mut c = self
            .registry
            .acquire_catalog_connection(op.clone(), CatalogSqlRole::RegisteredSnapshot)
            .await?;
        c.begin_read().await?;
        let result = async {
            self.current(c.sql()?, actor).await?;
            let identity = identity(c.sql()?).await?;
            self.check_identity(&identity)?;
            let row = Command::load(c.sql()?, id)
                .await?
                .ok_or(CatalogError::UnknownRequest)?;
            if row.actor != actor.context().client_id {
                return Err(CatalogError::InvalidActor);
            }
            let fact = if row.state == CatalogState::Registered {
                None
            } else {
                Some(row.fact(c.sql()?).await?)
            };
            Ok((row, identity, fact))
        }
        .await;
        let (before, identity, settled) = c.finish_catalog(result, d).await?;
        if let Some(fact) = settled {
            return Ok(fact);
        }
        let context = protection_context(&identity.node, &identity.key, id)?;
        let bytes = Zeroizing::new(
            self.protector
                .unprotect(
                    before.cipher.as_deref().ok_or(CatalogError::CorruptState)?,
                    &context,
                )
                .map_err(|_| CatalogError::ProtectionUnavailable)?,
        );
        let intent = Intent::decode(
            bytes.to_vec(),
            id,
            before.actor,
            before.generation,
            before.action,
        )?;
        self.final_fence(actor, d)?;
        let evidence = Arc::new(Evidence {
            before,
            identity,
            apply: Some((
                Keys::from_intent(&intent),
                actor.context().auth_generation,
                at,
            )),
        });
        op.set_effect(evidence.clone());
        let mut c = self
            .registry
            .acquire_catalog_connection(op.clone(), CatalogSqlRole::ApplyWrite)
            .await?;
        c.begin_write().await?;
        let result = async {
            self.current(c.sql()?, actor).await?;
            let v = identity_value(c.sql()?).await?;
            self.check_identity(&v)?;
            if v != evidence.identity {
                return Err(CatalogError::Conflict);
            }
            let row = Command::load(c.sql()?, id)
                .await?
                .ok_or(CatalogError::CorruptState)?;
            if row.actor != actor.context().client_id {
                return Err(CatalogError::InvalidActor);
            }
            if row.state != CatalogState::Registered {
                return evidence.settled_fact(c.sql()?, &row).await;
            }
            if row != evidence.before {
                return Err(CatalogError::Conflict);
            }
            match validate(c.sql()?, &intent).await {
                Ok(()) => {
                    apply_effect(c.sql()?, &intent, &row, at).await?;
                    let (kind, result_id) = match &intent.body {
                        Body::Install { .. } => ("harness", intent.harness),
                        Body::Publish { .. } => ("profile", intent.profile.id()),
                        Body::Bind { target, .. } => ("target", *target),
                    };
                    sqlx::query("UPDATE main.node_catalog_commands SET state='committed',intent_cipher=NULL,admission_auth_generation=?,result_kind=?,result_id=?,profile_id=?,profile_revision=?,result_at_ms=? WHERE request_id=? AND state='registered'")
                        .bind(actor.context().auth_generation)
                        .bind(kind)
                        .bind(result_id.encode())
                        .bind(intent.profile.id().encode())
                        .bind(intent.profile.revision().to_be_bytes().to_vec())
                        .bind(at)
                        .bind(id.encode())
                        .execute(c.sql()?)
                        .await
                        .map_err(storage)?;
                }
                Err(
                    CatalogError::Conflict
                    | CatalogError::InvalidSource
                    | CatalogError::InvalidTarget
                    | CatalogError::InvalidConfiguration,
                ) => {
                    sqlx::query("UPDATE main.node_catalog_commands SET state='rejected',intent_cipher=NULL,admission_auth_generation=?,result_kind='rejected',result_at_ms=? WHERE request_id=? AND state='registered'")
                        .bind(actor.context().auth_generation)
                        .bind(at)
                        .bind(id.encode())
                        .execute(c.sql()?)
                        .await
                        .map_err(storage)?;
                }
                Err(e) => return Err(e),
            }
            Command::load(c.sql()?, id)
                .await?
                .ok_or(CatalogError::CorruptState)?
                .fact(c.sql()?)
                .await
        }
        .await;
        c.finish_catalog(result, d).await
    }
    async fn query_inner(
        &self,
        actor: &CurrentCatalogActor,
        id: CatalogRequestId,
        d: &RegistryDeadline,
        op: &Arc<CatalogOperation>,
    ) -> Result<CatalogFact, CatalogError> {
        op.set_request(id);
        let mut c = self
            .registry
            .acquire_catalog_connection(op.clone(), CatalogSqlRole::QueryRead)
            .await?;
        c.begin_read().await?;
        let result = async {
            self.current(c.sql()?, actor).await?;
            self.check_identity(&identity(c.sql()?).await?)?;
            let row = Command::load(c.sql()?, id)
                .await?
                .ok_or(CatalogError::UnknownRequest)?;
            if row.actor != actor.context().client_id {
                return Err(CatalogError::InvalidActor);
            }
            row.fact(c.sql()?).await
        }
        .await;
        c.finish_catalog(result, d).await
    }

    async fn register_workspace_inner(
        &self,
        actor: Option<&CurrentCatalogActor>,
        path: &str,
        d: &RegistryDeadline,
        op: &Arc<CatalogOperation>,
    ) -> Result<WorkspaceRecord, CatalogError> {
        let canonical_path = super::source_sessions::verify_directory_on_disk(path).await?;
        self.final_fence_opt(actor, d)?;
        let workspace_id = intent::random_id()?;
        let evidence = Arc::new(WorkspaceEvidence {
            request: CatalogRequestId::new(workspace_id),
            identity: Identity {
                node: self.node_id.clone(),
                key: self.public_key.clone(),
            },
            workspace: workspace_id,
            canonical_path: canonical_path.clone(),
        });
        op.set_effect(evidence);
        let mut c = self
            .registry
            .acquire_catalog_connection(op.clone(), CatalogSqlRole::RegisterWrite)
            .await?;
        c.begin_write().await?;
        let result = async {
            self.ensure_actor_if_present(c.sql()?, actor).await?;
            let v = identity_value(c.sql()?).await?;
            self.check_identity(&v)?;
            if let Some(existing_id) = sqlx::query_scalar::<_, String>(
                "SELECT workspace_id FROM main.workspaces WHERE canonical_path=? ORDER BY workspace_id ASC LIMIT 1",
            )
            .bind(&canonical_path)
            .fetch_optional(c.sql()?)
            .await
            .map_err(storage)?
            {
                return Ok(WorkspaceRecord {
                    workspace_id: decode_id(existing_id)?,
                    canonical_path,
                });
            }
            sqlx::query("INSERT INTO main.workspaces(workspace_id,canonical_path) VALUES(?,?)")
                .bind(workspace_id.encode())
                .bind(&canonical_path)
                .execute(c.sql()?)
                .await
                .map_err(storage)?;
            Ok(WorkspaceRecord {
                workspace_id,
                canonical_path,
            })
        }
        .await;
        c.finish_catalog(result, d).await
    }

    async fn register_target_inner(
        &self,
        actor: Option<&CurrentCatalogActor>,
        source: EntityId,
        native_session_id: &str,
        workspace: EntityId,
        d: &RegistryDeadline,
        op: &Arc<CatalogOperation>,
    ) -> Result<ProductTargetRecord, CatalogError> {
        if native_session_id.is_empty()
            || native_session_id.len() > 4096
            || native_session_id.contains('\0')
        {
            return Err(CatalogError::InvalidTarget);
        }
        let mut c = self
            .registry
            .acquire_catalog_connection(op.clone(), CatalogSqlRole::QueryRead)
            .await?;
        c.begin_read().await?;
        let precheck = async {
            self.ensure_actor_if_present(c.sql()?, actor).await?;
            let v = identity(c.sql()?).await?;
            self.check_identity(&v)?;
            let source_exists = sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM main.product_sources WHERE source_id=?",
            )
            .bind(source.encode())
            .fetch_one(c.sql()?)
            .await
            .map_err(storage)?;
            if source_exists != 1 {
                return Err(CatalogError::InvalidSource);
            }
            let workspace_path = sqlx::query_scalar::<_, String>(
                "SELECT canonical_path FROM main.workspaces WHERE workspace_id=?",
            )
            .bind(workspace.encode())
            .fetch_optional(c.sql()?)
            .await
            .map_err(storage)?
            .ok_or(CatalogError::InvalidConfiguration)?;
            let existing = sqlx::query(
                "SELECT t.target_id,t.workspace_id,s.profile_id,s.profile_revision,s.workspace_id AS binding_workspace_id,s.launch_path,s.observed_canonical_path FROM main.product_targets t LEFT JOIN main.node_target_launch_selection s ON s.target_id=t.target_id WHERE t.source_id=? AND t.native_id=?",
            )
            .bind(source.encode())
            .bind(native_session_id)
            .fetch_optional(c.sql()?)
            .await
            .map_err(storage)?;
            let existing = existing
                .map(|r| {
                    let existing_workspace = r
                        .try_get::<Option<String>, _>("workspace_id")
                        .map_err(corrupt)?
                        .map(decode_id)
                        .transpose()?;
                    if existing_workspace != Some(workspace) {
                        return Err(CatalogError::Conflict);
                    }
                    let target_id = decode_id(r.try_get::<String, _>("target_id").map_err(corrupt)?)?;
                    let profile_id: Option<String> = r.try_get("profile_id").map_err(corrupt)?;
                    let binding = match profile_id {
                        Some(pid) => {
                            let rev = decode_u64_be(r.try_get("profile_revision").map_err(corrupt)?)?;
                            let bws = decode_id(
                                r.try_get::<String, _>("binding_workspace_id")
                                    .map_err(corrupt)?,
                            )?;
                            Some(TargetBindingSummary {
                                profile_id: decode_id(pid)?,
                                profile_revision: rev,
                                workspace_id: bws,
                                launch_path: r.try_get("launch_path").map_err(corrupt)?,
                                observed_canonical_path: r
                                    .try_get("observed_canonical_path")
                                    .map_err(corrupt)?,
                            })
                        }
                        None => None,
                    };
                    Ok(ProductTargetRecord {
                        target_id,
                        source_id: source,
                        native_session_id: native_session_id.to_owned(),
                        workspace_id: Some(workspace),
                        binding,
                    })
                })
                .transpose()?;
            Ok((v, workspace_path, existing))
        }
        .await;
        let (v, workspace_path, existing) = c.finish_catalog(precheck, d).await?;
        let _ = super::source_sessions::verify_directory_on_disk(&workspace_path).await?;
        if let Some(record) = existing {
            return Ok(record);
        }
        self.final_fence_opt(actor, d)?;
        let target_id = intent::random_id()?;
        let evidence = Arc::new(TargetEvidence {
            request: CatalogRequestId::new(target_id),
            identity: v,
            target: target_id,
            source,
            native_id: native_session_id.to_owned(),
            workspace,
        });
        op.set_effect(evidence.clone());
        let mut c = self
            .registry
            .acquire_catalog_connection(op.clone(), CatalogSqlRole::RegisterWrite)
            .await?;
        c.begin_write().await?;
        let result = async {
            self.ensure_actor_if_present(c.sql()?, actor).await?;
            let current_id = identity_value(c.sql()?).await?;
            self.check_identity(&current_id)?;
            if current_id != evidence.identity {
                return Err(CatalogError::Conflict);
            }
            let source_exists = sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM main.product_sources WHERE source_id=?",
            )
            .bind(source.encode())
            .fetch_one(c.sql()?)
            .await
            .map_err(storage)?;
            if source_exists != 1 {
                return Err(CatalogError::InvalidSource);
            }
            let workspace_exists = sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM main.workspaces WHERE workspace_id=?",
            )
            .bind(workspace.encode())
            .fetch_one(c.sql()?)
            .await
            .map_err(storage)?;
            if workspace_exists != 1 {
                return Err(CatalogError::InvalidConfiguration);
            }
            sqlx::query(
                "INSERT INTO main.product_targets(target_id,source_id,native_id,workspace_id) VALUES(?,?,?,?)",
            )
            .bind(target_id.encode())
            .bind(source.encode())
            .bind(native_session_id)
            .bind(workspace.encode())
            .execute(c.sql()?)
            .await
            .map_err(storage)?;
            Ok(ProductTargetRecord {
                target_id,
                source_id: source,
                native_session_id: native_session_id.to_owned(),
                workspace_id: Some(workspace),
                binding: None,
            })
        }
        .await;
        c.finish_catalog(result, d).await
    }

    async fn catalog_inventory_inner(
        &self,
        actor: Option<&CurrentCatalogActor>,
        d: &RegistryDeadline,
        op: &Arc<CatalogOperation>,
    ) -> Result<CatalogInventory, CatalogError> {
        let mut c = self
            .registry
            .acquire_catalog_connection(op.clone(), CatalogSqlRole::QueryRead)
            .await?;
        c.begin_read().await?;
        let result = async {
            self.ensure_actor_if_present(c.sql()?, actor).await?;
            self.check_identity(&identity(c.sql()?).await?)?;
            let ws_rows = sqlx::query(
                "SELECT workspace_id,canonical_path FROM main.workspaces ORDER BY workspace_id ASC",
            )
            .fetch_all(c.sql()?)
            .await
            .map_err(storage)?;
            let mut workspaces = Vec::with_capacity(ws_rows.len());
            for row in ws_rows {
                workspaces.push(WorkspaceRecord {
                    workspace_id: decode_id(
                        row.try_get::<String, _>("workspace_id").map_err(corrupt)?,
                    )?,
                    canonical_path: row.try_get("canonical_path").map_err(corrupt)?,
                });
            }

            let src_rows = sqlx::query(
                "SELECT source_id,name FROM main.product_sources ORDER BY source_id ASC",
            )
            .fetch_all(c.sql()?)
            .await
            .map_err(storage)?;
            let mut sources = Vec::with_capacity(src_rows.len());
            for row in src_rows {
                sources.push(ProductSourceSummary {
                    source_id: decode_id(row.try_get::<String, _>("source_id").map_err(corrupt)?)?,
                    name: row.try_get("name").map_err(corrupt)?,
                });
            }

            let harness_rows = sqlx::query(
                "SELECT h.harness_id,h.name,hs.source_id FROM main.harnesses h JOIN main.product_harness_sources hs ON hs.harness_id=h.harness_id ORDER BY h.harness_id ASC",
            )
            .fetch_all(c.sql()?)
            .await
            .map_err(storage)?;
            let mut harnesses = Vec::with_capacity(harness_rows.len());
            for row in harness_rows {
                harnesses.push(HarnessSummary {
                    harness_id: decode_id(
                        row.try_get::<String, _>("harness_id").map_err(corrupt)?,
                    )?,
                    name: row.try_get("name").map_err(corrupt)?,
                    source_id: decode_id(row.try_get::<String, _>("source_id").map_err(corrupt)?)?,
                });
            }

            let profile_rows = sqlx::query(
                "SELECT profile_id,revision,harness_id,source_id FROM main.node_launch_profiles ORDER BY profile_id ASC, revision ASC",
            )
            .fetch_all(c.sql()?)
            .await
            .map_err(storage)?;
            let mut profiles = Vec::with_capacity(profile_rows.len());
            for row in profile_rows {
                profiles.push(ProfileSummary {
                    profile_id: decode_id(
                        row.try_get::<String, _>("profile_id").map_err(corrupt)?,
                    )?,
                    revision: decode_u64_be(row.try_get("revision").map_err(corrupt)?)?,
                    harness_id: decode_id(
                        row.try_get::<String, _>("harness_id").map_err(corrupt)?,
                    )?,
                    source_id: decode_id(row.try_get::<String, _>("source_id").map_err(corrupt)?)?,
                });
            }

            let target_rows = sqlx::query(
                "SELECT t.target_id,t.source_id,t.native_id,t.workspace_id,s.profile_id,s.profile_revision,s.workspace_id AS binding_workspace_id,s.launch_path,s.observed_canonical_path FROM main.product_targets t LEFT JOIN main.node_target_launch_selection s ON s.target_id=t.target_id ORDER BY t.target_id ASC",
            )
            .fetch_all(c.sql()?)
            .await
            .map_err(storage)?;
            let mut targets = Vec::with_capacity(target_rows.len());
            for row in target_rows {
                let profile_id: Option<String> = row.try_get("profile_id").map_err(corrupt)?;
                let binding = match profile_id {
                    Some(pid) => {
                        let rev = decode_u64_be(row.try_get("profile_revision").map_err(corrupt)?)?;
                        let bws = decode_id(
                            row.try_get::<String, _>("binding_workspace_id")
                                .map_err(corrupt)?,
                        )?;
                        Some(TargetBindingSummary {
                            profile_id: decode_id(pid)?,
                            profile_revision: rev,
                            workspace_id: bws,
                            launch_path: row.try_get("launch_path").map_err(corrupt)?,
                            observed_canonical_path: row
                                .try_get("observed_canonical_path")
                                .map_err(corrupt)?,
                        })
                    }
                    None => None,
                };
                targets.push(ProductTargetRecord {
                    target_id: decode_id(row.try_get::<String, _>("target_id").map_err(corrupt)?)?,
                    source_id: decode_id(row.try_get::<String, _>("source_id").map_err(corrupt)?)?,
                    native_session_id: row.try_get("native_id").map_err(corrupt)?,
                    workspace_id: row
                        .try_get::<Option<String>, _>("workspace_id")
                        .map_err(corrupt)?
                        .map(decode_id)
                        .transpose()?,
                    binding,
                });
            }

            Ok(CatalogInventory {
                workspaces,
                sources,
                harnesses,
                profiles,
                targets,
            })
        }
        .await;
        c.finish_catalog(result, d).await
    }

    /// Registers an existing absolute directory on disk as a workspace on behalf of the verified local OS owner.
    ///
    /// # Errors
    /// Returns a catalog failure if the directory does not exist, storage is unavailable, or the node is stopping.
    pub async fn register_workspace_owner(
        &self,
        path: &str,
        d: &RegistryDeadline,
    ) -> Result<WorkspaceRecord, CatalogFailure> {
        <Self as CatalogRepository>::register_workspace(self, None, path, d).await
    }

    /// Registers a product target for an existing source and workspace on behalf of the verified local OS owner.
    ///
    /// # Errors
    /// Returns a catalog failure on invalid source, missing workspace/directory, conflict, or storage errors.
    pub async fn register_target_owner(
        &self,
        source: EntityId,
        native_session_id: &str,
        workspace: EntityId,
        d: &RegistryDeadline,
    ) -> Result<ProductTargetRecord, CatalogFailure> {
        <Self as CatalogRepository>::register_target(
            self,
            None,
            source,
            native_session_id,
            workspace,
            d,
        )
        .await
    }

    /// Reads the safe catalog inventory (no secrets or profile payloads) on behalf of the verified local OS owner.
    ///
    /// # Errors
    /// Returns a catalog failure if storage is unavailable or the node is stopping.
    pub async fn catalog_inventory_owner(
        &self,
        d: &RegistryDeadline,
    ) -> Result<CatalogInventory, CatalogFailure> {
        <Self as CatalogRepository>::catalog_inventory(self, None, d).await
    }
}
async fn identity_value(sql: &mut SqliteConnection) -> Result<Identity, CatalogError> {
    identity(sql).await
}
impl CatalogRepository for SqliteNodeConfiguration {
    async fn observe_workspace(
        &self,
        actor: &CurrentCatalogActor,
        target: EntityId,
        source: EntityId,
        workspace: EntityId,
        d: &RegistryDeadline,
    ) -> Result<crate::application::native_catalog::CatalogWorkspaceRow, CatalogFailure> {
        let (guard, _lease) = self.start(actor, CatalogOperationKind::Query, d).await?;
        let op = guard.operation.clone();
        let scope = self.registry.owner_scope();
        let result = d
            .run(scope.while_open(async {
                Ok(async {
                    let mut c = self
                        .registry
                        .acquire_catalog_connection(op.clone(), CatalogSqlRole::QueryRead)
                        .await?;
                    c.begin_read().await?;
                    let result = async {
                        self.current(c.sql()?, actor).await?;
                        let original = identity(c.sql()?).await?;
                        self.check_identity(&original)?;
                        let r = sqlx::query("SELECT t.source_id,t.workspace_id,w.canonical_path FROM main.product_targets t JOIN main.workspaces w ON w.workspace_id=t.workspace_id WHERE t.target_id=?")
                            .bind(target.encode())
                            .fetch_optional(c.sql()?)
                            .await
                            .map_err(storage)?
                            .ok_or(CatalogError::InvalidTarget)?;
                        if decode_id(r.try_get::<String, _>("source_id").map_err(corrupt)?)?
                            != source
                        {
                            return Err(CatalogError::InvalidSource);
                        }
                        if decode_id(r.try_get::<String, _>("workspace_id").map_err(corrupt)?)?
                            != workspace
                        {
                            return Err(CatalogError::InvalidTarget);
                        }
                        Ok(crate::application::native_catalog::CatalogWorkspaceRow {
                            target,
                            source,
                            workspace,
                            canonical: r.try_get("canonical_path").map_err(corrupt)?,
                        })
                    }
                    .await;
                    let row = c.finish_catalog(result, d).await?;
                    let _ = super::source_sessions::verify_directory_on_disk(&row.canonical).await?;
                    Ok(row)
                }
                .await)
            }))
            .await;
        let result = match result {
            Ok(r) => r,
            Err(e) => Err(e.into()),
        }
        .and_then(|r| self.final_fence(actor, d).map(|()| r))
        .map_err(|e| op.failure(e));
        drop(guard);
        result
    }

    async fn register_workspace(
        &self,
        actor: Option<&CurrentCatalogActor>,
        path: &str,
        d: &RegistryDeadline,
    ) -> Result<WorkspaceRecord, CatalogFailure> {
        let (guard, _lease) = self
            .start_opt(actor, CatalogOperationKind::Register, d)
            .await?;
        let op = guard.operation.clone();
        let scope = self.registry.owner_scope();
        let result =
            d.run(scope.while_open(async {
                Ok(self.register_workspace_inner(actor, path, d, &op).await)
            }))
            .await;
        let value = match result {
            Ok(v) => v,
            Err(e) => Err(e.into()),
        }
        .and_then(|v| self.final_fence_opt(actor, d).map(|()| v))
        .map_err(|e| op.failure(e));
        drop(guard);
        value
    }

    async fn register_target(
        &self,
        actor: Option<&CurrentCatalogActor>,
        source: EntityId,
        native_session_id: &str,
        workspace: EntityId,
        d: &RegistryDeadline,
    ) -> Result<ProductTargetRecord, CatalogFailure> {
        let (guard, _lease) = self
            .start_opt(actor, CatalogOperationKind::Register, d)
            .await?;
        let op = guard.operation.clone();
        let scope = self.registry.owner_scope();
        let result = d
            .run(scope.while_open(async {
                Ok(self
                    .register_target_inner(actor, source, native_session_id, workspace, d, &op)
                    .await)
            }))
            .await;
        let value = match result {
            Ok(v) => v,
            Err(e) => Err(e.into()),
        }
        .and_then(|v| self.final_fence_opt(actor, d).map(|()| v))
        .map_err(|e| op.failure(e));
        drop(guard);
        value
    }

    async fn catalog_inventory(
        &self,
        actor: Option<&CurrentCatalogActor>,
        d: &RegistryDeadline,
    ) -> Result<CatalogInventory, CatalogFailure> {
        let (guard, _lease) = self
            .start_opt(actor, CatalogOperationKind::Query, d)
            .await?;
        let op = guard.operation.clone();
        let scope = self.registry.owner_scope();
        let result = d
            .run(scope.while_open(async { Ok(self.catalog_inventory_inner(actor, d, &op).await) }))
            .await;
        let value = match result {
            Ok(v) => v,
            Err(e) => Err(e.into()),
        }
        .and_then(|v| self.final_fence_opt(actor, d).map(|()| v))
        .map_err(|e| op.failure(e));
        drop(guard);
        value
    }

    fn owner_scope(&self) -> RegistryOwnerScope {
        self.registry.owner_scope()
    }
    async fn register(
        &self,
        actor: &CurrentCatalogActor,
        draft: CatalogDraft,
        at: i64,
        d: &RegistryDeadline,
    ) -> Result<CatalogRequestId, CatalogFailure> {
        let (guard, _lease) = self.start(actor, CatalogOperationKind::Register, d).await?;
        let op = guard.operation.clone();
        let scope = self.registry.owner_scope();
        let result = d
            .run(
                scope.while_open(async { Ok(self.register_inner(actor, draft, at, d, &op).await) }),
            )
            .await;
        let value = match result {
            Ok(v) => v,
            Err(e) => Err(e.into()),
        }
        .and_then(|v| self.final_fence(actor, d).map(|()| v))
        .map_err(|e| op.failure(e));
        drop(guard);
        value
    }
    async fn apply(
        &self,
        actor: &CurrentCatalogActor,
        id: CatalogRequestId,
        at: i64,
        d: &RegistryDeadline,
    ) -> Result<CatalogReceipt, CatalogFailure> {
        let (guard, _lease) = self.start(actor, CatalogOperationKind::Apply, d).await?;
        let op = guard.operation.clone();
        let scope = self.registry.owner_scope();
        let result = d
            .run(scope.while_open(async { Ok(self.apply_inner(actor, id, at, d, &op).await) }))
            .await;
        let value = match result {
            Ok(v) => v,
            Err(e) => Err(e.into()),
        }
        .and_then(|v| {
            self.final_fence(actor, d)
                .map(|()| CatalogReceipt::new(v, AuditWarning::Unavailable, op.observation()))
        })
        .map_err(|e| op.failure(e));
        drop(guard);
        value
    }
    async fn query(
        &self,
        actor: &CurrentCatalogActor,
        id: CatalogRequestId,
        d: &RegistryDeadline,
    ) -> Result<CatalogFact, CatalogFailure> {
        let (guard, _lease) = self.start(actor, CatalogOperationKind::Query, d).await?;
        let op = guard.operation.clone();
        let scope = self.registry.owner_scope();
        let result = d
            .run(scope.while_open(async { Ok(self.query_inner(actor, id, d, &op).await) }))
            .await;
        let value = match result {
            Ok(v) => v,
            Err(e) => Err(e.into()),
        }
        .and_then(|v| self.final_fence(actor, d).map(|()| v))
        .map_err(|e| op.failure(e));
        drop(guard);
        value
    }
    async fn reconcile(&self, d: &RegistryDeadline) -> Result<(), CatalogFailure> {
        self.fence.recover(&self.registry, d).await
    }
}
async fn validate(sql: &mut SqliteConnection, i: &Intent) -> Result<(), CatalogError> {
    match &i.body {
        Body::Install { source_name, .. } => {
            let source = sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM main.product_sources WHERE source_id=?",
            )
            .bind(i.source.encode())
            .fetch_one(&mut *sql)
            .await
            .map_err(storage)?;
            if (source_name.is_none() && source != 1) || (source_name.is_some() && source != 0) {
                return Err(CatalogError::InvalidSource);
            }
            if sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM main.harnesses WHERE harness_id=?",
            )
            .bind(i.harness.encode())
            .fetch_one(&mut *sql)
            .await
            .map_err(storage)?
                != 0
            {
                return Err(CatalogError::Conflict);
            }
            if sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM main.node_launch_profiles WHERE profile_id=?",
            )
            .bind(i.profile.id().encode())
            .fetch_one(&mut *sql)
            .await
            .map_err(storage)?
                != 0
            {
                return Err(CatalogError::Conflict);
            }
        }
        Body::Publish { expected } => validate_publication(sql, i, expected.as_ref()).await?,
        Body::Bind {
            target,
            workspace,
            launch_path,
            canonical,
        } => {
            let p = Profile::load(sql, i.profile).await?;
            if p.harness != i.harness || p.source != i.source {
                return Err(CatalogError::InvalidSource);
            }
            let row=sqlx::query("SELECT t.source_id,t.workspace_id,w.canonical_path FROM main.product_targets t JOIN main.workspaces w ON w.workspace_id=t.workspace_id WHERE t.target_id=?").bind(target.encode()).fetch_optional(&mut *sql).await.map_err(storage)?.ok_or(CatalogError::InvalidTarget)?;
            if decode_id(row.try_get::<String, _>("source_id").map_err(corrupt)?)? != i.source
                || decode_id(row.try_get::<String, _>("workspace_id").map_err(corrupt)?)?
                    != *workspace
                || row
                    .try_get::<String, _>("canonical_path")
                    .map_err(corrupt)?
                    != *canonical
            {
                return Err(CatalogError::InvalidTarget);
            }
            if let Some(b) = Binding::load(sql, *target).await?
                && (b.profile != i.profile
                    || b.workspace != *workspace
                    || b.launch_path != *launch_path
                    || b.canonical != *canonical)
            {
                return Err(CatalogError::Conflict);
            }
        }
    }
    for (workspace, path) in &i.directories {
        let registered = sqlx::query_scalar::<_, String>(
            "SELECT canonical_path FROM main.workspaces WHERE workspace_id=?",
        )
        .bind(workspace.encode())
        .fetch_optional(&mut *sql)
        .await
        .map_err(storage)?;
        if registered.as_deref() != Some(path) {
            return Err(CatalogError::InvalidConfiguration);
        }
    }
    Ok(())
}
async fn apply_effect(
    sql: &mut SqliteConnection,
    i: &Intent,
    row: &Command,
    at: i64,
) -> Result<(), CatalogError> {
    match &i.body {
        Body::Install { name, source_name } => {
            if let Some(name) = source_name {
                sqlx::query("INSERT INTO main.product_sources(source_id,name) VALUES(?,?)")
                    .bind(i.source.encode())
                    .bind(name)
                    .execute(&mut *sql)
                    .await
                    .map_err(storage)?;
            }
            let reference = format!("node-profile-v1:{}", i.profile.id().encode());
            let arguments = serde_json::to_string(&reference)
                .map_err(|_| CatalogError::InvalidConfiguration)?;
            sqlx::query("INSERT INTO main.harnesses(harness_id,name,executable,arguments_json,config_ref) VALUES(?,?,?,?,?)").bind(i.harness.encode()).bind(name).bind(&reference).bind(arguments).bind(&reference).execute(&mut *sql).await.map_err(storage)?;
            sqlx::query(
                "INSERT INTO main.product_harness_sources(harness_id,source_id) VALUES(?,?)",
            )
            .bind(i.harness.encode())
            .bind(i.source.encode())
            .execute(&mut *sql)
            .await
            .map_err(storage)?;
        }
        Body::Publish { .. } => {}
        Body::Bind {
            target,
            workspace,
            launch_path,
            canonical,
        } => {
            if Binding::load(sql, *target).await?.is_none() {
                sqlx::query("INSERT INTO main.node_target_launch_selection(target_id,profile_id,profile_revision,workspace_id,launch_path,observed_canonical_path,selected_by_request_id,selected_at_ms) VALUES(?,?,?,?,?,?,?,?)").bind(target.encode()).bind(i.profile.id().encode()).bind(i.profile.revision().to_be_bytes().to_vec()).bind(workspace.encode()).bind(launch_path).bind(canonical).bind(row.request.encode()).bind(at).execute(&mut *sql).await.map_err(storage)?;
            }
            return Ok(());
        }
    }
    sqlx::query("INSERT INTO main.node_launch_profiles(profile_id,revision,harness_id,source_id,payload_format,protected_payload,payload_digest,created_by_request_id,created_at_ms) VALUES(?,?,?,?,1,?,?,?,?)").bind(i.profile.id().encode()).bind(i.profile.revision().to_be_bytes().to_vec()).bind(i.harness.encode()).bind(i.source.encode()).bind(row.cipher.as_deref()).bind(&row.digest).bind(row.request.encode()).bind(at).execute(sql).await.map_err(storage)?;
    Ok(())
}

async fn validate_publication(
    sql: &mut SqliteConnection,
    i: &Intent,
    expected: Option<&ProfileKey>,
) -> Result<(), CatalogError> {
    let count=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM main.product_harness_sources hs JOIN main.harnesses h ON h.harness_id=hs.harness_id JOIN main.product_sources s ON s.source_id=hs.source_id WHERE hs.harness_id=? AND hs.source_id=?").bind(i.harness.encode()).bind(i.source.encode()).fetch_one(&mut *sql).await.map_err(storage)?;
    if count != 1 {
        return Err(CatalogError::InvalidSource);
    }
    let row=sqlx::query("SELECT revision,harness_id,source_id FROM main.node_launch_profiles WHERE profile_id=? ORDER BY revision DESC LIMIT 1").bind(i.profile.id().encode()).fetch_optional(&mut *sql).await.map_err(storage)?;
    match (expected, row) {
        (None, None) => {}
        (Some(expected), Some(r)) => {
            let k = key(
                Some(expected.id().encode()),
                Some(r.try_get("revision").map_err(corrupt)?),
            )?
            .ok_or(CatalogError::CorruptState)?;
            if k != *expected
                || decode_id(r.try_get::<String, _>("harness_id").map_err(corrupt)?)? != i.harness
                || decode_id(r.try_get::<String, _>("source_id").map_err(corrupt)?)? != i.source
            {
                return Err(CatalogError::Conflict);
            }
            let p = Profile::load(sql, *expected).await?;
            if p.harness != i.harness || p.source != i.source {
                return Err(CatalogError::Conflict);
            }
        }
        _ => return Err(CatalogError::Conflict),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
