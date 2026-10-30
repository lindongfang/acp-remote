//! SQLite repository for durable input admission. No network or process dispatch lives here.
pub mod approvals;
pub mod history;
pub(crate) mod schema;
use crate::{
    application::InputRepository,
    domain::{
        AcceptedInput, ActorContext, Admission, CommandRecord, CommandStatus, DispatchPermit,
        EntityId, InputRequest, SessionError,
    },
};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool, sqlite::SqliteRow};
use std::time::{SystemTime, UNIX_EPOCH};
const MAX_COMMANDS: i64 = 100_000;
const MAX_CONTENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

impl From<sqlx::Error> for SessionError {
    fn from(_: sqlx::Error) -> Self {
        Self::StorageUnavailable
    }
}

#[derive(Clone)]
pub struct SessionStore {
    pool: SqlitePool,
}
impl SessionStore {
    pub(crate) const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Persists one fully assembled input object; chunk assembly is a separate gateway concern.
    /// # Errors
    /// Rejects stale actors, missing sessions, oversized content and failed transactions.
    pub async fn persist_input(
        &self,
        actor: ActorContext,
        session: EntityId,
        body: &[u8],
    ) -> Result<EntityId, SessionError> {
        if body.len() > MAX_CONTENT_BYTES {
            return Err(SessionError::InvalidContent);
        }
        let reference = new_id()?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        require_actor(&mut transaction, actor).await?;
        require_session(&mut transaction, &session.encode()).await?;
        sqlx::query("INSERT INTO content_objects(content_ref,session_id,actor_id,auth_generation,purpose,body,sha256,total_bytes) VALUES(?,?,?,?,'input',?,?,?)")
            .bind(reference.encode()).bind(session.encode()).bind(actor.client_id.encode()).bind(actor.auth_generation)
            .bind(body).bind(hex::encode(Sha256::digest(body)))
            .bind(i64::try_from(body.len()).map_err(|_| SessionError::InvalidContent)?)
            .execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(reference)
    }

    /// Atomically accepts a command, its input, turn, run, reference and metadata-only audit.
    /// # Errors
    /// Rejects stale actors, conflicts, unavailable references, busy sessions or write failures.
    #[allow(clippy::too_many_lines)]
    pub async fn admit_input(
        &self,
        actor: ActorContext,
        request: InputRequest,
    ) -> Result<Admission, SessionError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        require_actor(&mut transaction, actor).await?;
        let session_id = request.session_id.encode();
        let reference = request.content_ref.encode();
        let content = sqlx::query("SELECT body,sha256 FROM content_objects WHERE content_ref=? AND session_id=? AND actor_id=? AND auth_generation=? AND purpose='input'")
            .bind(&reference).bind(&session_id).bind(actor.client_id.encode()).bind(actor.auth_generation)
            .fetch_optional(&mut *transaction).await?.ok_or(SessionError::InvalidContent)?;
        let body: Vec<u8> = content.try_get("body")?;
        let content_digest: String = content.try_get("sha256")?;
        if body.len() > MAX_CONTENT_BYTES || hex::encode(Sha256::digest(&body)) != content_digest {
            return Err(SessionError::InvalidContent);
        }
        let digest = input_digest(request, &content_digest);
        if let Some(row) = sqlx::query("SELECT command_id,action,payload_digest FROM session_commands WHERE actor_id=? AND request_id=?")
            .bind(actor.client_id.encode()).bind(request.request_id.encode()).fetch_optional(&mut *transaction).await? {
            let stored_digest: String = row.try_get("payload_digest")?;
            if row.try_get::<String,_>("action")? != "session.input" || stored_digest != digest { return Err(SessionError::RequestConflict); }
            let command_id: String = row.try_get("command_id")?;
            let command = read_command(&mut transaction, actor, &command_id).await?;
            transaction.commit().await?;
            return Ok(Admission { command, replayed: true });
        }
        let state = require_session(&mut transaction, &session_id).await?;
        if state != "loaded" {
            return Err(SessionError::InvalidState);
        }
        let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE session_id=? AND status IN ('accepted','dispatching','running','cancelling','unknown')")
            .bind(&session_id).fetch_one(&mut *transaction).await?;
        if active != 0 {
            return Err(SessionError::Busy);
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_commands")
            .fetch_one(&mut *transaction)
            .await?;
        if count >= MAX_COMMANDS {
            return Err(SessionError::CapacityExhausted);
        }
        let sequence = sqlx::query(
            "SELECT revision,next_position,next_turn FROM session_state WHERE session_id=?",
        )
        .bind(&session_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(SessionError::CorruptState)?;
        let revision: i64 = sequence.try_get("revision")?;
        let position: i64 = sequence.try_get("next_position")?;
        let ordinal: i64 = sequence.try_get("next_turn")?;
        if [revision, position, ordinal]
            .iter()
            .any(|value| !(0..MAX_SAFE).contains(value))
        {
            return Err(SessionError::CapacityExhausted);
        }
        let input = AcceptedInput {
            command_id: new_id()?.encode(),
            run_id: new_id()?.encode(),
            turn_id: new_id()?.encode(),
            item_id: new_id()?.encode(),
        };
        sqlx::query("INSERT INTO session_commands(command_id,actor_id,request_id,action,payload_digest,session_id,status) VALUES(?,?,?,'session.input',?,?,'accepted')")
            .bind(&input.command_id).bind(actor.client_id.encode()).bind(request.request_id.encode()).bind(digest).bind(&session_id)
            .execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO turns(turn_id,session_id,command_id,ordinal) VALUES(?,?,?,?)")
            .bind(&input.turn_id)
            .bind(&session_id)
            .bind(&input.command_id)
            .bind(ordinal)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("INSERT INTO runs(run_id,session_id,command_id,turn_id,status) VALUES(?,?,?,?,'accepted')")
            .bind(&input.run_id).bind(&session_id).bind(&input.command_id).bind(&input.turn_id).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO conversation_items(item_id,session_id,turn_id,source_id,first_position,kind,content_ref,content_version,committed_version) VALUES(?,?,?,?,?,'user',?,1,1)")
            .bind(&input.item_id).bind(&session_id).bind(&input.turn_id).bind(&input.item_id).bind(position).bind(reference).execute(&mut *transaction).await?;
        sqlx::query("UPDATE session_state SET revision=revision+1,next_position=next_position+1,next_turn=next_turn+1 WHERE session_id=?")
            .bind(&session_id).execute(&mut *transaction).await?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|value| i64::try_from(value.as_secs()).ok())
            .ok_or(SessionError::CorruptState)?;
        sqlx::query("INSERT INTO security_audit(actor_id,action,target_id,outcome,occurred_at) VALUES(?,'session.input',?,'accepted',?)")
            .bind(actor.client_id.encode()).bind(&input.command_id).bind(now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(Admission {
            command: CommandRecord {
                input,
                status: CommandStatus::Accepted,
            },
            replayed: false,
        })
    }

    /// Reads only this current actor's original association, without dispatching it.
    /// # Errors
    /// Rejects stale actors, inaccessible commands and unavailable/corrupt records.
    pub async fn command(
        &self,
        actor: ActorContext,
        command_id: &str,
    ) -> Result<CommandRecord, SessionError> {
        let mut transaction = self.pool.begin().await?;
        require_actor(&mut transaction, actor).await?;
        let record = read_command(&mut transaction, actor, command_id).await?;
        transaction.commit().await?;
        Ok(record)
    }

    /// Claims one durable dispatch attempt; an accepted replay does not create another permit.
    /// The application execution owner must hold its current actor gate across the side effect.
    /// # Errors
    /// Returns actor, state or storage errors before any external dispatch is possible.
    pub async fn claim_dispatch(
        &self,
        actor: ActorContext,
        command_id: &str,
    ) -> Result<Option<DispatchPermit>, SessionError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        require_actor(&mut transaction, actor).await?;
        let record = read_command(&mut transaction, actor, command_id).await?;
        if record.status != CommandStatus::Accepted {
            return Ok(None);
        }
        let state: String = sqlx::query_scalar("SELECT s.state FROM sessions s JOIN session_commands c ON c.session_id=s.session_id WHERE c.command_id=?")
            .bind(command_id).fetch_one(&mut *transaction).await?;
        if state != "loaded" {
            return Err(SessionError::InvalidState);
        }
        sqlx::query("UPDATE session_commands SET status='dispatching' WHERE command_id=? AND status='accepted'")
            .bind(command_id).execute(&mut *transaction).await?;
        let run = sqlx::query(
            "UPDATE runs SET status='dispatching' WHERE command_id=? AND status='accepted'",
        )
        .bind(command_id)
        .execute(&mut *transaction)
        .await?;
        if run.rows_affected() != 1 {
            return Err(SessionError::CorruptState);
        }
        transaction.commit().await?;
        Ok(Some(DispatchPermit {
            input: record.input,
        }))
    }

    /// Startup reconciliation, called once by the node owner before accepting session work.
    /// Does not dispatch accepted or possibly dispatched inputs.
    /// # Errors
    /// A failed recovery transaction leaves original facts unchanged and readiness must fail.
    pub async fn recover_interrupted(&self) -> Result<u64, SessionError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        approvals::recover(&mut transaction)
            .await
            .map_err(|_| SessionError::StorageUnavailable)?;
        sqlx::query("DELETE FROM history_content_pins")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM content_objects WHERE purpose='output' AND NOT EXISTS(SELECT 1 FROM conversation_items WHERE conversation_items.content_ref=content_objects.content_ref)").execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO history_integrity(session_id,run_id,reason,first_position,last_position,dropped_bytes) SELECT r.session_id,r.run_id,'interrupted',COALESCE((SELECT MIN(i.first_position) FROM conversation_items i WHERE i.turn_id=r.turn_id),0),MAX(0,st.next_position-1),0 FROM runs r JOIN session_state st ON st.session_id=r.session_id WHERE r.status IN ('accepted','dispatching','running','cancelling','unknown') ON CONFLICT(session_id,run_id) DO UPDATE SET reason='interrupted',first_position=CASE WHEN history_integrity.first_position IS NULL THEN excluded.first_position ELSE MIN(history_integrity.first_position,excluded.first_position) END,last_position=CASE WHEN history_integrity.last_position IS NULL THEN excluded.last_position ELSE MAX(history_integrity.last_position,excluded.last_position) END").execute(&mut *transaction).await?;
        sqlx::query("UPDATE sessions SET state='interrupted' WHERE session_id IN (SELECT session_id FROM runs WHERE status IN ('accepted','dispatching','running','cancelling','unknown'))")
            .execute(&mut *transaction).await?;
        sqlx::query("UPDATE runs SET status='unknown' WHERE status IN ('accepted','dispatching','running','cancelling')")
            .execute(&mut *transaction).await?;
        let commands = sqlx::query("UPDATE session_commands SET status='delivery_unknown' WHERE action='session.input' AND status IN ('accepted','dispatching')")
            .execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(commands.rows_affected())
    }
}
impl InputRepository for SessionStore {
    async fn admit(
        &self,
        actor: ActorContext,
        request: InputRequest,
    ) -> Result<Admission, SessionError> {
        self.admit_input(actor, request).await
    }
}
fn new_id() -> Result<EntityId, SessionError> {
    crate::random::bytes()
        .map(EntityId::from_bytes)
        .map_err(|_| SessionError::RandomUnavailable)
}
async fn require_actor(
    connection: &mut SqliteConnection,
    actor: ActorContext,
) -> Result<(), SessionError> {
    let current: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM clients WHERE client_id=? AND auth_generation=? AND status='trusted'",
    )
    .bind(actor.client_id.encode())
    .bind(actor.auth_generation)
    .fetch_one(connection)
    .await?;
    if current == 1 {
        Ok(())
    } else {
        Err(SessionError::InvalidActor)
    }
}
async fn require_session(
    connection: &mut SqliteConnection,
    session: &str,
) -> Result<String, SessionError> {
    sqlx::query_scalar("SELECT state FROM sessions WHERE session_id=?")
        .bind(session)
        .fetch_optional(connection)
        .await?
        .ok_or(SessionError::NotFound)
}
fn input_digest(request: InputRequest, content_digest: &str) -> String {
    let canonical = format!(
        r#"{{"action":"session.input","payload":{{"contentRef":"{}","sessionId":"{}"}}}}"#,
        request.content_ref.encode(),
        request.session_id.encode()
    );
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hasher.update(b"\0content-sha256\0");
    hasher.update(content_digest.as_bytes());
    hex::encode(hasher.finalize())
}
fn decode_record(row: &SqliteRow) -> Result<CommandRecord, SessionError> {
    let status: String = row.try_get("status")?;
    Ok(CommandRecord {
        input: AcceptedInput {
            command_id: row.try_get("command_id")?,
            run_id: row.try_get("run_id")?,
            turn_id: row.try_get("turn_id")?,
            item_id: row.try_get("item_id")?,
        },
        status: CommandStatus::from_storage(&status).ok_or(SessionError::CorruptState)?,
    })
}
async fn read_command(
    connection: &mut SqliteConnection,
    actor: ActorContext,
    command: &str,
) -> Result<CommandRecord, SessionError> {
    let row = sqlx::query("SELECT c.command_id,c.status,r.run_id,r.turn_id,i.item_id FROM session_commands c JOIN runs r ON r.command_id=c.command_id JOIN conversation_items i ON i.turn_id=r.turn_id AND i.kind='user' WHERE c.actor_id=? AND c.command_id=?")
        .bind(actor.client_id.encode()).bind(command).fetch_optional(connection).await?.ok_or(SessionError::NotFound)?;
    decode_record(&row)
}
#[cfg(test)]
mod tests;
