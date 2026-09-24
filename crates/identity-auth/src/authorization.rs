//! 授权词表展开（合同 §6.1）：`pack.*` / `preset.*` / `grant.*` → 命令级 scope。
//!
//! 唯一机器来源是 [`compatibility/commands/v1/commands.json`](../../../compatibility/commands/v1/commands.json)
//! （路径相对仓库根）；本文件的三张表是它的 Rust 镜像，由 `tests/authorization.rs` 逐项断言（漂移即失败）。
//! 展开结果只允许是**独立命令名**：`pack.*`/`preset.*` 不进 wire、不进记录；`local.*` 7 项永不远程授予。

use std::collections::BTreeSet;

use acp_core::model::{GrantSet, ScopeSet};

use crate::error::AuthorizationError;

/// 设备授权包（`pack.*` → 命令名集合）。
pub const PACKS: &[(&str, &[&str])] = &[
    (
        "pack.observe",
        &[
            "session.list",
            "session.read",
            "command.status",
            "session.mode.list",
            "session.config.list",
        ],
    ),
    (
        "pack.interact",
        &["session.prompt", "session.cancel", "elicitation.respond"],
    ),
    (
        "pack.configure-session",
        &["session.mode.set", "session.config.set"],
    ),
    ("pack.approve", &["permission.resolve"]),
];

/// 配对预设（`preset.*` → `pack.*` 集合；递归展开到命令名）。
pub const PRESETS: &[(&str, &[&str])] = &[
    (
        "preset.remote-control",
        &[
            "pack.observe",
            "pack.interact",
            "pack.configure-session",
            "pack.approve",
        ],
    ),
    ("preset.read-only", &["pack.observe"]),
];

/// 跨节点导出授权（`grant.*` → 命令名集合；Node Link Export 的 `scopes` 用同一张表）。
pub const GRANTS: &[(&str, &[&str])] = &[
    (
        "grant.observe",
        &[
            "session.list",
            "session.read",
            "command.status",
            "session.mode.list",
            "session.config.list",
        ],
    ),
    (
        "grant.interact",
        &["session.prompt", "session.cancel", "elicitation.respond"],
    ),
    (
        "grant.configure-session",
        &["session.mode.set", "session.config.set"],
    ),
    ("grant.approve", &["permission.resolve"]),
    ("grant.remote-work", &["session.create"]),
];

/// 7 项本地管理能力：永不远程授予，也不参与任何展开（`SECURITY_DESIGN.md` §10.3）。
pub const LOCAL_CAPABILITIES: &[&str] = &[
    "local.workspace.select",
    "local.agent.configure",
    "local.provider.configure",
    "local.device.manage",
    "local.export.manage",
    "local.node.rotate-key",
    "local.audit.export",
];

/// 设备 `device.pair.begin` 的请求（`requestedPacks` + `requestedScopes`）。
///
/// 展开结果是独立 scope 集合：`pack.*` 只在这里出现，wire 与记录里只有命令名。
/// 未知/拼错名称、以及任何 `local.*` 一律显式拒绝（不做部分展开）。
pub fn expand_device_request(
    requested_packs: &[String],
    requested_scopes: &[String],
) -> Result<ScopeSet, AuthorizationError> {
    let mut scopes = ScopeSet::empty();
    for pack in requested_packs {
        reject_local(pack)?;
        for command in expand_pack(pack)? {
            scopes.insert(&command)?;
        }
    }
    for scope in requested_scopes {
        reject_local(scope)?;
        if !command_names().contains(scope.as_str()) {
            return Err(AuthorizationError::UnknownName(scope.clone()));
        }
        scopes.insert(scope)?;
    }
    Ok(scopes)
}

/// 节点 `node.pair.begin` 的请求（`requestedGrants`）。
///
/// 返回两件东西：记录用的 grant 名集合，以及它展开出的命令级 scope（Export 侧使用）。
pub fn expand_node_request(
    requested_grants: &[String],
) -> Result<GrantRequest, AuthorizationError> {
    let mut grants = GrantSet::empty();
    let mut scopes = ScopeSet::empty();
    for grant in requested_grants {
        reject_local(grant)?;
        for command in expand_grant_scopes(grant)? {
            scopes.insert(&command)?;
        }
        grants.insert(grant)?;
    }
    Ok(GrantRequest { grants, scopes })
}

/// 一个授权包的展开结果。
pub fn expand_pack(name: &str) -> Result<Vec<String>, AuthorizationError> {
    reject_local(name)?;
    let members =
        lookup(PACKS, name).ok_or_else(|| AuthorizationError::UnknownName(name.to_owned()))?;
    Ok(members.iter().map(|member| (*member).to_owned()).collect())
}

/// 一个预设的展开结果：它列出的包与这些包展开出的命令级 scope。
pub fn expand_preset(name: &str) -> Result<PresetExpansion, AuthorizationError> {
    reject_local(name)?;
    let packs = lookup(PRESETS, name)
        .ok_or_else(|| AuthorizationError::UnknownName(name.to_owned()))?
        .iter()
        .map(|pack| (*pack).to_owned())
        .collect::<Vec<_>>();
    let mut scopes = ScopeSet::empty();
    for pack in &packs {
        for command in expand_pack(pack)? {
            scopes.insert(&command)?;
        }
    }
    Ok(PresetExpansion { packs, scopes })
}

/// 一个导出授权的展开结果（命令级 scope）。
pub fn expand_grant_scopes(name: &str) -> Result<Vec<String>, AuthorizationError> {
    reject_local(name)?;
    let members =
        lookup(GRANTS, name).ok_or_else(|| AuthorizationError::UnknownName(name.to_owned()))?;
    Ok(members.iter().map(|member| (*member).to_owned()).collect())
}

/// 全部命令名（`packs ∪ grants` 的成员集合；与 `commands.json` 的 12 条命令逐项相等，由测试断言）。
pub fn command_names() -> BTreeSet<&'static str> {
    let mut names = BTreeSet::new();
    for (_, members) in PACKS.iter().chain(GRANTS.iter()) {
        for member in *members {
            names.insert(*member);
        }
    }
    names
}

/// 展开后的两件产物：记录用的 grant 名 + 命令级 scope。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRequest {
    /// 被请求的 `grant.*` 名称集合。
    pub grants: GrantSet,
    /// 展开出的命令级 scope 集合。
    pub scopes: ScopeSet,
}

/// 预设的展开结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetExpansion {
    /// 该预设列出的 `pack.*`。
    pub packs: Vec<String>,
    /// 递归展开出的命令级 scope。
    pub scopes: ScopeSet,
}

fn lookup<'a>(table: &'a [(&'a str, &'a [&'a str])], name: &str) -> Option<&'a [&'a str]> {
    table
        .iter()
        .find(|(entry, _)| *entry == name)
        .map(|(_, members)| *members)
}

/// `local.*` 永不远程授予、也不参与展开：在任何输入位置出现都直接拒绝。
fn reject_local(name: &str) -> Result<(), AuthorizationError> {
    if LOCAL_CAPABILITIES.contains(&name) || name.starts_with("local.") {
        return Err(AuthorizationError::LocalCapability(name.to_owned()));
    }
    Ok(())
}
