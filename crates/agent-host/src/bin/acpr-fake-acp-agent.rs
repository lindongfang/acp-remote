//! 行为可控的 fake ACP Agent（测试与回归用，**不是**产品的一部分）。
//!
//! 场景只由 **argv** 选择（`--scenario <name>`）：子进程环境先被清空、只注入白名单变量，测试因此不能
//! 依赖环境变量传参。所有场景都只经 stdio 的 LF 分隔 JSON 通信，与真实 Agent 的分帧一致。
//!
//! 场景清单见 `README`/`tasks.md` 2.13：`normal`、`chunked-updates`、`permission-request`、
//! `elicitation`、`slow-initialize`、`no-response`、`illegal-json`、`unknown-id`、`out-of-order`、
//! `crash-on-prompt`、`spawn-grandchild`、`heartbeat-child`，另有 `--dump-env <path>` 便于断言
//! 注入给子进程的环境变量集合。

use std::io::{BufRead, Write};
use std::process::ExitCode;

use serde_json::{Value, json};

/// 一次运行的参数。
struct Args {
    scenario: String,
    heartbeat_file: Option<String>,
    dump_env: Option<String>,
    capabilities: Value,
    /// `session/new` 不返回 `modes`（用于「未宣告」路径）。
    no_modes: bool,
    /// `session/new` 不返回 `configOptions`（用于「未宣告」路径）。
    no_config_options: bool,
    /// 收到任何配置写入请求就退出：用来证明「拒绝时真的没有发消息」。
    exit_on_config_write: bool,
}

impl Args {
    fn parse() -> Self {
        let argv: Vec<String> = std::env::args().collect();
        let mut args = Self {
            scenario: "normal".to_owned(),
            heartbeat_file: None,
            dump_env: None,
            capabilities: json!({}),
            no_modes: false,
            no_config_options: false,
            exit_on_config_write: false,
        };
        let mut index = 1;
        while index < argv.len() {
            let key = argv[index].as_str();
            // 值是可选的：下一个 token 若以 `--` 开头，说明当前是个开关（否则开关会吞掉后面的开关）。
            let has_value = argv
                .get(index + 1)
                .is_some_and(|next| !next.starts_with("--"));
            let value = has_value.then(|| argv[index + 1].clone());
            match (key, value) {
                ("--scenario", Some(value)) => args.scenario = value,
                ("--heartbeat-file", Some(value)) => args.heartbeat_file = Some(value),
                ("--dump-env", Some(value)) => args.dump_env = Some(value),
                ("--capabilities", Some(value)) => {
                    args.capabilities = serde_json::from_str(&value).unwrap_or_else(|_| json!({}));
                }
                ("--no-modes", _) => args.no_modes = true,
                ("--no-config-options", _) => args.no_config_options = true,
                ("--exit-on-config-write", _) => args.exit_on_config_write = true,
                _ => {}
            }
            index += if has_value { 2 } else { 1 };
        }
        args
    }
}

/// 待结清的 prompt（ACP 请求 id 原样保存）。
#[derive(Default)]
struct State {
    next_id: u64,
    /// 尚未响应的 prompt：`(所属会话, 请求 id)`。
    prompts: Vec<(String, Value)>,
    /// `session/new` 的调用序号（每个会话一个 ACP 会话标识）。
    session_seq: u64,
    /// 我们发出去的权限/elicitation 请求：(请求 id, 类别, 对应的 prompt id)。
    ///
    /// 类别用于校验回包形状：回传了错误的 id 或错误的动作时以 `refusal` 结清，
    /// 从而让「回传不保真」在测试里表现为可观察的失败，而不是被默默接受。
    interactions: Vec<(u64, &'static str, Value)>,
    prompt_seq: u64,
}

impl State {
    fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

fn main() -> ExitCode {
    let args = Args::parse();

    if args.scenario == "heartbeat-child" {
        return heartbeat_child(&args);
    }
    if let Some(path) = &args.dump_env {
        let mut lines: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
        lines.sort();
        let _ = std::fs::write(path, lines.join("\n"));
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut state = State::default();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            // 对端发来的非法 JSON 不是本 fake 的职责范围：忽略。
            continue;
        };
        handle(&message, &mut state, &mut out, &args);
    }
    ExitCode::SUCCESS
}

/// 心跳子进程：每 50 ms 追加一个字节，直到被杀。
fn heartbeat_child(args: &Args) -> ExitCode {
    let Some(path) = args.heartbeat_file.clone() else {
        return ExitCode::from(2);
    };
    loop {
        let mut file = match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(_) => return ExitCode::from(3),
        };
        if file.write_all(b".").is_err() {
            return ExitCode::from(4);
        }
        let _ = file.flush();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn handle(message: &Value, state: &mut State, out: &mut impl Write, args: &Args) {
    let method = message.get("method").and_then(Value::as_str);
    let id = message.get("id").cloned();

    match (method, id) {
        (Some("initialize"), Some(id)) => {
            if args.scenario == "illegal-json" {
                // 先写一条非法 JSON（不是合法 JSON-RPC 文档），再正常响应。
                let _ = writeln!(out, "{{ this is not json");
                let _ = out.flush();
            }
            if args.scenario == "slow-initialize" {
                // 超过固定启动超时（10 s），用于断言 initialize 超时路径。
                std::thread::sleep(std::time::Duration::from_secs(20));
            }
            respond(
                out,
                &id,
                json!({
                    "protocolVersion": 1,
                    "agentCapabilities": args.capabilities.clone(),
                    "agentInfo": { "name": "acpr-fake-acp-agent", "version": "0.0.0" },
                }),
            );
        }
        (Some("session/new"), Some(id)) => {
            state.session_seq += 1;
            let session_id = format!("acp-session-{}", state.session_seq);
            let mut result = serde_json::Map::new();
            result.insert("sessionId".to_owned(), json!(session_id));
            if !args.no_modes {
                result.insert(
                    "modes".to_owned(),
                    json!({
                        "currentModeId": "default",
                        "availableModes": [
                            { "id": "default", "name": "Default" },
                            { "id": "plan", "name": "Plan" },
                        ],
                    }),
                );
            }
            if !args.no_config_options {
                result.insert(
                    "configOptions".to_owned(),
                    json!([
                        {
                            "id": "verbose",
                            "name": "Verbose",
                            "description": "详细输出",
                            "category": "output",
                            "type": "boolean",
                            "currentValue": false,
                        },
                        {
                            "id": "model",
                            "name": "Model",
                            "type": "select",
                            "currentValue": "small",
                            "options": [
                                { "value": "small", "name": "Small" },
                                { "value": "large", "name": "Large", "description": "更大" },
                            ],
                        },
                    ]),
                );
            }
            respond(out, &id, Value::Object(result));
        }
        (Some("session/set_mode"), Some(id)) => {
            if args.exit_on_config_write {
                // 收到不该被发送的请求：立即退出，让「未宣告也发消息」变成可观察的失败。
                std::process::exit(7);
            }
            respond(out, &id, json!({}));
            let session = message
                .get("params")
                .and_then(|params| params.get("sessionId"))
                .and_then(Value::as_str)
                .unwrap_or("acp-session-1");
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": { "sessionUpdate": "current_mode_update", "currentModeId": "plan" },
                }),
            );
        }
        (Some("session/set_config_option"), Some(id)) => {
            if args.exit_on_config_write {
                std::process::exit(7);
            }
            respond(out, &id, json!({}));
        }
        (Some("session/prompt"), Some(id)) => {
            state.prompt_seq += 1;
            let session = message
                .get("params")
                .and_then(|params| params.get("sessionId"))
                .and_then(Value::as_str)
                .unwrap_or("acp-session-1")
                .to_owned();
            start_prompt(state, out, args, id, &session);
        }
        (Some("session/cancel"), None) => {
            // 取消：只结清**属于该会话**的未响应 prompt（取消不得影响其它会话）。
            let target = message
                .get("params")
                .and_then(|params| params.get("sessionId"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let mut remaining = Vec::new();
            let mut cancelled = Vec::new();
            for (session, prompt) in std::mem::take(&mut state.prompts) {
                if session == target {
                    cancelled.push(prompt);
                } else {
                    remaining.push((session, prompt));
                }
            }
            state.prompts = remaining;
            for prompt in cancelled {
                respond(out, &prompt, json!({ "stopReason": "cancelled" }));
            }
        }
        // 我们发出去的请求收到了响应（权限/elicitation）：校验后结清对应的 prompt。
        (None, Some(id)) => {
            let number = id.as_u64();
            let mut remaining = Vec::new();
            let mut matched: Vec<(&'static str, Value)> = Vec::new();
            for (request_id, kind, prompt) in std::mem::take(&mut state.interactions) {
                if Some(request_id) == number {
                    matched.push((kind, prompt));
                } else {
                    remaining.push((request_id, kind, prompt));
                }
            }
            state.interactions = remaining;
            for (kind, prompt) in matched {
                // 权限必须带回 `outcome.selected.outcome + outcome.optionId`，
                // elicitation 必须带回 `action`：缺一即判定回传不保真。
                let ok = match kind {
                    "permission" => {
                        let outcome = message.get("result").and_then(|value| value.get("outcome"));
                        outcome
                            .and_then(|value| value.get("outcome"))
                            .and_then(Value::as_str)
                            == Some("selected")
                            && outcome
                                .and_then(|value| value.get("optionId"))
                                .and_then(Value::as_str)
                                .is_some()
                    }
                    "elicitation" => message
                        .get("result")
                        .and_then(|value| value.get("action"))
                        .and_then(Value::as_str)
                        .is_some(),
                    _ => false,
                };
                if ok {
                    respond(out, &prompt, json!({ "stopReason": "end_turn" }));
                } else {
                    respond(out, &prompt, json!({ "stopReason": "refusal" }));
                }
            }
        }
        _ => {}
    }
}

fn start_prompt(state: &mut State, out: &mut impl Write, args: &Args, id: Value, session: &str) {
    match args.scenario.as_str() {
        "crash-on-prompt" => {
            emit_chunk(out, session, "崩溃前的一小段");
            let _ = out.flush();
            std::process::exit(3);
        }
        "no-response" => {
            // 只登记、不响应：用于断言 `session/prompt` 不设超时。
            state.prompts.push((session.to_owned(), id));
        }
        "out-of-order" => {
            state.prompts.push((session.to_owned(), id));
            if state.prompts.len() >= 2 {
                // 后到的 id 先响应，先到的后响应：pending 注册表必须仍然能一一对上。
                let mut prompts = std::mem::take(&mut state.prompts);
                prompts.reverse();
                for (_, prompt) in prompts {
                    respond(out, &prompt, json!({ "stopReason": "end_turn" }));
                }
            }
        }
        "spawn-grandchild" => {
            let file = args
                .heartbeat_file
                .clone()
                .unwrap_or_else(|| "heartbeat.txt".to_owned());
            let exe = std::env::current_exe().unwrap_or_default();
            let _ = std::process::Command::new(exe)
                .args(["--scenario", "heartbeat-child", "--heartbeat-file", &file])
                .spawn();
            // 故意不响应：父进程与孙进程都活着，直到测试结束整棵树。
            state.prompts.push((session.to_owned(), id));
        }
        "permission-request" => {
            let request_id = state.next_id();
            state.interactions.push((request_id, "permission", id));
            write_line(
                out,
                &json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "method": "session/request_permission",
                    "params": {
                        "sessionId": session,
                        "toolCall": {
                            "toolCallId": "tool-1",
                            "title": "写入文件",
                            "kind": "edit",
                            "status": "pending",
                            "rawInput": { "path": "a.txt" },
                        },
                        "options": [
                            { "optionId": "allow-1", "name": "允许一次", "kind": "allow_once" },
                            { "optionId": "reject-1", "name": "拒绝", "kind": "reject_once" },
                        ],
                    },
                }),
            );
        }
        "elicitation" => {
            let request_id = state.next_id();
            state.interactions.push((request_id, "elicitation", id));
            write_line(
                out,
                &json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "method": "elicitation/create",
                    "params": {
                        "message": "请补充提交信息",
                        "mode": "form",
                        "requestedSchema": {
                            "type": "object",
                            "properties": { "message": { "type": "string" } },
                        },
                    },
                }),
            );
        }
        "chunked-updates" => {
            emit_chunk(out, session, "第一段");
            emit_chunk(out, session, "第二段");
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "agent_thought_chunk",
                        "content": { "type": "text", "text": "思考片段" },
                        "messageId": "thought-1",
                    },
                }),
            );
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "tool_call",
                        "toolCallId": "tool-1",
                        "title": "编辑文件",
                        "kind": "edit",
                        "status": "in_progress",
                        "locations": [ { "path": "src/main.rs", "line": 12 } ],
                        "content": [
                            {
                                "type": "diff",
                                "path": "src/main.rs",
                                "oldText": "let a = 1;",
                                "newText": "let a = 2;",
                            },
                            { "type": "terminal", "terminalId": "term-1" }
                        ],
                    },
                }),
            );
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "tool_call_update",
                        "toolCallId": "tool-1",
                        "status": "completed",
                        "content": [
                            { "type": "content", "content": { "type": "text", "text": "完成" } }
                        ],
                    },
                }),
            );
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "plan",
                        "entries": [
                            { "content": "第一步", "priority": "high", "status": "completed" },
                            { "content": "第二步", "priority": "medium", "status": "in_progress" },
                        ],
                    },
                }),
            );
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "available_commands_update",
                        "availableCommands": [
                            { "name": "/review", "description": "审查改动" }
                        ],
                    },
                }),
            );
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "current_mode_update",
                        "currentModeId": "plan",
                    },
                }),
            );
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "config_option_update",
                        "configOptions": [
                            { "id": "verbose", "type": "boolean", "currentValue": true }
                        ],
                    },
                }),
            );
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "session_info_update",
                        "title": "会话标题",
                        "updatedAt": "2026-09-24T10:00:00.000Z",
                    },
                }),
            );
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": { "sessionUpdate": "usage_update", "used": 1024, "size": 200000 },
                }),
            );
            // 未来判别子：必须可见降级（不是解码失败）。
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": { "sessionUpdate": "future_thing_update", "detail": { "a": 1 } },
                }),
            );
            respond(out, &id, json!({ "stopReason": "end_turn" }));
        }
        "unknown-id" => {
            emit_chunk(out, session, "内容");
            // 与任何未完成请求都不匹配的响应：必须记为明确协议错误且不影响其它 pending。
            write_line(
                out,
                &json!({ "jsonrpc": "2.0", "id": 987654321u64, "result": { "ghost": true } }),
            );
            respond(out, &id, json!({ "stopReason": "end_turn" }));
        }
        "stderr-flood" => {
            // 写超过环形缓冲上限的 stderr：内容必须被有界采集、丢弃计数必须增长，
            // 而 stdout 通道必须仍然可用（stderr 不得进入 ACP 通道）。
            let line = "S".repeat(4096);
            for _ in 0..192 {
                let _ = writeln!(std::io::stderr(), "{line}");
            }
            let _ = std::io::stderr().flush();
            emit_chunk(out, session, "stderr 洪水之后仍然可用");
            respond(out, &id, json!({ "stopReason": "end_turn" }));
        }
        "unknown-content-block" => {
            // 未登记的 content block：必须可见降级并保留结构化 payload（不得降成 null）。
            notify(
                out,
                "session/update",
                json!({
                    "sessionId": session,
                    "update": {
                        "sessionUpdate": "agent_message_chunk",
                        "messageId": "message-future",
                        "content": { "type": "future_content_block", "data": { "nested": [1, 2, 3] } },
                    },
                }),
            );
            respond(out, &id, json!({ "stopReason": "end_turn" }));
        }
        "huge-line" => {
            // 一条超过上限且**不换行**的输出：读侧必须在超限时结束该 Agent（而不是继续挂着）。
            let filler = "x".repeat(1024 * 1024 + 4096);
            let _ = write!(
                out,
                "{{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\"sessionId\":\"{session}\",\"filler\":\"{filler}\"}}"
            );
            let _ = out.flush();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        "illegal-json" => {
            emit_chunk(out, session, "内容");
            let _ = writeln!(out, "not json at all");
            let _ = out.flush();
            respond(out, &id, json!({ "stopReason": "end_turn" }));
        }
        _ => {
            emit_chunk(out, session, "普通回复");
            respond(out, &id, json!({ "stopReason": "end_turn" }));
        }
    }
}

fn emit_chunk(out: &mut impl Write, session: &str, text: &str) {
    notify(
        out,
        "session/update",
        json!({
            "sessionId": session,
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "messageId": "message-1",
                "content": { "type": "text", "text": text },
            },
        }),
    );
}

fn respond(out: &mut impl Write, id: &Value, result: Value) {
    write_line(
        out,
        &json!({ "jsonrpc": "2.0", "id": id.clone(), "result": result }),
    );
}

fn notify(out: &mut impl Write, method: &str, params: Value) {
    write_line(
        out,
        &json!({ "jsonrpc": "2.0", "method": method, "params": params }),
    );
}

fn write_line(out: &mut impl Write, value: &Value) {
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}
