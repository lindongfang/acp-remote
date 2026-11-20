use super::{
    AuthProof, FakeProtector, Id128, JwsType, NodeRuntime, Nonce256, SigningKey, TestArea,
    client_public_key, free_address, ipc, node_public_key, pair_browser, policy_actor, sign,
};
use crate::{
    access_policy::{AccessPolicyError, ApplicationPolicy, AuthenticatedActor, PolicyClock},
    application::{
        queue_owner::BlockedEnvironment,
        request_registry::{NoAudit, RequestAuthority, SystemNodeClock, ValidatedBodyPort},
    },
    domain::{
        EntityId,
        request_registry::{QueueAction, RegistryError},
    },
    session_storage::request_registry::{
        authority_slot_observation::SlotOutcome,
        connection_close_test_hooks::{CloseBarrier, ClosePoint},
    },
};
use std::{
    future::{Future, poll_fn},
    path::Path,
    sync::Arc,
    task::Poll,
};

const fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}

struct TestValidator;
impl ValidatedBodyPort for TestValidator {
    type Raw = str;
    type Body = ();
    fn validate(&self, _: &str) -> Result<((), [u8; 32]), RegistryError> {
        Ok(((), [0; 32]))
    }
}

struct FixedPolicyClock;
impl PolicyClock for FixedPolicyClock {
    fn unix_seconds(&self) -> Result<u64, AccessPolicyError> {
        Ok(100)
    }
}

async fn signed_actor(
    policy: &ApplicationPolicy,
    key: &SigningKey,
    client: &str,
) -> (AuthenticatedActor, Id128, String) {
    let public_key = client_public_key(key);
    let lease = policy
        .issue_authentication_challenge(
            Some(Id128::parse_base64url(client).unwrap()),
            public_key,
            Nonce256::new([17; 32]),
        )
        .unwrap();
    let connection = lease.connection_id();
    let c = lease.challenge();
    let proof = AuthProof {
        v: 1,
        node_id: c.node_id,
        node_public_key: c.node_public_key,
        origin: c.origin.clone(),
        connection_id: connection,
        challenge_id: c.challenge_id,
        nonce: c.nonce,
        iat: c.iat,
        exp: c.exp,
        client_id: Some(Id128::parse_base64url(client).unwrap()),
        client_public_key: public_key,
        client_nonce: Nonce256::new([17; 32]),
    };
    let compact = sign(
        JwsType::AuthClient,
        &serde_json::to_vec(&proof).unwrap(),
        key,
    )
    .unwrap();
    let actor = policy.authenticate(connection, &compact).await.unwrap();
    (actor, connection, compact)
}

async fn target(runtime: &NodeRuntime, native: &str) {
    let pool = runtime.storage.as_ref().unwrap().pool_for_test();
    sqlx::query("INSERT INTO product_sources VALUES(?,'authority development fixture')")
        .bind(id(1).encode())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
        .bind(id(2).encode())
        .bind(id(1).encode())
        .bind(native)
        .execute(pool)
        .await
        .unwrap();
}

fn exclusive_database(path: &Path) {
    use std::os::windows::fs::OpenOptionsExt;
    drop(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(path)
            .unwrap(),
    );
}

#[tokio::test]
async fn authority_dev_real_proof_clone_clock_and_replay() {
    let area = TestArea::new();
    let runtime = NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        std::sync::Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    target(&runtime, "auth-proof-native").await;
    let key = SigningKey::from_slice(&[31; 32]).unwrap();
    let client = pair_browser(&runtime, &key, "authority proof development").await;
    let policy = runtime.application_policy().unwrap();
    let (actor, connection, compact) = signed_actor(&policy, &key, &client).await;
    assert!(policy.authenticate(connection, &compact).await.is_err());
    let authority = runtime.request_authority().unwrap();
    let client_entity = EntityId::decode(&client).unwrap();
    let root_key =
        crate::domain::native::NativeSessionKey::from_config(Some(id(1)), "auth-proof-native")
            .unwrap();
    authority.session_control().claim_initial(
        &root_key,
        client_entity,
        "authority proof development",
        true,
    );
    let context = authority
        .authorize(&actor.clone(), id(2), QueueAction::Submit)
        .await
        .unwrap();
    assert_eq!(context.client_id.encode(), client);
    assert!(
        policy
            .with_clock(Arc::new(FixedPolicyClock))
            .require_current_actor(&actor)
            .await
            .is_ok()
    );
    let clocked = policy.with_clock(Arc::new(FixedPolicyClock));
    let (clocked_actor, _, _) = signed_actor(&clocked, &key, &client).await;
    authority
        .authorize_read(&clocked_actor, id(2))
        .await
        .unwrap();
    runtime.shutdown(None).await.unwrap();
    exclusive_database(&area.data_dir().join("node.sqlite3"));
}

#[tokio::test]
async fn authority_dev_revocation_rejects_new_authorization() {
    let area = TestArea::new();
    let runtime = NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        std::sync::Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    let key = SigningKey::from_slice(&[32; 32]).unwrap();
    let client = pair_browser(&runtime, &key, "authority revoke development").await;
    let policy = runtime.application_policy().unwrap();
    let actor = policy_actor(&policy, &key, &client, Id128::new([19; 16])).await;
    let authority = runtime.request_authority().unwrap();
    authority.authorize_read(&actor, id(2)).await.unwrap();
    ipc::device_revoke(runtime.ipc_address(), &client)
        .await
        .unwrap();
    assert_eq!(
        authority
            .authorize(&actor, id(2), QueueAction::Cancel)
            .await
            .unwrap_err(),
        RegistryError::InvalidActor
    );
    assert_eq!(
        authority.authorize_read(&actor, id(2)).await.unwrap_err(),
        RegistryError::InvalidActor
    );
    runtime.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn authority_dev_same_database_foreign_owner_rejected_before_wrap() {
    let area = Arc::new(TestArea::new());
    let runtime = NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        std::sync::Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    let authority = runtime.request_authority().unwrap();
    let other_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
        &area.data_dir().join("node.sqlite3"),
        area.clone(),
    );
    let other = other_scope.open().await.unwrap();
    let wrapped = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = wrapped.clone();
    let rejected = other
        .start_queue_service_with_repository_and_audit_for_fixture(
            authority.clone(),
            TestValidator,
            SystemNodeClock,
            Arc::new(BlockedEnvironment),
            Arc::new(NoAudit),
            move |registry| {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(registry)
            },
        )
        .await;
    assert!(matches!(rejected, Err(RegistryError::InvalidActor)));
    assert_eq!(wrapped.load(std::sync::atomic::Ordering::SeqCst), 0);
    let (service, driver) = runtime
        .storage
        .as_ref()
        .unwrap()
        .start_queue_service_with_audit_for_fixture(
            authority,
            TestValidator,
            SystemNodeClock,
            Arc::new(BlockedEnvironment),
            Arc::new(NoAudit),
        )
        .await
        .unwrap();
    assert_eq!(
        service.startup_status(),
        crate::application::queue_owner::StartupStatus::Running
    );
    drop(driver);
    other.close().await;
    other_scope.stop_and_observe().await;
    runtime.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn authority_dev_equal_identity_two_real_nodes_and_restart() {
    let area_a = TestArea::new();
    let seed = NodeRuntime::start_control_only_for_test(
        area_a.config(free_address()),
        std::sync::Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    let key = SigningKey::from_slice(&[33; 32]).unwrap();
    let client = pair_browser(&seed, &key, "authority instance development").await;
    let old_policy = seed.application_policy().unwrap();
    let old_actor = policy_actor(&old_policy, &key, &client, Id128::new([20; 16])).await;
    let old_authority = seed.request_authority().unwrap();
    seed.shutdown(None).await.unwrap();
    exclusive_database(&area_a.data_dir().join("node.sqlite3"));
    for suffix in ["node.sqlite3-wal", "node.sqlite3-shm"] {
        let path = area_a.data_dir().join(suffix);
        assert!(!path.exists() || std::fs::metadata(path).unwrap().len() == 0);
    }
    let area_b = TestArea::new();
    std::fs::create_dir(area_b.data_dir()).unwrap();
    for file in ["node.sqlite3", "identity.key"] {
        std::fs::copy(area_a.data_dir().join(file), area_b.data_dir().join(file)).unwrap();
    }
    let a = NodeRuntime::start_control_only_for_test(
        area_a.config(free_address()),
        std::sync::Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    let b = NodeRuntime::start_control_only_for_test(
        area_b.config(free_address()),
        std::sync::Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    assert_eq!(node_public_key(&a), node_public_key(&b));
    assert_eq!(
        a.identity.as_ref().unwrap().node_id(),
        b.identity.as_ref().unwrap().node_id()
    );
    let pa = a.application_policy().unwrap();
    let pb = b.application_policy().unwrap();
    let aa = policy_actor(&pa, &key, &client, Id128::new([21; 16])).await;
    let ab = policy_actor(&pb, &key, &client, Id128::new([22; 16])).await;
    let qa = a.request_authority().unwrap();
    let qb = b.request_authority().unwrap();
    qa.authorize_read(&aa, id(2)).await.unwrap();
    qb.authorize_read(&ab, id(2)).await.unwrap();
    assert_eq!(
        qa.authorize_read(&ab, id(2)).await.unwrap_err(),
        RegistryError::InvalidActor
    );
    assert_eq!(
        qb.authorize_read(&aa, id(2)).await.unwrap_err(),
        RegistryError::InvalidActor
    );
    assert_eq!(
        qa.authorize_read(&old_actor, id(2)).await.unwrap_err(),
        RegistryError::InvalidActor
    );
    assert_eq!(
        old_authority
            .authorize_read(&old_actor, id(2))
            .await
            .unwrap_err(),
        RegistryError::NodeStopping
    );
    a.shutdown(None).await.unwrap();
    b.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn authority_dev_target_mapping_and_utf8_byte_boundary() {
    let area = TestArea::new();
    let runtime = NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        std::sync::Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    let native = format!("{}a", "界".repeat(1365));
    assert_eq!(native.len(), 4096);
    target(&runtime, &native).await;
    let authority = runtime.request_authority().unwrap();
    let resolved = authority.resolve_target(id(2)).await.unwrap();
    assert_eq!(resolved.source().0, id(1));
    assert_eq!(resolved.native_id(), native);
    assert_eq!(
        authority.resolve_target(id(3)).await.unwrap_err(),
        RegistryError::InvalidTarget
    );
    let pool = runtime.storage.as_ref().unwrap().pool_for_test();
    let natural_error = sqlx::query("UPDATE product_targets SET native_id=? WHERE target_id=?")
        .bind(format!("{native}b"))
        .bind(id(2).encode())
        .execute(pool)
        .await
        .unwrap_err();
    assert_eq!(
        natural_error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .as_deref(),
        Some("275")
    );
    assert_eq!(
        authority.resolve_target(id(2)).await.unwrap().native_id(),
        native
    );

    // Deliberate self-owned fault fixture: natural entry rejects this row above.
    let mut fault_connection = pool.acquire().await.unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=ON")
        .execute(&mut *fault_connection)
        .await
        .unwrap();
    let injected = sqlx::query("UPDATE product_targets SET native_id=? WHERE target_id=?")
        .bind(format!("{native}b"))
        .bind(id(2).encode())
        .execute(&mut *fault_connection)
        .await;
    let restored = sqlx::query("PRAGMA ignore_check_constraints=OFF")
        .execute(&mut *fault_connection)
        .await;
    restored.unwrap();
    injected.unwrap();
    let checks: i64 = sqlx::query_scalar("PRAGMA ignore_check_constraints")
        .fetch_one(&mut *fault_connection)
        .await
        .unwrap();
    assert_eq!(checks, 0);
    drop(fault_connection);
    assert_eq!(
        authority.resolve_target(id(2)).await.unwrap_err(),
        RegistryError::CorruptState
    );
    sqlx::query("UPDATE product_targets SET native_id=? WHERE target_id=?")
        .bind(&native)
        .bind(id(2).encode())
        .execute(pool)
        .await
        .unwrap();
    runtime.shutdown(None).await.unwrap();
}

// This guard owns only development barriers, never the original acquisition task.
struct AuthorityAcquireGate(Arc<CloseBarrier>);
impl Drop for AuthorityAcquireGate {
    fn drop(&mut self) {
        self.0.release();
    }
}
type AuthorityAcquisition = Arc<crate::session_storage::AcquisitionObservation>;

async fn cancel_authority_pending<F: Future>(
    caller: F,
    registry: &crate::session_storage::request_registry::SqliteRequestRegistry,
    blocker: &mut Option<sqlx::pool::PoolConnection<sqlx::Sqlite>>,
    held: &AuthorityAcquisition,
) -> Result<AuthorityAcquisition, &'static str> {
    use crate::session_storage::AcquisitionCloseProgress as CloseProgress;
    use std::time::Duration;
    use tokio::time::timeout;
    let before = registry.acquisitions_for_test();
    let predecessor_leases = registry.observe_authority_slots_for_test().leases;
    let acquired = CloseBarrier::new();
    let closing = CloseBarrier::new();
    let _release = (
        AuthorityAcquireGate(acquired.clone()),
        AuthorityAcquireGate(closing.clone()),
    );
    let hook = registry.connection_closer_for_test();
    hook.pause_next(ClosePoint::AfterAcquire, acquired.clone())
        .map_err(|_| "original acquire gate registration")?;
    let mut caller = Box::pin(caller);
    if !poll_fn(|cx| Poll::Ready(caller.as_mut().poll(cx).is_pending())).await {
        return Err("business future unexpectedly completed");
    }
    let mut added = registry
        .acquisitions_for_test()
        .into_iter()
        .filter(|candidate| !before.iter().any(|old| Arc::ptr_eq(candidate, old)));
    let observed = added.next().ok_or("original acquisition missing")?;
    if added.next().is_some() {
        return Err("unexpected second business acquisition");
    }
    timeout(Duration::from_secs(2), observed.wait_pool_first_pending())
        .await
        .map_err(|_| "original SQLx acquire never returned Pending")?;
    let pending = observed.snapshot();
    if pending.transferred
        || pending.acquisition_joined
        || registry.observe_authority_slots_for_test().leases <= predecessor_leases
    {
        return Err("blocker did not preserve the additional original acquisition responsibility");
    }
    drop(caller);
    let cancelled = observed.snapshot();
    if cancelled.transferred
        || cancelled.acquisition_joined
        || cancelled.close != CloseProgress::NotStarted
    {
        return Err("caller cancellation bypassed the blocker or invented completion");
    }
    if !registry
        .acquisitions_for_test()
        .iter()
        .any(|slot| Arc::ptr_eq(slot, &observed))
    {
        return Err("caller cancellation lost the original owner task");
    }
    drop(blocker.take());
    timeout(Duration::from_secs(2), acquired.arrived())
        .await
        .map_err(|_| "original acquire did not continue after blocker release")?;
    hook.pause_next(ClosePoint::BeforeClose, closing.clone())
        .map_err(|_| "original close gate registration")?;
    acquired.release();
    timeout(Duration::from_secs(2), closing.arrived())
        .await
        .map_err(|_| "cancelled original did not enter explicit close")?;
    let closing_original = observed.snapshot();
    if !closing_original.transferred
        || observed.same_physical(held)
        || closing_original.close != CloseProgress::Pending
        || closing_original.acquisition_joined
    {
        return Err("original physical close chain or pre-ACK responsibility changed");
    }
    closing.release();
    if !timeout(Duration::from_secs(2), observed.join_original_close())
        .await
        .map_err(|_| "original close join deadline")?
    {
        return Err("original PhysicalCloseLink has no original CloseSlot");
    }
    if observed.snapshot().close != CloseProgress::ConfirmedJoined {
        return Err("original physical ACK and close join were not confirmed");
    }
    Ok(observed)
}

async fn authority_pool_pending_case(resolve: bool) {
    use crate::session_storage::AcquisitionCloseProgress as CloseProgress;
    use std::time::Duration;
    use tokio::time::timeout;
    let area = TestArea::new();
    let runtime = NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        std::sync::Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    let key = SigningKey::from_slice(&[34; 32]).unwrap();
    let client = pair_browser(&runtime, &key, "authority acquire development").await;
    let actor = policy_actor(
        &runtime.application_policy().unwrap(),
        &key,
        &client,
        Id128::new([23; 16]),
    )
    .await;
    target(&runtime, "authority-acquire").await;
    let authority = runtime.request_authority().unwrap();
    let storage = runtime.storage.as_ref().unwrap();
    let registry = storage.request_registry();
    let scope = registry.owner_scope();
    authority.verify_owner_scope(&scope).unwrap();
    let idle_original = registry.acquisitions_for_test().last().cloned();
    let held = registry.trust_owned_connection().await.unwrap();
    let held_original = registry.acquisitions_for_test().last().unwrap().clone();
    let held_was_idle = idle_original
        .as_ref()
        .is_some_and(|idle| idle.same_physical(&held_original));
    let held_before = held_original.snapshot();
    let mut blocker = Some(storage.pool_for_test().acquire().await.unwrap());
    let cancelled = if resolve {
        cancel_authority_pending(
            authority.resolve_target(id(2)),
            &registry,
            &mut blocker,
            &held_original,
        )
        .await
    } else {
        cancel_authority_pending(
            authority.authorize_read(&actor, id(2)),
            &registry,
            &mut blocker,
            &held_original,
        )
        .await
    };
    // Both error and success paths release all owned resources before asserting outcomes.
    drop(blocker.take());
    drop(held);
    let held_close = timeout(Duration::from_secs(2), held_original.join_original_close()).await;
    let successful = if cancelled.is_ok() {
        let read = authority.authorize_read(&actor, id(2)).await;
        let target = authority.resolve_target(id(2)).await;
        Some((read, target, authority.verify_owner_scope(&scope)))
    } else {
        None
    };
    let shutdown = runtime.shutdown(None).await;
    exclusive_database(&area.data_dir().join("node.sqlite3"));
    assert!(
        held_was_idle,
        "the original idle physical connection was really held"
    );
    assert!(held_before.transferred);
    assert_eq!(held_before.close, CloseProgress::NotStarted);
    assert!(held_close.unwrap());
    assert_eq!(
        held_original.snapshot().close,
        CloseProgress::ConfirmedJoined
    );
    assert!(held_original.snapshot().acquisition_joined);
    let original = cancelled.expect("real pool cancellation and original physical close");
    assert!(original.snapshot().acquisition_joined);
    assert_eq!(original.snapshot().close, CloseProgress::ConfirmedJoined);
    let (read, target, same_scope) = successful.unwrap();
    read.unwrap();
    target.unwrap();
    same_scope.unwrap();
    shutdown.unwrap();
    // Each branch proves real pool Pending and original closure, not blocked SQL execution.
}

#[tokio::test]
async fn authority_dev_actual_pool_acquire_pending_is_cancellable() {
    for resolve in [false, true] {
        authority_pool_pending_case(resolve).await;
    }
}

#[tokio::test]
async fn authority_dev_query_uses_its_slot_result_not_global_failure() {
    let area = TestArea::new();
    let runtime = NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        std::sync::Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    target(&runtime, "authority-slot").await;
    let authority = runtime.request_authority().unwrap();
    let registry = runtime.storage.as_ref().unwrap().request_registry();
    let hook = registry.connection_closer_for_test();
    let barrier = CloseBarrier::new();
    hook.pause_next(ClosePoint::AfterCloseAck, barrier.clone())
        .unwrap();
    let first_authority = authority.clone();
    let first = tokio::spawn(async move { first_authority.resolve_target(id(2)).await });
    barrier.arrived().await;
    let observed = registry.observe_authority_slots_for_test();
    assert!(!observed.sealed);
    assert_eq!(observed.slots.len(), 1);
    let first_slot = &observed.slots[0];
    assert_eq!(first_slot.stored(), SlotOutcome::Pending);
    hook.fail_next_result_after_ack().unwrap();
    barrier.release();
    assert_eq!(
        first.await.unwrap().unwrap_err(),
        RegistryError::StorageUnavailable
    );
    assert_eq!(first_slot.wait().await, SlotOutcome::Unconfirmed);
    authority.resolve_target(id(2)).await.unwrap();
    let later = registry.observe_authority_slots_for_test();
    assert!(later.slots.iter().all(|slot| !slot.same_slot(first_slot)));
    assert!(
        later
            .slots
            .iter()
            .any(|slot| slot.stored() == SlotOutcome::Confirmed)
    );
    assert_eq!(first_slot.stored(), SlotOutcome::Unconfirmed);
    assert!(
        runtime.shutdown(None).await.is_err(),
        "global uncertainty remains, despite the later query success"
    );
    exclusive_database(&area.data_dir().join("node.sqlite3"));
}
