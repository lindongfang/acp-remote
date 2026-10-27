//! Node Link 的 catalog 投影与 `catalog.subscribe` 处理（`design.md` D4/D7、`NODE_LINK_PROTOCOL.md`
//! §12.3）。
//!
//! 两条边界：
//!
//! - **实时投影，不做缓存副本**：每次 `catalog.subscribe` 都按当次持久化记录重新投影（core 的
//!   `node_link_catalog_view`），因此撤销不需要等任何推送就立即生效；
//! - **没有 Export/授权集合过滤**：已配对且未撤销的节点看到同一份 Agent/目录/会话清单。
//!
//! 不读时钟、不查 SQLite、不调用平级 adapter：全部持久事实经 `core::use_cases` 取得。

use std::str::FromStr as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use acp_core::use_cases::{NodeLinkCatalogSession, NodeLinkCatalogView};
use node_link_protocol::catalog::{
    AgentEntryList, CatalogSession, CatalogSessionList, CatalogSnapshot, CatalogSubscribe,
    DirectoryEntryList,
};
use node_link_protocol::common::{
    AgentEntry, DecimalString, DirectoryEntry, DirectoryId, NonEmptyText, Nullable, OpaqueRef,
    SessionState, Timestamp, Uuid,
};
use node_link_protocol::envelope::{Envelope, MessageType};
use node_link_protocol::error::ErrorCode;
use serde::de::DeserializeOwned;
use tracing::{debug, warn};

use crate::node_link::conn::{ConnectionHandle, MessageRoute, RouteOutcome, SendFault};
use crate::node_link::link_error;

/// Catalog membership also changes through session lifecycle events, which do not append audit
/// rows. Keep their revision bump process-local while `max()` still preserves the persisted audit
/// watermark across restarts.
static CATALOG_REVISION_FLOOR: AtomicU64 = AtomicU64::new(0);

/// Serialize catalog reads with revision advances so a read that started before a deletion can
/// never publish its old membership under the post-deletion revision.
static CATALOG_PUBLICATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) async fn publication_guard() -> tokio::sync::MutexGuard<'static, ()> {
    CATALOG_PUBLICATION.lock().await
}

/// The event's global sequence prevents the synthetic revision from falling behind the event log;
/// the extra increment guarantees consecutive catalog-changing events cannot reuse a revision even
/// when the persisted audit watermark has not moved.
pub(super) fn advance_catalog_revision(global_sequence: u64) {
    let _ = CATALOG_REVISION_FLOOR.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        Some(current.max(global_sequence).saturating_add(1))
    });
}

fn projection_revision(persisted: u64) -> u64 {
    CATALOG_REVISION_FLOOR
        .fetch_max(persisted, Ordering::AcqRel)
        .max(persisted)
}

/// `catalog.subscribe` 的处理件（无连接级状态：投影每次现算）。
pub struct CatalogRoute {
    core: Arc<acp_core::use_cases::UseCases>,
}

impl CatalogRoute {
    /// 装配。
    pub fn new(core: Arc<acp_core::use_cases::UseCases>) -> Self {
        Self { core }
    }
}

#[async_trait::async_trait]
impl MessageRoute for CatalogRoute {
    async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome {
        if message.message_type() != MessageType::CatalogSubscribe {
            return RouteOutcome::Unclaimed;
        }
        let Some(request) = decode::<CatalogSubscribe>(message) else {
            return self
                .reject(session, ErrorCode::ProtocolSchemaInvalid, message)
                .await;
        };
        debug!(
            event = "node_link.catalog_subscribe",
            access_node_id = session.node_id().as_str(),
            known_revision = request.known_revision.as_ref().map(|value| value.as_str()),
            "the access node subscribed to the catalog"
        );
        let _publication = publication_guard().await;
        let view = match self.core.node_link_catalog_view(session.node_id()).await {
            Ok(view) => view,
            Err(error) => {
                warn!(
                    event = "node_link.catalog_view_failed",
                    access_node_id = session.node_id().as_str(),
                    error = ?error,
                    "the catalog projection source could not be read"
                );
                return self
                    .fail(session, ErrorCode::InternalUnavailable, message)
                    .await;
            }
        };
        match project(&view) {
            Ok(snapshot) => {
                if let Err(fault) = session.send(MessageType::CatalogSnapshot, &snapshot) {
                    self.report_send_fault(session, fault);
                }
                RouteOutcome::Claimed
            }
            Err(fault) => {
                warn!(
                    event = "node_link.catalog_projection_failed",
                    access_node_id = session.node_id().as_str(),
                    fault = fault.as_str(),
                    "the catalog projection cannot be published as-is"
                );
                self.fail(session, ErrorCode::InternalUnavailable, message)
                    .await
            }
        }
    }
}

impl CatalogRoute {
    fn report_send_fault(&self, session: &ConnectionHandle, fault: SendFault) {
        match fault {
            SendFault::HighWater | SendFault::Closed => debug!(
                event = "node_link.catalog_not_delivered",
                access_node_id = session.node_id().as_str(),
                fault = ?fault,
                "the catalog snapshot could not be queued"
            ),
            SendFault::Encoding => warn!(
                event = "node_link.catalog_encoding_failed",
                access_node_id = session.node_id().as_str(),
                "the catalog snapshot could not be encoded"
            ),
        }
    }

    async fn reject(
        &self,
        session: &ConnectionHandle,
        code: ErrorCode,
        message: &Envelope,
    ) -> RouteOutcome {
        self.send_error(
            session,
            code,
            "the catalog request does not match the v1 schema",
            message,
        );
        RouteOutcome::Claimed
    }

    async fn fail(
        &self,
        session: &ConnectionHandle,
        code: ErrorCode,
        message: &Envelope,
    ) -> RouteOutcome {
        self.send_error(
            session,
            code,
            "the owner node cannot serve the catalog",
            message,
        );
        RouteOutcome::Claimed
    }

    fn send_error(
        &self,
        session: &ConnectionHandle,
        code: ErrorCode,
        message: &'static str,
        envelope: &Envelope,
    ) {
        let Some(body) = link_error(code, message, Some(envelope.message_id())) else {
            warn!(
                event = "node_link.error_body_failed",
                code = code.as_str(),
                "the link.error body cannot be assembled"
            );
            return;
        };
        debug!(
            event = "node_link.catalog_rejected",
            code = code.as_str(),
            "the catalog request was rejected without closing the connection"
        );
        let _ = session.send(MessageType::LinkError, &body);
    }
}

/// 投影失败的原因（只用于结构化日志；对端一律看到 `internal.unavailable`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectionFault {
    /// 字段超出 wire 的取值域，或列表超过 schema 的 `maxItems`。
    Field,
}

impl ProjectionFault {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Field => "field_out_of_range",
        }
    }
}

/// 把 core 的持久事实投影成 wire 快照（Agent / 目录 / 会话三类，均不含正文与路径）。
pub(super) fn project(view: &NodeLinkCatalogView) -> Result<CatalogSnapshot, ProjectionFault> {
    let revision = DecimalString::parse(&projection_revision(view.revision).to_string())
        .map_err(|_| ProjectionFault::Field)?;
    let agents = view
        .agents
        .iter()
        .map(|descriptor| {
            let agent_id = descriptor.agent.agent_id();
            Ok(AgentEntry {
                agent_id: node_link_protocol::common::AgentId::parse(agent_id.as_str())
                    .map_err(|_| ProjectionFault::Field)?,
                name: NonEmptyText::<128>::parse(descriptor.agent.name())
                    .map_err(|_| ProjectionFault::Field)?,
                capabilities_ref: OpaqueRef::parse(agent_id.as_str())
                    .map_err(|_| ProjectionFault::Field)?,
            })
        })
        .collect::<Result<Vec<_>, ProjectionFault>>()?;
    let directories = view
        .directories
        .iter()
        .map(|record| {
            Ok(DirectoryEntry {
                directory_id: DirectoryId::parse(record.directory_id().as_str())
                    .map_err(|_| ProjectionFault::Field)?,
            })
        })
        .collect::<Result<Vec<_>, ProjectionFault>>()?;
    let sessions = view
        .sessions
        .iter()
        .map(project_session)
        .collect::<Result<Vec<_>, ProjectionFault>>()?;
    Ok(CatalogSnapshot {
        revision,
        agents: AgentEntryList::new(agents).map_err(|_| ProjectionFault::Field)?,
        directories: DirectoryEntryList::new(directories).map_err(|_| ProjectionFault::Field)?,
        sessions: CatalogSessionList::new(sessions).map_err(|_| ProjectionFault::Field)?,
    })
}

fn project_session(entry: &NodeLinkCatalogSession) -> Result<CatalogSession, ProjectionFault> {
    let summary = &entry.session;
    Ok(CatalogSession {
        session_id: Uuid::parse(summary.session_id().as_str())
            .map_err(|_| ProjectionFault::Field)?,
        origin_epoch: Uuid::parse(entry.origin_epoch.as_str())
            .map_err(|_| ProjectionFault::Field)?,
        agent_id: node_link_protocol::common::AgentId::parse(summary.agent().agent_id().as_str())
            .map_err(|_| ProjectionFault::Field)?,
        state: SessionState::from_str(summary.state().as_str())
            .map_err(|_| ProjectionFault::Field)?,
        version: DecimalString::parse(&summary.version().get().to_string())
            .map_err(|_| ProjectionFault::Field)?,
        created_at: Timestamp::parse(summary.created_at().as_str())
            .map_err(|_| ProjectionFault::Field)?,
        workspace_directory_id: match summary.workspace() {
            Some(workspace) => Nullable::value(
                DirectoryId::parse(workspace.directory_id().as_str())
                    .map_err(|_| ProjectionFault::Field)?,
            ),
            None => Nullable::null(),
        },
    })
}

/// 解码消息 body（`Envelope` 的 body 是原始字节；失败按 schema 错误处理）。
fn decode<T: DeserializeOwned>(message: &Envelope) -> Option<T> {
    serde_json::from_str(message.body().get()).ok()
}
