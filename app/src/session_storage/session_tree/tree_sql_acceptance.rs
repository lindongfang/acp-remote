//! Independent SQLite port acceptance; original fixture metadata is development-only.
use super::*;
type ParentEvidence = (String, String, Vec<u8>, Vec<u8>, Vec<u8>);

/// OT-007: three-level two-branch identity shares a single root allocation and immutable proof.
#[tokio::test]
async fn ot007_sql_branches_keep_original_evidence_and_never_create_child_capacity() {
    let fixture = Fixture::new().await;
    fixture.root(120, "root").await;
    let repository = fixture.repository();
    for (parent, child) in [("root", "a"), ("a", "leaf"), ("root", "b")] {
        repository
            .record_session_parent(&fixture.proof(parent, child), &deadline())
            .await
            .unwrap();
    }
    let original: Vec<ParentEvidence> = sqlx::query_as(
        "SELECT child_native_id,parent_native_id,sequence,receipt,fingerprint FROM node_session_parents ORDER BY child_native_id"
    ).fetch_all(fixture.storage.pool_for_test()).await.unwrap();
    for (parent, child) in [("root", "a"), ("a", "leaf"), ("root", "b")] {
        let duplicate = session_parent_development_fixture(
            fixture.source.clone(),
            native(parent),
            native(child),
            99,
        );
        repository
            .record_session_parent(&duplicate, &deadline())
            .await
            .unwrap();
    }
    assert_eq!(
        repository
            .record_session_parent(&fixture.proof("b", "leaf"), &deadline())
            .await,
        Err(DispatchError::Conflict)
    );
    assert!(
        repository
            .record_session_parent(&fixture.proof("leaf", "root"), &deadline())
            .await
            .is_err()
    );
    let after: Vec<ParentEvidence> = sqlx::query_as(
        "SELECT child_native_id,parent_native_id,sequence,receipt,fingerprint FROM node_session_parents ORDER BY child_native_id"
    ).fetch_all(fixture.storage.pool_for_test()).await.unwrap();
    assert_eq!(original, after);
    let leaf = repository
        .resolve_session_tree(&native("leaf"), &deadline())
        .await
        .unwrap();
    assert_eq!(leaf.root, Some(native("root")));
    assert_eq!(leaf.ancestors, vec![native("a"), native("root")]);
    let members = repository
        .session_tree_members(&native("root"), &deadline())
        .await
        .unwrap();
    assert_eq!(members.len(), 4);
    assert_eq!(
        repository
            .inspect_capacity(&deadline())
            .await
            .unwrap()
            .occupied,
        1
    );
    for child in ["a", "b", "leaf"] {
        assert!(
            repository
                .capacity_owner(&native(child), &deadline())
                .await
                .unwrap()
                .is_none()
        );
    }
    fixture.close().await;
}

/// OT-008: failure before insert leaves identity absent and the original root slot occupied.
#[tokio::test]
async fn ot008_sql_relationship_write_failure_does_not_refund_or_invent_identity() {
    let fixture = Fixture::new().await;
    fixture.root(121, "root").await;
    sqlx::query("CREATE TRIGGER independent_tree_fail BEFORE INSERT ON node_session_parents BEGIN SELECT RAISE(ABORT,'independent controlled necessary failure'); END")
        .execute(fixture.storage.pool_for_test()).await.unwrap();
    let repository = fixture.repository();
    assert!(
        repository
            .record_session_parent(&fixture.proof("root", "c"), &deadline())
            .await
            .is_err()
    );
    assert_eq!(
        repository
            .inspect_capacity(&deadline())
            .await
            .unwrap()
            .occupied,
        1
    );
    assert_eq!(
        repository
            .resolve_session_tree(&native("c"), &deadline())
            .await,
        Err(DispatchError::SessionTreeUnverified)
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM node_session_parents")
        .fetch_one(fixture.storage.pool_for_test())
        .await
        .unwrap();
    assert_eq!(rows, 0);
    sqlx::query("DROP TRIGGER independent_tree_fail")
        .execute(fixture.storage.pool_for_test())
        .await
        .unwrap();
    repository
        .record_session_parent(&fixture.proof("root", "c"), &deadline())
        .await
        .unwrap();
    assert_eq!(
        repository
            .resolve_session_tree(&native("c"), &deadline())
            .await
            .unwrap()
            .root,
        Some(native("root"))
    );
    assert_eq!(
        repository
            .inspect_capacity(&deadline())
            .await
            .unwrap()
            .occupied,
        1
    );
    fixture.close().await;
}
