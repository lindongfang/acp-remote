//! SQLite command metadata. Every transaction owns a detached connection: cancellation never returns an open transaction to a pool.
use crate::{
    application::product_commands::TimeSource,
    domain::{
        ActorContext, EntityId,
        native::RequestEpoch,
        product_commands::{
            AUDIT_LIMIT, COMMAND_LIMIT, InputReceipt, InputResult, InputTarget,
            ProductCommandError as Error, ProductCommandFact, ProductCommandStatus, audit_room,
            retention,
        },
        requests::{
            MAX_REQUEST_TIME, ProductAction, ProductRequestIdentity, ProductTargetKind,
            RequestTime, VerifiedProductRequest,
        },
    },
};
use sqlx::{Row, SqliteConnection, SqlitePool, sqlite::SqliteRow};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::time::{Duration, Instant, timeout_at};

impl From<sqlx::Error> for Error {
    fn from(_: sqlx::Error) -> Self {
        Self::StorageUnavailable
    }
}

#[derive(Clone)]
pub struct ProductStore {
    pool: SqlitePool,
    faulted: Arc<AtomicBool>,
    #[cfg(test)]
    fault: Arc<std::sync::Mutex<Option<Fault>>>,
    #[cfg(test)]
    fault_arrived: Arc<tokio::sync::Notify>,
    audit_limit: u64,
}
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum Fault {
    Step(usize),
    DeferredCommit,
    CommitUnknown,
    CommitPending,
    AfterCommitPending,
    CasZero,
}

struct Transaction {
    connection: SqliteConnection,
    faulted: Arc<AtomicBool>,
    finished: bool,
    #[cfg(test)]
    fault: Option<Fault>,
    #[cfg(test)]
    fault_arrived: Arc<tokio::sync::Notify>,
    step: usize,
}
impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.finished {
            self.faulted.store(true, Ordering::SeqCst);
        }
    }
}
impl Transaction {
    fn point(&mut self) -> Result<(), Error> {
        self.step = self.step.checked_add(1).ok_or(Error::StorageUnavailable)?;
        #[cfg(test)]
        if matches!(self.fault,Some(Fault::Step(n)) if n==self.step) {
            return Err(Error::StorageUnavailable);
        }
        Ok(())
    }
    async fn commit(mut self) -> Result<(), Error> {
        self.point()?;
        #[cfg(test)]
        if matches!(self.fault, Some(Fault::CommitPending)) {
            self.fault_arrived.notify_one();
            std::future::pending::<()>().await;
        }
        #[cfg(test)]
        if matches!(self.fault, Some(Fault::DeferredCommit)) {
            sqlx::query("PRAGMA defer_foreign_keys=ON")
                .execute(&mut self.connection)
                .await?;
            sqlx::query("INSERT INTO product_audit_slots(command_id,stage,actor_id,reserved_at) VALUES('AAAAAAAAAAAAAAAAAAAAAA','result','AAAAAAAAAAAAAAAAAAAAAA',0)").execute(&mut self.connection).await?;
        }
        sqlx::query("COMMIT").execute(&mut self.connection).await?;
        #[cfg(test)]
        if matches!(self.fault, Some(Fault::AfterCommitPending)) {
            self.fault_arrived.notify_one();
            std::future::pending::<()>().await;
        }
        #[cfg(test)]
        if matches!(self.fault, Some(Fault::CommitUnknown)) {
            return Err(Error::StorageUnavailable);
        }
        self.finished = true;
        Ok(())
    }
    async fn rollback(mut self) -> Result<(), Error> {
        let result = timeout_at(
            Instant::now() + Duration::from_secs(5),
            sqlx::query("ROLLBACK").execute(&mut self.connection),
        )
        .await;
        if !matches!(result, Ok(Ok(_))) {
            return Err(Error::StorageUnavailable);
        }
        self.finished = true;
        Ok(())
    }
}
impl ProductStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            faulted: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            fault: Arc::default(),
            #[cfg(test)]
            fault_arrived: Arc::default(),
            audit_limit: AUDIT_LIMIT,
        }
    }
    #[must_use]
    pub fn faulted(&self) -> bool {
        self.faulted.load(Ordering::SeqCst)
    }
    pub(crate) fn fault(&self) {
        self.faulted.store(true, Ordering::SeqCst);
    }
    fn require_healthy(&self) -> Result<(), Error> {
        if self.faulted() {
            Err(Error::StorageUnavailable)
        } else {
            Ok(())
        }
    }
    const fn limit(&self) -> u64 {
        self.audit_limit
    }
    async fn begin(&self) -> Result<Transaction, Error> {
        self.begin_with("BEGIN IMMEDIATE").await
    }
    async fn begin_with(&self, statement: &'static str) -> Result<Transaction, Error> {
        let mut connection = self.pool.acquire().await?.detach();
        // A dropped begin future also destroys this detached connection, never pooling it.
        sqlx::query(statement).execute(&mut connection).await?;
        Ok(Transaction {
            connection,
            faulted: self.faulted.clone(),
            finished: false,
            #[cfg(test)]
            fault: self
                .fault
                .lock()
                .map_err(|_| Error::StorageUnavailable)?
                .take(),
            #[cfg(test)]
            fault_arrived: self.fault_arrived.clone(),
            step: 0,
        })
    }
    pub(crate) async fn bounded<T>(
        &self,
        deadline: Instant,
        future: impl std::future::Future<Output = Result<T, Error>>,
    ) -> Result<T, Error> {
        struct Guard<'a>(&'a ProductStore, bool);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                if !self.1 {
                    self.0.fault();
                }
            }
        }
        let mut guard = Guard(self, false);
        let result = timeout_at(
            deadline.min(Instant::now() + Duration::from_secs(5)),
            future,
        )
        .await
        .map_err(|_| Error::Cancelled)?;
        guard.1 = true;
        if matches!(
            result,
            Err(Error::StorageUnavailable | Error::CorruptState | Error::ClockUnreliable)
        ) {
            self.fault();
        }
        result
    }
    /// Reads startup configuration only. It does not reset a fault or create a dispatch ticket.
    /// # Errors
    /// Rejects missing, invalid or unavailable persisted control metadata.
    pub async fn control(&self) -> Result<(RequestEpoch, u64), Error> {
        self.bounded(Instant::now() + Duration::from_secs(5), async {
            let row = sqlx::query(
                "SELECT request_epoch,time_floor_ms FROM product_control WHERE singleton=1",
            )
            .fetch_one(&self.pool)
            .await?;
            Ok((
                RequestEpoch(decode(row.try_get("request_epoch")?)?),
                number(row.try_get("time_floor_ms")?)?,
            ))
        })
        .await
    }
    async fn complete<T>(&self, tx: Transaction, result: Result<T, Error>) -> Result<T, Error> {
        match result {
            Ok(value) => {
                tx.commit().await?;
                Ok(value)
            }
            Err(e) => {
                tx.rollback().await?;
                Err(e)
            }
        }
    }
    /// The caller supplies a trusted time port, sampled only AFTER BEGIN IMMEDIATE succeeds.
    pub(crate) async fn checkpoint(
        &self,
        actor: ActorContext,
        epoch: RequestEpoch,
        time: &impl TimeSource,
        deadline: Instant,
    ) -> Result<RequestTime, Error> {
        self.require_healthy()?;
        self.bounded(deadline, async {
            let mut tx = self.begin().await?;
            let result = async {
                require_actor(&mut tx.connection, actor).await?;
                checkpoint(&mut tx, epoch, time).await
            }
            .await;
            self.complete(tx, result).await
        })
        .await
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "The private transaction boundary explicitly verifies independent identity, owner and time coordinates"
    )]
    pub(crate) async fn admit(
        &self,
        actor: ActorContext,
        verified: &VerifiedProductRequest,
        target: &InputTarget,
        available: bool,
        time: &impl TimeSource,
        epoch: RequestEpoch,
        deadline: Instant,
    ) -> Result<(ProductCommandFact, bool), Error> {
        self.require_healthy()?;
        self.bounded(deadline,async {
            let mut tx=self.begin().await?;
            let result=async {
                require_actor(&mut tx.connection,actor).await?;
                let now=checkpoint(&mut tx,epoch,time).await?;
                let identity=verified.identity();
                if let Err(e)=valid(identity,actor,target,epoch,now) { if e==Error::Expired { return Ok(Err(e)); } return Err(e); }
                if let Some(row)=sqlx::query("SELECT c.*,r.run_id FROM product_commands c JOIN product_runs r USING(command_id) WHERE c.actor_id=? AND c.request_id=?").bind(actor.client_id.encode()).bind(identity.request.encode()).fetch_optional(&mut tx.connection).await? {
                    same(&row,identity,actor)?;
                    return Ok(Ok((fact(&row)?,true)));
                }
                if !available {return Err(Error::Busy);}
                require_target(&mut tx.connection,target).await?;
                let busy:i64=sqlx::query_scalar("SELECT COUNT(*) FROM product_runs WHERE target_id=? AND status IN ('accepted','dispatching','running','cancelling','unknown')").bind(target.target.encode()).fetch_one(&mut tx.connection).await?;
                if busy!=0 {return Err(Error::Busy);}
                let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM product_commands").fetch_one(&mut tx.connection).await?;
                let slots:i64=sqlx::query_scalar("SELECT COUNT(*) FROM product_audit_slots").fetch_one(&mut tx.connection).await?;
                if number(count)? >= COMMAND_LIMIT || !audit_room(number(slots)?,self.limit()) {return Err(Error::Capacity);}
                let command=new_id()?; let run=new_id()?;
                tx.point()?;
                sqlx::query("INSERT INTO product_commands(command_id,actor_id,auth_generation,request_id,request_epoch,action,target_kind,target_id,payload_digest,issued_at,expires_at,status) VALUES(?,?,?,?,?,'session.input','session',?,?,?,?,'accepted')")
                    .bind(command.encode()).bind(actor.client_id.encode()).bind(actor.auth_generation).bind(identity.request.encode()).bind(epoch.0.encode()).bind(target.target.encode()).bind(hex::encode(identity.intent.payload_digest)).bind(integer(identity.issued_at)?).bind(integer(identity.expires_at)?).execute(&mut tx.connection).await?;
                tx.point()?;
                sqlx::query("INSERT INTO product_runs(run_id,command_id,target_id,status) VALUES(?,?,?,'accepted')").bind(run.encode()).bind(command.encode()).bind(target.target.encode()).execute(&mut tx.connection).await?;
                for stage in ["admission","dispatch","result","reconciliation"] {
                    tx.point()?;
                    sqlx::query("INSERT INTO product_audit_slots(command_id,stage,actor_id,reserved_at) VALUES(?,?,?,?)").bind(command.encode()).bind(stage).bind(actor.client_id.encode()).bind(integer(now.millis())?).execute(&mut tx.connection).await?;
                }
                fill(&mut tx,command,"admission","accepted",now.millis()).await?;
                Ok(Ok((ProductCommandFact {command,run,target:target.target,status:ProductCommandStatus::Accepted},false)))
            }.await;
            self.complete(tx,result).await?
        }).await
    }
    /// # Errors
    /// Rejects stale tickets, changed native activity, expiry and unconfirmed claim commits.
    #[allow(
        clippy::too_many_arguments,
        reason = "The private CAS boundary keeps original request, owner and single-use claim coordinates explicit"
    )]
    pub(crate) async fn claim(
        &self,
        actor: ActorContext,
        verified: &VerifiedProductRequest,
        target: &InputTarget,
        fact: &ProductCommandFact,
        claim: EntityId,
        time: &impl TimeSource,
        epoch: RequestEpoch,
        deadline: Instant,
    ) -> Result<bool, Error> {
        self.require_healthy()?;
        self.bounded(deadline,async {
            let mut tx=self.begin().await?;
            let result=async {
                require_actor(&mut tx.connection,actor).await?;
                let now=checkpoint(&mut tx,epoch,time).await?;
                let validity=valid(verified.identity(),actor,target,epoch,now);
                if validity.as_ref().is_err_and(|e|*e!=Error::Expired) {return Err(validity.err().ok_or(Error::CorruptState)?);}
                let row=read(&mut tx.connection,fact.command).await?;
                if validity==Err(Error::Expired) {
                    if same(&row,verified.identity(),actor).is_ok() && row.try_get::<String,_>("status")?=="accepted" && row.try_get::<Option<String>,_>("claim_id")?.is_none() {
                        terminal(&mut tx,fact,InputResult::Expired,now.millis()).await?;
                    }
                    return Ok(Err(Error::Expired));
                }
                same(&row,verified.identity(),actor)?;
                if fact.run!=decode(row.try_get("run_id")?)? {return Err(Error::InvalidTicket);}
                if row.try_get::<String,_>("status")? != "accepted" || row.try_get::<Option<String>,_>("claim_id")?.is_some() { return Ok(Ok(false)); }
                require_target(&mut tx.connection,target).await?;
                tx.point()?;
                let changed=sqlx::query("UPDATE product_commands SET status='dispatching',claim_id=? WHERE command_id=? AND status='accepted' AND claim_id IS NULL").bind(claim.encode()).bind(fact.command.encode()).execute(&mut tx.connection).await?;
                if changed.rows_affected()!=1 {return Err(Error::Conflict);}
                tx.point()?;
                let changed=sqlx::query("UPDATE product_runs SET status='dispatching' WHERE command_id=? AND run_id=? AND target_id=? AND status='accepted'").bind(fact.command.encode()).bind(fact.run.encode()).bind(target.target.encode()).execute(&mut tx.connection).await?;
                if changed.rows_affected()!=1 {return Err(Error::CorruptState);}
                fill(&mut tx,fact.command,"dispatch","dispatching",now.millis()).await?;
                Ok(Ok(true))
            }.await;
            self.complete(tx,result).await?
        }).await
    }
    pub(crate) async fn final_fence(
        &self,
        actor: ActorContext,
        verified: &VerifiedProductRequest,
        target: &InputTarget,
        time: &impl TimeSource,
        epoch: RequestEpoch,
        deadline: Instant,
    ) -> Result<(), Error> {
        self.require_healthy()?;
        self.bounded(deadline, async {
            let mut tx = self.begin().await?;
            let result = async {
                require_actor(&mut tx.connection, actor).await?;
                let now = checkpoint(&mut tx, epoch, time).await?;
                valid(verified.identity(), actor, target, epoch, now)?;
                require_target(&mut tx.connection, target).await
            }
            .await;
            self.complete(tx, result).await
        })
        .await
    }
    pub(crate) async fn fail_unsent(
        &self,
        fact: &ProductCommandFact,
        claim: Option<EntityId>,
        result: InputResult,
        time: &impl TimeSource,
        deadline: Instant,
    ) -> Result<(), Error> {
        self.bounded(deadline, async {
            let mut tx = self.begin().await?;
            let outcome = async {
                let row = read(&mut tx.connection, fact.command).await?;
                if decode(row.try_get("run_id")?)? != fact.run
                    || row.try_get::<Option<String>, _>("claim_id")? != claim.map(EntityId::encode)
                {
                    return Err(Error::InvalidTicket);
                }
                if !matches!(result, InputResult::Expired | InputResult::NotSent) {
                    return Err(Error::InvalidTicket);
                }
                terminal(&mut tx, fact, result, time.sample()?.millis()).await
            }
            .await;
            self.complete(tx, outcome).await
        })
        .await
    }
    /// # Errors
    /// Rejects stale native evidence, conflicting prior results and unconfirmed result commits.
    pub(crate) async fn report(
        &self,
        receipt: &InputReceipt,
        result: InputResult,
        time: &impl TimeSource,
        deadline: Instant,
    ) -> Result<(), Error> {
        self.bounded(deadline, async {
            let mut tx = self.begin().await?;
            let outcome = async {
                let row = read(&mut tx.connection, receipt.command).await?;
                if row.try_get::<Option<String>, _>("claim_id")?.as_deref()
                    != Some(receipt.claim.encode().as_str())
                    || decode(row.try_get("run_id")?)? != receipt.run
                    || decode(row.try_get("target_id")?)? != receipt.target.target
                {
                    return Err(Error::InvalidTicket);
                }
                terminal(
                    &mut tx,
                    &ProductCommandFact {
                        command: receipt.command,
                        run: receipt.run,
                        target: receipt.target.target,
                        status: ProductCommandStatus::Dispatching,
                    },
                    result,
                    time.sample()?.millis(),
                )
                .await
            }
            .await;
            self.complete(tx, outcome).await
        })
        .await
    }
    pub(crate) async fn insertion(
        &self,
        receipt: &InputReceipt,
        deadline: Instant,
    ) -> Result<(), Error> {
        self.bounded(deadline,async {
            let mut tx=self.begin().await?;
            let result=async {
                let row=read(&mut tx.connection,receipt.command).await?;
                if row.try_get::<Option<String>,_>("claim_id")? != Some(receipt.claim.encode()) || decode(row.try_get("run_id")?)? != receipt.run {return Err(Error::InvalidTicket);}
                tx.point()?;
                let result=sqlx::query("UPDATE product_runs SET status='running' WHERE run_id=? AND command_id=? AND status IN ('dispatching','running')").bind(receipt.run.encode()).bind(receipt.command.encode()).execute(&mut tx.connection).await?;
                if result.rows_affected()!=1 {return Err(Error::Conflict);}
                Ok(())
            }.await;
            self.complete(tx,result).await
        }).await
    }
    pub(crate) async fn get(
        &self,
        actor: ActorContext,
        command: EntityId,
        deadline: Instant,
    ) -> Result<ProductCommandFact, Error> {
        self.bounded(deadline, async {
            let mut tx = self.begin_with("BEGIN").await?;
            let result = async {
                require_actor(&mut tx.connection, actor).await?;
                let row = read(&mut tx.connection, command).await?;
                if row.try_get::<String, _>("actor_id")? != actor.client_id.encode() {
                    return Err(Error::InvalidActor);
                }
                fact(&row)
            }
            .await;
            self.complete(tx, result).await
        })
        .await
    }
    /// Conservative startup marking only. It never clears a sticky fault or releases resources.
    /// # Errors
    /// Rejects unreliable time and unconfirmed startup marking transactions.
    pub async fn mark_interrupted(&self, time: &impl TimeSource) -> Result<u64, Error> {
        let(count,needs_reconciliation)=self.bounded(Instant::now()+Duration::from_secs(5),async {
            let mut tx=self.begin().await?;
            let result=async {
                let row=sqlx::query("SELECT request_epoch FROM product_control WHERE singleton=1").fetch_one(&mut tx.connection).await?;
                let epoch=RequestEpoch(decode(row.try_get("request_epoch")?)?);
                let now=checkpoint(&mut tx,epoch,time).await?.millis();
                let rows=sqlx::query("SELECT c.*,r.run_id FROM product_commands c JOIN product_runs r USING(command_id) WHERE c.status IN ('accepted','dispatching') OR r.status IN ('accepted','dispatching','running','cancelling','unknown')").fetch_all(&mut tx.connection).await?;
                for row in &rows {
                    let f=fact(row)?;
                    let existing:Option<String>=sqlx::query_scalar("SELECT outcome FROM product_audit_slots WHERE command_id=? AND stage='result'").bind(f.command.encode()).fetch_one(&mut tx.connection).await?;
                    if existing.is_none() {terminal(&mut tx,&f,InputResult::Unknown,now).await?;}
                    else {
                        if matches!(f.status,ProductCommandStatus::Accepted|ProductCommandStatus::Dispatching) {return Err(Error::CorruptState);}
                        sqlx::query("UPDATE product_runs SET status='unknown' WHERE run_id=? AND status IN ('accepted','dispatching','running','cancelling')").bind(f.run.encode()).execute(&mut tx.connection).await?;
                    }
                }
                tx.point()?;
                let resources=sqlx::query("UPDATE product_resources SET state='unknown' WHERE state!='released'").execute(&mut tx.connection).await?;
                Ok((u64::try_from(rows.len()).map_err(|_|Error::CorruptState)?,!rows.is_empty() || resources.rows_affected()!=0))
            }.await;
            self.complete(tx,result).await
        }).await?;
        if needs_reconciliation {
            self.fault();
        }
        Ok(count)
    }
}
#[allow(
    clippy::needless_pass_by_value,
    reason = "SQL record decoding consumes an owned column value"
)]
fn decode(s: String) -> Result<EntityId, Error> {
    EntityId::decode(&s).map_err(|_| Error::CorruptState)
}
fn integer(n: u64) -> Result<i64, Error> {
    if n > MAX_REQUEST_TIME {
        return Err(Error::CorruptState);
    }
    i64::try_from(n).map_err(|_| Error::CorruptState)
}
fn number(n: i64) -> Result<u64, Error> {
    let n = u64::try_from(n).map_err(|_| Error::CorruptState)?;
    if n > MAX_REQUEST_TIME {
        Err(Error::CorruptState)
    } else {
        Ok(n)
    }
}
pub(crate) fn new_id() -> Result<EntityId, Error> {
    crate::random::bytes()
        .map(EntityId::from_bytes)
        .map_err(|_| Error::RandomUnavailable)
}
async fn require_actor(c: &mut SqliteConnection, a: ActorContext) -> Result<(), Error> {
    if a.auth_generation < 0
        || u64::try_from(a.auth_generation).map_err(|_| Error::InvalidActor)? > MAX_REQUEST_TIME
    {
        return Err(Error::InvalidActor);
    }
    let found: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM clients WHERE client_id=? AND auth_generation=? AND status='trusted'",
    )
    .bind(a.client_id.encode())
    .bind(a.auth_generation)
    .fetch_one(c)
    .await?;
    if found == 1 {
        Ok(())
    } else {
        Err(Error::InvalidActor)
    }
}
async fn checkpoint(
    tx: &mut Transaction,
    epoch: RequestEpoch,
    time: &impl TimeSource,
) -> Result<RequestTime, Error> {
    let row =
        sqlx::query("SELECT request_epoch,time_floor_ms FROM product_control WHERE singleton=1")
            .fetch_one(&mut tx.connection)
            .await?;
    if row.try_get::<String, _>("request_epoch")? != epoch.0.encode() {
        return Err(Error::EpochMismatch);
    }
    let floor = number(row.try_get("time_floor_ms")?)?;
    let now = time.sample()?;
    if now.millis() < floor {
        return Err(Error::ClockUnreliable);
    }
    tx.point()?;
    #[cfg(test)]
    let expected = if matches!(tx.fault, Some(Fault::CasZero)) {
        floor.checked_add(1).ok_or(Error::CorruptState)?
    } else {
        floor
    };
    #[cfg(not(test))]
    let expected = floor;
    let changed=sqlx::query("UPDATE product_control SET time_floor_ms=? WHERE singleton=1 AND request_epoch=? AND time_floor_ms=?").bind(integer(now.millis())?).bind(epoch.0.encode()).bind(integer(expected)?).execute(&mut tx.connection).await?;
    if changed.rows_affected() != 1 {
        return Err(Error::StorageUnavailable);
    }
    Ok(now)
}
fn valid(
    i: &ProductRequestIdentity,
    a: ActorContext,
    t: &InputTarget,
    epoch: RequestEpoch,
    now: RequestTime,
) -> Result<(), Error> {
    if i.epoch != epoch {
        return Err(Error::EpochMismatch);
    }
    if i.intent.actor != a.client_id
        || i.intent.action != ProductAction::SessionInput
        || i.intent.target.kind != ProductTargetKind::Session
        || i.intent.target.id != t.target
        || i.issued_at > now.millis()
        || i.expires_at <= i.issued_at
        || i.expires_at - i.issued_at > 86_400_000
    {
        return Err(Error::InvalidIdentity);
    }
    if now.millis() >= i.expires_at {
        return Err(Error::Expired);
    }
    Ok(())
}
async fn require_target(c: &mut SqliteConnection, t: &InputTarget) -> Result<(), Error> {
    let found:i64=sqlx::query_scalar("SELECT COUNT(*) FROM product_targets t JOIN product_resources r USING(target_id) WHERE t.target_id=? AND t.source_id=? AND t.native_id=? AND r.load_epoch=? AND r.runtime_epoch=? AND r.connection_epoch=? AND r.instance_id=? AND r.revision=? AND r.state='loaded'").bind(t.target.encode()).bind(t.native.source().0.encode()).bind(t.native.native_id()).bind(t.epoch.load.0.encode()).bind(t.epoch.runtime.0.encode()).bind(t.epoch.connection.0.encode()).bind(t.instance.encode()).bind(integer(t.revision)?).fetch_one(c).await?;
    if found == 1 {
        Ok(())
    } else {
        Err(Error::InvalidTarget)
    }
}
async fn read(c: &mut SqliteConnection, id: EntityId) -> Result<SqliteRow, Error> {
    sqlx::query("SELECT c.*,r.run_id FROM product_commands c JOIN product_runs r USING(command_id) WHERE c.command_id=?").bind(id.encode()).fetch_optional(c).await?.ok_or(Error::InvalidTicket)
}
fn same(row: &SqliteRow, i: &ProductRequestIdentity, a: ActorContext) -> Result<(), Error> {
    if row.try_get::<String, _>("request_epoch")? != i.epoch.0.encode()
        || row.try_get::<String, _>("request_id")? != i.request.encode()
        || row.try_get::<String, _>("actor_id")? != a.client_id.encode()
        || row.try_get::<i64, _>("auth_generation")? != a.auth_generation
        || row.try_get::<String, _>("action")? != i.intent.action.label()
        || row.try_get::<String, _>("target_kind")? != i.intent.target.kind.label()
        || row.try_get::<String, _>("target_id")? != i.intent.target.id.encode()
        || row.try_get::<String, _>("payload_digest")? != hex::encode(i.intent.payload_digest)
        || number(row.try_get("issued_at")?)? != i.issued_at
        || number(row.try_get("expires_at")?)? != i.expires_at
    {
        return Err(Error::Conflict);
    }
    Ok(())
}
fn fact(row: &SqliteRow) -> Result<ProductCommandFact, Error> {
    Ok(ProductCommandFact {
        command: decode(row.try_get("command_id")?)?,
        run: decode(row.try_get("run_id")?)?,
        target: decode(row.try_get("target_id")?)?,
        status: ProductCommandStatus::parse(&row.try_get::<String, _>("status")?)?,
    })
}
async fn fill(
    tx: &mut Transaction,
    command: EntityId,
    stage: &str,
    outcome: &str,
    now: u64,
) -> Result<(), Error> {
    tx.point()?;
    let changed=sqlx::query("UPDATE product_audit_slots SET occurred_at=?,retain_until=?,outcome=? WHERE command_id=? AND stage=? AND occurred_at IS NULL AND retain_until IS NULL AND outcome IS NULL").bind(integer(now)?).bind(integer(retention(now)?)?).bind(outcome).bind(command.encode()).bind(stage).execute(&mut tx.connection).await?;
    if changed.rows_affected() != 1 {
        return Err(Error::Conflict);
    }
    Ok(())
}
async fn terminal(
    tx: &mut Transaction,
    f: &ProductCommandFact,
    result: InputResult,
    now: u64,
) -> Result<(), Error> {
    let (command_status, run_status, outcome) = result.labels();
    let existing = sqlx::query(
        "SELECT occurred_at,outcome FROM product_audit_slots WHERE command_id=? AND stage='result'",
    )
    .bind(f.command.encode())
    .fetch_one(&mut tx.connection)
    .await?;
    if let Some(old) = existing.try_get::<Option<String>, _>("outcome")? {
        return if old == outcome {
            Ok(())
        } else {
            Err(Error::Conflict)
        };
    }
    tx.point()?;
    let changed=sqlx::query("UPDATE product_commands SET status=? WHERE command_id=? AND status IN ('accepted','dispatching')").bind(command_status).bind(f.command.encode()).execute(&mut tx.connection).await?;
    if changed.rows_affected() != 1 {
        return Err(Error::Conflict);
    }
    tx.point()?;
    let changed=sqlx::query("UPDATE product_runs SET status=? WHERE run_id=? AND command_id=? AND target_id=? AND status IN ('accepted','dispatching','running','cancelling','unknown')").bind(run_status).bind(f.run.encode()).bind(f.command.encode()).bind(f.target.encode()).execute(&mut tx.connection).await?;
    if changed.rows_affected() != 1 {
        return Err(Error::CorruptState);
    }
    fill(tx, f.command, "result", outcome, now).await
}
#[cfg(test)]
#[path = "../../tests/registration_acceptance/c/management_independent.rs"]
mod independent_acceptance;
#[cfg(test)]
mod tests;

impl crate::application::product_commands::ProductRepository for ProductStore {
    fn fault_signal(&self) -> crate::application::product_commands::FaultSignal {
        crate::application::product_commands::FaultSignal(self.faulted.clone())
    }
    async fn control(&self) -> Result<(RequestEpoch, u64), Error> {
        Self::control(self).await
    }
    async fn mark_interrupted(&self, time: &impl TimeSource) -> Result<u64, Error> {
        Self::mark_interrupted(self, time).await
    }
    async fn checkpoint(
        &self,
        actor: &crate::application::product_commands::CurrentActor,
        epoch: RequestEpoch,
        time: &impl TimeSource,
        deadline: Instant,
    ) -> Result<RequestTime, Error> {
        Self::checkpoint(self, actor.context(), epoch, time, deadline).await
    }
    async fn admit(
        &self,
        actor: &crate::application::product_commands::CurrentActor,
        verified: &VerifiedProductRequest,
        target: &InputTarget,
        available: bool,
        time: &impl TimeSource,
        epoch: RequestEpoch,
        deadline: Instant,
    ) -> Result<(ProductCommandFact, bool), Error> {
        Self::admit(
            self,
            actor.context(),
            verified,
            target,
            available,
            time,
            epoch,
            deadline,
        )
        .await
    }
    async fn claim(
        &self,
        actor: &crate::application::product_commands::CurrentActor,
        verified: &VerifiedProductRequest,
        target: &InputTarget,
        fact: &ProductCommandFact,
        claim: EntityId,
        time: &impl TimeSource,
        epoch: RequestEpoch,
        deadline: Instant,
    ) -> Result<bool, Error> {
        Self::claim(
            self,
            actor.context(),
            verified,
            target,
            fact,
            claim,
            time,
            epoch,
            deadline,
        )
        .await
    }
    async fn final_fence(
        &self,
        actor: &crate::application::product_commands::CurrentActor,
        verified: &VerifiedProductRequest,
        target: &InputTarget,
        time: &impl TimeSource,
        epoch: RequestEpoch,
        deadline: Instant,
    ) -> Result<(), Error> {
        Self::final_fence(
            self,
            actor.context(),
            verified,
            target,
            time,
            epoch,
            deadline,
        )
        .await
    }
    async fn fail_unsent(
        &self,
        ownership: &crate::application::product_commands::CommandOwnership,
        result: InputResult,
        time: &impl TimeSource,
        deadline: Instant,
    ) -> Result<(), Error> {
        Self::fail_unsent(
            self,
            ownership.fact(),
            ownership.claim(),
            result,
            time,
            deadline,
        )
        .await
    }
    async fn report(
        &self,
        receipt: &InputReceipt,
        result: InputResult,
        time: &impl TimeSource,
        deadline: Instant,
    ) -> Result<(), Error> {
        Self::report(self, receipt, result, time, deadline).await
    }
    async fn insertion(&self, receipt: &InputReceipt, deadline: Instant) -> Result<(), Error> {
        Self::insertion(self, receipt, deadline).await
    }
    async fn get(
        &self,
        actor: &crate::application::product_commands::CurrentActor,
        command: EntityId,
        deadline: Instant,
    ) -> Result<ProductCommandFact, Error> {
        Self::get(self, actor.context(), command, deadline).await
    }
}
