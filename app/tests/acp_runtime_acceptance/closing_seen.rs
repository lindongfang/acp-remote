#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use support::*;

#[tokio::test]
async fn rc06_server_seen_has_no_closing_drain_exemption_for_same_read_batch() {
    let mut duplicates = 0;
    for permission_kind in [true, false] {
        let mut r = Rig::ready(Version::V2).await;
        let res = r.reserve_prompt();
        let t = send(&r.client, res);
        finish(&t).await;
        let bad = br#"{"jsonrpc":"2.0","id":"unmatched-first","result":{}}"#;
        let id = RpcId::Integer(812);
        let request = if permission_kind {
            permission(Version::V2, &id)
        } else {
            serde_json::to_vec(
                &json!({"jsonrpc":"2.0","id":812,"method":"_future/request","params":{}}),
            )
            .unwrap()
        };
        let tail = notification("A");
        let mut batch = bad.to_vec();
        batch.push(b'\n');
        batch.extend_from_slice(&request);
        batch.push(b'\n');
        batch.extend_from_slice(&request);
        batch.push(b'\n');
        batch.extend_from_slice(&tail);
        batch.push(b'\n');
        r.read.push(batch);
        let first = r.next().await.unwrap().unwrap();
        assert_eq!(first.raw().bytes(), request);
        assert!(matches!(
            first.incoming(),
            Incoming::Permission(_) | Incoming::UnsupportedRequest(_)
        ));
        let next = r.next().await.unwrap().unwrap();
        let repeated = matches!(
            next.incoming(),
            Incoming::Permission(_) | Incoming::UnsupportedRequest(_)
        );
        let tail_received = if repeated {
            assert_eq!(next.raw().bytes(), request);
            assert_eq!(first.receipt_order(), next.receipt_order());
            duplicates += 1;
            r.next().await.unwrap().unwrap()
        } else {
            next
        };
        assert_eq!(tail_received.raw().bytes(), tail);
        assert_eq!(first.receipt_order(), tail_received.receipt_order());
        println!(
            "SEEN001 permission={permission_kind} duplicate_emitted={repeated} same_read_cut=true"
        );
        assert_eq!(r.client.server_pending_count(), 0);
        assert!(matches!(r.next().await, Err(AcpError::UnmatchedResponse)));
        assert!(r.next().await.unwrap().is_none());
        r.close().await;
    }
    assert_eq!(
        duplicates, 0,
        "closing drain retains lifetime server ID uniqueness without minting reply grants"
    );
}
