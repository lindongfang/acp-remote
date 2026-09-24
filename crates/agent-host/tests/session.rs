//! 会话端点：事件映射与保真、turn 生命周期、交互往返、能力门控与映射表。

mod support;

use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{
    AgentId, AgentProfile, AgentRef, CreateSessionRequest, ExportId, InteractionResolution, NodeId,
    OriginEpoch, PermissionDecision, PermissionDecisionKind, PortError, PromptContentBlock,
    RemoteSessionRef, ResourceOrigin, SessionId, SessionReference,
};
use acp_core::ports::{AgentCatalog, SessionBackendFactory, SessionEndpoint};
use agent_host::runtime_running;
use agent_host::{AgentHost, HostConfig};
use serde_json::Value;
use support::{
    Collector, FAKE_AGENT, FakeConfig, FakeCredentials, TestClock, TestIds, digest_of, profile,
    profile_with,
};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const CRASH_SESSION: &str = "66666666-6666-4666-8666-666666666666";
const GATE_SESSION: &str = "77777777-7777-4777-8777-777777777777";
const BLOCK_SESSION: &str = "88888888-8888-4888-8888-888888888888";
const OTHER_SESSION: &str = "22222222-2222-4222-8222-222222222222";

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
