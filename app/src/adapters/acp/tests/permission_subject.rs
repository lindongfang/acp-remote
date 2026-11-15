use super::*;

#[tokio::test]
async fn standard_optional_subject_and_command_tool_are_not_unknown_variants() {
    for subject in [
        None,
        Some(Value::Null),
        Some(json!({"type":"command","command":"synthetic","cwd":"C:/fixture"})),
        Some(json!({"type":"command","command":"synthetic","cwd":"C:/fixture","toolCallId":null})),
    ] {
        let (mut connection, mut peer) = ready(Version::V2, Version::V2).await;
        let mut frame = permission(Version::V2, &json!("optional"));
        frame["params"].as_object_mut().unwrap().remove("subject");
        if let Some(subject) = subject {
            frame["params"]["subject"] = subject;
        }
        let expected = frame.clone();
        feed(&mut peer, frame).await;
        let Incoming::Permission(request) = connection.next(deadline()).await.unwrap() else {
            panic!("permission")
        };
        assert!(request.supported_subject);
        assert!(request.tool.is_none());
        assert_eq!(
            json_fields(&request.raw).get("params"),
            expected.get("params")
        );
        connection
            .permission_response(&request, Some(&request.options[0].id), deadline())
            .await
            .unwrap();
        let response = sent(&mut peer).await;
        assert_eq!(response["id"], json!("optional"));
        assert_eq!(response["result"]["outcome"]["outcome"], json!("selected"));
    }
}

#[tokio::test]
async fn malformed_subject_is_not_treated_as_absent_or_custom_variant() {
    for subject in [
        json!(true),
        json!([]),
        json!(12),
        json!("command"),
        json!({}),
        json!({"type":null}),
        json!({"type":true}),
        json!({"type":[]}),
    ] {
        let (mut connection, mut peer) = ready(Version::V2, Version::V2).await;
        let mut frame = permission(Version::V2, &json!("malformed"));
        frame["params"]["subject"] = subject;
        feed(&mut peer, frame).await;
        assert!(matches!(
            connection.next(deadline()).await,
            Err(AcpError::InvalidMessage)
        ));
        assert!(connection.is_closed());
    }
}

#[tokio::test]
async fn known_command_fields_remain_strict_when_tool_is_optional() {
    for field_name in ["command", "cwd", "toolCallId"] {
        for wrong in [json!(false), json!([]), json!({}), json!(1)] {
            let (mut connection, mut peer) = ready(Version::V2, Version::V2).await;
            let mut frame = permission(Version::V2, &json!("command"));
            frame["params"]["subject"] =
                json!({"type":"command","command":"synthetic","cwd":"C:/fixture"});
            frame["params"]["subject"][field_name] = wrong;
            feed(&mut peer, frame).await;
            assert!(matches!(
                connection.next(deadline()).await,
                Err(AcpError::InvalidMessage)
            ));
        }
    }
}

#[tokio::test]
async fn future_subject_keeps_diagnostic_and_manual_selection_is_once() {
    for tag in ["future", "_custom"] {
        let (mut connection, mut peer) = ready(Version::V2, Version::V2).await;
        let mut frame = permission(Version::V2, &json!("unknown"));
        frame["params"]["subject"] = json!({"type":tag,"payload":{"opaque":[1,null]}});
        feed(&mut peer, frame).await;
        let Incoming::Permission(request) = connection.next(deadline()).await.unwrap() else {
            panic!("permission")
        };
        assert!(!request.supported_subject);
        connection
            .permission_response(&request, Some(&request.options[0].id), deadline())
            .await
            .unwrap();
        assert_eq!(
            sent(&mut peer).await["result"]["outcome"]["outcome"],
            json!("selected")
        );
        assert!(
            connection
                .permission_response(&request, None, deadline())
                .await
                .is_err()
        );
    }
}
