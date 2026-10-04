//! profile 进程的连接生命周期（节点级 `agent.connected` / `agent.disconnected`）。
//!
//! 归属与语义（`openspec/changes/sync-scope-and-pwa-client/specs/local-agent-host/spec.md`、
//! `design.md` D6）：
//!
//! - 生命周期归属于 **profile 的进程**，不是任何单个会话：同一 profile 的进程被多个会话复用时
//!   `MUST NOT` 重复上报连接（[`AgentLifecycle`] 的 `Stopped → Connected` 只成立一次）。
//! - 事件是**节点级**的：`EndpointEvent.turn` 与 `causation` 都是 `None`，`payload.acp` 也是 `None`
//!   （适配器自己产生的事件，没有 ACP 原文）。会话标识不属于本类型——它由组合根经 core 的节点级
//!   提交入口（`OwnedCommit { session: None, .. }`）落库；`agent-host` **不** import core 的 broker，
//!   因此这里只交付「已构造好的事件」，投递通道由组合根经 [`NodeEvents`] 注入。
//! - 断开上报 `MUST NOT` 早于该运行时被标记为已退出：由 [`ExitMark`] 承载「先标记、后上报」的顺序
//!   （结构上不可绕过，不是注释约定）。

use std::sync::atomic::{AtomicBool, Ordering};

use acp_core::model::{AgentId, EndpointEvent, EventKind, EventType, Timestamp, ViewJson};
use acp_core::ports::{Clock, EventSink};

/// 节点级事件的交付口，由组合根注入。
///
/// 未接线时（[`NodeEvents::unbound`]）事件被静默丢弃：`agent-host` 在没有组合根注入通道时仍需可用
/// （既有单测与诊断路径都不接线），且**绝不**伪造成「已投递」。
#[derive(Clone)]
pub struct NodeEvents {
    sink: Option<EventSink>,
}

impl std::fmt::Debug for NodeEvents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeEvents")
            .field("bound", &self.sink.is_some())
            .finish()
    }
}

impl Default for NodeEvents {
    fn default() -> Self {
        Self::unbound()
    }
}

impl NodeEvents {
    /// 未接线的出口（事件无处可去，静默丢弃）。
    #[must_use]
    pub fn unbound() -> Self {
        Self { sink: None }
    }

    /// 接线的出口（组合根把 core 的节点级事件入口包成 [`EventSink`] 后注入）。
    #[must_use]
    pub fn new(sink: EventSink) -> Self {
        Self { sink: Some(sink) }
    }

    /// 是否已接线。
    #[must_use]
    pub fn is_bound(&self) -> bool {
        self.sink.is_some()
    }

    fn send(&self, event: EndpointEvent) {
        if let Some(sink) = &self.sink {
            sink.send(event);
        }
    }
}

/// 一个进程实例的「已退出」标记。
///
/// 存在的理由是 spec 明文的**顺序**要求（「进程超限退出时，断开上报 MUST NOT 早于该运行时被判定为
/// 已退出」）：先 [`ExitMark::mark`]，再 [`ExitMark::take_disconnect`]。`mark` 是 `compare_exchange`，
/// 因此并发的退出路径（超限结束、读循环 EOF、显式关闭）只有第一条能拿到 `true`——断开上报也因此
/// 恰好一次。
#[derive(Debug, Default)]
pub struct ExitMark {
    exited: AtomicBool,
}

impl ExitMark {
    /// 新的未退出标记。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 标记退出；返回本次调用是否为**首次**标记（即「这条路径负责上报断开」）。
    pub fn mark(&self) -> bool {
        self.exited
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// 该运行时是否已被判定为已退出。
    #[must_use]
    pub fn is_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// 断开上报的唯一入口：**未标记退出时返回 `None`**（不上报）。
    ///
    /// 这个前置让「先标记、后上报」成为类型上的必要条件，而不是调用方要记住的纪律。
    #[must_use]
    pub fn take_disconnect(&self) -> bool {
        self.is_exited()
    }
}

/// 一个 profile 进程的连接生命周期（单次转移，幂等）。
///
/// 两个不变量由状态本身表达，而不是靠调用方的纪律：
///
/// 1. `Stopped → Connected` 只成立一次 ⇒ **复用既有进程不重复上报连接**；
/// 2. `Connected → Disconnected` 只成立一次（且必须经过 `Connected`）⇒ 每个进程实例最多一条断开，
///    且「从未上报连接的进程」（例如 `initialize` 失败后即被回收）不上报断开。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgentLifecycle {
    /// 从未上报过连接。
    #[default]
    Stopped,
    /// 已上报连接，尚未上报断开。
    Connected,
    /// 已上报断开。
    Disconnected,
}

impl AgentLifecycle {
    /// 进程建立：本次是否**应当**上报连接。
    pub fn on_spawn(&mut self) -> bool {
        if *self != Self::Stopped {
            return false;
        }
        *self = Self::Connected;
        true
    }

    /// 进程退出：本次是否**应当**上报断开。
    pub fn on_exit(&mut self) -> bool {
        if *self != Self::Connected {
            return false;
        }
        *self = Self::Disconnected;
        true
    }
}

/// 断开事件可携带的退出错误（视图形状的借用版，避免在适配器里复制一份 core 的所有权类型）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicErrorView<'a> {
    /// 已登记的公开错误码（`compatibility/errors/v1/errors.json`）。
    pub code: &'a str,
    /// 面向用户的消息（不含凭据、prompt 正文或本机路径）。
    pub message: &'a str,
    /// 是否可重试。
    pub retryable: bool,
}

/// 构造 `agent.connected`（节点级：无 turn、无 causation、无 ACP 原文）。
///
/// `state` 的取值来自封闭词表（`event-views.schema.json#/$defs/agent.connected` 的 `enum` 与
/// `crates/sync-protocol/src/views.rs` 的 `VIEW_ENUMS` 登记，两处唯一取值都是 `connected`）。
#[must_use]
pub fn connected_event(agent: &AgentId, at: Timestamp) -> Option<EndpointEvent> {
    node_event(agent, "agent.connected", "connected", None, at)
}

/// 构造 `agent.disconnected`。
///
/// `error` 为 `None` 时视图**不带** `error` 键（正常终止没有错误可报，而不是编造一个）。
#[must_use]
pub fn disconnected_event(
    agent: &AgentId,
    error: Option<PublicErrorView<'_>>,
    at: Timestamp,
) -> Option<EndpointEvent> {
    node_event(agent, "agent.disconnected", "disconnected", error, at)
}

fn node_event(
    agent: &AgentId,
    event_type: &str,
    state: &str,
    error: Option<PublicErrorView<'_>>,
    at: Timestamp,
) -> Option<EndpointEvent> {
    let mut view = serde_json::json!({
        "agentId": agent.as_str(),
        "state": state,
    });
    if let Some(error) = error {
        if let Some(object) = view.as_object_mut() {
            object.insert(
                "error".to_owned(),
                serde_json::json!({
                    "code": error.code,
                    "message": error.message,
                    "retryable": error.retryable,
                    "details": {},
                }),
            );
        }
    }
    let view = ViewJson::new(&view.to_string()).ok()?;
    let event_type = EventType::new(event_type).ok()?;
    Some(EndpointEvent::view_only(
        EventKind::State,
        event_type,
        view,
        None,
        None,
        at,
    ))
}

/// 进程退出的原因（决定断开事件是否携带错误，以及用哪个已登记错误码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCause {
    /// 单条 stdout 消息超限后结束（`SECURITY_DESIGN.md` §12.2）。
    MessageTooLarge,
    /// 自然退出或被动终止（空闲回收、显式关闭、Agent 自己退出）。
    Normal,
}

impl ExitCause {
    /// 从「是否因超限结束」判定原因。
    #[must_use]
    pub fn classify(oversize: bool) -> Self {
        if oversize {
            Self::MessageTooLarge
        } else {
            Self::Normal
        }
    }

    /// 断开事件携带的公开错误：只含**已登记**错误码与不含正文/路径的消息。
    #[must_use]
    pub fn public_error(self) -> PublicErrorView<'static> {
        match self {
            Self::MessageTooLarge => PublicErrorView {
                code: "protocol.message_too_large",
                message: "Agent 进程因单条 stdout 消息超限被结束",
                retryable: false,
            },
            Self::Normal => PublicErrorView {
                code: "internal.unavailable",
                message: "Agent 进程已退出",
                retryable: true,
            },
        }
    }
}

/// 时钟读取（适配器只在这里取事件时间戳；core 的 `Clock` 端口不暴露额外语义）。
pub(crate) fn now(clock: &dyn Clock) -> Timestamp {
    clock.now()
}

/// 上报连接（事件构造失败时静默跳过：`ViewJson`/`EventType` 的校验对这两个常量形状不会失败）。
pub(crate) fn report_connected(events: &NodeEvents, agent: &AgentId, clock: &dyn Clock) -> bool {
    match connected_event(agent, now(clock)) {
        Some(event) => {
            events.send(event);
            true
        }
        None => false,
    }
}

/// 上报断开；`exited` 为假时**不上报**（返回 `false`）。
///
/// 这是 spec 的「断开 MUST NOT 早于该运行时被判定为已退出」在调用面的表达：调用方必须先把运行时
/// 标记为退出（`ExitMark::mark` 并由 `Supervisor::is_running()` 反映）。
pub(crate) fn report_disconnected(
    events: &NodeEvents,
    agent: &AgentId,
    cause: ExitCause,
    exited: bool,
    clock: &dyn Clock,
) -> bool {
    if !exited {
        return false;
    }
    match disconnected_event(agent, Some(cause.public_error()), now(clock)) {
        Some(event) => {
            events.send(event);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> AgentId {
        AgentId::new("agent-lifecycle").expect("agent id")
    }

    fn at() -> Timestamp {
        Timestamp::new("2026-09-24T10:00:00.000Z").expect("timestamp")
    }

    /// `Stopped → Connected` 只成立一次：这就是「复用既有进程不重复上报连接」的机器判据。
    #[test]
    fn lifecycle_reports_connect_at_most_once() {
        let mut lifecycle = AgentLifecycle::default();
        assert_eq!(lifecycle, AgentLifecycle::Stopped);
        assert!(lifecycle.on_spawn(), "首次建立必须上报连接");
        assert_eq!(lifecycle, AgentLifecycle::Connected);
        // 多个会话复用同一个进程：每一条复用路径都会走到这里，但只有第一条成立。
        for _ in 0..5 {
            assert!(!lifecycle.on_spawn(), "复用既有进程不得重复上报连接");
        }
        assert_eq!(lifecycle, AgentLifecycle::Connected);
    }

    /// 断开只在「已上报连接」之后成立一次：从未连接的进程（`initialize` 失败即回收）不上报断开。
    #[test]
    fn lifecycle_reports_disconnect_at_most_once_and_only_after_connect() {
        let mut never_connected = AgentLifecycle::default();
        assert!(!never_connected.on_exit(), "从未连接的进程不上报断开");
        assert!(never_connected.on_spawn());

        let mut lifecycle = AgentLifecycle::default();
        assert!(lifecycle.on_spawn());
        assert!(lifecycle.on_exit(), "退出必须上报断开");
        assert!(!lifecycle.on_exit(), "断开恰好一次");
        assert_eq!(lifecycle, AgentLifecycle::Disconnected);
        // 断开之后进程不会再被当作已连接（新进程是新的 runtime/新的 lifecycle）。
        assert!(!lifecycle.on_spawn());
    }

    /// 「断开上报 MUST NOT 早于该运行时被判定为已退出」：`ExitMark` 让顺序成为必要条件。
    #[test]
    fn exit_mark_makes_disconnect_impossible_before_the_exit_verdict() {
        let mark = ExitMark::new();
        assert!(!mark.is_exited());
        assert!(!mark.take_disconnect(), "未判定退出时不得上报断开");

        assert!(mark.mark(), "第一条退出路径负责标记");
        assert!(!mark.mark(), "后续退出路径不重复标记");
        assert!(mark.is_exited());
        assert!(mark.take_disconnect(), "判定退出之后才允许上报断开");
    }

    /// 超限原因映射到**已登记**错误码，且正常退出与超限退出可区分。
    #[test]
    fn exit_cause_maps_to_registered_error_codes() {
        assert_eq!(ExitCause::classify(true), ExitCause::MessageTooLarge);
        assert_eq!(ExitCause::classify(false), ExitCause::Normal);
        let oversize = ExitCause::MessageTooLarge.public_error();
        assert_eq!(oversize.code, "protocol.message_too_large");
        assert!(!oversize.retryable);
        let normal = ExitCause::Normal.public_error();
        assert_eq!(normal.code, "internal.unavailable");
        assert!(normal.retryable);
        assert_ne!(oversize.code, normal.code, "两类退出必须可区分");
    }

    /// 连接事件是节点级的：无 turn、无 causation、无 ACP 原文；`state` 是封闭词表的唯一取值。
    #[test]
    fn connected_event_is_node_level_and_uses_the_closed_state() {
        let event = connected_event(&agent(), at()).expect("event");
        assert_eq!(event.event_type.as_str(), "agent.connected");
        assert_eq!(event.kind, EventKind::State);
        assert!(event.turn.is_none(), "节点级事件不带 turn");
        assert!(event.causation.is_none());
        assert!(event.payload.acp.is_none(), "适配器自产事件没有 ACP 原文");
        let view: serde_json::Value =
            serde_json::from_str(event.payload.view.as_str()).expect("view json");
        assert_eq!(view["agentId"], "agent-lifecycle");
        assert_eq!(view["state"], "connected");
        assert_eq!(
            view.as_object().expect("object").len(),
            2,
            "连接事件的视图只有两个键：{view}"
        );
    }

    /// 断开事件携带已登记错误码的 `error` 对象；`None` 时**不带** `error` 键（不编造）。
    #[test]
    fn disconnected_event_carries_the_error_only_when_present() {
        let without = disconnected_event(&agent(), None, at()).expect("event");
        let view: serde_json::Value =
            serde_json::from_str(without.payload.view.as_str()).expect("view json");
        assert_eq!(view["state"], "disconnected");
        assert!(view.get("error").is_none(), "无错误时不带 error 键");

        let with = disconnected_event(
            &agent(),
            Some(ExitCause::MessageTooLarge.public_error()),
            at(),
        )
        .expect("event");
        let view: serde_json::Value =
            serde_json::from_str(with.payload.view.as_str()).expect("view json");
        assert_eq!(view["error"]["code"], "protocol.message_too_large");
        assert_eq!(view["error"]["retryable"], false);
        // `publicError` 的四个必填键齐备（`details` 为空对象）。
        let error = view["error"].as_object().expect("error object");
        let mut keys: Vec<&str> = error.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["code", "details", "message", "retryable"]);
        assert_eq!(event_type_of(&with), "agent.disconnected");
    }

    fn event_type_of(event: &EndpointEvent) -> &str {
        event.event_type.as_str()
    }

    /// 未接线的出口不投递也不 panic（`agent-host` 在没有组合根注入通道时仍需可用）。
    #[test]
    fn unbound_node_events_drop_silently_but_stay_reportable() {
        let events = NodeEvents::unbound();
        assert!(!events.is_bound());
        let collector = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = {
            let collector = std::sync::Arc::clone(&collector);
            EventSink::new(move |event| {
                if let Ok(mut guard) = collector.lock() {
                    guard.push(event);
                }
            })
        };
        let bound = NodeEvents::new(sink);
        assert!(bound.is_bound());
        let clock = FixedClock;
        assert!(report_connected(&events, &agent(), &clock));
        assert!(report_disconnected(
            &bound,
            &agent(),
            ExitCause::Normal,
            true,
            &clock
        ));
        assert_eq!(collector.lock().expect("lock").len(), 1);
    }

    /// 未判定退出时 `report_disconnected` 是空操作（不产生事件）。
    #[test]
    fn report_disconnected_is_a_noop_before_the_exit_verdict() {
        let collector = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = {
            let collector = std::sync::Arc::clone(&collector);
            EventSink::new(move |event| {
                if let Ok(mut guard) = collector.lock() {
                    guard.push(event);
                }
            })
        };
        let bound = NodeEvents::new(sink);
        assert!(!report_disconnected(
            &bound,
            &agent(),
            ExitCause::Normal,
            false,
            &FixedClock
        ));
        assert!(collector.lock().expect("lock").is_empty());
    }

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> Timestamp {
            at()
        }
    }
}
