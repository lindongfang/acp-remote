//! Additive schema10 metadata for durable, actor-bound native release operations.
pub(super) const DEFINITIONS: &[&str] = &[
    r"CREATE TABLE node_native_release_operations (
  operation_id TEXT PRIMARY KEY CHECK(length(operation_id)=22),
  request_id TEXT NOT NULL CHECK(length(request_id)=22),
  actor_id TEXT NOT NULL REFERENCES clients(client_id),
  actor_auth_generation INTEGER NOT NULL CHECK(actor_auth_generation>=0),
  target_id TEXT NOT NULL REFERENCES product_targets(target_id),
  phase TEXT NOT NULL CHECK(phase IN ('registered','releasing','released','failed')),
  error_code TEXT CHECK(error_code IN
    ('node_stopping','necessary_storage_unavailable','invalid_target',
     'request_conflict','release_unconfirmed','protocol_failure','release_failed')),
  registered_at_ms INTEGER NOT NULL CHECK(registered_at_ms>=0),
  updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=registered_at_ms),
  CHECK((phase='failed' AND error_code IS NOT NULL) OR
        (phase!='failed' AND error_code IS NULL)),
  UNIQUE(actor_id,actor_auth_generation,request_id)
) STRICT",
    r"CREATE INDEX node_native_release_actor_lookup
  ON node_native_release_operations(actor_id,target_id,updated_at_ms)",
];

#[cfg(test)]
mod tests;
