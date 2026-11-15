#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use support::*;

#[tokio::test]
async fn rt17_remote_error_numeric_code_stays_public_but_is_not_debug_output() {
    let mut leaks = 0;
    for version in [Version::V1, Version::V2] {
        let mut r = Rig::ready(version).await;
        let res = r.reserve_prompt();
        let id = res.id().unwrap().clone();
        let t = send(&r.client, res);
        finish(&t).await;
        let canary = -642_918_337_i64;
        let wire=serde_json::to_vec(&json!({"jsonrpc":"2.0","id":rpc_value(&id),"error":{"code":canary,"message":"synthetic-debug-canary","data":{"opaque":true}}})).unwrap();
        let received = r.feed(&wire).await;
        let Incoming::Response { response, .. } = received.incoming() else {
            panic!()
        };
        assert!(matches!(response,Response::RemoteError{code}if *code==canary));
        assert_eq!(received.raw().bytes(), wire);
        let expected_private = canary.to_string();
        for (kind, formatted) in [
            ("Response", format!("{response:?}")),
            ("Incoming", format!("{:?}", received.incoming())),
            ("Received", format!("{received:?}")),
        ] {
            let leaked = formatted.contains(&expected_private);
            println!("DEBUG001 version={version:?} kind={kind} numeric_canary_leaked={leaked}");
            leaks += usize::from(leaked);
        }
        r.close().await;
    }
    assert_eq!(
        leaks, 0,
        "Debug may not expose arbitrary remote numeric data"
    );
}
