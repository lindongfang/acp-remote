#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use support::*;

#[tokio::test]
async fn rt08_known_embedded_resource_requires_uri_and_text_or_blob_anyof() {
    let mut invalid_accepted = 0;
    for version in [Version::V1, Version::V2] {
        let r = Rig::ready(version).await;
        let s = r.client.strategy().unwrap();
        assert!(s.negotiated().capabilities.embedded_context);
        for (name, resource) in [
            ("empty", "{}"),
            ("missing_uri", "{\"text\":\"text\"}"),
            ("uri_wrong_type", "{\"uri\":7,\"text\":\"text\"}"),
            ("no_text_or_blob", "{\"uri\":\"synthetic://resource\"}"),
            (
                "wrong_payload_types",
                "{\"uri\":\"synthetic://resource\",\"text\":false,\"blob\":7}",
            ),
        ] {
            let prompt = owned(format!(
                "[{{\"type\":\"resource\",\"resource\":{resource}}}]"
            ));
            let result = s.prompt(sid("A"), prompt, ExtensionFields::empty().unwrap());
            println!(
                "RT08 resource version={version:?} case={name} expected_rejected=true actual_rejected={}",
                result.is_err()
            );
            if result.is_ok() {
                invalid_accepted += 1;
            }
        }
        for resource in [
            r#"{"uri":"synthetic://resource","text":"text","future":1e99999}"#,
            r#"{"uri":"synthetic://resource","blob":"AA==","future":1e99999}"#,
            r#"{"uri":"synthetic://resource","text":"text","blob":"AA==","future":1e99999}"#,
            r#"{"uri":"synthetic://resource","text":false,"blob":"AA==","future":1e99999}"#,
        ] {
            let prompt = owned(format!(
                "[{{\"type\":\"resource\",\"resource\":{resource}}}]"
            ));
            let req = s
                .prompt(sid("A"), prompt, ExtensionFields::empty().unwrap())
                .unwrap();
            assert!(req.params().fragment().contains(resource));
        }
        assert!(r.write.bytes().is_empty());
        r.close().await;
    }
    assert_eq!(
        invalid_accepted, 0,
        "known embedded resource shape must reject before any outbound IO"
    );
}
