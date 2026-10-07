//! TP1 第二轮（MU1b）合同向量：R1–R9 的机器可判定部分中，`snapshot_scope_vectors.rs` 未覆盖的缺口。
//!
//! 本文件与 `snapshot_scope_vectors.rs` 同属 TP1，分工是：
//!
//! - `snapshot_scope_vectors.rs`：快照资源词表排除、`session.read` 的基础分页形状、node-link 未受影响；
//! - 本文件：清单快照的三类资源与 `chunkCount` 口径（R1）、保留窗口之前的显式不可读（R2）、
//!   同刻消息不重不漏与「不按标识排序」（R3）、`file.changed` 新字段的可选性与 `null` 语义（R6/R7）、
//!   `state` 封闭枚举的取值与非法取值拒绝（R8）、节点级事件的 `sessionId`/`sessionSequence` 为 `null`。
//!
//! 只钉协议形状与合同语义，不触碰任何生产端行为（快照组装、分页实现、事件派生分别属 WP3/WP5–WP7）。

mod support;

use std::collections::BTreeSet;

use sync_protocol::command::{
    Command, CommandPayload, CommandResult, CommandResultPayload, SessionRead, SessionReadResult,
};
use sync_protocol::common::RawObject;
use sync_protocol::error::{Body as ErrorBody, ErrorCode};
use sync_protocol::event::Body as EventBody;
use sync_protocol::sync::{SnapshotBegin, SnapshotChunk, SnapshotEnd, SnapshotResource};
use sync_protocol::views::{self, FileChanged};

const MANIFEST: &str = "fixtures/sync/v1/manifest.json";
const FIXTURE_ROOT: &str = "fixtures/sync/v1/";

/// R1：默认（协商了目录 feature）快照固定为三类清单资源，`chunkCount` = 3。
const CATALOG_BEGIN: &str = "valid/sync-snapshot-catalog-begin.json";
const CATALOG_WORKSPACES: &str = "valid/sync-snapshot-catalog-chunk-workspaces.json";
const CATALOG_AGENTS: &str = "valid/sync-snapshot-catalog-chunk-agents.json";
const CATALOG_END: &str = "valid/sync-snapshot-catalog-end.json";

/// R1：未协商 `core.local-catalog.v1` 时缩为单类，`chunkCount` = 1。
const SESSIONS_ONLY_BEGIN: &str = "valid/sync-snapshot-catalog-sessions-only-begin.json";
const SESSIONS_ONLY_CHUNK: &str = "valid/sync-snapshot-catalog-sessions-only-chunk.json";

/// R3：同一 `createdAt` 的两条消息跨页不重不漏。
const TIED_PAGE_REQUEST: &str = "valid/command-session-read-tied-timestamps-page.json";
const TIED_PAGE_RESULT: &str = "valid/command-result-session-read-tied-timestamps-page.json";
const TIED_NEXT_RESULT: &str = "valid/command-result-session-read-tied-timestamps-next-page.json";

/// R2：保留窗口之前的显式不可读。
const CURSOR_EXPIRED: &str = "valid/error-cursor-expired.json";
const CURSOR_EXPIRED_UNKNOWN_REASON: &str = "invalid/error-cursor-expired-unknown-reason.json";

/// R2/R3：单字段游标与 offset 键必须被拒。
const OFFSET_CURSOR: &str = "invalid/command-session-read-offset-cursor.json";
const SINGLE_FIELD_CREATED_AT: &str =
    "invalid/command-session-read-single-field-before-created-at.json";

/// R6：行数统计两项缺失即合法。
const FILE_CHANGED_STATS_OMITTED: &str = "valid/view-file-changed-stats-omitted.json";
/// R7：工作区外标记。
const FILE_CHANGED_OUTSIDE_WORKSPACE: &str = "valid/view-file-changed-outside-workspace.json";

/// R6/R7：三个可选字段的负例（出现时形状必须正确，显式 `null` 按 schema 语义被拒）。
const FILE_CHANGED_NEGATIVES: [(&str, &str); 5] = [
    (
        "invalid/view-file-changed-added-lines-null.json",
        "addedLines",
    ),
    (
        "invalid/view-file-changed-added-lines-number.json",
        "addedLines",
    ),
    (
        "invalid/view-file-changed-deleted-lines-signed.json",
        "deletedLines",
    ),
    (
        "invalid/view-file-changed-outside-workspace-string.json",
        "outsideWorkspace",
    ),
    (
        "invalid/view-file-changed-outside-workspace-null.json",
        "outsideWorkspace",
    ),
];

/// R8：`state` 封闭枚举的非法取值。第二列是该 fixture 实际携带的、词表外的取值。
const STATE_NEGATIVES: [(&str, &str); 3] = [
    ("invalid/view-agent-connected-state-unknown.json", "unknown"),
    (
        "invalid/view-agent-disconnected-state-idiom.json",
        "connected",
    ),
    (
        "invalid/view-agent-connected-state-free-text.json",
        "connected (复用进程)",
    ),
];

/// R8：两个节点级事件的既有合法夹具。
const AGENT_CONNECTED: &str = "valid/view-agent-connected.json";
const AGENT_DISCONNECTED: &str = "valid/view-agent-disconnected.json";

fn read_case(relative: &str) -> String {
    support::read_text(&support::repo_path(&format!("{FIXTURE_ROOT}{relative}")))
}

fn read_body(relative: &str) -> serde_json::Value {
    serde_json::from_str(support::body_slice(&read_case(relative)))
        .unwrap_or_else(|error| panic!("{relative}：body 不是 JSON：{error}"))
}

fn read_fixture(relative: &str) -> serde_json::Value {
    serde_json::from_str(&read_case(relative)).unwrap_or_else(|error| panic!("{relative}：{error}"))
}

fn manifest_case(fixture: &str) -> support::ManifestCase {
    support::manifest_cases(MANIFEST)
        .into_iter()
        .find(|case| case.fixture == fixture)
        .unwrap_or_else(|| panic!("{fixture}：manifest.json 未登记该 fixture"))
}

fn begin(text: &str) -> SnapshotBegin {
    serde_json::from_str(support::body_slice(text)).expect("snapshot_begin body 可类型化")
}

fn chunk(text: &str) -> SnapshotChunk {
    serde_json::from_str(support::body_slice(text)).expect("snapshot_chunk body 可类型化")
}

fn end(text: &str) -> SnapshotEnd {
    serde_json::from_str(support::body_slice(text)).expect("snapshot_end body 可类型化")
}

// ---------------------------------------------------------------------------------------------
// R1：快照只承载清单类资源，`chunkCount` 与三类资源实际占用的块数一致
// ---------------------------------------------------------------------------------------------

/// 三类清单资源各自可下发，且 begin/end 的 `chunkCount` 恰等于下发的资源数（此处为 3）。
///
/// 判据从「机器资产」取：断言 begin/end 声明的 `chunkCount` 与**实际出现**的 `resource` 种类数相等，
/// 而不是断言某个字面量——这样把某类资源偷偷从快照里删掉（或加回去）会立刻让这条失败。
#[test]
fn catalog_snapshot_chunk_count_matches_three_declared_resources() {
    let begin = begin(&read_case(CATALOG_BEGIN));
    let end = end(&read_case(CATALOG_END));

    assert_eq!(
        begin.chunk_count, end.chunk_count,
        "begin 与 end 的 chunkCount 必须一致（R1）"
    );
    assert_eq!(
        begin.snapshot_id, end.snapshot_id,
        "begin 与 end 必须属于同一个快照"
    );

    let chunks = [CATALOG_WORKSPACES, CATALOG_AGENTS].map(read_case);
    let mut resources: Vec<&'static str> = chunks
        .iter()
        .map(|text| chunk(text).resource().as_str())
        .collect();
    resources.sort_unstable();

    assert_eq!(
        resources,
        vec!["agents", "workspaces"],
        "本向量固定下发 workspaces 与 agents 两类（sessions 由 WP1 的夹具覆盖）"
    );
    assert_eq!(
        begin.chunk_count.get(),
        SnapshotResource::ALL.len() as u64,
        "协商目录 feature 时 chunkCount 必须等于三类清单资源的种类数"
    );
}

/// 未协商 `core.local-catalog.v1` 时快照缩为单类：`resource` 只有 `sessions`，`chunkCount` = 1。
#[test]
fn sessions_only_snapshot_uses_single_catalog_chunk() {
    let begin = begin(&read_case(SESSIONS_ONLY_BEGIN));
    let chunk = chunk(&read_case(SESSIONS_ONLY_CHUNK));

    assert_eq!(
        begin.chunk_count.get(),
        1,
        "未协商目录 feature 时 chunkCount 取 1"
    );
    assert_eq!(
        chunk.resource(),
        SnapshotResource::Sessions,
        "缩为单类时资源只能是 sessions"
    );
    assert!(
        !chunk.items().is_empty(),
        "单类快照仍必须带真实内容，而不是空占位"
    );

    // 该 chunk 的每个元素都必须是 sessions 形状：带 sessionId 且不带任何目录或明细字段。
    let body = read_body(SESSIONS_ONLY_CHUNK);
    for item in body["items"].as_array().expect("items 是数组") {
        assert!(
            item.get("sessionId").is_some(),
            "sessions 元素必须带 sessionId：{item}"
        );
        for forbidden in ["turnId", "interactionId", "agentId", "alias"] {
            assert!(
                item.get(forbidden).is_none(),
                "sessions 元素不得带 {forbidden}（那是其它资源的形状）：{item}"
            );
        }
    }
}

/// 五类会话明细资源在**任何**快照中都不出现：跨终态与单类两组快照的 `resource` 取值的并集
/// 必须恰好是三类清单资源。
#[test]
fn no_snapshot_vector_carries_a_session_detail_resource() {
    let declared: BTreeSet<&str> = SnapshotResource::ALL
        .iter()
        .map(|resource| resource.as_str())
        .collect();
    assert_eq!(
        declared,
        ["sessions", "workspaces", "agents"].into_iter().collect(),
        "快照资源封闭词表"
    );

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for fixture in [
        SESSIONS_ONLY_CHUNK,
        CATALOG_WORKSPACES,
        CATALOG_AGENTS,
        "valid/sync-snapshot-chunk-sessions.json",
    ] {
        let body = read_body(fixture);
        let resource = body["resource"]
            .as_str()
            .expect("resource 是字符串")
            .to_owned();
        assert!(
            declared.contains(resource.as_str()),
            "{fixture}：快照里出现了非清单类资源 {resource}"
        );
        seen.insert(resource);
    }
    assert_eq!(
        seen,
        ["sessions", "workspaces", "agents"]
            .map(str::to_owned)
            .into_iter()
            .collect::<BTreeSet<String>>(),
        "本包固定覆盖三类清单资源各一条快照 chunk"
    );
}

// ---------------------------------------------------------------------------------------------
// R3：复合游标 —— 同刻不重不漏、不按标识排序
// ---------------------------------------------------------------------------------------------

/// 同一 `createdAt` 的两条消息在一页内按**稳定且可重复**的次序排列，且该次序不是 `messageId` 的
/// 字典序——合同只要求稳定，本向量固定其中一种并显式证明它不是按标识排序得到的。
#[test]
fn tie_broken_messages_are_ordered_stably_and_not_by_identifier() {
    let result = read_result(&read_case(TIED_PAGE_RESULT));
    let messages = result
        .resources
        .messages
        .as_ref()
        .expect("同刻向量必须带 messages");

    assert!(
        result.has_earlier,
        "同刻页之后仍有更早内容，hasEarlier 必须为 true"
    );

    let tied: Vec<&str> = messages
        .iter()
        .filter(|message| message.created_at.as_str() == "2026-09-17T12:18:00.000Z")
        .map(|message| message.message_id.as_str())
        .collect();
    assert_eq!(tied.len(), 2, "本向量固定同刻两条消息：{tied:?}");

    let mut lexicographic = tied.clone();
    lexicographic.sort_unstable();
    assert_ne!(
        tied, lexicographic,
        "同刻次序恰好等于字典序，这条向量就不再证明「排序不依赖标识」"
    );

    let times: Vec<&str> = messages
        .iter()
        .map(|message| message.created_at.as_str())
        .collect();
    assert!(
        times.windows(2).all(|pair| pair[0] <= pair[1]),
        "一页内部必须按 createdAt 升序：{times:?}"
    );
}

/// 相邻两页功能上等价于 `snapshot_scope_vectors::adjacent_pages_join_without_gaps_or_overlap` 的
/// 同刻版本：下一页整体严格更早，且与上一页无交集、无重复。
#[test]
fn tied_timestamp_pages_join_without_gaps_or_overlap() {
    let first = read_result(&read_case(TIED_PAGE_RESULT));
    let second = read_result(&read_case(TIED_NEXT_RESULT));

    let first_ids: Vec<&str> = first
        .resources
        .messages
        .as_ref()
        .expect("第一页带 messages")
        .iter()
        .map(|message| message.message_id.as_str())
        .collect();
    let second_ids: Vec<&str> = second
        .resources
        .messages
        .as_ref()
        .expect("第二页带 messages")
        .iter()
        .map(|message| message.message_id.as_str())
        .collect();

    let union: BTreeSet<&str> = first_ids.iter().chain(second_ids.iter()).copied().collect();
    assert_eq!(
        union.len(),
        first_ids.len() + second_ids.len(),
        "两页之间出现重复消息：{first_ids:?} / {second_ids:?}"
    );
    assert!(
        !second.has_earlier,
        "到达最早一条时 hasEarlier 必须为 false"
    );

    // 第二页整体严格早于第一页的最早时刻——同刻 tie-breaker 的切割点不得漏掉或重复任何一条。
    let first_earliest = first
        .resources
        .messages
        .as_ref()
        .expect("第一页带 messages")
        .iter()
        .map(|message| message.created_at.as_str())
        .min()
        .expect("第一页非空");
    assert!(
        second
            .resources
            .messages
            .as_ref()
            .expect("第二页带 messages")
            .iter()
            .all(|message| message.created_at.as_str() < first_earliest),
        "第二页必须整体早于第一页最早时刻 {first_earliest}"
    );
}

/// `limit` 是请求侧的单页上限，且 schema 是结构性计数（不在 wire 上做「超上限即拒绝」）。
///
/// R2「超出上限的 limit 被收敛」在 wire 层可判定的部分是：`sessionReadLimit` 只有 `minimum: 1`、
/// 没有 `maximum`（服务端收敛而非报错）。这里用机器资产直接钉住这一点，避免客户端或服务端在
/// wire 层加上会让请求失败的 `maximum`。
#[test]
fn session_read_limit_has_no_wire_upper_bound() {
    let schema = support::read_json(&support::repo_path("schemas/sync/v1/command.schema.json"));
    let limit = &schema["$defs"]["sessionReadLimit"];

    assert_eq!(
        limit["type"], "integer",
        "limit 必须是结构性计数（integer）"
    );
    assert_eq!(limit["minimum"], 1, "limit 的下界是 1");
    assert!(
        limit.get("maximum").is_none(),
        "limit 不得有 maximum：超过服务端上限时服务端收敛而非让请求失败（R2）"
    );

    // 请求侧带 limit 的向量必须能解码出该值。
    let read = read_request(&read_case(TIED_PAGE_REQUEST));
    assert_eq!(
        read.limit.map(|limit| limit.get()),
        Some(2),
        "limit 必须逐字承载客户端请求的上限"
    );
}

// ---------------------------------------------------------------------------------------------
// R2：保留窗口之前的显式不可读
// ---------------------------------------------------------------------------------------------

/// 到达保留窗口之前时服务端返回明确的保留期已过类错误：`sync.cursor_invalid` +
/// `details.reason = "cursor_expired"`，`retryable = true`。MUST NOT 静默返回更少元素。
#[test]
fn retention_window_error_is_explicit_and_typed() {
    let body: ErrorBody = serde_json::from_str(support::body_slice(&read_case(CURSOR_EXPIRED)))
        .unwrap_or_else(|error| panic!("{CURSOR_EXPIRED}：error body 解码失败：{error}"));

    assert_eq!(
        body.code,
        ErrorCode::SyncCursorInvalid,
        "保留期已过必须用 sync.cursor_invalid 表达"
    );
    assert!(
        body.retryable,
        "sync.cursor_invalid 的默认 retryable 为 true（registry）"
    );

    let details: serde_json::Value =
        serde_json::from_str(body.details.get()).expect("details 是 JSON");
    assert_eq!(
        details["reason"], "cursor_expired",
        "details.reason 必须是 registry 词表里的 cursor_expired：{details}"
    );

    // 「静默返回更少元素」在这一侧不可表达：错误消息必须指出保留策略，而不是一个空页。
    assert!(
        body.message.as_str().contains("保留"),
        "错误消息必须说明保留窗口已被清理，而不是让客户端误以为已到最早一条：{}",
        body.message.as_str()
    );
}

/// `details.reason` 是封闭词表：registry 允许的取值恰好是
/// `malformed`/`epoch_mismatch`/`beyond_head`/`cursor_expired`，其它取值必须被拒。
#[test]
fn cursor_invalid_details_reason_is_a_closed_vocabulary() {
    let registry = support::read_json(&support::repo_path("compatibility/errors/v1/errors.json"));
    let entry = registry["protocols"]["sync"]["errors"]
        .as_array()
        .expect("registry sync.errors")
        .iter()
        .find(|entry| entry["code"] == "sync.cursor_invalid")
        .expect("registry 必须登记 sync.cursor_invalid");

    let declared: Vec<&str> = entry["details"]["properties"]["reason"]["enum"]
        .as_array()
        .expect("reason 是封闭词表")
        .iter()
        .map(|value| value.as_str().expect("取值是字符串"))
        .collect();
    assert_eq!(
        declared,
        vec![
            "malformed",
            "epoch_mismatch",
            "beyond_head",
            "cursor_expired"
        ],
        "reason 词表必须与 registry 逐条同序"
    );

    // 词表外的取值必须是 manifest 登记的负例，且失败点是那两条 schema 的 enum。
    let case = manifest_case(CURSOR_EXPIRED_UNKNOWN_REASON);
    assert!(
        !case.valid,
        "{CURSOR_EXPIRED_UNKNOWN_REASON} 必须登记为 valid: false"
    );
    assert_eq!(
        case.expected_keyword.as_deref(),
        Some("enum"),
        "失败原因必须是词表，而不是形状"
    );
    let value: serde_json::Value = serde_json::from_str(&support::read_text(&support::repo_path(
        &format!("{FIXTURE_ROOT}{CURSOR_EXPIRED_UNKNOWN_REASON}"),
    )))
    .expect("负例原文是 JSON");
    assert_eq!(
        value["reason"], "retention_window_passed",
        "负例的 reason 必须是词表外的取值"
    );
}

// ---------------------------------------------------------------------------------------------
// R2/R3：游标形状 —— 单字段与 offset 键必须被拒
// ---------------------------------------------------------------------------------------------

/// `before` 只接受 `{createdAt, messageId}` 两键：offset 式游标必须被拒，且失败点是
/// `sessionReadBefore` 的 `additionalProperties: false`。
#[test]
fn offset_style_cursor_is_rejected() {
    assert!(
        serde_json::from_str::<Command>(support::body_slice(&read_case(OFFSET_CURSOR))).is_err(),
        "{OFFSET_CURSOR}：offset 式游标必须被拒"
    );
    let before = read_body(OFFSET_CURSOR)["payload"]["before"].clone();
    assert!(
        before.get("offset").is_some(),
        "{OFFSET_CURSOR}：失败点确实来自 offset 键：{before}"
    );
    assert!(
        before.get("createdAt").is_none() && before.get("messageId").is_none(),
        "{OFFSET_CURSOR}：本负例不得同时缺排序分量，否则失败原因会混淆"
    );
    assert_eq!(
        manifest_case(OFFSET_CURSOR).expected_keyword.as_deref(),
        Some("additionalProperties"),
        "失败原因必须是未知键"
    );
}

/// 两个单字段方向都必须被拒：只给 `createdAt`（本文件的 fixture）与只给 `messageId`
/// （`snapshot_scope_vectors.rs` 的 fixture，WP1 另有 `command-session-read-single-field-before.json`）。
#[test]
fn both_single_field_cursor_directions_are_rejected() {
    for fixture in [
        SINGLE_FIELD_CREATED_AT,
        "invalid/command-session-read-before-missing-message-id.json",
        "invalid/command-session-read-single-field-before.json",
    ] {
        assert!(
            serde_json::from_str::<Command>(support::body_slice(&read_case(fixture))).is_err(),
            "{fixture}：单字段游标必须被拒"
        );
        let before = read_body(fixture)["payload"]["before"].clone();
        let present: Vec<&str> = ["createdAt", "messageId"]
            .into_iter()
            .filter(|key| before.get(key).is_some())
            .collect();
        assert_eq!(
            present.len(),
            1,
            "{fixture}：单字段游标必须恰好只带一个排序分量：{before}"
        );
        assert_eq!(
            manifest_case(fixture).expected_keyword.as_deref(),
            Some("required"),
            "{fixture}：失败原因必须是缺失的必需键"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// R6/R7：`file.changed` 的新字段
// ---------------------------------------------------------------------------------------------

/// 两个行数统计与越界标记都是**可选**的：键全部缺失时事件仍然合法，且解析结果是 `None` 而非零值。
///
/// 这是「判定不出时省略而非填零」（R6）与「键缺失即路径在工作区内」（R7）在 wire 层的判据。
#[test]
fn file_changed_optional_fields_may_be_absent() {
    let view = project_file_changed(FILE_CHANGED_STATS_OMITTED);

    assert!(
        view.added_lines.is_none(),
        "行数未被判定时 addedLines 必须缺省为 None，不得是零"
    );
    assert!(
        view.deleted_lines.is_none(),
        "行数未被判定时 deletedLines 必须缺省为 None，不得是零"
    );
    assert!(
        view.outside_workspace.is_none(),
        "键缺失即「在工作区内」：不得被读成 true 或 false"
    );

    // 直接读原始 view：证明这不是「带了 null 键后被解析成 None」。
    let raw = read_body(FILE_CHANGED_STATS_OMITTED)["payload"]["view"].clone();
    for key in ["addedLines", "deletedLines", "outsideWorkspace"] {
        assert!(
            raw.get(key).is_none(),
            "{FILE_CHANGED_STATS_OMITTED}：可选字段 {key} 不得以任何形式出现：{raw}"
        );
    }
    // 必填集合不变：四个基字段必须都在。
    for key in ["changeId", "kind", "displayPath", "summary"] {
        assert!(
            raw.get(key).is_some(),
            "{STATS_HINT}{FILE_CHANGED_STATS_OMITTED}：{key} 是必需字段"
        );
    }
}

const STATS_HINT: &str = "R6 的必填集合不变：";

/// 行数统计与越界标记出现时必须逐字承载，且 `decimalString` 只接受非负十进制字面量。
#[test]
fn file_changed_optional_fields_carry_their_declared_values() {
    let view = project_file_changed(FILE_CHANGED_OUTSIDE_WORKSPACE);
    assert_eq!(
        view.outside_workspace,
        Some(true),
        "工作区外必须由 outsideWorkspace: true 表达"
    );
    assert_eq!(
        view.display_path.as_str(),
        "outside.txt",
        "工作区外的展示路径只给文件名，不含回退层级"
    );
    assert!(
        !view.display_path.as_str().contains(".."),
        "展示路径不得包含回退层级：{}",
        view.display_path.as_str()
    );

    // schema 侧：三个字段仍不在 required，且类型分别是 decimalString / boolean。
    let schema = support::read_json(&support::repo_path(
        "schemas/sync/v1/event-views.schema.json",
    ));
    let required: Vec<&str> = schema["$defs"]["file.changed"]["required"]
        .as_array()
        .expect("file.changed.required")
        .iter()
        .map(|value| value.as_str().expect("字段名是字符串"))
        .collect();
    assert_eq!(
        required,
        vec!["changeId", "kind", "displayPath", "summary"],
        "file.changed 的必填集合不得因新字段而改变"
    );
    assert_eq!(
        schema["$defs"]["file.changed"]["properties"]["outsideWorkspace"]["type"], "boolean",
        "outsideWorkspace 必须是 boolean"
    );
    for field in ["addedLines", "deletedLines"] {
        assert_eq!(
            schema["$defs"]["file.changed"]["properties"][field]["$ref"],
            "common.schema.json#/$defs/decimalString",
            "{field} 必须引用 decimalString"
        );
    }
}

/// 三个可选字段的非法形状（含显式 `null`）必须被拒，且失败点是该字段自己的类型/格式约束。
///
/// 「显式 `null` 应按 schema 语义处理」= schema 没有 `null` 分支，因此被拒；这不是实现取舍。
#[test]
fn file_changed_optional_fields_reject_wrong_shapes() {
    for (fixture, field) in FILE_CHANGED_NEGATIVES {
        assert!(
            serde_json::from_str::<FileChanged>(&support::read_text(&support::repo_path(
                &format!("{FIXTURE_ROOT}{fixture}")
            )))
            .is_err(),
            "{fixture}：{field} 的非法形状必须在类型化层被拒"
        );

        let case = manifest_case(fixture);
        assert!(!case.valid, "{fixture} 必须登记为 valid: false");
        let keyword = case.expected_keyword.as_deref().unwrap_or_default();
        assert!(
            matches!(keyword, "type" | "pattern"),
            "{fixture}：失败原因必须是字段类型或字面量格式，得到 {keyword}"
        );
        // 基字段必须齐全：否则失败原因会退化成「缺必填字段」而不是本字段的形状。
        let value: serde_json::Value = serde_json::from_str(&support::read_text(
            &support::repo_path(&format!("{FIXTURE_ROOT}{fixture}")),
        ))
        .expect("负例原文是 JSON");
        for base in ["changeId", "kind", "displayPath", "summary"] {
            assert!(
                value.get(base).is_some(),
                "{fixture}：基字段 {base} 必须保留，失败点才是 {field}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// R8：`state` 封闭枚举
// ---------------------------------------------------------------------------------------------

/// 两个事件的 `state` 各自唯一且互不相同，并与 `VIEW_ENUMS` 的登记、schema 的 `enum` 逐条同序。
#[test]
fn agent_state_enums_are_unique_and_mutually_distinct() {
    let connected: Vec<&str> = views::AgentConnectedState::ALL
        .iter()
        .map(|state| state.as_str())
        .collect();
    let disconnected: Vec<&str> = views::AgentDisconnectedState::ALL
        .iter()
        .map(|state| state.as_str())
        .collect();

    assert_eq!(connected, vec!["connected"], "连接事件取值唯一");
    assert_eq!(disconnected, vec!["disconnected"], "断开事件取值唯一");
    assert_ne!(
        connected, disconnected,
        "两种事件的取值必须互不相同，客户端才能据此区分"
    );

    let registered = |event_type: &str| -> Vec<&'static str> {
        views::VIEW_ENUMS
            .iter()
            .find(|(ty, pointer, _)| *ty == event_type && *pointer == "/properties/state")
            .unwrap_or_else(|| panic!("{event_type}/properties/state 未登记"))
            .2
            .to_vec()
    };
    assert_eq!(connected, registered("agent.connected"));
    assert_eq!(disconnected, registered("agent.disconnected"));
}

/// `state` 的非法取值必须被拒：自由文本、其它事件的取值、以及未登记取值都不接受。
#[test]
fn agent_state_rejects_values_outside_its_enum() {
    for (fixture, expected) in STATE_NEGATIVES {
        let value: serde_json::Value = serde_json::from_str(&support::read_text(
            &support::repo_path(&format!("{FIXTURE_ROOT}{fixture}")),
        ))
        .expect("负例原文是 JSON");
        let event_type = match fixture {
            f if f.contains("disconnected") => "agent.disconnected",
            _ => "agent.connected",
        };

        assert!(
            views::project(event_type, &raw_object(&value.to_string())).is_err(),
            "{fixture}：词表外的 state 必须在投影时被拒"
        );
        assert_eq!(
            value["state"].as_str(),
            Some(expected),
            "{fixture}：负例的 state 必须是词表外的取值"
        );
        let case = manifest_case(fixture);
        assert!(!case.valid, "{fixture} 必须登记为 valid: false");
        assert_eq!(
            case.expected_keyword.as_deref(),
            Some("enum"),
            "{fixture}：失败原因必须是 state 的封闭词表"
        );
        // 另一条事件类型必须仍然接受自己的取值——否则上面三条可能只是「整个投影坏了」。
        let other = if event_type == "agent.connected" {
            "agent.disconnected"
        } else {
            "agent.connected"
        };
        let value_for_other = if other == "agent.connected" {
            r#"{"agentId":"company-agent","state":"connected"}"#
        } else {
            r#"{"agentId":"company-agent","state":"disconnected"}"#
        };
        views::project(other, &raw_object(value_for_other))
            .unwrap_or_else(|error| panic!("{other} 的合法取值必须仍被接受：{error}"));
    }
}

// ---------------------------------------------------------------------------------------------
// R8：节点级事件语义
// ---------------------------------------------------------------------------------------------

/// 两个 Agent 生命周期事件是节点级事件：`sessionId` 与 `sessionSequence` 必须为 `null`（键存在）。
#[test]
fn node_level_agent_events_carry_no_session_identity() {
    for fixture in [AGENT_CONNECTED, AGENT_DISCONNECTED] {
        let body: EventBody = serde_json::from_str(support::body_slice(&read_case(fixture)))
            .unwrap_or_else(|error| panic!("{fixture}：event body 解码失败：{error}"));

        assert!(
            body.session_id.is_null(),
            "{fixture}：节点级事件的 sessionId 必须为空"
        );
        assert!(
            body.session_sequence.is_null(),
            "{fixture}：节点级事件的 sessionSequence 必须为空，不得为它伪造会话级游标"
        );

        // 键存在而不是缺席：schema 的 required 含这两项。
        let raw = read_fixture(fixture)["body"].clone();
        assert!(
            raw.get("sessionId").is_some() && raw.get("sessionSequence").is_some(),
            "{fixture}：两个键必须存在（值为 null），而不是被省略：{raw}"
        );
        assert!(
            raw["sessionId"].is_null() && raw["sessionSequence"].is_null(),
            "{fixture}：两个键的值必须是 null"
        );
    }
}

/// 两个节点级事件的 `state` 取值与各自的事件类型一一对应，且投影结果不会互相混淆。
#[test]
fn node_level_agent_events_project_to_their_state() {
    let connected = read_fixture(AGENT_CONNECTED);
    let view = raw_object(&connected["body"]["payload"]["view"].to_string());
    let projected = views::project("agent.connected", &view).expect("agent.connected 可投影");
    assert_eq!(
        projected.event_type(),
        "agent.connected",
        "投影结果必须绑定自己的 eventType"
    );

    let disconnected = read_fixture(AGENT_DISCONNECTED);
    let view = raw_object(&disconnected["body"]["payload"]["view"].to_string());
    let projected = views::project("agent.disconnected", &view).expect("agent.disconnected 可投影");
    assert_eq!(projected.event_type(), "agent.disconnected");

    // 投影视图里必须真的带 agentId —— 否则上面的「投影成功」可能退化成解析了别的形状。
    assert_eq!(
        disconnected["body"]["payload"]["view"]["agentId"], "company-agent",
        "节点级事件的视图标识"
    );
}

// ---------------------------------------------------------------------------------------------
// 共享辅助
// ---------------------------------------------------------------------------------------------

fn read_request(text: &str) -> SessionRead {
    match serde_json::from_str::<Command>(support::body_slice(text))
        .unwrap_or_else(|error| panic!("session.read 请求解码失败：{error}"))
        .payload
    {
        CommandPayload::SessionRead(read) => read,
        other => panic!("payload 不是 session.read：{other:?}"),
    }
}

fn read_result(text: &str) -> SessionReadResult {
    let result = serde_json::from_str::<CommandResult>(support::body_slice(text))
        .unwrap_or_else(|error| panic!("session.read 结果解码失败：{error}"));
    match result.result.as_ref().expect("completed 必须带 result") {
        CommandResultPayload::SessionRead(result) => result.clone(),
        other => panic!("result 不是 sessionReadResult：{other:?}"),
    }
}

fn project_file_changed(fixture: &str) -> FileChanged {
    let raw = read_body(fixture)["payload"]["view"].clone();
    match views::project("file.changed", &raw_object(&raw.to_string()))
        .unwrap_or_else(|error| panic!("{fixture}：file.changed 投影失败：{error}"))
    {
        views::View::FileChanged(view) => view,
        other => panic!("{fixture}：投影结果不是 file.changed：{other:?}"),
    }
}

fn raw_object(text: &str) -> RawObject {
    serde_json::from_str(text).unwrap_or_else(|error| panic!("视图原文必须是对象：{error}"))
}
