//! Access 侧出站运行时（`docs/MODULE_ARCHITECTURE.md` §4.6、`NODE_LINK_PROTOCOL.md` §13/§15）。
//!
//! 三件事，全部围绕「本机信任了哪些 Owner」这一份持久事实：
//!
//! 1. **配对**：`node pair --mode access` 的 claim/status 状态机（[`AccessPairing`] 的实现）。
//!    secret 只在内存；二维码 URL 的 host 与 endpoint 必须一致；Owner 的 `ownerProof` 用二维码固定的
//!    公钥验签；观察到 `approved` 且身份与本机/邀请一致后，才把 Owner 信任记录写入 `TrustStore`。
//! 2. **出站连接**：启动时按 `kind = Owner`、`state = Paired` 的记录逐个建立 [`OwnerLink`]
//!    （`node-link-client` 的 supervisor 自有重连与撤销语义）；链路建立后的 catalog/import 注册由
//!    `node-link-client` 的 `ImportedSink`（[`node_link_client::delivery::DeliveryBridge`]）完成——
//!    即「先提交无正文收据，再发布，最后 ACK」。
//! 3. **路由**：远程 `SessionEndpoint` 按「当前活着的 link」惰性构造（[`AccessManager::endpoint`]），
//!    附件转发走独立认证连接（[`AccessManager::open_attachment`]）。撤销（`node.revoke`）先移除 link
//!    再 `shutdown`，因此**路由失效是移除的必然结果**，不存在需要单独清理的旁表。
//!
//! 关闭顺序：本结构持有全部 `OwnerLink`；[`AccessManager::shutdown_all`] 逐个 `shutdown` 并等待
//! supervisor 结束（每个 link 内部有 5 s 上限），因此没有 detached task。组合根必须在关闭存储之前
//! 调用它（见 `Composition::close`）。
//!
//! 明确的边界：本地 Sync 客户端**读**远程会话（列表/重放/无正文事件）在本切片连通；把本地客户端的
//! `session.prompt` 等 mutation 转发给 Owner 属 `server::acp_facade`（尚未落地），core 对 imported
//! mutation 的显式拒绝保持不变（[`AccessManager::endpoint`] 是该 facade 的现成输入）。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use acp_core::broker::Broker;
use acp_core::model::{
    Actor, NodeId, NodeKind, NodeRecord, NodeState, Nonce, PeerIdentity, PeerPublicKey, PortError,
    RemoteSessionRef, Timestamp, UnavailableKind,
};
use acp_core::ports::{
    Clock, ImportedRead, ImportedReadQuery, RemoteDeliveryStore, RemoteSessionReads,
    SessionEndpoint,
};
use acp_core::use_cases::UseCases;
use identity_auth::EntropySource;
use node_link_client::delivery::DeliveryBridge;
use node_link_client::link::ImportedSink;
use node_link_client::pairing::{self, ClaimSession, StatusOutcome};
use node_link_client::{
    AttachmentForwarder, ClientConfig, ClientError, KeystoreIdentity, NodeIdentity as LinkIdentity,
    NodeLinkClient, OwnerLink, OwnerTrust, PairingTicket, WssTransportFactory,
};
use node_link_protocol::catalog::CatalogSnapshot;
use node_link_protocol::common::{
    OriginCursor, RemoteSessionRef as WireRemoteSessionRef, SessionMeta,
};
use node_link_protocol::pairing::PairingStatus;
use node_link_protocol::resource::Event as NodeLinkEvent;
use serde_json::Value;
use server::local_admin::{
    AccessPairing, AdminError, DaemonLink, DaemonLinkState, JsonObject, LocalErrorCode,
};

use crate::identity::NodeIdentity;
use crate::trust::{OutboundTrust, TrustError};

/// 配对状态（§5.4 的 `state` 词表 + 终态）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    /// claim 已受理，等待 Owner 本机确认。
    PendingConfirmation,
    /// Owner 已批准，且本机信任记录已提交。
    Approved {
        node_id: NodeId,
        confirmed_at: Timestamp,
    },
    /// Owner 明确拒绝。
    Rejected,
    /// 已过期。
    Expired,
    /// pairing secret 已被使用。
    Consumed,
}

impl Phase {
    /// §5.4 的 `state` 文本。
    fn token(&self) -> &'static str {
        match self {
            Self::PendingConfirmation => "pending_confirmation",
            Self::Approved { .. } => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
            Self::Consumed => "consumed",
        }
    }

    fn is_terminal(&self) -> bool {
        !matches!(self, Self::PendingConfirmation)
    }
}

/// 一条 Access 方向的配对（全部状态只在内存；secret 由 [`ClaimSession`] 持有）。
#[derive(Clone)]
struct ClaimState {
    session: ClaimSession,
    owner_id: NodeId,
    fingerprint: String,
    owner_key: PeerPublicKey,
    phase: Phase,
    last_refresh_error: Option<String>,
}

/// 组装 Access 运行时所需的依赖（全部来自组合根注入）。
pub struct AccessDeps {
    pub identity: NodeIdentity,
    pub keystore: Arc<dyn identity_auth::IdentityKeystore>,
    pub core: Arc<UseCases>,
    pub broker: Arc<Broker>,
    pub deliveries: Arc<dyn RemoteDeliveryStore>,
    pub clock: Arc<dyn Clock>,
    pub entropy: Arc<dyn EntropySource>,
    /// `[node] name`：claim 时作为 `nodeName`。
    pub node_name: Option<String>,
}

/// Node Link 入站事件的消费者：无正文收据/索引仍由 [`DeliveryBridge`] 完成，本类型只补一件
/// `DeliveryBridge` 职责之外的事实——**Owner 撤回信任时把本地信任行落成撤销终态**。
///
/// 为什么不能留给 supervisor：`OwnerLink` 只负责「停止重连」，它不拥有持久化信任。若这里不写库，
/// Access 重启后 `start_persisted` 会把状态仍是 `paired` 的 Owner 重新连上；Owner 会再次拒绝，但本地
/// 状态与事实不符（并且 `node.list` 会显示 `paired`）。
struct AccessImportedSink {
    bridge: Arc<DeliveryBridge>,
    core: Arc<UseCases>,
}

#[async_trait::async_trait]
impl ImportedSink for AccessImportedSink {
    async fn event(
        &self,
        owner: &NodeId,
        event: &NodeLinkEvent,
    ) -> Result<Option<OriginCursor>, PortError> {
        self.bridge.event(owner, event).await
    }

    async fn catalog(&self, owner: &NodeId, snapshot: &CatalogSnapshot) -> Result<(), PortError> {
        self.bridge.catalog(owner, snapshot).await
    }

    async fn session_meta(
        &self,
        owner: &NodeId,
        session: &WireRemoteSessionRef,
        meta: &SessionMeta,
    ) -> Result<(), PortError> {
        self.bridge.session_meta(owner, session, meta).await
    }

    async fn revoked(
        &self,
        owner: &NodeId,
        _revoked_at: Option<&Timestamp>,
    ) -> Result<(), PortError> {
        // 撤销是终态，重复写入是幂等的（`TrustStore::revoke_node` 的终态语义）。
        // 归因用 `LocalCli`：这是本机记录一条从 Owner 收到的信任事实，不是对端命令。
        self.core.revoke_node(&Actor::LocalCli, owner).await?;
        tracing::info!(
            event = "daemon.access_owner_revoked_local",
            node = %owner.as_str(),
            "本地信任行已标记为撤销：该 Owner 不再进入启动重连计划"
        );
        Ok(())
    }
}

/// Access 侧出站运行时。
pub struct AccessManager {
    core: Arc<UseCases>,
    deliveries: Arc<dyn RemoteDeliveryStore>,
    clock: Arc<dyn Clock>,
    identity: Arc<dyn LinkIdentity>,
    client: NodeLinkClient,
    trust: OutboundTrust,
    entropy: Arc<dyn EntropySource>,
    node_name: Option<String>,
    /// Owner node id → 连接句柄。**这是路由的唯一事实来源**：移除即撤销路由。
    links: Mutex<BTreeMap<String, Arc<OwnerLink>>>,
    /// `pairingId` → claim 状态。
    claims: Mutex<BTreeMap<String, ClaimState>>,
}

impl std::fmt::Debug for AccessManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccessManager")
            .field("links", &lock(&self.links).len())
            .field("claims", &lock(&self.claims).len())
            .finish_non_exhaustive()
    }
}

impl AccessManager {
    /// 装配。信任锚由 [`OutboundTrust::load`] 提供；`Roots` 为空时失败关闭。
    pub fn new(deps: AccessDeps, trust: OutboundTrust) -> Result<Self, TrustError> {
        // 启动期失败关闭：至少有一个根锚（`OutboundTrust::load` 已保证；这里不依赖装配顺序）。
        trust.roots_trust()?;
        // 传输不在这里构造：`ws://` dev-loopback 与 `wss://` 生产端点需要**不同的** `TlsTrust`，
        // 因此由 [`AccessManager::ensure_link`] 按 endpoint scheme 选择后交给 `node-link-client`。
        let identity: Arc<dyn LinkIdentity> = Arc::new(KeystoreIdentity::new(
            deps.identity.node_id().clone(),
            Arc::clone(&deps.keystore),
            deps.identity.key().clone(),
        ));
        let sink = Arc::new(AccessImportedSink {
            bridge: Arc::new(DeliveryBridge::new(
                Arc::clone(&deps.broker),
                Arc::clone(&deps.deliveries),
                Arc::clone(&deps.clock),
            )),
            core: Arc::clone(&deps.core),
        });
        let client = NodeLinkClient::new(
            Arc::new(WssTransportFactory),
            Arc::clone(&identity),
            sink,
            Arc::clone(&deps.deliveries),
        );
        Ok(Self {
            core: deps.core,
            deliveries: deps.deliveries,
            clock: deps.clock,
            identity,
            client,
            trust,
            entropy: deps.entropy,
            node_name: deps.node_name,
            links: Mutex::new(BTreeMap::new()),
            claims: Mutex::new(BTreeMap::new()),
        })
    }

    /// 启动时按持久化 Owner 信任建立连接（失败只记录，不阻止启动：supervisor 会继续重连）。
    pub async fn start_persisted(&self) {
        let nodes = match self.core.nodes(&Actor::LocalCli).await {
            Ok(nodes) => nodes,
            Err(error) => {
                tracing::warn!(
                    event = "daemon.access_trust_unreadable",
                    reason = crate::compose::port_error_token(&error),
                    "读取节点信任失败：本次启动不建立任何出站连接"
                );
                return;
            }
        };
        for record in nodes {
            if record.kind() != NodeKind::Owner || record.state() != NodeState::Paired {
                continue;
            }
            let Some(endpoint) = record.owner_endpoint().map(str::to_owned) else {
                tracing::warn!(
                    event = "daemon.access_endpoint_missing",
                    node = %record.node_id().as_str(),
                    "Owner 信任记录缺少 endpoint：跳过该连接（不猜测地址）"
                );
                continue;
            };
            let key = match self
                .core
                .peer_key(
                    &Actor::LocalCli,
                    &PeerIdentity::Node(record.node_id().clone()),
                )
                .await
            {
                Ok(Some(key)) => key,
                Ok(None) => {
                    tracing::warn!(
                        event = "daemon.access_peer_key_missing",
                        node = %record.node_id().as_str(),
                        "Owner 信任记录没有绑定公钥：跳过该连接"
                    );
                    continue;
                }
                Err(error) => {
                    tracing::warn!(
                        event = "daemon.access_peer_key_unreadable",
                        node = %record.node_id().as_str(),
                        reason = crate::compose::port_error_token(&error),
                        "读取 Owner 公钥失败：跳过该连接"
                    );
                    continue;
                }
            };
            match self.ensure_link(record.node_id(), &key, &endpoint) {
                Ok(_) => tracing::info!(
                    event = "daemon.access_link_started",
                    node = %record.node_id().as_str(),
                    "已建立（或复用）到 Owner 的出站连接"
                ),
                Err(error) => tracing::warn!(
                    event = "daemon.access_link_failed",
                    node = %record.node_id().as_str(),
                    error = %error,
                    "出站连接无法建立：该 Owner 保持离线，等待人工修正信任记录或配置"
                ),
            }
        }
    }

    /// 该 Owner 当前是否在线（Sync 面 `origin.online` 的第二个来源）。
    pub fn is_online(&self, owner: &NodeId) -> bool {
        lock(&self.links)
            .get(owner.as_str())
            .is_some_and(|link| link.is_online())
    }

    /// `daemon.status.links[]`：每个已登记 Owner 一条运行时状态。
    pub fn links(&self) -> Vec<DaemonLink> {
        lock(&self.links)
            .values()
            .map(|link| DaemonLink {
                node_id: link.owner().clone(),
                state: if link.is_online() {
                    DaemonLinkState::Connected
                } else if link.is_revoked() {
                    DaemonLinkState::Offline
                } else {
                    DaemonLinkState::Connecting
                },
                // `OwnerLink` 的观察快照不携带时间戳：不编造「最近连接/下次重连」时间。
                last_connected_at: None,
                next_retry_at: None,
            })
            .collect()
    }

    /// 远程会话端点（`acp_facade` 与本地客户端的读取面共用）。
    ///
    /// 端点由**当前活着的 link** 构造：link 被移除（撤销）后这里立即返回 `None`，因此不存在
    /// 「路由仍然指向已撤销 Owner」的中间态。
    pub fn endpoint(&self, remote: &RemoteSessionRef) -> Option<Arc<dyn SessionEndpoint>> {
        let link = lock(&self.links)
            .get(remote.owner_node_id.as_str())
            .cloned()?;
        // 撤销之后 link 可能仍留在表里等待 supervisor 收尾（`revoke` 会主动移除，但 Owner 推送的
        // `node.trust.revoked` 只置 revoked 标志）。路由必须立即失效，不能把请求发给已被撤销的节点。
        if link.is_revoked() {
            return None;
        }
        Some(Arc::new(
            link.session(remote.clone(), Arc::clone(&self.deliveries)),
        ))
    }

    /// 远程附件转发（独立认证连接；每次调用一条，用完即关）。
    pub async fn open_attachment(
        &self,
        owner: &NodeId,
    ) -> Result<AttachmentForwarder, ClientError> {
        let link = lock(&self.links).get(owner.as_str()).cloned();
        let Some(link) = link else {
            return Err(ClientError::OwnerOffline);
        };
        if link.is_revoked() {
            return Err(ClientError::Revoked);
        }
        AttachmentForwarder::connect(&link).await
    }

    /// 撤销后停止重连并取消路由（幂等）。`node.revoke` 在持久提交之后调用。
    pub async fn revoke(&self, node: &NodeId) {
        let link = lock(&self.links).remove(node.as_str());
        if let Some(link) = link {
            link.shutdown().await;
            tracing::info!(
                event = "daemon.access_link_revoked",
                node = %node.as_str(),
                "节点撤销已提交：出站连接已停止，远程路由随之失效"
            );
        }
    }

    /// 关闭全部出站连接并等待 supervisor 结束（关闭序列在关闭存储之前调用）。
    pub async fn shutdown_all(&self) {
        let links = std::mem::take(&mut *lock(&self.links));
        for link in links.values() {
            link.shutdown().await;
        }
        if !links.is_empty() {
            tracing::info!(
                event = "daemon.access_links_stopped",
                count = links.len(),
                "全部出站连接已停止"
            );
        }
    }

    /// 建立（或复用）一个 Owner 的连接。
    fn ensure_link(
        &self,
        owner: &NodeId,
        key: &PeerPublicKey,
        endpoint: &str,
    ) -> Result<Arc<OwnerLink>, ClientError> {
        if let Some(link) = lock(&self.links).get(owner.as_str()).cloned() {
            return Ok(link);
        }
        // 端点必须与当前信任锚匹配：Owner 端点只允许 `wss://`（`NodeRecord` 的同一契约）；
        // 明文 scheme 显式拒绝，不回退、不猜测。
        let (scheme, host) = endpoint_parts(endpoint)?;
        let trust = self.trust.for_scheme(scheme, &host).map_err(|error| {
            ClientError::Local(match error {
                TrustError::PlaintextNotSupported => "the owner endpoint must use wss://",
                TrustError::UnsupportedScheme => "the owner endpoint scheme is not supported",
                _ => "the owner endpoint has no usable TLS trust",
            })
        })?;
        let config = ClientConfig::new(
            OwnerTrust {
                node_id: owner.clone(),
                public_key: key.clone(),
            },
            endpoint,
            trust,
        );
        let link = self.client.open(config)?;
        lock(&self.links).insert(owner.as_str().to_owned(), Arc::clone(&link));
        Ok(link)
    }

    /// 一个 32 字节 nonce（`clientNonce`/`requestNonce`）。
    fn nonce(&self) -> Result<Nonce, AdminError> {
        let mut bytes = [0u8; 32];
        self.entropy.fill(&mut bytes).map_err(|_| {
            AdminError::new(
                LocalErrorCode::Internal,
                "the local entropy source is unavailable",
            )
        })?;
        let text = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes);
        Nonce::new(&text).map_err(|_| {
            AdminError::new(
                LocalErrorCode::Internal,
                "the generated nonce is not canonical base64url",
            )
        })
    }

    /// claim 的展示名：`displayName` → `[node] name` → node id 前缀（与 owner 方向同款回落）。
    fn display_name(&self, supplied: Option<&str>) -> String {
        supplied
            .map(str::to_owned)
            .or_else(|| self.node_name.clone())
            .unwrap_or_else(|| {
                let text = self.identity.node_id().as_str();
                text[..8.min(text.len())].to_owned()
            })
    }

    /// 持久化 Owner 信任并启动连接；返回 `confirmedAt`。
    async fn persist_owner(&self, state: &ClaimState) -> Result<(NodeId, Timestamp), AdminError> {
        let owner_id = state.owner_id.clone();
        let now = self.clock.now();
        let existing = self
            .core
            .node(&Actor::LocalCli, &owner_id, NodeKind::Owner)
            .await
            .map_err(|error| map_port_error("node.pair.status", error))?;
        if let Some(record) = existing {
            if record.node_public_key_fingerprint() != &state.owner_key.fingerprint() {
                return Err(AdminError::new(
                    LocalErrorCode::Conflict,
                    "node.pair.status: the owner node id is already bound to a different key",
                ));
            }
            if record.state() == NodeState::Revoked {
                return Err(AdminError::new(
                    LocalErrorCode::Conflict,
                    "node.pair.status: the owner node was revoked locally; re-pairing is not allowed",
                ));
            }
            return Ok((owner_id, now));
        }
        let endpoint = state.session.ticket().endpoint().as_str().to_owned();
        let display_name = display_name_for_endpoint(&endpoint)?;
        let record = NodeRecord::try_new(
            owner_id.clone(),
            &display_name,
            NodeKind::Owner,
            state.owner_key.fingerprint(),
            NodeState::Paired,
            Some(endpoint.clone()),
            now.clone(),
            None,
            None,
        )
        .map_err(|_| {
            AdminError::new(
                LocalErrorCode::Internal,
                "node.pair.status: the owner trust record cannot be assembled",
            )
        })?;
        self.core
            .put_node(&Actor::LocalCli, record, state.owner_key.clone())
            .await
            .map_err(|error| map_port_error("node.pair.status", error))?;
        if let Err(error) = self.ensure_link(&owner_id, &state.owner_key, &endpoint) {
            // 信任记录已提交（这是配对的目的）；连接失败只意味着 Owner 暂时离线，supervisor 之外
            // 的重试在下次启动/下次配对时发生，不回滚已提交的信任。
            tracing::warn!(
                event = "daemon.access_link_failed",
                node = %owner_id.as_str(),
                error = %error,
                "Owner 信任已提交，但出站连接建立失败"
            );
        }
        tracing::info!(
            event = "daemon.access_owner_paired",
            node = %owner_id.as_str(),
            "已持久化 Owner 信任记录"
        );
        Ok((owner_id, now))
    }

    /// 一次远程刷新。
    async fn refresh(&self, state: &ClaimState) -> Result<StatusOutcome, ClientError> {
        let nonce = Nonce::new(&base64url(&random_bytes(&self.entropy)?))
            .map_err(|_| ClientError::Local("the generated nonce is not canonical base64url"))?;
        let trust = self
            .trust
            .roots_trust()
            .map_err(|_| ClientError::Local("no TLS trust anchor is available"))?;
        let now = self.clock.now();
        pairing::status(&state.session, self.identity.as_ref(), &nonce, &trust, &now).await
    }
}

#[async_trait::async_trait]
impl AccessPairing for AccessManager {
    async fn begin(
        &self,
        pairing_url: &str,
        display_name: Option<&str>,
    ) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "node.pair.begin";
        let ticket =
            PairingTicket::parse(pairing_url, self.trust.allows_plaintext()).map_err(|error| {
                AdminError::new(
                    LocalErrorCode::InvalidParams,
                    format!("{OPERATION}: {error}"),
                )
            })?;
        let now = self.clock.now();
        if ticket.is_expired(&now) {
            return Err(AdminError::new(
                LocalErrorCode::Expired,
                format!("{OPERATION}: the pairing invitation has expired"),
            ));
        }
        let owner_id = ticket.owner_node_id().map_err(|error| {
            AdminError::new(
                LocalErrorCode::InvalidParams,
                format!("{OPERATION}: {error}"),
            )
        })?;
        let owner_key = ticket.owner_public_key().map_err(|error| {
            AdminError::new(
                LocalErrorCode::InvalidParams,
                format!("{OPERATION}: {error}"),
            )
        })?;
        let fingerprint = owner_key.fingerprint().as_str().to_owned();
        let pairing_id = ticket.pairing_id().map_err(|error| {
            AdminError::new(
                LocalErrorCode::InvalidParams,
                format!("{OPERATION}: {error}"),
            )
        })?;
        if let Some(existing) = lock(&self.claims).get(pairing_id.as_str()) {
            if existing.phase.is_terminal() {
                return Err(AdminError::new(
                    LocalErrorCode::Conflict,
                    format!(
                        "{OPERATION}: this pairing is already {}",
                        existing.phase.token()
                    ),
                ));
            }
        }
        let nonce = self.nonce()?;
        let trust = self
            .trust
            .roots_trust()
            .map_err(|error| AdminError::new(LocalErrorCode::Internal, error.message()))?;
        let name = self.display_name(display_name);
        let outcome = pairing::claim(&ticket, self.identity.as_ref(), &name, &nonce, &trust, &now)
            .await
            .map_err(|error| claim_failure(OPERATION, error))?;
        let state = ClaimState {
            session: outcome.session.clone(),
            owner_id,
            fingerprint,
            owner_key,
            phase: Phase::PendingConfirmation,
            last_refresh_error: None,
        };
        lock(&self.claims).insert(pairing_id.as_str().to_owned(), state.clone());
        let mut result = JsonObject::new();
        result.insert("pairingId".to_owned(), Value::from(pairing_id.as_str()));
        // 回显输入 URL（§5.4）：它含 secret，但这是**同一次调用**的返回值，CLI 只展示、不落盘。
        result.insert("pairingUrl".to_owned(), Value::from(pairing_url));
        result.insert("state".to_owned(), Value::from(state.phase.token()));
        result.insert(
            "expiresAt".to_owned(),
            Value::from(state.session.expires_at().as_str()),
        );
        result.insert(
            "peerNodeId".to_owned(),
            Value::from(state.owner_id.as_str()),
        );
        result.insert(
            "peerPublicKeyFingerprint".to_owned(),
            Value::from(state.fingerprint.as_str()),
        );
        Ok(result)
    }

    async fn status(&self, pairing_id: &str) -> Result<Option<JsonObject>, AdminError> {
        const OPERATION: &str = "node.pair.status";
        let state = match lock(&self.claims).get(pairing_id).cloned() {
            Some(state) => state,
            None => return Ok(None),
        };
        if state.phase.is_terminal() {
            return Ok(Some(status_result(&state)));
        }
        match self.refresh(&state).await {
            Ok(outcome) => {
                let mut updated = state.clone();
                match outcome.status {
                    PairingStatus::Approved => {
                        let (node_id, confirmed_at) = self.persist_owner(&state).await?;
                        updated.phase = Phase::Approved {
                            node_id,
                            confirmed_at,
                        };
                        updated.last_refresh_error = None;
                    }
                    PairingStatus::PendingConfirmation => {
                        updated.last_refresh_error = None;
                    }
                    PairingStatus::Rejected => updated.phase = Phase::Rejected,
                    PairingStatus::Expired => updated.phase = Phase::Expired,
                    PairingStatus::Consumed => updated.phase = Phase::Consumed,
                }
                let result = status_result(&updated);
                lock(&self.claims).insert(pairing_id.to_owned(), updated);
                Ok(Some(result))
            }
            Err(error) => {
                // 刷新失败不改变已有状态（§5.4）：只记录简短原因。
                let mut updated = state.clone();
                updated.last_refresh_error = Some(refresh_failure(OPERATION, &error));
                let result = status_result(&updated);
                lock(&self.claims).insert(pairing_id.to_owned(), updated);
                Ok(Some(result))
            }
        }
    }

    async fn confirm(&self, pairing_id: &str) -> Result<Option<JsonObject>, AdminError> {
        let state = match lock(&self.claims).get(pairing_id).cloned() {
            Some(state) => state,
            None => return Ok(None),
        };
        match state.phase {
            Phase::Approved {
                node_id,
                confirmed_at,
            } => {
                let mut result = JsonObject::new();
                result.insert("nodeId".to_owned(), Value::from(node_id.as_str()));
                result.insert("confirmedAt".to_owned(), Value::from(confirmed_at.as_str()));
                Ok(Some(result))
            }
            Phase::PendingConfirmation => Err(AdminError::new(
                LocalErrorCode::Conflict,
                "node.pair.confirm: an access claim can only be confirmed by the owner node",
            )),
            _ => Err(AdminError::new(
                LocalErrorCode::Expired,
                "node.pair.confirm: this pairing is no longer pending",
            )),
        }
    }

    async fn reject(&self, pairing_id: &str, reason: Option<&str>) -> Result<bool, AdminError> {
        let removed = lock(&self.claims).remove(pairing_id).is_some();
        if removed {
            tracing::info!(
                event = "daemon.access_pairing_rejected",
                has_reason = reason.is_some(),
                "本地配对状态已终结（不向 Owner 发送请求：协议没有对应的拒绝端点）"
            );
        }
        Ok(removed)
    }
}

fn status_result(state: &ClaimState) -> JsonObject {
    let mut result = JsonObject::new();
    result.insert("state".to_owned(), Value::from(state.phase.token()));
    result.insert(
        "peerNodeId".to_owned(),
        Value::from(state.owner_id.as_str()),
    );
    result.insert(
        "peerPublicKeyFingerprint".to_owned(),
        Value::from(state.fingerprint.as_str()),
    );
    result.insert(
        "expiresAt".to_owned(),
        Value::from(state.session.expires_at().as_str()),
    );
    result.insert(
        "lastRefreshError".to_owned(),
        match &state.last_refresh_error {
            Some(text) => Value::from(text.as_str()),
            None => Value::Null,
        },
    );
    result
}

fn claim_failure(operation: &str, error: ClientError) -> AdminError {
    match error {
        ClientError::Pairing(reason) if reason.contains("expired") => AdminError::new(
            LocalErrorCode::Expired,
            format!("{operation}: the pairing invitation has expired"),
        ),
        other => AdminError::new(LocalErrorCode::RemoteError, format!("{operation}: {other}")),
    }
}

fn refresh_failure(operation: &str, error: &ClientError) -> String {
    match error {
        ClientError::Transport(_) | ClientError::Timeout => {
            format!("{operation}: the owner is unreachable")
        }
        ClientError::Pairing(reason) => format!("{operation}: {reason}"),
        other => format!("{operation}: {other}"),
    }
}

fn map_port_error(operation: &str, error: acp_core::model::PortError) -> AdminError {
    AdminError::new(
        LocalErrorCode::Unavailable,
        format!(
            "{operation}: the local store cannot serve the request ({})",
            crate::compose::port_error_token(&error)
        ),
    )
}

/// `wss://<authority>/node-link/v1` → scheme 与 host。形状已由 `ClientConfig` 的端点校验兜住；
/// 这里只做投影，失败即拒绝建立连接（不猜测）。
fn endpoint_parts(endpoint: &str) -> Result<(&str, String), ClientError> {
    let (scheme, rest) = endpoint
        .split_once("://")
        .ok_or(ClientError::Local("the owner endpoint is not a URL"))?;
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        return Err(ClientError::Local("the owner endpoint has no authority"));
    }
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split_once(']')
            .map(|(host, _)| host.to_owned())
            .ok_or(ClientError::Local("the owner endpoint has an invalid host"))?
    } else {
        authority
            .rsplit_once(':')
            .map_or_else(|| authority.to_owned(), |(host, _)| host.to_owned())
    };
    Ok((scheme, host))
}

/// Owner 信任记录的展示名：endpoint 的 host；host 超过 128 字符时回落到 node id 前缀（形状上限）。
fn display_name_for_endpoint(endpoint: &str) -> Result<String, AdminError> {
    let (_, host) = endpoint_parts(endpoint).map_err(|_| {
        AdminError::new(
            LocalErrorCode::Internal,
            "node.pair.status: the owner endpoint is malformed",
        )
    })?;
    if host.is_empty() || host.chars().count() > 128 {
        return Err(AdminError::new(
            LocalErrorCode::Internal,
            "node.pair.status: the owner endpoint host cannot be used as a display name",
        ));
    }
    Ok(host)
}

fn base64url(bytes: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

fn random_bytes(entropy: &Arc<dyn EntropySource>) -> Result<[u8; 32], ClientError> {
    let mut bytes = [0u8; 32];
    entropy
        .fill(&mut bytes)
        .map_err(|_| ClientError::Local("the local entropy source is unavailable"))?;
    Ok(bytes)
}

/// `Mutex` 中毒时取回内部值（与仓库其它适配器的处理一致：不因一次 panic 让路由永久不可用）。
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[async_trait::async_trait]
impl RemoteSessionReads for AccessManager {
    /// imported 会话的 `session.read` 回源（`NODE_LINK_PROTOCOL.md` §12.10）。
    ///
    /// 路由只来自当前活着的 link：Owner 被撤销或从未连接时返回 [`UnavailableKind::OwnerOffline`]，
    /// 由 Sync 适配器映射成 `resource.remote_unavailable`，**不**返回空历史或 `session.not_found`。
    /// 正文只在这次调用的栈上存在；本实现不写数据库、临时文件或日志。
    async fn read_imported(
        &self,
        session: &RemoteSessionRef,
        query: ImportedReadQuery,
    ) -> Result<ImportedRead, PortError> {
        let link = lock(&self.links)
            .get(session.owner_node_id.as_str())
            .cloned();
        let Some(link) = link else {
            return Err(PortError::Unavailable(UnavailableKind::OwnerOffline));
        };
        if link.is_revoked() || !link.is_online() {
            return Err(PortError::Unavailable(UnavailableKind::OwnerOffline));
        }
        link.session(session.clone(), Arc::clone(&self.deliveries))
            .read_page(query)
            .await
    }
}
