//! R27 + R34：`session.resume` 的 **wire 层**契约（命令登记、payload 空对象规则、session 范围规则）。
//!
//! 入口是 `node_link_protocol` 的类型化解码 + 只读静态合同文件（`compatibility/commands/v1/commands.json`、
//! `schemas/node-link/v1/command.schema.json`、`fixtures/node-link/v1/manifest.json`）。
//!
//! **与既有门禁的关系**：命令词表的四处一致由 `npm run check`（`check:command-catalog`）承担、
//! schema 与 Rust 类型的逐条相等由既有 `tests/schema_drift.rs` 承担。本文件**不重复**整套门禁，只补
//! `session.resume` 特有的三条断言：
//!
//! 1. 该命令在机器词表里的 `kind`/`grant`/`pack`/`transport`/`delivery` 取值（R27：按
//!    `grant.remote-work` 判定授权、不新增授权维度；D6：`pack: null` 只落在 node_link-only 命令上）；
//! 2. `payload` 恒为 `{}`——**任何**键都在解码边界被拒（R34）；
//! 3. `expectedVersion` 必须 `null`、`sessionRef`/`attachmentId`/`attachmentGeneration` 必须非 `null`
//!    （R27/R34 的 session-scoped 与版本规则）。
//!
//! **前置数据**：全部来自既有 fixture 与合同文件（只读），**不新增** fixture 目录内容
//! （`fixtures/node-link/v1/manifest.json` 的计数由既有门禁双向断言）。

mod support;

use node_link_protocol::command::{CommandName, CommandPayload, CommandSubmit};
use node_link_protocol::envelope::Envelope;
use serde_json::Value;
use support::repo_path;

/// WP2 落下的合法夹具（只读复用）。
const RESUME_SUBMIT_FIXTURE: &str =
    "fixtures/node-link/v1/valid/command-submit-session-resume.json";
const RESUME_TERMINAL_FIXTURE: &str =
    "fixtures/node-link/v1/valid/command-terminal-session-resume-completed.json";
/// 命令机器词表。
const COMMAND_CATALOG: &str = "compatibility/commands/v1/commands.json";

/// 夹具里的 `body`（信封其余字段对断言无影响）。
fn fixture_body(relative: &str) -> Value {
    support::read_json(&repo_path(relative))["body"].clone()
}

/// 用给定 `command.submit` body 组一条完整信封并解码出类型化 `CommandSubmit`。
///
/// 走的是**真实信封入口** `Envelope::decode`（而不是直接 `serde_json::from_value`），
/// 因此 session 范围规则与 payload 空对象规则都在同一层被验证。
fn decode_submit(body: &Value) -> Result<CommandSubmit, String> {
    let text = serde_json::json!({
        "protocolVersion": 1,
        "type": "command.submit",
        "messageId": "c1d2e3f4-a506-4718-b9ca-dbecfda0b1c2",
        "connectionId": "40414243-4445-4647-8849-4a4b4c4d4e4f",
        "connectionSequence": "6",
        "body": body.clone(),
    })
    .to_string();
    let envelope = Envelope::decode(&text).map_err(|error| error.to_string())?;
    serde_json::from_str(envelope.body().get()).map_err(|error| error.to_string())
}

/// 一个合法的 `session.resume` `command.submit` body（夹具原文，必要时改一个字段）。
fn valid_submit() -> Value {
    fixture_body(RESUME_SUBMIT_FIXTURE)
}

/// R27：`session.resume` 在机器词表里的登记必须与 wire 一致，且**授权维度不新增**。
#[test]
fn the_command_catalog_row_matches_the_wire_contract() {
    let catalog = support::read_json(&repo_path(COMMAND_CATALOG));
    let row = catalog["commands"]
        .as_array()
        .expect("commands 数组")
        .iter()
        .find(|row| row["name"] == Value::String("session.resume".to_owned()))
        .expect("命令目录必须登记 session.resume")
        .clone();

    assert_eq!(row["kind"], Value::String("mutation".to_owned()));
    assert_eq!(
        row["grant"],
        Value::String("grant.remote-work".to_owned()),
        "R27：按 grant.remote-work 判定授权，不新增授权维度"
    );
    assert_eq!(
        row["scope"],
        Value::String("session.resume".to_owned()),
        "scope 是命令名本身（不是新的一类权限）"
    );
    assert!(
        row["pack"].is_null(),
        "D6：transport 不含 sync 的命令允许 pack = null（本命令只在 node_link 上可达）"
    );
    assert_eq!(
        row["transport"],
        serde_json::json!(["node_link"]),
        "session.resume 只经 Node Link 接受（不得出现在 sync transport 上）"
    );
    assert_eq!(row["delivery"], Value::String("conditional_mvp".to_owned()));

    // `grant.remote-work` 的会员表必须同时包含 `session.create` 与 `session.resume`。
    let memberships = catalog["grants"]
        .as_object()
        .expect("grants 表")
        .get("grant.remote-work")
        .and_then(Value::as_array)
        .expect("grant.remote-work 会员")
        .clone();
    assert!(
        memberships.contains(&Value::String("session.resume".to_owned())),
        "grant.remote-work 必须覆盖 session.resume：{memberships:?}"
    );

    // Rust 侧的枚举与词表不得漂移。
    assert_eq!(CommandName::SessionResume.as_str(), "session.resume");
}

/// R34：合法夹具解码成 `SessionResume` 变体，`payload` 是空对象（不携带任何语义）。
#[test]
fn a_well_formed_resume_submit_decodes_into_the_session_resume_variant() {
    let submit = decode_submit(&valid_submit()).expect("合法夹具必须能解码");
    assert_eq!(submit.command, CommandName::SessionResume);
    assert!(
        matches!(submit.payload, CommandPayload::SessionResume(_)),
        "payload 必须解码成 SessionResume 变体（而不是开放对象）"
    );
    assert_eq!(
        submit.expected_version.as_ref(),
        None,
        "expectedVersion 必须为 null"
    );
    assert!(
        submit.session_ref.as_ref().is_some(),
        "session.resume 是 session-scoped 命令，必须携带非 null 的 sessionRef"
    );
    assert!(submit.attachment_id.as_ref().is_some());
    assert!(submit.attachment_generation.as_ref().is_some());
    assert!(
        CommandName::SessionResume.requires_session_attachment(),
        "session.resume 必须要求当前 attachment 代际（R27）"
    );
    assert!(
        !CommandName::SessionResume.requires_expected_version(),
        "session.resume 的 expectedVersion 恒为 null，因此不要求它存在（R34）"
    );
}

/// R34：`payload` 携带**任何**键都在解码边界被拒（不进入 typed decode 之后的业务层）。
///
/// 判别力：若实现放宽成「忽略未知键」，这三条都会通过。
#[test]
fn any_resume_payload_field_is_rejected_at_the_decode_boundary() {
    for field in ["cwd", "agentId", "unknown", "workspaceAlias"] {
        let mut body = valid_submit();
        body["payload"] = serde_json::json!({ field: "x" });
        assert!(
            decode_submit(&body).is_err(),
            "payload 携带 `{field}` 必须被拒（R34：出现任何键都拒绝）"
        );
    }
    // 空对象是唯一合法形状。
    assert!(
        decode_submit(&valid_submit()).is_ok(),
        "payload 恒为 `{{}}`"
    );
}

/// R27/R34：session 范围规则——`sessionRef` 为 `null`、`expectedVersion` 非 `null` 都必须被拒。
#[test]
fn session_scoped_and_version_rules_are_enforced_at_the_decode_boundary() {
    let mut no_ref = valid_submit();
    no_ref["sessionRef"] = Value::Null;
    assert!(
        decode_submit(&no_ref).is_err(),
        "session-scoped 命令的 sessionRef 必须非 null（R27）"
    );

    let mut no_attachment = valid_submit();
    no_attachment["attachmentId"] = Value::Null;
    assert!(
        decode_submit(&no_attachment).is_err(),
        "session-scoped 命令必须携带当前 attachmentId（R27）"
    );

    let mut no_generation = valid_submit();
    no_generation["attachmentGeneration"] = Value::Null;
    assert!(
        decode_submit(&no_generation).is_err(),
        "session-scoped 命令必须携带当前 attachmentGeneration（R27）"
    );

    let mut versioned = valid_submit();
    versioned["expectedVersion"] = Value::String("3".to_owned());
    assert!(
        decode_submit(&versioned).is_err(),
        "session.resume 的 expectedVersion MUST 为 null（R34）"
    );
}

/// R34 + R27：`completed` 终态的结果是 `SessionResumeResult`（含 `remoteSessionRef` 与 `sessionMeta`），
/// **不是**空对象 `{}`。
#[test]
fn a_completed_resume_terminal_carries_the_named_result_payload() {
    let terminal = fixture_body(RESUME_TERMINAL_FIXTURE);
    assert_eq!(
        terminal["command"],
        Value::String("session.resume".to_owned())
    );
    assert_eq!(
        terminal["terminal"]["status"],
        Value::String("completed".to_owned())
    );
    let result = &terminal["terminal"]["result"];
    assert!(
        result.is_object() && !result.as_object().expect("result").is_empty(),
        "completed 的 result 必须是非空 object（R27/§12.5）"
    );
    assert!(
        result["remoteSessionRef"].is_object(),
        "R34：SessionResumeResult 必须含 remoteSessionRef：{result}"
    );
    assert!(
        result["sessionMeta"].is_object(),
        "R34：SessionResumeResult 必须含 sessionMeta：{result}"
    );
    assert!(
        terminal["terminal"]["error"].is_null(),
        "completed 不带 error（R27）"
    );
    assert!(
        terminal["terminal"]["terminalEventId"].is_string(),
        "command.status 重查返回的终态必须带 terminalEventId（§12.5）"
    );
}
