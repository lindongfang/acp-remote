//! ACP Remote 客户端同步线的 wire 合同（`docs/SYNC_PROTOCOL.md`）。
//!
//! v1 的 18 个消息类型、33 个事件视图定义与配对 HTTPS 载荷全部落地：§6.3 的 transcript domain/tag 表
//! （`domains`）、v1 信封与消息类型分派（`envelope`）、六个家族的 body——`auth`、`sync`、`control`、
//! `error`、`event`、`command`——`event.payload.view` 的类型化投影（`views`），以及二维码/claim/status
//! 配对载荷（`pairing`）；公共值对象在 `common`。
//!
//! 传输层不解析 body：`envelope` 以 `RawValue` 原样承载，类型化解析由调用方按 `type` 显式选择。
//! 本 crate 只做 wire 形状与取值域校验；游标保留窗口、快照一致性、命令终态唯一性、授权与状态机
//! 判定都不在这里。

pub mod auth;
pub mod command;
pub mod common;
pub mod control;
pub mod domains;
pub mod envelope;
pub mod error;
pub mod event;
pub mod pairing;
pub mod sync;
pub mod views;
