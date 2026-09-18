//! `control.ping` / `control.pong` 的 body（`schemas/sync/v1/error.schema.json` 的 `ping`/`pong`）。
//!
//! 两者 body 形状相同（`nonce` 为 16 字节 base64url），方向由对称的 `type` 区分。

use serde::{Deserialize, Serialize};

use crate::common::Base64Url;

/// `control.ping` 与 `control.pong` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Body {
    pub nonce: Base64Url<16>,
}
