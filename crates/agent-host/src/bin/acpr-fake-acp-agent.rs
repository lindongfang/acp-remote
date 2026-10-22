//! 行为可控的 fake ACP Agent（测试与回归用，**不是**产品的一部分）。
//!
//! 场景只由 **argv** 选择（`--scenario <name>`）：子进程环境先被清空、只注入白名单变量，测试因此不能
//! 依赖环境变量传参。所有场景都只经 stdio 的 LF 分隔 JSON 通信，与真实 Agent 的分帧一致。
//!
//! 场景清单见 `README`/`tasks.md` 2.13：`normal`、`chunked-updates`、`permission-request`、
//! `permission-request-then-exit`（写出权限请求后退出，用于验证写入确认失败路径）、
//! `elicitation`、`slow-initialize`、`no-response`、`illegal-json`、`unknown-id`、`out-of-order`、
//! `crash-on-prompt`、`exit-after-new`（`session/new` 应答后退出）、`spawn-grandchild`、
//! `heartbeat-child`、`stderr-flood`、
//! `stderr-protocol-noise`（在 **stderr** 上写语法完全合法的 ACP 报文，stdout 仍正常应答），
//! 另有 `--dump-env <path>` 便于断言注入给子进程的环境变量集合。
//!
//! 会话恢复（`session/resume`）的三个场景：`resume-ok`（已宣告 `sessionCapabilities.resume` 且恢复
//! 成功）、`resume-error`（已宣告能力却拒绝恢复，例如原生会话已被清理）、`session-new-error`
//! （`session/new` 返回错误）。`--dump-requests <path>` 把每次收到的带 `method` 的入站报文按行追加
//! method 名，供用例断言「某个请求发过 / 没发过」。
//!
//! `session/close` 的四个场景：`close-error`（明确拒绝）、`close-timeout`（永不答复）、`slow-close`
//! （延迟 300 ms 才 ack）、`close-error-terminal`（拒绝关闭，但随后的 prompt 照常走 `terminal-ok`
//! 的终端流程——用于断言释放失败后绑定仍能服务 `terminal/*`）。
//!
//! `--dump-request-params <path>` 是与它**并列**的独立选项：每次收到带 `method` 的入站报文，向该文件
//! 追加一行单行 JSON `{"method":…,"params":…}`（`params` 缺失时为 `null`），供用例断言**发出去的取值**
//! （例如 `session/resume` 的 `params.sessionId` 与 `params.cwd` 逐字等于期望值）。
//! `--dump-requests` 的冻结语义（每行只有 method）因此保持不变；两个选项可同时给出，各自写各自的文件。
//!
//! `--heartbeat-file <path>` 让子进程在存活期间每 50 ms 追加一个字节（`heartbeat-child` 直接用它，
//! 其余场景另外开一个线程写同一个文件）：进程是否真的结束因此可以在**进程外**观察，而不依赖进程内的
//! `is_running()` 状态位。

use std::io::{BufRead, Write};
use std::process::ExitCode;

use acp_protocol::limits::MAX_MESSAGE_BYTES;
use serde_json::{Value, json};

/// `stderr-protocol-noise` 场景写进 stderr 的唯一 marker（测试用它断言「stderr 不得进入事件流」）。
const STDERR_NOISE_MARKER: &str = "ACPR-STDERR-PROTOCOL-NOISE-MARKER";

/// 一次运行的参数。
struct Args {
    scenario: String,
    heartbeat_file: Option<String>,
    dump_env: Option<String>,
    dump_requests: Option<String>,
    dump_request_params: Option<String>,
    capabilities: Value,
    /// `session/new` 不返回 `modes`（用于「未宣告」路径）。
    no_modes: bool,
    /// `session/new` 不返回 `configOptions`（用于「未宣告」路径）。
    no_config_options: bool,
    /// 收到任何配置写入请求就退出：用来证明「拒绝时真的没有发消息」。
    exit_on_config_write: bool,
    /// `--terminal-child <mode>`：本进程扮演 **Daemon 启动的终端子进程**（`terminal/create` 的
    /// `command` 指向本可执行文件的场景），而不是 ACP Agent。
    terminal_child: Option<String>,
    /// `--terminal-bytes <n>`：`big` 子进程模式写出的字节数。
    terminal_bytes: usize,
    /// `--probe-file <path>`：把内部探针结果按行追加（例如「收到了预期的错误响应」），
    /// 让负面路径在进程外可观察。
    probe_file: Option<String>,
    /// `--terminal-heartbeat <path>`：`grandchild` 终端子进程写心跳的文件（进程树清理探针）。
    terminal_heartbeat: Option<String>,
}

impl Args {
    fn parse() -> Self {
        Self::from_argv(&std::env::args().skip(1).collect::<Vec<String>>())
    }

    /// 从去掉程序名的 argv 解析（与 `parse` 同一实现，便于直接对参数序列做断言）。
    fn from_argv(argv: &[String]) -> Self {
        let mut args = Self {
            scenario: "normal".to_owned(),
            heartbeat_file: None,
            dump_env: None,
            dump_requests: None,
            dump_request_params: None,
            capabilities: json!({}),
            no_modes: false,
            no_config_options: false,
            exit_on_config_write: false,
            terminal_child: None,
            terminal_bytes: 8192,
            probe_file: None,
            terminal_heartbeat: None,
        };
        let mut index = 0;
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
                ("--dump-requests", Some(value)) => args.dump_requests = Some(value),
                ("--dump-request-params", Some(value)) => args.dump_request_params = Some(value),
                ("--capabilities", Some(value)) => {
                    args.capabilities = serde_json::from_str(&value).unwrap_or_else(|_| json!({}));
                }
                ("--terminal-child", Some(value)) => args.terminal_child = Some(value),
                ("--terminal-bytes", Some(value)) => {
                    args.terminal_bytes = value.parse().unwrap_or(8192);
                }
                ("--probe-file", Some(value)) => args.probe_file = Some(value),
                ("--terminal-heartbeat", Some(value)) => args.terminal_heartbeat = Some(value),
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
    /// 进行中的终端探针流程（`terminal/*` 场景）。
    terminal_flow: Option<TerminalFlow>,
}

impl State {
    fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

/// 终端探针场景的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalFlowKind {
    /// 正常输出 + 失败/非零退出 + 释放。
    Happy,
    /// 非零退出。
    NonZero,
    /// 输出预算截断。
    Big,
    /// kill 后仍可读、重复操作收敛。
    Kill,
    /// 留下一个仍在运行的终端（清理探针）。
    Orphan,
    /// 留下一个仍在运行的终端，然后 Agent 崩溃（清理探针）。
    OrphanCrash,
    /// 跨会话访问终端标识必须被拒绝。
    CrossSession,
    /// 未宣告能力时 `terminal/create` 必须被拒绝，且不得启动任何进程。
    NotAdvertised,
    /// `wait_for_exit` 在会话关闭时必须被取消（不悬挂）。
    WaitHang,
    /// 接近 1 MiB 的纯文本输出：`terminal/output` 的响应必须留在 ACP 帧预算内。
    FrameBudget,
    /// 大量换行/ANSI 转义：保留正文放得进 1 MiB，但 JSON 转义后会超帧，必须再次裁剪。
    FrameEscapes,
}

/// 进行中的终端探针流程（fake Agent 侧的状态机）。
struct TerminalFlow {
    kind: TerminalFlowKind,
    session: String,
    prompt: Value,
    /// 当前在等待响应的请求 id。
    request_id: u64,
    step: u8,
    terminal_id: Option<String>,
    /// 上一次 `wait_for_exit` 的退出码（重复等待必须一致）。
    exit_code: Option<u64>,
    /// 输出轮询次数：首次 `terminal/output` 可能早于子进程写出内容（真实异步语义）。
    output_polls: u32,
}

fn main() -> ExitCode {
    let args = Args::parse();

    // 终端探针：本可执行文件被 `terminal/create` 当作命令启动时，扮演普通子进程而不是 ACP Agent。
    if let Some(mode) = args.terminal_child.clone() {
        return run_terminal_child(&args, &mode);
    }
    if args.scenario == "heartbeat-child" {
        return heartbeat_child(&args);
    }
    // `--heartbeat-file`：任何场景都持续写心跳，让「进程真的结束了」在进程外可观察。
    if let Some(path) = args.heartbeat_file.clone() {
        std::thread::spawn(move || {
            let _ = heartbeat_loop(&path);
        });
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
        if let Some(method) = message.get("method").and_then(Value::as_str) {
            dump_inbound(&args, method, message.get("params"));
        }
        handle(&message, &mut state, &mut out, &args);
    }
    if args.scenario == "linger-after-eof" {
        // 故意忽略 stdin EOF 继续活着：用于断言 `Supervisor::shutdown` 在 grace 到期后强制结束整棵树。
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
    ExitCode::SUCCESS
}

/// 心跳子进程：每 50 ms 追加一个字节，直到被杀。
fn heartbeat_child(args: &Args) -> ExitCode {
    let Some(path) = args.heartbeat_file.clone() else {
        return ExitCode::from(2);
    };
    match heartbeat_loop(&path) {
        // 循环只在进程结束时终止，因此正常路径不可达；保留分支以免误报成功。
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::from(3),
    }
}

/// 持续向 `path` 追加一个字节（每 50 ms），直到进程结束或文件不可写。
fn heartbeat_loop(path: &str) -> Result<(), ()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|_| ())?;
    loop {
        if file.write_all(b".").is_err() {
            return Err(());
        }
        let _ = file.flush();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// 终端子进程模式：本可执行文件被 `terminal/create` 当作命令启动时执行。
///
/// 这些模式是**真实子进程**（stdout/stderr 是真实管道），因此可以验证分块输出、跨 UTF-8 边界、
/// 非零退出、输出预算与进程树清理——不是进程内模拟。
fn run_terminal_child(args: &Args, mode: &str) -> ExitCode {
    match mode {
        "echo-utf8" => {
            let mut out = std::io::stdout();
            // 5 个汉字 + 一个 4 字节 emoji（分两次写出，强制跨读取边界）+ 3 个汉字。
            write_stdout(&mut out, "第一段".as_bytes());
            let emoji = "🙂".as_bytes();
            write_stdout(&mut out, &emoji[..2]);
            std::thread::sleep(std::time::Duration::from_millis(30));
            write_stdout(&mut out, &emoji[2..]);
            write_stdout(&mut out, "第二段\n".as_bytes());
            let _ = std::io::stderr().write_all("警告\n".as_bytes());
            let _ = std::io::stderr().flush();
            ExitCode::SUCCESS
        }
        "exit-code" => {
            let mut out = std::io::stdout();
            write_stdout(&mut out, "boom\n".as_bytes());
            let _ = std::io::stderr().write_all("失败原因\n".as_bytes());
            let _ = std::io::stderr().flush();
            ExitCode::from(3)
        }
        "big" => {
            let mut out = std::io::stdout();
            let payload = vec![b'x'; args.terminal_bytes];
            write_stdout(&mut out, &payload);
            write_stdout(&mut out, b"\n");
            ExitCode::SUCCESS
        }
        "escaped" => {
            // 每 9 字节里有 1 个换行（2 字节转义）与 1 个 ESC（6 字节转义），JSON 后约膨胀到 1.44 倍。
            let mut out = std::io::stdout();
            let line = b"\x1b[31mX\n";
            let payload: Vec<u8> = line
                .iter()
                .copied()
                .cycle()
                .take(args.terminal_bytes.saturating_mul(line.len()))
                .collect();
            write_stdout(&mut out, &payload);
            ExitCode::SUCCESS
        }
        "hang" => {
            let mut out = std::io::stdout();
            write_stdout(&mut out, "started\n".as_bytes());
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        "exit-with-background" => {
            // 启动一个**继承 stdout/stderr 的后台子进程**并立即退出：后台进程继续持有管道写端，
            // Daemon 侧因此永远观察不到 EOF。用于验证退出事实不是「管道 EOF」的产物。
            let exe = std::env::current_exe().unwrap_or_default();
            let spawned = std::process::Command::new(exe)
                .args(["--terminal-child", "hang"])
                .spawn();
            if spawned.is_err() {
                return ExitCode::from(4);
            }
            let mut out = std::io::stdout();
            write_stdout(&mut out, "exiting\n".as_bytes());
            ExitCode::from(7)
        }
        "grandchild" => {
            if let Some(path) = &args.terminal_heartbeat {
                let exe = std::env::current_exe().unwrap_or_default();
                let _ = std::process::Command::new(exe)
                    .args(["--scenario", "heartbeat-child", "--heartbeat-file", path])
                    .spawn();
            }
            let mut out = std::io::stdout();
            write_stdout(&mut out, "started\n".as_bytes());
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        _ => ExitCode::from(2),
    }
}

fn write_stdout(out: &mut impl Write, bytes: &[u8]) {
    let _ = out.write_all(bytes);
    let _ = out.flush();
}

/// 探针结果追加一行（测试在进程外读取；失败不影响协议流程）。
fn probe(args: &Args, line: &str) {
    if let Some(path) = &args.probe_file {
        append_line(path, line);
    }
}

/// 收到一条带 `method` 的入站报文时，按**各自给定的路径**记录到两个 dump 选项。
///
/// 两个选项完全独立：只给出其中一个时，另一个不产生任何文件。
fn dump_inbound(args: &Args, method: &str, params: Option<&Value>) {
    dump_method(args, method);
    dump_request_params(args, method, params);
}

/// `--dump-requests <path>`：追加一行 method（正常错误都静默：本选项只服务于测试断言）。
fn dump_method(args: &Args, method: &str) {
    if let Some(path) = &args.dump_requests {
        append_line(path, method);
    }
}

/// `--dump-request-params <path>`：追加一行单行 JSON `{"method":…,"params":…}`。
///
/// `params` 缺失（通知没有参数、或对端省略）时写 `null`，因此每行的键集合恒为 `{method, params}`。
fn dump_request_params(args: &Args, method: &str, params: Option<&Value>) {
    let Some(path) = &args.dump_request_params else {
        return;
    };
    let line = json!({
        "method": method,
        "params": params.cloned().unwrap_or(Value::Null),
    });
    append_line(path, &line.to_string());
}

/// 以「追加一行」的方式写 `path`（正常错误都静默：本选项只服务于测试断言）。
fn append_line(path: &str, line: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
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
            if args.scenario == "initialize-error" {
                // 协商明确失败：进程从未可服务会话，因此「连接/断开」都不该被上报。
                write_line(
                    out,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32000, "message": "无法协商" },
                    }),
                );
                return;
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
            // 形状校验：`NewSessionRequest` 的 `cwd` 与 `mcpServers` 都是必填。形状不合规即退出，
            // 让「发出缺必填字段的请求」变成测试里可观察的失败。
            let params = message.get("params");
            let shaped = params
                .and_then(|params| params.get("cwd"))
                .and_then(Value::as_str)
                .is_some()
                && params
                    .and_then(|params| params.get("mcpServers"))
                    .and_then(Value::as_array)
                    .is_some();
            if !shaped {
                std::process::exit(8);
            }
            if args.scenario == "session-new-error" {
                // 会话创建失败（Agent 返回明确错误）：不得伪造一个会话标识。
                write_line(
                    out,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32002, "message": "无法创建会话" },
                    }),
                );
                return;
            }
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
            if args.scenario == "exit-after-new" {
                // 让 session/new 的响应先被读侧处理，再退出：之后任何写入都必须失败
                // （用于验证写确认失败路径）。
                std::thread::sleep(std::time::Duration::from_millis(200));
                std::process::exit(0);
            }
        }
        (Some("session/set_mode"), Some(id)) => {
            // 形状校验：`{ sessionId, modeId }`（pinned schema 的两个必填字段）。
            let shaped = message
                .get("params")
                .and_then(|params| params.get("sessionId"))
                .and_then(Value::as_str)
                .is_some()
                && message
                    .get("params")
                    .and_then(|params| params.get("modeId"))
                    .and_then(Value::as_str)
                    .is_some();
            if !shaped {
                std::process::exit(8);
            }
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
            // 形状校验：`value` 的 anyOf —— 布尔必须带 `type: boolean`，其余只能是 `value-id`。
            let value = message.get("params").and_then(|params| params.get("value"));
            let shaped = match value {
                Some(Value::Object(object)) => {
                    let boolean_ok = object.get("type").and_then(Value::as_str) == Some("boolean")
                        && object.get("value").and_then(Value::as_bool).is_some();
                    let id_ok = object.get("value").and_then(Value::as_str).is_some();
                    boolean_ok || id_ok
                }
                _ => false,
            };
            if !shaped {
                std::process::exit(8);
            }
            if args.exit_on_config_write {
                std::process::exit(7);
            }
            respond(out, &id, json!({}));
        }
        (Some("session/resume"), Some(id)) => {
            // 形状校验：`ResumeSessionRequest` 的 `sessionId` 与 `cwd` 都是 pinned schema 的必填字段。
            // 形状不合规即退出，让「发出缺必填字段的恢复请求」变成测试里可观察的失败。
            let params = message.get("params");
            let shaped = params
                .and_then(|params| params.get("sessionId"))
                .and_then(Value::as_str)
                .is_some()
                && params
                    .and_then(|params| params.get("cwd"))
                    .and_then(Value::as_str)
                    .is_some();
            if !shaped {
                std::process::exit(8);
            }
            match args.scenario.as_str() {
                // 已宣告能力、Agent 却拒绝恢复（例如原生会话已被清理）：明确失败，不伪造成功。
                "resume-error" => write_line(
                    out,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32001, "message": "原生会话已被清理" },
                    }),
                ),
                // `resume-ok` 与本 fake 的其它场景都是恢复成功：模式与配置项都缺失是合法响应。
                _ => respond(out, &id, json!({})),
            }
        }
        (Some("session/close"), Some(id)) => {
            // 形状校验：`CloseSessionRequest` 的唯一必填字段是 `sessionId`。
            let shaped = message
                .get("params")
                .and_then(|params| params.get("sessionId"))
                .and_then(Value::as_str)
                .is_some();
            if !shaped {
                std::process::exit(8);
            }
            match args.scenario.as_str() {
                // Agent 明确拒绝关闭（资源必须保留）。`close-error-terminal` 是同一拒绝路径，但随后的
                // prompt 会走终端流程：用于断言「关闭失败后绑定仍能服务 terminal/*」。
                "close-error" | "close-error-terminal" => write_line(
                    out,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32003, "message": "无法关闭会话" },
                    }),
                ),
                // 永不响应：用于断言有界超时与「结果不确定」时仍保留资源。
                "close-timeout" => {}
                // 慢 ack：用于在关闭进行中重绑，验证迟到旧代结果不得摘除新绑定。
                "slow-close" => {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    respond(out, &id, json!({}));
                }
                _ => respond(out, &id, json!({})),
            }
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
            if handle_terminal_response(message, state, out, args) {
                return;
            }
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
    if let Some(kind) = terminal_kind_for(args.scenario.as_str()) {
        start_terminal_flow(state, out, args, kind, id, session);
        return;
    }
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
            send_permission_request(state, out, session, id);
        }
        "permission-request-then-exit" => {
            // 请求写出后立刻退出：之后对权限的应答必然无法写进 stdin（用于验证确认失败路径）。
            send_permission_request(state, out, session, id);
            std::process::exit(0);
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
        "stderr-protocol-noise" => {
            // stderr 上写**语法完全合法**的 ACP 报文（带唯一 marker 的通知 + 一个带 id 的响应）：
            // 它们不得被当作协议输入，也不得进入事件或端点；stdout 仍正常应答。
            let mut err = std::io::stderr();
            let _ = writeln!(
                err,
                "{}",
                json!({
                    "jsonrpc": "2.0",
                    "method": "session/update",
                    "params": {
                        "sessionId": session,
                        "update": {
                            "sessionUpdate": "agent_message_chunk",
                            "messageId": "stderr-noise",
                            "content": { "type": "text", "text": STDERR_NOISE_MARKER },
                        },
                    },
                })
            );
            let _ = writeln!(
                err,
                "{}",
                json!({
                    "jsonrpc": "2.0",
                    "id": 987654321u64,
                    "result": { "stopReason": "end_turn" },
                })
            );
            let _ = err.flush();
            emit_chunk(out, session, "stderr 噪声之后仍然可用");
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

/// 发出一条权限请求并把对应的 prompt 登记为待交互。
fn send_permission_request(state: &mut State, out: &mut impl Write, session: &str, prompt: Value) {
    let request_id = state.next_id();
    state.interactions.push((request_id, "permission", prompt));
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

fn terminal_kind_for(scenario: &str) -> Option<TerminalFlowKind> {
    Some(match scenario {
        "terminal-ok" => TerminalFlowKind::Happy,
        // 关闭被拒之后仍能建终端/读输出：断言释放失败不得把终端服务永久关死。
        "close-error-terminal" => TerminalFlowKind::Happy,
        "terminal-exit-code" => TerminalFlowKind::NonZero,
        "terminal-big" => TerminalFlowKind::Big,
        "terminal-kill" => TerminalFlowKind::Kill,
        "terminal-orphan" => TerminalFlowKind::Orphan,
        "terminal-orphan-crash" => TerminalFlowKind::OrphanCrash,
        "terminal-cross-session" => TerminalFlowKind::CrossSession,
        "terminal-not-advertised" => TerminalFlowKind::NotAdvertised,
        "terminal-wait-hang" => TerminalFlowKind::WaitHang,
        "terminal-frame-budget" => TerminalFlowKind::FrameBudget,
        "terminal-frame-escapes" => TerminalFlowKind::FrameEscapes,
        _ => return None,
    })
}

fn start_terminal_flow(
    state: &mut State,
    out: &mut impl Write,
    args: &Args,
    kind: TerminalFlowKind,
    prompt: Value,
    session: &str,
) {
    let exe = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (mode, extra) = match kind {
        TerminalFlowKind::Happy
        | TerminalFlowKind::CrossSession
        | TerminalFlowKind::NotAdvertised => ("echo-utf8", Vec::new()),
        TerminalFlowKind::NonZero => ("exit-code", Vec::new()),
        TerminalFlowKind::Big => (
            "big",
            vec!["--terminal-bytes".to_owned(), "8192".to_owned()],
        ),
        TerminalFlowKind::FrameBudget => (
            "big",
            vec!["--terminal-bytes".to_owned(), "1048576".to_owned()],
        ),
        TerminalFlowKind::FrameEscapes => (
            "escaped",
            vec!["--terminal-bytes".to_owned(), "120000".to_owned()],
        ),
        TerminalFlowKind::Kill => ("hang", Vec::new()),
        TerminalFlowKind::WaitHang => {
            let mut extra = Vec::new();
            if let Some(path) = &args.terminal_heartbeat {
                extra.push("--terminal-heartbeat".to_owned());
                extra.push(path.clone());
            }
            ("grandchild", extra)
        }
        TerminalFlowKind::Orphan | TerminalFlowKind::OrphanCrash => {
            let mut extra = Vec::new();
            if let Some(path) = &args.terminal_heartbeat {
                extra.push("--terminal-heartbeat".to_owned());
                extra.push(path.clone());
            }
            ("grandchild", extra)
        }
    };
    let mut child_args = vec!["--terminal-child".to_owned(), mode.to_owned()];
    child_args.extend(extra);
    let mut params = json!({ "sessionId": session, "command": exe, "args": child_args });
    if kind == TerminalFlowKind::Big {
        params["outputByteLimit"] = json!(1024);
    }
    let mut flow = TerminalFlow {
        kind,
        session: session.to_owned(),
        prompt,
        request_id: 0,
        step: 0,
        terminal_id: None,
        exit_code: None,
        output_polls: 0,
    };
    send_terminal_request(state, out, &mut flow, "terminal/create", params);
    state.terminal_flow = Some(flow);
}

fn send_terminal_request(
    state: &mut State,
    out: &mut impl Write,
    flow: &mut TerminalFlow,
    method: &str,
    params: Value,
) {
    let id = state.next_id();
    flow.request_id = id;
    write_line(
        out,
        &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
    );
}

fn terminal_ref_params(session: &str, terminal_id: &str) -> Value {
    json!({ "sessionId": session, "terminalId": terminal_id })
}

fn emit_terminal_tool(out: &mut impl Write, session: &str, terminal_id: &str) {
    notify(
        out,
        "session/update",
        json!({
            "sessionId": session,
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "terminal-tool-1",
                "title": "运行命令",
                "kind": "execute",
                "status": "in_progress",
                "content": [ { "type": "terminal", "terminalId": terminal_id } ],
            },
        }),
    );
}

fn finish_terminal_flow(out: &mut impl Write, flow: &TerminalFlow) {
    respond(out, &flow.prompt, json!({ "stopReason": "end_turn" }));
}

/// 校验失败：发一条带 `PROBE_FAILED` 的普通增量（测试断言它不出现）并以 `refusal` 结清 prompt。
fn fail_terminal_flow(out: &mut impl Write, args: &Args, flow: &TerminalFlow, reason: &str) {
    probe(args, &format!("failed: {reason}"));
    emit_chunk(out, &flow.session, &format!("PROBE_FAILED:{reason}"));
    respond(out, &flow.prompt, json!({ "stopReason": "refusal" }));
}

fn response_error(message: &Value) -> bool {
    message.get("error").is_some_and(|value| !value.is_null())
}

/// 处理一条发给 fake Agent 的响应；返回 `true` 表示它属于终端探针流程。
fn handle_terminal_response(
    message: &Value,
    state: &mut State,
    out: &mut impl Write,
    args: &Args,
) -> bool {
    let Some(expected) = state.terminal_flow.as_ref().map(|flow| flow.request_id) else {
        return false;
    };
    if message.get("id").and_then(Value::as_u64) != Some(expected) {
        return false;
    }
    let Some(mut flow) = state.terminal_flow.take() else {
        return false;
    };
    let failed = response_error(message);
    let result = message.get("result").cloned().unwrap_or(Value::Null);
    let session = flow.session.clone();
    let mut keep = true;

    match flow.kind {
        TerminalFlowKind::NotAdvertised => {
            if failed {
                probe(args, "create_rejected");
                finish_terminal_flow(out, &flow);
            } else {
                fail_terminal_flow(out, args, &flow, "未宣告能力却创建成功");
            }
            keep = false;
        }
        TerminalFlowKind::Happy
        | TerminalFlowKind::NonZero
        | TerminalFlowKind::Big
        | TerminalFlowKind::FrameBudget
        | TerminalFlowKind::FrameEscapes => {
            match flow.step {
                0 => match result.get("terminalId").and_then(Value::as_str) {
                    Some(terminal_id) => {
                        flow.terminal_id = Some(terminal_id.to_owned());
                        emit_terminal_tool(out, &session, terminal_id);
                        flow.step = 1;
                        send_terminal_request(
                            state,
                            out,
                            &mut flow,
                            "terminal/output",
                            terminal_ref_params(&session, terminal_id),
                        );
                    }
                    None => {
                        fail_terminal_flow(out, args, &flow, "create 响应缺少 terminalId");
                        keep = false;
                    }
                },
                1 => {
                    let output = result.get("output").and_then(Value::as_str).unwrap_or("");
                    let truncated = result
                        .get("truncated")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let ok = match flow.kind {
                        TerminalFlowKind::Happy => {
                            // stdout 与 stderr 按实际接收次序合并：stderr 可能夹在两个 stdout 分块之间，
                            // 因此只要求两段 stdout 都在、emoji 没被拆坏。
                            output.contains("第一段🙂") && output.contains("第二段") && !truncated
                        }
                        TerminalFlowKind::NonZero => output.contains("boom") && !truncated,
                        TerminalFlowKind::Big => {
                            truncated
                                && output.chars().count() <= 1024
                                && output.chars().all(|ch| ch == 'x' || ch == '\n')
                        }
                        TerminalFlowKind::FrameBudget | TerminalFlowKind::FrameEscapes => {
                            // 线上帧必须留在 ACP 单条消息上限内；正文仍保留绝大部分（转义模式
                            // 因每个换行/ESC 变长而保留得少一些），且必须如实上报 truncated。
                            let wire = serde_json::to_string(message)
                                .map(|text| text.len())
                                .unwrap_or(usize::MAX);
                            let retained = if flow.kind == TerminalFlowKind::FrameEscapes {
                                500_000
                            } else {
                                1_000_000
                            };
                            let tail_ok = if flow.kind == TerminalFlowKind::FrameEscapes {
                                output.ends_with("X\n")
                            } else {
                                output.ends_with('\n')
                            };
                            truncated
                                && wire <= MAX_MESSAGE_BYTES
                                && output.len() >= retained
                                && tail_ok
                        }
                        _ => false,
                    };
                    if !ok && flow.output_polls < 200 {
                        // 真实 ACP 语义：output 读到的是「到目前为止」的内容，可能还没写出。
                        flow.output_polls += 1;
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                        send_terminal_request(
                            state,
                            out,
                            &mut flow,
                            "terminal/output",
                            terminal_ref_params(&session, &terminal_id),
                        );
                    } else if !ok {
                        fail_terminal_flow(out, args, &flow, "输出内容/顺序/预算不符");
                        keep = false;
                    } else {
                        if matches!(
                            flow.kind,
                            TerminalFlowKind::FrameBudget | TerminalFlowKind::FrameEscapes
                        ) {
                            probe(args, "frame_budget_ok");
                        }
                        let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                        flow.step = 2;
                        send_terminal_request(
                            state,
                            out,
                            &mut flow,
                            "terminal/wait_for_exit",
                            terminal_ref_params(&session, &terminal_id),
                        );
                    }
                }
                2 => {
                    let expected = match flow.kind {
                        TerminalFlowKind::Happy
                        | TerminalFlowKind::Big
                        | TerminalFlowKind::FrameBudget
                        | TerminalFlowKind::FrameEscapes => Some(0),
                        TerminalFlowKind::NonZero => Some(3),
                        _ => None,
                    };
                    if result.get("exitCode").and_then(Value::as_u64) != expected {
                        fail_terminal_flow(out, args, &flow, "退出码不符");
                        keep = false;
                    } else {
                        let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                        flow.step = 3;
                        send_terminal_request(
                            state,
                            out,
                            &mut flow,
                            "terminal/release",
                            terminal_ref_params(&session, &terminal_id),
                        );
                    }
                }
                3 => {
                    if failed {
                        fail_terminal_flow(out, args, &flow, "release 被拒绝");
                        keep = false;
                    } else {
                        let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                        flow.step = 4;
                        send_terminal_request(
                            state,
                            out,
                            &mut flow,
                            "terminal/output",
                            terminal_ref_params(&session, &terminal_id),
                        );
                    }
                }
                4 => {
                    if failed {
                        probe(args, "output_after_release_rejected");
                        let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                        flow.step = 5;
                        send_terminal_request(
                            state,
                            out,
                            &mut flow,
                            "terminal/release",
                            terminal_ref_params(&session, &terminal_id),
                        );
                    } else {
                        fail_terminal_flow(out, args, &flow, "release 后仍可读取输出");
                        keep = false;
                    }
                }
                5 => {
                    if failed {
                        probe(args, "release_after_release_rejected");
                        finish_terminal_flow(out, &flow);
                    } else {
                        fail_terminal_flow(out, args, &flow, "release 可以重复成功");
                    }
                    keep = false;
                }
                _ => {
                    fail_terminal_flow(out, args, &flow, "流程步数越界");
                    keep = false;
                }
            }
        }
        TerminalFlowKind::Kill => match flow.step {
            0 => match result.get("terminalId").and_then(Value::as_str) {
                Some(terminal_id) => {
                    flow.terminal_id = Some(terminal_id.to_owned());
                    emit_terminal_tool(out, &session, terminal_id);
                    flow.step = 1;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/output",
                        terminal_ref_params(&session, terminal_id),
                    );
                }
                None => {
                    fail_terminal_flow(out, args, &flow, "create 响应缺少 terminalId");
                    keep = false;
                }
            },
            1 => {
                if result.get("output").and_then(Value::as_str) != Some("started\n") {
                    if flow.output_polls < 200 {
                        flow.output_polls += 1;
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                        send_terminal_request(
                            state,
                            out,
                            &mut flow,
                            "terminal/output",
                            terminal_ref_params(&session, &terminal_id),
                        );
                    } else {
                        fail_terminal_flow(out, args, &flow, "kill 前的输出不符");
                        keep = false;
                    }
                } else {
                    let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                    flow.step = 2;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/kill",
                        terminal_ref_params(&session, &terminal_id),
                    );
                }
            }
            2 => {
                if failed {
                    fail_terminal_flow(out, args, &flow, "kill 被拒绝");
                    keep = false;
                } else {
                    probe(args, "kill_ok");
                    let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                    flow.step = 3;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/wait_for_exit",
                        terminal_ref_params(&session, &terminal_id),
                    );
                }
            }
            3 => {
                let code = result.get("exitCode").and_then(Value::as_u64);
                let signal = result.get("signal").and_then(Value::as_str);
                // Unix 被 SIGKILL 时只有 signal（code 为 null），Windows 只有 exitCode；
                // 两个都有或都没有都说明退出事实不可判定。
                if failed || code.is_some() == signal.is_some() {
                    fail_terminal_flow(out, args, &flow, "kill 后没有可判定的退出事实");
                    keep = false;
                } else {
                    flow.exit_code = code;
                    probe(args, "kill_exit_reported");
                    let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                    flow.step = 4;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/kill",
                        terminal_ref_params(&session, &terminal_id),
                    );
                }
            }
            4 => {
                if failed {
                    fail_terminal_flow(out, args, &flow, "重复 kill 被拒绝");
                    keep = false;
                } else {
                    probe(args, "kill_twice_ok");
                    let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                    flow.step = 5;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/wait_for_exit",
                        terminal_ref_params(&session, &terminal_id),
                    );
                }
            }
            5 => {
                if result.get("exitCode").and_then(Value::as_u64) != flow.exit_code {
                    fail_terminal_flow(out, args, &flow, "重复等待的退出码不一致");
                    keep = false;
                } else {
                    probe(args, "wait_twice_stable");
                    let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                    flow.step = 6;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/output",
                        terminal_ref_params(&session, &terminal_id),
                    );
                }
            }
            6 => {
                let readable = result.get("output").and_then(Value::as_str) == Some("started\n")
                    && result
                        .get("exitStatus")
                        .is_some_and(|value| !value.is_null());
                if !readable {
                    fail_terminal_flow(out, args, &flow, "kill 后输出不可读或没有退出状态");
                    keep = false;
                } else {
                    let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                    flow.step = 7;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/release",
                        terminal_ref_params(&session, &terminal_id),
                    );
                }
            }
            7 => {
                if failed {
                    fail_terminal_flow(out, args, &flow, "release 被拒绝");
                    keep = false;
                } else {
                    let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                    flow.step = 8;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/output",
                        terminal_ref_params(&session, &terminal_id),
                    );
                }
            }
            8 => {
                if failed {
                    probe(args, "output_after_release_rejected");
                    let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                    flow.step = 9;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/release",
                        terminal_ref_params(&session, &terminal_id),
                    );
                } else {
                    fail_terminal_flow(out, args, &flow, "release 后仍可读取输出");
                    keep = false;
                }
            }
            9 => {
                if failed {
                    probe(args, "release_after_release_rejected");
                    finish_terminal_flow(out, &flow);
                } else {
                    fail_terminal_flow(out, args, &flow, "release 可以重复成功");
                }
                keep = false;
            }
            _ => {
                fail_terminal_flow(out, args, &flow, "流程步数越界");
                keep = false;
            }
        },
        TerminalFlowKind::CrossSession => match flow.step {
            0 => match result.get("terminalId").and_then(Value::as_str) {
                Some(terminal_id) => {
                    flow.terminal_id = Some(terminal_id.to_owned());
                    emit_terminal_tool(out, &session, terminal_id);
                    flow.step = 1;
                    let other = if session == "acp-session-1" {
                        "acp-session-2"
                    } else {
                        "acp-session-1"
                    };
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/output",
                        terminal_ref_params(other, terminal_id),
                    );
                }
                None => {
                    fail_terminal_flow(out, args, &flow, "create 响应缺少 terminalId");
                    keep = false;
                }
            },
            1 => {
                if failed {
                    probe(args, "cross_session_rejected");
                    let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                    flow.step = 2;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/output",
                        terminal_ref_params(&session, &terminal_id),
                    );
                } else {
                    fail_terminal_flow(out, args, &flow, "跨会话访问没有被拒绝");
                    keep = false;
                }
            }
            2 => {
                if failed {
                    fail_terminal_flow(out, args, &flow, "本会话输出被拒绝");
                    keep = false;
                } else {
                    let terminal_id = flow.terminal_id.clone().unwrap_or_default();
                    flow.step = 3;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/release",
                        terminal_ref_params(&session, &terminal_id),
                    );
                }
            }
            3 => {
                if failed {
                    fail_terminal_flow(out, args, &flow, "release 被拒绝");
                } else {
                    finish_terminal_flow(out, &flow);
                }
                keep = false;
            }
            _ => {
                fail_terminal_flow(out, args, &flow, "流程步数越界");
                keep = false;
            }
        },
        TerminalFlowKind::Orphan | TerminalFlowKind::OrphanCrash => match flow.step {
            0 => match result.get("terminalId").and_then(Value::as_str) {
                Some(terminal_id) => {
                    flow.terminal_id = Some(terminal_id.to_owned());
                    emit_terminal_tool(out, &session, terminal_id);
                    probe(args, "orphan_created");
                    finish_terminal_flow(out, &flow);
                    keep = false;
                    if flow.kind == TerminalFlowKind::OrphanCrash {
                        // 等到孙进程真的开始写心跳，再崩溃：这样「Agent 退出后进程树被清理」是可观察的。
                        if let Some(path) = &args.terminal_heartbeat {
                            let deadline =
                                std::time::Instant::now() + std::time::Duration::from_secs(3);
                            while !std::path::Path::new(path).exists()
                                && std::time::Instant::now() < deadline
                            {
                                std::thread::sleep(std::time::Duration::from_millis(20));
                            }
                        }
                        let _ = out.flush();
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        std::process::exit(0);
                    }
                }
                None => {
                    fail_terminal_flow(out, args, &flow, "create 响应缺少 terminalId");
                    keep = false;
                }
            },
            _ => {
                fail_terminal_flow(out, args, &flow, "流程步数越界");
                keep = false;
            }
        },
        TerminalFlowKind::WaitHang => match flow.step {
            0 => match result.get("terminalId").and_then(Value::as_str) {
                Some(terminal_id) => {
                    flow.terminal_id = Some(terminal_id.to_owned());
                    emit_terminal_tool(out, &session, terminal_id);
                    probe(args, "wait_pending");
                    flow.step = 1;
                    send_terminal_request(
                        state,
                        out,
                        &mut flow,
                        "terminal/wait_for_exit",
                        terminal_ref_params(&session, terminal_id),
                    );
                }
                None => {
                    fail_terminal_flow(out, args, &flow, "create 响应缺少 terminalId");
                    keep = false;
                }
            },
            1 => {
                if failed {
                    probe(args, "wait_cancelled");
                    finish_terminal_flow(out, &flow);
                } else {
                    fail_terminal_flow(out, args, &flow, "等待退出没有被取消");
                }
                keep = false;
            }
            _ => {
                fail_terminal_flow(out, args, &flow, "流程步数越界");
                keep = false;
            }
        },
    }

    if keep {
        state.terminal_flow = Some(flow);
    }
    true
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

#[cfg(test)]
mod tests {
    use super::{Args, Value, dump_inbound, json};

    /// 用例独占的临时文件路径：`Drop` 时尽力删除（正常结束与 panic 展开两条路径都生效）。
    struct TempPath(std::path::PathBuf);

    impl TempPath {
        fn new(name: &str) -> Self {
            Self(
                std::env::temp_dir()
                    .join(format!("acpr-fake-params-{}-{name}", std::process::id())),
            )
        }
    }

    impl std::ops::Deref for TempPath {
        type Target = std::path::Path;

        fn deref(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempPath {
        fn drop(&mut self) {
            // 尽力而为，不 panic：文件可能从未被创建。
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// 按 argv 构造参数（程序名已在 `from_argv` 里被剔除）。
    fn args_from(argv: &[&str]) -> Args {
        Args::from_argv(
            &argv
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect::<Vec<String>>(),
        )
    }

    /// 文件的全部行（文件不存在时返回空序列）。
    fn lines(path: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// `session/resume` 形状的一行请求（用例断言的正是它发出去的取值）。
    fn resume_request(session_id: &str, cwd: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "session/resume",
            "params": { "sessionId": session_id, "cwd": cwd },
        })
    }

    /// 新选项：每行是单行合法 JSON，同时含 `method` 与 `params`，取值逐字保真。
    #[test]
    fn dump_request_params_keeps_method_and_params_per_line() {
        let path = TempPath::new("both-fields.jsonl");
        let cwd = "/持久化的/cwd";
        let args = args_from(&["--dump-request-params", &path.to_string_lossy()]);

        dump_inbound(
            &args,
            "session/resume",
            resume_request("acp-session-1", cwd).get("params"),
        );
        dump_inbound(&args, "session/cancel", None);

        let written = lines(&path);
        assert_eq!(written.len(), 2, "每条请求一行：{written:?}");
        for line in &written {
            // 单行、合法 JSON、键集合恰为 {method, params}。
            let value: Value = serde_json::from_str(line).expect("每行都是合法 JSON");
            let object = value.as_object().expect("每行都是 JSON 对象");
            let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, ["method", "params"]);
            assert!(object["method"].is_string());
        }
        // 发出去的取值逐字可读（`cwd` 是 R26 的线级证据，`sessionId` 是 R8 的）。
        let first: Value = serde_json::from_str(&written[0]).expect("首行");
        assert_eq!(first["method"], json!("session/resume"));
        assert_eq!(first["params"]["sessionId"], json!("acp-session-1"));
        assert_eq!(first["params"]["cwd"], json!(cwd));
        // 缺失的 `params` 写成 `null`，而不是让键消失或整行缺字段。
        let second: Value = serde_json::from_str(&written[1]).expect("次行");
        assert_eq!(second["method"], json!("session/cancel"));
        assert_eq!(second["params"], Value::Null);
    }

    /// 两个选项可并存：各自写各自的文件，`--dump-requests` 仍每行只写 method。
    #[test]
    fn both_dump_options_coexist_in_independent_files() {
        let methods = TempPath::new("methods.txt");
        let params = TempPath::new("params.jsonl");
        let args = args_from(&[
            "--dump-requests",
            &methods.to_string_lossy(),
            "--dump-request-params",
            &params.to_string_lossy(),
        ]);

        dump_inbound(
            &args,
            "session/resume",
            resume_request("acp-session-2", "/tmp/x").get("params"),
        );
        dump_inbound(&args, "session/update", None);

        // 冻结语义未被改写：method 文件里没有 JSON、没有 params。
        assert_eq!(lines(&methods), vec!["session/resume", "session/update"]);
        let params_lines = lines(&params);
        assert_eq!(params_lines.len(), 2);
        let first: Value = serde_json::from_str(&params_lines[0]).expect("首行是 JSON");
        assert_eq!(first["params"]["cwd"], json!("/tmp/x"));
    }

    /// 只给一个选项时，另一个文件根本不出现。
    #[test]
    fn a_single_option_creates_only_its_own_file() {
        let methods = TempPath::new("only-methods.txt");
        let params = TempPath::new("never-written.jsonl");
        let args = args_from(&["--dump-requests", &methods.to_string_lossy()]);

        dump_inbound(&args, "session/resume", Some(&json!({ "cwd": "/tmp/y" })));

        assert_eq!(lines(&methods), vec!["session/resume"]);
        assert!(!params.exists(), "未给出的选项不得创建文件");
    }

    /// 两个选项都不给：没有任何副作用（不创建文件、不 panic）。
    #[test]
    fn without_options_nothing_is_written() {
        let methods = TempPath::new("no-methods.txt");
        let params = TempPath::new("no-params.jsonl");
        let args = args_from(&["--scenario", "resume-ok"]);

        dump_inbound(
            &args,
            "session/resume",
            resume_request("acp-session-3", "/tmp/z").get("params"),
        );

        assert!(args.dump_requests.is_none());
        assert!(args.dump_request_params.is_none());
        assert!(!methods.exists(), "未给出的选项不得创建文件");
        assert!(!params.exists(), "未给出的选项不得创建文件");
    }

    /// 选项解析：路径后面的开关不被吞掉，两选项都能独立给出。
    #[test]
    fn option_parsing_keeps_following_switches() {
        let path = TempPath::new("parsed.jsonl");
        let args = args_from(&[
            "--scenario",
            "resume-ok",
            "--dump-request-params",
            &path.to_string_lossy(),
            "--no-modes",
        ]);

        assert_eq!(args.scenario, "resume-ok");
        assert!(args.no_modes, "路径之后的开关必须仍被识别");
        assert_eq!(
            args.dump_request_params.as_deref(),
            Some(&*path.to_string_lossy())
        );
        assert!(args.dump_requests.is_none());
    }
}
