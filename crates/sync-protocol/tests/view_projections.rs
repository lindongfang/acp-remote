//! view 投影层：`event.payload.view` 必须能按 `eventType` 投影成类型化视图。
//!
//! 夹具的每条 `viewDef` 都声明了该 `payload.view` 在 `schemas/sync/v1/event-views.schema.json`
//! 里对应的 `$defs` 条目；本测试要求投影结果落在**同一个** event 类型上，而不是"能解析成某个
//! 视图"——后者在 34 个形状相近的视图之间几乎必然假通过。
//!
//! `views::VIEW_TYPES` 与 schema `$defs` 的逐条相等由 `tests/schema_drift.rs` 断言；本文件要求的
//! 是另一半：**每个登记的视图都有夹具覆盖**，且夹具声明的绑定与投影结果一致。

mod support;

use std::collections::BTreeSet;

use sync_protocol::common::RawObject;
use sync_protocol::envelope::Envelope;
use sync_protocol::event::Body;
use sync_protocol::views;

const FIXTURE_ROOT: &str = "fixtures/sync/v1/";
const MANIFEST: &str = "fixtures/sync/v1/manifest.json";

/// 声明了 `viewDef` 的夹具数（`event-remote-origin.json` 与三条事件夹具复用同一批 event 类型）。
const EXPECTED_VIEW_CASES: usize = 36;
/// 被夹具覆盖的**不同** event 类型数，等于 `event-views.schema.json` 的 `$defs` 条目数。
const EXPECTED_DISTINCT_VIEWS: usize = 34;

#[test]
fn every_view_fixture_projects_onto_its_declared_event_type() {
    let cases = support::manifest_cases(MANIFEST);
    let mut view_cases = 0;
    let mut projected: BTreeSet<String> = BTreeSet::new();

    for case in &cases {
        let Some(view_def) = case.view_def.as_deref() else {
            continue;
        };
        let event_type = view_def
            .strip_prefix("#/$defs/")
            .unwrap_or_else(|| panic!("{}：viewDef 形状不符：{view_def}", case.fixture));

        let text = support::read_text(&support::repo_path(&format!(
            "{FIXTURE_ROOT}{}",
            case.fixture
        )));
        let envelope = Envelope::decode(&text)
            .unwrap_or_else(|error| panic!("{}：声明的夹具信封必须合法：{error}", case.fixture));
        let body: Body = serde_json::from_str(envelope.body().get())
            .unwrap_or_else(|error| panic!("{}：event body 必须可解析：{error}", case.fixture));

        assert_eq!(
            body.event_type.as_str(),
            event_type,
            "{}：夹具的 eventType 与 viewDef 声明不一致",
            case.fixture
        );

        let view = views::project(body.event_type.as_str(), &body.payload.view)
            .unwrap_or_else(|error| panic!("{}：投影失败：{error}", case.fixture));
        assert_eq!(
            view.event_type(),
            event_type,
            "{}：投影出的视图类型与 viewDef 不一致",
            case.fixture
        );

        view_cases += 1;
        projected.insert(event_type.to_owned());
    }

    assert_eq!(view_cases, EXPECTED_VIEW_CASES, "带 viewDef 的夹具数变化");
    assert_eq!(
        projected.len(),
        EXPECTED_DISTINCT_VIEWS,
        "被覆盖的 view 类型数变化"
    );
    assert_eq!(
        views::VIEW_TYPES.len(),
        EXPECTED_DISTINCT_VIEWS,
        "登记的 view 类型数变化"
    );

    for registered in views::VIEW_TYPES {
        assert!(
            projected.contains(registered),
            "登记的 view {registered} 没有任何夹具覆盖"
        );
    }

    println!(
        "sync views: {view_cases} 条夹具投影到 {} 个登记视图，登记表 {} 项全部被覆盖",
        projected.len(),
        views::VIEW_TYPES.len()
    );
}

#[test]
fn unregistered_event_type_is_rejected_by_projection() {
    let empty = RawObject::empty();
    let error =
        views::project("session.nonexistent", &empty).expect_err("未登记的 eventType 必须被拒绝");
    assert!(
        error.to_string().contains("session.nonexistent"),
        "错误里必须带上被拒的 eventType：{error}"
    );
}

#[test]
fn view_keeps_unknown_fields_verbatim() {
    // 视图是开放对象（`additionalProperties: true`）：未来协商后的新字段必须被保留而不是静默丢弃，
    // 且其中的大整数不得被解析成浮点后改写。
    let fixture = support::repo_path(&format!("{FIXTURE_ROOT}valid/view-session-created.json"));
    let text = support::read_text(&fixture);
    let envelope = Envelope::decode(&text).expect("信封合法");
    let mut body: Body = serde_json::from_str(envelope.body().get()).expect("event body");

    let injected = format!(
        "{}, \"futureField\":{{\"nested\":[1,2,3],\"huge\":123456789012345678901234567890}}}}",
        body.payload.view.get().trim_end_matches('}')
    );
    body.payload.view = RawObject::parse(&injected).expect("注入后仍是 object");

    let view = views::project(body.event_type.as_str(), &body.payload.view).expect("投影");
    let serialized = serde_json::to_string(&view).expect("投影结果可序列化");

    let expected: serde_json::Value = serde_json::from_str(&injected).expect("注入后是 JSON");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&serialized).expect("重编码是 JSON"),
        expected,
        "投影不得增删 view 的字段"
    );
    assert!(
        serialized.contains("123456789012345678901234567890"),
        "未知字段里的大整数不得被改写"
    );
}
