//! Sync 配对 HTTP 端点（`docs/SYNC_PROTOCOL.md` §7.1–§7.4，`design.md` D6）。
//!
//! 两个端点（`POST /sync/v1/pairing/claim`、`POST /sync/v1/pairing/status`）与 Node Link 的配对端点
//! （[`crate::node_link::pairing`]）**结构同形、类型各自定义**：`design.md` D6 要求不复用 wire DTO，
//! 因为两侧的 domain（`acp-remote/pairing-proof/v1` ↔ `node-link-pairing-proof/v1`）与载荷形状不同。
//! 因此这里的模块文档与那份逐条对应，但每个 DTO、每个校验入口都取自 `sync-protocol`/`identity-auth`。
//!
//! 边界纪律：
//!
//! - **状态机是唯一的内存态**：pairing secret、server nonce、request id 与失败计数只在 `identity-auth`
//!   的 `Authority` 里；配对行/对端行/设备行只经 `core::use_cases` 读（配对通道以绑定该配对的
//!   `Actor::PairingClaimant`），本模块不建第二份注册表、不查 SQLite、不 import `storage-sqlite`；
//! - **不调用 `server::node_link`**：两条配对路径只共享 `transport::net` 与 core/identity-auth 的公开面
//!   （`docs/MODULE_ARCHITECTURE.md` §4.9 的平级约束）；
//! - **认领方 actor 的构造顺序**（`docs/IDENTITY_AND_AUTH_CONTRACT.md` §5.1）：claim/status 路径允许先用
//!   绑定 actor 只读该配对自身的记录（HMAC 校验必须先拿到记录），但**写入**只发生在 proof 校验**通过
//!   之后**；这次读取零写入、不产生审计，也不向对端回任何记录内容。
//!
//! ## 原子性：先校验、后提交
//!
//! §7.2 要求服务端「原子检查 pairing 存在、未过期、仍为 `created`、Host/Origin 匹配和 HMAC 正确」。
//! 本模块的取法是**先校验、后提交**——五项检查全部在读取-判定阶段完成（存在/过期/终态在处理器里，
//! 状态/绑定/集合/HMAC 在 `Authority::verify_claim` 里），它们**一个字节都不写**；唯一的写入是
//! `UseCases::claim_pairing` 的**单个事务写集**（状态推进 + `pairing.claimed` 审计，`CORE_PORTS_AND_STORAGE.md`
//! §11.6），且只在五项检查全部通过后才调用。因此任一检查失败时配对状态、对端行与信任记录都不变
//! （`specs/sync-pairing-http/spec.md`「校验失败不写入状态」）。
//!
//! 证明失败会推进 `identity-auth` 的**内存**失败计数（§4.5/§14：第 5 次使配对失效）——那是内存态，
//! 不是配对状态或信任记录；失效后的持久状态仍由过期扫描终结。失效的**落库**（合同 §4.5 要求调用方
//! 提交一次拒绝落定）需要本机主体，本 HTTP 面不伪造 `Actor::LocalCli`，因此这里只做内存失效并把
//! 同一事实投影到 status（见 [`status_token`] 的说明）。
//!
//! ## 请求边界
//!
//! - **四个安全响应头**（§7.1）在所有响应上：处理器自己带（[`secure`]），并**必须**把
//!   [`PairingHttp::default_response_headers`] 交给 `NetListener::register_post`，否则接入层在调用
//!   处理器前产生的 413（请求体超限）、405（方法不匹配）与 Host 400 不带这四个头（`route.rs` 的
//!   每路径默认头语义）；
//! - **只接受同源 `application/json`**：`Content-Type` 必须是 `application/json`（允许参数）；请求的
//!   `Origin` 头必须存在、可归一化，并与该配对登记的 canonical origin **逐字**相等（§3.2「服务端必须
//!   校验 `Origin`，不能信任客户端 JSON 中自报的 Origin」——`Absent` 与不可归一化都按不允许处理，
//!   绝不以空字节串放行，与 WP3 对 `auth.client_hello` 的口径一致）；
//! - **不用 Cookie / HTTP Basic Auth / query 承载凭据**：本模块只读 `Content-Type`/`Origin` 与（归接入层
//!   判的）`Host`，凭据一律是 body 里的 HMAC/签名 proof；
//! - **HTTP `Host` 由接入层判**：虚拟主机边界是 `HostPolicy`（`daemon.allowed_hosts`/`daemon.public_origin`）
//!   的职责，在调用处理器**之前**已完成（§3.2 的「允许的 Host」）；本模块判的是协议身份字段
//!   （`Origin` 头、claim 的 `hostId`、claim 的 `canonicalOrigin`）。
//!
//! ## 限流（§14）
//!
//! claim 按**来源真实连接地址** 10 次/分钟（键 = `PeerInfo::client_ip`，复用
//! `transport::net::ratelimit::SlidingWindowLimiter`），status 按 **pairingId** 60 次/分钟（键不是地址，
//! 因此用 [`PairingIdWindow`]：`SlidingWindowLimiter` 的键固定为 `IpAddr`，无法承载配对标识；两者的
//! 窗口语义、键集上限与失败关闭口径相同）。转发头不参与计数：`client_ip` 只在 `daemon.trusted_proxies`
//! 内的对端提供的转发头**被采信**时才是头部取值，否则就是 TCP 对端地址（`transport::net::proxy`）。
//!
//! ## 错误映射（§7.4；`code` 一律取自 §12.2 的封闭词表）
//!
//! | HTTP | 场景 | `code` |
//! |---|---|---|
//! | 400 | JSON 语法错误 / 字段 schema 无效 / 非 `application/json` | `protocol.invalid_json`、`protocol.schema_invalid` |
//! | 401 | proof 无效、失败计数达上限、载荷与配对目标族不符 | `auth.proof_invalid` |
//! | 403 | 来源与登记绑定不一致 | `auth.origin_mismatch` |
//! | 403 | claim 声明的 `hostId` 不是本节点 | `auth.host_mismatch` |
//! | 404 | pairing 不存在 | `pairing.expired`（v1 没有「配对不存在」专用码，见 [`not_found_pairing`]） |
//! | 409 | 已被认领 / 状态不是 `created` / 已失效 | `pairing.already_claimed` |
//! | 410 | 已过期（含时钟越过 `expiresAt`） | `pairing.expired` |
//! | 410 | 已被首次认证消费 | `pairing.consumed` |
//! | 413 | 请求体超限（接入层） | —（接入层的纯文本响应，只保证带头） |
//! | 429 | 限流 | `resource.rate_limited`（`details.retryAfterMs`） |
//!
//! 401 的四个来源（proof 无效、失败计数达上限、载荷与配对目标族不符、secret 已不可用）**共用同一
//! code 与 message**：对端看不到校验差异（§4.5），细分原因只进结构化日志。
//!
//! ## 敏感信息边界（§7.1/§14/`SECURITY_DESIGN.md` §13.1/§14.1）
//!
//! pairing secret、HMAC/签名输入（transcript 字节）、proof 与完整认证载荷**不进**日志、错误消息、
//! 响应体或审计：本模块的日志只写操作名、配对标识、错误分类与状态码；错误 body 的 `message` 是固定
//! 短文本，从不插值请求内容；响应体只含 §7.2/§7.3 登记的非秘密字段。

use std::collections::HashMap;
use std::collections::VecDeque;
use std::str::FromStr as _;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use acp_core::model::{
    Actor, ConflictKind, DeviceId, NodeId, Nonce, PairingId, PairingPeer, PairingRecord,
    PairingState, PeerIdentity, PeerPublicKey, PortError,
};
use acp_core::use_cases::UseCases;
use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use identity_auth::{
    Authority, CanonicalOrigin, ClaimFields, ClaimKindFields, ClaimOutcome, ClaimRejection,
    ClaimedPairing, ClientKind, P1363Signature, PAIRING_MAX_FAILURES, PairingError, PairingProof,
    PairingRequestMaterial, RequestedCapabilities, SyncPairingHostProof, SyncPairingStatus,
};
use sync_protocol::auth::Scopes;
use sync_protocol::common::{
    Base64Url, NonEmptyText, Nullable, ProtocolVersionV1, RawObject, Timestamp, Uuid,
};
use sync_protocol::error::{Body as ErrorBody, ErrorCode};
use sync_protocol::pairing::{
    ClaimRequest, ClaimResponse, DeviceBlock, HostBlock, PairingStatus, PendingConfirmation,
    StatusRequest, StatusResponse,
};

use crate::transport::net::origin::request_origin;
use crate::transport::net::ratelimit::MAX_TRACKED_KEYS;
use crate::transport::net::{
    HttpHandler, HttpRequest, HttpResponse, RateLimit, SlidingWindowLimiter,
};

/// `POST /sync/v1/pairing/claim`（§7.2）。
pub const CLAIM_PATH: &str = "/sync/v1/pairing/claim";

/// `POST /sync/v1/pairing/status`（§7.3）。
pub const STATUS_PATH: &str = "/sync/v1/pairing/status";

/// claim 的固定限流（§14：10 次/分钟/来源地址，不可配置）。
const CLAIM_LIMIT_PER_MINUTE: u32 = 10;

/// status 的固定限流（§14：60 次/分钟/`pairingId`，不可配置）。
const STATUS_LIMIT_PER_MINUTE: u32 = 60;

/// 限流窗口（§14 的两条配对限流都以 1 分钟为窗口）。
const LIMIT_WINDOW: Duration = Duration::from_secs(60);

/// `(pairingId, requestNonce)` → 原响应的缓存条目上限（§7.3 的重试语义；超出后淘汰最早的一条）。
const MAX_STATUS_REPLAYS: usize = 1024;

/// 401 的统一消息（§7.4：响应不得泄露具体校验差异，因此证明/失败计数/目标族/secret 缺失共用同一文本）。
const PROOF_INVALID_MESSAGE: &str = "pairing proof is not valid";

/// 403 的统一消息（来源与登记绑定不一致）。
const ORIGIN_MISMATCH_MESSAGE: &str = "the request origin does not match the pairing binding";

/// 403 的消息（claim 声明的 `hostId` 不是本节点）。
const HOST_MISMATCH_MESSAGE: &str = "the request host identity does not match this host";

/// 四个安全响应头（§7.1）：所有配对 HTTP 响应（成功与失败）都必须携带。
///
/// 同一份常量被两条路径消费：处理器内的 [`secure`]（处理器自己带）与 `NetListener::register_post` 的
/// **每路径默认响应头**（接入层在调用处理器前产生的 413/405 与 Host 400 也带）——后者经
/// [`PairingHttp::default_response_headers`] 传入。两边同源，因此不会漂移，也不会变成重复头。
const SECURITY_HEADERS: [(&str, &str); 4] = [
    ("cache-control", "no-store"),
    ("pragma", "no-cache"),
    ("referrer-policy", "no-referrer"),
    ("x-content-type-options", "nosniff"),
];

/// 配对 HTTP 的共享状态（组合根装配一次，注册到两个 path）。
///
/// 不读配置文件、不持有端口快照：本面判定所需的一切都是「已提交的持久事实」（经 core 的用例面读）与
/// 「本机身份」（`Authority` 的只读入口）；`Host` 边界由接入层按 `NetConfig` 判完。
pub struct PairingHttp {
    core: Arc<UseCases>,
    authority: Arc<Authority>,
    /// claim 限流：键是 `PeerInfo::client_ip`（对端真实连接地址；不可信来源的转发头不生效）。
    claim_limit: SlidingWindowLimiter,
    /// status 限流：键是 `pairingId`（§14 的另一条固定限流）。
    status_limit: PairingIdWindow,
    /// status 的「网络重试返回原响应」缓存（§7.3）。
    status_replays: Mutex<StatusReplays>,
}

impl PairingHttp {
    /// 装配。`core` 是用例面，`authority` 是身份状态机（两者都由组合根持有）。
    pub fn new(core: Arc<UseCases>, authority: Arc<Authority>) -> Self {
        Self {
            core,
            authority,
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

    /// 两个配对 path 的每路径默认响应头（§7.1：**所有**配对 HTTP 响应都带这四个头）。
    ///
    /// 组合根注册 [`Self::claim_handler`]/[`Self::status_handler`] 时**必须**把它作为 `default_headers`
    /// 传给 `NetListener::register_post`。
    pub fn default_response_headers(&self) -> &'static [(&'static str, &'static str)] {
        &SECURITY_HEADERS
    }

    /// `POST /sync/v1/pairing/claim`（§7.2）：先校验（存在 → 未过期 → 仍可认领 → 来源与 host 身份 →
    /// HMAC），全部通过后才把配对推进到 `claimed`/`pending_confirmation` 并签发 host 证明。
    async fn claim(&self, request: HttpRequest) -> HttpResponse {
        // ① 限流（§14）：超限的请求不进入配对状态机。
        if let RateLimit::Denied { retry_after } = self.claim_limit.check(request.peer.client_ip) {
            return rate_limited(retry_after);
        }
        // ② 只接受同源 `application/json`（§7.1）。
        if !is_json_content_type(&request.headers) {
            return refuse(
                StatusCode::BAD_REQUEST,
                ErrorCode::ProtocolSchemaInvalid,
                "content type must be application/json",
            );
        }
        // ③ 解码：JSON 语法错误与字段/schema 错误都是 400（§7.4），但 code 分开。
        let claim = match decode_claim(&request.body) {
            Ok(claim) => claim,
            Err(code) => {
                return refuse(
                    StatusCode::BAD_REQUEST,
                    code,
                    "the request body is not a valid pairing claim",
                );
            }
        };
        let Some(input) = ClaimInput::from_wire(&claim) else {
            return refuse(
                StatusCode::BAD_REQUEST,
                ErrorCode::ProtocolSchemaInvalid,
                "the request body is not a valid pairing claim",
            );
        };

        // ④ 只读该配对自身的记录与对端行；claimant 绑定该配对，这次读取零写入（§5.1 的顺序规则）。
        let pairing = input.pairing.clone();
        let actor = Actor::PairingClaimant {
            pairing: pairing.clone(),
        };
        let view = match self.core.pairing_channel_view(&actor, &pairing).await {
            Ok(Some(view)) => view,
            // ⑤ 配对不存在 → 404（§7.4）。
            Ok(None) => return not_found_pairing(),
            Err(error) => return self.port_error("claim", &pairing, error),
        };
        let record = &view.record;
        // ⑥ 已 consumed → 410（§7.4 的 410 只用于 claim 端点）。
        if record.state() == PairingState::Consumed {
            return refuse(
                StatusCode::GONE,
                ErrorCode::PairingConsumed,
                "the pairing has already been used",
            );
        }
        // ⑦ 已过期 → 410（时钟越过 `expiresAt`，或已被过期扫描终结）。时间戳是定宽 RFC 3339 文本，
        //    字典序即时间序。
        if record.state() == PairingState::Expired
            || self.authority.now().as_str() >= record.expires_at().as_str()
        {
            return refuse(
                StatusCode::GONE,
                ErrorCode::PairingExpired,
                "the pairing has expired",
            );
        }
        // ⑧ 来源与 host 身份必须与该配对登记的绑定一致 → 403（§3.2/§7.2）。
        if let Err(code) = self.binding_check(record, &claim, &request.headers) {
            return refuse(StatusCode::FORBIDDEN, code, binding_message(code));
        }

        // ⑨ 结构 → 幂等 → 状态/失效 → 过期 → 绑定 → 集合 → HMAC 的唯一入口（`identity-auth`）。
        //    证明失败会推进内存失败计数；这一阶段仍然零写入。
        let fields = ClaimFields {
            pairing: input.pairing,
            peer: PeerIdentity::Device(input.device),
            kind: ClaimKindFields::Device {
                client_kind: input.client_kind,
            },
            display_name: input.display_name,
            host_binding: input.canonical_origin,
            public_key: input.public_key,
            client_nonce: input.client_nonce,
            // Sync 的 claim 载荷**没有**集合字段（§7.2；§7.0「PWA 不能在 claim 中申请或扩大 scopes」），
            // 因此请求集合恒取配对行登记的集合：幂等比对与「不得超出登记值」的判定因此同源。
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
            Err(error) => return self.identity_error("claim", &pairing, error),
        };
        let claimed = match outcome {
            ClaimOutcome::Claimed(claimed) => {
                // ⑩ 唯一的写入：状态推进 + `pairing.claimed` 审计在同一写集里（design D12），actor 绑定
                //    该配对。只有走到这里（五项检查全过）才会有任何持久变化。
                let claim = match claimed.to_claim() {
                    Ok(claim) => claim,
                    Err(error) => return self.identity_error("claim", &pairing, error),
                };
                match self.core.claim_pairing(&actor, claim).await {
                    Ok(_) => {}
                    // 同内容并发 claim 的极窄窗口：两个 `verify_claim` 都判 `Claimed`，后落库者撞上存储层
                    // 的 `AlreadyClaimed`/`IdentityMismatch` → 409。此分支自愈：客户端用**相同内容**再试一次
                    // 即被 `verify_claim` 识别为幂等重试（`Repeat`）并按原 pairing request 回 201。
                    Err(PortError::Conflict(
                        ConflictKind::AlreadyClaimed | ConflictKind::IdentityMismatch,
                    )) => {
                        return refuse(
                            StatusCode::CONFLICT,
                            ErrorCode::PairingAlreadyClaimed,
                            "the pairing has already been claimed",
                        );
                    }
                    Err(PortError::Conflict(ConflictKind::Expired)) => {
                        return refuse(
                            StatusCode::GONE,
                            ErrorCode::PairingExpired,
                            "the pairing has expired",
                        );
                    }
                    Err(PortError::NotFound(_)) => return not_found_pairing(),
                    Err(error) => return self.port_error("claim", &pairing, error),
                }
                claimed
            }
            // ⑪ 幂等重试：相同内容的重复 claim → 返回原 pairing request（同一 `pairingRequestId`/
            //    `serverNonce`），不创建第二条记录（§7.2/§7.4）。
            ClaimOutcome::Repeat(claimed) => claimed,
            ClaimOutcome::Rejected(rejection) => {
                let (status, code, message) = match rejection {
                    ClaimRejection::Expired => (
                        StatusCode::GONE,
                        ErrorCode::PairingExpired,
                        "the pairing has expired",
                    ),
                    // 已被认领、状态不是 `created`、或已被内存标记失效 → 409（§7.4 的「状态转换冲突」是
                    // `pairing.already_claimed` 的登记语义）。
                    ClaimRejection::NotClaimable => (
                        StatusCode::CONFLICT,
                        ErrorCode::PairingAlreadyClaimed,
                        "the pairing has already been claimed",
                    ),
                    ClaimRejection::BindingMismatch => (
                        StatusCode::FORBIDDEN,
                        ErrorCode::AuthOriginMismatch,
                        ORIGIN_MISMATCH_MESSAGE,
                    ),
                    // 集合越界在 Sync 面不可达（请求集合恒等于登记集合，见上面 `requested` 的说明）；仍然
                    // 具名映射而不是与证明失败混同，以免将来它变得可达时对外语义错位。
                    ClaimRejection::CapabilitiesExceedRegistered => (
                        StatusCode::FORBIDDEN,
                        ErrorCode::AuthorizationScopeDenied,
                        "the claim requests capabilities outside the registered set",
                    ),
                    // 结构（与目标族不符）、证明无效、失败计数达上限：统一 401，响应不区分是哪一个。
                    ClaimRejection::Malformed
                    | ClaimRejection::ProofInvalid
                    | ClaimRejection::TooManyFailures => (
                        StatusCode::UNAUTHORIZED,
                        ErrorCode::AuthProofInvalid,
                        PROOF_INVALID_MESSAGE,
                    ),
                };
                if rejection == ClaimRejection::TooManyFailures {
                    // 内存态已失效（`verify_claim` 的 `invalidate`），此后的 claim 一律 409，且即使 proof
                    // 完全正确也不再可认领。落库（合同 §4.5 的一次拒绝落定）需要本机主体，本 HTTP 面不伪造，
                    // 见模块文档「原子性」。
                    tracing::warn!(
                        operation = "claim",
                        pairing_id = pairing.as_str(),
                        "the pairing is invalidated after too many proof failures"
                    );
                }
                return refuse(status, code, message);
            }
        };
        self.claim_response(record, &claimed).await
    }

    /// claim 的成功响应（201 + `claimResponse`，§7.2）：`serverNonce`/`pairingRequestId` 来自本机为该配对
    /// 登记的材料（与 secret 同生同灭），`hostProof` 是本节点对 `pairing-host-proof` 域的签名。
    async fn claim_response(
        &self,
        record: &PairingRecord,
        claimed: &ClaimedPairing,
    ) -> HttpResponse {
        let Some(material) = self.authority.pairing_request_material(record.id()) else {
            // 材料与 secret 同生同灭（§4.3）。走到这里说明内存态与持久状态分歧（过期扫描/重启后的竞态或
            // 接线缺陷）：按「不可认领」失败关闭，绝不伪造 `pairingRequestId`。
            tracing::warn!(
                operation = "claim",
                pairing_id = record.id().as_str(),
                "claim material is gone while the pairing record is not terminal"
            );
            return refuse(
                StatusCode::CONFLICT,
                ErrorCode::PairingAlreadyClaimed,
                "the pairing has already been claimed",
            );
        };
        let PeerIdentity::Device(device) = &claimed.peer else {
            // 设备配对的 claimant 必然绑定设备身份（`Malformed` 已在上面拦掉别的目标族）。
            tracing::error!(
                operation = "claim",
                pairing_id = record.id().as_str(),
                "the claimed peer is not a device identity"
            );
            return internal_unavailable();
        };
        let Ok(canonical_origin) = CanonicalOrigin::parse(record.host_binding()) else {
            tracing::error!(
                operation = "claim",
                pairing_id = record.id().as_str(),
                "the pairing binding is not a canonical origin"
            );
            return internal_unavailable();
        };
        let host_public_key = match self.authority.node_public_key().await {
            Ok(key) => key,
            Err(error) => return self.identity_error("claim", record.id(), error),
        };
        let proof_input = SyncPairingHostProof {
            host_id: self.authority.local_node().clone(),
            device_id: device.clone(),
            pairing_id: record.id().clone(),
            canonical_origin,
            host_public_key,
            device_public_key: claimed.public_key.clone(),
            client_nonce: claimed.client_nonce.clone(),
            server_nonce: material.server_nonce.clone(),
            pairing_request_id: material.pairing_request_id.clone(),
        };
        let signature = match self
            .authority
            .sign_sync_pairing_host_proof(&proof_input)
            .await
        {
            Ok(signature) => signature,
            Err(error) => return self.identity_error("claim", record.id(), error),
        };
        let Some(body) = claim_response_body(record, &material, &signature) else {
            tracing::error!(
                operation = "claim",
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

    /// `POST /sync/v1/pairing/status`（§7.3）：校验 `pairing-status` 域的 HMAC 后**一律**返回 200，业务
    /// 状态只由 body 的 `status` 表达（`expired`/`consumed` 也不例外）。
    async fn status(&self, request: HttpRequest) -> HttpResponse {
        // ① 只接受同源 `application/json`（§7.1）。
        if !is_json_content_type(&request.headers) {
            return refuse(
                StatusCode::BAD_REQUEST,
                ErrorCode::ProtocolSchemaInvalid,
                "content type must be application/json",
            );
        }
        // ② 解码：JSON 语法错误与字段/schema 错误都是 400（§7.4）。
        let status_request = match decode_status(&request.body) {
            Ok(request) => request,
            Err(code) => {
                return refuse(
                    StatusCode::BAD_REQUEST,
                    code,
                    "the request body is not a valid pairing status query",
                );
            }
        };
        let Some(input) = StatusInput::from_wire(&status_request) else {
            return refuse(
                StatusCode::BAD_REQUEST,
                ErrorCode::ProtocolSchemaInvalid,
                "the request body is not a valid pairing status query",
            );
        };
        // ③ 限流（§14：60 次/分钟/`pairingId`）：超限的请求不进入配对状态机。
        if let RateLimit::Denied { retry_after } = self.status_limit.check(input.pairing.as_str()) {
            return rate_limited(retry_after);
        }
        // ④ 只读该配对自身（claimant 绑定该配对；与 claim 一样，这次读取不产生任何写入）。
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
        // ⑤ 来源必须与该配对登记的绑定一致 → 403（§7.1 的同源要求；与 claim 同一套判定，只判 `Origin`）。
        if let Err(code) = self.origin_check(record, &request.headers) {
            return refuse(StatusCode::FORBIDDEN, code, binding_message(code));
        }
        let peer_view = view.peer.as_ref().map(|peer| claimed_pairing(record, peer));
        let state = self
            .authority
            .pairing_status(record, peer_view.as_ref(), None)
            .state;
        let status = status_token(&self.authority, &input.pairing, state);
        // ⑥ 证明校验：本机仍持有该配对的 secret 时一律先验 proof（终态也一样），失败 → 401（§7.3）。
        //    secret 已被清除（§4.3：过期扫描、重启、首次认证成功后）时无法验 proof：此时只对**终态**
        //    回 200 + 该状态（§7.3：终态仍可轮询），非终态回 401。
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
        // ⑦ 网络重试：同一 `requestNonce` 的请求返回原响应（§7.3）。缓存只在身份已判定后读取，因此 401
        //    路径不会回出任何业务状态。
        let replay_key = input.replay_key();
        if let Some(body) = self.replay_body(&replay_key) {
            return ok_body(body);
        }
        // ⑧ 组装 200 响应（五状态一律 200）。`approved` 才带 `device`/`host`（schema 的 if/then/else）。
        let Some(body) = self
            .status_body(record, status, view.peer.as_ref(), &input)
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

    /// 校验 `acp-remote/pairing-status/v1` 的 HMAC（§7.3）：密钥是本机内存里该配对的 secret。
    ///
    /// `SecretUnavailable`（secret 已清除）与 `Proof(Hmac)`（HMAC 不匹配）是具名失败，但调用方把它们
    /// 收敛为同一个 401（[`proof_invalid`]），不泄露校验差异。
    fn verify_status_proof(&self, input: &StatusInput) -> Result<(), PairingError> {
        let status = SyncPairingStatus {
            host_id: input.host.clone(),
            device_id: input.device.clone(),
            pairing_id: input.pairing.clone(),
            pairing_request_id: input.pairing_request_id.clone(),
            request_nonce: input.request_nonce.clone(),
        };
        self.authority
            .verify_sync_pairing_status(&status, &input.proof)
    }

    /// 组装 `statusResponse`（§7.3）：`approved` 带设备的非秘密元数据（标识、展示名、已授予的 scope）与
    /// 本节点身份材料，不签发任何凭据；其余状态只带状态与过期时间。
    async fn status_body(
        &self,
        record: &PairingRecord,
        status: PairingStatus,
        peer: Option<&PairingPeer>,
        input: &StatusInput,
    ) -> Option<StatusResponse> {
        let (device_block, host_block) = if status.carries_device_and_host() {
            let peer = peer?;
            let PeerIdentity::Device(device_id) = peer.id() else {
                return None;
            };
            // 设备行是「授予的 scope」与展示名的唯一来源（`owned_device`，确认事务创建）；本入口是未认证
            // 的窄读面，不带 `require_local`（`design.md` D2）。
            let device = self
                .core
                .sync_device_handshake_view(device_id)
                .await
                .ok()?
                .device?;
            let host_public_key = self.authority.node_public_key().await.ok()?;
            (
                Some(DeviceBlock {
                    device_id: Uuid::parse(device.device_id().as_str()).ok()?,
                    name: NonEmptyText::parse(device.display_name()).ok()?,
                    scopes: wire_scopes(device.scopes())?,
                }),
                Some(HostBlock {
                    host_id: Uuid::parse(self.authority.local_node().as_str()).ok()?,
                    host_public_key: Base64Url::<65>::parse(&encode_base64url(
                        host_public_key.as_bytes(),
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
            device: device_block,
            host: host_block,
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

    /// 记住本次成功响应（§7.3 的重试语义；条目数有上限）。
    fn remember_replay(&self, key: String, body: Bytes) {
        let mut replays = self
            .status_replays
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        replays.remember(key, body);
    }

    /// 请求的 `Origin` 头是否与该配对登记的绑定一致（§3.2/§7.2）。
    fn origin_check(&self, record: &PairingRecord, headers: &HeaderMap) -> Result<(), ErrorCode> {
        let Ok(origin) = request_origin(headers) else {
            return Err(ErrorCode::AuthOriginMismatch);
        };
        let Some(origin) = origin.as_str() else {
            // 头缺席（`Absent`）：§3.2 要求服务端校验 `Origin`；设备身份绑定在 canonical origin 上，
            // 缺了它就没有可比较的绑定，因此与不可归一化同处置，绝不以空字节串放行。
            return Err(ErrorCode::AuthOriginMismatch);
        };
        if origin != record.host_binding() {
            return Err(ErrorCode::AuthOriginMismatch);
        }
        Ok(())
    }

    /// claim 的「来源与 Host 身份与配对绑定一致」（§7.2 的第四项校验）。
    ///
    /// 三条子判定，任一不成立即 403 且**在任何写入之前**：
    ///
    /// ① 请求的 `Origin` 头必须存在、可归一化，并与该配对登记的 canonical origin 逐字相等（[`Self::origin_check`]；
    ///    §3.2 不允许信任 body 里自报的 Origin）；
    /// ② claim 声明的 `hostId` 必须就是本节点——配对由本节点创建，登记在它自己的绑定上（别的 host 的
    ///    claim 即使 proof 正确也不属于本机）；
    /// ③ body 的 `canonicalOrigin` 必须与登记值逐字相等：这一条由 `Authority::verify_claim` 的
    ///    `BindingMismatch` 判定（绑定比较的唯一实现在状态机，本层不重复实现，见 §4.2 第 3 条）。
    fn binding_check(
        &self,
        record: &PairingRecord,
        claim: &ClaimRequest,
        headers: &HeaderMap,
    ) -> Result<(), ErrorCode> {
        self.origin_check(record, headers)?;
        if claim.host_id.as_str() != self.authority.local_node().as_str() {
            return Err(ErrorCode::AuthHostMismatch);
        }
        Ok(())
    }

    /// 用例层失败：一律按 500 失败关闭（内部不可用），不伪装成 4xx，也不转述端口文本。
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
/// wire 类型已经校验过形状；这里的失败只可能是「形状合法但取值不可用」（例如公钥不在 P-256 上），一律按
/// 400 处理。`hostId` **不进**本结构：证明 transcript 的 `hostId` 由状态机固定用**本节点**标识装配
/// （`verify_claim_proof`），因此声明了别的 host 的 claim 必然验不过；处理器另外把 `hostId` 与节点身份
/// 逐字比对（[`PairingHttp::binding_check`]），使「Host 与配对绑定一致」这一项有独立的失败分支。
struct ClaimInput {
    pairing: PairingId,
    device: DeviceId,
    client_kind: ClientKind,
    display_name: String,
    canonical_origin: String,
    public_key: PeerPublicKey,
    client_nonce: Nonce,
    proof: PairingProof,
}

impl ClaimInput {
    fn from_wire(claim: &ClaimRequest) -> Option<Self> {
        Some(Self {
            pairing: PairingId::new(claim.pairing_id.as_str()).ok()?,
            device: DeviceId::new(claim.device_id.as_str()).ok()?,
            client_kind: ClientKind::from_str(claim.client_kind.as_str()).ok()?,
            display_name: claim.device_name.as_str().to_owned(),
            canonical_origin: claim.canonical_origin.as_str().to_owned(),
            public_key: PeerPublicKey::try_from_bytes(claim.device_public_key.as_bytes()).ok()?,
            client_nonce: Nonce::new(&base64url_text(&claim.client_nonce)).ok()?,
            proof: PairingProof::try_from_bytes(claim.proof.as_bytes()).ok()?,
        })
    }
}

/// wire 的 status 请求 → 内部值对象（失败只可能是「形状合法但取值不可用」）。
struct StatusInput {
    host: NodeId,
    device: DeviceId,
    pairing: PairingId,
    pairing_request_id: identity_auth::PairingRequestId,
    request_nonce: Nonce,
    proof: PairingProof,
}

impl StatusInput {
    fn from_wire(request: &StatusRequest) -> Option<Self> {
        Some(Self {
            host: NodeId::new(request.host_id.as_str()).ok()?,
            device: DeviceId::new(request.device_id.as_str()).ok()?,
            pairing: PairingId::new(request.pairing_id.as_str()).ok()?,
            pairing_request_id: identity_auth::PairingRequestId::parse(
                request.pairing_request_id.as_str(),
            )
            .ok()?,
            request_nonce: Nonce::new(&base64url_text(&request.request_nonce)).ok()?,
            proof: PairingProof::try_from_bytes(request.proof.as_bytes()).ok()?,
        })
    }

    /// 重放缓存的键：`(pairingId, requestNonce)`（§7.3：只有网络重试才复用原 nonce）。
    fn replay_key(&self) -> String {
        format!("{}\0{}", self.pairing.as_str(), self.request_nonce.as_str())
    }
}

/// 持久化的对端行 → 状态机的对端视图（唯一用途：`verify_claim` 的幂等比对输入与状态视图的 SAS 入参）。
///
/// `requested` 取配对行登记的集合：与 [`ClaimFields`] 的构造同源。
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

/// 组装 `claimResponse`（§7.2）。字段都来自已核对的事实；装配失败只可能是实现缺陷。
fn claim_response_body(
    record: &PairingRecord,
    material: &PairingRequestMaterial,
    signature: &P1363Signature,
) -> Option<ClaimResponse> {
    Some(ClaimResponse {
        protocol_version: ProtocolVersionV1::new(1).ok()?,
        pairing_request_id: Uuid::parse(material.pairing_request_id.as_str()).ok()?,
        server_nonce: Base64Url::<32>::parse(material.server_nonce.as_str()).ok()?,
        host_proof: Base64Url::<64>::parse(&signature.to_base64url()).ok()?,
        status: PendingConfirmation,
        expires_at: Timestamp::parse(record.expires_at().as_str()).ok()?,
    })
}

/// 状态机的配对状态 → wire 的 `status`（§7.3 的五值封闭词表）。
///
/// `created`/`claimed` 在配对通道里没有对外名字：未完成本机确认的配对统一呈现为 `pending_confirmation`
/// （与 `LOCAL_ADMIN_PROTOCOL.md` §5.3 的 `claimed` 同义）。
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

/// 状态视图 → wire 的 `status`，含**失效投影**。
///
/// 失效（proof 失败累计 5 次，§14）由状态机记在内存里，持久状态仍是 `created`；v1 的五值词表没有
/// 「失效」取值，而合同 §4.5 要求把失效当作一次**拒绝**（「调用方 MUST 提交一次拒绝落定把失效持久化」），
/// 因此这里投影为 `rejected`：客户端据此立即停止轮询（§7.3「观察到 `rejected`、`expired` 或 `consumed`
/// 后停止轮询」），而不是继续轮询一个永远不会被批准的配对。判定与状态机同源——`failure_count` 是
/// [`PAIRING_MAX_FAILURES`] 的同一次写入结果（`verify_claim` 在计数达上限时 `invalidate`），两者不可能分歧。
///
/// 这正是 `docs/IDENTITY_AND_AUTH_CONTRACT.md` §5.1 登记的 `Authority::failure_count` 在其测试/诊断用途
/// 之外的**生产用途**（另一个是 `has_secret` 在本端点的 secret 生命周期分支）。
fn status_token(authority: &Authority, pairing: &PairingId, state: PairingState) -> PairingStatus {
    if authority.failure_count(pairing) >= PAIRING_MAX_FAILURES {
        return PairingStatus::Rejected;
    }
    wire_status(state)
}

/// 401（proof 无效、失败计数达上限、目标族不符，或 secret 已不可用）：本端点的所有身份失败共用同一形状
/// 与文本（§7.4）。
fn proof_invalid() -> HttpResponse {
    refuse(
        StatusCode::UNAUTHORIZED,
        ErrorCode::AuthProofInvalid,
        PROOF_INVALID_MESSAGE,
    )
}

/// 404（pairing 不存在，§7.4）。
///
/// v1 的封闭错误码表里没有「配对不存在」的专用码（§12.2 与 registry 只有
/// `pairing.already_claimed`/`pairing.expired`/`pairing.consumed`）：这里用 `pairing.expired`——从客户端
/// 的角度，「这个二维码已不可用」是同一个动作（重新生成二维码），且不在 404/410 的 HTTP 状态差异之外
/// 再加一层词汇差异。
fn not_found_pairing() -> HttpResponse {
    refuse(
        StatusCode::NOT_FOUND,
        ErrorCode::PairingExpired,
        "the pairing is not known or is no longer usable",
    )
}

/// 已缓存的 200 响应（重试路径原样重发首次响应体）。
fn ok_body(body: Bytes) -> HttpResponse {
    secure(HttpResponse::json(StatusCode::OK, body))
}

/// 403 的消息（`code` → 文本；两个 code 都只由 [`PairingHttp::origin_check`]/[`PairingHttp::binding_check`]
/// 产生）。
fn binding_message(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::AuthHostMismatch => HOST_MISMATCH_MESSAGE,
        _ => ORIGIN_MISMATCH_MESSAGE,
    }
}

/// status 的「网络重试返回原响应」缓存（§7.3）。
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

/// 按 `pairingId` 计数的滑动窗口（§14：status 60 次/分钟/`pairingId`）。
///
/// 与 `transport::net` 的 [`SlidingWindowLimiter`] 同构，但**键不是对端地址**：那一个的键固定为
/// `IpAddr`（`PeerInfo::client_ip` 是它的唯一来源），本端点的键是配对标识。两者都实现同一套固定窗口
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

/// 请求体解码（§7.4：JSON 或字段 schema 无效 → 400）。
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

/// `Content-Type` 是否为 `application/json`（允许 `; charset=…` 之类的参数，§7.1）。
fn is_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
}

/// 无填充 base64url 编码（wire 的 `Base64Url<32>` 没有文本访问器，而 core 的 `Nonce` 只接受文本）。
fn encode_base64url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// `Base64Url<BYTES>` 的解码后文本：`base64url` 只在解码期保留字节，重新编码得到与线上逐字相同的写法
/// （它能通过同一套规范校验），而 core 的 newtype（`Nonce`）只接受文本。
fn base64url_text<const BYTES: usize>(value: &Base64Url<BYTES>) -> String {
    encode_base64url(value.as_bytes())
}

/// `auth`/`status` 响应里的 `scopes` 形状。
///
/// `Scopes` 只提供反序列化这一条构造路径（它自己承担去重与非空文本校验），因此这里经一次 JSON 往返：
/// 失败即本机接线错误（例如 scope 名超过 128 字符），调用方按内部不可用失败关闭。
fn wire_scopes(scopes: &acp_core::model::ScopeSet) -> Option<Scopes> {
    let text = serde_json::to_string(&scopes.iter().collect::<Vec<&str>>()).ok()?;
    serde_json::from_str(&text).ok()
}

/// 限流拒绝（§14/§12.2）：`retryAfterMs` 是 `resource.rate_limited` 唯一登记的 `details` 字段。
fn rate_limited(retry_after: Duration) -> HttpResponse {
    let correlation = correlation_id();
    tracing::debug!(
        correlation_id = correlation.as_str(),
        retry_after_ms = retry_after.as_millis(),
        "sync pairing attempt is rate limited"
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
    refuse(
        StatusCode::INTERNAL_SERVER_ERROR,
        ErrorCode::InternalUnavailable,
        "the pairing channel is not available",
    )
}

/// 具名拒绝（§7.4 的结构化错误 body；`code` 一律取 §12.2 的封闭词表）。
fn refuse(status: StatusCode, code: ErrorCode, message: &str) -> HttpResponse {
    let correlation = correlation_id();
    tracing::debug!(
        correlation_id = correlation.as_str(),
        status = status.as_u16(),
        code = code.as_str(),
        "sync pairing request rejected"
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

/// 四个安全响应头（§7.1）：所有配对响应（成功与失败）都必须携带。
///
/// 与 [`PairingHttp::default_response_headers`] 同源（[`SECURITY_HEADERS`]）：接入层按「响应未带同名头
/// 才补齐」的语义附加，因此处理器返回的响应不会出现重复头。
fn secure(response: HttpResponse) -> HttpResponse {
    let mut response = response;
    for (name, value) in SECURITY_HEADERS {
        response = response.with_header(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::SocketAddr;
    use std::sync::Mutex as StdMutex;

    use acp_core::model::{PairingState, Timestamp as CoreTimestamp};
    use acp_core::ports::TrustStore as _;
    use base64::Engine as _;
    use serde_json::{Value, json};
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::TcpStream;
    use tokio::task::JoinHandle;
    use tracing::field::{Field, Visit};
    use tracing::level_filters::LevelFilter;
    use tracing::span::{Attributes, Id as SpanId, Record as SpanRecord};
    use tracing::{Event, Metadata, Subscriber};

    use crate::local_admin::envelope::{AdminOutcome, AdminRequest, AdminResponse, decode_request};
    use crate::local_admin::handler::LocalAdminHandler as _;
    use crate::local_admin::method::Method;
    use crate::local_admin::test_support::{
        FixedStore, TEST_PUBLIC_ORIGIN, TEST_SERVER_EPOCH, TestWorld, test_nonce, test_public_key,
    };
    use crate::transport::net::{NetConfig, NetError, NetListener, Shutdown, ShutdownHandle};
    use identity_auth::transcript::{SyncPairingProof, SyncPairingStatus as SyncStatusTranscript};
    use identity_auth::{PairingSecret, SyncPairingHostProof};
    use sync_protocol::pairing::CanonicalOrigin as WireOrigin;

    /// 本节点标识（与 `TestWorld` 装配的 `Authority` 同源）。
    const HOST: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
    /// 测试扮演的设备标识（`SYNC_PROTOCOL.md` §7.2 的 `deviceId`）。
    const DEVICE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
    /// 另一台设备（重复 claim 的「不同内容」用例）。
    const OTHER_DEVICE: &str = "9c8f6b1d-7a35-4f0b-9b6a-2f6d5c4e3b1a";
    /// 另一个来源（来源不一致的用例）。
    const OTHER_ORIGIN: &str = "https://evil.example.test";
    /// 单次 I/O 的超时（用例失败要快速失败，不挂住测试进程）。
    const IO_TIMEOUT: Duration = Duration::from_secs(10);
    /// 存储替身的固定水位（`sync_device_handshake_view` 会读 `head()`）。
    const STORE_HEAD_SEQUENCE: u64 = 7;
    /// 四个安全响应头（§7.1）。
    const SECURITY_HEADERS: [(&str, &str); 4] = [
        ("cache-control", "no-store"),
        ("pragma", "no-cache"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
    ];

    // ---------------------------------------------------------------------------------------------
    // 测试世界
    // ---------------------------------------------------------------------------------------------

    /// 一次已登记（`created`）的设备配对：`device.pair.begin` 的结果 + 二维码里的一次性 secret。
    struct Pairing {
        id: String,
        secret: PairingSecret,
        canonical_origin: String,
        expires_at: String,
    }

    impl Pairing {
        /// 同一个配对（同 id/secret/来源）但 `expiresAt` 不同：用于构造「proof 的 transcript 字段与权威
        /// 记录不一致」的失败分支（客户端拿错了二维码里的过期时刻）。
        fn with_expires_at(&self, expires_at: &str) -> Self {
            Self {
                id: self.id.clone(),
                secret: self.secret,
                canonical_origin: self.canonical_origin.clone(),
                expires_at: expires_at.to_owned(),
            }
        }
    }

    /// 端口替身世界 + 两个配对端点 + 真实 loopback listener。
    struct Harness {
        world: TestWorld,
        addr: SocketAddr,
        shutdown: ShutdownHandle,
        join: JoinHandle<Result<(), NetError>>,
    }

    impl Harness {
        /// 起一个已注册两个配对端点的接入层（`127.0.0.1:0`，端口由内核分配）。
        async fn start(world: TestWorld) -> Self {
            let pairing = Arc::new(PairingHttp::new(
                world.core.clone(),
                world.authority.clone(),
            ));
            let mut listener = NetListener::bind(NetConfig {
                listen: "127.0.0.1:0".to_owned(),
                ..NetConfig::default()
            })
            .await
            .expect("绑定 loopback listener");
            // 与组合根接线同口径：配对注册必须声明四个安全头（否则接入层预拒绝的 413/405/Host 400 不带）。
            let default_headers = pairing.default_response_headers();
            listener
                .register_post(CLAIM_PATH, pairing.claim_handler(), default_headers)
                .expect("注册 claim 端点");
            listener
                .register_post(STATUS_PATH, pairing.status_handler(), default_headers)
                .expect("注册 status 端点");
            let addr = listener.local_addrs()[0];
            let (shutdown, signal) = Shutdown::channel();
            let join = tokio::spawn(async move { listener.serve(signal).await });
            Self {
                world,
                addr,
                shutdown,
                join,
            }
        }

        /// 默认世界（存储替身带固定水位：`sync_device_handshake_view` 会读 `head()`）。
        async fn new() -> Self {
            Self::start(TestWorld::with_store(Arc::new(FixedStore::new(
                TEST_SERVER_EPOCH,
                STORE_HEAD_SEQUENCE,
            ))))
            .await
        }

        /// 结束接入层（用例末尾调用；端口随 listener 释放）。
        async fn stop(self) {
            self.shutdown.trigger();
            let _ = tokio::time::timeout(IO_TIMEOUT, self.join).await;
        }

        /// 本机用户登记一次设备配对（`device.pair.begin`），返回二维码里的公开字段与 secret。
        async fn begin(&self) -> Pairing {
            self.begin_with_window(json!(null)).await
        }

        /// 同上，但可指定 `expiresInMs`（过期路径用例用极短窗口）。
        async fn begin_with_window(&self, expires_in_ms: Value) -> Pairing {
            let response = self
                .world
                .router()
                .handle(admin_request(
                    Method::DevicePairBegin,
                    json!({
                        "requestedPacks": [],
                        "requestedScopes": ["session.list"],
                        "expiresInMs": expires_in_ms,
                    }),
                ))
                .await;
            let result = success(&response);
            let url = result["pairingUrl"].as_str().expect("pairingUrl");
            let payload: sync_protocol::pairing::QrPayload =
                serde_json::from_slice(&qr_payload(url)).expect("二维码 payload");
            Pairing {
                id: result["pairingId"].as_str().expect("pairingId").to_owned(),
                secret: PairingSecret::try_from_bytes(payload.pairing_secret.as_bytes())
                    .expect("32 字节 pairing secret"),
                canonical_origin: payload.canonical_origin.as_str().to_owned(),
                expires_at: payload.expires_at.as_str().to_owned(),
            }
        }

        /// 发送一条 claim 请求（`application/json` + 配对的来源）。
        async fn claim(&self, pairing: &Pairing, body: &[u8]) -> Response {
            self.post(
                CLAIM_PATH,
                &pairing.canonical_origin,
                body,
                Some("application/json"),
            )
            .await
        }

        /// 发送一条 status 请求（`application/json` + 配对的来源）。
        async fn status(&self, pairing: &Pairing, body: &[u8]) -> Response {
            self.post(
                STATUS_PATH,
                &pairing.canonical_origin,
                body,
                Some("application/json"),
            )
            .await
        }

        /// 发送一个 POST 请求到已注册的 path（带来源头）。
        async fn post(
            &self,
            path: &str,
            origin: &str,
            body: &[u8],
            content_type: Option<&str>,
        ) -> Response {
            let request = WireRequest {
                method: "POST",
                path,
                host: host_of(self.addr),
                origin: Some(origin.to_owned()),
                content_type: content_type.map(str::to_owned),
                forwarded_for: None,
                body: body.to_vec(),
            };
            send(self.addr, &request).await
        }

        /// 该配对的持久化记录（状态断言用）。
        fn record(&self, pairing: &Pairing) -> PairingRecord {
            self.world
                .trust
                .pairing(&pairing.id)
                .expect("配对必须已落库")
        }

        /// 本机用户确认该配对（`device.pair.confirm`，`approved`）。
        async fn approve(&self, pairing: &Pairing, scopes: Value) {
            let response = self
                .world
                .router()
                .handle(admin_request(
                    Method::DevicePairConfirm,
                    json!({"pairingId": pairing.id, "scopes": scopes}),
                ))
                .await;
            let _ = success(&response);
        }

        /// 本机用户拒绝该配对（`device.pair.reject`，`rejected`）。
        async fn reject(&self, pairing: &Pairing) {
            let response = self
                .world
                .router()
                .handle(admin_request(
                    Method::DevicePairReject,
                    json!({"pairingId": pairing.id, "reason": null}),
                ))
                .await;
            let _ = success(&response);
        }
    }

    // ---------------------------------------------------------------------------------------------
    // 请求 / 响应
    // ---------------------------------------------------------------------------------------------

    /// 一条待发送的 HTTP 请求（字段可参数化）。
    struct WireRequest<'a> {
        method: &'a str,
        path: &'a str,
        host: String,
        origin: Option<String>,
        content_type: Option<String>,
        forwarded_for: Option<String>,
        body: Vec<u8>,
    }

    /// 解析出的 HTTP 响应。
    struct Response {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    }

    impl Response {
        /// 按名字取响应头（大小写不敏感）。
        fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        }

        /// 响应体的 JSON（失败时给出可读的消息）。
        fn json(&self) -> Value {
            serde_json::from_slice(&self.body).unwrap_or_else(|error| {
                panic!(
                    "响应体不是 JSON（{error}）：{}",
                    String::from_utf8_lossy(&self.body)
                )
            })
        }

        /// 响应体的文本（断言「不出现 secret」用）。
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.body).into_owned()
        }

        /// 断言四个安全头齐全（§7.1）。
        fn assert_security_headers(&self) {
            for (name, expected) in SECURITY_HEADERS {
                assert_eq!(
                    self.header(name),
                    Some(expected),
                    "配对响应（{}）必须携带 {name}: {expected}",
                    self.status
                );
            }
        }
    }

    /// 发一条 HTTP/1.1 请求并读回一个响应。
    async fn send(addr: SocketAddr, request: &WireRequest<'_>) -> Response {
        let mut stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
            .await
            .expect("连接超时")
            .expect("连接 loopback");
        let mut raw = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
            request.method, request.path, request.host
        )
        .into_bytes();
        if let Some(origin) = &request.origin {
            raw.extend_from_slice(format!("Origin: {origin}\r\n").as_bytes());
        }
        if let Some(content_type) = &request.content_type {
            raw.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
        }
        if let Some(forwarded_for) = &request.forwarded_for {
            raw.extend_from_slice(format!("X-Forwarded-For: {forwarded_for}\r\n").as_bytes());
        }
        raw.extend_from_slice(format!("Content-Length: {}\r\n\r\n", request.body.len()).as_bytes());
        raw.extend_from_slice(&request.body);
        // 写失败被容忍（413 这类响应会在服务端提前关闭连接时让写入报 `EPIPE`）。
        let _ = tokio::time::timeout(IO_TIMEOUT, stream.write_all(&raw)).await;

        let mut response = Vec::new();
        let mut chunk = [0u8; 16 * 1024];
        loop {
            if let Some(head_end) = find_subsequence(&response, b"\r\n\r\n") {
                if response.len() >= head_end + 4 + content_length(&response[..head_end]) {
                    break;
                }
            }
            let read = tokio::time::timeout(IO_TIMEOUT, stream.read(&mut chunk))
                .await
                .expect("读响应超时")
                .expect("读响应");
            if read == 0 {
                break;
            }
            response.extend_from_slice(&chunk[..read]);
        }
        parse_response(&response)
    }

    /// 解析 HTTP/1.1 响应（状态行 + 头 + 按 `Content-Length` 的体）。
    fn parse_response(raw: &[u8]) -> Response {
        let head_end = find_subsequence(raw, b"\r\n\r\n").unwrap_or_else(|| {
            panic!("响应缺少 header 结束标记：{}", String::from_utf8_lossy(raw))
        });
        let head = String::from_utf8_lossy(&raw[..head_end]);
        let mut lines = head.split("\r\n");
        let status_line = lines.next().unwrap_or_default();
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .unwrap_or_else(|| panic!("状态行缺少状态码：{status_line}"));
        let headers = lines
            .filter_map(|line| {
                line.split_once(':').map(|(name, value)| {
                    (name.trim().to_ascii_lowercase(), value.trim().to_owned())
                })
            })
            .collect();
        let length = content_length(&raw[..head_end]);
        let body = raw
            .get(head_end + 4..head_end + 4 + length)
            .unwrap_or_default()
            .to_vec();
        Response {
            status,
            headers,
            body,
        }
    }

    /// `Content-Length`（缺失或非法按 0 处理：本接入层的有体响应都带该头）。
    fn content_length(head: &[u8]) -> usize {
        let text = String::from_utf8_lossy(head);
        text.split("\r\n")
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
            .unwrap_or(0)
    }

    fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    /// `Host` 头取值（默认配置只接受 loopback 形态）。
    fn host_of(addr: SocketAddr) -> String {
        addr.to_string()
    }

    // ---------------------------------------------------------------------------------------------
    // 配对与请求的构造（测试扮演设备）
    // ---------------------------------------------------------------------------------------------

    /// 本地管理响应的成功结果（失败时给出可读的消息）。
    fn success(response: &AdminResponse) -> Value {
        match response.outcome() {
            AdminOutcome::Success { result } => Value::Object(result.clone()),
            AdminOutcome::Failure { error } => panic!(
                "期望成功响应，实际 {}：{}",
                error.code().as_str(),
                error.message()
            ),
        }
    }

    fn admin_request(method: Method, params: Value) -> AdminRequest {
        let bytes = serde_json::to_vec(&json!({
            "v": 1,
            "id": "2ae1c07c-0000-4000-8000-0000000000ff",
            "method": method.as_str(),
            "params": params,
        }))
        .expect("请求信封可序列化");
        decode_request(&bytes).expect("信封合法")
    }

    /// 二维码 URL 的 fragment（`#data=<无填充 base64url(json)>`）。
    fn qr_payload(url: &str) -> Vec<u8> {
        let data = url.split("#data=").nth(1).expect("URL 带 data fragment");
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(data)
            .expect("fragment 是无填充 base64url")
    }

    fn uuid(text: &str) -> Uuid {
        Uuid::parse(text).expect("测试用 UUID 文本必须规范")
    }

    fn node(text: &str) -> NodeId {
        NodeId::new(text).expect("测试用 NodeId 文本必须规范")
    }

    fn device(text: &str) -> DeviceId {
        DeviceId::new(text).expect("测试用 DeviceId 文本必须规范")
    }

    fn pairing_id(text: &str) -> PairingId {
        PairingId::new(text).expect("测试用 PairingId 文本必须规范")
    }

    fn request_id(text: &str) -> identity_auth::PairingRequestId {
        identity_auth::PairingRequestId::parse(text).expect("测试用 request id 必须规范")
    }

    /// 32 字节 nonce 的规范无填充 base64url 文本（末字节的低 2 位固定为 0）。
    fn nonce_text(seed: u8) -> String {
        let mut bytes = [seed; 32];
        bytes[31] &= 0b1111_1100;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }

    /// 第 `index` 个不同的 32 字节 nonce（避免重试缓存命中）。
    fn nonce_for_index(index: usize) -> String {
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&u64::try_from(index).expect("小整数").to_be_bytes());
        bytes[31] &= 0b1111_1100;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }

    /// 测试扮演的设备公钥（基点 G 的 SEC1 未压缩编码）。
    fn device_public_key() -> PeerPublicKey {
        test_public_key()
    }

    fn wire_public_key(key: &PeerPublicKey) -> Base64Url<65> {
        Base64Url::parse(&encode_base64url(key.as_bytes())).expect("65 字节 SEC1 公钥的 base64url")
    }

    fn wire_nonce(text: &str) -> Base64Url<32> {
        Base64Url::parse(text).expect("32 字节 nonce 的 base64url")
    }

    fn wire_proof(proof: &identity_auth::PairingProof) -> Base64Url<32> {
        Base64Url::parse(&encode_base64url(proof.as_bytes())).expect("32 字节 proof 的 base64url")
    }

    /// 一次 claim 的字段（用例按需改单个字段表达失败分支）。
    struct ClaimParts {
        device: String,
        nonce: String,
        device_name: String,
        canonical_origin: String,
        host_id: String,
        public_key: PeerPublicKey,
    }

    impl ClaimParts {
        /// 一次内容完全合法的 claim（来源、host、绑定、proof 都对）。
        fn valid(pairing: &Pairing, nonce_seed: u8) -> Self {
            Self {
                device: DEVICE.to_owned(),
                nonce: nonce_text(nonce_seed),
                device_name: "Zhang's Phone".to_owned(),
                canonical_origin: pairing.canonical_origin.clone(),
                host_id: HOST.to_owned(),
                public_key: device_public_key(),
            }
        }

        /// proof 用二维码里的 secret 对 `acp-remote/pairing-proof/v1` 计算（设备侧的行为）。
        fn request(&self, pairing: &Pairing) -> ClaimRequest {
            let client_nonce = Nonce::new(&self.nonce).expect("规范 nonce");
            let proof = SyncPairingProof {
                host_id: node(&self.host_id),
                device_id: device(&self.device),
                pairing_id: pairing_id(&pairing.id),
                pairing_expires_at: CoreTimestamp::new(&pairing.expires_at).expect("规范时间戳"),
                canonical_origin: CanonicalOrigin::parse(&self.canonical_origin)
                    .expect("测试用 origin 必须规范"),
                device_public_key: self.public_key.clone(),
                client_nonce: client_nonce.clone(),
                device_name: self.device_name.clone(),
                client_kind: ClientKind::Pwa,
            }
            .hmac(&pairing.secret)
            .expect("证明必须可计算");
            ClaimRequest {
                protocol_version: ProtocolVersionV1::new(1).expect("v1"),
                pairing_id: uuid(&pairing.id),
                host_id: uuid(&self.host_id),
                device_id: uuid(&self.device),
                device_name: NonEmptyText::parse(&self.device_name).expect("设备名合法"),
                client_kind: sync_protocol::auth::ClientKind::Pwa,
                canonical_origin: WireOrigin::parse(&self.canonical_origin)
                    .expect("测试用 origin 必须规范"),
                device_public_key: wire_public_key(&self.public_key),
                client_nonce: wire_nonce(&self.nonce),
                proof: wire_proof(&proof),
            }
        }

        fn body(&self, pairing: &Pairing) -> Vec<u8> {
            serde_json::to_vec(&self.request(pairing)).expect("claim 请求可序列化")
        }
    }

    /// 一次合法 claim 的请求体。
    fn claim_body(pairing: &Pairing, nonce_seed: u8) -> Vec<u8> {
        ClaimParts::valid(pairing, nonce_seed).body(pairing)
    }

    /// 写入一个 `StatusParts` 的合法值。
    struct StatusParts {
        pairing_request_id: String,
        nonce: String,
    }

    impl StatusParts {
        /// 用二维码里的 secret 对 `acp-remote/pairing-status/v1` 计算 proof。
        fn request(&self, pairing: &Pairing) -> StatusRequest {
            let request_nonce = Nonce::new(&self.nonce).expect("规范 nonce");
            let proof = SyncStatusTranscript {
                host_id: node(HOST),
                device_id: device(DEVICE),
                pairing_id: pairing_id(&pairing.id),
                pairing_request_id: request_id(&self.pairing_request_id),
                request_nonce: request_nonce.clone(),
            }
            .hmac(&pairing.secret)
            .expect("证明必须可计算");
            StatusRequest {
                protocol_version: ProtocolVersionV1::new(1).expect("v1"),
                host_id: uuid(HOST),
                device_id: uuid(DEVICE),
                pairing_id: uuid(&pairing.id),
                pairing_request_id: uuid(&self.pairing_request_id),
                request_nonce: wire_nonce(request_nonce.as_str()),
                proof: wire_proof(&proof),
            }
        }

        fn body(&self, pairing: &Pairing) -> Vec<u8> {
            serde_json::to_vec(&self.request(pairing)).expect("status 请求可序列化")
        }
    }

    /// 用真实路径 claim 一次并把 `pairingRequestId` 回带（status 请求必须带同一个值）。
    async fn claimed(harness: &Harness, pairing: &Pairing, nonce_seed: u8) -> String {
        let response = harness
            .claim(pairing, &claim_body(pairing, nonce_seed))
            .await;
        assert_eq!(response.status, 201, "claim 必须成功");
        response.json()["pairingRequestId"]
            .as_str()
            .expect("pairingRequestId")
            .to_owned()
    }

    /// 以有效 proof 查询一次状态。
    async fn query_status(
        harness: &Harness,
        pairing: &Pairing,
        request_id: &str,
        nonce: &str,
    ) -> Response {
        let parts = StatusParts {
            pairing_request_id: request_id.to_owned(),
            nonce: nonce.to_owned(),
        };
        harness.status(pairing, &parts.body(pairing)).await
    }

    /// 断言响应里的 `hostProof` 可由二维码里的 `hostPublicKey` 验证（§7.2；测试世界的固定私钥对应
    /// `test_public_key()`）。
    fn assert_host_proof_verifies(pairing: &Pairing, body: &Value, device_text: &str, nonce: &str) {
        let key = device_public_key();
        let signature = P1363Signature::try_from_base64url(
            body["hostProof"].as_str().expect("hostProof 是字符串"),
        )
        .expect("hostProof 是 64 字节 P1363");
        let input = SyncPairingHostProof {
            host_id: node(HOST),
            device_id: device(device_text),
            pairing_id: pairing_id(&pairing.id),
            canonical_origin: CanonicalOrigin::parse(&pairing.canonical_origin)
                .expect("测试用 origin 必须规范"),
            host_public_key: key.clone(),
            device_public_key: key.clone(),
            client_nonce: Nonce::new(nonce).expect("规范 nonce"),
            server_nonce: Nonce::new(body["serverNonce"].as_str().expect("serverNonce"))
                .expect("serverNonce 是规范 nonce"),
            pairing_request_id: request_id(
                body["pairingRequestId"].as_str().expect("pairingRequestId"),
            ),
        };
        input
            .verify(&key, &signature)
            .expect("hostProof 必须由二维码里的 hostPublicKey 验证通过");
    }

    /// 错误 body 的稳定部分（`correlationId` 每次请求不同，不参与比较）。
    fn error_shape(response: &Response) -> Value {
        let body = response.json();
        json!({
            "code": body["code"],
            "message": body["message"],
            "retryable": body["retryable"],
            "details": body["details"],
        })
    }

    /// 断言失败路径没有改变任何持久状态（配对行与设备信任记录）。
    fn assert_untouched(harness: &Harness, pairing: &Pairing, expected_state: PairingState) {
        assert_eq!(
            harness.record(pairing).state(),
            expected_state,
            "失败路径不得改变配对状态"
        );
        assert_eq!(
            harness.world.trust.device_count(),
            0,
            "失败路径不得创建任何信任记录"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // 5.1 / 5.2：claim 的原子校验与幂等
    // ---------------------------------------------------------------------------------------------

    #[tokio::test]
    async fn claim_enters_pending_confirmation_and_returns_a_verifiable_host_proof() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        let nonce = nonce_text(0x01);

        let response = harness.claim(&pairing, &claim_body(&pairing, 0x01)).await;
        assert_eq!(response.status, 201, "首次 claim 成功返回 201（§7.4）");
        assert_eq!(
            response.header("content-type").map(str::to_owned),
            Some("application/json".to_owned())
        );
        response.assert_security_headers();
        let body = response.json();
        assert_eq!(body["protocolVersion"], json!(1));
        assert_eq!(body["status"], json!("pending_confirmation"));
        assert_eq!(body["expiresAt"], json!(pairing.expires_at));
        assert_host_proof_verifies(&pairing, &body, DEVICE, &nonce);

        // 服务器签发的 `serverNonce`/`pairingRequestId` 就是本机内存里的配对材料。
        let material = harness
            .world
            .authority
            .pairing_request_material(&pairing_id(&pairing.id))
            .expect("claim 之后仍有配对材料");
        assert_eq!(
            body["pairingRequestId"].as_str().expect("pairingRequestId"),
            material.pairing_request_id.as_str()
        );
        assert_eq!(
            body["serverNonce"].as_str().expect("serverNonce"),
            material.server_nonce.as_str()
        );

        // 配对推进到 `pending_confirmation`，对端行已固定；仍然没有设备信任记录（确认只走本机入口）。
        assert_eq!(
            harness.record(&pairing).state(),
            PairingState::PendingConfirmation
        );
        let peer = harness
            .world
            .trust
            .pairing_peer(&pairing_id(&pairing.id))
            .await
            .expect("读对端行")
            .expect("认领后必须有对端行");
        assert_eq!(peer.id(), &PeerIdentity::Device(device(DEVICE)));
        assert_eq!(
            harness.world.trust.device_count(),
            0,
            "claim 本身不创建信任记录（§7.0）"
        );
        harness.stop().await;
    }

    /// 5.1 的第四项校验（来源与登记绑定一致）——来源头不对。
    #[tokio::test]
    async fn claim_from_another_origin_is_forbidden_and_writes_nothing() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        let body = claim_body(&pairing, 0x02);

        // ① 另一个 https 来源 → 403。
        let other = harness
            .post(CLAIM_PATH, OTHER_ORIGIN, &body, Some("application/json"))
            .await;
        assert_eq!(other.status, 403, "来源不一致必须 403（§7.4）");
        other.assert_security_headers();
        assert_eq!(other.json()["code"], json!("auth.origin_mismatch"));
        assert_untouched(&harness, &pairing, PairingState::Created);

        // ② 完全没有 `Origin` 头（非浏览器形态）→ 同样拒绝，不以空字节串放行。
        let absent = send(
            harness.addr,
            &WireRequest {
                method: "POST",
                path: CLAIM_PATH,
                host: host_of(harness.addr),
                origin: None,
                content_type: Some("application/json".to_owned()),
                forwarded_for: None,
                body: body.clone(),
            },
        )
        .await;
        assert_eq!(absent.status, 403);
        assert_eq!(absent.json()["code"], json!("auth.origin_mismatch"));
        assert_untouched(&harness, &pairing, PairingState::Created);

        // ③ 头存在但不可归一化（`Origin: null`）→ 失败关闭。
        let malformed = harness
            .post(CLAIM_PATH, "null", &body, Some("application/json"))
            .await;
        assert_eq!(malformed.status, 403);
        assert_eq!(malformed.json()["code"], json!("auth.origin_mismatch"));
        assert_untouched(&harness, &pairing, PairingState::Created);
        harness.stop().await;
    }

    /// 5.1 的第四项校验——claim 声明的 `hostId` 不是本节点。
    #[tokio::test]
    async fn claim_declaring_another_host_is_forbidden_and_writes_nothing() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        let mut parts = ClaimParts::valid(&pairing, 0x03);
        parts.host_id = OTHER_DEVICE.to_owned();
        let response = harness.claim(&pairing, &parts.body(&pairing)).await;
        assert_eq!(response.status, 403);
        response.assert_security_headers();
        assert_eq!(response.json()["code"], json!("auth.host_mismatch"));
        assert_untouched(&harness, &pairing, PairingState::Created);
        harness.stop().await;
    }

    /// 5.1 的第四项校验——claim 声明的 `canonicalOrigin` 与登记绑定不一致（由状态机的绑定比较判定）。
    #[tokio::test]
    async fn claim_declaring_another_binding_is_forbidden_and_writes_nothing() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        let mut parts = ClaimParts::valid(&pairing, 0x04);
        parts.canonical_origin = OTHER_ORIGIN.to_owned();
        let response = harness.claim(&pairing, &parts.body(&pairing)).await;
        assert_eq!(response.status, 403, "绑定不一致必须 403（§7.4）");
        assert_eq!(response.json()["code"], json!("auth.origin_mismatch"));
        assert_untouched(&harness, &pairing, PairingState::Created);
        harness.stop().await;
    }

    /// 5.1 的第一项校验——pairing 不存在。
    #[tokio::test]
    async fn unknown_pairing_is_rejected_with_404_and_writes_nothing() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        // 一个格式合法但本机从未登记的 pairingId（这里沿用它自己的 secret 也无所谓：存在性判定先于
        // proof，返回 404 之前根本不看证明）。
        let unknown = Pairing {
            id: "a1b2c3d4-0000-4000-8000-0000000000fe".to_owned(),
            secret: pairing.secret,
            canonical_origin: pairing.canonical_origin.clone(),
            expires_at: pairing.expires_at.clone(),
        };
        let response = harness
            .claim(&unknown, &ClaimParts::valid(&unknown, 0x05).body(&unknown))
            .await;
        assert_eq!(response.status, 404, "未知 pairingId 必须 404（§7.4）");
        response.assert_security_headers();
        assert_eq!(response.json()["code"], json!("pairing.expired"));
        assert_eq!(harness.world.trust.pairing_count(), 1, "不创建第二条配对");
        assert_eq!(
            harness.record(&pairing).state(),
            PairingState::Created,
            "别的配对的记录不受影响"
        );
        assert_eq!(harness.world.trust.device_count(), 0, "不创建信任记录");
        harness.stop().await;
    }

    /// 5.1 的第二项校验——pairing 已过期。
    #[tokio::test]
    async fn expired_pairing_is_rejected_with_410_and_writes_nothing() {
        let harness = Harness::new().await;
        // 极短窗口（1 毫秒）+ 推进注入时钟：过期判定以本机时钟为权威（§7.0）。
        let pairing = harness.begin_with_window(json!(1)).await;
        harness.world.clock.set("2026-09-18T09:14:00.000Z");
        let expired = harness.record(&pairing);
        assert_eq!(expired.state(), PairingState::Created);

        let response = harness.claim(&pairing, &claim_body(&pairing, 0x06)).await;
        assert_eq!(response.status, 410, "过期配对必须 410（§7.4）");
        response.assert_security_headers();
        assert_eq!(response.json()["code"], json!("pairing.expired"));
        assert_untouched(&harness, &pairing, PairingState::Created);
        harness.stop().await;
    }

    /// 5.1 的第三项校验（仍为 `created`）与 5.2 的两侧——重复 claim。
    #[tokio::test]
    async fn repeated_claim_is_idempotent_for_the_same_payload_and_conflicting_otherwise() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        let body = claim_body(&pairing, 0x07);

        let first = harness.claim(&pairing, &body).await;
        assert_eq!(first.status, 201);
        let request_id = first.json()["pairingRequestId"].clone();
        let server_nonce = first.json()["serverNonce"].clone();

        // ① 内容完全相同的重试 → 幂等：原 pairing request，不建第二条记录（§7.2）。
        let repeat = harness.claim(&pairing, &body).await;
        assert_eq!(repeat.status, 201, "相同内容的重试返回首次结果");
        repeat.assert_security_headers();
        assert_eq!(repeat.json()["pairingRequestId"], request_id);
        assert_eq!(repeat.json()["serverNonce"], server_nonce);
        assert_eq!(harness.world.trust.pairing_count(), 1);
        assert_eq!(
            harness.record(&pairing).state(),
            PairingState::PendingConfirmation
        );

        // ② 内容不同（另一个 deviceId 与 clientNonce）→ 409，且首个 claim 固定的对端不被替换。
        let mut other = ClaimParts::valid(&pairing, 0x08);
        other.device = OTHER_DEVICE.to_owned();
        let conflict = harness.claim(&pairing, &other.body(&pairing)).await;
        assert_eq!(
            conflict.status, 409,
            "不同内容的重复 claim 必须冲突（§7.2）"
        );
        conflict.assert_security_headers();
        assert_eq!(conflict.json()["code"], json!("pairing.already_claimed"));
        let peer = harness
            .world
            .trust
            .pairing_peer(&pairing_id(&pairing.id))
            .await
            .expect("读对端行")
            .expect("对端行仍在");
        assert_eq!(
            peer.id(),
            &PeerIdentity::Device(device(DEVICE)),
            "首个 claim 固定的对端不被替换"
        );
        assert_eq!(harness.world.trust.pairing_count(), 1);
        assert_eq!(harness.world.trust.device_count(), 0);
        harness.stop().await;
    }

    /// 5.1 的第五项校验——HMAC 无效：401 且不泄露校验差异。
    #[tokio::test]
    async fn invalid_proof_is_unauthorized_without_leaking_the_difference() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;

        // ① 篡改 proof（形状合法、HMAC 不匹配）。
        let mut tampered = ClaimParts::valid(&pairing, 0x09).request(&pairing);
        tampered.proof =
            wire_proof(&identity_auth::PairingProof::try_from_bytes(&[9u8; 32]).unwrap());
        let tampered = harness
            .claim(&pairing, &serde_json::to_vec(&tampered).expect("可序列化"))
            .await;
        assert_eq!(tampered.status, 401);
        tampered.assert_security_headers();
        assert_eq!(tampered.json()["code"], json!("auth.proof_invalid"));
        assert_eq!(tampered.json()["retryable"], json!(false));
        assert_eq!(tampered.json()["details"], json!({}));
        assert_untouched(&harness, &pairing, PairingState::Created);

        // ② 客户端拿错了二维码里的过期时刻（proof 覆盖 `pairingExpiresAt`，而权威值来自记录）→ 同样 401，
        //    且 code/message/retryable/details 与上一条逐字相同。
        let shifted = pairing.with_expires_at("2027-01-01T00:00:00.000Z");
        let shifted = harness
            .claim(&pairing, &ClaimParts::valid(&pairing, 0x0a).body(&shifted))
            .await;
        assert_eq!(shifted.status, 401);
        assert_eq!(
            error_shape(&shifted),
            error_shape(&tampered),
            "不同的校验失败对端看到同一个形状"
        );
        assert_untouched(&harness, &pairing, PairingState::Created);

        // ③ 别的 secret 算出的 proof（同 id、同载荷、只换一把 secret）→ 同样 401，形状不变。
        let foreign = Pairing {
            id: pairing.id.clone(),
            secret: PairingSecret::try_from_bytes(&[3u8; 32]).expect("32 字节"),
            canonical_origin: pairing.canonical_origin.clone(),
            expires_at: pairing.expires_at.clone(),
        };
        let foreign = harness.claim(&pairing, &claim_body(&foreign, 0x0b)).await;
        assert_eq!(foreign.status, 401);
        assert_eq!(error_shape(&foreign), error_shape(&tampered));
        assert_untouched(&harness, &pairing, PairingState::Created);
        harness.stop().await;
    }

    /// 5.1 的第四项校验——把本机登记的**节点**配对拿到 Sync 设备端点上 claim：绑定（endpoint 而不是
    /// canonical origin）不可能与请求来源一致 → 403，且不写任何状态。
    ///
    /// 这条同时封住「目标族混淆」：设备端点的绑定比较先于状态机的目标族判定，因此这类请求拿不到
    /// `pending_confirmation`，也不会在节点配对上留下一行设备对端。
    #[tokio::test]
    async fn node_pairing_cannot_be_claimed_through_the_device_endpoint() {
        let harness = Harness::new().await;
        let response = harness
            .world
            .router()
            .handle(admin_request(
                Method::NodePairBegin,
                json!({
                    "mode": "owner",
                    "pairingUrl": null,
                    "displayName": "Owner Node",
                    "requestedGrants": ["grant.observe"],
                }),
            ))
            .await;
        let result = success(&response);
        let node_pairing = result["pairingId"].as_str().expect("pairingId").to_owned();
        let device_side = Pairing {
            id: node_pairing.clone(),
            secret: PairingSecret::try_from_bytes(&[1u8; 32]).expect("32 字节"),
            canonical_origin: TEST_PUBLIC_ORIGIN.to_owned(),
            expires_at: "2026-09-18T09:17:03.412Z".to_owned(),
        };
        let rejected = harness
            .claim(
                &device_side,
                &ClaimParts::valid(&device_side, 0x0c).body(&device_side),
            )
            .await;
        assert_eq!(rejected.status, 403, "节点配对不能经设备端点认领");
        assert_eq!(rejected.json()["code"], json!("auth.origin_mismatch"));
        assert_eq!(rejected.json()["details"], json!({}));
        assert_eq!(
            harness
                .world
                .trust
                .pairing(&node_pairing)
                .expect("节点配对必须已落库")
                .state(),
            PairingState::Created,
            "失败路径不得改变配对状态"
        );
        assert_eq!(harness.world.trust.device_count(), 0);
        assert_eq!(harness.world.trust.node_count(), 0);
        harness.stop().await;
    }

    #[tokio::test]
    async fn malformed_claim_body_is_rejected_with_400() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;

        // ① JSON 语法错误 → `protocol.invalid_json`。
        let broken = harness.claim(&pairing, b"{\"pairingId\":").await;
        assert_eq!(broken.status, 400);
        broken.assert_security_headers();
        assert_eq!(broken.json()["code"], json!("protocol.invalid_json"));

        // ② 字段缺失/未知字段 → `protocol.schema_invalid`。
        let missing = harness.claim(&pairing, b"{}").await;
        assert_eq!(missing.status, 400);
        assert_eq!(missing.json()["code"], json!("protocol.schema_invalid"));
        let unknown = harness
            .claim(&pairing, b"{\"pairingId\":\"x\",\"extra\":1}")
            .await;
        assert_eq!(unknown.status, 400);

        // ③ 非 `application/json` → 400，且不进入配对逻辑。
        let wrong_type = harness
            .post(
                CLAIM_PATH,
                &pairing.canonical_origin,
                &claim_body(&pairing, 0x0d),
                Some("text/plain"),
            )
            .await;
        assert_eq!(wrong_type.status, 400);
        assert_eq!(wrong_type.json()["code"], json!("protocol.schema_invalid"));
        let no_type = harness
            .post(
                CLAIM_PATH,
                &pairing.canonical_origin,
                &claim_body(&pairing, 0x0e),
                None,
            )
            .await;
        assert_eq!(no_type.status, 400);

        // ④ 词表外的 `clientKind` → 400（取值域属于 wire 层，不进配对状态机）。
        let mut raw = serde_json::to_value(ClaimParts::valid(&pairing, 0x0f).request(&pairing))
            .expect("可序列化");
        raw["clientKind"] = json!("workstation");
        let unknown_kind = harness
            .claim(&pairing, &serde_json::to_vec(&raw).expect("可序列化"))
            .await;
        assert_eq!(unknown_kind.status, 400);
        assert_eq!(
            unknown_kind.json()["code"],
            json!("protocol.schema_invalid")
        );
        assert_untouched(&harness, &pairing, PairingState::Created);
        harness.stop().await;
    }

    // ---------------------------------------------------------------------------------------------
    // 5.3：status 端点
    // ---------------------------------------------------------------------------------------------

    /// §7.3：五种业务状态一律 200，业务状态只由 body 的 `status` 表达。
    #[tokio::test]
    async fn status_reports_the_five_business_states_with_200() {
        let harness = Harness::new().await;

        // ① pending_confirmation（claim 之后、本机确认之前）。
        let pending = harness.begin().await;
        let request_id = claimed(&harness, &pending, 0x20).await;
        let response = query_status(&harness, &pending, &request_id, &nonce_text(0x21)).await;
        assert_eq!(response.status, 200);
        response.assert_security_headers();
        let body = response.json();
        assert_eq!(body["status"], json!("pending_confirmation"));
        assert_eq!(body["expiresAt"], json!(pending.expires_at));
        assert_eq!(body["pairingRequestId"], json!(request_id));
        assert!(
            body.get("device").is_none() && body.get("host").is_none(),
            "未批准的状态必须不带 device/host（§7.3）"
        );

        // ② approved：设备非秘密元数据 + 已授予的 scope + 本节点身份材料。
        harness.approve(&pending, json!(["session.list"])).await;
        let approved = query_status(&harness, &pending, &request_id, &nonce_text(0x22)).await;
        assert_eq!(approved.status, 200);
        let body = approved.json();
        assert_eq!(body["status"], json!("approved"));
        assert_eq!(body["device"]["deviceId"], json!(DEVICE));
        assert_eq!(body["device"]["name"], json!("Zhang's Phone"));
        assert_eq!(body["device"]["scopes"], json!(["session.list"]));
        assert_eq!(body["host"]["hostId"], json!(HOST));
        assert_eq!(
            body["host"]["hostPublicKey"]
                .as_str()
                .expect("hostPublicKey"),
            encode_base64url(device_public_key().as_bytes())
        );

        // ③ rejected（本机拒绝）。
        let rejected = harness.begin().await;
        let rejected_id = claimed(&harness, &rejected, 0x23).await;
        harness.reject(&rejected).await;
        let response = query_status(&harness, &rejected, &rejected_id, &nonce_text(0x24)).await;
        assert_eq!(response.status, 200);
        let body = response.json();
        assert_eq!(body["status"], json!("rejected"));
        assert!(body.get("device").is_none() && body.get("host").is_none());

        // ④ expired（推进注入时钟越过窗口；secret 在过期扫描时清除，终态仍按 200 返回）。
        let expired = harness.begin_with_window(json!(60_000)).await;
        let expired_id = claimed(&harness, &expired, 0x25).await;
        harness.world.clock.set("2026-09-18T09:15:00.000Z");
        let due = harness.world.authority.due_pairings(
            &[harness.record(&expired)],
            &CoreTimestamp::new("2026-09-18T09:15:00.000Z").expect("规范时间戳"),
        );
        assert_eq!(due.len(), 1, "未终结的过期配对需要提交终态写集");
        let response = query_status(&harness, &expired, &expired_id, &nonce_text(0x26)).await;
        assert_eq!(response.status, 200);
        let body = response.json();
        assert_eq!(body["status"], json!("expired"));
        assert!(
            body.get("device").is_none() && body.get("host").is_none(),
            "非 approved 状态不得带 device/host"
        );

        // ⑤ consumed（首次 WSS 认证成功后消费）：先经本机入口批准，再以持久化的设备身份提交消费
        //    （与连接面的收尾同口径：actor 归因取 `owned_device` 的持久记录，不取自对端自报值）。
        let consumed = harness.begin().await;
        let consumed_id = claimed(&harness, &consumed, 0x27).await;
        harness.approve(&consumed, json!(["session.list"])).await;
        let scopes = harness
            .world
            .trust
            .device(DEVICE)
            .expect("批准后必须存在设备行")
            .scopes()
            .clone();
        let consumed_record = harness
            .world
            .core
            .consume_pairing(
                &Actor::Device {
                    device: device(DEVICE),
                    scopes,
                },
                &pairing_id(&consumed.id),
            )
            .await
            .expect("消费已批准的配对必须成功");
        assert_eq!(consumed_record.state(), PairingState::Consumed);
        let response = query_status(&harness, &consumed, &consumed_id, &nonce_text(0x28)).await;
        assert_eq!(response.status, 200, "consumed 同样是 200（§7.3）");
        assert_eq!(response.json()["status"], json!("consumed"));
        harness.stop().await;
    }

    /// §7.3：proof 无效 → 401；未登记的 pairingId 也是 401（不区分存在性）。
    #[tokio::test]
    async fn status_with_an_invalid_proof_is_unauthorized() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        let request_id_text = claimed(&harness, &pairing, 0x30).await;

        // ① 换一把 secret 计算的 proof → 401。
        let foreign = StatusParts {
            pairing_request_id: request_id_text.clone(),
            nonce: nonce_text(0x31),
        };
        let mut request = foreign.request(&pairing);
        let other = PairingSecret::try_from_bytes(&[5u8; 32]).expect("32 字节");
        let proof = SyncStatusTranscript {
            host_id: node(HOST),
            device_id: device(DEVICE),
            pairing_id: pairing_id(&pairing.id),
            pairing_request_id: request_id(&request_id_text),
            request_nonce: Nonce::new(&nonce_text(0x31)).expect("规范 nonce"),
        }
        .hmac(&other)
        .expect("证明必须可计算");
        request.proof = wire_proof(&proof);
        let response = harness
            .status(&pairing, &serde_json::to_vec(&request).expect("可序列化"))
            .await;
        assert_eq!(response.status, 401);
        response.assert_security_headers();
        assert_eq!(response.json()["code"], json!("auth.proof_invalid"));

        // ② 另一个 requestNonce 的 proof（transcript 不同）→ 401。
        let mut swapped = foreign.request(&pairing);
        swapped.request_nonce = wire_nonce(&nonce_text(0x32));
        let response = harness
            .status(&pairing, &serde_json::to_vec(&swapped).expect("可序列化"))
            .await;
        assert_eq!(response.status, 401);

        // ③ 未登记的 pairingId → 401（本端点的存在性不对外区分）。
        let unknown = Pairing {
            id: "a1b2c3d4-0000-4000-8000-0000000000ff".to_owned(),
            secret: PairingSecret::try_from_bytes(&[6u8; 32]).expect("32 字节"),
            canonical_origin: pairing.canonical_origin.clone(),
            expires_at: pairing.expires_at.clone(),
        };
        let unknown_parts = StatusParts {
            pairing_request_id: request_id_text.clone(),
            nonce: nonce_text(0x33),
        };
        let response = harness
            .status(&unknown, &unknown_parts.body(&unknown))
            .await;
        assert_eq!(response.status, 401);
        assert_eq!(
            response.json()["code"],
            json!("auth.proof_invalid"),
            "未知配对与 proof 失败共用同一 code"
        );

        // ④ 非 JSON / 缺字段 → 400（业务状态未判定）。
        let plain = harness
            .post(
                STATUS_PATH,
                &pairing.canonical_origin,
                b"not json",
                Some("application/json"),
            )
            .await;
        assert_eq!(plain.status, 400);
        assert_eq!(plain.json()["code"], json!("protocol.invalid_json"));
        harness.stop().await;
    }

    /// §7.3：只有网络重试（同一 `requestNonce`）才复用原响应。
    #[tokio::test]
    async fn status_retry_with_the_same_nonce_returns_the_original_response() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        let request_id = claimed(&harness, &pairing, 0x40).await;
        let nonce = nonce_text(0x41);

        let first = query_status(&harness, &pairing, &request_id, &nonce).await;
        assert_eq!(first.status, 200);
        let retry = query_status(&harness, &pairing, &request_id, &nonce).await;
        assert_eq!(
            retry.body, first.body,
            "同一 nonce 的网络重试必须原样返回首次响应"
        );
        // 新轮询用新 nonce：拿到的是当前状态（这里内容相同，因为业务状态没变，但走的是重新组装的路径）。
        let fresh = query_status(&harness, &pairing, &request_id, &nonce_text(0x42)).await;
        assert_eq!(fresh.status, 200);
        harness.stop().await;
    }

    // ---------------------------------------------------------------------------------------------
    // 5.4：安全响应头
    // ---------------------------------------------------------------------------------------------

    /// 200/400/401/403/404/409/410/429 都带四个安全头（§7.1）。
    #[tokio::test]
    async fn every_pairing_response_carries_the_four_security_headers() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;

        let created = harness.claim(&pairing, &claim_body(&pairing, 0x50)).await;
        assert_eq!(created.status, 201);

        let request_id = created.json()["pairingRequestId"]
            .as_str()
            .expect("pairingRequestId")
            .to_owned();
        let ok = query_status(&harness, &pairing, &request_id, &nonce_text(0x51)).await;
        assert_eq!(ok.status, 200);

        let bad_request = harness.claim(&pairing, b"not json").await;
        assert_eq!(bad_request.status, 400);

        // 401：另起一个尚未认领的配对（已认领的配对换 nonce 会先落进 409 的幂等/冲突判定）。
        let unauthorized_pairing = harness.begin().await;
        let unauthorized = harness
            .claim(&unauthorized_pairing, &{
                let mut parts =
                    ClaimParts::valid(&unauthorized_pairing, 0x52).request(&unauthorized_pairing);
                parts.proof = wire_proof(
                    &identity_auth::PairingProof::try_from_bytes(&[1u8; 32]).expect("32 字节"),
                );
                serde_json::to_vec(&parts).expect("可序列化")
            })
            .await;
        assert_eq!(unauthorized.status, 401);

        // 403：来源与登记绑定不一致（载荷本身合法）。
        let forbidden = harness
            .post(
                CLAIM_PATH,
                OTHER_ORIGIN,
                &claim_body(&pairing, 0x56),
                Some("application/json"),
            )
            .await;
        assert_eq!(forbidden.status, 403);

        let unknown = Pairing {
            id: "a1b2c3d4-0000-4000-8000-0000000000aa".to_owned(),
            secret: PairingSecret::try_from_bytes(&[8u8; 32]).expect("32 字节"),
            canonical_origin: pairing.canonical_origin.clone(),
            expires_at: pairing.expires_at.clone(),
        };
        let not_found = harness.claim(&unknown, &claim_body(&unknown, 0x53)).await;
        assert_eq!(not_found.status, 404);

        // 409：已认领的配对收到内容不同的 claim。
        let mut other = ClaimParts::valid(&pairing, 0x54);
        other.device = OTHER_DEVICE.to_owned();
        let conflict = harness.claim(&pairing, &other.body(&pairing)).await;
        assert_eq!(conflict.status, 409);

        // 410：过期的配对（极短窗口 + 推进注入时钟）。
        let expired_pairing = harness.begin_with_window(json!(1)).await;
        harness.world.clock.set("2026-09-18T09:16:00.000Z");
        let gone = harness
            .claim(&expired_pairing, &claim_body(&expired_pairing, 0x55))
            .await;
        assert_eq!(gone.status, 410);
        harness.world.clock.set("2026-09-18T09:12:03.412Z");

        // 429：用一台新的接入层（限流窗口是它的私有状态）把 claim 额度用满（§14：10 次/分钟/IP）。
        let limiter = Harness::new().await;
        let limited_pairing = limiter.begin().await;
        for _ in 0..CLAIM_LIMIT_PER_MINUTE {
            assert_eq!(
                limiter.claim(&limited_pairing, b"not json").await.status,
                400
            );
        }
        let limited = limiter.claim(&limited_pairing, b"not json").await;
        assert_eq!(limited.status, 429);
        assert_eq!(limited.json()["code"], json!("resource.rate_limited"));

        for response in [
            &created,
            &ok,
            &bad_request,
            &unauthorized,
            &forbidden,
            &not_found,
            &conflict,
            &gone,
            &limited,
        ] {
            response.assert_security_headers();
        }
        limiter.stop().await;
        harness.stop().await;
    }

    /// §7.1：接入层在调用处理器之前产生的响应（413 请求体超限、405 方法不匹配、Host 校验失败）
    /// 也带四个安全头——靠注册时声明的每路径默认响应头。
    #[tokio::test]
    async fn access_layer_rejections_on_the_pairing_paths_carry_the_security_headers() {
        let harness = Harness::new().await;

        // ① 请求体超限：413 由接入层产生（处理器不被调用）。
        let oversized = harness
            .claim(
                &Pairing {
                    id: "a1b2c3d4-0000-4000-8000-0000000000bb".to_owned(),
                    secret: PairingSecret::try_from_bytes(&[2u8; 32]).expect("32 字节"),
                    canonical_origin: TEST_PUBLIC_ORIGIN.to_owned(),
                    expires_at: "2026-09-18T09:17:03.412Z".to_owned(),
                },
                &vec![b'a'; 80 * 1024],
            )
            .await;
        assert_eq!(oversized.status, 413, "超过请求体上限的 claim 在接入层被拒");
        oversized.assert_security_headers();

        // ② 方法不匹配：405 由 `axum` 的方法路由器产生（本 path 的默认头层在该路由器最外层）。
        let wrong_method = send(
            harness.addr,
            &WireRequest {
                method: "GET",
                path: STATUS_PATH,
                host: host_of(harness.addr),
                origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
                content_type: None,
                forwarded_for: None,
                body: Vec::new(),
            },
        )
        .await;
        assert_eq!(wrong_method.status, 405);
        wrong_method.assert_security_headers();

        // ③ Host 不在允许集合内：400 发生在路由之前（本接入层在未注册 path 上也先给 400）。
        for path in [CLAIM_PATH, STATUS_PATH] {
            let rejected = send(
                harness.addr,
                &WireRequest {
                    method: "POST",
                    path,
                    host: "evil.example.com".to_owned(),
                    origin: Some(TEST_PUBLIC_ORIGIN.to_owned()),
                    content_type: Some("application/json".to_owned()),
                    forwarded_for: None,
                    body: b"{}".to_vec(),
                },
            )
            .await;
            assert_eq!(rejected.status, 400, "{path} 上的错误 Host 必须 400");
            rejected.assert_security_headers();
        }
        harness.stop().await;
    }

    // ---------------------------------------------------------------------------------------------
    // 5.5：限流、转发头与失败计数
    // ---------------------------------------------------------------------------------------------

    /// §14：claim 每 IP 10 次/分钟，超出返回 429（`resource.rate_limited`）。
    #[tokio::test]
    async fn claim_rate_limit_returns_429_after_ten_attempts_per_ip() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        // 前 10 次尝试都进入处理器（用语法错误的 body，因此不推进状态）；第 11 次被限流。
        for _ in 0..10 {
            assert_eq!(harness.claim(&pairing, b"not json").await.status, 400);
        }
        let limited = harness.claim(&pairing, b"not json").await;
        assert_eq!(
            limited.status, 429,
            "同一 IP 的第 11 次 claim 被限流（§14）"
        );
        limited.assert_security_headers();
        let body = limited.json();
        assert_eq!(body["code"], json!("resource.rate_limited"));
        assert_eq!(body["retryable"], json!(true));
        let retry_after = body["details"]["retryAfterMs"]
            .as_u64()
            .expect("details.retryAfterMs 是登记字段");
        assert!(retry_after <= 60_000, "退避不超过窗口：{retry_after}");
        // 该次请求没有进入配对状态机。
        assert_eq!(harness.record(&pairing).state(), PairingState::Created);
        harness.stop().await;
    }

    /// §14：计数以**真实连接地址**为准——伪造的转发头不生效（直连对端不在 `daemon.trusted_proxies` 内）。
    ///
    /// 回退探针：把限流键换成 `X-Forwarded-For` 的取值时，本用例的每条请求都会带一个**不同**的伪造地址，
    /// 于是永远攒不满 10 次 → 第 11 条得到 400 而不是 429，本用例变红。
    #[tokio::test]
    async fn forged_forwarded_headers_do_not_bypass_the_claim_rate_limit() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        let forged = |index: usize| WireRequest {
            method: "POST",
            path: CLAIM_PATH,
            host: host_of(harness.addr),
            origin: Some(pairing.canonical_origin.clone()),
            content_type: Some("application/json".to_owned()),
            forwarded_for: Some(format!("203.0.113.{}", index + 1)),
            body: b"not json".to_vec(),
        };
        for index in 0..10 {
            let response = send(harness.addr, &forged(index)).await;
            assert_eq!(
                response.status, 400,
                "每次换一个伪造转发来源，计数仍落在真实连接地址上"
            );
        }
        let limited = send(harness.addr, &forged(10)).await;
        assert_eq!(limited.status, 429, "伪造转发头不得让限流绕过（§14）");
        assert_eq!(harness.record(&pairing).state(), PairingState::Created);
        harness.stop().await;
    }

    /// §14：status 每 `pairingId` 60 次/分钟，超出返回 429。
    #[tokio::test]
    async fn status_rate_limit_returns_429_after_sixty_queries_per_pairing() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        let request_id = claimed(&harness, &pairing, 0x60).await;
        // 60 次成功查询（每次新 nonce：新轮询不复用原 nonce）。
        for index in 0..60 {
            let response =
                query_status(&harness, &pairing, &request_id, &nonce_for_index(index)).await;
            assert_eq!(response.status, 200, "第 {index} 次状态查询应被允许");
        }
        let limited = query_status(&harness, &pairing, &request_id, &nonce_for_index(60)).await;
        assert_eq!(limited.status, 429, "同一 pairingId 的第 61 次查询被限流");
        limited.assert_security_headers();
        assert_eq!(limited.json()["code"], json!("resource.rate_limited"));
        harness.stop().await;
    }

    /// §14：同一配对的 proof 校验失败累计 5 次后该配对失效，不可再被认领。
    ///
    /// 失效后即使 proof 完全正确也拿不到 201；status 把同一事实投影为 `rejected`（见 [`status_token`]）。
    #[tokio::test]
    async fn fifth_failure_invalidates_the_pairing() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        let mut wrong = ClaimParts::valid(&pairing, 0x70).request(&pairing);
        wrong.proof =
            wire_proof(&identity_auth::PairingProof::try_from_bytes(&[9u8; 32]).expect("32 字节"));
        let wrong = serde_json::to_vec(&wrong).expect("可序列化");

        for attempt in 1..=4u32 {
            let response = harness.claim(&pairing, &wrong).await;
            assert_eq!(response.status, 401, "第 {attempt} 次失败是普通证明失败");
            assert_eq!(
                harness
                    .world
                    .authority
                    .failure_count(&pairing_id(&pairing.id)),
                attempt
            );
            assert_untouched(&harness, &pairing, PairingState::Created);
        }
        let fifth = harness.claim(&pairing, &wrong).await;
        assert_eq!(fifth.status, 401, "第 5 次失败同样是 401（不泄露失败计数）");
        assert_untouched(&harness, &pairing, PairingState::Created);

        // 失效后：完全正确的 claim 也被拒（409 `pairing.already_claimed`），不写任何状态。
        let correct = harness.claim(&pairing, &claim_body(&pairing, 0x71)).await;
        assert_eq!(correct.status, 409, "失效后不可再认领");
        assert_eq!(correct.json()["code"], json!("pairing.already_claimed"));
        assert_untouched(&harness, &pairing, PairingState::Created);

        // status 把失效投影为终态 `rejected`：客户端据此停止轮询（§7.3）。
        let material = harness
            .world
            .authority
            .pairing_request_material(&pairing_id(&pairing.id))
            .expect("失效不清除内存材料（清除只发生在过期/重启/消费）");
        let parts = StatusParts {
            pairing_request_id: material.pairing_request_id.as_str().to_owned(),
            nonce: nonce_text(0x72),
        };
        let response = harness.status(&pairing, &parts.body(&pairing)).await;
        assert_eq!(response.status, 200);
        assert_eq!(response.json()["status"], json!("rejected"));
        harness.stop().await;
    }

    // ---------------------------------------------------------------------------------------------
    // 5.6：敏感信息边界
    // ---------------------------------------------------------------------------------------------

    /// 以固定 secret 驱动失败路径：响应体、错误消息与**结构化日志**都不含 secret、proof 或完整载荷。
    #[tokio::test]
    async fn failure_paths_never_leak_the_pairing_secret() {
        let harness = Harness::new().await;
        let pairing = harness.begin().await;
        let secret_text = encode_base64url(pairing.secret.as_bytes());

        let captured = Arc::new(StdMutex::new(String::new()));
        let guard = tracing::subscriber::set_default(Capturing {
            level: LevelFilter::TRACE,
            lines: Arc::clone(&captured),
        });

        // ① 无效 proof（形状合法）→ 401。
        let mut tampered = ClaimParts::valid(&pairing, 0x80).request(&pairing);
        let tampered_proof =
            identity_auth::PairingProof::try_from_bytes(&[4u8; 32]).expect("32 字节");
        tampered.proof = wire_proof(&tampered_proof);
        let body = serde_json::to_vec(&tampered).expect("可序列化");
        let claim = harness.claim(&pairing, &body).await;
        assert_eq!(claim.status, 401);

        // ② 语法错误 → 400；③ 来源不一致 → 403；④ status 的无效 proof → 401。
        let malformed = harness.claim(&pairing, b"{\"pairingId\":").await;
        assert_eq!(malformed.status, 400);
        let origin = harness
            .post(CLAIM_PATH, OTHER_ORIGIN, &body, Some("application/json"))
            .await;
        assert_eq!(origin.status, 403);
        let mut status_request = StatusParts {
            pairing_request_id: "b2f3fbfa-c2c6-4f49-9e0c-643d152de18d".to_owned(),
            nonce: nonce_text(0x81),
        }
        .request(&pairing);
        status_request.proof = wire_proof(&tampered_proof);
        let status = harness
            .status(
                &pairing,
                &serde_json::to_vec(&status_request).expect("可序列化"),
            )
            .await;
        assert_eq!(status.status, 401);

        // ⑤ 未登记的 pairingId（存在性判定）→ 404。
        let unknown = Pairing {
            id: "a1b2c3d4-0000-4000-8000-0000000000cc".to_owned(),
            secret: PairingSecret::try_from_bytes(&[1u8; 32]).expect("32 字节"),
            canonical_origin: pairing.canonical_origin.clone(),
            expires_at: pairing.expires_at.clone(),
        };
        let missing = harness
            .claim(&unknown, &ClaimParts::valid(&unknown, 0x82).body(&unknown))
            .await;
        assert_eq!(missing.status, 404);

        drop(guard);
        let logs = captured
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();

        // 响应体不含任何秘密材料，也不回显请求体里的 HMAC 输入。
        for response in [&claim, &malformed, &origin, &status, &missing] {
            let text = response.text();
            assert!(
                !text.contains(&secret_text),
                "响应体不得包含 pairingSecret：{text}"
            );
            assert!(
                !text.contains(&encode_base64url(tampered_proof.as_bytes())),
                "响应体不得包含 proof"
            );
        }
        assert!(
            !logs.contains(&secret_text),
            "日志不得包含 pairingSecret：{logs}"
        );
        assert!(
            !logs.contains(&encode_base64url(tampered_proof.as_bytes())),
            "日志不得包含 proof"
        );
        // 日志确实记录了这些失败（否则上面的断言是空转）。
        assert!(
            logs.contains("sync pairing request rejected"),
            "失败路径必须留下结构化日志：{logs}"
        );
        harness.stop().await;
    }

    /// 捕获结构化日志的最小订阅器（测试专用；与 `app::logging` 同款的最小实现，不引入新依赖）。
    struct Capturing {
        level: LevelFilter,
        lines: Arc<StdMutex<String>>,
    }

    impl Subscriber for Capturing {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            *metadata.level() <= self.level
        }

        fn new_span(&self, _span: &Attributes<'_>) -> SpanId {
            SpanId::from_u64(1)
        }

        fn record(&self, _span: &SpanId, _values: &SpanRecord<'_>) {}

        fn record_follows_from(&self, _span: &SpanId, _follows: &SpanId) {}

        fn enter(&self, _span: &SpanId) {}

        fn exit(&self, _span: &SpanId) {}

        fn event(&self, event: &Event<'_>) {
            if !self.enabled(event.metadata()) {
                return;
            }
            let mut collector = Fields::default();
            event.record(&mut collector);
            let mut lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
            lines.push_str(event.metadata().level().as_str());
            lines.push(' ');
            lines.push_str(event.metadata().target());
            for (name, value) in collector.fields {
                lines.push(' ');
                lines.push_str(&name);
                lines.push('=');
                lines.push_str(&value);
            }
            lines.push('\n');
        }
    }

    /// 把事件字段按 `Debug` 收集成 `name=value` 文本（捕获日志用；`{value:?}` 与生产格式化同口径）。
    #[derive(Default)]
    struct Fields {
        fields: Vec<(String, String)>,
    }

    impl Visit for Fields {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.fields
                .push((field.name().to_owned(), format!("{value:?}")));
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            self.fields
                .push((field.name().to_owned(), value.to_owned()));
        }

        fn record_u64(&mut self, field: &Field, value: u64) {
            self.fields
                .push((field.name().to_owned(), value.to_string()));
        }

        fn record_i64(&mut self, field: &Field, value: i64) {
            self.fields
                .push((field.name().to_owned(), value.to_string()));
        }

        fn record_bool(&mut self, field: &Field, value: bool) {
            self.fields
                .push((field.name().to_owned(), value.to_string()));
        }
    }

    // ---------------------------------------------------------------------------------------------
    // 限流窗口与重放缓存的边界（与真实监听器无关的纯单元用例）
    // ---------------------------------------------------------------------------------------------

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

    /// 静默使用的 `test_nonce` 只用于构造合法 nonce 的对照，确保本文件的 `nonce_text` 与它同口径。
    #[test]
    fn nonce_helper_matches_the_shared_test_nonce() {
        let shared = test_nonce();
        assert_eq!(shared.as_str().len(), 43);
        assert_eq!(nonce_text(0x00), shared.as_str());
    }
}
