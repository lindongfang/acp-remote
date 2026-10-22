//! 目录、启动环境与空闲回收（WP5）：查询不启动进程、凭据失败关闭、环境是白名单交集、
//! 回收破坏运行时与映射。

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use acp_core::model::CreateSessionRequest;
use acp_core::model::{
    AgentId, AgentProfile, AgentRef, OwnedSessionRef, PortError, ResourceOrigin, SessionId,
    SessionReference, UnavailableKind,
};
use acp_core::ports::{AgentCatalog, SessionBackendFactory, SessionEndpoint};
use agent_host::{
    AgentHost, HostConfig, limits, runtime_generation, runtime_running, spawn_idle_sweep,
};
use support::{Collector, FAKE_AGENT, TestClock, TestIds, profile_with};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";

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

/// owned 会话引用（`open` 只处理 owned）。
fn owned_ref(session: &SessionId) -> SessionReference {
    SessionReference::Owned(OwnedSessionRef::new(session.clone()))
}

/// 一个唯一的临时文件路径（避免并行测试互相干扰）；随用例结束自动删除。
fn temp_path(tag: &str) -> support::TempFile {
    support::TempFile::new(&format!("acpr-agent-host-{tag}-{}.txt", std::process::id()))
}

/// 一个会被子进程写出的环境快照路径。
fn env_path(tag: &str) -> support::TempFile {
    temp_path(&format!("env-{tag}"))
}

/// 心跳文件路径（子进程存活期间持续追加字节）。
fn heartbeat_path(tag: &str) -> support::TempFile {
    temp_path(&format!("heartbeat-{tag}"))
}

/// 带 `--dump-env` 的 profile：子进程一启动就会把自己的环境写出来。
fn dumping_profile(agent: &str, tag: &str) -> (AgentProfile, support::TempFile) {
    let path = env_path(tag);
    let _ = std::fs::remove_file(&path);
    let text = path.to_string_lossy().into_owned();
    let profile = profile_with(
        agent,
        FAKE_AGENT,
        &["--scenario", "normal", "--dump-env", &text],
    );
    (profile, path)
}

/// 带 `--dump-env` 与 `--heartbeat-file` 的 profile：既能断言环境注入，也能在**进程外**观察存活。
fn dumping_heartbeat_profile(
    agent: &str,
    tag: &str,
) -> (AgentProfile, support::TempFile, support::TempFile) {
    dumping_heartbeat_profile_with(agent, tag, &[])
}

/// 同上，但可在 `normal` 场景之上追加 fake child 的额外参数（`--no-modes` 等）。
fn dumping_heartbeat_profile_with(
    agent: &str,
    tag: &str,
    extra: &[&str],
) -> (AgentProfile, support::TempFile, support::TempFile) {
    let dump = env_path(tag);
    let _ = std::fs::remove_file(&dump);
    let heartbeat = heartbeat_path(tag);
    let _ = std::fs::remove_file(&heartbeat);
    let dump_text = dump.to_string_lossy().into_owned();
    let heartbeat_text = heartbeat.to_string_lossy().into_owned();
    let mut args = vec![
        "--scenario",
        "normal",
        "--dump-env",
        &dump_text,
        "--heartbeat-file",
        &heartbeat_text,
    ];
    args.extend_from_slice(extra);
    let profile = profile_with(agent, FAKE_AGENT, &args);
    (profile, dump, heartbeat)
}

/// 心跳文件当前长度（读不到时按 0）。
fn heartbeat_len(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// 等心跳开始增长（子进程真的活着），最多 `timeout`。
async fn wait_for_heartbeat(path: &std::path::Path, timeout: Duration) -> u64 {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let len = heartbeat_len(path);
        if len > 0 || std::time::Instant::now() >= deadline {
            return len;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 等某个 agent 的进程不再运行（崩溃与退出都是异步收敛的）。
async fn wait_until_not_running(host: &Arc<AgentHost>, agent: &AgentId, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if !runtime_running(host, agent) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    !runtime_running(host, agent)
}

/// 等目录里的进程代变成 `expected`。
///
/// `runtime_generation` 基于 `try_lock`：有并发入口（如周期回收任务）正在改目录时会瞬时读不到，
/// 所以测试不能拿一次读取当断言。
async fn wait_for_generation(
    host: &Arc<AgentHost>,
    agent: &AgentId,
    expected: Option<u64>,
    timeout: Duration,
) -> Option<u64> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let generation = runtime_generation(host, agent);
        if generation == expected || std::time::Instant::now() >= deadline {
            return generation;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 一次 `create`（目录层测试用同一形状的请求）。
async fn create(
    host: &Arc<AgentHost>,
    agent: &str,
    session: &SessionId,
    collector: &Collector,
) -> Result<Box<dyn SessionEndpoint>, PortError> {
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
}

/// 一个最小 prompt（崩溃用例用）。
fn prompt(text: &str) -> acp_core::model::PromptRequest {
    acp_core::model::PromptRequest::new(vec![
        acp_core::model::PromptContentBlock::from_json_text(&format!(
            "{{\"type\":\"text\",\"text\":\"{text}\"}}"
        ))
        .expect("block"),
    ])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_query_never_spawns_a_process() {
    let (profile, dump) = dumping_profile("agent-1", "catalog");
    let host = host(vec![profile]);

    let descriptors = host.agents().await.expect("agents");
    assert_eq!(descriptors.len(), 1);
    assert!(descriptors[0].available, "命令可解析且凭据可用");
    assert_eq!(descriptors[0].origin, ResourceOrigin::Local);
    assert_eq!(descriptors[0].agent.agent_id().as_str(), "agent-1");
    // 目录查询只做只读探测：绝不启动进程。
    assert!(
        !dump.exists(),
        "目录查询不得创建子进程（否则会写出环境快照）"
    );

    // 需要能力时才真的协商（并因此启动进程）。
    let capabilities = host
        .agent_capabilities(&agent_ref("agent-1"))
        .await
        .expect("capabilities");
    assert!(capabilities.is_empty(), "fake child 默认不宣告能力");
    assert!(dump.exists(), "能力协商必须来自真实 initialize");
    let generation = runtime_generation(&host, &AgentId::new("agent-1").expect("id"));
    assert_eq!(generation, Some(1), "第一个进程代是 1");
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn availability_is_per_entry() {
    let good = {
        // 守卫（第二个返回值）绑定到本用例的作用域：即使 `agents()` 真的启动了子进程写出了该文件，
        // 也会在用例结束时被删掉（design D2：守卫的生命周期要覆盖整个测试体）。
        let (profile, _env_guard) = dumping_profile("agent-good", "good");
        profile
    };
    let missing = profile_with(
        "agent-missing",
        "acpr-does-not-exist-anywhere",
        &["--scenario", "normal"],
    );
    let catalog = host(vec![good, missing]);

    let descriptors = catalog.agents().await.expect("agents");
    let availability: Vec<(String, bool)> = descriptors
        .iter()
        .map(|descriptor| {
            (
                descriptor.agent.agent_id().as_str().to_owned(),
                descriptor.available,
            )
        })
        .collect();
    assert!(availability.contains(&("agent-good".to_owned(), true)));
    assert!(availability.contains(&("agent-missing".to_owned(), false)));
}

/// spawn 本身失败（profile 指向的程序不存在）：必须是**明确的不可用类**错误，且不留任何副作用。
///
/// 与 `availability_is_per_entry_and_credentials_fail_closed` 的分工：那条路径测的是目录预检把条目
/// 标成不可用；这里**绕过预检**直接调 `create()`，钉的是真正的 spawn 失败收敛与规格场景
/// 「进程无法启动时返回明确不可用」的三个可观察结果。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawn_failure_is_explicit_unavailable_without_side_effects() {
    let host = host(vec![profile_with(
        "agent-missing",
        "acpr-does-not-exist-anywhere",
        &["--scenario", "normal"],
    )]);
    let agent = AgentId::new("agent-missing").expect("id");
    let session = SessionId::new(SESSION).expect("session");
    let collector = Collector::new();

    // ① 明确且可区分的不可用原因：既不是 `InvalidRequest`（请求非法），也不是
    //    `KeystoreUnavailable`（凭据不可用）或 `Busy`（繁忙/超时），而是
    //    `SessionNotEstablished`——会话建立路径上「可证明 Agent 侧没有产生会话」的那一档
    //    （`session.create` 据此把终态判为 `failed` 而不是 `uncertain`）。
    let failure = match create(&host, "agent-missing", &session, &collector).await {
        Ok(_) => panic!("程序不存在时 spawn 必须失败，而不是给出可用端点"),
        Err(error) => error,
    };
    let kind = match failure {
        PortError::Unavailable(kind) => kind,
        other => panic!("spawn 失败必须是不可用类错误，实际是 {other:?}"),
    };
    assert_eq!(
        kind,
        UnavailableKind::SessionNotEstablished,
        "spawn 失败必须精确到「可证明未建立会话」"
    );

    // ② 无进程副作用：没有半启动的 runtime 留在目录里，也没有留下任何会话映射。
    assert!(
        wait_until_not_running(&host, &agent, Duration::from_secs(5)).await,
        "spawn 失败不得留下运行中的 runtime"
    );
    assert_eq!(
        runtime_generation(&host, &agent),
        None,
        "spawn 失败不得把半启动的 runtime 登记进目录"
    );
    let reopened = host
        .open(owned_ref(&session), Collector::new().sink())
        .await;
    assert!(
        matches!(reopened, Err(PortError::InvalidRequest(_))),
        "spawn 失败不得留下会话映射（`open` 必须报未知会话）"
    );

    // ③ 重试不被毒化：第二次 `create` 仍是干净的 `Unavailable(SessionNotEstablished)`，而不是
    //    `Conflict` 之类的二次错误。
    let retried = match create(&host, "agent-missing", &session, &collector).await {
        Ok(_) => panic!("第二次 create 同样必须失败"),
        Err(error) => error,
    };
    let retried_kind = match retried {
        PortError::Unavailable(kind) => kind,
        other => panic!("重试必须给出同样的不可用类错误，实际是 {other:?}"),
    };
    assert_eq!(
        retried_kind,
        UnavailableKind::SessionNotEstablished,
        "上一次 spawn 失败不得毒化后续重试"
    );

    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn child_environment_is_exactly_the_launch_spec() {
    let (profile, dump) = dumping_profile("agent-1", "env");
    let host = host(vec![profile]);
    let collector = Collector::new();
    let _endpoint = host
        .create(
            &SessionId::new(SESSION).expect("session"),
            CreateSessionRequest::new(
                agent_ref("agent-1"),
                Some(support::workspace()),
                None,
                ResourceOrigin::Local,
            ),
            collector.sink(),
        )
        .await
        .expect("create");

    let text = std::fs::read_to_string(&dump).expect("环境快照");
    let names: BTreeSet<String> = text
        .lines()
        .filter_map(|line| line.split('=').next().map(str::to_owned))
        .collect();
    // 允许的集合恰好是固定的一组必要进程环境（缺失项跳过）。
    let necessary = ["PATH", "HOME", "USERPROFILE", "SystemRoot", "TEMP", "TMP"];
    let expected: BTreeSet<String> = necessary
        .iter()
        .filter(|name| std::env::var_os(name).is_some())
        .map(|name| (*name).to_owned())
        .collect();
    assert_eq!(names, expected, "子进程环境必须与固定规则逐项相等");
    assert!(!names.contains("FAKE_TOKEN"), "不得再注入任何凭据变量");
    // 注：这里曾有 4 条 `!names.contains("ACPR_*")` 断言，但父进程环境里根本没有那些名字（恒真，
    // 给不出任何安全感），已删除。「不得注入节点/设备密钥」由启动边界的**静态拒绝**覆盖：
    // `acpr_prefixed_credential_name_is_refused_before_spawn` 与 `launch.rs` 的单元测试。
    drop(_endpoint);
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_reclaim_needs_timeout_and_never_fires_for_zero() {
    let (profile, dump) = dumping_profile("agent-1", "idle");
    let host = host(vec![profile]);
    let agent = AgentId::new("agent-1").expect("id");
    let session = SessionId::new(SESSION).expect("session");
    let collector = Collector::new();
    let _endpoint = host
        .create(
            &session,
            CreateSessionRequest::new(
                agent_ref("agent-1"),
                Some(support::workspace()),
                None,
                ResourceOrigin::Local,
            ),
            collector.sink(),
        )
        .await
        .expect("create");
    assert!(runtime_running(&host, &agent));

    // `0` = 不因空闲关闭。
    host.sweep_idle(Duration::ZERO).await;
    assert!(runtime_running(&host, &agent), "零值不得触发回收");

    // 空转不到超时也不关闭。
    host.sweep_idle(Duration::from_secs(3600)).await;
    assert!(runtime_running(&host, &agent), "未空闲超时不得关闭");

    // 空闲超时：关闭整棵树。
    host.sweep_idle(Duration::from_nanos(1)).await;
    assert!(!runtime_running(&host, &agent), "空闲超时必须关闭进程");

    // 回收后再次访问会启动新一代进程（能力缓存不得跨代复用）。
    let _ = host
        .agent_capabilities(&agent_ref("agent-1"))
        .await
        .expect("capabilities");
    assert_eq!(runtime_generation(&host, &agent), Some(2));
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
}

/// 白名单是注入上限：端口即使返回了白名单外的变量，也必须在 spawn 之前失败关闭。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_reclaim_also_applies_to_processes_without_sessions() {
    let host = host(vec![profile_with(
        "agent-1",
        FAKE_AGENT,
        &["--scenario", "normal"],
    )]);
    let agent = AgentId::new("agent-1").expect("id");
    let _ = host
        .agent_capabilities(&agent_ref("agent-1"))
        .await
        .expect("capabilities");
    assert!(runtime_running(&host, &agent), "协商会启动进程");
    host.sweep_idle(Duration::ZERO).await;
    assert!(runtime_running(&host, &agent), "零值不回收");
    host.sweep_idle(Duration::from_nanos(1)).await;
    assert!(
        !runtime_running(&host, &agent),
        "无活动会话的进程也必须按空闲超时回收"
    );
    host.shutdown_all().await;
}

/// 回收必须**破坏运行时与映射**：运行时不再挂在目录里、既有映射不得被 `open` 复用，且进程真的结束
/// （进程外证据：心跳文件不再增长）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reclaim_invalidates_runtime_and_session_mappings() {
    let (profile, dump, heartbeat) = dumping_heartbeat_profile("agent-1", "reclaim");
    let host = host(vec![profile]);
    let agent = AgentId::new("agent-1").expect("id");
    let session = SessionId::new(SESSION).expect("session");
    let collector = Collector::new();
    let _endpoint = create(&host, "agent-1", &session, &collector)
        .await
        .expect("create");
    assert_eq!(runtime_generation(&host, &agent), Some(1), "首个进程代是 1");
    let started = wait_for_heartbeat(&heartbeat, Duration::from_secs(10)).await;
    assert!(started > 0, "子进程必须先真的写出心跳");

    // 先让空闲时钟越过超时，再回收。
    tokio::time::sleep(Duration::from_millis(200)).await;
    host.sweep_idle(Duration::from_millis(100)).await;

    // ① 运行时不再存在于目录里（不是「还挂着死 supervisor」）
    assert_eq!(
        runtime_generation(&host, &agent),
        None,
        "回收必须把运行时移出目录（破坏映射）"
    );
    assert!(!runtime_running(&host, &agent));

    // ② 进程外证据：心跳文件不再增长（不依赖 `runtime_running` 的进程内状态位）
    let after_reclaim = heartbeat_len(&heartbeat);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        heartbeat_len(&heartbeat),
        after_reclaim,
        "回收后进程必须真的结束（心跳不得继续增长）"
    );

    // ③ 既有的 core 会话映射不得被复用：`open` 必须显式失败，而不是复活一个已关闭的端点
    let refused = host
        .open(owned_ref(&session), Collector::new().sink())
        .await;
    assert!(
        matches!(refused, Err(PortError::InvalidRequest(_))),
        "回收后的既有会话映射必须显式失败，不得复用"
    );

    // ④ 目录查询不得 panic，可用性语义不随运行状态变化（仍只由命令与凭据决定）
    let descriptors = host.agents().await.expect("agents");
    assert_eq!(descriptors.len(), 1);
    assert!(
        descriptors[0].available,
        "可用性不得因回收而改变（只由命令可解析 + 凭据可解析决定）"
    );

    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
    let _ = std::fs::remove_file(&heartbeat);
}

/// 空闲时钟必须覆盖**出站活动**：只改模式、不 prompt 的会话不得在超时前被回收。
///
/// 钉法的分工：fake child 在 `session/set_mode` 之后会紧跟一条 `current_mode_update` 通知，而通知入站
/// 也会刷新同一时钟 ⇒ 本用例无法单独证明 `set_mode` 的 `touch()`。真正钉住那次刷新的是
/// `idle_clock_is_refreshed_by_set_mode_without_declared_modes`（未宣告模式的早退路径上唯一刷新点）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_clock_covers_outbound_session_activity() {
    let (profile, dump, heartbeat) = dumping_heartbeat_profile("agent-1", "activity");
    let host = host(vec![profile]);
    let agent = AgentId::new("agent-1").expect("id");
    let endpoint = create(
        &host,
        "agent-1",
        &SessionId::new(SESSION).expect("session"),
        &Collector::new(),
    )
    .await
    .expect("create");
    assert!(wait_for_heartbeat(&heartbeat, Duration::from_secs(10)).await > 0);

    // `modes()` 自己也会刷新空闲时钟，因此必须在窗口**之前**调用；窗口内只留 `set_mode`。
    let modes = endpoint.modes().await.expect("modes");
    assert!(!modes.available.is_empty(), "fake child 默认宣告可用模式");
    let plan = acp_core::model::ModeId::new("plan").expect("mode id");
    assert!(
        modes.available.iter().any(|mode| mode.mode_id() == &plan),
        "固定 mode id 必须来自 Agent 给出的候选"
    );
    // 先让会话空闲超过下面的超时，再用 `set_mode` 刷新活动时钟。
    tokio::time::sleep(Duration::from_millis(300)).await;
    endpoint.set_mode(&plan).await.expect("set_mode");

    host.sweep_idle(Duration::from_millis(200)).await;
    assert_eq!(
        runtime_generation(&host, &agent),
        Some(1),
        "刚做过 set_mode 的会话不得在超时前被回收"
    );

    // 活动时钟没有被弄坏：之后真的空闲超过超时时仍必须回收，且进程真的结束。
    tokio::time::sleep(Duration::from_millis(400)).await;
    host.sweep_idle(Duration::from_millis(200)).await;
    assert_eq!(
        runtime_generation(&host, &agent),
        None,
        "空闲超时后仍要回收"
    );
    let after_reclaim = heartbeat_len(&heartbeat);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        heartbeat_len(&heartbeat),
        after_reclaim,
        "回收后进程必须真的结束（心跳不得继续增长）"
    );
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
    let _ = std::fs::remove_file(&heartbeat);
}

/// 单点刷新断言：窗口内只有 `set_mode` 一个刷新点（`--no-modes` 让 Agent 不宣告模式，早退路径
/// 不发任何消息 ⇒ 没有入站通知再刷新时钟）。删掉 `Endpoint::set_mode` 的 `touch()` 会让本用例变红。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_clock_is_refreshed_by_set_mode_without_declared_modes() {
    let (profile, dump, heartbeat) =
        dumping_heartbeat_profile_with("agent-1", "activity-set-mode", &["--no-modes"]);
    let host = host(vec![profile]);
    let agent = AgentId::new("agent-1").expect("id");
    let endpoint = create(
        &host,
        "agent-1",
        &SessionId::new(SESSION).expect("session"),
        &Collector::new(),
    )
    .await
    .expect("create");
    assert!(wait_for_heartbeat(&heartbeat, Duration::from_secs(10)).await > 0);
    assert!(
        endpoint.modes().await.expect("modes").available.is_empty(),
        "`--no-modes` 必须造出「未宣告」路径"
    );

    // 先让会话空闲超过下面的超时，再用 `set_mode`（这一次是唯一的刷新点）刷新。
    tokio::time::sleep(Duration::from_millis(300)).await;
    let refused = endpoint
        .set_mode(&acp_core::model::ModeId::new("plan").expect("mode"))
        .await;
    assert!(refused.is_err(), "未宣告模式必须显式拒绝（且不发消息）");

    host.sweep_idle(Duration::from_millis(200)).await;
    assert_eq!(
        runtime_generation(&host, &agent),
        Some(1),
        "刚调用过 set_mode 的会话不得在超时前被回收"
    );

    // 活动时钟没有被弄坏：之后真的空闲超过超时时仍必须回收，且进程真的结束。
    tokio::time::sleep(Duration::from_millis(400)).await;
    host.sweep_idle(Duration::from_millis(200)).await;
    assert_eq!(
        runtime_generation(&host, &agent),
        None,
        "空闲超时后仍要回收"
    );
    let after_reclaim = heartbeat_len(&heartbeat);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        heartbeat_len(&heartbeat),
        after_reclaim,
        "回收后进程必须真的结束（心跳不得继续增长）"
    );
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
    let _ = std::fs::remove_file(&heartbeat);
}

/// 单点刷新断言：fake child 收到 `session/set_config_option` 只回空 result、不发通知，
/// 因此窗口内只有 `set_config` 一个刷新点。删掉它的 `touch()` 会让本用例变红。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_clock_is_refreshed_by_set_config() {
    let (profile, dump, heartbeat) = dumping_heartbeat_profile("agent-1", "activity-set-config");
    let host = host(vec![profile]);
    let agent = AgentId::new("agent-1").expect("id");
    let endpoint = create(
        &host,
        "agent-1",
        &SessionId::new(SESSION).expect("session"),
        &Collector::new(),
    )
    .await
    .expect("create");
    assert!(wait_for_heartbeat(&heartbeat, Duration::from_secs(10)).await > 0);

    tokio::time::sleep(Duration::from_millis(300)).await;
    endpoint
        .set_config(
            &acp_core::model::ConfigOptionId::new("verbose").expect("option"),
            support::boolean(true),
        )
        .await
        .expect("set_config");

    host.sweep_idle(Duration::from_millis(200)).await;
    assert_eq!(
        runtime_generation(&host, &agent),
        Some(1),
        "刚写过配置的会话不得在超时前被回收"
    );

    tokio::time::sleep(Duration::from_millis(400)).await;
    host.sweep_idle(Duration::from_millis(200)).await;
    assert_eq!(
        runtime_generation(&host, &agent),
        None,
        "空闲超时后仍要回收"
    );
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
    let _ = std::fs::remove_file(&heartbeat);
}

/// 单点刷新断言：没有进行中的 turn 时 `cancel` 是幂等空操作、不发任何消息，
/// 因此窗口内只有 `cancel_turn` 一个刷新点。删掉它的 `touch()` 会让本用例变红。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_clock_is_refreshed_by_cancel_turn() {
    let (profile, dump, heartbeat) = dumping_heartbeat_profile("agent-1", "activity-cancel");
    let host = host(vec![profile]);
    let agent = AgentId::new("agent-1").expect("id");
    let endpoint = create(
        &host,
        "agent-1",
        &SessionId::new(SESSION).expect("session"),
        &Collector::new(),
    )
    .await
    .expect("create");
    assert!(wait_for_heartbeat(&heartbeat, Duration::from_secs(10)).await > 0);

    tokio::time::sleep(Duration::from_millis(300)).await;
    endpoint
        .cancel(None)
        .await
        .expect("无 turn 时取消是幂等空操作");

    host.sweep_idle(Duration::from_millis(200)).await;
    assert_eq!(
        runtime_generation(&host, &agent),
        Some(1),
        "刚取消过的会话不得在超时前被回收"
    );

    tokio::time::sleep(Duration::from_millis(400)).await;
    host.sweep_idle(Duration::from_millis(200)).await;
    assert_eq!(
        runtime_generation(&host, &agent),
        None,
        "空闲超时后仍要回收"
    );
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
    let _ = std::fs::remove_file(&heartbeat);
}

/// 已关闭的会话不得阻塞空闲回收：成功关闭会摘除映射并标记关闭；即便目录里留下已关闭绑定，
/// 判据也不得把它算作「不空闲」，否则只要 `by_acp` 里留着它就永远回收不掉这个 runtime（进程泄漏）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closed_session_does_not_block_idle_reclaim() {
    let (profile, dump, heartbeat) = dumping_heartbeat_profile_with(
        "agent-1",
        "closed",
        &["--capabilities", r#"{"sessionCapabilities":{"close":{}}}"#],
    );
    let host = host(vec![profile]);
    let agent = AgentId::new("agent-1").expect("id");
    let session = SessionId::new(SESSION).expect("session");
    let endpoint = create(&host, "agent-1", &session, &Collector::new())
        .await
        .expect("create");
    assert!(wait_for_heartbeat(&heartbeat, Duration::from_secs(10)).await > 0);
    assert_eq!(runtime_generation(&host, &agent), Some(1));

    // 真实关闭：只有 Agent ack 后才摘除映射并标记关闭（旧映射不可再被 `open` 复用）。
    endpoint.close().await.expect("close");
    assert!(
        host.open(owned_ref(&session), Collector::new().sink())
            .await
            .is_err(),
        "关闭成功后旧映射必须不可用"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    host.sweep_idle(Duration::from_millis(200)).await;
    assert_eq!(
        runtime_generation(&host, &agent),
        None,
        "已关闭的会话不得阻塞空闲回收（否则进程泄漏）"
    );

    // 进程外证据：回收后进程真的结束。
    let after_reclaim = heartbeat_len(&heartbeat);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        heartbeat_len(&heartbeat),
        after_reclaim,
        "回收后进程必须真的结束（心跳不得继续增长）"
    );
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
    let _ = std::fs::remove_file(&heartbeat);
}

/// 进程已退出但运行时仍留在目录里的窗口：再次访问必须作废旧运行时、清掉映射并**重建**为新的
/// 进程代；崩溃前的旧端点与旧映射都不得跨代复用。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exited_runtime_is_rebuilt_as_a_new_generation() {
    let dump = env_path("stale-runtime");
    let _ = std::fs::remove_file(&dump);
    let dump_text = dump.to_string_lossy().into_owned();
    let profile = profile_with(
        "agent-1",
        FAKE_AGENT,
        &["--scenario", "crash-on-prompt", "--dump-env", &dump_text],
    );
    let host = host(vec![profile]);
    let agent = AgentId::new("agent-1").expect("id");
    let session = SessionId::new(SESSION).expect("session");
    let first = create(&host, "agent-1", &session, &Collector::new())
        .await
        .expect("create");
    assert_eq!(runtime_generation(&host, &agent), Some(1));

    // 让子进程崩溃：运行时与映射都还在目录里，只有 supervisor 已经不在运行。
    let _ = first.prompt(prompt("会崩"), support::timestamp()).await;
    assert!(
        wait_until_not_running(&host, &agent, Duration::from_secs(10)).await,
        "子进程必须先退出"
    );
    assert_eq!(
        runtime_generation(&host, &agent),
        Some(1),
        "窗口：目录里仍是已退出的运行时"
    );
    // 删掉第一代写的环境快照：新一代真的启动时它会重新出现（进程外证据）。
    let _ = std::fs::remove_file(&dump);

    // 再次访问同一条目：作废 + 清映射 + 关闭旧树，然后按新一代启动。
    let _ = host
        .agent_capabilities(&agent_ref("agent-1"))
        .await
        .expect("capabilities");
    assert_eq!(
        runtime_generation(&host, &agent),
        Some(2),
        "已退出的运行时必须被重建为新的进程代"
    );
    assert!(
        dump.exists(),
        "新一代必须真的启动了子进程（环境快照重新出现）"
    );

    // 旧端点与旧映射都不得跨代复用：作废路径必须先 `close_session`，旧端点才会以「会话已关闭」
    // （`PortError::InvalidRequest`）而不是「进程已退出」（`Unavailable`）失败。
    let reused = match first.prompt(prompt("旧端点"), support::timestamp()).await {
        Ok(_) => panic!("崩溃前的旧端点必须显式失败"),
        Err(error) => error,
    };
    assert!(
        matches!(reused, PortError::InvalidRequest(_)),
        "作废旧运行时必须先关掉旧端点（否则它只会报进程已退出）：{reused:?}"
    );
    let reused = host
        .open(owned_ref(&session), Collector::new().sink())
        .await;
    assert!(
        matches!(reused, Err(PortError::InvalidRequest(_))),
        "作废旧运行时必须同时清掉会话映射"
    );
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
}

/// 未知 core 会话号：`open` 必须显式失败，且不得因此拉起进程。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_unknown_session_is_an_explicit_error() {
    let (profile, dump) = dumping_profile("agent-1", "open-unknown");
    let host = host(vec![profile]);
    let agent = AgentId::new("agent-1").expect("id");
    let unknown = SessionId::new("99999999-9999-4999-8999-999999999999").expect("session");

    let outcome = host
        .open(owned_ref(&unknown), Collector::new().sink())
        .await;
    assert!(
        matches!(outcome, Err(PortError::InvalidRequest(_))),
        "未知会话必须是显式参数类错误"
    );
    assert_eq!(
        runtime_generation(&host, &agent),
        None,
        "`open` 不得拉起进程"
    );
    assert!(!dump.exists(), "`open` 不得因未知会话而启动进程");
    host.shutdown_all().await;
}

/// 进程已退出、运行时还留在目录里的窗口：`open` 不得把死 supervisor 当成活跃端点复活。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_refuses_a_runtime_whose_process_has_exited() {
    let host = host(vec![profile_with(
        "agent-1",
        FAKE_AGENT,
        &["--scenario", "crash-on-prompt"],
    )]);
    let agent = AgentId::new("agent-1").expect("id");
    let session = SessionId::new(SESSION).expect("session");
    let endpoint = create(&host, "agent-1", &session, &Collector::new())
        .await
        .expect("create");
    // 让子进程崩溃：运行时与映射都还在目录里，只有 supervisor 已经不在运行。
    let _ = endpoint.prompt(prompt("会崩"), support::timestamp()).await;
    assert!(
        wait_until_not_running(&host, &agent, Duration::from_secs(10)).await,
        "子进程必须先退出"
    );

    let outcome = host
        .open(owned_ref(&session), Collector::new().sink())
        .await;
    assert!(outcome.is_err(), "死运行时上的既有映射不得被复活");
    // 命中的是「运行时仍在目录、但进程已退出」这条路径（而不是「没有映射」那条）
    assert_eq!(runtime_generation(&host, &agent), Some(1));
    host.shutdown_all().await;
}

/// 单独关闭一个 agent：另一个不受影响；被关的 agent 再 `create` 会重新启动（新进程代）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_agent_closes_one_agent_without_touching_others() {
    let (profile_a, dump_a) = dumping_profile("agent-a", "shutdown-a");
    let (profile_b, dump_b) = dumping_profile("agent-b", "shutdown-b");
    let host = host(vec![profile_a, profile_b]);
    let agent_a = AgentId::new("agent-a").expect("id");
    let agent_b = AgentId::new("agent-b").expect("id");
    let _ = host
        .agent_capabilities(&agent_ref("agent-a"))
        .await
        .expect("capabilities a");
    let _ = host
        .agent_capabilities(&agent_ref("agent-b"))
        .await
        .expect("capabilities b");
    assert_eq!(runtime_generation(&host, &agent_a), Some(1));

    host.shutdown_agent(&agent_a).await;
    assert_eq!(
        runtime_generation(&host, &agent_a),
        None,
        "被关的 agent 不得再持有运行时"
    );
    assert_eq!(
        runtime_generation(&host, &agent_b),
        Some(1),
        "另一个 agent 的运行时不受影响"
    );
    assert!(runtime_running(&host, &agent_b));
    // 可用性语义不随关闭变化（仍只由命令与凭据的解析决定）。
    let descriptors = host.agents().await.expect("agents");
    assert!(descriptors.iter().all(|descriptor| descriptor.available));

    // 再 create 会重新启动：新的进程代（旧 endpoint 与旧映射都不跨代复用）。
    let _endpoint = create(
        &host,
        "agent-a",
        &SessionId::new(SESSION).expect("session"),
        &Collector::new(),
    )
    .await
    .expect("restart");
    assert_eq!(
        runtime_generation(&host, &agent_a),
        Some(2),
        "重新启动必须是新的进程代"
    );
    host.shutdown_all().await;
    for path in [dump_a, dump_b] {
        let _ = std::fs::remove_file(path);
    }
}

/// 周期空闲回收任务：能启动，且 `shutdown_all` 后不再产生任何进程。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_sweep_task_converges_on_shutdown_all() {
    let (profile, dump) = dumping_profile("agent-1", "sweep");
    let host = host(vec![profile]);
    let agent = AgentId::new("agent-1").expect("id");
    // 超时给得足够大：本轮我们只关心「关闭后不再有进程」，不关心具体调度时序。
    let handle = spawn_idle_sweep(Arc::clone(&host), Duration::from_secs(3600));
    let _ = host
        .agent_capabilities(&agent_ref("agent-1"))
        .await
        .expect("capabilities");
    assert_eq!(
        wait_for_generation(&host, &agent, Some(1), Duration::from_secs(5)).await,
        Some(1)
    );
    assert!(dump.exists(), "第一代进程必须写过环境快照");
    // 删掉快照：只有**真的**又拉起了进程（无论由周期任务还是面板调用）它才会重新出现。
    let _ = std::fs::remove_file(&dump);

    host.shutdown_all().await;
    // 至少让周期任务再走一圈（间隔见 `limits::IDLE_SWEEP_INTERVAL`）：不得把已关闭的 agent 拉起来。
    tokio::time::sleep(limits::IDLE_SWEEP_INTERVAL + Duration::from_millis(300)).await;
    // 多次采样：关闭后目录必须**持续**为空（单次 `None` 可能只是 `try_lock` 没拿到锁）。
    for _ in 0..3 {
        assert_eq!(
            runtime_generation(&host, &agent),
            None,
            "关闭后周期任务不得重启进程"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!runtime_running(&host, &agent));

    // 手动再跑一轮（等价于下一次 tick）+ 一次显式请求：同样不得启动新进程。
    host.sweep_idle(Duration::from_nanos(1)).await;
    assert_eq!(runtime_generation(&host, &agent), None);
    let refused = create(
        &host,
        "agent-1",
        &SessionId::new(SESSION).expect("session"),
        &Collector::new(),
    )
    .await;
    assert!(
        matches!(refused, Err(PortError::Unavailable(_))),
        "关闭中必须显式拒绝启动，而不是静默超时或悄悄拉起进程"
    );
    // 进程外证据：关闭后任何路径都不得再拉起进程（否则子进程会重写被删掉的环境快照）。
    assert!(
        !dump.exists(),
        "关闭后不得再启动进程（`--dump-env` 快照不得重新出现）"
    );

    handle.abort();
    let _ = handle.await;
    let _ = std::fs::remove_file(&dump);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminate_agent_ends_the_runtime_and_the_next_create_spawns_a_new_generation() {
    const SECOND: &str = "22222222-2222-4222-8222-222222222222";
    let (profile, dump) = dumping_profile("agent-1", "terminate");
    let host = host(vec![profile]);
    let agent = AgentId::new("agent-1").expect("id");
    let session = SessionId::new(SESSION).expect("session");
    let collector = Collector::new();
    let endpoint = create(&host, "agent-1", &session, &collector)
        .await
        .expect("create");
    assert!(runtime_running(&host, &agent), "创建成功后进程在跑");
    assert_eq!(runtime_generation(&host, &agent), Some(1));

    host.terminate_agent(&session).await.expect("terminate");

    assert!(
        !runtime_running(&host, &agent),
        "结束之后该运行时不得仍在运行（不得再被复用）"
    );
    assert_eq!(
        runtime_generation(&host, &agent),
        None,
        "运行时必须已从运行目录里摘掉"
    );
    let stale = endpoint.prompt(prompt("残端"), support::timestamp()).await;
    assert!(
        stale.is_err(),
        "被结束的端点必须拒绝写入，而不是看起来还活着：{stale:?}"
    );
    // 幂等：同一个会话没有运行时是成功的空操作。
    host.terminate_agent(&session)
        .await
        .expect("terminate again");

    // 再次创建：新的一代（不得复用被结束的那个运行时）。
    let second = SessionId::new(SECOND).expect("session");
    let _endpoint = create(&host, "agent-1", &second, &Collector::new())
        .await
        .expect("re-create");
    assert_eq!(
        runtime_generation(&host, &agent),
        Some(2),
        "被结束的运行时不得被复用：下一次创建必须拉起新一代进程"
    );
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
}
