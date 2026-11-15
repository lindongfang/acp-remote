//! Additive schema7; historical definitions and rows are never rewritten.
macro_rules! audit_body {
    () => {
        r"(
    record_id TEXT PRIMARY KEY,
    event_version INTEGER NOT NULL CHECK(event_version=1),
    event_kind TEXT NOT NULL CHECK(event_kind='queue.settled'),
    request_id TEXT NOT NULL REFERENCES node_request_registrations(request_id),
    operation_id TEXT NOT NULL REFERENCES node_request_operations(operation_id),
    actor_id TEXT NOT NULL REFERENCES clients(client_id),
    action TEXT NOT NULL CHECK(action IN
        ('input.submit','input.edit','input.cancel','input.prioritize','input.resume')),
    target_id TEXT NOT NULL REFERENCES product_targets(target_id),
    basis_kind TEXT NOT NULL CHECK(basis_kind='registered_operation'),
    register_auth_generation INTEGER NOT NULL CHECK(register_auth_generation>=0),
    admission_auth_generation INTEGER NOT NULL CHECK(admission_auth_generation>=0),
    runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
    outcome TEXT NOT NULL CHECK(outcome IN
        ('queued','direct_selected','edited','cancelled','prioritized','resumed','rejected')),
    result_item_id TEXT CHECK(result_item_id IS NULL OR length(result_item_id)=22),
    result_revision BLOB CHECK(result_revision IS NULL OR length(result_revision)=8),
    restored_count INTEGER CHECK(restored_count BETWEEN 0 AND 3),
    rejection_code TEXT CHECK(rejection_code IN
        ('full','duplicate_id','not_found','revision_changed','revision_exhausted',
         'dispatch_started','suspended','busy','sequence_exhausted')),
    event_at_ms INTEGER NOT NULL,
    observed_at_ms INTEGER NOT NULL,
    recorded_at_ms INTEGER NOT NULL
        CHECK(recorded_at_ms BETWEEN 0 AND 9223372034262775807),
    retain_until_ms INTEGER NOT NULL
        CHECK(retain_until_ms>=recorded_at_ms AND
              retain_until_ms-recorded_at_ms>=2592000000),
    retention_hold INTEGER NOT NULL DEFAULT 0 CHECK(retention_hold IN (0,1)),
    CHECK(length(request_id)=22 AND length(operation_id)=22 AND
          length(actor_id)=22 AND length(target_id)=22),
    CHECK(length(record_id)=36 AND record_id='queue.settled:'||request_id),
    CHECK(
        (outcome IN ('queued','direct_selected','edited','cancelled','prioritized')
            AND result_item_id IS NOT NULL AND result_revision IS NOT NULL
            AND restored_count IS NULL AND rejection_code IS NULL) OR
        (outcome='resumed' AND result_item_id IS NULL AND result_revision IS NULL
            AND restored_count IS NOT NULL AND rejection_code IS NULL) OR
        (outcome='rejected' AND result_item_id IS NULL AND result_revision IS NULL
            AND restored_count IS NULL AND rejection_code IS NOT NULL)),
    UNIQUE(request_id,event_kind)
) STRICT"
    };
}
pub(super) const TABLE: &str = concat!("CREATE TABLE node_management_audit ", audit_body!());
pub(super) const TABLE_DDL: &str =
    concat!("CREATE TABLE main.node_management_audit ", audit_body!());
pub(super) const INDEX: &str = "CREATE INDEX node_management_audit_expiry ON node_management_audit(retention_hold,retain_until_ms,record_id)";
pub(super) const INDEX_DDL: &str = "CREATE INDEX main.node_management_audit_expiry ON node_management_audit(retention_hold,retain_until_ms,record_id)";
