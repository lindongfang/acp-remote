//! `model` 的单元测试：每个值对象的正/负取值、`Sequence` 上界、ID 与 `EventType` 的拒绝路径、状态机终止态
//! 约束、`Ephemeral` 的类型级排除、以及 §7.3 表约束在构造器里的镜像。
//!
//! 这里的断言只依赖公开 API（含 crate 内部可见的模块路径），不依赖进程 cwd，也不读写数据库。

use std::str::FromStr;

use super::*;

const UUID_A: &str = "11111111-2222-3333-4444-555555555555";
const UUID_B: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const UUID_C: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";
const T0: &str = "2026-09-18T12:00:00.000Z";
const T1: &str = "2026-09-18T12:00:01.000Z";
const T2: &str = "2026-09-18T12:00:02.000Z";

fn ts(text: &str) -> Timestamp {
    Timestamp::from_str(text).expect("valid timestamp")
}

fn session_id() -> SessionId {
    SessionId::from_str(UUID_A).expect("session id")
}

fn turn_id() -> TurnId {
    TurnId::from_str(UUID_B).expect("turn id")
}

fn request_id() -> RequestId {
    RequestId::from_str(UUID_C).expect("request id")
}

fn digest() -> Digest {
    Digest::from_str(&"A".repeat(43)).expect("digest")
}

fn nonce() -> Nonce {
    Nonce::from_str(&"E".repeat(43)).expect("nonce")
}

fn fingerprint() -> Fingerprint {
    Fingerprint::from_str(&"ab".repeat(32)).expect("fingerprint")
}

fn agent_ref() -> AgentRef {
    AgentRef::try_new(AgentId::new("codex").expect("agent id"), "Codex CLI").expect("agent ref")
}

fn view(text: &str) -> ViewJson {
    ViewJson::from_str(text).expect("view json")
}

fn local_session(state: SessionState) -> Session {
    let created = ts(T0);
    let reference = OwnedSessionRef::new(session_id());
    Session::try_new(
        session_id(),
        reference,
        Some("title".to_owned()),
        agent_ref(),
        state,
        ResourceOrigin::Local,
        None,
        Version::from(1),
        created.clone(),
        created,
    )
    .expect("session")
}

// ---------------------------------------------------------------- §3.1 标识

#[test]
fn uuid_ids_accept_canonical_and_reject_everything_else() {
    assert!(SessionId::from_str(UUID_A).is_ok());
    assert!(NodeId::from_str(UUID_B).is_ok());
    assert!(DeviceId::from_str(UUID_B).is_ok());
    assert!(EventId::from_str(UUID_B).is_ok());
    assert!(TurnId::from_str(UUID_B).is_ok());
    assert!(InteractionId::from_str(UUID_B).is_ok());
    assert!(PairingId::from_str(UUID_B).is_ok());
    assert!(AttachmentId::from_str(UUID_B).is_ok());
    assert!(ServerEpoch::from_str(UUID_B).is_ok());
    assert!(OriginEpoch::from_str(UUID_B).is_ok());

    // 大写、缺连字符、长度错、非 hex、NUL、空串全部拒绝。
    for text in [
        "11111111-2222-3333-4444-55555555555A",
        "11111111222233334444555555555555",
        "11111111-2222-3333-4444-55555555555",
        "11111111-2222-3333-4444-55555555555g",
        "11111111-2222-3333-4444-5555555555\0Z",
        "",
    ] {
        assert_eq!(
            SessionId::from_str(text),
            Err(InvalidValue::Uuid),
            "must reject {text:?}"
        );
    }
    assert_eq!(
        SessionId::try_from("not-a-uuid".to_owned()),
        Err(InvalidValue::Uuid)
    );
    assert_eq!(session_id().as_str(), UUID_A);
    assert_eq!(session_id().to_string(), UUID_A);
}

#[test]
fn spec_ids_follow_their_own_patterns() {
    assert!(WorkspaceAlias::new("work-pc.1").is_ok());
    // alias 必须以小写字母或数字开头（大写与 `-` 开头都不行）。
    assert_eq!(
        WorkspaceAlias::new("Work"),
        Err(InvalidValue::WorkspaceAlias)
    );
    assert_eq!(
        WorkspaceAlias::new("-work"),
        Err(InvalidValue::WorkspaceAlias)
    );
    assert_eq!(
        WorkspaceAlias::new(&"a".repeat(65)),
        Err(InvalidValue::WorkspaceAlias)
    );

    assert!(EventType::new("agent.message.delta").is_ok());
    assert!(EventType::new("a").is_ok());
    assert_eq!(
        EventType::new("Agent.Message"),
        Err(InvalidValue::EventType)
    );
    assert_eq!(
        EventType::new("agent message"),
        Err(InvalidValue::EventType)
    );
    assert_eq!(EventType::new(""), Err(InvalidValue::EventType));
    assert_eq!(
        EventType::new(&"a".repeat(129)),
        Err(InvalidValue::EventType)
    );

    assert!(AgentId::new("codex").is_ok());
    assert_eq!(AgentId::new(""), Err(InvalidValue::AgentId));
    assert_eq!(AgentId::new(&"a".repeat(129)), Err(InvalidValue::AgentId));

    assert!(TemplateId::new("tpl.1").is_ok());
    assert_eq!(TemplateId::new("tpl/1"), Err(InvalidValue::TemplateId));
    assert!(
        TemplateId::new(&"a".repeat(128)).is_ok(),
        "权威宽度是 schemas/node-link/v1/common.schema.json 的 {{1,128}}"
    );
    assert_eq!(
        TemplateId::new(&"a".repeat(129)),
        Err(InvalidValue::TemplateId)
    );
}

#[test]
fn entity_ref_exposes_kind_and_target_id_for_storage_columns() {
    let session = EntityRef::Session(session_id());
    assert_eq!(session.kind(), "session");
    assert_eq!(session.target_id(), UUID_A);
    assert_eq!(session.to_string(), format!("session:{UUID_A}"));

    let command = EntityRef::Command {
        session: Some(session_id()),
        request: request_id(),
    };
    assert_eq!(command.kind(), "command");
    assert_eq!(command.target_id(), format!("{UUID_A}/{UUID_C}"));

    let detached = EntityRef::Command {
        session: None,
        request: request_id(),
    };
    assert_eq!(detached.target_id(), UUID_C);

    // §3.1：`Provider(String)` 的 token 必须能作审计/仓储的 `target_kind` 与 `target_id` 落库。
    let provider = EntityRef::Provider("openai".to_owned());
    assert_eq!(provider.kind(), "provider");
    assert_eq!(provider.target_id(), "openai");
    assert_eq!(provider.to_string(), "provider:openai");
}

// ---------------------------------------------------------------- §3.2 序号/时间/摘要

#[test]
fn sequence_is_capped_at_2_pow_63_minus_1() {
    let max = Sequence::new(Sequence::MAX).expect("max sequence");
    assert_eq!(max.get(), i64::MAX as u64);
    assert_eq!(
        Sequence::new(Sequence::MAX + 1),
        Err(InvalidValue::SequenceRange)
    );
    assert_eq!(
        Sequence::try_from(u64::MAX),
        Err(InvalidValue::SequenceRange)
    );
    assert_eq!(max.checked_next(), None);
    assert_eq!(
        Sequence::new(0).expect("zero").checked_next(),
        Some(Sequence::new(1).expect("one"))
    );
    assert_eq!(Sequence::new(42).expect("seq").to_string(), "42");
    assert_eq!(u64::from(Sequence::new(42).expect("seq")), 42);
}

#[test]
fn other_counters_are_unbounded() {
    assert_eq!(Version::from(u64::MAX).get(), u64::MAX);
    assert_eq!(AttachmentGeneration::from(0).get(), 0);
    assert_eq!(LocalCursor::from(7).to_string(), "7");
    assert_eq!(Version::from(1).checked_next(), Some(Version::from(2)));
}

#[test]
fn timestamp_shape_and_ranges_are_enforced_and_ordering_is_chronological() {
    assert!(Timestamp::from_str(T0).is_ok());
    for bad in [
        "2026-09-18T12:00:00.00Z", // 毫秒位数不足
        "2026-09-18T12:00:00.000", // 缺 Z
        "2026-09-18T12:00:00.000+00:00",
        "2026-09-18 12:00:00.000Z", // 空格代替 T
        "2026-09-18T12:00:00.000z", // 小写 z
        "2026-13-18T12:00:00.000Z", // 月份越界
        "2026-09-32T12:00:00.000Z", // 日期越界
        "2026-09-18T24:00:00.000Z", // 小时越界
        "2026-09-18T12:60:00.000Z", // 分钟越界
        "2026-09-18T12:00:60.000Z", // 秒越界
        "",
    ] {
        assert_eq!(
            Timestamp::from_str(bad),
            Err(InvalidValue::Timestamp),
            "must reject {bad:?}"
        );
    }
    assert!(ts(T0) < ts(T1));
    assert!(ts(T1) < ts(T2));
    assert_eq!(ts(T0).as_str(), T0);
}

#[test]
fn digest_requires_canonical_unpadded_base64url_of_32_bytes() {
    // 真实 SHA-256 的规范 base64url（无填充）。末字符取自 16 个字母表下标为 4 的倍数的字符
    // （A E I M Q U Y c g k o s w 0 4 8）——只接受 A/E/I/M 会让约 3/4 的真实摘要无法构造。
    for real in [
        "DiKpPGEQSOroFzUNvOiVymdFVeVKkhp_kNNvPhTNAFw", // sha256("attachment-bytes")，末字符 w
        "I59Z7VXnN8dxR89VrQwbAwttfudIp0JpUvm4UtWpNeU", // sha256("payload")，末字符 U
        "ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0", // sha256("abc")，末字符 0
        "vJF9gxWBLCOChqUWzmYyi6nh_BgHwa8MkjXaDdHsUA0", // sha256("acp-remote")，末字符 0
        "LXEWQrcmsEQBYnyp-6wy9chTD7GQPMTbAiWHF5IaSIE", // sha256("x")，末字符 E
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", // 全零摘要的规范形式
    ] {
        assert_eq!(
            Digest::from_str(real).expect("real digest").as_str(),
            real,
            "canonical digest {real} must be accepted"
        );
        assert!(Nonce::from_str(real).is_ok());
    }

    // 42/44 字符、`=` 填充、标准字母表都拒绝。
    assert_eq!(Digest::from_str(&"A".repeat(42)), Err(InvalidValue::Digest));
    assert_eq!(Digest::from_str(&"A".repeat(44)), Err(InvalidValue::Digest));
    assert_eq!(
        Digest::from_str(&format!("{}=", "A".repeat(42))),
        Err(InvalidValue::Digest)
    );
    assert_eq!(
        Digest::from_str(&format!("{}+", "A".repeat(42))),
        Err(InvalidValue::Digest)
    );
    // 末字符的字母表下标必须是 4 的倍数（低 2 位为 0）：w→x(49)、E→F(5)、E→-(62) 都被拒绝。
    assert_eq!(
        Digest::from_str("DiKpPGEQSOroFzUNvOiVymdFVeVKkhp_kNNvPhTNAFx"),
        Err(InvalidValue::Digest)
    );
    assert_eq!(
        Digest::from_str("LXEWQrcmsEQBYnyp-6wy9chTD7GQPMTbAiWHF5IaSIF"),
        Err(InvalidValue::Digest)
    );
    assert_eq!(
        Digest::from_str("LXEWQrcmsEQBYnyp-6wy9chTD7GQPMTbAiWHF5IaSI-"),
        Err(InvalidValue::Digest)
    );
    assert_eq!(
        Digest::from_str(&format!("{}B", "A".repeat(42))),
        Err(InvalidValue::Digest)
    );
    assert_eq!(Nonce::from_str(&"A".repeat(42)), Err(InvalidValue::Nonce));

    assert_eq!(fingerprint().as_str().len(), 64);
    assert_eq!(
        Fingerprint::new(&"AB".repeat(32)),
        Err(InvalidValue::Fingerprint)
    );
    assert_eq!(
        Fingerprint::new(&"a".repeat(63)),
        Err(InvalidValue::Fingerprint)
    );
}

// ---------------------------------------------------------------- ViewJson

#[test]
fn view_json_keeps_bytes_and_requires_a_well_formed_object() {
    let text = r#"{"b":1,"a":{"nested":[-1,0,1.5,2e3],"s":"\u00e9\/"}}"#;
    let parsed = view(text);
    assert_eq!(parsed.as_str(), text, "bytes must be preserved verbatim");
    assert_eq!(parsed.byte_len(), text.len());
    assert_eq!(parsed.depth(), 3);

    for bad in [
        "[]", // 顶层不是 object
        "null",
        "42",
        r#"{"a":1"#, // 截断
        r#"{"a":1} extra"#,
        r#"{"a":01}"#, // 前导零
        r#"{"a":.5}"#,
        r#"{"a":1.}"#,
        r#"{"a":1e}"#,
        r#"{"a":"\x"}"#,     // 非法转义
        "{\"a\":\"\u{1}\"}", // 未转义控制字符（U+0001）
        r#"{"a":"\ud800"}"#, // 孤立高代理
        r#"{"a":"\udc00"}"#, // 孤立低代理
        r#"{"a":tru}"#,
        "{",
        "",
    ] {
        assert_eq!(
            ViewJson::from_str(bad),
            Err(InvalidValue::Json),
            "must reject {bad:?}"
        );
    }
    assert_eq!(ViewJson::empty_object().as_str(), "{}");
    assert_eq!(ViewJson::empty_object().depth(), 1);
    // 合法代理对与深嵌套都接受。
    assert!(ViewJson::from_str(r#"{"a":"\ud83d\ude00"}"#).is_ok());
    let deep = format!("{}1{}", r#"{"a":"#.repeat(20), "}".repeat(20));
    assert_eq!(view(&deep).depth(), 20);
}

// ---------------------------------------------------------------- 顶层成员的读取与前置插入（§10.3 收口）

#[test]
fn top_level_member_distinguishes_absent_text_and_non_text() {
    let text = r#"{"a":"x","escaped":"\u0061","number":12,"nested":{"a":1}}"#;
    assert!(matches!(
        top_level_member(text, "missing"),
        Ok(MemberValue::Absent)
    ));
    assert!(matches!(
        top_level_member(text, "number"),
        Ok(MemberValue::NonText)
    ));
    assert!(matches!(
        top_level_member(text, "nested"),
        Ok(MemberValue::NonText)
    ));
    match top_level_member(text, "a").expect("member") {
        MemberValue::Text(value) => assert_eq!(value, "x"),
        other => panic!("a must be text: {other:?}"),
    }
    // 转义按解码后的取值比较（同值不同字节也算一致）。
    match top_level_member(text, "escaped").expect("member") {
        MemberValue::Text(value) => assert_eq!(value, "a"),
        other => panic!("escaped must be text: {other:?}"),
    }
}

#[test]
fn inserting_a_top_level_member_changes_nothing_else() {
    let text = r#"{"b":[1,{"c":"d"}],"e":"\u00e9"}"#;
    let inserted = insert_string_member_front(text, "turnId", "\"abc\"").expect("insert");
    assert_eq!(
        inserted,
        r#"{"turnId":"abc","b":[1,{"c":"d"}],"e":"\u00e9"}"#
    );
    assert_eq!(
        view(&inserted).as_str(),
        inserted,
        "结果必须仍是合法 object"
    );
    // 前导空白与被插入对象里的空白都原样保留。
    let spaced = insert_string_member_front("  {  \"a\" : 1 }", "k", "\"v\"").expect("insert");
    assert_eq!(spaced, "  {\"k\":\"v\",  \"a\" : 1 }");
    // 空对象。
    assert_eq!(
        insert_string_member_front("{}", "k", "\"v\"").expect("insert"),
        r#"{"k":"v"}"#
    );
    // 字符串成员是标量，不增加嵌套深度（深度上限不受影响）。
    let deep = format!("{}1{}", r#"{"a":"#.repeat(128), "}".repeat(128));
    let deep_inserted = insert_string_member_front(&deep, "k", "\"v\"").expect("insert");
    assert_eq!(view(&deep_inserted).depth(), 128);
}

#[test]
fn inserting_rejects_existing_keys_non_string_values_and_non_objects() {
    assert_eq!(
        insert_string_member_front(r#"{"k":"v"}"#, "k", "\"v\""),
        Err(InvalidValue::Field),
        "已有同名成员时不得写出第二个键"
    );
    assert_eq!(
        insert_string_member_front("{}", "k", "12"),
        Err(InvalidValue::Json),
        "值必须是 JSON 字符串字面量"
    );
    assert_eq!(
        insert_string_member_front("[]", "k", "\"v\""),
        Err(InvalidValue::Json),
        "非 object 文本必须被拒绝"
    );
}

#[test]
fn view_json_rejects_nesting_beyond_the_cap() {
    let too_deep = format!("{}1{}", r#"{"a":"#.repeat(129), "}".repeat(129));
    assert_eq!(
        ViewJson::from_str(&too_deep),
        Err(InvalidValue::Depth {
            max: ViewJson::MAX_DEPTH
        })
    );
}

// ---------------------------------------------------------------- §3.3 会话/turn/状态机

#[test]
fn session_state_has_exactly_three_values_and_a_single_reducer() {
    assert_eq!(
        SessionState::ALL,
        &[
            SessionState::Idle,
            SessionState::Running,
            SessionState::Pending
        ]
    );
    // 唯一归约规则：有效交互优先，其次未结束任务，否则空闲。会话没有终态。
    assert_eq!(SessionActivity::reduce(false, false), SessionState::Idle);
    assert_eq!(SessionActivity::reduce(false, true), SessionState::Running);
    assert_eq!(SessionActivity::reduce(true, false), SessionState::Pending);
    assert_eq!(SessionActivity::reduce(true, true), SessionState::Pending);
    assert!(SessionActivity::turn_is_unfinished(TurnState::Queued));
    assert!(SessionActivity::turn_is_unfinished(
        TurnState::WaitingPermission
    ));
    assert!(!SessionActivity::turn_is_unfinished(TurnState::Completed));
    assert!(!SessionActivity::turn_is_unfinished(TurnState::Failed));

    let mut session = local_session(SessionState::Idle);
    session.transition(SessionState::Running, &ts(T0));
    assert_eq!(session.state(), SessionState::Running);
    session.transition(SessionState::Pending, &ts(T1));
    assert_eq!(session.updated_at().as_str(), T1);
    session.transition(SessionState::Idle, &ts(T2));
    assert_eq!(session.updated_at().as_str(), T2);
}

#[test]
fn session_constructor_checks_cross_field_invariants() {
    let created = ts(T0);
    let reference = OwnedSessionRef::new(session_id());
    // reference 必须指向本 id。
    assert_eq!(
        Session::try_new(
            session_id(),
            OwnedSessionRef::new(SessionId::from_str(UUID_B).expect("session id")),
            None,
            agent_ref(),
            SessionState::Idle,
            ResourceOrigin::Local,
            None,
            Version::from(1),
            created.clone(),
            created.clone(),
        ),
        Err(InvalidValue::Field)
    );
    assert_eq!(
        Session::try_new(
            session_id(),
            reference,
            Some("t".repeat(513)),
            agent_ref(),
            SessionState::Idle,
            ResourceOrigin::Local,
            None,
            Version::from(1),
            created.clone(),
            created,
        ),
        Err(InvalidValue::TooLong { max: 512 })
    );

    let summary = local_session(SessionState::Running).summary(None);
    assert_eq!(summary.session_id(), &session_id());
    assert_eq!(summary.state(), SessionState::Running);
    assert_eq!(summary.version(), Version::from(1));
}

#[test]
fn turn_state_machine_sets_timestamps_and_stops_at_terminal() {
    use TurnState::*;
    assert!(!Queued.is_terminal());
    assert!(Completed.is_terminal());
    assert!(Failed.is_terminal());
    assert!(Cancelled.is_terminal());
    assert!(Queued.can_transition_to(Running));
    assert!(!Queued.can_transition_to(Completed));
    assert!(!Completed.can_transition_to(Running));

    let mut turn = Turn::try_new(
        turn_id(),
        session_id(),
        Queued,
        0,
        Some(request_id()),
        None,
        None,
    )
    .expect("turn");
    turn.transition(Running, &ts(T0))
        .expect("queued -> running");
    assert_eq!(turn.started_at().map(Timestamp::as_str), Some(T0));
    // 自转换不是合法转换。
    assert_eq!(
        turn.transition(Running, &ts(T1)),
        Err(InvalidValue::StateTransition)
    );
    turn.transition(Completed, &ts(T2))
        .expect("running -> completed");
    assert_eq!(turn.ended_at().map(Timestamp::as_str), Some(T2));
    assert_eq!(
        turn.transition(Cancelled, &ts(T2)),
        Err(InvalidValue::StateTransition)
    );

    // ended_at 与终态必须一致。
    assert_eq!(
        Turn::try_new(turn_id(), session_id(), Completed, 0, None, None, None),
        Err(InvalidValue::Field)
    );
}

#[test]
fn command_status_terminal_rules() {
    assert_eq!(CommandStatus::ALL.len(), 5);
    assert!(!CommandStatus::Accepted.is_terminal());
    for status in [
        CommandStatus::Completed,
        CommandStatus::Failed,
        CommandStatus::Rejected,
        CommandStatus::Uncertain,
    ] {
        assert!(status.is_terminal());
        assert!(
            !status.can_transition_to(CommandStatus::Completed),
            "{status:?} is terminal"
        );
    }
    assert!(CommandStatus::Accepted.can_transition_to(CommandStatus::Uncertain));
    assert_eq!(
        CommandStatus::from_str("uncertain").expect("token"),
        CommandStatus::Uncertain
    );
    assert_eq!(CommandStatus::from_str("bogus"), Err(InvalidValue::Field));
    assert_eq!(CommandStatus::Uncertain.as_str(), "uncertain");
}

#[test]
fn config_option_requires_current_value_to_match_kind() {
    let id = ConfigOptionId::new("model").expect("config id");
    assert_eq!(
        ConfigOption::try_new(
            id.clone(),
            "Model",
            None,
            None,
            ConfigOptionKind::Boolean,
            ConfigValue::select("gpt").expect("select"),
            Vec::new(),
        ),
        Err(InvalidValue::Field)
    );
    let option = ConfigOption::try_new(
        id,
        "Model",
        Some("d".to_owned()),
        Some("core".to_owned()),
        ConfigOptionKind::Select,
        ConfigValue::select("gpt").expect("select"),
        vec![
            ConfigOptionEntry::try_new(SelectValue::new("gpt").expect("value"), "GPT", None)
                .expect("entry"),
        ],
    )
    .expect("config option");
    assert_eq!(option.kind(), ConfigOptionKind::Select);
    assert_eq!(option.options().len(), 1);
    assert!(ConfigValue::boolean(true).matches_kind(ConfigOptionKind::Boolean));
    assert!(!ConfigValue::boolean(true).matches_kind(ConfigOptionKind::Select));
    assert_eq!(
        ConfigValue::text(&"t".repeat(4097)),
        Err(InvalidValue::TooLong { max: 4096 })
    );
    assert!(ConfigValue::text("").is_ok());
}

#[test]
fn client_command_validates_family_session_and_version() {
    let actor = Actor::Device {
        device: DeviceId::from_str(UUID_B).expect("device"),
    };
    let command = ClientCommand {
        actor,
        request: request_id(),
        command: "session.prompt".to_owned(),
        kind: CommandKind::Mutation,
        session: Some(session_id()),
        expected_version: None,
        request_fingerprint: digest(),
        payload: CommandPayload::Prompt {
            content: vec![
                PromptContentBlock::from_json_text(r#"{"type":"text","text":"hi"}"#)
                    .expect("block"),
            ],
        },
    };
    command.validate().expect("valid prompt command");

    // 家族不一致。
    let mut wrong_family = command.clone();
    wrong_family.kind = CommandKind::Query;
    assert_eq!(wrong_family.validate(), Err(InvalidValue::Field));

    // 缺 sessionId。
    let mut missing_session = command.clone();
    missing_session.session = None;
    assert_eq!(missing_session.validate(), Err(InvalidValue::Field));

    // 查询命令禁止 expectedVersion。
    let mut query = ClientCommand {
        actor: Actor::LocalCli,
        request: request_id(),
        command: "session.list".to_owned(),
        kind: CommandKind::Query,
        session: None,
        expected_version: Some(Version::from(1)),
        request_fingerprint: digest(),
        payload: CommandPayload::SessionList {},
    };
    assert_eq!(query.validate(), Err(InvalidValue::Field));
    query.expected_version = None;
    query.validate().expect("valid query");

    // mode.set 必须带 expectedVersion。
    let mut mode_set = ClientCommand {
        actor: Actor::LocalCli,
        request: request_id(),
        command: "session.mode.set".to_owned(),
        kind: CommandKind::Mutation,
        session: Some(session_id()),
        expected_version: None,
        request_fingerprint: digest(),
        payload: CommandPayload::ModeSet {
            mode: ModeId::new("code").expect("mode"),
        },
    };
    assert_eq!(mode_set.validate(), Err(InvalidValue::Field));
    mode_set.expected_version = Some(Version::from(3));
    mode_set.validate().expect("valid mode set");

    // include 必须非空且不重复；prompt 项数受限。
    assert_eq!(
        CommandPayload::SessionRead {
            include: Vec::new()
        }
        .validate(),
        Err(InvalidValue::Empty)
    );
    assert_eq!(
        CommandPayload::SessionRead {
            include: vec![ReadInclude::Messages, ReadInclude::Messages],
        }
        .validate(),
        Err(InvalidValue::Field)
    );
    assert_eq!(
        CommandPayload::Prompt {
            content: Vec::new()
        }
        .validate(),
        Err(InvalidValue::Empty)
    );
    let block = PromptContentBlock::from_json_text("{}").expect("block");
    assert_eq!(
        CommandPayload::Prompt {
            content: vec![block; 65],
        }
        .validate(),
        Err(InvalidValue::TooLong { max: 64 })
    );
    assert_eq!(
        CommandPayload::PermissionResolve {
            interaction: InteractionId::from_str(UUID_B).expect("id"),
            option_id: String::new(),
        }
        .validate(),
        Err(InvalidValue::Empty)
    );
    // 命令名必须是 scope 形状。
    assert_eq!(
        CommandPayload::Cancel { turn: None }.family(),
        CommandKind::Mutation
    );
    assert!(
        CommandPayload::CommandStatus {
            target_request: request_id()
        }
        .validate()
        .is_ok()
    );
}

// ---------------------------------------------------------------- §3.3 命令记录/交互

#[test]
fn command_record_enforces_the_table_checks() {
    let accepted = CommandRecord::try_new(
        Some(session_id()),
        request_id(),
        "session.prompt",
        CommandKind::Mutation,
        Actor::LocalCli,
        Some(ts(T0)),
        CommandStatus::Accepted,
        None,
        None,
        None,
        None,
        Some(Version::from(1)),
        digest(),
    )
    .expect("accepted command");
    assert_eq!(accepted.status(), CommandStatus::Accepted);
    assert_eq!(accepted.request_fingerprint(), &digest());
    assert!(accepted.terminal().is_none());

    // rejected 必须无 accepted_at、无 result、有 error。
    assert_eq!(
        CommandRecord::try_new(
            None,
            request_id(),
            "session.list",
            CommandKind::Query,
            Actor::LocalCli,
            Some(ts(T0)),
            CommandStatus::Rejected,
            None,
            None,
            None,
            Some(PublicError::coded("protocol.schema_invalid", "bad", false).expect("error")),
            None,
            digest(),
        ),
        Err(InvalidValue::Field)
    );

    // mutation 的 completed 必须有 terminal_event。
    assert_eq!(
        CommandRecord::try_new(
            Some(session_id()),
            request_id(),
            "session.prompt",
            CommandKind::Mutation,
            Actor::LocalCli,
            Some(ts(T0)),
            CommandStatus::Completed,
            Some(ts(T1)),
            None,
            Some(CommandResult::new(view(r#"{"turnId":"x"}"#))),
            None,
            None,
            digest(),
        ),
        Err(InvalidValue::Field)
    );

    let terminal = CommandTerminalRecord::try_new(
        CommandStatus::Completed,
        Some(ts(T1)),
        Some(EventId::from_str(UUID_B).expect("event id")),
        Some(CommandResult::new(view(r#"{"turnId":"x"}"#))),
        None,
    )
    .expect("terminal record");
    assert_eq!(terminal.status(), CommandStatus::Completed);

    // accepted 不是终态，不能出现在终态子集里。
    assert_eq!(
        CommandTerminalRecord::try_new(CommandStatus::Accepted, None, None, None, None),
        Err(InvalidValue::Field)
    );
    // completed 不得携带 error。
    assert_eq!(
        CommandTerminalRecord::try_new(
            CommandStatus::Completed,
            Some(ts(T1)),
            None,
            None,
            Some(PublicError::coded("internal.unavailable", "x", true).expect("error")),
        ),
        Err(InvalidValue::Field)
    );
    // failed 必须携带 error。
    assert_eq!(
        CommandTerminalRecord::try_new(CommandStatus::Failed, Some(ts(T1)), None, None, None),
        Err(InvalidValue::Field)
    );
    // rejected 没有 terminal_at。
    assert_eq!(
        CommandTerminalRecord::try_new(
            CommandStatus::Rejected,
            Some(ts(T1)),
            None,
            None,
            Some(PublicError::coded("command.unsupported", "x", false).expect("error")),
        ),
        Err(InvalidValue::Field)
    );
}

#[test]
fn public_error_validates_code_charset_and_message_length() {
    let error =
        PublicError::coded("command.idempotency_conflict", "conflict", false).expect("error");
    assert_eq!(error.code(), "command.idempotency_conflict");
    assert!(!error.retryable());
    assert_eq!(error.details().as_str(), "{}");
    assert_eq!(
        PublicError::coded("Command.Conflict", "x", false),
        Err(InvalidValue::ErrorCode)
    );
    assert_eq!(
        PublicError::coded("command.conflict", &"x".repeat(1025), false),
        Err(InvalidValue::TooLong { max: 1024 })
    );
}

#[test]
fn elicitation_values_round_trip_all_six_shapes() {
    let fields = vec![
        (
            "text".to_owned(),
            ElicitationContentValue::Text("héllo\n".to_owned()),
        ),
        ("integer".to_owned(), ElicitationContentValue::Integer(-42)),
        ("number".to_owned(), ElicitationContentValue::Number(1.5)),
        (
            "whole_number".to_owned(),
            ElicitationContentValue::Number(2.0),
        ),
        ("boolean".to_owned(), ElicitationContentValue::Boolean(true)),
        (
            "names".to_owned(),
            ElicitationContentValue::TextArray(vec!["a".to_owned(), "b".to_owned()]),
        ),
        (
            "future".to_owned(),
            ElicitationContentValue::unknown(
                r#"{"nested":[1,{"deep":true}],"big":12345678901234567890123}"#,
            )
            .expect("unknown"),
        ),
    ];
    let values = ElicitationValues::from_fields(fields.clone()).expect("values");
    let text = values.to_json_text();
    // 规范序列化 → 解析回来必须逐字段相等（五形态往返 + Unknown 原文保留）。
    let parsed = ElicitationValues::parse_json(&text).expect("reparse");
    assert_eq!(parsed, values);
    assert_eq!(parsed.fields(), Some(fields.as_slice()));
    // 数值不会退化：整数字面量仍是 Integer，`2.0` 仍是 Number（序列化带小数点）。
    assert_eq!(
        parsed.get("integer"),
        Some(&ElicitationContentValue::Integer(-42))
    );
    assert_eq!(
        parsed.get("whole_number"),
        Some(&ElicitationContentValue::Number(2.0))
    );
    assert!(text.contains("2.0"));
    // Unknown 原样保留（含超过 i64 的大整数）。
    let Some(ElicitationContentValue::Unknown(raw)) = parsed.get("future") else {
        panic!("future value must stay Unknown");
    };
    assert!(raw.as_str().contains("12345678901234567890123"));
    assert!(
        !ElicitationContentValue::unknown("{}")
            .expect("unknown")
            .is_known()
    );

    // 从 ACP 原文直接解析等价。
    let from_text = ElicitationValues::parse_json(&text).expect("parse");
    assert_eq!(from_text, values);
    assert_eq!(ElicitationValues::from_str(&text).expect("FromStr"), values);
}

#[test]
fn elicitation_values_classify_acp_shapes_from_wire_text() {
    let parsed = ElicitationValues::parse_json(
        r#"{ "s": "x", "i": 7, "n": 1e3, "b": false, "a": ["p", "q"],
             "empty_array": [], "big": 99999999999999999999, "mixed": [1, "x"],
             "obj": {"k": 1}, "nil": null }"#,
    )
    .expect("wire values");
    assert_eq!(
        parsed.get("s"),
        Some(&ElicitationContentValue::Text("x".to_owned()))
    );
    assert_eq!(parsed.get("i"), Some(&ElicitationContentValue::Integer(7)));
    assert_eq!(
        parsed.get("n"),
        Some(&ElicitationContentValue::Number(1000.0))
    );
    assert_eq!(
        parsed.get("b"),
        Some(&ElicitationContentValue::Boolean(false))
    );
    assert_eq!(
        parsed.get("a"),
        Some(&ElicitationContentValue::TextArray(vec![
            "p".to_owned(),
            "q".to_owned()
        ]))
    );
    assert_eq!(
        parsed.get("empty_array"),
        Some(&ElicitationContentValue::TextArray(Vec::new()))
    );
    // 超出 i64 的整数、混合数组、object、null 一律 Unknown 且保留原文。
    for key in ["big", "mixed", "obj", "nil"] {
        match parsed.get(key) {
            Some(ElicitationContentValue::Unknown(_)) => {}
            other => panic!("{key} must be Unknown, got {other:?}"),
        }
    }
    assert!(matches!(
        parsed.get("nil"),
        Some(ElicitationContentValue::Unknown(raw)) if raw.as_str() == "null"
    ));
}

#[test]
fn elicitation_values_distinguishes_null_from_empty_object() {
    let null = ElicitationValues::null();
    let empty = ElicitationValues::empty_object();
    let filled =
        ElicitationValues::from_fields([("q".to_owned(), ElicitationContentValue::Integer(1))])
            .expect("filled");

    assert!(null.is_null());
    assert!(!null.is_empty_object());
    assert!(null.fields().is_none());
    assert_eq!(null.to_json_text(), "null");

    assert!(!empty.is_null());
    assert!(empty.is_empty_object());
    assert_eq!(empty.fields(), Some([].as_slice()));
    assert_eq!(empty.to_json_text(), "{}");

    assert!(!filled.is_null());
    assert!(!filled.is_empty_object());
    assert_eq!(filled.to_json_text(), r#"{"q":1}"#);

    assert_ne!(null, empty);
    assert_ne!(empty, filled);
    assert_eq!(ElicitationValues::parse_json(" null ").expect("null"), null);
    assert_eq!(ElicitationValues::parse_json("{}").expect("empty"), empty);
}

#[test]
fn elicitation_values_enforce_size_depth_and_unique_keys() {
    // 序列化后 ≤64 KiB。
    let big = ElicitationValues::from_fields([(
        "q".to_owned(),
        ElicitationContentValue::Text("x".repeat(ElicitationValues::MAX_BYTES)),
    )]);
    assert_eq!(
        big,
        Err(InvalidValue::TooLarge {
            max: ElicitationValues::MAX_BYTES
        })
    );

    // 深度 ≤16（容器自身算第 1 层）。
    let deep_raw = format!(
        "{}1{}",
        r#"{"a":"#.repeat(ElicitationValues::MAX_DEPTH as usize),
        "}".repeat(ElicitationValues::MAX_DEPTH as usize)
    );
    let deep = ElicitationContentValue::unknown(&deep_raw).expect("unknown");
    assert_eq!(
        ElicitationValues::from_fields([("deep".to_owned(), deep)]),
        Err(InvalidValue::Depth {
            max: ElicitationValues::MAX_DEPTH
        })
    );
    // 刚好不越界的一条：深度 15 的嵌套值放在第 1 层容器里。
    let shallow_raw = format!(
        "{}1{}",
        r#"{"a":"#.repeat(ElicitationValues::MAX_DEPTH as usize - 1),
        "}".repeat(ElicitationValues::MAX_DEPTH as usize - 1)
    );
    assert!(
        ElicitationValues::from_fields([(
            "deep".to_owned(),
            ElicitationContentValue::unknown(&shallow_raw).expect("unknown")
        )])
        .is_ok()
    );

    // 重复键与非 object/null 顶层被拒。
    assert_eq!(
        ElicitationValues::parse_json(r#"{"a":1,"a":2}"#),
        Err(InvalidValue::Field)
    );
    assert_eq!(ElicitationValues::parse_json("[]"), Err(InvalidValue::Json));
    assert_eq!(ElicitationValues::parse_json("42"), Err(InvalidValue::Json));
    assert_eq!(
        ElicitationValues::parse_json("{oops"),
        Err(InvalidValue::Json)
    );
    assert_eq!(
        ElicitationValues::from_fields([
            ("a".to_owned(), ElicitationContentValue::Integer(1)),
            ("a".to_owned(), ElicitationContentValue::Integer(2)),
        ]),
        Err(InvalidValue::Field)
    );
    // 非有限数值不是 JSON。
    assert_eq!(
        ElicitationContentValue::number(f64::NAN),
        Err(InvalidValue::Json)
    );
    assert_eq!(
        ElicitationContentValue::unknown("[1,"),
        Err(InvalidValue::Json)
    );
}

#[test]
fn interaction_resolution_enforces_action_versus_values() {
    let filled = ElicitationValues::from_fields([(
        "q".to_owned(),
        ElicitationContentValue::Text("a".to_owned()),
    )])
    .expect("values");

    // submit ⇒ null / 空对象 / 非空对象都可以。
    InteractionResolution::elicitation_submit(filled.clone()).expect("submit with values");
    InteractionResolution::elicitation_submit(ElicitationValues::empty_object())
        .expect("submit {}");
    InteractionResolution::elicitation_submit(ElicitationValues::null()).expect("submit null");

    // cancel / decline ⇒ 必须 null。
    InteractionResolution::elicitation_cancel()
        .validate()
        .expect("cancel");
    InteractionResolution::elicitation_decline()
        .validate()
        .expect("decline");
    for action in [ElicitationAction::Cancel, ElicitationAction::Decline] {
        assert_eq!(
            InteractionResolution::elicitation(action, filled.clone()),
            Err(InvalidValue::ElicitationValues)
        );
        assert_eq!(
            InteractionResolution::Elicitation {
                action,
                values: filled.clone(),
            }
            .validate(),
            Err(InvalidValue::ElicitationValues)
        );
    }
    assert_eq!(ElicitationAction::Decline.as_str(), "decline");
    assert_eq!(ElicitationAction::ALL.len(), 3);

    let decision = PermissionDecision::try_new("allow-once", PermissionDecisionKind::AllowOnce)
        .expect("decision");
    InteractionResolution::permission(decision)
        .validate()
        .expect("permission resolution");

    // 同一规则在 payload 校验里复用。
    assert_eq!(
        CommandPayload::ElicitationRespond {
            interaction: InteractionId::from_str(UUID_B).expect("id"),
            action: ElicitationAction::Cancel,
            values: filled.clone(),
        }
        .validate(),
        Err(InvalidValue::ElicitationValues)
    );
    assert!(
        CommandPayload::ElicitationRespond {
            interaction: InteractionId::from_str(UUID_B).expect("id"),
            action: ElicitationAction::Decline,
            values: ElicitationValues::null(),
        }
        .validate()
        .is_ok()
    );
    assert!(
        CommandPayload::ElicitationRespond {
            interaction: InteractionId::from_str(UUID_B).expect("id"),
            action: ElicitationAction::Submit,
            values: filled,
        }
        .validate()
        .is_ok()
    );
}

#[test]
fn pending_interaction_and_options_validate_lengths() {
    let option = InteractionOption::try_new("allow-once", "Allow once", "allow").expect("option");
    assert_eq!(option.option_id(), "allow-once");
    assert_eq!(option.kind(), "allow");
    assert_eq!(
        InteractionOption::try_new("", "label", "kind"),
        Err(InvalidValue::Empty)
    );
    assert_eq!(
        InteractionOption::try_new("id", "l", &"k".repeat(65)),
        Err(InvalidValue::TooLong { max: 64 })
    );

    let pending = PendingInteraction::try_new(
        InteractionId::from_str(UUID_B).expect("id"),
        InteractionKind::Permission,
        session_id(),
        ts(T0),
        vec![option],
    )
    .expect("pending interaction");
    assert_eq!(pending.kind(), InteractionKind::Permission);
    assert_eq!(pending.session(), &session_id());
    assert_eq!(pending.options().len(), 1);
    assert_eq!(InteractionKind::Elicitation.as_str(), "elicitation");
}

#[test]
fn permission_decision_option_id_is_bounded_like_the_wire_field() {
    assert_eq!(
        PermissionDecision::try_new(&"o".repeat(65), PermissionDecisionKind::RejectOnce),
        Err(InvalidValue::TooLong { max: 64 })
    );
}

// ---------------------------------------------------------------- §3.4 事件

#[test]
fn ephemeral_cannot_enter_a_commit() {
    // 编译期：`StoredPolicy` 只有两个变体；给回 `Ephemeral` 会让这个穷举 match 直接编译失败。
    fn stored_policy_token(policy: StoredPolicy) -> &'static str {
        match policy {
            StoredPolicy::Durable => "durable",
            StoredPolicy::ShortTerm => "short_term",
        }
    }
    assert_eq!(StoredPolicy::ALL.len(), 2);
    assert_eq!(stored_policy_token(StoredPolicy::Durable), "durable");
    assert!(!StoredPolicy::ALL.iter().any(|p| p.as_str() == "ephemeral"));

    // 运行期：唯一的转换入口拒绝 Ephemeral，broker 的组装辅助只返回 None。
    assert_eq!(
        StoredPolicy::try_from(PersistencePolicy::Ephemeral),
        Err(InvalidValue::Ephemeral)
    );
    assert_eq!(
        StoredPolicy::from_persistence(PersistencePolicy::Ephemeral),
        Err(InvalidValue::Ephemeral)
    );
    assert_eq!(
        PersistencePolicy::from(StoredPolicy::ShortTerm),
        PersistencePolicy::ShortTerm
    );
    let event = PendingEvent::from_persistence(
        EventKind::Delta,
        EventType::new("agent.message.delta").expect("event type"),
        PersistencePolicy::Ephemeral,
        EventPayload::new(view(r#"{"text":"x"}"#), None),
        EventOrigin::Agent,
        Some(turn_id()),
        Some(request_id()),
    );
    assert!(
        event.is_none(),
        "ephemeral events are filtered, never committed"
    );

    let persisted = PendingEvent::from_persistence(
        EventKind::Delta,
        EventType::new("agent.message.delta").expect("event type"),
        PersistencePolicy::Durable,
        EventPayload::new(view(r#"{"text":"x"}"#), None),
        EventOrigin::Agent,
        Some(turn_id()),
        Some(request_id()),
    )
    .expect("durable event");
    assert_eq!(persisted.policy, StoredPolicy::Durable);
    persisted.validate().expect("valid payload");
}

#[test]
fn acp_raw_enforces_byte_length_and_json_shape() {
    let raw = r#"{"jsonrpc":"2.0","method":"session/update"}"#;
    let available = AcpRaw::available("application/json", raw, digest()).expect("acp raw");
    let (media_type, text, byte_length, sha256) = available.as_available().expect("available");
    assert_eq!(media_type, "application/json");
    assert_eq!(text, raw);
    assert_eq!(byte_length, raw.len() as u64);
    assert_eq!(sha256, &digest());
    available.validate().expect("coherent");
    assert!(available.as_unavailable().is_none());

    // byte_length 与 raw_json 不一致时 validate 失败（enum 字段公有，读取路径必须校验一次）。
    let incoherent = AcpRaw::Available {
        media_type: "application/json".to_owned(),
        raw_json: raw.to_owned(),
        byte_length: 1,
        sha256: digest(),
    };
    assert_eq!(incoherent.validate(), Err(InvalidValue::Field));
    assert_eq!(
        AcpRaw::available("application/json", "{oops", digest()),
        Err(InvalidValue::JsonDocument)
    );
    assert_eq!(
        AcpRaw::available("", "{}", digest()),
        Err(InvalidValue::Empty)
    );

    let unavailable = AcpRaw::unavailable(RawUnavailableReason::SizeLimit, 4096, None);
    assert_eq!(
        unavailable.as_unavailable(),
        Some((RawUnavailableReason::SizeLimit, 4096, None))
    );
    assert!(unavailable.validate().is_ok());
    assert_eq!(
        RawUnavailableReason::RetentionExpired.as_str(),
        "retention_expired"
    );
}

#[test]
fn committed_event_and_delivery_keep_session_sequences_consistent() {
    let epoch = OriginEpoch::from_str(UUID_C).expect("epoch");
    let sequence = Sequence::new(1).expect("sequence");
    let event = CommittedEvent {
        id: EventId::from_str(UUID_B).expect("event id"),
        event_type: EventType::new("agent.message.delta").expect("event type"),
        session: Some(session_id()),
        session_sequence: Some(sequence),
        global_sequence: sequence,
        origin_epoch: Some(epoch.clone()),
        origin_sequence: Some(sequence),
        created_at: ts(T0),
    };
    event.validate().expect("session event");

    let detached = CommittedEvent {
        session: None,
        session_sequence: None,
        origin_epoch: None,
        origin_sequence: None,
        ..event.clone()
    };
    detached.validate().expect("non-session event");

    let inconsistent = CommittedEvent {
        session: None,
        session_sequence: Some(sequence),
        ..event.clone()
    };
    assert_eq!(inconsistent.validate(), Err(InvalidValue::Field));

    // 两个方向都不得绕过成对 CHECK：非会话级不能带 origin cursor，会话级不能缺 origin cursor。
    let detached_with_origin = CommittedEvent {
        session: None,
        session_sequence: None,
        origin_epoch: None,
        origin_sequence: Some(sequence),
        ..event.clone()
    };
    assert_eq!(detached_with_origin.validate(), Err(InvalidValue::Field));

    let session_without_origin = CommittedEvent {
        origin_epoch: None,
        ..event.clone()
    };
    assert_eq!(session_without_origin.validate(), Err(InvalidValue::Field));
    let session_without_origin_sequence = CommittedEvent {
        origin_sequence: None,
        ..event.clone()
    };
    assert_eq!(
        session_without_origin_sequence.validate(),
        Err(InvalidValue::Field)
    );

    let endpoint = EndpointEvent::view_only(
        EventKind::Structured,
        EventType::new("session.created").expect("event type"),
        view(r#"{"session":{}}"#),
        None,
        None,
        ts(T0),
    );
    endpoint.validate().expect("endpoint event");
    assert_eq!(EventOrigin::LocalCli.as_str(), "local_cli");
    // §7.3 的 CHECK 是五值：`node` 是 Node Link 命令的执行者身份（不是 imported 的跳数，那由
    // `remoteOrigin` 表达）。
    assert_eq!(EventOrigin::Node.as_str(), "node");
    assert_eq!(
        EventOrigin::ALL
            .iter()
            .map(|origin| origin.as_str())
            .collect::<Vec<_>>(),
        vec!["agent", "device", "daemon", "local_cli", "node"]
    );
}

// ---------------------------------------------------------------- §3.5 身份/信任/审计

#[test]
fn actor_exposes_the_storage_identity_columns() {
    let device = DeviceId::from_str(UUID_B).expect("device");
    let actor = Actor::Device {
        device: device.clone(),
    };
    assert_eq!(actor.kind(), ActorKind::Device);
    assert_eq!(actor.kind().as_str(), "device");
    assert_eq!(actor.id_text(), UUID_B);
    assert_eq!(actor.device_id(), Some(&device));

    let node = NodeId::from_str(UUID_A).expect("node");
    let access = NodeId::from_str(UUID_C).expect("access");
    let node_actor = Actor::Node {
        node: node.clone(),
        access_node: access.clone(),
    };
    assert_eq!(node_actor.kind(), ActorKind::Node);
    assert_eq!(
        node_actor.id_text(),
        format!("{UUID_A}/{UUID_C}"),
        "composite idempotency key"
    );
    assert_eq!(node_actor.node_ids(), Some((&node, &access)));

    let cli = Actor::LocalCli;
    assert!(cli.is_local_cli());
    assert_eq!(cli.kind(), ActorKind::Cli);
    assert_eq!(cli.id_text(), "cli");
    assert_eq!(ActorKind::from_str("cli").expect("kind"), ActorKind::Cli);
    assert_eq!(ActorKind::from_str("local_cli"), Err(InvalidValue::Field));
}

/// design D12：配对认领方是第四个 `Actor` 变体，且绑定一个配对——它既不是设备/节点，也不带任何授权
/// 集合，只靠 `pairing()` 让用例面判定授权。
#[test]
fn pairing_claimant_actor_is_bound_to_one_pairing() {
    let pairing = PairingId::from_str(UUID_C).expect("pairing");
    let claimant = Actor::PairingClaimant {
        pairing: pairing.clone(),
    };
    assert_eq!(claimant.kind(), ActorKind::PairingClaimant);
    assert_eq!(claimant.kind().as_str(), "pairing_claimant");
    assert_eq!(claimant.id_text(), UUID_C, "审计行的 actor_id 即该配对 id");
    assert_eq!(claimant.pairing(), Some(&pairing));
    assert!(claimant.device_id().is_none());
    assert!(claimant.node_ids().is_none());
    assert!(!claimant.is_local_cli());
    assert_eq!(Actor::LocalCli.pairing(), None);
    assert_eq!(
        ActorKind::from_str("pairing_claimant").expect("kind"),
        ActorKind::PairingClaimant
    );
    assert_eq!(ActorKind::ALL.len(), 4);
}

#[test]
fn device_record_ties_revocation_state_to_timestamp() {
    let device = DeviceId::from_str(UUID_B).expect("device");
    let record = DeviceRecord::try_new(
        device.clone(),
        "Zhang's Phone",
        fingerprint(),
        DeviceState::Active,
        ts(T0),
        Some(ts(T1)),
        None,
    )
    .expect("device record");
    assert_eq!(record.state(), DeviceState::Active);
    assert_eq!(record.device_id(), &device);
    assert_eq!(record.public_key_fingerprint(), &fingerprint());
    assert_eq!(record.last_seen_at().map(Timestamp::as_str), Some(T1));
    assert!(record.revoked_at().is_none());

    assert_eq!(
        DeviceRecord::try_new(
            device.clone(),
            "Phone",
            fingerprint(),
            DeviceState::Active,
            ts(T0),
            None,
            Some(ts(T1)),
        ),
        Err(InvalidValue::Field)
    );
    assert_eq!(
        DeviceRecord::try_new(
            device,
            &"n".repeat(129),
            fingerprint(),
            DeviceState::Pending,
            ts(T0),
            None,
            None,
        ),
        Err(InvalidValue::TooLong { max: 128 })
    );
}

#[test]
fn node_record_requires_owner_endpoint_only_for_owner_kind() {
    let node = NodeId::from_str(UUID_C).expect("node");
    let build = |kind: NodeKind, endpoint: Option<&str>| {
        NodeRecord::try_new(
            node.clone(),
            "Work PC",
            kind,
            fingerprint(),
            NodeState::Paired,
            endpoint.map(str::to_owned),
            ts(T0),
            None,
            None,
        )
    };
    assert!(build(NodeKind::Access, None).is_ok());
    assert_eq!(
        build(NodeKind::Access, Some("wss://owner.example/node-link/v1")),
        Err(InvalidValue::Field)
    );
    assert_eq!(build(NodeKind::Owner, None), Err(InvalidValue::Field));
    assert_eq!(
        build(NodeKind::Owner, Some("https://owner.example")),
        Err(InvalidValue::Endpoint)
    );
    let owner = build(NodeKind::Owner, Some("wss://owner.example")).expect("owner record");
    assert_eq!(owner.kind(), NodeKind::Owner);
    assert_eq!(owner.owner_endpoint(), Some("wss://owner.example"));
}

/// 配对登记方宣告的绑定（设备为 canonical origin，节点为本机 endpoint）。
const HOST_BINDING: &str = "https://host.example";

#[test]
fn pairing_record_state_machine_matches_timestamps() {
    let id = PairingId::from_str(UUID_B).expect("pairing");
    let build = |state: PairingState,
                 claimed_at: Option<&str>,
                 approved_at: Option<&str>,
                 terminal_at: Option<&str>| {
        PairingRecord::try_new(
            id.clone(),
            PairingTarget::Device,
            state,
            None,
            digest(),
            HOST_BINDING,
            ts(T0),
            ts(T2),
            claimed_at.map(ts),
            approved_at.map(ts),
            terminal_at.map(ts),
        )
    };
    let created = build(PairingState::Created, None, None, None).expect("created");
    assert_eq!(created.state(), PairingState::Created);
    assert!(!created.state().is_terminal());
    assert!(created.claimed_at().is_none());
    assert_eq!(created.host_binding(), HOST_BINDING);

    let claimed = build(PairingState::Claimed, Some(T0), None, None).expect("claimed");
    assert!(!claimed.state().is_visible(), "claimed 只用于事务与审计");

    let approved = build(PairingState::Approved, Some(T0), Some(T1), None).expect("approved");
    assert_eq!(approved.approved_at().map(Timestamp::as_str), Some(T1));
    assert_eq!(approved.expires_at().as_str(), T2);

    // created 不得带 claimed_at。
    assert_eq!(
        build(PairingState::Created, Some(T0), None, None),
        Err(InvalidValue::Field)
    );
    // approved 必须有 approved_at。
    assert_eq!(
        build(PairingState::Approved, Some(T0), None, None),
        Err(InvalidValue::Field)
    );
    // 终态必须有 terminal_at，且 consumed 同时要求 approved_at。
    assert_eq!(
        build(PairingState::Expired, Some(T0), None, None),
        Err(InvalidValue::Field)
    );
    assert_eq!(
        build(PairingState::Consumed, Some(T0), None, Some(T2)),
        Err(InvalidValue::Field)
    );
    let consumed = build(PairingState::Consumed, Some(T0), Some(T1), Some(T2)).expect("consumed");
    assert!(consumed.state().is_terminal());
    // 绑定是配对的事实之一：空值不是合法绑定（§11.2 第 1 条要求认领时逐字回显）。
    assert_eq!(
        PairingRecord::try_new(
            id,
            PairingTarget::Device,
            PairingState::Created,
            None,
            digest(),
            "",
            ts(T0),
            ts(T2),
            None,
            None,
            None,
        ),
        Err(InvalidValue::Empty)
    );
}

#[test]
fn pairing_claim_and_peer_are_typed_and_consistent() {
    let peer = PairingPeer::try_new(
        PeerIdentity::Node(NodeId::from_str(UUID_C).expect("node")),
        "Office Access",
        test_peer_public_key(),
        HOST_BINDING,
        nonce(),
    )
    .expect("peer");
    assert_eq!(peer.id().kind(), "node");
    assert_eq!(peer.display_name(), "Office Access");
    assert_eq!(peer.host_binding(), HOST_BINDING);
    assert_eq!(peer.client_nonce(), &nonce());
    // 指纹不是独立字段：它只能由公钥派生（§11.5）。
    assert_eq!(
        peer.public_key_fingerprint(),
        peer.public_key().fingerprint()
    );

    let pairing = PairingId::from_str(UUID_B).expect("pairing");
    let claim = PairingClaim::try_new(pairing.clone(), peer.clone()).expect("node claim");
    assert_eq!(claim.pairing(), &pairing);
    assert_eq!(claim.peer().id(), peer.id());

    // 对端绑定是 claim 的回显值（设备为 `canonicalOrigin`、节点为 `endpoint`），空值不合法。
    assert_eq!(
        PairingPeer::try_new(
            PeerIdentity::Node(NodeId::from_str(UUID_C).expect("node")),
            "Office Access",
            test_peer_public_key(),
            "",
            nonce(),
        ),
        Err(InvalidValue::Empty)
    );

    let device_peer = PairingPeer::try_new(
        PeerIdentity::Device(DeviceId::from_str(UUID_B).expect("device")),
        "Phone",
        test_peer_public_key(),
        HOST_BINDING,
        nonce(),
    )
    .expect("peer");
    assert!(
        PairingClaim::try_new(pairing, device_peer).is_ok(),
        "设备与节点配对都只携带身份，不带任何授权集合"
    );

    let settlement = PairingSettlement::approved();
    assert!(settlement.is_approved());
    settlement.validate().expect("settlement");
    assert_eq!(settlement, PairingSettlement::Approved);
    let rejected = PairingSettlement::rejected(Some("user said no")).expect("rejected");
    assert!(!rejected.is_approved());
    assert_eq!(
        PairingSettlement::rejected(Some(&"r".repeat(257))),
        Err(InvalidValue::TooLong { max: 256 })
    );
}

#[test]
fn audit_record_carries_no_content_and_closed_action_set() {
    assert_eq!(AuditAction::ALL.len(), 22);
    assert_eq!(AuditAction::PairingCreated.as_str(), "pairing.created");
    assert_eq!(
        AuditAction::from_str("storage.integrity_failed").expect("action"),
        AuditAction::StorageIntegrityFailed
    );
    assert_eq!(
        AuditAction::NodeAuthenticated.as_str(),
        "node.authenticated",
        "节点握手留痕的动作 token"
    );
    assert_eq!(
        AuditAction::from_str("node.authenticated").expect("action"),
        AuditAction::NodeAuthenticated
    );
    assert_eq!(
        AuditAction::from_str("node.auth_failed").expect("action"),
        AuditAction::NodeAuthFailed
    );
    assert_eq!(
        AuditAction::from_str("device.scopes_changed").expect("action"),
        AuditAction::DeviceScopesChanged,
        "历史取值必须仍可按 token 解码：升级前的库可能有这类审计行"
    );
    assert_eq!(
        AuditAction::from_str("something.else"),
        Err(InvalidValue::Field)
    );
    assert_eq!(AuditOutcome::Denied.as_str(), "denied");

    let record = AuditRecord::try_new(
        ts(T0),
        AuditAction::DeviceRevoked,
        Actor::LocalCli,
        None,
        Some("local-user".to_owned()),
        EntityRef::Device(DeviceId::from_str(UUID_B).expect("device")),
        AuditOutcome::Success,
        Some(digest()),
    )
    .expect("audit record");
    assert_eq!(record.action(), AuditAction::DeviceRevoked);
    assert_eq!(record.target().kind(), "device");
    assert_eq!(record.local_principal_ref(), Some("local-user"));
    assert_eq!(record.detail_digest(), Some(&digest()));
    assert_eq!(
        AuditRecord::try_new(
            ts(T0),
            AuditAction::DeviceRevoked,
            Actor::LocalCli,
            None,
            Some(String::new()),
            EntityRef::Session(session_id()),
            AuditOutcome::Success,
            None,
        ),
        Err(InvalidValue::Empty)
    );
}

// ---------------------------------------------------------------- §3.5 Export/Import

#[test]
fn capability_set_dedups_and_template_selection_validates_keys() {
    let set: CapabilitySet = [
        Capability::try_new("tool.call", None).expect("capability"),
        Capability::try_new("tool.call", None).expect("capability"),
        Capability::try_new("mode", Some("plan".to_owned())).expect("capability"),
    ]
    .into_iter()
    .collect();
    assert_eq!(set.len(), 2);
    assert!(CapabilitySet::empty().is_empty());
    assert!(set.contains(&Capability::try_new("tool.call", None).expect("capability")));
    assert_eq!(set.iter().count(), 2);
    assert_eq!(Capability::try_new("", None), Err(InvalidValue::Empty));
    assert_eq!(
        Capability::try_new("k", Some("d".repeat(257))),
        Err(InvalidValue::TooLong { max: 256 })
    );

    let selection = TemplateSelection::try_new(
        TemplateId::new("tpl").expect("template id"),
        vec![(
            "branch".to_owned(),
            ConfigValue::select("main").expect("select"),
        )],
    )
    .expect("selection");
    assert_eq!(selection.params().len(), 1);
    assert_eq!(
        TemplateSelection::try_new(
            TemplateId::new("tpl").expect("template id"),
            vec![
                ("branch".to_owned(), ConfigValue::boolean(true)),
                ("branch".to_owned(), ConfigValue::boolean(false)),
            ],
        ),
        Err(InvalidValue::Field)
    );
    assert_eq!(
        TemplateSelection::try_new(
            TemplateId::new("tpl").expect("template id"),
            vec![("has space".to_owned(), ConfigValue::boolean(true))],
        ),
        Err(InvalidValue::Field)
    );

    let descriptor = AgentDescriptor::new(agent_ref(), true, ResourceOrigin::Local);
    assert!(descriptor.available);
    assert!(descriptor.origin.is_local());
    assert_eq!(descriptor.origin.online(), None);
    let remote = ResourceOrigin::Remote {
        owner_node_id: NodeId::from_str(UUID_C).expect("node"),
        origin_epoch: OriginEpoch::from_str(UUID_B).expect("epoch"),
        online: true,
    };
    assert_eq!(remote.online(), Some(true));
    let remote_ref = remote.remote_session(session_id()).expect("remote ref");
    assert_eq!(remote_ref.session_id, session_id());
}

// ---------------------------------------------------------------- §2 错误类型

#[test]
fn port_error_and_kinds_are_wired_to_the_model_errors() {
    let error = PortError::from(InvalidValue::Uuid);
    assert!(matches!(
        error,
        PortError::InvalidRequest("id is not a canonical lowercase UUID")
    ));
    assert_eq!(
        PortError::NotFound(EntityRef::Session(session_id())).to_string(),
        format!("not found: session:{UUID_A}")
    );
    assert_eq!(
        PortError::Conflict(ConflictKind::IdempotencyConflict).to_string(),
        "conflict: idempotency_conflict"
    );
    assert_eq!(
        PortError::Unavailable(UnavailableKind::StorageFull).to_string(),
        "unavailable: storage_full"
    );
    assert_eq!(ConflictKind::ALL.len(), 8);
    assert_eq!(UnavailableKind::ALL.len(), 8);
    assert_eq!(ConflictKind::AlreadyResolved.as_str(), "already_resolved");
    assert_eq!(
        UnavailableKind::RemoteUnavailable.as_str(),
        "remote_unavailable"
    );
    // 「后端不支持该操作」是 `UnavailableKind` 的独立取值：与「临时故障」类可区分（§2、§5.1）。
    assert_eq!(
        UnavailableKind::BackendUnsupported.as_str(),
        "backend_unsupported"
    );
    assert_eq!(
        PortError::Unavailable(UnavailableKind::BackendUnsupported).to_string(),
        "unavailable: backend_unsupported"
    );
    // `session-create-settle-order`：会话建立入口的「可证明未产生会话」取值（`session.create` 的
    // `failed` 判据输入）；它同样不新增 wire 码，公开错误仍取 `internal.unavailable`。
    assert_eq!(
        UnavailableKind::SessionNotEstablished.as_str(),
        "session_not_established"
    );
    assert_eq!(
        PortError::Unavailable(UnavailableKind::SessionNotEstablished).to_string(),
        "unavailable: session_not_established"
    );
    // Infallible 到 PortError 的收敛存在（供 `TryFrom<u64>` 的不可失败分支使用）。
    let never: Result<Version, std::convert::Infallible> = Ok(Version::from(1));
    assert_eq!(never.map_err(PortError::from).expect("version").get(), 1);
    assert_eq!(
        InvalidValue::TooLong { max: 3 }.to_string(),
        "value exceeds 3 characters"
    );
}

#[test]
fn peer_public_key_is_validated_at_construction_and_derives_its_fingerprint() {
    let bytes = hex_to_bytes(TEST_PUBLIC_KEY_HEX);
    let key = PeerPublicKey::try_from_bytes(&bytes).expect("valid P-256 point");
    assert_eq!(key.as_bytes(), &bytes.as_slice());

    // 指纹 = SHA-256(65 字节原始公钥) 的 64 字符小写 hex，可由独立实现复算。
    let expected = {
        use sha2::Digest as _;
        sha2::Sha256::digest(bytes.as_slice())
            .iter()
            .fold(String::new(), |mut text, byte| {
                text.push_str(&format!("{byte:02x}"));
                text
            })
    };
    assert_eq!(
        Fingerprint::from_str(&expected).expect("hex"),
        key.fingerprint()
    );
    assert_eq!(key.fingerprint().as_str(), expected);

    // 33 字节压缩点必须被拒：`from_sec1_bytes` 本身接受它，长度断言挡在前面（§11.5）。
    let y_is_even = bytes[64] & 1 == 0;
    let mut compressed = vec![if y_is_even { 0x02 } else { 0x03 }];
    compressed.extend_from_slice(&bytes[1..33]);
    assert_eq!(compressed.len(), 33);
    assert_eq!(
        PeerPublicKey::try_from_bytes(&compressed),
        Err(InvalidValue::PublicKey)
    );

    // 长度、前缀与曲线点三类非法输入。
    assert_eq!(
        PeerPublicKey::try_from_bytes(&bytes[..64]),
        Err(InvalidValue::PublicKey)
    );
    let mut wrong_prefix = bytes.clone();
    wrong_prefix[0] = 0x05;
    assert_eq!(
        PeerPublicKey::try_from_bytes(&wrong_prefix),
        Err(InvalidValue::PublicKey)
    );
    let mut off_curve = bytes.clone();
    off_curve[64] ^= 0x01;
    assert_eq!(
        PeerPublicKey::try_from_bytes(&off_curve),
        Err(InvalidValue::PublicKey)
    );
}

#[test]
fn agent_profile_is_a_bounded_command_declaration() {
    let id = AgentId::new("omp").expect("agent id");
    let profile =
        AgentProfile::try_new(id.clone(), "omp", vec!["acp".to_owned()], true).expect("profile");
    assert_eq!(profile.id(), &id);
    assert_eq!(profile.display_name(), "omp");
    assert_eq!(profile.command(), "omp");
    assert_eq!(profile.args(), ["acp"]);
    assert!(profile.is_default());
    assert!(AgentProfile::try_new(id.clone(), "", Vec::new(), false).is_err());
    assert!(AgentProfile::try_new(id.clone(), "omp", vec![String::new()], false).is_err());
    assert!(AgentProfile::try_new(id, "omp", vec!["a\0b".to_owned()], false).is_err());
}

#[test]
fn agent_session_id_is_bounded_and_nul_free() {
    assert_eq!(
        AgentSessionId::new("acp-session-1").expect("合法").as_str(),
        "acp-session-1"
    );
    assert_eq!(
        AgentSessionId::new(&"a".repeat(512))
            .expect("上限内")
            .as_str()
            .len(),
        512
    );
    assert_eq!(AgentSessionId::new(""), Err(InvalidValue::AgentSessionId));
    assert_eq!(
        AgentSessionId::new(&"a".repeat(513)),
        Err(InvalidValue::AgentSessionId)
    );
    assert_eq!(
        AgentSessionId::new("a\0b"),
        Err(InvalidValue::AgentSessionId)
    );
    assert!(matches!(
        PortError::from(InvalidValue::AgentSessionId),
        PortError::InvalidRequest("agent session id must be 1..=512 characters without NUL")
    ));
}

/// §3.1：`WorkspaceRef` 只承载 `{ directoryId }`；标识必须是 32 字符小写十六进制。
#[test]
fn workspace_ref_carries_only_the_directory_id() {
    let directory_id = DirectoryId::new("0123456789abcdef0123456789abcdef").expect("directory id");

    let workspace = WorkspaceRef::new(directory_id.clone());
    assert_eq!(workspace.directory_id(), &directory_id);
    assert_eq!(
        workspace.directory_id().as_str(),
        "0123456789abcdef0123456789abcdef"
    );

    assert_eq!(
        DirectoryId::new("0123456789ABCDEF0123456789abcdef"),
        Err(InvalidValue::DirectoryId)
    );
    assert_eq!(
        DirectoryId::new("0123456789abcdef"),
        Err(InvalidValue::DirectoryId)
    );
    assert_eq!(
        DirectoryId::new("0123456789abcdef0123456789abcdeg"),
        Err(InvalidValue::DirectoryId)
    );
}

/// §3.6：`ResumeSessionRequest` 只承载**持久化的**取值，其中 `workspace_cwd` 必须是绝对路径形状
/// （存在性、目录性与 `canonicalize` 一致性由恢复用例在调用后端之前复校验，不在构造器里做）。
#[test]
fn resume_session_request_validates_the_persisted_working_directory() {
    let agent = agent_ref();
    let id = AgentSessionId::new("acp-session-1").expect("agent session id");
    let absolute = std::env::temp_dir().to_string_lossy().into_owned();

    let request = ResumeSessionRequest::try_new(agent.clone(), id.clone(), absolute.clone())
        .expect("绝对路径合法");
    assert_eq!(request.agent, agent);
    assert_eq!(request.agent_session_id, id);
    assert_eq!(request.workspace_cwd, absolute);

    for rejected in [
        String::new(),
        "relative/path".to_owned(),
        format!("{absolute}\0x"),
        format!("{absolute}{}", "a".repeat(5000)),
    ] {
        assert_eq!(
            ResumeSessionRequest::try_new(agent.clone(), id.clone(), rejected.clone()),
            Err(InvalidValue::Field),
            "必须拒绝 {rejected:?}"
        );
    }
}

/// §3.6：`SessionRecoveryRecord` 的两个 `Option` 就是 `owned_session` 的两列（`None` = `NULL`）。
#[test]
fn session_recovery_record_reports_absent_columns_as_none() {
    let record = SessionRecoveryRecord {
        agent: agent_ref(),
        agent_session_id: None,
        workspace_cwd: Some("C:\\work".to_owned()),
    };
    assert!(record.agent_session_id.is_none());
    assert_eq!(record.workspace_cwd.as_deref(), Some("C:\\work"));
}

#[test]
fn cursors_are_compound_and_ordered() {
    let epoch = ServerEpoch::from_str(UUID_C).expect("epoch");
    let head = GlobalCursor::new(epoch.clone(), Sequence::new(5).expect("sequence"));
    assert_eq!(head.server_epoch, epoch);
    assert_eq!(head.global_sequence.get(), 5);
    let later = GlobalCursor::new(
        ServerEpoch::from_str(UUID_C).expect("epoch"),
        Sequence::new(6).expect("sequence"),
    );
    assert!(head < later, "same epoch compares by sequence");

    let origin = OriginCursor::new(
        OriginEpoch::from_str(UUID_B).expect("epoch"),
        Sequence::new(1).expect("sequence"),
    );
    assert_eq!(origin.origin_sequence.get(), 1);
}

// ---------------------------------------------------------------------------------------------
// §3.3/§5.8：消息通道、部分结果、媒体引用与表单 schema（WP2）
// ---------------------------------------------------------------------------------------------

fn attachment_id() -> AttachmentId {
    AttachmentId::from_str(UUID_A).expect("attachment id")
}

fn device_id() -> DeviceId {
    DeviceId::from_str(UUID_B).expect("device id")
}

#[test]
fn partial_results_keep_the_ongoing_state_distinct_from_failure() {
    // 流式输出中：partial 允许没有原因（`media.schema.json#/$defs/partialResult` 允许 reason: null）。
    let ongoing = PartialResult::try_new(MessageState::Partial, None).expect("ongoing partial");
    assert_eq!(ongoing.state, MessageState::Partial);
    assert_eq!(ongoing.reason, None);

    // 已完成不得带原因；失败/取消必须说明原因。
    assert!(PartialResult::try_new(MessageState::Completed, None).is_ok());
    assert!(
        PartialResult::try_new(MessageState::Completed, Some(PartialReason::AgentFailed)).is_err()
    );
    assert!(PartialResult::try_new(MessageState::Failed, None).is_err());
    assert!(PartialResult::try_new(MessageState::Cancelled, None).is_err());
    assert_eq!(
        PartialResult::try_new(MessageState::Failed, Some(PartialReason::AgentFailed))
            .expect("failed")
            .reason,
        Some(PartialReason::AgentFailed)
    );
    // token 词汇与 wire 逐值一致。
    assert_eq!(MessageChannel::Reply.as_str(), "reply");
    assert_eq!(MessageChannel::Thought.as_str(), "thought");
    assert_eq!(MessageState::Partial.as_str(), "partial");
    assert_eq!(PartialReason::TurnCancelled.as_str(), "turn_cancelled");
    assert_eq!(MediaKind::Image.as_str(), "image");
    assert_eq!(MediaAvailability::NotFetched.as_str(), "not_fetched");
}

#[test]
fn media_references_carry_no_location_and_bound_their_strings() {
    let reference = MediaReference::try_new(
        attachment_id(),
        session_id(),
        MediaKind::Image,
        "image/png",
        123,
        digest(),
        Some("diagram.png"),
        MediaAvailability::Available,
    )
    .expect("media reference");
    assert_eq!(reference.attachment(), &attachment_id());
    assert_eq!(reference.session(), &session_id());
    assert_eq!(reference.kind(), MediaKind::Image);
    assert_eq!(reference.byte_length(), 123);
    assert_eq!(reference.display_name(), Some("diagram.png"));
    assert_eq!(reference.availability(), MediaAvailability::Available);
    assert!(
        MediaReference::try_new(
            attachment_id(),
            session_id(),
            MediaKind::File,
            "",
            0,
            digest(),
            None,
            MediaAvailability::Available,
        )
        .is_err()
    );
    assert!(
        MediaReference::try_new(
            attachment_id(),
            session_id(),
            MediaKind::File,
            "text/plain",
            0,
            digest(),
            Some(""),
            MediaAvailability::Available,
        )
        .is_err()
    );
}

#[test]
fn attachment_subjects_are_only_devices_and_nodes() {
    let device = AttachmentSubject::from_actor(&Actor::Device {
        device: device_id(),
    })
    .expect("device subject");
    assert_eq!(device.kind(), "device");
    assert_eq!(device.id_text(), device_id().as_str());
    let node = NodeId::from_str(UUID_C).expect("node id");
    let node_subject = AttachmentSubject::from_actor(&Actor::Node {
        node: node.clone(),
        access_node: node.clone(),
    })
    .expect("node subject");
    assert_eq!(node_subject.kind(), "node");
    assert_eq!(node_subject.id_text(), node.as_str());
    assert!(AttachmentSubject::from_actor(&Actor::LocalCli).is_none());
}

fn elicitation_schema() -> ElicitationSchema {
    ElicitationSchema::new(
        r#"{"type":"object","required":["count","mode"],"properties":{"count":{"type":"integer","minimum":1,"maximum":10},"mode":{"type":"string","enum":["fast","safe"]},"note":{"type":"string","maxLength":5},"tags":{"type":"array","items":{"type":"string"},"maxLength":2}}}"#,
    )
    .expect("schema")
}

fn values(text: &str) -> ElicitationValues {
    ElicitationValues::parse_json(text).expect("values")
}

#[test]
fn elicitation_schema_validates_exact_integers_enums_ranges_and_required() {
    let schema = elicitation_schema();
    schema
        .validate_values(&values(r#"{"count":5,"mode":"fast"}"#))
        .expect("valid");
    // 精确整数：大于 2^53 的整数不经过浮点，范围比较仍然精确。
    schema
        .validate_values(&values(r#"{"count":9007199254740993,"mode":"safe"}"#))
        .expect_err("beyond maximum");
    for bad in [
        r#"{"mode":"fast"}"#,
        r#"{"count":0,"mode":"fast"}"#,
        r#"{"count":11,"mode":"fast"}"#,
        r#"{"count":5,"mode":"slow"}"#,
        r#"{"count":5.0,"mode":"fast"}"#,
        r#"{"count":5,"mode":"fast","note":"toolong"}"#,
        r#"{"count":5,"mode":"fast","unknown":true}"#,
        r#"{"count":5,"mode":"fast","tags":["a","b","c"]}"#,
    ] {
        assert!(
            schema.validate_values(&values(bad)).is_err(),
            "must reject {bad}"
        );
    }
    assert!(
        ElicitationValues::parse_json(r#"[{"count":5}]"#).is_err(),
        "top-level array is not an elicitation content"
    );
    // 无 required 时 null 合法；有 required 时 null 不可代替填写。
    let optional =
        ElicitationSchema::new(r#"{"type":"object","properties":{"flag":{"type":"boolean"}}}"#)
            .expect("schema");
    optional
        .validate_values(&ElicitationValues::null())
        .expect("null without required");
    assert!(schema.validate_values(&ElicitationValues::null()).is_err());
    // 空对象与 null 是不同输入：required 下空对象同样不可用。
    assert!(
        schema
            .validate_values(&ElicitationValues::empty_object())
            .is_err()
    );
}

#[test]
fn elicitation_numeric_bounds_are_exact_beyond_2_53() {
    // 2^53 附近的整数边界必须用精确整数比较：先转 f64 会让 9007199254740992 与
    // 9007199254740993 变成同一个值。
    let pinned = ElicitationSchema::new(
        r#"{"type":"object","properties":{"n":{"type":"integer","minimum":9007199254740993,"maximum":9007199254740993}}}"#,
    )
    .expect("schema");
    pinned
        .validate_values(&values(r#"{"n":9007199254740993}"#))
        .expect("the exact bound is inside");
    for outside in [
        r#"{"n":9007199254740992}"#,
        r#"{"n":9007199254740994}"#,
        r#"{"n":9007199254740995}"#,
    ] {
        assert!(
            pinned.validate_values(&values(outside)).is_err(),
            "must reject {outside}"
        );
    }

    let maximum = ElicitationSchema::new(
        r#"{"type":"object","properties":{"n":{"type":"integer","maximum":9007199254740992}}}"#,
    )
    .expect("schema");
    assert!(
        maximum
            .validate_values(&values(r#"{"n":9007199254740993}"#))
            .is_err(),
        "2^53 + 1 不得因 f64 舍入而被当作 2^53"
    );
    maximum
        .validate_values(&values(r#"{"n":9007199254740992}"#))
        .expect("the bound itself is accepted");

    let minimum = ElicitationSchema::new(
        r#"{"type":"object","properties":{"n":{"type":"integer","minimum":9007199254740993}}}"#,
    )
    .expect("schema");
    assert!(
        minimum
            .validate_values(&values(r#"{"n":9007199254740992}"#))
            .is_err(),
        "2^53 不得因 f64 舍入而被当作 2^53 + 1"
    );

    // 非整数边界仍回退浮点比较（`number` 类型）。
    let float = ElicitationSchema::new(
        r#"{"type":"object","properties":{"n":{"type":"number","maximum":1.5}}}"#,
    )
    .expect("schema");
    float
        .validate_values(&values(r#"{"n":1.5}"#))
        .expect("equal bound");
    assert!(
        float
            .validate_values(&values(r#"{"n":1.5000001}"#))
            .is_err()
    );
}

#[test]
fn elicitation_schema_rejects_unrepresentable_required_types() {
    // 未知类型（object）对必需字段一律拒绝。
    let schema = ElicitationSchema::new(
        r#"{"type":"object","required":["x"],"properties":{"x":{"type":"object"}}}"#,
    )
    .expect("schema");
    let value = ElicitationValues::from_fields([(
        "x".to_owned(),
        ElicitationContentValue::unknown(r#"{"nested":true}"#).expect("unknown"),
    )])
    .expect("values");
    assert!(schema.validate_values(&value).is_err());

    // 无 schema 的 elicitation 只接受 null / 空对象，不凭空接受任意字段。
    let interaction = EffectiveInteraction::try_new(
        InteractionId::from_str(UUID_A).expect("interaction"),
        session_id(),
        InteractionKind::Elicitation,
        Version::new(1),
        None,
        Vec::new(),
        None,
    )
    .expect("effective");
    interaction
        .validate_submission(&ElicitationValues::null())
        .expect("null");
    interaction
        .validate_submission(&ElicitationValues::empty_object())
        .expect("empty");
    assert!(
        interaction
            .validate_submission(&values(r#"{"x":1}"#))
            .is_err()
    );
    // 0 不是有效请求版本。
    assert!(
        EffectiveInteraction::try_new(
            InteractionId::from_str(UUID_A).expect("interaction"),
            session_id(),
            InteractionKind::Permission,
            Version::new(0),
            None,
            Vec::new(),
            None,
        )
        .is_err()
    );
}

#[test]
fn message_records_store_the_folded_content_as_json_text() {
    let content = JsonValueText::new(r#"[{"type":"text","text":"hi"}]"#).expect("content");
    let record = MessageRecord::try_new(
        MessageId::from_str(UUID_A).expect("message"),
        session_id(),
        Some(turn_id()),
        MessageChannel::Thought,
        PartialResult::try_new(MessageState::Cancelled, Some(PartialReason::TurnCancelled))
            .expect("result"),
        content.clone(),
        Vec::new(),
        ts(T0),
        ts(T1),
    )
    .expect("message record");
    assert_eq!(record.channel, MessageChannel::Thought);
    assert_eq!(record.content.as_str(), content.as_str());
    assert_eq!(record.result.reason, Some(PartialReason::TurnCancelled));
}
