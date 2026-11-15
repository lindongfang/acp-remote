use super::*;

const TAGS: [&str; 3] = [
    "user_message_chunk",
    "agent_message_chunk",
    "agent_thought_chunk",
];
fn update(tag: &str, id: Option<Value>) -> Value {
    let mut frame = json!({"jsonrpc":"2.0","method":"session/update","params":{
        "sessionId":"s","update":{"sessionUpdate":tag,
            "content":{"type":"text","text":"<script>opaque</script>"},
            "future":{"array":[null,false,{"keep":"raw"}]}}}});
    if let Some(id) = id {
        frame["params"]["update"]["messageId"] = id;
    }
    frame
}
fn check(incoming: Incoming, tag: &str, id: Option<&str>, origin: UpdateOrigin, bytes: &[u8]) {
    let Incoming::Update {
        event:
            SessionEvent::Message {
                session,
                message,
                kind,
            },
        origin: actual_origin,
        raw,
    } = incoming
    else {
        panic!("expected message update");
    };
    assert_eq!(session.as_str(), "s");
    assert_eq!(kind.as_ref(), tag);
    assert_eq!(message.as_ref().map(MessageId::as_str), id);
    assert_eq!(actual_origin, origin);
    assert_eq!(raw.bytes(), bytes);
}

#[tokio::test]
async fn v1_nullable_message_ids_preserve_tags_raw_order_and_the_connection() {
    for offer in [Version::V1, Version::V2] {
        let (mut client, mut peer) = ready(offer, Version::V1).await;
        for tag in TAGS {
            for id in [
                None,
                Some(Value::Null),
                Some(json!("native")),
                Some(json!("")),
            ] {
                let expected_id = id.as_ref().and_then(Value::as_str).map(str::to_owned);
                let frame = update(tag, id);
                let bytes = serde_json::to_vec(&frame).unwrap();
                feed(&mut peer, frame).await;
                check(
                    client.next(deadline()).await.unwrap(),
                    tag,
                    expected_id.as_deref(),
                    UpdateOrigin::Live,
                    &bytes,
                );
            }
        }
    }
}

#[tokio::test]
async fn v1_only_null_is_absent_and_native_id_limits_are_utf8_bytes() {
    let at_limit = format!("{}a", "界".repeat(1365));
    assert_eq!(at_limit.len(), 4096);
    for tag in TAGS {
        let (mut client, mut peer) = ready(Version::V1, Version::V1).await;
        let frame = update(tag, Some(json!(at_limit)));
        let bytes = serde_json::to_vec(&frame).unwrap();
        feed(&mut peer, frame).await;
        check(
            client.next(deadline()).await.unwrap(),
            tag,
            Some(&at_limit),
            UpdateOrigin::Live,
            &bytes,
        );
        for invalid in [json!(5), json!(true), json!([]), json!({})] {
            let (mut client, mut peer) = ready(Version::V1, Version::V1).await;
            feed(&mut peer, update(tag, Some(invalid))).await;
            assert_eq!(
                client.next(deadline()).await.unwrap_err(),
                AcpError::InvalidMessage
            );
        }
    }
}

#[tokio::test]
async fn v2_coarse_message_projection_keeps_its_existing_id_rules() {
    for tag in TAGS {
        let (mut client, mut peer) = ready(Version::V2, Version::V2).await;
        for id in [None, Some(json!("native"))] {
            let expected_id = id.as_ref().and_then(Value::as_str).map(str::to_owned);
            let frame = update(tag, id);
            let bytes = serde_json::to_vec(&frame).unwrap();
            feed(&mut peer, frame).await;
            check(
                client.next(deadline()).await.unwrap(),
                tag,
                expected_id.as_deref(),
                UpdateOrigin::Live,
                &bytes,
            );
        }
        for invalid in [Value::Null, json!(5), json!(false), json!([]), json!({})] {
            let (mut client, mut peer) = ready(Version::V2, Version::V2).await;
            feed(&mut peer, update(tag, Some(invalid))).await;
            assert_eq!(
                client.next(deadline()).await.unwrap_err(),
                AcpError::InvalidMessage
            );
        }
    }
}

#[tokio::test]
async fn nullable_v1_id_remains_an_observation_during_replay_and_after_its_response() {
    let (mut client, mut peer) = pair();
    let initialize = client.initialize(Version::V1, deadline()).await.unwrap();
    sent(&mut peer).await;
    let mut result = init_result(Version::V1);
    result["agentCapabilities"]["loadSession"] = json!(true);
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":initialize.json(),"result":result}),
    )
    .await;
    client.next(deadline()).await.unwrap();
    let request = client
        .strategy()
        .unwrap()
        .replay(parse_session(&json!("s")).unwrap(), "C:/synthetic")
        .unwrap();
    let id = client.request(request, deadline()).await.unwrap();
    assert_eq!(sent(&mut peer).await["method"], "session/load");
    let frame = update("agent_message_chunk", Some(Value::Null));
    let bytes = serde_json::to_vec(&frame).unwrap();
    feed(&mut peer, frame.clone()).await;
    check(
        client.next(deadline()).await.unwrap(),
        "agent_message_chunk",
        None,
        UpdateOrigin::DuringReplay,
        &bytes,
    );
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":id.json(),"result":{}}),
    )
    .await;
    assert!(matches!(
        client.next(deadline()).await.unwrap(),
        Incoming::Response {
            response: Response::Replayed { .. },
            ..
        }
    ));
    feed(&mut peer, frame).await;
    check(
        client.next(deadline()).await.unwrap(),
        "agent_message_chunk",
        None,
        UpdateOrigin::Live,
        &bytes,
    );
}
