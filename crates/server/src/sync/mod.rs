//! Sync 入站适配器（浏览器 PWA 经 WSS 连到本机 Host 的那条线）。
//!
//! 已落地的切片：
//!
//! - [`conn`]：连接面——`/sync/v1` 的一次会话（四消息握手与 15 秒窗口、来源绑定、单设备替换、撤销
//!   传播、心跳与静默超时、close code 映射、连接注册表与有界出站队列）。
//!
//! 尚未落地（与本变更的其他工作包对应）：订阅与 cursor 判定、快照组装与摘要、设备配对的 HTTP 端点
//! （`subscribe`/`snapshot`/`pairing` 子模块）。本模块**只**声明已经存在的子模块，不由占位符预支。
//!
//! 依赖纪律（`docs/MODULE_ARCHITECTURE.md` §4.9、`AGENTS.md` §5）：本模块是入站适配器，只调用
//! `core::use_cases` 与 `sync-protocol`/`identity-auth` 的公开面——
//!
//! - **不 import `storage-sqlite`**、不查 SQLite（持久事实一律经用例面读取）；
//! - **不调用 `server::node_link`**（平级的入站适配器互不调用；两者只共享 `transport::net` 与
//!   `core` 的公开面）；
//! - **不依赖 `app`**（组合根反向依赖本模块）。
//!
//! 与本模块的 wire 权威：`docs/SYNC_PROTOCOL.md`（§3 传输、§4 信封、§5 协商、§8 认证、§12 错误、
//! §13 心跳、§14 限额、§15 安全）。行为范围以
//! `openspec/changes/sync-server-connection/specs/sync-device-auth/spec.md` 为准。

pub mod conn;
