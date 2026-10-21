//! 事件、事件 payload 与提交结果（`docs/CORE_PORTS_AND_STORAGE.md` §3.4）。
//!
//! 关键不变量：
//!
//! - `EventType` 是**开放集合**（`^[a-z0-9_.-]{1,128}$`），不是闭合枚举：未登记取值按未知事件降级处理，
//!   而 `EventKind` 是闭合枚举（`owned_event.kind` 的 CHECK 取值）。
//! - `PendingEvent.policy` 的类型是 [`StoredPolicy`]，它**没有** `Ephemeral` 变体——`Ephemeral` 事件因此
//!   在类型上无法进入 `OwnedCommit.events`（§3.4/§6 第 11 条）。adapter 读到的 `PersistencePolicy` 要经
//!   [`StoredPolicy::try_from`] 或 [`PendingEvent::from_persistence`] 转换，`Ephemeral` 那条路径只能内存转发。
//! - owned 事件同时携带 `session_sequence` 与 `origin_sequence`：两者在同一事务内各自 +1，**不得**互相替代
//!   （`NODE_LINK_PROTOCOL.md` §7 要求跨节点只认 origin cursor）。非会话级事件两者均为 `None`。

use super::error::InvalidValue;
use super::ids::{
    EventId, OriginEpoch, RequestId, SessionId, TurnId, is_event_type, require_bounded,
};
use super::json::validate_document;
use super::scalars::{Digest, Sequence, Timestamp};
use std::fmt;
use std::str::FromStr;

use super::ViewJson;

newtype!(
    /// 事件类型：`^[a-z0-9_.-]{1,128}$`。未登记取值合法但无语义（按未知事件降级）。
    EventType,
    check_event_type
);

fn check_event_type(text: &str) -> Result<(), InvalidValue> {
    if is_event_type(text) {
        Ok(())
    } else {
        Err(InvalidValue::EventType)
    }
}

token_enum!(
    /// 事件类别（`owned_event.kind` 的 CHECK 取值，`INITIAL_DESIGN.md` §10.2/§10.3）。
    EventKind {
        State => "state",
        Delta => "delta",
        FinalMessage => "final_message",
        Structured => "structured",
        Interaction => "interaction",
        Summary => "summary",
    }
);

token_enum!(
    /// 事件来源（`SYNC_PROTOCOL.md` §10.1）：描述**产出该事件的主体**，由 broker 按诱发命令的执行者判定
    /// （`broker::event_origin`），存储层只原样写 `owned_event.origin_kind`。
    ///
    /// 取值与 §7.3 的 `owned_event.origin_kind` CHECK 逐值一致：`agent` 为 ACP 侧 Agent 产出、`device`
    /// 为本机已授权 Sync 设备经命令产出、`local_cli` 为本机 CLI 经本地通道产出、`daemon` 为 Daemon/Broker
    /// 自身产出、`node` 为**另一个节点经 Node Link 命令**产出。跨节点的跳数另由 `remoteOrigin` 表达
    /// （§9.6），两者**不得**互相替代；本取值也不是「远程节点」本身——`node` 说的是命令的执行者身份。
    EventOrigin {
        Agent => "agent",
        Device => "device",
        Daemon => "daemon",
        LocalCli => "local_cli",
        Node => "node",
    }
);

token_enum!(
    /// ACP 原文不可用的原因（`common.schema.json#/$defs/rawAcp`）。
    RawUnavailableReason {
        SizeLimit => "size_limit",
        RetentionExpired => "retention_expired",
        StorageFailure => "storage_failure",
    }
);

/// ACP 原文（§3.4）。逐字节保真的 JSON 文档；`Available` 与 `Unavailable` 二选一。
///
/// §3 冻结的是带字段名的 enum 形状，因此字段是公有的；不变量由 [`AcpRaw::available`] 与
/// [`AcpRaw::validate`] 表达（`byte_length` 必须等于 `raw_json` 的 UTF-8 字节数，`raw_json` 必须是良构
/// JSON 文档）。读取路径（存储层）应当调用 [`AcpRaw::validate`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpRaw {
    Available {
        media_type: String,
        raw_json: String,
        byte_length: u64,
        sha256: Digest,
    },
    Unavailable {
        reason: RawUnavailableReason,
        byte_length: u64,
        sha256: Option<Digest>,
    },
}

impl AcpRaw {
    /// 可用原文：`byte_length` 由 `raw_json` 的 UTF-8 字节数导出，`raw_json` 必须是良构 JSON 文档。
    pub fn available(
        media_type: &str,
        raw_json: &str,
        sha256: Digest,
    ) -> Result<Self, InvalidValue> {
        require_bounded(media_type, 1, 128)?;
        validate_document(raw_json, false).map_err(|_| InvalidValue::JsonDocument)?;
        Ok(Self::Available {
            media_type: media_type.to_owned(),
            byte_length: raw_json.len() as u64,
            raw_json: raw_json.to_owned(),
            sha256,
        })
    }

    /// 原文不可用（`size_limit`/`retention_expired`/`storage_failure`）；`sha256` 只在确实算不出时为 `None`。
    pub fn unavailable(
        reason: RawUnavailableReason,
        byte_length: u64,
        sha256: Option<Digest>,
    ) -> Self {
        Self::Unavailable {
            reason,
            byte_length,
            sha256,
        }
    }

    /// `Available` 的三元组视图：`(media_type, raw_json, byte_length, sha256)`。
    pub fn as_available(&self) -> Option<(&str, &str, u64, &Digest)> {
        match self {
            Self::Available {
                media_type,
                raw_json,
                byte_length,
                sha256,
            } => Some((media_type, raw_json, *byte_length, sha256)),
            Self::Unavailable { .. } => None,
        }
    }

    /// `Unavailable` 的三元组视图：`(reason, byte_length, sha256)`。
    pub fn as_unavailable(&self) -> Option<(RawUnavailableReason, u64, Option<&Digest>)> {
        match self {
            Self::Unavailable {
                reason,
                byte_length,
                sha256,
            } => Some((*reason, *byte_length, sha256.as_ref())),
            Self::Available { .. } => None,
        }
    }

    /// 校验 §10.1 的相容性：`Available` 的 `raw_json` 良构且 `byte_length` 与之一致。
    pub fn validate(&self) -> Result<(), InvalidValue> {
        match self {
            Self::Available {
                media_type,
                raw_json,
                byte_length,
                ..
            } => {
                require_bounded(media_type, 1, 128)?;
                validate_document(raw_json, false).map_err(|_| InvalidValue::JsonDocument)?;
                if *byte_length != raw_json.len() as u64 {
                    return Err(InvalidValue::Field);
                }
                Ok(())
            }
            Self::Unavailable { .. } => Ok(()),
        }
    }
}

/// 事件 payload（§3.4）：`view` 是公共结构化视图（原样保存），`acp` 是 ACP 原文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventPayload {
    pub view: ViewJson,
    pub acp: Option<AcpRaw>,
}

impl EventPayload {
    /// 构造。
    pub fn new(view: ViewJson, acp: Option<AcpRaw>) -> Self {
        Self { view, acp }
    }

    /// 校验 `acp` 的相容性。
    pub fn validate(&self) -> Result<(), InvalidValue> {
        match &self.acp {
            Some(acp) => acp.validate(),
            None => Ok(()),
        }
    }
}

token_enum!(
    /// 事件的**期望**持久化策略（`INITIAL_DESIGN.md` §10.2）。`Ephemeral` 不得进入任何提交。
    PersistencePolicy {
        Durable => "durable",
        ShortTerm => "short_term",
        Ephemeral => "ephemeral",
    }
);

token_enum!(
    /// 可提交的持久化策略（本合同 §3.4）：`OwnedCommit` 只接受它，使 `Ephemeral` 不可表达。
    StoredPolicy {
        Durable => "durable",
        ShortTerm => "short_term",
    }
);

impl StoredPolicy {
    /// 从期望策略转换；`Ephemeral` 返回 `InvalidValue::Ephemeral`。
    pub fn from_persistence(policy: PersistencePolicy) -> Result<Self, InvalidValue> {
        match policy {
            PersistencePolicy::Durable => Ok(Self::Durable),
            PersistencePolicy::ShortTerm => Ok(Self::ShortTerm),
            PersistencePolicy::Ephemeral => Err(InvalidValue::Ephemeral),
        }
    }
}

impl TryFrom<PersistencePolicy> for StoredPolicy {
    type Error = InvalidValue;

    fn try_from(policy: PersistencePolicy) -> Result<Self, Self::Error> {
        Self::from_persistence(policy)
    }
}

impl From<StoredPolicy> for PersistencePolicy {
    fn from(policy: StoredPolicy) -> Self {
        match policy {
            StoredPolicy::Durable => Self::Durable,
            StoredPolicy::ShortTerm => Self::ShortTerm,
        }
    }
}

/// 待提交事件（§3.4）。
///
/// `policy` 的类型是 [`StoredPolicy`]：`Ephemeral` 在类型上不可表达，因此 `OwnedCommit.events` 里的每一条
/// 都必然是可持久化的（§6 第 11 条的编译期保证）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEvent {
    pub kind: EventKind,
    pub event_type: EventType,
    pub policy: StoredPolicy,
    pub payload: EventPayload,
    /// 事件产生者（`SYNC_PROTOCOL.md` §10.1 的 `origin.kind`）：由 broker 按「谁产生」判定（命令类事件
    /// 取诱发命令的执行者，见 `broker::event_origin`），存储层原样写 `owned_event.origin_kind`，**不得**
    /// 按有没有会话（或有没有 actor）猜。
    pub origin: EventOrigin,
    pub turn: Option<TurnId>,
    /// **诱发这条事件的命令**（`SYNC_PROTOCOL.md` §10.1 的 `causationRequestId`）：由 broker 在提交时
    /// 从诱发命令取（turn 类取该 turn 的 `causation`、其余命令类事件取会话在作用域的命令），没有命令直接
    /// 导致的事件为 `None`。它与 `origin` 表达不同的事实——`origin` 回答「谁产生的」、`causation` 回答
    /// 「哪条命令导致的」——**不得**互相替代（turn 内由 Agent 产出的事件 `origin = agent`，但照样带该
    /// 命令的 `causation`）。
    pub causation: Option<RequestId>,
}

impl PendingEvent {
    /// 构造。
    #[allow(clippy::too_many_arguments)] // §3.4 冻结的字段形状
    pub fn new(
        kind: EventKind,
        event_type: EventType,
        policy: StoredPolicy,
        payload: EventPayload,
        origin: EventOrigin,
        turn: Option<TurnId>,
        causation: Option<RequestId>,
    ) -> Self {
        Self {
            kind,
            event_type,
            policy,
            payload,
            origin,
            turn,
            causation,
        }
    }

    /// 从期望策略构造：`Ephemeral` 返回 `None`（broker 只做内存转发，不组装进 `OwnedCommit`）。
    pub fn from_persistence(
        kind: EventKind,
        event_type: EventType,
        policy: PersistencePolicy,
        payload: EventPayload,
        origin: EventOrigin,
        turn: Option<TurnId>,
        causation: Option<RequestId>,
    ) -> Option<Self> {
        let policy = StoredPolicy::from_persistence(policy).ok()?;
        Some(Self {
            kind,
            event_type,
            policy,
            payload,
            origin,
            turn,
            causation,
        })
    }

    /// 校验 `payload.acp` 的相容性。
    pub fn validate(&self) -> Result<(), InvalidValue> {
        self.payload.validate()
    }
}

/// 已提交事件的定位信息（§3.4）。非会话级事件的 `session`/`session_sequence` 均为 `None`，但仍有
/// `global_sequence`（`SYNC_PROTOCOL.md` §10.1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedEvent {
    pub id: EventId,
    /// 事件类型：必填，历史与重放都要能在 wire 上报出它（§3.3）。
    pub event_type: EventType,
    pub session: Option<SessionId>,
    pub session_sequence: Option<Sequence>,
    pub global_sequence: Sequence,
    /// 会话级 origin cursor 的两个分量（`NODE_LINK_PROTOCOL.md` §7）。非会话级事件
    /// （`session: None`）为 `None`——node 作用域事件没有会话级 origin cursor（§3.4/§7.3 的成对 CHECK）。
    pub origin_epoch: Option<OriginEpoch>,
    pub origin_sequence: Option<Sequence>,
    pub created_at: Timestamp,
}

impl CommittedEvent {
    /// 会话级定位必须成对成立：`session` 存在 ⇔ `session_sequence`/`origin_epoch`/`origin_sequence` 都存在。
    pub fn validate(&self) -> Result<(), InvalidValue> {
        if self.session.is_some() != self.session_sequence.is_some() {
            return Err(InvalidValue::Field);
        }
        if self.session.is_some() != self.origin_epoch.is_some() {
            return Err(InvalidValue::Field);
        }
        if self.session.is_some() != self.origin_sequence.is_some() {
            return Err(InvalidValue::Field);
        }
        Ok(())
    }
}
