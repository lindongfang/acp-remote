use acp_remote::{
    access_policy::{
        AccessPolicyError, ApplicationPolicy, ApprovalPresentation, AuthenticatedActor,
        PolicyClock, RuntimeCapabilityRequest,
    },
    claims::{AuthProof, Id128, Nonce256, PublicKeySec1},
    config::NodeConfig,
    ipc::{self, LocalIpcAddress},
    jws::{self, JwsType},
    node::NodeRuntime,
    platform::{DpapiProtector, protect_private_directory, protect_private_file},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::SigningKey;
use serde_json::{Value, json};
use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
type R<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn check(name: &str, value: bool) -> R<()> {
    if !value {
        return Err(format!("FAIL {name}").into());
    }
    println!("PASS {name}");
    Ok(())
}
fn public(key: &SigningKey) -> R<PublicKeySec1> {
    Ok(PublicKeySec1::new(
        key.verifying_key()
            .to_sec1_point(false)
            .as_bytes()
            .try_into()?,
    )?)
}
fn proof(
    policy: &ApplicationPolicy,
    client: Option<Id128>,
    key: &SigningKey,
    n: u8,
) -> R<(acp_remote::access_policy::AuthenticationChallenge, String)> {
    let nonce = Nonce256::new([n; 32]);
    let lease = policy.issue_authentication_challenge(client, public(key)?, nonce)?;
    let c = lease.challenge();
    let payload = AuthProof {
        v: 1,
        node_id: c.node_id,
        node_public_key: c.node_public_key,
        origin: c.origin.clone(),
        connection_id: lease.connection_id(),
        challenge_id: c.challenge_id,
        nonce: c.nonce,
        iat: c.iat,
        exp: c.exp,
        client_id: client,
        client_public_key: public(key)?,
        client_nonce: nonce,
    };
    let signature = jws::sign(JwsType::AuthClient, &serde_json::to_vec(&payload)?, key)?;
    Ok((lease, signature))
}
async fn post(tls: Arc<rustls::ClientConfig>, endpoint: &str, value: Value) -> R<Value> {
    let tcp = TcpStream::connect("127.0.0.1:30452").await?;
    let mut stream = tokio_rustls::TlsConnector::from(tls)
        .connect("localhost".try_into()?, tcp)
        .await?;
    let body = value.to_string();
    stream.write_all(format!("POST /api/access/v1/pair/{endpoint} HTTP/1.1\r\nHost: localhost:30452\r\nOrigin: https://localhost:30452\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await?;
    let response = String::from_utf8(bytes)?;
    let (head, body) = response.split_once("\r\n\r\n").ok_or("HTTP separator")?;
    check("real pairing HTTP 200", head.starts_with("HTTP/1.1 200"))?;
    Ok(serde_json::from_str(body)?)
}
async fn pair(
    address: &LocalIpcAddress,
    tls: Arc<rustls::ClientConfig>,
    key: &SigningKey,
) -> R<Id128> {
    let (mut session, start) = ipc::begin_pairing(address).await?;
    let lookup = post(
        tls.clone(),
        "lookup",
        json!({"v":1,"code":start.code,"requestId":Id128::new([21;16])}),
    )
    .await?;
    let mut claim = lookup.as_object().ok_or("lookup object")?.clone();
    claim.remove("nodePublicKey");
    claim.remove("expiresAt");
    claim.insert(
        "clientPublicKey".into(),
        json!(URL_SAFE_NO_PAD.encode(public(key)?.as_bytes())),
    );
    claim.insert(
        "displayName".into(),
        json!("Independent application-policy actor"),
    );
    let compact = jws::sign(JwsType::PairClaim, &serde_json::to_vec(&claim)?, key)?;
    post(tls, "claim", json!({"v":1,"proof":compact})).await?;
    check(
        "actual owner IPC pending confirmation",
        session.next_event().await?.event == "pending_confirmation",
    )?;
    session.approve().await?;
    let result = session.next_event().await?;
    check(
        "actual owner IPC approval committed",
        result.event == "approved",
    )?;
    Ok(Id128::parse_base64url(
        result.client_id.as_deref().ok_or("no client ID")?,
    )?)
}
struct Clock {
    now: u64,
    fail: bool,
}
impl PolicyClock for Clock {
    fn unix_seconds(&self) -> Result<u64, AccessPolicyError> {
        if self.fail {
            Err(AccessPolicyError::ClockUnavailable)
        } else {
            Ok(self.now)
        }
    }
}
struct BlockingClock {
    entered: std::sync::atomic::AtomicBool,
    released: std::sync::Mutex<bool>,
    wake: std::sync::Condvar,
}
impl PolicyClock for BlockingClock {
    fn unix_seconds(&self) -> Result<u64, AccessPolicyError> {
        self.entered.store(true, Ordering::SeqCst);
        let mut released = self
            .released
            .lock()
            .map_err(|_| AccessPolicyError::ClockUnavailable)?;
        while !*released {
            released = self
                .wake
                .wait(released)
                .map_err(|_| AccessPolicyError::ClockUnavailable)?;
        }
        Ok(1000)
    }
}
async fn denied(
    policy: &ApplicationPolicy,
    actor: &AuthenticatedActor,
    expiry: Option<u64>,
    expected: &str,
    calls: &AtomicUsize,
) -> R<()> {
    let request = RuntimeCapabilityRequest {
        name: "independent.mock-harness-operation".into(),
        approval: expiry.map(|expires_at| ApprovalPresentation { expires_at }),
    };
    let before = calls.load(Ordering::SeqCst);
    let result = policy
        .request_runtime_capability(actor, &request, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
        })
        .await;
    let actual = match result {
        Err(AccessPolicyError::ApprovalRequired) => "missing",
        Err(AccessPolicyError::ApprovalExpired) => "expired",
        Err(AccessPolicyError::ApprovalUnverified) => "unverified",
        Err(AccessPolicyError::ClockUnavailable) => "clock",
        Err(AccessPolicyError::InvalidActor) => "actor",
        Err(AccessPolicyError::NodeUnavailable) => "node",
        Err(AccessPolicyError::StorageUnavailable) => "storage",
        Err(e) => return Err(e.into()),
        Ok(()) => "ALLOWED",
    };
    check(
        &format!("runtime gate {expected}, dispatcher unchanged"),
        actual == expected && calls.load(Ordering::SeqCst) == before,
    )
}
async fn matrix(
    policy: Arc<ApplicationPolicy>,
    address: &LocalIpcAddress,
    tls: Arc<rustls::ClientConfig>,
) -> R<(AuthenticatedActor, ApplicationPolicy)> {
    let key = SigningKey::from_slice(&[0x52; 32])?;
    let id = pair(address, tls, &key).await?;
    let (lease, compact) = proof(&policy, Some(id), &key, 1)?;
    let actor = policy.authenticate(lease.connection_id(), &compact).await?;
    check(
        "real proof mints actor for paired stable ID",
        actor.client_id() == id.to_base64url(),
    )?;
    check(
        "same proof cannot authenticate twice",
        policy
            .authenticate(lease.connection_id(), &compact)
            .await
            .is_err(),
    )?;
    policy.require_current_actor(&actor).await?;
    check("real DB current-actor qualification", true)?;
    let calls = Arc::new(AtomicUsize::new(0));
    let controlled = policy.with_clock(Arc::new(Clock {
        now: 1000,
        fail: false,
    }));
    denied(&controlled, &actor, None, "missing", &calls).await?;
    for expiry in [0, 999, 1000] {
        denied(&controlled, &actor, Some(expiry), "expired", &calls).await?;
    }
    for expiry in [1001, u64::MAX] {
        denied(&controlled, &actor, Some(expiry), "unverified", &calls).await?;
    }
    let advanced = policy.with_clock(Arc::new(Clock {
        now: 1002,
        fail: false,
    }));
    denied(&advanced, &actor, Some(1001), "expired", &calls).await?;
    let broken_clock = policy.with_clock(Arc::new(Clock {
        now: 1000,
        fail: true,
    }));
    denied(&broken_clock, &actor, Some(1001), "clock", &calls).await?;
    // Positive spy control is outside the product gate and does not grant permission.
    let spy = || {
        calls.fetch_add(1, Ordering::SeqCst);
    };
    spy();
    check(
        "dispatcher spy positive control",
        calls.load(Ordering::SeqCst) == 1,
    )?;
    calls.store(0, Ordering::SeqCst);
    let (dropped, dropped_proof) = proof(&policy, Some(id), &key, 2)?;
    let dropped_scope = dropped.connection_id();
    drop(dropped);
    check(
        "lease Drop invalidates its private challenge",
        policy
            .authenticate(dropped_scope, &dropped_proof)
            .await
            .is_err(),
    )?;
    let (survivor, survivor_proof) = proof(&policy, Some(id), &key, 3)?;
    let (other, _) = proof(&policy, Some(id), &key, 4)?;
    check(
        "scope IDs are independently randomized",
        survivor.connection_id() != other.connection_id(),
    )?;
    drop(other);
    check(
        "dropping another lease does not cancel survivor",
        policy
            .authenticate(survivor.connection_id(), &survivor_proof)
            .await
            .is_ok(),
    )?;
    for _ in 0..4100 {
        drop(policy.issue_authentication_challenge(
            Some(id),
            public(&key)?,
            Nonce256::new([0x55; 32]),
        )?);
    }
    check(
        "4100 issue/drop cycles do not exhaust the 4096 challenge budget",
        true,
    )?;
    let (wrong, _) = proof(&policy, Some(id), &key, 5)?;
    check(
        "malformed proof rejected",
        policy
            .authenticate(wrong.connection_id(), "independent-invalid-proof")
            .await
            .is_err(),
    )?;
    let (fresh, fresh_proof) = proof(&policy, None, &key, 6)?;
    check(
        "failed proof does not poison unrelated scope",
        policy
            .authenticate(fresh.connection_id(), &fresh_proof)
            .await
            .is_ok(),
    )?;
    let stranger = SigningKey::from_slice(&[0x53; 32])?;
    let (unknown, unknown_proof) = proof(&policy, None, &stranger, 7)?;
    check(
        "valid proof with untrusted public key cannot mint actor",
        matches!(
            policy
                .authenticate(unknown.connection_id(), &unknown_proof)
                .await,
            Err(AccessPolicyError::InvalidActor)
        ),
    )?;
    let blocking_clock = Arc::new(BlockingClock {
        entered: std::sync::atomic::AtomicBool::new(false),
        released: std::sync::Mutex::new(false),
        wake: std::sync::Condvar::new(),
    });
    let gated_policy = policy.with_clock(blocking_clock.clone());
    let gated_actor = actor.clone();
    let gated_calls = calls.clone();
    let gated_request = tokio::spawn(async move {
        gated_policy
            .request_runtime_capability(
                &gated_actor,
                &RuntimeCapabilityRequest {
                    name: "independent.gate-ordering".into(),
                    approval: Some(ApprovalPresentation { expires_at: 1001 }),
                },
                |_| {
                    gated_calls.fetch_add(1, Ordering::SeqCst);
                },
            )
            .await
    });
    let gate_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while !blocking_clock.entered.load(Ordering::SeqCst) {
        if tokio::time::Instant::now() > gate_deadline {
            return Err("runtime request did not reach actual policy clock".into());
        }
        tokio::task::yield_now().await;
    }
    let revoke_address = address.clone();
    let revoke_id = id.to_base64url();
    let revoke_task =
        tokio::spawn(async move { ipc::device_revoke(&revoke_address, &revoke_id).await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    let blocked_by_shared_gate = !revoke_task.is_finished();
    *blocking_clock
        .released
        .lock()
        .map_err(|_| "clock lock poisoned")? = true;
    blocking_clock.wake.notify_all();
    check(
        "owner revoke waits while real runtime capability gate is held",
        blocked_by_shared_gate,
    )?;
    check(
        "blocked runtime request remains unverified without dispatch",
        matches!(
            tokio::time::timeout(Duration::from_secs(5), gated_request).await??,
            Err(AccessPolicyError::ApprovalUnverified)
        ) && calls.load(Ordering::SeqCst) == 0,
    )?;
    let revoked = tokio::time::timeout(Duration::from_secs(5), revoke_task).await???;
    check(
        "actual DB revocation completed",
        revoked.client_id == id.to_base64url(),
    )?;
    check(
        "old actor rejected by same shared trust",
        matches!(
            policy.require_current_actor(&actor).await,
            Err(AccessPolicyError::InvalidActor)
        ),
    )?;
    denied(&controlled, &actor, Some(u64::MAX), "actor", &calls).await?;
    let (after, after_proof) = proof(&policy, Some(id), &key, 8)?;
    check(
        "revoked key cannot remint actor",
        matches!(
            policy
                .authenticate(after.connection_id(), &after_proof)
                .await,
            Err(AccessPolicyError::InvalidActor)
        ),
    )?;
    check(
        "dispatcher total is zero",
        calls.load(Ordering::SeqCst) == 0,
    )?;
    Ok((actor, controlled))
}
async fn run(root: PathBuf) -> R<()> {
    if root.exists() {
        return Err("refuse existing run directory".into());
    }
    std::fs::create_dir(&root)?;
    protect_private_directory(&root)?;
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    std::fs::write(root.join("server.pem"), cert.cert.pem())?;
    std::fs::write(
        root.join("server-key.pem"),
        cert.signing_key.serialize_pem(),
    )?;
    protect_private_file(&root.join("server-key.pem"))?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.cert.der().clone())?;
    let tls = Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    );
    let runtime = NodeRuntime::start(
        NodeConfig {
            data_dir: root.join("data"),
            listen: "127.0.0.1:30452".parse()?,
            canonical_origin: "https://localhost:30452".into(),
            tls_cert: root.join("server.pem"),
            tls_key: root.join("server-key.pem"),
        },
        &DpapiProtector,
    )
    .await?;
    let policy = runtime
        .application_policy()
        .ok_or("missing ordinary application-policy API")?;
    let address = runtime.ipc_address().clone();
    let owner = tokio::spawn(runtime.wait_for_stop());
    let result = tokio::time::timeout(
        Duration::from_secs(60),
        matrix(policy.clone(), &address, tls.clone()),
    )
    .await;
    let dummy_key = SigningKey::from_slice(&[0x54; 32])?;
    let (pending_at_stop, real_stop_proof) = proof(&policy, None, &dummy_key, 11)?;
    let stop_address = address.clone();
    let stop = tokio::spawn(async move { ipc::stop(&stop_address).await });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if matches!(
            policy.issue_authentication_challenge(
                None,
                public(&dummy_key)?,
                Nonce256::new([9; 32])
            ),
            Err(AccessPolicyError::NodeUnavailable)
        ) {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            return Err("stop did not close policy issuance".into());
        }
        tokio::task::yield_now().await;
    }
    println!(
        "OBSERVATION policy gate closed while runtime_join_finished={}",
        owner.is_finished()
    );
    check(
        "stop gate rejects actual prepared signed authentication",
        matches!(
            policy
                .authenticate(pending_at_stop.connection_id(), &real_stop_proof)
                .await,
            Err(AccessPolicyError::NodeUnavailable)
        ),
    )?;
    if let Ok(Ok((actor, clocked))) = &result {
        check(
            "stop gate rejects current-actor before awaiting shutdown",
            matches!(
                policy.require_current_actor(actor).await,
                Err(AccessPolicyError::NodeUnavailable)
            ),
        )?;
        denied(clocked, actor, Some(u64::MAX), "node", &AtomicUsize::new(0)).await?;
    }
    let stopped = tokio::time::timeout(Duration::from_secs(20), stop).await???;
    let joined = tokio::time::timeout(Duration::from_secs(25), owner).await???;
    let _ = (stopped, joined);
    check(
        "owner stop and runtime join completed with retained policy",
        true,
    )?;
    match result {
        Ok(Ok((actor, clocked))) => {
            check(
                "stop rejects original current-actor check",
                matches!(
                    policy.require_current_actor(&actor).await,
                    Err(AccessPolicyError::NodeUnavailable)
                ),
            )?;
            check(
                "stop rejects derived-clock current-actor check",
                matches!(
                    clocked.require_current_actor(&actor).await,
                    Err(AccessPolicyError::NodeUnavailable)
                ),
            )?;
            check(
                "stop rejects authenticate before proof parsing",
                matches!(
                    policy.authenticate(Id128::new([10; 16]), "invalid").await,
                    Err(AccessPolicyError::NodeUnavailable)
                ),
            )?;
            denied(&clocked, &actor, None, "node", &AtomicUsize::new(0)).await?;
            let other = NodeRuntime::start(
                NodeConfig {
                    data_dir: root.join("other-node-data"),
                    listen: "127.0.0.1:30452".parse()?,
                    canonical_origin: "https://localhost:30452".into(),
                    tls_cert: root.join("server.pem"),
                    tls_key: root.join("server-key.pem"),
                },
                &DpapiProtector,
            )
            .await?;
            let other_policy = other.application_policy().ok_or("second policy missing")?;
            let other_address = other.ipc_address().clone();
            let other_owner = tokio::spawn(other.wait_for_stop());
            let cross_node = other_policy.require_current_actor(&actor).await;
            let cross_request = other_policy
                .request_runtime_capability(
                    &actor,
                    &RuntimeCapabilityRequest {
                        name: "independent.cross-node".into(),
                        approval: None,
                    },
                    |_| {},
                )
                .await;
            ipc::stop(&other_address).await?;
            tokio::time::timeout(Duration::from_secs(25), other_owner).await???;
            check(
                "real second node rejects actor from first node",
                matches!(cross_node, Err(AccessPolicyError::InvalidActor))
                    && matches!(cross_request, Err(AccessPolicyError::InvalidActor)),
            )?;
        }
        Ok(Err(error)) => return Err(error),
        Err(error) => return Err(error.into()),
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:30452")?;
    drop(listener);
    let lock = acp_remote::data_dir::DataDirectory::acquire(&root.join("data"))?;
    lock.unlock()?;
    drop(lock);
    check("listener and data lock reusable after stop", true)?;
    let drop_runtime = NodeRuntime::start(
        NodeConfig {
            data_dir: root.join("drop-node-data"),
            listen: "127.0.0.1:30452".parse()?,
            canonical_origin: "https://localhost:30452".into(),
            tls_cert: root.join("server.pem"),
            tls_key: root.join("server-key.pem"),
        },
        &DpapiProtector,
    )
    .await?;
    let drop_policy = drop_runtime
        .application_policy()
        .ok_or("drop policy missing")?;
    let drop_key = SigningKey::from_slice(&[0x63; 32])?;
    let drop_id = pair(drop_runtime.ipc_address(), tls, &drop_key).await?;
    let (actor_lease, actor_signature) = proof(&drop_policy, Some(drop_id), &drop_key, 21)?;
    let drop_actor = drop_policy
        .authenticate(actor_lease.connection_id(), &actor_signature)
        .await?;
    drop_policy.require_current_actor(&drop_actor).await?;
    let drop_clocked = drop_policy.with_clock(Arc::new(Clock {
        now: 1000,
        fail: false,
    }));
    let (prepared, prepared_signature) = proof(&drop_policy, Some(drop_id), &drop_key, 22)?;
    drop(drop_runtime);
    check(
        "bare Drop synchronously rejects issue before yielding",
        matches!(
            drop_policy.issue_authentication_challenge(
                Some(drop_id),
                public(&drop_key)?,
                Nonce256::new([23; 32])
            ),
            Err(AccessPolicyError::NodeUnavailable)
        ),
    )?;
    check(
        "bare Drop rejects a valid prepared proof",
        matches!(
            drop_policy
                .authenticate(prepared.connection_id(), &prepared_signature)
                .await,
            Err(AccessPolicyError::NodeUnavailable)
        ),
    )?;
    check(
        "bare Drop rejects previously current actor",
        matches!(
            drop_policy.require_current_actor(&drop_actor).await,
            Err(AccessPolicyError::NodeUnavailable)
        ),
    )?;
    check(
        "bare Drop closes derived-clock gate too",
        matches!(
            drop_clocked.require_current_actor(&drop_actor).await,
            Err(AccessPolicyError::NodeUnavailable)
        ),
    )?;
    denied(
        &drop_clocked,
        &drop_actor,
        Some(u64::MAX),
        "node",
        &AtomicUsize::new(0),
    )
    .await?;
    drop(prepared);
    drop(actor_lease);
    drop(drop_actor);
    drop(drop_clocked);
    drop(drop_policy);
    let release_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(listener) = std::net::TcpListener::bind("127.0.0.1:30452") {
            drop(listener);
            break;
        }
        if tokio::time::Instant::now() >= release_deadline {
            return Err("bare Drop listener remained bound".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let drop_lock = acp_remote::data_dir::DataDirectory::acquire(&root.join("drop-node-data"))?;
    drop_lock.unlock()?;
    drop(drop_lock);
    check("bare Drop tasks release listener and directory lock", true)?;
    println!("PROBE COMPLETE");
    Ok(())
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> R<()> {
    let root = std::env::args()
        .nth(1)
        .ok_or("pass unique owner-only .tmp runtime path")?;
    if !Path::new(&root).is_absolute() {
        return Err("absolute runtime root required".into());
    }
    run(PathBuf::from(root)).await
}
