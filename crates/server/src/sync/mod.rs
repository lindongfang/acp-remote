//! Sync 入站适配器（浏览器 PWA 经 WSS 连到本机 Host 的那条线）。
//!
//! 已落地的切片：
//!
//! - [`conn`]：连接面——`/sync/v1` 的一次会话（四消息握手与 15 秒窗口、来源绑定、单设备替换、撤销
//!   传播、心跳与静默超时、close code 映射、连接注册表与有界出站队列）；
//! - [`subscribe`]：订阅面——`sync.subscribe` 的 cursor 判定与 `sync.reset_required`、
//!   `sync.snapshot_request`、`sync.ack`（累积、单调不减、不得越过已发送游标）；
//! - [`snapshot`]：[`snapshot::plan`] 组装三类清单资源与分块，[`snapshot::send`] 下发并计算
//!   `snapshotDigest`（§9.4）；
//! - [`pairing`]：两条配对 HTTP 端点（`POST /sync/v1/pairing/claim`、`POST /sync/v1/pairing/status`）的
//!   判定、四个安全响应头与按来源/`pairingId` 的固定限流（§7.1–§7.4、§14）。
//!
//! 尚未落地（与本变更的其他工作包对应）：事件投递腿（`event` 帧的装配与实时扇出）——它需要一个能给出
//! `origin`/`causationRequestId` 的 core 事件读面（见 [`subscribe`] 的模块文档）。本模块**只**声明已经
//! 存在的子模块，不由占位符预支。
//!
//! 依赖纪律（`docs/MODULE_ARCHITECTURE.md` §4.9、`AGENTS.md` §5）：本模块是入站适配器，调用面是
//! `core` 的公开面 + `sync-protocol`/`identity-auth` 的公开面。**当前的过渡形态**：[`subscribe`]
//! 今天还**直连** `core::broker`——`Broker::authorize` 是观察授权的唯一判定点，`Broker::replay` 读事件
//! 重放的一致性视图。按 §4.9，adapter 只应调用 `core::use_cases`；其中**事件重放读面**待 core 提供
//! `UseCases::sync_replay`（任务 1.6，在途）后即收拢到用例面，`Broker::replay` 的直连届时移除。
//! 其余的 core 调用面都走用例（`sync_device_handshake_view`、`sync_visible_sessions`、
//! `workspaces`/`profiles`、`ids`）。
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
pub mod event;
pub mod pairing;
pub mod snapshot;
pub mod subscribe;
