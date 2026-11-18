-- Historical schema12: exact schema0..11 fixture plus schema12 DDL captured before schema13.
-- Exact historical schema0..11 DDL extracted from Git, before capacity implementation.
-- Baseline: 65d734f638bcb951e237b69faa592917b3a6d2c7
-- Regenerator: .tmp/capacity-storage/build_schema11_fixture.cjs (development handoff only).
-- Source app/src/session_storage/schema.rs SHA256 8247264a5d580b758ff49593b2d0896c18065b5d9fd55baa97b395b3b06e1910
-- Source app/src/session_storage/product_schema.rs SHA256 e3d2d32c0e3ec32f1c6ef4ed4a971f31c4f203bd8ca6d67063f81b4f14aa6c00
-- Source app/src/session_storage/registration_schema.rs SHA256 4bc3bb41a8b40456ac1bccf6bb331f1549b5e4e60b048a106504d4ba0efddf67
-- Source app/src/session_storage/native_dispatch_schema.rs SHA256 fe155c0d82b5aa587dee8d5000f5dc5e959f00e95974c4db07a886e536a2e0ad
-- Source app/src/session_storage/management_audit_schema.rs SHA256 14b5621c6f0dd73c341dcaa6207a1996efbe84863f98bd06199d5957e26c9cdc
-- Source app/src/session_storage/approval_owner_schema.rs SHA256 c5d1936477eece6e2503932287517587f1e67e516b3fc1a4d50b270d7499009c
-- Source app/src/session_storage/node_catalog_schema.rs SHA256 d5eddcb03f0a5415bf5fc278a52e5c512314d6cb2344c6487cc64172c1b2e152
-- Source app/src/session_storage/native_release_schema.rs SHA256 2d0eef62272638878d04ac507a177458ebc0530a3055a975bfd5a43b324182eb
-- Source app/src/session_storage/source_sessions_schema.rs SHA256 e9ddaf5167b79c16c934d6d3823c9751f71b0ab1c2639adf9976fa173e1f74a4

CREATE TABLE IF NOT EXISTS node_meta (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                schema_version INTEGER NOT NULL,
                node_id TEXT NOT NULL,
                public_key BLOB NOT NULL,
                key_file_ref TEXT NOT NULL
            );

CREATE TABLE IF NOT EXISTS clients (
                client_id TEXT PRIMARY KEY,
                public_key BLOB NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('trusted', 'revoked')),
                auth_generation INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                revoked_at INTEGER
            );

CREATE TABLE IF NOT EXISTS management_commands (
                actor_id TEXT NOT NULL,
                request_id TEXT NOT NULL,
                action TEXT NOT NULL,
                payload_digest TEXT NOT NULL,
                result_json TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                PRIMARY KEY (actor_id, request_id)
            );

CREATE TABLE IF NOT EXISTS security_audit (
                audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
                actor_id TEXT,
                action TEXT NOT NULL,
                target_id TEXT,
                outcome TEXT NOT NULL,
                occurred_at INTEGER NOT NULL
            );

CREATE TABLE IF NOT EXISTS node_state (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                revision INTEGER NOT NULL
            );

CREATE TABLE harnesses(harness_id TEXT PRIMARY KEY, name TEXT NOT NULL, executable TEXT NOT NULL, arguments_json TEXT NOT NULL, config_ref TEXT);

CREATE TABLE workspaces(workspace_id TEXT PRIMARY KEY, canonical_path TEXT NOT NULL);

CREATE TABLE sessions(session_id TEXT PRIMARY KEY, harness_id TEXT NOT NULL REFERENCES harnesses(harness_id), workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id), state TEXT NOT NULL CHECK(state IN ('loaded','released','releasing','interrupted')), native_scope TEXT, native_session_id TEXT, recovery_json TEXT);

CREATE TABLE content_objects(content_ref TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), actor_id TEXT NOT NULL REFERENCES clients(client_id), auth_generation INTEGER NOT NULL CHECK(auth_generation >= 0), purpose TEXT NOT NULL CHECK(purpose IN ('input','output')), body BLOB NOT NULL, sha256 TEXT NOT NULL CHECK(length(sha256) = 64), total_bytes INTEGER NOT NULL CHECK(total_bytes >= 0 AND total_bytes <= 16777216 AND length(body) = total_bytes));

CREATE TABLE session_commands(command_id TEXT PRIMARY KEY, actor_id TEXT NOT NULL REFERENCES clients(client_id), request_id TEXT NOT NULL, action TEXT NOT NULL, payload_digest TEXT NOT NULL, session_id TEXT NOT NULL REFERENCES sessions(session_id), status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','completed','failed','delivery_unknown')), UNIQUE(actor_id, request_id));

CREATE TABLE turns(turn_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), command_id TEXT NOT NULL UNIQUE REFERENCES session_commands(command_id), ordinal INTEGER NOT NULL CHECK(ordinal >= 0 AND ordinal <= 9007199254740991), UNIQUE(session_id, ordinal));

CREATE TABLE runs(run_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), command_id TEXT NOT NULL UNIQUE REFERENCES session_commands(command_id), turn_id TEXT NOT NULL UNIQUE REFERENCES turns(turn_id), status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','running','cancelling','completed','cancelled','failed','interrupted','unknown')));

CREATE TABLE conversation_items(item_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), turn_id TEXT NOT NULL REFERENCES turns(turn_id), source_id TEXT NOT NULL, first_position INTEGER NOT NULL CHECK(first_position >= 0 AND first_position <= 9007199254740991), kind TEXT NOT NULL, protocol TEXT NOT NULL DEFAULT 'platform', protocol_version TEXT NOT NULL DEFAULT '1', native_tool_id TEXT, native_tool_digest TEXT, tool_instance_id TEXT, tool_generation INTEGER, tool_native_session_id TEXT, tool_native_session_digest TEXT, content_ref TEXT NOT NULL REFERENCES content_objects(content_ref), content_version INTEGER NOT NULL CHECK(content_version >= 1 AND content_version <= 9007199254740991), committed_version INTEGER NOT NULL CHECK(committed_version >= 1 AND committed_version <= content_version), UNIQUE(session_id, first_position), UNIQUE(session_id, source_id));

CREATE TABLE approval_connections(instance_id TEXT PRIMARY KEY, generation INTEGER NOT NULL CHECK(generation >= 0 AND generation <= 9007199254740991), state TEXT NOT NULL CHECK(state IN ('active','closed')));

CREATE TABLE approval_run_sources(run_id TEXT PRIMARY KEY REFERENCES runs(run_id), session_id TEXT NOT NULL REFERENCES sessions(session_id), turn_id TEXT NOT NULL REFERENCES turns(turn_id), instance_id TEXT NOT NULL REFERENCES approval_connections(instance_id), generation INTEGER NOT NULL CHECK(generation >= 0 AND generation <= 9007199254740991), native_session_id TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('active','invalid')), invalid_reason TEXT);

CREATE TABLE approvals(approval_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(session_id), run_id TEXT NOT NULL REFERENCES runs(run_id), turn_id TEXT NOT NULL REFERENCES turns(turn_id), instance_id TEXT NOT NULL REFERENCES approval_connections(instance_id), connection_generation INTEGER NOT NULL, native_session_id TEXT NOT NULL, native_request_id TEXT NOT NULL, source_digest TEXT NOT NULL, request_kind TEXT NOT NULL, protocol TEXT NOT NULL, protocol_version TEXT NOT NULL, raw_request BLOB NOT NULL CHECK(length(raw_request) <= 16777216), scope_raw BLOB NOT NULL CHECK(length(scope_raw) <= 16777216), tool_native_id TEXT, tool_item_id TEXT, tool_source_id TEXT, status TEXT NOT NULL CHECK(status IN ('pending','decided','unsupported','invalid')), restriction TEXT, invalid_reason TEXT, decision_command_id TEXT UNIQUE REFERENCES session_commands(command_id), selected_choice_id TEXT REFERENCES approval_options(choice_id), deciding_actor_id TEXT REFERENCES clients(client_id), deciding_generation INTEGER, UNIQUE(instance_id,connection_generation,native_request_id), FOREIGN KEY(session_id,tool_item_id,tool_source_id) REFERENCES conversation_items(session_id,item_id,source_id));

CREATE TABLE approval_options(choice_id TEXT PRIMARY KEY, approval_id TEXT NOT NULL REFERENCES approvals(approval_id), ordinal INTEGER NOT NULL CHECK(ordinal >= 0), native_id TEXT NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL, UNIQUE(approval_id,ordinal));

CREATE TABLE approval_deliveries(approval_id TEXT PRIMARY KEY REFERENCES approvals(approval_id), state TEXT NOT NULL CHECK(state IN ('not_sent','claimed','transport_written','unknown','suppressed')), claim_id TEXT UNIQUE, write_report TEXT CHECK(write_report IN ('write_completed','unknown')), source_current_at_report INTEGER CHECK(source_current_at_report IN (0,1)));

CREATE TABLE approval_audit_links(audit_id INTEGER PRIMARY KEY REFERENCES security_audit(audit_id), approval_id TEXT NOT NULL REFERENCES approvals(approval_id), command_id TEXT REFERENCES session_commands(command_id), basis TEXT NOT NULL, actor_generation INTEGER, instance_id TEXT NOT NULL, connection_generation INTEGER NOT NULL);

CREATE TABLE session_state(session_id TEXT PRIMARY KEY REFERENCES sessions(session_id), revision INTEGER NOT NULL CHECK(revision >= 0 AND revision <= 9007199254740991), next_position INTEGER NOT NULL CHECK(next_position >= 0 AND next_position <= 9007199254740991), next_turn INTEGER NOT NULL CHECK(next_turn >= 0 AND next_turn <= 9007199254740991));

CREATE TABLE history_integrity(session_id TEXT NOT NULL REFERENCES sessions(session_id), run_id TEXT NOT NULL REFERENCES runs(run_id), reason TEXT NOT NULL, first_position INTEGER, last_position INTEGER, dropped_bytes INTEGER NOT NULL CHECK(dropped_bytes >= 0), PRIMARY KEY(session_id,run_id), CHECK((first_position IS NULL AND last_position IS NULL) OR (first_position >= 0 AND last_position >= first_position)));

CREATE TABLE history_content_pins(pin_id TEXT PRIMARY KEY, content_ref TEXT NOT NULL REFERENCES content_objects(content_ref));

CREATE UNIQUE INDEX tool_reference_key ON conversation_items(session_id,item_id,source_id);

CREATE UNIQUE INDEX tool_native_identity ON conversation_items(session_id,turn_id,tool_instance_id,tool_generation,tool_native_session_digest,native_tool_digest) WHERE native_tool_digest IS NOT NULL;

CREATE UNIQUE INDEX one_foreground_run ON runs(session_id) WHERE status IN ('accepted','dispatching','running','cancelling','unknown');

CREATE TABLE product_control (
        singleton INTEGER PRIMARY KEY CHECK(singleton=1),
        request_epoch TEXT NOT NULL CHECK(length(request_epoch)=22),
        time_floor_ms INTEGER NOT NULL CHECK(time_floor_ms BETWEEN 0 AND 9007199254740991)) STRICT;

CREATE TABLE product_sources (
        source_id TEXT PRIMARY KEY CHECK(length(source_id)=22),
        name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 80)) STRICT;

CREATE TABLE product_harness_sources (
        harness_id TEXT PRIMARY KEY REFERENCES harnesses(harness_id),
        source_id TEXT NOT NULL REFERENCES product_sources(source_id)) STRICT;

CREATE TABLE product_targets (
        target_id TEXT PRIMARY KEY CHECK(length(target_id)=22),
        source_id TEXT NOT NULL REFERENCES product_sources(source_id),
        native_id TEXT NOT NULL CHECK(length(CAST(native_id AS BLOB)) BETWEEN 1 AND 4096),
        workspace_id TEXT REFERENCES workspaces(workspace_id),
        UNIQUE(source_id,native_id)) STRICT;

CREATE TABLE product_commands (
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
        UNIQUE(actor_id,request_id)) STRICT;

CREATE TABLE product_runs (
        run_id TEXT PRIMARY KEY CHECK(length(run_id)=22),
        command_id TEXT NOT NULL UNIQUE REFERENCES product_commands(command_id),
        target_id TEXT NOT NULL REFERENCES product_targets(target_id),
        status TEXT NOT NULL CHECK(status IN ('accepted','dispatching','running','cancelling','completed','cancelled','failed','interrupted','unknown')),
        UNIQUE(target_id,run_id)) STRICT;

CREATE TABLE product_resources (
        load_epoch TEXT PRIMARY KEY CHECK(length(load_epoch)=22),
        target_id TEXT NOT NULL REFERENCES product_targets(target_id),
        runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
        connection_epoch TEXT NOT NULL CHECK(length(connection_epoch)=22),
        instance_id TEXT NOT NULL CHECK(length(instance_id)=22),
        state TEXT NOT NULL CHECK(state IN ('reserved','loaded','releasing','released','unknown')),
        revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND 9007199254740991),
        UNIQUE(target_id,load_epoch)) STRICT;

CREATE TABLE product_approvals (
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
        FOREIGN KEY(target_id,run_id) REFERENCES product_runs(target_id,run_id)) STRICT;

CREATE TABLE product_deliveries (
        approval_id TEXT PRIMARY KEY REFERENCES product_approvals(approval_id),
        state TEXT NOT NULL CHECK(state IN ('not_sent','claimed','transport_written','unknown','suppressed')),
        claim_id TEXT UNIQUE CHECK(claim_id IS NULL OR length(claim_id)=22),
        CHECK(state NOT IN ('claimed','transport_written','unknown') OR claim_id IS NOT NULL)) STRICT;

CREATE TABLE product_audit_slots (
        command_id TEXT NOT NULL REFERENCES product_commands(command_id),
        stage TEXT NOT NULL CHECK(stage IN ('admission','dispatch','result','reconciliation')),
        actor_id TEXT NOT NULL REFERENCES clients(client_id),
        reserved_at INTEGER NOT NULL CHECK(reserved_at BETWEEN 0 AND 9007199254740991),
        occurred_at INTEGER CHECK(occurred_at BETWEEN 0 AND 9007199254740991),
        retain_until INTEGER CHECK(retain_until BETWEEN 0 AND 9007199254740991),
        outcome TEXT CHECK(outcome IN ('accepted','dispatching','completed','failed','unknown','expired','reconciled','denied','cancelled')),
        CHECK((occurred_at IS NULL AND retain_until IS NULL AND outcome IS NULL) OR
              (occurred_at IS NOT NULL AND retain_until IS NOT NULL AND outcome IS NOT NULL AND retain_until>=occurred_at AND retain_until-occurred_at>=7776000000)),
        PRIMARY KEY(command_id,stage)) STRICT;

CREATE UNIQUE INDEX product_one_foreground_run ON product_runs(target_id) WHERE status IN ('accepted','dispatching','running','cancelling','unknown');

CREATE UNIQUE INDEX product_one_resource_owner ON product_resources(target_id) WHERE state!='released';

CREATE UNIQUE INDEX product_delivery_claim_identity
      ON product_deliveries(approval_id,claim_id);

CREATE TABLE product_delivery_attempts (
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
    ) STRICT;

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

CREATE TABLE node_native_source_instances (
  instance_id TEXT PRIMARY KEY CHECK(length(instance_id)=22),
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  connection_epoch TEXT NOT NULL UNIQUE CHECK(length(connection_epoch)=22),
  config_identity TEXT NOT NULL CHECK(length(config_identity)=22),
  config_revision BLOB NOT NULL CHECK(length(config_revision)=8),
  owner_kind TEXT NOT NULL CHECK(owner_kind IN ('windows_job','test_owned_stream')),
  lifecycle TEXT NOT NULL CHECK(lifecycle IN ('reserved','spawned','initialized','closing','closed','unknown')),
  scope_binding_id TEXT CHECK(scope_binding_id IS NULL OR length(scope_binding_id)=22),
  scope_bound_at_ms INTEGER,
  close_basis TEXT CHECK(close_basis IN ('never_launched','owned_converged','entered_never_created')),
  close_stage TEXT CHECK(close_stage IN ('never_entered','entered_before_create','create_rejected','owned_before_io','io_unstarted','source_bound','started','initialized','load_pending','loaded')),
  close_io_evidence TEXT CHECK(close_io_evidence IN ('not_applicable','not_prepared','before_adopt_rejected','adopted_unstarted_joined','started_joined')),
  close_owner_evidence TEXT CHECK(close_owner_evidence IN ('not_applicable','windows_job','test_owned_stream','no_process_created','suspended_root_exited')),
  close_job_active INTEGER CHECK(close_job_active IS NULL OR close_job_active=0),
  close_evidence_fingerprint BLOB CHECK(close_evidence_fingerprint IS NULL OR length(close_evidence_fingerprint)=32),
  protocol_version INTEGER CHECK(protocol_version IN (1,2)),
  initialize_rpc_kind TEXT CHECK(initialize_rpc_kind IN ('integer','string')),
  initialize_rpc_bytes BLOB,
  initialize_sequence BLOB CHECK(initialize_sequence IS NULL OR length(initialize_sequence)=8),
  initialize_receipt BLOB CHECK(initialize_receipt IS NULL OR length(initialize_receipt)=8),
  evidence_fingerprint BLOB CHECK(evidence_fingerprint IS NULL OR length(evidence_fingerprint)=32),
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  UNIQUE(instance_id,connection_epoch),
  UNIQUE(instance_id,scope_binding_id),
  CHECK((scope_binding_id IS NULL AND scope_bound_at_ms IS NULL) OR (scope_binding_id IS NOT NULL AND scope_bound_at_ms IS NOT NULL)),
  CHECK((lifecycle='closed' AND close_basis IS NOT NULL AND close_stage IS NOT NULL AND close_io_evidence IS NOT NULL AND close_owner_evidence IS NOT NULL AND close_evidence_fingerprint IS NOT NULL) OR
        (lifecycle!='closed' AND close_basis IS NULL AND close_stage IS NULL AND close_io_evidence IS NULL AND close_owner_evidence IS NULL AND close_job_active IS NULL AND close_evidence_fingerprint IS NULL)),
  CHECK(close_basis IS NULL OR
        (close_basis='never_launched' AND close_stage='never_entered' AND close_io_evidence='not_applicable' AND close_owner_evidence='not_applicable' AND close_job_active IS NULL) OR
        (close_basis='entered_never_created' AND close_stage IN ('entered_before_create','create_rejected') AND
          close_io_evidence='not_prepared' AND close_owner_evidence='no_process_created' AND close_job_active IS NULL) OR
        (close_basis='owned_converged' AND close_stage IN ('owned_before_io','io_unstarted','source_bound','started','initialized','load_pending','loaded') AND close_io_evidence!='not_applicable' AND
          ((close_owner_evidence='windows_job' AND close_job_active IS NOT NULL AND close_job_active=0) OR
           (close_owner_evidence='test_owned_stream' AND close_job_active IS NULL) OR
           (close_owner_evidence='suspended_root_exited' AND close_stage='owned_before_io' AND close_io_evidence='not_prepared' AND close_job_active IS NULL)))),
  CHECK(close_basis IS NULL OR close_basis!='never_launched' OR (scope_binding_id IS NULL AND initialize_rpc_kind IS NULL AND protocol_version IS NULL)),
  CHECK(close_basis IS NULL OR NOT(close_basis='entered_never_created' OR close_owner_evidence='suspended_root_exited') OR
        (owner_kind='windows_job' AND scope_binding_id IS NULL AND scope_bound_at_ms IS NULL AND protocol_version IS NULL AND
         initialize_rpc_kind IS NULL AND initialize_rpc_bytes IS NULL AND initialize_sequence IS NULL AND initialize_receipt IS NULL AND evidence_fingerprint IS NULL)),
  CHECK((initialize_rpc_kind IS NULL AND initialize_rpc_bytes IS NULL) OR
        (initialize_rpc_kind='integer' AND initialize_rpc_bytes IS NOT NULL AND length(initialize_rpc_bytes)=8) OR
        (initialize_rpc_kind='string' AND initialize_rpc_bytes IS NOT NULL)),
  CHECK(lifecycle!='initialized' OR (scope_binding_id IS NOT NULL AND protocol_version IS NOT NULL AND initialize_rpc_kind IS NOT NULL
    AND initialize_sequence IS NOT NULL AND initialize_receipt IS NOT NULL AND evidence_fingerprint IS NOT NULL))
) STRICT;

CREATE TABLE node_native_resource_bindings (
  load_epoch TEXT PRIMARY KEY CHECK(length(load_epoch)=22),
  target_id TEXT NOT NULL REFERENCES product_targets(target_id),
  owner_id TEXT NOT NULL CHECK(length(owner_id)=22),
  instance_id TEXT NOT NULL,
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  connection_epoch TEXT NOT NULL,
  config_identity TEXT NOT NULL CHECK(length(config_identity)=22),
  config_revision BLOB NOT NULL CHECK(length(config_revision)=8),
  lifecycle TEXT NOT NULL CHECK(lifecycle IN ('reserved','loading','loaded','releasing','released','unknown')),
  scope_binding_id TEXT CHECK(scope_binding_id IS NULL OR length(scope_binding_id)=22),
  release_basis TEXT CHECK(release_basis IN ('never_launched','owned_converged','target_native_converged','unloaded_reservation_abandoned','entered_never_created')),
  release_stage TEXT CHECK(release_stage IN ('never_entered','entered_before_create','create_rejected','reserved_no_target_io','bound_no_target_io','loading','loaded','unknown')),
  release_evidence_fingerprint BLOB CHECK(release_evidence_fingerprint IS NULL OR length(release_evidence_fingerprint)=32),
  protocol_version INTEGER CHECK(protocol_version IN (1,2)),
  load_rpc_kind TEXT CHECK(load_rpc_kind IN ('integer','string')),
  load_rpc_bytes BLOB,
  load_frame_length BLOB CHECK(load_frame_length IS NULL OR (length(load_frame_length)=8 AND load_frame_length>X'0000000000000000')),
  load_transport_state TEXT CHECK(load_transport_state IN ('armed','zero','unknown','written')),
  load_accepted_length BLOB CHECK(load_accepted_length IS NULL OR length(load_accepted_length)=8),
  load_transport_reason TEXT CHECK(load_transport_reason IN ('cancelled','deadline','fence_rejected','connection_closed','abandoned','io','invalid_fence','connection_lost','restart')),
  loaded_sequence BLOB CHECK(loaded_sequence IS NULL OR length(loaded_sequence)=8),
  loaded_receipt BLOB CHECK(loaded_receipt IS NULL OR length(loaded_receipt)=8),
  evidence_fingerprint BLOB CHECK(evidence_fingerprint IS NULL OR length(evidence_fingerprint)=32),
  reserved_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  UNIQUE(load_epoch,target_id),
  FOREIGN KEY(instance_id,connection_epoch) REFERENCES node_native_source_instances(instance_id,connection_epoch),
  FOREIGN KEY(instance_id,scope_binding_id) REFERENCES node_native_source_instances(instance_id,scope_binding_id),
  CHECK((load_rpc_kind IS NULL AND load_rpc_bytes IS NULL AND load_frame_length IS NULL AND load_transport_state IS NULL AND load_accepted_length IS NULL) OR
        (load_rpc_kind IS NOT NULL AND load_rpc_bytes IS NOT NULL AND load_frame_length IS NOT NULL AND load_transport_state IS NOT NULL AND load_accepted_length IS NOT NULL)),
  CHECK(load_rpc_kind IS NULL OR load_rpc_kind='string' OR length(load_rpc_bytes)=8),
  CHECK(load_accepted_length IS NULL OR load_accepted_length<=load_frame_length),
  CHECK(load_transport_state IS NULL OR
    (load_transport_state='armed' AND load_accepted_length=X'0000000000000000' AND load_transport_reason IS NULL) OR
    (load_transport_state='zero' AND load_accepted_length=X'0000000000000000' AND load_transport_reason IN ('cancelled','deadline','fence_rejected','connection_closed','abandoned')) OR
    (load_transport_state='unknown' AND load_transport_reason IN ('io','deadline','cancelled','fence_rejected','invalid_fence','connection_lost','restart')) OR
    (load_transport_state='written' AND load_accepted_length=load_frame_length AND load_transport_reason IS NULL)),
  CHECK(lifecycle NOT IN ('loading','loaded') OR (scope_binding_id IS NOT NULL AND protocol_version IS NOT NULL AND load_rpc_kind IS NOT NULL)),
  CHECK(lifecycle!='loaded' OR (loaded_sequence IS NOT NULL AND loaded_receipt IS NOT NULL AND evidence_fingerprint IS NOT NULL)),
  CHECK((lifecycle='released' AND release_basis IS NOT NULL AND release_stage IS NOT NULL AND release_evidence_fingerprint IS NOT NULL) OR
        (lifecycle!='released' AND release_basis IS NULL AND release_stage IS NULL AND release_evidence_fingerprint IS NULL)),
  CHECK(release_basis IS NULL OR release_basis!='unloaded_reservation_abandoned' OR
        (release_stage IN ('reserved_no_target_io','bound_no_target_io') AND loaded_sequence IS NULL AND loaded_receipt IS NULL AND
         (load_transport_state IS NULL OR load_transport_state='zero'))),
  CHECK(release_basis IS NULL OR release_basis!='never_launched' OR (release_stage='never_entered' AND scope_binding_id IS NULL AND load_rpc_kind IS NULL AND load_transport_state IS NULL)),
  CHECK((release_stage IN ('entered_before_create','create_rejected') AND release_basis='entered_never_created') OR
        (release_stage IS NULL OR release_stage NOT IN ('entered_before_create','create_rejected'))),
  CHECK(release_basis IS NULL OR release_basis!='entered_never_created' OR
        (release_stage IN ('entered_before_create','create_rejected') AND scope_binding_id IS NULL AND protocol_version IS NULL AND
         load_rpc_kind IS NULL AND load_rpc_bytes IS NULL AND load_frame_length IS NULL AND load_transport_state IS NULL AND
         load_accepted_length IS NULL AND load_transport_reason IS NULL AND loaded_sequence IS NULL AND loaded_receipt IS NULL AND evidence_fingerprint IS NULL))
) STRICT;

CREATE UNIQUE INDEX node_native_one_resource_owner
  ON node_native_resource_bindings(target_id) WHERE lifecycle!='released';

CREATE TABLE node_native_dispatches (
  dispatch_id TEXT PRIMARY KEY CHECK(length(dispatch_id)=22),
  planned_run_id TEXT NOT NULL UNIQUE CHECK(length(planned_run_id)=22),
  target_id TEXT NOT NULL REFERENCES product_targets(target_id),
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  native_id TEXT NOT NULL,
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  owner_id TEXT NOT NULL CHECK(length(owner_id)=22),
  instance_id TEXT NOT NULL CHECK(length(instance_id)=22),
  connection_epoch TEXT NOT NULL CHECK(length(connection_epoch)=22),
  load_epoch TEXT NOT NULL CHECK(length(load_epoch)=22),
  item_id TEXT NOT NULL REFERENCES node_request_registrations(request_id),
  content_revision BLOB NOT NULL CHECK(length(content_revision)=8),
  submit_request_id TEXT NOT NULL REFERENCES node_request_registrations(request_id),
  content_request_id TEXT NOT NULL REFERENCES node_request_registrations(request_id),
  content_digest BLOB NOT NULL CHECK(length(content_digest)=32),
  config_identity TEXT NOT NULL CHECK(length(config_identity)=22),
  config_revision BLOB NOT NULL CHECK(length(config_revision)=8),
  selection_kind TEXT NOT NULL CHECK(selection_kind IN ('waiting','existing_active')),
  lifecycle TEXT NOT NULL CHECK(lifecycle IN ('prepared_zero','selected','closed')),
  selection_id TEXT CHECK(selection_id IS NULL OR length(selection_id)=22),
  selection_owner_order BLOB CHECK(selection_owner_order IS NULL OR length(selection_owner_order)=8),
  selected_at_ms INTEGER,
  close_reason TEXT CHECK(close_reason IN ('known_zero','restart_zero','native_settled','resources_converged')),
  close_evidence_fingerprint BLOB CHECK(close_evidence_fingerprint IS NULL OR length(close_evidence_fingerprint)=32),
  prepared_at_ms INTEGER NOT NULL,
  closed_at_ms INTEGER,
  UNIQUE(target_id,dispatch_id),
  UNIQUE(dispatch_id,connection_epoch),
  UNIQUE(dispatch_id,item_id,content_revision),
  UNIQUE(dispatch_id,selection_id),
  FOREIGN KEY(load_epoch,target_id) REFERENCES node_native_resource_bindings(load_epoch,target_id),
  FOREIGN KEY(instance_id,connection_epoch) REFERENCES node_native_source_instances(instance_id,connection_epoch),
  CHECK((selection_id IS NULL AND selection_owner_order IS NULL AND selected_at_ms IS NULL) OR
        (selection_id IS NOT NULL AND selection_owner_order IS NOT NULL AND selected_at_ms IS NOT NULL)),
  CHECK(lifecycle!='prepared_zero' OR selection_id IS NULL),
  CHECK(lifecycle!='selected' OR selection_id IS NOT NULL),
  CHECK((lifecycle='closed' AND close_reason IS NOT NULL AND closed_at_ms IS NOT NULL AND close_evidence_fingerprint IS NOT NULL) OR
        (lifecycle!='closed' AND close_reason IS NULL AND closed_at_ms IS NULL AND close_evidence_fingerprint IS NULL))
) STRICT;

CREATE TABLE node_native_dispatch_slots (
  target_id TEXT PRIMARY KEY REFERENCES product_targets(target_id),
  dispatch_id TEXT NOT NULL UNIQUE,
  FOREIGN KEY(target_id,dispatch_id) REFERENCES node_native_dispatches(target_id,dispatch_id)
) STRICT;

CREATE TABLE node_native_write_attempts (
  attempt_id TEXT PRIMARY KEY CHECK(length(attempt_id)=22),
  dispatch_id TEXT NOT NULL,
  selection_id TEXT NOT NULL CHECK(length(selection_id)=22),
  item_id TEXT NOT NULL,
  content_revision BLOB NOT NULL CHECK(length(content_revision)=8),
  connection_epoch TEXT NOT NULL CHECK(length(connection_epoch)=22),
  protocol_version INTEGER NOT NULL CHECK(protocol_version IN (1,2)),
  rpc_id_kind TEXT NOT NULL CHECK(rpc_id_kind IN ('integer','string')),
  rpc_id_bytes BLOB NOT NULL,
  frame_length BLOB NOT NULL CHECK(length(frame_length)=8 AND frame_length>X'0000000000000000'),
  accepted_length BLOB NOT NULL CHECK(length(accepted_length)=8),
  transport_state TEXT NOT NULL CHECK(transport_state IN ('armed','zero','unknown','written')),
  transport_reason TEXT CHECK(transport_reason IN
    ('cancelled','deadline','fence_rejected','connection_closed','abandoned',
     'io','invalid_fence','connection_lost','restart')),
  armed_at_ms INTEGER NOT NULL,
  recorded_at_ms INTEGER,
  UNIQUE(connection_epoch,rpc_id_kind,rpc_id_bytes),
  FOREIGN KEY(dispatch_id,connection_epoch) REFERENCES node_native_dispatches(dispatch_id,connection_epoch),
  FOREIGN KEY(dispatch_id,item_id,content_revision) REFERENCES node_native_dispatches(dispatch_id,item_id,content_revision),
  FOREIGN KEY(dispatch_id,selection_id) REFERENCES node_native_dispatches(dispatch_id,selection_id),
  CHECK((rpc_id_kind='integer' AND length(rpc_id_bytes)=8) OR rpc_id_kind='string'),
  CHECK(accepted_length<=frame_length),
  CHECK((transport_state='armed' AND accepted_length=X'0000000000000000' AND transport_reason IS NULL AND recorded_at_ms IS NULL) OR
        (transport_state='zero' AND accepted_length=X'0000000000000000' AND transport_reason IN
          ('cancelled','deadline','fence_rejected','connection_closed','abandoned') AND recorded_at_ms IS NOT NULL) OR
        (transport_state='unknown' AND transport_reason IN
          ('io','deadline','cancelled','fence_rejected','invalid_fence','connection_lost','restart') AND recorded_at_ms IS NOT NULL) OR
        (transport_state='written' AND accepted_length=frame_length AND transport_reason IS NULL AND recorded_at_ms IS NOT NULL))
) STRICT;

CREATE UNIQUE INDEX node_native_one_unresolved_write
  ON node_native_write_attempts(item_id,content_revision)
  WHERE transport_state IN ('armed','unknown','written');

CREATE TABLE node_native_observations (
  observation_id TEXT PRIMARY KEY CHECK(length(observation_id)=22),
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  instance_id TEXT NOT NULL CHECK(length(instance_id)=22),
  connection_epoch TEXT NOT NULL CHECK(length(connection_epoch)=22),
  observation_sequence BLOB NOT NULL CHECK(length(observation_sequence)=8),
  receipt_order BLOB NOT NULL CHECK(length(receipt_order)=8),
  batch_fingerprint BLOB NOT NULL CHECK(length(batch_fingerprint)=32),
  observed_at_ms INTEGER NOT NULL,
  UNIQUE(connection_epoch,observation_sequence)
) STRICT;

CREATE TABLE node_native_dispatch_facts (
  observation_id TEXT NOT NULL REFERENCES node_native_observations(observation_id),
  fact_index BLOB NOT NULL CHECK(length(fact_index)=8),
  requirement_id TEXT NOT NULL UNIQUE CHECK(length(requirement_id)=22),
  target_id TEXT NOT NULL REFERENCES product_targets(target_id),
  owner_id TEXT NOT NULL CHECK(length(owner_id)=22),
  load_epoch TEXT NOT NULL CHECK(length(load_epoch)=22),
  owner_order BLOB NOT NULL CHECK(length(owner_order)=8),
  attempt_id TEXT REFERENCES node_native_write_attempts(attempt_id),
  fact_kind TEXT NOT NULL CHECK(fact_kind IN
    ('v1_prompt_result','v2_inserted','prompt_rejected','foreground_running',
     'foreground_requires_action','foreground_idle','foreground_unknown',
     'foreground_terminal','approval_present','approval_retired')),
  native_message_id TEXT,
  native_request_id_kind TEXT CHECK(native_request_id_kind IN ('integer','string')),
  native_request_id_bytes BLOB,
  result_kind TEXT CHECK(result_kind IN ('completed','stopped','failed','rejected','unknown')),
  native_stop_reason TEXT CHECK(native_stop_reason IN
    ('end_turn','max_tokens','max_turn_requests','refusal','cancelled','error','unknown')),
  transition_kind TEXT NOT NULL CHECK(transition_kind IN ('new','snapshot','repeated','unknown')),
  interval_serial BLOB CHECK(interval_serial IS NULL OR length(interval_serial)=8),
  PRIMARY KEY(observation_id,fact_index),
  CHECK((fact_kind='v1_prompt_result' AND attempt_id IS NOT NULL AND native_message_id IS NULL AND result_kind IS NOT NULL AND native_stop_reason IS NOT NULL) OR
        (fact_kind='v2_inserted' AND attempt_id IS NOT NULL AND native_message_id IS NOT NULL AND result_kind IS NULL AND native_stop_reason IS NULL) OR
        (fact_kind='prompt_rejected' AND attempt_id IS NOT NULL AND native_message_id IS NULL AND result_kind='rejected' AND native_stop_reason IS NULL) OR
        (fact_kind IN ('foreground_running','foreground_requires_action','foreground_idle','foreground_unknown','approval_present','approval_retired')
          AND attempt_id IS NULL AND native_message_id IS NULL AND result_kind IS NULL) OR
        (fact_kind='foreground_terminal' AND attempt_id IS NULL AND native_message_id IS NULL AND result_kind IS NOT NULL AND native_stop_reason IS NOT NULL
          AND transition_kind='new' AND interval_serial IS NOT NULL)),
  CHECK(result_kind IS NULL OR result_kind='rejected' OR
        (result_kind='completed' AND native_stop_reason='end_turn') OR
        (result_kind='stopped' AND native_stop_reason IN ('max_tokens','max_turn_requests','refusal','cancelled')) OR
        (result_kind='failed' AND native_stop_reason='error') OR
        (result_kind='unknown' AND native_stop_reason='unknown')),
  CHECK(fact_kind!='v1_prompt_result' OR native_stop_reason!='error'),
  CHECK((fact_kind IN ('approval_present','approval_retired') AND native_request_id_kind IS NOT NULL AND native_request_id_bytes IS NOT NULL
          AND (native_request_id_kind='string' OR length(native_request_id_bytes)=8)) OR
        (fact_kind NOT IN ('approval_present','approval_retired') AND native_request_id_kind IS NULL AND native_request_id_bytes IS NULL))
) STRICT;

CREATE INDEX node_native_facts_target
  ON node_native_dispatch_facts(target_id,load_epoch,owner_order);

CREATE TABLE node_native_convergences (
  convergence_id TEXT PRIMARY KEY CHECK(length(convergence_id)=22),
  requirement_id TEXT NOT NULL UNIQUE CHECK(length(requirement_id)=22),
  target_id TEXT NOT NULL REFERENCES product_targets(target_id),
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  owner_id TEXT NOT NULL CHECK(length(owner_id)=22),
  instance_id TEXT NOT NULL CHECK(length(instance_id)=22),
  connection_epoch TEXT NOT NULL CHECK(length(connection_epoch)=22),
  load_epoch TEXT NOT NULL CHECK(length(load_epoch)=22),
  owner_order BLOB NOT NULL CHECK(length(owner_order)=8),
  evidence_kind TEXT NOT NULL CHECK(evidence_kind IN
    ('joined_owned_source','native_target_closed_verified','old_runtime_stopped')),
  evidence_fingerprint BLOB NOT NULL CHECK(length(evidence_fingerprint)=32),
  verified_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE node_native_source_losses (
  loss_id TEXT PRIMARY KEY CHECK(length(loss_id)=22),
  source_requirement_id TEXT NOT NULL UNIQUE CHECK(length(source_requirement_id)=22),
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  instance_id TEXT NOT NULL,
  connection_epoch TEXT NOT NULL,
  scope_binding_id TEXT NOT NULL,
  reason TEXT NOT NULL CHECK(reason IN ('eof','io','protocol','stopped','observation_fault')),
  metadata_fingerprint BLOB NOT NULL CHECK(length(metadata_fingerprint)=32),
  observed_at_ms INTEGER NOT NULL,
  FOREIGN KEY(instance_id,connection_epoch) REFERENCES node_native_source_instances(instance_id,connection_epoch),
  FOREIGN KEY(instance_id,scope_binding_id) REFERENCES node_native_source_instances(instance_id,scope_binding_id)
) STRICT;

CREATE TABLE node_native_loss_targets (
  loss_id TEXT NOT NULL REFERENCES node_native_source_losses(loss_id),
  target_id TEXT NOT NULL REFERENCES product_targets(target_id),
  load_epoch TEXT NOT NULL,
  owner_id TEXT NOT NULL CHECK(length(owner_id)=22),
  owner_order BLOB NOT NULL CHECK(length(owner_order)=8),
  requirement_id TEXT NOT NULL UNIQUE CHECK(length(requirement_id)=22),
  metadata_fingerprint BLOB NOT NULL CHECK(length(metadata_fingerprint)=32),
  PRIMARY KEY(loss_id,target_id,load_epoch),
  FOREIGN KEY(load_epoch,target_id) REFERENCES node_native_resource_bindings(load_epoch,target_id)
) STRICT;

CREATE TABLE node_native_control_facts (
  control_id TEXT NOT NULL CHECK(length(control_id)=22),
  phase TEXT NOT NULL CHECK(phase IN ('reserved','terminal')),
  obligation_id TEXT NOT NULL CHECK(length(obligation_id)=22),
  attempt_serial BLOB NOT NULL CHECK(length(attempt_serial)=8),
  requirement_id TEXT NOT NULL UNIQUE CHECK(length(requirement_id)=22),
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  instance_id TEXT NOT NULL,
  connection_epoch TEXT NOT NULL,
  scope_binding_id TEXT NOT NULL,
  target_id TEXT,
  owner_id TEXT CHECK(owner_id IS NULL OR length(owner_id)=22),
  load_epoch TEXT,
  work_serial BLOB CHECK(work_serial IS NULL OR length(work_serial)=8),
  intent_kind TEXT NOT NULL CHECK(intent_kind IN ('priority','shutdown','unsupported')),
  intent_reference TEXT NOT NULL CHECK(length(intent_reference)=22),
  control_kind TEXT NOT NULL CHECK(control_kind IN ('native_cancel','permission_cancelled','unsupported')),
  rpc_id_kind TEXT CHECK(rpc_id_kind IN ('integer','string')),
  rpc_id_bytes BLOB,
  frame_length BLOB CHECK(frame_length IS NULL OR (length(frame_length)=8 AND frame_length>X'0000000000000000')),
  transport_state TEXT NOT NULL CHECK(transport_state IN ('reserved','zero','unknown','written')),
  transport_reason TEXT CHECK(transport_reason IN ('cancelled','deadline','fence_rejected','connection_closed','abandoned','io','invalid_fence','connection_lost','restart')),
  accepted_length BLOB NOT NULL CHECK(length(accepted_length)=8),
  metadata_fingerprint BLOB NOT NULL CHECK(length(metadata_fingerprint)=32),
  observed_at_ms INTEGER NOT NULL,
  PRIMARY KEY(control_id,phase),
  UNIQUE(obligation_id,attempt_serial,phase),
  FOREIGN KEY(instance_id,connection_epoch) REFERENCES node_native_source_instances(instance_id,connection_epoch),
  FOREIGN KEY(instance_id,scope_binding_id) REFERENCES node_native_source_instances(instance_id,scope_binding_id),
  FOREIGN KEY(load_epoch,target_id) REFERENCES node_native_resource_bindings(load_epoch,target_id),
  CHECK((control_kind='unsupported' AND intent_kind='unsupported' AND target_id IS NULL AND owner_id IS NULL AND load_epoch IS NULL AND work_serial IS NULL) OR
        (control_kind IN ('native_cancel','permission_cancelled') AND intent_kind IN ('priority','shutdown') AND target_id IS NOT NULL AND owner_id IS NOT NULL AND load_epoch IS NOT NULL) OR
        (control_kind='permission_cancelled' AND intent_kind='shutdown' AND target_id IS NULL AND owner_id IS NULL AND load_epoch IS NULL AND work_serial IS NULL)),
  CHECK((control_kind='native_cancel' AND rpc_id_kind IS NULL AND rpc_id_bytes IS NULL) OR
        (control_kind IN ('permission_cancelled','unsupported') AND rpc_id_kind IS NOT NULL AND rpc_id_bytes IS NOT NULL AND
         (rpc_id_kind='string' OR length(rpc_id_bytes)=8))),
  CHECK(frame_length IS NULL OR accepted_length<=frame_length),
  CHECK(frame_length IS NOT NULL OR (phase='terminal' AND transport_state='zero')),
  CHECK((phase='reserved' AND transport_state='reserved' AND frame_length IS NOT NULL AND accepted_length=X'0000000000000000' AND transport_reason IS NULL) OR
        (phase='terminal' AND transport_state='zero' AND accepted_length=X'0000000000000000' AND transport_reason IN ('cancelled','deadline','fence_rejected','connection_closed','abandoned')) OR
        (phase='terminal' AND transport_state='unknown' AND frame_length IS NOT NULL AND transport_reason IN ('io','deadline','cancelled','fence_rejected','invalid_fence','connection_lost','restart')) OR
        (phase='terminal' AND transport_state='written' AND frame_length IS NOT NULL AND accepted_length=frame_length AND transport_reason IS NULL))
) STRICT;

CREATE TABLE node_management_audit (
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
) STRICT;

CREATE INDEX node_management_audit_expiry ON node_management_audit(retention_hold,retain_until_ms,record_id);

CREATE TABLE node_approval_requests (
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
) STRICT;

CREATE TABLE node_approval_associations (
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
) STRICT;

CREATE TABLE node_approval_commands (
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
) STRICT;

CREATE TABLE node_approval_decisions (
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
) STRICT;

CREATE TABLE node_approval_reply_attempts (
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
) STRICT;

CREATE TABLE node_approval_reply_facts (
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
) STRICT;

CREATE TABLE node_approval_invalidations (
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
) STRICT;

CREATE TABLE node_approval_audit (
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
) STRICT;

CREATE INDEX node_approval_by_source
  ON node_approval_requests(instance_id,scope_binding_id,approval_id);

CREATE INDEX node_approval_commands_by_actor
  ON node_approval_commands(actor_id,registration_id);

CREATE INDEX node_approval_replies_by_request
  ON node_approval_reply_attempts(approval_id,attempt_serial);

CREATE INDEX node_approval_audit_expiry
  ON node_approval_audit(retention_hold,retain_until_ms,record_id);

CREATE TABLE node_catalog_commands (
 request_id TEXT PRIMARY KEY CHECK(length(request_id)=22),
 actor_id TEXT NOT NULL REFERENCES clients(client_id),
 register_auth_generation INTEGER NOT NULL CHECK(register_auth_generation>=0),
 action TEXT NOT NULL CHECK(action IN ('harness.install','profile.publish','target.bind')),
 state TEXT NOT NULL CHECK(state IN ('registered','committed','rejected')),
 registered_at_ms INTEGER NOT NULL,
 intent_cipher BLOB,
 intent_digest BLOB NOT NULL CHECK(length(intent_digest)=32),
 admission_auth_generation INTEGER CHECK(admission_auth_generation IS NULL OR admission_auth_generation>=0),
 result_kind TEXT CHECK(result_kind IN ('harness','profile','target','rejected')),
 result_id TEXT CHECK(result_id IS NULL OR length(result_id)=22),
 profile_id TEXT,
 profile_revision BLOB CHECK(profile_revision IS NULL OR length(profile_revision)=8),
 result_at_ms INTEGER,
 FOREIGN KEY(profile_id,profile_revision) REFERENCES node_launch_profiles(profile_id,revision),
 CHECK((profile_id IS NULL)=(profile_revision IS NULL)),
 CHECK((state='registered' AND intent_cipher IS NOT NULL AND admission_auth_generation IS NULL
        AND result_kind IS NULL AND result_id IS NULL AND profile_id IS NULL AND result_at_ms IS NULL)
    OR (state='committed' AND intent_cipher IS NULL AND admission_auth_generation IS NOT NULL
        AND result_kind IS NOT NULL AND result_kind IN ('harness','profile','target') AND result_id IS NOT NULL AND profile_id IS NOT NULL AND result_at_ms IS NOT NULL)
    OR (state='rejected' AND intent_cipher IS NULL AND admission_auth_generation IS NOT NULL
        AND result_kind IS NOT NULL AND result_kind='rejected' AND result_id IS NULL AND profile_id IS NULL AND result_at_ms IS NOT NULL))
) STRICT;

CREATE TABLE node_launch_profiles (
 profile_id TEXT NOT NULL CHECK(length(profile_id)=22),
 revision BLOB NOT NULL CHECK(length(revision)=8 AND revision!=X'0000000000000000'),
 harness_id TEXT NOT NULL REFERENCES harnesses(harness_id),
 source_id TEXT NOT NULL REFERENCES product_sources(source_id),
 payload_format INTEGER NOT NULL CHECK(payload_format=1),
 protected_payload BLOB NOT NULL CHECK(length(protected_payload)>0),
 payload_digest BLOB NOT NULL CHECK(length(payload_digest)=32),
 created_by_request_id TEXT NOT NULL UNIQUE REFERENCES node_catalog_commands(request_id),
 created_at_ms INTEGER NOT NULL,
 PRIMARY KEY(profile_id,revision)
) STRICT;

CREATE TABLE node_target_launch_selection (
 target_id TEXT PRIMARY KEY REFERENCES product_targets(target_id),
 profile_id TEXT NOT NULL,
 profile_revision BLOB NOT NULL CHECK(length(profile_revision)=8),
 workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id),
 launch_path TEXT NOT NULL,
 observed_canonical_path TEXT NOT NULL,
 selected_by_request_id TEXT NOT NULL UNIQUE REFERENCES node_catalog_commands(request_id),
 selected_at_ms INTEGER NOT NULL,
 FOREIGN KEY(profile_id,profile_revision) REFERENCES node_launch_profiles(profile_id,revision)
) STRICT;

CREATE TABLE node_native_release_operations (
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
) STRICT;

CREATE INDEX node_native_release_actor_lookup
  ON node_native_release_operations(actor_id,target_id,updated_at_ms);

CREATE TABLE node_source_session_registrations (
  operation_id TEXT PRIMARY KEY CHECK(length(operation_id)=22),
  action TEXT NOT NULL CHECK(action IN ('discover','create')),
  actor_id TEXT NOT NULL REFERENCES clients(client_id),
  register_auth_generation INTEGER NOT NULL CHECK(register_auth_generation>=0),
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  harness_id TEXT NOT NULL REFERENCES harnesses(harness_id),
  profile_id TEXT NOT NULL,
  profile_revision BLOB NOT NULL CHECK(length(profile_revision)=8 AND profile_revision!=X'0000000000000000'),
  workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id),
  launch_path TEXT NOT NULL CHECK(length(launch_path)>0),
  cursor TEXT,
  intent_digest BLOB NOT NULL CHECK(length(intent_digest)=32),
  registered_at_ms INTEGER NOT NULL,
  UNIQUE(operation_id,action),
  FOREIGN KEY(profile_id,profile_revision) REFERENCES node_launch_profiles(profile_id,revision),
  CHECK(action='discover' OR cursor IS NULL)
) STRICT;

CREATE TABLE node_source_session_operations (
  operation_id TEXT PRIMARY KEY,
  action TEXT NOT NULL CHECK(action IN ('discover','create')),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  admission_auth_generation INTEGER NOT NULL CHECK(admission_auth_generation>=0),
  admitted_at_ms INTEGER NOT NULL,
  operation_state TEXT NOT NULL CHECK(operation_state IN ('owned','terminal','interrupted')),
  outcome TEXT NOT NULL CHECK(outcome IN ('pending','succeeded','refused','unknown')),
  source_instance_id TEXT,
  connection_epoch TEXT,
  protocol_version INTEGER CHECK(protocol_version IN (1,2)),
  rpc_id_kind TEXT CHECK(rpc_id_kind IN ('integer','string')),
  rpc_id_bytes BLOB,
  frame_length BLOB CHECK(frame_length IS NULL OR (length(frame_length)=8 AND frame_length>X'0000000000000000')),
  accepted_length BLOB CHECK(accepted_length IS NULL OR length(accepted_length)=8),
  transport_state TEXT CHECK(transport_state IN ('reserved','armed','zero','unknown','written')),
  transport_reason TEXT CHECK(transport_reason IN
    ('cancelled','deadline','fence_rejected','connection_closed','abandoned',
     'io','invalid_fence','connection_lost','restart')),
  transport_recorded_at_ms INTEGER,
  response_kind TEXT CHECK(response_kind IN ('created','sessions','remote_error')),
  native_session_id TEXT,
  remote_error_code INTEGER,
  response_sequence BLOB CHECK(response_sequence IS NULL OR length(response_sequence)=8),
  response_receipt BLOB CHECK(response_receipt IS NULL OR length(response_receipt)=8),
  response_evidence BLOB CHECK(response_evidence IS NULL OR length(response_evidence)=32),
  result_bytes BLOB CHECK(result_bytes IS NULL OR length(result_bytes)=8),
  ack_state TEXT CHECK(ack_state IN ('pending','confirmed','rejected','unknown')),
  failure_class TEXT CHECK(failure_class IN
    ('configuration','protocol','source','metadata','ownership','allocation','interrupted')),
  updated_at_ms INTEGER NOT NULL,
  terminal_at_ms INTEGER,
  FOREIGN KEY(operation_id,action) REFERENCES node_source_session_registrations(operation_id,action),
  FOREIGN KEY(source_instance_id,connection_epoch)
    REFERENCES node_native_source_instances(instance_id,connection_epoch),
  UNIQUE(connection_epoch,rpc_id_kind,rpc_id_bytes),
  CHECK((source_instance_id IS NULL AND connection_epoch IS NULL AND protocol_version IS NULL)
     OR (source_instance_id IS NOT NULL AND length(source_instance_id)=22
         AND connection_epoch IS NOT NULL AND length(connection_epoch)=22)),
  CHECK((rpc_id_kind IS NULL AND rpc_id_bytes IS NULL AND frame_length IS NULL
         AND accepted_length IS NULL AND transport_state IS NULL AND transport_reason IS NULL
         AND transport_recorded_at_ms IS NULL)
     OR (source_instance_id IS NOT NULL AND protocol_version IS NOT NULL
         AND rpc_id_kind IS NOT NULL AND rpc_id_bytes IS NOT NULL AND frame_length IS NOT NULL
         AND accepted_length IS NOT NULL AND accepted_length<=frame_length
         AND transport_state IS NOT NULL
         AND (rpc_id_kind='string' OR length(rpc_id_bytes)=8))),
  CHECK(transport_state IS NULL OR
    (transport_state IN ('reserved','armed') AND accepted_length=X'0000000000000000'
      AND transport_reason IS NULL AND transport_recorded_at_ms IS NULL) OR
    (transport_state='zero' AND accepted_length=X'0000000000000000'
      AND transport_reason IS NOT NULL AND transport_reason IN ('cancelled','deadline','fence_rejected','connection_closed','abandoned')
      AND transport_recorded_at_ms IS NOT NULL) OR
    (transport_state='unknown' AND transport_reason IS NOT NULL AND transport_reason IN
       ('io','deadline','cancelled','fence_rejected','invalid_fence','connection_lost','restart')
      AND transport_recorded_at_ms IS NOT NULL) OR
    (transport_state='written' AND accepted_length=frame_length
      AND transport_reason IS NULL AND transport_recorded_at_ms IS NOT NULL)),
  CHECK((response_kind IS NULL AND native_session_id IS NULL AND remote_error_code IS NULL
         AND response_sequence IS NULL AND response_receipt IS NULL AND response_evidence IS NULL
         AND result_bytes IS NULL AND ack_state IS NULL)
     OR (response_kind IS NOT NULL AND rpc_id_kind IS NOT NULL
         AND response_sequence IS NOT NULL AND response_receipt IS NOT NULL
         AND response_evidence IS NOT NULL AND result_bytes IS NOT NULL AND ack_state IS NOT NULL
         AND ((response_kind='created' AND action='create' AND native_session_id IS NOT NULL
               AND remote_error_code IS NULL)
           OR (response_kind='sessions' AND action='discover' AND native_session_id IS NULL
               AND remote_error_code IS NULL)
           OR (response_kind='remote_error' AND native_session_id IS NULL
               AND remote_error_code IS NOT NULL)))),
  CHECK((operation_state='owned' AND outcome='pending' AND terminal_at_ms IS NULL)
     OR (operation_state='terminal' AND outcome IN ('succeeded','refused','unknown')
         AND terminal_at_ms IS NOT NULL)
     OR (operation_state='interrupted' AND outcome='unknown' AND terminal_at_ms IS NOT NULL)),
  CHECK(outcome!='succeeded' OR (response_kind IS NOT NULL AND ack_state IS NOT NULL
      AND response_kind IN ('created','sessions') AND ack_state='confirmed')),
  CHECK(response_kind IS NULL OR (transport_state IS NOT NULL
      AND transport_state IN ('armed','unknown','written'))),
  CHECK(outcome!='refused' OR failure_class IS NOT NULL
      OR (response_kind IS NOT NULL AND response_kind='remote_error')),
  CHECK(outcome!='refused' OR response_kind IS NULL
     OR (response_kind='remote_error' AND ack_state IS NOT NULL AND ack_state='confirmed'))
) STRICT;

CREATE INDEX node_source_session_runtime_operations
  ON node_source_session_operations(runtime_epoch,operation_state);

CREATE INDEX node_source_session_created_owners
  ON node_source_session_operations(native_session_id,source_instance_id)
  WHERE response_kind='created';

CREATE TABLE node_session_capacity (
  reservation_id TEXT PRIMARY KEY CHECK(length(reservation_id)=22),
  source_id TEXT NOT NULL REFERENCES product_sources(source_id),
  native_id TEXT CHECK(native_id IS NULL OR length(CAST(native_id AS BLOB)) BETWEEN 1 AND 4096),
  instance_id TEXT NOT NULL REFERENCES node_native_source_instances(instance_id),
  runtime_epoch TEXT NOT NULL CHECK(length(runtime_epoch)=22),
  create_operation TEXT REFERENCES node_source_session_operations(operation_id),
  root_verified INTEGER NOT NULL CHECK(root_verified IN (0,1)),
  physically_closed INTEGER NOT NULL CHECK(physically_closed IN (0,1)),
  released INTEGER NOT NULL CHECK(released IN (0,1)),
  CHECK(root_verified=0 OR (native_id IS NOT NULL AND create_operation IS NOT NULL)),
  CHECK(released=0 OR physically_closed=1)
) STRICT;
CREATE INDEX node_session_capacity_native
  ON node_session_capacity(source_id,native_id,released);
CREATE INDEX node_session_capacity_instance
  ON node_session_capacity(instance_id,runtime_epoch,released);
CREATE TABLE node_session_capacity_releases (
  operation_id TEXT NOT NULL REFERENCES node_native_release_operations(operation_id),
  reservation_id TEXT NOT NULL REFERENCES node_session_capacity(reservation_id),
  PRIMARY KEY(operation_id,reservation_id)
) STRICT;
