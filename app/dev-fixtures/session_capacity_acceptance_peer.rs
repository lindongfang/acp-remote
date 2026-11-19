//! Independent synthetic ACP peer. No model, credentials or network are used.
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

fn emit(value: &Value) {
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, value).expect("fixture response");
    out.write_all(b"\n").expect("fixture newline");
    out.flush().expect("fixture flush");
}

fn event(method: &str, params: &Value, native: Option<&str>) {
    let keys: Vec<_> = params
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("capacity-peer-{}.jsonl", std::process::id()))
        .expect("owned fixture event file");
    serde_json::to_writer(
        &mut file,
        &json!({"method":method,"keys":keys,"native":native,"pid":std::process::id()}),
    )
    .expect("fixture event");
    file.write_all(b"\n").expect("fixture event newline");
    file.flush().expect("fixture event flush");
}

fn gate(kind: &str) {
    let hold = format!("capacity-hold-{kind}");
    if std::path::Path::new(&hold).is_file() {
        std::fs::write(format!("capacity-entered-{kind}"), b"entered").expect("fixture gate");
        let release = format!("capacity-release-{kind}");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(25);
        while !std::path::Path::new(&release).is_file() {
            assert!(
                std::time::Instant::now() < until,
                "owned fixture gate deadline"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Current controlled peer keeps complete per-method contracts together"
)]
fn main() {
    let scenario = std::env::args().nth(1).unwrap_or_default();
    let v2 = scenario == "source-create-replay-state";
    let mut created = 0_u32;
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line.expect("fixture stdin"))
            .expect("product must write valid JSON");
        let params = &request["params"];
        let native = params["sessionId"].as_str();
        let Some(method) = request["method"].as_str() else {
            if request["id"] == "runtime-child-permission" {
                assert_eq!(request["result"]["outcome"]["outcome"], "selected");
                assert_eq!(request["result"]["outcome"]["optionId"], "");
                event(
                    "permission.response",
                    &json!({}),
                    Some("runtime-child-leaf"),
                );
                std::fs::write("runtime-approval-selected", b"actual reply received")
                    .expect("owned receipt");
            }
            continue;
        };
        match method {
            "initialize" => {
                event(method, params, None);
                let result = if v2 {
                    json!({"protocolVersion":2,"info":{"name":"capacity-independent-peer","version":"fixture"},"capabilities":{"session":{}}})
                } else {
                    json!({"protocolVersion":1,"agentInfo":{"name":"capacity-independent-peer","version":"fixture"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"list":{},"newSession":{},"resume":{},"close":{}}}})
                };
                emit(&json!({"jsonrpc":"2.0","id":request["id"],"result":result}));
            }
            "session/new" => {
                assert!(
                    params.get("sessionId").is_none(),
                    "session/new identity must originate at peer, never client"
                );
                created += 1;
                let id = if std::path::Path::new("capacity-fixed-id").is_file() {
                    "capacity-shared-short-id".to_owned()
                } else if std::path::Path::new("capacity-duplicate-id").is_file() {
                    format!("capacity-{}-1", std::process::id())
                } else {
                    format!("capacity-{}-{created}", std::process::id())
                };
                event(method, params, Some(&id));
                gate("new");
                let mut result = json!({"sessionId":id});
                if let Ok(mode) = std::fs::read_to_string("capacity-new-unknown-root") {
                    if mode == "top" {
                        result["x-capacity-relation"] = json!({"parent":"unverified-parent"});
                    } else if mode == "meta" {
                        result["_meta"] =
                            json!({"x-capacity-relation":{"parent":"unverified-parent"}});
                    } else {
                        panic!("unsupported owned unknown-root fixture mode");
                    }
                }
                // Current RB-001 regression fixture: created trees are explicitly
                // protected. The one-shot idle flag is used only by new positive
                // automatic cases; old phase1 source remains byte-identical.
                if v2 {
                    let idle_flag = std::path::Path::new("runtime-next-created-idle");
                    let idle = idle_flag.is_file();
                    if idle {
                        std::fs::remove_file(idle_flag).expect("owned one-shot idle flag");
                    }
                    emit(&json!({"jsonrpc":"2.0","method":"session/update","params":{
                        "sessionId":id,"update":{"sessionUpdate":"state_update","state":if idle {"idle"} else {"running"}}
                    }}));
                }
                emit(&json!({"jsonrpc":"2.0","id":request["id"],"result":result}));
            }
            "session/list" => {
                event(method, params, None);
                if let Ok(root) = std::fs::read_to_string("runtime-emit-approval-tree") {
                    std::fs::remove_file("runtime-emit-approval-tree")
                        .expect("owned relation trigger");
                    assert!(v2);
                    for (parent, child) in [
                        (root.as_str(), "runtime-child-parent"),
                        ("runtime-child-parent", "runtime-child-leaf"),
                    ] {
                        emit(
                            &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":parent,"update":{
                                "sessionUpdate":"subagent_update","sessionId":child,"state":{"state":"idle","background":[]}
                            }}}),
                        );
                    }
                    emit(
                        &json!({"jsonrpc":"2.0","id":"runtime-child-permission","method":"session/request_permission","params":{
                            "sessionId":"runtime-child-leaf","title":"inert independent approval",
                            "subject":{"type":"tool_call","toolCall":{"toolCallId":"runtime-inert-tool"}},
                            "options":[{"optionId":"","name":"inert","kind":"_future_kind"}]
                        }}),
                    );
                }

                if let Ok(root) = std::fs::read_to_string("runtime-direct-tree") {
                    for child in ["rt-direct-child-A", "rt-direct-child-B"] {
                        emit(&json!({"jsonrpc":"2.0","method":"session/update","params":{
                            "sessionId":root,"update":{"sessionUpdate":"subagent_update",
                            "sessionId":child,"state":{"state":"idle","background":[]}}
                        }}));
                    }
                }
                if std::path::Path::new("capacity-list-disconnect").is_file() {
                    return;
                }
                emit(&json!({"jsonrpc":"2.0","id":request["id"],"result":{"sessions":[]}}));
            }
            "session/load" | "session/resume" => {
                event(method, params, native);
                gate("replay");
                if std::path::Path::new("capacity-replay-disconnect").is_file() {
                    return;
                }
                emit(
                    &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":native,"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"capacity fixture replay"}}}}),
                );
                if std::path::Path::new("capacity-replay-error").is_file() {
                    emit(&json!({"jsonrpc":"2.0","id":request["id"],"error":{
                        "code":-32603,"message":"synthetic replay failed after actual write"}}));
                    continue;
                }
                if std::path::Path::new("capacity-replay-unknown").is_file() {
                    emit(&json!({"jsonrpc":"2.0","id":request["id"],"result":{
                        "sessionId":native,"x-capacity-unknown-completion":true}}));
                    continue;
                }
                if v2 {
                    if params.get("replayFrom").is_some() {
                        emit(
                            &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":native,"update":{"sessionUpdate":"replay_complete"}}}),
                        );
                    }

                    if std::fs::read_to_string("runtime-direct-tree")
                        .ok()
                        .as_deref()
                        == native
                    {
                        for (child, body) in [
                            ("rt-direct-child-A", "RT actual alpha child A"),
                            ("rt-direct-child-B", "RT actual beta child B"),
                        ] {
                            emit(&json!({"jsonrpc":"2.0","method":"session/update","params":{
                                "sessionId":child,"update":{"sessionUpdate":"agent_message_chunk",
                                "content":{"type":"text","text":body}}
                            }}));
                        }
                    }
                    emit(
                        &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":native,"update":{"sessionUpdate":"state_update","state":if std::path::Path::new("runtime-replay-running").is_file() {"running"} else {"idle"}}}}),
                    );
                }
                emit(&json!({"jsonrpc":"2.0","id":request["id"],"result":{"sessionId":native}}));
            }
            "session/close" => {
                event(method, params, native);
                gate("close");
                if std::path::Path::new("capacity-close-error").is_file() {
                    emit(
                        &json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32603,"message":"synthetic close unknown"}}),
                    );
                } else {
                    emit(&json!({"jsonrpc":"2.0","id":request["id"],"result":{}}));
                }
            }
            "session/prompt" => {
                event(method, params, native);
                gate("prompt");
                if v2 {
                    emit(
                        &json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":native,"update":{"sessionUpdate":"state_update","state":"idle","stopReason":"end_turn"}}}),
                    );
                } else {
                    emit(
                        &json!({"jsonrpc":"2.0","id":request["id"],"result":{"stopReason":"end_turn"}}),
                    );
                }
            }
            "session/cancel" | "notifications/cancelled" => event(method, params, native),
            _ => {
                event(method, &json!({}), None);
                if request.get("id").is_some() {
                    emit(
                        &json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32601,"message":"unsupported synthetic method"}}),
                    );
                }
            }
        }
    }
}
