//! WP4a 集成测试的公共部分：真实二进制 + 独立临时数据目录。
//!
//! 口径（`openspec/changes/daemon-cli-and-local-admin/specs/daemon-lifecycle/spec.md`）：
//!
//! - 每个用例一个独立 `dataDir`（与独立的 `XDG_RUNTIME_DIR`）：R1「全新数据目录」可判定，且并行用例互不干扰；
//! - 用**真实二进制**（`CARGO_BIN_EXE_acp-remote`）而不是进程内直接调用：单实例锁、endpoint 创建、
//!   进程退出码与「CLI 无法直接访问 SQLite」这几条只有跨进程才能验证；
//! - Daemon 是**前台进程**：用例必须显式 `daemon.stop` 并等待退出，未退出则杀掉（测试不得留下僵尸进程）；
//! - 就绪判定不靠 sleep：轮询运行记录 + `daemon.status`（本地通道可用即视为就绪）；
//! - 每个用例都在 `dev_mode.enabled = true` + `identity.keystore = "ephemeral"` 下运行：CI 的 Linux
//!   runner 没有平台 keystore，而本切片要求 fail-closed，只有显式开发模式才允许进程内 keystore
//!   （`CONFIG_REFERENCE.md` §8/§10）。测试因此**不**覆盖平台 keystore 路径（见交接说明）。
//!
//! 网络接入面接线后，基础配置一律写 `listen = "127.0.0.1:0"`（内核分配端口）：默认的固定端口 8765
//! 会让并行用例（以及同一台机器上的其他实例）互相抢端口（plan 的「Runtime Resources」规则）。
//! 需要特定监听地址的用例用 `Daemon::configure_with_listen`。

#![allow(dead_code)] // 各用例文件只用到其中一部分辅助函数

// 受控路径全链路集成测试的 Owner 侧与 Access 侧（`node_link_e2e`）：生产接线 + 真实 loopback
// listener + 脚本化 fake Access 客户端。其余用例文件不使用这两个模块。
pub mod nodelink;
pub mod owner;

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use app::LocalAdminClient;
use app::client::{ClientOutcome, outcome_of};
use app::lock::{LockRecord, read_record};
use serde_json::Value;
use server::local_admin::{AdminResponse, JsonObject, Method, RequestId, decode_request};

/// 与 `storage-sqlite`/`server` 的目录创建同口径：Unix 上按 `0700` 建立，否则
/// `strict_permissions`/endpoint 的运行目录权限检查会在 Linux/macOS 上对既有目录失败关闭
/// （Windows 的权限判定是 `Unverifiable`，本地不触发）。
#[cfg(unix)]
pub fn create_owner_only_dir(path: &Path) {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .expect("仅所有者可访问的目录");
}

/// Windows 等价物：ACL 由平台默认继承为当前用户专属（见 `storage-sqlite` 的权限口径）。
#[cfg(not(unix))]
pub fn create_owner_only_dir(path: &Path) {
    std::fs::create_dir_all(path).expect("仅所有者可访问的目录");
}

/// 等待就绪/退出的上限。启动含 SQLite 迁移与 keystore 准备，慢机器上留足余量。
const READY_TIMEOUT: Duration = Duration::from_secs(30);
/// 停止后等待进程退出的上限。
const EXIT_TIMEOUT: Duration = Duration::from_secs(15);
/// 轮询间隔。
const POLL: Duration = Duration::from_millis(50);

/// 用例级临时目录（`Drop` 清理；Windows 上句柄释放有延迟，因此带重试）。
///
/// **归属**：只有创建它的用例负责删除；同一数据目录的第二轮运行（重启用例）持有 `owner = false` 的
/// 视图，删不删由第一轮的所有者决定。
pub struct TempRoot {
    path: PathBuf,
    owner: bool,
}

impl TempRoot {
    /// 新建独立临时目录。
    pub fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("acpr-wp4a-{label}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        create_owner_only_dir(&path);
        Self { path, owner: true }
    }

    /// 复用已有临时目录（不负责删除）。
    pub fn borrowed(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            owner: false,
        }
    }

    /// 目录路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 子路径（必要时由调用方自行创建）。
    pub fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }

    /// 供 TOML 使用的路径文本（正斜杠，避免基本字符串转义）。
    pub fn toml(&self, relative: &str) -> String {
        self.join(relative).display().to_string().replace('\\', "/")
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        if !self.owner {
            return;
        }
        for _ in 0..10 {
            match std::fs::remove_dir_all(&self.path) {
                Ok(()) => return,
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }
}

/// 一次 `daemon start` 的子进程与它的数据目录。
pub struct Daemon {
    root: TempRoot,
    config_path: PathBuf,
    data_dir: PathBuf,
    log_path: PathBuf,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    #[cfg(unix)]
    runtime_dir: PathBuf,
    child: Option<Child>,
    instance_id: Option<String>,
    endpoint: Option<String>,
}

/// 基础配置里三段可覆盖内容的来源（`build` 的唯一入参形状）。
struct ConfigParts<'a> {
    /// `daemon.listen`（`None` = `127.0.0.1:0`）。
    listen: Option<&'a str>,
    /// 追加到 `[daemon]` 段落里的键。
    daemon: &'a str,
    /// 追加到 `[dev_mode]` 段落里的键。
    dev_mode: &'a str,
    /// 追加在整份配置末尾的段落/表（例如 `[[agents.profiles]]`）。
    extra: &'a str,
}

impl Daemon {
    /// 写好独立配置（不启动）。`extra` 追加在基础配置之后（例如 `[[agents.profiles]]`）。
    pub fn configure(label: &str, extra: &str) -> Self {
        Self::build(
            TempRoot::new(label),
            label,
            ConfigParts {
                listen: None,
                daemon: "",
                dev_mode: "",
                extra,
            },
        )
    }

    /// 同上，但允许覆盖基础 `[daemon]` 段落里的键（例如 `shutdown_grace_ms`）。
    pub fn configure_with(label: &str, daemon_extra: &str, extra: &str) -> Self {
        Self::build(
            TempRoot::new(label),
            label,
            ConfigParts {
                listen: None,
                daemon: daemon_extra,
                dev_mode: "",
                extra,
            },
        )
    }

    /// 同上，但显式给出 `daemon.listen`（共享 listener 接线后的绑定/失败关闭用例）。
    ///
    /// 基础配置默认写 `listen = "127.0.0.1:0"`（内核分配端口）：并行用例与同一台机器上的其他实例
    /// 不会因为默认的固定端口 8765 互相抢端口（plan 的「Runtime Resources」规则）。
    pub fn configure_with_listen(
        label: &str,
        listen: &str,
        daemon_extra: &str,
        extra: &str,
    ) -> Self {
        Self::build(
            TempRoot::new(label),
            label,
            ConfigParts {
                listen: Some(listen),
                daemon: daemon_extra,
                dev_mode: "",
                extra,
            },
        )
    }

    /// 同上，但允许覆盖基础 `[dev_mode]` 段落里的键（例如 `allow_plaintext`）。
    ///
    /// 基础配置固定 `[dev_mode]` 的表头，因此覆盖只能经本构造器（写第二段 `[dev_mode]` 会被 TOML
    /// 判成重复键）。
    pub fn configure_with_dev_mode(
        label: &str,
        listen: &str,
        dev_mode_extra: &str,
        extra: &str,
    ) -> Self {
        Self::build(
            TempRoot::new(label),
            label,
            ConfigParts {
                listen: Some(listen),
                daemon: "",
                dev_mode: dev_mode_extra,
                extra,
            },
        )
    }

    /// 同一数据目录的**第二轮运行**（重启用例）：配置/日志/输出各自独立文件，数据目录与运行时目录相同。
    pub fn configure_in(&self, label: &str, extra: &str) -> Self {
        let mut next = Self::build(
            TempRoot::borrowed(self.root()),
            label,
            ConfigParts {
                listen: None,
                daemon: "",
                dev_mode: "",
                extra,
            },
        );
        next.data_dir = self.data_dir.clone();
        #[cfg(unix)]
        {
            next.runtime_dir = self.runtime_dir.clone();
        }
        next
    }

    fn build(root: TempRoot, label: &str, parts: ConfigParts<'_>) -> Self {
        let data_dir = root.path().join("data");
        #[cfg(unix)]
        let runtime_dir = root.path().join("runtime");
        #[cfg(unix)]
        create_owner_only_dir(&runtime_dir);
        // 第二轮运行不得覆盖第一轮的配置与日志（顺序不同、内容不同，便于分别断言）。
        let suffix = if label.is_empty() {
            String::new()
        } else {
            format!("{label}.")
        };
        let config_path = root.path().join(format!("acp-remote.{suffix}toml"));
        let log_path = root.path().join(format!("daemon.{suffix}log"));
        let log_text = root
            .path()
            .join(format!("daemon.{suffix}log"))
            .display()
            .to_string()
            .replace('\\', "/");
        let text = format!(
            r#"[daemon]
data_dir = "{data_dir}"
public_origin = "https://acpr-test.example.invalid"
listen = "{listen}"
{daemon_extra}

[identity]
keystore = "ephemeral"

[dev_mode]
enabled = true
ephemeral_identity = true
{dev_mode_extra}

[logging]
level = "info"
format = "json"
file = "{log}"

{extra}
"#,
            data_dir = data_dir.display().to_string().replace('\\', "/"),
            listen = parts.listen.unwrap_or("127.0.0.1:0"),
            daemon_extra = parts.daemon,
            dev_mode_extra = parts.dev_mode,
            extra = parts.extra,
            log = log_text,
        );
        std::fs::write(&config_path, text).expect("配置文件");
        Self {
            stdout_path: root.path().join(format!("stdout.{suffix}txt")),
            stderr_path: root.path().join(format!("stderr.{suffix}txt")),
            root,
            config_path,
            data_dir,
            log_path,
            #[cfg(unix)]
            runtime_dir,
            child: None,
            instance_id: None,
            endpoint: None,
        }
    }

    /// 配置路径。
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// 数据目录。
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// 子进程 pid。
    pub fn pid(&self) -> u32 {
        self.child.as_ref().expect("已启动").id()
    }

    /// 临时根目录。
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// 日志文件内容（`logging.file`；按行 JSON）。
    pub fn log(&self) -> String {
        std::fs::read_to_string(&self.log_path).unwrap_or_default()
    }

    /// 日志中 `event` 字段等于 `event` 的行（保持出现顺序）。
    pub fn log_events(&self, event: &str) -> Vec<Value> {
        self.log()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|value| value["event"].as_str() == Some(event))
            .collect()
    }

    /// 某个事件在日志里的首次出现位置（不存在为 `None`）。
    pub fn log_index(&self, event: &str) -> Option<usize> {
        self.log().lines().position(|line| {
            serde_json::from_str::<Value>(line).is_ok_and(|value| value["event"] == event)
        })
    }

    /// 子进程 stderr 内容。
    pub fn stderr(&self) -> String {
        let mut text = String::new();
        if let Ok(mut file) = std::fs::File::open(&self.stderr_path) {
            let _ = file.read_to_string(&mut text);
        }
        text
    }

    /// 子进程 stdout 内容。
    pub fn stdout(&self) -> String {
        let mut text = String::new();
        if let Ok(mut file) = std::fs::File::open(&self.stdout_path) {
            let _ = file.read_to_string(&mut text);
        }
        text
    }

    /// 启动并等待就绪（运行记录已发布 + `daemon.status` 可回答）。
    pub fn start(&mut self) {
        self.spawn();
        self.wait_ready();
    }

    /// 只派生进程（用于「重复启动被拒」这类需要自行断言的用例）。
    pub fn spawn(&mut self) -> &mut Child {
        assert!(self.child.is_none(), "已经启动过");
        let mut command = Command::new(env!("CARGO_BIN_EXE_acp-remote"));
        command
            .arg("daemon")
            .arg("start")
            .arg("--config")
            .arg(&self.config_path)
            .env("ACP_REMOTE_CONFIG", &self.config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                std::fs::File::create(&self.stdout_path).expect("stdout 文件"),
            ))
            .stderr(Stdio::from(
                std::fs::File::create(&self.stderr_path).expect("stderr 文件"),
            ));
        #[cfg(unix)]
        command.env("XDG_RUNTIME_DIR", &self.runtime_dir);
        self.child = Some(command.spawn().expect("启动 acp-remote"));
        self.child.as_mut().expect("已启动")
    }

    /// 等待就绪：轮询运行记录与本地通道。超时即失败，并把日志/错误输出带进断言信息。
    pub fn wait_ready(&mut self) {
        let deadline = Instant::now() + READY_TIMEOUT;
        let record_path = self.data_dir.join(app::lock::RECORD_FILE_NAME);
        while Instant::now() < deadline {
            if let Some(child) = self.child.as_mut() {
                if let Some(status) = child.try_wait().expect("try_wait") {
                    panic!("Daemon 在就绪前退出（{status}）：{}", self.stderr());
                }
            }
            if let Ok(Some(record)) = read_record(&record_path) {
                if let Some(endpoint) = record.endpoint.clone() {
                    if let Ok(response) = self.call_raw(&endpoint, Method::DaemonStatus, params()) {
                        if matches!(outcome_of(&response), ClientOutcome::Success(_)) {
                            self.instance_id = Some(record.instance_id);
                            self.endpoint = Some(endpoint);
                            return;
                        }
                    }
                }
            }
            std::thread::sleep(POLL);
        }
        panic!(
            "等待 Daemon 就绪超时：stderr={} log={}",
            self.stderr(),
            self.log()
        );
    }

    /// 运行记录（就绪后可用）。
    pub fn record(&self) -> LockRecord {
        read_record(&self.data_dir.join(app::lock::RECORD_FILE_NAME))
            .expect("读取运行记录")
            .expect("运行记录存在")
    }

    /// 本次运行的 `instanceId`。
    pub fn instance_id(&self) -> &str {
        self.instance_id.as_deref().expect("已就绪")
    }

    /// endpoint 定位串。
    pub fn endpoint(&self) -> &str {
        self.endpoint.as_deref().expect("已就绪")
    }

    /// 新连接上执行一条请求；返回响应（协议错误即失败）。
    pub fn call(&self, method: Method, params: JsonObject) -> AdminResponse {
        self.call_raw(self.endpoint(), method, params)
            .expect("调用")
    }

    /// 成功结果的 `result`（失败即失败）。
    pub fn ok(&self, method: Method, params: JsonObject) -> JsonObject {
        match outcome_of(&self.call(method, params)) {
            ClientOutcome::Success(result) => result,
            ClientOutcome::Failure { code, message } => {
                panic!("{method:?} 必须成功，实际 {code}：{message}")
            }
        }
    }

    /// 成功结果的 `result` 作为 JSON 值。
    pub fn ok_value(&self, method: Method, params: JsonObject) -> Value {
        Value::Object(self.ok(method, params))
    }

    /// 失败结果的本地错误码（成功即失败）。
    pub fn error_code(&self, method: Method, params: JsonObject) -> String {
        match self.call_outcome(method, params) {
            ClientOutcome::Failure { code, .. } => code,
            ClientOutcome::Success(_) => panic!("{method:?} 必须失败，实际成功"),
        }
    }

    /// 一次调用并拆解结果。
    pub fn call_outcome(&self, method: Method, params: JsonObject) -> ClientOutcome {
        outcome_of(&self.call(method, params))
    }

    /// 在指定 endpoint 上执行一条请求。
    fn call_raw(
        &self,
        endpoint: &str,
        method: Method,
        params: JsonObject,
    ) -> Result<AdminResponse, app::client::ClientError> {
        block_on(async move {
            let mut client = LocalAdminClient::connect(endpoint).await?;
            client.call(method, params).await
        })
    }

    /// 打开一条长连接（用于「关闭期间的在途连接」用例）。
    pub fn open_connection(&self) -> HeldConnection {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("运行时");
        let client = runtime
            .block_on(LocalAdminClient::connect(self.endpoint()))
            .expect("连接");
        HeldConnection { runtime, client }
    }

    /// 请求关闭并等待进程退出。
    pub fn stop(&mut self) -> ExitStatus {
        match self.call_outcome(Method::DaemonStop, stop_params()) {
            ClientOutcome::Success(result) => {
                assert_eq!(result.get("accepted"), Some(&Value::Bool(true)))
            }
            ClientOutcome::Failure { code, message } => {
                panic!("daemon.stop 必须成功，实际 {code}：{message}")
            }
        }
        self.wait_exit()
    }

    /// 等待子进程退出（超时则杀掉并失败）。
    pub fn wait_exit(&mut self) -> ExitStatus {
        let child = self.child.as_mut().expect("正在运行");
        let deadline = Instant::now() + EXIT_TIMEOUT;
        while Instant::now() < deadline {
            if let Some(status) = child.try_wait().expect("try_wait") {
                return status;
            }
            std::thread::sleep(POLL);
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("Daemon 未在期限内退出：log={}", self.log());
    }

    /// 子进程是否仍在运行。
    pub fn is_running(&mut self) -> bool {
        self.child
            .as_mut()
            .expect("正在运行")
            .try_wait()
            .expect("try_wait")
            .is_none()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

/// 一条保持打开的管理连接（在途连接）。
///
/// **必须持有自己的 runtime**：连接句柄属于创建它的 Tokio 驱动，runtime 一旦 drop，后续 I/O 会以
/// 「A Tokio 1.x context was found, but it is being shutdown」失败（这与 `/runtime` 无关，是 tokio 的
/// 既有约束）。
pub struct HeldConnection {
    runtime: tokio::runtime::Runtime,
    client: LocalAdminClient,
}

impl HeldConnection {
    /// 在这条连接上发一条请求。
    pub fn call(&mut self, method: Method, params: JsonObject) -> AdminResponse {
        match self.try_call(method, params) {
            Ok(response) => response,
            Err(error) => panic!("{method:?} 在在途连接上失败：{error}"),
        }
    }

    /// 在这条连接上发一条请求并保留传输错误（关闭序列已经关闭 endpoint 时连接会断）。
    pub fn try_call(
        &mut self,
        method: Method,
        params: JsonObject,
    ) -> Result<AdminResponse, app::client::ClientError> {
        let Self { runtime, client } = self;
        runtime.block_on(client.call(method, params))
    }

    /// 关闭连接。
    pub fn close(self) {}
}

/// 空参数。
pub fn params() -> JsonObject {
    JsonObject::new()
}

/// `daemon.stop` 的参数（`graceMs` 是必需字段，可为 null）。
pub fn stop_params() -> JsonObject {
    params_of(serde_json::json!({ "graceMs": null }))
}

/// 由 JSON 构造参数（字段名逐一列出，避免 `unwrap`）。
pub fn params_of(value: Value) -> JsonObject {
    match value {
        Value::Object(map) => map,
        other => panic!("params 必须是对象：{other}"),
    }
}

/// 在独立线程的 current-thread runtime 上执行（用例是同步的，客户端是异步的）。
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("运行时")
        .block_on(future)
}

/// 启动一个「第二次 `daemon start`」进程并返回其退出状态与 stderr。
pub fn run_start_once(config: &Path) -> (ExitStatus, String, String) {
    run_start_once_with_env(config, &[])
}

/// 一次性启动（带环境覆盖）：`daemon start` 的失败路径必须在期限内退出，
/// 不允许驻留——没有超时的 `output()` 会让「意外启动成功」挂死整个测试进程（CI 实踩）。
pub fn run_start_once_with_env(
    config: &Path,
    envs: &[(&str, &Path)],
) -> (ExitStatus, String, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_acp-remote"));
    command
        .arg("daemon")
        .arg("start")
        .arg("--config")
        .arg(config)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("运行 acp-remote");
    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait().expect("轮询子进程").is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("一次性启动用例的 daemon 在 30s 内没有退出（意外驻留？）");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().expect("收集输出");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 解析 CLI 失败时 stderr 的那一行结构化 JSON（恰好一行，含 `code`）。
pub fn failure_code(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "stderr 必须恰好一行：{stderr}");
    let parsed: Value = serde_json::from_str(lines[0]).expect("合法 JSON");
    parsed["code"].as_str().expect("code 是字符串").to_owned()
}

/// `RequestId` 由用例固定，便于把「响应 id 必须等于请求 id」作为断言（§4 规则 5）。
pub fn request_id(text: &str) -> RequestId {
    RequestId::parse(text).expect("请求 id 是规范 uuid")
}
/// 解码一份请求信封（断言 CLI 形状时使用）。
pub fn decode(bytes: &[u8]) -> server::local_admin::AdminRequest {
    decode_request(bytes).expect("信封合法")
}

/// 一次 CLI 子进程调用的结果。
///
pub struct CliRun {
    /// 退出状态；超时被强制结束时为 `None`。
    pub status: Option<ExitStatus>,
    /// stdout 全文。
    pub stdout: String,
    /// stderr 全文。
    pub stderr: String,
    /// 是否超时被强杀（用例据此失败，而不是挂到宿主超时）。
    pub timed_out: bool,
}

impl CliRun {
    /// 退出码（超时即失败）。
    pub fn exit_code(&self) -> i32 {
        assert!(!self.timed_out, "CLI 子进程超时（未在期限内退出）");
        self.status.expect("已结束").code().expect("有退出码")
    }

    /// 断言成功的退出码（`0`）。
    pub fn assert_success(&self) {
        assert_eq!(
            self.exit_code(),
            0,
            "stdout={} stderr={}",
            self.stdout,
            self.stderr
        );
        assert!(
            self.stderr.trim().is_empty(),
            "成功时 stderr 必须为空：{}",
            self.stderr
        );
    }

    /// 断言失败的退出码（非零）与 stderr 那一行 JSON 的 `code`。
    pub fn assert_failure(&self, code: &str) -> String {
        assert!(!self.timed_out, "CLI 子进程超时（未在期限内退出）");
        assert_ne!(self.exit_code(), 0, "必须非零退出：stdout={}", self.stdout);
        let actual = failure_code(&self.stderr);
        assert_eq!(
            actual, code,
            "stdout={} stderr={}",
            self.stdout, self.stderr
        );
        actual
    }
}

/// stdin 的给法。
pub enum Stdin {
    /// 空设备（`/dev/null` 等价）。
    Null,
    /// 管道：写入这些字节后关闭（含 NUL 的二进制也按原样写）。
    Bytes(Vec<u8>),
}

/// CLI 子进程的等待上限（挂住时强杀，用例以「超时」失败而不是拖住整个测试）。
const CLI_TIMEOUT: Duration = Duration::from_secs(60);

/// 运行一条 CLI 子命令（不启动 Daemon）。
///
/// `config` 非空时追加 `--config <path>`；`stdin` 按 [`Stdin`] 给出（默认空设备，保证非交互）。
///
/// 输出目录用 [`TempRoot`] 托管：**不**在函数尾部手动删（否则下一行的 `Drop` 会重试删除已删目录），
/// 因此断言失败或提前返回时也不会留下 `acpr-*` 目录。
pub fn run_cli(label: &str, config: Option<&Path>, args: &[&str], stdin: Stdin) -> CliRun {
    let root = TempRoot::new(&format!("cli-{label}"));
    let stdout_path = root.join("stdout.txt");
    let stderr_path = root.join("stderr.txt");
    let mut command = Command::new(env!("CARGO_BIN_EXE_acp-remote"));
    command.args(args);
    if let Some(config) = config {
        command.arg("--config").arg(config);
    }
    command
        .env_remove("ACP_REMOTE_CONFIG")
        .stdin(match &stdin {
            Stdin::Null => Stdio::null(),
            Stdin::Bytes(_) => Stdio::piped(),
        })
        .stdout(Stdio::from(
            std::fs::File::create(&stdout_path).expect("stdout 文件"),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(&stderr_path).expect("stderr 文件"),
        ));
    let mut child = command.spawn().expect("运行 acp-remote");
    if let Stdin::Bytes(bytes) = &stdin {
        use std::io::Write as _;
        let mut pipe = child.stdin.take().expect("stdin 管道");
        pipe.write_all(bytes).expect("写 stdin");
        // 关闭 stdin：字节泵与配对仪式都必须能在对端关闭时退出（不留半开的管道）。
        drop(pipe);
    }
    let deadline = Instant::now() + CLI_TIMEOUT;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            timed_out = true;
            break None;
        }
        std::thread::sleep(POLL);
    };
    let read = |path: &Path| std::fs::read_to_string(path).unwrap_or_default();
    CliRun {
        status,
        stdout: read(&stdout_path),
        stderr: read(&stderr_path),
        timed_out,
    }
}

/// 目录里的文件名（排序；供「CLI 未创建任何文件」这类断言）。
pub fn dir_entries(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(path)
        .expect("目录可读")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}
