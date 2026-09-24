//! 固定 v1 上限。
//!
//! 这些值是**固定 v1 常量，不是配置键**（`docs/SECURITY_DESIGN.md` §12.2 与 `docs/SYNC_PROTOCOL.md`
//! §14 的惯例）。实现不得自行发明未列出的数值，也不得把它们做成可配置项。

/// 单条 ACP 消息（一个 JSON document，不含 stdio 分帧的换行）的解析上限。
///
/// 与 `docs/SYNC_PROTOCOL.md` §14 的 `maxMessageBytes` 同值（1 MiB），避免同一条消息在两跳上有
/// 两个上限。超过该值时 `RawDocument::parse` 返回 [`crate::AcpError::Oversize`]，不保留部分结果。
pub const MAX_MESSAGE_BYTES: usize = 1_048_576;
