use super::*;
use sqlx::Connection;
async fn record(
    state: &str,
    cipher: Option<&[u8]>,
    kind: Option<&str>,
    revision: Option<Vec<u8>>,
) -> Result<Command, CatalogError> {
    let mut sql = SqliteConnection::connect("sqlite::memory:").await.unwrap();
    let id = EntityId::from_bytes([5; 16]).encode();
    let digest = Sha256::digest(b"cipher").to_vec();
    let settled = state != "registered";
    let success = kind == Some("profile");
    let row=sqlx::query("SELECT ? AS request_id,? AS actor_id,0 AS register_auth_generation,'profile.publish' AS action,? AS state,13 AS registered_at_ms,? AS intent_cipher,? AS intent_digest,? AS admission_auth_generation,? AS result_kind,? AS result_id,? AS profile_id,? AS profile_revision,? AS result_at_ms").bind(&id).bind(&id).bind(state).bind(cipher).bind(digest).bind(settled.then_some(0_i64)).bind(kind).bind(success.then_some(&id)).bind(success.then_some(&id)).bind(revision).bind(settled.then_some(17_i64)).fetch_one(&mut sql).await.unwrap();
    Command::from_row(&row)
}
#[tokio::test]
async fn node_configuration_command_record_preserves_u64_and_rejects_partial_or_bad_rows() {
    let c = record("registered", Some(b"cipher"), None, None)
        .await
        .unwrap();
    assert_eq!(c.state, CatalogState::Registered);
    assert!(c.profile.is_none());
    let c = record(
        "committed",
        None,
        Some("profile"),
        Some(u64::MAX.to_be_bytes().to_vec()),
    )
    .await
    .unwrap();
    assert_eq!(c.profile.unwrap().revision(), u64::MAX);
    assert!(c.cipher.is_none());
    assert!(
        record("rejected", None, Some("rejected"), None)
            .await
            .is_ok()
    );
    assert!(matches!(
        record("registered", Some(b"other"), None, None).await,
        Err(CatalogError::CorruptState)
    ));
    assert!(matches!(
        record("registered", None, None, None).await,
        Err(CatalogError::CorruptState)
    ));
    assert!(matches!(
        record(
            "committed",
            Some(b"cipher"),
            Some("profile"),
            Some(1_u64.to_be_bytes().to_vec())
        )
        .await,
        Err(CatalogError::CorruptState)
    ));
    assert!(matches!(
        record(
            "committed",
            None,
            Some("profile"),
            Some(0_u64.to_be_bytes().to_vec())
        )
        .await,
        Err(CatalogError::CorruptState)
    ));
    assert!(matches!(
        record("committed", None, Some("profile"), Some(vec![1; 7])).await,
        Err(CatalogError::CorruptState)
    ));
    assert!(matches!(
        record("other", None, None, None).await,
        Err(CatalogError::CorruptState)
    ));
}

#[tokio::test]
async fn node_configuration_settled_rejected_rechecks_original_registration() {
    let before = record("registered", Some(b"cipher"), None, None)
        .await
        .unwrap();
    let mut settled = record("rejected", None, Some("rejected"), None)
        .await
        .unwrap();
    let evidence = Evidence {
        before,
        identity: Identity {
            node: "node".into(),
            key: vec![1],
        },
        apply: Some((
            Keys {
                harness: EntityId::from_bytes([1; 16]),
                source: EntityId::from_bytes([2; 16]),
                profile: ProfileKey::try_new(EntityId::from_bytes([3; 16]), 1).unwrap(),
                binding: None,
            },
            42,
            999,
        )),
    };
    let mut sql = SqliteConnection::connect("sqlite::memory:").await.unwrap();
    assert!(evidence.settled_fact(&mut sql, &settled).await.is_ok());
    for field in 0..6 {
        settled = record("rejected", None, Some("rejected"), None)
            .await
            .unwrap();
        match field {
            0 => settled.request = CatalogRequestId::new(EntityId::from_bytes([9; 16])),
            1 => settled.actor = EntityId::from_bytes([9; 16]),
            2 => settled.generation += 1,
            3 => settled.action = CatalogAction::InstallHarness,
            4 => settled.at += 1,
            _ => settled.digest[0] ^= 1,
        }
        assert!(
            matches!(
                evidence.settled_fact(&mut sql, &settled).await,
                Err(CatalogError::CorruptState)
            ),
            "field {field}"
        );
    }
}

fn committed_fact(result: CatalogResult) -> CatalogFact {
    let action = match result {
        CatalogResult::Harness { .. } => CatalogAction::InstallHarness,
        CatalogResult::Profile { .. } => CatalogAction::PublishProfile,
        CatalogResult::Target { .. } => CatalogAction::BindTargetInitial,
    };
    CatalogFact::from_record(
        CatalogRequestId::new(EntityId::from_bytes([9; 16])),
        EntityId::from_bytes([9; 16]),
        action,
        CatalogState::Committed,
        (13, 0),
        Some((42, 17)),
        Some(result),
    )
}

#[test]
fn node_configuration_settled_effect_requires_every_expected_result_key() {
    let harness = EntityId::from_bytes([1; 16]);
    let source = EntityId::from_bytes([2; 16]);
    let profile = ProfileKey::try_new(EntityId::from_bytes([3; 16]), 1).unwrap();
    let target = EntityId::from_bytes([4; 16]);
    let workspace = EntityId::from_bytes([5; 16]);
    let other = EntityId::from_bytes([9; 16]);

    let keys = Keys {
        harness,
        source,
        profile,
        binding: None,
    };
    assert!(keys.matches_fact(&committed_fact(CatalogResult::Harness {
        harness,
        source,
        profile
    })));
    assert!(keys.matches_fact(&committed_fact(CatalogResult::Profile {
        harness,
        source,
        profile
    })));
    for bad in [
        CatalogResult::Profile {
            harness: other,
            source,
            profile,
        },
        CatalogResult::Profile {
            harness,
            source: other,
            profile,
        },
        CatalogResult::Profile {
            harness,
            source,
            profile: ProfileKey::try_new(other, 1).unwrap(),
        },
        CatalogResult::Profile {
            harness,
            source,
            profile: ProfileKey::try_new(profile.id(), 2).unwrap(),
        },
        CatalogResult::Target {
            target,
            workspace,
            harness,
            source,
            profile,
        },
    ] {
        assert!(!keys.matches_fact(&committed_fact(bad)));
    }
}

#[test]
fn node_configuration_settled_effect_requires_every_bound_target_key() {
    let harness = EntityId::from_bytes([1; 16]);
    let source = EntityId::from_bytes([2; 16]);
    let profile = ProfileKey::try_new(EntityId::from_bytes([3; 16]), 1).unwrap();
    let target = EntityId::from_bytes([4; 16]);
    let workspace = EntityId::from_bytes([5; 16]);
    let other = EntityId::from_bytes([9; 16]);

    let keys = Keys {
        harness,
        source,
        profile,
        binding: Some((target, workspace, "launch".into(), "canonical".into())),
    };
    assert!(keys.matches_fact(&committed_fact(CatalogResult::Target {
        target,
        workspace,
        harness,
        source,
        profile
    })));
    for bad in [
        CatalogResult::Target {
            target: other,
            workspace,
            harness,
            source,
            profile,
        },
        CatalogResult::Target {
            target,
            workspace: other,
            harness,
            source,
            profile,
        },
        CatalogResult::Target {
            target,
            workspace,
            harness: other,
            source,
            profile,
        },
        CatalogResult::Target {
            target,
            workspace,
            harness,
            source: other,
            profile,
        },
        CatalogResult::Target {
            target,
            workspace,
            harness,
            source,
            profile: ProfileKey::try_new(other, 1).unwrap(),
        },
        CatalogResult::Profile {
            harness,
            source,
            profile,
        },
    ] {
        assert!(!keys.matches_fact(&committed_fact(bad)));
    }
}
