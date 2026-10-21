//! `docs/CORE_PORTS_AND_STORAGE.md` §7.1/§7.2/§7.3/§7.4：文件布局、PRAGMA、连接模型、`owned_*`/
//! `imported_*` 表结构与 migration。
//!
//! 本模块只依赖 `sqlx` 与 `std`——不引用 `core` 的任何类型。表结构常量是合同 §7.3/§7.4 的逐字转录，
//! 改动必须同时改合同。migration 的键与值都以 `TEXT` 形式存在于 `meta`，时间值由调用方以
//! `&str` 传入（§2：`storage-sqlite` 内部不得读系统时间）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Connection, Executor, Row, Sqlite, SqliteConnection, Transaction};

use acp_core::model::derive_directory_id;

use crate::error::StorageError;

/// §7.2：`PRAGMA user_version` = 文件格式版本（v14：删除全部 `imported_*` 远程资源表并清理可辨认的
/// 远程代理记录；v13 的三态 `owned_session`、v12 的分块附件传输/清单、消息通道与媒体引用保持不变）。
pub const FILE_FORMAT_VERSION: i64 = 14;
/// §7.2：`meta.owned_schema_version` 的已知版本（v14 只删除 `imported_*` 家族，owned 表结构沿用 v13）。
pub const OWNED_SCHEMA_VERSION: i64 = 14;

/// §7.1：单文件 `<data_dir>/acp-remote.sqlite3`。
pub const DATABASE_FILE: &str = "acp-remote.sqlite3";
/// §7.1：附件目录默认 `<data_dir>/attachments`。
pub const ATTACHMENTS_DIR: &str = "attachments";

/// §7.2 的 `meta` 必需键（§7.3）。
pub const META_SERVER_EPOCH: &str = "server_epoch";
pub const META_OWNED_SCHEMA_VERSION: &str = "owned_schema_version";
pub const META_CREATED_AT: &str = "created_at";
pub const META_LAST_PRUNE_AT: &str = "last_prune_at";
/// 启动清理门禁已完成的文件格式版本：缺失或低于当前版本时，下一次启动必须重写数据库。
pub const META_STARTUP_CLEANUP_VERSION: &str = "startup_cleanup_version";
/// v9 → v10 删除 `owned_agent_profile` 时记下的行数；启动日志据此提示「Agent 需在配置中重新声明」。
pub const META_DISCARDED_AGENT_PROFILES: &str = "discarded_agent_profiles";

/// §7.1：`busy_timeout = 5000`（毫秒）。
pub const BUSY_TIMEOUT_MS: u64 = 5_000;
/// §7.1：`wal_autocheckpoint = 1000`（页）。
pub const WAL_AUTOCHECKPOINT_PAGES: u32 = 1_000;
/// 默认只读连接数。§7.1 只要求「1 写 + N 读」，N 的取值属实现细节。
pub const DEFAULT_READ_CONNECTIONS: u32 = 4;

/// §7.3 的 `owned_*` 表与索引（含 §7.3 的 `meta`）。
///
/// `IF NOT EXISTS` 是 migration 幂等的实现方式（§7.2：「可重复执行」）：已存在的表不会被重写，
/// 因此 `sqlite_master.sql` 的文本在第二次启动后逐字节不变（§9.1）。
pub const OWNED_SCHEMA_V1: &str = r#"
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
  workspace_directory_id TEXT                  -- 创建时解析出的目录标识；NULL = 未分组（不得按 workspace_cwd 反查补齐）
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
  channel           TEXT NOT NULL CHECK (channel IN ('reply','thought')),
  state             TEXT NOT NULL CHECK (state IN ('partial','completed','failed','cancelled')),
  content_json      TEXT NOT NULL,
  partial_reason    TEXT,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  CHECK (state <> 'completed' OR partial_reason IS NULL)
) STRICT;
CREATE INDEX IF NOT EXISTS owned_message_session ON owned_message(session_id, created_at, message_id);

CREATE TABLE IF NOT EXISTS owned_media_ref (
  message_id        TEXT NOT NULL REFERENCES owned_message(message_id) ON DELETE CASCADE,
  position          INTEGER NOT NULL,
  attachment_id     TEXT NOT NULL,
  kind              TEXT NOT NULL CHECK (kind IN ('image','audio','file')),
  PRIMARY KEY (message_id, position)
) STRICT;
CREATE INDEX IF NOT EXISTS owned_media_ref_attachment ON owned_media_ref(attachment_id);

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
"#;

/// §7.2 的 v1 → v2 升级：`owned_audit` 的 12-step 表重建（§11.8 第 7 条）。
///
/// SQLite 不能修改既有 CHECK，只能新建表 → 按列拷贝**全部行（含 `audit_id`）** → `DROP` 旧表 →
/// `RENAME` → 重建索引。本常量重建出的是 **v2 形状**（下一段 `V3_UPGRADE_OWNED` 再把它扩到 v3 形状的
/// CHECK）：v1 库必须能连续 `v1 → v2 → v3` 升级，两段各自保持可核算的目标形状。
///
/// `sqlite_sequence` 由 `migrate()` 在重建前后单独回填（`DROP TABLE` 会带走那一行，只按现存行的
/// `max(audit_id)` 回填会让已清理过尾部行的库序列回退）。
const V2_UPGRADE_OWNED: &str = r#"
CREATE TABLE owned_audit_v2 (
  audit_id     INTEGER PRIMARY KEY AUTOINCREMENT,
  at           TEXT NOT NULL,
  action       TEXT NOT NULL CHECK (action IN (
                 'pairing.created','pairing.claimed','pairing.approved','pairing.rejected','pairing.expired',
                 'device.authenticated','device.auth_failed','device.revoked','device.scopes_changed',
                 'node.paired','node.trust_revoked','node.identity_changed',
                 'export.created','export.revoked','import.added','import.removed','provider.configured',
                 'authorization.denied','rate_limit.triggered','storage.integrity_failed')),
  actor_kind   TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id     TEXT NOT NULL,
  via_node_id  TEXT,
  local_principal_ref TEXT,
  target_kind  TEXT NOT NULL, target_id TEXT NOT NULL,
  outcome      TEXT NOT NULL CHECK (outcome IN ('success','denied','failed')),
  detail_digest TEXT
) STRICT;

INSERT INTO owned_audit_v2 (audit_id, at, action, actor_kind, actor_id, via_node_id, local_principal_ref,
                            target_kind, target_id, outcome, detail_digest)
  SELECT audit_id, at, action, actor_kind, actor_id, via_node_id, local_principal_ref,
         target_kind, target_id, outcome, detail_digest FROM owned_audit;

DROP TABLE owned_audit;
ALTER TABLE owned_audit_v2 RENAME TO owned_audit;
CREATE INDEX IF NOT EXISTS owned_audit_at ON owned_audit(at);
CREATE INDEX IF NOT EXISTS owned_audit_action ON owned_audit(action, at);
"#;

/// §7.2 的 v1 → v2 升级：`imported_audit` 的 12-step 重建 + `imported_import` 的拆分迁移。
///
/// 顺序不可交换，原因有二：
/// - `imported_audit` 的重建与 `owned_audit` 同法（重建出的是 **v2 形状**的列与 CHECK，随后由
///   `V3_UPGRADE_IMPORTED` 扩到 v3 形状）；
/// - `imported_import` 被 `imported_import_export` 的外键引用，而 `foreign_keys = ON` 时 `DROP TABLE`
///   会先做隐式 DELETE 并按 `ON DELETE CASCADE` 删掉子行。因此先把原行里的 Export 关联搬进一张
///   **没有外键**的过渡表，再重建父表，最后才写进真正的关联表。
///
/// `grants_json` 一律写 `'[]'`：v1 没有可信的 grants 来源，**不得**凭空补齐或默认放权，这类 Import
/// 保持不可用，等本地重新授权（§11.8 第 3 条）。`added_at` 取原行的 `created_at`。
const V2_UPGRADE_IMPORTED: &str = r#"
CREATE TABLE imported_audit_v2 (
  audit_id     INTEGER PRIMARY KEY AUTOINCREMENT,
  at           TEXT NOT NULL,
  action       TEXT NOT NULL CHECK (action IN (
                 'pairing.created','pairing.claimed','pairing.approved','pairing.rejected','pairing.expired',
                 'device.authenticated','device.auth_failed','device.revoked','device.scopes_changed',
                 'node.paired','node.trust_revoked','node.identity_changed',
                 'export.created','export.revoked','import.added','import.removed','provider.configured',
                 'authorization.denied','rate_limit.triggered','storage.integrity_failed')),
  actor_kind   TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id     TEXT NOT NULL,
  owner_node_id TEXT, export_id TEXT, session_id TEXT,
  request_id   TEXT,
  local_principal_ref TEXT,
  target_kind  TEXT NOT NULL, target_id TEXT NOT NULL,
  outcome      TEXT NOT NULL CHECK (outcome IN ('success','denied','failed')),
  detail_digest TEXT
) STRICT;

INSERT INTO imported_audit_v2 (audit_id, at, action, actor_kind, actor_id, owner_node_id, export_id,
                               session_id, request_id, local_principal_ref, target_kind, target_id,
                               outcome, detail_digest)
  SELECT audit_id, at, action, actor_kind, actor_id, owner_node_id, export_id, session_id, request_id,
         local_principal_ref, target_kind, target_id, outcome, detail_digest FROM imported_audit;

DROP TABLE imported_audit;
ALTER TABLE imported_audit_v2 RENAME TO imported_audit;
CREATE INDEX IF NOT EXISTS imported_audit_at ON imported_audit(at);

CREATE TABLE imported_import_export_pending AS
  SELECT import_id, owner_node_id, export_id, created_at AS added_at FROM imported_import;

CREATE TABLE IF NOT EXISTS imported_import_export (
  import_id     TEXT NOT NULL REFERENCES imported_import(import_id) ON DELETE CASCADE,
  owner_node_id TEXT NOT NULL,
  export_id     TEXT NOT NULL,
  added_at      TEXT NOT NULL,
  PRIMARY KEY (import_id, export_id),
  UNIQUE (owner_node_id, export_id)
) STRICT;

CREATE TABLE imported_import_v2 (
  import_id     TEXT PRIMARY KEY,
  owner_node_id TEXT NOT NULL,
  display_name  TEXT,
  endpoint_ref  TEXT,
  cache_policy  TEXT NOT NULL CHECK (cache_policy = 'no-content-cache'),
  owner_server_epoch TEXT,
  created_at    TEXT NOT NULL,
  removed_at    TEXT,
  grants_json   TEXT NOT NULL
) STRICT;

INSERT INTO imported_import_v2 (import_id, owner_node_id, display_name, endpoint_ref, cache_policy,
                                owner_server_epoch, created_at, removed_at, grants_json)
  SELECT import_id, owner_node_id, display_name, endpoint_ref, cache_policy,
         owner_server_epoch, created_at, removed_at, '[]' FROM imported_import;

DROP TABLE imported_import;
ALTER TABLE imported_import_v2 RENAME TO imported_import;

INSERT INTO imported_import_export (import_id, owner_node_id, export_id, added_at)
  SELECT import_id, owner_node_id, export_id, added_at FROM imported_import_export_pending;
DROP TABLE imported_import_export_pending;
"#;

/// §7.2 的 v2 → v3 升级：`owned_audit` 的 12-step 表重建（与 v2 升级同款手法）。
///
/// 只有两处 CHECK 变化，列集合与列顺序**逐字不变**（因此 §9 判据 14 的黄金列清单期望值不动）：
/// `actor_kind` 增 `'pairing_claimant'`（配对认领方的审计归因），`action` 增
/// `'node.authenticated'`/`'node.auth_failed'`（节点握手留痕）。`owned_command` **不重建**：
/// 认领方永不提交命令，它的 `actor_kind` CHECK 保持三值（design D12）。
const V3_UPGRADE_OWNED: &str = r#"
CREATE TABLE owned_audit_v3 (
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

INSERT INTO owned_audit_v3 (audit_id, at, action, actor_kind, actor_id, via_node_id, local_principal_ref,
                            target_kind, target_id, outcome, detail_digest)
  SELECT audit_id, at, action, actor_kind, actor_id, via_node_id, local_principal_ref,
         target_kind, target_id, outcome, detail_digest FROM owned_audit;

DROP TABLE owned_audit;
ALTER TABLE owned_audit_v3 RENAME TO owned_audit;
CREATE INDEX IF NOT EXISTS owned_audit_at ON owned_audit(at);
CREATE INDEX IF NOT EXISTS owned_audit_action ON owned_audit(action, at);
"#;

/// §7.2 的 v2 → v3 升级：`imported_audit` 同款重建（两表的 CHECK 必须一起扩宽，否则「本机可写的
/// 动作在收到的审计元数据上写不进去」）。
const V3_UPGRADE_IMPORTED: &str = r#"
CREATE TABLE imported_audit_v3 (
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
  owner_node_id TEXT, export_id TEXT, session_id TEXT,
  request_id   TEXT,
  local_principal_ref TEXT,
  target_kind  TEXT NOT NULL, target_id TEXT NOT NULL,
  outcome      TEXT NOT NULL CHECK (outcome IN ('success','denied','failed')),
  detail_digest TEXT
) STRICT;

INSERT INTO imported_audit_v3 (audit_id, at, action, actor_kind, actor_id, owner_node_id, export_id,
                               session_id, request_id, local_principal_ref, target_kind, target_id,
                               outcome, detail_digest)
  SELECT audit_id, at, action, actor_kind, actor_id, owner_node_id, export_id, session_id, request_id,
         local_principal_ref, target_kind, target_id, outcome, detail_digest FROM imported_audit;

DROP TABLE imported_audit;
ALTER TABLE imported_audit_v3 RENAME TO imported_audit;
CREATE INDEX IF NOT EXISTS imported_audit_at ON imported_audit(at);
"#;

/// §7.2 的 v3 → v4 升级：`owned_node` 末尾追加 `export_ids_json`。
///
/// **只加列，不做 12-step 表重建**：重建的唯一理由是改既有 CHECK（v2 → v3 的先例），而本段不动任何
/// 约束。`ALTER TABLE ADD COLUMN` 也把列追加在末尾，因此升级库与新建库的 `pragma table_info` 列顺序
/// 逐项相等。整表 `NOT NULL` 由默认值满足，因此既有节点行得到 `'[]'`——即**不默认放权**（升级后这些
/// 配对在本机重新 `node pair confirm` 之前看不到任何 Export，连接与握手仍然正常）。
///
/// 本段不碰 `imported_*`（imported 家族版本保持 3），也不碰任何行与序号。
const V4_UPGRADE_OWNED: &str = r#"
ALTER TABLE owned_node ADD COLUMN export_ids_json TEXT NOT NULL DEFAULT '[]';
"#;

/// §7.2 的 v4 → v5 升级：`owned_session` 末尾追加 `agent_session_id` 与 `workspace_cwd` 两列。
///
/// **只加列，不做 12-step 表重建**（重建的唯一理由是改既有 CHECK，v2 → v3 的先例）：两列都可空、
/// **无默认值**，因此既有会话行得到 `NULL`——`NULL` 的语义是「该会话没有可用于恢复的标识或目录」，
/// 恢复必须显式失败，`MUST NOT` 被任何读取路径补全、推导或以别名解析结果替换。空串做不到这一点
/// （`NOT NULL DEFAULT ''` 会让「可恢复」判定被默认值蒙蔽），因此刻意不设默认值。
///
/// `ALTER TABLE ADD COLUMN` 把列追加在末尾，因此升级库与新建库的 `pragma table_info` 列顺序逐项相等
/// （§9 判据 18/28）。本段不碰 `imported_*`（imported 家族版本保持 3），也不碰任何行与序号。
const V5_UPGRADE_OWNED: &str = r#"
ALTER TABLE owned_session ADD COLUMN agent_session_id TEXT;
ALTER TABLE owned_session ADD COLUMN workspace_cwd TEXT;
"#;

/// §7.2 的 v5 → v6 升级：`owned_session` 末尾追加 `workspace_alias`（会话的目录归属）。
///
/// **只加列，不做 12-step 表重建**（重建的唯一理由是改既有 CHECK，v2 → v3 的先例）：一列可空、
/// **无默认值**，因此既有会话行得到 `NULL`——`NULL` 的语义是「该会话没有目录归属（未分组）」，
/// `MUST NOT` 被任何读取路径按 `workspace_cwd` 反查 `owned_workspace` 补齐，也 `MUST NOT` 用空串或
/// 占位别名代替（空串做不到这一点：它会让「已分组」判定被默认值蒙蔽，与 v5 两列同一理由）。
///
/// 该列与 v5 两列同属会话创建的那一次窄写提交（`CASE WHEN ?x IS NULL THEN col ELSE ?x END`），
/// 恢复流程既不读也不改它。`ALTER TABLE ADD COLUMN` 把列追加在末尾，因此升级库与新建库的
/// `pragma table_info` 列顺序逐项相等（§9 判据 18/28）。本段不碰 `imported_*`（imported 家族版本
/// 保持 3），也不碰任何行与序号。
const V6_UPGRADE_OWNED: &str = r#"
ALTER TABLE owned_session ADD COLUMN workspace_alias TEXT;
"#;

/// §7.2 的 v6 → v7 升级：`owned_event.origin_kind` 的 CHECK 从四值扩到五值（新增 `node`）。
///
/// SQLite 不能修改既有 CHECK，因此本段走 §11.8 第 7 条的 **12-step 表重建**（新建表 → 按列拷贝**全部
/// 行（含 `global_sequence`）** → `DROP` 旧表 → `RENAME` → 重建索引）。列集合与列顺序**逐字不变**，
/// 只有 `origin_kind` 的 CHECK 多一个 `'node'`。
///
/// **为什么要重建子表**：`owned_interaction.request_event` 是 `owned_event` 的子外键（`NO ACTION`），
/// 而 `foreign_keys = ON` 时 `DROP TABLE` 会先做隐式 DELETE 并检查该外键——子表有行就直接报
/// `FOREIGN KEY constraint failed`（实测）。官方 12-step 流程为此要求先关外键，但 `PRAGMA foreign_keys`
/// 在事务内是 no-op，而本迁移是**单事务、失败整体回滚**（§7.2）。因此按 v2 的 `imported_import` 同款手法
/// 把冲突挪开：先把子表行搬进一张**无外键**的过渡表并 `DROP` 子表（它是叶子，删除永远安全），再重建父表，
/// 最后把子表按**规范 DDL** 建回并回填行。过渡表在父表重建期间充当子表，因此两端都不会看到悬空引用；
/// `owned_interaction` 建回时用的就是 §7.3 的规范文本（含全部 CHECK 与那条外键），升级库与新建库因此
/// 逐列同形。
///
/// 事件表的 `AUTOINCREMENT` 序列（`sqlite_sequence` 的 `owned_event` 行）由 `migrate()` 在重建前后单独
/// 回填——`DROP TABLE` 会带走那一行，而只按现存行的 `max(global_sequence)` 回填会让「尾部行已被保留期
/// 清理」的库把 `global_sequence` 压回去（§7.2 的「不得重新编号」）。
const V7_UPGRADE_OWNED: &str = r#"
CREATE TABLE owned_event_v7 (
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

INSERT INTO owned_event_v7 (global_sequence, session_id, session_sequence, origin_epoch, origin_sequence,
                            event_id, turn_id, event_type, kind, policy, origin_kind, causation,
                            payload_json, payload_digest, acp_media_type, acp_raw_json, acp_byte_length,
                            acp_sha256, acp_raw_unavailable_reason, created_at, expires_at, compacted_into)
  SELECT global_sequence, session_id, session_sequence, origin_epoch, origin_sequence,
         event_id, turn_id, event_type, kind, policy, origin_kind, causation,
         payload_json, payload_digest, acp_media_type, acp_raw_json, acp_byte_length,
         acp_sha256, acp_raw_unavailable_reason, created_at, expires_at, compacted_into
  FROM owned_event;

CREATE TABLE owned_interaction_stage AS SELECT * FROM owned_interaction;
DROP TABLE owned_interaction;

DROP TABLE owned_event;
ALTER TABLE owned_event_v7 RENAME TO owned_event;
CREATE UNIQUE INDEX IF NOT EXISTS owned_event_session ON owned_event(session_id, session_sequence);
CREATE UNIQUE INDEX IF NOT EXISTS owned_event_origin ON owned_event(session_id, origin_epoch, origin_sequence) WHERE session_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS owned_event_expiry ON owned_event(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS owned_event_turn ON owned_event(session_id, turn_id, kind);

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
  resolved_by_kind TEXT, resolved_by_id TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS owned_interaction_pending ON owned_interaction(session_id, state) WHERE state = 'pending';

INSERT INTO owned_interaction (interaction_id, session_id, kind, request_event, state, created_at,
                               resolved_at, decision_option_id, decision_kind, elicitation_action,
                               elicitation_values_json, resolved_by_kind, resolved_by_id)
  SELECT interaction_id, session_id, kind, request_event, state, created_at,
         resolved_at, decision_option_id, decision_kind, elicitation_action,
         elicitation_values_json, resolved_by_kind, resolved_by_id
  FROM owned_interaction_stage;
DROP TABLE owned_interaction_stage;
"#;

/// §7.2 的 v7 → v8 升级：删除 `owned_device.scopes_json`（设备不再有授权集合）。
///
/// **只删一列，不做 12-step 表重建**：重建的唯一理由是改既有 CHECK（v2 → v3 的先例），而这一列既不在任何
/// CHECK / 索引 / 外键 / 生成列里，也没有表引用 `owned_device`，因此 `ALTER TABLE DROP COLUMN` 足够；
/// 它同样把列从表的列顺序里去掉，升级库与新建库的 `pragma table_info` 因此逐项相等（§9 判据 18/28）。
///
/// **为什么值直接丢弃而不是留空**：留存旧值等于让「设备权限」这一维度以数据形式继续存在——升级库里的
/// 旧 scope 集合不再有任何语义，却会被任何读原始库的人（含未来的读取路径）当成授权来源。设备记录不再
/// 有授权集合（`AGENTS.md` §4 的安全不变量），因此这里连同取值一起删除。
///
/// **本段不动任何审计 CHECK**：`device.scopes_changed` 作为**历史取值**保留在 `owned_audit`/
/// `imported_audit` 的 `action` CHECK 里——升级前的库可能已经有这类行，收缩 CHECK 会让这些行写不回去、
/// 读不出来（`core::model::AuditAction` 与 `docs/SECURITY_DESIGN.md` §14.2 同款保留并标注「不再产生」）。
///
/// 本段不碰 `imported_*`（imported 家族版本保持 3），也不碰任何行与序号。
const V8_UPGRADE_OWNED: &str = r#"
ALTER TABLE owned_device DROP COLUMN scopes_json;
"#;

/// §7.2 的 v8 → v9 升级：新增 `owned_directory`、把既有目录与既有会话归属回填、把
/// `owned_session.workspace_alias` 改名为 `workspace_directory_id`，最后删除 `owned_workspace`。
///
/// 目录标识由规范化路径派生（`derive_directory_id`），因此回填必须走 Rust：SQLite 没有 sha256。
/// 同一路径的多个别名合并成一条目录行（`directory_id` 相同）；会话的归属取该标识。
/// `workspace_alias` 为空但 `workspace_cwd` 非空的会话也登记目录并回填（历史孤儿会话因此获得分组），
/// 这些目录的 `created_at` 取会话创建时间、`last_used_at` 取本次升级时间。
///
/// 单事务、失败整体回滚；`ALTER TABLE ... RENAME COLUMN` 不触发 12-step 重建。
async fn v9_upgrade_owned(tx: &mut Transaction<'_, Sqlite>, at: &str) -> Result<(), StorageError> {
    if table_exists(&mut **tx, "owned_workspace").await? {
        let rows = sqlx::query(
            "SELECT alias, canonical_path, created_at, updated_at FROM owned_workspace",
        )
        .fetch_all(&mut **tx)
        .await?;
        for row in &rows {
            let canonical_path: String = row.try_get("canonical_path")?;
            let created_at: String = row.try_get("created_at")?;
            let updated_at: String = row.try_get("updated_at")?;
            let directory_id = derive_directory_id(&canonical_path);
            sqlx::query(
                "INSERT INTO owned_directory (directory_id, canonical_path, created_at, last_used_at) \
                 VALUES (?1, ?2, ?3, ?4) ON CONFLICT(directory_id) DO UPDATE SET \
                 created_at = MIN(owned_directory.created_at, excluded.created_at), \
                 last_used_at = MAX(owned_directory.last_used_at, excluded.last_used_at)",
            )
            .bind(directory_id.as_str())
            .bind(&canonical_path)
            .bind(&created_at)
            .bind(&updated_at)
            .execute(&mut **tx)
            .await?;
        }
        sqlx::query(
            "UPDATE owned_session SET workspace_alias = ( \
                 SELECT d.directory_id FROM owned_directory d \
                 JOIN owned_workspace w ON w.canonical_path = d.canonical_path \
                 WHERE w.alias = owned_session.workspace_alias) \
             WHERE workspace_alias IS NOT NULL",
        )
        .execute(&mut **tx)
        .await?;
    }

    let orphans = sqlx::query(
        "SELECT session_id, workspace_cwd, created_at FROM owned_session \
         WHERE workspace_alias IS NULL AND workspace_cwd IS NOT NULL",
    )
    .fetch_all(&mut **tx)
    .await?;
    for row in &orphans {
        let session_id: String = row.try_get("session_id")?;
        let cwd: String = row.try_get("workspace_cwd")?;
        let created_at: String = row.try_get("created_at")?;
        let directory_id = derive_directory_id(&cwd);
        sqlx::query(
            "INSERT INTO owned_directory (directory_id, canonical_path, created_at, last_used_at) \
             VALUES (?1, ?2, ?3, ?4) ON CONFLICT(directory_id) DO UPDATE SET \
             created_at = MIN(owned_directory.created_at, excluded.created_at), \
             last_used_at = MAX(owned_directory.last_used_at, excluded.last_used_at)",
        )
        .bind(directory_id.as_str())
        .bind(&cwd)
        .bind(&created_at)
        .bind(at)
        .execute(&mut **tx)
        .await?;
        sqlx::query("UPDATE owned_session SET workspace_alias = ?1 WHERE session_id = ?2")
            .bind(directory_id.as_str())
            .bind(&session_id)
            .execute(&mut **tx)
            .await?;
    }

    sqlx::raw_sql(
        "ALTER TABLE owned_session RENAME COLUMN workspace_alias TO workspace_directory_id;
         DROP TABLE IF EXISTS owned_workspace;",
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// §7.2 的 v11 → v12 升级：交互有效版本与失效标记。
///
/// 新表（`owned_attachment_transfer`/`owned_attachment_submission`/`owned_attachment_submission_item`/
/// `owned_message`/`owned_media_ref`）已由 [`OWNED_SCHEMA_V1`] 的 `CREATE ... IF NOT EXISTS` 建出，
/// 因此本段只给既有 `owned_interaction` 追加两列：
///
/// - `request_version INTEGER NOT NULL DEFAULT 1`（旧行得到 1：既有的 pending 请求仍有唯一版本）；
/// - `invalidated_at TEXT`（可空 = 尚未失效）。
///
/// 列存在性守卫让本段可重复执行：升级事务被重试、或某段已被人工应用时不会因重复列失败。
async fn v12_upgrade_owned(tx: &mut Transaction<'_, Sqlite>) -> Result<(), StorageError> {
    if !column_exists(&mut **tx, "owned_interaction", "request_version").await? {
        sqlx::raw_sql(
            "ALTER TABLE owned_interaction ADD COLUMN request_version INTEGER NOT NULL DEFAULT 1;",
        )
        .execute(&mut **tx)
        .await?;
    }
    if !column_exists(&mut **tx, "owned_interaction", "invalidated_at").await? {
        sqlx::raw_sql("ALTER TABLE owned_interaction ADD COLUMN invalidated_at TEXT;")
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// §7.2 的 v12 → v13 升级：会话摘要收敛为三态，交互补上确认阶段与 turn 归属。
///
/// 三件事在同一事务里完成，版本只在全部成功后才前进（失败整体回滚，重开可重试）：
///
/// 1. `owned_interaction` 追加 `confirmed_at`/`turn_id`。两列都可空，`turn_id` 用**创建它的交互事件**
///    的真实 `turn_id` 回填——事件是权威归属，绝不按时间相邻或会话内最近 turn 猜测；`request_event`
///    为空（历史行）时保持 `NULL`。旧的 `resolved` 行在旧模型里已经算「处理完毕」（它们不再计入旧摘要的
///    待处理数），因此回填 `confirmed_at = resolved_at`，避免升级后凭空多出一批「已仲裁未确认」。
/// 2. 重建 `owned_session`：删除 `closed_at`，把 `state` 的 CHECK 换成三态，并按旧七态做迁移暂存映射
///    （idle→idle、queued/running→running、waiting_input/waiting_permission→pending、failed/closed→idle）。
///    这里是**暂存值**：真正的三态由启动恢复按有效交互与未结束 turn 重算，迁移不得凭旧标签伪造任务结果。
/// 3. 归一化 `imported_session.state` 的旧七态（imported 表没有 CHECK，因此只改值，不重建、不加正文列）。
///
/// **为什么重建 `owned_session` 依赖事务外关闭外键**：它的每个子表都用 `ON DELETE CASCADE` 引用
/// `owned_session(session_id)`。外键开启时，`DROP TABLE` 旧表会级联删除全部子表行。`migrate()` 因此在
/// `BEGIN` **之前**关闭 `PRAGMA foreign_keys`（事务内该 pragma 是 no-op），迁移结束后恢复。
///
/// 重建顺序**不重命名旧表**，而是「建新表 → 拷贝 → DROP 旧表 → 把新表改名为规范名」：子表的外键
/// 文本始终指向 `owned_session` 这个名字，DROP 时外键检查关闭、不级联，改名时也没有任何引用指向
/// 临时表名，因此不会像「先重命名父表」那样把子表的 `REFERENCES` 改写成临时表名。
const V13_REBUILD_OWNED_SESSION: &str = r#"
CREATE TABLE owned_session_v13_new (
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
  agent_session_id  TEXT,
  workspace_cwd     TEXT,
  workspace_directory_id TEXT
) STRICT;

INSERT INTO owned_session_v13_new (session_id, title, agent_id, agent_name, state, origin_epoch,
                                   current_mode_id, current_mode_name, version, created_at,
                                   updated_at, agent_session_id, workspace_cwd,
                                   workspace_directory_id)
  SELECT session_id, title, agent_id, agent_name,
         CASE state
           WHEN 'queued' THEN 'running'
           WHEN 'running' THEN 'running'
           WHEN 'waiting_input' THEN 'pending'
           WHEN 'waiting_permission' THEN 'pending'
           ELSE 'idle'
         END,
         origin_epoch, current_mode_id, current_mode_name, version, created_at, updated_at,
         agent_session_id, workspace_cwd, workspace_directory_id
  FROM owned_session;

DROP TABLE owned_session;
ALTER TABLE owned_session_v13_new RENAME TO owned_session;
"#;

async fn v13_upgrade(tx: &mut Transaction<'_, Sqlite>) -> Result<(), StorageError> {
    // ---- 交互：确认阶段与 turn 归属
    if !column_exists(&mut **tx, "owned_interaction", "confirmed_at").await? {
        sqlx::raw_sql("ALTER TABLE owned_interaction ADD COLUMN confirmed_at TEXT;")
            .execute(&mut **tx)
            .await?;
    }
    if !column_exists(&mut **tx, "owned_interaction", "turn_id").await? {
        sqlx::raw_sql(
            "ALTER TABLE owned_interaction ADD COLUMN turn_id TEXT REFERENCES owned_turn(turn_id);",
        )
        .execute(&mut **tx)
        .await?;
    }
    // 归属只取创建交互的那条事件：`request_event` 是唯一可靠关联，不按时间相邻猜测。
    sqlx::query(
        "UPDATE owned_interaction SET turn_id = ( \
             SELECT e.turn_id FROM owned_event e \
             WHERE e.global_sequence = owned_interaction.request_event) \
         WHERE turn_id IS NULL AND request_event IS NOT NULL",
    )
    .execute(&mut **tx)
    .await?;
    // 旧 `resolved` 行在旧模型里已经退出待处理集合；升级后保持这个结论，不制造「未确认」。
    sqlx::query(
        "UPDATE owned_interaction SET confirmed_at = resolved_at \
         WHERE state = 'resolved' AND confirmed_at IS NULL",
    )
    .execute(&mut **tx)
    .await?;

    // ---- 会话：三态 CHECK + 删除关闭时间
    //
    // 父表的重建依赖 `migrate()` 在事务外关闭外键：这样 `RENAME` 不会改写子表的 `REFERENCES`
    // 子句，`DROP` 旧表也不会级联删除子表行（详见 [`migrate`] 的说明）。
    if column_exists(&mut **tx, "owned_session", "closed_at").await? {
        sqlx::raw_sql(V13_REBUILD_OWNED_SESSION)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

/// §7.2 的 v13 → v14 升级：删除全部 `imported_*` 远程资源表，并按**明确来源谓词**清理 owned 家族里
/// 可辨认的远程代理记录。
///
/// 顺序与判据（不可交换）：
/// 1. 先把 `imported_session.session_id` 固化成一张过渡表；**只**取不在 `owned_session` 里的标识——
///    两边都有的标识归属无法安全判定，迁移失败并给出本机诊断，不按时间/名称猜测；
/// 2. `owned_command.session_id` 指向这些远程标识的行删除（正常外键下不可能存在，但历史库可能在外键
///    关闭时写入过）；
/// 3. `owned_audit` 里目标明确指向这些远程标识的行删除：`target_kind = 'session'` 精确匹配、
///    `target_kind = 'command'` 的 `"{session}/{request}"` 前段匹配，以及已废除的 `import` 目标。
///    不按 turn/interaction 标识或时间猜测归属；
/// 4. 按外键依赖顺序 `DROP` 远程表（交付索引与命令引用先于会话），并删掉旧的 `meta.imported_schema_version`。
///
/// owned 会话、事件（含 `global_sequence` 序列）、epoch、命令幂等与节点信任一律不动。
async fn v14_upgrade(tx: &mut Transaction<'_, Sqlite>) -> Result<(), StorageError> {
    if table_exists(&mut **tx, "imported_session").await? {
        // 同一标识既在本机 owned 会话里、又在远程索引里：归属本身无法判定。只有确实存在
        // **指向该标识**的 owned 审计/命令行时才失败关闭；没有这类行时，删除远程索引不会误伤任何
        // 本机记录，可以继续（不得凭时间或名称猜测）。
        // `owned_command` 有指向 `owned_session` 的外键：标识两边都有时，那一行按外键定义就是本机
        // 记录，不需要判定；只有没有外键的 `owned_audit` 才可能无法归因。
        let ambiguous: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM owned_audit a \
             WHERE (a.target_kind = 'session' AND a.target_id IN (\
                      SELECT session_id FROM imported_session \
                      INTERSECT SELECT session_id FROM owned_session)) \
                OR (a.target_kind = 'command' AND instr(a.target_id, '/') > 0 \
                    AND substr(a.target_id, 1, instr(a.target_id, '/') - 1) IN (\
                        SELECT session_id FROM imported_session \
                        INTERSECT SELECT session_id FROM owned_session))",
        )
        .fetch_one(&mut **tx)
        .await?;
        if ambiguous > 0 {
            // 诊断只含计数：不得把远程会话标识或正文写进错误消息（§8）。
            return Err(StorageError::FailClosed(
                "a session id is both locally owned and remotely imported while owned audits reference it; ownership cannot be determined",
            ));
        }
        sqlx::raw_sql(V14_CLEAN_OWNED_PROXIES)
            .execute(&mut **tx)
            .await?;
        sqlx::raw_sql("DROP TABLE v14_remote_sessions;")
            .execute(&mut **tx)
            .await?;
    }
    sqlx::raw_sql(V14_DROP_IMPORTED).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM meta WHERE key = 'imported_schema_version'")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// v14：把可辨认的远程代理记录从 owned 家族清出（不触碰 owned 事件/epoch/幂等/信任）。
const V14_CLEAN_OWNED_PROXIES: &str = r#"
CREATE TABLE v14_remote_sessions AS
  SELECT session_id FROM imported_session
  WHERE session_id NOT IN (SELECT session_id FROM owned_session);

DELETE FROM owned_command
 WHERE session_id IS NOT NULL
   AND session_id IN (SELECT session_id FROM v14_remote_sessions);

DELETE FROM owned_audit
 WHERE (target_kind = 'session'
        AND target_id IN (SELECT session_id FROM v14_remote_sessions))
    OR (target_kind = 'command'
        AND instr(target_id, '/') > 0
        AND substr(target_id, 1, instr(target_id, '/') - 1)
            IN (SELECT session_id FROM v14_remote_sessions))
    OR target_kind = 'import';
"#;

/// v14：按外键依赖顺序删除全部远程资源表（`IF EXISTS` 让没有这些表的新库与已清理库也能走完）。
const V14_DROP_IMPORTED: &str = r#"
DROP TABLE IF EXISTS imported_delivery_index;
DROP TABLE IF EXISTS imported_command_ref;
DROP TABLE IF EXISTS imported_session;
DROP TABLE IF EXISTS imported_audit;
DROP TABLE IF EXISTS imported_import_export;
DROP TABLE IF EXISTS imported_import;
"#;

/// v10 升级前统计旧 `owned_agent_profile` 的行数；表不存在（新库或已升级）即 0。
async fn legacy_agent_profile_count(tx: &mut Transaction<'_, Sqlite>) -> Result<i64, StorageError> {
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'owned_agent_profile'",
    )
    .fetch_optional(&mut **tx)
    .await?;
    if exists.is_none() {
        return Ok(0);
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_agent_profile")
        .fetch_one(&mut **tx)
        .await?;
    Ok(count)
}

/// §7.2 的 v9 → v10 升级：删除 `owned_agent_profile` 与 `owned_provider_ref`（Agent 声明改为
/// 启动配置的唯一权威，Provider 凭据不再由本系统持有）。审计行保留；升级不搬运数据。
const V10_UPGRADE_OWNED: &str = r#"
DROP TABLE IF EXISTS owned_agent_profile;
DROP TABLE IF EXISTS owned_provider_ref;
"#;

/// §7.2 的 v10 → v11 升级：删除 Export/Import 与节点授权列，imported 家族按 Owner 节点寻址。
///
/// 迁移规则（单事务，失败整体回滚）：
/// - 删除 `owned_export`、`imported_import`、`imported_import_export`；
/// - 12-step 重建 `owned_node`（去掉 `grants_json`/`export_ids_json`）；
/// - 重建 imported 三表：主键改为 `(owner_node_id, session_id)`。同一 Owner 下跨 Export 的重复会话合并，
///   保留最新 origin cursor / `updated_at` 的那一行，`next_local_sequence` 取组内最大值（不回退）；
///   交付索引与命令引用只保留获胜 Export 的行，因此不会产生孤儿索引。
async fn v11_upgrade_storage(tx: &mut Transaction<'_, Sqlite>) -> Result<(), StorageError> {
    // owned_node：去掉 grants_json/export_ids_json（v3/v4 等历史形状可能只有其中一列）。
    if column_exists(&mut **tx, "owned_node", "grants_json").await?
        || column_exists(&mut **tx, "owned_node", "export_ids_json").await?
    {
        sqlx::raw_sql(
            r#"
CREATE TABLE owned_node_v11 (
  node_id        TEXT NOT NULL,
  kind           TEXT NOT NULL CHECK (kind IN ('access','owner')),
  display_name   TEXT NOT NULL,
  fingerprint    TEXT NOT NULL,
  state          TEXT NOT NULL CHECK (state IN ('pending','paired','revoked')),
  owner_endpoint TEXT,
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
INSERT INTO owned_node_v11 (node_id, kind, display_name, fingerprint, state, owner_endpoint,
                            created_at, last_connected_at, revoked_at, revoke_reason)
  SELECT node_id, kind, display_name, fingerprint, state, owner_endpoint,
         created_at, last_connected_at, revoked_at, revoke_reason FROM owned_node;
DROP TABLE owned_node;
ALTER TABLE owned_node_v11 RENAME TO owned_node;
CREATE INDEX IF NOT EXISTS owned_node_role ON owned_node(kind, state);
"#,
        )
        .execute(&mut **tx)
        .await?;
    }

    // imported 家族：只有在还是带 export_id 的旧形状时才重建。
    if column_exists(&mut **tx, "imported_session", "export_id").await? {
        sqlx::raw_sql(
            r#"
CREATE TABLE imported_session_v11 (
  owner_node_id  TEXT NOT NULL,
  session_id     TEXT NOT NULL,
  title          TEXT,
  agent_id       TEXT, agent_name TEXT,
  state          TEXT,
  version        INTEGER,
  created_at     TEXT,
  last_origin_epoch   TEXT,
  last_origin_sequence INTEGER,
  acked_origin_epoch  TEXT,
  acked_origin_sequence INTEGER,
  next_local_sequence INTEGER NOT NULL DEFAULT 1,
  attachment_id TEXT,
  attachment_generation INTEGER,
  updated_at     TEXT NOT NULL,
  PRIMARY KEY (owner_node_id, session_id)
) STRICT;

CREATE TABLE imported_winner AS
  SELECT owner_node_id, export_id, session_id FROM (
    SELECT owner_node_id, export_id, session_id,
      ROW_NUMBER() OVER (
        PARTITION BY owner_node_id, session_id
        ORDER BY last_origin_sequence DESC, updated_at DESC, export_id ASC
      ) AS rn
    FROM imported_session
  ) WHERE rn = 1;

INSERT INTO imported_session_v11 (
  owner_node_id, session_id, title, agent_id, agent_name, state, version, created_at,
  last_origin_epoch, last_origin_sequence, acked_origin_epoch, acked_origin_sequence,
  next_local_sequence, attachment_id, attachment_generation, updated_at)
  SELECT s.owner_node_id, s.session_id, s.title, s.agent_id, s.agent_name, s.state, s.version,
         s.created_at, s.last_origin_epoch, s.last_origin_sequence, s.acked_origin_epoch,
         s.acked_origin_sequence,
         (SELECT MAX(x.next_local_sequence) FROM imported_session x
           WHERE x.owner_node_id = s.owner_node_id AND x.session_id = s.session_id),
         s.attachment_id, s.attachment_generation, s.updated_at
  FROM imported_session s
  JOIN imported_winner w
    ON w.owner_node_id = s.owner_node_id AND w.export_id = s.export_id
   AND w.session_id = s.session_id;

CREATE TABLE imported_delivery_index_saved AS
  SELECT d.owner_node_id, d.session_id, d.origin_event_id, d.origin_epoch, d.origin_sequence,
         d.local_sequence, d.event_type, d.payload_digest, d.received_at
  FROM imported_delivery_index d
  JOIN imported_winner w
    ON w.owner_node_id = d.owner_node_id AND w.export_id = d.export_id
   AND w.session_id = d.session_id;

CREATE TABLE imported_command_ref_saved AS
  SELECT c.owner_node_id, c.session_id, c.request_id, c.command, c.status, c.accepted_at,
         c.terminal_at, c.terminal_event_id, c.error_code, c.retryable
  FROM imported_command_ref c
  JOIN imported_winner w
    ON w.owner_node_id = c.owner_node_id AND w.export_id = c.export_id
   AND w.session_id = c.session_id;

DROP TABLE imported_delivery_index;
DROP TABLE imported_command_ref;
DROP TABLE imported_session;
ALTER TABLE imported_session_v11 RENAME TO imported_session;

CREATE TABLE imported_delivery_index (
  owner_node_id   TEXT NOT NULL,
  session_id      TEXT NOT NULL,
  origin_event_id TEXT NOT NULL,
  origin_epoch    TEXT NOT NULL,
  origin_sequence INTEGER NOT NULL,
  local_sequence  INTEGER NOT NULL,
  event_type      TEXT NOT NULL,
  payload_digest  TEXT NOT NULL,
  received_at     TEXT NOT NULL,
  PRIMARY KEY (owner_node_id, session_id, origin_event_id),
  UNIQUE (owner_node_id, session_id, local_sequence),
  UNIQUE (owner_node_id, session_id, origin_epoch, origin_sequence),
  FOREIGN KEY (owner_node_id, session_id)
    REFERENCES imported_session(owner_node_id, session_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE imported_command_ref (
  owner_node_id  TEXT NOT NULL, session_id TEXT NOT NULL,
  request_id     TEXT NOT NULL,
  command        TEXT NOT NULL,
  status         TEXT NOT NULL CHECK (status IN ('accepted','completed','failed','rejected','uncertain')),
  accepted_at    TEXT,
  terminal_at    TEXT, terminal_event_id TEXT,
  error_code     TEXT, retryable INTEGER,
  PRIMARY KEY (owner_node_id, session_id, request_id),
  CHECK (status = 'rejected' OR accepted_at IS NOT NULL),
  FOREIGN KEY (owner_node_id, session_id)
    REFERENCES imported_session(owner_node_id, session_id) ON DELETE CASCADE
) STRICT;

INSERT INTO imported_delivery_index
  SELECT owner_node_id, session_id, origin_event_id, origin_epoch, origin_sequence,
         local_sequence, event_type, payload_digest, received_at
  FROM imported_delivery_index_saved;
INSERT INTO imported_command_ref
  SELECT owner_node_id, session_id, request_id, command, status, accepted_at,
         terminal_at, terminal_event_id, error_code, retryable
  FROM imported_command_ref_saved;
DROP TABLE imported_delivery_index_saved;
DROP TABLE imported_command_ref_saved;
DROP TABLE imported_winner;
"#,
        )
        .execute(&mut **tx)
        .await?;
    }

    sqlx::raw_sql(
        "DROP TABLE IF EXISTS imported_import_export;\nDROP TABLE IF EXISTS imported_import;\nDROP TABLE IF EXISTS owned_export;",
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// §7.5 的存储配置键。默认值逐项对应合同表格。
///
/// 配置加载属 `app`；本结构只承载 `storage-sqlite` 需要知道的部分（`storage.flush_interval_ms` 不在
/// 其中：它是 broker 的 delta 合并窗口，§6 第 10 条，存储层不参与）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
    pub attachment_dir: PathBuf,
    /// §7.1 的 N 个只读连接。
    pub read_connections: u32,
    /// `storage.transcript_retention_days`（90）。
    pub transcript_retention_days: u32,
    /// `storage.sync_event_retention_days`（7）。
    pub sync_event_retention_days: u32,
    /// `storage.audit_retention_days`（365）。
    pub audit_retention_days: u32,
    /// `storage.max_total_size_bytes`（2 GiB）。
    pub max_total_size_bytes: u64,
    /// `storage.max_session_size_bytes`（100 MiB）。
    pub max_session_size_bytes: u64,
    /// `storage.persist_deltas`（false）。
    pub persist_deltas: bool,
    /// `storage.attachment_max_file_bytes`（20 MiB）。
    pub attachment_max_file_bytes: u64,
    /// `storage.attachment_max_total_bytes`（1 GiB）。
    pub attachment_max_total_bytes: u64,
    /// §7.1：正式模式下的权限检查失败关闭。
    pub strict_permissions: bool,
}

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

impl StorageConfig {
    /// 以 `<data_dir>` 为根，其余键取 §7.5 的默认值。
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        let attachment_dir = data_dir.join(ATTACHMENTS_DIR);
        StorageConfig {
            data_dir,
            attachment_dir,
            read_connections: DEFAULT_READ_CONNECTIONS,
            transcript_retention_days: 90,
            sync_event_retention_days: 7,
            audit_retention_days: 365,
            max_total_size_bytes: 2 * GIB,
            max_session_size_bytes: 100 * MIB,
            persist_deltas: false,
            attachment_max_file_bytes: 20 * MIB,
            attachment_max_total_bytes: GIB,
            strict_permissions: true,
        }
    }

    /// §7.1：单文件数据库路径。
    pub fn database_path(&self) -> PathBuf {
        self.data_dir.join(DATABASE_FILE)
    }
}

/// migration 之后 `meta` 中可用的库级事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreMetadata {
    /// §3.2 `ServerEpoch`：事件库首次创建时生成，升级与重启保持不变。
    pub server_epoch: String,
    pub owned_schema_version: i64,
    pub created_at: String,
    pub last_prune_at: String,
}

/// 一次 `open` 的两个连接池：1 个写连接（`max_connections = 1`，串行化写事务）＋ N 个只读连接。
#[derive(Debug, Clone)]
pub struct Pools {
    pub write: SqlitePool,
    pub read: SqlitePool,
}

impl Pools {
    pub async fn close(self) {
        self.read.close().await;
        self.write.close().await;
    }
}

/// §7.1：打开连接池。目录与文件权限由本函数一并落实；`strict_permissions` 决定宽松权限是否失败关闭。
pub async fn open_pools(config: &StorageConfig) -> Result<Pools, StorageError> {
    prepare_data_dir(config)?;
    let write = open_write_pool(config).await?;
    tighten_database_files(config)?;
    let read = open_read_pool(config).await?;
    Ok(Pools { write, read })
}

/// §7.1：数据库/WAL/`-shm` 目标模式 `0600`。SQLite 按 umask 创建这些文件，所以打开后主动收紧。
fn tighten_database_files(config: &StorageConfig) -> Result<(), StorageError> {
    let base = config.database_path();
    let base = base.as_os_str().to_string_lossy().into_owned();
    for suffix in ["", "-wal", "-shm", "-journal"] {
        tighten_file_permissions(Path::new(&format!("{base}{suffix}")))?;
    }
    Ok(())
}

async fn open_write_pool(config: &StorageConfig) -> Result<SqlitePool, StorageError> {
    let options = SqliteConnectOptions::new()
        .filename(config.database_path())
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true)
        .busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))
        .pragma("wal_autocheckpoint", WAL_AUTOCHECKPOINT_PAGES.to_string());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    Ok(pool)
}

async fn open_read_pool(config: &StorageConfig) -> Result<SqlitePool, StorageError> {
    // 只读连接不设 `journal_mode`（SQLite 拒绝在只读句柄上切换日志模式），也不 `create_if_missing`。
    let options = SqliteConnectOptions::new()
        .filename(config.database_path())
        .read_only(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS));
    let pool = SqlitePoolOptions::new()
        .max_connections(config.read_connections)
        .connect_with(options)
        .await?;
    Ok(pool)
}

/// §7.1：数据目录与附件目录按 `0700` 创建；Unix 上同时收紧已存在文件的模式。
fn prepare_data_dir(config: &StorageConfig) -> Result<(), StorageError> {
    create_dir(&config.data_dir)?;
    create_dir(&config.attachment_dir)?;
    for path in [&config.data_dir, &config.attachment_dir] {
        if config.strict_permissions {
            enforce_path_permissions(path)?;
        }
    }
    Ok(())
}

fn create_dir(path: &Path) -> Result<(), StorageError> {
    if path.is_dir() {
        return Ok(());
    }
    if path.exists() {
        return Err(StorageError::PathUnusable {
            path: path.display().to_string(),
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// §7.1：数据库/WAL/`-shm` 与附件文件的目标模式是 `0600`。
#[cfg(unix)]
pub fn tighten_file_permissions(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;
    if !path.exists() {
        return Ok(());
    }
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub fn tighten_file_permissions(_path: &Path) -> Result<(), StorageError> {
    // Windows：文件继承其所在目录的 ACL；§13.2 要求的「当前用户专属 ACL」由 §7.1 的目录检查兜底。
    Ok(())
}

/// 合成权限视图（§9.12）：平台 ACL 读取被抽象成纯函数，判定可单测。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PermissionView {
    pub group_read: bool,
    pub group_write: bool,
    pub other_read: bool,
    pub other_write: bool,
    /// 存在非当前用户的显式 ACL 条目（Windows ACL 视图）。
    pub foreign_principal_read: bool,
    pub foreign_principal_write: bool,
}

impl PermissionView {
    /// §9.12：宽松判定。目标状态是「只允许当前 OS 用户访问」（§13.2），因此除所有者位以外的任何
    /// 读/写授予都算宽松。
    pub fn is_relaxed(self) -> bool {
        self.group_read
            || self.group_write
            || self.other_read
            || self.other_write
            || self.foreign_principal_read
            || self.foreign_principal_write
    }

    /// Unix 模式位 → 视图：`group`/`other` 的读或写位都算宽松。
    pub fn from_unix_mode(mode: u32) -> Self {
        PermissionView {
            group_read: mode & 0o040 != 0,
            group_write: mode & 0o020 != 0,
            other_read: mode & 0o004 != 0,
            other_write: mode & 0o002 != 0,
            foreign_principal_read: false,
            foreign_principal_write: false,
        }
    }
}

/// 一次权限检查的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionCheck {
    /// 证据表明只有所有者可访问。
    OwnerOnly,
    /// 证据表明存在额外的读/写授予。
    Relaxed(PermissionView),
    /// 平台无法在不引入不安全代码的前提下读取 ACL。
    Unverifiable,
}

/// 读取路径的权限证据。
///
/// Windows 下读取 ACL 需要 `GetNamedSecurityInfoW` 一类的 API，而 workspace 禁止 `unsafe`
/// （`[workspace.lints.rust] unsafe_code = "forbid"`）且本项目未依赖任何 ACL 封装 crate，因此
/// Windows 返回 [`PermissionCheck::Unverifiable`]：正式模式不会因此失败关闭（§7.1 的失败关闭针对
/// **已经观察到**的宽松权限），数据库位于用户目录时其 ACL 由平台默认继承为当前用户专属。
#[cfg(unix)]
pub fn inspect_path_permissions(path: &Path) -> Result<PermissionCheck, StorageError> {
    use std::os::unix::fs::MetadataExt;
    let mode = std::fs::metadata(path)?.mode();
    let view = PermissionView::from_unix_mode(mode);
    Ok(if view.is_relaxed() {
        PermissionCheck::Relaxed(view)
    } else {
        PermissionCheck::OwnerOnly
    })
}

#[cfg(not(unix))]
pub fn inspect_path_permissions(_path: &Path) -> Result<PermissionCheck, StorageError> {
    Ok(PermissionCheck::Unverifiable)
}

/// §7.1/§9.12：正式模式下宽松即失败关闭。
pub fn enforce_path_permissions(path: &Path) -> Result<(), StorageError> {
    match inspect_path_permissions(path)? {
        PermissionCheck::Relaxed(_) => Err(StorageError::InsecurePermissions {
            path: path.display().to_string(),
        }),
        PermissionCheck::OwnerOnly | PermissionCheck::Unverifiable => Ok(()),
    }
}

/// §7.1：`PRAGMA quick_check`；失败即进入只读失败关闭。
pub async fn quick_check(pool: &SqlitePool) -> Result<(), StorageError> {
    let results: Vec<String> = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_all(pool)
        .await?;
    expect_ok(results)
}

/// 完整一致性检查；崩溃恢复判据用它（§9.2c）。
pub async fn integrity_check(pool: &SqlitePool) -> Result<(), StorageError> {
    let results: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_all(pool)
        .await?;
    expect_ok(results)
}

fn expect_ok(results: Vec<String>) -> Result<(), StorageError> {
    if results.len() == 1 && results.first().is_some_and(|row| row == "ok") {
        return Ok(());
    }
    // §8：错误消息不得包含正文。`quick_check`/`integrity_check` 的输出是页/索引结构描述，不含行内容，
    // 这里只保留固定文案。
    Err(StorageError::Corrupt(
        "sqlite integrity check did not return ok",
    ))
}

/// §7.2：执行 migration。单事务、幂等、失败整体回滚、版本过新拒绝启动。
///
/// 单事务的边界是 `BEGIN IMMEDIATE`：写池只有 1 个连接，但 `IMMEDIATE` 让「读版本 → 建表 → 升级 →
/// 写版本」在拿锁后整体执行，避免与外部进程的写事务在升级锁时冲突。
///
/// 升级判据 =「升级前已有 schema」+ `user_version < FILE_FORMAT_VERSION`：版本号与表结构在同一个
/// 事务里落盘，所以这两个条件一起出现就等价于「库是旧形状」。新建库由两个 DDL 常量直接建成当前形状
/// （`user_version` 此时是 0，不能只按版本判断）；已经是最新版的库**跳过**全部升级步骤，因此第二次
/// 打开不产生任何 DDL、行级或版本写入（§9.1 的逐字节幂等）。
///
/// 事务内每段升级另有自己的版本守卫（`file_version < N`，见函数体），因此老库连续升级、而「已经比某段
/// 新」的库不会重跑那一段的 12-step 重建。v4/v8 两段另外只作用于**升级前就存在**的表（`owned_node`/
/// `owned_device`；v1 库没有这两张表，它们由 DDL 常量直接建成当前形状）；v5/v6 两段的追加对象
/// `owned_session` 与 v7 段的重建对象 `owned_event` 是「升级库 vs 新建库」的判据本身，走升级分支即必然
/// 已存在（v1 起每代都有这两张表）。
/// §7.2：执行 migration。单事务、幂等、失败整体回滚、版本过新拒绝启动。
///
/// 12-step 表重建会 `DROP` 被 `ON DELETE CASCADE` 子表引用的父表（`owned_session` → `owned_turn`/
/// `owned_event`/…）。`foreign_keys` 在事务内是 no-op，因此本函数先在**事务外**把写连接的外键检查
/// 关掉，再在同一个连接上开 `BEGIN IMMEDIATE`，结束后无论成败都恢复为 `ON`（连接会回到池里）。
/// 标准 12-step 流程正是这么做的：关外键时 `ALTER TABLE ... RENAME` 也不会改写子表的 `REFERENCES`
/// 子句，旧表因此可以被安全删除，重建后的新表仍以原名被引用。
pub async fn migrate(write: &SqlitePool, at: &str) -> Result<StoreMetadata, StorageError> {
    let mut connection = write.acquire().await?;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *connection)
        .await?;
    let result = migrate_locked(&mut connection, at).await;
    // 无论升级成功与否都复位：失败路径也要把连接完好地还给池。
    let restored = sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *connection)
        .await;
    match (result, restored) {
        (Ok(metadata), Ok(_)) => Ok(metadata),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
    }
}

async fn migrate_locked(
    connection: &mut SqliteConnection,
    at: &str,
) -> Result<StoreMetadata, StorageError> {
    let mut tx = connection.begin_with("BEGIN IMMEDIATE").await?;

    let file_version = read_user_version(&mut *tx).await?;
    if file_version > FILE_FORMAT_VERSION {
        // 拒绝启动，且事务内没有任何写入（§7.2、§9.1）：这一条必须早于任何 DDL。
        return Err(StorageError::FileFormatTooNew {
            found: file_version,
            supported: FILE_FORMAT_VERSION,
        });
    }
    let new_database = !table_exists(&mut *tx, "owned_session").await?;
    // v4 段只给**升级前就存在**的 `owned_node` 追加列、v8 段只从**升级前就存在**的 `owned_device` 删列：
    // v1 库没有这两张表，它们由下面的 DDL 常量按**当前形状**建出来（已含 `export_ids_json`、已不含
    // `scopes_json`），那时再 `ALTER TABLE` 就是重复列或缺失列。
    let owned_node_pre_existing = table_exists(&mut *tx, "owned_node").await?;
    let owned_device_pre_existing = table_exists(&mut *tx, "owned_device").await?;
    // v5/v6 两段的追加对象是 `owned_session`，而它正是「库是升级库还是新建库」的判据：走升级分支就说明它
    // 在 DDL 常量执行前已经存在（v1 起每代都有这张表），因此这两段不再单独探测。

    sqlx::raw_sql(OWNED_SCHEMA_V1).execute(&mut *tx).await?;

    let owned_version = read_meta_row(&mut *tx, META_OWNED_SCHEMA_VERSION)
        .await?
        .map(|value| parse_version(&value, "meta.owned_schema_version"))
        .transpose()?
        .unwrap_or(OWNED_SCHEMA_VERSION);
    if owned_version > OWNED_SCHEMA_VERSION {
        return Err(StorageError::SchemaTooNew {
            found: owned_version,
            supported: OWNED_SCHEMA_VERSION,
        });
    }

    if !new_database && file_version < FILE_FORMAT_VERSION {
        // v1 → v2 → v3 → v4 → v5 → v6 → v7 → v8（§7.2 的连续升级）：两张审计表的 CHECK 扩宽、`imported_import`
        // 的拆分、`owned_event.origin_kind` 的扩宽与 `owned_device` 的删列必须在同一个事务里完成，否则会留下
        // 「新建库可写新取值、升级库不可写」或「列形状不一致」的状态（§11.8 第 7 条）。SQLite 不能修改既有
        // CHECK，因此 v2/v3 两段与 v7 段都是 12-step 表重建，且把全部行（含 `audit_id`/`global_sequence`）
        // 一起搬过去；v4 段只往 `owned_node` 加列、v5/v6 两段只往 `owned_session` 加列、v8 段只从
        // `owned_device` 删一列（都不重建）。
        //
        // 每段都有自己的版本守卫（`file_version < N`）：升级判据是 `file_version < FILE_FORMAT_VERSION`，
        // 不守卫的段会在「文件格式已经比该段新、但仍低于最新版」时重复执行那些 12-step 重建——例如
        // 常量推进到 4 之后，一个 v3 库会重跑 v3 段（多余且会重写 `sqlite_master`）。
        //
        // 序列在**全部**重建之前取一次、之后回填一次：每段重建的 `DROP TABLE` 都会带走
        // `sqlite_sequence` 里的那一行，分段回填会让前一段的回填结果被后一段再次丢掉。
        let sequences = autoincrement_sequences(&mut *tx).await?;
        if file_version < 2 {
            sqlx::raw_sql(V2_UPGRADE_OWNED).execute(&mut *tx).await?;
            sqlx::raw_sql(V2_UPGRADE_IMPORTED).execute(&mut *tx).await?;
        }
        if file_version < 3 {
            sqlx::raw_sql(V3_UPGRADE_OWNED).execute(&mut *tx).await?;
            sqlx::raw_sql(V3_UPGRADE_IMPORTED).execute(&mut *tx).await?;
        }
        if file_version < 4 && owned_node_pre_existing {
            sqlx::raw_sql(V4_UPGRADE_OWNED).execute(&mut *tx).await?;
        }
        if file_version < 5 {
            sqlx::raw_sql(V5_UPGRADE_OWNED).execute(&mut *tx).await?;
        }
        if file_version < 6 {
            sqlx::raw_sql(V6_UPGRADE_OWNED).execute(&mut *tx).await?;
        }
        if file_version < 7 {
            sqlx::raw_sql(V7_UPGRADE_OWNED).execute(&mut *tx).await?;
        }
        if file_version < 8 && owned_device_pre_existing {
            sqlx::raw_sql(V8_UPGRADE_OWNED).execute(&mut *tx).await?;
        }
        if file_version < 9 {
            v9_upgrade_owned(&mut tx, at).await?;
        }
        if file_version < 10 {
            let discarded = legacy_agent_profile_count(&mut tx).await?;
            sqlx::raw_sql(V10_UPGRADE_OWNED).execute(&mut *tx).await?;
            sqlx::query(
                "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )
            .bind(META_DISCARDED_AGENT_PROFILES)
            .bind(discarded.to_string())
            .execute(&mut *tx)
            .await?;
        }
        if file_version < 11 {
            v11_upgrade_storage(&mut tx).await?;
        }
        if file_version < 12 {
            v12_upgrade_owned(&mut tx).await?;
        }
        if file_version < 13 {
            v13_upgrade(&mut tx).await?;
        }
        if file_version < 14 {
            v14_upgrade(&mut tx).await?;
        }
        restore_sequences(&mut tx, &sequences).await?;
        mark_schema_versions(&mut tx).await?;
    }

    write_meta_if_absent(&mut tx, META_SERVER_EPOCH, &new_server_epoch()).await?;
    write_meta_if_absent(&mut tx, META_CREATED_AT, at).await?;
    write_meta_if_absent(&mut tx, META_LAST_PRUNE_AT, at).await?;
    write_meta_if_absent(
        &mut tx,
        META_OWNED_SCHEMA_VERSION,
        &OWNED_SCHEMA_VERSION.to_string(),
    )
    .await?;

    if file_version != FILE_FORMAT_VERSION {
        tx.execute(format!("PRAGMA user_version = {FILE_FORMAT_VERSION}").as_str())
            .await?;
    }

    let server_epoch = read_meta_row(&mut *tx, META_SERVER_EPOCH)
        .await?
        .ok_or(StorageError::InvalidRequest("meta.server_epoch missing"))?;
    let created_at = read_meta_row(&mut *tx, META_CREATED_AT)
        .await?
        .ok_or(StorageError::InvalidRequest("meta.created_at missing"))?;
    let last_prune_at = read_meta_row(&mut *tx, META_LAST_PRUNE_AT)
        .await?
        .ok_or(StorageError::InvalidRequest("meta.last_prune_at missing"))?;

    tx.commit().await?;

    Ok(StoreMetadata {
        server_epoch,
        owned_schema_version: OWNED_SCHEMA_VERSION,
        created_at,
        last_prune_at,
    })
}

/// §3.2：`ServerEpoch` 首次创建时生成，随后保持不变。用 UUID v4（随机源，不读系统时间）。
fn new_server_epoch() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// §7.2：库内是否已经有 schema（用 `owned_session` 判定，它在 v1 与 v2 都存在）。
///
/// 版本号与表结构在同一个事务里落盘，所以「升级前已有 schema」是区分「升级」与「新建」的准确判据：
/// 新建文件上 `PRAGMA user_version` 是 `0`，而 `0 < FILE_FORMAT_VERSION` 会让它走进升级分支，去重建
/// 刚刚由 DDL 常量建好的表。
async fn table_exists<'e, E>(executor: E, name: &str) -> Result<bool, StorageError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1")
            .bind(name)
            .fetch_one(executor)
            .await?;
    Ok(count > 0)
}

async fn column_exists<'e, E>(executor: E, table: &str, column: &str) -> Result<bool, StorageError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let rows: Vec<String> =
        sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .fetch_all(executor)
            .await?;
    Ok(rows.iter().any(|name| name == column))
}

/// §7.2：重建前需要保留的 `sqlite_sequence` 现值（`(表名, seq)`）。
///
/// 重建会把那一行随 `DROP TABLE` 一起删掉；只按现存行的 `max(…)` 回填会让「尾部行已被保留期清理」的库
/// 序列回退——审计有 365 天 TTL、事件也有保留窗口（§7.5），这个场景是常态。两张表都是
/// `AUTOINCREMENT`（`owned_audit` 的 `audit_id`、`owned_event` 的 `global_sequence`），
/// 因此 `sqlite_sequence` 一定存在，且**都**必须原样保留：`global_sequence` 是跨节点排序与 cursor 的
/// 基准（`NODE_LINK_PROTOCOL.md` §7），回退等于重复编号。
async fn autoincrement_sequences<'e, E>(executor: E) -> Result<Vec<(String, i64)>, StorageError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT name, seq FROM sqlite_sequence \
         WHERE name IN ('owned_audit','owned_event')",
    )
    .fetch_all(executor)
    .await?;
    Ok(rows)
}

/// §7.2：把重建前的序列值回填进 `sqlite_sequence`（只增不减）。
///
/// `sqlite_sequence` 上没有唯一索引，因此不能用 `ON CONFLICT`：先按值更新既有的更小行，再在缺行
/// （重建后表为空）时插入。
async fn restore_sequences(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    sequences: &[(String, i64)],
) -> Result<(), StorageError> {
    for (name, seq) in sequences {
        // 先取 `?1` 的绑定值：两条语句共用同一次绑定顺序（`UPDATE` 里 `name = ?1 AND seq < ?2`）。
        sqlx::query("UPDATE sqlite_sequence SET seq = ?2 WHERE name = ?1 AND seq < ?2")
            .bind(name)
            .bind(*seq)
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "INSERT INTO sqlite_sequence (name, seq) SELECT ?1, ?2 \
             WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = ?1)",
        )
        .bind(name)
        .bind(*seq)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// §7.2：升级的最后一步：把 owned 版本键写成当前常量。
///
/// `write_meta_if_absent` 只在键缺失时写入，升级路径必须显式覆盖既有值（新建库走不到这里，它的版本
/// 键由 `write_meta_if_absent` 按当前常量写入）。
async fn mark_schema_versions(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<(), StorageError> {
    {
        sqlx::query(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(META_OWNED_SCHEMA_VERSION)
        .bind(OWNED_SCHEMA_VERSION.to_string())
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn read_user_version<'e, E>(executor: E) -> Result<i64, StorageError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(executor)
        .await?;
    Ok(version)
}

/// 读取 `meta` 中的一行。表不存在时返回 `None`（首次创建）。
pub async fn read_meta_row<'e, E>(executor: E, key: &str) -> Result<Option<String>, StorageError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
        .bind(key)
        .fetch_optional(executor)
        .await?;
    Ok(row.map(|(value,)| value))
}

/// 只在键不存在时写入：保证第二次启动不产生任何行级写入（§9.1 的逐字节幂等）。
pub async fn write_meta_if_absent(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    key: &str,
    value: &str,
) -> Result<(), StorageError> {
    sqlx::query("INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO NOTHING")
        .bind(key)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// `meta` 的版本值必须是十进制非负整数；其它取值说明库被外部改写。
fn parse_version(value: &str, column: &'static str) -> Result<i64, StorageError> {
    value.parse::<i64>().map_err(|_| StorageError::ColumnValue {
        column,
        expected: "decimal integer",
    })
}

/// §7.1：`PRAGMA wal_checkpoint(TRUNCATE)`。失败不影响调用结果——检查点只是把 WAL 归并回主库，
/// 未归并的 WAL 在下次打开时仍会被正常读取。
pub async fn checkpoint_truncate(pool: &SqlitePool) {
    let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await;
}

/// §7.5 的保留窗口算术。
///
/// `Timestamp` 在 core 里是不透明文本（固定 `%Y-%m-%dT%H:%M:%S%.3fZ`，§3.2），core 也不暴露任何
/// 日期算法；本 crate 不能读系统时间，也没有日期库依赖，因此这里只做**纯函数**的公历换算：
/// 解析 → 加减整天 → 按同一格式重新格式化。闰年与月长由 Hinnant 的 `days_from_civil`/
/// `civil_from_days` 覆盖，不涉及时区（时间戳恒为 UTC 且带 `Z`）。
pub mod window {
    /// 一天的毫秒数。
    pub const DAY_MILLIS: i64 = 86_400_000;

    /// 解析 `YYYY-MM-DDTHH:MM:SS.mmmZ`，返回 Unix 毫秒。形状非法（长度、分隔符、字段范围）返回
    /// `None`——调用方的 `Timestamp` 已经过 core 校验，这里只防御被外部改写的库。
    pub fn parse_millis(text: &str) -> Option<i64> {
        let bytes = text.as_bytes();
        if bytes.len() != 24 {
            return None;
        }
        if bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes[10] != b'T'
            || bytes[13] != b':'
            || bytes[16] != b':'
            || bytes[19] != b'.'
            || bytes[23] != b'Z'
        {
            return None;
        }
        let field = |from: usize, to: usize| -> Option<i64> {
            let slice = text.get(from..to)?;
            if !slice.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            slice.parse::<i64>().ok()
        };
        let year = field(0, 4)?;
        let month = field(5, 7)?;
        let day = field(8, 10)?;
        let hour = field(11, 13)?;
        let minute = field(14, 16)?;
        let second = field(17, 19)?;
        let millis = field(20, 23)?;
        if !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || !(0..=23).contains(&hour)
            || !(0..=59).contains(&minute)
            || !(0..=59).contains(&second)
        {
            return None;
        }
        // 日历有效性（闰日、月长）**刻意不校验**：core 的 `Timestamp` 也只校验字段范围
        // （`model/scalars.rs` 的 `is_timestamp`），这里若更严格就会拒绝 core 已接受的值。
        let days = days_from_civil(year, month as u32, day as u32);
        days.checked_mul(DAY_MILLIS)?
            .checked_add(hour * 3_600_000 + minute * 60_000 + second * 1_000 + millis)
    }

    /// 按 `YYYY-MM-DDTHH:MM:SS.mmmZ` 格式化 Unix 毫秒。
    pub fn format_millis(millis: i64) -> String {
        let days = millis.div_euclid(DAY_MILLIS);
        let rest = millis.rem_euclid(DAY_MILLIS);
        let (year, month, day) = civil_from_days(days);
        let hour = rest / 3_600_000;
        let minute = rest % 3_600_000 / 60_000;
        let second = rest % 60_000 / 1_000;
        let milli = rest % 1_000;
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z")
    }

    /// `text + days` 天；`days` 为负即减法。
    pub fn shift_days(text: &str, days: i64) -> Option<String> {
        let millis = parse_millis(text)?;
        let shifted = millis.checked_add(days.checked_mul(DAY_MILLIS)?)?;
        Some(format_millis(shifted))
    }

    /// Howard Hinnant, `days_from_civil`：`1970-01-01` 为 0。
    fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
        let year = if month <= 2 { year - 1 } else { year };
        let era = if year >= 0 { year } else { year - 399 } / 400;
        let yoe = year - era * 400;
        let mp = ((month + 9) % 12) as i64;
        let doy = (153 * mp + 2) / 5 + day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Howard Hinnant, `civil_from_days`。
    fn civil_from_days(days: i64) -> (i64, u32, u32) {
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = if month <= 2 { year + 1 } else { year };
        (year, month as u32, day as u32)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_and_formats_round_trip() {
            for text in [
                "2026-09-18T00:00:00.000Z",
                "2024-02-29T23:59:59.999Z",
                "2000-02-29T12:00:00.000Z",
                "1970-01-01T00:00:00.000Z",
            ] {
                let millis = parse_millis(text).expect("parses");
                assert_eq!(format_millis(millis), text);
            }
        }

        #[test]
        fn rejects_malformed_shapes() {
            for text in [
                "",
                "2026-09-18T00:00:00.000",
                "2026-09-18 00:00:00.000Z",
                "2026-13-18T00:00:00.000Z",
                "2026-09-32T00:00:00.000Z",
                "2026-09-18T24:00:00.000Z",
                "2026-09-18T00:00:60.001Z",
                "2026-0a-18T00:00:00.000Z",
            ] {
                assert!(parse_millis(text).is_none(), "{text} must be rejected");
            }
        }

        #[test]
        fn shifts_across_month_and_year_boundaries() {
            assert_eq!(
                shift_days("2026-09-18T00:00:00.000Z", 90).as_deref(),
                Some("2026-12-17T00:00:00.000Z")
            );
            assert_eq!(
                shift_days("2026-12-31T23:59:59.999Z", 1).as_deref(),
                Some("2027-01-01T23:59:59.999Z")
            );
            assert_eq!(
                shift_days("2024-02-28T00:00:00.000Z", 1).as_deref(),
                Some("2024-02-29T00:00:00.000Z")
            );
            assert_eq!(
                shift_days("2026-09-18T00:00:00.000Z", -90).as_deref(),
                Some("2026-06-20T00:00:00.000Z")
            );
        }
    }
}
