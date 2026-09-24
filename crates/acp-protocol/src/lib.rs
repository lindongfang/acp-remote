//! ACP v1 wire 合同：JSON-RPC 信封、raw document 保真、wire DTO、capability 与固定上限。
//!
//! 本 crate 是叶子：只描述并编解码 ACP wire protocol，不依赖 `core`、不依赖其他协议 crate，
//! 也不启动子进程或访问数据库（`docs/MODULE_ARCHITECTURE.md` §4.2/§5）。wire 到公共领域视图的
//! mapper 不在这里，而在 `agent-host` 与 `server::acp_facade`。
//!
//! # 保真的实现方式
//!
//! 保真靠**原文承载**：[`RawDocument`] 持有收到的原始 UTF-8 JSON document 文本（不含 stdio 分帧的
//! 换行），「解码 → 再编码」就是原样回写这段文本。类型化 DTO 只用于读取字段，本身不是保真路径：
//! 任何「先转成通用 `Value` 再序列化」的写法都会改变键顺序、空白、转义与数字文本形式（例如
//! `1.2300` 会变成 `1.23`），所以本 crate 不提供这样的 API。
//!
//! # 三件容易混淆的事
//!
//! - **类别与方向**：类别（request/notification/response）由 JSON-RPC 结构决定；方向由方法名决定
//!   （[`methods`] 的登记表，与 `compatibility/acp/v1/matrix.json` 逐条对应）。两者都不靠猜。
//! - **未知判别子 vs 未知方法**：`session/update` 里未登记的判别子是**可见降级**
//!   （[`update::SessionUpdate::Unknown`]，保留原文，不是错误）；未登记的方法（含 `_` 前缀扩展方法）
//!   是**显式不支持**（[`error::AcpError::Unsupported`]），不得转发、不得静默丢弃。
//! - **结构化 vs 文本**：tool call、diff、terminal 等内容块始终按结构化类型解码
//!   （[`content`]），不得合并成普通文本。

pub mod capability;
pub mod content;
pub mod envelope;
pub mod error;
pub mod limits;
pub mod message;
pub mod methods;
pub mod raw;
pub mod update;

pub use envelope::{Envelope, MessageClass};
pub use error::{AcpError, Result};
pub use methods::{MethodDirection, MethodKind, MethodSpec, MethodStatus};
pub use raw::{IdLiteral, RawDocument};
