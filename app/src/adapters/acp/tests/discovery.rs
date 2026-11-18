use super::*;

async fn discovery_ready(version: Version) -> (Client, Peer) {
    let (mut client, mut peer) = pair();
    let id = client.initialize(version, deadline()).await.unwrap();
    sent(&mut peer).await;
    let mut result = init_result(version);
    if version == Version::V1 {
        result["agentCapabilities"]["loadSession"] = json!(true);
        result["agentCapabilities"]["sessionCapabilities"]["list"] = json!({});
    }
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":id.json(),"result":result}),
    )
    .await;
    client.next(deadline()).await.unwrap();
    (client, peer)
}
fn sid(value: &str) -> SessionId {
    parse_session(&json!(value)).unwrap()
}
fn reading(client: &Client, replay: bool, session: &str) -> Request {
    let strategy = client.strategy().unwrap();
    if replay {
        strategy.replay(sid(session), "C:/synthetic").unwrap()
    } else {
        strategy.resume(sid(session), "C:/synthetic").unwrap()
    }
}
async fn no_write(peer: &mut Peer) {
    assert!(
        timeout(Duration::from_millis(15), peer.0.fill_buf())
            .await
            .is_err()
    );
}
async fn state(peer: &mut Peer, session: &str, value: &str) {
    feed(
        peer,
        json!({"jsonrpc":"2.0","method":"session/update","params":{
        "sessionId":session,"update":{"sessionUpdate":"state_update","state":value}}}),
    )
    .await;
}
async fn finish_read(
    client: &mut Client,
    peer: &mut Peer,
    id: &RpcId,
    session: &str,
    version: Version,
    replay: bool,
) {
    if version == Version::V2 {
        if replay {
            feed(
                peer,
                json!({"jsonrpc":"2.0","method":"session/update","params":{
                "sessionId":session,"update":{"sessionUpdate":"replay_complete"}}}),
            )
            .await;
            client.next(deadline()).await.unwrap();
        }
        state(peer, session, "idle").await;
        client.next(deadline()).await.unwrap();
    }
    feed(peer, json!({"jsonrpc":"2.0","id":id.json(),"result":{}})).await;
    assert!(matches!(
        client.next(deadline()).await.unwrap(),
        Incoming::Response {
            response: Response::Replayed { .. } | Response::Resumed { .. },
            ..
        }
    ));
}

#[tokio::test]
async fn preferred_offers_highest_once_and_records_the_actual_selection() {
    for selected in [Version::V1, Version::V2] {
        let (mut client, mut peer) = pair();
        let id = client.initialize_preferred(deadline()).await.unwrap();
        assert_eq!(sent(&mut peer).await["params"]["protocolVersion"], 2);
        feed(
            &mut peer,
            json!({"jsonrpc":"2.0","id":id.json(),"result":init_result(selected)}),
        )
        .await;
        client.next(deadline()).await.unwrap();
        let negotiated = client.strategy().unwrap().negotiated();
        assert_eq!(negotiated.offered, Version::V2);
        assert_eq!(negotiated.version, selected);
        assert_eq!(
            client.initialize_preferred(deadline()).await.unwrap_err(),
            AcpError::AlreadyInitialized
        );
        no_write(&mut peer).await;
    }
}

#[tokio::test]
async fn v1_list_replay_and_resume_are_independent_capabilities() {
    for list in [false, true] {
        for replay in [false, true] {
            for resume in [false, true] {
                let (mut client, mut peer) = pair();
                let id = client.initialize_preferred(deadline()).await.unwrap();
                sent(&mut peer).await;
                let mut capabilities = json!({"loadSession":replay,"sessionCapabilities":{}});
                if list {
                    capabilities["sessionCapabilities"]["list"] = json!({});
                }
                if resume {
                    capabilities["sessionCapabilities"]["resume"] = json!({});
                }
                feed(&mut peer,json!({"jsonrpc":"2.0","id":id.json(),"result":{"protocolVersion":1,"agentCapabilities":capabilities}})).await;
                client.next(deadline()).await.unwrap();
                let strategy = client.strategy().unwrap();
                assert_eq!(strategy.negotiated().capabilities.list, list);
                assert_eq!(strategy.negotiated().capabilities.standard_replay, replay);
                assert_eq!(strategy.list_sessions(None, None).is_ok(), list);
                assert_eq!(
                    strategy
                        .replay(sid("opaque-not-a-platform-id"), "C:/test")
                        .is_ok(),
                    replay
                );
                assert_eq!(
                    strategy
                        .resume(sid("opaque-not-a-platform-id"), "C:/test")
                        .is_ok(),
                    resume
                );
                assert_eq!(client.pending_count(), 0);
                no_write(&mut peer).await;
            }
        }
    }
}

#[tokio::test]
async fn session_listing_omits_optional_parameters_and_preserves_order_and_raw_fields() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer) = discovery_ready(version).await;
        for (cwd, cursor) in [
            (None, None),
            (Some("C:/test"), None),
            (None, Some("opaque")),
            (Some("C:/test"), Some("opaque")),
        ] {
            let request = client
                .strategy()
                .unwrap()
                .list_sessions(cwd, cursor)
                .unwrap();
            let id = client.request(request, deadline()).await.unwrap();
            let frame = sent(&mut peer).await;
            assert_eq!(frame["method"], "session/list");
            assert_eq!(frame["params"].get("cwd"), cwd.map(Value::from).as_ref());
            assert_eq!(
                frame["params"].get("cursor"),
                cursor.map(Value::from).as_ref()
            );
            let result = json!({"sessions":[{"sessionId":"z","cwd":"C:/second","title":"A","_meta":{"future":{"keep":[1,true]}}},
                {"sessionId":"a","cwd":"C:/first","title":"Z"}],"nextCursor":"opaque-next","future":{"also":"retained"}});
            feed(
                &mut peer,
                json!({"jsonrpc":"2.0","id":id.json(),"result":result}),
            )
            .await;
            let Incoming::Response {
                response: Response::Sessions(list),
                raw,
                ..
            } = client.next(deadline()).await.unwrap()
            else {
                panic!("listing")
            };
            assert_eq!(
                list.sessions
                    .iter()
                    .map(|item| item.session.as_str())
                    .collect::<Vec<_>>(),
                ["z", "a"]
            );
            assert_eq!(list.next_cursor.as_deref(), Some("opaque-next"));
            assert_eq!(json_fields(&raw)["result"], result);
            assert!(!format!("{list:?}").contains("C:/second"));
        }
    }
}

#[tokio::test]
async fn listing_and_parameter_budgets_reject_without_truncation_or_premature_write() {
    let (mut client, mut peer) = discovery_ready(Version::V2).await;
    let boundary = format!("{}a", "界".repeat(1365));
    assert_eq!(boundary.len(), 4096);
    assert!(
        client
            .strategy()
            .unwrap()
            .list_sessions(None, Some(&boundary))
            .is_ok()
    );
    assert_eq!(
        client
            .strategy()
            .unwrap()
            .list_sessions(None, Some(&(boundary + "a")))
            .err(),
        Some(AcpError::Capacity)
    );
    no_write(&mut peer).await;
    for count in [4096, 4097] {
        let request = client
            .strategy()
            .unwrap()
            .list_sessions(None, None)
            .unwrap();
        let id = client.request(request, deadline()).await.unwrap();
        sent(&mut peer).await;
        feed(&mut peer,json!({"jsonrpc":"2.0","id":id.json(),"result":{"sessions":vec![json!({"sessionId":"s","cwd":"/"});count]}})).await;
        assert!(
            matches!(client.next(deadline()).await.unwrap(),Incoming::Response{response:Response::Sessions(list),..} if list.sessions.len()==count)
        );
    }
}

#[tokio::test]
async fn invalid_required_list_fields_and_cursor_do_not_publish_partial_lists() {
    for result in [
        json!({}),
        json!({"sessions":false}),
        json!({"sessions":[{"sessionId":"s"}]}),
        json!({"sessions":[{"sessionId":42,"cwd":"/"}]}),
        json!({"sessions":[],"nextCursor":42}),
    ] {
        let (mut client, mut peer) = discovery_ready(Version::V2).await;
        let request = client
            .strategy()
            .unwrap()
            .list_sessions(None, None)
            .unwrap();
        let id = client.request(request, deadline()).await.unwrap();
        sent(&mut peer).await;
        feed(
            &mut peer,
            json!({"jsonrpc":"2.0","id":id.json(),"result":result}),
        )
        .await;
        assert_eq!(
            client.next(deadline()).await.unwrap_err(),
            AcpError::InvalidMessage
        );
        assert!(client.is_closed());
        assert!(client.take_rejected_message().is_some());
    }
}

#[tokio::test]
async fn standard_replay_uses_native_id_and_version_specific_method_without_substitutes() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer) = discovery_ready(version).await;
        let request = reading(&client, true, "opaque-native");
        let id = client.request(request, deadline()).await.unwrap();
        let frame = sent(&mut peer).await;
        assert_eq!(
            frame["method"],
            if version == Version::V1 {
                "session/load"
            } else {
                "session/resume"
            }
        );
        assert_eq!(frame["params"]["sessionId"], "opaque-native");
        assert_eq!(frame["params"]["mcpServers"], json!([]));
        assert_eq!(
            frame["params"].get("replayFrom"),
            (version == Version::V2)
                .then(|| json!({"type":"start"}))
                .as_ref()
        );
        finish_read(&mut client, &mut peer, &id, "opaque-native", version, true).await;
        no_write(&mut peer).await;
    }
}

#[tokio::test]
async fn resume_and_replay_share_one_window_per_session_without_blocking_other_sessions() {
    for version in [Version::V1, Version::V2] {
        for first_replay in [false, true] {
            for second_replay in [false, true] {
                let (mut client, mut peer) = discovery_ready(version).await;
                let request = reading(&client, first_replay, "A");
                let first = client.request(request, deadline()).await.unwrap();
                sent(&mut peer).await;
                let conflict = reading(&client, second_replay, "A");
                assert_eq!(
                    client.request(conflict, deadline()).await.unwrap_err(),
                    AcpError::SessionReadBusy
                );
                assert_eq!(client.pending_count(), 1);
                no_write(&mut peer).await;
                let other = reading(&client, true, "B");
                let other = client.request(other, deadline()).await.unwrap();
                sent(&mut peer).await;
                assert_eq!(client.pending_count(), 2);
                finish_read(&mut client, &mut peer, &other, "B", version, true).await;
                assert_eq!(client.pending_count(), 1);
                client.cancel(&sid("A"), deadline()).await.unwrap();
                sent(&mut peer).await;
                let conflict = reading(&client, second_replay, "A");
                assert_eq!(
                    client.request(conflict, deadline()).await.unwrap_err(),
                    AcpError::SessionReadBusy
                );
                finish_read(&mut client, &mut peer, &first, "A", version, first_replay).await;
                let explicit = reading(&client, second_replay, "A");
                client.request(explicit, deadline()).await.unwrap();
                sent(&mut peer).await;
                client.shutdown(deadline()).await.unwrap();
            }
        }
    }
}

#[tokio::test]
async fn replay_window_keeps_permissions_independent_and_does_not_label_other_sessions_as_history()
{
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer) = discovery_ready(version).await;
        let request = reading(&client, true, "A");
        let id = client.request(request, deadline()).await.unwrap();
        sent(&mut peer).await;
        for (session, expected) in [
            ("A", UpdateOrigin::DuringReplay),
            ("B", UpdateOrigin::Live),
            ("A", UpdateOrigin::DuringReplay),
        ] {
            let update = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":{"sessionUpdate":"future","payload":{"same":"text","nested":[true]}}}});
            feed(&mut peer, update.clone()).await;
            let Incoming::Update { origin, raw, .. } = client.next(deadline()).await.unwrap()
            else {
                panic!("update")
            };
            assert_eq!(origin, expected);
            assert_eq!(json_fields(&raw)["params"], update["params"]);
        }
        let options = json!([{"optionId":"deny","name":"Deny","kind":"reject_once"}]);
        let params = match version {
            Version::V1 => {
                json!({"sessionId":"A","toolCall":{"toolCallId":"tool"},"options":options})
            }
            Version::V2 => {
                json!({"sessionId":"A","title":"synthetic","subject":{"type":"tool_call","toolCall":{"toolCallId":"tool"}},"options":options})
            }
        };
        feed(&mut peer,json!({"jsonrpc":"2.0","id":"permission","method":"session/request_permission","params":params})).await;
        let Incoming::Permission(permission) = client.next(deadline()).await.unwrap() else {
            panic!("live permission")
        };
        no_write(&mut peer).await;
        client
            .permission_response(&permission, None, deadline())
            .await
            .unwrap();
        assert_eq!(
            sent(&mut peer).await["result"]["outcome"]["outcome"],
            "cancelled"
        );
        finish_read(&mut client, &mut peer, &id, "A", version, true).await;
        feed(&mut peer,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"A","update":{"sessionUpdate":"future"}}})).await;
        assert!(matches!(
            client.next(deadline()).await.unwrap(),
            Incoming::Update {
                origin: UpdateOrigin::Live,
                ..
            }
        ));
    }
}

#[tokio::test]
async fn replay_requires_its_own_v2_snapshot_and_business_error_never_retries() {
    for replay in [false, true] {
        for wrong_session in [false, true] {
            let (mut client, mut peer) = discovery_ready(Version::V2).await;
            state(&mut peer, "A", "idle").await;
            client.next(deadline()).await.unwrap(); // Before this read is not its snapshot.
            let request = reading(&client, replay, "A");
            let id = client.request(request, deadline()).await.unwrap();
            sent(&mut peer).await;
            if wrong_session {
                state(&mut peer, "B", "idle").await;
                client.next(deadline()).await.unwrap();
            }
            feed(
                &mut peer,
                json!({"jsonrpc":"2.0","id":id.json(),"result":{}}),
            )
            .await;
            assert_eq!(
                client.next(deadline()).await.unwrap_err(),
                AcpError::MissingSnapshot
            );
            assert!(client.is_closed());
        }
    }
    let (mut client, mut peer) = discovery_ready(Version::V2).await;
    let request = reading(&client, true, "A");
    let id = client.request(request, deadline()).await.unwrap();
    sent(&mut peer).await;
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":id.json(),"error":{"code":-32000,"message":"synthetic"}}),
    )
    .await;
    assert!(matches!(
        client.next(deadline()).await.unwrap(),
        Incoming::Response {
            response: Response::RemoteError { .. },
            ..
        }
    ));
    assert_eq!(client.pending_count(), 0);
    no_write(&mut peer).await;
    let request = reading(&client, false, "A");
    let id = client.request(request, deadline()).await.unwrap();
    sent(&mut peer).await;
    state(&mut peer, "A", "future_state").await;
    assert!(
        matches!(client.next(deadline()).await.unwrap(),Incoming::Update{event:SessionEvent::State{state,..},origin:UpdateOrigin::DuringResume,..} if state.as_ref()=="future_state")
    );
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":id.json(),"result":{}}),
    )
    .await;
    assert!(matches!(
        client.next(deadline()).await.unwrap(),
        Incoming::Response {
            response: Response::Resumed { .. },
            ..
        }
    ));
}
