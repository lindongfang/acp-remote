//! catalog 快照到 imported 索引的映射（`NODE_LINK_PROTOCOL.md` §12.3、`CORE_PORTS_AND_STORAGE.md` §7.4）。
//!
//! 这里只做**无正文**投影：catalog 不含消息、思考、附件或 ACP raw，因此它写出的
//! `imported_session` 行也只有身份、Agent、状态、版本、创建时间与 origin epoch。cursor/attachment
//! 不是 catalog 的事实，由调用方从既有行保留。
//!
//! `origin_epoch` **是** catalog 的事实（`catalog.session.originEpoch`）：没有它，imported 会话在
//! 第一条 `resource.event` 到达之前无法投影成带 `origin.kind = "remote"` 的摘要，Access 首页与
//! `session.list` 会把它当成不存在。Owner 侧每次目录投影都从 `owned_session.origin_epoch` 现读，
//! 因此这里不保留旧值——epoch 不一致意味着来源会话已被重建，旧的 cursor 本来也不可续读。
//!
//! 一个实现细节：catalog 的 `agentId` 是 Owner 的值；Agent 的展示名来自同一快照的
//! `catalog.snapshot.agents`。两者都按原值保留，本层不把 Owner 的目录/Provider 名称改写成 Access
//! 的本地概念。

use std::collections::BTreeMap;

use acp_core::model::{
    AgentId, AgentRef, NodeId, OriginEpoch, RemoteSessionRef, SessionId,
    SessionState as CoreSessionState, Timestamp as CoreTimestamp, Version,
};
use acp_core::ports::ImportedSessionRecord;
use node_link_protocol::catalog::{CatalogSession, CatalogSnapshot};
use node_link_protocol::common::{AgentId as WireAgentId, SessionState as WireSessionState};

/// 把 catalog 的会话条目投影成 imported 索引行。
///
/// `existing` 是同一会话的既有行：`origin_epoch`/`last_origin_sequence`/`acked`/`attachment` 不是
/// catalog 的事实，必须原样保留（`upsert_session` 会整体覆盖这些列）。
pub fn imported_record(
    owner: &NodeId,
    session: &CatalogSession,
    agent_name: Option<&str>,
    existing: Option<&ImportedSessionRecord>,
    at: &CoreTimestamp,
) -> Option<ImportedSessionRecord> {
    let session_id = SessionId::new(session.session_id.as_str()).ok()?;
    let reference = RemoteSessionRef::new(owner.clone(), session_id);
    let agent_id = AgentId::new(session.agent_id.as_str()).ok()?;
    let name = agent_name
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| session.agent_id.as_str());
    let agent = AgentRef::try_new(agent_id, name).ok()?;
    let state = core_state(session.state);
    let origin_epoch = OriginEpoch::new(session.origin_epoch.as_str()).ok()?;
    let version = session
        .version
        .as_str()
        .parse::<u64>()
        .ok()
        .map(Version::new);
    let created_at = CoreTimestamp::new(session.created_at.as_str()).ok();
    Some(ImportedSessionRecord {
        session: reference,
        origin_epoch: Some(origin_epoch),
        title: existing.and_then(|row| row.title.clone()),
        agent: Some(agent),
        state: Some(state),
        version,
        created_at,
        last_origin_sequence: existing.and_then(|row| row.last_origin_sequence),
        acked: existing.and_then(|row| row.acked.clone()),
        attachment: existing.and_then(|row| row.attachment.clone()),
        updated_at: at.clone(),
    })
}

/// 会话状态的封闭词表映射（wire 与 core 的取值集合逐条相同）。
pub fn core_state(state: WireSessionState) -> CoreSessionState {
    match state {
        WireSessionState::Idle => CoreSessionState::Idle,
        WireSessionState::Queued => CoreSessionState::Queued,
        WireSessionState::Running => CoreSessionState::Running,
        WireSessionState::WaitingInput => CoreSessionState::WaitingInput,
        WireSessionState::WaitingPermission => CoreSessionState::WaitingPermission,
        WireSessionState::Failed => CoreSessionState::Failed,
        WireSessionState::Closed => CoreSessionState::Closed,
    }
}

/// 一次快照里的 Agent 展示名表（`agentId → name`）。
pub fn agent_names(snapshot: &CatalogSnapshot) -> BTreeMap<String, String> {
    snapshot
        .agents
        .as_slice()
        .iter()
        .map(|agent| {
            (
                agent.agent_id.as_str().to_owned(),
                agent.name.as_str().to_owned(),
            )
        })
        .collect()
}

/// 把 wire 的 Agent 标识转成 core 值对象（`session.create` 的 payload 构造也用它）。
pub fn core_agent_id(agent: &WireAgentId) -> Option<AgentId> {
    AgentId::new(agent.as_str()).ok()
}
