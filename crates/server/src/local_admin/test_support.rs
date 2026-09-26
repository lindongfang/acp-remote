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
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::*;
use acp_core::ports::*;
use acp_core::use_cases::{UseCaseDeps, UseCases};
use identity_auth::{
    Authority, EntropyError, EntropySource, IdentityKeystore, KeyHandle, KeyPurpose, KeystoreError,
    PairingClaim, PairingPeer, SecretBytes, SecretPurpose,
};

use crate::local_admin::daemon::{DaemonAgent, DaemonControl, DaemonCounts, DaemonStatus};
use crate::local_admin::error::AdminError;
use crate::local_admin::pairing::{ConnectionCloser, PairingSessions};
use crate::local_admin::router::{LocalAdminDeps, LocalAdminRouter};

/// 未实现的统一替身标记。
const NOT_TOUCHED: &str = "本轮路由测试未触及";

/// `UseCases` 与 `Broker` 共用的两个未触及端口（每个 `UseCases` 一份 `Arc`）。
fn store_arc() -> Arc<dyn SessionStore> {
    Arc::new(NotTouched)
}

fn deliveries_arc() -> Arc<dyn RemoteDeliveryStore> {
    Arc::new(NotTouched)
}

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

/// 测试用的合法 nonce：32 字节全零的无填充 base64url（43 字符，末字符低 2 位为 0）。
pub(crate) fn test_nonce() -> Nonce {
    use base64::Engine as _;
    let text = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0u8; 32]);
    Nonce::new(&text).expect("规范的无填充 base64url nonce")
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
    /// 管理写集水位（`AuditStore::watermark`，即 `catalogRevision` 的来源）：用例把它固定成已知值，
    /// 而不靠审计行数（真实实现取 `sqlite_sequence`）。
    watermark: Arc<Mutex<u64>>,
}

impl FakeAudit {
    /// 固定本次测试的管理写集水位（`catalogRevision`）。
    pub(crate) fn set_watermark(&self, value: u64) {
        *self.watermark.lock().expect("审计锁") = value;
    }
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

    async fn watermark(&self) -> Result<u64, PortError> {
        Ok(*self.watermark.lock().expect("审计锁"))
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

    /// 与存储同形（`storage-sqlite/src/admin/export.rs`）：**只有已撤销**的既有 `export_id` 是
    /// `Conflict`，未撤销的既有行走 `ON CONFLICT DO UPDATE` 覆盖并返回 `Ok`。
    ///
    /// 这一点是测试强度的前提：§5.5「`exportId` 已存在 → `local.conflict`」在真实存储下完全由
    /// `Router::export_create` 的查重 guard 保障，若 fake 在这里更严，那条 guard 被删掉也不会让任何
    /// 用例变红（`router.rs` 的 `export_create_requires_registered_aliases_and_a_free_id`）。
    async fn put_export(&self, write: ExportWrite) -> Result<(), PortError> {
        if let Some(error) = self.failure.lock().expect("export 锁").take() {
            return Err(error);
        }
        let already_revoked = self
            .exports
            .lock()
            .expect("export 锁")
            .get(write.record.export_id().as_str())
            .is_some_and(|stored| stored.revoked_at().is_some());
        if already_revoked {
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
        // 存储层用 `UPDATE ... SET revoked_at = COALESCE(revoked_at, ?2)`：重复撤销是幂等的，
        // 保留首次时间而**不**报 `NotFound`（`storage-sqlite/src/admin/export.rs`）。§7 的
        // 「`*.revoke` 重试得到 `local.not_found`」由 `server` 侧在调用本方法之前判定。
        let revoked_at = record
            .revoked_at()
            .cloned()
            .unwrap_or_else(|| self.clock.now());
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
            Some(revoked_at),
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
        // 本机身份公钥：与 `DaemonStatus::node_public_key` 同一条可判定素材（配对 URL 的
        // `hostPublicKey`/`ownerPublicKey` 因此可断言）。
        Ok(test_public_key())
    }

    async fn sign(
        &self,
        _handle: &KeyHandle,
        transcript: &[u8],
    ) -> Result<identity_auth::P1363Signature, KeystoreError> {
        // 测试用固定私钥：标量 1，其公钥就是 [`test_public_key`] 的基点 G。因此
        // `sign_node_link_pairing_owner_proof` 的产物能在用例里被**同一个**公钥验证（R20）；
        // 这不是密码学材料，也不进生产路径（`p256` 只是本 crate 的 dev-dependency）。
        use p256::ecdsa::signature::Signer as _;
        // 标量必须在 1..n 内，且其公钥要等于 [`test_public_key`]（基点 G）——即标量 1（大端）。
        let mut scalar = [0u8; 32];
        scalar[31] = 1;
        let signing =
            p256::ecdsa::SigningKey::from_slice(&scalar).map_err(|_| KeystoreError::Unavailable)?;
        let signature: p256::ecdsa::Signature = signing.sign(transcript);
        identity_auth::P1363Signature::try_from_bytes(signature.to_bytes().as_slice())
            .map_err(|_| KeystoreError::Unavailable)
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

/// 「只有水位是真的」存储替身：`head()` 返回构造时给定的固定水位，其余方法仍不触及。
///
/// Node Link 握手视图（`UseCases::node_link_handshake_view`）是本仓库唯一会读 `head()` 的无副作用入口，
/// 用例需要它给 `node.ready.catalogRevision`/`serverEpoch` 一个**可判定**的值（不是常量、不依赖时钟）。
/// 除 `head()` 外的方法保持 `unreachable!`，以免替身比真实存储更宽容。
#[derive(Clone)]
pub(crate) struct FixedStore {
    head: GlobalCursor,
}

impl FixedStore {
    pub(crate) fn new(server_epoch: &str, global_sequence: u64) -> Self {
        Self {
            head: GlobalCursor::new(
                ServerEpoch::new(server_epoch).expect("固定 epoch 合法"),
                Sequence::new(global_sequence).expect("固定水位合法"),
            ),
        }
    }
}

#[async_trait::async_trait]
impl SessionStore for FixedStore {
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
        Ok(self.head.clone())
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

/// `FixedStore` 的水位 epoch（与 `TEST_PUBLIC_ORIGIN` 同一套固定素材）。
pub(crate) const TEST_SERVER_EPOCH: &str = "018f6f89-8a23-7a10-a0d3-f92e6a31d952";

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

    async fn pairing_for(&self, _peer: &PeerIdentity) -> Result<Option<PairingRecord>, PortError> {
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

    async fn consume_pairing(
        &self,
        _write: PairingConsumption,
    ) -> Result<PairingRecord, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn record_node_connected(&self, _write: NodeConnectedWrite) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }
}

/// 确定性 id 生成器：单调递增的规范 uuid 文本（与 core 内部测试的 `TestIds` 同款）。
///
/// 它是端口替身里唯一**必须能用**的生成器：Node Link 的每条出站消息都要一个 `messageId`
/// （`UseCases::ids()`），而 id 对调用方是不透明值，用例只要求「唯一且形状规范」。
#[derive(Default)]
pub(crate) struct FakeIds {
    next: std::sync::atomic::AtomicU64,
}

impl FakeIds {
    fn next_text(&self) -> String {
        let value = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        format!("00000000-0000-4000-8000-{value:012x}")
    }
}

impl IdGenerator for FakeIds {
    fn turn_id(&self) -> TurnId {
        TurnId::new(&self.next_text()).expect("规范 uuid 文本")
    }

    fn interaction_id(&self) -> InteractionId {
        InteractionId::new(&self.next_text()).expect("规范 uuid 文本")
    }

    fn pairing_id(&self) -> PairingId {
        PairingId::new(&self.next_text()).expect("规范 uuid 文本")
    }

    fn message_id(&self) -> MessageId {
        MessageId::new(&self.next_text()).expect("规范 uuid 文本")
    }

    fn origin_epoch(&self) -> OriginEpoch {
        OriginEpoch::new(&self.next_text()).expect("规范 uuid 文本")
    }

    fn request_id(&self) -> RequestId {
        RequestId::new(&self.next_text()).expect("规范 uuid 文本")
    }
}

// ---------------------------------------------------------------------------------------------
// 信任（设备/节点/配对）：
// ---------------------------------------------------------------------------------------------

/// 内存信任仓储：只实现本轮路由与配对测试触及的方法。
///
/// 写路径刻意**照抄存储层的可观察语义**（`storage-sqlite/src/admin/trust.rs`）：状态守卫、终态冲突、
/// 子集校验、批准时创建信任行、撤销时保留既有时间（COALESCE）——否则测试会验证一个不存在的存储行为。
/// 未触及的方法一律 `unreachable!`。
#[derive(Clone, Default)]
pub(crate) struct FakeTrust {
    devices: Arc<Mutex<BTreeMap<String, DeviceRecord>>>,
    nodes: Arc<Mutex<BTreeMap<(String, String), NodeRecord>>>, // key = (nodeId, NodeKind token)
    keys: Arc<Mutex<BTreeMap<(String, String), PeerPublicKey>>>,
    pairings: Arc<Mutex<BTreeMap<String, PairingRecord>>>,
    peers: Arc<Mutex<BTreeMap<String, PairingPeer>>>,
    audits: Arc<Mutex<Vec<AuditRecord>>>,
    settle_failure: Arc<Mutex<Option<PortError>>>,
}

impl FakeTrust {
    pub(crate) fn device(&self, device: &str) -> Option<DeviceRecord> {
        self.devices.lock().expect("信任锁").get(device).cloned()
    }

    pub(crate) fn device_count(&self) -> usize {
        self.devices.lock().expect("信任锁").len()
    }

    pub(crate) fn node_count(&self) -> usize {
        self.nodes.lock().expect("信任锁").len()
    }

    /// 某个 `nodeId` 的全部角色行（测试断言用）。
    pub(crate) fn nodes_of(&self, node: &str) -> Vec<NodeRecord> {
        self.nodes
            .lock()
            .expect("信任锁")
            .iter()
            .filter(|((id, _), _)| id == node)
            .map(|(_, record)| record.clone())
            .collect()
    }

    pub(crate) fn pairing(&self, pairing_id: &str) -> Option<PairingRecord> {
        self.pairings
            .lock()
            .expect("信任锁")
            .get(pairing_id)
            .cloned()
    }

    pub(crate) fn pairing_count(&self) -> usize {
        self.pairings.lock().expect("信任锁").len()
    }

    /// 由信任写集提交的审计行（`consume_pairing`/`settle_pairing`/撤销等同事务写入）。
    ///
    /// 真实存储把这些行写进同一张审计表，因此读取入口是 `AuditStore::query`；替身把两张表分开，
    /// 这里给出等价视图，免得用例漏看写集里的留痕。
    pub(crate) fn audits(&self) -> Vec<AuditRecord> {
        self.audits.lock().expect("信任锁").clone()
    }

    /// 让下一次落定失败（用于覆盖「写集提交失败 → 不得产生内存已批准、库无信任」）。
    pub(crate) fn fail_next_settle(&self, error: PortError) {
        *self.settle_failure.lock().expect("信任锁") = Some(error);
    }

    /// 直接预置一条设备记录（`device.list` 的边界用例）。
    pub(crate) fn seed_device(&self, record: DeviceRecord) {
        self.devices
            .lock()
            .expect("信任锁")
            .insert(record.device_id().as_str().to_owned(), record);
    }

    /// 直接预置一条节点记录（`node.list` 的 `pending` 边界用例）。
    pub(crate) fn seed_node(&self, record: NodeRecord) {
        self.nodes.lock().expect("信任锁").insert(
            (
                record.node_id().as_str().to_owned(),
                record.kind().as_str().to_owned(),
            ),
            record,
        );
    }
}

#[async_trait::async_trait]
impl TrustStore for FakeTrust {
    async fn device(&self, id: &DeviceId) -> Result<Option<DeviceRecord>, PortError> {
        Ok(self.device(id.as_str()))
    }

    async fn devices(&self) -> Result<Vec<DeviceRecord>, PortError> {
        Ok(self
            .devices
            .lock()
            .expect("信任锁")
            .values()
            .cloned()
            .collect())
    }

    async fn node(&self, id: &NodeId, kind: NodeKind) -> Result<Option<NodeRecord>, PortError> {
        Ok(self
            .nodes
            .lock()
            .expect("信任锁")
            .get(&(id.as_str().to_owned(), kind.as_str().to_owned()))
            .cloned())
    }

    async fn nodes(&self) -> Result<Vec<NodeRecord>, PortError> {
        Ok(self
            .nodes
            .lock()
            .expect("信任锁")
            .values()
            .cloned()
            .collect())
    }

    async fn nodes_for(&self, id: &NodeId) -> Result<Vec<NodeRecord>, PortError> {
        Ok(self
            .nodes
            .lock()
            .expect("信任锁")
            .iter()
            .filter(|((node, _), _)| node == id.as_str())
            .map(|(_, record)| record.clone())
            .collect())
    }

    async fn peer_key(&self, peer: &PeerIdentity) -> Result<Option<PeerPublicKey>, PortError> {
        Ok(self
            .keys
            .lock()
            .expect("信任锁")
            .get(&(peer.kind().to_owned(), peer.id_text().to_owned()))
            .cloned())
    }

    async fn pairing(&self, id: &PairingId) -> Result<Option<PairingRecord>, PortError> {
        Ok(self.pairing(id.as_str()))
    }

    async fn pairing_peer(&self, id: &PairingId) -> Result<Option<PairingPeer>, PortError> {
        Ok(self.peers.lock().expect("信任锁").get(id.as_str()).cloned())
    }

    /// 该对端最近一次配对：替身按 `pairings` 的登记顺序取最后一条匹配（真实实现按
    /// `created_at`/`pairing_id` 降序，两者都只服务「最近一次」这一个语义）。
    async fn pairing_for(&self, peer: &PeerIdentity) -> Result<Option<PairingRecord>, PortError> {
        let pairings = self.pairings.lock().expect("信任锁");
        let peers = self.peers.lock().expect("信任锁");
        let matched: Vec<PairingRecord> = peers
            .iter()
            .filter(|(_, row)| row.id() == peer)
            .filter_map(|(pairing_id, _)| pairings.get(pairing_id).cloned())
            .collect();
        Ok(matched.into_iter().last())
    }

    async fn put_device(&self, _write: DeviceWrite) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn put_node(&self, _write: NodeWrite) -> Result<(), PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    async fn revoke_device(&self, write: DeviceRevocation) -> Result<(), PortError> {
        let mut devices = self.devices.lock().expect("信任锁");
        let Some(record) = devices.get(write.device.as_str()).cloned() else {
            return Err(PortError::NotFound(EntityRef::Device(write.device)));
        };
        // 存储层用 `COALESCE(revoked_at, ?)`：重复撤销保留首次时间。
        let revoked_at = record
            .revoked_at()
            .cloned()
            .unwrap_or_else(|| write.context.at.clone());
        let revoked = DeviceRecord::try_new(
            record.device_id().clone(),
            record.display_name(),
            record.public_key_fingerprint().clone(),
            record.scopes().clone(),
            DeviceState::Revoked,
            record.created_at().clone(),
            record.last_seen_at().cloned(),
            Some(revoked_at),
        )
        .expect("撤销后的设备记录合法");
        self.append_audits(&write.context.at, write.context.audit);
        devices.insert(write.device.as_str().to_owned(), revoked);
        Ok(())
    }

    async fn revoke_node(&self, write: NodeRevocation) -> Result<(), PortError> {
        let mut nodes = self.nodes.lock().expect("信任锁");
        if !nodes.keys().any(|(node, _)| node == write.node.as_str()) {
            return Err(PortError::NotFound(EntityRef::Node(write.node)));
        }
        for ((node, _), record) in nodes.iter_mut() {
            if node != write.node.as_str() {
                continue;
            }
            let revoked_at = record
                .revoked_at()
                .cloned()
                .unwrap_or_else(|| write.context.at.clone());
            *record = NodeRecord::try_new(
                record.node_id().clone(),
                record.display_name(),
                record.kind(),
                record.node_public_key_fingerprint().clone(),
                record.grants().clone(),
                NodeState::Revoked,
                record.owner_endpoint().map(str::to_owned),
                record.created_at().clone(),
                record.last_connected_at().cloned(),
                Some(revoked_at),
            )
            .expect("撤销后的节点记录合法");
        }
        self.append_audits(&write.context.at, write.context.audit);
        Ok(())
    }

    async fn create_pairing(&self, write: PairingWrite) -> Result<(), PortError> {
        if write.record.state() != PairingState::Created {
            return Err(PortError::InvalidRequest(
                "a pairing must be registered in the created state",
            ));
        }
        let mut pairings = self.pairings.lock().expect("信任锁");
        if pairings.contains_key(write.record.id().as_str()) {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        self.append_audits(&write.context.at, write.context.audit);
        pairings.insert(write.record.id().as_str().to_owned(), write.record);
        Ok(())
    }

    /// 与存储层同款：原子检查「未过期、仍为 `created`、绑定一致、集合不超出登记值」，写 peer 行并推进到
    /// `pending_confirmation`。HMAC/证明校验属 HTTPS claim 路径（本切片未落地），不在本替身内。
    async fn claim_pairing(
        &self,
        write: PairingClaimWrite,
    ) -> Result<PairingClaimOutcome, PortError> {
        let pairing_id = write.claim.pairing().clone();
        let mut pairings = self.pairings.lock().expect("信任锁");
        let Some(record) = pairings.get(pairing_id.as_str()).cloned() else {
            return Err(PortError::NotFound(EntityRef::Pairing(pairing_id)));
        };
        if self
            .peers
            .lock()
            .expect("信任锁")
            .contains_key(pairing_id.as_str())
        {
            return Err(PortError::Conflict(ConflictKind::AlreadyClaimed));
        }
        if record.state() != PairingState::Created {
            return Err(PortError::Conflict(ConflictKind::AlreadyClaimed));
        }
        if write.context.at.as_str() >= record.expires_at().as_str() {
            return Err(PortError::Conflict(ConflictKind::Expired));
        }
        if write.claim.peer().host_binding() != record.host_binding() {
            return Err(PortError::InvalidRequest("pairing host binding mismatch"));
        }
        let claimed = PairingRecord::try_new(
            record.id().clone(),
            record.target(),
            PairingState::PendingConfirmation,
            Some(write.claim.peer().display_name().to_owned()),
            record.requested_scopes().clone(),
            record.requested_grants().clone(),
            record.secret_digest().clone(),
            record.host_binding(),
            record.created_at().clone(),
            record.expires_at().clone(),
            Some(write.context.at.clone()),
            None,
            None,
        )
        .expect("认领后的配对记录合法");
        self.append_audits(&write.context.at, write.context.audit);
        self.peers
            .lock()
            .expect("信任锁")
            .insert(pairing_id.as_str().to_owned(), write.claim.peer().clone());
        pairings.insert(pairing_id.as_str().to_owned(), claimed.clone());
        Ok(PairingClaimOutcome { pairing: claimed })
    }

    async fn settle_pairing(
        &self,
        write: PairingSettlementWrite,
    ) -> Result<TrustRecordRef, PortError> {
        if let Some(error) = self.settle_failure.lock().expect("信任锁").take() {
            return Err(error);
        }
        let mut pairings = self.pairings.lock().expect("信任锁");
        let Some(record) = pairings.get(write.pairing.as_str()).cloned() else {
            return Err(PortError::NotFound(EntityRef::Pairing(write.pairing)));
        };
        if record.state().is_terminal() {
            return Err(terminal_conflict(record.state()));
        }
        if record.claimed_at().is_none() {
            return Err(PortError::InvalidRequest("pairing has not been claimed"));
        }
        if record.state() != PairingState::PendingConfirmation {
            return Err(PortError::Conflict(ConflictKind::Consumed));
        }
        let Some(peer) = self
            .peers
            .lock()
            .expect("信任锁")
            .get(write.pairing.as_str())
            .cloned()
        else {
            return Err(PortError::Corrupt("claimed pairing has no peer row"));
        };
        let reference = match peer.id() {
            PeerIdentity::Device(id) => TrustRecordRef::Device(id.clone()),
            PeerIdentity::Node(id) => TrustRecordRef::Node(id.clone()),
        };
        let terminal = match &write.settlement {
            PairingSettlement::Rejected { .. } => PairingState::Rejected,
            PairingSettlement::Approved {
                granted_scopes,
                granted_grants,
            } => {
                if write.context.at.as_str() >= record.expires_at().as_str() {
                    return Err(PortError::Conflict(ConflictKind::Expired));
                }
                if !is_subset(granted_scopes.iter(), record.requested_scopes().iter())
                    || !is_subset(granted_grants.iter(), record.requested_grants().iter())
                {
                    return Err(PortError::InvalidRequest(
                        "granted sets must not exceed the requested sets",
                    ));
                }
                match record.target() {
                    PairingTarget::Device => {
                        if !granted_grants.is_empty() {
                            return Err(PortError::InvalidRequest(
                                "a device pairing must not carry grants",
                            ));
                        }
                        self.approve_device(&peer, granted_scopes, &write.context.at);
                    }
                    PairingTarget::Node => {
                        if !granted_scopes.is_empty() {
                            return Err(PortError::InvalidRequest(
                                "a node pairing must not carry scopes",
                            ));
                        }
                        self.approve_node(&peer, granted_grants, &write.context.at);
                    }
                }
                PairingState::Approved
            }
        };
        self.append_audits(&write.context.at, write.context.audit);
        let settled = PairingRecord::try_new(
            record.id().clone(),
            record.target(),
            terminal,
            record.display_name().map(str::to_owned),
            record.requested_scopes().clone(),
            record.requested_grants().clone(),
            record.secret_digest().clone(),
            record.host_binding(),
            record.created_at().clone(),
            record.expires_at().clone(),
            record.claimed_at().cloned(),
            (terminal == PairingState::Approved).then(|| write.context.at.clone()),
            terminal.is_terminal().then(|| write.context.at.clone()),
        )
        .expect("落定后的配对记录合法");
        pairings.insert(write.pairing.as_str().to_owned(), settled);
        Ok(reference)
    }

    async fn expire_pairings(&self, _write: ExpiryWrite) -> Result<u64, PortError> {
        unreachable!("{NOT_TOUCHED}")
    }

    /// 与存储层同款（§11.6 第 8 条）：主体与审计归因一致 → 读配对行/对端行 → 对端同类同 id → 只有
    /// `approved` 能推进到 `consumed`（`terminal_at` 取本次 `at`，审计同写集，并推进对端节点行的
    /// `last_connected_at`），已是 `consumed` 且对端一致时幂等成功（不覆盖首次时间、不重复写审计）。
    async fn consume_pairing(&self, write: PairingConsumption) -> Result<PairingRecord, PortError> {
        if write
            .context
            .audit
            .iter()
            .any(|row| row.actor != write.actor)
        {
            return Err(PortError::InvalidRequest(
                "consume_pairing actor must match every audit row",
            ));
        }
        let mut pairings = self.pairings.lock().expect("信任锁");
        let Some(record) = pairings.get(write.pairing.as_str()).cloned() else {
            return Err(PortError::NotFound(EntityRef::Pairing(write.pairing)));
        };
        let Some(peer) = self
            .peers
            .lock()
            .expect("信任锁")
            .get(write.pairing.as_str())
            .cloned()
        else {
            return Err(PortError::Corrupt("pairing has no peer row"));
        };
        if !peer_matches_actor(&write.actor, peer.id()) {
            return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
        }
        match record.state() {
            PairingState::Consumed => return Ok(record),
            PairingState::Approved => {}
            state if state.is_terminal() => return Err(terminal_conflict(state)),
            _ => return Err(PortError::InvalidRequest("pairing has not been approved")),
        }
        let consumed = PairingRecord::try_new(
            record.id().clone(),
            record.target(),
            PairingState::Consumed,
            record.display_name().map(str::to_owned),
            record.requested_scopes().clone(),
            record.requested_grants().clone(),
            record.secret_digest().clone(),
            record.host_binding(),
            record.created_at().clone(),
            record.expires_at().clone(),
            record.claimed_at().cloned(),
            record.approved_at().cloned(),
            Some(write.context.at.clone()),
        )
        .expect("消费后的配对记录合法");
        self.append_audits(&write.context.at, write.context.audit);
        if let Actor::Node { node, .. } = &write.actor {
            self.advance_node_connected_at(node, NodeKind::Access, &write.context.at)?;
        }
        pairings.insert(write.pairing.as_str().to_owned(), consumed.clone());
        Ok(consumed)
    }

    /// 与存储层同款（§11.6 第 9 条）：推进 `last_connected_at`（只前进不倒退）与 `context.audit`
    /// 同一写集；行不存在 → `NotFound(Node)`。
    async fn record_node_connected(&self, write: NodeConnectedWrite) -> Result<(), PortError> {
        self.advance_node_connected_at(&write.node, write.kind, &write.context.at)?;
        self.append_audits(&write.context.at, write.context.audit);
        Ok(())
    }
}

/// 主体与对端身份的一致性（与 `core::use_cases` 的同名判定同口径）。
fn peer_matches_actor(actor: &Actor, peer: &PeerIdentity) -> bool {
    match (actor, peer) {
        (Actor::Device { device, .. }, PeerIdentity::Device(id)) => device == id,
        (Actor::Node { node, .. }, PeerIdentity::Node(id)) => node == id,
        _ => false,
    }
}

impl FakeTrust {
    /// 认证收尾推进节点行的 `last_connected_at`（§11.6 第 9 条）：与存储层同款——只前进不倒退、
    /// 不抹掉已存值（比较按固定宽度 UTC 毫秒文本的字典序）；行不存在 → `NotFound(Node)`。
    ///
    /// 替身只改这一个字段（不像 `upsert_node` 那样重写整行），撤销时间与原因保持原样。
    fn advance_node_connected_at(
        &self,
        node: &NodeId,
        kind: NodeKind,
        at: &Timestamp,
    ) -> Result<(), PortError> {
        let mut nodes = self.nodes.lock().expect("信任锁");
        let key = (node.as_str().to_owned(), kind.as_str().to_owned());
        let Some(record) = nodes.get(&key).cloned() else {
            return Err(PortError::NotFound(EntityRef::Node(node.clone())));
        };
        if record
            .last_connected_at()
            .is_some_and(|stored| stored.as_str() >= at.as_str())
        {
            return Ok(());
        }
        let advanced = NodeRecord::try_new(
            record.node_id().clone(),
            record.display_name(),
            record.kind(),
            record.node_public_key_fingerprint().clone(),
            record.grants().clone(),
            record.state(),
            record.owner_endpoint().map(str::to_owned),
            record.created_at().clone(),
            Some(at.clone()),
            record.revoked_at().cloned(),
        )
        .expect("推进时间后的节点记录合法");
        nodes.insert(key, advanced);
        Ok(())
    }

    fn append_audits(&self, at: &Timestamp, audit: Vec<PendingAudit>) {
        let mut audits = self.audits.lock().expect("信任锁");
        for pending in audit {
            audits.push(
                AuditRecord::try_new(
                    at.clone(),
                    pending.action,
                    pending.actor,
                    pending.via_node,
                    pending.local_principal_ref,
                    pending.target,
                    pending.outcome,
                    pending.detail_digest,
                )
                .expect("审计行合法"),
            );
        }
    }

    /// 批准（设备）：与存储层的 `approve_device` 同形状。
    fn approve_device(&self, peer: &PairingPeer, granted_scopes: &ScopeSet, at: &Timestamp) {
        let PeerIdentity::Device(device_id) = peer.id() else {
            unreachable!("目标族已由写集保证")
        };
        let record = DeviceRecord::try_new(
            device_id.clone(),
            peer.display_name(),
            peer.public_key_fingerprint(),
            granted_scopes.clone(),
            DeviceState::Active,
            at.clone(),
            None,
            None,
        )
        .expect("设备记录合法");
        self.seed_device(record);
    }

    /// 批准（节点）：对端角色恒为 `Access`，`ownerEndpoint` 为 `None`。
    fn approve_node(&self, peer: &PairingPeer, granted_grants: &GrantSet, at: &Timestamp) {
        let PeerIdentity::Node(node_id) = peer.id() else {
            unreachable!("目标族已由写集保证")
        };
        let record = NodeRecord::try_new(
            node_id.clone(),
            peer.display_name(),
            NodeKind::Access,
            peer.public_key_fingerprint(),
            granted_grants.clone(),
            NodeState::Paired,
            None,
            at.clone(),
            None,
            None,
        )
        .expect("节点记录合法");
        self.nodes.lock().expect("信任锁").insert(
            (
                node_id.as_str().to_owned(),
                NodeKind::Access.as_str().to_owned(),
            ),
            record,
        );
        // 与存储层同款（§11.6 第 4 条）：批准把对端公钥从 `owned_pairing_peer` 转入 `owned_peer_key`，
        // 它是握手验签材料的**唯一**来源（`IDENTITY_AND_AUTH_CONTRACT.md` §5.1），既有行允许沿用。
        self.keys.lock().expect("信任锁").insert(
            (peer.id().kind().to_owned(), peer.id().id_text().to_owned()),
            peer.public_key().clone(),
        );
    }
}

/// `left ⊆ right`（存储层的同款判定：集合已去重）。
fn is_subset<'a>(
    left: impl Iterator<Item = &'a str>,
    right: impl Iterator<Item = &'a str>,
) -> bool {
    let right: Vec<&str> = right.collect();
    left.into_iter().all(|item| right.contains(&item))
}

/// 终态配对上的操作（存储层的 `terminal_conflict`）：`expired` → `Expired`，其余终态 → `Consumed`。
fn terminal_conflict(state: PairingState) -> PortError {
    if state == PairingState::Expired {
        PortError::Conflict(ConflictKind::Expired)
    } else {
        PortError::Conflict(ConflictKind::Consumed)
    }
}

// ---------------------------------------------------------------------------------------------
// 熵源与连接关闭（配对方法的注入口）
// ---------------------------------------------------------------------------------------------

/// 确定性熵源（只为可判定，不作密码学用途）。
#[derive(Clone, Default)]
pub(crate) struct FakeEntropy {
    state: Arc<Mutex<u64>>,
}

impl EntropySource for FakeEntropy {
    fn fill(&self, out: &mut [u8]) -> Result<(), EntropyError> {
        let mut state = self.state.lock().expect("熵源锁");
        for byte in out.iter_mut() {
            *state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            *byte = (*state >> 33) as u8;
        }
        Ok(())
    }
}

/// 记录撤销后关闭了哪些设备/节点（断言「提交后才关闭」的观察点）。
#[derive(Clone, Default)]
pub(crate) struct RecordingCloser {
    devices: Arc<Mutex<Vec<String>>>,
    nodes: Arc<Mutex<Vec<String>>>,
}

impl RecordingCloser {
    pub(crate) fn closed_devices(&self) -> Vec<String> {
        self.devices.lock().expect("关闭锁").clone()
    }

    pub(crate) fn closed_nodes(&self) -> Vec<String> {
        self.nodes.lock().expect("关闭锁").clone()
    }
}

#[async_trait::async_trait]
impl ConnectionCloser for RecordingCloser {
    async fn close_device(&self, device: &DeviceId) {
        self.devices
            .lock()
            .expect("关闭锁")
            .push(device.as_str().to_owned());
    }

    async fn close_node(&self, node: &NodeId) {
        self.nodes
            .lock()
            .expect("关闭锁")
            .push(node.as_str().to_owned());
    }
}

// ---------------------------------------------------------------------------------------------
// 用例自建的临时路径
// ---------------------------------------------------------------------------------------------

/// 用例自建临时目录的守卫：`Drop` 时删除（正常结束与 panic 展开两条路径都生效）。
///
/// `Deref<Target = Path>` 让 `directory.join(..)`、`directory.to_str()`、`&directory`（`&Path` 形参）
/// 照常工作；`AsRef<Path>` 让 `fs::remove_dir_all(&directory)` 这类泛型入参也直接收。`pub(crate)` 只在
/// `#[cfg(test)]` 下存在（`mod.rs` 的声明）。
pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// 在系统临时目录下新建唯一子目录（`name` 必须已含 pid/序列号等唯一化成分）。
    #[must_use]
    pub(crate) fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("临时目录可建");
        Self { path }
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // 尽力而为，且**不 panic**（展开中 panic 会 abort）：目录可能已被用例自己删掉（`NotFound`
        // 立即返回），Windows 上也可能因句柄释放/扫描瞬时占用而失败——此时重试若干次（与 `app` 测试的
        // `TempRoot` 同一口径）。
        for _ in 0..10 {
            match std::fs::remove_dir_all(&self.path) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }
    }
}

/// 用例自建临时**文件**的守卫（`Drop` 时 `remove_file`；文件由用例或被测代码创建）。
pub(crate) struct TempFile {
    path: PathBuf,
}

impl TempFile {
    /// 在系统临时目录下取一个唯一文件路径（**不**创建文件；`name` 必须已含唯一化成分）。
    #[must_use]
    pub(crate) fn new(name: &str) -> Self {
        Self {
            path: std::env::temp_dir().join(name),
        }
    }
}

impl std::ops::Deref for TempFile {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TempFile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // 同 `TempDir`：尽力而为、不 panic，Windows 上的瞬时占用重试若干次。
        for _ in 0..10 {
            match std::fs::remove_file(&self.path) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 测试世界
// ---------------------------------------------------------------------------------------------

/// 测试世界默认的本机 canonical origin（`daemon.public_origin`；组合根注入到 `PairingSessions`）。
pub(crate) const TEST_PUBLIC_ORIGIN: &str = "https://work-pc.example.test";

/// 一套 fake 世界 + 由它装配出的路由，附带测试要观察的句柄。
pub(crate) struct TestWorld {
    pub(crate) clock: FakeClock,
    pub(crate) config: FakeConfig,
    pub(crate) exports: FakeExports,
    pub(crate) audit: FakeAudit,
    pub(crate) keystore: FakeKeystore,
    pub(crate) daemon: FakeDaemon,
    pub(crate) trust: FakeTrust,
    pub(crate) authority: Arc<Authority>,
    pub(crate) closer: Arc<RecordingCloser>,
    pub(crate) core: Arc<UseCases>,
    public_origin: Option<String>,
    temporary: Arc<Mutex<Vec<PathBuf>>>,
}

impl TestWorld {
    pub(crate) fn new() -> Self {
        Self::with_public_origin(Some(TEST_PUBLIC_ORIGIN))
    }

    /// 显式控制 `daemon.public_origin`（`None` 覆盖「未配置 → 配对方法失败关闭」的分支）。
    pub(crate) fn with_public_origin(origin: Option<&str>) -> Self {
        Self::build(origin, store_arc())
    }

    /// 用带固定水位的存储替身装配（Node Link 握手视图读 `head()` 的用例）。
    pub(crate) fn with_store(store: Arc<dyn SessionStore>) -> Self {
        Self::build(Some(TEST_PUBLIC_ORIGIN), store)
    }

    fn build(origin: Option<&str>, store: Arc<dyn SessionStore>) -> Self {
        let clock = FakeClock::new();
        let keystore = FakeKeystore::default();
        let trust = FakeTrust::default();
        let exports = FakeExports::default().with_clock(clock.clone());
        let audit = FakeAudit::default();
        let config = FakeConfig::default();
        let authority = Arc::new(Authority::new(
            NodeId::new("bdb2ec20-f98c-4d87-b789-e540d527ef87").expect("本机 node id"),
            KeyHandle::new("node-identity").expect("key handle"),
            Arc::new(keystore.clone()),
            Arc::new(FakeEntropy::default()),
            Arc::new(clock.clone()),
        ));
        let core = Arc::new(Self::assemble_use_cases(
            &clock,
            &store,
            &deliveries_arc(),
            &exports,
            &audit,
            &config,
            &trust,
        ));
        Self {
            clock,
            config,
            exports,
            audit,
            keystore,
            daemon: FakeDaemon::default(),
            trust,
            authority,
            closer: Arc::new(RecordingCloser::default()),
            core,
            public_origin: origin.map(str::to_owned),
            temporary: Arc::default(),
        }
    }

    /// 端口 → `UseCases` 的装配（`router()` 与测试共用同一份实例：测试可以像 HTTPS claim 路径
    /// 那样直接调 `UseCases::claim_pairing` 模拟认领）。
    #[allow(clippy::too_many_arguments)] // 与 `UseCaseDeps` 的字段一一对应
    fn assemble_use_cases(
        clock: &FakeClock,
        store: &Arc<dyn SessionStore>,
        deliveries: &Arc<dyn RemoteDeliveryStore>,
        exports: &FakeExports,
        audit: &FakeAudit,
        config: &FakeConfig,
        trust: &FakeTrust,
    ) -> UseCases {
        let clock: Arc<dyn Clock> = Arc::new(clock.clone());
        let exports: Arc<dyn ExportStore> = Arc::new(exports.clone());
        let audit: Arc<dyn AuditStore> = Arc::new(audit.clone());
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: store.clone(),
                deliveries: deliveries.clone(),
                backends: Arc::new(NotTouched),
                exports: exports.clone(),
                trust: Arc::new(trust.clone()),
                publisher: Arc::new(NotTouched),
                clock: clock.clone(),
                ids: Arc::new(FakeIds::default()),
                audit: Some(audit.clone()),
            },
            BrokerConfig::default(),
        ));
        UseCases::new(UseCaseDeps {
            broker,
            store: store.clone(),
            deliveries: deliveries.clone(),
            exports,
            trust: Arc::new(trust.clone()),
            audit,
            config: Arc::new(config.clone()),
            attachments: Arc::new(NotTouched),
            catalog: Arc::new(FakeCatalog::with_agents(vec![
                AgentRef::try_new(AgentId::new("codex").expect("agent id"), "Codex")
                    .expect("agent ref"),
            ])),
            clock,
            ids: Arc::new(FakeIds::default()),
        })
    }

    pub(crate) fn router(&self) -> LocalAdminRouter {
        LocalAdminRouter::new(LocalAdminDeps {
            daemon: Arc::new(self.daemon.clone()),
            core: self.core.clone(),
            keystore: Arc::new(self.keystore.clone()),
            audit: Arc::new(self.audit.clone()),
            clock: Arc::new(self.clock.clone()),
            pairing: Arc::new(PairingSessions::new(
                self.authority.clone(),
                self.public_origin.clone(),
                self.closer.clone(),
            )),
        })
    }

    /// 模拟 Owner 侧的 claim（HTTPS claim 路径属后续切片）：只提交与存储层相同的事实
    /// （peer 行 + 状态推进到 `pending_confirmation`），HMAC/绑定校验不在本替身的职责里。
    ///
    /// 请求集合传空集：存储层保留的是登记值（`owned_pairing.requested_*`），claim 的请求集合只用于
    /// `verify_claim` 的「不得超出登记值」判定，不落库（`storage-sqlite` 同款）。
    pub(crate) async fn claim(
        &self,
        pairing_id: &PairingId,
        peer: PairingPeer,
    ) -> Result<PairingClaimOutcome, PortError> {
        let claim = PairingClaim::try_new(
            pairing_id.clone(),
            peer,
            ScopeSet::empty(),
            GrantSet::empty(),
        )
        .expect("测试用的 claim 载荷与目标族一致");
        self.core.claim_pairing(&Actor::LocalCli, claim).await
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
