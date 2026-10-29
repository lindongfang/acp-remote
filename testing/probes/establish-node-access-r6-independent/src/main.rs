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
    let mut needles = vec![
        start.code.replace('-', "").into_bytes(),
        compact.as_bytes().to_vec(),
    ];
    for field in ["nonce", "challengeId", "pairingId"] {
        let s = claim[field].as_str().ok_or("field")?;
        needles.push(s.as_bytes().to_vec());
        needles.push(URL_SAFE_NO_PAD.decode(s)?);
    }
    scan(&needles)?;
    post(tls, "claim", json!({"v":1,"proof":compact})).await?;
    check(
        "actual owner IPC pending confirmation",
        session.next_event().await?.event == "pending_confirmation",
    )?;
    scan(&needles)?;
    session.approve().await?;
    let result = session.next_event().await?;
    check(
        "actual owner IPC approval committed",
        result.event == "approved",
    )?;
    scan(&needles)?;
    Ok(Id128::parse_base64url(
        result.client_id.as_deref().ok_or("no client ID")?,
    )?)
}
fn scan(needles: &[Vec<u8>]) -> R<()> {
    let root = Path::new("D:/Project/acp-remote/.tmp/independent-r6-20261030/network-fixture/data");
    for e in std::fs::read_dir(root)? {
        let p = e?.path();
        if p.is_file()
            && p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("node.sqlite3"))
        {
            let b = std::fs::read(p)?;
            for n in needles {
                if b.windows(n.len()).any(|w| w == n) {
                    return Err("temporary secret persisted".into());
                }
            }
        }
    }
    println!(
        "PASS live database/WAL files do not contain code, proof, nonce, challengeId, pairingId (raw/encoded)"
    );
    Ok(())
}
#[tokio::main]
async fn main() -> R<()> {
    let root = PathBuf::from("D:/Project/acp-remote/.tmp/independent-r6-20261030/network-fixture");
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
    let config = NodeConfig {
        data_dir: root.join("data"),
        listen: "127.0.0.1:30452".parse()?,
        canonical_origin: "https://localhost:30452".into(),
        tls_cert: root.join("server.pem"),
        tls_key: root.join("server-key.pem"),
    };
    let config_path = root.join("node.toml");
    std::fs::write(
        &config_path,
        format!(
            "data_dir = {:?}\nlisten = \"127.0.0.1:30452\"\ncanonical_origin = \"https://localhost:30452\"\ntls_cert = {:?}\ntls_key = {:?}\n",
            root.join("data").to_string_lossy().replace('\\', "/"),
            root.join("server.pem").to_string_lossy().replace('\\', "/"),
            root.join("server-key.pem")
                .to_string_lossy()
                .replace('\\', "/")
        ),
    )?;
    let runtime = NodeRuntime::start(config, &DpapiProtector).await?;
    let address = runtime.ipc_address().clone();
    let owner = tokio::spawn(runtime.wait_for_stop());
    let result: R<()> = async {
        let output = std::process::Command::new(
            "D:/Project/acp-remote/.tmp/independent-r6-20261030/bin/acp-remote.exe",
        )
        .args([
            "--config",
            config_path.to_str().ok_or("path")?,
            "device",
            "pair",
        ])
        .output()?;
        check(
            "r6 live-node noninteractive CLI exit1 before creating pairing",
            output.status.code() == Some(1)
                && String::from_utf8_lossy(&output.stderr)
                    .contains("requires an interactive terminal"),
        )?;
        let key = SigningKey::from_slice(&[53; 32])?;
        let _id = pair(&address, tls, &key).await?;
        Ok(())
    }
    .await;
    let stopped = ipc::stop(&address).await;
    let joined = owner.await?;
    stopped?;
    joined?;
    result?;
    std::fs::remove_dir_all(&root)?;
    check(
        "owned runtime data, cert/key cleaned after owner stop",
        !root.exists(),
    )?;
    Ok(())
}
