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
//!    未登记的命令名在解码期即被拒，本层之下没有任何 core 调用。
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

use acp_core::broker::{Broker, FirstSubmissionContinue, FirstSubmissionStart};
use acp_core::model::{
    Actor, AgentRef, AttachmentId, ClientCommand, CommandKind, CommandPayload as CorePayload,
    CommandReceipt, ConfigOptionId, ConfigValue as CoreConfigValue, ConflictKind, ContentVersion,
    Digest, ElicitationAction as CoreElicitationAction, ElicitationValues, FirstSubmissionConfig,
    FirstSubmissionSettlement, FirstSubmissionStage, InteractionId, ModeId, ModeRef, PortError,
    PreparationFreeze, PreparationStaleReason, PromptContentBlock, PublicError as CorePublicError,
    RESULT_TOO_LARGE, ReadInclude as CoreReadInclude, RequestId as CoreRequestId, RequestOutcome,
    ResolvedWorkspace, ResourceGeneration, SessionFilePath, SessionId, SessionTitle, SubmissionId,
    SubmissionManifestClaim, Timestamp as CoreTimestamp, TurnId, UnavailableKind, Version,
    ViewJson, WorkspaceSource,
};
use acp_core::ports::{
    ActivationOutcome, ActivationRequest, RemoteCommandRecord, SessionFiles, SessionQuery,
};
use acp_core::use_cases::{
    ActivationPlan, LocalSessionLifecycle, ResumeAdmission, SyncSessionQuery, UseCases,
};
use identity_auth::Authority;
use sha2::Digest as _;
use sync_protocol::command::{
    Command, CommandName, CommandPayload, CommandResult, CommandResultPayload, CommandState,
    ElicitationAction as WireElicitationAction, IncludeResource as WireReadInclude,
    SessionCreateResult, SessionCreateWorkspace, SessionFilesRead, SessionStartPayload,
    SessionStartProjection,
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
use crate::sync::local_lifecycle::LOCAL_LIFECYCLE_FEATURE;
use crate::sync::preparation::{
    self as preparation, PREPARATION_FEATURE, PreparationConfigure, PreparationEstablish,
    Preparations, RuntimeCheck,
};
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
/// `session.title.set` 的 feature（`compatibility/features/v1/features.json` 的 sync 子集）。
///
/// 未协商时服务端 MUST NOT 静默接受（§11.5 的 `protocol.feature_required` + `details.features`）：
/// 这条命令会改写会话元数据，旧连接不得因为「命令名恰好能解码」而获得写权限。
pub const SESSION_TITLE_FEATURE: &str = "core.session-title.v1";

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

/// 一次 `session.resume` 失败的映射（拒绝类与终态类共用）。
#[derive(Debug, Clone)]
struct ResumeFailure {
    code: ErrorCode,
    message: &'static str,
    retryable: bool,
    /// `true` = 已受理但结果无法证明（终态 `uncertain`）。
    uncertain: bool,
}

/// 同会话激活的合并门：建立者完成/失败后，**由建立者代所有等待者收口**（等待者已各自落了 `accepted`
/// 记录），因此等待者不需要阻塞连接、也不需要自己再等一次后端。
///
/// 门只对**当前这一次**建立有意义（[`ResumeGateState::live`]）：`begin` 标记建立者在场、自增周期号
/// （[`ResumeGateState::epoch`]）并清掉上一周期的结果，建立者在每个出口带着自己那一代的 `epoch`
/// `publish` 并复位 `live`。没有活的建立者时（回收未确认、进程重启后只剩 `Activating` 的目录事实、
/// 或结果已经发布）[`ResumeGate::register`] 一律立即回显式 `uncertain`——既不空等一个永远不会到来的
/// 结果，也不把上一周期的 `Ok(())` 当成这次请求的 `completed`。
///
/// `epoch` 是必需的栅栏：建立者在 `settle_session` 之后、`publish` 之前有窗口，此时下一周期可能已经
/// `begin` 并登记了新的等待者。`live` 只是无身份的布尔，迟到的那次 `publish` 会把新周期一起关掉，
/// 并用自己的旧结果收口属于新周期的等待者（本该 `completed` 的 resume 被写成 `uncertain`）。因此
/// 收口必须校验周期号，且等待者按登记时的周期入队。
#[derive(Debug, Default)]
struct ResumeGate {
    state: Mutex<ResumeGateState>,
}

#[derive(Debug, Default)]
struct ResumeGateState {
    /// 当前建立周期的身份：每次 `begin` 自增。谁开周期谁持有它，`publish` 时出示。
    epoch: u64,
    /// 是否有活着的建立者会在之后 `publish`（唯一能证明「结果还会来」的事实）。
    live: bool,
    outcome: Option<Result<(), ResumeFailure>>,
    /// 等待者连同**登记时的周期号**入队：旧周期的建立者只能收口自己那一代的等待者。
    waiters: Vec<(u64, ResumeWaiter)>,
}

/// 一条等待同一会话建立结果的命令：建立者收口时用它写自己的终态。
#[derive(Debug, Clone)]
struct ResumeWaiter {
    actor: Actor,
    request_id: CoreRequestId,
    features: NegotiatedFeatures,
}

impl ResumeGate {
    fn new() -> Self {
        Self::default()
    }

    /// 开始一次新的建立：自增周期号、置 `live` 并清掉上一周期的结果（已有等待者保留给**它们那一代**
    /// 的建立者收口）。返回的 `epoch` 就是建立者身份的凭据，必须原样交给自己的每次 `publish`。
    fn begin(&self) -> u64 {
        let mut state = lock(&self.state);
        state.epoch = state.epoch.wrapping_add(1);
        state.live = true;
        state.outcome = None;
        state.epoch
    }

    /// 登记等待者；**只有活的建立者**才可能给出本请求的结局。
    ///
    /// `!live` 覆盖三种情况：从未建立过（进程重启后目录里仍是 `Activating` 或回收未确认）、建立者已经
    /// 发布结果、上一周期的结果还留在门里。三者都拿不到本请求可靠的结局，因此不得复用旧 `outcome`
    /// （上一轮的 `Ok(())` 会被伪造成这次 `completed`），只能显式失败——客户端据此重试或查
    /// `command.status`，不会永远停在 `accepted`。
    fn register(&self, waiter: ResumeWaiter) -> Option<Result<(), ResumeFailure>> {
        let mut state = lock(&self.state);
        if !state.live {
            return Some(Err(unconfirmed_resume_failure()));
        }
        if let Some(outcome) = state.outcome.clone() {
            return Some(outcome);
        }
        let epoch = state.epoch;
        state.waiters.push((epoch, waiter));
        None
    }

    /// 发布结果并收口**本代**等待者：只在自己那一代仍是当前周期时才复位 `live` 并记录结果（此后到达
    /// 的等待者一律 `uncertain`，迟到的结果也不覆盖已发布的事实）。
    ///
    /// 建立者公布结果时可能已经不是当前周期（它 `settle_session` 期间下一周期已 `begin` 并登记了新等待
    /// 者）。此时**只**取走登记在它那一代的等待者：它们等的确实是这次建立的结果，留在这里没人会再收口，
    /// 会把它们的请求永久钉在 `accepted`；而新周期的 `live`、结果与等待者一概不碰——那批等待者由新建立
    /// 者用自己的结果收口，不得被旧结果顶替成 `uncertain`。
    fn publish(&self, epoch: u64, outcome: Result<(), ResumeFailure>) -> Vec<ResumeWaiter> {
        let mut state = lock(&self.state);
        let mut own = Vec::new();
        let mut rest = Vec::with_capacity(state.waiters.len());
        for (queued, waiter) in std::mem::take(&mut state.waiters) {
            if queued == epoch {
                own.push(waiter);
            } else {
                rest.push((queued, waiter));
            }
        }
        state.waiters = rest;
        if state.epoch == epoch {
            state.live = false;
            if state.outcome.is_none() {
                state.outcome = Some(outcome);
            }
        }
        own
    }
}

/// 一条显式的 `uncertain`：已受理，但结局无法证明。
///
/// 三处共用同一个值：没有活的建立者可等（[`ResumeGate::register`]）、建立者发现本次只是幂等重放
/// （[`replayed_resume_settlement`]），以及同键不同语义的冲突（等待者不因别人的冲突被误判成同一失败）。
fn unconfirmed_resume_failure() -> ResumeFailure {
    ResumeFailure {
        code: ErrorCode::CommandUncertain,
        message: "the session resume outcome cannot be confirmed",
        retryable: false,
        uncertain: true,
    }
}

/// 建立者发现本次**没有执行后端**（同 `requestId` 已有受理行，`established == false`）时的收口。
///
/// 两个决定都不许可松：激活归约必须用 `ActivationOutcome::Failed`——它是「可证明本次没有建立外部资源」
/// 的那条语义，会**交还本代预占**并把会话收敛回 `Idle`；若就此返回，会话会永久停在 `Activating` + 已预占，
/// 后续 `resume` 全落 `InFlight` 且名额永不归还（`Uncertain` 反而会保留占位）。持久记录与等待者则收敛为
/// 显式 `uncertain`：本次既没建立、也没资格替别人的受理写 `completed`。
fn replayed_resume_settlement() -> (ActivationOutcome, ResumeFailure) {
    let failure = unconfirmed_resume_failure();
    (
        ActivationOutcome::Failed {
            error: core_public_error(&failure),
        },
        failure,
    )
}

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
#[derive(Debug, Clone, PartialEq, Eq)]
enum ContentTarget {
    Session(SessionId),
    Workspace(
        sync_protocol::command::WorkspaceFilesTarget,
        sync_protocol::command::FileReadPurpose,
    ),
}

#[derive(Debug, Clone)]
struct ContentBinding {
    /// 建立快照的连接所属设备。
    device: acp_core::model::DeviceId,
    /// 该快照所属的会话。
    target: ContentTarget,
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
        target: &ContentTarget,
        path: &str,
        version: &str,
    ) -> bool {
        self.device == *device
            && self.target == *target
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
        CommandStatus::Accepted => CommandState::Accepted,
        CommandStatus::Rejected => CommandState::Rejected,
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
    /// 本机生命周期（`core.local-session-lifecycle.v1`）：`session.resume` 的激活/复用与 owned 摘要的
    /// `localActivity` 都经它读。`None` = 组合根没有接线（该 feature 不宣告）。
    lifecycle: Option<Arc<LocalSessionLifecycle>>,
    /// 准备运行期目录、凭据验证与首条受理读面（`core.session-preparation.v1`）。`None` = 组合根没有
    /// 接线：`session.start` 与协商该 feature 的 `command.status` 投影显式不可用。
    preparations: Option<Arc<crate::sync::preparation::Preparations>>,
    /// 准备端点的建立/复核/转正窄面（组合根用 `agent-host` 实现）。两者必须同时接线。
    preparation_endpoints: Option<Arc<dyn crate::sync::preparation::PreparationEndpoints>>,
    /// 同会话 `session.start` 的准备阶段串行门：establish → claim → 核心受理必须对同一会话线性化，
    /// 否则两个并发继续可能各自建立端点、互相释放对方的绑定（核心的会话门只覆盖受理之后）。
    start_gates: Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// 同会话激活的合并门：建立者完成/失败时唤醒其它设备的等待者。
    resume_gates: Mutex<BTreeMap<String, Arc<ResumeGate>>>,
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
        lifecycle: Option<Arc<LocalSessionLifecycle>>,
    ) -> Self {
        let (mkdir_queue, mkdir_receiver) = mpsc::channel(MKDIR_QUEUE_CAPACITY);
        Self {
            core,
            broker,
            authority,
            registry,
            owner,
            files,
            lifecycle,
            preparations: None,
            preparation_endpoints: None,
            start_gates: Mutex::new(BTreeMap::new()),
            resume_gates: Mutex::new(BTreeMap::new()),
            rate: CommandRateLimiter::new(limits::COMMANDS_PER_MINUTE, limits::COMMAND_WINDOW),
            consecutive_limit: limits::MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS,
            states: Mutex::new(BTreeMap::new()),
            wake: tokio::sync::Notify::new(),
            mkdir_queue,
            mkdir_receiver: Mutex::new(Some(mkdir_receiver)),
        }
    }

    /// 接线准备控制/首次提交面（`core.session-preparation.v1`）。
    ///
    /// 只有两个零件**同时**就位才宣告能力：`preparations` 提供凭据验证/运行期目录/首条读面，
    /// `endpoints` 提供临时端点建立、设置复核与所有权转移。任一缺失时 `session.start` 必须显式不可用，
    /// 组合根因此不宣告该 feature（能力协商不得虚报）。
    pub fn with_preparation(
        mut self,
        preparations: Arc<crate::sync::preparation::Preparations>,
        endpoints: Arc<dyn crate::sync::preparation::PreparationEndpoints>,
    ) -> Self {
        self.preparations = Some(preparations);
        self.preparation_endpoints = Some(endpoints);
        self
    }

    /// 准备面是否已完整接线。
    fn preparation_ready(&self) -> bool {
        self.preparations.is_some() && self.preparation_endpoints.is_some()
    }

    /// 该会话的 `session.start` 准备阶段串行门（表项在最后一个持有者离开后可被回收）。
    fn start_gate(&self, session: &SessionId) -> Arc<tokio::sync::Mutex<()>> {
        let mut gates = lock(&self.start_gates);
        gates.retain(|_, gate| Arc::strong_count(gate) > 1);
        Arc::clone(gates.entry(session.as_str().to_owned()).or_default())
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
                query::session_list(
                    &self.core,
                    &self.owner,
                    actor,
                    &features,
                    self.lifecycle.as_ref(),
                )
                .await
            }
            CommandPayload::SessionRead(payload) => {
                query::session_read(
                    &self.core,
                    actor,
                    &command,
                    payload,
                    &features,
                    self.lifecycle.as_ref(),
                )
                .await
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
        let target = match payload.workspace.as_ref() {
            Some(workspace) => {
                query::workspace_source(command, Some(workspace), features)?;
                ContentTarget::Workspace(workspace.clone(), payload.purpose)
            }
            None => ContentTarget::Session(query::command_session(command)?),
        };
        let requested = payload
            .version
            .as_ref()
            .map(|version| version.as_str().to_owned());
        let path = payload.path.as_str().to_owned();
        match requested.as_deref() {
            Some(version) => {
                if !self.content_matches(handle, &target, &path, version) {
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
                    target,
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
        target: &ContentTarget,
        path: &str,
        version: &str,
    ) -> bool {
        lock(&self.states)
            .get(handle.connection_id().as_str())
            .and_then(|state| state.content.as_ref())
            .is_some_and(|binding| binding.matches(handle.device_id(), target, path, version))
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
        // `session.title.set` 的目标可能是 owned（本机写入）或 imported（纯在线转发给 Owner）。两条
        // 路径的入口不同（`set_session_title` / `submit_remote_session_title`），因此在这里分流，
        // 而不是让 generic mutation 管道把它当成「会话不存在」。
        if command.command == CommandName::SessionTitleSet {
            return self.on_session_title(handle, &actor, command).await;
        }
        if command.command == CommandName::SessionResume {
            return self.on_session_resume(handle, &actor, command).await;
        }
        // `session.start`：准备凭据 + 首条原子受理 + 派发围栏由独立用例承载（不走通用 mutation 管道）。
        if command.command == CommandName::SessionStart {
            return self.on_session_start(handle, &actor, command).await;
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
                // 普通 prompt 的派发可能在 `submit_command` 返回前就同步失败（脚本后端/快速拒绝），
                // 此时持久记录已是终态：只回一条终态，但把本次 `turnId` 放进 `result`，客户端仍能
                // 把本地即时条目与持久用户消息按稳定身份接替；不能先伪造一条 `accepted`（重放也会
                // 命中同一分支，重复的 accepted 会破坏「重放只回终态」）。
                self.send_record_terminal_with_turn(
                    handle,
                    &command.request_id,
                    command.command,
                    &record,
                    turn.as_ref(),
                );
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

    /// `session.title.set`（`design.md` D2/D3；spec `session-custom-title`）。
    ///
    /// 目标会话的归属决定入口，**两者只走一条**：
    ///
    /// - **本机 owned**：`UseCases::set_session_title` —— 唯一权威是本机。core 在同一事务里写标题、
    ///   推进版本、写完整 `session.updated` 与命令终态，并先提交后广播；重复 `requestId` 由 core 的
    ///   幂等行去重（同键重发回同一终态、不重复修改）。
    /// - **imported（不是本机 owned）**：`UseCases::submit_remote_session_title` —— 纯在线转发给 Owner，
    ///   本层只把 Owner 返回的当前记录投影成 `command.result`。**不写** `owned_command`、**不**追加
    ///   本地事件、**不**缓存标题：Access 不是远程标题的第二权威；结果丢失只能由客户端用原
    ///   `requestId` + `command.status.remoteTarget` 回源。
    /// - **两者都不是**：`session.not_found`。
    ///
    /// 顺序与其余 mutation 一致：feature 门控 → 形状/指纹/requestId → 归属判定 → core 调用 → 回复。
    /// 门控在任何存储读取之前（未协商的连接连一次会话行都读不到）。
    async fn on_session_title(
        &self,
        handle: &ConnectionHandle,
        actor: &Actor,
        command: Command,
    ) -> RouteOutcome {
        if !self.features(handle).contains(SESSION_TITLE_FEATURE) {
            let Some(error) = public_error(
                ErrorCode::ProtocolFeatureRequired,
                "the client did not negotiate the session-title feature",
                details_object(&serde_json::json!({ "features": [SESSION_TITLE_FEATURE] })),
            ) else {
                return self.encoding_fault(handle);
            };
            self.send_rejected(handle, &command, error);
            return RouteOutcome::Claimed;
        }
        let CommandPayload::SessionTitleSet(payload) = &command.payload else {
            self.deny(
                handle,
                &command,
                "the payload does not match the command",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        };
        // 合同明文：`session.title.set` 不接受 `expectedVersion`（按 Owner 提交顺序后写生效）。
        if command.expected_version.is_some() {
            self.deny(
                handle,
                &command,
                "session.title.set does not take expectedVersion",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        }
        let Some(session) = command
            .session_id
            .as_ref()
            .and_then(|session| SessionId::new(session.as_str()).ok())
        else {
            self.deny(
                handle,
                &command,
                "session.title.set requires a sessionId",
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
        let Ok(title) = SessionTitle::parse(payload.title.as_str()) else {
            self.deny(
                handle,
                &command,
                "the title is not within bounds",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        };
        // 归属判定：按 `SessionQuery::only` 只取这一行（不是全表扫描，也不建第二条可见性路径——授权
        // 仍由 core 的 `session.list` 判定）。`owned_session` 里存在即 owned；imported 会话在本机没有行。
        let owned = match self
            .core
            .list_sessions(
                actor,
                SessionQuery {
                    only: Some(vec![session.clone()]),
                    states: Vec::new(),
                    limit: Some(1),
                },
            )
            .await
        {
            Ok(rows) => !rows.is_empty(),
            Err(error) => return self.reject_port_error(handle, &command, &error),
        };
        if !owned {
            // Access 侧受理这条转发的时刻：在**发出请求之前**取，作为 Owner 终态不携带
            // `acceptedAt` 时的代理值（见 `send_remote_record`）；不能用回包时刻冒充受理时刻。
            let accepted_at = self.authority.now();
            match self
                .core
                .submit_remote_session_title(actor, &session, request_id, title)
                .await
            {
                Ok(record) => {
                    self.send_remote_record(handle, &command, &record, accepted_at.as_str())
                }
                // 本机没有这个会话；远程目录里也没有（或本进程根本没有远程写面：只服务 Owner 角色时
                // 不存在 imported 会话）。两个入口都给不出结果 ⇒ `session.not_found`。
                Err(PortError::NotFound(_))
                | Err(PortError::Unavailable(UnavailableKind::BackendUnsupported)) => {
                    let Some(error) = public_error(
                        ErrorCode::SessionNotFound,
                        "the referenced session is not available",
                        RawObject::empty(),
                    ) else {
                        return self.encoding_fault(handle);
                    };
                    self.send_rejected(handle, &command, error);
                }
                // Owner 明确拒绝（例如没有协商 `core.session-title.v1`）→ 走既有的端口错误映射
                // （`capability.unsupported_by_broker` / `resource.remote_unavailable`），不伪装成功。
                Err(error) => return self.reject_port_error(handle, &command, &error),
            }
            return RouteOutcome::Claimed;
        }
        let client_command = ClientCommand {
            actor: actor.clone(),
            request: request_id.clone(),
            command: command.command.as_str().to_owned(),
            kind: CommandKind::Mutation,
            session: Some(session),
            expected_version: None,
            request_fingerprint: fingerprint,
            payload: CorePayload::TitleSet { title },
        };
        let receipt = match self.core.set_session_title(actor, client_command).await {
            Ok(receipt) => receipt,
            Err(error) => return self.reject_port_error(handle, &command, &error),
        };
        let turn = match receipt {
            CommandReceipt::Rejected { error } => {
                let Some(error) = wire_public_error(&error) else {
                    return self.encoding_fault(handle);
                };
                self.send_rejected(handle, &command, error);
                return RouteOutcome::Claimed;
            }
            CommandReceipt::Accepted { turn, .. } => turn,
        };
        // 与其余 mutation 同口径：按 `(actor, requestId)` 读回持久记录。`session.title.set` 的写入与
        // 终态在**同一事务**完成，因此首次提交读到的一般就是终态；同键重放也走同一条路径。
        match self.broker.command_status(actor, &request_id).await {
            Ok(Some(record)) if record.status().is_terminal() => {
                self.send_record_terminal(handle, &command.request_id, command.command, &record);
            }
            Ok(Some(record)) => {
                self.send_accepted(handle, &command, turn, record.accepted_at());
                self.watch_terminal(handle, &command, actor);
            }
            Ok(None) | Err(_) => {
                self.encoding_fault(handle);
            }
        }
        RouteOutcome::Claimed
    }

    /// 端口错误 → `command.result(rejected)`（与 generic mutation 路径同一映射与同一 `details` 取值）。
    fn reject_port_error(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        error: &PortError,
    ) -> RouteOutcome {
        let (code, text, details) = port_rejection(error).into_parts();
        let Some(error) = public_error(code, text, details) else {
            return self.encoding_fault(handle);
        };
        self.send_rejected(handle, command, error);
        RouteOutcome::Claimed
    }

    /// Owner 为一条 **imported** 命令返回的记录 → `command.result`（`design.md` D3）。
    ///
    /// 只做透明投影：
    /// - `accepted`：`terminalEventId`/`result` 均为 `null`（Owner 尚未终结）；
    /// - 终态：status、Owner 的 `terminalEventId`、result/error 原样；
    /// - `acceptedAt` 取 Owner 记录里的值。`command.terminal` 不携带接受时间，因此 Owner 直接给终态时
    ///   本字段只能是 `null`——这是「Access 不伪造本地事实」的直接结果，不拿本地时钟冒充 Owner 的时间。
    ///
    /// 本实现**不**写 `command_log`：远程请求标识不进入任何本地日志（`RemoteTitleWrite` 的合同）。
    fn send_remote_record(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        record: &RemoteCommandRecord,
        forwarded_at: &str,
    ) {
        // 远程记录的 `acceptedAt` 是 Owner 的受理时间，但 Node Link 的 `command.terminal` 不携带它。
        // `forwarded_at` 是 Access **发出转发之前**记下的受理时刻；Owner 未给出受理时间时用它补齐，
        // 否则 Sync 的 `command.result` 形状会被客户端拒绝。它不写回 Owner 记录。
        let Some(result) = remote_result(command, record, Some(forwarded_at)) else {
            self.encoding_fault(handle);
            return;
        };
        self.send_result(handle, &result, "remote");
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
        let features = self.features(handle);
        let catalog = features.contains(LOCAL_CATALOG_FEATURE);
        let lifecycle_visible = features.contains(LOCAL_LIFECYCLE_FEATURE);
        let settled = match self
            .created_session_summary(actor, &prepared, catalog, lifecycle_visible)
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

    // -------------------------------------------------------------------------------------
    // session.resume（core.local-session-lifecycle.v1）
    // -------------------------------------------------------------------------------------

    /// `session.resume` 的 Sync 面（§11.5/§11.8）。
    ///
    /// 顺序固定为「feature 门控 → 认证（已由 `on_command` 完成）→ owned 定位 → 生命周期准入 → 持久
    /// 受理 → 工作 → 终态」：
    ///
    /// - **未协商 feature**：`command.unsupported`，不读会话行、不触碰 Agent（§5.2/§11.8）。
    /// - **ready 复用与并发合并**：受理-only 记录由 core 的
    ///   [`UseCases::admit_session_resume`](acp_core::use_cases::UseCases::admit_session_resume) 写入，
    ///   **不调用** `resume_session`（那会再发一次 ACP `session/resume`）；同一会话的建立者完成后经
    ///   [`ResumeGate`] 唤醒等待者，后者各自写自己的终态。
    /// - **建立**：`resume_session` 先落 `accepted`、再做唯一一次后端恢复；失败按
    ///   [`Self::resume_failure`] 分类，绝不误报 `protocol.schema_invalid`。
    async fn on_session_resume(
        &self,
        handle: &ConnectionHandle,
        actor: &Actor,
        command: Command,
    ) -> RouteOutcome {
        if !self.features(handle).contains(LOCAL_LIFECYCLE_FEATURE) {
            return self.reject_resume(
                handle,
                &command,
                ErrorCode::CommandUnsupported,
                "the local session lifecycle feature was not negotiated",
            );
        }
        let Some(lifecycle) = self.lifecycle.clone() else {
            return self.reject_resume(
                handle,
                &command,
                ErrorCode::CommandUnsupported,
                "the local session lifecycle is not available on this host",
            );
        };
        if !matches!(command.payload, CommandPayload::SessionResume(_)) {
            self.deny(
                handle,
                &command,
                "the payload does not match the command",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        }
        let Some(session) = command
            .session_id
            .as_ref()
            .and_then(|id| acp_core::model::SessionId::new(id.as_str()).ok())
        else {
            self.deny(
                handle,
                &command,
                "sessionId is not a session id",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        };
        // owned 定位：本机会话列表是唯一口径；远程/导入会话一律 `session.not_found`（不泄露来源）。
        match self
            .core
            .list_sessions(actor, acp_core::ports::SessionQuery::default())
            .await
        {
            Ok(sessions) if sessions.iter().any(|s| s.session_id() == &session) => {}
            Ok(_) => {
                return self.reject_resume(
                    handle,
                    &command,
                    ErrorCode::SessionNotFound,
                    "the session is not available on this host",
                );
            }
            Err(error) => {
                return self.resume_rejected_by_port(handle, &command, &error);
            }
        }
        let Some(fingerprint) = payload_fingerprint(&command.payload) else {
            return self.encoding_fault(handle);
        };
        let Some(request_id) = core_request_id(&command.request_id) else {
            return self.encoding_fault(handle);
        };
        let request = ActivationRequest {
            actor: actor.clone(),
            request_id: request_id.clone(),
            request_fingerprint: fingerprint.clone(),
            session: session.clone(),
        };
        let plan = match lifecycle.activate_session(actor, request).await {
            Ok(plan) => plan,
            Err(error) => return self.resume_rejected_by_port(handle, &command, &error),
        };
        match plan {
            ActivationPlan::Settled(record) => {
                self.send_record_terminal(handle, &command.request_id, command.command, &record);
                RouteOutcome::Claimed
            }
            ActivationPlan::CapacityExceeded => self.reject_resume(
                handle,
                &command,
                ErrorCode::SessionCapacityExceeded,
                "no active session slot is available right now",
            ),
            ActivationPlan::Reused { .. } => {
                // 资源已经 ready：只落受理与终态，**不触碰 ACP**。
                match self
                    .core
                    .admit_session_resume(actor, request_id.clone(), fingerprint, session.clone())
                    .await
                {
                    Ok(ResumeAdmission::Settled(record)) => {
                        self.send_record_terminal(
                            handle,
                            &command.request_id,
                            command.command,
                            &record,
                        );
                    }
                    Ok(ResumeAdmission::InFlight) => {
                        // 同键已有受理行：回首次落盘时间，再等观察者推终态（不阻塞后续帧）。
                        self.send_accepted_if_persisted(handle, &command, actor, &request_id)
                            .await;
                        self.watch_terminal(handle, &command, actor);
                    }
                    Ok(ResumeAdmission::Prepared) => {
                        // 刚写入的受理行就是首次落盘时间（不用现场时钟）。
                        self.send_accepted_if_persisted(handle, &command, actor, &request_id)
                            .await;
                        self.finish_resume(handle, actor, &command, &session, Ok(()))
                            .await;
                    }
                    Err(error) => {
                        return self.resume_rejected_by_port(handle, &command, &error);
                    }
                }
                RouteOutcome::Claimed
            }
            ActivationPlan::Establish { generation } => {
                // 建立者：开合并门 → 后端恢复一次 → 生命周期收口 → 写自己的终态 → 代等待者收口。
                let gate = self.resume_gate(&session);
                // 本次建立周期的身份：之后每个出口的 `publish` 都必须出示它，迟到的发布才不会把下一代
                // 周期（别人的 `live` 与等待者）一起收口。
                let epoch = gate.begin();
                let result = match self
                    .core
                    .resume_session_with_execution(
                        actor,
                        request_id.clone(),
                        fingerprint,
                        session.clone(),
                    )
                    .await
                {
                    Ok(execution) if execution.established => {
                        // `Broker` 已先持久化 accepted 行；在终态之前显式回一次 accepted（§11.2：
                        // mutation 只同步接受，终态由持久事件给出），时间取首次落盘值。
                        self.send_accepted(handle, &command, None, execution.accepted_at.as_ref());
                        let _ = lifecycle
                            .settle_session(
                                actor,
                                session.clone(),
                                generation,
                                ActivationOutcome::Ready,
                            )
                            .await;
                        self.finish_resume(handle, actor, &command, &session, Ok(()))
                            .await;
                        Ok(())
                    }
                    Ok(_) => {
                        // 幂等重放：同 `requestId` 已有受理行，本次**没有**执行后端，也不会再有任何
                        // 建立者收口这条会话的资源代。就此返回会让会话永久停在 `Activating` + 已预占：
                        // 之后每次 resume 都落 `InFlight`，名额永不归还。因此显式收口——先按「先受理、
                        // 后终态」补发 accepted，用 `Failed` 交还本代预占，持久记录收敛为 `uncertain`，
                        // 再把同一结果交给外层 `publish` 唤醒等待者（不写 `completed`，不碰 `Ready`）。
                        let (activation, failure) = replayed_resume_settlement();
                        self.send_accepted_if_persisted(handle, &command, actor, &request_id)
                            .await;
                        let _ = lifecycle
                            .settle_session(actor, session.clone(), generation, activation)
                            .await;
                        self.finish_resume(handle, actor, &command, &session, Err(failure.clone()))
                            .await;
                        Err(failure)
                    }
                    Err(error) => {
                        let failure = self.resume_failure(&error);
                        if failure.code == ErrorCode::CommandIdempotencyConflict {
                            // 同键不同语义：零副作用，回 `rejected`，不写失败终态。等待者不因这条
                            // 冲突的请求被误判成同一失败：它们的会话仍无建立结果，收敛为 uncertain。
                            let waiter_failure = unconfirmed_resume_failure();
                            self.finish_resume_waiters(
                                &session,
                                gate.publish(epoch, Err(waiter_failure.clone())),
                                Err(waiter_failure),
                            )
                            .await;
                            return self.reject_resume(
                                handle,
                                &command,
                                ErrorCode::CommandIdempotencyConflict,
                                "the requestId was already used with different parameters",
                            );
                        }
                        // `resume_session` 可能已先写下 accepted 行再失败（例如 Agent 拒绝恢复）：
                        // 按「先受理、后终态」补发 accepted；前置检查类失败没有持久行，不发。
                        self.send_accepted_if_persisted(handle, &command, actor, &request_id)
                            .await;
                        let activation = if failure.uncertain {
                            ActivationOutcome::Uncertain {
                                error: core_public_error(&failure),
                            }
                        } else {
                            ActivationOutcome::Failed {
                                error: core_public_error(&failure),
                            }
                        };
                        let _ = lifecycle
                            .settle_session(actor, session.clone(), generation, activation)
                            .await;
                        self.finish_resume(handle, actor, &command, &session, Err(failure.clone()))
                            .await;
                        Err(failure)
                    }
                };
                // 只收口本代等待者：若期间已开启下一周期，`publish` 不动新周期的 `live` 与等待者。
                let waiters = gate.publish(epoch, result.clone());
                self.finish_resume_waiters(&session, waiters, result).await;
                RouteOutcome::Claimed
            }
            ActivationPlan::InFlight => {
                // 另一个请求正在建立（或回收未确认）：本请求留下自己的合法受理记录，然后登记为等待者。
                // 门只在**活的建立者**在场时排队（登记即回 `accepted` + 终态观察，**不阻塞**连接）；
                // 没有活的建立者（回收未确认 / 结果已发布 / 重启后只剩目录事实）时 `register` 直接回
                // 显式 `uncertain`，本请求立即收口，绝不停在 `accepted` 等一个不会到来的结果。
                match self
                    .core
                    .admit_session_resume(actor, request_id.clone(), fingerprint, session.clone())
                    .await
                {
                    Ok(ResumeAdmission::Settled(record)) => {
                        self.send_record_terminal(
                            handle,
                            &command.request_id,
                            command.command,
                            &record,
                        );
                        return RouteOutcome::Claimed;
                    }
                    Ok(ResumeAdmission::InFlight | ResumeAdmission::Prepared) => {}
                    Err(error) => {
                        return self.resume_rejected_by_port(handle, &command, &error);
                    }
                }
                // 受理时间取**首次落盘**的记录（不是现在的时钟）；没有行时（不应发生）不发。
                self.send_accepted_if_persisted(handle, &command, actor, &request_id)
                    .await;
                let gate = self.resume_gate(&session);
                let immediate = gate.register(ResumeWaiter {
                    actor: actor.clone(),
                    request_id: request_id.clone(),
                    features: self.features(handle),
                });
                match immediate {
                    Some(outcome) => {
                        // 不用再等：要么建立者已经收口（按它的结果写本请求自己的终态），要么门里没有
                        // 活的建立者（`uncertain` 是显式失败，不伪造 completed）。
                        self.finish_resume(handle, actor, &command, &session, outcome)
                            .await;
                    }
                    None => self.watch_terminal(handle, &command, actor),
                }
                RouteOutcome::Claimed
            }
        }
    }

    /// `session.start` 的入口（`web-session-preparation` D7/D8/D8.1）。
    ///
    /// 顺序固定：feature 门控 → 认证/凭据绑定 → 同 `requestId` 的幂等回放 → 已受理查询 → 运行期与
    /// 资源代 → 端点复核/转正 → 核心受理（原子提交 + 派发围栏）。任何一步失败都不会产生半提交，
    /// 也不会自动派发。
    async fn on_session_start(
        &self,
        handle: &ConnectionHandle,
        actor: &Actor,
        command: Command,
    ) -> RouteOutcome {
        if !self.features(handle).contains(PREPARATION_FEATURE) {
            // §11.9：未协商该 feature 时 `session.start` 回 `command.unsupported`（不建资源、不读存储）。
            self.deny(
                handle,
                &command,
                "the client did not negotiate core.session-preparation.v1",
                ErrorCode::CommandUnsupported,
            );
            return RouteOutcome::Claimed;
        }
        if !self.preparation_ready() {
            self.deny(
                handle,
                &command,
                "the session-preparation capability is not assembled",
                ErrorCode::CommandUnsupported,
            );
            return RouteOutcome::Claimed;
        }
        let CommandPayload::SessionStart(payload) = &command.payload else {
            self.deny(
                handle,
                &command,
                "the payload does not match session.start",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        };
        let Some(session) = command
            .session_id
            .as_ref()
            .and_then(|id| SessionId::new(id.as_str()).ok())
        else {
            self.deny(
                handle,
                &command,
                "session.start requires envelope.sessionId",
                ErrorCode::ProtocolSchemaInvalid,
            );
            return RouteOutcome::Claimed;
        };
        let Some(request_id) = core_request_id(&command.request_id) else {
            return self.encoding_fault(handle);
        };
        let Some(fingerprint) = payload_fingerprint(&command.payload) else {
            return self.encoding_fault(handle);
        };

        // 同一 `requestId` 的重放只读原受理/终态（包括 rejected/failed），不触碰准备资源。
        match self.core.command_status(actor, request_id.clone()).await {
            Ok(Some(record)) if record.command() == "session.start" => {
                if record.session() != Some(&session)
                    || record.request_fingerprint() != &fingerprint
                {
                    let Some(error) = public_error(
                        ErrorCode::CommandIdempotencyConflict,
                        "the requestId was already used for a different session.start",
                        RawObject::empty(),
                    ) else {
                        return self.encoding_fault(handle);
                    };
                    self.send_rejected(handle, &command, error);
                    return RouteOutcome::Claimed;
                }
                return self.reply_existing_start(handle, &command, &record);
            }
            Ok(_) => {}
            Err(_) => return self.encoding_fault(handle),
        }

        let (Some(preparations), Some(endpoints)) = (
            self.preparations.as_ref(),
            self.preparation_endpoints.as_ref(),
        ) else {
            self.deny(
                handle,
                &command,
                "the session-preparation capability is not assembled",
                ErrorCode::CommandUnsupported,
            );
            return RouteOutcome::Claimed;
        };
        let outcome = match payload {
            SessionStartPayload::Initial {
                receipt,
                runtime_epoch,
                generation,
                content,
                submission_id,
                mode,
                config_options,
            } => {
                self.start_initial(
                    handle,
                    actor,
                    preparations,
                    endpoints,
                    &session,
                    &request_id,
                    &fingerprint,
                    receipt,
                    runtime_epoch,
                    generation,
                    content,
                    submission_id,
                    mode,
                    config_options,
                )
                .await
            }
            SessionStartPayload::Continue {
                first_submission_id,
                retry_of_request_id,
                mode,
                config_options,
            } => {
                self.start_continue(
                    handle,
                    actor,
                    preparations,
                    endpoints,
                    &session,
                    &request_id,
                    &fingerprint,
                    first_submission_id,
                    retry_of_request_id,
                    mode,
                    config_options,
                )
                .await
            }
        };
        match outcome {
            Ok(Some(receipt)) => {
                self.reply_first_submission(handle, &command, actor, &request_id, receipt)
                    .await
            }
            // 已经在上面的幂等分支里回复过。
            Ok(None) => RouteOutcome::Claimed,
            Err((code, message, details)) => {
                let Some(error) = public_error(code, message, details) else {
                    return self.encoding_fault(handle);
                };
                self.send_rejected(handle, &command, error);
                RouteOutcome::Claimed
            }
        }
    }

    /// 回放一条既有的 `session.start` 持久记录（`command.status` 与提交回复共用同一条路径）。
    fn reply_existing_start(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        record: &acp_core::model::CommandRecord,
    ) -> RouteOutcome {
        if record.status().is_terminal() {
            self.send_record_terminal(handle, &command.request_id, command.command, record);
            return RouteOutcome::Claimed;
        }
        let turn = None;
        self.send_accepted(handle, command, turn, record.accepted_at());
        self.watch_terminal(
            handle,
            command,
            &Actor::Device {
                device: handle.device_id().clone(),
            },
        );
        RouteOutcome::Claimed
    }

    /// 核心受理结果 → 与通用 mutation 相同的回包（`accepted` 后按持久记录观察终态）。
    async fn reply_first_submission(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        actor: &Actor,
        request_id: &CoreRequestId,
        receipt: CommandReceipt,
    ) -> RouteOutcome {
        let turn = match receipt {
            CommandReceipt::Rejected { error } => {
                let Some(error) = wire_public_error(&error) else {
                    return self.encoding_fault(handle);
                };
                self.send_rejected(handle, command, error);
                return RouteOutcome::Claimed;
            }
            CommandReceipt::Accepted { turn, .. } => turn,
        };
        match self.broker.command_status(actor, request_id).await {
            Ok(Some(record)) if record.status().is_terminal() => {
                self.send_record_terminal(handle, &command.request_id, command.command, &record);
            }
            Ok(Some(record)) => {
                self.send_accepted(handle, command, turn, record.accepted_at());
                self.watch_terminal(handle, command, actor);
            }
            Ok(None) | Err(_) => {
                self.encoding_fault(handle);
            }
        }
        RouteOutcome::Claimed
    }

    /// `kind: initial`：凭据 → 已受理 → 运行期 → 端点复核/转正 → 核心原子受理。
    #[allow(clippy::too_many_arguments)] // 与 wire 的闭合 `initial` 分支字段一一对应
    async fn start_initial(
        &self,
        handle: &ConnectionHandle,
        actor: &Actor,
        preparations: &Arc<Preparations>,
        endpoints: &Arc<dyn crate::sync::preparation::PreparationEndpoints>,
        session: &SessionId,
        request_id: &CoreRequestId,
        fingerprint: &Digest,
        receipt: &sync_protocol::common::PreparationReceipt,
        runtime_epoch: &Uuid,
        generation: &sync_protocol::common::PositiveDecimalString,
        content: &sync_protocol::command::PromptContentList,
        submission_id: &Option<Uuid>,
        mode: &Option<sync_protocol::common::NonEmptyText<256>>,
        config_options: &Option<sync_protocol::command::PreparationConfigSelection>,
    ) -> Result<Option<CommandReceipt>, StartRejection> {
        let runtime_epoch =
            preparation::parse_epoch(runtime_epoch).map_err(|error| error.into_parts())?;
        let generation =
            preparation::parse_generation(generation).map_err(|error| error.into_parts())?;
        // 认证与凭据签名/claims 绑定先于任何运行期读取（D3.1 的固定优先级）。
        let verified = preparations
            .verify(
                handle.device_id(),
                handle.canonical_origin(),
                receipt,
                session,
            )
            .await
            .map_err(preparation::receipt_rejection)?;

        // 同会话准备阶段串行：establish → claim → 核心受理不能被并发请求交错。
        let gate = self.start_gate(session);
        let _serial = gate.lock().await;

        if preparations
            .committed(session)
            .await
            .map_err(|error| port_rejection(&error).into_parts())?
            .is_some()
        {
            return Err((
                ErrorCode::CommandIdempotencyConflict,
                "the first submission was already accepted for this session",
                RawObject::empty(),
            ));
        }

        let view = match preparations.runtime(session, &runtime_epoch, generation, true) {
            RuntimeCheck::Stale(reason) => return Err(preparation::stale_rejection(reason)),
            RuntimeCheck::Ready(view) => view,
        };
        if !preparations.matches_runtime(&verified, &view) {
            return Err(binding_rejection());
        }
        let now = preparations.now();
        match preparations.directory().freeze_for_submission(
            session,
            generation,
            &runtime_epoch,
            now.clone(),
        ) {
            PreparationFreeze::Granted(_) => {}
            PreparationFreeze::Busy => {
                return Err((
                    ErrorCode::SessionBusy,
                    "another first submission is in progress",
                    RawObject::empty(),
                ));
            }
            PreparationFreeze::Stale(reason) => return Err(preparation::stale_rejection(reason)),
            PreparationFreeze::Unknown => {
                return Err(preparation::stale_rejection(
                    PreparationStaleReason::Released,
                ));
            }
        };

        let selection = preparation::selection_input(mode, config_options)
            .map_err(|error| error.into_parts())?;
        // 复核并在该资源代上重新应用草稿选择：以 Agent 当场确认为准，失败不派发。
        let options = match endpoints
            .configure(PreparationConfigure {
                session: session.clone(),
                generation: generation.get(),
                selection: selection.clone(),
            })
            .await
        {
            Ok(options) => options,
            Err(error) => {
                let _ = preparations.directory().abort_submission(
                    session,
                    generation,
                    preparations.now(),
                );
                return Err(port_rejection(&error).into_parts());
            }
        };
        let (selected_mode, selected_config) =
            match submission_selection(&options, mode, config_options) {
                Ok(selection) => selection,
                Err(rejection) => {
                    // D8：选择被拒时撤销冻结并释放临时端点，保持同 ID 可重试。
                    let _ = preparations.directory().abort_submission(
                        session,
                        generation,
                        preparations.now(),
                    );
                    let _ = endpoints.release(session.clone(), generation.get()).await;
                    return Err(rejection);
                }
            };
        let endpoint = match endpoints.claim(session.clone(), generation.get()).await {
            Ok(endpoint) => endpoint,
            Err(error) => {
                let _ = preparations.directory().abort_submission(
                    session,
                    generation,
                    preparations.now(),
                );
                return Err(port_rejection(&error).into_parts());
            }
        };

        let workspace = ResolvedWorkspace::at_path(verified.workspace_path.clone())
            .map_err(|_| binding_rejection())?;
        let agent = AgentRef::try_new(verified.agent.clone(), verified.agent.as_str())
            .map_err(|_| binding_rejection())?;
        let manifest = match submission_id.as_ref() {
            Some(submission) => Some(SubmissionManifestClaim {
                submission: SubmissionId::new(submission.as_str())
                    .map_err(|_| binding_rejection())?,
                runtime_epoch: runtime_epoch.clone(),
                generation,
            }),
            None => None,
        };
        let mut blocks = Vec::with_capacity(content.as_slice().len());
        for block in content.as_slice() {
            let text = serde_json::to_string(block)
                .map_err(|_| schema_rejection("the prompt block cannot be re-encoded"))?;
            blocks.push(
                PromptContentBlock::from_json_text(&text).map_err(|_| {
                    schema_rejection("the prompt block is not a valid content block")
                })?,
            );
        }
        let start = FirstSubmissionStart {
            session: session.clone(),
            request: request_id.clone(),
            fingerprint: fingerprint.clone(),
            agent,
            workspace_cwd: Some(workspace.canonical_path().to_owned()),
            workspace_directory: Some(workspace.directory_id().clone()),
            mode: selected_mode,
            config: selected_config,
            content: blocks,
            manifest,
            generation,
        };
        let receipt = match self
            .core
            .start_first_submission(actor, start, endpoint)
            .await
        {
            Ok(receipt) => receipt,
            Err(error) => {
                let _ = preparations.directory().abort_submission(
                    session,
                    generation,
                    preparations.now(),
                );
                let _ = endpoints.release(session.clone(), generation.get()).await;
                return Err(port_rejection(&error).into_parts());
            }
        };
        match &receipt {
            CommandReceipt::Accepted { .. } => {
                // 受理事务已提交：名额转给正式会话（准备条目不再可观察）。
                let _ = preparations.directory().finish_submission(
                    session,
                    generation,
                    preparations.now(),
                );
            }
            CommandReceipt::Rejected { .. } => {
                let _ = preparations.directory().abort_submission(
                    session,
                    generation,
                    preparations.now(),
                );
                let _ = endpoints.release(session.clone(), generation.get()).await;
            }
        }
        Ok(Some(receipt))
    }

    /// `kind: continue`：准入 → 端点重建/复核 → 核心派发围栏（新 requestId + `retryOfRequestId`）。
    #[allow(clippy::too_many_arguments)] // 与 wire 的闭合 `continue` 分支字段一一对应
    async fn start_continue(
        &self,
        handle: &ConnectionHandle,
        actor: &Actor,
        preparations: &Arc<Preparations>,
        endpoints: &Arc<dyn crate::sync::preparation::PreparationEndpoints>,
        session: &SessionId,
        request_id: &CoreRequestId,
        fingerprint: &Digest,
        first_submission_id: &Uuid,
        retry_of_request_id: &Uuid,
        mode: &Option<sync_protocol::common::NonEmptyText<256>>,
        config_options: &Option<sync_protocol::command::PreparationConfigSelection>,
    ) -> Result<Option<CommandReceipt>, StartRejection> {
        let gate = self.start_gate(session);
        let _serial = gate.lock().await;
        let Some(record) = self
            .core
            .first_submission(session)
            .await
            .map_err(|error| port_rejection(&error).into_parts())?
        else {
            return Err((
                ErrorCode::CommandIdempotencyConflict,
                "the session has no first submission",
                RawObject::empty(),
            ));
        };
        if record.first_submission_id.as_str() != first_submission_id.as_str() {
            return Err((
                ErrorCode::CommandIdempotencyConflict,
                "firstSubmissionId does not match this session",
                RawObject::empty(),
            ));
        }
        // D8.1 顺序：成功终态 → 可能已派发 → 仅失败且可证明未派发才允许继续。
        if record
            .terminal
            .as_ref()
            .is_some_and(|terminal| terminal.outcome == RequestOutcome::Completed)
        {
            return Err((
                ErrorCode::CommandIdempotencyConflict,
                "the first submission already has a terminal result",
                RawObject::empty(),
            ));
        }
        // `uncertain` 不是 failed：说不清是否已发出，不得重建、不得派发（D8.1）。
        if record.ever_dispatched
            || record.stage == FirstSubmissionStage::DispatchStarted
            || record
                .terminal
                .as_ref()
                .is_some_and(|terminal| terminal.outcome == RequestOutcome::Uncertain)
        {
            return Err((
                ErrorCode::CommandUncertain,
                "the first submission may already have been dispatched",
                RawObject::empty(),
            ));
        }
        match record.terminal.as_ref() {
            Some(terminal) if terminal.outcome == RequestOutcome::Failed => {
                if terminal.request_id.as_str() != retry_of_request_id.as_str() {
                    return Err((
                        ErrorCode::CommandIdempotencyConflict,
                        "retryOfRequestId is not the failed attempt of this session",
                        RawObject::empty(),
                    ));
                }
            }
            _ => {
                return Err((
                    ErrorCode::CommandIdempotencyConflict,
                    "only a provably-not-dispatched failure can be continued",
                    RawObject::empty(),
                ));
            }
        }
        if record.current_request_id.as_str() != retry_of_request_id.as_str() {
            return Err((
                ErrorCode::CommandIdempotencyConflict,
                "retryOfRequestId does not reference the latest attempt",
                RawObject::empty(),
            ));
        }
        let retry_of = CoreRequestId::new(retry_of_request_id.as_str())
            .map_err(|_| schema_rejection("retryOfRequestId is not a request id"))?;
        // 认证先于目录/端点读取：设备 revoked/unknown 时停止。
        preparations
            .trust()
            .snapshot(handle.device_id())
            .await
            .map_err(|error| port_rejection(&error).into_parts())?;

        let (agent, workspace) = self.session_workspace(actor, session).await?;
        let selection = continue_selection(&record, mode, config_options)?;
        let generation = record
            .resource_generation
            .unwrap_or_else(|| ResourceGeneration::new(1));
        let established = endpoints
            .establish(PreparationEstablish {
                session: session.clone(),
                agent,
                workspace,
                generation: generation.get(),
                selection: selection.clone(),
            })
            .await;
        let options = match established {
            Ok(options) => options,
            Err(error) => {
                // D8.1：端点建立/配置复核失败写该次尝试的 failed 终态，正文与首条身份不变。
                self.settle_failed_continue(actor, session, request_id, fingerprint, &error)
                    .await;
                return Err(port_rejection(&error).into_parts());
            }
        };
        if let Err(rejection) = validate_continue_selection(&options, &selection) {
            let error = PortError::InvalidRequest("the preparation selection is not available");
            self.settle_failed_continue(actor, session, request_id, fingerprint, &error)
                .await;
            let _ = endpoints.release(session.clone(), generation.get()).await;
            return Err(rejection);
        }
        let endpoint = match endpoints.claim(session.clone(), generation.get()).await {
            Ok(endpoint) => endpoint,
            Err(error) => {
                self.settle_failed_continue(actor, session, request_id, fingerprint, &error)
                    .await;
                return Err(port_rejection(&error).into_parts());
            }
        };
        let receipt = match self
            .core
            .continue_first_submission(
                actor,
                FirstSubmissionContinue {
                    session: session.clone(),
                    request: request_id.clone(),
                    fingerprint: fingerprint.clone(),
                    first_submission_id: record.first_submission_id.clone(),
                    retry_of,
                    generation,
                    endpoint,
                },
            )
            .await
        {
            Ok(receipt) => receipt,
            Err(error) => {
                let _ = endpoints.release(session.clone(), generation.get()).await;
                return Err(port_rejection(&error).into_parts());
            }
        };
        if matches!(receipt, CommandReceipt::Rejected { .. }) {
            // 准入在核心被拒：本请求没有跨越派发围栏，释放本次新建的端点。
            let _ = endpoints.release(session.clone(), generation.get()).await;
        }
        Ok(Some(receipt))
    }

    /// 显式继续的准备阶段失败：写本次尝试的 `failed` 终态（唯一 writer 在存储层）。
    async fn settle_failed_continue(
        &self,
        actor: &Actor,
        session: &SessionId,
        request_id: &CoreRequestId,
        fingerprint: &Digest,
        error: &PortError,
    ) {
        let (code, message, details) = port_rejection(error).into_parts();
        let Ok(view) = ViewJson::new(details.get()) else {
            return;
        };
        let Ok(public) =
            CorePublicError::try_new(code.as_str(), message, code.default_retryable(), view)
        else {
            return;
        };
        let settlement = FirstSubmissionSettlement {
            session: session.clone(),
            actor: actor.clone(),
            request_id: request_id.clone(),
            request_fingerprint: fingerprint.clone(),
            outcome: RequestOutcome::Failed,
            error: Some(public),
            at: self.authority.now(),
        };
        if self.core.settle_first_submission(settlement).await.is_err() {
            debug!(
                event = "sync.session_start_settlement_failed",
                request_id = request_id.as_str(),
                "the failed continue attempt could not be persisted"
            );
        }
    }

    /// 显式继续所需的 Agent 与工作目录（只读持久事实；缺一即不可继续）。
    async fn session_workspace(
        &self,
        actor: &Actor,
        session: &SessionId,
    ) -> Result<(AgentRef, ResolvedWorkspace), StartRejection> {
        let sessions = self
            .core
            .sync_visible_sessions(
                actor,
                SyncSessionQuery {
                    owned: SessionQuery::default(),
                    remote: None,
                },
                |_| false,
            )
            .await
            .map_err(|error| port_rejection(&error).into_parts())?;
        let summary = sessions
            .into_iter()
            .find(|summary| summary.session_id() == session)
            .ok_or((
                ErrorCode::SessionNotFound,
                "the session is not visible to this device",
                RawObject::empty(),
            ))?;
        let directory = summary
            .workspace()
            .map(|workspace| workspace.directory_id().clone())
            .ok_or((
                ErrorCode::InternalUnavailable,
                "the session has no recorded workspace",
                RawObject::empty(),
            ))?;
        // 目录路径必须经**设备可读**的解析入口取得：`UseCases::directory` 是本机管理读面
        // （`Actor::LocalCli` 守卫），拿它解析会让任何设备的继续都以 `authorization.scope_denied`
        // 失败。这里用与准备 allocate 同一个解析入口（已认证设备可读、只认已登记目录）。
        let resolved = self
            .core
            .resolve_preparation_workspace(
                actor,
                acp_core::use_cases::PreparationWorkspaceSource::Directory(directory.clone()),
            )
            .await
            .map_err(|error| match error {
                PortError::InvalidRequest(_) => (
                    ErrorCode::SessionNotFound,
                    "the session workspace is no longer registered",
                    RawObject::empty(),
                ),
                other => port_rejection(&other).into_parts(),
            })?;
        let workspace =
            ResolvedWorkspace::try_new(directory, resolved.workspace.canonical_path().to_owned())
                .map_err(|_| {
                (
                    ErrorCode::InternalUnavailable,
                    "the recorded workspace path is not usable",
                    RawObject::empty(),
                )
            })?;
        Ok((summary.agent().clone(), workspace))
    }

    /// 建立/复用完成后的终态落盘与回包。
    ///
    /// `Ok(())` 写 `completed { sessionId, session }`；`Err(failure)` 写 `failed`（可证明）或
    /// `uncertain`（说不清）。回包以持久记录为唯一权威（重试与 `command.status` 因此同形）。
    async fn finish_resume(
        &self,
        handle: &ConnectionHandle,
        actor: &Actor,
        command: &Command,
        session: &acp_core::model::SessionId,
        outcome: Result<(), ResumeFailure>,
    ) {
        let Some(request_id) = core_request_id(&command.request_id) else {
            self.encoding_fault(handle);
            return;
        };
        let features = self.features(handle);
        let persisted = self
            .settle_resume_for(actor, &request_id, &features, session, outcome.clone())
            .await;
        if !persisted {
            // 落盘没成功：以持久记录为唯一事实，区分三种形状（不伪造 completed）。
            match self.core.command_status(actor, request_id.clone()).await {
                Ok(Some(record)) if record.status().is_terminal() => {
                    // 已经由另一条路径收口：回首次终态。
                    self.send_record_terminal(
                        handle,
                        &command.request_id,
                        command.command,
                        &record,
                    );
                }
                Ok(Some(record)) => {
                    // 受理行存在但终态没落盘：无法证明结果，回显式 uncertain（带首次受理时间）。
                    debug!(
                        event = "sync.session_resume_terminal_unsettled",
                        connection_id = handle.connection_id().as_str(),
                        request_id = command.request_id.as_str(),
                        "the session.resume terminal could not be persisted"
                    );
                    self.send_resume_uncertain(handle, command, record.accepted_at());
                }
                Ok(None) => {
                    // **没有受理行** = 本次在受理之前就失败（没有恢复数据、cwd 复校验失败、
                    // 授权/幂等冲突）：可证明本机没有任何副作用，必须回 `rejected` + 分类错误码，
                    // 不能借用 `command.uncertain` —— 那会让客户端以为有一条已受理、待核实的命令。
                    let failure = match &outcome {
                        Err(failure) => failure.clone(),
                        Ok(()) => ResumeFailure {
                            code: ErrorCode::InternalUnavailable,
                            message: "the session resume outcome cannot be confirmed",
                            retryable: true,
                            uncertain: true,
                        },
                    };
                    let code = if failure.code == ErrorCode::CommandUncertain {
                        ErrorCode::InternalUnavailable
                    } else {
                        failure.code
                    };
                    self.reject_resume(handle, command, code, failure.message);
                }
                Err(_) => {
                    // 记录读不回来：不得回 `rejected`（它断言零副作用），保持 uncertain。
                    self.send_resume_uncertain(handle, command, None);
                }
            }
            return;
        }
        self.reply_resume_record(handle, command, actor, &request_id)
            .await;
    }

    /// 把一条等待者（自己的 `accepted` 记录）按建立者的结果收口；观察者随后把终态推回对端。
    async fn finish_resume_waiters(
        &self,
        session: &acp_core::model::SessionId,
        waiters: Vec<ResumeWaiter>,
        outcome: Result<(), ResumeFailure>,
    ) {
        for waiter in waiters {
            let persisted = self
                .settle_resume_for(
                    &waiter.actor,
                    &waiter.request_id,
                    &waiter.features,
                    session,
                    outcome.clone(),
                )
                .await;
            if !persisted {
                debug!(
                    event = "sync.session_resume_waiter_unsettled",
                    session_id = session.as_str(),
                    request_id = waiter.request_id.as_str(),
                    "a merged session.resume waiter could not be persisted"
                );
            }
        }
    }

    /// 计算并写入一条 `session.resume` 的终态；返回是否真的写入了记录。
    async fn settle_resume_for(
        &self,
        actor: &Actor,
        request_id: &CoreRequestId,
        features: &NegotiatedFeatures,
        session: &acp_core::model::SessionId,
        outcome: Result<(), ResumeFailure>,
    ) -> bool {
        let (status, result, error) = match &outcome {
            Ok(()) => match self.resume_result(actor, session, features).await {
                Some(projected) => {
                    let command_result = serde_json::to_string(&projected).ok().and_then(|text| {
                        acp_core::model::CommandResult::from_json_text(&text).ok()
                    });
                    (
                        acp_core::model::CommandStatus::Completed,
                        command_result,
                        None,
                    )
                }
                // 恢复已成功但摘要投影不出来：不能用 `failed` 撒谎，按契约收敛为 `uncertain`。
                None => {
                    let failure = ResumeFailure {
                        code: ErrorCode::CommandUncertain,
                        message: "the resumed session cannot be projected",
                        retryable: false,
                        uncertain: true,
                    };
                    (
                        acp_core::model::CommandStatus::Uncertain,
                        None,
                        Some(core_public_error(&failure)),
                    )
                }
            },
            Err(failure) => (
                if failure.uncertain {
                    acp_core::model::CommandStatus::Uncertain
                } else {
                    acp_core::model::CommandStatus::Failed
                },
                None,
                Some(core_public_error(failure)),
            ),
        };
        matches!(
            self.core
                .settle_session_resume(actor, request_id, status, result, error)
                .await,
            Ok(true)
        )
    }

    /// 以持久记录回包；记录不可读/未终结时回显式 `uncertain`（不伪造 completed）。
    async fn reply_resume_record(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        actor: &Actor,
        request_id: &CoreRequestId,
    ) {
        match self.core.command_status(actor, request_id.clone()).await {
            Ok(Some(record)) if record.status().is_terminal() => {
                self.send_record_terminal(handle, &command.request_id, command.command, &record);
            }
            Ok(Some(record)) => self.send_resume_uncertain(handle, command, record.accepted_at()),
            _ => self.send_resume_uncertain(handle, command, None),
        }
    }

    /// 一条显式的 `uncertain`：`acceptedAt` 取持久记录的首次受理时间；记录不可读时退回本机时钟。
    fn send_resume_uncertain(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        accepted_at: Option<&CoreTimestamp>,
    ) {
        let Some(error) = public_error(
            ErrorCode::CommandUncertain,
            "the session resume outcome cannot be confirmed",
            RawObject::empty(),
        ) else {
            return;
        };
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
                status: CommandState::Uncertain,
                accepted_at: Nullable::value(accepted_at),
                terminal_event_id: Nullable::null(),
                result: Nullable::null(),
                error: Nullable::value(error),
            },
            "mutation",
        );
    }

    /// 激活成功后的 `sessionResumeResult`（摘要与 `session.list` 同源，含协商后的 `localActivity`）。
    async fn resume_result(
        &self,
        actor: &Actor,
        session: &acp_core::model::SessionId,
        features: &NegotiatedFeatures,
    ) -> Option<sync_protocol::command::SessionResumeResult> {
        let summaries = self
            .core
            .list_sessions(actor, acp_core::ports::SessionQuery::default())
            .await
            .ok()?;
        let summary = summaries
            .iter()
            .find(|summary| summary.session_id() == session)?;
        let activity = match (&self.lifecycle, features.contains(LOCAL_LIFECYCLE_FEATURE)) {
            (Some(lifecycle), true)
                if matches!(summary.origin(), acp_core::model::ResourceOrigin::Local) =>
            {
                crate::sync::local_lifecycle::activity_of(lifecycle, actor, session, true, true)
                    .await
            }
            _ => None,
        };
        Some(sync_protocol::command::SessionResumeResult {
            session_id: Uuid::parse(session.as_str()).ok()?,
            session: self::query::wire_session(
                summary,
                features.contains(LOCAL_CATALOG_FEATURE),
                activity,
            )?,
        })
    }

    fn resume_failure(&self, error: &PortError) -> ResumeFailure {
        match error {
            PortError::Conflict(ConflictKind::IdempotencyConflict) => ResumeFailure {
                code: ErrorCode::CommandIdempotencyConflict,
                message: "the requestId was already used with different parameters",
                retryable: false,
                uncertain: false,
            },
            PortError::Unavailable(UnavailableKind::BackendUnsupported) => ResumeFailure {
                code: ErrorCode::CapabilityUnsupportedByAgent,
                message: "the agent cannot resume this session",
                retryable: false,
                uncertain: false,
            },
            // agent-host proves the attempt established no session (including an explicit
            // Agent refusal). Preserve that fact so retries do not get stuck querying uncertain.
            PortError::Unavailable(UnavailableKind::SessionNotEstablished) => ResumeFailure {
                code: ErrorCode::InternalUnavailable,
                message: "会话恢复未成功建立：Agent 已拒绝恢复，或恢复请求未能发出；请检查 Agent 的原会话恢复数据。",
                retryable: true,
                uncertain: false,
            },
            PortError::NotFound(_) => ResumeFailure {
                code: ErrorCode::SessionNotFound,
                message: "the session is not available on this host",
                retryable: false,
                uncertain: false,
            },
            // 其余失败无法证明「没有建立外部资源」：按 uncertain 收口，不自动重派。
            _ => unconfirmed_resume_failure(),
        }
    }

    fn resume_rejected_by_port(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        error: &PortError,
    ) -> RouteOutcome {
        match error {
            PortError::Conflict(ConflictKind::IdempotencyConflict) => self.reject_resume(
                handle,
                command,
                ErrorCode::CommandIdempotencyConflict,
                "the requestId was already used with different parameters",
            ),
            PortError::Unavailable(UnavailableKind::BackendUnsupported) => self.reject_resume(
                handle,
                command,
                ErrorCode::CapabilityUnsupportedByAgent,
                "the agent cannot resume this session",
            ),
            PortError::NotFound(_) => self.reject_resume(
                handle,
                command,
                ErrorCode::SessionNotFound,
                "the session is not available on this host",
            ),
            _ => self.reject_resume(
                handle,
                command,
                ErrorCode::CommandUncertain,
                "the session resume outcome cannot be confirmed",
            ),
        }
    }

    fn reject_resume(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        code: ErrorCode,
        message: &str,
    ) -> RouteOutcome {
        match public_error(code, message, RawObject::empty()) {
            Some(error) => self.send_rejected(handle, command, error),
            None => {
                self.encoding_fault(handle);
            }
        }
        RouteOutcome::Claimed
    }

    fn resume_gate(&self, session: &acp_core::model::SessionId) -> Arc<ResumeGate> {
        let mut gates = lock(&self.resume_gates);
        Arc::clone(
            gates
                .entry(session.as_str().to_owned())
                .or_insert_with(|| Arc::new(ResumeGate::new())),
        )
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
        lifecycle_visible: bool,
    ) -> Option<WireSessionSummary> {
        // 取值本身不使用：窗口期列表里还没有这条会话，本调用只落实那次授权判定。
        self.core
            .list_sessions(actor, acp_core::ports::SessionQuery::default())
            .await
            .ok()?;
        let activity = match (&self.lifecycle, lifecycle_visible) {
            (Some(lifecycle), true)
                if matches!(
                    prepared.summary.origin(),
                    acp_core::model::ResourceOrigin::Local
                ) =>
            {
                crate::sync::local_lifecycle::activity_of(
                    lifecycle,
                    actor,
                    prepared.summary.session_id(),
                    true,
                    true,
                )
                .await
            }
            _ => None,
        };
        self::query::wire_session(&prepared.summary, catalog, activity)
    }

    /// 回一条 `accepted` 的 `command.result`：`session.prompt` 带 `{turnId}`（客户端据此把本地
    /// 即时条目与随后的持久用户消息按稳定身份接替），其余命令的 `result` 与 `terminalEventId` 为 `null`。
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
        let result = if command.command == CommandName::SessionPrompt {
            turn.as_ref().and_then(|turn| {
                RawObject::parse(&format!(r#"{{"turnId":"{}"}}"#, turn.as_str()))
                    .ok()
                    .map(CommandResultPayload::Object)
            })
        } else {
            None
        };
        self.send_result(
            handle,
            &CommandResult {
                request_id: command.request_id.clone(),
                command: command.command,
                status: CommandState::Accepted,
                accepted_at: Nullable::value(accepted_at),
                terminal_event_id: Nullable::null(),
                result: result.map_or_else(Nullable::null, Nullable::value),
                error: Nullable::null(),
            },
            "mutation",
        );
    }

    /// 命令在 core 里已经留下 `accepted` 行但随后失败时，按「先受理、后终态」补发一次 accepted。
    ///
    /// 只在前置检查之后调用；自身不写任何记录（`command_status` 是只读查询）。没有持久行（例如
    /// 授权/解码/会话不存在类失败）时不发 accepted——那类拒绝本来就没有受理阶段。
    async fn send_accepted_if_persisted(
        &self,
        handle: &ConnectionHandle,
        command: &Command,
        actor: &Actor,
        request: &acp_core::model::RequestId,
    ) {
        let Ok(Some(record)) = self.core.command_status(actor, request.clone()).await else {
            return;
        };
        if record.status() == acp_core::model::CommandStatus::Accepted {
            // 首次落盘时间，不换成现场时钟（§11.2 的 `acceptedAt`）。
            self.send_accepted(handle, command, None, record.accepted_at());
        }
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
        self.send_record_terminal_with_turn(handle, request_id, name, record, None);
    }

    /// 同 [`Self::send_record_terminal`]，但额外把本次派发的 `turnId` 放进 `session.prompt` 的
    /// 终态 `result`：派发在受理返回前同步失败时，客户端只能拿到终态，仍需按稳定轮次接替本地条目。
    fn send_record_terminal_with_turn(
        &self,
        handle: &ConnectionHandle,
        request_id: &Uuid,
        name: CommandName,
        record: &acp_core::model::CommandRecord,
        turn: Option<&TurnId>,
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
            Some(result) if name == CommandName::SessionResume => {
                match serde_json::from_str::<sync_protocol::command::SessionResumeResult>(
                    result.as_str(),
                ) {
                    Ok(value) => {
                        Nullable::value(CommandResultPayload::SessionResume(Box::new(value)))
                    }
                    Err(_) => {
                        self.encoding_fault(handle);
                        return;
                    }
                }
            }
            // `session.start` 的 completed 是六字段首条投影（`sessionStartProjection`）。
            Some(result) if name == CommandName::SessionStart => {
                match serde_json::from_str::<SessionStartProjection>(result.as_str()) {
                    Ok(value) => Nullable::value(CommandResultPayload::SessionStart(value)),
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
        // 普通 prompt 的快速失败/重放只有终态：把本次（或回放到的）turnId 放进开放结果对象，
        // 客户端据此按稳定身份接替本地条目；记录已有结果时以记录为准。
        let result = if name == CommandName::SessionPrompt && record.result().is_none() {
            turn.and_then(|turn| {
                RawObject::parse(&format!(r#"{{"turnId":"{}"}}"#, turn.as_str())).ok()
            })
            .map_or_else(Nullable::null, |value| {
                Nullable::value(CommandResultPayload::Object(value))
            })
        } else {
            result
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

/// `ResumeFailure` → core 的 `PublicError`（终态落盘用）。已登记的错误码与消息必然合法；万一不合法，
/// 落成 `internal.unavailable` 而不是 panic 或伪造成功。
fn core_public_error(failure: &ResumeFailure) -> CorePublicError {
    CorePublicError::coded(failure.code.as_str(), failure.message, failure.retryable)
        .unwrap_or_else(|_| {
            CorePublicError::coded("internal.unavailable", "internal error", true)
                .expect("canonical error code and message")
        })
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

/// Owner 的远程记录 → wire `command.result`（`design.md` D3 的透明投影；Access 不重算任何字段）。
///
/// 三处不能省：
/// - `accepted` 的 `terminalEventId` 与 `result` 恒为 `null`（Owner 尚未终结）；
/// - 终态的 `terminalEventId` 是 **Owner** 的事件标识，不是本机事件（Access 不得伪造本地序号）；
/// - `acceptedAt` 优先用 Owner 记录给出的受理时间；Node Link 的 `command.terminal` 不携带它时，
///   用调用方给出的 Access 侧受理时刻补齐（Sync 只允许 `rejected` 为 `null`）。
fn remote_result(
    command: &Command,
    record: &RemoteCommandRecord,
    fallback_accepted_at: Option<&str>,
) -> Option<CommandResult> {
    let accepted_at = match record.accepted_at.as_ref() {
        Some(at) => Nullable::value(wire_timestamp(at.as_str())?),
        // Sync §11.2：只有 `rejected` 允许 `acceptedAt = null`。远程终态不携带 Owner 的受理时间时，
        // 用 Access 侧受理这次转发的时刻补齐（见 `send_remote_record`），不伪造 Owner 事实。
        None if !matches!(record.status, acp_core::model::CommandStatus::Rejected) => {
            match fallback_accepted_at {
                Some(stamp) => Nullable::value(wire_timestamp(stamp)?),
                None => Nullable::null(),
            }
        }
        None => Nullable::null(),
    };
    let terminal_event_id = match record.terminal_event_id.as_ref() {
        Some(event) => Nullable::value(Uuid::parse(event.as_str()).ok()?),
        None => Nullable::null(),
    };
    let result = match record.result.as_ref() {
        Some(result) => Nullable::value(CommandResultPayload::Object(
            RawObject::parse(result.as_str()).ok()?,
        )),
        None => Nullable::null(),
    };
    let error = match record.error.as_ref() {
        Some(error) => Nullable::value(wire_public_error(error)?),
        None => Nullable::null(),
    };
    Some(CommandResult {
        request_id: command.request_id.clone(),
        command: command.command,
        status: wire_command_state(record.status),
        accepted_at,
        terminal_event_id,
        result,
        error,
    })
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
        // 远程标题重放换了参数：Owner 明确拒绝为幂等冲突，不能被折叠成可重试的本机故障。
        PortError::Conflict(ConflictKind::IdempotencyConflict) => Rejection::Code(
            ErrorCode::CommandIdempotencyConflict,
            "the requestId was already used for a different command payload",
        ),
        // `session.create` 的容量准入（与 `session.resume` 共用同一本账）：这是**可重试**的明确
        // 拒绝，不是请求形状问题，也不是「说不清」的临时故障。
        PortError::InvalidRequest(acp_core::broker::SESSION_CAPACITY_EXCEEDED) => Rejection::Code(
            ErrorCode::SessionCapacityExceeded,
            "no active session slot is available right now",
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
        CommandPayload::SessionResume(_) => {
            return Err("session.resume is dispatched by its own handler");
        }
        // `session.start` 有自己的受理/继续用例（准备凭据、首条事务与派发围栏都不在通用 mutation 管道里）。
        CommandPayload::SessionStart(_) => {
            return Err("session.start is dispatched by its own handler");
        }
        // `session.title.set` 的标题在 wire 层已按 1..=512 个标量值校验，core 的值对象是同一规则的
        // 唯一实现：这里只做类型映射（trim 由 `SessionTitle::parse` 完成，保存值即 trim 后的文本）。
        CommandPayload::SessionTitleSet(set) => CorePayload::TitleSet {
            title: SessionTitle::parse(set.title.as_str())
                .map_err(|_| "the title is not within bounds")?,
        },
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

/// `session.start` 的拒绝三元组（与 [`Rejection::into_parts`] 同形）。
type StartRejection = (ErrorCode, &'static str, RawObject);

/// 凭据/claims 不绑定当前资源：统一折叠，不泄露具体失败字段。
fn binding_rejection() -> StartRejection {
    (
        ErrorCode::AttachmentBindingMismatch,
        "the preparation receipt does not bind this resource",
        RawObject::empty(),
    )
}

/// 形状错误。
fn schema_rejection(message: &'static str) -> StartRejection {
    (
        ErrorCode::ProtocolSchemaInvalid,
        message,
        RawObject::empty(),
    )
}

/// Agent 无法确认草稿选择（不存在的模式/配置，或配置未生效）。
fn capability_rejection(message: &'static str) -> StartRejection {
    (
        ErrorCode::CapabilityUnsupportedByAgent,
        message,
        RawObject::empty(),
    )
}

/// wire 的配置值 → core 值对象。
///
/// `select` 类型选项在 wire 上就是字符串（`{ <configId>: string }`），而 Agent 确认的当前值一律是
/// [`CoreConfigValue::Select`]（`agent-host` 把 ACP 的 `currentValue` 投影成 select，且显式拒绝
/// 文本取值）。因此字符串必须映射成 `select`，不能映射成 `text`——否则同一个草稿选择在准备控制帧
/// （`selection_input` 用的就是 `select`）通过，到了 `session.start` 却永远「无法应用」。
fn core_config_value(value: &WireConfigValue) -> Result<CoreConfigValue, StartRejection> {
    match value {
        WireConfigValue::Text(text) => CoreConfigValue::select(text.as_str())
            .map_err(|_| schema_rejection("the config value is out of range")),
        WireConfigValue::Boolean(flag) => Ok(CoreConfigValue::Boolean(*flag)),
    }
}

/// `kind=initial` 的模式/配置：必须在**本次** Agent 复核后的真实选项里存在且已生效。
fn submission_selection(
    options: &preparation::PreparationOptions,
    mode: &Option<sync_protocol::common::NonEmptyText<256>>,
    config_options: &Option<sync_protocol::command::PreparationConfigSelection>,
) -> Result<(Option<ModeRef>, Vec<FirstSubmissionConfig>), StartRejection> {
    if options.blocks_dispatch {
        return Err(capability_rejection(
            "the preparation has an unavailable selection",
        ));
    }
    let selected_mode = match mode {
        Some(selected) => {
            let id = ModeId::new(selected.as_str())
                .map_err(|_| schema_rejection("the mode is out of range"))?;
            if options.modes.current_mode.as_ref().map(ModeRef::mode_id) != Some(&id) {
                return Err(capability_rejection(
                    "the selected mode is not available on the agent",
                ));
            }
            options.modes.current_mode.clone()
        }
        None => options.modes.current_mode.clone(),
    };
    let mut config = Vec::new();
    if let Some(selection) = config_options {
        for (id, value) in selection.as_map() {
            let id = ConfigOptionId::new(id)
                .map_err(|_| schema_rejection("the config id is out of range"))?;
            let option = options
                .config
                .iter()
                .find(|option| option.id() == &id)
                .ok_or_else(|| {
                    capability_rejection("the selected configuration is not available on the agent")
                })?;
            if option.current() != &core_config_value(value)? {
                return Err(capability_rejection(
                    "the selected configuration could not be applied",
                ));
            }
            config.push(FirstSubmissionConfig {
                id,
                value: option.current().clone(),
            });
        }
    }
    Ok((selected_mode, config))
}

/// `kind=continue` 的选择：载荷给出则用载荷，否则沿用受理记录（D8.1）。
fn continue_selection(
    record: &acp_core::model::FirstSubmissionRecord,
    mode: &Option<sync_protocol::common::NonEmptyText<256>>,
    config_options: &Option<sync_protocol::command::PreparationConfigSelection>,
) -> Result<preparation::PreparationSelectionInput, StartRejection> {
    let mode = match mode {
        Some(selected) => Some(
            ModeId::new(selected.as_str())
                .map_err(|_| schema_rejection("the mode is out of range"))?,
        ),
        None => record.mode_id.clone(),
    };
    let mut config = Vec::new();
    match config_options {
        Some(selection) => {
            for (id, value) in selection.as_map() {
                config.push((
                    ConfigOptionId::new(id)
                        .map_err(|_| schema_rejection("the config id is out of range"))?,
                    core_config_value(value)?,
                ));
            }
        }
        None => {
            if !record.config.as_str().is_empty() {
                let parsed: serde_json::Value = serde_json::from_str(record.config.as_str())
                    .map_err(|_| schema_rejection("the recorded configuration is not readable"))?;
                if let serde_json::Value::Object(entries) = parsed {
                    for (id, value) in entries {
                        let id = ConfigOptionId::new(&id)
                            .map_err(|_| schema_rejection("the config id is out of range"))?;
                        let value = match value {
                            serde_json::Value::String(text) => CoreConfigValue::text(&text)
                                .map_err(|_| {
                                    schema_rejection("the config value is out of range")
                                })?,
                            serde_json::Value::Bool(flag) => CoreConfigValue::Boolean(flag),
                            _ => {
                                return Err(schema_rejection(
                                    "the recorded configuration is not a string/boolean",
                                ));
                            }
                        };
                        config.push((id, value));
                    }
                }
            }
        }
    }
    Ok(preparation::PreparationSelectionInput { mode, config })
}

/// 继续的端点复核：选中的模式/配置必须与 Agent 当场确认的值一致。
fn validate_continue_selection(
    options: &preparation::PreparationOptions,
    selection: &preparation::PreparationSelectionInput,
) -> Result<(), StartRejection> {
    if options.blocks_dispatch {
        return Err(capability_rejection(
            "the preparation has an unavailable selection",
        ));
    }
    if let Some(mode) = &selection.mode {
        if options.modes.current_mode.as_ref().map(ModeRef::mode_id) != Some(mode) {
            return Err(capability_rejection(
                "the selected mode is not available on the agent",
            ));
        }
    }
    for (id, value) in &selection.config {
        let option = options
            .config
            .iter()
            .find(|option| option.id() == id)
            .ok_or_else(|| {
                capability_rejection("the selected configuration is not available on the agent")
            })?;
        if option.current() != value {
            return Err(capability_rejection(
                "the selected configuration could not be applied",
            ));
        }
    }
    Ok(())
}

/// 载荷指纹（ACPR-CJ1 之后的 SHA-256，§6 第 6 条）——幂等冲突判定的唯一依据。
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
mod start_tests;

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
            target: ContentTarget::Session(SessionId::new(session).expect("session id")),
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
        assert!(binding.matches(
            &device,
            &ContentTarget::Session(session.clone()),
            "src/main.rs",
            "version_1"
        ));

        // 另一个连接 = 另一台设备：同一份版本令牌不得当 bearer 使用。
        let other_device = acp_core::model::DeviceId::new("04dbad03-8869-4c5a-94fa-2f3af2fb622d")
            .expect("device id");
        assert!(!binding.matches(
            &other_device,
            &ContentTarget::Session(session.clone()),
            "src/main.rs",
            "version_1"
        ));
        // 同一设备上的另一个会话。
        let other_session =
            SessionId::new("04dbad03-8869-4c5a-94fa-2f3af2fb622e").expect("session id");
        assert!(!binding.matches(
            &device,
            &ContentTarget::Session(other_session.clone()),
            "src/main.rs",
            "version_1"
        ));
        // 同一会话的另一个路径。
        assert!(!binding.matches(
            &device,
            &ContentTarget::Session(session.clone()),
            "src/lib.rs",
            "version_1"
        ));
        // 另一个版本。
        assert!(!binding.matches(
            &device,
            &ContentTarget::Session(session.clone()),
            "src/main.rs",
            "version_2"
        ));
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

    /// 远程（Access 侧）`session.title.set` 的记录投影：`accepted` 与终态两种形状都只做透明转发。
    ///
    /// 变红点：①`accepted` 带上终态事件或结果（伪造「已完成」）；②终态的 `terminalEventId` 被换成
    /// `null` 或本机序号；③终态 `failed` 不携带 Owner 的 error；④`acceptedAt` 被本地时钟补出来。
    #[test]
    fn a_remote_title_record_projects_accepted_and_terminal_without_local_facts() {
        use acp_core::ports::RemoteCommandTarget;

        let body = r#"{
            "requestId": "0b7f5c1e-2a4d-4e6f-8a11-c3d5e7f90123",
            "command": "session.title.set",
            "sessionId": "1c8e6d2f-3b5e-4f70-9b22-d4e6f8031245",
            "payload": { "title": "标题" }
        }"#;
        let command: Command = serde_json::from_str(body).expect("合法 command body");
        let target = RemoteCommandTarget {
            owner_node_id: acp_core::model::NodeId::new("3d9f7e30-4c6f-4182-ac33-e5f709142356")
                .expect("owner id"),
            session_id: Some(
                acp_core::model::SessionId::new("1c8e6d2f-3b5e-4f70-9b22-d4e6f8031245")
                    .expect("session id"),
            ),
        };
        let record = |status: acp_core::model::CommandStatus,
                      accepted_at: Option<CoreTimestamp>,
                      terminal_at: Option<CoreTimestamp>,
                      terminal_event_id: Option<acp_core::model::EventId>,
                      result: Option<acp_core::model::CommandResult>,
                      error: Option<CorePublicError>| {
            RemoteCommandRecord {
                target: target.clone(),
                request: CoreRequestId::new("0b7f5c1e-2a4d-4e6f-8a11-c3d5e7f90123")
                    .expect("request"),
                status,
                accepted_at,
                terminal_at,
                terminal_event_id,
                result,
                error,
            }
        };

        // ① Owner 只回了 accepted：终态字段一律为 null。
        let accepted = remote_result(
            &command,
            &record(
                acp_core::model::CommandStatus::Accepted,
                Some(CoreTimestamp::new("2026-10-21T00:00:01.000Z").expect("时间戳")),
                None,
                None,
                None,
                None,
            ),
            Some("2026-10-21T00:00:01.000Z"),
        )
        .expect("accepted 记录可投影");
        assert_eq!(accepted.status, CommandState::Accepted);
        assert_eq!(accepted.request_id.as_str(), command.request_id.as_str());
        assert_eq!(accepted.command, CommandName::SessionTitleSet);
        assert!(accepted.accepted_at.as_ref().is_some());
        assert!(accepted.terminal_event_id.as_ref().is_none());
        assert!(accepted.result.as_ref().is_none());
        assert!(accepted.error.as_ref().is_none());

        // ② Owner 的终态：terminalEventId 原样透传，error 按 Owner 的记录投影。
        let terminal_event = "4e0a8f41-5d70-4293-bd44-f6081a253467";
        let failed = remote_result(
            &command,
            &record(
                acp_core::model::CommandStatus::Failed,
                None,
                Some(CoreTimestamp::new("2026-10-21T00:00:02.000Z").expect("时间戳")),
                Some(acp_core::model::EventId::new(terminal_event).expect("event id")),
                None,
                Some(
                    acp_core::model::PublicError::coded(
                        "internal.unavailable",
                        "the owner cannot serve the command",
                        false,
                    )
                    .expect("错误码"),
                ),
            ),
            Some("2026-10-21T00:00:02.000Z"),
        )
        .expect("终态记录可投影");
        assert_eq!(failed.status, CommandState::Failed);
        assert_eq!(
            failed
                .terminal_event_id
                .as_ref()
                .map(|event| event.as_str()),
            Some(terminal_event)
        );
        assert!(
            failed.accepted_at.as_ref().is_some(),
            "非 rejected 的 command.result 必须带 acceptedAt；Owner 终态缺该字段时由 Access 受理时刻补齐"
        );
        assert_eq!(
            failed.error.as_ref().map(|error| error.code.as_str()),
            Some("internal.unavailable")
        );
    }

    // -------------------------------------------------------------------------------------
    // ResumeGate：同会话建立的合并协议（§11.5/§11.8）
    // -------------------------------------------------------------------------------------

    /// 等待者只需要「身份 + 自己那条受理记录」：门收口时用它写终态。
    fn resume_waiter(request: &str) -> ResumeWaiter {
        ResumeWaiter {
            actor: Actor::LocalCli,
            request_id: CoreRequestId::new(request).expect("request id"),
            features: NegotiatedFeatures::new(Vec::new()),
        }
    }

    /// 显式 `uncertain` 的形状：可证明「已受理、结局说不清」，不是 `rejected` 也不是 `completed`。
    fn is_unconfirmed(outcome: &Result<(), ResumeFailure>) -> bool {
        matches!(
            outcome,
            Err(failure) if failure.uncertain && failure.code == ErrorCode::CommandUncertain
        )
    }

    /// 幂等重放（同 `requestId` 已有受理行、本次没有执行后端）的收口形状不得漂移：必须用
    /// `ActivationOutcome::Failed` 交还本代预占（core 语义是「可证明本次没有建立外部资源」，会释放名额
    /// 并把会话收敛回 `Idle`），持久记录/等待者收敛为显式 `uncertain`。
    ///
    /// 变红点：改回 `Ready` 或 `Uncertain`——前者伪造本次建立成功，后者永久保留占位，后续 resume 全落
    /// `InFlight`、名额永不归还（这正是本轮修复的悬空路径）。
    #[test]
    fn a_replayed_establishment_settles_failed_and_reports_uncertain() {
        let (activation, failure) = replayed_resume_settlement();
        assert!(failure.uncertain, "重放没有执行后端，结局最多是 uncertain");
        assert_eq!(failure.code, ErrorCode::CommandUncertain);
        match activation {
            ActivationOutcome::Failed { error } => {
                assert_eq!(error.code(), "command.uncertain");
            }
            ActivationOutcome::Ready | ActivationOutcome::Uncertain { .. } => {
                panic!(
                    "幂等重放必须用 Failed 交还预占，不得 Ready（伪造成功）或 Uncertain（保留占位）"
                );
            }
        }
    }

    /// 建立者在窗口内登记的等待者由它一次收口；门收口后不再留人，重放路径因此不会把名额与请求一起悬空。
    #[test]
    fn an_establishment_settles_every_waiter_and_leaves_no_one_queued() {
        let gate = ResumeGate::new();
        let epoch = gate.begin();
        assert!(
            gate.register(resume_waiter("6f1c2a10-0000-4000-8000-000000000001"))
                .is_none(),
            "活的建立者在场时等待者必须排队，由它收口"
        );
        assert!(
            gate.register(resume_waiter("6f1c2a10-0000-4000-8000-000000000002"))
                .is_none()
        );
        let (_, failure) = replayed_resume_settlement();
        let waiters = gate.publish(epoch, Err(failure));
        assert_eq!(waiters.len(), 2, "两个等待者都必须被本次建立者收口");
        assert!(
            lock(&gate.state).waiters.is_empty(),
            "门里不得留下永久停在 accepted 的等待者"
        );
        // 迟到的重放请求（建立者已收口）：立即显式失败，不排进一扇没人会关的门。
        let late = gate.register(resume_waiter("6f1c2a10-0000-4000-8000-000000000003"));
        assert!(is_unconfirmed(&late.expect("收口之后不得排队")));
        assert!(lock(&gate.state).waiters.is_empty());
    }

    /// 没有活的建立者（回收未确认 `Reclaiming` / 重启后只剩 `Activating` 的目录事实 / 从未建立）时，
    /// `InFlight` 的等待者必须**立即**拿到显式 `uncertain`：没有人会发布结果，排队等于永远 `accepted`。
    #[test]
    fn a_resume_without_a_live_establishment_fails_fast_instead_of_queueing() {
        let gate = ResumeGate::new();
        let immediate = gate
            .register(resume_waiter("7a2b3c40-0000-4000-8000-000000000001"))
            .expect("没有活的建立者时必须立即收口");
        assert!(is_unconfirmed(&immediate));
        assert!(
            lock(&gate.state).waiters.is_empty(),
            "不得为不存在的建立者排队"
        );

        // 建立者在场 → 结果已发布 → 又回到 `InFlight`（例如刚进入回收未确认）：`live` 已复位，
        // 新请求同样立即失败，而不是等一个不会再来的建立者。
        let epoch = gate.begin();
        gate.publish(epoch, Ok(()));
        let after = gate
            .register(resume_waiter("7a2b3c40-0000-4000-8000-000000000002"))
            .expect("结果发布之后不得排队");
        assert!(is_unconfirmed(&after));
        assert!(lock(&gate.state).waiters.is_empty());
    }

    /// 上一周期的成功结果不得被后代消费：`live` 复位后 `register` 一律 `uncertain`，即使门里还留着
    /// 上一轮的 `Ok(())`。晚到但同周期的等待者拿到显式失败（客户端可重试/查 `command.status`），
    /// 绝不把别人的 `completed` 冒充成这次的。
    #[test]
    fn a_stale_success_is_never_replayed_as_this_requests_completion() {
        let gate = ResumeGate::new();
        let epoch = gate.begin();
        assert!(
            gate.register(resume_waiter("8b3c4d50-0000-4000-8000-000000000001"))
                .is_none()
        );
        // 上一周期成功收口：结果留在门里，`live` 复位。
        assert_eq!(gate.publish(epoch, Ok(())).len(), 1);
        assert!(lock(&gate.state).outcome.is_some(), "旧结果确实还留在门里");

        // 后代（例如会话已 `ready` 后又进入 `reclaiming`）不得拿到那份 `Ok(())`。
        let stale = gate
            .register(resume_waiter("8b3c4d50-0000-4000-8000-000000000002"))
            .expect("不得排队");
        assert!(
            is_unconfirmed(&stale),
            "旧周期的 completed 不得被本请求消费"
        );

        // 下一次建立用 `begin` 清掉旧结果：新周期只由新建立者收口，不继承上一周期的结论。
        let next = gate.begin();
        assert_ne!(next, epoch, "每个建立周期都必须有独立的身份");
        assert!(
            lock(&gate.state).outcome.is_none(),
            "begin 必须清掉上一周期的结果"
        );
        assert!(
            gate.register(resume_waiter("8b3c4d50-0000-4000-8000-000000000003"))
                .is_none(),
            "新周期里等待者由新建立者收口"
        );
        assert_eq!(gate.publish(next, Ok(())).len(), 1);
    }

    /// 迟到的旧周期发布不得冒充下一代：建立者 R1 在 `settle_session` 之后、`publish` 之前有窗口，
    /// 此时 R2 已 `begin` 新周期（`live = true`）并登记了等待者 R3。若 `live` 只是无身份的布尔，
    /// R1 的 `publish` 会把新周期一起关掉、并用自己的旧结果收口 R3——本该 `completed` 的 resume 被写成
    /// `uncertain`。周期栅栏的要求：
    ///
    /// - 旧 epoch 的 `publish`：不得复位新周期的 `live`、不得把结果写进门、不得取走 R3；它只能收口
    ///   登记在它自己那一代的等待者（留在这里没人会再收口，会永久停在 `accepted`）。
    /// - 新 epoch 的 `publish`：正常收口 R3 并复位 `live`。
    ///
    /// 变红点：`publish` 忽略 epoch（旧行为）——第二次断言 `live` 仍为 `true`、R3 仍在队里，两者都会失败。
    #[test]
    fn a_stale_epoch_publish_cannot_close_the_current_cycle() {
        let gate = ResumeGate::new();
        let first = gate.begin();
        assert!(
            gate.register(resume_waiter("9c4d5e60-0000-4000-8000-000000000001"))
                .is_none(),
            "第一代的等待者由第一代建立者收口"
        );

        // R1 仍在收尾时 R2 已开启第二代并登记了 R3。
        let second = gate.begin();
        assert_ne!(second, first, "新周期必须拿到新的 epoch");
        assert!(
            gate.register(resume_waiter("9c4d5e60-0000-4000-8000-000000000002"))
                .is_none(),
            "新周期里的等待者同样排队"
        );

        // R1 的旧结果迟到：只收口它自己那一代，不得关掉新周期。
        let own = gate.publish(first, Ok(()));
        assert_eq!(own.len(), 1, "旧 epoch 只收口自己那一代的等待者");
        assert!(
            lock(&gate.state).live,
            "旧 epoch 的 publish 不得复位新周期的 live"
        );
        assert!(
            lock(&gate.state).outcome.is_none(),
            "旧结果不得写进新周期（会被当成本次的结论）"
        );
        assert_eq!(
            lock(&gate.state).waiters.len(),
            1,
            "新周期的等待者必须留给新建立者收口"
        );

        // 再迟到的同代发布不会再取走任何人。
        assert!(gate.publish(first, Ok(())).is_empty());

        // 新周期的建立者正常收口 R3 并复位 live。
        let settled = gate.publish(second, Ok(()));
        assert_eq!(settled.len(), 1, "新 epoch 的 publish 正常收口自己的等待者");
        assert!(!lock(&gate.state).live, "收口之后不得再有人排队");
        assert!(
            matches!(lock(&gate.state).outcome, Some(Ok(()))),
            "新周期记录的是自己的结果"
        );
        assert!(lock(&gate.state).waiters.is_empty());
    }
}
