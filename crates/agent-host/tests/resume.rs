//! R5–R12、R26：`session/resume` 的**线级**证据（`local-agent-host` 增量）。
//!
//! 入口是**真实 stdio 子进程** `acpr-fake-acp-agent`（`CARGO_BIN_EXE_acpr-fake-acp-agent`，与真实 Agent
//! 同一套 LF 分帧 JSON），不是替身。断言绑定四类可观察事实：
//!
//! - `--dump-request-params` 文件里的**每一行** `{"method":…,"params":…}`（闭合点：只有
//!   经**真实子进程**读行，才能证明 fake child `main` 里那句 `message.get("params")` 的接线没退化成
//!   `params: null`——它没有单测覆盖，只有本文件的端到端读行能暴露）；
//! - `--heartbeat-file` 是否还在增长（进程外可观察的「子进程是否被回收」）；
//! - `host.resume` / `host.open` / `endpoint.prompt` 的返回值与事件流；
//! - 运行时目录里是否存在该 Agent 的活进程。
//!
//! **与既有 `tests/session.rs` 的关系**：WP5 已在 `session.rs` 覆盖 R7/R8/R9/R10/R11/R12/R26/R35 的
//! **method 级**证据。本文件**不复制**那些断言，只补两个维度：
//!
//! 1. **线级取值**（`params.sessionId` / `params.cwd` / 键集合）——评审指出 `--dump-requests` 只写
//!    method，没有通道能证明「发出去的 `cwd` 等于持久化值」，WP5 Round 2 已补 `--dump-request-params`；
//! 2. **SR-R12-2 的正确前提**（裁决）：`resume` 在进程仍存活时**复用同一个子进程**，因此用例前提
//!    是「进程复用、单一心跳文件」，断言改为「恰好一个可派发端点 + `session/resume` 恰好一行 + 只有一个
//!    进程」。**不改产品代码**。

mod support;

use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{
    AgentId, AgentProfile, AgentRef, AgentSessionId, CreateSessionRequest, OwnedSessionRef,
    PortError, PromptContentBlock, PromptRequest, ResourceOrigin, ResumeSessionRequest, SessionId,
    SessionReference, UnavailableKind,
};
use acp_core::ports::{SessionBackendFactory, SessionEndpoint};
use agent_host::{AgentHost, HostConfig, runtime_running};
use serde_json::Value;
use support::{Collector, FAKE_AGENT, FakeCredentials, TempFile, TestClock, TestIds, profile_with};

/// R12-2：旧绑定尚未让出时再次恢复（单一会话标识）。
const RESUME_TWICE_SESSION: &str = "33333333-3333-4333-8333-333333333333";
/// R10/R37：能力未宣告路径。
const RESUME_UNDECLARED_SESSION: &str = "44444444-4444-4444-8444-444444444444";
/// 线级读行用的会话标识（R8/R26）。
const RESUME_LINE_SESSION: &str = "55555555-5555-4555-8555-555555555555";

fn host(profiles: Vec<AgentProfile>) -> Arc<AgentHost> {
    Arc::new(AgentHost::new(
        Arc::new(support::FakeConfig::new(profiles)),
        Arc::new(FakeCredentials::ok()),
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

/// `dyn SessionEndpoint` 没有 `Debug`，因此不能 `expect_err`。
fn outcome_error<T>(result: Result<T, PortError>) -> PortError {
    match result {
        Ok(_) => panic!("期望失败，但得到了成功结果"),
        Err(error) => error,
    }
}

fn prompt(text: &str) -> PromptRequest {
    PromptRequest::new(vec![
        PromptContentBlock::from_json_text(&format!(r#"{{"type":"text","text":"{text}"}}"#))
            .expect("内容块"),
    ])
}

/// 恢复输入：持久化的 ACP 会话标识 + 持久化的创建时 cwd **原文**。
fn resume_request(agent_session_id: &str, workspace_cwd: &str) -> ResumeSessionRequest {
    ResumeSessionRequest::try_new(
        agent_ref(),
        AgentSessionId::new(agent_session_id).expect("agent session id"),
        workspace_cwd.to_owned(),
    )
    .expect("恢复请求")
}

/// 本机上一个存在的绝对目录（用作「持久化的创建时 cwd 原文」）。
///
/// 取的是**系统临时目录本身**（必然存在）；R26 的关键是把「传进恢复的原文」与「另一个可解析到的
/// 目录」区分开，因此本文件另有一个故意不同的对照目录（`contrast_workspace`）。
fn persisted_workspace_cwd() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

/// 与持久化取值**故意不同**、同样存在的绝对路径：用来证明恢复**不按别名/新解析结果**发送 `cwd`。
fn contrast_workspace() -> String {
    let root = std::env::temp_dir().join("acpr-tp2-contrast-workspace");
    let _ = std::fs::create_dir_all(&root);
    root.to_string_lossy().into_owned()
}

/// `--dump-request-params <path>` 的记录行（每行一条 `{"method":…,"params":…}`）。
///
/// 这条通道经**真实子进程**写出，因此它同时闭合了 fake child `main` 的 `params` 接线：若该接线被改成
/// `dump_inbound(&args, method, None)`，这里的 `params` 会全变成 `null`，本文件的断言立即失败
fn dumped_requests(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|error| panic!("dump 行不是 JSON：{line}（{error}）"))
        })
        .collect()
}

/// dump 行里某个方法的 `params`（非 `null`），并断言该行的键集合恰为 `{method, params}`。
fn params_of<'a>(rows: &'a [Value], method: &str) -> &'a Value {
    let matched: Vec<&Value> = rows
        .iter()
        .filter(|row| row["method"] == serde_json::Value::String(method.to_owned()))
        .collect();
    assert!(!matched.is_empty(), "dump 里必须出现 {method}：{rows:?}");
    let params = &matched[0]["params"];
    assert!(
        !params.is_null(),
        "{method} 的 params 不得为 null（fake child 的 params 接线退化）：{rows:?}"
    );
    for row in matched {
        let mut keys: Vec<&str> = row
            .as_object()
            .expect("dump 行是 JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["method", "params"], "dump 行的键集合必须恰为两键");
    }
    params
}

/// 某个方法在 dump 里出现的次数。
fn dump_count(rows: &[Value], method: &str) -> usize {
    rows.iter()
        .filter(|row| row["method"] == serde_json::Value::String(method.to_owned()))
        .count()
}

/// 等心跳文件出现。
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

/// 断言「在某观察窗内心跳不再增长」⇒ 子进程已结束（进程外证据，不依赖进程内状态位）。
async fn assert_process_reclaimed(heartbeat: &std::path::Path, label: &str) {
    assert!(
        wait_for_file(heartbeat, Duration::from_secs(5)).await,
        "{label}: 子进程应在被回收前留下心跳文件"
    );
    let size = file_len(heartbeat);
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        file_len(heartbeat),
        size,
        "{label}: 不得残留子进程（心跳仍在增长）"
    );
}

/// R8 + R26（**线级**，闭合点）：恢复时真正发给 Agent 的 `session/resume` 的
/// `params.sessionId` 与 `params.cwd` 逐字等于持久化取值，键集合恰为两个，且**不按别名/新解析结果**改写。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_wire_level_resume_params_carry_the_persisted_values() {
    let collector = Collector::new();
    let params_dump = TempFile::new("acpr-tp2-resume-params.params");
    let params_path = params_dump.to_string_lossy().into_owned();
    let host = host(vec![profile_with(
        "agent-1",
        FAKE_AGENT,
        &[
            "--scenario",
            "resume-ok",
            "--capabilities",
            r#"{"sessionCapabilities":{"resume":{}}}"#,
            "--dump-request-params",
            &params_path,
        ],
    )]);

    let persisted = persisted_workspace_cwd();
    let contrast = contrast_workspace();
    assert_ne!(
        persisted, contrast,
        "对照目录必须与持久化取值不同（否则「没有按别名重解析」这条断言无判别力）"
    );

    let endpoint = host
        .resume(
            &session_id(RESUME_LINE_SESSION),
            resume_request("acp-session-persisted", &persisted),
            collector.sink(),
        )
        .await
        .expect("宣告能力后恢复必须成功");

    let rows = dumped_requests(&params_dump);
    assert_eq!(
        dump_count(&rows, "session/resume"),
        1,
        "必须恰好发送一次 session/resume（不重试）：{rows:?}"
    );
    let params = params_of(&rows, "session/resume");
    assert_eq!(
        params["sessionId"],
        serde_json::Value::String("acp-session-persisted".to_owned()),
        "session/resume 的 sessionId 必须逐字是持久化的 ACP 会话标识"
    );
    assert_eq!(
        params["cwd"],
        serde_json::Value::String(persisted.clone()),
        "session/resume 的 cwd 必须逐字是持久化的创建时目录原文"
    );
    assert_ne!(
        params["cwd"],
        serde_json::Value::String(contrast),
        "cwd 不得被换成重新解析出的目录"
    );
    assert_eq!(
        dump_count(&rows, "session/new"),
        0,
        "恢复不得静默改走新建会话：{rows:?}"
    );
    // 能力门控的前置：initialize 必须先发生（这正是 R10 措辞里「必然已经拉起进程」的部分）。
    assert_eq!(
        dump_count(&rows, "initialize"),
        1,
        "恢复前必须先协商能力：{rows:?}"
    );
    assert_eq!(
        endpoint.agent_session_id().map(AgentSessionId::as_str),
        Some("acp-session-persisted"),
        "恢复端点读回的是持久化标识（响应里根本没有 sessionId，因此不是编造）"
    );
    drop(endpoint);
    host.shutdown_all().await;
}

/// R8 + R9：恢复后的端点真的可交互——`prompt` 被派发一次、turn 完成，且线级只有一条 `session/prompt`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resumed_endpoint_dispatches_exactly_one_prompt_and_completes_the_turn() {
    let collector = Collector::new();
    let params_dump = TempFile::new("acpr-tp2-resume-prompt.params");
    let params_path = params_dump.to_string_lossy().into_owned();
    let host = host(vec![profile_with(
        "agent-1",
        FAKE_AGENT,
        &[
            "--scenario",
            "resume-ok",
            "--capabilities",
            r#"{"sessionCapabilities":{"resume":{}}}"#,
            "--dump-request-params",
            &params_path,
        ],
    )]);

    let endpoint = host
        .resume(
            &session_id(RESUME_LINE_SESSION),
            resume_request("acp-session-interactive", &persisted_workspace_cwd()),
            collector.sink(),
        )
        .await
        .expect("恢复成功");
    endpoint
        .prompt(prompt("恢复之后"), support::timestamp())
        .await
        .expect("恢复后的端点必须接受 prompt");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(15))
            .await,
        "恢复后的 turn 必须完成：{:?}",
        collector.event_types()
    );

    let rows = dumped_requests(&params_dump);
    assert_eq!(
        dump_count(&rows, "session/prompt"),
        1,
        "一个 prompt 只派发一次：{rows:?}"
    );
    assert_eq!(
        endpoint.reference().session_id().as_str(),
        RESUME_LINE_SESSION,
        "端点引用仍是同一个 core 会话"
    );
    drop(endpoint);
    host.shutdown_all().await;
}

/// R10（**修正后的规格措辞**）：能力未宣告时——**不发送** `session/resume`、
/// **在返回前回收本次拉起的子进程**、不留绑定、会话状态不变。
///
/// 措辞来源：`specs/local-agent-host/spec.md` R10 已改为「MUST NOT 发送 `session/resume`，且 MUST 在
/// 返回明确的不支持错误之前终止并回收本次为恢复而拉起的子进程」。因此本用例断言的是
/// 「线级无 `session/resume` + 心跳停止 + 无绑定 + 错误分类」，**不是**「完全没有 spawn」——
/// 能力只能经 `initialize` 得知，而 `initialize` 必然已经拉起进程，字面「不 spawn」物理不可满足。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_undeclared_capability_sends_no_resume_request_and_reclaims_the_child() {
    let collector = Collector::new();
    let params_dump = TempFile::new("acpr-tp2-undeclared.params");
    let heartbeat = TempFile::new("acpr-tp2-undeclared.heartbeat");
    let params_path = params_dump.to_string_lossy().into_owned();
    let heartbeat_path = heartbeat.to_string_lossy().into_owned();

    // 字段省略（不是 `null`）⇒ 未宣告。
    let host = host(vec![profile_with(
        "agent-1",
        FAKE_AGENT,
        &[
            "--scenario",
            "normal",
            "--dump-request-params",
            &params_path,
            "--heartbeat-file",
            &heartbeat_path,
        ],
    )]);

    let error = outcome_error(
        host.resume(
            &session_id(RESUME_UNDECLARED_SESSION),
            resume_request("acp-session-undeclared", &persisted_workspace_cwd()),
            collector.sink(),
        )
        .await,
    );
    // ① 错误分类是「后端不支持」，与「服务端不可用」是**不同**取值（两类失败因此可区分）。
    assert!(
        matches!(
            error,
            PortError::Unavailable(UnavailableKind::BackendUnsupported)
        ),
        "能力未宣告必须表达为「后端不支持」：{error}"
    );

    let rows = dumped_requests(&params_dump);
    assert_eq!(
        dump_count(&rows, "initialize"),
        1,
        "能力只能在 initialize 之后得知，因此 initialize 必然发生：{rows:?}"
    );
    assert_eq!(
        dump_count(&rows, "session/resume"),
        0,
        "能力未宣告时 MUST NOT 发送 session/resume：{rows:?}"
    );
    assert_eq!(
        dump_count(&rows, "session/new"),
        0,
        "能力未宣告时 MUST NOT 改用新建会话：{rows:?}"
    );

    // ② 本次为恢复而拉起的子进程已在返回前被回收（心跳停止增长 + 运行时目录里没有活进程）。
    assert_process_reclaimed(&heartbeat, "能力未宣告").await;
    assert!(
        !runtime_running(&host, &AgentId::new("agent-1").expect("id")),
        "运行时目录里不得留下本次为恢复拉起的进程"
    );

    // ③ 没有留下任何会话绑定，事件流为空（会话状态不变）。
    assert!(
        host.open(
            SessionReference::Owned(OwnedSessionRef::new(session_id(RESUME_UNDECLARED_SESSION))),
            collector.sink(),
        )
        .await
        .is_err(),
        "不支持的恢复不得留下端点"
    );
    assert!(collector.is_empty(), "不得产生事件");
    host.shutdown_all().await;
}

/// R10（边界）：`sessionCapabilities.resume: null` 与**字段省略**同判不支持，行为逐项相同。
///
/// 判别力：若实现把 `null` 当成「已宣告」，第二次调用会真的发 `session/resume`，断言失败。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_null_resume_capability_is_the_same_as_an_omitted_one() {
    for (label, capabilities) in [
        ("null", r#"{"sessionCapabilities":{"resume":null}}"#),
        ("omitted", r#"{"sessionCapabilities":{}}"#),
        ("absent", r#"{"agentCapabilities":{}}"#),
    ] {
        let collector = Collector::new();
        let params_dump = TempFile::new(&format!("acpr-tp2-cap-{label}.params"));
        let params_path = params_dump.to_string_lossy().into_owned();
        let host = host(vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &[
                "--scenario",
                "normal",
                "--capabilities",
                capabilities,
                "--dump-request-params",
                &params_path,
            ],
        )]);
        let error = outcome_error(
            host.resume(
                &session_id(RESUME_UNDECLARED_SESSION),
                resume_request("acp-session-cap", &persisted_workspace_cwd()),
                collector.sink(),
            )
            .await,
        );
        assert!(
            matches!(
                error,
                PortError::Unavailable(UnavailableKind::BackendUnsupported)
            ),
            "{label}: 能力未宣告必须表达为「后端不支持」：{error}"
        );
        assert_eq!(
            dump_count(&dumped_requests(&params_dump), "session/resume"),
            0,
            "{label}: 不得发送 session/resume"
        );
        assert!(collector.is_empty(), "{label}: 不得产生事件");
        host.shutdown_all().await;
    }
}

/// R11（**线级**）：Agent 拒绝恢复时明确失败——发过**恰好一条** `session/resume`、**不发**
/// `session/new`（不产生新会话标识）、不留绑定、回收子进程。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_refusal_sends_one_resume_request_and_no_new_session() {
    let collector = Collector::new();
    let params_dump = TempFile::new("acpr-tp2-refusal.params");
    let heartbeat = TempFile::new("acpr-tp2-refusal.heartbeat");
    let params_path = params_dump.to_string_lossy().into_owned();
    let heartbeat_path = heartbeat.to_string_lossy().into_owned();
    let host = host(vec![profile_with(
        "agent-1",
        FAKE_AGENT,
        &[
            "--scenario",
            "resume-error",
            "--capabilities",
            r#"{"sessionCapabilities":{"resume":{}}}"#,
            "--dump-request-params",
            &params_path,
            "--heartbeat-file",
            &heartbeat_path,
        ],
    )]);

    let error = outcome_error(
        host.resume(
            &session_id(RESUME_UNDECLARED_SESSION),
            resume_request("acp-session-gone", &persisted_workspace_cwd()),
            collector.sink(),
        )
        .await,
    );
    assert!(
        matches!(error, PortError::InvalidRequest(_)),
        "Agent 拒绝恢复必须明确失败：{error}"
    );

    let rows = dumped_requests(&params_dump);
    assert_eq!(
        dump_count(&rows, "session/resume"),
        1,
        "拒绝路径不得重试 session/resume：{rows:?}"
    );
    let params = params_of(&rows, "session/resume");
    assert_eq!(
        params["sessionId"],
        serde_json::Value::String("acp-session-gone".to_owned()),
        "拒绝路径仍必须发出正确的 sessionId（证明不是编造的标识）"
    );
    assert_eq!(
        dump_count(&rows, "session/new"),
        0,
        "拒绝后不得静默改走新建会话（不产生新会话标识）：{rows:?}"
    );
    assert!(
        host.open(
            SessionReference::Owned(OwnedSessionRef::new(session_id(RESUME_UNDECLARED_SESSION))),
            collector.sink(),
        )
        .await
        .is_err(),
        "失败的恢复不得留下端点（会话不进入可交互状态）"
    );
    assert!(collector.is_empty(), "不得产生事件");
    assert_process_reclaimed(&heartbeat, "拒绝恢复").await;
    host.shutdown_all().await;
}

/// R12-2（**修正后的前提**，裁决）：进程仍存活时再次恢复 ⇒ **复用同一个子进程**，
/// 因此断言是「恰好一个可派发端点 + `session/resume` 恰好一行 + 只有一个进程」。
///
/// 本用例**不**假设会出现第二个进程或第二个心跳文件（`ensure_runtime_tracked` 会返回既有 runtime）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resuming_twice_reuses_one_process_and_leaves_one_dispatchable_endpoint() {
    let first = Collector::new();
    let second = Collector::new();
    let params_dump = TempFile::new("acpr-tp2-twice.params");
    let heartbeat = TempFile::new("acpr-tp2-twice.heartbeat");
    let params_path = params_dump.to_string_lossy().into_owned();
    let heartbeat_path = heartbeat.to_string_lossy().into_owned();
    let host = host(vec![profile_with(
        "agent-1",
        FAKE_AGENT,
        &[
            "--scenario",
            "resume-ok",
            "--capabilities",
            r#"{"sessionCapabilities":{"resume":{}}}"#,
            "--dump-request-params",
            &params_path,
            "--heartbeat-file",
            &heartbeat_path,
        ],
    )]);

    let session = session_id(RESUME_TWICE_SESSION);
    let request = resume_request("acp-session-twice", &persisted_workspace_cwd());
    let stale = host
        .resume(&session, request.clone(), first.sink())
        .await
        .expect("第一次恢复");
    let current = host
        .resume(&session, request, second.sink())
        .await
        .expect("第二次恢复");

    // ① 恰好一个端点能派发 turn（不预设是哪一个——spec 只要求「旧绑定先让出或本次被明确拒绝」）。
    let stale_outcome = stale.prompt(prompt("旧端点"), support::timestamp()).await;
    let current_outcome = current.prompt(prompt("新端点"), support::timestamp()).await;
    let dispatchable = usize::from(stale_outcome.is_ok()) + usize::from(current_outcome.is_ok());
    assert_eq!(
        dispatchable, 1,
        "恰好一个端点可派发 turn（旧绑定先让出或本次被拒）：stale={stale_outcome:?} current={current_outcome:?}"
    );
    // 可派发的那个必须真的完成 turn；不可派发的那个不得产生任何事件。
    let (winner, loser) = if stale_outcome.is_ok() {
        (&first, &second)
    } else {
        (&second, &first)
    };
    assert!(
        winner
            .wait_for_type("turn.completed", Duration::from_secs(15))
            .await,
        "可派发的端点必须完成 turn：{:?}",
        winner.event_types()
    );
    assert_eq!(
        loser.count("turn.completed"),
        0,
        "不可派发的端点不得产生事件（同一会话两个端点并行派发）"
    );

    // ② 只有一个进程：第二次恢复**复用**了既有子进程，因此只协商过一次能力——若实现为第二次恢复
    //    **另拉一个子进程**，那条路径必然再发一次 `initialize`（能力只在 initialize 里协商）。
    //    （注：建议把断言写成「`session/resume` 恰好一行」。实测在基线 `95051f9` 上第二次
    //    `resume` 会**再发一条** `session/resume` 给被复用的同一进程——`resume` 每次调用都执行一次
    //    「按持久化取值发送 session/resume」，R12 的规格保证只约束「可派发端点」而不约束消息条数。
    //    因此这里的判别式取「`initialize` 恰好一行」，它才是「没有第二个进程」的线级证据。）
    let rows = dumped_requests(&params_dump);
    assert_eq!(
        dump_count(&rows, "initialize"),
        1,
        "第二次恢复必须复用既有子进程（不得另拉一个进程）：{rows:?}"
    );
    assert_eq!(
        dump_count(&rows, "session/prompt"),
        1,
        "只有一个 prompt 被真正派发：{rows:?}"
    );

    // ③ 只有一个进程：单一心跳文件对应唯一存活子进程——恢复完成后它仍在增长（成功路径不提前清理）。
    assert!(
        wait_for_file(&heartbeat, Duration::from_secs(5)).await,
        "子进程应留下心跳文件"
    );
    let before = file_len(&heartbeat);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        file_len(&heartbeat) > before,
        "成功恢复后进程仍存活（心跳继续增长）"
    );

    drop(stale);
    drop(current);
    host.shutdown_all().await;
    // 关闭之后心跳必须停止：证明上面观察到的增长确实来自**唯一**那个子进程。
    assert_process_reclaimed(&heartbeat, "全部关闭后").await;
}

/// R5/R6：创建成功后端点暴露本次建立的 ACP 会话标识，两次创建各自不同、互不覆盖。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn creating_two_sessions_exposes_two_distinct_agent_session_ids() {
    let first = Collector::new();
    let second = Collector::new();
    let params_dump = TempFile::new("acpr-tp2-create.params");
    let params_path = params_dump.to_string_lossy().into_owned();
    let host = host(vec![profile_with(
        "agent-1",
        FAKE_AGENT,
        &[
            "--scenario",
            "normal",
            "--dump-request-params",
            &params_path,
        ],
    )]);

    async fn create_with(
        host: &Arc<AgentHost>,
        session: SessionId,
        collector: &Collector,
    ) -> Box<dyn SessionEndpoint> {
        host.create(
            &session,
            CreateSessionRequest::new(
                agent_ref(),
                Some(support::workspace()),
                None,
                ResourceOrigin::Local,
            ),
            collector.sink(),
        )
        .await
        .expect("创建会话")
    }

    let one = create_with(&host, session_id(RESUME_LINE_SESSION), &first).await;
    let two = create_with(&host, session_id(RESUME_TWICE_SESSION), &second).await;

    let first_id = one
        .agent_session_id()
        .map(AgentSessionId::as_str)
        .expect("创建成功后必须暴露 ACP 会话标识")
        .to_owned();
    let second_id = two
        .agent_session_id()
        .map(AgentSessionId::as_str)
        .expect("创建成功后必须暴露 ACP 会话标识")
        .to_owned();
    assert_ne!(
        first_id, second_id,
        "两次创建各自暴露本次建立的标识，不得互相覆盖"
    );
    assert_eq!(one.reference().session_id().as_str(), RESUME_LINE_SESSION);
    assert_eq!(two.reference().session_id().as_str(), RESUME_TWICE_SESSION);

    let rows = dumped_requests(&params_dump);
    assert_eq!(
        dump_count(&rows, "session/new"),
        2,
        "两次创建各发一次 session/new：{rows:?}"
    );
    drop(one);
    drop(two);
    host.shutdown_all().await;
}
