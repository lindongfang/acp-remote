//! Trusted same-crate test ports. Real authentication remains outside this harness.
use crate::{
    application::request_registry::{
        NodeClock, QueueAction, RegistryError, RequestAuthority, ValidatedBodyPort,
    },
    domain::{ActorContext, EntityId, native::NativeSessionKey},
};
use sha2::{Digest, Sha256};
pub struct Authority;
#[allow(
    clippy::unused_async_trait_impl,
    reason = "Synchronous trusted fixture follows the async port without artificial yields."
)]
impl RequestAuthority for Authority {
    type Actor = ActorContext;
    async fn authorize(
        &self,
        actor: &ActorContext,
        _: EntityId,
        _: QueueAction,
    ) -> Result<ActorContext, RegistryError> {
        Ok(*actor)
    }
    async fn authorize_read(
        &self,
        actor: &ActorContext,
        _: EntityId,
    ) -> Result<ActorContext, RegistryError> {
        Ok(*actor)
    }
    async fn resolve_target(&self, target: EntityId) -> Result<NativeSessionKey, RegistryError> {
        if target != EntityId::from_bytes([81; 16]) {
            return Err(RegistryError::InvalidTarget);
        }
        NativeSessionKey::from_config(
            Some(EntityId::from_bytes([80; 16])),
            "registry-fault-synthetic",
        )
        .map_err(|_| RegistryError::InvalidTarget)
    }
}
pub struct Validator;
impl ValidatedBodyPort for Validator {
    type Raw = [u8];
    type Body = Vec<u8>;
    fn validate(&self, raw: &[u8]) -> Result<(Vec<u8>, [u8; 32]), RegistryError> {
        Ok((raw.to_vec(), Sha256::digest(raw).into()))
    }
}
pub struct Clock;
impl NodeClock for Clock {
    fn unix_millis(&self) -> i64 {
        -1
    }
}

/// Controlled resource observation only; this fixture has no protocol writer.
pub struct Environment;
pub const fn resource() -> crate::domain::native::ResourceEpoch {
    use crate::domain::native::{HarnessConnectionEpoch, LoadEpoch, ResourceEpoch, RuntimeEpoch};
    ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([90; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([91; 16])),
        load: LoadEpoch(EntityId::from_bytes([92; 16])),
    }
}
impl crate::application::queue_owner::QueueEnvironmentPort for Environment {
    fn readiness(&self, _: &NativeSessionKey) -> crate::domain::input_queue::DispatchReadiness {
        crate::domain::input_queue::DispatchReadiness::Blocked
    }
    fn resource_epoch(&self, _: &NativeSessionKey) -> Option<crate::domain::native::ResourceEpoch> {
        Some(resource())
    }
    fn poll_event(
        &self,
        _: &NativeSessionKey,
    ) -> Option<crate::application::queue_owner::QueueOwnerEvent> {
        None
    }
}
