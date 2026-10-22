//! 启动配置的加载与校验（唯一权威：`docs/CONFIG_REFERENCE.md`）。
//!
//! 边界纪律：
//!
//! - **只读**：本模块不回写配置文件（因此 workspace 不引入 `toml_edit`），管理状态的权威是 SQLite。
//! - **不新增配置键**：键名、类型与默认值逐条来自 `CONFIG_REFERENCE.md`；本切片消费
//!   `daemon.*`/`storage.*`/`sessions.*`/`[agent]`/`identity.*`/`logging.*`/`dev_mode.*`/`sync.*`，
//!   仅 `node_link.*` 解析后保留为「已知但未接线」并在启动日志里注明（`app::daemon` 记 debug 级），
//!   既不报错也不生效。终端输出上限不是配置键（`CONFIG_REFERENCE.md` §6）。
//! - **明确 schema**：未知键一律拒绝（`SECURITY_DESIGN.md` §12.1）；启动配置里出现 `imports`/`exports`
//!   必须明确拒绝并指向本地管理命令（`CONFIG_REFERENCE.md` 的「配置与管理状态的权威」）。
//! - **失败关闭**：解析或校验失败即拒绝启动，不「猜一个默认值继续跑」。
//!
//! 配置来源与优先级（`CONFIG_REFERENCE.md`）：命令行参数 > 配置文件 > 内置默认值；环境变量只用于
//! 替换配置文件路径（`ACP_REMOTE_CONFIG`），不承载业务配置。

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use acp_core::broker::QueuePolicy;
use acp_core::model::{AgentId, AgentProfile};
use identity_auth::CanonicalOrigin;
use serde::Deserialize;
use server::sync::conn::SyncConfig;
use server::sync::conn::limits::{
    DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_MAX_PROMPT_BYTES, DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH,
};
use server::transport::net::{DEFAULT_PORT, TlsMode};
use storage_sqlite::migrate::StorageConfig as SqliteStorageConfig;

/// `daemon.data_dir` 未配置时的目录名（平台用户配置目录下的 `acp-remote/`）。
const APP_DIRECTORY: &str = "acp-remote";

/// `storage.flush_interval_ms` 的默认值（`CONFIG_REFERENCE.md` §5）。
const DEFAULT_FLUSH_INTERVAL_MS: u64 = 250;

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
    /// 允许明文 HTTP/WS（**只允许 loopback**）。非 loopback 监听地址上该开关不生效：
    /// `NetListener::bind` 以 `NetError::PlaintextDevNonLoopback` 失败关闭（`SECURITY_DESIGN.md` §7.3）。
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

/// `sessions.*`（`CONFIG_REFERENCE.md` §4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionsConfig {
    /// 同一会话已有 active turn 时新 prompt 的处理策略。
    pub queue_policy: QueuePolicy,
    /// `queue` 策略下的排队上限。
    pub max_queued_turns: u32,
    /// 会话空闲回收（旧 `agent-host` 进程级扫描的输入）；`None` 表示不因空闲关闭会话（`0` 与未配置同义）。
    ///
    /// 注意：生命周期接线后该值**不再**交给进程级 idle sweep（它看不到查看者租约，会杀受保护会话）。
    /// 真正的保留期由 [`SessionsConfig::lifecycle`] 读取同一个 `sessions.idle_timeout_ms`；本字段只保留
    /// 配置解析口径（显式 `0` 仍被接受与校验），供只读诊断/兼容比较，不驱动任何关闭动作。
    pub idle_timeout: Option<Duration>,
    /// 本机生命周期（`core.local-session-lifecycle.v1`）：保留期与容量上限的**已校验**形态。
    ///
    /// 与 `idle_timeout` 的分工：后者是 `agent-host` 的既有空闲回收（保持旧语义），本字段服务
    /// `LocalSessionLifecycle`（显式 `0` 只关闭按时间回收，容量保护仍生效）。
    pub lifecycle: acp_core::use_cases::LifecycleConfig,
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            queue_policy: QueuePolicy::Queue,
            max_queued_turns: 16,
            idle_timeout: None,
            lifecycle: acp_core::use_cases::LifecycleConfig::new(1_800_000, 50)
                .expect("缺省保留期与容量是合法值"),
        }
    }
}

/// `[agent]` 表的解析结果：键即 `agentId`，值是非空命令字符串或完整 argv 数组；可选 `default`。
fn parse_agents(
    raw: Option<std::collections::BTreeMap<String, toml::Value>>,
) -> Result<Vec<AgentProfile>, ConfigError> {
    let Some(table) = raw else {
        return Ok(Vec::new());
    };
    let default = match table.get("default") {
        None => None,
        Some(toml::Value::String(name)) => Some(name.clone()),
        Some(_) => {
            return Err(ConfigError::Invalid {
                detail: "`agent.default` 必须是字符串".to_owned(),
            });
        }
    };
    let mut profiles = Vec::new();
    for (key, value) in &table {
        if key == "default" {
            continue;
        }
        let id = AgentId::new(key).map_err(|_| ConfigError::Invalid {
            detail: format!("`agent.{key}` 不是合法的 agentId"),
        })?;
        let (command, args) = match value {
            toml::Value::String(command) => (command.clone(), Vec::new()),
            toml::Value::Array(items) => {
                if items.is_empty() {
                    return Err(ConfigError::Invalid {
                        detail: format!("`agent.{key}` 的 argv 不得为空"),
                    });
                }
                let mut parts = Vec::with_capacity(items.len());
                for item in items {
                    let toml::Value::String(part) = item else {
                        return Err(ConfigError::Invalid {
                            detail: format!("`agent.{key}` 的 argv 只能包含字符串"),
                        });
                    };
                    if part.is_empty() {
                        return Err(ConfigError::Invalid {
                            detail: format!("`agent.{key}` 的 argv 不得包含空串"),
                        });
                    }
                    parts.push(part.clone());
                }
                let command = parts.remove(0);
                (command, parts)
            }
            _ => {
                return Err(ConfigError::Invalid {
                    detail: format!("`agent.{key}` 必须是命令字符串或 argv 数组"),
                });
            }
        };
        profiles.push(
            AgentProfile::try_new(id, &command, args, false).map_err(|_| ConfigError::Invalid {
                detail: format!("`agent.{key}` 的命令或 argv 非法"),
            })?,
        );
    }
    match default {
        None => {}
        Some(name) => {
            let index = profiles
                .iter()
                .position(|profile| profile.id().as_str() == name)
                .ok_or_else(|| ConfigError::Invalid {
                    detail: format!("`agent.default` 指向未声明的 `agent.{name}`"),
                })?;
            profiles[index] = profiles[index].clone().with_default(true);
        }
    }
    Ok(profiles)
}

/// `node_link.access.*`（`CONFIG_REFERENCE.md` §3）：Access 侧出站连接的本地设置。
///
/// 只承载「信任哪些 CA」这一条部署事实；限额、重连策略与 feature 集合都是协议/实现常量，不在这里开口。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeLinkAccessConfig {
    /// 追加到系统根存储的固定 CA（PEM 文件，绝对路径）；空 = 只信任系统根。
    ///
    /// 文件存在性与 PEM 内容在 [`crate::trust::OutboundTrust::load`] 里校验：任一路径不可读或无证书
    /// 即拒绝启动（失败关闭）。
    pub trust_roots: Vec<PathBuf>,
}

impl NodeLinkAccessConfig {
    /// 解析 `[node_link.access]`：路径必须是绝对路径（相对路径会随工作目录漂移，不是部署事实）。
    fn from_raw(raw: Option<RawNodeLinkAccess>) -> Result<Self, ConfigError> {
        let Some(raw) = raw else {
            return Ok(Self::default());
        };
        let Some(paths) = raw.trust_roots else {
            return Ok(Self::default());
        };
        let mut roots = Vec::with_capacity(paths.len());
        for path in paths {
            if path.is_empty() {
                return Err(ConfigError::Invalid {
                    detail: "`node_link.access.trust_roots` 的条目不得为空".to_owned(),
                });
            }
            let path = PathBuf::from(&path);
            if !path.is_absolute() {
                return Err(ConfigError::Invalid {
                    detail:
                        "`node_link.access.trust_roots` 必须是绝对路径（文件的存在性与 PEM 格式在启动时校验）"
                            .to_owned(),
                });
            }
            roots.push(path);
        }
        Ok(Self { trust_roots: roots })
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
    /// `daemon.shutdown_grace_ms`（网络 listener 的排空宽限也取它，见 `NetConfig::drain_grace`）。
    pub shutdown_grace_ms: u64,
    /// `daemon.port`：共享 HTTP/WSS listener 的端口（默认 `8765`）。
    pub port: u16,
    /// `daemon.bind`：显式绑定地址；`None` = 按 TLS 模式取默认。
    pub bind: Option<IpAddr>,
    /// `daemon.allowed_hosts`：反向代理场景下的 `Host` 白名单；空表示按 `public_origin` 推导。
    pub allowed_hosts: Vec<String>,
    /// `daemon.trusted_proxies`：允许采信 `Forwarded`/`X-Forwarded-*` 的对端地址。
    pub trusted_proxies: Vec<String>,
    /// `daemon.tls.*`：`proxy`/`direct` 两种终止模式（`direct` 带两个 PEM 路径）。
    pub tls: TlsMode,
    /// `storage.flush_interval_ms`：broker 的 delta 合并窗口（`CORE_PORTS_AND_STORAGE.md` §6 第 10 条），
    /// 由组合根的定时器按下述间隔触发 `pump`。它不是存储刷盘开关（`storage-sqlite` 不参与，见
    /// `crates/storage-sqlite/src/migrate.rs` 的 `StorageConfig` 注记）。
    pub flush_interval_ms: u64,
    /// 存储参数（`storage.*` 的已接线部分）。
    pub storage: SqliteStorageConfig,
    /// `[node] name`：本节点展示名（可选）。
    pub node_name: Option<String>,
    /// `[agent]` 表解析出的 Agent 声明（唯一权威）。
    pub agents: Vec<AgentProfile>,
    /// `identity.keystore`。
    pub keystore: KeystoreChoice,
    /// `identity.fail_closed_on_missing_keystore`。
    pub fail_closed_on_missing_keystore: bool,
    /// `logging.*`。
    pub logging: LoggingConfig,
    /// `dev_mode.*`。
    pub dev_mode: DevMode,
    /// `sync.*` 三个可下调的 Sync 限额（`CONFIG_REFERENCE.md` §2；取值已按「只下调」收敛）。
    pub sync: SyncConfig,
    /// `sessions.*`（`CONFIG_REFERENCE.md` §4）。
    pub sessions: SessionsConfig,
    /// `node_link.access.*`（Access 侧出站设置；本机作为 Owner 入站时不使用）。
    pub node_link_access: NodeLinkAccessConfig,
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
        if daemon.listen.is_some() {
            return Err(ConfigError::Invalid {
                detail: "`daemon.listen` 已拆分：请改用 `daemon.port` 与可选的 `daemon.bind`（例如 port = 8765, bind = \"0.0.0.0\"）"
                    .to_owned(),
            });
        }
        let port = daemon.port.unwrap_or(DEFAULT_PORT);
        let bind = match daemon.bind.as_deref() {
            None => None,
            Some(text) => {
                Some(
                    text.trim()
                        .parse::<IpAddr>()
                        .map_err(|_| ConfigError::Invalid {
                            detail: "`daemon.bind` 必须是 IP 字面量（不支持主机名）".to_owned(),
                        })?,
                )
            }
        };
        let allowed_hosts = daemon.allowed_hosts.clone().unwrap_or_default();
        let trusted_proxies = daemon.trusted_proxies.clone().unwrap_or_default();
        let tls = resolve_tls(daemon.tls.as_ref())?;
        // `proxy` 模式下 Daemon 不说 TLS：非 loopback 绑定会把明文面暴露到本机之外。
        if matches!(&tls, TlsMode::Proxy) {
            if let Some(ip) = bind {
                if !ip.is_loopback() {
                    return Err(ConfigError::Invalid {
                        detail: format!(
                            "`proxy` 模式只允许 loopback 的 `daemon.bind`（收到 `{ip}`）；远程入口请用 `direct` TLS 或同机反代"
                        ),
                    });
                }
            }
        }
        let raw_public_origin = daemon.public_origin.clone();

        let storage_section = raw.storage.unwrap_or_default();
        let storage = storage_section.to_storage_config(&data_dir)?;
        let flush_interval_ms = storage_section
            .flush_interval_ms
            .unwrap_or(DEFAULT_FLUSH_INTERVAL_MS);
        // 0 会让合并窗口退化成热循环（每个调度点都枚举会话），不是合法取值；上界由运维自己定，不加。
        if flush_interval_ms == 0 {
            return Err(ConfigError::Invalid {
                detail: "`storage.flush_interval_ms` 必须是正整数（毫秒）".to_owned(),
            });
        }

        let mut unwired = Vec::new();
        // 单实例锁只有 OS advisory 文件锁一种实现（`fs4`）。`ipc` 曾作为取值被接受但不生效，
        // 那会让用户以为换了实现；现在直接失败关闭（与 `daemon.local_admin.endpoint` 同一策略）。
        if let Some(instance_lock) = daemon.instance_lock.as_deref() {
            if instance_lock != "file" {
                return Err(ConfigError::Invalid {
                    detail: format!(
                        "`daemon.instance_lock` 只能取 `file`（收到 `{instance_lock}`）"
                    ),
                });
            }
        }
        for key in raw
            .node_link
            .as_ref()
            .map_or_else(Vec::new, RawNodeLink::present_keys)
        {
            unwired.push(key.to_owned());
        }
        // `[node_link.access]` 是**已接线**的：只承载出站信任锚（见 `crate::trust`）。
        let node_link_access =
            NodeLinkAccessConfig::from_raw(raw.node_link.and_then(|section| section.access))?;
        let sessions = RawSessions::into_config(raw.sessions.unwrap_or_default())?;
        let node_name = raw.node.and_then(|node| node.name);
        if let Some(name) = &node_name {
            if name.is_empty() || name.chars().count() > 128 {
                return Err(ConfigError::Invalid {
                    detail: "`node.name` 必须是 1..=128 字符".to_owned(),
                });
            }
        }
        let agents = parse_agents(raw.agent)?;

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
        let logging = raw
            .logging
            .map_or_else(|| Ok(LoggingConfig::default()), RawLogging::into_config)?;

        let public_origin = normalize_public_origin(raw_public_origin.as_deref(), &dev_mode)?;
        // `port = 0` 只用于开发模式下的本机临时端口（测试与本地调试）。
        let effective_loopback = bind.map_or(matches!(&tls, TlsMode::Proxy), |ip| ip.is_loopback());
        if port == 0 && !(dev_mode.enabled && effective_loopback) {
            return Err(ConfigError::Invalid {
                detail:
                    "`daemon.port = 0` 只允许在 `dev_mode.enabled = true` 且绑定 loopback 时使用"
                        .to_owned(),
            });
        }
        // 非 loopback 且没有任何 Host 判定来源时，远程请求一律被接入层拒绝：这是坏配置。
        let non_loopback = bind.map_or(!matches!(&tls, TlsMode::Proxy), |ip| !ip.is_loopback());
        if non_loopback && public_origin.is_none() && allowed_hosts.is_empty() {
            return Err(ConfigError::Invalid {
                detail: "绑定非 loopback 时必须配置 `daemon.public_origin` 或 `daemon.allowed_hosts`（否则远程 Host 一律被拒）"
                    .to_owned(),
            });
        }

        // `sync.*` 三个键**已接线**（`CONFIG_REFERENCE.md` §2）：它们只允许把 §14 的默认值往**下**调，
        // 因此高于默认值的取值在这里就收敛成默认值——写得更大不会放宽对端可见的 `limits`，也不像
        // 「未接线」那样被静默忽略。取值随 `SyncConfig` 一路进 `server::sync::conn`（组合根
        // `daemon::sync_config` 是唯一映射点），协议层 `SessionLimits::negotiate` 再按
        // `[wire 下限, §14 默认值]` 夹一次（「任何路径都不得放宽」的构造期不变量）。
        let sync_section = raw.sync.unwrap_or_default();
        let sync = SyncConfig {
            max_message_bytes: lower_of(sync_section.max_message_bytes, DEFAULT_MAX_MESSAGE_BYTES),
            max_prompt_bytes: lower_of(sync_section.max_prompt_bytes, DEFAULT_MAX_PROMPT_BYTES),
            max_replay_events_per_batch: lower_of(
                sync_section.max_replay_events_per_batch,
                DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH,
            ),
        };

        Ok(Self {
            data_dir,
            public_origin,
            shutdown_grace_ms,
            port,
            bind,
            allowed_hosts,
            trusted_proxies,
            tls,
            flush_interval_ms,
            storage,
            node_name,
            agents,
            keystore,
            fail_closed_on_missing_keystore,
            logging,
            dev_mode,
            sync,
            sessions,
            node_link_access,
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
/// 取**错误正文**（首个不缩进、不是定位行也不是源码片段行的那一行）：定位行只有行列号，没有判据；
/// 源码片段（`2 | data_diry = "/tmp/x"`）与它的 `|` 边框会逐字回显用户写的配置文本，不进日志。
fn toml_error_detail(text: &str) -> String {
    let body = text
        .lines()
        .map(str::trim_end)
        .find(|line| {
            !line.is_empty()
                && !line.starts_with(char::is_whitespace)
                && !line.starts_with("TOML parse error")
                && !is_toml_locator_line(line)
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

/// toml 诊断里的定位/源码片段行：`  |`、`2 | data_diry = "/tmp/x"`、`  | ^^^^^^^^`。
///
/// 它们不属于错误正文：`2 | …` 这样的行逐字包含用户写的配置文本（`ConfigError::Invalid` 只承载
/// 不含配置值的简短说明）。
fn is_toml_locator_line(line: &str) -> bool {
    let Some((head, _)) = line.split_once('|') else {
        return false;
    };
    let head = head.trim();
    head.is_empty() || head.chars().all(|c| c.is_ascii_digit())
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
    node: Option<RawNode>,
    agent: Option<std::collections::BTreeMap<String, toml::Value>>,
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
    port: Option<u16>,
    bind: Option<String>,
    /// 旧键：只用于给出迁移错误，不参与解析。
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

/// `daemon.tls.*` → [`TlsMode`]（`CONFIG_REFERENCE.md` §1）。
///
/// 失败关闭：未知模式、`direct` 缺 PEM 路径、以及 `proxy` 模式下给了 PEM 路径（会被静默忽略的形态）
/// 都在解析期拒绝，不把矛盾配置带进启动序列。PEM 文件本身的可读性、权限与解析由 `NetListener::bind`
/// （`transport::net::tls`）判定——那条路径同样失败关闭，且不把私钥内容写进错误消息。
fn resolve_tls(raw: Option<&RawTls>) -> Result<TlsMode, ConfigError> {
    let mode = raw.and_then(|tls| tls.mode.as_deref()).unwrap_or("proxy");
    match mode {
        "proxy" => {
            if raw.is_some_and(|tls| tls.cert_path.is_some() || tls.key_path.is_some()) {
                return Err(ConfigError::Invalid {
                    detail: "`daemon.tls.cert_path`/`daemon.tls.key_path` 只在 `daemon.tls.mode = \"direct\"` 时有效"
                        .to_owned(),
                });
            }
            Ok(TlsMode::Proxy)
        }
        "direct" => {
            let cert_path = raw.and_then(|tls| tls.cert_path.as_ref());
            let key_path = raw.and_then(|tls| tls.key_path.as_ref());
            match (cert_path, key_path) {
                (Some(cert_path), Some(key_path)) => Ok(TlsMode::Direct {
                    cert_path: PathBuf::from(cert_path),
                    key_path: PathBuf::from(key_path),
                }),
                _ => Err(ConfigError::Invalid {
                    detail: "`daemon.tls.mode = \"direct\"` 需要 `daemon.tls.cert_path` 与 `daemon.tls.key_path`"
                        .to_owned(),
                }),
            }
        }
        other => Err(ConfigError::Invalid {
            detail: format!("`daemon.tls.mode` 只能是 `proxy` 或 `direct`（收到 `{other}`）"),
        }),
    }
}

/// 把 `daemon.public_origin` 归一化为 canonical origin（`CONFIG_REFERENCE.md` §1）。
///
/// 输入是 `host[:port]` 或完整 `http(s)://host[:port]`；裸值补 `https://`。`http://` 只允许在
/// 开发明文模式；其它 scheme、路径、查询、片段与用户信息都失败关闭。
fn normalize_public_origin(
    raw: Option<&str>,
    dev_mode: &DevMode,
) -> Result<Option<String>, ConfigError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(ConfigError::Invalid {
            detail: "`daemon.public_origin` 不能为空".to_owned(),
        });
    }
    let (scheme, authority) = if let Some(rest) = raw.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = raw.strip_prefix("http://") {
        if !(dev_mode.enabled && dev_mode.allow_plaintext) {
            return Err(ConfigError::Invalid {
                detail: "`daemon.public_origin` 必须是 https://（只有开发明文模式允许 http://）"
                    .to_owned(),
            });
        }
        ("http", rest)
    } else if raw.contains("://") {
        return Err(ConfigError::Invalid {
            detail: "`daemon.public_origin` 只支持 http(s):// 或 host[:port]".to_owned(),
        });
    } else {
        ("https", raw)
    };
    validate_origin_authority(authority)?;
    let normalized = format!("{scheme}://{authority}");
    if scheme == "https" {
        CanonicalOrigin::parse(&normalized).map_err(|_| ConfigError::Invalid {
            detail: "`daemon.public_origin` 不是合法的 canonical origin".to_owned(),
        })?;
    }
    Ok(Some(normalized))
}

/// 校验 origin 的 authority：允许域名、IPv4、方括号 IPv6，可带 `:port`。
fn validate_origin_authority(authority: &str) -> Result<(), ConfigError> {
    let invalid = || ConfigError::Invalid {
        detail: "`daemon.public_origin` 只能是 host[:port]（不含路径、查询、片段或用户信息）"
            .to_owned(),
    };
    if authority.is_empty()
        || authority.contains(['/', '?', '#', '@'])
        || authority
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(invalid());
    }
    let port = if let Some(rest) = authority.strip_prefix('[') {
        let (inner, tail) = rest.split_once(']').ok_or_else(invalid)?;
        inner.parse::<std::net::Ipv6Addr>().map_err(|_| invalid())?;
        tail.strip_prefix(':')
    } else {
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        if host.is_empty() {
            return Err(invalid());
        }
        port
    };
    if let Some(port) = port {
        if matches!(port.parse::<u16>(), Ok(0) | Err(_)) {
            return Err(invalid());
        }
    }
    Ok(())
}

/// `[sync]` 段：三个**可下调**的 Sync 限额（`CONFIG_REFERENCE.md` §2）。
///
/// 三个键都已接线，因此这里没有 `present_keys`：不再有任何一个是「已知但未接线」。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSync {
    max_message_bytes: Option<u64>,
    max_prompt_bytes: Option<u64>,
    max_replay_events_per_batch: Option<u64>,
}

/// 「只下调」：未配置取协议默认值，配置高于默认值时**也**取默认值（`SYNC_PROTOCOL.md` §14 的默认值
/// 同时是上界；`CONFIG_REFERENCE.md` §2 的取值口径）。
///
/// 低于协议默认值是合法的（收紧本机限额），再往下的 wire 下限由协议层
/// `SessionLimits::negotiate` 兜住——它把值夹在 `[wire 下限, 默认值]` 内，因此无论配置写多小都不会
/// 得到对端无法接受的 `limits`。
fn lower_of(configured: Option<u64>, protocol_default: u64) -> u64 {
    configured.unwrap_or(protocol_default).min(protocol_default)
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
    access: Option<RawNodeLinkAccess>,
}

/// `[node_link.access]`（`CONFIG_REFERENCE.md` §3）。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNodeLinkAccess {
    trust_roots: Option<Vec<String>>,
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
    max_active_sessions: Option<u64>,
}

impl RawSessions {
    /// 三个键都已接线（`CONFIG_REFERENCE.md` §4）：非法取值在解析期失败关闭，不把矛盾配置带进启动。
    fn into_config(self) -> Result<SessionsConfig, ConfigError> {
        let queue_policy = match self.queue_policy.as_deref() {
            None | Some("queue") => QueuePolicy::Queue,
            Some("reject_busy") => QueuePolicy::RejectBusy,
            Some(other) => {
                return Err(ConfigError::Invalid {
                    detail: format!(
                        "`sessions.queue_policy` 只能是 `queue` 或 `reject_busy`（收到 `{other}`）"
                    ),
                });
            }
        };
        let max_queued_turns = match self.max_queued_turns {
            None => 16,
            Some(value) => u32::try_from(value).map_err(|_| ConfigError::Invalid {
                detail: "`sessions.max_queued_turns` 必须是 0..=4294967295 的整数".to_owned(),
            })?,
        };
        // `0` 与未配置同义（旧口径）；该字段不再驱动进程级 idle sweep，只保留配置解析形状。
        // 生命周期（下方 `lifecycle`）仍读取原始 `idle_timeout_ms`：显式 0 只关闭按时间回收。
        let idle_timeout = self
            .idle_timeout_ms
            .filter(|value| *value > 0)
            .map(Duration::from_millis);
        // 生命周期：缺省 30 分钟保留期；显式 `0` 只关闭按时间回收（容量保护仍生效）。
        // `max_active_sessions` 必须是正整数——`0`/超范围在解析期失败关闭，不静默降级。
        let lifecycle_idle = self.idle_timeout_ms.unwrap_or(1_800_000);
        let max_active = match self.max_active_sessions {
            None => 50,
            Some(value) => u32::try_from(value)
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| ConfigError::Invalid {
                    detail: "`sessions.max_active_sessions` 必须是 1..=4294967295 的正整数"
                        .to_owned(),
                })?,
        };
        let lifecycle = acp_core::use_cases::LifecycleConfig::new(lifecycle_idle, max_active)
            .map_err(|_| ConfigError::Invalid {
                detail: "`sessions.max_active_sessions` 必须是正整数".to_owned(),
            })?;
        Ok(SessionsConfig {
            queue_policy,
            max_queued_turns,
            idle_timeout,
            lifecycle,
        })
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
struct RawNode {
    name: Option<String>,
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

/// CLI 的配置加载入口：CLI 只调用组合根提供的加载器，不自行解析 TOML 或复制校验规则。
pub(crate) fn load_for_cli(path: Option<&std::path::Path>) -> Result<Loaded, ConfigError> {
    Config::load(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    // 协议层的协商结果（对端可见的 `limits`）是「真正被消费」的判据，因此用例直接经它断言。
    use server::sync::conn::SessionLimits;

    fn parse(text: &str) -> Result<Config, ConfigError> {
        Config::from_toml(text)
    }

    #[test]
    fn defaults_match_the_reference_table() {
        let config = parse("").expect("空配置必须可解析");
        assert_eq!(config.shutdown_grace_ms, 10_000);
        assert_eq!(config.flush_interval_ms, 250);
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
        // 网络接入面的键：默认值来自 `CONFIG_REFERENCE.md` §1（与 `transport::net` 的内置默认值同源，
        // 不在这里另写一份字面量）。
        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(config.bind, None);
        assert!(config.allowed_hosts.is_empty());
        assert!(config.trusted_proxies.is_empty());
        assert!(matches!(config.tls, TlsMode::Proxy));
        assert_eq!(config.keystore, KeystoreChoice::Platform);
        assert!(config.fail_closed_on_missing_keystore);
        assert_eq!(config.logging, LoggingConfig::default());
        assert_eq!(config.dev_mode, DevMode::default());
        assert!(config.agents.is_empty());
        assert!(config.node_name.is_none());
        // `sync.*` 三个键已接线：默认值来自 `CONFIG_REFERENCE.md` §2（与协议层的 §14 默认值同源，不在这
        // 里另写一份字面量），因此空配置下它们既生效、也不进「已知但未接线」清单。
        assert_eq!(config.sync, SyncConfig::default());
        assert_eq!(config.sync.max_message_bytes, DEFAULT_MAX_MESSAGE_BYTES);
        assert_eq!(config.sync.max_prompt_bytes, DEFAULT_MAX_PROMPT_BYTES);
        assert_eq!(
            config.sync.max_replay_events_per_batch,
            DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH
        );
        assert_eq!(config.sessions.queue_policy, QueuePolicy::Queue);
        assert_eq!(config.sessions.max_queued_turns, 16);
        assert!(
            config.sessions.idle_timeout.is_none(),
            "agent-host 的既有空闲回收在未配置时仍关闭"
        );
        assert_eq!(
            config.sessions.lifecycle,
            SessionsConfig::default().lifecycle
        );
        assert_eq!(
            config.sessions.lifecycle.idle_timeout_ms(),
            1_800_000,
            "保留期默认 30 分钟"
        );
        assert_eq!(
            config.sessions.lifecycle.max_active_sessions(),
            50,
            "容量默认候选值 50"
        );
        assert!(config.sessions.lifecycle.time_based_reclaim_enabled());
        assert!(config.unwired.is_empty(), "{:?}", config.unwired);
    }

    /// 显式 `0` 只关闭**按时间**回收：容量上限仍生效（两者不是同一个开关）。
    #[test]
    fn an_explicit_zero_idle_timeout_only_disables_time_based_reclaim() {
        let config = parse(
            r#"
[sessions]
idle_timeout_ms = 0
"#,
        )
        .expect("显式 0 是合法配置");
        assert_eq!(config.sessions.lifecycle.idle_timeout_ms(), 0);
        assert!(
            !config.sessions.lifecycle.time_based_reclaim_enabled(),
            "显式 0 关闭按时间回收"
        );
        assert_eq!(
            config.sessions.lifecycle.max_active_sessions(),
            50,
            "显式 0 不得连带关闭容量策略"
        );
        assert!(
            config.sessions.idle_timeout.is_none(),
            "agent-host 的既有空闲回收同样把 0 当作关闭"
        );
    }

    /// `max_active_sessions` 没有「无限」取值：0、负数与非整数都在解析期失败关闭。
    #[test]
    fn max_active_sessions_rejects_zero_negative_and_non_integer_values() {
        for value in ["0", "-1", "\"50\""] {
            let error = parse(&format!(
                r#"
[sessions]
max_active_sessions = {value}
"#
            ))
            .expect_err(&format!("max_active_sessions = {value} 必须被拒绝"));
            assert!(
                matches!(error, ConfigError::Invalid { .. }),
                "max_active_sessions = {value} 必须是配置错误：{error:?}"
            );
        }
        let accepted = parse(
            r#"
[sessions]
max_active_sessions = 7
"#,
        )
        .expect("正整数是合法容量");
        assert_eq!(accepted.sessions.lifecycle.max_active_sessions(), 7);
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
flush_interval_ms = 500
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

[node]
name = "work-pc"

[sessions]
queue_policy = "reject_busy"
max_queued_turns = 4
idle_timeout_ms = 1500

[agent]
default = "codex"
codex = ["codex-acp", "--acp"]
"#
        ))
        .expect("合法配置必须可解析");
        assert_eq!(config.data_dir, PathBuf::from(format!("{root}/data")));
        assert_eq!(
            config.public_origin.as_deref(),
            Some("https://work-pc.example.ts.net")
        );
        assert_eq!(config.shutdown_grace_ms, 1500);
        assert_eq!(config.flush_interval_ms, 500);
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
        assert_eq!(config.node_name.as_deref(), Some("work-pc"));
        assert_eq!(config.sessions.queue_policy, QueuePolicy::RejectBusy);
        assert_eq!(config.sessions.max_queued_turns, 4);
        assert_eq!(
            config.sessions.idle_timeout,
            Some(Duration::from_millis(1500))
        );
        assert_eq!(config.agents.len(), 1);
        let agent = &config.agents[0];
        assert_eq!(agent.id().as_str(), "codex");
        assert_eq!(agent.display_name(), "codex");
        assert_eq!(agent.command(), "codex-acp");
        assert_eq!(agent.args(), ["--acp"]);
        assert!(agent.is_default());
    }

    #[test]
    fn only_node_link_keys_remain_unwired() {
        let config = parse(
            r#"
[node_link]
heartbeat_interval_ms = 30000

[sessions]
idle_timeout_ms = 1000
"#,
        )
        .expect("未接线段落必须被接受");
        assert_eq!(config.unwired, vec!["node_link.heartbeat_interval_ms"]);
        // `sessions.*` 已接线：写下的值必须真的进入配置，而不是「被接受但不生效」。
        assert_eq!(
            config.sessions.idle_timeout,
            Some(Duration::from_millis(1000))
        );
        // 本切片接线后的键不得再出现在「已知但未接线」清单里（否则启动日志会谎报）。
        for key in [
            "daemon.port",
            "daemon.bind",
            "daemon.allowed_hosts",
            "daemon.trusted_proxies",
            "daemon.tls.mode",
            "daemon.tls.cert_path",
            "daemon.tls.key_path",
            "dev_mode.allow_plaintext",
            "sync.max_message_bytes",
            "sync.max_prompt_bytes",
            "sync.max_replay_events_per_batch",
            "sessions.queue_policy",
            "sessions.max_queued_turns",
            "sessions.idle_timeout_ms",
        ] {
            assert!(
                !config.unwired.iter().any(|key_name| key_name == key),
                "`{key}` 已接线，不得再报未接线",
            );
        }
    }

    /// `[node_link.access]` 是**已接线**的（出站信任锚），且只接受绝对路径：相对路径会随工作目录
    /// 漂移，不是部署事实。文件存在性与 PEM 内容在启动时由 `app::trust` 校验。
    #[test]
    fn access_trust_roots_are_wired_and_must_be_absolute() {
        let config = parse(
            r#"
[node_link]
heartbeat_interval_ms = 30000

[node_link.access]
trust_roots = []
"#,
        )
        .expect("空数组合法");
        assert!(config.node_link_access.trust_roots.is_empty());
        assert_eq!(config.unwired, vec!["node_link.heartbeat_interval_ms"]);

        let absolute = std::env::temp_dir().join("acpr-ca.pem");
        let absolute = absolute.display().to_string().replace('\\', "/");
        let config = parse(&format!(
            "[node_link.access]\ntrust_roots = [\"{absolute}\"]\n"
        ))
        .expect("绝对路径合法");
        assert_eq!(
            config.node_link_access.trust_roots,
            vec![PathBuf::from(&absolute)]
        );
        assert!(config.unwired.is_empty());

        let error = parse("[node_link.access]\ntrust_roots = [\"ca.pem\"]\n")
            .expect_err("相对路径必须失败关闭");
        assert!(matches!(error, ConfigError::Invalid { .. }), "{error}");

        let error =
            parse("[node_link.access]\ntrust_roots = [\"\"]\n").expect_err("空条目必须失败关闭");
        assert!(matches!(error, ConfigError::Invalid { .. }), "{error}");
    }

    /// `sync.*` 三个键**已接线**（`CONFIG_REFERENCE.md` §2）：写下的值一路进 `SyncConfig` →
    /// `SessionLimits`（对端可见的 `auth.authenticated.limits`），且方向只允许**下调**。
    ///
    /// 「上调被忽略」与「下调生效」都必须真的走到协议层协商结果上——只断言 `config.sync` 会漏掉
    /// 「解析到但没接进协议层」这一整类回归。
    #[test]
    fn the_sync_limits_are_wired_and_can_only_be_lowered() {
        let lowered = parse(
            r#"
[sync]
max_message_bytes = 65536
max_prompt_bytes = 1024
max_replay_events_per_batch = 1
"#,
        )
        .expect("三个键必须被接受并生效");
        assert_eq!(lowered.sync.max_message_bytes, 65_536);
        assert_eq!(lowered.sync.max_prompt_bytes, 1_024);
        assert_eq!(lowered.sync.max_replay_events_per_batch, 1);
        assert!(lowered.unwired.is_empty(), "{:?}", lowered.unwired);
        let limits = SessionLimits::negotiate(&crate::daemon::sync_config(&lowered));
        assert_eq!(limits.max_message_bytes(), 65_536);
        assert_eq!(limits.max_prompt_bytes(), 1_024);
        assert_eq!(limits.max_replay_events_per_batch(), 1);

        let raised = parse(
            r#"
[sync]
max_message_bytes = 8388608
max_prompt_bytes = 4194304
max_replay_events_per_batch = 5000
"#,
        )
        .expect("高于 §14 默认值的配置必须被接受（只是不生效）");
        assert_eq!(
            raised.sync,
            SyncConfig::default(),
            "高于协议默认值的取值必须收敛为默认值"
        );
        let limits = SessionLimits::negotiate(&crate::daemon::sync_config(&raised));
        assert_eq!(limits.max_message_bytes(), DEFAULT_MAX_MESSAGE_BYTES);
        assert_eq!(limits.max_prompt_bytes(), DEFAULT_MAX_PROMPT_BYTES);
        assert_eq!(
            limits.max_replay_events_per_batch(),
            u32::try_from(DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH).expect("默认值在 u32 内")
        );

        // 未配置 = 同默认值（不是「关掉」），且与省略 `[sync]` 整段等价。
        assert_eq!(parse("").expect("空配置").sync, SyncConfig::default());
        assert_eq!(
            parse("[sync]\n").expect("空段落").sync,
            SyncConfig::default()
        );
    }

    /// 网络接入面的四个键 + `dev_mode.allow_plaintext` 从 TOML 取到值（接线后的解析面）。
    #[test]
    fn the_listener_keys_are_wired_into_the_config() {
        let config = parse(
            r#"
[daemon]
port = 8765
bind = "0.0.0.0"
allowed_hosts = ["example.ts.net", "proxy.internal"]
trusted_proxies = ["127.0.0.1:443"]

[daemon.tls]
mode = "direct"
cert_path = "/tmp/owner-cert.pem"
key_path = "/tmp/owner-key.pem"

[dev_mode]
enabled = true
allow_plaintext = true
"#,
        )
        .expect("四个键必须被接受并接线");
        assert_eq!(config.port, 8765);
        assert_eq!(config.bind, Some("0.0.0.0".parse().expect("ip")));
        assert_eq!(
            config.allowed_hosts,
            vec!["example.ts.net", "proxy.internal"]
        );
        assert_eq!(config.trusted_proxies, vec!["127.0.0.1:443"]);
        assert!(config.dev_mode.allow_plaintext);
        match &config.tls {
            TlsMode::Direct {
                cert_path,
                key_path,
            } => {
                assert_eq!(cert_path, &PathBuf::from("/tmp/owner-cert.pem"));
                assert_eq!(key_path, &PathBuf::from("/tmp/owner-key.pem"));
            }
            other => panic!("`mode = \"direct\"` 必须解析为 direct，实际 {other:?}"),
        }
    }

    /// 仓库根的 `config.example.toml` 必须能被真实配置解析器接受，且取值等于内置默认值。
    ///
    /// 这条用例把“模板漂移”变成编译/测试失败：新增键、改默认值或写错 TOML 都会在这里暴露。
    #[test]
    fn the_example_template_parses_and_matches_defaults() {
        let config = parse(include_str!("../../../config.example.toml"))
            .expect("config.example.toml 必须可解析");
        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(config.bind, None);
        assert_eq!(config.public_origin, None);
        assert!(config.allowed_hosts.is_empty());
        assert!(config.trusted_proxies.is_empty());
        assert_eq!(
            config.sessions.queue_policy,
            SessionsConfig::default().queue_policy
        );
        assert_eq!(
            config.sessions.max_queued_turns,
            SessionsConfig::default().max_queued_turns
        );
        assert_eq!(
            config.sessions.lifecycle,
            SessionsConfig::default().lifecycle
        );
        assert_eq!(config.sync, SyncConfig::default());
        assert_eq!(config.storage.transcript_retention_days, 90);
        assert_eq!(config.storage.sync_event_retention_days, 7);
        assert_eq!(config.storage.audit_retention_days, 365);
        assert_eq!(config.keystore, KeystoreChoice::Platform);
        assert!(config.fail_closed_on_missing_keystore);
        assert!(!config.dev_mode.enabled);
        assert_eq!(config.logging, LoggingConfig::default());
        assert!(config.agents.is_empty());
        assert!(config.node_name.is_none());
        assert!(
            config.unwired.is_empty(),
            "模板里被注释的 node_link 不应进入未接线清单：{:?}",
            config.unwired
        );
    }

    /// `daemon.public_origin` 只写 host[:port] 或完整 URL，内部归一化为 canonical origin。
    #[test]
    fn public_origin_accepts_host_or_full_url() {
        for (input, expected) in [
            ("work-pc.tail.ts.net", "https://work-pc.tail.ts.net"),
            (
                "work-pc.tail.ts.net:8443",
                "https://work-pc.tail.ts.net:8443",
            ),
            ("https://work-pc.tail.ts.net", "https://work-pc.tail.ts.net"),
            ("[::1]:8765", "https://[::1]:8765"),
            ("100.101.102.103", "https://100.101.102.103"),
        ] {
            let config = parse(&format!("[daemon]\npublic_origin = \"{input}\"\n"))
                .expect("合法 origin 必须被接受");
            assert_eq!(config.public_origin.as_deref(), Some(expected));
        }
        for invalid in [
            "work-pc.tail.ts.net/path",
            "https://host/path",
            "host:0",
            "ftp://host",
            "https://user@host",
            "https://:443",
            "host:",
        ] {
            assert!(
                parse(&format!("[daemon]\npublic_origin = \"{invalid}\"\n")).is_err(),
                "必须拒绝：{invalid}"
            );
        }
    }

    #[test]
    fn http_public_origin_requires_dev_plaintext() {
        assert!(
            parse("[daemon]\npublic_origin = \"http://host\"\n").is_err(),
            "非开发模式不得使用 http origin"
        );
        let config = parse(
            "[daemon]\npublic_origin = \"http://host\"\n[dev_mode]\nenabled = true\nallow_plaintext = true\n",
        )
        .expect("开发明文模式允许 http origin");
        assert_eq!(config.public_origin.as_deref(), Some("http://host"));
    }

    #[test]
    fn listen_is_rejected_with_migration_hint() {
        let error = parse("[daemon]\nlisten = \"127.0.0.1:8765\"\n").expect_err("旧键必须被拒绝");
        let message = error.to_string();
        assert!(message.contains("daemon.listen"), "{message}");
        assert!(message.contains("daemon.port"), "{message}");
    }

    #[test]
    fn proxy_mode_rejects_non_loopback_bind() {
        assert!(
            parse("[daemon]\nbind = \"0.0.0.0\"\n").is_err(),
            "proxy 模式不得绑定非 loopback"
        );
        assert!(parse("[daemon]\nbind = \"127.0.0.1\"\n").is_ok());
        assert!(parse("[daemon]\nbind = \"::1\"\n").is_ok());
    }

    #[test]
    fn zero_port_requires_dev_mode_and_loopback() {
        assert!(parse("[daemon]\nport = 0\n").is_err());
        assert!(
            parse("[daemon]\nport = 0\nbind = \"0.0.0.0\"\n[dev_mode]\nenabled = true\n").is_err(),
            "非 loopback 即使开发模式也不得用临时端口"
        );
        assert!(
            parse("[daemon]\nport = 0\nbind = \"127.0.0.1\"\n[dev_mode]\nenabled = true\n").is_ok()
        );
    }

    #[test]
    fn non_loopback_bind_requires_a_host_source() {
        let direct = "[daemon.tls]\nmode = \"direct\"\ncert_path = \"/tmp/c.pem\"\nkey_path = \"/tmp/k.pem\"\n";
        let error = parse(&format!("[daemon]\nbind = \"0.0.0.0\"\n{direct}"))
            .expect_err("非 loopback 必须有 Host 判定来源");
        assert!(
            error.to_string().contains("daemon.public_origin"),
            "{error}"
        );
        for accepted in [
            format!("[daemon]\nbind = \"0.0.0.0\"\npublic_origin = \"host.ts.net\"\n{direct}"),
            format!("[daemon]\nbind = \"0.0.0.0\"\nallowed_hosts = [\"host.ts.net\"]\n{direct}"),
        ] {
            assert!(parse(&accepted).is_ok(), "必须接受：{accepted}");
        }
        // `direct` 的默认绑定就是非 loopback：同样必须给出 Host 来源。
        assert!(parse(direct).is_err());
    }

    /// `daemon.tls.*` 的矛盾/不完整形态在解析期失败关闭（不把矛盾配置带进启动序列）。
    #[test]
    fn contradictory_tls_configuration_is_rejected() {
        for (text, expect) in [
            (
                "[daemon.tls]\nmode = \"direct\"\ncert_path = \"/tmp/c.pem\"\n",
                "daemon.tls.key_path",
            ),
            ("[daemon.tls]\nmode = \"direct\"\n", "daemon.tls.cert_path"),
            (
                "[daemon.tls]\nmode = \"proxy\"\ncert_path = \"/tmp/c.pem\"\nkey_path = \"/tmp/k.pem\"\n",
                "daemon.tls.mode",
            ),
            ("[daemon.tls]\nmode = \"tunnel\"\n", "daemon.tls.mode"),
        ] {
            let error = parse(text).expect_err("必须失败关闭");
            assert!(
                error.to_string().contains(expect),
                "期望提及 `{expect}`，实际：{error}"
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

    /// `toml_error_detail` 的诊断要能直接进日志与 CLI 的 stdout「一行简述」：单行、不吐配置源码
    /// 片段（源码会回显用户写入的值）、不超过实现声明的长度上限。
    ///
    /// 反例能力：把该函数改回「原样多行诊断」（定位行 + `|` 源码片段 + 正文）时，前两条断言必然
    /// 失败——诊断会含 `\n`、也会包含源码片段里的 `endpoing = `；长度上限由 [`ERROR_DETAIL_MAX_CHARS`] 固定。
    #[test]
    fn toml_parse_error_detail_is_single_line_without_the_source_snippet() {
        let text = "[daemon.local_admin]\nendpoing = \"wp427-marker\"\n";
        let detail = match parse(text) {
            Err(ConfigError::Invalid { detail }) => detail,
            Err(other) => panic!("TOML schema 错误必须是 ConfigError::Invalid：{other}"),
            Ok(_) => panic!("未知键必须被拒绝"),
        };
        assert!(!detail.contains('\n'), "诊断必须是单行：{detail:?}");
        assert!(
            !detail.contains("endpoing = ") && !detail.contains("wp427-marker"),
            "诊断不得回显配置源码片段：{detail}"
        );
        assert!(
            detail.contains("endpoing"),
            "诊断必须点明出错的键（否则等于吞掉判据）：{detail}"
        );
        assert!(
            detail.chars().count() <= ERROR_DETAIL_MAX_CHARS,
            "诊断长度 {} 超过上限 {ERROR_DETAIL_MAX_CHARS}：{detail}",
            detail.chars().count()
        );
    }

    #[test]
    fn invalid_values_fail_closed() {
        for text in [
            "[daemon]\ndata_dir = \"relative/path\"\n",
            "[daemon]\nshutdown_grace_ms = 60001\n",
            "[daemon]\ninstance_lock = \"mutex\"\n",
            "[daemon]\ninstance_lock = \"ipc\"\n",
            "[daemon.local_admin]\nendpoint = \"\\\\\\\\.\\\\pipe\\\\acpr\"\n",
            "[terminal]\nkeep_head_bytes = 1024\n",
            "[sessions]\nqueue_policy = \"drop\"\n",
            "[sessions]\nmax_queued_turns = 4294967296\n",
            "[storage]\nattachment_dir = \"relative\"\n",
            "[storage]\nflush_interval_ms = 0\n",
            "[identity]\nkeystore = \"dpapi\"\n",
            "[logging]\nlevel = \"verbose\"\n",
            "[logging]\nformat = \"xml\"\n",
            "[agent]\na = \"\"\n",
            "[agent]\ndefault = \"ghost\"\na = \"cmd\"\n",
            "[agent]\na = [\"cmd\", \"\"]\n",
            "[agent]\na = [\"cmd\", 1]\n",
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
