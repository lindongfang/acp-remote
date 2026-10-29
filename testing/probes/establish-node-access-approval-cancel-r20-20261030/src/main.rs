use acp_remote::ipc::LocalIpcAddress;
use serde_json::{Value, json};
use std::{
    error::Error,
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer},
    sync::watch,
    time::{Instant, timeout},
};
use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const ROOT: &str = "D:/Project/acp-remote/.tmp/approval-cancel-boundary-r20-20261030";
async fn frame<R: AsyncRead + Unpin>(r: &mut R) -> Result<Vec<u8>> {
    let mut b = Vec::new();
    loop {
        let x = r.read_u8().await?;
        b.push(x);
        if x == b'\n' {
            return Ok(b);
        }
        if b.len() > 32 * 1024 {
            return Err("IPC frame budget".into());
        }
    }
}
async fn forward<W: AsyncWrite + Unpin>(w: &mut W, b: &[u8]) -> Result<()> {
    w.write_all(b).await?;
    w.flush().await?;
    Ok(())
}
fn state(path: &Path, s: &mut Value, stage: &str) -> Result<()> {
    s["stage"] = json!(stage);
    std::fs::write(path, serde_json::to_vec_pretty(s)?)?;
    Ok(())
}
fn backend_client(address: &LocalIpcAddress, pid: u32) -> Result<NamedPipeClient> {
    let c = ClientOptions::new().open(&address.pipe_name)?;
    let mut actual = 0;
    let ok = unsafe { GetNamedPipeServerProcessId(c.as_raw_handle(), &mut actual) };
    if ok == 0 || actual != pid {
        return Err("not the owned backend PID".into());
    }
    Ok(c)
}
async fn release<R: AsyncRead + Unpin>(r: &mut R, flag: &Path) -> Result<bool> {
    let end = Instant::now() + Duration::from_secs(50);
    loop {
        if flag.exists() {
            return Ok(true);
        }
        tokio::select! {x=r.read_u8()=>return match x{Err(e) if e.kind()==std::io::ErrorKind::UnexpectedEof=>Ok(false),Err(e)=>Err(e.into()),Ok(_)=>Err("unexpected bytes during delay".into())},()=tokio::time::sleep(Duration::from_millis(25))=>{},()=tokio::time::sleep_until(end)=>return Err("hold deadline".into())}
    }
}
async fn pair_session(
    pipe: NamedPipeServer,
    client: NamedPipeClient,
    case: &str,
    dir: &Path,
    s: &mut Value,
) -> Result<()> {
    let path = dir.join("proxy-state.json");
    let (mut fr, mut fw) = tokio::io::split(pipe);
    let (mut br, mut bw) = tokio::io::split(client);
    let first = frame(&mut fr).await?;
    let request: Value = serde_json::from_slice(&first)?;
    if request["v"] != 1 || request["command"] != "device.pair" {
        return Err("first connection must be real CLI device.pair".into());
    }
    forward(&mut bw, &first).await?;
    loop {
        let bytes = frame(&mut br).await?;
        let response: Value = serde_json::from_slice(&bytes)?;
        let event = response["result"]["event"].as_str().unwrap_or("");
        forward(&mut fw, &bytes).await?;
        if event == "pending_confirmation" {
            s["pendingFingerprint"] = response["result"]["fingerprint"].clone();
            state(&path, s, "pending_forwarded")?;
            break;
        }
    }
    let decision = match frame(&mut fr).await {
        Ok(x) => x,
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|x| x.kind() == std::io::ErrorKind::UnexpectedEof) =>
        {
            s["clientDisconnected"] = json!(true);
            state(&path, s, "cancelled_before_decision")?;
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let d: Value = serde_json::from_slice(&decision)?;
    if d["decision"] != "approve" {
        return Err("expected real CLI approve".into());
    }
    if case == "precommit" {
        state(&path, s, "decision_held")?;
        if !release(&mut fr, &dir.join("release.flag")).await? {
            s["clientDisconnected"] = json!(true);
            state(&path, s, "cancelled_before_forward")?;
            return Ok(());
        }
    }
    forward(&mut bw, &decision).await?;
    s["decisionForwarded"] = json!(true);
    state(&path, s, "decision_forwarded")?;
    let bytes = frame(&mut br).await?;
    let response: Value = serde_json::from_slice(&bytes)?;
    if response["result"]["event"] != "approved" {
        return Err("expected real backend approved result".into());
    }
    s["clientId"] = response["result"]["clientId"].clone();
    if case == "committed" {
        s["approvedHeld"] = json!(true);
        state(&path, s, "approved_held")?;
        if !release(&mut fr, &dir.join("release.flag")).await? {
            s["clientDisconnected"] = json!(true);
            state(&path, s, "client_disconnected_after_commit")?;
            return Ok(());
        }
    }
    forward(&mut fw, &bytes).await?;
    s["approvedDelivered"] = json!(true);
    state(&path, s, "done")?;
    Ok(())
}
async fn queries(
    front: LocalIpcAddress,
    back: LocalIpcAddress,
    pid: u32,
    mut pipe: NamedPipeServer,
    dir: PathBuf,
    mut done: watch::Receiver<bool>,
) -> Result<()> {
    let path = dir.join("proxy-query-state.json");
    let mut log = json!({"rows":[],"limit":8,"perQueryDeadlineMs":3000,"afterPairDeadlineMs":6000});
    state(&path, &mut log, "listening")?;
    let hard = Instant::now() + Duration::from_secs(120);
    let mut finish = None;
    let mut done_seen = false;
    loop {
        let deadline = finish.unwrap_or(hard).min(hard);
        tokio::select! {
         result=pipe.connect()=>{result?;},
         changed=done.changed(),if !done_seen=>{let _=changed;done_seen=true;finish=Some(Instant::now()+Duration::from_secs(6));continue;},
         ()=tokio::time::sleep_until(deadline)=>{state(&path,&mut log,"closed")?;return Ok(())}
        }
        if log["rows"].as_array().map_or(0, Vec::len) >= 8 {
            return Err("owner query count exceeded".into());
        }
        front.verify_peer(&pipe)?;
        let mut current = pipe;
        pipe = front.bind_next()?;
        let started = Instant::now();
        let result=timeout(Duration::from_secs(3),async{
  let request=frame(&mut current).await?;let parsed:Value=serde_json::from_slice(&request)?;
  if parsed["v"]!=1||parsed["command"]!="device.list"{return Err::<Value,_>("auxiliary connection permits only genuine owner device.list".into())}
  let mut backend=backend_client(&back,pid)?;forward(&mut backend,&request).await?;let response=frame(&mut backend).await?;let parsed_response:Value=serde_json::from_slice(&response)?;forward(&mut current,&response).await?;
  Ok::<Value,Box<dyn Error+Send+Sync>>(json!({"command":"device.list","frontOwnerVerified":true,"backendPidChecked":true,"backendPid":pid,"requestBytes":request.len(),"responseBytes":response.len(),"replyOk":parsed_response["ok"],"itemCount":parsed_response["result"]["items"].as_array().map(Vec::len),"cursorPresent":!parsed_response["result"]["nextCursor"].is_null(),"responseForwarded":true}))
 }).await;
        let mut row = match result {
            Ok(Ok(row)) => row,
            Ok(Err(_)) => json!({"queryError":true,"frontOwnerVerified":true}),
            Err(_) => json!({"queryTimeout":true,"frontOwnerVerified":true}),
        };
        row["elapsedMs"] = json!(started.elapsed().as_millis());
        log["rows"]
            .as_array_mut()
            .ok_or("query log not array")?
            .push(row);
        state(&path, &mut log, "listening")?;
    }
}
#[tokio::main]
async fn main() -> Result<()> {
    let case = std::env::args().nth(1).ok_or("case required")?;
    if !["committed", "precommit", "control"].contains(&case.as_str()) {
        return Err("invalid owned case".into());
    }
    let root = PathBuf::from(ROOT);
    let dir = root.join(&case);
    let path = dir.join("proxy-state.json");
    let back = LocalIpcAddress::for_data_directory(&root.join("data-30455"))?;
    let front = LocalIpcAddress::for_data_directory(&dir.join("proxydata"))?;
    if front.owner_sid != back.owner_sid {
        return Err("owner mismatch".into());
    }
    let pipe = front.bind_first()?;
    let mut s = json!({"case":case,"frontOwnerVerified":false,"backendPidChecked":false,"decisionForwarded":false,"approvedHeld":false,"approvedDelivered":false,"ownerSid":front.owner_sid});
    state(&path, &mut s, "listening")?;
    timeout(Duration::from_secs(90), pipe.connect()).await??;
    front.verify_peer(&pipe)?;
    s["frontOwnerVerified"] = json!(true);
    let pid: u32 = std::fs::read_to_string(root.join("owned-node.pid"))?
        .trim()
        .parse()?;
    let client = backend_client(&back, pid)?;
    s["backendPidChecked"] = json!(true);
    s["backendPid"] = json!(pid);
    // An already-listening additional instance permits R20's genuine post-interrupt owner query.
    let next = front.bind_next()?;
    let (tx, rx) = watch::channel(false);
    let q = tokio::spawn(queries(front, back, pid, next, dir.clone(), rx));
    let paired = pair_session(pipe, client, &case, &dir, &mut s).await;
    tx.send_replace(true);
    let query_result = timeout(Duration::from_secs(9), q).await???;
    let _ = query_result;
    paired
}
