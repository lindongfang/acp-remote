use super::*;

#[test]
fn original_storage_and_order_survive_nested_owned_values() {
    let input = b" \r\n{\"z\": [1, {\"\\u0061\":2}], \"a\":3}\t ".to_vec();
    let document = RawDocument::parse(input.clone()).unwrap();
    assert_eq!(document.bytes(), input);
    let clone = document.clone();
    assert_eq!(document.bytes().as_ptr(), clone.bytes().as_ptr());
    let nested = {
        let mut members = document.root().object().unwrap().iter();
        assert_eq!(members.len(), 2);
        assert_eq!(members.next_back().unwrap().0, "a");
        members
            .next()
            .unwrap()
            .1
            .array()
            .unwrap()
            .get(1)
            .unwrap()
            .to_owned()
    };
    let pointer = nested.as_ref().fragment().as_ptr();
    drop(document);
    drop(clone);
    assert_eq!(nested.as_ref().fragment().as_ptr(), pointer);
    assert_eq!(
        nested
            .as_ref()
            .object()
            .unwrap()
            .get("a")
            .expect("unique decoded field")
            .unwrap()
            .integer(),
        Some(2)
    );
}

#[test]
fn exact_numbers_and_primitive_types_are_not_coerced() {
    for (source, signed, unsigned) in [
        ("-0", Some(0), None),
        ("0", Some(0), Some(0)),
        ("-9223372036854775808", Some(i64::MIN), None),
        ("18446744073709551615", None, Some(u64::MAX)),
        ("18446744073709551616", None, None),
        ("1e999999", None, None),
        ("1.0", None, None),
        ("1e0", None, None),
    ] {
        let document = RawDocument::parse(source.as_bytes().to_vec()).unwrap();
        assert_eq!(document.root().kind(), RawKind::Number);
        assert_eq!(document.root().fragment(), source);
        assert_eq!(document.root().integer(), signed);
        assert_eq!(document.root().unsigned(), unsigned);
        assert_eq!(document.root().string(), Err(RawJsonError::WrongType));
    }
    for (source, boolean, null) in [
        ("true", Some(true), false),
        ("false", Some(false), false),
        ("null", None, true),
    ] {
        let document = RawDocument::parse(source.as_bytes().to_vec()).unwrap();
        assert_eq!(document.root().boolean(), boolean);
        assert_eq!(document.root().is_null(), null);
        assert_eq!(document.root().integer(), None);
    }
}

#[test]
fn compact_preserves_escapes_number_lexemes_and_string_whitespace() {
    let source = "{\n \"x\" : [ -0, 1E+09, \"a b\\n\\\\\\\"[]{}\u{a0}\u{2028}\" ]\r\n}";
    let document = RawDocument::parse(source.as_bytes().to_vec()).unwrap();
    assert_eq!(
        document.root().compact().unwrap(),
        "{\"x\":[-0,1E+09,\"a b\\n\\\\\\\"[]{}\u{a0}\u{2028}\"]}"
    );
    assert_eq!(document.bytes(), source.as_bytes());
}

#[test]
fn decoded_duplicates_survive_until_queried_or_strictly_validated() {
    for source in [
        r#"{"a":1,"\u0061":2}"#,
        r#"[{"child":{"x":1,"x":2}}]"#,
        r#"{"\uD83D\uDE00":1,"😀":2}"#,
    ] {
        let document = RawDocument::parse(source.as_bytes().to_vec()).unwrap();
        assert_eq!(document.bytes(), source.as_bytes());
        assert_eq!(
            document.require_unique_fields(),
            Err(RawJsonError::DuplicateField)
        );
    }
    let document =
        RawDocument::parse(r#"[{"a":1},{"\u0061":2},{"é":1,"e\u0301":2}]"#.as_bytes().to_vec())
            .unwrap();
    assert_eq!(document.root().array().unwrap().len(), 3);
}

#[test]
fn all_strings_validate_surrogates_in_source_order() {
    for source in [
        r#""\uD800""#,
        r#"{"unknown":["\uDC00"]}"#,
        r#"{"\uDBFF":0}"#,
        r#""\uD800\\uDC00""#,
    ] {
        assert!(matches!(
            RawDocument::parse(source.as_bytes().to_vec()),
            Err(RawJsonError::InvalidUnicode)
        ));
    }
    let document = RawDocument::parse(br#""\uD800\uDC00\uDBFF\uDFFF\\uD800""#.to_vec()).unwrap();
    assert_eq!(
        document.root().string().unwrap(),
        "\u{10000}\u{10ffff}\\uD800"
    );
    assert!(matches!(
        RawDocument::parse(br#"{"a":1,"a":2,"x":"\uD800"}"#.to_vec()),
        Err(RawJsonError::InvalidUnicode)
    ));
    assert!(matches!(
        RawDocument::parse(br#"{"x":"\uD800","a":1,"a":2}"#.to_vec()),
        Err(RawJsonError::InvalidUnicode)
    ));
}

#[test]
fn complete_syntax_and_utf8_precede_key_and_unicode_checks() {
    for source in [
        b"".as_slice(),
        br#"{"a":1,"a":2,}"#,
        br#"["\uD800",]"#,
        b"0 1",
        b"\xff",
        b"01",
        b"[true false]",
    ] {
        assert!(matches!(
            RawDocument::parse(source.to_vec()),
            Err(RawJsonError::InvalidJson)
        ));
    }
}

#[test]
fn deep_success_and_failure_clean_up_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let depth = 20_000;
            let source = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
            let document = RawDocument::parse(source.into_bytes()).unwrap();
            let owned = document.root_owned();
            let mut value = owned.as_ref();
            for _ in 0..depth {
                value = value.array().unwrap().get(0).unwrap();
            }
            assert_eq!(value.integer(), Some(0));
            let shared = document.clone();
            drop(document);
            drop(shared);
            drop(owned);
            let invalid = format!(
                "{}{{\"x\":0,\"x\":1}}{}",
                "[".repeat(depth),
                "]".repeat(depth)
            );
            assert!(matches!(
                RawDocument::parse(invalid.into_bytes())
                    .unwrap()
                    .require_unique_fields(),
                Err(RawJsonError::DuplicateField)
            ));
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn public_debug_and_errors_redact_keys_and_values() {
    let document =
        RawDocument::parse(br#"{"SECRET_KEY":["SECRET_VALUE",123456789]}"#.to_vec()).unwrap();
    let root = document.root();
    let object = root.object().unwrap();
    let array = object
        .get("SECRET_KEY")
        .expect("unique decoded field")
        .unwrap()
        .array()
        .unwrap();
    for output in [
        format!("{document:?}"),
        format!("{root:?}"),
        format!("{:?}", root.to_owned()),
        format!("{object:?}"),
        format!("{array:?}"),
    ] {
        assert!(!output.contains("SECRET"));
        assert!(!output.contains("123456789"));
    }
    for error in [
        RawJsonError::InvalidJson,
        RawJsonError::DuplicateField,
        RawJsonError::InvalidUnicode,
        RawJsonError::WrongType,
        RawJsonError::Allocation,
    ] {
        assert!(!format!("{error:?}: {error}").contains("SECRET"));
    }
}

#[test]
fn queried_decoded_name_rejects_ambiguity_without_touching_opaque_siblings() {
    let source = br#"{"opaque":{"x":1,"\u0078":2},"a":1,"\u0061":2,"unique":3}"#;
    let document = RawDocument::parse(source.to_vec()).unwrap();
    let object = document.root().object().unwrap();
    assert_eq!(object.len(), 4);
    assert!(matches!(object.get("a"), Err(RawJsonError::DuplicateField)));
    assert_eq!(object.get("unique").unwrap().unwrap().integer(), Some(3));
    assert!(object.get("absent").unwrap().is_none());
    assert_eq!(
        object.get("opaque").unwrap().unwrap().fragment(),
        r#"{"x":1,"\u0078":2}"#
    );
    assert_eq!(
        document.require_unique_fields(),
        Err(RawJsonError::DuplicateField)
    );
    assert_eq!(document.bytes(), source);
}
