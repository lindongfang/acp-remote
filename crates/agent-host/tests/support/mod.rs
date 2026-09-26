#![allow(dead_code)] // 共享测试支撑：各测试目标只用到其中一部分

//! 测试支撑：fake ACP child 的启动、端口 fake 与事件收集。
//!
//! 未被本 crate 消费的端口方法一律 `todo!()`：真被调用时测试会立刻炸掉，而不是给出看似合理的空结果。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use acp_core::model::Digest;
use acp_core::model::{
    AgentId, AgentProfile, ConfigValue, ProviderEnvBinding, ProviderRef, ResolvedWorkspace,
    SecretValue, Timestamp, WorkspaceAlias, WorkspaceRecord,
};
use acp_core::ports::{
    Clock, CredentialResolver, EventSink, IdGenerator, LocalConfigStore, ProfileWrite,
    ProviderRefWrite, SeedWrite, WorkspaceWrite,
};
use agent_host::LaunchSpec;

/// fake ACP child 的可执行文件（由 cargo 在测试构建时提供）。
pub const FAKE_AGENT: &str = env!("CARGO_BIN_EXE_acpr-fake-acp-agent");

/// 用例自建临时**文件**的守卫：`Drop` 时 `remove_file`（正常结束与 panic 展开两条路径都生效）。
///
/// 文件由用例或 fake ACP child 进程写出，守卫只持有路径（**不**创建文件）。`Deref<Target = Path>`
/// 让 `path.exists()`、`path.to_string_lossy()`、`&path`（`&Path` 形参）照常工作；`AsRef<Path>` 让
/// `std::fs::remove_file(&path)` / `remove_file(path)` 这类泛型入参也直接收。
pub struct TempFile {
    path: std::path::PathBuf,
}

impl TempFile {
    /// 在系统临时目录下取一个唯一文件路径（**不**创建文件；`name` 必须已含 pid/sequence 等成分）。
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            path: std::env::temp_dir().join(name),
        }
    }
}

impl std::ops::Deref for TempFile {
    type Target = std::path::Path;

    fn deref(&self) -> &std::path::Path {
        &self.path
    }
}

impl AsRef<std::path::Path> for TempFile {
    fn as_ref(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // 尽力而为，且**不 panic**（展开中 panic 会 abort）：文件可能已被用例自己删掉（`NotFound`
        // 立即返回），Windows 上子进程刚写完/刚退出时也可能瞬时占用——此时重试若干次。
        for _ in 0..10 {
            match std::fs::remove_file(&self.path) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }
    }
}

/// 造一个指向 fake child 的 `LaunchSpec`（环境只留必要的进程项）。
#[must_use]
pub fn launch_spec(args: &[&str]) -> LaunchSpec {
    let mut env: Vec<(String, String)> = Vec::new();
    for name in ["PATH", "SystemRoot", "TEMP", "TMP", "HOME", "USERPROFILE"] {
        if let Ok(value) = std::env::var(name) {
            env.push((name.to_owned(), value));
        }
    }
    LaunchSpec {
        program: FAKE_AGENT.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        env,
    }
}

/// 运行 fake child 的一个场景。
#[must_use]
pub fn scenario(name: &str) -> LaunchSpec {
    launch_spec(&["--scenario", name])
}

/// 带额外参数的场景。
#[must_use]
pub fn scenario_with(name: &str, extra: &[&str]) -> LaunchSpec {
    let mut args = vec!["--scenario", name];
    args.extend_from_slice(extra);
    launch_spec(&args)
}

/// 收集 `EndpointEvent` 的 sink。
#[derive(Debug, Default, Clone)]
pub struct Collector {
    events: Arc<Mutex<Vec<acp_core::model::EndpointEvent>>>,
}

impl Collector {
    /// 新的收集器。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 造一个 `EventSink`。
    #[must_use]
    pub fn sink(&self) -> EventSink {
        let events = Arc::clone(&self.events);
        EventSink::new(move |event| {
            if let Ok(mut guard) = events.lock() {
                guard.push(event);
            }
        })
    }

    /// 已收到的事件数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.lock().map(|guard| guard.len()).unwrap_or(0)
    }

    /// 是否还没有事件。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 全部事件的快照。
    #[must_use]
    pub fn snapshot(&self) -> Vec<acp_core::model::EndpointEvent> {
        self.events
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// 快照里某个 `event_type` 的个数。
    #[must_use]
    pub fn count(&self, event_type: &str) -> usize {
        self.snapshot()
            .iter()
            .filter(|event| event.event_type.as_str() == event_type)
            .count()
    }

    /// 快照里某个 `event_type` 第一条事件的 view 文本。
    #[must_use]
    pub fn first_view(&self, event_type: &str) -> Option<String> {
        self.snapshot()
            .into_iter()
            .find(|event| event.event_type.as_str() == event_type)
            .map(|event| event.payload.view.as_str().to_owned())
    }

    /// 快照里出现的全部事件类型（去重、稳定顺序）。
    #[must_use]
    pub fn event_types(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut types = Vec::new();
        for event in self.snapshot() {
            let name = event.event_type.as_str().to_owned();
            if seen.insert(name.clone()) {
                types.push(name);
            }
        }
        types
    }

    /// 等待事件数量达到 `count`（带超时，避免测试悬挂）。
    pub async fn wait_for(&self, count: usize, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if self.len() >= count {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        self.len() >= count
    }

    /// 等待出现某个 `event_type`。
    pub async fn wait_for_type(&self, event_type: &str, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if self.count(event_type) > 0 {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        self.count(event_type) > 0
    }
}

/// 单调时钟：每次读都推进 1 ms。
#[derive(Debug, Default)]
pub struct TestClock {
    counter: Mutex<u64>,
}

impl TestClock {
    /// 新的时钟。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        let mut counter = self
            .counter
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *counter += 1;
        let millis = *counter;
        Timestamp::new(&format!(
            "2026-09-24T10:00:{:02}.{:03}Z",
            (millis / 1000) % 60,
            millis % 1000
        ))
        .expect("时间格式")
    }
}

/// 递增的标识分配器（形态合法即可，测试不依赖随机性）。
#[derive(Debug, Default)]
pub struct TestIds {
    counter: Mutex<u64>,
}

impl TestIds {
    /// 新的分配器。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn next(&self) -> String {
        let mut counter = self
            .counter
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *counter += 1;
        format!("00000000-0000-4000-8000-{:012}", *counter)
    }
}

impl IdGenerator for TestIds {
    fn turn_id(&self) -> acp_core::model::TurnId {
        acp_core::model::TurnId::new(&self.next()).expect("turn id")
    }

    fn interaction_id(&self) -> acp_core::model::InteractionId {
        acp_core::model::InteractionId::new(&self.next()).expect("interaction id")
    }

    fn pairing_id(&self) -> acp_core::model::PairingId {
        acp_core::model::PairingId::new(&self.next()).expect("pairing id")
    }

    fn message_id(&self) -> acp_core::model::MessageId {
        acp_core::model::MessageId::new(&self.next()).expect("message id")
    }

    fn origin_epoch(&self) -> acp_core::model::OriginEpoch {
        acp_core::model::OriginEpoch::new(&self.next()).expect("origin epoch")
    }

    fn request_id(&self) -> acp_core::model::RequestId {
        acp_core::model::RequestId::new(&self.next()).expect("request id")
    }
}

/// 一个合法的 `Timestamp`。
#[must_use]
pub fn timestamp() -> Timestamp {
    Timestamp::new("2026-09-24T10:00:00.000Z").expect("timestamp")
}

/// 一个 Agent profile（命令可指向 fake child，也可指向不存在的命令）。
#[must_use]
pub fn profile(agent: &str, command: &str) -> AgentProfile {
    profile_with(agent, command, &["--scenario", "normal"])
}

/// 同上，但使用给定的 fake child 参数（场景、`--dump-env`、`--capabilities` 等）。
#[must_use]
pub fn profile_with(agent: &str, command: &str, args: &[&str]) -> AgentProfile {
    profile_with_env_vars(agent, command, args, &["FAKE_TOKEN"])
}

/// 同上，但白名单与凭据绑定集合由 `vars` 给定（每个变量一条绑定）。
#[must_use]
pub fn profile_with_env_vars(
    agent: &str,
    command: &str,
    args: &[&str],
    vars: &[&str],
) -> AgentProfile {
    AgentProfile::try_new(
        AgentId::new(agent).expect("agent id"),
        &format!("Agent {agent}"),
        command,
        args.iter().map(|arg| (*arg).to_owned()).collect(),
        vars.iter().map(|name| (*name).to_owned()).collect(),
        vars.iter()
            // `(provider_id, field)` 不得重复：用变量名当字段名，这样多变量 profile 也合法。
            .map(|name| ProviderEnvBinding::try_new("fake", name, name).expect("binding"))
            .collect(),
        false,
        timestamp(),
        timestamp(),
    )
    .expect("profile")
}

/// fake `LocalConfigStore`：只实现 profile 相关方法。
#[derive(Debug, Default)]
pub struct FakeConfig {
    profiles: Vec<AgentProfile>,
    /// `profiles()` 被调用的次数：用来证明目录查询**确实**经由注入端口取 profile。
    profiles_calls: AtomicUsize,
}

impl FakeConfig {
    /// 用给定 profile 构造。
    #[must_use]
    pub fn new(profiles: Vec<AgentProfile>) -> Self {
        Self {
            profiles,
            profiles_calls: AtomicUsize::new(0),
        }
    }

    /// `profiles()` 至今被调用的次数。
    #[must_use]
    pub fn profiles_calls(&self) -> usize {
        self.profiles_calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl LocalConfigStore for FakeConfig {
    async fn profiles(&self) -> Result<Vec<AgentProfile>, acp_core::model::PortError> {
        self.profiles_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.profiles.clone())
    }

    async fn profile(
        &self,
        id: &AgentId,
    ) -> Result<Option<AgentProfile>, acp_core::model::PortError> {
        Ok(self
            .profiles
            .iter()
            .find(|profile| profile.id() == id)
            .cloned())
    }

    async fn put_profile(&self, _write: ProfileWrite) -> Result<(), acp_core::model::PortError> {
        todo!("测试 fake 未实现 put_profile")
    }

    async fn workspaces(&self) -> Result<Vec<WorkspaceRecord>, acp_core::model::PortError> {
        todo!("测试 fake 未实现 workspaces")
    }

    async fn workspace(
        &self,
        _alias: &acp_core::model::WorkspaceAlias,
    ) -> Result<Option<WorkspaceRecord>, acp_core::model::PortError> {
        todo!("测试 fake 未实现 workspace")
    }

    async fn put_workspace(
        &self,
        _write: WorkspaceWrite,
    ) -> Result<(), acp_core::model::PortError> {
        todo!("测试 fake 未实现 put_workspace")
    }

    async fn provider_refs(&self) -> Result<Vec<ProviderRef>, acp_core::model::PortError> {
        todo!("测试 fake 未实现 provider_refs")
    }

    async fn put_provider_ref(
        &self,
        _write: ProviderRefWrite,
    ) -> Result<(), acp_core::model::PortError> {
        todo!("测试 fake 未实现 put_provider_ref")
    }

    async fn seed_state(&self) -> Result<acp_core::model::SeedState, acp_core::model::PortError> {
        todo!("测试 fake 未实现 seed_state")
    }

    async fn mark_seeded(&self, _write: SeedWrite) -> Result<(), acp_core::model::PortError> {
        todo!("测试 fake 未实现 mark_seeded")
    }
}

/// fake `CredentialResolver`：按 profile 的绑定给出固定值；可配置为失败。
#[derive(Debug, Default)]
pub struct FakeCredentials {
    fail: bool,
    /// 故意返回一个不在白名单里的变量（用于验证「白名单是上限」的纵深防御）。
    leak: bool,
    /// 故意**不**返回这个已绑定且在白名单内的变量（用于验证「期望集合必须完整」的纵深防御）。
    drop_name: Option<String>,
}

impl FakeCredentials {
    /// 正常解析。
    #[must_use]
    pub fn ok() -> Self {
        Self {
            fail: false,
            leak: false,
            drop_name: None,
        }
    }

    /// 一律失败（模拟 keystore 不可用或引用失效）。
    #[must_use]
    pub fn failing() -> Self {
        Self {
            fail: true,
            leak: false,
            drop_name: None,
        }
    }

    /// 返回一个白名单外的变量（模拟端口实现出错）。
    #[must_use]
    pub fn leaking() -> Self {
        Self {
            fail: false,
            leak: true,
            drop_name: None,
        }
    }

    /// 故意漏掉一个「已绑定且在白名单内」的变量（模拟端口违反「不得静默跳过变量」的契约）。
    #[must_use]
    pub fn dropping(name: &str) -> Self {
        Self {
            fail: false,
            leak: false,
            drop_name: Some(name.to_owned()),
        }
    }
}

#[async_trait::async_trait]
impl CredentialResolver for FakeCredentials {
    async fn resolve_env(
        &self,
        profile: &AgentProfile,
    ) -> Result<Vec<(String, SecretValue)>, acp_core::model::PortError> {
        if self.fail {
            return Err(acp_core::model::PortError::Unavailable(
                acp_core::model::UnavailableKind::KeystoreUnavailable,
            ));
        }
        if self.leak {
            return Ok(vec![(
                "NOT_ALLOW_LISTED".to_owned(),
                SecretValue::new("fake-secret-value".to_owned()),
            )]);
        }
        // 白名单是上限：绑定不在白名单里就不注入（与真实端口契约一致）。
        Ok(profile
            .env()
            .iter()
            .filter(|binding| {
                profile
                    .env_allowlist()
                    .iter()
                    .any(|name| name == binding.name())
            })
            .filter(|binding| self.drop_name.as_deref() != Some(binding.name()))
            .map(|binding| {
                (
                    binding.name().to_owned(),
                    SecretValue::new("fake-secret-value".to_owned()),
                )
            })
            .collect())
    }
}

/// 一个存在的 workspace（ 需要已解析的 cwd）。
#[must_use]
pub fn workspace() -> ResolvedWorkspace {
    ResolvedWorkspace::try_new(
        WorkspaceAlias::new("demo").expect("alias"),
        std::env::temp_dir().to_string_lossy().into_owned(),
    )
    .expect("workspace")
}

/// 计算一段文本的 `Digest`（与实现同口径：SHA-256 → 无填充 base64url）。
#[must_use]
pub fn digest_of(text: &str) -> Digest {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(text.as_bytes());
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
    Digest::new(&encoded).expect("digest")
}

/// 键值集合（断言环境变量集合用）。
#[must_use]
pub fn as_map(pairs: impl IntoIterator<Item = (String, String)>) -> HashMap<String, String> {
    pairs.into_iter().collect()
}

/// 一个合法的 `ConfigValue`。
#[must_use]
pub fn boolean(value: bool) -> ConfigValue {
    ConfigValue::boolean(value)
}
