//! `acp-remote` 的命令行入口（`docs/LOCAL_ADMIN_PROTOCOL.md` §5.8 的映射表、`specs/cli-commands`）。
//!
//! 三条硬约束：
//!
//! 1. **子命令 ↔ 方法的映射只认 §5.8**：本模块是 kebab-case 子命令/flags → camelCase `params` 的唯一翻译
//!    点，不新增方法。`doctor` 不是方法（组合 `daemon.status` 与 CLI 侧本地检查），`acp-stdio` 走通道
//!    channel `0x02`（见 [`crate::stdio`]），不进管理信封。
//! 2. **CLI 零业务规则、不打开 SQLite**：只做参数解析、展示与轮询。判定「Daemon 是否运行」只读
//!    `<dataDir>/daemon.lock`（尝试加锁）与 `<dataDir>/daemon.instance.json`（[`crate::lock::probe`]），
//!    因此在 Daemon 未运行时，`daemon status`/`daemon stop`/`doctor` **不连接、不打开数据库**，其余管理
//!    子命令明确失败（§7）。
//! 3. **统一错误出口**（[`fail`]）：成功退出码 `0`，失败非零；失败时 stdout 一行人类可读说明、stderr
//!    **恰好一行**含 `code` 的结构化 JSON（`code` 取自 §6 的本地错误码表或 Daemon 原样返回的码，不解析
//!    `error.message`）。消息不含 secret、不含完整敏感路径。
//!
//! 全部子命令的入口都在这里，产物只有 stdout 的人类可读结果与 stderr 的错误行：`daemon start` 之外的任何
//! 子命令都不持有 `SqliteStore`，也不构造数据库连接。

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use server::local_admin::{JsonObject, LocalErrorCode, Method};

use crate::client::{ClientError, ClientOutcome, LocalAdminClient, outcome_of};
use crate::config::{Config, ConfigError, Loaded, config_path_from_env};
use crate::lock::{self, LockState};

pub(crate) mod input;
pub(crate) mod pairing;

/// `daemon stop` 之后等待 Daemon 释放单实例锁的上限（关闭序列含排空与 `wal_checkpoint`）。
const STOP_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

/// 等待锁释放的轮询间隔。
const STOP_LOCK_POLL: Duration = Duration::from_millis(50);

/// ACP Remote 的本地 Daemon 与管理 CLI。
#[derive(Debug, Parser)]
#[command(
    name = "acp-remote",
    version,
    about = "ACP Remote 的本地 Daemon 与管理 CLI",
    disable_help_subcommand = true
)]
struct Cli {
    /// 配置文件路径（默认取 `ACP_REMOTE_CONFIG`，再退到平台默认位置）。
    #[arg(long, value_name = "PATH", global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

/// §5.8 映射表第一列的全部子命令（顶层分组）。
#[derive(Debug, Subcommand)]
enum Command {
    /// Daemon 生命周期（`daemon start` 是前台进程）。
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// 本地工作区符号名（`workspace.select`）。
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Agent profile（`agent.configure`）。
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Provider 凭据（`provider.configure`）。
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// 设备配对与信任（`device.pair.*`、`device.list`、`device.revoke`）。
    Device {
        #[command(subcommand)]
        command: DeviceCommand,
    },
    /// 节点配对与信任（`node.pair.*`、`node.list`、`node.revoke`）。
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    /// Export 管理（`export.create|list|revoke`）。
    Export {
        #[command(subcommand)]
        command: ExportCommand,
    },
    /// Import 管理（`import.add|list|remove`）。
    Import {
        #[command(subcommand)]
        command: ImportCommand,
    },
    /// 组合 `daemon.status` 与 CLI 侧本地检查；Daemon 离线时在 CLI 进程内完成（**不新增方法**）。
    Doctor,
    /// stdin/stdout ↔ 本地通道 channel `0x02` 的字节泵（不进管理信封）。
    AcpStdio,
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// 前台启动 Daemon（取得单实例锁、创建本地管理 endpoint 并开始接受连接）。
    Start,
    /// 停止运行中的 Daemon，并等待它释放单实例锁。
    Stop(DaemonStop),
    /// 查询状态；Daemon 未运行时在 CLI 进程内回答（不连接、不打开数据库）。
    Status,
}

#[derive(Debug, Args)]
struct DaemonStop {
    /// `daemon.stop` 的 `graceMs`（0–60000）；缺省为 Daemon 的 `daemon.shutdown_grace_ms`。
    #[arg(long, value_name = "MS")]
    grace_ms: Option<u64>,
}

#[derive(Debug, Subcommand)]
enum WorkspaceCommand {
    /// 建立或更新工作区别名（`workspace.select`）。
    Select(WorkspaceSelect),
}

/// `workspace select`：§5.8 明写以 `--alias`/`--display-name`/`--root-path` 提供 §5.2 的三个参数。
#[derive(Debug, Args)]
struct WorkspaceSelect {
    /// 发布给 Export 的符号名（`^[a-z0-9][a-z0-9._-]{0,63}$`）。
    #[arg(long, value_name = "ALIAS")]
    alias: String,
    /// 展示名（≤128 字符）。
    #[arg(long, value_name = "NAME")]
    display_name: String,
    /// 绝对路径；必须已存在且是目录（路径只发送到本机 Daemon）。
    #[arg(long, value_name = "PATH")]
    root_path: PathBuf,
}

#[derive(Debug, Subcommand)]
enum AgentCommand {
    /// 建立或更新 Agent profile（`agent.configure`）。
    Configure(AgentConfigure),
}

/// `agent configure --file`：文件与 §5.2 的 `params` 同形。
#[derive(Debug, Args)]
struct AgentConfigure {
    /// 与 `agent.configure` 的 `params` 同形的 JSON 对象（不得含凭据值）。
    #[arg(long, value_name = "PATH")]
    file: PathBuf,
}

#[derive(Debug, Subcommand)]
enum ProviderCommand {
    /// 写入 Provider 凭据（`provider.configure`）：非秘密字段走 flags，凭据值在交互终端逐项无回显录入。
    Configure(ProviderConfigure),
}

/// `provider configure`：**没有**承载凭据值的 flag（凭据值不得作为命令行参数或文件输入）。
#[derive(Debug, Args)]
struct ProviderConfigure {
    /// Provider 标识（`^[A-Za-z0-9._-]{1,64}$`）。
    #[arg(long, value_name = "ID")]
    provider_id: String,
    /// 类别（`provider` | `mcp`，由方法侧校验）。
    #[arg(long, value_name = "KIND")]
    kind: String,
    /// 展示名（≤128 字符）。
    #[arg(long, value_name = "NAME")]
    display_name: String,
    /// 凭据字段名（可重复；值由 CLI 无回显读取）。
    #[arg(long = "field", value_name = "NAME", required = true)]
    fields: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum DeviceCommand {
    /// 设备配对三步骤：`device.pair.begin` → `device.pair.status`（轮询）→ `device.pair.confirm`。
    Pair(DevicePair),
    /// 列出设备（含 `pending` 与 `revoked`）。
    List,
    /// 撤销设备（`device.revoke`）。
    Revoke(DeviceRevoke),
}

/// `device pair`：`--request` 是逗号分隔的 scope 列表（§5.8 的示例形态）。
#[derive(Debug, Args)]
struct DevicePair {
    /// 请求的 scope（逗号分隔，可重复；§5.3 的 `requestedScopes`）。
    #[arg(long, value_delimiter = ',', value_name = "SCOPE")]
    request: Vec<String>,
    /// 请求的 pack（可重复；§5.3 的 `requestedPacks`）。
    #[arg(long = "pack", value_name = "PACK")]
    pack: Vec<String>,
    /// 非交互场景必须与 `--fingerprint` 同时给出：6 位 SAS（必须与设备屏幕逐字一致）。
    #[arg(long, value_name = "DIGITS")]
    sas: Option<String>,
    /// 非交互场景必须与 `--sas` 同时给出：64 位小写 hex 指纹（逐字匹配）。
    #[arg(long, value_name = "HEX")]
    fingerprint: Option<String>,
    /// 收窄配对有效期（毫秒，`1..=300000`）；缺省用满 5 分钟窗口。
    #[arg(long, value_name = "MS")]
    expires_in_ms: Option<u64>,
}

#[derive(Debug, Args)]
struct DeviceRevoke {
    /// 目标设备 id（UUID）。
    #[arg(long, value_name = "UUID")]
    device_id: String,
}

#[derive(Debug, Subcommand)]
enum NodeCommand {
    /// 节点配对：`node.pair.begin` → `node.pair.status`（轮询）→ `node.pair.confirm`。
    Pair(NodePair),
    /// 列出节点。
    List,
    /// 撤销节点（`node.revoke`）。
    Revoke(NodeRevoke),
}

/// `node pair`：`--mode owner` 在本机生成二维码并本机确认；`--mode access` 本切片回 `local.unsupported`。
#[derive(Debug, Args)]
struct NodePair {
    /// 配对方向（`owner` | `access`，由方法侧校验）。
    #[arg(long, value_name = "MODE")]
    mode: Option<String>,
    /// `--mode access` 时用户提供的 Owner 配对 URL（`--mode owner` 必须省略）。
    #[arg(long, value_name = "URL")]
    pairing_url: Option<String>,
    /// 本节点在对端界面上的展示名（≤128 字符）。
    #[arg(long, value_name = "NAME")]
    display_name: Option<String>,
    /// 本机允许授予的 grant 上限（逗号分隔，可重复）。
    #[arg(long = "grant", value_delimiter = ',', value_name = "GRANT")]
    grant: Vec<String>,
    /// 非交互场景必须与 `--fingerprint` 同时给出：6 位 SAS（逐字匹配）。
    #[arg(long, value_name = "DIGITS")]
    sas: Option<String>,
    /// 非交互场景必须与 `--sas` 同时给出：64 位小写 hex 指纹（逐字匹配）。
    #[arg(long, value_name = "HEX")]
    fingerprint: Option<String>,
    /// 收窄配对有效期（毫秒，`1..=300000`）；缺省用满 5 分钟窗口。
    #[arg(long, value_name = "MS")]
    expires_in_ms: Option<u64>,
}

#[derive(Debug, Args)]
struct NodeRevoke {
    /// 目标节点 id（UUID）。
    #[arg(long, value_name = "UUID")]
    node_id: String,
}

#[derive(Debug, Subcommand)]
enum ExportCommand {
    /// 建立 Export（`export.create`）；参数以 `--file` 传一份 JSON。
    Create(ExportCreate),
    /// 列出 Export（含已撤销项）。
    List,
    /// 撤销 Export（`export.revoke`）。
    Revoke(ExportRevoke),
}

#[derive(Debug, Args)]
struct ExportCreate {
    /// 与 `export.create` 的 `params` 同形的 JSON 对象。
    #[arg(long, value_name = "PATH")]
    file: PathBuf,
}

#[derive(Debug, Args)]
struct ExportRevoke {
    /// 目标 Export id。
    #[arg(long, value_name = "ID")]
    export_id: String,
}

#[derive(Debug, Subcommand)]
enum ImportCommand {
    /// 登记 Import（`import.add`；依赖当前 Node Link 连接上的 catalog 快照）。
    Add(ImportAdd),
    /// 列出 Import。
    List,
    /// 移除 Import（`import.remove`）。
    Remove(ImportRemove),
}

#[derive(Debug, Args)]
struct ImportAdd {
    /// Import 标识（`^[A-Za-z0-9._-]{1,128}$`）。
    #[arg(long, value_name = "ID")]
    import_id: String,
    /// Owner 的 `wss://` 端点。
    #[arg(long, value_name = "URL")]
    owner_endpoint: String,
    /// Owner 节点 id（UUID；必须是已配对且 `kind = "owner"` 的节点）。
    #[arg(long, value_name = "UUID")]
    owner_node_id: String,
    /// 请求的 Export id（逗号分隔，可重复）。
    #[arg(long = "export-id", value_delimiter = ',', value_name = "ID")]
    export_ids: Vec<String>,
    /// 请求的 grant（逗号分隔，可重复）。
    #[arg(long = "grant", value_delimiter = ',', value_name = "GRANT")]
    grants: Vec<String>,
}

#[derive(Debug, Args)]
struct ImportRemove {
    /// 目标 Import id。
    #[arg(long, value_name = "ID")]
    import_id: String,
}

/// 解析参数并运行。返回值即进程退出码。
pub fn run() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return parse_failure(&error),
    };
    // `acp-stdio` 的 stdout 是 ACP 通道，任何情况下都不得写入人类可读文本（包括失败简述）。
    let stream_stdout = matches!(&cli.command, Command::AcpStdio);
    match dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) if stream_stdout => fail_stream(&failure),
        Err(failure) => fail(&failure),
    }
}

/// 子命令分派：每个分支只做「解析出的 flags → §5.8 的方法与 `params`」这一件事。
fn dispatch(cli: Cli) -> Result<(), Failure> {
    let Cli { config, command } = cli;
    let mut context = Context::resolve(config)?;
    match command {
        Command::Daemon { command } => match command {
            DaemonCommand::Start => daemon_start(context.loaded),
            DaemonCommand::Stop(args) => daemon_stop(&mut context, &args),
            DaemonCommand::Status => daemon_status(&mut context),
        },
        Command::Workspace {
            command: WorkspaceCommand::Select(args),
        } => call_once(
            &mut context,
            Method::WorkspaceSelect,
            workspace_select_params(&args),
        ),
        Command::Agent {
            command: AgentCommand::Configure(args),
        } => call_once(
            &mut context,
            Method::AgentConfigure,
            agent_configure_params(&args)?,
        ),
        Command::Provider {
            command: ProviderCommand::Configure(args),
        } => provider_configure(&mut context, &args),
        Command::Device { command } => match command {
            DeviceCommand::Pair(args) => pairing::run(
                &mut context,
                pairing::PairTarget::Device,
                &pairing::PairArgs::from(&args),
            ),
            DeviceCommand::List => call_once(&mut context, Method::DeviceList, JsonObject::new()),
            DeviceCommand::Revoke(args) => call_once(
                &mut context,
                Method::DeviceRevoke,
                device_revoke_params(&args),
            ),
        },
        Command::Node { command } => match command {
            NodeCommand::Pair(args) => pairing::run(
                &mut context,
                pairing::PairTarget::Node,
                &pairing::PairArgs::from(&args),
            ),
            NodeCommand::List => call_once(&mut context, Method::NodeList, JsonObject::new()),
            NodeCommand::Revoke(args) => {
                call_once(&mut context, Method::NodeRevoke, node_revoke_params(&args))
            }
        },
        Command::Export { command } => match command {
            ExportCommand::Create(args) => call_once(
                &mut context,
                Method::ExportCreate,
                export_create_params(&args)?,
            ),
            ExportCommand::List => call_once(&mut context, Method::ExportList, JsonObject::new()),
            ExportCommand::Revoke(args) => call_once(
                &mut context,
                Method::ExportRevoke,
                export_revoke_params(&args),
            ),
        },
        Command::Import { command } => match command {
            ImportCommand::Add(args) => {
                call_once(&mut context, Method::ImportAdd, import_add_params(&args))
            }
            ImportCommand::List => call_once(&mut context, Method::ImportList, JsonObject::new()),
            ImportCommand::Remove(args) => call_once(
                &mut context,
                Method::ImportRemove,
                import_remove_params(&args),
            ),
        },
        Command::Doctor => doctor(&mut context),
        Command::AcpStdio => acp_stdio(&context),
    }
}

// ---------------------------------------------------------------------------------------------
// 运行环境（配置 + 本地通道定位）
// ---------------------------------------------------------------------------------------------

/// 一次 CLI 调用的共享环境：已加载的配置 + 按需创建的异步 runtime。
pub(crate) struct Context {
    loaded: Loaded,
    runtime: Option<tokio::runtime::Runtime>,
}

impl Context {
    /// 加载配置（`--config` → `ACP_REMOTE_CONFIG` → 平台默认位置）。
    fn resolve(config: Option<PathBuf>) -> Result<Self, Failure> {
        let path = config.or_else(config_path_from_env);
        let loaded = Config::load(path.as_deref())
            .map_err(|error| Failure::local(code_for_config_error(&error), error.to_string()))?;
        Ok(Self {
            loaded,
            runtime: None,
        })
    }

    /// 数据目录（`daemon.data_dir`）。
    fn data_dir(&self) -> &Path {
        &self.loaded.config.data_dir
    }

    /// current-thread runtime（管理子命令只需要单连接上的少量请求）。
    fn runtime(&mut self) -> Result<&tokio::runtime::Runtime, Failure> {
        if self.runtime.is_none() {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| Failure::local(LocalErrorCode::Internal, "无法创建异步 runtime"))?;
            self.runtime = Some(runtime);
        }
        self.runtime
            .as_ref()
            .ok_or_else(|| Failure::local(LocalErrorCode::Internal, "无法创建异步 runtime"))
    }

    /// 释放缓存的 runtime（连同其中所有连接）。
    ///
    /// **`daemon stop` 必须在等待单实例锁之前调用**：Tokio 在 Windows 上不会在 `drop(连接)` 时同步释放
    /// Named Pipe 句柄——句柄由挂起的 overlapped 读持有（mio 的 `NamedPipe::drop` 只取消挂起操作、
    /// 不关闭句柄），只有 runtime 的 I/O driver 处理完那个 completion 才真正 `CloseHandle`，而
    /// current-thread runtime 只在 `block_on` 期间驱动 driver。等锁循环全程 `std::thread::sleep`，
    /// 因此句柄会一直开到进程退出；在那之前 Daemon 的 `drain_connections` 看不到对端 EOF，只能等满
    /// `daemon.shutdown_grace_ms`（实测：`drain_timeout{remaining:1}` 与 `elapsed_ms ≈ grace_ms`）。
    fn release_runtime(&mut self) {
        // 显式命名被释放的值：该 `Runtime` 的析构会停机 I/O driver，让挂起的 completion 落定，
        // 连接句柄随之关闭。
        if let Some(runtime) = self.runtime.take() {
            drop(runtime);
        }
    }

    /// Daemon 运行中则返回本地通道定位串；没有有效锁时 `Ok(None)`（§7）。
    ///
    /// 「有锁但记录缺失/损坏」不视为「未运行」：这是 R11 的 `local.unavailable` 路径（不自动强杀、不直接读库）。
    fn running_endpoint(&self) -> Result<Option<String>, Failure> {
        match lock::probe(&lock::lock_path(self.data_dir())) {
            LockState::Free => Ok(None),
            LockState::Held => match lock::read_record(&lock::record_path(self.data_dir())) {
                Ok(Some(record)) => match record.endpoint {
                    Some(endpoint) if !endpoint.is_empty() => Ok(Some(endpoint)),
                    _ => Err(Failure::local(
                        LocalErrorCode::Unavailable,
                        "已有 Daemon 持有单实例锁，但它尚未发布本地通道定位串",
                    )),
                },
                Ok(None) => Err(Failure::local(
                    LocalErrorCode::Unavailable,
                    "已有 Daemon 持有单实例锁，但运行记录缺失（无法定位本地通道）",
                )),
                Err(_) => Err(Failure::local(
                    LocalErrorCode::Unavailable,
                    "运行记录内容损坏（无法定位本地通道）",
                )),
            },
            LockState::Unusable { .. } => Err(Failure::local(
                LocalErrorCode::Unavailable,
                "单实例锁文件不可用，无法判定 Daemon 是否在运行",
            )),
        }
    }

    /// 管理子命令的定位串：Daemon 未运行时明确失败（§7：不打开数据库、不启动核心）。
    fn endpoint(&self) -> Result<String, Failure> {
        self.running_endpoint()?.ok_or_else(|| {
            Failure::local(
                LocalErrorCode::Unavailable,
                "没有 acp-remote Daemon 在运行；先执行 `acp-remote daemon start`",
            )
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 参数 → `params`（kebab-case flags → camelCase wire 字段的唯一翻译点）
// ---------------------------------------------------------------------------------------------

fn workspace_select_params(args: &WorkspaceSelect) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert("alias".to_owned(), Value::from(args.alias.as_str()));
    params.insert(
        "displayName".to_owned(),
        Value::from(args.display_name.as_str()),
    );
    params.insert(
        "rootPath".to_owned(),
        Value::from(args.root_path.to_string_lossy().into_owned()),
    );
    params
}

/// `agent configure --file`：文件即 `params`；顶层字段名必须是 §5.2 的六个（凭据值没有字段可承载）。
fn agent_configure_params(args: &AgentConfigure) -> Result<JsonObject, Failure> {
    input::read_params_file(
        &args.file,
        &[
            "agentId",
            "displayName",
            "command",
            "args",
            "envAllowlist",
            "default",
        ],
    )
}

/// `export create --file`：文件即 `params`（嵌套结构由方法侧校验）。
fn export_create_params(args: &ExportCreate) -> Result<JsonObject, Failure> {
    input::read_params_file(&args.file, &[])
}

fn device_revoke_params(args: &DeviceRevoke) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert("deviceId".to_owned(), Value::from(args.device_id.as_str()));
    params
}

fn node_revoke_params(args: &NodeRevoke) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert("nodeId".to_owned(), Value::from(args.node_id.as_str()));
    params
}

fn export_revoke_params(args: &ExportRevoke) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert("exportId".to_owned(), Value::from(args.export_id.as_str()));
    params
}

fn import_add_params(args: &ImportAdd) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert("importId".to_owned(), Value::from(args.import_id.as_str()));
    params.insert(
        "ownerEndpoint".to_owned(),
        Value::from(args.owner_endpoint.as_str()),
    );
    params.insert(
        "ownerNodeId".to_owned(),
        Value::from(args.owner_node_id.as_str()),
    );
    params.insert("exportIds".to_owned(), strings(&args.export_ids));
    params.insert("grants".to_owned(), strings(&args.grants));
    params
}

fn import_remove_params(args: &ImportRemove) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert("importId".to_owned(), Value::from(args.import_id.as_str()));
    params
}

fn stop_params(args: &DaemonStop) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert(
        "graceMs".to_owned(),
        match args.grace_ms {
            Some(ms) => Value::from(ms),
            None => Value::Null,
        },
    );
    params
}

/// 字符串数组 → JSON 数组（保序）。
pub(crate) fn strings(items: &[String]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| Value::from(item.as_str()))
            .collect(),
    )
}

/// `provider configure` 的 `values`：字段名 → 凭据值（值只来自终端录入）。
fn provider_configure_params(args: &ProviderConfigure, values: &[(String, String)]) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert(
        "providerId".to_owned(),
        Value::from(args.provider_id.as_str()),
    );
    params.insert("kind".to_owned(), Value::from(args.kind.as_str()));
    params.insert(
        "displayName".to_owned(),
        Value::from(args.display_name.as_str()),
    );
    let mut map = JsonObject::new();
    for (field, value) in values {
        map.insert(field.clone(), Value::from(value.as_str()));
    }
    params.insert("values".to_owned(), Value::Object(map));
    params
}

/// 逐项读取凭据值（保持 `--field` 给出的顺序）。
pub(crate) fn provider_values(
    fields: &[String],
    prompt: &mut dyn input::SecretPrompt,
) -> Result<Vec<(String, String)>, Failure> {
    let mut values = Vec::with_capacity(fields.len());
    for field in fields {
        values.push((field.clone(), prompt.read_secret(field)?));
    }
    Ok(values)
}

// ---------------------------------------------------------------------------------------------
// 子命令实现
// ---------------------------------------------------------------------------------------------

/// `daemon start`：前台运行 Daemon，直到关闭序列完成（唯一会打开存储的子命令）。
fn daemon_start(loaded: Loaded) -> Result<(), Failure> {
    if let Err(error) = crate::logging::init(&loaded.config.logging) {
        return Err(Failure::local(LocalErrorCode::Internal, error.to_string()));
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| Failure::local(LocalErrorCode::Internal, "无法创建异步 runtime"))?;
    runtime
        .block_on(crate::daemon::run(loaded))
        .map_err(|error| Failure::reported(error.code().to_owned(), error.message()))
}

/// `daemon status`：运行中经本地通道问 Daemon；未运行时在 CLI 进程内回答（不连接、不打开数据库）。
fn daemon_status(context: &mut Context) -> Result<(), Failure> {
    match context.running_endpoint()? {
        None => {
            println!("daemon 未运行（没有有效单实例锁）");
            Ok(())
        }
        Some(endpoint) => {
            let runtime = context.runtime()?;
            let outcome =
                call_outcome(runtime, &endpoint, Method::DaemonStatus, JsonObject::new())?;
            print_result(&success(&outcome)?);
            Ok(())
        }
    }
}

/// `daemon stop`：接受后关闭连接并等待 Daemon 释放单实例锁（§7、R13）。
fn daemon_stop(context: &mut Context, args: &DaemonStop) -> Result<(), Failure> {
    let Some(endpoint) = context.running_endpoint()? else {
        println!("daemon 未运行（没有有效单实例锁），无需停止");
        return Ok(());
    };
    let runtime = context.runtime()?;
    runtime.block_on(async {
        let mut client = LocalAdminClient::connect(&endpoint)
            .await
            .map_err(client_failure)?;
        let response = client
            .call(Method::DaemonStop, stop_params(args))
            .await
            .map_err(client_failure)?;
        let result = success(&outcome_of(&response))?;
        match result.get("accepted") {
            Some(Value::Bool(true)) => {}
            _ => {
                return Err(Failure::local(
                    LocalErrorCode::Internal,
                    "daemon.stop 的结果缺 `accepted: true`",
                ));
            }
        }
        // 本进程主动关闭连接：Daemon 的排空窗口不为 CLI 的读端等待（关闭序列第一步已释放 endpoint）。
        drop(client);
        Ok(())
    })?;
    // 连接句柄只有在它的 runtime 被销毁时才真正释放（见 `Context::release_runtime`）：不释放的话，
    // 整个 `wait_for_lock_release` 期间对端都看不到 EOF，Daemon 的排空窗口只能等满宽限。
    context.release_runtime();
    wait_for_lock_release(context.data_dir())?;
    println!("daemon 已停止（单实例锁已释放）");
    Ok(())
}

/// 轮询到单实例锁可获取（即 Daemon 已完成关闭序列）。
fn wait_for_lock_release(data_dir: &Path) -> Result<(), Failure> {
    let lock_path = lock::lock_path(data_dir);
    let deadline = std::time::Instant::now() + STOP_LOCK_TIMEOUT;
    loop {
        if matches!(lock::probe(&lock_path), LockState::Free) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(Failure::local(
                LocalErrorCode::Unavailable,
                "Daemon 在期限内未释放单实例锁",
            ));
        }
        std::thread::sleep(STOP_LOCK_POLL);
    }
}

/// `doctor`：组合 `daemon.status`（运行时）与 CLI 侧本地检查；离线时全部在进程内完成。
fn doctor(context: &mut Context) -> Result<(), Failure> {
    println!("{:<12}ok", "config");
    match std::fs::metadata(context.data_dir()) {
        Ok(meta) if meta.is_dir() => println!("{:<12}存在", "dataDir"),
        Ok(_) => {
            return Err(Failure::local(
                LocalErrorCode::Unavailable,
                "配置的数据目录路径存在但不是目录",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("{:<12}不存在（Daemon 尚未初始化）", "dataDir");
        }
        Err(_) => {
            return Err(Failure::local(
                LocalErrorCode::Unavailable,
                "配置的数据目录不可访问",
            ));
        }
    }
    match context.running_endpoint()? {
        None => {
            println!("{:<12}未运行（没有有效单实例锁）", "daemon");
            Ok(())
        }
        Some(endpoint) => {
            let runtime = context.runtime()?;
            let outcome =
                call_outcome(runtime, &endpoint, Method::DaemonStatus, JsonObject::new())?;
            let result = success(&outcome)?;
            println!("{:<12}运行中", "daemon");
            for (label, key) in [
                ("version", "version"),
                ("instanceId", "instanceId"),
                ("nodeId", "nodeId"),
                ("uptimeMs", "uptimeMs"),
            ] {
                let value = match result.get(key) {
                    Some(Value::String(text)) => text.clone(),
                    Some(Value::Number(number)) => number.to_string(),
                    _ => continue,
                };
                println!("{label:<12}{value}");
            }
            if let Some(counts) = result.get("counts") {
                println!("{:<12}{}", "counts", counts);
            }
            Ok(())
        }
    }
}

/// `acp-stdio`：只做 stdin/stdout ↔ channel `0x02` 的字节泵（不进管理信封）。
fn acp_stdio(context: &Context) -> Result<(), Failure> {
    let Some(endpoint) = context.running_endpoint()? else {
        return Err(Failure::local(
            LocalErrorCode::Unavailable,
            "没有 acp-remote Daemon 在运行：无法提供 ACP 流",
        ));
    };
    crate::stdio::pump(&endpoint)
        .map(|_| ())
        .map_err(|error| match error {
            crate::stdio::StdioError::NoStream | crate::stdio::StdioError::NoInput => {
                Failure::local(LocalErrorCode::Unavailable, error.to_string())
            }
            crate::stdio::StdioError::Connect | crate::stdio::StdioError::Transport => {
                Failure::local(LocalErrorCode::Unavailable, error.to_string())
            }
            crate::stdio::StdioError::Frame | crate::stdio::StdioError::Output => {
                Failure::local(LocalErrorCode::Internal, error.to_string())
            }
            crate::stdio::StdioError::Runtime => {
                Failure::local(LocalErrorCode::Internal, error.to_string())
            }
        })
}

/// 一条「本地方法 + 已构造 `params`」的子命令：未运行即失败，成功后展示 `result`。
fn call_once(context: &mut Context, method: Method, params: JsonObject) -> Result<(), Failure> {
    let endpoint = context.endpoint()?;
    let runtime = context.runtime()?;
    let outcome = call_outcome(runtime, &endpoint, method, params)?;
    print_result(&success(&outcome)?);
    Ok(())
}

/// 一次「连接 → 一条请求 → 关闭」的调用，并把响应拆成 [`ClientOutcome`]。
fn call_outcome(
    runtime: &tokio::runtime::Runtime,
    endpoint: &str,
    method: Method,
    params: JsonObject,
) -> Result<ClientOutcome, Failure> {
    let response = runtime
        .block_on(crate::client::call_once(endpoint, method, params))
        .map_err(client_failure)?;
    Ok(outcome_of(&response))
}

/// `provider configure`：无回显逐项录入凭据后调用 `provider.configure`。
fn provider_configure(context: &mut Context, args: &ProviderConfigure) -> Result<(), Failure> {
    // 无交互终端时**先**明确失败：不读取凭据、不调用方法（R69）。
    let mut prompt = input::TerminalPrompt::new()?;
    let values = provider_values(&args.fields, &mut prompt)?;
    call_once(
        context,
        Method::ProviderConfigure,
        provider_configure_params(args, &values),
    )
}

// ---------------------------------------------------------------------------------------------
// 输出与错误出口
// ---------------------------------------------------------------------------------------------

/// 成功结果的展示：`result` 的紧凑 JSON。
///
/// `result` 的字段名与类型就是 §5.2–§5.6 的 wire 形状，且 `result` **永不回显凭据值**
/// （`provider.configure` 只回字段名），因此这里直接展示即可，不需要（也不允许）另造展示模型。
pub(crate) fn print_result(result: &JsonObject) {
    println!("{}", Value::Object(result.clone()));
}

/// 失败响应的拆解：`ok = false` 时把 `local.*` 码原样带出（CLI 不解析 `error.message`）。
pub(crate) fn success(outcome: &ClientOutcome) -> Result<JsonObject, Failure> {
    match outcome {
        ClientOutcome::Success(result) => Ok(result.clone()),
        ClientOutcome::Failure { code, message } => {
            Err(Failure::reported(code.clone(), message.clone()))
        }
    }
}

/// 传输级失败 → 错误码。消息是固定的英文短句（不回显底层路径/凭据，也不含堆栈）。
pub(crate) fn client_failure(error: ClientError) -> Failure {
    let message = match error {
        ClientError::Connect { .. } => {
            "failed to connect to the local admin channel (the daemon is not running or is shutting down)"
        }
        ClientError::Transport { .. } => {
            "the local admin channel failed while the request was in flight"
        }
        ClientError::Closed => "the local admin channel was closed before the response arrived",
        ClientError::Frame { .. } | ClientError::Response { .. } => {
            return Failure::local(
                LocalErrorCode::Internal,
                "the daemon returned a response that is not a valid v1 admin envelope",
            );
        }
    };
    Failure::local(LocalErrorCode::Unavailable, message)
}

/// 配置解析失败的错误码：`local.invalid_params`（「未知字段/类型不符/越界」正是配置文件的失败形态）。
fn code_for_config_error(error: &ConfigError) -> LocalErrorCode {
    match error {
        ConfigError::Unreadable { .. } | ConfigError::NoDefaultDirectory => {
            LocalErrorCode::Unavailable
        }
        ConfigError::Invalid { .. } | ConfigError::ManagedSectionInStartupConfig { .. } => {
            LocalErrorCode::InvalidParams
        }
    }
}

/// 一次 CLI 失败。
///
/// `code` 取自 `LOCAL_ADMIN_PROTOCOL.md` §6 的本地错误码表（[`LocalErrorCode`]，CLI 侧失败）或 Daemon 原样
/// 返回的码（`reported`，新 Daemon 可能返回我们词表外的码——那时不静默改写成别的码）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Failure {
    code: String,
    message: String,
}

impl Failure {
    /// CLI 侧失败：码来自 §6 的本地错误码表。
    pub(crate) fn local(code: LocalErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: code.as_str().to_owned(),
            message: message.into(),
        }
    }

    /// Daemon 报告的失败：码原样带出。
    pub(crate) fn reported(code: String, message: String) -> Self {
        Self { code, message }
    }

    /// stderr 结构化行里的 `code`。
    pub(crate) fn code(&self) -> &str {
        &self.code
    }

    /// 人类可读说明。
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

/// 失败的统一出口：stdout 一行人类可读说明 + stderr **恰好一行**结构化 JSON（含 `code`）。
fn fail(failure: &Failure) -> ExitCode {
    println!("acp-remote: {}", failure.message);
    eprintln!("{}", failure_line(&failure.code, &failure.message));
    ExitCode::FAILURE
}

/// `acp-stdio` 的失败出口：stdout **只**承载 ACP 字节流，因此人类可读说明也走 stderr（但仍是单行 JSON 为主）。
fn fail_stream(failure: &Failure) -> ExitCode {
    eprintln!("{}", failure_line(&failure.code, &failure.message));
    ExitCode::FAILURE
}

/// 结构化的 stderr 行（与 [`fail`] 共用同一构造，避免两处漂移）。
fn failure_line(code: &str, message: &str) -> String {
    serde_json::json!({ "code": code, "message": message }).to_string()
}

/// 用法错误的出口：`--help`/`--version` 走 stdout 且成功；其余以 `local.invalid_request` 失败。
///
/// 退出码 `2` 是用法错误的独立档位（非零，且与运行期失败的 `1` 区分）。
fn parse_failure(error: &clap::Error) -> ExitCode {
    use clap::error::ErrorKind;
    match error.kind() {
        ErrorKind::DisplayHelp
        | ErrorKind::DisplayVersion
        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            print!("{error}");
            ExitCode::SUCCESS
        }
        _ => {
            // 只取首段（clap 的渲染含 usage 与提示行，多行会破坏「stdout 一行人类可读说明」）。
            let summary = error
                .to_string()
                .lines()
                .take_while(|line| !line.trim().is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let failure = Failure::local(LocalErrorCode::InvalidRequest, summary);
            println!("acp-remote: {}", failure.message());
            eprintln!("{}", failure_line(failure.code(), failure.message()));
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory as _;

    /// §5.8 映射表第一列的子命令与第二列的分组（唯一权威：`docs/LOCAL_ADMIN_PROTOCOL.md` §5.8）。
    const DOCUMENTED: [(&str, &[&str]); 8] = [
        ("daemon", &["start", "stop", "status"]),
        ("workspace", &["select"]),
        ("agent", &["configure"]),
        ("provider", &["configure"]),
        ("device", &["pair", "list", "revoke"]),
        ("node", &["pair", "list", "revoke"]),
        ("export", &["create", "list", "revoke"]),
        ("import", &["add", "list", "remove"]),
    ];

    /// 子命令集合与 §5.8 逐条一致：不多不少（`doctor`/`acp-stdio` 是顶层叶子），且没有 `session create`
    /// 与 `daemon doctor` 这类自造方法。
    #[test]
    fn the_subcommand_set_matches_the_documented_mapping() {
        let command = Cli::command();
        let names: Vec<&str> = command
            .get_subcommands()
            .map(|subcommand| subcommand.get_name())
            .collect();
        let mut expected: Vec<&str> = DOCUMENTED.iter().map(|(name, _)| *name).collect();
        expected.push("doctor");
        expected.push("acp-stdio");
        assert_eq!(names, expected);

        for (name, nested) in DOCUMENTED {
            let group = command
                .get_subcommands()
                .find(|subcommand| subcommand.get_name() == name)
                .expect("分组子命令");
            let actual: Vec<&str> = group
                .get_subcommands()
                .map(|subcommand| subcommand.get_name())
                .collect();
            assert_eq!(actual, nested, "`{name}` 的子命令必须与 §5.8 逐条一致");
        }

        // 自造方法/未落地入口必须被 clap 拒绝（不是静默 no-op）。
        for rejected in [
            vec!["acp-remote", "daemon", "doctor"],
            vec!["acp-remote", "session", "create"],
            vec!["acp-remote", "node", "rotate-key"],
            vec!["acp-remote", "audit", "export"],
        ] {
            assert!(
                Cli::try_parse_from(rejected.clone()).is_err(),
                "{rejected:?} 必须不存在"
            );
        }
    }

    /// `doctor`/`acp-stdio` 是顶层叶子，且 `--config` 是全局参数（子命令之后同样接受）。
    #[test]
    fn global_config_and_leaf_commands_parse() {
        assert!(Cli::try_parse_from(["acp-remote", "doctor", "--config", "x"]).is_ok());
        assert!(Cli::try_parse_from(["acp-remote", "acp-stdio"]).is_ok());
        assert!(Cli::try_parse_from(["acp-remote", "daemon", "start", "--config", "x"]).is_ok());
        assert!(Cli::try_parse_from(["acp-remote", "--config", "x", "daemon", "status"]).is_ok());
        assert!(Cli::try_parse_from(["acp-remote", "daemon"]).is_err());
    }

    /// 失败行恰好一行、含 `code`（`cli-commands` 规格的「退出码与错误输出契约」）。
    #[test]
    fn the_failure_line_is_one_json_object_with_a_code() {
        let line = failure_line("local.conflict", "another acp-remote daemon is running");
        assert!(!line.contains('\n'), "必须恰好一行：{line}");
        let parsed: Value = serde_json::from_str(&line).expect("合法 JSON");
        assert_eq!(parsed["code"], Value::from("local.conflict"));
        assert!(parsed["message"].is_string());

        let failure = Failure::reported("local.future_code".to_owned(), "x".to_owned());
        assert_eq!(failure.code(), "local.future_code");
        assert_eq!(
            Failure::local(LocalErrorCode::Unavailable, "y").code(),
            "local.unavailable"
        );
    }

    /// kebab-case flags → camelCase `params`：字段名与 §5.2/§5.6 的 wire 形状逐条对应。
    #[test]
    fn flags_map_onto_the_documented_wire_field_names() {
        let workspace = workspace_select_params(&WorkspaceSelect {
            alias: "ws".to_owned(),
            display_name: "工作区".to_owned(),
            root_path: PathBuf::from("/tmp/ws"),
        });
        assert_eq!(
            Value::Object(workspace),
            serde_json::json!({ "alias": "ws", "displayName": "工作区", "rootPath": "/tmp/ws" })
        );

        let stop = stop_params(&DaemonStop {
            grace_ms: Some(1500),
        });
        assert_eq!(Value::Object(stop), serde_json::json!({ "graceMs": 1500 }));
        let stop = stop_params(&DaemonStop { grace_ms: None });
        assert_eq!(Value::Object(stop), serde_json::json!({ "graceMs": null }));

        assert_eq!(
            Value::Object(device_revoke_params(&DeviceRevoke {
                device_id: "D".to_owned()
            })),
            serde_json::json!({ "deviceId": "D" })
        );
        assert_eq!(
            Value::Object(node_revoke_params(&NodeRevoke {
                node_id: "N".to_owned()
            })),
            serde_json::json!({ "nodeId": "N" })
        );
        assert_eq!(
            Value::Object(export_revoke_params(&ExportRevoke {
                export_id: "E".to_owned()
            })),
            serde_json::json!({ "exportId": "E" })
        );
        assert_eq!(
            Value::Object(import_remove_params(&ImportRemove {
                import_id: "I".to_owned()
            })),
            serde_json::json!({ "importId": "I" })
        );
        assert_eq!(
            Value::Object(import_add_params(&ImportAdd {
                import_id: "I".to_owned(),
                owner_endpoint: "wss://owner.example.invalid".to_owned(),
                owner_node_id: "00000000-0000-4000-8000-000000000001".to_owned(),
                export_ids: vec!["E".to_owned()],
                grants: vec!["grant.session.read".to_owned()],
            })),
            serde_json::json!({
                "importId": "I",
                "ownerEndpoint": "wss://owner.example.invalid",
                "ownerNodeId": "00000000-0000-4000-8000-000000000001",
                "exportIds": ["E"],
                "grants": ["grant.session.read"],
            })
        );

        let provider = provider_configure_params(
            &ProviderConfigure {
                provider_id: "openai".to_owned(),
                kind: "provider".to_owned(),
                display_name: "OpenAI".to_owned(),
                fields: vec!["api_key".to_owned()],
            },
            &[("api_key".to_owned(), "sk-x".to_owned())],
        );
        assert_eq!(
            Value::Object(provider),
            serde_json::json!({
                "providerId": "openai",
                "kind": "provider",
                "displayName": "OpenAI",
                "values": { "api_key": "sk-x" },
            })
        );

        // 凭据值**没有**命令行入口：没有 `--values`/`--value`/`--file` 这类 flag。
        let unknown = Cli::try_parse_from([
            "acp-remote",
            "provider",
            "configure",
            "--provider-id",
            "p",
            "--kind",
            "provider",
            "--display-name",
            "P",
            "--field",
            "api_key",
            "--value",
            "sk-x",
        ]);
        assert!(unknown.is_err(), "凭据值不得作为命令行参数");
        let file_input = Cli::try_parse_from([
            "acp-remote",
            "provider",
            "configure",
            "--provider-id",
            "p",
            "--kind",
            "provider",
            "--display-name",
            "P",
            "--field",
            "api_key",
            "--file",
            "secrets.json",
        ]);
        assert!(file_input.is_err(), "凭据值不得经普通文件输入");
    }

    #[test]
    fn config_errors_map_into_the_local_error_table() {
        assert_eq!(
            code_for_config_error(&ConfigError::Invalid {
                detail: String::new()
            }),
            LocalErrorCode::InvalidParams
        );
        assert_eq!(
            code_for_config_error(&ConfigError::Unreadable {
                source: std::io::Error::from(std::io::ErrorKind::NotFound)
            }),
            LocalErrorCode::Unavailable
        );
    }
}
