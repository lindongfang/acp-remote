//! 命令级结构化日志：两个协议面共用的字段与**封闭**的成因词表
//! （`sync-command-handling` 的「命令级结构化日志」要求、变更 `session-create-settle-order` 的 `design.md` D5）。
//!
//! ## 为什么集中在一处
//!
//! 要求的两件事都只能是**单一来源**：
//!
//! - **字段名**：同一台机器上的日志流要能按同一组键聚合（「当前由谁在控制」「哪条命令失败了」），
//!   不能因入口协议不同而分裂成两套；
//! - **失败成因**：要求明确「成因类别 SHALL 使用封闭词表，未登记的取值不得凭空写入」——如果每个调用点
//!   各写各的字符串，词表就不再封闭。本模块是唯一产出点，取值只有 [`Cause`] 的枚举项。
//!
//! ## 字段
//!
//! | 字段 | 取值 | 出现于 |
//! | --- | --- | --- |
//! | `event` | `sync.command_accepted` / `sync.command_terminal` / `node_link.command_accepted` / `node_link.command_terminal` | 全部 |
//! | `device_id` / `access_node_id` | Sync 面的已认证设备标识 / Node Link 面的 Access 节点标识（主体标识：谁在控制） | 全部 |
//! | `command` | 命令名（`session.create`、`session.prompt`……） | 全部 |
//! | `session_id` | 资源标识；命令不带会话时为 `None`（`session.create` 的受理阶段就是这一档） | 全部 |
//! | `request_id` | 幂等键的一半：把「受理」与「终态」两条日志（以及协议侧的 `requestId`）对上 | 全部 |
//! | `turn` | 被接受 mutation 的适配器侧 turn（无 turn 的创建类命令为 `None`） | 受理 |
//! | `terminal` | `completed` / `failed` / `uncertain` | 终态 |
//! | `cause` | [`Cause`] 的取值；`completed` 为 `none` | 终态 |
//!
//! 主体字段名沿用两个模块既有的写法（`sync` 用 `device_id`、Node Link 用 `access_node_id`）：两个面在
//! 同一份日志流里靠 `event` 前缀区分，改名只会与这两个模块其余日志不一致。
//!
//! ## 脱敏（`docs/SECURITY_DESIGN.md` §14.1）
//!
//! 只写标识、命令名与封闭类别，**不写**路径、凭据、prompt 正文；也**不写** `PortError`/`PublicError` 的
//! `Debug`——`PortError::Backend` 承载后端（含文件系统）的任意错误文本，`Debug` 出去就可能带出路径，
//! 而 §14.1 允许的只是错误**码**与类别。

use acp_core::model::{PortError, SessionId, TurnId, UnavailableKind};
use tracing::info;

/// 主体标识：哪个设备或节点在控制这条命令。
pub(crate) enum Subject<'a> {
    /// Sync 面：已认证的设备标识（设备信任行里的 `deviceId`）。
    Device(&'a str),
    /// Node Link 面：该 Access 节点的 node id。
    Node(&'a str),
}

/// 一条被接受的业务命令的**受理**日志（与它的终态日志成对）。
///
/// 由两个面在「已确认受理」的那一处调用：Sync 面的 `command.result(status = accepted)`、Node Link 面的
/// `command.accepted`。`session.create` 的受理阶段还没有会话行，`session` 因此传 `None`。
pub(crate) fn accepted(
    subject: Subject<'_>,
    command: &str,
    session: Option<&SessionId>,
    request_id: &str,
    turn: Option<&TurnId>,
) {
    let session_id = session.map(SessionId::as_str);
    let turn = turn.map(TurnId::as_str);
    match subject {
        Subject::Device(device_id) => info!(
            event = "sync.command_accepted",
            device_id,
            command,
            session_id = ?session_id,
            request_id,
            turn = ?turn,
            "the accepted command is admitted; the terminal follows as a persisted event"
        ),
        Subject::Node(access_node_id) => info!(
            event = "node_link.command_accepted",
            access_node_id,
            command,
            session_id = ?session_id,
            request_id,
            turn = ?turn,
            "the accepted command is admitted; the terminal follows as a persisted event"
        ),
    }
}

/// 一条已接受业务命令的**终态**日志。
///
/// 由两个面在「把持久终态交付给对端」的唯一落点调用（Sync 的 `send_record_terminal`、Node Link 的
/// `send_terminal`），以及本地合成终态的那两条路径（没有持久记录可读的构建/恢复前置失败）。同一
/// `requestId` 的重发与 `command.status` 重查会再次交付同一条终态、因此也会再记一条：漏记比重复记更
/// 不可诊断。
pub(crate) fn terminal(
    subject: Subject<'_>,
    command: &str,
    session: Option<&SessionId>,
    request_id: &str,
    class: TerminalClass,
    cause: Cause,
) {
    let session_id = session.map(SessionId::as_str);
    let terminal = class.as_str();
    let cause = cause.as_str();
    match subject {
        Subject::Device(device_id) => info!(
            event = "sync.command_terminal",
            device_id,
            command,
            session_id = ?session_id,
            request_id,
            terminal,
            cause,
            "the accepted command reached its terminal state"
        ),
        Subject::Node(access_node_id) => info!(
            event = "node_link.command_terminal",
            access_node_id,
            command,
            session_id = ?session_id,
            request_id,
            terminal,
            cause,
            "the accepted command reached its terminal state"
        ),
    }
}

/// 终态类别（封闭词表：两个协议面的状态词表都含这三个取值——Sync 的 `command.result.status` 五个取值、
/// Node Link 的 `command.terminal.status` 四个取值里的终态部分）。`accepted` 不是终态；`rejected` 不是
/// **已接受**命令的终态（那条命令从未被接受，它的结局由拒绝类日志记录），两者都不在这里记。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalClass {
    /// `completed`：业务完成（`result` 非空）。
    Completed,
    /// `failed`：可证明未产生副作用（`session.create` 的「进程未能启动 / 本机解析失败 / Agent 明确返回
    /// 错误」）。
    Failed,
    /// `uncertain`：无法确认副作用，客户端不得自动重试。
    Uncertain,
}

impl TerminalClass {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Uncertain => "uncertain",
        }
    }
}

/// 失败成因类别（**封闭**词表，取值只由本模块的映射产出）。
///
/// 两处**已登记的**来源（本模块只做收敛，不发明新值）：
///
/// - [`PortError`] 的分类（`crates/core/src/model/error.rs`）——[`Cause::of_port_error`]：手里有端口错误
///   时用它，粒度最细（`SessionNotEstablished` 与其余不可用取值分得开）；
/// - core 的公开错误码（`crates/core/src/broker.rs` 的 `port_error_public`/`create_failure_error` 是这些
///   码的产出点）——[`Cause::of_code`]：只持有持久记录里的错误码时用它（终态日志的常态）。
///
/// 词表之外的码收敛到 [`Cause::Unknown`]，**绝不**把错误原文写进日志（`PublicError.message` 可能含路径或
/// 后端文本）。新增 core 错误码时在这里补一条臂：`Unknown` 的比例是这张词表的覆盖度指标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cause {
    /// 没有失败：终态是 `completed`。
    None,
    /// 目标不存在（`PortError::NotFound`、`command.not_found`、`session.not_found`、`export.not_found`）。
    NotFound,
    /// 冲突：状态版本、幂等键、交互已解析（`PortError::Conflict` 与对应的 core 错误码）。
    Conflict,
    /// 参数/形状不合法（`PortError::InvalidRequest`、`protocol.schema_invalid`、
    /// `nodelink.command.unsupported_field`）。
    InvalidRequest,
    /// 授权拒绝（`authorization.scope_denied`、`export.not_granted`、`nodelink.export.not_granted`）。
    Denied,
    /// 存储或后端不可用（`PortError::Unavailable` 的多数取值、`internal.unavailable`、
    /// `resource.remote_unavailable`）。
    Unavailable,
    /// **可证明** Agent 侧没有产生会话（`UnavailableKind::SessionNotEstablished`）——`session.create`
    /// 据此判 `failed` 的那一类成因。
    SessionNotEstablished,
    /// 后端不支持该操作（`UnavailableKind::BackendUnsupported`、`command.unsupported`）。
    Unsupported,
    /// 后端失败且只有未细分的后端错误（`PortError::Backend`）。
    Backend,
    /// 存储内容自身损坏（`PortError::Corrupt`）。
    Corrupt,
    /// 本层投影失败：会话已建立，但结果投影不出来（终态按 `uncertain` 收口）。
    Projection,
    /// core 自己的「无法确认副作用」码（`command.uncertain`：创建/恢复失败与启动恢复都写它）。
    Unconfirmed,
    /// 词表之外的 core 错误码（未登记的取值只落这里，不落原文）。
    Unknown,
}

impl Cause {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::InvalidRequest => "invalid_request",
            Self::Denied => "denied",
            Self::Unavailable => "unavailable",
            Self::SessionNotEstablished => "session_not_established",
            Self::Unsupported => "unsupported",
            Self::Backend => "backend",
            Self::Corrupt => "corrupt",
            Self::Projection => "projection",
            Self::Unconfirmed => "unconfirmed",
            Self::Unknown => "unknown",
        }
    }

    /// 端口错误分类 → 成因（本层手里有错误对象时用）。
    ///
    /// `UnavailableKind` **逐值**列出而不是用通配臂接住「其余」：新增取值时编译器会报错，
    /// 新的「可证明」类成因不会被静默归进 `unavailable`。
    pub(crate) fn of_port_error(error: &PortError) -> Self {
        match error {
            PortError::NotFound(_) => Self::NotFound,
            PortError::Conflict(_) => Self::Conflict,
            PortError::InvalidRequest(reason) => match *reason {
                "authorization.scope_denied" => Self::Denied,
                _ => Self::InvalidRequest,
            },
            PortError::Unavailable(kind) => match kind {
                UnavailableKind::SessionNotEstablished => Self::SessionNotEstablished,
                UnavailableKind::BackendUnsupported => Self::Unsupported,
                UnavailableKind::Busy
                | UnavailableKind::StorageFull
                | UnavailableKind::IoError
                | UnavailableKind::RemoteUnavailable
                | UnavailableKind::OwnerOffline
                | UnavailableKind::KeystoreUnavailable => Self::Unavailable,
            },
            PortError::Corrupt(_) => Self::Corrupt,
            PortError::Backend(_) => Self::Backend,
        }
    }

    /// core 公开错误码 → 成因（只持有持久记录里的错误码时用）。
    pub(crate) fn of_code(code: &str) -> Self {
        match code {
            "command.not_found"
            | "nodelink.command.not_found"
            | "session.not_found"
            | "export.not_found" => Self::NotFound,
            "state.version_conflict"
            | "interaction.already_resolved"
            | "command.idempotency_conflict"
            | "nodelink.command.idempotency_conflict" => Self::Conflict,
            "protocol.schema_invalid"
            | "nodelink.protocol.schema_invalid"
            | "nodelink.command.unsupported_field" => Self::InvalidRequest,
            "authorization.scope_denied" | "export.not_granted" | "nodelink.export.not_granted" => {
                Self::Denied
            }
            "command.unsupported" | "nodelink.command.unsupported" => Self::Unsupported,
            "command.uncertain" | "nodelink.command.uncertain" => Self::Unconfirmed,
            "internal.unavailable"
            | "resource.remote_unavailable"
            | "local.unavailable"
            | "nodelink.internal.unavailable"
            | "nodelink.resource.remote_unavailable" => Self::Unavailable,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
pub(crate) mod capture {
    //! 测试专用的最小捕获订阅器：把每条日志收成「字段名 → 字段值」，用来断言字段**集合**与取值
    //! （`sync-command-handling` 的「断言字段集合」判据）。与 `app::logging` 同款的最小实现，不引入
    //! 依赖。
    //!
    //! `InterestGate` 的理由与 `sync::pairing` 的同名实现相同：tracing 的 interest 缓存是**进程级**的，
    //! 只要有任一测试线程在无订阅器的上下文里先把 callsite 注册掉，interest 就被缓存成 `never`，此后
    //! 装上作用域订阅器也收不到事件（并行跑测试时偶发假失败）。先装一个「总是感兴趣」的全局订阅器把
    //! 重算路径固定住。

    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex, PoisonError};

    use tracing::field::{Field, Visit};
    use tracing::level_filters::LevelFilter;
    use tracing::span::{Attributes, Id as SpanId, Record as SpanRecord};
    use tracing::subscriber::{DefaultGuard, Interest, set_default};
    use tracing::{Event, Metadata, Subscriber};

    /// 一条日志的字段集合（字段名 → 渲染后的取值）。
    pub(crate) type Fields = BTreeMap<String, String>;

    /// 进程级的「总是感兴趣」订阅器（见模块文档）。
    struct InterestGate;

    impl Subscriber for InterestGate {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn register_callsite(&self, _metadata: &'static Metadata<'static>) -> Interest {
            Interest::always()
        }

        fn new_span(&self, _attributes: &Attributes<'_>) -> SpanId {
            SpanId::from_u64(1)
        }

        fn record(&self, _span: &SpanId, _values: &SpanRecord<'_>) {}

        fn record_follows_from(&self, _span: &SpanId, _follows: &SpanId) {}

        fn enter(&self, _span: &SpanId) {}

        fn exit(&self, _span: &SpanId) {}

        fn event(&self, _event: &Event<'_>) {}
    }

    fn install_interest_gate() {
        static GATE: std::sync::Once = std::sync::Once::new();
        GATE.call_once(|| {
            let _ = tracing::subscriber::set_global_default(InterestGate);
        });
    }

    /// 捕获订阅器本体。
    struct Capturing {
        level: LevelFilter,
        events: Arc<Mutex<Vec<Fields>>>,
    }

    impl Subscriber for Capturing {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            *metadata.level() <= self.level
        }

        fn new_span(&self, _attributes: &Attributes<'_>) -> SpanId {
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
            let mut fields = Fields::new();
            event.record(&mut Collect {
                fields: &mut fields,
            });
            self.events
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(fields);
        }
    }

    /// 字段收集器（`record_str`/`record_debug` 都收成文本，便于按取值断言）。
    struct Collect<'a> {
        fields: &'a mut Fields,
    }

    impl Visit for Collect<'_> {
        fn record_str(&mut self, field: &Field, value: &str) {
            self.fields
                .insert(field.name().to_owned(), value.to_owned());
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.fields
                .insert(field.name().to_owned(), format!("{value:?}"));
        }
    }

    /// 已捕获的事件集合（`Clone` 只为在 `for` 里按值取用）。
    #[derive(Clone, Default)]
    pub(crate) struct Logs {
        events: Arc<Mutex<Vec<Fields>>>,
    }

    impl Logs {
        /// 装上作用域订阅器；掉落的 `DefaultGuard` 卸下它（作用域订阅器优先于进程级 gate）。
        pub(crate) fn install() -> (Self, DefaultGuard) {
            install_interest_gate();
            let logs = Self::default();
            let guard = set_default(Capturing {
                level: LevelFilter::TRACE,
                events: Arc::clone(&logs.events),
            });
            (logs, guard)
        }

        /// 指定 `event` 字段取值的全部日志（书写顺序）。
        pub(crate) fn of(&self, event: &str) -> Vec<Fields> {
            self.all()
                .into_iter()
                .filter(|fields| fields.get("event").is_some_and(|name| name == event))
                .collect()
        }

        /// 捕获到的全部日志。
        pub(crate) fn all(&self) -> Vec<Fields> {
            self.events
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    /// 字段名集合**恰好**是 `expected`（`message` 也算字段：`tracing` 把事件消息记成同名 `Debug` 字段）。
    ///
    /// 要求是「至少包含」，但多出来的字段同样是问题：这条断言让「顺手把某个错误原文/路径写进日志」在
    /// 用例里直接变红。
    pub(crate) fn assert_field_set(fields: &Fields, expected: &[&str]) {
        let mut actual: Vec<&str> = fields.keys().map(String::as_str).collect();
        actual.sort_unstable();
        let mut expected = expected.to_vec();
        expected.sort_unstable();
        assert_eq!(actual, expected, "字段集合不符：{fields:?}");
    }
}

#[cfg(test)]
mod tests {
    use acp_core::model::{
        ConflictKind, EntityRef, PortError, PublicError, SessionId, UnavailableKind,
    };

    use super::*;

    /// 词表是封闭的：每个取值都有稳定文本，且文本互不相同（两条日志能据此区分）。
    #[test]
    fn every_cause_has_a_distinct_registered_token() {
        let all = [
            Cause::None,
            Cause::NotFound,
            Cause::Conflict,
            Cause::InvalidRequest,
            Cause::Denied,
            Cause::Unavailable,
            Cause::SessionNotEstablished,
            Cause::Unsupported,
            Cause::Backend,
            Cause::Corrupt,
            Cause::Projection,
            Cause::Unconfirmed,
            Cause::Unknown,
        ];
        let mut tokens: Vec<&str> = all.iter().map(|cause| cause.as_str()).collect();
        tokens.sort_unstable();
        let count = tokens.len();
        tokens.dedup();
        assert_eq!(tokens.len(), count, "成因取值必须互不相同：{tokens:?}");
    }

    /// 终态类别与 wire 的两个终态词表逐字一致（日志里的类别因此能直接与协议帧对齐）。
    #[test]
    fn terminal_classes_match_the_wire_vocabulary() {
        assert_eq!(TerminalClass::Completed.as_str(), "completed");
        assert_eq!(TerminalClass::Failed.as_str(), "failed");
        assert_eq!(TerminalClass::Uncertain.as_str(), "uncertain");
    }

    /// 「可证明未产生会话」在端口错误与持久错误码两条路上都分得出来（判据的来源是 core 的类别）。
    #[test]
    fn the_provable_session_failure_is_recognized_on_both_sources() {
        assert_eq!(
            Cause::of_port_error(&PortError::Unavailable(
                UnavailableKind::SessionNotEstablished
            ))
            .as_str(),
            "session_not_established"
        );
        // 其余不可用取值（短请求超时、掉线、存储失败）不是「可证明」那一类。
        assert_eq!(
            Cause::of_port_error(&PortError::Unavailable(UnavailableKind::IoError)).as_str(),
            "unavailable"
        );
        assert_eq!(
            Cause::of_port_error(&PortError::Backend("io".into())).as_str(),
            "backend"
        );
        assert_eq!(
            Cause::of_port_error(&PortError::Corrupt("row")).as_str(),
            "corrupt"
        );
    }

    /// 未登记的码只落 `unknown`，绝不把码的原文（更不用说错误消息）写进日志。
    #[test]
    fn unregistered_codes_collapse_to_unknown() {
        assert_eq!(Cause::of_code("local.conflict").as_str(), "unknown");
        assert_eq!(
            Cause::of_code("nodelink.protocol.schema_invalid").as_str(),
            "invalid_request"
        );
        assert_eq!(Cause::of_code("command.uncertain").as_str(), "unconfirmed");
        assert_eq!(
            Cause::of_code("internal.unavailable").as_str(),
            "unavailable"
        );
    }

    /// 两个路径对同一事实给出同一个成因（记录里的错误码 vs 手里的端口错误）。
    #[test]
    fn the_two_sources_agree_on_the_shared_facts() {
        let pairs = [
            (
                PortError::NotFound(EntityRef::Command {
                    session: None,
                    request: acp_core::model::RequestId::new(
                        "7ae1c07c-9242-46e9-a9d2-4ec58c130f4e",
                    )
                    .expect("request id"),
                }),
                "command.not_found",
            ),
            (
                PortError::Conflict(ConflictKind::VersionMismatch),
                "state.version_conflict",
            ),
            (
                PortError::Conflict(ConflictKind::IdempotencyConflict),
                "command.idempotency_conflict",
            ),
            (
                PortError::InvalidRequest("authorization.scope_denied"),
                "authorization.scope_denied",
            ),
            (
                PortError::InvalidRequest("workspace alias is not registered on this node"),
                "protocol.schema_invalid",
            ),
            (
                PortError::Unavailable(UnavailableKind::BackendUnsupported),
                "command.unsupported",
            ),
            (
                PortError::Unavailable(UnavailableKind::IoError),
                "internal.unavailable",
            ),
        ];
        for (error, code) in pairs {
            assert_eq!(
                Cause::of_port_error(&error),
                Cause::of_code(code),
                "两条来源对 {code} 必须给出同一成因"
            );
        }
        // 唯一的例外：`SessionNotEstablished`（「可证明未产生会话」那条具名成因）被 core 折叠进
        // `internal.unavailable`（design D4 不新增错误码），因此只持有错误码时只能给 `unavailable`；
        // 终态**类别**（`failed`）已经承担了那条区分，两份日志合起来仍是完整事实。
        assert_eq!(
            Cause::of_port_error(&PortError::Unavailable(
                UnavailableKind::SessionNotEstablished
            )),
            Cause::SessionNotEstablished
        );
        assert_eq!(Cause::of_code("internal.unavailable"), Cause::Unavailable);
    }

    /// 受理与终态各一条，字段集合恰好是登记的那一组（主体、命令名、资源标识、请求标识；终态再加
    /// 类别与成因）——多一个字段（例如把错误原文带出去）就会红。
    #[test]
    fn both_stages_log_the_registered_field_set() {
        let (logs, guard) = capture::Logs::install();
        let session = SessionId::new("5d73cd10-a465-43cd-b3f1-704e2d49e99e").expect("session id");
        let request = "7ae1c07c-9242-46e9-a9d2-4ec58c130f4e";
        accepted(
            Subject::Device("a1b2c3d4-0000-4000-8000-0000000000a1"),
            "session.prompt",
            Some(&session),
            request,
            None,
        );
        terminal(
            Subject::Node("b1b2c3d4-0000-4000-8000-0000000000b1"),
            "session.create",
            None,
            request,
            TerminalClass::Failed,
            Cause::SessionNotEstablished,
        );
        drop(guard);

        let accepted_events = logs.of("sync.command_accepted");
        assert_eq!(accepted_events.len(), 1, "受理一条：{accepted_events:?}");
        capture::assert_field_set(
            &accepted_events[0],
            &[
                "event",
                "device_id",
                "command",
                "session_id",
                "request_id",
                "turn",
                "message",
            ],
        );
        assert_eq!(
            accepted_events[0]["device_id"],
            "a1b2c3d4-0000-4000-8000-0000000000a1"
        );
        assert_eq!(accepted_events[0]["command"], "session.prompt");
        assert_eq!(
            accepted_events[0]["session_id"],
            format!("Some({:?})", session.as_str())
        );
        assert_eq!(accepted_events[0]["request_id"], request);

        let terminal_events = logs.of("node_link.command_terminal");
        assert_eq!(terminal_events.len(), 1, "终态一条：{terminal_events:?}");
        capture::assert_field_set(
            &terminal_events[0],
            &[
                "event",
                "access_node_id",
                "command",
                "session_id",
                "request_id",
                "terminal",
                "cause",
                "message",
            ],
        );
        assert_eq!(terminal_events[0]["terminal"], "failed");
        assert_eq!(terminal_events[0]["cause"], "session_not_established");
        // 日志里不出现任何错误原文（只有标识、命令名与封闭类别）。
        let text = format!("{:?}", terminal_events[0]);
        assert!(
            !text.contains("失败") && !text.contains("\\"),
            "终态日志不得带出错误消息或路径：{text}"
        );
    }

    /// `PublicError` 的原文（可能含路径/后端文本）**不**进入日志：只有码参与成因映射。
    #[test]
    fn the_error_message_never_reaches_the_log() {
        let secret = "C:\\Users\\someone\\secret\\project";
        let error = PublicError::coded("internal.unavailable", secret, false).expect("公开错误");
        let (logs, guard) = capture::Logs::install();
        terminal(
            Subject::Device("a1b2c3d4-0000-4000-8000-0000000000a2"),
            "session.create",
            None,
            "7ae1c07c-9242-46e9-a9d2-4ec58c130f4f",
            TerminalClass::Failed,
            Cause::of_code(error.code()),
        );
        drop(guard);
        let events = logs.of("sync.command_terminal");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["cause"], "unavailable");
        let text = format!("{events:?}");
        assert!(!text.contains("secret"), "日志不得带出错误消息：{text}");
    }
}
