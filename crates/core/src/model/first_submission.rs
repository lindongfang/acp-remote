//! 首条提交、派发围栏与回显关联的纯类型（`docs/CORE_PORTS_AND_STORAGE.md` §3.9/§5.11）。
//!
//! 三个硬约束（`design.md` D7/D8）：
//!
//! - **受理 ≠ 派发**：本模块只表达「已保存但可证明未派发」「可能已派发」「已有终态」三种持久事实；
//!   阶段只能前进，`ever_dispatched` 一旦为真不再回落。
//! - **首条正文先存**：完整有序输入由 [`FirstSubmissionInput`] 原样保留（块内扩展字段与精确值不重写），
//!   摘要在构造期用同一份字节算出；首条用户消息因此不需要 Agent 回显即可读取。
//! - **回显只按派发上下文关联**：相同文本或 Agent 的 `messageId` **不是**匹配依据；没有可证明上下文
//!   的用户输入事件保留为独立来源（[`EchoAssociation::Unattributed`]），不得吞掉或覆盖首条原文。
//!
//! 本模块不读时间、不访问存储、不依赖 wire DTO。

use sha2::{Digest as _, Sha256};

use super::error::InvalidValue;
use super::identity::Actor;
use super::ids::{
    AgentRef, AgentSessionId, ConfigOptionId, DirectoryId, FirstSubmissionId, MessageId, ModeId,
    ModeRef, OriginEpoch, RequestId, RuntimeEpoch, SessionId, SubmissionId, TurnId,
};
use super::json::{JsonValueText, ViewJson};
use super::scalars::{Digest, ResourceGeneration, Timestamp};
use super::session::{ConfigValue, PromptContentBlock, PublicError};
use super::{MemberValue, top_level_member};

/// 首条输入的内容块上限（与 `session.prompt` 相同：1..=64）。
pub const FIRST_SUBMISSION_MAX_BLOCKS: usize = 64;

/// 持久首条提交的阶段（`owned_first_submission.stage` 的 CHECK 逐值一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FirstSubmissionStage {
    /// 正式记录存在且**可证明未派发**；客户端可以显式继续。
    SavedNotDispatched,
    /// 派发围栏已提交：可能已派发，崩溃后保守 `uncertain`，不再自动重发。
    DispatchStarted,
    /// 首个命令已有终态（`completed`/`failed`/`uncertain`）。
    Terminal,
}

impl FirstSubmissionStage {
    /// 全部取值（存储层与测试穷举）。
    pub const ALL: [Self; 3] = [
        Self::SavedNotDispatched,
        Self::DispatchStarted,
        Self::Terminal,
    ];

    /// 稳定 token（与 `owned_first_submission.stage` 的 CHECK 一致）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SavedNotDispatched => "saved_not_dispatched",
            Self::DispatchStarted => "dispatch_started",
            Self::Terminal => "terminal",
        }
    }
}

impl std::fmt::Display for FirstSubmissionStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for FirstSubmissionStage {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "saved_not_dispatched" => Ok(Self::SavedNotDispatched),
            "dispatch_started" => Ok(Self::DispatchStarted),
            "terminal" => Ok(Self::Terminal),
            _ => Err(InvalidValue::Field),
        }
    }
}

/// 一次尝试的持久结果（`owned_first_submission.terminal_outcome`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestOutcome {
    Completed,
    Failed,
    Uncertain,
}

impl RequestOutcome {
    /// 全部取值。
    pub const ALL: [Self; 3] = [Self::Completed, Self::Failed, Self::Uncertain];

    /// 稳定 token。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Uncertain => "uncertain",
        }
    }
}

impl std::fmt::Display for RequestOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for RequestOutcome {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "uncertain" => Ok(Self::Uncertain),
            _ => Err(InvalidValue::Field),
        }
    }
}

/// 完整有序首条输入：内容块原文、保真的 JSON 数组文本与内容摘要。
///
/// 构造期是唯一一次把块串成数组的地方，摘要对**同一份字节**计算，因此调用方无法把摘要求和正文指认成
/// 两个不同的内容。块内未知字段、精确数值与大整数都逐字节保留（`PromptContentBlock` 是不透明载体）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstSubmissionInput {
    content: Vec<PromptContentBlock>,
    json: JsonValueText,
    digest: Digest,
}

impl FirstSubmissionInput {
    /// 由有序内容块构造。空或超过 [`FIRST_SUBMISSION_MAX_BLOCKS`] 项即拒绝。
    pub fn new(content: Vec<PromptContentBlock>) -> Result<Self, InvalidValue> {
        if content.is_empty() {
            return Err(InvalidValue::Empty);
        }
        if content.len() > FIRST_SUBMISSION_MAX_BLOCKS {
            return Err(InvalidValue::TooLong {
                max: FIRST_SUBMISSION_MAX_BLOCKS,
            });
        }
        let mut text = String::from("[");
        for (index, block) in content.iter().enumerate() {
            if index > 0 {
                text.push(',');
            }
            text.push_str(block.as_str());
        }
        text.push(']');
        let json = JsonValueText::new(&text)?;
        let digest = digest_of(text.as_bytes())?;
        Ok(Self {
            content,
            json,
            digest,
        })
    }

    /// 有序内容块（逐项原文）。
    pub fn content(&self) -> &[PromptContentBlock] {
        &self.content
    }

    /// 保真数组文本（`[<block>,...]`）。
    pub fn json(&self) -> &JsonValueText {
        &self.json
    }

    /// 内容摘要（SHA-256 的规范 base64url）。
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
}

fn digest_of(bytes: &[u8]) -> Result<Digest, InvalidValue> {
    let hash = Sha256::digest(bytes);
    let encoded = base64url_32(hash.as_slice());
    Digest::new(&encoded).map_err(|_| InvalidValue::Digest)
}

/// 32 字节的规范无填充 base64url（43 字符，末字符低 2 位为 0）。
///
/// 与 `scalars.rs` 的**校验**同源；这里只做编码，不建立第二套摘要口径。
fn base64url_32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut text = String::with_capacity(43);
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let chunk = ((bytes[index] as u32) << 16)
            | ((bytes[index + 1] as u32) << 8)
            | bytes[index + 2] as u32;
        text.push(ALPHABET[((chunk >> 18) & 0x3f) as usize] as char);
        text.push(ALPHABET[((chunk >> 12) & 0x3f) as usize] as char);
        text.push(ALPHABET[((chunk >> 6) & 0x3f) as usize] as char);
        text.push(ALPHABET[(chunk & 0x3f) as usize] as char);
        index += 3;
    }
    if index + 1 == bytes.len() {
        let chunk = (bytes[index] as u32) << 16;
        text.push(ALPHABET[((chunk >> 18) & 0x3f) as usize] as char);
        text.push(ALPHABET[((chunk >> 12) & 0x3f) as usize] as char);
    } else if index + 2 == bytes.len() {
        let chunk = ((bytes[index] as u32) << 16) | ((bytes[index + 1] as u32) << 8);
        text.push(ALPHABET[((chunk >> 18) & 0x3f) as usize] as char);
        text.push(ALPHABET[((chunk >> 12) & 0x3f) as usize] as char);
        text.push(ALPHABET[((chunk >> 6) & 0x3f) as usize] as char);
    }
    text
}

/// 一次已确认的配置选择（`owned_first_submission.config_json` 的条目）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstSubmissionConfig {
    pub id: ConfigOptionId,
    pub value: ConfigValue,
}

/// 首次受理时要认领的准备附件清单（已封存）。
///
/// `submission` 是附件清单标识（客户端随 `attachment.manifest_begin` 给出）；`runtime_epoch`/
/// `generation` 用来证明该清单属于当前运行期的同一准备资源。认领**不**再次上传、复制或全量重哈希。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionManifestClaim {
    pub submission: SubmissionId,
    pub runtime_epoch: RuntimeEpoch,
    pub generation: ResourceGeneration,
}

/// 首次提交的原子写集（`docs/CORE_PORTS_AND_STORAGE.md` §3.9）。
///
/// 存储层必须在**同一事务**里写正式会话行、完整首条用户消息、命令受理、turn、首条关联与附件引用；
/// 任一步失败全回滚，调用方不得派发。
#[derive(Debug, Clone, PartialEq)]
pub struct FirstSubmissionWrite {
    pub actor: Actor,
    /// 由 Owner 分配、可经准备凭据证明的会话标识（不是客户端任意 UUID）。
    pub session: SessionId,
    pub first_submission_id: FirstSubmissionId,
    /// 持久用户消息身份（`owned_message.message_id`）；回显只关联到它，不新增第二份首条。
    pub user_message_id: MessageId,
    pub origin_epoch: OriginEpoch,
    pub agent: AgentRef,
    /// 创建时解析出的规范化绝对路径（只写列，不进可投影形状）。
    pub workspace_cwd: Option<String>,
    pub workspace_directory: Option<DirectoryId>,
    pub mode: Option<ModeRef>,
    pub config: Vec<FirstSubmissionConfig>,
    pub input: FirstSubmissionInput,
    pub manifest: Option<SubmissionManifestClaim>,
    pub request_id: RequestId,
    pub request_fingerprint: Digest,
    pub turn: TurnId,
    pub at: Timestamp,
}

/// 持久首条关联的只读投影（`owned_first_submission` 的行语义）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstSubmissionRecord {
    pub session: SessionId,
    pub first_submission_id: FirstSubmissionId,
    pub user_message_id: MessageId,
    pub first_request_id: RequestId,
    pub current_request_id: RequestId,
    pub stage: FirstSubmissionStage,
    pub ever_dispatched: bool,
    pub agent_session: Option<AgentSessionId>,
    pub resource_generation: Option<ResourceGeneration>,
    /// 受理时记录的模式选择（`owned_first_submission.mode_id`）；`None` = 未选择。
    pub mode_id: Option<ModeId>,
    /// 受理时记录的配置选择原文（`owned_first_submission.config_json` 的 object；`{}` = 未选择）。
    ///
    /// 与 `continue` 载荷的 `configOptions` 同形，因此缺省时可直接沿用；存储层**不**把文本值反推成
    /// `Text`/`Select`（那取决于适配器已确认的选项类别）。
    pub config: ViewJson,
    pub input_digest: Digest,
    /// 当前尝试（或首个命令）的持久终态；`None` = 尚无终态。
    pub terminal: Option<RequestTerminal>,
}

/// 一次尝试的终态投影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestTerminal {
    pub request_id: RequestId,
    pub outcome: RequestOutcome,
    pub at: Timestamp,
    pub error: Option<PublicError>,
}

/// 首次受理结果：受理只产生正式记录与 `SavedNotDispatched`，**不**派发。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirstSubmissionAdmission {
    Accepted(FirstSubmissionRecord),
    /// 同 ID 已被受理：返回既有记录，不重建准备、不追加首条、不重传附件。
    AlreadyCommitted(FirstSubmissionRecord),
}

/// 派发围栏：在真正调用 Agent **之前**提交。
///
/// `retry_of` 只在显式继续时出现，且必须指向本会话一个持久终态为 `failed` 的 `session.start` 尝试；
/// `request_id` 是该次尝试的新稳定请求标识。`agent_session`/`resource_generation` 与阶段在同一次
/// 条件更新里固定，之后不得再换 Agent 身份补发。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchFence {
    pub session: SessionId,
    pub first_submission_id: FirstSubmissionId,
    pub actor: Actor,
    pub request_id: RequestId,
    pub retry_of: Option<RequestId>,
    pub request_fingerprint: Digest,
    pub agent_session: AgentSessionId,
    pub resource_generation: ResourceGeneration,
    pub at: Timestamp,
}

/// 派发围栏的判定。`NotEligible` 由适配层映射为 `command.idempotency_conflict`（已派发/已终态）或
/// `command.uncertain`（可能已派发）；本层不区分——调用方先读持久终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchAdmission {
    Granted,
    NotEligible,
}

/// 一次尝试的持久结算。`Failed` 在 `saved_not_dispatched` 阶段也可写（端点建立/配置复核失败）；
/// `Completed`/`Uncertain` 只允许在 `dispatch_started` 之后。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstSubmissionSettlement {
    pub session: SessionId,
    pub actor: Actor,
    pub request_id: RequestId,
    pub request_fingerprint: Digest,
    pub outcome: RequestOutcome,
    pub error: Option<PublicError>,
    pub at: Timestamp,
}

/// 启动收敛的结论：只收敛到可证明的事实，**绝不**自动派发。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirstSubmissionConvergence {
    /// 没有首条关联（旧会话/普通会话）：什么都不做。
    NotPreparation,
    /// 可证明未派发：本次尝试已收敛为失败，正文与身份保留，等待显式继续。
    ProvablyNotDispatched(FirstSubmissionRecord),
    /// 已跨越派发边界且没有终态：保守 `uncertain`，不自动重发。
    Uncertain(FirstSubmissionRecord),
    /// 已有终态：原样返回（幂等）。
    Terminal(FirstSubmissionRecord),
}

/// 持久首条用户消息的读取投影（`owned_message.channel = 'user'` 的唯一读取入口）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstUserMessage {
    pub session: SessionId,
    pub message: MessageId,
    pub turn: Option<TurnId>,
    pub content: JsonValueText,
    pub created_at: Timestamp,
}

/// 一次 prompt 派发的内部上下文（`design.md` D7；`conversation-reliability` D2）。
///
/// 只在内存里随该次调用的事件 sink 传入 mapper；它**不**是持久凭据，也不含 ACP DTO。资源代把旧代的
/// 迟到事件挡在关联之外。
///
/// 首条与普通后续输入共用这一种上下文：`first_submission` 让首条保持原关联（`None` = 普通输入），
/// `turn` 让普通输入也能在「固定字段全部相等」的判据里被证明。`input_digest` 是**本次受理的完整
/// 内容**摘要（不是事件文本摘要），只有它与回显摘要逐字节相等才算 [`EchoAssociation::Duplicate`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchContext {
    pub session: SessionId,
    pub first_submission: Option<FirstSubmissionId>,
    pub user_message: MessageId,
    pub request: RequestId,
    pub turn: TurnId,
    pub agent: AgentRef,
    pub agent_session: AgentSessionId,
    pub resource_generation: ResourceGeneration,
    pub input_digest: Digest,
}

/// 一条 Agent 用户输入回显携带的可证明关联（字段由适配器从当前派发上下文与事件本身取得）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEchoLink {
    pub session: SessionId,
    pub first_submission: Option<FirstSubmissionId>,
    pub user_message: MessageId,
    pub request: RequestId,
    pub turn: TurnId,
    pub agent: AgentRef,
    pub agent_session: AgentSessionId,
    pub resource_generation: ResourceGeneration,
}

/// 一条待关联的用户输入事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEchoEvent {
    /// 无任何可证明关联时为 `None`（例如重启后、旧资源代或没有派发上下文的事件）。
    pub link: Option<UserEchoLink>,
    /// Agent 自己给这条回显的 messageId；**不**作为匹配依据，只用于展示去重。
    pub agent_message: Option<MessageId>,
    pub content: JsonValueText,
    pub content_digest: Digest,
}

/// 回显关联的三态结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EchoAssociation {
    /// 可证明属于该尝试且内容与已保存首条一致：只补充到同一 `user_message`，不新增消息。
    Duplicate { user_message: MessageId },
    /// 可证明属于该尝试但内容不同：显示为 Agent 回显，**不覆盖**已保存原文。
    Differs {
        user_message: MessageId,
        agent_message: Option<MessageId>,
    },
    /// 无法证明关联：保留为独立 Agent 输入事件（来源可辨），不吞掉、不改写首条。
    Unattributed,
}

/// `design.md` D7 的唯一回显关联规则（`conversation-reliability` D2 扩展到普通后续输入）。
///
/// - 没有上下文、事件没有关联、或任一固定字段（会话/首条/用户消息/请求/turn/Agent 身份/资源代）不相等
///   → [`EchoAssociation::Unattributed`]；**不以内容或 Agent `messageId` 兜底**。
/// - 全部相等：内容摘要与本次受理的完整内容一致 → `Duplicate`；否则 `Differs`（同一条用户消息的 Agent
///   回显）。
/// - 第二条**有意**发送的相同文字属于新的 `request`/`turn`/`user_message`，因此不会被这里吞并。
///
/// 八个身份字段而不是七个：普通输入的 `first_submission` 为 `None`，只靠其余字段无法区分同一会话里
/// 相邻两次派发的迟到事件，`turn` 因此是判据的一部分。
pub fn associate_user_echo(
    context: Option<&DispatchContext>,
    event: &UserEchoEvent,
) -> EchoAssociation {
    let Some(context) = context else {
        return EchoAssociation::Unattributed;
    };
    let Some(link) = &event.link else {
        return EchoAssociation::Unattributed;
    };
    if link.session != context.session
        || link.first_submission != context.first_submission
        || link.user_message != context.user_message
        || link.request != context.request
        || link.turn != context.turn
        || link.agent != context.agent
        || link.agent_session != context.agent_session
        || link.resource_generation != context.resource_generation
    {
        return EchoAssociation::Unattributed;
    }
    if event.content_digest == context.input_digest {
        EchoAssociation::Duplicate {
            user_message: context.user_message.clone(),
        }
    } else {
        EchoAssociation::Differs {
            user_message: context.user_message.clone(),
            agent_message: event.agent_message.clone(),
        }
    }
}

/// 首条消息投影进入事件流时的来源标记：首条来自已保存的用户输入，而不是 Agent 回显。
pub const FIRST_USER_MESSAGE_SOURCE: &str = "owned_first_submission";

/// 普通（后续）输入投影进入事件流时的来源标记（§6 第 24 条）：受理事务写的那条
/// `user.message.delta` 只做实时可见性，正文权威仍是 `owned_message.content_json`。
pub const PROMPT_USER_MESSAGE_SOURCE: &str = "owned_prompt";

/// 普通输入可见性事件里 `text` 的字符上限（与首条同口径：§10.3 单条 message 文本上限）。
pub const PROMPT_MESSAGE_TEXT_MAX_CHARS: usize = 262_144;

/// 普通（后续）输入的受理写集：并入 [`crate::ports::OwnedCommit`] 的新成员（§3.11/§5.2）。
///
/// `None` = 本提交不是普通输入受理（首条、delta 合批、终态、交互等既有提交）。
#[derive(Debug, Clone, PartialEq)]
pub struct UserInputWrite {
    pub message: MessageId,
    pub session: SessionId,
    pub turn: TurnId,
    pub request: RequestId,
    /// 与首条**同一内容编码**（有序内容块 + 保真 JSON 数组 + 同源 SHA-256 摘要）。
    pub content: FirstSubmissionInput,
    pub at: Timestamp,
}

/// 一次普通输入受理的持久事实（并入 [`crate::ports::CommitOutcome`]）。
///
/// 只在受理事务提交成功后为 `Some`；幂等回放原样返回首次值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedInput {
    pub session: SessionId,
    pub message: MessageId,
    pub request: RequestId,
    pub turn: TurnId,
}

/// 受理输入进入事件流时的 `text` 投影：只拼接 `text` 块，超长按
/// [`PROMPT_MESSAGE_TEXT_MAX_CHARS`] 截断（正文权威仍是 `owned_message.content_json`）。
///
/// 内容块是首条同款不透明 JSON 载体，因此这里只读顶层 `type`/`text` 两个成员，不重建块、不改写原文。
pub fn prompt_message_text(input: &FirstSubmissionInput) -> String {
    let mut text = String::new();
    let mut remaining = PROMPT_MESSAGE_TEXT_MAX_CHARS;
    for block in input.content() {
        if remaining == 0 {
            break;
        }
        let member = top_level_member(block.as_str(), "type");
        let is_text = matches!(&member, Ok(MemberValue::Text(value)) if value == "text");
        if !is_text {
            continue;
        }
        let Ok(MemberValue::Text(chunk)) = top_level_member(block.as_str(), "text") else {
            continue;
        };
        let taken: String = chunk.chars().take(remaining).collect();
        remaining -= taken.chars().count();
        text.push_str(&taken);
    }
    text
}

#[cfg(test)]
#[path = "first_submission_tests.rs"]
mod tests;
