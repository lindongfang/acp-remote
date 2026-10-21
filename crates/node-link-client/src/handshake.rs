//! Access 侧的 `node.hello` → `node.challenge` → `node.proof` → `node.ready`（`NODE_LINK_PROTOCOL.md`
//! §2/§12.2）。
//!
//! 三条不变量：
//!
//! 1. **Owner 公钥只来自本机信任记录**：调用方传入 [`OwnerTrust`]，本模块不接受挑战消息里自报的公钥。
//! 2. **challenge 先验签、再回 proof**：`node.challenge.nodeProof` 的 transcript 由 `identity-auth` 的
//!    `NodeLinkChallenge` 装配（domain 与 tag 表唯一）。验签失败立即中止，不发出任何 Access 证明。
//! 3. **required feature 未选中即中止**：握手不得进入业务阶段；错误码与 `link.error` 的语义由调用方映射
//!    （§11.3）。
//!
//! 认证前信封**不得**携带 `connectionId`/`connectionSequence`；`node.ready` 是认证后的第一条消息，必须
//! 携带两者。本模块用 `Envelope::decode_in_phase` 在解码期就拒绝错误阶段的消息。

use std::sync::Arc;
use std::time::Duration;

use acp_core::model::{NodeId, PeerPublicKey, Timestamp};
use identity_auth::{NodeLinkChallenge, NodeLinkProof, P1363Signature};
use node_link_protocol::common::{Base64Url, FeatureList, NodeLinkLimits};
use node_link_protocol::common::{DecimalString, ProtocolVersionV1, Uuid};
use node_link_protocol::envelope::{ConnectionFields, Envelope, MessageType, Phase, encode_body};
use node_link_protocol::error::{Body as LinkErrorBody, ErrorCode};
use node_link_protocol::handshake::{NodeChallenge, NodeHello, NodeProof, NodeReady};
use uuid::Uuid as RandomUuid;

use crate::error::ClientError;
use crate::identity::NodeIdentity;
use crate::transport::{Incoming, MessageSink, MessageSource};

/// 单次握手的上限（`NODE_LINK_PROTOCOL.md` §2.5：15 秒）。业务连接与独立附件连接共用。
///
/// 明文/TLS/TCP 连接建立另有 [`crate::transport`] 的 10 秒上限；这个常量约束的是**升级完成后**
/// 的四条握手消息，避免对端接受连接却不再说话时调用方无限等待。
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// `node-link.core.v1`（`compatibility/features/v1/features.json`）。
pub const CORE_FEATURE: &str = "node-link.core.v1";

/// `core.session-activity.v1`：会话状态只有 `idle|running|pending` 三态。
///
/// 该 feature 在登记表里是 `required`：Owner 未选中它时，编解码层会把三态之外的旧值当成非法，
/// 因此握手阶段必须失败关闭（`NODE_LINK_PROTOCOL.md` §11.3）。
pub const SESSION_ACTIVITY_FEATURE: &str = "core.session-activity.v1";

/// `node-link.raw-acp.v1`。
pub const RAW_ACP_FEATURE: &str = "node-link.raw-acp.v1";

/// `node-link.command-status.v1`。
pub const COMMAND_STATUS_FEATURE: &str = "node-link.command-status.v1";

/// `node-link.session-create.v1`。
pub const SESSION_CREATE_FEATURE: &str = "node-link.session-create.v1";

/// `node-link.attachment-transfer.v1`。
pub const ATTACHMENT_FEATURE: &str = "node-link.attachment-transfer.v1";

/// `node-link.typed-media.v1`。
pub const TYPED_MEDIA_FEATURE: &str = "node-link.typed-media.v1";

/// `node-link.pending-interactions.v1`。
pub const PENDING_INTERACTIONS_FEATURE: &str = "node-link.pending-interactions.v1";

/// `node-link.remote-history.v1`。
pub const REMOTE_HISTORY_FEATURE: &str = "node-link.remote-history.v1";

/// `core.session-title.v1`：Owner 元数据命令 `session.title.set` 与有效标题投影。
///
/// 与 Sync 面共用同一 ID（`compatibility/features/v1/features.json`）。Access 只在已协商的连接上转发
/// 该命令；未协商时 `RemoteTitleWrite` 显式不可用（不落盘、不排队）。
pub const SESSION_TITLE_FEATURE: &str = "core.session-title.v1";

/// 本 crate 声明的 `supportedFeatures`（顺序无关；`FeatureList` 会排序）。
///
/// 每一条都对应当前真实实现的客户端行为：附件转发（有界内存）、类型化媒体映射、有效交互读取与远程
/// 历史分页。未实现的能力不在此列——能力协商不得虚报（`AGENTS.md` §4）。
pub const SUPPORTED_FEATURES: [&str; 11] = [
    CORE_FEATURE,
    "resource.stateless-origin.v1",
    SESSION_ACTIVITY_FEATURE,
    RAW_ACP_FEATURE,
    COMMAND_STATUS_FEATURE,
    SESSION_CREATE_FEATURE,
    ATTACHMENT_FEATURE,
    TYPED_MEDIA_FEATURE,
    PENDING_INTERACTIONS_FEATURE,
    REMOTE_HISTORY_FEATURE,
    // `core.session-title.v1`：Access 只转发 `session.title.set`、不保存标题或命令状态
    // （`MODULE_ARCHITECTURE.md` 的 stateless-access 取舍）。未选中时 `RemoteTitleWrite` 显式不可用。
    SESSION_TITLE_FEATURE,
];

/// 没有这些 feature 就没有可用的业务语义（§11：Owner 未选中时必须中止）。
///
/// `core.session-activity.v1` 与 `node-link.core.v1` 一起是注册表里的 `required`：三态摘要
/// 是 catalog/事件投影的既定语义，缺少它时旧七态会以非法值进入 core。
pub const REQUIRED_FEATURES: [&str; 2] = [CORE_FEATURE, SESSION_ACTIVITY_FEATURE];

/// Owner 的已配对身份（来自本机 `TrustStore`，**不**取自 wire）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerTrust {
    /// Owner node id。
    pub node_id: NodeId,
    /// Owner 的 65 字节 SEC1 公钥。
    pub public_key: PeerPublicKey,
}

/// 握手输入。
#[derive(Clone)]
pub struct HandshakeConfig {
    /// Access 的节点身份（签名端口）。
    pub identity: Arc<dyn NodeIdentity>,
    /// Owner 的已配对身份。
    pub owner: OwnerTrust,
    /// 本端声明的 feature。
    pub supported_features: Vec<String>,
    /// 本端要求 Owner 必须选中的 feature。
    pub required_features: Vec<String>,
}

impl std::fmt::Debug for HandshakeConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HandshakeConfig")
            .field("owner", &self.owner.node_id)
            .field("supported_features", &self.supported_features)
            .field("required_features", &self.required_features)
            .finish_non_exhaustive()
    }
}

impl HandshakeConfig {
    /// 使用默认能力集合（[`SUPPORTED_FEATURES`] / [`REQUIRED_FEATURES`]）。
    pub fn new(identity: Arc<dyn NodeIdentity>, owner: OwnerTrust) -> Self {
        Self {
            identity,
            owner,
            supported_features: SUPPORTED_FEATURES
                .iter()
                .map(|feature| (*feature).to_owned())
                .collect(),
            required_features: REQUIRED_FEATURES
                .iter()
                .map(|feature| (*feature).to_owned())
                .collect(),
        }
    }
}

/// 握手成功后的连接事实。
#[derive(Debug, Clone)]
pub struct Established {
    /// 本次连接标识（信封与 `resource.*` 共用）。
    pub connection_id: Uuid,
    /// 下一条出站消息的 `connectionSequence`（认证完成后从 1 开始）。
    pub next_sequence: u64,
    /// 下一条入站消息的 `connectionSequence`。
    ///
    /// 握手已经消费了 Owner 的认证后序号 1（`node.ready`），因此进入业务/附件阶段时它是 2。
    /// 附件连接用它初始化入站校验；若从 1 开始，Owner 的第一条回复会被误判为序号违规。
    pub inbound_next: u64,
    /// Owner 选中的 feature（`node.challenge.selectedFeatures` 的同值）。
    pub selected_features: Vec<String>,
    /// Owner 下发的限额（只会比本端配置更小）。
    pub limits: NodeLinkLimits,
    /// Owner 的事件保留 epoch。
    pub server_epoch: Uuid,
    /// Owner 当前 catalog revision。
    pub catalog_revision: DecimalString,
}

impl Established {
    /// 本次协商是否包含某个 feature。
    pub fn has(&self, feature: &str) -> bool {
        self.selected_features
            .iter()
            .any(|selected| selected == feature)
    }
}

/// 执行客户端握手。
///
/// `on_ping` 不在这里处理：握手阶段只有四条消息；若对端提前发来其它类型，一律按协议违规中止。
pub async fn handshake(
    sink: &mut dyn MessageSink,
    source: &mut dyn MessageSource,
    config: &HandshakeConfig,
) -> Result<Established, ClientError> {
    let client_nonce = random_bytes_32()?;
    let hello = NodeHello {
        min_protocol_version: protocol_version()?,
        max_protocol_version: protocol_version()?,
        access_node_id: wire_uuid(config.identity.node_id().as_str())?,
        role: node_link_protocol::common::NodeKind::Access,
        client_nonce: Base64Url::<32>::parse(&base64url(&client_nonce))
            .map_err(|_| ClientError::Local("client nonce cannot be encoded"))?,
        supported_features: wire_feature_list(&config.supported_features)?,
        required_features: wire_feature_list(&config.required_features)?,
    };
    send_pre_auth(sink, MessageType::NodeHello, &hello).await?;

    let challenge: NodeChallenge = receive_pre_auth(source, MessageType::NodeChallenge).await?;
    let challenge_connection = challenge.connection_id.clone();
    if challenge.owner_node_id.as_str() != config.owner.node_id.as_str() {
        return Err(ClientError::Handshake(
            "the challenge belongs to a different owner node".to_owned(),
        ));
    }
    let selected: Vec<String> = challenge
        .selected_features
        .as_slice()
        .iter()
        .map(|feature| feature.as_str().to_owned())
        .collect();
    for required in &config.required_features {
        // 只校验「本端要求」的那一批；supported 里其余条目允许 Owner 不选（显式降级）。
        if !selected.iter().any(|entry| entry == required) {
            return Err(ClientError::Handshake(format!(
                "the owner did not select required feature {required}"
            )));
        }
    }
    if selected.iter().any(|entry| {
        !config
            .supported_features
            .iter()
            .any(|feature| feature == entry)
    }) {
        // Owner 不得选中本端未声明的 feature（能力协商是交集，见 §11）。
        return Err(ClientError::Handshake(
            "the owner selected a feature this client did not declare".to_owned(),
        ));
    }

    let catalog_revision = challenge
        .catalog_revision
        .as_str()
        .parse::<u64>()
        .map_err(|_| ClientError::Handshake("catalog revision is not a u64".to_owned()))?;
    let access_node_id = config.identity.node_id().clone();

    // Owner 挑战签名：transcript 的字段集合由 `identity-auth` 唯一装配。
    let challenge_input = NodeLinkChallenge {
        owner_node_id: config.owner.node_id.clone(),
        access_node_id: access_node_id.clone(),
        catalog_revision,
        client_nonce: nonce(&client_nonce)?,
        server_nonce: nonce(challenge.server_nonce.as_bytes())?,
        connection_id: challenge_id(challenge_connection.as_str())?,
        negotiated_features: feature_list(&selected),
    };
    let signature = P1363Signature::try_from_base64url(&base64url(challenge.node_proof.as_bytes()))
        .map_err(|_| ClientError::Handshake("owner proof is not a 64-byte signature".to_owned()))?;
    challenge_input
        .verify(&config.owner.public_key, &signature)
        .map_err(|_| ClientError::Handshake("owner challenge proof does not verify".to_owned()))?;

    // Access 证明：同一批字段（domain 不同），签名经 keystore 端口。
    let proof_input = NodeLinkProof {
        owner_node_id: config.owner.node_id.clone(),
        access_node_id,
        catalog_revision,
        client_nonce: nonce(&client_nonce)?,
        server_nonce: nonce(challenge.server_nonce.as_bytes())?,
        connection_id: challenge_id(challenge_connection.as_str())?,
        negotiated_features: feature_list(&selected),
    };
    let transcript = proof_input
        .transcript()
        .map_err(|_| ClientError::Local("node proof transcript cannot be assembled"))?;
    let access_signature = config.identity.sign(&transcript).await?;
    let proof = NodeProof {
        connection_id: challenge_connection.clone(),
        access_node_id: wire_uuid(config.identity.node_id().as_str())?,
        node_proof: Base64Url::<64>::parse(&access_signature.to_base64url())
            .map_err(|_| ClientError::Local("node proof cannot be encoded"))?,
    };
    send_pre_auth(sink, MessageType::NodeProof, &proof).await?;

    // `node.ready`：认证后第一条消息，信封必须带连接字段。
    // 对端的 `connectionSequence` 从 1 开始；本端的出站序号是**另一个方向**的计数，也从 1 开始。
    let ready: NodeReady = receive_post_auth(
        sink,
        source,
        &challenge_connection,
        1,
        MessageType::NodeReady,
    )
    .await?;
    let next_sequence = 1;
    if ready.owner_node_id.as_str() != config.owner.node_id.as_str() {
        return Err(ClientError::Handshake(
            "node.ready names a different owner node".to_owned(),
        ));
    }
    if ready.server_epoch.as_str().is_empty() {
        return Err(ClientError::Handshake(
            "node.ready carries an empty server epoch".to_owned(),
        ));
    }
    Ok(Established {
        connection_id: challenge_connection,
        next_sequence,
        // `node.ready` 是 Owner 的第一条认证后消息，已占用序号 1。
        inbound_next: 2,
        selected_features: selected,
        limits: ready.limits,
        server_epoch: ready.server_epoch,
        catalog_revision: ready.catalog_revision,
    })
}

/// 生成 32 字节客户端 nonce（系统 CSPRNG；失败即失败关闭）。
pub fn random_bytes_32() -> Result<[u8; 32], ClientError> {
    let first = RandomUuid::new_v4();
    let second = RandomUuid::new_v4();
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(first.as_bytes());
    bytes[16..].copy_from_slice(second.as_bytes());
    Ok(bytes)
}

/// 生成 16 字节 ping nonce。
pub fn random_bytes_16() -> Result<[u8; 16], ClientError> {
    Ok(*RandomUuid::new_v4().as_bytes())
}

/// 无填充 base64url（只用于把本模块产生的字节装进 wire 值对象）。
fn base64url(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn wire_uuid(text: &str) -> Result<Uuid, ClientError> {
    Uuid::parse(text).map_err(|_| ClientError::Local("a local identifier is not a canonical uuid"))
}

fn protocol_version() -> Result<ProtocolVersionV1, ClientError> {
    ProtocolVersionV1::new(1).map_err(|_| ClientError::Local("protocol version 1 is unavailable"))
}

fn nonce(bytes: &[u8]) -> Result<acp_core::model::Nonce, ClientError> {
    acp_core::model::Nonce::new(&base64url(bytes))
        .map_err(|_| ClientError::Local("nonce cannot be encoded"))
}

fn challenge_id(text: &str) -> Result<identity_auth::ChallengeId, ClientError> {
    identity_auth::ChallengeId::parse(text)
        .map_err(|_| ClientError::Local("connection id cannot be used as a challenge id"))
}

fn feature_list(features: &[String]) -> identity_auth::FeatureList {
    identity_auth::FeatureList::new(features.iter().cloned())
}

/// wire 的 `featureList` 是 `FeatureId` 的数组（不排序）；取值域校验在此完成。
fn wire_feature_list(features: &[String]) -> Result<FeatureList, ClientError> {
    let items = features
        .iter()
        .map(|feature| {
            node_link_protocol::common::FeatureId::parse(feature)
                .map_err(|_| ClientError::Local("a feature id is outside the wire domain"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    FeatureList::new(items).map_err(|_| ClientError::Local("a feature list is not encodable"))
}

async fn send_pre_auth<T: serde::Serialize>(
    sink: &mut dyn MessageSink,
    message_type: MessageType,
    body: &T,
) -> Result<(), ClientError> {
    let body =
        encode_body(body).map_err(|_| ClientError::Local("message body cannot be encoded"))?;
    let envelope = Envelope::new(
        message_type,
        wire_uuid(&RandomUuid::new_v4().to_string())?,
        None,
        body,
    )
    .map_err(|_| ClientError::Local("message envelope cannot be assembled"))?;
    let text = envelope
        .encode()
        .map_err(|_| ClientError::Local("message envelope cannot be encoded"))?;
    sink.send(text).await
}

async fn receive_pre_auth<T: serde::de::DeserializeOwned>(
    source: &mut dyn MessageSource,
    expected: MessageType,
) -> Result<T, ClientError> {
    let text = next_text(source).await?;
    let envelope = Envelope::decode_in_phase(&text, Phase::PreAuth)
        .map_err(|error| ClientError::Protocol(error.to_string()))?;
    decode_expected(envelope, expected)
}

async fn receive_post_auth<T: serde::de::DeserializeOwned>(
    sink: &mut dyn MessageSink,
    source: &mut dyn MessageSource,
    connection_id: &Uuid,
    next_sequence: u64,
    expected: MessageType,
) -> Result<T, ClientError> {
    let text = next_text(source).await?;
    let envelope = Envelope::decode_in_phase(&text, Phase::PostAuth)
        .map_err(|error| ClientError::Protocol(error.to_string()))?;
    match envelope.message_type() {
        MessageType::LinkError => return Err(link_error(&envelope)),
        actual if actual == expected => {}
        actual => {
            return Err(ClientError::Protocol(format!(
                "expected {expected} during the handshake, received {actual}"
            )));
        }
    }
    let Some(fields) = envelope.connection() else {
        return Err(ClientError::Protocol(
            "an authenticated message is missing connection fields".to_owned(),
        ));
    };
    if fields.connection_id != *connection_id {
        return Err(ClientError::Protocol(
            "the connection id changed during the handshake".to_owned(),
        ));
    }
    if fields.connection_sequence.as_str() != next_sequence.to_string() {
        return Err(ClientError::Protocol(
            "the owner connection sequence is not monotonic".to_owned(),
        ));
    }
    let body = serde_json::from_str(envelope.body().get())
        .map_err(|_| ClientError::Protocol("the handshake body is malformed".to_owned()))?;
    let _ = sink;
    Ok(body)
}

fn decode_expected<T: serde::de::DeserializeOwned>(
    envelope: Envelope,
    expected: MessageType,
) -> Result<T, ClientError> {
    match envelope.message_type() {
        MessageType::LinkError => Err(link_error(&envelope)),
        actual if actual == expected => serde_json::from_str(envelope.body().get())
            .map_err(|_| ClientError::Protocol("the handshake body is malformed".to_owned())),
        actual => Err(ClientError::Protocol(format!(
            "expected {expected} during the handshake, received {actual}"
        ))),
    }
}

async fn next_text(source: &mut dyn MessageSource) -> Result<String, ClientError> {
    match source.recv().await? {
        Incoming::Text(text) => Ok(text),
        Incoming::Closed(_) => Err(ClientError::Closed),
    }
}

/// `link.error` → [`ClientError`]。未识别的码保留为协议错误（不伪造具体语义）。
fn link_error(envelope: &Envelope) -> ClientError {
    let Ok(body) = serde_json::from_str::<LinkErrorBody>(envelope.body().get()) else {
        return ClientError::Protocol("the owner sent an unreadable link.error".to_owned());
    };
    match body.code {
        ErrorCode::AuthNodeRevoked => ClientError::Revoked,
        ErrorCode::ProtocolFeatureRequired
        | ErrorCode::ProtocolVersionUnsupported
        | ErrorCode::AuthNodeUnknown => {
            ClientError::Handshake(format!("{}: {}", body.code.as_str(), body.message.as_str()))
        }
        _ => ClientError::Protocol(format!("{}: {}", body.code.as_str(), body.message.as_str())),
    }
}

/// 时钟无关的时间戳来源：调用方必须传入（本 crate 不读系统时间之外的业务状态，
/// 但传输层超时需要 `tokio::time`，这与 core 不在同一层）。
pub fn wire_timestamp(
    at: &Timestamp,
) -> Result<node_link_protocol::common::Timestamp, ClientError> {
    node_link_protocol::common::Timestamp::parse(at.as_str())
        .map_err(|_| ClientError::Local("a timestamp cannot be encoded for the wire"))
}

/// 把 core 的 `SessionState` 映射到 wire 的同一词表（两处都是封闭枚举，本函数是唯一映射点）。
pub fn wire_connection_fields(
    connection_id: &Uuid,
    sequence: u64,
) -> Result<ConnectionFields, ClientError> {
    Ok(ConnectionFields {
        connection_id: connection_id.clone(),
        connection_sequence: DecimalString::parse(&sequence.to_string())
            .map_err(|_| ClientError::Local("connection sequence cannot be encoded"))?,
    })
}
