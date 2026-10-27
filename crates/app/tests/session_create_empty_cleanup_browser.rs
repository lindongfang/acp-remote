//! Cross-layer acceptance for direct Web creation and empty-session cleanup.
//!
//! The ignored test launches the built PWA, the real Daemon binary, a controlled
//! ACP Agent over stdio, and a real Chromium/Edge instance. The browser driver
//! asserts DOM and Sync-wire facts while this Rust supervisor owns the real
//! data directory, exact 30-minute file-clock movement, Daemon restart, and
//! SQLite inspection.
//!
//! Run explicitly:
//! `cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture`
mod support;

use std::{
    net::TcpListener,
    ops::Deref,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use acp_core::{
    model::{
        Actor, CommandStatus, EntityRef, PortError, RequestId, SessionDeletionWrite, SessionId,
    },
    ports::ActivationRequest,
};
use serde_json::{Value, json};
use server::local_admin::Method;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use support::sync::HostNode;

const HOST: &str = "session-cleanup.example.invalid";
const AGENT: &str = "codex";
const PAIRING_WINDOW_MS: i64 = 5 * 60 * 1_000;

const MOCK_AGENT: &str = r#"
const fs = require('fs');
const readline = require('readline');
const evidence = process.argv[2];
const delayedResume = Number(process.argv[3] || '1500');
const append = (record) => fs.appendFileSync(
  evidence,
  JSON.stringify({ ...record, at: Date.now() }) + '\n',
);
const send = (message) => process.stdout.write(JSON.stringify(message) + '\n');
let next = 0;
let resumeCount = 0;
const prompts = new Map();
readline.createInterface({ input: process.stdin }).on('line', (line) => {
  if (!line.trim()) return;
  let message;
  try { message = JSON.parse(line); } catch { return; }
  if (message.method === undefined || message.method === null) return;
  if (message.method === 'initialize') {
    send({
      jsonrpc: '2.0',
      id: message.id,
      result: {
        protocolVersion: 1,
        agentCapabilities: {
          promptCapabilities: { embeddedContext: true, image: true },
          sessionCapabilities: { resume: {}, close: {} },
        },
        agentInfo: { name: 'session-create-empty-cleanup-agent', version: '1' },
      },
    });
    return;
  }
  if (message.method === 'session/new') {
    const sessionId = 'cleanup-agent-' + (++next);
    append({
      type: 'session_new',
      sessionId,
      cwd: message.params ? message.params.cwd : null,
    });
    send({ jsonrpc: '2.0', id: message.id, result: { sessionId } });
    return;
  }
  if (message.method === 'session/resume') {
    resumeCount += 1;
    const sessionId = message.params ? message.params.sessionId : null;
    append({ type: 'session_resume', sessionId, resumeCount });
    const delay = resumeCount === 1 || resumeCount === 3 ? delayedResume : 30;
    setTimeout(() => {
      append({ type: 'session_resume_result', sessionId, resumeCount });
      send({ jsonrpc: '2.0', id: message.id, result: {} });
    }, delay);
    return;
  }
  if (message.method === 'session/close') {
    const sessionId = message.params ? message.params.sessionId : null;
    append({ type: 'session_close', sessionId });
    send({ jsonrpc: '2.0', id: message.id, result: {} });
    return;
  }
  if (message.method === 'session/prompt') {
    const text = Array.isArray(message.params && message.params.prompt)
      ? message.params.prompt
          .map((block) => block && typeof block.text === 'string' ? block.text : '')
          .join('')
      : '';
    append({ type: 'prompt', sessionId: message.params && message.params.sessionId, text });
    const promptId = message.id;
    prompts.set(promptId, true);
    setTimeout(() => {
      if (!prompts.has(promptId)) return;
      prompts.delete(promptId);
      const requested = /token and nothing else:\s*(\S+)/i.exec(text);
      const reply = requested ? requested[1] : 'ok';
      send({
        jsonrpc: '2.0',
        method: 'session/update',
        params: {
          sessionId: message.params && message.params.sessionId,
          update: {
            sessionUpdate: 'agent_message_chunk',
            content: { type: 'text', text: reply },
          },
        },
      });
      send({ jsonrpc: '2.0', id: promptId, result: { stopReason: 'end_turn' } });
    }, 80);
    return;
  }
  if (message.id !== undefined) {
    send({ jsonrpc: '2.0', id: message.id, error: { code: -32601, message: 'unsupported' } });
  }
});
"#;

struct VisibleWorkspace(PathBuf);

impl VisibleWorkspace {
    fn create() -> Self {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .expect("user profile for a directory-picker-visible workspace");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after the Unix epoch")
            .as_millis();
        let path = home.join(format!(
            "acpr-wp5-session-cleanup-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("create test-owned visible workspace");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Deref for VisibleWorkspace {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl Drop for VisibleWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn toml_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn atomic_write(path: &Path, contents: &[u8]) {
    let temporary = path.with_extension(format!(
        "{}.tmp-{}",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("file"),
        std::process::id()
    ));
    std::fs::write(&temporary, contents)
        .unwrap_or_else(|error| panic!("write temporary file {}: {error}", temporary.display()));
    std::fs::rename(&temporary, path).unwrap_or_else(|error| {
        panic!(
            "replace {} with {}: {error}",
            path.display(),
            temporary.display()
        )
    });
}

fn write_json(path: &Path, value: &Value) {
    let bytes = serde_json::to_vec_pretty(value).expect("serialize JSON");
    atomic_write(path, &bytes);
}

fn write_clock(path: &Path, millis: i64) {
    atomic_write(path, millis.to_string().as_bytes());
}

fn pairing_value(daemon: &support::Daemon) -> Value {
    daemon.ok_value(
        Method::DevicePairBegin,
        support::params_of(json!({ "expiresInMs": null })),
    )
}

async fn sqlite_pool(data_dir: &Path) -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(data_dir.join("acp-remote.sqlite3"))
                .create_if_missing(false),
        )
        .await
        .expect("open acceptance database")
}

async fn inspect_session(data_dir: &Path, session: &str) -> Value {
    let pool = sqlite_pool(data_dir).await;
    let live: Option<String> = sqlx::query_scalar(
        "SELECT session_id FROM owned_session WHERE session_id=?1 AND deleted_at IS NULL",
    )
    .bind(session)
    .fetch_optional(&pool)
    .await
    .expect("read session row");
    let directory: Option<String> = sqlx::query_scalar(
        "SELECT workspace_directory_id FROM owned_session \
         WHERE session_id=?1 AND deleted_at IS NULL",
    )
    .bind(session)
    .fetch_optional(&pool)
    .await
    .expect("read directory assignment")
    .flatten();
    let deleted_at: Option<String> =
        sqlx::query_scalar("SELECT deleted_at FROM owned_session WHERE session_id=?1")
            .bind(session)
            .fetch_optional(&pool)
            .await
            .expect("read tombstone")
            .flatten();
    let messages: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_message WHERE session_id=?1")
            .bind(session)
            .fetch_one(&pool)
            .await
            .expect("count messages");
    let turns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_turn WHERE session_id=?1")
        .bind(session)
        .fetch_one(&pool)
        .await
        .expect("count turns");
    let interactions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_interaction WHERE session_id=?1")
            .bind(session)
            .fetch_one(&pool)
            .await
            .expect("count interactions");
    let commands: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT request_id, command, status, terminal_event_id FROM owned_command WHERE session_id=?1 AND kind='mutation'",
    )
    .bind(session)
    .fetch_all(&pool)
    .await
    .expect("inspect authoritative mutation protection and terminals");
    let commands: Vec<Value> = commands.into_iter().map(|(request, command, status, terminal)| {
        json!({ "requestId": request, "command": command, "status": status, "terminalEventId": terminal })
    }).collect();
    pool.close().await;
    json!({
        "commands": commands,
        "live": live.is_some(),
        "directoryAssigned": directory.is_some(),
        "deletedAt": deleted_at,
        "messages": messages,
        "turns": turns,
        "interactions": interactions,
    })
}

fn control_response_path(control_dir: &Path, id: u64) -> PathBuf {
    control_dir.join(format!("response-{id}.json"))
}

struct BrowserDriver {
    child: Option<Child>,
}

impl BrowserDriver {
    fn spawn(command: &mut Command) -> Self {
        Self {
            child: Some(command.spawn().expect("browser acceptance driver")),
        }
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child
            .as_mut()
            .expect("browser acceptance driver is running")
            .try_wait()
    }

    fn terminate_tree(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        if child.try_wait().ok().flatten().is_some() {
            return;
        }

        #[cfg(windows)]
        {
            let pid = child.id().to_string();
            let _ = Command::new("taskkill")
                .args(["/PID", pid.as_str(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}

impl Drop for BrowserDriver {
    fn drop(&mut self) {
        self.terminate_tree();
    }
}

fn browser_failure_context(
    daemon: &support::Daemon,
    browser_stderr: &Path,
    browser_evidence: &Path,
    agent_evidence: &Path,
) -> String {
    format!(
        "daemon stderr:\n{}\n\ndaemon log:\n{}\n\nbrowser stderr:\n{}\n\n\
         browser evidence:\n{}\n\nagent evidence:\n{}",
        daemon.stderr(),
        daemon.log(),
        std::fs::read_to_string(browser_stderr).unwrap_or_default(),
        std::fs::read_to_string(browser_evidence).unwrap_or_default(),
        std::fs::read_to_string(agent_evidence).unwrap_or_default(),
    )
}

fn stop_daemon_checked(
    daemon: &mut support::Daemon,
    when: &str,
    browser_stderr: &Path,
    browser_evidence: &Path,
    agent_evidence: &Path,
) {
    let context = browser_failure_context(daemon, browser_stderr, browser_evidence, agent_evidence);
    let status = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| daemon.stop())) {
        Ok(status) => status,
        Err(payload) => {
            let detail = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            panic!("{when}: Daemon shutdown request failed: {detail}\n{context}");
        }
    };
    assert!(
        status.success(),
        "{when}: Daemon shutdown exited with {status}\n{context}"
    );
}

#[test]
#[ignore = "requires clients/app/dist, Node, Chromium/Edge, and a local loopback TLS listener"]
fn direct_create_cleanup_restart_and_races_are_observable() {
    let base_clock_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after the Unix epoch")
        .as_millis()
        .try_into()
        .expect("system clock fits i64 milliseconds");
    let restart_clock_ms = base_clock_ms + PAIRING_WINDOW_MS;
    let driver = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../clients/app/e2e/session-create-empty-cleanup-live.mjs");
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clients/app/dist/index.html");
    assert!(
        driver.is_file(),
        "browser driver is missing: {}",
        driver.display()
    );
    assert!(
        dist.is_file(),
        "built PWA is missing: {} (run `npm run export:web` in clients/app)",
        dist.display()
    );

    let port = TcpListener::bind("127.0.0.1:0")
        .expect("reserve loopback port")
        .local_addr()
        .expect("port address")
        .port();
    let mut daemon = support::Daemon::configure_with_bind(
        "session-create-empty-cleanup",
        "127.0.0.1",
        port,
        "",
        "",
    );
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])
        .expect("self-signed certificate");
    let cert_path = daemon.root().join("cert.pem");
    let key_path = daemon.root().join("key.pem");
    let mock_path = daemon.root().join("agent.cjs");
    let agent_evidence = daemon.root().join("agent-evidence.jsonl");
    let browser_evidence = daemon.root().join("browser-evidence.json");
    let browser_stderr = daemon.root().join("browser.stderr.log");
    let browser_stdout = daemon.root().join("browser.stdout.log");
    let control_dir = daemon.root().join("control");
    let clock_path = daemon.root().join("clock.ms");
    let workspace_root = VisibleWorkspace::create();
    std::fs::create_dir_all(&control_dir).expect("control directory");
    for name in [
        "direct",
        "switch",
        "restart-seeded-boundary",
        "restart-activation",
        "restart-fresh-boundary",
        "messaged",
        "usable",
        "boundary",
        "untouched",
    ] {
        std::fs::create_dir_all(workspace_root.join(name)).expect("workspace directory");
    }
    std::fs::write(&mock_path, MOCK_AGENT).expect("controlled ACP Agent");
    std::fs::write(&cert_path, cert.cert.pem()).expect("certificate");
    std::fs::write(&key_path, cert.signing_key.serialize_pem()).expect("private key");
    write_clock(&clock_path, base_clock_ms);

    let config = std::fs::read_to_string(daemon.config_path())
        .expect("base daemon configuration")
        .replace(
            "https://acpr-test.example.invalid",
            &format!("https://localhost:{port}"),
        );
    let service_config = format!(
        "[daemon.tls]\nmode = \"direct\"\ncert_path = {:?}\n\
         key_path = {:?}\n\n[agent]\ndefault = \"test\"\n\
         test = [\"node\", {:?}, {:?}, \"5000\"]\n",
        toml_path(&cert_path),
        toml_path(&key_path),
        toml_path(&mock_path),
        toml_path(&agent_evidence),
    );
    std::fs::write(daemon.config_path(), format!("{config}\n{service_config}"))
        .expect("TLS and Agent configuration");
    daemon = daemon
        .with_env("ACPR_TEST_CLOCK_FILE", &clock_path)
        .with_env("ACPR_TEST_RECLAIM_INTERVAL_MS", "25");
    daemon.start();

    let initial_pairing = pairing_value(&daemon);
    let initial_pairing_id = initial_pairing["pairingId"]
        .as_str()
        .expect("initial pairing id")
        .to_owned();
    let mut pending_pairing = Some(initial_pairing_id);

    let mut browser_command = Command::new("node");
    browser_command
        .arg(&driver)
        .env("SESSION_CREATE_ORIGIN", format!("https://localhost:{port}"))
        .env(
            "SESSION_CREATE_CODE",
            initial_pairing["pairingCode"]
                .as_str()
                .expect("initial pairing code"),
        )
        .env("SESSION_CREATE_WORKSPACE_ROOT", workspace_root.path())
        .env("SESSION_CREATE_EVIDENCE", &browser_evidence)
        .env("SESSION_CREATE_AGENT_EVIDENCE", &agent_evidence)
        .env("SESSION_CREATE_CONTROL_DIR", &control_dir)
        .env("SESSION_CREATE_CLOCK_FILE", &clock_path)
        .env("SESSION_CREATE_BASE_CLOCK_MS", base_clock_ms.to_string())
        .env(
            "SESSION_CREATE_RESTART_CLOCK_MS",
            restart_clock_ms.to_string(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            std::fs::File::create(&browser_stdout).expect("browser stdout"),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(&browser_stderr).expect("browser stderr"),
        ));
    let mut browser = BrowserDriver::spawn(&mut browser_command);

    let deadline = Instant::now() + Duration::from_secs(360);
    let mut handled_requests = std::collections::HashSet::new();
    let mut retired_daemons = Vec::new();
    let status = loop {
        if let Some(status) = browser.try_wait().expect("browser status") {
            break Some(status);
        }
        if Instant::now() >= deadline {
            browser.terminate_tree();
            break None;
        }

        if let Some(id) = pending_pairing.as_deref() {
            let state = daemon.ok_value(
                Method::DevicePairStatus,
                support::params_of(json!({ "pairingId": id })),
            );
            if state["state"] == "claimed" {
                daemon.ok_value(
                    Method::DevicePairConfirm,
                    support::params_of(json!({ "pairingId": id })),
                );
                pending_pairing = None;
            }
        }

        let mut requests = std::fs::read_dir(&control_dir)
            .expect("control directory")
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name();
                let name = name.to_str()?;
                let id = name.strip_prefix("request-")?.strip_suffix(".json")?;
                Some((id.parse::<u64>().ok()?, entry.path()))
            })
            .collect::<Vec<_>>();
        requests.sort_by_key(|(id, _)| *id);
        for (id, request_path) in requests {
            if !handled_requests.insert(id) {
                continue;
            }
            let request: Value = serde_json::from_slice(
                &std::fs::read(&request_path).expect("read browser control request"),
            )
            .expect("browser control request JSON");
            let response = match request["op"].as_str().expect("control operation") {
                "inspect_created" | "inspect_deleted" => {
                    let session = request["sessionId"].as_str().expect("control session id");
                    json!({
                        "ok": true,
                        "session": support::block_on(inspect_session(daemon.data_dir(), session)),
                    })
                }
                "restart" => {
                    stop_daemon_checked(
                        &mut daemon,
                        "Daemon restart",
                        &browser_stderr,
                        &browser_evidence,
                        &agent_evidence,
                    );
                    write_clock(&clock_path, restart_clock_ms);
                    // Keep the exact restart boundary independent of the slow switching fixture;
                    // the initial process retains the 5s delayed switching race.
                    let restarted_service_config = service_config.replace("\"5000\"", "\"1500\"");
                    let mut restarted = daemon.configure_in_with_bind(
                        "session-create-empty-cleanup-restarted",
                        "127.0.0.1",
                        port,
                        "",
                        &restarted_service_config,
                    );
                    let restarted_config = std::fs::read_to_string(restarted.config_path())
                        .expect("restart daemon configuration")
                        .replace(
                            "https://acpr-test.example.invalid",
                            &format!("https://localhost:{port}"),
                        );
                    std::fs::write(restarted.config_path(), restarted_config)
                        .expect("restart public origin");
                    restarted.start();
                    let pairing = pairing_value(&restarted);
                    pending_pairing = Some(
                        pairing["pairingId"]
                            .as_str()
                            .expect("restart pairing id")
                            .to_owned(),
                    );
                    let previous = std::mem::replace(&mut daemon, restarted);
                    retired_daemons.push(previous);
                    json!({
                        "ok": true,
                        "pairingCode": pairing["pairingCode"],
                        "restartClockMs": restart_clock_ms,
                    })
                }
                operation => panic!("unknown browser control operation: {operation}"),
            };
            write_json(&control_response_path(&control_dir, id), &response);
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    stop_daemon_checked(
        &mut daemon,
        "acceptance shutdown",
        &browser_stderr,
        &browser_evidence,
        &agent_evidence,
    );
    let context =
        browser_failure_context(&daemon, &browser_stderr, &browser_evidence, &agent_evidence);
    let report: Value =
        serde_json::from_slice(&std::fs::read(&browser_evidence).unwrap_or_else(|error| {
            panic!("read browser evidence after shutdown: {error}\n{context}")
        }))
        .unwrap_or_else(|error| {
            panic!("parse browser evidence after shutdown: {error}\n{context}")
        });
    let page_errors = report["pageErrors"]
        .as_array()
        .unwrap_or_else(|| panic!("browser report has no pageErrors array:\n{context}"));
    assert!(
        page_errors.is_empty(),
        "browser Runtime.exceptionThrown occurred:\n{}\n{context}",
        serde_json::to_string_pretty(page_errors).expect("page error JSON")
    );
    let status = status.expect("browser acceptance timed out and its process tree was terminated");
    assert!(
        status.success(),
        "real-browser acceptance failed\n{context}"
    );

    let scenarios = report["scenarios"]
        .as_array()
        .expect("browser scenario array");
    for required in [
        "direct-create-is-persisted-and-active",
        "switch-keeps-latest-session",
        "restart-grants-a-fresh-window",
        "exact-boundary-cleanup-and-retention",
        "untouched-empty-session-is-cleaned",
        "deleted-session-does-not-resurrect",
    ] {
        assert!(
            scenarios
                .iter()
                .any(|scenario| { scenario["name"] == required && scenario["status"] == "passed" }),
            "required browser scenario `{required}` did not pass:\n{context}"
        );
    }
    assert!(
        report["skipped"].as_array().is_some_and(Vec::is_empty),
        "the required acceptance run cannot report SKIP:\n{}",
        serde_json::to_string_pretty(&report).expect("report JSON")
    );

    eprintln!(
        "WP5_BROWSER_EVIDENCE={}",
        json!({
            "scenarios": report["scenarios"], "clock": report["clock"],
            "pageErrors": report["pageErrors"], "skipped": report["skipped"],
            "browserVersion": report["browserVersion"],
            "cleanShutdownFenceConsumed": daemon.log().contains("agent_host.clean_shutdown_fence_consumed"),
        })
    );
    support::block_on(run_cross_layer_races());
    eprintln!("WP5_CROSS_LAYER_RACES=PASS activation-first deletion-first");
}

fn digest(label: &str) -> acp_core::model::Digest {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(label.as_bytes());
    acp_core::model::Digest::new(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize()),
    )
    .expect("digest")
}

fn lifecycle_config(
    idle_timeout_ms: u64,
    max_active_sessions: u32,
) -> acp_core::use_cases::LifecycleConfig {
    acp_core::use_cases::LifecycleConfig::new(idle_timeout_ms, max_active_sessions)
        .expect("lifecycle config")
}

async fn local_session(host: &HostNode) -> String {
    let request_id = RequestId::new(&support::sync::uuid_text()).expect("request id");
    let workspace_path = host.data_dir().join("race-workspace");
    std::fs::create_dir_all(&workspace_path).expect("workspace");
    let canonical = std::fs::canonicalize(&workspace_path).expect("canonical workspace");
    let workspace =
        acp_core::model::ResolvedWorkspace::at_path(canonical.to_string_lossy().into_owned())
            .expect("resolved workspace");
    let prepared = match host
        .core()
        .create_session(
            &Actor::LocalCli,
            request_id.clone(),
            digest("session-create-empty-cleanup/race"),
            acp_core::model::CreateSessionRequest::new(
                acp_core::model::AgentRef::try_new(
                    acp_core::model::AgentId::new(AGENT).expect("agent id"),
                    "Codex",
                )
                .expect("agent ref"),
                Some(workspace),
                None,
                acp_core::model::ResourceOrigin::Local,
            ),
            None,
        )
        .await
        .expect("create session")
    {
        acp_core::ports::SessionCreateAdmission::Prepared(prepared) => prepared,
        other => panic!("first create admission must be prepared: {other:?}"),
    };
    let result = acp_core::model::CommandResult::from_json_text(r#"{"sessionId":"projected"}"#)
        .expect("result object");
    assert!(
        host.core()
            .settle_session_create(
                &Actor::LocalCli,
                &request_id,
                CommandStatus::Completed,
                Some(result),
                None,
            )
            .await
            .expect("settle create"),
        "session create must commit"
    );
    prepared.session.as_str().to_owned()
}

fn activation_request(session: &str, label: &str) -> ActivationRequest {
    ActivationRequest {
        actor: Actor::LocalCli,
        request_id: RequestId::new(&support::sync::uuid_text()).expect("request id"),
        request_fingerprint: digest(label),
        session: SessionId::new(session).expect("session id"),
    }
}

async fn admit_delete(host: &HostNode, session: &str) -> SessionDeletionWrite {
    let request = host.core().new_request_id();
    let fingerprint = server::sync::local_lifecycle::session_delete_fingerprint();
    let admission = host
        .core()
        .admit_session_delete(
            &Actor::LocalCli,
            request.clone(),
            fingerprint.clone(),
            SessionId::new(session).expect("session id"),
        )
        .await
        .expect("admit delete");
    assert!(matches!(
        admission,
        acp_core::broker::SessionDeleteAdmission::Prepared
    ));
    SessionDeletionWrite::new(
        SessionId::new(session).expect("session id"),
        Actor::LocalCli,
        request,
        fingerprint,
        host.clock().now(),
    )
}

async fn session_is_visible(host: &HostNode, session: &str) -> bool {
    host.core()
        .list_sessions(&Actor::LocalCli, acp_core::ports::SessionQuery::default())
        .await
        .expect("list sessions")
        .iter()
        .any(|item| item.session_id().as_str() == session)
}

async fn wait_for_retention_candidate(
    host: &HostNode,
    session: &str,
) -> acp_core::ports::ReclaimCandidate {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let candidates = host
            .lifecycle()
            .retention_due(&Actor::LocalCli, host.clock().now())
            .await
            .expect("retention candidates");
        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.session.as_str() == session)
        {
            return candidate.clone();
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "session did not become a retention candidate: {candidates:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn run_cross_layer_races() {
    let activation_host =
        HostNode::start_with_lifecycle("wp5-activation-first", HOST, lifecycle_config(1, 5)).await;
    let activation_session = local_session(&activation_host).await;
    let candidate = wait_for_retention_candidate(&activation_host, &activation_session).await;
    let candidate = server::sync::local_lifecycle::retention_delete_candidate(
        activation_host.lifecycle().as_ref(),
        candidate,
    )
    .expect("capture candidate")
    .expect("candidate lifecycle snapshot");
    let plan = activation_host
        .lifecycle()
        .activate_session(
            &Actor::LocalCli,
            activation_request(&activation_session, "wp5-activation-first"),
        )
        .await
        .expect("activation wins");
    assert!(plan.reuses_ready_resource());
    let delete_write = admit_delete(&activation_host, &activation_session).await;
    let delete_request = delete_write.request().clone();
    server::sync::local_lifecycle::execute_retention_session_delete(
        activation_host.core().as_ref(),
        activation_host.lifecycle().as_ref(),
        delete_write.at().clone(),
        delete_write.actor(),
        delete_write.request(),
        delete_write.request_fingerprint(),
        &candidate,
    )
    .await
    .expect("stale candidate becomes failed terminal");
    assert_eq!(
        activation_host.script().release_calls(),
        0,
        "activation-first must prevent the stale candidate from releasing the Agent"
    );
    assert!(
        session_is_visible(&activation_host, &activation_session).await,
        "activation-first must keep the session visible"
    );
    let delete_record = activation_host
        .core()
        .command_status(&Actor::LocalCli, delete_request)
        .await
        .expect("delete command status")
        .expect("delete terminal");
    assert_eq!(delete_record.status(), CommandStatus::Failed);
    activation_host.stop().await;

    let deletion_host =
        HostNode::start_with_lifecycle("wp5-deletion-first", HOST, lifecycle_config(1_800_000, 5))
            .await;
    let deletion_session = local_session(&deletion_host).await;
    let delete_write = admit_delete(&deletion_host, &deletion_session).await;
    let session_id = delete_write.session().clone();
    deletion_host.script().block_releases();
    let delete = tokio::spawn({
        let core = Arc::clone(deletion_host.core());
        let lifecycle = Arc::clone(deletion_host.lifecycle());
        async move {
            server::sync::local_lifecycle::execute_session_delete(
                core.as_ref(),
                lifecycle.as_ref(),
                delete_write.at().clone(),
                delete_write.actor(),
                delete_write.request(),
                delete_write.request_fingerprint(),
                delete_write.session(),
            )
            .await
        }
    });
    deletion_host.script().wait_for_release_entered().await;
    let resumes_before = deletion_host.script().resume_calls();
    let mut activation = tokio::spawn({
        let lifecycle = Arc::clone(deletion_host.lifecycle());
        let request = activation_request(&deletion_session, "wp5-deletion-first");
        async move { lifecycle.activate_session(&Actor::LocalCli, request).await }
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(30), &mut activation)
            .await
            .is_err(),
        "late activation must queue behind deletion"
    );
    deletion_host.script().allow_release();
    delete
        .await
        .expect("delete task")
        .expect("delete execution");
    let error = activation
        .await
        .expect("activation task")
        .expect_err("activation after deletion must fail");
    assert!(matches!(
        error,
        PortError::NotFound(EntityRef::Session(ref missing)) if missing == &session_id
    ));
    assert_eq!(
        deletion_host.script().resume_calls(),
        resumes_before,
        "a late activation after deletion must not start an Agent"
    );
    assert!(!session_is_visible(&deletion_host, &deletion_session).await);
    deletion_host.stop().await;
}
