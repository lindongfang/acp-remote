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

fn runtime_root() -> PathBuf {
    std::env::var_os("NA022_RUNTIME_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("D:/Project/acp-remote/.tmp/independent-na022-20261030"))
}

async fn start_owner(config: NodeConfig) -> R<(LocalIpcAddress, tokio::task::JoinHandle<R<()>>)> {
    let Some(binary) = std::env::var_os("NA022_BINARY") else {
        let runtime = NodeRuntime::start(config, &DpapiProtector).await?;
        let address = runtime.ipc_address().clone();
        return Ok((
            address,
            tokio::spawn(async move {
                runtime.wait_for_stop().await?;
                Ok(())
            }),
        ));
    };
    let parent = config.data_dir.parent().ok_or("data parent")?;
    let path = parent.join("node.toml");
    let text = format!(
        "data_dir = {:?}\nlisten = {:?}\ncanonical_origin = {:?}\ntls_cert = {:?}\ntls_key = {:?}\n",
        config.data_dir.to_string_lossy().replace('\\', "/"),
        config.listen.to_string(),
        config.canonical_origin,
        config.tls_cert.to_string_lossy().replace('\\', "/"),
        config.tls_key.to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&path, text)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(parent.join("node.log"))?;
    let mut child = tokio::process::Command::new(binary)
        .arg("--config")
        .arg(path)
        .args(["daemon", "start"])
        .kill_on_drop(true)
        .stdout(std::process::Stdio::from(log.try_clone()?))
        .stderr(std::process::Stdio::from(log))
        .spawn()?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let address = loop {
        if let Some(status) = child.try_wait()? {
            return Err(format!("fixed binary startup exited {status}").into());
        }
        if config.data_dir.exists() {
            if let Ok(address) = LocalIpcAddress::for_data_directory(&config.data_dir) {
                if ipc::status(&address).await.is_ok() {
                    break address;
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            child.kill().await?;
            child.wait().await?;
            return Err("fixed binary startup timeout".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    Ok((
        address,
        tokio::spawn(async move {
            let status = child.wait().await?;
            if !status.success() {
                return Err(format!("fixed binary exit {status}").into());
            }
            Ok(())
        }),
    ))
}

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
    let status = head
        .split_whitespace()
        .nth(1)
        .ok_or("status")?
        .parse::<u16>()?;
    Ok(json!({"status":status,"body":serde_json::from_str::<Value>(body)?}))
}
fn signed(challenge: &Value, key: &SigningKey, typ: JwsType, claim: bool) -> R<String> {
    let mut p = challenge.as_object().ok_or("challenge")?.clone();
    p.remove("nodePublicKey");
    p.remove("expiresAt");
    p.insert(
        "clientPublicKey".into(),
        json!(URL_SAFE_NO_PAD.encode(public(key)?.as_bytes())),
    );
    if claim {
        p.insert("displayName".into(), json!("Independent old-material test"));
    }
    Ok(jws::sign(typ, &serde_json::to_vec(&p)?, key)?)
}
async fn run_case(mode: &str) -> R<Vec<Value>> {
    let root = runtime_root().join(mode);
    std::fs::create_dir(&root)?;
    protect_private_directory(&root)?;
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    std::fs::write(root.join("cert.pem"), cert.cert.pem())?;
    std::fs::write(root.join("key.pem"), cert.signing_key.serialize_pem())?;
    protect_private_file(&root.join("key.pem"))?;
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
        tls_cert: root.join("cert.pem"),
        tls_key: root.join("key.pem"),
    };
    let (address, mut owner) = start_owner(config.clone()).await?;
    let result:R<Vec<Value>>=async {
 let (mut session,start)=ipc::begin_pairing(&address).await?;let key=SigningKey::from_slice(&[61;32])?;
 let lookup_request=json!({"v":1,"code":start.code,"requestId":Id128::new([21;16])});
 let lookup=post(tls.clone(),"lookup",lookup_request.clone()).await?;check("initial lookup",lookup["status"]==200)?;
 let proof=signed(&lookup["body"],&key,JwsType::PairClaim,true)?;
 let claim_request=json!({"v":1,"proof":proof});let first=post(tls.clone(),"claim",claim_request.clone()).await?;check("initial claim",first["status"]==200)?;
 check("owner sees pending confirmation",session.next_event().await?.event=="pending_confirmation")?;
 let challenge_request=json!({"v":1,"pairingId":start.pairing_id,"clientPublicKey":URL_SAFE_NO_PAD.encode(public(&key)?.as_bytes()),"requestId":Id128::new([22;16])});
 let c=post(tls.clone(),"challenge",challenge_request.clone()).await?;check("initial status challenge",c["status"]==200)?;
 let mut value=c["body"].clone();value["pairingId"]=json!(start.pairing_id);
 let status_request=json!({"v":1,"proof":signed(&value,&key,JwsType::PairStatus,false)?});
 let mut approved_id=None;
 match mode {"approved"=>{session.approve().await?;let e=session.next_event().await?;check("owner approved",e.event=="approved")?;approved_id=e.client_id;drop(session);let issued=post(tls.clone(),"challenge",challenge_request.clone()).await?;check("fresh post-approval status challenge",issued["status"]==200)?;let mut approved_challenge=issued["body"].clone();approved_challenge["pairingId"]=json!(start.pairing_id);let lost=post(tls.clone(),"status",json!({"v":1,"proof":signed(&approved_challenge,&key,JwsType::PairStatus,false)?})).await?;check("approved response really delivered before controlled discard",lost["status"]==200)?;let parsed=jws::ParsedJws::parse(lost["body"]["hostProof"].as_str().ok_or("hostproof")?,&[JwsType::HostStatus])?;parsed.verify(&URL_SAFE_NO_PAD.decode(lookup["body"]["nodePublicKey"].as_str().ok_or("nodekey")?)?)?;check("discarded response host signature and original stable ID",parsed.payload()["status"]=="approved"&&parsed.payload()["clientId"].as_str()==approved_id.as_deref())?;drop(lost);},"rejected"=>{session.reject().await?;check("owner rejected",session.next_event().await?.event=="rejected")?;drop(session);},"cancelled"=>{drop(session);tokio::time::sleep(Duration::from_millis(250)).await;},"restart"=>{drop(session);ipc::stop(&address).await?;(&mut owner).await??;let (restarted,new_owner)=start_owner(config.clone()).await?;check("restart uses same owner pipe",restarted.pipe_name==address.pipe_name)?;owner=new_owner;},_=>return Err("mode".into())}
 let mut rows=vec![];
 for (endpoint,request) in [("lookup",lookup_request),("claim",claim_request),("status",status_request)] {
 if mode=="approved"&&endpoint=="status" {continue;}
 let r=post(tls.clone(),endpoint,request).await?;let pass=r["status"]==401&&r["body"]["error"]["code"]=="pairing_invalid";
 println!("OBS mode={mode} endpoint={endpoint} status={} code={} expected_pairing_invalid={pass}",r["status"],r["body"]["error"]["code"]);
 rows.push(json!({"mode":mode,"endpoint":endpoint,"status":r["status"],"code":r["body"]["error"]["code"],"expectedPairingInvalid":pass}));
 }
 tokio::time::sleep(Duration::from_millis(1100)).await;
 let c=post(tls.clone(),"challenge",challenge_request).await?;
 if mode!="restart" {check("bound key fresh status challenge",c["status"]==200)?;let mut p=c["body"].clone();p["pairingId"]=json!(start.pairing_id);let r=post(tls.clone(),"status",json!({"v":1,"proof":signed(&p,&key,JwsType::PairStatus,false)?})).await?;check("bound key signed terminal status",r["status"]==200)?;
 let raw=r["body"]["hostProof"].as_str().ok_or("hostproof")?;let parsed=jws::ParsedJws::parse(raw,&[JwsType::HostStatus])?;parsed.verify(&URL_SAFE_NO_PAD.decode(lookup["body"]["nodePublicKey"].as_str().ok_or("nodekey")?)?)?;if mode=="approved" {check("fresh retained status recovers same stable ID after controlled response loss",parsed.payload()["status"]=="approved"&&parsed.payload()["clientId"].as_str()==approved_id.as_deref())?;} else {check("terminal and null ID",parsed.payload()["status"]==mode&&parsed.payload()["clientId"].is_null())?;}
 } else {check("restart cannot obtain status challenge",c["status"]==401)?;}
 Ok(rows)
 }.await;
    let stop = ipc::stop(&address).await;
    let join = owner.await?;
    stop?;
    join?;
    check(
        "pipe removed after owner stop",
        !Path::new(&address.pipe_name).exists(),
    )?;
    std::fs::remove_dir_all(&root)?;
    result
}
#[tokio::main]
async fn main() -> R<()> {
    let mut rows = vec![];
    let modes: &[&str] = if std::env::var_os("NA022_APPROVED_ONLY").is_some() {
        &["approved"]
    } else {
        &["cancelled", "rejected", "restart"]
    };
    for mode in modes {
        rows.extend(run_case(mode).await?);
    }
    std::fs::write(
        runtime_root().join(if std::env::var_os("NA022_APPROVED_ONLY").is_some() {
            "approved-results.json"
        } else {
            "results.json"
        }),
        serde_json::to_vec_pretty(&rows)?,
    )?;
    let failures = rows
        .iter()
        .filter(|r| r["expectedPairingInvalid"] != true)
        .count();
    println!(
        "COMPLETE old-material rows={} assertion_failures={failures}",
        rows.len()
    );
    if failures > 0 {
        return Err("old material accepted after terminal".into());
    }
    Ok(())
}
