#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn rt11_prompt_result_completion_and_insertion_are_version_exact() {
    for version in [Version::V1, Version::V2] {
        let r = Rig::ready(version).await;
        let s = r.client.strategy().unwrap();
        if version == Version::V1 {
            assert!(
                matches!(s.prompt_result(owned("{\"stopReason\":\"end_turn\"}").as_ref()).unwrap(),PromptResult::Completed{stop_reason}if stop_reason.as_ref()=="end_turn")
            );
            assert!(
                s.prompt_result(owned("{\"messageId\":\"inserted\"}").as_ref())
                    .is_err()
            );
        } else {
            assert!(
                matches!(s.prompt_result(owned("{\"messageId\":\"inserted\"}").as_ref()).unwrap(),PromptResult::Inserted{message}if message.as_str()=="inserted")
            );
            assert!(
                s.prompt_result(owned("{\"stopReason\":\"end_turn\"}").as_ref())
                    .is_err()
            );
        }
        r.close().await;
    }
}
use support::*;

#[test]
fn rt01_raw_exact_shape_unicode_and_routing_separation() {
    let bytes = br#"  {"jsonrpc":false,"a":1e9999,"\u0062":"\u0061"}  "#.to_vec();
    let raw = RawMessage::parse(bytes.clone()).unwrap();
    assert_eq!(raw.bytes(), bytes);
    assert_eq!(raw.fields().unwrap().len(), 3);
    assert!(raw.field("missing").unwrap().is_none());
    assert_eq!(raw.field("b").unwrap().unwrap().string().unwrap(), "a");
    let c = raw.clone();
    drop(raw);
    assert_eq!(c.bytes(), bytes);
    for bad in [
        b"[]".as_slice(),
        b"null",
        b"{",
        br#"{"a":"\ud800"}"#,
        b"{\"x\":\"\xff\"}",
    ] {
        assert!(RawMessage::parse(bad.to_vec()).is_err());
    }
    let ambiguous_bytes = br#"{"a":1,"\u0061":2}"#.to_vec();
    let ambiguous = RawMessage::parse(ambiguous_bytes.clone()).unwrap();
    assert_eq!(ambiguous.bytes(), ambiguous_bytes);
    assert_eq!(
        ambiguous
            .fields()
            .unwrap()
            .iter()
            .map(|(key, value)| (key, value.fragment()))
            .collect::<Vec<_>>(),
        [("a", "1"), ("a", "2")]
    );
    assert!(matches!(
        ambiguous.field("a"),
        Err(AcpError::InvalidMessage)
    ));
    assert_eq!(ambiguous.bytes(), ambiguous_bytes);
}
#[test]
fn rt02_rpc_id_lexical_boundaries_and_typed_identity() {
    for text in ["0", "9007199254740991", "-9007199254740991"] {
        let raw = owned(text);
        let id = RpcId::parse(raw.as_ref()).unwrap();
        let mut out = vec![];
        id.write_json(&mut out).unwrap();
        assert_eq!(out, text.as_bytes());
    }
    for text in ["-0", "1.0", "1e0", "9007199254740992", "null", "true"] {
        assert!(RpcId::parse(owned(text).as_ref()).is_err(), "{text}");
    }
    assert_ne!(
        RpcId::parse(owned("1").as_ref()).unwrap(),
        RpcId::parse(owned("\"1\"").as_ref()).unwrap()
    );
    for (n, ok) in [(1024, true), (1025, false)] {
        assert_eq!(
            RpcId::parse(raw_value(&json!("x".repeat(n))).as_ref()).is_ok(),
            ok
        );
    }
}
#[test]
fn cf02_cf04_output_identity_has_no_4096_cap_session_option_keep_controls() {
    for s in [
        String::new(),
        "é".repeat(2048),
        "x".repeat(4097),
        "é".repeat(4097),
    ] {
        let o = raw_value(&json!(s));
        assert_eq!(MessageId::parse(o.as_ref()).unwrap().as_str(), s);
        assert_eq!(ToolCallId::parse(o.as_ref()).unwrap().as_str(), s);
        assert_eq!(SessionId::parse(o.as_ref()).is_ok(), s.len() <= 4096);
        assert_eq!(OptionId::parse(o.as_ref()).is_ok(), s.len() <= 4096);
    }
    for bad in [json!(null), json!([]), json!(7), json!({})] {
        let o = raw_value(&bad);
        assert!(MessageId::parse(o.as_ref()).is_err());
        assert!(ToolCallId::parse(o.as_ref()).is_err());
    }
}
#[test]
fn cfg01_unknown_priority_known_empty_getters() {
    let snapshot = |cwd, mcp_servers, additional_directories| SetupSnapshot {
        wire: Version::V1,
        source: ConfigSourceStamp {
            reference: Arc::from("SENSITIVE_SOURCE"),
            revision: 7,
        },
        cwd,
        mcp_servers,
        additional_directories,
        extra: ExtensionFields::empty().unwrap(),
    };
    assert!(matches!(
        SessionSetup::try_from_snapshot(snapshot(Known::Unknown, Known::Unknown, Known::Unknown)),
        Err(SetupError::Incomplete(SetupField::Cwd))
    ));
    assert!(matches!(
        SessionSetup::try_from_snapshot(snapshot(
            Known::Known(Arc::from("C:/synthetic")),
            Known::Unknown,
            Known::Unknown
        )),
        Err(SetupError::Incomplete(SetupField::Mcp))
    ));
    assert!(matches!(
        SessionSetup::try_from_snapshot(snapshot(
            Known::Known(Arc::from("C:/synthetic")),
            Known::Known(vec![]),
            Known::Unknown
        )),
        Err(SetupError::Incomplete(SetupField::AdditionalDirectories))
    ));
    let complete = SessionSetup::try_from_snapshot(snapshot(
        Known::Known(Arc::from("C:/synthetic")),
        Known::Known(vec![]),
        Known::Known(vec![]),
    ))
    .unwrap();
    assert_eq!(complete.cwd(), "C:/synthetic");
    assert_eq!(complete.wire(), Version::V1);
    assert_eq!(complete.source().revision, 7);
    assert!(complete.mcp_servers().is_empty());
    assert!(complete.additional_directories().is_empty());
    assert_eq!(complete.extra().as_raw().object().unwrap().len(), 0);
    assert!(!format!("{complete:?}").contains("SENSITIVE_SOURCE"));
}
#[test]
fn api01_metadata_order_shallow_raw_and_iteration() {
    let raw = owned(br#"{"a":{"keep":1e9999},"\u0062":"\u0061"}"#);
    let obj = OrderedRawObject::from_raw(raw).unwrap();
    assert_eq!(obj.len(), 2);
    assert!(!obj.is_empty());
    assert_eq!(obj.iter().len(), 2);
    assert_eq!(obj.iter().next().unwrap().0, "a");
    assert_eq!(obj.iter().next_back().unwrap().0, "b");
    assert_eq!(
        obj.get("a").unwrap().unwrap().fragment(),
        "{\"keep\":1e9999}"
    );
    assert!(obj.get("none").unwrap().is_none());
    let back = obj.to_raw().unwrap();
    assert_eq!(
        back.as_ref()
            .object()
            .unwrap()
            .get("a")
            .unwrap()
            .unwrap()
            .fragment(),
        "{\"keep\":1e9999}"
    );
    let m = RawMetadata::from_raw(back).unwrap();
    assert!(!m.is_null());
    assert_eq!(m.as_object().unwrap().len(), 2);
    assert!(!m.to_raw().unwrap().as_ref().is_null());
    let null = RawMetadata::from_raw(owned("null")).unwrap();
    assert!(null.is_null());
    assert!(null.as_object().is_none());
    assert!(null.to_raw().unwrap().as_ref().is_null());
    assert!(OrderedRawObject::from_raw(owned("[]")).is_err());
    assert!(RawMetadata::from_raw(owned("[]")).is_err());
}
#[tokio::test]
async fn rt14_rt15_version_patch_null_order_and_unknown_values() {
    for version in [Version::V1, Version::V2] {
        let r = Rig::ready(version).await;
        let s = r.client.strategy().unwrap();
        assert!(matches!(
            s.tool_name_patch(owned("{}").as_ref()).unwrap(),
            ToolNamePatch::Unchanged
        ));
        let null = s
            .tool_name_patch(owned("{\"name\":null}").as_ref())
            .unwrap();
        assert!(matches!(
            (version, null),
            (Version::V1, ToolNamePatch::Unchanged) | (Version::V2, ToolNamePatch::Clear)
        ));
        assert!(
            matches!(s.tool_name_patch(owned("{\"name\":\"\"}").as_ref()).unwrap(),ToolNamePatch::Set(n)if n.is_empty())
        );
        assert!(s.tool_name_patch(owned("{\"name\":7}").as_ref()).is_err());
        let current = RawMetadata::from_raw(owned("{\"a\":{\"old\":1},\"b\":1e9999}")).unwrap();
        let patch = owned("{\"_meta\":{\"a\":{\"new\":2},\"c\":-0,\"b\":null}}");
        if version == Version::V1 {
            assert!(matches!(
                s.tool_metadata_patch(&current, patch.as_ref()),
                Err(AcpError::UnsupportedCapability)
            ));
        } else {
            let updated = s.tool_metadata_patch(&current, patch.as_ref()).unwrap();
            let o = updated.as_object().unwrap();
            assert_eq!(o.iter().map(|(k, _)| k).collect::<Vec<_>>(), ["a", "c"]);
            assert_eq!(o.get("a").unwrap().unwrap().fragment(), "{\"new\":2}");
            assert_eq!(o.get("c").unwrap().unwrap().fragment(), "-0");
            assert!(o.get("b").unwrap().is_none());
            assert_eq!(
                current
                    .as_object()
                    .unwrap()
                    .get("b")
                    .unwrap()
                    .unwrap()
                    .fragment(),
                "1e9999"
            );
        }
        r.close().await;
    }
}
#[tokio::test]
async fn cfg03_cfg04_mcp_exact_raw_order_duplicates_and_sensitive_debug() {
    for version in [Version::V1, Version::V2] {
        let r = Rig::ready(version).await;
        let nv = || {
            NamedValue::new(
                Arc::from("SENSITIVE_NAME"),
                Arc::from("SENSITIVE_VALUE"),
                ExtensionFields::empty().unwrap(),
            )
            .unwrap()
        };
        let n = nv();
        assert_eq!(n.name(), "SENSITIVE_NAME");
        assert_eq!(n.value(), "SENSITIVE_VALUE");
        assert_eq!(n.extra().as_raw().object().unwrap().len(), 0);
        assert!(!format!("{n:?}").contains("SENSITIVE"));
        let stdio = McpServer::stdio(
            version,
            Arc::from("server"),
            Arc::from("SENSITIVE_COMMAND"),
            vec![Arc::from("arg1"), Arc::from("arg2")],
            vec![nv(), nv()],
            ExtensionFields::empty().unwrap(),
        )
        .unwrap();
        assert_eq!(stdio.wire(), version);
        assert!(matches!(stdio.transport(), McpTransport::Stdio));
        assert!(!format!("{stdio:?}").contains("SENSITIVE"));
        let shape = stdio.as_raw().object().unwrap();
        assert_eq!(shape.get("env").unwrap().unwrap().array().unwrap().len(), 2);
        assert_eq!(
            shape.get("args").unwrap().unwrap().array().unwrap().len(),
            2
        );
        let http = McpServer::http(
            version,
            Arc::from("http"),
            Arc::from("https://synthetic.invalid"),
            vec![nv(), nv()],
            ExtensionFields::empty().unwrap(),
        )
        .unwrap();
        assert!(matches!(http.transport(), McpTransport::Http));
        assert_eq!(
            http.as_raw()
                .object()
                .unwrap()
                .get("headers")
                .unwrap()
                .unwrap()
                .array()
                .unwrap()
                .len(),
            2
        );
        let mut snap = SetupSnapshot {
            wire: version,
            source: ConfigSourceStamp {
                reference: Arc::from("source"),
                revision: 7,
            },
            cwd: Known::Known(Arc::from("C:/synthetic")),
            mcp_servers: Known::Known(vec![stdio.clone(), http]),
            additional_directories: Known::Known(vec![
                Arc::from("C:/root-a"),
                Arc::from("C:/root-b"),
            ]),
            extra: ExtensionFields::empty().unwrap(),
        };
        let complete = SessionSetup::try_from_snapshot(snap.clone()).unwrap();
        let request = r.client.strategy().unwrap().new_session(&complete).unwrap();
        assert_eq!(
            request
                .params()
                .object()
                .unwrap()
                .get("mcpServers")
                .unwrap()
                .unwrap()
                .array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(request.setup_stamp().unwrap().revision, 7);
        snap.wire = if version == Version::V1 {
            Version::V2
        } else {
            Version::V1
        };
        assert!(SessionSetup::try_from_snapshot(snap).is_err());
        r.close().await;
    }
}
#[tokio::test]
async fn cf01_cfg10_listing_over_4096_and_directory_knowledge() {
    for version in [Version::V1, Version::V2] {
        let r = Rig::ready(version).await;
        let s = r.client.strategy().unwrap();
        let mut sessions = vec![];
        for i in 0..4097 {
            sessions.push(
                json!({"sessionId":format!("s{i}"),"cwd":"C:/synthetic","extension":{"i":i}}),
            );
        }
        let raw = raw_value(&json!({"sessions":sessions}));
        let listing = s.session_listing(raw.as_ref()).unwrap();
        assert_eq!(listing.sessions().len(), 4097);
        assert_eq!(listing.sessions()[4096].session().as_str(), "s4096");
        assert_eq!(listing.sessions()[0].cwd(), "C:/synthetic");
        assert!(listing.next_cursor().is_none());
        assert_eq!(listing.raw().fragment(), raw.as_ref().fragment());
        for (value, tag) in [
            (None, 0),
            (Some(json!([])), 1),
            (Some(json!(["C:/a"])), 2),
            (Some(json!(null)), 3),
            (Some(json!(17)), 4),
        ] {
            let mut item = json!({"sessionId":"A","cwd":"C:/synthetic"});
            if let Some(v) = value {
                item["additionalDirectories"] = v;
            }
            let raw = raw_value(&json!({"sessions":[item]}));
            let l = s.session_listing(raw.as_ref()).unwrap();
            let e = &l.sessions()[0];
            assert!(!e.raw().fragment().is_empty());
            let dirs = e.additional_directories();
            match tag {
                0 => assert!(matches!(dirs, ReportedDirectories::Omitted)),
                1 => {
                    assert!(matches!(dirs, ReportedDirectories::Empty));
                    assert_eq!(dirs.known().unwrap().len(), 0);
                }
                2 => {
                    assert!(matches!(dirs, ReportedDirectories::Known(_)));
                    assert_eq!(dirs.known().unwrap()[0].as_ref(), "C:/a");
                }
                3 => assert!(matches!(dirs, ReportedDirectories::Null)),
                _ => assert!(matches!(dirs, ReportedDirectories::Uninterpreted(_))),
            }
            if tag == 0 || tag >= 3 {
                assert!(dirs.known().is_none());
            }
        }
        r.close().await;
    }
}
