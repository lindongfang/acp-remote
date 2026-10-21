//! 十条查询命令的用例面调用与 core → wire 投影（§11.2/§11.5；任务 2.3/2.4、3.1）。
//!
//! 四条边界：
//!
//! - **同步完成**：十条查询命令都在本模块内直接算出终态，返回**一次** `completed` 或 `rejected`，
//!   绝不先回 `accepted`（§11.2「查询命令不进入异步队列」）。结果的形状由命令名决定，逐条与
//!   `command.schema.json` 的 `allOf/if-then` 对齐。
//! - **授权已在调用方完成**：本模块只负责读取与投影；每条命令进入这里之前都已按设备 scope 判定过
//!   （见 [`super::CommandRoute::authorize`]），因此这里既不重算可见性，也不把未授权的读取当成「读出来
//!   再丢掉」。用例内部还会再授权一次（各查询用例的第一步都是同一处 `Broker::authorize`），那是同一
//!   判定点的第二次廉价判定，不是第二条判定路径。
//! - **feature 门控先于本机读取**：需要 feature 的两条（`session.read` 的 `history`、
//!   `workspace.browse`）都在调用用例**之前**判定协商结果，未协商即 `protocol.feature_required`——
//!   尤其 `workspace.browse` 的门控必须在触碰文件系统之前（spec `sync-workspace-browse`），因此它排在
//!   [`workspace_browse`] 的第一行，而不是留给用例去发现。
//! - **不可表达就显式失败，绝不伪装**：`session.read` 的三类明细资源（`turns`、`pending_interactions`、
//!   `capabilities`）在**当前 core 读面**上拿不到投影所需的取值（`SnapshotItemTurn.createdAt`、
//!   `SnapshotItemPendingInteraction.schema`、`SnapshotItemCapability.agentCapabilities` 在 core 的
//!   `Turn`/`PendingInteraction`/`CapabilitySet` 里都没有对应字段），因此请求它们一律以
//!   `capability.unsupported_by_broker` 显式拒绝（§11.3「Broker 无法表达时返回
//!   `capability.unsupported_by_broker`，不能伪装成功」），而不是静默省略该键、返回一份「看起来完整」
//!   的空资源。

use std::str::FromStr as _;
use std::sync::Arc;

use acp_core::broker::Broker;
use acp_core::model::{
    Actor, CommandRecord, CommandStatus, FileReadPurpose as CoreFileReadPurpose,
    GitDiff as CoreGitDiff, GitDiffBase as CoreGitDiffBase, GitDiffKind as CoreGitDiffKind,
    GitDiffUnavailableReason as CoreGitDiffUnavailable, GitFileStatus as CoreGitFileStatus,
    GitState as CoreGitState, GitStatusPage, MessageId, ModeRef as CoreModeRef, OwnedSessionRef,
    RemoteSessionRef, ResourceOrigin, Sequence, SessionFileChunk, SessionFileContentKind,
    SessionFileEntryKind as CoreEntryKind, SessionFilePage,
    SessionFileRestrictedReason as CoreRestrictedReason, SessionId, SessionReference,
    SessionState as CoreSessionState, Timestamp as CoreTimestamp,
};
use acp_core::ports::{HistoryInclude, MessageCursor};
use acp_core::use_cases::{ModeListing, SyncSessionQuery, UseCases};
use base64::Engine as _;
use sync_protocol::command::{
    Command, CommandResultPayload, CommandState, CommandStatus as WireCommandStatus,
    CommandStatusRecord, ConfigListResult, ContentVersion as WireContentVersion,
    FileReadPurpose as WireFileReadPurpose, GitDiffBase as WireGitDiffBase,
    GitDiffKind as WireGitDiffKind, GitDiffUnavailableReason as WireGitDiffUnavailable,
    GitFileStatus as WireGitFileStatus, GitState as WireGitState,
    GitStatusEntry as WireGitStatusEntry, IncludeList, SessionFileData,
    SessionFileEntry as WireSessionFileEntry, SessionFilesList, SessionFilesListResult,
    SessionFilesPath, SessionFilesRead, SessionFilesReadResult, SessionGitDiff,
    SessionGitDiffResult, SessionGitStatus, SessionGitStatusResult, SessionListResult,
    SessionPageToken, SessionRead, SessionRelativePath, WorkspaceBrowse, WorkspaceBrowseEntry,
    WorkspaceBrowseResult,
};
use sync_protocol::common::{
    DecimalString, ModeRef as WireModeRef, ModeState as WireModeState, NonEmptyText, Nullable,
    OriginBlock, RawObject, SessionAgentRef, SessionState as WireSessionState,
    SessionSummary as WireSessionSummary, Text, Timestamp, Uuid, WorkspaceRef as WireWorkspaceRef,
};
use sync_protocol::error::ErrorCode;
use tracing::debug;

use crate::session_read::{
    DEFAULT_PAGE_MESSAGES, MAX_PAGE_MESSAGES, SessionReadSource, wire_config_option,
};
#[cfg(test)]
use crate::session_read::{Message, MessageMerger, needs_older_agents};
use crate::sync::event::{FILE_DIFF_FEATURE, RAW_PAYLOAD_FEATURE, TYPED_MEDIA_FEATURE};
use crate::sync::remote::STATELESS_ORIGIN_FEATURE;
use crate::sync::snapshot::{LOCAL_CATALOG_FEATURE, REMOTE_ORIGIN_FEATURE};
use crate::sync::subscribe::cursor::CursorFault;
use crate::sync::subscribe::{NegotiatedFeatures, OwnerReachability};

use super::{core_request_id, details_object, port_rejection, wire_public_error, wire_timestamp};

/// `core.session-history.v1`：`session.read` 的 `include: ["history"]` 需要协商该 feature（§11.5）。
///
/// 未协商却请求了它时**显式**回 `protocol.feature_required`（`details.features` 给出缺的那个），
/// MUST NOT 静默忽略该 include 后返回一份看似完整的空结果。
pub const SESSION_HISTORY_FEATURE: &str = "core.session-history.v1";

/// `core.pending-interactions.v1`：`session.read` 的 `include: ["pending_interactions"]` 需要协商该
/// feature（§11.7）。未协商时显式回 `protocol.feature_required`，不得返回空数组。
pub const PENDING_INTERACTIONS_FEATURE: &str = "core.pending-interactions.v1";

/// core 表达「游标早于仍可读的下界」的那条静态文本（`storage-sqlite` 的 `read_session`：
/// `PortError::InvalidRequest("history cursor precedes the retained window")`）。
///
/// 这里给出本 crate 内的唯一一份取值，让 [`super::port_rejection`] 与 `session.read` 自己的消息游标判定
/// （见 [`session_read`]）收敛到同一个 [`Rejection::CursorInvalid`]：§11.5 要求对端看到
/// `sync.cursor_invalid` + `details.reason = "cursor_expired"`，落进通配臂则变成
/// `protocol.schema_invalid`——另一类事实（形状错误），会让客户端以为是自己发的帧不合法。
pub const RETAINED_WINDOW_EXCEEDED: &str = "history cursor precedes the retained window";

/// 一次查询的拒绝（调用方把它折成 `command.result` 的 `rejected` 终态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// 已登记的错误码 + 固定说明文本。
    Code(ErrorCode, &'static str),
    /// 游标不可用（`sync.cursor_invalid` + `details.reason`）。
    ///
    /// `reason` 是 registry 为这个码登记的**必需**字段（`compatibility/errors/v1/errors.json`），取值域
    /// 固定为 [`CursorFault`] 的四种原因。`session.read` 的两套游标——消息的 `before` 与历史条目的
    /// `beforeHistory`——早于保留窗口时都走这里（§11.5「保留窗口之前的游标显式不可读」）。
    CursorInvalid(CursorFault),
    /// body 形状不符（`protocol.schema_invalid`）。
    Schema(&'static str),
    /// 请求了未协商的 feature（`protocol.feature_required`，`details.features` 给出缺的那个）。
    ///
    /// 携带**缺的那一个** feature ID：登记的 `details.features` 因此不可能与它漂移，且多一条门控
    /// （`core.session-history.v1`、`core.workspace-browse.v1`）各自报出自己缺的那一条，不共用一份
    /// 写死的取值。
    FeatureRequired(&'static str),
    /// 请求了用例面无法表达的资源（`capability.unsupported_by_broker`）。
    UnsupportedResource(&'static str),
    /// Owner 不在线（imported 会话回源失败）：`resource.remote_unavailable` +
    /// `details.reason = "owner_offline"`。这是显式的「现在读不到」，不是 `session.not_found` 或空历史。
    RemoteUnavailable,
    /// `expectedVersion` 与当前会话版本不一致（`state.version_conflict`）。
    ///
    /// registry 为这个码登记的必需 `details.currentVersion` 就在这里给出：它是**读到的当前版本**，
    /// 不是客户端给的那个，否则客户端无法据此重试。
    VersionConflict(u64),
}

impl Rejection {
    /// 对端可见的错误码、它的固定说明（文本不含请求内容）与**已登记**的 `details`。
    ///
    /// `details` 与错误码在这里一起给出：登记的必需字段（`sync.cursor_invalid.reason`、
    /// `protocol.feature_required.features`）因此不可能与码本身漂移；未登记 `details` 的码一律 `{}`。
    pub fn into_parts(self) -> (ErrorCode, &'static str, RawObject) {
        match self {
            Rejection::Code(code, text) => (code, text, RawObject::empty()),
            Rejection::CursorInvalid(fault) => (
                ErrorCode::SyncCursorInvalid,
                "the cursor cannot be used",
                cursor_details(fault),
            ),
            Rejection::Schema(text) => (ErrorCode::ProtocolSchemaInvalid, text, RawObject::empty()),
            Rejection::FeatureRequired(feature) => (
                ErrorCode::ProtocolFeatureRequired,
                "the client did not negotiate a feature this resource requires",
                details_object(&serde_json::json!({ "features": [feature] })),
            ),
            Rejection::VersionConflict(current) => (
                ErrorCode::StateVersionConflict,
                "the expected version does not match the current session version",
                details_object(&serde_json::json!({ "currentVersion": current.to_string() })),
            ),
            Rejection::UnsupportedResource(resource) => {
                debug!(
                    event = "sync.session_read_resource_unsupported",
                    resource, "the include value cannot be projected from the use-case surface"
                );
                (
                    ErrorCode::CapabilityUnsupportedByBroker,
                    "the broker cannot express this resource",
                    RawObject::empty(),
                )
            }
            Rejection::RemoteUnavailable => (
                ErrorCode::ResourceRemoteUnavailable,
                "the owner node is not reachable",
                details_object(&serde_json::json!({ "reason": "owner_offline" })),
            ),
        }
    }
}

/// `sync.cursor_invalid.details`（唯一登记字段 `reason`，四种取值见 [`CursorFault`]）。
fn cursor_details(fault: CursorFault) -> RawObject {
    details_object(&serde_json::json!({ "reason": fault.as_str() }))
}

/// `session.list`：设备可见的会话摘要集合。
///
/// 可见性与 `sync.snapshot_*` 的 `sessions` 资源**同一条口径**（同一个 `sync_visible_sessions` 用例：
/// 自有 + 已导入合并、已按设备 `session.list` scope 过滤），已导入条目仅在协商
/// `resource.remote-origin.v1` 时纳入——未协商的客户端一行 imported 摘要都看不到。
pub async fn session_list(
    core: &Arc<UseCases>,
    owner: &Arc<dyn OwnerReachability>,
    actor: &Actor,
    features: &NegotiatedFeatures,
) -> Result<CommandResultPayload, Rejection> {
    let include_imported = features.contains(REMOTE_ORIGIN_FEATURE);
    let query = SyncSessionQuery {
        remote: include_imported.then(acp_core::use_cases::RemoteSessionQuery::default),
        ..SyncSessionQuery::default()
    };
    let summaries = core
        .sync_visible_sessions(actor, query, |remote: &RemoteSessionRef| {
            owner.reachable(&remote.owner_node_id)
        })
        .await
        .map_err(|error| port_rejection(&error))?;
    let catalog = features.contains(LOCAL_CATALOG_FEATURE);
    let sessions = summaries
        .iter()
        .map(|summary| wire_session(summary, catalog))
        .collect::<Option<Vec<_>>>()
        .ok_or(Rejection::Code(
            ErrorCode::InternalUnavailable,
            "a session summary cannot be projected",
        ))?;
    Ok(CommandResultPayload::SessionList(SessionListResult {
        sessions,
    }))
}

/// `session.read`：该会话的明细资源与分页标记（§11.5）。
///
/// 页语义（本工作包实现在 core 的**升序**读面之上）：
///
/// - `beforeHistory` 直接交给用例（core 的 `history` 一页天然是「省略游标 = 最新一页」）；
/// - `limit` 既是 `history` 一页的上限，也是本次要返回的**消息页**上限（§11.5「每一页各自的单页上限」）；
/// - 消息页按 `before`（`{createdAt, messageId}` 复合游标）取**紧接其之前**的一页，省略游标时取最新一页，
///   排序按 `createdAt` 升序、同一时刻以 `messageId` 稳定定序（§11.5 的排序规则）；
/// - 页标记 `hasEarlier` 只描述这一页**消息**之前是否仍有更早内容；
/// - **保留窗口之前的游标显式不可读**（§11.5）：`before` 早于本次读到的**最早一条可读消息**、且这一页
///   因此为空、页标记也说没有更早内容时，回 `sync.cursor_invalid`
///   （`details.reason = "cursor_expired"`）——绝不静默返回空页让客户端误以为已到最早一条。判定是
///   **严格**比较，因此「游标恰好是最早一条可读消息」（客户端翻到最早一条后再问一页的合法空页）不会
///   被误报。
///
/// 事件窗口必须覆盖该会话可读事件的全量，才能给出「最新一页」：本函数按 `next` 升序分批读到 head，
/// 超过 [`MAX_SCAN_EVENTS`] 仍未读完时以 `resource.result_too_large` 显式失败——宁可不给，也不给一页
/// 会被误当成最新页的**旧**内容（wire 上没有「还有更新的内容」这个页标记，静默返回旧页会让客户端以为
/// 已经到底而停止翻页）。
pub async fn session_read<C: SessionReadSource>(
    core: &C,
    actor: &Actor,
    command: &Command,
    payload: SessionRead,
    features: &NegotiatedFeatures,
) -> Result<CommandResultPayload, Rejection> {
    let Some(session) = command.session_id.as_ref() else {
        return Err(Rejection::Schema("session.read requires sessionId"));
    };
    let Some(session_id) = SessionId::new(session.as_str()).ok() else {
        return Err(Rejection::Schema("sessionId is not a session id"));
    };
    let include = history_include(&payload.include)?;
    if include.history && !features.contains(SESSION_HISTORY_FEATURE) {
        return Err(Rejection::FeatureRequired(SESSION_HISTORY_FEATURE));
    }
    if include.pending_interactions && !features.contains(PENDING_INTERACTIONS_FEATURE) {
        return Err(Rejection::UnsupportedResource("pending_interactions"));
    }
    for (requested, resource) in [
        (include.turns, "turns"),
        (include.capabilities, "capabilities"),
    ] {
        if requested {
            return Err(Rejection::UnsupportedResource(resource));
        }
    }
    let page_limit = payload
        .limit
        .as_ref()
        .map_or(DEFAULT_PAGE_MESSAGES, |limit| {
            u32::try_from(limit.get()).unwrap_or(MAX_PAGE_MESSAGES)
        })
        .clamp(1, MAX_PAGE_MESSAGES);
    let before_history = match payload.before_history.as_ref() {
        Some(value) => Some(parse_sequence(value.as_str())?),
        None => None,
    };
    let before_messages = match payload.before.as_ref() {
        Some(before) => Some(MessageCursor {
            created_at: CoreTimestamp::new(before.created_at.as_str())
                .map_err(|_| Rejection::Schema("before.createdAt is not a timestamp"))?,
            message_id: MessageId::new(before.message_id.as_str())
                .map_err(|_| Rejection::Schema("before.messageId is not a message id"))?,
        }),
        None => None,
    };
    let request = crate::session_read::SessionReadRequest {
        session: session_id.clone(),
        include,
        before_history,
        before_messages,
        page_limit,
        typed_media: features.contains(TYPED_MEDIA_FEATURE),
        negotiate_acp: features.contains(RAW_PAYLOAD_FEATURE),
        negotiate_file_diff: features.contains(FILE_DIFF_FEATURE),
        catalog: features.contains(LOCAL_CATALOG_FEATURE),
        // 远程来源解析与读取覆盖字段都只在协商后启用（§9.7）：未协商的连接连来源解析都不做，
        // 远程会话对它仍然不可见。
        stateless_origin: features.contains(STATELESS_ORIGIN_FEATURE),
    };
    match crate::session_read::read_session(core, actor, request.clone())
        .await
        .map_err(session_read_fault)?
    {
        crate::session_read::SessionReadOutcome::Owned(result) => {
            Ok(CommandResultPayload::SessionRead(result))
        }
        crate::session_read::SessionReadOutcome::Remote(page) => {
            crate::session_read::project_remote_read(page, &session_id, &request)
                .map(CommandResultPayload::SessionRead)
                .map_err(session_read_fault)
        }
    }
}

/// 共享投影失败 → 本模块的拒绝（错误码与 details 的唯一声明点仍在 [`Rejection::into_parts`]）。
fn session_read_fault(fault: crate::session_read::SessionReadFault) -> Rejection {
    use crate::session_read::SessionReadFault;
    match fault {
        SessionReadFault::Schema(text) => Rejection::Schema(text),
        SessionReadFault::CursorExpired => Rejection::CursorInvalid(CursorFault::CursorExpired),
        SessionReadFault::TooLarge(text) => {
            Rejection::Code(ErrorCode::ResourceResultTooLarge, text)
        }
        SessionReadFault::Internal(text) => Rejection::Code(ErrorCode::InternalUnavailable, text),
        SessionReadFault::Unsupported(text) => {
            Rejection::Code(ErrorCode::CapabilityUnsupportedByBroker, text)
        }
        SessionReadFault::Port(error) => port_rejection(&error),
    }
}

/// `command.status`：目标请求的当前记录（§11.4）。
///
/// 目标不存在或**对本设备不可见**时一律 `command.not_found`（§11.4：避免权限侧信道）。可见性由 core 的
/// `find_request(request, actor)` 给出——它按 `(actor, requestId)` 查，永远只返回该 actor 自己的请求。
///
/// 协商 `resource.stateless-origin.v1` 后，`payload.remoteTarget` 决定这条查询去**哪个来源**：
/// - 携带 `remoteTarget` → 经 [`UseCases::remote_command_status`] 直接向原 Owner 查询，**不**读本地请求
///   表（Access 没有远程命令记录，也从不推断「本地查不到 = 没执行」）；`sessionId = null` 只用于尚未建成
///   会话的 `session.create`；
/// - 未协商该 feature 却携带 `remoteTarget` → `protocol.schema_invalid`（封闭字段门控，不静默忽略）；
/// - 缺席 → 保留 owned 行为（本地请求表）。
pub async fn command_status(
    core: &Arc<UseCases>,
    actor: &Actor,
    payload: WireCommandStatus,
    features: &NegotiatedFeatures,
) -> Result<CommandResultPayload, Rejection> {
    let Some(target) = core_request_id(&payload.target_request_id) else {
        return Err(Rejection::Schema("targetRequestId is not a request id"));
    };
    let Some(remote_target) = payload.remote_target.as_ref() else {
        let record = core
            .command_status(actor, target)
            .await
            .map_err(|error| port_rejection(&error))?;
        let Some(record) = record else {
            return Err(Rejection::Code(
                ErrorCode::CommandNotFound,
                "the request is not known to this device",
            ));
        };
        let wire = wire_status_record(&record).ok_or(Rejection::Code(
            ErrorCode::InternalUnavailable,
            "the command record cannot be projected",
        ))?;
        return Ok(CommandResultPayload::CommandStatus(wire));
    };
    if !features.contains(STATELESS_ORIGIN_FEATURE) {
        return Err(Rejection::Schema(
            "remoteTarget requires the stateless-origin feature",
        ));
    }
    let owner = acp_core::model::NodeId::new(remote_target.owner_node_id.as_str())
        .map_err(|_| Rejection::Schema("remoteTarget.ownerNodeId is not a node id"))?;
    let session = match remote_target.session_id.as_ref() {
        Some(session) => Some(
            SessionId::new(session.as_str())
                .map_err(|_| Rejection::Schema("remoteTarget.sessionId is not a session id"))?,
        ),
        None => None,
    };
    let lookup = core
        .remote_command_status(
            actor,
            acp_core::ports::RemoteCommandTarget {
                owner_node_id: owner,
                session_id: session,
            },
            target,
        )
        .await
        .map_err(|error| port_rejection(&error))?;
    match lookup {
        acp_core::ports::RemoteCommandLookup::NotFound => Err(Rejection::Code(
            ErrorCode::CommandNotFound,
            "the request is not known to the owner node",
        )),
        acp_core::ports::RemoteCommandLookup::Accepted(record)
        | acp_core::ports::RemoteCommandLookup::Terminal(record)
        | acp_core::ports::RemoteCommandLookup::Uncertain(record) => {
            let wire = wire_remote_status_record(&record).ok_or(Rejection::Code(
                ErrorCode::InternalUnavailable,
                "the command record cannot be projected",
            ))?;
            Ok(CommandResultPayload::CommandStatus(wire))
        }
    }
}

/// 远程命令记录 → wire `commandStatusRecord`（与 owned 记录同形；状态词表由 core 枚举决定）。
fn wire_remote_status_record(
    record: &acp_core::ports::RemoteCommandRecord,
) -> Option<CommandStatusRecord> {
    Some(CommandStatusRecord {
        target_request_id: Uuid::parse(record.request.as_str()).ok()?,
        state: wire_command_state(record.status),
        accepted_at: match record.accepted_at.as_ref() {
            Some(at) => Nullable::value(Timestamp::parse(at.as_str()).ok()?),
            None => Nullable::null(),
        },
        terminal_at: match record.terminal_at.as_ref() {
            Some(at) => Nullable::value(Timestamp::parse(at.as_str()).ok()?),
            None => Nullable::null(),
        },
        // Owner 的记录只有终态结果，没有「终态事件标识」；wire 上该键 required 且可 `null`，
        // 这里如实给 `null`（不伪造一个事件 id）。
        terminal_event_id: Nullable::null(),
        result: match record.result.as_ref() {
            Some(result) => Nullable::value(RawObject::parse(result.as_str()).ok()?),
            None => Nullable::null(),
        },
        error: match record.error.as_ref() {
            Some(error) => Nullable::value(wire_public_error(error)?),
            None => Nullable::null(),
        },
    })
}

/// core 的命令状态 → wire `commandStatusRecord.state` 词表。
fn wire_command_state(status: CommandStatus) -> CommandState {
    match status {
        CommandStatus::Accepted => CommandState::Accepted,
        CommandStatus::Completed => CommandState::Completed,
        CommandStatus::Failed => CommandState::Failed,
        CommandStatus::Rejected => CommandState::Rejected,
        CommandStatus::Uncertain => CommandState::Uncertain,
    }
}

/// `session.mode.list`：`ModeState`（`currentModeId` + `availableModes` + `version`）。
pub async fn mode_list(
    core: &Arc<UseCases>,
    actor: &Actor,
    command: &Command,
) -> Result<CommandResultPayload, Rejection> {
    let reference = session_reference(command)?;
    let listing = core
        .mode_list(actor, &reference)
        .await
        .map_err(|error| port_rejection(&error))?;
    let state = wire_mode_state(&listing).ok_or(Rejection::Code(
        ErrorCode::InternalUnavailable,
        "the mode listing cannot be projected",
    ))?;
    Ok(CommandResultPayload::ModeList(state))
}

/// `session.config.list`：`{ configOptions, version }`。
///
/// `configOptions` 来自 `UseCases::config_options`（活体数据由用例层合并），`version` 来自
/// `Broker::session_version`——与 `session.mode.list` 结果里的版本同源（同一个会话版本读）。两笔读都发生在
/// 授权之后（见模块文档的依赖纪律）。
pub async fn config_list(
    core: &Arc<UseCases>,
    broker: &Arc<Broker>,
    actor: &Actor,
    command: &Command,
) -> Result<CommandResultPayload, Rejection> {
    let reference = session_reference(command)?;
    let options = core
        .config_options(actor, &reference)
        .await
        .map_err(|error| port_rejection(&error))?;
    let SessionReference::Owned(OwnedSessionRef { session_id }) = reference else {
        return Err(Rejection::UnsupportedResource("config_options"));
    };
    let version = broker
        .session_version(&session_id)
        .await
        .map_err(|error| port_rejection(&error))?;
    let config_options = options
        .iter()
        .map(wire_config_option)
        .collect::<Option<Vec<_>>>()
        .ok_or(Rejection::Code(
            ErrorCode::InternalUnavailable,
            "a config option cannot be projected",
        ))?;
    let version = decimal(&version.get().to_string()).ok_or(Rejection::Code(
        ErrorCode::InternalUnavailable,
        "the session version cannot be projected",
    ))?;
    Ok(CommandResultPayload::ConfigList(ConfigListResult {
        config_options,
        version,
    }))
}

/// `core.workspace-browse.v1`：`workspace.browse` 需要协商该 feature（§11.5；spec
/// `sync-workspace-browse`）。
///
/// 门控判定是 [`workspace_browse`] 的第一件事，因此未协商的连接既拿不到任何条目，也不会让
/// `UseCases::workspace_browse`（形状校验与目录读取都在它里面）被调用一次——「未协商时不触碰文件系统」
/// 这条因此是**可观察**的（端口一次都没被调用），而不只是「错误码看着对」。
pub const WORKSPACE_BROWSE_FEATURE: &str = "core.workspace-browse.v1";

/// `workspace.browse`：逐层浏览本机目录（§11.5；spec `sync-workspace-browse`）。
///
/// 四条判定的顺序与归属：
///
/// 1. **未协商 `core.workspace-browse.v1`** → `protocol.feature_required`
///    （`details.features = [WORKSPACE_BROWSE_FEATURE]`）。判定在**本函数第一行**，先于一切文件系统
///    访问——core 的用例根本不会被调用；
/// 2. **形状非法**（相对路径、含 `..`、空串）→ `protocol.schema_invalid`，由 core 的
///    [`acp_core::use_cases::UseCases::workspace_browse`] 在文件系统访问**之前**判定
///    （[`port_rejection`] 的 `schema_hint` 只回字段名与规则，不回显路径原文）；
/// 3. **不可达**（该层不存在/不可读/不是目录）→ 服务端不可用类 `internal.unavailable`；core 不返回
///    任何条目，也**不回退**到父层或根列表，本层因此没有可回退的取值可用；
/// 4. **整条结果超限** → 由调用方（[`super::CommandRoute::on_query`]）在入队前按编码后字节数换成
///    `resource.result_too_large`；单层条目上限（128）与截断标记是 core 的事，**不是**本层的截断
///    （本层绝不截断后当成功）。
///
/// 结果的 `path` 是**请求父路径的逐字回显**（`null` = 根列表）：`command.schema.json#/$defs/
/// workspaceBrowsePath` 明确要求「请求字段与结果回显字段必须逐字相同」，因此本层直接回显解码后的
/// `payload.path`。core 的 [`UseCases::workspace_browse`] 口径同源：它把请求原文留在 `echo` 里原样写进
/// 结果的 `path`，规范化结果只用来调端口与算 `registered`，所以本层没有「换成 core 那个值」的理由。
/// **条目**的 `path` 才是规范化后剥掉平台 verbatim 前缀的**下发形式**（`design.md` D2）。条目的名称/
/// 绝对路径/是否已登记与截断标记都来自 core 的投影。
pub async fn workspace_browse(
    core: &Arc<UseCases>,
    actor: &Actor,
    payload: WorkspaceBrowse,
    features: &NegotiatedFeatures,
) -> Result<CommandResultPayload, Rejection> {
    if !features.contains(WORKSPACE_BROWSE_FEATURE) {
        return Err(Rejection::FeatureRequired(WORKSPACE_BROWSE_FEATURE));
    }
    // `null` = 请求本机根列表；`Some` = 该层的父路径原文（形状与可达性由 core 判定）。
    let parent = payload.path.as_ref().map(|path| path.as_str().to_owned());
    let browse = core
        .workspace_browse(actor, parent.as_deref())
        .await
        .map_err(|error| port_rejection(&error))?;
    let entries = browse
        .entries
        .iter()
        .map(|entry| {
            Some(WorkspaceBrowseEntry {
                name: NonEmptyText::<256>::parse(&entry.name).ok()?,
                path: NonEmptyText::<4096>::parse(&entry.path).ok()?,
                registered: entry.registered,
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(Rejection::Code(
            ErrorCode::InternalUnavailable,
            "a directory entry cannot be projected",
        ))?;
    Ok(CommandResultPayload::WorkspaceBrowse(
        WorkspaceBrowseResult {
            path: payload.path,
            entries,
            truncated: browse.truncated,
        },
    ))
}

// ---------------------------------------------------------------------------------------------
// 会话文件与 Git 只读查询（`core.session-files.v1`；§11.5、design D1/D2/D3）
// ---------------------------------------------------------------------------------------------

/// 四条会话文件查询的 feature 门控（`core.session-files.v1`，§11.5）。
///
/// 判定发生在**任何**用例调用之前：未协商时 `UseCases` 的授权、会话读取、文件端口与 Git 进程都不会
/// 被调用一次，因此「未协商零 I/O」是可观察的（端口调用计数为零），而不只是错误码看着对。
pub(super) fn require_session_files(features: &NegotiatedFeatures) -> Result<(), Rejection> {
    if features.contains(super::SESSION_FILES_FEATURE) {
        Ok(())
    } else {
        Err(Rejection::FeatureRequired(super::SESSION_FILES_FEATURE))
    }
}

/// 命令信封上的 `sessionId`（四条文件/Git 查询都以它定位会话）。
pub(super) fn command_session(command: &Command) -> Result<SessionId, Rejection> {
    let Some(session) = command.session_id.as_ref() else {
        return Err(Rejection::Schema("this command requires sessionId"));
    };
    SessionId::new(session.as_str()).map_err(|_| Rejection::Schema("sessionId is not a session id"))
}

/// `session.files.list`：当前会话工作目录内一层的只读枚举（§11.5）。
pub async fn session_files_list(
    core: &Arc<UseCases>,
    actor: &Actor,
    command: &Command,
    payload: SessionFilesList,
    features: &NegotiatedFeatures,
) -> Result<CommandResultPayload, Rejection> {
    require_session_files(features)?;
    let session = command_session(command)?;
    let page = core
        .list_session_files(
            actor,
            &session,
            payload.path.as_str(),
            payload.hide_ignored,
            payload.after.as_ref().map(|after| after.as_str()),
        )
        .await
        .map_err(|error| port_rejection(&error))?;
    let result = wire_session_file_page(&page, payload.path).ok_or(Rejection::Code(
        ErrorCode::InternalUnavailable,
        "a session file entry cannot be projected",
    ))?;
    Ok(CommandResultPayload::SessionFilesList(result))
}

/// `session.files.read`：读取连接绑定内容快照的一个字节块（§11.5）。
///
/// 返回 core 的字节块而不是 wire 结果：连接期内容版本的登记/校验必须与连接状态在同一处完成（见
/// [`super::CommandRoute::session_files_read`]），本函数只负责门控、入参形状与用例调用。
pub async fn session_files_read(
    core: &Arc<UseCases>,
    actor: &Actor,
    command: &Command,
    payload: &SessionFilesRead,
    features: &NegotiatedFeatures,
) -> Result<SessionFileChunk, Rejection> {
    require_session_files(features)?;
    let session = command_session(command)?;
    let purpose = match payload.purpose {
        WireFileReadPurpose::Preview => CoreFileReadPurpose::Preview,
        WireFileReadPurpose::Download => CoreFileReadPurpose::Download,
    };
    // `offset` 是规范十进制字符串；超出 `u64` 的取值是形状错误，不得回绕成一个小偏移。
    let offset = payload
        .offset
        .as_str()
        .parse::<u64>()
        .map_err(|_| Rejection::Schema("offset is not a decimal byte offset"))?;
    let version = payload.version.as_ref().map(|version| version.as_str());
    core.read_session_file(
        actor,
        &session,
        payload.path.as_str(),
        purpose,
        offset,
        version,
    )
    .await
    .map_err(|error| port_rejection(&error))
}

/// `session.git.status`：当前会话工作目录内的 Git 状态页（§11.5）。
pub async fn session_git_status(
    core: &Arc<UseCases>,
    actor: &Actor,
    command: &Command,
    payload: SessionGitStatus,
    features: &NegotiatedFeatures,
) -> Result<CommandResultPayload, Rejection> {
    require_session_files(features)?;
    let session = command_session(command)?;
    let page = core
        .session_git_status(
            actor,
            &session,
            payload.after.as_ref().map(|after| after.as_str()),
        )
        .await
        .map_err(|error| port_rejection(&error))?;
    let result = wire_git_status_page(&page, features.contains("core.session-git-stats.v1"))
        .ok_or(Rejection::Code(
            ErrorCode::InternalUnavailable,
            "a git status entry cannot be projected",
        ))?;
    Ok(CommandResultPayload::SessionGitStatus(result))
}

/// `session.git.diff`：当前会话工作目录内一个文件的 HEAD/工作树差异（§11.5）。
pub async fn session_git_diff(
    core: &Arc<UseCases>,
    actor: &Actor,
    command: &Command,
    payload: SessionGitDiff,
    features: &NegotiatedFeatures,
) -> Result<CommandResultPayload, Rejection> {
    require_session_files(features)?;
    let session = command_session(command)?;
    let diff = core
        .session_git_diff(actor, &session, payload.path.as_str())
        .await
        .map_err(|error| port_rejection(&error))?;
    let mut result = wire_git_diff(&diff).ok_or(Rejection::Code(
        ErrorCode::InternalUnavailable,
        "a git diff cannot be projected",
    ))?;
    if !features.contains("core.session-git-compare.v1") {
        result.comparison = None;
    }
    Ok(CommandResultPayload::SessionGitDiff(result))
}

/// core 的枚举页 → wire `sessionFilesListResult`（`path` 逐字回显请求）。
fn wire_session_file_page(
    page: &SessionFilePage,
    path: SessionFilesPath,
) -> Option<SessionFilesListResult> {
    let entries = page
        .entries()
        .iter()
        .map(|entry| {
            Some(WireSessionFileEntry {
                path: SessionRelativePath::parse(entry.path().as_str()).ok()?,
                name: NonEmptyText::<256>::parse(entry.name()).ok()?,
                kind: match entry.kind() {
                    CoreEntryKind::File => sync_protocol::command::SessionFileEntryKind::File,
                    CoreEntryKind::Directory => {
                        sync_protocol::command::SessionFileEntryKind::Directory
                    }
                    CoreEntryKind::Link => sync_protocol::command::SessionFileEntryKind::Link,
                    CoreEntryKind::Other => sync_protocol::command::SessionFileEntryKind::Other,
                },
                openable: entry.openable(),
                restricted: entry.restricted().map(|reason| match reason {
                    CoreRestrictedReason::OutsideRoot => {
                        sync_protocol::command::SessionFileRestrictedReason::OutsideRoot
                    }
                    CoreRestrictedReason::Unreadable => {
                        sync_protocol::command::SessionFileRestrictedReason::Unreadable
                    }
                    CoreRestrictedReason::Unsupported => {
                        sync_protocol::command::SessionFileRestrictedReason::Unsupported
                    }
                }),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let next = match page.next() {
        Some(token) => Some(SessionPageToken::parse(token.as_str()).ok()?),
        None => None,
    };
    Some(SessionFilesListResult {
        path,
        git_state: wire_git_state(page.git_state()),
        entries,
        next,
    })
}

/// core 的字节块 → wire `sessionFilesReadResult`（`dataBase64` 为无填充 base64url）。
pub(super) fn wire_session_file_chunk(chunk: &SessionFileChunk) -> Option<SessionFilesReadResult> {
    let data = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(chunk.data());
    Some(SessionFilesReadResult {
        path: SessionRelativePath::parse(chunk.path().as_str()).ok()?,
        kind: match chunk.kind() {
            SessionFileContentKind::Text => sync_protocol::command::SessionFileContentKind::Text,
            SessionFileContentKind::Image => sync_protocol::command::SessionFileContentKind::Image,
            SessionFileContentKind::Binary => {
                sync_protocol::command::SessionFileContentKind::Binary
            }
        },
        size_bytes: decimal(&chunk.size_bytes().to_string())?,
        version: WireContentVersion::parse(chunk.version().as_str()).ok()?,
        offset: decimal(&chunk.offset().to_string())?,
        data_base64: SessionFileData::parse(&data).ok()?,
        eof: chunk.eof(),
        preview_truncated: chunk.preview_truncated(),
    })
}

/// core 的 Git 状态页 → wire `sessionGitStatusResult`。
fn wire_git_status_page(
    page: &GitStatusPage,
    include_counts: bool,
) -> Option<SessionGitStatusResult> {
    let entries = page
        .entries()
        .iter()
        .map(|entry| {
            Some(WireGitStatusEntry {
                path: SessionRelativePath::parse(entry.path().as_str()).ok()?,
                status: match entry.status() {
                    CoreGitFileStatus::Added => WireGitFileStatus::Added,
                    CoreGitFileStatus::Modified => WireGitFileStatus::Modified,
                    CoreGitFileStatus::Deleted => WireGitFileStatus::Deleted,
                    CoreGitFileStatus::Renamed => WireGitFileStatus::Renamed,
                    CoreGitFileStatus::Untracked => WireGitFileStatus::Untracked,
                },
                previous_path: match entry.previous_path() {
                    Some(path) => Some(SessionRelativePath::parse(path.as_str()).ok()?),
                    None => None,
                },
                conflicted: entry.conflicted(),
                binary: entry.binary(),
                added_lines: entry
                    .line_counts()
                    .filter(|_| include_counts)
                    .map(|counts| counts.0),
                deleted_lines: entry
                    .line_counts()
                    .filter(|_| include_counts)
                    .map(|counts| counts.1),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let next = match page.next() {
        Some(token) => Some(SessionPageToken::parse(token.as_str()).ok()?),
        None => None,
    };
    Some(SessionGitStatusResult {
        git_state: wire_git_state(page.git_state()),
        entries,
        next,
    })
}

/// core 的差异视图 → wire `sessionGitDiffResult`。
fn wire_git_diff(diff: &CoreGitDiff) -> Option<SessionGitDiffResult> {
    let diff_lines = if diff.lines().is_empty() {
        None
    } else {
        Some(
            diff.lines()
                .iter()
                .map(|line| Text::<4096>::parse(line).ok())
                .collect::<Option<Vec<_>>>()?,
        )
    };
    Some(SessionGitDiffResult {
        path: SessionRelativePath::parse(diff.path().as_str()).ok()?,
        base: match diff.base() {
            CoreGitDiffBase::Head => WireGitDiffBase::Head,
            CoreGitDiffBase::Empty => WireGitDiffBase::Empty,
        },
        kind: match diff.kind() {
            CoreGitDiffKind::Text => WireGitDiffKind::Text,
            CoreGitDiffKind::Binary => WireGitDiffKind::Binary,
            CoreGitDiffKind::Conflict => WireGitDiffKind::Conflict,
        },
        comparison: match diff.comparison() {
            Some((before, after)) => Some(sync_protocol::command::GitFileComparison {
                before: Text::<65536>::parse(before).ok()?,
                after: Text::<65536>::parse(after).ok()?,
            }),
            None => None,
        },
        diff_lines,
        unavailable: diff.unavailable().map(|reason| match reason {
            CoreGitDiffUnavailable::NotRepository => WireGitDiffUnavailable::NotRepository,
            CoreGitDiffUnavailable::GitUnavailable => WireGitDiffUnavailable::GitUnavailable,
            CoreGitDiffUnavailable::TooLarge => WireGitDiffUnavailable::TooLarge,
        }),
        truncated: diff.truncated(),
    })
}

/// core 的三种 Git 可查询状态 → wire 封闭词表。
fn wire_git_state(state: CoreGitState) -> WireGitState {
    match state {
        CoreGitState::Available => WireGitState::Available,
        CoreGitState::NotRepository => WireGitState::NotRepository,
        CoreGitState::Unavailable => WireGitState::Unavailable,
    }
}

/// `sessionId` → owned 会话引用（`session.mode.list`/`session.config.list` 的入参形状）。
fn session_reference(command: &Command) -> Result<SessionReference, Rejection> {
    let Some(session) = command.session_id.as_ref() else {
        return Err(Rejection::Schema("this command requires sessionId"));
    };
    let session_id = SessionId::new(session.as_str())
        .map_err(|_| Rejection::Schema("sessionId is not a session id"))?;
    Ok(SessionReference::Owned(OwnedSessionRef { session_id }))
}

/// wire 的 `include` 列表 → core 的 `HistoryInclude`（未知取值由 schema 的封闭枚举挡住）。
fn history_include(list: &IncludeList) -> Result<HistoryInclude, Rejection> {
    let mut include = HistoryInclude::default();
    for resource in list.as_slice() {
        match resource.as_str() {
            "messages" => include.messages = true,
            "turns" => include.turns = true,
            "pending_interactions" => include.pending_interactions = true,
            "config_options" => include.config_options = true,
            "capabilities" => include.capabilities = true,
            "history" => include.history = true,
            other => {
                debug!(
                    event = "sync.session_read_unknown_include",
                    include = other,
                    "the include value is not part of the v1 set"
                );
                return Err(Rejection::Schema("include contains an unknown resource"));
            }
        }
    }
    Ok(include)
}

/// `beforeHistory` 的十进制序号。
fn parse_sequence(text: &str) -> Result<Sequence, Rejection> {
    text.parse::<u64>()
        .ok()
        .and_then(|value| Sequence::new(value).ok())
        .ok_or(Rejection::Schema("beforeHistory is not a decimal sequence"))
}

/// 十进制串（序号/版本/长度在 wire 上的形状）。
fn decimal(text: &str) -> Option<DecimalString> {
    DecimalString::parse(text).ok()
}

// ---------------------------------------------------------------------------------------------
// core → wire 投影
// ---------------------------------------------------------------------------------------------

/// core 的会话摘要 → wire `sessionSummary`。
///
/// 与 [`crate::sync::snapshot`] 的同名投影**逐字段同口径**（那是快照 `sessions` 资源的唯一投影；本模块
/// 拿不到它的私有函数，因为它只在快照模块内可见）。`catalog`（`core.local-catalog.v1` 的协商结果）决定
/// `workspace` 键的三态：未协商时**缺席**（schema 是 `additionalProperties: false`，发 `null` 会让未协商
/// 的客户端拒绝整条摘要）；协商时键在场，`null` = 未分组。
pub(crate) fn wire_session(
    summary: &acp_core::model::SessionSummary,
    catalog: bool,
) -> Option<WireSessionSummary> {
    let agent = summary.agent();
    let workspace = if catalog {
        Some(match summary.workspace() {
            Some(reference) => Nullable::value(WireWorkspaceRef {
                directory_id: NonEmptyText::<32>::parse(reference.directory_id().as_str()).ok()?,
            }),
            None => Nullable::null(),
        })
    } else {
        None
    };
    Some(WireSessionSummary {
        session_id: Uuid::parse(summary.session_id().as_str()).ok()?,
        title: match summary.title() {
            Some(title) => Nullable::value(Text::<512>::parse(title).ok()?),
            None => Nullable::null(),
        },
        agent: SessionAgentRef {
            agent_id: NonEmptyText::<128>::parse(agent.agent_id().as_str()).ok()?,
            name: NonEmptyText::<128>::parse(agent.name()).ok()?,
        },
        state: wire_session_state(summary.state()),
        origin: match summary.origin() {
            ResourceOrigin::Local => OriginBlock::Local,
            ResourceOrigin::Remote {
                owner_node_id,
                origin_epoch,
                online,
            } => OriginBlock::Remote {
                owner_node_id: Uuid::parse(owner_node_id.as_str()).ok()?,
                origin_epoch: Uuid::parse(origin_epoch.as_str()).ok()?,
                online: *online,
            },
        },
        current_mode: match summary.current_mode() {
            Some(mode) => Nullable::value(wire_mode_ref(mode)?),
            None => Nullable::null(),
        },
        version: decimal(&summary.version().get().to_string())?,
        created_at: Timestamp::parse(summary.created_at().as_str()).ok()?,
        updated_at: Timestamp::parse(summary.updated_at().as_str()).ok()?,
        workspace,
    })
}

/// core 的会话状态 → wire 词表（两侧取值一一对应，逐条显式映射）。
fn wire_session_state(state: CoreSessionState) -> WireSessionState {
    match state {
        CoreSessionState::Idle => WireSessionState::Idle,
        CoreSessionState::Running => WireSessionState::Running,
        CoreSessionState::Pending => WireSessionState::Pending,
    }
}

/// core 的 `ModeRef` → wire `modeRef`。
fn wire_mode_ref(mode: &CoreModeRef) -> Option<WireModeRef> {
    Some(WireModeRef {
        mode_id: NonEmptyText::<256>::parse(mode.mode_id().as_str()).ok()?,
        display_name: NonEmptyText::<256>::parse(mode.display_name()).ok()?,
    })
}

/// `session.mode.list` 的结果：core 把 `version` 与会话状态分开给出，wire 要三键同对象。
fn wire_mode_state(listing: &ModeListing) -> Option<WireModeState> {
    Some(WireModeState {
        current_mode_id: match listing.state.current_mode.as_ref() {
            Some(mode) => {
                Nullable::value(NonEmptyText::<256>::parse(mode.mode_id().as_str()).ok()?)
            }
            None => Nullable::null(),
        },
        available_modes: listing
            .state
            .available
            .iter()
            .map(wire_mode_ref)
            .collect::<Option<Vec<_>>>()?,
        version: decimal(&listing.version.get().to_string())?,
    })
}

/// core 的 `CommandRecord` → wire `commandStatusRecord`（`command.status` 的完成结果）。
fn wire_status_record(record: &CommandRecord) -> Option<CommandStatusRecord> {
    Some(CommandStatusRecord {
        target_request_id: Uuid::parse(record.request().as_str()).ok()?,
        state: sync_protocol::command::CommandState::from_str(record.status().as_str()).ok()?,
        accepted_at: match record.accepted_at() {
            Some(at) => Nullable::value(wire_timestamp(at.as_str())?),
            None => Nullable::null(),
        },
        terminal_at: match record.terminal_at() {
            Some(at) => Nullable::value(wire_timestamp(at.as_str())?),
            None => Nullable::null(),
        },
        terminal_event_id: match record.terminal_event() {
            Some(event) => Nullable::value(Uuid::parse(event.as_str()).ok()?),
            None => Nullable::null(),
        },
        result: match record.result() {
            Some(result) => Nullable::value(RawObject::parse(result.as_str()).ok()?),
            None => Nullable::null(),
        },
        error: match record.error() {
            Some(error) => Nullable::value(wire_public_error(error)?),
            None => Nullable::null(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_read::message_key;
    use acp_core::model::{
        AgentId, AgentRef, EffectiveInteraction as CoreEffectiveInteraction,
        FileReadPurpose as CorePurpose, GitDiffBase as CoreBase, GitDiffKind as CoreDiffKind,
        GitFileStatus as CoreStatus, GitState as CoreState, GitStatusEntry as CoreStatusEntry,
        GitStatusPage as CoreStatusPage, GlobalCursor, JsonValueText,
        MessageChannel as CoreMessageChannel, MessageRecord, PartialResult as CorePartialResult,
        PortError, ServerEpoch, SessionFileEntry as CoreEntry, SessionFilePage as CorePage,
        SessionFilePath, SessionFileRestrictedReason as CoreRestricted,
        SessionListPath as CoreListPath, SessionPageToken as CoreToken, SessionSummary, Version,
    };
    use acp_core::ports::{HistoryPage, HistoryQuery, OwnedEventRecord};
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    use sync_protocol::common::AgentContentBlock;

    use async_trait::async_trait;

    fn path(text: &str) -> SessionFilePath {
        SessionFilePath::parse(text).expect("session path")
    }

    /// 四条命令的 feature 门控在用例之前：未协商就是 `protocol.feature_required`，且 `details.features`
    /// 只给出缺的那一条。
    #[test]
    fn session_file_queries_require_their_feature() {
        let empty = NegotiatedFeatures::new(Vec::new());
        let rejection = require_session_files(&empty).expect_err("未协商必须拒绝");
        let (code, _, details) = rejection.into_parts();
        assert_eq!(code, ErrorCode::ProtocolFeatureRequired);
        assert_eq!(
            details.get(),
            serde_json::json!({ "features": ["core.session-files.v1"] }).to_string()
        );

        let negotiated =
            NegotiatedFeatures::new(vec![super::super::SESSION_FILES_FEATURE.to_owned()]);
        assert!(require_session_files(&negotiated).is_ok());
    }

    /// `session.files.list` 的成功投影：请求路径逐字回显、枚举类型/可打开性/受限原因与 `next` 同形。
    #[test]
    fn session_file_page_projects_the_wire_shape() {
        let entries = vec![
            CoreEntry::try_new(
                path("src/main.rs"),
                "main.rs",
                CoreEntryKind::File,
                true,
                None,
            )
            .expect("entry"),
            CoreEntry::try_new(
                path("linked"),
                "linked",
                CoreEntryKind::Link,
                false,
                Some(CoreRestricted::OutsideRoot),
            )
            .expect("entry"),
        ];
        let page = CorePage::try_new(
            CoreListPath::root(),
            CoreState::Available,
            entries,
            Some(CoreToken::new("token_1").expect("token")),
        )
        .expect("page");
        let requested = SessionFilesPath::parse("").expect("root path");
        let wire = wire_session_file_page(&page, requested).expect("projection");
        assert_eq!(
            serde_json::to_value(&wire).expect("json"),
            serde_json::json!({
                "path": "",
                "gitState": "available",
                "entries": [
                    {
                        "path": "src/main.rs",
                        "name": "main.rs",
                        "kind": "file",
                        "openable": true
                    },
                    {
                        "path": "linked",
                        "name": "linked",
                        "kind": "link",
                        "openable": false,
                        "restricted": "outside_root"
                    }
                ],
                "next": "token_1"
            })
        );
    }

    /// `session.files.read` 的 `dataBase64` 是无填充 base64url；块元数据逐项来自 core。
    #[test]
    fn session_file_chunk_uses_unpadded_base64url() {
        let chunk = SessionFileChunk::try_new(
            path("assets/logo.png"),
            SessionFileContentKind::Image,
            4,
            acp_core::model::ContentVersion::parse("version_1").expect("version"),
            0,
            vec![0xfb, 0xff, 0x00, 0x01],
            true,
            false,
        )
        .expect("chunk");
        let wire = wire_session_file_chunk(&chunk).expect("projection");
        assert_eq!(wire.data_base64.as_str(), "-_8AAQ");
        assert_eq!(wire.size_bytes.as_str(), "4");
        assert_eq!(wire.offset.as_str(), "0");
        assert_eq!(wire.version.as_str(), "version_1");
        assert!(wire.eof);
        assert!(!wire.preview_truncated);
    }

    #[test]
    fn git_line_counts_require_negotiated_feature() {
        let entry =
            CoreStatusEntry::try_new(path("a.rs"), CoreStatus::Modified, None, false, false)
                .expect("entry")
                .with_line_counts(Some((12, 3)));
        let page = CoreStatusPage::try_new(CoreState::Available, vec![entry], None).expect("page");
        let old = wire_git_status_page(&page, false).expect("old client");
        assert_eq!(old.entries[0].added_lines, None);
        assert_eq!(old.entries[0].deleted_lines, None);
        let new = wire_git_status_page(&page, true).expect("new client");
        assert_eq!(new.entries[0].added_lines, Some(12));
        assert_eq!(new.entries[0].deleted_lines, Some(3));
    }

    /// `session.git.status`：改名带旧路径，冲突/二进制是显式标记；非仓库是成功结果里的状态。
    #[test]
    fn git_status_page_projects_explicit_states() {
        let entries = vec![
            CoreStatusEntry::try_new(
                path("src/new.rs"),
                CoreStatus::Renamed,
                Some(path("src/old.rs")),
                true,
                false,
            )
            .expect("entry"),
        ];
        let page = CoreStatusPage::try_new(CoreState::NotRepository, entries, None).expect("page");
        let wire = wire_git_status_page(&page, false).expect("projection");
        assert_eq!(
            serde_json::to_value(&wire).expect("json"),
            serde_json::json!({
                "gitState": "not_repository",
                "entries": [{
                    "path": "src/new.rs",
                    "status": "renamed",
                    "previousPath": "src/old.rs",
                    "conflicted": true,
                    "binary": false
                }]
            })
        );
    }

    /// `session.git.diff`：文本行映射到 `diffLines`，不可用原因单独出现，二者不同时存在。
    #[test]
    fn git_diff_projects_lines_and_unavailability() {
        let text = CoreGitDiff::try_new(
            path("src/main.rs"),
            CoreBase::Empty,
            CoreDiffKind::Text,
            vec!["+hello".to_owned(), "-world".to_owned()],
            None,
            true,
        )
        .expect("diff");
        let wire = wire_git_diff(&text).expect("projection");
        assert_eq!(
            serde_json::to_value(&wire).expect("json"),
            serde_json::json!({
                "path": "src/main.rs",
                "base": "empty",
                "kind": "text",
                "diffLines": ["+hello", "-world"],
                "truncated": true
            })
        );

        let unavailable = CoreGitDiff::try_new(
            path("src/main.rs"),
            CoreBase::Head,
            CoreDiffKind::Binary,
            Vec::new(),
            Some(CoreGitDiffUnavailable::TooLarge),
            false,
        )
        .expect("diff");
        let wire = wire_git_diff(&unavailable).expect("projection");
        let value = serde_json::to_value(&wire).expect("json");
        assert_eq!(value["unavailable"], "too_large");
        assert!(value.get("diffLines").is_none());
        assert_eq!(value["kind"], "binary");
    }

    /// 读用途两个取值都必须显式映射，未知分支不存在（封闭枚举由编译器保证）。
    #[test]
    fn read_purpose_is_mapped_exactly() {
        let chunk = SessionFileChunk::try_new(
            path("a.txt"),
            SessionFileContentKind::Text,
            0,
            acp_core::model::ContentVersion::parse("v").expect("version"),
            0,
            Vec::new(),
            true,
            false,
        )
        .expect("chunk");
        assert_eq!(chunk.data().len(), 0);
        // 用途映射本身由下方 `match` 的穷尽性保证；这里固定两个 wire 取值的字符串形态。
        assert_eq!(WireFileReadPurpose::Preview.as_str(), "preview");
        assert_eq!(WireFileReadPurpose::Download.as_str(), "download");
        let _ = CorePurpose::Preview;
    }

    fn uid(number: u32) -> String {
        format!("00000000-0000-4000-8000-{number:012}")
    }

    /// 一条持久化的 `user.message.delta`（正文页的输入那一半）。
    fn user_delta(
        event: u32,
        message: u32,
        index: u64,
        text: &str,
        created_at: &str,
    ) -> OwnedEventRecord {
        let session = acp_core::model::SessionId::new(&uid(1)).expect("session id");
        let view = serde_json::json!({
            "messageId": uid(message),
            "turnId": uid(90),
            "deltaIndex": index.to_string(),
            "text": text,
        })
        .to_string();
        OwnedEventRecord {
            event: acp_core::model::CommittedEvent {
                id: acp_core::model::EventId::new(&uid(event)).expect("event id"),
                event_type: acp_core::model::EventType::new("user.message.delta")
                    .expect("event type"),
                session: Some(session.clone()),
                session_sequence: Some(
                    acp_core::model::Sequence::new(index + 1).expect("sequence"),
                ),
                global_sequence: acp_core::model::Sequence::new(event as u64 + 1)
                    .expect("sequence"),
                origin_epoch: Some(
                    acp_core::model::OriginEpoch::new(&uid(2)).expect("origin epoch"),
                ),
                origin_sequence: Some(acp_core::model::Sequence::new(index + 1).expect("sequence")),
                created_at: acp_core::model::Timestamp::new(created_at).expect("timestamp"),
            },
            payload: acp_core::model::EventPayload::new(
                acp_core::model::ViewJson::new(&view).expect("view"),
                None,
            ),
        }
    }

    fn text_of(message: &Message) -> String {
        message
            .content
            .iter()
            .filter_map(|block| match block {
                AgentContentBlock::Text { text } => Some(text.as_str().to_owned()),
                _ => None,
            })
            .collect()
    }

    /// 已提交增量在重连后重复到达：整体只应用一次（不重复追加）。
    #[test]
    fn duplicate_user_delta_events_are_applied_once() {
        let record = user_delta(10, 2, 0, "hello ", "2026-01-01T00:00:00.000Z");
        let mut merger = MessageMerger::new(None);
        merger.absorb(&record);
        merger.absorb(&record);
        let page = merger.finish(20, Vec::new(), false);
        assert_eq!(page.messages.len(), 1);
        assert_eq!(text_of(&page.messages[0]), "hello ");
        assert_eq!(
            page.messages[0].message_id,
            Uuid::parse(&uid(2)).expect("message id")
        );
    }

    /// 同一 `(messageId, deltaIndex)` 换了 eventId 重投：也只应用一次。
    #[test]
    fn duplicate_delta_position_with_a_new_event_id_is_applied_once() {
        let first = user_delta(11, 3, 0, "same", "2026-01-01T00:00:00.000Z");
        let retried = user_delta(12, 3, 0, "same", "2026-01-01T00:00:00.000Z");
        let mut merger = MessageMerger::new(None);
        merger.absorb(&first);
        merger.absorb(&retried);
        let page = merger.finish(20, Vec::new(), false);
        assert_eq!(page.messages.len(), 1);
        assert_eq!(text_of(&page.messages[0]), "same");
    }

    /// 读取屏障：每次读只覆盖读口内的增量；旧页不因新增量而改变，新页按游标只取游标之前的消息。
    #[test]
    fn read_barrier_keeps_the_snapshot_and_later_reads_see_new_increments() {
        let first = user_delta(20, 4, 0, "one", "2026-01-01T00:00:00.000Z");
        let second = user_delta(21, 5, 0, "two", "2026-01-01T00:00:01.000Z");
        let mut initial = MessageMerger::new(None);
        initial.absorb(&first);
        initial.absorb(&second);
        let snapshot = initial.finish(20, Vec::new(), false);
        assert_eq!(
            snapshot.messages.iter().map(text_of).collect::<Vec<_>>(),
            vec!["one".to_owned(), "two".to_owned()]
        );

        // 新增量到达后：不带游标的读看到全部三条；带游标（`before` = 第三条）的读只看到它之前的两条。
        // 旧的一次读是已经被取走的页，不会被后来的增量“覆盖”成三条。
        let third = user_delta(22, 6, 0, "three", "2026-01-01T00:00:02.000Z");
        let mut latest = MessageMerger::new(None);
        for record in [&first, &second, &third] {
            latest.absorb(record);
        }
        let head = latest.finish(20, Vec::new(), false);
        assert_eq!(
            head.messages.iter().map(text_of).collect::<Vec<_>>(),
            vec!["one".to_owned(), "two".to_owned(), "three".to_owned()]
        );

        let cursor = (
            "2026-01-01T00:00:02.000Z".to_owned(),
            Uuid::parse(&uid(6)).expect("message id"),
        );
        let mut older = MessageMerger::new(Some(cursor));
        for record in [&first, &second, &third] {
            older.absorb(record);
        }
        let page = older.finish(20, Vec::new(), false);
        assert_eq!(
            page.messages.iter().map(text_of).collect::<Vec<_>>(),
            vec!["one".to_owned(), "two".to_owned()]
        );
        assert_eq!(text_of(&snapshot.messages[0]), "one");
        assert_eq!(snapshot.messages.len(), 2);
    }

    /// 一条持久化的 Agent 正文（`owned_message` 的投影原料）。
    fn message_record(content: &serde_json::Value) -> MessageRecord {
        MessageRecord::try_new(
            MessageId::new(&uid(80)).expect("message id"),
            acp_core::model::SessionId::new(&uid(1)).expect("session id"),
            None,
            CoreMessageChannel::Reply,
            CorePartialResult::completed(),
            acp_core::model::JsonValueText::new(&content.to_string()).expect("content"),
            Vec::new(),
            CoreTimestamp::new("2026-01-01T00:00:00.000Z").expect("timestamp"),
            CoreTimestamp::new("2026-01-01T00:00:00.000Z").expect("timestamp"),
        )
        .expect("record")
    }

    /// 未协商 `core.typed-media.v1` 的消息页必须走与事件轨道**同一个**降级：`audio_ref`/`file_ref`
    /// 整块去掉、`image_ref` 去掉 `attachmentId`。否则旧 peer 的封闭 `agentContentBlock` 词表会让整条
    /// `session.read` 结果解码失败；协商的连接则必须逐字保留。
    #[test]
    fn unnegotiated_message_content_is_degraded_like_event_views() {
        let record = message_record(&serde_json::json!([
            {"type": "text", "text": "hi"},
            {
                "type": "image_ref",
                "mimeType": "image/png",
                "byteLength": "3",
                "displayState": "available",
                "attachmentId": uid(70)
            },
            {
                "type": "audio_ref",
                "mimeType": "audio/wav",
                "byteLength": "9",
                "displayState": "available",
                "attachmentId": uid(71)
            },
            {
                "type": "file_ref",
                "attachmentId": uid(72),
                "mimeType": "text/plain",
                "byteLength": "5",
                "displayName": null,
                "displayState": "available"
            }
        ]));
        let legacy = Message::from_record(&record, false).expect("legacy projection");
        let blocks = serde_json::to_value(&legacy.content)
            .expect("json")
            .as_array()
            .expect("blocks")
            .clone();
        assert_eq!(blocks.len(), 2, "audio_ref/file_ref 整块去掉");
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "image_ref");
        assert!(
            blocks[1].get("attachmentId").is_none(),
            "旧 peer 的 image_ref 没有 attachmentId"
        );

        // 协商的连接不做任何改写。
        let current = Message::from_record(&record, true).expect("projection");
        let blocks = serde_json::to_value(&current.content)
            .expect("json")
            .as_array()
            .expect("blocks")
            .clone();
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[1]["attachmentId"], uid(70));
    }

    /// 页边界：用户输入超过一页时，下界是从最新往前数第 `limit+1` 条用户消息；不超过一页时没有下界
    /// （页从最早一条开始）。下界只统计**严格早于**游标的用户消息。
    #[test]
    fn page_floor_is_the_user_message_before_the_page() {
        let mut merger = MessageMerger::new(None);
        for (index, second) in [0u32, 1, 2].into_iter().enumerate() {
            merger.absorb(&user_delta(
                30 + index as u32,
                10 + second,
                0,
                "x",
                &format!("2026-01-01T00:00:0{second}.000Z"),
            ));
        }
        assert_eq!(merger.floor(3), None);
        assert_eq!(
            merger.floor(2),
            Some(("2026-01-01T00:00:00.000Z".to_owned(), uid(10)))
        );

        // 游标 = 最新一条：它的下界是第二条（游标本身不计数）。
        let cursor = (
            "2026-01-01T00:00:02.000Z".to_owned(),
            Uuid::parse(&uid(12)).expect("message id"),
        );
        let mut narrowed = MessageMerger::new(Some(cursor));
        for (index, second) in [0u32, 1, 2].into_iter().enumerate() {
            narrowed.absorb(&user_delta(
                40 + index as u32,
                10 + second,
                0,
                "x",
                &format!("2026-01-01T00:00:0{second}.000Z"),
            ));
        }
        assert_eq!(
            narrowed.floor(1),
            Some(("2026-01-01T00:00:00.000Z".to_owned(), uid(10)))
        );
    }

    /// Agent 正文的补齐判定：没有下界时读到最早；有下界时最早一条严格更早才算覆盖。
    #[test]
    fn message_paging_continues_until_the_page_floor_is_covered() {
        let floor = ("2026-01-01T00:00:01.000Z".to_owned(), uid(10));
        assert!(needs_older_agents(None, None));
        assert!(needs_older_agents(Some(&floor), None));
        assert!(needs_older_agents(Some(&floor), Some(&floor)));
        assert!(needs_older_agents(
            Some(&floor),
            Some(&("2026-01-01T00:00:02.000Z".to_owned(), uid(11)))
        ));
        assert!(!needs_older_agents(
            Some(&floor),
            Some(&("2026-01-01T00:00:00.000Z".to_owned(), uid(9)))
        ));
    }

    /// `session.read` 的用例读面替身：`messages` 按真实端口的口径分页（`before` 严格更早、一页
    /// `messages_limit` 条、多取一条决定页标记），并记录每次请求的游标。
    struct PagedSource {
        all: Vec<MessageRecord>,
        events: Vec<OwnedEventRecord>,
        session: SessionSummary,
        queries: Mutex<Vec<(Option<MessageCursor>, u32)>>,
    }

    #[async_trait]
    impl SessionReadSource for PagedSource {
        async fn read_session(
            &self,
            _actor: &Actor,
            query: HistoryQuery,
        ) -> Result<HistoryPage, PortError> {
            let limit = query.messages_limit.events();
            self.queries
                .lock()
                .expect("query log")
                .push((query.before_messages.clone(), limit));
            let (messages, messages_has_earlier) = if limit == 0 {
                (Vec::new(), false)
            } else {
                let end = match &query.before_messages {
                    None => self.all.len(),
                    Some(cursor) => {
                        let key = (
                            cursor.created_at.as_str().to_owned(),
                            cursor.message_id.as_str().to_owned(),
                        );
                        self.all.partition_point(|record| message_key(record) < key)
                    }
                };
                let start = end.saturating_sub(limit as usize);
                (self.all[start..end].to_vec(), start > 0)
            };
            // 首批（无 `after`、无 `before`）带回用户输入事件；更早的正文页不重复给。
            let events = if limit > 0 && query.after.is_none() && query.before_messages.is_none() {
                self.events.clone()
            } else {
                Vec::new()
            };
            Ok(HistoryPage {
                session: self.session.clone(),
                events,
                history: Vec::new(),
                history_has_earlier: false,
                messages,
                messages_has_earlier,
                turns: Vec::new(),
                interactions: Vec::new(),
                config: Vec::new(),
                capabilities: None,
                head: GlobalCursor::new(
                    ServerEpoch::new(&uid(2)).expect("server epoch"),
                    Sequence::new(1).expect("sequence"),
                ),
                next: None,
            })
        }

        async fn effective_interactions(
            &self,
            _actor: &Actor,
            _reference: &SessionReference,
        ) -> Result<Vec<CoreEffectiveInteraction>, PortError> {
            Ok(Vec::new())
        }
    }

    fn second(index: u32) -> String {
        format!("2026-01-01T00:{:02}:{:02}.000Z", index / 60, index % 60)
    }

    /// 编号 `index` 的 Agent 正文（升序全集里的第 `index` 条）。
    fn agent_record(index: u32) -> MessageRecord {
        let created_at = second(index);
        MessageRecord::try_new(
            MessageId::new(&uid(1000 + index)).expect("message id"),
            acp_core::model::SessionId::new(&uid(1)).expect("session id"),
            None,
            CoreMessageChannel::Reply,
            CorePartialResult::completed(),
            JsonValueText::new(&format!(r#"[{{"type":"text","text":"agent {index}"}}]"#))
                .expect("content"),
            Vec::new(),
            CoreTimestamp::new(&created_at).expect("timestamp"),
            CoreTimestamp::new(&created_at).expect("timestamp"),
        )
        .expect("record")
    }

    fn page_session() -> SessionSummary {
        let created_at = CoreTimestamp::new(&second(0)).expect("timestamp");
        SessionSummary::try_new(
            acp_core::model::SessionId::new(&uid(1)).expect("session id"),
            Some("paged".to_owned()),
            AgentRef::try_new(AgentId::new("codex").expect("agent id"), "Codex CLI")
                .expect("agent ref"),
            CoreSessionState::Idle,
            ResourceOrigin::Local,
            None,
            Version::new(1),
            created_at.clone(),
            created_at,
            None,
        )
        .expect("summary")
    }

    /// 修复前：补齐循环每轮都用初始页最早一条当 `before`，重复取同一页直到 `resource.result_too_large`。
    /// 这里用 450 条 Agent 正文（200 + 200 + 50 三页）驱动真正的 `session_read`：游标必须逐页前移，
    /// 合并页完整且无重复，`hasEarlier` 与真实页边界一致。
    #[tokio::test]
    async fn session_read_completes_agent_messages_across_pages() {
        let all = (0..450).map(agent_record).collect::<Vec<_>>();
        let source = PagedSource {
            all,
            events: vec![user_delta(
                900,
                5000,
                0,
                "hello",
                "2026-01-01T00:03:45.500Z",
            )],
            session: page_session(),
            queries: Mutex::new(Vec::new()),
        };
        let command: Command = serde_json::from_str(&format!(
            r#"{{"requestId":"{}","command":"session.read","sessionId":"{}","payload":{{"include":["messages"],"limit":2}}}}"#,
            uid(7),
            uid(1)
        ))
        .expect("session.read command");
        let crate::sync::command::CommandPayload::SessionRead(payload) = command.payload.clone()
        else {
            panic!("payload is session.read");
        };
        let outcome = session_read(
            &source,
            &Actor::LocalCli,
            &command,
            payload,
            &NegotiatedFeatures::new(Vec::new()),
        )
        .await
        .expect("多页 Agent 正文不应以 resource.result_too_large 失败");

        let CommandResultPayload::SessionRead(result) = outcome else {
            panic!("session.read returns SessionReadResult");
        };
        assert!(!result.has_earlier, "已读到最早一条：hasEarlier 必须为假");
        let messages = result.resources.messages.expect("messages requested");
        assert_eq!(messages.len(), 451, "450 条 Agent 正文 + 1 条用户输入");
        let ids = messages
            .iter()
            .map(|message| message.message_id.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), messages.len(), "分页不得重复同一条消息");
        let order = messages
            .iter()
            .map(|message| {
                (
                    message.created_at.as_str(),
                    message.message_id.as_str().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(order, sorted, "合并页必须按 (createdAt, messageId) 升序");
        assert_eq!(
            messages.first().expect("first").message_id,
            Uuid::parse(&uid(1000)).expect("message id")
        );
        assert_eq!(
            messages.last().expect("last").message_id,
            Uuid::parse(&uid(1449)).expect("message id")
        );
        assert!(
            ids.contains(&Uuid::parse(&uid(5000)).expect("message id")),
            "事件流里的用户输入必须仍在合并页内"
        );

        let queries = source.queries.lock().expect("query log");
        assert_eq!(queries.len(), 3, "初始页 + 两个更早的 Agent 正文页");
        assert!(queries[0].0.is_none(), "首批没有游标");
        assert_eq!(
            queries[1]
                .0
                .as_ref()
                .map(|cursor| cursor.message_id.clone()),
            Some(MessageId::new(&uid(1250)).expect("message id")),
            "第二个游标是初始页最早一条，而不是重复首批"
        );
        assert_eq!(
            queries[2]
                .0
                .as_ref()
                .map(|cursor| cursor.message_id.clone()),
            Some(MessageId::new(&uid(1050)).expect("message id")),
            "第三个游标是第二页最早一条"
        );
    }
}
