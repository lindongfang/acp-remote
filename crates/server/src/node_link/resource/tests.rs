//! `server::node_link::resource` 的行为用例（`[PV3]` 的 WP5 部分）。
//!
//! 前半是纯函数的契约用例（摘要、payload 映射、cursor 往返），后半用真实 `ConnectionHandle` 的出站队列
//! 驱动 [`ResourceRoute`]：不看内部状态，只看对端真的收到的那几帧。

use super::*;
use acp_core::model::{AcpRaw, Digest, EventPayload as CoreEventPayload, ViewJson};

fn digest_of(text: &str) -> Digest {
    use base64::Engine as _;
    let bytes = Sha256::digest(text.as_bytes());
    Digest::new(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)).expect("摘要文本")
}

#[test]
fn payload_digest_is_acpr_cj1_over_the_payload_object() {
    let payload = WireEventPayload::new(
        Some(RawObject::parse(r#"{"b":1,"a":{"d":[2,1],"c":null}}"#).expect("view")),
        None,
    )
    .expect("payload");
    let digest = payload_digest(&payload).expect("可算摘要");
    // 手工复算：ACPR-CJ1 只规范化**成员顺序与转义**，不改变数组顺序。
    let canonical =
        acpr_wire::cj1::canonicalize(&serde_json::to_string(&payload).expect("序列化 payload"))
            .expect("规范化");
    let expected = {
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(canonical.as_bytes()))
    };
    assert_eq!(base64url(digest.as_bytes()), expected);
    // 同值不同书写顺序 → 同一摘要（ACPR-CJ1 的语义）。
    let reordered = WireEventPayload::new(
        Some(RawObject::parse(r#"{"a":{"c":null,"d":[2,1]},"b":1}"#).expect("view")),
        None,
    )
    .expect("payload");
    assert_eq!(payload_digest(&reordered).unwrap(), digest);
}

#[test]
fn snapshot_digest_is_the_sha256_of_the_ordered_chunk_digests() {
    let first = Sha256::digest(b"frame-one");
    let second = Sha256::digest(b"frame-two");
    let digest = snapshot_digest(&[first, second]).expect("摘要");
    let mut hasher = Sha256::new();
    hasher.update(first);
    hasher.update(second);
    assert_eq!(
        base64url(digest.as_bytes()),
        base64url(&hasher.finalize()),
        "连接顺序必须被摘要覆盖"
    );
    // 顺序不同 → 摘要不同：批次内顺序是协议事实，不能被摘要忽略。
    let reordered = snapshot_digest(&[Sha256::digest(b"frame-two"), Sha256::digest(b"frame-one")])
        .expect("摘要");
    assert_ne!(
        base64url(digest.as_bytes()),
        base64url(reordered.as_bytes())
    );
}

#[test]
fn inline_acp_raw_keeps_the_document_bytes_and_the_media_type() {
    let payload = CoreEventPayload::new(
        ViewJson::new(r#"{"kind":"agent.message"}"#).expect("view"),
        Some(
            AcpRaw::available(
                "application/json",
                r#"{"jsonrpc":"2.0","id":1}"#,
                digest_of(r#"{"jsonrpc":"2.0","id":1}"#),
            )
            .expect("acp raw"),
        ),
    );
    let wire = wire_payload(&payload).expect("可映射");
    let text = serde_json::to_string(&wire).expect("序列化");
    let value: serde_json::Value = serde_json::from_str(&text).expect("解析");
    assert_eq!(value["acp"]["mediaType"], "application/json");
    assert_eq!(
        value["acp"]["rawJson"], r#"{"jsonrpc":"2.0","id":1}"#,
        "rawJson 是承载原文档的 JSON 字符串（逐字节保真）"
    );
    assert_eq!(value["view"]["kind"], "agent.message");
}

#[test]
fn acp_raw_over_the_inline_limit_is_downgraded_not_truncated() {
    let document = format!(r#"{{"pad":"{}"}}"#, "x".repeat(MAX_INLINE_ACP_BYTES));
    let payload = CoreEventPayload::new(
        ViewJson::new(r#"{"kind":"agent.message"}"#).expect("view"),
        Some(
            AcpRaw::available("application/json", &document, digest_of(&document))
                .expect("acp raw"),
        ),
    );
    let wire = wire_payload(&payload).expect("可映射");
    let value: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&wire).expect("序列化")).expect("解析");
    assert_eq!(value["acp"]["rawUnavailable"]["reason"], "size_limit");
    assert_eq!(
        value["acp"]["rawUnavailable"]["byteLength"],
        document.len().to_string(),
        "byteLength 是 decimalString，取降级前的原始字节数"
    );
    assert_eq!(value["acp"]["mediaType"], "application/json");
    assert!(
        value["acp"].get("rawJson").is_none(),
        "降级后不得携带被截断的原文"
    );
    // `view` 仍逐字节保留：降级只影响 `acp`。
    assert_eq!(value["view"]["kind"], "agent.message");
}

#[test]
fn core_and_wire_origin_cursors_round_trip() {
    let wire = node_link_protocol::common::OriginCursor {
        origin_epoch: Uuid::parse("018f6f89-8a23-7a10-a0d3-f92e6a31d952").expect("epoch"),
        origin_sequence: DecimalString::parse("41").expect("sequence"),
    };
    let core = core_cursor(&wire).expect("可映射");
    assert_eq!(core.origin_epoch.as_str(), wire.origin_epoch.as_str());
    assert_eq!(core.origin_sequence.get(), 41);
}

// ---------------------------------------------------------------------------------------------
// 路由层用例：真实 `ConnectionHandle` 的出站队列 + 端口替身世界
// ---------------------------------------------------------------------------------------------

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{
    Actor, AgentId, AgentRef, CachePolicy, CommandRecord, CommittedDelivery, EventId, EventPayload,
    EventType, ExportRecord, ExportTemplate, GlobalCursor, GrantSet, InteractionId, NodeId,
    NodeKind, NodeRecord, NodeState, OriginCursor, OriginEpoch, OwnedSessionRef,
    PendingInteraction, RequestId, ResourceOrigin, Sequence, ServerEpoch, Session, SessionSnapshot,
    SessionState, SessionSummary, TemplateId, Timestamp, Version,
    WorkspaceAlias as CoreWorkspaceAlias, WorkspaceAliasEntry as CoreWorkspaceAliasEntry,
};
use acp_core::ports::{
    CommitOutcome, HistoryPage, HistoryQuery, NodeLinkSlice, OwnedCommit, OwnedEventRecord,
    PendingInteractionOrigin, PruneReport, ReadView, ReplayBatch, RetentionPolicy, SessionQuery,
    SessionStore, StoreHealth,
};
use node_link_protocol::envelope::ConnectionFields;
use serde_json::{Value, json};

use crate::local_admin::test_support::{FakeIds, TEST_SERVER_EPOCH, TestWorld, test_public_key};
use crate::node_link::conn::registry::Outbound;
use crate::node_link::conn::{
    ConnectionHandle, ConnectionRegistry, MessageRoute, NodeLinkConfig, RouteOutcome, SessionLimits,
};

/// 测试扮演的 Access Node（与信任行同源）。
const ACCESS_NODE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
/// 会话与 Export（Export 覆盖 `codex`，见 [`Fixture::new`]）。
const SESSION: &str = "3ae1c07c-9242-46e9-a9d2-4ec58c130f4a";
const EXPORT: &str = "11111111-1111-4111-8111-111111111111";
/// 会话的 origin epoch（快照/重放/ACK 三处共用）。
const ORIGIN_EPOCH: &str = "018f6f89-8a23-7a10-a0d3-f92e6a31d952";
/// 第一条已持久化事件的 id。
const EVENT: &str = "4ae1c07c-9242-46e9-a9d2-4ec58c130f4b";
const CONNECTION: &str = "5ae1c07c-9242-46e9-a9d2-4ec58c130f4c";
/// 读一帧的上限（用例失败要快速失败）。
const IO_TIMEOUT: Duration = Duration::from_secs(5);

fn ts(text: &str) -> Timestamp {
    Timestamp::new(text).expect("固定时间戳")
}

fn session_id() -> SessionId {
    SessionId::new(SESSION).expect("session id")
}

fn export_id() -> ExportId {
    ExportId::new(EXPORT).expect("export id")
}

fn agent_ref() -> AgentRef {
    AgentRef::try_new(AgentId::new("codex").expect("agent id"), "Codex").expect("agent ref")
}

/// 会话聚合（归属校验读 `session.agent()`，因此 agent 必须与 Export 的清单一致）。
fn owned_session() -> Session {
    Session::try_new(
        session_id(),
        OwnedSessionRef::new(session_id()),
        Some("Session".to_owned()),
        agent_ref(),
        SessionState::Idle,
        ResourceOrigin::Local,
        None,
        Version::new(3),
        ts("2026-09-18T09:00:00.000Z"),
        ts("2026-09-18T09:12:00.000Z"),
        None,
    )
    .expect("会话聚合")
}

fn session_summary() -> SessionSummary {
    SessionSummary::try_new(
        session_id(),
        Some("Session".to_owned()),
        agent_ref(),
        SessionState::Idle,
        ResourceOrigin::Local,
        None,
        Version::new(3),
        ts("2026-09-18T09:00:00.000Z"),
        ts("2026-09-18T09:12:00.000Z"),
    )
    .expect("会话摘要")
}

fn origin_head(sequence: u64) -> OriginCursor {
    OriginCursor::new(
        OriginEpoch::new(ORIGIN_EPOCH).expect("epoch"),
        Sequence::new(sequence).expect("sequence"),
    )
}

/// 一条已持久化的会话事件（含正文；`view` 是库里 `payload_json` 的原样文本）。
fn stored_event(sequence: u64, event_type: &str, view: &str) -> OwnedEventRecord {
    OwnedEventRecord {
        event: CommittedEvent {
            id: EventId::new(EVENT).expect("event id"),
            event_type: EventType::new(event_type).expect("event type"),
            session: Some(session_id()),
            session_sequence: Some(Sequence::new(sequence).expect("sequence")),
            global_sequence: Sequence::new(sequence).expect("sequence"),
            origin_epoch: Some(OriginEpoch::new(ORIGIN_EPOCH).expect("epoch")),
            origin_sequence: Some(Sequence::new(sequence).expect("sequence")),
            created_at: ts("2026-09-18T09:10:00.000Z"),
        },
        payload: CoreEventPayload::new(ViewJson::new(view).expect("view"), None),
    }
}

fn pending_interaction(id: &str) -> PendingInteractionOrigin {
    PendingInteractionOrigin {
        interaction: PendingInteraction::try_new(
            InteractionId::new(id).expect("interaction id"),
            InteractionKind::Permission,
            session_id(),
            ts("2026-09-18T09:11:00.000Z"),
            Vec::new(),
        )
        .expect("未决交互"),
        origin_event: EventId::new(EVENT).expect("event id"),
    }
}

/// 端口替身：`head()` 与 Node Link 会话读面（其余方法一律 `unreachable!`，替身不比真实存储宽容）。
#[derive(Clone)]
struct SliceStore {
    head: GlobalCursor,
    slice: NodeLinkSlice,
}

#[async_trait::async_trait]
impl SessionStore for SliceStore {
    async fn commit(&self, _commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        unreachable!("WP5 路由用例不写存储")
    }

    async fn load(&self, session: &SessionId) -> Result<Option<SessionSnapshot>, PortError> {
        if session != &session_id() {
            return Ok(None);
        }
        Ok(Some(SessionSnapshot {
            session: owned_session(),
            origin_epoch: OriginEpoch::new(ORIGIN_EPOCH).expect("epoch"),
            head: self.head.clone(),
        }))
    }

    async fn list(&self, _query: SessionQuery) -> Result<Vec<SessionSummary>, PortError> {
        unreachable!("WP5 路由用例不列举会话")
    }

    async fn head(&self) -> Result<GlobalCursor, PortError> {
        Ok(self.head.clone())
    }

    async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError> {
        Ok(Box::new(self.clone()))
    }

    async fn find_request(
        &self,
        _request: &RequestId,
        _actor: &Actor,
    ) -> Result<Option<CommandRecord>, PortError> {
        unreachable!("WP5 路由用例不查命令")
    }

    async fn unsettled_commands(
        &self,
        _limit: ReplayLimit,
    ) -> Result<Vec<CommandRecord>, PortError> {
        unreachable!("WP5 路由用例不查命令")
    }

    async fn retention_window(
        &self,
        _session: &SessionId,
    ) -> Result<Option<(Sequence, Sequence)>, PortError> {
        unreachable!("WP5 路由用例不查保留窗口")
    }

    async fn prune(
        &self,
        _policy: RetentionPolicy,
        _at: Timestamp,
    ) -> Result<PruneReport, PortError> {
        unreachable!("WP5 路由用例不清扫")
    }

    async fn health(&self) -> Result<StoreHealth, PortError> {
        unreachable!("WP5 路由用例不查健康")
    }
}

#[async_trait::async_trait]
impl ReadView for SliceStore {
    async fn head(&self) -> Result<GlobalCursor, PortError> {
        Ok(self.head.clone())
    }

    async fn replay(
        &self,
        _after: Option<GlobalCursor>,
        _limit: ReplayLimit,
    ) -> Result<ReplayBatch, PortError> {
        unreachable!("WP5 路由用例不读 Sync 重放面")
    }

    async fn read_session(&self, _query: HistoryQuery) -> Result<HistoryPage, PortError> {
        unreachable!("WP5 路由用例不读历史面")
    }

    async fn event_payload(&self, _event: &EventId) -> Result<Option<EventPayload>, PortError> {
        unreachable!("WP5 路由用例只经会话绑定的读入口取正文")
    }

    /// 只按 `origin_sequence > after` 取行；`after` 的 epoch 解释权在用例层（与真实存储同口径）。
    async fn node_link_slice(
        &self,
        session: &SessionId,
        after: Option<OriginCursor>,
        _limit: ReplayLimit,
    ) -> Result<NodeLinkSlice, PortError> {
        if session != &session_id() {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        }
        let mut slice = self.slice.clone();
        if let Some(after) = after {
            slice
                .events
                .retain(|record| record.event.origin_sequence > Some(after.origin_sequence));
        }
        Ok(slice)
    }

    async fn session_event_payload(
        &self,
        session: &SessionId,
        event: &EventId,
    ) -> Result<Option<EventPayload>, PortError> {
        if session != &session_id() {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        }
        Ok(self
            .slice
            .events
            .iter()
            .find(|record| &record.event.id == event)
            .map(|record| record.payload.clone()))
    }
}
/// 端口替身世界 + 已注册的连接句柄 + 真实路由。
struct Fixture {
    world: TestWorld,
    handle: Arc<ConnectionHandle>,
    outbound: mpsc::Receiver<Outbound>,
    registry: Arc<ConnectionRegistry>,
}

impl Fixture {
    /// 一条事件 + 空的未决交互（默认素材）。
    async fn new() -> Self {
        Self::with_slice(NodeLinkSlice {
            summary: session_summary(),
            head: origin_head(1),
            pending_interactions: Vec::new(),
            events: vec![stored_event(
                1,
                "agent.message",
                r#"{"kind":"agent.message"}"#,
            )],
        })
        .await
    }

    async fn with_slice(slice: NodeLinkSlice) -> Self {
        let store = SliceStore {
            head: GlobalCursor::new(
                ServerEpoch::new(TEST_SERVER_EPOCH).expect("server epoch"),
                Sequence::new(1).expect("sequence"),
            ),
            slice,
        };
        let world = TestWorld::with_store(Arc::new(store));
        world.trust.seed_node(
            NodeRecord::try_new(
                NodeId::new(ACCESS_NODE).expect("node id"),
                "Office Access",
                NodeKind::Access,
                test_public_key().fingerprint(),
                GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
                NodeState::Paired,
                None,
                ts("2026-09-18T09:00:00.000Z"),
                None,
                None,
            )
            .expect("信任行"),
        );
        world.exports.seed_export(
            ExportRecord::try_new(
                export_id(),
                "Project Export",
                vec![AgentId::new("codex").expect("agent id")],
                vec![
                    CoreWorkspaceAliasEntry::try_new(
                        CoreWorkspaceAlias::new("project").expect("alias"),
                        "Project",
                    )
                    .expect("alias entry"),
                ],
                CoreWorkspaceAlias::new("project").expect("alias"),
                vec![
                    ExportTemplate::try_new(
                        TemplateId::new("template-1").expect("template id"),
                        "Template",
                        CoreWorkspaceAlias::new("project").expect("alias"),
                        Vec::new(),
                    )
                    .expect("template"),
                ],
                TemplateId::new("template-1").expect("template id"),
                GrantSet::try_from_iter(["grant.observe"]).expect("scopes"),
                CachePolicy::NoContentCache,
                ts("2026-09-18T09:00:00.000Z"),
                None,
            )
            .expect("Export 记录"),
        );
        let registry = ConnectionRegistry::new();
        let (handle, outbound) = ConnectionHandle::new(
            Uuid::parse(CONNECTION).expect("connection id"),
            NodeId::new(ACCESS_NODE).expect("node id"),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            SessionLimits::negotiate(&NodeLinkConfig::default()),
            Arc::new(FakeIds::default()),
        );
        registry.register(Arc::clone(&handle));
        Self {
            world,
            handle,
            outbound,
            registry,
        }
    }

    fn route(&self) -> ResourceRoute {
        ResourceRoute::new(Arc::clone(&self.world.core), Arc::clone(&self.registry))
    }

    /// 认证后消息的信封（连接字段按 §2.2 必需）。
    fn envelope(&self, message_type: MessageType, body: Value) -> Envelope {
        Envelope::new(
            message_type,
            Uuid::parse("6ae1c07c-9242-46e9-a9d2-4ec58c130f4d").expect("message id"),
            Some(ConnectionFields {
                connection_id: Uuid::parse(CONNECTION).expect("connection id"),
                connection_sequence: DecimalString::parse("1").expect("sequence"),
            }),
            serde_json::value::RawValue::from_string(body.to_string()).expect("body"),
        )
        .expect("信封")
    }

    /// 排空出站队列（「只看本轮新增的那几帧」）。
    fn drain(&mut self) -> Vec<Frame> {
        let mut frames = Vec::new();
        while let Ok(outbound) = self.outbound.try_recv() {
            frames.push(Frame::new(outbound.text));
        }
        frames
    }

    /// 等一帧（扇出路径要等后台任务）。
    async fn next_frame(&mut self) -> Frame {
        let outbound = tokio::time::timeout(IO_TIMEOUT, self.outbound.recv())
            .await
            .expect("必须收到一帧")
            .expect("队列未关闭");
        Frame::new(outbound.text)
    }

    /// 走完 attach（返回 `resource.attached` 的 body）。
    async fn attach(&mut self, route: &ResourceRoute) -> Value {
        let envelope = self.envelope(
            MessageType::ResourceAttach,
            json!({ "remoteSessionRef": remote_session_ref() }),
        );
        assert_eq!(
            route.route(&self.handle, &envelope).await,
            RouteOutcome::Claimed
        );
        let frames = self.drain();
        let attached = of_type(&frames, "resource.attached");
        assert_eq!(attached.len(), 1, "attach 必须回一帧 resource.attached");
        attached[0]["body"].clone()
    }
}

/// 一帧：原始文本 + 解析后的 JSON。
///
/// 两者必须同时给出：快照 digest 的前像是**实际发出的原始文本**（接收方不得重新序列化），而
/// `serde_json::Value` 的成员顺序不保证与原帧一致，不能拿它当摘要前像。
struct Frame {
    text: String,
    value: Value,
}

impl Frame {
    fn new(text: String) -> Self {
        let value = serde_json::from_str(text.as_str()).expect("帧是合法 JSON");
        Self { text, value }
    }
}

impl std::ops::Deref for Frame {
    type Target = Value;

    fn deref(&self) -> &Value {
        &self.value
    }
}

/// 只取类型等于 `expected` 的帧。
fn of_type<'a>(frames: &'a [Frame], expected: &str) -> Vec<&'a Frame> {
    frames
        .iter()
        .filter(|frame| frame["type"] == expected)
        .collect()
}

fn error_code(frame: &Value) -> &str {
    frame["body"]["code"].as_str().expect("错误码")
}

fn remote_session_ref() -> Value {
    json!({
        "ownerNodeId": ACCESS_NODE,
        "exportId": EXPORT,
        "sessionId": SESSION,
    })
}

/// [R53]/[R54]/[R55]：attach 签发新代际，重新 attach 覆盖旧代际，旧 frame 被按「代际过期」拒绝。
#[tokio::test]
async fn a_reissued_attachment_invalidates_the_previous_generation() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();

    let first = fixture.attach(&route).await;
    let first_id = first["attachmentId"]
        .as_str()
        .expect("attachment id")
        .to_owned();
    assert_eq!(first["attachmentGeneration"], "1");
    assert_eq!(first["sessionMeta"]["state"], "idle");
    assert_eq!(first["sessionMeta"]["version"], "3");

    let second = fixture.attach(&route).await;
    assert_eq!(
        second["attachmentGeneration"], "2",
        "同一连接内代际单调递增"
    );
    assert_ne!(
        second["attachmentId"].as_str().expect("attachment id"),
        first_id,
        "每次 attach 都是新的临时凭据"
    );

    // 旧代际的 subscribe：不是当前 attachment → attach_generation_stale（可恢复，不关连接）。
    let stale = fixture.envelope(
        MessageType::ResourceSubscribe,
        json!({
            "attachmentId": first_id,
            "attachmentGeneration": "1",
            "cursor": null,
        }),
    );
    assert_eq!(
        route.route(&fixture.handle, &stale).await,
        RouteOutcome::Claimed
    );
    let frames = fixture.drain();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["type"], "link.error");
    assert_eq!(
        error_code(&frames[0]),
        "nodelink.resource.attach_generation_stale"
    );
}

/// [R52]/[R53]：与节点 grants 不相交的 Export 即便存在也不可见 → `export.not_granted`。
#[tokio::test]
async fn an_export_disjoint_from_the_node_grants_is_not_granted() {
    let mut fixture = Fixture::new().await;
    // 同一条 Export 的 scopes 换成与节点 grants（`grant.observe`）不相交的值。
    fixture.world.exports.seed_export(
        ExportRecord::try_new(
            export_id(),
            "Project Export",
            vec![AgentId::new("codex").expect("agent id")],
            vec![
                CoreWorkspaceAliasEntry::try_new(
                    CoreWorkspaceAlias::new("project").expect("alias"),
                    "Project",
                )
                .expect("alias entry"),
            ],
            CoreWorkspaceAlias::new("project").expect("alias"),
            vec![
                ExportTemplate::try_new(
                    TemplateId::new("template-1").expect("template id"),
                    "Template",
                    CoreWorkspaceAlias::new("project").expect("alias"),
                    Vec::new(),
                )
                .expect("template"),
            ],
            TemplateId::new("template-1").expect("template id"),
            GrantSet::try_from_iter(["grant.remote-work"]).expect("scopes"),
            CachePolicy::NoContentCache,
            ts("2026-09-18T09:00:00.000Z"),
            None,
        )
        .expect("Export 记录"),
    );
    let route = fixture.route();
    let envelope = fixture.envelope(
        MessageType::ResourceAttach,
        json!({ "remoteSessionRef": remote_session_ref() }),
    );
    assert_eq!(
        route.route(&fixture.handle, &envelope).await,
        RouteOutcome::Claimed
    );
    let frames = fixture.drain();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["type"], "link.error");
    assert_eq!(error_code(&frames[0]), "nodelink.export.not_granted");
}

/// [R53]：Export 存在但会话不属于本机 → `export.not_found`（不泄露「会话存在与否」之外的差别）。
#[tokio::test]
async fn an_unknown_session_is_reported_as_export_not_found() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    let envelope = fixture.envelope(
        MessageType::ResourceAttach,
        json!({
            "remoteSessionRef": {
                "ownerNodeId": ACCESS_NODE,
                "exportId": EXPORT,
                "sessionId": "3ae1c07c-9242-46e9-a9d2-4ec58c130f9f",
            }
        }),
    );
    assert_eq!(
        route.route(&fixture.handle, &envelope).await,
        RouteOutcome::Claimed
    );
    let frames = fixture.drain();
    assert_eq!(frames.len(), 1);
    assert_eq!(error_code(&frames[0]), "nodelink.export.not_found");
}

/// [R56]/[R57]/[R58]：`cursor = null` 的快照只有元数据，且 `snapshotDigest` 能由收到的帧复算。
#[tokio::test]
async fn a_snapshot_carries_metadata_only_and_a_verifiable_digest() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    let attached = fixture.attach(&route).await;

    let subscribe = fixture.envelope(
        MessageType::ResourceSubscribe,
        json!({
            "attachmentId": attached["attachmentId"],
            "attachmentGeneration": attached["attachmentGeneration"],
            "cursor": null,
        }),
    );
    assert_eq!(
        route.route(&fixture.handle, &subscribe).await,
        RouteOutcome::Claimed
    );
    let frames = fixture.drain();
    let begin = of_type(&frames, "resource.snapshot_begin");
    let chunks = of_type(&frames, "resource.snapshot_chunk");
    let end = of_type(&frames, "resource.snapshot_end");
    assert_eq!((begin.len(), chunks.len(), end.len()), (1, 1, 1));

    assert_eq!(begin[0]["body"]["chunkCount"], 1);
    assert_eq!(begin[0]["body"]["schemaVersion"], 1);
    assert_eq!(
        begin[0]["body"]["cursor"]["originSequence"], "1",
        "快照的 cursor 是该会话当前的 origin head"
    );
    let items = chunks[0]["body"]["items"].as_array().expect("items");
    assert_eq!(chunks[0]["body"]["resource"], "session_meta");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["sessionMeta"]["state"], "idle");
    assert_eq!(items[0]["sessionRef"]["sessionId"], SESSION);
    assert!(
        chunks[0]["body"].get("events").is_none(),
        "快照不得携带会话正文"
    );

    // digest 必须能由「收到的帧字节」复算（接收方不得重新序列化）。
    let expected = {
        let mut hasher = Sha256::new();
        hasher.update(Sha256::digest(chunks[0].text.as_bytes()));
        base64url(&hasher.finalize())
    };
    assert_eq!(end[0]["body"]["snapshotDigest"], expected);
    assert_eq!(
        end[0]["body"]["snapshotId"], begin[0]["body"]["snapshotId"],
        "begin/chunk/end 属于同一次快照"
    );
}

/// [R58]：未决交互按协商的 `resourceSnapshotBatchSize` 分批，`pending_interactions` 的
/// `payloadDigest` 取自创建该交互的 origin 事件。
#[tokio::test]
async fn pending_interactions_are_batched_by_the_negotiated_limit() {
    let slice = NodeLinkSlice {
        summary: session_summary(),
        head: origin_head(1),
        pending_interactions: vec![
            pending_interaction("7ae1c07c-9242-46e9-a9d2-4ec58c130f41"),
            pending_interaction("7ae1c07c-9242-46e9-a9d2-4ec58c130f42"),
        ],
        events: vec![stored_event(
            1,
            "agent.message",
            r#"{"kind":"agent.message"}"#,
        )],
    };

    let mut fixture = Fixture::with_slice(slice).await;
    let route = fixture.route();
    let attached = fixture.attach(&route).await;
    let subscribe = fixture.envelope(
        MessageType::ResourceSubscribe,
        json!({
            "attachmentId": attached["attachmentId"],
            "attachmentGeneration": attached["attachmentGeneration"],
            "cursor": null,
        }),
    );
    route.route(&fixture.handle, &subscribe).await;
    let frames = fixture.drain();
    let begin = of_type(&frames, "resource.snapshot_begin");
    let chunks = of_type(&frames, "resource.snapshot_chunk");
    assert_eq!(
        begin[0]["body"]["chunkCount"], 2,
        "1 帧 session_meta + 1 帧未决交互"
    );
    assert_eq!(chunks.len(), 2);
    let items = chunks[1]["body"]["items"].as_array().expect("items");
    assert_eq!(chunks[1]["body"]["resource"], "pending_interactions");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["kind"], "permission");
    assert_eq!(
        items[0]["interactionId"],
        "7ae1c07c-9242-46e9-a9d2-4ec58c130f41"
    );
    assert!(items[0]["payloadDigest"].is_string());
    assert_eq!(end_of(&frames)["chunkCount"], 2);
}

fn end_of(frames: &[Frame]) -> Value {
    of_type(frames, "resource.snapshot_end")[0]["body"].clone()
}

/// [R59]/[R62]：origin 增量重放按 cursor 之后的 origin 序列继续；epoch 不符 → `sequence_invalid`。
#[tokio::test]
async fn replay_resumes_after_the_cursor_and_rejects_an_epoch_mismatch() {
    let slice = NodeLinkSlice {
        summary: session_summary(),
        head: origin_head(2),
        pending_interactions: Vec::new(),
        events: vec![
            stored_event(1, "agent.message", r#"{"kind":"agent.message","n":1}"#),
            OwnedEventRecord {
                event: CommittedEvent {
                    id: EventId::new("4ae1c07c-9242-46e9-a9d2-4ec58c130f4e").expect("event id"),
                    event_type: EventType::new("agent.message").expect("event type"),
                    session: Some(session_id()),
                    session_sequence: Some(Sequence::new(2).expect("sequence")),
                    global_sequence: Sequence::new(2).expect("sequence"),
                    origin_epoch: Some(OriginEpoch::new(ORIGIN_EPOCH).expect("epoch")),
                    origin_sequence: Some(Sequence::new(2).expect("sequence")),
                    created_at: ts("2026-09-18T09:10:30.000Z"),
                },
                payload: CoreEventPayload::new(
                    ViewJson::new(r#"{"kind":"agent.message","n":2}"#).expect("view"),
                    None,
                ),
            },
        ],
    };
    let mut fixture = Fixture::with_slice(slice).await;
    let route = fixture.route();
    let attached = fixture.attach(&route).await;

    // cursor = 该会话的第 1 条 → 只投递第 2 条。
    let subscribe = fixture.envelope(
        MessageType::ResourceSubscribe,
        json!({
            "attachmentId": attached["attachmentId"],
            "attachmentGeneration": attached["attachmentGeneration"],
            "cursor": { "originEpoch": ORIGIN_EPOCH, "originSequence": "1" },
        }),
    );
    assert_eq!(
        route.route(&fixture.handle, &subscribe).await,
        RouteOutcome::Claimed
    );
    let frames = fixture.drain();
    let events = of_type(&frames, "resource.event");
    assert_eq!(events.len(), 1, "只重放 cursor 之后的事件");
    assert_eq!(events[0]["body"]["originSequence"], "2");
    assert_eq!(events[0]["body"]["sessionRef"]["exportId"], EXPORT);
    assert!(events[0]["body"]["payloadDigest"].is_string());
    // 重放路径不带快照帧（快照只在 cursor = null 时走）。
    assert!(of_type(&frames, "resource.snapshot_begin").is_empty());

    // epoch 不符：同一位置、不同 epoch → sequence_invalid（不从错误位置重放）。
    let mismatched = fixture.envelope(
        MessageType::ResourceSubscribe,
        json!({
            "attachmentId": attached["attachmentId"],
            "attachmentGeneration": attached["attachmentGeneration"],
            "cursor": {
                "originEpoch": "018f6f89-8a23-7a10-a0d3-f92e6a31d953",
                "originSequence": "1",
            },
        }),
    );
    assert_eq!(
        route.route(&fixture.handle, &mismatched).await,
        RouteOutcome::Claimed
    );
    let frames = fixture.drain();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["type"], "link.error");
    assert_eq!(error_code(&frames[0]), "nodelink.protocol.sequence_invalid");
}

/// [R63]/[R64]：ACK 只推进水位；回退与 epoch 不符一律 `sequence_invalid`，且不影响后续 ACK。
#[tokio::test]
async fn ack_progress_is_monotonic_and_bound_to_the_attachment() {
    let mut fixture = Fixture::new().await;
    let route = fixture.route();
    let attached = fixture.attach(&route).await;
    let subscribe = fixture.envelope(
        MessageType::ResourceSubscribe,
        json!({
            "attachmentId": attached["attachmentId"],
            "attachmentGeneration": attached["attachmentGeneration"],
            "cursor": null,
        }),
    );
    route.route(&fixture.handle, &subscribe).await;
    let _ = fixture.drain();

    fn ack_envelope(fixture: &Fixture, sequence: &str, epoch: &str) -> Envelope {
        fixture.envelope(
            MessageType::ResourceAck,
            json!({
                "sessionRef": remote_session_ref(),
                "cursor": { "originEpoch": epoch, "originSequence": sequence },
            }),
        )
    }

    // 合法 ACK：不产生任何回帧（ACK 没有响应），水位推进到 1。
    assert_eq!(
        route
            .route(&fixture.handle, &ack_envelope(&fixture, "1", ORIGIN_EPOCH))
            .await,
        RouteOutcome::Claimed
    );
    assert!(fixture.drain().is_empty(), "ACK 成功不产生回帧");

    // 回退到 0 → sequence_invalid。
    assert_eq!(
        route
            .route(&fixture.handle, &ack_envelope(&fixture, "0", ORIGIN_EPOCH))
            .await,
        RouteOutcome::Claimed
    );
    let frames = fixture.drain();
    assert_eq!(frames.len(), 1);
    assert_eq!(error_code(&frames[0]), "nodelink.protocol.sequence_invalid");

    // epoch 不符 → sequence_invalid。
    assert_eq!(
        route
            .route(
                &fixture.handle,
                &ack_envelope(&fixture, "2", "018f6f89-8a23-7a10-a0d3-f92e6a31d953"),
            )
            .await,
        RouteOutcome::Claimed
    );
    let frames = fixture.drain();
    assert_eq!(error_code(&frames[0]), "nodelink.protocol.sequence_invalid");

    // 未 attach 的连接（同一路由的另一条连接）上的 ACK → 归属不符 → sequence_invalid。
    let other = Fixture::new().await;
    let mut other = other;
    let foreign = other.envelope(
        MessageType::ResourceAck,
        json!({
            "sessionRef": remote_session_ref(),
            "cursor": { "originEpoch": ORIGIN_EPOCH, "originSequence": "1" },
        }),
    );
    // 该连接没有 attachment：ACK 必须被拒绝，而不是被静默接受。
    assert_eq!(
        route.route(&other.handle, &foreign).await,
        RouteOutcome::Claimed
    );
    let frames = other.drain();
    assert_eq!(frames.len(), 1);
    assert_eq!(error_code(&frames[0]), "nodelink.protocol.sequence_invalid");
}

/// [R65]：扇出入口经有界 channel + 异步分发，把已持久化事件投给订阅了该会话的连接。
#[tokio::test]
async fn persisted_events_reach_the_subscribed_connection() {
    let mut fixture = Fixture::new().await;
    let route = Arc::new(fixture.route());
    let attached = fixture.attach(&route).await;
    let subscribe = fixture.envelope(
        MessageType::ResourceSubscribe,
        json!({
            "attachmentId": attached["attachmentId"],
            "attachmentGeneration": attached["attachmentGeneration"],
            "cursor": null,
        }),
    );
    route.route(&fixture.handle, &subscribe).await;
    let _ = fixture.drain();

    let (publisher, queue) = NodeLinkPublisher::channel();
    let (shutdown_handle, shutdown) = Shutdown::channel();
    let dispatch = tokio::spawn(Arc::clone(&route).dispatch(queue, shutdown));

    // broker 在持久化成功之后发布的正是这个定位单元（不含正文）。
    let stored = stored_event(1, "agent.message", r#"{"kind":"agent.message"}"#);
    publisher.publish(CommittedDelivery::Owned(stored.event.clone()));

    let frame = fixture.next_frame().await;
    assert_eq!(frame["type"], "resource.event");
    assert_eq!(frame["body"]["originEventId"], EVENT);
    assert_eq!(frame["body"]["originSequence"], "1");
    assert_eq!(frame["body"]["eventType"], "agent.message");
    assert_eq!(frame["body"]["sessionRef"]["ownerNodeId"], ACCESS_NODE);
    // 接收方复算摘要：payloadDigest 必须是 ACPR-CJ1(payload) 的 SHA-256。
    let payload = &frame["body"]["payload"];
    let canonical = acpr_wire::cj1::canonicalize(&payload.to_string()).expect("规范化");
    let expected = {
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        base64url(&hasher.finalize())
    };
    assert_eq!(frame["body"]["payloadDigest"], expected);

    // 关闭序列：分发循环随 Shutdown 结束（不留 detached task）。
    shutdown_handle.trigger();
    tokio::time::timeout(IO_TIMEOUT, dispatch)
        .await
        .expect("分发循环必须随关闭信号结束")
        .expect("分发任务不 panic");
}

/// 没有订阅的连接不会收到事件（扇出按 attachment 的当前代际过滤）。
#[tokio::test]
async fn events_are_not_delivered_without_a_live_subscription() {
    let mut fixture = Fixture::new().await;
    let route = Arc::new(fixture.route());
    // 只 attach、不 subscribe。
    let _ = fixture.attach(&route).await;

    let (publisher, queue) = NodeLinkPublisher::channel();
    let (shutdown_handle, shutdown) = Shutdown::channel();
    let dispatch = tokio::spawn(Arc::clone(&route).dispatch(queue, shutdown));
    publisher.publish(CommittedDelivery::Owned(
        stored_event(1, "agent.message", r#"{"kind":"agent.message"}"#).event,
    ));
    // 给分发循环一个调度机会：没有任何帧应当出现。
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        fixture.outbound.try_recv().is_err(),
        "未订阅的连接不得收到 resource.event"
    );
    shutdown_handle.trigger();
    let _ = tokio::time::timeout(IO_TIMEOUT, dispatch).await;
}
