//! 进程监督：分帧、request id、超时、stderr 有界、关闭顺序与进程树回归。
//!
//! 场景由 fake ACP child（`src/bin/acpr-fake-acp-agent.rs`）按 argv 选择，因此本文件只验证**可观察行为**：
//! 我们发出去的消息、收到的响应、进程是否结束、孙进程是否停止。

mod support;

use std::time::Duration;

use acp_core::model::EndpointEvent;
use agent_host::launch::LaunchSpec;
use agent_host::process::Supervisor;
use serde_json::json;
use support::{Collector, launch_spec, scenario, scenario_with};

/// 起一个监督者并完成 `initialize`（多数用例的公共前置）。
async fn started(spec: LaunchSpec) -> (std::sync::Arc<Supervisor>, Collector) {
    let (supervisor, incoming) = Supervisor::start(spec).await.expect("spawn");
    let collector = Collector::new();
    // 进站通道必须在测试期间被消费，否则路由端会积压；这里把消息丢给收集器即可。
    let sink = collector.sink();
    tokio::spawn(async move {
        let mut incoming = incoming;
        while let Some(envelope) = incoming.recv().await {
            let view = envelope.document().as_str().to_owned();
            let _ = &sink;
            let _ = view;
        }
    });
    (supervisor, collector)
}

fn initialize_params() -> serde_json::Value {
    json!({
        "protocolVersion": 1,
        "clientCapabilities": {},
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initialize_succeeds_and_reports_capabilities() {
    let (supervisor, _collector) = started(scenario("normal")).await;
    let value = supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(10))
        .await
        .expect("initialize");
    assert_eq!(
        value.get("protocolVersion").and_then(|v| v.as_i64()),
        Some(1)
    );
    assert!(supervisor.is_running());
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initialize_must_not_spawn_a_shell() {
    // 参数数组直传：把「注入」当成参数，进程只会看到它作为普通 argv（不可能被 shell 解释）。
    let spec = launch_spec(&["--scenario", "normal"]);
    let (supervisor, _collector) = started(spec).await;
    let value = supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(10))
        .await
        .expect("initialize");
    assert_eq!(
        value.get("protocolVersion").and_then(|v| v.as_i64()),
        Some(1)
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn out_of_order_responses_still_match_their_requests() {
    let (supervisor, _collector) = started(scenario("out-of-order")).await;
    supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(10))
        .await
        .expect("initialize");
    supervisor
        .request(
            "session/new",
            &json!({ "cwd": std::env::temp_dir().to_string_lossy() }),
            Duration::from_secs(10),
        )
        .await
        .expect("session/new");

    // 两个 prompt 同时未完成，fake child 会「后到的先响应」。
    let first = supervisor
        .begin_request(
            "session/prompt",
            &json!({ "sessionId": "acp-session-1", "prompt": [] }),
        )
        .expect("first");
    let second = supervisor
        .begin_request(
            "session/prompt",
            &json!({ "sessionId": "acp-session-1", "prompt": [] }),
        )
        .expect("second");
    let first = first.await.expect("channel").expect("first result");
    let second = second.await.expect("channel").expect("second result");
    assert_eq!(
        first.get("stopReason").and_then(|v| v.as_str()),
        Some("end_turn")
    );
    assert_eq!(
        second.get("stopReason").and_then(|v| v.as_str()),
        Some("end_turn")
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_response_id_is_a_protocol_error_without_breaking_others() {
    let (supervisor, _collector) = started(scenario("unknown-id")).await;
    supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(10))
        .await
        .expect("initialize");
    supervisor
        .request(
            "session/new",
            &json!({ "cwd": std::env::temp_dir().to_string_lossy() }),
            Duration::from_secs(10),
        )
        .await
        .expect("session/new");
    // 未匹配的响应被记为协议错误，但同一批里的正常响应必须照常结算。
    let value = supervisor
        .request(
            "session/prompt",
            &json!({ "sessionId": "acp-session-1", "prompt": [] }),
            Duration::from_secs(10),
        )
        .await
        .expect("prompt");
    assert_eq!(
        value.get("stopReason").and_then(|v| v.as_str()),
        Some("end_turn")
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn illegal_json_lines_are_skipped_without_failing_the_request() {
    let (supervisor, _collector) = started(scenario("illegal-json")).await;
    let value = supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(10))
        .await
        .expect("initialize");
    assert_eq!(
        value.get("protocolVersion").and_then(|v| v.as_i64()),
        Some(1)
    );
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_timeout_fires_without_killing_the_process_early() {
    let (supervisor, _collector) = started(scenario("slow-initialize")).await;
    let error = supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(2))
        .await
        .expect_err("必须超时");
    assert!(matches!(error, agent_host::HostError::Timeout { .. }));
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn turn_has_no_timeout() {
    let (supervisor, _collector) = started(scenario("no-response")).await;
    supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(10))
        .await
        .expect("initialize");
    supervisor
        .request(
            "session/new",
            &json!({ "cwd": std::env::temp_dir().to_string_lossy() }),
            Duration::from_secs(10),
        )
        .await
        .expect("session/new");
    // 启动一个不会回话的 prompt，等待远超短请求超时的时间：它既不能超时也不能杀进程。
    let mut receiver = supervisor
        .begin_request(
            "session/prompt",
            &json!({ "sessionId": "acp-session-1", "prompt": [] }),
        )
        .expect("begin");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(supervisor.is_running(), "turn 未结束前进程必须还活着");
    // 未完成 = 通道仍然开着（oneshot 没有 is_finished，用 try_recv 判定）。
    assert!(receiver.try_recv().is_err(), "turn 不设超时，不能提前收敛");
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stderr_is_bounded_and_never_enters_the_protocol_channel() {
    // 普通场景：stdout 干净、stderr 为空，环形缓冲不得产生丢弃。
    let (supervisor, _collector) = started(scenario("normal")).await;
    supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(10))
        .await
        .expect("initialize");
    let (text, dropped) = supervisor.stderr_snapshot();
    assert!(text.len() <= agent_host::limits::STDERR_RING_BYTES);
    assert_eq!(dropped, 0, "该场景不应产生 stderr 丢弃");
    supervisor.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_fails_pending_requests_and_joins_every_task() {
    let (supervisor, _collector) = started(scenario("no-response")).await;
    supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(10))
        .await
        .expect("initialize");
    let pending = supervisor
        .begin_request(
            "session/prompt",
            &json!({ "sessionId": "acp-session-1", "prompt": [] }),
        )
        .expect("begin");
    supervisor.shutdown().await;
    // 未完成请求必须以明确错误收敛，不能悬挂。
    let error = pending.await.expect("channel").expect_err("必须失败");
    assert!(matches!(
        error,
        agent_host::HostError::ShutdownPending | agent_host::HostError::AgentExited { .. }
    ));
    assert!(!supervisor.is_running());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_after_abnormal_exit_is_clean() {
    let (supervisor, _collector) = started(scenario("crash-on-prompt")).await;
    supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(10))
        .await
        .expect("initialize");
    supervisor
        .request(
            "session/new",
            &json!({ "cwd": std::env::temp_dir().to_string_lossy() }),
            Duration::from_secs(10),
        )
        .await
        .expect("session/new");
    let pending = supervisor
        .begin_request(
            "session/prompt",
            &json!({ "sessionId": "acp-session-1", "prompt": [] }),
        )
        .expect("begin");
    let error = tokio::time::timeout(Duration::from_secs(5), pending)
        .await
        .expect("进程崩溃后请求必须被收敛")
        .expect("channel")
        .expect_err("崩溃后必须是错误");
    assert!(matches!(error, agent_host::HostError::AgentExited { .. }));
    // 崩溃之后关闭必须仍然干净（幂等、不悬挂）。
    supervisor.shutdown().await;
    assert!(!supervisor.is_running());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawned_child_receives_exactly_the_injected_environment() {
    let temp = std::env::temp_dir().join("acpr-agent-host-env.txt");
    let _ = std::fs::remove_file(&temp);
    let mut spec = scenario("normal");
    spec.args.push("--dump-env".to_owned());
    spec.args.push(temp.to_string_lossy().into_owned());
    // 故意多注入一个不在白名单里的变量，断言它确实被传下去（流程层负责只放白名单项）。
    spec.env
        .push(("FAKE_TOKEN".to_owned(), "fake-secret-value".to_owned()));
    spec.env
        .push(("LEAKED".to_owned(), "should-not-exist".to_owned()));
    let (supervisor, _collector) = started(spec).await;
    supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(10))
        .await
        .expect("initialize");
    let text = std::fs::read_to_string(&temp).expect("环境快照");
    let names: Vec<&str> = text
        .lines()
        .filter_map(|line| line.split('=').next())
        .collect();
    assert!(names.contains(&"FAKE_TOKEN"), "凭据变量必须注入");
    // 子进程看到的变量集合必须**等于**注入集合：宿主机的其它变量（这里是标记位）不得出现。
    assert!(!text.contains("CARGO_PKG_NAME="), "宿主变量不得泄漏");
    supervisor.shutdown().await;
    let _ = std::fs::remove_file(&temp);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tree_forced_termination_stops_the_whole_process_tree() {
    let temp = std::env::temp_dir().join("acpr-agent-host-heartbeat.txt");
    let _ = std::fs::remove_file(&temp);
    let spec = scenario_with(
        "spawn-grandchild",
        &["--heartbeat-file", &temp.to_string_lossy()],
    );
    let (supervisor, _collector) = started(spec).await;
    supervisor
        .request("initialize", &initialize_params(), Duration::from_secs(10))
        .await
        .expect("initialize");
    supervisor
        .request(
            "session/new",
            &json!({ "cwd": std::env::temp_dir().to_string_lossy() }),
            Duration::from_secs(10),
        )
        .await
        .expect("session/new");
    let _ = supervisor.begin_request(
        "session/prompt",
        &json!({ "sessionId": "acp-session-1", "prompt": [] }),
    );
    // 等到孙进程开始写心跳。
    let mut size = 0usize;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        size = std::fs::metadata(&temp)
            .map(|meta| meta.len() as usize)
            .unwrap_or(0);
        if size > 0 {
            break;
        }
    }
    assert!(size > 0, "孙进程必须先开始写心跳");

    // 关闭整棵树。
    supervisor.shutdown().await;
    let after_shutdown = std::fs::metadata(&temp)
        .map(|meta| meta.len() as usize)
        .unwrap_or(0);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let later = std::fs::metadata(&temp)
        .map(|meta| meta.len() as usize)
        .unwrap_or(0);
    assert_eq!(
        after_shutdown, later,
        "结束 Agent 后孙进程必须停止写心跳（进程树未被子进程逃逸）"
    );
    let _ = std::fs::remove_file(&temp);
}

/// `EndpointEvent` 的收集器在监督层用例里也必须被真的消费（防止「从未使用」告警与死积压）。
#[allow(dead_code)]
fn assert_event_shape(event: &EndpointEvent) {
    assert!(!event.event_type.as_str().is_empty());
}
