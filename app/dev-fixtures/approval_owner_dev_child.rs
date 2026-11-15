//! Opt-in local approval peer. It advertises an unknown kind and a genuine empty option ID.
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
fn emit(value: &Value) -> io::Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, value)?;
    out.write_all(b"\n")?;
    out.flush()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let expect_cancelled = std::env::args().any(|argument| argument == "--expect-cancelled");
    let request_session = if std::env::args().any(|a| a == "--unassociated-request") {
        "synthetic-native-B"
    } else {
        "synthetic-native-A"
    };
    let mut resume = None;
    let mut selected = false;
    for line in io::stdin().lock().lines() {
        let value: Value = serde_json::from_str(&line?)?;
        match value["method"].as_str() {
            Some("initialize") => {
                emit(
                    &json!({"jsonrpc":"2.0","id":value["id"],"result":{"protocolVersion":1,"agentInfo":{"name":"approval-owner-dev","version":"synthetic"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{},"close":{}}}}}),
                )?;
                emit(
                    &json!({"jsonrpc":"2.0","id":"approval-original-empty","method":"session/request_permission","params":{"sessionId":request_session,"toolCall":{"toolCallId":"approval-dev-tool","title":"synthetic inert title","kind":"_future_kind","_unknown":{"retain":[null,true,3]}},"options":[{"optionId":"","name":"synthetic inert option","kind":"_future_kind","_unknown":{"raw":[1,null]}}],"_extension":{"keep":true}}}),
                )?;
            }
            Some("session/load" | "session/resume") => {
                if selected {
                    emit(
                        &json!({"jsonrpc":"2.0","id":value["id"],"result":{"sessionId":value["params"]["sessionId"]}}),
                    )?;
                } else {
                    resume = Some(value);
                }
            }
            Some("session/close") => emit(&json!({"jsonrpc":"2.0","id":value["id"],"result":{}}))?,
            Some("session/cancel") => {}
            None if value["id"] == "approval-original-empty" => {
                if expect_cancelled {
                    if value["result"]["outcome"]["outcome"] != "cancelled" {
                        return Err("expected original Cancelled reply".into());
                    }
                } else if value["result"]["outcome"]["outcome"] != "selected"
                    || value["result"]["outcome"]["optionId"] != ""
                {
                    return Err("original empty ID was not selected".into());
                }
                if selected {
                    return Err("duplicate permission response".into());
                }
                selected = true;
                std::fs::write(
                    if expect_cancelled {
                        "approval-cancelled"
                    } else {
                        "approval-selected"
                    },
                    b"synthetic permission receipt",
                )?;
                if let Some(resume) = resume.take() {
                    emit(
                        &json!({"jsonrpc":"2.0","id":resume["id"],"result":{"sessionId":resume["params"]["sessionId"]}}),
                    )?;
                }
            }
            _ => return Err("unexpected synthetic approval peer input".into()),
        }
    }
    Ok(())
}
