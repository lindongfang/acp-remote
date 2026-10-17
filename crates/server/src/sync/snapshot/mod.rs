//! 快照组装：三类清单资源的投影、分块与 `snapshotDigest`
//! （`docs/SYNC_PROTOCOL.md` §9.4；变更 `sync-server-connection` 的任务 4.4/4.5/4.6）。
//!
//! 三条边界：
//!
//! - **只承载清单资源**：`sessions`/`workspaces`/`agents` 三类（[`SnapshotResource::ALL`]）。六类
//!   session-scoped 明细资源（`messages`/`turns`/`pending_interactions`/`config_options`/
//!   `capabilities`/`history`）**不进快照**，无论会话来源是本节点自有还是已导入——因此本模块**没有**
//!   读取明细的代码路径，快照体积只随会话**条数**增长，与消息总量无关（§9.4）。
//! - **`sessions` 恒发一个 chunk**：会话集合为空时也发一个空数组（`chunkCount: 0` 是形状错误），而
//!   `workspaces`/`agents` 受 `core.local-catalog.v1` 门控——未协商时既不发送、也不以空数组占位
//!   （§9.4）。`chunkCount` 因此只可能是 `1` 或 `3`，且**等于实际下发的 chunk 数**。
//! - **`snapshotDigest` 覆盖将要发出的那几份字节**：逐 chunk 对 `send_with_frame` 回传的**完整帧文本**
//!   求 SHA-256，按 `chunkIndex` 顺序连接后再求一次 SHA-256，无填充 base64url 表示（§9.4）。**不得**
//!   重新序列化后计算：客户端按收到的原始文本复算，重新序列化会得到不同的字节（键序/空白/转义）。
//!
//! 投影纪律：`sessions` 资源来自 core 的 `sync_visible_sessions`（自有 + 已导入合并、已按设备
//! `session.list` 授权），本模块只做 core → wire 的字段映射，**不重算可见性**；`workspaces`/`agents`
//! 来自 `LocalConfigStore`（`require_local` 卡死，只能以 `Actor::LocalCli` 读），其对外可见性由
//! 「设备已过 `session.list` 授权 ∧ 已协商 `core.local-catalog.v1`」两把闸共同决定。目录元素只出
//! `alias`/`displayName`/`agentId`/`default`，**外加** `core.workspace-path.v1` 门控下的可选 `path`
//! ——已登记目录的规范化路径只在协商了该 feature 的连接上出现（`SECURITY_DESIGN.md` §12.3 的现行口径：
//! 客户端自己提交的目录不被回显，本机路径的可见面只有快照的这一个键与 §11.5 的目录浏览结果）。

use acp_core::model::{
    Actor, GlobalCursor, PortError, RemoteSessionRef, ResourceOrigin,
    SessionState as CoreSessionState, SessionSummary as CoreSessionSummary,
    WorkspaceCatalogEntry as CoreWorkspaceCatalogEntry,
};
use acp_core::use_cases::{SyncSessionQuery, UseCases};
use sha2::{Digest as _, Sha256};
use sync_protocol::common::{
    AgentCatalogEntry, Base64Url, BoundedU64, Cursor, DecimalString, ModeRef as WireModeRef,
    NonEmptyText, Nullable, OriginBlock, ProtocolVersionV1, SessionAgentRef,
    SessionState as WireSessionState, SessionSummary as WireSessionSummary, Text, Timestamp, Uuid,
    WorkspaceCatalogEntry as WireWorkspaceCatalogEntry, WorkspaceRef as WireWorkspaceRef,
};
use sync_protocol::envelope::MessageType;
use sync_protocol::sync::{SnapshotBegin, SnapshotChunk, SnapshotEnd, SnapshotItems};
use tracing::debug;

use crate::sync::conn::{ConnectionHandle, SendFault};
use crate::sync::subscribe::NegotiatedFeatures;

/// 组装一次快照所需的调用方判定（本模块不自己决定这三件事）。
pub struct SnapshotInputs<'a> {
    /// 设备侧 actor：只用于区分「已认证设备」与其它主体（设备方向没有可见性过滤，节点主体仍按
    /// Export/grant 判定，判定在 core 内完成）。
    pub actor: &'a Actor,
    /// 本次连接的 feature 协商结果（目录资源与 imported 会话的门控）。
    pub features: &'a NegotiatedFeatures,
    /// Owner 可达性（`origin.online` 的唯一来源）：运行时事实由调用方按自己的连接事实给出，core 不缓存
    /// 它（`MODULE_ARCHITECTURE.md` §4.1）。
    pub reachable: &'a (dyn Fn(&RemoteSessionRef) -> bool + Send + Sync),
}

/// 一次快照的资源计划（`begin`/`end` 的 `chunkCount` 与实发 chunk 一一对应）。
#[derive(Debug)]
pub struct SnapshotPlan {
    snapshot_id: Uuid,
    cursor: Cursor,
    chunks: Vec<SnapshotChunk>,
}

impl SnapshotPlan {
    /// 本次快照的 id（`sync.snapshot_begin`/`chunk`/`end` 同值）。
    pub fn snapshot_id(&self) -> &Uuid {
        &self.snapshot_id
    }

    /// 快照屏障游标（同时出现在 `begin` 与 `end` 的 `cursor` 字段里）。
    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    /// 实际下发的 chunk 数（`1..=3`，等于 `chunks.len()`）。
    pub fn chunk_count(&self) -> u64 {
        self.chunks.len() as u64
    }

    /// 按 `chunkIndex` 升序的 chunk 列表（index 从 `0` 起连续）。
    pub fn chunks(&self) -> &[SnapshotChunk] {
        &self.chunks
    }
}

/// 整条快照的发出去向：`begin` → `chunks` → `end` 都经**同一条**有界出站队列（§14 的背压口径）。
#[derive(Debug)]
pub struct SnapshotSent {
    /// 实发 chunk 数（等于 `begin`/`end` 声明的 `chunkCount`）。
    pub chunk_count: u64,
    /// `snapshotDigest`：对实发帧文本按序计算的摘要。
    pub digest: Base64Url<32>,
}

/// 快照未能完整发出的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFault {
    /// 本机装配失败（id/序号/摘要/base64 文本构造失败）：接线错误，调用方按内部不可用失败关闭。
    Local,
    /// 出站队列拒绝（高水位或连接已结束）：调用方停读、让客户端重连后重新请求（§14）。
    Send(SendFault),
}

/// 组装设备可见的清单资源（§9.4）。
///
/// 顺序固定为 `sessions` → `workspaces` → `agents`，`chunkIndex` 与之一致。`sessions` 恒为第一块；
/// 目录资源在未协商 `core.local-catalog.v1` 时**整个缺席**（不是空数组）。任一投影失败都返回错误：
/// 会话摘要映射不出来意味着存储里出现了不满足值对象约束的数据，静默跳过会让对端看到一个"少了几条"
/// 却无从察觉的快照（`sync-event-delivery` 的「无权会话不进入结果」是**授权**结论，不是投影失配）。
pub async fn plan(
    core: &UseCases,
    inputs: SnapshotInputs<'_>,
    cursor: &GlobalCursor,
) -> Result<SnapshotPlan, PortError> {
    let Some(snapshot_id) = Uuid::parse(core.ids().message_id().as_str()).ok() else {
        return Err(PortError::Corrupt("snapshot id"));
    };
    let Some(wire_barrier) = wire_cursor(cursor) else {
        return Err(PortError::Corrupt("snapshot cursor"));
    };
    // `sessions`：自有 + 已导入（已导入仅在协商 `resource.remote-origin.v1` 时纳入）；设备方向不再有
    // 按授权的过滤，本层因此只做投影，不重算任何可见性。
    let include_imported = inputs.features.contains(REMOTE_ORIGIN_FEATURE);
    let query = SyncSessionQuery {
        include_imported,
        ..SyncSessionQuery::default()
    };
    let summaries = core
        .sync_visible_sessions(inputs.actor, query, inputs.reachable)
        .await?;
    let catalog_visible = inputs.features.contains(LOCAL_CATALOG_FEATURE);
    let sessions = summaries
        .iter()
        .map(|summary| wire_session(summary, catalog_visible))
        .collect::<Option<Vec<_>>>()
        .ok_or(PortError::Corrupt("session summary"))?;
    let mut chunks = vec![chunk(
        snapshot_id.clone(),
        0,
        SnapshotItems::Sessions(sessions),
    )?];
    if catalog_visible {
        // 目录资源的两笔读都以 `Actor::LocalCli` 发出：`workspaces`/`profiles` 被 `require_local` 守卫
        // （它们同时保护着本机管理读面），设备无权直接读；对端可见性由上面两把闸决定，因此这里**只能**
        // 以本机身份代读——放宽 `require_local` 会连带打开一批本机管理面（`design.md` D2 的否决理由）。
        //
        // `workspaces` 的投影（含 `path` 键的门控）在 core 的 `sync_workspace_catalog` 里：已登记目录的
        // 规范化路径只在协商了 `core.workspace-path.v1` 时出现（`include_path` 就是本连接的那个协商
        // 结果），未协商时该键**缺席**——条目是 `additionalProperties: false` 的封闭对象，只协商
        // `core.local-catalog.v1` 的旧客户端因此仍收到与本变更前逐字节相同的两个键。
        let catalog = core.sync_workspace_catalog(&Actor::LocalCli, true).await?;
        let items = catalog
            .iter()
            .map(wire_workspace)
            .collect::<Option<Vec<_>>>()
            .ok_or(PortError::Corrupt("workspace record"))?;
        let index = chunks.len() as u64;
        chunks.push(chunk(
            snapshot_id.clone(),
            index,
            SnapshotItems::Workspaces(items),
        )?);
        let profiles = core.agent_profiles(&Actor::LocalCli).await?;
        let items = profiles
            .iter()
            .map(wire_agent)
            .collect::<Option<Vec<_>>>()
            .ok_or(PortError::Corrupt("agent profile"))?;
        let index = chunks.len() as u64;
        chunks.push(chunk(
            snapshot_id.clone(),
            index,
            SnapshotItems::Agents(items),
        )?);
    }
    debug!(
        event = "sync.snapshot_prepared",
        chunks = chunks.len(),
        catalog = catalog_visible,
        imported = include_imported,
        "the sync snapshot resources were projected"
    );
    Ok(SnapshotPlan {
        snapshot_id,
        cursor: wire_barrier,
        chunks,
    })
}

/// 发送整条快照：`sync.snapshot_begin` → `sync.snapshot_chunk`（`chunkIndex` 从 `0` 起连续）→
/// `sync.snapshot_end`。
///
/// 任一步被拒（高水位/连接结束）即**停止**且不发 `snapshot_end`：客户端因此丢弃整个暂存快照、重连后
/// 重新请求（§9.4），绝不会把"少了几块"的暂存区当成完整快照提交。缓存的摘要只来自
/// [`ConnectionHandle::send_with_frame`] 回传的**真实帧文本**，因此 `snapshotDigest` 覆盖的正是对端
/// 将逐字节收到的那几份字节。
pub fn send(handle: &ConnectionHandle, plan: &SnapshotPlan) -> Result<SnapshotSent, SnapshotFault> {
    let Some(chunk_count) = BoundedU64::<1, 3>::new(plan.chunk_count()).ok() else {
        return Err(SnapshotFault::Local);
    };
    let Some(begin) = begin(plan, chunk_count) else {
        return Err(SnapshotFault::Local);
    };
    if let Err(fault) = handle.send(MessageType::SyncSnapshotBegin, &begin) {
        return Err(paused(handle, 0, fault));
    }
    let mut digests = Vec::with_capacity(plan.chunks.len());
    for part in &plan.chunks {
        match handle.send_with_frame(MessageType::SyncSnapshotChunk, part) {
            Ok(frame) => digests.push(Sha256::digest(frame.as_bytes())),
            Err(fault) => return Err(paused(handle, digests.len() as u64, fault)),
        }
    }
    let Some(snapshot_digest) = digest(&digests) else {
        return Err(SnapshotFault::Local);
    };
    let end = SnapshotEnd {
        snapshot_id: plan.snapshot_id.clone(),
        cursor: plan.cursor.clone(),
        chunk_count,
        snapshot_digest: snapshot_digest.clone(),
    };
    if let Err(fault) = handle.send(MessageType::SyncSnapshotEnd, &end) {
        return Err(paused(handle, digests.len() as u64, fault));
    }
    Ok(SnapshotSent {
        chunk_count: plan.chunk_count(),
        digest: snapshot_digest,
    })
}

/// `snapshotDigest`：逐 chunk 对**完整帧文本**求 SHA-256，按 `chunkIndex` 顺序连接后再求一次（§9.4）。
///
/// 算法与 `server::node_link::resource::snapshot_digest` 同口径（§12.4 与 §9.4 明确写了是同一规则），
/// 但**类型各自定义**：两个边界不复用对方的 DTO/函数（`AGENTS.md` §6）。
pub fn digest(frames: &[sha2::digest::Output<Sha256>]) -> Option<Base64Url<32>> {
    let mut hasher = Sha256::new();
    for frame in frames {
        hasher.update(frame);
    }
    let text = {
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
    };
    Base64Url::<32>::parse(&text).ok()
}

/// `core.local-catalog.v1` 的门控（目录资源 `workspaces`/`agents`）。
pub const LOCAL_CATALOG_FEATURE: &str = "core.local-catalog.v1";

/// `core.workspace-path.v1` 的门控（快照 `workspaces` 条目的可选键 `path`）。
///
/// 未协商时该键**缺席**（不是 `null`、不是空串）：条目是 `additionalProperties: false` 的封闭对象，
/// 只协商 `core.local-catalog.v1` 的旧客户端多收到一个键就会拒收整条消息（§9.4）。

/// `resource.remote-origin.v1` 的门控（已导入会话与其来源信息）。
pub const REMOTE_ORIGIN_FEATURE: &str = "resource.remote-origin.v1";

/// 快照屏障游标的 wire 形状。
pub fn wire_cursor(cursor: &GlobalCursor) -> Option<Cursor> {
    Some(Cursor {
        server_epoch: Uuid::parse(cursor.server_epoch.as_str()).ok()?,
        global_sequence: DecimalString::parse(&cursor.global_sequence.get().to_string()).ok()?,
    })
}

/// 一条 chunk（`chunkIndex` 是规范十进制串，`resource` 由 `items` 的 variant 派生）。
fn chunk(snapshot_id: Uuid, index: u64, items: SnapshotItems) -> Result<SnapshotChunk, PortError> {
    let Some(index) = DecimalString::parse(&index.to_string()).ok() else {
        return Err(PortError::Corrupt("snapshot chunk index"));
    };
    Ok(SnapshotChunk::new(snapshot_id, index, items))
}

/// `sync.snapshot_begin` 的 body。
fn begin(plan: &SnapshotPlan, chunk_count: BoundedU64<1, 3>) -> Option<SnapshotBegin> {
    Some(SnapshotBegin {
        snapshot_id: plan.snapshot_id.clone(),
        cursor: plan.cursor.clone(),
        schema_version: ProtocolVersionV1::new(1).ok()?,
        chunk_count,
    })
}

/// 被拒时的日志与分类（`chunks_sent` 是**已真正入队**的 chunk 数）。
fn paused(handle: &ConnectionHandle, chunks_sent: u64, fault: SendFault) -> SnapshotFault {
    debug!(
        event = "sync.snapshot_incomplete",
        connection_id = handle.connection_id().as_str(),
        chunks_sent,
        fault = ?fault,
        "the snapshot was not sent in full; the client discards the staging area"
    );
    SnapshotFault::Send(fault)
}

/// core 的会话摘要 → wire `sessionSummary`（§10.3 的最低字段集合，字段逐个映射，不增不减）。
///
/// `catalog`（`core.local-catalog.v1` 的协商结果）决定 `workspace` 键的三态：未协商时**缺席**
/// （schema 是 `additionalProperties: false`，发 `null` 会让未协商的客户端拒绝整条摘要）；协商时键在场，
/// `null` = 未分组、`WorkspaceRef` = 归属该目录。imported 会话由 core 投影为"未分组"
/// （`workspace() == None`），因此本层不会用本机目录集合替它猜归属（§9.6）。
fn wire_session(summary: &CoreSessionSummary, catalog: bool) -> Option<WireSessionSummary> {
    let agent = summary.agent();
    let workspace = if catalog {
        Some(match summary.workspace() {
            Some(reference) => {
                Nullable::value(wire_workspace_ref(reference.directory_id().as_str())?)
            }
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
        state: wire_state(summary.state()),
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
            Some(mode) => Nullable::value(WireModeRef {
                mode_id: NonEmptyText::<256>::parse(mode.mode_id().as_str()).ok()?,
                display_name: NonEmptyText::<256>::parse(mode.display_name()).ok()?,
            }),
            None => Nullable::null(),
        },
        version: DecimalString::parse(&summary.version().get().to_string()).ok()?,
        created_at: Timestamp::parse(summary.created_at().as_str()).ok()?,
        updated_at: Timestamp::parse(summary.updated_at().as_str()).ok()?,
        workspace,
    })
}

/// core 的目录资源投影 → wire `workspaceCatalogEntry`（快照 `workspaces` 资源的元素）。
///
/// 只映射 core 给出的三个取值：`alias`/`displayName` 是 `WorkspaceRef` 的两键，`path` 是可选键——**键的
/// 三态由 core 的 `sync_workspace_catalog(actor, include_path)` 决定**（`None` = 本连接未协商
/// `core.workspace-path.v1`，键缺席；`Some` = 已登记目录的规范化路径，`canonicalize` 结果剥掉平台
/// verbatim 前缀），本层不重算协商结果、也不做第二套取值规范化。这些条目是本节点规范化路径唯一的
/// **快照侧**可见面（§9.4/§12.3）；会话摘要的目录引用仍是 `workspaceRef` 的两个键。
fn wire_workspace(entry: &CoreWorkspaceCatalogEntry) -> Option<WireWorkspaceCatalogEntry> {
    Some(WireWorkspaceCatalogEntry {
        directory_id: NonEmptyText::<32>::parse(entry.directory_id().as_str()).ok()?,
        path: NonEmptyText::<4096>::parse(entry.path()).ok()?,
    })
}

fn wire_workspace_ref(directory_id: &str) -> Option<WireWorkspaceRef> {
    Some(WireWorkspaceRef {
        directory_id: NonEmptyText::<32>::parse(directory_id).ok()?,
    })
}

/// `AgentProfile` → `agentCatalogEntry`（只有 `agentId`/`displayName`/`default`：命令、参数、环境变量与
/// Provider 凭据都不属于对端可见面）。
fn wire_agent(profile: &acp_core::model::AgentProfile) -> Option<AgentCatalogEntry> {
    Some(AgentCatalogEntry {
        agent_id: NonEmptyText::<128>::parse(profile.id().as_str()).ok()?,
        display_name: NonEmptyText::<128>::parse(profile.display_name()).ok()?,
        default: profile.is_default(),
    })
}

/// core 的会话状态 → wire 词表（两侧取值一一对应，逐条显式映射）。
fn wire_state(state: CoreSessionState) -> WireSessionState {
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
