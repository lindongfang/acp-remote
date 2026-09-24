//! 矩阵登记表与 `compatibility/acp/v1/matrix.json` 的逐条比对。
//!
//! 这是 `docs/ACP_COMPATIBILITY_MATRIX.md` §4 要求的「Rust 测试读取同一矩阵或引用相同 row/test id」：
//! `methods.rs` 与 `SESSION_UPDATE_WIRE_VALUES` 是矩阵在 Rust 侧的手工镜像，一旦矩阵更新而镜像没跟，
//! 本测试就会失败——所以「Rust 侧另有一份隐式支持列表」不可能悄悄存在。

mod support;

use std::collections::BTreeSet;

use acp_protocol::capability;
use acp_protocol::methods::{
    self, Delivery, METHODS, MethodDirection, MethodKind, MethodStatus, SESSION_UPDATE_WIRE_VALUES,
};

fn direction_from_wire(text: &str) -> MethodDirection {
    match text {
        "client_to_agent" => MethodDirection::ClientToAgent,
        "agent_to_client" => MethodDirection::AgentToClient,
        "bidirectional" => MethodDirection::Bidirectional,
        other => panic!("矩阵出现未登记方向：{other}"),
    }
}

fn kind_from_wire(text: &str) -> MethodKind {
    match text {
        "request" => MethodKind::Request,
        "notification" => MethodKind::Notification,
        other => panic!("矩阵出现未登记类别：{other}"),
    }
}

fn delivery_from_wire(text: &str) -> Delivery {
    match text {
        "mvp" => Delivery::Mvp,
        "conditional_mvp" => Delivery::ConditionalMvp,
        "post_mvp" => Delivery::PostMvp,
        other => panic!("矩阵出现未登记交付节奏：{other}"),
    }
}

#[test]
fn methods_table_matches_matrix() {
    let matrix = support::matrix();
    let rows = matrix["methods"].as_array().expect("matrix.methods");
    assert_eq!(
        rows.len(),
        METHODS.len(),
        "方法登记表条数与矩阵不一致：矩阵 {} 条，Rust 侧 {} 条",
        rows.len(),
        METHODS.len()
    );

    for row in rows {
        let wire_name = row["wireName"].as_str().expect("wireName");
        let spec = methods::find(wire_name)
            .unwrap_or_else(|| panic!("矩阵方法 {wire_name} 不在 Rust 登记表里"));
        assert_eq!(
            spec.direction,
            direction_from_wire(row["direction"].as_str().expect("direction"))
        );
        assert_eq!(
            spec.kind,
            kind_from_wire(row["kind"].as_str().expect("kind"))
        );
        assert_eq!(
            spec.delivery,
            delivery_from_wire(row["delivery"].as_str().expect("delivery"))
        );
    }

    // 方向不是摆设：两侧各至少一条，双向成立。
    assert!(
        METHODS
            .iter()
            .any(|m| m.direction == MethodDirection::ClientToAgent)
    );
    assert!(
        METHODS
            .iter()
            .any(|m| m.direction == MethodDirection::AgentToClient)
    );
    assert!(
        METHODS
            .iter()
            .any(|m| m.direction == MethodDirection::Bidirectional)
    );
}

#[test]
fn post_mvp_methods_are_not_claimed_as_implemented() {
    let matrix = support::matrix();
    for row in matrix["methods"].as_array().expect("matrix.methods") {
        let wire_name = row["wireName"].as_str().expect("wireName");
        let delivery = row["delivery"].as_str().expect("delivery");
        let spec = methods::find(wire_name).expect("登记表已在上一个测试中比对");
        if delivery == "post_mvp" {
            assert!(
                !spec.implemented,
                "{wire_name} 是 post_mvp，不得声称已实现（facade 也不得宣告）"
            );
            assert_eq!(methods::status_of(wire_name), MethodStatus::NotImplemented);
        }
        // post_mvp 行必须同时是 facade=not_advertised（矩阵 schema 已断言），Rust 侧只需保证不虚报。
        if delivery == "post_mvp" {
            assert_eq!(row["layers"]["facade"], "not_advertised");
        }
    }
}

#[test]
fn session_update_table_matches_matrix() {
    let matrix = support::matrix();
    let rows = matrix["sessionUpdates"]
        .as_array()
        .expect("matrix.sessionUpdates");
    assert_eq!(
        rows.len(),
        SESSION_UPDATE_WIRE_VALUES.len(),
        "判别子登记表条数与矩阵不一致"
    );
    for row in rows {
        let wire_value = row["wireValue"].as_str().expect("wireValue");
        assert!(
            methods::is_known_session_update(wire_value),
            "矩阵判别子 {wire_value} 不在 Rust 登记表里"
        );
    }
    for wire_value in SESSION_UPDATE_WIRE_VALUES {
        assert!(
            rows.iter().any(|row| row["wireValue"] == *wire_value),
            "Rust 登记表里的 {wire_value} 不在矩阵里"
        );
    }
    assert_eq!(
        SESSION_UPDATE_WIRE_VALUES.len(),
        11,
        "第一阶段要求 11 种判别子"
    );
}

#[test]
fn status_and_direction_lookups_agree_with_the_table() {
    for spec in METHODS {
        match methods::status_of(spec.wire_name) {
            MethodStatus::Implemented => assert!(spec.implemented),
            MethodStatus::NotImplemented => assert!(!spec.implemented),
            other => panic!("已登记方法 {} 的状态不可能是 {other:?}", spec.wire_name),
        }
        assert_eq!(methods::direction_of(spec.wire_name), Some(spec.direction));
    }
    assert_eq!(
        methods::status_of("_extension/example"),
        MethodStatus::Extension
    );
    assert_eq!(methods::status_of("example/unknown"), MethodStatus::Unknown);
    assert_eq!(methods::direction_of("_extension/example"), None);
}

#[test]
fn invariant_fixtures_exist() {
    let matrix = support::matrix();
    for invariant in matrix["invariants"].as_array().expect("matrix.invariants") {
        let fixture = invariant["fixture"].as_str().expect("fixture");
        let path = support::repo_path("compatibility/acp/v1").join(fixture);
        let path = path.canonicalize().unwrap_or_else(|error| {
            panic!(
                "不变量 {} 的 fixture 不存在：{}（{error}）",
                invariant["id"],
                path.display()
            )
        });
        assert!(path.is_file());
        assert!(
            invariant["expectation"].as_str().is_some(),
            "不变量必须声明 expectation"
        );
    }
}

/// 能力路径表必须与矩阵的 `capabilities[]` **逐条**对应（既不能少登记，也不能多登记）。
#[test]
fn capability_paths_match_the_matrix() {
    let matrix = support::matrix();
    let rows = matrix["capabilities"]
        .as_array()
        .expect("capabilities 必须是数组");
    let declared: BTreeSet<(String, &str)> = capability::CAPABILITY_PATHS
        .iter()
        .map(|row| {
            let side = match row.advertised_by {
                capability::CapabilityAdvertiser::Client => "client",
                capability::CapabilityAdvertiser::Agent => "agent",
            };
            (row.path.to_owned(), side)
        })
        .collect();
    let from_matrix: BTreeSet<(String, &str)> = rows
        .iter()
        .map(|row| {
            (
                row["path"].as_str().expect("path").to_owned(),
                row["advertisedBy"].as_str().expect("advertisedBy"),
            )
        })
        .collect();
    assert_eq!(
        declared, from_matrix,
        "CAPABILITY_PATHS 必须与 compatibility/acp/v1/matrix.json 的 capabilities[] 完全一致"
    );
    // 表里不能有重复路径。
    assert_eq!(
        declared.len(),
        capability::CAPABILITY_PATHS.len(),
        "能力路径不得重复登记"
    );
}

/// 未宣告能力的 Agent 必须得到空集合；宣告了的必须逐条出现（不虚报、不遗漏）。
#[test]
fn declared_capability_paths_follow_the_declaration() {
    let none = agent_host_capabilities_helper(&serde_json::json!({}));
    assert!(none.is_empty());
    let some = agent_host_capabilities_helper(&serde_json::json!({
        "loadSession": true,
        "mcpCapabilities": { "http": true },
        "sessionCapabilities": { "resume": {} },
    }));
    assert_eq!(
        some,
        vec![
            "agentCapabilities.loadSession",
            "agentCapabilities.mcpCapabilities.http",
            "agentCapabilities.sessionCapabilities.resume",
        ]
    );
    // `false` 与缺省都不算宣告。
    let falsy = agent_host_capabilities_helper(&serde_json::json!({ "loadSession": false }));
    assert!(falsy.is_empty());
}

fn agent_host_capabilities_helper(json: &serde_json::Value) -> Vec<&'static str> {
    let capabilities: capability::AgentCapabilities =
        serde_json::from_value(json.clone()).expect("能力声明");
    capabilities.declared_capability_paths()
}
