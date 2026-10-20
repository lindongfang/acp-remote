//! Opt-in real PWA → TLS Daemon → disk test. Requires built clients/app/dist and Chromium.
mod support;

use serde_json::json;
use server::local_admin::Method;
use std::{
    net::TcpListener,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires a local Chromium browser and built Web UI"]
fn real_browser_creates_three_levels_then_selects_and_cancels() {
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("port")
        .local_addr()
        .expect("address")
        .port();
    let mut daemon =
        support::Daemon::configure_with_bind("mkdir-browser", "127.0.0.1", port, "", "");
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).expect("certificate");
    let cert_path = daemon.root().join("cert.pem");
    let key_path = daemon.root().join("key.pem");
    let mock_path = daemon.root().join("agent.cjs");
    let cwd_path = daemon.root().join("agent-cwd.txt");
    std::fs::write(&mock_path, r#"
const fs = require('fs');
fs.writeFileSync(process.argv[2] + '.spawn', 'started');
const lines = require('readline').createInterface({input:process.stdin});
lines.on('line', line => {
  const message = JSON.parse(line);
  if (message.id === undefined) return;
  let result = {};
  if (message.method === 'initialize') result = {protocolVersion:1,agentCapabilities:{},agentInfo:{name:'mkdir-test',version:'1'}};
  if (message.method === 'session/new') {
    process.chdir(message.params.cwd);
    fs.writeFileSync(process.argv[2], JSON.stringify({cwd:process.cwd(), requested:message.params.cwd}));
    result = {sessionId:'mkdir-browser-session'};
  }
  console.log(JSON.stringify({jsonrpc:'2.0',id:message.id,result}));
});
"#).expect("controlled Agent");
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
            cert_path.to_string_lossy().replace('\\', "/"),
            key_path.to_string_lossy().replace('\\', "/"),
            mock_path.to_string_lossy().replace('\\', "/"),
            cwd_path.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("TLS configuration");
    daemon.start();
    let pairing = daemon.ok_value(
        Method::DevicePairBegin,
        support::params_of(json!({"expiresInMs": null})),
    );
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository");
    let repo = std::path::PathBuf::from(repo.to_string_lossy().trim_start_matches(r"\\?\"));
    let parent = repo
        .join("target")
        .join(format!("mkdir-browser-{}", std::process::id()));
    std::fs::create_dir(&parent).expect("isolated browser directory");
    let mut browser = Command::new("node")
        .arg(repo.join("clients/app/e2e/workspace-mkdir-live.mjs"))
        .env("MKDIR_TEST_ORIGIN", format!("https://localhost:{port}"))
        .env(
            "MKDIR_TEST_CODE",
            pairing["pairingCode"].as_str().expect("pairing code"),
        )
        .env("MKDIR_TEST_PARENT", &parent)
        .env("MKDIR_TEST_AGENT_CWD", &cwd_path)
        .stdin(Stdio::null())
        .spawn()
        .expect("browser driver");
    let deadline = Instant::now() + Duration::from_secs(90);
    let pairing_id = pairing["pairingId"].as_str().expect("pairing id");
    let mut approved = false;
    let status = loop {
        if let Some(status) = browser.try_wait().expect("browser status") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = browser.kill();
            let _ = browser.wait();
            panic!("browser test timeout");
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
    assert!(status.success(), "real browser test failed");
    assert!(parent.join("project/src/api").is_dir());
    assert!(parent.join("kept-after-cancel").is_dir());
    let agent: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cwd_path).expect("Agent cwd evidence"))
            .expect("Agent cwd JSON");
    assert_eq!(
        std::path::Path::new(agent["cwd"].as_str().expect("Agent cwd"))
            .canonicalize()
            .expect("actual cwd"),
        parent
            .join("project/src/api")
            .canonicalize()
            .expect("chosen directory")
    );
    assert!(daemon.stop().success());
    // Only this explicitly constructed per-process directory is removed.
    assert!(parent.starts_with(repo.join("target")));
    std::fs::remove_dir_all(parent).expect("clean isolated browser directory");
}
