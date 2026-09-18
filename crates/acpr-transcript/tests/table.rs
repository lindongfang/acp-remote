//! 表驱动层单测：字段数量、定长宽度、domain 归属与 tag 成员。
//!
//! 真实 domain 的逐字节复算在 `sync-protocol` / `node-link-protocol` 的固定向量测试里，
//! 这里用一张自造的小表覆盖错误路径。

use acpr_transcript::table::{
    DomainSpec, FieldSpec, FieldType, Proof, TableError, decode_transcript, encode_transcript,
};
use acpr_transcript::{FieldTag, encode};

const DEMO_FIELDS: [FieldSpec; 3] = [
    FieldSpec {
        tag: 1,
        name: "counter",
        ty: FieldType::U64be,
    },
    FieldSpec {
        tag: 2,
        name: "nonce",
        ty: FieldType::Bytes32,
    },
    FieldSpec {
        tag: 3,
        name: "features",
        ty: FieldType::NulJoined,
    },
];

const DEMO: DomainSpec = DomainSpec {
    domain: "acp-remote/demo/v1",
    proof: Proof::Hmac,
    fields: &DEMO_FIELDS,
};

const OTHER: DomainSpec = DomainSpec {
    domain: "acp-remote/other/v1",
    proof: Proof::Signature,
    fields: &DEMO_FIELDS,
};

fn demo_values() -> Vec<Vec<u8>> {
    vec![
        1u64.to_be_bytes().to_vec(),
        vec![7u8; 32],
        b"a.b\0c.d".to_vec(),
    ]
}

#[test]
fn round_trip_walks_the_table_in_order() {
    let owned = demo_values();
    let values: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();

    let bytes = encode_transcript(&DEMO, &values).expect("表驱动编码");
    let decoded = decode_transcript(&bytes, &DEMO).expect("表驱动解码");

    assert_eq!(
        decoded,
        vec![
            (1, owned[0].as_slice()),
            (2, owned[1].as_slice()),
            (3, owned[2].as_slice())
        ]
    );
}

#[test]
fn encode_rejects_wrong_field_count() {
    let values: Vec<&[u8]> = vec![&[0u8; 8][..]];
    assert_eq!(
        encode_transcript(&DEMO, &values),
        Err(TableError::FieldCountMismatch {
            expected: 3,
            got: 1
        })
    );
}

#[test]
fn encode_rejects_wrong_fixed_width() {
    let owned = [1u64.to_be_bytes().to_vec(), vec![7u8; 31], b"a".to_vec()];
    let values: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();

    assert_eq!(
        encode_transcript(&DEMO, &values),
        Err(TableError::LengthMismatch {
            tag: 2,
            declared: 31,
            expected: 32
        })
    );
}

#[test]
fn decode_rejects_foreign_domain() {
    let owned = demo_values();
    let values: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    let bytes = encode_transcript(&DEMO, &values).expect("表驱动编码");

    assert_eq!(
        decode_transcript(&bytes, &OTHER),
        Err(TableError::UnknownDomain("acp-remote/demo/v1".to_string()))
    );
}

#[test]
fn decode_rejects_unregistered_tag() {
    let bytes = encode(DEMO.domain, &[(FieldTag(9), &[1u8][..])]).expect("codec 编码");

    assert_eq!(
        decode_transcript(&bytes, &DEMO),
        Err(TableError::UnknownTag { tag: 9 })
    );
}

#[test]
fn decode_rejects_wire_width_mismatch() {
    let bytes = encode(DEMO.domain, &[(FieldTag(2), &[7u8; 31][..])]).expect("codec 编码");

    assert_eq!(
        decode_transcript(&bytes, &DEMO),
        Err(TableError::LengthMismatch {
            tag: 2,
            declared: 31,
            expected: 32
        })
    );
}
