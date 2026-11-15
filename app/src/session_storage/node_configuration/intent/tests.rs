use super::*;
use crate::adapters::acp::node_configuration_profile::parse_profile;
fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
fn profile() -> LaunchProfileDraft {
    parse_profile(br#"{"format":1,"launch":{"executable_utf16le_base64":"YwA6AFwAYQAuAGUAeABlAA==","arguments_utf16le_base64":["ANg="],"environment":[]},"recipes":[{"wire":2,"mcp_servers":[],"additional_directories":[],"extra":{"future":{"decimal":1.2300e+09,"ordered":{"b":2,"a":1}}}}]}"#.to_vec()).unwrap()
}
fn roundtrip(d: CatalogDraft) -> Intent {
    let intent = Intent::normalize(d).unwrap();
    let request = CatalogRequestId::new(id(9));
    let encoded = intent.encode(request, id(10), 17).unwrap();
    let parsed = Intent::decode(encoded.to_vec(), request, id(10), 17, intent.action).unwrap();
    assert_eq!(intent.profile, parsed.profile);
    assert_eq!(intent.harness, parsed.harness);
    assert_eq!(intent.source, parsed.source);
    parsed
}
#[test]
fn node_configuration_intent_all_actions_keep_exact_profile_bytes() {
    let original = profile();
    let original_bytes = original.document().to_vec();
    let i = roundtrip(
        CatalogDraft::install_harness(
            "harness".into(),
            SourceSelection::New {
                name: "source".into(),
            },
            original,
        )
        .unwrap(),
    );
    assert_eq!(
        i.document.as_deref().map(Vec::as_slice),
        Some(original_bytes.as_slice())
    );
    assert!(matches!(
        i.body,
        Body::Install {
            source_name: Some(_),
            ..
        }
    ));
    let i = roundtrip(
        CatalogDraft::install_harness(
            "harness".into(),
            SourceSelection::Existing(id(1)),
            profile(),
        )
        .unwrap(),
    );
    assert_eq!(i.source, id(1));
    assert!(matches!(
        i.body,
        Body::Install {
            source_name: None,
            ..
        }
    ));
    let i = roundtrip(
        CatalogDraft::publish_profile(id(1), id(2), ProfilePublication::New, profile()).unwrap(),
    );
    assert_eq!(i.profile.revision(), 1);
    let i = roundtrip(
        CatalogDraft::publish_profile(
            id(1),
            id(2),
            ProfilePublication::Next {
                expected: ProfileKey::try_new(id(3), u64::MAX - 1).unwrap(),
            },
            profile(),
        )
        .unwrap(),
    );
    assert_eq!(i.profile.revision(), u64::MAX);
    assert_eq!(i.profile.id(), id(3));
    let i = roundtrip(
        CatalogDraft::bind_target_initial(
            id(4),
            id(2),
            id(1),
            ProfileKey::try_new(id(3), u64::MAX).unwrap(),
            id(5),
            "C:\\alias".into(),
            "C:\\canonical".into(),
        )
        .unwrap(),
    );
    assert!(i.document.is_none());
    assert!(matches!(i.body,Body::Bind{target,workspace,..} if target==id(4)&&workspace==id(5)));
}
#[test]
fn node_configuration_intent_rejects_metadata_substitution_and_unknowns() {
    let i = Intent::normalize(
        CatalogDraft::publish_profile(id(1), id(2), ProfilePublication::New, profile()).unwrap(),
    )
    .unwrap();
    let request = CatalogRequestId::new(id(9));
    let bytes = i.encode(request, id(10), 17).unwrap();
    for (r, a, g, action) in [
        (CatalogRequestId::new(id(8)), id(10), 17, i.action),
        (request, id(11), 17, i.action),
        (request, id(10), 18, i.action),
        (request, id(10), 17, CatalogAction::InstallHarness),
    ] {
        assert!(Intent::decode(bytes.to_vec(), r, a, g, action).is_err());
    }
    let original = String::from_utf8(bytes.to_vec()).unwrap();
    for candidate in [
        original.replacen('{', "{\"unknown\":null,", 1),
        original.replacen('{', "{\"format\":1,", 1),
        original.replacen("\"kind\":\"new\"", "\"kind\":\"new\",\"unknown\":0", 1),
        original.replacen(
            "\"profile_revision\":\"1\"",
            "\"profile_revision\":\"01\"",
            1,
        ),
    ] {
        assert!(Intent::decode(candidate.into_bytes(), request, id(10), 17, i.action).is_err());
    }
}
#[test]
fn node_configuration_intent_next_metadata_cannot_choose_different_profile() {
    let i = Intent::normalize(
        CatalogDraft::publish_profile(
            id(1),
            id(2),
            ProfilePublication::Next {
                expected: ProfileKey::try_new(id(3), u64::MAX - 1).unwrap(),
            },
            profile(),
        )
        .unwrap(),
    )
    .unwrap();
    let r = CatalogRequestId::new(id(9));
    let encoded = i.encode(r, id(10), 0).unwrap();
    let candidate = String::from_utf8(encoded.to_vec()).unwrap().replace(
        &format!("\"expected_profile_id\":\"{}\"", id(3).encode()),
        &format!("\"expected_profile_id\":\"{}\"", id(4).encode()),
    );
    assert!(matches!(
        Intent::decode(candidate.into_bytes(), r, id(10), 0, i.action),
        Err(CatalogError::CorruptState)
    ));
    assert!(
        CatalogDraft::publish_profile(
            id(1),
            id(2),
            ProfilePublication::Next {
                expected: ProfileKey::try_new(id(3), u64::MAX).unwrap()
            },
            profile()
        )
        .is_err()
    );
}

#[test]
fn strict_intent_decode_rejects_nested_decoded_duplicates() {
    let intent = Intent::normalize(
        CatalogDraft::publish_profile(id(1), id(2), ProfilePublication::New, profile()).unwrap(),
    )
    .unwrap();
    let request = CatalogRequestId::new(id(9));
    let encoded = intent.encode(request, id(10), 17).unwrap();
    assert!(Intent::decode(encoded.to_vec(), request, id(10), 17, intent.action).is_ok());
    let original = String::from_utf8(encoded.to_vec()).unwrap();
    for replacement in [r#""b":2,"b":3"#, r#""b":2,"\u0062":3"#] {
        let duplicate = original.replace(r#""b":2"#, replacement);
        assert_ne!(duplicate, original);
        assert!(matches!(
            Intent::decode(duplicate.into_bytes(), request, id(10), 17, intent.action),
            Err(CatalogError::InvalidConfiguration)
        ));
    }
}
