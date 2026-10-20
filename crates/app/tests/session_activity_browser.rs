//! Opt-in real PWA → TLS Daemon → ACP Agent acceptance for `session-activity-three-states`.
//!
//! Requires built `clients/app/dist`, a local Chromium/Edge and `node` on `PATH`. The Daemon is the
//! real binary; the Agent is a controllable Node stdio server configured through `[agent]`, so the
//! browser flow can hold a turn open, emit two permission requests, fail a turn and keep a turn
//! running across a transport disconnect. The Agent records every inbound prompt and permission
//! answer in an evidence file: the Rust side asserts on that wire-level evidence, not on UI text.
mod support;

use serde_json::json;
use server::local_admin::Method;
use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Controllable ACP Agent used by the browser acceptance.
///
/// Behaviour is selected by the prompt text sent by the PWA:
/// - `hold-turn`       streams one chunk and completes after 10 s (page switch must not cancel it);
/// - `permissions-turn` emits two `session/request_permission` requests and completes 5 s after the
///   second answer (state must stay `pending` until both are answered);
/// - `fail-turn`       answers the prompt with a JSON-RPC error (turn must end `idle` with the reason);
/// - `recover-turn`    streams a chunk and completes after 0.8 s (a new prompt is accepted);
/// - `disconnect-turn` completes after 12 s (the turn outlives the WebSocket connection).
///
/// `session/cancel` is recorded but never required: the test fails if a page switch or reconnect
/// produces one.
const MOCK_AGENT: &str = r#"
const fs = require('fs');
const readline = require('readline');
const evidence = process.argv[2];
const append = (record) => fs.appendFileSync(evidence, JSON.stringify(record) + '\n');
const send = (message) => process.stdout.write(JSON.stringify(message) + '\n');
const sessionId = 'session-activity-agent';
let nextRequestId = 9100;
let permissions = [];
const activePrompts = new Set();
append({ type: 'spawn' });
const completeLater = (promptId, text, delayMs) => {
  setTimeout(() => {
    activePrompts.delete(promptId);
    send({
      jsonrpc: '2.0',
      method: 'session/update',
      params: {
        sessionId,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text },
        },
      },
    });
    send({ jsonrpc: '2.0', id: promptId, result: { stopReason: 'end_turn' } });
  }, delayMs);
};
readline.createInterface({ input: process.stdin }).on('line', (line) => {
  if (!line.trim()) return;
  let message;
  try { message = JSON.parse(line); } catch { return; }
  if (message.method === undefined || message.method === null) {
    const outcome = message.result && message.result.outcome ? message.result.outcome : null;
    append({
      type: 'permission_response',
      id: message.id,
      optionId: outcome ? outcome.optionId : null,
      outcome: outcome ? outcome.outcome : null,
    });
    const waiter = permissions.find((entry) => entry.id === message.id);
    if (!waiter) {
      append({ type: 'unexpected_response', id: message.id });
      return;
    }
    waiter.resolved = true;
    if (permissions.every((entry) => entry.resolved)) {
      const promptId = permissions[0].promptId;
      permissions = [];
      completeLater(promptId, 'permissions-done', 5000);
    }
    return;
  }
  if (message.method === 'initialize') {
    send({
      jsonrpc: '2.0',
      id: message.id,
      result: {
        protocolVersion: 1,
        agentCapabilities: {},
        agentInfo: { name: 'session-activity-agent', version: '1' },
      },
    });
    return;
  }
  if (message.method === 'session/new') {
    append({ type: 'session_new', cwd: message.params ? message.params.cwd : null });
    send({ jsonrpc: '2.0', id: message.id, result: { sessionId } });
    return;
  }
  if (message.method === 'session/cancel') {
    append({ type: 'cancel' });
    for (const promptId of activePrompts) {
      send({ jsonrpc: '2.0', id: promptId, result: { stopReason: 'cancelled' } });
    }
    activePrompts.clear();
    return;
  }
  if (message.method === 'session/prompt') {
    const chunks = message.params && Array.isArray(message.params.prompt) ? message.params.prompt : [];
    const text = chunks
      .map((block) => (block && block.type === 'text' && typeof block.text === 'string' ? block.text : ''))
      .join('')
      .trim();
    append({ type: 'prompt', text });
    const promptId = message.id;
    activePrompts.add(promptId);
    if (text === 'permissions-turn') {
      const request = (index, title) => {
        const id = nextRequestId + index;
        permissions.push({ id, promptId, resolved: false });
        append({ type: 'permission_request', id, title });
        send({
          jsonrpc: '2.0',
          id,
          method: 'session/request_permission',
          params: {
            sessionId,
            toolCall: { toolCallId: 'tool-' + index, title, kind: 'edit', status: 'pending' },
            options: [
              { optionId: 'allow_once', name: '允许一次', kind: 'allow_once' },
              { optionId: 'deny', name: '拒绝', kind: 'deny' },
            ],
          },
        });
      };
      nextRequestId += 10;
      request(1, '写入工作区 A');
      request(2, '写入工作区 B');
      return;
    }
    if (text === 'fail-turn') {
      setTimeout(() => {
        activePrompts.delete(promptId);
        send({ jsonrpc: '2.0', id: promptId, error: { code: -32000, message: '模拟失败' } });
      }, 150);
      return;
    }
    const delay =
      text === 'hold-turn' ? 10000 : text === 'disconnect-turn' ? 12000 : text === 'recover-turn' ? 800 : 200;
    const reply =
      text === 'hold-turn' ? 'hold-done' : text === 'disconnect-turn' ? 'disconnect-done' : text === 'recover-turn' ? 'recover-done' : 'ok-done';
    completeLater(promptId, reply, delay);
    return;
  }
  if (message.id !== undefined) {
    send({ jsonrpc: '2.0', id: message.id, error: { code: -32601, message: 'unsupported' } });
  }
});
"#;

/// Path text for TOML: forward slashes, no escaping.
fn toml_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[test]
#[ignore = "requires a local Chromium browser, node on PATH and built Web UI"]
fn real_browser_observes_idle_running_pending_failure_and_reconnect() {
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("port")
        .local_addr()
        .expect("address")
        .port();
    let mut daemon =
        support::Daemon::configure_with_bind("session-activity-browser", "127.0.0.1", port, "", "");
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).expect("certificate");
    let cert_path = daemon.root().join("cert.pem");
    let key_path = daemon.root().join("key.pem");
    let mock_path = daemon.root().join("agent.cjs");
    let evidence_path = daemon.root().join("agent-evidence.jsonl");
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
    let workspace = repo
        .join("target")
        .join(format!("session-activity-browser-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&workspace);
    std::fs::create_dir_all(&workspace).expect("workspace");

    let mut browser = Command::new("node")
        .arg(repo.join("clients/app/e2e/session-activity-live.mjs"))
        .env(
            "SESSION_ACTIVITY_ORIGIN",
            format!("https://localhost:{port}"),
        )
        .env(
            "SESSION_ACTIVITY_CODE",
            pairing["pairingCode"].as_str().expect("pairing code"),
        )
        .env("SESSION_ACTIVITY_WORKSPACE", &workspace)
        .env("SESSION_ACTIVITY_EVIDENCE", &evidence_path)
        .stdin(Stdio::null())
        .spawn()
        .expect("browser driver");

    let deadline = Instant::now() + Duration::from_secs(150);
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
    assert!(status.success(), "real browser acceptance failed");

    let evidence = std::fs::read_to_string(&evidence_path).expect("Agent evidence");
    let records: Vec<serde_json::Value> = evidence
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("Agent evidence line"))
        .collect();
    let field = |record: &serde_json::Value, name: &str| {
        record[name]
            .as_str()
            .map_or_else(|| panic!("evidence field {name}: {record}"), str::to_owned)
    };
    let record_type = |record: &serde_json::Value| {
        record
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned()
    };

    // One Agent session, created for exactly the directory the browser selected.
    let sessions: Vec<&serde_json::Value> = records
        .iter()
        .filter(|record| record_type(record) == "session_new")
        .collect();
    assert_eq!(sessions.len(), 1, "exactly one Agent session: {evidence}");
    let reported_cwd = Path::new(&field(sessions[0], "cwd"))
        .canonicalize()
        .expect("Agent cwd");
    assert_eq!(
        reported_cwd,
        workspace.canonicalize().expect("workspace"),
        "the directory chosen in the PWA must reach the Agent"
    );

    // One dispatch per turn: page switching and reconnecting must not re-send or cancel.
    let prompts: Vec<String> = records
        .iter()
        .filter(|record| record_type(record) == "prompt")
        .map(|record| field(record, "text"))
        .collect();
    let expected: Vec<String> = [
        "hold-turn",
        "permissions-turn",
        "fail-turn",
        "recover-turn",
        "disconnect-turn",
    ]
    .iter()
    .map(|text| (*text).to_owned())
    .collect();
    assert_eq!(
        prompts, expected,
        "unexpected prompt dispatches: {evidence}"
    );
    assert!(
        !records.iter().any(|record| record_type(record) == "cancel"),
        "a page switch or reconnect cancelled the running turn: {evidence}"
    );

    // Both permission requests reached the device and were answered with the option the UI showed.
    let requests = records
        .iter()
        .filter(|record| record_type(record) == "permission_request")
        .count();
    assert_eq!(requests, 2, "two permission requests: {evidence}");
    let answers: Vec<&serde_json::Value> = records
        .iter()
        .filter(|record| record_type(record) == "permission_response")
        .collect();
    assert_eq!(answers.len(), 2, "two permission answers: {evidence}");
    for answer in answers {
        assert_eq!(field(answer, "optionId"), "allow_once");
        assert_eq!(field(answer, "outcome"), "selected");
    }
    assert!(
        !records
            .iter()
            .any(|record| record_type(record) == "unexpected_response"),
        "Agent received a response for an unknown request: {evidence}"
    );

    assert!(daemon.stop().success());
    // Only this explicitly constructed per-process directory is removed.
    assert!(workspace.starts_with(repo.join("target")));
    std::fs::remove_dir_all(&workspace).expect("clean workspace");
}
