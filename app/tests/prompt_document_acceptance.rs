//! Independent PA01–03/08 data contract. A normal integration-test dependency.
use acp_remote::{
    adapters::acp::{
        AcpError, PromptDocument,
        native_dispatch::AcpNativeAdapter,
        raw_json::{OwnedRawValue, RawJsonError},
    },
    application::{
        native_dispatch::NativeProtocolPort, queue_owner::RetainPrompt,
        request_registry::ValidatedBodyPort,
    },
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[test]
fn pa01_normal_dependency_combines_only_existing_data_and_native_traits() {
    fn validator<V: ValidatedBodyPort<Raw = PromptDocument, Body = PromptDocument>>(_: &V) {}
    fn protocol<P: NativeProtocolPort<Retained = OwnedRawValue>>(_: &P) {}
    fn retain<R: RetainPrompt<PromptDocument, Retained = OwnedRawValue>>(_: &R) {}
    let adapter = Arc::new(AcpNativeAdapter::default());
    validator(&adapter);
    protocol(adapter.as_ref());
    retain(adapter.as_ref());
    let _retainer: Arc<dyn RetainPrompt<PromptDocument, Retained = OwnedRawValue>> =
        adapter.clone();
    let doc = PromptDocument::parse(b"[]".to_vec()).expect("sealed array");
    let (body, hash) = adapter.validate(&doc).expect("actual production validator");
    assert_eq!(body.uploaded_bytes(), b"[]");
    assert_eq!(hash, digest(b"[]"));
}

#[test]
fn pa02_full_upload_moves_without_copy_and_bound_body_shares_the_original_bytes() {
    let uploaded = b" \n[{\"type\":\"text\",\"text\":\"x\\u0061\",\"unknown\":9007199254740993,\"n\":1e+009}]\t".to_vec();
    let original_address = uploaded.as_ptr();
    let original = uploaded.clone();
    let doc = PromptDocument::parse(uploaded).expect("complete document");
    assert_eq!(doc.uploaded_bytes(), original);
    assert_eq!(doc.uploaded_bytes().as_ptr(), original_address);
    assert_eq!(
        doc.prompt_fragment().as_bytes(),
        &original[2..original.len() - 1]
    );
    let adapter = Arc::new(AcpNativeAdapter::default());
    let (body, actual) = adapter.validate(&doc).expect("production hash");
    assert_eq!(actual, digest(&original));
    assert_ne!(actual, digest(doc.prompt_fragment().as_bytes()));
    assert_eq!(body.uploaded_bytes().as_ptr(), original_address);
    assert_eq!(
        body.prompt_fragment().as_ptr(),
        doc.prompt_fragment().as_ptr()
    );
    drop(doc);
    assert_eq!(body.uploaded_bytes(), original);
    let clone = body.clone();
    drop(body);
    assert_eq!(clone.uploaded_bytes(), original);
}

#[test]
fn pa02_server_hash_uses_full_bytes_not_semantic_json_or_root_only() {
    let variants = [
        r#"[{"type":"text","text":"a"}]"#,
        " \n[{\"type\":\"text\",\"text\":\"a\"}]\t",
        r#"[{"type":"text","text":"\u0061"}]"#,
        r#"[{"text":"a","type":"text"}]"#,
    ];
    let adapter = Arc::new(AcpNativeAdapter::default());
    let hashes: Vec<_> = variants
        .iter()
        .map(|v| {
            let d = PromptDocument::parse(v.as_bytes().to_vec()).expect("valid variant");
            let (_, hash) = adapter.validate(&d).expect("production port");
            assert_eq!(hash, digest(v.as_bytes()));
            hash
        })
        .collect();
    for (i, left) in hashes.iter().enumerate() {
        for right in &hashes[i + 1..] {
            assert_ne!(left, right);
        }
    }
    let doc = PromptDocument::parse(variants[0].as_bytes().to_vec()).expect("independent parse");
    assert_eq!(adapter.validate(&doc).expect("same bytes").1, hashes[0]);
}

#[test]
fn pa03_known_shapes_preserve_existing_width_and_unknown_fields() {
    for bytes in [
        "[]",
        r#"[{"type":"text","text":"","optional":{"arbitrary":true}}]"#,
        r#"[{"type":"resource_link","uri":"not-normalized","name":""}]"#,
        r#"[{"type":"image","data":"not-base64","mimeType":"unknown"}]"#,
        r#"[{"type":"audio","data":"","mimeType":""}]"#,
        r#"[{"type":"resource","resource":{"uri":"x","text":"a","blob":"b"}}]"#,
        r#"[{"type":"resource","resource":{"uri":"x","text":4,"blob":"b"}}]"#,
        r#"[{"type":"resource","resource":{"uri":"x","text":"a","blob":null}}]"#,
        r#"[{"type":"future","deep":[{"noKnownFields":null}],"big":18446744073709551616}]"#,
    ] {
        let d = PromptDocument::parse(bytes.as_bytes().to_vec()).expect("shape-only accepts");
        assert_eq!(d.uploaded_bytes(), bytes.as_bytes());
        assert_eq!(d.prompt_fragment(), bytes);
    }
}

#[test]
fn pa03_invalid_shapes_and_complete_json_errors_never_produce_a_sealed_document() {
    let vectors = [
        "null",
        "{}",
        "1",
        "[null]",
        "[{}]",
        r#"[{"type":3}]"#,
        r#"[{"type":"text"}]"#,
        r#"[{"type":"text","text":null}]"#,
        r#"[{"type":"resource_link","uri":4,"name":"x"}]"#,
        r#"[{"type":"resource_link","uri":"x"}]"#,
        r#"[{"type":"image","data":null,"mimeType":"x"}]"#,
        r#"[{"type":"audio","data":"x","mimeType":3}]"#,
        r#"[{"type":"resource","resource":[]}]"#,
        r#"[{"type":"resource","resource":{"uri":"x","text":false,"blob":null}}]"#,
        r#"[{"type":"resource","resource":{"text":"a"}}]"#,
        r#"[{"type":"future","n":01}]"#,
        r#"[{"type":"future","n":NaN}]"#,
        r#"[{"type":"future","s":"\uD800"}]"#,
        r#"[{"type":"future","s":"\q"}]"#,
        r#"[{"type":"text","text":"x","text":"y"}]"#,
        r#"[{"type":"text","\u0074ype":"future","text":"x"}]"#,
        r#"[{"type":"future","u":{"a":1,"\u0061":2}}]"#,
        "[] []",
        "[",
        "",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .chain(std::iter::once(vec![b'[', 0xff, b']']));
    for (index, bytes) in vectors.enumerate() {
        match PromptDocument::parse(bytes) {
            Err(AcpError::InvalidMessage) => {}
            Err(other) => panic!("PA03 vector {index} wrong category: {other}"),
            Ok(_) => panic!("PA03 vector {index} unexpectedly sealed"),
        }
    }
}

#[test]
fn pa03_flat_owned_unknown_depth_and_number_lexemes_remain_data() {
    let mut bytes = br#"[{"type":"future","unknown":"#.to_vec();
    bytes.extend(std::iter::repeat_n(b'[', 512));
    bytes.extend_from_slice(
        b"184467440737095516160000000000000000000000000000000000000000000000000001e+9999",
    );
    bytes.extend(std::iter::repeat_n(b']', 512));
    bytes.extend_from_slice(b"}]");
    let expected = digest(&bytes);
    let d = PromptDocument::parse(bytes).expect("no platform depth or number conversion");
    let adapter = Arc::new(AcpNativeAdapter::default());
    assert_eq!(adapter.validate(&d).expect("full bytes").1, expected);
    assert!(d.prompt_fragment().contains("e+9999"));
}

#[test]
fn pa08_error_mapping_is_static_and_does_not_embed_body_or_parser_diagnostics() {
    for raw in [
        RawJsonError::InvalidJson,
        RawJsonError::DuplicateField,
        RawJsonError::InvalidUnicode,
        RawJsonError::WrongType,
    ] {
        assert_eq!(AcpError::from(raw), AcpError::InvalidMessage);
    }
    assert_eq!(
        AcpError::from(RawJsonError::Allocation),
        AcpError::Allocation
    );
    let canary = "PA_SECRET_CANARY_do_not_echo";
    let bytes = format!(r#"[{{"type":"text","text":{{"{canary}":1}}}}]"#).into_bytes();
    let Err(error) = PromptDocument::parse(bytes) else {
        panic!("must reject wrong text shape");
    };
    assert!(!format!("{error} {error:?}").contains(canary));
    assert_eq!(error, AcpError::InvalidMessage);
    // Enum conversion is not physical allocation-failure evidence.
}
