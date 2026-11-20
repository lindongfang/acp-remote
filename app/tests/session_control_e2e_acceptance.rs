#![cfg(windows)]

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use acp_remote::ipc::{self, LocalIpcAddress};

struct E2eCleanupGuard {
    root: PathBuf,
    ipc_address: LocalIpcAddress,
    node_child: Option<Child>,
    driver_child: Option<Child>,
}

impl Drop for E2eCleanupGuard {
    fn drop(&mut self) {
        if let Some(mut driver) = self.driver_child.take() {
            let _ = driver.kill();
            let _ = driver.wait();
        }
        if let Some(mut node) = self.node_child.take() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let addr = self.ipc_address.clone();
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        let _ =
                            tokio::time::timeout(Duration::from_secs(6), ipc::stop(&addr)).await;
                    });
                });
            }
            let _ = node.kill();
            let _ = node.wait();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct E2eTempArea {
    temp_root: PathBuf,
    workspace_dir: PathBuf,
    work_dir: PathBuf,
    peer_state_dir: PathBuf,
    config_path: PathBuf,
    origin: String,
    ipc_addr: LocalIpcAddress,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn free_loopback_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback port");
    listener.local_addr().expect("local addr").port()
}

fn toml_escaped(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

fn setup_temp_e2e_area() -> E2eTempArea {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root =
        std::env::temp_dir().join(format!("acp-remote-sct-016-{}-{nanos}", std::process::id()));
    let data_dir = temp_root.join("node-data");
    let workspace_dir = temp_root.join("workspace");
    let work_dir = temp_root.join("driver-work");
    let peer_state_dir = temp_root.join("peer-state");
    fs::create_dir_all(&data_dir).unwrap();
    fs::create_dir_all(&workspace_dir).unwrap();
    fs::create_dir_all(&work_dir).unwrap();
    fs::create_dir_all(&peer_state_dir).unwrap();

    let cert =
        rcgen::generate_simple_self_signed(vec!["localhost".to_owned(), "127.0.0.1".to_owned()])
            .expect("generate self-signed cert");
    let cert_path = temp_root.join("node.crt.pem");
    let key_path = temp_root.join("node.key.pem");
    fs::write(&cert_path, cert.cert.pem()).unwrap();
    fs::write(&key_path, cert.signing_key.serialize_pem()).unwrap();

    let port = free_loopback_port();
    let origin = format!("https://127.0.0.1:{port}");
    let config_path = temp_root.join("config.toml");
    let config_toml = format!(
        "data_dir = \"{}\"\nlisten = \"127.0.0.1:{port}\"\ncanonical_origin = \"{origin}\"\ntls_cert = \"{}\"\ntls_key = \"{}\"\n",
        toml_escaped(&data_dir),
        toml_escaped(&cert_path),
        toml_escaped(&key_path),
    );
    fs::write(&config_path, config_toml).unwrap();

    let ipc_addr = LocalIpcAddress::for_data_directory(&data_dir).expect("IPC address");
    E2eTempArea {
        temp_root,
        workspace_dir,
        work_dir,
        peer_state_dir,
        config_path,
        origin,
        ipc_addr,
    }
}

fn spawn_daemon(bin: &Path, config_path: &Path) -> Child {
    Command::new(bin)
        .arg("--config")
        .arg(config_path)
        .arg("daemon")
        .arg("start")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn acp-remote daemon")
}

async fn wait_node_ready(address: &LocalIpcAddress) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(status) = ipc::status(address).await
            && status.state == "ready"
        {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for acp-remote node ready"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn pair_one_client(address: &LocalIpcAddress, out_file: &Path) {
    let (mut session, start) = ipc::begin_pairing(address)
        .await
        .expect("begin pairing via IPC");
    fs::write(
        out_file,
        serde_json::to_vec(&serde_json::json!({ "code": start.code })).unwrap(),
    )
    .expect("write pairing code file");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for browser pairing claim"
        );
        let ev = session.next_event().await.expect("pairing event");
        if ev.event == "pending_confirmation" {
            session.approve().await.expect("approve browser pairing");
            return;
        }
    }
}

async fn wait_file_present(path: &Path, timeout_secs: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    while !path.is_file() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "PV4 L6 E2E: run after npm run build:rust"]
async fn sct_016_real_binary_dual_browser_takeover_and_restart_e2e() {
    let repo = repo_root();
    let area = setup_temp_e2e_area();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_acp-remote"));
    assert!(
        bin.is_file(),
        "missing acp-remote binary at {}",
        bin.display()
    );

    let mut guard = E2eCleanupGuard {
        root: area.temp_root.clone(),
        ipc_address: area.ipc_addr.clone(),
        node_child: Some(spawn_daemon(&bin, &area.config_path)),
        driver_child: None,
    };
    wait_node_ready(&area.ipc_addr).await;

    let evidence_out =
        repo.join("testing/runs/deliver-session-control-takeover/coder/sct-016-e2e.json");
    if let Some(parent) = evidence_out.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let driver_cfg_path = area.temp_root.join("driver-config.json");
    let peer_script = repo.join("clients/web/tests/e2e/controlled-acp-v2-peer.mjs");
    let driver_script = repo.join("clients/web/tests/e2e/sct-016-browser-driver.mjs");
    fs::write(
        &driver_cfg_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "origin": area.origin,
            "workspaceDir": area.workspace_dir,
            "peerScript": peer_script,
            "peerStateDir": area.peer_state_dir,
            "workDir": area.work_dir,
            "evidenceOut": evidence_out,
        }))
        .unwrap(),
    )
    .unwrap();

    guard.driver_child = Some(
        Command::new("node")
            .arg(&driver_script)
            .arg("--config")
            .arg(&driver_cfg_path)
            .current_dir(&repo)
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn Playwright sct-016 browser driver"),
    );

    pair_one_client(&area.ipc_addr, &area.work_dir.join("pairing-a.json")).await;
    wait_file_present(&area.work_dir.join("pair-a-done.json"), 60).await;
    pair_one_client(&area.ipc_addr, &area.work_dir.join("pairing-b.json")).await;

    wait_file_present(&area.work_dir.join("request-restart.json"), 90).await;
    ipc::stop(&area.ipc_addr)
        .await
        .expect("ipc stop before restart");
    if let Some(mut first_node) = guard.node_child.take() {
        let status = tokio::task::block_in_place(|| first_node.wait()).expect("wait first daemon");
        assert!(status.success(), "first daemon exit status: {status}");
    }
    guard.node_child = Some(spawn_daemon(&bin, &area.config_path));
    wait_node_ready(&area.ipc_addr).await;
    fs::write(area.work_dir.join("restart-done.json"), b"{}").unwrap();

    let mut driver = guard.driver_child.take().unwrap();
    let driver_status = tokio::task::block_in_place(|| driver.wait()).expect("wait browser driver");
    assert!(
        driver_status.success(),
        "browser driver failed: {driver_status}"
    );

    let mut report: serde_json::Value =
        serde_json::from_slice(&fs::read(&evidence_out).expect("read e2e evidence")).unwrap();
    report["realCodexHarness"] = serde_json::json!({
        "status": "not-run",
        "reason": "No dedicated authenticated real Codex CLI environment configured; full dual-browser workbench & real acp-remote.exe loop verified against controlled ACP V2 peer"
    });
    fs::write(&evidence_out, serde_json::to_vec_pretty(&report).unwrap()).unwrap();

    ipc::stop(&area.ipc_addr).await.expect("ipc stop after E2E");
    if let Some(mut second_node) = guard.node_child.take() {
        let status =
            tokio::task::block_in_place(|| second_node.wait()).expect("wait second daemon");
        assert!(status.success(), "second daemon exit status: {status}");
    }
    let root_check = area.temp_root.clone();
    drop(guard);
    assert!(
        !root_check.exists(),
        "temp root must be removed: {}",
        root_check.display()
    );
}

fn prepare_real_codex_env(repo: &Path, temp_root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let codex_adapter = repo.join(
        r"testing\probes\codex-session-loop\node_modules\@agentclientprotocol\codex-acp\dist\index.js",
    );
    assert!(
        codex_adapter.is_file(),
        "missing codex-acp adapter at {}",
        codex_adapter.display()
    );
    let codex_path = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|base| base.join(r"Programs\OpenAI\Codex\bin\codex.exe"))
        .filter(|path| path.is_file())
        .expect(
            "missing local codex.exe under %LOCALAPPDATA%\\Programs\\OpenAI\\Codex\\bin\\codex.exe",
        );
    let user_auth = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|home| home.join(".codex").join("auth.json"))
        .filter(|path| path.is_file())
        .expect("missing ~/.codex/auth.json for real Codex verification");
    let codex_home = temp_root.join("codex-home");
    fs::create_dir_all(&codex_home).unwrap();
    fs::copy(&user_auth, codex_home.join("auth.json")).expect("copy isolated auth.json");
    (codex_adapter, codex_path, codex_home)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "PV4 L6 real Codex E2E: requires local codex.exe, codex-acp, and user auth.json"]
async fn sct_016_real_codex_harness_dual_browser_takeover_and_restart_e2e() {
    let repo = repo_root();
    let area = setup_temp_e2e_area();
    let (codex_adapter, codex_path, codex_home) = prepare_real_codex_env(&repo, &area.temp_root);
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_acp-remote"));
    assert!(
        bin.is_file(),
        "missing acp-remote binary at {}",
        bin.display()
    );

    let mut guard = E2eCleanupGuard {
        root: area.temp_root.clone(),
        ipc_address: area.ipc_addr.clone(),
        node_child: Some(spawn_daemon(&bin, &area.config_path)),
        driver_child: None,
    };
    wait_node_ready(&area.ipc_addr).await;

    let evidence_out = repo
        .join("testing/runs/deliver-session-control-takeover/coder/sct-016-real-codex-e2e.json");
    if let Some(parent) = evidence_out.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let driver_cfg_path = area.temp_root.join("driver-config.json");
    let driver_script = repo.join("clients/web/tests/e2e/sct-016-browser-driver.mjs");
    fs::write(
        &driver_cfg_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "origin": area.origin,
            "workspaceDir": area.workspace_dir,
            "realCodexMode": true,
            "codexAdapter": codex_adapter,
            "codexHome": codex_home,
            "codexPath": codex_path,
            "workDir": area.work_dir,
            "evidenceOut": evidence_out,
        }))
        .unwrap(),
    )
    .unwrap();

    guard.driver_child = Some(
        Command::new("node")
            .arg(&driver_script)
            .arg("--config")
            .arg(&driver_cfg_path)
            .current_dir(&repo)
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn Playwright sct-016 real Codex browser driver"),
    );

    pair_one_client(&area.ipc_addr, &area.work_dir.join("pairing-a.json")).await;
    wait_file_present(&area.work_dir.join("pair-a-done.json"), 60).await;
    pair_one_client(&area.ipc_addr, &area.work_dir.join("pairing-b.json")).await;

    wait_file_present(&area.work_dir.join("request-restart.json"), 180).await;
    ipc::stop(&area.ipc_addr)
        .await
        .expect("ipc stop before restart");
    if let Some(mut first_node) = guard.node_child.take() {
        let status = tokio::task::block_in_place(|| first_node.wait()).expect("wait first daemon");
        assert!(status.success(), "first daemon exit status: {status}");
    }
    guard.node_child = Some(spawn_daemon(&bin, &area.config_path));
    wait_node_ready(&area.ipc_addr).await;
    fs::write(area.work_dir.join("restart-done.json"), b"{}").unwrap();

    let mut driver = guard.driver_child.take().unwrap();
    let driver_status = tokio::task::block_in_place(|| driver.wait()).expect("wait browser driver");
    assert!(
        driver_status.success(),
        "real Codex browser driver failed: {driver_status}"
    );

    let mut report: serde_json::Value =
        serde_json::from_slice(&fs::read(&evidence_out).expect("read real codex e2e evidence"))
            .unwrap();
    report["realCodexHarness"] = serde_json::json!({
        "status": "passed",
        "adapter": "@agentclientprotocol/codex-acp",
        "isolatedCodexHome": true
    });
    fs::write(&evidence_out, serde_json::to_vec_pretty(&report).unwrap()).unwrap();

    ipc::stop(&area.ipc_addr).await.expect("ipc stop after E2E");
    if let Some(mut second_node) = guard.node_child.take() {
        let status =
            tokio::task::block_in_place(|| second_node.wait()).expect("wait second daemon");
        assert!(status.success(), "second daemon exit status: {status}");
    }
    let root_check = area.temp_root.clone();
    drop(guard);
    assert!(
        !root_check.exists(),
        "temp root (including isolated codex-home) must be removed: {}",
        root_check.display()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "HN-027: requires embedded production Web, Edge and Playwright"]
async fn hn_027_controlled_history_navigation() {
    let repo = repo_root();
    let area = setup_temp_e2e_area();
    let bin = Path::new(env!("CARGO_BIN_EXE_acp-remote"));
    let mut guard = E2eCleanupGuard {
        root: area.temp_root.clone(),
        ipc_address: area.ipc_addr.clone(),
        node_child: Some(spawn_daemon(bin, &area.config_path)),
        driver_child: None,
    };
    let job = HistoryProcessJob::attach(guard.node_child.as_ref().unwrap());
    wait_node_ready(&area.ipc_addr).await;
    let scripts = repo.join("clients/web/tests/e2e");
    let evidence = repo.join("testing/runs/deliver-history-navigation/hn-027.json");
    fs::create_dir_all(evidence.parent().unwrap()).unwrap();
    fs::write(area.peer_state_dir.join("history-navigation"), b"").unwrap();
    let config = area.temp_root.join("browser.json");
    fs::write(&config, serde_json::to_vec(&serde_json::json!({
        "origin": area.origin, "workspaceDir": area.workspace_dir, "workDir": area.work_dir,
        "peerStateDir": area.peer_state_dir, "peerScript": scripts.join("controlled-acp-v2-peer.mjs"),
        "database": area.temp_root.join("node-data/node.sqlite3"), "evidenceOut": evidence,
    })).unwrap()).unwrap();
    guard.driver_child = Some(
        Command::new("node")
            .arg(scripts.join("history-navigation.mjs"))
            .arg(config)
            .spawn()
            .unwrap(),
    );
    for name in ["a", "b"] {
        tokio::time::timeout(
            Duration::from_secs(50),
            pair_one_client(
                &area.ipc_addr,
                &area.work_dir.join(format!("pairing-{name}.json")),
            ),
        )
        .await
        .expect("browser pairing deadline");
    }
    let seed = area.work_dir.join("seed-child.json");
    while !seed.exists() {
        if guard
            .driver_child
            .as_mut()
            .unwrap()
            .try_wait()
            .unwrap()
            .is_some()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if seed.exists() {
        use acp_remote::{
            adapters::acp::{RawMessage, Version},
            domain::{EntityId, native::NativeSessionKey},
            transcript_cache::{
                CapabilityProfile, NoProtectedSessions, SystemRetentionClock, TranscriptCacheStore,
            },
        };
        use std::sync::Arc;
        let seed: serde_json::Value = serde_json::from_slice(&fs::read(seed).unwrap()).unwrap();
        let source = EntityId::decode(seed["source_id"].as_str().unwrap()).unwrap();
        let key = NativeSessionKey::from_config(Some(source), "child-history").unwrap();
        let store = TranscriptCacheStore::open(
            area.temp_root.join("node-data/transcript-cache.sqlite3"),
            acp_remote::config::TranscriptCacheConfig::default(),
            Arc::new(SystemRetentionClock),
            Arc::new(NoProtectedSessions),
        )
        .await
        .unwrap();
        let frames = (1..=12).map(|i| {
            let frame=serde_json::json!({"jsonrpc":"2.0","method":"session/update","params":{
                "sessionId":"child-history","update":{"sessionUpdate":"user_message_chunk",
                "messageId":i.to_string(),"content":{"type":"text","text":format!("HN child {i}")}}}});
            (RawMessage::parse(serde_json::to_vec(&frame).unwrap()).unwrap(),i)
        }).collect();
        store
            .replace_with_replay(&key, Version::V2, CapabilityProfile::ReplayCapable, frames)
            .await
            .unwrap();
        store.shutdown().await.unwrap();
        fs::write(area.work_dir.join("child-seeded"), b"").unwrap();
    }
    let status = guard.driver_child.as_mut().unwrap().wait().unwrap();
    ipc::stop(&area.ipc_addr).await.unwrap();
    assert!(guard.node_child.as_mut().unwrap().wait().unwrap().success());
    assert_eq!(job.totals().1, 0, "node descendant still active");
    drop(guard);
    assert!(!area.temp_root.exists());
    assert!(TcpListener::bind(area.origin.trim_start_matches("https://")).is_ok());
    assert!(
        status.success(),
        "HN-027 browser assertions failed; see sanitized evidence"
    );
}

struct HistoryProcessJob(std::os::windows::io::OwnedHandle);

impl HistoryProcessJob {
    fn attach(node: &Child) -> Self {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW};
        // SAFETY: anonymous job without inherited handles; take ownership after checking creation.
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        assert!(!raw.is_null());
        let job = unsafe { OwnedHandle::from_raw_handle(raw) };
        // SAFETY: both handles outlive this call, before pairing or launching any harness.
        assert_ne!(
            unsafe { AssignProcessToJobObject(raw, node.as_raw_handle()) },
            0
        );
        Self(job)
    }

    fn totals(&self) -> (u32, u32) {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
            QueryInformationJobObject,
        };
        let mut counts = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: the output buffer and its length match the accounting information class.
        assert_ne!(
            unsafe {
                QueryInformationJobObject(
                    self.0.as_raw_handle(),
                    JobObjectBasicAccountingInformation,
                    (&raw mut counts).cast(),
                    std::mem::size_of_val(&counts).try_into().unwrap(),
                    std::ptr::null_mut(),
                )
            },
            0
        );
        (counts.TotalProcesses, counts.ActiveProcesses)
    }
}

fn isolated_history_codex_home(root: &Path) -> PathBuf {
    let source = std::env::var_os("HN_CODEX_HOME")
        .or_else(|| std::env::var_os("CODEX_HOME"))
        .map_or_else(
            || PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join(".codex"),
            PathBuf::from,
        );
    let home = root.join("codex-home");
    fs::create_dir(&home).unwrap();
    fs::copy(source.join("auth.json"), home.join("auth.json")).expect("copy isolated login");
    let config_path = source.join("config.toml");
    let mut selected = toml::Table::new();
    // session/close unsubscribes. Let this isolated app-server unload its idle
    // thread immediately so the independent writer can acquire the same thread.
    selected.insert("thread_unload_delay_secs".to_owned(), 0.into());
    if config_path.is_file() {
        let contents = fs::read_to_string(config_path).expect("read Codex configuration");
        let config: toml::Table = contents
            .parse()
            .unwrap_or_else(|_| panic!("invalid Codex configuration"));
        for key in [
            "model",
            "model_provider",
            "openai_base_url",
            "model_catalog_json",
            "model_reasoning_effort",
            "service_tier",
        ] {
            if let Some(value) = config.get(key) {
                selected.insert(key.to_owned(), value.clone());
            }
        }
        if let Some(catalog) = selected.get("model_catalog_json").and_then(|v| v.as_str()) {
            let path = source.join(catalog);
            fs::copy(path, home.join("models.json")).expect("copy configured model catalog");
            selected.insert(
                "model_catalog_json".to_owned(),
                home.join("models.json").to_str().unwrap().into(),
            );
        }
        if let Some(name) = config.get("model_provider").and_then(|v| v.as_str())
            && let Some(provider) = config.get("model_providers").and_then(|v| v.get(name))
        {
            let mut providers = toml::Table::new();
            providers.insert(name.to_owned(), provider.clone());
            selected.insert("model_providers".to_owned(), providers.into());
        }
    }
    fs::write(
        home.join("config.toml"),
        toml::to_string(&selected).unwrap(),
    )
    .unwrap();
    home
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "HN-028: requires installed authenticated Codex and codex-acp"]
async fn hn_028_real_history_navigation() {
    let repo = repo_root();
    let adapter = std::env::var_os("HN_CODEX_ADAPTER").map_or_else(||repo.join(
        "testing/probes/codex-session-loop/node_modules/@agentclientprotocol/codex-acp/dist/index.js"),
        PathBuf::from);
    let codex = std::env::var_os("HN_CODEX_PATH").map_or_else(
        || {
            PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
                .join("Programs/OpenAI/Codex/bin/codex.exe")
        },
        PathBuf::from,
    );
    assert!(
        adapter.is_file() && codex.is_file(),
        "not-run: missing real adapter or executable"
    );
    let area = setup_temp_e2e_area();
    let bin = Path::new(env!("CARGO_BIN_EXE_acp-remote"));
    let mut guard = E2eCleanupGuard {
        root: area.temp_root.clone(),
        ipc_address: area.ipc_addr.clone(),
        node_child: Some(spawn_daemon(bin, &area.config_path)),
        driver_child: None,
    };
    let home = isolated_history_codex_home(&area.temp_root);
    let mut job = HistoryProcessJob::attach(guard.node_child.as_ref().unwrap());
    wait_node_ready(&area.ipc_addr).await;
    let evidence = repo.join("testing/runs/deliver-history-navigation/hn-028.json");
    fs::create_dir_all(evidence.parent().unwrap()).unwrap();
    let config = area.temp_root.join("browser.json");
    fs::write(&config,serde_json::to_vec(&serde_json::json!({
        "realCodexMode":true,"codexHome":home,"codexPath":codex,"codexAdapter":adapter,
        "origin":area.origin,"workspaceDir":area.workspace_dir,"workDir":area.work_dir,"evidenceOut":evidence,
    })).unwrap()).unwrap();
    guard.driver_child = Some(
        Command::new("node")
            .arg(repo.join("clients/web/tests/e2e/history-navigation.mjs"))
            .arg(config)
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_API_KEY")
            .spawn()
            .unwrap(),
    );
    tokio::time::timeout(
        Duration::from_secs(50),
        pair_one_client(&area.ipc_addr, &area.work_dir.join("pairing-a.json")),
    )
    .await
    .unwrap();
    let mut total = None;
    let mut completed = 0;
    for step in ["restart", "before-pages", "after-pages"] {
        let signal = area.work_dir.join(step);
        while !signal.exists()
            && guard
                .driver_child
                .as_mut()
                .unwrap()
                .try_wait()
                .unwrap()
                .is_none()
        {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if !signal.exists() {
            break;
        }
        match step {
            "restart" => {
                ipc::stop(&area.ipc_addr).await.unwrap();
                assert!(guard.node_child.as_mut().unwrap().wait().unwrap().success());
                assert_eq!(job.totals().1, 0);
                guard.node_child = Some(spawn_daemon(bin, &area.config_path));
                job = HistoryProcessJob::attach(guard.node_child.as_ref().unwrap());
                wait_node_ready(&area.ipc_addr).await;
            }
            "before-pages" => total = Some(job.totals().0),
            _ => assert_eq!(
                Some(job.totals().0),
                total,
                "a node descendant started during offline paging"
            ),
        }
        fs::write(area.work_dir.join(format!("{step}.done")), b"").unwrap();
        completed += 1;
    }
    let status = guard.driver_child.as_mut().unwrap().wait().unwrap();
    ipc::stop(&area.ipc_addr).await.unwrap();
    assert!(guard.node_child.as_mut().unwrap().wait().unwrap().success());
    assert_eq!(job.totals().1, 0);
    drop(guard);
    assert!(!area.temp_root.exists());
    assert!(TcpListener::bind(area.origin.trim_start_matches("https://")).is_ok());
    assert!(
        status.success() && completed == 3,
        "real browser assertions failed"
    );
    let mut report: serde_json::Value =
        serde_json::from_slice(&fs::read(&evidence).unwrap()).unwrap();
    report["nodeChecks"] = serde_json::json!({
        "offlineNewJobProcesses": 0,
        "activeJobProcessesAfterShutdown": 0,
        "isolatedDirectoryRemoved": true,
        "portRebound": true
    });
    fs::write(evidence, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
