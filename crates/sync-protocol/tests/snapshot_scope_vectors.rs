//! TP1 合同向量：快照资源范围与 `session.read` 分页形状。
//!
//! 本文件只钉**协议形状**，不钉任何生产端行为：快照由谁组装、分页由谁实现、客户端如何翻页
//! 分别属于 WP3（core 生产者）与 WP5–WP7（前端），这里一概不触碰。
//!
//! 断言分三组，对应 `specs/sync-snapshot-scope/spec.md` 的三组要求：
//!
//! 1. 「快照只承载清单类资源」——五类会话明细资源 `messages`/`turns`/`pending_interactions`/
//!    `config_options`/`capabilities` 逐条不得出现在 `sync.snapshot_chunk.resource` 的封闭词表内，
//!    且逐条都有登记为 `invalid` 的 chunk 负例。逐条登记（而不是只断言词表长度）是为了让
//!    「把某个取值偷偷放回枚举」这类回归在**该取值自己那条负例**上立刻变红。
//! 2. 「会话明细经分页读取」与「游标由时间与标识复合构成」——默认页、复合游标、`hasEarlier`
//!    真假两态、单字段游标、缺 `hasEarlier` 的结果，全部按 wire 形状固定。
//! 3. 相邻两页的拼接性质：游标等于上一页最早一条的 `(createdAt, messageId)`，下一页整体严格
//!    更早且与上一页无交集——「不重不漏」在形状层面可判定的部分。

mod support;

use std::collections::BTreeSet;

use sync_protocol::command::{
    Command, CommandPayload, CommandResult, CommandResultPayload, SessionRead, SessionReadResult,
};
use sync_protocol::common::ValueError;
use sync_protocol::sync::{SnapshotChunk, SnapshotResource};

const MANIFEST: &str = "fixtures/sync/v1/manifest.json";
const FIXTURE_ROOT: &str = "fixtures/sync/v1/";

/// 快照里禁止出现的五类会话明细资源，各带一条登记为 `invalid` 的 chunk 负例。
///
/// `messages` 与 `config_options` 的负例由 WP1 交付，本表把它们纳入同一判据——否则那两条会随
/// WP1 的文件一起漂移；而 `turns`/`pending_interactions`/`capabilities` 三条由本 TP 新增。四条
/// 分属不同 worktree、五条合成一个封闭词表的完整否证，缺任何一条覆盖都是残缺的。
const FORBIDDEN_IN_SNAPSHOT: [(&str, &str); 5] = [
    (
        "messages",
        "invalid/snapshot-chunk-detail-resource-messages.json",
    ),
    ("turns", "invalid/snapshot-chunk-detail-resource-turns.json"),
    (
        "pending_interactions",
        "invalid/snapshot-chunk-detail-resource-pending-interactions.json",
    ),
    (
        "config_options",
        "invalid/snapshot-chunk-detail-resource-config-options.json",
    ),
    (
        "capabilities",
        "invalid/snapshot-chunk-detail-resource-capabilities.json",
    ),
];

/// 本 TP 新增的三条 chunk 负例：`items` 是**非空**且逐字段合法的元素数组。
///
/// 因此它们唯一的失败原因是 `resource` 不在封闭词表内；一旦有人把该取值放回枚举（并补回对应的
/// `if`/`then`），这三条会立刻变成合法样例，`check-schema-fixtures` 与本文件同时变红。
const POPULATED_NEGATIVES: [&str; 3] = ["turns", "pending_interactions", "capabilities"];

/// 三类清单资源的封闭词表：少一条是回归，多一条同样是回归。
const CATALOG_RESOURCES: [&str; 3] = ["sessions", "workspaces", "agents"];

const DEFAULT_PAGE_REQUEST: &str = "valid/command-session-read-default-page-include-only.json";
const FIRST_PAGE_RESULT: &str = "valid/command-result-session-read-first-page-completed.json";
const SECOND_PAGE_REQUEST: &str = "valid/command-session-read-second-page.json";
const EARLIEST_PAGE_RESULT: &str = "valid/command-result-session-read-earliest-page-completed.json";
const MISSING_MESSAGE_ID_CURSOR: &str =
    "invalid/command-session-read-before-missing-message-id.json";
const MISSING_HAS_EARLIER: &str =
    "invalid/command-result-session-read-result-without-has-earlier.json";

const SESSION: &str = "5d73cd10-a465-43cd-b3f1-704e2d49e99e";
/// 第二页游标指向的那条消息（第一页最早一条）的标识。
const CURSOR_MESSAGE: &str = "c47a90b2-6e35-4d18-8f26-9b0d3e7a5c14";
const CURSOR_CREATED_AT: &str = "2026-09-17T11:58:00.000Z";

fn read_case(relative: &str) -> String {
    support::read_text(&support::repo_path(&format!("{FIXTURE_ROOT}{relative}")))
}

fn read_body_json(relative: &str) -> serde_json::Value {
    let text = read_case(relative);
    serde_json::from_str(support::body_slice(&text)).expect("fixture 的 body 是 JSON")
}

fn manifest_case(fixture: &str) -> support::ManifestCase {
    support::manifest_cases(MANIFEST)
        .into_iter()
        .find(|case| case.fixture == fixture)
        .unwrap_or_else(|| panic!("{fixture}：manifest.json 未登记该 fixture"))
}

fn session_read(text: &str) -> SessionRead {
    let command = serde_json::from_str::<Command>(support::body_slice(text))
        .unwrap_or_else(|error| panic!("session.read 请求解码失败：{error}"));
    match command.payload {
        CommandPayload::SessionRead(read) => read,
        other => panic!("command 的 payload 不是 session.read：{other:?}"),
    }
}

fn session_read_result(text: &str) -> SessionReadResult {
    let command_result = serde_json::from_str::<CommandResult>(support::body_slice(text))
        .unwrap_or_else(|error| panic!("session.read 结果解码失败：{error}"));
    match command_result
        .result
        .as_ref()
        .expect("completed 结果必须带 result")
    {
        CommandResultPayload::SessionRead(result) => result.clone(),
        other => panic!("result 不是 sessionReadResult：{other:?}"),
    }
}

fn message_ids(result: &SessionReadResult) -> Vec<String> {
    result
        .resources
        .messages
        .as_ref()
        .expect("本组向量都带 messages")
        .iter()
        .map(|message| message.message_id.as_str().to_owned())
        .collect()
}

fn message_times(result: &SessionReadResult) -> Vec<String> {
    result
        .resources
        .messages
        .as_ref()
        .expect("本组向量都带 messages")
        .iter()
        .map(|message| message.created_at.as_str().to_owned())
        .collect()
}

/// 快照的封闭词表只含三类清单资源，五类会话明细资源逐条不在其中。
#[test]
fn snapshot_resource_enum_excludes_every_session_detail_resource() {
    let declared: Vec<&str> = SnapshotResource::ALL
        .iter()
        .map(|resource| resource.as_str())
        .collect();
    assert_eq!(
        declared, CATALOG_RESOURCES,
        "快照资源的封闭词表必须恰好是三类清单资源"
    );

    for (resource, fixture) in FORBIDDEN_IN_SNAPSHOT {
        match resource.parse::<SnapshotResource>() {
            Err(ValueError::Enumerated { field, value }) => {
                assert_eq!(
                    field, "resource",
                    "{resource}（{fixture}）：失败原因必须是 resource 不在封闭词表内"
                );
                assert_eq!(value, resource, "{resource}：错误里回显的取值");
            }
            Err(other) => panic!("{resource}（{fixture}）：期望封闭词表错误，得到 {other:?}"),
            Ok(parsed) => {
                panic!("{resource}（{fixture}）：会话明细资源不得是合法快照资源，却解析成 {parsed}")
            }
        }
    }
}

/// 五类会话明细资源各有一条**已登记**的 chunk 负例，且每条都因 `resource` 而失败。
///
/// 三重判据：manifest 声明 `valid: false` + `expectedKeyword: "enum"`（ajv 层）、类型化层解码
/// 必须失败（Rust 层）、`resource` 键本身的取值必须解析不出（失败原因）。缺任一条，这条负例就
/// 可能是因为别的原因在失败，或者根本没被登记。
#[test]
fn every_forbidden_detail_resource_has_a_registered_negative_chunk_fixture() {
    for (resource, fixture) in FORBIDDEN_IN_SNAPSHOT {
        let case = manifest_case(fixture);
        assert!(
            !case.valid,
            "{fixture}：负例必须在 manifest 登记为 valid: false"
        );
        assert_eq!(
            case.expected_keyword.as_deref(),
            Some("enum"),
            "{fixture}：失败原因必须是 snapshotResource 的封闭词表"
        );

        let body = read_body_json(fixture);
        assert_eq!(
            body["resource"].as_str(),
            Some(resource),
            "{fixture}：负例的 resource 取值"
        );
        assert!(
            serde_json::from_str::<SnapshotChunk>(&body.to_string()).is_err(),
            "{fixture}：五类会话明细资源的 chunk 必须在类型化层被拒"
        );
        assert!(
            body["resource"]
                .as_str()
                .expect("resource 是字符串")
                .parse::<SnapshotResource>()
                .is_err(),
            "{fixture}：失败点必须是 resource 词表，而不是 items 的形状"
        );

        if POPULATED_NEGATIVES.contains(&resource) {
            let items = body["items"].as_array().expect("items 是数组");
            assert!(
                !items.is_empty(),
                "{fixture}：本 TP 新增的负例必须带非空且合法的元素——\
                 否则把 resource 放回枚举后它依然非法，这条向量就证明不了枚举收窄"
            );
        }
    }
}

/// 省略 `before` 与 `limit` 的默认页请求必须合法，且两个分页键都**不存在**（而不是取默认值）。
///
/// 「键缺失」与「键存在但为空」在 wire 上是不同的东西：服务端 MUST NOT 要求客户端显式给出页大小，
/// 而 `session.read` 的 payload 只 `required: ["include"]`。
#[test]
fn default_page_request_omits_both_paging_parameters() {
    let read = session_read(&read_case(DEFAULT_PAGE_REQUEST));
    assert!(
        read.before.is_none(),
        "{DEFAULT_PAGE_REQUEST}：默认页请求不得带 before 游标"
    );
    assert!(
        read.limit.is_none(),
        "{DEFAULT_PAGE_REQUEST}：默认页请求不得带 limit"
    );

    // 直接读原始 body：证明这不是「带了 null 键后被解析成 None」。
    let payload = read_body_json(DEFAULT_PAGE_REQUEST)["payload"].clone();
    assert!(
        payload.get("before").is_none() && payload.get("limit").is_none(),
        "默认页请求的 payload 里出现了 before/limit 键：{payload}"
    );
    assert!(
        payload.get("include").is_some(),
        "include 是 session.read 唯一的必填键：{payload}"
    );
}

/// 复合游标必须同时携带 `createdAt` 与 `messageId`，`limit` 是客户端请求的单页上限。
#[test]
fn paged_request_carries_the_composite_cursor() {
    let read = session_read(&read_case(SECOND_PAGE_REQUEST));
    let before = read
        .before
        .as_ref()
        .unwrap_or_else(|| panic!("{SECOND_PAGE_REQUEST}：分页请求必须带 before 游标"));
    assert_eq!(
        before.created_at.as_str(),
        CURSOR_CREATED_AT,
        "游标的时间分量"
    );
    assert_eq!(
        before.message_id.as_str(),
        CURSOR_MESSAGE,
        "游标的消息标识分量"
    );
    assert_eq!(
        read.limit.map(|limit| limit.get()),
        Some(20),
        "limit 是客户端请求的单页上限"
    );
}

/// 单字段游标（只给 `messageId` 或只给 `createdAt`）必须被拒，不得被猜测或补全。
///
/// 两个方向都固定：fixture 一条（缺 `messageId`，本 TP 新增），内联 body 一条（缺 `createdAt`，
/// 与 WP1 交付的 `invalid/command-session-read-single-field-before.json` 合成一对）。
/// `sessionReadBefore` 的 `required` 是两个键，缺任一个都是同一个合同错误。
#[test]
fn single_field_cursor_is_rejected() {
    let missing_message_id = MISSING_MESSAGE_ID_CURSOR;
    assert!(
        serde_json::from_str::<Command>(support::body_slice(&read_case(missing_message_id)))
            .is_err(),
        "{missing_message_id}：缺 messageId 的单字段游标必须被拒"
    );
    assert!(
        read_body_json(missing_message_id)["payload"]["before"]
            .get("messageId")
            .is_none(),
        "{missing_message_id}：失败点确实是 messageId 键缺失"
    );

    let only_message_id = format!(
        r#"{{"requestId":"e7c2a934-5b18-4d60-9f37-2a6b8d0c4e51","command":"session.read","sessionId":"{SESSION}","payload":{{"include":["messages"],"before":{{"messageId":"{CURSOR_MESSAGE}"}},"limit":20}}}}"#
    );
    serde_json::from_str::<Command>(&only_message_id).expect_err(
        "只给 messageId 的单字段游标必须被拒（sessionReadBefore 的 required 是两个键）",
    );

    // 判据的另一半：两个键齐全时同一形状必须被接受，否则上面两条可能只是「整个 payload 被拒」。
    let complete = format!(
        r#"{{"include":["messages"],"before":{{"createdAt":"{CURSOR_CREATED_AT}","messageId":"{CURSOR_MESSAGE}"}},"limit":20}}"#
    );
    serde_json::from_str::<SessionRead>(&complete)
        .expect("两个键齐全的游标必须被接受——否则单字段游标的拒绝不是由游标形状造成的");
}

/// `session.read` 的完成结果必须带 `hasEarlier`，且真假两态都能表达。
#[test]
fn session_read_result_declares_whether_earlier_content_exists() {
    let first = session_read_result(&read_case(FIRST_PAGE_RESULT));
    assert!(
        first.has_earlier,
        "{FIRST_PAGE_RESULT}：还有更早内容时 hasEarlier 必须为 true"
    );

    let earliest = session_read_result(&read_case(EARLIEST_PAGE_RESULT));
    assert!(
        !earliest.has_earlier,
        "{EARLIEST_PAGE_RESULT}：已到最早一条时 hasEarlier 必须为 false"
    );
    // 到达最早一条时不得返回空占位：这一页必须带着真实内容，而不是一个空数组。
    let page = earliest
        .resources
        .messages
        .as_ref()
        .expect("最早一页必须带 messages");
    assert!(
        !page.is_empty(),
        "{EARLIEST_PAGE_RESULT}：hasEarlier=false 时不得返回空占位页"
    );
    assert!(
        page.iter()
            .all(|message| message.session_id.as_str() == SESSION),
        "最早一页的元素都属于被读取的会话"
    );

    // 缺 `hasEarlier` 的结果必须被拒：键缺失不得被读成「没有更早内容」。
    let body = support::body_slice(&read_case(MISSING_HAS_EARLIER)).to_owned();
    assert!(
        serde_json::from_str::<CommandResult>(&body).is_err(),
        "{MISSING_HAS_EARLIER}：缺 hasEarlier 的完成结果必须被拒"
    );
    let raw: serde_json::Value = serde_json::from_str(&body).expect("原文是 JSON");
    assert!(
        raw["result"].get("hasEarlier").is_none(),
        "{MISSING_HAS_EARLIER}：失败点确实是 hasEarlier 键缺失，而不是别处"
    );
    assert!(
        !raw["result"]["resources"]["messages"]
            .as_array()
            .expect("messages 是数组")
            .is_empty(),
        "{MISSING_HAS_EARLIER}：本页非空——因此被拒的原因不是「空页」，只能是缺 hasEarlier"
    );
}

/// 相邻两页的拼接性质：第二页游标 = 第一页最早一条；两页按 `createdAt` 升序、无交集、严格更早。
#[test]
fn adjacent_pages_join_without_gaps_or_overlap() {
    let cursor = session_read(&read_case(SECOND_PAGE_REQUEST))
        .before
        .clone()
        .expect("第二页请求必须带游标");
    let first = session_read_result(&read_case(FIRST_PAGE_RESULT));
    let second = session_read_result(&read_case(EARLIEST_PAGE_RESULT));

    let first_ids = message_ids(&first);
    let second_ids = message_ids(&second);
    let first_times = message_times(&first);
    let second_times = message_times(&second);

    // 每一页内部按 createdAt 升序。
    assert!(
        first_times.windows(2).all(|pair| pair[0] <= pair[1]),
        "第一页必须按 createdAt 升序：{first_times:?}"
    );
    assert!(
        second_times.windows(2).all(|pair| pair[0] <= pair[1]),
        "第二页必须按 createdAt 升序：{second_times:?}"
    );

    // 游标就是上一页最早一条的 (createdAt, messageId)。
    let earliest_first = first
        .resources
        .messages
        .as_ref()
        .and_then(|messages| messages.first())
        .expect("第一页非空");
    assert_eq!(
        (
            earliest_first.created_at.as_str(),
            earliest_first.message_id.as_str()
        ),
        (CURSOR_CREATED_AT, CURSOR_MESSAGE),
        "第二页游标必须由第一页最早一条构造"
    );
    assert_eq!(
        (cursor.created_at.as_str(), cursor.message_id.as_str()),
        (
            earliest_first.created_at.as_str(),
            earliest_first.message_id.as_str()
        ),
        "分页请求里的游标与上一页最早一条不一致"
    );

    // 拼接后覆盖两页全部消息且互不重叠。
    let union: BTreeSet<&String> = first_ids.iter().chain(second_ids.iter()).collect();
    assert_eq!(
        union.len(),
        first_ids.len() + second_ids.len(),
        "两页之间出现重复消息：{first_ids:?} / {second_ids:?}"
    );
    let newest_second = second_times.last().expect("第二页非空");
    assert!(
        newest_second.as_str() < CURSOR_CREATED_AT,
        "第二页必须整体早于游标时间：{second_times:?} vs {CURSOR_CREATED_AT}"
    );
    assert!(
        !second_ids.contains(&CURSOR_MESSAGE.to_owned()),
        "游标指向的消息不得再次出现在下一页里"
    );

    // 同一 createdAt 的多条消息按稳定次序排列，且该次序不是 messageId 的字典序——
    // 合同只要求「稳定且可重复」，本向量固定其中一种，并显式证明它不是按标识排序得到的。
    let tied: Vec<String> = first
        .resources
        .messages
        .as_ref()
        .expect("第一页带 messages")
        .iter()
        .filter(|message| message.created_at.as_str() == CURSOR_CREATED_AT)
        .map(|message| message.message_id.as_str().to_owned())
        .collect();
    assert_eq!(
        tied.len(),
        2,
        "第一页固定了同一 createdAt 的两条消息，用于钉住同刻次序：{tied:?}"
    );
    let mut lexicographic = tied.clone();
    lexicographic.sort();
    assert_ne!(
        tied, lexicographic,
        "同刻次序恰好等于字典序，这条向量就不再证明「排序不依赖标识」——请换一组 messageId"
    );
}

/// 「分页只作用于客户端同步面」：Node Link 面的 `session.read` 载荷形状不受 Sync 新增分页参数影响。
///
/// 判据取自机器资产而不是实现细节：`schemas/node-link/v1/command.schema.json` 里每个声明了
/// `include` 的载荷对象，其键集必须恰好是 `{ include }` 且 `additionalProperties: false`。于是
/// 「Node Link 调用方不带分页参数照常工作」与「它一旦带上分页参数就被拒」同时被钉住——Sync 面
/// 新增 `before`/`limit` 不会顺着 `$ref` 或复制粘贴渗进 Node Link 面。
#[test]
fn node_link_session_read_payload_carries_no_paging_parameters() {
    let document = support::read_json(&support::repo_path(
        "schemas/node-link/v1/command.schema.json",
    ));
    let mut include_payloads = Vec::new();
    collect_include_payloads(&document, &mut include_payloads);

    assert!(
        !include_payloads.is_empty(),
        "node-link command.schema.json 里没有找到声明 include 的载荷——本断言会空转，必须重写"
    );
    for (keys, closed) in &include_payloads {
        assert_eq!(
            keys,
            &["include".to_owned()],
            "Node Link 面的 session.read 载荷只允许 include，不得出现分页参数"
        );
        assert!(
            *closed,
            "Node Link 面的载荷必须 additionalProperties: false，否则分页参数会被静默接受"
        );
    }
}

/// 递归收集文档里每个「`required` 含 `include` 且 `properties` 含 `include`」的载荷对象：
/// 键集，以及它是否 `additionalProperties: false`。
fn collect_include_payloads(node: &serde_json::Value, found: &mut Vec<(Vec<String>, bool)>) {
    // `allOf`/`oneOf`/`anyOf` 都是数组：载荷定义常藏在数组项里，只走对象键会漏掉它们。
    if let Some(items) = node.as_array() {
        for value in items {
            collect_include_payloads(value, found);
        }
        return;
    }
    let Some(object) = node.as_object() else {
        return;
    };
    let properties = object
        .get("properties")
        .and_then(serde_json::Value::as_object);
    let requires_include = object
        .get("required")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|required| required.iter().any(|item| item == "include"));
    if requires_include && properties.is_some_and(|props| props.contains_key("include")) {
        let mut keys: Vec<String> = properties
            .expect("已判定 properties 存在")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        let closed = object
            .get("additionalProperties")
            .is_some_and(|value| value == &serde_json::Value::Bool(false));
        found.push((keys, closed));
    }
    for value in object.values() {
        collect_include_payloads(value, found);
    }
}
