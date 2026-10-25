//! `core.session-preparation.v1` 的 Rust 侧独立判定：准备凭据、`session.start` 的闭合判别联合、
//! 准备结果的闭合 status 分支、附件准备 target 的互斥，以及三个 `preparation.*` 错误码的
//! retryable 语义（`docs/SYNC_PROTOCOL.md` §3.4/§6.3/§11.9/§12.2）。
//!
//! 用共享 fixture 驱动：`fixtures/sync/v1/valid/*` 必须解析成功，同一内容只要混入相邻分支的字段
//! 就必须在解码期失败。fixture 由 `manifest.json` 登记，`scripts/check-contract-assets.mjs` 与
//! `check-schema-fixtures.mjs` 在 Node 侧执行同一条规则。

mod support;

use sync_protocol::attachment::{self, BeginBody, FrameBody, ManifestBeginBody};
use sync_protocol::command::{self, SessionStartPayload};
use sync_protocol::error::ErrorCode;
use sync_protocol::sync::{PreparationOutcome, SessionPreparationResult};

const ROOT: &str = "fixtures/sync/v1/";

fn fixture_text(relative: &str) -> String {
    support::read_text(&support::repo_path(&format!("{ROOT}{relative}")))
}

fn fixture_value(relative: &str) -> serde_json::Value {
    serde_json::from_str(&fixture_text(relative)).expect("fixture 必须是合法 JSON")
}

fn body_text(relative: &str) -> String {
    fixture_value(relative)["body"].to_string()
}

#[test]
fn initial_command_carries_the_closed_receipt_claims() {
    let command: command::Command =
        serde_json::from_str(&body_text("valid/command-session-start-initial.json"))
            .expect("initial session.start 必须解码");
    assert_eq!(command.command, command::CommandName::SessionStart);
    assert!(
        command.session_id.is_some(),
        "session.start 必须携带 sessionId"
    );

    let command::CommandPayload::SessionStart(SessionStartPayload::Initial {
        receipt,
        generation,
        content,
        submission_id,
        ..
    }) = &command.payload
    else {
        panic!("payload 必须是 kind=initial");
    };

    // 凭据 claims 与 §6.3 的 transcript 向量逐字段一致。
    assert_eq!(receipt.claims.version.get(), 1);
    assert_eq!(
        receipt.claims.session_id.as_str(),
        "6f1c2d3e-4a5b-4c6d-8e7f-901122334455"
    );
    assert_eq!(
        receipt.claims.device_id.as_str(),
        "2ae1c07c-9242-46e9-a9d2-4ec58c130f49"
    );
    assert_eq!(
        receipt.claims.store_epoch.as_str(),
        "9c6a1e34-5b7d-4f2a-9d18-6e0b2c3d4e5f"
    );
    assert_eq!(receipt.claims.agent_id.as_str(), "codex");
    assert_eq!(receipt.claims.workspace_source.as_str(), "workspacePath");
    assert_eq!(generation.as_str(), "1");
    assert_eq!(content.as_slice().len(), 2, "有序 content 原样保留");
    assert!(submission_id.is_some());

    // 往返逐字节保真（不缺键、不多键）。
    let encoded = serde_json::to_string(&command).expect("可序列化");
    let decoded: command::Command = serde_json::from_str(&encoded).expect("可再解码");
    assert_eq!(decoded, command);
}

#[test]
fn continue_command_is_closed() {
    let command: command::Command =
        serde_json::from_str(&body_text("valid/command-session-start-continue.json"))
            .expect("continue session.start 必须解码");
    let command::CommandPayload::SessionStart(SessionStartPayload::Continue {
        first_submission_id,
        retry_of_request_id,
        mode,
        config_options,
    }) = &command.payload
    else {
        panic!("payload 必须是 kind=continue");
    };
    assert_eq!(
        first_submission_id.as_str(),
        "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d"
    );
    assert_eq!(
        retry_of_request_id.as_str(),
        "1b2c3d4e-5f6a-4b7c-8d9e-0f1a2b3c4d5e"
    );
    assert_eq!(mode.as_ref().map(|value| value.as_str()), Some("compact"));
    assert!(config_options.is_none());

    // 混入首条正文或准备凭据字段必须被拒（闭合分支，不是「忽略多余键」）。
    for injected in [
        r#", "content": [{"type": "text", "text": "again"}]"#,
        r#", "receipt": null"#,
        r#", "runtimeEpoch": "3f2a1b0c-9d8e-4f7a-8b6c-5d4e3f2a1b0c""#,
    ] {
        let text = body_text("valid/command-session-start-continue.json");
        let mangled = text.replacen('}', &format!("{injected}}}"), 1);
        assert!(
            serde_json::from_str::<command::Command>(&mangled).is_err(),
            "continue 分支不得接受 {injected}"
        );
    }
}

#[test]
fn initial_command_rejects_first_submission_id() {
    let text = body_text("valid/command-session-start-initial.json");
    let mangled = text.replacen(
        r#""kind":"initial""#,
        r#""kind":"initial","firstSubmissionId":"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d""#,
        1,
    );
    assert!(
        serde_json::from_str::<command::Command>(&mangled).is_err(),
        "firstSubmissionId 由服务端生成，initial 不得携带"
    );
}

#[test]
fn preparation_results_are_closed_unions() {
    let committed: SessionPreparationResult = serde_json::from_str(&body_text(
        "valid/sync-session-preparation-result-committed.json",
    ))
    .expect("committed 结果必须解码");
    let PreparationOutcome::Committed {
        first_submission_id,
        first_request_id,
        current_request_id,
        first_message_id,
    } = &committed.outcome
    else {
        panic!("status 必须是 committed");
    };
    assert_eq!(
        first_submission_id.as_str(),
        "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d"
    );
    assert_eq!(
        first_request_id.as_str(),
        "1b2c3d4e-5f6a-4b7c-8d9e-0f1a2b3c4d5e"
    );
    assert_eq!(
        current_request_id.as_str(),
        "2c3d4e5f-6a7b-4c8d-9e0f-1a2b3c4d5e6f"
    );
    assert_eq!(
        first_message_id.as_str(),
        "3d4e5f6a-7b8c-4d9e-8f0a-1b2c3d4e5f6a"
    );

    // committed 不得携带临时代/租约（同一字段在相邻分支合法，因此只能在解码期拒绝）。
    let text = body_text("valid/sync-session-preparation-result-committed.json");
    let mangled = text.replacen('}', r#", "leaseExpiresAt": "2026-10-22T08:00:30.000Z"}"#, 1);
    assert!(
        serde_json::from_str::<SessionPreparationResult>(&mangled).is_err(),
        "committed 分支不得携带 leaseExpiresAt"
    );

    // stale 分支同样闭合：receipt 只属于 ready。
    let stale = body_text("valid/sync-session-preparation-result-stale.json");
    let mangled = stale.replacen('}', r#", "receipt": {}"}"#, 1);
    assert!(
        serde_json::from_str::<SessionPreparationResult>(&mangled).is_err(),
        "stale 分支不得携带 receipt"
    );
}

#[test]
fn attachment_begin_target_is_mutually_exclusive_with_session_id() {
    let frame = attachment::Frame::decode(&fixture_text("attachment/valid/begin-preparation.json"))
        .expect("准备 begin 必须解码");
    let FrameBody::Begin(BeginBody::Preparation(body)) = frame.body else {
        panic!("begin 必须落在准备分支");
    };
    assert_eq!(body.target.kind, attachment::PreparationTargetKind);
    assert_eq!(
        body.target.session_id.as_str(),
        "6f1c2d3e-4a5b-4c6d-8e7f-901122334455"
    );
    assert_eq!(body.target.generation.as_str(), "1");

    // 同时给出正式 sessionId 与 target：两个闭合分支都不接受，必须失败。
    assert_eq!(
        attachment::Frame::decode(&fixture_text(
            "attachment/invalid/begin-formal-and-preparation.json"
        )),
        Err(attachment::DecodeError::Schema)
    );
    // target.kind 取正式值不能绕过旧分支。
    assert_eq!(
        attachment::Frame::decode(&fixture_text(
            "attachment/invalid/begin-preparation-target-formal-kind.json"
        )),
        Err(attachment::DecodeError::Schema)
    );

    let frame = attachment::Frame::decode(&fixture_text(
        "attachment/valid/manifest-begin-preparation.json",
    ))
    .expect("准备清单必须解码");
    let FrameBody::ManifestBegin(ManifestBeginBody::Preparation(body)) = frame.body else {
        panic!("manifest_begin 必须落在准备分支");
    };
    assert_eq!(
        body.submission_id.as_str(),
        "c7d8e9fa-0b1c-4d2e-9f3a-4b5c6d7e8f90"
    );

    // 正式分支不受影响：旧 begin 仍按 Owned 解码。
    let frame = attachment::Frame::decode(&fixture_text("attachment/valid/begin.json"))
        .expect("正式 begin 必须解码");
    assert!(matches!(frame.body, FrameBody::Begin(BeginBody::Owned(_))));
    let frame = attachment::Frame::decode(&fixture_text("attachment/valid/manifest-begin.json"))
        .expect("正式 manifest_begin 必须解码");
    assert!(matches!(
        frame.body,
        FrameBody::ManifestBegin(ManifestBeginBody::Owned(_))
    ));
}

#[test]
fn preparation_error_codes_match_the_registry() {
    assert_eq!(ErrorCode::PreparationStale.as_str(), "preparation.stale");
    assert_eq!(
        ErrorCode::PreparationAlreadyCommitted.as_str(),
        "preparation.already_committed"
    );
    assert_eq!(
        ErrorCode::PreparationStoreRebuilt.as_str(),
        "preparation.store_rebuilt"
    );
    assert!(ErrorCode::PreparationStale.default_retryable());
    assert!(!ErrorCode::PreparationAlreadyCommitted.default_retryable());
    assert!(!ErrorCode::PreparationStoreRebuilt.default_retryable());

    // 附件错误帧可以携带清单标识；正式分支与旧 peer 的字段集合不变。
    let frame = attachment::Frame::decode(&fixture_text(
        "attachment/valid/error-preparation-stale.json",
    ))
    .expect("准备清单错误必须解码");
    let FrameBody::Error(body) = frame.body else {
        panic!("必须是 attachment.error");
    };
    assert_eq!(body.code, ErrorCode::PreparationStale);
    assert!(
        body.transfer_id.is_null(),
        "清单错误的 transferId 必须为 null"
    );
    assert!(
        body.submission_id.is_some(),
        "清单错误必须携带 submissionId"
    );
}
