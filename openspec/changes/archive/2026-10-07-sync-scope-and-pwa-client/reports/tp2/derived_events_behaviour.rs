// TP2 交付产物的**交接副本**：与 crates/storage-sqlite/tests/derived_events_behaviour.rs 逐字节相同。
// 权威可运行副本在交付提交 5ba44fd 的 crates/storage-sqlite/tests/ 下（cargo test --locked -p storage-sqlite --test derived_events_behaviour）。
// 本副本仅供 handoff 的 test_delivery.artifacts 登记（该字段相对变更目录解析），不参与构建。

//! TP2（`sync-scope-and-pwa-client`，交付单元 MU2）的 core 生产者行为判定：**R5–R9** 在真实存储上的可观察结果。
//!
//! ## 为什么本文件落在 `crates/storage-sqlite/tests/`
//!
//! TP2 的写入范围（`plan.md` 的 Work Packages 表）是 `crates/core/src/**/tests.rs` 与
//! `crates/storage-sqlite/tests/`。前者在本仓库**不可用**：`crates/core/src/broker.rs` 自己持有
//! `#[cfg(test)] mod tests`（内联模块），要新增 `crates/core/src/broker/tests.rs` 必须改 `broker.rs`
//! 的模块声明，而 `broker.rs` 是 WP3 的实现文件、不在 TP2 的写入范围内。因此本包把 core 生产者行为
//! 的判定放在 `crates/storage-sqlite/tests/`（该目录在写入范围内、且 `[PV2]` 覆盖 `-p storage-sqlite`）。
//!
//! ## 被测入口是真实的
//!
//! 全部用例经**公开端口**驱动真实 `acp_core::broker::Broker`：
//!
//! - 会话级派生：`Broker::sink(session)` → `Broker::flush(session)`（= 组合根合并窗口的同一个入口）；
//! - 节点级事件：`Broker::commit_node_event(event)`；
//! - 读回：`SessionStore::load`/`load_recovery`/`list` + `ReadView::replay`/`event_payload`。
//!
//! `BrokerDeps` 里除后端工厂/标识分配器/时钟/发布器之外**全部注入真实 `SqliteStore`**
//! （`deliveries`/`exports`/`trust`/`audit` 与 `store` 是同一个实例——`SqliteStore` 实现了这几个端口，
//! 与 `crates/app/tests/support/owner.rs` 的装配口径一致）。本文件**不**手写派生出的 view 文本：
//! `file.changed` 的每个字节都由 core 的派生路径产生，测试只断言真实落库结果。
//!
//! ## 与既有用例的分工（不重复）
//!
//! - `crates/core/src/broker.rs` 的内联测试与 `crates/core/src/derive.rs` 的单测：以**内存 fake 端口**
//!   覆盖派生逻辑本身（含 Myers 与暴力对拍）。本文件不重跑那些断言。
//! - `crates/storage-sqlite/tests/title_write.rs`（WP3 修复轮）：直接 `SessionStore::commit` 的标题**列**
//!   语义。本文件补的是「ACP 通知 → broker 投影 → 真实列」这条链路（含同批 `session.mode.changed` 的
//!   版本注入比对），并补齐 `ModeChange::Unchanged + Some(None) + 同批其它列` 这一未被覆盖的语句分支。
//!
//! ## 未覆盖（明确登记，见交付报告）
//!
//! 进程生命周期的**上报时机**（首次建立只上报一次、复用既有进程不重复、超限退出时断开不早于退出判定）
//! 与「未绑定 `NodeEvents` 不静默丢弃」属 `crates/agent-host`（WP4 的写入范围），本包**不**改该目录；
//! 那些断言由 WP4 的用例承载并在 `[PV2]` 的 `-p agent-host` 中执行。

mod support;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::{
    AcpRaw, AgentId, AgentRef, AgentSessionId, CommittedDelivery, CreateSessionRequest, Digest,
    EndpointEvent, EventKind, EventPayload, EventType, InteractionId, MessageId, OriginEpoch,
    PairingId, PortError, RequestId, SessionId, SessionReference, SessionSnapshot, SessionState,
    Timestamp, TurnId, ViewJson,
};
use acp_core::ports::{
    Clock, EventSink, IdGenerator, ModeChange, NewSession, OwnedCommit, SessionBackendFactory,
    SessionEndpoint, SessionStore, SessionUpdate, StateChange,
};
use storage_sqlite::migrate::{DATABASE_FILE, StorageConfig};
use storage_sqlite::session_store::SqliteStore;
use support::*;

/// 会话创建后写入的 ACP 会话标识（`owned_session.agent_session_id`，§3.6 的恢复列）。
const AGENT_SESSION_ID: &str = "acp-tp2-behaviour";
/// 会话的行标识前缀（r5–r9 的同一条会话）。
const EPOCH: &str = "7f3a5c11-4b2d-4e6f-9a80-1d2e3f4a5b6c";

// ---------------------------------------------------------------------------------------------
// 夹具：真实 Broker + 真实 SqliteStore
// ---------------------------------------------------------------------------------------------

/// 单调时钟（每次读推进 1 分钟）。core 不读系统时间，`updated_at` 的断言因此确定。
struct MonotonicClock {
    minutes: AtomicU32,
}

impl MonotonicClock {
    fn new() -> Self {
        Self {
            minutes: AtomicU32::new(0),
        }
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> Timestamp {
        let minutes = self.minutes.fetch_add(1, Ordering::SeqCst);
        stamp(minutes)
    }
}

/// `2026-10-05T00:<mm>:00.000Z`（`Timestamp` 的固定宽度口径）。
fn stamp(minutes: u32) -> Timestamp {
    Timestamp::new(&format!(
        "2026-10-05T{:02}:{:02}:00.000Z",
        (minutes / 60) % 24,
        minutes % 60
    ))
    .expect("规范时间戳")
}

/// 标识分配器：形态合法、确定性（用例不依赖随机性）。
#[derive(Default)]
struct SeqIds {
    counter: AtomicU32,
}

impl SeqIds {
    fn next(&self) -> String {
        format!(
            "00000000-0000-4000-8000-{:012}",
            self.counter.fetch_add(1, Ordering::SeqCst) + 1
        )
    }
}

impl IdGenerator for SeqIds {
    fn turn_id(&self) -> TurnId {
        TurnId::new(&self.next()).expect("turn id")
    }

    fn interaction_id(&self) -> InteractionId {
        InteractionId::new(&self.next()).expect("interaction id")
    }

    fn pairing_id(&self) -> PairingId {
        PairingId::new(&self.next()).expect("pairing id")
    }

    fn message_id(&self) -> MessageId {
        MessageId::new(&self.next()).expect("message id")
    }

    fn origin_epoch(&self) -> OriginEpoch {
        OriginEpoch::new(&self.next()).expect("origin epoch")
    }

    fn request_id(&self) -> RequestId {
        RequestId::new(&self.next()).expect("request id")
    }
}

/// 发布记录器（§5.4）：记录已提交的投递，供「先提交后发布」类的观察（本文件只做存在性断言）。
#[derive(Default)]
struct Recorder {
    published: Mutex<Vec<CommittedDelivery>>,
}

impl acp_core::ports::EventPublisher for Recorder {
    fn publish(&self, delivery: CommittedDelivery) {
        self.published
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(delivery);
    }
}

/// **未被本文件任何用例触及**的后端工厂：`sink`+`flush` 与 `commit_node_event` 都不经过它。保留为失败关闭
/// 的替身，使「误经后端」这一耦合在测试里立刻可见，而不是静默走通。
struct UntouchedBackends;

#[async_trait::async_trait]
impl SessionBackendFactory for UntouchedBackends {
    async fn create(
        &self,
        _session: &SessionId,
        _request: CreateSessionRequest,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        unreachable!("TP2 夹具：本文件不驱动后端工厂（会话由 store 建立）")
    }

    async fn open(
        &self,
        _reference: SessionReference,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        unreachable!("TP2 夹具：本文件不驱动后端工厂")
    }

    async fn resume(
        &self,
        _session: &SessionId,
        _request: acp_core::model::ResumeSessionRequest,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        unreachable!("TP2 夹具：本文件不驱动后端工厂")
    }
}

/// 测试夹具：真实 SQLite + 真实 broker。
struct Fixture {
    dir: TempDir,
    store: Arc<SqliteStore>,
    broker: Arc<Broker>,
    session: SessionId,
}

impl Fixture {
    /// 起一个夹具。`workspace` 为 `Some` 时按**产品写路径**（`SessionUpdate.workspace_cwd`）登记工作目录根，
    /// `None` 时该会话的 `workspace_cwd` 保持 `NULL`（= 工作区根未登记）。
    async fn new(name: &str, workspace: Option<&str>) -> Self {
        let dir = temp_dir(name);
        let store = Arc::new(
            SqliteStore::open(StorageConfig::new(&dir), &stamp(0))
                .await
                .expect("打开真实 SQLite"),
        );
        let created = store
            .commit(OwnedCommit {
                session: None,
                at: stamp(1),
                expected_version: None,
                state: Some(StateChange::Create(NewSession {
                    title: None,
                    agent: agent(),
                })),
                turns: Vec::new(),
                events: Vec::new(),
                interactions: Vec::new(),
                compacted: Vec::new(),
                idempotency: None,
                command_terminal: None,
                origin_epoch: Some(OriginEpoch::new(EPOCH).expect("epoch")),
            })
            .await
            .expect("建立会话行");
        let session = created.session_id.expect("会话 id");
        if let Some(root) = workspace {
            // 恢复两列由 core 在 `session.create` 里写入；这里走同一条列写路径（不绕过端口）。
            store
                .commit(OwnedCommit {
                    session: Some(session.clone()),
                    at: stamp(2),
                    expected_version: None,
                    state: Some(StateChange::Update(SessionUpdate {
                        state: None,
                        mode: ModeChange::Unchanged,
                        title: None,
                        closed_at: None,
                        interaction: None,
                        agent_session_id: Some(
                            AgentSessionId::new(AGENT_SESSION_ID).expect("agent session id"),
                        ),
                        workspace_cwd: Some(root.to_owned()),
                        workspace_alias: None,
                    })),
                    turns: Vec::new(),
                    events: Vec::new(),
                    interactions: Vec::new(),
                    compacted: Vec::new(),
                    idempotency: None,
                    command_terminal: None,
                    origin_epoch: None,
                })
                .await
                .expect("登记工作目录根");
        }
        let stored: Arc<dyn SessionStore> = store.clone();
        let deliveries: Arc<dyn acp_core::ports::RemoteDeliveryStore> = store.clone();
        let exports: Arc<dyn acp_core::ports::ExportStore> = store.clone();
        let trust: Arc<dyn acp_core::ports::TrustStore> = store.clone();
        let audit: Arc<dyn acp_core::ports::AuditStore> = store.clone();
        let broker = Arc::new(Broker::new(
            BrokerDeps {
                store: stored,
                deliveries,
                backends: Arc::new(UntouchedBackends),
                exports,
                trust,
                publisher: Arc::new(Recorder::default()),
                clock: Arc::new(MonotonicClock::new()),
                ids: Arc::new(SeqIds::default()),
                audit: Some(audit),
            },
            BrokerConfig {
                persist_deltas: true,
                ..BrokerConfig::default()
            },
        ));
        Self {
            dir,
            store,
            broker,
            session,
        }
    }

    /// 把一条后端事件交给 broker 的会话槽位（`SessionBackendFactory` 拿到的是同一个 sink）。
    fn push(&self, event: EndpointEvent) {
        self.broker.sink(&self.session).send(event);
    }

    /// 驱动一次合并窗口（组合根定时器做的同一件事）。
    async fn flush(&self) -> Result<(), PortError> {
        self.broker.flush(&self.session).await
    }

    /// 该会话在 `event_type` 下的**真实落库** view 文本（按全局序）。
    async fn stored_views(&self, event_type: &str) -> Vec<String> {
        self.stored_events(event_type)
            .await
            .into_iter()
            .map(|(_, view)| view)
            .collect()
    }

    /// 同上的 `(eventId, view)` 形式（`changeId` 唯一性断言需要 id 之外的取值）。
    async fn stored_events(&self, event_type: &str) -> Vec<(String, String)> {
        let view = self.store.read_view().await.expect("读视图");
        let batch = view
            .replay(None, acp_core::ports::ReplayLimit::new(1000))
            .await
            .expect("重放");
        let mut out = Vec::new();
        for delivery in batch.events {
            let CommittedDelivery::Owned(event) = delivery else {
                continue;
            };
            if event.session.as_ref() != Some(&self.session)
                || event.event_type.as_str() != event_type
            {
                continue;
            }
            let payload = view
                .event_payload(&event.id)
                .await
                .expect("正文")
                .expect("存在");
            out.push((
                event.id.as_str().to_owned(),
                payload.view.as_str().to_owned(),
            ));
        }
        out
    }

    /// 该会话的持久化行快照。
    async fn session_row(&self) -> SessionSnapshot {
        self.store
            .load(&self.session)
            .await
            .expect("load")
            .expect("会话行存在")
    }

    /// 会话摘要里的标题（`session.list` 的取值来源）。
    async fn summary_title(&self) -> Option<String> {
        let summaries = self
            .store
            .list(acp_core::ports::SessionQuery {
                only: Some(vec![self.session.clone()]),
                states: Vec::new(),
                limit: None,
            })
            .await
            .expect("list");
        summaries
            .first()
            .expect("该会话的摘要")
            .title()
            .map(str::to_owned)
    }

    /// 全部节点级事件的 `(event_type, session_id, session_sequence, origin_epoch, origin_sequence)`。
    ///
    /// 直接用**原始 SQL** 读 `owned_event`：这是「`session_id` 为空」最强形式的观察，不依赖任何投影。
    async fn node_level_rows(&self) -> Vec<(String, Option<String>)> {
        let pool = raw_pool(&self.dir.join(DATABASE_FILE)).await;
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT event_type, session_id FROM owned_event \
             WHERE session_id IS NULL ORDER BY global_sequence ASC",
        )
        .fetch_all(&pool)
        .await
        .expect("读 owned_event");
        pool.close().await;
        rows
    }

    /// 节点级事件的四个定位列（会话级定位必须全为 `NULL`）。
    async fn node_level_locators(&self, event_type: &str) -> Vec<(Option<i64>, Option<i64>)> {
        let pool = raw_pool(&self.dir.join(DATABASE_FILE)).await;
        let rows: Vec<(Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT session_sequence, origin_sequence FROM owned_event \
             WHERE session_id IS NULL AND event_type = ?1",
        )
        .bind(event_type)
        .fetch_all(&pool)
        .await
        .expect("读 owned_event");
        pool.close().await;
        rows
    }

    /// `owned_event` 的总行数（失败关闭类断言用它证明「什么都没写」）。
    async fn event_count(&self) -> i64 {
        let pool = raw_pool(&self.dir.join(DATABASE_FILE)).await;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_event")
            .fetch_one(&pool)
            .await
            .expect("计数");
        pool.close().await;
        count
    }
}

// ---------------------------------------------------------------------------------------------
// 构造工具
// ---------------------------------------------------------------------------------------------

fn agent() -> AgentRef {
    AgentRef::try_new(AgentId::new("codex").expect("agent id"), "Codex").expect("agent ref")
}

/// JSON 字符串字面量（转义 `"`、`\` 与控制字符；路径与正文都经此进入 view）。
fn json_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// 一个类型化 Diff 元素。
fn diff_element(path: &str, old: Option<&str>, new: Option<&str>) -> String {
    let mut out = String::from("{\"path\":");
    out.push_str(&json_text(path));
    if let Some(old) = old {
        out.push_str(",\"oldText\":");
        out.push_str(&json_text(old));
    }
    if let Some(new) = new {
        out.push_str(",\"newText\":");
        out.push_str(&json_text(new));
    }
    out.push('}');
    out
}

/// `tool.call.*` 的 view（`toolCallId` + 类型化 `diff`）。
fn tool_call_view(tool_call_id: &str, diff: &[String]) -> String {
    format!(
        "{{\"toolCallId\":{},\"state\":\"in_progress\",\"diff\":[{}]}}",
        json_text(tool_call_id),
        diff.join(",")
    )
}

fn event(kind: EventKind, event_type: &str, view: &str, at: Timestamp) -> EndpointEvent {
    EndpointEvent::new(
        kind,
        EventType::new(event_type).expect("事件类型"),
        EventPayload::new(ViewJson::new(view).expect("object view"), None),
        None,
        None,
        at,
    )
}

/// 带 ACP 原文的事件（R5 的「原文三要素不变」与 R9 的两层可选都靠它）。
fn event_with_acp(
    kind: EventKind,
    event_type: &str,
    view: &str,
    acp_raw: &str,
    seed: char,
    at: Timestamp,
) -> EndpointEvent {
    let acp = AcpRaw::available("application/json", acp_raw, digest(seed)).expect("acp 原文");
    EndpointEvent::new(
        kind,
        EventType::new(event_type).expect("事件类型"),
        EventPayload::new(ViewJson::new(view).expect("object view"), Some(acp)),
        None,
        None,
        at,
    )
}

/// 规范 43 字符 base64url 摘要（末字符低 2 位为 0）。
fn digest(seed: char) -> Digest {
    let mut text: String = std::iter::repeat_n(seed, 42).collect();
    text.push('A');
    Digest::new(&text).expect("43 字符规范 base64url")
}

/// `file.changed` 的 `changeId`（形状合法且全局唯一）。
fn change_id_of(view: &str) -> String {
    let marker = "\"changeId\":\"";
    let start = view.find(marker).expect("changeId 必须在") + marker.len();
    let rest = &view[start..];
    let end = rest.find('"').expect("changeId 是字符串");
    rest[..end].to_owned()
}

/// 工作目录根的真实临时目录（**已 canonicalize**，避免 symlink/verbatim 口径差）。
///
/// Windows 的 `canonicalize` 会带 verbatim 前缀（`\\?\`、`\\?\UNC\`）；测试侧同样剥掉它，使
/// 「测试构造的路径字符串」与「core 规范化后的取值」在同一个形态上比较（否则前置条件断言会因前缀
/// 形态不同而误报）。这是测试自身的路径卫生，不复制被测逻辑——被测的判定一律由 core 完成。
fn canonical_dir(path: &Path) -> String {
    let resolved = std::fs::canonicalize(path)
        .expect("canonicalize 临时目录")
        .to_string_lossy()
        .into_owned();
    match resolved.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => resolved
            .strip_prefix(r"\\?\")
            .map(str::to_owned)
            .unwrap_or(resolved),
    }
}

// =============================================================================================
// R5：文件改动事件由类型化 Diff 派生（唯一性 / 不派生 / 原文不变）
// =============================================================================================

/// `TP2-R5-DERIVE-ONCE`：两处不同改动各派生一条；同一处改动重复出现只派生一条。
#[tokio::test]
async fn two_distinct_changes_derive_two_and_a_repeated_one_derives_once() {
    let root = temp_dir("tp2-r5-derive-root");
    std::fs::create_dir_all(root.join("src")).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r5-derive-once", Some(&canonical)).await;

    let a = PathBuf::from(&canonical).join("src").join("a.rs");
    let b = PathBuf::from(&canonical).join("src").join("b.rs");
    let diff = vec![
        diff_element(&a.to_string_lossy(), Some("let a = 1;"), Some("let a = 2;")),
        diff_element(&b.to_string_lossy(), Some("x"), Some("y")),
    ];
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view("tool-two", &diff),
        stamp(10),
    ));
    // 同一处改动的**第二次**上报（`tool.call.updated` 携带同一个 Diff 元素）。
    fixture.push(event(
        EventKind::Structured,
        "tool.call.updated",
        &tool_call_view("tool-two", &diff),
        stamp(11),
    ));
    fixture.flush().await.expect("flush");

    let changes = fixture.stored_views("file.changed").await;
    assert_eq!(
        changes.len(),
        2,
        "R5：两处不同改动各派生一条，同一处改动的重复上报不得重复派生；实际 {changes:?}"
    );
    for view in &changes {
        assert!(
            view.contains("\"displayPath\":\"src/a.rs\"")
                || view.contains("\"displayPath\":\"src/b.rs\""),
            "R7：工作区内的展示路径必须相对化，实际 {view}"
        );
        assert!(
            !view.contains(&canonical),
            "R7：不得下发工作目录根的任何片段，实际 {view}"
        );
        assert!(!view.contains(".."), "R7：不得含回退层级，实际 {view}");
    }
}

/// `TP2-R5-NO-DIFF`：不含类型化 Diff 的工具调用不派生，其它事件不受影响。
#[tokio::test]
async fn a_tool_call_without_a_typed_diff_derives_nothing() {
    let fixture = Fixture::new("tp2-r5-no-diff", None).await;
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        r#"{"toolCallId":"tool-plain","state":"in_progress","content":[{"type":"text","text":"ls"}],"terminalRef":{"terminalId":"t1"}}"#,
        stamp(10),
    ));
    // 自由形状的原始输入里「看起来像编辑」的结构也不作数（`local-agent-host` R5 的第二条场景）。
    fixture.push(event(
        EventKind::Structured,
        "tool.call.updated",
        r#"{"toolCallId":"tool-guess","state":"in_progress","rawInput":{"path":"/etc/passwd","oldText":"a","newText":"b"}}"#,
        stamp(11),
    ));
    fixture.flush().await.expect("flush");

    assert!(
        fixture.stored_views("file.changed").await.is_empty(),
        "R5：MUST NOT 从非 Diff 内容或自由形状输入推断文件改动"
    );
    // 同批的其它事件照常落库（不因派生判定而整体丢弃）。
    assert_eq!(
        fixture.stored_views("tool.call.started").await.len(),
        1,
        "R5：不含 Diff 的工具调用本身仍按原有口径落库"
    );
}

/// `TP2-R5-ACP-UNTOUCHED`：派生不改写 ACP 原文的字节、摘要与字节长度（真实存储往返）。
#[tokio::test]
async fn deriving_leaves_the_acp_document_byte_identical_through_real_storage() {
    let root = temp_dir("tp2-r5-acp-root");
    std::fs::create_dir_all(&root).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r5-acp", Some(&canonical)).await;

    let inside = PathBuf::from(&canonical).join("main.rs");
    let acp_raw = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call","toolCallId":"tool-acp","content":[{"type":"diff","path":"main.rs","oldText":"a","newText":"b"}]}}}"#;
    let expected = AcpRaw::available("application/json", acp_raw, digest('Z')).expect("acp");
    let mut started = event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-acp",
            &[diff_element(
                &inside.to_string_lossy(),
                Some("a"),
                Some("b"),
            )],
        ),
        stamp(10),
    );
    started.payload.acp = Some(expected.clone());
    fixture.push(started);
    fixture.flush().await.expect("flush");

    assert_eq!(
        fixture.stored_views("file.changed").await.len(),
        1,
        "R5：先确认本批确实派生了"
    );
    // 携带 Diff 的那条工具调用仍在库里，ACP 三要素（原文/摘要/字节长度）逐字不变。
    let stored = fixture.stored_events("tool.call.started").await;
    assert_eq!(stored.len(), 1);
    let view = fixture.store.read_view().await.expect("视图");
    let event_id = acp_core::model::EventId::new(&stored[0].0).expect("event id");
    let payload = view
        .event_payload(&event_id)
        .await
        .expect("正文")
        .expect("存在");
    assert_eq!(
        payload.acp.as_ref(),
        Some(&expected),
        "R5：派生前后的 ACP 原文必须完全相同"
    );
    assert_eq!(
        payload
            .acp
            .as_ref()
            .and_then(AcpRaw::as_available)
            .map(|(_, _, len, _)| len),
        Some(acp_raw.len() as u64),
        "R5：字节长度由原文导出，派生不得改动"
    );
}

// =============================================================================================
// R6：改动行数按行级差异统计
// =============================================================================================

/// 单一文件改动走一遍 `sink`+`flush`，返回真实落库的 `file.changed` view。
async fn one_change_view(name: &str, diff: &[String]) -> String {
    let root = temp_dir(&format!("tp2-r6-{name}-root"));
    std::fs::create_dir_all(&root).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new(&format!("tp2-r6-{name}"), Some(&canonical)).await;
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(&format!("tool-{name}"), diff),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");
    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 1, "R5：恰好一条 file.changed");
    views[0].clone()
}

/// `TP2-R6-EQUAL-LINES`：行数相等但内容不同时必须报出非零改动（不得用行数差代替行级差异）。
#[tokio::test]
async fn equal_line_counts_still_report_a_non_zero_change() {
    // 两段文本都是 2 行，内容不同。
    let view = one_change_view(
        "equal-lines",
        &[diff_element(
            "same.json",
            Some("alpha\nbeta"),
            Some("alpha\ngamma"),
        )],
    )
    .await;
    assert!(
        view.contains("\"addedLines\":\"1\"") && view.contains("\"deletedLines\":\"1\""),
        "R6：行数相等但内容不同必须报出非零改动（行数差口径会报 0），实际 {view}"
    );
}

/// `TP2-R6-NEW-FILE`：新建文件的全部行计为新增、删除为零。
#[tokio::test]
async fn a_new_file_counts_every_line_as_added() {
    let view = one_change_view(
        "new-file",
        &[diff_element(
            "fresh.rs",
            None,
            Some("line one\nline two\nline three"),
        )],
    )
    .await;
    assert!(
        view.contains("\"addedLines\":\"3\"") && view.contains("\"deletedLines\":\"0\""),
        "R6：新建文件全部计新增、删除为零，实际 {view}"
    );
    assert!(view.contains("\"kind\":\"added\""), "R6：改动方向为新增");
}

/// `TP2-R6-DELETED-FILE`：删除文件的全部行计为删除、新增为零。
#[tokio::test]
async fn a_deleted_file_counts_every_line_as_deleted() {
    let view = one_change_view(
        "deleted-file",
        &[diff_element("gone.rs", Some("a\nb\nc\nd"), Some(""))],
    )
    .await;
    assert!(
        view.contains("\"addedLines\":\"0\"") && view.contains("\"deletedLines\":\"4\""),
        "R6：删除文件全部计删除、新增为零，实际 {view}"
    );
    assert!(view.contains("\"kind\":\"deleted\""), "R6：改动方向为删除");
}

/// `TP2-R6-OMIT-UNKNOWN`：判定不出行数时两项**整个缺席**（不得以 `0` 代替未知）。
#[tokio::test]
async fn unknown_line_counts_are_omitted_rather_than_zeroed() {
    let view = one_change_view(
        "omit-unknown",
        &[diff_element("bare.txt", Some("only old text"), None)],
    )
    .await;
    assert!(
        !view.contains("addedLines") && !view.contains("deletedLines"),
        "R6：判定不出时省略而非填零，实际 {view}"
    );
    // 其余四个必填字段照常存在（省略确实是「这一项未知」，不是整条事件退化）。
    for field in ["changeId", "kind", "displayPath", "summary"] {
        assert!(view.contains(field), "R6：{field} 必须存在，实际 {view}");
    }
}

/// `TP2-R6-BUDGET`：病态输入超出预算时同样省略两项（省略口径覆盖「算不出来」的全部成因）。
#[tokio::test]
async fn an_over_budget_diff_omits_the_line_counts() {
    // 4000 行整体替换 → 编辑距离 8000，Myers 的步数预算（8_000_000）在到达最小距离前耗尽。
    let old: Vec<String> = std::iter::repeat_n("old".to_owned(), 4000).collect();
    let new: Vec<String> = std::iter::repeat_n("new".to_owned(), 4000).collect();
    let view = one_change_view(
        "over-budget",
        &[diff_element(
            "huge.txt",
            Some(&old.join("\n")),
            Some(&new.join("\n")),
        )],
    )
    .await;
    assert!(
        !view.contains("addedLines") && !view.contains("deletedLines"),
        "R6：超预算时省略两项（不得报出一个算错的数），实际 {view}"
    );
}

// =============================================================================================
// R7：展示路径相对化，且不泄漏工作区外结构
// =============================================================================================

/// `TP2-R7-INSIDE`：工作区内的路径只下发相对形式。
#[tokio::test]
async fn an_inside_path_is_relativized_without_the_root_fragment() {
    let root = temp_dir("tp2-r7-inside-root");
    std::fs::create_dir_all(root.join("deep").join("nested")).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r7-inside", Some(&canonical)).await;
    let inside = PathBuf::from(&canonical)
        .join("deep")
        .join("nested")
        .join("file.txt");
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-inside",
            &[diff_element(
                &inside.to_string_lossy(),
                Some("a"),
                Some("b"),
            )],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 1);
    assert!(
        views[0].contains("\"displayPath\":\"deep/nested/file.txt\""),
        "R7：下发的必须是相对工作目录根的形式，实际 {}",
        views[0]
    );
    assert!(
        !views[0].contains(&canonical) && !views[0].contains(".."),
        "R7：不得含根的任何片段或回退层级，实际 {}",
        views[0]
    );
    assert!(
        !views[0].contains("outsideWorkspace"),
        "R7：工作区内不得标记越界，实际 {}",
        views[0]
    );
}

/// `TP2-R7-OUTSIDE`：工作区外的路径被显式标记，且只给文件名称。
#[tokio::test]
async fn an_outside_path_is_marked_and_carries_only_the_file_name() {
    let root = temp_dir("tp2-r7-outside-root");
    std::fs::create_dir_all(&root).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r7-outside", Some(&canonical)).await;
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-outside",
            &[diff_element("/etc/nginx/nginx.conf", Some("a"), Some("b"))],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 1);
    assert!(
        views[0].contains("\"outsideWorkspace\":true"),
        "R7：越界必须显式标记，实际 {}",
        views[0]
    );
    assert!(
        views[0].contains("\"displayPath\":\"nginx.conf\""),
        "R7：越界只下发文件名称（不含任何层级结构），实际 {}",
        views[0]
    );
    assert!(
        !views[0].contains("etc") && !views[0].contains(".."),
        "R7：不得下发目录结构或回退层级，实际 {}",
        views[0]
    );
}

/// `TP2-R7-SAME-PREFIX`：字符串前缀相同但不位于其下时判为越界（规范化后的组件前缀判定）。
#[tokio::test]
async fn a_string_prefix_is_not_treated_as_inside_the_workspace() {
    let base = temp_dir("tp2-r7-prefix");
    let root_path = base.join("api");
    let sibling = base.join("api-tools");
    std::fs::create_dir_all(&root_path).expect("根");
    std::fs::create_dir_all(&sibling).expect("同前缀兄弟目录");
    std::fs::write(sibling.join("file.txt"), "x").expect("兄弟目录里的文件");
    let canonical_root = canonical_dir(&root_path);
    let fixture = Fixture::new("tp2-r7-prefix", Some(&canonical_root)).await;

    let reported = sibling.join("file.txt");
    let reported_text = reported.to_string_lossy().into_owned();
    // 前置条件必须成立，否则本断言是空转：报告路径以**该取的规范化根取值**为字符串前缀，但不位于其下。
    assert!(
        reported_text.starts_with(&canonical_root) && reported_text != canonical_root,
        "前置条件：报告路径必须与工作目录根同前缀但不在其下（{} vs {}）",
        reported.display(),
        canonical_root
    );
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-prefix",
            &[diff_element(
                &reported.to_string_lossy(),
                Some("x"),
                Some("y"),
            )],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 1);
    assert!(
        views[0].contains("\"outsideWorkspace\":true"),
        "R7：同名前缀不得被当作工作区内，实际 {}",
        views[0]
    );
    assert!(
        views[0].contains("\"displayPath\":\"file.txt\""),
        "R7：越界时只下发文件名称，实际 {}",
        views[0]
    );
}

/// `TP2-R7-NO-ROOT`：工作目录根未登记时从严判越界（绝不静默下发绝对路径）。
#[tokio::test]
async fn an_unregistered_workspace_root_falls_back_to_outside() {
    let base = temp_dir("tp2-r7-noroot");
    let target = base.join("src").join("main.rs");
    std::fs::create_dir_all(base.join("src")).expect("目录");
    std::fs::write(&target, "x").expect("文件");
    // `workspace = None` → 该会话的 `owned_session.workspace_cwd` 保持 NULL。
    let fixture = Fixture::new("tp2-r7-noroot", None).await;
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-noroot",
            &[diff_element(
                &target.to_string_lossy(),
                Some("x"),
                Some("y"),
            )],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 1);
    assert!(
        views[0].contains("\"outsideWorkspace\":true")
            && views[0].contains("\"displayPath\":\"main.rs\""),
        "R7：无法证明在工作区内时从严判越界且只给文件名称，实际 {}",
        views[0]
    );
    assert!(
        !views[0].contains("src"),
        "R7：不得下发任何层级结构，实际 {}",
        views[0]
    );
}

/// `TP2-R7-RELATIVE-ESCAPE`：相对形式逃出工作目录根时同样判越界。
#[tokio::test]
async fn a_relative_path_escaping_the_root_is_outside() {
    let root = temp_dir("tp2-r7-escape-root");
    std::fs::create_dir_all(&root).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r7-escape", Some(&canonical)).await;
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-escape",
            &[diff_element("../../etc/passwd", Some("a"), Some("b"))],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 1);
    assert!(
        views[0].contains("\"outsideWorkspace\":true")
            && views[0].contains("\"displayPath\":\"passwd\""),
        "R7：逃出根的回退层级不得下发，只给文件名称，实际 {}",
        views[0]
    );
    assert!(!views[0].contains(".."), "R7：不得含回退层级");
}

/// `TP2-R7-INSIDE-RELATIVE`：适配器给出**相对形式**时按「相对工作目录根」解释（区内，不得误判越界）。
///
/// 回归防护：WP3 修 `normalize` 的前导 `..` 计数时，最可能被连带破坏的就是这条正常路径。
#[tokio::test]
async fn a_relative_form_inside_the_workspace_is_kept_relative() {
    let root = temp_dir("tp2-r7-relative-root");
    std::fs::create_dir_all(root.join("deep")).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r7-relative", Some(&canonical)).await;
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-relative",
            &[diff_element("deep/nested.rs", Some("a"), Some("b"))],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 1);
    assert!(
        views[0].contains("\"displayPath\":\"deep/nested.rs\""),
        "R7：相对形式按工作目录根解释，实际 {}",
        views[0]
    );
    assert!(
        !views[0].contains("outsideWorkspace"),
        "R7：区内不得标记越界，实际 {}",
        views[0]
    );
}

/// `TP2-R7-ROOT-ITSELF`：改动对象恰好是工作目录根本身时不得下发根的任何片段。
#[tokio::test]
async fn the_workspace_root_itself_never_leaks_its_fragment() {
    let root = temp_dir("tp2-r7-root-itself");
    std::fs::create_dir_all(&root).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r7-root-itself", Some(&canonical)).await;
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-root",
            &[diff_element(&canonical, Some("a"), Some("b"))],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 1);
    assert!(
        !views[0].contains(&canonical) && !views[0].contains(".."),
        "R7：不得含工作目录根的任何片段或回退层级，实际 {}",
        views[0]
    );
}

/// `TP2-R7-INNER-DOTDOT`：区内路径**内部**的 `..` 只要规范化后仍在区内，就必须判为区内（不得一刀切）。
#[tokio::test]
async fn an_inner_dotdot_that_stays_inside_is_inside() {
    let root = temp_dir("tp2-r7-inner-root");
    std::fs::create_dir_all(root.join("sub")).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r7-inner", Some(&canonical)).await;
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-inner",
            &[diff_element("sub/../file.txt", Some("a"), Some("b"))],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 1);
    assert!(
        views[0].contains("\"displayPath\":\"file.txt\"") && !views[0].contains("outsideWorkspace"),
        "R7：规范化后仍在区内，不得误判越界，实际 {}",
        views[0]
    );
}

/// `TP2-R7-ABS-INNER-DOTDOT`：**绝对**路径内部含 `..` 但规范化后仍在区内 → 仍判区内。
///
/// 回归防护（Main 点名的 `「区内含 .. 但未越界」` 的绝对形式）：修复前导 `..` 计数时若把「路径里出现
/// `..`」一律判越界，这条会红。
#[tokio::test]
async fn an_absolute_path_with_an_inner_dotdot_that_stays_inside_is_inside() {
    let root = temp_dir("tp2-r7-abs-inner-root");
    std::fs::create_dir_all(root.join("api").join("sub")).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r7-abs-inner", Some(&canonical)).await;
    let reported = PathBuf::from(&canonical)
        .join("api")
        .join("..")
        .join("api")
        .join("file.txt");
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-abs-inner",
            &[diff_element(
                &reported.to_string_lossy(),
                Some("a"),
                Some("b"),
            )],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 1);
    assert!(
        views[0].contains("\"displayPath\":\"api/file.txt\"")
            && !views[0].contains("outsideWorkspace")
            && !views[0].contains(".."),
        "R7：绝对路径内部的 `..` 规范化后仍在区内，必须判区内且不下发 `..`，实际 {}",
        views[0]
    );
}

/// `TP2-R7-ESCAPE-MANY`：**多级**前导 `..` 同样必须只给文件名称（`design.md` D5 点名的动机输入）。
#[tokio::test]
async fn multiple_leading_parent_traversals_keep_only_the_file_name() {
    let root = temp_dir("tp2-r7-many-root");
    std::fs::create_dir_all(&root).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r7-many", Some(&canonical)).await;
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-many",
            &[
                diff_element("../../etc/passwd", Some("a"), Some("b")),
                diff_element("../../Users/alice/.ssh/id_rsa", Some("c"), Some("d")),
            ],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 2, "两处越界改动各派一条");
    for view in &views {
        assert!(
            view.contains("\"outsideWorkspace\":true"),
            "R7：多级前导 `..` 必须判越界，实际 {view}"
        );
        assert!(!view.contains(".."), "R7：不得下发回退层级，实际 {view}");
        // 展示值只能是文件名称：不得出现任何目录片段。
        assert!(
            view.contains("\"displayPath\":\"passwd\"")
                || view.contains("\"displayPath\":\"id_rsa\""),
            "R7：越界只给文件名称，实际 {view}"
        );
        assert!(
            !view.contains("etc/") && !view.contains("Users/") && !view.contains(".ssh/"),
            "R7：不得泄漏工作区外的目录结构，实际 {view}"
        );
    }
}

/// `TP2-R7-ABS-OUTSIDE`：另一处工作区外的**绝对**路径同样只给文件名称（不同根与不同文件名）。
#[tokio::test]
async fn another_absolute_outside_path_keeps_only_the_file_name() {
    let root = temp_dir("tp2-r7-abs-outside-root");
    std::fs::create_dir_all(&root).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r7-abs-outside", Some(&canonical)).await;
    let outside = temp_dir("tp2-r7-elsewhere");
    std::fs::write(outside.join("secret.txt"), "x").expect("区外文件");
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-abs-outside",
            &[diff_element(
                &outside.join("secret.txt").to_string_lossy(),
                Some("a"),
                Some("b"),
            )],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let views = fixture.stored_views("file.changed").await;
    assert_eq!(views.len(), 1);
    assert!(
        views[0].contains("\"outsideWorkspace\":true")
            && views[0].contains("\"displayPath\":\"secret.txt\""),
        "R7：区外绝对路径只给文件名称，实际 {}",
        views[0]
    );
}

/// `TP2-R5-R7-SAME-NAMED-OUTSIDE`：两个同名但不同目录的越界文件是**两处**改动。
///
/// 判别力：展示路径在越界时按 R7 收窄为 `file_name()`，两者取值相同；若去重键/`changeId` 取展示路径，
/// 第二条会被当成「同一处改动」而静默丢掉（这正是 `review-w3-r1-F2` 的缺陷形状）。本用例经真实存储
/// 断言两条事件都落库、`changeId` 互不相同、展示值仍只给文件名。
#[tokio::test]
async fn two_same_named_outside_files_derive_two_changes_on_real_storage() {
    let root = temp_dir("tp2-r7-same-named-root");
    std::fs::create_dir_all(&root).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r7-same-named", Some(&canonical)).await;
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-same-named",
            &[
                diff_element("/etc/nginx/nginx.conf", Some("a"), Some("b")),
                diff_element("/tmp/nginx.conf", Some("c"), Some("d")),
            ],
        ),
        stamp(10),
    ));
    fixture.flush().await.expect("flush");

    let events = fixture.stored_events("file.changed").await;
    assert_eq!(
        events.len(),
        2,
        "R5：两处同名越界改动各派生一条（展示路径有损不得用作判据），实际 {events:?}"
    );
    let ids: Vec<String> = events.iter().map(|(_, view)| change_id_of(view)).collect();
    assert_ne!(ids[0], ids[1], "R5：同名越界文件的 changeId 必须互不相同");
    for (_, view) in &events {
        assert!(
            view.contains("\"displayPath\":\"nginx.conf\"")
                && view.contains("\"outsideWorkspace\":true"),
            "R7：越界展示值仍只给文件名称，实际 {view}"
        );
        assert!(
            !view.contains("/etc") && !view.contains("/tmp"),
            "R7：不得泄漏越界路径的目录结构，实际 {view}"
        );
    }
}

// =============================================================================================
// R8：Agent 连接状态是节点级生命周期
// =============================================================================================

/// `TP2-R8-NODE-NULL-SESSION`：节点级事件落库时会话标识为空，且四个会话级定位列全为 `NULL`。
#[tokio::test]
async fn node_level_events_land_without_session_identity() {
    let fixture = Fixture::new("tp2-r8-node-level", None).await;
    for (event_type, view) in [
        (
            "agent.connected",
            r#"{"agentId":"codex","state":"connected"}"#,
        ),
        (
            "agent.disconnected",
            r#"{"agentId":"codex","state":"disconnected","error":{"code":"agent.exit","message":"idle reclaim"}}"#,
        ),
    ] {
        fixture
            .broker
            .commit_node_event(event(EventKind::State, event_type, view, stamp(20)))
            .await
            .expect("节点级事件落库");
    }

    let rows = fixture.node_level_rows().await;
    assert_eq!(
        rows.iter()
            .map(|(kind, _)| kind.as_str())
            .collect::<Vec<_>>(),
        vec!["agent.connected", "agent.disconnected"],
        "R8：两类节点级事件各落一行，实际 {rows:?}"
    );
    for (kind, session) in &rows {
        assert!(
            session.is_none(),
            "R8：节点级事件的会话标识必须为空（{kind}），实际 {session:?}"
        );
    }
    for event_type in ["agent.connected", "agent.disconnected"] {
        let locators = fixture.node_level_locators(event_type).await;
        assert_eq!(locators.len(), 1, "R8：恰一行 {event_type}");
        assert_eq!(
            locators[0],
            (None, None),
            "R8：节点级事件不得有会话级序号（session_sequence/origin_sequence 必须为 NULL）"
        );
    }
    // 契约面（replay）与原始表一致：节点级事件进入全局重放流，但仍不带会话定位。
    let view = fixture.store.read_view().await.expect("视图");
    let batch = view
        .replay(None, acp_core::ports::ReplayLimit::new(100))
        .await
        .expect("重放");
    let node_events: Vec<_> = batch
        .events
        .iter()
        .filter_map(|delivery| match delivery {
            CommittedDelivery::Owned(event) if event.event_type.as_str().starts_with("agent.") => {
                Some(event)
            }
            _ => None,
        })
        .collect();
    assert_eq!(node_events.len(), 2, "R8：两条节点级事件都进重放流");
    for event in node_events {
        assert!(event.session.is_none(), "R8：replay 面同样无会话标识");
        assert!(event.session_sequence.is_none());
        assert!(event.origin_epoch.is_none());
        assert!(event.origin_sequence.is_none());
        assert!(event.global_sequence.get() > 0, "R8：仍有全局序号");
    }
}

/// `TP2-R8-FAIL-CLOSED`：形状不符的节点级事件失败关闭，且**什么都不写**（不静默丢弃、不降级写入）。
#[tokio::test]
async fn node_level_shape_violations_fail_closed_without_writing() {
    let fixture = Fixture::new("tp2-r8-fail-closed", None).await;
    let before = fixture.event_count().await;

    let cases: Vec<(&str, EventKind, &str, &str)> = vec![
        // 事件类型不在节点级词表内。
        (
            "非节点级事件类型",
            EventKind::State,
            "session.mode.changed",
            r#"{"currentModeId":"code"}"#,
        ),
        // `state` 与事件类型不一致（各自的取值唯一且互不相同）。
        (
            "state 与事件类型不一致",
            EventKind::State,
            "agent.connected",
            r#"{"agentId":"codex","state":"disconnected"}"#,
        ),
        (
            "state 是词表外的自由文本",
            EventKind::State,
            "agent.disconnected",
            r#"{"agentId":"codex","state":"busy"}"#,
        ),
        // 缺 `agentId` 与**空串** `agentId`（schema 是 `minLength: 1`）。
        (
            "缺 agentId",
            EventKind::State,
            "agent.connected",
            r#"{"state":"connected"}"#,
        ),
        (
            "空串 agentId",
            EventKind::State,
            "agent.connected",
            r#"{"agentId":"","state":"connected"}"#,
        ),
        // 类别不是 state。
        (
            "类别不是 state",
            EventKind::Structured,
            "agent.connected",
            r#"{"agentId":"codex","state":"connected"}"#,
        ),
    ];
    for (label, kind, event_type, view) in cases {
        let result = fixture
            .broker
            .commit_node_event(event(kind, event_type, view, stamp(30)))
            .await;
        assert!(
            result.is_err(),
            "R8：{label} 必须失败关闭（实际 {result:?}）"
        );
    }
    // 携带 turn 归属的节点级事件（不属于任何 turn）。
    let mut with_turn = event(
        EventKind::State,
        "agent.connected",
        r#"{"agentId":"codex","state":"connected"}"#,
        stamp(31),
    );
    with_turn.turn = Some(TurnId::new("33333333-3333-4333-8333-333333333333").expect("turn"));
    assert!(fixture.broker.commit_node_event(with_turn).await.is_err());
    // 携带 ACP 原文的节点级事件（节点级事件没有 ACP 原文）。
    let mut with_acp = event(
        EventKind::State,
        "agent.connected",
        r#"{"agentId":"codex","state":"connected"}"#,
        stamp(32),
    );
    with_acp.payload.acp =
        Some(AcpRaw::available("application/json", "{}", digest('A')).expect("acp"));
    assert!(fixture.broker.commit_node_event(with_acp).await.is_err());

    assert_eq!(
        fixture.event_count().await,
        before,
        "R8：失败关闭必须不产生任何事件行（既不静默丢弃也不降级写入）"
    );
    assert!(
        fixture.node_level_rows().await.is_empty(),
        "R8：非法节点级事件不得落库"
    );
}

/// `TP2-R8-NOT-SESSION-ACTIVITY`：节点级连接事件不表达会话活跃程度——会话侧的事件再多也不据此产生。
#[tokio::test]
async fn session_activity_never_manufactures_a_connection_event() {
    let root = temp_dir("tp2-r8-activity-root");
    std::fs::create_dir_all(&root).expect("工作目录");
    let canonical = canonical_dir(&root);
    let fixture = Fixture::new("tp2-r8-activity", Some(&canonical)).await;

    // 一段典型的会话活跃：文件改动 + 标题更新。
    let file = PathBuf::from(&canonical).join("busy.rs");
    fixture.push(event(
        EventKind::Structured,
        "tool.call.started",
        &tool_call_view(
            "tool-busy",
            &[diff_element(&file.to_string_lossy(), Some("a"), Some("b"))],
        ),
        stamp(40),
    ));
    fixture.push(event_with_acp(
        EventKind::State,
        "session.info.changed",
        r#"{"title":"活跃会话","updatedAt":"2026-10-05T00:00:00.000Z"}"#,
        r#"{"params":{"update":{"sessionUpdate":"session_info_update","title":"活跃会话"}}}"#,
        'B',
        stamp(41),
    ));
    fixture.flush().await.expect("flush");

    assert_eq!(
        fixture.stored_views("file.changed").await.len(),
        1,
        "前置条件：会话侧确实产出了事件"
    );
    assert_eq!(
        fixture.summary_title().await.as_deref(),
        Some("活跃会话"),
        "前置条件：会话标题已更新"
    );
    assert!(
        fixture.node_level_rows().await.is_empty(),
        "R8：会话活跃不得产生任何 Agent 连接/断开事件"
    );
    // 会话状态只体现在会话行自己的状态字段上（`sessionSummary.state` 的来源），与节点级事件无关。
    let state = fixture.session_row().await.session.state();
    assert!(
        !matches!(state, SessionState::Closed | SessionState::Failed),
        "R8：会话状态由会话行承载，实际 {state:?}"
    );
}

// =============================================================================================
// R9：会话标题只由 Agent 通知更新
// =============================================================================================

/// ACP `session_info_update` 通知（字段可部分更新）。
fn session_info_acp(title_field: Option<&str>) -> String {
    match title_field {
        Some(field) => format!(
            r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"s","update":{{"sessionUpdate":"session_info_update","title":{field},"updatedAt":"2020-01-01T00:00:00.000Z"}}}}}}"#
        ),
        None => r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"session_info_update","updatedAt":"2020-01-01T00:00:00.000Z"}}}"#.to_owned(),
    }
}

/// 把一条 `session.info.changed` 通知交给 broker 并落盘。
async fn push_info_update(
    fixture: &Fixture,
    title_field: Option<&str>,
    view_title: &str,
    at: Timestamp,
) {
    let view = format!(
        "{{\"title\":{},\"updatedAt\":\"2020-01-01T00:00:00.000Z\"}}",
        if view_title == "null" {
            "null".to_owned()
        } else {
            json_text(view_title)
        }
    );
    fixture.push(event_with_acp(
        EventKind::State,
        "session.info.changed",
        &view,
        &session_info_acp(title_field),
        'C',
        at,
    ));
    fixture.flush().await.expect("标题通知落库");
}

/// `TP2-R9-SET`：首次通知写入标题，摘要随之反映，且权威更新时间取 Daemon 持久化时间。
#[tokio::test]
async fn the_first_notification_writes_the_title_and_the_summary_follows() {
    let fixture = Fixture::new("tp2-r9-set", None).await;
    assert_eq!(
        fixture.session_row().await.session.title(),
        None,
        "R9：创建时标题为空"
    );
    push_info_update(&fixture, Some("\"新标题\""), "新标题", stamp(50)).await;

    let row = fixture.session_row().await;
    assert_eq!(
        row.session.title(),
        Some("新标题"),
        "R9：标题来自 Agent 的通知"
    );
    assert_ne!(
        row.session.updated_at().as_str(),
        "2020-01-01T00:00:00.000Z",
        "R9：权威更新时间取 Daemon 持久化时间，不采用 Agent 自报值"
    );
    assert_eq!(
        fixture.summary_title().await.as_deref(),
        Some("新标题"),
        "R9：会话摘要必须反映标题（客户端目录页的来源）"
    );
    // 事件侧的 view 照常转发 Agent 自报的 `updatedAt`（转发与权威时间是两件事）。
    let events = fixture.stored_views("session.info.changed").await;
    assert_eq!(events.len(), 1);
    assert!(
        events[0].contains("\"updatedAt\":\"2020-01-01T00:00:00.000Z\""),
        "R9：事件 view 仍转发 Agent 自报的更新时间，实际 {}",
        events[0]
    );
}

/// `TP2-R9-KEEP`：通知只携带更新时间（ACP 原文无 `title` 键）时既有标题不变。
#[tokio::test]
async fn a_notification_without_a_title_keeps_the_existing_one() {
    let fixture = Fixture::new("tp2-r9-keep", None).await;
    push_info_update(&fixture, Some("\"保留我\""), "保留我", stamp(51)).await;
    let after_first = fixture.session_row().await;
    assert_eq!(after_first.session.title(), Some("保留我"));

    // 只有更新时间：ACP 原文里没有 `title` 键（公共 view 的 `title` 是 `null` 也无法区分，故读原文）。
    // **判别力构造**：这条通知事件的 `at` 故意取一个**早于**上一次写入的取值（`stamp(1)` < `stamp(51)`）。
    // 若实现把权威更新时间取成**事件自报时间**，该列会**倒退**；只有取 Daemon 提交时钟（`commit.at`）
    // 才会前进。这正是「用了提交时钟」与「用了事件时间」的分界。
    push_info_update(&fixture, None, "null", stamp(1)).await;
    let after_second = fixture.session_row().await;
    assert_eq!(
        after_second.session.title(),
        Some("保留我"),
        "R9：只有更新时间时标题必须保持不变"
    );
    assert!(
        after_second.session.updated_at().as_str() > after_first.session.updated_at().as_str(),
        "R9/D7：`updated_at` 必须前进到 Daemon 提交时钟（不得取事件自报时间，否则倒退）；实际 {} → {}",
        after_first.session.updated_at(),
        after_second.session.updated_at()
    );
    assert!(
        !after_second
            .session
            .updated_at()
            .as_str()
            .starts_with("2020-"),
        "R9/D7：权威更新时间不得取 Agent 自报值，实际 {}",
        after_second.session.updated_at()
    );
}

/// `TP2-R9-CLEAR`：通知显式把标题置空时呈现为未命名会话。
#[tokio::test]
async fn an_explicit_null_clears_the_title_into_unnamed() {
    let fixture = Fixture::new("tp2-r9-clear", None).await;
    push_info_update(&fixture, Some("\"先有标题\""), "先有标题", stamp(53)).await;
    assert_eq!(
        fixture.session_row().await.session.title(),
        Some("先有标题")
    );

    push_info_update(&fixture, Some("null"), "null", stamp(54)).await;
    assert_eq!(
        fixture.session_row().await.session.title(),
        None,
        "R9：显式置空 → 未命名会话"
    );
    assert_eq!(
        fixture.summary_title().await,
        None,
        "R9：摘要同样呈现为未命名"
    );
}

/// `TP2-R9-CLEAR-WITH-MODE-CHANGE`：同批含要求注入 `version` 的事件时，整批不丢、标题照写。
///
/// 判别力（`review-w3-r1-F1` 的缺陷形状）：存储层若为「显式置空」替换成一条只写 `title` 的窄语句，
/// 版本不递增 → core 的 `commit_owned` 判出版本漂移 → `PortError::Corrupt` → **整批事件永久丢失**。
/// 本用例经**真实 broker + 真实存储**断言提交成功、两条事件都落库、标题写入。
#[tokio::test]
async fn clearing_a_title_in_a_batch_with_a_version_event_keeps_the_whole_batch() {
    let fixture = Fixture::new("tp2-r9-clear-with-mode", None).await;
    push_info_update(&fixture, Some("\"将被置空\""), "将被置空", stamp(55)).await;
    assert_eq!(
        fixture.session_row().await.session.title(),
        Some("将被置空")
    );

    // `session.mode.changed` 的 view 缺 `version` → core 在提交前按「含 StateChange ⇒ 当前 + 1」注入，
    // 并在提交后与存储层返回值比对（`§10.3` 的漂移检测）。
    fixture.push(event(
        EventKind::State,
        "session.mode.changed",
        r#"{"currentModeId":"code"}"#,
        stamp(56),
    ));
    push_info_update(&fixture, Some("null"), "null", stamp(57)).await;

    let row = fixture.session_row().await;
    assert_eq!(row.session.title(), None, "R9：显式置空必须真的写 NULL");
    let mode_events = fixture.stored_views("session.mode.changed").await;
    assert_eq!(
        mode_events.len(),
        1,
        "F1：同批的 `session.mode.changed` 不得被整批丢弃"
    );
    // §10.3 的版本规则：core 注入「当前 + 1」，存储层必须返回同一个值（否则 core 判漂移 → Corrupt）。
    // 断言两处取值**相等**而不是某个硬编码数字：本批含 `StateChange`，故它就是落盘后的会话版本。
    assert!(
        mode_events[0].contains(&format!("\"version\":\"{}\"", row.session.version().get())),
        "§10.3：注入的版本必须等于递增后的会话版本（{}），实际 {}",
        row.session.version().get(),
        mode_events[0]
    );
    assert_eq!(
        fixture.stored_views("session.info.changed").await.len(),
        2,
        "F1：标题通知事件同样不得被丢弃"
    );
    assert!(
        row.session.version().get() > 1,
        "F1：含 StateChange 的提交必须递增版本，实际 {}",
        row.session.version().get()
    );
}

/// `TP2-R9-STORE-UNCHANGED-KEEP`：`ModeChange::Unchanged` 下「不改标题」必须保留既有标题，且版本与时间照常推进。
///
/// 覆盖缺口（`title_write.rs` 的补充判据）：既有用例只在 `ModeChange::Set` 语句下验证过 `title: None`
/// 的「不改」语义；组合根路径（`commit_chunk`）恒用 `ModeChange::Unchanged`，而该语句的标题 CASE 是
/// `CASE WHEN ?9 THEN NULL WHEN ?8 IS NULL THEN title ELSE ?8 END`——`?8 IS NULL` 这一支只在这里被钉住。
#[tokio::test]
async fn unchanged_mode_without_a_title_keeps_it_and_still_advances_the_version() {
    let fixture = Fixture::new("tp2-r9-store-unchanged-keep", None).await;
    let write_title = |value: Option<Option<String>>| OwnedCommit {
        session: Some(fixture.session.clone()),
        at: stamp(62),
        expected_version: None,
        state: Some(StateChange::Update(SessionUpdate {
            state: None,
            mode: ModeChange::Unchanged,
            title: value,
            closed_at: None,
            interaction: None,
            agent_session_id: None,
            workspace_cwd: None,
            workspace_alias: None,
        })),
        turns: Vec::new(),
        events: Vec::new(),
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: None,
        command_terminal: None,
        origin_epoch: None,
    };
    fixture
        .store
        .commit(write_title(Some(Some("保留我".to_owned()))))
        .await
        .expect("先写入标题");
    let before = fixture.session_row().await;

    let outcome = fixture
        .store
        .commit(write_title(None))
        .await
        .expect("不改标题的提交必须成功");
    assert_eq!(
        outcome.version.get(),
        before.session.version().get() + 1,
        "含 StateChange ⇒ 版本 +1"
    );
    let after = fixture.session_row().await;
    assert_eq!(
        after.session.title(),
        Some("保留我"),
        "R9：`None` = 不改该列，既有标题不得被清空"
    );
    assert_eq!(
        after.session.updated_at().as_str(),
        stamp(62).as_str(),
        "权威更新时间取 commit.at"
    );
}

/// `TP2-R9-NO-RENAME-ENTRY`：命令目录中不存在重命名会话的命令，core 也没有写标题的第二条路径。
#[tokio::test]
async fn the_command_catalog_has_no_rename_entry() {
    // ① 机器目录（`npm run check` 的 `check:commands` 读的就是它）里没有任何改名命令。
    const CATALOG: &str = include_str!("../../../compatibility/commands/v1/commands.json");
    let names: Vec<&str> = CATALOG
        .match_indices("\"name\": \"")
        .map(|(index, _)| {
            let rest = &CATALOG[index + "\"name\": \"".len()..];
            &rest[..rest.find('"').expect("命令名以引号结束")]
        })
        .collect();
    assert!(
        !names.is_empty(),
        "前置条件：命令目录必须可解析出命令名（否则本断言是空转）"
    );
    assert!(
        !names.iter().any(|name| name.contains("rename")
            || name.contains("title")
            || name.contains("info.set")),
        "R9：命令目录中不得存在任何修改会话标题的命令，实际 {names:?}"
    );
    // ② core 的授权镜像里也没有这条命令（`required_grant` 是命令目录的手工镜像）。
    assert!(
        acp_core::broker::required_grant("session.rename").is_none(),
        "R9：core 不得为任何重命名命令登记授权"
    );
    // ③ 命令目录里的每一条都在 core 的镜像里（守卫：命令名与镜像不得漂移，避免 ② 变成空转）。
    for name in &names {
        assert!(
            acp_core::broker::required_grant(name).is_some(),
            "R9：命令目录的命令 {name} 必须在 core 的授权镜像里"
        );
    }
}

/// `TP2-R9-STORE-UNCHANGED-CLEAR`：`ModeChange::Unchanged` 下显式置空标题仍写版本、时间与同批其它列。
///
/// 补充 `title_write.rs` 的覆盖：既有用例覆盖 `Unchanged + title: None` 与 `Set + Some(_)`，
/// 但 `Unchanged + Some(None) + 同批其它列`（该语句的 `?1..?9` 绑定编号）此前没有用例；组合根路径
/// （`commit_chunk` 恒用 `ModeChange::Unchanged`）正是这条语句。
#[tokio::test]
async fn unchanged_mode_title_clear_writes_every_column_in_the_batch() {
    let fixture = Fixture::new("tp2-r9-store-unchanged", None).await;
    // 先写入一个标题。
    fixture
        .store
        .commit(OwnedCommit {
            session: Some(fixture.session.clone()),
            at: stamp(60),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                title: Some(Some("先有".to_owned())),
                closed_at: None,
                interaction: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_alias: None,
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        })
        .await
        .expect("写标题");
    let before = fixture.session_row().await;

    // `Unchanged` + 显式置空 + 同批的状态与恢复列。
    let outcome = fixture
        .store
        .commit(OwnedCommit {
            session: Some(fixture.session.clone()),
            at: stamp(61),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: Some(SessionState::WaitingInput),
                mode: ModeChange::Unchanged,
                title: Some(None),
                closed_at: None,
                interaction: None,
                agent_session_id: Some(
                    AgentSessionId::new(AGENT_SESSION_ID).expect("agent session id"),
                ),
                workspace_cwd: Some("/tmp/tp2-unchanged".to_owned()),
                workspace_alias: Some("tp2.unchanged".to_owned()),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        })
        .await
        .expect("Unchanged 分支的显式置空必须提交成功");

    assert_eq!(
        outcome.version.get(),
        before.session.version().get() + 1,
        "F1：含 StateChange ⇒ 版本 +1（不得返回未递增的旧值）"
    );
    let after = fixture.session_row().await;
    assert_eq!(after.session.title(), None, "显式置空必须真的写 NULL");
    assert_eq!(
        after.session.version().get(),
        outcome.version.get(),
        "落盘版本必须与 CommitOutcome 一致（core 的漂移检测依据）"
    );
    assert_eq!(
        after.session.updated_at().as_str(),
        stamp(61).as_str(),
        "F1：`updated_at` 必须前进到 commit.at"
    );
    // 同批的其它列不得被吞掉（`ModeChange::Unchanged` 语句的绑定编号判据）。
    assert_eq!(
        after.session.state(),
        SessionState::WaitingInput,
        "同批的 state 不得被吞掉"
    );
    let recovery = fixture
        .store
        .load_recovery(&fixture.session)
        .await
        .expect("load_recovery")
        .expect("两列都有值");
    assert_eq!(
        recovery
            .agent_session_id
            .as_ref()
            .map(AgentSessionId::as_str),
        Some(AGENT_SESSION_ID),
        "同批的 agent_session_id 不得被吞掉"
    );
    assert_eq!(
        recovery.workspace_cwd.as_deref(),
        Some("/tmp/tp2-unchanged"),
        "同批的 workspace_cwd 不得被吞掉"
    );
}

// =============================================================================================
// 夹具自身的健全性（防空转）：证明上述断言真的跑在真实存储上
// =============================================================================================

/// `TP2-FIXTURE-SANITY`：夹具确实把真实 `SqliteStore` 接进了 broker（事件确实落到磁盘上的库文件）。
#[tokio::test]
async fn the_fixture_commits_into_a_real_database_file() {
    let fixture = Fixture::new("tp2-fixture-sanity", None).await;
    assert!(
        fixture.dir.join(DATABASE_FILE).is_file(),
        "夹具必须打开真实库文件"
    );
    assert_eq!(fixture.event_count().await, 0, "初始无事件");
    fixture.push(event(
        EventKind::State,
        "session.mode.changed",
        r#"{"currentModeId":"code"}"#,
        stamp(70),
    ));
    fixture.flush().await.expect("flush");
    assert_eq!(
        fixture.event_count().await,
        1,
        "broker 的提交必须真的落到库文件（而不是内存替身）"
    );
}
