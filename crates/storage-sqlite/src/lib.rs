//! `core` 的 SQLite 持久化后端。
//!
//! 唯一权威是 `docs/CORE_PORTS_AND_STORAGE.md` §7：文件与 PRAGMA、两族 migration、`owned_*` 表、
//! `imported_*` 无正文表、保留/清理/容量与崩溃恢复。本 crate 是**唯一**允许持有连接池、SQL 文本与
//! 运行时依赖的地方；`sqlx::Error` 必须在这里映射成 `core::ports::PortError` 后才可以离开
//! （`docs/MODULE_ARCHITECTURE.md` §8）。
//!
//! Owned 家族只有 `SessionStore::commit` 一个写入口，imported 家族只有
//! `RemoteDeliveryStore::commit_receipt` 一个写入口，且默认 `no-content-cache`：本 crate 不写
//! 任何远程会话正文。

pub mod admin;
pub mod error;
pub mod migrate;
pub mod session_store;
