use sync_protocol::command::GitStatusEntry;

#[test]
fn shared_git_status_fixture_preserves_counts_and_rejects_invalid_pairs() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/sync/v1/valid/command-result-session-git-status-completed.json"
    ))
    .expect("shared fixture");
    let entry = fixture["body"]["result"]["entries"][0].clone();
    let decoded: GitStatusEntry = serde_json::from_value(entry.clone()).expect("valid counts");
    assert_eq!(decoded.added_lines, Some(12));
    assert_eq!(decoded.deleted_lines, Some(3));
    assert_eq!(serde_json::to_value(decoded).expect("encode"), entry);
    for (key, value) in [
        ("addedLines", serde_json::json!(-1)),
        ("addedLines", serde_json::json!(0.5)),
        ("addedLines", serde_json::json!(4294967296_u64)),
        ("addedLines", serde_json::Value::Null),
        ("binary", serde_json::json!(true)),
        ("conflicted", serde_json::json!(true)),
    ] {
        let mut invalid = entry.clone();
        invalid[key] = value;
        assert!(serde_json::from_value::<GitStatusEntry>(invalid).is_err());
    }
    let mut partial = entry;
    partial
        .as_object_mut()
        .expect("object")
        .remove("deletedLines");
    assert!(serde_json::from_value::<GitStatusEntry>(partial).is_err());
}
