//! Sync 配对 HTTP 端点（`docs/SYNC_PROTOCOL.md` §7.1–§7.4，`design.md` D6）。
//!
//! 四个端点（`POST /sync/v1/pairing/claim`、`POST /sync/v1/pairing/status`、
//! `POST /sync/v1/pairing/lookup`、`POST /sync/v1/pairing/qr-lookup`）与 Node Link 的配对端点
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
//! `transport::net::ratelimit::SlidingWindowLimiter`），lookup 与扫码资料取回（qr-lookup）各有**同形但
//! 独立**的 10 次/分钟额度（同一来源、各自计数；任一条入口的成功或失败都不占用其它入口或 claim 的额度），
//! status 按 **pairingId** 60 次/分钟（键不是地址，因此用 [`PairingIdWindow`]：`SlidingWindowLimiter` 的
//! 键固定为 `IpAddr`，无法承载配对标识；两者的窗口语义、键集上限与失败关闭口径相同）。转发头不参与计数：
//! `client_ip` 只在 `daemon.trusted_proxies` 内的对端提供的转发头**被采信**时才是头部取值，否则就是
//! TCP 对端地址（`transport::net::proxy`）。
//!
//! ## 错误映射（§7.4；`code` 一律取自 §12.2 的封闭词表）
//!
//! | HTTP | 场景 | `code` |
//! |---|---|---|
//! | 400 | JSON 语法错误 / 字段 schema 无效 / 非 `application/json` | `protocol.invalid_json`、`protocol.schema_invalid` |
//! | 401 | proof 无效、失败计数达上限、载荷与配对目标族不符 | `auth.proof_invalid` |
//! | 401 | lookup 的任何失败（码格式/不存在/已消耗/已过期/已认领/来源不符） | `pairing.code_invalid` |
//! | 401 | qr-lookup 的端点内失败（可解析 JSON 的请求形状/凭据/状态/过期/失效/来源或身份绑定不符） | `pairing.qr_invalid` |
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
    PairingRequestMaterial, PairingSecret, QrToken, SyncPairingHostProof, SyncPairingStatus,
};
use sync_protocol::common::{
    Base64Url, NonEmptyText, Nullable, ProtocolVersionV1, RawObject, Timestamp, Uuid,
};
use sync_protocol::error::{Body as ErrorBody, ErrorCode};
use sync_protocol::pairing::{
    ClaimRequest, ClaimResponse, DeviceBlock, HostBlock, LookupRequest, LookupResponse,
    PairingStatus, PendingConfirmation, QrLookupRequest, StatusRequest, StatusResponse,
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

/// `POST /sync/v1/pairing/lookup`（§7.5；网页入口的一次性配对码）。
pub const LOOKUP_PATH: &str = "/sync/v1/pairing/lookup";

/// `POST /sync/v1/pairing/qr-lookup`（§7.6；原生 App 扫码后的配对资料取回）。
pub const QR_LOOKUP_PATH: &str = "/sync/v1/pairing/qr-lookup";

/// claim 的固定限流（§14：10 次/分钟/来源地址，不可配置）。
const CLAIM_LIMIT_PER_MINUTE: u32 = 10;

/// lookup 的固定限流（§14：10 次/分钟/来源地址，与 claim **各自独立**）。
const LOOKUP_LIMIT_PER_MINUTE: u32 = 10;

/// 扫码资料取回的固定限流（§14：10 次/分钟/来源地址，与 lookup/claim **各自独立**）。
const QR_LOOKUP_LIMIT_PER_MINUTE: u32 = 10;

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

/// lookup 的统一失败文本（§7.5：码不存在、已消耗、已过期、已认领、来源不符都是同一形状，不泄露存在性）。
const CODE_INVALID_MESSAGE: &str = "the pairing code is not valid";

/// 扫码资料取回的统一失败文本（§7.6：格式/凭据/状态/过期/来源绑定失败都是同一形状，不泄露存在性）。
const QR_INVALID_MESSAGE: &str = "the pairing qr token is not valid";

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

/// 配对 HTTP 的共享状态（组合根装配一次，注册到四个 path）。
///
/// 不持有端口快照、不在请求路径上读配置文件：本面判定所需的一切都是「已提交的持久事实」（经 core 的用例面
/// 读）与「本机身份」（`Authority` 的只读入口）；唯一来自配置的是 `daemon.public_origin`（lookup 的前置
/// 来源校验与绑定核对），由组合根在装配时注入。`Host` 边界由接入层按 `NetConfig` 判完。
pub struct PairingHttp {
    core: Arc<UseCases>,
    authority: Arc<Authority>,
    /// 本机 canonical origin（`daemon.public_origin`；与配对登记的绑定同源）。
    ///
    /// 只用于 lookup 的**前置**来源校验：来源不一致的请求必须在进入配对码查找之前失败（否则跨源请求可以
    /// 烧掉别人的码）。`None` = 未配置 canonical origin：网页入口不可能存在配对，lookup 一律失败。
    public_origin: Option<String>,
    /// claim 限流：键是 `PeerInfo::client_ip`（对端真实连接地址；不可信来源的转发头不生效）。
    claim_limit: SlidingWindowLimiter,
    /// lookup 限流：与 claim **各自独立**的额度（同一来源 10 次/分钟）。
    lookup_limit: SlidingWindowLimiter,
    /// 扫码资料取回限流：与 claim/lookup **各自独立**的额度（同一来源 10 次/分钟）。
    qr_limit: SlidingWindowLimiter,
    /// status 限流：键是 `pairingId`（§14 的另一条固定限流）。
    status_limit: PairingIdWindow,
    /// status 的「网络重试返回原响应」缓存（§7.3）。
    status_replays: Mutex<StatusReplays>,
}

impl PairingHttp {
    /// 装配。`core` 是用例面，`authority` 是身份状态机（两者都由组合根持有）；`public_origin` 是
    /// `daemon.public_origin`（lookup 的前置来源校验与响应字段装配都用它）。
    pub fn new(
        core: Arc<UseCases>,
        authority: Arc<Authority>,
        public_origin: Option<String>,
    ) -> Self {
        Self {
            core,
            authority,
            public_origin,
            claim_limit: SlidingWindowLimiter::new(CLAIM_LIMIT_PER_MINUTE, LIMIT_WINDOW),
            lookup_limit: SlidingWindowLimiter::new(LOOKUP_LIMIT_PER_MINUTE, LIMIT_WINDOW),
            qr_limit: SlidingWindowLimiter::new(QR_LOOKUP_LIMIT_PER_MINUTE, LIMIT_WINDOW),
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

    /// lookup 端点的处理器（组合根注册到 [`LOOKUP_PATH`]）。
    pub fn lookup_handler(self: &Arc<Self>) -> Arc<dyn HttpHandler> {
        Arc::new(LookupEndpoint {
            pairing: self.clone(),
        })
    }

    /// 扫码资料取回端点的处理器（组合根注册到 [`QR_LOOKUP_PATH`]）。
    pub fn qr_lookup_handler(self: &Arc<Self>) -> Arc<dyn HttpHandler> {
        Arc::new(QrLookupEndpoint {
            pairing: self.clone(),
        })
    }

    /// 四个配对 path 的每路径默认响应头（§7.1：**所有**配对 HTTP 响应都带这四个头）。
    ///
    /// 组合根注册 [`Self::claim_handler`]/[`Self::status_handler`]/[`Self::lookup_handler`]/
    /// [`Self::qr_lookup_handler`] 时**必须**把它作为 `default_headers` 传给 `NetListener::register_post`。
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
                    // 集合越界在 Sync 面不可达（claim 载荷没有集合字段，且登记值恒为空集，见上面 `requested`
                    // 的说明）；仍然具名映射而不是与证明失败混同，以免将来它变得可达时对外语义错位。
                    // 用 `protocol.schema_invalid` 而不是 `authorization.scope_denied`：后者保留给节点侧
                    // grant 与本地通道，设备方向不产生它（`pairing-model`）。
                    ClaimRejection::CapabilitiesExceedRegistered => (
                        StatusCode::BAD_REQUEST,
                        ErrorCode::ProtocolSchemaInvalid,
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
        // ⑫ 认领已生效（含幂等 Repeat）：关闭扫码取回入口。`close_qr_retrieval` 幂等且只移除反查条目，
        //    保留 secret 供幂等 claim/status 继续使用；它与本方法的成功路径同步执行，因此**认领之后
        //    开始**的 qr-lookup 一定找不到凭据。已经在认领前获准的在途响应不受影响（design D5）。
        self.authority.close_qr_retrieval(&pairing);
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
            .pairing_status(record, peer_view.as_ref())
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

    /// `POST /sync/v1/pairing/lookup`（§7.5 / design D4）：用一次性配对码取回配对绑定与 secret。
    ///
    /// 顺序（每一步都在任何响应材料之前失败关闭）：
    ///
    /// ① 限流（与 claim **各自独立**的每来源额度）；
    /// ② 只接受同源 `application/json`；
    /// ③ 前置来源校验：`Origin` 必须等于本机 canonical origin（`daemon.public_origin`）。这一条**先于**
    ///    配对码查找：跨源请求不得消耗别人的码；
    /// ④ 解码 + 归一化配对码：任何形状错误都与「不存在」同形（401），不返回 400；
    /// ⑤ `Authority::consume_pairing_code`（成功即原子消耗；`None` 是所有失败的唯一形状）；
    /// ⑥ 读持久记录：必须仍是 `created`、未过期、绑定与该来源一致（lookup **不**推进配对状态）；
    /// ⑦ 组装响应：`hostId`/`hostPublicKey` 取本机身份，`pairingSecret` 是完整 256 bit。
    async fn lookup(&self, request: HttpRequest) -> HttpResponse {
        // ① 限流：超限的请求不进入状态机，也不消耗配对码。
        if let RateLimit::Denied { retry_after } = self.lookup_limit.check(request.peer.client_ip) {
            return rate_limited(retry_after);
        }
        // ② 同源 `application/json`（与 claim/status 同一规则）。
        if !is_json_content_type(&request.headers) {
            return refuse(
                StatusCode::BAD_REQUEST,
                ErrorCode::ProtocolSchemaInvalid,
                "content type must be application/json",
            );
        }
        // ③ 前置来源校验：必须在查找之前完成（否则跨源请求可以烧掉一个有效码）。
        let Some(origin) = self.lookup_origin(&request.headers) else {
            return code_invalid();
        };
        // ④ 解码与归一化：`LookupRequest` 的 wire 形状错误、缺字段、未知字段与配对码字符集错误都收敛为
        //    同一个失败——serde 无法区分「码格式错」与其它 schema 错误，所以这里不做二级分类（§7.5）。
        let Ok(lookup_request) = decode_lookup(&request.body) else {
            return code_invalid();
        };
        let Ok(code) = identity_auth::PairingCode::parse(lookup_request.pairing_code.as_str())
        else {
            return code_invalid();
        };
        // ⑤ 成功即消耗（原子）；`None` 与「不存在的码」不可区分。
        let Some(lookup) = self.authority.consume_pairing_code(&code) else {
            return code_invalid();
        };
        // ⑥ 该码所属配对必须仍可认领：`created`、未过期、绑定与本次来源一致。lookup 不推进状态。
        let actor = Actor::PairingClaimant {
            pairing: lookup.pairing.clone(),
        };
        let record = match self
            .core
            .pairing_channel_view(&actor, &lookup.pairing)
            .await
        {
            Ok(Some(view)) => view.record,
            Ok(None) => return code_invalid(),
            Err(error) => {
                return self.port_error("lookup", &lookup.pairing, error);
            }
        };
        if record.state() != PairingState::Created
            || self.authority.now().as_str() >= record.expires_at().as_str()
            || record.host_binding() != origin.as_str()
        {
            return code_invalid();
        }
        // ⑦ 组装 200。字段全部来自已核对的事实；装配失败只可能是实现缺陷。
        let Some(body) = self
            .lookup_response(&record, &lookup.pairing, &lookup.secret)
            .await
        else {
            return internal_unavailable();
        };
        match serde_json::to_vec(&body) {
            Ok(bytes) => secure(HttpResponse::json(StatusCode::OK, Bytes::from(bytes))),
            Err(error) => {
                tracing::error!(error = %error, "the lookup response cannot be encoded");
                internal_unavailable()
            }
        }
    }

    /// `POST /sync/v1/pairing/qr-lookup`（§7.6 / design D4/D5）：用扫码取回凭据重复取回同一份配对资料。
    ///
    /// 顺序（每一步都在任何响应材料之前失败关闭）：
    ///
    /// ① 本端点**独立**的每来源限流（与 lookup/claim 互不占用额度）；
    /// ② 只接受同源 `application/json`（非 JSON 沿用既有的 400，不新增 415）；
    /// ③ 先判 JSON 语法：坏 JSON（含空体与截断）是 400 `protocol.schema_invalid`（§7.4）；随后可解析
    ///    JSON 的未知键、字段类型、编码或长度错误都与「凭据不存在」同形（401）；
    /// ④ `Origin` 必须存在、可归一化并与本机 canonical origin 逐字相等（缺失/不符同形 401）；
    /// ⑤ 有界凭据反查 → 只读该配对自身的权威记录（claimant 绑定该配对，零写入）；
    /// ⑥ `Authority::retrieve_qr_material` 复核权威状态（`created`、未过期、未失效、记录摘要仍与
    ///    内存 secret 一致）与身份绑定，成功才交付；
    /// ⑦ 成功响应与配对码 lookup 同形；不消耗凭据、不消耗网页配对码、不推进状态、不创建信任。
    async fn qr_lookup(&self, request: HttpRequest) -> HttpResponse {
        // ① 限流：超限的请求不进入状态机，也不触碰任何配对材料。
        if let RateLimit::Denied { retry_after } = self.qr_limit.check(request.peer.client_ip) {
            return rate_limited(retry_after);
        }
        // ② 与 claim/status/lookup 同一规则：只接受同源 `application/json`。
        if !is_json_content_type(&request.headers) {
            return refuse(
                StatusCode::BAD_REQUEST,
                ErrorCode::ProtocolSchemaInvalid,
                "content type must be application/json",
            );
        }
        // ③ 语法错误不属于「凭据形状错误」：坏 JSON 仍是 400 `protocol.schema_invalid`（§7.4）。
        //    只有可解析 JSON 的形状/编码错误才收敛为统一 401，避免把「不是 JSON」伪装成凭据失败。
        if serde_json::from_slice::<serde_json::Value>(&request.body).is_err() {
            return refuse(
                StatusCode::BAD_REQUEST,
                ErrorCode::ProtocolSchemaInvalid,
                "request body must be valid JSON",
            );
        }
        let Ok(qr_request) = decode_qr_lookup(&request.body) else {
            return qr_invalid();
        };
        let Ok(token) = QrToken::try_from_bytes(qr_request.qr_token.as_bytes()) else {
            return qr_invalid();
        };
        // ④ 来源校验先于任何查找：跨源请求不得探测凭据是否存在（错误与「不存在」同形）。
        let Some(origin) = self.lookup_origin(&request.headers) else {
            return qr_invalid();
        };
        // ⑤ 有界反查：只覆盖本进程内存里仍持有 secret 且凭据仍绑定的配对。
        let Some(found) = self.authority.lookup_pairing_by_qr_token(&token) else {
            return qr_invalid();
        };
        let actor = Actor::PairingClaimant {
            pairing: found.pairing.clone(),
        };
        let record = match self.core.pairing_channel_view(&actor, &found.pairing).await {
            Ok(Some(view)) => view.record,
            Ok(None) => return qr_invalid(),
            Err(error) => return self.port_error("qr-lookup", &found.pairing, error),
        };
        // ⑥ 来源与持久绑定必须一致；状态/过期/失效/材料摘要由身份层复核（唯一实现）。
        if record.host_binding() != origin.as_str() {
            return qr_invalid();
        }
        let Some(retrieved) = self.authority.retrieve_qr_material(&record, &token) else {
            return qr_invalid();
        };
        // ⑦ 与配对码 lookup 完全同形的七字段；装配失败只可能是实现缺陷。
        let Some(body) = self
            .lookup_response(&record, &retrieved.pairing, &retrieved.secret)
            .await
        else {
            return internal_unavailable();
        };
        match serde_json::to_vec(&body) {
            Ok(bytes) => secure(HttpResponse::json(StatusCode::OK, Bytes::from(bytes))),
            Err(error) => {
                tracing::error!(error = %error, "the qr lookup response cannot be encoded");
                internal_unavailable()
            }
        }
    }

    /// 配对码 lookup 与扫码资料取回的**前置**来源校验：`Origin` 头必须存在、可归一化，并与本机
    /// canonical origin 逐字相等。
    ///
    /// 返回归一化后的来源文本（用于与持久记录的 `host_binding` 再核对一次）。`Host` 边界由接入层
    /// （`HostPolicy`）在调用处理器之前判定，本方法只判来源；两者不可互相替代（§3.2）。
    fn lookup_origin(&self, headers: &HeaderMap) -> Option<String> {
        let configured = self.public_origin.as_deref()?;
        // 配置值必须本身是 canonical origin；不是则网页入口不可用（失败关闭，不回落到 localhost）。
        let canonical = CanonicalOrigin::parse(configured).ok()?;
        let known = request_origin(headers).ok()?.as_str()?.to_owned();
        if known != canonical.as_str() {
            return None;
        }
        Some(canonical.as_str().to_owned())
    }

    /// 组装 `lookupResponse`（§7.5/§7.6）：主机身份材料、规范化来源、完整 pairing secret 与过期时间。
    ///
    /// 两条入口（配对码 lookup 与扫码资料取回）交付同一份绑定，因此共用这一处装配：`pairing`/`secret`
    /// 必须来自刚刚通过各自校验的那份材料。
    async fn lookup_response(
        &self,
        record: &PairingRecord,
        pairing: &PairingId,
        secret: &PairingSecret,
    ) -> Option<LookupResponse> {
        let host_public_key = self.authority.node_public_key().await.ok()?;
        Some(LookupResponse {
            protocol_version: ProtocolVersionV1::new(1).ok()?,
            pairing_id: Uuid::parse(pairing.as_str()).ok()?,
            host_id: Uuid::parse(self.authority.local_node().as_str()).ok()?,
            host_public_key: Base64Url::<65>::parse(&encode_base64url(host_public_key.as_bytes()))
                .ok()?,
            canonical_origin: sync_protocol::pairing::CanonicalOrigin::parse(record.host_binding())
                .ok()?,
            pairing_secret: Base64Url::<32>::parse(&encode_base64url(secret.as_bytes())).ok()?,
            expires_at: Timestamp::parse(record.expires_at().as_str()).ok()?,
        })
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

    /// 组装 `statusResponse`（§7.3）：`approved` 带该设备的**非秘密元数据**（标识、展示名）与本节点
    /// 身份材料（`hostId` 与主机公钥），不签发任何凭据、**不返回任何授权集合**（设备权限层已删除）；
    /// 其余状态只带状态与过期时间。主机公钥是本机身份材料的原文，客户端据此与配对载荷里的那把逐字节
    /// 比对（响应本身没有主机签名）。
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
            // 设备行是展示名的唯一来源（`owned_device`，确认事务创建）；本入口是未认证的窄读面，
            // 不带 `require_local`（`design.md` D2）。
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

/// lookup 端点的 [`HttpHandler`]（薄包装：把请求交给 [`PairingHttp`]）。
struct LookupEndpoint {
    pairing: Arc<PairingHttp>,
}

#[async_trait::async_trait]
impl HttpHandler for LookupEndpoint {
    async fn handle(&self, request: HttpRequest) -> HttpResponse {
        self.pairing.lookup(request).await
    }
}

/// 扫码资料取回端点的 [`HttpHandler`]（薄包装：把请求交给 [`PairingHttp`]）。
struct QrLookupEndpoint {
    pairing: Arc<PairingHttp>,
}

#[async_trait::async_trait]
impl HttpHandler for QrLookupEndpoint {
    async fn handle(&self, request: HttpRequest) -> HttpResponse {
        self.pairing.qr_lookup(request).await
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

/// 持久化的对端行 → 状态机的对端视图（唯一用途：`verify_claim` 的幂等比对与状态视图的入参）。
///
/// `requested` 取配对行登记的集合：与 [`ClaimFields`] 的构造同源（设备方向的登记值恒为空集，
/// 本协议不派生短验证码，因此这里也没有任何 SAS 材料）。
fn claimed_pairing(record: &PairingRecord, peer: &PairingPeer) -> ClaimedPairing {
    ClaimedPairing {
        pairing: record.id().clone(),
        peer: peer.id().clone(),
        display_name: peer.display_name().to_owned(),
        public_key: peer.public_key().clone(),
        host_binding: peer.host_binding().to_owned(),
        client_nonce: peer.client_nonce().clone(),
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

/// lookup 的统一失败（§7.5）：码格式错误、不存在、已消耗、已过期、已认领、来源不符全部收敛到这里。
///
/// 不加任何 `details`：不同原因的响应必须逐字节同形。消息是固定短文本，不回显码。
fn code_invalid() -> HttpResponse {
    refuse(
        StatusCode::UNAUTHORIZED,
        ErrorCode::PairingCodeInvalid,
        CODE_INVALID_MESSAGE,
    )
}

/// 扫码资料取回的统一失败（§7.6）：请求形状、凭据不存在/已清理、已过期、已拒绝、已认领、
/// 已失效、来源或身份绑定不符全部收敛到这里。
///
/// 与 [`code_invalid`] 一样不带 `details`：不同原因必须逐字节同形，响应不回显任何材料。
fn qr_invalid() -> HttpResponse {
    refuse(
        StatusCode::UNAUTHORIZED,
        ErrorCode::PairingQrInvalid,
        QR_INVALID_MESSAGE,
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

/// lookup 请求体的解码。**任何**失败都按统一失败处理（§7.5）：serde 无法区分「码形状错误」与其它
/// schema 错误，因此这里既不做两级错误分类，也不把错误码交给调用方。
fn decode_lookup(body: &[u8]) -> Result<LookupRequest, ()> {
    serde_json::from_slice::<LookupRequest>(body).map_err(|_| ())
}

/// 扫码资料取回请求体的解码（§7.6）：未知键、字段类型、编码与长度错误全部按统一失败处理。
///
/// 语法错误由调用方在此之前单独判为 400（§7.4），因此这里只需回答「形状是否可接受」。
fn decode_qr_lookup(body: &[u8]) -> Result<QrLookupRequest, ()> {
    serde_json::from_slice::<QrLookupRequest>(body).map_err(|_| ())
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
    let response = error_response(
        StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::ResourceRateLimited,
        "pairing attempt is rate limited",
        &correlation,
        details,
    );
    // §14：429 必须带可用的 `Retry-After`（整数秒，向上取整且至少 1）。`details.retryAfterMs`
    // 继续给出毫秒精度；两者同源，不各自计算窗口。
    let seconds = retry_after
        .as_millis()
        .div_ceil(1_000)
        .max(1)
        .min(u128::from(u32::MAX));
    match HeaderValue::from_str(&seconds.to_string()) {
        Ok(value) => response.with_header(HeaderName::from_static("retry-after"), value),
        Err(error) => {
            tracing::error!(error = %error, "the retry-after header cannot be encoded");
            response
        }
    }
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
