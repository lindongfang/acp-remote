//! 配对 HTTP 端点（`docs/NODE_LINK_PROTOCOL.md` §13.1–§13.4，`design.md` D2/D3/D12）。
//!
//! 两个端点共用同一套边界纪律：
//!
//! - **状态机是唯一的内存态**：pairing secret、server nonce 与 request id 只在 `identity-auth` 的
//!   `Authority` 里；配对行/对端行/已批准集合只经 `core::use_cases` 读（配对通道以绑定该配对的
//!   `Actor::PairingClaimant`），本模块不建第二份注册表、不查 SQLite；
//! - **认领方 actor 的构造顺序**（`docs/IDENTITY_AND_AUTH_CONTRACT.md` §5.1）：claim 路径允许先用绑定
//!   actor 只读该配对自身的记录（HMAC 校验必须先拿到记录），但**写入**只发生在 proof 校验**通过之后**；
//!   这次读取零写入、不产生审计，也不向对端回任何记录内容；
//! - **失败不泄露差异**：401 只由证明/绑定/失败计数类拒绝产生，且所有这类拒绝共用同一 `code` 与
//!   `message`；`code` 取自 Node Link 的封闭词表（v1 没有配对专属错误码，因此状态类拒绝统一用
//!   `nodelink.auth.proof_invalid`，HTTP 状态码才是 §13.4 规定的对外区分点——§13.4 对「不同 claim 内容
//!   返回 409」也正是要求该 code）；
//! - **四个安全响应头**（§13.1）在所有响应（含错误）上；secret、proof 与完整二维码 payload 不进日志、
//!   错误消息或审计。
//!
//! limits（§2.5）是固定常量、不可配置：claim 每 IP 10 次/分钟。

use std::collections::HashMap;
use std::collections::VecDeque;
use std::str::FromStr as _;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use acp_core::model::{
    Actor, ConflictKind, NodeId, NodeKind, NodeRecord, Nonce, PairingId, PairingPeer,
    PairingRecord, PairingState, PeerIdentity, PeerPublicKey, PortError,
};
use acp_core::use_cases::UseCases;
use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use identity_auth::{
    Authority, CanonicalOrigin, ClaimFields, ClaimKindFields, ClaimOutcome, ClaimRejection,
    ClaimedPairing, NodeEndpoint, NodeLinkPairingOwnerProof, NodeLinkPairingStatus, P1363Signature,
    PairingProof, PairingRequestMaterial, RequestedCapabilities,
};
use node_link_protocol::common::{
    Base64Url, GrantList, GrantName, NodeName, Nullable, ProtocolVersionV1, RawObject, Timestamp,
    Uuid,
};
use node_link_protocol::error::{Body as ErrorBody, ErrorCode};
use node_link_protocol::pairing::{
    ClaimRequest, ClaimResponse, NodeBlock, OwnerBlock, PairingStatus, PendingConfirmation,
    StatusRequest, StatusResponse,
};

use crate::transport::net::ratelimit::MAX_TRACKED_KEYS;
use crate::transport::net::{
    HttpHandler, HttpRequest, HttpResponse, RateLimit, SlidingWindowLimiter,
};

/// `POST /node-link/v1/pairing/claim`（§13.2）。
pub const CLAIM_PATH: &str = "/node-link/v1/pairing/claim";

/// `POST /node-link/v1/pairing/status`（§13.3）。
pub const STATUS_PATH: &str = "/node-link/v1/pairing/status";

/// claim 的固定限流（§2.5：10 次/分钟/IP，不可配置）。
const CLAIM_LIMIT_PER_MINUTE: u32 = 10;

/// status 的固定限流（§2.5：60 次/分钟/`pairingId`，不可配置）。
const STATUS_LIMIT_PER_MINUTE: u32 = 60;

/// `requestNonce` → 原响应的缓存条目上限（§13.3 的重试语义；超出后淘汰最早的一条）。
const MAX_STATUS_REPLAYS: usize = 1024;

/// 限流窗口（§2.5 的两条配对限流都以 1 分钟为窗口）。
const LIMIT_WINDOW: Duration = Duration::from_secs(60);

/// 401 的统一消息（§13.2：响应不得泄露具体校验差异，所以证明/绑定/失败计数类拒绝共用同一文本）。
const PROOF_INVALID_MESSAGE: &str = "pairing proof is not valid";

/// 组合根注入的配对端点配置快照（本模块不读配置文件，`design.md` D2）。
#[derive(Debug, Clone, Default)]
pub struct PairingHttpConfig {
    /// `daemon.public_origin`（`docs/CONFIG_REFERENCE.md` §1）。
    ///
    /// claim 的 `endpoint` host 必须与它一致（§13.1/§13.2）。未配置时本机没有可宣告的 Node Link
    /// endpoint，配对本身也无法创建（`node.pair.begin` 同口径失败关闭），因此 claim 一律按
    /// 「endpoint host 不允许」拒绝。
    pub public_origin: Option<String>,
}

/// 配对 HTTP 的共享状态（组合根装配一次，注册到两个 path；`PairingHttp::claim_handler`）。
pub struct PairingHttp {
    core: Arc<UseCases>,
    authority: Arc<Authority>,
    config: PairingHttpConfig,
    /// claim 限流：键是 `PeerInfo::client_ip`（对端真实连接地址；不可信来源的转发头不生效）。
    claim_limit: SlidingWindowLimiter,
    /// status 限流：键是 `pairingId`（§2.5 的另一条固定限流）。
    status_limit: PairingIdWindow,
    /// status 的「网络重试返回原响应」缓存（§13.3）。
    status_replays: Mutex<StatusReplays>,
}

impl PairingHttp {
    /// 装配。`core` 是用例面，`authority` 是身份状态机（两者都由组合根持有），`config` 是配置快照。
    pub fn new(core: Arc<UseCases>, authority: Arc<Authority>, config: PairingHttpConfig) -> Self {
        Self {
            core,
            authority,
            config,
            claim_limit: SlidingWindowLimiter::new(CLAIM_LIMIT_PER_MINUTE, LIMIT_WINDOW),
            status_limit: PairingIdWindow::new(STATUS_LIMIT_PER_MINUTE, LIMIT_WINDOW),
            status_replays: Mutex::new(StatusReplays::default()),
        }
    }

    /// claim 端点的处理器（组合根注册到 [`CLAIM_PATH`]）。
    pub fn claim_handler(self: &Arc<Self>) -> Arc<dyn HttpHandler> {
        Arc::new(ClaimEndpoint {
            pairing: self.clone(),
        })
    }

    /// status 端点的处理器（组合根注册到 [`STATUS_PATH`]）。
    pub fn status_handler(self: &Arc<Self>) -> Arc<dyn HttpHandler> {
        Arc::new(StatusEndpoint {
            pairing: self.clone(),
        })
    }

    /// `POST /node-link/v1/pairing/claim`（§13.2）：原子检查「存在 → 未过期 → 仍可认领 → endpoint
    /// host → HMAC」，通过后把配对推进到 `claimed`/`pending_confirmation` 并签发 Owner 证明。
    async fn claim(&self, request: HttpRequest) -> HttpResponse {
        // ① 限流（§2.5）：超限的请求不进入配对状态机。
        if let RateLimit::Denied { retry_after } = self.claim_limit.check(request.peer.client_ip) {
            return rate_limited(retry_after);
        }
        // ② 只接受 application/json（§13.1）。
        if !is_json_content_type(&request.headers) {
            return reject(
                StatusCode::BAD_REQUEST,
                ErrorCode::ProtocolSchemaInvalid,
                "content type must be application/json",
            );
        }
        // ③ 解码：JSON 语法错误与字段/schema 错误都是 400（§13.4），但 code 分开。
        let claim = match decode_claim(&request.body) {
            Ok(claim) => claim,
            Err(code) => {
                return reject(
                    StatusCode::BAD_REQUEST,
                    code,
                    "the request body is not a valid pairing claim",
                );
            }
        };
        let Some(input) = ClaimInput::from_wire(&claim) else {
            return reject(
                StatusCode::BAD_REQUEST,
                ErrorCode::ProtocolSchemaInvalid,
                "the request body is not a valid pairing claim",
            );
        };

        // ④ 只读该配对自身的记录与对端行；claimant 绑定该配对，读取零写入（§5.1 的顺序规则）。
        let actor = Actor::PairingClaimant {
            pairing: input.pairing.clone(),
        };
        let view = match self.core.pairing_channel_view(&actor, &input.pairing).await {
            Ok(Some(view)) => view,
            // ⑤ 配对不存在 → 404（§13.4）。
            Ok(None) => return not_claimable(StatusCode::NOT_FOUND, "pairing is not known"),
            Err(error) => return self.port_error("claim", &input.pairing, error),
        };
        let record = &view.record;
        // ⑥ 已过期或已 consumed → 410（§13.4，仅 claim 端点）。时间戳是定宽 RFC 3339 文本，字典序即时间序。
        if record.state() == PairingState::Consumed
            || self.authority.now().as_str() >= record.expires_at().as_str()
        {
            return not_claimable(StatusCode::GONE, "pairing has expired or was consumed");
        }
        // ⑦ 拒绝的状态 → 409（§13.4：已被 claim、状态转换冲突）。`approved` 仍在窗口内：幂等重试需要
        // 能返回原 pairing request，是否可继续由状态机决定。
        if record.state() == PairingState::Rejected {
            return not_claimable(StatusCode::CONFLICT, "pairing is not claimable");
        }
        // ⑧ endpoint host 必须与 `daemon.public_origin` 一致 → 403（§13.1/§13.2）。
        if !self.endpoint_host_allowed(&input.endpoint) {
            return not_claimable(StatusCode::FORBIDDEN, "endpoint host is not allowed");
        }

        // ⑨ 结构 → 状态/过期 → 绑定 → 集合 → HMAC 的唯一入口（`identity-auth`）。
        let fields = ClaimFields {
            pairing: input.pairing.clone(),
            peer: PeerIdentity::Node(input.access_node.clone()),
            kind: ClaimKindFields::Node {
                node_kind: input.kind,
            },
            display_name: input.display_name.clone(),
            host_binding: input.endpoint.clone(),
            public_key: input.public_key.clone(),
            client_nonce: input.client_nonce.clone(),
            // claim 请求不带请求集合（§13.2 的字段表里没有 scopes/grants）：认领方请求的就是登记值本身，
            // 因此两侧都用配对行登记的集合——幂等比对与「不得超出登记值」的判定因此同源。
            requested: RequestedCapabilities {
                scopes: record.requested_scopes().clone(),
                grants: record.requested_grants().clone(),
            },
            proof: input.proof,
        };
        let existing = view.peer.as_ref().map(|peer| claimed_pairing(record, peer));
        let outcome = match self
            .authority
            .verify_claim(record, existing.as_ref(), &fields)
        {
            Ok(outcome) => outcome,
            Err(error) => return self.identity_error("claim", &input.pairing, error),
        };
        match outcome {
            ClaimOutcome::Claimed(claimed) => {
                // ⑩ 落库：状态推进 + `pairing.claimed` 审计同一写集，actor 绑定该配对（design D12）。
                let claim = match claimed.to_claim() {
                    Ok(claim) => claim,
                    Err(error) => return self.identity_error("claim", &input.pairing, error),
                };
                match self.core.claim_pairing(&actor, claim).await {
                    Ok(_) => {}
                    Err(PortError::Conflict(
                        ConflictKind::AlreadyClaimed | ConflictKind::IdentityMismatch,
                    )) => {
                        return not_claimable(StatusCode::CONFLICT, "pairing is not claimable");
                    }
                    Err(PortError::Conflict(ConflictKind::Expired)) => {
                        return not_claimable(
                            StatusCode::GONE,
                            "pairing has expired or was consumed",
                        );
                    }
                    Err(PortError::NotFound(_)) => {
                        return not_claimable(StatusCode::NOT_FOUND, "pairing is not known");
                    }
                    Err(error) => return self.port_error("claim", &input.pairing, error),
                }
            }
            // ⑪ 幂等重试：相同 `accessNodeId`/`clientNonce` 与相同内容 → 返回原 pairing request
            // （同一 `pairingRequestId`/`serverNonce`），不创建第二条记录（§13.2/§13.4）。
            ClaimOutcome::Repeat(_) => {}
            ClaimOutcome::Rejected(rejection) => {
                let (status, message) = match rejection {
                    ClaimRejection::Expired => {
                        (StatusCode::GONE, "pairing has expired or was consumed")
                    }
                    ClaimRejection::NotClaimable | ClaimRejection::CapabilitiesExceedRegistered => {
                        (StatusCode::CONFLICT, "pairing is not claimable")
                    }
                    ClaimRejection::BindingMismatch => {
                        (StatusCode::FORBIDDEN, "endpoint host is not allowed")
                    }
                    // 结构/证明/失败计数：统一 401，响应不区分是哪一个字段或哪一种校验失败。
                    ClaimRejection::Malformed
                    | ClaimRejection::ProofInvalid
                    | ClaimRejection::TooManyFailures => {
                        (StatusCode::UNAUTHORIZED, PROOF_INVALID_MESSAGE)
                    }
                };
                return reject(status, ErrorCode::AuthProofInvalid, message);
            }
        }
        self.claim_response(
            record,
            &input.access_node,
            &input.public_key,
            &input.client_nonce,
        )
        .await
    }

    /// claim 的成功响应（201 + `claimResponse`）：`serverNonce`/`pairingRequestId` 来自本机为该配对登记的
    /// 材料（与 secret 同生同灭），`ownerProof` 是本节点对 `node-link-pairing-owner-proof/v1` 的签名。
    async fn claim_response(
        &self,
        record: &PairingRecord,
        access_node: &NodeId,
        public_key: &PeerPublicKey,
        client_nonce: &Nonce,
    ) -> HttpResponse {
        let Some(material) = self.authority.pairing_request_material(record.id()) else {
            // 材料与 secret 同生同灭（§4.3）。走到这里说明内存态与持久状态分歧（过期扫描/重启后的竞态
            // 或接线缺陷）：按「不可认领」失败关闭，绝不伪造 `pairingRequestId`。
            tracing::warn!(
                pairing_id = record.id().as_str(),
                "claim material is gone while the pairing record is not terminal"
            );
            return not_claimable(StatusCode::CONFLICT, "pairing is not claimable");
        };
        let owner_public_key = match self.authority.node_public_key().await {
            Ok(key) => key,
            Err(error) => return self.identity_error("claim", record.id(), error),
        };
        let proof_input = NodeLinkPairingOwnerProof {
            owner_node_id: self.authority.local_node().clone(),
            access_node_id: access_node.clone(),
            pairing_id: record.id().clone(),
            owner_public_key,
            access_public_key: public_key.clone(),
            client_nonce: client_nonce.clone(),
            server_nonce: material.server_nonce.clone(),
            pairing_request_id: material.pairing_request_id.clone(),
        };
        let signature = match self
            .authority
            .sign_node_link_pairing_owner_proof(&proof_input)
            .await
        {
            Ok(signature) => signature,
            Err(error) => return self.identity_error("claim", record.id(), error),
        };
        let Some(body) = claim_response_body(record, &material, &signature) else {
            tracing::error!(
                pairing_id = record.id().as_str(),
                "the claim response cannot be assembled from the pairing material"
            );
            return internal_unavailable();
        };
        match serde_json::to_vec(&body) {
            Ok(bytes) => secure(HttpResponse::json(StatusCode::CREATED, Bytes::from(bytes))),
            Err(error) => {
                tracing::error!(error = %error, "the claim response cannot be encoded");
                internal_unavailable()
            }
        }
    }

    /// `POST /node-link/v1/pairing/status`（§13.3）：校验 `node-link-pairing-status/v1` 的 HMAC 后**一律**
    /// 返回 200，业务状态只由 body 的 `status` 表达（`expired`/`consumed` 也不例外）。
    async fn status(&self, request: HttpRequest) -> HttpResponse {
        // ① 只接受 application/json（§13.1）。
        if !is_json_content_type(&request.headers) {
            return reject(
                StatusCode::BAD_REQUEST,
                ErrorCode::ProtocolSchemaInvalid,
                "content type must be application/json",
            );
        }
        // ② 解码：JSON 语法错误与字段/schema 错误都是 400（§13.4）。
        let status_request = match decode_status(&request.body) {
            Ok(request) => request,
            Err(code) => {
                return reject(
                    StatusCode::BAD_REQUEST,
                    code,
                    "the request body is not a valid pairing status query",
                );
            }
        };
        let Some(input) = StatusInput::from_wire(&status_request) else {
            return reject(
                StatusCode::BAD_REQUEST,
                ErrorCode::ProtocolSchemaInvalid,
                "the request body is not a valid pairing status query",
            );
        };
        // ③ 限流（§2.5：60 次/分钟/pairingId）：超限的请求不进入配对状态机。
        if let RateLimit::Denied { retry_after } = self.status_limit.check(input.pairing.as_str()) {
            return rate_limited(retry_after);
        }
        // ④ 只读该配对自身（claimant 绑定该配对；和 claim 一样，读取不产生任何写入）。
        let actor = Actor::PairingClaimant {
            pairing: input.pairing.clone(),
        };
        let view = match self.core.pairing_channel_view(&actor, &input.pairing).await {
            Ok(Some(view)) => view,
            // 未登记的 pairingId：本端点的存在性不对外区分，且没有 secret 就无法证明身份 → 401。
            Ok(None) => return proof_invalid(),
            Err(error) => return self.port_error("status", &input.pairing, error),
        };
        let record = &view.record;
        let peer_view = view.peer.as_ref().map(|peer| claimed_pairing(record, peer));
        let state = self
            .authority
            .pairing_status(record, peer_view.as_ref(), None)
            .state;
        let status = wire_status(state);
        // ⑤ 证明校验：本机仍持有该配对的 secret 时一律先验 proof（终态也一样），失败 → 401（§13.3）。
        //    secret 已被清除（§4.3：过期扫描、重启、首次认证成功后）时无法验 proof：此时只对**终态**
        //    回 200 + 该状态（§13.4：状态查询用 200 + `status=expired|consumed` 表达），非终态回 401。
        if self.authority.has_secret(&input.pairing) {
            if self.verify_status_proof(&input).is_err() {
                return proof_invalid();
            }
        } else if !matches!(
            state,
            PairingState::Expired | PairingState::Consumed | PairingState::Rejected
        ) {
            return proof_invalid();
        }
        // ⑥ 网络重试：同一 `requestNonce` 的请求返回原响应（§13.3）。缓存只在身份已判定后读取，
        //    因此 401 路径不会回出任何业务状态。
        let replay_key = input.replay_key();
        if let Some(body) = self.replay_body(&replay_key) {
            return ok_body(body);
        }
        // ⑦ 组装 200 响应（五状态一律 200）。`approved` 才带 `node`/`owner`（schema 的 if/then/else）。
        let Some(body) = self
            .status_body(record, status, view.node.as_ref(), &input)
            .await
        else {
            return internal_unavailable();
        };
        let Ok(bytes) = serde_json::to_vec(&body) else {
            return internal_unavailable();
        };
        let bytes = Bytes::from(bytes);
        self.remember_replay(replay_key, bytes.clone());
        secure(HttpResponse::json(StatusCode::OK, bytes))
    }

    /// 校验 `node-link-pairing-status/v1` 的 HMAC（§13.3）：密钥是本机内存里该配对的 secret。
    fn verify_status_proof(&self, input: &StatusInput) -> Result<(), identity_auth::PairingError> {
        let status = NodeLinkPairingStatus {
            owner_node_id: input.owner_node.clone(),
            access_node_id: input.access_node.clone(),
            pairing_id: input.pairing.clone(),
            pairing_request_id: input.pairing_request_id.clone(),
            request_nonce: input.request_nonce.clone(),
        };
        self.authority
            .verify_node_link_pairing_status(&status, &input.proof)
    }

    /// 组装 `statusResponse`（§13.3）：`approved` 带非秘密元数据 + 已授予的 `grant.*` + Owner identity，
    /// 不签发任何凭据；其余状态只带状态与过期时间。
    async fn status_body(
        &self,
        record: &PairingRecord,
        status: PairingStatus,
        node: Option<&NodeRecord>,
        input: &StatusInput,
    ) -> Option<StatusResponse> {
        let (node_block, owner_block) = if status == PairingStatus::Approved {
            let node = node?;
            let owner_public_key = self.authority.node_public_key().await.ok()?;
            let grants = node
                .grants()
                .iter()
                .map(GrantName::from_str)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            (
                Some(NodeBlock {
                    access_node_id: Uuid::parse(node.node_id().as_str()).ok()?,
                    name: NodeName::parse(node.display_name()).ok()?,
                    scopes: GrantList::new(grants).ok()?,
                }),
                Some(OwnerBlock {
                    owner_node_id: Uuid::parse(self.authority.local_node().as_str()).ok()?,
                    owner_public_key: Base64Url::<65>::parse(&encode_base64url(
                        owner_public_key.as_bytes(),
                    ))
                    .ok()?,
                }),
            )
        } else {
            (None, None)
        };
        // 请求标识取本机登记的材料（权威）；secret 已清除时（终态回退路径）回显请求里的值。
        let pairing_request_id = self
            .authority
            .pairing_request_material(&input.pairing)
            .map(|material| material.pairing_request_id)
            .unwrap_or_else(|| input.pairing_request_id.clone());
        Some(StatusResponse {
            protocol_version: ProtocolVersionV1::new(1).ok()?,
            pairing_request_id: Uuid::parse(pairing_request_id.as_str()).ok()?,
            status,
            expires_at: Timestamp::parse(record.expires_at().as_str()).ok()?,
            node: node_block,
            owner: owner_block,
        })
    }

    /// 该 `(pairingId, requestNonce)` 已缓存的原响应体。
    fn replay_body(&self, key: &str) -> Option<Bytes> {
        let replays = self
            .status_replays
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        replays.body(key)
    }

    /// 记住本次成功响应（§13.3 的重试语义；条目数有上限）。
    fn remember_replay(&self, key: String, body: Bytes) {
        let mut replays = self
            .status_replays
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        replays.remember(key, body);
    }

    /// claim 的 `endpoint` host 是否与 `daemon.public_origin` 一致（§13.1/§13.2）。
    ///
    /// 只比较 authority（`host[:port]`，与 `wss://<authority>/node-link/v1` 的推导同源）；逐字回显由
    /// `Authority::verify_claim` 的绑定判定负责。未配置或配置非法时一律不允许。
    fn endpoint_host_allowed(&self, endpoint: &str) -> bool {
        let Some(origin) = self.config.public_origin.as_deref() else {
            return false;
        };
        let Ok(origin) = CanonicalOrigin::parse(origin) else {
            return false;
        };
        let authority = origin.as_str().trim_start_matches("https://");
        NodeEndpoint::parse(endpoint).is_ok_and(|endpoint| endpoint.authority() == authority)
    }

    /// 用例层失败：一律按 500 失败关闭（内部不可用），不伪装成 401/409，也不转述端口文本。
    fn port_error(&self, operation: &str, pairing: &PairingId, error: PortError) -> HttpResponse {
        tracing::warn!(
            operation = operation,
            pairing_id = pairing.as_str(),
            error = ?error,
            "the pairing channel failed closed on a use-case error"
        );
        internal_unavailable()
    }

    /// 身份层失败：同上（错误分类只服务本地决策，对端只看到「内部不可用」）。
    fn identity_error(
        &self,
        operation: &str,
        pairing: &PairingId,
        error: impl std::fmt::Debug,
    ) -> HttpResponse {
        tracing::warn!(
            operation = operation,
            pairing_id = pairing.as_str(),
            error = ?error,
            "the pairing channel failed closed on an identity error"
        );
        internal_unavailable()
    }
}

/// claim 端点的 [`HttpHandler`]（薄包装：把请求交给 [`PairingHttp`]）。
struct ClaimEndpoint {
    pairing: Arc<PairingHttp>,
}

#[async_trait::async_trait]
impl HttpHandler for ClaimEndpoint {
    async fn handle(&self, request: HttpRequest) -> HttpResponse {
        self.pairing.claim(request).await
    }
}

/// status 端点的 [`HttpHandler`]（薄包装：把请求交给 [`PairingHttp`]）。
struct StatusEndpoint {
    pairing: Arc<PairingHttp>,
}

#[async_trait::async_trait]
impl HttpHandler for StatusEndpoint {
    async fn handle(&self, request: HttpRequest) -> HttpResponse {
        self.pairing.status(request).await
    }
}

/// wire 的 claim 请求 → 内部值对象。
///
/// wire 类型已经校验过形状；这里的失败只可能是「形状合法但取值不可用」（例如公钥不在 P-256 上），
/// 一律按 400 处理。`ownerNodeId` 不进本结构：证明 transcript 的 `ownerNodeId` 由状态机固定用
/// **本节点**标识装配（`verify_claim_proof`），因此声明了别的 owner 的 claim 必然验不过 → 401。
struct ClaimInput {
    pairing: PairingId,
    access_node: NodeId,
    kind: NodeKind,
    display_name: String,
    endpoint: String,
    public_key: PeerPublicKey,
    client_nonce: Nonce,
    proof: PairingProof,
}

impl ClaimInput {
    fn from_wire(claim: &ClaimRequest) -> Option<Self> {
        Some(Self {
            pairing: PairingId::new(claim.pairing_id.as_str()).ok()?,
            access_node: NodeId::new(claim.access_node_id.as_str()).ok()?,
            // §13.2 的 `nodeKind` 是常量 `access`（wire 类型 `AccessNodeKind` 已断言）。
            kind: NodeKind::Access,
            display_name: claim.node_name.as_str().to_owned(),
            endpoint: claim.endpoint.as_str().to_owned(),
            public_key: PeerPublicKey::try_from_bytes(claim.access_public_key.as_bytes()).ok()?,
            client_nonce: Nonce::new(&encode_base64url(claim.client_nonce.as_bytes())).ok()?,
            proof: PairingProof::try_from_bytes(claim.proof.as_bytes()).ok()?,
        })
    }
}

/// 持久化的对端行 → 状态机的对端视图（唯一用途：`verify_claim` 的幂等比对输入、状态视图的 SAS 入参）。
///
/// `requested` 取配对行登记的集合：与 [`ClaimFields`] 的构造同源（Node Link 的 claim 不带请求集合）。
fn claimed_pairing(record: &PairingRecord, peer: &PairingPeer) -> ClaimedPairing {
    ClaimedPairing {
        pairing: record.id().clone(),
        peer: peer.id().clone(),
        display_name: peer.display_name().to_owned(),
        public_key: peer.public_key().clone(),
        host_binding: peer.host_binding().to_owned(),
        client_nonce: peer.client_nonce().clone(),
        requested: RequestedCapabilities {
            scopes: record.requested_scopes().clone(),
            grants: record.requested_grants().clone(),
        },
    }
}

/// 组装 `claimResponse`（§13.2）。字段都来自已核对的事实；装配失败只可能是实现缺陷。
fn claim_response_body(
    record: &PairingRecord,
    material: &PairingRequestMaterial,
    signature: &P1363Signature,
) -> Option<ClaimResponse> {
    Some(ClaimResponse {
        protocol_version: ProtocolVersionV1::new(1).ok()?,
        pairing_request_id: Uuid::parse(material.pairing_request_id.as_str()).ok()?,
        server_nonce: Base64Url::<32>::parse(material.server_nonce.as_str()).ok()?,
        owner_proof: Base64Url::<64>::parse(&signature.to_base64url()).ok()?,
        status: PendingConfirmation,
        expires_at: Timestamp::parse(record.expires_at().as_str()).ok()?,
    })
}

/// 无填充 base64url 编码（wire 的 `Base64Url<32>` 没有文本访问器，而 core 的 `Nonce` 只接受文本）。
fn encode_base64url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 请求体解码（§13.4：JSON 或字段 schema 无效 → 400）。
///
/// 两类错误分开：语法层（含空体与截断）→ `protocol.invalid_json`；形状/取值域层（含
/// `deny_unknown_fields` 拒绝的未知键）→ `protocol.schema_invalid`。
fn decode_claim(body: &[u8]) -> Result<ClaimRequest, ErrorCode> {
    serde_json::from_slice::<ClaimRequest>(body).map_err(|error| match error.classify() {
        serde_json::error::Category::Syntax | serde_json::error::Category::Eof => {
            ErrorCode::ProtocolInvalidJson
        }
        serde_json::error::Category::Data | serde_json::error::Category::Io => {
            ErrorCode::ProtocolSchemaInvalid
        }
    })
}

/// status 请求体的解码（同 [`decode_claim`] 的两级错误分类）。
fn decode_status(body: &[u8]) -> Result<StatusRequest, ErrorCode> {
    serde_json::from_slice::<StatusRequest>(body).map_err(|error| match error.classify() {
        serde_json::error::Category::Syntax | serde_json::error::Category::Eof => {
            ErrorCode::ProtocolInvalidJson
        }
        serde_json::error::Category::Data | serde_json::error::Category::Io => {
            ErrorCode::ProtocolSchemaInvalid
        }
    })
}

/// wire 的 status 请求 → 内部值对象（失败只可能是「形状合法但取值不可用」）。
struct StatusInput {
    pairing: PairingId,
    owner_node: NodeId,
    access_node: NodeId,
    pairing_request_id: identity_auth::PairingRequestId,
    request_nonce: Nonce,
    proof: PairingProof,
}

impl StatusInput {
    fn from_wire(request: &StatusRequest) -> Option<Self> {
        Some(Self {
            pairing: PairingId::new(request.pairing_id.as_str()).ok()?,
            owner_node: NodeId::new(request.owner_node_id.as_str()).ok()?,
            access_node: NodeId::new(request.access_node_id.as_str()).ok()?,
            pairing_request_id: identity_auth::PairingRequestId::parse(
                request.pairing_request_id.as_str(),
            )
            .ok()?,
            request_nonce: Nonce::new(&encode_base64url(request.request_nonce.as_bytes())).ok()?,
            proof: PairingProof::try_from_bytes(request.proof.as_bytes()).ok()?,
        })
    }

    /// 重放缓存的键：`(pairingId, requestNonce)`（§13.3：只有网络重试才复用原 nonce）。
    fn replay_key(&self) -> String {
        format!("{}\0{}", self.pairing.as_str(), self.request_nonce.as_str())
    }
}

/// 状态机的配对状态 → wire 的 `PairingStatus`（§13.3 的五状态）。
///
/// `created`/`claimed` 在配对通道里没有对外名字：未完成本机确认的配对统一呈现为
/// `pending_confirmation`（与 `LOCAL_ADMIN_PROTOCOL.md` §5.4 的 `claimed` 同义），且 `created`
/// 的配对只有在 Access 自己拿到 secret 后才能被查询。
fn wire_status(state: PairingState) -> PairingStatus {
    match state {
        PairingState::Approved => PairingStatus::Approved,
        PairingState::Consumed => PairingStatus::Consumed,
        PairingState::Rejected => PairingStatus::Rejected,
        PairingState::Expired => PairingStatus::Expired,
        PairingState::Created | PairingState::Claimed | PairingState::PendingConfirmation => {
            PairingStatus::PendingConfirmation
        }
    }
}

/// 401（proof 无效或 secret 已不可用）：本端点的所有身份失败共用同一形状与文本（§13.3）。
fn proof_invalid() -> HttpResponse {
    reject(
        StatusCode::UNAUTHORIZED,
        ErrorCode::AuthProofInvalid,
        PROOF_INVALID_MESSAGE,
    )
}

/// 已缓存的 200 响应（重试路径原样重发首次响应体）。
fn ok_body(body: Bytes) -> HttpResponse {
    secure(HttpResponse::json(StatusCode::OK, body))
}

/// status 的「网络重试返回原响应」缓存（§13.3）。
///
/// 只保存成功响应；键是 `(pairingId, requestNonce)`，条目数有上限（先入先出淘汰最早的）。
#[derive(Default)]
struct StatusReplays {
    entries: VecDeque<(String, Bytes)>,
}

impl StatusReplays {
    fn body(&self, key: &str) -> Option<Bytes> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, body)| body.clone())
    }

    fn remember(&mut self, key: String, body: Bytes) {
        if self.entries.len() >= MAX_STATUS_REPLAYS {
            self.entries.pop_front();
        }
        self.entries.push_back((key, body));
    }
}

/// 按 `pairingId` 计数的滑动窗口（§2.5：status 60 次/分钟/`pairingId`）。
///
/// 与 `transport::net` 的 [`SlidingWindowLimiter`] 同构，但**键不是对端地址**：那一个的键固定为
/// `IpAddr`（`PeerInfo::client_ip` 是它的唯一来源），本端点的键是配对标识。两者都实现 §2.5 的固定窗口
/// 语义（含键集上限与失败关闭），各自有自己的边界用例。
struct PairingIdWindow {
    limit: u32,
    window: Duration,
    state: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl PairingIdWindow {
    fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            state: Mutex::new(HashMap::new()),
        }
    }

    fn check(&self, key: &str) -> RateLimit {
        self.check_at(key, Instant::now())
    }

    /// 与 [`PairingIdWindow::check`] 相同，但由调用方给出时刻（单测用）。
    fn check_at(&self, key: &str, now: Instant) -> RateLimit {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if self.limit == 0 {
            return RateLimit::Denied {
                retry_after: self.window,
            };
        }
        if !state.contains_key(key) && state.len() >= MAX_TRACKED_KEYS {
            state.retain(|_, attempts| {
                attempts
                    .back()
                    .is_some_and(|last| now.duration_since(*last) < self.window)
            });
            if state.len() >= MAX_TRACKED_KEYS {
                // 满了且没有可淘汰的空窗口：拒绝新键（失败关闭），不放弃对既有键的计数。
                return RateLimit::Denied {
                    retry_after: self.window,
                };
            }
        }
        let attempts = state.entry(key.to_owned()).or_default();
        while let Some(front) = attempts.front() {
            if now.duration_since(*front) < self.window {
                break;
            }
            attempts.pop_front();
        }
        let recorded = u32::try_from(attempts.len()).unwrap_or(u32::MAX);
        if recorded >= self.limit {
            let retry_after = attempts
                .front()
                .map(|front| self.window.saturating_sub(now.duration_since(*front)))
                .unwrap_or(self.window);
            return RateLimit::Denied { retry_after };
        }
        attempts.push_back(now);
        RateLimit::Allowed {
            remaining: self.limit - recorded - 1,
        }
    }

    /// 当前跟踪的键数（单测用）。
    #[cfg(test)]
    fn tracked_keys(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

/// `Content-Type` 是否为 `application/json`（允许 `; charset=…` 之类的参数，§13.1）。
fn is_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
}

/// 状态类拒绝（404/403/409/410）：HTTP 状态码是 §13.4 的区分点，`code` 仍是封闭词表里的
/// `nodelink.auth.proof_invalid`（Node Link v1 没有配对专属错误码，且 §13.4 对 409 明确要求该 code）。
fn not_claimable(status: StatusCode, message: &str) -> HttpResponse {
    reject(status, ErrorCode::AuthProofInvalid, message)
}

/// 限流拒绝（§2.5）：`retryAfterMs` 是 `nodelink.resource.rate_limited` 唯一登记的 `details` 字段。
fn rate_limited(retry_after: Duration) -> HttpResponse {
    let correlation = correlation_id();
    tracing::debug!(
        correlation_id = correlation.as_str(),
        retry_after_ms = retry_after.as_millis(),
        "node-link pairing attempt is rate limited"
    );
    let details = RawObject::parse(&format!(
        "{{\"retryAfterMs\":{}}}",
        retry_after.as_millis().min(u128::from(u32::MAX))
    ))
    .ok();
    error_response(
        StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::ResourceRateLimited,
        "pairing attempt is rate limited",
        &correlation,
        details,
    )
}

/// 内部不可用（500）：只用于实现缺陷或依赖不可用，不泄露任何内部细节。
fn internal_unavailable() -> HttpResponse {
    reject(
        StatusCode::INTERNAL_SERVER_ERROR,
        ErrorCode::InternalUnavailable,
        "the pairing channel is not available",
    )
}

/// 具名拒绝（§13.4 的结构化错误 body）。
fn reject(status: StatusCode, code: ErrorCode, message: &str) -> HttpResponse {
    let correlation = correlation_id();
    tracing::debug!(
        correlation_id = correlation.as_str(),
        status = status.as_u16(),
        code = code.as_str(),
        "node-link pairing request rejected"
    );
    error_response(status, code, message, &correlation, None)
}

/// 每次错误响应生成一个关联标识：日志与 body 用同一个值，便于把一次失败串起来。
fn correlation_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 组装 `httpError` body（`pairing.schema.json#/$defs/httpError`）并加上四个安全头。
///
/// `retryable` 一律取错误码在 registry 里登记的语义（`ErrorCode::default_retryable`），不自行发明。
fn error_response(
    status: StatusCode,
    code: ErrorCode,
    message: &str,
    correlation: &str,
    details: Option<RawObject>,
) -> HttpResponse {
    let body = ErrorBody::new(code, message, code.default_retryable())
        .or_else(|_| ErrorBody::new(code, code.as_str(), code.default_retryable()));
    let Ok(mut body) = body else {
        // 两条消息都是固定短文本，`Text<1024>` 不可能失败；仍然显式处理（正常路径不得 unwrap/expect）。
        return secure(HttpResponse::new(status));
    };
    body.correlation_id = Nullable::from_option(Uuid::parse(correlation).ok());
    body.details = details.unwrap_or_else(RawObject::empty);
    match serde_json::to_vec(&body) {
        Ok(bytes) => secure(HttpResponse::json(status, Bytes::from(bytes))),
        Err(error) => {
            tracing::error!(error = %error, "the pairing error body cannot be encoded");
            secure(HttpResponse::new(status))
        }
    }
}

/// 四个安全响应头（§13.1）：所有配对响应（成功与失败）都必须携带。
fn secure(response: HttpResponse) -> HttpResponse {
    response
        .with_header(
            HeaderName::from_static("cache-control"),
            HeaderValue::from_static("no-store"),
        )
        .with_header(
            HeaderName::from_static("pragma"),
            HeaderValue::from_static("no-cache"),
        )
        .with_header(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        )
        .with_header(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// status 的限流键是 `pairingId`（不是对端地址），窗口语义与接入层的 `SlidingWindowLimiter` 同口径。
    #[test]
    fn pairing_id_window_counts_per_key_and_recovers_after_the_window() {
        let window = PairingIdWindow::new(2, Duration::from_secs(60));
        let now = Instant::now();
        assert_eq!(
            window.check_at("pairing-a", now),
            RateLimit::Allowed { remaining: 1 }
        );
        assert_eq!(
            window.check_at("pairing-a", now),
            RateLimit::Allowed { remaining: 0 }
        );
        assert!(matches!(
            window.check_at("pairing-a", now),
            RateLimit::Denied { .. }
        ));
        // 另一个 pairingId 有自己的窗口，不受影响。
        assert_eq!(
            window.check_at("pairing-b", now),
            RateLimit::Allowed { remaining: 1 }
        );
        assert_eq!(window.tracked_keys(), 2);
        // 窗口滑过之后该键恢复可用。
        assert!(matches!(
            window.check_at("pairing-a", now + Duration::from_secs(60)),
            RateLimit::Allowed { .. }
        ));
    }

    /// 键数到上限后拒绝新键（失败关闭），既有键的计数不被重置。
    #[test]
    fn pairing_id_window_fails_closed_when_the_key_set_is_full() {
        let window = PairingIdWindow::new(60, Duration::from_secs(60));
        let now = Instant::now();
        for index in 0..MAX_TRACKED_KEYS {
            assert!(matches!(
                window.check_at(&format!("pairing-{index}"), now),
                RateLimit::Allowed { .. }
            ));
        }
        assert!(matches!(
            window.check_at("pairing-overflow", now),
            RateLimit::Denied { .. }
        ));
        assert_eq!(window.tracked_keys(), MAX_TRACKED_KEYS);
    }

    /// 五状态映射：`created`/`claimed` 对外都是 `pending_confirmation`（配对通道没有别的对外名字）。
    #[test]
    fn wire_status_covers_the_five_external_states() {
        assert_eq!(
            wire_status(PairingState::Created),
            PairingStatus::PendingConfirmation
        );
        assert_eq!(
            wire_status(PairingState::Claimed),
            PairingStatus::PendingConfirmation
        );
        assert_eq!(
            wire_status(PairingState::PendingConfirmation),
            PairingStatus::PendingConfirmation
        );
        assert_eq!(wire_status(PairingState::Approved), PairingStatus::Approved);
        assert_eq!(wire_status(PairingState::Rejected), PairingStatus::Rejected);
        assert_eq!(wire_status(PairingState::Expired), PairingStatus::Expired);
        assert_eq!(wire_status(PairingState::Consumed), PairingStatus::Consumed);
    }

    /// 重放缓存：同一键返回原响应，条目数有上限（超限淘汰最早的一条）。
    #[test]
    fn status_replays_are_bounded_and_keyed_by_pairing_and_nonce() {
        let mut replays = StatusReplays::default();
        replays.remember("a\0n1".to_owned(), Bytes::from_static(b"first"));
        assert_eq!(replays.body("a\0n1"), Some(Bytes::from_static(b"first")));
        assert_eq!(replays.body("a\0n2"), None);
        assert_eq!(replays.body("b\0n1"), None);
        for index in 0..MAX_STATUS_REPLAYS {
            replays.remember(format!("k\0{index}"), Bytes::from_static(b"x"));
        }
        assert!(replays.entries.len() <= MAX_STATUS_REPLAYS);
    }
}
