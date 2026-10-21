//! Sync 命令面：`command` 帧的解码分派、**授权前置**、十条查询命令的同步完成、结果超限的显式失败、
//! 按（设备，命令族）计数的命令限流与查询类命令的连接期去重
//! （`docs/SYNC_PROTOCOL.md` §11/§12.2/§14；变更 `sync-server-commands` 的任务 2.1–2.6，
//! 行为范围以 `openspec/changes/archive/2026-10-16-sync-server-commands/specs/sync-command-handling/spec.md`
//! 与变更 `sync-path-browse-and-diff-delivery` 的 `sync-workspace-browse` 为准）。
//!
//! ## 管线顺序（§11.2 的「命令处理建议顺序」）
//!
//! ```text
//! schema(解码/封闭词表) → 连接期去重 → 限流(设备×命令族) → 已认证设备 → scope 授权
//!   → 状态/版本 → …（幂等预留、持久接受、会话 actor、Agent、持久事件、广播属 WP3 的 mutation 路径）
//! ```
//!
//! 本工作包（WP2）落地的是**查询命令**那一半：`session.list`、`session.read`、`command.status`、
//! `session.mode.list`、`session.config.list` 同步返回**一次** `completed` 或 `rejected`，MUST NOT 先回
//! `accepted`（§11.2「查询命令不进入异步队列」）——`workspace.browse` 是同一形状的第六条（变更
//! `sync-path-browse-and-diff-delivery` 的 WP3 加入，见 [`query::workspace_browse`]）。mutation 的接受
//! 路径、幂等与终态观察者是同一个模块目录的下一个工作包（WP3）——本包在
//! [`CommandRoute::on_mutation`] 处**显式失败关闭**，绝不伪造 `accepted`、绝不静默成功，也绝不把
//! mutation 当作已实现。
//!
//! ## 五条硬规则
//!
//! 1. **前提先于读取**（[`CommandRoute::authorize`]）：每条业务命令在读会话行、触碰文件系统、启动进程
//!    **之前**先落定「本连接已通过设备认证」这条前提——设备行必须存在（缺失即认证类失败关闭），判定只
//!    取决于设备是否存在，与命令名、会话是否存在无关，因此错误类型不构成会话是否存在的预言机。设备
//!    方向**没有**命令级授权判定（`pairing-model` 删除设备权限层）；[`acp_core::broker::Broker::authorize`]
//!    仍是 core 侧唯一判定点（设备主体恒通过、节点主体按 Export/grant 判定），本层不重写那份判定，
//!    用例内部还会再走一次（同一次调用、同一判定，成功路径不产生审计行）。
//! 2. **结果超限显式失败**（[`CommandRoute::too_large`]）：查询结果在**入队前**按编码后字节数判定，
//!    超过本连接的单条消息上限时回 `resource.result_too_large`。**绝不**截断后当完整结果返回
//!    （§11.5 末段）。
//! 3. **限流按设备与命令族**（[`limits`]）：默认 120/分钟，键是 `(deviceId, 命令族)`，**不**以连接或
//!    网络来源身份计数（§14「网络身份不能替代设备授权」）；单次超限回可重试的 `resource.rate_limited`
//!    （带 `details.retryAfterMs`），**连续**超限达阈值才以 `4429` 关闭该连接。
//! 4. **封闭词表外一律 `command.unsupported` 且不在本机执行任何操作**（[`decode`]）：
//!    `session.resume` 只经 Node Link 接受，Sync v1 收到时同样按 `command.unsupported` 拒绝
//!    （§11.5 该行）。
//! 5. **连接期去重不落盘**（[`ConnectionState::answered`]）：查询类命令的重复提交只在本连接内去重
//!    （§11.2「查询类命令可以只在连接期去重」），同一连接内重复 `messageId` 不产生第二次业务读取；
//!    不同连接各自独立。
//!
//! ## 依赖纪律（`AGENTS.md` §5、`docs/MODULE_ARCHITECTURE.md` §4.9）
//!
//! 只调用 `core` 的公开面 + `sync-protocol`/`identity-auth` 的公开面 + 本 crate 内 `sync::conn` 的连接面。
//! **不 import `storage-sqlite`、不调用 `server::node_link`**。与 [`crate::sync::subscribe`] 同款地，
//! 本模块今天还**直连** `core::broker` 两处：`Broker::authorize` 是观察授权的唯一判定点，
//! `Broker::session_version` 是 `session.config.list` 结果里 `version` 的唯一来源（`session.mode.list`
//! 的同款数值由 `UseCases::mode_list` 一并给出）。两者都收窄到「已按同一命令名授权之后」的读，不构成
//! 第二条可见性判定路径。

pub mod decode;
pub mod limits;
pub(crate) mod query;

use std::collections::BTreeMap;
use std::str::FromStr as _;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use acp_core::broker::Broker;
use acp_core::model::{
    Actor, AttachmentId, ClientCommand, CommandKind, CommandPayload as CorePayload, CommandReceipt,
    ConfigOptionId, ConfigValue as CoreConfigValue, ConflictKind, ContentVersion, Digest,
    ElicitationAction as CoreElicitationAction, ElicitationValues, InteractionId, ModeId,
    PortError, PromptContentBlock, PublicError as CorePublicError, RESULT_TOO_LARGE,
    ReadInclude as CoreReadInclude, RequestId as CoreRequestId, SessionFilePath, SessionId,
    SubmissionId, Timestamp as CoreTimestamp, TurnId, UnavailableKind, Version, WorkspaceSource,
};
use acp_core::ports::SessionFiles;
use acp_core::use_cases::UseCases;
use identity_auth::Authority;
use sha2::Digest as _;
use sync_protocol::command::{
    Command, CommandName, CommandPayload, CommandResult, CommandResultPayload, CommandState,
    ElicitationAction as WireElicitationAction, IncludeResource as WireReadInclude,
    SessionCreateResult, SessionCreateWorkspace, SessionFilesRead,
};
use sync_protocol::common::{
    ConfigOptionValue as WireConfigValue, Nullable, PublicError, RawObject,
    SessionSummary as WireSessionSummary, Text, Timestamp, Uuid,
};
use sync_protocol::envelope::{Envelope, MessageType, encode_body};
use sync_protocol::error::{Body, ErrorCode};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::{debug, warn};

use crate::attachment_transfer::{
    AttachmentBinding, Fault as AttachmentFault, verify_prompt_bindings,
};
use crate::command_log::{self, Cause, Subject, TerminalClass};
use crate::sync::attachments::ATTACHMENT_FEATURE;
use crate::sync::conn::registry::SendFault;
use crate::sync::conn::{ConnectionHandle, ConnectionRegistry, MessageRoute, RouteOutcome, close};
use crate::sync::snapshot::LOCAL_CATALOG_FEATURE;
use crate::sync::subscribe::cursor::CursorFault;
use crate::sync::subscribe::{NegotiatedFeatures, OwnerReachability};
use crate::transport::net::Shutdown;

use self::limits::{CommandFamily, CommandRateLimiter};
use self::query::Rejection;

/// `core.session-create.v1`：Sync 面 `session.create` 的 feature 门控（§11.5）。
///
/// 未协商时服务端 MUST NOT 静默接受（回 `protocol.feature_required` 并给出缺的那个）。
pub const SESSION_CREATE_FEATURE: &str = "core.session-create.v1";
/// 独立建目录的可选能力；旧连接不得触发文件系统写操作。
pub const WORKSPACE_MKDIR_FEATURE: &str = "core.workspace-mkdir.v1";
/// 会话文件与 Git 只读查询的能力 ID（§11.5）。`session.files.*`/`session.git.*` 的门控在用例调用与
/// 任何文件/Git I/O 之前；宣告该 feature 的前提是真实路由与连接期内容版本释放已接线。
pub const SESSION_FILES_FEATURE: &str = "core.session-files.v1";

/// 单条命令结果的**信封**开销上界（字节）。
///
/// `ConnectionHandle::send` 把 body 包进完整信封后才写出，因此「结果超过单条消息上限」必须在**入队前**
/// 判定，而判定要拿「将要发出的那份字节」比，不能只比 body。信封的固定部分逐字为
/// `{"protocolVersion":1,"type":"command.result","messageId":"<36>","connectionId":"<36>",`
/// `"connectionSequence":"<≤20>","body":<body>}`：固定分隔与键名 110 字节 + 两个 UUID 的 72 字节 +
/// `connectionSequence` 的最多 20 位 = **202** 字节。`connectionSequence` 是 u64 的十进制串，最多 20 位；
/// `messageId` 与 `connectionId` 都经 `Uuid::parse`（canonical 带连字符，恒为 36 字符），因此 202 在
/// 任何序号与标识下都是上界，且取到该上界的唯一前提是序号恰好有 20 位。
///
/// 这个数字是**测出来的**而不是估出来的：`the_envelope_overhead_is_the_measured_worst_case` 用出站同一
/// 条装配路径（`sync_protocol` 的信封 + `encode_body`）装配一帧并与本常量逐字节比对；`an_almost_full_query_result_still_fits`
/// 再把真实出站帧的长度当作上限，证明这条判定不会把装得下的结果误判成超限。
///
/// **不得**放大取值「留余量」：与上限比较的是加法，常量每虚高 1 字节，就会把 body 落在
/// `[max - 常量, max - 真实开销]` 区间里**装得下**的结果误判成 `resource.result_too_large`。
const ENVELOPE_OVERHEAD_BYTES: u64 = 202;

/// 一条连接的命令面状态。
#[derive(Debug, Default)]
struct ConnectionState {
    /// 本次协商出的 feature（`core.session-history.v1` 与 `resource.remote-origin.v1` 的门控）。
    features: NegotiatedFeatures,
    /// 连续被限流的次数：达 [`limits::MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS`] 即以 `4429` 关闭连接。
    consecutive_rate_limited: u32,
    /// 查询类命令的连接期去重表（**不落盘**，§11.2）。键是 `(messageId, requestId)`：同一连接内重复的
    /// **同一帧**因此不再产生第二次业务读取；不同连接各自独立。
    answered: Vec<((Uuid, Uuid), CommandResult)>,
    /// 3.5：待观察终态的 mutation。连接断开即整表丢弃（观察者是连接期状态，不留 detached task）。
    watching: Vec<WatchedCommand>,
    /// 本连接唯一的活动内容版本（§11.5）：`version=null` 的首次成功读取建立它，带 `version` 的续读必须
    /// 命中同一 `(设备, 会话, 路径, 版本)`。连接关闭时由 [`CommandRoute::on_closed`] 释放。
    content: Option<ContentBinding>,
}

/// 一条连接的活动内容版本及其绑定维度（`docs/SYNC_PROTOCOL.md` §11.5）。
///
/// 版本令牌只在建立它的连接/设备/会话/路径组合内有效，不能当作跨连接的 bearer 凭据。设备维度与连接
/// 一一对应（连接在注册表里按设备键登记），但仍显式记录，使「跨连接/跨设备/跨会话/跨路径」四类拒绝
/// 由同一处比较给出。
#[derive(Debug, Clone)]
struct ContentBinding {
    /// 建立快照的连接所属设备。
    device: acp_core::model::DeviceId,
    /// 该快照所属的会话。
    session: SessionId,
    /// 根内相对路径。
    path: SessionFilePath,
    /// 不透明内容版本。
    version: ContentVersion,
}

impl ContentBinding {
    /// 续读是否命中同一绑定：设备、会话、路径与版本全部相同才算数。
    fn matches(
        &self,
        device: &acp_core::model::DeviceId,
        session: &SessionId,
        path: &str,
        version: &str,
    ) -> bool {
        self.device == *device
            && self.session == *session
            && self.path.as_str() == path
            && self.version.as_str() == version
    }
}

/// 释放一个连接期内容版本；失败只记诊断（释放是幂等的，端口的空闲 TTL 与 daemon 关闭的
/// `release_all` 还有兜底）。抽成自由函数让「连接关闭必须调用端口」这条路径可被 fake 端口直接断言。
async fn release_binding(files: &dyn SessionFiles, binding: Option<ContentBinding>) {
    let Some(binding) = binding else {
        return;
    };
    if let Err(error) = files.release_content(&binding.version).await {
        debug!(
            event = "sync.session_files_release_failed",
            error = ?error,
            "the content snapshot could not be released"
        );
    }
}

/// 3.5 的终态观察项：提交方按 `(deviceId, requestId)` 等自己的终态。
///
/// 存的是**提交时那份 `Actor`**：`find_request(request, actor)` 按 actor 收窄幂等键，用别的形状
/// 去读会读不到自己刚提交的记录。
#[derive(Debug, Clone)]
struct WatchedCommand {
    request_id: Uuid,
    device_id: acp_core::model::DeviceId,
    actor: Actor,
    /// 登记时刻：观察有界（超过 [`WATCH_MAX_AGE`] 即放弃，对端仍可凭 `command.status` 重查）。
    since: Instant,
}

/// 终态观察的轮询间隔（与 Node Link 的观察者同值）。
///
/// v1 只能轮询：broker 的扇出信号（`CommittedEvent`）不携带 `causation`，无法把一条终态定位回
/// `requestId` 与连接（该结论由 Node Link 的实现期得出，本适配器沿用同一手段）。
pub const TERMINAL_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// 一条终态观察的最长生存时间。超龄即放弃该观察项（记录仍在，对端可凭 `command.status` 重查）。
pub const WATCH_MAX_AGE: Duration = Duration::from_secs(600);

const MKDIR_QUEUE_CAPACITY: usize = 32;
const MKDIR_EXECUTION_LIMIT: usize = 4;

/// core 的 `CommandStatus`（token enum，`as_str`）+ Sync wire 的 `CommandState`。
///
/// 两个词表取值相同但类型不同源，因此显式映射：**不**依赖字符串相等（拼写漂移会在编译期沉默）。
/// 未知取值走 `Failed`：终态必须是四者之一，钉一个假 `uncertain` 会让对端以为不可重试。
fn wire_command_state(status: acp_core::model::CommandStatus) -> CommandState {
    use acp_core::model::CommandStatus;
    match status {
        CommandStatus::Completed => CommandState::Completed,
        CommandStatus::Failed => CommandState::Failed,
        CommandStatus::Uncertain => CommandState::Uncertain,
        CommandStatus::Rejected | CommandStatus::Accepted => CommandState::Rejected,
    }
}

/// wire 的 `CommandState` → 命令级日志的终态类别。
///
/// `accepted`/`rejected` 返回 `None`：前者不是终态，后者不是**已接受**命令的终态（那条命令从未被接受，
/// 它的结局由 `sync.command_rejected` 一类日志记录）。
fn terminal_class(status: CommandState) -> Option<TerminalClass> {
    match status {
        CommandState::Completed => Some(TerminalClass::Completed),
        CommandState::Failed => Some(TerminalClass::Failed),
        CommandState::Uncertain => Some(TerminalClass::Uncertain),
        CommandState::Accepted | CommandState::Rejected => None,
    }
}

/// Sync 命令面的路由（`conn` 的 [`MessageRoute`] 实现之一，组合根注册）。
pub struct CommandRoute {
    core: Arc<UseCases>,
    /// 授权与 `session.config.list` 的版本读取（见模块文档的依赖纪律）。
    broker: Arc<Broker>,
    /// 命令结果 `acceptedAt` 的时间来源（与身份状态机同一个注入时钟；本层不读系统时间）。
    authority: Arc<Authority>,
    /// 连接存活判据：连接期状态随连接离开注册表回收（不遗留无主状态）。
    registry: Arc<ConnectionRegistry>,
    /// Owner 可达性（`session.list` 的 `origin.online`）：本适配器没有 Owner 连接面
    /// （不得调用 `server::node_link`），因此这条运行时事实由组合根注入，与
    /// [`crate::sync::subscribe`] 的订阅面共用同一个来源。
    owner: Arc<dyn OwnerReachability>,
    /// 会话文件/Git 只读端口：与 `UseCases` 内部注入的是**同一个实例**，连接关闭/换新版本时由本路由
    /// 调用 `release_content`（用例面不暴露端口，释放责任在接线层）。
    files: Arc<dyn SessionFiles>,
    rate: CommandRateLimiter,
    /// 连续超限多少次即以 `4429` 关闭连接（生产路径恒为
    /// [`limits::MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS`]，用例把它压小）。
    consecutive_limit: u32,
    states: Mutex<BTreeMap<String, ConnectionState>>,
    /// 新的终态观察登记时唤醒 [`Self::dispatch`]，避免只有 250ms 轮询才发现它。
    wake: tokio::sync::Notify,
    mkdir_queue: mpsc::Sender<acp_core::ports::WorkspaceMkdirExecution>,
    mkdir_receiver: Mutex<Option<mpsc::Receiver<acp_core::ports::WorkspaceMkdirExecution>>>,
}

impl std::fmt::Debug for CommandRoute {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandRoute")
            .field("connections", &lock(&self.states).len())
            .finish_non_exhaustive()
    }
}

impl CommandRoute {
    /// 装配。`owner` 与 [`crate::sync::subscribe::SubscribeRoute`] 拿到的是同一个来源；`files` 必须与
    /// 组合根注入 `UseCases::with_session_files` 的是**同一个实例**（连接级内容版本释放用它）。
    pub fn new(
        core: Arc<UseCases>,
        broker: Arc<Broker>,
        authority: Arc<Authority>,
        registry: Arc<ConnectionRegistry>,
        owner: Arc<dyn OwnerReachability>,
        files: Arc<dyn SessionFiles>,
    ) -> Self {
        let (mkdir_queue, mkdir_receiver) = mpsc::channel(MKDIR_QUEUE_CAPACITY);
        Self {
            core,
            broker,
            authority,
            registry,
            owner,
            files,
            rate: CommandRateLimiter::new(limits::COMMANDS_PER_MINUTE, limits::COMMAND_WINDOW),
            consecutive_limit: limits::MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS,
            states: Mutex::new(BTreeMap::new()),
            wake: tokio::sync::Notify::new(),
            mkdir_queue,
            mkdir_receiver: Mutex::new(Some(mkdir_receiver)),
        }
    }

    /// 命令限流（§14：单设备业务命令 120/分钟，按 `deviceId` 与命令族分别统计）。
    ///
    /// 返回 `true` 表示本次命令**已被拒绝并已回复**，调用方不得继续处理。单次超限只回可重试的
    /// `resource.rate_limited`；连续超限达阈值时额外以 `4429` 请求关闭该连接（被拒命令零副作用）。
    fn admit_rate(
        &self,
        handle: &ConnectionHandle,
        command: CommandName,
        message_id: &Uuid,
    ) -> bool {
        let family = CommandFamily::of(command);
        let device = handle.device_id().as_str().to_owned();
        let outcome = self.rate.check(&device, family);
        let mut states = lock(&self.states);
        let state = states
            .entry(handle.connection_id().as_str().to_owned())
            .or_default();
        match outcome {
            limits::RateOutcome::Allowed => {
                state.consecutive_rate_limited = 0;
                false
            }
            limits::RateOutcome::Denied { retry_after } => {
                state.consecutive_rate_limited += 1;
                let close_now = state.consecutive_rate_limited >= self.consecutive_limit;
                drop(states);
                warn!(
                    event = "sync.command_rate_limited",
                    device_id = device.as_str(),
                    family = family.as_str(),
                    retry_after_ms = retry_after.as_millis() as u64,
                    close = close_now,
                    "the device exceeded its command rate"
                );
                let details = retry_after_details(retry_after);
                self.send_error_with_correlation(
                    handle,
                    ErrorCode::ResourceRateLimited,
                    "the command rate limit was exceeded",
                    details,
                    Some(message_id),
                );
                if close_now {
                    handle.request_close(close::RATE_LIMITED, "command rate limit exceeded");
                }
                true
            }
        }
    }

    /// 本连接的 feature 协商结果（未登记时取空集：两个门控都关闭，失败关闭的取值）。
    fn features(&self, handle: &ConnectionHandle) -> NegotiatedFeatures {
        lock(&self.states)
            .get(handle.connection_id().as_str())
            .map(|state| state.features.clone())
            .unwrap_or_default()
    }

    /// 本连接的**已认证前提**：该设备在本机必须有一条持久信任行。
    ///
    /// 设备方向不再有命令级授权判定（`pairing-model` 删除设备权限层：配对不授予授权、设备记录不保存
    /// 授权集合），因此本函数只把「已认证」这条前提落成**持久事实**——行不存在（认证后被删除、接线
    /// 错误）即失败关闭，不凭握手自报值继续。判定发生在任何会话行读取、文件系统访问与进程启动之前，
    /// 且只取决于设备是否存在（与命令名、会话是否存在无关），因此错误类型既不泄露会话是否存在，也
    /// 不构成第二条可见性判定路径。
    ///
    /// `Broker::authorize` 仍走一次：它是 core 侧唯一判定点（设备主体恒通过，节点主体仍按 Export/grant
    /// 判定），本层不重写那份判定，成功路径不产生审计。
    async fn authorize(&self, handle: &ConnectionHandle, command: &Command) -> Option<Actor> {
        let view = match self
            .core
            .sync_device_handshake_view(handle.device_id())
            .await
        {
            Ok(view) => view,
            Err(error) => {
                warn!(
                    event = "sync.command_device_view_failed",
                    device_id = handle.device_id().as_str(),
                    error = ?error,
                    "the persisted device snapshot cannot be read"
                );
                self.deny(
                    handle,
                    command,
                    "the device state cannot be read",
                    ErrorCode::InternalUnavailable,
                );
                return None;
            }
        };
        if view.device.is_none() {
            warn!(
                event = "sync.command_device_missing",
                device_id = handle.device_id().as_str(),
                "the authenticated device has no persisted trust row"
            );
            self.deny(
                handle,
                command,
                "the device is not known to this host",
                ErrorCode::AuthDeviceUnknown,
            );
            return None;
        }
        let actor = Actor::Device {
            device: handle.device_id().clone(),
        };
        if let Err(error) = self.authorize_actor(&actor, command).await {
            debug!(
                event = "sync.command_denied",
                device_id = handle.device_id().as_str(),
                command = command.command.as_str(),
                error = ?error,
                "the command is not allowed for this actor"
            );
            self.deny(
                handle,
                command,
                "the command cannot be served",
                ErrorCode::InternalUnavailable,
            );
            return None;
        }
        Some(actor)
    }

    /// `Broker::authorize` 的适配层调用（命令名、会话 id 与 requestId 的形状转换都在这里）。
    async fn authorize_actor(
        &self,
        actor: &Actor,
        command: &Command,
    ) -> Result<(), acp_core::broker::Denied> {
        let Some(request) = core_request_id(&command.request_id) else {
            // wire 的 `requestId` 已是 uuid，转换失败只可能是接线错误；失败关闭。
            return Err(acp_core::broker::Denied::scope_denied());
        };
        let session = command
            .session_id
            .as_ref()
            .and_then(|id| acp_core::model::SessionId::new(id.as_str()).ok());
        self.broker
            .authorize(actor, command.command.as_str(), session.as_ref(), &request)
            .await
    }

    /// 回一条 `rejected` 的 `command.result`（`acceptedAt` 为 `null`、`result` 为 `null`、`error` 非空）。
    fn deny(&self, handle: &ConnectionHandle, command: &Command, message: &str, code: ErrorCode) {
        let Some(error) = public_error(code, message, RawObject::empty()) else {
            return;
        };
        let result = CommandResult {
            request_id: command.request_id.clone(),
            command: command.command,
            status: CommandState::Rejected,
            accepted_at: Nullable::null(),
            terminal_event_id: Nullable::null(),
            result: Nullable::null(),
            error: Nullable::value(error),
        };
        self.send_result(handle, &result, "rejected");
    }

    /// 回一条 `command.result`（入队失败只记诊断：连接正在结束，调用方不需要额外分支）。
    fn send_result(&self, handle: &ConnectionHandle, result: &CommandResult, reason: &'static str) {
        match handle.send(MessageType::CommandResult, result) {
            Ok(()) => {}
            Err(SendFault::HighWater) | Err(SendFault::Closed) => debug!(
                event = "sync.command_result_not_queued",
                connection_id = handle.connection_id().as_str(),
                request_id = result.request_id.as_str(),
                reason,
                "the command result could not be queued"
            ),
            Err(SendFault::Encoding) => warn!(
                event = "sync.command_result_encoding_failed",
                connection_id = handle.connection_id().as_str(),
                request_id = result.request_id.as_str(),
                reason,
                "the command result could not be encoded"
            ),
        }
    }

    /// 回一条 `error` 消息（消息级拒绝：连接保持可用）。
    ///
    /// 用于**无法组装 `command.result`** 的拒绝：`command.unsupported`（命令名不在 Sync 的封闭词表里，
    /// 拿不到合法的 `command` 取值来填结果体）与 schema 类错误。`correlationId` 是该命令帧的
    /// `messageId`，因此客户端能把它和原请求对上。
    fn send_error(
        &self,
        handle: &ConnectionHandle,
        code: ErrorCode,
        message: &str,
        details: RawObject,
    ) {
        self.send_error_with_correlation(handle, code, message, details, None);
    }

    /// 与 [`Self::send_error`] 相同，但显式给出 `correlationId`（命令级拒绝一律填该命令帧的
    /// `messageId`，客户端据此把拒绝对回原请求）。
    fn send_error_with_correlation(
        &self,
        handle: &ConnectionHandle,
        code: ErrorCode,
        message: &str,
        details: RawObject,
        correlation: Option<&Uuid>,
    ) {
        let Some(body) = error_body(code, message, details, correlation) else {
            warn!(
                event = "sync.command_error_body_failed",
                connection_id = handle.connection_id().as_str(),
                "the command error body could not be assembled"
            );
            return;
        };
        if let Err(fault) = handle.send(MessageType::Error, &body) {
            debug!(
                event = "sync.command_error_not_queued",
                connection_id = handle.connection_id().as_str(),
                fault = ?fault,
                "the command error could not be queued"
            );
        }
    }

    /// 结果超限判定（§11.5：查询结果超过单条消息上限时返回 `resource.result_too_large`）。
    ///
    /// 判定点是**入队之前**：先按 `sync-protocol` 的信封编码算出将要发出的那份 body 的字节数，加上
    /// [`ENVELOPE_OVERHEAD_BYTES`] 后与本连接的 `maxMessageBytes` 比较。**不**截断、**不**分批、
    /// **不**把超限结果写成"看起来完整"的短结果——超限即显式失败，让客户端改用快照/事件视图。
    fn too_large(&self, handle: &ConnectionHandle, result: &CommandResult) -> bool {
        result_exceeds_message_limit(result, handle.limits().max_message_bytes())
    }

    /// 记录一次已答复的查询（连接期去重表，有界）。键是 `(messageId, requestId)`：同一连接内**同一帧**
    /// 的重发因此命中缓存，不再产生第二次业务读取；换了 `messageId` 的同 `requestId` 重发是合法重试，
    /// 走正常路径（§11.1 只要求 `requestId` 复用，查询类的去重是连接期的许可而非义务）。
    fn remember(&self, handle: &ConnectionHandle, message_id: &Uuid, result: CommandResult) {
        let mut states = lock(&self.states);
        let state = states
            .entry(handle.connection_id().as_str().to_owned())
            .or_default();
        while state.answered.len() >= limits::MAX_DEDUP_ENTRIES {
            state.answered.remove(0);
        }
        state
            .answered
            .push(((message_id.clone(), result.request_id.clone()), result));
    }

    /// 连接期去重命中（同一连接内重复的 `(messageId, requestId)` 不再产生第二次业务读取）。
    fn answered(
        &self,
        handle: &ConnectionHandle,
        message_id: &Uuid,
        request_id: &Uuid,
    ) -> Option<CommandResult> {
        let states = lock(&self.states);
        states
            .get(handle.connection_id().as_str())
            .and_then(|state| {
                state
                    .answered
                    .iter()
                    .find(|(key, _)| key.0 == *message_id && key.1 == *request_id)
                    .map(|(_, result)| result.clone())
            })
    }

    /// 回收「不在存活连接集合里」的连接期状态（限流窗口、去重表与内容版本随之消失）。
    ///
    /// 内容版本不能只丢引用：这里同时调用端口 `release_content`。正常关闭已由
    /// [`MessageRoute::on_closed`] 释放；本函数覆盖「连接任务被中止、没有走到收尾路径」的兜底。
    async fn reclaim(&self, current: &str) {
        let alive: Vec<String> = self
            .registry
            .handles()
            .iter()
            .map(|handle| handle.connection_id().as_str().to_owned())
            .collect();
        let mut released = Vec::new();
        {
            let mut states = lock(&self.states);
            states.retain(|key, state| {
                let keep = key == current || alive.contains(key);
                if !keep {
                    if let Some(binding) = state.content.take() {
                        released.push(binding);
                    }
                }
                keep
            });
        }
        for binding in released {
            release_binding(self.files.as_ref(), Some(binding)).await;
        }
    }

    /// 认证完成：记下本次协商结果（feature 门控的唯一来源）。
    fn note_features(&self, handle: &ConnectionHandle, features: &[String]) {
        let mut states = lock(&self.states);
        states
            .entry(handle.connection_id().as_str().to_owned())
            .or_default()
            .features = NegotiatedFeatures::new(features.to_vec());
    }

    /// 一条 `command` 帧的处理：解码 → 去重 → 限流 → 授权 → 查询/mutation。
    async fn on_command(&self, handle: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        self.reclaim(handle.connection_id().as_str()).await;
        let command = match decode::decode_command(message) {
            Ok(command) => command,
            Err(fault) => {
                debug!(
                    event = "sync.command_rejected",
                    device_id = handle.device_id().as_str(),
                    message_id = message.message_id().as_str(),
                    fault = fault.as_str(),
                    "the command frame was rejected before any local read"
                );
                self.reject_decode(handle, message, fault);
                return RouteOutcome::Claimed;
            }
        };
        // 连接期去重：同一连接内重复的同一帧不再产生第二次业务读取（§11.2 的查询类去重）。
        if let Some(result) = self.answered(handle, message.message_id(), &command.request_id) {
            debug!(
                event = "sync.command_duplicate",
                connection_id = handle.connection_id().as_str(),
                request_id = command.request_id.as_str(),
                "the same frame was already answered on this connection"
            );
            self.send_result(handle, &result, "duplicate");
            return RouteOutcome::Claimed;
        }
        if self.admit_rate(handle, command.command, message.message_id()) {
            return RouteOutcome::Claimed;
        }
        let Some(actor) = self.authorize(handle, &command).await else {
            return RouteOutcome::Claimed;
        };
        if command.command.is_query() {
            self.on_query(handle, message, command, &actor).await
        } else {
            self.on_mutation(handle, command, actor).await
        }
    }

    /// 查询命令：同步返回**一次** `completed` 或 `rejected`（绝不先回 `accepted`）。
    async fn on_query(
        &self,
        handle: &ConnectionHandle,
        message: &Envelope,
        command: Command,
        actor: &Actor,
    ) -> RouteOutcome {
        let features = self.features(handle);
        let outcome = match command.payload.clone() {
            CommandPayload::SessionList(_) => {
                query::session_list(&self.core, &self.owner, actor, &features).await
            }
            CommandPayload::SessionRead(payload) => {
                query::session_read(&self.core, actor, &command, payload, &features).await
            }
            CommandPayload::CommandStatus(payload) => {
                query::command_status(&self.core, actor, payload, &features).await
            }
            CommandPayload::ModeList(_) => query::mode_list(&self.core, actor, &command).await,
            CommandPayload::ConfigList(_) => {
                query::config_list(&self.core, &self.broker, actor, &command).await
            }
            // `workspace.browse` 的 feature 门控在 `query::workspace_browse` 的第一行：未协商时
            // 用例面（形状校验与目录读取）根本不会被调用（§11.5、spec `sync-workspace-browse`）。
            CommandPayload::WorkspaceBrowse(payload) => {
                query::workspace_browse(&self.core, actor, payload, &features).await
            }
            // 四条会话文件查询的 feature 门控都在 `query::*`/`Self::session_files_read` 的第一行：
            // 未协商时用例面（授权、会话读取、文件端口、Git 进程）一次都不会被调用。
            CommandPayload::SessionFilesList(payload) => {
                query::session_files_list(&self.core, actor, &command, payload, &features).await
            }
            CommandPayload::SessionFilesRead(payload) => {
                self.session_files_read(handle, actor, &command, payload, &features)
                    .await
            }
            CommandPayload::SessionGitStatus(payload) => {
                query::session_git_status(&self.core, actor, &command, payload, &features).await
            }
            CommandPayload::SessionGitDiff(payload) => {
                query::session_git_diff(&self.core, actor, &command, payload, &features).await
            }
            // 由 `CommandName::is_query` 与 `CommandPayload` 的判别一致性保证不可达；保留臂只为让判定
            // 穷尽，不 panic。
            _ => Err(Rejection::Schema(
                "the query payload does not match its command",
            )),
        };
        let result = match outcome {
            Ok(payload) => {
                let Some(accepted_at) = wire_timestamp(self.authority.now().as_str()) else {
                    return self.encoding_fault(handle);
                };
                CommandResult {
                    request_id: command.request_id.clone(),
                    command: command.command,
                    status: CommandState::Completed,
                    accepted_at: Nullable::value(accepted_at),
                    // 查询命令的 `terminalEventId` 固定为 `null`（§11.2 的 allOf）。
                    terminal_event_id: Nullable::null(),
                    result: Nullable::value(payload),
                    error: Nullable::null(),
                }
            }
            Err(rejection) => {
                let (code, text, details) = rejection.into_parts();
                let Some(error) = public_error(code, text, details) else {
                    return self.encoding_fault(handle);
                };
                debug!(
                    event = "sync.command_rejected",
                    device_id = handle.device_id().as_str(),
                    command = command.command.as_str(),
                    code = code.as_str(),
                    "the query command was rejected"
                );
                CommandResult {
                    request_id: command.request_id.clone(),
                    command: command.command,
                    status: CommandState::Rejected,
                    accepted_at: Nullable::null(),
                    terminal_event_id: Nullable::null(),
                    result: Nullable::null(),
                    error: Nullable::value(error),
                }
            }
        };
        // 结果超限：入队前按编码后字节数判定，超限即换成显式失败的终态（绝不截断后当完整结果返回）。
        let result = if result.status == CommandState::Completed && self.too_large(handle, &result)
        {
            warn!(
                event = "sync.command_result_too_large",
                connection_id = handle.connection_id().as_str(),
                request_id = result.request_id.as_str(),
                command = result.command.as_str(),
                "the query result exceeds the single message limit"
            );
            let Some(error) = public_error(
                ErrorCode::ResourceResultTooLarge,
                "the query result exceeds the single message limit",
                RawObject::empty(),
            ) else {
                return self.encoding_fault(handle);
            };
            CommandResult {
                request_id: command.request_id.clone(),
                command: command.command,
                status: CommandState::Rejected,
                accepted_at: Nullable::null(),
                terminal_event_id: Nullable::null(),
                result: Nullable::null(),
                error: Nullable::value(error),
            }
        } else {
            result
        };
        self.remember(handle, message.message_id(), result.clone());
        self.send_result(handle, &result, "query");
        RouteOutcome::Claimed
    }

    /// `session.files.read`：连接期内容版本的校验、替换与登记（§11.5）。
    ///
    /// 顺序（全部在文件端口调用之前）：
    ///
    /// 1. **feature 门控**：未协商 `core.session-files.v1` → `protocol.feature_required`，用例与端口零调用；
    /// 2. **带 `version` 的续读**：版本必须属于**本连接**且命中同一 `(设备, 会话, 路径)`，否则显式回
    ///    `internal.unavailable`——绝不把别人的/别的路径的版本当成可读凭据；
    /// 3. **不带 `version` 的首次读取**：先释放本连接既有的活动版本（同一连接只保留一个），再调用用例
    ///    建立新快照；成功后登记新版本。
    ///
    /// 续读失败（快照过期、文件变化等）时清掉本地绑定并返回端口映射的显式错误，要求客户端重新读取。
    async fn session_files_read(
        &self,
        handle: &ConnectionHandle,
        actor: &Actor,
        command: &Command,
        payload: SessionFilesRead,
        features: &NegotiatedFeatures,
    ) -> Result<CommandResultPayload, Rejection> {
        query::require_session_files(features)?;
        let session = query::command_session(command)?;
        let requested = payload
            .version
            .as_ref()
            .map(|version| version.as_str().to_owned());
        let path = payload.path.as_str().to_owned();
        match requested.as_deref() {
            Some(version) => {
                if !self.content_matches(handle, &session, &path, version) {
                    debug!(
                        event = "sync.session_files_version_rejected",
                        connection_id = handle.connection_id().as_str(),
                        "the content version does not belong to this connection, session and path"
                    );
                    return Err(Rejection::Code(
                        ErrorCode::InternalUnavailable,
                        "the content version is not valid for this connection",
                    ));
                }
            }
            None => {
                if let Some(previous) = self.take_content(handle) {
                    release_binding(self.files.as_ref(), Some(previous)).await;
                }
            }
        }
        let chunk =
            match query::session_files_read(&self.core, actor, command, &payload, features).await {
                Ok(chunk) => chunk,
                Err(error) => {
                    // 版本已不可用（过期/变化/撤销）：端口已自行废弃快照，这里清掉绑定，下一步必须重新
                    // 建立，避免客户端拿一个已死亡的版本反复续读。
                    if requested.is_some() {
                        if let Some(previous) = self.take_content(handle) {
                            release_binding(self.files.as_ref(), Some(previous)).await;
                        }
                    }
                    return Err(error);
                }
            };
        if requested.is_none() {
            self.bind_content(
                handle,
                ContentBinding {
                    device: handle.device_id().clone(),
                    session,
                    path: chunk.path().clone(),
                    version: chunk.version().clone(),
                },
            );
        }
        query::wire_session_file_chunk(&chunk)
            .ok_or(Rejection::Code(
                ErrorCode::InternalUnavailable,
                "the session file chunk cannot be projected",
            ))
            .map(CommandResultPayload::SessionFilesRead)
    }

    /// 带 `version` 的续读是否命中本连接唯一的活动版本（设备、会话、路径与版本全部相同）。
    fn content_matches(
        &self,
        handle: &ConnectionHandle,
        session: &SessionId,
        path: &str,
        version: &str,
    ) -> bool {
        lock(&self.states)
            .get(handle.connection_id().as_str())
            .and_then(|state| state.content.as_ref())
            .is_some_and(|binding| binding.matches(handle.device_id(), session, path, version))
    }

    /// 取出并清空本连接的活动内容版本（调用方负责在适当时机释放端口快照）。
    fn take_content(&self, handle: &ConnectionHandle) -> Option<ContentBinding> {
        lock(&self.states)
            .get_mut(handle.connection_id().as_str())
            .and_then(|state| state.content.take())
    }

    /// 登记本连接新的活动内容版本（此前已由 [`Self::take_content`] 清空）。
    fn bind_content(&self, handle: &ConnectionHandle, binding: ContentBinding) {
        lock(&self.states)
            .entry(handle.connection_id().as_str().to_owned())
            .or_default()
            .content = Some(binding);
    }

    /// mutation 的接受路径（WP3 的 3.1）：**先持久接受、回 `accepted`，绝不在同一响应里给业务结果**。
    ///
    /// 顺序与查询面同一套骨架（`on_command` → `authorize` → 业务），差别只在：
    ///
    /// - [`Self::authorize`] 已经做过一次授权，这里不重复；拒绝路径与该函数统一；
    /// - `(deviceId, requestId)` 的幂等由 **core 的持久记录**承担（`Broker::submit_mutation` 的幂等键），
    ///   本层只把 `request_fingerprint`（解码后 payload 的 ACPR-CJ1 摘要）交上去 —— 不另建内存幂等表，
    ///   否则重启后同一 `requestId` 会被当成新命令再派发一次；
    /// - 首次提交返回 core 的收据：已终结即回终态（同键重试的回复因此与首次同形），否则回 `accepted`
    ///   并挂终态观察（`terminal_event_id` 恒 `null`，终态只经持久 `command.*` 事件表达）。
    ///
    /// `session.create` **不在本路径**：core 有独立入口（`UseCases::create_session`），它的 workspace 别名
    /// 解析与 `remoteSessionRef` 生成不经过 `ClientCommand`。WP3 的后续迭代再接；当前按显式拒绝处理，
    /// 不伪装成已受理。
    async fn on_mutation(
        &self,
        handle: &ConnectionHandle,
        command: Command,
        actor: Actor,
    ) -> RouteOutcome {
        // 授权已在 `on_command` 完成（查询与 mutation 走同一次判定），这里不重复判一次、不重复写审计。
        // `session.create` 有独立的 core 入口（`UseCases::create_session`）：它不吃 `ClientCommand`。
        if command.command == CommandName::SessionCreate {
            return self.on_session_create(handle, &actor, command).await;
        }
        if command.command == CommandName::WorkspaceMkdir {
            return self.on_workspace_mkdir(handle, &actor, command).await;
        }
        let Some(payload) = core_payload(&command.payload).ok() else {
            self.deny(
                handle,
                &command,
                "the payload does not match the command",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        };
        // §11.5/§11.7：`requestVersion` 与 `submissionId` 是 mutation 的两个可恢复入口。两者都在
        // `core_payload` 之外处理：前者改变提交入口（条件更新），后者改变整批附件的原子绑定入口；
        // 把它们塞进 `ClientCommand.payload` 会让指纹与幂等语义偏离 wire payload 的原文。
        let request_version = match wire_request_version(&command.payload) {
            Ok(version) => version,
            Err(reason) => {
                self.deny(handle, &command, reason, ErrorCode::ProtocolSchemaInvalid);
                return RouteOutcome::Claimed;
            }
        };
        if request_version.is_none()
            && matches!(
                command.payload,
                CommandPayload::PermissionResolve(_) | CommandPayload::ElicitationRespond(_)
            )
            && self
                .features(handle)
                .contains(crate::sync::command::query::PENDING_INTERACTIONS_FEATURE)
        {
            // 协商了 `core.pending-interactions.v1` 就必须回传版本：否则无法证明应答针对的是当前请求。
            self.deny(
                handle,
                &command,
                "requestVersion is required once core.pending-interactions.v1 is negotiated",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        }
        let submission = match wire_submission(&command.payload) {
            Ok(submission) => submission,
            Err(reason) => {
                self.deny(handle, &command, reason, ErrorCode::ProtocolSchemaInvalid);
                return RouteOutcome::Claimed;
            }
        };
        let Some(fingerprint) = payload_fingerprint(&command.payload) else {
            return self.encoding_fault(handle);
        };
        let Some(request_id) = core_request_id(&command.request_id) else {
            return self.encoding_fault(handle);
        };
        let target_session = command
            .session_id
            .as_ref()
            .and_then(|session| acp_core::model::SessionId::new(session.as_str()).ok());
        // 附件引用（§11.5）在**任何派发之前**判定：`attachment_ref` 只在协商 `core.attachment-transfer.v1`
        // 后被接受，且必须携带已封存的 `submissionId`、逐块绑定到该清单。这里拒绝的请求既不触碰存储的
        // 业务行，也不会派发 Agent。
        if let Some(outcome) = self
            .check_prompt_attachments(
                handle,
                &command,
                &actor,
                target_session.as_ref(),
                &submission,
            )
            .await
        {
            return outcome;
        }
        let client_command = ClientCommand {
            actor: actor.clone(),
            request: request_id.clone(),
            command: command.command.as_str().to_owned(),
            kind: CommandKind::Mutation,
            session: target_session.clone(),
            expected_version: command
                .expected_version
                .as_ref()
                .and_then(|version| version.as_str().parse::<u64>().ok())
                .map(Version::new),
            request_fingerprint: fingerprint.clone(),
            payload,
        };
        // 3.2/3.3：幂等键 `(actor, requestId)` 的持久事实与冲突判定都在 core（`Broker::idempotency`
        // 读 `owned_command`、比命令名/kind/session/expectedVersion/ACPR-CJ1 指纹）。服务端**不**在
        // 提交前另查一次 `command.status`：那需要 `grant.observe`，会把只持 `grant.interact` 的
        // 设备误拒——而「重放自己的命令」只应凭当初提交它的那份授权。
        // 整批附件：受理前复核清单（存在、会话、主体），再随受理提交放进**同一事务**（§3.4）。
        // `submissionId` 只出现在 `session.prompt` 上：先取出正文，避免把 `client_command` 移进 core 时
        // 仍被 `&payload` 借用。
        let prompt_content = if submission.is_some() {
            let CorePayload::Prompt { content } = &client_command.payload else {
                // 其他命令带 `submissionId` 属于形状错误。
                self.deny(
                    handle,
                    &command,
                    "submissionId is only accepted on session.prompt",
                    ErrorCode::ProtocolSchemaInvalid,
                );
                return RouteOutcome::Claimed;
            };
            Some(content.clone())
        } else {
            None
        };
        let receipt = match (&submission, request_version) {
            (Some(submission), _) => {
                let content = prompt_content.expect("a submission implies a prompt payload");
                self.core
                    .submit_prompt_with_submission(
                        &actor,
                        client_command,
                        content,
                        submission.clone(),
                    )
                    .await
            }
            // 版本化应答（§11.7）：版本不符或已处理都回显式错误，且不消费请求。
            (None, Some(version)) => {
                self.core
                    .submit_command_versioned(&actor, client_command, Some(version))
                    .await
            }
            (None, None) => self.core.submit_command(&actor, client_command).await,
        };
        let receipt = match receipt {
            Ok(receipt) => receipt,
            Err(error) => {
                // 「带了版本但版本不对」必须回 `state.version_conflict` 并给**当前真实版本**（§11.5 的
                // `session.mode.set`/`session.config.set`）：这条细节 `PortError` 不携带，因此在这里
                // 按会话读一次（`Broker::session_version` 不再授权，命令本身已授权过）。
                let rejection = match (&error, target_session.as_ref()) {
                    (PortError::Conflict(ConflictKind::VersionMismatch), Some(session)) => {
                        match self.broker.session_version(session).await {
                            Ok(current) => Rejection::VersionConflict(current.get()),
                            Err(_) => port_rejection(&error),
                        }
                    }
                    _ => port_rejection(&error),
                };
                let (code, text, details) = rejection.into_parts();
                let Some(error) = public_error(code, text, details) else {
                    return self.encoding_fault(handle);
                };
                self.send_rejected(handle, &command, error);
                return RouteOutcome::Claimed;
            }
        };
        let turn = match receipt {
            CommandReceipt::Rejected { error } => {
                // core 的 `PublicError`：转成 wire 的同形错误体（消息与 details 原样）。
                let Some(error) = wire_public_error(&error) else {
                    return self.encoding_fault(handle);
                };
                self.send_rejected(handle, &command, error);
                return RouteOutcome::Claimed;
            }
            CommandReceipt::Accepted { turn, .. } => turn,
        };
        // 提交后按 `(actor, requestId)` 读回持久记录：幂等重放一条**已终结**的命令时，
        // §11.2 要求 `command.result` 回当前终态与原 `terminalEventId`（不重新派发）。首次提交时
        // 记录刚写成 `accepted`，走的也是同一分支——因此两种情形共用一条回复路径。
        match self.broker.command_status(&actor, &request_id).await {
            Ok(Some(record)) if record.status().is_terminal() => {
                self.send_record_terminal(handle, &command.request_id, command.command, &record);
            }
            Ok(Some(record)) => {
                self.send_accepted(handle, &command, turn, record.accepted_at());
                self.watch_terminal(handle, &command, &actor);
            }
            // 提交成功却不留记录：存储与用例不一致，按接线缺陷显式失败（不发半成品回复）。
            Ok(None) | Err(_) => {
                self.encoding_fault(handle);
            }
        }
        RouteOutcome::Claimed
    }

    /// `session.prompt` 的附件前置判定（§11.5）：返回 `Some(_)` 表示已回拒绝、调用方必须停止。
    ///
    /// 三道判定与 schema 的声明对齐（「`attachment_ref` 只在协商 `core.attachment-transfer.v1` 且附件已
    /// 封存、已绑定本会话/主体时接受」）：
    ///
    /// 1. 出现 `attachment_ref`（或 `submissionId`）时，本连接必须协商附件 feature——没有协商就说明客户端
    ///    不可能是那个清单的作者，回 `attachment.negotiation_required`；
    /// 2. `attachment_ref` 必须携带已封存的 `submissionId`——清单绑定是引用的唯一凭据，缺了就回
    ///    `protocol.schema_invalid`；
    /// 3. 清单本身必须已封存、属于本会话/本主体；每个引用的 `attachmentId`/`sha256`/`byteLength` 必须
    ///    与清单逐块一致（[`verify_prompt_bindings`]），否则回 `attachment.binding_mismatch`。
    ///
    /// 只有 `submissionId`（全部是 `text` 块）也走第 3 条的清单校验（绑定列表为空）：未封存或
    /// 跨会话/主体的清单必须在派发前显式拒绝，而不是落到 core 的通用参数错误上。校验通过后 core 的
    /// `submit_prompt_with_submission` 会把整批附件与消息放进同一事务。
    async fn check_prompt_attachments(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        actor: &Actor,
        session: Option<&SessionId>,
        submission: &Option<SubmissionId>,
    ) -> Option<RouteOutcome> {
        let Ok(bindings) = prompt_attachments(&command.payload) else {
            self.deny(
                handle,
                command,
                "the prompt content blocks cannot be used",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return Some(RouteOutcome::Claimed);
        };
        let bound = !bindings.is_empty() || submission.is_some();
        if !bound {
            return None;
        }
        if !self.features(handle).contains(ATTACHMENT_FEATURE) {
            self.deny(
                handle,
                command,
                "the connection did not negotiate core.attachment-transfer.v1",
                ErrorCode::AttachmentNegotiationRequired,
            );
            return Some(RouteOutcome::Claimed);
        }
        let Some(submission) = submission.as_ref() else {
            self.deny(
                handle,
                command,
                "an attachment_ref requires a sealed submissionId",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return Some(RouteOutcome::Claimed);
        };
        let Some(session) = session else {
            self.deny(
                handle,
                command,
                "session.prompt requires sessionId",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return Some(RouteOutcome::Claimed);
        };
        let Some(subject) = crate::attachment_transfer::Subject::from_actor(actor) else {
            self.deny(
                handle,
                command,
                "the connection identity is not an attachment subject",
                ErrorCode::InternalUnavailable,
            );
            return Some(RouteOutcome::Claimed);
        };
        let Some(store) = self.core.attachment_transfers().cloned() else {
            // 没有装配附件端口却协商了 feature：接线错误，失败关闭。
            self.deny(
                handle,
                command,
                "the attachment store is not assembled",
                ErrorCode::InternalUnavailable,
            );
            return Some(RouteOutcome::Claimed);
        };
        match verify_prompt_bindings(&store, &subject, session, submission, &bindings).await {
            Ok(()) => None,
            Err(AttachmentFault::BindingMismatch) => {
                self.deny(
                    handle,
                    command,
                    "the attachment reference is not bound to this submission",
                    ErrorCode::AttachmentBindingMismatch,
                );
                Some(RouteOutcome::Claimed)
            }
            Err(_) => {
                self.deny(
                    handle,
                    command,
                    "the attachment binding cannot be verified",
                    ErrorCode::InternalUnavailable,
                );
                Some(RouteOutcome::Claimed)
            }
        }
    }

    /// 无会话的目录 mutation：持久接受后执行，并只回复持久记录里的终态。
    async fn on_workspace_mkdir(
        &self,
        handle: &ConnectionHandle,
        actor: &Actor,
        command: Command,
    ) -> RouteOutcome {
        if !self.features(handle).contains(WORKSPACE_MKDIR_FEATURE) {
            let Some(error) = public_error(
                ErrorCode::ProtocolFeatureRequired,
                "the client did not negotiate the workspace-mkdir feature",
                details_object(&serde_json::json!({ "features": [WORKSPACE_MKDIR_FEATURE] })),
            ) else {
                return self.encoding_fault(handle);
            };
            self.send_rejected(handle, &command, error);
            return RouteOutcome::Claimed;
        }
        let CommandPayload::WorkspaceMkdir(payload) = &command.payload else {
            self.deny(
                handle,
                &command,
                "the payload does not match the command",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        };
        let Some(fingerprint) = payload_fingerprint(&command.payload) else {
            return self.encoding_fault(handle);
        };
        let Some(request_id) = core_request_id(&command.request_id) else {
            return self.encoding_fault(handle);
        };
        // Reserve bounded execution capacity before durable acceptance. A full or stopped
        // executor rejects without filesystem writes or a stranded acceptance record.
        let permit = match self.mkdir_queue.try_reserve() {
            Ok(permit) => permit,
            Err(_) => {
                self.deny(
                    handle,
                    &command,
                    "directory creation capacity is unavailable",
                    ErrorCode::ResourceBackpressure,
                );
                return RouteOutcome::Claimed;
            }
        };
        let execution = match self
            .core
            .admit_workspace_mkdir(
                actor,
                &request_id,
                &fingerprint,
                payload.parent_path.as_str(),
                payload.name.as_str(),
            )
            .await
        {
            Ok(acp_core::ports::WorkspaceMkdirAdmission::Accepted(execution)) => execution,
            Ok(acp_core::ports::WorkspaceMkdirAdmission::InFlight) => {
                let accepted_at = self
                    .broker
                    .command_status(actor, &request_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|record| record.accepted_at().cloned());
                self.send_accepted(handle, &command, None, accepted_at.as_ref());
                self.watch_terminal(handle, &command, actor);
                return RouteOutcome::Claimed;
            }
            Ok(acp_core::ports::WorkspaceMkdirAdmission::Settled(record)) => {
                self.send_record_terminal(handle, &command.request_id, command.command, &record);
                return RouteOutcome::Claimed;
            }
            Err(error) => {
                self.reply_after_create_failure(handle, actor, &command, &request_id, &error)
                    .await;
                return RouteOutcome::Claimed;
            }
        };
        let accepted_at = self
            .broker
            .command_status(actor, &request_id)
            .await
            .ok()
            .flatten()
            .and_then(|record| record.accepted_at().cloned());
        self.send_accepted(handle, &command, None, accepted_at.as_ref());
        self.watch_terminal(handle, &command, actor);
        permit.send(execution);
        RouteOutcome::Claimed
    }

    /// `session.create`（§11.5 的 Sync 面形状：payload 的目录来源是 `workspaceAlias` 或客户端提供的
    /// `workspacePath` 二者取一，加必填 `agentId`，以及仅在路径来源下允许的可选 `createIfMissing`
    /// 建目录开关）。
    ///
    /// 与其余 mutation 的差别只在入口：core 用独立用例（[`UseCases::create_session`]）而不是
    /// `ClientCommand`（别名解析与 owned 会话行由它负责）。**接受与终态语义完全相同**。
    ///
    /// 顺序上有一处**与其余 mutation 相反**、且是刻意的：这里先创建、再按需要回 `accepted`。原因是
    /// 「已接受」对 `session.create` 的语义是「该命令的**受理已经落盘**」（受理只落幂等记录，会话行
    /// 随落账提交里的终态在**同一次提交**里落盘，§6 第 20 条）：成功时先回 `accepted` 再等观察者推
    /// 终态；失败时先回 `accepted`（取记录里的首次持久接受时间）、再回 core 写定的终态——客户端因此
    /// 总能观察到「`accepted` → 终态」，而不是凭空收到一条终态。失败时的回包判据见
    /// [`Self::reply_after_create_failure`]（可证明未产生会话 ⇒ `failed`、说不清 ⇒ `uncertain`，都由
    /// core 写进持久终态；只有「受理之前就失败」那一档才是 `rejected`）。
    async fn on_session_create(
        &self,
        handle: &ConnectionHandle,
        actor: &Actor,
        command: Command,
    ) -> RouteOutcome {
        // feature 门控（§11.5）：未协商 `core.session-create.v1` 时不静默接受。
        if !self.features(handle).contains(SESSION_CREATE_FEATURE) {
            let Some(error) = public_error(
                ErrorCode::ProtocolFeatureRequired,
                "the client did not negotiate the session-create feature",
                details_object(&serde_json::json!({ "features": [SESSION_CREATE_FEATURE] })),
            ) else {
                return self.encoding_fault(handle);
            };
            self.send_rejected(handle, &command, error);
            return RouteOutcome::Claimed;
        }
        let CommandPayload::SessionCreate(create) = &command.payload else {
            self.deny(
                handle,
                &command,
                "the payload does not match the command",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        };
        let Some(fingerprint) = payload_fingerprint(&command.payload) else {
            return self.encoding_fault(handle);
        };
        let Some(request_id) = core_request_id(&command.request_id) else {
            return self.encoding_fault(handle);
        };
        let Ok(agent_id) = acp_core::model::AgentId::new(create.agent_id.as_str()) else {
            self.deny(
                handle,
                &command,
                "the agentId is out of range",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        };
        let Ok(agent) = acp_core::model::AgentRef::try_new(agent_id, create.agent_id.as_str())
        else {
            self.deny(
                handle,
                &command,
                "the agentId is out of range",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        };
        // 目录来源是 `directoryId` / `workspacePath` 的二选一（`SessionCreateWorkspace` 已在解码层
        // 钉死"恰好一个"）。标识方向的形状校验在这里做（不触碰文件系统）；路径方向的形状与文件系统
        // 校验都在 core 的解析路径里（先形状、后 IO），因此分类与该用例同源。
        let workspace = match &create.workspace {
            SessionCreateWorkspace::Directory(directory_id) => {
                // `createIfMissing` 与目录标识**互斥**（`sync-session-create`：已记录的目录必然已存在），
                // wire 的 schema 已用 `not` 在解码期拒掉这种组合。这里再核一次不变量：`SessionCreate` 的
                // 字段是公开的，构造侧（或将来多一条入口）把两者凑在一起时 MUST NOT 被当成合法输入。
                if create.create_if_missing.is_some() {
                    self.deny(
                        handle,
                        &command,
                        "createIfMissing is not allowed with directoryId",
                        ErrorCode::ProtocolSchemaInvalid,
                    );
                    return RouteOutcome::Claimed;
                }
                let Ok(directory_id) = acp_core::model::DirectoryId::new(directory_id.as_str())
                else {
                    self.deny(
                        handle,
                        &command,
                        "the directoryId is not a directory id",
                        ErrorCode::ProtocolSchemaInvalid,
                    );
                    return RouteOutcome::Claimed;
                };
                WorkspaceSource::Directory(directory_id)
            }
            // `createIfMissing` 缺席即假（行为与引入该键之前完全一致）：缺了开关的路径来源要求该目录
            // 已存在；显式 `true` 才走「先创建（连同缺失的父层）再启动 Agent」那一支。两种来源的解析与
            // 失败分类都在 core 的同一条形状校验之上（先形状、后 IO），因此本层只选来源，不自己判 IO。
            SessionCreateWorkspace::Path(path) => match create.create_if_missing {
                Some(true) => WorkspaceSource::PathWithCreate(path.as_str().to_owned()),
                _ => WorkspaceSource::Path(path.as_str().to_owned()),
            },
        };
        let request = acp_core::model::CreateSessionRequest::new(
            agent,
            None,
            None,
            acp_core::model::ResourceOrigin::Local,
        );
        let prepared = match self
            .core
            .create_session(
                actor,
                request_id.clone(),
                fingerprint,
                request,
                Some(workspace),
            )
            .await
        {
            Ok(acp_core::ports::SessionCreateAdmission::Prepared(prepared)) => prepared,
            Ok(acp_core::ports::SessionCreateAdmission::InFlight) => {
                // 同键、首次创建仍在窗口期（并发重发）：**不**派发第二次创建（§11.5 的「同一稳定
                // requestId 重复提交不产生第二个会话」）。终态由首次受理那一次的结算写入，这里与其余
                // mutation 的「已 accepted」分支同形：回受理态并挂终态观察。
                //
                // `acceptedAt` 取**首次持久接受**的时间（§11.2：`rejected` 之外的所有状态都必须给出该
                // 时间）：`InFlight` 只可能在该 `requestId` 已有 `accepted` 记录时返回，因此那笔时间是可
                // 读的；读不回来时退回本机时钟（与其余 mutation 的已接受分支同款兜底），不伪造首见时间。
                let accepted_at = match self.broker.command_status(actor, &request_id).await {
                    Ok(Some(record)) => record.accepted_at().cloned(),
                    _ => None,
                };
                self.send_accepted(handle, &command, None, accepted_at.as_ref());
                self.watch_terminal(handle, &command, actor);
                return RouteOutcome::Claimed;
            }
            Ok(acp_core::ports::SessionCreateAdmission::Settled(record)) => {
                // 同键且首次已经终结：回首次结果，不覆盖、不重建（与其余 mutation 的已终结分支同形）。
                self.send_record_terminal(handle, &command.request_id, command.command, &record);
                return RouteOutcome::Claimed;
            }
            Err(error) => {
                // 失败分类在 core（`Unavailable(SessionNotEstablished)` ⇒ `failed`，其余 ⇒ `uncertain`），
                // 且后端失败时 core **已经**写好终态（无会话形状）再把原错误上抛：本层不再自己分类，
                // 也不再以「有没有受理记录」当副作用证据（受理记录在受理阶段就已存在，用它当替身会把
                // 可证明的失败误报成 `uncertain`）。回包一律见 [`Self::reply_after_create_failure`]。
                self.reply_after_create_failure(handle, actor, &command, &request_id, &error)
                    .await;
                return RouteOutcome::Claimed;
            }
        };
        // 创建已落盘（含幂等行）：现在回 `accepted` 才是如实的。
        self.send_accepted(handle, &command, None, None);
        // 终态：把持久记录推进到 `completed { sessionId, session }`。摘要与 `session.list` **同源**
        // （同一个 `wire_session` 投影），因此两侧形状必然一致。
        let catalog = self.features(handle).contains(LOCAL_CATALOG_FEATURE);
        let settled = match self
            .created_session_summary(actor, &prepared, catalog)
            .await
        {
            Some(summary) => {
                match Uuid::parse(prepared.session.as_str())
                    .ok()
                    .map(|session_id| SessionCreateResult {
                        session_id,
                        session: summary,
                    })
                    .and_then(|result| serde_json::to_string(&result).ok())
                    .and_then(|text| acp_core::model::CommandResult::from_json_text(&text).ok())
                {
                    Some(result) => {
                        self.core
                            .settle_session_create(
                                actor,
                                &request_id,
                                acp_core::model::CommandStatus::Completed,
                                Some(result),
                                None,
                            )
                            .await
                    }
                    None => Ok(false),
                }
            }
            None => Ok(false),
        };
        if !matches!(settled, Ok(true)) {
            // 会话已创建，但摘要投影不出来（或那条终态没落盘）：**不得**回 `failed`（那会让客户端以为
            // 没有会话）。§11.5 对这一档的定论是 `uncertain`。终态的成因只写封闭类别：`PortError` 的
            // `Debug` 可能带出后端文本（含路径），不写进日志。
            warn!(
                event = "sync.session_create_terminal_unsettled",
                device_id = handle.device_id().as_str(),
                command = command.command.as_str(),
                session_id = prepared.session.as_str(),
                request_id = request_id.as_str(),
                cause = settled
                    .as_ref()
                    .err()
                    .map_or(Cause::Projection, Cause::of_port_error)
                    .as_str(),
                "the created session cannot be projected into the terminal result"
            );
            let Some(error) = acp_core::model::PublicError::coded(
                "internal.unavailable",
                "the created session cannot be projected into the terminal result",
                false,
            )
            .ok() else {
                return self.encoding_fault(handle);
            };
            let _ = self
                .core
                .settle_session_create(
                    actor,
                    &request_id,
                    acp_core::model::CommandStatus::Uncertain,
                    None,
                    Some(error),
                )
                .await;
        }
        self.watch_terminal(handle, &command, actor);
        RouteOutcome::Claimed
    }

    /// `session.create` 失败后的回包：**以持久记录为唯一权威**。
    ///
    /// core 已按错误类别写好终态（[`acp_core::broker::Broker::create_session`]：可证明 Agent 侧未产生
    /// 会话 ⇒ `failed`，说不清 ⇒ `uncertain`，两者都是**零会话行**的形状）并原样上抛后端错误，因此本层
    /// 的责任只有「按持久记录回包」这一件：
    ///
    /// - **有记录且已终结** ⇒ 回首次终态（`terminalEventId` 非空）。同 `requestId` 重发因此既不会被回成
    ///   `rejected`（那是「未产生任何副作用」的宣告），也不会被当成一条新命令；
    /// - **有记录但仍是 `accepted`**（core 的失败终态没落盘，例如存储瞬断）⇒ 回受理态并挂终态观察，
    ///   **不伪造终态**：记录是唯一事实，它由启动恢复按 §6 第 16 条收敛到 `uncertain`；
    /// - **没有记录**（受理提交之前就失败：授权拒绝、schema/参数拒绝、别名未登记、本机解析失败，或受理
    ///   提交本身写失败）⇒ 保留今天的行为：`port_rejection` 的 `rejected`。这一档**可以**被证明「本机
    ///   没有任何副作用」，主 spec「引用失败与解析失败的分类」要求以 `rejected` 表达；
    /// - **记录读不回来**（存储不可用）⇒ 不得回 `rejected`：它断言「本机没有副作用」，而此刻恰恰不知道
    ///   有没有。回受理态并挂观察，对端可凭 `command.status` 自行确认。
    ///
    /// 一处例外：**幂等冲突**（同键不同语义）先判——那时确实有一条记录，但它的首次结果属于另一条语义的
    /// 命令，重放它会让对端以为这次的不同 payload 被执行过。只有这一档「已有记录也回 `rejected`」是
    /// 如实的，且主 spec 的幂等要求指定了它的错误码。
    async fn reply_after_create_failure(
        &self,
        handle: &ConnectionHandle,
        actor: &Actor,
        command: &Command,
        request_id: &CoreRequestId,
        error: &PortError,
    ) {
        if matches!(
            error,
            PortError::Conflict(ConflictKind::IdempotencyConflict)
        ) {
            match public_error(
                ErrorCode::CommandIdempotencyConflict,
                "the requestId was already used for a different command or payload",
                RawObject::empty(),
            ) {
                Some(error) => self.send_rejected(handle, command, error),
                None => {
                    self.encoding_fault(handle);
                }
            }
            return;
        }
        match self.broker.command_status(actor, request_id).await {
            Ok(Some(record)) if record.status().is_terminal() => {
                // 首次响应必须是**受理态**：这条命令确实被受理过（受理提交在本请求里先落盘，后端紧接着
                // 失败、终态也已经写好）。先回 `accepted`（取记录里的首次受理时间），再回该终态——与
                // Node Link 面同形，客户端因此总能观察到「`accepted` → 终态」，而不是凭空收到一条终态
                // （`SYNC_PROTOCOL.md` §11.2：mutation 首次提交只返回 `accepted` 或 `rejected`）。
                // 受理日志由 `send_accepted` 记，不再另记一条。
                self.send_accepted(handle, command, None, record.accepted_at());
                self.send_record_terminal(handle, &command.request_id, command.command, &record);
            }
            Ok(Some(record)) => {
                self.send_accepted(handle, command, None, record.accepted_at());
                self.watch_terminal(handle, command, actor);
            }
            Ok(None) => self.reject_from_port_error(handle, command, error),
            Err(read_error) => {
                warn!(
                    event = "sync.session_create_status_unreadable",
                    device_id = handle.device_id().as_str(),
                    command = command.command.as_str(),
                    request_id = command.request_id.as_str(),
                    cause = Cause::of_port_error(&read_error).as_str(),
                    "the command record could not be read; the reply stays non-committal"
                );
                self.send_accepted(handle, command, None, None);
                self.watch_terminal(handle, command, actor);
            }
        }
    }

    /// 端口错误 → `rejected`（`port_rejection` 的映射 + 错误体装配）。
    fn reject_from_port_error(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        error: &PortError,
    ) {
        let (code, text, details) = port_rejection(error).into_parts();
        match public_error(code, text, details) {
            Some(error) => self.send_rejected(handle, command, error),
            None => {
                self.encoding_fault(handle);
            }
        }
    }

    /// 创建终态的 wire 摘要（与 `session.list` 同一条可见性与投影口径）。
    ///
    /// 摘要取自 core 随 `Prepared` 给出的那一份：会话行要到**落账提交**（`settle_session_create`）才
    /// 存在（§6 第 20 条），此前会话读面里必然还没有它——按读面取摘要会让每次创建都降级为
    /// `uncertain`；而 core 的摘要与落账提交将写入的取值同源，投影又复用 `session.list` 的
    /// `wire_session`，因此两侧形状必然一致。
    ///
    /// `session.list` 这一读保留原职：它是「本主体能不能读会话读面」这一判定的落点（`list_sessions`
    /// 先授权、再读行），未授权时返回 `None`、由 §11.5 的 `uncertain` 收口，不伪造摘要。
    async fn created_session_summary(
        &self,
        actor: &Actor,
        prepared: &acp_core::ports::PreparedSessionCreate,
        catalog: bool,
    ) -> Option<WireSessionSummary> {
        // 取值本身不使用：窗口期列表里还没有这条会话，本调用只落实那次授权判定。
        self.core
            .list_sessions(actor, acp_core::ports::SessionQuery::default())
            .await
            .ok()?;
        self::query::wire_session(&prepared.summary, catalog)
    }

    /// 回一条 `accepted` 的 `command.result`：`result` 与 `terminalEventId` 均为 `null`。
    fn send_accepted(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        turn: Option<TurnId>,
        accepted_at: Option<&CoreTimestamp>,
    ) {
        // 受理日志（`sync-command-handling` 的「命令级结构化日志」）：主体、命令名、资源标识与请求标识，
        // 字段名与成因/终态词表集中在 `crate::command_log`。`session.create` 的受理阶段还没有会话行，
        // 因此 `sessionId` 为 `null`（wire 的 `sessionId` 就是那一档的证据）。
        command_log::accepted(
            Subject::Device(handle.device_id().as_str()),
            command.command.as_str(),
            command
                .session_id
                .as_ref()
                .and_then(|id| acp_core::model::SessionId::new(id.as_str()).ok())
                .as_ref(),
            command.request_id.as_str(),
            turn.as_ref(),
        );
        // 首见提交用本机时钟；幂等重放用首次落盘的那个 acceptedAt（不得换成现在）。
        // 两个时钟类型不同源（`Authority` 与 core 各自持有），因此以 ISO 文本为公共面。
        let stamp = match accepted_at {
            Some(stamp) => stamp.as_str().to_owned(),
            None => self.authority.now().as_str().to_owned(),
        };
        let Some(accepted_at) = wire_timestamp(&stamp) else {
            self.encoding_fault(handle);
            return;
        };
        self.send_result(
            handle,
            &CommandResult {
                request_id: command.request_id.clone(),
                command: command.command,
                status: CommandState::Accepted,
                accepted_at: Nullable::value(accepted_at),
                terminal_event_id: Nullable::null(),
                result: Nullable::null(),
                error: Nullable::null(),
            },
            "mutation",
        );
    }

    /// 一条**已终结**的持久记录（或幂等重放）→ `command.result`（§11.2）。
    ///
    /// 三处不能省：`status` 是终态本身；`acceptedAt` 必须是**首次**落盘的接受时间（只有 `rejected`
    /// 才允许 `null`）；重复查询一个已终结的 mutation 时 `terminalEventId` **必填** —— 它就是该
    /// request 唯一的 command terminal event 的 `eventId`。`result` 从记录原文还原（`session.create`
    /// 的 `completed` 因此能给出首次的 `{sessionId, session}`，而不是重新推导）。
    fn send_record_terminal(
        &self,
        handle: &ConnectionHandle,
        request_id: &Uuid,
        name: CommandName,
        record: &acp_core::model::CommandRecord,
    ) {
        let status = wire_command_state(record.status());
        // 终态日志（与受理那条成对）：类别取自持久记录本身，成因由记录里的错误码经封闭词表收敛
        // （`accepted`/`rejected` 不是**已接受**命令的终态，因此不在这里记）。
        if let Some(class) = terminal_class(status) {
            command_log::terminal(
                Subject::Device(handle.device_id().as_str()),
                name.as_str(),
                record.session(),
                request_id.as_str(),
                class,
                record
                    .error()
                    .map(|error| Cause::of_code(error.code()))
                    .unwrap_or(Cause::None),
            );
        }
        let accepted_at = match record.accepted_at() {
            Some(at) => match wire_timestamp(at.as_str()) {
                Some(at) => Nullable::value(at),
                None => {
                    self.encoding_fault(handle);
                    return;
                }
            },
            // `rejected` 之外的记录都该有接受时间；缺了说明记录不完整，按接线缺陷显式失败。
            None if status == CommandState::Rejected => Nullable::null(),
            None => {
                self.encoding_fault(handle);
                return;
            }
        };
        let terminal_event_id = match record.terminal_event() {
            Some(event) => match Uuid::parse(event.as_str()) {
                Ok(event) => Nullable::value(event),
                Err(_) => {
                    self.encoding_fault(handle);
                    return;
                }
            },
            None => Nullable::null(),
        };
        let error = match record.error() {
            Some(error) => match wire_public_error(error) {
                Some(error) => Nullable::value(error),
                None => {
                    self.encoding_fault(handle);
                    return;
                }
            },
            None => Nullable::null(),
        };
        // 记录里的 `result` 原文 → wire 结果：`session.create` 的 `completed` 有具名形状，其余按
        // 开放对象承载（§11.5 的表：只有查询命令与 `session.create` 规定具名结果）。
        let result = match record.result() {
            Some(result) if name == CommandName::WorkspaceMkdir => {
                match serde_json::from_str::<sync_protocol::command::WorkspaceMkdirResult>(
                    result.as_str(),
                ) {
                    Ok(value) => Nullable::value(CommandResultPayload::WorkspaceMkdir(value)),
                    Err(_) => {
                        self.encoding_fault(handle);
                        return;
                    }
                }
            }
            Some(result) if name == CommandName::SessionCreate => {
                match serde_json::from_str::<SessionCreateResult>(result.as_str()) {
                    Ok(value) => {
                        Nullable::value(CommandResultPayload::SessionCreate(Box::new(value)))
                    }
                    Err(_) => {
                        self.encoding_fault(handle);
                        return;
                    }
                }
            }
            Some(result) => match RawObject::parse(result.as_str()) {
                Ok(value) => Nullable::value(CommandResultPayload::Object(value)),
                Err(_) => {
                    self.encoding_fault(handle);
                    return;
                }
            },
            None => Nullable::null(),
        };
        self.send_result(
            handle,
            &CommandResult {
                request_id: request_id.clone(),
                command: name,
                status,
                accepted_at,
                terminal_event_id,
                result,
                error,
            },
            "mutation",
        );
    }

    /// 一条 `rejected` 的 `command.result`：`acceptedAt`/`terminalEventId`/`result` 均为 `null`，
    /// `error` 携带结构化原因（§11.2 对 `rejected` 的取值约束）。
    fn send_rejected(&self, handle: &ConnectionHandle, command: &Command, error: PublicError) {
        debug!(
            event = "sync.command_rejected",
            device_id = handle.device_id().as_str(),
            command = command.command.as_str(),
            request_id = command.request_id.as_str(),
            code = error.code.as_str(),
            "the mutation was rejected without any side effect"
        );
        self.send_result(
            handle,
            &CommandResult {
                request_id: command.request_id.clone(),
                command: command.command,
                status: CommandState::Rejected,
                accepted_at: Nullable::null(),
                terminal_event_id: Nullable::null(),
                result: Nullable::null(),
                error: Nullable::value(error),
            },
            "mutation",
        );
    }

    /// 3.5：把该命令挂上终态观察者（提交方在它到达终态时收到一条 `command.terminal`）。
    fn watch_terminal(&self, handle: &ConnectionHandle, command: &Command, actor: &Actor) {
        {
            let mut states = lock(&self.states);
            let state = states
                .entry(handle.connection_id().as_str().to_owned())
                .or_default();
            // 同一 requestId 不重复登记（幂等重放会再次走到这里）。
            if state
                .watching
                .iter()
                .any(|watched| watched.request_id == command.request_id)
            {
                return;
            }
            state.watching.push(WatchedCommand {
                request_id: command.request_id.clone(),
                device_id: handle.device_id().clone(),
                actor: actor.clone(),
                since: Instant::now(),
            });
        }
        self.wake.notify_one();
    }

    /// 终态观察者的循环：有界轮询每条待观察命令的持久记录，把已终结的那些推成 `command.result`。
    ///
    /// 由组合根 spawn，`Shutdown` 触发即返回（**不**留下 detached task）。观察项的取消路径有三条：
    /// 连接离开注册表、命令到达终态、超过 [`WATCH_MAX_AGE`]。
    pub async fn dispatch(self: Arc<Self>, shutdown: Shutdown) {
        let Some(mut receiver) = lock(&self.mkdir_receiver).take() else {
            warn!(
                event = "sync.command_dispatch_duplicate",
                "the command executor already has an owner"
            );
            return;
        };
        let mut executions = JoinSet::new();
        loop {
            let tick = tokio::time::sleep(TERMINAL_POLL_INTERVAL);
            tokio::select! {
                biased;
                () = shutdown.clone().wait() => {
                    // Queued and interrupted accepted commands remain durable for uncertain
                    // recovery. No execution survives its dispatcher or daemon shutdown.
                    receiver.close();
                    while receiver.try_recv().is_ok() {}
                    executions.abort_all();
                    while executions.join_next().await.is_some() {}
                    debug!(event = "sync.command_dispatch_shutdown", "the command executor and terminal watcher stop");
                    return;
                }
                Some(_) = executions.join_next(), if !executions.is_empty() => {}
                Some(execution) = receiver.recv(), if executions.len() < MKDIR_EXECUTION_LIMIT => {
                    let core = self.core.clone();
                    executions.spawn(async move {
                        let _outcome = core.execute_workspace_mkdir(execution).await;
                    });
                }
                () = self.wake.notified() => {}
                () = tick => {}
            }
            self.poll_pending().await;
        }
    }

    /// 轮询所有待观察命令：已终结的推终态并从观察表移除；连接消失的整条状态一起回收。
    async fn poll_pending(&self) {
        // 先回收离开注册表的连接：观察表是连接期状态，`MUST NOT` 留成无主表项。
        self.reclaim("\u{0}").await;
        let mut done: Vec<(String, Uuid)> = Vec::new();
        for (connection_id, device_id, watched) in self.watching_snapshot() {
            let Some(handle) = self.handle_of(&connection_id) else {
                self.forget_connection(&connection_id).await;
                continue;
            };
            if watched.since.elapsed() >= WATCH_MAX_AGE {
                // 有界观察：超龄即放弃，避免一条永不终结的记录把表撑住；对端仍可凭 `command.status` 重查。
                done.push((connection_id.clone(), watched.request_id.clone()));
                warn!(
                    event = "sync.command_terminal_abandoned",
                    connection_id = connection_id.as_str(),
                    request_id = watched.request_id.as_str(),
                    "the command terminal was not observed within the watch budget"
                );
                continue;
            }
            let Some(request) = core_request_id(&watched.request_id) else {
                done.push((connection_id.clone(), watched.request_id));
                continue;
            };
            match self.broker.command_status(&watched.actor, &request).await {
                Ok(Some(record)) if record.status().is_terminal() => {
                    done.push((connection_id.clone(), watched.request_id.clone()));
                    // 命令名取自持久记录本身（观察项只记 requestId；重放的回包必须与首次同形）。
                    match CommandName::from_str(record.command()) {
                        Ok(name) => {
                            self.send_record_terminal(&handle, &watched.request_id, name, &record)
                        }
                        Err(_) => warn!(
                            event = "sync.command_terminal_name_unknown",
                            request_id = watched.request_id.as_str(),
                            command = record.command(),
                            "the persisted command name is not part of the v1 command set"
                        ),
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    warn!(
                        event = "sync.command_status_failed",
                        device_id = device_id.as_str(),
                        request_id = watched.request_id.as_str(),
                        error = ?error,
                        "the command terminal could not be read; the watcher retries on the next tick"
                    );
                }
            }
        }
        for (connection_id, request_id) in done {
            self.forget_watch(&connection_id, &request_id);
        }
    }

    /// 观察表的快照（含所属设备），避免在持锁期间 `await`。
    fn watching_snapshot(&self) -> Vec<(String, acp_core::model::DeviceId, WatchedCommand)> {
        lock(&self.states)
            .iter()
            .flat_map(|(connection_id, state)| {
                state
                    .watching
                    .iter()
                    .map(move |watched| {
                        (
                            connection_id.clone(),
                            watched.device_id.clone(),
                            watched.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn handle_of(&self, connection_id: &str) -> Option<Arc<ConnectionHandle>> {
        self.registry
            .handles()
            .into_iter()
            .find(|handle| handle.connection_id().as_str() == connection_id)
    }

    fn forget_watch(&self, connection_id: &str, request_id: &Uuid) {
        let mut states = lock(&self.states);
        let Some(state) = states.get_mut(connection_id) else {
            return;
        };
        state
            .watching
            .retain(|watched| watched.request_id != *request_id);
    }

    async fn forget_connection(&self, connection_id: &str) {
        let content = lock(&self.states)
            .remove(connection_id)
            .and_then(|state| state.content);
        release_binding(self.files.as_ref(), content).await;
    }

    /// 本机装配失败（时间戳/错误体构造不出来）：这是接线缺陷，按 `internal.unavailable` 显式失败，
    /// 不发一条半成品结果。
    fn encoding_fault(&self, handle: &ConnectionHandle) -> RouteOutcome {
        warn!(
            event = "sync.command_encoding_failed",
            connection_id = handle.connection_id().as_str(),
            "the command result could not be assembled"
        );
        // 绝不静默丢弃：对端要么拿到结果，要么拿到显式的失败，不会一直等一条永不到来的响应。
        self.send_error(
            handle,
            ErrorCode::InternalUnavailable,
            "the command result could not be assembled",
            RawObject::empty(),
        );
        RouteOutcome::Claimed
    }

    /// 解码阶段的拒绝：封闭词表外 → `command.unsupported`，形状不符 → `protocol.schema_invalid`。
    ///
    /// 两者都用 `error` 消息：命令名不在 Sync 的封闭词表里时**拿不到**合法的 `command` 取值，
    /// 组不出一条合规的 `command.result`（§11.5 对 `session.resume` 的要求正是这个码）。
    fn reject_decode(&self, handle: &ConnectionHandle, message: &Envelope, fault: decode::Fault) {
        let (code, text) = match fault {
            decode::Fault::Unsupported => (
                ErrorCode::CommandUnsupported,
                "the command is not part of the sync command set",
            ),
            decode::Fault::Schema => (
                ErrorCode::ProtocolSchemaInvalid,
                "the command does not match the v1 schema",
            ),
        };
        let Some(body) = error_body(code, text, RawObject::empty(), Some(message.message_id()))
        else {
            warn!(
                event = "sync.command_error_body_failed",
                connection_id = handle.connection_id().as_str(),
                "the decode rejection body could not be assembled"
            );
            return;
        };
        if let Err(fault) = handle.send(MessageType::Error, &body) {
            debug!(
                event = "sync.command_error_not_queued",
                connection_id = handle.connection_id().as_str(),
                fault = ?fault,
                "the decode rejection could not be queued"
            );
        }
    }
}

#[async_trait::async_trait]
impl MessageRoute for CommandRoute {
    async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        match message.message_type() {
            MessageType::Command => self.on_command(session, message).await,
            _ => RouteOutcome::Unclaimed,
        }
    }

    fn on_authenticated(&self, session: &ConnectionHandle, features: &[String]) {
        self.note_features(session, features);
    }

    /// 连接关闭（正常关闭、断链、设备撤销、daemon 关闭都走同一收尾路径）：释放本连接的全部连接期状态，
    /// 并显式 `release_content` 它唯一的活动内容版本（§11.5 的连接生命周期）。
    ///
    /// 先整表移出再 `await`：释放失败也不会让状态复活，且不跨 `await` 持有锁。快照的额外兜底是端口的
    /// 空闲 TTL 与 daemon 关闭时的 `NativeSessionFiles::release_all`。
    async fn on_closed(&self, session: &ConnectionHandle) {
        let content = lock(&self.states)
            .remove(session.connection_id().as_str())
            .and_then(|state| state.content);
        release_binding(self.files.as_ref(), content).await;
    }
}

/// 结果超限判定（§11.5）：按**将要发出的那份 body** 的编码字节数加信封开销，与单条消息上限比较。
///
/// 编码失败返回 `false`：由 `send_result` 的 `Encoding` 分支诊断，不谎报超限。判定与比较只此一处，
/// 让查询命令的入队前检查与用例可以用同一份字节口径断言「超限不发送」。
fn result_exceeds_message_limit(result: &CommandResult, max_message_bytes: u64) -> bool {
    let Ok(body) = encode_body(result) else {
        return false;
    };
    body.get().len() as u64 + ENVELOPE_OVERHEAD_BYTES > max_message_bytes
}

/// `details.retryAfterMs`（`resource.rate_limited` 的唯一登记字段，§12.2）。
fn retry_after_details(retry_after: std::time::Duration) -> RawObject {
    details_object(&serde_json::json!({
        "retryAfterMs": retry_after.as_millis() as u64
    }))
}

/// 一个开放对象 `details`（构造失败时退化为 `{}`：登记的 details 是可选扩展点，绝不因它 panic）。
fn details_object(value: &serde_json::Value) -> RawObject {
    RawObject::parse(&value.to_string()).unwrap_or_else(|_| RawObject::empty())
}

/// wire 的 `PublicError`（`retryable` 取封闭错误码的登记值，不自行发明）。
fn public_error(code: ErrorCode, message: &str, details: RawObject) -> Option<PublicError> {
    Some(PublicError {
        code,
        message: Text::<1024>::parse(message).ok()?,
        retryable: code.default_retryable(),
        details,
    })
}

/// `error` 消息的 body（与 `sync/conn` 的同名装配同口径：`retryable` 取登记的默认值）。
fn error_body(
    code: ErrorCode,
    message: &str,
    details: RawObject,
    correlation: Option<&Uuid>,
) -> Option<Body> {
    let retryable = code.default_retryable();
    let mut body = Body::new(code, message, retryable)
        .or_else(|_| Body::new(code, code.as_str(), retryable))
        .ok()?;
    body.correlation_id = Nullable::from_option(correlation.cloned());
    body.details = details;
    Some(body)
}

/// core 的 `PublicError` → wire 的 `PublicError`。
///
/// core 的错误码是开放字符串（它的词表比 Sync 的 36 个更宽，且能携带 `nodelink.*` 之类的细分），wire 的
/// 是封闭枚举：**认不出的码一律落到 `internal.unavailable`**——保守地按可重试处理，而不是谎报一个语义
/// 更窄的码。`details` 逐字节保留（`ViewJson` 是库里/核心给出的原文）。
pub(crate) fn wire_public_error(error: &CorePublicError) -> Option<PublicError> {
    let code = wire_error_code(error.code());
    Some(PublicError {
        code,
        message: Text::<1024>::parse(error.message()).ok()?,
        retryable: error.retryable() && code.default_retryable(),
        details: RawObject::parse(error.details().as_str()).unwrap_or_else(|_| RawObject::empty()),
    })
}

/// core 的错误码 → wire 的封闭错误码（只有确实同义的那些直接对应，其余保守落到 `internal.unavailable`）。
fn wire_error_code(core_code: &str) -> ErrorCode {
    match core_code {
        "authorization.scope_denied" => ErrorCode::AuthorizationScopeDenied,
        "command.unsupported" => ErrorCode::CommandUnsupported,
        "command.not_found" => ErrorCode::CommandNotFound,
        "command.idempotency_conflict" => ErrorCode::CommandIdempotencyConflict,
        "command.uncertain" => ErrorCode::CommandUncertain,
        "state.version_conflict" => ErrorCode::StateVersionConflict,
        "session.not_found" => ErrorCode::SessionNotFound,
        "session.busy" => ErrorCode::SessionBusy,
        "interaction.already_resolved" => ErrorCode::InteractionAlreadyResolved,
        "capability.unsupported_by_client" => ErrorCode::CapabilityUnsupportedByClient,
        "capability.unsupported_by_broker" => ErrorCode::CapabilityUnsupportedByBroker,
        "capability.unsupported_by_agent" => ErrorCode::CapabilityUnsupportedByAgent,
        _ => ErrorCode::InternalUnavailable,
    }
}

/// wire 的 `Timestamp`（core 与 wire 的时间戳形状相同：UTC、毫秒、`Z`）。
pub(crate) fn wire_timestamp(text: &str) -> Option<Timestamp> {
    Timestamp::parse(text).ok()
}

/// wire 的 uuid → core 的 `RequestId`（两侧都是 canonical 小写 UUID 文本）。
pub(crate) fn core_request_id(id: &Uuid) -> Option<CoreRequestId> {
    CoreRequestId::new(id.as_str()).ok()
}

/// 端口错误 → 拒绝。
///
/// `authorization.scope_denied`（节点侧 grant 与本地通道的拒绝）必须先判：把它映射成
/// `protocol.schema_invalid` 会让一个未被授权的**节点**主体把授权拒绝看成形状错误，正是 2.2 要避免的
/// 错误类型漂移。设备方向已不产生该错误码（`pairing-model`：设备不再有授权维度），因此这一臂服务的是
/// core 其它调用方与用例内部的节点判定。
///
/// 保留窗口之前的游标同理必须先判：core 用 `InvalidRequest("history cursor precedes the retained
/// window")` 表达「该游标早于仍可读的下界」（`storage-sqlite` 的 `read_session`），而 §11.5 要求对端看到
/// `sync.cursor_invalid` + `details.reason = "cursor_expired"`。落进通配臂会变成
/// `protocol.schema_invalid`——那是**另一类**事实（形状错误），客户端据此会以为是自己发的帧不合法，
/// 而不是「该翻页位置已被清理」。
pub(crate) fn port_rejection(error: &PortError) -> Rejection {
    match error {
        PortError::InvalidRequest("authorization.scope_denied") => Rejection::Code(
            ErrorCode::AuthorizationScopeDenied,
            "the device is not allowed to send this command",
        ),
        // 会话文件查询对 imported 会话的显式拒绝（core 在文件/Git 端口调用之前给出）。
        PortError::InvalidRequest("capability.unsupported_by_broker") => Rejection::Code(
            ErrorCode::CapabilityUnsupportedByBroker,
            "the broker cannot serve session files for this session",
        ),
        PortError::InvalidRequest(query::RETAINED_WINDOW_EXCEEDED) => {
            Rejection::CursorInvalid(CursorFault::CursorExpired)
        }
        // 会话文件下载超过 64 MiB：core 用这条可识别文本表达预算失败（`SessionFiles::open_content`
        // 的 MUST）。落进下面的通配臂会变成 `protocol.schema_invalid`——那是形状错误，客户端会以为
        // 自己发的帧不合法，而 §11.5 要求的是 `resource.result_too_large`。
        PortError::InvalidRequest(RESULT_TOO_LARGE) => Rejection::Code(
            ErrorCode::ResourceResultTooLarge,
            "the file exceeds the download budget",
        ),
        PortError::InvalidRequest(reason) => Rejection::Schema(schema_hint(reason)),
        // Owner 不在线（imported 会话回源）：显式「现在读不到」，不是 `session.not_found` 或空历史。
        PortError::Unavailable(
            UnavailableKind::OwnerOffline | UnavailableKind::RemoteUnavailable,
        ) => Rejection::RemoteUnavailable,
        PortError::NotFound(_) => Rejection::Code(
            ErrorCode::SessionNotFound,
            "the referenced session is not available",
        ),
        // 目标 Agent 未宣告所需能力（或会话没有所需的持久化数据）：这是永久失败，不是
        // `internal.unavailable` 的临时故障。客户端据此保留草稿并显示明确原因。
        PortError::Unavailable(acp_core::model::UnavailableKind::BackendUnsupported) => {
            Rejection::Code(
                ErrorCode::CommandUnsupported,
                "the agent does not support this command",
            )
        }
        _ => Rejection::Code(
            ErrorCode::InternalUnavailable,
            "the command cannot be served",
        ),
    }
}

/// `PortError::InvalidRequest` 的静态文本 → schema 类拒绝的说明（未知细分取通用说明）。
///
/// 目录类细分只回**字段名与规则**，不回显任何路径原文（`sync-workspace-catalog` 的「错误响应不回显
/// 路径」：客户端自己提供的目录也不得成为对端可见字段）。
fn schema_hint(reason: &str) -> &'static str {
    match reason {
        "cursor" => "the cursor is not valid for this session",
        "workspace alias is not registered on this node" => {
            "the workspaceAlias is not registered on this node"
        }
        "the workspace path must be an absolute path without parent references" => {
            "the workspacePath must be an absolute path without parent references"
        }
        _ => "the request does not match the v1 schema",
    }
}

/// 锁中毒时取回内层数据而不是 panic：命令面在任何 panic 之后仍要能继续拒绝后续命令。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// wire 的命令 payload → core 的 [`acp_core::model::CommandPayload`]（WP3 的 3.1）。
///
/// **不复制第二份映射**：`crates/server/src/node_link/command.rs` 有一份同款 `core_payload`（同族的
/// Node Link 侧），两份的差别只在 wire DTO 的类型来自各自协议 crate —— 这是 `AGENTS.md` §6「Node Link
/// 与 Sync 不得共享 wire DTO」的必然结果，不是重复约定。
///
/// `session.create` **不在这里**：core 没有 `CommandPayload::SessionCreate`，它走独立入口
/// `UseCases::create_session`（Node Link 侧同款分流，见 `node_link/command.rs` 的 query/mutation 分流）。
fn core_payload(payload: &CommandPayload) -> Result<CorePayload, &'static str> {
    Ok(match payload {
        CommandPayload::SessionList(_) => CorePayload::SessionList {},
        // `workspace.browse` 是查询命令（`CommandName::is_query`），永远不会走到本函数（mutation 的
        // 载荷映射）：保留臂只为让判定穷尽，不伪造一个 core 载荷。
        CommandPayload::WorkspaceBrowse(_) => {
            return Err("workspace.browse does not use CommandPayload");
        }
        CommandPayload::WorkspaceMkdir(_) => {
            return Err("workspace.mkdir uses its independent admission and execution use cases");
        }
        CommandPayload::SessionRead(read) => {
            if read
                .include
                .as_slice()
                .iter()
                .any(|item| matches!(item, WireReadInclude::History))
            {
                // `history` 是 §6.2 已裁定的本期降级项（core 的 `ReadInclude` 无此变体）：
                // 显式拒绝而不是静默丢掉它 —— 否则客户端以为请求被满足，拿到的却是残缺结果。
                return Err("the history resource is deferred");
            }
            CorePayload::SessionRead {
                include: read
                    .include
                    .as_slice()
                    .iter()
                    .map(|item| match item {
                        WireReadInclude::Messages => CoreReadInclude::Messages,
                        WireReadInclude::Turns => CoreReadInclude::Turns,
                        WireReadInclude::PendingInteractions => {
                            CoreReadInclude::PendingInteractions
                        }
                        WireReadInclude::ConfigOptions => CoreReadInclude::ConfigOptions,
                        WireReadInclude::Capabilities => CoreReadInclude::Capabilities,
                        WireReadInclude::History => unreachable!("上一条已整体拒绝"),
                    })
                    .collect(),
            }
        }
        CommandPayload::CommandStatus(status) => CorePayload::CommandStatus {
            target_request: core_request_id(&status.target_request_id)
                .ok_or("the targetRequestId is not a uuid")?,
        },
        CommandPayload::ModeList(_) => CorePayload::ModeList {},
        CommandPayload::ConfigList(_) => CorePayload::ConfigList {},
        CommandPayload::SessionPrompt(prompt) => {
            let mut content = Vec::with_capacity(prompt.content.as_slice().len());
            for block in prompt.content.as_slice() {
                // 内容块按原文交给 core（core 只做不透明载体），不重新构造字段。
                let text = serde_json::to_string(block)
                    .map_err(|_| "the prompt block cannot be re-encoded")?;
                content.push(
                    PromptContentBlock::from_json_text(&text)
                        .map_err(|_| "the prompt block is not a valid content block")?,
                );
            }
            CorePayload::Prompt { content }
        }
        CommandPayload::SessionCancel(cancel) => CorePayload::Cancel {
            turn: Some(
                TurnId::new(cancel.turn_id.as_str()).map_err(|_| "the turnId is not a uuid")?,
            ),
        },
        CommandPayload::ModeSet(set) => CorePayload::ModeSet {
            mode: ModeId::new(set.mode_id.as_str())
                .map_err(|_| "the modeId is not a valid mode id")?,
        },
        CommandPayload::ConfigSet(set) => CorePayload::ConfigSet {
            id: ConfigOptionId::new(set.config_id.as_str())
                .map_err(|_| "the configId is out of range")?,
            value: match &set.value {
                WireConfigValue::Text(text) => CoreConfigValue::text(text.as_str())
                    .map_err(|_| "the config value is out of range")?,
                WireConfigValue::Boolean(flag) => CoreConfigValue::Boolean(*flag),
            },
        },
        CommandPayload::PermissionResolve(resolve) => CorePayload::PermissionResolve {
            interaction: InteractionId::new(resolve.interaction_id.as_str())
                .map_err(|_| "the interactionId is not a uuid")?,
            option_id: resolve.option_id.as_str().to_owned(),
        },
        CommandPayload::ElicitationRespond(respond) => CorePayload::ElicitationRespond {
            interaction: InteractionId::new(respond.interaction_id.as_str())
                .map_err(|_| "the interactionId is not a uuid")?,
            action: match respond.action {
                WireElicitationAction::Submit => CoreElicitationAction::Submit,
                WireElicitationAction::Decline => CoreElicitationAction::Decline,
                WireElicitationAction::Cancel => CoreElicitationAction::Cancel,
            },
            values: match respond.values.as_ref() {
                Some(values) => ElicitationValues::parse_json(values.get())
                    .map_err(|_| "the values are not valid elicitation values")?,
                None => ElicitationValues::null(),
            },
        },
        CommandPayload::SessionCreate(_) => {
            return Err("session.create does not use CommandPayload");
        }
        CommandPayload::SessionFilesList(_)
        | CommandPayload::SessionFilesRead(_)
        | CommandPayload::SessionGitStatus(_)
        | CommandPayload::SessionGitDiff(_) => {
            return Err("session file queries are not wired yet");
        }
    })
}

/// wire payload 的 `requestVersion`（`permission.resolve`/`elicitation.respond`）。
///
/// 只在两组命令上有意义；其它 payload 返回 `Ok(None)`。值必须 ≥ 1（0 不是可回传的版本）。
fn wire_request_version(payload: &CommandPayload) -> Result<Option<Version>, &'static str> {
    let version = match payload {
        CommandPayload::PermissionResolve(resolve) => resolve.request_version.as_ref(),
        CommandPayload::ElicitationRespond(respond) => respond.request_version.as_ref(),
        _ => return Ok(None),
    };
    let Some(version) = version else {
        return Ok(None);
    };
    let value = version
        .as_str()
        .parse::<u64>()
        .map_err(|_| "requestVersion is not a decimal string")?;
    if value == 0 {
        return Err("requestVersion must be at least 1");
    }
    Ok(Some(Version::new(value)))
}

/// wire payload 的 `submissionId`（只有 `session.prompt` 有）。
fn wire_submission(payload: &CommandPayload) -> Result<Option<SubmissionId>, &'static str> {
    let CommandPayload::SessionPrompt(prompt) = payload else {
        return Ok(None);
    };
    let Some(submission) = prompt.submission_id.as_ref() else {
        return Ok(None);
    };
    SubmissionId::new(submission.as_str())
        .map(Some)
        .map_err(|_| "submissionId is not a submission id")
}

/// `session.prompt.content` 里的 `attachment_ref` 声明（协议中立；其余命令/分支返回空）。
///
/// 只读 `attachment_ref` 的字段视图（`sync_protocol::common::AttachmentRefView`），不重新序列化块：
/// 形状与字段名只在协议 crate 有一份。数值转换失败（不可能是解码后的合法 wire 值）按形状错误拒绝。
fn prompt_attachments(payload: &CommandPayload) -> Result<Vec<AttachmentBinding>, ()> {
    let CommandPayload::SessionPrompt(prompt) = payload else {
        return Ok(Vec::new());
    };
    let mut bindings = Vec::new();
    for block in prompt.content.as_slice() {
        let Some(reference) = block.attachment_ref() else {
            continue;
        };
        let attachment = AttachmentId::new(reference.attachment_id.as_str()).map_err(|_| ())?;
        let byte_length = reference
            .byte_length
            .as_str()
            .parse::<u64>()
            .map_err(|_| ())?;
        bindings.push(AttachmentBinding {
            attachment,
            sha256: wire_digest(reference.sha256)?,
            byte_length,
        });
    }
    Ok(bindings)
}

/// 定长 base64url 摘要 → core 的规范摘要文本（校验口径一致，只做编码转换）。
fn wire_digest(value: &sync_protocol::common::Base64Url<32>) -> Result<Digest, ()> {
    use base64::Engine as _;
    let text = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.as_bytes());
    Digest::new(&text).map_err(|_| ())
}

/// 载荷指纹（ACPR-CJ1 之后的 SHA-256，§6 第 6 条）——幂等冲突判定的唯一依据。
///
/// 与 `node_link` 的同名助手同一算法；协议无关，但各自持有本协议的 DTO，因此不复用一个函数。
fn payload_fingerprint(payload: &CommandPayload) -> Option<acp_core::model::Digest> {
    use base64::Engine as _;
    let text = serde_json::to_string(payload).ok()?;
    let canonical = acpr_wire::cj1::canonicalize(&text).ok()?;
    let bytes = sha2::Sha256::digest(canonical.as_bytes());
    acp_core::model::Digest::new(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 下载超预算必须走 §11.5 的 `resource.result_too_large`，不能落进 `protocol.schema_invalid`
    /// 通配臂——后者会让客户端以为请求形状不合法，而不是「文件太大」。
    #[test]
    fn download_budget_failure_maps_to_result_too_large() {
        let rejection = port_rejection(&PortError::InvalidRequest(RESULT_TOO_LARGE));
        assert_eq!(
            rejection,
            Rejection::Code(
                ErrorCode::ResourceResultTooLarge,
                "the file exceeds the download budget",
            )
        );
        let (code, text, details) = rejection.into_parts();
        assert_eq!(code.as_str(), "resource.result_too_large");
        assert!(!text.is_empty(), "固定说明文本不得为空");
        assert!(!text.contains('/'), "说明文本不得回显路径");
        assert_eq!(details.get(), "{}", "该错误码没有登记 details");
    }

    /// Owner 离线（imported 会话回源）必须是可区分的 `resource.remote_unavailable` +
    /// `details.reason = "owner_offline"`，不能退化成 `session.not_found` 或通用 `internal.unavailable`。
    /// `RemoteUnavailable` 是同一事实的另一种端口分类，映射必须一致。
    #[test]
    fn owner_offline_maps_to_resource_remote_unavailable() {
        for kind in [
            UnavailableKind::OwnerOffline,
            UnavailableKind::RemoteUnavailable,
        ] {
            let (code, text, details) = port_rejection(&PortError::Unavailable(kind)).into_parts();
            assert_eq!(code.as_str(), "resource.remote_unavailable");
            assert_eq!(text, "the owner node is not reachable");
            let parsed: serde_json::Value =
                serde_json::from_str(details.get()).expect("details 是 JSON");
            assert_eq!(
                parsed,
                serde_json::json!({ "reason": "owner_offline" }),
                "详情必须给出 owner_offline"
            );
            assert_ne!(code.as_str(), "session.not_found");
        }
    }

    /// `attachment_ref` 块经协议 crate 的字段视图读取：文本块忽略，摘要/长度按声明取值，其他命令没有
    /// 附件声明。这是 `session.prompt` 绑定校验的输入端，字段名因此只由协议 crate 决定。
    #[test]
    fn prompt_attachment_refs_are_extracted_from_wire_blocks() {
        use base64::Engine as _;
        let sha = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode({
            use sha2::Digest as _;
            sha2::Sha256::digest(b"hello")
        });
        let attachment = "00000000-0000-4000-8000-000000000123";
        let prompt: sync_protocol::command::SessionPrompt = serde_json::from_str(&format!(
            r#"{{"content":[{{"type":"text","text":"hi"}},{{"type":"attachment_ref",
                "attachmentId":"{attachment}","mediaType":"text/plain",
                "byteLength":"5","sha256":"{sha}"}}]}}"#
        ))
        .expect("prompt");
        let bindings =
            prompt_attachments(&CommandPayload::SessionPrompt(prompt)).expect("bindings");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].attachment.as_str(), attachment);
        assert_eq!(bindings[0].byte_length, 5);
        assert_eq!(bindings[0].sha256.as_str(), sha);
        assert!(
            prompt_attachments(&CommandPayload::SessionList(
                sync_protocol::command::SessionList {}
            ))
            .expect("no bindings")
            .is_empty()
        );
    }

    /// 未登记的 `InvalidRequest` 仍按形状错误处理：新臂只认下载预算这一条文本。
    #[test]
    fn other_invalid_requests_still_map_to_schema_invalid() {
        let rejection = port_rejection(&PortError::InvalidRequest("some other reason"));
        let (code, _, _) = rejection.into_parts();
        assert_eq!(code, ErrorCode::ProtocolSchemaInvalid);
    }

    /// imported 会话在 core 的文件/Git 端口调用**之前**返回 `capability.unsupported_by_broker`；
    /// 会话缺失与端口不可用各自落到既有结构化错误码。
    #[test]
    fn session_file_source_and_io_failures_map_to_existing_codes() {
        let imported = port_rejection(&PortError::InvalidRequest(
            "capability.unsupported_by_broker",
        ));
        let (code, text, details) = imported.into_parts();
        assert_eq!(code, ErrorCode::CapabilityUnsupportedByBroker);
        assert!(!text.contains('/'), "说明文本不得回显路径");
        assert_eq!(details.get(), "{}");

        let missing = port_rejection(&PortError::NotFound(acp_core::model::EntityRef::Session(
            SessionId::new("04dbad03-8869-4c5a-94fa-2f3af2fb622c").expect("session id"),
        )));
        let (code, _, _) = missing.into_parts();
        assert_eq!(code, ErrorCode::SessionNotFound);

        let unavailable = port_rejection(&PortError::Unavailable(
            acp_core::model::UnavailableKind::IoError,
        ));
        let (code, text, _) = unavailable.into_parts();
        assert_eq!(code, ErrorCode::InternalUnavailable);
        assert!(!text.contains('/'));

        let unsupported = port_rejection(&PortError::Unavailable(
            acp_core::model::UnavailableKind::BackendUnsupported,
        ));
        let (code, text, _) = unsupported.into_parts();
        assert_eq!(code, ErrorCode::CommandUnsupported);
        assert!(text.contains("does not support"));
    }

    // -------------------------------------------------------------------------------------
    // 连接期内容版本（§11.5）：跨连接/跨设备/跨会话/跨路径拒绝，关闭时释放
    // -------------------------------------------------------------------------------------

    fn binding(device: &str, session: &str, path: &str, version: &str) -> ContentBinding {
        ContentBinding {
            device: acp_core::model::DeviceId::new(device).expect("device id"),
            session: SessionId::new(session).expect("session id"),
            path: SessionFilePath::parse(path).expect("session path"),
            version: ContentVersion::parse(version).expect("content version"),
        }
    }

    /// 版本只在建立它的四个维度内有效：换连接（设备）、换会话、换路径或换版本都必须拒绝。
    #[test]
    fn content_binding_matches_only_the_establishing_dimensions() {
        let binding = binding(
            "04dbad03-8869-4c5a-94fa-2f3af2fb622b",
            "04dbad03-8869-4c5a-94fa-2f3af2fb622c",
            "src/main.rs",
            "version_1",
        );
        let device = acp_core::model::DeviceId::new("04dbad03-8869-4c5a-94fa-2f3af2fb622b")
            .expect("device id");
        let session = SessionId::new("04dbad03-8869-4c5a-94fa-2f3af2fb622c").expect("session id");
        assert!(binding.matches(&device, &session, "src/main.rs", "version_1"));

        // 另一个连接 = 另一台设备：同一份版本令牌不得当 bearer 使用。
        let other_device = acp_core::model::DeviceId::new("04dbad03-8869-4c5a-94fa-2f3af2fb622d")
            .expect("device id");
        assert!(!binding.matches(&other_device, &session, "src/main.rs", "version_1"));
        // 同一设备上的另一个会话。
        let other_session =
            SessionId::new("04dbad03-8869-4c5a-94fa-2f3af2fb622e").expect("session id");
        assert!(!binding.matches(&device, &other_session, "src/main.rs", "version_1"));
        // 同一会话的另一个路径。
        assert!(!binding.matches(&device, &session, "src/lib.rs", "version_1"));
        // 另一个版本。
        assert!(!binding.matches(&device, &session, "src/main.rs", "version_2"));
    }

    /// 连接关闭路径必须把活动版本交给端口释放，而不是只从内存表里丢掉。
    #[tokio::test]
    async fn closing_a_connection_releases_the_bound_content_version() {
        let files = RecordingFiles::default();
        let binding = binding(
            "04dbad03-8869-4c5a-94fa-2f3af2fb622b",
            "04dbad03-8869-4c5a-94fa-2f3af2fb622c",
            "src/main.rs",
            "version_1",
        );
        release_binding(&files, Some(binding)).await;
        assert_eq!(
            files.releases.lock().expect("releases").as_slice(),
            ["version_1"]
        );
        // 没有绑定（已释放/从未建立）时是空操作，不产生多余端口调用。
        release_binding(&files, None).await;
        assert_eq!(files.releases.lock().expect("releases").len(), 1);
    }

    // -------------------------------------------------------------------------------------
    // 消息预算与限流（§11.5/§14）
    // -------------------------------------------------------------------------------------

    /// 一条 32 KiB 内容块的 wire 结果在单帧预算下必须被判为超限，且判定与出站使用同一份编码字节；
    /// 预算足够时不得误报（超限只换成 `resource.result_too_large`，绝不截断）。
    #[test]
    fn oversized_session_file_results_are_detected_before_sending() {
        use sync_protocol::command::{SessionFilesReadResult, SessionRelativePath};
        use sync_protocol::common::{DecimalString, Nullable};

        let chunk = vec![b'a'; acp_core::model::SESSION_FILE_CHUNK_MAX_BYTES];
        let data = {
            use base64::Engine as _;
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&chunk)
        };
        let result = CommandResult {
            request_id: sync_protocol::common::Uuid::parse("04dbad03-8869-4c5a-94fa-2f3af2fb622b")
                .expect("request id"),
            command: CommandName::SessionFilesRead,
            status: CommandState::Completed,
            accepted_at: Nullable::null(),
            terminal_event_id: Nullable::null(),
            result: Nullable::value(CommandResultPayload::SessionFilesRead(
                SessionFilesReadResult {
                    path: SessionRelativePath::parse("src/main.rs").expect("path"),
                    kind: sync_protocol::command::SessionFileContentKind::Text,
                    size_bytes: DecimalString::parse("32768").expect("size"),
                    version: sync_protocol::command::ContentVersion::parse("version_1")
                        .expect("version"),
                    offset: DecimalString::parse("0").expect("offset"),
                    data_base64: sync_protocol::command::SessionFileData::parse(&data)
                        .expect("base64url"),
                    eof: true,
                    preview_truncated: false,
                },
            )),
            error: Nullable::null(),
        };
        // 单帧上限是小值：真实编码字节（含信封开销）必然超过它。
        assert!(result_exceeds_message_limit(&result, 4 * 1024));
        // 1 MiB 的默认消息预算下，32 KiB 内容块仍装得下：判定不得虚报。
        assert!(!result_exceeds_message_limit(&result, 1024 * 1024));
    }

    /// 四条新命令与既有查询同属 `observe` 族（§14）；超限判定用同一个可重试错误码与 `retryAfterMs`。
    #[test]
    fn session_file_commands_share_the_observe_rate_family() {
        for command in [
            CommandName::SessionFilesList,
            CommandName::SessionFilesRead,
            CommandName::SessionGitStatus,
            CommandName::SessionGitDiff,
        ] {
            assert_eq!(CommandFamily::of(command), CommandFamily::Observe);
        }
        let limiter = CommandRateLimiter::new(1, limits::COMMAND_WINDOW);
        let now = Instant::now();
        assert_eq!(
            limiter.check_at("device-a", CommandFamily::Observe, now),
            limits::RateOutcome::Allowed
        );
        let denied = limiter.check_at("device-a", CommandFamily::Observe, now);
        assert!(matches!(denied, limits::RateOutcome::Denied { .. }));
        let details = retry_after_details(Duration::from_millis(1500));
        assert_eq!(
            details.get(),
            serde_json::json!({ "retryAfterMs": 1500 }).to_string()
        );
        assert_eq!(
            ErrorCode::ResourceRateLimited.as_str(),
            "resource.rate_limited"
        );
    }

    /// 最小 `SessionFiles` 替身：只记录 `release_content` 调用（其余方法在本模块用例里不可达）。
    #[derive(Default)]
    struct RecordingFiles {
        releases: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl SessionFiles for RecordingFiles {
        async fn list(
            &self,
            _root: &str,
            _path: Option<&SessionFilePath>,
            _hide_ignored: bool,
            _after: Option<&acp_core::model::SessionPageToken>,
        ) -> Result<acp_core::model::SessionFilePage, PortError> {
            Err(PortError::Unavailable(
                acp_core::model::UnavailableKind::IoError,
            ))
        }

        async fn open_content(
            &self,
            _root: &str,
            _path: &SessionFilePath,
            _purpose: acp_core::model::FileReadPurpose,
        ) -> Result<acp_core::model::ContentSnapshot, PortError> {
            Err(PortError::Unavailable(
                acp_core::model::UnavailableKind::IoError,
            ))
        }

        async fn read_content(
            &self,
            _version: &ContentVersion,
            _offset: u64,
        ) -> Result<acp_core::model::SessionFileChunk, PortError> {
            Err(PortError::Unavailable(
                acp_core::model::UnavailableKind::IoError,
            ))
        }

        async fn release_content(&self, version: &ContentVersion) -> Result<(), PortError> {
            lock(&self.releases).push(version.as_str().to_owned());
            Ok(())
        }

        async fn git_status(
            &self,
            _root: &str,
            _after: Option<&acp_core::model::SessionPageToken>,
        ) -> Result<acp_core::model::GitStatusPage, PortError> {
            Err(PortError::Unavailable(
                acp_core::model::UnavailableKind::IoError,
            ))
        }

        async fn git_diff(
            &self,
            _root: &str,
            _path: &SessionFilePath,
        ) -> Result<acp_core::model::GitDiff, PortError> {
            Err(PortError::Unavailable(
                acp_core::model::UnavailableKind::IoError,
            ))
        }
    }
}
