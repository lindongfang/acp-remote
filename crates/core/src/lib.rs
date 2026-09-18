//! ACP Remote 的领域层：`model`（值对象与不变量）、`ports`（出站端口）、`use_cases`（用例面）与
//! `broker`（提交与发布顺序）。
//!
//! 唯一权威是 `docs/CORE_PORTS_AND_STORAGE.md`（编码前契约 v1）：§3 值对象、§4 用例面、§5 端口签名、
//! §6 broker 顺序与事务契约。本 crate **不依赖** Tokio、Axum、SQLite、WebSocket、子进程、ACP DTO 或任何
//! wire protocol（`AGENTS.md` §4、`docs/MODULE_ARCHITECTURE.md` §4.1）；端口用 `#[async_trait]`
//! （纯 proc-macro，不引入 runtime），由组合根注入具体实现。

pub mod broker;
pub mod model;
pub mod ports;
pub mod use_cases;
