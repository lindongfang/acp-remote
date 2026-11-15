//! Developer-only real node pairing/current-read fixtures. They are not WebSocket/Native acceptance.
use super::*;
use crate::{
    application::shared_views::{
        CurrentViewAuthority,
        local_authority::{AuthenticatedLocalViewClient, LocalViewAuthority},
    },
    claims::{AuthProof, Id128, Nonce256, PairClaim, PublicKeySec1},
    domain::{
        EntityId,
        native::{NativeSessionKey, NativeSourceId},
        shared_view::ViewError,
    },
    jws::{self, JwsType},
    platform::DpapiProtector,
};
use p256::ecdsa::SigningKey;
use std::{
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::sync::Mutex;

struct Area {
    parent: PathBuf,
    path: PathBuf,
    marker: String,
}
impl Area {
    fn create() -> Self {
        let parent = std::env::temp_dir().canonicalize().expect("test parent");
        let marker = crate::random::hex(16).expect("self-owned nonce");
        let path = parent.join(format!("acp-shared-view-auth-{marker}"));
        std::fs::create_dir(&path).expect("unique new child");
        std::fs::write(path.join("owner"), &marker).expect("owner marker");
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
            .expect("self-owned TLS");
        std::fs::write(path.join("server.pem"), cert.cert.pem()).expect("certificate");
        std::fs::write(
            path.join("server-key.pem"),
            cert.signing_key.serialize_pem(),
        )
        .expect("own TLS key");
        Self {
            parent,
            path,
            marker,
        }
    }
    fn config(&self) -> NodeConfig {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("test port");
        let listen = listener.local_addr().expect("actual port");
        drop(listener);
        NodeConfig {
            data_dir: self.path.join("data"),
            listen,
            canonical_origin: "https://view.example.test".to_owned(),
            tls_cert: self.path.join("server.pem"),
            tls_key: self.path.join("server-key.pem"),
        }
    }
    fn cleanup(&self) {
        let actual = self.path.canonicalize().expect("owned child still present");
        assert_eq!(actual, self.path);
        assert_eq!(actual.parent(), Some(self.parent.as_path()));
        assert_eq!(
            std::fs::read_to_string(actual.join("owner")).expect("marker"),
            self.marker
        );
        reject_reparse(&actual);
        assert_closed_files(&actual);
        std::fs::remove_dir_all(actual).expect("remove only closed, verified child");
    }
}
fn assert_closed_files(path: &Path) {
    use std::os::windows::fs::OpenOptionsExt;
    if path.is_dir() {
        for entry in std::fs::read_dir(path).expect("owned diagnostic entries") {
            assert_closed_files(&entry.expect("entry").path());
        }
    } else {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .expect("actual closed fixture file permits exclusive open");
        drop(file);
    }
}
fn reject_reparse(path: &Path) {
    use std::os::windows::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path).expect("metadata");
    assert_eq!(
        metadata.file_attributes() & 0x400,
        0,
        "test cleanup refuses reparse points"
    );
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path).expect("own contents") {
            reject_reparse(&entry.expect("entry").path());
        }
    }
}
struct StopSlot {
    join: Mutex<Option<JoinHandle<Result<(), AppError>>>>,
    finished: AtomicBool,
}
struct AuthFixture {
    area: Area,
    pairing: Arc<PairingService>,
    policy: Arc<ApplicationPolicy>,
    authority: LocalViewAuthority,
    control: IpcControl,
    address: LocalIpcAddress,
    stop: Arc<StopSlot>,
    pool: sqlx::SqlitePool,
}
impl AuthFixture {
    async fn start(source: NativeSourceId) -> Self {
        let area = Area::create();
        let runtime = NodeRuntime::start(area.config(), std::sync::Arc::new(DpapiProtector))
            .await
            .expect("actual Node");
        let policy = runtime
            .application_policy()
            .expect("assembled actual policy");
        let authority = policy.local_view_authority(vec![source]);
        let pairing = runtime
            .pairing
            .as_ref()
            .expect("actual pairing service")
            .clone();
        let address = runtime.ipc_address().clone();
        let control = runtime.control.clone();
        let pool = runtime
            .storage
            .as_ref()
            .expect("actual Storage")
            .pool_for_test()
            .clone();
        let stop = Arc::new(StopSlot {
            join: Mutex::new(None),
            finished: AtomicBool::new(false),
        });
        let task_slot = Arc::clone(&stop);
        let task = tokio::spawn(async move {
            let result = runtime.wait_for_stop().await;
            task_slot.finished.store(true, Ordering::Release);
            result
        });
        *stop.join.lock().await = Some(task);
        Self {
            area,
            pairing,
            policy,
            authority,
            control,
            address,
            stop,
            pool,
        }
    }
    async fn authenticate(&self) -> AuthenticatedLocalViewClient {
        let key = loop {
            let seed = zeroize::Zeroizing::new(crate::random::bytes::<32>().expect("CSPRNG"));
            if let Ok(key) = SigningKey::from_slice(seed.as_ref()) {
                break key;
            }
        };
        let public = PublicKeySec1::new(
            key.verifying_key()
                .to_sec1_point(false)
                .as_bytes()
                .try_into()
                .expect("SEC1"),
        )
        .expect("public key");
        let (mut session, start) = ipc::begin_pairing(&self.address)
            .await
            .expect("actual owner pairing");
        let pairing_id = Id128::parse_base64url(&start.pairing_id).expect("pair id");
        let request_id = Id128::new(crate::random::bytes().expect("request nonce"));
        let lookup = self
            .pairing
            .lookup(&start.code, request_id)
            .await
            .expect("actual challenge");
        let claim = PairClaim {
            v: 1,
            node_id: lookup.challenge.node_id,
            origin: lookup.challenge.origin,
            request_id,
            challenge_id: lookup.challenge.challenge_id,
            nonce: lookup.challenge.nonce,
            iat: lookup.challenge.iat,
            exp: lookup.challenge.exp,
            pairing_id,
            client_public_key: public,
            display_name: "View fixture".to_owned(),
        };
        let compact = jws::sign(
            JwsType::PairClaim,
            &serde_json::to_vec(&claim).expect("claim"),
            &key,
        )
        .expect("signed claim");
        self.pairing
            .claim(&compact)
            .await
            .expect("real claim verification");
        assert_eq!(
            session.next_event().await.expect("pending").event,
            "pending_confirmation"
        );
        session.approve().await.expect("owner approval");
        let approved = session.next_event().await.expect("approved");
        assert_eq!(approved.event, "approved");
        let id =
            Id128::parse_base64url(&approved.client_id.expect("actual client ID")).expect("id");
        let nonce = Nonce256::new(crate::random::bytes().expect("client nonce"));
        let lease = self
            .policy
            .issue_authentication_challenge(Some(id), public, nonce)
            .expect("real policy challenge");
        let c = lease.challenge();
        let connection_id = lease.connection_id();
        let proof = AuthProof {
            v: 1,
            node_id: c.node_id,
            node_public_key: c.node_public_key,
            origin: c.origin.clone(),
            connection_id,
            challenge_id: c.challenge_id,
            nonce: c.nonce,
            iat: c.iat,
            exp: c.exp,
            client_id: Some(id),
            client_public_key: public,
            client_nonce: nonce,
        };
        let proof = jws::sign(
            JwsType::AuthClient,
            &serde_json::to_vec(&proof).expect("proof fields"),
            &key,
        )
        .expect("signed AuthProof");
        let actor = self
            .policy
            .authenticate(connection_id, &proof)
            .await
            .expect("actual authentication");
        self.authority
            .accept_actor(actor)
            .await
            .expect("current actor, same actual policy/trust")
    }
    async fn close(&self) {
        self.authority.close();
        self.control.request_stop();
        let mut slot = self.stop.join.lock().await;
        if let Some(task) = slot.as_mut() {
            task.await
                .expect("real node task join")
                .expect("actual node resource close");
            slot.take();
        }
        drop(slot);
        assert!(self.stop.finished.load(Ordering::Acquire));
        assert!(self.pool.is_closed(), "same original pool is closed");
        assert_eq!(
            self.pool.size(),
            0,
            "node join released SQLite while original pool clones remain"
        );
        self.area.cleanup();
    }
}
impl Drop for AuthFixture {
    fn drop(&mut self) {
        self.authority.close();
        self.control.request_stop();
    }
}

#[tokio::test]
async fn shared_view_real_pairing_authentication_revoke_and_other_client_are_independent() {
    let source = NativeSourceId(EntityId::from_bytes([31; 16]));
    let fixture = AuthFixture::start(source).await;
    let a = fixture.authenticate().await;
    let b = fixture.authenticate().await;
    let key =
        NativeSessionKey::from_config(Some(source.0), "test-native").expect("configured source");
    assert_eq!(
        fixture.authority.with_current(&a, &key, || Ok(17)).await,
        Ok(17)
    );
    let wait = fixture.authority.wait_client_change(&a);
    tokio::pin!(wait);
    let revoke = async {
        fixture.authority.test_wait_until_registered().await;
        assert_eq!(
            fixture.authority.test_registered_waits(),
            1,
            "actual post-SQL idle wait registered before revoke"
        );
        ipc::device_revoke(&fixture.address, a.client_id())
            .await
            .expect("real revoke COMMIT");
    };
    let (wait_result, ()) = timeout(Duration::from_secs(5), async { tokio::join!(wait, revoke) })
        .await
        .expect("idle wait and revoke are both continuously polled");
    assert_eq!(wait_result, Err(ViewError::Revoked));
    let copied = std::sync::atomic::AtomicBool::new(false);
    assert_eq!(
        fixture
            .authority
            .with_current(&a, &key, || {
                copied.store(true, Ordering::Release);
                Ok(())
            })
            .await,
        Err(ViewError::Revoked)
    );
    assert!(!copied.load(Ordering::Acquire));
    assert_eq!(
        fixture.authority.with_current(&b, &key, || Ok(19)).await,
        Ok(19)
    );
    let foreign =
        NativeSessionKey::from_config(Some(EntityId::from_bytes([32; 16])), "test-native")
            .expect("other source");
    assert_eq!(
        fixture
            .authority
            .with_current(&b, &foreign, || Ok(()))
            .await,
        Err(ViewError::InvalidTarget)
    );
    fixture.close().await;
}

#[tokio::test]
async fn shared_view_local_issuer_does_not_become_a_socket_or_another_local_issuer() {
    let source = NativeSourceId(EntityId::from_bytes([33; 16]));
    let fixture = AuthFixture::start(source).await;
    let client = fixture.authenticate().await;
    let other = fixture.policy.local_view_authority(vec![source]);
    assert!(matches!(
        other.client_binding(&client),
        Err(ViewError::InvalidClient)
    ));
    assert!(
        fixture
            .authority
            .client_binding(&client)
            .expect("local")
            .same_client(
                &fixture
                    .authority
                    .client_binding(&client)
                    .expect("same local")
            )
    );
    fixture.close().await;
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "One actual authenticated node and explicit IO/seal/credit stage assertions keep this developer race sequence self-contained"
)]
async fn shared_view_actual_local_peer_credit_flush_seals_and_revocation() {
    use crate::application::shared_views::{
        local_transfer::{LocalCreditResult, LocalTransferPhase, OwnedDisplaySendGroup},
        wire::{HEADER_BYTES, RecordKind, TransferHeader},
    };
    use crate::domain::shared_view::ViewEpoch;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let source = NativeSourceId(EntityId::from_bytes([34; 16]));
    let fixture = AuthFixture::start(source).await;
    let client = fixture.authenticate().await;
    let key = NativeSessionKey::from_config(Some(source.0), "local-byte-test")
        .expect("configured source");
    let binding = fixture
        .authority
        .client_binding(&client)
        .expect("real current client");
    let deadline = || tokio::time::Instant::now() + Duration::from_secs(5);
    let group = OwnedDisplaySendGroup::new();
    let view = group.view(ViewEpoch::fresh().expect("actual epoch"));
    let stream = view.open(binding.clone()).expect("stream");
    let (endpoint, mut peer) = tokio::io::duplex(128);
    let (mut writer, mut reader) = stream
        .bind_duplex(endpoint, &binding)
        .expect("one real endpoint");
    let (extra, _extra_peer) = tokio::io::duplex(64);
    assert!(
        stream.bind_duplex(extra, &binding).is_err(),
        "same stream cannot install a second endpoint"
    );
    peer.write_all(
        &TransferHeader::credit(stream.id(), 0)
            .encode(&[])
            .expect("initial credit"),
    )
    .await
    .expect("real credit write");
    assert_eq!(
        reader
            .read_credit(&fixture.authority, &client, &key, deadline())
            .await,
        Ok(LocalCreditResult::Initial)
    );
    let payload: Vec<u8> = (0_u8..251).cycle().take(16384).collect();
    let chunk = stream
        .prepare_copy(
            &fixture.authority,
            &client,
            &key,
            1,
            0,
            16385,
            RecordKind::OriginalRaw,
            || Ok(payload.clone()),
        )
        .await
        .expect("current chunk copy");
    peer.write_all(
        &TransferHeader::credit(stream.id(), 1)
            .encode(&[])
            .expect("guessed credit"),
    )
    .await
    .expect("credit");
    assert_eq!(
        reader
            .read_credit(&fixture.authority, &client, &key, deadline())
            .await,
        Err(ViewError::InvalidCursor),
        "Prepared cannot accept future credit"
    );
    let mut gate = stream
        .test_pause_after_flush()
        .expect("one real after-flush pause");
    let sent = writer.write_chunk(chunk, &fixture.authority, &client, &key, deadline());
    let received = async {
        let mut bytes = vec![0; HEADER_BYTES + payload.len()];
        peer.read_exact(&mut bytes)
            .await
            .expect("real peer receives complete bytes");
        let (header, actual) = TransferHeader::decode(&bytes).expect("actual opaque wire");
        assert_eq!(actual, payload);
        assert_eq!(header.packet(), 1);
        gate.arrived()
            .await
            .expect("actual flush fact reached outside actor gate");
        let observed = stream.observation().expect("actual flush observation");
        assert_eq!(observed.highest_flushed, 0);
        assert_eq!(observed.actual_flush_successes, 1);
        assert_eq!(observed.highest_committed, 0);
        assert_eq!(observed.cursor, 0);
        peer.write_all(
            &TransferHeader::credit(stream.id(), 1)
                .encode(&[])
                .expect("true peer credit"),
        )
        .await
        .expect("real peer credit write");
        assert_eq!(
            reader
                .read_credit(&fixture.authority, &client, &key, deadline())
                .await,
            Ok(LocalCreditResult::HeldEarly)
        );
        assert_eq!(stream.observation().expect("no early commit").cursor, 0);
        gate.release();
    };
    let (result, ()) = timeout(Duration::from_secs(5), async {
        tokio::join!(sent, received)
    })
    .await
    .expect("owned local writer and actual peer join");
    assert!(result.expect("actual flush").committed);
    assert_eq!(stream.observation().expect("committed").cursor, 16384);
    let chunk = stream
        .prepare_copy(
            &fixture.authority,
            &client,
            &key,
            1,
            16384,
            16385,
            RecordKind::OriginalRaw,
            || Ok(vec![255]),
        )
        .await
        .expect("next chunk only after credit and flush");
    let sent = writer.write_chunk(chunk, &fixture.authority, &client, &key, deadline());
    let received = async {
        let mut bytes = vec![0; HEADER_BYTES + 1];
        peer.read_exact(&mut bytes).await.expect("next real frame");
        assert_eq!(TransferHeader::decode(&bytes).expect("decode").1, [255]);
    };
    let (result, ()) = tokio::join!(sent, received);
    assert!(!result.expect("flush without early credit").committed);
    assert_eq!(
        stream.observation().expect("await true credit").cursor,
        16384
    );
    peer.write_all(
        &TransferHeader::credit(stream.id(), 2)
            .encode(&[])
            .expect("credit2"),
    )
    .await
    .expect("peer credit");
    assert_eq!(
        reader
            .read_credit(&fixture.authority, &client, &key, deadline())
            .await,
        Ok(LocalCreditResult::Committed)
    );
    assert_eq!(
        stream
            .observation()
            .expect("normal credit moves cursor")
            .cursor,
        16385
    );
    peer.write_all(
        &TransferHeader::credit(stream.id(), 1)
            .encode(&[])
            .expect("duplicate"),
    )
    .await
    .expect("peer duplicate");
    assert_eq!(
        reader
            .read_credit(&fixture.authority, &client, &key, deadline())
            .await,
        Ok(LocalCreditResult::Duplicate)
    );
    assert_eq!(
        stream.observation().expect("one permit").phase,
        LocalTransferPhase::Ready
    );
    drop((writer, reader, peer));

    for which in 0..3 {
        let group = OwnedDisplaySendGroup::new();
        let view = group.view(ViewEpoch::fresh().expect("epoch"));
        let stream = view.open(binding.clone()).expect("stream");
        let (endpoint, mut peer) = tokio::io::duplex(128);
        let (mut writer, mut reader) = stream
            .bind_duplex(endpoint, &binding)
            .expect("real owned endpoint");
        peer.write_all(
            &TransferHeader::credit(stream.id(), 0)
                .encode(&[])
                .expect("initial"),
        )
        .await
        .expect("initial write");
        reader
            .read_credit(&fixture.authority, &client, &key, deadline())
            .await
            .expect("initial");
        let chunk = stream
            .prepare_copy(
                &fixture.authority,
                &client,
                &key,
                1,
                0,
                1,
                RecordKind::OriginalRaw,
                || Ok(vec![42]),
            )
            .await
            .expect("prepared before seal");
        match which {
            0 => group.seal().expect("group seal"),
            1 => view.seal().expect("view seal"),
            _ => stream.seal().expect("stream seal"),
        }
        assert_eq!(
            writer
                .write_chunk(chunk, &fixture.authority, &client, &key, deadline())
                .await,
            Err(ViewError::StaleCursor)
        );
        let observed = stream.observation().expect("no begin");
        assert_eq!(observed.highest_started, 0);
        assert_eq!(observed.cursor, 0);
        drop((writer, reader));
        let mut received = Vec::new();
        peer.read_to_end(&mut received)
            .await
            .expect("true endpoint EOF");
        assert!(received.is_empty(), "seal won before first byte");
    }

    let group = OwnedDisplaySendGroup::new();
    let view = group.view(ViewEpoch::fresh().expect("epoch"));
    let stream = view.open(binding.clone()).expect("stream");
    let (endpoint, mut peer) = tokio::io::duplex(128);
    let (mut writer, mut reader) = stream
        .bind_duplex(endpoint, &binding)
        .expect("real endpoint");
    peer.write_all(
        &TransferHeader::credit(stream.id(), 0)
            .encode(&[])
            .expect("initial"),
    )
    .await
    .expect("initial");
    reader
        .read_credit(&fixture.authority, &client, &key, deadline())
        .await
        .expect("initial");
    let chunk = stream
        .prepare_copy(
            &fixture.authority,
            &client,
            &key,
            1,
            0,
            16384,
            RecordKind::OriginalRaw,
            || Ok(vec![41; 16384]),
        )
        .await
        .expect("copy");
    let sent = writer.write_chunk(chunk, &fixture.authority, &client, &key, deadline());
    let received = async {
        stream.test_wait_started().await;
        let observed = stream.observation().expect("actual first write");
        assert_eq!(observed.highest_started, 1);
        assert_eq!(observed.highest_flushed, 0);
        group
            .seal()
            .expect("synchronous seal while writer cannot finish into capacity128");
        let mut received = vec![0; HEADER_BYTES + 16384];
        peer.read_exact(&mut received)
            .await
            .expect("already begun fixed chunk may finish");
        assert_eq!(
            TransferHeader::decode(&received).expect("actual frame").1,
            vec![41; 16384]
        );
    };
    let (result, ()) = timeout(Duration::from_secs(5), async {
        tokio::join!(sent, received)
    })
    .await
    .expect("seal never waits for slow network");
    let actual = result.expect("actual fixed chunk flushed");
    assert!(actual.sealed_after_start);
    assert!(!actual.committed);
    assert_eq!(stream.observation().expect("sealed cursor").cursor, 0);
    assert!(matches!(
        stream
            .prepare_copy(
                &fixture.authority,
                &client,
                &key,
                2,
                0,
                1,
                RecordKind::OriginalRaw,
                || Ok(vec![9])
            )
            .await,
        Err(ViewError::StaleCursor)
    ));
    drop((writer, reader, peer));

    let group = OwnedDisplaySendGroup::new();
    let view = group.view(ViewEpoch::fresh().expect("epoch"));
    let stream = view.open(binding.clone()).expect("stream");
    let (endpoint, mut peer) = tokio::io::duplex(128);
    let (mut writer, mut reader) = stream.bind_duplex(endpoint, &binding).expect("endpoint");
    peer.write_all(
        &TransferHeader::credit(stream.id(), 0)
            .encode(&[])
            .expect("initial"),
    )
    .await
    .expect("initial");
    reader
        .read_credit(&fixture.authority, &client, &key, deadline())
        .await
        .expect("initial");
    let chunk = stream
        .prepare_copy(
            &fixture.authority,
            &client,
            &key,
            1,
            0,
            1,
            RecordKind::OriginalRaw,
            || Ok(vec![8]),
        )
        .await
        .expect("copy before actual revoke");
    ipc::device_revoke(&fixture.address, client.client_id())
        .await
        .expect("real revoke COMMIT");
    assert_eq!(
        writer
            .write_chunk(chunk, &fixture.authority, &client, &key, deadline())
            .await,
        Err(ViewError::Revoked)
    );
    drop((writer, reader));
    let mut received = Vec::new();
    peer.read_to_end(&mut received).await.expect("true EOF");
    assert!(received.is_empty());
    assert_eq!(stream.observation().expect("revoked no commit").cursor, 0);
    fixture.close().await;
}
