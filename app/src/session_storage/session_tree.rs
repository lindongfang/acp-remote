use super::native_dispatch::SqliteNativeDispatch;
use crate::{
    application::{
        native_dispatch::{BoxFuture, SessionParentProof, SessionTreePath, SessionTreeRepository},
        request_registry::RegistryDeadline,
    },
    domain::{native::NativeSessionKey, native_dispatch::DispatchError},
};
use sqlx::SqliteConnection;
use std::collections::HashSet;

impl SessionTreeRepository for SqliteNativeDispatch {
    fn session_tree_members<'a>(
        &'a self,
        root: &'a NativeSessionKey,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Vec<SessionTreePath>, DispatchError>> {
        Box::pin(async move {
            let mut owned = self.begin(deadline).await?;
            let result = {
                let connection = owned.connection()?;
                tokio::select! {
                    biased;
                    () = deadline.cancelled() => Err(DispatchError::StorageUnavailable),
                    result = tokio::time::timeout_at(deadline.deadline(), members(connection, root)) =>
                        result.unwrap_or(Err(DispatchError::StorageUnavailable)),
                }
            };
            self.finish(owned, result, deadline).await
        })
    }

    fn record_session_parent<'a>(
        &'a self,
        proof: &'a SessionParentProof,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SessionTreePath, DispatchError>> {
        Box::pin(async move {
            let mut owned = self.begin(deadline).await?;
            let result = {
                let connection = owned.connection()?;
                tokio::select! {
                    biased;
                    () = deadline.cancelled() => Err(DispatchError::StorageUnavailable),
                    result = tokio::time::timeout_at(deadline.deadline(), record(connection, proof)) =>
                        result.unwrap_or(Err(DispatchError::StorageUnavailable)),
                }
            };
            self.finish(owned, result, deadline).await
        })
    }

    fn resolve_session_tree<'a>(
        &'a self,
        native: &'a NativeSessionKey,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SessionTreePath, DispatchError>> {
        Box::pin(async move {
            let mut owned = self.begin(deadline).await?;
            let result = {
                let connection = owned.connection()?;
                tokio::select! {
                    biased;
                    () = deadline.cancelled() => Err(DispatchError::StorageUnavailable),
                    result = tokio::time::timeout_at(deadline.deadline(), path(connection, native)) =>
                        result.unwrap_or(Err(DispatchError::StorageUnavailable)).and_then(|path| {
                            if path.root.is_some() {
                                Ok(path)
                            } else {
                                Err(DispatchError::SessionTreeUnverified)
                            }
                        }),
                }
            };
            self.finish(owned, result, deadline).await
        })
    }
}

async fn members(
    connection: &mut SqliteConnection,
    root: &NativeSessionKey,
) -> Result<Vec<SessionTreePath>, DispatchError> {
    let root_path = path(connection, root).await?;
    if root_path.root.as_ref() != Some(root) || !root_path.ancestors.is_empty() {
        return Err(DispatchError::SessionTreeUnverified);
    }
    let natives: Vec<String> = sqlx::query_scalar("WITH RECURSIVE members(native_id) AS (SELECT ? UNION SELECT p.child_native_id FROM main.node_session_parents p JOIN members m ON p.parent_native_id=m.native_id WHERE p.source_id=?) SELECT native_id FROM members ORDER BY native_id COLLATE BINARY")
        .bind(root.native_id()).bind(root.source().0.encode()).fetch_all(&mut *connection).await?;
    let mut result = Vec::with_capacity(natives.len());
    for native in natives {
        let native = NativeSessionKey::from_config(Some(root.source().0), &native)
            .map_err(|_| DispatchError::CorruptState)?;
        let resolved = path(connection, &native).await?;
        if resolved.root.as_ref() != Some(root) {
            return Err(DispatchError::CorruptState);
        }
        result.push(resolved);
    }
    Ok(result)
}

async fn record(
    connection: &mut SqliteConnection,
    proof: &SessionParentProof,
) -> Result<SessionTreePath, DispatchError> {
    let source = proof.source().metadata();
    if proof.child().source().0 != source.source_id
        || proof.parent().source().0 != source.source_id
        || proof.child() == proof.parent()
        || proof.sequence() == 0
        || proof.receipt() == 0
    {
        return Err(DispatchError::Conflict);
    }
    let lifecycle: Option<String> = sqlx::query_scalar("SELECT lifecycle FROM main.node_native_source_instances WHERE instance_id=? AND source_id=? AND runtime_epoch=? AND connection_epoch=? AND config_identity=? AND config_revision=?")
        .bind(source.instance_id.encode()).bind(source.source_id.encode())
        .bind(source.runtime.0.encode()).bind(source.connection_epoch.encode())
        .bind(source.config_identity.encode()).bind(source.config_revision.to_be_bytes().to_vec())
        .fetch_optional(&mut *connection).await?;
    match lifecycle.as_deref() {
        None => return Err(DispatchError::Conflict),
        Some("closed") => {
            let original: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM main.node_session_parents WHERE source_id=? AND child_native_id=? AND parent_native_id=? AND observed_instance=? AND runtime_epoch=? AND connection_epoch=? AND sequence=? AND receipt=? AND fingerprint=? AND observed_at_ms=?)")
                .bind(source.source_id.encode()).bind(proof.child().native_id())
                .bind(proof.parent().native_id()).bind(source.instance_id.encode())
                .bind(source.runtime.0.encode()).bind(source.connection_epoch.encode())
                .bind(proof.sequence().to_be_bytes().to_vec()).bind(proof.receipt().to_be_bytes().to_vec())
                .bind(proof.fingerprint().as_slice()).bind(proof.observed_at_ms())
                .fetch_one(&mut *connection).await?;
            if original != 1 {
                return Err(DispatchError::Conflict);
            }
            return path(connection, proof.child()).await;
        }
        Some(_) => {}
    }
    match super::session_capacity::require_root(connection, proof.child()).await {
        Ok(()) => return Err(DispatchError::Conflict),
        Err(DispatchError::SessionTreeUnverified) => {}
        Err(error) => return Err(error),
    }
    if let Some(existing) = parent(connection, proof.child()).await? {
        if existing != *proof.parent() {
            return Err(DispatchError::Conflict);
        }
        return path(connection, proof.child()).await;
    }
    let ancestors = path(connection, proof.parent()).await?;
    if ancestors.subject == *proof.child()
        || ancestors
            .ancestors
            .iter()
            .any(|value| value == proof.child())
    {
        return Err(DispatchError::Conflict);
    }
    sqlx::query("INSERT INTO main.node_session_parents(source_id,child_native_id,parent_native_id,observed_instance,runtime_epoch,connection_epoch,sequence,receipt,fingerprint,observed_at_ms) VALUES(?,?,?,?,?,?,?,?,?,?)")
        .bind(source.source_id.encode()).bind(proof.child().native_id())
        .bind(proof.parent().native_id()).bind(source.instance_id.encode())
        .bind(source.runtime.0.encode()).bind(source.connection_epoch.encode())
        .bind(proof.sequence().to_be_bytes().to_vec()).bind(proof.receipt().to_be_bytes().to_vec())
        .bind(proof.fingerprint().as_slice()).bind(proof.observed_at_ms())
        .execute(&mut *connection).await?;
    path(connection, proof.child()).await
}

async fn parent(
    connection: &mut SqliteConnection,
    native: &NativeSessionKey,
) -> Result<Option<NativeSessionKey>, DispatchError> {
    let value: Option<String> = sqlx::query_scalar("SELECT parent_native_id FROM main.node_session_parents WHERE source_id=? AND child_native_id=?")
        .bind(native.source().0.encode()).bind(native.native_id())
        .fetch_optional(connection).await?;
    value
        .as_deref()
        .map(|value| {
            NativeSessionKey::from_config(Some(native.source().0), value)
                .map_err(|_| DispatchError::CorruptState)
        })
        .transpose()
}

async fn path(
    connection: &mut SqliteConnection,
    native: &NativeSessionKey,
) -> Result<SessionTreePath, DispatchError> {
    let mut current = native.clone();
    let mut ancestors = Vec::new();
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return Err(DispatchError::CorruptState);
        }
        let next = parent(connection, &current).await?;
        let root = match super::session_capacity::require_root(connection, &current).await {
            Ok(()) => true,
            Err(DispatchError::SessionTreeUnverified) => false,
            Err(error) => return Err(error),
        };
        if root && next.is_some() {
            return Err(DispatchError::CorruptState);
        }
        let Some(next) = next else {
            return Ok(SessionTreePath {
                subject: native.clone(),
                ancestors,
                root: root.then_some(current),
            });
        };
        ancestors.push(next.clone());
        current = next;
    }
}

#[cfg(test)]
mod development_tests;
