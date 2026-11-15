//! Opt-in synthetic ACP peer. Reads/writes actual unbuffered stdio, no application data.
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
fn emit(value: &Value) {
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, value).unwrap();
    out.write_all(b"\n").unwrap();
    out.flush().unwrap();
}
fn main() {
    let mut version = 1;
    let mut prompt = None;
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        match v["method"].as_str() {
            Some("initialize") => {
                version = v["params"]["protocolVersion"].as_u64().unwrap();
                let result = if version == 1 {
                    json!({"protocolVersion":1,"agentInfo":{"name":"independent-pipe-child","version":"synthetic"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{},"close":{}}}})
                } else {
                    json!({"protocolVersion":2,"info":{"name":"independent-pipe-child","version":"synthetic"},"capabilities":{"session":{}}})
                };
                emit(&json!({"jsonrpc":"2.0","id":v["id"],"result":result}));
            }
            Some("session/new" | "session/resume" | "session/load") => {
                assert!(v["params"]["cwd"].is_string());
                assert!(v["params"]["mcpServers"].is_array());
                emit(&json!({"jsonrpc":"2.0","id":v["id"],"result":{"sessionId":"A"}}));
            }
            Some("session/prompt") => {
                assert!(v["params"]["prompt"].is_array());
                prompt = Some(v["id"].clone());
                let params = if version == 1 {
                    json!({"sessionId":"A","toolCall":{"toolCallId":"t","title":"independent"},"options":[{"optionId":"allow","name":"Allow synthetic","kind":"allow_once"}]})
                } else {
                    json!({"sessionId":"A","title":"independent","subject":{"type":"_synthetic_future"},"options":[{"optionId":"allow","name":"Allow synthetic","kind":"allow_once"}]})
                };
                emit(
                    &json!({"jsonrpc":"2.0","id":"nested","method":"session/request_permission","params":params}),
                );
            }
            Some("session/cancel" | "session/close") => {
                if v.get("id").is_some() {
                    emit(&json!({"jsonrpc":"2.0","id":v["id"],"result":{}}));
                }
            }
            None if v["id"] == "nested" => {
                let result = if version == 1 {
                    json!({"stopReason":"end_turn"})
                } else {
                    json!({"messageId":"inserted"})
                };
                emit(&json!({"jsonrpc":"2.0","id":prompt.take().unwrap(),"result":result}));
            }
            _ => panic!("unexpected synthetic protocol method"),
        }
    }
}
