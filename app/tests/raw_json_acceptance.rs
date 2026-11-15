//! Independent acceptance; fixed RJ01-RJ30 oracle predates implementation.
#![allow(
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::items_after_statements
)]
use acp_remote::adapters::acp::raw_json::{
    OwnedRawValue, RawArray, RawDocument, RawJsonError, RawKind, RawObject, RawValueRef,
};
use std::{fmt::Debug, process::Command};

#[path = "../src/strict_json.rs"]
mod management_strict_json;

fn doc(text: &str) -> RawDocument {
    RawDocument::parse(text.as_bytes().to_vec()).unwrap()
}
fn err(text: &str, expected: RawJsonError) {
    assert_eq!(
        RawDocument::parse(text.as_bytes().to_vec()).unwrap_err(),
        expected
    );
}

// Unknown duplicate members are retained; actual unique lookup and the explicit
// strict visitor reject the queried decoded key, without re-encoding the input.
fn duplicate_error(text: &str, key: &str, expected_values: [&str; 2]) -> RawJsonError {
    let document = doc(text);
    assert_eq!(document.bytes(), text.as_bytes());
    let owned = document.root_owned();
    let copy = document.clone();
    let root_fragment = document.root().fragment().to_owned();
    let mut pending = vec![document.root()];
    let mut observed = None;
    while let Some(value) = pending.pop() {
        if let Some(object) = value.object() {
            let count = object.iter().filter(|(name, _)| *name == key).count();
            if count > 1 {
                assert_eq!(count, 2);
                assert_eq!(object.len(), 2);
                let pairs = object
                    .iter()
                    .map(|(name, value)| (name.to_owned(), value.fragment().as_bytes().to_vec()))
                    .collect::<Vec<_>>();
                assert_eq!(
                    pairs
                        .iter()
                        .map(|(name, _)| name.as_str())
                        .collect::<Vec<_>>(),
                    [key, key]
                );
                assert_eq!(
                    pairs
                        .iter()
                        .map(|(_, value)| value.as_slice())
                        .collect::<Vec<_>>(),
                    expected_values.map(str::as_bytes)
                );
                assert_eq!(
                    object
                        .iter()
                        .rev()
                        .map(|(name, value)| (
                            name.to_owned(),
                            value.fragment().as_bytes().to_vec()
                        ))
                        .collect::<Vec<_>>(),
                    pairs.iter().rev().cloned().collect::<Vec<_>>()
                );
                observed = Some(object.get(key).unwrap_err());
                assert_eq!(
                    object
                        .iter()
                        .map(|(name, value)| (
                            name.to_owned(),
                            value.fragment().as_bytes().to_vec()
                        ))
                        .collect::<Vec<_>>(),
                    pairs
                );
                break;
            }
            pending.extend(object.iter().rev().map(|(_, child)| child));
        } else if let Some(array) = value.array() {
            pending.extend(array.iter().rev());
        }
    }
    drop(pending);
    assert_eq!(observed, Some(RawJsonError::DuplicateField));
    assert_eq!(
        document.require_unique_fields().unwrap_err(),
        RawJsonError::DuplicateField
    );
    assert_eq!(document.bytes(), text.as_bytes());
    assert_eq!(copy.bytes(), text.as_bytes());
    drop(document);
    drop(copy);
    assert_eq!(owned.as_ref().fragment(), root_fragment);
    drop(owned);
    observed.unwrap()
}

fn size<I: ExactSizeIterator>(iter: &I, expected: usize) {
    assert_eq!(iter.len(), expected);
    assert_eq!(iter.size_hint(), (expected, Some(expected)));
}
fn small_stack(name: &'static str, action: fn()) {
    if std::env::var("RAW_ACCEPTANCE_CHILD").as_deref() == Ok(name) {
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(action)
            .unwrap()
            .join()
            .unwrap();
    } else {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env("RAW_ACCEPTANCE_CHILD", name)
            .spawn()
            .unwrap();
        // Only a test-process cleanup deadline; never a product capacity limit.
        let started = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > std::time::Duration::from_secs(120) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("owned small-stack child timed out: {name}");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert!(
            status.success(),
            "owned small-stack child failed: {name}: {status}"
        );
    }
}
fn deep(depth: usize, mixed: bool, leaf: &str) -> String {
    let mut text = String::new();
    for n in 0..depth {
        text.push_str(if mixed && n % 2 == 0 { "{\"k\":" } else { "[" });
    }
    text.push_str(leaf);
    for n in (0..depth).rev() {
        text.push(if mixed && n % 2 == 0 { '}' } else { ']' });
    }
    text
}
fn walk(mut value: RawValueRef<'_>, depth: usize, mixed: bool) -> RawValueRef<'_> {
    for n in 0..depth {
        value = if mixed && n % 2 == 0 {
            let object = value.object().unwrap();
            assert_eq!(object.len(), 1);
            assert_eq!(object.iter().next().unwrap().0, "k");
            object.get("k").unwrap().unwrap()
        } else {
            let array = value.array().unwrap();
            assert_eq!(array.len(), 1);
            array.get(0).unwrap()
        };
    }
    value
}

#[test]
fn rj01_six_roots_and_exact_original_bytes() {
    for (body, kind) in [
        ("{\"中文\":\"😀\"}", RawKind::Object),
        ("[\"😀\"]", RawKind::Array),
        ("\"中文😀\"", RawKind::String),
        ("-12", RawKind::Number),
        ("true", RawKind::Boolean),
        ("null", RawKind::Null),
    ] {
        let text = format!(" \t\r\n{body}\n\r\t ");
        let d = doc(&text);
        assert_eq!(d.bytes(), text.as_bytes());
        assert_eq!(d.root().fragment(), body);
        assert_eq!(d.root().kind(), kind);
    }
}
#[test]
fn rj02_typed_accessors_never_coerce() {
    for text in [
        "{}", "[]", "\"123\"", "123", "true", "false", "null", "\"true\"", "1.0", "1e0",
    ] {
        let d = doc(text);
        let v = d.root();
        assert_eq!(v.object().is_some(), text == "{}");
        assert_eq!(v.array().is_some(), text == "[]");
        assert_eq!(v.is_null(), text == "null");
        assert_eq!(
            v.boolean(),
            match text {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            }
        );
        assert_eq!(v.integer(), if text == "123" { Some(123) } else { None });
        assert_eq!(v.unsigned(), if text == "123" { Some(123) } else { None });
        if text.starts_with('"') {
            assert_eq!(v.string().unwrap(), text.trim_matches('"'));
        } else {
            assert_eq!(v.string(), Err(RawJsonError::WrongType));
        }
    }
}
#[test]
fn rj03_invalid_syntax_utf8_and_trailing_data() {
    for text in [
        "",
        " \t\r\n",
        "01",
        "+1",
        ".1",
        "1.",
        "1e",
        "NaN",
        "Infinity",
        "[1 2]",
        "{\"a\" 1}",
        "[1,]",
        "{\"a\":1,}",
        "[",
        "{",
        "\"a",
        "\"\n\"",
        "\"\\x\"",
        "{} []",
        "true garbage",
        "\u{a0}null",
        "null\u{2028}",
    ] {
        err(text, RawJsonError::InvalidJson);
    }
    for bytes in [
        vec![0xff],
        vec![b'"', 0xe2, 0x82, b'"'],
        vec![b'"', 0xc0, 0xaf, b'"'],
        vec![b'"', 0xed, 0xa0, 0x80, b'"'],
    ] {
        assert_eq!(
            RawDocument::parse(bytes).unwrap_err(),
            RawJsonError::InvalidJson
        );
    }
}
#[test]
fn rj04_full_syntax_validation_precedes_scan_errors() {
    for text in [
        r#"{"x":1,"x":2,}"#,
        r#"["\uD800",]"#,
        r#"{"x":1,"x":2} null"#,
    ] {
        err(text, RawJsonError::InvalidJson);
    }
    let mut text = br#""\uD800""#.to_vec();
    text.push(0xff);
    assert_eq!(
        RawDocument::parse(text).unwrap_err(),
        RawJsonError::InvalidJson
    );
}
#[test]
fn rj05_ordered_decoded_object_and_exact_double_ended_iter() {
    let d = doc(r#"{ "z":1,"\u0061":[2],"é":3,"":4 }"#);
    let object = d.root().object().unwrap();
    assert_eq!(object.len(), 4);
    assert!(!object.is_empty());
    assert_eq!(object.get("a").unwrap().unwrap().fragment(), "[2]");
    assert!(object.get("missing").unwrap().is_none());
    let mut it = object.iter();
    size(&it, 4);
    assert_eq!(it.next().unwrap().0, "z");
    size(&it, 3);
    assert_eq!(it.next_back().unwrap().0, "");
    size(&it, 2);
    assert_eq!(it.next_back().unwrap().0, "é");
    size(&it, 1);
    assert_eq!(it.next().unwrap().0, "a");
    size(&it, 0);
    assert!(it.next().is_none());
    assert!(it.next_back().is_none());
    assert_eq!(
        object
            .iter()
            .map(|(k, v)| (k, v.fragment()))
            .collect::<Vec<_>>(),
        [("z", "1"), ("a", "[2]"), ("é", "3"), ("", "4")]
    );
    let empty = doc("{}");
    let object = empty.root().object().unwrap();
    assert!(object.is_empty());
    assert_eq!(object.len(), 0);
    size(&object.iter(), 0);
    assert!(object.get("").unwrap().is_none());
}
#[test]
fn rj06_ordered_array_bounds_and_exact_double_ended_iter() {
    let d = doc(r#"[1,{"k":2},[3],"x"]"#);
    let array = d.root().array().unwrap();
    assert_eq!(array.len(), 4);
    assert!(!array.is_empty());
    assert_eq!(array.get(0).unwrap().fragment(), "1");
    assert_eq!(array.get(3).unwrap().fragment(), r#""x""#);
    assert!(array.get(4).is_none());
    assert!(array.get(usize::MAX).is_none());
    let mut it = array.iter();
    size(&it, 4);
    assert_eq!(it.next().unwrap().fragment(), "1");
    size(&it, 3);
    assert_eq!(it.next_back().unwrap().fragment(), r#""x""#);
    size(&it, 2);
    assert_eq!(it.next().unwrap().fragment(), r#"{"k":2}"#);
    size(&it, 1);
    assert_eq!(it.next_back().unwrap().fragment(), "[3]");
    size(&it, 0);
    assert!(it.next().is_none());
    assert!(it.next_back().is_none());
    let empty = doc("[]");
    let array = empty.root().array().unwrap();
    assert!(array.is_empty());
    assert_eq!(array.len(), 0);
    size(&array.iter(), 0);
    assert!(array.get(0).is_none());
}
#[test]
fn rj07_document_clone_and_root_owned_share_storage() {
    let text = format!("[\"{}\"]", "x".repeat(1024 * 1024));
    let d = doc(&text);
    let clone = d.clone();
    assert_eq!(d.bytes().as_ptr(), clone.bytes().as_ptr());
    assert_eq!(clone.bytes(), text.as_bytes());
    let owned = d.root_owned();
    let other = owned.clone();
    assert_eq!(
        owned.as_ref().fragment().as_ptr(),
        d.root().fragment().as_ptr()
    );
    drop(d);
    drop(clone);
    drop(owned);
    assert_eq!(other.as_ref().fragment(), text);
}
#[test]
fn rj08_nested_owned_outlives_every_original_handle() {
    let (owned, address) = {
        let d = doc(r#"{"outer":[{"未知":[1,2,{"x":3}]}],"other":0}"#);
        let v = d
            .root()
            .object()
            .unwrap()
            .get("outer")
            .unwrap()
            .unwrap()
            .array()
            .unwrap()
            .get(0)
            .unwrap()
            .object()
            .unwrap()
            .get("未知")
            .unwrap()
            .unwrap();
        (v.to_owned(), v.fragment().as_ptr())
    };
    let v = owned.as_ref();
    assert_eq!(v.fragment().as_ptr(), address);
    assert_eq!(v.fragment(), r#"[1,2,{"x":3}]"#);
    assert_eq!(v.compact().unwrap(), r#"[1,2,{"x":3}]"#);
    let array = v.array().unwrap();
    assert_eq!(array.iter().count(), 3);
    assert_eq!(
        array
            .get(2)
            .unwrap()
            .object()
            .unwrap()
            .get("x")
            .unwrap()
            .unwrap()
            .integer(),
        Some(3)
    );
}
#[test]
fn rj09_compact_only_four_json_whitespaces_outside_strings() {
    let text = " \n { \r\n\t\"s\" : \"a b \u{2028}\\n\\r\\t\\\\\\\"[]{}\", \n\"n\": -0,\t\"e\":1E+09 } \r\n";
    let expected = "{\"s\":\"a b \u{2028}\\n\\r\\t\\\\\\\"[]{}\",\"n\":-0,\"e\":1E+09}";
    let d = doc(text);
    let result = d.root().compact().unwrap();
    assert_eq!(result, expected);
    assert!(!result.contains('\n'));
    assert_eq!(d.bytes(), text.as_bytes());
}
#[test]
fn rj10_compact_nested_fragment_roundtrip_keeps_input() {
    let text = "{\"a\": [ 1,\n { \"x\": \"a b\\n\" }, 1E+09 ], \"b\":0}";
    let d = doc(text);
    let v = d.root().object().unwrap().get("a").unwrap().unwrap();
    let compact = v.compact().unwrap();
    assert_eq!(compact, r#"[1,{"x":"a b\n"},1E+09]"#);
    let round = doc(&compact);
    assert_eq!(round.root().array().unwrap().len(), 3);
    assert_eq!(
        round
            .root()
            .array()
            .unwrap()
            .get(1)
            .unwrap()
            .object()
            .unwrap()
            .get("x")
            .unwrap()
            .unwrap()
            .string()
            .unwrap(),
        "a b\n"
    );
    assert_eq!(d.bytes(), text.as_bytes());
}
#[test]
fn rj11_strings_all_escapes_and_scalar_pairs() {
    for (text, expected) in [
        (r#""\"\\\/\b\f\n\r\t\u0000""#, "\"\\/\u{8}\u{c}\n\r\t\0"),
        (r#""\uD7FF\uE000\uFFFF""#, "\u{d7ff}\u{e000}\u{ffff}"),
        (
            r#""\uD800\uDC00\uDBFF\uDFFF\uD83D\uDE00""#,
            "\u{10000}\u{10ffff}😀",
        ),
        (r#""中文😀[]{}""#, "中文😀[]{}"),
    ] {
        let d = doc(text);
        assert_eq!(d.root().string().unwrap(), expected);
        assert_eq!(d.root().fragment(), text);
        assert_eq!(d.root().compact().unwrap(), text);
    }
}
#[test]
fn rj12_unknown_numbers_keep_arbitrary_lexical_size() {
    for text in [
        "9".repeat(4096),
        format!("-{}", "8".repeat(4096)),
        "1e999999".into(),
        "-1.234567890123456789e-999999".into(),
    ] {
        for body in [text.clone(), format!("{{\"unknown\":{text}}}")] {
            let d = doc(&body);
            let v = if body.starts_with('{') {
                d.root().object().unwrap().get("unknown").unwrap().unwrap()
            } else {
                d.root()
            };
            assert_eq!(v.kind(), RawKind::Number);
            assert_eq!(v.fragment(), text);
            assert_eq!(v.compact().unwrap(), text);
        }
    }
}
#[test]
fn rj13_signed_integer_exact_boundaries() {
    for (text, expected) in [
        ("0", Some(0)),
        ("-0", Some(0)),
        ("1", Some(1)),
        ("-1", Some(-1)),
        ("-9223372036854775808", Some(i64::MIN)),
        ("9223372036854775807", Some(i64::MAX)),
        ("-9223372036854775809", None),
        ("9223372036854775808", None),
        ("18446744073709551615", None),
        ("1.0", None),
        ("1e0", None),
        ("1E+0", None),
    ] {
        let d = doc(text);
        assert_eq!(d.root().integer(), expected, "{text}");
        assert_eq!(d.root().fragment(), text);
    }
}
#[test]
fn rj14_unsigned_exact_boundaries_and_negative_zero() {
    for (text, expected) in [
        ("0", Some(0)),
        ("1", Some(1)),
        ("18446744073709551615", Some(u64::MAX)),
        ("18446744073709551616", None),
        ("-0", None),
        ("-1", None),
        ("1.0", None),
        ("1e0", None),
    ] {
        assert_eq!(doc(text).root().unsigned(), expected, "{text}");
    }
    assert_eq!(doc(&"9".repeat(4096)).root().unsigned(), None);
}
#[test]
fn rj15_invalid_unicode_values_and_unknown_descendants() {
    for body in [
        r#""\uD800""#,
        r#""\uDBFF""#,
        r#""\uDC00""#,
        r#""\uDFFF""#,
        r#""\uD800a""#,
        r#""\uD800\\uDC00""#,
        r#""\uD800\uDBFF""#,
        r#""\uDC00\uD800""#,
    ] {
        err(body, RawJsonError::InvalidUnicode);
        err(
            &format!("{{\"unknown\":[{{\"x\":{body}}}]}}"),
            RawJsonError::InvalidUnicode,
        );
    }
}
#[test]
fn rj16_invalid_unicode_keys_and_pair_boundaries() {
    for key in [r"\uD800", r"\uDBFF", r"\uDC00", r"\uDFFF"] {
        err(&format!("{{\"{key}\":0}}"), RawJsonError::InvalidUnicode);
    }
    let d = doc(r#"{"\uD83D\uDE00":1,"\\uD800":2}"#);
    let o = d.root().object().unwrap();
    assert_eq!(o.get("😀").unwrap().unwrap().integer(), Some(1));
    assert_eq!(o.get(r"\uD800").unwrap().unwrap().integer(), Some(2));
    assert_eq!(
        duplicate_error(r#"{"\uD83D\uDE00":1,"😀":2}"#, "😀", ["1", "2"]),
        RawJsonError::DuplicateField
    );
}
#[test]
fn rj17_first_scanner_error_follows_source_order() {
    err(
        r#"{"a":1,"a":2,"bad":"\uD800"}"#,
        RawJsonError::InvalidUnicode,
    );
    err(
        r#"{"bad":"\uD800","a":1,"a":2}"#,
        RawJsonError::InvalidUnicode,
    );
    err(r#"{"\uD800":1,"a":1,"a":2}"#, RawJsonError::InvalidUnicode);
    err(r#"{"a":1,"a":2,"\uD800":1}"#, RawJsonError::InvalidUnicode);
}
#[test]
fn rj18_duplicate_decoded_keys_at_every_depth() {
    for body in [
        r#"{"a":1,"\u0061":2}"#,
        r#"{"é":1,"\u00e9":2}"#,
        r#"{"":1,"":2}"#,
        r#"{"literal":1,"literal":2}"#,
    ] {
        for wrapper in [
            body.to_owned(),
            format!("[{{\"unknown\":{body}}}]"),
            deep(129, true, body),
        ] {
            let duplicate = doc(body);
            let key = duplicate.root().object().unwrap().iter().next().unwrap().0;
            assert_eq!(
                duplicate_error(&wrapper, key, ["1", "2"]),
                RawJsonError::DuplicateField
            );
        }
    }
}
#[test]
fn rj19_sibling_keys_and_no_unicode_normalization() {
    let d = doc(r#"[{"a":1},{"\u0061":2}]"#);
    for (i, v) in d.root().array().unwrap().iter().enumerate() {
        assert_eq!(
            v.object().unwrap().get("a").unwrap().unwrap().fragment(),
            if i == 0 { "1" } else { "2" }
        );
    }
    let d = doc("{\"é\":1,\"e\u{301}\":2,\"A\":3,\"a\":4}");
    let o = d.root().object().unwrap();
    assert_eq!(
        o.iter().map(|(k, _)| k).collect::<Vec<_>>(),
        ["é", "e\u{301}", "A", "a"]
    );
    for (k, n) in [("é", 1), ("e\u{301}", 2), ("A", 3), ("a", 4)] {
        assert_eq!(o.get(k).unwrap().unwrap().integer(), Some(n));
    }
}
#[test]
fn rj20_escaped_delimiters_and_quotes_in_keys() {
    let d = doc(r#"{"\"\\[]{}:,":1,"\u0000":2,"\uD83D\uDE00":3}"#);
    let o = d.root().object().unwrap();
    for (k, n) in [("\"\\[]{}:,", 1), ("\0", 2), ("😀", 3)] {
        assert_eq!(o.get(k).unwrap().unwrap().integer(), Some(n));
    }
    assert_eq!(
        duplicate_error(r#"{"\"":1,"\u0022":2}"#, "\"", ["1", "2"]),
        RawJsonError::DuplicateField
    );
    assert_eq!(
        duplicate_error(r#"{"\\":1,"\u005c":2}"#, "\\", ["1", "2"]),
        RawJsonError::DuplicateField
    );
    assert_eq!(
        duplicate_error(r#"{"\u0000":1,"\u0000":2}"#, "\0", ["1", "2"]),
        RawJsonError::DuplicateField
    );
}
#[test]
fn rj21_exceed_old_structural_token_budget() {
    let text = format!("[{}]", vec!["0"; 262_145].join(","));
    let d = doc(&text);
    assert_eq!(d.bytes(), text.as_bytes());
    let a = d.root().array().unwrap();
    assert_eq!(a.len(), 262_145);
    assert_eq!(a.get(0).unwrap().integer(), Some(0));
    assert_eq!(a.get(262_144).unwrap().integer(), Some(0));
    for v in a.iter() {
        assert_eq!(v.fragment(), "0");
    }
    assert_eq!(a.iter().count(), 262_145);
    let text = format!(
        "{{{}}}",
        (0..131_073)
            .map(|n| format!("\"k{n}\":{n}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let d = doc(&text);
    assert_eq!(d.bytes(), text.as_bytes());
    let o = d.root().object().unwrap();
    assert_eq!(o.len(), 131_073);
    for (n, (key, v)) in o.iter().enumerate() {
        assert_eq!(key, format!("k{n}"));
        assert_eq!(v.fragment(), n.to_string());
    }
    assert_eq!(o.iter().count(), 131_073);
}
#[test]
fn rj22_body_above_thirty_two_mib() {
    let content = format!("{}尾😀", "x".repeat(32 * 1024 * 1024 + 257));
    let text = format!("{{\"unknown\":\"{content}\",\"number\":1e999999}}");
    let d = doc(&text);
    assert_eq!(d.bytes(), text.as_bytes());
    assert_eq!(d.root().compact().unwrap(), text);
    let v = d.root().object().unwrap().get("unknown").unwrap().unwrap();
    assert_eq!(v.string().unwrap(), content);
    assert_eq!(v.fragment(), format!("\"{content}\""));
    assert_eq!(
        d.root()
            .object()
            .unwrap()
            .get("number")
            .unwrap()
            .unwrap()
            .fragment(),
        "1e999999"
    );
}
#[test]
fn rj23_twenty_thousand_arrays() {
    small_stack("rj23_twenty_thousand_arrays", || {
        for depth in [96, 128, 129, 20_000] {
            let text = deep(depth, false, "0");
            let d = doc(&text);
            assert_eq!(d.bytes(), text.as_bytes());
            assert_eq!(walk(d.root(), depth, false).integer(), Some(0));
        }
    });
}
#[test]
fn rj24_twenty_thousand_mixed_objects() {
    small_stack("rj24_twenty_thousand_mixed_objects", || {
        let text = deep(20_000, true, r#""[]{}\"\\""#);
        let d = doc(&text);
        assert_eq!(d.bytes(), text.as_bytes());
        assert_eq!(walk(d.root(), 20_000, true).string().unwrap(), "[]{}\"\\");
    });
}
#[test]
fn rj25_deep_clone_owned_and_drop() {
    small_stack("rj25_deep_clone_owned_and_drop", || {
        for mixed in [false, true] {
            let text = deep(20_000, mixed, "0");
            let d = doc(&text);
            let pointer = d.root().fragment().as_ptr();
            let mut documents = Vec::new();
            let mut roots = Vec::new();
            let mut leaves = Vec::new();
            for _ in 0..64 {
                let c = d.clone();
                assert_eq!(c.root().fragment().as_ptr(), pointer);
                documents.push(c);
                roots.push(d.root_owned().clone());
                leaves.push(walk(d.root(), 20_000, mixed).to_owned().clone());
            }
            let final_owned = roots[0].clone();
            drop(d);
            while let Some(c) = documents.pop() {
                drop(c);
                drop(roots.pop());
                drop(leaves.pop());
            }
            assert_eq!(final_owned.as_ref().fragment().as_ptr(), pointer);
            assert_eq!(walk(final_owned.as_ref(), 20_000, mixed).integer(), Some(0));
            drop(final_owned);
        }
    });
}
#[test]
fn rj26_deep_failure_drop_is_nonrecursive() {
    small_stack("rj26_deep_failure_drop_is_nonrecursive", || {
        for mixed in [false, true] {
            assert_eq!(
                duplicate_error(
                    &deep(20_000, mixed, r#"{"a":1,"\u0061":2}"#),
                    "a",
                    ["1", "2"]
                ),
                RawJsonError::DuplicateField,
            );
            err(
                &deep(20_000, mixed, r#""\uD800""#),
                RawJsonError::InvalidUnicode,
            );
            let mut missing = deep(20_000, mixed, "0");
            missing.pop();
            err(&missing, RawJsonError::InvalidJson);
            err(&deep(20_000, mixed, "[0,]"), RawJsonError::InvalidJson);
            assert_eq!(
                doc("{\"ok\":1}")
                    .root()
                    .object()
                    .unwrap()
                    .get("ok")
                    .unwrap()
                    .unwrap()
                    .integer(),
                Some(1)
            );
        }
    });
}
#[test]
fn rj27_errors_never_include_sensitive_payload() {
    let key = "CANARY_FIELD_NAME";
    let value = "CANARY_FIELD_VALUE";
    let number = "91939393939393939393939393939393939393939";
    let mut errors = vec![
        RawJsonError::InvalidJson,
        RawJsonError::DuplicateField,
        RawJsonError::InvalidUnicode,
        RawJsonError::WrongType,
        RawJsonError::Allocation,
    ];
    for (index, text) in [
        format!("{{\"{key}\":\"{value}\",\"n\":{number},}}"),
        format!("{{\"{key}\":\"{value}\",\"{key}\":{number}}}"),
        format!("{{\"{key}\":\"{value}\\uD800\",\"n\":{number}}}"),
    ]
    .into_iter()
    .enumerate()
    {
        if index == 1 {
            errors.push(duplicate_error(
                &text,
                key,
                [&format!("\"{value}\""), number],
            ));
        } else {
            errors.push(RawDocument::parse(text.into_bytes()).unwrap_err());
        }
    }
    errors.push(
        doc(&format!("{{\"{key}\":\"{value}\",\"n\":{number}}}"))
            .root()
            .string()
            .unwrap_err(),
    );
    for e in errors {
        let expected = match e {
            RawJsonError::InvalidJson => "JSON is invalid",
            RawJsonError::DuplicateField => "JSON contains a duplicate object field",
            RawJsonError::InvalidUnicode => "JSON contains invalid Unicode",
            RawJsonError::WrongType => "JSON value has a different type",
            RawJsonError::Allocation => "JSON allocation failed",
        };
        assert_eq!(e.to_string(), expected);
        for rendered in [format!("{e}"), format!("{e:?}"), e.to_string()] {
            for canary in [key, value, number, "CANARY_"] {
                assert!(!rendered.contains(canary));
            }
        }
        assert!(std::error::Error::source(&e).is_none());
    }
}
#[test]
#[allow(clippy::redundant_clone)] // Clone Debug is itself an acceptance branch.
fn rj28_all_public_handle_debug_redacted() {
    let d = doc(
        r#"{"CANARY_FIELD_NAME":["CANARY_FIELD_VALUE",91939393939393939393939393939393939393939]}"#,
    );
    let root = d.root();
    let object = root.object().unwrap();
    let array = object.iter().next().unwrap().1.array().unwrap();
    let owned = root.to_owned();
    let mut rendered = vec![
        format!("{d:?}"),
        format!("{:?}", d.clone()),
        format!("{root:?}"),
        format!("{:?}", root.clone()),
        format!("{owned:?}"),
        format!("{:?}", owned.clone()),
        format!("{object:?}"),
        format!("{:?}", object.clone()),
        format!("{array:?}"),
        format!("{:?}", array.clone()),
    ];
    for v in array.iter() {
        rendered.push(format!("{v:?}"));
        rendered.push(format!("{:?}", v.to_owned()));
    }
    for debug in rendered {
        for canary in [
            "CANARY_FIELD_NAME",
            "CANARY_FIELD_VALUE",
            "91939393939393939393939393939393939393939",
            "CANARY_",
        ] {
            assert!(!debug.contains(canary), "Debug leaked canary");
        }
    }
}
#[test]
#[allow(clippy::elidable_lifetime_names)] // Explicit 'a checks the published iterator lifetime contract.
fn rj29_public_traits_and_temporary_lifetimes() {
    fn clone_type<T: Clone>() {}
    fn copy_type<T: Copy + Clone>() {}
    fn enum_type<T: Copy + Clone + Debug + Eq + PartialEq>() {}
    fn error_type<T: std::error::Error>() {}
    clone_type::<RawDocument>();
    clone_type::<OwnedRawValue>();
    copy_type::<RawValueRef<'_>>();
    copy_type::<RawObject<'_>>();
    copy_type::<RawArray<'_>>();
    enum_type::<RawKind>();
    enum_type::<RawJsonError>();
    error_type::<RawJsonError>();
    fn object_iter<'a>(
        d: &'a RawDocument,
    ) -> impl ExactSizeIterator<Item = (&'a str, RawValueRef<'a>)> + DoubleEndedIterator + 'a {
        d.root().object().unwrap().iter()
    }
    fn array_iter<'a>(
        d: &'a RawDocument,
    ) -> impl ExactSizeIterator<Item = RawValueRef<'a>> + DoubleEndedIterator + 'a {
        d.root().array().unwrap().iter()
    }
    let d = doc("{\"a\":1,\"b\":2}");
    let mut it = object_iter(&d);
    size(&it, 2);
    assert_eq!(it.next().unwrap().0, "a");
    size(&it, 1);
    assert_eq!(it.next_back().unwrap().0, "b");
    size(&it, 0);
    let d = doc("[1,2]");
    let mut it = array_iter(&d);
    size(&it, 2);
    assert_eq!(it.next_back().unwrap().integer(), Some(2));
    size(&it, 1);
    assert_eq!(it.next().unwrap().integer(), Some(1));
    size(&it, 0);
}
#[test]
fn rj30_management_eight_layers_and_old_assets_unchanged() {
    use management_strict_json::{MAX_PROTOCOL_JSON_DEPTH, StrictJsonError, parse_object};
    assert_eq!(MAX_PROTOCOL_JSON_DEPTH, 8);
    let eight = deep(8, true, "0").replace('[', "{\"k\":").replace(']', "}");
    assert!(parse_object(eight.as_bytes(), MAX_PROTOCOL_JSON_DEPTH).is_ok());
    let nine = format!("{{\"k\":{eight}}}");
    assert_eq!(
        parse_object(nine.as_bytes(), MAX_PROTOCOL_JSON_DEPTH).unwrap_err(),
        StrictJsonError::TooDeep
    );
}
