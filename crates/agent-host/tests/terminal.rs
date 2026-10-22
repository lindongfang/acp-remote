//! ACP 终端服务（`terminal/*`）的真实子进程测试。
//!
//! 这里**没有**终端替身：fake ACP Agent 通过 stdio 发出真实的 `terminal/create`/`output`/`wait_for_exit`/
//! `kill`/`release`，Daemon 侧由 `agent-host` 启动真实子进程（本测试可执行文件同一包里的 fake 二进制，
//! 以 `--terminal-child` 模式运行），输出/退出/进程树因此都是可观察的真实行为。
//!
//! 断言分三层：
//!
//! 1. **线级**：fake Agent 对每个响应的校验结果写进 `--probe-file`（成功/失败各一行），测试再读它；
//!    任何不符都会同时产生一条 `PROBE_FAILED:` 文本增量，测试断言它不出现。
//! 2. **事件**：`terminal.output` 的 `terminalId`/`chunkIndex`/`stream`/`text` 与 `tool.call.started`
//!    的 ACP 原文（terminal 引用）逐项核对。
//! 3. **进程外**：孙进程写心跳文件；会话关闭/Agent 崩溃/Daemon 关闭后心跳必须停止增长
//!    （Windows Job Object、Unix 进程组的可复现探针）。

mod support;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use acp_core::model::{
    AgentId, AgentRef, CreateSessionRequest, PromptContentBlock, PromptRequest, ResourceOrigin,
    SessionId,
};
use acp_core::ports::{SessionBackendFactory, SessionEndpoint};
use acp_protocol::capability::ClientCapabilities;
use agent_host::{AgentHost, HostConfig, runtime_running};
use serde_json::Value;
use support::{
    Collector, FAKE_AGENT, TempFile, TestClock, TestIds, params_of, profile_with,
    wait_for_dumped_method,
};

const AGENT: &str = "agent-1";
const SESSION_A: &str = "11111111-1111-4111-8111-111111111111";
const SESSION_B: &str = "22222222-2222-4222-8222-222222222222";

fn terminal_capabilities() -> ClientCapabilities {
    ClientCapabilities {
        terminal: Some(true),
        ..ClientCapabilities::none()
    }
}

fn host_with(config: HostConfig, args: &[&str]) -> Arc<AgentHost> {
    let profile = profile_with(AGENT, FAKE_AGENT, args);
    Arc::new(AgentHost::new(
        Arc::new(vec![profile]),
        config,
        Arc::new(TestIds::new()),
        Arc::new(TestClock::new()),
    ))
}

fn agent_ref() -> AgentRef {
    AgentRef::try_new(AgentId::new(AGENT).expect("id"), "Agent").expect("ref")
}

fn session(text: &str) -> SessionId {
    SessionId::new(text).expect("session id")
}

async fn create(
    host: &Arc<AgentHost>,
    id: &SessionId,
    collector: &Collector,
) -> Box<dyn SessionEndpoint> {
    host.create(
        id,
        CreateSessionRequest::new(
            agent_ref(),
            Some(support::workspace()),
            None,
            ResourceOrigin::Local,
        ),
        collector.sink(),
    )
    .await
    .expect("create")
}

fn prompt(text: &str) -> PromptRequest {
    PromptRequest::new(vec![
        PromptContentBlock::from_json_text(&format!("{{\"type\":\"text\",\"text\":\"{text}\"}}"))
            .expect("block"),
    ])
}

fn temp_path(tag: &str) -> TempFile {
    TempFile::new(&format!("acpr-terminal-{}-{tag}", std::process::id()))
}

fn read_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

async fn wait_for_probe(path: &Path, needle: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if read_lines(path).iter().any(|line| line == needle) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    read_lines(path).iter().any(|line| line == needle)
}

fn assert_probe(path: &Path, needle: &str) {
    let lines = read_lines(path);
    assert!(
        lines.iter().any(|line| line == needle),
        "探针里必须出现 {needle}：{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("failed:")),
        "fake Agent 的线级校验失败：{lines:?}"
    );
}

fn heartbeat_size(path: &Path) -> u64 {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

async fn wait_for_heartbeat(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if heartbeat_size(path) > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "终端孙进程必须在 {timeout:?} 内开始写心跳（{}）",
        path.display()
    );
}

/// 断言心跳文件在超时内**停止增长**（连续 400 ms 大小不变）。
async fn assert_heartbeat_stops(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    let mut last = heartbeat_size(path);
    let mut stable_since = Instant::now();
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(60)).await;
        let current = heartbeat_size(path);
        if current != last {
            last = current;
            stable_since = Instant::now();
        } else if stable_since.elapsed() >= Duration::from_millis(400) {
            return;
        }
    }
    panic!(
        "进程树没有被清理：心跳仍在增长（{} 字节，{}）",
        heartbeat_size(path),
        path.display()
    );
}

fn views(collector: &Collector, event_type: &str) -> Vec<Value> {
    collector
        .snapshot()
        .into_iter()
        .filter(|event| event.event_type.as_str() == event_type)
        .map(|event| serde_json::from_str(event.payload.view.as_str()).expect("view 是 JSON 对象"))
        .collect()
}

fn assert_no_probe_failure(collector: &Collector) {
    for view in views(collector, "agent.message.delta") {
        let text = view.get("text").and_then(Value::as_str).unwrap_or_default();
        assert!(
            !text.contains("PROBE_FAILED"),
            "fake Agent 的线级校验失败：{text}"
        );
    }
    assert!(
        views(collector, "turn.failed").is_empty(),
        "终端流程不应产生 turn.failed：{:?}",
        views(collector, "turn.failed")
    );
}

/// 终端事件按 `chunkIndex` 递增，且 stdout/stderr 的来源可区分。
fn terminal_streams(collector: &Collector) -> (String, String) {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut last: Option<u64> = None;
    for view in views(collector, "terminal.output") {
        let index: u64 = view["chunkIndex"]
            .as_str()
            .expect("chunkIndex")
            .parse()
            .expect("chunkIndex 是十进制字符串");
        if let Some(previous) = last {
            assert_eq!(index, previous + 1, "chunkIndex 必须逐块 +1：{view}");
        }
        last = Some(index);
        let text = view["text"].as_str().expect("text");
        assert_eq!(view["truncated"], Value::Bool(false), "事件本身不截断");
        match view["stream"].as_str().expect("stream") {
            "stdout" => stdout.push_str(text),
            "stderr" => stderr.push_str(text),
            other => panic!("未登记的 stream：{other}"),
        }
    }
    (stdout, stderr)
}

/// 终端场景的 fake Agent 参数。
///
/// 这些测试在结尾都会走真实的 `Endpoint::close`（组合层的释放顺序：停终端 → 能力门控的
/// `session/close` → ack 后摘除映射）。因此 fake Agent **必须**在 `initialize` 里宣告
/// `sessionCapabilities.close`：未宣告时关闭会被按「后端不支持」明确拒绝（见
/// `crates/agent-host/src/host.rs` 的 `release_bound_session`），这不是终端流程要断言的语义。
const CLOSE_CAPABILITIES: &str = r#"{"sessionCapabilities":{"close":{}}}"#;

fn terminal_args(scenario: &str, probe: &Path, heartbeat: Option<&Path>) -> Vec<String> {
    let mut args = vec![
        "--scenario".to_owned(),
        scenario.to_owned(),
        "--capabilities".to_owned(),
        CLOSE_CAPABILITIES.to_owned(),
        "--probe-file".to_owned(),
        probe.to_string_lossy().into_owned(),
    ];
    if let Some(heartbeat) = heartbeat {
        args.push("--terminal-heartbeat".to_owned());
        args.push(heartbeat.to_string_lossy().into_owned());
    }
    args
}

fn args_refs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_output_is_real_and_release_revokes_the_handle() {
    let probe = temp_path("probe-ok.txt");
    let args = terminal_args("terminal-ok", &probe, None);
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("运行一个命令"), support::timestamp())
        .await
        .expect("prompt");

    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(20))
            .await,
        "turn 必须完成：{:?}",
        collector.event_types()
    );
    assert_no_probe_failure(&collector);

    // 线级：release 之后 output / release 都必须被拒绝（fake Agent 已逐项校验）。
    assert_probe(&probe, "output_after_release_rejected");
    assert_probe(&probe, "release_after_release_rejected");

    // 事件：跨 UTF-8 边界的真实输出 + 可区分的 stderr。
    let (stdout, stderr) = terminal_streams(&collector);
    assert!(
        stdout.contains("第一段🙂第二段"),
        "stdout 必须完整（含跨读取边界的 emoji）：{stdout:?}"
    );
    assert!(stderr.contains("警告"), "stderr 内容必须保留：{stderr:?}");

    // 工具引用：tool.call.started 的 ACP 原文里有同一个 terminalId 的 structured 引用。
    let tool = collector
        .first_view("tool.call.started")
        .expect("tool call");
    let tool: Value = serde_json::from_str(&tool).expect("view");
    let terminal_id = views(&collector, "terminal.output")[0]["terminalId"]
        .as_str()
        .expect("terminalId")
        .to_owned();
    assert_eq!(tool["toolCallId"], "terminal-tool-1");
    let started = collector
        .snapshot()
        .into_iter()
        .find(|event| event.event_type.as_str() == "tool.call.started")
        .expect("tool.call.started");
    let raw = match started.payload.acp {
        Some(acp_core::model::AcpRaw::Available { raw_json, .. }) => raw_json,
        _ => panic!("tool.call.started 必须保留 ACP 原文"),
    };
    assert!(
        raw.contains(&terminal_id),
        "工具调用的原文必须引用同一个终端标识：{raw}"
    );

    endpoint.close().await.expect("close");
    host.shutdown_all().await;
}

/// 释放失败（Agent 明确拒绝）后绑定必须保留，且**终端服务不能因为释放前的收口而永久关闭**：
/// 那次收口换入的新实例必须照常完成一次真实的 `terminal/*` 流程。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_close_leaves_the_terminal_service_usable() {
    let probe = temp_path("probe-close-refused.txt");
    let args = terminal_args("close-error-terminal", &probe, None);
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;

    // Agent 明确拒绝关闭：不得声称成功，也不得摘除映射。
    assert!(
        endpoint.close().await.is_err(),
        "Agent 拒绝时单会话释放必须失败"
    );

    // 重绑把同一个终端服务移交给新绑定；它必须仍在服务（不是那个永久关闭的旧实例）。
    let reopened = host
        .open(endpoint.reference(), collector.sink())
        .await
        .expect("失败的关闭不得摘除映射");
    reopened
        .prompt(prompt("关闭失败后仍然可以运行命令"), support::timestamp())
        .await
        .expect("prompt");

    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(20))
            .await,
        "turn 必须完成：{:?}",
        collector.event_types()
    );
    assert_no_probe_failure(&collector);
    assert_probe(&probe, "output_after_release_rejected");
    assert_probe(&probe, "release_after_release_rejected");
    let (stdout, stderr) = terminal_streams(&collector);
    assert!(
        stdout.contains("第一段🙂第二段"),
        "终端输出必须真实到达：{stdout:?}"
    );
    assert!(stderr.contains("警告"), "stderr 内容必须保留：{stderr:?}");

    drop(endpoint);
    drop(reopened);
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_nonzero_exit_is_reported_with_its_output() {
    let probe = temp_path("probe-exit.txt");
    let args = terminal_args("terminal-exit-code", &probe, None);
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("失败的命令"), support::timestamp())
        .await
        .expect("prompt");

    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(20))
            .await,
        "{:?}",
        collector.event_types()
    );
    assert_no_probe_failure(&collector);
    let (stdout, stderr) = terminal_streams(&collector);
    assert!(stdout.contains("boom"), "保留已收到的 stdout：{stdout:?}");
    assert!(
        stderr.contains("失败原因"),
        "保留已收到的 stderr：{stderr:?}"
    );

    endpoint.close().await.expect("close");
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_output_budget_truncates_retained_output_only() {
    let probe = temp_path("probe-big.txt");
    let args = terminal_args("terminal-big", &probe, None);
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("大输出"), support::timestamp())
        .await
        .expect("prompt");

    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(20))
            .await,
        "{:?}",
        collector.event_types()
    );
    assert_no_probe_failure(&collector);
    // fake Agent 已断言 terminal/output 的 `truncated = true` 且正文 ≤ 1024 字符；
    // 实时事件仍然携带真实分块（可能超过 1024 字节），这正是「截断只影响保留正文」。
    let (stdout, _stderr) = terminal_streams(&collector);
    assert!(
        stdout.len() > 1024,
        "事件必须携带真实输出（不被保留预算裁掉）：{} 字节",
        stdout.len()
    );

    endpoint.close().await.expect("close");
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_kill_keeps_output_readable_and_repeated_operations_converge() {
    let probe = temp_path("probe-kill.txt");
    let args = terminal_args("terminal-kill", &probe, None);
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("长期命令"), support::timestamp())
        .await
        .expect("prompt");

    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(20))
            .await,
        "{:?}",
        collector.event_types()
    );
    assert_no_probe_failure(&collector);
    for marker in [
        "kill_ok",
        "kill_exit_reported",
        "kill_twice_ok",
        "wait_twice_stable",
        "output_after_release_rejected",
        "release_after_release_rejected",
    ] {
        assert_probe(&probe, marker);
    }
    let (stdout, _stderr) = terminal_streams(&collector);
    assert!(stdout.contains("started"), "kill 前的输出必须可见");

    endpoint.close().await.expect("close");
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_cross_session_access_is_rejected() {
    let probe = temp_path("probe-cross.txt");
    let args = terminal_args("terminal-cross-session", &probe, None);
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let first = create(&host, &session(SESSION_A), &collector).await;
    let second = create(&host, &session(SESSION_B), &collector).await;

    second
        .prompt(prompt("第二个会话"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(20))
            .await,
        "{:?}",
        collector.event_types()
    );
    assert_no_probe_failure(&collector);
    assert_probe(&probe, "cross_session_rejected");

    second.close().await.expect("close");
    first.close().await.expect("close");
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_capability_is_only_advertised_when_wired() {
    let probe = temp_path("probe-unadvertised.txt");
    let dump = temp_path("dump-unadvertised.jsonl");
    let mut args = terminal_args("terminal-not-advertised", &probe, None);
    args.push("--dump-request-params".to_owned());
    args.push(dump.to_string_lossy().into_owned());
    // **不**宣告 clientCapabilities.terminal：服务存在但能力未接线。
    let host = host_with(HostConfig::default(), &args_refs(&args));
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("未宣告能力"), support::timestamp())
        .await
        .expect("prompt");

    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(20))
            .await,
        "{:?}",
        collector.event_types()
    );
    assert_no_probe_failure(&collector);
    // Agent 收到了明确拒绝（等价于方法未找到），没有进程被启动、也没有任何终端事件。
    assert_probe(&probe, "create_rejected");
    assert!(
        views(&collector, "terminal.output").is_empty(),
        "未宣告能力时不得产生终端输出"
    );

    // `initialize` 里不得宣告 terminal（宣告与执行同一个判据）。
    let rows = wait_for_dumped_method(&dump, "initialize", Duration::from_secs(10)).await;
    let params = params_of(&rows, "initialize");
    let terminal = params
        .get("clientCapabilities")
        .and_then(|value| value.get("terminal"));
    assert_eq!(
        terminal, None,
        "未接线时 initialize 不得宣告 terminal：{params}"
    );

    endpoint.close().await.expect("close");
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_wait_is_cancelled_on_session_close_and_the_tree_is_cleaned() {
    let probe = temp_path("probe-wait.txt");
    let heartbeat = temp_path("heartbeat-wait.txt");
    let args = terminal_args("terminal-wait-hang", &probe, Some(heartbeat.as_ref()));
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("等待一个不会退出的命令"), support::timestamp())
        .await
        .expect("prompt");

    assert!(
        wait_for_probe(&probe, "wait_pending", Duration::from_secs(20)).await,
        "fake Agent 必须已经发出 wait_for_exit：{:?}",
        read_lines(&probe)
    );
    wait_for_heartbeat(heartbeat.as_ref(), Duration::from_secs(10)).await;

    // 关闭会话必须让等待者以明确结果结束（而不是永久悬挂），并结束整棵终端进程树。
    tokio::time::timeout(Duration::from_secs(10), endpoint.close())
        .await
        .expect("close 不得悬挂")
        .expect("close");
    assert!(
        wait_for_probe(&probe, "wait_cancelled", Duration::from_secs(10)).await,
        "wait_for_exit 必须被取消：{:?}",
        read_lines(&probe)
    );
    assert_heartbeat_stops(heartbeat.as_ref(), Duration::from_secs(10)).await;

    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_close_cleans_the_terminal_process_tree() {
    let probe = temp_path("probe-close.txt");
    let heartbeat = temp_path("heartbeat-close.txt");
    let args = terminal_args("terminal-orphan", &probe, Some(heartbeat.as_ref()));
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("留下一个仍在运行的终端"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(20))
            .await,
        "{:?}",
        collector.event_types()
    );
    assert_probe(&probe, "orphan_created");
    wait_for_heartbeat(heartbeat.as_ref(), Duration::from_secs(10)).await;

    endpoint.close().await.expect("close");
    assert_heartbeat_stops(heartbeat.as_ref(), Duration::from_secs(10)).await;

    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_crash_cleans_the_terminal_process_tree() {
    let probe = temp_path("probe-crash.txt");
    let heartbeat = temp_path("heartbeat-crash.txt");
    let args = terminal_args("terminal-orphan-crash", &probe, Some(heartbeat.as_ref()));
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("Agent 崩溃前留下终端"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(20))
            .await,
        "{:?}",
        collector.event_types()
    );
    wait_for_heartbeat(heartbeat.as_ref(), Duration::from_secs(10)).await;

    // Agent 进程退出后，终端进程树必须由退出路径清理（不依赖 Daemon 关闭）。
    let agent = AgentId::new(AGENT).expect("agent id");
    let deadline = Instant::now() + Duration::from_secs(10);
    while runtime_running(&host, &agent) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!runtime_running(&host, &agent), "Agent 必须已被判定退出");
    assert_heartbeat_stops(heartbeat.as_ref(), Duration::from_secs(10)).await;

    endpoint.close().await.expect("close");
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_shutdown_cleans_the_terminal_process_tree() {
    let probe = temp_path("probe-shutdown.txt");
    let heartbeat = temp_path("heartbeat-shutdown.txt");
    let args = terminal_args("terminal-orphan", &probe, Some(heartbeat.as_ref()));
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("Daemon 关闭前留下终端"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(20))
            .await,
        "{:?}",
        collector.event_types()
    );
    wait_for_heartbeat(heartbeat.as_ref(), Duration::from_secs(10)).await;

    host.shutdown_all().await;
    assert_heartbeat_stops(heartbeat.as_ref(), Duration::from_secs(10)).await;
}

/// `terminal/output` 的响应必须留在 ACP 单条消息上限内：保留正文接近 1 MiB 时，JSON-RPC 信封
/// 与 JSON 转义会把响应顶出上限。fake Agent 在响应到达时校验线上字节数并断言正文仍保留绝大部分；
/// 旧实现编码失败后会静默丢弃发送错误，这里会表现为 turn 超时而不是 PROBE_FAILED。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_output_response_stays_within_the_frame_budget_near_one_mib() {
    let probe = temp_path("probe-frame-budget.txt");
    let args = terminal_args("terminal-frame-budget", &probe, None);
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("接近 1 MiB 的纯文本输出"), support::timestamp())
        .await
        .expect("prompt");

    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(30))
            .await,
        "响应超帧时请求会悬挂而不是完成：{:?}",
        collector.event_types()
    );
    assert_no_probe_failure(&collector);
    assert_probe(&probe, "frame_budget_ok");

    endpoint.close().await.expect("close");
    host.shutdown_all().await;
}

/// 大量换行与 ANSI 序列：保留正文的原始字节数没到 1 MiB，但 JSON 转义后（换行 2 字节、
/// ESC 6 字节）会超帧。裁剪必须按转义后的实际字节计算，并从开头丢弃、置 `truncated = true`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_output_response_accounts_for_newline_and_ansi_escaping() {
    let probe = temp_path("probe-frame-escapes.txt");
    let args = terminal_args("terminal-frame-escapes", &probe, None);
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("大量换行与 ANSI 序列"), support::timestamp())
        .await
        .expect("prompt");

    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(30))
            .await,
        "转义膨胀导致的超帧同样必须显式裁剪而不是悬挂：{:?}",
        collector.event_types()
    );
    assert_no_probe_failure(&collector);
    assert_probe(&probe, "frame_budget_ok");

    endpoint.close().await.expect("close");
    host.shutdown_all().await;
}

/// 标准关闭路径（`close_session` → `shutdown_terminals`）必须排空最后的输出：
/// 同步关闭排空出的终端要保留给异步关闭，否则关闭返回后驱动任务仍可能补发事件。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_close_drains_the_last_terminal_output_before_returning() {
    let probe = temp_path("probe-drain.txt");
    let heartbeat = temp_path("heartbeat-drain.txt");
    let args = terminal_args("terminal-orphan", &probe, Some(heartbeat.as_ref()));
    let host = host_with(
        HostConfig {
            client_capabilities: terminal_capabilities(),
            ..HostConfig::default()
        },
        &args_refs(&args),
    );
    let collector = Collector::new();
    let endpoint = create(&host, &session(SESSION_A), &collector).await;
    endpoint
        .prompt(prompt("留下一个仍在运行的终端"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        wait_for_probe(&probe, "orphan_created", Duration::from_secs(20)).await,
        "fake Agent 必须已经创建终端：{:?}",
        read_lines(&probe)
    );
    // 先确认终端子进程真的运行过（孙进程心跳），避免在它写出启动行之前就关闭。
    wait_for_heartbeat(heartbeat.as_ref(), Duration::from_secs(10)).await;

    // 不额外等待输出事件：创建后立刻关闭，把「驱动任务尚未交付的输出」留给关闭路径排空。
    endpoint.close().await.expect("close");

    let (stdout, _stderr) = terminal_streams(&collector);
    assert!(
        stdout.contains("started"),
        "关闭路径必须排空已读输出（不能因为同步关闭先消费列表而丢失）：{stdout:?}"
    );
    let delivered = views(&collector, "terminal.output").len();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        views(&collector, "terminal.output").len(),
        delivered,
        "async 关闭返回后不得再有 terminal.output 事件"
    );

    assert_heartbeat_stops(heartbeat.as_ref(), Duration::from_secs(10)).await;
    host.shutdown_all().await;
}
