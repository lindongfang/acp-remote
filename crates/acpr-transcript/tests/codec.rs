//! `acpr-transcript` 的公开 API 测试：往返、每条结构错误的判定，以及无填充 base64url 的严格性。

use acpr_transcript::{
    CODEC_VERSION, DecodedField, DecodedTranscript, FieldTag, MAGIC, TranscriptError, decode,
    decode_base64url, decode_nul_joined, decode_u16be, decode_u64be, encode, encode_base64url,
    encode_nul_joined, encode_u16be, encode_u64be,
};

const DOMAIN: &str = "acp-remote/test-domain/v1";

fn raw_bytes(domain: &[u8], field_count: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.push(CODEC_VERSION);
    out.extend_from_slice(&(domain.len() as u16).to_be_bytes());
    out.extend_from_slice(domain);
    out.extend_from_slice(&field_count.to_be_bytes());
    out.extend_from_slice(body);
    out
}

fn raw(domain: &str, field_count: u16, body: &[u8]) -> Vec<u8> {
    raw_bytes(domain.as_bytes(), field_count, body)
}

fn field(tag: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&tag.to_be_bytes());
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
    out
}

fn valid() -> Vec<u8> {
    encode(
        DOMAIN,
        &[(FieldTag(1), &[0x00, 0x01]), (FieldTag(2), &[0xAA; 16])],
    )
    .expect("valid encoding")
}

#[test]
fn round_trip_preserves_domain_and_fields() {
    let bytes = valid();
    let decoded = decode(&bytes).expect("decodes");
    assert_eq!(
        decoded,
        DecodedTranscript {
            domain: DOMAIN,
            fields: vec![
                DecodedField {
                    tag: FieldTag(1),
                    bytes: &[0x00, 0x01]
                },
                DecodedField {
                    tag: FieldTag(2),
                    bytes: &[0xAA; 16]
                },
            ],
        }
    );
}

#[test]
fn empty_field_value_is_allowed() {
    let bytes = encode(DOMAIN, &[(FieldTag(4), &[])]).expect("valid encoding");
    let decoded = decode(&bytes).expect("decodes");
    assert_eq!(decoded.fields.len(), 1);
    assert_eq!(decoded.fields[0].bytes, &[] as &[u8]);
}

#[test]
fn encode_rejects_duplicate_tag_before_field_order() {
    assert_eq!(
        encode(DOMAIN, &[(FieldTag(3), &[1]), (FieldTag(3), &[2])]),
        Err(TranscriptError::DuplicateTag { current: 3 })
    );
}

#[test]
fn encode_rejects_descending_tag() {
    assert_eq!(
        encode(DOMAIN, &[(FieldTag(3), &[1]), (FieldTag(2), &[2])]),
        Err(TranscriptError::FieldOrder {
            previous: 3,
            current: 2
        })
    );
}

#[test]
fn decode_rejects_duplicate_tag_before_field_order() {
    let mut body = field(9, &[0xAA; 32]);
    body.extend_from_slice(&field(9, &[0xAA; 32]));
    assert_eq!(
        decode(&raw(DOMAIN, 2, &body)),
        Err(TranscriptError::DuplicateTag { current: 9 })
    );

    let mut body = field(10, &[0xAA; 32]);
    body.extend_from_slice(&field(9, &[0xAA; 32]));
    assert_eq!(
        decode(&raw(DOMAIN, 2, &body)),
        Err(TranscriptError::FieldOrder {
            previous: 10,
            current: 9
        })
    );
}

#[test]
fn decode_rejects_bad_magic() {
    let mut bytes = valid();
    bytes[0] = b'X';
    assert_eq!(decode(&bytes), Err(TranscriptError::BadMagic));

    assert_eq!(decode(&[]), Err(TranscriptError::BadMagic));
    assert_eq!(decode(b"ACP"), Err(TranscriptError::BadMagic));
}

#[test]
fn decode_rejects_bad_codec_version() {
    let mut bytes = valid();
    bytes[4] = 2;
    assert_eq!(
        decode(&bytes),
        Err(TranscriptError::BadCodecVersion { found: 2 })
    );

    // 版本字节本身缺失时同样归类为版本错误。
    assert_eq!(
        decode(&MAGIC),
        Err(TranscriptError::BadCodecVersion { found: 0 })
    );
}

#[test]
fn decode_rejects_truncated_domain() {
    // 只有 magic + version，没有 domain 长度前缀。
    assert_eq!(
        decode(&[b'A', b'C', b'P', b'R', CODEC_VERSION]),
        Err(TranscriptError::TruncatedDomain)
    );

    // 长度前缀声明的 domain 超出缓冲区。
    let mut bytes = raw(DOMAIN, 0, &[]);
    bytes.truncate(bytes.len() - 3);
    assert_eq!(decode(&bytes), Err(TranscriptError::TruncatedDomain));
}

#[test]
fn decode_rejects_invalid_utf8_domain() {
    let bytes = raw_bytes(&[0xFF, 0xFE], 0, &[]);
    assert_eq!(decode(&bytes), Err(TranscriptError::NotUtf8));
}

#[test]
fn decode_rejects_truncated_field() {
    // 字段数量声明为 1，但字段头不完整。
    assert_eq!(
        decode(&raw(DOMAIN, 1, &[0x00])),
        Err(TranscriptError::TruncatedField)
    );
    // 字段头完整但字段体被截断。
    let mut body = field(1, &[0xAA; 4]);
    body.truncate(body.len() - 2);
    assert_eq!(
        decode(&raw(DOMAIN, 1, &body)),
        Err(TranscriptError::TruncatedField)
    );
}

#[test]
fn decode_rejects_trailing_bytes() {
    let mut bytes = valid();
    bytes.push(0x00);
    assert_eq!(decode(&bytes), Err(TranscriptError::TrailingBytes));
}

#[test]
fn encode_rejects_invalid_domain() {
    assert_eq!(
        encode("", &[(FieldTag(1), &[1])]),
        Err(TranscriptError::InvalidDomain)
    );
    assert_eq!(
        encode("bad\0domain", &[(FieldTag(1), &[1])]),
        Err(TranscriptError::InvalidDomain)
    );
}

#[test]
fn base64url_round_trip_is_unpadded() {
    let bytes = [0x00_u8, 0x01, 0x02, 0xFB, 0xFF];
    let text = encode_base64url(&bytes);
    assert!(!text.contains('='), "输出不得带填充：{text}");
    assert_eq!(decode_base64url(&text), Ok(bytes.to_vec()));
}

#[test]
fn base64url_rejects_padding() {
    let text = format!("{}=", encode_base64url(&[0xAA; 32]));
    assert_eq!(decode_base64url(&text), Err(TranscriptError::Base64Url));
}

#[test]
fn base64url_rejects_standard_alphabet() {
    assert_eq!(
        decode_base64url("AAA+"),
        Err(TranscriptError::Base64Url),
        "标准 base64 的 + 必须被拒"
    );
    assert_eq!(
        decode_base64url("AAA/"),
        Err(TranscriptError::Base64Url),
        "标准 base64 的 / 必须被拒"
    );
}

#[test]
fn base64url_rejects_non_canonical_and_empty() {
    assert_eq!(
        decode_base64url(""),
        Err(TranscriptError::Base64Url),
        "空串不是合法 base64url"
    );
    assert_eq!(
        decode_base64url("A"),
        Err(TranscriptError::Base64Url),
        "长度为 1 mod 4 非法"
    );
    // "AAB" 与 "AAA" 都表示 [0, 0]，只有 "AAA" 无多余非零位；非规范编码必须被拒，
    // 否则同一签名字节串会有多种文本表示。
    assert_eq!(
        decode_base64url("AAB"),
        Err(TranscriptError::Base64Url),
        "非规范尾部位必须被拒"
    );
    assert_eq!(decode_base64url("AAA"), Ok(vec![0, 0]));
}

#[test]
fn integer_field_helpers_round_trip() {
    assert_eq!(decode_u16be(&encode_u16be(0x0102)), Ok(0x0102));
    assert_eq!(
        decode_u64be(&encode_u64be(0x0102_0304_0506_0708)),
        Ok(0x0102_0304_0506_0708)
    );
    assert_eq!(decode_u16be(&[0x01]), Err(TranscriptError::TruncatedField));
    assert_eq!(
        decode_u64be(&[0x00; 7]),
        Err(TranscriptError::TruncatedField)
    );
}

#[test]
fn nul_joined_round_trip_and_rejections() {
    let joined = encode_nul_joined(&["core.snapshot.v1", "acp.raw-payload.v1"]).expect("joins");
    assert_eq!(joined, b"core.snapshot.v1\0acp.raw-payload.v1".to_vec());
    assert_eq!(
        decode_nul_joined(&joined),
        Ok(vec!["core.snapshot.v1", "acp.raw-payload.v1"])
    );
    assert_eq!(decode_nul_joined(&[]), Ok(Vec::new()));
    assert_eq!(
        encode_nul_joined(&["good", "bad\0part"]),
        Err(TranscriptError::EmbeddedNul)
    );
    assert_eq!(decode_nul_joined(&[0xFF]), Err(TranscriptError::NotUtf8));
}
