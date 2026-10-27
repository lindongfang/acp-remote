-- Frozen historical v18 schema fixture for migration tests.
-- Source: repository HEAD before the session-create-and-empty-cleanup WP2 changes.
-- Do not regenerate this file from the current OWNED_SCHEMA_V1 constant.
CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS owned_session (
  session_id        TEXT PRIMARY KEY,
  title             TEXT,
  agent_id          TEXT NOT NULL,
  agent_name        TEXT NOT NULL,
  state             TEXT NOT NULL CHECK (state IN ('idle','running','pending')),
  origin_epoch      TEXT NOT NULL,
  current_mode_id   TEXT,
  current_mode_name TEXT,
  version           INTEGER NOT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  agent_session_id  TEXT,                      -- Agent（ACP）侧会话标识；NULL = 该会话没有可用于恢复的标识，不得被补齐/推导
  workspace_cwd     TEXT,                      -- 创建时解析出的规范化绝对路径；NULL = 同上（不得按别名重解析来填上）
  workspace_directory_id TEXT,                 -- 创建时解析出的目录标识；NULL = 未分组（不得按 workspace_cwd 反查补齐）
  custom_title      TEXT                       -- 用户设置的自定义标题（trim 后 1..=512 标量值）；NULL = 从未设置。与 title 互不覆盖
) STRICT;

CREATE TABLE IF NOT EXISTS owned_turn (
  turn_id     TEXT PRIMARY KEY,
  session_id  TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  state       TEXT NOT NULL CHECK (state IN ('queued','running','waiting_input','waiting_permission','completed','failed','cancelled')),
  queue_index INTEGER NOT NULL,
  causation   TEXT,
  started_at  TEXT,
  ended_at    TEXT,
  UNIQUE (session_id, queue_index)
) STRICT;

CREATE TABLE IF NOT EXISTS owned_event (
  global_sequence  INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id       TEXT REFERENCES owned_session(session_id) ON DELETE CASCADE,
  session_sequence INTEGER,
  origin_epoch     TEXT,
  origin_sequence  INTEGER,
  event_id         TEXT NOT NULL UNIQUE,
  turn_id          TEXT REFERENCES owned_turn(turn_id),
  event_type       TEXT NOT NULL,
  kind             TEXT NOT NULL CHECK (kind IN ('state','delta','final_message','structured','interaction','summary')),
  policy           TEXT NOT NULL CHECK (policy IN ('durable','short_term')),
  origin_kind      TEXT NOT NULL CHECK (origin_kind IN ('agent','device','daemon','local_cli','node')),
  causation        TEXT,
  payload_json     TEXT NOT NULL,
  payload_digest   TEXT NOT NULL,
  acp_media_type   TEXT,
  acp_raw_json     TEXT,
  acp_byte_length  INTEGER,
  acp_sha256       TEXT,
  acp_raw_unavailable_reason TEXT CHECK (acp_raw_unavailable_reason IN ('size_limit','retention_expired','storage_failure')),
  created_at       TEXT NOT NULL,
  expires_at       TEXT,
  compacted_into   INTEGER,
  CHECK (session_id IS NOT NULL OR session_sequence IS NULL),
  CHECK ((session_id IS NULL) = (origin_epoch IS NULL)),
  CHECK ((session_id IS NULL) = (origin_sequence IS NULL)),
  CHECK (acp_raw_unavailable_reason IS NOT NULL
         OR (acp_raw_json IS NULL) = (acp_sha256 IS NULL)),
  CHECK (acp_raw_unavailable_reason IS NULL OR acp_raw_json IS NULL),
  CHECK (acp_raw_json IS NOT NULL OR acp_raw_unavailable_reason IS NOT NULL OR acp_sha256 IS NULL)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS owned_event_session ON owned_event(session_id, session_sequence);
CREATE UNIQUE INDEX IF NOT EXISTS owned_event_origin ON owned_event(session_id, origin_epoch, origin_sequence) WHERE session_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS owned_event_expiry ON owned_event(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS owned_event_turn ON owned_event(session_id, turn_id, kind);

CREATE TABLE IF NOT EXISTS owned_command (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  actor_kind    TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id      TEXT NOT NULL,
  request_id    TEXT NOT NULL,
  session_id    TEXT REFERENCES owned_session(session_id) ON DELETE CASCADE,
  command       TEXT NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('query','mutation')),
  expected_version TEXT,
  request_fingerprint TEXT NOT NULL,
  accepted_at   TEXT,
  status        TEXT NOT NULL CHECK (status IN ('accepted','completed','failed','rejected','uncertain')),
  terminal_at   TEXT,
  terminal_event_id TEXT,
  result_json   TEXT,
  error_code    TEXT, error_message TEXT, error_details_json TEXT, retryable INTEGER,
  UNIQUE (actor_kind, actor_id, request_id),
  CHECK (status = 'rejected' OR accepted_at IS NOT NULL),
  CHECK (status <> 'rejected' OR result_json IS NULL),
  CHECK (status <> 'completed' OR error_code IS NULL),
  CHECK (status IN ('accepted','completed') OR error_code IS NOT NULL),
  CHECK (kind <> 'query' OR terminal_event_id IS NULL),
  CHECK (status <> 'completed' OR kind <> 'mutation' OR terminal_event_id IS NOT NULL),
  CHECK (status NOT IN ('failed','uncertain') OR terminal_event_id IS NOT NULL)
) STRICT;
CREATE INDEX IF NOT EXISTS owned_command_status ON owned_command(status) WHERE status IN ('uncertain','accepted');
CREATE INDEX IF NOT EXISTS owned_command_session ON owned_command(session_id, accepted_at);

CREATE TABLE IF NOT EXISTS owned_interaction (
  interaction_id  TEXT PRIMARY KEY,
  session_id      TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  kind            TEXT NOT NULL CHECK (kind IN ('permission','elicitation')),
  request_event   INTEGER REFERENCES owned_event(global_sequence),
  state           TEXT NOT NULL CHECK (state IN ('pending','resolved','expired')),
  created_at      TEXT NOT NULL,
  resolved_at     TEXT,
  decision_option_id TEXT,
  decision_kind   TEXT CHECK (decision_kind IN ('allow_once','allow_always','reject_once','reject_always')),
  elicitation_action TEXT CHECK (elicitation_action IN ('submit','decline','cancel')),
  elicitation_values_json TEXT,
  resolved_by_kind TEXT, resolved_by_id TEXT,
  request_version INTEGER NOT NULL DEFAULT 1,
  invalidated_at  TEXT,
  confirmed_at    TEXT,                        -- 端点确认处理结果的时刻；NULL = 仍计入有效待处理
  turn_id         TEXT REFERENCES owned_turn(turn_id)  -- 所属 turn（由创建事件回填）；NULL = 历史行无法归属
) STRICT;
CREATE INDEX IF NOT EXISTS owned_interaction_pending ON owned_interaction(session_id, state) WHERE state = 'pending';

CREATE TABLE IF NOT EXISTS owned_audit (
  audit_id     INTEGER PRIMARY KEY AUTOINCREMENT,
  at           TEXT NOT NULL,
  action       TEXT NOT NULL CHECK (action IN (
                 'pairing.created','pairing.claimed','pairing.approved','pairing.rejected','pairing.expired',
                 'device.authenticated','device.auth_failed','device.revoked','device.scopes_changed',
                 'node.paired','node.authenticated','node.auth_failed','node.trust_revoked','node.identity_changed',
                 'export.created','export.revoked','import.added','import.removed','provider.configured',
                 'authorization.denied','rate_limit.triggered','storage.integrity_failed')),
  actor_kind   TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli','pairing_claimant')),
  actor_id     TEXT NOT NULL,
  via_node_id  TEXT,
  local_principal_ref TEXT,
  target_kind  TEXT NOT NULL, target_id TEXT NOT NULL,
  outcome      TEXT NOT NULL CHECK (outcome IN ('success','denied','failed')),
  detail_digest TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS owned_audit_at ON owned_audit(at);
CREATE INDEX IF NOT EXISTS owned_audit_action ON owned_audit(action, at);

CREATE TABLE IF NOT EXISTS owned_attachment (
  attachment_id TEXT NOT NULL UNIQUE,
  sha256      TEXT PRIMARY KEY,
  byte_length INTEGER NOT NULL,
  media_type  TEXT NOT NULL,
  relative_path TEXT NOT NULL,
  created_at  TEXT NOT NULL,
  last_used_at TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS owned_attachment_link (
  session_id   TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  attachment_id TEXT NOT NULL REFERENCES owned_attachment(attachment_id),
  generation   INTEGER NOT NULL,
  sha256       TEXT NOT NULL REFERENCES owned_attachment(sha256),
  PRIMARY KEY (session_id, attachment_id)
) STRICT;

CREATE TABLE IF NOT EXISTS owned_attachment_transfer (
  transfer_id       TEXT PRIMARY KEY,
  session_id        TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  subject_kind      TEXT NOT NULL CHECK (subject_kind IN ('device','node')),
  subject_id        TEXT NOT NULL,
  total_length      INTEGER NOT NULL,
  received_length   INTEGER NOT NULL DEFAULT 0,
  chunk_size        INTEGER NOT NULL CHECK (chunk_size = 32768),
  state             TEXT NOT NULL CHECK (state IN ('receiving','sealed','aborted','expired')),
  media_type        TEXT NOT NULL,
  file_name         TEXT NOT NULL,
  sealed_sha256     TEXT,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  CHECK ((state = 'sealed') = (sealed_sha256 IS NOT NULL))
) STRICT;
CREATE INDEX IF NOT EXISTS owned_attachment_transfer_stale
  ON owned_attachment_transfer(updated_at) WHERE state = 'receiving';

CREATE TABLE IF NOT EXISTS owned_attachment_submission (
  submission_id     TEXT PRIMARY KEY,
  session_id        TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  subject_kind      TEXT NOT NULL CHECK (subject_kind IN ('device','node')),
  subject_id        TEXT NOT NULL,
  state             TEXT NOT NULL CHECK (state IN ('open','sealed')),
  attachment_count  INTEGER NOT NULL,
  manifest_digest   TEXT,
  created_at        TEXT NOT NULL,
  sealed_at         TEXT,
  CHECK ((state = 'sealed') = (manifest_digest IS NOT NULL)),
  CHECK ((state = 'sealed') = (sealed_at IS NOT NULL))
) STRICT;

CREATE TABLE IF NOT EXISTS owned_attachment_submission_item (
  submission_id     TEXT NOT NULL REFERENCES owned_attachment_submission(submission_id) ON DELETE CASCADE,
  position          INTEGER NOT NULL,
  attachment_id     TEXT NOT NULL,
  PRIMARY KEY (submission_id, position)
) STRICT;

CREATE TABLE IF NOT EXISTS owned_message (
  message_id        TEXT PRIMARY KEY,
  session_id        TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  turn_id           TEXT,
  channel           TEXT NOT NULL CHECK (channel IN ('reply','thought','user')), -- 'user' = 已受理的用户输入（首条或后续普通输入）；不是 Agent 消息
  state             TEXT NOT NULL CHECK (state IN ('partial','completed','failed','cancelled')),
  content_json      TEXT NOT NULL,
  partial_reason    TEXT,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  first_sequence    INTEGER CHECK(first_sequence > 0),
  first_origin_sequence INTEGER CHECK(first_origin_sequence > 0),
  CHECK (state <> 'completed' OR partial_reason IS NULL)
) STRICT;
CREATE INDEX IF NOT EXISTS owned_message_session ON owned_message(session_id, created_at, message_id);

CREATE TABLE IF NOT EXISTS owned_timeline_anchor (
  session_id TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  entity_kind TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  first_sequence INTEGER NOT NULL CHECK(first_sequence > 0),
  first_origin_sequence INTEGER CHECK(first_origin_sequence > 0),
  PRIMARY KEY(session_id, entity_kind, entity_id)
) STRICT;

CREATE TABLE IF NOT EXISTS owned_media_ref (
  message_id        TEXT NOT NULL REFERENCES owned_message(message_id) ON DELETE CASCADE,
  position          INTEGER NOT NULL,
  attachment_id     TEXT NOT NULL,
  kind              TEXT NOT NULL CHECK (kind IN ('image','audio','file')),
  PRIMARY KEY (message_id, position)
) STRICT;
CREATE INDEX IF NOT EXISTS owned_media_ref_attachment ON owned_media_ref(attachment_id);

CREATE TABLE IF NOT EXISTS owned_first_submission (
  session_id          TEXT PRIMARY KEY REFERENCES owned_session(session_id) ON DELETE RESTRICT,
  first_submission_id TEXT NOT NULL UNIQUE,
  user_message_id     TEXT NOT NULL UNIQUE,
  first_request_id    TEXT NOT NULL,
  current_request_id  TEXT NOT NULL,
  actor_kind          TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id            TEXT NOT NULL,
  request_fingerprint TEXT NOT NULL,
  input_json          TEXT NOT NULL,
  input_digest        TEXT NOT NULL,
  mode_id             TEXT,
  config_json         TEXT NOT NULL,
  stage               TEXT NOT NULL CHECK (stage IN ('saved_not_dispatched','dispatch_started','terminal')),
  ever_dispatched     INTEGER NOT NULL DEFAULT 0 CHECK (ever_dispatched IN (0,1)),
  agent_session_id    TEXT,
  resource_epoch      TEXT,
  terminal_outcome    TEXT CHECK (terminal_outcome IN ('completed','failed','uncertain')),
  terminal_at         TEXT,
  error_code          TEXT, error_message TEXT, error_details_json TEXT, retryable INTEGER,
  created_at          TEXT NOT NULL,
  updated_at          TEXT NOT NULL,
  CHECK (stage = 'saved_not_dispatched' OR agent_session_id IS NOT NULL),
  CHECK (stage <> 'terminal' OR terminal_outcome IS NOT NULL),
  CHECK (ever_dispatched = 0 OR stage <> 'saved_not_dispatched'),
  CHECK (stage <> 'dispatch_started' OR ever_dispatched = 1),
  CHECK (terminal_outcome IS NULL OR terminal_outcome = 'completed' OR error_code IS NOT NULL)
) STRICT;

CREATE TABLE IF NOT EXISTS owned_user_input (
  message_id       TEXT PRIMARY KEY REFERENCES owned_message(message_id) ON DELETE CASCADE,
  session_id       TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  turn_id          TEXT NOT NULL REFERENCES owned_turn(turn_id) ON DELETE CASCADE,
  actor_kind       TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id         TEXT NOT NULL,
  request_id       TEXT NOT NULL,
  content_digest   TEXT NOT NULL,
  attachment_count INTEGER NOT NULL DEFAULT 0 CHECK (attachment_count >= 0),
  created_at       TEXT NOT NULL,
  UNIQUE (session_id, actor_kind, actor_id, request_id),
  UNIQUE (session_id, turn_id)
) STRICT;

CREATE TABLE IF NOT EXISTS owned_device (
  device_id     TEXT PRIMARY KEY,
  display_name  TEXT NOT NULL,
  public_key    BLOB NOT NULL,                 -- 65 字节 SEC1 未压缩 P-256
  fingerprint   TEXT NOT NULL,                 -- 64 字符小写 hex = SHA-256(public_key)
  state         TEXT NOT NULL CHECK (state IN ('pending','active','revoked')),
  created_at    TEXT NOT NULL,
  last_seen_at  TEXT,
  revoked_at    TEXT,
  revoke_reason TEXT CHECK (revoke_reason IN ('user_requested','key_changed','compromised')),
  CHECK (length(public_key) = 65),
  CHECK (length(fingerprint) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*'),
  CHECK ((state = 'revoked') = (revoked_at IS NOT NULL)),
  CHECK ((state = 'revoked') = (revoke_reason IS NOT NULL))
) STRICT;

CREATE TABLE IF NOT EXISTS owned_node (
  node_id        TEXT NOT NULL,
  kind           TEXT NOT NULL CHECK (kind IN ('access','owner')),
  display_name   TEXT NOT NULL,
  fingerprint    TEXT NOT NULL,
  state          TEXT NOT NULL CHECK (state IN ('pending','paired','revoked')),
  owner_endpoint TEXT,                         -- 仅 kind = 'owner' 非空
  created_at     TEXT NOT NULL,
  last_connected_at TEXT,
  revoked_at     TEXT,
  revoke_reason  TEXT CHECK (revoke_reason IN ('user_requested','key_changed','compromised')),
  PRIMARY KEY (node_id, kind),
  CHECK (length(fingerprint) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*'),
  CHECK ((kind = 'owner') = (owner_endpoint IS NOT NULL)),
  CHECK ((state = 'revoked') = (revoked_at IS NOT NULL)),
  CHECK ((state = 'revoked') = (revoke_reason IS NOT NULL))
) STRICT;
CREATE INDEX IF NOT EXISTS owned_node_role ON owned_node(kind, state);

-- Node 双角色共享一条身份材料：主键不含 kind。
CREATE TABLE IF NOT EXISTS owned_peer_key (
  peer_kind   TEXT NOT NULL CHECK (peer_kind IN ('device','node')),
  peer_id     TEXT NOT NULL,
  public_key  BLOB NOT NULL,
  fingerprint TEXT NOT NULL,
  bound_at    TEXT NOT NULL,
  PRIMARY KEY (peer_kind, peer_id),
  CHECK (length(public_key) = 65),
  CHECK (length(fingerprint) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*')
) STRICT;

-- 只存 pairing secret 的摘要与绑定；明文、HMAC、QR URL、完整认证 payload 都不落库。
CREATE TABLE IF NOT EXISTS owned_pairing (
  pairing_id          TEXT PRIMARY KEY,
  target_kind         TEXT NOT NULL CHECK (target_kind IN ('device','node')),
  state               TEXT NOT NULL CHECK (state IN ('created','claimed','pending_confirmation','approved','rejected','expired','consumed')),
  display_name        TEXT,
  secret_digest       TEXT NOT NULL,
  host_binding        TEXT NOT NULL,           -- 设备：canonical origin；节点：owner endpoint
  created_at          TEXT NOT NULL,
  expires_at          TEXT NOT NULL,
  claimed_at          TEXT,
  approved_at         TEXT,
  terminal_at         TEXT,
  CHECK ((state = 'created') = (claimed_at IS NULL)),
  CHECK ((state IN ('approved','consumed')) = (approved_at IS NOT NULL)),
  CHECK ((state IN ('rejected','expired','consumed')) = (terminal_at IS NOT NULL))
) STRICT;

-- 每个配对最多一个 peer；claim 之后不能换人（配对行条件更新与唯一主键共同保证）。
CREATE TABLE IF NOT EXISTS owned_pairing_peer (
  pairing_id   TEXT PRIMARY KEY REFERENCES owned_pairing(pairing_id) ON DELETE CASCADE,
  peer_kind    TEXT NOT NULL CHECK (peer_kind IN ('device','node')),
  peer_id      TEXT NOT NULL,
  display_name TEXT NOT NULL,
  public_key   BLOB NOT NULL,
  fingerprint  TEXT NOT NULL,
  client_nonce TEXT NOT NULL,
  claimed_at   TEXT NOT NULL,
  CHECK (length(public_key) = 65),
  CHECK (length(fingerprint) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*')
) STRICT;

-- 路径不进入 Node Link catalog（§11.1）：它只随 Sync 快照的 `workspaces` 条目下发（已记录目录的
-- 规范化路径），命令行日志、审计记录与错误 details 仍不含路径。
CREATE TABLE IF NOT EXISTS owned_directory (
  directory_id   TEXT PRIMARY KEY,
  canonical_path TEXT NOT NULL UNIQUE,
  created_at     TEXT NOT NULL,
  last_used_at   TEXT NOT NULL
) STRICT;

INSERT INTO meta (key, value) VALUES
  ('server_epoch', 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa'),
  ('created_at', '2026-10-26T00:00:00Z'),
  ('last_prune_at', '2026-10-26T00:00:00Z'),
  ('owned_schema_version', '18');
PRAGMA user_version = 18;
