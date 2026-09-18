//! 五个家族的合同级边界：夹具只覆盖"能解析/不能解析"，这里钉住**语义**。
//!
//! 每条断言都对应一句 `docs/SYNC_PROTOCOL.md` §4.3 的要求（"ID、sequence、timestamp、枚举和长度
//! 限制必须在 wire DTO 边界验证"）或 schema 的 `required` / `if-then` 结构；只做"形状对得上"的实现
//! 会在这里失败。文本一律用 JSON 字面量构造，避免把测试绑在 Rust 字段布局上。

mod support;

use sync_protocol::command::Command;
use sync_protocol::common::{ModeState, RawObject, SessionSummary};
use sync_protocol::envelope::Envelope;
use sync_protocol::event::Body as EventBody;
use sync_protocol::sync::{SnapshotChunk, Subscribe};
use sync_protocol::views;

const MESSAGE_ID: &str = "1728394a-5c6d-4e7f-88a9-b0c1d2e3f405";
const CONNECTION_ID: &str = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const SESSION_ID: &str = "9aa8bf8c-5cba-4c58-9e27-b7f4d1a2c3d4";
const EVENT_ID: &str = "3a4b5c6d-7e80-4192-a3b4-c5d6e7f8091a";
const SERVER_EPOCH: &str = "00384a03-bc90-4095-b65d-82fb8cc47e13";
const TURN_ID: &str = "b7e3a6f1-2c4d-4e5f-8a9b-0c1d2e3f4051";
const CREATED_AT: &str = "2026-09-18T09:12:33.000Z";

fn post_auth(message_type: &str, body: &str) -> String {
    format!(
        "{{\"protocolVersion\":1,\"type\":\"{message_type}\",\"messageId\":\"{MESSAGE_ID}\",\"connectionId\":\"{CONNECTION_ID}\",\"connectionSequence\":\"50\",\"body\":{body}}}"
    )
}

fn decode<T: serde::de::DeserializeOwned>(message_type: &str, body: &str) -> Result<T, String> {
    let envelope = Envelope::decode(&post_auth(message_type, body)).map_err(|e| e.to_string())?;
    serde_json::from_str(envelope.body().get()).map_err(|error| error.to_string())
}

fn session_summary(state: &str, title: &str) -> String {
    format!(
        "{{\"sessionId\":\"{SESSION_ID}\",\"title\":{title},\"agent\":{{\"agentId\":\"omp\",\"name\":\"Oh My Pi\"}},\"state\":\"{state}\",\"origin\":{{\"kind\":\"local\"}},\"currentMode\":null,\"version\":\"7\",\"createdAt\":\"{CREATED_AT}\",\"updatedAt\":\"{CREATED_AT}\"}}"
    )
}

#[test]
fn subscribe_cursor_key_is_required_but_may_be_null() {
    let ok_null: Result<Subscribe, String> =
        decode("sync.subscribe", "{\"cursor\":null,\"scope\":\"machine\"}");
    assert!(ok_null.is_ok(), "cursor=null 是 schema 允许的：{ok_null:?}");

    let ok_value: Result<Subscribe, String> = decode(
        "sync.subscribe",
        &format!(
            "{{\"cursor\":{{\"serverEpoch\":\"{SERVER_EPOCH}\",\"globalSequence\":\"2318\"}},\"scope\":\"machine\"}}"
        ),
    );
    assert!(ok_value.is_ok(), "cursor 对象必须被接受：{ok_value:?}");

    let missing: Result<Subscribe, String> = decode("sync.subscribe", "{\"scope\":\"machine\"}");
    assert!(
        missing.is_err(),
        "cursor 是 required：键缺失必须被拒，而不是解成 null"
    );

    let bad_scope: Result<Subscribe, String> =
        decode("sync.subscribe", "{\"cursor\":null,\"scope\":\"all\"}");
    assert!(bad_scope.is_err(), "scope 只有 machine 一个取值");
}

#[test]
fn snapshot_chunk_index_is_a_decimal_string_and_items_follow_the_resource() {
    let chunk = |chunk_index: &str, resource: &str, items: &str| {
        format!(
            "{{\"snapshotId\":\"{EVENT_ID}\",\"chunkIndex\":{chunk_index},\"resource\":\"{resource}\",\"items\":{items}}}"
        )
    };

    let ok: Result<SnapshotChunk, String> = decode(
        "sync.snapshot_chunk",
        &chunk(
            "\"0\"",
            "sessions",
            &format!("[{}]", session_summary("idle", "null")),
        ),
    );
    assert!(ok.is_ok(), "合法 chunk 必须被接受：{ok:?}");

    let numeric: Result<SnapshotChunk, String> = decode(
        "sync.snapshot_chunk",
        &chunk(
            "0",
            "sessions",
            &format!("[{}]", session_summary("idle", "null")),
        ),
    );
    assert!(
        numeric.is_err(),
        "chunkIndex 是 decimalString，数字字面量必须被拒"
    );

    let leading_zero: Result<SnapshotChunk, String> = decode(
        "sync.snapshot_chunk",
        &chunk(
            "\"01\"",
            "sessions",
            &format!("[{}]", session_summary("idle", "null")),
        ),
    );
    assert!(leading_zero.is_err(), "decimalString 不允许前导零");

    // items 的元素形状由同一 body 的 resource 决定：sessions 的分片不能塞 capabilities 的项。
    let mismatched: Result<SnapshotChunk, String> = decode(
        "sync.snapshot_chunk",
        &chunk(
            "\"0\"",
            "sessions",
            &format!(
                "[{{\"sessionId\":\"{SESSION_ID}\",\"agentCapabilities\":{{}},\"brokerAdditions\":{{}}}}]"
            ),
        ),
    );
    assert!(mismatched.is_err(), "resource 与 items 形状不符必须被拒");

    let unknown_resource: Result<SnapshotChunk, String> =
        decode("sync.snapshot_chunk", &chunk("\"0\"", "everything", "[]"));
    assert!(unknown_resource.is_err(), "未登记的 resource 必须被拒");
}

#[test]
fn snapshot_session_state_enum_is_enforced() {
    let ok: Result<SnapshotChunk, String> = decode(
        "sync.snapshot_chunk",
        &format!(
            "{{\"snapshotId\":\"{EVENT_ID}\",\"chunkIndex\":\"0\",\"resource\":\"sessions\",\"items\":[{}]}}",
            session_summary("waiting_permission", "\"标题\"")
        ),
    );
    assert!(ok.is_ok(), "waiting_permission 是合法 state：{ok:?}");

    let bad: Result<SnapshotChunk, String> = decode(
        "sync.snapshot_chunk",
        &format!(
            "{{\"snapshotId\":\"{EVENT_ID}\",\"chunkIndex\":\"0\",\"resource\":\"sessions\",\"items\":[{}]}}",
            session_summary("archived", "null")
        ),
    );
    assert!(bad.is_err(), "未登记的 state 必须被拒");

    let bad_timestamp: Result<SnapshotChunk, String> = decode(
        "sync.snapshot_chunk",
        &format!(
            "{{\"snapshotId\":\"{EVENT_ID}\",\"chunkIndex\":\"0\",\"resource\":\"sessions\",\"items\":[{{\"sessionId\":\"{SESSION_ID}\",\"title\":null,\"agent\":{{\"agentId\":\"omp\",\"name\":\"Oh My Pi\"}},\"state\":\"idle\",\"origin\":{{\"kind\":\"local\"}},\"currentMode\":null,\"version\":\"7\",\"createdAt\":\"2026-09-18T09:12:33Z\",\"updatedAt\":\"{CREATED_AT}\"}}]}}"
        ),
    );
    assert!(
        bad_timestamp.is_err(),
        "timestamp 必须带毫秒与 Z（schema 的 pattern）"
    );
}

fn event_body(origin: &str, remote_origin: &str, view: &str) -> String {
    format!(
        "{{\"globalSequence\":\"1041\",\"sessionSequence\":\"12\",\"eventId\":\"{EVENT_ID}\",\"sessionId\":\"{SESSION_ID}\",\"eventType\":\"session.created\",\"causationRequestId\":null,\"origin\":{origin},\"remoteOrigin\":{remote_origin},\"createdAt\":\"{CREATED_AT}\",\"payload\":{{\"view\":{view}}}}}"
    )
}

#[test]
fn event_origin_and_remote_origin_follow_the_event_schema() {
    // `origin.kind` 只描述产生事件的来源类型（docs §10.1）：远程不作为取值，跳数由 remoteOrigin 表达。
    let agent_ok: Result<EventBody, String> = decode(
        "event",
        &event_body("{\"kind\":\"agent\",\"deviceId\":null}", "null", "{}"),
    );
    assert!(
        agent_ok.is_ok(),
        "本地事件的 remoteOrigin 必须是 null：{agent_ok:?}"
    );

    let device_ok: Result<EventBody, String> = decode(
        "event",
        &event_body("{\"kind\":\"device\",\"deviceId\":null}", "null", "{}"),
    );
    assert!(device_ok.is_ok(), "device 是合法 kind：{device_ok:?}");

    let remote_kind: Result<EventBody, String> = decode(
        "event",
        &event_body("{\"kind\":\"remote\",\"deviceId\":null}", "null", "{}"),
    );
    assert!(
        remote_kind.is_err(),
        "remote 不是 origin.kind 的取值（docs §10.1）"
    );

    // `remoteOrigin` 是 required（可 null）：键缺失必须被拒
    // （对应 invalid/remote-event-without-origin.json，expectedKeyword = required）。
    let key_missing: Result<EventBody, String> = decode(
        "event",
        &event_body("{\"kind\":\"agent\",\"deviceId\":null}", "null", "{}")
            .replace(",\"remoteOrigin\":null", ""),
    );
    assert!(
        key_missing.is_err(),
        "remoteOrigin 是 required（可 null）：键缺失必须被拒"
    );

    // imported 事件的 eventId 必须等于 remoteOrigin.originEventId（docs §9.6 / §10.1）。
    let matching = format!(
        "{{\"ownerNodeId\":\"{SERVER_EPOCH}\",\"exportId\":\"export-laptop-zed\",\"originEpoch\":\"{TURN_ID}\",\"originEventId\":\"{EVENT_ID}\",\"originSequence\":\"900\"}}"
    );
    let imported_ok: Result<EventBody, String> = decode(
        "event",
        &event_body("{\"kind\":\"agent\",\"deviceId\":null}", &matching, "{}"),
    );
    assert!(
        imported_ok.is_ok(),
        "imported 事件的 remoteOrigin 形状合法：{imported_ok:?}"
    );

    let mismatched = matching.replace(EVENT_ID, TURN_ID);
    let mismatch: Result<EventBody, String> = decode(
        "event",
        &event_body("{\"kind\":\"agent\",\"deviceId\":null}", &mismatched, "{}"),
    );
    assert!(
        mismatch.is_err(),
        "imported 事件的 eventId 必须等于 remoteOrigin.originEventId"
    );

    let incomplete = "{\"ownerNodeId\":\"00384a03-bc90-4095-b65d-82fb8cc47e13\",\"exportId\":\"exp-1\",\"originEventId\":\"3a4b5c6d-7e80-4192-a3b4-c5d6e7f8091a\",\"originSequence\":\"9\"}";
    assert!(
        decode::<EventBody>(
            "event",
            &event_body("{\"kind\":\"agent\",\"deviceId\":null}", incomplete, "{}")
        )
        .is_err(),
        "remoteOrigin 缺 originEpoch 必须被拒"
    );

    let bad_event_type: Result<EventBody, String> = decode(
        "event",
        &event_body("{\"kind\":\"agent\",\"deviceId\":null}", "null", "{}")
            .replace("session.created", "Session.Created"),
    );
    assert!(
        bad_event_type.is_err(),
        "eventType 必须匹配 ^[a-z0-9_.-]{{1,128}}$"
    );

    let view_not_object: Result<EventBody, String> = decode(
        "event",
        &event_body("{\"kind\":\"agent\",\"deviceId\":null}", "null", "[]"),
    );
    assert!(view_not_object.is_err(), "payload.view 必须是 object");
}

#[test]
fn session_summary_origin_block_is_discriminated_by_kind() {
    let parse = |origin: &str| {
        let text = format!(
            "{{\"sessionId\":\"{SESSION_ID}\",\"title\":null,\"agent\":{{\"agentId\":\"omp\",\"name\":\"Oh My Pi\"}},\"state\":\"idle\",\"origin\":{origin},\"currentMode\":null,\"version\":\"7\",\"createdAt\":\"{CREATED_AT}\",\"updatedAt\":\"{CREATED_AT}\"}}"
        );
        serde_json::from_str::<SessionSummary>(&text).map_err(|error| error.to_string())
    };

    let local: Result<SessionSummary, String> = parse("{\"kind\":\"local\"}");
    assert!(local.is_ok(), "本节点会话只带 kind：{local:?}");

    let remote: Result<SessionSummary, String> = parse(&format!(
        "{{\"kind\":\"remote\",\"ownerNodeId\":\"{SERVER_EPOCH}\",\"exportId\":\"export-laptop-zed\",\"originEpoch\":\"{TURN_ID}\",\"online\":true}}"
    ));
    assert!(remote.is_ok(), "imported 会话的四种字段必需：{remote:?}");

    let remote_incomplete: Result<SessionSummary, String> =
        parse("{\"kind\":\"remote\",\"online\":true}");
    assert!(
        remote_incomplete.is_err(),
        "remote 形状缺 ownerNodeId/exportId/originEpoch 必须被拒"
    );

    let local_extra: Result<SessionSummary, String> = parse("{\"kind\":\"local\",\"online\":true}");
    assert!(
        local_extra.is_err(),
        "local 形状不接受 remote 的字段（additionalProperties:false）"
    );

    let unknown_kind: Result<SessionSummary, String> = parse("{\"kind\":\"elsewhere\"}");
    assert!(unknown_kind.is_err(), "未登记的 origin.kind 必须被拒");
}

#[test]
fn prompt_content_only_accepts_text_blocks() {
    let command = |content: &str| {
        format!(
            "{{\"requestId\":\"{MESSAGE_ID}\",\"command\":\"session.prompt\",\"sessionId\":\"{SESSION_ID}\",\"payload\":{{\"content\":{content}}}}}"
        )
    };

    let text_ok: Result<Command, String> =
        decode("command", &command("[{\"type\":\"text\",\"text\":\"hi\"}]"));
    assert!(text_ok.is_ok(), "文本 prompt 必须被接受：{text_ok:?}");

    let image: Result<Command, String> = decode(
        "command",
        &command("[{\"type\":\"image\",\"data\":\"AAAA\"}]"),
    );
    assert!(
        image.is_err(),
        "v1 的 prompt content 只有 text 分支，image 必须被明确拒绝而不是文本化"
    );

    let empty: Result<Command, String> = decode("command", &command("[]"));
    assert!(empty.is_err(), "content 的 minItems 是 1");
}

#[test]
fn command_name_and_payload_must_agree() {
    // `command` 说 session.prompt，payload 却是 config.set 的形状：必须被拒，不能按其中一个解释。
    let mismatch: Result<Command, String> = decode(
        "command",
        &format!(
            "{{\"requestId\":\"{MESSAGE_ID}\",\"command\":\"session.prompt\",\"sessionId\":\"{SESSION_ID}\",\"payload\":{{\"optionId\":\"model\",\"value\":\"gpt\",\"expectedVersion\":\"3\"}}}}"
        ),
    );
    assert!(mismatch.is_err(), "命令名与 payload 形状不一致必须被拒");

    let unknown_command: Result<Command, String> = decode(
        "command",
        &format!(
            "{{\"requestId\":\"{MESSAGE_ID}\",\"command\":\"session.rename\",\"payload\":{{}}}}"
        ),
    );
    assert!(unknown_command.is_err(), "未登记的命令名必须被拒");

    let session_scoped_without_id: Result<Command, String> = decode(
        "command",
        &format!(
            "{{\"requestId\":\"{MESSAGE_ID}\",\"command\":\"session.read\",\"payload\":{{\"include\":[\"messages\"]}}}}"
        ),
    );
    assert!(
        session_scoped_without_id.is_err(),
        "session.read 的 sessionId 是 required"
    );

    let bad_include: Result<Command, String> = decode(
        "command",
        &format!(
            "{{\"requestId\":\"{MESSAGE_ID}\",\"command\":\"session.read\",\"sessionId\":\"{SESSION_ID}\",\"payload\":{{\"include\":[\"messages\",\"messages\"]}}}}"
        ),
    );
    assert!(bad_include.is_err(), "include 的 uniqueItems 必须被执行");

    let cancel_without_turn: Result<Command, String> = decode(
        "command",
        &format!(
            "{{\"requestId\":\"{MESSAGE_ID}\",\"command\":\"session.cancel\",\"sessionId\":\"{SESSION_ID}\",\"payload\":{{}}}}"
        ),
    );
    assert!(
        cancel_without_turn.is_err(),
        "session.cancel 必须携带 turnId"
    );

    let ok_cancel: Result<Command, String> = decode(
        "command",
        &format!(
            "{{\"requestId\":\"{MESSAGE_ID}\",\"command\":\"session.cancel\",\"sessionId\":\"{SESSION_ID}\",\"payload\":{{\"turnId\":\"{TURN_ID}\"}}}}"
        ),
    );
    assert!(ok_cancel.is_ok(), "合法的 session.cancel：{ok_cancel:?}");
}

#[test]
fn required_nullable_keys_are_enforced_on_nested_common_types() {
    let mode_state_missing_key = "{\"availableModes\":[],\"version\":\"3\"}";
    let missing: Result<ModeState, String> =
        serde_json::from_str(mode_state_missing_key).map_err(|error| error.to_string());
    assert!(
        missing.is_err(),
        "currentModeId 是 required+nullable：键缺失必须被拒，而不是解成 null"
    );

    let null_mode: Result<ModeState, String> =
        serde_json::from_str("{\"currentModeId\":null,\"availableModes\":[],\"version\":\"3\"}")
            .map_err(|error| error.to_string());
    assert!(null_mode.is_ok(), "currentModeId=null 合法：{null_mode:?}");

    let summary_missing_title = format!(
        "{{\"sessionId\":\"{SESSION_ID}\",\"agent\":{{\"agentId\":\"omp\",\"name\":\"Oh My Pi\"}},\"state\":\"idle\",\"origin\":{{\"kind\":\"local\"}},\"currentMode\":null,\"version\":\"7\",\"createdAt\":\"{CREATED_AT}\",\"updatedAt\":\"{CREATED_AT}\"}}"
    );
    let missing_title: Result<SessionSummary, String> =
        serde_json::from_str(&summary_missing_title).map_err(|error| error.to_string());
    assert!(
        missing_title.is_err(),
        "sessionSummary.title 是 required+nullable：键缺失必须被拒"
    );
}

#[test]
fn projection_rejects_view_that_violates_its_registered_shape() {
    // session.created 要求 `session`（sessionSummary）；缺它就不得投影成功。
    let view = RawObject::parse(&format!("{{\"sessionId\":\"{SESSION_ID}\"}}")).expect("object");
    let error =
        views::project("session.created", &view).expect_err("缺少注册字段的 view 必须被拒绝");
    assert!(
        error.to_string().contains("session"),
        "错误必须指出缺失的字段：{error}"
    );

    // 字段形状不符（title 必须是 string|null）同样不得投影成功。
    let wrong_shape = RawObject::parse(&format!(
        "{{\"session\":{{\"sessionId\":\"{SESSION_ID}\",\"title\":7,\"agent\":{{\"agentId\":\"omp\",\"name\":\"Oh My Pi\"}},\"state\":\"idle\",\"origin\":{{\"kind\":\"local\"}},\"currentMode\":null,\"version\":\"7\",\"createdAt\":\"{CREATED_AT}\",\"updatedAt\":\"{CREATED_AT}\"}}}}"
    ))
    .expect("object");
    assert!(
        views::project("session.created", &wrong_shape).is_err(),
        "title 为数字必须被拒"
    );
}
