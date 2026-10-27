//! 普通 prompt 的内容编码与回显关联。
//!
//! 内容块按客户端给出的顺序逐字节组成 JSON 数组，摘要与持久正文使用同一份字节。这里不读时间、
//! 不访问存储，也不依赖 wire DTO。

use sha2::{Digest as _, Sha256};

use super::error::InvalidValue;
use super::ids::{AgentRef, AgentSessionId, MessageId, RequestId, SessionId, TurnId};
use super::json::JsonValueText;
use super::scalars::{Digest, ResourceGeneration, Timestamp};
use super::session::PromptContentBlock;
use super::{MemberValue, top_level_member};

/// 一条 prompt 的内容块上限。
pub const PROMPT_MAX_BLOCKS: usize = 64;

/// 普通 prompt 的完整有序内容：块原文、保真 JSON 数组与同源摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptInput {
    content: Vec<PromptContentBlock>,
    json: JsonValueText,
    digest: Digest,
}

impl PromptInput {
    /// 构造输入；空数组或超过 [`PROMPT_MAX_BLOCKS`] 时拒绝。
    pub fn new(content: Vec<PromptContentBlock>) -> Result<Self, InvalidValue> {
        if content.is_empty() {
            return Err(InvalidValue::Empty);
        }
        if content.len() > PROMPT_MAX_BLOCKS {
            return Err(InvalidValue::TooLong {
                max: PROMPT_MAX_BLOCKS,
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

    /// 有序内容块原文。
    pub fn content(&self) -> &[PromptContentBlock] {
        &self.content
    }

    /// 保真 JSON 数组文本。
    pub fn json(&self) -> &JsonValueText {
        &self.json
    }

    /// 与 [`Self::json`] 同源的 SHA-256 摘要。
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
}

fn digest_of(bytes: &[u8]) -> Result<Digest, InvalidValue> {
    let hash = Sha256::digest(bytes);
    let encoded = base64url_32(hash.as_slice());
    Digest::new(&encoded).map_err(|_| InvalidValue::Digest)
}

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

/// 普通 prompt 派发时的内存上下文；它不是持久凭据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchContext {
    pub session: SessionId,
    pub user_message: MessageId,
    pub request: RequestId,
    pub turn: TurnId,
    pub agent: AgentRef,
    pub agent_session: AgentSessionId,
    pub resource_generation: ResourceGeneration,
    pub input_digest: Digest,
}

/// Agent 回显携带的可证明关联。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEchoLink {
    pub session: SessionId,
    pub user_message: MessageId,
    pub request: RequestId,
    pub turn: TurnId,
    pub agent: AgentRef,
    pub agent_session: AgentSessionId,
    pub resource_generation: ResourceGeneration,
}

/// 一条待关联的用户输入回显。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEchoEvent {
    pub link: Option<UserEchoLink>,
    pub agent_message: Option<MessageId>,
    pub content: JsonValueText,
    pub content_digest: Digest,
}

/// 回显关联结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EchoAssociation {
    Duplicate {
        user_message: MessageId,
    },
    Differs {
        user_message: MessageId,
        agent_message: Option<MessageId>,
    },
    Unattributed,
}

/// 仅当全部身份字段相等且正文摘要相等时复用已保存的用户消息；否则保留独立回显事件。
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

/// 普通输入进入实时事件流时的来源标记。
pub const PROMPT_USER_MESSAGE_SOURCE: &str = "owned_prompt";

/// 普通输入可见性事件中的文本上限。
pub const PROMPT_MESSAGE_TEXT_MAX_CHARS: usize = 262_144;

/// 普通输入的受理写集。
#[derive(Debug, Clone, PartialEq)]
pub struct UserInputWrite {
    pub message: MessageId,
    pub session: SessionId,
    pub turn: TurnId,
    pub request: RequestId,
    pub content: PromptInput,
    pub at: Timestamp,
}

/// 普通输入受理成功后的持久事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedInput {
    pub session: SessionId,
    pub message: MessageId,
    pub request: RequestId,
    pub turn: TurnId,
}

/// 将 text 块投影为有界字符串；完整正文仍以 `owned_message.content_json` 为准。
pub fn prompt_message_text(input: &PromptInput) -> String {
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
mod tests {
    use super::*;
    use crate::model::{AgentId, AgentRef, PromptContentBlock, ResourceGeneration, Timestamp};

    fn agent() -> AgentRef {
        AgentRef::try_new(AgentId::new("codex").expect("agent"), "Codex").expect("agent ref")
    }

    fn digest(seed: &str) -> Digest {
        PromptInput::new(vec![
            PromptContentBlock::from_json_text(
                format!(r#"{{"type":"text","text":{seed}}}"#).as_str(),
            )
            .expect("block"),
        ])
        .expect("input")
        .digest()
        .clone()
    }

    #[test]
    fn prompt_input_preserves_order_values_and_digest() {
        let first = PromptContentBlock::from_json_text(
            r#"{"type":"text","text":"hello","extension":{"n":9007199254740993}}"#,
        )
        .expect("block");
        let second =
            PromptContentBlock::from_json_text(r#"{"type":"resource_link","uri":"file:///x"}"#)
                .expect("block");
        let input = PromptInput::new(vec![first, second]).expect("input");
        assert_eq!(
            input.json().as_str(),
            r#"[{"type":"text","text":"hello","extension":{"n":9007199254740993}},{"type":"resource_link","uri":"file:///x"}]"#
        );
        assert_eq!(prompt_message_text(&input), "hello");
    }

    #[test]
    fn prompt_input_rejects_empty_and_too_many_blocks() {
        assert_eq!(PromptInput::new(Vec::new()), Err(InvalidValue::Empty));
        let block =
            PromptContentBlock::from_json_text(r#"{"type":"text","text":"x"}"#).expect("block");
        assert!(matches!(
            PromptInput::new(vec![block; PROMPT_MAX_BLOCKS + 1]),
            Err(InvalidValue::TooLong {
                max: PROMPT_MAX_BLOCKS
            })
        ));
    }

    #[test]
    fn echo_association_is_table_driven() {
        let input = PromptInput::new(vec![
            PromptContentBlock::from_json_text(r#"{"type":"text","text":"hello"}"#).expect("block"),
        ])
        .expect("input");
        let context = DispatchContext {
            session: SessionId::new("00000000-0000-4000-8000-000000000001").expect("session"),
            user_message: MessageId::new("00000000-0000-4000-8000-000000000002").expect("message"),
            request: RequestId::new("00000000-0000-4000-8000-000000000003").expect("request"),
            turn: TurnId::new("00000000-0000-4000-8000-000000000004").expect("turn"),
            agent: agent(),
            agent_session: AgentSessionId::new("00000000-0000-4000-8000-000000000005")
                .expect("agent session"),
            resource_generation: ResourceGeneration::new(7),
            input_digest: input.digest().clone(),
        };
        let link = UserEchoLink {
            session: context.session.clone(),
            user_message: context.user_message.clone(),
            request: context.request.clone(),
            turn: context.turn.clone(),
            agent: context.agent.clone(),
            agent_session: context.agent_session.clone(),
            resource_generation: context.resource_generation,
        };
        let event = UserEchoEvent {
            link: Some(link),
            agent_message: None,
            content: JsonValueText::new(r#"{"type":"text","text":"hello"}"#).expect("view"),
            content_digest: input.digest().clone(),
        };
        assert_eq!(
            associate_user_echo(Some(&context), &event),
            EchoAssociation::Duplicate {
                user_message: context.user_message.clone()
            }
        );
        assert_eq!(
            associate_user_echo(None, &event),
            EchoAssociation::Unattributed
        );
        let changed = UserEchoEvent {
            content_digest: digest(r#""different""#),
            ..event
        };
        assert!(matches!(
            associate_user_echo(Some(&context), &changed),
            EchoAssociation::Differs { .. }
        ));
    }

    #[test]
    fn timeout_clock_is_caller_supplied() {
        let at = Timestamp::new("2026-01-01T00:00:00.000Z").expect("timestamp");
        assert_eq!(at.as_str(), "2026-01-01T00:00:00.000Z");
    }
}
