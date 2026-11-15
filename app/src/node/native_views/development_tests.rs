// One real Node issuer and registry owns this local reader and its original ACP child.
use super::{
    Id128, NodeRuntime, SigningKey, TestArea, free_address, ipc, pair_browser, policy_actor,
};
use crate::{
    adapters::acp::native_dispatch::AcpNativeAdapter,
    application::{
        native_dispatch::{
            NativeRuntimeStatus,
            dev_tests::{Configuration, Retain, Validator, options},
        },
        request_registry::SystemNodeClock,
        shared_views::{
            budget::{MonotonicReadOptionsProvider, ReadReason, ReadResponseBudget},
            reader::PrepareNext,
        },
    },
    domain::{EntityId, shared_view::ViewError},
    platform::{DpapiProtector, native_source::WindowsSourceLauncher},
};
use std::{num::NonZeroUsize, sync::Arc, time::Duration};
use tokio::time::timeout;

const fn id(value: u8) -> EntityId {
    EntityId::from_bytes([value; 16])
}

async fn configure_fixture(
    runtime: &NodeRuntime,
    area: &TestArea,
) -> Arc<
    crate::session_storage::native_configuration::SqliteNativeConfigurationProvider<Configuration>,
> {
    let storage = runtime.storage.as_ref().unwrap();
    let pool = storage.pool_for_test();
    let child = crate::test_support::native_dispatch_dev_child(&["NATIVE_DISPATCH_DEV_CHILD"]);
    assert!(child.is_file());
    sqlx::query("INSERT INTO product_sources VALUES(?,'native Node developer')")
        .bind(id(1).encode())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces VALUES(?,?)")
        .bind(id(2).encode())
        .bind(area.directory.to_str().unwrap())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO harnesses VALUES(?,'native Node developer',?,?,NULL)")
        .bind(id(3).encode())
        .bind(child.to_str().unwrap())
        .bind(r#"["read-first-held"]"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_harness_sources VALUES(?,?)")
        .bind(id(3).encode())
        .bind(id(1).encode())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,?)")
        .bind(id(4).encode())
        .bind(id(1).encode())
        .bind("synthetic-native-A")
        .bind(id(2).encode())
        .execute(pool)
        .await
        .unwrap();
    Arc::new(storage.native_configuration_provider(Arc::new(Configuration)))
}

#[tokio::test]
async fn node_native_view_real_pairing_private_actor_revocation_and_original_close() {
    let area = TestArea::new();
    let runtime = NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        std::sync::Arc::new(DpapiProtector),
    )
    .await
    .unwrap();
    let key = SigningKey::from_slice(&[21; 32]).unwrap();
    let client_id = pair_browser(&runtime, &key, "Native local view developer").await;
    let policy = runtime.application_policy().unwrap();
    let actor = policy_actor(&policy, &key, &client_id, Id128::new([22; 16])).await;
    let configuration = configure_fixture(&runtime, &area).await;
    let started = runtime
        .start_native_views(
            Validator,
            SystemNodeClock,
            Arc::new(AcpNativeAdapter::default()),
            Arc::new(WindowsSourceLauncher),
            configuration,
            Arc::new(Retain),
            options(),
            Arc::new(MonotonicReadOptionsProvider::new(
                Duration::from_secs(5),
                ReadResponseBudget::Unbounded,
            )),
            NonZeroUsize::new(64).unwrap(),
        )
        .await
        .unwrap();
    timeout(Duration::from_secs(5), async {
        while started.native.status() == NativeRuntimeStatus::Starting {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(started.native.status(), NativeRuntimeStatus::Running);
    let client = started.views.accept_actor(actor).await.unwrap();
    let read = timeout(
        Duration::from_secs(10),
        started.views.open(&client, id(4), ReadReason::FirstOpen),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(read.descriptor().source_stamp().source().0, id(1));
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM node_native_dispatches),(SELECT count(*) FROM node_native_resource_bindings)").fetch_one(runtime.storage.as_ref().unwrap().pool_for_test()).await.unwrap();
    assert_eq!(counts, (0, 1), "opening a view must not create a dispatch");
    let next = read.next_snapshot(&client).await;
    let observed = match &next {
        Ok(PrepareNext::AwaitCredit) => "await-credit",
        Ok(PrepareNext::AwaitData) => "await-data",
        Ok(PrepareNext::SnapshotFinished) => "snapshot-finished",
        Ok(PrepareNext::Closed) => "closed",
        Ok(PrepareNext::Chunk(_)) => "chunk",
        Err(error) => panic!("first view read after open failed: {error:?}"),
    };
    assert!(
        matches!(next, Ok(PrepareNext::AwaitCredit)),
        "unexpected first view read after open: {observed}"
    );
    std::fs::write(area.directory.join("release-read-first-held"), b"release").unwrap();
    let revoke = ipc::device_revoke(runtime.ipc_address(), &client_id).await;
    let (post_read, post_open) = if revoke.is_ok() {
        (
            matches!(read.next_snapshot(&client).await, Err(ViewError::Revoked)),
            matches!(
                started.views.open(&client, id(4), ReadReason::Reopen).await,
                Err(ViewError::Revoked)
            ),
        )
    } else {
        eprintln!(
            "original IPC revoke failure: {:?}",
            runtime.control.first_revoke_error_for_development()
        );
        (false, false)
    };
    let close = read.release();
    assert_eq!(close.last_committed_packet(), 0);
    runtime.shutdown(None).await.unwrap();
    let report = started.native.last_shutdown_report().unwrap();
    assert!(report.joined && report.metadata_confirmed);
    drop(started);
    drop(client);
    drop(policy);
    std::fs::remove_dir_all(&area.directory).unwrap();
    assert!(
        !area.directory.exists(),
        "first physical cleanup of the owned Node area"
    );
    assert!(revoke.is_ok(), "the original IPC revocation must succeed");
    assert!(
        post_read,
        "the original reader must reject the revoked client"
    );
    assert!(
        post_open,
        "the original admission must reject revoked Reopen"
    );
}
