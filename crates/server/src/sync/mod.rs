//! Sync 入站适配器（浏览器 PWA 经 WSS 连到本机 Host 的那条线）。
//!
//! 已落地的切片：
//!
//! - [`conn`]：连接面——`/sync/v1` 的一次会话（四消息握手与 15 秒窗口、来源绑定、单设备替换、撤销
//!   传播、心跳与静默超时、close code 映射、连接注册表与有界出站队列）；
//! - [`subscribe`]：订阅面——`sync.subscribe` 的 cursor 判定与 `sync.reset_required`、
//!   `sync.snapshot_request`、`sync.ack`（累积、单调不减、不得越过已发送游标）；
//! - [`event`]：事件腿——把持久化 `payload.view` 交给一次形状判定并折成显式枚举（[`event::project`]；
//!   判定只影响诊断、绝不影响「这一条是否下发」），按协商结果做 `acp.raw-payload.v1`（原文容器）与
//!   `core.file-diff.v1`（`file.changed` 视图的 `diffLines`/`diffUnavailable` 两个键）两道门控
//!   （[`event::frame`]）。真实投递腿是实时扇出与增量重放：两条腿各自调 `event::frame`
//!   （[`subscribe::fanout`] / [`subscribe::SubscribeRoute::advance`]），再经**既有**那条有界出站队列
//!   投递（§10.1/§10.3）；`event` 模块的公开入口 [`event::deliver`]（判定 → 诊断 → 装配 → 入队）
//!   今天只有用例调用，生产链路不经过它；
//! - [`command`]：命令面——`command` 帧的构造校验、§14 的按设备限流、授权判定与十条查询命令
//!   （§11.3；`workspace.browse` 与四条 `session.files.*`/`session.git.*` 是其中的只读命令）的
//!   `command.result` 投影；
//! - [`router`]：[`router::SyncRoutes`]——连接只有一个路由槽，本组合件把订阅面与命令面按序链起来，并把
//!   认证后交接的 feature 转发给两者（组合根注册的就是它）；
//! - [`snapshot`]：[`snapshot::plan`] 组装三类清单资源与分块，[`snapshot::send`] 下发并计算
//!   `snapshotDigest`（§9.4）；
//! - [`pairing`]：四条配对 HTTP 端点（`POST /sync/v1/pairing/claim`、`POST /sync/v1/pairing/status`、
//!   `POST /sync/v1/pairing/lookup`、`POST /sync/v1/pairing/qr-lookup`）的判定、四个安全响应头、按来源/
//!   `pairingId` 的固定限流与配对码的一次性消耗（§7.1–§7.6、§14）。
//!
//! 本模块**只**声明已经存在的子模块，不由占位符预支。
//!
//! 依赖纪律（`docs/MODULE_ARCHITECTURE.md` §4.9、`AGENTS.md` §5）：本模块是入站适配器，调用面是
//! `core` 的公开面 + `sync-protocol`/`identity-auth` 的公开面。**当前的过渡形态**：[`subscribe`] 与
//! [`command`] 今天还**直连** `core::broker` 的公开面——[`subscribe`] 一处（`Broker::replay`，用于
//! 保留窗口探测），[`command`] 两处（`Broker::authorize` 是观察授权的唯一判定点、`Broker::session_version`
//! 是 `session.config.list` 结果的 `version` 来源）。按 §4.9，adapter 只应调用 `core::use_cases`；
//! **事件投递已收拢到用例面**（`UseCases::sync_replay`，任务 1.6 已落地）：`Broker::replay` 的直连只剩
//! 保留窗口探测一处（`reset_required` 的唯一来源，该结论不在 `sync_replay` 的判定范围内，见
//! [`subscribe`] 的模块文档）；[`command`] 的两处都收窄到「已按同一命令名授权之后」的读，不构成
//! 第二条可见性判定路径（见 [`command`] 的模块文档）。
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
//! `openspec/changes/archive/2026-10-15-sync-server-connection/specs/sync-device-auth/spec.md` 为准。

pub mod attachments;
pub mod command;
pub mod conn;
pub mod event;
pub mod local_lifecycle;
pub mod pairing;
pub mod remote;
pub mod router;
pub mod snapshot;
pub mod subscribe;
