//! 启动配置的加载与校验（唯一权威：`docs/CONFIG_REFERENCE.md`）。
//!
//! 边界纪律：
//!
//! - **只读**：本模块不回写配置文件（因此 workspace 不引入 `toml_edit`），管理状态的权威是 SQLite。
//! - **不新增配置键**：键名、类型与默认值逐条来自 `CONFIG_REFERENCE.md`；本切片消费
//!   `daemon.*`/`storage.*`/`agents.*`（种子）/`identity.*`/`logging.*`/`dev_mode.*`，其余段落解析后
//!   保留为「已知但未接线」并在启动日志里注明（`app::daemon` 记 debug 级），既不报错也不生效。
//! - **明确 schema**：未知键一律拒绝（`SECURITY_DESIGN.md` §12.1）；启动配置里出现 `imports`/`exports`
//!   必须明确拒绝并指向本地管理命令（`CONFIG_REFERENCE.md` 的「配置与管理状态的权威」）。
//! - **失败关闭**：解析或校验失败即拒绝启动，不「猜一个默认值继续跑」。
//!
//! 配置来源与优先级（`CONFIG_REFERENCE.md`）：命令行参数 > 配置文件 > 内置默认值；环境变量只用于
//! 替换配置文件路径（`ACP_REMOTE_CONFIG`），不承载业务配置。

use std::path::{Path, PathBuf};

use acp_core::model::{AgentId, AgentProfile, ProviderEnvBinding};
use serde::Deserialize;
use storage_sqlite::migrate::StorageConfig as SqliteStorageConfig;

/// `daemon.data_dir` 未配置时的目录名（平台用户配置目录下的 `acp-remote/`）。
const APP_DIRECTORY: &str = "acp-remote";

/// 默认配置文件在平台用户配置目录下的相对路径。
const DEFAULT_CONFIG_FILE: &str = "config.toml";

/// 配置文件路径的环境变量（`CONFIG_REFERENCE.md`：环境变量只用于替换配置文件路径）。
pub const CONFIG_PATH_ENV: &str = "ACP_REMOTE_CONFIG";

/// 配置加载失败。所有变体都是失败关闭：调用方必须拒绝启动。
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// 显式给出的配置文件不存在或不可读。
    #[error("配置文件不可读（{source}）")]
    Unreadable {
        /// 底层错误。
        #[source]
        source: std::io::Error,
    },
    /// TOML 语法或 schema 不符（含未知键）。
    #[error("配置文件非法：{detail}")]
    Invalid {
        /// 不含配置值的简短说明。
        detail: String,
    },
    /// 启动配置里出现 `imports`/`exports`：管理状态不接受启动配置输入。
    #[error(
        "启动配置不得出现 `{section}`：Import/Export 只能由本地管理命令维护（CONFIG_REFERENCE.md）"
    )]
    ManagedSectionInStartupConfig {
        /// 段落名。
        section: &'static str,
    },
    /// 无法确定平台默认目录（配置与数据目录都没有显式给出）。
    #[error("无法确定平台默认配置目录：请显式配置 `daemon.data_dir`")]
    NoDefaultDirectory,
}

/// `identity.keystore` 的取值（`CONFIG_REFERENCE.md` §8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeystoreChoice {
    /// `platform`：平台安全存储（Windows DPAPI 包裹；其它平台无可用后端）。
    Platform,
    /// `ephemeral`：进程内 keystore，**只允许**在显式开发模式下选择。
    Ephemeral,
}

/// `dev_mode.*`（`CONFIG_REFERENCE.md` §10）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DevMode {
    /// 发布构建默认关闭；启用时启动输出必须显示非安全状态。
    pub enabled: bool,
    /// 允许明文 HTTP/WS（只允许 loopback）。本切片没有网络 listener，因此**未接线**。
    pub allow_plaintext: bool,
    /// 使用进程期临时身份。
    pub ephemeral_identity: bool,
}

/// `logging.*`（`CONFIG_REFERENCE.md` §11）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggingConfig {
    /// 最高级别（`trace`|`debug`|`info`|`warn`|`error`）。
    pub level: tracing::Level,
    /// 输出格式（`text`|`json`）。
    pub format: LogFormat,
    /// 输出文件；`None` = stderr。
    pub file: Option<PathBuf>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: tracing::Level::INFO,
            format: LogFormat::Text,
            file: None,
        }
    }
}

/// `logging.format` 的两个取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// `text`：一行一条 `key=value` 记录。
    Text,
    /// `json`：一行一个 JSON 对象（字段集合与 `text` 相同）。
    Json,
}

/// `[[agents.profiles]]` 的一条种子（`CONFIG_REFERENCE.md` §7）。
///
/// 配置里是「首次初始化种子」：只在管理存储尚未初始化时导入一次，之后 SQLite 是唯一权威。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedProfile {
    /// Agent selector。
    pub agent_id: AgentId,
    /// 展示名。配置文件示例未给出该键，缺省时回落为 `agent_id`（见 `RAW_PROFILE_DISPLAY_NAME` 注记）。
    pub display_name: String,
    /// 可执行文件路径或名字（不经 shell 拼接）。
    pub command: String,
    /// 逐项传给子进程的参数。
    pub args: Vec<String>,
    /// 环境变量白名单（注入集合的上限）。
    pub env_allowlist: Vec<String>,
    /// 凭据到环境变量的绑定（值来自平台 keystore，永不落配置文件）。
    pub env: Vec<ProviderEnvBinding>,
    /// 是否为默认 profile（全库至多一个）。
    pub default: bool,
}

impl SeedProfile {
    /// 以给定时间戳构造 `core` 的 `AgentProfile`（种子导入用）。
    pub fn to_profile(&self, at: &acp_core::model::Timestamp) -> Result<AgentProfile, ConfigError> {
        AgentProfile::try_new(
            self.agent_id.clone(),
            &self.display_name,
            &self.command,
            self.args.clone(),
            self.env_allowlist.clone(),
            self.env.clone(),
            self.default,
            at.clone(),
            at.clone(),
        )
        .map_err(|error| ConfigError::Invalid {
            detail: format!(
                "`[[agents.profiles]]` 第 `{}` 项非法：{error}",
                self.agent_id
            ),
        })
    }
}

/// 已解析并校验的启动配置。
///
/// `unwired` 是「已知但未接线」的键名列表（本切片不消费的段落与实际出现的键），由启动日志以 debug
/// 级注明；它不影响行为，也不是错误。
#[derive(Debug)]
pub struct Config {
    /// `daemon.data_dir`（绝对路径；SQLite、附件、日志与锁文件的根）。
    pub data_dir: PathBuf,
    /// `daemon.public_origin`（canonical public origin；未配置为 `None`）。
    pub public_origin: Option<String>,
    /// `daemon.shutdown_grace_ms`。
    pub shutdown_grace_ms: u64,
    /// 存储参数（`storage.*` 的已接线部分）。
    pub storage: SqliteStorageConfig,
    /// 首次种子导入的 profile。
    pub seeds: Vec<SeedProfile>,
    /// `identity.keystore`。
    pub keystore: KeystoreChoice,
    /// `identity.fail_closed_on_missing_keystore`。
    pub fail_closed_on_missing_keystore: bool,
    /// `logging.*`。
    pub logging: LoggingConfig,
    /// `dev_mode.*`。
    pub dev_mode: DevMode,
    /// 「已知但未接线」的键。
    pub unwired: Vec<String>,
}

impl Config {
    /// 加载配置：`path` 非空即使用该文件（CLI 参数或 `ACP_REMOTE_CONFIG`），为空时使用平台默认路径。
    ///
    /// 显式给出的文件缺失即失败（调用方表达了明确意图）；默认路径缺失时按内置默认值继续，并返回
    /// [`ConfigLoad::default_file_missing`] 供调用方记一条提示。
    pub fn load(path: Option<&Path>) -> Result<Loaded, ConfigError> {
        let explicit = path.is_some();
        let path = match path {
            Some(path) => path.to_path_buf(),
            None => default_app_directory()?.join(DEFAULT_CONFIG_FILE),
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if explicit {
                    return Err(ConfigError::Unreadable { source: error });
                }
                None
            }
            Err(source) => return Err(ConfigError::Unreadable { source }),
        };
        Ok(Loaded {
            default_file_missing: text.is_none(),
            config: Self::from_toml(text.as_deref().unwrap_or(""))?,
        })
    }

    /// 解析并校验一份 TOML 文本（空文本 = 全部取内置默认值）。
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(text).map_err(|error| ConfigError::Invalid {
            detail: toml_error_detail(&error.to_string()),
        })?;
        for (section, present) in [
            ("imports", raw.imports.is_some()),
            ("exports", raw.exports.is_some()),
        ] {
            if present {
                return Err(ConfigError::ManagedSectionInStartupConfig { section });
            }
        }
        Self::resolve(raw)
    }

    fn resolve(raw: RawConfig) -> Result<Self, ConfigError> {
        let daemon = raw.daemon.unwrap_or_default();
        let data_dir = match daemon.data_dir.as_deref() {
            Some(text) => PathBuf::from(text),
            None => default_app_directory()?,
        };
        if !data_dir.is_absolute() {
            return Err(ConfigError::Invalid {
                detail: "`daemon.data_dir` 必须是绝对路径".to_owned(),
            });
        }
        // 显式 endpoint 值只用于测试或路径冲突排查（§1），而本切片只能创建平台默认位置
        // （`server::transport::local` 的 `LocalEndpointConfig` 没有显式路径入口）。静默按 `auto`
        // 启动会让管理通道落在与用户配置不同的位置，因此失败关闭。
        match daemon
            .local_admin
            .as_ref()
            .and_then(|section| section.endpoint.as_deref())
        {
            None | Some("auto") => {}
            Some(_) => {
                return Err(ConfigError::Invalid {
                    detail: "`daemon.local_admin.endpoint` 在本切片只支持 `auto`".to_owned(),
                });
            }
        }
        let shutdown_grace_ms = daemon.shutdown_grace_ms.unwrap_or(10_000);
        let shutdown_grace_ms =
            u64::try_from(shutdown_grace_ms).map_err(|_| ConfigError::Invalid {
                detail: "`daemon.shutdown_grace_ms` 必须是 0..=60000 的整数".to_owned(),
            })?;
        if shutdown_grace_ms > 60_000 {
            return Err(ConfigError::Invalid {
                detail: "`daemon.shutdown_grace_ms` 必须是 0..=60000 的整数".to_owned(),
            });
        }
        let public_origin = daemon.public_origin.clone();

        let storage_section = raw.storage.unwrap_or_default();
        let storage = storage_section.to_storage_config(&data_dir)?;

        let mut unwired = Vec::new();
        if daemon.listen.is_some() {
            unwired.push("daemon.listen".to_owned());
        }
        if daemon.allowed_hosts.is_some() {
            unwired.push("daemon.allowed_hosts".to_owned());
        }
        if daemon.trusted_proxies.is_some() {
            unwired.push("daemon.trusted_proxies".to_owned());
        }
        if let Some(tls) = &daemon.tls {
            for key in tls.present_keys() {
                unwired.push(key.to_owned());
            }
        }
        // `daemon.instance_lock = "ipc"`：本切片的单实例锁恒为 OS advisory 文件锁（`fs4`），两者互斥
        // 语义相同，因此按「已知但未接线」处理并在启动日志里显式说明，不静默换实现。
        if let Some(instance_lock) = daemon.instance_lock.as_deref() {
            match instance_lock {
                "file" => {}
                "ipc" => unwired.push("daemon.instance_lock(ipc)".to_owned()),
                other => {
                    return Err(ConfigError::Invalid {
                        detail: format!(
                            "`daemon.instance_lock` 只能是 `file` 或 `ipc`（收到 `{other}`）"
                        ),
                    });
                }
            }
        }
        for key in raw
            .sync
            .as_ref()
            .map_or_else(Vec::new, RawSync::present_keys)
        {
            unwired.push(key.to_owned());
        }
        for key in raw
            .node_link
            .as_ref()
            .map_or_else(Vec::new, RawNodeLink::present_keys)
        {
            unwired.push(key.to_owned());
        }
        for key in raw
            .sessions
            .as_ref()
            .map_or_else(Vec::new, RawSessions::present_keys)
        {
            unwired.push(key.to_owned());
        }
        for key in raw
            .terminal
            .as_ref()
            .map_or_else(Vec::new, RawTerminal::present_keys)
        {
            unwired.push(key.to_owned());
        }
        if storage_section.flush_interval_ms.is_some() {
            unwired.push("storage.flush_interval_ms".to_owned());
        }

        let seeds = match raw.agents {
            Some(agents) => agents.into_seed_profiles()?,
            None => Vec::new(),
        };

        let identity_section = raw.identity.unwrap_or_default();
        let keystore = match identity_section.keystore.as_deref() {
            None | Some("platform") => KeystoreChoice::Platform,
            Some("ephemeral") => KeystoreChoice::Ephemeral,
            Some(other) => {
                return Err(ConfigError::Invalid {
                    detail: format!(
                        "`identity.keystore` 只能是 `platform` 或 `ephemeral`（收到 `{other}`）"
                    ),
                });
            }
        };
        let fail_closed_on_missing_keystore = identity_section
            .fail_closed_on_missing_keystore
            .unwrap_or(true);

        let dev_mode = raw
            .dev_mode
            .map_or_else(DevMode::default, |section| DevMode {
                enabled: section.enabled.unwrap_or(false),
                allow_plaintext: section.allow_plaintext.unwrap_or(false),
                ephemeral_identity: section.ephemeral_identity.unwrap_or(false),
            });
        if keystore == KeystoreChoice::Ephemeral && !dev_mode.enabled {
            return Err(ConfigError::Invalid {
                detail:
                    "`identity.keystore = \"ephemeral\"` 只允许在 `dev_mode.enabled = true` 时使用"
                        .to_owned(),
            });
        }
        if !fail_closed_on_missing_keystore && !dev_mode.enabled {
            return Err(ConfigError::Invalid {
                detail:
                    "`identity.fail_closed_on_missing_keystore = false` 只能出现在开发模式配置中"
                        .to_owned(),
            });
        }
        if dev_mode.allow_plaintext {
            unwired.push("dev_mode.allow_plaintext".to_owned());
        }

        let logging = raw
            .logging
            .map_or_else(|| Ok(LoggingConfig::default()), RawLogging::into_config)?;

        Ok(Self {
            data_dir,
            public_origin,
            shutdown_grace_ms,
            storage,
            seeds,
            keystore,
            fail_closed_on_missing_keystore,
            logging,
            dev_mode,
            unwired,
        })
    }
}

/// [`Config::load`] 的返回值。
#[derive(Debug)]
pub struct Loaded {
    /// 已解析的配置。
    pub config: Config,
    /// 平台默认配置文件不存在（按内置默认值继续）。
    pub default_file_missing: bool,
}

/// 从环境变量与平台默认值取配置文件路径的优先级实现（`CONFIG_REFERENCE.md`）。
pub fn config_path_from_env() -> Option<PathBuf> {
    std::env::var_os(CONFIG_PATH_ENV).map(PathBuf::from)
}

/// 平台用户配置目录下的 `acp-remote/`（`daemon.data_dir` 与默认配置文件位置）。
fn default_app_directory() -> Result<PathBuf, ConfigError> {
    #[cfg(windows)]
    {
        if let Some(base) = std::env::var_os("APPDATA") {
            return Ok(PathBuf::from(base).join(APP_DIRECTORY));
        }
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            return Ok(PathBuf::from(profile)
                .join("AppData")
                .join("Roaming")
                .join(APP_DIRECTORY));
        }
    }
    #[cfg(unix)]
    {
        if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(base).join(APP_DIRECTORY));
        }
        if let Some(home) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(home).join(".config").join(APP_DIRECTORY));
        }
    }
    Err(ConfigError::NoDefaultDirectory)
}

/// 把 toml 的多行诊断压成一句可进日志/CLI 的错误文本。
///
/// toml 的错误形如：
///
/// ```text
/// TOML parse error at line 2, column 1
///   |
/// 2 | data_diry = "/tmp/x"
///   | ^^^^^^^^
/// unknown field `data_diry`, expected one of …
/// ```
///
/// 取**错误正文**（首个不缩进且不是定位行的行）：定位行只有行列号，没有判据；源码片段会回显用户配置
/// 文本，不进日志。
fn toml_error_detail(text: &str) -> String {
    let body = text
        .lines()
        .map(str::trim_end)
        .find(|line| {
            !line.is_empty()
                && !line.starts_with(char::is_whitespace)
                && !line.starts_with("TOML parse error")
        })
        .or_else(|| text.lines().next())
        .unwrap_or_default()
        .trim();
    let mut detail: String = body.chars().take(ERROR_DETAIL_MAX_CHARS).collect();
    if body.chars().count() > ERROR_DETAIL_MAX_CHARS {
        detail.push('…');
    }
    detail
}

/// 进日志的错误正文长度上限（单行，不含源码片段）。
const ERROR_DETAIL_MAX_CHARS: usize = 240;

// ---------------------------------------------------------------------------------------------
// 原始 TOML schema（键名逐条来自 docs/CONFIG_REFERENCE.md；未知键一律拒绝）
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    daemon: Option<RawDaemon>,
    sync: Option<RawSync>,
    node_link: Option<RawNodeLink>,
    sessions: Option<RawSessions>,
    storage: Option<RawStorage>,
    terminal: Option<RawTerminal>,
    agents: Option<RawAgents>,
    identity: Option<RawIdentity>,
    dev_mode: Option<RawDevMode>,
    logging: Option<RawLogging>,
    /// 出现即拒绝（见 [`ConfigError::ManagedSectionInStartupConfig`]）。
    imports: Option<toml::Value>,
    /// 出现即拒绝。
    exports: Option<toml::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDaemon {
    data_dir: Option<String>,
    listen: Option<String>,
    public_origin: Option<String>,
    allowed_hosts: Option<Vec<String>>,
    trusted_proxies: Option<Vec<String>>,
    instance_lock: Option<String>,
    shutdown_grace_ms: Option<i64>,
    local_admin: Option<RawLocalAdmin>,
    tls: Option<RawTls>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLocalAdmin {
    endpoint: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTls {
    mode: Option<String>,
    cert_path: Option<String>,
    key_path: Option<String>,
}

impl RawTls {
    fn present_keys(&self) -> Vec<&'static str> {
        let mut keys = Vec::new();
        if self.mode.is_some() {
            keys.push("daemon.tls.mode");
        }
        if self.cert_path.is_some() {
            keys.push("daemon.tls.cert_path");
        }
        if self.key_path.is_some() {
            keys.push("daemon.tls.key_path");
        }
        keys
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSync {
    max_message_bytes: Option<u64>,
    max_prompt_bytes: Option<u64>,
    max_replay_events_per_batch: Option<u64>,
}

impl RawSync {
    fn present_keys(&self) -> Vec<&'static str> {
        let mut keys = Vec::new();
        if self.max_message_bytes.is_some() {
            keys.push("sync.max_message_bytes");
        }
        if self.max_prompt_bytes.is_some() {
            keys.push("sync.max_prompt_bytes");
        }
        if self.max_replay_events_per_batch.is_some() {
            keys.push("sync.max_replay_events_per_batch");
        }
        keys
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNodeLink {
    max_message_bytes: Option<u64>,
    catalog_snapshot_batch_size: Option<u64>,
    resource_snapshot_batch_size: Option<u64>,
    max_in_flight_commands: Option<u64>,
    max_pending_queue_bytes: Option<u64>,
    max_pending_queue_messages: Option<u64>,
    heartbeat_interval_ms: Option<u64>,
}

impl RawNodeLink {
    fn present_keys(&self) -> Vec<&'static str> {
        let mut keys = Vec::new();
        if self.max_message_bytes.is_some() {
            keys.push("node_link.max_message_bytes");
        }
        if self.catalog_snapshot_batch_size.is_some() {
            keys.push("node_link.catalog_snapshot_batch_size");
        }
        if self.resource_snapshot_batch_size.is_some() {
            keys.push("node_link.resource_snapshot_batch_size");
        }
        if self.max_in_flight_commands.is_some() {
            keys.push("node_link.max_in_flight_commands");
        }
        if self.max_pending_queue_bytes.is_some() {
            keys.push("node_link.max_pending_queue_bytes");
        }
        if self.max_pending_queue_messages.is_some() {
            keys.push("node_link.max_pending_queue_messages");
        }
        if self.heartbeat_interval_ms.is_some() {
            keys.push("node_link.heartbeat_interval_ms");
        }
        keys
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSessions {
    queue_policy: Option<String>,
    max_queued_turns: Option<u64>,
    idle_timeout_ms: Option<u64>,
}

impl RawSessions {
    fn present_keys(&self) -> Vec<&'static str> {
        let mut keys = Vec::new();
        if self.queue_policy.is_some() {
            keys.push("sessions.queue_policy");
        }
        if self.max_queued_turns.is_some() {
            keys.push("sessions.max_queued_turns");
        }
        if self.idle_timeout_ms.is_some() {
            keys.push("sessions.idle_timeout_ms");
        }
        keys
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStorage {
    transcript_retention_days: Option<u32>,
    sync_event_retention_days: Option<u32>,
    max_total_size_bytes: Option<u64>,
    max_session_size_bytes: Option<u64>,
    persist_deltas: Option<bool>,
    flush_interval_ms: Option<u64>,
    attachment_max_file_bytes: Option<u64>,
    attachment_max_total_bytes: Option<u64>,
    attachment_dir: Option<String>,
    audit_retention_days: Option<u32>,
}

impl RawStorage {
    /// 已接线的键 → `storage-sqlite` 的配置；未给出的键取 `CORE_PORTS_AND_STORAGE.md` §7.5 的默认值。
    fn to_storage_config(&self, data_dir: &Path) -> Result<SqliteStorageConfig, ConfigError> {
        let mut config = SqliteStorageConfig::new(data_dir);
        if let Some(dir) = &self.attachment_dir {
            let dir = PathBuf::from(dir);
            if !dir.is_absolute() {
                return Err(ConfigError::Invalid {
                    detail: "`storage.attachment_dir` 必须是绝对路径".to_owned(),
                });
            }
            config.attachment_dir = dir;
        }
        if let Some(value) = self.transcript_retention_days {
            config.transcript_retention_days = value;
        }
        if let Some(value) = self.sync_event_retention_days {
            config.sync_event_retention_days = value;
        }
        if let Some(value) = self.audit_retention_days {
            config.audit_retention_days = value;
        }
        if let Some(value) = self.max_total_size_bytes {
            config.max_total_size_bytes = value;
        }
        if let Some(value) = self.max_session_size_bytes {
            config.max_session_size_bytes = value;
        }
        if let Some(value) = self.persist_deltas {
            config.persist_deltas = value;
        }
        if let Some(value) = self.attachment_max_file_bytes {
            config.attachment_max_file_bytes = value;
        }
        if let Some(value) = self.attachment_max_total_bytes {
            config.attachment_max_total_bytes = value;
        }
        // 正式模式：权限无法保证即拒绝启动（`SECURITY_DESIGN.md` §13.2）。`dev_mode` 不放宽它——
        // `CONFIG_REFERENCE.md` §10 没有「放宽存储权限」这类开关。
        config.strict_permissions = true;
        Ok(config)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTerminal {
    max_output_per_command_bytes: Option<u64>,
    keep_head_bytes: Option<u64>,
    keep_tail_bytes: Option<u64>,
}

impl RawTerminal {
    fn present_keys(&self) -> Vec<&'static str> {
        let mut keys = Vec::new();
        if self.max_output_per_command_bytes.is_some() {
            keys.push("terminal.max_output_per_command_bytes");
        }
        if self.keep_head_bytes.is_some() {
            keys.push("terminal.keep_head_bytes");
        }
        if self.keep_tail_bytes.is_some() {
            keys.push("terminal.keep_tail_bytes");
        }
        keys
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAgents {
    profiles: Option<Vec<RawProfile>>,
}

impl RawAgents {
    fn into_seed_profiles(self) -> Result<Vec<SeedProfile>, ConfigError> {
        self.profiles
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, profile)| profile.into_seed(index))
            .collect()
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    agent_id: Option<String>,
    /// `CONFIG_REFERENCE.md` §7 的示例没有给出该键，而 `AgentProfile` 要求 1..=128 的展示名：
    /// 缺省时回落为 `agent_id`（`agent.configure` 之后以 SQLite 里的值为准）。
    display_name: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    env_allowlist: Option<Vec<String>>,
    default: Option<bool>,
    env: Option<Vec<RawBinding>>,
}

impl RawProfile {
    fn into_seed(self, index: usize) -> Result<SeedProfile, ConfigError> {
        let detail = |what: &str| ConfigError::Invalid {
            detail: format!("`[[agents.profiles]]` 第 {} 项缺少 `{what}`", index + 1),
        };
        let agent_id =
            AgentId::new(&self.agent_id.ok_or_else(|| detail("agent_id"))?).map_err(|error| {
                ConfigError::Invalid {
                    detail: format!(
                        "`[[agents.profiles]]` 第 {} 项 `agent_id` 非法：{error}",
                        index + 1
                    ),
                }
            })?;
        let display_name = self
            .display_name
            .unwrap_or_else(|| agent_id.as_str().to_owned());
        let env = self
            .env
            .unwrap_or_default()
            .into_iter()
            .map(|binding| binding.into_binding(index))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SeedProfile {
            agent_id,
            display_name,
            command: self.command.ok_or_else(|| detail("command"))?,
            args: self.args.unwrap_or_default(),
            env_allowlist: self.env_allowlist.unwrap_or_default(),
            env,
            default: self.default.unwrap_or(false),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBinding {
    provider_id: Option<String>,
    field: Option<String>,
    name: Option<String>,
}

impl RawBinding {
    fn into_binding(self, index: usize) -> Result<ProviderEnvBinding, ConfigError> {
        let missing = |what: &str| ConfigError::Invalid {
            detail: format!("`[[agents.profiles.env]]` 第 {} 项缺少 `{what}`", index + 1),
        };
        ProviderEnvBinding::try_new(
            &self.provider_id.ok_or_else(|| missing("provider_id"))?,
            &self.field.ok_or_else(|| missing("field"))?,
            &self.name.ok_or_else(|| missing("name"))?,
        )
        .map_err(|error| ConfigError::Invalid {
            detail: format!("`[[agents.profiles.env]]` 第 {} 项非法：{error}", index + 1),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIdentity {
    keystore: Option<String>,
    fail_closed_on_missing_keystore: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDevMode {
    enabled: Option<bool>,
    allow_plaintext: Option<bool>,
    ephemeral_identity: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLogging {
    level: Option<String>,
    format: Option<String>,
    file: Option<String>,
}

impl RawLogging {
    fn into_config(self) -> Result<LoggingConfig, ConfigError> {
        let level = match self.level.as_deref() {
            None => tracing::Level::INFO,
            Some("trace") => tracing::Level::TRACE,
            Some("debug") => tracing::Level::DEBUG,
            Some("info") => tracing::Level::INFO,
            Some("warn") => tracing::Level::WARN,
            Some("error") => tracing::Level::ERROR,
            Some(other) => {
                return Err(ConfigError::Invalid {
                    detail: format!(
                        "`logging.level` 只能是 trace|debug|info|warn|error（收到 `{other}`）"
                    ),
                });
            }
        };
        let format = match self.format.as_deref() {
            None | Some("text") => LogFormat::Text,
            Some("json") => LogFormat::Json,
            Some(other) => {
                return Err(ConfigError::Invalid {
                    detail: format!("`logging.format` 只能是 text 或 json（收到 `{other}`）"),
                });
            }
        };
        Ok(LoggingConfig {
            level,
            format,
            file: self.file.map(PathBuf::from),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config, ConfigError> {
        Config::from_toml(text)
    }

    #[test]
    fn defaults_match_the_reference_table() {
        let config = parse("").expect("空配置必须可解析");
        assert_eq!(config.shutdown_grace_ms, 10_000);
        assert_eq!(config.storage.transcript_retention_days, 90);
        assert_eq!(config.storage.sync_event_retention_days, 7);
        assert_eq!(config.storage.audit_retention_days, 365);
        assert_eq!(config.storage.max_total_size_bytes, 2 * 1024 * 1024 * 1024);
        assert_eq!(config.storage.max_session_size_bytes, 100 * 1024 * 1024);
        assert!(!config.storage.persist_deltas);
        assert_eq!(config.storage.attachment_max_file_bytes, 20 * 1024 * 1024);
        assert_eq!(
            config.storage.attachment_max_total_bytes,
            1024 * 1024 * 1024
        );
        assert_eq!(
            config.storage.attachment_dir,
            config.data_dir.join("attachments")
        );
        assert!(config.storage.strict_permissions);
        assert!(config.public_origin.is_none());
        assert_eq!(config.keystore, KeystoreChoice::Platform);
        assert!(config.fail_closed_on_missing_keystore);
        assert_eq!(config.logging, LoggingConfig::default());
        assert_eq!(config.dev_mode, DevMode::default());
        assert!(config.seeds.is_empty());
        assert!(config.unwired.is_empty(), "{:?}", config.unwired);
    }

    #[test]
    fn consumed_keys_are_applied() {
        // `data_dir`/`attachment_dir` 必须是绝对路径：Windows 会拒绝 `/tmp/...`，因此用平台临时目录
        // 生成（并把反斜杠换成正斜杠，避开 TOML 基本字符串的转义问题）。
        let root = std::env::temp_dir()
            .join("acpr-wp4a-config-consumed")
            .display()
            .to_string()
            .replace('\\', "/");
        let config = parse(&format!(
            r#"
[daemon]
data_dir = "{root}/data"
public_origin = "https://work-pc.example.ts.net"
shutdown_grace_ms = 1500

[storage]
transcript_retention_days = 7
sync_event_retention_days = 1
audit_retention_days = 30
max_total_size_bytes = 1048576
max_session_size_bytes = 65536
persist_deltas = true
attachment_max_file_bytes = 1024
attachment_max_total_bytes = 2048
attachment_dir = "{root}/attachments"

[identity]
keystore = "platform"

[logging]
level = "debug"
format = "json"
file = "{root}/acpr.log"

[dev_mode]
enabled = true
ephemeral_identity = true

[[agents.profiles]]
agent_id = "codex"
command = "codex-acp"
args = ["--acp"]
env_allowlist = ["OPENAI_API_KEY"]
default = true

[[agents.profiles.env]]
provider_id = "openai"
field = "api_key"
name = "OPENAI_API_KEY"
"#
        ))
        .expect("合法配置必须可解析");
        assert_eq!(config.data_dir, PathBuf::from(format!("{root}/data")));
        assert_eq!(
            config.public_origin.as_deref(),
            Some("https://work-pc.example.ts.net")
        );
        assert_eq!(config.shutdown_grace_ms, 1500);
        assert_eq!(config.storage.transcript_retention_days, 7);
        assert!(config.storage.persist_deltas);
        assert_eq!(
            config.storage.attachment_dir,
            PathBuf::from(format!("{root}/attachments"))
        );
        assert_eq!(
            config.logging.file.as_deref(),
            Some(Path::new(&format!("{root}/acpr.log")))
        );
        assert_eq!(config.logging.level, tracing::Level::DEBUG);
        assert_eq!(config.logging.format, LogFormat::Json);
        assert!(config.dev_mode.enabled && config.dev_mode.ephemeral_identity);
        assert_eq!(config.seeds.len(), 1);
        let seed = &config.seeds[0];
        assert_eq!(seed.agent_id.as_str(), "codex");
        assert_eq!(
            seed.display_name, "codex",
            "示例缺 display_name 时回落为 agent_id"
        );
        assert_eq!(seed.command, "codex-acp");
        assert_eq!(seed.args, vec!["--acp".to_owned()]);
        assert_eq!(seed.env.len(), 1);
        assert_eq!(seed.env[0].name(), "OPENAI_API_KEY");
        assert!(seed.default);
    }

    #[test]
    fn unwired_sections_are_accepted_and_reported() {
        let config = parse(
            r#"
[daemon]
listen = "0.0.0.0:8765"
allowed_hosts = ["example.ts.net"]
trusted_proxies = ["127.0.0.1:443"]
instance_lock = "ipc"

[daemon.tls]
mode = "direct"
cert_path = "/tmp/cert.pem"
key_path = "/tmp/key.pem"

[sync]
max_message_bytes = 1048576

[node_link]
heartbeat_interval_ms = 30000

[sessions]
idle_timeout_ms = 1000

[terminal]
keep_head_bytes = 1024

[storage]
flush_interval_ms = 250

[dev_mode]
allow_plaintext = true
"#,
        )
        .expect("未接线段落必须被接受");
        for key in [
            "daemon.listen",
            "daemon.allowed_hosts",
            "daemon.trusted_proxies",
            "daemon.instance_lock(ipc)",
            "daemon.tls.mode",
            "daemon.tls.cert_path",
            "daemon.tls.key_path",
            "sync.max_message_bytes",
            "node_link.heartbeat_interval_ms",
            "sessions.idle_timeout_ms",
            "terminal.keep_head_bytes",
            "storage.flush_interval_ms",
            "dev_mode.allow_plaintext",
        ] {
            assert!(
                config.unwired.iter().any(|key_name| key_name == key),
                "缺少 `{key}`：{:?}",
                config.unwired
            );
        }
    }

    #[test]
    fn managed_sections_and_unknown_keys_are_rejected() {
        for (text, expect) in [
            ("[[imports]]\nimport_id = \"imp-1\"\n", "imports"),
            ("[[exports]]\nexport_id = \"exp-1\"\n", "exports"),
            ("[daemon]\ndata_diry = \"/tmp/x\"\n", "data_diry"),
            ("[unknown_section]\nvalue = 1\n", "unknown_section"),
        ] {
            let error = parse(text).expect_err("必须拒绝");
            let message = error.to_string();
            assert!(message.contains(expect), "{message}");
        }
        assert!(matches!(
            parse("[[imports]]\nimport_id = \"i\"\n"),
            Err(ConfigError::ManagedSectionInStartupConfig { section: "imports" })
        ));
    }

    #[test]
    fn invalid_values_fail_closed() {
        for text in [
            "[daemon]\ndata_dir = \"relative/path\"\n",
            "[daemon]\nshutdown_grace_ms = 60001\n",
            "[daemon]\ninstance_lock = \"mutex\"\n",
            "[daemon.local_admin]\nendpoint = \"\\\\\\\\.\\\\pipe\\\\acpr\"\n",
            "[storage]\nattachment_dir = \"relative\"\n",
            "[identity]\nkeystore = \"dpapi\"\n",
            "[logging]\nlevel = \"verbose\"\n",
            "[logging]\nformat = \"xml\"\n",
            "[[agents.profiles]]\ncommand = \"codex\"\n",
            "[[agents.profiles]]\nagent_id = \"codex\"\n",
        ] {
            assert!(parse(text).is_err(), "必须拒绝：{text}");
        }
    }

    #[test]
    fn dev_mode_gates_the_soft_security_switches() {
        // `ephemeral` 只允许显式开发模式；`fail_closed_on_missing_keystore = false` 同理。
        assert!(
            parse("[identity]\nkeystore = \"ephemeral\"\n").is_err(),
            "非开发模式不得选 ephemeral"
        );
        assert!(
            parse("[identity]\nfail_closed_on_missing_keystore = false\n").is_err(),
            "非开发模式不得关闭失败关闭"
        );
        let config = parse(
            "[identity]\nkeystore = \"ephemeral\"\nfail_closed_on_missing_keystore = false\n\
             [dev_mode]\nenabled = true\nephemeral_identity = true\n",
        )
        .expect("开发模式配置必须可解析");
        assert_eq!(config.keystore, KeystoreChoice::Ephemeral);
        assert!(!config.fail_closed_on_missing_keystore);
    }
}
