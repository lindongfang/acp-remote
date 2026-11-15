use super::*;

#[tokio::test]
async fn sequential_peer_requests_do_not_exhaust_connection_lifetime() {
    for version in [Version::V1, Version::V2] {
        let (mut connection, mut peer) = ready(version, version).await;
        for sequence in 0..5_000 {
            let id = json!(sequence);
            if sequence % 2 == 0 {
                feed(&mut peer, permission(version, &id)).await;
                let Incoming::Permission(request) = connection
                    .next(deadline())
                    .await
                    .unwrap_or_else(|error| panic!("request {sequence} on {version:?}: {error}"))
                else {
                    panic!("expected permission request")
                };
                connection
                    .permission_response(&request, None, deadline())
                    .await
                    .expect("cancel permission once");
                let response = sent(&mut peer).await;
                assert_eq!(response["id"], id);
                assert_eq!(response["result"]["outcome"]["outcome"], "cancelled");
            } else {
                feed(
                    &mut peer,
                    json!({"jsonrpc":"2.0","id":id,"method":"future/method"}),
                )
                .await;
                let Incoming::UnsupportedRequest(request_id) = connection
                    .next(deadline())
                    .await
                    .unwrap_or_else(|error| panic!("request {sequence} on {version:?}: {error}"))
                else {
                    panic!("expected unsupported request")
                };
                connection
                    .reject_unsupported(request_id.id(), deadline())
                    .await
                    .expect("reject once");
                let response = sent(&mut peer).await;
                assert_eq!(response["id"], id);
                assert_eq!(response["error"]["code"], -32601);
            }
            assert!(!connection.is_closed());
        }

        let request = connection
            .strategy()
            .expect("still initialized")
            .new_session("C:/fixture")
            .expect("request");
        let id = connection
            .request(request, deadline())
            .await
            .expect("client can still send after 5000 peer requests");
        assert_eq!(sent(&mut peer).await["method"], "session/new");
        feed(
            &mut peer,
            json!({"jsonrpc":"2.0","id":id.json(),"result":{"sessionId":"after-many-requests"}}),
        )
        .await;
        assert!(matches!(
            connection.next(deadline()).await.expect("response"),
            Incoming::Response {
                response: Response::NewSession(_),
                ..
            }
        ));
        assert!(!connection.is_closed());

        // Removing the lifetime cap must not allow a previously consumed peer ID to replay.
        feed(&mut peer, permission(version, &json!(0))).await;
        assert_eq!(
            connection.next(deadline()).await.expect_err("replayed ID"),
            AcpError::InvalidMessage
        );
        assert!(connection.is_closed());
    }
}
