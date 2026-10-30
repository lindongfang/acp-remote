//! Durable permission bookkeeping. Runtime coordinates are internal data, not a verified issuer.
use crate::{
    application::approvals::ApprovalRepository,
    domain::{
        ActorContext, EntityId,
        approvals::{
            ApprovalBinding, ApprovalChoice, ApprovalDeliveryClaim, ApprovalError, ApprovalId,
            ApprovalObservation, ApprovalRestriction, ApprovalState, ApprovalView, ConnectionEpoch,
            DecisionInput, DecisionProbe, DecisionRecord, DeliveryState, InstanceId,
            InvalidationReason, MAX_APPROVAL_BYTES, NativePermissionOption, NativeRequestId,
            NativeSessionId, PermissionKind, PermissionOptionKind, RunLifecycle, TransportRecord,
            TransportReport,
        },
        history::{
            ItemId, MAX_VERSION, OutputScope, RawContent, RunId, SessionId, SourceId, SourceOrigin,
            ToolReference, TurnId,
        },
    },
};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool, sqlite::SqliteRow};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone)]
pub struct ApprovalStore {
    pool: SqlitePool,
}
impl ApprovalStore {
    pub(crate) const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    /// Records the runtime owner's epoch only; it does not authenticate a protocol peer.
    /// # Errors
    /// Rejects stale or reused closed generations and failed transactions.
    pub async fn bind_connection(&self, epoch: ConnectionEpoch) -> Result<(), ApprovalError> {
        epoch.validate()?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        let old =
            sqlx::query("SELECT generation,state FROM approval_connections WHERE instance_id=?")
                .bind(epoch.instance.encode())
                .fetch_optional(&mut *transaction)
                .await
                .map_err(storage)?;
        if let Some(row) = old {
            let generation = as_u64(row.try_get("generation").map_err(storage)?)?;
            let state: String = row.try_get("state").map_err(storage)?;
            if epoch.generation < generation || epoch.generation == generation && state != "active"
            {
                return Err(ApprovalError::InvalidSource);
            }
            if epoch.generation == generation {
                return Ok(());
            }
            let prior = ConnectionEpoch {
                instance: epoch.instance,
                generation,
            };
            invalidate(
                &mut transaction,
                Target::Connection(prior),
                InvalidationReason::ConnectionReplaced,
            )
            .await?;
            mark_connection_runs_unknown(&mut transaction, prior).await?;
            sqlx::query(
                "UPDATE approval_connections SET generation=?,state='active' WHERE instance_id=?",
            )
            .bind(as_i64(epoch.generation)?)
            .bind(epoch.instance.encode())
            .execute(&mut *transaction)
            .await
            .map_err(storage)?;
        } else {
            sqlx::query("INSERT INTO approval_connections VALUES(?,?,'active')")
                .bind(epoch.instance.encode())
                .bind(as_i64(epoch.generation)?)
                .execute(&mut *transaction)
                .await
                .map_err(storage)?;
        }
        transaction.commit().await.map_err(storage)
    }
    /// Permanently binds one original execution to its source epoch; invalid bindings never revive.
    /// # Errors
    /// Rejects noncurrent sources, mismatched scopes and terminal or cancelling Runs.
    pub async fn bind_run(
        &self,
        scope: OutputScope,
        epoch: ConnectionEpoch,
        native_session: &NativeSessionId,
    ) -> Result<(), ApprovalError> {
        epoch.validate()?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        require_connection(&mut transaction, epoch).await?;
        require_scope(&mut transaction, scope).await?;
        let native: Option<String> =
            sqlx::query_scalar("SELECT native_session_id FROM sessions WHERE session_id=?")
                .bind(scope.session.encode())
                .fetch_one(&mut *transaction)
                .await
                .map_err(storage)?;
        if native.as_deref() != Some(native_session.as_str()) {
            return Err(ApprovalError::InvalidSource);
        }
        let active:i64=sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE run_id=? AND status IN ('accepted','dispatching','running')").bind(scope.run.encode()).fetch_one(&mut *transaction).await.map_err(storage)?;
        if active != 1 {
            return Err(ApprovalError::InvalidSource);
        }
        let row=sqlx::query("SELECT session_id,turn_id,instance_id,generation,native_session_id,state FROM approval_run_sources WHERE run_id=?").bind(scope.run.encode()).fetch_optional(&mut *transaction).await.map_err(storage)?;
        if let Some(row) = row {
            if row.try_get::<String, _>("session_id").map_err(storage)? != scope.session.encode()
                || row.try_get::<String, _>("turn_id").map_err(storage)? != scope.turn.encode()
                || row.try_get::<String, _>("instance_id").map_err(storage)?
                    != epoch.instance.encode()
                || as_u64(row.try_get("generation").map_err(storage)?)? != epoch.generation
                || row
                    .try_get::<String, _>("native_session_id")
                    .map_err(storage)?
                    != native_session.as_str()
                || row.try_get::<String, _>("state").map_err(storage)? != "active"
            {
                return Err(ApprovalError::InvalidSource);
            }
        } else {
            sqlx::query("INSERT INTO approval_run_sources VALUES(?,?,?,?,?,?,'active',NULL)")
                .bind(scope.run.encode())
                .bind(scope.session.encode())
                .bind(scope.turn.encode())
                .bind(epoch.instance.encode())
                .bind(as_i64(epoch.generation)?)
                .bind(native_session.as_str())
                .execute(&mut *transaction)
                .await
                .map_err(storage)?;
        }
        transaction.commit().await.map_err(storage)
    }
    /// Preserves original structure and advertises only representable, resolved requests.
    /// # Errors
    /// Returns source conflicts, bounded-content or transaction errors without default approval.
    #[allow(clippy::too_many_lines)]
    pub async fn observe(
        &self,
        observation: &ApprovalObservation,
    ) -> Result<ApprovalView, ApprovalError> {
        validate_observation(observation)?;
        let digest = observation_digest(observation)?;
        let binding = &observation.binding;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        require_binding(&mut transaction, binding).await?;
        if let Some(row)=sqlx::query("SELECT approval_id,source_digest FROM approvals WHERE instance_id=? AND connection_generation=? AND native_request_id=?").bind(binding.connection.instance.encode()).bind(as_i64(binding.connection.generation)?).bind(binding.native_request.as_json()).fetch_optional(&mut *transaction).await.map_err(storage)? {
            if row.try_get::<String,_>("source_digest").map_err(storage)?!=digest{return Err(ApprovalError::SourceConflict);}let id=ApprovalId(parse_id(&row.try_get::<String,_>("approval_id").map_err(storage)?)?);let result=load(&mut transaction,id).await?;transaction.commit().await.map_err(storage)?;return Ok(result);
        }
        let tool = if let Some(native) = &observation.tool_call_id {
            super::history::resolve_tool_ref(
                &mut transaction,
                binding.scope,
                &SourceOrigin {
                    connection: binding.connection,
                    native_session: binding.native_session.clone(),
                },
                native,
            )
            .await
            .map_err(|_| ApprovalError::HistoryUnavailable)?
        } else {
            None
        };
        let restriction = restriction(observation, tool);
        let state = if restriction.is_some() {
            "unsupported"
        } else {
            "pending"
        };
        let id = ApprovalId(new_id()?);
        sqlx::query("INSERT INTO approvals(approval_id,session_id,run_id,turn_id,instance_id,connection_generation,native_session_id,native_request_id,source_digest,request_kind,protocol,protocol_version,raw_request,scope_raw,tool_native_id,tool_item_id,tool_source_id,status,restriction) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(id.encode()).bind(binding.scope.session.encode()).bind(binding.scope.run.encode()).bind(binding.scope.turn.encode()).bind(binding.connection.instance.encode()).bind(as_i64(binding.connection.generation)?).bind(binding.native_session.as_str()).bind(binding.native_request.as_json()).bind(digest).bind(observation.kind.label()).bind(observation.protocol.as_ref()).bind(observation.protocol_version.as_ref()).bind(observation.raw_request.0.as_ref()).bind(observation.scope_raw.0.as_ref()).bind(observation.tool_call_id.as_ref().map(crate::domain::history::NativeToolCallId::as_str)).bind(tool.map(|tool|tool.item.encode())).bind(tool.map(|tool|tool.source.encode())).bind(state).bind(restriction.map(ApprovalRestriction::code)).execute(&mut *transaction).await.map_err(storage)?;
        for (ordinal, option) in observation.options.iter().enumerate() {
            sqlx::query("INSERT INTO approval_options VALUES(?,?,?,?,?,?)")
                .bind(new_id()?.encode())
                .bind(id.encode())
                .bind(i64::try_from(ordinal).map_err(|_| ApprovalError::ContentTooLarge)?)
                .bind(option.native_id.as_ref())
                .bind(option.name.as_ref())
                .bind(option.kind.label())
                .execute(&mut *transaction)
                .await
                .map_err(storage)?;
        }
        sqlx::query("INSERT INTO approval_deliveries(approval_id,state) VALUES(?,'not_sent')")
            .bind(id.encode())
            .execute(&mut *transaction)
            .await
            .map_err(storage)?;
        audit(
            &mut transaction,
            AuditFact {
                approval: id,
                command: None,
                actor: None,
                actor_generation: None,
                binding,
                action: "approval.request",
                target: id.0,
                outcome: state,
                basis: "runtime_observation",
            },
        )
        .await?;
        let result = load(&mut transaction, id).await?;
        transaction.commit().await.map_err(storage)?;
        Ok(result)
    }
    /// Reads metadata for a current trusted actor; this repository is not a network authorization entry.
    /// # Errors
    /// Rejects stale actors and missing or malformed requests.
    pub async fn view(
        &self,
        actor: ActorContext,
        id: ApprovalId,
    ) -> Result<ApprovalView, ApprovalError> {
        let mut transaction = self.pool.begin().await.map_err(storage)?;
        require_actor(&mut transaction, actor).await?;
        let result = load(&mut transaction, id).await?;
        transaction.commit().await.map_err(storage)?;
        Ok(result)
    }
    /// Checks the shared actor/request namespace before history work or new decision.
    /// # Errors
    /// Rejects conflicts, invalid actors/requests and unsupported choices.
    pub async fn probe(
        &self,
        actor: ActorContext,
        input: DecisionInput,
    ) -> Result<DecisionProbe, ApprovalError> {
        let mut transaction = self.pool.begin().await.map_err(storage)?;
        require_actor(&mut transaction, actor).await?;
        let result = probe(&mut transaction, actor, input).await?;
        transaction.commit().await.map_err(storage)?;
        Ok(result)
    }

    /// Accepts one decision, its global Command and metadata-only audit atomically.
    /// # Errors
    /// Rejects conflicts, stale sources, unsupported choices and failed compare-and-set/commit.
    pub async fn decide(
        &self,
        actor: ActorContext,
        input: DecisionInput,
    ) -> Result<DecisionRecord, ApprovalError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        require_actor(&mut transaction, actor).await?;
        let view = match probe(&mut transaction, actor, input).await? {
            DecisionProbe::Replay(record) => {
                transaction.commit().await.map_err(storage)?;
                return Ok(record);
            }
            DecisionProbe::Fresh(view) => view,
        };
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_commands")
            .fetch_one(&mut *transaction)
            .await
            .map_err(storage)?;
        if count >= 100_000 {
            return Err(ApprovalError::CapacityExhausted);
        }
        let command = new_id()?;
        sqlx::query("INSERT INTO session_commands VALUES(?,?,?,'approval.decide',?,?,'accepted')")
            .bind(command.encode())
            .bind(actor.client_id.encode())
            .bind(input.request_id.encode())
            .bind(decision_digest(input))
            .bind(input.session.encode())
            .execute(&mut *transaction)
            .await
            .map_err(storage)?;
        let updated=sqlx::query("UPDATE approvals SET status='decided',decision_command_id=?,selected_choice_id=?,deciding_actor_id=?,deciding_generation=? WHERE approval_id=? AND status='pending'").bind(command.encode()).bind(input.choice.encode()).bind(actor.client_id.encode()).bind(actor.auth_generation).bind(input.approval.encode()).execute(&mut *transaction).await.map_err(storage)?;
        if updated.rows_affected() != 1 {
            return Err(ApprovalError::AlreadyDecided);
        }
        audit(
            &mut transaction,
            AuditFact {
                approval: input.approval,
                command: Some(command),
                actor: Some(actor.client_id),
                actor_generation: Some(actor.auth_generation),
                binding: &view.binding,
                action: "approval.decide",
                target: command,
                outcome: "accepted",
                basis: "repository_pending_cas",
            },
        )
        .await?;
        transaction.commit().await.map_err(storage)?;
        Ok(DecisionRecord {
            command,
            approval: input.approval,
            actor: actor.client_id,
            choice: input.choice,
            binding: view.binding,
            delivery: DeliveryState::NotSent,
            replayed: false,
        })
    }
    /// Claims a single delivery attempt; there is no transition back to `not_sent`.
    /// # Errors
    /// Rejects stale source/actor state, missing decisions and previously spent claims.
    pub async fn claim_delivery(
        &self,
        actor: ActorContext,
        id: ApprovalId,
    ) -> Result<ApprovalDeliveryClaim, ApprovalError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        require_actor(&mut transaction, actor).await?;
        let view = load(&mut transaction, id).await?;
        require_binding(&mut transaction, &view.binding).await?;
        if view.state != ApprovalState::Decided {
            return Err(ApprovalError::InvalidApproval);
        }
        let row = sqlx::query(
            "SELECT deciding_actor_id,deciding_generation FROM approvals WHERE approval_id=?",
        )
        .bind(id.encode())
        .fetch_one(&mut *transaction)
        .await
        .map_err(storage)?;
        if row
            .try_get::<String, _>("deciding_actor_id")
            .map_err(storage)?
            != actor.client_id.encode()
            || row
                .try_get::<i64, _>("deciding_generation")
                .map_err(storage)?
                != actor.auth_generation
        {
            return Err(ApprovalError::InvalidActor);
        }
        if view.delivery != DeliveryState::NotSent {
            return Err(ApprovalError::DeliveryAlreadyClaimed);
        }
        let command = view.decision_command.ok_or(ApprovalError::CorruptState)?;
        let selected = view.selected_choice.ok_or(ApprovalError::CorruptState)?;
        let option = view
            .choices
            .iter()
            .find(|choice| choice.choice_id == selected)
            .ok_or(ApprovalError::CorruptState)?;
        let token = new_id()?;
        let updated=sqlx::query("UPDATE approval_deliveries SET state='claimed',claim_id=? WHERE approval_id=? AND state='not_sent'").bind(token.encode()).bind(id.encode()).execute(&mut *transaction).await.map_err(storage)?;
        if updated.rows_affected() != 1 {
            return Err(ApprovalError::DeliveryAlreadyClaimed);
        }
        audit(
            &mut transaction,
            AuditFact {
                approval: id,
                command: Some(command),
                actor: Some(actor.client_id),
                actor_generation: Some(actor.auth_generation),
                binding: &view.binding,
                action: "approval.claim",
                target: token,
                outcome: "claimed",
                basis: "once_current_claim",
            },
        )
        .await?;
        let native_option = Arc::clone(&option.option.native_id);
        transaction.commit().await.map_err(storage)?;
        Ok(ApprovalDeliveryClaim {
            token,
            approval: id,
            command,
            actor,
            binding: view.binding,
            native_option,
        })
    }
    /// Rechecks a spent claim at the source-owned write boundary; it does not grant a retry.
    /// # Errors
    /// Storage errors fail closed; false includes cancelled and old-epoch requests.
    pub async fn claim_current(
        &self,
        claim: &ApprovalDeliveryClaim,
    ) -> Result<bool, ApprovalError> {
        let mut transaction = self.pool.begin().await.map_err(storage)?;
        let result = claim_current(&mut transaction, claim).await?;
        transaction.commit().await.map_err(storage)?;
        Ok(result)
    }
    /// Records a transport report only, never peer execution/application acknowledgement.
    /// # Errors
    /// Reporting failure preserves the decision and rejects conflicting final reports.
    pub async fn report_transport(
        &self,
        claim: &ApprovalDeliveryClaim,
        report: TransportReport,
    ) -> Result<TransportRecord, ApprovalError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        let row=sqlx::query("SELECT state,write_report,source_current_at_report FROM approval_deliveries WHERE approval_id=? AND claim_id=?").bind(claim.approval.encode()).bind(claim.token.encode()).fetch_optional(&mut *transaction).await.map_err(storage)?.ok_or(ApprovalError::InvalidApproval)?;
        if let Some(prior) = row
            .try_get::<Option<String>, _>("write_report")
            .map_err(storage)?
        {
            if prior != report.code() {
                return Err(ApprovalError::ReportConflict);
            }
            let state = delivery_state(&row.try_get::<String, _>("state").map_err(storage)?)?;
            let current: Option<i64> = row.try_get("source_current_at_report").map_err(storage)?;
            transaction.commit().await.map_err(storage)?;
            return Ok(TransportRecord {
                state,
                reported: report,
                source_current_at_report: current == Some(1),
            });
        }
        let current = claim_current(&mut transaction, claim).await?;
        let state = if current && report == TransportReport::WriteCompleted {
            DeliveryState::TransportWritten
        } else {
            DeliveryState::Unknown
        };
        sqlx::query("UPDATE approval_deliveries SET state=?,write_report=?,source_current_at_report=? WHERE approval_id=? AND claim_id=? AND write_report IS NULL").bind(delivery_label(state)).bind(report.code()).bind(i64::from(current)).bind(claim.approval.encode()).bind(claim.token.encode()).execute(&mut *transaction).await.map_err(storage)?;
        audit(
            &mut transaction,
            AuditFact {
                approval: claim.approval,
                command: Some(claim.command),
                actor: Some(claim.actor.client_id),
                actor_generation: Some(claim.actor.auth_generation),
                binding: &claim.binding,
                action: "approval.transport",
                target: claim.token,
                outcome: if state == DeliveryState::TransportWritten {
                    "transport_written"
                } else {
                    "unknown"
                },
                basis: "transport_report_no_peer_ack",
            },
        )
        .await?;
        transaction.commit().await.map_err(storage)?;
        Ok(TransportRecord {
            state,
            reported: report,
            source_current_at_report: current,
        })
    }
    /// Invalidates a closed source, retaining unresolved execution/claim facts without replay.
    /// # Errors
    /// Failed transactions roll back. The runtime must keep its physical source fence closed.
    pub async fn close_connection(&self, epoch: ConnectionEpoch) -> Result<(), ApprovalError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        invalidate(
            &mut transaction,
            Target::Connection(epoch),
            InvalidationReason::ConnectionClosed,
        )
        .await?;
        mark_connection_runs_unknown(&mut transaction, epoch).await?;
        sqlx::query(
            "UPDATE approval_connections SET state='closed' WHERE instance_id=? AND generation=?",
        )
        .bind(epoch.instance.encode())
        .bind(as_i64(epoch.generation)?)
        .execute(&mut *transaction)
        .await
        .map_err(storage)?;
        transaction.commit().await.map_err(storage)
    }
    /// Applies reliable runtime facts; `CancelRequested` keeps foreground mutual exclusion.
    /// # Errors
    /// Rejects stale epochs and rolls back lifecycle and audit changes together.
    pub async fn invalidate_run(
        &self,
        scope: OutputScope,
        epoch: ConnectionEpoch,
        event: RunLifecycle,
    ) -> Result<(), ApprovalError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        require_connection(&mut transaction, epoch).await?;
        require_scope(&mut transaction, scope).await?;
        let bound:i64=sqlx::query_scalar("SELECT COUNT(*) FROM approval_run_sources WHERE run_id=? AND session_id=? AND turn_id=? AND instance_id=? AND generation=?").bind(scope.run.encode()).bind(scope.session.encode()).bind(scope.turn.encode()).bind(epoch.instance.encode()).bind(as_i64(epoch.generation)?).fetch_one(&mut *transaction).await.map_err(storage)?;
        if bound != 1 {
            return Err(ApprovalError::InvalidSource);
        }
        let (run, command, reason) = match event {
            RunLifecycle::CancelRequested => ("cancelling", None, InvalidationReason::Cancelled),
            RunLifecycle::Completed => (
                "completed",
                Some("completed"),
                InvalidationReason::RunCompleted,
            ),
            RunLifecycle::Cancelled => (
                "cancelled",
                Some("completed"),
                InvalidationReason::Cancelled,
            ),
            RunLifecycle::Failed => ("failed", Some("failed"), InvalidationReason::RunFailed),
        };
        let changed=sqlx::query("UPDATE runs SET status=? WHERE run_id=? AND status IN ('accepted','dispatching','running','cancelling')").bind(run).bind(scope.run.encode()).execute(&mut *transaction).await.map_err(storage)?;
        if changed.rows_affected() != 1 {
            return Err(ApprovalError::InvalidSource);
        }
        if let Some(command) = command {
            sqlx::query("UPDATE session_commands SET status=? WHERE action='session.input' AND command_id IN(SELECT command_id FROM runs WHERE run_id=?)").bind(command).bind(scope.run.encode()).execute(&mut *transaction).await.map_err(storage)?;
        }
        invalidate(&mut transaction, Target::Run(scope.run), reason).await?;
        sqlx::query("UPDATE approval_run_sources SET state='invalid',invalid_reason=COALESCE(invalid_reason,?) WHERE run_id=?").bind(reason.code()).bind(scope.run.encode()).execute(&mut *transaction).await.map_err(storage)?;
        transaction.commit().await.map_err(storage)
    }
}

impl ApprovalRepository for ApprovalStore {
    async fn probe(
        &self,
        actor: ActorContext,
        input: DecisionInput,
    ) -> Result<DecisionProbe, ApprovalError> {
        self.probe(actor, input).await
    }
    async fn observe(
        &self,
        observation: &ApprovalObservation,
    ) -> Result<ApprovalView, ApprovalError> {
        self.observe(observation).await
    }
    async fn decide(
        &self,
        actor: ActorContext,
        input: DecisionInput,
    ) -> Result<DecisionRecord, ApprovalError> {
        self.decide(actor, input).await
    }
    async fn view(
        &self,
        actor: ActorContext,
        id: ApprovalId,
    ) -> Result<ApprovalView, ApprovalError> {
        self.view(actor, id).await
    }
    async fn claim(
        &self,
        actor: ActorContext,
        id: ApprovalId,
    ) -> Result<ApprovalDeliveryClaim, ApprovalError> {
        self.claim_delivery(actor, id).await
    }
    async fn current(&self, claim: &ApprovalDeliveryClaim) -> Result<bool, ApprovalError> {
        self.claim_current(claim).await
    }
    async fn report(
        &self,
        claim: &ApprovalDeliveryClaim,
        report: TransportReport,
    ) -> Result<TransportRecord, ApprovalError> {
        self.report_transport(claim, report).await
    }
    async fn close_connection(&self, epoch: ConnectionEpoch) -> Result<(), ApprovalError> {
        self.close_connection(epoch).await
    }
    async fn invalidate_run(
        &self,
        scope: OutputScope,
        epoch: ConnectionEpoch,
        event: RunLifecycle,
    ) -> Result<(), ApprovalError> {
        self.invalidate_run(scope, epoch, event).await
    }
}
#[derive(Clone, Copy)]
enum Target {
    Run(RunId),
    Connection(ConnectionEpoch),
    All,
}
struct AuditFact<'a> {
    approval: ApprovalId,
    command: Option<EntityId>,
    actor: Option<EntityId>,
    actor_generation: Option<i64>,
    binding: &'a ApprovalBinding,
    action: &'static str,
    target: EntityId,
    outcome: &'static str,
    basis: &'static str,
}
async fn audit(
    connection: &mut SqliteConnection,
    fact: AuditFact<'_>,
) -> Result<(), ApprovalError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_secs()).ok())
        .ok_or(ApprovalError::CorruptState)?;
    let row=sqlx::query("INSERT INTO security_audit(actor_id,action,target_id,outcome,occurred_at) VALUES(?,?,?,?,?)").bind(fact.actor.map(EntityId::encode)).bind(fact.action).bind(fact.target.encode()).bind(fact.outcome).bind(now).execute(&mut *connection).await.map_err(storage)?;
    sqlx::query("INSERT INTO approval_audit_links VALUES(?,?,?,?,?,?,?)")
        .bind(row.last_insert_rowid())
        .bind(fact.approval.encode())
        .bind(fact.command.map(EntityId::encode))
        .bind(fact.basis)
        .bind(fact.actor_generation)
        .bind(fact.binding.connection.instance.encode())
        .bind(as_i64(fact.binding.connection.generation)?)
        .execute(connection)
        .await
        .map_err(storage)?;
    Ok(())
}
fn storage(_: sqlx::Error) -> ApprovalError {
    ApprovalError::StorageUnavailable
}
fn parse_id(value: &str) -> Result<EntityId, ApprovalError> {
    EntityId::decode(value).map_err(|_| ApprovalError::CorruptState)
}
fn new_id() -> Result<EntityId, ApprovalError> {
    crate::random::bytes()
        .map(EntityId::from_bytes)
        .map_err(|_| ApprovalError::RandomUnavailable)
}
fn as_u64(value: i64) -> Result<u64, ApprovalError> {
    u64::try_from(value).map_err(|_| ApprovalError::CorruptState)
}
fn as_i64(value: u64) -> Result<i64, ApprovalError> {
    if value > MAX_VERSION {
        return Err(ApprovalError::InvalidSource);
    }
    i64::try_from(value).map_err(|_| ApprovalError::InvalidSource)
}
async fn require_actor(
    connection: &mut SqliteConnection,
    actor: ActorContext,
) -> Result<(), ApprovalError> {
    super::require_actor(connection, actor)
        .await
        .map_err(|error| match error {
            crate::domain::SessionError::InvalidActor => ApprovalError::InvalidActor,
            _ => ApprovalError::StorageUnavailable,
        })
}
async fn require_scope(
    connection: &mut SqliteConnection,
    scope: OutputScope,
) -> Result<(), ApprovalError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM runs WHERE run_id=? AND session_id=? AND turn_id=?",
    )
    .bind(scope.run.encode())
    .bind(scope.session.encode())
    .bind(scope.turn.encode())
    .fetch_one(connection)
    .await
    .map_err(storage)?;
    if count == 1 {
        Ok(())
    } else {
        Err(ApprovalError::InvalidSource)
    }
}
async fn require_connection(
    connection: &mut SqliteConnection,
    epoch: ConnectionEpoch,
) -> Result<(), ApprovalError> {
    epoch.validate()?;
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM approval_connections WHERE instance_id=? AND generation=? AND state='active'").bind(epoch.instance.encode()).bind(as_i64(epoch.generation)?).fetch_one(connection).await.map_err(storage)?;
    if count == 1 {
        Ok(())
    } else {
        Err(ApprovalError::InvalidSource)
    }
}
async fn require_binding(
    connection: &mut SqliteConnection,
    binding: &ApprovalBinding,
) -> Result<(), ApprovalError> {
    require_connection(connection, binding.connection).await?;
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM approval_run_sources s JOIN runs r ON r.run_id=s.run_id WHERE s.run_id=? AND s.session_id=? AND s.turn_id=? AND s.instance_id=? AND s.generation=? AND s.native_session_id=? AND s.state='active' AND r.status IN ('accepted','dispatching','running')").bind(binding.scope.run.encode()).bind(binding.scope.session.encode()).bind(binding.scope.turn.encode()).bind(binding.connection.instance.encode()).bind(as_i64(binding.connection.generation)?).bind(binding.native_session.as_str()).fetch_one(connection).await.map_err(storage)?;
    if count == 1 {
        Ok(())
    } else {
        Err(ApprovalError::InvalidSource)
    }
}
async fn probe(
    connection: &mut SqliteConnection,
    actor: ActorContext,
    input: DecisionInput,
) -> Result<DecisionProbe, ApprovalError> {
    if let Some(row)=sqlx::query("SELECT command_id,action,payload_digest FROM session_commands WHERE actor_id=? AND request_id=?").bind(actor.client_id.encode()).bind(input.request_id.encode()).fetch_optional(&mut *connection).await.map_err(storage)? {
        if row.try_get::<String,_>("action").map_err(storage)?!="approval.decide"||row.try_get::<String,_>("payload_digest").map_err(storage)?!=decision_digest(input){return Err(ApprovalError::RequestConflict);}let command=parse_id(&row.try_get::<String,_>("command_id").map_err(storage)?)?;let view=load(connection,input.approval).await?;if view.decision_command!=Some(command)||view.selected_choice!=Some(input.choice){return Err(ApprovalError::CorruptState);}return Ok(DecisionProbe::Replay(DecisionRecord{command,approval:input.approval,actor:actor.client_id,choice:input.choice,binding:view.binding,delivery:view.delivery,replayed:true}));
    }
    let view = load(connection, input.approval).await?;
    if view.binding.scope.session != input.session {
        return Err(ApprovalError::InvalidApproval);
    }
    match view.state {
        ApprovalState::Decided => return Err(ApprovalError::AlreadyDecided),
        ApprovalState::Invalid => return Err(ApprovalError::InvalidApproval),
        ApprovalState::Unsupported => return Err(ApprovalError::UnsupportedRequest),
        ApprovalState::Pending => {}
    }
    require_binding(connection, &view.binding).await?;
    if !view
        .choices
        .iter()
        .any(|choice| choice.choice_id == input.choice && choice.option.kind.supported())
    {
        return Err(ApprovalError::UnsupportedOption);
    }
    Ok(DecisionProbe::Fresh(view))
}
fn decision_digest(input: DecisionInput) -> String {
    let value = format!(
        r#"{{"action":"approval.decide","payload":{{"approvalId":"{}","decisionId":"{}","sessionId":"{}"}}}}"#,
        input.approval.encode(),
        input.choice.encode(),
        input.session.encode()
    );
    hex::encode(Sha256::digest(value.as_bytes()))
}
fn validate_observation(value: &ApprovalObservation) -> Result<(), ApprovalError> {
    value.binding.connection.validate()?;
    if value.raw_request.0.len() > MAX_APPROVAL_BYTES
        || value.scope_raw.0.len() > MAX_APPROVAL_BYTES
        || value.protocol.len() > 64
        || value.protocol_version.len() > 64
        || value.protocol.is_empty()
        || value.protocol_version.is_empty()
    {
        return Err(ApprovalError::ContentTooLarge);
    }
    let bytes = value.options.iter().fold(0usize, |sum, option| {
        sum.saturating_add(option.native_id.len())
            .saturating_add(option.name.len())
            .saturating_add(option.kind.label().len())
            .saturating_add(64)
    });
    if bytes > MAX_APPROVAL_BYTES {
        return Err(ApprovalError::ContentTooLarge);
    }
    Ok(())
}
fn restriction(
    value: &ApprovalObservation,
    tool: Option<ToolReference>,
) -> Option<ApprovalRestriction> {
    if value.kind != PermissionKind::ToolPermission {
        return Some(ApprovalRestriction::UnknownRequest);
    }
    if !value.scope_understood {
        return Some(ApprovalRestriction::UnknownScope);
    }
    if value.options.iter().any(|option| !option.kind.supported()) {
        return Some(ApprovalRestriction::UnknownOption);
    }
    let mut seen = HashSet::new();
    if value.options.is_empty()
        || value
            .options
            .iter()
            .any(|option| option.native_id.is_empty() || !seen.insert(option.native_id.as_ref()))
    {
        return Some(ApprovalRestriction::AmbiguousOptions);
    }
    if tool.is_none() {
        return Some(ApprovalRestriction::UnresolvedTool);
    }
    None
}
fn observation_digest(value: &ApprovalObservation) -> Result<String, ApprovalError> {
    let options:Vec<_>=value.options.iter().map(|option|serde_json::json!({"id":option.native_id.as_ref(),"name":option.name.as_ref(),"kind":option.kind.label()})).collect();
    let metadata = serde_json::json!({"session":value.binding.scope.session.encode(),"run":value.binding.scope.run.encode(),"turn":value.binding.scope.turn.encode(),"instance":value.binding.connection.instance.encode(),"generation":value.binding.connection.generation,"nativeSession":value.binding.native_session.as_str(),"nativeRequest":value.binding.native_request.as_json(),"kind":value.kind.label(),"tool":value.tool_call_id.as_ref().map(crate::domain::history::NativeToolCallId::as_str),"options":options,"scopeUnderstood":value.scope_understood,"scopeSha256":hex::encode(Sha256::digest(&value.scope_raw.0)),"rawSha256":hex::encode(Sha256::digest(&value.raw_request.0)),"protocol":value.protocol.as_ref(),"protocolVersion":value.protocol_version.as_ref()});
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(&metadata).map_err(|_| ApprovalError::CorruptState)?,
    )))
}
const fn delivery_label(value: DeliveryState) -> &'static str {
    match value {
        DeliveryState::NotSent => "not_sent",
        DeliveryState::Claimed => "claimed",
        DeliveryState::TransportWritten => "transport_written",
        DeliveryState::Unknown => "unknown",
        DeliveryState::Suppressed => "suppressed",
    }
}
fn delivery_state(value: &str) -> Result<DeliveryState, ApprovalError> {
    match value {
        "not_sent" => Ok(DeliveryState::NotSent),
        "claimed" => Ok(DeliveryState::Claimed),
        "transport_written" => Ok(DeliveryState::TransportWritten),
        "unknown" => Ok(DeliveryState::Unknown),
        "suppressed" => Ok(DeliveryState::Suppressed),
        _ => Err(ApprovalError::CorruptState),
    }
}

async fn load(
    connection: &mut SqliteConnection,
    id: ApprovalId,
) -> Result<ApprovalView, ApprovalError> {
    let row=sqlx::query("SELECT a.*,d.state AS delivery,d.claim_id FROM approvals a JOIN approval_deliveries d ON d.approval_id=a.approval_id WHERE a.approval_id=?").bind(id.encode()).fetch_optional(&mut *connection).await.map_err(storage)?.ok_or(ApprovalError::InvalidApproval)?;
    let binding = binding(&row)?;
    let state = match row
        .try_get::<String, _>("status")
        .map_err(storage)?
        .as_str()
    {
        "pending" => ApprovalState::Pending,
        "decided" => ApprovalState::Decided,
        "unsupported" => ApprovalState::Unsupported,
        "invalid" => ApprovalState::Invalid,
        _ => return Err(ApprovalError::CorruptState),
    };
    let restriction = row
        .try_get::<Option<String>, _>("restriction")
        .map_err(storage)?
        .map(|value| ApprovalRestriction::from_code(&value).ok_or(ApprovalError::CorruptState))
        .transpose()?;
    let tool = match (
        row.try_get::<Option<String>, _>("tool_item_id")
            .map_err(storage)?,
        row.try_get::<Option<String>, _>("tool_source_id")
            .map_err(storage)?,
    ) {
        (Some(item), Some(source)) => Some(ToolReference {
            session: binding.scope.session,
            item: ItemId(parse_id(&item)?),
            source: SourceId(parse_id(&source)?),
        }),
        (None, None) => None,
        _ => return Err(ApprovalError::CorruptState),
    };
    let rows=sqlx::query("SELECT choice_id,native_id,name,kind FROM approval_options WHERE approval_id=? ORDER BY ordinal").bind(id.encode()).fetch_all(&mut *connection).await.map_err(storage)?;
    let choices = rows
        .iter()
        .map(|row| {
            Ok(ApprovalChoice {
                choice_id: parse_id(&row.try_get::<String, _>("choice_id").map_err(storage)?)?,
                option: NativePermissionOption {
                    native_id: Arc::from(row.try_get::<String, _>("native_id").map_err(storage)?),
                    name: Arc::from(row.try_get::<String, _>("name").map_err(storage)?),
                    kind: PermissionOptionKind::from_label(
                        &row.try_get::<String, _>("kind").map_err(storage)?,
                    ),
                },
            })
        })
        .collect::<Result<Vec<_>, ApprovalError>>()?;
    Ok(ApprovalView {
        id,
        binding,
        state,
        restriction,
        tool,
        choices,
        scope_raw: RawContent(Arc::from(
            row.try_get::<Vec<u8>, _>("scope_raw").map_err(storage)?,
        )),
        raw_request: RawContent(Arc::from(
            row.try_get::<Vec<u8>, _>("raw_request").map_err(storage)?,
        )),
        protocol: Arc::from(row.try_get::<String, _>("protocol").map_err(storage)?),
        protocol_version: Arc::from(
            row.try_get::<String, _>("protocol_version")
                .map_err(storage)?,
        ),
        delivery_claim: row
            .try_get::<Option<String>, _>("claim_id")
            .map_err(storage)?
            .map(|value| parse_id(&value))
            .transpose()?,
        decision_command: row
            .try_get::<Option<String>, _>("decision_command_id")
            .map_err(storage)?
            .map(|value| parse_id(&value))
            .transpose()?,
        selected_choice: row
            .try_get::<Option<String>, _>("selected_choice_id")
            .map_err(storage)?
            .map(|value| parse_id(&value))
            .transpose()?,
        delivery: delivery_state(&row.try_get::<String, _>("delivery").map_err(storage)?)?,
    })
}
fn binding(row: &SqliteRow) -> Result<ApprovalBinding, ApprovalError> {
    Ok(ApprovalBinding {
        scope: OutputScope {
            session: SessionId(parse_id(
                &row.try_get::<String, _>("session_id").map_err(storage)?,
            )?),
            run: RunId(parse_id(
                &row.try_get::<String, _>("run_id").map_err(storage)?,
            )?),
            turn: TurnId(parse_id(
                &row.try_get::<String, _>("turn_id").map_err(storage)?,
            )?),
        },
        connection: ConnectionEpoch {
            instance: InstanceId(parse_id(
                &row.try_get::<String, _>("instance_id").map_err(storage)?,
            )?),
            generation: as_u64(row.try_get("connection_generation").map_err(storage)?)?,
        },
        native_session: NativeSessionId::new(Arc::from(
            row.try_get::<String, _>("native_session_id")
                .map_err(storage)?,
        ))?,
        native_request: NativeRequestId::from_json(
            &row.try_get::<String, _>("native_request_id")
                .map_err(storage)?,
        )
        .map_err(|_| ApprovalError::CorruptState)?,
    })
}
async fn claim_current(
    connection: &mut SqliteConnection,
    claim: &ApprovalDeliveryClaim,
) -> Result<bool, ApprovalError> {
    match require_actor(connection, claim.actor).await {
        Ok(()) => {}
        Err(ApprovalError::InvalidActor) => return Ok(false),
        Err(error) => return Err(error),
    }
    match require_binding(connection, &claim.binding).await {
        Ok(()) => {}
        Err(ApprovalError::InvalidSource) => return Ok(false),
        Err(error) => return Err(error),
    }
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM approvals a JOIN approval_deliveries d ON d.approval_id=a.approval_id WHERE a.approval_id=? AND a.status='decided' AND a.decision_command_id=? AND a.deciding_actor_id=? AND a.deciding_generation=? AND d.claim_id=? AND d.state='claimed' AND d.write_report IS NULL").bind(claim.approval.encode()).bind(claim.command.encode()).bind(claim.actor.client_id.encode()).bind(claim.actor.auth_generation).bind(claim.token.encode()).fetch_one(connection).await.map_err(storage)?;
    Ok(count == 1)
}
async fn invalidate(
    connection: &mut SqliteConnection,
    target: Target,
    reason: InvalidationReason,
) -> Result<(), ApprovalError> {
    loop {
        let query = match target {
            Target::Run(_) => {
                "SELECT * FROM approvals WHERE run_id=? AND status!='invalid' LIMIT 32"
            }
            Target::Connection(_) => {
                "SELECT * FROM approvals WHERE instance_id=? AND connection_generation=? AND status!='invalid' LIMIT 32"
            }
            Target::All => "SELECT * FROM approvals WHERE status!='invalid' LIMIT 32",
        };
        let query = sqlx::query(query);
        let query = match target {
            Target::Run(run) => query.bind(run.encode()),
            Target::Connection(epoch) => query
                .bind(epoch.instance.encode())
                .bind(as_i64(epoch.generation)?),
            Target::All => query,
        };
        let rows = query.fetch_all(&mut *connection).await.map_err(storage)?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let id = ApprovalId(parse_id(
                &row.try_get::<String, _>("approval_id").map_err(storage)?,
            )?);
            let binding = binding(&row)?;
            sqlx::query("UPDATE approvals SET status='invalid',invalid_reason=COALESCE(invalid_reason,?) WHERE approval_id=? AND status!='invalid'").bind(reason.code()).bind(id.encode()).execute(&mut *connection).await.map_err(storage)?;
            sqlx::query("UPDATE approval_deliveries SET state=CASE WHEN state='claimed' THEN 'unknown' WHEN state='not_sent' THEN 'suppressed' ELSE state END WHERE approval_id=?").bind(id.encode()).execute(&mut *connection).await.map_err(storage)?;
            let command = row
                .try_get::<Option<String>, _>("decision_command_id")
                .map_err(storage)?
                .map(|value| parse_id(&value))
                .transpose()?;
            audit(
                connection,
                AuditFact {
                    approval: id,
                    command,
                    actor: None,
                    actor_generation: None,
                    binding: &binding,
                    action: "approval.invalidate",
                    target: id.0,
                    outcome: reason.code(),
                    basis: "runtime_invalidation",
                },
            )
            .await?;
        }
    }
    Ok(())
}
async fn mark_connection_runs_unknown(
    connection: &mut SqliteConnection,
    epoch: ConnectionEpoch,
) -> Result<(), ApprovalError> {
    sqlx::query("UPDATE sessions SET state='interrupted' WHERE session_id IN(SELECT s.session_id FROM approval_run_sources s JOIN runs r ON r.run_id=s.run_id WHERE s.instance_id=? AND s.generation=? AND r.status IN ('accepted','dispatching','running','cancelling','unknown'))").bind(epoch.instance.encode()).bind(as_i64(epoch.generation)?).execute(&mut *connection).await.map_err(storage)?;
    sqlx::query("UPDATE runs SET status='unknown' WHERE status IN ('accepted','dispatching','running','cancelling') AND run_id IN(SELECT run_id FROM approval_run_sources WHERE instance_id=? AND generation=?)").bind(epoch.instance.encode()).bind(as_i64(epoch.generation)?).execute(&mut *connection).await.map_err(storage)?;
    sqlx::query("UPDATE session_commands SET status='delivery_unknown' WHERE action='session.input' AND command_id IN(SELECT r.command_id FROM runs r JOIN approval_run_sources s ON s.run_id=r.run_id WHERE s.instance_id=? AND s.generation=? AND r.status='unknown')").bind(epoch.instance.encode()).bind(as_i64(epoch.generation)?).execute(&mut *connection).await.map_err(storage)?;
    sqlx::query("UPDATE approval_run_sources SET state='invalid',invalid_reason=COALESCE(invalid_reason,'connection_closed') WHERE instance_id=? AND generation=?").bind(epoch.instance.encode()).bind(as_i64(epoch.generation)?).execute(connection).await.map_err(storage)?;
    Ok(())
}
pub(super) async fn recover(connection: &mut SqliteConnection) -> Result<(), ApprovalError> {
    invalidate(connection, Target::All, InvalidationReason::Restarted).await?;
    sqlx::query("UPDATE approval_run_sources SET state='invalid',invalid_reason=COALESCE(invalid_reason,'restarted')").execute(&mut *connection).await.map_err(storage)?;
    sqlx::query("UPDATE approval_connections SET state='closed'")
        .execute(connection)
        .await
        .map_err(storage)?;
    Ok(())
}
#[cfg(test)]
mod tests;
