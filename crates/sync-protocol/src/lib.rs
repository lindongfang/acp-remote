//! ACP Remote 客户端同步线的 wire 合同（`docs/SYNC_PROTOCOL.md`）。
//!
//! v1 的 29 个消息类型、35 个事件视图定义与配对 HTTPS 载荷全部落地：§6.3 的 transcript domain/tag 表
//! （`domains`）、v1 信封与消息类型分派（`envelope`）、六个家族的 body——`auth`、`sync`（含 9 个
//! `sync.remote_*` 来源流消息，见 §9.7）、`control`、`error`、`event`、`command`——`event.payload.view`
//! 的类型化投影（`views`），以及设备短二维码的 `q2` 载荷与地址编解码（`device_qr`）、
//! claim/status/lookup/qr-lookup 配对载荷（`pairing`）；公共值对象在 `common`。
//!
//! 传输层不解析 body：`envelope` 以 `RawValue` 原样承载，类型化解析由调用方按 `type` 显式选择。
//! 本 crate 只做 wire 形状与取值域校验；游标保留窗口、快照一致性、命令终态唯一性、授权与状态机
//! 判定都不在这里。
//!
//! 公共值对象在 `common`；独立已认证附件连接的帧族在 `attachment`，类型化媒体与有效交互的公共值对象
//! 在 `media`。

pub mod attachment;
pub mod auth;
pub mod command;
pub mod common;
pub mod control;
pub mod device_qr;
pub mod domains;
pub mod envelope;
pub mod error;
pub mod event;
pub mod media;
pub mod pairing;
pub mod sync;
pub mod views;
