//! 组合根与 Daemon 生命周期（`docs/MODULE_ARCHITECTURE.md` §4.10：daemon、CLI 与组合根）。
//!
//! 模块组成：
//!
//! - [`config`]：`docs/CONFIG_REFERENCE.md` 的启动配置解析（只读；「已知但未接线」的段落解析后保留并在
//!   debug 日志注明）；
//! - [`logging`]：`logging.*` 的消费——组合根初始化 `tracing` 订阅器；
//! - [`clock`]：`core::ports::Clock` 的实现与 `core` 时间戳文本的格式化；
//! - [`identity`]：本节点身份（keystore 条目 + 由节点公钥确定性派生的 `nodeId`）；
//! - [`lock`]：单实例锁、`instanceId` 与锁文件记录（CLI 判定「Daemon 是否运行」的依据）；
//! - [`compose`]：组合根——构造并持有全部具体实现（`storage-sqlite`、`identity-keystore`、熵源、
//!   `Clock`、`IdGenerator`、`identity_auth::Authority`、`core::use_cases::UseCases`、
//!   `LocalAdminDeps` 与四个注入口）；
//! - [`client`]：本地管理通道的客户端（WP4b 的 CLI 子命令与集成测试共用同一份客户端）；
//! - [`daemon`]：启动序列、接受循环、周期任务与关闭序列；
//! - [`cli`]：本切片的 `daemon start`（其余子命令属 WP4b，`tasks.md` 2.15–2.17）。
//!
//! 唯一权威：`docs/CONFIG_REFERENCE.md`（配置键与默认值）、`docs/LOCAL_ADMIN_PROTOCOL.md`（通道与方法）、
//! `docs/SECURITY_DESIGN.md` §12.1（启动/关闭顺序）。本 crate 只装配，不承载业务规则；`app` 可以依赖
//! 全部具体 crate，任何其它 crate 不得依赖 `app`。

pub mod cli;
pub mod client;
pub mod clock;
pub mod compose;
pub mod config;
pub mod daemon;
pub mod identity;
pub mod lock;
pub mod logging;

pub use client::{ClientOutcome, LocalAdminClient, outcome_of};
pub use config::{Config, Loaded};
pub use identity::NodeIdentity;
pub use lock::{DaemonLock, LockRecord, read_record};
