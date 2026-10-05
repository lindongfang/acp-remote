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
//! - 投递通道**未接线**是可观测的故障，不是空操作：见 [`NodeEventError`]（`reports/review-w4-r1.md`
//!   的 F1——把「接缝漏接」变成运行时无声失败是被禁止的）。

use std::sync::atomic::{AtomicBool, Ordering};

use acp_core::model::{AgentId, EndpointEvent, EventKind, EventType, Timestamp, ViewJson};
use acp_core::ports::{Clock, NodeEventSink};

/// 节点级事件**无法**交给投递通道的原因。
///
/// 存在的理由（`reports/review-w4-r1.md` F1）：[`NodeEvents::unbound`] 是组合根尚未接线时的状态，而
/// 「未接线」与「已投递」是两件事。把前者当成后者，会让整条节点级事件链（core 的
/// `Broker::commit_node_event` 落库、`core-derived-events` R8 的「节点级事件落库且会话标识为空」）在
/// 漏接时无声停摆——事件在 core 之前就被丢掉，没有任何人看得见。因此未接线时**返回错误并记一条
/// `error` 级日志**，调用方与运维都必须看见。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeEventError {
    /// 出口未接线（组合根没有调用 `AgentHost::with_node_events`/`set_node_events`）。
    ///
    /// 这个取值的语义是**事件没有被交付**（也就不会被落库），绝不是「已交付」。
    Unbound,
}

impl std::fmt::Display for NodeEventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "节点级事件出口未接线（组合根未调用 AgentHost::with_node_events/set_node_events）",
        )
    }
}

impl std::error::Error for NodeEventError {}

/// 节点级事件的交付口，由组合根注入。
///
/// 未接线时（[`NodeEvents::unbound`]）事件**不会**被静默丢弃：[`NodeEvents::send`] 返回
/// [`NodeEventError::Unbound`] 并记一条 `error` 级日志。`agent-host` 在没有组合根注入通道时仍然可用
/// （构造、目录查询、会话端点都不依赖它），但本类型**绝不**把丢弃伪装成投递——接线缺失必须在
/// 开发期（错误返回值）或运行期（日志）被看见。
#[derive(Clone)]
pub struct NodeEvents {
    sink: Option<NodeEventSink>,
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
    /// 未接线的出口：**不是**空操作——[`NodeEvents::send`] 会返回 [`NodeEventError::Unbound`]。
    #[must_use]
    pub fn unbound() -> Self {
        Self { sink: None }
    }

    /// 接线的出口（组合根把 core 的节点级事件入口包成 [`NodeEventSink`] 后注入）。
    #[must_use]
    pub fn new(sink: NodeEventSink) -> Self {
        Self { sink: Some(sink) }
    }

    /// 是否已接线。
    #[must_use]
    pub fn is_bound(&self) -> bool {
        self.sink.is_some()
    }

    /// 交付一条节点级事件。
    ///
    /// 已接线：同步交给 sink（`EventSource` 是同步调用面），返回 `Ok(())`。
    ///
    /// 未接线：**不静默丢弃**——记一条 `error` 级日志（运维可见）并返回
    /// [`NodeEventError::Unbound`]（开发期可见）。返回 `Ok(())` 只表示「已交给 sink」，
    /// 不表示下游已落库（落库是 core 的事）；但返回 `Err` 时才确定地表示「事件没有去向」。
    pub fn send(&self, event: EndpointEvent) -> Result<(), NodeEventError> {
        match &self.sink {
            Some(sink) => {
                sink.send(event);
                Ok(())
            }
            None => {
                tracing::error!(
                    event_type = event.event_type.as_str(),
                    "节点级事件没有交付通道：组合根未接线 NodeEvents，事件被丢弃\
                     （AgentHost::new(...).with_node_events(...) 或 set_node_events(...) 未调用）"
                );
                Err(NodeEventError::Unbound)
            }
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

/// 为「事件没有交付通道」补一条带 Agent 上下文的 `error` 日志。
///
/// [`NodeEvents::send`] 已经为未接线记了一条（含 `event_type`）；本函数由持有 Agent 标识的调用方
/// （`AgentRuntime`）补上 `agent_id`，让漏接在运行期可定位到具体 profile 进程。
pub(crate) fn log_undelivered(event_type: &str, agent: &AgentId) {
    tracing::error!(
        event_type,
        agent_id = agent.as_str(),
        "节点级事件未交付：该进程的这条连接/断开不会被落库（组合根漏接 NodeEvents）"
    );
}

/// 上报连接。
///
/// 返回 [`NodeEventError`] 表示事件**没有**被交付（当前唯一原因：出口未接线）。事件构造失败时静默跳过
/// （`ViewJson`/`EventType` 的校验对这两个常量形状不会失败），此时返回 `Ok(())`——没有事件需要交付，
/// 也就无所谓投递失败。
pub(crate) fn report_connected(
    events: &NodeEvents,
    agent: &AgentId,
    clock: &dyn Clock,
) -> Result<(), NodeEventError> {
    match connected_event(agent, now(clock)) {
        Some(event) => events.send(event),
        None => Ok(()),
    }
}

/// 上报断开；`exited` 为假时**不上报**（返回 `Ok(())`，没有事件需要交付）。
///
/// 这是 spec 的「断开 MUST NOT 早于该运行时被判定为已退出」在调用面的表达：调用方必须先把运行时
/// 标记为退出（`ExitMark::mark` 并由 `Supervisor::is_running()` 反映）。
pub(crate) fn report_disconnected(
    events: &NodeEvents,
    agent: &AgentId,
    cause: ExitCause,
    exited: bool,
    clock: &dyn Clock,
) -> Result<(), NodeEventError> {
    if !exited {
        return Ok(());
    }
    match disconnected_event(agent, Some(cause.public_error()), now(clock)) {
        Some(event) => events.send(event),
        None => Ok(()),
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

    /// 未接线的出口**不静默丢弃**：`send` 返回 `Err(Unbound)`（F1 的机器判据——接线缺失在开发期
    /// 可见），同时绝不 panic、绝不伪造成「已投递」。已接线的出口交付并返回 `Ok`。
    #[test]
    fn unbound_node_events_refuse_to_drop_silently() {
        let events = NodeEvents::unbound();
        assert!(!events.is_bound());

        let collector = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = {
            let collector = std::sync::Arc::clone(&collector);
            NodeEventSink::new(move |event| {
                if let Ok(mut guard) = collector.lock() {
                    guard.push(event);
                }
            })
        };
        let bound = NodeEvents::new(sink);
        assert!(bound.is_bound());

        let clock = FixedClock;
        // 未接线：上报返回 `Err`（事件没有去向），且没有任何东西被「投递」。
        assert_eq!(
            report_connected(&events, &agent(), &clock),
            Err(NodeEventError::Unbound),
            "未接线的 outlet 必须拒绝，而不是报告成功"
        );
        assert_eq!(
            report_disconnected(&events, &agent(), ExitCause::Normal, true, &clock),
            Err(NodeEventError::Unbound),
            "未接线的 outlet 必须拒绝，而不是报告成功"
        );
        // 已接线：交付一次并返回 `Ok`。
        assert!(report_connected(&bound, &agent(), &clock).is_ok());
        assert!(report_disconnected(&bound, &agent(), ExitCause::Normal, true, &clock).is_ok());
        assert_eq!(collector.lock().expect("lock").len(), 2);
    }

    /// 未判定退出时 `report_disconnected` 是空操作：没有事件需要交付，因此**不**是投递失败。
    #[test]
    fn report_disconnected_is_a_noop_before_the_exit_verdict() {
        let collector = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = {
            let collector = std::sync::Arc::clone(&collector);
            NodeEventSink::new(move |event| {
                if let Ok(mut guard) = collector.lock() {
                    guard.push(event);
                }
            })
        };
        let bound = NodeEvents::new(sink);
        assert!(
            report_disconnected(&bound, &agent(), ExitCause::Normal, false, &FixedClock).is_ok()
        );
        assert!(collector.lock().expect("lock").is_empty());
        // 即便出口未接线，「未判定退出」也不是投递失败——没有事件产生。
        assert!(
            report_disconnected(
                &NodeEvents::unbound(),
                &agent(),
                ExitCause::Normal,
                false,
                &FixedClock
            )
            .is_ok()
        );
    }

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> Timestamp {
            at()
        }
    }
}
