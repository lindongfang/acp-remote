//! 出站 `cwd` 的**线级形态**：`session/new`（`host.create`）与 `session/resume`（`host.resume`）两条腿。
//!
//! 被测行为：Agent **实际收到的字节**是剥掉 Windows verbatim 前缀后的形式（`\\?\D:\ws` → `D:\ws`、
//! `\\?\UNC\s\sh\ws` → `\\s\sh\ws`），无前缀路径逐字节直通，且持久化取值本身不变（`sessionId` 一字不改）。
//!
//! 证据来源是**真实 stdio 子进程** `acpr-fake-acp-agent` 的 `--dump-request-params` 文件（与
//! `resume.rs` 顶部同一口径）：断言的是发出去的 `params`，**不是** `outbound_cwd` 的返回值——后者等于
//! 把实现抄进测试，实现反着写也照样绿。dump 的三个助手外加一个轮询助手只在 `support` 里实现一次。
//!
//! **平台门控**：`acp_core::strip_verbatim` 是 `#[cfg]` 分支函数（Windows 剥前缀、其它平台逐字节直通），
//! 因此「带 `\\?\` 的输入必须被剥掉」只在 Windows 成立：那四条用例整体 `#[cfg(windows)]`。平台无关的
//! 断言（无前缀直通、键集合、`sessionId` 未被改写）在两条平台上都执行（Linux CI 会真正跑它们）。
//!
//! 真实场景用例不手造字符串：输入取 `std::fs::canonicalize` 的真实结果（core 落盘的正是这个取值），
//! 并**当场断言前提**（Windows 上它必须以 `\\?\` 开头）——前提不成立时立刻失败，而不是让「输入 == 期望」
//! 把用例退化成恒真断言。

mod support;

use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{
    AgentId, AgentRef, AgentSessionId, CreateSessionRequest, ResolvedWorkspace, ResourceOrigin,
    ResumeSessionRequest, SessionId, WorkspaceAlias,
};
use acp_core::ports::SessionBackendFactory;
use agent_host::{AgentHost, HostConfig};
use serde_json::{Value, json};
use support::{
    Collector, FAKE_AGENT, FakeCredentials, TempFile, TestClock, TestIds, assert_param_keys,
    dump_count, params_of, profile_with, wait_for_dumped_method,
};

/// dump 文件由子进程**异步**写出：等到目标方法出现的上限取宽松值（慢机器上不闪红；真失败仍由断言兜住）。
const DUMP_TIMEOUT: Duration = Duration::from_secs(30);

/// 恢复腿的前置：Agent 必须在 `initialize` 里宣告 `sessionCapabilities.resume`。
const RESUME_CAPABILITIES: &str = r#"{"sessionCapabilities":{"resume":{}}}"#;

/// 创建腿的 core 会话标识。
const CREATE_SESSION: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
/// 恢复腿的 core 会话标识。
const RESUME_SESSION: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
/// 恢复腿的**持久化 ACP 会话标识**：必须逐字出现在出站 `params.sessionId` 上（形态转换只作用于 cwd）。
const PERSISTED_ACP_SESSION: &str = "acp-session-cwd-form";

fn host(agent_args: &[&str]) -> Arc<AgentHost> {
    Arc::new(AgentHost::new(
        Arc::new(support::FakeConfig::new(vec![profile_with(
            "agent-1", FAKE_AGENT, agent_args,
        )])),
        Arc::new(FakeCredentials::ok()),
        HostConfig::default(),
        Arc::new(TestIds::new()),
        Arc::new(TestClock::new()),
    ))
}

fn session_id(text: &str) -> SessionId {
    SessionId::new(text).expect("session id")
}

fn agent_ref() -> AgentRef {
    AgentRef::try_new(AgentId::new("agent-1").expect("id"), "Agent agent-1").expect("ref")
}

/// 本文件各用例独占的 dump 路径（名字含标签与 pid，满足 `TempFile` 的唯一性约定）。
fn dump_file(label: &str) -> TempFile {
    TempFile::new(&format!(
        "acpr-cwd-form-{label}-{}.params",
        std::process::id()
    ))
}

/// 用给定的 canonical path **原文**构造 workspace（core 落盘的正是这个字符串）。
fn workspace(canonical_path: &str) -> ResolvedWorkspace {
    ResolvedWorkspace::try_new(
        WorkspaceAlias::new("demo").expect("alias"),
        canonical_path.to_owned(),
    )
    .expect("workspace（canonical path 必须是绝对路径形状）")
}

/// 恢复输入：持久化的 ACP 会话标识 + 持久化的创建时 cwd 原文。
fn resume_request(workspace_cwd: &str) -> ResumeSessionRequest {
    ResumeSessionRequest::try_new(
        agent_ref(),
        AgentSessionId::new(PERSISTED_ACP_SESSION).expect("agent session id"),
        workspace_cwd.to_owned(),
    )
    .expect("恢复请求（cwd 必须是绝对路径形状）")
}

/// 创建腿：以 `canonical_path` 原文建会话，返回线级 `session/new` 的 `params`。
async fn create_wire_params(canonical_path: &str, dump: &TempFile) -> Value {
    // 清掉可能残留的同名 dump（pid 复用）：陈旧行会让 `dump_count == 1` 变成假失败。
    let _ = std::fs::remove_file(&**dump);
    let dump_path = dump.to_string_lossy().into_owned();
    let host = host(&["--scenario", "normal", "--dump-request-params", &dump_path]);
    let collector = Collector::new();
    let endpoint = host
        .create(
            &session_id(CREATE_SESSION),
            CreateSessionRequest::new(
                agent_ref(),
                Some(workspace(canonical_path)),
                None,
                ResourceOrigin::Local,
            ),
            collector.sink(),
        )
        .await
        .expect("建会话必须成功（cwd 的形态转换不得让请求失败）");
    let rows = wait_for_dumped_method(dump, "session/new", DUMP_TIMEOUT).await;
    assert_eq!(
        dump_count(&rows, "session/new"),
        1,
        "一次 create 只应发一条 session/new（读到的 params 因此不是重试来的）：{rows:?}"
    );
    let params = params_of(&rows, "session/new").clone();
    drop(endpoint);
    host.shutdown_all().await;
    params
}

/// 恢复腿：以 `workspace_cwd` 原文恢复会话，返回线级 `session/resume` 的 `params`。
async fn resume_wire_params(workspace_cwd: &str, dump: &TempFile) -> Value {
    // 清掉可能残留的同名 dump（pid 复用）：陈旧行会让 `dump_count == 1` 变成假失败。
    let _ = std::fs::remove_file(&**dump);
    let dump_path = dump.to_string_lossy().into_owned();
    let host = host(&[
        "--scenario",
        "resume-ok",
        "--capabilities",
        RESUME_CAPABILITIES,
        "--dump-request-params",
        &dump_path,
    ]);
    let collector = Collector::new();
    let endpoint = host
        .resume(
            &session_id(RESUME_SESSION),
            resume_request(workspace_cwd),
            collector.sink(),
        )
        .await
        .expect("宣告能力后恢复必须成功（cwd 的形态转换不得让请求失败）");
    let rows = wait_for_dumped_method(dump, "session/resume", DUMP_TIMEOUT).await;
    assert_eq!(
        dump_count(&rows, "session/resume"),
        1,
        "一次恢复只应发一条 session/resume：{rows:?}"
    );
    let params = params_of(&rows, "session/resume").clone();
    drop(endpoint);
    host.shutdown_all().await;
    params
}

/// 创建腿的断言：`cwd` 恰是期望形态、`mcpServers` 仍是空数组、键集合未新增。
fn assert_create_params(params: &Value, expected_cwd: &str) {
    assert_eq!(
        params["cwd"],
        Value::String(expected_cwd.to_owned()),
        "session/new 的 cwd 必须是「{expected_cwd}」这一形态：{params}"
    );
    assert_eq!(
        params["mcpServers"],
        json!([]),
        "session/new 的 mcpServers 必须仍为空数组（空数组合法，不得被形态转换顺手改掉）：{params}"
    );
    assert_param_keys(params, &["cwd", "mcpServers"]);
}

/// 恢复腿的断言：`cwd` 恰是期望形态、`sessionId` 逐字是持久化标识、键集合未新增。
fn assert_resume_params(params: &Value, expected_cwd: &str) {
    assert_eq!(
        params["cwd"],
        Value::String(expected_cwd.to_owned()),
        "session/resume 的 cwd 必须是「{expected_cwd}」这一形态：{params}"
    );
    assert_eq!(
        params["sessionId"],
        Value::String(PERSISTED_ACP_SESSION.to_owned()),
        "session/resume 的 sessionId 必须逐字是传入的持久化 ACP 会话标识（形态转换只作用于 cwd）：{params}"
    );
    assert_param_keys(params, &["cwd", "sessionId"]);
}

/// 造一个**真实存在**的临时目录，返回它（供清理）与 `std::fs::canonicalize` 的结果。
///
/// 后者就是 core 实际交给后端的**持久化创建时路径原文**：Windows 上它带 `\\?\` 前缀，其它平台是普通绝对路径。
fn canonicalized_temp_dir(label: &str) -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("acpr-cwd-form-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("临时目录必须能创建");
    let canonical = std::fs::canonicalize(&dir).expect("canonicalize 必须成功");
    (dir, canonical.to_string_lossy().into_owned())
}

/// canonicalize 结果的**期望出站形态**：Windows 上剥掉 `\\?\` 前缀，其它平台逐字节直通。
///
/// Windows 上的**前提当场断言**：canonicalize 的结果必须以 `\\?\` 开头。若前提不成立而这里默默返回原值，
/// 「出站 cwd == 输入」就会退化成恒真断言，用例再也抓不到任何实现错误。
fn expected_outbound_of_canonicalized(input: &str) -> String {
    match input.strip_prefix(r"\\?\") {
        Some(rest) => rest.to_owned(),
        #[cfg(windows)]
        None => panic!(
            "前提不成立：Windows 上 canonicalize 的结果必须带 \\\\?\\ 前缀，实际是 {input}（用例前提被破坏时必须当场失败）"
        ),
        #[cfg(not(windows))]
        None => input.to_owned(),
    }
}

/// 普通 verbatim 形态（创建腿，Windows 独有）：`\\?\D:\tmp\ws` ⇒ 出站 `cwd` 恰为 `D:\tmp\ws`。
///
/// 判别力：把出站 `cwd` 改回直通（不剥前缀）时，这里读到的是 `\\?\D:\tmp\ws`，`assert_create_params` 失败。
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_new_strips_the_verbatim_prefix() {
    let dump = dump_file("new-verbatim");
    let params = create_wire_params(r"\\?\D:\tmp\ws", &dump).await;
    assert_create_params(&params, r"D:\tmp\ws");
}

/// 普通 verbatim 形态（恢复腿，Windows 独有）：`\\?\D:\tmp\ws` ⇒ 出站 `cwd` 恰为 `D:\tmp\ws`。
///
/// 判别力：恢复腿的 `cwd` 改回直通时同样会读到带前缀的原值，`assert_resume_params` 失败
/// （顺带证明 `sessionId` 的逐字传递与 cwd 的形态转换是两件互不干扰的事）。
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_resume_strips_the_verbatim_prefix() {
    let dump = dump_file("resume-verbatim");
    let params = resume_wire_params(r"\\?\D:\tmp\ws", &dump).await;
    assert_resume_params(&params, r"D:\tmp\ws");
}

/// UNC verbatim 形态（创建腿，Windows 独有）：`\\?\UNC\server\share\ws` ⇒ `\\server\share\ws`（普通 UNC）。
///
/// 判别力：若实现只做「去掉开头四个字符」，出站值会变成 `UNC\server\share\ws`（丢了一个反斜杠），
/// 与期望的 `\\server\share\ws` 不等；若实现原样透传 verbatim UNC，同样不等。
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_new_restores_verbatim_unc_to_plain_unc() {
    let dump = dump_file("new-unc");
    let params = create_wire_params(r"\\?\UNC\server\share\ws", &dump).await;
    assert_create_params(&params, r"\\server\share\ws");
}

/// UNC verbatim 形态（恢复腿，Windows 独有）：同上，作用于持久化的创建时 cwd。
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_resume_restores_verbatim_unc_to_plain_unc() {
    let dump = dump_file("resume-unc");
    let params = resume_wire_params(r"\\?\UNC\server\share\ws", &dump).await;
    assert_resume_params(&params, r"\\server\share\ws");
}

/// 无前缀直通（创建腿，平台无关）：普通绝对路径必须**逐字节**原样发出。
///
/// 判别力：抓「无条件剪掉前四个字符」「无条件 canonicalize/替换分隔符/补尾部分隔符」这类顺手改写
/// （它们会把无前缀路径改坏），也抓「多加一个字段」这类形状漂移。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_new_passes_an_unprefixed_absolute_path_through_byte_for_byte() {
    let input = std::env::temp_dir().to_string_lossy().into_owned();
    let dump = dump_file("new-plain");
    let params = create_wire_params(&input, &dump).await;
    assert_create_params(&params, &input);
}

/// 无前缀直通（恢复腿，平台无关）：持久化的普通绝对路径必须逐字节原样发出。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_resume_passes_an_unprefixed_absolute_path_through_byte_for_byte() {
    let input = std::env::temp_dir().to_string_lossy().into_owned();
    let dump = dump_file("resume-plain");
    let params = resume_wire_params(&input, &dump).await;
    assert_resume_params(&params, &input);
}

/// 真实场景（创建腿）：输入是真实目录 `canonicalize` 的结果（core 落盘的取值，Windows 上带 `\\?\`）。
///
/// 断言：出站 `cwd` 等于「该取值去掉 `\\?\` 前缀」，且 Windows 上**不等于**输入本身
/// （证明转换真的发生，而不是输入碰巧不带前缀）；非 Windows 上退化为逐字节相同。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_new_strips_the_prefix_of_a_real_canonicalized_directory() {
    let (dir, canonical) = canonicalized_temp_dir("canonical-new");
    let expected = expected_outbound_of_canonicalized(&canonical);
    let dump = dump_file("new-canonical");
    let params = create_wire_params(&canonical, &dump).await;
    assert_create_params(&params, &expected);
    #[cfg(windows)]
    assert_ne!(
        params["cwd"].as_str(),
        Some(canonical.as_str()),
        "Windows 上出站 cwd 必须不再等于带前缀的持久化取值（证明 \\\\?\\ 真的被剥掉了）：{params}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 真实场景（恢复腿）：同上的取值经 `session/resume` 发出，`sessionId` 同时必须逐字不变。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_resume_strips_the_prefix_of_a_real_canonicalized_directory() {
    let (dir, canonical) = canonicalized_temp_dir("canonical-resume");
    let expected = expected_outbound_of_canonicalized(&canonical);
    let dump = dump_file("resume-canonical");
    let params = resume_wire_params(&canonical, &dump).await;
    assert_resume_params(&params, &expected);
    #[cfg(windows)]
    assert_ne!(
        params["cwd"].as_str(),
        Some(canonical.as_str()),
        "Windows 上出站 cwd 必须不再等于带前缀的持久化取值（证明 \\\\?\\ 真的被剥掉了）：{params}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
