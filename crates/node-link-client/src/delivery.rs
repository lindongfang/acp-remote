//! imported 事件的唯一写入路径：**先提交无正文收据，再发布，最后 ACK**。
//!
//! 顺序与权威（`CORE_PORTS_AND_STORAGE.md` §6.2/§7.4、`NODE_LINK_PROTOCOL.md` §6）：
//!
//! 1. 按 wire 的 `payloadDigest` 复算 ACPR-CJ1 摘要；不符即拒绝提交（不把被改写的正文写进本地索引，
//!    也不 ACK）；
//! 2. `Broker::deliver_imported` 组装 [`DeliveryReceipt`](acp_core::ports::DeliveryReceipt) 并调用
//!    `RemoteDeliveryStore::commit_receipt`；成功后发布 `CommittedDelivery::Imported`；
//! 3. 本模块再把该 origin cursor 持久化为累计 ACK；只有持久化成功才让调用方发 `resource.ack`。
//!
//! 正文（view/ACP raw）只在调用栈上存在：`deliver_imported` 把它们交给广播，本模块不写任何文件、
//! 不把正文放进错误/日志，也不在内存里保留队列（重放靠 Owner + persisted cursor）。
//!
//! `catalog` 与 `session_meta` 只更新 `imported_session` 的无正文列；既有行的 cursor/attachment 不是
//! catalog 的事实，因此先读再保留。

use std::collections::HashMap;
use std::sync::Arc;

use acp_core::broker::Broker;
use acp_core::model::{
    AcpRaw, Digest, EventId, EventPayload, EventType, NodeId, OriginCursor as CoreOriginCursor,
    OriginEpoch, PortError, RawUnavailableReason, RemoteSessionRef, Sequence, SessionId,
    Timestamp as CoreTimestamp, ViewJson,
};
use acp_core::ports::{Clock, ImportedSessionQuery, ImportedSessionRecord, RemoteDeliveryStore};
use node_link_protocol::catalog::CatalogSnapshot;
use node_link_protocol::common::{
    OriginCursor, RemoteSessionRef as WireRemoteSessionRef, SessionMeta,
};
use node_link_protocol::resource::Event;
use sha2::{Digest as _, Sha256};

use crate::catalog;
use crate::error::ClientError;
use crate::link::ImportedSink;

/// 交付桥：无正文收据 + ACK。
pub struct DeliveryBridge {
    broker: Arc<Broker>,
    deliveries: Arc<dyn RemoteDeliveryStore>,
    clock: Arc<dyn Clock>,
}

impl DeliveryBridge {
    /// 装配。
    pub fn new(
        broker: Arc<Broker>,
        deliveries: Arc<dyn RemoteDeliveryStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            broker,
            deliveries,
            clock,
        }
    }
}

#[async_trait::async_trait]
impl ImportedSink for DeliveryBridge {
    async fn event(
        &self,
        owner: &NodeId,
        event: &Event,
    ) -> Result<Option<OriginCursor>, PortError> {
        if event.session_ref.owner_node_id.as_str() != owner.as_str() {
            // 跨 Owner 的 sessionRef 是协议违规：不提交、不 ACK。
            return Err(PortError::InvalidRequest(
                "the imported event belongs to a different owner node",
            ));
        }
        // ① 摘要：前像必须是**收到的 payload 对象**（ACPR-CJ1 规范化后逐字节一致）。
        let expected = payload_digest(&event.payload).ok_or(PortError::InvalidRequest(
            "the imported payload cannot be canonicalized",
        ))?;
        if expected != *event.payload_digest.as_bytes() {
            return Err(PortError::InvalidRequest(
                "the imported payload digest does not match",
            ));
        }

        let session = RemoteSessionRef::new(
            owner.clone(),
            SessionId::new(event.session_ref.session_id.as_str())?,
        );
        let origin_epoch = OriginEpoch::new(event.origin_epoch.as_str())?;
        let event_id = EventId::new(event.origin_event_id.as_str())?;
        let origin_sequence = Sequence::new(
            event
                .origin_sequence
                .as_str()
                .parse::<u64>()
                .map_err(|_| acp_core::model::InvalidValue::SequenceRange)?,
        )?;
        let event_type = EventType::new(event.event_type.as_str())?;
        let digest = Digest::new(&base64url(event.payload_digest.as_bytes()))?;
        let payload = core_payload(&event.payload);

        // ② 收据提交 + 发布（core 的既有顺序，本层不重排）。
        self.broker
            .deliver_imported(
                session.clone(),
                acp_core::model::OriginEventRef::new(owner.clone(), origin_epoch.clone(), event_id),
                origin_sequence,
                event_type,
                digest,
                payload,
            )
            .await?;

        // ③ 累计 ACK：只有持久化成功才返回可确认的游标。
        let cursor = CoreOriginCursor::new(origin_epoch, origin_sequence);
        let outcome = self
            .deliveries
            .ack(&session, cursor.clone(), self.clock.now())
            .await?;
        if outcome.applied {
            Ok(Some(OriginCursor {
                origin_epoch: node_link_protocol::common::Uuid::parse(cursor.origin_epoch.as_str())
                    .map_err(|_| {
                        PortError::InvalidRequest("the origin epoch is not a canonical uuid")
                    })?,
                origin_sequence: node_link_protocol::common::DecimalString::parse(
                    &cursor.origin_sequence.get().to_string(),
                )
                .map_err(|_| PortError::InvalidRequest("the origin sequence cannot be encoded"))?,
            }))
        } else {
            // 回退请求（或 epoch 变化）：不确认任何位置，交给重连后的重放。
            Ok(None)
        }
    }

    async fn catalog(&self, owner: &NodeId, snapshot: &CatalogSnapshot) -> Result<(), PortError> {
        let at = self.clock.now();
        let names = catalog::agent_names(snapshot);
        let existing = self
            .deliveries
            .list_sessions(ImportedSessionQuery::default())
            .await?;
        let index: HashMap<String, &ImportedSessionRecord> = existing
            .iter()
            .map(|record| (record.session.session_id.as_str().to_owned(), record))
            .collect();
        for session in snapshot.sessions.as_slice() {
            let name = names.get(session.agent_id.as_str()).map(String::as_str);
            let previous = index.get(session.session_id.as_str()).copied();
            let Some(record) = catalog::imported_record(owner, session, name, previous, &at) else {
                tracing::warn!(
                    event = "node_link_client.catalog_item_invalid",
                    owner = %owner,
                    "a catalog session entry cannot be mapped to the imported index"
                );
                continue;
            };
            self.deliveries.upsert_session(record, at.clone()).await?;
        }
        Ok(())
    }

    async fn session_meta(
        &self,
        owner: &NodeId,
        session: &WireRemoteSessionRef,
        meta: &SessionMeta,
    ) -> Result<(), PortError> {
        let session_id = SessionId::new(session.session_id.as_str())?;
        let reference = RemoteSessionRef::new(owner.clone(), session_id);
        let existing = self
            .deliveries
            .list_sessions(ImportedSessionQuery {
                only: Some(vec![reference.clone()]),
                limit: Some(1),
            })
            .await?;
        let Some(mut record) = existing.into_iter().next() else {
            // 快照可以早于 catalog 抵达：没有索引行时不凭空造一条（会缺 Agent/创建时间）。
            return Ok(());
        };
        record.state = Some(catalog::core_state(meta.state));
        record.version = meta
            .version
            .as_str()
            .parse::<u64>()
            .ok()
            .map(acp_core::model::Version::new);
        record.updated_at = self.clock.now();
        self.deliveries
            .upsert_session(record, self.clock.now())
            .await
    }
}

/// 本 Owner 下需要恢复订阅的会话（含持久化 ACK 作为续读点）。
pub async fn subscriptions(
    deliveries: &dyn RemoteDeliveryStore,
    owner: &NodeId,
) -> Result<Vec<(RemoteSessionRef, Option<OriginCursor>)>, PortError> {
    let rows = deliveries
        .list_sessions(ImportedSessionQuery::default())
        .await?;
    let mut out = Vec::new();
    for row in rows {
        if row.session.owner_node_id.as_str() != owner.as_str() {
            continue;
        }
        let cursor = match &row.acked {
            Some(cursor) => Some(OriginCursor {
                origin_epoch: node_link_protocol::common::Uuid::parse(cursor.origin_epoch.as_str())
                    .map_err(|_| {
                        PortError::InvalidRequest("a stored origin epoch is not a canonical uuid")
                    })?,
                origin_sequence: node_link_protocol::common::DecimalString::parse(
                    &cursor.origin_sequence.get().to_string(),
                )
                .map_err(|_| PortError::InvalidRequest("a stored ack cannot be encoded"))?,
            }),
            None => None,
        };
        out.push((row.session, cursor));
    }
    Ok(out)
}

/// `payloadDigest` = `base64url(SHA-256(ACPR-CJ1(payload)))`（与 Owner 侧的同一前像）。
fn payload_digest(payload: &node_link_protocol::resource::EventPayload) -> Option<[u8; 32]> {
    let text = serde_json::to_string(payload).ok()?;
    let canonical = acpr_wire::cj1::canonicalize(&text).ok()?;
    Some(Sha256::digest(canonical.as_bytes()).into())
}

/// wire payload → core payload。
///
/// 只有 `view` 能构成 core 的 [`EventPayload`]（它的 `view` 是必填的开放对象）。仅带 ACP raw 的
/// 未登记事件仍然提交无正文收据，但**不**把 ACP raw 伪造成 view：调用方拿到 `None`，客户端只看到
/// 「有这条事件、没有可展示的结构化内容」。
fn core_payload(payload: &node_link_protocol::resource::EventPayload) -> Option<EventPayload> {
    let view = payload.view()?;
    let view = ViewJson::new(view.get()).ok()?;
    let acp = payload.acp().and_then(core_acp);
    Some(EventPayload::new(view, acp))
}

fn core_acp(acp: &acpr_wire::RawAcp) -> Option<AcpRaw> {
    match acp {
        acpr_wire::RawAcp::Complete {
            raw_json,
            byte_length,
            sha256,
        } => {
            // `rawJson` 是 JSON **字符串**（内容为 ACP 原文档文本），先解出字符串再交给 core 校验。
            let document: String = serde_json::from_str(raw_json.get()).ok()?;
            if document.len() as u64 != byte_length.as_str().parse::<u64>().ok()? {
                return None;
            }
            let digest: [u8; 32] = *sha256.as_bytes();
            let digest = Digest::new(&base64url(&digest)).ok()?;
            AcpRaw::available("application/json", &document, digest).ok()
        }
        acpr_wire::RawAcp::Unavailable {
            reason,
            byte_length,
            sha256,
        } => {
            let reason = match reason {
                acpr_wire::RawAcpUnavailableReason::SizeLimit => RawUnavailableReason::SizeLimit,
                acpr_wire::RawAcpUnavailableReason::RetentionExpired => {
                    RawUnavailableReason::RetentionExpired
                }
                acpr_wire::RawAcpUnavailableReason::StorageFailure => {
                    RawUnavailableReason::StorageFailure
                }
            };
            let digest = sha256
                .as_ref()
                .map(|sha256| Digest::new(&base64url(sha256.as_bytes())))
                .transpose()
                .ok()?;
            Some(AcpRaw::unavailable(
                reason,
                byte_length.as_str().parse::<u64>().ok()?,
                digest,
            ))
        }
    }
}

fn base64url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 把 core 游标编码成 wire 形状（供 supervisor 在重连时构造 `resource.subscribe`）。
pub fn wire_cursor(cursor: &CoreOriginCursor) -> Result<OriginCursor, ClientError> {
    Ok(OriginCursor {
        origin_epoch: node_link_protocol::common::Uuid::parse(cursor.origin_epoch.as_str())
            .map_err(|_| ClientError::Local("a stored origin epoch is not a canonical uuid"))?,
        origin_sequence: node_link_protocol::common::DecimalString::parse(
            &cursor.origin_sequence.get().to_string(),
        )
        .map_err(|_| ClientError::Local("a stored cursor cannot be encoded"))?,
    })
}

/// 读取某会话的持久化 ACK（supervisor 在重连时用）。
pub async fn load_cursor(
    deliveries: &dyn RemoteDeliveryStore,
    session: &RemoteSessionRef,
) -> Result<Option<CoreOriginCursor>, PortError> {
    deliveries.load_ack(session).await
}

/// 时间戳转换（`deliver_imported` 的 receipt 用 core 的时钟，这里只暴露给 supervisor 的诊断）。
pub fn now(clock: &dyn Clock) -> CoreTimestamp {
    clock.now()
}
