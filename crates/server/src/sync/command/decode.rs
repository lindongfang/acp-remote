//! `command` 帧的解码与**封闭词表**判定（§11.1/§11.5；任务 2.1）。
//!
//! 两层判定，顺序固定：
//!
//! 1. **命令名**：先看原始 body 的 `command` 字段。取值不在 Sync v1 的 18 条枚举里（含只经 Node Link
//!    接受的 `session.resume`）即 [`Fault::Unsupported`] → `command.unsupported`。这一步刻意发生在
//!    `Command` 解码**之前**：`CommandName` 的 `Deserialize` 会把 `session.resume` 报成形状错误，而协议
//!    要求的是 `command.unsupported`（§11.5 该行），且**不得在本机执行任何操作**——本模块在这一步之下
//!    没有任何 core 调用，因此「拒绝」就是「什么也没做」。
//! 2. **形状**：交给 `sync-protocol` 的 `Command` DTO（`additionalProperties: false`、`required` 与
//!    `sessionId`/`expectedVersion` 的出现性规则都在那里）。不符即 [`Fault::Schema`] →
//!    `protocol.schema_invalid`。本层不重复实现 schema（那会变成第二套判定）。

use std::str::FromStr as _;

use sync_protocol::command::{Command, CommandName};
use sync_protocol::envelope::Envelope;

/// 一次解码的失败分类（两种对端可见的后果不同，因此不合并成一个 `bool`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// 命令名不在 Sync v1 的封闭词表里（含 `session.resume`）→ `command.unsupported`。
    Unsupported,
    /// body 不符合 v1 schema → `protocol.schema_invalid`。
    Schema,
}

impl Fault {
    /// 结构化日志与 `error` 消息文本的稳定取值（不含请求内容）。
    pub fn as_str(self) -> &'static str {
        match self {
            Fault::Unsupported => "unsupported",
            Fault::Schema => "schema_invalid",
        }
    }
}

/// 解码一条 `command` 帧的 body（`conn` 已完成信封/序号/type 判定）。
pub fn decode_command(message: &Envelope) -> Result<Command, Fault> {
    let body = message.body().get();
    // ① 命令名：`command` 键必须存在且是字符串（缺键/类型不符是**形状**问题），其取值必须在封闭词表内
    //    （词表外是**支持性**问题）：两者的错误码不同，不能混。
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| Fault::Schema)?;
    let Some(name) = value.get("command") else {
        return Err(Fault::Schema);
    };
    let Some(name) = name.as_str() else {
        return Err(Fault::Schema);
    };
    if CommandName::from_str(name).is_err() {
        return Err(Fault::Unsupported);
    }
    // ② 形状：命令名已确定，payload/sessionId/expectedVersion 的规则交回 `sync-protocol` 的 DTO。
    serde_json::from_str(body).map_err(|_| Fault::Schema)
}
