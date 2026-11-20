# 独立持久化展示快照库存储与生命周期实现

行为权威依据为 [展示快照库规格](../../openspec/specs/session-transcript-cache/spec.md)、[结构化历史规格](../../openspec/specs/structured-history/spec.md) 与 [设计文档](../../openspec/changes/archive/2026-11-20-deliver-session-transcript-cache/design.md)。上层节点混合自适应读取、后台单飞标准重放与上下文恢复协调见 [展示快照协调实现](transcript-cache.md)。

## 物理分库、零新依赖 `sqlx::sqlite` 与 Windows 句柄生命周期

1. **物理文件与控制库隔离**：
   - `DataDirectory::transcript_cache_path` 将展示快照库固定于 `<data_dir>/transcript-cache.sqlite3`（连同 `-wal`、`-shm`、`-journal` 附属文件），与核心控制管理库 `node.sqlite3`（`storage.rs::SCHEMA_VERSION = 15`）完全物理分离，不向 `node.sqlite3` 增加任何表，不占用控制面写事务锁。
   - `DataDirectory::acquire` 与 `DataDirectory::secure_transcript_cache_files` 对快照库及附属文件执行符号链接/重解析点（`FILE_ATTRIBUTE_REPARSE_POINT`）及非普通文件拒绝检查（`AppError::UnsafeDataDirectoryEntry`），并在 Windows 下调用 `protect_private_file` 施加当前进程 owner 独占 SDDL ACL。
2. **零新依赖独立连接池与确定性关闭**：
   - `app/src/transcript_cache/` 直接复用仓库已带 worker join 补丁的 `sqlx::sqlite`（`vendor/sqlx-sqlite`），通过独立的 `StorageOpenOwner::start_worker_scope` 与 `StorageLifecycleHost` 建立指向 `transcript-cache.sqlite3` 的物理独立 `SqlitePool`，在 `--locked` 约束下保持零新增 Cargo 依赖。
   - 连接统一启用 `PRAGMA journal_mode = WAL`、`PRAGMA foreign_keys = ON` 与 `PRAGMA secure_delete = ON`。
   - `TranscriptCacheStore::shutdown` 与 `TranscriptCacheWriter::shutdown` 先封存异步写入队列并等待后台刷盘与周期清理任务 `join`，随后触发 `StorageOpenOwner::request_stop` 与 `StorageLifecycleHost::observe_actual`，确保底层 SQLite worker 线程与全部 Windows 文件句柄确定性释放，节点停止后临时 `data_dir` 可立即无锁删除。

## 配置契约与 Schema 布局

1. **`acp-remote.toml` 可选 `[transcript_cache]` 配置（`app/src/config.rs`）**：
   - 缺省 `[transcript_cache]` 时自动启用 `retention_days = 30`（`2_592_000_000 ms`）与 `max_bytes = 10_737_418_240`（`10 GiB`），100% 兼容既有配置文件。
   - 保持 `#[serde(deny_unknown_fields)]`，明确拒绝 `0`、负数、毫秒换算溢出及任何未知配置键（返回 `AppError::Config`）。
2. **表结构（`TRANSCRIPT_CACHE_SCHEMA_VERSION = 1`）**：
   - `transcript_cache_meta(id INTEGER PRIMARY KEY CHECK (id = 1), schema_version INTEGER NOT NULL)`；
   - `transcript_sessions`：按 `(source_id BLOB, native_session_id TEXT)` 唯一索引会话当前快照代际，持久化 `active_generation`、`protocol_version`（`1` 对应 ACP `V1`、`2` 对应 ACP `V2`）、32 字节规范化 SHA-256 `content_digest`、`last_updated_at_ms`、`last_synced_at_ms`、`capability_profile`、`native_recovery_state`、`record_count` 与精确累计的 `byte_size`，并建立 `idx_transcript_sessions_updated(last_updated_at_ms ASC, source_id ASC, native_session_id ASC)`；
   - `transcript_records`：按 `(source_id, native_session_id, generation, ordinal)` 存储经标准 ACP 观测到的有序原始 `session/update` 帧 `raw_json` 与 `connection_sequence`，外键 `ON DELETE CASCADE` 关联 `transcript_sessions`。

## 离线版本感知投影、防虚刷滑动保留与安全擦除

1. **离线版本感知还原**：
   - `AcpViewProjector::for_version(Version)` 支持在不持有存活 `Connection` 的冷启动场景下按 `transcript_sessions.protocol_version` 离线构造 `V1` 或 `V2` 投影器，投入 `IndexedJournal` 完整还原轮次顺序、思考块、9 槽工具原位更新、子步骤原始引用及 `V1`/`V2` 协议差异语义，保持前端 `RawJsonDocument` 不可信内容安全隔离。
2. **`content_digest` 防虚刷与 30 天 / 10 GiB 清理**：
   - `TranscriptCacheStore::load_snapshot` 纯读取快照绝不修改 `last_updated_at_ms`；`replace_with_replay` 对 `protocol_version` 与规范化 JSON 令牌序列计算 `content_digest`，当 `new_digest == old_digest` 时仅更新 `last_synced_at_ms` 而不刷新 `last_updated_at_ms`，仅在实时新帧写入或原生重放产生实质变更时才滑动续期。
   - `enforce_retention_and_budget` 在启动、每 5 分钟周期扫描及写后检查中：先删除超过 `retention_days` 的非受保护快照，再在 `SUM(byte_size) > max_bytes` 时按 `last_updated_at_ms ASC` 跳过受 `ProtectedSessionsPort` 保护的已加载活跃会话及持有在途分块读取租约的会话，整会话原子淘汰最旧非活跃快照，且绝不触碰 Harness 原生会话文件。
3. **原生删除同步擦除**：
   - `purge_deleted_native_session` 在确认原生会话删除时立即在事务内删除对应 `transcript_records` 与 `transcript_sessions` 行，配合 `PRAGMA secure_delete = ON` 与 `PRAGMA wal_checkpoint(TRUNCATE)` 覆写底层页并同步失效内存缓存代际，确保磁盘文件不残留已删除会话明文序列。
