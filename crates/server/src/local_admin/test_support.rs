//! 路由单元测试用的最小 fake 端口（仅测试代码）。
//!
//! `#![cfg(test)]` 是**自我声明**：本文件只在 `cargo test` 下编译（`mod.rs` 已用 `#[cfg(test)]` 声明
//! 该模块，这一行让文件本身也承担同一前提，便于「正常路径无 unwrap/expect/panic」这类静态自检判定）。
//!
//! 为什么需要它：方法路由的入参校验不需要存储，但 `core::use_cases::UseCases` 是具体类型，构造它必须
//! 提供全部端口。因此这里按「**只实现被本轮路由触及的方法**」的原则提供最小 fake：其余 trait 方法一律
//! `unreachable!("本轮路由测试未触及")`。方法级端到端行为（真实 SQLite、真实 keystore、真实 IPC）由
//! WP4 的集成测试（[PV4]）承担，本模块**不是**第二个实现，也不打算成为通用测试替身。
//!
//! 每个 fake 内含 `Arc<Mutex<…>>`，因此 `TestWorld` 的克隆（注入路由的那份）与测试持有的那份共享状态。
#![cfg(test)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::*;
use acp_core::ports::*;
use acp_core::use_cases::{UseCaseDeps, UseCases};
use identity_auth::{
    IdentityKeystore, KeyHandle, KeyPurpose, KeystoreError, SecretBytes, SecretPurpose,
};

use crate::local_admin::daemon::{DaemonAgent, DaemonControl, DaemonCounts, DaemonStatus};
use crate::local_admin::error::AdminError;
use crate::local_admin::router::{LocalAdminDeps, LocalAdminRouter};

/// 未实现方法的统一替身标记。
const NOT_TOUCHED: &str = "本轮路由测试未触及";

/// 测试用的固定时间戳（§1.1 形状）。
pub(crate) const NOW: &str = "2026-09-18T09:12:03.412Z";

/// 测试用的合法 P-256 公钥：基点 G 的 SEC1 未压缩编码（曲线上的确定点，无需随机源）。
const TEST_PUBLIC_KEY_HEX: &str = concat!(
    "04",
    "6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296",
    "4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5"
);

pub(crate) fn test_public_key() -> PeerPublicKey {
    let bytes: Vec<u8> = TEST_PUBLIC_KEY_HEX
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let hi = (pair[0] as char).to_digit(16).expect("hex") as u8;
            let lo = (pair[1] as char).to_digit(16).expect("hex") as u8;
            (hi << 4) | lo
        })
        .collect();
    PeerPublicKey::try_from_bytes(&bytes).expect("基点 G 是合法的 P-256 未压缩点")
}

fn timestamp(text: &str) -> Timestamp {
    Timestamp::new(text).expect("测试时间戳合法")
}

/// 32 字节零字节的规范无填充 base64url 摘要（`AuditRecord` 的 `detail_digest` 取值）。
fn test_digest() -> Digest {
    use base64::Engine as _;
    let text = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0u8; 32]);
    Digest::new(&text).expect("规范 base64url 摘要")
}

// ---------------------------------------------------------------------------------------------
// 时钟与 id
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
pub(crate) struct FakeClock {
    now: Arc<Mutex<String>>,
}

impl FakeClock {
    pub(crate) fn new() -> Self {
        Self {
            now: Arc::new(Mutex::new(NOW.to_owned())),
        }
    }

    pub(crate) fn set(&self, text: &str) {
        *self.now.lock().expect("时钟锁") = text.to_owned();
    }

    pub(crate) fn text(&self) -> String {
        self.now.lock().expect("时钟锁").clone()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        timestamp(&self.text())
    }
}

// ---------------------------------------------------------------------------------------------
// 审计
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
pub(crate) struct FakeAudit {
    records: Arc<Mutex<Vec<AuditRecord>>>,
    failure: Arc<Mutex<Option<PortError>>>,
}

impl FakeAudit {
    pub(crate) fn seed(&self, record: AuditRecord) {
        self.records.lock().expect("审计锁").push(record);
    }

    /// 两条记录：一条 `device.revoked`（09:12:03.412Z，带 detail digest）与一条 `export.created`
    /// （08:00:00.000Z）。
    pub(crate) fn seed_default(&self) {
        self.seed(
            AuditRecord::try_new(
                timestamp("2026-09-18T09:12:03.412Z"),
                AuditAction::DeviceRevoked,
                Actor::LocalCli,
                None,
                None,
                EntityRef::Device(
                    DeviceId::new("2ae1c07c-0000-4000-8000-000000000001").expect("设备 id"),
                ),
                AuditOutcome::Success,
                Some(test_digest()),
            )
            .expect("审计行"),
        );
        self.seed(
            AuditRecord::try_new(
                timestamp("2026-09-18T08:00:00.000Z"),
                AuditAction::ExportCreated,
                Actor::LocalCli,
                None,
                None,
                EntityRef::Export(ExportId::new("export-1").expect("export id")),
                AuditOutcome::Success,
                None,
            )
            .expect("审计行"),
        );
    }

    pub(crate) fn fail_next_query(&self, error: PortError) {
        *self.failure.lock().expect("审计锁") = Some(error);
    }
}

#[async_trait::async_trait]
impl AuditStore for FakeAudit {
    async fn append(&self, record: AuditRecord) -> Result<(), PortError> {
        self.records.lock().expect("审计锁").push(record);
        Ok(())
    }

    async fn query(&self, query: AuditQuery) -> Result<Vec<AuditRecord>, PortError> {
        if let Some(error) = self.failure.lock().expect("审计锁").take() {
            return Err(error);
        }
        let records = self.records.lock().expect("审计锁");
        Ok(records
            .iter()
            .filter(|record| {
                query
                    .since
                    .as_ref()
                    .is_none_or(|since| record.at() >= since)
            })
            .filter(|record| {
                query
                    .until
                    .as_ref()
                    .is_none_or(|until| record.at() <= until)
            })
            .filter(|record| query.actions.is_empty() || query.actions.contains(&record.action()))
            .cloned()
            .collect())
    }
}

// ---------------------------------------------------------------------------------------------
// 本地配置（profile / workspace / provider 引用）
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
pub(crate) struct FakeConfig {
    profiles: Arc<Mutex<BTreeMap<String, AgentProfile>>>,
    workspaces: Arc<Mutex<BTreeMap<String, WorkspaceRecord>>>,
    providers: Arc<Mutex<Vec<ProviderRef>>>,
    failure: Arc<Mutex<Option<PortError>>>,
    provider_failure: Arc<Mutex<Option<PortError>>>,
}

impl FakeConfig {
    pub(crate) fn seed_profile(&self, profile: AgentProfile) {
        self.profiles
            .lock()
            .expect("配置锁")
            .insert(profile.id().as_str().to_owned(), profile);
    }

    pub(crate) fn seed_workspace(&self, record: WorkspaceRecord) {
        self.workspaces
            .lock()
            .expect("配置锁")
            .insert(record.alias().as_str().to_owned(), record);
    }

    pub(crate) fn workspace_path(&self, alias: &str) -> Option<String> {
        self.workspaces
            .lock()
            .expect("配置锁")
            .get(alias)
            .map(|record| record.canonical_path().to_owned())
    }

    pub(crate) fn profile(&self, agent_id: &str) -> Option<AgentProfile> {
        self.profiles.lock().expect("配置锁").get(agent_id).cloned()
    }

    pub(crate) fn profile_count(&self) -> usize {
        self.profiles.lock().expect("配置锁").len()
    }

    pub(crate) fn profile_default(&self, agent_id: &str) -> Option<bool> {
        self.profile(agent_id).map(|profile| profile.is_default())
    }

    /// 已提交的 Provider 引用：`(configured_fields, keystore_ref, version)`。
    pub(crate) fn provider_ref(&self, provider_id: &str) -> Option<(Vec<String>, String, u64)> {
        self.providers
            .lock()
            .expect("配置锁")
            .iter()
            .find(|reference| reference.id() == provider_id)
            .map(|reference| {
                (
                    reference.configured_fields().to_vec(),
                    reference.keystore_ref().to_owned(),
                    reference.version(),
                )
            })
    }

    pub(crate) fn fail_next_workspace_lookup(&self, error: PortError) {
        *self.failure.lock().expect("配置锁") = Some(error);
    }

    /// 让下一次 Provider 引用提交失败（用于覆盖「引用未提交 → 清理本次 keystore 条目」分支）。
    pub(crate) fn fail_next_provider_ref(&self, error: PortError) {
        *self.provider_failure.lock().expect("配置锁") = Some(error);
    }
}

#[async_trait::async_trait]
impl LocalConfigStore for FakeConfig {
    async fn profiles(&self) -> Result<Vec<AgentProfile>, PortError> {
        Ok(self
            .profiles
            .lock()
            .expect("配置锁")
            .values()
            .cloned()
            .collect())
    }

    async fn profile(&self, id: &AgentId) -> Result<Option<AgentProfile>, PortError> {
        Ok(self.profile(id.as_str()))
    }

    async fn put_profile(&self, write: ProfileWrite) -> Result<(), PortError> {
        self.seed_profile(write.profile);
        Ok(())
    }

    async fn workspaces(&self) -> Result<Vec<WorkspaceRecord>, PortError> {
        Ok(self
            .workspaces
            .lock()
            .expect("配置锁")
            .values()
            .cloned()
            .collect())
    }

    async fn workspace(
        &self,
        alias: &WorkspaceAlias,
    ) -> Result<Option<WorkspaceRecord>, PortError> {
        if let Some(error) = self.failure.lock().expect("配置锁").take() {
            return Err(error);
        }
        Ok(self
            .workspaces
            .lock()
            .expect("配置锁")
            .get(alias.as_str())
            .cloned())
    }

    async fn put_workspace(&self, write: WorkspaceWrite) -> Result<(), PortError> {
        self.seed_workspace(write.record);
        Ok(())
    }

    async fn provider_refs(&self) -> Result<Vec<ProviderRef>, PortError> {
        Ok(self.providers.lock().expect("配置锁").clone())
    }

    async fn put_provider_ref(&self, write: ProviderRefWrite) -> Result<(), PortError> {
        if let Some(error) = self.provider_failure.lock().expect("配置锁").take() {
            return Err(error);
        }
        let mut providers = self.providers.lock().expect("配置锁");
        providers.retain(|reference| {
            !(reference.id() == write.reference.id() && reference.kind() == write.reference.kind())
        });
        providers.push(write.reference);
        Ok(())
    }

    async fn seed_state(&self) -> Result<SeedState, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn mark_seeded(&self, _write: SeedWrite) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }
}

// ---------------------------------------------------------------------------------------------
// Export / Import
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct FakeExports {
    clock: FakeClock,
    exports: Arc<Mutex<BTreeMap<String, ExportRecord>>>,
    imports: Arc<Mutex<BTreeMap<String, ImportRecord>>>,
    failure: Arc<Mutex<Option<PortError>>>,
}

impl Default for FakeExports {
    fn default() -> Self {
        Self {
            clock: FakeClock::new(),
            exports: Arc::default(),
            imports: Arc::default(),
            failure: Arc::default(),
        }
    }
}

impl FakeExports {
    /// 使用给定时钟（与路由共用同一时间来源，撤销时间因此可判定）。
    pub(crate) fn with_clock(mut self, clock: FakeClock) -> Self {
        self.clock = clock;
        self
    }

    pub(crate) fn count(&self) -> usize {
        self.exports.lock().expect("export 锁").len()
    }

    pub(crate) fn import_count(&self) -> usize {
        self.imports.lock().expect("import 锁").len()
    }

    pub(crate) fn seed_export(&self, record: ExportRecord) {
        self.exports
            .lock()
            .expect("export 锁")
            .insert(record.export_id().as_str().to_owned(), record);
    }

    /// 预置一条**已撤销**的 Export（`export.list` 必须包含已撤销项）。
    pub(crate) fn seed_revoked(&self, export_id: &str) {
        let revoked_at = timestamp("2026-01-01T00:00:00.000Z");
        let record = ExportRecord::try_new(
            ExportId::new(export_id).expect("export id"),
            "Old Export",
            vec![AgentId::new("codex").expect("agent id")],
            vec![
                WorkspaceAliasEntry::try_new(
                    WorkspaceAlias::new("project").expect("alias"),
                    "Project",
                )
                .expect("alias entry"),
            ],
            WorkspaceAlias::new("project").expect("alias"),
            vec![
                ExportTemplate::try_new(
                    TemplateId::new("template-1").expect("template id"),
                    "Template",
                    WorkspaceAlias::new("project").expect("alias"),
                    Vec::new(),
                )
                .expect("template"),
            ],
            TemplateId::new("template-1").expect("template id"),
            GrantSet::empty(),
            CachePolicy::NoContentCache,
            timestamp("2026-01-01T00:00:00.000Z"),
            Some(revoked_at),
        )
        .expect("Export 记录");
        self.seed_export(record);
    }

    pub(crate) fn seed_import(&self, import_id: &str) {
        let record = ImportRecord::try_new(
            ImportId::new(import_id).expect("import id"),
            "wss://owner.example/",
            NodeId::new("2ae1c07c-0000-4000-8000-000000000002").expect("node id"),
            vec![ExportId::new("export-1").expect("export id")],
            GrantSet::try_from_iter(["grant.session.read"]).expect("grants"),
        )
        .expect("Import 记录");
        self.imports
            .lock()
            .expect("import 锁")
            .insert(import_id.to_owned(), record);
    }

    pub(crate) fn fail_next_imports(&self, error: PortError) {
        *self.failure.lock().expect("export 锁") = Some(error);
    }
}

#[async_trait::async_trait]
impl ExportStore for FakeExports {
    async fn export(&self, id: &ExportId) -> Result<Option<ExportRecord>, PortError> {
        Ok(self
            .exports
            .lock()
            .expect("export 锁")
            .get(id.as_str())
            .cloned())
    }

    async fn exports(&self) -> Result<Vec<ExportRecord>, PortError> {
        Ok(self
            .exports
            .lock()
            .expect("export 锁")
            .values()
            .cloned()
            .collect())
    }

    async fn import(&self, id: &ImportId) -> Result<Option<ImportRecord>, PortError> {
        Ok(self
            .imports
            .lock()
            .expect("import 锁")
            .get(id.as_str())
            .cloned())
    }

    async fn imports(&self) -> Result<Vec<ImportRecord>, PortError> {
        if let Some(error) = self.failure.lock().expect("export 锁").take() {
            return Err(error);
        }
        Ok(self
            .imports
            .lock()
            .expect("import 锁")
            .values()
            .cloned()
            .collect())
    }

    async fn put_export(&self, write: ExportWrite) -> Result<(), PortError> {
        if let Some(error) = self.failure.lock().expect("export 锁").take() {
            return Err(error);
        }
        if self
            .exports
            .lock()
            .expect("export 锁")
            .contains_key(write.record.export_id().as_str())
        {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        self.seed_export(write.record);
        Ok(())
    }

    async fn revoke_export(&self, write: ExportRevocation) -> Result<(), PortError> {
        let mut exports = self.exports.lock().expect("export 锁");
        let Some(record) = exports.get(write.export.as_str()).cloned() else {
            return Err(PortError::NotFound(EntityRef::Export(write.export.clone())));
        };
        if record.is_revoked() {
            return Err(PortError::NotFound(EntityRef::Export(write.export)));
        }
        let revoked = ExportRecord::try_new(
            record.export_id().clone(),
            record.display_name(),
            record.agent_ids().to_vec(),
            record.workspace_aliases().to_vec(),
            record.default_workspace_alias().clone(),
            record.templates().to_vec(),
            record.default_template_id().clone(),
            record.scopes().clone(),
            record.cache_policy(),
            record.created_at().clone(),
            Some(self.clock.now()),
        )
        .expect("撤销后的 Export 记录");
        exports.insert(write.export.as_str().to_owned(), revoked);
        Ok(())
    }

    async fn add_import(&self, _write: ImportWrite) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn remove_import(&self, write: ImportRemoval) -> Result<(), PortError> {
        let removed = self
            .imports
            .lock()
            .expect("import 锁")
            .remove(write.import.as_str());
        match removed {
            Some(_) => Ok(()),
            None => Err(PortError::NotFound(EntityRef::Import(write.import))),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// keystore
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct FakeKeystore {
    available: Arc<Mutex<bool>>,
    secrets: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    failing_label: Arc<Mutex<Option<String>>>,
}

impl Default for FakeKeystore {
    fn default() -> Self {
        Self {
            available: Arc::new(Mutex::new(true)),
            secrets: Arc::default(),
            failing_label: Arc::default(),
        }
    }
}

impl FakeKeystore {
    pub(crate) fn set_available(&self, available: bool) {
        *self.available.lock().expect("keystore 锁") = available;
    }

    /// 让写指定标签的调用失败一次（用于覆盖「部分写入 → 回滚」分支）。
    pub(crate) fn fail_put_for(&self, label: &str) {
        *self.failing_label.lock().expect("keystore 锁") = Some(label.to_owned());
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.secrets.lock().expect("keystore 锁").is_empty()
    }

    /// 某个标签下的秘密值（测试断言用；只在本模块内出现）。
    pub(crate) fn secret(&self, label: &str) -> Option<String> {
        self.secrets
            .lock()
            .expect("keystore 锁")
            .get(label)
            .map(|bytes| String::from_utf8(bytes.clone()).expect("测试值都是 UTF-8"))
    }
}

#[async_trait::async_trait]
impl IdentityKeystore for FakeKeystore {
    async fn generate(
        &self,
        _purpose: KeyPurpose,
        _label: &str,
    ) -> Result<KeyHandle, KeystoreError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn public_key(&self, _handle: &KeyHandle) -> Result<PeerPublicKey, KeystoreError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn sign(
        &self,
        _handle: &KeyHandle,
        _transcript: &[u8],
    ) -> Result<identity_auth::P1363Signature, KeystoreError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn delete(&self, _handle: &KeyHandle) -> Result<(), KeystoreError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn get_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
    ) -> Result<Option<SecretBytes>, KeystoreError> {
        assert_eq!(purpose, SecretPurpose::ProviderCredential);
        if !*self.available.lock().expect("keystore 锁") {
            return Err(KeystoreError::Unavailable);
        }
        Ok(self
            .secrets
            .lock()
            .expect("keystore 锁")
            .get(key)
            .map(|bytes| SecretBytes::new(bytes)))
    }

    async fn put_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
        value: &SecretBytes,
    ) -> Result<(), KeystoreError> {
        assert_eq!(purpose, SecretPurpose::ProviderCredential);
        if !*self.available.lock().expect("keystore 锁") {
            return Err(KeystoreError::Unavailable);
        }
        if self.failing_label.lock().expect("keystore 锁").as_deref() == Some(key) {
            return Err(KeystoreError::Unavailable);
        }
        if key.len() > 128 {
            return Err(KeystoreError::EntryInvalid);
        }
        self.secrets
            .lock()
            .expect("keystore 锁")
            .insert(key.to_owned(), value.as_bytes().to_vec());
        Ok(())
    }

    async fn delete_secret(&self, purpose: SecretPurpose, key: &str) -> Result<(), KeystoreError> {
        assert_eq!(purpose, SecretPurpose::ProviderCredential);
        if !*self.available.lock().expect("keystore 锁") {
            return Err(KeystoreError::Unavailable);
        }
        match self.secrets.lock().expect("keystore 锁").remove(key) {
            Some(_) => Ok(()),
            None => Err(KeystoreError::SecretMissing),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Daemon 生命周期
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct FakeDaemon {
    status: Arc<Mutex<DaemonStatus>>,
    stops: Arc<Mutex<Vec<Option<u64>>>>,
    failure: Arc<Mutex<Option<AdminError>>>,
}

impl Default for FakeDaemon {
    fn default() -> Self {
        Self {
            status: Arc::new(Mutex::new(DaemonStatus {
                version: "0.0.0-test".to_owned(),
                instance_id: "0123456789abcdef".to_owned(),
                node_id: NodeId::new("2ae1c07c-0000-4000-8000-000000000001").expect("node id"),
                node_public_key: test_public_key(),
                started_at: timestamp(NOW),
                uptime_ms: 1,
                data_dir: "D:\\data".to_owned(),
                listen: Vec::new(),
                public_origin: None,
                counts: DaemonCounts::default(),
                agents: vec![DaemonAgent {
                    agent_id: AgentId::new("codex").expect("agent id"),
                    available: true,
                }],
                links: Vec::new(),
            })),
            stops: Arc::default(),
            failure: Arc::default(),
        }
    }
}

impl FakeDaemon {
    pub(crate) fn set_version(&self, version: &str) {
        self.status.lock().expect("daemon 锁").version = version.to_owned();
    }

    pub(crate) fn stops(&self) -> Vec<Option<u64>> {
        self.stops.lock().expect("daemon 锁").clone()
    }

    pub(crate) fn fail_next_stop(&self) {
        *self.failure.lock().expect("daemon 锁") = Some(AdminError::new(
            crate::local_admin::error::LocalErrorCode::Unavailable,
            "daemon is shutting down",
        ));
    }
}

#[async_trait::async_trait]
impl DaemonControl for FakeDaemon {
    async fn status(&self) -> DaemonStatus {
        self.status.lock().expect("daemon 锁").clone()
    }

    async fn stop(&self, grace_ms: Option<u64>) -> Result<(), AdminError> {
        if let Some(error) = self.failure.lock().expect("daemon 锁").take() {
            return Err(error);
        }
        self.stops.lock().expect("daemon 锁").push(grace_ms);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Agent 目录
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
pub(crate) struct FakeCatalog {
    agents: Vec<AgentRef>,
}

impl FakeCatalog {
    fn with_agents(agents: Vec<AgentRef>) -> Self {
        Self { agents }
    }
}

#[async_trait::async_trait]
impl AgentCatalog for FakeCatalog {
    async fn agents(&self) -> Result<Vec<AgentDescriptor>, PortError> {
        Ok(self
            .agents
            .iter()
            .cloned()
            .map(|agent| AgentDescriptor::new(agent, true, ResourceOrigin::Local))
            .collect())
    }

    async fn agent_capabilities(&self, _agent: &AgentRef) -> Result<CapabilitySet, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }
}

// ---------------------------------------------------------------------------------------------
// 其余端口的空实现
// ---------------------------------------------------------------------------------------------

/// 未被本轮路由触及的端口实现：所有方法都 `unreachable!`，用作 `UseCases`/`Broker` 的占位依赖。
pub(crate) struct NotTouched;

#[async_trait::async_trait]
impl SessionStore for NotTouched {
    async fn commit(&self, _commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn load(&self, _session: &SessionId) -> Result<Option<SessionSnapshot>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn list(&self, _query: SessionQuery) -> Result<Vec<SessionSummary>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn head(&self) -> Result<GlobalCursor, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn find_request(
        &self,
        _request: &RequestId,
        _actor: &Actor,
    ) -> Result<Option<CommandRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn unsettled_commands(
        &self,
        _limit: ReplayLimit,
    ) -> Result<Vec<CommandRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn retention_window(
        &self,
        _session: &SessionId,
    ) -> Result<Option<(Sequence, Sequence)>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn prune(
        &self,
        _policy: RetentionPolicy,
        _at: Timestamp,
    ) -> Result<PruneReport, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn health(&self) -> Result<StoreHealth, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }
}

#[async_trait::async_trait]
impl RemoteDeliveryStore for NotTouched {
    async fn commit_receipt(&self, _receipt: DeliveryReceipt) -> Result<ReceiptOutcome, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn local_replay(
        &self,
        _after: Option<LocalCursor>,
        _limit: ReplayLimit,
    ) -> Result<Vec<DeliveryIndexEntry>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn ack(
        &self,
        _session: &RemoteSessionRef,
        _cursor: OriginCursor,
        _at: Timestamp,
    ) -> Result<AckOutcome, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn load_ack(
        &self,
        _session: &RemoteSessionRef,
    ) -> Result<Option<OriginCursor>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn upsert_session(
        &self,
        _record: ImportedSessionRecord,
        _at: Timestamp,
    ) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn list_sessions(
        &self,
        _query: ImportedSessionQuery,
    ) -> Result<Vec<ImportedSessionRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn find_remote_request(
        &self,
        _session: &RemoteSessionRef,
        _request: &RequestId,
    ) -> Result<Option<RemoteCommandRef>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn drop_import(&self, _import: &ImportId) -> Result<DropReport, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn prune(
        &self,
        _policy: RetentionPolicy,
        _at: Timestamp,
    ) -> Result<PruneReport, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }
}

#[async_trait::async_trait]
impl SessionBackendFactory for NotTouched {
    async fn create(
        &self,
        _session: &SessionId,
        _request: CreateSessionRequest,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn open(
        &self,
        _reference: SessionReference,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }
}

impl EventPublisher for NotTouched {
    fn publish(&self, _delivery: CommittedDelivery) {
        unreachable!("{NOT_TOUCHED}")
    }
}

#[async_trait::async_trait]
impl AttachmentStore for NotTouched {
    async fn put(
        &self,
        _bytes: &[u8],
        _media_type: &str,
        _at: Timestamp,
    ) -> Result<AttachmentRef, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn get(&self, _id: &AttachmentId) -> Result<Option<Vec<u8>>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn link(
        &self,
        _session: &SessionId,
        _attachment: &AttachmentId,
        _generation: AttachmentGeneration,
    ) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn prune_lru(
        &self,
        _budget_bytes: u64,
        _at: Timestamp,
    ) -> Result<PruneReport, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn sweep_orphans(&self, _at: Timestamp, _limit: u32) -> Result<u32, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }
}

#[async_trait::async_trait]
impl TrustStore for NotTouched {
    async fn device(&self, _id: &DeviceId) -> Result<Option<DeviceRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn devices(&self) -> Result<Vec<DeviceRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn node(&self, _id: &NodeId, _kind: NodeKind) -> Result<Option<NodeRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn nodes(&self) -> Result<Vec<NodeRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn nodes_for(&self, _id: &NodeId) -> Result<Vec<NodeRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn peer_key(&self, _peer: &PeerIdentity) -> Result<Option<PeerPublicKey>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn pairing(&self, _id: &PairingId) -> Result<Option<PairingRecord>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn pairing_peer(&self, _id: &PairingId) -> Result<Option<PairingPeer>, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn put_device(&self, _write: DeviceWrite) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn put_node(&self, _write: NodeWrite) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn revoke_device(&self, _write: DeviceRevocation) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn revoke_node(&self, _write: NodeRevocation) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn create_pairing(&self, _write: PairingWrite) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn claim_pairing(
        &self,
        _write: PairingClaimWrite,
    ) -> Result<PairingClaimOutcome, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn settle_pairing(
        &self,
        _write: PairingSettlementWrite,
    ) -> Result<TrustRecordRef, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn expire_pairings(&self, _write: ExpiryWrite) -> Result<u64, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }
}

pub(crate) struct FakeIds;

impl IdGenerator for FakeIds {
    fn turn_id(&self) -> TurnId {
        unreachable!("{NOT_TOUCHED}")
    }

    fn interaction_id(&self) -> InteractionId {
        unreachable!("{NOT_TOUCHED}")
    }

    fn pairing_id(&self) -> PairingId {
        unreachable!("{NOT_TOUCHED}")
    }

    fn message_id(&self) -> MessageId {
        unreachable!("{NOT_TOUCHED}")
    }

    fn origin_epoch(&self) -> OriginEpoch {
        unreachable!("{NOT_TOUCHED}")
    }

    fn request_id(&self) -> RequestId {
        unreachable!("{NOT_TOUCHED}")
    }
}

// ---------------------------------------------------------------------------------------------
// 测试世界
// ---------------------------------------------------------------------------------------------

/// 一套 fake 世界 + 由它装配出的路由，附带测试要观察的句柄。
pub(crate) struct TestWorld {
    pub(crate) clock: FakeClock,
    pub(crate) config: FakeConfig,
    pub(crate) exports: FakeExports,
    pub(crate) audit: FakeAudit,
    pub(crate) keystore: FakeKeystore,
    pub(crate) daemon: FakeDaemon,
    temporary: Arc<Mutex<Vec<PathBuf>>>,
}

impl TestWorld {
    pub(crate) fn new() -> Self {
        let clock = FakeClock::new();
        Self {
            exports: FakeExports::default().with_clock(clock.clone()),
            config: FakeConfig::default(),
            audit: FakeAudit::default(),
            keystore: FakeKeystore::default(),
            daemon: FakeDaemon::default(),
            clock,
            temporary: Arc::default(),
        }
    }

    pub(crate) fn router(&self) -> LocalAdminRouter {
        let clock: Arc<dyn Clock> = Arc::new(self.clock.clone());
        let store: Arc<dyn SessionStore> = Arc::new(NotTouched);
        let deliveries: Arc<dyn RemoteDeliveryStore> = Arc::new(NotTouched);
        let exports: Arc<dyn ExportStore> = Arc::new(self.exports.clone());
        let audit: Arc<dyn AuditStore> = Arc::new(self.audit.clone());
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: store.clone(),
                deliveries: deliveries.clone(),
                backends: Arc::new(NotTouched),
                exports: exports.clone(),
                publisher: Arc::new(NotTouched),
                clock: clock.clone(),
                ids: Arc::new(FakeIds),
                audit: Some(audit.clone()),
            },
            BrokerConfig::default(),
        ));
        let core = Arc::new(UseCases::new(UseCaseDeps {
            broker,
            store,
            deliveries,
            exports,
            trust: Arc::new(NotTouched),
            audit,
            config: Arc::new(self.config.clone()),
            attachments: Arc::new(NotTouched),
            catalog: Arc::new(FakeCatalog::with_agents(vec![
                AgentRef::try_new(AgentId::new("codex").expect("agent id"), "Codex")
                    .expect("agent ref"),
            ])),
            clock: clock.clone(),
            ids: Arc::new(FakeIds),
        }));
        LocalAdminRouter::new(LocalAdminDeps {
            daemon: Arc::new(self.daemon.clone()),
            core,
            keystore: Arc::new(self.keystore.clone()),
            audit: Arc::new(self.audit.clone()),
            clock,
        })
    }

    /// 时钟当前文本（测试断言 `createdAt`/`revokedAt` 用）。
    pub(crate) fn clock_text(&self) -> String {
        self.clock.text()
    }

    /// 新建一个临时目录（如 `rootPath` 之类需要真实目录的用例）；随 `TestWorld` 一起清理。
    pub(crate) fn temporary_directory(&self) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "acpr-wp3b1-router-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("临时目录可建");
        self.temporary
            .lock()
            .expect("临时目录锁")
            .push(path.clone());
        path
    }

    /// 建立一条 workspace 记录（`export.create` 的 alias 前置条件）。
    pub(crate) fn write_workspace(&self, alias: &str, display_name: &str) {
        let record = WorkspaceRecord::try_new(
            WorkspaceAlias::new(alias).expect("alias"),
            display_name,
            &std::env::temp_dir().to_string_lossy(),
            timestamp(NOW),
            timestamp(NOW),
        )
        .expect("workspace 记录");
        self.config.seed_workspace(record);
    }
}

impl Drop for TestWorld {
    fn drop(&mut self) {
        // 只清理本世界创建的临时目录；`Arc` 的最后一个持有者（测试结束时就是它）执行。
        for path in self.temporary.lock().expect("临时目录锁").iter() {
            std::fs::remove_dir_all(path).ok();
        }
    }
}
