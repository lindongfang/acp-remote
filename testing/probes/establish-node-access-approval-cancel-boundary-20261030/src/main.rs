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
    net::windows::named_pipe::ClientOptions,
};
use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const ROOT: &str = "D:/Project/acp-remote/.tmp/approval-cancel-boundary-20261030";
async fn frame<R: AsyncRead + Unpin>(r: &mut R) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let byte = r.read_u8().await?;
        bytes.push(byte);
        if byte == b'\n' {
            return Ok(bytes);
        }
        if bytes.len() > 32 * 1024 {
            return Err("IPC frame exceeded budget".into());
        }
    }
}
async fn forward<W: AsyncWrite + Unpin>(w: &mut W, b: &[u8]) -> Result<()> {
    w.write_all(b).await?;
    w.flush().await?;
    Ok(())
}
fn state(path: &Path, value: &mut Value, stage: &str) -> Result<()> {
    value["stage"] = json!(stage);
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
async fn wait_release<R: AsyncRead + Unpin>(r: &mut R, release: &Path) -> Result<bool> {
    let timeout = tokio::time::sleep(Duration::from_secs(50));
    tokio::pin!(timeout);
    loop {
        if release.exists() {
            return Ok(true);
        }
        tokio::select! {x=r.read_u8()=>{match x {Ok(_)=>return Err("unexpected client bytes during hold".into()),Err(e) if e.kind()==std::io::ErrorKind::UnexpectedEof=>return Ok(false),Err(e)=>return Err(e.into())}},()=tokio::time::sleep(Duration::from_millis(25))=>{},()=&mut timeout=>return Err("adapter hold deadline".into())}
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
    let backend = LocalIpcAddress::for_data_directory(&root.join("data-30455"))?;
    let frontend = LocalIpcAddress::for_data_directory(&dir.join("proxydata"))?;
    if backend.owner_sid != frontend.owner_sid {
        return Err("owner mismatch".into());
    }
    let pipe = frontend.bind_first()?;
    let mut s = json!({"case":case,"ownerSid":frontend.owner_sid,"frontOwnerVerified":false,"backendPidChecked":false,"decisionForwarded":false,"approvedHeld":false,"approvedDelivered":false});
    state(&path, &mut s, "listening")?;
    tokio::time::timeout(Duration::from_secs(90), pipe.connect()).await??;
    frontend.verify_peer(&pipe)?;
    s["frontOwnerVerified"] = json!(true);
    let client = ClientOptions::new().open(&backend.pipe_name)?;
    let expected: u32 = std::fs::read_to_string(root.join("owned-node.pid"))?
        .trim()
        .parse()?;
    let mut actual = 0;
    let checked = unsafe { GetNamedPipeServerProcessId(client.as_raw_handle(), &mut actual) };
    if checked == 0 || actual != expected {
        return Err("backend pipe does not belong to owned node pid".into());
    }
    s["backendPidChecked"] = json!(true);
    s["backendPid"] = json!(actual);
    let (mut fr, mut fw) = tokio::io::split(pipe);
    let (mut br, mut bw) = tokio::io::split(client);
    let first = frame(&mut fr).await?;
    let request: Value = serde_json::from_slice(&first)?;
    if request["v"] != 1 || request["command"] != "device.pair" {
        return Err("only real CLI device.pair allowed".into());
    }
    forward(&mut bw, &first).await?;
    // Frames stay byte-for-byte in memory; only public phase labels and IDs are recorded.
    loop {
        let bytes = frame(&mut br).await?;
        let response: Value = serde_json::from_slice(&bytes)?;
        let event = response["result"]["event"].as_str().unwrap_or("");
        forward(&mut fw, &bytes).await?;
        if event == "pending_confirmation" {
            state(&path, &mut s, "pending_forwarded")?;
            break;
        }
    }
    let decision = match frame(&mut fr).await {
        Ok(b) => b,
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|x| x.kind() == std::io::ErrorKind::UnexpectedEof) =>
        {
            s["clientDisconnected"] = json!(true);
            state(&path, &mut s, "cancelled_before_decision")?;
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let d: Value = serde_json::from_slice(&decision)?;
    if d["decision"] != "approve" {
        return Err("expected genuine CLI approve decision".into());
    }
    if case == "precommit" {
        state(&path, &mut s, "decision_held")?;
        if !wait_release(&mut fr, &dir.join("release.flag")).await? {
            s["clientDisconnected"] = json!(true);
            state(&path, &mut s, "cancelled_before_forward")?;
            return Ok(());
        }
    }
    forward(&mut bw, &decision).await?;
    s["decisionForwarded"] = json!(true);
    state(&path, &mut s, "decision_forwarded")?;
    let bytes = frame(&mut br).await?;
    let response: Value = serde_json::from_slice(&bytes)?;
    if response["result"]["event"] != "approved" {
        return Err("backend did not return committed approved event".into());
    }
    s["clientId"] = response["result"]["clientId"].clone();
    if case == "committed" {
        s["approvedHeld"] = json!(true);
        state(&path, &mut s, "approved_held")?;
        if !wait_release(&mut fr, &dir.join("release.flag")).await? {
            s["clientDisconnected"] = json!(true);
            state(&path, &mut s, "client_disconnected_after_commit")?;
            return Ok(());
        }
    }
    forward(&mut fw, &bytes).await?;
    s["approvedDelivered"] = json!(true);
    state(&path, &mut s, "done")?;
    Ok(())
}
