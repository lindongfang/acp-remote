//! [PV3] R53–R68 / 任务 2.7：授权词表展开与机器资产逐项一致。
//!
//! 唯一机器来源是 `compatibility/commands/v1/commands.json`：本文件既断言展开行为，
//! 也断言 `identity_auth::authorization` 里的三张表与它**逐项相等**（漂移即红）。

mod support;

use identity_auth::AuthorizationError;
use identity_auth::authorization::{
    GRANTS, LOCAL_CAPABILITIES, PACKS, PRESETS, command_names, expand_device_request,
    expand_grant_scopes, expand_node_request, expand_pack, expand_preset,
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

/// 目录里的一个对象（`packs`/`presets`/`grants`）转成 `(名称, 成员)` 列表。
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
fn packs_match_machine_catalog() {
    // R53 / 表与资产逐项相等。
    let catalog = catalog();
    let expected = table_of(&catalog, "packs");
    assert_eq!(PACKS.len(), expected.len(), "包的数量必须一致");
    for (name, members) in &expected {
        let actual = expand_pack(name).unwrap_or_else(|error| panic!("{name} 必须可展开：{error}"));
        assert_eq!(&actual, members, "{name} 的成员必须与目录一致");
    }
}

#[test]
fn presets_match_machine_catalog() {
    // R53 / 预设同样逐项相等（成员是包名）。
    let catalog = catalog();
    let expected = table_of(&catalog, "presets");
    assert_eq!(PRESETS.len(), expected.len());
    for (name, members) in &expected {
        let expansion = expand_preset(name).expect("预设必须可展开");
        assert_eq!(&expansion.packs, members, "{name} 列的包必须与目录一致");
    }
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
    // R54 / 展开结果只可能是登记命令：`packs ∪ grants` 必须覆盖目录里的 12 条命令。
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
fn preset_expansion_is_recursive_and_deduplicated() {
    // R55 / 预设递归展开、集合去重。
    let expansion = expand_preset("preset.remote-control").expect("预设必须可展开");
    assert_eq!(expansion.packs.len(), 4);
    let expected = support::scopes(&[
        "session.list",
        "session.read",
        "command.status",
        "session.mode.list",
        "session.config.list",
        "session.prompt",
        "session.cancel",
        "elicitation.respond",
        "session.mode.set",
        "session.config.set",
        "permission.resolve",
    ]);
    assert_eq!(
        expansion.scopes, expected,
        "预设展开出的命令集合必须完整且去重"
    );
    // `read-only` 只含观察包，且不得包含任何 pack 名。
    let read_only = expand_preset("preset.read-only").expect("预设必须可展开");
    assert_eq!(read_only.packs, vec!["pack.observe".to_owned()]);
    assert!(
        read_only
            .scopes
            .iter()
            .all(|name| !name.starts_with("pack."))
    );
}

#[test]
fn pack_names_never_leak_into_scopes() {
    // R56 / 包名不进 wire、不进记录：展开结果只含命令名。
    for (pack, _) in PACKS {
        for command in expand_pack(pack).expect("包必须可展开") {
            assert!(!command.starts_with("pack."), "{command} 不是命令名");
            assert!(!command.starts_with("preset."));
            assert!(command_names().contains(command.as_str()));
        }
    }
}

#[test]
fn unknown_names_are_rejected_without_partial_expansion() {
    // R57 / 未知或拼错的名称必须显式拒绝，不做部分展开。
    assert_eq!(
        expand_pack("pack.observ"),
        Err(AuthorizationError::UnknownName("pack.observ".to_owned()))
    );
    assert_eq!(
        expand_preset("preset.readonly"),
        Err(AuthorizationError::UnknownName(
            "preset.readonly".to_owned()
        ))
    );
    assert_eq!(
        expand_grant_scopes("grant.remote_work"),
        Err(AuthorizationError::UnknownName(
            "grant.remote_work".to_owned()
        ))
    );
    // 混合输入：只要有一个非法名称，整体失败（不返回已展开的部分）。
    let error = expand_device_request(&["pack.observe".to_owned(), "pack.nope".to_owned()], &[])
        .expect_err("必须整体失败");
    assert_eq!(
        error,
        AuthorizationError::UnknownName("pack.nope".to_owned())
    );
    // 未知 scope（形状合法但不是登记命令）同样拒绝。
    assert_eq!(
        expand_device_request(&[], &["session.magic".to_owned()]),
        Err(AuthorizationError::UnknownName("session.magic".to_owned()))
    );
}

#[test]
fn local_capabilities_are_never_granted_remotely() {
    // R58 / `local.*` 7 项在任何输入位置都被拒绝，也不参与展开。
    for capability in LOCAL_CAPABILITIES {
        assert_eq!(
            expand_pack(capability),
            Err(AuthorizationError::LocalCapability(
                (*capability).to_owned()
            ))
        );
        assert_eq!(
            expand_grant_scopes(capability),
            Err(AuthorizationError::LocalCapability(
                (*capability).to_owned()
            ))
        );
        assert_eq!(
            expand_device_request(&[], &[(*capability).to_owned()]),
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
        expand_device_request(&[], &["local.unknown".to_owned()]),
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
fn device_request_expands_packs_and_explicit_scopes() {
    // R59–R61 / 设备请求 = 包展开 ∪ 显式 scope；`session.create` 只能经 `grant.remote-work`。
    let scopes = expand_device_request(
        &["pack.observe".to_owned()],
        &["permission.resolve".to_owned()],
    )
    .expect("请求必须可展开");
    assert_eq!(scopes.len(), 6, "观察包 5 项 + 显式 1 项");
    assert!(scopes.contains("session.list"));
    assert!(scopes.contains("permission.resolve"));
    assert!(!scopes.contains("session.prompt"), "未请求的包不得出现");
    // 显式请求 `session.create` 也是合法命令名（是否授权由 Export Policy 决定，不在这里静默放行/拒绝）。
    let with_create =
        expand_device_request(&[], &["session.create".to_owned()]).expect("登记命令名必须可展开");
    assert!(with_create.contains("session.create"));
}

#[test]
fn node_request_expands_grants_to_command_scopes() {
    // R62–R64 / 节点请求 = grant 名集合 + 其展开出的命令级 scope。
    let request =
        expand_node_request(&["grant.observe".to_owned(), "grant.remote-work".to_owned()])
            .expect("节点请求必须可展开");
    assert_eq!(request.grants.len(), 2);
    assert!(request.grants.contains("grant.remote-work"));
    assert!(request.scopes.contains("session.create"));
    assert_eq!(request.scopes.len(), 6);
    // grant 名本身不是命令名，绝不进入 scope 集合。
    assert!(!request.scopes.contains("grant.observe"));
}

#[test]
fn empty_request_yields_empty_sets() {
    // R65 / 空请求是合法的空集合，而不是错误。
    let scopes = expand_device_request(&[], &[]).expect("空请求必须合法");
    assert!(scopes.is_empty());
    let request = expand_node_request(&[]).expect("空请求必须合法");
    assert!(request.grants.is_empty());
    assert!(request.scopes.is_empty());
}

#[test]
fn expansion_is_stable_and_order_independent() {
    // R66 / 集合语义：同一集合不同输入顺序得到同一结果。
    let first = expand_device_request(
        &["pack.interact".to_owned(), "pack.observe".to_owned()],
        &[],
    )
    .expect("必须可展开");
    let second = expand_device_request(
        &["pack.observe".to_owned(), "pack.interact".to_owned()],
        &[],
    )
    .expect("必须可展开");
    assert_eq!(first, second);
}

#[test]
fn pack_members_are_registered_commands_with_matching_grant() {
    // R67 / 包成员与目录里的 `pack`/`grant` 归属一致（同一命令不得映射到不同包）。
    let catalog = catalog();
    for command in catalog["commands"].as_array().expect("必须是数组") {
        let name = command["name"].as_str().expect("命令必须有名字");
        //  没有设备包（只用节点授权），因此  允许为 null。
        let grant = command["grant"].as_str().expect("命令必须有授权");
        if let Some(pack) = command["pack"].as_str() {
            assert!(
                expand_pack(pack)
                    .expect("包必须可展开")
                    .contains(&name.to_owned()),
                "{name} 必须能由其包 {pack} 展开出来"
            );
        }
        assert!(
            expand_grant_scopes(grant)
                .expect("授权必须可展开")
                .contains(&name.to_owned()),
            "{name} 必须能由其授权 {grant} 展开出来"
        );
    }
}

#[test]
fn scope_and_grant_kinds_are_not_interchangeable() {
    // R68 / 设备请求不得带 grant 名、节点请求不得带 scope 名（出现即未知名称拒绝）。
    assert_eq!(
        expand_device_request(&[], &["grant.observe".to_owned()]),
        Err(AuthorizationError::UnknownName("grant.observe".to_owned()))
    );
    assert_eq!(
        expand_node_request(&["pack.observe".to_owned()]),
        Err(AuthorizationError::UnknownName("pack.observe".to_owned()))
    );
}
