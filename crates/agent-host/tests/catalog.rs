//! 目录、启动环境与空闲回收（WP5）：查询不启动进程、凭据失败关闭、环境是白名单交集。

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use acp_core::model::CreateSessionRequest;
use acp_core::model::{AgentId, AgentProfile, AgentRef, PortError, ResourceOrigin, SessionId};
use acp_core::ports::{AgentCatalog, SessionBackendFactory};
use agent_host::{AgentHost, HostConfig, runtime_generation, runtime_running};
use support::{
    Collector, FAKE_AGENT, FakeConfig, FakeCredentials, TestClock, TestIds, profile_with,
};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";

fn host(profiles: Vec<AgentProfile>, credentials: FakeCredentials) -> Arc<AgentHost> {
    Arc::new(AgentHost::new(
        Arc::new(FakeConfig::new(profiles)),
        Arc::new(credentials),
        HostConfig::default(),
        Arc::new(TestIds::new()),
        Arc::new(TestClock::new()),
    ))
}

fn agent_ref(agent: &str) -> AgentRef {
    AgentRef::try_new(AgentId::new(agent).expect("id"), &format!("Agent {agent}")).expect("ref")
}

/// 一个会被子进程写出的环境快照路径（唯一，避免并行测试互相干扰）。
fn env_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "acpr-agent-host-env-{tag}-{}.txt",
        std::process::id()
    ))
}

/// 带 `--dump-env` 的 profile：子进程一启动就会把自己的环境写出来。
fn dumping_profile(agent: &str, tag: &str) -> (AgentProfile, std::path::PathBuf) {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_query_never_spawns_a_process() {
    let (profile, dump) = dumping_profile("agent-1", "catalog");
    let host = host(vec![profile], FakeCredentials::ok());

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
async fn availability_is_per_entry_and_credentials_fail_closed() {
    let good = {
        let (profile, _) = dumping_profile("agent-good", "good");
        profile
    };
    let missing = profile_with(
        "agent-missing",
        "acpr-does-not-exist-anywhere",
        &["--scenario", "normal"],
    );
    let catalog = host(vec![good, missing], FakeCredentials::ok());

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

    // 凭据引用失效：只让**该条目**不可用，且启动必须失败关闭（进程不启动）。
    let broken = host(
        vec![dumping_profile("agent-broken", "broken").0],
        FakeCredentials::failing(),
    );
    let descriptors = broken.agents().await.expect("agents");
    assert!(!descriptors[0].available, "凭据不可用时条目必须不可用");
    let error = broken
        .create(
            &SessionId::new(SESSION).expect("session"),
            CreateSessionRequest::new(
                agent_ref("agent-broken"),
                Some(support::workspace()),
                None,
                ResourceOrigin::Local,
            ),
            Collector::new().sink(),
        )
        .await;
    let error = match error {
        Ok(_) => panic!("凭据失败时不得启动进程"),
        Err(error) => error,
    };
    assert!(matches!(error, PortError::Unavailable(_)));
    let dump = env_path("broken");
    assert!(
        !dump.exists(),
        "凭据解析失败必须在 spawn 之前失败关闭（进程不得启动）"
    );
    let _ = std::fs::remove_file(&dump);
    broken.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn child_environment_is_exactly_the_launch_spec() {
    let (profile, dump) = dumping_profile("agent-1", "env");
    let host = host(vec![profile], FakeCredentials::ok());
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
    // 允许的集合恰好是「白名单里的凭据绑定 + 必要的进程环境」。
    let allowed: BTreeSet<String> = [
        "FAKE_TOKEN",
        "PATH",
        "SystemRoot",
        "TEMP",
        "TMP",
        "HOME",
        "USERPROFILE",
    ]
    .iter()
    .map(|name| (*name).to_owned())
    .collect();
    let unexpected: Vec<&String> = names.difference(&allowed).collect();
    assert!(
        unexpected.is_empty(),
        "子进程环境不得包含未注入的变量：{unexpected:?}"
    );
    assert!(names.contains("FAKE_TOKEN"), "白名单内的凭据必须注入");
    // 节点/设备密钥绝不能出现在 Agent 的环境里。
    for forbidden in [
        "ACPR_NODE_KEY",
        "ACPR_DEVICE_KEY",
        "ACPR_NODE_SIGNING_KEY",
        "ACPR_DEVICE_PRIVATE_KEY",
    ] {
        assert!(!names.contains(forbidden), "{forbidden} 不得进入子进程");
    }
    drop(_endpoint);
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn profile_selection_ignores_startup_configuration_files() {
    // 启动配置文件（同名条目）必须**不被使用**：profile 只来自 `LocalConfigStore`。
    let config = std::env::temp_dir().join(format!(
        "acpr-agent-host-config-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &config,
        r#"{"agents":{"agent-1":{"command":"acpr-from-config-file","args":[]}}}"#,
    )
    .expect("写配置文件");

    let (profile, dump) = dumping_profile("agent-1", "config");
    let host = host(vec![profile], FakeCredentials::ok());
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
    // 子进程是我们 profile 里那个 fake child（配置文件里的命令根本不会被执行）。
    assert!(
        dump.exists(),
        "必须使用 LocalConfigStore 的 profile（配置文件的同名条目不得生效）"
    );
    let dumped = std::fs::read_to_string(&dump).expect("环境快照");
    assert!(!dumped.contains("acpr-from-config-file"));
    drop(_endpoint);
    host.shutdown_all().await;
    let _ = std::fs::remove_file(&dump);
    let _ = std::fs::remove_file(&config);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn idle_reclaim_needs_timeout_and_never_fires_for_zero() {
    let (profile, dump) = dumping_profile("agent-1", "idle");
    let host = host(vec![profile], FakeCredentials::ok());
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
async fn credential_variable_outside_the_allowlist_is_refused_before_spawn() {
    let (profile, dump) = dumping_profile("agent-1", "leak");
    let host = host(vec![profile], FakeCredentials::leaking());
    let outcome = host
        .create(
            &SessionId::new(SESSION).expect("session"),
            CreateSessionRequest::new(
                agent_ref("agent-1"),
                Some(support::workspace()),
                None,
                ResourceOrigin::Local,
            ),
            Collector::new().sink(),
        )
        .await;
    assert!(outcome.is_err(), "白名单外的凭据变量必须被拒绝");
    assert!(
        !dump.exists(),
        "拒绝必须发生在 spawn 之前（子进程不得启动）"
    );
    let _ = std::fs::remove_file(&dump);
    host.shutdown_all().await;
}
