//! `server::sync::event` 的行为用例（变更 `sync-server-commands` 的任务 1.2–1.6）。
//!
//! 驱动方式是**真实出站队列**：`deliver` 把帧交给 `ConnectionHandle::send`，用例只看对端真的收到
//! 的那几帧（`fixture::frames`）与出站账本的水位（`saturated`/`pending`）。因此「判定失败仍下发」
//! 「未协商不携带原文」「降级不截断」「水位触发停读」都是对**将发出的字节与队列状态**的断言，
//! 而不是对内部结构的断言。
//!
//! 两类观测口：
//!
//! - **帧序列**：`fixture::bodies(&frames, "event")` 给出入队顺序上的 body，用 `rawJson`/`view`
//!   的**文本**比对逐字节保真（不经 `serde_json::Value` 中转，否则大整数字面量会被改写）；
//! - **结构化了志**：一个最小 `Subscriber` 把 `sync.event_view_shape_rejected` /
//!   `sync.event_type_unregistered` 的字段收进字符串（与 `sync::pairing` 的用例同一形状，
//!   不引入新依赖）。`InterestGate` 的理由见 `sync::pairing` 的同名实现：tracing 的 interest
//!   缓存是进程级的，必须先有一个全局订阅器把它固定为 `always`，否则并行跑时会偶发假失败。

use std::collections::BTreeMap;
use std::sync::Arc;

use acp_core::model::{
    CommittedEvent, Digest, EventId, EventOrigin, EventPayload as CoreEventPayload, EventType,
    OriginEpoch, Sequence, SessionId, Timestamp, ViewJson,
};
use acp_core::ports::SyncEventRecord;
use base64::Engine as _;
use sha2::{Digest as _, Sha256};
use tracing::field::{Field, Visit};
use tracing::level_filters::LevelFilter;
use tracing::span::{Attributes, Id as SpanId, Record as SpanRecord};
use tracing::subscriber::Interest;
use tracing::{Event, Level, Metadata, Subscriber};

use super::*;

use crate::sync::conn::registry::Outbound;
use crate::sync::subscribe::fixture::{self, DEVICE, Frame};

/// 已协商原文能力的连接（`acp.raw-payload.v1`）。
///
/// **同时含 `core.event-origin.v1`**：两条协商集合必须只差那一个 feature，否则「协商与未协商的
/// 其余字段一致」这类比对会掺进 `origin` 形状的差异（`nodeId` 在场与否），变成比苹果和橘子。
fn with_raw_payload() -> NegotiatedFeatures {
    NegotiatedFeatures::new(vec![
        EVENT_ORIGIN_FEATURE.to_owned(),
        RAW_PAYLOAD_FEATURE.to_owned(),
    ])
}

/// 未协商原文能力的连接（协商集合里只有 `core.event-origin.v1`）。
fn without_raw_payload() -> NegotiatedFeatures {
    NegotiatedFeatures::new(vec![EVENT_ORIGIN_FEATURE.to_owned()])
}

/// `Digest` 的构造（`AcpRaw::available` 要求 43 字符无填充 base64url）。
fn digest_of(text: &str) -> Digest {
    let bytes = Sha256::digest(text.as_bytes());
    Digest::new(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)).expect("摘要文本")
}

/// 一条合法 UUID 的固定事件 id（末段两位十进制派生，与 `subscribe::fixture` 同手法）。
fn event_id(sequence: u64) -> EventId {
    EventId::new(&format!("da702ba1-f49f-4e16-a677-a7744dfedb{sequence:02}")).expect("event id")
}

/// 一条 `agent` 来源、无 causation 的已持久化事件（定位字段齐全，正文由调用方给）。
fn record(sequence: u64, event_type: &str, payload: CoreEventPayload) -> SyncEventRecord {
    SyncEventRecord {
        event: CommittedEvent {
            id: event_id(sequence),
            event_type: EventType::new(event_type).expect("合法 eventType"),
            session: Some(SessionId::new(fixture::SESSION).expect("session id")),
            session_sequence: Some(Sequence::new(sequence).expect("session sequence")),
            global_sequence: Sequence::new(sequence).expect("global sequence"),
            origin_epoch: Some(OriginEpoch::new(fixture::ORIGIN_EPOCH).expect("origin epoch")),
            origin_sequence: Some(Sequence::new(sequence).expect("origin sequence")),
            created_at: Timestamp::new("2026-09-17T12:10:00.123Z").expect("时间戳"),
        },
        payload,
        origin_kind: EventOrigin::Agent,
        origin_device_id: None,
        origin_node_id: None,
        causation: None,
    }
}

/// `turn.started` 的最低字段视图（登记类型 + 形状完整）。
///
/// 两个字段都是 `views::TurnStarted` 的必填项：`turnId` 是 canonical UUID、`state` 是非空文本
/// （`crates/sync-protocol/src/views.rs` 的 `TurnStarted`）。少任何一个都会落进
/// [`ViewShape::Malformed`]，那样「形状完整」的用例就变成了在证明失败路径。
fn turn_started(sequence: u64) -> SyncEventRecord {
    record(
        sequence,
        "turn.started",
        CoreEventPayload::new(
            ViewJson::new(r#"{"turnId":"7f3e9a10-5c2b-4d8e-9f01-3a6b5c7d8e9f","state":"running"}"#)
                .expect("view"),
            None,
        ),
    )
}

/// 同一 `turn.started` 但**缺 `state`**：登记类型 + 形状缺失（1.2 的降级路径）。
fn turn_started_without_state(sequence: u64) -> SyncEventRecord {
    record(
        sequence,
        "turn.started",
        CoreEventPayload::new(
            ViewJson::new(r#"{"turnId":"7f3e9a10-5c2b-4d8e-9f01-3a6b5c7d8e9f"}"#).expect("view"),
            None,
        ),
    )
}

/// 一个含**超出 64 位整数字面量**与**未识别字段**的 ACP 原文档（§3.3/§10.3 的对抗素材）。
///
/// 空白、键顺序、转义与数字文本形式都是判据：任何「解析后重新序列化」的实现都会在这里被抓住。
const ADVERSARIAL_RAW: &str = r#"{ "id" : 18446744073709551617 ,"extra":{ "note":"未识别字段" }, "text":"制表符\t与引号\"与反斜杠\\" }"#;

/// 一个**字段齐全但字节不规整**的 `turn.started` view：空白不对称、键序与规范序相反。
///
/// 判定为 `Complete`（`turnId` 是 canonical UUID、`state` 是非空文本），因此它同时是
/// 「形状完整时仍逐字节下发原文」的素材 —— 任何「解析后重新序列化」都会把它规整成
/// `{"state":"running","turnId":"…"}`（键序与空白都变），从而被帧文本断言抓住。
const ADVERSARIAL_VIEW: &str =
    r#"{ "state" : "running" ,  "turnId":"7f3e9a10-5c2b-4d8e-9f01-3a6b5c7d8e9f" }"#;

/// 投递一条事件并排空出站队列，返回对端收到的帧。
fn deliver_once(
    handle: &ConnectionHandle,
    record: &SyncEventRecord,
    features: &NegotiatedFeatures,
    receiver: &mut tokio::sync::mpsc::Receiver<Outbound>,
) -> (EventDelivery, Vec<Frame>) {
    let outcome = deliver(handle, record, features);
    (outcome, fixture::frames(receiver))
}

/// 一条已认证连接的句柄（出站队列的接收端交回用例）。
fn connection() -> (Arc<ConnectionHandle>, tokio::sync::mpsc::Receiver<Outbound>) {
    fixture::connection_with(DEVICE, fixture::CONNECTION)
}

/// 事件 body 的 `payload`（`view`/`acp` 两个键都在这里）。
fn payload_of(body: &serde_json::Value) -> &serde_json::Value {
    &body["payload"]
}

// ---------------------------------------------------------------------------------------------
// 结构化了志的最小订阅器（与 `sync::pairing` 的用例同形状）
// ---------------------------------------------------------------------------------------------

/// 进程级的「总是感兴趣」订阅器：让 tracing 的 callsite interest 缓存停在 `always`。
///
/// 理由见 `sync::pairing` 的同名实现——interest 缓存是进程级的，没有全局订阅器兜底时，并行跑测试会
/// 偶发「捕不到日志」的假失败。
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

/// 装一次进程级订阅器（每进程一次；并发调用时后到的忽略错误即可）。
fn install_interest_gate() {
    static GATE: std::sync::Once = std::sync::Once::new();
    GATE.call_once(|| {
        let _ = tracing::subscriber::set_global_default(InterestGate);
    });
}

/// 捕获结构化日志的最小订阅器（字段按书写顺序收集）。
struct Capturing {
    level: LevelFilter,
    lines: Arc<std::sync::Mutex<String>>,
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
        let mut fields = BTreeMap::new();
        let mut visitor = FieldCollector {
            fields: &mut fields,
        };
        event.record(&mut visitor);
        let mut lines = self
            .lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        lines.push_str(event.metadata().level().as_str());
        lines.push(' ');
        for (name, value) in &fields {
            lines.push(' ');
            lines.push_str(name);
            lines.push('=');
            lines.push_str(value);
        }
        lines.push('\n');
    }
}

/// 字段收集器（`record_str`/`record_debug` 都收成文本，便于按子串断言）。
struct FieldCollector<'a> {
    fields: &'a mut BTreeMap<String, String>,
}

impl Visit for FieldCollector<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_owned(), format!("{value:?}"));
    }
}

/// 在捕获订阅器的作用域内跑一段代码，交回收到的日志文本。
fn capture(level: Level, body: impl FnOnce()) -> String {
    install_interest_gate();
    let lines = Arc::new(std::sync::Mutex::new(String::new()));
    let subscriber = Capturing {
        level: level.into(),
        lines: Arc::clone(&lines),
    };
    tracing::subscriber::with_default(subscriber, body);
    Arc::try_unwrap(lines)
        .map(|lines| lines.into_inner().unwrap_or_default())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// 1.2 视图投影：形状正确按类型投影；形状缺失事件仍下发且诊断可见
// ---------------------------------------------------------------------------------------------

/// 形状满足登记类型的最低字段 ⇒ `Registered`，且不产生任何诊断。
#[test]
fn a_complete_view_projects_to_its_registered_type() {
    let record = turn_started(1);
    let projection = project(&record);
    assert!(
        matches!(projection, ViewShape::Complete),
        "字段齐全的 turn.started 必须投影成功，实际 {projection:?}"
    );
    assert!(
        projection.fault().is_none(),
        "形状正确时不得产生诊断（否则调用方会为正常事件刷告警）"
    );
}

/// 形状缺失（缺 `state`）⇒ `Malformed`，且**事件仍然下发**（任务 1.2 的核心判据）。
///
/// 判定失败不得让事件消失：客户端会因此看到序号空洞并误判丢包。诊断的可见性由下一条用例覆盖。
#[test]
fn a_malformed_view_is_still_delivered_verbatim() {
    let record = turn_started_without_state(2);
    let (handle, mut receiver) = connection();

    let (outcome, frames) = deliver_once(&handle, &record, &with_raw_payload(), &mut receiver);

    assert_eq!(
        outcome,
        EventDelivery::Queued,
        "形状缺失的事件必须照常入队（不得丢弃、不得造序号空洞）"
    );
    assert_eq!(frames.len(), 1, "对端必须真的收到这一帧");
    assert_eq!(frames[0].kind, "event");
    assert_eq!(
        payload_of(&frames[0].body)["view"]["turnId"],
        "7f3e9a10-5c2b-4d8e-9f01-3a6b5c7d8e9f",
        "view 必须是持久化原文（不得补字段冒充完整视图）"
    );
    assert_eq!(
        project(&record).fault().map(|(event_type, _)| event_type),
        Some("turn.started"),
        "诊断必须带上出错的 event_type"
    );
}

/// 形状缺失必须留下 `warn` 级结构化诊断（事件名 + event_type 都在）。
#[test]
fn a_malformed_view_logs_the_event_type_at_warn() {
    let record = turn_started_without_state(3);
    let (handle, mut receiver) = connection();

    let text = capture(Level::WARN, move || {
        let _ = deliver_once(&handle, &record, &with_raw_payload(), &mut receiver);
    });

    assert!(
        text.contains("sync.event_view_shape_rejected"),
        "形状缺失必须留下 warn 级结构化诊断，实际：{text:?}"
    );
    assert!(
        text.contains("event_type=turn.started"),
        "诊断必须含 event_type，实际：{text:?}"
    );
}

/// 形状正确时**不得**产生形状类诊断（否则正常事件会刷告警）。
#[test]
fn a_complete_view_logs_no_shape_diagnostic() {
    let record = turn_started(4);
    let (handle, mut receiver) = connection();

    let text = capture(Level::WARN, move || {
        let _ = deliver_once(&handle, &record, &with_raw_payload(), &mut receiver);
    });

    assert!(
        !text.contains("sync.event_view_shape_rejected"),
        "形状正确时不得报形状拒绝，实际：{text:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 1.3 ACP 原文逐字节保真
// ---------------------------------------------------------------------------------------------

/// 对抗素材（超 64 位整数字面量 + 未识别字段 + 制表符/引号/反斜杠）经下发后**逐字节可比**。
///
/// 任何「先解析成 `serde_json::Value` 再序列化」的实现都会改写空白、键顺序或数字文本，在这里被抓住。
#[test]
fn raw_json_survives_an_adversarial_document() {
    let record = record(
        10,
        "turn.started",
        CoreEventPayload::new(
            // view 也用对抗素材：空白不对称、键序与规范序相反（但字段齐全 ⇒ 判定为 `Complete`）。
            // 断言打在**帧文本**上而不是解析后的值上——解析值挡不住键序/空白被改写。
            ViewJson::new(ADVERSARIAL_VIEW).expect("view"),
            Some(
                acp_core::model::AcpRaw::available(
                    "application/json",
                    ADVERSARIAL_RAW,
                    digest_of(ADVERSARIAL_RAW),
                )
                .expect("可用原文"),
            ),
        ),
    );
    let (handle, mut receiver) = connection();

    let (outcome, frames) = deliver_once(&handle, &record, &with_raw_payload(), &mut receiver);
    assert_eq!(outcome, EventDelivery::Queued);
    let acp = &payload_of(&frames[0].body)["acp"];
    assert_eq!(
        acp["rawJson"].as_str(),
        Some(ADVERSARIAL_RAW),
        "rawJson 必须与持久化原文逐字节相同（不得重新序列化）"
    );
    assert_eq!(
        acp["byteLength"].as_str(),
        Some(ADVERSARIAL_RAW.len().to_string().as_str()),
        "byteLength 必须是原始 UTF-8 字节数"
    );
    // `Frame.text` 是**未解析**的帧文本：view 的原文必须逐字节出现在里面。
    // 把 `payload.view` 改成「先解析再序列化」会让键序与空白被改写，这条断言因此变红
    // （解析后的值比对做不到这一点——两者在解析后相等）。
    assert!(
        frames[0].text.contains(ADVERSARIAL_VIEW),
        "payload.view 必须逐字节出现在帧文本里（不得重新序列化）；实际帧文本：{}",
        frames[0].text
    );
}

// ---------------------------------------------------------------------------------------------
// 1.4 不可用标记的三种原因、内嵌上限与能力门控
// ---------------------------------------------------------------------------------------------

/// 超过内嵌上限的可用原文必须**显式降级为 `size_limit`**，而不是截断后当完整内容下发。
#[test]
fn an_over_limit_document_is_degraded_to_size_limit_not_truncated() {
    let oversized = format!(r#"{{"pad":"{}"}}"#, "x".repeat(300 * 1024));
    assert!(oversized.len() > 256 * 1024, "素材必须真的超过上限");
    let record = record(
        11,
        "turn.started",
        CoreEventPayload::new(
            ViewJson::new(r#"{"turnId":"7f3e9a10-5c2b-4d8e-9f01-3a6b5c7d8e9f","state":"running"}"#)
                .expect("view"),
            Some(
                acp_core::model::AcpRaw::available(
                    "application/json",
                    &oversized,
                    digest_of(&oversized),
                )
                .expect("可用原文"),
            ),
        ),
    );
    let (handle, mut receiver) = connection();

    let (_, frames) = deliver_once(&handle, &record, &with_raw_payload(), &mut receiver);
    let acp = &payload_of(&frames[0].body)["acp"];
    assert_eq!(
        acp["rawUnavailable"]["reason"].as_str(),
        Some("size_limit"),
        "超限必须降级为 size_limit（不得截断后当完整内容下发）"
    );
    assert!(
        acp.get("rawJson").is_none(),
        "降级后不得同时携带被截断的 rawJson"
    );
}

/// 非 `application/json` 的 media type 必须降级（wire 的 `mediaType` 是固定值，无法诚实表达）。
#[test]
fn a_non_json_media_type_is_degraded_to_storage_failure() {
    // `AcpRaw::available` 只校验「是良构 JSON 文档」，media type 任意。
    let record = record(
        12,
        "turn.started",
        CoreEventPayload::new(
            ViewJson::new(r#"{"turnId":"7f3e9a10-5c2b-4d8e-9f01-3a6b5c7d8e9f","state":"running"}"#)
                .expect("view"),
            Some(
                acp_core::model::AcpRaw::available(
                    "text/plain",
                    ADVERSARIAL_RAW,
                    digest_of(ADVERSARIAL_RAW),
                )
                .expect("可用原文"),
            ),
        ),
    );
    let (handle, mut receiver) = connection();

    let (_, frames) = deliver_once(&handle, &record, &with_raw_payload(), &mut receiver);
    assert_eq!(
        payload_of(&frames[0].body)["acp"]["rawUnavailable"]["reason"].as_str(),
        Some("storage_failure"),
        "非 JSON media type 无法在 wire 上诚实表达，必须降级"
    );
}

/// 持久化的三种不可用原因必须原样到达客户端（一个都不许被改写成别的）。
#[test]
fn every_persisted_unavailability_reason_reaches_the_client_unchanged() {
    for (reason, expected) in [
        (
            acp_core::model::RawUnavailableReason::SizeLimit,
            "size_limit",
        ),
        (
            acp_core::model::RawUnavailableReason::RetentionExpired,
            "retention_expired",
        ),
        (
            acp_core::model::RawUnavailableReason::StorageFailure,
            "storage_failure",
        ),
    ] {
        let record = record(
            13,
            "turn.started",
            CoreEventPayload::new(
                ViewJson::new(
                    r#"{"turnId":"7f3e9a10-5c2b-4d8e-9f01-3a6b5c7d8e9f","state":"running"}"#,
                )
                .expect("view"),
                Some(acp_core::model::AcpRaw::Unavailable {
                    reason,
                    byte_length: 42,
                    sha256: None,
                }),
            ),
        );
        let (handle, mut receiver) = connection();
        let (_, frames) = deliver_once(&handle, &record, &with_raw_payload(), &mut receiver);
        assert_eq!(
            payload_of(&frames[0].body)["acp"]["rawUnavailable"]["reason"].as_str(),
            Some(expected),
            "{reason:?} 必须原样下发"
        );
    }
}

/// 未协商 `acp.raw-payload.v1` 时容器**整个缺席**（不是 `null`）—— schema 的
/// `additionalProperties: false` 不接受一个 `"acp": null`。
///
/// 规格场景还要求「其余字段与协商时**一致**」：本用例对同一条记录分别用协商/未协商两次投递，
/// 逐字段比对两份 body，因此「未协商分支顺手改坏了别的字段」也会被抓住。
#[test]
fn an_unnegotiated_raw_payload_feature_omits_the_container_entirely() {
    let record = record(
        14,
        "turn.started",
        CoreEventPayload::new(
            ViewJson::new(r#"{"turnId":"7f3e9a10-5c2b-4d8e-9f01-3a6b5c7d8e9f","state":"running"}"#)
                .expect("view"),
            Some(
                acp_core::model::AcpRaw::available(
                    "application/json",
                    ADVERSARIAL_RAW,
                    digest_of(ADVERSARIAL_RAW),
                )
                .expect("可用原文"),
            ),
        ),
    );

    let (negotiated_handle, mut negotiated_receiver) = connection();
    let (_, negotiated_frames) = deliver_once(
        &negotiated_handle,
        &record,
        &with_raw_payload(),
        &mut negotiated_receiver,
    );
    let (plain_handle, mut plain_receiver) = connection();
    let (_, plain_frames) = deliver_once(
        &plain_handle,
        &record,
        &without_raw_payload(),
        &mut plain_receiver,
    );

    let negotiated = payload_of(&negotiated_frames[0].body);
    let plain = payload_of(&plain_frames[0].body);
    assert!(
        plain.get("acp").is_none(),
        "未协商时 acp 键必须整个缺席（不得是 null），实际 {plain}"
    );
    assert!(
        negotiated.get("acp").is_some(),
        "协商时 acp 键必须在场（否则本用例证明不了门控）"
    );
    assert_eq!(
        negotiated.get("view"),
        plain.get("view"),
        "无论是否协商原文，view 必须完全一致"
    );
    // 除 `payload.acp` 外，整条 body 必须逐字段相同。
    let mut negotiated_body = negotiated_frames[0].body.clone();
    let plain_body = plain_frames[0].body.clone();
    negotiated_body["payload"]
        .as_object_mut()
        .expect("payload 是 object")
        .remove("acp");
    assert_eq!(
        negotiated_body, plain_body,
        "未协商原文只能影响 payload.acp 这一个键，其余字段必须与协商时逐字段相同"
    );
}

// ---------------------------------------------------------------------------------------------
// 1.5 未登记内容显式降级
// ---------------------------------------------------------------------------------------------

/// 未登记的 `eventType`（开放集合里的合法取值）必须**降级下发**：事件在、原文在、不阻断后续。
#[test]
fn an_unregistered_event_type_is_degraded_not_dropped() {
    let unknown = record(
        20,
        "future.event.type",
        CoreEventPayload::new(ViewJson::new(r#"{"anything":1}"#).expect("view"), None),
    );
    assert!(
        matches!(project(&unknown), ViewShape::Unregistered { .. }),
        "未登记的 eventType 必须是 Unregistered（与形状缺失分开）"
    );

    let (handle, mut receiver) = connection();
    let (outcome, frames) = deliver_once(&handle, &unknown, &with_raw_payload(), &mut receiver);
    assert_eq!(outcome, EventDelivery::Queued, "未登记的事件必须下发");
    assert_eq!(frames[0].kind, "event");
    assert_eq!(
        payload_of(&frames[0].body)["view"]["anything"],
        1,
        "未登记类型的 view 按原文保留（不得丢弃、不得降级为普通文本）"
    );
}

/// 一条降级事件**不得阻断同会话的后续事件**（序号连续、都有帧）。
#[test]
fn a_degraded_event_does_not_block_later_events_of_the_same_session() {
    let (handle, mut receiver) = connection();

    let (first, first_frames) = deliver_once(
        &handle,
        &record(
            21,
            "future.event.type",
            CoreEventPayload::new(ViewJson::new(r#"{"anything":1}"#).expect("view"), None),
        ),
        &with_raw_payload(),
        &mut receiver,
    );
    let (second, second_frames) = deliver_once(
        &handle,
        &turn_started(22),
        &with_raw_payload(),
        &mut receiver,
    );

    assert_eq!(first, EventDelivery::Queued);
    assert_eq!(second, EventDelivery::Queued, "降级事件不得阻断后续");
    assert_eq!(first_frames.len(), 1);
    assert_eq!(second_frames.len(), 1, "后续事件必须照样有帧");
    assert_eq!(
        second_frames[0].body["globalSequence"].as_str(),
        Some("22"),
        "后续事件保持自己的序号（不得造空洞）"
    );
}

// ---------------------------------------------------------------------------------------------
// 1.6 事件轨道与既有投递队列的衔接
// ---------------------------------------------------------------------------------------------

/// 事件经**既有**那条有界出站账本入队；填满后投递层被告知停读（`Refused(HighWater)`）。
///
/// 判据是「复用同一条队列」与「高水位信号真的回给调用方」：本模块不另建第二条队列。
#[test]
fn a_full_ledger_reports_high_water_to_the_caller() {
    let record = turn_started(30);
    let (handle, mut receiver) = connection();

    // 只投递、不排空：直到账本拒绝。上限是 sync 侧的常量（`MAX_PENDING_QUEUE_MESSAGES`）。
    let mut refused = None;
    for _ in 0..(crate::sync::conn::limits::MAX_PENDING_QUEUE_MESSAGES + 16) {
        match deliver(&handle, &record, &with_raw_payload()) {
            EventDelivery::Queued => {}
            other => {
                refused = Some(other);
                break;
            }
        }
    }

    let refused = refused.expect("账本填满后必须出现拒绝（否则高水位信号没传到投递层）");
    assert_eq!(
        refused,
        EventDelivery::Refused(crate::sync::conn::SendFault::HighWater),
        "高水位必须以 Refused(HighWater) 回给调用方（§14 两级背压的第一级）"
    );

    // 排空后必须能继续入队：拒绝是背压，不是永久关闭。**必须经 `consumed`**——直接排空 mpsc 不会
    // 递减账本（真实路径里递减发生在会话写出之后），那样断言会误判「拒绝是永久的」。
    let mut drained = 0usize;
    while let Ok(outbound) = receiver.try_recv() {
        handle.consumed(&outbound);
        drained += 1;
    }
    assert!(drained > 0, "队列里必须真的有已入队的帧");
    assert_eq!(
        deliver(&handle, &record, &with_raw_payload()),
        EventDelivery::Queued,
        "排空之后必须恢复入队（高水位是背压，不是永久关闭）"
    );
}

/// `RAW_PAYLOAD_FEATURE` 必须真的是协商词表里的一项。
///
/// 否则门控会**静默永不命中**：`with_raw_payload()` 用同一个常量拼协商集合，两边同源，写错也全绿。
/// 词表的机器定义在 `compatibility/features/v1/features.json`，`handshake` 的 `SUPPORTED_FEATURES`
/// 已由它自己的用例逐条比对；这里把本模块的常量绑到那张表上。
#[test]
fn the_raw_payload_feature_id_is_in_the_negotiated_vocabulary() {
    assert!(
        crate::sync::conn::handshake::SUPPORTED_FEATURES.contains(&RAW_PAYLOAD_FEATURE),
        "{RAW_PAYLOAD_FEATURE} 不在 SUPPORTED_FEATURES 里 —— 门控会永不命中"
    );
}

// ---------------------------------------------------------------------------------------------
// `core.file-diff.v1` 的投递侧门控（`file.changed` 的差异行）
// ---------------------------------------------------------------------------------------------

/// 已协商差异行能力的连接（协商集合只比 [`without_file_diff`] 多 `core.file-diff.v1`）。
fn with_file_diff() -> NegotiatedFeatures {
    NegotiatedFeatures::new(vec![
        EVENT_ORIGIN_FEATURE.to_owned(),
        RAW_PAYLOAD_FEATURE.to_owned(),
        FILE_DIFF_FEATURE.to_owned(),
    ])
}

/// 未协商差异行能力的连接（其余两条与本模块的既有素材一致）。
fn without_file_diff() -> NegotiatedFeatures {
    NegotiatedFeatures::new(vec![
        EVENT_ORIGIN_FEATURE.to_owned(),
        RAW_PAYLOAD_FEATURE.to_owned(),
    ])
}

/// core 的 `file_changed_view` 逐字符拼出的紧凑视图：`changeId`/`kind`/`displayPath`/`summary`/统计 +
/// **尾部的** `diffLines`（差异行是最后一个成员，因此去键就是去掉这一段）。
const FILE_CHANGED_WITH_DIFF: &str = r#"{"changeId":"3f0c1a44-2b7e-4d51-9c8a-1e2f3a4b5c6d","kind":"modified","displayPath":"src/lib.rs","summary":"1 处改动","addedLines":"1","deletedLines":"1","diffLines":[{"kind":"deleted","text":"a"},{"kind":"added","text":"X"}]}"#;

/// 同一视图**去掉差异行**后的字节（协商与未协商两条路径的其余字段必须逐字节相同）。
const FILE_CHANGED_WITH_DIFF_STRIPPED: &str = r#"{"changeId":"3f0c1a44-2b7e-4d51-9c8a-1e2f3a4b5c6d","kind":"modified","displayPath":"src/lib.rs","summary":"1 处改动","addedLines":"1","deletedLines":"1"}"#;

/// 差异行因超限未下发：视图带 `diffUnavailable`（与 `diffLines` 互斥）——它受**同一条**门控。
const FILE_CHANGED_OVER_LIMIT: &str = r#"{"changeId":"3f0c1a44-2b7e-4d51-9c8a-1e2f3a4b5c6d","kind":"modified","displayPath":"src/lib.rs","summary":"900 处改动","addedLines":"900","deletedLines":"900","diffUnavailable":{"reason":"size_limit"}}"#;

/// 同一视图**去掉不可用标记**后的字节。
const FILE_CHANGED_OVER_LIMIT_STRIPPED: &str = r#"{"changeId":"3f0c1a44-2b7e-4d51-9c8a-1e2f3a4b5c6d","kind":"modified","displayPath":"src/lib.rs","summary":"900 处改动","addedLines":"900","deletedLines":"900"}"#;

/// 同一条差异行视图，但 `diffLines` 在**对象第一个成员位**（core 今天把这两个键写在末位，这里刻意反过来）。
///
/// 视图的登记形状（`event-views.schema.json#/$defs/file.changed`）不约束成员顺序，将来多一个
/// `file.changed` 视图生产者也完全可能给出别的顺序，因此去键必须对两种形状都成立——见用例
/// [`a_leading_file_diff_key_is_stripped_like_a_trailing_one`]。
const FILE_CHANGED_DIFF_FIRST: &str = r#"{"diffLines":[{"kind":"deleted","text":"a"},{"kind":"added","text":"X"}],"changeId":"3f0c1a44-2b7e-4d51-9c8a-1e2f3a4b5c6d","kind":"modified","displayPath":"src/lib.rs","summary":"1 处改动","addedLines":"1","deletedLines":"1"}"#;

/// 同一条 `diffUnavailable` 视图，同样把被删键放在第一个成员位。
const FILE_CHANGED_OVER_LIMIT_FIRST: &str = r#"{"diffUnavailable":{"reason":"size_limit"},"changeId":"3f0c1a44-2b7e-4d51-9c8a-1e2f3a4b5c6d","kind":"modified","displayPath":"src/lib.rs","summary":"900 处改动","addedLines":"900","deletedLines":"900"}"#;

/// 一条 `file.changed` 事件（正文就是给定的视图文本）。
fn file_changed(sequence: u64, view: &str) -> SyncEventRecord {
    record(
        sequence,
        "file.changed",
        CoreEventPayload::new(ViewJson::new(view).expect("view"), None),
    )
}

/// 协商了 `core.file-diff.v1` 的连接照常收到差异行（这一条锚住另一侧：门控不是「永不携带」）。
#[test]
fn a_negotiated_file_diff_feature_carries_the_diff_lines() {
    let record = file_changed(21, FILE_CHANGED_WITH_DIFF);
    let (handle, mut receiver) = connection();

    let (outcome, frames) = deliver_once(&handle, &record, &with_file_diff(), &mut receiver);

    assert_eq!(outcome, EventDelivery::Queued);
    assert!(
        frames[0].text.contains(FILE_CHANGED_WITH_DIFF),
        "协商后视图必须逐字节携带差异行：{}",
        frames[0].text
    );
}

/// 未协商 `core.file-diff.v1`：差异行键**缺席**（不是 `null`、不是空数组），其余字段逐字节一致。
///
/// 两条判据都在**帧文本**上做：`diffLines`/`diffUnavailable` 一次都不许出现，而去掉该键的那段字节必须
/// 逐字出现在帧里（因此「顺手改坏了别的字段」也会被抓住）。
#[test]
fn an_unnegotiated_file_diff_feature_omits_the_keys_and_keeps_the_rest_verbatim() {
    for (index, (view, stripped, key)) in [
        (
            FILE_CHANGED_WITH_DIFF,
            FILE_CHANGED_WITH_DIFF_STRIPPED,
            "diffLines",
        ),
        (
            FILE_CHANGED_OVER_LIMIT,
            FILE_CHANGED_OVER_LIMIT_STRIPPED,
            "diffUnavailable",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let record = file_changed(30 + index as u64, view);
        let (handle, mut receiver) = connection();

        let (outcome, frames) = deliver_once(&handle, &record, &without_file_diff(), &mut receiver);

        assert_eq!(outcome, EventDelivery::Queued, "事件仍必须下发");
        let text = &frames[0].text;
        assert!(!text.contains(key), "未协商时 {key} 必须缺席：{text}");
        assert!(
            !text.contains(&format!("\"{key}\":null")) && !text.contains(&format!("\"{key}\":[]")),
            "缺席不是 null、也不是空数组：{text}"
        );
        assert!(
            text.contains(stripped),
            "去键后其余字段必须逐字节保留（期望片段 {stripped}）：{text}"
        );
        // 去键后的视图仍是登记形状（`event-views.schema.json#/$defs/file.changed` 里两个键都可选），
        // 因此客户端按自己的封闭形状校验不会因为「多键」或「缺键」拒收整条事件。
        let view = RawObject::parse(&payload_of(&frames[0].body)["view"].to_string())
            .expect("view 是对象");
        assert!(
            views::project("file.changed", &view).is_ok(),
            "去键后的视图必须仍满足登记的最低字段：{text}"
        );
    }
}

/// 被删键位于**第一个成员位**时必须照样去键（上一条用例只覆盖「被删键在末位」的形状）。
///
/// 这两种形状会把「用一个标记同时表达『输出里是否已有成员』与『语法上此处是否该出现成员』」的实现
/// 分开：合并写法下删掉首位成员不改变那个标记，下一次循环读到的**合法**分隔逗号会被当成「开头就是
/// 逗号」，整段文本判失败 → [`crate::sync::event::gate_file_diff`] 返回 `None` → 调用方保留原文 →
/// 未协商 `core.file-diff.v1` 的连接照常收到差异行（fail-open）。因此本用例断言两条：这两个键缺席，
/// 且其余成员逐字节保留（去键后的整段视图就是既有末位夹具去键后的那段字节）。
#[test]
fn a_leading_file_diff_key_is_stripped_like_a_trailing_one() {
    for (index, (view, stripped, key)) in [
        (
            FILE_CHANGED_DIFF_FIRST,
            FILE_CHANGED_WITH_DIFF_STRIPPED,
            "diffLines",
        ),
        (
            FILE_CHANGED_OVER_LIMIT_FIRST,
            FILE_CHANGED_OVER_LIMIT_STRIPPED,
            "diffUnavailable",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let record = file_changed(50 + index as u64, view);
        let (handle, mut receiver) = connection();

        let (outcome, frames) = deliver_once(&handle, &record, &without_file_diff(), &mut receiver);

        assert_eq!(outcome, EventDelivery::Queued, "事件仍必须下发");
        let text = &frames[0].text;
        assert!(
            !text.contains(key),
            "首位被删键 {key} 必须缺席（合并标记的写法会在这里变红）：{text}"
        );
        assert!(
            text.contains(stripped),
            "去键后其余成员必须逐字节保留且顺序不变（期望片段 {stripped}）：{text}"
        );
        // 去键后的视图仍是登记形状：客户端按自己的封闭形状校验不会因为「缺键」拒收整条事件。
        let view = RawObject::parse(&payload_of(&frames[0].body)["view"].to_string())
            .expect("view 是对象");
        assert!(
            views::project("file.changed", &view).is_ok(),
            "去键后的视图必须仍满足登记的最低字段：{text}"
        );
    }
}

/// `FILE_DIFF_FEATURE` 必须真的是协商词表里的一项（否则门控会静默永不命中）。
#[test]
fn the_file_diff_feature_id_is_in_the_negotiated_vocabulary() {
    assert!(
        crate::sync::conn::handshake::SUPPORTED_FEATURES.contains(&FILE_DIFF_FEATURE),
        "{FILE_DIFF_FEATURE} 不在 SUPPORTED_FEATURES 里 —— 门控会永不命中"
    );
}

/// 门控只作用于 `file.changed`：别的 eventType 上同名键（生产者错写、或将来的别的事件视图）不动。
#[test]
fn the_diff_gate_leaves_other_event_types_untouched() {
    let view =
        r#"{"turnId":"7f3e9a10-5c2b-4d8e-9f01-3a6b5c7d8e9f","state":"running","diffLines":[]}"#;
    let record = record(
        40,
        "turn.started",
        CoreEventPayload::new(ViewJson::new(view).expect("view"), None),
    );
    let (handle, mut receiver) = connection();

    let (outcome, frames) = deliver_once(&handle, &record, &without_file_diff(), &mut receiver);

    assert_eq!(outcome, EventDelivery::Queued);
    assert!(
        frames[0].text.contains(view),
        "只有 file.changed 受该门控：{}",
        frames[0].text
    );
}
