//! 表驱动层：把某个 domain 的字段表与长度前缀 codec 组合成协议可直接调用的编解码。
//!
//! 本模块拥有"表怎么校验"的通用规则——字段数量、tag 成员、定长宽度——但**不拥有任何 domain
//! 字符串或 tag 取值**：每个协议 crate 导出自己的 `DOMAINS` 表（`sync-protocol` 对应
//! `docs/SYNC_PROTOCOL.md` §6.3，`node-link-protocol` 对应 `docs/NODE_LINK_PROTOCOL.md` §9.3/§9.4）。
//! 这样宽度表与校验逻辑只有一份实现，协议 crate 之间仍然互不依赖。

use crate::{FieldTag, TranscriptError as CodecError, decode, encode};

/// 该 domain 的 transcript 使用哪种证明方式；密钥、签名算法与 transcript 输入见对应协议文档。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proof {
    /// HMAC-SHA256。
    Hmac,
    /// ECDSA P-256 / SHA-256 签名（wire 上为 P1363 64 字节）。
    Signature,
}

/// 字段值的 wire 编码类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// u16be 整数，2 字节。
    U16be,
    /// u64be 整数，8 字节。
    U64be,
    /// UUID 解码后的 16 字节。
    Uuid16,
    /// SEC1 uncompressed point，65 字节。
    Sec1_65,
    /// 固定 16 字节。
    Bytes16,
    /// 固定 32 字节。
    Bytes32,
    /// UTF-8 文本的原始字节，长度由长度前缀给出。
    Utf8,
    /// 字符串数组以单个 `0x00` 连接。
    NulJoined,
}

impl FieldType {
    /// 定长字段的宽度；变长类型返回 `None`。
    pub fn fixed_width(self) -> Option<u32> {
        match self {
            FieldType::U16be => Some(2),
            FieldType::U64be => Some(8),
            FieldType::Uuid16 => Some(16),
            FieldType::Sec1_65 => Some(65),
            FieldType::Bytes16 => Some(16),
            FieldType::Bytes32 => Some(32),
            FieldType::Utf8 | FieldType::NulJoined => None,
        }
    }
}

/// 单个字段在 domain 中的位置与类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSpec {
    /// 字段 tag；同一 domain 内严格递增且唯一。
    pub tag: u16,
    /// registry 与协议文档中的字段名，用于跨语言与文档对齐。
    pub name: &'static str,
    /// 字段类型。
    pub ty: FieldType,
}

/// 一个 transcript domain 的完整字段集合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainSpec {
    /// domain 字符串，逐字节进入 transcript 前缀。
    pub domain: &'static str,
    /// 证明方式。
    pub proof: Proof,
    /// 字段表，顺序即 transcript 中的字段顺序。
    pub fields: &'static [FieldSpec],
}

/// 表驱动层错误：codec 结构错误，加上 domain、tag 与宽度校验错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error("domain {0} 不在该协议的 transcript 表里")]
    UnknownDomain(String),
    #[error("字段 tag {tag} 不是该 domain 的登记字段")]
    UnknownTag { tag: u16 },
    #[error("字段 tag {tag} 声明 {declared} 字节，登记宽度为 {expected}")]
    LengthMismatch {
        tag: u16,
        declared: u32,
        expected: u32,
    },
    #[error("字段数量不符：期望 {expected}，实际 {got}")]
    FieldCountMismatch { expected: usize, got: usize },
}

/// 在协议自己的表里按 domain 字符串查表。
pub fn domain_spec<'a>(domains: &'a [DomainSpec], domain: &str) -> Option<&'a DomainSpec> {
    domains.iter().find(|spec| spec.domain == domain)
}

/// 按 `spec` 的字段顺序编码各字段值；`values` 长度必须等于该 domain 的字段数。
///
/// 编码前先按 `fixed_width` 校验每个值，避免把宽度写错的字段发到 wire 上。
pub fn encode_transcript(spec: &DomainSpec, values: &[&[u8]]) -> Result<Vec<u8>, TableError> {
    if values.len() != spec.fields.len() {
        return Err(TableError::FieldCountMismatch {
            expected: spec.fields.len(),
            got: values.len(),
        });
    }

    let mut fields = Vec::with_capacity(values.len());
    for (field_spec, value) in spec.fields.iter().zip(values) {
        if let Some(width) = field_spec.ty.fixed_width() {
            let declared = u32::try_from(value.len()).map_err(|_| CodecError::FieldTooLong)?;
            if declared != width {
                return Err(TableError::LengthMismatch {
                    tag: field_spec.tag,
                    declared,
                    expected: width,
                });
            }
        }
        fields.push((FieldTag(field_spec.tag), *value));
    }

    Ok(encode(spec.domain, &fields)?)
}

/// 解码并校验 domain、字段 tag 成员与定长宽度，返回 wire 顺序的 `(tag, bytes)`。
///
/// 不校验字段集合与登记字段完全一致：签名覆盖整条 transcript，裁剪字段集合无法通过验签；
/// 该口径与 `scripts/check-contract-assets.mjs` 的解码器一致。
pub fn decode_transcript<'a>(
    bytes: &'a [u8],
    spec: &DomainSpec,
) -> Result<Vec<(u16, &'a [u8])>, TableError> {
    let decoded = decode(bytes)?;
    if decoded.domain != spec.domain {
        return Err(TableError::UnknownDomain(decoded.domain.to_string()));
    }

    let mut out = Vec::with_capacity(decoded.fields.len());
    for field in &decoded.fields {
        let field_spec = spec
            .fields
            .iter()
            .find(|candidate| candidate.tag == field.tag.0)
            .ok_or(TableError::UnknownTag { tag: field.tag.0 })?;
        if let Some(width) = field_spec.ty.fixed_width() {
            let declared =
                u32::try_from(field.bytes.len()).map_err(|_| CodecError::FieldTooLong)?;
            if declared != width {
                return Err(TableError::LengthMismatch {
                    tag: field.tag.0,
                    declared,
                    expected: width,
                });
            }
        }
        out.push((field.tag.0, field.bytes));
    }

    Ok(out)
}
