use super::*;
use sha2::{Digest, Sha256};

#[tokio::test]
async fn production_factory_default_ready_owns_same_provider_socket_and_body() {
    let area = TestArea::new();
    let node = NodeRuntime::start(
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
    )
    .await
    .expect("ordinary factory ready");
    let services = node
        .session_services
        .as_ref()
        .expect("production service set");
    let provider = services.catalog.configuration();
    assert!(Arc::ptr_eq(&provider, &services.configuration));
    assert!(Arc::ptr_eq(
        &services.socket_authority,
        &node.management.as_ref().unwrap().socket_authority()
    ));
    let (claimed, supervisor) = node
        .storage
        .as_ref()
        .unwrap()
        .original_runtime_registration_for_test();
    assert!(claimed);
    assert!(supervisor.is_some());
    let upload = b" \n[{\"type\":\"text\",\"text\":\"factory body\",\"extra\":true},{\"type\":\"future-block\",\"x\":7}] \t".to_vec();
    let mut receipt = services
        .body
        .begin(upload.clone())
        .expect("original body owner accepts");
    let prepared = receipt
        .wait()
        .await
        .expect("real parser/hash worker joined");
    let lease = prepared.acquire_use().expect("same custody lease");
    assert_eq!(lease.validated().body().uploaded_bytes(), upload.as_slice());
    assert_eq!(
        *lease.validated().digest(),
        <[u8; 32]>::from(Sha256::digest(&upload))
    );
    drop(lease);
    drop(prepared);
    drop(receipt);
    drop(provider);
    node.shutdown(None)
        .await
        .expect("ordinary factory original shutdown");
}

#[tokio::test]
async fn production_factory_control_only_fixture_is_explicit_and_unclaimed() {
    let area = TestArea::new();
    let node = NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
    )
    .await
    .expect("explicit component fixture ready");
    assert!(node.session_services.is_none());
    let (claimed, supervisor) = node
        .storage
        .as_ref()
        .unwrap()
        .original_runtime_registration_for_test();
    assert!(!claimed);
    assert!(supervisor.is_none());
    node.shutdown(None)
        .await
        .expect("control-only original shutdown");
}

#[tokio::test]
async fn production_factory_stop_seals_original_body_and_keeps_receipt() {
    let area = TestArea::new();
    let node = NodeRuntime::start(
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
    )
    .await
    .expect("ordinary factory ready");
    let body = node.session_services.as_ref().unwrap().body.clone();
    let storage = node.storage.as_ref().unwrap().clone();
    let registry = storage.request_registry();
    let original = storage
        .prompt_preparation_receipt()
        .expect("receipt installed before ready");
    let mut receipt = body
        .begin(b"[{\"type\":\"text\",\"text\":\"seal\"}]".to_vec())
        .unwrap();
    let prepared = receipt.wait().await.unwrap();
    drop(prepared);
    drop(receipt);
    node.shutdown(None)
        .await
        .expect("all original workers close");
    assert!(body.begin(b"[]".to_vec()).is_err());
    assert!(original.observe().all_joined());
    assert!(registry.close_status().is_complete());
    assert_eq!(storage.pool_for_test().size(), 0);
}

#[tokio::test]
async fn production_factory_original_install_and_ready_fences_are_observable() {
    use crate::node::session_services::{FactoryStage, FactoryTestHooks};
    let area = TestArea::new();
    let before_install = Arc::new(crate::test_support::AsyncBarrier::default());
    let before_ready = Arc::new(crate::test_support::AsyncBarrier::default());
    let mut hooks = FactoryTestHooks::default();
    hooks.before_view_install = Some(before_install.clone());
    hooks.before_ready = Some(before_ready.clone());
    let owner = NodeProcessOwner::begin_for_factory_test(
        &tokio::runtime::Handle::current(),
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
        hooks,
    );
    before_install.wait_until_entered().await;
    let installed = owner.factory_observation_for_test();
    before_install.release();
    before_ready.wait_until_entered().await;
    let ready = owner.factory_observation_for_test();
    before_ready.release();
    let shutdown = match owner.ready().await {
        Ok(node) => node.shutdown(None).await,
        Err(error) => Err(error),
    };
    let joined = owner.join_actual().await;
    assert!(shutdown.is_ok(), "same original shutdown: {shutdown:?}");
    assert!(joined.is_ok(), "same original closure: {joined:?}");
    assert_eq!(installed.stage, FactoryStage::BeforeViewInstall);
    assert!(installed.claim && installed.supervisor.is_some());
    assert!(!installed.services_installed && !installed.ready);
    assert_eq!(ready.operation, installed.operation);
    assert_eq!(ready.supervisor, installed.supervisor);
    assert_eq!(ready.stage, FactoryStage::BeforeReady);
    assert!(ready.services_installed && ready.claim && !ready.ready);
}
