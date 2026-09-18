//! 固定向量复算：正向量必须逐字节重编码一致，transcript 负向量必须按声明的错误被拒。
//! 公钥负向量依赖 p256，属于 `identity-auth` 的范围，本增量显式跳过并计数。

mod support;

use acpr_transcript::table::{
    DomainSpec, TableError, decode_transcript, domain_spec, encode_transcript,
};
use node_link_protocol::domains::DOMAINS;
use sha2::{Digest, Sha256};

const MANIFEST: &str = "fixtures/node-link/v1/manifest.json";
const FIXTURE_ROOT: &str = "fixtures/node-link/v1/";
const PROTOCOL: &str = "node_link";
const POSITIVE_EXPECTED: usize = 6;
const TRANSCRIPT_NEGATIVES_EXPECTED: usize = 5;
const PUBLIC_KEY_NEGATIVES_EXPECTED: usize = 2;

fn values_from_input(
    spec: &DomainSpec,
    input: &serde_json::Value,
    registry_entry: &serde_json::Value,
) -> Vec<Vec<u8>> {
    let fields = registry_entry["fields"]
        .as_array()
        .expect("registry fields");
    spec.fields
        .iter()
        .map(|field_spec| {
            let registry_field = fields
                .iter()
                .find(|field| field["tag"].as_u64() == Some(field_spec.tag as u64))
                .unwrap_or_else(|| panic!("registry 缺少 tag {} 的字段", field_spec.tag));
            support::input_bytes(input, registry_field)
        })
        .collect()
}

#[test]
fn positive_vectors_re_encode_byte_exactly() {
    let registry = support::registry();
    let mut checked = 0;

    for relative in support::vectors(MANIFEST) {
        let vector = support::read_json(&support::repo_path(&format!("{FIXTURE_ROOT}{relative}")));
        let Some(expected) = vector
            .get("expected")
            .and_then(|expected| expected.get("transcriptBase64url"))
        else {
            continue;
        };

        let domain = vector["domain"].as_str().expect("domain");
        let spec =
            domain_spec(&DOMAINS, domain).unwrap_or_else(|| panic!("{relative}：domain 未登记"));
        let entry = support::registry_domain_entry(&registry, domain).expect("registry entry");

        let owned = values_from_input(spec, &vector["input"], &entry);
        let values: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
        let encoded = encode_transcript(spec, &values)
            .unwrap_or_else(|error| panic!("{relative}：编码失败 {error}"));

        assert_eq!(
            acpr_transcript::encode_base64url(&encoded),
            expected.as_str().expect("expected transcript"),
            "{relative}：重编码结果与固定向量不一致"
        );

        let digest_hex: String = Sha256::digest(&encoded)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(
            digest_hex,
            vector["expected"]["transcriptSha256Hex"]
                .as_str()
                .expect("sha256"),
            "{relative}：transcript 摘要不一致"
        );

        checked += 1;
    }

    assert_eq!(checked, POSITIVE_EXPECTED, "复算的正向量数量与预期不符");
    println!("{PROTOCOL}: {checked} 个正向量逐字节一致");
}

#[test]
fn transcript_negatives_are_rejected_with_declared_error() {
    let mut rejected = 0;
    let mut skipped_public_keys = 0;

    for relative in support::vectors(MANIFEST) {
        let vector = support::read_json(&support::repo_path(&format!("{FIXTURE_ROOT}{relative}")));

        let Some(text) = vector
            .get("malformedTranscriptBase64url")
            .and_then(|value| value.as_str())
        else {
            if vector.get("malformedPublicKeyBase64url").is_some() {
                skipped_public_keys += 1;
            }
            continue;
        };

        let expected = vector["expectedError"].as_str().expect("expectedError");
        let domain = vector["domain"].as_str().expect("domain");
        let spec =
            domain_spec(&DOMAINS, domain).unwrap_or_else(|| panic!("{relative}：domain 未登记"));

        let error = match acpr_transcript::decode_base64url(text) {
            Ok(bytes) => decode_transcript(&bytes, spec)
                .err()
                .unwrap_or_else(|| panic!("{relative}：畸形 transcript 被接受")),
            Err(error) => TableError::from(error),
        };

        assert_eq!(
            support::error_name(&error),
            expected,
            "{relative}：错误分类与声明不一致"
        );
        rejected += 1;
    }

    assert_eq!(
        rejected, TRANSCRIPT_NEGATIVES_EXPECTED,
        "被拒的 transcript 负向量数量与预期不符"
    );
    assert_eq!(
        skipped_public_keys, PUBLIC_KEY_NEGATIVES_EXPECTED,
        "跳过的公钥负向量数量与预期不符"
    );
    println!(
        "{PROTOCOL}: {rejected} 个 transcript 负向量按声明错误被拒，跳过 {skipped_public_keys} 个公钥负向量"
    );
}
