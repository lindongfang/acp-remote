//! [PV3] R53–R68 / 任务 2.7：导出授权的展开与机器资产逐项一致。
//!
//! 唯一机器来源是 `compatibility/commands/v1/commands.json`：本文件既断言展开行为，
//! 也断言 `identity_auth::authorization` 里的两张表与它**逐项相等**（漂移即红）。
//!
//! 设备方向没有任何授权词汇（`pairing-model` 删除设备权限层）：本模块不再有设备侧入口，
//! 因此这里只断言「设备词汇在任何入口都无处可去」；设备配对请求集合的定向拒绝由 `tests/pairing.rs`
//! 用 `RequestedCapabilities` 断言。

mod support;

use identity_auth::AuthorizationError;
use identity_auth::authorization::{
    GRANTS, LOCAL_CAPABILITIES, command_names, expand_grant_scopes, expand_node_request,
};
use serde_json::Value;

macro_rules! commands_json {
    () => {
        include_str!("../../../compatibility/commands/v1/commands.json")
    };
}

fn catalog() -> Value {
    serde_json::from_str(commands_json!()).expect("命令目录必须是合法 JSON")
}

/// 目录里的一个对象（`grants`）转成 `(名称, 成员)` 列表。
fn table_of(catalog: &Value, key: &str) -> Vec<(String, Vec<String>)> {
    let object = catalog[key].as_object().expect("必须是对象");
    object
        .iter()
        .map(|(name, members)| {
            (
                name.clone(),
                members
                    .as_array()
                    .expect("成员必须是数组")
                    .iter()
                    .map(|item| item.as_str().expect("成员必须是字符串").to_owned())
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn grants_match_machine_catalog() {
    // R53 / 导出授权逐项相等（Node Link Export 的 scopes 用同一张表）。
    let catalog = catalog();
    let expected = table_of(&catalog, "grants");
    assert_eq!(GRANTS.len(), expected.len());
    for (name, members) in &expected {
        let actual =
            expand_grant_scopes(name).unwrap_or_else(|error| panic!("{name} 必须可展开：{error}"));
        assert_eq!(&actual, members, "{name} 的成员必须与目录一致");
    }
}

#[test]
fn local_capabilities_match_machine_catalog() {
    // R53 / 7 项本地能力逐项相等。
    let catalog = catalog();
    let expected: Vec<String> = catalog["localCapabilities"]
        .as_array()
        .expect("必须是数组")
        .iter()
        .map(|item| item.as_str().expect("必须是字符串").to_owned())
        .collect();
    assert_eq!(LOCAL_CAPABILITIES.len(), expected.len());
    assert_eq!(
        LOCAL_CAPABILITIES
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn command_names_cover_the_whole_catalog() {
    // R54 / 展开结果只可能是登记命令：`grants` 必须覆盖目录里的 14 条命令。
    let catalog = catalog();
    let mut expected: Vec<String> = catalog["commands"]
        .as_array()
        .expect("必须是数组")
        .iter()
        .map(|command| command["name"].as_str().expect("命令必须有名字").to_owned())
        .collect();
    expected.sort();
    let mut actual: Vec<String> = command_names()
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    actual.sort();
    assert_eq!(actual, expected, "命令词表必须与目录完全一致");
}

#[test]
fn unknown_names_are_rejected_without_partial_expansion() {
    // R57 / 未知或拼错的名称必须显式拒绝，不做部分展开。
    assert_eq!(
        expand_grant_scopes("grant.remote_work"),
        Err(AuthorizationError::UnknownName(
            "grant.remote_work".to_owned()
        ))
    );
    // 设备词汇（授权包与配对预设）在这里没有登记，因此与拼错名称同类：整体拒绝。
    for vocabulary in ["pack.observe", "pack.create-session", "preset.read-only"] {
        assert_eq!(
            expand_grant_scopes(vocabulary),
            Err(AuthorizationError::UnknownName(vocabulary.to_owned())),
            "{vocabulary} 不是导出授权，必须被拒绝"
        );
    }
    // 混合输入：只要有一个非法名称，整体失败（不返回已展开的部分）。
    let error = expand_node_request(&[
        "grant.observe".to_owned(),
        "grant.nope".to_owned(),
        "grant.interact".to_owned(),
    ])
    .expect_err("必须整体失败");
    assert_eq!(
        error,
        AuthorizationError::UnknownName("grant.nope".to_owned())
    );
}

#[test]
fn local_capabilities_are_never_granted_remotely() {
    // R58 / `local.*` 7 项在任何输入位置都被拒绝，也不参与展开。
    for capability in LOCAL_CAPABILITIES {
        assert_eq!(
            expand_grant_scopes(capability),
            Err(AuthorizationError::LocalCapability(
                (*capability).to_owned()
            ))
        );
        assert_eq!(
            expand_node_request(&[(*capability).to_owned()]),
            Err(AuthorizationError::LocalCapability(
                (*capability).to_owned()
            ))
        );
    }
    // 未登记的 `local.*` 同样拒绝（前缀即保留域）。
    assert_eq!(
        expand_node_request(&["local.unknown".to_owned()]),
        Err(AuthorizationError::LocalCapability(
            "local.unknown".to_owned()
        ))
    );
    // 目录里的 7 项必须都是 `local.` 前缀（映射关系不会张冠李戴）。
    for capability in LOCAL_CAPABILITIES {
        assert!(capability.starts_with("local."));
    }
}

#[test]
fn node_request_expands_grants_to_command_scopes() {
    // R62–R64 / 节点请求 = grant 名集合 + 其展开出的命令级 scope（Export 侧的 scope 表达同源）。
    let request =
        expand_node_request(&["grant.observe".to_owned(), "grant.remote-work".to_owned()])
            .expect("节点请求必须可展开");
    assert_eq!(request.grants.len(), 2);
    assert!(request.grants.contains("grant.remote-work"));
    assert!(request.scopes.contains("session.create"));
    assert!(request.scopes.contains("session.resume"));
    assert_eq!(request.scopes.len(), 8);
    // grant 名本身不是命令名，绝不进入 scope 集合。
    assert!(!request.scopes.contains("grant.observe"));
}

#[test]
fn empty_request_yields_empty_sets() {
    // R65 / 空请求是合法的空集合，而不是错误。
    let request = expand_node_request(&[]).expect("空请求必须合法");
    assert!(request.grants.is_empty());
    assert!(request.scopes.is_empty());
}

#[test]
fn expansion_is_stable_and_order_independent() {
    // R66 / 集合语义：同一集合不同输入顺序得到同一结果。
    let first = expand_node_request(&["grant.interact".to_owned(), "grant.observe".to_owned()])
        .expect("必须可展开");
    let second = expand_node_request(&["grant.observe".to_owned(), "grant.interact".to_owned()])
        .expect("必须可展开");
    assert_eq!(first, second);
}

#[test]
fn grant_members_are_registered_commands() {
    // R67 / 每个命令都必须能由目录登记的 grant 展开出来（覆盖关系不会张冠李戴）。
    let catalog = catalog();
    for command in catalog["commands"].as_array().expect("必须是数组") {
        let name = command["name"].as_str().expect("命令必须有名字");
        let grant = command["grant"].as_str().expect("命令必须有授权");
        assert!(
            expand_grant_scopes(grant)
                .expect("授权必须可展开")
                .contains(&name.to_owned()),
            "{name} 必须能由其授权 {grant} 展开出来"
        );
    }
}

#[test]
fn scope_names_are_not_grant_names() {
    // R68 / 命令名不是 grant 名：节点请求里的命令名必须作为未知名称被拒（两类集合不可互换）。
    assert_eq!(
        expand_node_request(&["session.list".to_owned()]),
        Err(AuthorizationError::UnknownName("session.list".to_owned()))
    );
    assert_eq!(
        expand_node_request(&["grant.observe".to_owned(), "permission.resolve".to_owned()]),
        Err(AuthorizationError::UnknownName(
            "permission.resolve".to_owned()
        ))
    );
}
