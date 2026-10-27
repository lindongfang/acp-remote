//! 单会话释放：能力门控、线级 `session/close`、真实资源保留与旧代隔离。
//!
//! 入口同样是**真实 stdio 子进程** `acpr-fake-acp-agent`。断言绑定四类可观察事实：
//!
//! - `--dump-request-params` 的每一行（是否发了 `session/close`、参数形状、发送次数）；
//! - `SessionEndpoint::close` / `AgentHost::release_session` 的返回值；
//! - `AgentHost::open` 是否还能复用绑定（映射是否真的被摘除）；
//! - 运行时目录里进程是否仍存活（单会话释放不得终止共享进程）。

mod support;

use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{
    AgentId, AgentProfile, AgentRef, AgentSessionId, CreateSessionRequest, PortError,
    PromptContentBlock, PromptRequest, ResourceOrigin, ResumeSessionRequest, SessionId,
    UnavailableKind,
};
use acp_core::ports::{SessionBackendFactory, SessionEndpoint};
use agent_host::{AgentHost, HostConfig, ReleaseOutcome, runtime_running};
use support::{
    Collector, FAKE_AGENT, TempFile, TestClock, TestIds, dump_count, dumped_requests, params_of,
    profile_with,
};

const CLOSE_DECLARED: &str = r#"{"sessionCapabilities":{"close":{}}}"#;
const CLOSE_AND_RESUME: &str = r#"{"sessionCapabilities":{"close":{},"resume":{}}}"#;

fn host(profiles: Vec<AgentProfile>) -> Arc<AgentHost> {
    Arc::new(AgentHost::new(
        Arc::new(profiles),
        HostConfig::default(),
        Arc::new(TestIds::new()),
        Arc::new(TestClock::new()),
    ))
}

fn agent_ref(agent: &str) -> AgentRef {
    AgentRef::try_new(AgentId::new(agent).expect("id"), &format!("Agent {agent}")).expect("ref")
}

fn session_id(text: &str) -> SessionId {
    SessionId::new(text).expect("session id")
}

async fn create(
    host: &Arc<AgentHost>,
    agent: &str,
    session: &SessionId,
    collector: &Collector,
) -> Box<dyn SessionEndpoint> {
    host.create(
        session,
        CreateSessionRequest::new(
            agent_ref(agent),
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
        PromptContentBlock::from_json_text(&format!(r#"{{"type":"text","text":"{text}"}}"#))
            .expect("内容块"),
    ])
}

/// `dyn SessionEndpoint` 没有 `Debug`，因此不能 `expect_err`。
fn outcome_error<T>(result: Result<T, PortError>) -> PortError {
    match result {
        Ok(_) => panic!("期望失败，但得到了成功结果"),
        Err(error) => error,
    }
}

fn persisted_workspace_cwd() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

fn resume_request(agent: &str, agent_session_id: &str) -> ResumeSessionRequest {
    ResumeSessionRequest::try_new(
        agent_ref(agent),
        AgentSessionId::new(agent_session_id).expect("agent session id"),
        persisted_workspace_cwd(),
    )
    .expect("恢复请求")
}

fn profile(agent: &str, scenario: &str, capabilities: &str, params_path: &str) -> AgentProfile {
    profile_with(
        agent,
        FAKE_AGENT,
        &[
            "--scenario",
            scenario,
            "--capabilities",
            capabilities,
            "--dump-request-params",
            params_path,
        ],
    )
}

/// 当前绑定的资源代（经由本层报告的资源事实；`SessionEndpoint` 端口本身不带代）。
async fn generation_of(host: &AgentHost, session: &SessionId) -> u64 {
    host.session_resources()
        .await
        .into_iter()
        .find(|facts| &facts.session == session)
        .map(|facts| facts.generation)
        .expect("会话必须有一条活动绑定")
}

/// 未宣告 `sessionCapabilities.close`：MUST NOT 发送 `session/close`，且必须显式报告后端不支持，
/// 同时保留真实资源（映射仍可 `open`、进程仍在运行）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_undeclared_close_capability_sends_no_request_and_keeps_the_resource() {
    let params_dump = TempFile::new("acpr-close-undeclared.params");
    let params_path = params_dump.to_string_lossy().into_owned();
    let host = host(vec![profile("agent-1", "normal", "{}", &params_path)]);
    let session = session_id("11111111-1111-4111-8111-111111111111");
    let endpoint = create(&host, "agent-1", &session, &Collector::new()).await;

    let error = outcome_error(endpoint.close().await);
    assert!(
        matches!(
            error,
            PortError::Unavailable(UnavailableKind::BackendUnsupported)
        ),
        "未宣告 close 必须表达为「后端不支持」：{error}"
    );
    assert_eq!(
        dump_count(&dumped_requests(&params_dump), "session/close"),
        0,
        "未宣告时 MUST NOT 发送 session/close"
    );

    // 资源事实仍在：映射可复用，进程没有因为一次失败的关闭被结束。
    let reused = host
        .open(endpoint.reference(), Collector::new().sink())
        .await
        .expect("失败的关闭不得摘除映射");
    assert_eq!(reused.reference().session_id(), &session);
    assert_eq!(
        host.release_session(&session).await,
        ReleaseOutcome::Failed,
        "逐会话释放端口同样必须报告未释放"
    );
    assert_eq!(
        dump_count(&dumped_requests(&params_dump), "session/close"),
        0,
        "release_session 也不得绕过能力门控"
    );
    assert!(
        runtime_running(&host, &AgentId::new("agent-1").expect("id")),
        "未宣告能力时不得以任何方式结束共享进程"
    );
    drop(endpoint);
    drop(reused);
    host.shutdown_all().await;
}

/// 宣告 close 后：线级只发一次 `session/close { sessionId }`；成功后才摘除旧映射、标记关闭；
/// 再次恢复会建立**新代**绑定（旧端点不可用、新端点可派发）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_declared_close_request_is_wire_exact_and_releases_the_binding() {
    let params_dump = TempFile::new("acpr-close-declared.params");
    let params_path = params_dump.to_string_lossy().into_owned();
    let host = host(vec![profile(
        "agent-1",
        "resume-ok",
        CLOSE_AND_RESUME,
        &params_path,
    )]);
    let session = session_id("22222222-2222-4222-8222-222222222222");
    let endpoint = create(&host, "agent-1", &session, &Collector::new()).await;
    let acp_session_id = endpoint
        .agent_session_id()
        .map(AgentSessionId::as_str)
        .expect("create 必须给出 ACP 会话标识")
        .to_owned();
    let first_generation = generation_of(&host, &session).await;

    endpoint.close().await.expect("宣告能力后关闭必须成功");

    let rows = dumped_requests(&params_dump);
    assert_eq!(
        dump_count(&rows, "session/close"),
        1,
        "恰好一次 session/close：{rows:?}"
    );
    let params = params_of(&rows, "session/close");
    assert_eq!(
        params["sessionId"],
        serde_json::Value::String(acp_session_id.clone()),
        "sessionId 必须逐字是该绑定的 ACP 会话标识"
    );
    assert_eq!(
        params.as_object().map(|object| object.len()),
        Some(1),
        "关闭参数只允许 sessionId：{params}"
    );

    // 旧映射不可用，且再次释放是幂等的（没有本层资源可释放）。
    assert!(
        host.open(endpoint.reference(), Collector::new().sink())
            .await
            .is_err(),
        "关闭成功后旧映射必须不可用"
    );
    assert_eq!(
        host.release_session(&session).await,
        ReleaseOutcome::Released,
        "已确认释放后的重复释放不得再发请求或报占用"
    );
    assert_eq!(
        dump_count(&dumped_requests(&params_dump), "session/close"),
        1,
        "重复释放不得再发 session/close"
    );

    // 同一会话可以按原身份重新建立绑定：新代严格更大，且真的可派发。
    let collector = Collector::new();
    let resumed = host
        .resume(
            &session,
            resume_request("agent-1", &acp_session_id),
            collector.sink(),
        )
        .await
        .expect("关闭后必须能重新恢复");
    let resumed_generation = generation_of(&host, &session).await;
    assert!(
        resumed_generation > first_generation,
        "重新恢复必须得到新的资源代：{resumed_generation} <= {first_generation}"
    );
    assert_eq!(
        resumed
            .agent_session_id()
            .map(AgentSessionId::as_str)
            .map(str::to_owned),
        Some(acp_session_id),
        "恢复仍使用原会话身份"
    );
    // 针对旧资源代的释放必须被拒绝：不得发出 session/close，也不得动新代绑定。
    assert_eq!(
        host.release_session_at_generation(&session, first_generation)
            .await,
        ReleaseOutcome::Failed,
        "旧资源代的释放不得作用于新代绑定"
    );
    assert_eq!(
        dump_count(&dumped_requests(&params_dump), "session/close"),
        1,
        "旧代释放不得再发 session/close"
    );
    resumed
        .prompt(prompt("关闭后恢复"), support::timestamp())
        .await
        .expect("新代绑定必须可派发");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(15))
            .await,
        "恢复后的 turn 必须完成：{:?}",
        collector.event_types()
    );
    let rows = dumped_requests(&params_dump);
    assert_eq!(
        dump_count(&rows, "session/resume"),
        1,
        "恢复恰好一次：{rows:?}"
    );
    assert_eq!(
        dump_count(&rows, "initialize"),
        1,
        "恢复必须复用既有进程（不得另拉进程）：{rows:?}"
    );

    drop(endpoint);
    drop(resumed);
    host.shutdown_all().await;
}

/// Agent 明确拒绝与超时都**不**算释放：映射保留、`open` 仍可用、`release_session` 分别报
/// `Failed` / `Uncertain`（不得只置本地 closed 位）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_or_timed_out_close_keeps_the_binding_and_reports_not_released() {
    let refusal_dump = TempFile::new("acpr-close-refused.params");
    let timeout_dump = TempFile::new("acpr-close-timeout.params");
    let refusal_path = refusal_dump.to_string_lossy().into_owned();
    let timeout_path = timeout_dump.to_string_lossy().into_owned();
    let host = host(vec![
        profile("agent-1", "close-error", CLOSE_DECLARED, &refusal_path),
        profile("agent-2", "close-timeout", CLOSE_DECLARED, &timeout_path),
    ]);

    // ① Agent 明确拒绝。
    let refused_session = session_id("33333333-3333-4333-8333-333333333333");
    let refused = create(&host, "agent-1", &refused_session, &Collector::new()).await;
    let error = outcome_error(refused.close().await);
    assert!(
        matches!(error, PortError::InvalidRequest(_)),
        "Agent 明确拒绝必须返回明确错误：{error}"
    );
    assert!(
        host.open(refused.reference(), Collector::new().sink())
            .await
            .is_ok(),
        "被拒绝的关闭必须保留映射与资源"
    );
    assert_eq!(
        host.release_session(&refused_session).await,
        ReleaseOutcome::Failed,
        "Agent 明确拒绝 ⇒ Failed（不是 Uncertain）"
    );
    assert_eq!(
        dump_count(&dumped_requests(&refusal_dump), "session/close"),
        2,
        "显式 close 与 release_session 各发一次，且不重试：{:?}",
        dumped_requests(&refusal_dump)
    );
    assert!(runtime_running(
        &host,
        &AgentId::new("agent-1").expect("id")
    ));

    // ② Agent 不响应：有界超时后结果不确定，资源仍占用。
    let timeout_session = session_id("44444444-4444-4444-8444-444444444444");
    let timed_out = create(&host, "agent-2", &timeout_session, &Collector::new()).await;
    assert_eq!(
        host.release_session(&timeout_session).await,
        ReleaseOutcome::Uncertain,
        "超时属于说不清，不得声称释放"
    );
    assert!(
        host.open(timed_out.reference(), Collector::new().sink())
            .await
            .is_ok(),
        "超时的关闭必须保留映射与资源"
    );
    assert_eq!(
        dump_count(&dumped_requests(&timeout_dump), "session/close"),
        1,
        "超时路径只发一次 session/close（不重试）：{:?}",
        dumped_requests(&timeout_dump)
    );
    assert!(runtime_running(
        &host,
        &AgentId::new("agent-2").expect("id")
    ));

    drop(refused);
    drop(timed_out);
    host.shutdown_all().await;
}

/// 同 Agent 的 A 关闭不影响 B：共享进程仍在运行，B 仍可派发与重开。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_one_session_leaves_the_shared_process_and_other_sessions_untouched() {
    let params_dump = TempFile::new("acpr-close-isolation.params");
    let params_path = params_dump.to_string_lossy().into_owned();
    let host = host(vec![profile(
        "agent-1",
        "normal",
        CLOSE_DECLARED,
        &params_path,
    )]);
    let session_a = session_id("55555555-5555-4555-8555-555555555555");
    let session_b = session_id("66666666-6666-4666-8666-666666666666");
    let collector_b = Collector::new();
    let a = create(&host, "agent-1", &session_a, &Collector::new()).await;
    let b = create(&host, "agent-1", &session_b, &collector_b).await;
    let a_acp_id = a
        .agent_session_id()
        .map(AgentSessionId::as_str)
        .map(str::to_owned)
        .expect("A 的 ACP 标识");
    assert_ne!(
        Some(a_acp_id.clone()),
        b.agent_session_id()
            .map(AgentSessionId::as_str)
            .map(str::to_owned),
        "两个会话必须各有自己的 ACP 标识"
    );

    a.close().await.expect("A 关闭成功");

    let rows = dumped_requests(&params_dump);
    assert_eq!(dump_count(&rows, "session/close"), 1, "只关闭 A：{rows:?}");
    assert_eq!(
        params_of(&rows, "session/close")["sessionId"],
        serde_json::Value::String(a_acp_id),
        "关闭必须指向 A 的 ACP 标识"
    );
    assert_eq!(
        dump_count(&rows, "session/new"),
        2,
        "两个会话各建一次：{rows:?}"
    );

    // B 不受影响：仍可派发、仍可重开，且共享进程没有因为 A 的释放被结束。
    b.prompt(prompt("B 仍然可用"), support::timestamp())
        .await
        .expect("B 必须仍可派发");
    assert!(
        collector_b
            .wait_for_type("turn.completed", Duration::from_secs(15))
            .await,
        "B 的 turn 必须完成：{:?}",
        collector_b.event_types()
    );
    assert!(
        host.open(b.reference(), Collector::new().sink())
            .await
            .is_ok(),
        "B 的映射必须仍然有效"
    );
    assert!(
        runtime_running(&host, &AgentId::new("agent-1").expect("id")),
        "单会话释放不得终止共享进程"
    );
    assert_eq!(
        host.release_session(&session_a).await,
        ReleaseOutcome::Released
    );

    drop(a);
    drop(b);
    host.shutdown_all().await;
}

/// 关闭在途时**不得**重绑同一个会话：释放与绑定变更共用一把门，`open` 必须等释放结束。
///
/// 旧实现允许关闭在途时重绑：旧代 ack 到达后 `remove_session_if_current` 因代不同返回 false，目录里
/// 留下一个指向「已被 Agent 释放的会话」的新绑定。现在这条路被绑定门堵住——重绑只能看到释放之后的
/// 事实（映射已摘除 ⇒ 显式失败），真正重新建立绑定必须走 `resume`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_close_in_flight_blocks_rebinding_the_same_session() {
    let params_dump = TempFile::new("acpr-close-stale.params");
    let params_path = params_dump.to_string_lossy().into_owned();
    let host = host(vec![profile(
        "agent-1",
        "slow-close",
        CLOSE_AND_RESUME,
        &params_path,
    )]);
    let session = session_id("77777777-7777-4777-8777-777777777777");
    let endpoint = create(&host, "agent-1", &session, &Collector::new()).await;
    let acp_session_id = endpoint
        .agent_session_id()
        .map(AgentSessionId::as_str)
        .expect("create 必须给出 ACP 会话标识")
        .to_owned();
    let first_generation = generation_of(&host, &session).await;
    let reference = endpoint.reference();

    let close_future = endpoint.close();
    let rebind_future = async {
        // 等线级证据：`session/close` 已经发出（而不是靠固定 sleep 猜时序）。
        for _ in 0..200 {
            if dump_count(&dumped_requests(&params_dump), "session/close") > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // Agent 还会等 300 ms 才 ack；这段时间内重绑必须仍被挡住（既不得成功，也不得提前失败）。
        tokio::time::timeout(
            Duration::from_millis(120),
            host.open(reference.clone(), Collector::new().sink()),
        )
        .await
    };
    let (close_result, rebind) = tokio::join!(close_future, rebind_future);
    assert!(
        rebind.is_err(),
        "关闭在途时 open 不得完成（重绑必须等释放结束）"
    );
    close_result.expect("慢 ack 最终仍应成功");

    // 释放已确认：映射被摘除，重绑没有可复用的绑定；重复释放幂等且不再发请求。
    assert!(
        host.open(reference.clone(), Collector::new().sink())
            .await
            .is_err(),
        "已确认释放的会话不得被重绑"
    );
    assert_eq!(
        host.release_session(&session).await,
        ReleaseOutcome::Released,
        "确认释放后的重复释放必须幂等"
    );
    assert_eq!(
        dump_count(&dumped_requests(&params_dump), "session/close"),
        1,
        "重复释放不得再发 session/close：{:?}",
        dumped_requests(&params_dump)
    );

    // 真的重新建立一个绑定只能走 resume：它拿到新代，且旧端点随后的关闭不会发出第二个请求。
    let collector = Collector::new();
    let resumed = host
        .resume(
            &session,
            resume_request("agent-1", &acp_session_id),
            collector.sink(),
        )
        .await
        .expect("释放后必须能重新恢复");
    let resumed_generation = generation_of(&host, &session).await;
    assert!(
        resumed_generation > first_generation,
        "重新恢复必须得到新代：{resumed_generation} <= {first_generation}"
    );
    endpoint
        .close()
        .await
        .expect("已让出的旧端点重复关闭必须幂等成功");
    assert_eq!(
        dump_count(&dumped_requests(&params_dump), "session/close"),
        1,
        "旧代端点不得再发 session/close 把共享会话真关掉：{:?}",
        dumped_requests(&params_dump)
    );
    resumed
        .prompt(prompt("新代仍然可用"), support::timestamp())
        .await
        .expect("新代绑定必须可派发");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(15))
            .await,
        "新代的 turn 必须完成：{:?}",
        collector.event_types()
    );

    drop(endpoint);
    drop(resumed);
    host.shutdown_all().await;
}

/// A managed process that exited has already released every session resource in its tree.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_exited_managed_process_is_a_confirmed_session_release() {
    let host = host(vec![profile_with(
        "agent-1",
        FAKE_AGENT,
        &["--scenario", "exit-after-new"],
    )]);
    let session = session_id("88888888-8888-4888-8888-888888888888");
    let endpoint = create(&host, "agent-1", &session, &Collector::new()).await;
    let generation = generation_of(&host, &session).await;
    let agent = AgentId::new("agent-1").expect("id");
    for _ in 0..200 {
        if !runtime_running(&host, &agent) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        !runtime_running(&host, &agent),
        "the fake process must have exited before release"
    );

    assert_eq!(
        host.release_session_at_generation(&session, generation)
            .await,
        ReleaseOutcome::Released,
        "an exited process confirms that the target session no longer has a live resource"
    );
    assert_eq!(
        host.release_session_at_generation(&session, generation)
            .await,
        ReleaseOutcome::Released,
        "releasing the already-exited binding must be idempotent"
    );

    drop(endpoint);
    host.shutdown_all().await;
}

/// 没有 descriptor 只代表本进程没有可检查的句柄；除非同代已有肯定释放证明，
/// 否则不能把重启前可能仍存活的进程误判成已释放。
#[tokio::test]
async fn a_missing_descriptor_without_release_evidence_is_uncertain() {
    let host = host(Vec::new());
    let session = session_id("99999999-9999-4999-8999-999999999999");

    assert_eq!(
        host.release_session_at_generation(&session, 1).await,
        ReleaseOutcome::Uncertain,
        "missing descriptor alone cannot confirm that a prior process exited"
    );
    assert_eq!(
        host.release_session(&session).await,
        ReleaseOutcome::Uncertain,
        "the unversioned endpoint must also fail closed without release evidence"
    );

    host.shutdown_all().await;
}
