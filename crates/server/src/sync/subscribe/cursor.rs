//! `sync.subscribe.cursor` 的判定与 `sync.reset_required` 的下发判据
//! （`docs/SYNC_PROTOCOL.md` §9.2/§9.4；变更 `sync-server-connection` 的任务 4.1/4.2）。
//!
//! 本模块是纯函数层：输入是**已解码的形状**（`SubmittedCursor`）与当前事件库水位（`GlobalCursor`），
//! 输出是对端可见的后果（回 `sync.cursor_invalid` 用的原因、是否要主动下发 `sync.reset_required`，或
//! "这是协议错误"这一 schema 级结论）。不读时钟、不碰端口、不写日志——所有判定都能用一条断言盯住。
//!
//! 四条硬规则：
//!
//! - **四种拒绝原因可区分**（§9.2 的 `details.reason` 是封闭词表
//!   `malformed|epoch_mismatch|beyond_head|cursor_expired`）：格式非法（解码期）→ `malformed`；
//!   `serverEpoch` 与当前不匹配 → `epoch_mismatch`；序号**可表示**且大于当前 head → `beyond_head`；
//!   序号早于保留窗口下界 → `cursor_expired`。四者 MUST NOT 混用同一取值，也 MUST NOT 用别的错误码代替。
//! - **超出 v1 序号域不是 cursor 判定**：`globalSequence` 超过 `2^63-1`（`u64` 也无法承载的取值同属
//!   此类）是协议错误 `protocol.schema_invalid`（§3.3），不是 `sync.cursor_invalid`——§9.2 的那张词表
//!   表达不了"取值根本无法表示"。它是 [`CursorDecision::SequenceOutOfRange`]，由调用方回 schema 错误码
//!   （与 `conn::wire` 对信封 `connectionSequence` 先判上界再比期望值同一口径）。
//! - **不得把越界 cursor 钳制到 head**（§9.2）：`beyond_head` / `cursor_expired` / `epoch_mismatch` /
//!   [`CursorDecision::SequenceOutOfRange`] 一律不回 `Resume`，因此调用方拿不到可用于投递的事件游标
//!   ——钳制在类型上就不可表达。
//! - **`cursor_expired` 的唯一来源是存储层的保留窗口判定**（`ReplayBatch.reset_required`）：窗口下界
//!   （最早仍可重放的行）只在存储层可见，本模块不猜它，只把存储层的结论翻译成协议词表
//!   （[`decide_from_reset`]）。序号域校验、`epoch_mismatch` 与 `beyond_head` 则优先在本地用同一份
//!   `head()` 判定，**先于**存储层调用发生——这样它们绝不进入存储的"从窗口下界开始读"分支（那正是
//!   钳制）。

use acp_core::model::{GlobalCursor, Sequence, ServerEpoch};
use acp_core::ports::ResetReason as CoreResetReason;
use sync_protocol::common::Cursor;
use sync_protocol::sync::ResetReason as WireResetReason;

/// v1 的序号上界（§3.3：`connectionSequence` 不得超过 `2^63-1`；cursor 的 `globalSequence` 同域）。
/// 超出它的取值**不是** cursor 判定能表达的错误：它是协议级 schema 错误（`protocol.schema_invalid`），
/// 接收方不得截断、回绕或钳制到边界。
pub const MAX_SEQUENCE: u64 = i64::MAX as u64;

/// 四种可区分的 cursor 拒绝原因（§9.2 的 `sync.cursor_invalid.details.reason`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorFault {
    /// 格式非法（不是合法 `Cursor` 形状：缺键、类型不对、非规范十进制串等）。
    Malformed,
    /// `serverEpoch` 与当前事件库不匹配（§9.1：序号只在同一 epoch 内有意义）。
    EpochMismatch,
    /// 序号大于当前 head（§9.2）。
    BeyondHead,
    /// 序号早于保留窗口下界（对应的事件已被保留策略清理）。
    CursorExpired,
}

impl CursorFault {
    /// 四种取值的完整集合（顺序即 §9.2 的词表顺序）。
    pub const ALL: [CursorFault; 4] = [
        CursorFault::Malformed,
        CursorFault::EpochMismatch,
        CursorFault::BeyondHead,
        CursorFault::CursorExpired,
    ];

    /// `details.reason` 的取值文本（`compatibility/errors/v1/errors.json` 的
    /// `sync.cursor_invalid.details.reason` 词表）。
    pub fn as_str(self) -> &'static str {
        match self {
            CursorFault::Malformed => "malformed",
            CursorFault::EpochMismatch => "epoch_mismatch",
            CursorFault::BeyondHead => "beyond_head",
            CursorFault::CursorExpired => "cursor_expired",
        }
    }
}

/// `sync.subscribe.body.cursor` 的形状分类（**解码期**的产物，见 `SubscribeBody` 的判定）。
///
/// `cursor` 在 schema 里是 required 且可 `null`，因此"键缺席"不是 [`SubmittedCursor`] 的取值：它是
/// `protocol.schema_invalid`，由调用方在解码失败路径上定案；本类型只表达"键在场、值不是
/// `null` 且合法"（[`SubmittedCursor::Value`]）、"值为 `null`"（[`SubmittedCursor::Null`]）与
/// "值为 `null` 之外的非法形状"（[`SubmittedCursor::Malformed`]）三态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmittedCursor {
    /// `cursor` 在场但形状非法（`malformed` 的唯一来源）。
    Malformed,
    /// `cursor: null`：首次同步（§9.2）。
    Null,
    /// 形状合法的游标。
    Value(Cursor),
}

impl SubmittedCursor {
    /// 从 `Nullable<Cursor>` 的解码结果构造。
    ///
    /// `Ok(None)` 是 `null`、`Ok(Some)` 是合法值、`Err` 是形状非法——三态一一对应，不给"缺键"留位置。
    pub fn from_decoded(decoded: Result<Option<Cursor>, ()>) -> Self {
        match decoded {
            Ok(None) => SubmittedCursor::Null,
            Ok(Some(cursor)) => SubmittedCursor::Value(cursor),
            Err(()) => SubmittedCursor::Malformed,
        }
    }
}

/// cursor 判定的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorDecision {
    /// 首次同步（`cursor: null`）：进快照流程，并主动下发 `sync.reset_required{initial_sync}`。
    FirstSync,
    /// 游标有效：调用方从该游标之后续传。
    Resume(GlobalCursor),
    /// 序号超出 v1 序号域（§3.3：`2^63-1`；`u64` 溢出同属此类）：**协议错误**
    /// （`protocol.schema_invalid`），不是 cursor 判定。调用方据此回 schema 错误码，且不建立任何
    /// 可投递状态（也不是 `beyond_head`——那个取值只表达"可表示但比当前 head 更靠前"）。
    SequenceOutOfRange,
    /// 该游标不可用：回 `sync.cursor_invalid`，`details.reason` 取 `fault`；需要客户端重建快照时
    /// 由服务端主动下发 `reset`（缺席 = 不要求重建，例如 `malformed`/`beyond_head`）。
    Rejected {
        fault: CursorFault,
        reset: Option<WireResetReason>,
    },
}

/// 按当前水位判定一条**已提交**的 cursor（§9.2）。
///
/// 判定顺序照抄 `storage-sqlite` 的 `resolve_cursor` 的层次，只是把域校验提到最前：形状（解码期已定案）
/// → 序号域（§3.3 的上界）→ epoch → 与 head 比较，最后留给存储层判保留窗口。本地先判这些不只是省一次
/// 查询——它保证被拒的游标**不会**落进存储层"从窗口下界开始读"的那条分支（那正是被禁止的钳制）。
pub fn decide(submitted: &SubmittedCursor, head: &GlobalCursor) -> CursorDecision {
    let cursor = match submitted {
        SubmittedCursor::Malformed => {
            return CursorDecision::Rejected {
                fault: CursorFault::Malformed,
                reset: None,
            };
        }
        SubmittedCursor::Null => return CursorDecision::FirstSync,
        SubmittedCursor::Value(cursor) => cursor,
    };
    // §3.3 的序号域校验先于任何语义比较（`conn::wire` 对信封 `connectionSequence` 也是先判上界再比
    // 期望值）：超出 v1 上界（含 `u64` 溢出）是协议错误，不能退化成 `sync.cursor_invalid` 的任一取值。
    let Some(sequence) = parse_sequence(cursor.global_sequence.as_str()) else {
        return CursorDecision::SequenceOutOfRange;
    };
    // §9.1：序号只在同一 epoch 内有意义，跨 epoch 的序号不可比（比"谁更大"没有意义）。
    if cursor.server_epoch.as_str() != head.server_epoch.as_str() {
        return CursorDecision::Rejected {
            fault: CursorFault::EpochMismatch,
            reset: Some(WireResetReason::EpochMismatch),
        };
    }
    // 到这里序号是**可表示**的：比 head 更靠前就是 §9.2 的 `beyond_head`（不是 schema 错误）。
    if sequence.get() > head.global_sequence.get() {
        return CursorDecision::Rejected {
            fault: CursorFault::BeyondHead,
            reset: None,
        };
    }
    CursorDecision::Resume(GlobalCursor::new(
        ServerEpoch::new(head.server_epoch.as_str()).expect("head 的 epoch 来自存储层，形状已校验"),
        sequence,
    ))
}

/// 把存储层的重置结论翻译成协议词表（§9.4 的四种 `reason` 与 §9.2 的四种拒绝原因各取所需）。
///
/// `cache_incompatible` 返回 `None`：它不是 cursor 判定（§9.4 的第四种重建触发来自客户端在
/// `sync.snapshot_request.reason` 里的自报），因此不该被翻译成"某条 cursor 不可用"。其余三种：
///
/// - `initial_sync` → 首次同步（`cursor: null` 的存储侧同义结论），不发错误、只下发 reset；
/// - `epoch_mismatch` → 既回错误（`details.reason = epoch_mismatch`）也下发 reset；
/// - `cursor_expired` → 既回错误（`cursor_expired`）也下发 reset（唯一能判出"早于保留窗口"的地方）。
pub fn decide_from_reset(reason: CoreResetReason) -> Option<CursorDecision> {
    match reason {
        CoreResetReason::InitialSync => Some(CursorDecision::FirstSync),
        CoreResetReason::EpochMismatch => Some(CursorDecision::Rejected {
            fault: CursorFault::EpochMismatch,
            reset: Some(WireResetReason::EpochMismatch),
        }),
        CoreResetReason::CursorExpired => Some(CursorDecision::Rejected {
            fault: CursorFault::CursorExpired,
            reset: Some(WireResetReason::CursorExpired),
        }),
        CoreResetReason::CacheIncompatible => None,
    }
}

/// 判定结论要主动下发的 `sync.reset_required.reason`（`None` = 不下发）。
pub fn reset_for(decision: &CursorDecision) -> Option<WireResetReason> {
    match decision {
        CursorDecision::FirstSync => Some(WireResetReason::InitialSync),
        CursorDecision::Rejected { reset, .. } => *reset,
        // `SequenceOutOfRange` 是 schema 错误而不是"这条 cursor 不可用"：不涉及重置语义。
        CursorDecision::SequenceOutOfRange | CursorDecision::Resume(_) => None,
    }
}

/// 判定结论要回的 `sync.cursor_invalid.details.reason`（`None` = 不回 `sync.cursor_invalid`）。
///
/// `None` 有两种含义，调用方必须区分：结论是 `Resume`/`FirstSync`（不回错误），或是
/// `SequenceOutOfRange`（回的是 `protocol.schema_invalid`，不是 `sync.cursor_invalid`）。
pub fn fault_for(decision: &CursorDecision) -> Option<CursorFault> {
    match decision {
        CursorDecision::Rejected { fault, .. } => Some(*fault),
        CursorDecision::FirstSync
        | CursorDecision::Resume(_)
        | CursorDecision::SequenceOutOfRange => None,
    }
}

/// `cursor.globalSequence` 是否超出 v1 序号域（§3.3：`2^63-1`；`u64` 溢出同属此类）。
///
/// 形状合法的 `Cursor` 里该字段是规范十进制串（`DecimalString` 已挡掉前导零、非数字与空串），因此本判定
/// 为真只可能是"取值大得无法用 v1 的序号域承载"——那是协议错误 `protocol.schema_invalid`，不是 §9.2 的
/// 四种 `sync.cursor_invalid` 取值中的任何一种。[`decide`]（`sync.subscribe`）与 `sync.ack` 共用本判定，
/// 两条路径因此不会给出两种结论。
pub fn sequence_out_of_range(cursor: &Cursor) -> bool {
    parse_sequence(cursor.global_sequence.as_str()).is_none()
}

/// wire 游标 → core 游标（仅用于把**已判定**的游标交给调用方）。
///
/// 调用方 MUST 先用 [`decide`]（`sync.subscribe`）或 [`sequence_out_of_range`] + epoch 比较（`sync.ack`）
/// 判定过对端输入：序号超出 v1 域是协议错误，不归本函数的 `None` 表达。因此 epoch 与序号在这里都已经
/// 校验过，失败是**接线错误**而不是对端输入问题——返回 `None` 让调用方按内部不可用失败关闭，不把它写成
/// `expect`。
pub fn to_core_cursor(cursor: &Cursor) -> Option<GlobalCursor> {
    Some(GlobalCursor::new(
        ServerEpoch::new(cursor.server_epoch.as_str()).ok()?,
        parse_sequence(cursor.global_sequence.as_str())?,
    ))
}

/// 规范十进制串 → v1 序号：超出 v1 上界（含 `u64` 溢出的取值）返回 `None`。
///
/// `None` 只可能是"无法用 v1 序号域承载"：形状合法的 `Cursor` 里 `globalSequence` 已由 `DecimalString`
/// 挡掉非规范十进制串，不存在"格式非法"这一支。因此调用方 MUST 把 `None` 当协议错误
/// （`protocol.schema_invalid`，§3.3）处置，而不是 cursor 判定（[`sequence_out_of_range`] 与 [`decide`]
/// 就是这么用的）。
fn parse_sequence(text: &str) -> Option<Sequence> {
    let value = text.parse::<u64>().ok()?;
    if value > MAX_SEQUENCE {
        return None;
    }
    Sequence::new(value).ok()
}
