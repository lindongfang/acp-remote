//! Rust 表与 `compatibility/transcripts/v1/transcripts.json` 的逐项相等门禁：
//! 表是 registry 的转录，任何一侧单独改动都必须让这个测试失败。

mod support;

use acpr_transcript::table::{FieldType, Proof, domain_spec};
use node_link_protocol::domains::DOMAINS;

const PROTOCOL: &str = "node_link";

fn field_type_of(name: &str) -> FieldType {
    match name {
        "u16be" => FieldType::U16be,
        "u64be" => FieldType::U64be,
        "uuid16" => FieldType::Uuid16,
        "sec1-65" => FieldType::Sec1_65,
        "bytes16" => FieldType::Bytes16,
        "bytes32" => FieldType::Bytes32,
        "utf8" => FieldType::Utf8,
        "nul-joined-utf8" => FieldType::NulJoined,
        other => panic!("registry 里出现未知字段类型：{other}"),
    }
}

#[test]
fn domains_match_registry() {
    let registered = support::registry_domains(PROTOCOL);
    assert_eq!(
        DOMAINS.len(),
        registered.len(),
        "domain 数量与 registry 不一致"
    );

    for (spec, entry) in DOMAINS.iter().zip(&registered) {
        assert_eq!(spec.domain, entry["domain"].as_str().expect("domain"));

        let proof = match entry["proof"].as_str().expect("proof") {
            "hmac" => Proof::Hmac,
            "signature" => Proof::Signature,
            other => panic!("registry 里出现未知 proof：{other}"),
        };
        assert_eq!(spec.proof, proof, "{}：proof 不一致", spec.domain);

        let fields = entry["fields"].as_array().expect("fields");
        assert_eq!(
            spec.fields.len(),
            fields.len(),
            "{}：字段数不一致",
            spec.domain
        );
        for (field_spec, field) in spec.fields.iter().zip(fields) {
            assert_eq!(
                field_spec.tag as u64,
                field["tag"].as_u64().expect("tag"),
                "{}：tag 不一致",
                spec.domain
            );
            assert_eq!(
                field_spec.name,
                field["name"].as_str().expect("name"),
                "{}：字段名不一致",
                spec.domain
            );
            assert_eq!(
                field_spec.ty,
                field_type_of(field["type"].as_str().expect("type")),
                "{}：字段类型不一致",
                spec.domain
            );
        }
    }
}

#[test]
fn domain_lookup_covers_every_registered_domain() {
    for spec in DOMAINS.iter() {
        assert_eq!(
            domain_spec(&DOMAINS, spec.domain).map(|found| found.domain),
            Some(spec.domain)
        );
    }
    assert!(domain_spec(&DOMAINS, "acp-remote/not-registered/v1").is_none());
}

#[test]
fn tags_are_strictly_ascending() {
    for spec in DOMAINS.iter() {
        for pair in spec.fields.windows(2) {
            assert!(
                pair[0].tag < pair[1].tag,
                "{}：字段 tag 必须严格升序",
                spec.domain
            );
        }
    }
}
