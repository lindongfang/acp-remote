#![cfg(windows)]
#![allow(
    clippy::similar_names,
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]

use acp_remote::{
    adapters::acp::{
        ConfigSourceStamp, Connection, ExtensionFields, FenceRejection, Incoming, Known, PollIo,
        Response, ResponseDeadline, SessionSetup, SetupSnapshot, Version, WriteFence,
        WriteIdentity, WriteReport, WriteStep,
    },
    platform::process_owner::{OwnedProcess, ProcessSpec},
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    task::Context,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    time::{Instant, timeout},
};

struct AllowFence;

impl WriteFence for AllowFence {
    fn check_and_poll(
        &self,
        _: &WriteIdentity,
        _: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let _ = io.poll(cx);
        Ok(())
    }
}

fn find_node() -> PathBuf {
    for candidate in [
        PathBuf::from(r"D:\Application\Node\nodejs\node.exe"),
        PathBuf::from(r"C:\Program Files\nodejs\node.exe"),
    ] {
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("node.exe")
}

fn find_codex_exe() -> PathBuf {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let candidate = PathBuf::from(local_app_data).join(r"Programs\OpenAI\Codex\bin\codex.exe");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("codex.exe")
}

fn find_codex_acp_js() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .join(r"testing\probes\codex-session-loop\node_modules\@agentclientprotocol\codex-acp\dist\index.js")
}

#[tokio::test]
#[ignore = "requires local node, codex, and codex-acp"]
async fn real_codex_acp_v2_unauthenticated_initialize_and_job_shutdown() {
    let node = find_node();
    let codex = find_codex_exe();
    let adapter = find_codex_acp_js();
    assert!(node.is_file(), "node.exe not found at {}", node.display());
    assert!(
        codex.is_file(),
        "codex.exe not found at {}",
        codex.display()
    );
    assert!(
        adapter.is_file(),
        "codex-acp entrypoint not found at {}",
        adapter.display()
    );

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root");
    let run_root = repo_root
        .join(".tmp")
        .join(format!("codex-acp-rust-probe-{}", std::process::id()));
    let codex_home = run_root.join("codex-home");
    let workspace = run_root.join("workspace");
    let _ = std::fs::remove_dir_all(&run_root);
    std::fs::create_dir_all(&codex_home).expect("create codex home");
    std::fs::create_dir_all(&workspace).expect("create workspace");

    let mut owner = OwnedProcess::spawn(
        ProcessSpec {
            executable: node,
            args: vec![adapter.into_os_string()],
            cwd: workspace.clone(),
            env: vec![
                (OsString::from("CODEX_HOME"), codex_home.into_os_string()),
                (OsString::from("CODEX_PATH"), codex.into_os_string()),
                (OsString::from("NO_BROWSER"), OsString::from("1")),
                (
                    OsString::from("INITIAL_AGENT_MODE"),
                    OsString::from("read-only"),
                ),
            ],
        },
        Instant::now() + Duration::from_secs(20),
    )
    .await
    .expect("spawn codex-acp via OwnedProcess");

    let io = owner.take_io().expect("take stdio");
    let mut stderr = io.stderr;
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf).await;
        String::from_utf8_lossy(&buf).into_owned()
    });

    let (connection, mut events, acp_owner) =
        Connection::spawn(io.stdout, io.stdin).expect("spawn ACP connection");

    let init_res = connection
        .reserve_preferred_initialize(Instant::now() + Duration::from_secs(20))
        .expect("reserve initialize");
    let init_frame = init_res.encode().expect("encode initialize");
    let init_ticket = connection
        .enqueue(
            init_res,
            init_frame,
            Arc::new(AllowFence),
            Instant::now() + Duration::from_secs(5),
        )
        .expect("enqueue initialize");
    assert_eq!(
        timeout(Duration::from_secs(5), init_ticket.wait())
            .await
            .expect("init write finish"),
        WriteReport::Written
    );

    let strategy = loop {
        let event = timeout(Duration::from_secs(20), events.next())
            .await
            .expect("init event timeout")
            .expect("init event stream")
            .expect("init event present");
        if let Incoming::Response {
            response: Response::Initialized(strategy),
            ..
        } = event.incoming()
        {
            break strategy.clone();
        }
    };
    assert_eq!(strategy.version(), Version::V2);
    let negotiated = strategy.negotiated();
    assert_eq!(negotiated.offered, Version::V2);
    assert_eq!(negotiated.version, Version::V2);
    assert!(negotiated.capabilities.sessions);
    assert!(negotiated.capabilities.list);
    assert!(negotiated.capabilities.resume);
    assert!(negotiated.capabilities.close);
    let info_name = negotiated
        .raw
        .field("result")
        .expect("result field")
        .and_then(acp_remote::adapters::acp::raw_json::RawValueRef::object)
        .and_then(|o| o.get("info").ok().flatten())
        .and_then(acp_remote::adapters::acp::raw_json::RawValueRef::object)
        .and_then(|o| o.get("name").ok().flatten())
        .and_then(|n| n.string().ok());
    eprintln!("negotiated ACP v2 info.name = {info_name:?}");

    // Verify session/list on empty CODEX_HOME
    let list_req = strategy
        .list_sessions(
            Some(&workspace.to_string_lossy()),
            None,
            ExtensionFields::empty().expect("empty ext"),
        )
        .expect("build list_sessions");
    let list_res = connection
        .reserve_request(
            list_req,
            ResponseDeadline::At(Instant::now() + Duration::from_secs(20)),
        )
        .expect("reserve list_sessions");
    let list_frame = list_res.encode().expect("encode list_sessions");
    let list_ticket = connection
        .enqueue(
            list_res,
            list_frame,
            Arc::new(AllowFence),
            Instant::now() + Duration::from_secs(5),
        )
        .expect("enqueue list_sessions");
    assert_eq!(
        timeout(Duration::from_secs(5), list_ticket.wait())
            .await
            .expect("list write finish"),
        WriteReport::Written
    );

    let list_event = timeout(Duration::from_secs(20), events.next())
        .await
        .expect("list event timeout")
        .expect("list event ok")
        .expect("list event present");
    match list_event.incoming() {
        Incoming::Response {
            response: Response::Sessions(listing),
            ..
        } => {
            assert!(listing.sessions().is_empty());
            eprintln!("session/list returned 0 sessions (Sessions)");
        }
        Incoming::Response {
            response: Response::DeferredSessions(strat),
            ..
        } => {
            let result = list_event
                .raw()
                .field("result")
                .expect("raw result")
                .expect("result present");
            let listing = strat.session_listing(result).expect("decode listing");
            assert!(listing.sessions().is_empty());
            eprintln!("session/list returned 0 sessions (DeferredSessions)");
        }
        Incoming::Response {
            response: Response::RemoteError { code },
            ..
        } => {
            eprintln!("session/list returned remote error code={code}");
        }
        other => panic!("unexpected list response: {other:?}"),
    }

    // Verify session/new on unauthenticated empty CODEX_HOME (should return RemoteError, not protocol fault)
    let setup = SessionSetup::try_from_snapshot(SetupSnapshot {
        wire: Version::V2,
        source: ConfigSourceStamp {
            reference: "probe".into(),
            revision: 1,
        },
        cwd: Known::Known(workspace.to_string_lossy().into_owned().into()),
        mcp_servers: Known::Known(Vec::new()),
        additional_directories: Known::Known(Vec::new()),
        extra: ExtensionFields::empty().expect("empty ext"),
    })
    .expect("setup snapshot");
    let new_req = strategy.new_session(&setup).expect("build new_session");
    let new_res = connection
        .reserve_request(
            new_req,
            ResponseDeadline::At(Instant::now() + Duration::from_secs(20)),
        )
        .expect("reserve new_session");
    let new_frame = new_res.encode().expect("encode new_session");
    let new_ticket = connection
        .enqueue(
            new_res,
            new_frame,
            Arc::new(AllowFence),
            Instant::now() + Duration::from_secs(5),
        )
        .expect("enqueue new_session");
    assert_eq!(
        timeout(Duration::from_secs(5), new_ticket.wait())
            .await
            .expect("new write finish"),
        WriteReport::Written
    );
    let new_event = timeout(Duration::from_secs(20), events.next())
        .await
        .expect("new event timeout")
        .expect("new event ok")
        .expect("new event present");
    match new_event.incoming() {
        Incoming::Response {
            response: Response::RemoteError { code },
            ..
        } => {
            eprintln!("unauthenticated session/new returned RemoteError code={code}");
        }
        Incoming::Response {
            response: Response::NewSession(session),
            ..
        } => {
            eprintln!("unexpectedly succeeded without auth: {}", session.as_str());
        }
        other => panic!("unexpected new_session response: {other:?}"),
    }

    let io_shutdown = acp_owner
        .shutdown(Instant::now() + Duration::from_secs(5))
        .await;
    assert!(io_shutdown.joined, "ACP IO tasks must join cleanly");

    let proc_shutdown = owner
        .shutdown(Instant::now() + Duration::from_secs(5))
        .await
        .expect("process tree shutdown");
    assert_eq!(
        proc_shutdown.active_processes, 0,
        "Windows Job must terminate entire node + codex process tree"
    );

    let _ = stderr_task.await;
    let _ = std::fs::remove_dir_all(&run_root);
}
