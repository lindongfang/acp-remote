//! ACP Remote 的领域层：`model`（值对象与不变量）、`ports`（出站端口）、`use_cases`（用例面）、
//! `lifecycle`（本机会话生命周期的运行期目录与五个端口的 core 实现）与 `broker`（提交与发布顺序）。
//!
//! 唯一权威是 `docs/CORE_PORTS_AND_STORAGE.md`（编码前契约 v1）：§3 值对象、§4 用例面、§5 端口签名、
//! §6 broker 顺序与事务契约。本 crate **不依赖** Tokio、Axum、SQLite、WebSocket、子进程、ACP DTO 或任何
//! wire protocol（`AGENTS.md` §5、`docs/MODULE_ARCHITECTURE.md` §4.1）；端口用 `#[async_trait]`
//! （纯 proc-macro，不引入 runtime），由组合根注入具体实现。
//!
//! `derive` 仍是 crate 内部的**纯函数投影层**（`file.changed` 的派生源与行级 diff、展示路径相对化、
//! 会话标题的部分更新语义），只被 `broker` 使用，因此模块本身保持私有；对外**只**再导出
//! [`strip_verbatim`] 一项——`agent-host` 在构造发往本地 ACP Agent 的 `session/new`/`session/resume`
//! 参数时，`cwd` 必须复用同一条 Windows verbatim 前缀规则，避免出现第二份实现。

pub mod broker;
pub mod lifecycle;
pub mod model;
pub mod ports;
pub mod preparation;
pub mod use_cases;

mod derive;

pub use derive::strip_verbatim;
