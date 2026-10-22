//! Opt-in real PWA → TLS Daemon acceptance for the local session lifecycle (`session-activation-lifecycle`).
//!
//! Requires a built `clients/app/dist`, a local Chromium/Edge and `node` on `PATH`. The Daemon is the
//! real binary; the Agent is a controllable Node stdio server configured through `[agent]`, so the
//! browser flow can create sessions, prompt them and exercise `session.view`/`session.resume` against
//! a real Sync connection. The Node driver (`clients/app/e2e/session-lifecycle-live.mjs`) asserts on
//! DOM hooks and real wire frames; this test only prepares the daemon/pairing and reports failure.
//!
//! Run explicitly:
//! `cargo test -p app --test session_lifecycle_browser -- --ignored --nocapture`
mod support;

use serde_json::json;
use server::local_admin::Method;
use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Minimal ACP Agent used by the browser acceptance.
///
/// It advertises `sessionCapabilities.resume`/`close` (the lifecycle feature is usable), records
/// every `session/new`/`session/prompt`, and answers with a short streamed turn. It never cancels a
/// turn on its own: a page switch or reconnect must not re-dispatch or cancel.
const MOCK_AGENT: &str = r#"
const fs = require('fs');
const readline = require('readline');
const evidence = process.argv[2];
const append = (record) => fs.appendFileSync(evidence, JSON.stringify(record) + '\n');
const send = (message) => process.stdout.write(JSON.stringify(message) + '\n');
let next = 0;
const active = new Map();
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
        agentInfo: { name: 'session-lifecycle-browser-agent', version: '1' },
      },
    });
    return;
  }
  if (message.method === 'session/new') {
    const sessionId = 'lifecycle-agent-' + (++next);
    append({ type: 'session_new', sessionId, cwd: message.params ? message.params.cwd : null });
    send({ jsonrpc: '2.0', id: message.id, result: { sessionId } });
    return;
  }
  if (message.method === 'session/resume') {
    append({ type: 'session_resume', sessionId: message.params ? message.params.sessionId : null });
    send({ jsonrpc: '2.0', id: message.id, result: {} });
    return;
  }
  if (message.method === 'session/close') {
    append({ type: 'session_close', sessionId: message.params ? message.params.sessionId : null });
    send({ jsonrpc: '2.0', id: message.id, result: {} });
    return;
  }
  if (message.method === 'session/prompt') {
    const text = Array.isArray(message.params && message.params.prompt)
      ? message.params.prompt.map((block) => (block && typeof block.text === 'string' ? block.text : '')).join('')
      : '';
    append({ type: 'prompt', text });
    const promptId = message.id;
    active.set(promptId, true);
    setTimeout(() => {
      if (!active.has(promptId)) return;
      active.delete(promptId);
      const requested = /token and nothing else:\s*(\S+)/i.exec(text);
      const reply = requested ? requested[1] : 'ok';
      send({
        jsonrpc: '2.0',
        method: 'session/update',
        params: {
          sessionId: message.params && message.params.sessionId,
          update: { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: reply } },
        },
      });
      send({ jsonrpc: '2.0', id: promptId, result: { stopReason: 'end_turn' } });
    }, 120);
    return;
  }
  if (message.method === 'session/cancel') {
    for (const promptId of active.keys()) {
      active.delete(promptId);
      send({ jsonrpc: '2.0', id: promptId, result: { stopReason: 'cancelled' } });
    }
    return;
  }
  if (message.id !== undefined) {
    send({ jsonrpc: '2.0', id: message.id, error: { code: -32601, message: 'unsupported' } });
  }
});
"#;

fn toml_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[test]
#[ignore = "requires a local Chromium browser, node on PATH and built Web UI"]
fn real_browser_activates_keeps_alive_and_switches_sessions() {
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("port")
        .local_addr()
        .expect("address")
        .port();
    let mut daemon = support::Daemon::configure_with_bind(
        "session-lifecycle-browser",
        "127.0.0.1",
        port,
        "",
        "",
    );
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).expect("certificate");
    let cert_path = daemon.root().join("cert.pem");
    let key_path = daemon.root().join("key.pem");
    let mock_path = daemon.root().join("agent.cjs");
    let evidence_path = daemon.root().join("agent-evidence.jsonl");
    let lifecycle_evidence = daemon.root().join("lifecycle-evidence.json");
    std::fs::write(&mock_path, MOCK_AGENT).expect("controlled Agent");
    std::fs::write(&cert_path, cert.cert.pem()).expect("certificate file");
    std::fs::write(&key_path, cert.signing_key.serialize_pem()).expect("key file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cert_path, std::fs::Permissions::from_mode(0o600))
            .expect("certificate permissions");
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
            .expect("key permissions");
    }
    let config = std::fs::read_to_string(daemon.config_path())
        .expect("configuration")
        .replace(
            "https://acpr-test.example.invalid",
            &format!("https://localhost:{port}"),
        );
    std::fs::write(
        daemon.config_path(),
        format!(
            "{config}\n[daemon.tls]\nmode = \"direct\"\ncert_path = {:?}\nkey_path = {:?}\n\n[agent]\ndefault = \"test\"\ntest = [\"node\", {:?}, {:?}]\n",
            toml_path(&cert_path),
            toml_path(&key_path),
            toml_path(&mock_path),
            toml_path(&evidence_path),
        ),
    )
    .expect("TLS configuration");
    daemon.start();

    let pairing = daemon.ok_value(
        Method::DevicePairBegin,
        support::params_of(json!({"expiresInMs": null})),
    );

    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository");
    let repo = PathBuf::from(repo.to_string_lossy().trim_start_matches(r"\\?\"));
    // 目录选择器默认隐藏系统/隐藏目录（`AppData` 不可见）：把工作目录放在用户目录下，
    // 它在浏览器第一屏就能被点开，避免测试依赖隐藏目录导航。
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .expect("user profile");
    let workspace = home.join(format!("acpr-lifecycle-browser-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&workspace);
    std::fs::create_dir_all(&workspace).expect("workspace");

    let mut browser = Command::new("node")
        .arg(repo.join("clients/app/e2e/session-lifecycle-live.mjs"))
        .env(
            "SESSION_LIFECYCLE_ORIGIN",
            format!("https://localhost:{port}"),
        )
        .env(
            "SESSION_LIFECYCLE_CODE",
            pairing["pairingCode"].as_str().expect("pairing code"),
        )
        .env("SESSION_LIFECYCLE_WORKSPACE", &workspace)
        .env("SESSION_LIFECYCLE_EVIDENCE", &lifecycle_evidence)
        .stdin(Stdio::null())
        .spawn()
        .expect("browser driver");

    let deadline = Instant::now() + Duration::from_secs(300);
    let pairing_id = pairing["pairingId"].as_str().expect("pairing id");
    let mut approved = false;
    let status = loop {
        if let Some(status) = browser.try_wait().expect("browser status") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = browser.kill();
            let _ = browser.wait();
            panic!("browser acceptance timeout");
        }
        if !approved {
            let state = daemon.ok_value(
                Method::DevicePairStatus,
                support::params_of(json!({"pairingId": pairing_id})),
            );
            if state["state"] == "claimed" {
                daemon.ok_value(
                    Method::DevicePairConfirm,
                    support::params_of(json!({"pairingId": pairing_id})),
                );
                approved = true;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if !status.success() {
        eprintln!("daemon log:\n{}", daemon.log());
        eprintln!("daemon stderr:\n{}", daemon.stderr());
    }
    assert!(status.success(), "real browser lifecycle acceptance failed");

    let evidence = std::fs::read_to_string(&evidence_path).expect("Agent evidence");
    let records: Vec<serde_json::Value> = evidence
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("Agent evidence line"))
        .collect();
    let count = |kind: &str| {
        records
            .iter()
            .filter(|record| record["type"] == json!(kind))
            .count()
    };
    assert!(
        count("session_new") >= 1,
        "the PWA must create a session: {evidence}"
    );
    assert!(
        count("prompt") >= 1,
        "the PWA must dispatch a prompt: {evidence}"
    );
    // L4 switches between two sessions; the Agent must have seen both.
    assert!(
        count("session_new") >= 2,
        "the A/B switch scenario must create two sessions: {evidence}"
    );
    let lifecycle = std::fs::read_to_string(&lifecycle_evidence).expect("lifecycle evidence");
    assert!(
        lifecycle.contains("enter-activates-immediately"),
        "the live driver must record its scenario results: {lifecycle}"
    );
    let _ = std::fs::remove_dir_all(&workspace);
}
