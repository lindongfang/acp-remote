//! 序号、游标、时间与摘要（`docs/CORE_PORTS_AND_STORAGE.md` §3.2）。
//!
//! 两个硬约束：
//!
//! - `Sequence` 的 v1 上界是 `2^63-1`（§9 的 `[确认]`），超过即**非法值**，不静默回绕——回绕会让
//!   `UNIQUE(session_id, session_sequence)` 与 wire 上的十进制字符串失去单调性含义。
//! - `Timestamp` 固定 `%Y-%m-%dT%H:%M:%S%.3fZ`（UTC、毫秒、`Z`）。定宽字符串的字典序即时间序，所以宽度
//!   是硬约束：少一位毫秒、用 `+00:00` 或省略 `Z` 都会被拒绝。
//!
//! 时间一律由调用方通过 `Clock` 取得并显式传入；本模块不读系统时间。

use std::fmt;
use std::str::FromStr;

use super::error::InvalidValue;
use super::ids::{OriginEpoch, ServerEpoch};

/// 事件与游标序号。v1 上界 `2^63-1`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sequence(u64);

impl Sequence {
    /// v1 上界 `2^63-1`。
    pub const MAX: u64 = i64::MAX as u64;

    /// 构造：超过上界返回 `InvalidValue::SequenceRange`。
    pub fn new(value: u64) -> Result<Self, InvalidValue> {
        if value > Self::MAX {
            Err(InvalidValue::SequenceRange)
        } else {
            Ok(Self(value))
        }
    }

    /// 底层数值。
    pub fn get(self) -> u64 {
        self.0
    }

    /// 下一个序号；已在上界时返回 `None`（不回绕，由调用方决定失败方式）。
    pub fn checked_next(self) -> Option<Self> {
        (self.0 < Self::MAX).then(|| Self(self.0 + 1))
    }
}

impl fmt::Display for Sequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<u64> for Sequence {
    type Error = InvalidValue;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Sequence> for u64 {
    fn from(value: Sequence) -> Self {
        value.0
    }
}

/// 生成一个无上界的 `u64` 计数器 newtype（`Version`/`AttachmentGeneration`/`LocalCursor`）。
macro_rules! counter {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            /// 构造。
            pub fn new(value: u64) -> Self {
                Self(value)
            }

            /// 底层数值。
            pub fn get(self) -> u64 {
                self.0
            }

            /// 下一个值；`u64::MAX` 时返回 `None`。
            pub fn checked_next(self) -> Option<Self> {
                self.0.checked_add(1).map(Self)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self(value)
            }
        }

        impl From<$name> for u64 {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

counter!(
    /// 会话乐观并发版本（`MODULE_ARCHITECTURE.md` §4.1）。由存储层在每次提交时递增。
    Version
);
counter!(
    /// Node Link attachment 代际；每次重新 attach 递增，旧代际的 frame 一律拒绝
    /// （`NODE_LINK_PROTOCOL.md` §7）。
    AttachmentGeneration
);
counter!(
    /// Access 本地投递序号，只服务本节点客户端（`NODE_LINK_PROTOCOL.md` §6）。
    LocalCursor
);

/// 全局游标：`{ serverEpoch, globalSequence }`（`SYNC_PROTOCOL.md` §9.1）。序号只在同一 epoch 内有意义。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlobalCursor {
    pub server_epoch: ServerEpoch,
    pub global_sequence: Sequence,
}

impl GlobalCursor {
    /// 构造。
    pub fn new(server_epoch: ServerEpoch, global_sequence: Sequence) -> Self {
        Self {
            server_epoch,
            global_sequence,
        }
    }
}

/// 会话级 origin 游标：`{ originEpoch, originSequence }`（`NODE_LINK_PROTOCOL.md` §6）。不可跨会话比较。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OriginCursor {
    pub origin_epoch: OriginEpoch,
    pub origin_sequence: Sequence,
}

impl OriginCursor {
    /// 构造。
    pub fn new(origin_epoch: OriginEpoch, origin_sequence: Sequence) -> Self {
        Self {
            origin_epoch,
            origin_sequence,
        }
    }
}

newtype!(
    /// 毫秒精度 UTC RFC 3339 时间戳，固定 `%Y-%m-%dT%H:%M:%S%.3fZ`。
    ///
    /// 校验形状与各字段范围（month 1..=12、day 1..=31、hour ≤23、minute ≤59、second ≤59），
    /// **不做**日历（闰日）校验：定宽与字段范围已足以保证「TEXT 排序即时间序」。
    Timestamp,
    check_timestamp
);

newtype!(
    /// 32 字节 SHA-256 的规范无填充 base64url 文本（43 字符，`common.schema.json#/$defs/base64url32`）。
    ///
    /// “规范”指末字符的低 2 位必须为 0（只有 4 位有效），因此 `=` 填充、`+`/`/` 与非法尾字节都被拒绝。
    Digest,
    check_digest
);

newtype!(
    /// 32 字节随机数的规范无填充 base64url 文本（43 字符；`clientNonce`/`serverNonce`/`requestNonce`）。
    Nonce,
    check_nonce
);

fn is_timestamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 24 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        let expected = match index {
            4 | 7 => Some(b'-'),
            10 => Some(b'T'),
            13 | 16 => Some(b':'),
            19 => Some(b'.'),
            23 => Some(b'Z'),
            _ => None,
        };
        match expected {
            Some(fixed) => {
                if *byte != fixed {
                    return false;
                }
            }
            None => {
                if !byte.is_ascii_digit() {
                    return false;
                }
            }
        }
    }
    let field = |from: usize, to: usize| -> u32 {
        bytes[from..to]
            .iter()
            .fold(0u32, |acc, byte| acc * 10 + u32::from(byte - b'0'))
    };
    (1..=12).contains(&field(5, 7))
        && (1..=31).contains(&field(8, 10))
        && field(11, 13) <= 23
        && field(14, 16) <= 59
        && field(17, 19) <= 59
}

fn base64url_index(byte: u8) -> Option<u32> {
    match byte {
        b'A'..=b'Z' => Some(u32::from(byte - b'A')),
        b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn is_base64url_32(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 43 {
        return false;
    }
    match bytes.split_last() {
        Some((last, head)) => {
            // 43 字符承载 32 字节：末字符的 6 位里只有高 4 位有效，低 2 位必须为 0，
            // 即字母表下标必须是 4 的倍数（A E I M Q U Y c g k o s w 0 4 8）。
            let last_ok = matches!(base64url_index(*last), Some(index) if index % 4 == 0);
            last_ok && head.iter().all(|byte| base64url_index(*byte).is_some())
        }
        None => false,
    }
}

fn check_timestamp(text: &str) -> Result<(), InvalidValue> {
    if is_timestamp(text) {
        Ok(())
    } else {
        Err(InvalidValue::Timestamp)
    }
}

fn check_digest(text: &str) -> Result<(), InvalidValue> {
    if is_base64url_32(text) {
        Ok(())
    } else {
        Err(InvalidValue::Digest)
    }
}

fn check_nonce(text: &str) -> Result<(), InvalidValue> {
    if is_base64url_32(text) {
        Ok(())
    } else {
        Err(InvalidValue::Nonce)
    }
}
