//! 首条提交/回显关联纯类型测试（`design.md` D7/D8）。

use super::*;
use crate::model::{AgentId, SelectValue, ViewJson};

fn text(value: &str) -> ConfigValue {
    ConfigValue::text(value).expect("config text")
}

fn agent() -> AgentRef {
    AgentRef::try_new(AgentId::new("codex").expect("agent id"), "Codex").expect("agent ref")
}

fn agent_session() -> AgentSessionId {
    AgentSessionId::new("acp-session").expect("agent session")
}

fn uuid(seed: u8) -> String {
    format!(
        "{seed:02x}{seed:02x}{seed:02x}{seed:02x}-1111-4111-8111-{seed:02x}{seed:02x}{seed:02x}{seed:02x}{seed:02x}{seed:02x}"
    )
}

fn context() -> DispatchContext {
    let input = FirstSubmissionInput::new(vec![
        PromptContentBlock::from_json_text(
            r#"{"type":"text","text":"hello","futureField":{"n":9007199254740993}}"#,
        )
        .expect("block"),
    ])
    .expect("input");
    DispatchContext {
        session: SessionId::new(&uuid(1)).expect("session"),
        first_submission: Some(FirstSubmissionId::new(&uuid(2)).expect("first submission")),
        user_message: MessageId::new(&uuid(3)).expect("message"),
        request: RequestId::new(&uuid(4)).expect("request"),
        turn: TurnId::new(&uuid(5)).expect("turn"),
        agent: agent(),
        agent_session: agent_session(),
        resource_generation: ResourceGeneration::new(1),
        input_digest: input.digest().clone(),
    }
}

fn link(context: &DispatchContext) -> UserEchoLink {
    UserEchoLink {
        session: context.session.clone(),
        first_submission: context.first_submission.clone(),
        user_message: context.user_message.clone(),
        request: context.request.clone(),
        turn: context.turn.clone(),
        agent: context.agent.clone(),
        agent_session: context.agent_session.clone(),
        resource_generation: context.resource_generation,
    }
}

#[test]
fn input_preserves_order_exact_values_and_digest() {
    let first = PromptContentBlock::from_json_text(
        r#"{"type":"text","text":"a","unknown":{"big":123456789012345678901234567890}}"#,
    )
    .expect("block");
    let second =
        PromptContentBlock::from_json_text(r#"{"type":"resource_link","uri":"file:///x"}"#)
            .expect("block");
    let input = FirstSubmissionInput::new(vec![first.clone(), second.clone()]).expect("input");
    assert_eq!(input.content(), &[first, second]);
    assert_eq!(
        input.json().as_str(),
        r#"[{"type":"text","text":"a","unknown":{"big":123456789012345678901234567890}},{"type":"resource_link","uri":"file:///x"}]"#
    );
    // 同一内容得到同一摘要；不同内容得到不同摘要。
    let again = FirstSubmissionInput::new(input.content().to_vec()).expect("input");
    assert_eq!(input.digest(), again.digest());
    let changed = FirstSubmissionInput::new(vec![
        PromptContentBlock::from_json_text(r#"{"type":"text","text":"b"}"#).expect("block"),
    ])
    .expect("input");
    assert_ne!(input.digest(), changed.digest());
}

#[test]
fn input_rejects_empty_and_too_many_blocks() {
    assert!(FirstSubmissionInput::new(Vec::new()).is_err());
    let block = PromptContentBlock::from_json_text(r#"{"type":"text","text":"x"}"#).expect("block");
    assert!(
        FirstSubmissionInput::new(vec![block; FIRST_SUBMISSION_MAX_BLOCKS + 1]).is_err(),
        "超过 64 块必须拒绝"
    );
}

#[test]
fn stages_and_outcomes_roundtrip() {
    for stage in FirstSubmissionStage::ALL {
        assert_eq!(stage.as_str().parse::<FirstSubmissionStage>(), Ok(stage));
    }
    for outcome in RequestOutcome::ALL {
        assert_eq!(outcome.as_str().parse::<RequestOutcome>(), Ok(outcome));
    }
}

#[test]
fn echo_without_context_or_link_is_unattributed() {
    let event = UserEchoEvent {
        link: None,
        agent_message: None,
        content: JsonValueText::new(r#"[{"type":"text","text":"hello"}]"#).expect("content"),
        content_digest: Digest::new(&"A".repeat(43)).expect("digest"),
    };
    assert_eq!(
        associate_user_echo(None, &event),
        EchoAssociation::Unattributed
    );
    let context = context();
    assert_eq!(
        associate_user_echo(Some(&context), &event),
        EchoAssociation::Unattributed
    );
}

#[test]
fn echo_matches_only_the_exact_dispatch_context() {
    let context = context();
    let content = JsonValueText::new(r#"[{"type":"text","text":"hello"}]"#).expect("content");
    let event = UserEchoEvent {
        link: Some(link(&context)),
        // 不同的 Agent messageId 不是匹配依据。
        agent_message: Some(MessageId::new(&uuid(9)).expect("message")),
        content_digest: context.input_digest.clone(),
        content,
    };
    assert_eq!(
        associate_user_echo(Some(&context), &event),
        EchoAssociation::Duplicate {
            user_message: context.user_message.clone()
        }
    );

    let mut other_generation = link(&context);
    other_generation.resource_generation = ResourceGeneration::new(2);
    let late = UserEchoEvent {
        link: Some(other_generation),
        agent_message: None,
        content_digest: context.input_digest.clone(),
        content: JsonValueText::new(r#"[{"type":"text","text":"hello"}]"#).expect("content"),
    };
    assert_eq!(
        associate_user_echo(Some(&context), &late),
        EchoAssociation::Unattributed,
        "旧资源代/迟到事件不得被猜测归并"
    );
}

#[test]
fn different_echo_content_does_not_overwrite_the_saved_message() {
    let context = context();
    let event = UserEchoEvent {
        link: Some(link(&context)),
        agent_message: Some(MessageId::new(&uuid(8)).expect("message")),
        content: JsonValueText::new(r#"[{"type":"text","text":"different"}]"#).expect("content"),
        content_digest: Digest::new(&"E".repeat(43)).expect("digest"),
    };
    match associate_user_echo(Some(&context), &event) {
        EchoAssociation::Differs {
            user_message,
            agent_message,
        } => {
            assert_eq!(user_message, context.user_message);
            assert_eq!(agent_message, event.agent_message);
        }
        other => panic!("expected Differs, got {other:?}"),
    }
}

#[test]
fn duplicate_echo_is_reused_without_a_second_message() {
    let context = context();
    let event = UserEchoEvent {
        link: Some(link(&context)),
        agent_message: Some(MessageId::new(&uuid(9)).expect("message")),
        content: JsonValueText::new(r#"[{"type":"text","text":"hello"}]"#).expect("content"),
        content_digest: context.input_digest.clone(),
    };
    // 重复投递（同一条回显事件两次）只指向已保存的那条用户消息，不产生第二个身份。
    let first = associate_user_echo(Some(&context), &event);
    let second = associate_user_echo(Some(&context), &event);
    assert_eq!(first, second);
    assert_eq!(
        first,
        EchoAssociation::Duplicate {
            user_message: context.user_message.clone()
        }
    );
}

#[test]
fn differs_and_unattributed_echoes_are_preserved() {
    let context = context();
    let differs_content =
        JsonValueText::new(r#"[{"type":"text","text":"echoed differently"}]"#).expect("content");
    let differs = UserEchoEvent {
        link: Some(link(&context)),
        agent_message: Some(MessageId::new(&uuid(8)).expect("message")),
        content: differs_content.clone(),
        content_digest: Digest::new(&"E".repeat(43)).expect("digest"),
    };
    assert!(matches!(
        associate_user_echo(Some(&context), &differs),
        EchoAssociation::Differs { user_message, .. } if user_message == context.user_message
    ));
    // 异内容事件本身不被改写：调用方保留它作为独立的 Agent 回显。
    assert_eq!(differs.content, differs_content);

    let unattributed_content =
        JsonValueText::new(r#"[{"type":"text","text":"late echo"}]"#).expect("content");
    let mut late_link = link(&context);
    late_link.turn = TurnId::new(&uuid(10)).expect("turn");
    let late = UserEchoEvent {
        link: Some(late_link),
        agent_message: None,
        content: unattributed_content.clone(),
        content_digest: context.input_digest.clone(),
    };
    assert_eq!(
        associate_user_echo(Some(&context), &late),
        EchoAssociation::Unattributed,
        "迟到/不可归属的回显必须保留为独立事件，不覆盖已保存输入"
    );
    assert_eq!(late.content, unattributed_content);
}

#[test]
fn a_second_intentional_identical_text_has_a_new_identity() {
    let first = context();
    let mut second = first.clone();
    second.first_submission = Some(FirstSubmissionId::new(&uuid(5)).expect("first submission"));
    second.user_message = MessageId::new(&uuid(6)).expect("message");
    second.request = RequestId::new(&uuid(7)).expect("request");

    let echo = UserEchoEvent {
        link: Some(link(&second)),
        agent_message: None,
        content: JsonValueText::new(r#"[{"type":"text","text":"hello"}]"#).expect("content"),
        content_digest: first.input_digest.clone(),
    };
    assert_eq!(
        associate_user_echo(Some(&first), &echo),
        EchoAssociation::Unattributed,
        "旧尝试的上下文不得吞掉新一次输入的相同文字"
    );
    assert_eq!(
        associate_user_echo(Some(&second), &echo),
        EchoAssociation::Duplicate {
            user_message: second.user_message.clone()
        }
    );
}

#[test]
fn config_selection_keeps_exact_values() {
    let config = FirstSubmissionConfig {
        id: ConfigOptionId::new("model").expect("id"),
        value: ConfigValue::Select(SelectValue::new("gpt-5").expect("value")),
    };
    assert_eq!(config.value, ConfigValue::select("gpt-5").expect("value"));
    assert_eq!(text("x"), ConfigValue::text("x").expect("value"));
}

#[test]
fn view_json_is_not_used_for_the_saved_input() {
    // 摘要只来自 `FirstSubmissionInput`，不接受调用方另行提供的 JSON（防止正文与摘要被指认成两份内容）。
    let input = FirstSubmissionInput::new(vec![
        PromptContentBlock::from_json_text(r#"{"type":"text","text":"hello"}"#).expect("block"),
    ])
    .expect("input");
    assert_eq!(input.digest().as_str().len(), 43);
    assert!(
        ViewJson::new(input.json().as_str()).is_err(),
        "输入顶层是数组"
    );
}
