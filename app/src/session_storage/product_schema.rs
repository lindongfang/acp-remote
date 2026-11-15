//! Additive management-only schema. Legacy conversation tables are retained, never copied here.
pub(super) const DEFINITIONS: &[&str] = &[
    r"CREATE TABLE product_control (
        singleton INTEGER PRIMARY KEY CHECK(singleton=1),
        request_epoch TEXT NOT NULL CHECK(length(request_epoch)=22),
        time_floor_ms INTEGER NOT NULL CHECK(time_floor_ms BETWEEN 0 AND 9007199254740991)) STRICT",
    r"CREATE TABLE product_sources (
        source_id TEXT PRIMARY KEY CHECK(length(source_id)=22),
        name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 80)) STRICT",
    r"CREATE TABLE product_harness_sources (
        harness_id TEXT PRIMARY KEY REFERENCES harnesses(harness_id),
        source_id TEXT NOT NULL REFERENCES product_sources(source_id)) STRICT",
    r"CREATE TABLE product_targets (
        target_id TEXT PRIMARY KEY CHECK(length(target_id)=22),
        source_id TEXT NOT NULL REFERENCES product_sources(source_id),
        native_id TEXT NOT NULL CHECK(length(CAST(native_id AS BLOB)) BETWEEN 1 AND 4096),
        workspace_id TEXT REFERENCES workspaces(workspace_id),
        UNIQUE(source_id,native_id)) STRICT",
    r"CREATE TABLE product_commands (
        command_id TEXT PRIMARY KEY CHECK(length(command_id)=22),
        actor_id TEXT NOT NULL REFERENCES clients(client_id),
        auth_generation INTEGER NOT NULL CHECK(auth_generation BETWEEN 0 AND 9007199254740991),
        request_id TEXT NOT NULL CHECK(length(request_id)=22),
        request_epoch TEXT NOT NULL CHECK(length(request_epoch)=22),
        action TEXT NOT NULL CHECK(action IN ('harness.register','workspace.register','session.create','session.load','session.input','session.cancel','session.release','session.resume','approval.decide')),
        target_kind TEXT NOT NULL CHECK(target_kind IN ('harness','workspace','source','session','approval')),
        target_id TEXT NOT NULL CHECK(length(target_id)=22),
        payload_digest TEXT NOT NULL CHECK(length(payload_digest)=64 AND payload_digest NOT GLOB '*[^0-9a-f]*'),
        issued_at INTEGER NOT NULL CHECK(issued_at BETWEEN 0 AND 9007199254740991),
        expires_at INTEGER NOT NULL CHECK(expires_at BETWEEN 0 AND 9007199254740991 AND expires_at>issued_at AND expires_at-issued_at<=86400000),
        status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','completed','failed','unknown','expired')),
        claim_id TEXT UNIQUE CHECK(claim_id IS NULL OR length(claim_id)=22),
        CHECK(status!='dispatching' OR claim_id IS NOT NULL),
        UNIQUE(actor_id,request_id)) STRICT",
    r"CREATE TABLE product_runs (
        run_id TEXT PRIMARY KEY CHECK(length(run_id)=22),
        command_id TEXT NOT NULL UNIQUE REFERENCES product_commands(command_id),
        target_id TEXT NOT NULL REFERENCES product_targets(target_id),
        status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','running','cancelling','completed','cancelled','failed','interrupted','unknown')),
        UNIQUE(target_id,run_id)) STRICT",
    r"CREATE TABLE product_resources (
        load_epoch TEXT PRIMARY KEY CHECK(length(load_epoch)=22),
        target_id TEXT NOT NULL REFERENCES product_targets(target_id),
        runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
        connection_epoch TEXT NOT NULL CHECK(length(connection_epoch)=22),
        instance_id TEXT NOT NULL CHECK(length(instance_id)=22),
        state TEXT NOT NULL CHECK(state IN ('reserved','loaded','releasing','released','unknown')),
        revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 9007199254740991),
        UNIQUE(target_id,load_epoch)) STRICT",
    r"CREATE TABLE product_approvals (
        approval_id TEXT PRIMARY KEY CHECK(length(approval_id)=22),
        target_id TEXT NOT NULL REFERENCES product_targets(target_id),
        load_epoch TEXT NOT NULL,
        native_request_digest TEXT NOT NULL CHECK(length(native_request_digest)=64 AND native_request_digest NOT GLOB '*[^0-9a-f]*'),
        request_digest TEXT NOT NULL CHECK(length(request_digest)=64 AND request_digest NOT GLOB '*[^0-9a-f]*'),
        run_id TEXT,
        protocol_version INTEGER NOT NULL CHECK(protocol_version IN (1,2)),
        state TEXT NOT NULL CHECK(state IN ('pending','decided','invalid','unsupported')),
        decision_command_id TEXT UNIQUE REFERENCES product_commands(command_id),
        deciding_actor_id TEXT REFERENCES clients(client_id),
        choice_digest TEXT CHECK(choice_digest IS NULL OR (length(choice_digest)=64 AND choice_digest NOT GLOB '*[^0-9a-f]*')),
        decision_kind TEXT CHECK(decision_kind IN ('allow_once','allow_always','reject_once','reject_always')),
        CHECK(state!='decided' OR (decision_command_id IS NOT NULL AND deciding_actor_id IS NOT NULL AND choice_digest IS NOT NULL AND decision_kind IS NOT NULL)),
        UNIQUE(load_epoch,native_request_digest),
        FOREIGN KEY(target_id,load_epoch) REFERENCES product_resources(target_id,load_epoch),
        FOREIGN KEY(target_id,run_id) REFERENCES product_runs(target_id,run_id)) STRICT",
    r"CREATE TABLE product_deliveries (
        approval_id TEXT PRIMARY KEY REFERENCES product_approvals(approval_id),
        state TEXT NOT NULL CHECK(state IN ('not_sent','claimed','transport_written','unknown','suppressed')),
        claim_id TEXT UNIQUE CHECK(claim_id IS NULL OR length(claim_id)=22),
        CHECK(state NOT IN ('claimed','transport_written','unknown') OR claim_id IS NOT NULL)) STRICT",
    r"CREATE TABLE product_audit_slots (
        command_id TEXT NOT NULL REFERENCES product_commands(command_id),
        stage TEXT NOT NULL CHECK(stage IN ('admission','dispatch','result','reconciliation')),
        actor_id TEXT NOT NULL REFERENCES clients(client_id),
        reserved_at INTEGER NOT NULL CHECK(reserved_at BETWEEN 0 AND 9007199254740991),
        occurred_at INTEGER CHECK(occurred_at BETWEEN 0 AND 9007199254740991),
        retain_until INTEGER CHECK(retain_until BETWEEN 0 AND 9007199254740991),
        outcome TEXT CHECK(outcome IN ('accepted','dispatching','completed','failed','unknown','expired','reconciled','denied','cancelled')),
        CHECK((occurred_at IS NULL AND retain_until IS NULL AND outcome IS NULL) OR
              (occurred_at IS NOT NULL AND retain_until IS NOT NULL AND outcome IS NOT NULL AND retain_until>=occurred_at AND retain_until-occurred_at>=7776000000)),
        PRIMARY KEY(command_id,stage)) STRICT",
    r"CREATE UNIQUE INDEX product_one_foreground_run ON product_runs(target_id) WHERE status IN ('accepted','dispatching','running','cancelling','unknown')",
    r"CREATE UNIQUE INDEX product_one_resource_owner ON product_resources(target_id) WHERE state!='released'",
];

/// Schema4 adds response-attempt metadata without changing any schema3 definition.
pub(super) const DELIVERY_DEFINITIONS: &[&str] = &[
    r"CREATE UNIQUE INDEX product_delivery_claim_identity
      ON product_deliveries(approval_id,claim_id)",
    r"CREATE TABLE product_delivery_attempts (
      approval_id TEXT PRIMARY KEY REFERENCES product_approvals(approval_id),
      claim_id TEXT NOT NULL UNIQUE CHECK(length(claim_id)=22),
      planned_response_kind TEXT NOT NULL CHECK(planned_response_kind IN ('selected','cancelled')),
      planned_option_digest TEXT CHECK(planned_option_digest IS NULL OR
        (length(planned_option_digest)=64 AND planned_option_digest NOT GLOB '*[^0-9a-f]*')),
      actual_response_kind TEXT CHECK(actual_response_kind IN ('selected','cancelled')),
      actual_option_digest TEXT CHECK(actual_option_digest IS NULL OR
        (length(actual_option_digest)=64 AND actual_option_digest NOT GLOB '*[^0-9a-f]*')),
      reason TEXT NOT NULL CHECK(reason IN
        ('user_choice','request_cancelled','source_invalid','management_failure','node_stop')),
      transport_state TEXT NOT NULL CHECK(transport_state IN ('claimed','transport_written','unknown')),
      observed_at INTEGER NOT NULL CHECK(observed_at BETWEEN 0 AND 9007199254740991),
      CHECK((planned_response_kind='selected' AND planned_option_digest IS NOT NULL) OR
            (planned_response_kind='cancelled' AND planned_option_digest IS NULL)),
      CHECK((actual_response_kind IS NULL AND actual_option_digest IS NULL) OR
            (actual_response_kind IS NOT NULL AND
              ((actual_response_kind='selected' AND actual_option_digest IS NOT NULL) OR
               (actual_response_kind='cancelled' AND actual_option_digest IS NULL)))),
      CHECK(transport_state!='claimed' OR actual_response_kind IS NULL),
      CHECK(transport_state!='transport_written' OR actual_response_kind IS NOT NULL),
      FOREIGN KEY(approval_id,claim_id) REFERENCES product_deliveries(approval_id,claim_id)
    ) STRICT",
];

#[cfg(test)]
mod delivery_tests;
#[cfg(test)]
mod tests;
