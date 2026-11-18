//! Durable native-release operation identity. Rows are body-free and actor-bound.
use super::{
    RegistryConnection, registry_connection_owner::CloseOutcome,
    request_registry::SqliteRequestRegistry,
};
use crate::{
    application::request_registry::RegistryDeadline,
    domain::request_registry::RegistryError,
    domain::{ActorContext, EntityId},
};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};
use std::{future::Future, pin::Pin};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeReleaseStoredPhase {
    Registered,
    Releasing,
    Released,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeReleaseRecord {
    pub(crate) operation: EntityId,
    pub(crate) request: EntityId,
    pub(crate) target: EntityId,
    pub(crate) phase: NativeReleaseStoredPhase,
    pub(crate) error_code: Option<String>,
}

#[derive(Clone)]
pub struct SqliteNativeReleaseOperations {
    registry: SqliteRequestRegistry,
}

impl SqliteNativeReleaseOperations {
    pub(crate) const fn new(registry: SqliteRequestRegistry) -> Self {
        Self { registry }
    }

    pub(crate) async fn register(
        &self,
        actor: ActorContext,
        target: EntityId,
        request: EntityId,
        operation: EntityId,
        registered_at_ms: i64,
        deadline: &RegistryDeadline,
    ) -> Result<NativeReleaseRecord, RegistryError> {
        if registered_at_ms < 0 {
            return Err(RegistryError::StorageUnavailable);
        }
        transaction(&self.registry, deadline, move |connection| {
            Box::pin(async move {
                if let Some(current) = read_request(connection, actor, request).await? {
                    return if current.target == target {
                        Ok(current)
                    } else {
                        Err(RegistryError::Conflict)
                    };
                }
                let inserted = sqlx::query(
                    "INSERT INTO node_native_release_operations(\
                     operation_id,request_id,actor_id,actor_auth_generation,target_id,phase,\
                     error_code,registered_at_ms,updated_at_ms) \
                     VALUES(?,?,?,?,?,'registered',NULL,?,?)",
                )
                .bind(operation.encode())
                .bind(request.encode())
                .bind(actor.client_id.encode())
                .bind(actor.auth_generation)
                .bind(target.encode())
                .bind(registered_at_ms)
                .bind(registered_at_ms)
                .execute(&mut *connection)
                .await;
                if let Err(error) = inserted {
                    return if error
                        .as_database_error()
                        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
                    {
                        Err(RegistryError::Conflict)
                    } else {
                        Err(RegistryError::StorageUnavailable)
                    };
                }
                read_owned(connection, actor, operation).await
            })
        })
        .await
    }

    pub(crate) async fn inspect(
        &self,
        actor: ActorContext,
        operation: EntityId,
        deadline: &RegistryDeadline,
    ) -> Result<NativeReleaseRecord, RegistryError> {
        transaction(&self.registry, deadline, move |connection| {
            Box::pin(async move { read_owned(connection, actor, operation).await })
        })
        .await
    }

    pub(crate) async fn begin(
        &self,
        actor: ActorContext,
        operation: EntityId,
        at_ms: i64,
        deadline: &RegistryDeadline,
    ) -> Result<NativeReleaseRecord, RegistryError> {
        transaction(&self.registry, deadline, move |connection| {
            Box::pin(async move {
                let current = read_owned(connection, actor, operation).await?;
                if current.phase != NativeReleaseStoredPhase::Registered {
                    return Ok(current);
                }
                super::session_capacity::bind_capacity_release(
                    connection,
                    current.operation,
                    current.target,
                )
                .await?;
                let updated_at_ms = read_registered_at(connection, operation).await?.max(at_ms);
                let changed = sqlx::query(
                    "UPDATE node_native_release_operations SET phase='releasing',updated_at_ms=? \
                     WHERE operation_id=? AND actor_id=? AND actor_auth_generation=? \
                       AND phase='registered'",
                )
                .bind(updated_at_ms)
                .bind(operation.encode())
                .bind(actor.client_id.encode())
                .bind(actor.auth_generation)
                .execute(&mut *connection)
                .await
                .map_err(|_| RegistryError::StorageUnavailable)?;
                if changed.rows_affected() != 1 {
                    return Err(RegistryError::Conflict);
                }
                read_owned(connection, actor, operation).await
            })
        })
        .await
    }

    pub(crate) async fn finish(
        &self,
        actor: ActorContext,
        operation: EntityId,
        error_code: Option<&str>,
        at_ms: i64,
        deadline: &RegistryDeadline,
    ) -> Result<NativeReleaseRecord, RegistryError> {
        if error_code.is_some_and(|value| value.len() > 64) {
            return Err(RegistryError::Conflict);
        }
        let error_code = error_code.map(str::to_owned);
        transaction(&self.registry, deadline, move |connection| {
            let error_code = error_code.clone();
            Box::pin(async move {
                let current = read_owned(connection, actor, operation).await?;
                let expected = error_code.as_deref();
                if current.phase == NativeReleaseStoredPhase::Released && expected.is_none()
                    || current.phase == NativeReleaseStoredPhase::Failed
                        && current.error_code.as_deref() == expected
                {
                    if current.phase == NativeReleaseStoredPhase::Released {
                        super::session_capacity::release_capacity_for_operation(connection, current.operation, current.target).await?;
                    }
                    return Ok(current);
                }
                if current.phase != NativeReleaseStoredPhase::Releasing {
                    return Err(RegistryError::Conflict);
                }
                let phase = if expected.is_some() {
                    "failed"
                } else {
                    "released"
                };
                let updated_at_ms = read_registered_at(connection, operation).await?.max(at_ms);
                let changed = sqlx::query(
                    "UPDATE node_native_release_operations SET phase=?,error_code=?,updated_at_ms=? \
                     WHERE operation_id=? AND actor_id=? AND actor_auth_generation=? \
                       AND phase='releasing'",
                )
                .bind(phase)
                .bind(expected)
                .bind(updated_at_ms)
                .bind(operation.encode())
                .bind(actor.client_id.encode())
                .bind(actor.auth_generation)
                .execute(&mut *connection)
                .await
                .map_err(|_| RegistryError::StorageUnavailable)?;
                if changed.rows_affected() != 1 {
                    return Err(RegistryError::Conflict);
                }
                if expected.is_none() {
                    super::session_capacity::release_capacity_for_operation(connection, current.operation, current.target).await?;
                }
                read_owned(connection, actor, operation).await
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests;

type TransactionFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, RegistryError>> + Send + 'a>>;

async fn transaction<T>(
    registry: &SqliteRequestRegistry,
    deadline: &RegistryDeadline,
    work: impl for<'a> FnOnce(&'a mut SqliteConnection) -> TransactionFuture<'a, T>,
) -> Result<T, RegistryError> {
    let scope = registry.owner_scope();
    scope
        .while_open(deadline.run(async {
            let mut connection = registry.detached_owned_connection().await?;
            sqlx::query("BEGIN IMMEDIATE")
                .execute(connection.connection()?)
                .await?;
            let result = work(connection.connection()?).await;
            let result = match result {
                Ok(value) => match sqlx::query("COMMIT")
                    .execute(connection.connection()?)
                    .await
                {
                    Ok(_) => Ok(value),
                    Err(_) => Err(RegistryError::CommitUnconfirmed),
                },
                Err(error) => {
                    if sqlx::query("ROLLBACK")
                        .execute(connection.connection()?)
                        .await
                        .is_err()
                    {
                        Err(RegistryError::StorageUnavailable)
                    } else {
                        Err(error)
                    }
                }
            };
            close(connection, result).await
        }))
        .await
}

async fn close<T>(
    connection: RegistryConnection,
    result: Result<T, RegistryError>,
) -> Result<T, RegistryError> {
    match connection.close().await {
        CloseOutcome::Confirmed => result,
        CloseOutcome::Unconfirmed => Err(RegistryError::StorageUnavailable),
    }
}

async fn read_registered_at(
    connection: &mut SqliteConnection,
    operation: EntityId,
) -> Result<i64, RegistryError> {
    sqlx::query_scalar(
        "SELECT registered_at_ms FROM node_native_release_operations WHERE operation_id=?",
    )
    .bind(operation.encode())
    .fetch_optional(connection)
    .await
    .map_err(|_| RegistryError::StorageUnavailable)?
    .ok_or(RegistryError::UnknownRegistration)
}

async fn read_owned(
    connection: &mut SqliteConnection,
    actor: ActorContext,
    operation: EntityId,
) -> Result<NativeReleaseRecord, RegistryError> {
    let row = sqlx::query(
        "SELECT * FROM node_native_release_operations WHERE operation_id=? AND actor_id=? \
         AND actor_auth_generation=?",
    )
    .bind(operation.encode())
    .bind(actor.client_id.encode())
    .bind(actor.auth_generation)
    .fetch_optional(connection)
    .await
    .map_err(|_| RegistryError::StorageUnavailable)?
    .ok_or(RegistryError::UnknownRegistration)?;
    decode(&row)
}

async fn read_request(
    connection: &mut SqliteConnection,
    actor: ActorContext,
    request: EntityId,
) -> Result<Option<NativeReleaseRecord>, RegistryError> {
    let row = sqlx::query(
        "SELECT * FROM node_native_release_operations \
         WHERE actor_id=? AND actor_auth_generation=? AND request_id=?",
    )
    .bind(actor.client_id.encode())
    .bind(actor.auth_generation)
    .bind(request.encode())
    .fetch_optional(connection)
    .await
    .map_err(|_| RegistryError::StorageUnavailable)?;
    row.as_ref().map(decode).transpose()
}

fn decode(row: &SqliteRow) -> Result<NativeReleaseRecord, RegistryError> {
    let operation = EntityId::decode(
        row.try_get::<String, _>("operation_id")
            .map_err(|_| RegistryError::CorruptState)?
            .as_str(),
    )
    .map_err(|_| RegistryError::CorruptState)?;
    let target = EntityId::decode(
        row.try_get::<String, _>("target_id")
            .map_err(|_| RegistryError::CorruptState)?
            .as_str(),
    )
    .map_err(|_| RegistryError::CorruptState)?;
    let request = EntityId::decode(
        row.try_get::<String, _>("request_id")
            .map_err(|_| RegistryError::CorruptState)?
            .as_str(),
    )
    .map_err(|_| RegistryError::CorruptState)?;
    let phase = match row
        .try_get::<String, _>("phase")
        .map_err(|_| RegistryError::CorruptState)?
        .as_str()
    {
        "registered" => NativeReleaseStoredPhase::Registered,
        "releasing" => NativeReleaseStoredPhase::Releasing,
        "released" => NativeReleaseStoredPhase::Released,
        "failed" => NativeReleaseStoredPhase::Failed,
        _ => return Err(RegistryError::CorruptState),
    };
    let error_code = row
        .try_get::<Option<String>, _>("error_code")
        .map_err(|_| RegistryError::CorruptState)?;
    if (phase == NativeReleaseStoredPhase::Failed) != error_code.is_some() {
        return Err(RegistryError::CorruptState);
    }
    Ok(NativeReleaseRecord {
        operation,
        request,
        target,
        phase,
        error_code,
    })
}
