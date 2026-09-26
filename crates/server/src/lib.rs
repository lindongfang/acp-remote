//! 入站协议 adapter 的宿主（`docs/MODULE_ARCHITECTURE.md` §4.9）。
//!
//! 已落地的路径：
//!
//! - [`transport::local`]：平台本地 IPC 的 endpoint 创建与访问控制、帧编解码、channel 用途绑定、
//!   未完成请求上限，以及 `0x02` 连接在 facade 缺席期的失败方式；
//! - [`transport::net`]：`daemon.listen` 的共享 HTTP/WSS listener、path 路由、Host/代理头边界、
//!   TLS `proxy`/`direct` 两种模式与连接级上限（`node-link-owner` 变更的 WP2）；
//! - [`local_admin`]：管理信封（channel `0x01`）的值对象与编解码、`local.*` 错误码、方法分发表，
//!   以及方法路由（本地配置族、`daemon.*`、Export/Import、`audit.export`）。
//!
//! `server::sync`、`server::node_link`、`server::acp_facade` 尚未落地：`transport::net` 只提供连接与
//! 字节/帧规则，Node Link 与 Sync 的协议语义由各自的入站适配器实现，三者不共享 DTO。
//!
//! 唯一权威：`docs/LOCAL_ADMIN_PROTOCOL.md`（endpoint、访问控制、framing、信封、方法集、错误码、生命周期）
//! 与 `docs/CONFIG_REFERENCE.md` §1/§10 + `docs/NODE_LINK_PROTOCOL.md` §2.1/§2.5/§13.4（网络接入面）。
//! 边界纪律：
//!
//! - 本 crate 不含业务规则；方法路由只调用 `core::use_cases`（以 `Actor::LocalCli`）与 core 端口，
//!   不查询 SQLite、不启动 Agent、不调用 `node-link-client`，也不依赖 `app`；
//! - wire 词表（方法名、错误码、framing 常量）以 `schemas/local-admin/v1/envelope.schema.json` 为机器定义，
//!   由常驻漂移测试逐项断言，不在别处复制第二套；
//! - 平台分支只出现在 [`transport::local`] 的 `platform` 子模块与 [`transport::net`] 的权限/流实现里。

pub mod local_admin;
pub mod transport;
