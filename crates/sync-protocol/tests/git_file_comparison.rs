use sync_protocol::command::SessionGitDiffResult;

#[test]
fn shared_fixture_round_trips_complete_before_after_text() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/sync/v1/valid/command-result-session-git-diff-completed.json"
    ))
    .expect("shared fixture");
    let result = fixture["body"]["result"].clone();
    let decoded: SessionGitDiffResult =
        serde_json::from_value(result.clone()).expect("complete comparison");
    assert!(decoded.comparison.is_some());
    assert_eq!(serde_json::to_value(decoded).expect("encode"), result);
    for comparison in [
        serde_json::json!({"before":""}),
        serde_json::json!({"before":"","after":"","unknown":true}),
        serde_json::Value::Null,
        serde_json::json!({"before":"x".repeat(65537),"after":""}),
    ] {
        let mut invalid = result.clone();
        invalid["comparison"] = comparison;
        assert!(serde_json::from_value::<SessionGitDiffResult>(invalid).is_err());
    }
}
