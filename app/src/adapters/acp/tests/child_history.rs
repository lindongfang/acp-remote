use super::*;
use crate::adapters::acp::child_history::{ChildHistoryAssembly, ChildHistoryProgress, KEY};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

async fn negotiated(version: Version, caps: Value) -> (Client, Peer) {
    let (mut client, mut peer) = pair();
    let id = client.initialize(version, deadline()).await.unwrap();
    sent(&mut peer).await;
    let mut result = init_result(version);
    result[if version == Version::V1 {
        "agentCapabilities"
    } else {
        "capabilities"
    }] = caps;
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":id.json(),"result":result}),
    )
    .await;
    client.next(deadline()).await.unwrap();
    (client, peer)
}

#[tokio::test]
async fn child_history_negotiation_and_method_rejection_are_connection_local() {
    for version in [Version::V1, Version::V2] {
        let root = parse_session(&json!("root")).unwrap();
        let child = parse_session(&json!("child")).unwrap();
        for invalid in [
            json!({}),
            json!({"loadSession":true}),
            json!({KEY:{"version":1}}),
            json!({"_meta":{KEY:{"version":2}}}),
            json!({"_meta":{KEY:{"method":"ignored"}}}),
        ] {
            let (client, _) = negotiated(version, invalid).await;
            assert_eq!(
                client
                    .strategy()
                    .unwrap()
                    .0
                    .child_history(&root, &child, None)
                    .unwrap_err(),
                AcpError::UnsupportedCapability
            );
        }
        let (mut client, mut peer) = negotiated(
            version,
            json!({"_meta":{KEY:{"version":1,"method":"ignored"}}}),
        )
        .await;
        let strategy = client.strategy().unwrap().0.clone();
        let request = strategy.child_history(&root, &child, None).unwrap();
        let id = client.request(request, deadline()).await.unwrap();
        let sent = sent(&mut peer).await;
        assert_eq!(
            sent["method"],
            "_io.github.lindongfang.acp-remote/child-history"
        );
        assert_eq!(
            sent["params"],
            json!({"rootSessionId":"root","sessionId":"child","cursor":null})
        );
        feed(
            &mut peer,
            json!({"jsonrpc":"2.0","id":id.json(),"error":{"code":-32601,"message":"unsupported"}}),
        )
        .await;
        client.next(deadline()).await.unwrap();
        assert_eq!(
            strategy.child_history(&root, &child, None).unwrap_err(),
            AcpError::UnsupportedCapability
        );
    }
}

#[tokio::test]
async fn child_history_chunk_chain_preserves_split_utf8_and_raw_fields() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer) =
            negotiated(version, json!({"_meta":{KEY:{"version":1}}})).await;
        let root = parse_session(&json!("root")).unwrap();
        let child = parse_session(&json!("child")).unwrap();
        let strategy = client.strategy().unwrap().0.clone();
        let mut assembly = ChildHistoryAssembly::new(strategy.clone(), root.clone(), child.clone());
        let frame = r#"{ "jsonrpc":"2.0","method":"session/update","params":{"sessionId":"child","update":{"sessionUpdate":"future","unknown":["字",1.00]}}}"#;
        let stream = format!("{frame}\n");
        let split = stream.find('字').unwrap() + 1;
        for (index, chunk) in [&stream.as_bytes()[..split], &stream.as_bytes()[split..]]
            .into_iter()
            .enumerate()
        {
            let cursor = (index == 1).then_some("next");
            let id = client
                .request(
                    strategy.child_history(&root, &child, cursor).unwrap(),
                    deadline(),
                )
                .await
                .unwrap();
            assert_eq!(sent(&mut peer).await["params"]["cursor"], json!(cursor));
            feed(&mut peer,json!({"jsonrpc":"2.0","id":id.json(),"result":{"rootSessionId":"root","sessionId":"child","sequence":index.to_string(),"snapshotId":"a","nextCursor":(index==0).then_some("next"),"chunk":URL_SAFE_NO_PAD.encode(chunk)}})).await;
            let Incoming::Response {
                response: Response::ChildHistory,
                raw,
                ..
            } = client.next(deadline()).await.unwrap()
            else {
                panic!("missing correlated response")
            };
            match assembly.accept(&raw).unwrap() {
                ChildHistoryProgress::More {
                    cursor,
                    assembly: next,
                } => {
                    assert_eq!(index, 0);
                    assert_eq!(&*cursor, "next");
                    assembly = next;
                }
                ChildHistoryProgress::Complete(frames) => {
                    assert_eq!(index, 1);
                    assert_eq!(frames.len(), 1);
                    assert_eq!(frames[0].bytes(), frame.as_bytes());
                    break;
                }
            }
        }
    }
}

#[tokio::test]
async fn child_history_rejects_invalid_batches_without_exposing_staged_frames() {
    let (client, _peer) = negotiated(Version::V1, json!({"_meta":{KEY:{"version":1}}})).await;
    let strategy = client.strategy().unwrap().0.clone();
    let new = || {
        ChildHistoryAssembly::new(
            strategy.clone(),
            parse_session(&json!("r")).unwrap(),
            parse_session(&json!("c")).unwrap(),
        )
    };
    let reply = |sequence: &str, chunk: &str, next: Value| json!({"rootSessionId":"r","sessionId":"c","snapshotId":"a","sequence":sequence,"chunk":chunk,"nextCursor":next});
    let message = |result: Value| {
        RawMessage::parse(
            json!({"jsonrpc":"2.0","id":1,"result":result})
                .to_string()
                .into_bytes(),
        )
        .unwrap()
    };
    let frame = |session: &str, method: &str| {
        format!(
            "{}\n",
            json!({"jsonrpc":"2.0","method":method,"params":{"sessionId":session,"update":{"sessionUpdate":"unknown"}}})
        )
    };
    let valid = URL_SAFE_NO_PAD.encode(frame("c", "session/update"));
    let first = || match new()
        .accept(&message(reply("0", &valid, json!("more"))))
        .unwrap()
    {
        ChildHistoryProgress::More { assembly, .. } => assembly,
        ChildHistoryProgress::Complete(_) => panic!("expected continuation"),
    };
    for (key, value) in [
        ("rootSessionId", json!("other")),
        ("sessionId", json!("sibling")),
        ("snapshotId", json!("")),
        ("snapshotId", json!("b")),
        ("sequence", json!(1)),
        ("sequence", json!("01")),
        ("sequence", json!("0")),
        ("sequence", json!("2")),
        ("nextCursor", json!("more")),
        ("chunk", json!("")),
        ("chunk", json!("==")),
        (
            "chunk",
            json!(URL_SAFE_NO_PAD.encode(vec![b'a'; 16 * 1024 + 1])),
        ),
        ("chunk", json!(URL_SAFE_NO_PAD.encode([0xff, 10]))),
        (
            "chunk",
            json!(URL_SAFE_NO_PAD.encode(frame("sibling", "session/update"))),
        ),
        (
            "chunk",
            json!(URL_SAFE_NO_PAD.encode(frame("c", "session/prompt"))),
        ),
        (
            "chunk",
            json!(URL_SAFE_NO_PAD.encode(frame("c", "session/update").trim_end())),
        ),
    ] {
        let mut batch = reply("1", &valid, Value::Null);
        batch[key] = value;
        assert!(first().accept(&message(batch)).is_err(), "invalid {key}");
    }
    assert!(
        new()
            .accept(&message(reply("0", "", json!("more"))))
            .is_err()
    );
    assert!(
        matches!(new().accept(&message(reply("0","",Value::Null))).unwrap(),ChildHistoryProgress::Complete(frames) if frames.is_empty())
    );
}

#[tokio::test]
async fn child_history_timeout_and_late_reply_leave_root_connection_usable() {
    let (mut client, mut peer) = negotiated(
        Version::V2,
        json!({"_meta":{KEY:{"version":1}},"session":{}}),
    )
    .await;
    let strategy = client.strategy().unwrap().0.clone();
    let child = parse_session(&json!("child")).unwrap();
    let root = parse_session(&json!("root")).unwrap();
    let until = deadline();
    let id = client
        .request(strategy.child_history(&root, &child, None).unwrap(), until)
        .await
        .unwrap();
    sent(&mut peer).await;
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":"foreign","result":{}}),
    )
    .await;
    tokio::time::sleep_until(until + Duration::from_millis(10)).await;
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":id.json(),"result":{}}),
    )
    .await;
    let list = client
        .strategy()
        .unwrap()
        .list_sessions(None, None)
        .unwrap();
    let id = client.request(list, deadline()).await.unwrap();
    assert_eq!(sent(&mut peer).await["method"], "session/list");
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":id.json(),"result":{"sessions":[]}}),
    )
    .await;
    assert!(matches!(
        client.next(deadline()).await.unwrap(),
        Incoming::Response {
            response: Response::Sessions(_),
            ..
        }
    ));
}
