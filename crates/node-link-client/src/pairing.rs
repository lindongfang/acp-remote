//! Access Node 的配对 HTTP 客户端（`NODE_LINK_PROTOCOL.md` §13.1–§13.4）。
//!
//! 本模块只做三件事：解析 Owner 的配对 URL（二维码载荷）、发出 claim、轮询 status；密码学部分全部复用
//! `identity-auth` 的 transcript（`node-link-pairing-proof/v1`、`node-link-pairing-owner-proof/v1`、
//! `node-link-pairing-status/v1`），不在这里另算 HMAC 或验签。
//!
//! 边界纪律：
//!
//! - **秘密只在内存**：`pairingSecret` 由 [`PairingTicket`] 持有，不实现 `Debug`/`Display`，也不写日志；
//!   错误消息只含 HTTP 状态与 wire `code`，不含完整 URL、fragment 或响应正文。
//! - **不读系统时钟**：`claim`/`status` 的当前时间由调用方传入（与 `link`/`delivery` 同口径）。
//! - **不做重定向**：3xx 一律失败（配对端点不是可跟随的公开 URL，跟随会改变端点信任）。
//! - **信任锚显式**：HTTPS 走 [`crate::transport::TlsTrust::Roots`]；明文 `http://` 只在调用方显式声明
//!   开发模式且目标为 loopback 时可用。没有「跳过校验」的开关。
//!
//! 这不是通用 HTTP 客户端：只支持 `POST` + 固定路径 + `Content-Length`/`chunked` 响应，请求体上限
//! [`MAX_RESPONSE_BYTES`]，超限即失败关闭。

use std::time::Duration;

use acp_core::model::{Fingerprint, NodeId, Nonce, PairingId, PeerPublicKey, Timestamp};
use identity_auth::{
    NodeLinkPairingOwnerProof, NodeLinkPairingProof, NodeLinkPairingStatus, P1363Signature,
    PairingRequestId, PairingSecret,
};
use node_link_protocol::common::{
    Base64Url, NodeName, ProtocolVersionV1, Timestamp as WireTimestamp, Uuid as WireUuid,
};
use node_link_protocol::pairing::{
    ClaimRequest, ClaimResponse, Endpoint, PairingStatus, StatusRequest, StatusResponse,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;

use crate::error::ClientError;
use crate::identity::NodeIdentity;
use crate::transport::{TlsTrust, tls_client_config};

/// claim 端点路径（§13.2）。
pub const CLAIM_PATH: &str = "/node-link/v1/pairing/claim";

/// status 端点路径（§13.3）。
pub const STATUS_PATH: &str = "/node-link/v1/pairing/status";

/// 二维码承载 secret 的路径（§13.1）。
pub const PAIR_PATH: &str = "/node-link/pair";

/// 响应体上限：配对响应只有元数据（§13.4 的错误体也是短结构），超过即视为异常。
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// 单次配对 HTTP 的超时（连接 + 请求 + 响应）。
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// 配对 URL 的 scheme。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scheme {
    Https,
    /// 只在显式开发模式下允许；目标必须是 loopback。
    Http,
}

/// claim/status 的 HTTP 目标（由二维码 URL 的 origin 派生，§13.1）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpTarget {
    scheme: Scheme,
    /// `Host` 头与 TLS SNI 的来源（`host[:port]`）。
    authority: String,
    host: String,
    port: u16,
}

/// Owner 的配对邀请（二维码载荷 + 它对应的 HTTP origin）。
///
/// `Debug` 只打印 owner id 与 endpoint：secret 与完整 URL 不进日志。
#[derive(Clone, PartialEq, Eq)]
pub struct PairingTicket {
    payload: node_link_protocol::pairing::QrPayload,
    target: HttpTarget,
}

impl std::fmt::Debug for PairingTicket {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PairingTicket")
            .field("owner_node_id", &self.payload.owner_node_id.as_str())
            .field("pairing_id", &self.payload.pairing_id.as_str())
            .field("endpoint", &self.payload.endpoint.as_str())
            .finish_non_exhaustive()
    }
}

impl PairingTicket {
    /// 解析 Owner 的配对 URL（`<scheme>://<authority>/node-link/pair#data=<base64url-json>`）。
    ///
    /// `allow_plaintext` 必须由组合根按 `dev_mode.enabled && dev_mode.allow_plaintext` 显式给出；
    /// 明文 URL 的目标还必须是 loopback。endpoint 的 host 必须与 URL 的 authority 一致（§13.1），
    /// 否则拒绝 claim。
    pub fn parse(url: &str, allow_plaintext: bool) -> Result<Self, ClientError> {
        let (scheme, rest) = url.split_once("://").ok_or_else(|| {
            ClientError::Pairing("the pairing URL is not an absolute URL".to_owned())
        })?;
        let scheme = match scheme {
            "https" => Scheme::Https,
            "http" if allow_plaintext => Scheme::Http,
            "http" => {
                return Err(ClientError::Pairing(
                    "plaintext pairing URLs require the explicit dev mode".to_owned(),
                ));
            }
            _ => {
                return Err(ClientError::Pairing(
                    "the pairing URL must use https".to_owned(),
                ));
            }
        };
        let (authority, tail) = match rest.split_once('/') {
            Some((authority, tail)) => (authority, tail),
            None => {
                return Err(ClientError::Pairing(
                    "the pairing URL has no payload".to_owned(),
                ));
            }
        };
        if authority.is_empty() {
            return Err(ClientError::Pairing(
                "the pairing URL has no authority".to_owned(),
            ));
        }
        let (path, fragment) = tail
            .split_once('#')
            .ok_or_else(|| ClientError::Pairing("the pairing URL has no fragment".to_owned()))?;
        if format!("/{path}") != PAIR_PATH {
            return Err(ClientError::Pairing(
                "the pairing URL path is not /node-link/pair".to_owned(),
            ));
        }
        let data = fragment
            .strip_prefix("data=")
            .filter(|data| !data.is_empty())
            .ok_or_else(|| {
                ClientError::Pairing("the pairing URL fragment has no data".to_owned())
            })?;
        let decoded =
            base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, data)
                .map_err(|_| {
                    ClientError::Pairing("the pairing payload is not base64url".to_owned())
                })?;
        let payload: node_link_protocol::pairing::QrPayload = serde_json::from_slice(&decoded)
            .map_err(|_| {
                ClientError::Pairing("the pairing payload is not a v1 QR payload".to_owned())
            })?;
        let (host, port) = split_authority(
            authority,
            match scheme {
                Scheme::Https => 443,
                Scheme::Http => 80,
            },
        )
        .ok_or_else(|| {
            ClientError::Pairing("the pairing URL authority is not a host[:port]".to_owned())
        })?;
        if scheme == Scheme::Http && !is_loopback(&host) {
            return Err(ClientError::Pairing(
                "plaintext pairing is only allowed for loopback hosts".to_owned(),
            ));
        }
        let endpoint_authority = endpoint_authority(&payload.endpoint).ok_or_else(|| {
            ClientError::Pairing("the pairing payload endpoint is malformed".to_owned())
        })?;
        if !endpoint_authority.eq_ignore_ascii_case(authority) {
            return Err(ClientError::Pairing(
                "the endpoint host does not match the pairing URL host".to_owned(),
            ));
        }
        Ok(Self {
            payload,
            target: HttpTarget {
                scheme,
                authority: authority.to_owned(),
                host,
                port,
            },
        })
    }

    /// Owner 的 node id。
    pub fn owner_node_id(&self) -> Result<NodeId, ClientError> {
        NodeId::new(self.payload.owner_node_id.as_str())
            .map_err(|_| ClientError::Pairing("the owner node id is malformed".to_owned()))
    }

    /// Owner 公钥（二维码里固定的那一枚）。
    pub fn owner_public_key(&self) -> Result<PeerPublicKey, ClientError> {
        PeerPublicKey::try_from_bytes(self.payload.owner_public_key.as_bytes())
            .map_err(|_| ClientError::Pairing("the owner public key is malformed".to_owned()))
    }

    /// Owner 公钥指纹（CLI 的 `--fingerprint` 与它逐字比较）。
    pub fn owner_fingerprint(&self) -> Result<Fingerprint, ClientError> {
        Ok(self.owner_public_key()?.fingerprint())
    }

    /// `pairingId`。
    pub fn pairing_id(&self) -> Result<PairingId, ClientError> {
        PairingId::new(self.payload.pairing_id.as_str())
            .map_err(|_| ClientError::Pairing("the pairing id is malformed".to_owned()))
    }

    /// `expiresAt`（wire 文本）。
    pub fn expires_at(&self) -> &WireTimestamp {
        &self.payload.expires_at
    }

    /// 邀请是否已在 `at` 之前过期（两个时间都是 §1.1 的定宽文本，字典序即时间序）。
    pub fn is_expired(&self, at: &Timestamp) -> bool {
        at.as_str() > self.payload.expires_at.as_str()
    }

    /// WSS 业务端点（imported 连接的 endpoint）。
    pub fn endpoint(&self) -> &Endpoint {
        &self.payload.endpoint
    }
}

/// claim 成功后 Access 侧必须留在内存里的恢复材料。
///
/// `Debug` 不打印 nonce（一次性随机数，虽不是秘密，但没有诊断价值）。
#[derive(Clone, PartialEq, Eq)]
pub struct ClaimSession {
    ticket: PairingTicket,
    pairing_request_id: PairingRequestId,
    server_nonce_text: String,
    expires_at: Timestamp,
}

impl std::fmt::Debug for ClaimSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClaimSession")
            .field("pairing_request_id", &self.pairing_request_id.as_str())
            .field("expires_at", &self.expires_at.as_str())
            .finish_non_exhaustive()
    }
}

impl ClaimSession {
    /// 原始邀请（轮询 status 时复用同一 secret 与 origin）。
    pub fn ticket(&self) -> &PairingTicket {
        &self.ticket
    }

    /// `pairingRequestId`（status 请求的字段）。
    pub fn pairing_request_id(&self) -> &PairingRequestId {
        &self.pairing_request_id
    }

    /// Owner 给出的 `expiresAt`。
    pub fn expires_at(&self) -> &Timestamp {
        &self.expires_at
    }
}

/// 一次 claim 的 wire 结果 + 已完成的本地校验。
#[derive(Debug, Clone)]
pub struct ClaimOutcome {
    /// 需要保留的恢复材料。
    pub session: ClaimSession,
    /// Owner 返回的 `expiresAt`。
    pub expires_at: Timestamp,
}

/// status 查询的结果（已做 owner 身份一致性校验）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusOutcome {
    /// wire 状态。
    pub status: PairingStatus,
    /// `expiresAt`。
    pub expires_at: Timestamp,
    /// `approved` 时的 Owner 身份（node id、公钥）。
    pub owner: Option<(NodeId, PeerPublicKey)>,
    /// `approved` 时被批准的 Access Node id（必须等于本机 node id）。
    pub access_node_id: Option<NodeId>,
}

/// 发出 claim（`POST /node-link/v1/pairing/claim`，§13.2）。
///
/// 校验顺序：邀请未过期 → Owner 的 `ownerProof` 对二维码固定公钥验签 → 响应字段与本机一致。
/// 任一步失败都不产生本地状态（调用方只在成功后保存 [`ClaimOutcome::session`]）。
pub async fn claim(
    ticket: &PairingTicket,
    identity: &dyn NodeIdentity,
    node_name: &str,
    client_nonce: &Nonce,
    trust: &TlsTrust,
    at: &Timestamp,
) -> Result<ClaimOutcome, ClientError> {
    if ticket.is_expired(at) {
        return Err(ClientError::Pairing(
            "the pairing invitation has expired".to_owned(),
        ));
    }
    let owner_node_id = ticket.owner_node_id()?;
    let access_node_id = identity.node_id().clone();
    let pairing_id = ticket.pairing_id()?;
    let pairing_expires_at = core_timestamp(ticket.expires_at())?;
    let owner_public_key = ticket.owner_public_key()?;
    let access_public_key = identity.public_key().await?;
    let node_name = NodeName::parse(node_name)
        .map_err(|_| ClientError::Pairing("the node name is not 1..=128 characters".to_owned()))?;

    let proof = NodeLinkPairingProof {
        owner_node_id: owner_node_id.clone(),
        access_node_id: access_node_id.clone(),
        pairing_id: pairing_id.clone(),
        pairing_expires_at: pairing_expires_at.clone(),
        access_public_key: access_public_key.clone(),
        client_nonce: client_nonce.clone(),
        node_name: node_name.as_str().to_owned(),
        node_kind: acp_core::model::NodeKind::Access,
    }
    .hmac(&secret_of(ticket)?)
    .map_err(|_| ClientError::Pairing("the claim proof cannot be computed".to_owned()))?;

    let request = ClaimRequest {
        protocol_version: protocol_version()?,
        pairing_id: ticket.payload.pairing_id.clone(),
        owner_node_id: ticket.payload.owner_node_id.clone(),
        access_node_id: wire_uuid(access_node_id.as_str())?,
        node_name,
        node_kind: node_link_protocol::pairing::AccessNodeKind,
        endpoint: ticket.payload.endpoint.clone(),
        access_public_key: base64url_65(access_public_key.as_bytes())?,
        client_nonce: base64url_32(client_nonce.as_str())?,
        proof: base64url_32(&base64url_text(proof.as_bytes()))?,
    };
    let body = serde_json::to_vec(&request)
        .map_err(|_| ClientError::Pairing("the claim request cannot be encoded".to_owned()))?;
    let reply = post_json(ticket.target.clone(), CLAIM_PATH, &body, trust).await?;
    if !matches!(reply.status, 200 | 201) {
        return Err(http_failure(&reply));
    }
    let response: ClaimResponse = serde_json::from_slice(&reply.body)
        .map_err(|_| ClientError::Pairing("the claim response is not a v1 claim".to_owned()))?;
    let expires_at = core_timestamp(&response.expires_at)?;

    // ownerProof：用二维码里固定的 owner 公钥验签（§13.2 的 MUST）。
    let server_nonce_text = base64url_text(response.server_nonce.as_bytes());
    let server_nonce = Nonce::new(&server_nonce_text)
        .map_err(|_| ClientError::Pairing("the server nonce is malformed".to_owned()))?;
    let signature =
        P1363Signature::try_from_base64url(&base64url_text(response.owner_proof.as_bytes()))
            .map_err(|_| ClientError::Pairing("the owner proof is malformed".to_owned()))?;
    NodeLinkPairingOwnerProof {
        owner_node_id: owner_node_id.clone(),
        access_node_id: access_node_id.clone(),
        pairing_id,
        owner_public_key: owner_public_key.clone(),
        access_public_key,
        client_nonce: client_nonce.clone(),
        server_nonce,
        pairing_request_id: PairingRequestId::parse(response.pairing_request_id.as_str())
            .map_err(|_| ClientError::Pairing("the pairing request id is malformed".to_owned()))?,
    }
    .verify(&owner_public_key, &signature)
    .map_err(|_| {
        ClientError::Pairing("the owner proof does not verify against the invitation".to_owned())
    })?;

    Ok(ClaimOutcome {
        session: ClaimSession {
            ticket: ticket.clone(),
            pairing_request_id: PairingRequestId::parse(response.pairing_request_id.as_str())
                .map_err(|_| {
                    ClientError::Pairing("the pairing request id is malformed".to_owned())
                })?,
            server_nonce_text,
            expires_at: expires_at.clone(),
        },
        expires_at,
    })
}

/// 查询 claim 状态（`POST /node-link/v1/pairing/status`，§13.3）。
///
/// 状态端点对业务状态恒回 `200`（含 `expired`/`consumed`）；`401` 等表示请求本身被拒绝。
/// `approved` 时必须核对：被批准的 Access Node 是本机、Owner 的 id 与公钥与邀请逐字一致。
pub async fn status(
    session: &ClaimSession,
    identity: &dyn NodeIdentity,
    request_nonce: &Nonce,
    trust: &TlsTrust,
    at: &Timestamp,
) -> Result<StatusOutcome, ClientError> {
    if at.as_str() > session.expires_at.as_str() {
        return Ok(StatusOutcome {
            status: PairingStatus::Expired,
            expires_at: session.expires_at.clone(),
            owner: None,
            access_node_id: None,
        });
    }
    let ticket = &session.ticket;
    let owner_node_id = ticket.owner_node_id()?;
    let access_node_id = identity.node_id().clone();
    let pairing_id = ticket.pairing_id()?;
    let proof = NodeLinkPairingStatus {
        owner_node_id: owner_node_id.clone(),
        access_node_id: access_node_id.clone(),
        pairing_id: pairing_id.clone(),
        pairing_request_id: session.pairing_request_id.clone(),
        request_nonce: request_nonce.clone(),
    }
    .hmac(&secret_of(ticket)?)
    .map_err(|_| ClientError::Pairing("the status proof cannot be computed".to_owned()))?;
    let request = StatusRequest {
        protocol_version: protocol_version()?,
        owner_node_id: ticket.payload.owner_node_id.clone(),
        access_node_id: wire_uuid(access_node_id.as_str())?,
        pairing_id: ticket.payload.pairing_id.clone(),
        pairing_request_id: wire_uuid(session.pairing_request_id.as_str())?,
        request_nonce: base64url_32(request_nonce.as_str())?,
        proof: base64url_32(&base64url_text(proof.as_bytes()))?,
    };
    let body = serde_json::to_vec(&request)
        .map_err(|_| ClientError::Pairing("the status request cannot be encoded".to_owned()))?;
    let reply = post_json(ticket.target.clone(), STATUS_PATH, &body, trust).await?;
    if reply.status != 200 {
        return Err(http_failure(&reply));
    }
    let response: StatusResponse = serde_json::from_slice(&reply.body)
        .map_err(|_| ClientError::Pairing("the status response is not a v1 status".to_owned()))?;
    if response.pairing_request_id.as_str() != session.pairing_request_id.as_str() {
        return Err(ClientError::Pairing(
            "the status response belongs to a different pairing request".to_owned(),
        ));
    }
    let expires_at = core_timestamp(&response.expires_at)?;
    match response.status {
        PairingStatus::Approved => {
            let node = response.node.as_ref().ok_or_else(|| {
                ClientError::Pairing("the approved status has no node".to_owned())
            })?;
            let owner = response.owner.as_ref().ok_or_else(|| {
                ClientError::Pairing("the approved status has no owner".to_owned())
            })?;
            if node.access_node_id.as_str() != access_node_id.as_str() {
                return Err(ClientError::Pairing(
                    "the owner approved a different access node".to_owned(),
                ));
            }
            let owner_id = NodeId::new(owner.owner_node_id.as_str()).map_err(|_| {
                ClientError::Pairing("the approved owner node id is malformed".to_owned())
            })?;
            let owner_key = PeerPublicKey::try_from_bytes(owner.owner_public_key.as_bytes())
                .map_err(|_| {
                    ClientError::Pairing("the approved owner public key is malformed".to_owned())
                })?;
            if owner_id != owner_node_id || owner_key != ticket.owner_public_key()? {
                return Err(ClientError::Pairing(
                    "the approved owner identity does not match the invitation".to_owned(),
                ));
            }
            Ok(StatusOutcome {
                status: response.status,
                expires_at,
                owner: Some((owner_id, owner_key)),
                access_node_id: Some(access_node_id),
            })
        }
        _ => Ok(StatusOutcome {
            status: response.status,
            expires_at,
            owner: None,
            access_node_id: None,
        }),
    }
}

/// 配对 secret（一次性；只在内存里由 ticket 解出）。
fn secret_of(ticket: &PairingTicket) -> Result<PairingSecret, ClientError> {
    PairingSecret::try_from_bytes(ticket.payload.pairing_secret.as_bytes())
        .map_err(|_| ClientError::Pairing("the pairing secret is malformed".to_owned()))
}

fn protocol_version() -> Result<ProtocolVersionV1, ClientError> {
    ProtocolVersionV1::new(1)
        .map_err(|_| ClientError::Pairing("the v1 protocol version is unavailable".to_owned()))
}

fn wire_uuid(text: &str) -> Result<WireUuid, ClientError> {
    WireUuid::parse(text)
        .map_err(|_| ClientError::Pairing("an identity id is not a canonical UUID".to_owned()))
}

fn core_timestamp(text: &WireTimestamp) -> Result<Timestamp, ClientError> {
    Timestamp::parse(text.as_str())
        .map_err(|_| ClientError::Pairing("the timestamp is malformed".to_owned()))
}

fn base64url_text(bytes: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

fn base64url_32(text: &str) -> Result<Base64Url<32>, ClientError> {
    Base64Url::parse(text)
        .map_err(|_| ClientError::Pairing("a 32-byte pairing value is malformed".to_owned()))
}

fn base64url_65(bytes: &[u8; 65]) -> Result<Base64Url<65>, ClientError> {
    Base64Url::parse(&base64url_text(bytes))
        .map_err(|_| ClientError::Pairing("the identity public key is malformed".to_owned()))
}

/// endpoint 的 authority（`wss://<authority>/node-link/v1`）的来源；形状已由 `Endpoint::parse` 冻结。
fn endpoint_authority(endpoint: &Endpoint) -> Option<&str> {
    endpoint
        .as_str()
        .strip_prefix("wss://")
        .and_then(|rest| rest.strip_suffix("/node-link/v1"))
}

/// 拆分 `host[:port]`（支持 `[v6]:port`）。
fn split_authority(authority: &str, default_port: u16) -> Option<(String, u16)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, tail) = rest.split_once(']')?;
        host.parse::<std::net::Ipv6Addr>().ok()?;
        return match tail {
            "" => Some((host.to_owned(), default_port)),
            tail => Some((host.to_owned(), tail.strip_prefix(':')?.parse().ok()?)),
        };
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            Some((host.to_owned(), port.parse::<u16>().ok()?))
        }
        _ => Some((authority.to_owned(), default_port)),
    }
}

/// 明文开发模式下的 loopback 判定（`localhost`、`127.0.0.0/8`、`::1`）。
fn is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(address) => address.is_loopback(),
        Err(_) => false,
    }
}

/// 一次 HTTP 交互的结果。
struct Reply {
    status: u16,
    body: Vec<u8>,
}

/// 把 non-2xx 响应收敛成不含正文的错误（只保留状态与 wire `code`，§13.4）。
fn http_failure(reply: &Reply) -> ClientError {
    let code = serde_json::from_slice::<node_link_protocol::error::Body>(&reply.body)
        .map(|body| body.code.as_str().to_owned())
        .unwrap_or_else(|_| "unknown".to_owned());
    ClientError::Pairing(format!(
        "the owner rejected the pairing request (HTTP {}, code {code})",
        reply.status
    ))
}

/// 最小 HTTP/1.1 `POST`：固定 JSON 内容类型、`Connection: close`、不跟随重定向。
async fn post_json(
    target: HttpTarget,
    path: &str,
    body: &[u8],
    trust: &TlsTrust,
) -> Result<Reply, ClientError> {
    tokio::time::timeout(HTTP_TIMEOUT, async move {
        let tcp = TcpStream::connect((target.host.as_str(), target.port))
            .await
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        tcp.set_nodelay(true)
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        let mut head = format!(
            "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\n\
             Accept: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n",
            authority = target.authority,
            length = body.len(),
        )
        .into_bytes();
        head.extend_from_slice(body);

        let bytes = match target.scheme {
            Scheme::Http => {
                let mut stream = tcp;
                stream
                    .write_all(&head)
                    .await
                    .map_err(|error| ClientError::Transport(error.to_string()))?;
                read_response(&mut stream).await?
            }
            Scheme::Https => {
                let config = tls_client_config(trust)?;
                let connector = tokio_rustls::TlsConnector::from(config);
                let server_name = rustls_pki_types::ServerName::try_from(target.host.clone())
                    .map_err(|_| {
                        ClientError::Pairing("the pairing host is not a TLS name".to_owned())
                    })?;
                let mut stream = connector
                    .connect(server_name, tcp)
                    .await
                    .map_err(|error| ClientError::Transport(error.to_string()))?;
                stream
                    .write_all(&head)
                    .await
                    .map_err(|error| ClientError::Transport(error.to_string()))?;
                read_response(&mut stream).await?
            }
        };
        parse_response(&bytes)
    })
    .await
    .map_err(|_| ClientError::Timeout)?
}

/// 读到 EOF（`Connection: close`），超过 [`MAX_RESPONSE_BYTES`] 即失败关闭。
async fn read_response<S>(stream: &mut S) -> Result<Vec<u8>, ClientError>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(4096);
    let mut chunk = [0u8; 8192];
    loop {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        if read == 0 {
            return Ok(bytes);
        }
        if bytes.len() + read > MAX_RESPONSE_BYTES {
            return Err(ClientError::Pairing(
                "the pairing response exceeds the size limit".to_owned(),
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
}

/// 解析状态行、头与正文（支持 `Content-Length` 与 `chunked`）。
fn parse_response(bytes: &[u8]) -> Result<Reply, ClientError> {
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| ClientError::Pairing("the pairing response is incomplete".to_owned()))?;
    let head = std::str::from_utf8(&bytes[..split])
        .map_err(|_| ClientError::Pairing("the pairing response head is not UTF-8".to_owned()))?;
    let mut lines = head.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| ClientError::Pairing("the pairing response has no status".to_owned()))?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| ClientError::Pairing("the pairing response status is invalid".to_owned()))?;
    let mut content_length = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "content-length" => {
                content_length = Some(value.parse::<usize>().map_err(|_| {
                    ClientError::Pairing("the response content length is invalid".to_owned())
                })?);
            }
            "transfer-encoding" => {
                chunked = value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("chunked"));
            }
            _ => {}
        }
    }
    let payload = &bytes[split + 4..];
    let body = if chunked {
        decode_chunked(payload)?
    } else {
        let length = content_length.unwrap_or(payload.len());
        if payload.len() < length {
            return Err(ClientError::Pairing(
                "the pairing response body is truncated".to_owned(),
            ));
        }
        payload[..length].to_vec()
    };
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(ClientError::Pairing(
            "the pairing response exceeds the size limit".to_owned(),
        ));
    }
    Ok(Reply { status, body })
}

/// `Transfer-Encoding: chunked` 的最小解码（Owner 的正常响应不会用到它，但代理可能重新分块）。
fn decode_chunked(mut payload: &[u8]) -> Result<Vec<u8>, ClientError> {
    let mut body = Vec::new();
    loop {
        let line_end = payload
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(|| ClientError::Pairing("the chunked body is incomplete".to_owned()))?;
        let size_text = std::str::from_utf8(&payload[..line_end])
            .map_err(|_| ClientError::Pairing("the chunk size is not UTF-8".to_owned()))?;
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| ClientError::Pairing("the chunk size is invalid".to_owned()))?;
        payload = &payload[line_end + 2..];
        if size == 0 {
            return Ok(body);
        }
        if payload.len() < size + 2 {
            return Err(ClientError::Pairing(
                "the chunked body is truncated".to_owned(),
            ));
        }
        body.extend_from_slice(&payload[..size]);
        if body.len() > MAX_RESPONSE_BYTES {
            return Err(ClientError::Pairing(
                "the pairing response exceeds the size limit".to_owned(),
            ));
        }
        payload = &payload[size + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER_ID: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
    const PAIRING_ID: &str = "2bc8b944-2a4f-46a7-8c31-b2c40923f60a";

    fn url(authority: &str, payload: &str) -> String {
        format!("https://{authority}{PAIR_PATH}#data={payload}")
    }

    /// P-256 生成元的 SEC1 未压缩编码：测试需要一枚**曲线上的**公钥（`PeerPublicKey` 会校验）。
    const GENERATOR_SEC1: &str = "046b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c2964fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5";

    fn owner_public_key() -> String {
        let bytes: Vec<u8> = GENERATOR_SEC1
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let text = std::str::from_utf8(pair).expect("ASCII");
                u8::from_str_radix(text, 16).expect("十六进制")
            })
            .collect();
        base64url_text(&bytes)
    }

    fn payload_json(owner_public_key: &str, endpoint: &str) -> String {
        format!(
            r#"{{"pairingProtocol":"acp-remote-nodelink-v1","ownerNodeId":"{OWNER_ID}","ownerPublicKey":"{owner_public_key}","endpoint":"{endpoint}","pairingId":"{PAIRING_ID}","pairingSecret":"{}","expiresAt":"2026-10-20T12:15:00.000Z"}}"#,
            base64url_text(&[7u8; 32])
        )
    }

    fn secret_payload(authority: &str) -> String {
        base64url_text(
            payload_json(
                &owner_public_key(),
                &format!("wss://{authority}/node-link/v1"),
            )
            .as_bytes(),
        )
    }

    #[test]
    fn parses_a_v1_invitation_and_derives_the_owner_identity() {
        let ticket = PairingTicket::parse(
            &url("work-pc.example", &secret_payload("work-pc.example")),
            false,
        )
        .expect("ticket");
        assert_eq!(ticket.owner_node_id().expect("owner").as_str(), OWNER_ID);
        assert_eq!(ticket.pairing_id().expect("pairing").as_str(), PAIRING_ID);
        assert_eq!(
            ticket.endpoint().as_str(),
            "wss://work-pc.example/node-link/v1"
        );
        assert_eq!(
            ticket
                .owner_fingerprint()
                .expect("fingerprint")
                .as_str()
                .len(),
            64
        );
    }

    #[test]
    fn rejects_an_endpoint_host_that_does_not_match_the_url() {
        let payload = base64url_text(
            payload_json(&owner_public_key(), "wss://elsewhere.example/node-link/v1").as_bytes(),
        );
        let error = PairingTicket::parse(&url("work-pc.example", &payload), false)
            .expect_err("host mismatch must be rejected");
        assert!(
            matches!(&error, ClientError::Pairing(reason) if reason.contains("endpoint host")),
            "{error}"
        );
    }

    #[test]
    fn rejects_plaintext_without_the_explicit_dev_mode() {
        let payload = secret_payload("127.0.0.1:8765");
        let error = PairingTicket::parse(
            &format!("http://127.0.0.1:8765{PAIR_PATH}#data={payload}"),
            false,
        )
        .expect_err("http must be rejected");
        assert!(matches!(error, ClientError::Pairing(_)));
        PairingTicket::parse(
            &format!("http://127.0.0.1:8765{PAIR_PATH}#data={payload}"),
            true,
        )
        .expect("dev mode allows loopback plaintext");
    }

    #[test]
    fn rejects_a_non_loopback_plaintext_target_even_in_dev_mode() {
        let payload = secret_payload("work-pc.example");
        let error = PairingTicket::parse(
            &url("work-pc.example", &payload).replace("https", "http"),
            true,
        )
        .expect_err("non-loopback plaintext must be rejected");
        assert!(matches!(error, ClientError::Pairing(_)));
    }

    #[test]
    fn rejects_malformed_fragments_and_paths() {
        for url in [
            "https://work-pc.example/node-link/pair".to_owned(),
            "https://work-pc.example/other#data=AAAA".to_owned(),
            "https://work-pc.example/node-link/pair#data=!!!".to_owned(),
            "wss://work-pc.example/node-link/pair#data=AAAA".to_owned(),
        ] {
            assert!(
                PairingTicket::parse(&url, false).is_err(),
                "{url} must be rejected"
            );
        }
    }

    #[test]
    fn expiry_uses_the_fixed_width_timestamp_order() {
        let ticket = PairingTicket::parse(
            &url("work-pc.example", &secret_payload("work-pc.example")),
            false,
        )
        .expect("ticket");
        let before = Timestamp::parse("2026-10-20T12:14:59.999Z").expect("timestamp");
        let after = Timestamp::parse("2026-10-20T12:15:00.001Z").expect("timestamp");
        assert!(!ticket.is_expired(&before));
        assert!(ticket.is_expired(&after));
    }
}
