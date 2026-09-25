//! 入站协议 adapter 的宿主（`docs/MODULE_ARCHITECTURE.md` §4.9）。
//!
//! 本切片（`daemon-cli-and-local-admin` 的 WP3a）只落地两条路径：
//!
//! - [`transport::local`]：平台本地 IPC 的 endpoint 创建与访问控制、帧编解码、channel 用途绑定、
//!   未完成请求上限，以及 `0x02` 连接在 facade 缺席期的失败方式；
//! - [`local_admin`]：管理信封（channel `0x01`）的值对象与编解码、`local.*` 错误码、方法分发表。
//!
//! `server::sync`、`server::node_link`、`server::acp_facade` 尚无实现（切片 6）；本 crate 的本地通道与
//! 那两个出站/入站边界不共享 DTO。
//!
//! 唯一权威：`docs/LOCAL_ADMIN_PROTOCOL.md`（endpoint、访问控制、framing、信封、方法集、错误码、生命周期）。
//! 边界纪律：
//!
//! - 本 crate 不含业务规则；方法路由只允许调用 `core::use_cases`（WP3b 接线），不查询 SQLite、不启动 Agent、
//!   不调用 `node-link-client`，也不依赖 `app`；
//! - wire 词表（方法名、错误码、framing 常量）以 `schemas/local-admin/v1/envelope.schema.json` 为机器定义，
//!   由常驻漂移测试逐项断言，不在别处复制第二套；
//! - 平台分支只出现在 [`transport::local`] 的 `platform` 子模块里。

pub mod local_admin;
pub mod transport;
