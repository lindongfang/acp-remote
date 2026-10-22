//! WP4：临时（准备）端点、真实选项、代隔离、释放与首次派发所有权转移。
//!
//! 所有场景都走**真实 stdio 子进程** `acpr-fake-acp-agent`（`--scenario` / `--dump-request-params`）。
//! 断言绑定四类可观察事实：
//!
//! - `session/new` 的响应（真实模式/配置，默认值来自 Agent）；
//! - `--dump-request-params` 的每一行（准备期是否发过 prompt、`session/close` 是否真的发出）；
//! - `release_preparation` 的三态结果与 `session_resources`（未确认释放时资源仍占用）；
//! - 首条派发的回显关联（`design.md` D7）：可证明重复的回显不产生第二条用户消息事件，且事件视图
//!   **没有**任何派发上下文字段（上下文只在适配器内部使用）。

mod support;

use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{
    AgentId, AgentProfile, AgentRef, ConfigOptionId, ConfigOptionKind, ConfigValue,
    CreateSessionRequest, DispatchContext, FirstSubmissionId, MessageId, ModeId, PortError,
    PromptContentBlock, PromptRequest, RequestId, ResourceGeneration, ResourceOrigin, SessionId,
};
use acp_core::ports::SessionEndpoint;
use agent_host::{
    AgentHost, HostConfig, PreparationConfigChoice, PreparationSelection, ReleaseOutcome,
    runtime_running,
};
use serde_json::{Value, json};
use support::{
    Collector, FAKE_AGENT, TempFile, TestClock, TestIds, digest_of, dump_count, profile_with,
    timestamp,
};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const REBUILD_SESSION: &str = "22222222-2222-4222-8222-222222222222";
const LATE_SESSION: &str = "33333333-3333-4333-8333-333333333333";
const REFUSED_SESSION: &str = "44444444-4444-4444-8444-444444444444";
const RELEASED_SESSION: &str = "55555555-5555-4555-8555-555555555555";
const ECHO_SESSION: &str = "66666666-6666-4666-8666-666666666666";
const TREE_SESSION: &str = "77777777-7777-4777-8777-777777777777";
const OWNED_SESSION: &str = "88888888-8888-4888-8888-888888888888";
const CLOSE_CAPABILITY: &str = r#"{"sessionCapabilities":{"close":{}}}"#;

fn host(profiles: Vec<AgentProfile>) -> Arc<AgentHost> {
    Arc::new(AgentHost::new(
        Arc::new(profiles),
        HostConfig::default(),
        Arc::new(TestIds::new()),
        Arc::new(TestClock::new()),
    ))
}

fn agent_id() -> AgentId {
    AgentId::new("agent-1").expect("agent id")
}

fn agent_ref() -> AgentRef {
    AgentRef::try_new(agent_id(), "Agent agent-1").expect("agent ref")
}

fn session_id(text: &str) -> SessionId {
    SessionId::new(text).expect("session id")
}

fn request() -> CreateSessionRequest {
    CreateSessionRequest::new(
        agent_ref(),
        Some(support::workspace()),
        None,
        ResourceOrigin::Local,
    )
}

fn prompt(text: &str) -> PromptRequest {
    PromptRequest::new(vec![
        PromptContentBlock::from_json_text(&format!(r#"{{"type":"text","text":"{text}"}}"#))
            .expect("内容块"),
    ])
}

/// `dyn` 端点的错误没有 `Debug` 约束时不能用 `expect_err`。
fn outcome_error<T>(result: Result<T, PortError>) -> PortError {
    match result {
        Ok(_) => panic!("期望失败，但得到了成功结果"),
        Err(error) => error,
    }
}

fn profile(scenario: &str, extra: &[&str]) -> AgentProfile {
    let mut args = vec!["--scenario", scenario];
    args.extend_from_slice(extra);
    profile_with("agent-1", FAKE_AGENT, &args)
}

/// 等文件出现（fake 子进程异步写心跳）。
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

fn file_len(path: &std::path::Path) -> usize {
    std::fs::metadata(path)
        .map(|meta| meta.len() as usize)
        .unwrap_or(0)
}

/// 等某个 `event_type` 的条数达到 `count`（`Collector` 只有「总条数」与「出现过」两种等待）。
async fn wait_for_count(
    collector: &Collector,
    event_type: &str,
    count: usize,
    timeout: Duration,
) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while collector.count(event_type) < count && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    collector.count(event_type) >= count
}

/// 准备端点读到的就是 Agent `session/new` 返回的真实选项；准备期不发送 prompt。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preparation_exposes_real_agent_options_without_sending_prompt() {
    let dump = TempFile::new("acpr-preparation-options.jsonl");
    let _ = std::fs::remove_file(&dump);
    let host = host(vec![profile(
        "normal",
        &["--dump-request-params", &dump.to_string_lossy()],
    )]);
    let collector = Collector::new();
    let endpoint = host
        .prepare(&session_id(SESSION), request(), collector.sink())
        .await
        .expect("prepare");

    // 模式：当前值与候选都来自 Agent（不是本层编造）。
    let modes = endpoint.modes().await.expect("modes");
    let current = modes.current_mode.as_ref().expect("current mode");
    assert_eq!(current.mode_id().as_str(), "default");
    assert_eq!(current.display_name(), "Default");
    let available: Vec<&str> = modes
        .available
        .iter()
        .map(|mode| mode.mode_id().as_str())
        .collect();
    assert_eq!(available, ["default", "plan"]);

    // 配置：默认值就是 Agent 的值（verbose=false、model=small）。
    let config = endpoint.list_config().await.expect("config");
    let verbose = config
        .iter()
        .find(|option| option.id().as_str() == "verbose")
        .expect("verbose");
    assert_eq!(verbose.kind(), ConfigOptionKind::Boolean);
    assert_eq!(verbose.current(), &ConfigValue::boolean(false));
    let model = config
        .iter()
        .find(|option| option.id().as_str() == "model")
        .expect("model");
    assert_eq!(model.kind(), ConfigOptionKind::Select);
    assert_eq!(
        model.current(),
        &ConfigValue::select("small").expect("select")
    );

    // 空选择 = 沿用 Agent 默认值，并通过复核（可转正）。
    let report = endpoint
        .apply_selection(&PreparationSelection::default())
        .await
        .expect("复核默认值");
    assert!(!report.blocks_dispatch());
    assert_eq!(
        report
            .modes
            .current_mode
            .as_ref()
            .expect("mode")
            .mode_id()
            .as_str(),
        "default"
    );

    let rows = support::wait_for_dumped_method(&dump, "session/new", Duration::from_secs(5)).await;
    assert_eq!(
        dump_count(&rows, "session/prompt"),
        0,
        "准备期不得发送任何 prompt（包括空消息）"
    );
    assert_eq!(dump_count(&rows, "session/new"), 1);
    host.shutdown_all().await;
}

/// 请求的模式/配置已不可用时保留在报告里可见，并阻止转正与派发（不静默回退）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unavailable_selection_stays_visible_and_blocks_dispatch() {
    let host = host(vec![profile("normal", &[])]);
    let collector = Collector::new();
    let endpoint = host
        .prepare(&session_id(SESSION), request(), collector.sink())
        .await
        .expect("prepare");

    let selection = PreparationSelection::new(
        Some(ModeId::new("not-a-mode").expect("mode id")),
        vec![
            PreparationConfigChoice::new(
                ConfigOptionId::new("verbose").expect("config id"),
                ConfigValue::boolean(true),
            ),
            PreparationConfigChoice::new(
                ConfigOptionId::new("missing").expect("config id"),
                ConfigValue::boolean(true),
            ),
        ],
    );
    let report = endpoint.apply_selection(&selection).await.expect("复核");
    assert_eq!(
        report.unavailable_mode.as_ref().map(ModeId::as_str),
        Some("not-a-mode")
    );
    assert_eq!(
        report
            .unavailable_config
            .iter()
            .map(ConfigOptionId::as_str)
            .collect::<Vec<_>>(),
        ["missing"]
    );
    assert_eq!(
        report
            .applied_config
            .iter()
            .map(ConfigOptionId::as_str)
            .collect::<Vec<_>>(),
        ["verbose"]
    );
    // 真实可用值仍可见（不可用项没有被默认值顶替）。
    assert_eq!(report.modes.available.len(), 2);
    let verbose = report
        .config
        .iter()
        .find(|option| option.id().as_str() == "verbose")
        .expect("verbose");
    assert_eq!(verbose.current(), &ConfigValue::boolean(true));
    assert!(report.blocks_dispatch());

    // 阻止转正与派发。
    assert!(host.transfer_preparation(&endpoint).await.is_err());
    let error = outcome_error(endpoint.prompt(prompt("hello"), timestamp()).await);
    assert!(matches!(error, PortError::InvalidRequest(_)), "{error}");
    host.shutdown_all().await;
}

/// 重建到新资源代后，选项消失时仍可见为空、选择不可用并被阻止；旧代不能释放新代。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rebuild_isolates_generations_and_detects_disappearing_options() {
    let host = host(vec![profile(
        "rebuild-options",
        &["--capabilities", CLOSE_CAPABILITY],
    )]);
    let collector = Collector::new();
    let first = host
        .prepare(&session_id(REBUILD_SESSION), request(), collector.sink())
        .await
        .expect("prepare");
    first
        .apply_selection(&PreparationSelection::default())
        .await
        .expect("默认值复核");

    let second = host
        .rebuild_preparation(&session_id(REBUILD_SESSION), request(), collector.sink())
        .await
        .expect("rebuild");
    assert!(
        second.generation() > first.generation(),
        "重建必须分配新的资源代：{} -> {}",
        first.generation(),
        second.generation()
    );

    // 第二次 `session/new` 没有宣告任何模式/配置：报告如实为空，不伪造默认值。
    let modes = second.modes().await.expect("modes");
    assert!(modes.available.is_empty());
    assert!(second.list_config().await.expect("config").is_empty());

    let selection = PreparationSelection::new(
        Some(ModeId::new("plan").expect("mode id")),
        vec![PreparationConfigChoice::new(
            ConfigOptionId::new("model").expect("config id"),
            ConfigValue::select("large").expect("select"),
        )],
    );
    let report = second.apply_selection(&selection).await.expect("复核");
    assert!(report.blocks_dispatch());
    assert_eq!(
        report.unavailable_mode.as_ref().map(ModeId::as_str),
        Some("plan")
    );
    assert_eq!(
        report
            .unavailable_config
            .iter()
            .map(ConfigOptionId::as_str)
            .collect::<Vec<_>>(),
        ["model"]
    );
    assert!(host.transfer_preparation(&second).await.is_err());

    // 重建已经真实关闭旧代（见下一条用例）；这里只确认幂等，且不会作用于新代。
    assert_eq!(
        host.release_preparation(&first).await,
        ReleaseOutcome::Released,
        "旧代在重建时已被 Agent ack 释放，重复释放幂等成功"
    );
    assert!(!second.is_released());
    assert!(second.modes().await.is_ok(), "新代绑定必须仍然可用");

    // 精确代释放：Agent 确认后才是 Released，并可从资源事实看到已释放。
    assert_eq!(
        host.release_preparation(&second).await,
        ReleaseOutcome::Released
    );
    assert!(second.is_released());
    host.shutdown_all().await;
}

/// 重建必须在旧代仍是当前映射时真实关闭它（`session/close`），不能只摘内存映射。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rebuild_closes_the_previous_agent_session() {
    let dump = TempFile::new("acpr-preparation-rebuild-close.jsonl");
    let _ = std::fs::remove_file(&dump);
    let host = host(vec![profile(
        "rebuild-options",
        &[
            "--capabilities",
            CLOSE_CAPABILITY,
            "--dump-request-params",
            &dump.to_string_lossy(),
        ],
    )]);
    let collector = Collector::new();
    let first = host
        .prepare(&session_id(REBUILD_SESSION), request(), collector.sink())
        .await
        .expect("prepare");
    first
        .apply_selection(&PreparationSelection::default())
        .await
        .expect("默认值复核");
    let first_agent_session = first
        .agent_session_id()
        .expect("真实 session/new 会话标识")
        .clone();

    let second = host
        .rebuild_preparation(&session_id(REBUILD_SESSION), request(), collector.sink())
        .await
        .expect("rebuild");
    assert!(first.is_released(), "旧代必须被 Agent ack 释放");

    // 旧代的 `session/close` 必须在换入新代之前发出，且参数指向旧代的 ACP 会话。
    let rows =
        support::wait_for_dumped_method(&dump, "session/close", Duration::from_secs(5)).await;
    assert_eq!(dump_count(&rows, "session/new"), 2, "两次真实 session/new");
    assert_eq!(dump_count(&rows, "session/close"), 1, "旧代必须被真实关闭");
    let close = rows
        .iter()
        .find(|row| row["method"] == json!("session/close"))
        .expect("必须出现 session/close");
    assert_eq!(
        close["params"]["sessionId"],
        json!(first_agent_session.as_str()),
        "关闭的必须是旧代的 Agent 会话"
    );

    assert_eq!(
        host.release_preparation(&first).await,
        ReleaseOutcome::Released,
        "重建已释放旧代，重复释放幂等成功"
    );
    assert!(second.modes().await.is_ok(), "新代绑定仍然可用");
    host.shutdown_all().await;
}

/// 已转正的绑定不能被准备重建替换：拒绝必须发生在**任何** `session/new` 之前。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rebuild_rejects_an_owned_binding_without_a_new_session() {
    let dump = TempFile::new("acpr-preparation-owned-reject.jsonl");
    let _ = std::fs::remove_file(&dump);
    let host = host(vec![profile(
        "normal",
        &[
            "--capabilities",
            CLOSE_CAPABILITY,
            "--dump-request-params",
            &dump.to_string_lossy(),
        ],
    )]);
    let collector = Collector::new();
    let endpoint = host
        .prepare(&session_id(OWNED_SESSION), request(), collector.sink())
        .await
        .expect("prepare");
    endpoint
        .apply_selection(&PreparationSelection::default())
        .await
        .expect("默认值复核");
    host.transfer_preparation(&endpoint).await.expect("转正");
    assert!(!endpoint.is_temporary());
    let rows = support::wait_for_dumped_method(&dump, "session/new", Duration::from_secs(5)).await;
    assert_eq!(dump_count(&rows, "session/new"), 1);

    let error = outcome_error(
        host.rebuild_preparation(&session_id(OWNED_SESSION), request(), collector.sink())
            .await,
    );
    assert!(
        matches!(error, PortError::Conflict(_)),
        "已转正的会话必须被拒绝：{error}"
    );
    // 拒绝是本地可判定的：没有任何新的 `session/new`，也没有关闭正式会话。
    let rows = support::dumped_requests(&dump);
    assert_eq!(
        dump_count(&rows, "session/new"),
        1,
        "拒绝路径不得建立新的 Agent 会话"
    );
    assert_eq!(dump_count(&rows, "session/close"), 0);
    assert!(
        endpoint.modes().await.is_ok(),
        "被拒绝的重建不得影响正式绑定"
    );
    host.shutdown_all().await;
}

/// 旧代迟到的 `set_mode` 结果（新代建立之后才到达）不得覆盖新代的选择。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn late_old_generation_response_does_not_overwrite_new_selection() {
    let dump = TempFile::new("acpr-preparation-late.jsonl");
    let _ = std::fs::remove_file(&dump);
    let host = host(vec![profile(
        "hold-set-mode",
        &[
            "--capabilities",
            CLOSE_CAPABILITY,
            "--dump-request-params",
            &dump.to_string_lossy(),
        ],
    )]);
    let collector = Collector::new();
    let first = Arc::new(
        host.prepare(&session_id(LATE_SESSION), request(), collector.sink())
            .await
            .expect("prepare"),
    );
    first
        .apply_selection(&PreparationSelection::default())
        .await
        .expect("默认值复核");

    let plan = ModeId::new("plan").expect("mode id");
    let pending = {
        let first = Arc::clone(&first);
        tokio::spawn(async move { first.set_mode(&plan).await })
    };
    // fake 收到请求后暂缓应答，直到重建的 `session/new` 之后才写出。
    let rows =
        support::wait_for_dumped_method(&dump, "session/set_mode", Duration::from_secs(5)).await;
    assert_eq!(dump_count(&rows, "session/set_mode"), 1);

    let second = host
        .rebuild_preparation(&session_id(LATE_SESSION), request(), collector.sink())
        .await
        .expect("rebuild");
    // 旧代的应答在新代建立后才被写出并被处理。
    tokio::time::timeout(Duration::from_secs(5), pending)
        .await
        .expect("旧代 set_mode 必须在有界时间内结束")
        .expect("task")
        .expect("旧代 set_mode 仍可完成（它作用于旧代）");

    let modes = second.modes().await.expect("新代模式");
    assert_eq!(
        modes
            .current_mode
            .as_ref()
            .expect("current")
            .mode_id()
            .as_str(),
        "default",
        "旧代的迟到结果不得改写新代的选择"
    );
    // 旧代在重建时已被真实关闭：再次释放只走幂等路径，不作用于新代。
    assert_eq!(
        host.release_preparation(&first).await,
        ReleaseOutcome::Released,
        "旧代不能释放新代，但它自己已被 Agent ack 释放"
    );
    host.shutdown_all().await;
}

/// 关闭被 Agent 拒绝时资源保持占用；未知/旧代句柄也不能“假装释放成功”。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refused_close_keeps_the_preparation_resource_occupied() {
    let host = host(vec![profile(
        "close-error",
        &["--capabilities", CLOSE_CAPABILITY],
    )]);
    let collector = Collector::new();
    let endpoint = host
        .prepare(&session_id(REFUSED_SESSION), request(), collector.sink())
        .await
        .expect("prepare");
    endpoint
        .apply_selection(&PreparationSelection::default())
        .await
        .expect("复核");

    assert_eq!(
        host.release_preparation(&endpoint).await,
        ReleaseOutcome::Failed
    );
    assert!(!endpoint.is_released());
    // 绑定仍然可服务，且资源事实仍报告占用（临时标记可见）。
    assert!(endpoint.modes().await.is_ok());
    let facts = host.session_resources().await;
    let fact = facts
        .iter()
        .find(|fact| fact.session.as_str() == REFUSED_SESSION)
        .expect("绑定仍存在");
    assert!(fact.temporary);
    assert!(!fact.closed);
    host.shutdown_all().await;
}

/// 已确认释放的端点是幂等的；释放后不再出现在资源事实里。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn released_preparation_is_idempotent_and_leaves_command() {
    let host = host(vec![profile(
        "normal",
        &["--capabilities", CLOSE_CAPABILITY],
    )]);
    let collector = Collector::new();
    let endpoint = host
        .prepare(&session_id(RELEASED_SESSION), request(), collector.sink())
        .await
        .expect("prepare");
    endpoint
        .apply_selection(&PreparationSelection::default())
        .await
        .expect("复核");
    assert_eq!(
        host.release_preparation(&endpoint).await,
        ReleaseOutcome::Released
    );
    assert_eq!(
        host.release_preparation(&endpoint).await,
        ReleaseOutcome::Released,
        "重复释放必须幂等成功"
    );
    assert!(host.session_resources().await.is_empty());
    host.shutdown_all().await;
}

/// 首次提交的所有权转移复用同一个端点；派发上下文只留在适配器内部：
/// 可证明重复的回显不产生第二条用户消息事件，内容不同/普通 prompt 照常下发，且视图里没有任何
/// 派发上下文字段（不新增 wire 键）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ownership_transfer_reuses_endpoint_and_associates_user_echo_internally() {
    let dump = TempFile::new("acpr-preparation-transfer.jsonl");
    let _ = std::fs::remove_file(&dump);
    let host = host(vec![profile(
        "echo-user",
        &["--dump-request-params", &dump.to_string_lossy()],
    )]);
    let collector = Collector::new();
    let endpoint = host
        .prepare(&session_id(ECHO_SESSION), request(), collector.sink())
        .await
        .expect("prepare");
    endpoint
        .apply_selection(&PreparationSelection::default())
        .await
        .expect("复核");
    let agent_session = endpoint.agent_session_id().expect("真正答会话标识").clone();
    host.transfer_preparation(&endpoint).await.expect("转正");
    assert!(!endpoint.is_temporary());

    // fake Agent 对每次 prompt 都回显同一段文字；这里让上下文摘要与它逐字节一致（`Duplicate`）。
    // 摘要口径是**内容块原文**：先按 fake 的同一序列化路径（`json!` + `to_string`）得到该块的原文，
    // 再按 `FirstSubmissionInput` 的 `[<block>]` 字节口径算摘要。
    let echo_block = serde_json::to_string(&json!({
        "type": "text",
        "text": "首条消息的回显",
    }))
    .expect("回显内容块");
    let echo_digest = digest_of(&format!("[{echo_block}]"));
    let context = DispatchContext {
        session: session_id(ECHO_SESSION),
        first_submission: FirstSubmissionId::new("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
            .expect("first submission"),
        user_message: MessageId::new("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").expect("message"),
        request: RequestId::new("cccccccc-cccc-4ccc-8ccc-cccccccccccc").expect("request"),
        agent: agent_ref(),
        agent_session,
        resource_generation: ResourceGeneration::new(endpoint.generation()),
        input_digest: echo_digest,
    };
    endpoint
        .prompt_with_context(prompt("hello"), timestamp(), Some(context.clone()))
        .await
        .expect("首次派发");
    assert!(
        wait_for_count(&collector, "turn.completed", 1, Duration::from_secs(5)).await,
        "turn 必须收敛"
    );
    assert_eq!(
        collector.count("user.message.delta"),
        0,
        "可证明重复的回显不得再造第二条用户消息事件"
    );

    // 内容不同（摘要不符）⇒ Differs：回显照常下发为独立用户输入事件，视图里没有派发上下文。
    let mut differs = context.clone();
    differs.input_digest = digest_of("与回显不同的摘要");
    endpoint
        .prompt_with_context(prompt("hello"), timestamp(), Some(differs))
        .await
        .expect("第二条");
    assert!(
        wait_for_count(&collector, "user.message.delta", 1, Duration::from_secs(5)).await,
        "内容不同的回显必须保留，不能被吞掉"
    );
    assert!(
        wait_for_count(&collector, "turn.completed", 2, Duration::from_secs(5)).await,
        "第二条 turn 必须收敛"
    );

    // 普通 prompt：没有上下文，行为与关联功能出现之前完全相同。
    endpoint
        .prompt_with_context(prompt("again"), timestamp(), None)
        .await
        .expect("第三条");
    assert!(
        wait_for_count(&collector, "user.message.delta", 2, Duration::from_secs(5)).await,
        "普通 prompt 的用户输入事件照常下发"
    );
    assert!(
        wait_for_count(&collector, "turn.completed", 3, Duration::from_secs(5)).await,
        "第三条 turn 必须收敛"
    );

    let events: Vec<_> = collector
        .snapshot()
        .into_iter()
        .filter(|event| event.event_type.as_str() == "user.message.delta")
        .collect();
    assert_eq!(events.len(), 2, "只有 Differs 与普通 prompt 各一条用户事件");
    for event in &events {
        let view: Value = serde_json::from_str(event.payload.view.as_str()).expect("view");
        assert!(
            view.get("dispatchContext").is_none(),
            "派发上下文只在适配器内部：事件视图不得出现任何关联字段"
        );
        // 登记形状的最低字段仍在，ACP 原文也逐字节保留。
        for key in ["messageId", "deltaIndex", "text"] {
            assert!(view.get(key).is_some(), "缺少登记字段 {key}：{view}");
        }
        assert!(event.payload.acp.is_some(), "ACP 原文必须保留");
    }

    let rows =
        support::wait_for_dumped_method(&dump, "session/prompt", Duration::from_secs(5)).await;
    assert_eq!(
        dump_count(&rows, "session/new"),
        1,
        "转正后的首次派发不得再建第二个 Agent 会话"
    );
    assert_eq!(dump_count(&rows, "session/prompt"), 3);
    host.shutdown_all().await;
}

/// 准备端点计入运行时时长，`shutdown_all` 结束整棵进程树并回收任务。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preparation_endpoint_is_owned_by_runtime_and_shuts_down_with_its_tree() {
    let heartbeat = TempFile::new("acpr-preparation-heartbeat.txt");
    let _ = std::fs::remove_file(&heartbeat);
    let host = host(vec![profile(
        "normal",
        &["--heartbeat-file", &heartbeat.to_string_lossy()],
    )]);
    let collector = Collector::new();
    let endpoint = host
        .prepare(&session_id(TREE_SESSION), request(), collector.sink())
        .await
        .expect("prepare");
    assert!(runtime_running(&host, &agent_id()));
    assert!(
        wait_for_file(&heartbeat, Duration::from_secs(5)).await,
        "fake Agent 必须先开始写心跳"
    );

    host.shutdown_all().await;
    assert!(!runtime_running(&host, &agent_id()));
    assert!(!endpoint.is_released(), "本地关闭不等于 Agent 确认释放");
    let after_shutdown = file_len(&heartbeat);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        after_shutdown,
        file_len(&heartbeat),
        "进程树必须停止写心跳（没有残留子进程）"
    );
    host.shutdown_all().await;
}
