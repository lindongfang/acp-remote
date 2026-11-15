use super::*;
use crate::{
    application::shared_views::socket_authority::SocketViewClient,
    management_ws::input_upload::{InputData, UploadError},
    node::input_upload::{UploadOwner, UploadPhase, UploadPurpose},
};
use futures_util::FutureExt;
use sha2::{Digest, Sha256};
use std::{future::Future, panic::AssertUnwindSafe, pin::Pin};

const fn target(n: u8) -> crate::domain::EntityId {
    crate::domain::EntityId::from_bytes([n; 16])
}
fn frame(id: Id128, offset: u64, bytes: &[u8]) -> Vec<u8> {
    let mut wire = vec![0; 32];
    wire[..4].copy_from_slice(b"ACPI");
    wire[4] = 1;
    wire[5] = 1;
    wire[8..24].copy_from_slice(id.as_bytes());
    wire[24..32].copy_from_slice(&offset.to_be_bytes());
    wire.extend_from_slice(bytes);
    wire
}
struct Case {
    area: TestArea,
    owner: Option<NodeProcessOwner>,
    node: Option<NodeRuntime>,
    peers: Vec<ManagementSocket>,
    clients: Vec<SocketViewClient>,
    client_id: String,
    registry: Option<crate::session_storage::request_registry::SqliteRequestRegistry>,
    pool: Option<sqlx::SqlitePool>,
}
impl Case {
    async fn start(&mut self) {
        self.owner = Some(NodeProcessOwner::begin(
            &tokio::runtime::Handle::current(),
            self.area.config(free_address()),
            Arc::new(FakeProtector::default()),
        ));
        self.node = Some(self.owner.as_ref().unwrap().ready().await.unwrap());
        let node = self.node.as_ref().unwrap();
        let storage = node.storage.as_ref().unwrap();
        self.registry = Some(storage.request_registry());
        self.pool = Some(storage.pool_for_test().clone());
        sqlx::query("INSERT INTO product_sources VALUES(?,'upload author fixture')")
            .bind(target(1).encode())
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        for n in [2, 3] {
            sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
                .bind(target(n).encode())
                .bind(target(1).encode())
                .bind(format!("upload-native-{n}"))
                .execute(storage.pool_for_test())
                .await
                .unwrap();
        }
        let key = SigningKey::from_slice(&[69; 32]).unwrap();
        self.client_id = pair_browser(node, &key, "Upload author fixture").await;
        for _ in 0..2 {
            let observed = node
                .management
                .as_ref()
                .unwrap()
                .observe_next_socket_client_for_upload_test()
                .unwrap();
            let peer = valid_management_socket(&self.area, node.listen_addr()).await;
            let peer =
                authenticate_socket(peer, &key, &self.client_id, node_public_key(node)).await;
            self.peers.push(peer);
            self.clients.push(observed.await.unwrap());
        }
    }
    fn uploads(&self, socket: usize) -> UploadOwner {
        self.node
            .as_ref()
            .unwrap()
            .input_upload_owner(&self.clients[socket])
            .unwrap()
    }
    async fn close_peers(&mut self) {
        for mut peer in self.peers.drain(..) {
            let _ = peer.close(None).await;
        }
    }
    async fn close(&mut self) -> (bool, bool, bool, bool) {
        self.close_peers().await;
        let shutdown = if let Some(node) = self.node.take() {
            node.shutdown(None).await.is_ok()
        } else {
            true
        };
        let joined = if let Some(owner) = &self.owner {
            owner.join_actual().await.is_ok()
        } else {
            true
        };
        let registry = self
            .registry
            .as_ref()
            .is_none_or(|r| r.close_status().is_complete());
        let pool = self.pool.as_ref().is_none_or(|p| p.size() == 0);
        (shutdown, joined, registry, pool)
    }
}
async fn case(run: impl for<'a> FnOnce(&'a mut Case) -> Pin<Box<dyn Future<Output = ()> + 'a>>) {
    let mut fixture = Case {
        area: TestArea::new(),
        owner: None,
        node: None,
        peers: Vec::new(),
        clients: Vec::new(),
        client_id: String::new(),
        registry: None,
        pool: None,
    };
    let result = AssertUnwindSafe(async {
        fixture.start().await;
        run(&mut fixture).await;
    })
    .catch_unwind()
    .await;
    let closed = fixture.close().await;
    let directory_free = crate::data_dir::DataDirectory::acquire(&fixture.area.data_dir()).is_ok();
    assert_eq!(
        closed,
        (true, true, true, true),
        "original closure receipts"
    );
    assert!(
        directory_free,
        "original directory lock released after join"
    );
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}
async fn ready(
    owner: &UploadOwner,
    client: &SocketViewClient,
    upload: Id128,
) -> crate::node::input_upload::UploadStatus {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status = owner.status(client, upload).await.unwrap();
            if matches!(status.phase(), UploadPhase::Ready | UploadPhase::Failed) {
                return status;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn upload_u1_exact_previous_repeat_and_conflict_preserve_original_body() {
    case(|fixture| Box::pin(async move {
        let owner = fixture.uploads(0); let client = &fixture.clients[0];
        let upload = owner.begin(client, target(2), UploadPurpose::Submit).await.unwrap().upload();
        let bytes = b" \n[{\"type\":\"future-block\",\"opaque\":{\"x\":1,\"y\":2,\"n\":9007199254740993}}] \t";
        let a = frame(upload, 0, &bytes[..17]);
        let ack = owner.accept_data(client, InputData::decode_client(&a).unwrap()).await.unwrap();
        assert_eq!(ack.next_offset(), 17); assert_eq!(ack.encode().len(), 32);
        assert_eq!(owner.accept_data(client, InputData::decode_client(&a).unwrap()).await.unwrap(), ack);
        let mut conflict = a.clone(); conflict[32] ^= 1;
        assert_eq!(owner.accept_data(client, InputData::decode_client(&conflict).unwrap()).await, Err(UploadError::Conflict));
        let future = frame(upload, 18, b"x");
        assert_eq!(owner.accept_data(client, InputData::decode_client(&future).unwrap()).await, Err(UploadError::Conflict));
        let b = frame(upload, 17, &bytes[17..]);
        owner.accept_data(client, InputData::decode_client(&b).unwrap()).await.unwrap();
        assert!(owner.finish(client, upload, bytes.len() as u64 + 1).await.is_err());
        assert_eq!(owner.finish(client, upload, bytes.len() as u64).await.unwrap().upload(), upload);
        assert!(owner.finish(client, upload, bytes.len() as u64).await.is_err());
        let status = ready(&owner, client, upload).await;
        assert_eq!(status.phase(), UploadPhase::Ready);
        let lease = owner.lease_body(client, status.body().unwrap(), target(2), UploadPurpose::Submit).await.unwrap();
        assert_eq!(lease.validated().body().uploaded_bytes(), bytes);
        assert_eq!(*lease.validated().digest(), <[u8;32]>::from(Sha256::digest(bytes)));
        drop(lease);
    })).await;
}

#[tokio::test]
async fn upload_u1_socket_target_purpose_and_current_actor_are_real() {
    case(|fixture| {
        Box::pin(async move {
            let first = fixture.uploads(0);
            let second = fixture.uploads(1);
            let upload = first
                .begin(&fixture.clients[0], target(2), UploadPurpose::Edit)
                .await
                .unwrap()
                .upload();
            assert_eq!(
                first.status(&fixture.clients[1], upload).await,
                Err(UploadError::InvalidOwner)
            );
            assert_eq!(
                second.status(&fixture.clients[1], upload).await,
                Err(UploadError::BodyUnavailable)
            );
            let wire = frame(upload, 0, b"[]");
            first
                .accept_data(
                    &fixture.clients[0],
                    InputData::decode_client(&wire).unwrap(),
                )
                .await
                .unwrap();
            first.finish(&fixture.clients[0], upload, 2).await.unwrap();
            let status = ready(&first, &fixture.clients[0], upload).await;
            assert!(
                first
                    .lease_body(
                        &fixture.clients[0],
                        status.body().unwrap(),
                        target(2),
                        UploadPurpose::Submit
                    )
                    .await
                    .is_err()
            );
            assert!(
                first
                    .lease_body(
                        &fixture.clients[0],
                        status.body().unwrap(),
                        target(3),
                        UploadPurpose::Edit
                    )
                    .await
                    .is_err()
            );
            let lease = first
                .lease_body(
                    &fixture.clients[0],
                    status.body().unwrap(),
                    target(2),
                    UploadPurpose::Edit,
                )
                .await
                .unwrap();
            drop(lease);
            ipc::device_revoke(
                fixture.node.as_ref().unwrap().ipc_address(),
                &fixture.client_id,
            )
            .await
            .unwrap();
            assert!(first.status(&fixture.clients[0], upload).await.is_err());
            assert!(
                second
                    .begin(&fixture.clients[1], target(2), UploadPurpose::Submit)
                    .await
                    .is_err()
            );
        })
    })
    .await;
}

#[tokio::test]
async fn upload_u1_malformed_body_fails_after_ack_and_release_is_owned() {
    case(|fixture| {
        Box::pin(async move {
            let owner = fixture.uploads(0);
            let client = &fixture.clients[0];
            let upload = owner
                .begin(client, target(2), UploadPurpose::Submit)
                .await
                .unwrap()
                .upload();
            let wire = frame(upload, 0, b"not-json-owner-canary");
            let ack = owner
                .accept_data(client, InputData::decode_client(&wire).unwrap())
                .await
                .unwrap();
            owner
                .finish(client, upload, ack.next_offset())
                .await
                .unwrap();
            let status = ready(&owner, client, upload).await;
            assert_eq!(status.phase(), UploadPhase::Failed);
            assert_eq!(status.error(), Some(UploadError::InvalidBody));
            assert!(!format!("{status:?}").contains("owner-canary"));
            owner.release(client, upload).await.unwrap();
            owner.release(client, upload).await.unwrap();
            assert_eq!(
                owner.status(client, upload).await,
                Err(UploadError::BodyUnavailable)
            );
            let later = owner
                .begin(client, target(2), UploadPurpose::Submit)
                .await
                .unwrap()
                .upload();
            owner.release(client, later).await.unwrap();
        })
    })
    .await;
}

#[tokio::test]
async fn upload_u1_4097_chunks_cross_control_size_without_lifetime_quota() {
    case(|fixture| {
        Box::pin(async move {
            let owner = fixture.uploads(0);
            let client = &fixture.clients[0];
            let upload = owner
                .begin(client, target(2), UploadPurpose::Submit)
                .await
                .unwrap()
                .upload();
            let bytes = format!(
                "[{{\"type\":\"text\",\"text\":\"{}\"}}]",
                "x".repeat(65_552)
            )
            .into_bytes();
            let mut offset = 0u64;
            let mut chunks = 0;
            for payload in bytes.chunks(16) {
                let wire = frame(upload, offset, payload);
                offset += payload.len() as u64;
                chunks += 1;
                assert_eq!(
                    owner
                        .accept_data(client, InputData::decode_client(&wire).unwrap())
                        .await
                        .unwrap()
                        .next_offset(),
                    offset
                );
            }
            assert!(chunks > 4097);
            assert!(bytes.len() > 32 * 1024);
            owner.finish(client, upload, offset).await.unwrap();
            let status = ready(&owner, client, upload).await;
            let lease = owner
                .lease_body(
                    client,
                    status.body().unwrap(),
                    target(2),
                    UploadPurpose::Submit,
                )
                .await
                .unwrap();
            assert_eq!(lease.validated().body().uploaded_bytes(), bytes.as_slice());
            drop(lease);
        })
    })
    .await;
}

#[tokio::test]
async fn upload_u1_socket_close_seals_only_that_original_consumer() {
    case(|fixture| {
        Box::pin(async move {
            let first = fixture.uploads(0);
            let second = fixture.uploads(1);
            first
                .begin(&fixture.clients[0], target(2), UploadPurpose::Submit)
                .await
                .unwrap();
            second
                .begin(&fixture.clients[1], target(3), UploadPurpose::Edit)
                .await
                .unwrap();
            let mut peer = fixture.peers.remove(0);
            let _ = peer.close(None).await;
            drop(peer);
            tokio::time::timeout(Duration::from_secs(5), async {
                while !first.observation().sealed {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let first_closed = first.observation();
            let second_live = second.observation();
            assert!(first_closed.sealed && first_closed.entries == 0);
            assert!(!second_live.sealed && second_live.receiving == 1);
            second
                .begin(&fixture.clients[1], target(2), UploadPurpose::Submit)
                .await
                .unwrap();
        })
    })
    .await;
}

#[tokio::test]
async fn upload_u1_live_original_lease_blocks_body_join_until_released() {
    case(|fixture| {
        Box::pin(async move {
            let owner = fixture.uploads(0);
            let client = &fixture.clients[0];
            let upload = owner
                .begin(client, target(2), UploadPurpose::Submit)
                .await
                .unwrap()
                .upload();
            let wire = frame(upload, 0, b"[]");
            owner
                .accept_data(client, InputData::decode_client(&wire).unwrap())
                .await
                .unwrap();
            owner.finish(client, upload, 2).await.unwrap();
            let status = ready(&owner, client, upload).await;
            let lease = owner
                .lease_body(
                    client,
                    status.body().unwrap(),
                    target(2),
                    UploadPurpose::Submit,
                )
                .await
                .unwrap();
            let receipt = fixture
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .prompt_preparation_receipt()
                .unwrap();
            fixture.close_peers().await;
            let node = fixture.node.take().unwrap();
            let shutdown = tokio::spawn(node.shutdown(None));
            tokio::time::timeout(Duration::from_secs(5), async {
                while !receipt.observe().sealed() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let before = receipt.observe();
            let pending = !shutdown.is_finished();
            drop(lease);
            let closed = shutdown.await.unwrap();
            assert!(pending && !before.all_joined() && before.active_use() == 1);
            assert!(closed.is_ok());
            assert!(receipt.observe().all_joined());
        })
    })
    .await;
}
