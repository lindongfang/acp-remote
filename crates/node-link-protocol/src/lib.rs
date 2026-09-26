//! ACP Remote 节点间 Node Link 的 wire 合同（`docs/NODE_LINK_PROTOCOL.md`）。
//!
//! §9.3/§9.4 的 transcript domain/tag 表在 `domains`；协议专属的值对象在 `common`，跨协议共用的值对象与
//! 字段校验机制来自叶子 crate `acpr-wire`（[ADR-0007](../../../docs/adr/0007-shared-wire-value-crate.md)）。
//! 事件视图契约与 Sync 共享：本 crate 只承载开放的 `payload.view` 对象，类型化视图投影在 `sync-protocol`。

pub mod catalog;
pub mod command;
pub mod common;
pub mod domains;
pub mod envelope;
pub mod error;
pub mod handshake;
pub mod pairing;
pub mod resource;
pub mod structure;
