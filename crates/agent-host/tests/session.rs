//! 会话端点：事件映射与保真、turn 生命周期、交互往返、能力门控、会话恢复与映射表。

mod support;

use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{
    AgentId, AgentProfile, AgentRef, AgentSessionId, CreateSessionRequest, ExportId,
    InteractionResolution, NodeId, OriginEpoch, OwnedSessionRef, PermissionDecision,
    PermissionDecisionKind, PortError, PromptContentBlock, RemoteSessionRef, ResourceOrigin,
    ResumeSessionRequest, SessionId, SessionReference, UnavailableKind,
};
use acp_core::ports::{AgentCatalog, SessionBackendFactory, SessionEndpoint};
use agent_host::runtime_running;
use agent_host::{AgentHost, HostConfig, NodeEventError};
use serde_json::Value;
use support::{
    Collector, FAKE_AGENT, FakeConfig, FakeCredentials, TempFile, TestClock, TestIds, digest_of,
    profile, profile_with,
};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const CRASH_SESSION: &str = "66666666-6666-4666-8666-666666666666";
const GATE_SESSION: &str = "77777777-7777-4777-8777-777777777777";
const BLOCK_SESSION: &str = "88888888-8888-4888-8888-888888888888";
const OTHER_SESSION: &str = "22222222-2222-4222-8222-222222222222";
const RESUME_SESSION: &str = "33333333-3333-4333-8333-333333333333";
const RESUME_REFUSED_SESSION: &str = "44444444-4444-4444-8444-444444444444";
const REPEATED_RESUME_SESSION: &str = "55555555-5555-4555-8555-555555555555";

fn host(profiles: Vec<AgentProfile>, credentials: FakeCredentials) -> Arc<AgentHost> {
    Arc::new(AgentHost::new(
        Arc::new(FakeConfig::new(profiles)),
        Arc::new(credentials),
        HostConfig::default(),
        Arc::new(TestIds::new()),
        Arc::new(TestClock::new()),
    ))
}

fn agent_ref() -> AgentRef {
    AgentRef::try_new(AgentId::new("agent-1").expect("id"), "Agent agent-1").expect("ref")
}

fn session_id(text: &str) -> SessionId {
    SessionId::new(text).expect("session id")
}

/// 取出错误（`dyn SessionEndpoint` 没有 `Debug`，因此不能用 `expect_err`）。
fn outcome_error<T>(result: Result<T, PortError>) -> PortError {
    match result {
        Ok(_) => panic!("期望失败，但得到了成功结果"),
        Err(error) => error,
    }
}

async fn create(
    host: &Arc<AgentHost>,
    session: &str,
    collector: &Collector,
) -> Box<dyn SessionEndpoint> {
    host.create(
        &session_id(session),
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

fn prompt(text: &str) -> acp_core::model::PromptRequest {
    acp_core::model::PromptRequest::new(vec![
        PromptContentBlock::from_json_text(&format!("{{\"type\":\"text\",\"text\":\"{text}\"}}"))
            .expect("block"),
    ])
}

/// 恢复的输入：持久化的 ACP 会话标识 + 持久化的创建时 cwd 原文。
///
/// 恢复路径上没有别名、没有客户端参数，因此用例只能从这两个可能来自存储的取值构造请求
/// （目录的有效性由 core 在调用后端**之前**复校验，本 crate 不解析路径）。
fn resume_request(agent_session_id: &str, workspace_cwd: &str) -> ResumeSessionRequest {
    ResumeSessionRequest::try_new(
        agent_ref(),
        AgentSessionId::new(agent_session_id).expect("agent session id"),
        workspace_cwd.to_owned(),
    )
    .expect("resume request")
}

/// 本机上一个存在的绝对目录（用作持久化的创建时 cwd 原文）。
fn persisted_workspace_cwd() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

/// `--dump-requests <path>` 的记录：每次收到的带 `method` 的入站报文一行。
fn dumped_methods(path: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// 等子进程的心跳文件出现（它在存活期间由该进程创建）。
async fn wait_for_file(path: &std::path::Path, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    path.exists()
}

fn file_len(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn agent_id() -> AgentId {
    AgentId::new("agent-1").expect("id")
}

/// 事件里的 `acp` 原文必须与 Agent 发出的那一行**逐字节相同**。
fn assert_raw_fidelity(event: &acp_core::model::EndpointEvent) {
    let Some((media_type, raw_json, byte_length, sha256)) = event
        .payload
        .acp
        .as_ref()
        .and_then(|acp| acp.as_available())
    else {
        return;
    };
    assert_eq!(media_type, "application/json");
    assert!(
        !raw_json.contains('\n'),
        "ACP 原文不得包含分帧用的换行：{raw_json}"
    );
    assert_eq!(
        byte_length,
        raw_json.len() as u64,
        "byte_length 必须等于原文的 UTF-8 字节数"
    );
    assert_eq!(
        sha256,
        &digest_of(raw_json),
        "sha256 必须与原文一致（不允许先转 Value 再算）"
    );
    // 原文必须是良构 JSON 文档（不是被截断或拼接的片段）。
    let parsed: Value = serde_json::from_str(raw_json).expect("原文必须是良构 JSON");
    assert!(parsed.is_object());
    // 原文必须能被独立解析出类型化判别子，证明保真不是靠通用 Value 往返。
    assert!(
        raw_json.contains("sessionUpdate") || raw_json.contains("method"),
        "原文应当带着路由字段"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn updates_are_mapped_with_structured_content_and_raw_fidelity() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "chunked-updates"],
        )],
        FakeCredentials::ok(),
    );
    let endpoint = create(&host, SESSION, &collector).await;
    endpoint
        .prompt(prompt("你好"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(10))
            .await,
        "turn 必须完成：{:?}",
        collector.event_types()
    );

    let types = collector.event_types();
    for expected in [
        "agent.message.delta",
        "agent.thought.delta",
        "tool.call.started",
        "tool.call.updated",
        "tool.call.completed",
        "session.plan.changed",
        "session.commands.changed",
        "session.mode.changed",
        "session.config.changed",
        "session.info.changed",
        "session.usage.changed",
        "session.update.unknown",
        "turn.completed",
    ] {
        assert!(
            types.contains(&expected.to_owned()),
            "缺少事件 {expected}：{types:?}"
        );
    }

    // turn 归属交给 core：适配器不得自己填 turnId。
    for event in collector.snapshot() {
        assert!(event.turn.is_none(), "EndpointEvent.turn 必须为 None");
        assert_raw_fidelity(&event);
    }

    // delta 的 deltaIndex 必须从 0 起逐条加一（core 折叠同一条消息时依赖它）。
    let indices: Vec<String> = collector
        .snapshot()
        .into_iter()
        .filter(|event| event.event_type.as_str() == "agent.message.delta")
        .map(|event| {
            let view: Value =
                serde_json::from_str(event.payload.view.as_str()).expect("view 是 JSON 对象");
            view.get("deltaIndex")
                .and_then(Value::as_str)
                .expect("deltaIndex 是十进制字符串")
                .to_owned()
        })
        .collect();
    assert_eq!(indices, vec!["0".to_owned(), "1".to_owned()]);

    // tool call 必须保持结构化（判别子、工具标识、diff 与 terminal 都不被文本化）。
    let tool_view = collector
        .first_view("tool.call.started")
        .expect("tool view");
    let tool: Value = serde_json::from_str(&tool_view).expect("view");
    assert_eq!(
        tool.get("toolCallId").and_then(Value::as_str),
        Some("tool-1")
    );
    assert_eq!(
        tool.get("state").and_then(Value::as_str),
        Some("in_progress")
    );
    let tool_raw = collector
        .snapshot()
        .into_iter()
        .find(|event| event.event_type.as_str() == "tool.call.started")
        .and_then(|event| event.payload.acp)
        .and_then(|acp| acp.as_available().map(|(_, raw, _, _)| raw.to_owned()))
        .expect("tool call 必须带原文");
    assert!(tool_raw.contains("\"diff\""), "diff 必须原样保留");
    assert!(tool_raw.contains("\"terminal\""), "terminal 必须原样保留");
    assert!(
        tool_raw.contains("\"oldText\"") && tool_raw.contains("\"newText\""),
        "diff 的新旧文本都要保留"
    );

    // 未来判别子是可见降级：保留原文，且不是解码失败。
    let unknown = collector
        .snapshot()
        .into_iter()
        .find(|event| event.event_type.as_str() == "session.update.unknown")
        .expect("未知判别子必须可见");
    let unknown_view: Value = serde_json::from_str(unknown.payload.view.as_str()).expect("view");
    assert_eq!(
        unknown_view.get("sessionUpdate").and_then(Value::as_str),
        Some("future_thing_update")
    );

    // usage 的数字以十进制字符串给出（Sync 合同），不是浮点。
    let usage = collector
        .first_view("session.usage.changed")
        .expect("usage");
    let usage: Value = serde_json::from_str(&usage).expect("view");
    assert_eq!(usage.get("used").and_then(Value::as_str), Some("1024"));
    assert_eq!(usage.get("size").and_then(Value::as_str), Some("200000"));

    drop(endpoint);
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn turn_terminal_event_is_produced_exactly_once() {
    let collector = Collector::new();
    let host = host(vec![profile("agent-1", FAKE_AGENT)], FakeCredentials::ok());
    let endpoint = create(&host, SESSION, &collector).await;
    endpoint
        .prompt(prompt("hi"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(10))
            .await
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(collector.count("turn.completed"), 1, "终态只能有一次");
    assert_eq!(collector.count("turn.failed"), 0);
    assert_eq!(collector.count("turn.cancelled"), 0);
    drop(endpoint);
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prompt_is_accepted_immediately_and_turn_completes_later() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "no-response"],
        )],
        FakeCredentials::ok(),
    );
    let endpoint = create(&host, SESSION, &collector).await;
    let accepted = tokio::time::timeout(
        Duration::from_secs(2),
        endpoint.prompt(prompt("hi"), support::timestamp()),
    )
    .await
    .expect("prompt 必须立即返回（不等待 turn 结束）")
    .expect("accepted");
    assert!(
        !accepted.turn.as_str().is_empty(),
        "占位 turn id 必须是合法形态"
    );
    // turn 尚未结束：不能有终态事件，且第二个 prompt 必须被拒绝（同一会话最多一个 active turn）。
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(collector.count("turn.completed"), 0);
    let busy = endpoint
        .prompt(prompt("again"), support::timestamp())
        .await
        .expect_err("进行中的 turn 必须拒绝第二次派发");
    assert!(matches!(busy, PortError::InvalidRequest(_)));
    drop(endpoint);
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_finishes_only_its_own_turn_and_releases_the_session() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "no-response"],
        )],
        FakeCredentials::ok(),
    );
    let first = create(&host, SESSION, &collector).await;
    let second = create(&host, OTHER_SESSION, &collector).await;
    first
        .prompt(prompt("长任务"), support::timestamp())
        .await
        .expect("first prompt");
    second
        .prompt(prompt("另一个会话"), support::timestamp())
        .await
        .expect("second prompt");

    first.cancel(None).await.expect("cancel");
    assert!(
        collector
            .wait_for_type("turn.cancelled", Duration::from_secs(10))
            .await,
        "取消必须产生 turn.cancelled"
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(collector.count("turn.cancelled"), 1, "终态只能有一次");
    // 取消释放了该会话：可以立即派发新的 turn。
    first
        .prompt(prompt("又一轮"), support::timestamp())
        .await
        .expect("取消后必须能重新派发");
    // 另一个会话不受影响（仍然持有自己的 active turn）。
    let second_busy = second.prompt(prompt("仍然忙"), support::timestamp()).await;
    assert!(second_busy.is_err(), "另一个会话的 turn 不应被取消");
    drop(first);
    drop(second);
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn permission_request_waits_for_external_resolution_and_returns_the_same_option() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "permission-request"],
        )],
        FakeCredentials::ok(),
    );
    let endpoint = create(&host, SESSION, &collector).await;
    endpoint
        .prompt(prompt("改文件"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("permission.requested", Duration::from_secs(10))
            .await,
        "权限请求必须交付给 core"
    );
    // 未解析前 turn 不得结束（不代答、不用超时伪造结论）。
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(collector.count("turn.completed"), 0);
    assert_eq!(collector.count("turn.failed"), 0);

    let view = collector.first_view("permission.requested").expect("view");
    let view: Value = serde_json::from_str(&view).expect("view");
    let interaction = view
        .get("interactionId")
        .and_then(Value::as_str)
        .expect("interactionId 必须由适配器给出（broker 从这里读）")
        .to_owned();
    let options = view
        .get("options")
        .and_then(Value::as_array)
        .expect("候选项必须是结构化的");
    assert_eq!(options.len(), 2);
    assert_eq!(
        options[0].get("optionId").and_then(Value::as_str),
        Some("allow-1")
    );

    let interaction = acp_core::model::InteractionId::new(&interaction).expect("interaction id");
    endpoint
        .resolve_interaction(
            &interaction,
            InteractionResolution::permission(
                PermissionDecision::try_new("allow-1", PermissionDecisionKind::AllowOnce)
                    .expect("decision"),
            ),
        )
        .await
        .expect("resolve");
    // fake child 会校验回包的形状：只有原样带回 optionId 才会得到 `end_turn`。
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(10))
            .await,
        "解析后 turn 必须完成：{:?}",
        collector.event_types()
    );
    assert_eq!(collector.count("turn.failed"), 0);

    // 同一个交互不能解析两次。
    let again = endpoint
        .resolve_interaction(
            &interaction,
            InteractionResolution::permission(
                PermissionDecision::try_new("reject-1", PermissionDecisionKind::RejectOnce)
                    .expect("decision"),
            ),
        )
        .await
        .expect_err("重复解析必须失败");
    assert!(matches!(again, PortError::Conflict(_)));
    drop(endpoint);
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn elicitation_is_delivered_and_resolved_without_auto_answering() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "elicitation"],
        )],
        FakeCredentials::ok(),
    );
    let endpoint = create(&host, SESSION, &collector).await;
    endpoint
        .prompt(prompt("请提问"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("elicitation.requested", Duration::from_secs(10))
            .await
    );
    let view = collector.first_view("elicitation.requested").expect("view");
    let view: Value = serde_json::from_str(&view).expect("view");
    let interaction = view
        .get("interactionId")
        .and_then(Value::as_str)
        .expect("interactionId")
        .to_owned();
    assert!(view.get("schema").is_some(), "表单 schema 必须保留");

    let interaction = acp_core::model::InteractionId::new(&interaction).expect("interaction id");
    endpoint
        .resolve_interaction(&interaction, InteractionResolution::elicitation_cancel())
        .await
        .expect("resolve");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(10))
            .await,
        "elicitation 解析后 turn 必须完成"
    );
    drop(endpoint);
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capabilities_are_negotiated_truthfully_and_gate_unsupported_calls() {
    // 无能力声明的 Agent：能力集合必须是空的（不虚报）。
    let bare = host(vec![profile("agent-1", FAKE_AGENT)], FakeCredentials::ok());
    let capabilities = bare.agent_capabilities(&agent_ref()).await.expect("caps");
    assert!(capabilities.is_empty(), "未宣告任何能力时必须是空集合");

    // 宣告了能力：只放进真正宣告的路径。
    let declared = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &[
                "--scenario",
                "normal",
                "--capabilities",
                "{\"loadSession\":true,\"promptCapabilities\":{\"image\":true}}",
            ],
        )],
        FakeCredentials::ok(),
    );
    let capabilities = declared
        .agent_capabilities(&agent_ref())
        .await
        .expect("caps");
    let kinds: Vec<&str> = capabilities.iter().map(|cap| cap.kind()).collect();
    assert_eq!(
        kinds,
        vec![
            "agentCapabilities.loadSession",
            "agentCapabilities.promptCapabilities.image"
        ],
        "能力声明必须逐条对应矩阵词表"
    );
    declared.shutdown_all().await;

    // 未宣告配置项时显式拒绝（不发消息、不假装成功）。
    let collector = Collector::new();
    let endpoint = create(&bare, SESSION, &collector).await;
    let unknown = acp_core::model::ConfigOptionId::new("not-a-real-option").expect("id");
    let error = endpoint
        .set_config(&unknown, support::boolean(true))
        .await
        .expect_err("未知配置项必须显式拒绝");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    // 已宣告的配置项按 Agent 的真实结果返回（模式来自 `session/new` 的真实声明）。
    let modes = endpoint.modes().await.expect("modes");
    assert_eq!(
        modes
            .current_mode
            .as_ref()
            .map(|mode| mode.mode_id().as_str()),
        Some("default")
    );
    assert_eq!(modes.available.len(), 2, "候选模式只能是 Agent 声明的");
    let config = endpoint.list_config().await.expect("config");
    assert!(
        config
            .iter()
            .any(|option| option.id().as_str() == "verbose")
    );

    // 历史往返必须显式不支持，而不是返回空页（空页会被误读成「没有历史」）。
    let history = endpoint
        .read_history(acp_core::ports::HistoryQuery {
            session: session_id(SESSION),
            include: Default::default(),
            after: None,
            limit: acp_core::ports::ReplayLimit::new(10),
        })
        .await
        .expect_err("读历史必须显式不支持");
    assert!(matches!(history, PortError::InvalidRequest(_)));
    drop(endpoint);
    bare.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_mapping_covers_create_open_and_unknown_references() {
    let collector = Collector::new();
    let host = host(vec![profile("agent-1", FAKE_AGENT)], FakeCredentials::ok());
    let endpoint = create(&host, SESSION, &collector).await;
    assert_eq!(
        endpoint.reference().session_id().as_str(),
        SESSION,
        "引用必须是 core 给的 SessionId"
    );

    // 同一 core 会话重复 create 必须冲突。
    let duplicate = outcome_error(
        host.create(
            &session_id(SESSION),
            CreateSessionRequest::new(
                agent_ref(),
                Some(support::workspace()),
                None,
                ResourceOrigin::Local,
            ),
            collector.sink(),
        )
        .await,
    );
    assert!(
        matches!(duplicate, PortError::Conflict(_)),
        "重复 create 必须返回冲突类错误"
    );

    // open 只接受已存在的映射，并且替换掉旧绑定（同一会话不能有两个 endpoint）。
    let reopened = host
        .open(endpoint.reference(), collector.sink())
        .await
        .expect("open 已有会话");
    assert_eq!(reopened.reference().session_id().as_str(), SESSION);
    let stale = host.open(endpoint.reference(), collector.sink()).await;
    assert!(
        stale.is_ok(),
        "再次 open 仍然合法（绑定唯一性由 create 判定）"
    );

    // 未知 core 会话引用必须报错。
    let unknown = outcome_error(
        host.open(
            SessionReference::Owned(acp_core::model::OwnedSessionRef::new(session_id(
                "33333333-3333-4333-8333-333333333333",
            ))),
            collector.sink(),
        )
        .await,
    );
    assert!(matches!(unknown, PortError::InvalidRequest(_)));

    // imported（Remote）会话不归本 crate。
    let remote = SessionReference::Remote(RemoteSessionRef {
        owner_node_id: NodeId::new("44444444-4444-4444-8444-444444444444").expect("node"),
        export_id: ExportId::new("export-1").expect("export"),
        session_id: session_id(SESSION),
    });
    let imported = outcome_error(host.open(remote, collector.sink()).await);
    assert!(matches!(imported, PortError::InvalidRequest(_)));

    let _ = OriginEpoch::new("55555555-5555-4555-8555-555555555555");
    drop(endpoint);
    host.shutdown_all().await;
}

/// 进程在 turn 中途崩溃：必须产生**恰好一次** `turn.failed`，且后续写入被拒绝。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn turn_failed_is_reported_once_when_the_agent_crashes_mid_turn() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "crash-on-prompt"],
        )],
        FakeCredentials::ok(),
    );
    let endpoint = create(&host, CRASH_SESSION, &collector).await;
    endpoint
        .prompt(prompt("会崩"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("turn.failed", Duration::from_secs(10))
            .await,
        "崩溃必须变成明确的 turn.failed：{:?}",
        collector.event_types()
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(collector.count("turn.failed"), 1, "终态只能有一次");
    assert_eq!(collector.count("turn.completed"), 0);
    let view = collector.first_view("turn.failed").expect("view");
    let view: Value = serde_json::from_str(&view).expect("view");
    assert!(
        view.get("error").is_some(),
        "turn.failed 必须带结构化错误：{view}"
    );
    // 进程已死：后续写入必须被拒绝，而不是悬挂。
    let refused = endpoint.prompt(prompt("再来"), support::timestamp()).await;
    assert!(refused.is_err(), "崩溃后不得接受新的 turn");
    drop(endpoint);
    host.shutdown_all().await;
}

/// Agent 未宣告模式/配置项时：显式拒绝，并且**一个字节都不发**（fake child 收到就退出）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn undeclared_capability_is_refused_without_sending_anything() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &[
                "--scenario",
                "normal",
                "--no-modes",
                "--no-config-options",
                "--exit-on-config-write",
            ],
        )],
        FakeCredentials::ok(),
    );
    let endpoint = create(&host, GATE_SESSION, &collector).await;
    let agent = AgentId::new("agent-1").expect("id");
    // 未宣告模式：返回空结果（不编造候选），且 set_mode 显式拒绝。
    let modes = endpoint.modes().await.expect("modes");
    assert!(modes.available.is_empty(), "未宣告时不得编造候选模式");
    assert!(modes.current_mode.is_none());
    let mode_error = endpoint
        .set_mode(&acp_core::model::ModeId::new("plan").expect("mode"))
        .await;
    assert!(mode_error.is_err(), "未宣告模式时必须显式拒绝");
    // 未宣告配置项：列表为空，写入被拒绝。
    let config = endpoint.list_config().await.expect("config");
    assert!(config.is_empty(), "未宣告配置项时列表必须为空");
    let option = acp_core::model::ConfigOptionId::new("verbose").expect("id");
    let config_error = endpoint.set_config(&option, support::boolean(true)).await;
    assert!(config_error.is_err(), "未宣告配置项时必须显式拒绝");
    // 两次拒绝都没有发出任何请求：fake child 若收到就会 exit(7)。
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        runtime_running(&host, &agent),
        "拒绝路径不得向 Agent 发送消息（否则 fake child 会退出）"
    );
    drop(endpoint);
    host.shutdown_all().await;
}

/// 未登记的 content block：必须可见降级（结构化 payload 进 `block`），不是 null、也不是丢失。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_content_block_stays_structured() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "unknown-content-block"],
        )],
        FakeCredentials::ok(),
    );
    let endpoint = create(&host, BLOCK_SESSION, &collector).await;
    endpoint
        .prompt(prompt("未来块"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("agent.message.delta", Duration::from_secs(10))
            .await,
        "必须收到 delta 事件"
    );
    let view = collector.first_view("agent.message.delta").expect("delta");
    let view: Value = serde_json::from_str(&view).expect("view");
    let block = view.get("block").expect("非文本块必须带 block 字段");
    assert!(!block.is_null(), "block 不得被降成 null：{view}");
    assert_eq!(
        block.get("type").and_then(Value::as_str),
        Some("future_content_block")
    );
    assert_eq!(
        block
            .get("data")
            .and_then(|data| data.get("nested"))
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(3)
    );
    drop(endpoint);
    host.shutdown_all().await;
}

/// 已宣告能力时：模式与配置写入必须按 pinned schema 的形状发出（fake child 形状不合规即退出）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn declared_capabilities_accept_mode_and_config_writes() {
    let collector = Collector::new();
    let host = host(vec![profile("agent-1", FAKE_AGENT)], FakeCredentials::ok());
    let endpoint = create(&host, OTHER_SESSION, &collector).await;
    let agent = AgentId::new("agent-1").expect("id");
    // 成功路径：`{ sessionId, modeId }`。
    endpoint
        .set_mode(&acp_core::model::ModeId::new("plan").expect("mode"))
        .await
        .expect("set_mode 必须按 schema 形状发出并被接受");
    let modes = endpoint.modes().await.expect("modes");
    assert_eq!(
        modes
            .current_mode
            .as_ref()
            .map(|mode| mode.mode_id().as_str()),
        Some("plan"),
        "模式状态来自 Agent 的声明与本次写入"
    );
    // 成功路径：布尔取值必须写成 `{ type: boolean, value: bool }`。
    let option = acp_core::model::ConfigOptionId::new("verbose").expect("id");
    endpoint
        .set_config(&option, support::boolean(true))
        .await
        .expect("set_config 必须按 anyOf 形状发出并被接受");
    assert!(
        runtime_running(&host, &agent),
        "形状合规时进程不得退出（fake child 在形状不合规时会 exit(8)）"
    );
    drop(endpoint);
    host.shutdown_all().await;
}

/// `stderr-protocol-noise` 场景写进 stderr 的 marker，与 `src/bin/acpr-fake-acp-agent.rs` 的
/// `STDERR_NOISE_MARKER` 保持一致（bin 不能被集成测试导入，因此只能各写一份）。
const STDERR_NOISE_MARKER: &str = "ACPR-STDERR-PROTOCOL-NOISE-MARKER";

/// stderr 上的**语法完全合法**的 ACP 报文（通知 + 带 id 的响应）不得被当作协议输入：
/// 它们既不能变成事件，也不能破坏进程；stdout 上的正常应答必须照常完成 turn。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stderr_protocol_messages_never_reach_the_endpoint() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "stderr-protocol-noise"],
        )],
        FakeCredentials::ok(),
    );
    let endpoint = create(&host, OTHER_SESSION, &collector).await;
    // ① 请求本身成功（stderr 噪声不得拦住 stdout 上的正常路径）。
    endpoint
        .prompt(prompt("噪声"), support::timestamp())
        .await
        .expect("prompt 必须成功");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(10))
            .await,
        "turn 必须完成：{:?}",
        collector.event_types()
    );

    // ② stderr 上的报文不得在任何事件（公共视图或 ACP 原文）里出现。
    let events = collector.snapshot();
    for event in &events {
        let view = event.payload.view.as_str();
        assert!(
            !view.contains(STDERR_NOISE_MARKER),
            "stderr 内容不得进入事件视图：{view}"
        );
        if let Some((_, raw, _, _)) = event
            .payload
            .acp
            .as_ref()
            .and_then(acp_core::model::AcpRaw::as_available)
        {
            assert!(
                !raw.contains(STDERR_NOISE_MARKER),
                "stderr 内容不得进入 ACP 原文：{raw}"
            );
        }
    }

    // ③ 进程保持健康：后续请求仍然可用。
    let modes = endpoint.modes().await.expect("后续请求仍必须可用");
    assert!(
        !modes.available.is_empty(),
        "模式声明不得被 stderr 噪声破坏"
    );
    assert!(
        runtime_running(&host, &AgentId::new("agent-1").expect("id")),
        "stderr 上的报文不得让进程退出"
    );
    drop(endpoint);
    host.shutdown_all().await;
}

/// R5/R6：创建成功后后端把 `session/new` 给出的 ACP 会话标识原样交给 core（不自行读写存储、不编造）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_exposes_the_acp_session_id_verbatim() {
    let collector = Collector::new();
    let host = host(vec![profile("agent-1", FAKE_AGENT)], FakeCredentials::ok());
    let endpoint = create(&host, SESSION, &collector).await;
    assert_eq!(
        endpoint.agent_session_id().map(AgentSessionId::as_str),
        Some("acp-session-1"),
        "创建成功后必须暴露 Agent 给出的 ACP 会话标识"
    );
    drop(endpoint);
    host.shutdown_all().await;
}

/// R7：`session/new` 未成功完成时不得产生任何 ACP 会话标识。
///
/// 后端唯一的暴露口是端点，因此「没有端点」就是「没有标识」；同时断言失败的创建不留下可被 `open`
/// 复用的绑定（否则 core 会把一个没有标识的会话当作可恢复会话）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_session_new_yields_no_endpoint_and_no_identifier() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "session-new-error"],
        )],
        FakeCredentials::ok(),
    );
    let error = outcome_error(
        host.create(
            &session_id(SESSION),
            CreateSessionRequest::new(
                agent_ref(),
                Some(support::workspace()),
                None,
                ResourceOrigin::Local,
            ),
            collector.sink(),
        )
        .await,
    );
    assert!(
        matches!(error, PortError::InvalidRequest(_)),
        "Agent 拒绝 session/new 必须明确失败：{error}"
    );
    let opened = host
        .open(
            SessionReference::Owned(OwnedSessionRef::new(session_id(SESSION))),
            collector.sink(),
        )
        .await;
    assert!(
        opened.is_err(),
        "失败的创建不得留下端点（没有端点 = 没有标识可以交给 core）"
    );
    assert!(collector.is_empty(), "失败的创建不得产生事件");
    host.shutdown_all().await;
}

/// R8/R9/R26/R35：Agent 宣告 `sessionCapabilities.resume` 时，按持久化取值拉起进程、发送
/// `session/resume`，并把会话抬回可交互状态（端点接受 prompt 并完成 turn）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_restores_an_interactive_endpoint_when_the_capability_is_declared() {
    let collector = Collector::new();
    let dumps = TempFile::new("acpr-fake-acp-resume.requests");
    let dump_path = dumps.to_string_lossy().into_owned();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &[
                "--scenario",
                "resume-ok",
                "--capabilities",
                "{\"sessionCapabilities\":{\"resume\":{}}}",
                "--dump-requests",
                &dump_path,
            ],
        )],
        FakeCredentials::ok(),
    );
    let session = session_id(RESUME_SESSION);
    let endpoint = host
        .resume(
            &session,
            resume_request("acp-session-restored", &persisted_workspace_cwd()),
            collector.sink(),
        )
        .await
        .expect("宣告能力后恢复必须成功");
    assert_eq!(
        endpoint.agent_session_id().map(AgentSessionId::as_str),
        Some("acp-session-restored"),
        "恢复得到的端点必须读回持久化的 ACP 会话标识（不是新会话标识）"
    );
    assert_eq!(
        endpoint.reference().session_id().as_str(),
        RESUME_SESSION,
        "端点引用必须是 core 给的 SessionId"
    );

    endpoint
        .prompt(prompt("恢复之后"), support::timestamp())
        .await
        .expect("恢复后的端点必须接受 prompt");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(10))
            .await,
        "恢复后的 turn 必须完成：{:?}",
        collector.event_types()
    );

    let methods = dumped_methods(&dumps);
    assert!(
        methods.iter().any(|method| method == "session/resume"),
        "必须发送 session/resume：{methods:?}"
    );
    assert!(
        !methods.iter().any(|method| method == "session/new"),
        "恢复不得静默改走新建会话：{methods:?}"
    );
    drop(endpoint);
    host.shutdown_all().await;
}

/// R10/R37：能力未宣告（字段省略）时恢复入口显式返回「后端不支持」：**不发** `session/resume`，
/// 并在返回前回收本次为恢复而拉起的子进程（进程外证据：心跳文件不再增长）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn undeclared_resume_capability_is_refused_without_sending_the_request() {
    let collector = Collector::new();
    let dumps = TempFile::new("acpr-fake-acp-undeclared-resume.requests");
    let heartbeat = TempFile::new("acpr-fake-acp-undeclared-resume.heartbeat");
    let dump_path = dumps.to_string_lossy().into_owned();
    let heartbeat_path = heartbeat.to_string_lossy().into_owned();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &[
                "--scenario",
                "normal",
                "--heartbeat-file",
                &heartbeat_path,
                "--dump-requests",
                &dump_path,
            ],
        )],
        FakeCredentials::ok(),
    );
    let error = outcome_error(
        host.resume(
            &session_id(RESUME_REFUSED_SESSION),
            resume_request("acp-session-undeclared", &persisted_workspace_cwd()),
            collector.sink(),
        )
        .await,
    );
    assert!(
        matches!(
            error,
            PortError::Unavailable(UnavailableKind::BackendUnsupported)
        ),
        "能力未宣告必须表达为「后端不支持」：{error}"
    );

    // ① 一个字节都没发：Agent 只看到过 initialize。
    let methods = dumped_methods(&dumps);
    assert!(
        methods.iter().any(|method| method == "initialize"),
        "恢复前必须先协商能力：{methods:?}"
    );
    assert!(
        !methods.iter().any(|method| method == "session/resume"),
        "能力未宣告时不得发送 session/resume：{methods:?}"
    );

    // ② 本次拉起的子进程已被回收（进程外证据，不依赖进程内的状态位）。
    assert!(
        wait_for_file(&heartbeat, Duration::from_secs(2)).await,
        "子进程应在被回收前留下心跳文件"
    );
    let size = file_len(&heartbeat);
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        file_len(&heartbeat),
        size,
        "恢复失败后不得残留子进程（心跳仍在增长）"
    );
    assert!(
        !runtime_running(&host, &agent_id()),
        "运行时目录里不得留下本次为恢复拉起的进程"
    );

    // ③ 没有留下任何绑定。
    assert!(
        host.open(
            SessionReference::Owned(OwnedSessionRef::new(session_id(RESUME_REFUSED_SESSION))),
            collector.sink(),
        )
        .await
        .is_err(),
        "失败的恢复不得留下端点"
    );
    assert!(collector.is_empty());
    host.shutdown_all().await;
}

/// R11：Agent 宣告了能力却拒绝恢复——明确失败、不伪造成功、不改走新建、不留进程。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_refusal_of_resume_fails_explicitly() {
    let collector = Collector::new();
    let dumps = TempFile::new("acpr-fake-acp-resume-error.requests");
    let heartbeat = TempFile::new("acpr-fake-acp-resume-error.heartbeat");
    let dump_path = dumps.to_string_lossy().into_owned();
    let heartbeat_path = heartbeat.to_string_lossy().into_owned();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &[
                "--scenario",
                "resume-error",
                "--capabilities",
                "{\"sessionCapabilities\":{\"resume\":{}}}",
                "--heartbeat-file",
                &heartbeat_path,
                "--dump-requests",
                &dump_path,
            ],
        )],
        FakeCredentials::ok(),
    );
    let error = outcome_error(
        host.resume(
            &session_id(RESUME_REFUSED_SESSION),
            resume_request("acp-session-gone", &persisted_workspace_cwd()),
            collector.sink(),
        )
        .await,
    );
    assert!(
        matches!(error, PortError::InvalidRequest(_)),
        "Agent 拒绝恢复必须明确失败：{error}"
    );
    let methods = dumped_methods(&dumps);
    assert!(
        methods.iter().any(|method| method == "session/resume"),
        "宣告了能力才发送 session/resume：{methods:?}"
    );
    assert!(
        !methods.iter().any(|method| method == "session/new"),
        "拒绝后不得静默改走新建会话：{methods:?}"
    );
    assert!(
        host.open(
            SessionReference::Owned(OwnedSessionRef::new(session_id(RESUME_REFUSED_SESSION))),
            collector.sink(),
        )
        .await
        .is_err(),
        "失败的恢复不得留下端点"
    );
    assert!(collector.is_empty(), "失败的恢复不得产生事件");

    // 本次拉起的子进程已被回收。
    assert!(
        wait_for_file(&heartbeat, Duration::from_secs(2)).await,
        "子进程应在被回收前留下心跳文件"
    );
    let size = file_len(&heartbeat);
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(file_len(&heartbeat), size, "恢复失败后不得残留子进程");
    host.shutdown_all().await;
}

/// R12：反复恢复不产生第二个端点——旧绑定先让出（旧端点不再接受 turn），新端点可用。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repeated_resume_leaves_a_single_dispatching_binding() {
    let first = Collector::new();
    let second = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &[
                "--scenario",
                "resume-ok",
                "--capabilities",
                "{\"sessionCapabilities\":{\"resume\":{}}}",
            ],
        )],
        FakeCredentials::ok(),
    );
    let session = session_id(REPEATED_RESUME_SESSION);
    let request = resume_request("acp-session-restored", &persisted_workspace_cwd());
    let stale = host
        .resume(&session, request.clone(), first.sink())
        .await
        .expect("第一次恢复");
    let current = host
        .resume(&session, request, second.sink())
        .await
        .expect("第二次恢复");

    // 旧绑定已经让出：不得再派发 turn（否则同一会话会有两个端点并行派发）。
    let refused = outcome_error(stale.prompt(prompt("旧端点"), support::timestamp()).await);
    assert!(
        matches!(refused, PortError::InvalidRequest(_)),
        "旧绑定必须先让出：{refused}"
    );
    // 新端点照常可用。
    current
        .prompt(prompt("新端点"), support::timestamp())
        .await
        .expect("新端点必须可派发");
    assert!(
        second
            .wait_for_type("turn.completed", Duration::from_secs(10))
            .await,
        "恢复后的 turn 必须完成：{:?}",
        second.event_types()
    );
    assert_eq!(first.count("turn.completed"), 0, "旧绑定不得再产生事件");
    drop(stale);
    drop(current);
    host.shutdown_all().await;
}

// -------------------------------------------------------------------------------------------
// R8 / D6：profile 进程的连接生命周期上报（节点级 `agent.connected` / `agent.disconnected`）
// -------------------------------------------------------------------------------------------

/// 一个带节点级事件收集器的 host；收集器的 sink 在现场回读「运行时是否仍被当作在运行」。
fn host_with_node_events(
    profiles: Vec<AgentProfile>,
    credentials: FakeCredentials,
) -> (Arc<AgentHost>, Collector) {
    let node = Collector::new();
    let host = Arc::new(
        AgentHost::new(
            Arc::new(FakeConfig::new(profiles)),
            Arc::new(credentials),
            HostConfig::default(),
            Arc::new(TestIds::new()),
            Arc::new(TestClock::new()),
        )
        .with_node_events(agent_host::NodeEvents::new(node.node_sink())),
    );
    (host, node)
}

/// 等某个 `event_type` 的节点级事件出现（带超时）。
async fn wait_for_node_event(collector: &Collector, event_type: &str) -> bool {
    collector
        .wait_for_type(event_type, Duration::from_secs(10))
        .await
}

/// 首次建立时上报一次连接；同一进程被多个会话复用**不得**产生第二条连接。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn profile_process_reports_connect_once_and_reuse_adds_nothing() {
    let (host, node) =
        host_with_node_events(vec![profile("agent-1", FAKE_AGENT)], FakeCredentials::ok());

    let first = Collector::new();
    let _endpoint = create(&host, SESSION, &first).await;
    assert!(
        wait_for_node_event(&node, "agent.connected").await,
        "首个会话建立进程后必须上报一次连接"
    );
    assert_eq!(node.count("agent.connected"), 1);
    assert_eq!(node.count("agent.disconnected"), 0);
    let view: Value =
        serde_json::from_str(&node.first_view("agent.connected").expect("view")).expect("json");
    assert_eq!(view["agentId"], "agent-1");
    assert_eq!(view["state"], "connected", "封闭词表的唯一取值");
    assert_eq!(
        view.as_object().expect("object").len(),
        2,
        "节点级连接视图只有 agentId 与 state：{view}"
    );

    // 第二个、第三个会话复用同一进程：连接**不得**重复上报。
    let second = Collector::new();
    let _endpoint2 = create(&host, OTHER_SESSION, &second).await;
    let third = Collector::new();
    let _endpoint3 = create(&host, RESUME_SESSION, &third).await;
    assert!(runtime_running(&host, &agent_id()), "三个会话共用同一进程");
    assert_eq!(
        node.count("agent.connected"),
        1,
        "复用既有进程不得重复上报连接"
    );

    host.shutdown_all().await;
    assert!(
        wait_for_node_event(&node, "agent.disconnected").await,
        "进程退出时必须上报断开"
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(node.count("agent.disconnected"), 1, "断开恰好一次");
}

/// `agent_capabilities` 触发的首次 spawn 同样产生一次连接（进程确实可服务会话即可观察），
/// 后续复用不产生第二条。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capability_probe_spawn_reports_connect_once() {
    let (host, node) = host_with_node_events(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &[
                "--scenario",
                "normal",
                "--capabilities",
                r#"{"loadSession":true}"#,
            ],
        )],
        FakeCredentials::ok(),
    );
    let _ = host.agent_capabilities(&agent_ref()).await;
    assert!(wait_for_node_event(&node, "agent.connected").await);
    let _ = host.agent_capabilities(&agent_ref()).await;
    assert_eq!(node.count("agent.connected"), 1, "复用不重复上报");
    host.shutdown_all().await;
}

/// 空闲回收终止进程时同样上报一次断开。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_reclaim_reports_disconnect_once() {
    let (host, node) =
        host_with_node_events(vec![profile("agent-1", FAKE_AGENT)], FakeCredentials::ok());
    let collector = Collector::new();
    let _endpoint = create(&host, SESSION, &collector).await;
    assert!(wait_for_node_event(&node, "agent.connected").await);
    assert_eq!(node.count("agent.disconnected"), 0);

    // 空闲时长为 0 的超时下，一次扫描即命中（会话无进行中的 turn）。
    host.sweep_idle(Duration::from_nanos(1)).await;
    assert!(
        wait_for_node_event(&node, "agent.disconnected").await,
        "被空闲回收终止的进程必须上报断开"
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(node.count("agent.disconnected"), 1, "断开恰好一次");
    host.shutdown_all().await;
}

/// 自然退出（Agent 自己结束）同样上报断开，且恰好一次。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn natural_exit_reports_disconnect_once() {
    let (host, node) = host_with_node_events(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "crash-on-prompt"],
        )],
        FakeCredentials::ok(),
    );
    let collector = Collector::new();
    let endpoint = create(&host, SESSION, &collector).await;
    assert!(wait_for_node_event(&node, "agent.connected").await);

    // `crash-on-prompt`：Agent 在 prompt 期间自己退出（不是被我们结束）。
    let _ = endpoint
        .prompt(
            acp_core::model::PromptRequest::new(vec![
                PromptContentBlock::from_json_text("{\"type\":\"text\",\"text\":\"崩溃\"}")
                    .expect("block"),
            ]),
            support::timestamp(),
        )
        .await;
    assert!(
        wait_for_node_event(&node, "agent.disconnected").await,
        "进程自然退出必须上报断开"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(node.count("agent.disconnected"), 1, "断开恰好一次");
    let view: Value =
        serde_json::from_str(&node.first_view("agent.disconnected").expect("view")).expect("json");
    assert_eq!(view["state"], "disconnected");
    assert_eq!(view["error"]["code"], "internal.unavailable");
    host.shutdown_all().await;
}

/// 超限退出：断开上报**不早于**该运行时被判定为已退出。
///
/// 判据在现场采集：节点级事件的 sink 是**同步**交付路径，断开事件到达的那一刻读到的
/// `runtime_running(...)` 必须不是 `true`（`true` 就是 spec 禁止的「已断开但仍被当作存活」窗口）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversize_exit_reports_disconnect_after_the_exit_verdict() {
    let (host, node) = host_with_node_events(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "huge-line"],
        )],
        FakeCredentials::ok(),
    );
    let collector = Collector::new();
    let endpoint = create(&host, SESSION, &collector).await;
    assert!(
        wait_for_node_event(&node, "agent.connected").await,
        "先有连接"
    );

    // 这里把节点级收集器的 sink 换成「现场回读」版本：同一条同步调用链上读运行时状态。
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let probe = {
        let observed = Arc::clone(&observed);
        let host = Arc::clone(&host);
        agent_host::NodeEvents::new(acp_core::ports::NodeEventSink::new(move |event| {
            if event.event_type.as_str() == "agent.disconnected" {
                let still_running = runtime_running(&host, &agent_id());
                if let Ok(mut guard) = observed.lock() {
                    guard.push(still_running);
                }
            }
        }))
    };
    host.set_node_events(probe);

    let _ = endpoint
        .prompt(prompt("触发超限"), support::timestamp())
        .await
        .ok();

    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while observed.lock().expect("lock").is_empty() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let observed = observed.lock().expect("lock").clone();
    assert_eq!(observed.len(), 1, "超限退出只上报一次断开：{observed:?}");
    assert!(
        !observed[0],
        "断开上报时该运行时不得仍被当作在运行：{observed:?}"
    );
    assert!(!runtime_running(&host, &agent_id()), "超限后运行时已退出");
    host.shutdown_all().await;
}

/// 未拒绝 `initialize` 的进程（协商失败即被回收）**不上报**连接，因此也不上报断开。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_initialize_reports_neither_connect_nor_disconnect() {
    let (host, node) = host_with_node_events(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "initialize-error"],
        )],
        FakeCredentials::ok(),
    );
    let collector = Collector::new();
    let result = host
        .create(
            &session_id(SESSION),
            CreateSessionRequest::new(
                agent_ref(),
                Some(support::workspace()),
                None,
                ResourceOrigin::Local,
            ),
            collector.sink(),
        )
        .await;
    assert!(result.is_err(), "协商失败必须显式失败");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        node.count("agent.connected"),
        0,
        "从未可服务会话的进程不上报连接"
    );
    assert_eq!(node.count("agent.disconnected"), 0, "因此也不上报断开");
    host.shutdown_all().await;
}

/// F1：**未接线**的节点级出口不是空操作。
///
/// 组合根漏接 `NodeEvents` 时，事件必须在开发期/运行期被看见，而不是在 core 之前被无声丢弃：
/// - `node_events_bound()` 为假（接线缺失在进程内可查询）；
/// - `NodeEvents::send` 返回 [`NodeEventError::Unbound`]（开发期可见的机器判据）；
/// - 接线后（`set_node_events`）同一个实例即生效，交付返回 `Ok`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unbound_node_events_are_visible_not_silently_dropped() {
    // 默认构造 = 未接线。
    let host = host(vec![profile("agent-1", FAKE_AGENT)], FakeCredentials::ok());
    assert!(
        !host.node_events_bound(),
        "默认构造的 AgentHost 未接线节点级出口"
    );

    // 未接线的出口明确拒绝（而不是返回成功）：这就是「漏接被看见」的判据。
    let unbound = agent_host::NodeEvents::unbound();
    assert!(!unbound.is_bound());
    assert_eq!(
        unbound.send(
            agent_host::node::connected_event(&agent_id(), support::timestamp())
                .expect("connected event")
        ),
        Err(NodeEventError::Unbound),
        "未接线的出口必须拒绝，而不是静默丢弃"
    );

    // 接线之后即可交付：同一个实例（`set_node_events`）在一处接线即全局生效。
    let node = Collector::new();
    host.set_node_events(agent_host::NodeEvents::new(node.node_sink()));
    assert!(host.node_events_bound(), "接线后必须可见");
    host.shutdown_all().await;
}

// -------------------------------------------------------------------------------------------
// R22 / D4：工具调用里的类型化 Diff 元素随工具调用一并交付（逐字节原文）
// -------------------------------------------------------------------------------------------

/// 含 Diff 的工具调用：三个 `tool.call.*` 事件的 view 带 `diff` 数组，元素是**原文子串** +
/// 按 JSON 语义解码的 `path`/`oldText`/`newText`；ACP 原文三要素逐字不变。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typed_diff_elements_are_delivered_with_the_tool_call() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "chunked-updates"],
        )],
        FakeCredentials::ok(),
    );
    let endpoint = create(&host, SESSION, &collector).await;
    endpoint
        .prompt(prompt("编辑"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("tool.call.started", Duration::from_secs(10))
            .await,
        "工具调用必须到达：{:?}",
        collector.event_types()
    );

    for event in collector.snapshot() {
        let event_type = event.event_type.as_str().to_owned();
        if !event_type.starts_with("tool.call.") {
            continue;
        }
        let view: Value =
            serde_json::from_str(event.payload.view.as_str()).expect("view 是 JSON 对象");
        // 原文三要素必须可读（本用例只断言存在性；保真断言在下面按内容比对）。
        assert!(
            event.payload.acp.is_some(),
            "{event_type}: 工具调用必须携带 ACP 原文"
        );
        let Some(diff) = view.get("diff") else {
            // `tool.call.updated`/`completed` 在 fake 场景里只带普通内容块：这时 diff 键必须**缺席**
            // （不是 null、不是空数组），因为「不含 Diff 的工具调用」在视图层就是不可派生的。
            assert!(
                !event_type.is_empty(),
                "diff 键缺席是合法的（该次更新的内容里没有 Diff 元素）"
            );
            continue;
        };
        let elements = diff.as_array().expect("diff 恒为数组");
        assert!(!elements.is_empty(), "diff 键出现时至少一个元素");
        let raw_json = event
            .payload
            .acp
            .as_ref()
            .and_then(|acp| acp.as_available().map(|(_, raw, _, _)| raw.to_owned()))
            .expect("原文");
        for element in elements {
            let slice = element["raw"].as_str().expect("raw 切片");
            assert!(
                raw_json.contains(slice),
                "{event_type}: diff 元素的 raw 必须是 ACP **原文**的子串（逐字节）"
            );
            assert_eq!(element["path"], "src/main.rs");
            assert_eq!(element["oldText"], "let a = 1;");
            assert_eq!(element["newText"], "let a = 2;");
        }
    }

    // `tool.call.started` 的内容里确实有 Diff 元素：它必须带 `diff` 键（防空转守卫）。
    let started: Value = serde_json::from_str(
        &collector
            .first_view("tool.call.started")
            .expect("tool.call.started view"),
    )
    .expect("view");
    assert!(
        started.get("diff").is_some(),
        "含 Diff 元素的工具调用必须带 diff 键：{started}"
    );
    assert!(
        started.get("rawInput").is_none(),
        "适配器不把 rawInput 投影进视图：{started}"
    );

    drop(endpoint);
    host.shutdown_all().await;
}

/// Diff 逐字节：`raw` 切片与原文的字节偏移关系（含 `RawDocument` 的 `member_literal` 口径），
/// 且工具调用的 `payload.acp` 摘要与原文一致（三要素之一）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diff_slice_is_byte_identical_and_acp_raw_matches_its_digest() {
    let collector = Collector::new();
    let host = host(
        vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", "chunked-updates"],
        )],
        FakeCredentials::ok(),
    );
    let endpoint = create(&host, SESSION, &collector).await;
    endpoint
        .prompt(prompt("编辑"), support::timestamp())
        .await
        .expect("prompt");
    assert!(
        collector
            .wait_for_type("tool.call.started", Duration::from_secs(10))
            .await
    );

    let event = collector
        .snapshot()
        .into_iter()
        .find(|event| event.event_type.as_str() == "tool.call.started")
        .expect("tool.call.started");
    let (media_type, raw_json, byte_length, sha256) = event
        .payload
        .acp
        .as_ref()
        .and_then(|acp| acp.as_available())
        .expect("原文");
    assert_eq!(media_type, "application/json");
    assert_eq!(
        byte_length as usize,
        raw_json.len(),
        "byteLength 与原文逐字节一致"
    );
    assert_eq!(
        sha256.as_str(),
        digest_of(raw_json).as_str(),
        "sha256 必须与原文逐字节对应"
    );

    let view: Value = serde_json::from_str(event.payload.view.as_str()).expect("view");
    let element = &view["diff"][0];
    let slice = element["raw"].as_str().expect("raw");
    let offset = raw_json.find(slice).expect("raw 必须在原文里");
    assert_eq!(
        &raw_json[offset..offset + slice.len()],
        slice,
        "raw 是原文的逐字节子串"
    );
    // 这四个字段都是本次交付的载体；缺任何一个都会让 WP3 的派生失去输入。
    for field in ["path", "oldText", "newText", "raw"] {
        assert!(
            element.get(field).is_some(),
            "diff 元素必须带 {field}：{element}"
        );
    }

    drop(endpoint);
    host.shutdown_all().await;
}
