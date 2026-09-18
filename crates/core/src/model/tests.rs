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

fn local_session(state: SessionState, closed_at: Option<&str>) -> Session {
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
        closed_at.map(ts),
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
    assert!(ExportId::new("acp.raw-payload_v1").is_ok());
    assert_eq!(ExportId::new("has space"), Err(InvalidValue::ExportId));
    assert_eq!(ExportId::new(""), Err(InvalidValue::ExportId));
    assert_eq!(ExportId::new(&"a".repeat(129)), Err(InvalidValue::ExportId));

    assert!(ImportId::new("office-1").is_ok());
    assert_eq!(ImportId::new("office/1"), Err(InvalidValue::ImportId));

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

    assert_eq!(
        EntityRef::Export(ExportId::new("exp").expect("export id")).to_string(),
        "export:exp"
    );
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
fn session_state_machine_is_terminal_aware() {
    use SessionState::*;
    assert!(!Idle.is_terminal());
    assert!(Failed.is_terminal());
    assert!(Closed.is_terminal());

    assert!(Idle.can_transition_to(Running));
    assert!(Running.can_transition_to(WaitingPermission));
    assert!(WaitingPermission.can_transition_to(Running));
    assert!(Running.can_transition_to(Closed));
    assert!(!Idle.can_transition_to(WaitingInput));
    assert!(!Running.can_transition_to(Queued));

    let mut session = local_session(Idle, None);
    session
        .transition(Running, &ts(T0))
        .expect("idle -> running");
    assert_eq!(session.state(), Running);
    session
        .transition(WaitingPermission, &ts(T1))
        .expect("running -> waiting_permission");
    assert_eq!(session.updated_at().as_str(), T1);
    session
        .transition(Closed, &ts(T2))
        .expect("waiting_permission -> closed");
    assert_eq!(session.closed_at().map(Timestamp::as_str), Some(T2));
    // 终态无出边。
    assert_eq!(
        session.transition(Idle, &ts(T2)),
        Err(InvalidValue::StateTransition)
    );
    // 非法转换同样被拒。
    let mut idle = local_session(Idle, None);
    assert_eq!(
        idle.transition(WaitingInput, &ts(T0)),
        Err(InvalidValue::StateTransition)
    );
}

#[test]
fn session_constructor_checks_cross_field_invariants() {
    let created = ts(T0);
    let reference = OwnedSessionRef::new(session_id());
    // closed_at 与 state 必须一致。
    assert_eq!(
        Session::try_new(
            session_id(),
            reference.clone(),
            None,
            agent_ref(),
            SessionState::Closed,
            ResourceOrigin::Local,
            None,
            Version::from(1),
            created.clone(),
            created.clone(),
            None,
        ),
        Err(InvalidValue::Field)
    );
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
            None,
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
            None,
        ),
        Err(InvalidValue::TooLong { max: 512 })
    );

    let summary = local_session(SessionState::Running, None).summary();
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
    let scopes = ScopeSet::try_from_iter(["session.prompt"]).expect("scopes");
    let actor = Actor::Device {
        device: DeviceId::from_str(UUID_B).expect("device"),
        scopes,
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

    let owner = NodeId::from_str(UUID_C).expect("node");
    let remote = RemoteSessionRef::new(
        owner.clone(),
        ExportId::new("exp").expect("export"),
        session_id(),
    );
    let delivery = CommittedDelivery::Imported {
        session: remote,
        origin: OriginEventRef::new(owner, epoch, EventId::from_str(UUID_B).expect("event id")),
        origin_sequence: sequence,
        local_sequence: LocalCursor::from(1),
        event_type: EventType::new("agent.message.delta").expect("event type"),
        payload_digest: digest(),
        payload: Some(EventPayload::new(view(r#"{"text":"x"}"#), None)),
    };
    delivery.validate().expect("imported delivery");

    // origin 必须属于该会话的 Owner。
    let foreign = CommittedDelivery::Imported {
        session: RemoteSessionRef::new(
            NodeId::from_str(UUID_A).expect("node"),
            ExportId::new("exp").expect("export"),
            session_id(),
        ),
        origin: OriginEventRef::new(
            NodeId::from_str(UUID_B).expect("node"),
            OriginEpoch::from_str(UUID_C).expect("epoch"),
            EventId::from_str(UUID_B).expect("event id"),
        ),
        origin_sequence: sequence,
        local_sequence: LocalCursor::from(1),
        event_type: EventType::new("agent.message.delta").expect("event type"),
        payload_digest: digest(),
        payload: None,
    };
    assert_eq!(foreign.validate(), Err(InvalidValue::Field));
    assert!(CommittedDelivery::Owned(event).validate().is_ok());

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
}

// ---------------------------------------------------------------- §3.5 身份/信任/审计

#[test]
fn scope_and_grant_sets_validate_and_dedup() {
    let mut scopes = ScopeSet::empty();
    assert!(scopes.insert("session.prompt").expect("insert"));
    assert!(!scopes.insert("session.prompt").expect("re-insert"));
    assert_eq!(scopes.len(), 1);
    assert!(scopes.contains("session.prompt"));
    assert_eq!(
        scopes.insert("Session.Prompt"),
        Err(InvalidValue::ScopeName)
    );
    assert_eq!(
        ScopeSet::try_from_iter(["a b"]),
        Err(InvalidValue::ScopeName)
    );
    assert!(ScopeSet::default().is_empty());
    let set = ScopeSet::try_from_iter(["session.list", "session.read"]).expect("scopes");
    assert_eq!(
        set.iter().collect::<Vec<_>>(),
        vec!["session.list", "session.read"]
    );

    let grants =
        GrantSet::try_from_iter(["grant.observe", "grant.configure-session"]).expect("grants");
    assert_eq!(grants.len(), 2);
    assert_eq!(
        GrantSet::try_from_iter(["observe"]),
        Err(InvalidValue::GrantName)
    );
    assert_eq!(
        GrantSet::try_from_iter(["grant."]),
        Err(InvalidValue::GrantName)
    );
    assert!(scopes.insert("local.device.manage").is_ok());
}

#[test]
fn actor_exposes_the_storage_identity_columns() {
    let device = DeviceId::from_str(UUID_B).expect("device");
    let actor = Actor::Device {
        device: device.clone(),
        scopes: ScopeSet::try_from_iter(["session.list"]).expect("scopes"),
    };
    assert_eq!(actor.kind(), ActorKind::Device);
    assert_eq!(actor.kind().as_str(), "device");
    assert_eq!(actor.id_text(), UUID_B);
    assert_eq!(actor.device_id(), Some(&device));
    assert_eq!(actor.scopes().map(ScopeSet::len), Some(1));

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
    assert!(node_actor.scopes().is_none());

    let cli = Actor::LocalCli;
    assert!(cli.is_local_cli());
    assert_eq!(cli.kind(), ActorKind::Cli);
    assert_eq!(cli.id_text(), "cli");
    assert_eq!(ActorKind::from_str("cli").expect("kind"), ActorKind::Cli);
    assert_eq!(ActorKind::from_str("local_cli"), Err(InvalidValue::Field));
}

#[test]
fn device_record_ties_revocation_state_to_timestamp() {
    let device = DeviceId::from_str(UUID_B).expect("device");
    let record = DeviceRecord::try_new(
        device.clone(),
        "Zhang's Phone",
        fingerprint(),
        ScopeSet::try_from_iter(["session.list"]).expect("scopes"),
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
            ScopeSet::empty(),
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
            ScopeSet::empty(),
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
            GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
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
    assert_eq!(owner.grants().len(), 1);
}

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
            ScopeSet::empty(),
            GrantSet::empty(),
            digest(),
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
    // 设备配对不得携带 grants。
    assert_eq!(
        PairingRecord::try_new(
            id,
            PairingTarget::Device,
            PairingState::Created,
            None,
            ScopeSet::empty(),
            GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
            digest(),
            ts(T0),
            ts(T2),
            None,
            None,
            None,
        ),
        Err(InvalidValue::Field)
    );
}

#[test]
fn pairing_claim_and_peer_are_typed_and_consistent() {
    let peer = PairingPeer::try_new(
        PeerIdentity::Node(NodeId::from_str(UUID_C).expect("node")),
        "Office Access",
        fingerprint(),
        nonce(),
    )
    .expect("peer");
    assert_eq!(peer.id().kind(), "node");
    assert_eq!(peer.display_name(), "Office Access");
    assert_eq!(peer.client_nonce(), &nonce());

    let pairing = PairingId::from_str(UUID_B).expect("pairing");
    let claim = PairingClaim::try_new(
        pairing.clone(),
        peer.clone(),
        ScopeSet::empty(),
        GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
    )
    .expect("node claim");
    assert_eq!(claim.pairing(), &pairing);
    assert_eq!(claim.requested_grants().len(), 1);
    // 节点配对不得携带 scopes。
    assert_eq!(
        PairingClaim::try_new(
            pairing.clone(),
            peer.clone(),
            ScopeSet::try_from_iter(["session.list"]).expect("scopes"),
            GrantSet::empty(),
        ),
        Err(InvalidValue::Field)
    );

    let device_peer = PairingPeer::try_new(
        PeerIdentity::Device(DeviceId::from_str(UUID_B).expect("device")),
        "Phone",
        fingerprint(),
        nonce(),
    )
    .expect("peer");
    assert!(
        PairingClaim::try_new(
            pairing,
            device_peer,
            ScopeSet::try_from_iter(["session.list"]).expect("scopes"),
            GrantSet::empty()
        )
        .is_ok()
    );

    let settlement = PairingSettlement::approved(
        ScopeSet::try_from_iter(["session.list"]).expect("scopes"),
        GrantSet::empty(),
    );
    assert!(settlement.is_approved());
    settlement.validate().expect("settlement");
    let rejected = PairingSettlement::rejected(Some("user said no")).expect("rejected");
    assert!(!rejected.is_approved());
    assert_eq!(
        PairingSettlement::rejected(Some(&"r".repeat(257))),
        Err(InvalidValue::TooLong { max: 256 })
    );
}

#[test]
fn audit_record_carries_no_content_and_closed_action_set() {
    assert_eq!(AuditAction::ALL.len(), 15);
    assert_eq!(AuditAction::PairingCreated.as_str(), "pairing.created");
    assert_eq!(
        AuditAction::from_str("storage.integrity_failed").expect("action"),
        AuditAction::StorageIntegrityFailed
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
fn export_record_checks_defaults_and_uniqueness() {
    let alias = WorkspaceAlias::new("work-pc").expect("alias");
    let other = WorkspaceAlias::new("second").expect("alias");
    let template_workspace = alias.clone();
    let template = ExportTemplate::try_new(
        TemplateId::new("tpl.1").expect("template id"),
        "Default",
        template_workspace,
        vec![
            TemplateParam::try_new(
                ParamName::new("branch").expect("param"),
                TemplateParamType::String,
                true,
                Some("^[a-z]+$".to_owned()),
                None,
            )
            .expect("param"),
        ],
    )
    .expect("template");

    let build = |aliases: Vec<WorkspaceAliasEntry>,
                 default_alias: WorkspaceAlias,
                 templates: Vec<ExportTemplate>,
                 default_template: TemplateId| {
        ExportRecord::try_new(
            ExportId::new("exp").expect("export id"),
            "My Export",
            vec![AgentId::new("codex").expect("agent")],
            aliases,
            default_alias,
            templates,
            default_template,
            GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
            CachePolicy::NoContentCache,
            ts(T0),
            None,
        )
    };

    let entry = WorkspaceAliasEntry::try_new(alias.clone(), "Work PC").expect("entry");
    let record = build(
        vec![entry.clone()],
        alias.clone(),
        vec![template.clone()],
        TemplateId::new("tpl.1").expect("template id"),
    )
    .expect("export record");
    assert_eq!(record.export_id().as_str(), "exp");
    assert_eq!(record.agent_ids().len(), 1);
    assert_eq!(record.workspace_aliases().len(), 1);
    assert_eq!(record.default_workspace_alias(), &alias);
    assert_eq!(record.templates().len(), 1);
    assert_eq!(record.cache_policy(), CachePolicy::NoContentCache);
    assert!(!record.is_revoked());
    assert!(record.revoked_at().is_none());

    // 空集合、重复项、默认值不在集合内都必须失败。
    assert_eq!(
        build(
            Vec::new(),
            alias.clone(),
            vec![template.clone()],
            TemplateId::new("tpl.1").expect("id")
        ),
        Err(InvalidValue::Empty)
    );
    assert_eq!(
        build(
            vec![entry.clone(), entry.clone()],
            alias.clone(),
            vec![template.clone()],
            TemplateId::new("tpl.1").expect("id")
        ),
        Err(InvalidValue::Field)
    );
    assert_eq!(
        build(
            vec![entry.clone()],
            other.clone(),
            vec![template.clone()],
            TemplateId::new("tpl.1").expect("id")
        ),
        Err(InvalidValue::Field)
    );
    assert_eq!(
        build(
            vec![entry.clone()],
            alias.clone(),
            vec![template.clone()],
            TemplateId::new("tpl.2").expect("id")
        ),
        Err(InvalidValue::Field)
    );
    // template 的 workspaceAlias 必须已声明。
    let stray = ExportTemplate::try_new(
        TemplateId::new("tpl.2").expect("id"),
        "Stray",
        other,
        Vec::new(),
    )
    .expect("template");
    assert_eq!(
        build(
            vec![entry],
            alias.clone(),
            vec![stray],
            TemplateId::new("tpl.2").expect("id")
        ),
        Err(InvalidValue::Field)
    );
    // 参数名重复。
    let param = TemplateParam::try_new(
        ParamName::new("p").expect("param"),
        TemplateParamType::Boolean,
        false,
        None,
        None,
    )
    .expect("param");
    assert_eq!(
        ExportTemplate::try_new(
            TemplateId::new("tpl.3").expect("id"),
            "Dup",
            alias,
            vec![param.clone(), param],
        ),
        Err(InvalidValue::Field)
    );
    // pattern 只允许 string；enum 成员类型必须一致且非空。
    assert_eq!(
        TemplateParam::try_new(
            ParamName::new("p").expect("param"),
            TemplateParamType::Integer,
            false,
            Some("x".to_owned()),
            None,
        ),
        Err(InvalidValue::Field)
    );
    assert_eq!(
        TemplateParam::try_new(
            ParamName::new("p").expect("param"),
            TemplateParamType::String,
            false,
            None,
            Some(vec![TemplateParamValue::Integer(1)]),
        ),
        Err(InvalidValue::Field)
    );
    assert_eq!(
        TemplateParam::try_new(
            ParamName::new("p").expect("param"),
            TemplateParamType::String,
            false,
            None,
            Some(Vec::new()),
        ),
        Err(InvalidValue::Empty)
    );
}

#[test]
fn import_record_requires_wss_endpoint_and_unique_exports() {
    let export = ExportId::new("exp").expect("export id");
    let record = ImportRecord::try_new(
        ImportId::new("office").expect("import id"),
        "wss://owner.example/node-link/v1",
        NodeId::from_str(UUID_C).expect("node"),
        vec![export.clone()],
        GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
    )
    .expect("import record");
    assert_eq!(record.import_id().as_str(), "office");
    assert_eq!(record.owner_node_id().as_str(), UUID_C);
    assert_eq!(record.export_ids(), std::slice::from_ref(&export));
    assert_eq!(record.grants().len(), 1);

    assert_eq!(
        ImportRecord::try_new(
            ImportId::new("office").expect("import id"),
            "https://owner.example",
            NodeId::from_str(UUID_C).expect("node"),
            vec![export.clone()],
            GrantSet::empty(),
        ),
        Err(InvalidValue::Endpoint)
    );
    assert_eq!(
        ImportRecord::try_new(
            ImportId::new("office").expect("import id"),
            "wss://owner.example",
            NodeId::from_str(UUID_C).expect("node"),
            vec![export.clone(), export],
            GrantSet::empty(),
        ),
        Err(InvalidValue::Field)
    );
    assert_eq!(
        ImportRecord::try_new(
            ImportId::new("office").expect("import id"),
            "wss://owner.example",
            NodeId::from_str(UUID_C).expect("node"),
            Vec::new(),
            GrantSet::empty(),
        ),
        Err(InvalidValue::Empty)
    );
}

// ---------------------------------------------------------------- §3.6 后端/能力

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
        export_id: ExportId::new("exp").expect("export"),
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
    assert_eq!(ConflictKind::ALL.len(), 6);
    assert_eq!(UnavailableKind::ALL.len(), 6);
    assert_eq!(ConflictKind::AlreadyResolved.as_str(), "already_resolved");
    assert_eq!(
        UnavailableKind::RemoteUnavailable.as_str(),
        "remote_unavailable"
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
