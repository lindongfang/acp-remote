//! Explicit opt-in synthetic peer used only by native-dispatch developer tests.
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::io::{self, BufRead, Write};
fn emit(value: &Value) {
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, value).unwrap();
    out.write_all(b"\n").unwrap();
    out.flush().unwrap();
}
fn diagnostic(name: &str, bytes: &[u8]) {
    if let Some(root) = std::env::var_os("SOURCE_SESSION_DIAGNOSTIC_ROOT") {
        let path = std::path::Path::new(&root).join(name);
        let _ = std::fs::write(path, bytes);
    }
}
#[expect(
    clippy::too_many_lines,
    reason = "The opt-in synthetic peer keeps its explicitly selected protocol scenarios in one input loop; it is not a production binary."
)]
fn main() {
    let scenario = std::env::args().nth(1).unwrap_or_else(|| "success".into());
    let mut prompt = None;
    let mut prompt_session = None;
    let mut late_prompt = None;
    let mut prompts = 0;
    let mut read_first_replays = 0;
    let mut verified_root: Option<String> = None;
    let mut unsupported_written = false;
    let v2 = scenario.starts_with("stop-v2-")
        || scenario.starts_with("early-stop-v2-")
        || scenario.starts_with("priority-v2-")
        || scenario == "replay-live"
        || scenario == "unknown-shutdown-v2"
        || scenario == "source-create-replay-state"
        || scenario == "source-create-replay-state-verified"
        || scenario == "external-reads-quarantine"
        || scenario.starts_with("child-auto-read");
    for line in io::stdin().lock().lines() {
        let value: Value = serde_json::from_str(&line.unwrap()).unwrap();
        match value["method"].as_str() {
            Some("initialize") => {
                diagnostic("initialize-seen", b"yes");
                if scenario == "pre-permission" || scenario == "unbound-permission" {
                    std::fs::write("initialize-received", b"synthetic").unwrap();
                    while !std::path::Path::new("release-initialize").is_file() {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                    emit(
                        &json!({"jsonrpc":"2.0","id":value["id"],"result":{"protocolVersion":1,"agentInfo":{"name":"native-dispatch-dev","version":"synthetic"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{},"close":{}}}}}),
                    );
                    let native = if scenario == "unbound-permission" {
                        "synthetic-unbound-session"
                    } else {
                        "synthetic-native-A"
                    };
                    emit(
                        &json!({"jsonrpc":"2.0","id":"synthetic-permission","method":"session/request_permission","params":{"sessionId":native,"toolCall":{"toolCallId":"synthetic-pre-tool","title":"synthetic","kind":"_future_kind"},"options":[{"optionId":"original-future-option","name":"synthetic","kind":"_future_kind","_future":{"kept":[1,null,true]}}]}}),
                    );
                    std::fs::write("early-permission", b"synthetic").unwrap();
                    continue;
                }
                let result = if matches!(
                    scenario.as_str(),
                    "r2-v01-discover"
                        | "normal"
                        | "exit-create"
                        | "delayed-create"
                        | "wrong-reply-stays-unknown"
                        | "list-4097"
                        | "list-4098-partial-fail"
                        | "read-first-held"
                ) {
                    json!({"protocolVersion":1,"agentInfo":{"name":"native-dispatch-dev","version":"synthetic"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"list":{},"newSession":{},"resume":{},"close":{}}}})
                } else if scenario == "no-list-with-new" {
                    json!({"protocolVersion":1,"agentInfo":{"name":"native-dispatch-dev","version":"synthetic"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"newSession":{},"resume":{},"close":{}}}})
                } else if scenario == "no-replay-source" {
                    json!({"protocolVersion":1,"agentInfo":{"name":"native-dispatch-dev","version":"synthetic"},"agentCapabilities":{"loadSession":false,"sessionCapabilities":{"list":{},"newSession":{},"resume":{},"close":{}}}})
                } else if v2 {
                    json!({"protocolVersion":2,"info":{"name":"native-dispatch-dev","version":"synthetic"},"capabilities":{"session":{}}})
                } else {
                    json!({"protocolVersion":1,"agentInfo":{"name":"native-dispatch-dev","version":"synthetic"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"list":{},"resume":{},"close":{}}}})
                };
                emit(&json!({"jsonrpc":"2.0","id":value["id"],"result":result}));
            }
            Some("session/load" | "session/resume") => {
                bump_source_method(value["method"].as_str().unwrap());
                if scenario.starts_with("child-auto-read") {
                    let mut log = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("child-read-requests")
                        .unwrap();
                    writeln!(log, "{}", value["params"]["sessionId"].as_str().unwrap()).unwrap();
                    std::fs::write("child-root-resume-held", b"held").unwrap();
                    while !std::path::Path::new("release-child-root-resume").is_file() {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                    if scenario == "child-auto-read-failure" {
                        emit(
                            &json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":-32603,"message":"root resume rejected"}}),
                        );
                    } else {
                        for child in ["synthetic-child-A", "synthetic-child-B"] {
                            emit(
                                &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":child,"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":format!("automatically delivered {child}")}}}}),
                            );
                        }
                        emit(
                            &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":value["params"]["sessionId"],"update":{"sessionUpdate":"replay_complete"}}}),
                        );
                        state(&value["params"]["sessionId"], "idle", None);
                        if scenario.starts_with("child-auto-read-rt") {
                            for child in ["synthetic-child-A", "synthetic-child-B"] {
                                state(&json!(child), "idle", None);
                            }
                        }
                        emit(
                            &json!({"jsonrpc":"2.0","id":value["id"],"result":{"sessionId":value["params"]["sessionId"]}}),
                        );
                    }
                    continue;
                }
                if scenario == "r2-v01-discover"
                    && value["params"]["sessionId"] == "source-native-existing"
                {
                    std::fs::write("source-load-session", b"source-native-existing").unwrap();
                }
                if scenario == "cause-resume-error" {
                    emit(
                        &json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":-32603,"message":"synthetic resume failure","data":{"_future":null}}}),
                    );
                    continue;
                }
                if matches!(scenario.as_str(), "read-first" | "read-first-held")
                    && value["method"] == "session/load"
                {
                    read_first_replays += 1;
                    std::fs::write("read-first-replay-count", read_first_replays.to_string())
                        .unwrap();
                    emit(
                        &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":value["params"]["sessionId"],"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"native bridge replay text"}}}}),
                    );
                    let raw = br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"synthetic-native-A","update":{"sessionUpdate":"_future_bridge_event","data":{"z":null,"a":1234567890123456789012345678901234567890,"nested":[true,{"k":"kept"}]}}}}"#;
                    let mut stdout = io::stdout().lock();
                    stdout.write_all(raw).unwrap();
                    stdout.write_all(b"\n").unwrap();
                    stdout.flush().unwrap();
                    if scenario == "read-first-held" {
                        std::fs::write("read-first-held", b"synthetic").unwrap();
                        while !std::path::Path::new("release-read-first-held").is_file() {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        }
                    }
                }
                assert!(value["params"]["cwd"].is_string());
                assert!(value["params"]["mcpServers"].is_array());
                if scenario.starts_with("control-") {
                    emit(
                        &json!({"jsonrpc":"2.0","id":"synthetic-unsupported","method":"_future_method","params":{"_future":{"preserve":[1,null,{"x":true}]}}}),
                    );
                }
                if scenario == "replay-live" && value["params"].get("replayFrom").is_some() {
                    state(&value["params"]["sessionId"], "running", None);
                    state(&value["params"]["sessionId"], "idle", Some("cancelled"));
                    emit(
                        &json!({"jsonrpc":"2.0","id":value["id"],"result":{"sessionId":value["params"]["sessionId"]}}),
                    );
                    std::fs::write("replay-sent", b"synthetic").unwrap();
                } else {
                    if scenario == "source-create-replay-state"
                        || scenario == "source-create-replay-state-verified"
                    {
                        let replay_text = if scenario == "source-create-replay-state-verified" {
                            "capacity fixture replay"
                        } else {
                            "created source replay text"
                        };
                        emit(
                            &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":value["params"]["sessionId"],"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":replay_text}}}}),
                        );
                        if scenario == "source-create-replay-state-verified" {
                            emit_verified_child_bodies();
                        }
                        state(&value["params"]["sessionId"], "running", None);
                        if v2 && value["params"].get("replayFrom").is_some() {
                            emit(
                                &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":value["params"]["sessionId"],"update":{"sessionUpdate":"replay_complete"}}}),
                            );
                        }
                        state(&value["params"]["sessionId"], "idle", Some("cancelled"));
                        if std::path::Path::new("capacity-race-before-close-child").is_file() {
                            emit_race_child(&value["params"]["sessionId"]);
                        }
                    } else if scenario == "external-reads-quarantine" {
                        match value["params"]["sessionId"].as_str() {
                            Some("external-session-state") => {
                                state(&value["params"]["sessionId"], "idle", None);
                            }
                            Some("external-session-permission") => {
                                emit(
                                    &json!({"jsonrpc":"2.0","id":"external-perm-1","method":"session/request_permission","params":{"sessionId":value["params"]["sessionId"],"title":"ext","subject":{"type":"tool_call","toolCall":{"toolCallId":"ext-tool"}},"options":[{"optionId":"opt-1","name":"opt","kind":"_future_kind"}]}}),
                                );
                            }
                            Some("external-session-trigger-unknown") => {
                                state(&json!("completely-unknown-session"), "idle", None);
                            }
                            _ => {}
                        }
                    } else if v2 {
                        state(&value["params"]["sessionId"], "idle", None);
                    }
                    emit(
                        &json!({"jsonrpc":"2.0","id":value["id"],"result":{"sessionId":value["params"]["sessionId"]}}),
                    );
                }
            }
            Some("session/list") => {
                bump_source_method("session/list");
                diagnostic("list-seen", &serde_json::to_vec(&value).unwrap());
                let request_path = std::path::Path::new("source-list-request.jsonl");
                let mut prior = std::fs::read(request_path).unwrap_or_default();
                prior.extend_from_slice(&serde_json::to_vec(&value["params"]).unwrap());
                prior.push(b'\n');
                std::fs::write(request_path, prior).unwrap();
                let count_path = std::path::Path::new("source-list-count");
                let count = std::fs::read_to_string(count_path)
                    .ok()
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0)
                    + 1;
                std::fs::write(count_path, count.to_string()).unwrap();
                if value["params"]["cursor"] == "remote-error" {
                    emit(
                        &json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":-32602,"message":"SOURCE_REPLY_CANARY","data":{"retained":true}}}),
                    );
                } else if scenario == "source-create-replay-state-verified" {
                    if let Some(parent) = verified_root.as_deref() {
                        emit_verified_children(parent);
                    }
                    emit(&json!({"jsonrpc":"2.0","id":value["id"],"result":{"sessions":[]}}));
                } else if scenario == "r2-v01-discover"
                    && value["params"]["cursor"] == "opaque-next-cursor"
                {
                    let id = serde_json::to_string(&value["id"]).unwrap();
                    let raw = format!(
                        r#"{{"jsonrpc":"2.0","id":{id},"result":{{"sessions":[{{"sessionId":"external-session-B","title":"PAGE2_CANARY","cwd":"C:/synthetic/workspace"}}],"_futureListing":{{"page":2}}}}}}"#
                    );
                    let mut stdout = io::stdout().lock();
                    stdout.write_all(raw.as_bytes()).unwrap();
                    stdout.write_all(b"\n").unwrap();
                    stdout.flush().unwrap();
                } else if matches!(scenario.as_str(), "list-4097" | "list-4098-partial-fail") {
                    let id = serde_json::to_string(&value["id"]).unwrap();
                    let mut raw = format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{{"sessions":["#);
                    for idx in 0..4097usize {
                        if idx > 0 {
                            raw.push(',');
                        }
                        let _ = write!(
                            raw,
                            r#"{{"sessionId":"session-{idx:04}","cwd":"C:/synthetic/workspace"}}"#
                        );
                    }
                    if scenario == "list-4098-partial-fail" {
                        raw.push_str(r#",{"sessionId":42,"cwd":"C:/synthetic/workspace"}"#);
                    }
                    raw.push_str(r"]}}");
                    let mut stdout = io::stdout().lock();
                    stdout.write_all(raw.as_bytes()).unwrap();
                    stdout.write_all(b"\n").unwrap();
                    stdout.flush().unwrap();
                } else {
                    let id = serde_json::to_string(&value["id"]).unwrap();
                    let raw = format!(
                        r#"{{"jsonrpc":"2.0","id":{id},"result":{{"sessions":[{{"sessionId":"external-session-A","cwd":"C:/synthetic/workspace","escaped":"line1\n\"quoted\"\\slash","_future":{{"b":1.2300e+09,"a":null}},"opaqueBigInteger":1234567890123456789012345678901234567890}}],"nextCursor":"opaque-next-cursor","_futureListing":{{"preserve":[true,null,{{"x":"kept"}}]}}}}}}"#
                    );
                    let mut stdout = io::stdout().lock();
                    stdout.write_all(raw.as_bytes()).unwrap();
                    stdout.write_all(b"\n").unwrap();
                    stdout.flush().unwrap();
                }
                diagnostic("list-replied", b"yes");
            }
            Some("session/new")
                if scenario == "read-first-held" || scenario.starts_with("stop-v2-") =>
            {
                emit(
                    &json!({"jsonrpc":"2.0","id":value["id"],"result":{"sessionId":"synthetic-native-A"}}),
                );
            }
            Some("session/new")
                if matches!(
                    scenario.as_str(),
                    "r2-v01-discover"
                        | "source-create-replay-state"
                        | "source-create-replay-state-verified"
                        | "external-reads-quarantine"
                        | "normal"
                        | "exit-create"
                        | "delayed-create"
                        | "no-list-with-new"
                        | "no-replay-source"
                        | "wrong-reply-stays-unknown"
                        | "list-4097"
                        | "list-4098-partial-fail"
                ) =>
            {
                diagnostic("create-seen", &serde_json::to_vec(&value).unwrap());
                let request_path = std::path::Path::new("source-new-request.jsonl");
                let mut prior = std::fs::read(request_path).unwrap_or_default();
                prior.extend_from_slice(&serde_json::to_vec(&value["params"]).unwrap());
                prior.push(b'\n');
                std::fs::write(request_path, prior).unwrap();
                let count_path = std::path::Path::new("source-create-count");
                let count = std::fs::read_to_string(count_path)
                    .ok()
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0)
                    + 1;
                std::fs::write(count_path, count.to_string()).unwrap();
                if scenario == "exit-create" {
                    break;
                }
                if scenario == "wrong-reply-stays-unknown" {
                    emit(
                        &json!({"jsonrpc":"2.0","id":999_999,"result":{"sessionId":"forged-sid"}}),
                    );
                    break;
                }
                if scenario == "delayed-create" {
                    std::fs::write("source-create-waiting", b"waiting").unwrap();
                    while !std::path::Path::new("source-release-create").is_file() {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                }
                let id = serde_json::to_string(&value["id"]).unwrap();
                let created_native = if scenario == "source-create-replay-state-verified" {
                    format!("source-native-created-{count}")
                } else {
                    "source-native-created".to_owned()
                };
                let raw = if scenario == "source-create-replay-state-verified" {
                    format!(
                        r#"{{"jsonrpc":"2.0","id":{id},"result":{{"sessionId":"{created_native}"}}}}"#
                    )
                } else {
                    format!(
                        r#"{{"jsonrpc":"2.0","id":{id},"result":{{"sessionId":"source-native-created","_future":{{"kept":true,"num":1.2300e+09}}}}}}"#
                    )
                };
                let mut stdout = io::stdout().lock();
                stdout.write_all(raw.as_bytes()).unwrap();
                stdout.write_all(b"\n").unwrap();
                stdout.flush().unwrap();
                diagnostic("create-replied", b"yes");
                if scenario == "source-create-replay-state-verified" {
                    let idle_flag = std::path::Path::new("runtime-next-created-idle");
                    if idle_flag.is_file() {
                        std::fs::remove_file(idle_flag).unwrap();
                        state(&json!(created_native), "idle", None);
                        let deferred_native = created_native.clone();
                        std::thread::spawn(move || {
                            let deadline =
                                std::time::Instant::now() + std::time::Duration::from_secs(180);
                            while std::time::Instant::now() < deadline {
                                if std::path::Path::new("l6-deferred-current-ready").is_file() {
                                    state(&json!(deferred_native), "running", None);
                                    break;
                                }
                                std::thread::sleep(std::time::Duration::from_millis(2));
                            }
                        });
                    } else if std::path::Path::new("capacity-protect-created-roots").is_file() {
                        state(&json!(created_native), "running", None);
                    }
                    verified_root = Some(created_native);
                }
            }
            Some("session/prompt") => {
                bump_source_method("session/prompt");
                if scenario.starts_with("control-") {
                    assert!(
                        unsupported_written,
                        "ordinary prompt cannot bypass unresolved safe control"
                    );
                    std::fs::write("control-prompt", b"synthetic").unwrap();
                }
                if scenario == "fatal" {
                    let mut output = io::stdout().lock();
                    output.write_all(b"{synthetic-invalid-frame}\n").unwrap();
                    output.flush().unwrap();
                    continue;
                }
                prompts += 1;
                if scenario == "priority-v2-cancelled"
                    && prompts == 2
                    && let Some(previous) = late_prompt.take()
                {
                    emit(
                        &json!({"jsonrpc":"2.0","id":previous,"result":{"messageId":"synthetic-late-original-inserted"}}),
                    );
                }
                if scenario == "unknown-notification" && prompts == 1 {
                    emit(
                        &json!({"jsonrpc":"2.0","method":"_future_notification","params":{"_future":{"keep":[1,null,{"nested":true}]}}}),
                    );
                }
                assert!(value["params"]["prompt"].is_array());
                if scenario.starts_with("unknown-shutdown-") {
                    if v2 {
                        state(&value["params"]["sessionId"], "running", None);
                        emit(
                            &json!({"jsonrpc":"2.0","id":value["id"],"result":{"messageId":"synthetic-unknown-inserted"}}),
                        );
                    }
                    std::fs::write("unknown-prompt", b"synthetic").unwrap();
                    continue;
                }
                if scenario == "edit-prepared" {
                    std::fs::write(
                        "prompt-content.json",
                        serde_json::to_vec(&value["params"]["prompt"]).unwrap(),
                    )
                    .unwrap();
                }
                if scenario.starts_with("early-stop-v2-") {
                    std::fs::write("prompt-count", prompts.to_string()).unwrap();
                }
                let permission_prompt = scenario == "permission"
                    || scenario == "storage-close-permission"
                    || ((scenario.starts_with("priority-") || scenario == "shared-live")
                        && prompts == 1);
                if v2 && permission_prompt {
                    state(&value["params"]["sessionId"], "running", None);
                }
                if permission_prompt {
                    prompt = Some(value["id"].clone());
                    prompt_session = Some(value["params"]["sessionId"].clone());
                    let options = json!([{"optionId":"synthetic-option","name":"synthetic","kind":"_future_kind"}]);
                    let params = if v2 {
                        json!({"sessionId":value["params"]["sessionId"],"title":"synthetic","subject":{"type":"tool_call","toolCall":{"toolCallId":"synthetic-tool"}},"options":options})
                    } else {
                        json!({"sessionId":value["params"]["sessionId"],"toolCall":{"toolCallId":"synthetic-tool","title":"synthetic","kind":"_future_kind"},"options":options})
                    };
                    emit(
                        &json!({"jsonrpc":"2.0","id":"synthetic-permission","method":"session/request_permission","params":params}),
                    );
                } else {
                    if v2 {
                        state(&value["params"]["sessionId"], "running", None);
                        if !(scenario.starts_with("early-stop-v2-") && prompts == 1) {
                            emit(
                                &json!({"jsonrpc":"2.0","id":value["id"],"result":{"messageId":format!("synthetic-inserted-{prompts}")}}),
                            );
                        }
                    }
                    if scenario == "replay-live" {
                        prompt = Some(value["params"]["sessionId"].clone());
                        std::fs::write("live-prompt-ready", b"synthetic").unwrap();
                        continue;
                    }
                    if scenario == "early-stop-v2-never-insertion" && prompts == 1 {
                        eprintln!("native-dispatch-dev: never-insertion prompt ready");
                        std::fs::write("first-prompt-ready", b"synthetic").unwrap();
                        while !std::path::Path::new("release-first-prompt").is_file() {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        }
                        state(&value["params"]["sessionId"], "idle", Some("max_tokens"));
                        std::fs::write("early-idle-sent", b"synthetic").unwrap();
                        continue;
                    }
                    if let Some(reason) = scenario.strip_prefix("early-stop-v2-")
                        && prompts == 1
                    {
                        eprintln!("native-dispatch-dev: early prompt ready");
                        std::fs::write("first-prompt-ready", b"synthetic").unwrap();
                        while !std::path::Path::new("release-first-prompt").is_file() {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        }
                        eprintln!("native-dispatch-dev: emitting early idle");
                        state(&value["params"]["sessionId"], "idle", Some(reason));
                        std::fs::write("early-idle-sent", b"synthetic").unwrap();
                        eprintln!("native-dispatch-dev: early idle emitted");
                        let prompt_id = value["id"].clone();
                        std::thread::spawn(move || {
                            while !std::path::Path::new("release-late-insertion").is_file() {
                                std::thread::sleep(std::time::Duration::from_millis(2));
                            }
                            eprintln!("native-dispatch-dev: emitting late insertion");
                            emit(
                                &json!({"jsonrpc":"2.0","id":prompt_id,"result":{"messageId":"synthetic-late-inserted"}}),
                            );
                            eprintln!("native-dispatch-dev: late insertion emitted");
                        });
                        continue;
                    }
                    let priority_reason = if prompts == 2 {
                        scenario
                            .strip_prefix("priority-v1-")
                            .or_else(|| scenario.strip_prefix("priority-v2-"))
                            .map(|reason| reason.strip_suffix("-never-insert").unwrap_or(reason))
                    } else {
                        None
                    };
                    let reason = if prompts == 1 {
                        scenario
                            .strip_prefix("stop-v1-")
                            .or_else(|| scenario.strip_prefix("stop-v2-"))
                    } else {
                        priority_reason
                    };
                    if prompts == 1 && reason.is_some() {
                        std::fs::write("first-prompt-ready", b"synthetic").unwrap();
                        while !std::path::Path::new("release-first-prompt").is_file() {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        }
                    }
                    let reason = reason.unwrap_or("end_turn");
                    if !v2 && reason == "error" {
                        emit(
                            &json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":-32603,"message":"synthetic priority failure","data":{"_future":null}}}),
                        );
                    } else if v2 {
                        state(&value["params"]["sessionId"], "idle", Some(reason));
                    } else {
                        let mut result = json!({"stopReason":reason});
                        if reason == "refusal" {
                            result["followUpContext"] = json!({
                                "owner": "sdk",
                                "kind": "refusal-follow-up",
                                "_future": {"sdk-owned-context": [1, true, {"kept": null}]}
                            });
                        }
                        emit(&json!({"jsonrpc":"2.0","id":value["id"],"result":result}));
                    }
                }
            }
            Some("session/cancel") => {
                assert!(value.get("id").is_none());
                if let Some(prompt) = prompt.take() {
                    if scenario.starts_with("priority-v2-cancelled") {
                        late_prompt = Some(prompt.clone());
                    }
                    let session = prompt_session.take().unwrap_or_else(|| prompt.clone());
                    if v2 {
                        state(&session, "idle", Some("cancelled"));
                    } else {
                        emit(
                            &json!({"jsonrpc":"2.0","id":prompt,"result":{"stopReason":"cancelled"}}),
                        );
                    }
                }
            }
            Some("session/close") => {
                if std::path::Path::new("capacity-race-after-close-child").is_file() {
                    emit_race_child(&value["params"]["sessionId"]);
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
                    while !std::path::Path::new("capacity-race-release-closed").is_file() {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "owned Closed barrier expired"
                        );
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                }
                if std::path::Path::new("runtime-auto-count-close").is_file() {
                    let n = std::fs::read_to_string("rt-auto-close-count")
                        .ok()
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0)
                        + 1;
                    std::fs::write("rt-auto-close-count", n.to_string()).unwrap();
                }
                if scenario.starts_with("child-auto-read-rt") {
                    assert_eq!(value["params"]["sessionId"], "synthetic-native-A");
                    let n = std::fs::read_to_string("rt-child-close-count")
                        .ok()
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0)
                        + 1;
                    std::fs::write("rt-child-close-count", n.to_string()).unwrap();
                }
                emit(&json!({"jsonrpc":"2.0","id":value["id"],"result":{}}));
            }
            None if value["id"] == "synthetic-unsupported" => {
                assert!(!unsupported_written);
                assert_eq!(value["error"]["code"], -32601);
                unsupported_written = true;
                std::fs::write("unsupported-written", b"synthetic").unwrap();
            }
            None if value["id"] == "synthetic-permission" || value["id"] == "external-perm-1" => {
                assert_eq!(value["result"]["outcome"]["outcome"], "cancelled");
                std::fs::write("permission-cancelled", b"synthetic").unwrap();
            }
            _ => panic!("unexpected synthetic native dev method"),
        }
    }
}
fn emit_race_child(parent: &Value) {
    emit(&json!({"jsonrpc":"2.0","method":"session/update","params":{
        "sessionId":parent,"update":{"sessionUpdate":"subagent_update",
        "sessionId":"capacity-race-late-child","state":{"state":"running","background":[]}}}}));
    state(&json!("capacity-race-late-child"), "running", None);
    std::fs::write("capacity-race-child-emitted", b"standard-notification").unwrap();
}
fn emit_verified_children(parent: &str) {
    for child in ["rt-direct-child-A", "rt-direct-child-B"] {
        emit(&json!({"jsonrpc":"2.0","method":"session/update","params":{
            "sessionId":parent,"update":{"sessionUpdate":"subagent_update",
            "sessionId":child,"state":{"state":"idle","background":[]}}}}));
        state(&json!(child), "idle", None);
    }
}
fn emit_verified_child_bodies() {
    for (child, text) in [
        ("rt-direct-child-A", "RT actual alpha child A"),
        ("rt-direct-child-B", "RT actual beta child B"),
    ] {
        emit(&json!({"jsonrpc":"2.0","method":"session/update","params":{
            "sessionId":child,"update":{"sessionUpdate":"agent_message_chunk",
            "content":{"type":"text","text":text}}}}));
    }
}
fn bump_source_method(method: &str) {
    let filename = match method {
        "session/load" => "source-load-count",
        "session/resume" => "source-resume-count",
        "session/prompt" => "source-prompt-count",
        "session/list" => "source-list-count",
        _ => return,
    };
    let count = std::fs::read_to_string(filename)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
        + 1;
    std::fs::write(filename, count.to_string()).unwrap();
}
fn state(session: &Value, value: &str, reason: Option<&str>) {
    let mut update = json!({"sessionUpdate":"state_update","state":value});
    if let Some(reason) = reason {
        update["stopReason"] = json!(reason);
        if reason == "refusal" {
            update["followUpContext"] = json!({
                "owner": "sdk",
                "kind": "refusal-follow-up",
                "_future": {"sdk-owned-context": [1, true, {"kept": null}]}
            });
        }
    }
    emit(
        &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":update}}),
    );
}
