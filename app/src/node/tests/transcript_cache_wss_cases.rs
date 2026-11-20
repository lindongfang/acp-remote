//! Windows real-node HTTPS/WSS end-to-end acceptance tests for `deliver-session-transcript-cache`
//! covering `STC-003`..`STC-010`, `STC-014`, and `STC-015` under default Cargo features.

use super::*;
use crate::application::shared_views::wire::{RecordKind, TransferHeader};
use crate::domain::EntityId;
use crate::domain::native::NativeSessionKey;
use crate::domain::native_catalog::{CatalogDraft, CatalogResult, SourceSelection};
use crate::domain::source_sessions::SourceLaunchSelection;
use crate::storage::SCHEMA_VERSION;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REQ_SEQ: AtomicU64 = AtomicU64::new(1);

const fn tc_id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}

fn tc_deadline() -> crate::application::request_registry::RegistryDeadline {
    crate::application::request_registry::RegistryDeadline::new(
        tokio::time::Instant::now() + Duration::from_secs(15),
    )
}

fn utf16le_b64(value: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(
        value
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

fn find_node_executable() -> PathBuf {
    for candidate in [
        r"D:\Application\Node\nodejs\node.exe",
        r"C:\Program Files\nodejs\node.exe",
    ] {
        let p = PathBuf::from(candidate);
        if p.is_file() {
            return p;
        }
    }
    if let Ok(output) = std::process::Command::new("where.exe").arg("node").output()
        && output.status.success()
        && let Some(line) = String::from_utf8_lossy(&output.stdout).lines().next()
    {
        let trimmed = PathBuf::from(line.trim());
        if trimmed.is_file() {
            return trimmed;
        }
    }
    panic!("node.exe executable not found for ACP test peer harness");
}

fn write_controlled_acp_peer(dir: &std::path::Path) -> PathBuf {
    let script = dir.join("controlled-acp-peer.mjs");
    let source = r#"
import fs from "node:fs";
import readline from "node:readline";

const cfgPath = process.env.ACP_PEER_CONFIG_FILE;
const logPath = process.env.ACP_PEER_LOG_FILE;

function readCfg() {
  return JSON.parse(fs.readFileSync(cfgPath, "utf8"));
}

function logEvent(entry) {
  if (logPath) {
    fs.appendFileSync(logPath, JSON.stringify({ pid: process.pid, ...entry }) + "\n");
  }
}

function send(msg) {
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", ...msg }) + "\n");
}

const initCfg = readCfg();
if (initCfg.failSpawn) {
  logEvent({ event: "failSpawn" });
  process.exit(1);
}

let pendingPermResolve = null;
let reqSeq = 1000;
let sessionSeq = Number(process.env.ACP_PEER_SESSION_START || "1");

async function handleRequest(msg) {
  const cfg = readCfg();
  const { id, method, params } = msg;
  logEvent({ method, params });

  if (method === "initialize") {
    const version = Number(cfg.protocolVersion ?? 1);
    const mode = cfg.capabilityMode ?? "replay_capable";
    if (version === 2) {
      send({
        id,
        result: {
          protocolVersion: 2,
          info: { name: "controlled-acp", version: "2.0.0" },
          capabilities: { session: {} },
        },
      });
      return;
    }
    const loadSession = mode === "replay_capable" || mode === "replay_and_resume";
    const sessionCapabilities = { close: {}, list: {} };
    if (mode !== "pure_memory") {
      sessionCapabilities.resume = {};
    }
    send({
      id,
      result: {
        protocolVersion: 1,
        agentInfo: { name: "controlled-acp", version: "1.0.0" },
        agentCapabilities: { loadSession, sessionCapabilities },
      },
    });
    return;
  }

  if (method === "session/new") {
    const sid = cfg.fixedSessionId || `peer-session-${sessionSeq++}-${process.pid}`;
    send({ id, result: { sessionId: sid } });
    return;
  }

  if (method === "session/load" || (method === "session/resume" && params?.replayFrom)) {
    if (cfg.loadBarrierEnteredFile) {
      fs.writeFileSync(cfg.loadBarrierEnteredFile, "entered");
    }
    if (cfg.loadBarrierReleaseFile) {
      while (!fs.existsSync(cfg.loadBarrierReleaseFile)) {
        await new Promise((r) => setTimeout(r, 20));
      }
    }
    const latest = readCfg();
    if (latest.failLoadMessage) {
      send({
        id,
        error: { code: Number(latest.failLoadCode ?? -32001), message: latest.failLoadMessage },
      });
      return;
    }
    const sid = params.sessionId;
    const replayTexts = Array.isArray(latest.replayTexts) ? latest.replayTexts : [];
    for (let i = 0; i < replayTexts.length; i++) {
      send({
        method: "session/update",
        params: {
          sessionId: sid,
          update: {
            sessionUpdate: "agent_message_chunk",
            messageId: `replay-msg-${i + 1}`,
            content: { type: "text", text: replayTexts[i] },
          },
        },
      });
    }
    send({ id, result: { sessionId: sid } });
    return;
  }

  if (method === "session/resume") {
    if (cfg.failResumeMessage) {
      send({ id, error: { code: -32002, message: cfg.failResumeMessage } });
      return;
    }
    const version = Number(cfg.protocolVersion ?? 1);
    if (version === 2) {
      send({
        method: "session/update",
        params: {
          sessionId: params.sessionId,
          update: {
            sessionUpdate: "state_update",
            state: "idle",
          },
        },
      });
    }
    send({ id, result: { sessionId: params.sessionId } });
    return;
  }

  if (method === "session/prompt") {
    const sid = params.sessionId;
    const version = Number(cfg.protocolVersion ?? 1);
    const promptItems = Array.isArray(params.prompt) ? params.prompt : [];
    const promptText = promptItems.map((b) => b.text ?? "").join("");
    if (cfg.requestPermission) {
      const permId = `perm-${reqSeq++}`;
      const permWait = new Promise((resolve) => {
        pendingPermResolve = resolve;
      });
      if (version === 2) {
        send({
          id: permId,
          method: "session/request_permission",
          params: {
            sessionId: sid,
            title: "Allow step",
            options: [{ optionId: "allow_once", name: "Allow", kind: "allow_once" }],
          },
        });
      } else {
        send({
          id: permId,
          method: "session/request_permission",
          params: {
            sessionId: sid,
            toolCall: { toolCallId: `tc-${reqSeq}`, name: "allow_step" },
            options: [{ optionId: "allow_once", name: "Allow", kind: "allow_once" }],
          },
        });
      }
      await permWait;
    }
    const suffix = cfg.replyPrefix ?? "reply:";
    const msgId = `reply-${Date.now()}-${reqSeq++}`;
    send({
      method: "session/update",
      params: {
        sessionId: sid,
        update: {
          sessionUpdate: "agent_message_chunk",
          messageId: msgId,
          content: { type: "text", text: `${suffix}${promptText}` },
        },
      },
    });
    if (version === 2) {
      send({
        method: "session/update",
        params: {
          sessionId: sid,
          update: {
            sessionUpdate: "state_update",
            state: "idle",
            stopReason: "end_turn",
          },
        },
      });
      send({ id, result: { messageId: msgId } });
    } else {
      send({ id, result: { stopReason: "end_turn" } });
    }
    return;
  }

  if (method === "session/list") {
    send({ id, result: { sessions: Array.isArray(cfg.listedSessions) ? cfg.listedSessions : [] } });
    return;
  }

  if (method === "session/close") {
    send({ id, result: {} });
    return;
  }

  send({ id, error: { code: -32601, message: `unknown method ${method}` } });
}

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
rl.on("line", (line) => {
  if (!line.trim()) return;
  const msg = JSON.parse(line);
  if (msg.method) {
    void handleRequest(msg);
  } else if (msg.id && pendingPermResolve) {
    const r = pendingPermResolve;
    pendingPermResolve = null;
    r(msg.result);
  }
});
"#;
    std::fs::write(&script, source).expect("write controlled acp peer script");
    script
}

#[derive(Default)]
struct PeerConfigBuilder {
    protocol_version: u8,
    capability_mode: String,
    fail_spawn: bool,
    fail_load_message: Option<String>,
    fail_load_code: Option<i64>,
    fail_resume_message: Option<String>,
    request_permission: bool,
    load_barrier_entered_file: Option<String>,
    load_barrier_release_file: Option<String>,
    replay_texts: Vec<String>,
    reply_prefix: String,
    listed_sessions: Vec<serde_json::Value>,
}

impl PeerConfigBuilder {
    fn replay_capable_v1() -> Self {
        Self {
            protocol_version: 1,
            capability_mode: "replay_capable".to_owned(),
            reply_prefix: "reply:".to_owned(),
            ..Self::default()
        }
    }

    fn write(&self, path: &std::path::Path) {
        let obj = serde_json::json!({
            "protocolVersion": self.protocol_version,
            "capabilityMode": self.capability_mode,
            "failSpawn": self.fail_spawn,
            "failLoadMessage": self.fail_load_message,
            "failLoadCode": self.fail_load_code,
            "failResumeMessage": self.fail_resume_message,
            "requestPermission": self.request_permission,
            "loadBarrierEnteredFile": self.load_barrier_entered_file,
            "loadBarrierReleaseFile": self.load_barrier_release_file,
            "replayTexts": self.replay_texts,
            "replyPrefix": self.reply_prefix,
            "listedSessions": self.listed_sessions,
        });
        std::fs::write(
            path,
            serde_json::to_vec(&obj).expect("serialize peer config"),
        )
        .expect("write peer config json");
    }
}

fn encode_acpi_upload(upload_id: Id128, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(32 + payload.len());
    frame.extend_from_slice(b"ACPI");
    frame.push(1);
    frame.push(1);
    frame.push(0);
    frame.push(0);
    frame.extend_from_slice(upload_id.as_bytes());
    frame.extend_from_slice(&0_u64.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

#[derive(Default)]
struct DrainedStreamFrames {
    statuses: Vec<serde_json::Value>,
    projections: Vec<serde_json::Value>,
    snapshot_ends: usize,
}

async fn send_command_wait(
    socket: &mut ManagementSocket,
    action: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    let seq = NEXT_REQ_SEQ.fetch_add(1, Ordering::Relaxed);
    let request_id = Id128::new(u128::from(seq).to_be_bytes());
    let request_id_str = request_id.to_base64url();
    send_json(
        socket,
        &serde_json::json!({
            "v": 1,
            "type": "command",
            "requestId": request_id_str,
            "action": action,
            "payload": payload,
        }),
    )
    .await;

    timeout(Duration::from_secs(12), async {
        loop {
            match socket.next().await.expect("ws frame").expect("ws ok") {
                Message::Text(text) => {
                    let value: serde_json::Value =
                        serde_json::from_str(text.as_str()).expect("json frame");
                    if value.get("requestId").and_then(|v| v.as_str())
                        == Some(request_id_str.as_str())
                    {
                        return value;
                    }
                }
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
                Message::Binary(bytes) => {
                    if let Ok((hdr, _)) = TransferHeader::decode(&bytes) {
                        let credit = TransferHeader::credit(hdr.stream(), hdr.packet())
                            .encode(&[])
                            .expect("credit");
                        socket
                            .send(Message::Binary(credit.into()))
                            .await
                            .expect("send credit");
                    }
                }
                Message::Close(frame) => {
                    panic!("unexpected close while waiting for {action}: {frame:?}");
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for response to {action}"))
}

async fn read_until_snapshot_end(
    socket: &mut ManagementSocket,
    stream_id: Id128,
) -> DrainedStreamFrames {
    let mut drained = DrainedStreamFrames::default();
    let mut buffers: std::collections::HashMap<(u8, u64), Vec<u8>> =
        std::collections::HashMap::new();
    let initial_credit = TransferHeader::credit(stream_id, 0)
        .encode(&[])
        .expect("credit 0");
    socket
        .send(Message::Binary(initial_credit.into()))
        .await
        .expect("send initial credit");

    let mut received_kinds: Vec<String> = Vec::new();
    let res = timeout(Duration::from_secs(10), async {
        loop {
            match socket.next().await.expect("ws frame").expect("ws ok") {
                Message::Binary(bytes) => {
                    let (hdr, payload) =
                        TransferHeader::decode(&bytes).expect("decode transfer hdr");
                    if hdr.stream() == stream_id {
                        let credit = TransferHeader::credit(stream_id, hdr.packet())
                            .encode(&[])
                            .expect("ack credit");
                        socket
                            .send(Message::Binary(credit.into()))
                            .await
                            .expect("send ack");
                        received_kinds.push(format!(
                            "{:?}(end={})",
                            hdr.record_kind(),
                            hdr.is_last()
                        ));
                        if let Some(kind) = hdr.record_kind() {
                            if kind == RecordKind::SnapshotEnd {
                                drained.snapshot_ends += 1;
                                break;
                            }
                            let tag = match kind {
                                RecordKind::OriginalRaw => 1_u8,
                                RecordKind::ItemProjection => 2,
                                RecordKind::Status => 3,
                                RecordKind::SnapshotEnd => 4,
                            };
                            let buf = buffers.entry((tag, hdr.record())).or_default();
                            buf.extend_from_slice(payload);
                            if hdr.is_last() {
                                let full = buffers.remove(&(tag, hdr.record())).unwrap_or_default();
                                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&full)
                                {
                                    if kind == RecordKind::Status {
                                        let is_rebound = val.get("type").and_then(|t| t.as_str())
                                            == Some("stream_rebound");
                                        drained.statuses.push(val);
                                        if is_rebound {
                                            break;
                                        }
                                    } else if kind == RecordKind::ItemProjection {
                                        drained.projections.push(val);
                                    }
                                }
                            }
                        }
                    }
                }
                Message::Text(txt) => {
                    received_kinds.push(format!("Text({})", &txt[..txt.len().min(120)]));
                }
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
                Message::Close(frame) => {
                    panic!("unexpected close during snapshot read: {frame:?}");
                }
            }
        }
    })
    .await;
    if let Err(elapsed) = res {
        panic!("timed out waiting for SnapshotEnd ({elapsed}): received={received_kinds:?}");
    }
    drained
}

async fn read_next_status_matching<F>(
    socket: &mut ManagementSocket,
    stream_id: Id128,
    mut predicate: F,
    wait: Duration,
) -> serde_json::Value
where
    F: FnMut(&serde_json::Value) -> bool,
{
    let mut buffers: std::collections::HashMap<(u8, u64), Vec<u8>> =
        std::collections::HashMap::new();
    timeout(wait, async {
        loop {
            match socket.next().await.expect("ws frame").expect("ws ok") {
                Message::Binary(bytes) => {
                    let (hdr, payload) = TransferHeader::decode(&bytes).expect("decode hdr");
                    if hdr.stream() == stream_id {
                        let credit = TransferHeader::credit(stream_id, hdr.packet())
                            .encode(&[])
                            .expect("credit");
                        socket
                            .send(Message::Binary(credit.into()))
                            .await
                            .expect("send credit");
                        if hdr.record_kind() == Some(RecordKind::Status) {
                            let buf = buffers.entry((3, hdr.record())).or_default();
                            buf.extend_from_slice(payload);
                            if hdr.is_last() {
                                let full = buffers.remove(&(3, hdr.record())).unwrap_or_default();
                                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&full)
                                    && predicate(&val)
                                {
                                    return val;
                                }
                            }
                        }
                    }
                }
                Message::Text(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
                Message::Close(frame) => panic!("unexpected close waiting for status: {frame:?}"),
            }
        }
    })
    .await
    .expect("timed out waiting for matching binary status frame")
}

struct HarnessFixture {
    area: TestArea,
    cfg_path: PathBuf,
    log_path: PathBuf,
    script_path: PathBuf,
    node_exe: PathBuf,
    workspace_path: String,
    source_id: EntityId,
    workspace_id: EntityId,
}

impl HarnessFixture {
    fn new(name: &str) -> Self {
        let area = TestArea::new();
        let ws = area.directory.join("workspace");
        std::fs::create_dir_all(&ws).expect("create workspace");
        let cfg_path = area.directory.join("peer-config.json");
        let log_path = area.directory.join("peer-log.jsonl");
        let script_path = write_controlled_acp_peer(&area.directory);
        let node_exe = find_node_executable();
        PeerConfigBuilder::replay_capable_v1().write(&cfg_path);
        let workspace_path = std::fs::canonicalize(&ws)
            .expect("canonical workspace")
            .to_string_lossy()
            .into_owned();
        Self {
            area,
            cfg_path,
            log_path,
            script_path,
            node_exe,
            workspace_path,
            source_id: tc_id(
                name.as_bytes()
                    .first()
                    .copied()
                    .map_or(21, |b| b.wrapping_add(11)),
            ),
            workspace_id: tc_id(91),
        }
    }

    async fn install_profile(
        &self,
        node: &NodeRuntime,
        wire_version: u8,
        key_seed: u8,
    ) -> (String, SigningKey, SourceLaunchSelection) {
        let pool = node
            .storage
            .as_ref()
            .expect("storage")
            .pool_for_test()
            .clone();
        sqlx::query("INSERT OR IGNORE INTO product_sources VALUES(?,'stc test source')")
            .bind(self.source_id.encode())
            .execute(&pool)
            .await
            .expect("insert source");
        sqlx::query("INSERT OR IGNORE INTO workspaces VALUES(?,?)")
            .bind(self.workspace_id.encode())
            .bind(&self.workspace_path)
            .execute(&pool)
            .await
            .expect("insert workspace");

        let key = SigningKey::from_slice(&[key_seed; 32]).expect("signing key");
        let client_id = pair_browser(node, &key, "STC Client").await;
        let observed = node
            .management
            .as_ref()
            .expect("management")
            .observe_next_document_socket_for_test()
            .expect("observe socket");
        let mut ctrl_keep = authenticate_socket(
            valid_management_socket(&self.area, node.listen_addr()).await,
            &key,
            &client_id,
            node_public_key(node),
        )
        .await;
        let endpoint = observed.await.expect("endpoint");
        let endpoint_client = endpoint.client();
        let actor = endpoint_client.context.actor();

        let raw_profile = format!(
            r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}"],"environment":[{{"name_utf16le_base64":"{}","value_utf16le_base64":"{}"}},{{"name_utf16le_base64":"{}","value_utf16le_base64":"{}"}}]}},"recipes":[{{"wire":{wire_version},"mcp_servers":[],"additional_directories":[],"extra":{{}}}}]}}"#,
            utf16le_b64(&self.node_exe.to_string_lossy()),
            utf16le_b64(&self.script_path.to_string_lossy()),
            utf16le_b64("ACP_PEER_CONFIG_FILE"),
            utf16le_b64(&self.cfg_path.to_string_lossy()),
            utf16le_b64("ACP_PEER_LOG_FILE"),
            utf16le_b64(&self.log_path.to_string_lossy()),
        );
        let parsed = crate::adapters::acp::node_configuration_profile::parse_profile(
            raw_profile.into_bytes(),
        )
        .expect("parse profile");
        let draft = CatalogDraft::install_harness(
            "STC Controlled ACP".into(),
            SourceSelection::Existing(self.source_id),
            parsed,
        )
        .expect("install harness draft");
        let services = node.session_services.as_ref().expect("session services");
        let reg = services
            .catalog
            .register(actor, draft, &tc_deadline())
            .await
            .expect("catalog register");
        let applied = services
            .catalog
            .apply(actor, reg, &tc_deadline())
            .await
            .expect("catalog apply");
        let (harness, profile) = match applied.fact().result().expect("catalog result") {
            CatalogResult::Harness {
                harness, profile, ..
            } => (*harness, *profile),
            other => panic!("unexpected catalog result: {other:?}"),
        };
        let selection = SourceLaunchSelection {
            source: self.source_id,
            harness,
            profile,
            workspace: self.workspace_id,
            launch_path: self.workspace_path.clone(),
        };
        let _ = ctrl_keep.close(None).await;
        (client_id, key, selection)
    }

    /// Creates a real root-verified ACP session via `source.session.register`+`execute(create)` and binds a target to it.
    async fn create_and_bind_target(
        &self,
        node: &NodeRuntime,
        sock: &mut ManagementSocket,
        key: &SigningKey,
        client_id: &str,
        selection: &SourceLaunchSelection,
    ) -> (EntityId, String) {
        let reg = send_command_wait(
            sock,
            "source.session.register",
            serde_json::json!({
                "selection": selection_json(selection),
                "operation": { "kind": "create" },
            }),
        )
        .await;
        assert_eq!(
            reg["ok"], true,
            "source.session.register(create) failed: {reg}"
        );
        let exec = send_command_wait(
            sock,
            "source.session.execute",
            serde_json::json!({ "sourceOperationId": reg["result"]["sourceOperationId"] }),
        )
        .await;
        assert_eq!(
            exec["ok"], true,
            "source.session.execute(create) failed: {exec}"
        );
        assert_eq!(
            exec["result"]["outcome"], "succeeded",
            "create failed: {exec}"
        );
        let native_session_id = exec["result"]["response"]["nativeSessionId"]
            .as_str()
            .expect("nativeSessionId")
            .to_owned();
        let target_id = self
            .register_and_bind_target(node, key, client_id, selection, &native_session_id)
            .await;
        (target_id, native_session_id)
    }

    async fn register_and_bind_target(
        &self,
        node: &NodeRuntime,
        key: &SigningKey,
        client_id: &str,
        selection: &SourceLaunchSelection,
        native_session_id: &str,
    ) -> EntityId {
        let observed = node
            .management
            .as_ref()
            .expect("management")
            .observe_next_document_socket_for_test()
            .expect("observe socket");
        let mut ctrl_keep = authenticate_socket(
            valid_management_socket(&self.area, node.listen_addr()).await,
            key,
            client_id,
            node_public_key(node),
        )
        .await;
        let endpoint = observed.await.expect("endpoint");
        let endpoint_client = endpoint.client();
        let actor = endpoint_client.context.actor();
        let services = node.session_services.as_ref().expect("session services");

        let target = services
            .management_services()
            .register_target(
                actor,
                selection.source,
                native_session_id,
                selection.workspace,
            )
            .await
            .expect("register target");

        let observation = services
            .catalog
            .observe_workspace(
                actor,
                target.target_id,
                selection.source,
                selection.workspace,
                &tc_deadline(),
            )
            .await
            .expect("observe workspace");
        let bind_reg = services
            .catalog
            .register(
                actor,
                observation
                    .bind(
                        selection.harness,
                        selection.profile,
                        selection.launch_path.clone(),
                    )
                    .expect("bind target draft"),
                &tc_deadline(),
            )
            .await
            .expect("register target bind");
        services
            .catalog
            .apply(actor, bind_reg, &tc_deadline())
            .await
            .expect("apply target bind");
        let _ = ctrl_keep.close(None).await;
        target.target_id
    }

    fn peer_logs(&self) -> Vec<serde_json::Value> {
        let Ok(raw) = std::fs::read_to_string(&self.log_path) else {
            return Vec::new();
        };
        raw.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("valid log line"))
            .collect()
    }
}

fn selection_json(sel: &SourceLaunchSelection) -> serde_json::Value {
    serde_json::json!({
        "sourceId": sel.source.encode(),
        "harnessId": sel.harness.encode(),
        "profileId": sel.profile.id().encode(),
        "profileRevision": sel.profile.revision().to_string(),
        "workspaceId": sel.workspace.encode(),
        "launchPath": sel.launch_path,
    })
}

async fn takeover_target(sock: &mut ManagementSocket, target_id: EntityId) {
    let take = send_command_wait(
        sock,
        "session.control.takeover",
        serde_json::json!({ "targetId": target_id.encode() }),
    )
    .await;
    assert_eq!(take["ok"], true, "session.control.takeover failed: {take}");
}

async fn submit_prompt_turn(
    sock: &mut ManagementSocket,
    target_id: EntityId,
    prompt_text: &str,
) -> serde_json::Value {
    let tid = target_id.encode();
    let begin = send_command_wait(
        sock,
        "input.upload.begin",
        serde_json::json!({ "targetId": tid, "purpose": "submit" }),
    )
    .await;
    assert_eq!(begin["ok"], true, "input.upload.begin failed: {begin}");
    let upload_id_str = begin["result"]["uploadId"].as_str().expect("uploadId");
    let upload_id = Id128::parse_base64url(upload_id_str).expect("id128");
    let body_json = serde_json::to_vec(&serde_json::json!([
        { "type": "text", "text": prompt_text }
    ]))
    .expect("prompt json");
    sock.send(Message::Binary(
        encode_acpi_upload(upload_id, &body_json).into(),
    ))
    .await
    .expect("send acpi");
    let finish = send_command_wait(
        sock,
        "input.upload.finish",
        serde_json::json!({
            "uploadId": upload_id_str,
            "totalBytes": body_json.len().to_string(),
        }),
    )
    .await;
    assert_eq!(finish["ok"], true, "input.upload.finish failed: {finish}");

    let body_id_str = timeout(Duration::from_secs(5), async {
        loop {
            let status = send_command_wait(
                sock,
                "input.upload.status",
                serde_json::json!({ "uploadId": upload_id_str }),
            )
            .await;
            assert_eq!(status["ok"], true, "input.upload.status failed: {status}");
            if status["result"]["phase"] == "ready"
                && let Some(body) = status["result"]["bodyId"].as_str()
            {
                return body.to_owned();
            }
            assert_ne!(
                status["result"]["phase"], "failed",
                "input.upload failed: {status}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("upload body ready timeout");

    let reg = send_command_wait(
        sock,
        "input.register",
        serde_json::json!({
            "targetId": tid,
            "operation": { "kind": "submit", "bodyId": body_id_str }
        }),
    )
    .await;
    assert_eq!(reg["ok"], true, "input.register failed: {reg}");
    let reg_id = reg["result"]["registrationId"]
        .as_str()
        .expect("registrationId");

    send_command_wait(
        sock,
        "input.consume",
        serde_json::json!({
            "targetId": tid,
            "registrationId": reg_id,
            "bodyId": body_id_str
        }),
    )
    .await
}

async fn wait_for_peer_method(fixture: &HarnessFixture, method: &str, min_count: usize) {
    timeout(Duration::from_secs(8), async {
        loop {
            let count = fixture
                .peer_logs()
                .into_iter()
                .filter(|e| e.get("method").and_then(|m| m.as_str()) == Some(method))
                .count();
            if count >= min_count {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "timed out waiting for {min_count} calls to {method}; logs={:?}",
            fixture.peer_logs()
        )
    });
    tokio::time::sleep(Duration::from_millis(60)).await;
}

async fn release_target(sock: &mut ManagementSocket, target_id: EntityId) {
    let reg = send_command_wait(
        sock,
        "native.release.register",
        serde_json::json!({ "targetId": target_id.encode() }),
    )
    .await;
    assert_eq!(reg["ok"], true, "native.release.register failed: {reg}");
    let op_id = reg["result"]["releaseOperationId"].clone();
    timeout(Duration::from_secs(5), async {
        loop {
            let exec = send_command_wait(
                sock,
                "native.release",
                serde_json::json!({ "releaseOperationId": op_id }),
            )
            .await;
            assert_eq!(exec["ok"], true, "native.release failed: {exec}");
            if exec["result"]["state"] == "released" {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("native.release reaches released phase");
}

#[tokio::test]
async fn stc_003_and_stc_005_live_capture_cold_revalidation_singleflight_and_node_sqlite_isolation()
{
    let fixture = HarnessFixture::new("stc003_005");
    let owner1 = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node1 = owner1.ready().await.expect("start node1");

    // Verify `node.sqlite3` stays strictly at SCHEMA_VERSION 15 without any transcript cache tables.
    let node_tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .fetch_all(node1.storage.as_ref().expect("storage").pool_for_test())
            .await
            .expect("query node sqlite tables");
    assert!(
        !node_tables.iter().any(|t| t.contains("transcript_")),
        "node.sqlite3 must contain zero transcript tables: {node_tables:?}"
    );
    let node_schema_version: i64 = sqlx::query_scalar("SELECT schema_version FROM node_meta")
        .fetch_one(node1.storage.as_ref().expect("storage").pool_for_test())
        .await
        .expect("query node_meta schema_version");
    assert_eq!(node_schema_version, SCHEMA_VERSION);

    let (client_id, key, selection) = fixture.install_profile(&node1, 1, 61).await;
    let mut ctrl1 = authenticate_socket(
        valid_management_socket(&fixture.area, node1.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node1),
    )
    .await;
    let (target_id, native_id) = fixture
        .create_and_bind_target(&node1, &mut ctrl1, &key, &client_id, &selection)
        .await;

    let consume = submit_prompt_turn(&mut ctrl1, target_id, "first-prompt-v1").await;
    assert_eq!(consume["ok"], true, "prompt turn failed: {consume}");
    wait_for_peer_method(&fixture, "session/prompt", 1).await;
    let seed_key =
        NativeSessionKey::from_config(Some(fixture.source_id), &native_id).expect("seed key");
    timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(Some(_)) = node1
                .session_services
                .as_ref()
                .expect("services")
                .transcript_cache
                .store()
                .load_snapshot(&seed_key)
                .await
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("live capture persisted to transcript_cache");

    let live_open = send_command_wait(
        &mut ctrl1,
        "view.open",
        serde_json::json!({ "targetId": target_id.encode(), "reason": "first_open" }),
    )
    .await;
    assert_eq!(
        live_open["ok"],
        true,
        "live view.open: {live_open}\npeer_logs={:?}",
        fixture.peer_logs()
    );
    assert_eq!(live_open["result"]["status"]["transcriptSource"], "live");
    let live_stream =
        Id128::parse_base64url(live_open["result"]["streamId"].as_str().unwrap()).unwrap();
    let _ = read_until_snapshot_end(&mut ctrl1, live_stream).await;
    let _ = ctrl1.close(None).await;

    tokio::time::sleep(Duration::from_millis(80)).await;
    node1.shutdown(None).await.expect("shutdown node1");
    drop(owner1);

    let cache_db_path = fixture.area.data_dir().join("transcript-cache.sqlite3");
    assert!(cache_db_path.is_file());
    {
        let ext_cache =
            SqlitePool::connect_with(SqliteConnectOptions::new().filename(&cache_db_path))
                .await
                .expect("inspect transcript-cache.sqlite3");
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transcript_sessions WHERE native_session_id = ? AND protocol_version = 1",
        )
        .bind(&native_id)
        .fetch_one(&ext_cache)
        .await
        .expect("query v1 session");
        assert_eq!(count, 1);
        ext_cache.close().await;
    }

    // Configure barrier on `session/load` for cold restart (`STC-005`).
    let barrier_entered = fixture.area.directory.join("load-entered.flag");
    let barrier_release = fixture.area.directory.join("load-release.flag");
    let mut cfg2 = PeerConfigBuilder::replay_capable_v1();
    cfg2.load_barrier_entered_file = Some(barrier_entered.to_string_lossy().into_owned());
    cfg2.load_barrier_release_file = Some(barrier_release.to_string_lossy().into_owned());
    cfg2.replay_texts = vec!["replayed-fresh-from-peer".to_owned()];
    cfg2.write(&fixture.cfg_path);
    let _ = std::fs::remove_file(&fixture.log_path);

    let owner2 = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node2 = owner2.ready().await.expect("restart node2");

    let mut sock_a = authenticate_socket(
        valid_management_socket(&fixture.area, node2.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node2),
    )
    .await;
    let mut sock_b = authenticate_socket(
        valid_management_socket(&fixture.area, node2.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node2),
    )
    .await;

    // Authorization before cache read: wrong workspaceId and stale profileRevision fail closed.
    let mut wrong_sel = selection_json(&selection);
    wrong_sel["workspaceId"] = serde_json::json!(tc_id(199).encode());
    let wrong_res = send_command_wait(
        &mut sock_a,
        "view.open_source",
        serde_json::json!({
            "selection": wrong_sel,
            "nativeSessionId": native_id,
            "reason": "first_open",
        }),
    )
    .await;
    assert_eq!(
        wrong_res["ok"], false,
        "wrong workspace must fail before cache read"
    );

    let mut stale_sel = selection_json(&selection);
    stale_sel["profileRevision"] = serde_json::json!("999");
    let stale_res = send_command_wait(
        &mut sock_a,
        "view.open_source",
        serde_json::json!({
            "selection": stale_sel,
            "nativeSessionId": native_id,
            "reason": "first_open",
        }),
    )
    .await;
    assert_eq!(
        stale_res["ok"], false,
        "stale profileRevision must fail before cache read"
    );

    // Concurrent cold opens while `session/load` is pinned behind barrier.
    let (res_a, res_b) = tokio::join!(
        send_command_wait(
            &mut sock_a,
            "view.open",
            serde_json::json!({ "targetId": target_id.encode(), "reason": "first_open" })
        ),
        send_command_wait(
            &mut sock_b,
            "view.open_source",
            serde_json::json!({
                "selection": selection_json(&selection),
                "nativeSessionId": native_id,
                "reason": "first_open",
            })
        ),
    );
    assert_eq!(res_a["ok"], true, "cold view.open: {res_a}");
    assert_eq!(res_b["ok"], true, "cold view.open_source: {res_b}");
    assert_eq!(
        res_a["result"]["status"]["transcriptSource"],
        "transcript_cache"
    );
    assert_eq!(res_a["result"]["status"]["syncState"], "revalidating");

    let stream_a = Id128::parse_base64url(res_a["result"]["streamId"].as_str().unwrap()).unwrap();
    let stream_b = Id128::parse_base64url(res_b["result"]["streamId"].as_str().unwrap()).unwrap();
    let (snap_a, snap_b) = tokio::join!(
        read_until_snapshot_end(&mut sock_a, stream_a),
        read_until_snapshot_end(&mut sock_b, stream_b),
    );
    assert_eq!(snap_a.snapshot_ends, 1);
    assert_eq!(snap_b.snapshot_ends, 1);

    timeout(Duration::from_secs(5), async {
        while !barrier_entered.is_file() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "peer enters session/load barrier; peer_logs={:?}",
            fixture.peer_logs()
        )
    });

    let cap_during_load = send_command_wait(
        &mut sock_a,
        "session.capacity.inspect",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(cap_during_load["result"]["occupied"], 1);

    std::fs::write(&barrier_release, "go").expect("release load barrier");

    let rebound_a = read_next_status_matching(
        &mut sock_a,
        stream_a,
        |v| v.get("type").and_then(|t| t.as_str()) == Some("stream_rebound"),
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(rebound_a["type"], "stream_rebound");

    let rebind_res = send_command_wait(
        &mut sock_a,
        "view.rebind",
        serde_json::json!({ "streamId": res_a["result"]["streamId"] }),
    )
    .await;
    assert_eq!(rebind_res["ok"], true, "view.rebind failed: {rebind_res}");
    assert_eq!(rebind_res["result"]["status"]["transcriptSource"], "live");
    assert_eq!(rebind_res["result"]["status"]["syncState"], "synced");

    let load_calls = fixture
        .peer_logs()
        .into_iter()
        .filter(|e| e.get("method").and_then(|m| m.as_str()) == Some("session/load"))
        .count();
    assert_eq!(
        load_calls, 1,
        "singleflight must collapse concurrent session/load"
    );

    let _ = sock_a.close(None).await;
    let _ = sock_b.close(None).await;
    node2.shutdown(None).await.expect("shutdown node2");
}

#[tokio::test]
async fn stc_004_node_corrupt_cache_and_trigger_fault_preserve_live_run_and_approval() {
    let fixture = HarnessFixture::new("stc004_node");
    std::fs::create_dir_all(fixture.area.data_dir()).expect("create data dir");
    std::fs::write(
        fixture.area.data_dir().join("transcript-cache.sqlite3"),
        b"CORRUPTED-CACHE-HEADER-FOR-STC004",
    )
    .expect("write corrupt db");

    let owner = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node = owner
        .ready()
        .await
        .expect("node starts cleanly despite corrupt cache");
    let mut cfg = PeerConfigBuilder::replay_capable_v1();
    cfg.request_permission = true;
    cfg.write(&fixture.cfg_path);

    let (client_id, key, selection) = fixture.install_profile(&node, 1, 62).await;
    let mut sock_prompt = authenticate_socket(
        valid_management_socket(&fixture.area, node.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node),
    )
    .await;
    let mut sock_approve = authenticate_socket(
        valid_management_socket(&fixture.area, node.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node),
    )
    .await;
    let (target_id, _) = fixture
        .create_and_bind_target(&node, &mut sock_prompt, &key, &client_id, &selection)
        .await;

    let (consume_res, ()) = tokio::join!(
        submit_prompt_turn(&mut sock_prompt, target_id, "run-with-approval"),
        async {
            let approval = timeout(Duration::from_secs(10), async {
                loop {
                    let list = send_command_wait(
                        &mut sock_approve,
                        "approval.list",
                        serde_json::json!({}),
                    )
                    .await;
                    if let Some(first) = list["result"]["items"].as_array().and_then(|a| a.first())
                    {
                        return first.clone();
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
            })
            .await
            .expect("approval arrives during degraded cache");
            let approval_id = approval["approvalId"].as_str().expect("approvalId");
            let choice_id = approval["choices"][0]["choiceId"]
                .as_str()
                .expect("choiceId");
            let reg = send_command_wait(
                &mut sock_approve,
                "approval.register",
                serde_json::json!({ "approvalId": approval_id, "choiceId": choice_id }),
            )
            .await;
            assert_eq!(reg["ok"], true, "approval.register failed: {reg}");
            let dec = send_command_wait(
                &mut sock_approve,
                "approval.decide",
                serde_json::json!({ "approvalRegistrationId": reg["result"]["approvalRegistrationId"] }),
            )
            .await;
            assert_eq!(dec["ok"], true, "approval.decide failed: {dec}");
        }
    );
    assert_eq!(
        consume_res["ok"], true,
        "prompt consume failed: {consume_res}"
    );

    let open = send_command_wait(
        &mut sock_prompt,
        "view.open",
        serde_json::json!({ "targetId": target_id.encode(), "reason": "first_open" }),
    )
    .await;
    assert_eq!(open["ok"], true);
    assert_eq!(open["result"]["status"]["cacheWarning"], true);
    let _ = send_command_wait(
        &mut sock_prompt,
        "view.close",
        serde_json::json!({ "streamId": open["result"]["streamId"] }),
    )
    .await;

    let _ = sock_prompt.close(None).await;
    let _ = sock_approve.close(None).await;
    node.shutdown(None).await.expect("shutdown part A node");
    drop(owner);

    // Part B: Runtime SQLite write fault (`BEFORE INSERT` trigger) -> cacheWarning=true, live run unaffected.
    let trig_fix = HarnessFixture::new("stc004_trig");
    let owner_t = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        trig_fix.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node_t = owner_t.ready().await.expect("start trigger node");
    let (c_id, k_t, sel_t) = trig_fix.install_profile(&node_t, 1, 66).await;
    let mut sock_t = authenticate_socket(
        valid_management_socket(&trig_fix.area, node_t.listen_addr()).await,
        &k_t,
        &c_id,
        node_public_key(&node_t),
    )
    .await;
    let (target_t, _) = trig_fix
        .create_and_bind_target(&node_t, &mut sock_t, &k_t, &c_id, &sel_t)
        .await;
    {
        let db_path = trig_fix.area.data_dir().join("transcript-cache.sqlite3");
        let ext_pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
            .await
            .expect("open cache db for trigger");
        sqlx::query(
            "CREATE TRIGGER fault_on_insert BEFORE INSERT ON transcript_records BEGIN SELECT RAISE(ABORT, 'disk fault'); END;",
        )
        .execute(&ext_pool)
        .await
        .expect("create fault trigger");
        ext_pool.close().await;
    }

    let trig_consume = submit_prompt_turn(&mut sock_t, target_t, "live-under-write-fault").await;
    assert_eq!(
        trig_consume["ok"], true,
        "prompt must succeed under write fault: {trig_consume}"
    );
    wait_for_peer_method(&trig_fix, "session/prompt", 1).await;
    let open_t = timeout(Duration::from_secs(5), async {
        loop {
            let res = send_command_wait(
                &mut sock_t,
                "view.open",
                serde_json::json!({ "targetId": target_t.encode(), "reason": "first_open" }),
            )
            .await;
            assert_eq!(res["ok"], true);
            if res["result"]["status"]["cacheWarning"] == true {
                return res;
            }
            if let Some(sid) = res["result"]["streamId"].as_str() {
                let _ = send_command_wait(
                    &mut sock_t,
                    "view.close",
                    serde_json::json!({ "streamId": sid }),
                )
                .await;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("cacheWarning becomes true after async writer fault");
    assert_eq!(open_t["result"]["status"]["cacheWarning"], true);

    let _ = sock_t.close(None).await;
    node_t.shutdown(None).await.expect("shutdown trigger node");
}

#[tokio::test]
async fn stc_006_and_stc_014_capacity_50_full_and_unreachable_recovery() {
    let fixture = HarnessFixture::new("stc006");
    let owner = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node = owner.ready().await.expect("start node");
    let (client_id, key, selection) = fixture.install_profile(&node, 1, 63).await;

    // Pair a second client socket to keep each socket comfortably under the 120 cmds/60s window.
    let key2 = SigningKey::from_slice(&[69; 32]).expect("key2");
    let client_id2 = pair_browser(&node, &key2, "STC Client 2").await;

    let mut sock = authenticate_socket(
        valid_management_socket(&fixture.area, node.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node),
    )
    .await;
    let mut sock_fill = authenticate_socket(
        valid_management_socket(&fixture.area, node.listen_addr()).await,
        &key2,
        &client_id2,
        node_public_key(&node),
    )
    .await;

    let (target_51, _) = fixture
        .create_and_bind_target(&node, &mut sock, &key, &client_id, &selection)
        .await;
    let (target_release, _) = fixture
        .create_and_bind_target(&node, &mut sock, &key, &client_id, &selection)
        .await;

    let seeded = submit_prompt_turn(&mut sock, target_51, "cached-51-msg").await;
    assert_eq!(seeded["ok"], true);
    wait_for_peer_method(&fixture, "session/prompt", 1).await;
    release_target(&mut sock, target_51).await;
    release_target(&mut sock, target_release).await;
    let _ = sock.close(None).await;
    let _ = sock_fill.close(None).await;
    node.shutdown(None).await.expect("shutdown stc006 node1");
    drop(owner);

    // Part A: Unreachable source on cold open -> `degraded_unreachable`, 0 slots consumed, then explicit_refresh.
    let mut fail_cfg = PeerConfigBuilder::replay_capable_v1();
    fail_cfg.fail_spawn = true;
    fail_cfg.write(&fixture.cfg_path);

    let owner2 = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node2 = owner2.ready().await.expect("restart stc006 node2");
    let mut sock = authenticate_socket(
        valid_management_socket(&fixture.area, node2.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node2),
    )
    .await;
    let mut sock_fill = authenticate_socket(
        valid_management_socket(&fixture.area, node2.listen_addr()).await,
        &key2,
        &client_id2,
        node_public_key(&node2),
    )
    .await;

    let cold_unreach = send_command_wait(
        &mut sock,
        "view.open",
        serde_json::json!({ "targetId": target_51.encode(), "reason": "first_open" }),
    )
    .await;
    assert_eq!(cold_unreach["ok"], true);
    assert_eq!(
        cold_unreach["result"]["status"]["transcriptSource"],
        "transcript_cache"
    );
    let unreach_stream =
        Id128::parse_base64url(cold_unreach["result"]["streamId"].as_str().unwrap()).unwrap();
    let _ = read_until_snapshot_end(&mut sock, unreach_stream).await;
    let status_unreach = read_next_status_matching(
        &mut sock,
        unreach_stream,
        |v| v.get("syncState").and_then(|s| s.as_str()) == Some("degraded_unreachable"),
        Duration::from_secs(8),
    )
    .await;
    assert_eq!(status_unreach["syncState"], "degraded_unreachable");

    let cap_zero =
        send_command_wait(&mut sock, "session.capacity.inspect", serde_json::json!({})).await;
    assert_eq!(
        cap_zero["result"]["occupied"], 0,
        "cold degraded_unreachable must consume 0 slots: {cap_zero}"
    );

    PeerConfigBuilder::replay_capable_v1().write(&fixture.cfg_path);
    let _ = send_command_wait(
        &mut sock,
        "view.close",
        serde_json::json!({ "streamId": cold_unreach["result"]["streamId"] }),
    )
    .await;

    // Verify explicit_refresh recovers to synced, then restart node3 so target_51 is cold again for Part B.
    let recover_a = send_command_wait(
        &mut sock,
        "view.open",
        serde_json::json!({ "targetId": target_51.encode(), "reason": "explicit_refresh" }),
    )
    .await;
    assert_eq!(recover_a["ok"], true);
    let rec_stream =
        Id128::parse_base64url(recover_a["result"]["streamId"].as_str().unwrap()).unwrap();
    let snap_rec = read_until_snapshot_end(&mut sock, rec_stream).await;
    if !snap_rec
        .statuses
        .iter()
        .any(|v| v.get("type").and_then(|t| t.as_str()) == Some("stream_rebound"))
    {
        let _ = read_next_status_matching(
            &mut sock,
            rec_stream,
            |v| v.get("type").and_then(|t| t.as_str()) == Some("stream_rebound"),
            Duration::from_secs(8),
        )
        .await;
    }
    let _ = send_command_wait(
        &mut sock,
        "view.close",
        serde_json::json!({ "streamId": recover_a["result"]["streamId"] }),
    )
    .await;
    let _ = sock.close(None).await;
    let _ = sock_fill.close(None).await;
    node2.shutdown(None).await.expect("shutdown stc006 node2");
    drop(owner2);

    let owner3 = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node3 = owner3.ready().await.expect("start stc006 node3");
    let mut sock = authenticate_socket(
        valid_management_socket(&fixture.area, node3.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node3),
    )
    .await;
    let mut sock_fill = authenticate_socket(
        valid_management_socket(&fixture.area, node3.listen_addr()).await,
        &key2,
        &client_id2,
        node_public_key(&node3),
    )
    .await;

    // Part B (`STC-006` + `STC-014`): Occupy 50/50 capacity slots (`target_release` + 49 created).
    takeover_target(&mut sock_fill, target_release).await;
    let active_seed =
        submit_prompt_turn(&mut sock_fill, target_release, "active-slot-holder").await;
    assert_eq!(active_seed["ok"], true, "active_seed: {active_seed}");
    for _ in 0..49 {
        let sreg = send_command_wait(
            &mut sock_fill,
            "source.session.register",
            serde_json::json!({
                "selection": selection_json(&selection),
                "operation": { "kind": "create" },
            }),
        )
        .await;
        assert_eq!(sreg["ok"], true, "source.session.register failed: {sreg}");
        let sexec = send_command_wait(
            &mut sock_fill,
            "source.session.execute",
            serde_json::json!({ "sourceOperationId": sreg["result"]["sourceOperationId"] }),
        )
        .await;
        assert_eq!(sexec["ok"], true, "source.session.execute failed: {sexec}");
    }
    let cap_50 =
        send_command_wait(&mut sock, "session.capacity.inspect", serde_json::json!({})).await;
    assert_eq!(
        cap_50["result"]["occupied"], 50,
        "must hit 50/50 capacity: {cap_50}"
    );
    assert_eq!(cap_50["result"]["limit"], 50);

    // Open cold cached #51 -> immediate cache snapshot + `degraded_capacity_full`, zero slots evicted!
    let cap_full_open = send_command_wait(
        &mut sock,
        "view.open",
        serde_json::json!({ "targetId": target_51.encode(), "reason": "first_open" }),
    )
    .await;
    assert_eq!(
        cap_full_open["ok"], true,
        "view.open on 51st must still return cache: {cap_full_open}"
    );
    assert_eq!(
        cap_full_open["result"]["status"]["transcriptSource"],
        "transcript_cache"
    );
    let full_stream =
        Id128::parse_base64url(cap_full_open["result"]["streamId"].as_str().unwrap()).unwrap();
    let _ = read_until_snapshot_end(&mut sock, full_stream).await;
    let status_full = read_next_status_matching(
        &mut sock,
        full_stream,
        |v| v.get("syncState").and_then(|s| s.as_str()) == Some("degraded_capacity_full"),
        Duration::from_secs(8),
    )
    .await;
    assert_eq!(status_full["syncState"], "degraded_capacity_full");

    // Release 1 slot -> occupied 49 -> explicit_refresh on 51st succeeds (`synced`).
    release_target(&mut sock, target_release).await;
    let cap_49 =
        send_command_wait(&mut sock, "session.capacity.inspect", serde_json::json!({})).await;
    assert_eq!(cap_49["result"]["occupied"], 49, "one slot freed: {cap_49}");

    let _ = send_command_wait(
        &mut sock,
        "view.close",
        serde_json::json!({ "streamId": cap_full_open["result"]["streamId"] }),
    )
    .await;
    let synced_51 = send_command_wait(
        &mut sock,
        "view.open",
        serde_json::json!({ "targetId": target_51.encode(), "reason": "explicit_refresh" }),
    )
    .await;
    assert_eq!(synced_51["ok"], true);
    let stream_51 =
        Id128::parse_base64url(synced_51["result"]["streamId"].as_str().unwrap()).unwrap();
    let _ = read_until_snapshot_end(&mut sock, stream_51).await;

    let _ = sock.close(None).await;
    let _ = sock_fill.close(None).await;
    node3.shutdown(None).await.expect("shutdown stc006 node3");
}

#[tokio::test]
async fn stc_007_and_stc_008_and_stc_009_resume_only_pure_memory_unrecoverable_and_purge() {
    // Part A (`STC-007` + `STC-008`): `resume_only` lazy resume, zero stuffing, `native_unrecoverable`.
    let fixture = HarnessFixture::new("stc007_8");
    let mut resume_cfg = PeerConfigBuilder::replay_capable_v1();
    resume_cfg.capability_mode = "resume_only".to_owned();
    resume_cfg.write(&fixture.cfg_path);

    let owner = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node = owner.ready().await.expect("start node");
    let (client_id, key, sel) = fixture.install_profile(&node, 1, 64).await;
    let mut sock = authenticate_socket(
        valid_management_socket(&fixture.area, node.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node),
    )
    .await;
    let (target_id, native_id) = fixture
        .create_and_bind_target(&node, &mut sock, &key, &client_id, &sel)
        .await;

    let seed_turn =
        submit_prompt_turn(&mut sock, target_id, "secret-prior-context-never-stuffed").await;
    assert_eq!(seed_turn["ok"], true, "seed resume_only turn: {seed_turn}");
    wait_for_peer_method(&fixture, "session/prompt", 1).await;
    release_target(&mut sock, target_id).await;
    let _ = sock.close(None).await;
    node.shutdown(None).await.expect("shutdown first run");
    drop(owner);

    let _ = std::fs::remove_file(&fixture.log_path);
    let owner2 = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node2 = owner2.ready().await.expect("restart node2");
    let mut sock2 = authenticate_socket(
        valid_management_socket(&fixture.area, node2.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node2),
    )
    .await;

    let view_resume = send_command_wait(
        &mut sock2,
        "view.open",
        serde_json::json!({ "targetId": target_id.encode(), "reason": "first_open" }),
    )
    .await;
    assert_eq!(view_resume["ok"], true);
    assert_eq!(
        view_resume["result"]["status"]["capabilityBoundary"],
        "resume_only"
    );
    assert_eq!(view_resume["result"]["status"]["continuable"], true);
    let resume_stream =
        Id128::parse_base64url(view_resume["result"]["streamId"].as_str().unwrap()).unwrap();
    let _ = read_until_snapshot_end(&mut sock2, resume_stream).await;

    let resume_calls_on_view = fixture
        .peer_logs()
        .into_iter()
        .filter(|e| e.get("method").and_then(|m| m.as_str()) == Some("session/resume"))
        .count();
    assert_eq!(
        resume_calls_on_view, 0,
        "STC-007: view only must not invoke session/resume"
    );

    takeover_target(&mut sock2, target_id).await;
    let followup = submit_prompt_turn(&mut sock2, target_id, "followup-only-text").await;
    assert_eq!(followup["ok"], true, "followup failed: {followup}");
    wait_for_peer_method(&fixture, "session/prompt", 1).await;
    let logs = fixture.peer_logs();
    let prompt_calls: Vec<_> = logs
        .iter()
        .filter(|e| e.get("method").and_then(|m| m.as_str()) == Some("session/prompt"))
        .collect();
    assert!(!prompt_calls.is_empty());
    let prompt_json = serde_json::to_string(prompt_calls.last().unwrap()).unwrap();
    assert!(prompt_json.contains("followup-only-text"));
    assert!(
        !prompt_json.contains("secret-prior-context-never-stuffed"),
        "STC-007: cached snapshot must NEVER be stuffed into ACP prompt: {prompt_json}"
    );

    release_target(&mut sock2, target_id).await;
    resume_cfg.fail_resume_message = Some("remote session expired".to_owned());
    resume_cfg.write(&fixture.cfg_path);
    let _ = std::fs::remove_file(&fixture.log_path);

    takeover_target(&mut sock2, target_id).await;
    let _ = submit_prompt_turn(&mut sock2, target_id, "after-remote-expiry").await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    let logs_after_err = fixture.peer_logs();
    assert_eq!(
        logs_after_err
            .iter()
            .filter(|e| e.get("method").and_then(|m| m.as_str()) == Some("session/new"))
            .count(),
        0,
        "STC-008: RemoteError on resume must NEVER fabricate a replacement session/new"
    );

    let native_key = NativeSessionKey::from_config(Some(fixture.source_id), &native_id)
        .expect("native session key");
    let snap = timeout(Duration::from_secs(3), async {
        loop {
            let s = node2
                .session_services
                .as_ref()
                .expect("session services")
                .transcript_cache
                .store()
                .load_snapshot(&native_key)
                .await
                .expect("load snapshot from store")
                .expect("snapshot exists");
            if s.session.native_recovery_state
                == crate::transcript_cache::NativeRecoveryState::NativeUnrecoverable
            {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .expect("native_recovery_state becomes NativeUnrecoverable");
    assert_eq!(
        snap.session.native_recovery_state,
        crate::transcript_cache::NativeRecoveryState::NativeUnrecoverable
    );
    let _ = sock2.close(None).await;
    node2.shutdown(None).await.expect("shutdown stc007 node2");
    drop(owner2);

    // Part B (`STC-008` pure_memory + `STC-009` 5->3 compressed replay + synchronous purge).
    let replay_fix = HarnessFixture::new("stc009");
    let mut replay_cfg = PeerConfigBuilder::replay_capable_v1();
    replay_cfg.replay_texts = vec![
        "cmp-1".to_owned(),
        "cmp-2".to_owned(),
        "cmp-3".to_owned(),
        "cmp-4".to_owned(),
        "cmp-5".to_owned(),
    ];
    replay_cfg.write(&replay_fix.cfg_path);

    let owner3 = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        replay_fix.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node3 = owner3.ready().await.expect("start node3");
    let (cid3, key3, sel3) = replay_fix.install_profile(&node3, 1, 67).await;
    let mut sock3 = authenticate_socket(
        valid_management_socket(&replay_fix.area, node3.listen_addr()).await,
        &key3,
        &cid3,
        node_public_key(&node3),
    )
    .await;
    let (target_cmp, native_cmp) = replay_fix
        .create_and_bind_target(&node3, &mut sock3, &key3, &cid3, &sel3)
        .await;
    release_target(&mut sock3, target_cmp).await;

    // Open -> replays 5 items into cache, then close + release.
    let open5 = send_command_wait(
        &mut sock3,
        "view.open",
        serde_json::json!({ "targetId": target_cmp.encode(), "reason": "first_open" }),
    )
    .await;
    assert_eq!(open5["ok"], true, "open5: {open5}");
    let stream5 = Id128::parse_base64url(open5["result"]["streamId"].as_str().unwrap()).unwrap();
    let _ = read_until_snapshot_end(&mut sock3, stream5).await;
    let seed_key = NativeSessionKey::from_config(Some(replay_fix.source_id), &native_cmp)
        .expect("seed purge key");
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Some(snap)) = node3
                .session_services
                .as_ref()
                .expect("services")
                .transcript_cache
                .store()
                .load_snapshot(&seed_key)
                .await
                && snap.session.record_count == 5
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .expect("open5 persisted 5 records before node3 shutdown");
    let _ = send_command_wait(
        &mut sock3,
        "view.close",
        serde_json::json!({ "streamId": open5["result"]["streamId"] }),
    )
    .await;
    let _ = sock3.close(None).await;
    node3.shutdown(None).await.expect("shutdown stc009 node3");
    drop(owner3);

    // Peer shrinks to 3 items -> cold restart + explicit_refresh updates generation & shrinks record_count to 3.
    replay_cfg.replay_texts = vec!["cmp-1".to_owned(), "cmp-3".to_owned(), "cmp-5".to_owned()];
    replay_cfg.write(&replay_fix.cfg_path);
    let owner4 = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        replay_fix.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node4 = owner4.ready().await.expect("start node4");
    let mut sock4 = authenticate_socket(
        valid_management_socket(&replay_fix.area, node4.listen_addr()).await,
        &key3,
        &cid3,
        node_public_key(&node4),
    )
    .await;
    let open3 = send_command_wait(
        &mut sock4,
        "view.open",
        serde_json::json!({ "targetId": target_cmp.encode(), "reason": "explicit_refresh" }),
    )
    .await;
    assert_eq!(open3["ok"], true, "open3: {open3}");
    let stream3 = Id128::parse_base64url(open3["result"]["streamId"].as_str().unwrap()).unwrap();
    let _ = read_until_snapshot_end(&mut sock4, stream3).await;

    let purge_key =
        NativeSessionKey::from_config(Some(replay_fix.source_id), &native_cmp).expect("purge key");
    let cmp_snap = timeout(Duration::from_secs(5), async {
        loop {
            let s = node4
                .session_services
                .as_ref()
                .expect("services")
                .transcript_cache
                .store()
                .load_snapshot(&purge_key)
                .await
                .expect("load cmp snapshot")
                .expect("cmp snapshot exists");
            if s.session.record_count == 3 {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .expect("5->3 compressed replay must leave 3 records");
    assert_eq!(cmp_snap.session.record_count, 3);

    // Synchronous delete purge removes all rows and checkpoints WAL.
    node4
        .session_services
        .as_ref()
        .expect("services")
        .transcript_cache
        .store()
        .purge_deleted_native_session(&purge_key)
        .await
        .expect("purge deleted session");
    let after_purge = node4
        .session_services
        .as_ref()
        .expect("services")
        .transcript_cache
        .store()
        .load_snapshot(&purge_key)
        .await
        .expect("load after purge");
    assert!(
        after_purge.is_none(),
        "purged session must have no snapshot rows"
    );

    let _ = sock4.close(None).await;
    node4.shutdown(None).await.expect("shutdown stc009 node4");
}

#[tokio::test]
async fn stc_010_revoke_during_cached_and_revalidating_stream_closes_4410() {
    let fixture = HarnessFixture::new("stc010");
    let mut cfg = PeerConfigBuilder::replay_capable_v1();
    cfg.protocol_version = 2;
    cfg.write(&fixture.cfg_path);

    let owner = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node = owner.ready().await.expect("start node");
    let (client_id, key, sel) = fixture.install_profile(&node, 2, 65).await;
    let mut sock = authenticate_socket(
        valid_management_socket(&fixture.area, node.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node),
    )
    .await;
    let (target_id, _) = fixture
        .create_and_bind_target(&node, &mut sock, &key, &client_id, &sel)
        .await;

    let seed = submit_prompt_turn(&mut sock, target_id, "before-revoke-v2").await;
    assert_eq!(seed["ok"], true, "v2 seed failed: {seed}");
    wait_for_peer_method(&fixture, "session/prompt", 1).await;
    tokio::time::sleep(Duration::from_millis(80)).await;
    let _ = sock.close(None).await;
    node.shutdown(None).await.expect("shutdown stc010 node1");
    drop(owner);

    let owner2 = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node2 = owner2.ready().await.expect("restart stc010 node2");
    let mut sock = authenticate_socket(
        valid_management_socket(&fixture.area, node2.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(&node2),
    )
    .await;

    let open = send_command_wait(
        &mut sock,
        "view.open",
        serde_json::json!({ "targetId": target_id.encode(), "reason": "first_open" }),
    )
    .await;
    assert_eq!(open["ok"], true, "stc_010 view.open failed: {open}");
    assert_eq!(
        open["result"]["status"]["transcriptSource"],
        "transcript_cache"
    );
    let stream_id = Id128::parse_base64url(open["result"]["streamId"].as_str().unwrap()).unwrap();
    let _ = read_until_snapshot_end(&mut sock, stream_id).await;

    crate::ipc::device_revoke(node2.ipc_address(), &client_id)
        .await
        .expect("revoke client");
    let (close_code, _) = next_close(&mut sock, Duration::from_secs(5)).await;
    assert_eq!(close_code, 4410);

    node2.shutdown(None).await.expect("shutdown stc010 node2");
}
