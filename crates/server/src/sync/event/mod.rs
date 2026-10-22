//! Sync 事件腿：`payload.view` 的形状判定、`payload.acp` 的原文容器与把投影后的事件交给
//! 前置变更那条**唯一**的有界出站队列（`docs/SYNC_PROTOCOL.md` §10.1/§10.3；变更
//! `sync-server-commands` 的任务 1.1–1.6）。
//!
//! ## 与前置变更的分工
//!
//! 事件帧的**信封字段**（`origin` 五个产生来源与身份配对、`globalSequence`/`sessionSequence`、
//! `causationRequestId`、`createdAt`）与两条投递路径（增量重放、实时扇出）都在
//! [`crate::sync::subscribe`]，并且那里已有一条把 core 原文逐字节落进 `view`/`rawJson` 的装配。
//! 本模块**不复制第二份**那套映射（`AGENTS.md` 的「复用既有模式，不建立第二套约定」），它只做四件
//! 前置变更没做的事：
//!
//! 1. [`project`]：把持久化 `payload.view` 交给 `sync-protocol::views::project` 做一次**形状判定**，
//!    并把判定失败折成显式枚举（[`Projection`]），让调用方能各记一条结构化诊断；
//! 2. `acp.raw-payload.v1` 的**协商门控**：未协商该 feature 的连接 MUST NOT 收到原文容器
//!    （§5.2/§10.3），容器整个缺席而不是填空；
//! 3. `core.file-diff.v1` 的**协商门控**：`file.changed` 视图里的 `diffLines`/`diffUnavailable` 由 core
//!    无条件派生，未协商该 feature 的连接 MUST NOT 收到这两个键（键缺席而不是置空）——见 [`frame`]；
//! 4. [`deliver`]：事件轨道的投递入口——判定 → 装配 → 经 `ConnectionHandle::send` 进**既有**那条
//!    有界出站队列（§14 的双上限与两级背压），高水位时把「停读重放批次」交回调用方。
//!
//! ## 三条硬规则
//!
//! - **形状判定失败不丢事件**：`views::project` 失败意味着「这个 eventType 的 view 缺了协议要求的
//!   最低字段」——那是生产者或本机的 bug，但把它变成「事件消失」会让客户端看到序号空洞并误判丢包。
//!   因此判定失败时记一条含 `event_type` 与缺失字段的结构化日志，然后**仍按原样下发该 view**
//!   （客户端自带降级卡片）。绝不丢弃事件、绝不造序号空洞、绝不在本层补字段冒充完整视图。
//! - **原文逐字节保真**：`rawJson` 装的是 core 落库的那份文档文本，只经一次 JSON 字符串字面量编码
//!   （`subscribe::event` 的既有实现）。绝不先解析成 `serde_json::Value` 再序列化——那会改写空白、
//!   键顺序与超出 64 位的整数字面量。对抗用例 [`tests::raw_json_survives_an_adversarial_document`]
//!   钉住这条路径。
//! - **未登记内容显式降级**：未登记的 `eventType`（开放集合里的合法取值）与未登记的 ACP 判别子都
//!   按下发 + 保留原文处置，MUST NOT 静默丢弃、MUST NOT 把结构化事件降级为普通文本、MUST NOT
//!   阻断同会话后续事件。
//!
//! ## 依赖纪律（`AGENTS.md` §5、`docs/MODULE_ARCHITECTURE.md` §4.9）
//!
//! 只调用 `core` 的公开面（`acp_core::model` 的值对象与 [`acp_core::ports::SyncEventRecord`]）、
//! `sync-protocol` 的公开面（`event`/`views`/`common`）与本 crate 内 `sync::conn` 的连接面。
//! **不 import `storage-sqlite`、不调用 `server::node_link`**——平级入站适配器互不调用。
//!
//! 这条纪律目前靠**约定与检视**守：`npm run check:boundaries` 只比对 crate 级依赖边与 core 的
//! 普通依赖闭包，而 `crate::node_link` 是同一 crate 内的模块、不产生任何边，因此它**不覆盖**
//! 本模块这一条（`docs/MODULE_ARCHITECTURE.md` §5 的矩阵是 crate 粒度的）。

use acp_core::ports::SyncEventRecord;
use sync_protocol::common::{LocalActivity, RawObject, ValueError};
use sync_protocol::event::Body;
use sync_protocol::views;
use tracing::{debug, warn};

use crate::sync::conn::{ConnectionHandle, SendFault};
use crate::sync::local_lifecycle::LOCAL_LIFECYCLE_FEATURE;
use crate::sync::subscribe::NegotiatedFeatures;
use crate::sync::subscribe::event::{EVENT_ORIGIN_FEATURE, EventFrameFault, event_body};
use acp_core::use_cases::LOCAL_ACTIVITY_EVENT;

/// `acp.raw-payload.v1`：客户端声明可处理事件里的 ACP 原文容器（§5.2 的 feature 表）。
///
/// 本模块是它唯一的**投递侧**门控：未协商时 [`deliver`] 不装配原文容器（§10.3「未协商该能力时
/// SHALL NOT 携带原文容器」）。
pub const RAW_PAYLOAD_FEATURE: &str = "acp.raw-payload.v1";

/// `core.file-diff.v1`：客户端声明可处理 `file.changed` 视图里的结构化差异行（§5.2 的 feature 表）。
///
/// core 的派生总是写出 `diffLines`/`diffUnavailable`（`crates/core/src/derive.rs` 的
/// `file_changed_view`：派生层没有协商结果，见那里「投递层按协商结果去掉该键」的注记），因此本模块是
/// 这条能力的**投递侧唯一门控**：未协商时 [`frame`] 把该键从视图里**去掉**（键缺席，不是 `null`、
/// 不是空数组；`event-views.schema.json` 的 `file.changed` 里两个键都可选，因此去键后的视图仍是合法
/// 形状），其余字段与协商时逐字节一致。
pub const FILE_DIFF_FEATURE: &str = "core.file-diff.v1";

/// `core.typed-media.v1`：类型化媒体与消息通道（§10.4）。未协商时事件视图里不得出现
/// `channel`/`partial`/`audio_ref`/`file_ref`/`image_ref.attachmentId`（§5.3）。
pub const TYPED_MEDIA_FEATURE: &str = "core.typed-media.v1";

// 视图门控的唯一实现在 `crate::session_read`（`session.read` 的历史条目与事件帧共用同一套降级规则）：
// 这里只做重导出，避免两条下发路径各持一份判定。
pub(crate) use crate::session_read::{gate_file_diff, gate_typed_media};

/// 一次 `payload.view` 形状判定的结论。
///
/// 三种取值都**不改变下发内容**：view 永远是持久化原文。差别只在调用方是否要记诊断 —— 因此这里
/// **不携带**投影出的类型化视图：下发走的是原文（`wire_payload` 直接编码 `payload.view`），
/// 没有任何调用方需要那个值，带着它只会让枚举尺寸被最大的视图撑大。
#[derive(Debug, Clone)]
pub enum ViewShape {
    /// `eventType` 已登记（34 类）且 view 形状满足该类型的最低必填字段。
    Complete,
    /// 登记类型的最低字段缺失或类型不符：**事件仍下发**，诊断见 [`ViewShape::fault`]。
    Malformed {
        /// 出错的 eventType。
        event_type: String,
        /// serde 的形状诊断原文（缺哪个字段、哪个字段类型不符）。
        detail: String,
    },
    /// `eventType` 是开放集合里的合法取值但未登记：显式降级，view 原样保留。
    Unregistered {
        /// 未登记的 eventType。
        event_type: String,
    },
}

impl ViewShape {
    /// 需要记诊断时的 `(event_type, detail)`；形状正确时 `None`。
    pub fn fault(&self) -> Option<(&str, &str)> {
        match self {
            Self::Complete => None,
            Self::Malformed { event_type, detail } => Some((event_type, detail)),
            Self::Unregistered { event_type } => Some((event_type, UNREGISTERED_DETAIL)),
        }
    }
}

/// 未登记 eventType 的诊断口径（`views::project` 对它一律拒绝，v1 没有通用未知视图）。
const UNREGISTERED_DETAIL: &str = "eventType 未在 v1 事件视图登记表内，按未知事件降级并保留原文";

/// 把持久化 `payload.view` 做一次形状判定（`docs/SYNC_PROTOCOL.md` §10.3 的最低字段合同）。
///
/// 输入是 core 读面给出的 [`SyncEventRecord`]（`ReadView::sync_replay` 已保证 `payload.view` 是
/// 库里原文）。`views::project` 是 `sync-protocol` 已冻结的登记表，按 `eventType` 把 view 投影成
/// 类型化视图；本函数只把它的两种失败折成显式枚举：
///
/// - [`ValueError::ViewShape`] → [`ViewShape::Malformed`]：登记类型缺字段/类型不符；
/// - [`ValueError::Enumerated`] → [`ViewShape::Unregistered`]：`eventType` 不在登记表里。
///
/// `core` 的 `ViewJson` 保证文本是良构 JSON object，因此「不是 object」在本层不可达；真出现时按
/// [`ViewShape::Malformed`] 处置（同样只记诊断、不改下发内容）。
pub fn project(record: &SyncEventRecord) -> ViewShape {
    let event_type = record.event.event_type.as_str();
    match sync_protocol::common::RawObject::parse(record.payload.view.as_str()) {
        Err(_) => ViewShape::Malformed {
            event_type: event_type.to_owned(),
            detail: "payload.view 不是良构 JSON object".to_owned(),
        },
        Ok(view) => match views::project(event_type, &view) {
            // 类型化视图本身不参与下发（wire 送原文），因此这里只取「形状通过」这个结论。
            Ok(_) => ViewShape::Complete,
            Err(error) => match error {
                ValueError::Enumerated { value, .. } => {
                    ViewShape::Unregistered { event_type: value }
                }
                ValueError::ViewShape { detail, .. } => ViewShape::Malformed {
                    event_type: event_type.to_owned(),
                    detail,
                },
                other => ViewShape::Malformed {
                    event_type: event_type.to_owned(),
                    detail: other.to_string(),
                },
            },
        },
    }
}

/// 装配一条事件的 `event` 帧 body（`payload` 之外的全部字段走前置变更那**唯一**一条装配路径）。
///
/// 三处本模块的增量：
///
/// - `origin.nodeId` 的门控仍是 `core.event-origin.v1`（与重放/实时扇出同一个判定点）；
/// - **原文容器的门控是 `acp.raw-payload.v1`**：未协商时容器整个缺席（`payload.acp` 置 `None` 后
///   序列化整键跳过，`additionalProperties: false` 的 schema 因此不会收到一个 `"acp":null`），
///   其余字段与协商时逐字节一致。已协商时沿用既有装配——同一份 core 原文、同一次字符串字面量
///   编码，因此不存在「两条路径给出两种字节」的可能；
/// - **差异行的门控是 `core.file-diff.v1`**：未协商时把 `file.changed` 视图里的 `diffLines`/
///   `diffUnavailable` 两个顶层键去掉（同一种手法：键**缺席**而不是置空）。core 的派生总是写出它们，
///   因此去键必须发生在下发前——本函数是重放与实时扇出两条腿共用的唯一装配点，两处因此不会分叉。
pub fn frame(
    record: &SyncEventRecord,
    features: &NegotiatedFeatures,
    local_activity: Option<LocalActivity>,
) -> Result<Body, EventFrameFault> {
    // `session.activity.changed` 是本 feature 的新事件类型：未协商的连接 MUST NOT 收到它（§9.8）。
    // 它不是「未知类型降级」——对未协商连接整条不投递，因此在这里显式失败关闭而不是照发。
    if record.event.event_type.as_str() == LOCAL_ACTIVITY_EVENT
        && !features.contains(LOCAL_LIFECYCLE_FEATURE)
    {
        return Err(EventFrameFault::LifecycleNotNegotiated);
    }
    let mut body = event_body(record, features.contains(EVENT_ORIGIN_FEATURE))?;
    if !features.contains(RAW_PAYLOAD_FEATURE) {
        body.payload.acp = None;
    }
    if let Some(view) = gate_file_diff(
        record.event.event_type.as_str(),
        &body.payload.view,
        features.contains(FILE_DIFF_FEATURE),
    ) {
        body.payload.view = view;
    }
    if let Some(view) = gate_typed_media(&body.payload.view, features.contains(TYPED_MEDIA_FEATURE))
    {
        body.payload.view = view;
    }
    // owned 摘要的 `localActivity`：core 的持久视图不带协商结果（它不知道连接），因此由投递层在
    // 协商后的 `session.created`/`session.updated`/`session.activity.changed` 视图上补键。键**只增不
    // 改**：core 已经写出的值原样保留，绝不出现两个来源。
    if let Some(activity) = local_activity {
        if matches!(
            record.event.event_type.as_str(),
            "session.created" | "session.updated" | LOCAL_ACTIVITY_EVENT
        ) {
            if let Some(view) = inject_local_activity(&body.payload.view, activity) {
                body.payload.view = view;
            }
        }
    }
    Ok(body)
}

/// 在 `view.session` 对象上补 `localActivity`（仅在缺席时写入）。
fn inject_local_activity(view: &RawObject, activity: LocalActivity) -> Option<RawObject> {
    let mut value: serde_json::Value = serde_json::from_str(view.get()).ok()?;
    let session = value.get_mut("session")?.as_object_mut()?;
    session
        .entry("localActivity")
        .or_insert_with(|| serde_json::Value::String(activity.as_str().to_owned()));
    RawObject::parse(&serde_json::to_string(&value).ok()?).ok()
}

/// 一条事件投递给一条连接的结论（调用方只按它决定控制流，不自己判高水位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventDelivery {
    /// 已入队：经连接面那条唯一的有界出站队列（§14 的双上限）。
    Queued,
    /// 出站账本拒绝（高水位或连接已结束）：调用方 MUST 停读新的重放批次，客户端按最后收到的
    /// 事件序号 ACK，重连后续投（§14 的两级背压第一级）。**不另建第二条队列**。
    Refused(SendFault),
    /// 组装不出合法帧（`origin.kind`/身份/序号/时间戳/view 文本中某一项损坏）：这一条不发，
    /// 诊断已记，客户端按 cursor 重放兜底。与形状判定失败**不是**同一件事——那些事件照发。
    Unassemblable(EventFrameFault),
}

/// 事件轨道的投递入口：判定 → 装配 → 进既有出站队列。
///
/// 顺序固定为「先记诊断、再装配、再入队」：`views::project` 的结论只影响**日志**，绝不影响
/// 「这一条是否下发」（§10.3 的显式降级）。
///
/// 入队走 [`ConnectionHandle::send`]——前置变更那条有界出站账本（条数/字节双上限、序号与容量
/// 同一临界区、迟滞的高低水位）。它返回 [`SendFault::HighWater`] 时调用方据此停读重放批次；
/// 这正是 §14 两级背压的第一级，**不需要也不允许**第二条队列。
pub fn deliver(
    handle: &ConnectionHandle,
    record: &SyncEventRecord,
    features: &NegotiatedFeatures,
) -> EventDelivery {
    // 形状缺失（生产者/本机 bug）值得 `warn`；未登记 eventType 是协议明确的开放集合，属正常降级，
    // 只记 `debug`。两者都**不改变**这一条是否下发。
    match project(record) {
        ViewShape::Complete => {}
        ViewShape::Malformed { event_type, detail } => warn!(
            event = "sync.event_view_shape_rejected",
            event_id = record.event.id.as_str(),
            event_type = event_type,
            global_sequence = record.event.global_sequence.get(),
            session_id = ?record.event.session.as_ref().map(|session| session.as_str()),
            detail = detail,
            "the view does not satisfy the registered minimum fields; the event is still delivered verbatim"
        ),
        ViewShape::Unregistered { event_type } => debug!(
            event = "sync.event_type_unregistered",
            event_id = record.event.id.as_str(),
            event_type = event_type,
            global_sequence = record.event.global_sequence.get(),
            detail = UNREGISTERED_DETAIL,
            "the event type is outside the v1 registry; the event is degraded, not dropped"
        ),
    }
    match frame(record, features, None) {
        Err(fault) => {
            warn!(
                event = "sync.event_unassemblable",
                event_id = record.event.id.as_str(),
                global_sequence = record.event.global_sequence.get(),
                fault = %fault,
                "the persisted event cannot be mapped to a v1 event frame"
            );
            EventDelivery::Unassemblable(fault)
        }
        Ok(body) => match handle.send(sync_protocol::envelope::MessageType::Event, &body) {
            Ok(()) => EventDelivery::Queued,
            Err(fault) => EventDelivery::Refused(fault),
        },
    }
}
