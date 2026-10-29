use acp_remote::{
    claims::Id128,
    config::NodeConfig,
    ipc::{self, LocalIpcAddress},
    node::NodeRuntime,
    platform::{DpapiProtector, current_user_sid, protect_private_directory, protect_private_file},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::SigningKey;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{
    error::Error,
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpStream, windows::named_pipe::ClientOptions},
};
type R<T> = Result<T, Box<dyn Error + Send + Sync>>;
fn id(n: u128) -> String {
    Id128::new(n.to_be_bytes()).to_base64url()
}
fn check(label: &str, ok: bool) -> R<()> {
    println!("{} {label}", if ok { "PASS" } else { "FAIL" });
    if ok {
        Ok(())
    } else {
        Err(label.to_owned().into())
    }
}
async fn exec(db: &SqlitePool, sql: &str) -> R<()> {
    sqlx::raw_sql(sql).execute(db).await?;
    Ok(())
}
async fn num(db: &SqlitePool, sql: &str) -> R<i64> {
    Ok(sqlx::query_scalar(sql).fetch_one(db).await?)
}
async fn snapshot(db: &SqlitePool) -> R<Value> {
    let clients: Vec<(String, String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT client_id,status,auth_generation,revoked_at FROM clients ORDER BY client_id",
    )
    .fetch_all(db)
    .await?;
    Ok(
        json!({"clients":clients,"revision":num(db,"SELECT revision FROM node_state").await?,"audit":num(db,"SELECT count(*) FROM security_audit").await?,"results":num(db,"SELECT count(*) FROM management_commands").await?}),
    )
}
async fn request(a: &LocalIpcAddress, req: &str, command: &str, payload: Value) -> R<Value> {
    let mut pipe = loop {
        match ClientOptions::new().open(&a.pipe_name) {
            Ok(p) => break p,
            Err(e) if e.raw_os_error() == Some(231) => {
                tokio::time::sleep(Duration::from_millis(10)).await
            }
            Err(e) => return Err(e.into()),
        }
    };
    let data =
        json!({"v":1,"requestId":req,"command":command,"payload":payload}).to_string() + "\n";
    pipe.write_all(data.as_bytes()).await?;
    let mut line = String::new();
    BufReader::new(pipe).read_line(&mut line).await?;
    let v: Value = serde_json::from_str(&line)?;
    check(
        "IPC response version/request correlation",
        v["v"] == 1 && v["requestId"] == req,
    )?;
    Ok(v)
}
async fn revoke(a: &LocalIpcAddress, r: u128, c: u128) -> R<Value> {
    request(a, &id(r), "device.revoke", json!({"clientId":id(c)})).await
}
async fn post(
    addr: std::net::SocketAddr,
    tls: Arc<rustls::ClientConfig>,
    endpoint: &str,
    body: Value,
) -> R<Value> {
    let stream = TcpStream::connect(addr).await?;
    let mut stream = tokio_rustls::TlsConnector::from(tls)
        .connect("localhost".try_into()?, stream)
        .await?;
    let body = body.to_string();
    let raw = format!(
        "POST /api/access/v1/pair/{endpoint} HTTP/1.1\r\nHost: localhost\r\nOrigin: https://localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(raw.as_bytes()).await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    let response = String::from_utf8(response)?;
    let (head, body) = response.split_once("\r\n\r\n").ok_or("HTTP separator")?;
    check("TLS pair HTTP 200", head.starts_with("HTTP/1.1 200"))?;
    Ok(serde_json::from_str(body)?)
}
async fn approval_probe(
    db: &SqlitePool,
    a: &LocalIpcAddress,
    addr: std::net::SocketAddr,
    tls: Arc<rustls::ClientConfig>,
    deferred: bool,
) -> R<()> {
    let label = if deferred {
        "NA007 actual COMMIT failure"
    } else {
        "NA007 precommit statement failure"
    };
    let (mut session, start) = ipc::begin_pairing(a).await?;
    let lookup = post(
        addr,
        tls.clone(),
        "lookup",
        json!({"v":1,"code":start.code,"requestId":id(if deferred {102} else {101})}),
    )
    .await?;
    let seed = if deferred { 32 } else { 31 };
    let key = SigningKey::from_slice(&[seed; 32])?;
    let public = URL_SAFE_NO_PAD.encode(key.verifying_key().to_sec1_point(false).as_bytes());
    let mut claim = lookup.as_object().ok_or("lookup object")?.clone();
    claim.remove("nodePublicKey");
    claim.remove("expiresAt");
    claim.insert("clientPublicKey".into(), json!(public));
    claim.insert("displayName".into(), json!("storage-acceptance"));
    let proof = acp_remote::jws::sign(
        acp_remote::jws::JwsType::PairClaim,
        &serde_json::to_vec(&claim)?,
        &key,
    )?;
    post(addr, tls, "claim", json!({"v":1,"proof":proof})).await?;
    let event = session.next_event().await?;
    check(
        "pair pending_confirmation",
        event.event == "pending_confirmation",
    )?;
    let before = snapshot(db).await?;
    install_fault(
        db,
        if deferred { "commit" } else { "statement" },
        "device.pair.approve",
    )
    .await?;
    session.approve().await?;
    let event = session.next_event().await;
    check(
        label,
        matches!(event, Err(acp_remote::error::AppError::Ipc(ref code)) if code == "storage_unavailable"),
    )?;
    check(
        &format!("{label}: clients/revision/audit/results unchanged"),
        snapshot(db).await? == before,
    )?;
    remove_fault(db).await?;
    drop(session);
    Ok(())
}
async fn install_fault(db: &SqlitePool, kind: &str, action: &str) -> R<()> {
    if kind == "commit" {
        exec(db,"CREATE TABLE probe_parent(id INTEGER PRIMARY KEY); CREATE TABLE probe_deferred(id INTEGER REFERENCES probe_parent(id) DEFERRABLE INITIALLY DEFERRED)").await?;
        exec(db,&format!("CREATE TRIGGER probe_failure AFTER INSERT ON security_audit WHEN NEW.action='{action}' BEGIN INSERT INTO probe_deferred VALUES(999); END")).await?;
    } else {
        exec(db,&format!("CREATE TRIGGER probe_failure BEFORE INSERT ON security_audit WHEN NEW.action='{action}' BEGIN SELECT RAISE(ABORT,'independent probe'); END")).await?;
    }
    Ok(())
}
async fn remove_fault(db: &SqlitePool) -> R<()> {
    exec(db,"DROP TRIGGER probe_failure; DROP TABLE IF EXISTS probe_deferred; DROP TABLE IF EXISTS probe_parent").await
}
fn assert_private_acl(path: &Path, directory: bool) -> R<()> {
    // Inspect the effective descriptor independently through Windows Get-Acl.
    // Paths/SIDs are environment values, never interpolated into PowerShell source.
    let script = r#"$ErrorActionPreference='Stop';
        $a=Get-Acl -LiteralPath $env:PROBE_ACL_PATH;
        $sid=[System.Security.Principal.SecurityIdentifier];
        $rules=@($a.GetAccessRules($true,$true,$sid));
        if(-not $a.AreAccessRulesProtected -or $a.GetOwner($sid).Value -ne $env:PROBE_ACL_SID -or $rules.Count -ne 1){exit 1};
        $r=$rules[0];
        if($r.IdentityReference.Value -ne $env:PROBE_ACL_SID -or $r.AccessControlType.ToString() -ne 'Allow' -or $r.IsInherited -or $r.FileSystemRights -ne [System.Security.AccessControl.FileSystemRights]::FullControl){exit 2};
        if($env:PROBE_ACL_DIRECTORY -eq 'true' -and [int]$r.InheritanceFlags -ne 3){exit 3};
        exit 0"#;
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("PROBE_ACL_PATH", path)
        .env("PROBE_ACL_SID", current_user_sid()?)
        .env("PROBE_ACL_DIRECTORY", directory.to_string())
        .output()?;
    check(
        if directory {
            "P01 root protected owner-only inheritable FullControl DACL before secrets"
        } else {
            "P01 TLS private key protected owner-only FullControl DACL"
        },
        status.status.success(),
    )
}
async fn run(root: &Path, mode: &str, cleanup_safe: &mut bool) -> R<()> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    std::fs::write(root.join("server.pem"), cert.cert.pem())?;
    std::fs::write(
        root.join("server-key.pem"),
        cert.signing_key.serialize_pem(),
    )?;
    protect_private_file(&root.join("server-key.pem"))?;
    assert_private_acl(&root.join("server-key.pem"), false)?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.cert.der().clone())?;
    let tls = Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    );
    let reservation = std::net::TcpListener::bind("127.0.0.1:0")?;
    let listen = reservation.local_addr()?;
    drop(reservation);
    let config = NodeConfig {
        data_dir: root.join("data"),
        listen,
        canonical_origin: "https://localhost".into(),
        tls_cert: root.join("server.pem"),
        tls_key: root.join("server-key.pem"),
    };
    let runtime = NodeRuntime::start(config, &DpapiProtector).await?;
    *cleanup_safe = false;
    let address = runtime.ipc_address().clone();
    let listen = runtime.listen_addr();
    let mut node = tokio::spawn(runtime.wait_for_stop());
    let result=async {
        let db=SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(root.join("data/node.sqlite3")).foreign_keys(true)).await?;
        let budget=if mode=="timeout" {Duration::from_millis(20)} else {Duration::from_secs(180)};
        let outcome=tokio::time::timeout(budget, async {
        if mode=="failure" { return Err("P01 injected failure after live node and database acquisition".into()); }
        if mode=="timeout" { std::future::pending::<()>().await; }
        // Prove the injected constraint is deferred: statement succeeds, COMMIT itself fails.
        install_fault(&db,"commit","probe.control").await?;
        let mut tx=db.begin().await?;
        sqlx::query("INSERT INTO security_audit(action,outcome,occurred_at) VALUES ('probe.control','test',0)").execute(&mut *tx).await?;
        check("control: deferred trigger statement succeeds",true)?;
        let error=tx.commit().await.err().ok_or("control commit unexpectedly succeeded")?;
        check("control: actual COMMIT foreign-key error 787",matches!(&error,sqlx::Error::Database(e) if e.code().as_deref()==Some("787")))?;
        check("control: failed COMMIT rolled back audit",num(&db,"SELECT count(*) FROM security_audit").await?==0)?;
        remove_fault(&db).await?;
        approval_probe(&db,&address,listen,tls.clone(),false).await?;
        approval_probe(&db,&address,listen,tls,true).await?;
        for c in [1u128,2] {
            let key=SigningKey::from_slice(&[c as u8;32])?;
            sqlx::query("INSERT INTO clients VALUES(?,?,?,'trusted',0,1700000000,NULL)").bind(id(c)).bind(key.verifying_key().to_sec1_point(false).as_bytes()).bind("isolated fixture").execute(&db).await?;
        }
        for kind in ["statement","commit"] {
            install_fault(&db,kind,"device.revoke").await?;
            let before=snapshot(&db).await?;
            let response=revoke(&address,200,1).await?;
            check(&format!("NA027 {kind}: storage_unavailable, no success"),response["ok"]==false && response["error"]=="storage_unavailable")?;
            check(&format!("NA027 {kind}: complete rollback"),snapshot(&db).await?==before)?;
            remove_fault(&db).await?;
        }
        let first=revoke(&address,200,1).await?;
        check("NA027 retry failed request succeeds after fault removal",first["ok"]==true && first["result"]["status"]=="revoked")?;
        let before=snapshot(&db).await?;
        check("NA027 same request original response",revoke(&address,200,1).await?==first)?;
        check("NA027 same request no extra audit/revision",snapshot(&db).await?==before)?;
        check("NA027 same request different payload conflict",revoke(&address,200,2).await?["error"]=="request_conflict")?;
        let (digest,expiry):(String,i64)=sqlx::query_as("SELECT payload_digest,expires_at FROM management_commands WHERE request_id=?").bind(id(200)).fetch_one(&db).await?;
        let canonical=format!("{{\"action\":\"device.revoke\",\"payload\":{{\"clientId\":\"{}\"}}}}",id(1));
        check("NA027 digest matches independent closed RFC8785 bytes",digest==hex::encode(Sha256::digest(canonical.as_bytes())))?;
        let now=SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        check("NA027 expiry stored at now+86400 (+/-5 seconds)",(expiry-now-86400).abs()<=5)?;
        let before=snapshot(&db).await?;
        for _ in 0..5 {ipc::device_list(&address,None).await?; ipc::status(&address).await?;}
        check("NA027 list/status no per-query result or audit",snapshot(&db).await?==before)?;
        // Fill isolated DB with results via SQL, preserving the genuine result and its digest.
        sqlx::query("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<9998) INSERT INTO management_commands SELECT 'fixture-'||x,'request-'||x,'device.revoke','fixture','{}',? FROM n").bind(now+86400).execute(&db).await?;
        check("NA027 9999 results boundary fixture",num(&db,"SELECT count(*) FROM management_commands").await?==9999)?;
        let old_revision=num(&db,"SELECT revision FROM node_state").await?;
        let old_audit=num(&db,"SELECT count(*) FROM security_audit").await?;
        check("NA027 9999 admits new request for already revoked ID",revoke(&address,202,1).await?["ok"]==true)?;
        check("NA027 repeat revoke new request one audit, unchanged revision",num(&db,"SELECT count(*) FROM security_audit").await?==old_audit+1 && num(&db,"SELECT revision FROM node_state").await?==old_revision)?;
        check("NA027 exactly 10000 retained results",num(&db,"SELECT count(*) FROM management_commands").await?==10000)?;
        let before=snapshot(&db).await?;
        check("NA027 full budget rejects new revoke",revoke(&address,201,2).await?["error"]=="storage_unavailable")?;
        check("NA027 full budget no eviction/state/audit changes",snapshot(&db).await?==before)?;
        check("NA027 full budget preserves genuine replay",revoke(&address,200,1).await?==first)?;
        check("NA027 full budget preserves payload conflict",revoke(&address,200,2).await?["error"]=="request_conflict")?;
        sqlx::query("UPDATE management_commands SET expires_at=? WHERE actor_id='fixture-1'").bind(now-1).execute(&db).await?;
        check("NA027 expired slot admits one new revoke",revoke(&address,201,2).await?["ok"]==true)?;
        check("NA027 expired slot pruning keeps count 10000",num(&db,"SELECT count(*) FROM management_commands").await?==10000)?;
        check("NA027 unexpired genuine result not evicted",num(&db,"SELECT count(*) FROM management_commands WHERE actor_id='local-owner' AND request_id='AAAAAAAAAAAAAAAAAAAAyA'").await?==1)?;
        sqlx::query("UPDATE management_commands SET expires_at=? WHERE actor_id='local-owner' AND request_id=?").bind(now-1).bind(id(200)).execute(&db).await?;
        check("NA027 expired request changed payload accepted",revoke(&address,200,2).await?["ok"]==true)?;
        let (status,generation):(String,i64)=sqlx::query_as("SELECT status,auth_generation FROM clients WHERE client_id=?").bind(id(1)).fetch_one(&db).await?;
        check("NA027 expired ID does not revive first identity",status=="revoked" && generation==1)?;
        check("NA027 repeated revoke does not increment generation",num(&db,"SELECT auth_generation FROM clients WHERE client_id='AAAAAAAAAAAAAAAAAAAAAg'").await?==1)?;
        println!("FINAL_STATE {}",snapshot(&db).await?);
        Ok::<(),Box<dyn Error+Send+Sync>>(())
        }).await;
        // Retain pool ownership outside the cancellable body, close on error and timeout too.
        db.close().await;
        check("P01 cleanup: external SQLite pool explicitly closed", db.is_closed())?;
        match outcome {Ok(result)=>result,Err(_)=>Err("P01 injected/body timeout".into())}
    }.await;
    // The timeout cancels only the body. Its resource owner always stops/joins the node.
    let stop = tokio::time::timeout(Duration::from_secs(20), ipc::stop(&address)).await;
    let joined = tokio::time::timeout(Duration::from_secs(25), &mut node).await;
    if joined.is_err() {
        node.abort();
        let _ = node.await;
    }
    let stopped = matches!(stop, Ok(Ok(_)));
    let joined_ok = matches!(joined, Ok(Ok(Ok(()))));
    if !stopped || !joined_ok {
        return Err("P01 shutdown unconfirmed; preserve protected data-run for inspection".into());
    }
    *cleanup_safe = true;
    check("cleanup: owner stop succeeds", stopped)?;
    check("cleanup: runtime joins successfully", joined_ok)?;
    check(
        "P01 cleanup: listener released",
        std::net::TcpListener::bind(listen).is_ok(),
    )?;
    check(
        "P01 cleanup: owner pipe no longer available",
        ipc::status(&address).await.is_err(),
    )?;
    let lock = acp_remote::data_dir::DataDirectory::acquire(&root.join("data"))?;
    lock.unlock()?;
    drop(lock);
    check(
        "P01 cleanup: data-directory lock reacquired and explicitly released",
        true,
    )?;
    result
}
#[tokio::main]
async fn main() -> R<()> {
    let root = std::env::current_dir()?.join("data-run");
    if root.exists() {
        return Err("data-run exists; do not overwrite".into());
    }
    std::fs::create_dir(&root)?;
    // Do not generate or write any secret until protection and its assertion succeed.
    protect_private_directory(&root)?;
    assert_private_acl(&root, true)?;
    let mode = std::env::args().nth(1).unwrap_or_else(|| "normal".into());
    if !["normal", "failure", "timeout"].contains(&mode.as_str()) {
        return Err("expected normal/failure/timeout".into());
    }
    let mut cleanup_safe = true;
    let result = run(&root, &mode, &mut cleanup_safe).await;
    if !cleanup_safe {
        return Err(format!(
            "{}; protected run directory retained: {}",
            result
                .err()
                .map_or_else(|| "shutdown unconfirmed".into(), |e| e.to_string()),
            root.display()
        )
        .into());
    }
    // Only this freshly created directory is removed; never touch other data.
    std::fs::remove_dir_all(&root).map_err(|e| {
        format!(
            "cleanup failed; protected run directory retained at {}: {e}",
            root.display()
        )
    })?;
    check(
        "cleanup: isolated generated keys/cert/database removed",
        !root.exists(),
    )?;
    result?;
    println!("PROBE COMPLETE");
    Ok(())
}
