//! Additive schema8: necessary approvals and independently retained supplemental audit.
pub(super) const DEFINITIONS: &[&str] = &[
    r"CREATE TABLE node_approval_requests (
  approval_id TEXT PRIMARY KEY CHECK(length(approval_id)=22),
  obligation_id TEXT NOT NULL UNIQUE CHECK(length(obligation_id)=22),
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  instance_id TEXT NOT NULL CHECK(length(instance_id)=22),
  connection_epoch TEXT NOT NULL CHECK(length(connection_epoch)=22),
  scope_binding_id TEXT NOT NULL CHECK(length(scope_binding_id)=22),
  rpc_kind TEXT NOT NULL CHECK(rpc_kind IN ('integer','string')),
  rpc_digest BLOB NOT NULL CHECK(length(rpc_digest)=32),
  native_session_digest BLOB NOT NULL CHECK(length(native_session_digest)=32),
  request_digest BLOB NOT NULL CHECK(length(request_digest)=32),
  protocol_version INTEGER NOT NULL CHECK(protocol_version IN (1,2)),
  observed_sequence BLOB NOT NULL CHECK(length(observed_sequence)=8),
  observed_receipt BLOB NOT NULL CHECK(length(observed_receipt)=8),
  observed_at_ms INTEGER NOT NULL,
  dispatch_id TEXT REFERENCES node_native_dispatches(dispatch_id),
  context_kind TEXT NOT NULL CHECK(context_kind IN ('session_request','verified_input')),
  FOREIGN KEY(instance_id,connection_epoch)
    REFERENCES node_native_source_instances(instance_id,connection_epoch),
  FOREIGN KEY(instance_id,scope_binding_id)
    REFERENCES node_native_source_instances(instance_id,scope_binding_id),
  UNIQUE(instance_id,scope_binding_id,rpc_kind,rpc_digest),
  CHECK((context_kind='session_request' AND dispatch_id IS NULL) OR
        (context_kind='verified_input' AND dispatch_id IS NOT NULL))
) STRICT",
    r"CREATE TABLE node_approval_associations (
  approval_id TEXT PRIMARY KEY REFERENCES node_approval_requests(approval_id),
  target_id TEXT NOT NULL REFERENCES product_targets(target_id),
  association_kind TEXT NOT NULL CHECK(association_kind IN ('known_pre','registered_session')),
  load_epoch TEXT,
  owner_id TEXT CHECK(owner_id IS NULL OR length(owner_id)=22),
  association_fingerprint BLOB NOT NULL CHECK(length(association_fingerprint)=32),
  associated_at_ms INTEGER NOT NULL,
  UNIQUE(approval_id,target_id),
  FOREIGN KEY(load_epoch,target_id)
    REFERENCES node_native_resource_bindings(load_epoch,target_id),
  CHECK((association_kind='known_pre' AND load_epoch IS NOT NULL AND owner_id IS NOT NULL) OR
        (association_kind='registered_session' AND load_epoch IS NULL AND owner_id IS NULL))
) STRICT",
    r"CREATE TABLE node_approval_commands (
  registration_id TEXT PRIMARY KEY CHECK(length(registration_id)=22),
  approval_id TEXT NOT NULL,
  target_id TEXT NOT NULL,
  actor_id TEXT NOT NULL REFERENCES clients(client_id),
  registration_auth_generation INTEGER NOT NULL CHECK(registration_auth_generation>=0),
  option_digest BLOB NOT NULL CHECK(length(option_digest)=32),
  intent_fingerprint BLOB NOT NULL CHECK(length(intent_fingerprint)=32),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  registered_at_ms INTEGER NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('registered','accepted','rejected')),
  operation_id TEXT UNIQUE CHECK(operation_id IS NULL OR length(operation_id)=22),
  admission_auth_generation INTEGER CHECK(admission_auth_generation>=0),
  admitted_at_ms INTEGER,
  rejection_reason TEXT CHECK(rejection_reason IN
    ('already_decided','request_invalid','option_invalid','source_invalid','actor_invalid')),
  FOREIGN KEY(approval_id,target_id)
    REFERENCES node_approval_associations(approval_id,target_id),
  UNIQUE(approval_id,registration_id),
  CHECK((state='registered' AND operation_id IS NULL AND admission_auth_generation IS NULL
          AND admitted_at_ms IS NULL AND rejection_reason IS NULL) OR
        (state='accepted' AND operation_id IS NOT NULL AND admission_auth_generation IS NOT NULL
          AND admitted_at_ms IS NOT NULL AND rejection_reason IS NULL) OR
        (state='rejected' AND operation_id IS NOT NULL AND admitted_at_ms IS NOT NULL
          AND rejection_reason IS NOT NULL))
) STRICT",
    r"CREATE TABLE node_approval_decisions (
  approval_id TEXT PRIMARY KEY REFERENCES node_approval_requests(approval_id),
  registration_id TEXT NOT NULL UNIQUE,
  operation_id TEXT NOT NULL UNIQUE CHECK(length(operation_id)=22),
  actor_id TEXT NOT NULL REFERENCES clients(client_id),
  admission_auth_generation INTEGER NOT NULL CHECK(admission_auth_generation>=0),
  option_digest BLOB NOT NULL CHECK(length(option_digest)=32),
  known_option_kind TEXT CHECK(known_option_kind IN
    ('allow_once','allow_always','reject_once','reject_always')),
  authorization_fingerprint BLOB NOT NULL CHECK(length(authorization_fingerprint)=32),
  decision_fingerprint BLOB NOT NULL CHECK(length(decision_fingerprint)=32),
  accepted_at_ms INTEGER NOT NULL,
  FOREIGN KEY(approval_id,registration_id)
    REFERENCES node_approval_commands(approval_id,registration_id),
  UNIQUE(approval_id,operation_id)
) STRICT",
    r"CREATE TABLE node_approval_reply_attempts (
  response_id TEXT PRIMARY KEY CHECK(length(response_id)=22),
  approval_id TEXT NOT NULL REFERENCES node_approval_requests(approval_id),
  decision_operation_id TEXT,
  attempt_serial BLOB NOT NULL CHECK(length(attempt_serial)=8),
  response_kind TEXT NOT NULL CHECK(response_kind IN ('selected','cancelled')),
  origin TEXT NOT NULL CHECK(origin IN
    ('user_decision','protocol_cancel','management_failure','source_closing','target_closing')),
  option_digest BLOB CHECK(option_digest IS NULL OR length(option_digest)=32),
  frame_length BLOB NOT NULL CHECK(length(frame_length)=8 AND frame_length>X'0000000000000000'),
  reserved_fingerprint BLOB NOT NULL CHECK(length(reserved_fingerprint)=32),
  observed_at_ms INTEGER NOT NULL,
  recorded_at_ms INTEGER NOT NULL,
  FOREIGN KEY(approval_id,decision_operation_id)
    REFERENCES node_approval_decisions(approval_id,operation_id),
  UNIQUE(approval_id,attempt_serial),
  UNIQUE(response_id,approval_id),
  CHECK((response_kind='selected' AND origin='user_decision'
          AND decision_operation_id IS NOT NULL AND option_digest IS NOT NULL) OR
        (response_kind='cancelled' AND origin!='user_decision' AND option_digest IS NULL))
) STRICT",
    r"CREATE TABLE node_approval_reply_facts (
  response_id TEXT PRIMARY KEY,
  approval_id TEXT NOT NULL,
  transport_state TEXT NOT NULL CHECK(transport_state IN ('zero','written','unknown')),
  accepted_length BLOB NOT NULL CHECK(length(accepted_length)=8),
  transport_reason TEXT CHECK(transport_reason IN
    ('cancelled','deadline','fence_rejected','connection_closed','abandoned','io',
     'invalid_fence','connection_lost','restart')),
  terminal_fingerprint BLOB NOT NULL CHECK(length(terminal_fingerprint)=32),
  observed_at_ms INTEGER NOT NULL,
  recorded_at_ms INTEGER NOT NULL,
  FOREIGN KEY(response_id,approval_id)
    REFERENCES node_approval_reply_attempts(response_id,approval_id),
  CHECK((transport_state='zero' AND accepted_length=X'0000000000000000'
          AND transport_reason IN ('cancelled','deadline','fence_rejected','connection_closed','abandoned')) OR
        (transport_state='written' AND transport_reason IS NULL) OR
        (transport_state='unknown' AND transport_reason IS NOT NULL))
) STRICT",
    r"CREATE TABLE node_approval_invalidations (
  invalidation_id TEXT PRIMARY KEY CHECK(length(invalidation_id)=22),
  approval_id TEXT NOT NULL REFERENCES node_approval_requests(approval_id),
  reason TEXT NOT NULL CHECK(reason IN
    ('request_cancelled','response_spent','connection_lost','source_closed',
     'target_closed','verified_native_request_ended','node_restart')),
  evidence_fingerprint BLOB NOT NULL CHECK(length(evidence_fingerprint)=32),
  observed_sequence BLOB CHECK(observed_sequence IS NULL OR length(observed_sequence)=8),
  observed_at_ms INTEGER NOT NULL,
  recorded_at_ms INTEGER NOT NULL,
  UNIQUE(approval_id,reason,evidence_fingerprint)
) STRICT",
    r"CREATE TABLE node_approval_audit (
  record_id TEXT PRIMARY KEY,
  approval_id TEXT NOT NULL REFERENCES node_approval_requests(approval_id),
  basis_fingerprint BLOB NOT NULL CHECK(length(basis_fingerprint)=32),
  event_kind TEXT NOT NULL CHECK(event_kind IN
    ('approval.registered','approval.decided','approval.delivery','approval.invalidated')),
  actor_id TEXT REFERENCES clients(client_id),
  target_id TEXT REFERENCES product_targets(target_id),
  outcome TEXT NOT NULL CHECK(outcome IN
    ('registered','accepted','rejected','zero','written','unknown','suppressed','invalidated')),
  occurred_at_ms INTEGER NOT NULL,
  recorded_at_ms INTEGER NOT NULL
    CHECK(recorded_at_ms BETWEEN 0 AND 9223372034262775807),
  retain_until_ms INTEGER NOT NULL CHECK(retain_until_ms>=recorded_at_ms AND
    retain_until_ms-recorded_at_ms>=2592000000),
  retention_hold INTEGER NOT NULL DEFAULT 0 CHECK(retention_hold IN (0,1)),
  UNIQUE(approval_id,event_kind,basis_fingerprint)
) STRICT",
    r"CREATE INDEX node_approval_by_source
  ON node_approval_requests(instance_id,scope_binding_id,approval_id)",
    r"CREATE INDEX node_approval_commands_by_actor
  ON node_approval_commands(actor_id,registration_id)",
    r"CREATE INDEX node_approval_replies_by_request
  ON node_approval_reply_attempts(approval_id,attempt_serial)",
    r"CREATE INDEX node_approval_audit_expiry
  ON node_approval_audit(retention_hold,retain_until_ms,record_id)",
];
pub(super) const NECESSARY: &[&str] = &[
    DEFINITIONS[0],
    DEFINITIONS[1],
    DEFINITIONS[2],
    DEFINITIONS[3],
    DEFINITIONS[4],
    DEFINITIONS[5],
    DEFINITIONS[6],
    DEFINITIONS[8],
    DEFINITIONS[9],
    DEFINITIONS[10],
];
pub(super) fn main_ddl(definition: &str) -> String {
    if definition.starts_with("CREATE TABLE ") {
        definition.replacen("CREATE TABLE ", "CREATE TABLE main.", 1)
    } else {
        definition.replacen("CREATE INDEX ", "CREATE INDEX main.", 1)
    }
}

#[cfg(test)]
mod tests;
