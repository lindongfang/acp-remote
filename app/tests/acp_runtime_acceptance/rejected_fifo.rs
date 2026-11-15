#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use support::*;

#[tokio::test]
async fn rc06_each_bad_route_raw_is_fifo_even_after_stop_invalid_json_is_not_raw() {
    for version in [Version::V1, Version::V2] {
        let mut r = Rig::ready(version).await;
        let first = br#"  {"jsonrpc":"2.0","id":"missing-a","result":{"opaque":1e9999}}  "#;
        let second = br#"{"jsonrpc":"2.0","id":false,"result":{"lex":"\u0061"}}"#;
        let tail = notification("A");
        let mut batch = first.to_vec();
        batch.push(b'\n');
        batch.extend_from_slice(second);
        batch.push(b'\n');
        batch.extend_from_slice(b"{\n");
        batch.extend_from_slice(&tail);
        batch.push(b'\n');
        r.read.push(batch);
        let received = r.next().await.unwrap().unwrap();
        assert_eq!(received.raw().bytes(), tail);
        assert!(matches!(r.next().await, Err(AcpError::UnmatchedResponse)));
        assert!(r.next().await.unwrap().is_none());
        let mut rejected = vec![];
        while let Some(message) = r.client.take_rejected_message() {
            rejected.push(message);
        }
        println!(
            "REJECTED001 version={version:?} complete_bad_route_objects=2 retained={}",
            rejected.len()
        );
        assert_eq!(
            rejected.len(),
            2,
            "every syntactically valid rejected object must be retained, invalid JSON must not mint RawMessage"
        );
        assert_eq!(rejected[0].bytes(), first);
        assert_eq!(rejected[1].bytes(), second);
        assert!(r.client.take_rejected_message().is_none());
        r.close().await;
    }
}
