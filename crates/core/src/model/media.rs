//! 消息通道、类型化媒体、部分结果、分块附件与有效待处理交互
//! （`docs/CORE_PORTS_AND_STORAGE.md` §3.3/§5.8/§7.6；`SYNC_PROTOCOL.md` §10.4/§11.7）。
//!
//! 本模块只放**值对象与纯粹的不变量校验**：
//!
//! - 消息通道（`reply`/`thought`）与部分结果不变量：思考只在 Agent 明确提供时存在，
//!   `partial` 不得被表达成成功。
//! - 媒体引用只标识内容与可用性，**不含** URL、路径或凭据；字节由分块附件连接读取。
//! - 附件传输/清单的主体绑定与状态是封闭词表，与 `owned_attachment_transfer`/
//!   `owned_attachment_submission` 的 CHECK 逐值一致。
//! - [`ElicitationSchema`] 是服务端表单校验的唯一入口：只解释 ACP 表单实际使用的
//!   JSON Schema 子集（object + properties + required + 标量/字符串数组 + enum/范围），
//!   未知或不可表达的**必需**字段使提交整体不可用，绝不以空答案代答。
//!
//! 这里不读系统时间、不访问存储、不依赖 wire DTO。

use std::fmt;
use std::str::FromStr;

use super::elicitation::{ElicitationContentValue, ElicitationValues, parse_value};
use super::error::InvalidValue;
use super::ids::{
    AttachmentId, DeviceId, InteractionId, MessageId, NodeId, SessionId, SubmissionId, TurnId,
    require_bounded,
};
use super::json::{JsonValueText, ViewJson, array_items, decode_json_string, json_object_members};
use super::scalars::{Digest, Sequence, Timestamp, Version};
use super::session::{InteractionKind, InteractionOption};

/// 分块附件的固定单块上限（`SYNC_PROTOCOL.md` §3.4 的 `chunkSize` 常量；不是可协商值）。
pub const ATTACHMENT_CHUNK_SIZE: u32 = 32_768;
/// 附件清单单页上限（`attachment.manifest_page` 每页最多 64 个 `transferId`）。
pub const ATTACHMENT_MANIFEST_PAGE: usize = 64;

token_enum!(
    /// 消息通道（`SYNC_PROTOCOL.md` §10.4）：回复与思考**分别**持久化、分别折叠。
    ///
    /// `User` 是**内部存储通道**（`conversation-reliability` WP2）：它只用于解析
    /// `owned_message.channel='user'` 的行——Owner 自己受理并保存的用户输入（首条与后续同形），
    /// **不是** Agent 回显。该取值**不进** wire 的 `messageChannel` 词表：用户消息在 `session.read`
    /// 的 `messages` 里按 `role=user` 投影、`channel` 键缺席（§3.11/§5.2）。
    MessageChannel {
        Reply => "reply",
        Thought => "thought",
        User => "user",
    }
);

token_enum!(
    /// 单条消息的正文完整性（`owned_message.state` 的 CHECK 取值）。
    MessageState {
        Completed => "completed",
        Partial => "partial",
        Failed => "failed",
        Cancelled => "cancelled",
    }
);

token_enum!(
    /// 部分结果的原因（`SYNC_PROTOCOL.md` §10.4 的 `partialResult.reason`）。
    PartialReason {
        AgentStopped => "agent_stopped",
        AgentFailed => "agent_failed",
        TurnCancelled => "turn_cancelled",
        ConnectionLost => "connection_lost",
        RetentionExpired => "retention_expired",
    }
);

/// turn 终态的正文完整性标记（`SYNC_PROTOCOL.md` §10.4）。`Partial` 表示已收到内容被保留，
/// 但上游未完成：`MUST NOT` 被当成完整成功回复。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialResult {
    pub state: MessageState,
    pub reason: Option<PartialReason>,
}

impl PartialResult {
    /// 正常完成的回复。
    pub const fn completed() -> Self {
        Self {
            state: MessageState::Completed,
            reason: None,
        }
    }

    /// 构造并校验：`Completed` 不得携带失败原因；`Failed`/`Cancelled` **必须**说明原因
    /// （「保留部分但不说为什么」会让客户端无法区分停止、失败与断线）；`Partial` 允许 `None`——
    /// 那是 turn 仍在流式输出、`reason` 尚未产生的正常中间态（`media.schema.json#/$defs/partialResult`
    /// 允许 `reason: null`）。
    pub fn try_new(
        state: MessageState,
        reason: Option<PartialReason>,
    ) -> Result<Self, InvalidValue> {
        match (state, reason) {
            (MessageState::Completed, None) => Ok(Self { state, reason }),
            (MessageState::Completed, Some(_)) => Err(InvalidValue::Field),
            (MessageState::Partial, _) => Ok(Self { state, reason }),
            (_, Some(_)) => Ok(Self { state, reason }),
            (_, None) => Err(InvalidValue::Field),
        }
    }
}

token_enum!(
    /// 媒体类型（`SYNC_PROTOCOL.md` §10.4）。
    MediaKind {
        Image => "image",
        Audio => "audio",
        File => "file",
    }
);

token_enum!(
    /// 媒体可用性（`SYNC_PROTOCOL.md` §10.4）：不可用是显式事实，不得用空字节或空 URL 冒充。
    MediaAvailability {
        Available => "available",
        Expired => "expired",
        NotFetched => "not_fetched",
        Unsupported => "unsupported",
    }
);

/// Owner 侧已封存附件的读取引用：只标识内容与可用性，不含下载 URL、临时路径或凭据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaReference {
    attachment: AttachmentId,
    session: SessionId,
    kind: MediaKind,
    media_type: String,
    byte_length: u64,
    sha256: Digest,
    display_name: Option<String>,
    availability: MediaAvailability,
}

impl MediaReference {
    /// 构造。`media_type` 1..=255、`display_name`（若给出）1..=255。
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        attachment: AttachmentId,
        session: SessionId,
        kind: MediaKind,
        media_type: &str,
        byte_length: u64,
        sha256: Digest,
        display_name: Option<&str>,
        availability: MediaAvailability,
    ) -> Result<Self, InvalidValue> {
        require_bounded(media_type, 1, 255)?;
        if let Some(name) = display_name {
            require_bounded(name, 1, 255)?;
        }
        Ok(Self {
            attachment,
            session,
            kind,
            media_type: media_type.to_owned(),
            byte_length,
            sha256,
            display_name: display_name.map(str::to_owned),
            availability,
        })
    }

    pub fn attachment(&self) -> &AttachmentId {
        &self.attachment
    }

    pub fn session(&self) -> &SessionId {
        &self.session
    }

    pub fn kind(&self) -> MediaKind {
        self.kind
    }

    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    pub fn byte_length(&self) -> u64 {
        self.byte_length
    }

    pub fn sha256(&self) -> &Digest {
        &self.sha256
    }

    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    pub fn availability(&self) -> MediaAvailability {
        self.availability
    }
}

/// 附件传输与清单的主体绑定（`SYNC_PROTOCOL.md` §3.4、`NODE_LINK_PROTOCOL.md` §12.9）。
///
/// 每一帧都重复校验该绑定：跨设备/跨节点/跨会话复用标识返回绑定错误，不泄露内容也不触发 Agent 执行。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AttachmentSubject {
    Device(DeviceId),
    Node { access_node: NodeId },
}

impl AttachmentSubject {
    /// `owned_attachment_transfer.subject_kind` / `owned_attachment_submission.subject_kind`。
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Device(_) => "device",
            Self::Node { .. } => "node",
        }
    }

    /// `owned_attachment_transfer.subject_id` 的文本：设备为设备 UUID，节点为 Access 节点 UUID。
    pub fn id_text(&self) -> &str {
        match self {
            Self::Device(device) => device.as_str(),
            Self::Node { access_node } => access_node.as_str(),
        }
    }

    /// 从已认证主体得到附件绑定主体。
    ///
    /// 只有设备与节点两种远程主体能持有附件：本机 CLI 不参与远程附件帧，配对认领方没有持久身份，
    /// 两者都返回 `None`（调用方据此显式拒绝，而不是挑一个近义主体）。
    pub fn from_actor(actor: &super::identity::Actor) -> Option<Self> {
        match actor {
            super::identity::Actor::Device { device } => Some(Self::Device(device.clone())),
            super::identity::Actor::Node { access_node, .. } => Some(Self::Node {
                access_node: access_node.clone(),
            }),
            super::identity::Actor::LocalCli | super::identity::Actor::PairingClaimant { .. } => {
                None
            }
        }
    }
}

token_enum!(
    /// 临时传输状态（`docs/CORE_PORTS_AND_STORAGE.md` §7.6）。
    AttachmentTransferState {
        Receiving => "receiving",
        Sealed => "sealed",
        Aborted => "aborted",
        Expired => "expired",
    }
);

/// 封存后的附件事实；只在长度与流式摘要校验通过后产生。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedAttachment {
    /// 可读取的附件标识（内容寻址去重后的实际行）。
    pub attachment: AttachmentId,
    pub length: u64,
    pub sha256: Digest,
    pub sealed_at: Timestamp,
}

/// 一次临时传输的状态投影（`attachment.status`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentTransfer {
    pub transfer: AttachmentId,
    pub session: SessionId,
    pub subject: AttachmentSubject,
    pub total_length: u64,
    pub received_length: u64,
    /// 固定 v1 常量（`ATTACHMENT_CHUNK_SIZE`），不是客户端可协商值。
    pub chunk_size: u32,
    pub state: AttachmentTransferState,
    pub sealed: Option<SealedAttachment>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl AttachmentTransfer {
    /// 单块上限是固定 v1 常量。
    pub const fn chunk_size() -> u32 {
        ATTACHMENT_CHUNK_SIZE
    }
}

/// 已封存的附件清单；`attachments` 的顺序即清单顺序，封存后不可追加。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentSubmission {
    pub submission: SubmissionId,
    pub session: SessionId,
    pub subject: AttachmentSubject,
    pub attachments: Vec<AttachmentId>,
    pub digest: Digest,
    pub sealed_at: Timestamp,
}

/// elicitation 的 ACP 表单 schema（原样 JSON object 文本）。
///
/// 只解释 ACP 表单实际使用的 JSON Schema 子集；未知或不可表达的类型**不猜测**：必需字段是未知类型时
/// [`Self::validate_values`] 失败，调用方据此使提交整体不可用并说明原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElicitationSchema(ViewJson);

impl ElicitationSchema {
    /// 校验并保留 schema 原文（必须是 JSON object）。
    pub fn new(text: &str) -> Result<Self, InvalidValue> {
        ViewJson::new(text).map(Self)
    }

    /// schema 原文（逐字节保留）。
    pub fn as_json(&self) -> &str {
        self.0.as_str()
    }

    /// 服务端校验一次 `submit` 的取值。
    ///
    /// 规则（与前端共用同一组用例向量）：
    ///
    /// - `null` 只有在 schema 没有 `required` 字段时可用；
    /// - 每个 `required` 字段都必须出现在 `values` 里；
    /// - 声明了 `type` 的字段按 `string`/`integer`/`number`/`boolean`/`array(string)` 校验；未知类型
    ///   （包括 `object`）对**必需**字段一律拒绝，对可选字段只在用户确实提供该值时拒绝；
    /// - `enum` 按值比较，`minimum`/`maximum` 按数值比较，`minLength`/`maxLength` 对字符串与字符串数组
    ///   按字符/元素数比较。
    pub fn validate_values(&self, values: &ElicitationValues) -> Result<(), InvalidValue> {
        let members = json_object_members(self.0.as_str())?;
        let required = required_fields(&members)?;
        let properties = members
            .iter()
            .find(|(name, _)| name == "properties")
            .map(|(_, raw)| json_object_members(raw))
            .transpose()?
            .unwrap_or_default();
        let Some(fields) = values.fields() else {
            return if required.is_empty() {
                Ok(())
            } else {
                Err(InvalidValue::ElicitationValues)
            };
        };
        for name in &required {
            if !fields.iter().any(|(field, _)| field == name) {
                return Err(InvalidValue::ElicitationValues);
            }
        }
        for (name, value) in fields {
            // 表单是封闭的：schema 没有声明的字段一律拒绝（`additionalProperties` 的默认宽松语义
            // 在这里是有害的——它会让服务端静默接受 Agent 没问、前端也没渲染的输入）。
            let Some((_, schema)) = properties.iter().find(|(property, _)| property == name) else {
                return Err(InvalidValue::ElicitationValues);
            };
            validate_field(schema, value)?;
        }
        Ok(())
    }
}

/// schema 顶层 `required` 字符串数组。
fn required_fields(members: &[(String, &str)]) -> Result<Vec<String>, InvalidValue> {
    let Some((_, raw)) = members.iter().find(|(name, _)| name == "required") else {
        return Ok(Vec::new());
    };
    let items = array_items(raw).ok_or(InvalidValue::Json)?;
    let mut required = Vec::with_capacity(items.len());
    for item in items {
        let value = parse_value(item)?;
        match value {
            ElicitationContentValue::Text(name) => required.push(name),
            _ => return Err(InvalidValue::ElicitationValues),
        }
    }
    Ok(required)
}

fn validate_field(schema: &str, value: &ElicitationContentValue) -> Result<(), InvalidValue> {
    let members = json_object_members(schema)?;
    if let Some((_, raw)) = members.iter().find(|(name, _)| name == "type") {
        let declared = decode_json_string(raw).ok_or(InvalidValue::ElicitationSchema)?;
        if !type_matches(&declared, value) {
            return Err(InvalidValue::ElicitationValues);
        }
    }
    if let Some((_, raw)) = members.iter().find(|(name, _)| name == "enum") {
        let items = array_items(raw).ok_or(InvalidValue::Json)?;
        let canonical = value.to_json_text();
        let mut matched = false;
        for item in items {
            let candidate = parse_value(item)?;
            if candidate.to_json_text() == canonical {
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(InvalidValue::ElicitationValues);
        }
    }
    if let Some((_, raw)) = members.iter().find(|(name, _)| name == "minimum") {
        let bound = number_literal(raw).ok_or(InvalidValue::ElicitationSchema)?;
        if compare_numeric(value, bound) == Some(std::cmp::Ordering::Less) {
            return Err(InvalidValue::ElicitationValues);
        }
    }
    if let Some((_, raw)) = members.iter().find(|(name, _)| name == "maximum") {
        let bound = number_literal(raw).ok_or(InvalidValue::ElicitationSchema)?;
        if compare_numeric(value, bound) == Some(std::cmp::Ordering::Greater) {
            return Err(InvalidValue::ElicitationValues);
        }
    }
    if let Some((_, raw)) = members.iter().find(|(name, _)| name == "minLength") {
        let bound = float_literal(raw).ok_or(InvalidValue::ElicitationSchema)?;
        if length_of(value).ok_or(InvalidValue::ElicitationValues)? < bound {
            return Err(InvalidValue::ElicitationValues);
        }
    }
    if let Some((_, raw)) = members.iter().find(|(name, _)| name == "maxLength") {
        let bound = float_literal(raw).ok_or(InvalidValue::ElicitationSchema)?;
        if length_of(value).ok_or(InvalidValue::ElicitationValues)? > bound {
            return Err(InvalidValue::ElicitationValues);
        }
    }
    Ok(())
}

fn type_matches(declared: &str, value: &ElicitationContentValue) -> bool {
    match declared {
        "string" => matches!(value, ElicitationContentValue::Text(_)),
        "integer" => matches!(value, ElicitationContentValue::Integer(_)),
        "number" => matches!(
            value,
            ElicitationContentValue::Integer(_) | ElicitationContentValue::Number(_)
        ),
        "boolean" => matches!(value, ElicitationContentValue::Boolean(_)),
        "array" => matches!(value, ElicitationContentValue::TextArray(_)),
        // 未知类型（含 `object`）无法用 ACP 的六种取值无损表达：不猜测，按不匹配处理。
        _ => false,
    }
}

/// schema 里的数值边界：整数字面量保留精确值，只有非整数（含小数/指数）才落到 `f64`。
#[derive(Debug, Clone, Copy)]
enum NumericBound {
    Exact(i128),
    Float(f64),
}

fn compare_numeric(
    value: &ElicitationContentValue,
    bound: NumericBound,
) -> Option<std::cmp::Ordering> {
    match (value, bound) {
        // 两侧都是精确整数：用 `i128` 比较，超过 2^53 也不失真。
        (ElicitationContentValue::Integer(actual), NumericBound::Exact(bound)) => {
            Some((*actual as i128).cmp(&bound))
        }
        (ElicitationContentValue::Integer(actual), NumericBound::Float(bound)) => {
            (*actual as f64).partial_cmp(&bound)
        }
        (ElicitationContentValue::Number(actual), NumericBound::Exact(bound)) => {
            actual.partial_cmp(&(bound as f64))
        }
        (ElicitationContentValue::Number(actual), NumericBound::Float(bound)) => {
            actual.partial_cmp(&bound)
        }
        _ => None,
    }
}

fn length_of(value: &ElicitationContentValue) -> Option<f64> {
    match value {
        ElicitationContentValue::Text(text) => Some(text.chars().count() as f64),
        ElicitationContentValue::TextArray(items) => Some(items.len() as f64),
        _ => None,
    }
}

/// 整数字面量（无 `.`/`e`/`E`）按 `i128` 解析；其余回退 `f64`（`minLength`/`maxLength` 也走这里）。
fn number_literal(raw: &str) -> Option<NumericBound> {
    let raw = raw.trim();
    if !raw.contains(['.', 'e', 'E'])
        && let Ok(value) = raw.parse::<i128>()
    {
        return Some(NumericBound::Exact(value));
    }
    float_literal(raw).map(NumericBound::Float)
}

fn float_literal(raw: &str) -> Option<f64> {
    let value: f64 = raw.trim().parse().ok()?;
    value.is_finite().then_some(value)
}

/// 有效待处理交互（`SYNC_PROTOCOL.md` §11.7）：服务端在本次读取时确认过的完整当前请求。
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveInteraction {
    pub first_sequence: Option<Sequence>,
    pub first_origin_sequence: Option<Sequence>,
    pub interaction: InteractionId,
    pub session: SessionId,
    pub kind: InteractionKind,
    pub request_version: Version,
    pub expires_at: Option<Timestamp>,
    pub options: Vec<InteractionOption>,
    pub schema: Option<ElicitationSchema>,
}

impl EffectiveInteraction {
    /// 构造并校验：`request_version` 至少为 1（0 不是可回传的有效版本）。
    pub fn try_new(
        interaction: InteractionId,
        session: SessionId,
        kind: InteractionKind,
        request_version: Version,
        expires_at: Option<Timestamp>,
        options: Vec<InteractionOption>,
        schema: Option<ElicitationSchema>,
    ) -> Result<Self, InvalidValue> {
        if request_version.get() == 0 {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            first_sequence: None,
            first_origin_sequence: None,
            interaction,
            session,
            kind,
            request_version,
            expires_at,
            options,
            schema,
        })
    }

    /// 本次 `submit` 的取值是否符合当前表单 schema（`permission` 恒通过）。
    pub fn validate_submission(&self, values: &ElicitationValues) -> Result<(), InvalidValue> {
        match (&self.kind, &self.schema) {
            (InteractionKind::Elicitation, Some(schema)) => schema.validate_values(values),
            (InteractionKind::Elicitation, None) => {
                // 没有 schema 的 elicitation 只允许 `null`/空对象：不得凭空接受任意字段。
                if values.fields().is_some_and(|fields| !fields.is_empty()) {
                    Err(InvalidValue::ElicitationValues)
                } else {
                    Ok(())
                }
            }
            (InteractionKind::Permission, _) => Ok(()),
        }
    }
}

/// 一条已持久化的消息正文（回复或思考）。
///
/// 正文的权威只在 `owned_message`（`docs/CORE_PORTS_AND_STORAGE.md` §7.6）：历史读不再从事件或快照
/// 重复拼接正文；事件只承载增量与收尾收据。
#[derive(Debug, Clone, PartialEq)]
pub struct MessageRecord {
    /// First durable event position; absent only for unprovable legacy history.
    pub first_sequence: Option<Sequence>,
    /// Owner origin position, independent of the session position.
    pub first_origin_sequence: Option<Sequence>,
    pub message: MessageId,
    pub session: SessionId,
    pub turn: Option<TurnId>,
    pub channel: MessageChannel,
    pub result: PartialResult,
    /// 折叠后的 content block 数组原文（JSON array 文本，可能为空数组）。
    pub content: JsonValueText,
    /// 该消息引用的媒体（只存引用，不含字节）。
    pub media: Vec<MediaReference>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl MessageRecord {
    /// 构造（内容必须是良构 JSON 值文本；空消息用 `[]`）。
    #[allow(clippy::too_many_arguments)] // 字段与存储行一一对应；构造器只在这一处聚合。
    pub fn try_new(
        message: MessageId,
        session: SessionId,
        turn: Option<TurnId>,
        channel: MessageChannel,
        result: PartialResult,
        content: JsonValueText,
        media: Vec<MediaReference>,
        created_at: Timestamp,
        updated_at: Timestamp,
    ) -> Result<Self, InvalidValue> {
        Ok(Self {
            first_sequence: None,
            first_origin_sequence: None,
            message,
            session,
            turn,
            channel,
            result,
            content,
            media,
            created_at,
            updated_at,
        })
    }
}
