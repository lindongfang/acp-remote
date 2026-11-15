//! A trusted test authority which actually reads this Storage's max1 pool.
use crate::{
    application::request_registry::{QueueAction, RegistryError, RequestAuthority},
    domain::{ActorContext, EntityId, native::NativeSessionKey},
};
#[derive(Clone)]
pub(super) struct RealSqlAuthority {
    pub(super) pool: sqlx::SqlitePool,
}
impl RealSqlAuthority {
    async fn current(&self, actor: &ActorContext) -> Result<ActorContext, RegistryError> {
        let row: Option<(String, i64)> =
            sqlx::query_as("SELECT status,auth_generation FROM clients WHERE client_id=?")
                .bind(actor.client_id.encode())
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| RegistryError::StorageUnavailable)?;
        match row {
            Some((status, generation))
                if status == "trusted" && generation == actor.auth_generation =>
            {
                Ok(*actor)
            }
            _ => Err(RegistryError::InvalidActor),
        }
    }
}
impl RequestAuthority for RealSqlAuthority {
    type Actor = ActorContext;
    async fn authorize(
        &self,
        actor: &ActorContext,
        target: EntityId,
        _: QueueAction,
    ) -> Result<ActorContext, RegistryError> {
        self.resolve_target(target).await?;
        self.current(actor).await
    }
    async fn authorize_read(
        &self,
        actor: &ActorContext,
        target: EntityId,
    ) -> Result<ActorContext, RegistryError> {
        self.resolve_target(target).await?;
        self.current(actor).await
    }
    async fn resolve_target(&self, target: EntityId) -> Result<NativeSessionKey, RegistryError> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT source_id,native_id FROM product_targets WHERE target_id=?")
                .bind(target.encode())
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| RegistryError::StorageUnavailable)?;
        let (source, native) = row.ok_or(RegistryError::InvalidTarget)?;
        NativeSessionKey::from_config(
            Some(EntityId::decode(&source).map_err(|_| RegistryError::InvalidTarget)?),
            &native,
        )
        .map_err(|_| RegistryError::InvalidTarget)
    }
}
