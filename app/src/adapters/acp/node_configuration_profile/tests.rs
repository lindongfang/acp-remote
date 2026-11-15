use super::*;
use crate::domain::native_catalog::{CatalogDraft, ProfilePublication, SourceSelection};
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
fn document(recipes: &str) -> Vec<u8> {
    format!(r#"{{"format":1,"launch":{{"executable_utf16le_base64":"YQAA2A==","arguments_utf16le_base64":["AACQ3A=="],"environment":[{{"name_utf16le_base64":"QQA=","value_utf16le_base64":"ANg="}}]}},"recipes":{recipes}}}"#).into_bytes()
}
fn empty(wire: u64) -> String {
    format!(r#"{{"wire":{wire},"mcp_servers":[],"additional_directories":[],"extra":{{}}}}"#)
}
#[test]
fn keeps_original_document_and_full_utf16_code_units() {
    let input = document(&format!("[{},{}]", empty(1), empty(2)));
    let draft = parse_profile(input.clone()).expect("profile");
    assert_eq!(draft.document(), input);
    #[cfg(windows)]
    {
        let decoded = decode_profile(input).expect("decode");
        assert_eq!(
            decoded.executable.encode_wide().collect::<Vec<_>>(),
            vec![0x0061, 0xd800]
        );
        assert_eq!(
            decoded.arguments[0].encode_wide().collect::<Vec<_>>(),
            vec![0x0000, 0xdc90]
        );
        assert_eq!(
            decoded.environment[0].1.encode_wide().collect::<Vec<_>>(),
            vec![0xd800]
        );
        assert_eq!(decoded.recipes.len(), 2);
    }
}
#[test]
fn preserves_raw_numbers_unknown_fields_and_object_order() {
    let recipe = r#"{"wire":1,"mcp_servers":[{"name":"m","command":"exe","args":[],"env":[],"unknown":{"n":900719925474099312345678901234567890,"exponent":1e+9999}}],"additional_directories":[],"extra":{"z":{"a":2,"b":1},"n":18446744073709551616000}}"#;
    let input = document(&format!("[{recipe}]"));
    let profile = parse_profile(input.clone()).expect("profile");
    assert_eq!(profile.document(), input);
    #[cfg(windows)]
    {
        let decoded = decode_profile(input).expect("decode");
        assert_eq!(
            decoded.recipes[0].extra.as_raw().fragment(),
            r#"{"z":{"a":2,"b":1},"n":18446744073709551616000}"#
        );
    }
}
#[test]
fn rejects_decoded_duplicate_node_fields_and_unknown_outer_fields() {
    let input = String::from_utf8(document("[]")).expect("utf8");
    for input in [
        input.replace(r#""format":1"#, r#""format":1,"fo\u0072mat":1"#),
        input.replace(r#""format":1"#, r#""format":1,"unknown":null"#),
    ] {
        assert!(matches!(
            parse_profile(input.into_bytes()),
            Err(CatalogError::InvalidConfiguration)
        ));
    }
}
#[test]
fn requires_complete_known_recipe_and_canonical_base64() {
    let input = String::from_utf8(document(&format!("[{}]", empty(1)))).expect("utf8");
    for changed in [
        input.replace("YQAA2A==", "YQAA2A"),
        input.replace("YQAA2A==", "YQ=="),
        input.replace("YQAA2A==", "YQAA2B=="),
        input.replace(r#""mcp_servers":[],"#, ""),
        input.replace(r#""extra":{}"#, r#""extra":{"cwd":"x"}"#),
    ] {
        assert!(matches!(
            parse_profile(changed.into_bytes()),
            Err(CatalogError::InvalidConfiguration)
        ));
    }
    assert!(parse_profile(document(&format!("[{},{}]", empty(1), empty(1)))).is_err());
    assert!(parse_profile(document(&format!("[{}]", empty(3)))).is_err());
    assert!(parse_profile(document("[]")).is_ok());
}
#[test]
fn pure_ids_and_revisions_do_not_lose_u64_range() {
    let id = EntityId::from_bytes([3; 16]);
    let key = ProfileKey::try_new(id, u64::MAX).expect("key");
    assert_eq!(key.revision(), u64::MAX);
    assert!(ProfileKey::try_new(id, 0).is_err());
    assert!(
        CatalogDraft::publish_profile(
            id,
            id,
            ProfilePublication::Next { expected: key },
            parse_profile(document("[]")).expect("profile")
        )
        .is_err()
    );
    let request = crate::domain::native_catalog::CatalogRequestId::new(id);
    assert_eq!(
        crate::domain::native_catalog::CatalogRequestId::decode(&request.encode()).expect("id"),
        request
    );
    assert!(
        CatalogDraft::install_harness(
            "valid".into(),
            SourceSelection::New {
                name: "source".into()
            },
            parse_profile(document("[]")).expect("profile")
        )
        .is_ok()
    );
}

#[test]
fn strict_profile_entry_points_reject_nested_decoded_duplicates() {
    let unique = document(
        r#"[{"wire":2,"mcp_servers":[],"additional_directories":[],"extra":{"future":{"k":1}}}]"#,
    );
    let original = String::from_utf8(unique.clone()).unwrap();
    assert!(parse_profile(unique.clone()).is_ok());
    #[cfg(windows)]
    assert!(decode_profile(unique).is_ok());
    for replacement in [r#""k":1,"k":2"#, r#""k":1,"\u006b":2"#] {
        let duplicate = original.replace(r#""k":1"#, replacement).into_bytes();
        assert!(matches!(
            parse_profile(duplicate.clone()),
            Err(CatalogError::InvalidConfiguration)
        ));
        #[cfg(windows)]
        assert!(matches!(
            decode_profile(duplicate),
            Err(CatalogError::InvalidConfiguration)
        ));
    }
}
