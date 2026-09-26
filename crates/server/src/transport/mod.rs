//! 入站适配器的连接承载（`docs/MODULE_ARCHITECTURE.md` §4.9）。
//!
//! 本切片有两条路径：平台本地管理通道（[`local`]）与共享网络接入面（[`net`]，`design.md` D2）；
//! `sync`、`node_link`、`acp_facade` 各自独立的模块在对应切片落地。各入站适配器是平级关系：
//! 只调用 `core::use_cases`，不互相调用。

pub mod local;
pub mod net;
