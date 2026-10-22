//! Opt-in real PWA → TLS Daemon acceptance for **session preparation before the first message**
//! (`web-session-preparation` task 8.2; PV8).
//!
//! Requires a built `clients/app/dist` embedded in the Daemon binary, a local Chromium/Edge and
//! `node` on `PATH`. The Daemon is the real binary with a real SQLite database, real identity,
//! real Sync handshake and the real attachment endpoint; the Agent is a controllable Node stdio
//! server configured through `[agent]` so the browser flow can exercise real modes/config and the
//! attachment upload path deterministically. The **real** `codex-acp` cross-process behaviour is
//! covered by `scripts/probe-session-preparation.mjs`; the two layers are deliberately separate.
//!
//! The Node driver (`clients/app/e2e/session-preparation-live.mjs`) asserts on DOM hooks and real
//! wire frames. This test prepares the daemon/pairing, runs the driver, and then asserts on the
//! **Agent side of the wire** (`session/new`/`prompt`/`set_mode`/`set_config_option` evidence) and
//! on the persisted rows after a graceful stop:
//!
//! - exactly one `session/new` and **zero** `session/resume` (no fallback to formal resume);
//! - exactly one `prompt`, whose text equals the submitted first message (no hidden prompt);
//! - the non-default mode/config chosen in the page reached the adapter;
//! - exactly one formal session and exactly one persisted user message with the submitted text,
//!   whose `owned_first_submission` row is terminal and keeps the first request identity.
//!
//! Run explicitly:
//! `cargo test -p app --test session_preparation_browser -- --ignored --nocapture`
mod support;

use serde_json::{Value, json};
use server::local_admin::Method;
use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Minimal ACP Agent used by the browser acceptance.
///
/// It advertises two non-default choices (a third mode and a select config option), records every
/// `session/new`/`prompt`/`set_mode`/`set_config_option`/`session/resume`/`session/close` into an
/// evidence file, and answers a prompt with the token the page asked for. It never resumes a
/// session on its own and never prompts without being asked.
const MOCK_AGENT: &str = r#"
const fs = require('fs');
const readline = require('readline');
const evidence = process.argv[2];
const append = (record) => fs.appendFileSync(evidence, JSON.stringify(record) + '\n');
const send = (message) => process.stdout.write(JSON.stringify(message) + '\n');
let next = 0;
let currentMode = 'agent';
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
        agentInfo: { name: 'session-preparation-browser-agent', version: '1' },
      },
    });
    return;
  }
  if (message.method === 'session/new') {
    const sessionId = 'preparation-agent-' + (++next);
    append({ type: 'session_new', sessionId, cwd: message.params ? message.params.cwd : null });
    send({
      jsonrpc: '2.0',
      id: message.id,
      result: {
        sessionId,
        modes: {
          currentModeId: currentMode,
          availableModes: [
            { id: 'agent', name: 'Auto review' },
            { id: 'read-only', name: 'Read-only' },
            { id: 'workspace-write', name: 'Workspace access' },
          ],
        },
        configOptions: [
          {
            id: 'reasoning',
            name: 'Reasoning',
            type: 'select',
            currentValue: 'medium',
            options: [
              { value: 'low', name: 'Low' },
              { value: 'medium', name: 'Medium' },
              { value: 'high', name: 'High' },
            ],
          },
        ],
      },
    });
    return;
  }
  if (message.method === 'session/set_mode') {
    currentMode = message.params ? message.params.modeId : null;
    append({ type: 'set_mode', modeId: currentMode });
    send({ jsonrpc: '2.0', id: message.id, result: {} });
    return;
  }
  if (message.method === 'session/set_config_option') {
    const params = message.params || {};
    append({ type: 'set_config', configId: params.configId, value: params.value });
    send({ jsonrpc: '2.0', id: message.id, result: {} });
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
    const blocks = message.params && Array.isArray(message.params.prompt) ? message.params.prompt : [];
    const text = blocks.map((block) => (block && typeof block.text === 'string' ? block.text : '')).join('');
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

fn read_json_lines(path: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or(Value::Null))
        .collect()
}

fn count_type(records: &[Value], kind: &str) -> usize {
    records.iter().filter(|record| record["type"] == json!(kind)).count()
}

#[test]
#[ignore = "requires a local Chromium browser, node on PATH and a Web UI embedded in the Daemon"]
fn real_browser_prepares_then_commits_across_reload() {
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("port")
        .local_addr()
        .expect("address")
        .port();
    let mut daemon = support::Daemon::configure_with_bind(
        "session-preparation-browser",
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
    let lifecycle_evidence = daemon.root().join("preparation-evidence.json");
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
    // 目录选择器默认隐藏系统/隐藏目录：把工作目录放在用户目录下，浏览器第一屏就能打开它。
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .expect("user profile");
    let workspace = home.join(format!("acpr-preparation-browser-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&workspace);
    std::fs::create_dir_all(&workspace).expect("workspace");

    let mut browser = Command::new("node")
        .arg(repo.join("clients/app/e2e/session-preparation-live.mjs"))
        .env(
            "SESSION_PREPARATION_ORIGIN",
            format!("https://localhost:{port}"),
        )
        .env(
            "SESSION_PREPARATION_CODE",
            pairing["pairingCode"].as_str().expect("pairing code"),
        )
        .env("SESSION_PREPARATION_WORKSPACE", &workspace)
        .env("SESSION_PREPARATION_EVIDENCE", &lifecycle_evidence)
        .stdin(Stdio::null())
        .spawn()
        .expect("browser driver");

    let deadline = Instant::now() + Duration::from_secs(420);
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
    assert!(status.success(), "real browser preparation acceptance failed");

    // ── 浏览器驱动报告 ─────────────────────────────────────────────────────────────────────
    let evidence = std::fs::read_to_string(&lifecycle_evidence).expect("browser evidence");
    let report: Value = serde_json::from_str(&evidence).expect("browser evidence JSON");
    assert_eq!(report["exitCode"], json!(0), "{evidence}");
    let scenarios: Vec<&str> = report["scenarios"]
        .as_array()
        .expect("scenarios")
        .iter()
        .filter(|scenario| scenario["status"] == json!("passed"))
        .filter_map(|scenario| scenario["name"].as_str())
        .collect();
    for required in [
        "allocate-shows-real-settings-and-keeps-the-url",
        "draft-and-attachment-survive-reload",
        "lease-renewal-survives-thirty-seconds",
        "first-submit-commits-once-and-keeps-the-url",
        "formal-session-reads-back-after-reload",
        "unknown-session-id-returns-to-directory",
    ] {
        assert!(
            scenarios.contains(&required),
            "浏览器驱动缺少通过的场景 {required}：{evidence}"
        );
    }
    let first_message = report["firstMessage"].as_str().expect("firstMessage");
    let selected_mode = report["selectedMode"].as_str().expect("selectedMode");
    let selected_config = report["selectedConfig"].as_object().expect("selectedConfig");
    let selected_config_id = selected_config["id"].as_str().expect("selectedConfig.id");
    let selected_config_value = selected_config["value"].as_str().expect("selectedConfig.value");
    assert_ne!(selected_mode, "agent", "浏览器必须选择一个非默认模式");
    assert_ne!(selected_config_value, "medium", "浏览器必须选择一个非默认配置值");
    let attachment_sha = report["expectedAttachmentSha256"]
        .as_str()
        .expect("expectedAttachmentSha256");
    assert_eq!(attachment_sha.len(), 64, "附件摘要必须是 64 位 hex");
    let timings = &report["timings"];
    for key in [
        "submitToAttachmentBeginMs",
        "attachmentBeginToSealAckMs",
        "sealToManifestClaimMs",
        "manifestClaimToStartSentMs",
        "submitToAcceptedMs",
        "acceptedToCompletedMs",
        "totalSubmitToCompletedMs",
    ] {
        let value = timings[key].as_i64().unwrap_or_else(|| panic!("缺少分段耗时 {key}：{timings}"));
        assert!(value >= 0, "分段耗时 {key} 不能为负：{timings}");
    }
    let session_id = report["scenarios"]
        .as_array()
        .expect("scenarios")
        .iter()
        .find(|scenario| scenario["name"] == json!("first-submit-commits-once-and-keeps-the-url"))
        .and_then(|scenario| scenario["details"]["sessionId"].as_str())
        .expect("首条场景没有记录 sessionId")
        .to_owned();
    let request_id = report["scenarios"]
        .as_array()
        .expect("scenarios")
        .iter()
        .find(|scenario| scenario["name"] == json!("first-submit-commits-once-and-keeps-the-url"))
        .and_then(|scenario| scenario["details"]["requestId"].as_str())
        .expect("首条场景没有记录 requestId")
        .to_owned();

    // ── Agent 边界证据（真实 stdio 服务自己写的）────────────────────────────────────────────
    let records = read_json_lines(&evidence_path);
    assert_eq!(
        count_type(&records, "session_new"),
        1,
        "一次准备 + 一次提交只能建立一个 Agent 会话：{records:?}"
    );
    assert_eq!(
        count_type(&records, "session_resume"),
        0,
        "准备/首条路径 MUST NOT 退回旧正式恢复：{records:?}"
    );
    let prompts: Vec<&Value> = records.iter().filter(|record| record["type"] == json!("prompt")).collect();
    assert_eq!(prompts.len(), 1, "适配器必须恰好收到一次 prompt：{records:?}");
    assert_eq!(
        prompts[0]["text"].as_str().expect("prompt text"),
        first_message,
        "唯一的 prompt 必须与提交正文逐字相同（没有隐藏 prompt）"
    );
    let modes: Vec<&str> = records
        .iter()
        .filter(|record| record["type"] == json!("set_mode"))
        .filter_map(|record| record["modeId"].as_str())
        .collect();
    assert!(
        modes.contains(&selected_mode),
        "非默认模式必须真的下发到适配器（{selected_mode}）：{records:?}"
    );
    let configs: Vec<(&str, Value)> = records
        .iter()
        .filter(|record| record["type"] == json!("set_config"))
        .map(|record| {
            (
                record["configId"].as_str().unwrap_or_default(),
                record["value"].clone(),
            )
        })
        .collect();
    assert!(
        configs.iter().any(|(id, value)| *id == selected_config_id
            && value["value"] == json!(selected_config_value)),
        "非默认配置必须真的下发到适配器（{selected_config_id}={selected_config_value}）：{records:?}"
    );

    // ── 正式持久化（优雅停止后再读同一 data_dir 的 SQLite）────────────────────────────────
    let _ = daemon.stop();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let storage = storage_sqlite::migrate::StorageConfig::new(daemon.data_dir());
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(storage.database_path())
            .create_if_missing(false)
            .busy_timeout(Duration::from_secs(10));
        let mut connection = sqlx::Connection::connect_with(&options)
            .await
            .expect("打开同一 data_dir 的原始 SQL 连接");
        let session_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM owned_session WHERE session_id = ?1")
                .bind(&session_id)
                .fetch_one(&mut connection)
                .await
                .expect("统计 owned_session");
        assert_eq!(session_rows, 1, "正式会话必须恰好一行");
        let messages: Vec<(String,)> = sqlx::query_as(
            "SELECT content_json FROM owned_message WHERE session_id = ?1 AND channel = 'user'",
        )
        .bind(&session_id)
        .fetch_all(&mut connection)
        .await
        .expect("读用户消息");
        assert_eq!(messages.len(), 1, "首条用户消息必须恰好一条（回显不重复）");
        let content: Value = serde_json::from_str(&messages[0].0).expect("content_json");
        let text: String = content
            .as_array()
            .expect("content blocks")
            .iter()
            .filter(|block| block["type"] == json!("text"))
            .filter_map(|block| block["text"].as_str())
            .collect();
        assert_eq!(text, first_message, "持久化的首条正文必须逐字相同");
        let submission: Option<(String, String, String)> = sqlx::query_as(
            "SELECT first_request_id, current_request_id, stage FROM owned_first_submission \
             WHERE session_id = ?1",
        )
        .bind(&session_id)
        .fetch_optional(&mut connection)
        .await
        .expect("读 owned_first_submission");
        let (first_request, current_request, stage) = submission.expect("必须有首条关联行");
        assert_eq!(first_request, request_id, "首条请求身份不得被改写");
        assert_eq!(current_request, request_id, "首个尝试就是当前尝试");
        assert_eq!(stage, "terminal", "首条轮次结束后阶段必须是 terminal");
        let attachments: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM owned_attachment_link WHERE session_id = ?1")
                .bind(&session_id)
                .fetch_one(&mut connection)
                .await
                .expect("统计附件引用");
        assert!(attachments >= 1, "首条必须把准备附件转成正式引用");
        drop(connection);
    });

    let _ = std::fs::remove_dir_all(&workspace);
}
