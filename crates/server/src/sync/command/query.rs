//! 六条查询命令的用例面调用与 core → wire 投影（§11.2/§11.5；任务 2.3/2.4、3.1）。
//!
//! 四条边界：
//!
//! - **同步完成**：六条查询命令都在本模块内直接算出终态，返回**一次** `completed` 或 `rejected`，
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

use std::collections::BTreeMap;
use std::str::FromStr as _;
use std::sync::Arc;

use acp_core::broker::Broker;
use acp_core::model::{
    AcpRaw as CoreAcpRaw, Actor, CommandRecord, ConfigOption as CoreConfigOption, ConfigOptionKind,
    ConfigValue, ModeRef as CoreModeRef, OwnedSessionRef, RawUnavailableReason, RemoteSessionRef,
    ResourceOrigin, Sequence, SessionId, SessionReference, SessionState as CoreSessionState,
};
use acp_core::ports::{HistoryInclude, HistoryQuery, OwnedEventRecord, ReplayLimit};
use acp_core::use_cases::{ModeListing, SyncSessionQuery, UseCases};
use sync_protocol::command::{
    Command, CommandResultPayload, CommandStatus as WireCommandStatus, CommandStatusRecord,
    ConfigListResult, IncludeList, SessionListResult, SessionRead, SessionReadResources,
    SessionReadResult, WorkspaceBrowse, WorkspaceBrowseEntry, WorkspaceBrowseResult,
};
use sync_protocol::common::{
    AgentContentBlock, Base64Url, ConfigOption as WireConfigOptionEntry, ConfigOptionType,
    ConfigOptionValue, ConfigOptionView as WireConfigOptionView, DecimalString,
    ModeRef as WireModeRef, ModeState as WireModeState, NonEmptyText, Nullable, OriginBlock,
    RawAcp, RawAcpUnavailableReason, RawObject, SessionAgentRef, SessionState as WireSessionState,
    SessionSummary as WireSessionSummary, Text, Timestamp, Uuid, WorkspaceRef as WireWorkspaceRef,
};
use sync_protocol::error::ErrorCode;
use sync_protocol::event::EventType;
use sync_protocol::sync::{MessageRole, SnapshotItemHistory, SnapshotItemMessage};
use sync_protocol::views::{View, project};
use tracing::debug;

use crate::sync::event::{FILE_DIFF_FEATURE, RAW_PAYLOAD_FEATURE};
use crate::sync::snapshot::{LOCAL_CATALOG_FEATURE, REMOTE_ORIGIN_FEATURE};
use crate::sync::subscribe::cursor::CursorFault;
use crate::sync::subscribe::{NegotiatedFeatures, OwnerReachability};

use super::{core_request_id, details_object, port_rejection, wire_public_error, wire_timestamp};

/// `core.session-history.v1`：`session.read` 的 `include: ["history"]` 需要协商该 feature（§11.5）。
///
/// 未协商却请求了它时**显式**回 `protocol.feature_required`（`details.features` 给出缺的那个），
/// MUST NOT 静默忽略该 include 后返回一份看似完整的空结果。
pub const SESSION_HISTORY_FEATURE: &str = "core.session-history.v1";

/// core 表达「游标早于仍可读的下界」的那条静态文本（`storage-sqlite` 的 `read_session`：
/// `PortError::InvalidRequest("history cursor precedes the retained window")`）。
///
/// 这里给出本 crate 内的唯一一份取值，让 [`super::port_rejection`] 与 `session.read` 自己的消息游标判定
/// （见 [`session_read`]）收敛到同一个 [`Rejection::CursorInvalid`]：§11.5 要求对端看到
/// `sync.cursor_invalid` + `details.reason = "cursor_expired"`，落进通配臂则变成
/// `protocol.schema_invalid`——另一类事实（形状错误），会让客户端以为是自己发的帧不合法。
pub const RETAINED_WINDOW_EXCEEDED: &str = "history cursor precedes the retained window";

/// `session.read` 单页消息数的服务端上限（客户端给的 `limit` 收敛到它；§11.5「超过服务端为该会话配置的
/// 明细条数上限时，服务端收敛到自己的上限并照常返回」）。
const MAX_PAGE_MESSAGES: u32 = 200;

/// `session.read` 省略 `limit` 时的默认页大小（§11.5：默认页是最近 20 条用户输入）。
const DEFAULT_PAGE_MESSAGES: u32 = 20;

/// 单次 `session.read` 扫描事件的上限（超出即显式失败，见 [`session_read`]）。
const MAX_SCAN_EVENTS: u64 = 20_000;

/// 扫描事件时的单批取行数（与 §14 的 `maxReplayEventsPerBatch` 同量级）。
const SCAN_BATCH_EVENTS: u32 = 2_000;

/// 内嵌 ACP 原文的单条上限（§10.1/§11.5：超过即 `rawAcp.unavailable{size_limit}`）。
const MAX_INLINE_ACP_BYTES: usize = 256 * 1024;

/// 单个文本块的字符上限（`common.schema.json#/$defs/agentContentBlock` 的 `text.maxLength`）。
const MAX_TEXT_CHARS: usize = 262_144;

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
        include_imported,
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
pub async fn session_read(
    core: &Arc<UseCases>,
    actor: &Actor,
    command: &Command,
    payload: SessionRead,
    features: &NegotiatedFeatures,
) -> Result<CommandResultPayload, Rejection> {
    let Some(session) = command.session_id.as_ref() else {
        // schema 的 `required` 已保证出现；保留判定只为不 panic。
        return Err(Rejection::Schema("session.read requires sessionId"));
    };
    let Some(session_id) = SessionId::new(session.as_str()).ok() else {
        return Err(Rejection::Schema("sessionId is not a session id"));
    };
    let include = history_include(&payload.include)?;
    if include.history && !features.contains(SESSION_HISTORY_FEATURE) {
        return Err(Rejection::FeatureRequired(SESSION_HISTORY_FEATURE));
    }
    for (requested, resource) in [
        (include.turns, "turns"),
        (include.pending_interactions, "pending_interactions"),
        (include.capabilities, "capabilities"),
    ] {
        if requested {
            // core 读面给不出投影这些资源所需的取值（见模块文档），因此显式拒绝而不是省略。
            return Err(Rejection::UnsupportedResource(resource));
        }
    }
    let page_limit = payload
        .limit
        .as_ref()
        .map_or(DEFAULT_PAGE_MESSAGES, |limit| {
            // `limit` 是 `UIntAtLeast<1>`（u64）而页大小是 `u32`：转换必须**饱和**——`as u32` 会让
            // ≥ 2^32 的取值回绕（2^32 → 0，再被下面的 clamp 抬到 1），把 §11.5 的「超过服务端上限时
            // 收敛到上限并照常返回」变成「只给一条」。
            u32::try_from(limit.get()).unwrap_or(MAX_PAGE_MESSAGES)
        })
        .clamp(1, MAX_PAGE_MESSAGES);
    let before_history = match payload.before_history.as_ref() {
        Some(value) => Some(parse_sequence(value.as_str())?),
        None => None,
    };
    let cursor = payload.before.as_ref().map(|before| {
        (
            before.created_at.as_str().to_owned(),
            before.message_id.clone(),
        )
    });

    let mut batch = core
        .read_session(
            actor,
            HistoryQuery {
                session: session_id.clone(),
                include,
                after: None,
                before_history,
                limit: ReplayLimit::new(page_limit),
            },
        )
        .await
        .map_err(|error| port_rejection(&error))?;
    // 首批同时给出 `history` 那一页与 live config；后续批次只为把事件读到 head，因此两者都只取首批。
    let config_options = std::mem::take(&mut batch.config);
    let history = std::mem::take(&mut batch.history);
    let history_has_earlier = batch.history_has_earlier;
    let mut merger = MessageMerger::new(cursor);
    let mut scanned = 0_u64;
    let mut complete = false;
    loop {
        if include.messages {
            for record in &batch.events {
                merger.absorb(record);
            }
            scanned += batch.events.len() as u64;
        }
        let Some(next) = batch.next.clone() else {
            complete = true;
            break;
        };
        if !include.messages {
            // 不请求消息时不必把事件读到 head：`history` 那一页已经在首批里取完。
            complete = true;
            break;
        }
        if scanned >= MAX_SCAN_EVENTS {
            break;
        }
        batch = core
            .read_session(
                actor,
                HistoryQuery {
                    session: session_id.clone(),
                    include: HistoryInclude {
                        messages: true,
                        ..HistoryInclude::default()
                    },
                    after: Some(next),
                    before_history: None,
                    limit: ReplayLimit::new(SCAN_BATCH_EVENTS),
                },
            )
            .await
            .map_err(|error| port_rejection(&error))?;
    }
    if !complete {
        debug!(
            event = "sync.session_read_window_exceeded",
            session_id = session_id.as_str(),
            scanned,
            "the session history exceeds the readable window of a single read"
        );
        return Err(Rejection::Code(
            ErrorCode::ResourceResultTooLarge,
            "the session history exceeds the readable window of a single read",
        ));
    }
    let page = merger.finish(page_limit);
    // 消息游标早于保留窗口：显式拒绝，不静默返回一页「看起来已到最早一条」的空页（§11.5）。只在请求了
    // `messages` 时判定——没请求消息就没有消息游标可谈（那时 `before` 不描述任何返回的页）。
    if include.messages && page.cursor_expired {
        debug!(
            event = "sync.session_read_cursor_expired",
            session_id = session_id.as_str(),
            "the message cursor precedes the readable window"
        );
        return Err(Rejection::CursorInvalid(CursorFault::CursorExpired));
    }
    let messages = if include.messages {
        Some(
            page.messages
                .iter()
                .map(|message| message.to_wire(session))
                .collect::<Option<Vec<_>>>()
                .ok_or(Rejection::Code(
                    ErrorCode::InternalUnavailable,
                    "a message cannot be projected",
                ))?,
        )
    } else {
        None
    };
    let negotiate_acp = features.contains(RAW_PAYLOAD_FEATURE);
    let negotiate_file_diff = features.contains(FILE_DIFF_FEATURE);
    let history_items = if include.history {
        Some(
            history
                .iter()
                .map(|record| wire_history_item(record, negotiate_acp, negotiate_file_diff))
                .collect::<Option<Vec<_>>>()
                .ok_or(Rejection::Code(
                    ErrorCode::InternalUnavailable,
                    "a history entry cannot be projected",
                ))?,
        )
    } else {
        None
    };
    let config = if include.config_options {
        Some(
            config_options
                .iter()
                .map(wire_config_option)
                .collect::<Option<Vec<_>>>()
                .ok_or(Rejection::Code(
                    ErrorCode::InternalUnavailable,
                    "a config option cannot be projected",
                ))?,
        )
    } else {
        None
    };
    Ok(CommandResultPayload::SessionRead(SessionReadResult {
        session_id: Uuid::parse(session.as_str())
            .map_err(|_| Rejection::Schema("sessionId is not a uuid"))?,
        resources: SessionReadResources {
            messages,
            turns: None,
            pending_interactions: None,
            config_options: config,
            capabilities: None,
            history: history_items,
        },
        has_earlier: page.has_earlier,
        history_has_earlier: if include.history {
            Some(history_has_earlier)
        } else {
            None
        },
    }))
}

/// `command.status`：目标请求的当前记录（§11.4）。
///
/// 目标不存在或**对本设备不可见**时一律 `command.not_found`（§11.4：避免权限侧信道）。可见性由 core 的
/// `find_request(request, actor)` 给出——它按 `(actor, requestId)` 查，永远只返回该 actor 自己的请求。
pub async fn command_status(
    core: &Arc<UseCases>,
    actor: &Actor,
    payload: WireCommandStatus,
) -> Result<CommandResultPayload, Rejection> {
    let Some(target) = core_request_id(&payload.target_request_id) else {
        return Err(Rejection::Schema("targetRequestId is not a request id"));
    };
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
    Ok(CommandResultPayload::CommandStatus(wire))
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
pub(super) fn wire_session(
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
        CoreSessionState::Queued => WireSessionState::Queued,
        CoreSessionState::Running => WireSessionState::Running,
        CoreSessionState::WaitingInput => WireSessionState::WaitingInput,
        CoreSessionState::WaitingPermission => WireSessionState::WaitingPermission,
        CoreSessionState::Failed => WireSessionState::Failed,
        CoreSessionState::Closed => WireSessionState::Closed,
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

/// core 的 `ConfigOption` → wire `configOptionView`。
fn wire_config_option(option: &CoreConfigOption) -> Option<WireConfigOptionView> {
    let current_value = match option.current() {
        ConfigValue::Text(text) => ConfigOptionValue::Text(text.as_str().to_owned()),
        ConfigValue::Select(select) => ConfigOptionValue::Text(select.as_str().to_owned()),
        ConfigValue::Boolean(flag) => ConfigOptionValue::Boolean(*flag),
    };
    let candidates = option
        .options()
        .iter()
        .map(|entry| {
            Some(WireConfigOptionEntry {
                value: NonEmptyText::<256>::parse(entry.value().as_str()).ok()?,
                name: NonEmptyText::<256>::parse(entry.name()).ok()?,
                description: match entry.description() {
                    Some(text) => Nullable::value(Text::<1024>::parse(text).ok()?),
                    None => Nullable::null(),
                },
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(WireConfigOptionView {
        id: NonEmptyText::<256>::parse(option.id().as_str()).ok()?,
        name: NonEmptyText::<256>::parse(option.name()).ok()?,
        description: match option.description() {
            Some(text) => Nullable::value(Text::<1024>::parse(text).ok()?),
            None => Nullable::null(),
        },
        category: match option.category() {
            Some(text) => Nullable::value(Text::<128>::parse(text).ok()?),
            None => Nullable::null(),
        },
        option_type: match option.kind() {
            ConfigOptionKind::Select => ConfigOptionType::Select,
            ConfigOptionKind::Boolean => ConfigOptionType::Boolean,
        },
        current_value,
        // 空候选在 wire 上是**键缺席**（§11.5 的 `options` 不在 required 里），不是空数组。
        options: if candidates.is_empty() {
            None
        } else {
            Some(candidates)
        },
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

/// 一条结构化历史条目 → wire `snapshotItem.history`（与 `event` 帧的 `payload` 同形：同一个 `view` 原文、
/// 同一条 `acp.raw-payload.v1` 门控，以及同一条 `core.file-diff.v1` 门控）。
///
/// 两条门控共用事件腿的判定（[`crate::sync::event::gate_file_diff`]）：历史条目里装的就是同一条
/// `file.changed` 视图，未协商差异行能力的连接在**任何一条**下发路径上都不该收到那两个键。
fn wire_history_item(
    record: &OwnedEventRecord,
    negotiate_acp: bool,
    negotiate_file_diff: bool,
) -> Option<SnapshotItemHistory> {
    let event = &record.event;
    let sequence = event.session_sequence.as_ref()?;
    let view = RawObject::parse(record.payload.view.as_str()).ok()?;
    // `view` 是库里原文（`payload_json`）：未协商差异行时只在 `file.changed` 上去掉那两个键，其余字节
    // 原样（不解析、不重新序列化）。
    let view =
        crate::sync::event::gate_file_diff(event.event_type.as_str(), &view, negotiate_file_diff)
            .unwrap_or(view);
    Some(SnapshotItemHistory {
        event_id: Uuid::parse(event.id.as_str()).ok()?,
        event_type: EventType::parse(event.event_type.as_str()).ok()?,
        session_sequence: decimal(&sequence.get().to_string())?,
        created_at: Timestamp::parse(event.created_at.as_str()).ok()?,
        view,
        acp: if negotiate_acp {
            wire_raw_acp(record.payload.acp.as_ref())?
        } else {
            None
        },
    })
}

/// core 的 `payload.acp` → wire 的 `rawAcp`（`Complete`/`Unavailable` 两个分支）。
///
/// 口径与 [`crate::sync::subscribe::event`] 的同名映射**逐条相同**（media type 固定 `application/json`、
/// 内嵌上限 256 KiB、超限改判 `size_limit`、不可用原因三种一一对应），因为那是 §10.1 的登记规则；本模块
/// **拿不到**那两个函数（它们是订阅模块的私有实现），而历史条目的 `acp` 必须与事件帧的 `acp` 同源同规则，
/// 因此这份规则在本目录里各自持有（代价与 `design.md` D1 登记的「256 KiB 两处常量」同一性质）。
fn wire_raw_acp(acp: Option<&CoreAcpRaw>) -> Option<Option<RawAcp>> {
    Some(match acp {
        Some(CoreAcpRaw::Available {
            media_type,
            raw_json,
            byte_length,
            sha256,
        }) => {
            let digest = Base64Url::<32>::parse(sha256.as_str()).ok()?;
            if media_type != "application/json" || raw_json.len() > MAX_INLINE_ACP_BYTES {
                Some(RawAcp::Unavailable {
                    reason: if media_type != "application/json" {
                        RawAcpUnavailableReason::StorageFailure
                    } else {
                        RawAcpUnavailableReason::SizeLimit
                    },
                    byte_length: decimal(&byte_length.to_string())?,
                    sha256: Nullable::value(digest),
                })
            } else {
                Some(RawAcp::Complete {
                    // `rawJson` 是 JSON **字符串**（内容是 ACP 原文档文本），因此编码一次字符串字面量；
                    // 绝不先解析再重新序列化（那会改写空白、键序与超出 64 位的整数字面量）。
                    raw_json: serde_json::value::RawValue::from_string(
                        serde_json::to_string(raw_json).ok()?,
                    )
                    .ok()?,
                    byte_length: decimal(&byte_length.to_string())?,
                    sha256: digest,
                })
            }
        }
        Some(CoreAcpRaw::Unavailable {
            reason,
            byte_length,
            sha256,
        }) => Some(RawAcp::Unavailable {
            reason: match reason {
                RawUnavailableReason::SizeLimit => RawAcpUnavailableReason::SizeLimit,
                RawUnavailableReason::RetentionExpired => RawAcpUnavailableReason::RetentionExpired,
                RawUnavailableReason::StorageFailure => RawAcpUnavailableReason::StorageFailure,
            },
            byte_length: decimal(&byte_length.to_string())?,
            sha256: match sha256 {
                Some(digest) => Nullable::value(Base64Url::<32>::parse(digest.as_str()).ok()?),
                None => Nullable::null(),
            },
        }),
        None => None,
    })
}

// ---------------------------------------------------------------------------------------------
// 消息页：从持久化事件折叠出一页 `resources.messages`
// ---------------------------------------------------------------------------------------------

/// 一条待投影的消息（取值只来自持久化事件，不重算、不补全）。
#[derive(Debug, Clone)]
struct Message {
    message_id: Uuid,
    role: MessageRole,
    content: Vec<AgentContentBlock>,
    created_at: Timestamp,
    turn_id: Option<Uuid>,
}

impl Message {
    /// wire 的 `snapshotItem.messages` 元素。
    ///
    /// `status` 是 §11.5 未登记词表的开放串（`NonEmptyText<32>`，客户端不消费它）：这里的取值
    /// `completed` 表达的是**持久化日志里该消息的正文已完整**——用户消息的来源是它自己的全部 delta
    /// （发送即完整），Agent 消息的来源是 turn 收尾时由 Broker 折叠出的 `agent.message.completed`。
    fn to_wire(&self, session: &Uuid) -> Option<SnapshotItemMessage> {
        Some(SnapshotItemMessage {
            message_id: self.message_id.clone(),
            session_id: session.clone(),
            role: self.role,
            content: self.content.clone(),
            status: NonEmptyText::<32>::parse("completed").ok()?,
            created_at: self.created_at.clone(),
            turn_id: match self.turn_id.as_ref() {
                Some(turn) => Nullable::value(turn.clone()),
                None => Nullable::null(),
            },
        })
    }
}

/// 把事件流折叠成消息页的累加器（`session.read` 的 `messages` 面）。
///
/// 折叠规则：
///
/// - `user.message.delta` 是用户输入的持久化载体（正文的权威在 `messages`）：同一 `messageId` 的多个
///   delta 按 `deltaIndex` 升序拼成文本，`createdAt` 取最早一条，`turnId` 取该消息的 turn；
/// - `agent.message.completed` 是 Agent 消息的收尾正文（Broker 在 turn 结束时生成），直接采用其
///   `content` 块数组；`agent.message.delta`/`agent.thought.delta` **不参与**——它们在事件轨道上是流式
///   增量（客户端另有易失叠加层），把它们当持久正文会与收尾正文形成第二份来源；
/// - 其它事件类型不产生消息（历史条目是另一套资源，见 [`wire_history_item`]）。
#[derive(Debug)]
struct MessageMerger {
    /// 复合游标（`{createdAt, messageId}`）：只收集**严格早于**它的消息；`None` = 不按游标收窄。
    cursor: Option<(String, Uuid)>,
    users: BTreeMap<Uuid, UserMessage>,
    agents: Vec<Message>,
}

#[derive(Debug, Default)]
struct UserMessage {
    created_at: Option<Timestamp>,
    turn_id: Option<Uuid>,
    parts: Vec<(u64, String)>,
}

/// 一页消息 + 页标记（`cursor_expired` 见 [`MessageMerger::finish`]）。
struct MessagePage {
    messages: Vec<Message>,
    has_earlier: bool,
    cursor_expired: bool,
}

impl MessageMerger {
    fn new(cursor: Option<(String, Uuid)>) -> Self {
        Self {
            cursor,
            users: BTreeMap::new(),
            agents: Vec::new(),
        }
    }

    /// 收下一条持久化事件（不承载消息正文的事件类型直接跳过）。
    fn absorb(&mut self, record: &OwnedEventRecord) {
        let event_type = record.event.event_type.as_str();
        if event_type != "user.message.delta" && event_type != "agent.message.completed" {
            return;
        }
        let Ok(view_object) = RawObject::parse(record.payload.view.as_str()) else {
            return;
        };
        let Ok(view) = project(event_type, &view_object) else {
            // 形状不符只记诊断并跳过该事件：`session.read` 是读取面，不因为一条坏行让整页失败
            // （事件轨道那一侧的显式降级由 `sync::event` 负责）。
            debug!(
                event = "sync.session_read_view_skipped",
                event_type, "the persisted view does not match the registered shape"
            );
            return;
        };
        match view {
            View::UserMessageDelta(delta) => {
                let created_at = Timestamp::parse(record.event.created_at.as_str()).ok();
                let entry = self.users.entry(delta.message_id.clone()).or_default();
                if entry.created_at.is_none() {
                    entry.created_at = created_at;
                }
                entry.turn_id = Some(delta.turn_id.clone());
                let index = delta
                    .delta_index
                    .as_str()
                    .parse::<u64>()
                    .unwrap_or(u64::MAX);
                entry.parts.push((index, delta.text.as_str().to_owned()));
            }
            View::AgentMessageCompleted(completed) => {
                let Some(created_at) = Timestamp::parse(record.event.created_at.as_str()).ok()
                else {
                    return;
                };
                self.agents.push(Message {
                    message_id: completed.message_id.clone(),
                    role: MessageRole::Agent,
                    content: completed.content.clone(),
                    created_at,
                    turn_id: Some(completed.turn_id.clone()),
                });
            }
            _ => {}
        }
    }

    /// 折叠出一页：按 `createdAt` 升序（同一时刻以 `messageId` 稳定定序）、只保留游标之前的消息，再取
    /// **最新**一页（页里最多 `limit` 条用户输入，与 §11.5 的默认页口径一致）。
    ///
    /// 同时给出 `cursor_expired`：游标在场、这一页为空、页标记也说没有更早内容、且游标**严格早于**本次
    /// 读到的**最早一条可读消息**（或本次一条可读消息都没有）时，唯一能解释「游标指向的东西不在这份读
    /// 口里」的物理事实就是它已被保留策略清理（更早的行按 §9.7 前缀清理/压缩后不再可读）。比较必须是
    /// 严格的：游标恰好等于最早一条可读消息是**合法**的（客户端翻到最早一条后再问一页，得到的空页正是
    /// 「确实没有更早的」），不得报过期。
    ///
    /// 这份读口能给出的下界就是「最早一条可读消息」：`before` 只可能来自上一页返回的消息，而消息页是
    /// 单调前缀清理的，游标低于该下界即意味着它指向的行已不可读。窗口**内部**的空洞（例如窗口没前移、
    /// 但某条消息的 delta 被压缩掉）从这一份读口看不出来，不在本判定的覆盖范围内。
    fn finish(self, limit: u32) -> MessagePage {
        let MessageMerger {
            cursor,
            users,
            agents,
        } = self;
        let mut messages: Vec<Message> = users
            .into_iter()
            .filter_map(|(message_id, user)| {
                let created_at = user.created_at?;
                let mut parts = user.parts;
                parts.sort_by_key(|(index, _)| *index);
                let text = parts.into_iter().map(|(_, text)| text).collect::<String>();
                Some(Message {
                    message_id,
                    role: MessageRole::User,
                    content: text_blocks(&text)?,
                    created_at,
                    turn_id: user.turn_id,
                })
            })
            .chain(agents)
            .collect();
        // `createdAt` 定宽（UTC 毫秒），字典序即时间序；同一时刻以 `messageId` 作稳定 tie-breaker。
        messages.sort_by(|left, right| {
            (left.created_at.as_str(), &left.message_id)
                .cmp(&(right.created_at.as_str(), &right.message_id))
        });
        // 可读窗口的下界（本次读口内最早的一条消息），必须在按游标收窄**之前**取。
        let earliest = messages.first().map(|message| {
            (
                message.created_at.as_str().to_owned(),
                message.message_id.clone(),
            )
        });
        messages.retain(|message| match &cursor {
            Some((created_at, message_id)) => {
                (message.created_at.as_str(), &message.message_id)
                    < (created_at.as_str(), message_id)
            }
            None => true,
        });
        // 最新一页：从尾部往前收，直到这一页已含 `limit` 条用户输入；页首之前的 Agent 消息留在页内
        // （它们属于这一页的 turn）。
        let mut start = messages.len();
        let mut users_in_page = 0_u32;
        while start > 0 {
            let candidate = &messages[start - 1];
            let is_user = candidate.role == MessageRole::User;
            if is_user && users_in_page == limit {
                break;
            }
            if is_user {
                users_in_page += 1;
            }
            start -= 1;
        }
        let has_earlier = start > 0;
        messages.drain(..start);
        let cursor_expired = match (&cursor, &earliest) {
            // 游标早于最早一条可读消息：它指向的行已不在读口内。
            (Some(cursor), Some(earliest)) => {
                messages.is_empty()
                    && !has_earlier
                    && (cursor.0.as_str(), &cursor.1) < (earliest.0.as_str(), &earliest.1)
            }
            // 一条可读消息都没有：任何游标都指向读口之外的区间。
            (Some(_), None) => true,
            (None, _) => false,
        };
        MessagePage {
            messages,
            has_earlier,
            cursor_expired,
        }
    }
}

/// 一段文本 → `agentContentBlock` 列表（单个块的文本上限是 262144 **字符**，超出即切分为多个块：既不
/// 丢字也不改写内容；空白串产生一个空文本块，与"消息存在但正文为空"一致）。
fn text_blocks(text: &str) -> Option<Vec<AgentContentBlock>> {
    let mut blocks = Vec::new();
    let mut chunk = String::new();
    for character in text.chars() {
        if chunk.chars().count() >= MAX_TEXT_CHARS {
            blocks.push(AgentContentBlock::Text {
                text: Text::<MAX_TEXT_CHARS>::parse(&chunk).ok()?,
            });
            chunk.clear();
        }
        chunk.push(character);
    }
    blocks.push(AgentContentBlock::Text {
        text: Text::<MAX_TEXT_CHARS>::parse(&chunk).ok()?,
    });
    Some(blocks)
}
