-- NativeDispatch revision6 frozen ENGINEERING CANDIDATE; DO NOT EXECUTE.
-- Append only after unchanged schema1..5, inside the original BEGIN IMMEDIATE.
-- No prompt/raw/config/credentials/body column. Runtime ack frontier is not restored from SQL.
-- All u64 values are exact 8-byte unsigned big-endian BLOBs.

-- Minimal source/resource ownership used by this same dispatch stage.
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
  close_basis TEXT CHECK(close_basis IN ('never_launched','owned_converged')),
  close_stage TEXT CHECK(close_stage IN ('never_entered','owned_before_io','io_unstarted','source_bound','started','initialized','load_pending','loaded')),
  close_io_evidence TEXT CHECK(close_io_evidence IN ('not_applicable','not_prepared','before_adopt_rejected','adopted_unstarted_joined','started_joined')),
  close_owner_evidence TEXT CHECK(close_owner_evidence IN ('not_applicable','windows_job','test_owned_stream')),
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
        (close_basis='owned_converged' AND close_stage!='never_entered' AND close_io_evidence!='not_applicable' AND
          ((close_owner_evidence='windows_job' AND close_job_active IS NOT NULL AND close_job_active=0) OR
           (close_owner_evidence='test_owned_stream' AND close_job_active IS NULL)))),
  CHECK(close_basis IS NULL OR close_basis!='never_launched' OR (scope_binding_id IS NULL AND initialize_rpc_kind IS NULL AND protocol_version IS NULL)),
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
  release_basis TEXT CHECK(release_basis IN ('never_launched','owned_converged','target_native_converged','unloaded_reservation_abandoned')),
  release_stage TEXT CHECK(release_stage IN ('never_entered','reserved_no_target_io','bound_no_target_io','loading','loaded','unknown')),
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
  CHECK(release_basis IS NULL OR release_basis!='never_launched' OR (release_stage='never_entered' AND scope_binding_id IS NULL AND load_rpc_kind IS NULL AND load_transport_state IS NULL))
) STRICT;
CREATE UNIQUE INDEX node_native_one_resource_owner
  ON node_native_resource_bindings(target_id) WHERE lifecycle!='released';
-- reserve_resource also checks the existing product_resources live/unknown owner in the same
-- BEGIN IMMEDIATE. Existing rows are not silently released, rewritten, migrated or adopted.
-- A loaded SQL row may still have nonterminal load transport; only the live private binding,
-- exact Written proof, current native foreground, gaps and ownership together permit dispatch.

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

-- Sequence is global to the actual connection, target is deliberately absent.
-- Only Received batches with necessary metadata are persisted; gaps are legitimate.
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

-- A terminal receiver error has no Received/sequence. Real resource evidence is separate.
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

-- Loss is not convergence. There is deliberately no Received sequence/receipt column.
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

-- Repository validation is mandatory in addition to DDL:
-- - exact product_targets -> source/native match; content request settled provenance;
-- - all mutable dispatch selection fields change atomically with the first armed attempt;
-- - observation/fact/attempt target/source/runtime/instance/connection/load equivalence;
-- - ordered contiguous fact_index inside a batch; exact metadata fingerprint/idempotence;
-- - v1 RPC result vs v2 aggregate foreground causality, snapshot/repeated semantics;
-- - approval existence identity belongs to its real owner (no synthesized Run);
-- - SQL rows never reconstruct any live Probe/SelectedProof/Ticket/ReleaseProof;
-- - terminal transport transitions are armed -> zero|unknown|written, no different terminal;
-- - no closed dispatch reopens a slot; no released resource bypasses unresolved prior gaps.
-- Then, inside the SAME original migration transaction:
-- UPDATE node_meta SET schema_version=6;
-- PRAGMA user_version=6;
-- validate exact old+new schema, COMMIT. No user DB is authorized to be opened here.

-- revision4 repository-only semantic guards, not replaceable by row data:
-- record_source_bound accepts the one live Pre->Bound proof, never epoch-to-Scope reconstruction.
-- record_source_loss persists Unknown only; no Loss/StoredLoss accepted by any release method.
-- close_unlaunched requires irreversible unused LaunchPermit revocation, SQL-operation quiescence,
-- exact all-reserved resource set, no bound/init/load/dispatch/loss, same live controller identity.
-- It atomically marks only new reserved rows released/closed with never_launched evidence.
-- Caller Drop, launch Err/timeout, empty Option or absent handle are not zero-launch evidence.
-- Closed/released identical retries return original fact; different proof or reopening is Conflict.

-- revision5 closure address/evidence validation (repository transaction mandatory):
-- record_source_closed uses original PreSourceProbe + genuine SourceClosureProof, no required Scope/setup.
-- It checks exact all-resource set and actual stored optional bindings, then atomically records source
-- and covered resource closure; no manufactured binding/wire/loadRPC and no old product_resources edits.
-- Owned before prepare uses actual owner shutdown and NotPrepared/BeforeAdoptRejected, no IO joined=true.
-- Source Bound / Resource Pre is valid cleanup input; bind confirmation unknown resolved by quiet+inspect.
-- UnloadedReservationAbandoned seals one target only and requires genuine NoTargetIo/ReservedZero evidence;
-- it neither marks Source closed nor kills another target, nor proves native idle or Run success.

-- Control transport facts are not Received and cannot mint a native grant.
-- One immutable row per actual operation phase; terminal can arrive before reserved.
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
        (control_kind IN ('native_cancel','permission_cancelled') AND intent_kind IN ('priority','shutdown') AND target_id IS NOT NULL AND owner_id IS NOT NULL AND load_epoch IS NOT NULL)),
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
