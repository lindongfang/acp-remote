//! 网页入口的配对码：域分离派生、Crockford base32 编解码与单次查找的结果（design D2/D3）。
//!
//! 三条边界：
//!
//! - **码不是密钥**：配对码只用来一次性取回绑定与 secret；claim/status 的 HMAC 仍使用完整的
//!   256 bit pairing secret（`docs/IDENTITY_AND_AUTH_CONTRACT.md` §4.4）。
//! - **只在内存**：码与反查表都由 `Authority` 的进程内状态持有，与 secret 同生同灭；不新增持久
//!   字段，重启后不能从 digest 重建。
//! - **不记录**：码可以取回 secret，因此 [`fmt::Debug`] 只显示占位符；终端展示必须走
//!   [`PairingCode::as_str`] 或 [`fmt::Display`]。

use std::fmt;

use acp_core::model::PairingId;

use crate::error::ProofError;
use crate::transcript::hmac_sha256;
use crate::types::PairingSecret;

/// 配对码的固定字符数（40 bit / 每字符 5 bit）。
pub const PAIRING_CODE_LENGTH: usize = 8;

/// 配对码的字节数（40 bit）。
const PAIRING_CODE_BYTES: usize = 5;

/// 派生配对码的域分离标签（`HMAC-SHA256(key = pairingSecret, msg = ...)`）。
const PAIRING_CODE_DOMAIN: &[u8] = b"acp-remote/pairing-code/v1";

/// Crockford base32 字母表：不含 `I`/`L`/`O`/`U`。
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 配对码的规范化文本（8 位 Crockford base32）。
///
/// 值只在内存中流转：`Debug` 是脱敏的，`Display` 供终端/页面展示。`Ord` 与 `Hash` 只为把它
/// 用作反查表的键（状态机内部的查找不依赖它们）。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PairingCode(String);

impl PairingCode {
    /// 规范字符数。
    pub const LEN: usize = PAIRING_CODE_LENGTH;

    /// 归一化并解析用户输入的配对码：大小写不敏感，`I`/`L` → `1`、`O` → `0`，允许一个 `-`
    /// 分隔符。其余字符、长度不符或多余的 `-` 都是 [`InvalidPairingCode`]。
    ///
    /// 返回值总是规范形态（大写、无分隔符），因此可以直接与派生码比较。
    pub fn parse(text: &str) -> Result<Self, InvalidPairingCode> {
        let mut canonical = String::with_capacity(PAIRING_CODE_LENGTH);
        let mut separator = false;
        for character in text.chars() {
            if character == '-' {
                if separator {
                    return Err(InvalidPairingCode);
                }
                separator = true;
                continue;
            }
            let normalized = if character.is_ascii_digit() {
                character
            } else if character.is_ascii_alphabetic() {
                character.to_ascii_uppercase()
            } else {
                return Err(InvalidPairingCode);
            };
            // Crockford 的输入归一化：视觉上易混的三个字母映射到数字。
            let normalized = match normalized {
                'I' | 'L' => '1',
                'O' => '0',
                other => other,
            };
            if !ALPHABET.contains(&(normalized as u8)) {
                // `U` 不在字母表里；其余 ASCII 字母已被上面的映射吸收。
                return Err(InvalidPairingCode);
            }
            if canonical.len() >= PAIRING_CODE_LENGTH {
                return Err(InvalidPairingCode);
            }
            canonical.push(normalized);
        }
        if canonical.len() != PAIRING_CODE_LENGTH {
            return Err(InvalidPairingCode);
        }
        Ok(Self(canonical))
    }

    /// 规范文本（大写、无分隔符）。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 把 40 bit 大端值编码为 8 个 5-bit 组（高位在前）。
    fn from_bits(value: u64) -> Self {
        let mut text = String::with_capacity(PAIRING_CODE_LENGTH);
        for index in 0..PAIRING_CODE_LENGTH {
            let shift = 5 * (PAIRING_CODE_LENGTH - 1 - index);
            // `value & 0x1f` 必然 < 32，与 `ALPHABET` 的长度一致。
            let symbol = ((value >> shift) & 0x1f) as usize;
            text.push(char::from(ALPHABET[symbol]));
        }
        Self(text)
    }

    /// 常数时间比较（长度与内容都不提前返回）。
    ///
    /// 反查表按码遍历时使用：调用方必须在**全部**条目上调用它，不能在首个命中处短路。
    pub(crate) fn matches(&self, other: &Self) -> bool {
        let left = self.0.as_bytes();
        let right = other.0.as_bytes();
        let mut difference = left.len() ^ right.len();
        for (left, right) in left.iter().zip(right.iter()) {
            difference |= usize::from(left ^ right);
        }
        difference == 0
    }
}

impl fmt::Debug for PairingCode {
    /// 配对码可以取回 pairing secret：不进入日志/诊断输出。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingCode(<redacted>)")
    }
}

/// 配对码形状不合法（长度不符、含字母表之外的字符或多余的 `-`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("配对码必须是 8 位 Crockford base32（允许一个 `-` 分隔符）")]
pub struct InvalidPairingCode;

/// 从 pairing secret 派生配对码（design D2）：
///
/// ```text
/// digest = HMAC-SHA256(key = pairingSecret, msg = "acp-remote/pairing-code/v1")
/// code   = CrockfordBase32(digest[0..5])       // 40 bit, 8 chars
/// ```
///
/// 40 bit 恰好是 8 个 5-bit 组，没有取模偏差。返回 `Result` 不是因为它会失败，而是本 crate 的
/// 约定：正常路径不靠 `unwrap`/`expect` 处理理论上不可达的分支（同 [`PairingSecret::digest`]）。
pub fn derive_pairing_code(secret: &PairingSecret) -> Result<PairingCode, ProofError> {
    let digest = hmac_sha256(secret.as_bytes(), PAIRING_CODE_DOMAIN)?;
    let mut value = 0u64;
    for byte in &digest[..PAIRING_CODE_BYTES] {
        value = (value << 8) | u64::from(*byte);
    }
    Ok(PairingCode::from_bits(value))
}

/// 一次成功查找的结果：配对标识与完整 secret（lookup 端点组装响应所需的最小集合）。
///
/// 其余绑定字段（`hostId`/`hostPublicKey`/`canonicalOrigin`/`expiresAt`）来自调用方已读到的
/// 配对记录与本机身份，不需要由该入口返回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingCodeLookup {
    /// 码所属的配对。
    pub pairing: PairingId,
    /// 该配对的完整 256 bit secret（此后 claim/status 的 transcript 不变）。
    pub secret: PairingSecret,
}
