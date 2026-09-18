//! ACP Remote transcript codec v1（`docs/SYNC_PROTOCOL.md` §6.2）。
//!
//! 本 crate 是叶 crate（`docs/adr/0005-shared-transcript-codec.md`）：只拥有 codec 结构本身——
//! magic 与 `codecVersion` 校验、domain 长度前缀、字段 tag 的严格升序与唯一性、长度前缀字段，
//! 以及字段值的字节编码（u16be/u64be/NUL 连接）。它**不包含**任何 domain 字符串或 tag 取值，
//! 那些取值属于使用该 codec 的协议 crate（`sync-protocol`、`node-link-protocol`）。
//!
//! [`table`] 模块在其上提供表驱动编解码（字段数量、tag 成员与定长宽度校验）。

pub mod table;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// transcript 前缀，固定 4 字节 ASCII。
pub const MAGIC: [u8; 4] = *b"ACPR";

/// v1 codec 版本，紧随 magic 的单个字节。
pub const CODEC_VERSION: u8 = 1;

/// 字段 tag。wire 上为 u16be，在同一 transcript 内严格递增且唯一。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldTag(pub u16);

/// codec 层错误。domain 与 tag 的取值校验由协议 crate 负责。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TranscriptError {
    #[error("base64url 输入不是无填充规范形式")]
    Base64Url,
    #[error("domain 为空、含 NUL 或超过 u16 上限")]
    InvalidDomain,
    #[error("transcript magic 不是 ACPR")]
    BadMagic,
    #[error("不支持的 codecVersion：{found}（长度不足时以 0 表示无法判定）")]
    BadCodecVersion { found: u8 },
    #[error("domain 长度前缀越界")]
    TruncatedDomain,
    #[error("字段长度前缀越界")]
    TruncatedField,
    #[error("字段内容不是有效 UTF-8")]
    NotUtf8,
    #[error("字段 tag {current} 重复了前一个 tag")]
    DuplicateTag { current: u16 },
    #[error("字段 tag {current} 小于前一个 tag {previous}")]
    FieldOrder { previous: u16, current: u16 },
    #[error("最后一个字段之后仍有剩余字节")]
    TrailingBytes,
    #[error("字段数量超过 u16 上限")]
    TooManyFields,
    #[error("单个字段长度超过 u32 上限")]
    FieldTooLong,
    #[error("字段值含嵌入的 NUL")]
    EmbeddedNul,
}

/// 解码得到的单个字段：tag 与对输入缓冲区的借用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedField<'a> {
    pub tag: FieldTag,
    pub bytes: &'a [u8],
}

/// 解码得到的 transcript。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedTranscript<'a> {
    pub domain: &'a str,
    pub fields: Vec<DecodedField<'a>>,
}

/// 按 `domain` 与 `fields` 编码 transcript。
///
/// 布局：`ACPR` ‖ `codecVersion`(u8) ‖ `domainLength`(u16be) ‖ domain(UTF-8) ‖
/// `fieldCount`(u16be) ‖ 逐字段 `fieldTag`(u16be) ‖ `byteLength`(u32be) ‖ rawBytes。
///
/// tag 必须由调用方按升序提供；重复先于乱序判定，以保证与 [`decode`] 的判定顺序一致。
pub fn encode(domain: &str, fields: &[(FieldTag, &[u8])]) -> Result<Vec<u8>, TranscriptError> {
    if domain.is_empty() || domain.contains('\0') || domain.len() > u16::MAX as usize {
        return Err(TranscriptError::InvalidDomain);
    }
    if fields.len() > u16::MAX as usize {
        return Err(TranscriptError::TooManyFields);
    }

    let mut out = Vec::with_capacity(MAGIC.len() + 5 + domain.len() + fields.len() * 6);
    out.extend_from_slice(&MAGIC);
    out.push(CODEC_VERSION);
    out.extend_from_slice(&(domain.len() as u16).to_be_bytes());
    out.extend_from_slice(domain.as_bytes());
    out.extend_from_slice(&(fields.len() as u16).to_be_bytes());

    let mut previous: Option<u16> = None;
    for (tag, bytes) in fields {
        match previous {
            Some(previous) if previous == tag.0 => {
                return Err(TranscriptError::DuplicateTag { current: tag.0 });
            }
            Some(previous) if tag.0 < previous => {
                return Err(TranscriptError::FieldOrder {
                    previous,
                    current: tag.0,
                });
            }
            _ => {}
        }
        previous = Some(tag.0);

        if bytes.len() > u32::MAX as usize {
            return Err(TranscriptError::FieldTooLong);
        }
        out.extend_from_slice(&tag.0.to_be_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(bytes);
    }

    Ok(out)
}

/// 解码并做全部结构校验。
///
/// 检查顺序固定为：magic → codecVersion → domain 长度与 UTF-8 → 字段数量 → 逐字段
/// （tag/length 头是否完整、tag 重复先于乱序、字段体是否完整）→ 尾部是否有剩余字节。
pub fn decode(bytes: &[u8]) -> Result<DecodedTranscript<'_>, TranscriptError> {
    if bytes.len() < MAGIC.len() || bytes[..MAGIC.len()] != MAGIC {
        return Err(TranscriptError::BadMagic);
    }
    if bytes.len() < 5 {
        return Err(TranscriptError::BadCodecVersion { found: 0 });
    }
    if bytes[4] != CODEC_VERSION {
        return Err(TranscriptError::BadCodecVersion { found: bytes[4] });
    }
    if bytes.len() < 7 {
        return Err(TranscriptError::TruncatedDomain);
    }

    let domain_length = u16::from_be_bytes([bytes[5], bytes[6]]) as usize;
    if bytes.len() < 7 + domain_length {
        return Err(TranscriptError::TruncatedDomain);
    }
    let domain =
        std::str::from_utf8(&bytes[7..7 + domain_length]).map_err(|_| TranscriptError::NotUtf8)?;

    let mut offset = 7 + domain_length;
    if bytes.len() < offset + 2 {
        return Err(TranscriptError::TruncatedField);
    }
    let field_count = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
    offset += 2;

    let mut fields = Vec::with_capacity(field_count);
    let mut previous: Option<u16> = None;
    for _ in 0..field_count {
        if bytes.len() < offset + 6 {
            return Err(TranscriptError::TruncatedField);
        }
        let tag = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        let byte_length = u32::from_be_bytes([
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
        ]) as usize;
        offset += 6;

        match previous {
            Some(previous) if previous == tag => {
                return Err(TranscriptError::DuplicateTag { current: tag });
            }
            Some(previous) if tag < previous => {
                return Err(TranscriptError::FieldOrder {
                    previous,
                    current: tag,
                });
            }
            _ => {}
        }
        previous = Some(tag);

        if bytes.len() < offset + byte_length {
            return Err(TranscriptError::TruncatedField);
        }
        fields.push(DecodedField {
            tag: FieldTag(tag),
            bytes: &bytes[offset..offset + byte_length],
        });
        offset += byte_length;
    }

    if offset != bytes.len() {
        return Err(TranscriptError::TrailingBytes);
    }

    Ok(DecodedTranscript { domain, fields })
}

/// 无填充 base64url 编码（`docs/SYNC_PROTOCOL.md` §6.1）。
pub fn encode_base64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// 无填充 base64url 解码，拒绝填充、标准字母表与非规范尾字节。
pub fn decode_base64url(text: &str) -> Result<Vec<u8>, TranscriptError> {
    let canonical = !text.is_empty()
        && text.len() % 4 != 1
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if !canonical {
        return Err(TranscriptError::Base64Url);
    }

    let bytes = URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|_| TranscriptError::Base64Url)?;
    if encode_base64url(&bytes) != text {
        return Err(TranscriptError::Base64Url);
    }
    Ok(bytes)
}

/// u16be 字段值。
pub fn encode_u16be(value: u16) -> [u8; 2] {
    value.to_be_bytes()
}

/// u64be 字段值。
pub fn encode_u64be(value: u64) -> [u8; 8] {
    value.to_be_bytes()
}

/// 解码 u16be 字段值；长度不是 2 字节视为长度前缀错误。
pub fn decode_u16be(bytes: &[u8]) -> Result<u16, TranscriptError> {
    match <[u8; 2]>::try_from(bytes) {
        Ok(value) => Ok(u16::from_be_bytes(value)),
        Err(_) => Err(TranscriptError::TruncatedField),
    }
}

/// 解码 u64be 字段值；长度不是 8 字节视为长度前缀错误。
pub fn decode_u64be(bytes: &[u8]) -> Result<u64, TranscriptError> {
    match <[u8; 8]>::try_from(bytes) {
        Ok(value) => Ok(u64::from_be_bytes(value)),
        Err(_) => Err(TranscriptError::TruncatedField),
    }
}

/// `nul-joined-utf8` 字段值：各段以单个 `0x00` 连接，段内不得含 NUL。
pub fn encode_nul_joined(parts: &[&str]) -> Result<Vec<u8>, TranscriptError> {
    let mut out = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        if part.contains('\0') {
            return Err(TranscriptError::EmbeddedNul);
        }
        if index > 0 {
            out.push(0);
        }
        out.extend_from_slice(part.as_bytes());
    }
    Ok(out)
}

/// 解码 `nul-joined-utf8` 字段值；空输入得到空列表。
pub fn decode_nul_joined(bytes: &[u8]) -> Result<Vec<&str>, TranscriptError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    bytes
        .split(|byte| *byte == 0)
        .map(|part| std::str::from_utf8(part).map_err(|_| TranscriptError::NotUtf8))
        .collect()
}
