//! 标识、引用与实体定位（`docs/CORE_PORTS_AND_STORAGE.md` §3.1）。
//!
//! 所有 ID 都是 newtype，只能通过构造校验得到合法值：UUID 类要求 canonical 小写文本，字符串类要求各自的
//! 字符集与长度上限。非法输入返回 [`InvalidValue`]（`PortError::InvalidRequest` 的来源）。

use std::fmt;
use std::str::FromStr;

use super::error::InvalidValue;

// ---------------------------------------------------------------- 校验谓词

/// 字符数（Unicode code point），与 JSON Schema 的 `minLength`/`maxLength` 口径一致。
pub(crate) fn char_count(text: &str) -> usize {
    text.chars().count()
}

/// `min..=max` 字符数校验，区分“空”与“过长”两种具名错误。
pub(crate) fn require_bounded(text: &str, min: usize, max: usize) -> Result<(), InvalidValue> {
    let count = char_count(text);
    if count < min {
        Err(InvalidValue::Empty)
    } else if count > max {
        Err(InvalidValue::TooLong { max })
    } else {
        Ok(())
    }
}

/// ASCII 字符集模式：`first` 约束首字节，`rest` 约束其余字节。
pub(crate) fn charset_ok(
    text: &str,
    min: usize,
    max: usize,
    first: fn(u8) -> bool,
    rest: fn(u8) -> bool,
) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < min || bytes.len() > max {
        return false;
    }
    match bytes.split_first() {
        Some((head, tail)) => first(*head) && tail.iter().all(|byte| rest(*byte)),
        None => false,
    }
}

fn ascii_alnum(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
}

fn spec_rest(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
}

fn lower_alnum(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

fn lower_spec_rest(byte: u8) -> bool {
    lower_alnum(byte) || matches!(byte, b'.' | b'_' | b'-')
}

/// `^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`
pub(crate) fn is_uuid(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    bytes.iter().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            *byte == b'-'
        } else {
            matches!(byte, b'0'..=b'9' | b'a'..=b'f')
        }
    })
}

/// `^[A-Za-z0-9._-]{1,128}$`（`ExportId`/`ImportId`）
pub(crate) fn is_spec_id(text: &str) -> bool {
    charset_ok(text, 1, 128, ascii_alnum, spec_rest)
}

/// `^[a-z0-9][a-z0-9._-]{0,63}$`
pub(crate) fn is_workspace_alias(text: &str) -> bool {
    charset_ok(text, 1, 64, lower_alnum, lower_spec_rest)
}

/// `^[A-Za-z0-9._-]{1,128}$`（权威：`schemas/node-link/v1/common.schema.json#/$defs/templateId`）。
pub(crate) fn is_template_id(text: &str) -> bool {
    charset_ok(text, 1, 128, ascii_alnum, spec_rest)
}

/// `^[a-z0-9_.-]{1,128}$`
pub(crate) fn is_event_type(text: &str) -> bool {
    charset_ok(text, 1, 128, lower_spec_rest, lower_spec_rest)
}

/// `^[a-z][a-z0-9._-]{0,127}$`（scope 名等于命令名，`docs/SECURITY_DESIGN.md` §10.2）
pub(crate) fn is_scope_name(text: &str) -> bool {
    charset_ok(
        text,
        1,
        128,
        |byte| byte.is_ascii_lowercase(),
        lower_spec_rest,
    )
}

/// `^grant\.[a-z][a-z0-9._-]{0,63}$`
pub(crate) fn is_grant_name(text: &str) -> bool {
    match text.strip_prefix("grant.") {
        Some(rest) => charset_ok(rest, 1, 64, lower_alnum, lower_spec_rest),
        None => false,
    }
}

/// `^[a-z][a-z0-9._-]{0,127}$`（`PublicError.code` 与 wire 错误码同族）
pub(crate) fn is_error_code(text: &str) -> bool {
    charset_ok(
        text,
        1,
        128,
        |byte| byte.is_ascii_lowercase(),
        lower_spec_rest,
    )
}

/// 64 字符小写十六进制（SHA-256 指纹的既定编码，`docs/LOCAL_ADMIN_PROTOCOL.md` §5.3）
pub(crate) fn is_fingerprint(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 64
        && bytes
            .iter()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// 非空 `wss://` 端点，长度上限 2048。
pub(crate) fn is_endpoint(text: &str) -> bool {
    text.starts_with("wss://") && text.len() > "wss://".len() && char_count(text) <= 2048
}

fn check_uuid(text: &str) -> Result<(), InvalidValue> {
    if is_uuid(text) {
        Ok(())
    } else {
        Err(InvalidValue::Uuid)
    }
}

fn check_export_id(text: &str) -> Result<(), InvalidValue> {
    if is_spec_id(text) {
        Ok(())
    } else {
        Err(InvalidValue::ExportId)
    }
}

fn check_import_id(text: &str) -> Result<(), InvalidValue> {
    if is_spec_id(text) {
        Ok(())
    } else {
        Err(InvalidValue::ImportId)
    }
}

fn check_workspace_alias(text: &str) -> Result<(), InvalidValue> {
    if is_workspace_alias(text) {
        Ok(())
    } else {
        Err(InvalidValue::WorkspaceAlias)
    }
}

fn check_template_id(text: &str) -> Result<(), InvalidValue> {
    if is_template_id(text) {
        Ok(())
    } else {
        Err(InvalidValue::TemplateId)
    }
}

fn check_fingerprint(text: &str) -> Result<(), InvalidValue> {
    if is_fingerprint(text) {
        Ok(())
    } else {
        Err(InvalidValue::Fingerprint)
    }
}

fn check_agent_id(text: &str) -> Result<(), InvalidValue> {
    require_bounded(text, 1, 128).map_err(|_| InvalidValue::AgentId)
}

fn check_mode_id(text: &str) -> Result<(), InvalidValue> {
    require_bounded(text, 1, 256)
}

fn check_config_option_id(text: &str) -> Result<(), InvalidValue> {
    require_bounded(text, 1, 256)
}

// ---------------------------------------------------------------- newtype 骨架

newtype!(
    /// 节点标识（canonical 小写 UUID 文本，`SYNC_PROTOCOL.md` §3.2）。
    NodeId,
    check_uuid
);
newtype!(
    /// 设备标识（canonical 小写 UUID 文本）。
    DeviceId,
    check_uuid
);
newtype!(
    /// 会话标识。owned 会话由 `SessionStore::commit` 在创建事务内分配（§3.1）。
    SessionId,
    check_uuid
);
newtype!(
    /// 事件标识；owned 事件由 `SessionStore::commit` 在提交事务内分配，imported 事件必须等于 origin
    /// 事件的 id（§3.1/§3.4）。
    EventId,
    check_uuid
);
newtype!(
    /// 客户端请求标识，由 `IdGenerator` 在调用方分配；幂等键的一半（§6 第 6 条）。
    RequestId,
    check_uuid
);
newtype!(
    /// turn 标识，由 core 的 `IdGenerator` 在派发前分配（§3.1 的 id 分配规则）。
    TurnId,
    check_uuid
);
newtype!(
    /// 交互标识（permission 与 elicitation 共用），由 core 的 `IdGenerator` 分配（§3.1）。
    InteractionId,
    check_uuid
);
newtype!(
    /// 一次性配对标识，由 core 的 `IdGenerator` 分配（§3.1）。
    PairingId,
    check_uuid
);
newtype!(
    /// 附件标识，由 `AttachmentStore::put` 分配并返回（§3.1；`IdGenerator` 不再提供 `attachment_id`）。
    AttachmentId,
    check_uuid
);
newtype!(
    /// 事件库 epoch，首次创建时生成、普通升级与重启保持不变（`SYNC_PROTOCOL.md` §9.1）。
    ServerEpoch,
    check_uuid
);
newtype!(
    /// 会话级 origin epoch：owned 由 core 的 `IdGenerator` 在创建会话时生成并传入 `commit`（§3.1/§5.2）。
    OriginEpoch,
    check_uuid
);
newtype!(
    /// Export 标识：`^[A-Za-z0-9._-]{1,128}$`，**不是** UUID（`NODE_LINK_PROTOCOL.md` §12.3）。
    ExportId,
    check_export_id
);
newtype!(
    /// Import 标识：`^[A-Za-z0-9._-]{1,128}$`，Access 本地为主键（`LOCAL_ADMIN_PROTOCOL.md` §5.5）。
    ImportId,
    check_import_id
);
newtype!(
    /// workspace 符号名：`^[a-z0-9][a-z0-9._-]{0,63}$`，不构成路径。
    WorkspaceAlias,
    check_workspace_alias
);
newtype!(
    /// Export template 标识：`^[A-Za-z0-9._-]{1,128}$`
    /// （`schemas/node-link/v1/common.schema.json#/$defs/templateId`；`LOCAL_ADMIN_PROTOCOL.md` §5.5 同）。
    TemplateId,
    check_template_id
);
newtype!(
    /// SHA-256(65 字节未压缩 P-256 公钥) 的 64 字符小写十六进制指纹。
    Fingerprint,
    check_fingerprint
);

newtype!(
    /// Export 内 Agent selector（1..=128 字符，非空；schema 只约束长度）。
    AgentId,
    check_agent_id
);

newtype!(
    /// `modeId` 的 newtype（1..=256 字符）。
    ModeId,
    check_mode_id
);
newtype!(
    /// 配置项标识（1..=256 字符，`SYNC_PROTOCOL.md` §11.5）。
    ConfigOptionId,
    check_config_option_id
);

// ---------------------------------------------------------------- 组合引用

/// Agent 引用：`{ agentId: 1..=128, name: 1..=128 }`（`common.schema.json#/$defs/sessionSummary`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentRef {
    agent_id: AgentId,
    name: String,
}

impl AgentRef {
    /// 构造。
    pub fn try_new(agent_id: AgentId, name: &str) -> Result<Self, InvalidValue> {
        require_bounded(name, 1, 128)?;
        Ok(Self {
            agent_id,
            name: name.to_owned(),
        })
    }

    /// Agent selector。
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    /// 展示名。
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// 模式引用：`{ modeId, displayName }`。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModeRef {
    mode_id: ModeId,
    display_name: String,
}

impl ModeRef {
    /// 构造。
    pub fn try_new(mode_id: ModeId, display_name: &str) -> Result<Self, InvalidValue> {
        require_bounded(display_name, 1, 256)?;
        Ok(Self {
            mode_id,
            display_name: display_name.to_owned(),
        })
    }

    /// 模式标识。
    pub fn mode_id(&self) -> &ModeId {
        &self.mode_id
    }

    /// 展示名。
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

/// owned 会话引用（`MODULE_ARCHITECTURE.md` §4.1）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OwnedSessionRef {
    pub session_id: SessionId,
}

impl OwnedSessionRef {
    /// 构造。
    pub fn new(session_id: SessionId) -> Self {
        Self { session_id }
    }
}

/// imported 会话引用：`ownerNodeId + exportId + sessionId`（`NODE_LINK_PROTOCOL.md` §7）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteSessionRef {
    pub owner_node_id: NodeId,
    pub export_id: ExportId,
    pub session_id: SessionId,
}

impl RemoteSessionRef {
    /// 构造。
    pub fn new(owner_node_id: NodeId, export_id: ExportId, session_id: SessionId) -> Self {
        Self {
            owner_node_id,
            export_id,
            session_id,
        }
    }
}

/// origin 事件引用：`ownerNodeId + originEpoch + originEventId`（`NODE_LINK_PROTOCOL.md` §7）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OriginEventRef {
    pub owner_node_id: NodeId,
    pub origin_epoch: OriginEpoch,
    pub origin_event_id: EventId,
}

impl OriginEventRef {
    /// 构造。
    pub fn new(owner_node_id: NodeId, origin_epoch: OriginEpoch, origin_event_id: EventId) -> Self {
        Self {
            owner_node_id,
            origin_epoch,
            origin_event_id,
        }
    }
}

/// 会话引用：owned 与 imported 在类型上可区分（§3.1）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SessionReference {
    Owned(OwnedSessionRef),
    Remote(RemoteSessionRef),
}

impl SessionReference {
    /// 底层 `SessionId`（两个家族共用同一 ID 形状，但不同 Owner 的同名 ID 不是同一会话）。
    pub fn session_id(&self) -> &SessionId {
        match self {
            Self::Owned(owned) => &owned.session_id,
            Self::Remote(remote) => &remote.session_id,
        }
    }

    /// 是否为本节点 owned。
    pub fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }
}

impl From<OwnedSessionRef> for SessionReference {
    fn from(value: OwnedSessionRef) -> Self {
        Self::Owned(value)
    }
}

impl From<RemoteSessionRef> for SessionReference {
    fn from(value: RemoteSessionRef) -> Self {
        Self::Remote(value)
    }
}

/// 错误定位用的实体引用（§3.1）。`Command` 需要 request 与可选 session 才能唯一定位（§6 第 6 条）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityRef {
    Session(SessionId),
    Turn(TurnId),
    Interaction(InteractionId),
    Command {
        session: Option<SessionId>,
        request: RequestId,
    },
    Pairing(PairingId),
    Device(DeviceId),
    Node(NodeId),
    Export(ExportId),
    Import(ImportId),
}

impl EntityRef {
    /// 实体类别标记（仓储行与审计的 `target_kind` 用的稳定 token）。
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Session(_) => "session",
            Self::Turn(_) => "turn",
            Self::Interaction(_) => "interaction",
            Self::Command { .. } => "command",
            Self::Pairing(_) => "pairing",
            Self::Device(_) => "device",
            Self::Node(_) => "node",
            Self::Export(_) => "export",
            Self::Import(_) => "import",
        }
    }

    /// 实体 id 文本（仓储行与审计的 `target_id`）。`Command` 用 `{request}` 或 `{session}/{request}`。
    pub fn target_id(&self) -> String {
        match self {
            Self::Session(id) => id.as_str().to_owned(),
            Self::Turn(id) => id.as_str().to_owned(),
            Self::Interaction(id) => id.as_str().to_owned(),
            Self::Command { session, request } => match session {
                Some(session) => format!("{}/{}", session.as_str(), request.as_str()),
                None => request.as_str().to_owned(),
            },
            Self::Pairing(id) => id.as_str().to_owned(),
            Self::Device(id) => id.as_str().to_owned(),
            Self::Node(id) => id.as_str().to_owned(),
            Self::Export(id) => id.as_str().to_owned(),
            Self::Import(id) => id.as_str().to_owned(),
        }
    }
}

impl fmt::Display for EntityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind(), self.target_id())
    }
}
