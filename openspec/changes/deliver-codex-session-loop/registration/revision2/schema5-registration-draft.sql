-- Engineering candidate only. Not executed, not migration-approved.
-- Execute only inside the reviewed BEGIN IMMEDIATE migration after schema4 validation.
CREATE TABLE node_request_registrations (
    request_id TEXT PRIMARY KEY CHECK(length(request_id)=22),
    intent_version INTEGER NOT NULL CHECK(intent_version=1),
    actor_id TEXT NOT NULL REFERENCES clients(client_id),
    register_auth_generation INTEGER NOT NULL CHECK(register_auth_generation>=0),
    action TEXT NOT NULL CHECK(action IN
        ('input.submit','input.edit','input.cancel','input.prioritize','input.resume')),
    target_id TEXT NOT NULL REFERENCES product_targets(target_id),
    item_id TEXT CHECK(item_id IS NULL OR length(item_id)=22),
    expected_revision BLOB CHECK(expected_revision IS NULL OR length(expected_revision)=8),
    body_digest BLOB CHECK(body_digest IS NULL OR length(body_digest)=32),
    registered_at_ms INTEGER NOT NULL,
    retired INTEGER NOT NULL DEFAULT 0 CHECK(retired IN (0,1)),
    CHECK(
        (action='input.submit' AND item_id IS NULL AND expected_revision IS NULL
            AND body_digest IS NOT NULL) OR
        (action='input.edit' AND item_id IS NOT NULL AND expected_revision IS NOT NULL
            AND body_digest IS NOT NULL) OR
        (action IN ('input.cancel','input.prioritize') AND item_id IS NOT NULL
            AND expected_revision IS NOT NULL AND body_digest IS NULL) OR
        (action='input.resume' AND item_id IS NULL AND expected_revision IS NULL
            AND body_digest IS NULL))
) STRICT;

CREATE TABLE node_request_operations (
    request_id TEXT PRIMARY KEY REFERENCES node_request_registrations(request_id),
    operation_id TEXT NOT NULL UNIQUE CHECK(length(operation_id)=22),
    runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
    admission_auth_generation INTEGER NOT NULL CHECK(admission_auth_generation>=0),
    phase TEXT NOT NULL CHECK(phase IN ('applying','settled','interrupted')),
    begun_at_ms INTEGER NOT NULL,
    resolved_at_ms INTEGER,
    result_code TEXT CHECK(result_code IN
        ('queued','direct_selected','edited','cancelled','prioritized','resumed','rejected')),
    result_item_id TEXT CHECK(result_item_id IS NULL OR length(result_item_id)=22),
    result_revision BLOB CHECK(result_revision IS NULL OR length(result_revision)=8),
    restored_count INTEGER CHECK(restored_count BETWEEN 0 AND 3),
    rejection_code TEXT CHECK(rejection_code IN
        ('full','duplicate_id','not_found','revision_changed','revision_exhausted',
         'dispatch_started','suspended','busy','sequence_exhausted')),
    CHECK(
        (phase='applying' AND resolved_at_ms IS NULL AND result_code IS NULL
            AND result_item_id IS NULL AND result_revision IS NULL
            AND restored_count IS NULL AND rejection_code IS NULL) OR
        (phase='interrupted' AND resolved_at_ms IS NOT NULL AND result_code IS NULL
            AND result_item_id IS NULL AND result_revision IS NULL
            AND restored_count IS NULL AND rejection_code IS NULL) OR
        (phase='settled' AND resolved_at_ms IS NOT NULL AND result_code IS NOT NULL AND (
            (result_code IN ('queued','direct_selected','edited','cancelled','prioritized')
                AND result_item_id IS NOT NULL AND result_revision IS NOT NULL
                AND restored_count IS NULL AND rejection_code IS NULL) OR
            (result_code='resumed' AND result_item_id IS NULL AND result_revision IS NULL
                AND restored_count IS NOT NULL AND rejection_code IS NULL) OR
            (result_code='rejected' AND result_item_id IS NULL AND result_revision IS NULL
                AND restored_count IS NULL AND rejection_code IS NOT NULL))))
) STRICT;

CREATE INDEX node_request_target_lookup
    ON node_request_registrations(target_id,request_id);

-- No Run/foreground uniqueness or body column. No old table/index/row is changed here.
-- Migration implementation additionally updates node_meta.schema_version/user_version
-- together, validates all old/new definitions, and commits atomically.
