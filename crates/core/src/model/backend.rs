//! 后端与能力（`docs/CORE_PORTS_AND_STORAGE.md` §3.6）。
//!
//! 这些类型是 core 传给 `AgentCatalog`/`SessionBackendFactory` 的输入，以及 `SessionEndpoint` 输出流的
//! 元素。它们只描述「哪个 Agent、哪个 workspace alias、哪些能力、什么事件」，不含任何 ACP DTO。

use std::collections::BTreeSet;

use super::error::InvalidValue;
use super::event::{EventKind, EventPayload, EventType};
use super::ids::{
    AgentRef, RequestId, TemplateId, TurnId, WorkspaceAlias, is_template_id, require_bounded,
};
use super::json::ViewJson;
use super::scalars::Timestamp;
use super::session::{ConfigValue, ResourceOrigin};

/// Agent 目录条目（§3.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDescriptor {
    pub agent: AgentRef,
    pub available: bool,
    pub origin: ResourceOrigin,
}

impl AgentDescriptor {
    /// 构造。
    pub fn new(agent: AgentRef, available: bool, origin: ResourceOrigin) -> Self {
        Self {
            agent,
            available,
            origin,
        }
    }
}

/// 单个能力声明（`ACP_COMPATIBILITY_MATRIX.md` §4）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Capability {
    kind: String,
    detail: Option<String>,
}

impl Capability {
    /// 构造。`kind` 1..=128，`detail` ≤256 字符。
    pub fn try_new(kind: &str, detail: Option<String>) -> Result<Self, InvalidValue> {
        require_bounded(kind, 1, 128)?;
        if let Some(detail) = &detail {
            require_bounded(detail, 0, 256)?;
        }
        Ok(Self {
            kind: kind.to_owned(),
            detail,
        })
    }

    /// 能力名。
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// 可选的实例细节。
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

/// 去重后的能力集合（§3.6）。`collect()` 即去重构造。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet(BTreeSet<Capability>);

impl CapabilitySet {
    /// 空集合。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 插入；集合中已存在等价能力时返回 `false`。
    pub fn insert(&mut self, capability: Capability) -> bool {
        self.0.insert(capability)
    }

    /// 是否包含该能力。
    pub fn contains(&self, capability: &Capability) -> bool {
        self.0.contains(capability)
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 能力个数。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 迭代（`kind` 字典序）。
    pub fn iter(&self) -> impl Iterator<Item = &Capability> + '_ {
        self.0.iter()
    }
}

impl FromIterator<Capability> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = Capability>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// `session.create` 的 template 选择（§3.6）：`{ templateId, params }`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateSelection {
    template_id: TemplateId,
    params: Vec<(String, ConfigValue)>,
}

impl TemplateSelection {
    /// 构造。参数名必须是 `^[A-Za-z0-9._-]{1,64}$` 且不重复。
    pub fn try_new(
        template_id: TemplateId,
        params: Vec<(String, ConfigValue)>,
    ) -> Result<Self, InvalidValue> {
        for (name, _) in &params {
            if !is_template_id(name) {
                return Err(InvalidValue::Field);
            }
        }
        for (index, (name, _)) in params.iter().enumerate() {
            if params[..index].iter().any(|(other, _)| other == name) {
                return Err(InvalidValue::Field);
            }
        }
        Ok(Self {
            template_id,
            params,
        })
    }

    /// 目标 template。
    pub fn template_id(&self) -> &TemplateId {
        &self.template_id
    }

    /// 参数取值（键由该 Export 的 template 声明，`NODE_LINK_PROTOCOL.md` §2.4 的开放容器）。
    pub fn params(&self) -> &[(String, ConfigValue)] {
        &self.params
    }
}

/// 创建会话请求（§3.6）。
///
/// 注意：§5.1 的注释提到「`CreateSessionRequest.session` 由 core 分配后传入」，但 §3.6 冻结的字段表里没有
/// `session` 字段；按 §3.6 实现，`SessionId` 由 `SessionStore::commit` 在创建事务内分配并经
/// `CommitOutcome.session_id` 返回（§3.1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSessionRequest {
    pub agent: AgentRef,
    pub workspace_alias: Option<WorkspaceAlias>,
    pub template: Option<TemplateSelection>,
    pub origin: ResourceOrigin,
}

impl CreateSessionRequest {
    /// 构造。
    pub fn new(
        agent: AgentRef,
        workspace_alias: Option<WorkspaceAlias>,
        template: Option<TemplateSelection>,
        origin: ResourceOrigin,
    ) -> Self {
        Self {
            agent,
            workspace_alias,
            template,
            origin,
        }
    }
}

/// `SessionEndpoint` 输出流的元素（§3.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointEvent {
    pub kind: EventKind,
    pub event_type: EventType,
    pub payload: EventPayload,
    pub turn: Option<TurnId>,
    pub causation: Option<RequestId>,
    pub at: Timestamp,
}

impl EndpointEvent {
    /// 构造。
    pub fn new(
        kind: EventKind,
        event_type: EventType,
        payload: EventPayload,
        turn: Option<TurnId>,
        causation: Option<RequestId>,
        at: Timestamp,
    ) -> Self {
        Self {
            kind,
            event_type,
            payload,
            turn,
            causation,
            at,
        }
    }

    /// 只有 view、没有 ACP 原文的事件。
    pub fn view_only(
        kind: EventKind,
        event_type: EventType,
        view: ViewJson,
        turn: Option<TurnId>,
        causation: Option<RequestId>,
        at: Timestamp,
    ) -> Self {
        Self::new(
            kind,
            event_type,
            EventPayload::new(view, None),
            turn,
            causation,
            at,
        )
    }

    /// 校验 `payload.acp` 的相容性。
    pub fn validate(&self) -> Result<(), InvalidValue> {
        self.payload.validate()
    }
}
